//! Sight's ID buffer on skinned, textured, alpha-masked draws (from the round-7 review's probes):
//! the ids cover exactly the pixels the image shows (the bar posed on the GPU, its masked texels
//! cut with the scene's texture, UVs and mip level), on every adapter, under validation. Skips
//! cleanly (prints why, passes) without Vulkan.

mod common;

use common::*;
use specula_gpu::{
    AdapterPreference, DrawItem, Environment, FrameDesc, MaterialTextures, MeshStreams, SkinVertex, Surface, TextureFilter, TextureFormat,
    TextureSampler, Vertex,
};

const W: u32 = 256;
const H: u32 = 256;

/// A 0.4 × 2 × 0.4 bar standing on the origin, 17 rings up, skinned to two bones (blending around
/// y = 1), with UVs running down its sides.
fn bar() -> (Vec<Vertex>, Vec<u32>, Vec<SkinVertex>, Vec<[f32; 2]>) {
    let (mut vertices, mut skin, mut indices, mut uvs) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let corners = [[-0.2, -0.2], [0.2, -0.2], [0.2, 0.2], [-0.2, 0.2]];
    let rings = 17;
    for side in 0..4 {
        let (a, b) = (corners[side], corners[(side + 1) % 4]);
        let n = normalize([a[0] + b[0], 0.0, a[1] + b[1]]);
        let base = vertices.len() as u32;
        for r in 0..rings {
            let y = r as f32 * 2.0 / (rings - 1) as f32;
            let w1 = (((y - 0.9) / 0.2).clamp(0.0, 1.0) * 255.0).round() as u8;
            for (k, c) in [a, b].into_iter().enumerate() {
                vertices.push(Vertex::new([c[0], y, c[1]], n));
                skin.push(SkinVertex { joints: [0, 1, 0, 0], weights: [255 - w1, w1, 0, 0] });
                uvs.push([k as f32, 1.0 - y / 2.0]);
            }
        }
        for r in 0..rings as u32 - 1 {
            let i = base + r * 2;
            indices.extend_from_slice(&[i, i + 1, i + 3, i, i + 3, i + 2]);
        }
    }
    (vertices, indices, skin, uvs)
}

fn rotation_z(radians: f32) -> [f32; 16] {
    let (s, c) = radians.sin_cos();
    [c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]
}

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
            let n = normalize([0, 1, 2].map(|r| m[r] * v.normal[0] + m[4 + r] * v.normal[1] + m[8 + r] * v.normal[2]));
            Vertex::new(transform_point(&m, v.position), n)
        })
        .collect()
}

/// Flat white on blue: every pixel that isn't the clear colour shows the bar.
fn frame<'a>(draws: &'a [DrawItem], bones: &'a [[f32; 16]], distance: f32) -> FrameDesc<'a> {
    let view = look_at_rh([0.0, 1.0, distance], [0.0, 1.0, 0.0], [0.0, 1.0, 0.0]);
    let view_proj = mul(&perspective_vk_reverse_z(60f32.to_radians(), 1.0, 0.05), &view);
    let environment = Environment { ambient_intensity: 1.0, sun_intensity: 0.0, ..Default::default() };
    FrameDesc { view_proj, clear_color: [0.0, 0.0, 1.0, 1.0], draws, reverse_z: true, environment, bones, ..Default::default() }
}

fn shown(rgba: &[u8]) -> u64 {
    rgba.chunks(4).filter(|p| !(p[0] < 20 && p[1] < 20 && p[2] > 200)).count() as u64
}

