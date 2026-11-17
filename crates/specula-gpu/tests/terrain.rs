//! Terrain patches and water on real GPUs (src/terrain.rs, src/water.rs): a CDLOD terrain draws
//! its splat layers lit and shadowed, the ID buffer sees it as one opaque id, water draws in its
//! own pass after the opaque scene (deep where the floor is far, see-through where shallow),
//! captures are reproducible, frames without either keep their passes, and nothing raises a
//! validation message. Skips cleanly without Vulkan. Captures go to `$SPECULA_TEST_OUT` as
//! `terrain_*.png`.

mod common;

use common::*;
use specula_gpu::terrain::{self, HeightTree, Selection};
use specula_gpu::{
    AdapterPreference, DrawItem, Environment, FrameDesc, GiFrame, GiQuality, GiView, GiVolume, Gpu, Material, MaterialExt, Surface, TerrainLayer, TerrainMaterial,
    TextureFormat, TextureSampler, TextureWrap, Vertex, WaterStyle,
};

const W: u32 = 320;
const H: u32 = 200;
const SIDE: u32 = 129;

fn every_adapter(test: &str, f: impl Fn(&str, AdapterPreference)) {
    if !specula_gpu::is_available() {
        eprintln!("{test}: SKIPPED — Vulkan is not available");
        return;
    }
    for a in specula_gpu::enumerate_adapters().unwrap() {
        f(&format!("{test}[{}]", a.index), AdapterPreference::Index(a.index));
    }
}

/// Hills around a basin 3 m deep at the centre (128 m square, 1 m cells).
fn heights() -> Vec<f32> {
    let mut h = Vec::new();
    for j in 0..SIDE {
        for i in 0..SIDE {
            let (x, z) = (i as f32 - 64.0, j as f32 - 64.0);
            let r = (x * x + z * z).sqrt();
            let basin = -3.0 * (1.0 - (r / 18.0).min(1.0)).powi(2);
            let hills = 2.5 * (x * 0.11).sin() * (z * 0.09).cos() + 6.0 * ((r - 40.0) / 20.0).max(0.0).powi(2);
            h.push(basin + hills * (r / 30.0).min(1.0));
        }
    }
    h
}

struct Terrain {
    material: TerrainMaterial,
    tree: HeightTree,
}

fn upload_terrain(gpu: &mut Gpu) -> Terrain {
    let h = heights();
    let cell = [1.0, 1.0];
    let clamp = TextureSampler { wrap_u: TextureWrap::ClampToEdge, wrap_v: TextureWrap::ClampToEdge, anisotropy: false, ..Default::default() };
    let height = gpu.upload_texture(SIDE, SIDE, TextureFormat::Rgba8Unorm, &terrain::height_texels(&h)).unwrap();
    let normals = terrain::normal_texels(&h, SIDE, SIDE, cell);
    let levels = specula_gpu::generate_mips(SIDE, SIDE, TextureFormat::Rgba8Unorm, &normals).unwrap();
    let refs: Vec<&[u8]> = levels.iter().map(|l| l.as_slice()).collect();
    let normals = gpu.queue_texture_levels(SIDE, SIDE, TextureFormat::Rgba8Unorm, &refs, clamp).unwrap();
    // Grass on the flat, rock on slopes (from the normals' y).
    let n = terrain::normal_texels(&h, SIDE, SIDE, cell);
    let splat: Vec<u8> = n.chunks(4).flat_map(|t| {
        let up = t[1] as f32 / 255.0 * 2.0 - 1.0;
        let rock = ((0.97 - up) * 12.0).clamp(0.0, 1.0);
        let r = (rock * 255.0).round() as u8;
        [255 - r, r, 0, 0]
    }).collect();
    let levels = specula_gpu::generate_mips(SIDE, SIDE, TextureFormat::Rgba8Unorm, &splat).unwrap();
    let refs: Vec<&[u8]> = levels.iter().map(|l| l.as_slice()).collect();
    let splat = gpu.queue_texture_levels(SIDE, SIDE, TextureFormat::Rgba8Unorm, &refs, clamp).unwrap();
    let mut layers = [TerrainLayer::default(); 4];
    layers[0] = TerrainLayer { tint: [0.30, 0.45, 0.20], ..Default::default() };
    layers[1] = TerrainLayer { tint: [0.45, 0.43, 0.42], roughness: 0.8, ..Default::default() };
    Terrain {
        material: TerrainMaterial { columns: SIDE, rows: SIDE, cell, heights: height, normals, splat, layers, layer_count: 2, grain: 0.5 },
        tree: HeightTree::new(&h, SIDE, SIDE).unwrap(),
    }
}

