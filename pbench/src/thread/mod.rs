//! Thread pool and synchronization primitives for multi-threaded benchmarking.
//!
//! Provides [`CachePadded`] for false-sharing prevention, [`SyncWrap`] for
//! marking non-`Sync` types as `Sync` when the caller guarantees safety,
//! [`AtomicFlag`] for simple boolean signaling, and [`ThreadPool`] for
//! broadcasting benchmark tasks to multiple threads.

use std::mem as StdMem;
use std::num::NonZeroUsize;
use std::ops::{Deref, DerefMut};
use std::panic::{self as StdPanic, AssertUnwindSafe};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError, mpsc};
use std::thread as StdThread;
use std::thread::Thread;

use crate::util::Defer;

// =========================================================================
//  CachePadded - false sharing prevention
// =========================================================================

/// Aligns the inner value to a 64-byte cache line boundary.
///
/// Prevents false sharing when multiple threads write to adjacent memory
/// locations by ensuring each `CachePadded<T>` occupies its own cache line.
#[derive(Clone, Copy)]
#[repr(align(64))]
#[allow(
    dead_code,
    reason = "Reserved for future use in cache-line aligned thread-local storage"
)]
pub(crate) struct CachePadded<T>(
    /// The wrapped value, aligned to a cache line.
    pub T,
);

impl<T> Deref for CachePadded<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for CachePadded<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// =========================================================================
//  SyncWrap - unsafe Sync marker
// =========================================================================

/// Wraps a value to make it [`Sync`] even if `T` is not.
///
/// # Safety
///
/// This type merely asserts `Sync` to satisfy the compiler. The caller
/// bears full responsibility for ensuring soundness of concurrent access.
/// In pbench, the primary use is wrapping raw pointers to `FnMut` closures
/// that are shared across benchmark threads. The user explicitly opts in
/// to concurrent access via `--threads` / `#[bench(threads = N)]`.
pub(crate) struct SyncWrap<T> {
    /// The wrapped value.
    pub value: T,
}

/// # Safety
///
/// The caller is responsible for ensuring that concurrent access through
/// shared references is sound. See the struct-level safety documentation.
unsafe impl<T> Sync for SyncWrap<T> {}

impl<T> Deref for SyncWrap<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for SyncWrap<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T> SyncWrap<T> {
    /// Create a new `SyncWrap` around the given value.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the wrapped value is not concurrently
    /// accessed through shared references without external synchronization.
    #[inline]
    pub const unsafe fn new(value: T) -> Self {
        Self { value }
    }
}

// =========================================================================
//  AtomicFlag - convenience atomic bool
// =========================================================================

/// Convenience wrapper around [`AtomicBool`] with `Relaxed` ordering.
///
/// Used for simple boolean signaling between threads where sequentially
/// consistent ordering is not required.
#[allow(
    dead_code,
    reason = "Convenience primitive for future thread signaling patterns"
)]
pub(crate) struct AtomicFlag(AtomicBool);

#[allow(
    dead_code,
    reason = "Convenience primitive for future thread signaling patterns"
)]
impl AtomicFlag {
    /// Create a new `AtomicFlag` with the given initial value.
    #[inline]
    pub const fn new(value: bool) -> Self {
        Self(AtomicBool::new(value))
    }

    /// Load the current value with `Relaxed` ordering.
    #[inline]
    pub fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// Store a new value with `Relaxed` ordering.
    #[inline]
    pub fn set(&self, value: bool) {
        self.0.store(value, Ordering::Relaxed);
    }
}

// =========================================================================
//  ThreadPool - reusable single-task broadcast pool
// =========================================================================

/// Single shared thread pool for running benchmarks on.
pub(crate) static BENCH_POOL: ThreadPool = ThreadPool::new();

/// Reusable threads for broadcasting benchmark tasks.
///
/// This thread pool runs only a single task at a time, since only one
/// benchmark should run at a time. Invoking [`broadcast`](Self::broadcast)
/// from two threads will cause one to wait for the other to finish.
///
/// # How It Works
///
/// Upon calling [`broadcast`](Self::broadcast):
///
/// 1. The main thread creates a [`TaskShared`] pinned on its stack.
/// 2. New worker threads are spawned if needed (lazy, persistent).
/// 3. The task is sent to N auxiliary workers via rendezvous channels.
/// 4. The main thread executes the task as thread index 0.
/// 5. Workers execute with indices 1..=N and decrement a ref count.
/// 6. The main thread waits until the ref count reaches 0.
pub(crate) struct ThreadPool {
    /// Per-worker send handles. Workers persist across benchmark runs.
    threads: Mutex<Vec<mpsc::SyncSender<Task>>>,
}

impl ThreadPool {
    /// Create an empty thread pool.
    const fn new() -> Self {
        Self {
            threads: Mutex::new(Vec::new()),
        }
    }

