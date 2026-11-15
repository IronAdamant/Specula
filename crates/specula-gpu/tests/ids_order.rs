//! Sight's ID buffer follows the colour pass's order (M5 gpuwork, the text lane's review R3): a
//! later draw that paints over an earlier one about opaquely (alpha at least 0.9: opaque glass, a
//! sprite or shape on a higher layer behind it, a cutout, text) hides it in the counts too, as it
//! does in the image. Per subject, the pixels the image shows in its own colour are compared with
//! the pixels the ID buffer counts for it: glass behind a blended sprite and behind text, a
//! higher-layer shape and a higher-layer cutout behind them, a blended sprite over an opaque cube;
//! and see-through glass (0.3) hides nothing. On every adapter, under validation; skips cleanly
//! without Vulkan or fragment atomics.

mod common;

use common::*;
use specula_gpu::vector::{fill, Path, ShapeMesh};
use specula_gpu::{AdapterPreference, Draw2d, DrawItem, FrameDesc, GlyphInstance, Gpu, Material, ShapeDraw, Sprite, SpriteBlend, TargetId, TextFrame, TextRun};
use specula_text::{layout, slug, Bundled, SlugStore, TextStyle};

const W: u32 = 64;
const H: u32 = 64;
const IDENTITY: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// Orthographic, reversed Z, looking down −Z from z = 50 at the origin: 8 × 8 m (8 px per metre).
fn ortho() -> [f32; 16] {
    let (near, far, w, h) = (0.05f32, 100.0f32, 8.0f32, 8.0f32);
    let d = far - near;
    let proj = [2.0 / w, 0.0, 0.0, 0.0, 0.0, -2.0 / h, 0.0, 0.0, 0.0, 0.0, 1.0 / d, 0.0, 0.0, 0.0, far / d, 1.0];
    mul(&proj, &translation([0.0, 0.0, -50.0]))
}

fn scale3(t: [f32; 3], s: [f32; 3]) -> [f32; 16] {
    [s[0], 0.0, 0.0, 0.0, 0.0, s[1], 0.0, 0.0, 0.0, 0.0, s[2], 0.0, t[0], t[1], t[2], 1.0]
}

struct Scene<'a> {
    draws: Vec<DrawItem>,
    draws_2d: Vec<Draw2d>,
    text: TextFrame<'a>,
}

/// Renders `s` with ids 1.. (meshes, then 2D items): `(pixels per id, rgba)`.
fn counted(gpu: &mut Gpu, target: TargetId, s: &Scene) -> (Vec<u64>, Vec<u8>) {
    let n = s.draws.len() + s.draws_2d.len();
    let ids: Vec<u32> = (1..=n as u32).collect();
    let frame = FrameDesc { view_proj: ortho(), clear_color: [0.0, 0.0, 0.0, 1.0], draws: &s.draws, draws_2d: &s.draws_2d, text: s.text, reverse_z: true, ..Default::default() };
    gpu.render_with_ids(target, &frame, &ids).unwrap();
    let rgba = gpu.read_rgba8(target).unwrap();
    let per = gpu.id_coverage(target, n as u32).unwrap().per_id.iter().map(|c| c.pixels).collect();
    (per, rgba)
}

/// Pixels of `rgba` within 24 (per channel) of `c`.
fn showing(rgba: &[u8], c: [u8; 3]) -> u64 {
    rgba.as_chunks::<4>().0.iter().filter(|p| (0..3).all(|i| (p[i] as i32 - c[i] as i32).abs() <= 24)).count() as u64
}

/// The colour a subject draws in, alone (the middle of the image).
fn colour_alone(gpu: &mut Gpu, target: TargetId, s: &Scene, at: (u32, u32)) -> [u8; 3] {
    let (_, rgba) = counted(gpu, target, s);
    let p = pixel(&rgba, W, at.0, at.1);
    [p[0], p[1], p[2]]
}

