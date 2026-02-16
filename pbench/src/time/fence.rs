//! Memory and compiler fences for accurate timing measurements.
//!
//! Prevents the compiler and CPU from reordering operation across
//! measurement boundaries. All methods live on the unit struct [`Fence`].

use std::arch as StdArch;
use std::sync::atomic as StdAtomic;
use std::sync::atomic::Ordering as AtomicOrdering;

/// Unit struct grouping memory and compiler fence ops.
pub struct Fence;

impl Fence {
    /// Full memory fence, prevents both compiler and CPU reordering.
    #[inline(always)]
    pub fn full() {
        Self::asm();

        StdAtomic::fence(AtomicOrdering::SeqCst);
    }

    /// Compiler-only fence, prevents the compiler from reordering.
    #[inline(always)]
    pub fn compiler() {
        Self::asm();

        StdAtomic::compiler_fence(AtomicOrdering::SeqCst);
    }

    /// Stronger compiler fence on platforms with stable `asm!`.
    ///
    /// Prevents LLVM from removing loops or hoisting logic out of the
    /// benchmark loop.
    #[inline(always)]
    fn asm() {
        // Miri does not support inline assembly.
        if cfg!(miri) {
            return;
        }

        #[cfg(any(
            target_arch = "x86",
            target_arch = "x86_64",
            target_arch = "arm",
            target_arch = "aarch64",
            target_arch = "riscv32",
            target_arch = "riscv64",
            target_arch = "loongarch64",
        ))]
        // SAFETY: The inline assembly is a no-op.
        unsafe {
            StdArch::asm!("", options(nostack, preserves_flags));
        }
    }
}