/// `case`: "stripes" (1 × 4 texels, opaque and cut in turn, nearest, close up) or "noise"
/// (64 × 64 random alpha, trilinear, minified far away: the cut follows the mip level).
fn run(adapter: AdapterPreference, label: &str, case: &str) {
    let test = format!("ids_skin_tex[{case}, {label}]");
    let Some(mut gpu) = gpu_or_skip(&test, adapter) else { return };
    let (v, i, skin, uv) = bar();
    let mesh = gpu.upload_mesh_with(&v, &i, &MeshStreams { skin: Some(&skin), uvs: Some(&uv), ..Default::default() }).unwrap();
    let target = gpu.create_target(W, H).unwrap();
    wait_variants(&mut gpu, true, true);
    gpu.mark_warm();
    let (tex, distance) = if case == "stripes" {
        let px = [255, 255, 255, 255, 255, 255, 255, 0, 255, 255, 255, 255, 255, 255, 255, 0];
        let nearest = TextureSampler { mag: TextureFilter::Nearest, min: TextureFilter::Nearest, mipmap: TextureFilter::Nearest, ..Default::default() };
        (gpu.upload_material_texture(1, 4, TextureFormat::Rgba8Srgb, &px, nearest).unwrap(), 4.0)
    } else {
        let (mut px, mut seed) = (Vec::new(), 12345u32);
        for _ in 0..64 * 64 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            px.extend_from_slice(&[255, 255, 255, (seed >> 24) as u8]);
        }
        let linear = TextureSampler { mag: TextureFilter::Linear, min: TextureFilter::Linear, mipmap: TextureFilter::Linear, ..Default::default() };
        (gpu.upload_material_texture(64, 64, TextureFormat::Rgba8Srgb, &px, linear).unwrap(), 12.0)
    };
    let bones = palette(1.2);
    let mut d = DrawItem::new(mesh, IDENTITY, [1.0; 4]);
    d.skin = Some(0);
    d.bounds = Some([0.0, 1.0, 0.0, 1.5]);
    d.surface = Surface { textures: MaterialTextures { base_color: Some(tex), alpha_cutoff: 0.5, ..Default::default() }, ..Default::default() };
    // GPU-skinned, masked: the ids are exactly the pixels shown.
    gpu.render_with_ids(target, &frame(&[d], &bones, distance), &[1]).unwrap();
    let image = shown(&gpu.read_rgba8(target).unwrap());
    let ids = gpu.id_coverage(target, 1).unwrap().per_id[0];
    assert!(image > 100, "{test}: the bar shows ({image} px)");
    assert_eq!(ids.opaque_pixels, image, "{test}: ids {ids:?} vs {image} pixels shown");
    // The CPU-posed copy (the textured, unskinned path) covers the same.
    let cpu = gpu.upload_mesh_with(&posed(&v, &skin, &bones), &i, &MeshStreams { uvs: Some(&uv), ..Default::default() }).unwrap();
    let mut dc = DrawItem::new(cpu, IDENTITY, [1.0; 4]);
    dc.surface = d.surface;
    gpu.render_with_ids(target, &frame(&[dc], &[], distance), &[1]).unwrap();
    let image_cpu = shown(&gpu.read_rgba8(target).unwrap());
    let ids_cpu = gpu.id_coverage(target, 1).unwrap().per_id[0];
    assert_eq!(ids_cpu.opaque_pixels, image_cpu, "{test}: CPU-posed {ids_cpu:?} vs {image_cpu}");
    assert!(ids.opaque_pixels.abs_diff(ids_cpu.opaque_pixels) <= image / 50 + 4, "{test}: GPU- and CPU-posed agree: {ids:?} vs {ids_cpu:?}");
    // Unmasked, more shows.
    let mut du = d;
    du.surface.textures.alpha_cutoff = 0.0;
    gpu.render_with_ids(target, &frame(&[du], &bones, distance), &[1]).unwrap();
    let whole = gpu.id_coverage(target, 1).unwrap().per_id[0];
    assert!(whole.opaque_pixels > ids.opaque_pixels, "{test}: the mask cuts something ({whole:?} vs {ids:?})");
    assert_eq!(gpu.last_frame_stats().pipelines.created_after_warm, 0, "{test}: nothing compiled in a frame");
    let errors = validation_errors(&test, gpu);
    assert!(errors.is_empty(), "{test}: {errors:#?}");
}

#[test]
fn ids_cover_exactly_what_skinned_masked_draws_show() {
    let Ok(adapters) = specula_gpu::enumerate_adapters() else {
        eprintln!("ids_skin_tex: SKIPPED — Vulkan is not available");
        return;
    };
    for a in adapters {
        for case in ["stripes", "noise"] {
            run(AdapterPreference::Index(a.index), &format!("{}", a.index), case);
        }
    }
}
