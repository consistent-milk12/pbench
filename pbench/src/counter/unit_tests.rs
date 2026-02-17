//! Unit tests for [`Counter`] trait and concrete counter types.

use super::{BytesCount, CharsCount, Counter, CounterKind, CyclesCount, ItemsCount};

// --- BytesCount ---

#[test]
fn bytes_count_new() {
    let bc: BytesCount = BytesCount::new(1024);

    assert_eq!(bc.count(), 1024);
    assert_eq!(bc.kind(), CounterKind::Bytes);
}

#[test]
fn bytes_count_of_slice_u8() {
    let data: [u8; 256] = [0u8; 256];
    let bc: BytesCount = BytesCount::of_slice(&data);

    assert_eq!(bc.count(), 256);
}

#[test]
fn bytes_count_of_slice_u32() {
    let data: [u32; 10] = [0u32; 10];
    let bc: BytesCount = BytesCount::of_slice(&data);

    // 10 elements * 4 bytes = 40
    assert_eq!(bc.count(), 40);
}

#[test]
fn bytes_count_of_str() {
    let bc: BytesCount = BytesCount::of_str("hello");

    assert_eq!(bc.count(), 5);
}

#[test]
fn bytes_count_of_str_multibyte() {
    // "é" is 2 bytes in UTF-8
    let bc: BytesCount = BytesCount::of_str("café");

    assert_eq!(bc.count(), 5); // c=1 + a=1 + f=1 + é=2
}

#[test]
fn bytes_count_zero() {
    let bc: BytesCount = BytesCount::new(0);

    assert_eq!(bc.count(), 0);
}

#[test]
fn bytes_count_of_slice_zst() {
    // Zero-sized types: 100 elements * 0 bytes each = 0 bytes.
    let data: [(); 100] = [(); 100];
    let bc: BytesCount = BytesCount::of_slice(&data);

    assert_eq!(bc.count(), 0);
}

// --- ItemsCount ---

#[test]
fn items_count_new() {
    let ic: ItemsCount = ItemsCount::new(500);

    assert_eq!(ic.count(), 500);
    assert_eq!(ic.kind(), CounterKind::Items);
}

#[test]
fn items_count_of_iter() {
    let data: Vec<i32> = vec![1, 2, 3, 4, 5];
    let ic: ItemsCount = ItemsCount::of_iter(data);

    assert_eq!(ic.count(), 5);
}

#[test]
fn items_count_of_empty_iter() {
    let data: Vec<i32> = Vec::new();
    let ic: ItemsCount = ItemsCount::of_iter(data);

    assert_eq!(ic.count(), 0);
}

#[test]
fn items_count_of_iter_consumes() {
    use std::cell::Cell;

    let seen: Cell<u64> = Cell::new(0);
    let iter = (0..5).inspect(|_: &i32| {
        seen.set(seen.get() + 1);
    });
    let ic: ItemsCount = ItemsCount::of_iter(iter);

    assert_eq!(ic.count(), 5);

    // All elements were consumed by the iterator.
    assert_eq!(seen.get(), 5);
}

// --- CharsCount ---

#[test]
fn chars_count_new() {
    let cc: CharsCount = CharsCount::new(42);

    assert_eq!(cc.count(), 42);
    assert_eq!(cc.kind(), CounterKind::Chars);
}

#[test]
fn chars_count_of_str_ascii() {
    let cc: CharsCount = CharsCount::of_str("hello");

    assert_eq!(cc.count(), 5);
}

#[test]
fn chars_count_of_str_unicode() {
    // "café" has 4 Unicode characters despite 5 UTF-8 bytes
    let cc: CharsCount = CharsCount::of_str("café");

    assert_eq!(cc.count(), 4);
}

#[test]
fn chars_count_of_str_emoji() {
    // Each emoji is one Unicode scalar value
    let cc: CharsCount = CharsCount::of_str("👋🌍");

    assert_eq!(cc.count(), 2);
}

// --- CyclesCount ---

#[test]
fn cycles_count_new() {
    let cc: CyclesCount = CyclesCount::new(1_000_000);

    assert_eq!(cc.count(), 1_000_000);
    assert_eq!(cc.kind(), CounterKind::Cycles);
}

// --- CounterKind ---

#[test]
fn counter_kind_eq() {
    assert_eq!(CounterKind::Bytes, CounterKind::Bytes);
    assert_ne!(CounterKind::Bytes, CounterKind::Items);
    assert_ne!(CounterKind::Chars, CounterKind::Cycles);
}

// --- Counter trait dispatch ---

#[test]
fn counter_trait_dynamic_dispatch() {
    let counters: Vec<Box<dyn Counter>> = vec![
        Box::new(BytesCount::new(100)),
        Box::new(ItemsCount::new(200)),
        Box::new(CharsCount::new(300)),
        Box::new(CyclesCount::new(400)),
    ];

    let counts: Vec<u64> = counters.iter().map(|c| c.count()).collect();
    assert_eq!(counts, vec![100, 200, 300, 400]);

    let kinds: Vec<CounterKind> = counters.iter().map(|c| c.kind()).collect();
    assert_eq!(
        kinds,
        vec![
            CounterKind::Bytes,
            CounterKind::Items,
            CounterKind::Chars,
            CounterKind::Cycles,
        ]
    );
}
