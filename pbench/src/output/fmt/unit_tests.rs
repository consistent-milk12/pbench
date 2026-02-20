//! Unit tests for the `output::fmt` module.

use std::f64::consts::PI;

use super::{DisplayThroughput, Fmt, Scale, ScaleFormat};
use crate::cli::BytesFormat;
use crate::counter::CounterKind;

// =========================================================================
//  Fmt::format_f64 - basic cases
// =========================================================================

// --- format_f64_trims_trailing_zeros ---

#[test]
fn format_f64_trims_trailing_zeros() {
    let result: String = Fmt::format_f64(1.500, 4);
    assert_eq!(result, "1.5");
}

// --- format_f64_whole_number ---

#[test]
fn format_f64_whole_number() {
    let result: String = Fmt::format_f64(100.0, 4);
    assert_eq!(result, "100");
}

// --- format_f64_preserves_significant_digits ---

#[test]
fn format_f64_preserves_significant_digits() {
    let result: String = Fmt::format_f64(1.234_567, 4);
    assert_eq!(result, "1.234");
}

// --- format_f64_truncates_not_rounds ---

#[test]
fn format_f64_truncates_not_rounds() {
    // 1.999 with sig_figs=3 → 1 int digit + 2 frac → "1.99" (not "2.0")
    let result: String = Fmt::format_f64(1.999, 3);
    assert_eq!(result, "1.99");
}

// =========================================================================
//  Fmt::format_f64 - sub-1.0 values
// =========================================================================

// --- format_f64_sub_one ---

#[test]
fn format_f64_sub_one() {
    // 0.5 → int_sig=0, no leading zeros, fract_digits=4 → "0.5"
    let result: String = Fmt::format_f64(0.5, 4);
    assert_eq!(result, "0.5");
}

// --- format_f64_sub_one_with_leading_zeros ---

#[test]
fn format_f64_sub_one_with_leading_zeros() {
    // 0.001234 → int_sig=0, leading_zeros=2, fract_digits=4+2=6 → "0.001234"
    let result: String = Fmt::format_f64(0.001_234, 4);
    assert_eq!(result, "0.001234");
}

// --- format_f64_sub_one_two_sig_figs ---

#[test]
fn format_f64_sub_one_two_sig_figs() {
    // 0.001234 with sig_figs=2 → leading_zeros=2, fract_digits=2+2=4 → "0.0012"
    let result: String = Fmt::format_f64(0.001_234, 2);
    assert_eq!(result, "0.0012");
}

// =========================================================================
//  Fmt::format_f64 - negative values
// =========================================================================

// --- format_f64_negative ---

#[test]
fn format_f64_negative() {
    // -3.14159 → abs_int="3", int_sig=1, fract_digits=3 → "-3.141"
    let result: String = Fmt::format_f64(-PI, 4);
    assert_eq!(result, "-3.141");
}

// --- format_f64_negative_sub_one ---

#[test]
fn format_f64_negative_sub_one() {
    // -0.001234 → abs_int="0", int_sig=0, leading_zeros=2,
    // fract_digits=4+2=6 → "-0.001234"
    let result: String = Fmt::format_f64(-0.001_234, 4);
    assert_eq!(result, "-0.001234");
}

// =========================================================================
//  Fmt::format_f64 - special values
// =========================================================================

// --- format_f64_nan_passthrough ---

#[test]
fn format_f64_nan_passthrough() {
    let result: String = Fmt::format_f64(f64::NAN, 4);
    assert_eq!(result, "NaN");
}

// --- format_f64_infinity_passthrough ---

#[test]
fn format_f64_infinity_passthrough() {
    let result: String = Fmt::format_f64(f64::INFINITY, 4);
    assert_eq!(result, "inf");
}

// --- format_f64_neg_infinity_passthrough ---

#[test]
fn format_f64_neg_infinity_passthrough() {
    let result: String = Fmt::format_f64(f64::NEG_INFINITY, 4);
    assert_eq!(result, "-inf");
}

// =========================================================================
//  Fmt::format_bytes
// =========================================================================

// --- format_bytes_decimal ---

#[test]
fn format_bytes_decimal() {
    let result: String = Fmt::format_bytes(1000.0, 4, BytesFormat::Decimal);
    assert_eq!(result, "1 KB");
}

// --- format_bytes_binary ---

