//! Hierarchical entry tree for benchmark organisation and display.
//!
//! [`EntryTree`] groups benchmark entries by their `module_path` components,
//! creating a tree suitable for indented terminal output, options inheritance,
//! and filter/sort operations.

use std::cmp::Ordering;
use std::iter::Peekable;
use std::str::Chars;

use super::meta::EntryMeta;
use super::{AnyBenchEntry, GroupEntry};
use crate::cli::SortBy;

/// Hierarchical tree of benchmark entries organised by module path.
///
/// Built from a flat list of [`AnyBenchEntry`] via [`from_entries`](Self::from_entries).
/// Parent nodes represent modules; leaf nodes represent individual benchmarks.
pub enum EntryTree {
    /// Parent node (module or benchmark group).
    Parent {
        /// Module or group name (a single path component).
        raw_name: &'static str,

        /// Optional group entry for configuration inheritance.
        group: Option<&'static GroupEntry>,

        /// Child entries and sub-groups.
        children: Vec<Self>,
    },

    /// Leaf node (individual benchmark).
    Leaf {
        /// The benchmark entry.
        entry: AnyBenchEntry,
    },
}

impl EntryTree {
    // =====================================================================
    //  Construction
    // =====================================================================

    /// Build a tree from a flat list of entries.
    ///
    /// Separates groups from bench/generic entries, constructs the tree
    /// from benches and generics by splitting each entry's `module_path`
    /// into components, then inserts groups into matching parent nodes.
    #[must_use]
    pub(crate) fn from_entries(entries: &[AnyBenchEntry]) -> Vec<Self> {
        let mut tree: Vec<Self> = Vec::new();
        let mut groups: Vec<&'static GroupEntry> = Vec::new();

        for &entry in entries {
            match entry {
                AnyBenchEntry::Group(group) => {
                    groups.push(group);
                }

                bench_or_generic => {
                    let meta: &EntryMeta = bench_or_generic.meta();
                    let mut components = meta.module_path_components();

                    Self::insert_entry(&mut tree, bench_or_generic, &mut components);
                }
            }
        }

        for group in groups {
            Self::insert_group(&mut tree, group);
        }

        tree
    }

    /// Recursive helper: insert an entry into the tree by consuming
    /// remaining module path components.
    fn insert_entry(
        tree: &mut Vec<Self>,
        entry: AnyBenchEntry,
        rem_modules: &mut dyn Iterator<Item = &'static str>,
    ) {
        let Some(current_module) = rem_modules.next() else {
            // No more path components — insert the leaf here.
            tree.push(Self::Leaf { entry });

            return;
        };

        // Try to find an existing Parent for this module component.
        let existing: Option<&mut Vec<Self>> = Self::get_children(tree, current_module);

        if let Some(children) = existing {
            Self::insert_entry(children, entry, rem_modules);
        } else {
            tree.push(Self::from_path(entry, current_module, rem_modules));
        }
    }

    /// Build a chain of Parent nodes from remaining path components,
    /// terminating with a Leaf.
    fn from_path(
        entry: AnyBenchEntry,
        current_module: &'static str,
        rem_modules: &mut dyn Iterator<Item = &'static str>,
    ) -> Self {
        let child: Self = rem_modules
            .next()
            .map_or(Self::Leaf { entry }, |next_module: &'static str| {
                Self::from_path(entry, next_module, rem_modules)
            });

