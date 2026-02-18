//! Lock-free linked list for static benchmark entry registration.
//!
//! [`EntryList`] provides thread-safe insertion and iteration for benchmark
//! entries registered at pre-main init time via linker sections. Each node
//! is a `'static` reference, enabling safe construction in `static` items.

use std::{
    iter as StdIter, ptr as StdPtr,
    sync::atomic::{AtomicPtr, Ordering as AtomicOrdering},
};

/// Linked list of benchmark entries.
///
/// Each node optionally holds a `&'static T` entry and an atomic pointer
/// to the next node. The root node (created by [`root`](Self::root)) has
/// `entry = None`; data nodes (created by [`new`](Self::new)) wrap an entry.
///
/// # Thread Safety
///
/// Insertion via [`push`](Self::push) uses compare-and-swap, making it
/// safe for concurrent use despite being designed for single-threaded
/// pre-main initialization.
pub struct EntryList<T: 'static> {
    /// The entry stored in this node, `None` for the root sentinel.
    entry: Option<&'static T>,

    /// Pointer to the next node in the list.
    next: AtomicPtr<Self>,
}

impl<T> EntryList<T> {
    /// Create an empty root sentinel node.
    ///
    /// Used to initialise the global `BENCH_ENTRIES` static.
    #[must_use]
    pub const fn root() -> Self {
        Self {
            entry: None,
            next: AtomicPtr::new(StdPtr::null_mut()),
        }
    }

    /// Dereference the `next` pointer.
    #[inline]
    fn next(&self) -> Option<&Self> {
        // SAFETY: `next` is only assigned by `push`, which always receives
        // a `&'static Self`. The pointer is either null (terminal) or
        // points to a valid static node.
        unsafe { self.next.load(AtomicOrdering::Relaxed).as_ref() }
    }
}

// Public API used by macro-generated registration code.
impl<T> EntryList<T> {
    /// Create a data node wrapping a static entry.
    ///
    /// Used in macro-generated code to create per-benchmark static nodes.
    #[inline]
    #[must_use]
    pub const fn new(entry: &'static T) -> Self {
        Self {
            entry: Some(entry),
            next: AtomicPtr::new(StdPtr::null_mut()),
        }
    }

    /// Create an iterator over entries in the list.
    ///
    /// Traverses from `self` through the linked list, yielding each
    /// non-`None` entry. The root sentinel is skipped automatically.
    #[inline]
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        let mut list: Option<&Self> = Some(self);

        StdIter::from_fn(move || -> Option<Option<&T>> {
            let current: &Self = list?;
            list = current.next();

            Some(current.entry)
        })
        .flatten()
    }

    /// Insert `other` at the front of the list via compare-and-swap.
    ///
    /// Both `self` (root) and `other` (new node) must be `'static`,
    /// ensuring the linked list never contains dangling pointers.
    #[inline]
    pub fn push(&'static self, other: &'static Self) {
        let mut old_next: *mut Self = self.next.load(AtomicOrdering::Relaxed);

        loop {
            // Point `other.next` at the current head.
            other.next.store(old_next, AtomicOrdering::Release);

            // Try to swing `self.next` from `old_next` to `other`.
            let other_ptr: *mut Self = StdPtr::from_ref::<Self>(other).cast_mut();

            match self.next.compare_exchange_weak(
                old_next,
                other_ptr,
                AtomicOrdering::AcqRel,
                AtomicOrdering::Acquire,
            ) {
                Ok(_) => return,

                Err(new) => old_next = new,
            }
        }
    }
}

#[cfg(test)]
mod unit_tests;
