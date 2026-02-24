//! Hierarchical entry tree for benchmark organisation and display.
//!
//! [`EntryTree`] groups benchmark entries by their `module_path` components,
//! creating a tree suitable for indented terminal output, options inheritance,
//! and filter/sort operations.

use std::cmp::Ordering;
use std::collections::HashMap;

use super::meta::EntryMeta;
use super::{AnyBenchEntry, GroupEntry};
use crate::cli::SortBy;
use crate::time::FineDuration;
use crate::util::sort::NaturalCmp;

/// Append a path component to `buf`, separating with `::` if non-empty.
///
/// Centralizes the separator rule for the save-extend-use-truncate
/// buffer pattern used throughout tree traversal.
pub(crate) fn push_path_component(buf: &mut String, name: &str) {
    if !buf.is_empty() {
        buf.push_str("::");
    }
    buf.push_str(name);
}

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
        // Inner recursive implementation with accumulated path buffer.
        fn retain_inner(
            tree: &mut Vec<EntryTree>,
            buf: &mut String,
            filter: &mut dyn FnMut(&str) -> bool,
        ) {
            tree.retain_mut(|subtree: &mut EntryTree| {
                let saved: usize = buf.len();
                push_path_component(buf, subtree.raw_name());

                let keep: bool = match subtree {
                    EntryTree::Parent { children, .. } => {
                        retain_inner(children, buf, filter);

                        !children.is_empty()
                    }

                    EntryTree::Leaf { .. } => filter(buf),
                };

                debug_assert!(
                    buf.len() >= saved,
                    "buffer was modified beyond truncate point"
                );
                buf.truncate(saved);
                keep
            });
        }

        let mut buf: String = String::new();
        retain_inner(tree, &mut buf, &mut filter);
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

                // Stats-based sorting deferred to sort_by_stats() which runs
                // after benchmark collection. Pre-sort always uses name order.
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

    /// Sort entries by timing statistics from benchmark results.
    ///
    /// For `SortBy::Name`, delegates to [`sort`](Self::sort). For stats-based
    /// variants (`P50`, `P99`, `Mean`), looks up each leaf's statistic in
    /// `stats_map` (keyed by full path). Leaves with stats sort before those
    /// without, ascending (fastest first). Ties break by name (natural order)
    /// for deterministic output. Parents remain sorted by name.
    ///
    /// **Multi-thread policy:** When a benchmark runs with multiple thread
    /// counts, the `stats_map` should contain the fastest (minimum) stat
    /// across all thread counts for that benchmark name. This reflects
    /// best-case performance for sorting purposes.
    ///
    /// Uses a decorate-sort-undecorate pattern: each element's sort key is
    /// computed once (not per-comparison), avoiding `O(n log n)` allocations
    /// and hash lookups in the comparator.
    ///
    /// Must be called after benchmarks have run and `stats_map` has been
    /// populated from the collected `BenchRecord`s.
    pub(crate) fn sort_by_stats(
        tree: &mut [Self],
        sort_by: SortBy,
        stats_map: &HashMap<String, FineDuration>,
        buf: &mut String,
    ) {
        if sort_by == SortBy::Name {
            Self::sort(tree, sort_by);
            return;
        }

        // Decorate: build sort key for each element once.
        // Key: (kind, Option<picos>) — kind=0 for Leaf, 1 for Parent.
        // Parents always sort after Leaves (kind ordering), then by name.
        // Leaves sort by stat ascending, missing stats sort last.
        let keys: Vec<(u8, Option<u128>)> = tree
            .iter()
            .map(|node: &Self| {
                let kind: u8 = node.kind();

                if kind == 1 {
                    // Parent — no stat key, sorted by name below.
                    return (kind, None);
                }

                let saved: usize = buf.len();
                push_path_component(buf, node.raw_name());

                let stat: Option<u128> =
                    stats_map.get(buf.as_str()).map(|d: &FineDuration| d.picos);

                debug_assert!(
                    buf.len() >= saved,
                    "buffer was modified beyond truncate point"
                );
                buf.truncate(saved);

                (kind, stat)
            })
            .collect();

        // Sort: use cached keys with name as tiebreaker.
        let mut indices: Vec<usize> = (0..tree.len()).collect();
        indices.sort_by(|&ai: &usize, &bi: &usize| {
            let (a_kind, ref a_stat) = keys[ai];
            let (b_kind, ref b_stat) = keys[bi];

            // Leaves before Parents.
            let kind_cmp: Ordering = a_kind.cmp(&b_kind);
            if kind_cmp != Ordering::Equal {
                return kind_cmp;
            }

            // Both Parents — sort by name.
            if a_kind == 1 {
                return NaturalCmp::compare(tree[ai].raw_name(), tree[bi].raw_name());
            }

            // Both Leaves — sort by stat, missing last, name tiebreak.
            match (a_stat, b_stat) {
                (Some(a_picos), Some(b_picos)) => a_picos
                    .cmp(b_picos)
                    .then_with(|| NaturalCmp::compare(tree[ai].raw_name(), tree[bi].raw_name())),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => NaturalCmp::compare(tree[ai].raw_name(), tree[bi].raw_name()),
            }
        });

        // Undecorate: reorder tree in-place using the sorted indices.
        // Apply permutation by cycling.
        let mut placed: Vec<bool> = vec![false; tree.len()];
        for start in 0..tree.len() {
            if placed[start] || indices[start] == start {
                placed[start] = true;
                continue;
            }

            let mut current: usize = start;
            loop {
                let target: usize = indices[current];
                indices[current] = current;
                placed[current] = true;

                if target == start {
                    break;
                }

                tree.swap(current, target);
                current = target;
            }
        }

        drop(keys);

        // Recursively sort children of Parent nodes.
        for node in tree.iter_mut() {
            if let Self::Parent {
                raw_name, children, ..
            } = node
            {
                let saved: usize = buf.len();
                push_path_component(buf, raw_name);

                Self::sort_by_stats(children, sort_by, stats_map, buf);

                debug_assert!(
                    buf.len() >= saved,
                    "buffer was modified beyond truncate point"
                );
                buf.truncate(saved);
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
    #[allow(dead_code, reason = "Useful accessor for tree traversal by consumers")]
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
    #[allow(
        dead_code,
        reason = "Useful accessor for group inspection by consumers"
    )]
    pub(crate) const fn group(&self) -> Option<&'static GroupEntry> {
        match self {
            Self::Parent { group, .. } => *group,

            Self::Leaf { .. } => None,
        }
    }
}

#[cfg(test)]
mod unit_tests;
