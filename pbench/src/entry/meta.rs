//! Entry metadata for benchmark registration.
//!
//! [`EntryMeta`] carries compile-time information about each benchmark or
//! group: its name, module path, and source location. [`EntryLocation`]
//! records the file, line, and column where the entry was defined.

/// Metadata common to `#[pbench::bench]` and `#[pbench::bench_group]` entries.
///
/// Populated at compile time by the proc macro via `module_path!()`,
/// `file!()`, `line!()`, and `column!()`.
#[derive(Clone, Copy)]
pub struct EntryMeta {
    /// The entry's original function or module name (e.g. `"my_bench"`).
    pub raw_name: &'static str,

    /// The entry's `module_path!()` value (e.g. `"my_crate::tests"`).
    pub module_path: &'static str,

    /// Source location where the entry was defined.
    pub location: EntryLocation,
}

impl EntryMeta {
    /// Split `module_path` into path components.
    ///
    /// For `"crate::mod_a::mod_b"`, yields `["crate", "mod_a", "mod_b"]`.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn module_path_components(&self) -> impl Iterator<Item = &'static str> {
        self.module_path.split("::")
    }
}

/// Source location of a benchmark or group entry.
#[derive(Clone, Copy, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct EntryLocation {
    /// Source file path.
    pub file: &'static str,

    /// Line number.
    pub line: u32,

    /// Column number.
    pub col: u32,
}
