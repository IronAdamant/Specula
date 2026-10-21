//! Foundation F3: a warm mind's ticks (and its hash) allocate nothing: running leaves are
//! compared in place, parallel and utility buffers are reused, the blackboard is overwritten in
//! place and hashed without formatting.
//!
//! The only test in this file, so the counting allocator sees no other test's threads.

use specula_core::Json;
use specula_mens::run::Blackboard;
use specula_mens::{Arg, ArgKind, Def, Host, Mind, Phase, Status, Verb, Vocabulary};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

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

/// Sees every other tick; actions run forever; the distance input wobbles.
struct Quiet {
    tick: u64,
}

impl Host for Quiet {
    fn condition(&mut self, _: &str, _: &Json, _: &Blackboard) -> bool {
        self.tick % 2 == 0
    }
    fn action(&mut self, _: usize, _: &str, _: &Json, phase: Phase, _: &mut Blackboard) -> Status {
        if phase == Phase::Abort {
            Status::Failure
        } else {
            Status::Running
        }
    }
    fn input(&mut self, _: &str, _: &Json, _: &Blackboard) -> f64 {
        // Changes, but never enough to change the choice (a change of choice notes it in the trace).
        3.0 + (self.tick % 3) as f64 * 0.1
    }
}

#[test]
fn a_warm_mind_ticks_without_allocating() {
    let target = Arg { name: "target", kind: ArgKind::Target, required: true };
    let vocab = Vocabulary {
        conditions: vec![Verb { name: "sees", doc: "", args: vec![target] }],
        actions: vec![Verb { name: "chase", doc: "", args: vec![target] }, Verb { name: "patrol", doc: "", args: vec![] }],
        inputs: vec![Verb { name: "distance", doc: "", args: vec![target] }],
    };
    let def = Json::parse(
        r#"{"parallel": [
            {"repeat": {"selector": [{"sequence": [{"if": "sees", "target": "/p"}, {"set": "saw", "value": 1}]}, {"set": "saw", "value": 0}]}},
            {"utility": [
                {"label": "near", "considerations": [{"input": "distance", "target": "/p", "range": [0, 10]}], "run": {"do": "chase", "target": "/p"}},
                {"label": "far", "weight": 0.5, "run": {"do": "patrol"}}
            ]}
        ], "succeed": 2}"#,
    )
    .unwrap();
    let def = match Def::parse(&def, &vocab) {
        Ok(d) => d,
        Err(e) => panic!("{e:?}"),
    };
    let mut mind = Mind::new(Arc::new(def), Blackboard::new(), 30);
    let mut host = Quiet { tick: 0 };
    for t in 0..64 {
        host.tick = t;
        mind.tick(&mut host, t);
    }
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    let mut h = specula_core::hash::StableHasher::default();
    for t in 64..64 + 200 {
        host.tick = t;
        mind.tick(&mut host, t);
        mind.hash_into(&mut h);
    }
    let n = ALLOCATIONS.load(Ordering::Relaxed) - before;
    assert_eq!(n, 0, "{n} allocations in 200 warm ticks");
}
