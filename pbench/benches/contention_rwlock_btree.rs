//! Multi-threaded contention benchmarks for `RwLock<BTreeMap>`.
//!
//! Run with: `cargo bench -p pbench --bench contention_rwlock_btree`

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::collections::BTreeMap;
use std::sync::RwLock;
use std::thread;

use pbench::{Bencher, ItemsCount};

const THREADS: usize = 8;
const OPS_PER_THREAD: usize = 10_000;

/// 8 threads each reading 10k keys from a pre-populated map.
#[pbench::bench(sample_count = 10_000, max_time = 30)]
fn read_heavy(b: &Bencher<'_>) {
    let total_ops: u64 = (THREADS * OPS_PER_THREAD) as u64;
    b.counter(ItemsCount::new(total_ops));

    let map: RwLock<BTreeMap<u64, u64>> = RwLock::new(BTreeMap::new());
    for i in 0..OPS_PER_THREAD as u64 {
        map.write().unwrap().insert(i, i);
    }

    b.bench_refs(|| {
        thread::scope(|s| {
            for _ in 0..THREADS {
                let map_ref: &RwLock<BTreeMap<u64, u64>> = &map;
                s.spawn(move || {
                    for i in 0..OPS_PER_THREAD as u64 {
                        std::hint::black_box(map_ref.read().unwrap().get(&i));
                    }
                });
            }
        });
    });
}

/// 8 threads each inserting 10k keys into disjoint ranges.
#[pbench::bench(sample_count = 10_000, max_time = 30)]
fn write_heavy(b: &Bencher<'_>) {
    let total_ops: u64 = (THREADS * OPS_PER_THREAD) as u64;
    b.counter(ItemsCount::new(total_ops));

    b.bench_values(|| {
        let map: RwLock<BTreeMap<u64, u64>> = RwLock::new(BTreeMap::new());
        thread::scope(|s| {
            for t in 0..THREADS {
                let map_ref: &RwLock<BTreeMap<u64, u64>> = &map;
                let base: u64 = (t * OPS_PER_THREAD) as u64;
                s.spawn(move || {
                    for i in 0..OPS_PER_THREAD as u64 {
                        let key: u64 = base + i;
                        map_ref.write().unwrap().insert(key, key);
                    }
                });
            }
        });
        map
    });
}

/// 8 threads: half reading, half writing concurrently.
///
/// Writers overwrite existing keys so the map size stays constant across
/// samples, avoiding unbounded growth that would skew later measurements.
#[pbench::bench(sample_count = 10_000, max_time = 30)]
fn mixed_rw(b: &Bencher<'_>) {
    let total_ops: u64 = (THREADS * OPS_PER_THREAD) as u64;
    b.counter(ItemsCount::new(total_ops));

    let map: RwLock<BTreeMap<u64, u64>> = RwLock::new(BTreeMap::new());
    for i in 0..OPS_PER_THREAD as u64 {
        map.write().unwrap().insert(i, i);
    }

    b.bench_refs(|| {
        thread::scope(|s| {
            for t in 0..THREADS {
                let map_ref: &RwLock<BTreeMap<u64, u64>> = &map;
                let is_writer: bool = t >= THREADS / 2;
                s.spawn(move || {
                    if is_writer {
                        for i in 0..OPS_PER_THREAD as u64 {
                            map_ref.write().unwrap().insert(i, i);
                        }
                    } else {
                        for i in 0..OPS_PER_THREAD as u64 {
                            std::hint::black_box(map_ref.read().unwrap().get(&i));
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