#[test]
fn format_bytes_binary() {
    let result: String = Fmt::format_bytes(1024.0, 4, BytesFormat::Binary);
    assert_eq!(result, "1 KiB");
}

// =========================================================================
//  Scale::from_value
// =========================================================================

// --- scale_value_ranges ---

#[test]
fn scale_value_ranges() {
    let (val, scale): (f64, Scale) = Scale::from_value(1.0, BytesFormat::Decimal);
    assert_eq!(scale, Scale::One);
    assert!((val - 1.0).abs() < f64::EPSILON);

    let (val, scale): (f64, Scale) = Scale::from_value(1_000.0, BytesFormat::Decimal);
    assert_eq!(scale, Scale::Kilo);
    assert!((val - 1.0).abs() < f64::EPSILON);

    let (val, scale): (f64, Scale) = Scale::from_value(1_000_000.0, BytesFormat::Decimal);
    assert_eq!(scale, Scale::Mega);
    assert!((val - 1.0).abs() < f64::EPSILON);

    let (val, scale): (f64, Scale) = Scale::from_value(1_000_000_000.0, BytesFormat::Decimal);
    assert_eq!(scale, Scale::Giga);
    assert!((val - 1.0).abs() < f64::EPSILON);

    let (val, scale): (f64, Scale) = Scale::from_value(1e12, BytesFormat::Decimal);
    assert_eq!(scale, Scale::Tera);
    assert!((val - 1.0).abs() < f64::EPSILON);

    let (val, scale): (f64, Scale) = Scale::from_value(1e15, BytesFormat::Decimal);
    assert_eq!(scale, Scale::Peta);
    assert!((val - 1.0).abs() < f64::EPSILON);
}

// --- scale_value_binary_thresholds ---

#[test]
fn scale_value_binary_thresholds() {
    let (_val, scale): (f64, Scale) = Scale::from_value(1023.0, BytesFormat::Binary);
    assert_eq!(scale, Scale::One);

    let (val, scale): (f64, Scale) = Scale::from_value(1024.0, BytesFormat::Binary);
    assert_eq!(scale, Scale::Kilo);
    assert!((val - 1.0).abs() < f64::EPSILON);
}

// --- scale_value_nan ---

#[test]
fn scale_value_nan() {
    let (val, scale): (f64, Scale) = Scale::from_value(f64::NAN, BytesFormat::Decimal);

    assert_eq!(scale, Scale::One);
    assert!(val.is_nan());
}

// --- scale_value_negative ---

#[test]
fn scale_value_negative() {
    let (val, scale): (f64, Scale) = Scale::from_value(-500.0, BytesFormat::Decimal);

    assert_eq!(scale, Scale::One);
    assert!((val - (-500.0)).abs() < f64::EPSILON);
}

// --- scale_value_zero ---

#[test]
fn scale_value_zero() {
    let (val, scale): (f64, Scale) = Scale::from_value(0.0, BytesFormat::Decimal);

    assert_eq!(scale, Scale::One);
    assert!((val - 0.0).abs() < f64::EPSILON);
}

// =========================================================================
//  ScaleFormat
// =========================================================================

// --- scale_format_bytes_format ---

#[test]
fn scale_format_bytes_format() {
    let sf: ScaleFormat = ScaleFormat::Bytes(BytesFormat::Binary);
    assert!(matches!(sf.bytes_format(), BytesFormat::Binary));

    let sf: ScaleFormat = ScaleFormat::BytesThroughput(BytesFormat::Decimal);
    assert!(matches!(sf.bytes_format(), BytesFormat::Decimal));

    let sf: ScaleFormat = ScaleFormat::CharsThroughput;
    assert!(matches!(sf.bytes_format(), BytesFormat::Decimal));

    let sf: ScaleFormat = ScaleFormat::CyclesThroughput;
    assert!(matches!(sf.bytes_format(), BytesFormat::Decimal));

    let sf: ScaleFormat = ScaleFormat::ItemsThroughput;
    assert!(matches!(sf.bytes_format(), BytesFormat::Decimal));
}

// --- scale_format_throughput ---

