# pbench

Tail latency benchmarking for Rust.

 pbench reports p50/p95/p99/p99.9/p99.99 so you can see exactly where your code breaks down under pressure, especially under contention, with multi-threaded benchmarks that reveal how tail latencies scale across thread counts.

Built from scratch, heavily inspired by [divan](https://github.com/nvzqz/divan)'s architecture. Worked on this for analyzing tail latency of my [masstree](https://github.com/consistent-milk12/masstree) crate.

NOTE: Check `pbench/benches/` for detailed examples.

## Features

- **Tail latency analysis** - p50/p95/p99/p99.9/p99.99 reporting reveals worst-case behavior that mean/median benchmarks miss entirely
- **Multi-threaded benchmarks** - `threads = [1, 2, 4, 8]` runs each benchmark at multiple thread counts with barrier-synchronized parallel sampling, exposing how tail latencies degrade under contention
- **Dual-mode percentile computation** - exact sorted-array with linear interpolation for <100K samples, HdrHistogram for larger counts
- **Adaptive tuning** - automatically determines sample size based on timer precision
- **TSC support** - uses RDTSC on x86_64 for sub-nanosecond precision when available
- **Baseline regression detection** - save/load baselines with per-percentile threshold comparison to catch tail latency regressions in CI
- **Multiple output formats** - terminal table, JSON, CSV
- **Zero-boilerplate discovery** - linker-based benchmark registration via `#[pbench::bench]`

## Quick Start

```rust
use pbench::Bencher;

#[pbench::bench]
fn my_benchmark(b: &Bencher<'_>) {
    b.bench_refs(|| {
        std::hint::black_box(1u64 + 2)
    });
}

fn main() {
    pbench::main();
}
```

### Multi-threaded benchmark

```rust
use pbench::{Bencher, ItemsCount};

#[pbench::bench(threads = [1, 2, 4, 8, 12], sample_count = 10_000)]
fn concurrent_reads(b: &Bencher<'_>) {
    let data = std::sync::Arc::new(vec![42u64; 1000]);
    b.counter(ItemsCount::new(1000));

    b.bench_refs(|| {
        for item in data.iter() {
            std::hint::black_box(item);
        }
    });
}

fn main() {
    pbench::main();
}
```

## License

MIT
