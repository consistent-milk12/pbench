//! JSON output renderer for benchmark results.
//!
//! Produces a pretty-printed JSON object containing all benchmark statistics
//! with both raw picosecond values and human-readable display strings.
//! Gated behind the `json` feature flag.

use serde_json::{self as SJSON, json};

use crate::stats::PercentileStats;

/// JSON output renderer.
pub struct JsonRenderer;

impl JsonRenderer {
    /// Render benchmark results as a pretty-printed JSON string.
    ///
    /// Each benchmark entry includes raw picosecond values (`*_picos`) for
    /// machine consumption and human-readable display strings (`*_display`)
    /// for convenience.
    ///
    /// # Panics
    ///
    /// Panics if `SJSON` fails to serialize the constructed value
    /// (should never happen with well-formed `SJSON::Value`).
    #[expect(
        clippy::cast_possible_truncation,
        reason = "u128 picos → u64: benchmark durations never exceed u64::MAX"
    )]
    #[must_use]
    pub(crate) fn render(results: &[(&str, &PercentileStats)]) -> String {
        let benchmarks: Vec<SJSON::Value> = results
            .iter()
            .map(|&(name, stats): &(&str, &PercentileStats)| {
                json!({
                    "name": name,
                    "sample_count": stats.sample_count,
                    "iter_count": stats.iter_count,
                    "min_picos": stats.min.picos as u64,
                    "min_display": stats.min.to_string(),
                    "max_picos": stats.max.picos as u64,
                    "max_display": stats.max.to_string(),
                    "mean_picos": stats.mean.picos as u64,
                    "mean_display": stats.mean.to_string(),
                    "std_dev_picos": stats.std_dev.picos as u64,
                    "std_dev_display": stats.std_dev.to_string(),
                    "percentiles": {
                        "p50_picos": stats.percentiles.p50.picos as u64,
                        "p50_display": stats.percentiles.p50.to_string(),
                        "p95_picos": stats.percentiles.p95.picos as u64,
                        "p95_display": stats.percentiles.p95.to_string(),
                        "p99_picos": stats.percentiles.p99.picos as u64,
                        "p99_display": stats.percentiles.p99.to_string(),
                        "p99_9_picos": stats.percentiles.p99_9.picos as u64,
                        "p99_9_display": stats.percentiles.p99_9.to_string(),
                        "p99_99_picos": stats.percentiles.p99_99.picos as u64,
                        "p99_99_display": stats.percentiles.p99_99.to_string()
                    }
                })
            })
            .collect();

        let output: SJSON::Value = json!({
            "benchmarks": benchmarks
        });

        SJSON::to_string_pretty(&output).expect("SJSON::Value always serializes")
    }
}

#[cfg(test)]
mod unit_tests;
