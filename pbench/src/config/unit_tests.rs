//! Unit tests for [`BenchOptions`] and [`ResolvedBenchOptions`].

#![expect(clippy::cast_possible_truncation)]

use std::time::Duration;

use super::*;

// --- BenchOptions::default ---

#[test]
fn defaults_all_none() {
    let opts: BenchOptions = BenchOptions::default();

    assert!(opts.sample_count.is_none());
    assert!(opts.sample_size.is_none());
    assert!(opts.min_time.is_none());
    assert!(opts.max_time.is_none());
    assert!(opts.skip_ext_time.is_none());
    assert!(opts.ignore.is_none());
    assert!(opts.threads.is_none());
}

// --- ResolvedBenchOptions::from_options ---

#[test]
fn resolve_defaults() {
    let resolved: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&BenchOptions::default());

    assert_eq!(resolved.sample_count, 1_000);
    assert!(resolved.sample_size.is_none());
    assert_eq!(resolved.min_time, Duration::from_millis(1));
    assert_eq!(resolved.max_time, Duration::from_secs(5));
    assert!(!resolved.skip_ext_time);
    assert!(!resolved.ignore);
    assert_eq!(resolved.threads, vec![1]);
}

// --- BenchOptions::overwrite ---

#[test]
fn overwrite_precedence() {
    let parent: BenchOptions = BenchOptions {
        sample_count: Some(1_000),
        ..BenchOptions::default()
    };

    let child: BenchOptions = BenchOptions {
        sample_count: Some(5_000),
        ..BenchOptions::default()
    };

    let merged: BenchOptions = child.overwrite(&parent);
    assert_eq!(merged.sample_count, Some(5_000));
}

#[test]
fn overwrite_inheritance() {
    let parent: BenchOptions = BenchOptions {
        sample_count: Some(2_000),
        min_time: Some(Duration::from_millis(10)),
        ..BenchOptions::default()
    };

    let child: BenchOptions = BenchOptions::default();

    let merged: BenchOptions = child.overwrite(&parent);
    assert_eq!(merged.sample_count, Some(2_000));
    assert_eq!(merged.min_time, Some(Duration::from_millis(10)));
}

// --- BenchOptions::overwrite (full coverage) ---

#[test]
fn overwrite_all_fields_child_wins() {
    let parent: BenchOptions = BenchOptions {
        sample_count: Some(1_000),
        sample_size: Some(10),
        min_time: Some(Duration::from_millis(1)),
        max_time: Some(Duration::from_secs(5)),
        skip_ext_time: Some(false),
        ignore: Some(false),
        threads: Some(vec![1]),
    };

    let child: BenchOptions = BenchOptions {
        sample_count: Some(5_000),
        sample_size: Some(100),
        min_time: Some(Duration::from_millis(50)),
        max_time: Some(Duration::from_secs(10)),
        skip_ext_time: Some(true),
        ignore: Some(true),
        threads: Some(vec![1, 2, 4]),
    };

    let merged: BenchOptions = child.overwrite(&parent);

    assert_eq!(merged.sample_count, Some(5_000));
    assert_eq!(merged.sample_size, Some(100));
    assert_eq!(merged.min_time, Some(Duration::from_millis(50)));
    assert_eq!(merged.max_time, Some(Duration::from_secs(10)));
    assert_eq!(merged.skip_ext_time, Some(true));
    assert_eq!(merged.ignore, Some(true));
    assert_eq!(merged.threads, Some(vec![1, 2, 4]));
}

// --- ResolvedBenchOptions::from_options (partial) ---

#[test]
fn resolve_partial() {
    let opts: BenchOptions = BenchOptions {
        sample_count: Some(500),
        ..BenchOptions::default()
    };

    let resolved: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&opts);

    assert_eq!(resolved.sample_count, 500);
    assert!(resolved.sample_size.is_none());
    assert_eq!(resolved.min_time, Duration::from_millis(1));
    assert_eq!(resolved.max_time, Duration::from_secs(5));
    assert!(!resolved.skip_ext_time);
    assert!(!resolved.ignore);
    assert_eq!(resolved.threads, vec![1]);
}

// --- threads resolution ---

#[test]
fn resolve_threads_default_is_single() {
    let resolved: ResolvedBenchOptions =
        ResolvedBenchOptions::from_options(&BenchOptions::default());
    assert_eq!(resolved.threads, vec![1]);
}

#[test]
fn resolve_threads_explicit_list() {
    let opts: BenchOptions = BenchOptions {
        threads: Some(vec![1, 2, 4]),
        ..BenchOptions::default()
    };
    let resolved: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&opts);
    assert_eq!(resolved.threads, vec![1, 2, 4]);
}

#[test]
fn resolve_threads_sorted_and_deduped() {
    let opts: BenchOptions = BenchOptions {
        threads: Some(vec![4, 2, 4, 1, 2]),
        ..BenchOptions::default()
    };
    let resolved: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&opts);
    assert_eq!(resolved.threads, vec![1, 2, 4]);
}

#[test]
fn resolve_threads_zero_expands_to_available() {
    let opts: BenchOptions = BenchOptions {
        threads: Some(vec![0]),
        ..BenchOptions::default()
    };
    let resolved: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&opts);
    let available: u32 =
        std::thread::available_parallelism().map_or(1, |n: std::num::NonZeroUsize| n.get() as u32);
    assert_eq!(resolved.threads, vec![available]);
}

#[test]
fn resolve_threads_zero_mixed_with_explicit() {
    let opts: BenchOptions = BenchOptions {
        threads: Some(vec![1, 0]),
        ..BenchOptions::default()
    };
    let resolved: ResolvedBenchOptions = ResolvedBenchOptions::from_options(&opts);
    let available: u32 =
        std::thread::available_parallelism().map_or(1, |n: std::num::NonZeroUsize| n.get() as u32);
    // Result should be sorted and deduped: [1, available] or [available] if available==1
    assert!(resolved.threads.contains(&1));
    assert!(resolved.threads.contains(&available));
}

// --- threads overwrite ---

#[test]
fn overwrite_threads_child_wins() {
    let parent: BenchOptions = BenchOptions {
        threads: Some(vec![1]),
        ..BenchOptions::default()
    };
    let child: BenchOptions = BenchOptions {
        threads: Some(vec![1, 4, 8]),
        ..BenchOptions::default()
    };
    let merged: BenchOptions = child.overwrite(&parent);
    assert_eq!(merged.threads, Some(vec![1, 4, 8]));
}

#[test]
fn overwrite_threads_inherits_from_parent() {
    let parent: BenchOptions = BenchOptions {
        threads: Some(vec![1, 2, 4]),
        ..BenchOptions::default()
    };
    let child: BenchOptions = BenchOptions::default();
    let merged: BenchOptions = child.overwrite(&parent);
    assert_eq!(merged.threads, Some(vec![1, 2, 4]));
}
