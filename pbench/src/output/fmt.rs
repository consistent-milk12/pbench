//! Throughput display formatting for counter-annotated benchmarks.
//!
//! Provides auto-scaling throughput display (B/s, KB/s, item/s, MHz, etc.)
//! and byte value formatting with binary/decimal unit selection.
//!
//! All free-standing logic is organised as associated functions on
//! [`Fmt`] (number formatting) or [`Scale`] (magnitude selection).
//!
//! NOTE: Mostly follows divan, `format_f64` is more complete here. Though
//! the covered cases are rare enough to not make much difference.

use std::fmt as StdFmt;
use std::iter as StdIter;

use crate::cli::BytesFormat;
use crate::counter::CounterKind;

// =========================================================================
//  Scale
// =========================================================================

/// Auto-scaling magnitude for throughput and byte display.
///
/// Each variant represents a power-of-1000 (decimal) or power-of-1024
/// (binary) step, selected automatically by [`Scale::from_value`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    /// 10^0 / 2^0.
    One,

    /// 10^3 / 2^10.
    Kilo,

    /// 10^6 / 2^20.
    Mega,

    /// 10^9 / 2^30.
    Giga,

    /// 10^12 / 2^40.
    Tera,

    /// 10^15 / 2^50.
    Peta,
}

impl Scale {
    /// Number of scale levels.
    const COUNT: usize = 6;

    /// Select the appropriate scale for `value` and return the scaled value.
    ///
    /// Uses binary (1024) or decimal (1000) thresholds depending on
    /// `bytes_format`. Infinite, `NaN`, and negative values stay at
    /// [`Scale::One`].
    #[must_use]
    pub(crate) fn from_value(value: f64, bytes_format: BytesFormat) -> (f64, Self) {
        let starts: &[f64; Self::COUNT] = Self::starts(bytes_format);

        let scale: Self = if value.is_nan() || value.is_infinite() || (value < starts[1]) {
            Self::One
        } else if value < starts[2] {
            Self::Kilo
        } else if value < starts[3] {
            Self::Mega
        } else if value < starts[4] {
            Self::Giga
        } else if value < starts[5] {
            Self::Tera
        } else {
            Self::Peta
        };

        (value / starts[scale as usize], scale)
    }

    /// Unit suffix string for this scale in the given format context.
    #[must_use]
    pub(crate) const fn suffix(self, format: ScaleFormat) -> &'static str {
        match format {
            ScaleFormat::Bytes(bf) => {
                const SUFFIXES: &[[&str; Scale::COUNT]; 2] = &[
                    ["B", "KB", "MB", "GB", "TB", "PB"],
                    ["B", "KiB", "MiB", "GiB", "TiB", "PiB"],
                ];

                SUFFIXES[bf as usize][self as usize]
            }

            ScaleFormat::BytesThroughput(bf) => {
                const SUFFIXES: &[[&str; Scale::COUNT]; 2] = &[
                    ["B/s", "KB/s", "MB/s", "GB/s", "TB/s", "PB/s"],
                    ["B/s", "KiB/s", "MiB/s", "GiB/s", "TiB/s", "PiB/s"],
                ];

                SUFFIXES[bf as usize][self as usize]
            }

            ScaleFormat::CharsThroughput => {
                const SUFFIXES: &[&str; Scale::COUNT] = &[
                    "char/s", "Kchar/s", "Mchar/s", "Gchar/s", "Tchar/s", "Pchar/s",
                ];

                SUFFIXES[self as usize]
            }

            ScaleFormat::CyclesThroughput => {
                const SUFFIXES: &[&str; Scale::COUNT] = &["Hz", "KHz", "MHz", "GHz", "THz", "PHz"];

                SUFFIXES[self as usize]
            }

            ScaleFormat::ItemsThroughput => {
                const SUFFIXES: &[&str; Scale::COUNT] = &[
                    "item/s", "Kitem/s", "Mitem/s", "Gitem/s", "Titem/s", "Pitem/s",
                ];

                SUFFIXES[self as usize]
            }
        }
    }

    /// Scale boundary thresholds for the given bytes format.
    #[expect(
        clippy::cast_precision_loss,
        reason = "1024^N fits in f64 exactly up to 2^50"
    )]
    const fn starts(bytes_format: BytesFormat) -> &'static [f64; Self::COUNT] {
        const STARTS: &[[f64; Scale::COUNT]; 2] = &[
            [1.0, 1e3, 1e6, 1e9, 1e12, 1e15],
            [
                1.0,
                1024.0,
                1024_u64.pow(2) as f64,
                1024_u64.pow(3) as f64,
                1024_u64.pow(4) as f64,
                1024_u64.pow(5) as f64,
            ],
        ];

        &STARTS[bytes_format as usize]
    }
}

// =========================================================================
//  ScaleFormat
// =========================================================================

/// Context for selecting the right unit suffix on a [`Scale`].
///
/// Encodes both the counter kind and (for byte counters) the binary/decimal
/// display preference.
#[derive(Clone, Copy, Debug)]
pub enum ScaleFormat {
    /// Raw byte values (B, KB, MiB, etc.).
    Bytes(BytesFormat),

    /// Byte throughput (B/s, KB/s, MiB/s, etc.).
    BytesThroughput(BytesFormat),

    /// Character throughput (char/s, Kchar/s, etc.).
    CharsThroughput,

    /// CPU cycle frequency (Hz, `KHz`, MHz, etc.).
    CyclesThroughput,

