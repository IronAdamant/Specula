//! A warm frame allocates nothing on the heap: preparing, declaring and recording the frame graph
//! (every feature on) reuses last frame's storage and compiled plan. Counts this thread's
//! allocations through a counting global allocator (drivers allocate through their own C runtime,
//! not through Rust's allocator, so only Specula's own allocations are seen).

mod common;

use common::*;
use specula_gpu::vector::{fill, Path, ShapeMesh};
use specula_gpu::{
    AdapterPreference, Billboard, Draw2d, DrawItem, Environment, FrameDesc, Material, MaterialTextures, MeshStreams, OverlayQuad,
    PointLight, ShapeDraw, SkinVertex, Sprite, SpriteBlend, Surface, TextureFormat, TextureSampler, TileCell, TileChunk, TileGrid,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Counting;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: forwards to the system allocator unchanged; only counts.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: the caller's contract is passed through.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: as above.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        // SAFETY: as above.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

fn count() {
    // `try_with`: thread-locals may be gone while a thread shuts down.
    let _ = COUNTING.try_with(|on| {
        if on.get() {
            let _ = COUNT.try_with(|c| c.set(c.get() + 1));
        }
    });
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn allocations_during(f: impl FnOnce()) -> usize {
    COUNT.with(|c| c.set(0));
    COUNTING.with(|on| on.set(true));
    f();
    COUNTING.with(|on| on.set(false));
    COUNT.with(|c| c.get())
}

#[test]
fn warm_frames_do_not_allocate() {
    let test = "warm_frames_do_not_allocate";
    let Some(mut gpu) = gpu_or_skip(test, AdapterPreference::LowPower) else { return };
    let (v, i) = cube();
    let mesh = gpu.upload_mesh(&v, &i).unwrap();
    let texture = gpu.upload_texture(2, 2, TextureFormat::Rgba8Srgb, &[255; 16]).unwrap();
    let (w, h) = (96, 64);
    let target = gpu.create_target(w, h).unwrap();
    let view_proj = mul(&perspective_vk_reverse_z(1.0, w as f32 / h as f32, 0.05), &look_at_rh([3.0, 4.0, 6.0], [0.0; 3], [0.0, 1.0, 0.0]));
    let mut draws: Vec<DrawItem> = (0..20)
        .map(|k| DrawItem::new(mesh, translation([k as f32 * 0.3 - 3.0, 0.0, 0.0]), [0.6, 0.5, 0.4, 1.0]))
        .collect();
    let mut glass = DrawItem::new(mesh, translation([0.0, 1.0, 0.0]), [0.8, 0.9, 1.0, 0.3]);
    glass.material = Material::Glass;
    draws.push(glass);
    let mut glow = DrawItem::new(mesh, translation([1.0, 1.0, 0.0]), [1.0, 0.4, 0.1, 1.0]);
    glow.surface = Surface { emissive: 4.0, ..Default::default() };
    draws.push(glow);
    // A skinned, textured cube: its palette comes from the frame's bones.
    let uvs: Vec<[f32; 2]> = (0..v.len()).map(|k| [(k % 2) as f32, (k / 2 % 2) as f32]).collect();
    let skin = vec![SkinVertex { joints: [1, 0, 0, 0], weights: [255, 0, 0, 0] }; v.len()];
    let posed = gpu.upload_mesh_with(&v, &i, &MeshStreams { skin: Some(&skin), uvs: Some(&uvs), tangents: None }).unwrap();
    wait_variants(&mut gpu, true, true);
    let material = gpu.upload_material_texture(4, 4, TextureFormat::Rgba8Srgb, &[200; 64], TextureSampler::default()).unwrap();
    let mut creature = DrawItem::new(posed, translation([-1.0, 1.0, 0.0]), [1.0; 4]);
    creature.skin = Some(0);
    creature.surface.textures = MaterialTextures { base_color: Some(material), ..Default::default() };
    draws.push(creature);
    let bones = [translation([0.0; 3]), translation([0.0, 0.5, 0.0])];
    let lights = [PointLight { position: [0.0, 2.0, 0.0], color: [1.0, 0.8, 0.6], intensity: 2.0, range: 5.0 }];
    let overlay = [
        OverlayQuad::solid([4.0, 4.0, 30.0, 10.0], [0.1, 0.1, 0.1, 0.8]),
        OverlayQuad::textured([40.0, 4.0, 8.0, 8.0], [0.0, 0.0, 1.0, 1.0], texture, Default::default(), [1.0; 4]),
    ];
    // 2D: a lit, shadow-casting cutout billboard, a blended sprite, a tile chunk and a shape.
    let mut grid = TileGrid::new(4, 4);
    grid.set(1, 1, TileCell::animated(1, 2, 10));
    let cells = gpu.upload_texture(4, 4, TextureFormat::Rgba8Unorm, &grid.encode()).unwrap();
    let mut shape = ShapeMesh::default();
    fill(&Path::circle([0.0, 0.0], 0.5).flatten(0.01).unwrap(), [200, 50, 50, 255], true, &mut shape).unwrap();
    let shape_mesh = gpu.upload_mesh(&shape.vertices, &shape.indices).unwrap();
    let card = Sprite {
        position: [0.0, 1.5, 1.0],
        texture: Some(texture),
        billboard: Billboard::Cylindrical,
        blend: SpriteBlend::Cutout { threshold: 0.5 },
        lit: true,
        cast_shadows: true,
        ..Default::default()
    };
    let draws_2d = [
        Draw2d::Sprite(card),
        Draw2d::Sprite(Sprite { position: [1.0, 0.5, 1.0], billboard: Billboard::Spherical, color: [1.0, 1.0, 1.0, 0.5], ..Default::default() }),
        Draw2d::Tiles(TileChunk { origin: [-2.0, 0.01, -2.0], axes: [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]], ..TileChunk::new(texture, [1, 1], cells, [0, 0, 4, 4]) }),
        Draw2d::Shape(ShapeDraw { mesh: shape_mesh, model: translation([0.0, 2.0, 0.0]), color: [1.0; 4], layer: 1 }),
    ];
    let mut environment = Environment { tonemap: true, fxaa: true, ..Default::default() };
    environment.shadow.enabled = true;
    environment.shadow.cascades = 2;
    environment.bloom.enabled = true;
    environment.sky.enabled = true;
    environment.fog.enabled = true;
    // Grading (every step, with a LUT) and SSAO.
    let lut_pixels: Vec<u8> = (0..4 * 4 * 4).flat_map(|i| [(i % 4 * 85) as u8, (i / 16 * 85) as u8, (i / 4 % 4 * 85) as u8, 255]).collect();
    let lut = gpu.upload_texture(16, 4, TextureFormat::Rgba8Unorm, &lut_pixels).unwrap();
    let g = &mut environment.grading;
    (g.enabled, g.temperature, g.contrast, g.vibrance, g.lift, g.lut, g.vignette, g.grain) =
        (true, -0.2, 1.1, -0.3, [0.0, 0.0, 0.02], Some(lut), 0.3, 0.2);
    environment.ssao.enabled = true;
    gpu.mark_warm();
    let frame = FrameDesc {
        view_proj,
        clear_color: [0.1, 0.1, 0.2, 1.0],
        draws: &draws,
        reverse_z: true,
        environment,
        lights: &lights,
        time_s: 1.0,
        overlay: &overlay,
        draws_2d: &draws_2d,
        bones: &bones,
    };
    for _ in 0..5 {
        gpu.render(target, &frame).unwrap();
    }
    let compiles = gpu.last_frame_stats().graph.compiles;
    let n = allocations_during(|| {
        for _ in 0..20 {
            gpu.render(target, &frame).unwrap();
        }
    });
    let stats = gpu.last_frame_stats();
    eprintln!("{test}: {n} allocations in 20 warm frames; graph {:?}", stats.graph);
    assert_eq!(n, 0, "warm frames allocate");
    assert!(!stats.graph.recompiled, "an unchanged frame reuses the compiled graph");
    assert_eq!(stats.graph.compiles, compiles, "no recompiles while warm");
    assert_eq!(stats.pipelines.created_after_warm, 0, "every pipeline was prebuilt: {:?}", stats.pipelines);
    let graph = gpu.last_graph().expect("a frame was rendered");
    assert_eq!(stats.graph.barriers as usize, graph.barrier_count());
    assert!(graph.passes.iter().any(|p| p.name == "overlay" && p.inline), "{graph}");
    assert!(graph.passes.iter().any(|p| p.name == "sprites" && p.inline), "{graph}");
    assert_eq!((stats.draws.sprites, stats.draws.tile_chunks, stats.draws.shapes, stats.draws.shadow_sprites), (2, 1, 1, 1));
    gpu.destroy_target(target);
    gpu.destroy_texture(texture);
    gpu.destroy_texture(material);
    gpu.destroy_texture(lut);
    gpu.destroy_mesh(mesh);
    gpu.destroy_mesh(posed);
    let errors = validation_errors(test, gpu);
    assert!(errors.is_empty(), "{test}: {} validation error(s)", errors.len());
}
