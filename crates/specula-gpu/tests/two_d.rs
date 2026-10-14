//! 2D on real GPUs: sprites (pixel-exact pixel art, flips, layers, cutout and blended, billboards,
//! batching, shadows), tile maps (orientations, animation, empty cells, chunk culling) and vector
//! shapes (anti-aliased, concave, holes). Every test skips cleanly without Vulkan and requires zero
//! validation errors (synchronization validation included). Captures go to `$SPECULA_TEST_OUT`
//! (or cargo's per-target temp dir) as `two_d_*.png`.

mod common;

use common::*;
use specula_gpu::vector::{fill, stroke, LineCap, LineJoin, Path, ShapeMesh, StrokeStyle};
use specula_gpu::{
    AdapterPreference, Billboard, Draw2d, DrawItem, Environment, FrameDesc, Gpu, ShapeDraw, Sprite, SpriteBlend, TextureFilter,
    TextureFormat, TextureId, TileCell, TileChunk, TileGrid, ORIENT_FLIP_X,
};

/// Orthographic, reversed Z, looking down −Z from z = 50 at (`cx`, `cy`): `w` × `h` metres.
fn ortho(cx: f32, cy: f32, w: f32, h: f32) -> [f32; 16] {
    let (near, far) = (0.05f32, 100.0f32);
    let d = far - near;
    let proj = [2.0 / w, 0.0, 0.0, 0.0, 0.0, -2.0 / h, 0.0, 0.0, 0.0, 0.0, 1.0 / d, 0.0, 0.0, 0.0, far / d, 1.0];
    mul(&proj, &translation([-cx, -cy, -50.0]))
}

const CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// A `n` × `n` RGBA texture whose texel (x, y) is `f(x, y)`.
fn texture(gpu: &mut Gpu, n: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> TextureId {
    let mut px = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            px.extend_from_slice(&f(x, y));
        }
    }
    gpu.upload_texture(n, n, TextureFormat::Rgba8Srgb, &px).unwrap()
}

/// Four distinct colours for a 2 x 2 checker of texels (top-left red, top-right green, bottom-left
/// blue, bottom-right yellow).
fn quadrant(x: u32, y: u32, n: u32) -> [u8; 4] {
    match (x * 2 / n, y * 2 / n) {
        (0, 0) => [255, 0, 0, 255],
        (1, 0) => [0, 255, 0, 255],
        (0, _) => [0, 0, 255, 255],
        _ => [255, 255, 0, 255],
    }
}

fn render(gpu: &mut Gpu, w: u32, h: u32, frame: &FrameDesc) -> Vec<u8> {
    let target = gpu.create_target(w, h).unwrap();
    gpu.render(target, frame).unwrap();
    // Kept (freed with the GPU) so last_graph() still describes the frame.
    gpu.read_rgba8(target).unwrap()
}

fn finish(test: &str, gpu: Gpu) {
    let errors = validation_errors(test, gpu);
    assert!(errors.is_empty(), "{test}: {} validation errors", errors.len());
}

