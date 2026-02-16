//! Sample collection types for benchmarking harness.
//!
//! [`RawSample`] represents a single timing measurements with metadata.
//! [`SampleCollection`] aggregates samples and provides extraction methods
//! for feeding into the statistics engine.

use crate::time::FineDuration;

/// A single timing sample with metadata.
#[derive(Clone, Copy, Debug)]
pub struct RawSample {
    /// Measure per-iteration duration.
    pub(crate) duration: FineDuration,

    /// Number of iters in this sample batch.
    pub(crate) sample_size: u32,
}

/// Aggrgated collection of timing samples.
#[derive(Clone, Debug, Default)]
pub struct SampleCollection {
    /// The collected samples.
    samples: Vec<RawSample>,
}

impl SampleCollection {
    /// Creates an empty collection.
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            samples: Vec::new(),
        }
    }

    /// Creates an empty collection with pre-allocated cap.
    #[inline]
    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        Self {
            samples: Vec::with_capacity(n),
        }
    }

    /// Appends a sample to the collection.
    #[inline]
    pub fn push(&mut self, sample: RawSample) {
        self.samples.push(sample);
    }

    /// Returns the number of samples in the collection.
    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.samples.len()
    }

    /// Check if collection empty
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Returns picosecond durations sorted in ascending order.
    #[must_use]
    pub fn sorted_picos(&self) -> Vec<u128> {
        let mut picos: Vec<u128> = self
            .samples
            .iter()
            .map(|s: &RawSample| s.duration.picos)
            .collect();

        picos.sort_unstable();

        picos
    }

    /// Returns picosecond durations in collection order (unsorted).
    ///
    /// Making this to use as input to `PercentileStats::compute`,
    /// which will handle sorting internally.
    #[must_use]
    pub fn raw_picos(&self) -> Vec<u128> {
        self.samples
            .iter()
            .map(|s: &RawSample| s.duration.picos)
            .collect()
    }

    /// Returns the sum of all sample durations.
    #[must_use]
    pub fn total_duration(&self) -> FineDuration {
        let total: u128 = self
            .samples
            .iter()
            .map(|s: &RawSample| s.duration.picos)
            .sum();

        FineDuration { picos: total }
    }

    /// Returns the total number of iterations across all samples.
    ///
    /// Each sample may represent a different number of iterations,
    /// we just get a sum of them all.
    #[must_use]
    pub fn total_iterations(&self) -> u64 {
        self.samples
            .iter()
            .map(|s: &RawSample| u64::from(s.sample_size))
            .sum()
    }
}

#[cfg(test)]
mod unit_tests;
