//! Foundation F3: a window looking through a follow camera resolves it every frame, so resolving
//! one allocates nothing once warm (the followed entity is looked up again only when the world's
//! layout changes).
//!
//! The only test in this file, so the counting allocator sees no other test's threads.

use specula::cameras::{CameraSpec, Cameras, Follow};
use specula::world::{EntityKey, Shape, SpawnSpec, UpdateSpec, World};
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
fn resolving_a_follow_camera_every_frame_allocates_nothing() {
    let mut world = World::default();
    let mut group = SpawnSpec::new(Shape::Group);
    group.name = Some("wave".into());
    let parent = world.spawn(group, "t").unwrap().id;
    let mut cart = SpawnSpec::new(Shape::Cube);
    cart.name = Some("runner_3".into());
    cart.parent = Some(EntityKey::Id(parent));
    let id = world.spawn(cart, "t").unwrap().id;
    let follow = Follow { path: "/wave/runner_3".into(), offset: Vec3::new(0.0, 2.0, 5.0), look_offset: Vec3::ZERO, local: true };
    let spec = CameraSpec { name: "chase".into(), eye: None, target: None, fov_y_deg: 60.0, follow: Some(follow) };
    world.set_camera("ann", spec, true).unwrap();
    let mut cameras = Cameras::default();
    cameras.resolve("ann", "chase", None, &world).unwrap(); // warm up
    let mut allocations = 0;
    for frame in 1..=100 {
        // The runner moves every frame (the game's tick); the window follows it.
        let p = Vec3::new(frame as f32 * 0.1, 0.0, 0.0);
        world.update(&EntityKey::Id(id), UpdateSpec { position: Some(p), ..UpdateSpec::default() }, "t").unwrap();
        let before = ALLOCATIONS.load(Ordering::Relaxed);
        let r = cameras.resolve("ann", "chase", None, &world).unwrap();
        allocations += ALLOCATIONS.load(Ordering::Relaxed) - before;
        assert!((r.camera.eye.x - p.x).abs() < 1e-4, "it follows");
    }
    assert_eq!(allocations, 0, "100 frames of following allocated {allocations} times");
}
