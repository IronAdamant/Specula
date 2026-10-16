//! Foundation F3: once warm, a window frame of the game (input, the HUD's layout and overlay,
//! moving the dead between ticks, sound) allocates nothing.
//!
//! The only test in this file, so the counting allocator sees no other test's threads.

use sepulcra::Content;
use sepulcra_app::SepulcraGame;
use specula::game::{Clock, Ctx, Frame, Game, Overlay};
use specula::sight::Camera;
use specula::world::{Recording, World};
use specula_core::math::Vec3;
use specula_core::Json;
use specula_gpu::{GpuError, OverlayTextures, TextureFormat, TextureId};
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

/// Textures for the HUD without a GPU.
struct Textures(u32);

impl OverlayTextures for Textures {
    fn upload_texture(&mut self, _w: u32, _h: u32, _f: TextureFormat, _p: &[u8]) -> Result<TextureId, GpuError> {
        self.0 += 1;
        Ok(TextureId(self.0))
    }
    fn update_texture(&mut self, _t: TextureId, _x: u32, _y: u32, _w: u32, _h: u32, _p: &[u8]) -> Result<(), GpuError> {
        Ok(())
    }
    fn destroy_texture(&mut self, _t: TextureId) {}
}

#[test]
fn a_warm_window_frame_allocates_nothing() {
    let mut game = SepulcraGame::new(Arc::new(Content::builtin()), 7);
    let mut world = World::default();
    let mut clock = Clock::new(game.tick_hz());
    game.start(&mut Ctx { world: &mut world, clock: &clock, agent: "sepulcra", windowed: false });
    game.call_tool(
        &mut Ctx { world: &mut world, clock: &clock, agent: "sepulcra", windowed: false },
        "play_new",
        &Json::obj([("seed", Json::from("7")), ("layout", Json::from("centre"))]),
    );
    // Into wave 2, with the dead on the slab and stones shooting.
    for _ in 0..2600 {
        game.tick(&mut Ctx { world: &mut world, clock: &clock, agent: "sepulcra", windowed: false });
        clock.finish_tick();
    }
    assert!(game.state().enemies().len() >= 4, "wave 2 is on the slab ({} enemies)", game.state().enemies().len());
    let mut camera = Some(Camera::new(Vec3::new(38.0, 27.0, 52.0), Vec3::new(0.0, -9.0, 1.0), 45.0));
    let mut title = None;
    let mut quads = Vec::new();
    let mut textures = Textures(0);
    let mut frame = |game: &mut SepulcraGame, world: &mut World, alpha: f32| {
        let mut f = Frame { input: &[], size: (1280, 720), alpha, dt_s: 0.016, camera: &mut camera, agent_view: false, title: &mut title };
        // As the engine runs a frame: game changes are coalesced, transient ones not recorded.
        world.set_recording(Recording::Snapshot);
        game.frame(&mut Ctx { world, clock: &clock, agent: "sepulcra", windowed: false }, &mut f);
        let cam = camera.unwrap();
        quads.clear();
        let mut o = Overlay { size: (1280, 720), camera: cam, capture: false, quads: &mut quads, textures: &mut textures };
        game.overlay(&mut Ctx { world, clock: &clock, agent: "sepulcra", windowed: false }, &mut o);
        world.set_recording(Recording::Full);
        drop(world.take_changes());
    };
    for i in 0..20 {
        frame(&mut game, &mut world, (i % 10) as f32 / 10.0);
    }
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    for i in 0..10 {
        frame(&mut game, &mut world, 0.05 + i as f32 / 10.0);
    }
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before;
    assert_eq!(allocations, 0, "10 warm frames allocated {allocations} times");
}