    /// Broadcast `task` across the main thread (index 0) and `aux_threads`
    /// worker threads, collecting each thread's result into `vec`.
    ///
    /// After this call, `vec` has been extended by `aux_threads + 1` elements.
    /// Each slot contains `Some(task(index))` if the task completed normally,
    /// or `None` if the task panicked on that thread index.
    ///
    /// # Panics
    ///
    /// Panics if sending a task to an auxiliary thread fails (worker terminated
    /// unexpectedly). Panics within `task` are caught and silently discarded,
    /// leaving the corresponding slot as `None`.
    #[inline]
    pub(crate) fn par_extend<T, F>(&self, vec: &mut Vec<Option<T>>, aux_threads: usize, task: F)
    where
        F: Sync + Fn(usize) -> T,
        T: Sync + Send,
    {
        // SAFETY: We write `Some(result)` to exactly `additional` slots via
        // raw pointer writes within the `broadcast` closure. The broadcast
        // guarantees all writes complete before this function returns.
        unsafe {
            let old_len: usize = vec.len();
            let additional: usize = aux_threads + 1;

            vec.reserve_exact(additional);
            vec.spare_capacity_mut()[..additional]
                .iter_mut()
                .for_each(|val| {
                    val.write(None);
                });
            vec.set_len(old_len + additional);

            // SAFETY: Each thread writes to a disjoint slot at `ptr + index`,
            // so no two threads access the same memory location.
            let ptr: SyncWrap<*mut Option<T>> = SyncWrap::new(vec.as_mut_ptr().add(old_len));

            self.broadcast(aux_threads, move |index: usize| {
                // SAFETY: Each thread writes to its own disjoint slot.
                (*ptr).add(index).write(Some(task(index)));
            });
        }
    }

    /// Broadcast `task` across the main thread (index 0) and `aux_threads`
    /// worker threads.
    ///
    /// Returns once all threads have completed the task.
    ///
    /// # Panics
    ///
    /// Panics if sending a task to an auxiliary thread fails (worker terminated
    /// unexpectedly). Panics within `task` are caught and silently discarded.
    #[inline]
    pub(crate) fn broadcast<F>(&self, aux_threads: usize, task: F)
    where
        F: Sync + Fn(usize),
    {
        // SAFETY: `TaskShared` lives on this stack frame. We wait for
        // all workers to finish (ref_count == 0) before returning,
        // guaranteeing the reference remains valid.
        unsafe {
            let task: TaskShared<F> = TaskShared::new(aux_threads, task);
            let task: Task = Task {
                shared: NonNull::from(&task).cast(),
            };

            self.broadcast_task(aux_threads, task);
        }
    }

    /// Type-erased broadcast implementation.
    ///
    /// # Safety
    ///
    /// `task.shared` must point to a valid `TaskShared` that lives until
    /// the ref count reaches 0.
    unsafe fn broadcast_task(&self, aux_threads: usize, task: Task) {
        // Send task to auxiliary threads.
        if aux_threads > 0 {
            let threads: &mut Vec<mpsc::SyncSender<Task>> =
                &mut self.threads.lock().unwrap_or_else(PoisonError::into_inner);

            // Spawn more threads if necessary.
            if let Some(additional) = NonZeroUsize::new(aux_threads.saturating_sub(threads.len())) {
                Self::spawn(additional, threads);
            }

            for thread in &threads[..aux_threads] {
                thread.send(task).unwrap();
            }
        }

        // Run the task on the main thread (index 0).
        // SAFETY: task.shared is valid — it lives on the caller's stack frame.
        let main_result = StdPanic::catch_unwind(AssertUnwindSafe(|| unsafe { task.run(0) }));

        // Wait for other threads to finish.
        //
        // Acquire ordering ensures all writes from worker threads
        // are visible to the main thread after this loop.
        // SAFETY: task.shared is valid until ref_count reaches 0.
        while unsafe { task.shared.as_ref() }
            .ref_count
            .load(Ordering::Acquire)
            > 0
        {
            StdThread::park();
        }

        // Drop the main thread's result after workers finish, in case
        // the panic error's drop handler itself panics.
        drop(main_result);
    }

    /// Drop all worker threads by releasing their send handles.
    #[cfg(test)]
    pub(crate) fn drop_threads(&self) {
        *self.threads.lock().unwrap_or_else(PoisonError::into_inner) = Vec::new();
    }

