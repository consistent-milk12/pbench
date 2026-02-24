//! Benchmark for `RwLock<BTreeMap>` as a baseline concurrent ordered map.
//!
//! Uses 32-byte keys and randomized access patterns to match the
//! masstree benchmark for fair comparison.
//!
//! Run with: `cargo bench -p pbench --bench bench_rwlock_btree`

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::collections::BTreeMap;
use std::sync::RwLock;

use pbench::{Bencher, ItemsCount};

/// Key size in bytes — matches masstree bench.
const KEY_SIZE: usize = 32;

/// Multipliers for deterministic key chunk generation.
const MULTIPLIERS: [u64; 4] = [
    1,
    0x517c_c1b7_2722_0a95,
    0x9e37_79b9_7f4a_7c15,
    0xbf58_476d_1ce4_e5b9,
];

/// Generate deterministic 32-byte keys with per-chunk variation.
fn gen_keys(n: usize) -> Vec<[u8; KEY_SIZE]> {
    let mut out: Vec<[u8; KEY_SIZE]> = Vec::with_capacity(n);
    for i in 0..n {
        let mut key: [u8; KEY_SIZE] = [0u8; KEY_SIZE];
        for c in 0..4 {
            let v: u64 = (i as u64).wrapping_mul(MULTIPLIERS[c]);
            let start: usize = c * 8;
            key[start..start + 8].copy_from_slice(&v.to_be_bytes());
        }
        out.push(key);
    }
    out
}

/// Simple deterministic pseudo-random index generator.
fn random_indices(n: usize, count: usize, seed: u64) -> Vec<usize> {
    let mut state: u64 = seed;
    (0..count)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            ((state >> 32) as usize) % n
        })
        .collect()
}

/// Populate a map with `n` keys, returning (map, keys).
fn setup(n: usize) -> (RwLock<BTreeMap<[u8; KEY_SIZE], u64>>, Vec<[u8; KEY_SIZE]>) {
    let keys: Vec<[u8; KEY_SIZE]> = gen_keys(n);
    let map: RwLock<BTreeMap<[u8; KEY_SIZE], u64>> = RwLock::new(BTreeMap::new());
    {
        let mut w = map.write().unwrap();
        for (i, key) in keys.iter().enumerate() {
            w.insert(*key, i as u64);
        }
    }
    (map, keys)
}

// ---------------------------------------------------------------------------
// Insert
// ---------------------------------------------------------------------------

/// Insert 1000 keys (32-byte) into a fresh `RwLock<BTreeMap>`.
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000)]
fn insert_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    let keys: Vec<[u8; KEY_SIZE]> = gen_keys(n);
    b.counter(ItemsCount::new(n as u64));

    b.bench_values(|| {
        let map: RwLock<BTreeMap<[u8; KEY_SIZE], u64>> = RwLock::new(BTreeMap::new());
        {
            let mut w = map.write().unwrap();
            for (i, key) in keys.iter().enumerate() {
                w.insert(*key, i as u64);
            }
        }
        map
    });
}

/// Insert 10 000 keys (32-byte) into a fresh `RwLock<BTreeMap>`.
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000, max_time = 30)]
fn insert_10k(b: &Bencher<'_>) {
    let n: usize = 10_000;
    let keys: Vec<[u8; KEY_SIZE]> = gen_keys(n);
    b.counter(ItemsCount::new(n as u64));

    b.bench_values(|| {
        let map: RwLock<BTreeMap<[u8; KEY_SIZE], u64>> = RwLock::new(BTreeMap::new());
        {
            let mut w = map.write().unwrap();
            for (i, key) in keys.iter().enumerate() {
                w.insert(*key, i as u64);
            }
        }
        map
    });
}

// ---------------------------------------------------------------------------
// Overwrite
// ---------------------------------------------------------------------------

/// Overwrite 1000 existing keys in a pre-populated `RwLock<BTreeMap>`.
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000)]
fn overwrite_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    let (map, keys) = setup(n);
    b.counter(ItemsCount::new(n as u64));

    b.bench_refs(|| {
        let mut w = map.write().unwrap();
        for (i, key) in keys.iter().enumerate() {
            w.insert(*key, (i as u64).wrapping_add(1));
        }
    });
}

// ---------------------------------------------------------------------------
// Get (randomized access)
// ---------------------------------------------------------------------------

/// Random-access lookup in a 1000-entry map (hits only).
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000)]
fn get_hit_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    let (map, keys) = setup(n);
    let indices: Vec<usize> = random_indices(n, 256, 42);

    b.bench_refs(|| {
        let r = map.read().unwrap();
        for &idx in &indices {
            std::hint::black_box(r.get(&keys[idx]));
        }
    });
}

/// Random-access lookup for missing keys in a 1000-entry map.
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000)]
fn get_miss_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    let (map, _keys) = setup(n);
    let miss_keys: Vec<[u8; KEY_SIZE]> = gen_keys(n + 256)[n..].to_vec();

    b.bench_refs(|| {
        let r = map.read().unwrap();
        for key in &miss_keys {
            std::hint::black_box(r.get(key));
        }
    });
}

/// Random-access lookup in a 10 000-entry map (hits only).
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000)]
fn get_hit_10k(b: &Bencher<'_>) {
    let n: usize = 10_000;
    let (map, keys) = setup(n);
    let indices: Vec<usize> = random_indices(n, 256, 42);

    b.bench_refs(|| {
        let r = map.read().unwrap();
        for &idx in &indices {
            std::hint::black_box(r.get(&keys[idx]));
        }
    });
}

// ---------------------------------------------------------------------------
// Scan (sequential multi-key reads)
// ---------------------------------------------------------------------------

/// Scan 100 random-access gets across a 1000-entry map.
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000)]
fn scan_100(b: &Bencher<'_>) {
    let n: usize = 1000;
    let scan: usize = 100;
    let (map, keys) = setup(n);
    let indices: Vec<usize> = random_indices(n, scan, 99);
    b.counter(ItemsCount::new(scan as u64));

    b.bench_refs(|| {
        let r = map.read().unwrap();
        for &idx in &indices {
            std::hint::black_box(r.get(&keys[idx]));
        }
    });
}

// ---------------------------------------------------------------------------
// Remove
// ---------------------------------------------------------------------------

/// Remove 1000 keys from a pre-populated map.
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000)]
fn remove_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    let keys: Vec<[u8; KEY_SIZE]> = gen_keys(n);
    b.counter(ItemsCount::new(n as u64));

    b.bench_values(|| {
        let map: RwLock<BTreeMap<[u8; KEY_SIZE], u64>> = RwLock::new(BTreeMap::new());
        {
            let mut w = map.write().unwrap();
            for (i, key) in keys.iter().enumerate() {
                w.insert(*key, i as u64);
            }
        }
        {
            let mut w = map.write().unwrap();
            for key in &keys {
                w.remove(key);
            }
        }
        map
    });
}

fn main() {
    pbench::main();
}
