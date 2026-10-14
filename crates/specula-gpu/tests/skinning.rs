//! GPU skinning and textured materials on real GPUs (every adapter, under validation): a skinned
//! bar renders like its CPU-posed copy (colour and shadows, instanced with a palette each), the
//! bind pose without a palette, structured errors for bad palettes, streams and textures; textured
//! quads sample their base colour exactly, minify through sRGB-correct mips and cut alpha masks.
//! Every test skips cleanly (prints why, passes) without Vulkan.

mod common;

use common::*;
use specula_gpu::{
    AdapterPreference, DrawItem, Environment, ErrorReason, FrameDesc, Gpu, IdKind, MaterialTextures, MeshStream, MeshStreams, SkinVertex,
    Surface, TextureFilter, TextureFormat, TextureSampler, Vertex,
};

const W: u32 = 256;
const H: u32 = 256;

/// A 0.4 × 2 × 0.4 bar standing on the origin, 8 rings up: rings below y = 0.9 follow bone 0,
/// above 1.1 bone 1, between them they blend.
fn bar() -> (Vec<Vertex>, Vec<u32>, Vec<SkinVertex>) {
    let (mut vertices, mut skin, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    let corners = [[-0.2, -0.2], [0.2, -0.2], [0.2, 0.2], [-0.2, 0.2]];
    let rings = 9;
    for side in 0..4 {
        let (a, b) = (corners[side], corners[(side + 1) % 4]);
        let n = normalize([a[0] + b[0], 0.0, a[1] + b[1]]);
        let base = vertices.len() as u32;
        for r in 0..rings {
            let y = r as f32 * 2.0 / (rings - 1) as f32;
            let w1 = (((y - 0.9) / 0.2).clamp(0.0, 1.0) * 255.0).round() as u8;
            for c in [a, b] {
                vertices.push(Vertex::new([c[0], y, c[1]], n));
                skin.push(SkinVertex { joints: [0, 1, 0, 0], weights: [255 - w1, w1, 0, 0] });
            }
        }
        for r in 0..rings as u32 - 1 {
            let i = base + r * 2;
            indices.extend_from_slice(&[i, i + 1, i + 3, i, i + 3, i + 2]);
        }
    }
    (vertices, indices, skin)
}

fn rotation_z(radians: f32) -> [f32; 16] {
    let (s, c) = radians.sin_cos();
    [c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]
}

/// Bone 0 at rest; bone 1 (joint at y = 1) bent by `angle` about Z: T(0,1,0) · Rz · T(0,−1,0).
fn palette(angle: f32) -> [[f32; 16]; 2] {
    [IDENTITY, mul(&mul(&translation([0.0, 1.0, 0.0]), &rotation_z(angle)), &translation([0.0, -1.0, 0.0]))]
}

fn transform_point(m: &[f32; 16], p: V3) -> V3 {
    [0, 1, 2].map(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
}

/// The bar posed on the CPU (linear blend, as the shader does).
fn posed(vertices: &[Vertex], skin: &[SkinVertex], bones: &[[f32; 16]]) -> Vec<Vertex> {
    vertices
        .iter()
        .zip(skin)
        .map(|(v, s)| {
            let mut m = [0.0f32; 16];
            for k in 0..4 {
                let w = s.weights[k] as f32 / 255.0;
                for (o, b) in m.iter_mut().zip(&bones[s.joints[k] as usize]) {
                    *o += b * w;
                }
            }
            let p = transform_point(&m, v.position);
            let n = normalize([0, 1, 2].map(|r| m[r] * v.normal[0] + m[4 + r] * v.normal[1] + m[8 + r] * v.normal[2]));
            Vertex::new(p, n)
        })
        .collect()
}

/// Pixels (of w × h) where any channel differs by more than 3.
fn differing(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4).zip(b.chunks(4)).filter(|(p, q)| p.iter().zip(q.iter()).any(|(x, y)| x.abs_diff(*y) > 3)).count()
}

fn scene_frame<'a>(draws: &'a [DrawItem], bones: &'a [[f32; 16]], shadows: bool) -> FrameDesc<'a> {
    let view = look_at_rh([0.0, 2.0, 6.0], [0.0, 1.0, 0.0], [0.0, 1.0, 0.0]);
    let view_proj = mul(&perspective_vk_reverse_z(50f32.to_radians(), 1.0, 0.05), &view);
    let mut environment = Environment { sun_direction: [-0.3, 0.8, 0.5], ..Default::default() };
    environment.shadow.enabled = shadows;
    FrameDesc { view_proj, clear_color: [0.1, 0.1, 0.15, 1.0], draws, reverse_z: true, environment, bones, ..Default::default() }
}

fn skinned_bar_on(adapter: AdapterPreference, label: &str) {
    let test = format!("skinned_bar[{label}]");
    let Some(mut gpu) = gpu_or_skip(&test, adapter) else { return };
    eprintln!("{test}: {} (validation {})", gpu.adapter().name, gpu.validation_enabled());
    let (vertices, indices, skin) = bar();
    let mesh = gpu.upload_mesh_with(&vertices, &indices, &MeshStreams { skin: Some(&skin), ..Default::default() }).unwrap();
    let plain = gpu.upload_mesh(&vertices, &indices).unwrap();
    let (ground_v, ground_i) = cube();
    let ground = gpu.upload_mesh(&ground_v, &ground_i).unwrap();
    let target = gpu.create_target(W, H).unwrap();
    gpu.mark_warm();
    let floor = DrawItem::new(ground, mul(&translation([0.0, -0.05, 0.0]), &scale3([6.0, 0.1, 6.0])), [0.6, 0.6, 0.6, 1.0]);
    let colour = [0.9, 0.7, 0.4, 1.0];
    // Two bars: left bent one way, right the other — one instanced batch, a palette each.
    let left = mul(&translation([-1.0, 0.0, 0.0]), &IDENTITY);
    let right = translation([1.0, 0.0, 0.0]);
    let (pa, pb) = (palette(0.9), palette(-0.6));
    let bones: Vec<[f32; 16]> = pa.iter().chain(pb.iter()).copied().collect();
    let mut a = DrawItem::new(mesh, left, colour);
    a.skin = Some(0);
    let mut b = DrawItem::new(mesh, right, colour);
    b.skin = Some(2);
    for shadows in [false, true] {
        let gpu_draws = [floor, a, b];
        gpu.render(target, &scene_frame(&gpu_draws, &bones, shadows)).unwrap();
        let skinned = gpu.read_rgba8(target).unwrap();
        let stats = gpu.last_frame_stats();
        assert_eq!((stats.draws.draw_calls, stats.draws.instances), (2, 3), "{test}: the two bars are one instanced draw");
        assert_eq!(stats.pipelines.created_after_warm, 0, "{test}: skinned pipelines are prebuilt");
        // The CPU-posed copies.
        let ca = gpu.upload_mesh(&posed(&vertices, &skin, &pa), &indices).unwrap();
        let cb = gpu.upload_mesh(&posed(&vertices, &skin, &pb), &indices).unwrap();
        let cpu_draws = [floor, DrawItem::new(ca, left, colour), DrawItem::new(cb, right, colour)];
        gpu.render(target, &scene_frame(&cpu_draws, &[], shadows)).unwrap();
        let reference = gpu.read_rgba8(target).unwrap();
        gpu.destroy_mesh(ca);
        gpu.destroy_mesh(cb);
        let off = differing(&skinned, &reference);
        if off > 0 {
            write_png(&format!("skinned_bar_{label}_{shadows}_gpu"), &skinned, W, H);
            write_png(&format!("skinned_bar_{label}_{shadows}_cpu"), &reference, W, H);
        }
        assert!(off <= (W * H / 500) as usize, "{test}: shadows {shadows}: {off} pixels differ from the CPU-posed copy");
        // The bent tip really moved: the top of the left bar is left of its base.
        let vp = scene_frame(&[], &[], false).view_proj;
        let tip = transform_point(&mul(&left, &pa[1]), [0.0, 2.0, 0.0]);
        assert!(tip[0] < -1.5, "{tip:?}");
        let (x, y) = project(&vp, tip, W, H);
        let p = pixel(&skinned, W, (x as u32).min(W - 1), (y as u32 + 2).min(H - 1));
        assert!(p[0] > 60, "{test}: the posed tip at ({x:.0}, {y:.0}) is drawn: {p:?}");
    }
    // No palette: the bind pose, as the plain mesh draws it.
    let rest = [floor, DrawItem::new(mesh, left, colour)];
    gpu.render(target, &scene_frame(&rest, &[], true)).unwrap();
    let bind = gpu.read_rgba8(target).unwrap();
    let rest_plain = [floor, DrawItem::new(plain, left, colour)];
    gpu.render(target, &scene_frame(&rest_plain, &[], true)).unwrap();
    assert_eq!(differing(&bind, &gpu.read_rgba8(target).unwrap()), 0, "{test}: no palette draws the bind pose");
    let errors = validation_errors(&test, gpu);
    assert!(errors.is_empty(), "{test}: {errors:#?}");
}

fn scale3(s: V3) -> [f32; 16] {
    [s[0], 0.0, 0.0, 0.0, 0.0, s[1], 0.0, 0.0, 0.0, 0.0, s[2], 0.0, 0.0, 0.0, 0.0, 1.0]
}

#[test]
fn skinned_bar_matches_cpu_pose_on_every_adapter() {
    let Ok(adapters) = specula_gpu::enumerate_adapters() else {
        eprintln!("skinned_bar: SKIPPED — Vulkan is not available");
        return;
    };
    for a in adapters {
        skinned_bar_on(AdapterPreference::Index(a.index), &format!("{}", a.index));
    }
}

#[test]
fn bad_palettes_streams_and_textures_are_structured_errors() {
    let Some(mut gpu) = gpu_or_skip("skin_errors", AdapterPreference::HighPerformance) else { return };
    let (vertices, indices, mut skin) = bar();
    // Stream lengths.
    let short = &skin[..3];
    let e = gpu.upload_mesh_with(&vertices, &indices, &MeshStreams { skin: Some(short), ..Default::default() }).unwrap_err();
    assert_eq!(e.reason, Some(ErrorReason::BadStream { stream: MeshStream::Skin, vertex: 3 }));
    let uvs = vec![[0.0f32; 2]; 2];
    let e = gpu.upload_mesh_with(&vertices, &indices, &MeshStreams { uvs: Some(&uvs), ..Default::default() }).unwrap_err();
    assert_eq!(e.reason, Some(ErrorReason::BadStream { stream: MeshStream::Uvs, vertex: 2 }));
    skin[5].joints[1] = 255;
    skin[5].weights = [100, 155, 0, 0];
    let e = gpu.upload_mesh_with(&vertices, &indices, &MeshStreams { skin: Some(&skin), ..Default::default() }).unwrap_err();
    assert_eq!(e.reason, Some(ErrorReason::BadStream { stream: MeshStream::Skin, vertex: 5 }));
    // A palette that leaves the frame's bones: the mesh needs 2 matrices from 1, the frame has 2.
    let (_, _, skin) = bar();
    let mesh = gpu.upload_mesh_with(&vertices, &indices, &MeshStreams { skin: Some(&skin), ..Default::default() }).unwrap();
    let target = gpu.create_target(16, 16).unwrap();
    let mut d = DrawItem::new(mesh, IDENTITY, [1.0; 4]);
    d.skin = Some(1);
    let bones = palette(0.3);
    let e = gpu.render(target, &scene_frame(&[d], &bones, false)).unwrap_err();
    assert_eq!(e.reason, Some(ErrorReason::PaletteOutside { draw: 0, first: 1, joints: 2, bones: 2 }), "{e}");
    d.skin = Some(0);
    gpu.render(target, &scene_frame(&[d], &bones, false)).unwrap();
    // An unknown material texture.
    let mut t = DrawItem::new(mesh, IDENTITY, [1.0; 4]);
    t.surface.textures.normal = Some(specula_gpu::TextureId(9999));
    let e = gpu.render(target, &scene_frame(&[t], &[], false)).unwrap_err();
    assert_eq!(e.reason, Some(ErrorReason::UnknownId { kind: IdKind::Texture, id: 9999 }));
    let errors = validation_errors("skin_errors", gpu);
    assert!(errors.is_empty(), "{errors:#?}");
}

/// A unit quad on XY facing +Z, UVs with the origin at the top left (as glTF).
fn quad_uv() -> (Vec<Vertex>, Vec<u32>, Vec<[f32; 2]>) {
    let n = [0.0, 0.0, 1.0];
    let v = [[-0.5, -0.5, 0.0], [0.5, -0.5, 0.0], [0.5, 0.5, 0.0], [-0.5, 0.5, 0.0]].map(|p| Vertex::new(p, n));
    (v.to_vec(), vec![0, 1, 2, 0, 2, 3], vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]])
}

