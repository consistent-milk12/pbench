//! OS-clock timer backend using [`std::time::Instant`].

use std::time::Instant;
use std::{cmp::Ordering, hint as StdHint};

use crate::time::{fence::Fence, FineDuration};

/// Timer backed by the OS monotonic clock.
#[derive(Clone, Copy, Debug)]
pub struct InstantTimer;

impl InstantTimer {
    /// Take a timestamp.
    #[must_use]
    #[inline(always)]
    pub fn now(&self) -> Instant {
        Instant::now()
    }

    /// Compute elapsed time between two timestamps.
    #[must_use]
    #[inline(always)]
    pub fn elapsed(&self, start: Instant, end: Instant) -> FineDuration {
        FineDuration::from(end.duration_since(start))
    }

    /// Measure the smallest non-zero duration this timer can resolve.
    ///
    /// Takes back-to-back timestamp pairs in batches of 100, increasing an
    /// artificial delay if all measurements are zero. Returns the smallest
    /// non-zero elapsed time after observing it at least 100 times, or after
    /// the delay loop exceeds 100 iterations.
    #[must_use]
    pub fn measure_precision(&self) -> FineDuration {
        let mut min_sample: FineDuration = FineDuration::MAX;
        let mut seen_count: u32 = 0;

        // If immediate succession yields zero, add artificial delay.
        let mut delay_len: usize = 0;

        loop {
            for _ in 0..100 {
                Fence::full();

                let start: Instant = Instant::now();

                if delay_len > 0 {
                    for n in 0..delay_len {
                        StdHint::black_box(n);
                    }
                }

                let end: Instant = Instant::now();

                Fence::compiler();

                let sample: FineDuration = self.elapsed(start, end);

                if sample.is_zero() {
                    continue;
                }

                match sample.cmp(&min_sample) {
                    Ordering::Greater => {
                        if delay_len > 100 {
                            return min_sample;
                        }
                    }

                    Ordering::Equal => {
                        seen_count += 1;
                        if seen_count >= 100 {
                            return min_sample;
                        }
                    }

                    Ordering::Less => {
                        min_sample = sample;
                        seen_count = 0;
                    }
                }
            }

            delay_len = delay_len.saturating_add(1);
        }
    }
}
