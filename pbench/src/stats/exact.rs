//! Exact-mode percentile computation via sorted arrays and linear interpolation.
//! All functions are methods on the unit struct [`ExactPercentiles`].

use crate::time::FineDuration;

use super::PercentileSet;

/// Unit struct organizing exact-mode computation.
///
/// All methods expect pre-sorted ascending slices of picosecond values.
/// Panics on empty input, callers must ensure non-empty samples.
pub(crate) struct ExactPercentiles;

impl ExactPercentiles {
    /// Compute a single percentile from pre-sorted pico values using linear interpolation.
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
}
