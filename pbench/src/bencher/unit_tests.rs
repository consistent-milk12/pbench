//! Unit tests for [`Bencher`], [`BenchContext`], and [`BencherWithInput`].

use crate::config::BenchOptions;

use super::{
    BenchContext, BenchResult, Bencher, CounterCollection, PercentileStats, ResolvedBenchOptions,
    run_bench_closure, run_bench_closure_threaded,
};
use std::hint as StdHint;

// --- bench_refs ---

#[test]
fn bench_refs_collects_samples() {
    let stats: PercentileStats = run_bench_closure(100, Some(1), |b: &Bencher<'_>| {
        b.bench_refs(|| StdHint::black_box(42));
    });

    assert!(stats.sample_count >= 100);
    assert!(stats.percentiles.p50.picos > 0);
}

// --- bench_values ---

#[test]
fn bench_values_collects_samples() {
    let stats: PercentileStats = run_bench_closure(100, Some(1), |b: &Bencher<'_>| {
        b.bench_values(|| vec![1_i32, 2, 3]);
    });

    assert!(stats.sample_count >= 100);
    assert!(stats.percentiles.p50.picos > 0);
}

// --- with_inputs ---

#[test]
fn with_inputs_provides_fresh_input() {
    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(100),
        sample_size: Some(1),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 1);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher
        .with_inputs(|| Vec::<u8>::with_capacity(16))
        .bench_values(|mut v: Vec<u8>| {
            v.push(1);
            v
        });

    let stats: Option<BenchResult> = ctx.finish();

    assert!(stats.is_some());
    assert!(stats.unwrap().stats.sample_count >= 100);
}

#[test]
fn with_inputs_calls_generator_per_iteration() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static GEN_COUNT: AtomicUsize = AtomicUsize::new(0);
    GEN_COUNT.store(0, Ordering::Relaxed);

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(10),
        sample_size: Some(5),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 1);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher
        .with_inputs(|| {
            GEN_COUNT.fetch_add(1, Ordering::Relaxed);
            42_i32
        })
        .bench_refs(|val: &i32| {
            StdHint::black_box(*val);
        });

    let calls: usize = GEN_COUNT.load(Ordering::Relaxed);

    // 10 samples x 5 iterations = 50 generator calls minimum.
    assert!(calls >= 50, "expected >= 50 generator calls, got {calls}");
}

// --- bench_values drop correctness ---

#[test]
fn bench_values_drop_correctness() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[allow(dead_code)]
    struct Tracked(i32);

    impl Drop for Tracked {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::Relaxed);
        }
    }

    DROP_COUNT.store(0, Ordering::Relaxed);

    let _stats: PercentileStats = run_bench_closure(10, Some(1), |b: &Bencher<'_>| {
        b.bench_values(|| Tracked(42));
    });

    let drops: usize = DROP_COUNT.load(Ordering::Relaxed);

    // Each sample produces 1 value (sample_size=1), and we have >= 10 samples.
    // Every produced value must have been dropped.
    assert!(drops >= 10, "expected >= 10 drops, got {drops}");
}

// --- input/output type matrix ---

#[test]
fn bench_refs_unit_output() {
    let stats: PercentileStats = run_bench_closure(50, Some(1), |b: &Bencher<'_>| {
        b.bench_refs(|| {});
    });

    assert!(stats.sample_count >= 50);
}

#[test]
fn bench_values_string_output() {
    let stats: PercentileStats = run_bench_closure(50, Some(1), |b: &Bencher<'_>| {
        b.bench_values(|| String::from("hello"));
    });

    assert!(stats.sample_count >= 50);
}

#[test]
fn bench_refs_i32_output() {
    let stats: PercentileStats = run_bench_closure(50, Some(1), |b: &Bencher<'_>| {
        b.bench_refs(|| 123_i32);
    });

    assert!(stats.sample_count >= 50);
}

#[test]
fn with_inputs_bench_refs() {
    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(50),
        sample_size: Some(1),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 1);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher
        .with_inputs(|| String::from("test"))
        .bench_refs(|s: &String| {
            StdHint::black_box(s.len());
        });

    let result: Option<BenchResult> = ctx.finish();
    assert!(result.is_some());
}

// --- adaptive tuning ---

#[test]
fn adaptive_tuning_produces_stats() {
    // sample_size = None triggers adaptive tuning
    let stats: PercentileStats = run_bench_closure(50, None, |b: &Bencher<'_>| {
        b.bench_refs(|| StdHint::black_box(42));
    });

    assert!(stats.sample_count >= 50);
    assert!(stats.percentiles.p50.picos > 0);
}

// --- BenchContext::finish with no run ---