fn env() -> Environment {
    let mut env = Environment { tonemap: true, sun_direction: [0.4, 0.8, 0.3], ..Default::default() };
    env.shadow.enabled = true;
    env.bloom.enabled = true;
    env.ssao.enabled = true;
    env
}

struct Shot {
    rgba: Vec<u8>,
    passes: Vec<String>,
    stats: specula_gpu::FrameStats,
}

fn eye() -> V3 {
    [-30.0, 22.0, 45.0]
}

fn view_proj() -> [f32; 16] {
    let view = look_at_rh(eye(), [0.0, -1.0, 0.0], [0.0, 1.0, 0.0]);
    mul(&perspective_vk_reverse_z(60f32.to_radians(), W as f32 / H as f32, 0.05), &view)
}

fn draws(t: &Terrain, quad: specula_gpu::MeshId, terrain_on: bool, water: Option<WaterStyle>) -> Vec<DrawItem> {
    let mut out = Vec::new();
    if terrain_on {
        let r = terrain::ranges(0.0, 1.0);
        let s = Selection { tree: &t.tree, cell: [1.0, 1.0], world: &IDENTITY, ranges: &r, level_offset: 0, terrain: 0, skirt: 0.5, skirt_mask: 15 };
        let mut sel = Vec::new();
        terrain::select(&s, eye(), &mut sel);
        assert!(sel.len() > 4, "several patches: {}", sel.len());
        out.extend(sel.iter().map(|p| terrain::patch_draw(&IDENTITY, p)));
    }
    if let Some(style) = water {
        // A 40 m pond at y = 0.2 over the basin.
        let mut d = DrawItem::new(quad, scale_translation(1.0, [0.0, 0.2, 0.0]), [1.0; 4]);
        d.model[0] = 40.0;
        d.model[10] = 40.0;
        d.material = Material::Water;
        d.surface = Surface { ext: MaterialExt::Water(style), ..Surface::default() };
        out.push(d);
    }
    out
}

fn shoot(gpu: &mut Gpu, t: &Terrain, quad: specula_gpu::MeshId, terrain_on: bool, water: Option<WaterStyle>, ids: bool, name: &str) -> Shot {
    let target = gpu.create_target(W, H).unwrap();
    let draws = draws(t, quad, terrain_on, water);
    let terrains = [t.material];
    let frame = FrameDesc {
        view_proj: view_proj(),
        clear_color: [0.05, 0.07, 0.12, 1.0],
        draws: &draws,
        reverse_z: true,
        environment: env(),
        time_s: 3.25,
        terrains: if terrain_on { &terrains } else { &[] },
        ..Default::default()
    };
    if ids {
        // Terrain patches are id 1, the water id 2.
        let list: Vec<u32> = draws.iter().map(|d| if d.material == Material::Water { 2 } else { 1 }).collect();
        gpu.render_with_ids(target, &frame, &list).unwrap();
    } else {
        gpu.render(target, &frame).unwrap();
    }
    let rgba = gpu.read_rgba8(target).unwrap();
    let passes = gpu.last_graph().map(|g| g.passes.iter().map(|p| p.name.to_string()).collect()).unwrap_or_default();
    let stats = gpu.last_frame_stats();
    if ids {
        let c = gpu.id_coverage(target, 2).unwrap();
        assert!(c.per_id[0].opaque_pixels > (W * H / 4) as u64, "terrain covers much of the view: {:?}", c.per_id[0]);
        if water.is_some() && c.blended_counted {
            assert!(c.per_id[1].blended_pixels > 200, "the pond is counted (blended): {:?}", c.per_id[1]);
            assert_eq!(c.per_id[1].opaque_pixels, 0, "water never hides anything");
        }
    }
    gpu.destroy_target(target);
    let label: String = gpu.adapter().name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    write_png(&format!("terrain_{name}_{label}.png"), &rgba, W, H);
    Shot { rgba, passes, stats }
}

