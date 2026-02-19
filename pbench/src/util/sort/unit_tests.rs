//! Unit tests for natural string comparison.

use super::NaturalCmp;

// ---- natural_sort_order ----

#[test]
fn natural_sort_order() {
    let mut input: Vec<&str> = vec!["bench_10", "bench_2", "bench_1"];
    input.sort_by(|a: &&str, b: &&str| NaturalCmp::compare(a, b));

    assert_eq!(input, vec!["bench_1", "bench_2", "bench_10"]);
}

// --- natural_sort_mixed ---

#[test]
fn natural_sort_mixed() {
    let mut input: Vec<&str> = vec!["a2b", "a10b", "a1b"];
    input.sort_by(|a: &&str, b: &&str| NaturalCmp::compare(a, b));

    assert_eq!(input, vec!["a1b", "a2b", "a10b"]);
}
