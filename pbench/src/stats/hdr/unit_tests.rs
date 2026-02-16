use super::*;

// --- HdrRecorder basic percentiles ---

#[test]
fn hdr_basic_percentiles() {
    let mut recorder: HdrRecorder = HdrRecorder::new(3).unwrap();

    for i in 0..10_000_u128 {
        recorder.record(i);
    }

    let pset: PercentileSet = recorder.percentile_set();

    // HDR has quantization error, so use wider tolerance
    assert!(
        (4500..=5500).contains(&pset.p50.picos),
        "p50={}",
        pset.p50.picos
    );
    assert!(
        (9400..=9999).contains(&pset.p99.picos),
        "p99={}",
        pset.p99.picos
    );
}

#[test]
fn hdr_single_value() {
    let mut recorder: HdrRecorder = HdrRecorder::new(3).unwrap();

    recorder.record(42_000);

    let pset: PercentileSet = recorder.percentile_set();

    // All percentiles should return the same value (within HDR quantization)
    assert_eq!(pset.p50.picos, pset.p99_99.picos);
}

#[test]
fn hdr_mean_and_stddev() {
    let mut recorder: HdrRecorder = HdrRecorder::new(3).unwrap();

    for i in 0..1000_u128 {
        recorder.record(i);
    }

    let mean: u128 = recorder.mean();

    // Mean of [0..1000) should be ~499
    assert!((450..=550).contains(&mean), "mean={mean}");

    let sd: u128 = recorder.stdev();

    // Stdev should be non-zero for a uniform spread
    assert!(sd > 0, "stdev should be non-zero");
}

#[test]
fn hdr_min_max_len() {
    let mut recorder: HdrRecorder = HdrRecorder::new(3).unwrap();

    recorder.record(100);
    recorder.record(500);
    recorder.record(1000);

    assert_eq!(recorder.min(), 100);
    assert_eq!(recorder.max(), 1000);
    assert_eq!(recorder.len(), 3);
    assert!(!recorder.is_empty());
}

#[test]
fn hdr_empty() {
    let recorder: HdrRecorder = HdrRecorder::new(3).unwrap();

    assert!(recorder.is_empty());
    assert_eq!(recorder.len(), 0);
}

#[test]
fn hdr_clamps_u128_to_u64() {
    let mut recorder: HdrRecorder = HdrRecorder::new(3).unwrap();

    // Values above u64::MAX should be clamped, not panic
    recorder.record(u128::from(u64::MAX) + 1);

    assert_eq!(recorder.len(), 1);
}
