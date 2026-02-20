//! Tests for JSON output renderer.

use super::JsonRenderer;
use crate::stats::{PercentileSet, PercentileStats};
use crate::time::FineDuration;
use serde_json as SJSON;

// --- json_output_valid ---

#[test]
fn json_output_valid() {
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

    let json_str: String = JsonRenderer::render(&[("bench_a", &stats)]);
    let parsed: SJSON::Value = SJSON::from_str(&json_str).expect("output must be valid JSON");

    // Top-level structure
    let benchmarks: &Vec<SJSON::Value> = parsed["benchmarks"]
        .as_array()
        .expect("benchmarks must be an array");
    assert_eq!(benchmarks.len(), 1);

    // Benchmark entry fields
    let entry: &SJSON::Value = &benchmarks[0];
    assert_eq!(entry["name"], "bench_a");
    assert_eq!(entry["sample_count"], 1000);
    assert_eq!(entry["iter_count"], 10_000);
    assert_eq!(entry["min_picos"], 100_500);
    assert_eq!(entry["max_picos"], 620_800);
    assert_eq!(entry["mean_picos"], 142_100);
    assert_eq!(entry["std_dev_picos"], 15_300);

    // Display strings present
    assert!(entry["min_display"].is_string());
    assert!(entry["max_display"].is_string());
    assert!(entry["mean_display"].is_string());
    assert!(entry["std_dev_display"].is_string());

    // Percentiles sub-object
    let pctls: &SJSON::Value = &entry["percentiles"];
    assert_eq!(pctls["p50_picos"], 125_200);
    assert_eq!(pctls["p95_picos"], 180_300);
    assert_eq!(pctls["p99_picos"], 250_100);
    assert_eq!(pctls["p99_9_picos"], 410_500);
    assert_eq!(pctls["p99_99_picos"], 620_800);
    assert!(pctls["p50_display"].is_string());
    assert!(pctls["p99_99_display"].is_string());
}

// --- json_multiple_benchmarks ---

#[test]
fn json_multiple_benchmarks() {
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

    let json_str: String = JsonRenderer::render(&[("bench_a", &stats_a), ("bench_b", &stats_b)]);
    let parsed: SJSON::Value = SJSON::from_str(&json_str).expect("output must be valid JSON");

    let benchmarks: &Vec<SJSON::Value> = parsed["benchmarks"]
        .as_array()
        .expect("benchmarks must be an array");
    assert_eq!(benchmarks.len(), 2);
    assert_eq!(benchmarks[0]["name"], "bench_a");
    assert_eq!(benchmarks[1]["name"], "bench_b");
}

// --- json_empty_results ---

#[test]
fn json_empty_results() {
    let json_str: String = JsonRenderer::render(&[]);
    let parsed: SJSON::Value = SJSON::from_str(&json_str).expect("output must be valid JSON");

    let benchmarks: &Vec<SJSON::Value> = parsed["benchmarks"]
        .as_array()
        .expect("benchmarks must be an array");
    assert!(benchmarks.is_empty());
}