/// Unlit-looking frame: ambient 1, no sun, no tone mapping (so the output is the albedo).
fn flat_frame<'a>(draws: &'a [DrawItem], distance: f32) -> FrameDesc<'a> {
    let view = look_at_rh([0.0, 0.0, distance], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    // A 90° view: the quad spans 1/distance of the image.
    let view_proj = mul(&perspective_vk_reverse_z(90f32.to_radians(), 1.0, 0.05), &view);
    let environment = Environment { ambient_intensity: 1.0, sun_intensity: 0.0, ..Default::default() };
    FrameDesc { view_proj, clear_color: [0.0, 0.0, 1.0, 1.0], draws, reverse_z: true, environment, ..Default::default() }
}

fn textures_on(adapter: AdapterPreference, label: &str) {
    let test = format!("textured_quad[{label}]");
    let Some(mut gpu) = gpu_or_skip(&test, adapter) else { return };
    let (v, i, uv) = quad_uv();
    let quad = gpu.upload_mesh_with(&v, &i, &MeshStreams { uvs: Some(&uv), ..Default::default() }).unwrap();
    // 2 × 2: red, green / blue, white (top row first); alpha 0 on the blue texel.
    let px = [200, 30, 30, 255, 30, 200, 30, 255, 30, 30, 200, 0, 255, 255, 255, 255];
    let nearest = TextureSampler { mag: TextureFilter::Nearest, min: TextureFilter::Nearest, mipmap: TextureFilter::Nearest, ..Default::default() };
    let tex = gpu.upload_material_texture(2, 2, TextureFormat::Rgba8Srgb, &px, nearest).unwrap();
    let target = gpu.create_target(W, H).unwrap();
    let mut d = DrawItem::new(quad, IDENTITY, [1.0; 4]);
    d.surface = Surface { textures: MaterialTextures { base_color: Some(tex), ..Default::default() }, ..Default::default() };
    // The quad fills the middle half of the image.
    gpu.render(target, &flat_frame(&[d], 1.0)).unwrap();
    let rgba = gpu.read_rgba8(target).unwrap();
    write_png(&format!("textured_quad_{label}"), &rgba, W, H);
    let at = |x: u32, y: u32| pixel(&rgba, W, x, y);
    let close = |p: [u8; 4], want: [u8; 3]| p.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 2);
    for ((x, y), want) in [((90, 90), [200, 30, 30]), ((166, 90), [30, 200, 30]), ((90, 166), [30, 30, 200]), ((166, 166), [255, 255, 255])] {
        assert!(close(at(x, y), want), "{test}: texel at ({x}, {y}): {:?}, want {want:?}", at(x, y));
    }
    // Alpha masking cuts the blue texel: the clear colour shows.
    d.surface.textures.alpha_cutoff = 0.5;
    gpu.render(target, &flat_frame(&[d], 1.0)).unwrap();
    let rgba = gpu.read_rgba8(target).unwrap();
    assert!(close(pixel(&rgba, W, 90, 166), [0, 0, 255]), "{test}: masked texel {:?}", pixel(&rgba, W, 90, 166));
    assert!(close(pixel(&rgba, W, 90, 90), [200, 30, 30]));

    // Minified far below one texel per pixel: a 256² one-texel black/white checker averages to
    // grey through the mip chain, in linear light (sRGB 188, not 128).
    let mut checker = Vec::with_capacity(256 * 256 * 4);
    for y in 0..256 {
        for x in 0..256 {
            let c = if (x + y) % 2 == 0 { 0 } else { 255 };
            checker.extend_from_slice(&[c, c, c, 255]);
        }
    }
    let tex = gpu.upload_material_texture(256, 256, TextureFormat::Rgba8Srgb, &checker, TextureSampler::default()).unwrap();
    d.surface.textures = MaterialTextures { base_color: Some(tex), ..Default::default() };
    gpu.render(target, &flat_frame(&[d], 32.0)).unwrap();
    let rgba = gpu.read_rgba8(target).unwrap();
    let c = pixel(&rgba, W, W / 2, H / 2);
    assert!(c[..3].iter().all(|&v| v.abs_diff(188) <= 4), "{test}: minified checker {c:?}, want ~188");
    let errors = validation_errors(&test, gpu);
    assert!(errors.is_empty(), "{test}: {errors:#?}");
}

#[test]
fn textured_quads_sample_mips_and_masks_on_every_adapter() {
    let Ok(adapters) = specula_gpu::enumerate_adapters() else {
        eprintln!("textured_quad: SKIPPED — Vulkan is not available");
        return;
    };
    for a in adapters {
        textures_on(AdapterPreference::Index(a.index), &format!("{}", a.index));
    }
}

/// Keep `Gpu` in the imports used when every test skips.
#[allow(dead_code)]
fn _uses(_: &Gpu) {}
