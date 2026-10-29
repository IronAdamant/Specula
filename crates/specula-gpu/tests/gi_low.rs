//! GI tier low against high on baked scenes (`gi.rs`, `shaders/gi.glsl`), and tier low's
//! per-draw cache under moving and many draws. From the GI lane's round-4 review: tier low leaked
//! the sky into a sealed hut (its walls' single SH sat inside the walls, the ground's CPU grid
//! blended samples across them), and its cache flushed wholesale past 8192 entries while moving
//! draws added a key a frame. Skips cleanly without Vulkan. Captures go to `$SPECULA_TEST_OUT`
//! (or cargo's per-target temp dir) as `gi_low_*.png`.

mod common;

use common::*;
use specula_gpu::{AdapterPreference, DrawItem, Environment, FrameDesc, GiFrame, GiQuality, GiView, GiVolume, Gpu, MeshId, Vertex};
use specula_lux as lux;
use lux::{Control, PointLight, Request, Scene, Sky, Sun, Triangle};

const W: u32 = 256;
const H: u32 = 256;

fn solid(out: &mut Vec<Triangle>, lo: [f32; 3], hi: [f32; 3], albedo: [f32; 3]) {
    let c = |x: usize, y: usize, z: usize| [if x == 1 { hi[0] } else { lo[0] }, if y == 1 { hi[1] } else { lo[1] }, if z == 1 { hi[2] } else { lo[2] }];
    let faces: [([[usize; 3]; 4], [f32; 3]); 6] = [
        ([[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]], [1.0, 0.0, 0.0]),
        ([[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]], [-1.0, 0.0, 0.0]),
        ([[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]], [0.0, 1.0, 0.0]),
        ([[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]], [0.0, -1.0, 0.0]),
        ([[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]], [0.0, 0.0, 1.0]),
        ([[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]], [0.0, 0.0, -1.0]),
    ];
    for (q, n) in faces {
        let p: Vec<[f32; 3]> = q.iter().map(|k| c(k[0], k[1], k[2])).collect();
        out.push(Triangle { p: [p[0], p[1], p[2]], normal: n, albedo, emissive: [0.0; 3] });
        out.push(Triangle { p: [p[0], p[2], p[3]], normal: n, albedo, emissive: [0.0; 3] });
    }
}

fn box_model(lo: [f32; 3], hi: [f32; 3]) -> [f32; 16] {
    let s = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let c = [(hi[0] + lo[0]) * 0.5, (hi[1] + lo[1]) * 0.5, (hi[2] + lo[2]) * 0.5];
    [s[0], 0.0, 0.0, 0.0, 0.0, s[1], 0.0, 0.0, 0.0, 0.0, s[2], 0.0, c[0], c[1], c[2], 1.0]
}

/// A hut's walls and roof (`z` metres square, 0.2 m walls): sealed, or with a door.
fn hut(sealed: bool, z: f32) -> Vec<([f32; 3], [f32; 3])> {
    let mut b = vec![([0.0, 0.0, 0.0], [0.2, 2.4, z]), ([z - 0.2, 0.0, 0.0], [z, 2.4, z]), ([0.2, 0.0, z - 0.2], [z - 0.2, 2.4, z]), ([0.0, 2.4, 0.0], [z, 2.6, z])];
    if sealed {
        b.push(([0.2, 0.0, 0.0], [z - 0.2, 2.4, 0.2]));
    } else {
        b.push(([0.2, 0.0, 0.0], [1.1, 2.4, 0.2]));
        b.push(([2.3, 0.0, 0.0], [3.2, 2.4, 0.2]));
        b.push(([1.1, 2.0, 0.0], [2.3, 2.4, 0.2]));
    }
    b
}

/// The share of pixels more than 16/255 apart in any channel, the worst difference, and the mean.
fn compare(a: &[u8], b: &[u8]) -> (f64, i32, f64) {
    let (mut over, mut worst, mut sum, mut n) = (0usize, 0i32, 0f64, 0usize);
    for i in (0..a.len()).step_by(4) {
        let d = (0..3).map(|k| (a[i + k] as i32 - b[i + k] as i32).abs()).max().unwrap_or(0);
        over += usize::from(d > 16);
        worst = worst.max(d);
        sum += d as f64;
        n += 1;
    }
    (over as f64 / n as f64, worst, sum / n as f64)
}

