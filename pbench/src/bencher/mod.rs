//! Core benchmark measurement engine.
//!
//! [`BenchContext`] holds shared state (timer, options, overhead, results).
//! [`Bencher`] is the thin public API that runs closures through the
//! two-phase sampling loop (adaptive tuning + collection).
//! [`BencherWithInput`] extends `Bencher` with per-iteration input generation.

use std::cell::Cell;
use std::hint as StdHint;
use std::marker::PhantomData;
use std::time::Instant;

use crate::{
    config::ResolvedBenchOptions,
    counter::{Counter, CounterKind, collection::CounterCollection},
    stats::{
        PercentileStats,
        sample::{RawSample, SampleCollection},
    },
    time::{FineDuration, TimedOverhead, Timer},
};

/// Precision multiplier for adaptive tuning.
///
/// A sample must last at least this many times the timer precision
/// before the tuning phase locks in the sample size.
const PRECISION_MULTIPLIER: u128 = 100;

// =========================================================================
//  BenchResult - combined benchmark output
// =========================================================================

/// Combined results of a benchmark run.
///
/// Bundles percentile statistics with optional counter data for throughput
/// display. Returned by [`BenchContext::finish`].
pub(crate) struct BenchResult {
    /// Percetile statistics computed from timing samples.
    pub stats: PercentileStats,

    /// Counter collection for throughput display, present only if
    /// [`Bencher::counter`] was called before running.
    pub counter: Option<CounterCollection>,
}

// =========================================================================
//  BenchContext - internal shared state
// =========================================================================

/// Internal shared state for a single benchmark execution.
///
/// Created once per benchmark entry by the runer.
pub(crate) struct BenchContext {
    /// Selected timer backend.
    timer: Timer,

    /// Cached timer precision (smallest non-zero measurable duration).
    precision: FineDuration,

    /// Calibrated per-iteration overhead of the measurement harness.
    overhead: TimedOverhead,

    /// Fully resolved benchmark options.
    options: ResolvedBenchOptions,

    /// Collected results, set once after the sampling loop compltes.
    stats: Cell<Option<PercentileStats>>,

    /// Counter info set by [`Bencher::counter`], captured as (count, kind).
    ///
    /// Follows last-write-wins: repeated calls overwrite the previous value.
    counter_info: Cell<Option<(u64, CounterKind)>>,

    /// Collected counter results, set alongside stats after sampling.
    counter_result: Cell<Option<CounterCollection>>,

    /// Guard enforing single-use semantics.
    ///
    /// Set to `true` after the first sampling run completes. Any
    /// subsequent attempt to run via [`SamplingLoop::run_timed`] panics.
    has_run: Cell<bool>,
}

impl BenchContext {
    /// Create a new context, selecting the best timer and calibration.
    #[must_use]
    pub(crate) fn new(options: ResolvedBenchOptions) -> Self {
        let timer: Timer = Timer::best_available();
        let precision: FineDuration = timer.precision();
        let overhead: TimedOverhead = timer.bench_overhead();

        Self {
            timer,
            precision,
            overhead,
            options,
            stats: Cell::new(None),
            counter_info: Cell::new(None),
            counter_result: Cell::new(None),
            has_run: Cell::new(false),
        }
    }

    /// Consume the context and return collected stats (if any).
    #[must_use]
    pub(crate) fn finish(self) -> Option<BenchResult> {
        let stats: Option<PercentileStats> = self.stats.into_inner();
        let counter: Option<CounterCollection> = self.counter_result.into_inner();

        stats.map(|s: PercentileStats| BenchResult { stats: s, counter })
    }
}

// =========================================================================
//  Bencher
// =========================================================================

/// Public benchmark handle passed to user-defined benchmark functions.
///
/// Provides methods to run closures through the two-phase sampling loop.
/// Created by the runner and passed to each benchmark entry's function.
pub struct Bencher<'ctx> {
    /// Reference to the shared benchmark context.
    context: &'ctx BenchContext,
}

