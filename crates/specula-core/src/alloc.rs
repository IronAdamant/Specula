//! Memory tracker: a counting global allocator a binary can opt into.
//!
//! ```text
//! #[global_allocator]
//! static ALLOC: specula_core::alloc::Counting<std::alloc::System> =
//!     specula_core::alloc::Counting::new(std::alloc::System);
//! ```
//!
//! It wraps another allocator and keeps process-wide counters (live bytes, peak, allocation and
//! free counts, bytes ever allocated) in relaxed atomics, plus per-thread counters the CPU profiler
//! reads at zone boundaries, so allocations are attributed to the zone that made them (see
//! [`crate::profile`]). Binaries that don't install it report `installed: false`.
//!
//! Cost: a handful of uncontended atomic adds and a thread-local add per call.

use std::alloc::{GlobalAlloc, Layout};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::Relaxed};

static INSTALLED: AtomicBool = AtomicBool::new(false);
static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static FREES: AtomicU64 = AtomicU64::new(0);
static ALLOCATED: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// This thread's (allocations, bytes allocated). Const-initialised with no destructor, so
    /// reading it from inside the allocator is sound. Where std keeps thread-locals behind OS keys
    /// (windows-gnu, Android), the key's first use on a thread allocates; that nested allocation
    /// finds the key still uninitialised, gets `AccessError` from `try_with`, and is simply not
    /// counted per thread (it is counted globally).
    static THREAD: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}

/// A [`GlobalAlloc`] that counts what passes through to `A`.
pub struct Counting<A> {
    inner: A,
}

impl<A> Counting<A> {
    pub const fn new(inner: A) -> Counting<A> {
        Counting { inner }
    }
}

fn on_alloc(size: usize) {
    if !INSTALLED.load(Relaxed) {
        INSTALLED.store(true, Relaxed);
    }
    let now = CURRENT.fetch_add(size, Relaxed) + size;
    PEAK.fetch_max(now, Relaxed);
    ALLOCATIONS.fetch_add(1, Relaxed);
    ALLOCATED.fetch_add(size as u64, Relaxed);
    let _ = THREAD.try_with(|t| {
        let (n, b) = t.get();
        t.set((n + 1, b + size as u64));
    });
}

fn on_free(size: usize) {
    CURRENT.fetch_sub(size, Relaxed);
    FREES.fetch_add(1, Relaxed);
}

// SAFETY: every method forwards to the wrapped allocator with the caller's arguments unchanged and
// returns its result unchanged, so `A`'s guarantees are this allocator's guarantees. The counting
// around the calls only touches atomics and a const-initialised, destructor-free thread-local
// (never allocates, never unwinds).
unsafe impl<A: GlobalAlloc> GlobalAlloc for Counting<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract (non-zero size); passed on.
        let p = unsafe { self.inner.alloc(layout) };
        if !p.is_null() {
            on_alloc(layout.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: as for `alloc`.
        let p = unsafe { self.inner.alloc_zeroed(layout) };
        if !p.is_null() {
            on_alloc(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller guarantees `ptr` came from this allocator (so from `inner`) with
        // `layout`.
        unsafe { self.inner.dealloc(ptr, layout) };
        on_free(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr`/`layout` describe a live block of this allocator
        // (so of `inner`) and that `new_size` is valid for `layout.align()`.
        let p = unsafe { self.inner.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            // One allocation of `new_size` and the old block freed (not counted as a free call).
            CURRENT.fetch_sub(layout.size(), Relaxed);
            on_alloc(new_size);
        }
        p
    }
}

/// Process-wide allocation counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AllocStats {
    /// A [`Counting`] allocator is the global allocator (it has seen an allocation).
    pub installed: bool,
    /// Bytes currently allocated.
    pub current_bytes: u64,
    /// Highest `current_bytes` since start (or the last [`reset_peak`]).
    pub peak_bytes: u64,
    /// Allocation calls (reallocations included).
    pub allocations: u64,
    pub frees: u64,
    /// Bytes ever allocated.
    pub allocated_bytes: u64,
}

pub fn stats() -> AllocStats {
    AllocStats {
        installed: INSTALLED.load(Relaxed),
        current_bytes: CURRENT.load(Relaxed) as u64,
        peak_bytes: PEAK.load(Relaxed) as u64,
        allocations: ALLOCATIONS.load(Relaxed),
        frees: FREES.load(Relaxed),
        allocated_bytes: ALLOCATED.load(Relaxed),
    }
}

/// Whether a [`Counting`] allocator is installed.
pub fn installed() -> bool {
    INSTALLED.load(Relaxed)
}

/// `(allocations, bytes allocated)` by the calling thread so far; `(0, 0)` without [`Counting`].
pub fn thread_counts() -> (u64, u64) {
    THREAD.try_with(Cell::get).unwrap_or((0, 0))
}

/// Starts peak tracking over from the current size.
pub fn reset_peak() {
    PEAK.store(CURRENT.load(Relaxed), Relaxed);
}

/// The unit tests of this crate run under the counting allocator, so the profiler's
/// no-allocation guarantees can be checked.
#[cfg(test)]
#[global_allocator]
static TEST_ALLOCATOR: Counting<std::alloc::System> = Counting::new(std::alloc::System);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_this_threads_allocations() {
        let before = thread_counts();
        let total_before = stats();
        let v: Vec<u64> = Vec::with_capacity(100);
        let after = thread_counts();
        assert_eq!(after.0 - before.0, 1);
        assert_eq!(after.1 - before.1, 800);
        let s = stats();
        assert!(s.installed);
        assert!(s.allocations > total_before.allocations);
        assert!(s.peak_bytes >= s.current_bytes);
        drop(v);
        assert!(stats().frees > total_before.frees);
    }

    #[test]
    fn realloc_counts_as_one_allocation() {
        let mut v: Vec<u8> = Vec::with_capacity(16);
        let before = thread_counts();
        v.reserve_exact(4096);
        let after = thread_counts();
        assert_eq!(after.0 - before.0, 1);
        assert!(after.1 - before.1 >= 4096);
    }

    #[test]
    fn threads_count_separately() {
        std::thread::scope(|s| {
            s.spawn(|| {
                let b = thread_counts();
                let v: Vec<u8> = vec![1; 1000];
                assert_eq!((thread_counts().0 - b.0, thread_counts().1 - b.1), (1, 1000));
                drop(v);
            });
        });
    }

    #[test]
    fn peak_resets_to_current() {
        let big: Vec<u8> = Vec::with_capacity(1 << 20);
        drop(big);
        reset_peak();
        // Other tests allocate concurrently, so only the invariant is certain.
        let s = stats();
        assert!(s.peak_bytes >= s.current_bytes);
    }
}
