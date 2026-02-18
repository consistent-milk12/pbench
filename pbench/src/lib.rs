//! Library root.

#![deny(missing_docs)]
#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]
#![warn(clippy::perf)]
// Tested functions only
#![allow(clippy::inline_always)]

pub mod bencher;
pub(crate) mod cli;
pub mod config;
pub mod counter;
pub mod entry;
pub mod stats;
pub mod time;
pub(crate) mod util;

pub use pbench_macros::{bench, bench_group};

#[cfg(doc)]
mod compile_fail;

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

    use std::fmt;

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
    impl<T: fmt::Debug> fmt::Display for ToStringHelper<'_, T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
