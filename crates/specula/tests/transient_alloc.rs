//! Foundation F3: a game moving hundreds of transient entities every tick allocates nothing in
//! the update path (the world's update, the tick's change take, the per-tick state hash).
//!
//! The only test in this file, so the counting allocator sees no other test's threads.

use specula::world::{EntityKey, Recording, Shape, SpawnSpec, UpdateSpec, World};
use specula_core::math::Vec3;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

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

#[test]
fn moving_500_transient_entities_allocates_nothing() {
    let mut world = World::default();
    world.set_recording(Recording::Full);
    let mut group = SpawnSpec::new(Shape::Group);
    group.name = Some("enemies".into());
    let parent = world.spawn(group, "game").unwrap().id;
    let mut ids = Vec::new();
    for i in 0..500 {
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some(format!("e{i:03}"));
        s.parent = Some(EntityKey::Id(parent));
        s.transient = true;
        ids.push(world.spawn(s, "game").unwrap().id);
    }
    world.take_changes();
    let tick = |world: &mut World, t: u32| {
        world.set_recording(Recording::Snapshot);
        for (i, &id) in ids.iter().enumerate() {
            let p = Vec3::new(i as f32 * 0.1, 0.0, t as f32 * 0.05);
            let change = UpdateSpec { position: Some(p), rotation_deg: Some(Vec3::new(0.0, t as f32, 0.0)), ..UpdateSpec::default() };
            world.update(&EntityKey::Id(id), change, "game").unwrap();
        }
        world.set_recording(Recording::Full);
        let changes = world.take_changes();
        assert!(changes.is_empty());
        drop(changes);
        world.state_hash()
    };
    let hash = tick(&mut world, 0); // warm up
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    for t in 1..=10 {
        assert_eq!(tick(&mut world, t), hash, "presentation never moves the state hash");
    }
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before;
    assert_eq!(allocations, 0, "10 ticks x 500 transient moves allocated {allocations} times");
    assert_eq!(world.revision(), 1, "and never marked the world changed (the group's spawn is all)");
}
