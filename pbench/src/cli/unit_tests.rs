//! Unit tests for CLI argument parsing.

use super::*;

// --- parse_defaults ---

#[test]
fn parse_defaults() {
    let args: CliArgs = CliArgs::parse_from(&[]).unwrap();

    assert_eq!(args.filter, None);
    assert!(args.skip.is_empty());
    assert!(!args.list);
    assert!(!args.list_terse);
    assert!(!args.test);
    assert_eq!(args.run_ignored, RunIgnored::No);
    assert_eq!(args.output_format, OutputFormat::Table);
    assert_eq!(args.bytes_format, BytesFormat::Decimal);
    assert_eq!(args.save_baseline, None);
    assert_eq!(args.baseline, None);
    assert!((args.threshold - 5.0).abs() < f64::EPSILON);
    assert_eq!(args.sort, SortBy::Name);
    assert_eq!(args.sample_count, None);
    assert_eq!(args.sample_size, None);
}

// --- parse_filter ---

#[test]
fn parse_filter() {
    let args: CliArgs = CliArgs::parse_from(&["--filter", "hashmap"]).unwrap();

    assert_eq!(args.filter, Some("hashmap".to_owned()));
}

// --- parse_save_baseline ---

#[test]
fn parse_save_baseline() {
    let args: CliArgs = CliArgs::parse_from(&["--save-baseline", "main"]).unwrap();

    assert_eq!(args.save_baseline, Some("main".to_owned()));
}

// --- parse_output_json ---

#[test]
fn parse_output_json() {
    let args: CliArgs = CliArgs::parse_from(&["--output", "json"]).unwrap();

    assert_eq!(args.output_format, OutputFormat::Json);
}

// --- parse_threshold ---

#[test]
fn parse_threshold() {
    let args: CliArgs = CliArgs::parse_from(&["--threshold", "10"]).unwrap();

    assert!((args.threshold - 10.0).abs() < f64::EPSILON);
}

// --- parse_sort ---

#[test]
fn parse_sort() {
    let args: CliArgs = CliArgs::parse_from(&["--sort", "p99"]).unwrap();

    assert_eq!(args.sort, SortBy::P99);
}

// --- parse_list ---

#[test]
fn parse_list() {
    let args: CliArgs = CliArgs::parse_from(&["--list"]).unwrap();

    assert!(args.list);
}

// --- parse_test ---

#[test]
fn parse_test() {
    let args: CliArgs = CliArgs::parse_from(&["--test"]).unwrap();

    assert!(args.test);
}

// --- parse_invalid_output ---

#[test]
fn parse_invalid_output() {
    let err: ParseError = CliArgs::parse_from(&["--output", "xml"]).unwrap_err();

    assert!(
        err.message.contains("invalid output format"),
        "expected output format error, got: {}",
        err.message
    );
    assert!(
        err.message.contains("table, json, csv"),
        "expected valid formats listed, got: {}",
        err.message
    );
}

// --- parse_invalid_sort ---

#[test]
fn parse_invalid_sort() {
    let err: ParseError = CliArgs::parse_from(&["--sort", "invalid"]).unwrap_err();

    assert!(
        err.message.contains("invalid sort option"),
        "expected sort error, got: {}",
        err.message
    );
    assert!(
        err.message.contains("name, p50, p99, mean"),
        "expected valid sort options listed, got: {}",
        err.message
    );
}

// --- parse_invalid_threshold ---

#[test]
fn parse_invalid_threshold() {
    let err: ParseError = CliArgs::parse_from(&["--threshold", "abc"]).unwrap_err();

    assert!(
        err.message.contains("invalid threshold"),
        "expected threshold error, got: {}",
        err.message
    );
}

// --- parse_skip ---

#[test]
fn parse_skip() {
    let args: CliArgs = CliArgs::parse_from(&["--skip", "slow"]).unwrap();

    assert_eq!(args.skip, vec!["slow".to_owned()]);
}

// --- parse_skip_overrides_filter ---

#[test]
fn parse_skip_overrides_filter() {
    let args: CliArgs =
        CliArgs::parse_from(&["--filter", "bench", "--skip", "bench_slow"]).unwrap();

    assert_eq!(args.filter, Some("bench".to_owned()));
    assert_eq!(args.skip, vec!["bench_slow".to_owned()]);
}

// --- parse_ignored ---

#[test]
fn parse_ignored() {
    let args: CliArgs = CliArgs::parse_from(&["--ignored"]).unwrap();

    assert_eq!(args.run_ignored, RunIgnored::Only);
}

// --- parse_include_ignored ---

#[test]
fn parse_include_ignored() {
    let args: CliArgs = CliArgs::parse_from(&["--include-ignored"]).unwrap();

    assert_eq!(args.run_ignored, RunIgnored::Yes);
}

// --- parse_list_terse ---

#[test]
fn parse_list_terse() {
    let args: CliArgs = CliArgs::parse_from(&["--list", "--format", "terse"]).unwrap();

    assert!(args.list);
    assert!(args.list_terse);
}

// --- parse_bytes_format ---

#[test]
fn parse_bytes_format() {
    let args: CliArgs = CliArgs::parse_from(&["--bytes-format", "binary"]).unwrap();

    assert_eq!(args.bytes_format, BytesFormat::Binary);
}

// --- parse_unknown_flag ---

#[test]
fn parse_unknown_flag() {
    let err: ParseError = CliArgs::parse_from(&["--unknown"]).unwrap_err();

    assert!(
        err.message.contains("unknown flag"),
        "expected unknown flag error, got: {}",
        err.message
    );
    assert!(
        err.message.contains("--filter"),
        "expected available flags listed, got: {}",
        err.message
    );
}
