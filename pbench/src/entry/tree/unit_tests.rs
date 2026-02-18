//! Unit tests for [`EntryTree`].

use super::*;
use crate::bencher::Bencher;
use crate::config::BenchOptions;
use crate::entry::meta::{EntryLocation, EntryMeta};
use crate::entry::{AnyBenchEntry, BenchEntry, GroupEntry};

struct TestHelper;

impl TestHelper {
    fn dummy_bench(_b: &Bencher<'_>) {}

    fn group_opts() -> BenchOptions {
        BenchOptions {
            sample_count: Some(500),
            ..BenchOptions::default()
        }
    }
}

const LOC: EntryLocation = EntryLocation {
    file: "test.rs",
    line: 0,
    col: 0,
};

// --- from_entries: grouping ---

#[test]
fn entry_tree_grouping() {
    static BENCH_1: BenchEntry = BenchEntry {
        meta: EntryMeta {
            raw_name: "bench1",
            module_path: "mod_a",
            location: LOC,
        },
        bench_fn: TestHelper::dummy_bench,
        options: None,
    };

    static BENCH_2: BenchEntry = BenchEntry {
        meta: EntryMeta {
            raw_name: "bench2",
            module_path: "mod_a",
            location: LOC,
        },
        bench_fn: TestHelper::dummy_bench,
        options: None,
    };

    static BENCH_3: BenchEntry = BenchEntry {
        meta: EntryMeta {
            raw_name: "bench1",
            module_path: "mod_b",
            location: LOC,
        },
        bench_fn: TestHelper::dummy_bench,
        options: None,
    };

    let entries: Vec<AnyBenchEntry> = vec![
        AnyBenchEntry::Bench(&BENCH_1),
        AnyBenchEntry::Bench(&BENCH_2),
        AnyBenchEntry::Bench(&BENCH_3),
    ];

    let tree: Vec<EntryTree> = EntryTree::from_entries(&entries);

    // Top level: two Parent nodes (mod_a and mod_b).
    assert_eq!(tree.len(), 2);

    // mod_a has two children.
    let mod_a: &EntryTree = &tree[0];
    assert_eq!(mod_a.raw_name(), "mod_a");
    assert_eq!(mod_a.children().len(), 2);

    // mod_b has one child.
    let mod_b: &EntryTree = &tree[1];
    assert_eq!(mod_b.raw_name(), "mod_b");
    assert_eq!(mod_b.children().len(), 1);
}

// --- insert_group: options ---

#[test]
fn entry_tree_group_options() {
    static BENCH_1: BenchEntry = BenchEntry {
        meta: EntryMeta {
            raw_name: "bench1",
            module_path: "root::mod_a",
            location: LOC,
        },
        bench_fn: TestHelper::dummy_bench,
        options: None,
    };

    static GROUP: GroupEntry = GroupEntry {
        meta: EntryMeta {
            raw_name: "mod_a",
            module_path: "root",
            location: LOC,
        },
        options: Some(TestHelper::group_opts),
    };

    let entries: Vec<AnyBenchEntry> =
        vec![AnyBenchEntry::Bench(&BENCH_1), AnyBenchEntry::Group(&GROUP)];

    let tree: Vec<EntryTree> = EntryTree::from_entries(&entries);

    // Tree: Parent("root") → Parent("mod_a") → Leaf(bench1).
    assert_eq!(tree.len(), 1);

    let root: &EntryTree = &tree[0];
    assert_eq!(root.raw_name(), "root");
    assert_eq!(root.children().len(), 1);

    let mod_a: &EntryTree = &root.children()[0];
    assert_eq!(mod_a.raw_name(), "mod_a");

    // Group was inserted into the mod_a Parent.
    let group: &GroupEntry = mod_a.group().expect("mod_a should have a group entry");
    let opts: BenchOptions = (group.options.expect("group should have options"))();
    assert_eq!(opts.sample_count, Some(500));
}

// --- retain ---

#[test]
fn entry_tree_retain() {
    static BENCH_1: BenchEntry = BenchEntry {
        meta: EntryMeta {
            raw_name: "bench1",
            module_path: "mod_a",
            location: LOC,
        },
        bench_fn: TestHelper::dummy_bench,
        options: None,
    };

    static BENCH_2: BenchEntry = BenchEntry {
        meta: EntryMeta {
            raw_name: "bench2",
            module_path: "mod_a",
            location: LOC,
        },
        bench_fn: TestHelper::dummy_bench,
        options: None,
    };

    static BENCH_3: BenchEntry = BenchEntry {
        meta: EntryMeta {
            raw_name: "bench1",
            module_path: "mod_b",
            location: LOC,
        },
        bench_fn: TestHelper::dummy_bench,
        options: None,
    };

    let entries: Vec<AnyBenchEntry> = vec![
        AnyBenchEntry::Bench(&BENCH_1),
        AnyBenchEntry::Bench(&BENCH_2),
        AnyBenchEntry::Bench(&BENCH_3),
    ];

    let mut tree: Vec<EntryTree> = EntryTree::from_entries(&entries);

    // Retain only entries whose path contains "bench1".
    EntryTree::retain(&mut tree, |path: &str| path.contains("bench1"));

    // Both Parents remain (each has one "bench1" leaf).
    assert_eq!(tree.len(), 2);
    assert_eq!(tree[0].raw_name(), "mod_a");
    assert_eq!(tree[0].children().len(), 1);
    assert_eq!(tree[1].raw_name(), "mod_b");
    assert_eq!(tree[1].children().len(), 1);
}

// --- natural_cmp ---

#[test]
fn natural_cmp_ordering() {
    use std::cmp::Ordering;

    // Numeric-aware: bench_2 < bench_10 (not lexicographic).
    let result: Ordering = NaturalCmp::compare("bench_2", "bench_10");
    assert_eq!(result, Ordering::Less);

    // Mixed text and numbers.
    let result: Ordering = NaturalCmp::compare("a1b", "a2b");
    assert_eq!(result, Ordering::Less);

    let result: Ordering = NaturalCmp::compare("a2b", "a10b");
    assert_eq!(result, Ordering::Less);

    // Equal strings.
    let result: Ordering = NaturalCmp::compare("bench_1", "bench_1");
    assert_eq!(result, Ordering::Equal);

    // Pure text comparison.
    let result: Ordering = NaturalCmp::compare("alpha", "beta");
    assert_eq!(result, Ordering::Less);
}
