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
    time::{FineDuration, TimedOverhead, Timer, TimerOps},
};

use super::dispatch_timer;

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
    /// Percentile statistics computed from timing samples.
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
/// Created once per benchmark entry by the runner.
pub(crate) struct BenchContext {
    /// Selected timer backend.
    timer: Timer,

    /// Cached timer precision (smallest non-zero measurable duration).
    precision: FineDuration,

    /// Calibrated per-iteration overhead of the measurement harness.
    overhead: TimedOverhead,

    /// Fully resolved benchmark options.
    options: ResolvedBenchOptions,

    /// Collected results, set once after the sampling loop completes.
    stats: Cell<Option<PercentileStats>>,

    /// Counter info set by [`Bencher::counter`], captured as (count, kind).
    ///
    /// Follows last-write-wins: repeated calls overwrite the previous value.
    counter_info: Cell<Option<(u64, CounterKind)>>,

    /// Collected counter results, set alongside stats after sampling.
    counter_result: Cell<Option<CounterCollection>>,

    /// Guard enforcing single-use semantics.
    ///
    /// Set to `true` after the first sampling run completes. Any
    /// subsequent attempt to run via [`SamplingLoop::run_timed`] panics.
    has_run: Cell<bool>,

    /// Number of threads to run the benchmark with.
    ///
    /// `1` = single-threaded (zero-overhead, existing path).
    /// `> 1` = multi-threaded via `BENCH_POOL`
    thread_count: u32,
}

