//! Unit tests for [`Bencher`], [`BenchContext`], and [`BencherWithInput`].

use super::{
    BenchContext, BenchResult, Bencher, CounterCollection, PercentileStats, ResolvedBenchOptions,
    run_bench_closure,
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
    let options: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&crate::config::BenchOptions {
            sample_count: Some(100),
            sample_size: Some(1),
            ..Default::default()
        });

    let ctx: BenchContext = BenchContext::new(options);
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

    let options: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&crate::config::BenchOptions {
            sample_count: Some(10),
            sample_size: Some(5),
            ..Default::default()
        });

    let ctx: BenchContext = BenchContext::new(options);
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
    let options: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&crate::config::BenchOptions {
            sample_count: Some(50),
            sample_size: Some(1),
            ..Default::default()
        });

    let ctx: BenchContext = BenchContext::new(options);
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
        ResolvedBenchOptions::from_options(&crate::config::BenchOptions::default());

    let ctx: BenchContext = BenchContext::new(options);
    let result: Option<BenchResult> = ctx.finish();

    assert!(result.is_none());
}

// --- counter ---

#[test]
fn bench_with_counter() {
    use crate::counter::{BytesCount, CounterKind};
    use std::time::Duration;

    let options: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&crate::config::BenchOptions {
            sample_count: Some(100),
            sample_size: Some(1),
            max_time: Some(Duration::from_millis(100)),
            ..Default::default()
        });

    let ctx: BenchContext = BenchContext::new(options);
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

    let options: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&crate::config::BenchOptions {
            sample_count: Some(50),
            sample_size: Some(1),
            max_time: Some(Duration::from_millis(100)),
            ..Default::default()
        });

    let ctx: BenchContext = BenchContext::new(options);
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

    let options: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&crate::config::BenchOptions {
            sample_count: Some(50),
            sample_size: Some(1),
            max_time: Some(Duration::from_millis(100)),
            ..Default::default()
        });

    let ctx: BenchContext = BenchContext::new(options);
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
    let options: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&crate::config::BenchOptions {
            sample_count: Some(50),
            sample_size: None,
            max_time: Some(Duration::from_millis(100)),
            ..Default::default()
        });

    let ctx: BenchContext = BenchContext::new(options);
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
    let options: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&crate::config::BenchOptions {
            sample_count: Some(50),
            sample_size: Some(10),
            max_time: Some(Duration::from_millis(100)),
            ..Default::default()
        });

    let ctx: BenchContext = BenchContext::new(options);
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

    let options: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&crate::config::BenchOptions {
            sample_count: Some(50),
            sample_size: Some(1),
            max_time: Some(Duration::from_millis(100)),
            ..Default::default()
        });

    let ctx: BenchContext = BenchContext::new(options);
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
    let options: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&crate::config::BenchOptions {
            sample_count: Some(200),
            sample_size: Some(1),
            max_time: Some(Duration::from_millis(50)),
            ..Default::default()
        });

    let ctx: BenchContext = BenchContext::new(options);
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

    let options: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&crate::config::BenchOptions {
            sample_count: Some(10),
            sample_size: Some(1),
            max_time: Some(Duration::from_millis(50)),
            ..Default::default()
        });

    let ctx: BenchContext = BenchContext::new(options);
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

    let options: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&crate::config::BenchOptions {
            sample_count: Some(50),
            sample_size: Some(1),
            max_time: Some(Duration::from_millis(100)),
            ..Default::default()
        });

    let ctx: BenchContext = BenchContext::new(options);
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
