//! Percentile-focused benchmarking for Rust.
//!
//! **pbench** reports precise tail-latency statistics - p50, p95, p99, p99.9,
//! and p99.99, instead of just min/max/median/mean. This makes it easy to
//! spot outliers and understand the full latency distribution of your code.
//!
//! # Quick Start
//!
//! Add pbench to your `Cargo.toml`:
//!
//! ```toml
//! [dev-dependencies]
//! pbench = "0.1"
//! ```
//!
//! Create a benchmark file (e.g. `benches/my_bench.rs`):
//!
//! ```rust,ignore
//! use pbench::Bencher;
//!
//! #[pbench::bench]
//! fn my_function(b: &Bencher<'_>) {
//!     b.bench_refs(|| {
//!         // code to benchmark
//!     });
//! }
//!
//! fn main() {
//!     pbench::main();
//! }
//! ```
//!
//! Add to your `Cargo.toml`:
//!
//! ```toml
//! [[bench]]
//! name = "my_bench"
//! harness = false
//! ```
//!
//! Run with `cargo bench`.
//!
//! # Benchmark Patterns
//!
//! ## Simple closure
//!
//! ```rust,ignore
//! #[pbench::bench]
//! fn addition(b: &Bencher<'_>) {
//!     b.bench_refs(|| std::hint::black_box(1u64 + 2));
//! }
//! ```
//!
//! ## Deferred drop (`bench_values`)
//!
//! When the benchmarked code returns a value that needs dropping (e.g. a `Vec`),
//! use `bench_values` to defer the drop outside the measurement window:
//!
//! ```rust,ignore
//! #[pbench::bench]
//! fn alloc(b: &Bencher<'_>) {
//!     b.bench_values(|| Vec::<u8>::with_capacity(1024));
//! }
//! ```
//!
//! ## Throughput counters
//!
//! Attach a counter for items/s or bytes/s reporting:
//!
//! ```rust,ignore
//! use pbench::{Bencher, ItemsCount};
//!
//! #[pbench::bench]
//! fn process(b: &Bencher<'_>) {
//!     b.counter(ItemsCount::new(1000));
//!     b.bench_refs(|| { /* process 1000 items */ });
//! }
//! ```
//!
//! ## Parameterised benchmarks (`args`)
//!
//! ```rust,ignore
//! #[pbench::bench(args = [10, 100, 1000])]
//! fn sized(b: &Bencher<'_>, n: &str) {
//!     let size: usize = n.parse().unwrap();
//!     b.bench_values(|| Vec::<u8>::with_capacity(size));
//! }
//! ```
//!
//! # Feature Flags
//!
//! | Feature | Description |
//! |---------|-------------|
//! | `json`  | Enables JSON output (`--output json`) and baseline save/load (`--save-baseline`, `--baseline`). Adds `serde` + `serde_json` dependencies. |
//!
//! # CLI Flags
//!
//! | Flag | Description |
//! |------|-------------|
//! | `--filter <pattern>` | Only run benchmarks matching pattern |
//! | `--skip <pattern>` | Skip benchmarks matching pattern (repeatable, takes priority over `--filter`) |
//! | `--output table\|json\|csv` | Output format (default: table) |
//! | `--sort name\|p50\|p99\|mean` | Sort order (default: name) |
//! | `--sample-count <n>` | Number of samples to collect |
//! | `--sample-size <n>` | Iterations per sample |
//! | `--list` | List benchmark names |
//! | `--list --format terse` | One benchmark per line (nextest-compatible) |
//! | `--test` | Verify benchmarks compile and run (no timing) |
//! | `--ignored` | Run only ignored benchmarks |
//! | `--include-ignored` | Run all benchmarks including ignored |
//! | `--bytes-format binary\|decimal` | Byte display format (default: decimal) |
//! | `--save-baseline <name>` | Save results (requires `json` feature) |
//! | `--baseline <name>` | Compare against saved baseline (requires `json` feature) |
//! | `--threshold <pct>` | Regression threshold percentage (default: 5.0) |

