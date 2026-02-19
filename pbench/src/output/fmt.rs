//! Throughput display formatting for counter-annotated benchmarks.
//!
//! Provides auto-scaling throughput display and byte value formatting
//! with binary/decimal unit selection.
//!
//! All free-standing logic is organised as associated functions on
//! [`Fmt`] (number formatting) or [`Scale`] (magnitude selection).

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
pub(crate) enum Scale {
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
    /// Infinite vals stay at [`Scale::One`].
    pub(crate) fn from_value(value: f64, bytes_format: BytesFormat) -> (f64, Self) {
        let starts: &[f64; Self::COUNT] = Self::starts(bytes_format);

        let scale: Self = if value.is_infinite() || (value < starts[1]) {
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
    #[expect(clippy::cast_precision_loss)]
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
pub(crate) enum ScaleFormat {
    /// Raw byte values (B, KB, MiB, etc.).
    Bytes(BytesFormat),

    /// Byte throughput (B/s, KB/s, MiB/s, etc.).
    BytesThroughput(BytesFormat),

    /// Character throughput (char/s, Kchar/s, etc.).
    CharsThroughput,

    /// CPU cycle frequency (Hz, KHz, MHz, etc.).
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
    pub(crate) fn bytes_format(self) -> BytesFormat {
        match self {
            Self::Bytes(bf) | Self::BytesThroughput(bf) => bf,

            Self::CharsThroughput | Self::CyclesThroughput | Self::ItemsThroughput => {
                BytesFormat::Decimal
            }
        }
    }

    /// Create the throughput format for a given counter kind and bytes format.
    #[must_use]
    pub(crate) fn throughput(kind: CounterKind, bytes_format: BytesFormat) -> Self {
        match kind {
            CounterKind::Bytes => Self::BytesThroughput(bytes_format),

            CounterKind::Chars => Self::CharsThroughput,

            CounterKind::Cycles => Self::CyclesThroughput,

            CounterKind::Items => Self::ItemsThroughput,
        }
    }
}

// =========================================================================
//  Fmt — number formatting
// =========================================================================
