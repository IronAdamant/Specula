//! Sight's ID buffer on 2D items: sprites, tile chunks and vector shapes get ids (after the mesh
//! draws' in `render_with_ids`), hidden ones count 0 pixels, covered ones only what shows, and
//! cut texels (cutout alpha, blended alpha 0) count nothing, on every adapter, under validation.
//! Skips cleanly without Vulkan.

mod common;

use common::*;
use specula_gpu::vector::{fill, Path, ShapeMesh};
use specula_gpu::{
    AdapterPreference, Draw2d, DrawItem, FrameDesc, Gpu, ShapeDraw, Sprite, SpriteBlend, TextureFilter, TextureFormat, TileCell, TileChunk, TileGrid,
};

const W: u32 = 64;
const H: u32 = 64;

/// Orthographic, reversed Z, looking down −Z from z = 50 at the origin: 8 × 8 m (8 px per metre).
fn ortho() -> [f32; 16] {
    let (near, far, w, h) = (0.05f32, 100.0f32, 8.0f32, 8.0f32);
    let d = far - near;
    let proj = [2.0 / w, 0.0, 0.0, 0.0, 0.0, -2.0 / h, 0.0, 0.0, 0.0, 0.0, 1.0 / d, 0.0, 0.0, 0.0, far / d, 1.0];
    mul(&proj, &translation([0.0, 0.0, -50.0]))
}

fn frame<'a>(draws: &'a [DrawItem], draws_2d: &'a [Draw2d]) -> FrameDesc<'a> {
    FrameDesc { view_proj: ortho(), clear_color: [0.0, 0.0, 0.0, 1.0], draws, draws_2d, reverse_z: true, ..Default::default() }
}

