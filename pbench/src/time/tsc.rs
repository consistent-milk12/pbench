//! CPU timestamp counter (TSC) timer backend for `x86_64/x86`.
//!
//! Uses `RDTSC`/`RDTSCP` instructions for sub-nanosecond timing resolution.
//! Only available on `x86_64` and x86 platforms with an invariant TSC.

#[cfg(target_arch = "x86")]
use std::arch::x86;

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64 as x86;

use std::cmp::Ordering;
use std::fmt as StdFmt;
use std::thread as StdThread;
use std::time::{Duration, Instant};

use crate::time::{FineDuration, TimerOps, fence::Fence};

// ===========================================================================
//  TscUnavailable
// ===========================================================================

/// Reason why the TSC timer cannot be used.
#[must_use]
#[derive(Clone, Copy, Debug)]
pub enum TscUnavailable {
    /// `RDTSC`/`RDTSCP` instructions not supported by this CPU.
    MissingInstructions,

    /// TSC frequency is not constant (varies with power state).
    VariableFrequency,

    /// Measured frequency was zero.
    ZeroFrequency,

    /// Calibration measurements were too inconsistent.
    CalibrationFailed,
}

impl StdFmt::Display for TscUnavailable {
    fn fmt(&self, f: &mut StdFmt::Formatter<'_>) -> StdFmt::Result {
        let reason: &str = match self {
            Self::MissingInstructions => "missing RDTSC/RDTSCP instructions",
            Self::VariableFrequency => "variable TSC frequency",
            Self::ZeroFrequency => "zero TSC frequency",
            Self::CalibrationFailed => "TSC calibration failed",
        };

        f.write_str(reason)
    }
}

impl std::error::Error for TscUnavailable {}

// ===========================================================================
//  CPUID helpers
// ===========================================================================

/// Unit struct grouping CPUID query helpers.
struct Cpuid;

impl Cpuid {
    /// Returns `true` if `RDTSC` and `RDTSCP` instructions are available.
    ///
    /// Checks standard leaf `0x0000_0001` for RDTSC (EDX bit 4) and extended
    /// leaf `0x8000_0001` for RDTSCP (EDX bit 27). Both are required because
    /// `TscTimer::now()` uses RDTSC and `TscTimer::now_end()` uses RDTSCP.
    fn tsc_is_available() -> bool {
        // RDTSC: standard CPUID leaf 1, EDX bit 4.
        let has_rdtsc: bool = Self::query(0x0000_0001).edx & (1 << 4) != 0;

        // RDTSCP: extended CPUID leaf 0x8000_0001, EDX bit 27.
        let has_rdtscp: bool =
            Self::has_leaf(0x8000_0001) && Self::query(0x8000_0001).edx & (1 << 27) != 0;

        has_rdtsc && has_rdtscp
    }

    /// Returns `true` if the TSC has an invariant (constant) frequency.
    fn tsc_is_invariant() -> bool {
        let leaf: u32 = 0x8000_0007;

        if !Self::has_leaf(leaf) {
            return false;
        }

        Self::query(leaf).edx & (1 << 8) != 0
    }

    /// Returns `true` if the given extended CPUID leaf is available.
    fn has_leaf(leaf: u32) -> bool {
        Self::query(0x8000_0000).eax >= leaf
    }

    /// Execute the CPUID instruction.
    fn query(leaf: u32) -> x86::CpuidResult {
        unsafe { x86::__cpuid(leaf) }
    }
}

// ===========================================================================
//  TscTimer
// ===========================================================================

/// Timer backed by the CPU timestamp counter.
///
/// Provides sub-nanosecond resolution on `x86_64`/x86 CPUs with an invariant TSC.
/// Stores the TSC frequency and converts tick deltas to picoseconds via integer
/// arithmetic: `(ticks * 1_000_000_000_000) / frequency`.
#[derive(Clone, Copy, Debug)]
pub struct TscTimer {
    /// TSC frequency in Hz (ticks per second).
    frequency: u64,
}

