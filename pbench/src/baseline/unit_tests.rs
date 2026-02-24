//! Tests for baseline save/load functionality.

use tempfile::TempDir;

use super::{BaselineEntry, BaselineError, BaselineStore};

use std::fs as StdFs;
use std::path::PathBuf;

use crate::stats::{PercentileSet, PercentileStats};
use crate::time::fine_duration::FineDuration;

/// Helper: build a [`PercentileStats`] with all durations set to the given
/// picosecond value.
fn make_stats(picos: u128) -> PercentileStats {
    let dur: FineDuration = FineDuration { picos };
    PercentileStats {
        sample_count: 100,
        iter_count: 10_000,
        min: dur,
        max: dur,
        mean: dur,
        std_dev: FineDuration { picos: 0 },
        percentiles: PercentileSet {
            p50: dur,
            p95: dur,
            p99: dur,
            p99_9: dur,
            p99_99: dur,
        },
    }
}

// --- save_and_load_baseline ---

#[test]
fn save_and_load_baseline() {
    let dir: TempDir = tempfile::tempdir().expect("create temp dir");
    let stats: PercentileStats = make_stats(1_000_000);
    let results: Vec<(&str, u32, &PercentileStats)> = vec![("bench_a", 1, &stats)];

    BaselineStore::save(dir.path(), "test_baseline", &results).expect("save");

    let loaded: Vec<BaselineEntry> =
        BaselineStore::load(dir.path(), "test_baseline").expect("load");

    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "bench_a");
    assert_eq!(loaded[0].thread_count, 1);
    assert_eq!(loaded[0].stats.sample_count, 100);
    assert_eq!(loaded[0].stats.iter_count, 10_000);
    assert_eq!(loaded[0].stats.p50_picos, 1_000_000);

    // Verify round-trip through to_percentile_stats
    let restored: PercentileStats = loaded[0].stats.to_percentile_stats();
    assert_eq!(restored.percentiles.p50.picos, 1_000_000);
}

// --- load_incompatible_version ---

#[test]
fn load_incompatible_version() {
    let dir: TempDir = tempfile::tempdir().expect("create temp dir");
    let json: &str = r#"{"schema_version": 99, "results": []}"#;
    let path: PathBuf = dir.path().join("bad_version.json");
    StdFs::write(&path, json).expect("write");

    let err: BaselineError = BaselineStore::load(dir.path(), "bad_version").unwrap_err();

    match err {
        BaselineError::IncompatibleVersion { found, expected } => {
            assert_eq!(found, 99);
            assert_eq!(expected, 2);
        }
        other => panic!("expected IncompatibleVersion, got: {other}"),
    }
}

// --- load_malformed_json ---

#[test]
fn load_malformed_json() {
    let dir: TempDir = tempfile::tempdir().expect("create temp dir");
    let path: PathBuf = dir.path().join("malformed.json");
    StdFs::write(&path, "not valid json {{{").expect("write");

    let err: BaselineError = BaselineStore::load(dir.path(), "malformed").unwrap_err();

    assert!(
        matches!(err, BaselineError::Parse(_)),
        "expected Parse error, got: {err}"
    );
}

// --- schema_version_present_in_output ---

#[test]
fn schema_version_present_in_output() {
    let dir: TempDir = tempfile::tempdir().expect("create temp dir");
    let results: Vec<(&str, u32, &PercentileStats)> = vec![];

    BaselineStore::save(dir.path(), "empty", &results).expect("save");

    let contents: String = StdFs::read_to_string(dir.path().join("empty.json")).expect("read");

    assert!(
        contents.contains("\"schema_version\": 2"),
        "schema_version missing from output: {contents}"
    );
}

// --- save_and_load_with_thread_count ---

#[test]
fn save_and_load_with_thread_count() {
    let dir: TempDir = tempfile::tempdir().expect("create temp dir");
    let stats: PercentileStats = make_stats(500_000);
    let results: Vec<(&str, u32, &PercentileStats)> = vec![("bench_x", 4, &stats)];

    BaselineStore::save(dir.path(), "threaded", &results).expect("save");

    let loaded: Vec<BaselineEntry> = BaselineStore::load(dir.path(), "threaded").expect("load");

    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "bench_x");
    assert_eq!(loaded[0].thread_count, 4);
    assert_eq!(loaded[0].stats.p50_picos, 500_000);
}

// --- v1_migration_defaults_thread_count_to_one ---

#[test]
fn v1_migration_defaults_thread_count_to_one() {
    let dir: TempDir = tempfile::tempdir().expect("create temp dir");

    // Write a v1-style JSON file (no thread_count field in entries).
    let v1_json: &str = r#"{
        "schema_version": 1,
        "results": [
            {
                "name": "old_bench",
                "stats": {
                    "sample_count": 50,
                    "iter_count": 5000,
                    "min_picos": 100,
                    "max_picos": 200,
                    "mean_picos": 150,
                    "std_dev_picos": 10,
                    "p50_picos": 140,
                    "p95_picos": 180,
                    "p99_picos": 190,
                    "p99_9_picos": 195,
                    "p99_99_picos": 200
                }
            }
        ]
    }"#;

    let path: PathBuf = dir.path().join("v1_baseline.json");
    StdFs::write(&path, v1_json).expect("write v1 file");

    let loaded: Vec<BaselineEntry> =
        BaselineStore::load(dir.path(), "v1_baseline").expect("load v1");

    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "old_bench");
    // v1 entries default to thread_count = 1
    assert_eq!(loaded[0].thread_count, 1);
    assert_eq!(loaded[0].stats.sample_count, 50);
    assert_eq!(loaded[0].stats.p50_picos, 140);
}

// --- multi_thread_baseline_roundtrip ---

#[test]
fn multi_thread_baseline_roundtrip() {
    let dir: TempDir = tempfile::tempdir().expect("create temp dir");
    let stats: PercentileStats = make_stats(1_000_000);
    let results: Vec<(&str, u32, &PercentileStats)> =
        vec![("bench_a", 1, &stats), ("bench_a", 4, &stats)];

    BaselineStore::save(dir.path(), "multi", &results).expect("save");

    let loaded: Vec<BaselineEntry> = BaselineStore::load(dir.path(), "multi").expect("load");

    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].name, "bench_a");
    assert_eq!(loaded[0].thread_count, 1);
    assert_eq!(loaded[1].name, "bench_a");
    assert_eq!(loaded[1].thread_count, 4);
}