struct Baked {
    records: Vec<[u32; 12]>,
    origin: [f32; 3],
    spacing: f32,
    dims: [u32; 3],
}

impl Baked {
    fn volume(&self) -> GiVolume<'_> {
        GiVolume { origin: self.origin, spacing: self.spacing, dims: self.dims, intensity: 1.0, probes: &self.records }
    }
}

fn bake(scene: &Scene, r: &Request) -> Baked {
    let baked = lux::bake(scene, r, 4, &Control::default()).expect("bake");
    let l = baked.field.lattice;
    Baked { records: baked.field.pack(), origin: l.origin, spacing: l.spacing, dims: l.dims }
}

/// The interior views of a hut scene at high and at low: (share over 16/255, worst, mean) each.
fn hut_views(gpu: &mut Gpu, cube: MeshId, name: &str, sealed: bool, merged: bool, light: bool, key: u64) -> Vec<(String, (f64, i32, f64))> {
    let (cv, ci) = common::cube();
    let mut t = Vec::new();
    solid(&mut t, [-12.0, -1.0, -12.0], [12.0, 0.0, 12.0], [0.5, 0.5, 0.5]);
    let boxes = hut(sealed, 3.4);
    for (lo, hi) in &boxes {
        solid(&mut t, *lo, *hi, [0.6, 0.5, 0.4]);
    }
    let props = [([1.0, 0.0, 1.0], [1.5, 0.5, 1.5]), ([2.4, 0.0, 2.4], [2.9, 0.6, 2.9])];
    for (lo, hi) in &props {
        solid(&mut t, *lo, *hi, [0.5, 0.5, 0.5]);
    }
    let lights = if light { vec![PointLight { position: [1.7, 1.2, 1.7], color: [2.0, 0.8, 0.2], range: 5.0 }] } else { vec![] };
    let scene = Scene { triangles: t, lights, sun: Some(Sun { direction: [0.3, 0.5, -0.8], color: [0.6, 0.7, 1.0] }), sky: Some(Sky { radiance: [0.10, 0.15, 0.30], below: 0.5 }) };
    let baked = bake(&scene, &Request { min: [-4.5, 0.0, -4.5], max: [7.0, 3.5, 7.0], spacing: 1.0, rays: 128, bounces: 2, clamp: 0.0 });
    let vol = [baked.volume()];
    let mut draws = vec![DrawItem::new(cube, box_model([-12.0, -1.0, -12.0], [12.0, 0.0, 12.0]), [1.0; 4])];
    if merged {
        let (mut v, mut ix) = (Vec::new(), Vec::new());
        for (lo, hi) in &boxes {
            let m = box_model(*lo, *hi);
            let base = v.len() as u32;
            v.extend(cv.iter().map(|vert| Vertex::new([vert.position[0] * m[0] + m[12], vert.position[1] * m[5] + m[13], vert.position[2] * m[10] + m[14]], vert.normal)));
            ix.extend(ci.iter().map(|k| k + base));
        }
        let mesh = gpu.upload_mesh(&v, &ix).expect("mesh");
        draws.push(DrawItem::new(mesh, scale_translation(1.0, [0.0; 3]), [1.0; 4]));
    } else {
        draws.extend(boxes.iter().map(|(lo, hi)| DrawItem::new(cube, box_model(*lo, *hi), [1.0; 4])));
    }
    draws.extend(props.iter().map(|(lo, hi)| DrawItem::new(cube, box_model(*lo, *hi), [1.0; 4])));
    for d in draws.iter_mut() {
        d.surface.specular = 0.0;
    }
    let target = gpu.create_target(W, H).expect("target");
    let environment = Environment { sun_intensity: 0.0, ambient_color: [1.0, 1.0, 1.0], ambient_intensity: 0.05, ..Default::default() };
    let mut out = Vec::new();
    for (cam, eye, at) in [("inside", [1.7, 1.3, 0.6], [1.7, 0.6, 3.2]), ("inside_up", [1.7, 0.6, 1.7], [1.7, 2.4, 3.0])] {
        let vp = mul(&perspective_vk_reverse_z(80f32.to_radians(), 1.0, 0.05), &look_at_rh(eye, at, [0.0, 1.0, 0.0]));
        let mut imgs = Vec::new();
        for q in [GiQuality::High, GiQuality::Low] {
            let gi = GiFrame { key, volumes: &vol, quality: q, view: GiView::Irradiance };
            gpu.render(target, &FrameDesc { view_proj: vp, draws: &draws, reverse_z: true, environment, gi, ..Default::default() }).expect("render");
            let rgba = gpu.read_rgba8(target).expect("read");
            write_png(&format!("gi_low_{name}_{cam}_{q:?}.png"), &rgba, W, H);
            imgs.push(rgba);
        }
        let c = compare(&imgs[0], &imgs[1]);
        println!("{name:14} {cam:10}: high vs low: over 16/255 {:.2}%, worst {}, mean {:.2}", c.0 * 100.0, c.1, c.2);
        out.push((cam.to_string(), c));
    }
    gpu.destroy_target(target);
    out
}

