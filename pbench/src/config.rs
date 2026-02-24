//! Benchmark configuration with two-layer resolution.
//!
//! [`BenchOptions`] is the user-facing partial configuration where every field
//! is `Option<T>` — `None` means "inherit from parent or use default."
//! [`ResolvedBenchOptions`] is the fully resolved form with concrete values,
//! produced by [`ResolvedBenchOptions::from_options`].

use std::num::NonZeroUsize;
use std::time::Duration;

/// Partial benchmark configuration with `Option<T>` fields.
///
/// Every field defaults to `None`, meaning "inherit from parent or use
/// the crate default." Use [`overwrite`](Self::overwrite) to layer a child
/// configuration on top of a parent, then [`ResolvedBenchOptions::from_options`]
/// to produce a fully resolved configuration.
///
/// # Examples
///
/// ```
/// use pbench::config::BenchOptions;
///
/// let opts: BenchOptions = BenchOptions {
///     sample_count: Some(500),
///     ..BenchOptions::default()
/// };
/// ```
#[derive(Clone, Debug, Default)]
pub struct BenchOptions {
    /// Number of sample recordings. `None` = inherit.
    pub sample_count: Option<u32>,

    /// Number of iterations per sample. `None` = adaptive tuning.
    pub sample_size: Option<u32>,

    /// Minimum benchmarking time. `None` = inherit.
    pub min_time: Option<Duration>,

    /// Maximum benchmarking time. `None` = inherit.
    pub max_time: Option<Duration>,

    /// Skip time external to benchmarked functions (input generation,
    /// drops) when accounting for `min_time`/`max_time`. `None` = inherit.
    pub skip_ext_time: Option<bool>,

    /// Whether the benchmark should be ignored. `None` = inherit.
    pub ignore: Option<bool>,

    /// Thread counts to run the benchmark with. `None` = inherit.
    ///
    /// Each benchmark is executed once per thread count in the list.
    /// A value of `0` is expanded to [`std::thread::available_parallelism`]
    /// at resolution time.
    pub threads: Option<Vec<u32>>,
}

impl BenchOptions {
    /// Layer `self` (child) on top of `other` (parent).
    ///
    /// For each field, `self`'s value takes precedence if set (`Some`),
    /// otherwise `other`'s value is used.
    #[must_use]
    pub fn overwrite(&self, other: &Self) -> Self {
        Self {
            sample_count: self.sample_count.or(other.sample_count),
            sample_size: self.sample_size.or(other.sample_size),
            min_time: self.min_time.or(other.min_time),
            max_time: self.max_time.or(other.max_time),
            skip_ext_time: self.skip_ext_time.or(other.skip_ext_time),
            ignore: self.ignore.or(other.ignore),
            threads: self.threads.clone().or_else(|| other.threads.clone()),
        }
    }
}

/// Default sample count (1000 samples for meaningful tail percentiles).
const DEFAULT_SAMPLE_COUNT: u32 = 1_000;

/// Default minimum benchmarking time (1 ms).
const DEFAULT_MIN_TIME: Duration = Duration::from_millis(1);

/// Default maximum benchmarking time (5 s).
const DEFAULT_MAX_TIME: Duration = Duration::from_secs(5);

/// Fully resolved benchmark configuration with concrete values.
///
/// Produced by [`ResolvedBenchOptions::from_options`]. Every field has a
/// definite value — no `None` except `sample_size` which legitimately
/// uses `None` to indicate adaptive tuning.
#[derive(Clone, Debug)]
pub(crate) struct ResolvedBenchOptions {
    /// Number of sample recordings.
    pub sample_count: u32,

    /// Number of iterations per sample. `None` = adaptive tuning
    /// (the sampling loop will auto-select based on timer precision).
    pub sample_size: Option<u32>,

    /// Minimum benchmarking time.
    pub min_time: Duration,

    /// Maximum benchmarking time.
    pub max_time: Duration,

    /// Skip external time when accounting for min/max time.
    pub skip_ext_time: bool,

    /// Whether the benchmark is ignored.
    pub ignore: bool,

    /// Thread counts to run the benchmark with.
    ///
    /// Always non-empty. Sorted, deduplicated, with `0` expanded to the
    /// available parallelism on the current machine.
    pub threads: Vec<u32>,
}

impl ResolvedBenchOptions {
    /// Resolve a [`BenchOptions`] into concrete values, applying defaults
    /// for any unset fields.
    ///
    /// `sample_size` remains `None` if unset, signalling adaptive tuning
    /// to the sampling loop. `threads` defaults to `[1]` if unset.
    /// Any `0` entries are expanded to [`std::thread::available_parallelism`].
    #[must_use]
    pub fn from_options(options: &BenchOptions) -> Self {
        let threads: Vec<u32> = Self::resolve_threads(options.threads.as_deref());

        Self {
            sample_count: options.sample_count.unwrap_or(DEFAULT_SAMPLE_COUNT),
            sample_size: options.sample_size,
            min_time: options.min_time.unwrap_or(DEFAULT_MIN_TIME),
            max_time: options.max_time.unwrap_or(DEFAULT_MAX_TIME),
            skip_ext_time: options.skip_ext_time.unwrap_or(false),
            ignore: options.ignore.unwrap_or(false),
            threads,
        }
    }

    /// Resolve a thread count slice into a sorted, deduplicated, non-empty
    /// list. Expands `0` to available parallelism.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Thread count capped at u32::MAX; no machine has more than 4 billion cores"
    )]
    fn resolve_threads(threads: Option<&[u32]>) -> Vec<u32> {
        let available: u32 =
            std::thread::available_parallelism().map_or(1, |n: NonZeroUsize| n.get() as u32);

        let mut result: Vec<u32> = threads
            .unwrap_or(&[1])
            .iter()
            .map(|&t: &u32| if t == 0 { available } else { t })
            .collect();

        result.sort_unstable();
        result.dedup();

        // Safety net: should never be empty, but guard anyway.
        if result.is_empty() {
            result.push(1);
        }

        result
    }
}

#[cfg(test)]
mod unit_tests;