/// Picoseconds per second, used for tick-to-pico conversion.
const PICOS_PER_SEC: u128 = 1_000_000_000_000;

impl TscTimer {
    // -----------------------------------------------------------------------
    //  Public API
    // -----------------------------------------------------------------------

    /// Attempt to calibrate the TSC timer.
    ///
    /// # Errors
    ///
    /// Returns [`TscUnavailable`] if:
    /// - The CPU lacks `RDTSC`/`RDTSCP` instructions
    /// - The TSC frequency is not invariant
    /// - Calibration measurements are too inconsistent or yield zero
    pub fn calibrate() -> Result<Self, TscUnavailable> {
        if !Cpuid::tsc_is_available() {
            return Err(TscUnavailable::MissingInstructions);
        }

        if !Cpuid::tsc_is_invariant() {
            return Err(TscUnavailable::VariableFrequency);
        }

        let freq: f64 = Self::measure_frequency()?;
        if freq <= 0.0 {
            return Err(TscUnavailable::ZeroFrequency);
        }

        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "TSC frequency in Hz always fits u64 (CPUs are <100 GHz)"
        )]
        let frequency: u64 = freq.round() as u64;

        if frequency == 0 {
            return Err(TscUnavailable::ZeroFrequency);
        }

        Ok(Self { frequency })
    }

    /// Read the TSC as a start timestamp (serialized before measurement).
    ///
    /// Uses `LFENCE; RDTSC; LFENCE` to serialize the read relative to
    /// surrounding instructions, matching divan's fencing pattern.
    #[inline(always)]
    #[must_use]
    pub fn now(&self) -> u64 {
        // Serialize previous operations before RDTSC.
        Self::lfence();

        let tsc: u64 = Self::rdtsc();

        // Serialize RDTSC before measured code.
        Self::lfence();

        tsc
    }

    /// Read the TSC as an end timestamp (serialized after measurement).
    ///
    /// Uses `RDTSCP; LFENCE` — RDTSCP is inherently serializing after
    /// all previous instructions, and the trailing LFENCE prevents
    /// subsequent code from reordering before the read.
    #[inline(always)]
    #[must_use]
    pub fn now_end(&self) -> u64 {
        // RDTSCP is serialized after all previous operations.
        let tsc: u64 = Self::rdtscp();

        // Serialize RDTSCP before any subsequent code.
        Self::lfence();

        tsc
    }

    /// Compute elapsed time between two TSC timestamps.
    ///
    /// Uses integer arithmetic: `(ticks * 1_000_000_000_000) / frequency`
    /// for maximum precision (no floating-point intermediate).
    #[inline]
    #[must_use]
    pub fn elapsed(&self, start: u64, end: u64) -> FineDuration {
        let diff: u64 = end.saturating_sub(start);
        let picos: u128 = (u128::from(diff) * PICOS_PER_SEC) / u128::from(self.frequency);

        FineDuration { picos }
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
        let mut delay_len: usize = 0;

        loop {
            for _ in 0..100_u32 {
                let start: u64 = self.now();

                if delay_len > 0 {
                    for n in 0..delay_len {
                        std::hint::black_box(n);
                    }
                }

                let end: u64 = self.now_end();

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

    // -----------------------------------------------------------------------
    //  Low-level TSC access (private)
    // -----------------------------------------------------------------------

    /// Read the TSC via RDTSC.
    #[inline(always)]
    fn rdtsc() -> u64 {
        Fence::compiler();
        // SAFETY: Reading the TSC is memory safe.
        let tsc: u64 = unsafe { x86::_rdtsc() };

        Fence::compiler();

        tsc
    }

    /// Read the TSC via RDTSCP (serializing variant).
    #[inline(always)]
    fn rdtscp() -> u64 {
        Fence::compiler();

        // SAFETY: Reading the TSC is memory safe.
        let tsc: u64 = unsafe { x86::__rdtscp(&mut 0) };

        Fence::compiler();

        tsc
    }

    /// Load fence.
    #[inline(always)]
    fn lfence() {
        // SAFETY: SSE2 is guaranteed on all x86_64 CPUs. On 32-bit x86,
        // any CPU with RDTSC/RDTSCP (checked in calibrate) has SSE2.
        unsafe { x86::_mm_lfence() }
    }

    // -----------------------------------------------------------------------
    //  Frequency calibration (private)
    // -----------------------------------------------------------------------

    /// Measure the TSC frequency in Hz.
    ///
    /// Uses increasing sleep delays (1ms to 256ms) and returns the first
    /// measurement that converges within 0.1% of the previous one. If no
    /// pair converges, returns the frequency from the closest pair.
    fn measure_frequency() -> Result<f64, TscUnavailable> {
        const TRIES: usize = 8;

        let mut delay_ms: u64 = 1;
        let mut prev_measure: f64 = f64::NEG_INFINITY;
        let mut measures: [f64; TRIES] = [0.0; TRIES];

        for slot in &mut measures {
            let measure: f64 = Self::measure_frequency_once(Duration::from_millis(delay_ms));

            // Converged within 0.1%?
            if measure * 0.999 <= prev_measure && prev_measure <= measure * 1.001 {
                return Ok(measure);
            }

            *slot = measure;
            prev_measure = measure;
            delay_ms *= 2;
        }

        // Find the pair with the smallest delta.
        let mut min_delta: f64 = f64::INFINITY;
        let mut result_index: usize = 0;

        for i in 0..TRIES {
            for j in (i + 1)..TRIES {
                let delta: f64 = (measures[i] - measures[j]).abs();

                if delta < min_delta {
                    min_delta = delta;
                    result_index = i;
                }
            }
        }

        let freq: f64 = measures[result_index];

        if freq <= 0.0 {
            return Err(TscUnavailable::CalibrationFailed);
        }

        Ok(freq)
    }

    /// Single frequency measurement: sleep for `delay`, measure TSC ticks and
    /// wall time, compute frequency.
    #[expect(clippy::cast_precision_loss, reason = "TSC ticks and nanos fit f64")]
    fn measure_frequency_once(delay: Duration) -> f64 {
        let (start_tsc, start_instant): (u64, Instant) = Self::tsc_instant_pair();

        StdThread::sleep(delay);

        let (end_tsc, end_instant): (u64, Instant) = Self::tsc_instant_pair();

        let elapsed_tsc: u64 = end_tsc.saturating_sub(start_tsc);
        let elapsed_duration: Duration = end_instant.duration_since(start_instant);

        (elapsed_tsc as f64 / elapsed_duration.as_nanos() as f64) * 1e9
    }

    /// Get a TSC/Instant pair with minimal latency between the two reads.
    fn tsc_instant_pair() -> (u64, Instant) {
        let mut best_latency: Duration = Duration::MAX;
        let mut best_pair: (u64, Instant) = (0, Instant::now());

        for _ in 0..100_u32 {
            let instant: Instant = Instant::now();
            let tsc: u64 = Self::rdtsc();
            let latency: Duration = instant.elapsed();

            let pair: (u64, Instant) = (tsc, instant);

            if latency.is_zero() {
                return pair;
            }

            if latency < best_latency {
                best_latency = latency;
                best_pair = pair;
            }
        }

        best_pair
    }
}

impl TimerOps for TscTimer {
    type Stamp = u64;

    #[inline(always)]
    fn now(&self) -> u64 {
        self.now()
    }

    #[inline(always)]
    fn now_end(&self) -> u64 {
        self.now_end()
    }

    #[inline(always)]
    fn elapsed(&self, start: u64, end: u64) -> FineDuration {
        self.elapsed(start, end)
    }
}