#[test]
fn pixel_art_sprites_are_exact_flipped_and_snapped() {
    for adapter in [AdapterPreference::HighPerformance, AdapterPreference::LowPower] {
        let Some(mut gpu) = gpu_or_skip("pixel_art_sprites", adapter) else { return };
        let tex = texture(&mut gpu, 4, |x, y| quadrant(x, y, 4));
        // 64 x 64 pixels over 8 x 8 m: 8 px per metre; a 4 m sprite is 32 px, 8 px per texel.
        let (w, h) = (64u32, 64u32);
        let base = Sprite {
            size: [2.0, 2.0],
            texture: Some(tex),
            blend: SpriteBlend::Cutout { threshold: 0.5 },
            filter: TextureFilter::Nearest,
            pixel_snap: true,
            ..Default::default()
        };
        // Left: as is. Right: mirrored. Both 0.3 px off the pixel grid (snapping fixes it).
        let off = 0.3 / 8.0;
        let draws = [
            Draw2d::Sprite(Sprite { position: [-2.0 + off, 0.0, 0.0], ..base }),
            Draw2d::Sprite(Sprite { position: [2.0 + off, 0.0, 0.0], flip: [true, false], ..base }),
        ];
        let frame = FrameDesc { view_proj: ortho(0.0, 0.0, 8.0, 8.0), clear_color: CLEAR, reverse_z: true, draws_2d: &draws, ..Default::default() };
        let px = render(&mut gpu, w, h, &frame);
        write_png(&format!("two_d_pixel_art_{}.png", gpu.adapter().index), &px, w, h);
        // Left sprite covers x 8..24, y 24..40 (2 m): texels of 4 px, quadrants of 8, exactly.
        for (x, y, want) in [(8, 24, [255, 0, 0]), (15, 31, [255, 0, 0]), (16, 24, [0, 255, 0]), (8, 32, [0, 0, 255]), (23, 39, [255, 255, 0])] {
            assert_eq!(pixel(&px, w, x, y)[..3], want, "left sprite at ({x}, {y})");
        }
        // Right sprite (x 40..56) mirrored: green on the left.
        for (x, y, want) in [(40, 24, [0, 255, 0]), (55, 24, [255, 0, 0]), (40, 39, [255, 255, 0])] {
            assert_eq!(pixel(&px, w, x, y)[..3], want, "flipped sprite at ({x}, {y})");
        }
        // Crisp edges: the pixels just outside are the clear colour, no partial blend.
        for (x, y) in [(7, 30), (24, 30), (16, 23), (16, 40), (39, 30), (56, 30)] {
            assert_eq!(pixel(&px, w, x, y)[..3], [0, 0, 0], "outside at ({x}, {y})");
        }
        let stats = gpu.last_frame_stats();
        assert_eq!((stats.draws.sprites, stats.draws.draw_calls_2d), (2, 1), "one instanced call");
        let graph = gpu.last_graph().unwrap();
        assert!(graph.passes.iter().any(|p| p.name == "sprites" && p.inline), "{graph}");
        let scene = graph.passes.iter().find(|p| p.name == "scene").unwrap();
        let depth = scene.uses.iter().find(|u| u.resource == "depth").unwrap();
        assert_eq!(depth.store_op, "dont_care", "the inline 2D pass doesn't force a depth store: {graph}");
        finish("pixel_art_sprites", gpu);
    }
}

#[test]
fn layers_order_coplanar_cutout_and_blended_items() {
    let Some(mut gpu) = gpu_or_skip("layers", AdapterPreference::HighPerformance) else { return };
    let (w, h) = (64u32, 64u32);
    let solid = |x: f32, layer: i32, color: [f32; 4], blend: SpriteBlend| {
        Draw2d::Sprite(Sprite { position: [x, 0.0, 0.0], size: [4.0, 4.0], color, layer, blend, ..Default::default() })
    };
    let cut = SpriteBlend::Cutout { threshold: 0.5 };
    // All at z = 0. Listed out of order; layers decide. A half-transparent white veil on layer 1
    // sits between a red cutout (layer 0) and a blue cutout (layer 2).
    let draws = [
        solid(1.0, 2, [0.0, 0.0, 1.0, 1.0], cut),
        solid(-1.0, 0, [1.0, 0.0, 0.0, 1.0], cut),
        solid(0.0, 1, [1.0, 1.0, 1.0, 0.5], SpriteBlend::Blended),
    ];
    let frame = FrameDesc { view_proj: ortho(0.0, 0.0, 8.0, 8.0), clear_color: CLEAR, reverse_z: true, draws_2d: &draws, ..Default::default() };
    let px = render(&mut gpu, w, h, &frame);
    write_png("two_d_layers.png", &px, w, h);
    // x = -2.5 m (px 12): red only, under the veil (x -2..2 is veiled; 12 px is x = -2.5 m: not veiled).
    assert_eq!(pixel(&px, w, 12, 32)[..3], [255, 0, 0]);
    // x = -1.5 m (px 20): red under the half-white veil: linear 0.5 * 1 + 0.5 * 1 = 1 red, 0.5 green/blue.
    let veiled = pixel(&px, w, 20, 32);
    assert_eq!(veiled[0], 255);
    assert!((veiled[1] as i32 - 188).abs() <= 2, "{veiled:?}"); // linear 0.5 -> sRGB 188
    // x = 0.5 m (px 36): blue on top of the veil.
    assert_eq!(pixel(&px, w, 36, 32)[..3], [0, 0, 255]);
    let s = gpu.last_frame_stats().draws;
    assert_eq!((s.sprites, s.draw_calls_2d), (3, 3), "cutout, blended, cutout: three batches");
    finish("layers", gpu);
}