fn px(rgba: &[u8], p: V3) -> [u8; 4] {
    let (x, y) = project(&view_proj(), p, W, H);
    pixel(rgba, W, x as u32, y as u32)
}

/// Before the terrain variant (water's pipeline) is built, a water draw is drawn by nothing, so
/// the ID buffer must not count it either: a sight packet never reports what isn't on screen
/// (review of a89b5eb: `/lake` reported 426,527 visible pixels with no water drawn).
#[test]
fn water_is_counted_only_when_drawn() {
    every_adapter("terrain_water_ids", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        assert!(!gpu.pipeline_variants().terrain, "{test}: a new device has no terrain variant yet");
        let n = [0.0, 1.0, 0.0];
        let quad_v = [[-0.5, 0.0, 0.5], [0.5, 0.0, 0.5], [0.5, 0.0, -0.5], [-0.5, 0.0, -0.5]].map(|p| Vertex::new(p, n));
        let quad = gpu.upload_mesh(&quad_v, &[0, 1, 2, 0, 2, 3]).unwrap();
        // A ground quad (id 1) under a pond (id 2).
        let mut ground = DrawItem::new(quad, scale_translation(1.0, [0.0, -1.0, 0.0]), [0.3, 0.5, 0.3, 1.0]);
        ground.model[0] = 200.0;
        ground.model[10] = 200.0;
        let mut pond = DrawItem::new(quad, scale_translation(1.0, [0.0, 0.2, 0.0]), [1.0; 4]);
        pond.model[0] = 40.0;
        pond.model[10] = 40.0;
        pond.material = Material::Water;
        pond.surface = Surface { ext: MaterialExt::Water(WaterStyle::preset("lake").unwrap()), ..Surface::default() };
        let shot = |gpu: &mut Gpu, draws: &[DrawItem]| {
            let target = gpu.create_target(W, H).unwrap();
            let frame = FrameDesc { view_proj: view_proj(), clear_color: [0.05, 0.07, 0.12, 1.0], draws, reverse_z: true, environment: env(), time_s: 3.25, ..Default::default() };
            let ids: Vec<u32> = draws.iter().map(|d| if d.material == Material::Water { 2 } else { 1 }).collect();
            gpu.render_with_ids(target, &frame, &ids).unwrap();
            let rgba = gpu.read_rgba8(target).unwrap();
            let c = gpu.id_coverage(target, 2).unwrap();
            gpu.destroy_target(target);
            (rgba, c)
        };
        let (dry, _) = shot(&mut gpu, &[ground]);
        let (wet, c) = shot(&mut gpu, &[ground, pond]);
        assert_eq!(dry, wet, "{test}: without its pipeline the water draws nothing");
        assert_eq!((c.per_id[1].opaque_pixels, c.per_id[1].blended_pixels), (0, 0), "{test}: and nothing counts it: {:?}", c.per_id[1]);
        // With the pipeline: drawn, and counted.
        gpu.prepare_terrain_pipelines().unwrap();
        let (drawn, c) = shot(&mut gpu, &[ground, pond]);
        assert_ne!(drawn, dry, "{test}: the pond draws");
        if c.blended_counted {
            assert!(c.per_id[1].blended_pixels > 200, "{test}: and is counted: {:?}", c.per_id[1]);
        }
        assert!(validation_errors(test, gpu).is_empty(), "{test}: validation errors");
    });
}

