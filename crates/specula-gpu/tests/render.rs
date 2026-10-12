//! Headless rendering on real GPUs. Every test skips cleanly (prints why, passes) without Vulkan.

mod common;

use common::*;
use specula_gpu::{AdapterPreference, DrawItem, FrameDesc, MeshId, TargetId};

const SIZE: u32 = 256;
/// Exactly representable in 8-bit sRGB, so the clear must round-trip bit-exactly.
const CLEAR_BYTES: [u8; 4] = [26, 51, 77, 255];
const CUBE_SRGB: [f32; 4] = [0.85, 0.55, 0.25, 1.0];

fn clear_color() -> [f32; 4] {
    CLEAR_BYTES.map(|b| b as f32 / 255.0)
}

fn camera() -> [f32; 16] {
    let view = look_at_rh([2.0, 1.6, 2.6], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let proj = perspective_vk(45f32.to_radians(), 1.0, 0.1, 100.0);
    mul(&proj, &view)
}

#[test]
fn enumerate_adapters_lists_devices() {
    if !specula_gpu::is_available() {
        eprintln!("enumerate_adapters_lists_devices: SKIPPED — Vulkan is not available");
        return;
    }
    let adapters = specula_gpu::enumerate_adapters().expect("enumerate_adapters");
    for a in &adapters {
        eprintln!(
            "adapter {}: {} ({:?}) vendor {:#06x} device {:#06x} Vulkan {}.{}.{} driver {:#x}",
            a.index, a.name, a.kind, a.vendor_id, a.device_id, a.api_version.0, a.api_version.1, a.api_version.2, a.driver_version
        );
    }
    for (i, a) in adapters.iter().enumerate() {
        assert_eq!(a.index, i);
        assert!(!a.name.is_empty());
        assert!(a.api_version.0 >= 1);
    }
}

/// Render the lit cube on one adapter and check clear, coverage, orientation and lighting.
fn render_cube_on(adapter: AdapterPreference, label: &str) {
    let test = format!("headless_cube[{label}]");
    let Some(mut gpu) = gpu_or_skip(&test, adapter) else { return };
    eprintln!("{test}: {} (validation {})", gpu.adapter().name, gpu.validation_enabled());

    let (vertices, indices) = cube();
    assert_eq!((vertices.len(), indices.len()), (24, 36));
    let mesh = gpu.upload_mesh(&vertices, &indices).expect("upload_mesh");
    let target = gpu.create_target(SIZE, SIZE).expect("create_target");
    let view_proj = camera();
    let draws = [DrawItem { mesh, model: IDENTITY, color: CUBE_SRGB }];
    gpu.render(target, &FrameDesc { view_proj, clear_color: clear_color(), draws: &draws }).expect("render");
    let rgba = gpu.read_rgba8(target).expect("read_rgba8");
    assert_eq!(rgba.len(), (SIZE * SIZE * 4) as usize);

    let safe_name: String = gpu.adapter().name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let path = write_png(&format!("cube_{label}_{safe_name}.png"), &rgba, SIZE, SIZE);
    eprintln!("{test}: wrote {}", path.display());

    for (x, y) in [(0, 0), (SIZE - 1, 0), (0, SIZE - 1), (SIZE - 1, SIZE - 1)] {
        assert_eq!(pixel(&rgba, SIZE, x, y), CLEAR_BYTES, "corner ({x},{y}) must be the clear colour exactly");
    }
    let centre = pixel(&rgba, SIZE, SIZE / 2, SIZE / 2);
    assert_ne!(centre, CLEAR_BYTES, "centre pixel must be the cube");

    // Sample the centre of each visible face and compare with the lighting formula.
    let l = light_dir();
    let mut seen = Vec::new();
    for n in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        let (x, y) = project(&view_proj, [n[0] * 0.5, n[1] * 0.5, n[2] * 0.5], SIZE, SIZE);
        let got = pixel(&rgba, SIZE, x as u32, y as u32);
        let ndl = dot(n, l);
        let want = [expected_lit(CUBE_SRGB[0], ndl), expected_lit(CUBE_SRGB[1], ndl), expected_lit(CUBE_SRGB[2], ndl), 255];
        eprintln!("{test}: face {n:?} at ({x:.0},{y:.0}) got {got:?} want {want:?}");
        for c in 0..4 {
            assert!(got[c].abs_diff(want[c]) <= 2, "face {n:?} channel {c}: got {got:?}, want {want:?}");
        }
        seen.push(got);
    }
    assert!(seen[0] != seen[1] && seen[1] != seen[2] && seen[0] != seen[2], "faces must differ in brightness: {seen:?}");
    // Upright: the +Y face is above the image centre and is the brightest.
    let (_, top_y) = project(&view_proj, [0.0, 0.5, 0.0], SIZE, SIZE);
    assert!(top_y < SIZE as f32 / 2.0);
    assert!(seen[1][0] > seen[0][0] && seen[1][0] > seen[2][0]);

    gpu.destroy_target(target);
    gpu.destroy_mesh(mesh);
    let errors = validation_errors(&test, gpu);
    assert!(errors.is_empty(), "{test}: {} validation error(s)", errors.len());
}