#[test]
fn context_finish_without_run_returns_none() {
    let options: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&BenchOptions::default());

    let ctx: BenchContext = BenchContext::new(options, 1);
    let result: Option<BenchResult> = ctx.finish();

    assert!(result.is_none());
}

// --- counter ---

#[test]
fn bench_with_counter() {
    use crate::counter::{BytesCount, CounterKind};
    use std::time::Duration;

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(100),
        sample_size: Some(1),
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 1);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher
        .counter(BytesCount::new(1024))
        .bench_refs(|| StdHint::black_box(42));

    let result: BenchResult = ctx.finish().expect("benchmark should have produced stats");
    assert!(result.stats.sample_count >= 100);

    let counter: CounterCollection = result.counter.expect("should have counter collection");
    assert_eq!(counter.kind(), CounterKind::Bytes);

    let mean: f64 = counter.mean_count().expect("should have mean count");
    assert!((mean - 1024.0).abs() < f64::EPSILON);
}

#[test]
fn bench_without_counter_has_no_collection() {
    use std::time::Duration;

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(50),
        sample_size: Some(1),
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 1);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher.bench_refs(|| StdHint::black_box(42));

    let result: BenchResult = ctx.finish().expect("should have stats");
    assert!(result.stats.sample_count >= 50);
    assert!(
        result.counter.is_none(),
        "counter should be None when none was attached"
    );
}

#[test]
fn bench_with_counter_via_context_finish() {
    use crate::counter::{CounterKind, ItemsCount};
    use std::time::Duration;

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(50),
        sample_size: Some(1),
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 1);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher
        .counter(ItemsCount::new(500))
        .bench_values(|| vec![1_i32, 2, 3]);

    let result: BenchResult = ctx.finish().expect("should have stats");
    let counter: CounterCollection = result.counter.expect("should have counter");

    assert_eq!(counter.kind(), CounterKind::Items);
    assert_eq!(counter.len(), result.stats.sample_count as usize);
}

// --- counter: expanded coverage ---

#[test]
fn counter_with_adaptive_tuning() {
    use crate::counter::{BytesCount, CounterKind};
    use std::time::Duration;

    // sample_size = None triggers adaptive tuning
    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(50),
        sample_size: None,
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 1);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher
        .counter(BytesCount::new(256))
        .bench_refs(|| StdHint::black_box(42));

    let result: BenchResult = ctx.finish().expect("should have stats");
    assert!(result.stats.sample_count >= 50);

    let counter: CounterCollection = result.counter.expect("should have counter");
    assert_eq!(counter.kind(), CounterKind::Bytes);
    assert_eq!(counter.len(), result.stats.sample_count as usize);

    let mean: f64 = counter.mean_count().expect("should have mean");
    assert!((mean - 256.0).abs() < f64::EPSILON);
}

#[test]
fn counter_with_multi_iteration_samples() {
    use crate::counter::{CounterKind, ItemsCount};
    use std::time::Duration;

    // sample_size = 10 means each sample runs 10 iterations
    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(50),
        sample_size: Some(10),
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 1);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher
        .counter(ItemsCount::new(100))
        .bench_refs(|| StdHint::black_box(42));

    let result: BenchResult = ctx.finish().expect("should have stats");
    assert!(result.stats.sample_count >= 50);

    let counter: CounterCollection = result.counter.expect("should have counter");
    assert_eq!(counter.kind(), CounterKind::Items);

    // Counter collection has one entry per sample, not per iteration.
    assert_eq!(counter.len(), result.stats.sample_count as usize);

    let mean: f64 = counter.mean_count().expect("should have mean");
    assert!((mean - 100.0).abs() < f64::EPSILON);
}

#[test]
fn counter_overwrite_uses_last_value() {
    use crate::counter::{BytesCount, CounterKind, ItemsCount};
    use std::time::Duration;

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(50),
        sample_size: Some(1),
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 1);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    // First call sets BytesCount, second overwrites with ItemsCount.
    // Last-write-wins: only ItemsCount should appear in results.
    bencher
        .counter(BytesCount::new(1024))
        .counter(ItemsCount::new(42))
        .bench_refs(|| StdHint::black_box(42));

    let result: BenchResult = ctx.finish().expect("should have stats");
    let counter: CounterCollection = result.counter.expect("should have counter");

    // The last counter attached was ItemsCount, so kind must be Items.
    assert_eq!(counter.kind(), CounterKind::Items);

    let mean: f64 = counter.mean_count().expect("should have mean");
    assert!((mean - 42.0).abs() < f64::EPSILON);
}