fn run(adapter: AdapterPreference, label: &str) {
    let test = format!("ids_order[{label}]");
    let Some(mut gpu) = gpu_or_skip(&test, adapter) else { return };
    if !gpu.ids_count_blended() {
        eprintln!("{test}: SKIPPED — no fragment atomics (blended draws aren't counted)");
        return;
    }
    let (cv, ci) = cube();
    let cube = gpu.upload_mesh(&cv, &ci).unwrap();
    let target = gpu.create_target(W, H).unwrap();
    let none = TextFrame::default();
    // The covered subject: a blended red sprite, 2 × 2 m (16 × 16 px) in the middle, in front.
    let red = Sprite { position: [0.0, 0.0, 0.5], size: [2.0, 2.0], pivot: [0.5, 0.5], color: [1.0, 0.0, 0.0, 1.0], blend: SpriteBlend::Blended, ..Default::default() };
    let red_colour = colour_alone(&mut gpu, target, &Scene { draws: vec![], draws_2d: vec![Draw2d::Sprite(red)], text: none }, (32, 32));
    let glass = |opacity: f32| {
        let mut g = DrawItem::new(cube, scale3([0.0, 0.0, 0.0], [3.0, 3.0, 0.2]), [0.3, 0.6, 0.9, opacity]);
        g.material = Material::Glass;
        g
    };

    // 1. Opaque glass behind the sprite: drawn after every 2D item, it paints over the sprite
    // (which wrote no depth). The image shows none of the sprite; neither do the counts.
    let s = Scene { draws: vec![glass(1.0)], draws_2d: vec![Draw2d::Sprite(red)], text: none };
    let (ids, rgba) = counted(&mut gpu, target, &s);
    let shown = showing(&rgba, red_colour);
    assert_eq!(shown, 0, "{test}: opaque glass hides the sprite in the image");
    assert_eq!(ids[1], shown, "{test}: ...and in the counts (sprite {} px, glass {} px)", ids[1], ids[0]);
    assert!(ids[0] >= 24 * 24 - 8, "{test}: the glass counts its pixels: {ids:?}");

    // 2. See-through glass (0.3) hides nothing: the sprite keeps its 256 pixels.
    let s = Scene { draws: vec![glass(0.3)], draws_2d: vec![Draw2d::Sprite(red)], text: none };
    let (ids, _) = counted(&mut gpu, target, &s);
    assert_eq!(ids[1], 256, "{test}: see-through glass hides nothing: {ids:?}");

    // 3. A shape on a higher layer behind the sprite: drawn later (layers first), it paints over.
    let mut m = ShapeMesh::default();
    fill(&Path::rect(-1.5, -1.5, 3.0, 3.0).flatten(0.01).unwrap(), [255, 255, 255, 255], false, &mut m).unwrap();
    let mesh = gpu.upload_mesh(&m.vertices, &m.indices).unwrap();
    let shape = Draw2d::Shape(ShapeDraw { mesh, model: IDENTITY, color: [0.0, 1.0, 0.0, 1.0], layer: 5 });
    let s = Scene { draws: vec![], draws_2d: vec![Draw2d::Sprite(red), shape], text: none };
    let (ids, rgba) = counted(&mut gpu, target, &s);
    let shown = showing(&rgba, red_colour);
    assert_eq!((shown, ids[0]), (0, 0), "{test}: a higher-layer shape behind the sprite covers it, image and counts: {ids:?}");
    assert_eq!(ids[1], 24 * 24, "{test}: the shape counts all its pixels: {ids:?}");

    // 4. A cutout sprite on a higher layer behind it: the same.
    let blue = Sprite { position: [0.0, 0.0, 0.0], size: [3.0, 3.0], pivot: [0.5, 0.5], color: [0.0, 0.0, 1.0, 1.0], layer: 5, blend: SpriteBlend::Cutout { threshold: 0.5 }, ..Default::default() };
    let s = Scene { draws: vec![], draws_2d: vec![Draw2d::Sprite(red), Draw2d::Sprite(blue)], text: none };
    let (ids, rgba) = counted(&mut gpu, target, &s);
    assert_eq!((showing(&rgba, red_colour), ids[0]), (0, 0), "{test}: a higher-layer cutout behind the sprite covers it: {ids:?}");
    assert_eq!(ids[1], 24 * 24, "{test}: the cutout counts all its pixels: {ids:?}");
    // On a lower layer it is drawn first: the sprite paints over it.
    let s = Scene { draws: vec![], draws_2d: vec![Draw2d::Sprite(red), Draw2d::Sprite(Sprite { layer: -5, ..blue })], text: none };
    let (ids, rgba) = counted(&mut gpu, target, &s);
    assert_eq!((ids[0], showing(&rgba, red_colour)), (256, 256), "{test}: drawn after the cutout, the sprite shows: {ids:?}");
    assert_eq!(ids[1], 24 * 24 - 256, "{test}: ...and the cutout only around it: {ids:?}");

    // 5. A blended sprite over an opaque cube (drawn after every mesh): the cube shows only
    // around it, in the image and the counts.
    let wall = DrawItem::new(cube, scale3([0.0, 0.0, -0.5], [3.0, 3.0, 0.2]), [0.5, 0.5, 0.5, 1.0]);
    let s = Scene { draws: vec![wall], draws_2d: vec![Draw2d::Sprite(red)], text: none };
    let (ids, rgba) = counted(&mut gpu, target, &s);
    assert_eq!((ids[1], showing(&rgba, red_colour)), (256, 256), "{test}: {ids:?}");
    assert_eq!(ids[0], 24 * 24 - 256, "{test}: the cube is hidden under the opaque sprite: {ids:?}");
    // Half see-through, it hides nothing.
    let s = Scene { draws: vec![wall], draws_2d: vec![Draw2d::Sprite(Sprite { color: [1.0, 0.0, 0.0, 0.5], ..red })], text: none };
    let (ids, _) = counted(&mut gpu, target, &s);
    assert_eq!(ids[0], 24 * 24, "{test}: a half see-through sprite hides nothing: {ids:?}");

    // 6. Text (Slug glyphs, blended) with opaque glass behind it, or a higher-layer shape: hidden
    // in the image and the counts; alone, it counts its pixels.
    let font = Bundled::Display.load().unwrap();
    let mut store = SlugStore::new(slug::DEFAULT_STORE_BYTES);
    let mut glyphs = Vec::new();
    // One big glyph inside the covering glass's and shape's 3 x 3 m.
    let l = layout(&font, "H", &TextStyle::new(2.4));
    for g in &l.glyphs {
        let Some(sg) = store.glyph(&font, g.glyph).unwrap() else { continue };
        glyphs.push(GlyphInstance { word: sg.word, origin: [-0.9 + g.x, 1.2 - g.y], size: 2.4, bounds: sg.bounds, color: [1.0, 1.0, 0.0, 1.0] });
    }
    let rect = [-0.9, 1.2 - l.height, -0.9 + l.width, 1.2];
    let at_front = scale3([0.0, 0.0, 0.5], [1.0, 1.0, 1.0]);
    let text = Draw2d::Text(TextRun { model: at_front, rect, first: 0, count: glyphs.len() as u32, color: [1.0, 1.0, 0.0, 1.0], layer: 0, lit: false });
    let tf = TextFrame { store: store.id(), words: store.words(), glyphs: &glyphs };
    let alone = counted(&mut gpu, target, &Scene { draws: vec![], draws_2d: vec![text], text: tf });
    let yellow = showing(&alone.1, [255, 255, 0]);
    assert!(alone.0[0] > 60 && yellow > 20, "{test}: the text alone: {} px counted, {yellow} px yellow", alone.0[0]);
    let (ids, rgba) = counted(&mut gpu, target, &Scene { draws: vec![glass(1.0)], draws_2d: vec![text], text: tf });
    assert_eq!((showing(&rgba, [255, 255, 0]), ids[1]), (0, 0), "{test}: opaque glass behind the text hides it, image and counts: {ids:?}");
    let (ids, rgba) = counted(&mut gpu, target, &Scene { draws: vec![], draws_2d: vec![text, shape], text: tf });
    assert_eq!((showing(&rgba, [255, 255, 0]), ids[0]), (0, 0), "{test}: a higher-layer shape behind the text hides it: {ids:?}");
    let errors = validation_errors(&test, gpu);
    assert!(errors.is_empty(), "{test}: {errors:#?}");
}

#[test]
fn ids_follow_the_colour_passes_order() {
    let Ok(adapters) = specula_gpu::enumerate_adapters() else {
        eprintln!("ids_order: SKIPPED — Vulkan is not available");
        return;
    };
    for a in adapters {
        run(AdapterPreference::Index(a.index), &format!("{}", a.index));
    }
}
