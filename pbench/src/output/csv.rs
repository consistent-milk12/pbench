//! CSV output renderer for benchmark results.
//!
//! Produces comma-separated output with a header row and one data row per
//! benchmark. All duration values are raw picoseconds for machine parsing.
//! No external CSV crate needed.

use std::fmt::Write;

use crate::stats::PercentileStats;

/// CSV column header.
///
/// Includes `iter_count` for completeness.
const HEADER: &str = "name,sample_count,iter_count,min_picos,max_picos,mean_picos,std_dev_picos,p50_picos,p95_picos,p99_picos,p99_9_picos,p99_99_picos";

/// CSV output renderer.
///
/// Converts benchmark results into a CSV string with a header row followed
/// by one data row per benchmark. All durations are in raw picoseconds.
pub struct CsvRenderer;

impl CsvRenderer {
    /// Render benchmark results as a CSV string.
    ///
    /// The output starts with a header row followed by one row per benchmark.
    /// Duration fields are raw picosecond `u128` values.
    #[must_use]
    pub(crate) fn render(results: &[(&str, &PercentileStats)]) -> String {
        let mut out: String = String::with_capacity(HEADER.len() + results.len() * 128);

        out.push_str(HEADER);
        out.push('\n');

        for &(name, stats) in results {
            let _: std::fmt::Result = writeln!(
                out,
                "{},{},{},{},{},{},{},{},{},{},{},{}",
                name,
                stats.sample_count,
                stats.iter_count,
                stats.min.picos,
                stats.max.picos,
                stats.mean.picos,
                stats.std_dev.picos,
                stats.percentiles.p50.picos,
                stats.percentiles.p95.picos,
                stats.percentiles.p99.picos,
                stats.percentiles.p99_9.picos,
                stats.percentiles.p99_99.picos,
            );
        }

        out
    }
}

#[cfg(test)]
mod unit_tests;
