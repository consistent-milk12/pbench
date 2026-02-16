use super::{FineDuration, RawSample, SampleCollection};

// --- SampleCollection::sorted_picos ---

#[test]
fn collection_add_and_sort() {
    let mut coll: SampleCollection = SampleCollection::new();

    coll.push(RawSample {
        duration: FineDuration { picos: 300 },
        sample_size: 1,
    });
    coll.push(RawSample {
        duration: FineDuration { picos: 100 },
        sample_size: 1,
    });
    coll.push(RawSample {
        duration: FineDuration { picos: 200 },
        sample_size: 1,
    });

    let sorted: Vec<u128> = coll.sorted_picos();
    assert_eq!(sorted, vec![100, 200, 300]);
}

// --- SampleCollection::total_duration ---

#[test]
fn collection_total_duration() {
    let mut coll: SampleCollection = SampleCollection::new();

    coll.push(RawSample {
        duration: FineDuration { picos: 100 },
        sample_size: 1,
    });
    coll.push(RawSample {
        duration: FineDuration { picos: 200 },
        sample_size: 1,
    });

    let total: FineDuration = coll.total_duration();
    assert_eq!(total.picos, 300);
}

// --- SampleCollection::total_iterations ---

#[test]
fn collection_total_iterations() {
    let mut coll: SampleCollection = SampleCollection::new();

    coll.push(RawSample {
        duration: FineDuration { picos: 100 },
        sample_size: 10,
    });
    coll.push(RawSample {
        duration: FineDuration { picos: 200 },
        sample_size: 20,
    });

    let total: u64 = coll.total_iterations();
    assert_eq!(total, 30);
}

// --- SampleCollection::new / is_empty / len ---

#[test]
fn collection_new_is_empty() {
    let coll: SampleCollection = SampleCollection::new();
    assert!(coll.is_empty());
    assert_eq!(coll.len(), 0);
}

// --- SampleCollection::with_capacity ---

#[test]
fn collection_with_capacity() {
    let mut coll: SampleCollection = SampleCollection::with_capacity(100);
    assert!(coll.is_empty());

    coll.push(RawSample {
        duration: FineDuration { picos: 42 },
        sample_size: 1,
    });
    assert_eq!(coll.len(), 1);
    assert!(!coll.is_empty());
}

// --- SampleCollection::raw_picos ---

#[test]
fn collection_raw_picos_preserves_order() {
    let mut coll: SampleCollection = SampleCollection::new();

    coll.push(RawSample {
        duration: FineDuration { picos: 300 },
        sample_size: 1,
    });
    coll.push(RawSample {
        duration: FineDuration { picos: 100 },
        sample_size: 1,
    });
    coll.push(RawSample {
        duration: FineDuration { picos: 200 },
        sample_size: 1,
    });

    let raw: Vec<u128> = coll.raw_picos();
    assert_eq!(raw, vec![300, 100, 200]);
}