/// The review's sealed dark hut (no light inside, the sky outside): tier low must keep it as dark
/// as high does, the walls drawn one by one or merged into one wide draw. Other scenes are
/// printed for comparison.
#[test]
fn sealed_dark_hut_matches_high_at_low() {
    let test = "sealed_dark_hut_matches_high_at_low";
    let Some(mut gpu) = gpu_or_skip(test, AdapterPreference::LowPower) else { return };
    {
        let (cv, ci) = cube();
        let cube = gpu.upload_mesh(&cv, &ci).expect("cube");
        for (i, (name, sealed, merged, light, strict)) in
            [("sealed_dark", true, false, false, true), ("sealed_dark_merged", true, true, false, true), ("sealed_lit", true, false, true, false), ("open_lit", false, false, true, false)].into_iter().enumerate()
        {
            for (cam, (over, worst, mean)) in hut_views(&mut gpu, cube, name, sealed, merged, light, 100 + i as u64) {
                if strict {
                    assert!(over <= 0.01, "{name} {cam}: {:.2}% of pixels over 16/255 from high (worst {worst}, mean {mean:.2})", over * 100.0);
                }
            }
        }
    }
    let errors = validation_errors(test, gpu);
    assert!(errors.is_empty(), "{errors:?}");
}

/// A moving 30 x 6 x 30 m platform (one wide draw, a new cache key every frame) for 3000 frames:
/// the cache holds a few entries, never grows, and samples nothing for it (wide draws read the
/// probes per pixel).
#[test]
fn a_moving_wide_draw_keeps_the_cache_small() {
    let test = "a_moving_wide_draw_keeps_the_cache_small";
    let Some(mut gpu) = gpu_or_skip(test, AdapterPreference::LowPower) else { return };
    {
        let (cv, ci) = cube();
        let mesh = gpu.upload_mesh(&cv, &ci).expect("mesh");
        let ground = vec![Triangle { p: [[-40.0, 0.0, -40.0], [40.0, 0.0, 40.0], [40.0, 0.0, -40.0]], normal: [0.0, 1.0, 0.0], albedo: [0.5; 3], emissive: [0.0; 3] }];
        let scene = Scene { triangles: ground, lights: vec![], sun: Some(Sun { direction: [0.3, 0.5, -0.8], color: [0.6, 0.7, 1.0] }), sky: Some(Sky { radiance: [0.10, 0.15, 0.30], below: 0.5 }) };
        let baked = bake(&scene, &Request { min: [-30.0, 0.0, -30.0], max: [30.0, 8.0, 30.0], spacing: 1.0, rays: 16, bounces: 0, clamp: 0.0 });
        let vol = [baked.volume()];
        let target = gpu.create_target(64, 64).expect("target");
        let vp = mul(&perspective_vk_reverse_z(90f32.to_radians(), 1.0, 0.05), &look_at_rh([0.0, 60.0, 0.1], [0.0; 3], [0.0, 1.0, 0.0]));
        let environment = Environment { sun_intensity: 0.0, ambient_intensity: 0.05, ..Default::default() };
        let mut most = 0usize;
        for f in 0..3000u32 {
            let dx = f as f32 * 0.001;
            let draws = [DrawItem::new(mesh, box_model([-15.0 + dx, 0.5, -15.0], [15.0 + dx, 6.5, 15.0]), [1.0; 4])];
            let gi = GiFrame { key: 5, volumes: &vol, quality: GiQuality::Low, view: GiView::Lit };
            gpu.render(target, &FrameDesc { view_proj: vp, draws: &draws, reverse_z: true, environment, gi, ..Default::default() }).expect("render");
            let (entries, bytes, made) = gpu.gi_cache_stats();
            most = most.max(entries);
            assert!(made <= 1 && bytes <= entries * 64, "frame {f}: {entries} entries, {bytes} bytes, {made} made");
        }
        assert!(most <= 260, "the cache grew to {most} entries for one moving draw");
        gpu.destroy_target(target);
    }
    let errors = validation_errors(test, gpu);
    assert!(errors.is_empty(), "{errors:?}");
}

