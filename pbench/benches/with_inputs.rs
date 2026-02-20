//! Benchmark example demonstrating throughput counters and sorting benchmarks.
//!
//! Run with: `cargo bench -p pbench --bench with_inputs`

use pbench::{Bencher, ItemsCount};

/// Sort a Vec of 1000 elements, measuring per-iteration throughput.
///
/// `ItemsCount` reports throughput in items/s.
/// Input generation is included in the measurement to keep the example
/// simple. Use `bench_values` for deferred-drop of the result.
#[pbench::bench]
fn sort_1000(b: &Bencher<'_>) {
    let len: usize = 1000;

    b.counter(ItemsCount::new(len as u64));

    b.bench_values(|| {
        // Deterministic pseudo-random sequence (no external deps).
        let mut v: Vec<u64> = Vec::with_capacity(len);
        let mut state: u64 = 12345;

        for _ in 0..len {
            // Simple xorshift64.
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            v.push(state);
        }

        v.sort_unstable();
        v
    });
}

/// Sort a short Vec to compare scaling behaviour.
#[pbench::bench]
fn sort_10(b: &Bencher<'_>) {
    let len: usize = 10;

    b.counter(ItemsCount::new(len as u64));

    b.bench_values(|| {
        let mut v: Vec<u64> = Vec::with_capacity(len);
        let mut state: u64 = 54321;

        for _ in 0..len {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            v.push(state);
        }

        v.sort_unstable();
        v
    });
}

fn main() {
    pbench::main();
}
