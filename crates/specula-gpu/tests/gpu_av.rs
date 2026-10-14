//! Skinned draws under GPU-assisted validation (out-of-bounds buffer reads through device
//! addresses): a mesh whose vertices carry unweighted joints far past the palette (joints
//! [0, 255, 200, 100], weights [255, 0, 0, 0]) drawn with the last matrix of a 500-matrix frame
//! as its palette, in the scene and shadow passes, must read nothing outside the bones. Every
//! adapter; skips cleanly without Vulkan or the validation layer.
//!
//! The only test in this file: it sets `VK_LAYER_ENABLES` for the process before any instance
//! exists.

mod common;

use common::*;
use specula_gpu::{AdapterPreference, DrawItem, Environment, FrameDesc, MeshStreams, SkinVertex};

#[test]
fn unweighted_joints_never_read_outside_the_palette() {
    std::env::set_var("VK_LAYER_ENABLES", "VK_VALIDATION_FEATURE_ENABLE_GPU_ASSISTED_EXT");
    let Ok(adapters) = specula_gpu::enumerate_adapters() else {
        eprintln!("gpu_av: SKIPPED — Vulkan is not available");
        return;
    };
    for a in adapters {
        let test = format!("gpu_av[{}]", a.index);
        let Some(mut gpu) = gpu_or_skip(&test, AdapterPreference::Index(a.index)) else { return };
        if !gpu.validation_enabled() {
            eprintln!("{test}: SKIPPED — no validation layer");
            return;
        }
        let (v, i) = cube();
        let skin: Vec<SkinVertex> = v.iter().map(|_| SkinVertex { joints: [0, 255, 200, 100], weights: [255, 0, 0, 0] }).collect();
        let mesh = gpu.upload_mesh_with(&v, &i, &MeshStreams { skin: Some(&skin), ..Default::default() }).unwrap();
        let target = gpu.create_target(128, 128).unwrap();
        wait_variants(&mut gpu, true, false);
        let bones = vec![IDENTITY; 500];
        let mut d = DrawItem::new(mesh, translation([0.0, 0.0, -4.0]), [1.0, 0.5, 0.2, 1.0]);
        d.skin = Some(499);
        let mut environment = Environment { sun_direction: [-0.3, 0.8, 0.5], ..Default::default() };
        environment.shadow.enabled = true;
        let view_proj = perspective_vk_reverse_z(50f32.to_radians(), 1.0, 0.05);
        let frame = FrameDesc { view_proj, draws: &[d], reverse_z: true, bones: &bones, environment, ..Default::default() };
        for _ in 0..3 {
            gpu.render(target, &frame).unwrap();
        }
        let centre = pixel(&gpu.read_rgba8(target).unwrap(), 128, 64, 64);
        assert!(centre[0] > 100, "{test}: the cube is drawn {centre:?}");
        let errors = validation_errors(&test, gpu);
        assert!(errors.is_empty(), "{test}: {errors:#?}");
    }
}
