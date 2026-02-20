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
    let results: Vec<(&str, &PercentileStats)> = vec![("bench_a", &stats)];

    BaselineStore::save(dir.path(), "test_baseline", &results).expect("save");

    let loaded: Vec<BaselineEntry> =
        BaselineStore::load(dir.path(), "test_baseline").expect("load");

    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "bench_a");
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
            assert_eq!(expected, 1);
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
    let results: Vec<(&str, &PercentileStats)> = vec![];

    BaselineStore::save(dir.path(), "empty", &results).expect("save");

    let contents: String = StdFs::read_to_string(dir.path().join("empty.json")).expect("read");

    assert!(
        contents.contains("\"schema_version\": 1"),
        "schema_version missing from output: {contents}"
    );
}
