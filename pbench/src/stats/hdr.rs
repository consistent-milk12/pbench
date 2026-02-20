//! HDR histogram mode for percentile computation on large sample sets.
//!
//! Wraps the [`hdrhistogram`] crate to provide the same interface as
//! [`ExactPercentiles`](super::exact::ExactPercentiles) but with bounded memory
//! usage regardless of sample count.

use hdrhistogram::{CreationError, Histogram};

use crate::time::FineDuration;

use super::PercentileSet;

/// Initial maximum trackable value: 1 hour in picoseconds.
///
/// Auto-resize is enabled, so values above this will grow the histogram
/// rather than being clamped.
const INITIAL_MAX: u64 = 3_600_000_000_000_000;

/// Wrapper around [`hdrhistogram::Histogram`] for recording picosecond timing samples.
///
/// Uses 3 significant figures by default. Auto-resizing is enabled, so any
/// `u64`-representable value can be recorded without clamping.
pub struct HdrRecorder {
    /// The underlying HDR histogram.
    histogram: Histogram<u64>,
}

impl HdrRecorder {
    /// Create a new recorder with the given number of significant figures.
    ///
    /// The histogram is pre-sized to track values up to 1 hour in picoseconds
    /// with auto-resize enabled for values beyond that range.
    ///
    /// # Errors
    ///
    /// Returns [`hdrhistogram::CreationError`] if `sigfig` is not in `[0, 5]`.
    #[must_use = "returns a new recorder without modifying anything"]
    pub fn new(sigfig: u8) -> Result<Self, CreationError> {
        let mut histogram: Histogram<u64> = Histogram::new_with_max(INITIAL_MAX, sigfig)?;
        histogram.auto(true);

        Ok(Self { histogram })
    }

    /// Record a single picosecond value.
    ///
    /// Values exceeding `u64::MAX` are clamped to `u64::MAX`. The histogram
    /// auto-resizes for values above the initial max, so no data is lost.
    ///
    /// NOTE: HDR histograms have a minimum trackable value of 1, so a recorded
    /// value of 0 is stored as 1 (a 1-picosecond error, negligible for timing).
    #[inline]
    pub fn record(&mut self, picos: u128) {
        let clamped: u64 = u64::try_from(picos).unwrap_or(u64::MAX);

        // With auto-resize enabled, record never fails for valid u64 values.
        // The only theoretical failure is memory exhaustion from resizing.
        self.histogram
            .record(clamped)
            .expect("HDR auto-resize failed — out of memory");
    }

    /// Compute all five standard percentiles from recorded values.
    #[must_use]
    pub fn percentile_set(&self) -> PercentileSet {
        PercentileSet {
            p50: FineDuration {
                picos: u128::from(self.histogram.value_at_quantile(0.5)),
            },
            p95: FineDuration {
                picos: u128::from(self.histogram.value_at_quantile(0.95)),
            },
            p99: FineDuration {
                picos: u128::from(self.histogram.value_at_quantile(0.99)),
            },
            p99_9: FineDuration {
                picos: u128::from(self.histogram.value_at_quantile(0.999)),
            },
            p99_99: FineDuration {
                picos: u128::from(self.histogram.value_at_quantile(0.9999)),
            },
        }
    }

    /// Arithmetic mean of all recorded values, truncated to integer picos.
    #[inline]
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "HDR mean is non-negative and within u128 range for timing values"
    )]
    pub fn mean(&self) -> u128 {
        self.histogram.mean() as u128
    }

    /// Population standard deviation, truncated to integer picos.
    #[inline]
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "HDR stdev is non-negative and within u128 range for timing values"
    )]
    pub fn stdev(&self) -> u128 {
        self.histogram.stdev() as u128
    }

    /// Minimum recorded value.
    #[inline]
    #[must_use]
    pub fn min(&self) -> u128 {
        u128::from(self.histogram.min())
    }

    /// Maximum recorded value.
    #[inline]
    #[must_use]
    pub fn max(&self) -> u128 {
        u128::from(self.histogram.max())
    }

    /// Number of recorded values.
    #[inline]
    #[must_use]
    #[allow(
        dead_code,
        reason = "Public API surface - required by len/is_empty pairing rule"
    )]
    pub fn len(&self) -> u64 {
        self.histogram.len()
    }

    /// Returns `true` if no values have been recorded.
    #[inline]
    #[must_use]
    #[allow(
        dead_code,
        reason = "Public API surface - required by len/is_empty pairing rule"
    )]
    pub fn is_empty(&self) -> bool {
        self.histogram.is_empty()
    }
}

#[cfg(test)]
mod unit_tests;
