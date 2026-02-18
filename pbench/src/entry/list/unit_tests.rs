//! Unit tests for [`EntryList`].

use super::*;
use crate::bencher::Bencher;
use crate::entry::meta::{EntryLocation, EntryMeta};
use crate::entry::{AnyBenchEntry, BenchEntry};

struct TestHelper;

impl TestHelper {
    fn dummy_bench(_b: &Bencher<'_>) {}
}

// --- push and iter ---

#[test]
fn entry_list_push_and_iter() {
    const META_A: EntryMeta = EntryMeta {
        raw_name: "bench_a",
        module_path: "crate::mod_a",
        location: EntryLocation {
            file: "test.rs",
            line: 1,
            col: 1,
        },
    };

    const META_B: EntryMeta = EntryMeta {
        raw_name: "bench_b",
        module_path: "crate::mod_b",
        location: EntryLocation {
            file: "test.rs",
            line: 2,
            col: 1,
        },
    };

    static BENCH_A: BenchEntry = BenchEntry {
        meta: META_A,
        bench_fn: TestHelper::dummy_bench,
        options: None,
    };

    static BENCH_B: BenchEntry = BenchEntry {
        meta: META_B,
        bench_fn: TestHelper::dummy_bench,
        options: None,
    };

    static ENTRY_A: AnyBenchEntry = AnyBenchEntry::Bench(&BENCH_A);
    static ENTRY_B: AnyBenchEntry = AnyBenchEntry::Bench(&BENCH_B);

    static ROOT: EntryList<AnyBenchEntry> = EntryList::root();
    static NODE_A: EntryList<AnyBenchEntry> = EntryList::new(&ENTRY_A);
    static NODE_B: EntryList<AnyBenchEntry> = EntryList::new(&ENTRY_B);

    ROOT.push(&NODE_A);
    ROOT.push(&NODE_B);

    let names: Vec<&str> = ROOT.iter().map(|e: &AnyBenchEntry| e.raw_name()).collect();

    assert_eq!(names.len(), 2);
    assert!(names.contains(&"bench_a"));
    assert!(names.contains(&"bench_b"));
}