impl<'ctx> Bencher<'ctx> {
    /// Create a new [`Bencher`] wrapping a context.
    #[inline]
    #[must_use]
    pub(crate) const fn new(context: &'ctx BenchContext) -> Self {
        Self { context }
    }

    /// Attach a throughput counter to this benchmark.
    ///
    /// The counter value represents units processed per iteration
    /// (bytes, items, characters, or CPU cycles). The benchmarking
    /// harness records this alongside timing data so output formatters
    /// can display throughput ("1.234 GB/s").
    ///
    /// Current Design Limitations: The counter value is assumed constant across
    /// all iterations and samples. For variable-input benchmarks (like
    /// `with_inputs` generating differentsized data), the counter will
    /// not reflect per-sample variation. I may work on a future `counter_fn`
    /// API that will support dynamic per-iteration counts.
    ///
    /// Overwrite semantics: Calling this method multiple times
    /// before running the benchmark replaces the previous counter
    /// (last-write-wins). Only the final counter is used.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// bencher.counter(BytesCount::of_slice(&data)).bench_refs(|| {
    ///     process(&data);
    /// });
    /// ``
    #[expect(clippy::needless_pass_by_value)]
    pub fn counter(&self, c: impl Counter) -> &Self {
        self.context.counter_info.set(Some((c.count(), c.kind())));

        self
    }

    /// Benchmark a closure that returns a value by reference semantics.
    ///
    /// The return value is passed through [`StdHint::black_box`] to prevent the
    /// compiler from optimizing it away. Each sample runs `f` for
    /// `sample_size` iterations.
    pub fn bench_refs<R>(&self, mut f: impl FnMut() -> R) {
        let ctx: &BenchContext = self.context;

        match ctx.timer {
            Timer::Os(timer) => {
                let mut timed_body = |sample_size: u32| -> FineDuration {
                    let start: Instant = timer.now();

                    for _ in 0..sample_size {
                        let r: R = f();

                        StdHint::black_box(&r);
                    }

                    let end: Instant = timer.now();
                    timer.elapsed(start, end)
                };

                SamplingLoop::run_timed(ctx, &mut timed_body);
            }

            #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
            Timer::Tsc(timer) => {
                let mut timed_body = |sample_size: u32| -> FineDuration {
                    let start: u64 = timer.now();

                    for _ in 0..sample_size {
                        let r: R = f();

                        StdHint::black_box(&r);
                    }

                    let end: u64 = timer.now_end();
                    timer.elapsed(start, end)
                };

                SamplingLoop::run_timed(ctx, &mut timed_body);
            }
        }
    }

    /// Benchmark a closure that produces owned values.
    ///
    /// Return values are collected in a `Vec` during each sample and
    /// dropped *after* timing completes, keeping drop cost outside the
    /// measurement window.
    pub fn bench_values<R>(&self, mut f: impl FnMut() -> R) {
        let ctx: &BenchContext = self.context;

        match ctx.timer {
            Timer::Os(timer) => {
                let mut timed_body = |sample_size: u32| -> FineDuration {
                    let mut deferred: Vec<R> = Vec::with_capacity(sample_size as usize);

                    let start: Instant = timer.now();

                    for _ in 0..sample_size {
                        deferred.push(f());
                    }

                    StdHint::black_box(&deferred);

                    let end: Instant = timer.now();
                    let elapsed: FineDuration = timer.elapsed(start, end);
                    drop(deferred);

                    elapsed
                };

                SamplingLoop::run_timed(ctx, &mut timed_body);
            }

            #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
            Timer::Tsc(timer) => {
                let mut timed_body = |sample_size: u32| -> FineDuration {
                    let mut deferred: Vec<R> = Vec::with_capacity(sample_size as usize);

                    let start: u64 = timer.now();

                    for _ in 0..sample_size {
                        deferred.push(f());
                    }

                    StdHint::black_box(&deferred);

                    let end: u64 = timer.now_end();
                    let elapsed: FineDuration = timer.elapsed(start, end);
                    drop(deferred);

                    elapsed
                };

                SamplingLoop::run_timed(ctx, &mut timed_body);
            }
        }
    }

