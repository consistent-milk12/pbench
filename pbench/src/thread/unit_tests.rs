//! Unit tests for thread synchronization primitives and thread pool.

use super::{AtomicFlag, CachePadded, StdMem, StdThread, SyncWrap, ThreadPool};
use std::{cell::Cell, ptr as StdPtr, time::Duration};

// --- CachePadded ---

#[test]
fn cache_padded_alignment() {
    let padded: CachePadded<u64> = CachePadded(42);
    let addr: usize = StdPtr::from_ref(&padded) as usize;

    assert_eq!(addr % 64, 0, "CachePadded must be 64-byte aligned");
}

#[test]
fn cache_padded_size() {
    let size: usize = StdMem::size_of::<CachePadded<u64>>();

    assert!(size >= 64, "CachePadded<u64> must be at least 64 bytes");
}

#[test]
fn cache_padded_deref() {
    let padded: CachePadded<u64> = CachePadded(99);
    let value: u64 = *padded;

    assert_eq!(value, 99);
}

#[test]
fn cache_padded_deref_mut() {
    let mut padded: CachePadded<u64> = CachePadded(0);
    *padded = 42;

    assert_eq!(*padded, 42);
}

// --- SyncWrap ---

#[test]
fn sync_wrap_deref() {
    let wrapped: SyncWrap<u64> = unsafe { SyncWrap::new(123) };
    let value: u64 = *wrapped;
    assert_eq!(value, 123);
}

#[test]
fn sync_wrap_deref_mut() {
    let mut wrapped: SyncWrap<u64> = unsafe { SyncWrap::new(0) };
    *wrapped = 456;
    assert_eq!(*wrapped, 456);
}

#[test]
fn sync_wrap_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<SyncWrap<Cell<u64>>>();
}

// --- AtomicFlag ---

#[test]
fn atomic_flag_default_false() {
    let flag: AtomicFlag = AtomicFlag::new(false);
    assert!(!flag.get());
}

#[test]
fn atomic_flag_default_true() {
    let flag: AtomicFlag = AtomicFlag::new(true);
    assert!(flag.get());
}

#[test]
fn atomic_flag_roundtrip() {
    let flag: AtomicFlag = AtomicFlag::new(false);

    flag.set(true);
    assert!(flag.get());

    flag.set(false);
    assert!(!flag.get());
}

// --- ThreadPool::par_extend ---

#[test]
fn par_extend_zero_aux() {
    static POOL: ThreadPool = ThreadPool::new();

    let mut results: Vec<Option<usize>> = Vec::new();
    POOL.par_extend(&mut results, 0, |index: usize| index);

    assert_eq!(results, vec![Some(0)]);
    POOL.drop_threads();
}

#[test]
fn par_extend_one_aux() {
    static POOL: ThreadPool = ThreadPool::new();

    let mut results: Vec<Option<usize>> = Vec::new();
    POOL.par_extend(&mut results, 1, |index: usize| index);

    assert_eq!(results, vec![Some(0), Some(1)]);
    POOL.drop_threads();
}

#[test]
fn par_extend_four_aux() {
    static POOL: ThreadPool = ThreadPool::new();

    let mut results: Vec<Option<usize>> = Vec::new();
    POOL.par_extend(&mut results, 4, |index: usize| index);

    let expected: Vec<Option<usize>> = (0..5).map(Some).collect();
    assert_eq!(results, expected);
    POOL.drop_threads();
}

#[test]
fn par_extend_scaling() {
    static POOL: ThreadPool = ThreadPool::new();

    // Increasing thread counts, then decreasing.
    for &aux in &[0_usize, 1, 2, 4, 8, 4, 0] {
        let total: usize = aux + 1;
        let mut results: Vec<Option<usize>> = Vec::new();
        POOL.par_extend(&mut results, aux, |index: usize| index);

        let expected: Vec<Option<usize>> = (0..total).map(Some).collect();
        assert_eq!(results, expected);
    }

    POOL.drop_threads();
}

// --- ThreadPool::broadcast ---

#[test]
fn broadcast_thread_id_zero_is_main() {
    static POOL: ThreadPool = ThreadPool::new();

    let main_id: std::thread::ThreadId = std::thread::current().id();

    POOL.broadcast(4, |thread_id: usize| {
        let is_main: bool = main_id == std::thread::current().id();
        assert_eq!(is_main, thread_id == 0);
    });

    POOL.drop_threads();
}

#[test]
fn broadcast_with_sleep() {
    static POOL: ThreadPool = ThreadPool::new();

    POOL.broadcast(4, |thread_id: usize| {
        if thread_id > 0 {
            StdThread::sleep(Duration::from_millis(5));
        }
    });

    POOL.drop_threads();
}

// --- ThreadPool::aux_thread_count ---

#[test]
fn pool_spawns_lazily() {
    static POOL: ThreadPool = ThreadPool::new();

    assert_eq!(POOL.aux_thread_count(), 0);

    POOL.broadcast(3, |_: usize| {});
    assert_eq!(POOL.aux_thread_count(), 3);

    // Requesting fewer threads does not shrink the pool.
    POOL.broadcast(1, |_: usize| {});
    assert_eq!(POOL.aux_thread_count(), 3);

    POOL.drop_threads();
}

// --- panic handling ---

#[test]
fn broadcast_swallows_main_thread_panic() {
    static POOL: ThreadPool = ThreadPool::new();

    // Main thread (index 0) panic must not propagate to the caller.
    POOL.broadcast(0, |_: usize| panic!("main thread panic"));
    POOL.drop_threads();
}

#[test]
fn broadcast_swallows_aux_thread_panic() {
    static POOL: ThreadPool = ThreadPool::new();

    // Auxiliary thread panics must not propagate to the caller.
    POOL.broadcast(2, |_: usize| panic!("aux thread panic"));
    POOL.drop_threads();
}

#[test]
fn par_extend_panic_leaves_none_slot() {
    static POOL: ThreadPool = ThreadPool::new();
    let mut results: Vec<Option<usize>> = Vec::new();

    // When the task panics, the pre-initialized None slot is left unchanged.
    POOL.par_extend(&mut results, 0, |_: usize| -> usize {
        panic!("task panic")
    });

    assert_eq!(results, vec![None]);
    POOL.drop_threads();
}
