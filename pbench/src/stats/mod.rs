//! Core Percentile Computation Engine
//!
//! Provides two nodes: exact (sorted array + linear interpolation) for sample
//! counts below [`HDR_THRESHOLD`], and HDR histogram for larger sets. The unified
//! entry point is [`PercentileStats::Compute()`].

pub(crate) mod exact;
pub(crate) mod hdr;
pub(crate) mod sample;

use crate::{
    stats::{exact::ExactPercentiles, hdr::HdrRecorder},
    time::FineDuration,
};

/// Sample count threshold above which HDR histogram mode is used instead of
/// exact sorted-array mode.
const HDR_THRESHOLD: usize = 100_000;

/// Complete percentile stats for a benchmark run.
#[derive(Clone, Debug)]
pub struct PercentileStats {
    /// Number of samples collected.
    pub sample_count: u32,

    /// Total iterations across all samples.
    pub iter_count: u64,

    /// Minimum observed duration.
    pub min: FineDuration,

    /// Maximum observed duration.
    pub max: FineDuration,

    /// Arithmetic mean duration.
    pub mean: FineDuration,

    /// Population standard deviation.
    pub std_dev: FineDuration,

    /// Percentile breakpoints.
    pub percentiles: PercentileSet,
}

/// The five standard percentile breakpoints.
#[derive(Clone, Copy, Debug)]
pub struct PercentileSet {
    /// 50th percentile (median).
    pub p50: FineDuration,

    /// 95th percentile.
    pub p95: FineDuration,

    /// 99th percentile.
    pub p99: FineDuration,

    /// 99.9th percentile.
    pub p99_9: FineDuration,

    /// 99.99th percentile.
    pub p99_99: FineDuration,
}

impl PercentileStats {
    /// Compute percentile statistics from raw picosecond sample values.
    ///
    /// Automatically selects exact mode (sorted array + linear interpolation)
    /// for sample counts below [`HDR_THRESHOLD`], or HDR histogram mode for
    /// larger sets.
    ///
    /// # Arguments
    ///
    /// * `pico_samples` — per-iteration durations in picoseconds, one per sample
    /// * `sample_size` — number of iterations each sample represents
    ///
    /// # Panics
    ///
    /// Panics if `pico_samples` is empty or `sample_size` is zero.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Sample count bounded by HDR_THRESHOLD or Vec capacity, both fit u32"
    )]
    pub fn compute(pico_samples: &[u128], sample_size: u32) -> Self {
        assert!(
            !pico_samples.is_empty(),
            "cannot compute stats from empty samples"
        );
        assert!(sample_size > 0, "sample_size must be at least 1");

        let sample_count: u32 = pico_samples.len() as u32;
        let iter_count: u64 = u64::from(sample_count) * u64::from(sample_size);

        if pico_samples.len() < HDR_THRESHOLD {
            Self::compute_exact(pico_samples, sample_count, iter_count)
        } else {
            Self::compute_hdr(pico_samples, sample_count, iter_count)
        }
    }

    /// Exact mode: sort samples and use linear interpolation.
    fn compute_exact(pico_samples: &[u128], sample_count: u32, iter_count: u64) -> Self {
        let mut sorted: Vec<u128> = pico_samples.to_vec();
        sorted.sort_unstable();

        Self {
            sample_count,
            iter_count,
            min: FineDuration { picos: sorted[0] },
            max: FineDuration {
                picos: sorted[sorted.len() - 1],
            },
            mean: FineDuration {
                picos: ExactPercentiles::compute_mean(&sorted),
            },
            std_dev: FineDuration {
                picos: ExactPercentiles::compute_std_dev(&sorted),
            },
            percentiles: ExactPercentiles::compute_percentile_set(&sorted),
        }
    }

    fn compute_hdr(pico_samples: &[u128], sample_count: u32, iter_count: u64) -> Self {
        let mut recorder: HdrRecorder = HdrRecorder::new(3).expect("sigfig 3 is always valid");

        for &pico in pico_samples {
            recorder.record(pico);
        }

        Self {
            sample_count,
            iter_count,
            min: FineDuration {
                picos: recorder.min(),
            },
            max: FineDuration {
                picos: recorder.max(),
            },
            mean: FineDuration {
                picos: recorder.mean(),
            },
            std_dev: FineDuration {
                picos: recorder.stdev(),
            },
            percentiles: recorder.percentile_set(),
        }
    }

    /// Check whether the sample count is sufficient for each reported percentile.
    ///
    /// Returns a list of warning messages for percentiles that lack statistical
    /// significance. The rule is: a percentile at level `p` needs at least
    /// `1 / (1 - p)` samples (e.g., p99.99 needs ≥10,000 samples).
    ///
    /// Returns an empty [`Vec`] if all percentiles have sufficient samples.
    #[must_use]
    pub fn check_sample_sufficiency(sample_count: u32) -> Vec<&'static str> {
        let mut warnings: Vec<&'static str> = Vec::new();

        if sample_count < 20 {
            warnings.push("p95 needs at least 20 samples for statistical significance");
        }

        if sample_count < 100 {
            warnings.push("p99 needs at least 100 samples for statistical significance");
        }

        if sample_count < 1_000 {
            warnings.push("p99.9 needs at least 1,000 samples for statistical significance");
        }

        if sample_count < 10_000 {
            warnings.push("p99.99 needs at least 10,000 samples for statistical significance");
        }

        warnings
    }
}

#[cfg(test)]
mod unit_tests;
