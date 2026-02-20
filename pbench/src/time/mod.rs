//! Timing infrastructure.
//!
//! Provides [`FineDuration`] for picosecond-precision durations, [`InstantTimer`]
//! for OS-clock timing, and [`TscTimer`] for x86 TSC-based timing.

pub(crate) mod fence;
pub mod fine_duration;
pub(crate) mod instant;

#[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
pub(crate) mod tsc;

use std::hint as StdHint;
use std::sync::OnceLock;
use std::time::Instant;

pub use fine_duration::FineDuration;
pub use instant::InstantTimer;

#[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
pub use tsc::{TscTimer, TscUnavailable};

/// Active timer backend used for benchmarking.
///
/// Wraps either [`InstantTimer`] (always available) or [`TscTimer`]
/// (`x86_64`/x86 only, sub-nanosecond resolution).
#[derive(Clone, Copy, Debug)]
pub enum Timer {
    /// Operating system monotonic clock.
    Os(InstantTimer),

    /// CPU timestamp counter (`x86_64`/x86 only).
    #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
    Tsc(TscTimer),
}

/// Opaque timestamp from a [`Timer`].
#[derive(Clone, Copy, Debug)]
pub enum Timestamp {
    /// OS-clock timestamp.
    Os(Instant),

    /// TSC tick count.
    #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
    Tsc(u64),
}

/// Measured overhead of the benchmarking harness.
///
/// Used by the sampling loop to subtract per-iteration measurement
/// overhead from raw timing samples.
#[derive(Clone, Copy, Debug)]
pub struct TimedOverhead {
    /// Per-iteration overhead of the sample measurement loop.
    pub sample_loop: FineDuration,
}

impl TimedOverhead {
    /// Zero overhead (used under Miri where calibration is too slow).
    pub const ZERO: Self = Self {
        sample_loop: FineDuration::ZERO,
    };
}

/// Number of distinct timer backends (Os, Tsc).
const TIMER_KIND_COUNT: usize = 2;

impl Timer {
    /// Select the best available timer.
    ///
    /// Prefers TSC on `x86_64`/x86 if calibration succeeds, otherwise
    /// falls back to [`InstantTimer`].
    #[must_use]
    pub fn best_available() -> Self {
        #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
        {
            if let Ok(tsc_timer) = TscTimer::calibrate() {
                return Self::Tsc(tsc_timer);
            }
        }

        Self::Os(InstantTimer)
    }

    /// Take a start timestamp.
    #[must_use]
    #[inline(always)]
    pub fn now(&self) -> Timestamp {
        match self {
            Self::Os(t) => Timestamp::Os(t.now()),

            #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
            Self::Tsc(t) => Timestamp::Tsc(t.now()),
        }
    }

    /// Take an end timestamp.
    ///
    /// For TSC this uses `RDTSCP` (serializing variant). For the OS clock
    /// this is identical to [`now`](Self::now).
    #[must_use]
    #[inline(always)]
    pub fn now_end(&self) -> Timestamp {
        match self {
            Self::Os(t) => Timestamp::Os(t.now()),

            #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
            Self::Tsc(t) => Timestamp::Tsc(t.now_end()),
        }
    }

    /// Compute elapsed time between a start and end timestamp.
    ///
    /// # Panics
    ///
    /// Panics if `start` and `end` are from different timer backends.
    #[inline]
    #[must_use]
    pub fn elapsed(&self, start: Timestamp, end: Timestamp) -> FineDuration {
        match (self, start, end) {
            (Self::Os(t), Timestamp::Os(s), Timestamp::Os(e)) => t.elapsed(s, e),

            #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
            (Self::Tsc(t), Timestamp::Tsc(s), Timestamp::Tsc(e)) => t.elapsed(s, e),

            _ => panic!("mismatched timer and timestamp variants"),
        }
    }

    // -----------------------------------------------------------------------
    //  Precision (cached)
    // -----------------------------------------------------------------------

    /// Returns the smallest non-zero duration this timer can measure.
    ///
    /// The result is cached per timer kind via [`OnceLock`] — the first call
    /// measures precision, subsequent calls return the cached value.
    #[must_use]
    pub fn precision(&self) -> FineDuration {
        static CACHED: [OnceLock<FineDuration>; TIMER_KIND_COUNT] =
            [OnceLock::new(), OnceLock::new()];

        let cached: &OnceLock<FineDuration> = &CACHED[self.kind_index()];

        *cached.get_or_init(|| self.measure_precision())
    }

    /// Measure the smallest non-zero duration this timer can resolve (raw,
    /// uncached). Use [`precision`](Self::precision) for the cached version.
    fn measure_precision(&self) -> FineDuration {
        match self {
            Self::Os(t) => t.measure_precision(),

            #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
            Self::Tsc(t) => t.measure_precision(),
        }
    }

    // -----------------------------------------------------------------------
    //  Overhead calibration (cached)
    // -----------------------------------------------------------------------

    /// Returns the measured overhead of the benchmarking harness.
    ///
    /// The result is cached per timer kind via [`OnceLock`]. Under Miri,
    /// returns [`TimedOverhead::ZERO`] to avoid slow calibration.
    #[must_use]
    pub fn bench_overhead(&self) -> TimedOverhead {
        static CACHED: [OnceLock<TimedOverhead>; TIMER_KIND_COUNT] =
            [OnceLock::new(), OnceLock::new()];

        if cfg!(miri) {
            return TimedOverhead::ZERO;
        }

        let cached: &OnceLock<TimedOverhead> = &CACHED[self.kind_index()];

        *cached.get_or_init(|| TimedOverhead {
            sample_loop: self.measure_sample_loop_overhead(),
        })
    }

    /// Measure per-iteration overhead of the sample measurement loop.
    ///
    /// Runs 100 samples of 10,000 `black_box` iterations each, keeping the
    /// minimum per-iteration time. Matches divan's `measure_sample_loop_overhead`.
    fn measure_sample_loop_overhead(&self) -> FineDuration {
        let sample_count: usize = 100;
        let sample_size: usize = 10_000;

        let mut min_sample: FineDuration = FineDuration::MAX;

        for _ in 0..sample_count {
            let start: Timestamp = self.now();

            for i in 0..sample_size {
                let _: usize = StdHint::black_box(i);
            }

            let end: Timestamp = self.now_end();

            let total: FineDuration = self.elapsed(start, end);

            let per_iter: FineDuration = total.div_u64(sample_size as u64);

            if !per_iter.is_zero() && per_iter < min_sample {
                min_sample = per_iter;
            }
        }

        // If all samples were zero (extremely fast timer), return ZERO.
        if min_sample == FineDuration::MAX {
            return FineDuration::ZERO;
        }

        min_sample
    }

    // -----------------------------------------------------------------------
    //  Private helpers
    // -----------------------------------------------------------------------

    /// Returns 0 for Os, 1 for Tsc. Used as an index into `OnceLock` arrays.
    #[inline]
    const fn kind_index(&self) -> usize {
        match self {
            Self::Os(_) => 0,

            #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
            Self::Tsc(_) => 1,
        }
    }
}

#[cfg(test)]
mod unit_tests;
