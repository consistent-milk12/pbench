//! Precision module

use std::{fmt as StdFmt, ops as StdOps, time::Duration};

use crate::time::Formatter;

/// [Picosecond](https://en.wikipedia.org/wiki/Picosecond)-precise [`Duration`].
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct FineDuration {
    /// Picosecond precision
    pub picos: u128,
}

impl FineDuration {
    /// Definition
    pub const ZERO: Self = Self { picos: 0 };

    /// Max representable duration (sentinel val)
    pub const MAX: Self = Self { picos: u128::MAX };

    /// Divide picos by a u64 divisor.
    #[inline]
    #[must_use]
    pub const fn div_u64(self, n: u64) -> Self {
        Self {
            picos: self.picos / (n as u128),
        }
    }

    /// Multiply picos by a u64 factor.
    #[inline]
    #[must_use]
    pub const fn mul_u64(self, n: u64) -> Self {
        Self {
            picos: self.picos * (n as u128),
        }
    }

    /// Checked subtraction, returning `None` if `other > self`.
    #[inline]
    #[must_use]
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.picos
            .checked_sub(other.picos)
            .map(|picos: u128| Self { picos })
    }

    /// Returns true if this duration is zero.
    #[inline]
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.picos == 0
    }
}

impl From<Duration> for FineDuration {
    #[inline]
    fn from(duration: Duration) -> Self {
        Self {
            picos: duration
                .as_nanos()
                .checked_mul(1_000)
                .unwrap_or_else(|| panic!("{duration:?} is too large to fit in `FineDuration`")),
        }
    }
}

impl StdOps::Add for FineDuration {
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self {
        Self {
            picos: self.picos + other.picos,
        }
    }
}

impl StdOps::AddAssign for FineDuration {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        self.picos += other.picos;
    }
}

impl StdOps::Sub for FineDuration {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Self {
            picos: self.picos - other.picos,
        }
    }
}

// ===========================================================================
//  Display formatting
// ===========================================================================

const NANOS: u128 = 1_000;
const MICROS: u128 = 1_000 * NANOS;
const MILLIS: u128 = 1_000 * MICROS;
const SEC: u128 = 1_000 * MILLIS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimeScale {
    PicoSec,
    NanoSec,
    MicroSec,
    MilliSec,
    Sec,
}

impl TimeScale {
    pub const fn from_picos(p: u128) -> Self {
        if p < NANOS {
            Self::PicoSec
        } else if p < MICROS {
            Self::NanoSec
        } else if p < MILLIS {
            Self::MicroSec
        } else if p < SEC {
            Self::MilliSec
        } else {
            Self::Sec
        }
    }

    pub const fn picos(self) -> u128 {
        match self {
            Self::PicoSec => 1,

            Self::NanoSec => NANOS,

            Self::MicroSec => MICROS,

            Self::MilliSec => MILLIS,

            Self::Sec => SEC,
        }
    }

    pub const fn suffix(self) -> &'static str {
        match self {
            Self::PicoSec => "ps",

            Self::NanoSec => "ns",

            Self::MicroSec => "µs",

            Self::MilliSec => "ms",

            Self::Sec => "s",
        }
    }
}

impl StdFmt::Display for FineDuration {
    fn fmt(&self, f: &mut StdFmt::Formatter) -> StdFmt::Result {
        let sig_figs: usize = f.precision().unwrap_or(4);
        let p: u128 = self.picos;
        let mut scale = TimeScale::from_picos(p);

        if (scale == TimeScale::PicoSec) && (sig_figs > 3) {
            scale = TimeScale::NanoSec;
        }

        let multiple: u128 = {
            let sf: u32 = u32::try_from(sig_figs).unwrap_or(u32::MAX);
            10_u128.saturating_pow(sf)
        };

        #[expect(clippy::cast_precision_loss)]
        let val: f64 = (((p * multiple) / scale.picos()) as f64) / multiple as f64;
        let mut s: String = Formatter::format_f64(val, sig_figs);

        s.push(' ');
        s.push_str(scale.suffix());

        f.write_str(&s)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod unit_tests;