    /// Spawn `additional` worker threads and append their channels.
    #[cold]
    fn spawn(additional: NonZeroUsize, threads: &mut Vec<mpsc::SyncSender<Task>>) {
        let next_id: usize = threads.len() + 1;

        threads.extend(
            (next_id..(next_id + additional.get())).map(|thread_id: usize| {
                // Rendezvous channel (capacity 0) reduces memory usage.
                let (sender, receiver): (mpsc::SyncSender<Task>, mpsc::Receiver<Task>) =
                    mpsc::sync_channel::<Task>(0);

                let work = move || {
                    // Abort the process if the caught panic error itself
                    // panics when dropped.
                    let panic_guard: Defer<_> = Defer::new(|| std::process::abort());

                    while let Ok(task) = receiver.recv() {
                        // SAFETY: The task is valid until ref_count == 0.
                        let result: StdThread::Result<()> =
                            StdPanic::catch_unwind(AssertUnwindSafe(|| unsafe {
                                task.run(thread_id);
                            }));

                        // SAFETY: Clone the main thread handle before
                        // decrementing ref_count, because the TaskShared
                        // is invalidated when ref_count reaches 0.
                        unsafe {
                            let main_thread: Thread = task.shared.as_ref().main_thread.clone();

                            if task
                                .shared
                                .as_ref()
                                .ref_count
                                .fetch_sub(1, Ordering::Release)
                                == 1
                            {
                                main_thread.unpark();
                            }
                        }

                        // Drop result after notifying the main thread.
                        drop(result);
                    }

                    StdMem::forget(panic_guard);
                };

                StdThread::Builder::new()
                    .name(format!("pbench-{thread_id}"))
                    .spawn(work)
                    .expect("failed to spawn benchmark thread");

                sender
            }),
        );
    }

    /// Return the number of live auxiliary threads (test helper).
    #[cfg(test)]
    fn aux_thread_count(&self) -> usize {
        self.threads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

// =========================================================================
//  Task - type-erased task handle
// =========================================================================

/// Type-erased handle to a [`TaskShared`] pinned on the broadcaster's stack.
#[derive(Clone, Copy)]
struct Task {
    /// Pointer to the type-erased `TaskShared<()>`.
    shared: NonNull<TaskShared<()>>,
}

/// # Safety
///
/// `Task` is sent between threads. The broadcaster guarantees the pointee
/// lives until `ref_count` reaches 0.
unsafe impl Send for Task {}

/// # Safety
///
/// `Task` is shared between threads via broadcast. The `TaskShared` fields
/// use atomic operations for synchronization.
unsafe impl Sync for Task {}

impl Task {
    /// Run this task on behalf of `thread_id`.
    ///
    /// # Safety
    ///
    /// - The `TaskShared` must still be live (`ref_count > 0` or this is
    ///   the main thread call before the wait loop).
    /// - `thread_id` must be within the number of broadcast threads.
    #[inline]
    unsafe fn run(&self, thread_id: usize) {
        let shared_ptr: *mut TaskShared<()> = self.shared.as_ptr();
        // SAFETY: shared_ptr is valid - guaranteed by the caller.
        let shared: &TaskShared<()> = unsafe { &*shared_ptr };

        // SAFETY: task_fn_ptr was set by TaskShared::new with the correct type.
        unsafe { (shared.task_fn_ptr)(shared_ptr.cast(), thread_id) };
    }
}

// =========================================================================
//  TaskShared - broadcaster-pinned task data
// =========================================================================

/// Data stored on the main thread's stack, shared with workers.
///
/// Fields are ordered by usage order for cache locality after the
/// benchmark may have thrashed caches.
#[repr(C)]
struct TaskShared<F> {
    /// Handle to the main thread for unparking.
    main_thread: Thread,

    /// Number of auxiliary threads still executing. Main thread waits
    /// until this reaches 0.
    ref_count: AtomicUsize,

    /// Type-erased function pointer that calls `task_fn(thread_id)`.
    task_fn_ptr: unsafe fn(task: *const TaskShared<()>, thread: usize),

    /// The actual closure. Must be the last field so preceding fields
    /// have stable offsets regardless of `F`'s size.
    task_fn: F,
}

impl<F> TaskShared<F> {
    /// Create a new `TaskShared` for the given closure.
    #[inline]
    fn new(aux_threads: usize, task_fn: F) -> Self
    where
        F: Sync + Fn(usize),
    {
        /// Type-erased trampoline that calls the concrete closure.
        ///
        /// # Safety
        ///
        /// `task` must point to a valid `TaskShared<F>`.
        unsafe fn call<F>(task: *const TaskShared<()>, thread: usize)
        where
            F: Fn(usize),
        {
            // SAFETY: task points to a valid TaskShared<F> — guaranteed by the caller.
            let task_fn: &F = unsafe { &(*task.cast::<TaskShared<F>>()).task_fn };

            task_fn(thread);
        }

        Self {
            main_thread: StdThread::current(),
            ref_count: AtomicUsize::new(aux_threads),
            task_fn_ptr: call::<F>,
            task_fn,
        }
    }
}

#[cfg(test)]
mod unit_tests;
