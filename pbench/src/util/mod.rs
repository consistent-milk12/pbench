//! Utility types for internal use.
//!
//! These types are `pub` so they can appear in public type signatures
//! (type aliases in [`crate::entry`]), but they live in a `pub(crate)`
//! module, making them unreachable from outside the crate.

pub mod sort;

/// Marker type for unconfigured `Bencher` inputs.
///
/// Prevents users from accidentally passing `()` without calling
/// [`Bencher::with_inputs`](crate::bencher::Bencher::with_inputs) first.
/// Used by the entry system to default-parameterise `Bencher`.
#[non_exhaustive]
#[expect(
    dead_code,
    reason = "Public-in-private marker type for future entry system use"
)]
pub struct Unit;

/// RAII scope-exit guard that runs a closure on drop.
///
/// Used by the runner to ensure deferred cleanup (e.g. dropping collected
/// outputs) runs even on early return or panic.
pub struct Defer<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> Defer<F> {
    /// Create an RAII guard that calls `f` when dropped.
    #[must_use]
    pub(crate) const fn new(f: F) -> Self {
        Self(Some(f))
    }
}

impl<F: FnOnce()> Drop for Defer<F> {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            f();
        }
    }
}
