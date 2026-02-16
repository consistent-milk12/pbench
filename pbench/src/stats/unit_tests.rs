use super::*;

// --- compute (exact mode) ---

#[test]
fn compute_stats_exact_mode() {
    // 100 samples — well below HDR_THRESHOLD, uses exact mode
    let samples: Vec<u128> = (0..100).collect();
    let stats: PercentileStats = PercentileStats::compute(&samples, 1);

    assert_eq!(stats.sample_count, 100);
    assert_eq!(stats.iter_count, 100);
    assert_eq!(stats.min.picos, 0);
    assert_eq!(stats.max.picos, 99);

    // Mean of [0..100) = 49
    assert!(
        (48..=50).contains(&stats.mean.picos),
        "mean={}",
        stats.mean.picos
    );

    // p50 should be around 49-50
    assert!(
        (45..=55).contains(&stats.percentiles.p50.picos),
        "p50={}",
        stats.percentiles.p50.picos
    );
}

#[test]
fn compute_stats_exact_mode_with_sample_size() {
    let samples: Vec<u128> = (0..50).collect();
    let stats: PercentileStats = PercentileStats::compute(&samples, 10);

    assert_eq!(stats.sample_count, 50);
    assert_eq!(stats.iter_count, 500);
}

#[test]
fn compute_stats_exact_mode_unsorted_input() {
    // Verify compute() sorts internally — input is descending
    let samples: Vec<u128> = (0..100).rev().collect();
    let stats: PercentileStats = PercentileStats::compute(&samples, 1);

    assert_eq!(stats.min.picos, 0);
    assert_eq!(stats.max.picos, 99);
}

// --- compute (HDR mode) ---

#[test]
fn compute_stats_hdr_mode() {
    // 100K samples — at HDR_THRESHOLD, uses HDR mode
    let samples: Vec<u128> = (0..100_000).collect();
    let stats: PercentileStats = PercentileStats::compute(&samples, 1);

    assert_eq!(stats.sample_count, 100_000);
    assert_eq!(stats.iter_count, 100_000);

    // Should produce valid stats without OOM
    assert!(stats.mean.picos > 0);
    assert!(stats.percentiles.p50.picos > 0);
}

// --- compute (boundary) ---

#[test]
fn compute_stats_hdr_boundary() {
    // 99,999 samples → exact mode
    let exact_samples: Vec<u128> = (0..99_999).collect();
    let exact_stats: PercentileStats = PercentileStats::compute(&exact_samples, 1);

    // 100,000 samples → HDR mode
    let hdr_samples: Vec<u128> = (0..100_000).collect();
    let hdr_stats: PercentileStats = PercentileStats::compute(&hdr_samples, 1);

    // Both should produce valid stats
    assert_eq!(exact_stats.sample_count, 99_999);
    assert_eq!(hdr_stats.sample_count, 100_000);
    assert!(exact_stats.percentiles.p50.picos > 0);
    assert!(hdr_stats.percentiles.p50.picos > 0);
}

#[test]
#[should_panic(expected = "cannot compute stats from empty samples")]
fn compute_stats_empty_panics() {
    let _ = PercentileStats::compute(&[], 1);
}

#[test]
#[should_panic(expected = "sample_size must be at least 1")]
fn compute_stats_zero_sample_size_panics() {
    let samples: Vec<u128> = vec![100, 200, 300];
    let _ = PercentileStats::compute(&samples, 0);
}

// --- check_sample_sufficiency ---

#[test]
fn sample_sufficiency_warnings() {
    // 100 samples: p95 and p99 are fine, p99.9 and p99.99 warn
    let warnings: Vec<&str> = PercentileStats::check_sample_sufficiency(100);

    assert_eq!(warnings.len(), 2);
    assert!(warnings[0].contains("p99.9"));
    assert!(warnings[1].contains("p99.99"));
}

#[test]
fn sample_sufficiency_all_ok() {
    let warnings: Vec<&str> = PercentileStats::check_sample_sufficiency(10_000);

    assert!(warnings.is_empty());
}

#[test]
fn sample_sufficiency_very_low() {
    // 10 samples: warns for p95, p99, p99.9, p99.99
    let warnings: Vec<&str> = PercentileStats::check_sample_sufficiency(10);

    assert_eq!(warnings.len(), 4);
}
