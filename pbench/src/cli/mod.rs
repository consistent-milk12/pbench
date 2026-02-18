//! Hand-rolled CLI argument parsing.
//!
//! Parses benchmark runner options from `std::env::args()`. No external
//! dependency (no clap) — keeps the dependency tree small at the cost of
//! manual validation and help text. Compensated with rigorous unit tests.

// =========================================================================
//  Public enums
// =========================================================================

use std::env as StdEnv;
use std::process as StdProcess;

/// Output format for benchmark results.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OutputFormat {
    /// Human-readable terminal table with tree structure.
    #[default]
    Table,

    /// Machine-readable JSON (requires `json` feature).
    Json,

    /// Machine-readable CSV.
    Csv,
}

/// Sort order for benchmark entries.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SortBy {
    /// Sort by display name using natural comparison
    /// (so `bench_2` sorts before `bench_10`).
    #[default]
    Name,

    /// Sort by median (p50) duration, fastest first.
    P50,

    /// Sort by p99 duration, fastest first.
    P99,

    /// Sort by mean duration, fastest first.
    Mean,
}

/// Control whether ignored benchmarks are run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RunIgnored {
    /// Skip ignored benchmarks (default).
    #[default]
    No,

    /// Run both ignored and non-ignored benchmarks.
    Yes,

    /// Run only ignored benchmarks.
    Only,
}

impl RunIgnored {
    /// Whether a benchmark with the given `ignored` flag should run.
    #[must_use]
    pub const fn should_run(self, ignored: bool) -> bool {
        match self {
            Self::No => !ignored,

            Self::Yes => true,

            Self::Only => ignored,
        }
    }
}

/// Bytes display format for throughput counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BytesFormat {
    /// Powers of 1000 (KB, MB, GB).
    #[default]
    Decimal,

    /// Powers of 1024 (KiB, MiB, GiB).
    Binary,
}

// =========================================================================
//  CliArgs
// =========================================================================

/// Parsed CLI arguments for the benchmark runner.
#[derive(Clone, Debug)]
pub struct CliArgs {
    /// Inclusive filter: only run benchmarks matching this pattern.
    pub filter: Option<String>,

    /// Exclusive filters: skip benchmarks matching these patterns.
    /// Takes priority over `filter`.
    pub skip: Vec<String>,

    /// Print benchmark names with tree structure, then exit.
    pub list: bool,

    /// Print benchmark names one per line (nextest-compatible).
    pub list_terse: bool,

    /// Run benchmarks once without timing (compile/run verification).
    pub test: bool,

    /// Whether to run ignored benchmarks.
    pub run_ignored: RunIgnored,

    /// Output format for results.
    pub output_format: OutputFormat,

    /// Bytes display format for throughput counters.
    pub bytes_format: BytesFormat,

    /// Save results as a named baseline.
    pub save_baseline: Option<String>,

    /// Compare results against a named baseline.
    pub baseline: Option<String>,

    /// Regression threshold percentage.
    pub threshold: f64,

    /// Sort order for benchmark entries.
    pub sort: SortBy,

    /// CLI override for sample count.
    pub sample_count: Option<u32>,

    /// CLI override for sample size (iterations per sample).
    pub sample_size: Option<u32>,
}

impl Default for CliArgs {
    fn default() -> Self {
        Self {
            filter: None,
            skip: Vec::new(),
            list: false,
            list_terse: false,
            test: false,
            run_ignored: RunIgnored::default(),
            output_format: OutputFormat::default(),
            bytes_format: BytesFormat::default(),
            save_baseline: None,
            baseline: None,
            threshold: 5.0,
            sort: SortBy::default(),
            sample_count: None,
            sample_size: None,
        }
    }
}

// =========================================================================
//  Parse error
// =========================================================================

/// CLI parse error with a human-readable message.
#[derive(Debug)]
pub struct ParseError {
    /// Error message to display.
    pub message: String,
}

