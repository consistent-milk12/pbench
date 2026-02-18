//! Integration test: verify `#[pbench::bench]` generates valid code.
//!
//! This test confirms the macro produces syntactically and type-correct
//! registration code. It does not execute benchmarks.

use pbench::bencher::Bencher;

/// Plain benchmark with a `Bencher` parameter.
#[pbench::bench]
fn noop_bench(b: &Bencher<'_>) {
    b.bench_refs(|| {});
}

/// Zero-parameter benchmark — macro wraps it in `bench_refs`.
#[pbench::bench]
const fn zero_param_bench() {}

/// Benchmark with `args` — generates `GenericBenchEntry`.
#[pbench::bench(args = [1, 2, 4])]
fn args_bench(b: &Bencher<'_>, arg: &str) {
    let _ = arg;
    b.bench_refs(|| {});
}