#[test]
fn scale_format_throughput() {
    assert!(matches!(
        ScaleFormat::throughput(CounterKind::Bytes, BytesFormat::Binary),
        ScaleFormat::BytesThroughput(BytesFormat::Binary)
    ));

    assert!(matches!(
        ScaleFormat::throughput(CounterKind::Chars, BytesFormat::Decimal),
        ScaleFormat::CharsThroughput
    ));

    assert!(matches!(
        ScaleFormat::throughput(CounterKind::Cycles, BytesFormat::Decimal),
        ScaleFormat::CyclesThroughput
    ));

    assert!(matches!(
        ScaleFormat::throughput(CounterKind::Items, BytesFormat::Decimal),
        ScaleFormat::ItemsThroughput
    ));
}

// =========================================================================
//  DisplayThroughput
// =========================================================================

// --- throughput_display_bytes ---

#[test]
fn throughput_display_bytes() {
    let dt: DisplayThroughput = DisplayThroughput {
        kind: CounterKind::Bytes,
        count: 1_000_000.0,
        picos: 1_000_000_000.0,
        bytes_format: BytesFormat::Decimal,
    };

    let result: String = dt.to_string();
    assert!(result.contains("GB/s"), "expected GB/s in '{result}'");
}

// --- throughput_display_items ---

#[test]
fn throughput_display_items() {
    let dt: DisplayThroughput = DisplayThroughput {
        kind: CounterKind::Items,
        count: 500.0,
        picos: 1_000_000_000_000.0,
        bytes_format: BytesFormat::Decimal,
    };

    let result: String = dt.to_string();
    assert!(result.contains("item/s"), "expected item/s in '{result}'");
    assert!(result.contains("500"), "expected 500 in '{result}'");
}

// --- throughput_display_zero_count ---

#[test]
fn throughput_display_zero_count() {
    let dt: DisplayThroughput = DisplayThroughput {
        kind: CounterKind::Bytes,
        count: 0.0,
        picos: 1_000_000_000.0,
        bytes_format: BytesFormat::Decimal,
    };

    let result: String = dt.to_string();
    assert!(result.contains("B/s"), "expected B/s in '{result}'");
}

// --- throughput_display_cycles ---

#[test]
fn throughput_display_cycles() {
    let dt: DisplayThroughput = DisplayThroughput {
        kind: CounterKind::Cycles,
        count: 1_000_000_000.0,
        picos: 1_000_000_000_000.0,
        bytes_format: BytesFormat::Decimal,
    };

    let result: String = dt.to_string();
    assert!(result.contains("GHz"), "expected GHz in '{result}'");
}

// --- throughput_display_bytes_decimal_exact ---

#[test]
fn throughput_display_bytes_decimal_exact() {
    // 1_000_000 bytes at 1 ms → 1e9 B/s = 1 GB/s
    let dt: DisplayThroughput = DisplayThroughput {
        kind: CounterKind::Bytes,
        count: 1_000_000.0,
        picos: 1_000_000_000.0,
        bytes_format: BytesFormat::Decimal,
    };
    let result: String = dt.to_string();
    assert_eq!(result, "1 GB/s");
}

// --- throughput_display_bytes_binary_exact ---

#[test]
fn throughput_display_bytes_binary_exact() {
    let dt: DisplayThroughput = DisplayThroughput {
        kind: CounterKind::Bytes,
        count: 1_048_576.0,
        picos: 1_000_000.0,
        bytes_format: BytesFormat::Binary,
    };

    let result: String = dt.to_string();
    assert!(result.contains("GiB/s"), "expected GiB/s in '{result}'");
}

// --- throughput_display_zero_picos ---

#[test]
fn throughput_display_zero_picos() {
    let dt: DisplayThroughput = DisplayThroughput {
        kind: CounterKind::Items,
        count: 100.0,
        picos: 0.0,
        bytes_format: BytesFormat::Decimal,
    };

    let result: String = dt.to_string();
    assert!(result.contains('0'), "expected 0 in '{result}'");
}

// --- throughput_display_width_padding ---

#[test]
fn throughput_display_width_padding() {
    let dt: DisplayThroughput = DisplayThroughput {
        kind: CounterKind::Items,
        count: 500.0,
        picos: 1_000_000_000_000.0,
        bytes_format: BytesFormat::Decimal,
    };

    // Use width formatting: should pad with spaces.
    let result: String = format!("{dt:<20}");
    assert!(
        result.len() >= 20,
        "expected at least 20 chars, got {} in '{result}'",
        result.len()
    );
}