#![deny(missing_docs)]
#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]
#![warn(clippy::perf)]
// Tested functions only
#![allow(clippy::inline_always)]
#![allow(clippy::redundant_pub_crate)]

#[cfg(feature = "json")]
pub(crate) mod baseline;
pub mod bencher;
pub(crate) mod cli;
pub mod config;
pub mod counter;
pub mod entry;
pub(crate) mod output;
pub(crate) mod runner;
pub mod stats;
pub mod time;
pub(crate) mod util;

pub use pbench_macros::{bench, bench_group};

// Public API re-exports for convenience.
pub use bencher::{Bencher, BencherWithInput};
pub use config::BenchOptions;
pub use counter::{BytesCount, CharsCount, Counter, CyclesCount, ItemsCount};

use crate::{cli::CliArgs, runner::Runner};

/// Private re-exports consumed by macro-generated code.
///
/// **Do not use directly.** These items are public only because the
/// proc macro (an external crate) generates code that references them.
/// Semver guarantees do not apply to this module.
#[doc(hidden)]
pub mod __private {
    pub use crate::bencher::Bencher;
    pub use crate::config::BenchOptions;
    pub use crate::entry::meta::EntryLocation;
    pub use crate::entry::{
        AnyBenchEntry, BENCH_ENTRIES, BenchEntry, EntryList, EntryMeta, GenericBenchEntry,
        GroupEntry,
    };

    use std::fmt as StdFmt;

    /// Helper for polymorphic string conversion of benchmark arguments.
    ///
    /// Uses method resolution priority: if `T: ToString`, the inherent
    /// method is chosen over the trait method. Otherwise, the blanket
    /// `Display` impl (which delegates to `Debug`) provides `to_string()`
    /// through the `Display → ToString` blanket.
    pub struct ToStringHelper<'a, T: 'static>(
        /// The value to convert.
        pub &'a T,
    );

    /// Fallback: used when `T: Debug` but not `ToString`.
    impl<T: StdFmt::Debug> StdFmt::Display for ToStringHelper<'_, T> {
        fn fmt(&self, f: &mut StdFmt::Formatter<'_>) -> StdFmt::Result {
            write!(f, "{:?}", self.0)
        }
    }

    /// Preferred path: when `T: ToString`, this inherent method wins
    /// over the trait method from `Display → ToString` blanket.
    #[expect(
        clippy::inherent_to_string,
        reason = "Intentional: inherent method wins over Display→ToString blanket for polymorphic dispatch"
    )]
    impl<T: ToString> ToStringHelper<'_, T> {
        /// Convert the wrapped value to a string.
        #[must_use]
        pub fn to_string(&self) -> String {
            self.0.to_string()
        }
    }

    /// Trait for polymorphic benchmark argument access.
    ///
    /// Handles auto-dereferencing and conversions so that
    /// `args = [1, 2, 3]` works with both value and reference parameters.
    pub trait Arg<T> {
        /// Extract the argument value.
        fn get(self) -> T;
    }

    /// Identity: `T` → `T`.
    impl<T> Arg<T> for T {
        #[inline]
        fn get(self) -> T {
            self
        }
    }

    /// Copy through one reference: `&T` → `T` where `T: Copy`.
    impl<T: Copy> Arg<T> for &T {
        #[inline]
        fn get(self) -> T {
            *self
        }
    }

    /// Copy through two references: `&&T` → `T` where `T: Copy`.
    impl<T: Copy> Arg<T> for &&T {
        #[inline]
        fn get(self) -> T {
            **self
        }
    }

    /// `&String` → `&str`.
    impl<'a> Arg<&'a str> for &'a String {
        #[inline]
        fn get(self) -> &'a str {
            self.as_str()
        }
    }
}

/// Entry point for benchmark binaries.
///
/// Parses CLI arguments, discovers registered benchmarks, and executes
/// the appropriate action (list, test, or bench). Call this from `main()`
/// in the benchmark harness generated by `#[pbench::bench]`.
pub fn main() {
    let args: CliArgs = CliArgs::parse_env();
    Runner::new(args).run();
}
