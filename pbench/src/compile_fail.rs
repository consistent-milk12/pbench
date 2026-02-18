//! Compile-fail tests for proc macro error diagnostics.
//!
//! Each doc test uses `compile_fail` to verify that invalid macro usage
//! produces a compile error rather than silently succeeding.

/// Duplicate options must be rejected.
///
/// ```compile_fail
/// #[pbench::bench(sample_count = 10, sample_count = 20)]
/// fn duplicate_option(b: &pbench::bencher::Bencher<'_>) {
///     b.bench_refs(|| {});
/// }
/// ```
const _DUPLICATE_OPTION: () = ();

/// Unknown options should produce a compile error (forwarded to
/// `BenchOptions` struct where the field does not exist).
///
/// ```compile_fail
/// #[pbench::bench(unknown_option = 5)]
/// fn unknown_option(b: &pbench::bencher::Bencher<'_>) {
///     b.bench_refs(|| {});
/// }
/// ```
const _UNKNOWN_OPTION: () = ();

/// `args` must be an array literal, not an arbitrary expression.
///
/// ```compile_fail
/// #[pbench::bench(args = "not_an_array")]
/// fn invalid_args(b: &pbench::bencher::Bencher<'_>, arg: &str) {
///     let _ = arg;
///     b.bench_refs(|| {});
/// }
/// ```
const _INVALID_ARGS_TYPE: () = ();