/// The ground takes the form views as meshes do (`FormView`): in `normals` every pixel of it is
/// a facing as colour (its blue, world z, is never the only channel, and where the ground is
/// level green leads), whatever its layers' colours, and in `curvature` its flats are mid grey.
#[test]
fn terrain_is_shape_alone_in_a_form_view() {
    every_adapter("terrain_form", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        gpu.prepare_terrain_pipelines().unwrap();
        let t = upload_terrain(&mut gpu);
        let n = [0.0, 1.0, 0.0];
        let quad_v = [[-0.5, 0.0, 0.5], [0.5, 0.0, 0.5], [0.5, 0.0, -0.5], [-0.5, 0.0, -0.5]].map(|p| Vertex::new(p, n));
        let quad = gpu.upload_mesh(&quad_v, &[0, 1, 2, 0, 2, 3]).unwrap();
        let draws = draws(&t, quad, true, None);
        let terrains = [t.material];
        let mut shoot = |form: specula_gpu::FormView| {
            let target = gpu.create_target(W, H).unwrap();
            // (The plain frame a form view is drawn in: no tone curve, no post, no sky.)
            let plain = Environment { tonemap: false, ..Default::default() };
            let frame = FrameDesc {
                view_proj: view_proj(),
                clear_color: [0.0, 0.0, 0.0, 1.0],
                draws: &draws,
                reverse_z: true,
                environment: if form == specula_gpu::FormView::World { env() } else { plain },
                terrains: &terrains,
                form,
                ..Default::default()
            };
            gpu.render(target, &frame).unwrap();
            let rgba = gpu.read_rgba8(target).unwrap();
            gpu.destroy_target(target);
            rgba
        };
        let world = shoot(specula_gpu::FormView::World);
        let normals = shoot(specula_gpu::FormView::Normals);
        write_png(&format!("{test}_normals.png"), &normals, W, H);
        assert!(normals != world, "{test}: a form view of the ground is not its world's frame");
        // Every drawn pixel is a unit facing as colour: (2c - 1) has length 1, give or take the
        // bytes' rounding; and the ground faces up more than any other way over most of it.
        let (mut drawn, mut unit, mut up) = (0, 0, 0);
        for p in normals.as_chunks::<4>().0 {
            if p[..3] == [0, 0, 0] {
                continue;
            }
            drawn += 1;
            let v = [0, 1, 2].map(|k| f32::from(p[k]) / 255.0 * 2.0 - 1.0);
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            unit += usize::from((len - 1.0).abs() < 0.03);
            up += usize::from(v[1] > v[0].abs() && v[1] > v[2].abs());
        }
        assert!(drawn > (W * H / 4) as usize && unit * 100 >= drawn * 99 && up * 2 > drawn, "{test}: {drawn} drawn, {unit} unit facings, {up} facing up");
        let curvature = shoot(specula_gpu::FormView::Curvature);
        write_png(&format!("{test}_curvature.png"), &curvature, W, H);
        let grey = curvature.as_chunks::<4>().0.iter().filter(|p| p[..3] != [0, 0, 0]).filter(|p| p[0].abs_diff(p[1]) <= 1 && p[1].abs_diff(p[2]) <= 1).count();
        assert!(grey == drawn, "{test}: curvature is grey on every drawn pixel ({grey} of {drawn})");
        let errors = validation_errors(test, gpu);
        assert!(errors.is_empty(), "{test}: {} validation error(s)", errors.len());
    });
}