#[test]
fn tile_maps_orient_animate_and_cull_by_chunk() {
    let Some(mut gpu) = gpu_or_skip("tile_maps", AdapterPreference::HighPerformance) else { return };
    // Tileset: 4 tiles of 8 x 8 in a 16 x 16 image: tile 1 red with a white top-left texel,
    // 2 green, 3 blue, 4 yellow (2 x 2 grid, numbered left to right, top to bottom).
    let colours = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 0, 255]];
    let tileset = texture(&mut gpu, 16, |x, y| {
        let t = (y / 8) * 2 + x / 8;
        if t == 0 && x % 8 == 0 && y % 8 == 0 {
            [255, 255, 255, 255]
        } else {
            colours[t as usize]
        }
    });
    let mut grid = TileGrid::new(4, 2);
    grid.set(0, 0, TileCell::new(1));
    grid.set(1, 0, TileCell::new(1).rotated_cw(1));
    grid.set(2, 0, TileCell { orient: ORIENT_FLIP_X, ..TileCell::new(1) });
    grid.set(3, 0, TileCell::animated(2, 3, 30)); // green, blue, yellow, half a second each
    grid.set(0, 1, TileCell::new(4));
    // (1, 1) .. (3, 1) stay empty.
    let cells = gpu.upload_texture(4, 2, TextureFormat::Rgba8Unorm, &grid.encode()).unwrap();
    // 1 m tiles, top-left at (0, 2): rows go down. 64 x 32 px over 4 x 2 m = 16 px per metre;
    // each tileset texel is 2 px.
    let chunk = TileChunk { origin: [0.0, 2.0, 0.0], ..TileChunk::new(tileset, [8, 8], cells, [0, 0, 4, 2]) };
    let (w, h) = (64u32, 32u32);
    let mut shots = Vec::new();
    for time_s in [0.25f32, 0.75] {
        let draws = [Draw2d::Tiles(chunk)];
        let frame = FrameDesc { view_proj: ortho(2.0, 1.0, 4.0, 2.0), clear_color: CLEAR, reverse_z: true, draws_2d: &draws, time_s, ..Default::default() };
        let px = render(&mut gpu, w, h, &frame);
        write_png(&format!("two_d_tiles_t{}.png", (time_s * 100.0) as u32), &px, w, h);
        shots.push(px);
    }
    let px = &shots[0];
    // Tile (0, 0): white texel at its top-left corner (2 x 2 px), red elsewhere.
    assert_eq!(pixel(px, w, 0, 0)[..3], [255, 255, 255]);
    assert_eq!(pixel(px, w, 8, 8)[..3], [255, 0, 0]);
    // Turned a quarter clockwise: the white corner moves to the top right.
    assert_eq!(pixel(px, w, 16 + 15, 0)[..3], [255, 255, 255], "rotated tile");
    assert_eq!(pixel(px, w, 16, 0)[..3], [255, 0, 0]);
    // Mirrored: top right too.
    assert_eq!(pixel(px, w, 32 + 15, 0)[..3], [255, 255, 255], "mirrored tile");
    // Animated: green at 0.25 s, blue at 0.75 s.
    assert_eq!(pixel(px, w, 56, 8)[..3], [0, 255, 0]);
    assert_eq!(pixel(&shots[1], w, 56, 8)[..3], [0, 0, 255], "next frame");
    // Row 1: yellow, then empty cells show the clear colour.
    assert_eq!(pixel(px, w, 8, 24)[..3], [255, 255, 0]);
    assert_eq!(pixel(px, w, 24, 24)[..3], [0, 0, 0]);

    // A 256 x 256 map in 64 chunks: a view of a 20 x 12 m corner draws only the chunks it sees.
    let mut big = TileGrid::new(256, 256);
    for y in 0..256 {
        for x in 0..256 {
            big.set(x, y, TileCell::new(1 + ((x + y) % 4) as u16));
        }
    }
    let big_cells = gpu.upload_texture(256, 256, TextureFormat::Rgba8Unorm, &big.encode()).unwrap();
    let draws: Vec<Draw2d> = (0..big.chunk_count())
        .map(|i| Draw2d::Tiles(TileChunk { origin: [0.0, 0.0, 0.0], ..TileChunk::new(tileset, [8, 8], big_cells, big.chunk_region(i)) }))
        .collect();
    let frame = FrameDesc { view_proj: ortho(10.0, -6.0, 20.0, 12.0), clear_color: CLEAR, reverse_z: true, draws_2d: &draws, ..Default::default() };
    let px = render(&mut gpu, 200, 120, &frame);
    write_png("two_d_tiles_big.png", &px, 200, 120);
    let s = gpu.last_frame_stats().draws;
    assert_eq!((s.tile_chunks, s.culled_2d), (1, 63), "one chunk in view");
    assert_eq!(pixel(&px, 200, 5, 5)[..3], [255, 0, 0], "cell (0, 0) is tile 1 (white corner only at its texel 0)");
    finish("tile_maps", gpu);
}