#[test]
fn counter_length_matches_actual_sample_count() {
    use crate::counter::{BytesCount, CounterKind};
    use std::time::Duration;

    // Use a very short max_time but a large sample_count.
    // The min-time floor may cause extra samples beyond sample_count,
    // or max_time may cut collection short. Either way, counter length
    // must exactly match the actual number of collected samples
    // (i.e. stats.sample_count).
    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(200),
        sample_size: Some(1),
        max_time: Some(Duration::from_millis(50)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 1);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher
        .counter(BytesCount::new(512))
        .bench_refs(|| StdHint::black_box(42));

    let result: BenchResult = ctx.finish().expect("should have stats");
    let counter: CounterCollection = result.counter.expect("should have counter");
    assert_eq!(counter.kind(), CounterKind::Bytes);

    // The critical invariant: counter entries == actual samples collected.
    assert_eq!(
        counter.len(),
        result.stats.sample_count as usize,
        "counter length must match actual sample count"
    );
}

// --- contract enforcement ---

#[test]
#[should_panic(expected = "BenchContext has already been used for a sampling run")]
fn second_run_panics() {
    use std::time::Duration;

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(10),
        sample_size: Some(1),
        max_time: Some(Duration::from_millis(50)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 1);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    // First run succeeds.
    bencher.bench_refs(|| StdHint::black_box(42));

    // Second run must panic - single-use contract enforcement.
    bencher.bench_refs(|| StdHint::black_box(42));
}

#[test]
fn with_inputs_counter_uses_constant_value() {
    use crate::counter::{BytesCount, CounterKind};
    use std::time::Duration;

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(50),
        sample_size: Some(1),
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 1);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    // Attach a constant counter, then use with_inputs generating
    // variable-sized data. This is a limitation of the current design.
    bencher.counter(BytesCount::new(1024));
    bencher
        .with_inputs(|| vec![0_u8; 512])
        .bench_values(|v: Vec<u8>| StdHint::black_box(v.len()));

    let result: BenchResult = ctx.finish().expect("should have stats");
    let counter: CounterCollection = result.counter.expect("should have counter");
    assert_eq!(counter.kind(), CounterKind::Bytes);

    // Every sample gets the same constant counter value (1024),
    // regardless of what the input generator produced (512 bytes).
    let mean: f64 = counter.mean_count().expect("should have mean");
    assert!((mean - 1024.0).abs() < f64::EPSILON);
}

// --- multi-threaded ---

#[test]
fn bench_refs_threaded_collects_samples() {
    // 2 threads, 100 total samples requested, sample_size = 1.
    // round_count = ceil(100 / 2) = 50 rounds.
    // Total samples = 50 rounds x 2 threads = 100.
    let stats: PercentileStats = run_bench_closure_threaded(100, Some(1), 2, |b: &Bencher<'_>| {
        b.bench_refs(|| StdHint::black_box(42));
    });

    // Should collect exactly 100 samples (50 rounds x 2 threads).
    // May collect more due to min_time floor.
    assert!(
        stats.sample_count >= 100,
        "expected >= 100 samples with 2 threads, got {}",
        stats.sample_count
    );
    assert!(stats.percentiles.p50.picos > 0);
}

#[test]
fn bench_values_threaded_collects_samples() {
    let stats: PercentileStats = run_bench_closure_threaded(100, Some(1), 2, |b: &Bencher<'_>| {
        b.bench_values(|| vec![1_i32, 2, 3]);
    });

    assert!(
        stats.sample_count >= 100,
        "expected >= 100 samples with 2 threads, got {}",
        stats.sample_count
    );
    assert!(stats.percentiles.p50.picos > 0);
}

#[test]
fn bench_refs_threaded_sample_count_is_total_not_per_thread() {
    // With sample_count=60, thread_count=4:
    // round_count = ceil(60 / 4) = 15.
    // total = 15 * 4 = 60 samples.
    let stats: PercentileStats = run_bench_closure_threaded(60, Some(1), 4, |b: &Bencher<'_>| {
        b.bench_refs(|| StdHint::black_box(42));
    });

    // Must collect at least 60 samples (exactly 60 from ceiling division).
    assert!(
        stats.sample_count >= 60,
        "expected >= 60 total samples with 4 threads, got {}",
        stats.sample_count
    );

    // Must NOT collect wildly more than expected (old bug: sample_count * thread_count).
    // With round_count=15 and 4 threads, max is 60 plus any min_time overflow.
    // Allow up to 2x for min_time floor.
    assert!(
        stats.sample_count <= 120,
        "expected <= 120 samples (2x headroom), got {} — sample_count may be multiplied incorrectly",
        stats.sample_count
    );
}

