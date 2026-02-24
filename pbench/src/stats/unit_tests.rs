#![allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]

use hdrhistogram::Histogram;

use super::exact::ExactPercentiles;
use super::hdr::HdrRecorder;
use super::{PercentileSet, PercentileStats};

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
fn sample_sufficiency_config_too_low() {
    // Requested 100, got 100 — config issue, not max_time
    let warnings: Vec<String> = PercentileStats::check_sample_sufficiency(100, 100);

    assert_eq!(warnings.len(), 2);
    assert!(warnings[0].contains("p99.9"));
    assert!(warnings[0].contains("increase sample_count"));
    assert!(warnings[1].contains("p99.99"));
    assert!(warnings[1].contains("increase sample_count"));
}

#[test]
fn sample_sufficiency_max_time_truncated() {
    // Requested 10_000, but only got 100 — max_time cut it short
    let warnings: Vec<String> = PercentileStats::check_sample_sufficiency(100, 10_000);

    assert_eq!(warnings.len(), 2);
    assert!(warnings[0].contains("p99.9"));
    assert!(warnings[0].contains("max_time"));
    assert!(warnings[1].contains("p99.99"));
    assert!(warnings[1].contains("max_time"));
    assert!(warnings[1].contains("only 100"));
}

#[test]
fn sample_sufficiency_all_ok() {
    let warnings: Vec<String> = PercentileStats::check_sample_sufficiency(10_000, 10_000);

    assert!(warnings.is_empty());
}

#[test]
fn sample_sufficiency_very_low() {
    // 10 samples requested and collected: warns for p95, p99, p99.9, p99.99
    let warnings: Vec<String> = PercentileStats::check_sample_sufficiency(10, 10);

    assert_eq!(warnings.len(), 4);
}

// =========================================================================
//  Cross-validation: exact vs HDR on identical data
// =========================================================================

/// Feed the same dataset to both exact and HDR paths, verify all percentiles
/// agree within HDR's precision tolerance (~0.1% at sigfig=3).
#[test]
fn cross_validate_exact_vs_hdr() {
    // Use 10,000 samples so both paths have meaningful tail percentiles.
    // Values spread across a realistic nanosecond-scale range in picoseconds.
    let samples: Vec<u128> = (0..10_000)
        .map(|i: u64| {
            // Simulate a skewed latency distribution: base 100ns + quadratic tail.
            let base: u128 = 100_000; // 100 ns in picos
            let skew: u128 = (u128::from(i) * u128::from(i)) / 100;
            base + skew
        })
        .collect();

    // Exact path.
    let mut sorted: Vec<u128> = samples.clone();
    sorted.sort_unstable();
    let exact_set: PercentileSet = ExactPercentiles::compute_percentile_set(&sorted);
    let exact_mean: u128 = ExactPercentiles::compute_mean(&sorted);

    // HDR path.
    let mut recorder: HdrRecorder = HdrRecorder::new(3).expect("valid sigfig");
    for &v in &samples {
        recorder.record(v);
    }
    let hdr_set: PercentileSet = recorder.percentile_set();
    let hdr_mean: u128 = recorder.mean();

    // HDR with sigfig=3 guarantees values are within 0.1% of the true value.
    // We use 1% tolerance to be safe across bucket boundaries.
    let tolerance: f64 = 0.01;

    let pairs: [(&str, u128, u128); 6] = [
        ("p50", exact_set.p50.picos, hdr_set.p50.picos),
        ("p95", exact_set.p95.picos, hdr_set.p95.picos),
        ("p99", exact_set.p99.picos, hdr_set.p99.picos),
        ("p99.9", exact_set.p99_9.picos, hdr_set.p99_9.picos),
        ("p99.99", exact_set.p99_99.picos, hdr_set.p99_99.picos),
        ("mean", exact_mean, hdr_mean),
    ];

    for (label, exact_val, hdr_val) in pairs {
        let diff: f64 = (exact_val as f64 - hdr_val as f64).abs();
        let relative: f64 = diff / exact_val as f64;

        assert!(
            relative < tolerance,
            "{label}: exact={exact_val}, hdr={hdr_val}, relative_error={relative:.6} exceeds {tolerance}"
        );
    }
}

