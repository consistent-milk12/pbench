//! Tests for CSV output renderer.

use super::*;
use crate::stats::{PercentileSet, PercentileStats};
use crate::time::FineDuration;

// --- csv_output_header_and_row ---

#[test]
fn csv_output_header_and_row() {
    let stats: PercentileStats = PercentileStats {
        sample_count: 1000,
        iter_count: 10_000,
        min: FineDuration { picos: 100_500 },
        max: FineDuration { picos: 620_800 },
        mean: FineDuration { picos: 142_100 },
        std_dev: FineDuration { picos: 15_300 },
        percentiles: PercentileSet {
            p50: FineDuration { picos: 125_200 },
            p95: FineDuration { picos: 180_300 },
            p99: FineDuration { picos: 250_100 },
            p99_9: FineDuration { picos: 410_500 },
            p99_99: FineDuration { picos: 620_800 },
        },
    };

    let csv_str: String = CsvRenderer::render(&[("bench_a", &stats)]);
    let lines: Vec<&str> = csv_str.lines().collect();

    // Header row
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("name,"));
    assert!(lines[0].contains("p50_picos"));
    assert!(lines[0].contains("p99_99_picos"));
    assert!(lines[0].contains("iter_count"));

    // Data row
    let cols: Vec<&str> = lines[1].split(',').collect();
    assert_eq!(cols[0], "bench_a");
    assert_eq!(cols[1], "1000"); // sample_count
    assert_eq!(cols[2], "10000"); // iter_count
    assert_eq!(cols[3], "100500"); // min_picos
    assert_eq!(cols[4], "620800"); // max_picos
    assert_eq!(cols[5], "142100"); // mean_picos
    assert_eq!(cols[6], "15300"); // std_dev_picos
    assert_eq!(cols[7], "125200"); // p50_picos
    assert_eq!(cols[8], "180300"); // p95_picos
    assert_eq!(cols[9], "250100"); // p99_picos
    assert_eq!(cols[10], "410500"); // p99_9_picos
    assert_eq!(cols[11], "620800"); // p99_99_picos
}

// --- csv_multiple_rows ---

#[test]
fn csv_multiple_rows() {
    let stats_a: PercentileStats = PercentileStats {
        sample_count: 100,
        iter_count: 1_000,
        min: FineDuration { picos: 50_000 },
        max: FineDuration { picos: 200_000 },
        mean: FineDuration { picos: 100_000 },
        std_dev: FineDuration { picos: 10_000 },
        percentiles: PercentileSet {
            p50: FineDuration { picos: 90_000 },
            p95: FineDuration { picos: 150_000 },
            p99: FineDuration { picos: 180_000 },
            p99_9: FineDuration { picos: 195_000 },
            p99_99: FineDuration { picos: 200_000 },
        },
    };

    let stats_b: PercentileStats = PercentileStats {
        sample_count: 200,
        iter_count: 2_000,
        min: FineDuration { picos: 1_000_000 },
        max: FineDuration { picos: 5_000_000 },
        mean: FineDuration { picos: 2_500_000 },
        std_dev: FineDuration { picos: 500_000 },
        percentiles: PercentileSet {
            p50: FineDuration { picos: 2_000_000 },
            p95: FineDuration { picos: 4_000_000 },
            p99: FineDuration { picos: 4_500_000 },
            p99_9: FineDuration { picos: 4_800_000 },
            p99_99: FineDuration { picos: 5_000_000 },
        },
    };

    let csv_str: String = CsvRenderer::render(&[("bench_a", &stats_a), ("bench_b", &stats_b)]);
    let lines: Vec<&str> = csv_str.lines().collect();

    assert_eq!(lines.len(), 3); // header + 2 data rows
    assert!(lines[1].starts_with("bench_a,"));
    assert!(lines[2].starts_with("bench_b,"));
}

// --- csv_empty_results ---

#[test]
fn csv_empty_results() {
    let csv_str: String = CsvRenderer::render(&[]);
    let lines: Vec<&str> = csv_str.lines().collect();

    // Header only, no data rows
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("name,"));
}