        Self::Parent {
            raw_name: current_module,
            group: None,
            children: vec![child],
        }
    }

    /// Find the `children` vec of an existing Parent matching `module`.
    fn get_children<'t>(tree: &'t mut [Self], module: &str) -> Option<&'t mut Vec<Self>> {
        tree.iter_mut().find_map(|node: &mut Self| match node {
            Self::Parent {
                raw_name, children, ..
            } if *raw_name == module => Some(children),
            _ => None,
        })
    }

    // =====================================================================
    //  Group insertion
    // =====================================================================

    /// Insert a group entry into the tree.
    ///
    /// Navigates through the group's `module_path` components to find the
    /// correct level, then matches `raw_name` to set the `group` slot on
    /// the corresponding Parent node.
    ///
    /// Groups that do not match an existing Parent are silently skipped.
    /// This prevents phantom parent nodes for groups without benchmarks.
    pub(crate) fn insert_group(mut tree: &mut [Self], group: &'static GroupEntry) {
        // Navigate through module_path components to reach the parent level.
        'component: for component in group.meta.module_path_components() {
            for subtree in tree.iter_mut() {
                match subtree {
                    Self::Parent {
                        raw_name, children, ..
                    } if component == *raw_name => {
                        tree = children;

                        continue 'component;
                    }

                    _ => {}
                }
            }

            // No match at this level — group has no corresponding parent.
            return;
        }

        // Find the Parent matching the group's raw_name.
        for subtree in tree.iter_mut() {
            match subtree {
                Self::Parent {
                    raw_name,
                    group: slot,
                    ..
                } if group.meta.raw_name == *raw_name => {
                    *slot = Some(group);
                    return;
                }
                _ => {}
            }
        }
    }

    // =====================================================================
    //  Filter / retain
    // =====================================================================

    /// Remove entries whose full path does not satisfy `filter`.
    ///
    /// Builds the full path by concatenating parent names with `::`.
    /// Leaves are retained if `filter(full_path)` returns `true`.
    /// Parents are retained only if they have remaining children.
    pub(crate) fn retain(tree: &mut Vec<Self>, mut filter: impl FnMut(&str) -> bool) {
        // Inner recursive implementation with accumulated parent path.
        fn retain_inner(
            tree: &mut Vec<EntryTree>,
            parent_path: &str,
            filter: &mut dyn FnMut(&str) -> bool,
        ) {
            tree.retain_mut(|subtree: &mut EntryTree| {
                let subtree_path_owned: String;
                let subtree_path: &str = if parent_path.is_empty() {
                    subtree.raw_name()
                } else {
                    subtree_path_owned = format!("{parent_path}::{}", subtree.raw_name());
                    &subtree_path_owned
                };

                match subtree {
                    EntryTree::Parent { children, .. } => {
                        retain_inner(children, subtree_path, filter);

                        !children.is_empty()
                    }

                    EntryTree::Leaf { .. } => filter(subtree_path),
                }
            });
        }

        retain_inner(tree, "", &mut filter);
    }

    // =====================================================================
    //  Sort
    // =====================================================================

    /// Sort entries within each group.
    ///
    /// Leaves sort before Parents (benchmarks before sub-groups).
    /// Within the same kind, entries are sorted according to `sort_by`.
    pub(crate) fn sort(tree: &mut [Self], sort_by: SortBy) {
        tree.sort_by(|a: &Self, b: &Self| {
            // Leaves (kind=0) before Parents (kind=1).
            let kind_cmp: Ordering = a.kind().cmp(&b.kind());

            if kind_cmp != Ordering::Equal {
                return kind_cmp;
            }

            match sort_by {
                SortBy::Name => NaturalCmp::compare(a.raw_name(), b.raw_name()),

                // Stats-based sorting (P50, P99, Mean) falls back to name
                // order until the runner wires in actual timing data.
                SortBy::P50 | SortBy::P99 | SortBy::Mean => {
                    NaturalCmp::compare(a.raw_name(), b.raw_name())
                }
            }
        });

        // Recursively sort children of Parent nodes.
        for node in tree.iter_mut() {
            if let Self::Parent { children, .. } = node {
                Self::sort(children, sort_by);
            }
        }
    }

    // =====================================================================
    //  Accessors
    // =====================================================================

    /// The display name of this tree node.
    #[must_use]
    pub(crate) const fn raw_name(&self) -> &'static str {
        match self {
            Self::Parent { group: Some(g), .. } => g.meta.raw_name,

            Self::Parent { raw_name, .. } => raw_name,

            Self::Leaf { entry } => entry.raw_name(),
        }
    }

    /// Children of this node (empty slice for leaves).
    #[must_use]
    pub(crate) fn children(&self) -> &[Self] {
        match self {
            Self::Parent { children, .. } => children,

            Self::Leaf { .. } => &[],
        }
    }

    /// Enum variant discriminant for sort ordering.
    ///
    /// Returns `0` for Leaf (benchmarks first) and `1` for Parent
    /// (sub-groups after).
    const fn kind(&self) -> u8 {
        match self {
            Self::Leaf { .. } => 0,

            Self::Parent { .. } => 1,
        }
    }

    /// The group entry attached to this Parent, if any.
    #[must_use]
    pub(crate) const fn group(&self) -> Option<&'static GroupEntry> {
        match self {
            Self::Parent { group, .. } => *group,

            Self::Leaf { .. } => None,
        }
    }
}

// =========================================================================
//  Natural string comparison (numeric-aware)
// =========================================================================

/// Natural string comparison that sorts numeric subsequences by value.
///
/// So `"bench_2"` sorts before `"bench_10"` (unlike lexicographic order).
///
/// TODO: Move to `util/sort.rs` later.
struct NaturalCmp;

impl NaturalCmp {
    /// Compare two strings with natural (numeric-aware) ordering.
    fn compare(a: &str, b: &str) -> Ordering {
        let mut a_chars: Peekable<Chars<'_>> = a.chars().peekable();
        let mut b_chars: Peekable<Chars<'_>> = b.chars().peekable();

        loop {
            match (a_chars.peek(), b_chars.peek()) {
                (None, None) => return Ordering::Equal,

                (None, Some(_)) => return Ordering::Less,

                (Some(_), None) => return Ordering::Greater,

                (Some(&ac), Some(&bc)) => {
                    if ac.is_ascii_digit() && bc.is_ascii_digit() {
                        let a_num: u64 = Self::extract_number(&mut a_chars);
                        let b_num: u64 = Self::extract_number(&mut b_chars);
                        let cmp: Ordering = a_num.cmp(&b_num);

                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    } else {
                        a_chars.next();
                        b_chars.next();

                        let cmp: Ordering = ac.cmp(&bc);

                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                }
            }
        }
    }

    /// Extract a contiguous run of ASCII digits as a `u64`.
    fn extract_number(chars: &mut Peekable<Chars<'_>>) -> u64 {
        let mut n: u64 = 0;

        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit() {
                n = n
                    .saturating_mul(10)
                    .saturating_add(u64::from(c as u32 - '0' as u32));
                chars.next();
            } else {
                break;
            }
        }

        n
    }
}

#[cfg(test)]
mod unit_tests;
