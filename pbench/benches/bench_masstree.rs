//! Benchmark for the `masstree` crate (`MassTree15Inline`).
//!
//! Uses `_with_guard` API to amortize guard creation, deterministic
//! multi-chunk keys to exercise trie depth, and randomized access
//! patterns to avoid sequential cache friendliness.
//!
//! Run with: `cargo bench -p pbench --bench bench_masstree`

use std::sync::atomic::{AtomicUsize, Ordering};

use masstree::MassTree15Inline as MassTree;
use pbench::{Bencher, ItemsCount};

/// Key size in bytes — 32 bytes = 4 trie layers in masstree.
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

/// Populate a tree with `n` keys, returning (tree, keys).
fn setup(n: usize) -> (MassTree<u64>, Vec<[u8; KEY_SIZE]>) {
    let keys: Vec<[u8; KEY_SIZE]> = gen_keys(n);
    let tree: MassTree<u64> = MassTree::new();

    {
        let guard = tree.guard();
        for (i, key) in keys.iter().enumerate() {
            let _ = tree.insert_with_guard(key, i as u64, &guard);
        }
    }

    (tree, keys)
}

// ---------------------------------------------------------------------------
// Insert
// ---------------------------------------------------------------------------

/// Insert 1000 keys (32-byte, multi-chunk) into a fresh tree.
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000)]
fn insert_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    let keys: Vec<[u8; KEY_SIZE]> = gen_keys(n);
    b.counter(ItemsCount::new(n as u64));

    b.bench_values(|| {
        let tree: MassTree<u64> = MassTree::new();
        {
            let guard = tree.guard();

            for (i, key) in keys.iter().enumerate() {
                let _ = tree.insert_with_guard(key, i as u64, &guard);
            }
        }
        tree
    });
}

/// Insert 10 000 keys (32-byte, multi-chunk) into a fresh tree.
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000, max_time = 30)]
fn insert_10k(b: &Bencher<'_>) {
    let n: usize = 10_000;
    let keys: Vec<[u8; KEY_SIZE]> = gen_keys(n);
    b.counter(ItemsCount::new(n as u64));

    b.bench_values(|| {
        let tree: MassTree<u64> = MassTree::new();
        {
            let guard = tree.guard();

            for (i, key) in keys.iter().enumerate() {
                let _ = tree.insert_with_guard(key, i as u64, &guard);
            }
        }
        tree
    });
}

// ---------------------------------------------------------------------------
// Overwrite
// ---------------------------------------------------------------------------

/// Overwrite 1000 existing keys in a pre-populated tree.
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000)]
fn overwrite_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    let (tree, keys) = setup(n);
    b.counter(ItemsCount::new(n as u64));

    b.bench_refs(|| {
        let guard = tree.guard();

        for (i, key) in keys.iter().enumerate() {
            let _ = tree.insert_with_guard(key, (i as u64).wrapping_add(1), &guard);
        }
    });
}

// ---------------------------------------------------------------------------
// Get (randomized access)
// ---------------------------------------------------------------------------

/// Random-access lookup in a 1000-entry tree (hits only).
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000)]
fn get_hit_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    let (tree, keys) = setup(n);
    let indices: Vec<usize> = random_indices(n, 256, 42);

    b.bench_refs(|| {
        let guard = tree.guard();

        for &idx in &indices {
            std::hint::black_box(tree.get_with_guard(&keys[idx], &guard));
        }
    });
}

/// Random-access lookup for missing keys in a 1000-entry tree.
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000)]
fn get_miss_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    let (tree, _keys) = setup(n);
    // Generate keys that don't exist in the tree (offset by n).
    let miss_keys: Vec<[u8; KEY_SIZE]> = gen_keys(n + 256)[n..].to_vec();

    b.bench_refs(|| {
        let guard = tree.guard();
        for key in &miss_keys {
            std::hint::black_box(tree.get_with_guard(key, &guard));
        }
    });
}

/// Random-access lookup in a 10 000-entry tree (hits only).
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000)]
fn get_hit_10k(b: &Bencher<'_>) {
    let n: usize = 10_000;
    let (tree, keys) = setup(n);
    let indices: Vec<usize> = random_indices(n, 256, 42);

    b.bench_refs(|| {
        let guard = tree.guard();
        for &idx in &indices {
            std::hint::black_box(tree.get_with_guard(&keys[idx], &guard));
        }
    });
}

