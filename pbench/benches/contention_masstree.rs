//! Multi-threaded contention benchmarks for `masstree::MassTree`.
//!
//! Run with: `cargo bench -p pbench --bench contention_masstree`

use std::thread;

use masstree::MassTree;
use pbench::{Bencher, ItemsCount};

const THREADS: usize = 8;
const OPS_PER_THREAD: usize = 10_000;

/// 8 threads each reading 10k keys from a pre-populated tree.
#[pbench::bench(sample_count = 10_000, max_time = 30)]
fn read_heavy(b: &Bencher<'_>) {
    let total_ops: u64 = (THREADS * OPS_PER_THREAD) as u64;
    b.counter(ItemsCount::new(total_ops));

    let tree: MassTree<u64> = MassTree::new();
    for i in 0..OPS_PER_THREAD as u64 {
        tree.insert(&i.to_be_bytes(), i);
    }

    b.bench_refs(|| {
        thread::scope(|s| {
            for _ in 0..THREADS {
                let tree_ref: &MassTree<u64> = &tree;
                s.spawn(move || {
                    for i in 0..OPS_PER_THREAD as u64 {
                        std::hint::black_box(tree_ref.get(&i.to_be_bytes()));
                    }
                });
            }
        });
    });
}

/// 8 threads each inserting 10k keys into disjoint ranges.
#[pbench::bench(sample_count = 10_000, max_time = 40)]
fn write_heavy(b: &Bencher<'_>) {
    let total_ops: u64 = (THREADS * OPS_PER_THREAD) as u64;
    b.counter(ItemsCount::new(total_ops));

    b.bench_values(|| {
        let tree: MassTree<u64> = MassTree::new();
        thread::scope(|s| {
            for t in 0..THREADS {
                let tree_ref: &MassTree<u64> = &tree;
                let base: u64 = (t * OPS_PER_THREAD) as u64;
                s.spawn(move || {
                    for i in 0..OPS_PER_THREAD as u64 {
                        let key: u64 = base + i;
                        tree_ref.insert(&key.to_be_bytes(), key);
                    }
                });
            }
        });
        tree
    });
}

/// 8 threads: half reading, half writing concurrently.
///
/// Writers overwrite existing keys so the tree size stays constant across
/// samples, avoiding unbounded growth that would skew later measurements.
#[pbench::bench(sample_count = 10_000, max_time = 30)]
fn mixed_rw(b: &Bencher<'_>) {
    let total_ops: u64 = (THREADS * OPS_PER_THREAD) as u64;
    b.counter(ItemsCount::new(total_ops));

    let tree: MassTree<u64> = MassTree::new();
    for i in 0..OPS_PER_THREAD as u64 {
        tree.insert(&i.to_be_bytes(), i);
    }

    b.bench_refs(|| {
        thread::scope(|s| {
            for t in 0..THREADS {
                let tree_ref: &MassTree<u64> = &tree;
                let is_writer: bool = t >= THREADS / 2;
                s.spawn(move || {
                    if is_writer {
                        for i in 0..OPS_PER_THREAD as u64 {
                            tree_ref.insert(&i.to_be_bytes(), i);
                        }
                    } else {
                        for i in 0..OPS_PER_THREAD as u64 {
                            std::hint::black_box(tree_ref.get(&i.to_be_bytes()));
                        }
                    }
                });
            }
        });
    });
}

fn main() {
    pbench::main();
}