impl BenchContext {
    /// Create a new context, selecting the best timer and calibration.
    ///
    /// # Panics
    ///
    /// Panics if `thread_count` is 0.
    #[must_use]
    pub(crate) fn new(options: ResolvedBenchOptions, thread_count: u32) -> Self {
        assert!(
            thread_count >= 1,
            "thread_count must be at least 1, got {thread_count}"
        );

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
            thread_count,
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
    /// ```
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
    ///
    /// When `thread_count > 1`, all threads call `f` concurrently via
    /// barrier-synchronized [`par_extend`](crate::thread::ThreadPool::par_extend).
    ///
    /// # Closure bounds
    ///
    /// `f` must be [`Fn`] (not just [`FnMut`]) and [`Sync`] because it may be
    /// called concurrently from multiple threads. Closures that only read
    /// shared state or operate on thread-local data satisfy both bounds
    /// naturally. For benchmarks needing mutable per-iteration state, use
    /// [`with_inputs`](Self::with_inputs).
    pub fn bench_refs<R>(&self, f: impl Fn() -> R + Sync) {
        let ctx: &BenchContext = self.context;

        if ctx.thread_count <= 1 {
            dispatch_timer!(ctx.timer, |timer| {
                let mut body =
                    |ss: u32| -> FineDuration { SamplingLoop::timed_refs(timer, &f, ss) };

                SamplingLoop::run_timed(ctx, &mut body);
            });
        } else {
            dispatch_timer!(ctx.timer, |timer| {
                let body = |ss: u32| -> FineDuration { SamplingLoop::timed_refs(timer, &f, ss) };

                SamplingLoop::run_timed_threaded(ctx, &body);
            });
        }
    }

    /// Benchmark a closure that produces owned values.
    ///
    /// Return values are collected in a `Vec` during each sample and
    /// dropped *after* timing completes, keeping drop cost outside the
    /// measurement window.
    ///
    /// When `thread_count > 1`, each thread maintains its own deferred-drop
    /// `Vec` and times independently.
    ///
    /// # Closure bounds
    ///
    /// Same as [`bench_refs`](Self::bench_refs): `f` must be [`Fn`] + [`Sync`].
    pub fn bench_values<R>(&self, f: impl Fn() -> R + Sync) {
        let ctx: &BenchContext = self.context;

        if ctx.thread_count <= 1 {
            // Single-threaded path: reusable deferred-drop buffer.
            let mut deferred: Vec<R> = Vec::new();

            dispatch_timer!(ctx.timer, |timer| {
                let mut body = |ss: u32| -> FineDuration {
                    SamplingLoop::timed_values(timer, &f, ss, &mut deferred)
                };

                SamplingLoop::run_timed(ctx, &mut body);
            });

            // Drop deferred values from the final sample.
            drop(deferred);
        } else {
            // Multi-threaded path: each thread has its own deferred Vec.
            dispatch_timer!(ctx.timer, |timer| {
                let body = |ss: u32| -> FineDuration {
                    SamplingLoop::timed_values_threaded(timer, &f, ss)
                };

                SamplingLoop::run_timed_threaded(ctx, &body);
            });
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
    ///
    /// When `thread_count > 1`, the generator runs on the main thread only.
    /// Each round generates `sample_size * thread_count` inputs, which are
    /// partitioned across threads. Each thread consumes its partition and
    /// times `f(input)` independently.
    ///
    /// # Closure bounds
    ///
    /// `f` must be [`Fn`] + [`Sync`] because it may be called concurrently
    /// from multiple threads. `I` must be [`Send`] to transfer inputs to
    /// worker threads.
    pub fn bench_values<R>(mut self, f: impl Fn(I) -> R + Sync)
    where
        I: Send,
    {
        let ctx: &BenchContext = self.context;
        let generator: &mut G = &mut self.generator;

        if ctx.thread_count <= 1 {
            // Single-threaded path.
            dispatch_timer!(ctx.timer, |timer| {
                let mut timed_body = |sample_size: u32| -> FineDuration {
                    let inputs: Vec<I> =
                        SamplingLoop::generate_inputs(generator, sample_size as usize);

                    SamplingLoop::timed_with_input_values(timer, inputs, &f)
                };

                SamplingLoop::run_timed(ctx, &mut timed_body);
            });
        } else {
            // Multi-threaded path.
            SamplingLoop::run_timed_threaded_with_values(ctx, generator, &f);
        }
    }

    /// Benchmark a closure that borrows the generated input.
    ///
    /// For each sample: inputs are generated *outside* timing via `gen()`,
    /// then `f(&input)` is called *inside* timing. Inputs and outputs
    /// are dropped after timing.
    ///
    /// When `thread_count > 1`, the generator runs on the main thread only.
    /// Each round generates `sample_size * thread_count` inputs into a
    /// shared slice. Each thread borrows its partition and times `f(&input)`.
    ///
    /// # Closure bounds
    ///
    /// `f` must be [`Fn`] + [`Sync`] because it may be called concurrently
    /// from multiple threads. `I` must be [`Sync`] to share input references
    /// across threads.
    pub fn bench_refs<R>(mut self, f: impl Fn(&I) -> R + Sync)
    where
        I: Sync,
    {
        let ctx: &BenchContext = self.context;
        let generator: &mut G = &mut self.generator;

        if ctx.thread_count <= 1 {
            // Single-threaded path.
            dispatch_timer!(ctx.timer, |timer| {
                let mut timed_body = |sample_size: u32| -> FineDuration {
                    let inputs: Vec<I> =
                        SamplingLoop::generate_inputs(generator, sample_size as usize);
                    let elapsed: FineDuration =
                        SamplingLoop::timed_with_input_refs(timer, &inputs, &f);

                    drop(inputs);

                    elapsed
                };

                SamplingLoop::run_timed(ctx, &mut timed_body);
            });
        } else {
            // Multi-threaded path.
            SamplingLoop::run_timed_threaded_with_refs(ctx, generator, &f);
        }
    }
}

// =========================================================================
//  Sampling loop - Core Measurement Algorithms
// =========================================================================

/// Unit struct containing core measurement algorithms.
pub struct SamplingLoop;

impl SamplingLoop {
    // =====================================================================
    //  Generic timing helpers (monomorphized per timer backend)
    // =====================================================================

    /// Timed body for `bench_refs`: times `sample_size` calls to `f`,
    /// using `black_box` to prevent dead-code elimination.
    #[inline(always)]
    fn timed_refs<T: TimerOps, R>(
        timer: T,
        f: &(impl Fn() -> R + Sync),
        sample_size: u32,
    ) -> FineDuration {
        let start: T::Stamp = timer.now();

        for _ in 0..sample_size {
            let r: R = f();
            StdHint::black_box(&r);
        }

        let end: T::Stamp = timer.now_end();
        timer.elapsed(start, end)
    }

    /// Timed body for `bench_values` (single-threaded): times `sample_size`
    /// calls to `f`, collecting values in `deferred` for post-timing drop.
    ///
    /// `deferred` is cleared and reserved at the start of each call,
    /// reusing the heap allocation across samples.
    #[inline(always)]
    fn timed_values<T: TimerOps, R>(
        timer: T,
        f: &(impl Fn() -> R + Sync),
        sample_size: u32,
        deferred: &mut Vec<R>,
    ) -> FineDuration {
        deferred.clear();
        deferred.reserve(sample_size as usize);

        let start: T::Stamp = timer.now();

        for _ in 0..sample_size {
            deferred.push(f());
        }

        StdHint::black_box(&*deferred);

        let end: T::Stamp = timer.now_end();
        timer.elapsed(start, end)
    }

    /// Timed body for `bench_values` (multi-threaded): each thread allocates
    /// its own deferred-drop Vec since closure is `Fn + Sync`.
    #[inline(always)]
    fn timed_values_threaded<T: TimerOps, R>(
        timer: T,
        f: &(impl Fn() -> R + Sync),
        sample_size: u32,
    ) -> FineDuration {
        let mut deferred: Vec<R> = Vec::with_capacity(sample_size as usize);

        let start: T::Stamp = timer.now();

        for _ in 0..sample_size {
            deferred.push(f());
        }

        StdHint::black_box(&deferred);

        let end: T::Stamp = timer.now_end();
        let elapsed: FineDuration = timer.elapsed(start, end);
        drop(deferred);

        elapsed
    }

    /// Timed body for with-inputs `bench_values`: consumes pre-generated inputs,
    /// collecting outputs in a deferred-drop Vec.
    #[inline(always)]
    fn timed_with_input_values<T: TimerOps, I, R>(
        timer: T,
        inputs: Vec<I>,
        f: &(impl Fn(I) -> R + Sync),
    ) -> FineDuration {
        let mut deferred: Vec<R> = Vec::with_capacity(inputs.len());

        let start: T::Stamp = timer.now();

        for input in inputs {
            deferred.push(f(input));
        }

        StdHint::black_box(&deferred);

        let end: T::Stamp = timer.now_end();
        let elapsed: FineDuration = timer.elapsed(start, end);
        drop(deferred);

        elapsed
    }

    /// Timed body for with-inputs `bench_refs`: borrows pre-generated inputs.
    #[inline(always)]
    fn timed_with_input_refs<T: TimerOps, I, R>(
        timer: T,
        inputs: &[I],
        f: &(impl Fn(&I) -> R + Sync),
    ) -> FineDuration {
        let start: T::Stamp = timer.now();

        for input in inputs {
            let r: R = f(input);
            StdHint::black_box(&r);
        }

        let end: T::Stamp = timer.now_end();
        timer.elapsed(start, end)
    }

    /// Timed body for threaded with-inputs `bench_values`: reads inputs
    /// from raw pointer (disjoint per-thread range), collects outputs.
    ///
    /// # Safety
    ///
    /// Caller must ensure `base.add(index * ss)` through `base.add((index+1) * ss - 1)`
    /// are valid, initialized, and disjoint from other threads' ranges.
    #[inline(always)]
    unsafe fn timed_threaded_input_values<T: TimerOps, I, R>(
        timer: T,
        base: *mut I,
        ss: usize,
        index: usize,
        f: &(impl Fn(I) -> R + Sync),
    ) -> FineDuration {
        let thread_base: *mut I = unsafe { base.add(index * ss) };
        let mut deferred: Vec<R> = Vec::with_capacity(ss);

        let start: T::Stamp = timer.now();

        for i in 0..ss {
            let input: I = unsafe { thread_base.add(i).read() };
            deferred.push(f(input));
        }

        StdHint::black_box(&deferred);

        let end: T::Stamp = timer.now_end();
        let elapsed: FineDuration = timer.elapsed(start, end);
        drop(deferred);

        elapsed
    }

    /// Timed body for threaded with-inputs `bench_refs`: borrows a chunk
    /// of the shared input slice.
    #[inline(always)]
    fn timed_threaded_input_refs<T: TimerOps, I, R>(
        timer: T,
        chunk: &[I],
        f: &(impl Fn(&I) -> R + Sync),
    ) -> FineDuration {
        let start: T::Stamp = timer.now();

        for input in chunk {
            let r: R = f(input);
            StdHint::black_box(&r);
        }

        let end: T::Stamp = timer.now_end();
        timer.elapsed(start, end)
    }

    /// Generate `count` inputs from a generator closure.
    #[inline]
    fn generate_inputs<I>(generator: &mut impl FnMut() -> I, count: usize) -> Vec<I> {
        let mut inputs: Vec<I> = Vec::with_capacity(count);

        for _ in 0..count {
            inputs.push(generator());
        }

        inputs
    }

    // =====================================================================
    //  Core sampling loop
    // =====================================================================

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

        let counter_collection: Option<CounterCollection> =
            Self::build_counter_collection(ctx, samples.len());

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

    // =====================================================================
    //  Multi-threaded sampling
    // =====================================================================

    /// Run the two-phase sampling loop for multi-threaded benchmarks.
    ///
    /// Phase 1 (adaptive tuning) runs single-threaded. Phase 2 (collection)
    /// uses [`BENCH_POOL`](crate::thread::BENCH_POOL) to run `timed_body`
    /// across `thread_count` threads per sample. Each thread's result
    /// becomes a separate sample in `SampleCollection`.
    ///
    /// The `timed_body` closure must be `Fn + Sync` (not `FnMut`) because
    /// it is called concurrently from multiple threads.
    ///
    /// # Panics
    ///
    /// Panics if the context has already been used for a sampling run.
    fn run_timed_threaded(ctx: &BenchContext, timed_body: &(impl Fn(u32) -> FineDuration + Sync)) {
        assert!(
            !ctx.has_run.get(),
            "BenchContext has already been used for a sampling run. Each context supports exactly one run"
        );

        // Phase 1: Adaptive tuning (single-threaded).
        // Wrap the Fn closure in a FnMut shim for tune_sample_size.
        let sample_size: u32 = ctx.options.sample_size.map_or_else(
            || {
                let mut tune_wrapper =
                    |sample_size: u32| -> FineDuration { timed_body(sample_size) };

                Self::tune_sample_size(ctx, &mut tune_wrapper)
            },
            |fixed: u32| fixed.max(1),
        );

        // Phase 2: Threaded collection.
        Self::collect_samples_threaded(ctx, sample_size, timed_body);

        ctx.has_run.set(true);
    }

    /// Collection phase for multi-threaded benchmarks.
    ///
    /// # Panics
    ///
    /// Panics if any thread panics during benchmark execution. Thread
    /// panics are detected via `None` slots in the `par_extend` result
    /// and propagated immediately.
    ///
    /// Panics if `round_count * thread_count` overflows `usize`.
    fn collect_samples_threaded(
        ctx: &BenchContext,
        sample_size: u32,
        timed_body: &(impl Fn(u32) -> FineDuration + Sync),
    ) {
        use std::sync::Barrier;

        use crate::thread::BENCH_POOL;

        let thread_count: u32 = ctx.thread_count;
        let tc: usize = thread_count as usize;
        let aux_threads: usize = tc - 1;
        let sample_count: u32 = ctx.options.sample_count;
        let min_time: FineDuration = FineDuration::from(ctx.options.min_time);
        let max_time: FineDuration = FineDuration::from(ctx.options.max_time);
        let skip_ext_time: bool = ctx.options.skip_ext_time;
        let overhead_per_iter: FineDuration = ctx.overhead.sample_loop;

        // sample_count is the total desired samples.
        // round_count = ceil(sample_count / thread_count).
        let round_count: u32 = sample_count.div_ceil(thread_count);

        // Checked capacity: round_count * thread_count may slightly exceed sample_count.
        let total_capacity: usize = (round_count as usize)
            .checked_mul(tc)
            .expect("sample capacity overflow: round_count * thread_count exceeds usize");
        let mut samples: SampleCollection = SampleCollection::with_capacity(total_capacity);
        let mut elapsed_total: FineDuration = FineDuration::ZERO;
        let mut rounds_remaining: u32 = round_count;
        let mut thread_results: Vec<Option<FineDuration>> = Vec::new();
        let barrier: Barrier = Barrier::new(tc);

        loop {
            if elapsed_total >= max_time {
                break;
            }

            if (rounds_remaining == 0) && (elapsed_total >= min_time) {
                break;
            }

            let wall_start: Instant = Instant::now();
            thread_results.clear();

            BENCH_POOL.par_extend(
                &mut thread_results,
                aux_threads,
                |_index: usize| -> FineDuration {
                    barrier.wait();
                    timed_body(sample_size)
                },
            );

            let wall_elapsed: FineDuration = FineDuration::from(wall_start.elapsed());

            Self::assert_no_panics(&thread_results, tc);
            Self::push_results_and_track_elapsed(
                &thread_results,
                &mut samples,
                sample_size,
                overhead_per_iter,
                wall_elapsed,
                skip_ext_time,
                &mut elapsed_total,
            );

            rounds_remaining = rounds_remaining.saturating_sub(1);
        }

        let counter_collection: Option<CounterCollection> =
            Self::build_counter_collection(ctx, samples.len());

        Self::store_stats(ctx, &samples, sample_size, counter_collection);
    }

    // =====================================================================
    //  Multi-threaded with-inputs: consuming variant
    // =====================================================================

    /// Run the two-phase sampling loop for multi-threaded benchmarks with
    /// per-iteration input generation (consuming variant).
    ///
    /// The generator runs on the main thread only. Each round:
    /// 1. Main thread generates `sample_size * thread_count` inputs.
    /// 2. Inputs are partitioned into per-thread chunks.
    /// 3. Each thread consumes its chunk and times `f(input)`.
    ///
    /// # Panics
    ///
    /// Panics if the context has already been used for a sampling run.
    fn run_timed_threaded_with_values<I, G, F, R>(ctx: &BenchContext, generator: &mut G, f: &F)
    where
        I: Send,
        G: FnMut() -> I,
        F: Fn(I) -> R + Sync,
    {
        assert!(
            !ctx.has_run.get(),
            "BenchContext has already been used for a sampling run. Each context supports exactly one run"
        );

        // Phase 1: Adaptive tuning (single-threaded, with inputs).
        let sample_size: u32 = ctx.options.sample_size.map_or_else(
            || {
                let mut tune_body = |sample_size: u32| -> FineDuration {
                    let inputs: Vec<I> = Self::generate_inputs(generator, sample_size as usize);

                    dispatch_timer!(ctx.timer, |timer| {
                        Self::timed_with_input_values(timer, inputs, f)
                    })
                };

                Self::tune_sample_size(ctx, &mut tune_body)
            },
            |fixed: u32| fixed.max(1),
        );

        // Phase 2: Threaded collection with inputs.
        Self::collect_samples_threaded_with_values(ctx, sample_size, generator, f);

        ctx.has_run.set(true);
    }

    /// Collection phase for multi-threaded benchmarks with input generation
    /// (consuming variant).
    ///
    /// # Panics
    ///
    /// Panics if any thread panics during benchmark execution.
    /// Panics if `round_count * thread_count` overflows `usize`.
    fn collect_samples_threaded_with_values<I, G, F, R>(
        ctx: &BenchContext,
        sample_size: u32,
        generator: &mut G,
        f: &F,
    ) where
        I: Send,
        G: FnMut() -> I,
        F: Fn(I) -> R + Sync,
    {
        use std::sync::Barrier;

        use crate::thread::{BENCH_POOL, SyncWrap};

        let thread_count: u32 = ctx.thread_count;
        let tc: usize = thread_count as usize;
        let aux_threads: usize = tc - 1;
        let ss: usize = sample_size as usize;
        let sample_count: u32 = ctx.options.sample_count;
        let min_time: FineDuration = FineDuration::from(ctx.options.min_time);
        let max_time: FineDuration = FineDuration::from(ctx.options.max_time);
        let skip_ext_time: bool = ctx.options.skip_ext_time;
        let overhead_per_iter: FineDuration = ctx.overhead.sample_loop;

        let round_count: u32 = sample_count.div_ceil(thread_count);
        let total_capacity: usize = (round_count as usize)
            .checked_mul(tc)
            .expect("sample capacity overflow: round_count * thread_count exceeds usize");
        let mut samples: SampleCollection = SampleCollection::with_capacity(total_capacity);
        let mut elapsed_total: FineDuration = FineDuration::ZERO;
        let mut rounds_remaining: u32 = round_count;
        let mut thread_results: Vec<Option<FineDuration>> = Vec::new();
        let barrier: Barrier = Barrier::new(tc);

        loop {
            if elapsed_total >= max_time {
                break;
            }

            if (rounds_remaining == 0) && (elapsed_total >= min_time) {
                break;
            }

            let wall_start: Instant = Instant::now();

            // Generate all inputs on the main thread (outside timing).
            let total_inputs: usize = ss
                .checked_mul(tc)
                .expect("input count overflow: sample_size * thread_count exceeds usize");
            let mut all_inputs: Vec<I> = Vec::with_capacity(total_inputs);

            for _ in 0..total_inputs {
                all_inputs.push(generator());
            }

            // SAFETY: We wrap the raw pointer in SyncWrap for cross-thread
            // access. Each thread reads from a disjoint range.
            let input_ptr: SyncWrap<*mut I> = unsafe { SyncWrap::new(all_inputs.as_mut_ptr()) };
            thread_results.clear();

            dispatch_timer!(ctx.timer, |timer| {
                BENCH_POOL.par_extend(
                    &mut thread_results,
                    aux_threads,
                    |index: usize| -> FineDuration {
                        barrier.wait();

                        // SAFETY: Each thread reads from disjoint range
                        // [index * ss .. (index + 1) * ss]. All inputs were
                        // fully initialized.
                        unsafe {
                            Self::timed_threaded_input_values(timer, *input_ptr, ss, index, f)
                        }
                    },
                );
            });

            // SAFETY: All elements have been moved out via ptr::read by the
            // worker threads. Prevent the Vec destructor from double-dropping.
            unsafe {
                all_inputs.set_len(0);
            }
            drop(all_inputs);

            let wall_elapsed: FineDuration = FineDuration::from(wall_start.elapsed());

            Self::assert_no_panics(&thread_results, tc);
            Self::push_results_and_track_elapsed(
                &thread_results,
                &mut samples,
                sample_size,
                overhead_per_iter,
                wall_elapsed,
                skip_ext_time,
                &mut elapsed_total,
            );

            rounds_remaining = rounds_remaining.saturating_sub(1);
        }

        // Counter handling (same as single-threaded — replicated uniformly).
        let counter_collection: Option<CounterCollection> =
            Self::build_counter_collection(ctx, samples.len());

        Self::store_stats(ctx, &samples, sample_size, counter_collection);
    }

    // =====================================================================
    //  Multi-threaded with-inputs: borrowing variant
    // =====================================================================

    /// Run the two-phase sampling loop for multi-threaded benchmarks with
    /// per-iteration input generation (borrowing variant).
    ///
    /// The generator runs on the main thread only. Each round:
    /// 1. Main thread generates `sample_size * thread_count` inputs.
    /// 2. Inputs are stored in a shared `Vec<I>` (`I: Sync`).
    /// 3. Each thread borrows its partition and times `f(&input)`.
    ///
    /// # Panics
    ///
    /// Panics if the context has already been used for a sampling run.
    fn run_timed_threaded_with_refs<I, G, F, R>(ctx: &BenchContext, generator: &mut G, f: &F)
    where
        I: Sync,
        G: FnMut() -> I,
        F: Fn(&I) -> R + Sync,
    {
        assert!(
            !ctx.has_run.get(),
            "BenchContext has already been used for a sampling run. Each context supports exactly one run"
        );

        // Phase 1: Adaptive tuning (single-threaded, with inputs).
        let sample_size: u32 = ctx.options.sample_size.map_or_else(
            || {
                let mut tune_body = |sample_size: u32| -> FineDuration {
                    let inputs: Vec<I> = Self::generate_inputs(generator, sample_size as usize);

                    let elapsed: FineDuration = dispatch_timer!(ctx.timer, |timer| {
                        Self::timed_with_input_refs(timer, &inputs, f)
                    });

                    drop(inputs);
                    elapsed
                };

                Self::tune_sample_size(ctx, &mut tune_body)
            },
            |fixed: u32| fixed.max(1),
        );

        // Phase 2: Threaded collection with inputs.
        Self::collect_samples_threaded_with_refs(ctx, sample_size, generator, f);

        ctx.has_run.set(true);
    }

    /// Collection phase for multi-threaded benchmarks with input generation
    /// (borrowing variant).
    ///
    /// Each round, the main thread generates `sample_size * thread_count`
    /// inputs into a `Vec<I>`. Since `I: Sync`, worker threads safely borrow
    /// disjoint slices of the input vector. No unsafe code is needed.
    ///
    /// # Panics
    ///
    /// Panics if any thread panics during benchmark execution.
    /// Panics if `round_count * thread_count` overflows `usize`.
    fn collect_samples_threaded_with_refs<I, G, F, R>(
        ctx: &BenchContext,
        sample_size: u32,
        generator: &mut G,
        f: &F,
    ) where
        I: Sync,
        G: FnMut() -> I,
        F: Fn(&I) -> R + Sync,
    {
        use std::sync::Barrier;

        use crate::thread::BENCH_POOL;

        let thread_count: u32 = ctx.thread_count;
        let tc: usize = thread_count as usize;
        let aux_threads: usize = tc - 1;
        let ss: usize = sample_size as usize;
        let sample_count: u32 = ctx.options.sample_count;
        let min_time: FineDuration = FineDuration::from(ctx.options.min_time);
        let max_time: FineDuration = FineDuration::from(ctx.options.max_time);
        let skip_ext_time: bool = ctx.options.skip_ext_time;
        let overhead_per_iter: FineDuration = ctx.overhead.sample_loop;

        let round_count: u32 = sample_count.div_ceil(thread_count);
        let total_capacity: usize = (round_count as usize)
            .checked_mul(tc)
            .expect("sample capacity overflow: round_count * thread_count exceeds usize");
        let mut samples: SampleCollection = SampleCollection::with_capacity(total_capacity);
        let mut elapsed_total: FineDuration = FineDuration::ZERO;
        let mut rounds_remaining: u32 = round_count;
        let mut thread_results: Vec<Option<FineDuration>> = Vec::with_capacity(tc);
        let barrier: Barrier = Barrier::new(tc);

        loop {
            if elapsed_total >= max_time {
                break;
            }

            if (rounds_remaining == 0) && (elapsed_total >= min_time) {
                break;
            }

            // wall_start is taken before input generation to match
            // single-threaded collect_samples semantics, where wall-clock
            // time wraps the entire timed_body call (which includes
            // generator work inside the closure).
            let wall_start: Instant = Instant::now();

            // Generate all inputs on the main thread (outside timing).
            let total_inputs: usize = ss
                .checked_mul(tc)
                .expect("input count overflow: sample_size * thread_count exceeds usize");
            let mut all_inputs: Vec<I> = Vec::with_capacity(total_inputs);

            for _ in 0..total_inputs {
                all_inputs.push(generator());
            }

            // I: Sync, so &[I] is safe to share across threads.
            let inputs_ref: &[I] = &all_inputs;
            thread_results.clear();

            dispatch_timer!(ctx.timer, |timer| {
                BENCH_POOL.par_extend(
                    &mut thread_results,
                    aux_threads,
                    |index: usize| -> FineDuration {
                        barrier.wait();

                        let chunk: &[I] = &inputs_ref[index * ss..(index + 1) * ss];
                        Self::timed_threaded_input_refs(timer, chunk, f)
                    },
                );
            });

            // Drop inputs after timing completes.
            drop(all_inputs);

            let wall_elapsed: FineDuration = FineDuration::from(wall_start.elapsed());

            Self::assert_no_panics(&thread_results, tc);
            Self::push_results_and_track_elapsed(
                &thread_results,
                &mut samples,
                sample_size,
                overhead_per_iter,
                wall_elapsed,
                skip_ext_time,
                &mut elapsed_total,
            );

            rounds_remaining = rounds_remaining.saturating_sub(1);
        }

        let counter_collection: Option<CounterCollection> =
            Self::build_counter_collection(ctx, samples.len());

        Self::store_stats(ctx, &samples, sample_size, counter_collection);
    }

    // =====================================================================
    //  Multi-threaded helpers
    // =====================================================================

    /// Build a [`CounterCollection`] From the context's counter info.
    ///
    /// Returns `None` if no counter was attached via [`Bencher::counter`].
    #[must_use]
    #[inline(always)]
    fn build_counter_collection(
        ctx: &BenchContext,
        sample_count: usize,
    ) -> Option<CounterCollection> {
        ctx.counter_info
            .get()
            .map(|(count, kind): (u64, CounterKind)| {
                CounterCollection::uniform(kind, count, sample_count)
            })
    }

    /// Assert that no threads panicked during the benchmark round.
    ///
    /// # Panics
    ///
    /// Panics if any slot in `results` is `None`, indicating a thread panic.
    #[inline]
    fn assert_no_panics(results: &[Option<FineDuration>], tc: usize) {
        let panic_count: usize = results
            .iter()
            .filter(|r: &&Option<FineDuration>| r.is_none())
            .count();

        assert!(
            panic_count == 0,
            "benchmark panicked on {panic_count} of {tc} threads during sample round"
        );
    }

    /// Push each thread's timing result as a sample and update elapsed time.
    ///
    /// # Panics
    ///
    /// Panics if any result slot is `None`. Call [`assert_no_panics`](Self::assert_no_panics)
    /// before this method.
    fn push_results_and_track_elapsed(
        results: &[Option<FineDuration>],
        samples: &mut SampleCollection,
        sample_size: u32,
        overhead_per_iter: FineDuration,
        wall_elapsed: FineDuration,
        skip_ext_time: bool,
        elapsed_total: &mut FineDuration,
    ) {
        let mut slowest: FineDuration = FineDuration::ZERO;

        for result in results {
            let raw_duration: &FineDuration = result.as_ref().unwrap();

            let per_iter: FineDuration = raw_duration
                .div_u64(u64::from(sample_size))
                .checked_sub(overhead_per_iter)
                .unwrap_or(FineDuration::ZERO);

            samples.push(RawSample {
                duration: per_iter,
                sample_size,
            });

            if skip_ext_time && (raw_duration.picos > slowest.picos) {
                slowest = *raw_duration;
            }
        }

        if skip_ext_time {
            *elapsed_total += slowest;
        } else {
            *elapsed_total += wall_elapsed;
        }
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

    use crate::config::BenchOptions;

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(sample_count),
        sample_size,
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 1);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    f(&bencher);

    ctx.finish()
        .expect("benchmark should have produced stats")
        .stats
}

/// Run a benchmark closure with multiple threads and return stats.
///
/// Same as [`run_bench_closure`] but accepts a `thread_count` parameter
/// for testing the multi-threaded sampling path.
#[cfg(test)]
pub(crate) fn run_bench_closure_threaded(
    sample_count: u32,
    sample_size: Option<u32>,
    thread_count: u32,
    f: impl FnOnce(&Bencher<'_>),
) -> PercentileStats {
    use std::time::Duration;

    use crate::config::BenchOptions;

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(sample_count),
        sample_size,
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, thread_count);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    f(&bencher);

    ctx.finish()
        .expect("benchmark should have produced stats")
        .stats
}

/// Dispatch a timer-dependent operation across all timer backends.
///
/// Binds the concrete timer value from the enum variant to `$timer`,
/// then evaluates `$body` once per variant. Since `$body` is duplicated
/// in each match arm, it is monomorphized for each timer backend.
#[macro_export]
macro_rules! dispatch_timer {
    ($timer_expr:expr, |$timer:ident| $body:expr) => {
        match $timer_expr {
            Timer::Os($timer) => $body,

            #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
            Timer::Tsc($timer) => $body,
        }
    };
}

#[cfg(test)]
mod unit_tests;