// =========================================================================
//  Known uniform distribution: analytical percentile verification
// =========================================================================

/// For a sorted uniform sequence [0, 1, 2, ..., N-1], the theoretical
/// percentile at quantile q is: (N-1) * q (with linear interpolation).
/// Verify pbench's exact mode matches this formula precisely.
#[test]
fn exact_percentiles_match_uniform_analytical() {
    let n: usize = 10_000;
    let samples: Vec<u128> = (0..n as u128).collect();

    let set: PercentileSet = ExactPercentiles::compute_percentile_set(&samples);

    // Analytical: percentile_q = floor((N-1) * q + frac_interp)
    // For integer N-1 and clean quantiles, linear interpolation gives exact results.
    let n_f: f64 = (n - 1) as f64;

    let cases: [(&str, u128, f64); 5] = [
        ("p50", set.p50.picos, 0.50),
        ("p95", set.p95.picos, 0.95),
        ("p99", set.p99.picos, 0.99),
        ("p99.9", set.p99_9.picos, 0.999),
        ("p99.99", set.p99_99.picos, 0.9999),
    ];

    for (label, actual, quantile) in cases {
        let expected: u128 = (n_f * quantile) as u128;

        // Allow ±1 for truncation in the linear interpolation.
        assert!(
            actual.abs_diff(expected) <= 1,
            "{label}: expected={expected}, actual={actual} (quantile={quantile})"
        );
    }

    // Mean of [0..N) = (N-1)/2
    let mean: u128 = ExactPercentiles::compute_mean(&samples);
    let expected_mean: u128 = (n as u128 - 1) / 2;

    assert_eq!(
        mean, expected_mean,
        "mean: expected={expected_mean}, actual={mean}"
    );
}

// =========================================================================
//  Reference comparison: pbench exact vs hdrhistogram crate directly
// =========================================================================

/// Construct an [`Histogram`] manually and compare its quantile
/// values against pbench's exact mode on the same data. This ensures our
/// [`HdrRecorder`] wrapper faithfully delegates to the underlying crate.
#[test]
fn hdr_wrapper_matches_raw_hdrhistogram() {
    // Realistic benchmark-like values: 500ns to 50us in picoseconds.
    let samples: Vec<u128> = (0..5_000)
        .map(|i: u64| {
            let base: u128 = 500_000; // 500 ns
            let spike: u128 = if i.is_multiple_of(100) {
                50_000_000 // occasional 50 µs spike
            } else {
                u128::from(i) * 100
            };
            base + spike
        })
        .collect();

    // pbench HdrRecorder path.
    let mut recorder: HdrRecorder = HdrRecorder::new(3).expect("valid sigfig");
    for &v in &samples {
        recorder.record(v);
    }
    let pbench_set: PercentileSet = recorder.percentile_set();

    // Raw hdrhistogram crate, constructed identically.
    let mut raw_hist: Histogram<u64> =
        Histogram::new_with_max(3_600_000_000_000_000, 3).expect("valid");
    raw_hist.auto(true);
    for &v in &samples {
        let clamped: u64 = u64::try_from(v).unwrap_or(u64::MAX);
        raw_hist.record(clamped).expect("record");
    }

    // Must be identical — same crate, same parameters, same data.
    assert_eq!(
        pbench_set.p50.picos,
        u128::from(raw_hist.value_at_quantile(0.5)),
        "p50 mismatch"
    );
    assert_eq!(
        pbench_set.p95.picos,
        u128::from(raw_hist.value_at_quantile(0.95)),
        "p95 mismatch"
    );
    assert_eq!(
        pbench_set.p99.picos,
        u128::from(raw_hist.value_at_quantile(0.99)),
        "p99 mismatch"
    );
    assert_eq!(
        pbench_set.p99_9.picos,
        u128::from(raw_hist.value_at_quantile(0.999)),
        "p99.9 mismatch"
    );
    assert_eq!(
        pbench_set.p99_99.picos,
        u128::from(raw_hist.value_at_quantile(0.9999)),
        "p99.99 mismatch"
    );
}