#[test]
fn bench_refs_threaded_four_threads() {
    // 4 threads, 120 total samples.
    // round_count = ceil(120 / 4) = 30.
    // total = 30 * 4 = 120.
    let stats: PercentileStats = run_bench_closure_threaded(120, Some(1), 4, |b: &Bencher<'_>| {
        b.bench_refs(|| StdHint::black_box(42));
    });

    assert!(
        stats.sample_count >= 120,
        "expected >= 120 samples with 4 threads, got {}",
        stats.sample_count
    );
}

#[test]
#[should_panic(expected = "benchmark panicked")]
fn bench_refs_threaded_propagates_panic() {
    use std::sync::atomic::{AtomicU32, Ordering};

    static CALL_COUNT: AtomicU32 = AtomicU32::new(0);

    let _ = run_bench_closure_threaded(50, Some(1), 2, |b: &Bencher<'_>| {
        b.bench_refs(|| {
            let n: u32 = CALL_COUNT.fetch_add(1, Ordering::Relaxed);

            // Panic on the 3rd call — will happen on one of the threads.
            assert!(n != 2, "intentional benchmark panic");

            StdHint::black_box(42)
        });
    });
}

#[test]
fn bench_refs_threaded_counter_length_matches_sample_count() {
    use crate::counter::{BytesCount, CounterKind};
    use std::time::Duration;

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(80),
        sample_size: Some(1),
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 2);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher
        .counter(BytesCount::new(256))
        .bench_refs(|| StdHint::black_box(42));

    let result: BenchResult = ctx.finish().expect("should have stats");
    let counter: CounterCollection = result.counter.expect("should have counter");
    assert_eq!(counter.kind(), CounterKind::Bytes);

    // Critical invariant: counter entries == actual sample count.
    assert_eq!(
        counter.len(),
        result.stats.sample_count as usize,
        "counter length must match actual sample count in threaded mode"
    );

    let mean: f64 = counter.mean_count().expect("should have mean");
    assert!((mean - 256.0).abs() < f64::EPSILON);
}

#[test]
fn bench_refs_threaded_with_one_thread_matches_single() {
    // thread_count=1 should take the single-threaded path.
    let stats: PercentileStats = run_bench_closure_threaded(100, Some(1), 1, |b: &Bencher<'_>| {
        b.bench_refs(|| StdHint::black_box(42));
    });

    assert!(stats.sample_count >= 100);
    assert!(stats.percentiles.p50.picos > 0);
}

#[test]
#[should_panic(expected = "thread_count must be at least 1")]
fn bench_context_zero_threads_panics() {
    let options: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&BenchOptions::default());

    let _ = BenchContext::new(options, 0);
}

// --- multi-threaded with_inputs ---

#[test]
fn with_inputs_bench_values_threaded() {
    use std::time::Duration;

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(100),
        sample_size: Some(1),
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 2);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher
        .with_inputs(|| vec![1_i32, 2, 3])
        .bench_values(|mut v: Vec<i32>| {
            v.sort_unstable();
            v
        });

    let result: BenchResult = ctx.finish().expect("should have stats");

    assert!(
        result.stats.sample_count >= 100,
        "expected >= 100 samples with 2 threads, got {}",
        result.stats.sample_count
    );
    assert!(result.stats.percentiles.p50.picos > 0);
}

#[test]
fn with_inputs_bench_refs_threaded() {
    use std::time::Duration;

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(100),
        sample_size: Some(1),
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 2);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher
        .with_inputs(|| String::from("hello world"))
        .bench_refs(|s: &String| {
            StdHint::black_box(s.len());
        });

    let result: BenchResult = ctx.finish().expect("should have stats");

    assert!(
        result.stats.sample_count >= 100,
        "expected >= 100 samples with 2 threads, got {}",
        result.stats.sample_count
    );
    assert!(result.stats.percentiles.p50.picos > 0);
}

#[test]
fn with_inputs_bench_values_threaded_four_threads() {
    use std::time::Duration;

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(120),
        sample_size: Some(1),
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 4);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher
        .with_inputs(|| 42_i32)
        .bench_values(|x: i32| StdHint::black_box(x + 1));

    let result: BenchResult = ctx.finish().expect("should have stats");

    assert!(
        result.stats.sample_count >= 120,
        "expected >= 120 samples with 4 threads, got {}",
        result.stats.sample_count
    );
}

