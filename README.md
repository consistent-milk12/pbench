# pbench

Percentile-focused benchmarking for Rust. Reports p50/p95/p99/p99.9/p99.99 instead of just min/max/median/mean.

Built from scratch, heavily inspired by [divan](https://github.com/nvzqz/divan)'s architecture.

NOTE: Currently only supports single-threaded benches. Working on this for my [masstree](https://github.com/consistent-milk12/masstree) project, so I will eventually add multi-thread support.

## Quick Start

Check the pbench/benches/ in repo.

```rust
use pbench::Bencher;

#[pbench::bench]
fn my_benchmark(b: &Bencher<'_>) {
    b.bench_refs(|| {
        // code to benchmark
        std::hint::black_box(1u64 + 2)
    });
}

fn main() {
    pbench::main();
}
```

## License

MIT
