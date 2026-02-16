//! Exact-mode percentile computation via sorted arrays and linear interpolation.
//! All functions are methods on the unit struct [`ExactPercentiles`].

use crate::time::FineDuration;

use super::PercentileSet;

/// Unit struct organizing exact-mode computation.
///
/// All methods expect pre-sorted ascending slices of picosecond values.
/// Panics on empty input, callers must ensure non-empty samples.
pub struct ExactPercentiles;

impl ExactPercentiles {
    /// Compute a single percentile from pre-sorted pico values using linear interpolation.
    ///
    /// `samples` must be sorted ascending and non-empty. `p` is in `[0.0, 1.0]`.
    ///
    /// # Panics
    ///
    /// Empty samples or out of bound `p`.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "Computed from sample length"
    )]
    pub fn compute_percentile(samples: &[u128], p: f64) -> u128 {
        assert!(
            !samples.is_empty(),
            "cannot compute percentile of empty samples"
        );
        assert!(
            (0.0..=1.0).contains(&p),
            "percentile must be in [0.0, 1.0], got {p}"
        );

        if samples.len() == 1 {
            return samples[0];
        }

        let rank: f64 = p * (samples.len() - 1) as f64;
        let lower_idx: usize = rank.floor() as usize;
        let upper_idx: usize = rank.ceil() as usize;
        let frac: f64 = rank - rank.floor();

        let lower: u128 = samples[lower_idx];
        let upper: u128 = samples[upper_idx];

        // Linear interpolation: lower + (upper - lower) * frac
        lower + ((upper as f64 - lower as f64) * frac) as u128
    }

    /// Compute all five standard percentiles from pre-sorted pico vals.
    #[must_use]
    pub fn compute_percentile_set(samples: &[u128]) -> PercentileSet {
        PercentileSet {
            p50: FineDuration {
                picos: Self::compute_percentile(samples, 0.5),
            },
            p95: FineDuration {
                picos: Self::compute_percentile(samples, 0.95),
            },
            p99: FineDuration {
                picos: Self::compute_percentile(samples, 0.99),
            },
            p99_9: FineDuration {
                picos: Self::compute_percentile(samples, 0.999),
            },
            p99_99: FineDuration {
                picos: Self::compute_percentile(samples, 0.9999),
            },
        }
    }

    /// Arithmetic mean
    ///
    /// # Panics
    ///
    /// Panics on empty samples or if the sum of all samples overflows `u128`.
    #[must_use]
    #[inline(always)]
    pub fn compute_mean(samples: &[u128]) -> u128 {
        assert!(!samples.is_empty(), "cannot compute mean of empty samples");

        let sum: u128 = samples
            .iter()
            .try_fold(0_u128, |acc: u128, v: &u128| acc.checked_add(*v))
            .expect("sum of samples overflowed u128 - values are corrupt or unrealistic");

        sum / (samples.len() as u128)
    }

    /// Compute population standard deviation of pico values.
    ///
    /// Uses the formula: `sqrt(sum((x - mean)^2) / N)`.
    ///
    /// # Panics
    ///
    /// Panics on empty input.
    #[inline]
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation
    )]
    pub fn compute_std_dev(samples: &[u128]) -> u128 {
        assert!(
            !samples.is_empty(),
            "cannot compute std_dev of empty samples"
        );

        let mean: f64 = Self::compute_mean(samples) as f64;
        let variance: f64 = samples
            .iter()
            .map(|sample: &u128| {
                let diff: f64 = (*sample as f64) - mean;

                diff * diff
            })
            .sum::<f64>()
            / (samples.len() as f64);

        variance.sqrt() as u128
    }
}

#[cfg(test)]
mod unit_tests;