#[test]
fn terrain_and_water_draw_lit_counted_and_reproducibly() {
    every_adapter("terrain", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        gpu.prepare_terrain_pipelines().unwrap();
        assert!(gpu.pipeline_variants().terrain, "{test}: terrain pipelines built");
        let t = upload_terrain(&mut gpu);
        let n = [0.0, 1.0, 0.0];
        let quad_v = [[-0.5, 0.0, 0.5], [0.5, 0.0, 0.5], [0.5, 0.0, -0.5], [-0.5, 0.0, -0.5]].map(|p| Vertex::new(p, n));
        let quad = gpu.upload_mesh(&quad_v, &[0, 1, 2, 0, 2, 3]).unwrap();

        // Nothing: no terrain or water passes.
        let none = shoot(&mut gpu, &t, quad, false, None, false, "none");
        assert!(!none.passes.iter().any(|p| p == "water"), "{test}: {:?}", none.passes);
        assert_eq!(none.stats.draws.terrain_patches, 0);

        let dry = shoot(&mut gpu, &t, quad, true, None, false, "dry");
        assert!(dry.stats.draws.terrain_patches > 4, "{test}: {:?}", dry.stats.draws);
        assert!(!dry.passes.iter().any(|p| p == "water"), "{test}: {:?}", dry.passes);
        // The hills are drawn (not the clear colour), shaded, and their shadows exist.
        let hill = px(&dry.rgba, [30.0, 3.0, -20.0]);
        assert!(hill[1] > 25 && hill[1] > hill[2], "{test}: green hills {hill:?}");
        assert!(dry.stats.draws.shadow_instances > 0, "{test}: terrain casts shadows {:?}", dry.stats.draws);

        let lake = WaterStyle::preset("lake").unwrap();
        let wet = shoot(&mut gpu, &t, quad, true, Some(lake), true, "lake");
        assert!(wet.passes.iter().any(|p| p == "water"), "{test}: {:?}", wet.passes);
        assert_eq!(wet.stats.draws.water_draws, 1, "{test}");
        // The pond's middle (3 m deep) is darker and bluer than the dry basin floor there.
        let (d, w) = (px(&dry.rgba, [0.0, -2.5, 0.0]), px(&wet.rgba, [0.0, 0.2, 0.0]));
        assert!(w[2] as i32 >= w[0] as i32 && (w[1] as i32 - d[1] as i32) < 0, "{test}: deep water {d:?} -> {w:?}");
        // Outside the pond nothing changes.
        let (d, w) = (px(&dry.rgba, [30.0, 3.0, -20.0]), px(&wet.rgba, [30.0, 3.0, -20.0]));
        assert!((d[0] as i32 - w[0] as i32).abs() <= 8 && (d[1] as i32 - w[1] as i32).abs() <= 8, "{test}: {d:?} vs {w:?}");

        // Reproducible: the same frame, the same bytes (waves are a function of time_s).
        let again = shoot(&mut gpu, &t, quad, true, Some(lake), false, "lake_again");
        assert_eq!(again.rgba, wet.rgba, "{test}: captures are reproducible");

        // Every preset draws (lava is opaque and glows).
        for name in specula_gpu::WATER_PRESETS {
            let s = shoot(&mut gpu, &t, quad, true, Some(WaterStyle::preset(name).unwrap()), false, name);
            assert_eq!(s.stats.draws.water_draws, 1, "{test} {name}");
            if name == "lava" {
                // Dark crust with glowing veins: some of the pond glows, most of it is crust.
                let pts: Vec<[u8; 4]> = (0..81).map(|k| px(&s.rgba, [(k % 9) as f32 * 2.0 - 8.0, 0.2, (k / 9) as f32 * 2.0 - 8.0])).collect();
                let glowing = pts.iter().filter(|p| p[0] > 150 && p[0] as i32 > p[2] as i32 + 80).count();
                let crust = pts.iter().filter(|p| p[0] < 120).count();
                assert!(glowing >= 2 && crust > glowing, "{test}: lava glows in veins ({glowing} glowing, {crust} crust of 81)");
            }
        }
        assert!(validation_errors(test, gpu).is_empty(), "{test}: validation errors");
    });
}