    /// Generic item throughput (item/s, Kitem/s, etc.).
    ItemsThroughput,
}

impl ScaleFormat {
    /// The bytes format relevant to this scale context.
    ///
    /// Non-byte formats return [`BytesFormat::Decimal`] since they always
    /// use powers of 1000.
    #[must_use]
    pub(crate) const fn bytes_format(self) -> BytesFormat {
        match self {
            Self::Bytes(bf) | Self::BytesThroughput(bf) => bf,

            Self::CharsThroughput | Self::CyclesThroughput | Self::ItemsThroughput => {
                BytesFormat::Decimal
            }
        }
    }

    /// Create the throughput format for a given counter kind and bytes format.
    #[must_use]
    pub(crate) const fn throughput(kind: CounterKind, bytes_format: BytesFormat) -> Self {
        match kind {
            CounterKind::Bytes => Self::BytesThroughput(bytes_format),

            CounterKind::Chars => Self::CharsThroughput,

            CounterKind::Cycles => Self::CyclesThroughput,

            CounterKind::Items => Self::ItemsThroughput,
        }
    }
}

// =========================================================================
//  Fmt - number formatting
// =========================================================================

/// Utility struct for number formatting.
pub struct Fmt;

impl Fmt {
    /// Format an `f64` to the given number of significant figures.
    #[must_use]
    pub(crate) fn format_f64(val: f64, sig_figs: usize) -> String {
        let mut s: String = val.to_string();

        let Some(dot_index) = s.find('.') else {
            return s;
        };

        // Count significant integer digits (skip sign, "0" has zero sig digits).
        let int_str: &str = &s[..dot_index];
        let abs_int: &str = int_str
            .strip_prefix('-')
            .map_or(int_str, |stripped| stripped);

        let int_sig: usize = if abs_int == "0" { 0 } else { abs_int.len() };

        if int_sig >= sig_figs {
            s.truncate(dot_index);

            return s;
        }

        let fract_start: usize = dot_index + 1;
        let fract_bytes: &str = &s[fract_start..];

        let leading_zeros: usize = if int_sig == 0 {
            fract_bytes.bytes().take_while(|&b: &u8| b == b'0').count()
        } else {
            0
        };

        let fract_digits: usize = sig_figs - int_sig + leading_zeros;
        let fract_end: usize = fract_start + fract_digits;

        // Truncate and trim trailing zeros.
        let trim_start: usize = fract_start;
        let trim_end: usize = if fract_end <= s.len() {
            fract_end
        } else {
            s.len()
        };

        let fract_slice: &str = &s[trim_start..trim_end];
        let trimmed_len: usize = fract_slice.trim_end_matches('0').len();

        if trimmed_len == 0 {
            s.truncate(dot_index);
        } else {
            s.truncate(trim_start + trimmed_len);
        }

        s
    }

    /// Format a byte value with auto-scaling and unit suffix.
    ///
    /// Uses binary (1024-based: KiB, MiB) or decimal (1000-based: KB, MB)
    /// scaling depending on `bytes_format`.
    #[must_use]
    #[allow(
        dead_code,
        reason = "Standalone byte formatting for future output modes and docs"
    )]
    pub(crate) fn format_bytes(val: f64, sig_figs: usize, bytes_format: BytesFormat) -> String {
        let (scaled, scale): (f64, Scale) = Scale::from_value(val, bytes_format);

        let mut result: String = Self::format_f64(scaled, sig_figs);
        result.push(' ');
        result.push_str(scale.suffix(ScaleFormat::Bytes(bytes_format)));
        result
    }
}

// =========================================================================
//  DisplayThroughput
// =========================================================================

/// Displays throughput (count per second) for a counter-annotated benchmark.
pub struct DisplayThroughput {
    /// Counter classification.
    pub(crate) kind: CounterKind,

    /// Mean per-iteration count.
    pub(crate) count: f64,

    /// Per-iteration duration in picoseconds for this column.
    pub(crate) picos: f64,

    /// Byte format selection.
    pub(crate) bytes_format: BytesFormat,
}

impl StdFmt::Display for DisplayThroughput {
    fn fmt(&self, f: &mut StdFmt::Formatter<'_>) -> StdFmt::Result {
        let count_per_sec: f64 = if (self.count == 0.0) || (self.picos == 0.0) {
            0.0
        } else {
            self.count * (1e12 / self.picos)
        };

        let format: ScaleFormat = ScaleFormat::throughput(self.kind, self.bytes_format);
        let (scaled, scale): (f64, Scale) = Scale::from_value(count_per_sec, format.bytes_format());
        let sig_figs: usize = f.precision().unwrap_or(4);

        let mut s: String = Fmt::format_f64(scaled, sig_figs);
        s.push(' ');
        s.push_str(scale.suffix(format));

        // Fill up to specified width.
        if let Some(fill_len) = f
            .width()
            .and_then(|width: usize| width.checked_sub(s.len()))
        {
            match f.align() {
                None | Some(StdFmt::Alignment::Left) => {
                    s.extend(StdIter::repeat_n(f.fill(), fill_len));
                }

                _ => return Err(StdFmt::Error),
            }
        }

        f.write_str(&s)
    }
}

impl StdFmt::Debug for DisplayThroughput {
    fn fmt(&self, f: &mut StdFmt::Formatter<'_>) -> StdFmt::Result {
        StdFmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod unit_tests;
