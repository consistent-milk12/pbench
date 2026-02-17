//! Throughput counter system for benchmark measurements.
//!
//! Users attach a [`Counter`] to a benchmark to measure throughput, bytes
//! processed per second, items iterated, characters handled, or CPU cycles.
//! The runner collects counter values alongside timing data and the output
//! formatters display throughput as "1.234 GB/s" or "5.678 Mitem/s".
//!
//! Four concrete counter types are provided:
//! - [`BytesCount`] — byte throughput (B/s, KB/s, MB/s, …)
//! - [`CharsCount`] — Unicode character throughput (char/s, Kchar/s, …)
//! - [`CyclesCount`] — CPU cycle throughput displayed as frequency (Hz, `KHz`, MHz, …)
//! - [`ItemsCount`] — generic item throughput (item/s, Kitem/s, …)

use std::mem as StdMem;

pub(crate) mod collection;

/// Classification of a counter for display formatting.
///
/// The output formatters (table, json, CSV) use this to select the
/// appropriate unit suffix and scaling strategy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CounterKind {
    /// Byte throughput, scaled as B/s, KB/s, MB/s, GB/s, TB/s.
    Bytes,

    /// Unicode character throughput, scaled as char/s, Kchar/s, Mchar/s.
    Chars,

    /// CPU cycle throughput, displayed as Hz, `KHz`, MHz, GHz.
    Cycles,

    /// Generic item throughput, scaled as item/s, Kitem/s, Mitem/s.
    Items,
}

/// Trait for throughput counters attached to benchmarks.
///
/// Each counter reports how many units of work were processed per
/// iteration. The benchmarking harness multiplies by iteration count
/// and divides by elapsed time to compute throughput.
///
/// # Implementors
///
/// Four built-in types implement this trait: [`BytesCount`],
/// [`ItemsCount`], [`CharsCount`], and [`CyclesCount`].
pub trait Counter: Send + Sync {
    /// The number of units processed per iteration.
    fn count(&self) -> u64;

    /// The kind of counter, used for display formatting.
    fn kind(&self) -> CounterKind;
}

// =========================================================================
//  BytesCount
// =========================================================================

/// Counts bytes processed per iteration.
///
/// Attach to a benchmark to display throughput as B/s, KB/s, MB/s, etc.
///
/// # Examples
///
/// ```ignore
/// // Chainable builder: `counter` takes `&self` and returns `&Self`.
/// bencher.counter(BytesCount::of_slice(&data)).bench_refs(|| {
///     process(&data);
/// });
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BytesCount(u64);

impl BytesCount {
    /// Create a byte counter with an explicit count.
    #[inline]
    #[must_use]
    pub const fn new(count: u64) -> Self {
        Self(count)
    }

    /// Count the byte length of a slice.
    ///
    /// Computes `std::mem::size_of_val(slice)`.
    ///
    /// # Panics
    ///
    /// Panics if the byte length exceeds `u64::MAX` (impossible on
    /// current 64-bit targets, but guarded for correctness).
    #[inline]
    #[must_use]
    pub fn of_slice<T>(slice: &[T]) -> Self {
        let byte_len: usize = StdMem::size_of_val(slice);

        Self(u64::try_from(byte_len).expect("slice byte size exceeds u64::MAX"))
    }

    /// Count the byte length of a string slice.
    #[inline]
    #[must_use]
    pub const fn of_str(s: &str) -> Self {
        Self(s.len() as u64)
    }
}

impl Counter for BytesCount {
    #[inline]
    fn count(&self) -> u64 {
        self.0
    }

    #[inline]
    fn kind(&self) -> CounterKind {
        CounterKind::Bytes
    }
}

// =========================================================================
//  CharsCount
// =========================================================================

/// Counts Unicode characters processed per iteration.
///
/// Attach to a benchmark to display throughput as char/s, Kchar/s, etc.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharsCount(u64);

impl CharsCount {
    /// Create a character counter with an explicit count.
    #[inline]
    #[must_use]
    pub const fn new(count: u64) -> Self {
        Self(count)
    }

    /// Count the number of Unicode characters in a string slice.
    #[inline]
    #[must_use]
    pub fn of_str(s: &str) -> Self {
        Self(s.chars().count() as u64)
    }
}

impl Counter for CharsCount {
    #[inline]
    fn count(&self) -> u64 {
        self.0
    }

    #[inline]
    fn kind(&self) -> CounterKind {
        CounterKind::Chars
    }
}

// =========================================================================
//  CyclesCount
// =========================================================================

/// Counts CPU cycles per iteration.
///
/// Displayed as frequency: Hz, `KHz`, MHz, GHz.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CyclesCount(u64);

impl CyclesCount {
    /// Create a cycle counter with an explicit count.
    #[inline]
    #[must_use]
    pub const fn new(count: u64) -> Self {
        Self(count)
    }
}

impl Counter for CyclesCount {
    #[inline]
    fn count(&self) -> u64 {
        self.0
    }

    #[inline]
    fn kind(&self) -> CounterKind {
        CounterKind::Cycles
    }
}

// =========================================================================
//  ItemsCount
// =========================================================================

/// Counts generic items processed per iteration.
///
/// Attach to a benchmark to display throughput as item/s, Kitem/s, etc.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemsCount(u64);

impl ItemsCount {
    /// Create an item counter with an explicit count.
    #[inline]
    #[must_use]
    pub const fn new(count: u64) -> Self {
        Self(count)
    }

    /// Count the number of elements yielded by an iterator.
    ///
    /// Consumes the iterator to determine its length. Passing an infinite
    /// iterator (like `std::iter::repeat(0)`) will hang indefinitely.
    #[must_use]
    pub fn of_iter<I: IntoIterator>(iter: I) -> Self {
        Self(iter.into_iter().count() as u64)
    }
}

impl Counter for ItemsCount {
    #[inline]
    fn count(&self) -> u64 {
        self.0
    }

    #[inline]
    fn kind(&self) -> CounterKind {
        CounterKind::Items
    }
}

#[cfg(test)]
mod unit_tests;