impl ParseError {
    /// Create a new parse error.
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

// =========================================================================
//  Parsing implementation
// =========================================================================

impl CliArgs {
    /// Parse from a slice of string arguments (for testing).
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if an argument is invalid or missing a value.
    #[expect(
        clippy::too_many_lines,
        reason = "Flat match-arm dispatch; splitting would obscure the 1:1 flag→field mapping"
    )]
    pub fn parse_from(args: &[&str]) -> Result<Self, ParseError> {
        let mut result: Self = Self::default();
        let mut i: usize = 0;

        while i < args.len() {
            let arg: &str = args[i];

            match arg {
                "--filter" => {
                    let val: &str = Self::next_value(args, &mut i, "--filter")?;
                    result.filter = Some(val.to_owned());
                }

                "--skip" => {
                    let val: &str = Self::next_value(args, &mut i, "--skip")?;
                    result.skip.push(val.to_owned());
                }

                "--list" => {
                    result.list = true;
                }

                "--test" => {
                    result.test = true;
                }

                "--ignored" => {
                    result.run_ignored = RunIgnored::Only;
                }

                "--include-ignored" => {
                    result.run_ignored = RunIgnored::Yes;
                }

                "--output" => {
                    let val: &str = Self::next_value(args, &mut i, "--output")?;

                    result.output_format = match val {
                        "table" => OutputFormat::Table,

                        "json" => OutputFormat::Json,

                        "csv" => OutputFormat::Csv,

                        other => {
                            return Err(ParseError::new(format!(
                                "invalid output format '{other}'. Valid formats: table, json, csv"
                            )));
                        }
                    };
                }

                "--bytes-format" => {
                    let val: &str = Self::next_value(args, &mut i, "--bytes-format")?;

                    result.bytes_format = match val {
                        "decimal" => BytesFormat::Decimal,

                        "binary" => BytesFormat::Binary,

                        other => {
                            return Err(ParseError::new(format!(
                                "invalid bytes format '{other}'. Valid formats: decimal, binary"
                            )));
                        }
                    };
                }

                "--save-baseline" => {
                    let val: &str = Self::next_value(args, &mut i, "--save-baseline")?;

                    result.save_baseline = Some(val.to_owned());
                }

                "--baseline" => {
                    let val: &str = Self::next_value(args, &mut i, "--baseline")?;

                    result.baseline = Some(val.to_owned());
                }

                "--threshold" => {
                    let val: &str = Self::next_value(args, &mut i, "--threshold")?;

                    let parsed: f64 =
                        val.parse::<f64>().map_err(|_: std::num::ParseFloatError| {
                            ParseError::new(format!(
                                "invalid threshold '{val}'. Expected a number (e.g. 5.0)"
                            ))
                        })?;

                    result.threshold = parsed;
                }

                "--sort" => {
                    let val: &str = Self::next_value(args, &mut i, "--sort")?;
                    result.sort = match val {
                        "name" => SortBy::Name,

                        "p50" => SortBy::P50,

                        "p99" => SortBy::P99,

                        "mean" => SortBy::Mean,

                        other => {
                            return Err(ParseError::new(format!(
                                "invalid sort option '{other}'. Valid options: name, p50, p99, mean"
                            )));
                        }
                    };
                }

                "--sample-count" => {
                    let val: &str = Self::next_value(args, &mut i, "--sample-count")?;

                    let parsed: u32 =
                        val.parse::<u32>().map_err(|_: std::num::ParseIntError| {
                            ParseError::new(format!(
                                "invalid sample count '{val}'. Expected a positive integer"
                            ))
                        })?;

                    result.sample_count = Some(parsed);
                }

                "--sample-size" => {
                    let val: &str = Self::next_value(args, &mut i, "--sample-size")?;

                    let parsed: u32 =
                        val.parse::<u32>().map_err(|_: std::num::ParseIntError| {
                            ParseError::new(format!(
                                "invalid sample size '{val}'. Expected a positive integer"
                            ))
                        })?;

                    result.sample_size = Some(parsed);
                }

                "--format" => {
                    let val: &str = Self::next_value(args, &mut i, "--format")?;

                    match val {
                        "terse" => {
                            result.list_terse = true;
                        }

                        "pretty" => {
                            // Default list format, no-op.
                        }

                        other => {
                            return Err(ParseError::new(format!(
                                "invalid format '{other}'. Valid formats: pretty, terse"
                            )));
                        }
                    }
                }

                "--help" | "-h" => {
                    Self::print_help();

                    StdProcess::exit(0);
                }

                // Silently ignore known cargo/test harness flags.
                "--bench" | "--nocapture" | "--show-output" => {}

                other if other.starts_with("--") => {
                    return Err(ParseError::new(format!(
                        "unknown flag '{other}'. Available flags: --filter, --skip, --list, \
                         --test, --output, --sort, --threshold, --save-baseline, --baseline, \
                         --sample-count, --sample-size, --ignored, --include-ignored, \
                         --bytes-format, --format, --help"
                    )));
                }

                // Positional argument treated as filter (same as cargo bench behavior).
                _ => {
                    if result.filter.is_none() {
                        result.filter = Some(arg.to_owned());
                    }
                }
            }

            i += 1;
        }

        Ok(result)
    }

    /// Extract the next value after a flag, advancing the index.
    fn next_value<'a>(args: &[&'a str], i: &mut usize, flag: &str) -> Result<&'a str, ParseError> {
        *i += 1;
        args.get(*i)
            .copied()
            .ok_or_else(|| ParseError::new(format!("flag '{flag}' requires a value")))
    }

    /// Parse CLI arguments from `std::env::args()`.
    ///
    /// Skips the first argument (binary name) and any arguments before `--`
    /// (which `cargo bench` uses to separate cargo flags from benchmark flags).
    ///
    /// On parse error, prints the error to stderr and exits with code 1.
    #[must_use]
    pub fn parse_env() -> Self {
        let raw_args: Vec<String> = StdEnv::args().collect();

        // Find `--` separator. Everything after it is for us.
        // If no `--`, take all args after the binary name.
        let bench_args: &[String] = raw_args
            .iter()
            .position(|a: &String| a == "--")
            .map_or_else(
                || {
                    // No separator — skip binary name.
                    if raw_args.len() > 1 {
                        &raw_args[1..]
                    } else {
                        &raw_args[..0]
                    }
                },
                |pos: usize| &raw_args[pos + 1..],
            );

        let args_strs: Vec<&str> = bench_args.iter().map(String::as_str).collect();

        match Self::parse_from(&args_strs) {
            Ok(args) => args,

            Err(e) => {
                eprintln!("error: {e}");
                eprintln!("Run with --help for usage information.");

                StdProcess::exit(1);
            }
        }
    }

    /// Print usage help to stderr.
    fn print_help() {
        eprintln!(
            "\
pbench — Percentile-focused benchmarking for Rust

USAGE:
    cargo bench [-- <OPTIONS>]

OPTIONS:
    --filter <PATTERN>       Only run benchmarks matching PATTERN
    --skip <PATTERN>         Skip benchmarks matching PATTERN (repeatable)
    --list                   List benchmarks without running
    --format <FORMAT>        List format: pretty (default), terse
    --test                   Run benchmarks once without timing
    --output <FORMAT>        Output format: table (default), json, csv
    --sort <ATTR>            Sort order: name (default), p50, p99, mean
    --sample-count <N>       Number of samples (default: 1000)
    --sample-size <N>        Iterations per sample (default: adaptive)
    --threshold <PCT>        Regression threshold percentage (default: 5.0)
    --save-baseline <NAME>   Save results as named baseline
    --baseline <NAME>        Compare results against named baseline
    --ignored                Run only ignored benchmarks
    --include-ignored        Run both ignored and non-ignored benchmarks
    --bytes-format <FMT>     Bytes display: decimal (default), binary
    -h, --help               Print this help message"
        );
    }
}

#[cfg(test)]
mod unit_tests;
