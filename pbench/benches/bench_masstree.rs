//! Benchmark example for the `masstree` crate (`MassTree`).
//!
//! Run with: `cargo bench -p pbench --bench bench_masstree`

use masstree::MassTree;
use pbench::{Bencher, ItemsCount};

/// Insert 1000 sequential keys into a fresh `MassTree`.
#[pbench::bench]
fn insert_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    b.counter(ItemsCount::new(n as u64));

    b.bench_values(|| {
        let tree: MassTree<u64> = MassTree::new();
        for i in 0..n as u64 {
            tree.insert(&i.to_be_bytes(), i);
        }
        tree
    });
}

/// Insert 10 000 sequential keys into a fresh `MassTree`.
#[pbench::bench]
fn insert_10k(b: &Bencher<'_>) {
    let n: usize = 10_000;
    b.counter(ItemsCount::new(n as u64));

    b.bench_values(|| {
        let tree: MassTree<u64> = MassTree::new();
        for i in 0..n as u64 {
            tree.insert(&i.to_be_bytes(), i);
        }
        tree
    });
}

/// Lookup an existing key in a pre-populated tree (1000 entries).
#[pbench::bench]
fn get_hit_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    let tree: MassTree<u64> = MassTree::new();
    for i in 0..n as u64 {
        tree.insert(&i.to_be_bytes(), i);
    }

    let target: [u8; 8] = 500_u64.to_be_bytes();

    b.bench_refs(|| {
        std::hint::black_box(tree.get(&target));
    });
}

/// Lookup a missing key in a pre-populated tree (1000 entries).
#[pbench::bench]
fn get_miss_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    let tree: MassTree<u64> = MassTree::new();
    for i in 0..n as u64 {
        tree.insert(&i.to_be_bytes(), i);
    }

    let target: [u8; 8] = 9999_u64.to_be_bytes();

    b.bench_refs(|| {
        std::hint::black_box(tree.get(&target));
    });
}

/// Lookup an existing key in a larger tree (10 000 entries).
#[pbench::bench]
fn get_hit_10k(b: &Bencher<'_>) {
    let n: usize = 10_000;
    let tree: MassTree<u64> = MassTree::new();
    for i in 0..n as u64 {
        tree.insert(&i.to_be_bytes(), i);
    }

    let target: [u8; 8] = 5000_u64.to_be_bytes();

    b.bench_refs(|| {
        std::hint::black_box(tree.get(&target));
    });
}

/// Remove 1000 keys from a pre-populated tree.
#[pbench::bench]
fn remove_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    b.counter(ItemsCount::new(n as u64));

    b.bench_values(|| {
        let tree: MassTree<u64> = MassTree::new();
        for i in 0..n as u64 {
            tree.insert(&i.to_be_bytes(), i);
        }
        for i in 0..n as u64 {
            let _ = tree.remove(&i.to_be_bytes());
        }
        tree
    });
}

fn main() {
    pbench::main();
}
