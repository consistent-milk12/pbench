use super::*;

// --- compute_percentile ---

#[test]
fn percentile_single_sample() {
    let samples: Vec<u128> = vec![100];

    assert_eq!(ExactPercentiles::compute_percentile(&samples, 0.0), 100);
    assert_eq!(ExactPercentiles::compute_percentile(&samples, 0.5), 100);
    assert_eq!(ExactPercentiles::compute_percentile(&samples, 0.99), 100);
    assert_eq!(ExactPercentiles::compute_percentile(&samples, 1.0), 100);
}

#[test]
fn percentile_two_samples() {
    let samples: Vec<u128> = vec![100, 200];

    assert_eq!(ExactPercentiles::compute_percentile(&samples, 0.0), 100);
    assert_eq!(ExactPercentiles::compute_percentile(&samples, 0.5), 150);
    assert_eq!(ExactPercentiles::compute_percentile(&samples, 1.0), 200);
}

#[test]
fn percentile_ten_samples() {
    let samples: Vec<u128> = (0..10).collect();

    // rank = 0.5 * 9 = 4.5 → interp(4, 5, 0.5) = 4.5 → truncated to 4
    assert_eq!(ExactPercentiles::compute_percentile(&samples, 0.5), 4);

    // rank = 0.9 * 9 = 8.1 → interp(8, 9, 0.1) = 8.1 → truncated to 8
    assert_eq!(ExactPercentiles::compute_percentile(&samples, 0.9), 8);
}

#[test]
fn percentile_hundred_samples() {
    let samples: Vec<u128> = (0..100).collect();

    // rank = 0.99 * 99 = 98.01 → interp(98, 99, 0.01) ≈ 98
    let p99: u128 = ExactPercentiles::compute_percentile(&samples, 0.99);

    assert!((98..=99).contains(&p99), "p99 was {p99}");
}

#[test]
#[should_panic(expected = "cannot compute percentile of empty samples")]
fn percentile_empty_panics() {
    let samples: Vec<u128> = vec![];

    let _ = ExactPercentiles::compute_percentile(&samples, 0.5);
}

#[test]
#[should_panic(expected = "percentile must be in [0.0, 1.0]")]
fn percentile_out_of_range_panics() {
    let samples: Vec<u128> = vec![100];

    let _ = ExactPercentiles::compute_percentile(&samples, 1.5);
}

// --- compute_percentile_set ---

#[test]
fn compute_all_percentiles() {
    let samples: Vec<u128> = (0..1000).collect();
    let pset: PercentileSet = ExactPercentiles::compute_percentile_set(&samples);

    assert!(
        pset.p50.picos >= 495 && pset.p50.picos <= 505,
        "p50={}",
        pset.p50.picos
    );

    assert!(
        pset.p99.picos >= 985 && pset.p99.picos <= 995,
        "p99={}",
        pset.p99.picos
    );

    assert!(pset.p99_99.picos >= 998, "p99.99={}", pset.p99_99.picos);
}

// --- compute_mean ---

#[test]
fn mean_basic() {
    assert_eq!(ExactPercentiles::compute_mean(&[10, 20, 30]), 20);
}

#[test]
fn mean_single() {
    assert_eq!(ExactPercentiles::compute_mean(&[42]), 42);
}

#[test]
#[should_panic(expected = "cannot compute mean of empty samples")]
fn mean_empty_panics() {
    let _ = ExactPercentiles::compute_mean(&[]);
}

// --- compute_std_dev ---

#[test]
fn std_dev_uniform() {
    let samples: Vec<u128> = (0..100).collect();
    let sd: u128 = ExactPercentiles::compute_std_dev(&samples);

    assert!((27..=30).contains(&sd), "std_dev={sd}");
}

#[test]
fn std_dev_identical_values() {
    let samples: Vec<u128> = vec![100; 50];

    assert_eq!(ExactPercentiles::compute_std_dev(&samples), 0);
}

#[test]
#[should_panic(expected = "cannot compute std_dev of empty samples")]
fn std_dev_empty_panics() {
    let _ = ExactPercentiles::compute_std_dev(&[]);
}
