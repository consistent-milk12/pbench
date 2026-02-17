//! Unit tests for [`CounterCollection`].

use super::*;
use crate::counter::CounterKind;

// --- new ---

#[test]
fn new_is_empty() {
    let coll: CounterCollection = CounterCollection::new(CounterKind::Bytes);

    assert!(coll.is_empty());
    assert_eq!(coll.len(), 0);
    assert_eq!(coll.kind(), CounterKind::Bytes);
}

// --- push + len ---

#[test]
fn push_increments_len() {
    let mut coll: CounterCollection = CounterCollection::new(CounterKind::Items);

    coll.push(100);
    coll.push(200);
    coll.push(300);

    assert_eq!(coll.len(), 3);
    assert!(!coll.is_empty());
}

// --- mean_count ---

#[test]
fn mean_count_empty() {
    let coll: CounterCollection = CounterCollection::new(CounterKind::Bytes);

    assert!(coll.mean_count().is_none());
}

#[test]
fn mean_count_single() {
    let mut coll: CounterCollection = CounterCollection::new(CounterKind::Bytes);
    coll.push(1024);

    let mean: f64 = coll.mean_count().unwrap();
    assert!((mean - 1024.0).abs() < f64::EPSILON);
}

#[test]
fn mean_count_multiple() {
    let mut coll: CounterCollection = CounterCollection::new(CounterKind::Items);
    coll.push(100);
    coll.push(200);
    coll.push(300);

    let mean: f64 = coll.mean_count().unwrap();
    assert!((mean - 200.0).abs() < f64::EPSILON);
}

#[test]
fn mean_count_non_integer() {
    let mut coll: CounterCollection = CounterCollection::new(CounterKind::Items);
    coll.push(100);
    coll.push(201);

    let mean: f64 = coll.mean_count().unwrap();
    assert!((mean - 150.5).abs() < f64::EPSILON);
}

#[test]
fn mean_count_large_values_no_overflow() {
    let mut coll: CounterCollection = CounterCollection::new(CounterKind::Bytes);

    // Push values near u64::MAX — sum would overflow u64 but not u128.
    coll.push(u64::MAX);
    coll.push(u64::MAX);

    let mean: f64 = coll.mean_count().unwrap();

    // Mean should be approximately u64::MAX.
    #[expect(clippy::cast_precision_loss)]
    let expected: f64 = u64::MAX as f64;
    let relative_error: f64 = ((mean - expected) / expected).abs();

    assert!(
        relative_error < 1e-10,
        "mean {mean} too far from expected {expected}"
    );
}

// --- with_capacity ---

#[test]
fn with_capacity_starts_empty() {
    let coll: CounterCollection = CounterCollection::with_capacity(CounterKind::Chars, 100);

    assert!(coll.is_empty());
    assert_eq!(coll.len(), 0);
    assert_eq!(coll.kind(), CounterKind::Chars);
}

// --- counts ---

#[test]
fn counts_returns_raw_values() {
    let mut coll: CounterCollection = CounterCollection::new(CounterKind::Cycles);
    coll.push(10);
    coll.push(20);
    coll.push(30);

    assert_eq!(coll.counts(), &[10, 20, 30]);
}
