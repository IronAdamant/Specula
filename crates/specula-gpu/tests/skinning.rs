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
    // Posed bounds (the bar reaches at most 2 m from its base), as the engine gives them.
    let mut a = DrawItem::new(mesh, left, colour);
    a.skin = Some(0);
    a.bounds = Some([-1.0, 1.0, 0.0, 1.5]);
    let mut b = DrawItem::new(mesh, right, colour);
    b.skin = Some(2);
    b.bounds = Some([1.0, 1.0, 0.0, 1.5]);
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

/// The glTF normal-map convention, derived from the spec rather than the code: UV (0, 0) is the
/// image's top-left texel, and a tangent-space normal's +Y (green) leans towards the image's top
/// row. A quad facing +Z whose image top is world +Y, mapped with a texel leaning 45° to the
/// image top (128, 218, 218), must be lit more by a sun above (+Y) than by a sun below; a flat
/// texel (128, 128, 255) must be lit the same by both. Checked with generated tangents and with
/// explicit glTF tangents ((1, 0, 0, +1): bitangent = cross(+Z, +X) = +Y, the image top).
fn normal_map_convention_on(adapter: AdapterPreference, label: &str) {
    let test = format!("normal_map_convention[{label}]");
    let Some(mut gpu) = gpu_or_skip(&test, adapter) else { return };
    let n = [0.0, 0.0, 1.0];
    let v = vec![
        Vertex::new([-1.0, 1.0, 0.0], n),  // image top left
        Vertex::new([1.0, 1.0, 0.0], n),   // image top right
        Vertex::new([1.0, -1.0, 0.0], n),  // image bottom right
        Vertex::new([-1.0, -1.0, 0.0], n), // image bottom left
    ];
    let uvs = [[0.0f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let idx = [0, 3, 2, 0, 2, 1];
    let tangents = [[1.0f32, 0.0, 0.0, 1.0]; 4];
    let generated = gpu.upload_mesh_with(&v, &idx, &MeshStreams { uvs: Some(&uvs), ..Default::default() }).unwrap();
    let explicit = gpu.upload_mesh_with(&v, &idx, &MeshStreams { uvs: Some(&uvs), tangents: Some(&tangents), ..Default::default() }).unwrap();
    let texture = |gpu: &mut Gpu, texel: [u8; 4]| {
        let px: Vec<u8> = std::iter::repeat_n(texel, 16).flatten().collect();
        gpu.upload_material_texture(4, 4, TextureFormat::Rgba8Unorm, &px, TextureSampler::default()).unwrap()
    };
    let up = texture(&mut gpu, [128, 218, 218, 255]);
    let flat = texture(&mut gpu, [128, 128, 255, 255]);
    let target = gpu.create_target(64, 64).unwrap();
    let view = look_at_rh([0.0, 0.0, 3.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let view_proj = mul(&perspective_vk_reverse_z(50f32.to_radians(), 1.0, 0.05), &view);
    let lit = |gpu: &mut Gpu, mesh, map, sun: [f32; 3]| {
        let mut d = DrawItem::new(mesh, IDENTITY, [1.0; 4]);
        d.surface.textures = MaterialTextures { normal: Some(map), ..MaterialTextures::NONE };
        let environment = Environment { sun_direction: sun, ..Default::default() };
        gpu.render(target, &FrameDesc { view_proj, draws: &[d], reverse_z: true, environment, ..Default::default() }).unwrap();
        pixel(&gpu.read_rgba8(target).unwrap(), 64, 32, 32)[0]
    };
    let (above, below) = ([0.0, 0.6, 0.8], [0.0, -0.6, 0.8]);
    for (which, mesh) in [("generated tangents", generated), ("glTF tangents", explicit)] {
        let (a, b) = (lit(&mut gpu, mesh, up, above), lit(&mut gpu, mesh, up, below));
        eprintln!("{test}: {which}: leaning-up texel lit {a} by a sun above, {b} by a sun below");
        assert!(a > b + 40, "{test}: {which}: a normal leaning to the image top faces a sun above ({a} vs {b})");
        let (fa, fb) = (lit(&mut gpu, mesh, flat, above), lit(&mut gpu, mesh, flat, below));
        assert!(fa.abs_diff(fb) <= 1, "{test}: {which}: a flat map is symmetric ({fa} vs {fb})");
    }
    let errors = validation_errors(&test, gpu);
    assert!(errors.is_empty(), "{test}: {errors:#?}");
}

#[test]
fn normal_maps_follow_the_gltf_convention_on_every_adapter() {
    let Ok(adapters) = specula_gpu::enumerate_adapters() else {
        eprintln!("normal_map_convention: SKIPPED — Vulkan is not available");
        return;
    };
    for a in adapters {
        normal_map_convention_on(AdapterPreference::Index(a.index), &format!("{}", a.index));
    }
}

/// Skinned and textured pipeline variants are built by the first upload with those streams (or
/// `prepare_variants`), never by a frame; a mesh with UVs drawn without textures keeps its
/// vertex colour; an emissive texture without a colour glows white; an alpha-masked caster's
/// cut-out casts no shadow.
#[test]
fn variants_vertex_colours_emission_and_masked_shadows() {
    let test = "material_rules";
    let Some(mut gpu) = gpu_or_skip(test, AdapterPreference::HighPerformance) else { return };
    let created = |gpu: &Gpu| gpu.last_frame_stats().pipelines.created;
    let plain = created(&gpu);
    let (cv, ci) = cube();
    let cube_mesh = gpu.upload_mesh(&cv, &ci).unwrap();
    assert_eq!(created(&gpu), plain, "plain meshes need no new pipelines");
    let (v, i, skin) = bar();
    gpu.upload_mesh_with(&v, &i, &MeshStreams { skin: Some(&skin), ..Default::default() }).unwrap();
    let skinned = created(&gpu);
    assert!(skinned > plain, "the first skinned mesh builds the skinned variants");
    // A red-tinted quad with UVs.
    let (mut qv, qi, quv) = quad_uv();
    for vert in &mut qv {
        vert.color = [255, 0, 0, 255];
    }
    let quad = gpu.upload_mesh_with(&qv, &qi, &MeshStreams { uvs: Some(&quv), ..Default::default() }).unwrap();
    let textured = created(&gpu);
    assert!(textured > skinned, "the first mesh with UVs builds the textured variants");
    gpu.prepare_variants(true, true).unwrap();
    assert_eq!(created(&gpu), textured, "already built");
    gpu.mark_warm();
    let target = gpu.create_target(W, H).unwrap();
    // Without textures the quad keeps its vertex colour (the plain pipeline).
    gpu.render(target, &flat_frame(&[DrawItem::new(quad, IDENTITY, [1.0; 4])], 1.0)).unwrap();
    let p = pixel(&gpu.read_rgba8(target).unwrap(), W, W / 2, H / 2);
    assert!(p[0] > 200 && p[1] < 30 && p[2] < 30, "{test}: vertex colour kept: {p:?}");
    // An emissive texture without an emissive colour glows white × emissive, in the dark.
    let white: Vec<u8> = vec![255; 16];
    let glow = gpu.upload_material_texture(2, 2, TextureFormat::Rgba8Srgb, &white, TextureSampler::default()).unwrap();
    let mut d = DrawItem::new(quad, IDENTITY, [0.0, 0.0, 0.0, 1.0]);
    d.surface = Surface { emissive: 1.0, textures: MaterialTextures { emissive: Some(glow), ..Default::default() }, ..Default::default() };
    let draws = [d];
    let mut dark = flat_frame(&draws, 1.0);
    dark.environment.ambient_intensity = 0.0;
    gpu.render(target, &dark).unwrap();
    let p = pixel(&gpu.read_rgba8(target).unwrap(), W, W / 2, H / 2);
    assert!(p[..3].iter().all(|&c| c > 240), "{test}: a black surface glows white: {p:?}");
    // Masked shadows: a quad lying 1 m above a floor (x −1.5..1.5) under a sun 45° from +X, so
    // its shadow falls 1 m towards −X: the floor at x = −2 is in it but not under the quad. A
    // fully transparent texture masked at 0.5 casts nothing there.
    let clear: Vec<u8> = [255, 255, 255, 0].repeat(4);
    let cut = gpu.upload_material_texture(2, 2, TextureFormat::Rgba8Srgb, &clear, TextureSampler::default()).unwrap();
    let solid = gpu.upload_material_texture(2, 2, TextureFormat::Rgba8Srgb, &white, TextureSampler::default()).unwrap();
    let floor = DrawItem::new(cube_mesh, mul(&translation([0.0, -0.5, 0.0]), &scale_xyz([8.0, 1.0, 8.0])), [0.8, 0.8, 0.8, 1.0]);
    // The XY quad turned to lie flat (facing +Y) at y = 1, 3 m across.
    let lying = [3.0, 0.0, 0.0, 0.0, 0.0, 0.0, -3.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0];
    let view = look_at_rh([0.0, 6.0, 0.01], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let view_proj = mul(&perspective_vk_reverse_z(40f32.to_radians(), 1.0, 0.05), &view);
    let mut floor_lit = |texture, cutoff: f32| {
        let mut q = DrawItem::new(quad, lying, [1.0; 4]);
        q.surface.textures = MaterialTextures { base_color: Some(texture), alpha_cutoff: cutoff, ..Default::default() };
        let mut environment = Environment { sun_direction: [1.0, 1.0, 0.0], ..Default::default() };
        environment.shadow.enabled = true;
        let draws = [floor, q];
        gpu.render(target, &FrameDesc { view_proj, draws: &draws, reverse_z: true, environment, ..Default::default() }).unwrap();
        let rgba = gpu.read_rgba8(target).unwrap();
        let (x, y) = project(&view_proj, [-2.0, 0.0, 0.0], W, H);
        pixel(&rgba, W, x as u32, y as u32)[0]
    };
    let shaded = floor_lit(solid, 0.5);
    let open = floor_lit(cut, 0.5);
    eprintln!("{test}: under an opaque caster {shaded}, under a cut-out one {open}");
    assert!(open as u32 > shaded as u32 + 40, "{test}: the cut-out casts no shadow ({open} vs {shaded})");
    let stats = gpu.last_frame_stats();
    assert_eq!(stats.pipelines.created_after_warm, 0, "no frame built a pipeline");
    let errors = validation_errors(test, gpu);
    assert!(errors.is_empty(), "{test}: {errors:#?}");
}

fn scale_xyz(s: V3) -> [f32; 16] {
    [s[0], 0.0, 0.0, 0.0, 0.0, s[1], 0.0, 0.0, 0.0, 0.0, s[2], 0.0, 0.0, 0.0, 0.0, 1.0]
}

/// Keep `Gpu` in the imports used when every test skips.
#[allow(dead_code)]
fn _uses(_: &Gpu) {}