/// Timing report: a screen-filling terrain at 1600x900 in a Sepulcra-like HDR frame (shadows,
/// bloom, FXAA), median GPU ms over 40 frames: a lit ground quad (the baseline), the terrain
/// untextured without grain, with grain, with a textured triplanar layer, and with a lake; then
/// the terrain and the lake under a baked GI volume (2 m spacing over the whole terrain) at tiers
/// low and high.
/// `cargo test --release -p specula-gpu --test terrain terrain_cost -- --ignored --nocapture`.
#[test]
#[ignore = "timing report"]
fn terrain_cost_report() {
    every_adapter("terrain_cost", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        gpu.prepare_terrain_pipelines().unwrap();
        let t = upload_terrain(&mut gpu);
        let (w, h) = (1600u32, 900u32);
        let n = [0.0, 1.0, 0.0];
        let quad_v = [[-0.5, 0.0, 0.5], [0.5, 0.0, 0.5], [0.5, 0.0, -0.5], [-0.5, 0.0, -0.5]].map(|p| Vertex::new(p, n));
        let quad = gpu.upload_mesh(&quad_v, &[0, 1, 2, 0, 2, 3]).unwrap();
        // A textured layer: a 256 px noise texture with mips.
        let px: Vec<u8> = (0..256 * 256).flat_map(|k| {
            let v = ((k * 2654435761u64 as usize) >> 24) as u8;
            [v / 2 + 64, v / 2 + 60, v / 2 + 56, 255]
        }).collect();
        let levels = specula_gpu::generate_mips(256, 256, TextureFormat::Rgba8Srgb, &px).unwrap();
        let refs: Vec<&[u8]> = levels.iter().map(|l| l.as_slice()).collect();
        let rock = gpu.queue_texture_levels(256, 256, TextureFormat::Rgba8Srgb, &refs, TextureSampler::default()).unwrap();
        let view = look_at_rh([-20.0, 12.0, 30.0], [10.0, 0.0, -10.0], [0.0, 1.0, 0.0]);
        let view_proj = mul(&perspective_vk_reverse_z(60f32.to_radians(), w as f32 / h as f32, 0.05), &view);
        let r = terrain::ranges(0.0, 1.0);
        let s = Selection { tree: &t.tree, cell: [1.0, 1.0], world: &IDENTITY, ranges: &r, level_offset: 0, terrain: 0, skirt: 0.5, skirt_mask: 15 };
        let mut sel = Vec::new();
        terrain::select(&s, [-20.0, 12.0, 30.0], &mut sel);
        let patches: Vec<DrawItem> = sel.iter().map(|p| terrain::patch_draw(&IDENTITY, p)).collect();
        let mut env = env();
        env.fxaa = true;
        env.ssao.enabled = false;
        let target = gpu.create_target(w, h).unwrap();
        let mut ground = DrawItem::new(quad, IDENTITY, [0.4, 0.5, 0.3, 1.0]);
        ground.model[0] = 400.0;
        ground.model[10] = 400.0;
        let plain = TerrainMaterial { grain: 0.0, ..t.material };
        let grain = t.material;
        let mut textured = t.material;
        textured.layers[1].albedo = Some(rock);
        textured.layers[0].albedo = Some(rock);
        let lake = {
            let mut d = DrawItem::new(quad, scale_translation(1.0, [0.0, 0.2, 0.0]), [1.0; 4]);
            d.model[0] = 60.0;
            d.model[10] = 60.0;
            d.material = Material::Water;
            d.surface = Surface { ext: MaterialExt::Water(WaterStyle::preset("lake").unwrap()), ..Surface::default() };
            d
        };
        let mut with_lake = patches.clone();
        with_lake.push(lake);
        let mut flat = env;
        flat.shadow.enabled = false;
        let h2 = |a: f32, b: f32| u32::from(specula_gpu::f16_bits(a)) | (u32::from(specula_gpu::f16_bits(b)) << 16);
        let probe = [h2(0.1, 0.12), h2(0.15, 0.0), 0, 0, 0, 0, h2(0.0, 0.0), h2(0.0, 1.0), u32::MAX, u32::MAX, u32::MAX, 0x00ff_ffff];
        let probes = vec![probe; 65 * 9 * 65];
        let volumes = [GiVolume { origin: [-64.0, -6.0, -64.0], spacing: 2.0, dims: [65, 9, 65], intensity: 1.0, probes: &probes }];
        let gi = |q: GiQuality| GiFrame { key: 77, volumes: &volumes, quality: q, view: GiView::Lit };
        type Case<'a> = (&'a str, &'a [DrawItem], Option<TerrainMaterial>, Environment, GiFrame<'a>);
        let cases: [Case; 11] = [
            ("lit ground quad", std::slice::from_ref(&ground), None, env, GiFrame::default()),
            ("lit ground quad, no shadows", std::slice::from_ref(&ground), None, flat, GiFrame::default()),
            ("terrain plain", &patches, Some(plain), env, GiFrame::default()),
            ("terrain plain, no shadows", &patches, Some(plain), flat, GiFrame::default()),
            ("terrain grain", &patches, Some(grain), env, GiFrame::default()),
            ("terrain textured", &patches, Some(textured), env, GiFrame::default()),
            ("terrain + lake", &with_lake, Some(grain), env, GiFrame::default()),
            ("terrain grain, GI low", &patches, Some(grain), env, gi(GiQuality::Low)),
            ("terrain grain, GI high", &patches, Some(grain), env, gi(GiQuality::High)),
            ("terrain + lake, GI low", &with_lake, Some(grain), env, gi(GiQuality::Low)),
            ("terrain + lake, GI high", &with_lake, Some(grain), env, gi(GiQuality::High)),
        ];
        for (label, draws, material, environment, gi) in cases {
            let mats: Vec<TerrainMaterial> = material.into_iter().collect();
            let frame = FrameDesc { view_proj, clear_color: [0.05, 0.07, 0.12, 1.0], draws, reverse_z: true, environment, terrains: &mats, time_s: 1.0, gi, ..Default::default() };
            // The GI pipelines first (a timing report measures frames, not building them).
            gpu.render(target, &frame).unwrap();
            let mut totals = Vec::new();
            let mut passes: std::collections::BTreeMap<&str, Vec<f64>> = Default::default();
            for _ in 0..40 {
                gpu.render(target, &frame).unwrap();
                if let Some(g) = gpu.last_frame_stats().gpu {
                    totals.push(g.total_ms);
                    for p in g.passes {
                        passes.entry(p.name).or_default().push(p.gpu_ms);
                    }
                }
            }
            let median = |v: &mut Vec<f64>| {
                v.sort_by(f64::total_cmp);
                v.get(v.len() / 2).copied().unwrap_or(0.0)
            };
            let per: Vec<String> = passes.iter_mut().map(|(n, v)| format!("{n} {:.3}", median(v))).collect();
            eprintln!("{} {label}: total {:.3} ms | {}", gpu.adapter().name, median(&mut totals), per.join(", "));
        }
        gpu.destroy_target(target);
        assert!(validation_errors(test, gpu).is_empty(), "{test}: validation errors");
    });
}