#[test]
fn vector_shapes_fill_concave_and_holed_with_soft_edges() {
    let Some(mut gpu) = gpu_or_skip("vector_shapes", AdapterPreference::HighPerformance) else { return };
    let (w, h) = (128u32, 64u32);
    // 16 px per metre.
    let mut mesh = ShapeMesh::default();
    // An L (concave) at the left, a ring (square with a round hole) at the right.
    let l = Path::polygon(&[[0.5, 0.5], [3.5, 0.5], [3.5, 1.5], [1.5, 1.5], [1.5, 3.5], [0.5, 3.5]]);
    fill(&l.flatten(0.01).unwrap(), [230, 120, 40, 255], true, &mut mesh).unwrap();
    let mut ring = Path::rect(4.5, 0.5, 3.0, 3.0);
    ring.cmds.extend(Path::circle([6.0, 2.0], 1.0).cmds);
    fill(&ring.flatten(0.005).unwrap(), [80, 200, 255, 255], true, &mut mesh).unwrap();
    let outline = StrokeStyle { width: 0.125, color: [255, 255, 255, 255], join: LineJoin::Round, cap: LineCap::Round };
    stroke(&Path::polyline(&[[0.5, 3.8], [3.5, 3.8], [3.0, 3.3]]).flatten(0.01).unwrap(), &outline, 0.01, true, &mut mesh).unwrap();
    let id = gpu.upload_mesh(&mesh.vertices, &mesh.indices).unwrap();
    let draws = [Draw2d::Shape(ShapeDraw { mesh: id, model: IDENTITY, color: [1.0; 4], layer: 0 })];
    let frame = FrameDesc { view_proj: ortho(4.0, 2.0, 8.0, 4.0), clear_color: CLEAR, reverse_z: true, draws_2d: &draws, ..Default::default() };
    let px = render(&mut gpu, w, h, &frame);
    write_png("two_d_shapes.png", &px, w, h);
    let at = |x: f32, y: f32| pixel(&px, w, (x * 16.0) as u32, ((4.0 - y) * 16.0) as u32);
    assert_eq!(at(1.0, 1.0)[..3], [230, 120, 40], "inside the L");
    assert_eq!(at(2.5, 2.5)[..3], [0, 0, 0], "the L's notch is empty");
    assert_eq!(at(5.0, 1.0)[..3], [80, 200, 255], "the ring");
    assert_eq!(at(6.0, 2.0)[..3], [0, 0, 0], "the ring's hole");
    // The L's left edge lies on a pixel boundary (x = 0.5 m = 8 px): the one-pixel fringe is
    // centred on it, so pixel 7 is empty and pixel 8 fully covered, as true coverage says.
    assert_eq!((pixel(&px, w, 7, 32)[0], pixel(&px, w, 8, 32)[0]), (0, 230));
    // A curved edge has partially covered pixels too.
    let partial = (0..w).filter(|&x| {
        let c = pixel(&px, w, x, 32);
        c[2] > 10 && c[2] < 245 && x > 64
    });
    assert!(partial.count() >= 2, "anti-aliased hole edge");
    assert_eq!(at(2.0, 3.8)[..3], [255, 255, 255], "stroke");
    finish("vector_shapes", gpu);
}