    /// Return a builder that generates fresh input for each iteration.
    ///
    /// The generator `gen` is called once per iteration *outside* the
    /// timing window. The input is then passed to the benchmark closure
    /// inside the timing window.
    ///
    /// `gen` is `FnMut` so stateful generators (RNG, counters) are
    /// supported.
    #[must_use]
    pub const fn with_inputs<I, G: FnMut() -> I>(
        self,
        generator: G,
    ) -> BencherWithInput<'ctx, I, G> {
        BencherWithInput {
            context: self.context,
            generator,
            _marker: PhantomData,
        }
    }
}

// =========================================================================
//  BencherWithInput
// =========================================================================

/// Builder returned by [`Bencher::with_inputs`].
///
/// Generates fresh input per iteration via the stored generator closure.
/// Input generation happens *outside* the timing window.
pub struct BencherWithInput<'ctx, I, G: FnMut() -> I> {
    /// Reference to the shared benchmark context.
    context: &'ctx BenchContext,

    /// Input generator closure (mutable, supports stateful generators).
    generator: G,

    /// Marker for the input type.
    _marker: PhantomData<I>,
}

impl<I, G: FnMut() -> I> BencherWithInput<'_, I, G> {
    /// Benchmark a closure that consumes the generated input.
    ///
    /// For each sample: inputs are generated *outside* timing via `generator()`,
    /// then `f(input)` is called *inside* timing. Outputs are dropped
    /// after timing completes.
    pub fn bench_values<R>(mut self, mut f: impl FnMut(I) -> R) {
        let ctx: &BenchContext = self.context;
        let generator: &mut G = &mut self.generator;

        match ctx.timer {
            Timer::Os(timer) => {
                let mut timed_body = |sample_size: u32| -> FineDuration {
                    // Generate inputs outside timing.
                    let mut inputs: Vec<I> = Vec::with_capacity(sample_size as usize);

                    for _ in 0..sample_size {
                        inputs.push(generator());
                    }

                    let mut deferred: Vec<R> = Vec::with_capacity(sample_size as usize);
                    let start: Instant = timer.now();

                    for input in inputs {
                        deferred.push(f(input));
                    }

                    StdHint::black_box(&deferred);

                    let end: Instant = timer.now();
                    let elapsed: FineDuration = timer.elapsed(start, end);
                    drop(deferred);

                    elapsed
                };

                SamplingLoop::run_timed(ctx, &mut timed_body);
            }

            #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
            Timer::Tsc(timer) => {
                let mut timed_body = |sample_size: u32| -> FineDuration {
                    let mut inputs: Vec<I> = Vec::with_capacity(sample_size as usize);

                    for _ in 0..sample_size {
                        inputs.push(generator());
                    }

                    let mut deferred: Vec<R> = Vec::with_capacity(sample_size as usize);
                    let start: u64 = timer.now();

                    for input in inputs {
                        deferred.push(f(input));
                    }

                    StdHint::black_box(&deferred);

                    let end: u64 = timer.now_end();
                    let elapsed: FineDuration = timer.elapsed(start, end);
                    drop(deferred);

                    elapsed
                };

                SamplingLoop::run_timed(ctx, &mut timed_body);
            }
        }
    }

    /// Benchmark a closure that borrows the generated input.
    ///
    /// For each sample: inputs are generated *outside* timing via `gen()`,
    /// then `f(&input)` is called *inside* timing. Inputs and outputs
    /// are dropped after timing.
    pub fn bench_refs<R>(mut self, mut f: impl FnMut(&I) -> R) {
        let ctx: &BenchContext = self.context;
        let generator: &mut G = &mut self.generator;

        match ctx.timer {
            Timer::Os(timer) => {
                let mut timed_body = |sample_size: u32| -> FineDuration {
                    // Generate inputs outside timing.
                    let mut inputs: Vec<I> = Vec::with_capacity(sample_size as usize);

                    for _ in 0..sample_size {
                        inputs.push(generator());
                    }

                    // Time only the benchmark work.
                    let start: Instant = timer.now();

                    for input in &inputs {
                        let r: R = f(input);
                        StdHint::black_box(&r);
                    }

                    let end: Instant = timer.now();
                    let elapsed: FineDuration = timer.elapsed(start, end);
                    drop(inputs);

                    elapsed
                };

                SamplingLoop::run_timed(ctx, &mut timed_body);
            }

            #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
            Timer::Tsc(timer) => {
                let mut timed_body = |sample_size: u32| -> FineDuration {
                    let mut inputs: Vec<I> = Vec::with_capacity(sample_size as usize);

                    for _ in 0..sample_size {
                        inputs.push(generator());
                    }

                    let start: u64 = timer.now();

                    for input in &inputs {
                        let r: R = f(input);

                        StdHint::black_box(&r);
                    }

                    let end: u64 = timer.now_end();
                    let elapsed: FineDuration = timer.elapsed(start, end);
                    drop(inputs);

                    elapsed
                };

                SamplingLoop::run_timed(ctx, &mut timed_body);
            }
        }
    }
}

