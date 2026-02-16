//! Timing infrastructure.
//!
//! Provides [`FineDuration`] for picosecond-precision durations, [`InstantTimer`]
//! for OS-clock timing, and [`TscTimer`] for x86 TSC-based timing.

pub(crate) mod fence;
pub mod fine_duration;
pub(crate) mod instant;
pub mod utils;

#[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
pub(crate) mod tsc;

use std::time::Instant;

pub use fine_duration::FineDuration;
pub use instant::InstantTimer;
pub use utils::Formatter;

#[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
pub use tsc::{TscTimer, TscUnavailable};

/// Active timer backend used for benchmarking.
///
/// Wraps either [`InstantTimer`] (always available) or [`TscTimer`]
/// (`x86_64`/x86 only, sub-nanosecond resolution).
#[derive(Clone, Copy, Debug)]
pub enum Timer {
    /// Operating system monotonic clock.
    Os(InstantTimer),

    /// CPU timestamp counter (`x86_64`/x86 only).
    #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
    Tsc(TscTimer),
}

/// Opaque timestamp from a [`Timer`].
#[derive(Clone, Copy, Debug)]
pub enum Timestamp {
    /// OS-clock timestamp.
    Os(Instant),

    /// TSC tick count.
    #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
    Tsc(u64),
}

impl Timer {
    /// Select the best available timer.
    ///
    /// Prefers TSC on `x86_64`/x86 if calibration succeeds, otherwise
    /// falls back to [`InstantTimer`].
    #[must_use]
    pub fn best_available() -> Self {
        #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
        {
            if let Ok(tsc_timer) = TscTimer::calibrate() {
                return Self::Tsc(tsc_timer);
            }
        }

        Self::Os(InstantTimer)
    }

    /// Take a start timestamp.
    #[must_use]
    #[inline(always)]
    pub fn now(&self) -> Timestamp {
        match self {
            Self::Os(t) => Timestamp::Os(t.now()),

            #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
            Self::Tsc(t) => Timestamp::Tsc(t.now()),
        }
    }

    /// Take an end timestamp.
    ///
    /// For TSC this uses `RDTSCP` (serializing variant). For the OS clock
    /// this is identical to [`now`](Self::now).
    #[must_use]
    #[inline(always)]
    pub fn now_end(&self) -> Timestamp {
        match self {
            Self::Os(t) => Timestamp::Os(t.now()),

            #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
            Self::Tsc(t) => Timestamp::Tsc(t.now_end()),
        }
    }

    /// Compute elapsed time between a start and end timestamp.
    ///
    /// # Panics
    ///
    /// Panics if `start` and `end` are from different timer backends.
    #[inline]
    #[must_use]
    pub fn elapsed(&self, start: Timestamp, end: Timestamp) -> FineDuration {
        match (self, start, end) {
            (Self::Os(t), Timestamp::Os(s), Timestamp::Os(e)) => t.elapsed(s, e),

            #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
            (Self::Tsc(t), Timestamp::Tsc(s), Timestamp::Tsc(e)) => t.elapsed(s, e),

            _ => panic!("mismatched timer and timestamp variants"),
        }
    }

    /// Measure the smallest non-zero duration this timer can resolve.
    #[must_use]
    pub fn measure_precision(&self) -> FineDuration {
        match self {
            Self::Os(t) => t.measure_precision(),

            #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
            Self::Tsc(t) => t.measure_precision(),
        }
    }
}

#[cfg(test)]
mod unit_tests;
