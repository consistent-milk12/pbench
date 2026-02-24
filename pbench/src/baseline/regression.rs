//! Regression detection by comparing percentile statistics.
//!
//! Computes per-percentile deltas between old (baseline) and new benchmark
//! runs, flagging regressions when the delta exceeds a threshold.

use crate::stats::PercentileStats;

#[cfg(test)]
mod unit_tests;

/// Result of a regression check between two benchmark runs.
#[derive(Clone, Debug)]
#[allow(
    dead_code,
    reason = "Fields consumed by compare_baseline text output and future table integration"
)]
#[expect(clippy::struct_excessive_bools)]
pub(crate) struct RegressionResult {
    /// Whether any percentile exceeds the threshold.
    pub is_regression: bool,

    /// Percentage change at p50.
    pub p50_delta_pct: f64,

    /// Percentage change at p95.
    pub p95_delta_pct: f64,

    /// Percentage change at p99.
    pub p99_delta_pct: f64,

    /// Percentage change at p99.9.
    pub p99_9_delta_pct: f64,

    /// Percentage change at p99.99.
    pub p99_99_delta_pct: f64,

    /// Percentage change in mean.
    pub mean_delta_pct: f64,

    /// Whether p50 individually regressed.
    pub p50_regression: bool,

    /// Whether p95 individually regressed.
    pub p95_regression: bool,

    /// Whether p99 individually regressed.
    pub p99_regression: bool,

    /// Whether p99.9 individually regressed.
    pub p99_9_regression: bool,

    /// Whether p99.99 individually regressed.
    pub p99_99_regression: bool,
}

/// Regression detection logic.
pub(crate) struct RegressionChecker;

impl RegressionChecker {
    /// Compare old and new statistics, returning per-percentile deltas.
    ///
    /// Delta formula: `((new - old) / old) * 100.0`.
    /// If `old_picos == 0`, the delta is `0.0` (avoids division by zero).
    /// A percentile is flagged as a regression when `delta > threshold_pct`.
    #[expect(
        clippy::similar_names,
        reason = "p99_9 and p99_99 are domain names for percentiles, not confusable"
    )]
    #[must_use]
    pub(crate) fn check(
        old: &PercentileStats,
        new: &PercentileStats,
        threshold_pct: f64,
    ) -> RegressionResult {
        let p50_delta_pct: f64 =
            Self::delta_pct(old.percentiles.p50.picos, new.percentiles.p50.picos);
        let p95_delta_pct: f64 =
            Self::delta_pct(old.percentiles.p95.picos, new.percentiles.p95.picos);
        let p99_delta_pct: f64 =
            Self::delta_pct(old.percentiles.p99.picos, new.percentiles.p99.picos);
        let p99_9_delta_pct: f64 =
            Self::delta_pct(old.percentiles.p99_9.picos, new.percentiles.p99_9.picos);
        let p99_99_delta_pct: f64 =
            Self::delta_pct(old.percentiles.p99_99.picos, new.percentiles.p99_99.picos);
        let mean_delta_pct: f64 = Self::delta_pct(old.mean.picos, new.mean.picos);

        let p50_regression: bool = p50_delta_pct > threshold_pct;
        let p95_regression: bool = p95_delta_pct > threshold_pct;
        let p99_regression: bool = p99_delta_pct > threshold_pct;
        let p99_9_regression: bool = p99_9_delta_pct > threshold_pct;
        let p99_99_regression: bool = p99_99_delta_pct > threshold_pct;

        let is_regression: bool = p50_regression
            || p95_regression
            || p99_regression
            || p99_9_regression
            || p99_99_regression;

        RegressionResult {
            is_regression,
            p50_delta_pct,
            p95_delta_pct,
            p99_delta_pct,
            p99_9_delta_pct,
            p99_99_delta_pct,
            mean_delta_pct,
            p50_regression,
            p95_regression,
            p99_regression,
            p99_9_regression,
            p99_99_regression,
        }
    }

    /// Compute percentage delta between old and new pico values.
    ///
    /// Returns `0.0` if `old == 0` to avoid division by zero.
    #[expect(
        clippy::cast_precision_loss,
        reason = "u128 pico values fit f64 for percentage computation"
    )]
    fn delta_pct(old: u128, new: u128) -> f64 {
        if old == 0 {
            return 0.0;
        }

        let old_f: f64 = old as f64;
        let new_f: f64 = new as f64;

        ((new_f - old_f) / old_f) * 100.0
    }
}
