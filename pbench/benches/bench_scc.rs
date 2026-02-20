//! Benchmark example for the `scc` crate (`TreeIndex`).
//!
//! Run with: `cargo bench -p pbench --bench bench_scc`

use pbench::{Bencher, ItemsCount};
use scc::TreeIndex;

/// Insert 1000 sequential keys into a fresh `TreeIndex`.
#[pbench::bench]
fn insert_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    b.counter(ItemsCount::new(n as u64));

    b.bench_values(|| {
        let tree: TreeIndex<u64, u64> = TreeIndex::new();
        for i in 0..n as u64 {
            let _ = tree.insert_sync(i, i);
        }
        tree
    });
}

/// Insert 10 000 sequential keys into a fresh `TreeIndex`.
#[pbench::bench]
fn insert_10k(b: &Bencher<'_>) {
    let n: usize = 10_000;
    b.counter(ItemsCount::new(n as u64));

    b.bench_values(|| {
        let tree: TreeIndex<u64, u64> = TreeIndex::new();
        for i in 0..n as u64 {
            let _ = tree.insert_sync(i, i);
        }
        tree
    });
}

/// Lookup an existing key in a pre-populated tree (1000 entries).
#[pbench::bench]
fn peek_hit_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    let tree: TreeIndex<u64, u64> = TreeIndex::new();
    for i in 0..n as u64 {
        let _ = tree.insert_sync(i, i);
    }

    b.bench_refs(|| {
        std::hint::black_box(tree.peek_with(&500, |_k: &u64, v: &u64| *v));
    });
}

/// Lookup a missing key in a pre-populated tree (1000 entries).
#[pbench::bench]
fn peek_miss_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    let tree: TreeIndex<u64, u64> = TreeIndex::new();
    for i in 0..n as u64 {
        let _ = tree.insert_sync(i, i);
    }

    b.bench_refs(|| {
        std::hint::black_box(tree.peek_with(&9999, |_k: &u64, v: &u64| *v));
    });
}

/// Lookup an existing key in a larger tree (10 000 entries).
#[pbench::bench]
fn peek_hit_10k(b: &Bencher<'_>) {
    let n: usize = 10_000;
    let tree: TreeIndex<u64, u64> = TreeIndex::new();
    for i in 0..n as u64 {
        let _ = tree.insert_sync(i, i);
    }

    b.bench_refs(|| {
        std::hint::black_box(tree.peek_with(&5000, |_k: &u64, v: &u64| *v));
    });
}

/// Remove 1000 keys from a pre-populated tree.
#[pbench::bench]
fn remove_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    b.counter(ItemsCount::new(n as u64));

    b.bench_values(|| {
        let tree: TreeIndex<u64, u64> = TreeIndex::new();
        for i in 0..n as u64 {
            let _ = tree.insert_sync(i, i);
        }
        for i in 0..n as u64 {
            tree.remove_sync(&i);
        }
        tree
    });
}

fn main() {
    pbench::main();
}