#[test]
fn headless_cube_on_every_adapter() {
    if !specula_gpu::is_available() {
        eprintln!("headless_cube_on_every_adapter: SKIPPED — Vulkan is not available");
        return;
    }
    let adapters = specula_gpu::enumerate_adapters().expect("enumerate_adapters");
    if adapters.is_empty() {
        eprintln!("headless_cube_on_every_adapter: SKIPPED — no adapters");
    }
    for a in &adapters {
        render_cube_on(AdapterPreference::Index(a.index), &format!("{}", a.index));
    }
}

#[test]
fn preferences_pick_expected_kinds() {
    let Some(gpu) = gpu_or_skip("preferences", AdapterPreference::HighPerformance) else { return };
    let adapters = specula_gpu::enumerate_adapters().unwrap();
    if adapters.iter().any(|a| a.kind == specula_gpu::AdapterKind::Discrete) {
        assert_eq!(gpu.adapter().kind, specula_gpu::AdapterKind::Discrete);
    }
    drop(gpu);
    let Some(gpu) = gpu_or_skip("preferences", AdapterPreference::LowPower) else { return };
    if adapters.iter().any(|a| a.kind == specula_gpu::AdapterKind::Integrated) {
        assert_eq!(gpu.adapter().kind, specula_gpu::AdapterKind::Integrated);
    }
    let err = specula_gpu::Gpu::new(&specula_gpu::GpuOptions {
        validation: specula_gpu::Validation::Off,
        adapter: AdapterPreference::Index(adapters.len()),
    })
    .err()
    .expect("out-of-range adapter index must fail");
    assert!(err.message.contains("out of range"), "{err}");
}

#[test]
fn resource_churn_and_errors() {
    let test = "resource_churn";
    let Some(mut gpu) = gpu_or_skip(test, AdapterPreference::HighPerformance) else { return };
    let (vertices, indices) = cube();
    let view_proj = camera();

    // Structured errors, no validation noise.
    assert!(gpu.upload_mesh(&[], &[]).is_err());
    assert!(gpu.upload_mesh(&vertices, &indices[..4]).is_err());
    assert!(gpu.upload_mesh(&vertices, &[0, 1, 99]).is_err());
    assert!(gpu.create_target(0, 16).is_err());
    let t = gpu.create_target(8, 8).unwrap();
    assert!(gpu.read_rgba8(t).is_err(), "reading an unrendered target is an error");
    let bogus = [DrawItem { mesh: MeshId(9999), model: IDENTITY, color: CUBE_SRGB }];
    let err = gpu.render(t, &FrameDesc { view_proj, clear_color: clear_color(), draws: &bogus }).unwrap_err();
    assert!(err.message.contains("9999"), "{err}");
    assert!(gpu.render(TargetId(9999), &FrameDesc { view_proj, clear_color: clear_color(), draws: &[] }).is_err());
    gpu.destroy_target(t);
    gpu.destroy_target(t); // double destroy is a no-op
    gpu.destroy_mesh(MeshId(12345));

    let mut kept = Vec::new();
    for i in 0..24u32 {
        let w = 16 + i * 13;
        let h = 9 + i * 7;
        let target = gpu.create_target(w, h).expect("create_target");
        let mesh = gpu.upload_mesh(&vertices, &indices).expect("upload_mesh");
        let draws = [
            DrawItem { mesh, model: rotation_y(i as f32 * 0.3), color: CUBE_SRGB },
            DrawItem { mesh, model: translation([0.9, 0.0, -0.8]), color: [0.2, 0.6, 0.9, 1.0] },
        ];
        for _ in 0..2 {
            gpu.render(target, &FrameDesc { view_proj, clear_color: clear_color(), draws: &draws }).expect("render");
            let rgba = gpu.read_rgba8(target).expect("read");
            assert_eq!(rgba.len(), (w * h * 4) as usize);
            assert_eq!(pixel(&rgba, w, 0, 0), CLEAR_BYTES);
        }
        if i % 3 == 0 {
            kept.push((mesh, target)); // destroyed later, out of creation order
        } else {
            gpu.destroy_mesh(mesh);
            gpu.destroy_target(target);
        }
    }
    for (mesh, target) in kept.into_iter().rev() {
        gpu.destroy_target(target);
        gpu.destroy_mesh(mesh);
    }
    let errors = validation_errors(test, gpu);
    assert!(errors.is_empty(), "{test}: {} validation error(s)", errors.len());
}
