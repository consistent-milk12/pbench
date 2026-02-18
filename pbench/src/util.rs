//! Utility types for internal use.
//!
//! These types are `pub` so they can appear in public type signatures
//! (type aliases in [`crate::entry`]), but they live in a `pub(crate)`
//! module, making them unreachable from outside the crate.

/// Marker type for unconfigured `Bencher` inputs.
///
/// Prevents users from accidentally passing `()` without calling
/// [`Bencher::with_inputs`](crate::bencher::Bencher::with_inputs) first.
/// Used by the entry system to default-parameterise `Bencher`.
#[non_exhaustive]
pub struct Unit;