#[test]
fn bad_terrain_frames_are_structured_errors() {
    every_adapter("terrain_errors", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let t = upload_terrain(&mut gpu);
        let target = gpu.create_target(16, 16).unwrap();
        let r = terrain::ranges(0.0, 1.0);
        let s = Selection { tree: &t.tree, cell: [1.0, 1.0], world: &IDENTITY, ranges: &r, level_offset: 0, terrain: 3, skirt: 0.0, skirt_mask: 0 };
        let mut sel = Vec::new();
        terrain::select(&s, eye(), &mut sel);
        let draws: Vec<DrawItem> = sel.iter().map(|p| terrain::patch_draw(&IDENTITY, p)).collect();
        let terrains = [t.material];
        let frame = FrameDesc { view_proj: view_proj(), draws: &draws, reverse_z: true, terrains: &terrains, ..Default::default() };
        let e = gpu.render(target, &frame).unwrap_err();
        assert!(matches!(e.reason, Some(specula_gpu::ErrorReason::TerrainOutside { terrain: 3, terrains: 1, .. })), "{test}: {e:?}");
        let bad = TerrainMaterial { columns: 1, ..t.material };
        let frame = FrameDesc { view_proj: view_proj(), reverse_z: true, terrains: std::slice::from_ref(&bad), ..Default::default() };
        assert!(gpu.render(target, &frame).is_err(), "{test}: a 1-column terrain is refused");
        gpu.destroy_target(target);
        assert!(validation_errors(test, gpu).is_empty(), "{test}: validation errors");
    });
}