// =========================================================================
//  Sampling loop - Core Measurement Algorithms
// =========================================================================

/// Unit struct containing core measurement algorithms.
pub struct SamplingLoop;

impl SamplingLoop {
    /// Run the two-phase sampling loop (tuning + collection).
    ///
    /// The `timed_body` closure receives a `sample_size` and returns the
    /// measured [`FineDuration`] for that sample. The closure is responsible
    /// for starting/stopping the timer internally, which allows input
    /// generation and deferred drops to happen outside the timing window.
    ///
    /// # Panics
    ///
    /// Panics if the context already been used for a sampling run.
    /// Each [`BenchContext`] supports exactly one run.
    fn run_timed(ctx: &BenchContext, timed_body: &mut impl FnMut(u32) -> FineDuration) {
        assert!(
            !ctx.has_run.get(),
            "BenchContext has already been used for a sampling run. Each context supports exactly one run"
        );

        // Phase 1: Adaptive tuning
        let sample_size: u32 = ctx.options.sample_size.map_or_else(
            || Self::tune_sample_size(ctx, timed_body),
            |fixed: u32| fixed.max(1),
        );

        // Phase 2: Collection
        Self::collect_samples(ctx, sample_size, timed_body);

        ctx.has_run.set(true);
    }

    /// Adaptive tuning: double `sample_size` until sample duration exceeds
    /// [`PRECISION_MULTIPLIER`] x timer precision.
    fn tune_sample_size(
        ctx: &BenchContext,
        timed_body: &mut impl FnMut(u32) -> FineDuration,
    ) -> u32 {
        let threshold: u128 = ctx.precision.picos.saturating_mul(PRECISION_MULTIPLIER);
        let max_tune_time: FineDuration = FineDuration::from(ctx.options.max_time).div_u64(2);
        let tune_start: Instant = Instant::now();
        let mut sample_size: u32 = 1;

        loop {
            let elapsed: FineDuration = timed_body(sample_size);

            if elapsed.picos >= threshold {
                return sample_size;
            }

            // Double, capping at u32::MAX to avoid overflow.
            sample_size = sample_size.saturating_mul(2).max(1);

            // cap sample_size
            if sample_size >= 1_000_000 {
                return sample_size;
            }

            // Stop if tuning has consumed too much wall-clock time.
            let wall_elapsed: FineDuration = FineDuration::from(tune_start.elapsed());

            if wall_elapsed >= max_tune_time {
                return sample_size;
            }
        }
    }

