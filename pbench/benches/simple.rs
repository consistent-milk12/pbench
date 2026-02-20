//! Simple benchmark example demonstrating basic pbench usage.
//!
//! Run with: `cargo bench -p pbench --bench simple`
//! JSON output: `cargo bench -p pbench --bench simple -- --output json`

use pbench::Bencher;

/// Baseline: measures the overhead of the benchmarking harness itself.
#[pbench::bench]
fn noop(b: &Bencher<'_>) {
    b.bench_refs(|| {});
}

/// Arithmetic operation through `black_box` to prevent optimisation.
#[pbench::bench]
fn black_box_addition(b: &Bencher<'_>) {
    b.bench_refs(|| std::hint::black_box(1u64 + 2));
}

/// Allocate a 1 KiB Vec, measuring allocation cost.
///
/// Uses `bench_values` so the allocated Vec is dropped outside the
/// measurement window (deferred drop).
#[pbench::bench]
fn vec_alloc(b: &Bencher<'_>) {
    b.bench_values(|| Vec::<u8>::with_capacity(1024));
}

fn main() {
    pbench::main();
}
