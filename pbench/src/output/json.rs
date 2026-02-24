//! JSON output renderer for benchmark results.
//!
//! Produces a pretty-printed JSON object containing all benchmark statistics
//! with both raw picosecond values and human-readable display strings.
//! Gated behind the `json` feature flag.

use serde_json::{self as SJSON, json};

use crate::baseline::SerializableStats;
use crate::stats::PercentileStats;

/// JSON output renderer.
pub struct JsonRenderer;

impl JsonRenderer {
    /// Render benchmark results as a pretty-printed JSON string.
    ///
    /// Each result tuple contains `(name, thread_count, stats)`.
    /// Each benchmark entry includes raw picosecond values (`*_picos`) for
    /// machine consumption and human-readable display strings (`*_display`)
    /// for convenience.
    ///
    /// # Panics
    ///
    /// Panics if `SJSON` fails to serialize the constructed value
    /// (should never happen with well-formed `SJSON::Value`).
    ///
    /// Debug-panics if any pico value exceeds `u64::MAX`.
    #[must_use]
    pub(crate) fn render(results: &[(&str, u32, &PercentileStats)]) -> String {
        let benchmarks: Vec<SJSON::Value> = results
            .iter()
            .map(
                |&(name, thread_count, stats): &(&str, u32, &PercentileStats)| {
                    let p = SerializableStats::picos_as_u64;

                    json!({
                        "name": name,
                        "threads": thread_count,
                        "sample_count": stats.sample_count,
                        "iter_count": stats.iter_count,
                        "min_picos": p(stats.min.picos, "min_picos"),
                        "min_display": stats.min.to_string(),
                        "max_picos": p(stats.max.picos, "max_picos"),
                        "max_display": stats.max.to_string(),
                        "mean_picos": p(stats.mean.picos, "mean_picos"),
                        "mean_display": stats.mean.to_string(),
                        "std_dev_picos": p(stats.std_dev.picos, "std_dev_picos"),
                        "std_dev_display": stats.std_dev.to_string(),
                        "percentiles": {
                            "p50_picos": p(stats.percentiles.p50.picos, "p50_picos"),
                            "p50_display": stats.percentiles.p50.to_string(),
                            "p95_picos": p(stats.percentiles.p95.picos, "p95_picos"),
                            "p95_display": stats.percentiles.p95.to_string(),
                            "p99_picos": p(stats.percentiles.p99.picos, "p99_picos"),
                            "p99_display": stats.percentiles.p99.to_string(),
                            "p99_9_picos": p(stats.percentiles.p99_9.picos, "p99_9_picos"),
                            "p99_9_display": stats.percentiles.p99_9.to_string(),
                            "p99_99_picos": p(stats.percentiles.p99_99.picos, "p99_99_picos"),
                            "p99_99_display": stats.percentiles.p99_99.to_string()
                        }
                    })
                },
            )
            .collect();

        let output: SJSON::Value = json!({
            "benchmarks": benchmarks
        });

        SJSON::to_string_pretty(&output).expect("SJSON::Value always serializes")
    }
}

#[cfg(test)]
mod unit_tests;