#[test]
fn with_inputs_threaded_generator_call_count() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    static GEN_COUNT: AtomicUsize = AtomicUsize::new(0);
    GEN_COUNT.store(0, Ordering::Relaxed);

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(10),
        sample_size: Some(5),
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 2);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher
        .with_inputs(|| {
            GEN_COUNT.fetch_add(1, Ordering::Relaxed);
            42_i32
        })
        .bench_refs(|val: &i32| {
            StdHint::black_box(*val);
        });

    let calls: usize = GEN_COUNT.load(Ordering::Relaxed);

    // 10 total samples requested, 2 threads.
    // round_count = ceil(10 / 2) = 5 rounds.
    // Per round: sample_size(5) * thread_count(2) = 10 generator calls.
    // Total: 5 rounds * 10 calls = 50 minimum.
    // Tuning phase adds more calls on top.
    assert!(
        calls >= 50,
        "expected >= 50 generator calls with 2 threads and sample_size=5, got {calls}"
    );
}

#[test]
fn with_inputs_bench_refs_threaded_counter_matches() {
    use crate::counter::{BytesCount, CounterKind};
    use std::time::Duration;

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(80),
        sample_size: Some(1),
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 2);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher.counter(BytesCount::new(256));
    bencher
        .with_inputs(|| String::from("test"))
        .bench_refs(|s: &String| {
            StdHint::black_box(s.len());
        });

    let result: BenchResult = ctx.finish().expect("should have stats");
    let counter: CounterCollection = result.counter.expect("should have counter");
    assert_eq!(counter.kind(), CounterKind::Bytes);

    assert_eq!(
        counter.len(),
        result.stats.sample_count as usize,
        "counter length must match actual sample count in threaded with_inputs mode"
    );
}

#[test]
#[should_panic(expected = "benchmark panicked")]
fn with_inputs_bench_values_threaded_propagates_panic() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    static CALL_COUNT: AtomicU32 = AtomicU32::new(0);
    CALL_COUNT.store(0, Ordering::Relaxed);

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(50),
        sample_size: Some(1),
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 2);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher.with_inputs(|| 42_i32).bench_values(|x: i32| {
        let n: u32 = CALL_COUNT.fetch_add(1, Ordering::Relaxed);

        assert!(n != 2, "intentional benchmark panic");

        StdHint::black_box(x + 1)
    });
}

#[test]
fn with_inputs_bench_values_threaded_non_divisible_sample_count() {
    use std::time::Duration;

    // 7 samples / 3 threads = ceil(7/3) = 3 rounds * 3 threads = 9 actual samples.
    // Validates div_ceil behavior and the "may slightly exceed" contract.
    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(7),
        sample_size: Some(1),
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 3);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher
        .with_inputs(|| 42_i32)
        .bench_values(|x: i32| StdHint::black_box(x + 1));

    let result: BenchResult = ctx.finish().expect("should have stats");

    // div_ceil(7, 3) = 3 rounds * 3 threads = 9 samples minimum.
    assert!(
        result.stats.sample_count >= 7,
        "expected >= 7 samples (requested), got {}",
        result.stats.sample_count
    );

    // The actual count should be a multiple of thread_count (3).
    assert_eq!(
        result.stats.sample_count % 3,
        0,
        "sample count {} should be a multiple of thread_count (3)",
        result.stats.sample_count
    );
}

#[test]
fn with_inputs_bench_values_threaded_drop_correctness() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[allow(dead_code)]
    struct TrackedValue(u64);

    impl Drop for TrackedValue {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    // Track how many values are alive (constructed but not yet dropped).
    // If deferred drop is working correctly, values should be dropped
    // AFTER timing completes, not during the timed loop.
    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
    static CREATE_COUNT: AtomicUsize = AtomicUsize::new(0);

    DROP_COUNT.store(0, Ordering::SeqCst);
    CREATE_COUNT.store(0, Ordering::SeqCst);

    let options: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&BenchOptions {
        sample_count: Some(10),
        sample_size: Some(4),
        max_time: Some(Duration::from_millis(100)),
        ..Default::default()
    });

    let ctx: BenchContext = BenchContext::new(options, 2);
    let bencher: Bencher<'_> = Bencher::new(&ctx);

    bencher.with_inputs(|| 42_u64).bench_values(|x: u64| {
        CREATE_COUNT.fetch_add(1, Ordering::SeqCst);
        TrackedValue(x)
    });

    let total_created: usize = CREATE_COUNT.load(Ordering::SeqCst);
    let total_dropped: usize = DROP_COUNT.load(Ordering::SeqCst);

    // Every created TrackedValue must have been dropped.
    assert_eq!(
        total_created, total_dropped,
        "created {total_created} TrackedValues but only dropped {total_dropped} — deferred drop leak"
    );

    // Sanity: at least some values were created.
    assert!(
        total_created > 0,
        "no TrackedValues created — test is not exercising the code"
    );
}