fn run(adapter: AdapterPreference, label: &str) {
    let test = format!("ids_2d[{label}]");
    let Some(mut gpu) = gpu_or_skip(&test, adapter) else { return };
    let (cv, ci) = cube();
    let cube = gpu.upload_mesh(&cv, &ci).unwrap();
    let target = gpu.create_target(W, H).unwrap();
    let counting = gpu.ids_count_blended();
    let coverage = |gpu: &mut Gpu, draws: &[DrawItem], draws_2d: &[Draw2d]| {
        let ids: Vec<u32> = (1..=(draws.len() + draws_2d.len()) as u32).collect();
        gpu.render_with_ids(target, &frame(draws, draws_2d), &ids).unwrap();
        gpu.id_coverage(target, ids.len() as u32).unwrap().per_id
    };
    // A wall at z = 1 in front of everything at z = 0: 4 × 4 m in the middle.
    let wall = DrawItem::new(cube, mul(&translation([0.0, 0.0, 1.0]), &scale3([4.0, 4.0, 0.2])), [0.5, 0.5, 0.5, 1.0]);
    // A 2 m flat cutout sprite (16 × 16 px).
    let sprite = Sprite { size: [2.0, 2.0], pivot: [0.5, 0.5], blend: SpriteBlend::Cutout { threshold: 0.5 }, ..Default::default() };
    let alone = coverage(&mut gpu, &[], &[Draw2d::Sprite(sprite)])[0];
    assert_eq!((alone.opaque_pixels, alone.min, alone.max), (256, [24, 24], [39, 39]), "{test}: a cutout sprite writes opaque ids");
    let hidden = coverage(&mut gpu, &[wall], &[Draw2d::Sprite(sprite)]);
    assert_eq!((hidden[0].pixels, hidden[1].pixels), (1024, 0), "{test}: behind the wall the sprite shows nothing: {hidden:?}");
    if counting {
        assert_eq!(hidden[1].rasterized, Some(true), "{test}: ...but it was drawn: occluded, not sub-pixel");
    }
    // A blended sprite behind the wall: nothing either; in the open, its pixels through the sets.
    let soft = Sprite { blend: SpriteBlend::Blended, ..sprite };
    if counting {
        let behind = coverage(&mut gpu, &[wall], &[Draw2d::Sprite(soft)]);
        assert_eq!(behind[1].pixels, 0, "{test}: blended, behind the wall: {behind:?}");
        let open = coverage(&mut gpu, &[], &[Draw2d::Sprite(soft)])[0];
        assert_eq!((open.blended_pixels, open.opaque_pixels, open.approx), (256, 0, false), "{test}: blended sprite {open:?}");
    }
    // Cut texels count nothing: a 4 × 4 texture whose left half has alpha 0.
    let mut px = Vec::new();
    for _ in 0..4 {
        for x in 0..4 {
            px.extend_from_slice(&[255, 255, 255, if x < 2 { 0 } else { 255 }]);
        }
    }
    let tex = gpu.upload_texture(4, 4, TextureFormat::Rgba8Srgb, &px).unwrap();
    let half = Sprite { texture: Some(tex), filter: TextureFilter::Nearest, ..sprite };
    let cut = coverage(&mut gpu, &[], &[Draw2d::Sprite(half)])[0];
    assert_eq!((cut.opaque_pixels, cut.min, cut.max), (128, [32, 24], [39, 39]), "{test}: the cutout's transparent half: {cut:?}");
    if counting {
        let soft_half = coverage(&mut gpu, &[], &[Draw2d::Sprite(Sprite { blend: SpriteBlend::Blended, ..half })])[0];
        assert_eq!((soft_half.blended_pixels, soft_half.min, soft_half.max), (128, [32, 24], [39, 39]), "{test}: blended, alpha 0 shows nothing");
    }
    // A tile map half covered by the wall: 8 × 2 tiles of 1 m from (−4, 1) (8 × 2 m: 64 × 16 px),
    // the wall over its middle 4 m.
    let tileset = gpu.upload_texture(8, 8, TextureFormat::Rgba8Srgb, &[255u8; 8 * 8 * 4]).unwrap();
    let mut grid = TileGrid::new(8, 2);
    for x in 0..8 {
        for y in 0..2 {
            grid.set(x, y, TileCell::new(1));
        }
    }
    let cells = gpu.upload_texture(8, 2, TextureFormat::Rgba8Unorm, &grid.encode()).unwrap();
    let chunk = TileChunk { origin: [-4.0, 1.0, 0.0], ..TileChunk::new(tileset, [8, 8], cells, [0, 0, 8, 2]) };
    let open = coverage(&mut gpu, &[], &[Draw2d::Tiles(chunk)])[0];
    assert_eq!(open.opaque_pixels, 64 * 16, "{test}: the whole map: {open:?}");
    let covered = coverage(&mut gpu, &[wall], &[Draw2d::Tiles(chunk)]);
    assert_eq!(covered[1].opaque_pixels, 32 * 16, "{test}: half the map shows: {covered:?}");
    // A vector shape (2 × 2 m, no fringe): blended, through the sets; hidden behind the wall.
    if counting {
        let mut m = ShapeMesh::default();
        fill(&Path::rect(-1.0, -1.0, 2.0, 2.0).flatten(0.01).unwrap(), [255, 255, 255, 255], false, &mut m).unwrap();
        let mesh = gpu.upload_mesh(&m.vertices, &m.indices).unwrap();
        let shape = Draw2d::Shape(ShapeDraw { mesh, model: IDENTITY, color: [1.0; 4], layer: 0 });
        let open = coverage(&mut gpu, &[], &[shape])[0];
        assert_eq!((open.blended_pixels, open.min, open.max), (256, [24, 24], [39, 39]), "{test}: shape {open:?}");
        let behind = coverage(&mut gpu, &[wall], &[shape]);
        assert_eq!((behind[1].pixels, behind[1].rasterized), (0, Some(true)), "{test}: shape behind the wall {behind:?}");
    }
    // Mesh ids come first, 2D after: a wrong length is a structured error.
    let e = gpu.render_with_ids(target, &frame(&[wall], &[Draw2d::Sprite(sprite)]), &[1]).unwrap_err();
    assert!(matches!(e.reason, Some(specula_gpu::ErrorReason::DataLength { given: 1, expected: 2 })), "{e:?}");
    let errors = validation_errors(&test, gpu);
    assert!(errors.is_empty(), "{test}: {errors:#?}");
}

fn scale3(s: [f32; 3]) -> [f32; 16] {
    [s[0], 0.0, 0.0, 0.0, 0.0, s[1], 0.0, 0.0, 0.0, 0.0, s[2], 0.0, 0.0, 0.0, 0.0, 1.0]
}

#[test]
fn ids_count_2d_items_like_the_image() {
    let Ok(adapters) = specula_gpu::enumerate_adapters() else {
        eprintln!("ids_2d: SKIPPED — Vulkan is not available");
        return;
    };
    for a in adapters {
        run(AdapterPreference::Index(a.index), &format!("{}", a.index));
    }
}
