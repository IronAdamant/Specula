//! Rule 15 and F3: the editor outliner's search reads a 100k-entity world a slice per window
//! frame, allocating nothing per entity read and stopping at its time budget (review R1 of the
//! editor lane: 4,000 entities a frame cost two allocations each, 8,133 a frame, and 10-32 ms of
//! CPU).
//!
//! The only test in this file, so the counting allocator sees no other test's threads.

use specula::outline::{Budget, Cursor, Query, Scan, CHECK_EVERY};
use specula::world::{EntityKey, Shape, SpawnSpec, World};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

struct Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards every call to the system allocator unchanged; only counts allocations.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: same contract as our caller's.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as our caller's.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: same contract as our caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// What one slice may allocate whatever it reads: the walk's level list, a range bound per
/// level it resumes, a name per level where it stops, and the cursor's level list growing.
const PER_SLICE: usize = 12;

#[test]
fn searching_a_100k_world_allocates_nothing_per_entity_and_keeps_its_time_budget() {
    // Probe 2's world: 10 groups of 10,000 children, and a crate.
    let mut world = World::default();
    let mut s = SpawnSpec::new(Shape::Cube);
    s.name = Some("crate".into());
    world.spawn(s, "t").unwrap();
    for g in 0..10 {
        let mut s = SpawnSpec::new(Shape::Group);
        s.name = Some(format!("g{g}"));
        let parent = world.spawn(s, "t").unwrap().id;
        for i in 0..10_000 {
            let mut s = SpawnSpec::new(Shape::Group);
            s.name = Some(format!("e{g}_{i:05}"));
            s.parent = Some(EntityKey::Id(parent));
            world.spawn(s, "t").unwrap();
        }
    }
    let total = world.len();
    assert_eq!(total, 100_011);

    // A search for something absent reads the whole world, a slice at a time.
    let query = Query::parse("zq");
    let steps = Budget { max_steps: 4000, max_time: Duration::from_secs(60) };
    let mut scan = Scan::new(Cursor::start(), 18);
    let (mut slices, mut worst) = (0usize, 0usize);
    while !scan.done {
        let before = ALLOCATIONS.load(Ordering::Relaxed);
        let read = scan.step(&world, &query, 18, steps);
        let allocated = ALLOCATIONS.load(Ordering::Relaxed) - before;
        worst = worst.max(allocated);
        assert!(read <= 4000);
        slices += 1;
        assert!(slices < 100, "the walk ends");
    }
    assert_eq!(scan.read, total, "every entity read once");
    assert!(scan.matches.is_empty());
    assert!(worst <= PER_SLICE, "a slice of 4,000 entities allocated {worst} times (at most {PER_SLICE}, never per entity)");

    // Matches: one page of 18 in path order, the next page starting at the 19th.
    let query = Query::parse("e3_000");
    let mut scan = Scan::new(Cursor::start(), 18);
    while !scan.done {
        scan.step(&world, &query, 18, steps);
    }
    let paths: Vec<String> = scan.matches.iter().map(|id| world.path_of(*id)).collect();
    assert_eq!(paths.len(), 18);
    assert_eq!(paths[0], "/g3/e3_00000");
    assert_eq!(paths[17], "/g3/e3_00017");
    let mut next = scan.next.clone().expect("more matches follow");
    assert_eq!(next.next(&world, &|_| true).map(|(id, _)| world.path_of(id)).as_deref(), Some("/g3/e3_00018"));

    // The time budget: a slice with no step limit stops near its time budget (checked every
    // CHECK_EVERY entities), so a frame never walks the world.
    let budget = Budget { max_steps: usize::MAX, max_time: Duration::from_micros(500) };
    let query = Query::parse("zq");
    let mut scan = Scan::new(Cursor::start(), 18);
    let mut longest = Duration::ZERO;
    let mut slices = 0;
    while !scan.done {
        let t = Instant::now();
        let read = scan.step(&world, &query, 18, budget);
        longest = longest.max(t.elapsed());
        assert!(scan.done || read.is_multiple_of(CHECK_EVERY), "it stops at a clock check");
        slices += 1;
    }
    assert_eq!(scan.read, total);
    assert!(slices > 1, "100k entities don't fit in one 0.5 ms slice");
    // The budget is 0.5 ms plus at most CHECK_EVERY entities.
    assert!(longest < specula_core::speed_budget(std::time::Duration::from_millis(25)), "the longest slice took {longest:?}");
}