/// A slab, 300 static props and 50 walkers moving every frame, then 9000 static props (past the
/// old cache's 8192 entries, which then flushed every frame): after the first frame only the
/// walkers are sampled, and an unchanged frame samples nothing.
#[test]
fn moving_and_many_draws_never_flush_the_cache() {
    let test = "moving_and_many_draws_never_flush_the_cache";
    let Some(mut gpu) = gpu_or_skip(test, AdapterPreference::LowPower) else { return };
    {
        let (cv, ci) = cube();
        let mesh = gpu.upload_mesh(&cv, &ci).expect("mesh");
        let mut t = Vec::new();
        solid(&mut t, [-22.0, -4.0, -22.0], [22.0, 0.0, 22.0], [0.5; 3]);
        let scene = Scene { triangles: t, lights: vec![], sun: Some(Sun { direction: [0.3, 0.5, -0.8], color: [0.6, 0.7, 1.0] }), sky: Some(Sky { radiance: [0.10, 0.15, 0.30], below: 0.5 }) };
        let baked = bake(&scene, &Request { min: [-22.0, -1.0, -22.0], max: [22.0, 7.0, 22.0], spacing: 1.0, rays: 16, bounces: 0, clamp: 0.0 });
        let vol = [baked.volume()];
        let target = gpu.create_target(64, 64).expect("target");
        let vp = mul(&perspective_vk_reverse_z(90f32.to_radians(), 1.0, 0.05), &look_at_rh([0.0, 60.0, 0.1], [0.0; 3], [0.0, 1.0, 0.0]));
        let environment = Environment { sun_intensity: 0.0, ambient_intensity: 0.05, ..Default::default() };
        let gi = GiFrame { key: 5, volumes: &vol, quality: GiQuality::Low, view: GiView::Lit };
        let render = |gpu: &mut Gpu, draws: &[DrawItem]| {
            gpu.render(target, &FrameDesc { view_proj: vp, draws, reverse_z: true, environment, gi, ..Default::default() }).expect("render");
            gpu.gi_cache_stats()
        };
        for f in 0..300u32 {
            let mut draws = vec![DrawItem::new(mesh, box_model([-22.0, -4.0, -22.0], [22.0, 0.0, 22.0]), [1.0; 4])];
            draws.extend((0..300).map(|i| {
                let (x, z) = ((i % 20) as f32 * 2.0 - 20.0, (i / 20) as f32 * 2.0 - 15.0);
                DrawItem::new(mesh, box_model([x, 0.0, z], [x + 0.4, 0.8, z + 0.4]), [1.0; 4])
            }));
            draws.extend((0..50).map(|i| {
                let (x, z) = ((i % 10) as f32 * 3.0 - 15.0 + f as f32 * 0.01, (i / 10) as f32 * 3.0 - 7.0);
                DrawItem::new(mesh, box_model([x, 0.0, z], [x + 0.5, 1.8, z + 0.5]), [1.0; 4])
            }));
            let (entries, _, made) = render(&mut gpu, &draws);
            if f > 0 {
                assert!(made <= 50, "frame {f}: {made} draws sampled (only the 50 walkers moved)");
            }
            assert!(entries <= 351 + 50 * 3 + 256, "frame {f}: {entries} entries");
        }
        let many: Vec<DrawItem> = (0..9000)
            .map(|i| {
                let (x, z) = ((i % 100) as f32 * 0.4 - 20.0, (i / 100) as f32 * 0.4 - 18.0);
                DrawItem::new(mesh, box_model([x, 0.0, z], [x + 0.3, 0.3, z + 0.3]), [1.0; 4])
            })
            .collect();
        render(&mut gpu, &many);
        for _ in 0..3 {
            let (entries, _, made) = render(&mut gpu, &many);
            assert_eq!(made, 0, "an unchanged frame of 9000 draws samples nothing ({entries} entries)");
        }
        gpu.destroy_target(target);
    }
    let errors = validation_errors(test, gpu);
    assert!(errors.is_empty(), "{errors:?}");
}
