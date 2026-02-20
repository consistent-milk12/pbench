//! Tests for regression detection.

#![allow(clippy::float_cmp)]

use super::{RegressionChecker, RegressionResult};
use crate::stats::{PercentileSet, PercentileStats};
use crate::time::fine_duration::FineDuration;

/// Helper: build stats with specific percentile pico values.
fn make_stats_detailed(
    p50: u128,
    p95: u128,
    p99: u128,
    p99_9: u128,
    p99_99: u128,
    mean: u128,
) -> PercentileStats {
    PercentileStats {
        sample_count: 100,
        iter_count: 10_000,
        min: FineDuration { picos: p50 },
        max: FineDuration { picos: p99_99 },
        mean: FineDuration { picos: mean },
        std_dev: FineDuration { picos: 0 },
        percentiles: PercentileSet {
            p50: FineDuration { picos: p50 },
            p95: FineDuration { picos: p95 },
            p99: FineDuration { picos: p99 },
            p99_9: FineDuration { picos: p99_9 },
            p99_99: FineDuration { picos: p99_99 },
        },
    }
}

/// Helper: build uniform stats (all percentiles same value).
fn make_uniform_stats(picos: u128) -> PercentileStats {
    make_stats_detailed(picos, picos, picos, picos, picos, picos)
}

// --- no_regression_within_threshold ---

#[test]
fn no_regression_within_threshold() {
    let old: PercentileStats = make_uniform_stats(1_000_000);
    // 3% increase
    let new: PercentileStats = make_uniform_stats(1_030_000);

    let result: RegressionResult = RegressionChecker::check(&old, &new, 5.0);

    assert!(!result.is_regression);
    assert!(!result.p50_regression);
    assert!(!result.p99_regression);
    // Delta should be ~3%
    assert!(
        (result.p50_delta_pct - 3.0).abs() < 0.1,
        "expected ~3%, got {}",
        result.p50_delta_pct
    );
}

// --- regression_above_threshold ---

#[test]
fn regression_above_threshold() {
    let old: PercentileStats = make_uniform_stats(1_000_000);
    // 20% increase
    let new: PercentileStats = make_uniform_stats(1_200_000);

    let result: RegressionResult = RegressionChecker::check(&old, &new, 5.0);

    assert!(result.is_regression);
    assert!(result.p50_regression);
    assert!(result.p95_regression);
    assert!(result.p99_regression);
    assert!(result.p99_9_regression);
    assert!(result.p99_99_regression);
    // Delta should be ~20%
    assert!(
        (result.p50_delta_pct - 20.0).abs() < 0.1,
        "expected ~20%, got {}",
        result.p50_delta_pct
    );
}

// --- per_percentile_deltas ---

#[test]
fn per_percentile_deltas() {
    let old: PercentileStats = make_stats_detailed(
        1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000,
    );
    // p50 at +5% (at threshold edge), p99 at +10% (above threshold)
    let new: PercentileStats = make_stats_detailed(
        1_050_000, 1_050_000, 1_100_000, 1_100_000, 1_100_000, 1_050_000,
    );

    let result: RegressionResult = RegressionChecker::check(&old, &new, 5.0);

    // p50 at exactly 5%: delta > threshold is strictly greater, so 5.0 > 5.0 is false
    assert!(!result.p50_regression);
    assert!(!result.p95_regression);
    // p99 at 10%: above 5% threshold
    assert!(result.p99_regression);
    assert!(result.p99_9_regression);
    assert!(result.p99_99_regression);
    // Overall regression because p99+ exceeds threshold
    assert!(result.is_regression);
}

// --- regression_zero_baseline ---

#[test]
fn regression_zero_baseline() {
    let old: PercentileStats = make_uniform_stats(0);
    let new: PercentileStats = make_uniform_stats(1_000_000);

    let result: RegressionResult = RegressionChecker::check(&old, &new, 5.0);

    // Zero baseline -> delta is 0.0, no regression
    assert!(!result.is_regression);
    assert_eq!(result.p50_delta_pct, 0.0);
    assert_eq!(result.p99_delta_pct, 0.0);
    assert_eq!(result.mean_delta_pct, 0.0);
}

// --- improvement_not_flagged ---

#[test]
fn improvement_not_flagged() {
    let old: PercentileStats = make_uniform_stats(1_000_000);
    // 20% faster (decrease)
    let new: PercentileStats = make_uniform_stats(800_000);

    let result: RegressionResult = RegressionChecker::check(&old, &new, 5.0);

    assert!(!result.is_regression);
    assert!(!result.p50_regression);
    // Delta should be ~-20%
    assert!(
        (result.p50_delta_pct - (-20.0)).abs() < 0.1,
        "expected ~-20%, got {}",
        result.p50_delta_pct
    );
}

// --- mean_delta_computed ---

#[test]
fn mean_delta_computed() {
    let old: PercentileStats = make_stats_detailed(
        1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000, 500_000,
    );
    let new: PercentileStats = make_stats_detailed(
        1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000, 600_000,
    );

    let result: RegressionResult = RegressionChecker::check(&old, &new, 5.0);

    // Percentiles unchanged
    assert!(!result.is_regression);
    assert_eq!(result.p50_delta_pct, 0.0);
    // Mean changed by 20%
    assert!(
        (result.mean_delta_pct - 20.0).abs() < 0.1,
        "expected ~20%, got {}",
        result.mean_delta_pct
    );
}