#[test]
fn many_sprites_batch_and_billboards_face_the_camera() {
    let Some(mut gpu) = gpu_or_skip("billboards", AdapterPreference::HighPerformance) else { return };
    let textures: Vec<TextureId> = (0..4).map(|k| texture(&mut gpu, 2, |_, _| [60 * k as u8, 255, 255 - 60 * k as u8, 255])).collect();
    // 1000 blended sprites, four textures: one draw call.
    let mut draws: Vec<Draw2d> = (0..1000)
        .map(|i| {
            let (x, y) = ((i % 40) as f32 * 0.2 - 4.0, (i / 40) as f32 * 0.2 - 2.5);
            Draw2d::Sprite(Sprite { position: [x, y, 0.0], size: [0.15, 0.15], texture: Some(textures[i % 4]), ..Default::default() })
        })
        .collect();
    let frame = FrameDesc { view_proj: ortho(0.0, 0.0, 10.0, 6.0), clear_color: CLEAR, reverse_z: true, draws_2d: &draws, ..Default::default() };
    render(&mut gpu, 160, 96, &frame);
    let s = gpu.last_frame_stats().draws;
    assert_eq!((s.sprites, s.draw_calls_2d, s.triangles_2d), (1000, 1, 2000), "textures are bindless: one batch");

    // Perspective: a spherical billboard straight ahead is a square on screen from any side; a
    // cylindrical one stays upright.
    draws.clear();
    let (w, h) = (160u32, 120u32);
    let eye = [6.0, 3.0, 6.0];
    let view_proj = mul(&perspective_vk_reverse_z(45f32.to_radians(), w as f32 / h as f32, 0.05), &look_at_rh(eye, [0.0, 1.0, 0.0], [0.0, 1.0, 0.0]));
    let card = Sprite { position: [0.0, 1.0, 0.0], size: [1.0, 1.0], color: [1.0, 0.8, 0.2, 1.0], billboard: Billboard::Spherical, ..Default::default() };
    draws.push(Draw2d::Sprite(card));
    let frame = FrameDesc { view_proj, clear_color: CLEAR, reverse_z: true, draws_2d: &draws, ..Default::default() };
    let px = render(&mut gpu, w, h, &frame);
    write_png("two_d_billboard.png", &px, w, h);
    let lit: Vec<(u32, u32)> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| pixel(&px, w, x, y)[0] > 128).collect();
    let (x0, x1) = (lit.iter().map(|p| p.0).min().unwrap(), lit.iter().map(|p| p.0).max().unwrap());
    let (y0, y1) = (lit.iter().map(|p| p.1).min().unwrap(), lit.iter().map(|p| p.1).max().unwrap());
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    assert!(cw.abs_diff(ch) <= 1, "facing the camera, the card is square on screen: {cw} x {ch}");
    assert_eq!(lit.len() as u32, cw * ch, "an axis-aligned rectangle (no perspective skew)");
    finish("billboards", gpu);
}

#[test]
fn cutout_sprites_cast_shadows_and_lit_sprites_receive_light() {
    let Some(mut gpu) = gpu_or_skip("sprite_shadows", AdapterPreference::HighPerformance) else { return };
    let (v, i) = cube();
    let cube_mesh = gpu.upload_mesh(&v, &i).unwrap();
    let (w, h) = (160u32, 120u32);
    let view_proj = mul(&perspective_vk_reverse_z(50f32.to_radians(), w as f32 / h as f32, 0.05), &look_at_rh([0.0, 6.0, 9.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
    let mut ground = IDENTITY;
    ground[0] = 20.0;
    ground[5] = 0.1;
    ground[10] = 20.0;
    ground[13] = -0.05;
    let mesh_draws = [DrawItem::new(cube_mesh, ground, [0.7, 0.7, 0.7, 1.0])];
    let mut env = Environment { sun_direction: [0.0, 1.0, 0.0], ..Default::default() };
    env.shadow.enabled = true;
    // A 2 m upright cutout card at the origin; with the sun overhead, a yaw-facing card casts a
    // thin shadow; lay it flat a little above the ground so the shadow is its own shape.
    let card = Sprite {
        position: [0.0, 1.0, 0.0],
        size: [2.0, 2.0],
        axes: [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0]],
        color: [0.2, 0.9, 0.3, 1.0],
        blend: SpriteBlend::Cutout { threshold: 0.5 },
        cast_shadows: true,
        lit: true,
        ..Default::default()
    };
    let draws = [Draw2d::Sprite(card)];
    let frame = FrameDesc { view_proj, clear_color: CLEAR, reverse_z: true, draws: &mesh_draws, draws_2d: &draws, environment: env, ..Default::default() };
    let px = render(&mut gpu, w, h, &frame);
    write_png("two_d_shadow.png", &px, w, h);
    let s = gpu.last_frame_stats().draws;
    assert_eq!(s.shadow_sprites, 1, "{s:?}");
    // The ground right under the card (seen past its edge) is shadowed; far away it is lit.
    let ground_px = |x: f32, z: f32| {
        let (sx, sy) = project(&view_proj, [x, 0.0, z], w, h);
        pixel(&px, w, sx as u32, sy as u32)
    };
    let (open, under) = (ground_px(3.5, 2.5), ground_px(0.5, 0.9));
    assert!(under[0] + 30 < open[0], "shadow {under:?} vs lit {open:?}");
    finish("sprite_shadows", gpu);
}
