//! Aggregation of per-iteration counter values across benchmark samples.
//!
//! [`CounterCollection`] accumulates one per-iteration counter value per
//! sample and computes the mean for throughput display.

use super::CounterKind;

/// Aggregates per-iteration counter values for a single counter kind.
///
/// The runner pushes one **per-iteration** count per sample (i.e. the
/// sample's total counter value divided by `sample_size`). After
/// collection completes, [`mean_count`](Self::mean_count) returns the
/// mean per-iteration count, which the output formatters divide by
/// per-iteration duration to compute throughput.
///
/// This matches the bencher pipeline where durations are stored
/// per-iteration (`raw_duration / sample_size` in `Bencher::run`).
///
/// NOTE: Current limitation: A single `CounterCollection` stores exactly one
/// [`CounterKind`]. Multi-counter support (attaching both `BytesCount`
/// and `ItemsCount` to the same benchmark) would require evolving to
/// per-kind storage (e.g. `HashMap<CounterKind, Vec<u64>>`).
#[derive(Clone, Debug)]
pub struct CounterCollection {
    /// The kind of counter being aggregated.
    kind: CounterKind,

    /// Per-sample counts, one entry per collected sample.
    counts: Vec<u64>,
}

impl CounterCollection {
    /// Create a new empty collection for the given counter kind.
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn new(kind: CounterKind) -> Self {
        Self {
            kind,
            counts: Vec::new(),
        }
    }

    /// Create a new collection pre-allocated for `capacity` samples.
    #[must_use]
    pub(crate) fn with_capacity(kind: CounterKind, capacity: usize) -> Self {
        Self {
            kind,
            counts: Vec::with_capacity(capacity),
        }
    }

    /// Push a per-iteration count for one sample.
    ///
    /// The caller must divide the sample's total counter value by
    /// `sample_size` before pushing, so that the stored value is
    /// per-iteration — consistent with the per-iteration durations
    /// in the stats pipeline.
    pub(crate) fn push(&mut self, count: u64) {
        self.counts.push(count);
    }

    /// The kind of counter being aggregated.
    #[inline]
    #[must_use]
    pub(crate) const fn kind(&self) -> CounterKind {
        self.kind
    }

    /// Number of samples recorded.
    #[inline]
    #[must_use]
    #[allow(
        dead_code,
        reason = "Public API surface - required by len/is_empty pairing rule"
    )]
    pub(crate) const fn len(&self) -> usize {
        self.counts.len()
    }

    /// Whether any samples have been recorded.
    #[inline]
    #[must_use]
    #[allow(
        dead_code,
        reason = "Public API surface - required by len/is_empty pairing rule"
    )]
    pub(crate) const fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    /// Compute the mean per-iteration count across all samples.
    ///
    /// Returns `None` if no samples have been recorded.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "Precision loss acceptable for mean calculation — f64 has sufficient range for throughput display"
    )]
    pub(crate) fn mean_count(&self) -> Option<f64> {
        if self.counts.is_empty() {
            return None;
        }

        // Sum as u128 to prevent overflow when aggregating many large counts.
        let sum: u128 = self.counts.iter().map(|c: &u64| u128::from(*c)).sum();
        Some(sum as f64 / self.counts.len() as f64)
    }

    /// Return the raw per-iteration counts (one per sample).
    #[cfg(test)]
    #[inline]
    #[must_use]
    pub(crate) fn counts(&self) -> &[u64] {
        &self.counts
    }
}

#[cfg(test)]
mod unit_tests;