    /// Collection phase: collect samples with three-tier stop condition
    /// matching divan's loop semantics.
    ///
    /// Stop conditions (evaluated in order):
    /// 1. Hard stop: `elapsed >= max_time`
    /// 2. Continue: `samples_remaining > 0`
    /// 3. Floor: `elapsed < min_time`
    fn collect_samples(
        ctx: &BenchContext,
        sample_size: u32,
        timed_body: &mut impl FnMut(u32) -> FineDuration,
    ) {
        let sample_count: u32 = ctx.options.sample_count;
        let min_time: FineDuration = FineDuration::from(ctx.options.min_time);
        let max_time: FineDuration = FineDuration::from(ctx.options.max_time);
        let skip_ext_time: bool = ctx.options.skip_ext_time;
        let overhead_per_iter: FineDuration = ctx.overhead.sample_loop;

        let mut samples: SampleCollection = SampleCollection::with_capacity(sample_count as usize);
        let mut elapsed_total: FineDuration = FineDuration::ZERO;
        let mut samples_remaining: u32 = sample_count;

        loop {
            if elapsed_total >= max_time {
                break;
            }

            if (samples_remaining == 0) && (elapsed_total >= min_time) {
                break;
            }

            let wall_start: Instant = Instant::now();
            let raw_duration: FineDuration = timed_body(sample_size);
            let wall_elapsed: FineDuration = FineDuration::from(wall_start.elapsed());

            let per_iter: FineDuration = raw_duration
                .div_u64(u64::from(sample_size))
                .checked_sub(overhead_per_iter)
                .unwrap_or(FineDuration::ZERO);

            samples.push(RawSample {
                duration: per_iter,
                sample_size,
            });

            samples_remaining = samples_remaining.saturating_sub(1);

            if skip_ext_time {
                elapsed_total += raw_duration;
            } else {
                elapsed_total += wall_elapsed;
            }
        }

        // Build counter collection if a counter was attached.
        //
        // NOTE: Counter values are duplicated uniformly across all samples
        // because current design assumes constant work per iter. Detailed
        // explanation in [`Bencher::counter`] doc.
        //
        // TODO: Support dynamic per-sample counters via `counter_fn` for
        // variable input benchmarks.
        let counter_collection: Option<CounterCollection> =
            ctx.counter_info
                .get()
                .map(|(count, kind): (u64, CounterKind)| {
                    let mut collection: CounterCollection =
                        CounterCollection::with_capacity(kind, samples.len());

                    for _ in 0..samples.len() {
                        collection.push(count);
                    }

                    collection
                });

        Self::store_stats(ctx, &samples, sample_size, counter_collection);
    }

    /// Compute and store percentile statistics from collected samples.
    fn store_stats(
        ctx: &BenchContext,
        samples: &SampleCollection,
        sample_size: u32,
        counter: Option<CounterCollection>,
    ) {
        if samples.is_empty() {
            return;
        }

        let picos: Vec<u128> = samples.raw_picos();
        let stats: PercentileStats = PercentileStats::compute(&picos, sample_size);

        ctx.stats.set(Some(stats));
        ctx.counter_result.set(counter);
    }
}

// =========================================================================
//  Test helper
// =========================================================================

/// Run a benchmark closure with the given options and return stats.
///
/// Convenience helper for tests: creates a `BenchContext`, wraps it in a
/// `Bencher`, calls the provided setup function, and returns the stats.
///
/// Uses a short `max_time` (100 ms) to keep test execution fast.
#[cfg(test)]
pub(crate) fn run_bench_closure(
    sample_count: u32,
    sample_size: Option<u32>,
    f: impl FnOnce(&Bencher<'_>),
) -> PercentileStats {
    use std::time::Duration;

    let options: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&crate::config::BenchOptions {
            sample_count: Some(sample_count),
            sample_size,
            max_time: Some(Duration::from_millis(100)),
            ..Default::default()
        });

    let ctx: BenchContext = BenchContext::new(options);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    f(&bencher);

    ctx.finish()
        .expect("benchmark should have produced stats")
        .stats
}

#[cfg(test)]
mod unit_tests;
