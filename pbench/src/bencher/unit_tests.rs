//! Unit tests for [`Bencher`], [`BenchContext`], and [`BencherWithInput`].

use super::*;
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

    let stats: Option<PercentileStats> = ctx.finish();
    assert!(stats.is_some());
    assert!(stats.unwrap().sample_count >= 100);
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

    let stats: Option<PercentileStats> = ctx.finish();
    assert!(stats.is_some());
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
    let result: Option<PercentileStats> = ctx.finish();
    assert!(result.is_none());
}
