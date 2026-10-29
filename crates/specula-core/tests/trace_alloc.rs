//! Foundation F3: a disabled trace event allocates nothing (its fields aren't even evaluated),
//! and neither does a disabled span (beyond its profile zone, which doesn't allocate either).
//!
//! The only test in this file, so the counting allocator's per-thread counts are this test's.

use specula_core::alloc::{thread_counts, Counting};
use specula_core::trace::{self, Level, Subsystem};
use specula_core::trace_event;

#[global_allocator]
static ALLOC: Counting<std::alloc::System> = Counting::new(std::alloc::System);

static SUB: Subsystem = Subsystem::new("test.alloc");

#[test]
fn the_disabled_path_does_not_allocate() {
    // Registers the subsystem (one allocation, once) at the default level.
    assert!(!SUB.enabled(Level::Debug));
    let name = String::from("never built");
    let before = thread_counts();
    for i in 0..10_000u64 {
        trace_event!(&SUB, Debug, "test.quiet", i = i, text = format!("{name} {i}"));
        let _s = trace::span(&SUB, Level::Trace, "test.quiet_span");
    }
    let after = thread_counts();
    assert_eq!(after.0 - before.0, 0, "allocations on the disabled path");
    // Enabled, it records (and allocates its fields).
    trace_event!(&SUB, Info, "test.loud", i = 1u64);
    assert_eq!(trace::read(|r| r.iter().filter(|e| e.subsystem == "test.alloc").count()), 1);
}