// ---------------------------------------------------------------------------
// Scan (sequential multi-key reads)
// ---------------------------------------------------------------------------

/// Scan 100 random-access gets across a 1000-entry tree.
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000)]
fn scan_100(b: &Bencher<'_>) {
    let n: usize = 1000;
    let scan: usize = 100;
    let (tree, keys) = setup(n);
    let indices: Vec<usize> = random_indices(n, scan, 99);
    b.counter(ItemsCount::new(scan as u64));

    b.bench_refs(|| {
        let guard = tree.guard();
        for &idx in &indices {
            std::hint::black_box(tree.get_with_guard(&keys[idx], &guard));
        }
    });
}

/// Prefix scan over a 10 000-entry tree with shared-prefix keys.
///
/// Uses 10 prefix buckets (~1000 keys per prefix). Each sample scans
/// one random prefix and visits all matching keys.
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000)]
fn prefix_scan_10k(b: &Bencher<'_>) {
    let n: usize = 10_000;
    let prefix_buckets: u64 = 10;

    // Generate keys where the first 8 bytes are drawn from 10 buckets.
    let keys: Vec<[u8; KEY_SIZE]> = {
        let mut out: Vec<[u8; KEY_SIZE]> = Vec::with_capacity(n);

        for i in 0..n {
            let mut key: [u8; KEY_SIZE] = [0u8; KEY_SIZE];

            // First chunk: bucket prefix
            let prefix: [u8; 8] = ((i as u64) % prefix_buckets).to_be_bytes();
            key[0..8].copy_from_slice(&prefix);

            // Remaining chunks: unique per key
            for c in 1..4 {
                let v: u64 = (i as u64).wrapping_mul(MULTIPLIERS[c]);
                let start: usize = c * 8;

                key[start..start + 8].copy_from_slice(&v.to_be_bytes());
            }

            out.push(key);
        }

        out
    };

    let tree: MassTree<u64> = MassTree::new();
    {
        let guard = tree.guard();

        for (i, key) in keys.iter().enumerate() {
            let _ = tree.insert_with_guard(key, i as u64, &guard);
        }
    }

    // Pre-generate random prefix bytes to scan.
    let prefixes: Vec<[u8; 8]> = {
        let indices: Vec<usize> = random_indices(prefix_buckets as usize, 64, 77);
        indices
            .into_iter()
            .map(|idx: usize| (idx as u64).to_be_bytes())
            .collect()
    };

    let prefix_idx: AtomicUsize = AtomicUsize::new(0);

    b.bench_refs(|| {
        let guard = tree.guard();
        let idx: usize = prefix_idx.fetch_add(1, Ordering::Relaxed);
        let prefix: &[u8; 8] = &prefixes[idx % prefixes.len()];
        let mut sum: u64 = 0;

        tree.scan_prefix(
            prefix,
            |_key: &[u8], v: u64| {
                sum = sum.wrapping_add(v);
                true
            },
            &guard,
        );

        std::hint::black_box(sum);
    });
}

// ---------------------------------------------------------------------------
// Remove
// ---------------------------------------------------------------------------

/// Remove 1000 keys from a pre-populated tree.
#[pbench::bench(threads = [1, 2, 4, 6, 8, 12], sample_count = 10_000)]
fn remove_1k(b: &Bencher<'_>) {
    let n: usize = 1000;
    let keys: Vec<[u8; KEY_SIZE]> = gen_keys(n);
    b.counter(ItemsCount::new(n as u64));

    b.bench_values(|| {
        let tree: MassTree<u64> = MassTree::new();
        {
            let guard = tree.guard();

            for (i, key) in keys.iter().enumerate() {
                let _ = tree.insert_with_guard(key, i as u64, &guard);
            }
        }

        {
            let guard = tree.guard();

            for key in &keys {
                let _ = tree.remove_with_guard(key, &guard);
            }
        }

        tree
    });
}

fn main() {
    pbench::main();
}