/// Terrain and water take the baked GI at both tiers (lit_common.glsl's `lit_ambient`: the full
/// sampler at high, the lean probes at low): under a volume of uniform green probes the hills and
/// the pond turn green against the frame without GI, and the tiers agree (uniform probes: the
/// lean sampler's missing reaches and backface weights change nothing).
#[test]
fn terrain_and_water_take_the_baked_gi_at_both_tiers() {
    every_adapter("terrain_gi", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        gpu.prepare_terrain_pipelines().unwrap();
        let t = upload_terrain(&mut gpu);
        let n = [0.0, 1.0, 0.0];
        let quad_v = [[-0.5, 0.0, 0.5], [0.5, 0.0, 0.5], [0.5, 0.0, -0.5], [-0.5, 0.0, -0.5]].map(|p| Vertex::new(p, n));
        let quad = gpu.upload_mesh(&quad_v, &[0, 1, 2, 0, 2, 3]).unwrap();
        let h = |a: f32, b: f32| u32::from(specula_gpu::f16_bits(a)) | (u32::from(specula_gpu::f16_bits(b)) << 16);
        // Green irradiance 2.0, every probe valid and visible, nothing in reach.
        let probe = [h(0.0, 2.0), h(0.0, 0.0), 0, 0, 0, 0, h(0.0, 0.0), h(0.0, 1.0), u32::MAX, u32::MAX, u32::MAX, 0x00ff_ffff];
        let probes = vec![probe; 17 * 5 * 17];
        let volumes = [GiVolume { origin: [-64.0, -16.0, -64.0], spacing: 8.0, dims: [17, 5, 17], intensity: 1.0, probes: &probes }];
        let draws = draws(&t, quad, true, Some(WaterStyle::preset("lake").unwrap()));
        let terrains = [t.material];
        let target = gpu.create_target(W, H).unwrap();
        let mut shots = Vec::new();
        for (name, quality) in [("none", None), ("high", Some(GiQuality::High)), ("low", Some(GiQuality::Low))] {
            let gi = quality.map_or(GiFrame::default(), |q| GiFrame { key: 31, volumes: &volumes, quality: q, view: GiView::Lit });
            let frame = FrameDesc {
                view_proj: view_proj(),
                clear_color: [0.05, 0.07, 0.12, 1.0],
                draws: &draws,
                reverse_z: true,
                environment: env(),
                time_s: 3.25,
                terrains: &terrains,
                gi,
                ..Default::default()
            };
            gpu.render(target, &frame).unwrap();
            assert_eq!(gpu.gi_drawn(), quality.is_some(), "{test} {name}: GI drawn");
            let rgba = gpu.read_rgba8(target).unwrap();
            write_png(&format!("terrain_gi_{name}.png"), &rgba, W, H);
            shots.push(rgba);
        }
        for (what, p) in [("hill", [30.0, 3.0, -20.0]), ("pond", [0.0, 0.2, 0.0])] {
            let [none, high, low] = [0, 1, 2].map(|i| px(&shots[i], p));
            assert!(high[1] as i32 > none[1] as i32 + 20, "{test} {what}: green with GI at high: {none:?} -> {high:?}");
            assert!((0..3).all(|c| (low[c] as i32 - high[c] as i32).abs() <= 6), "{test} {what}: low {low:?} as high {high:?}");
        }
        gpu.destroy_target(target);
        assert!(validation_errors(test, gpu).is_empty(), "{test}: validation errors");
    });
}
