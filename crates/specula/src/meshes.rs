//! Built-in shape meshes (object space, metres, centred on the origin).

use crate::world::{Shape, World};
use specula_gpu::{Gpu, GpuError, MeshId, Vertex};
use std::collections::BTreeMap;
use std::f32::consts::{PI, TAU};

/// Lathe meshes by profile key ([`crate::world::Profile::key`]): entities with identical
/// profiles share one mesh, so they also instance together.
#[derive(Default)]
pub struct LatheMeshes {
    meshes: BTreeMap<u64, MeshId>,
    /// Profiles whose upload failed, with why (retried when the world's meshes change).
    failed: BTreeMap<u64, String>,
    /// The world mesh epoch and generation last synced to.
    synced: Option<(u64, u64)>,
}

impl LatheMeshes {
    /// Uploads meshes for profiles in use and frees the rest, when the world's meshes changed
    /// since the last call. Returns a message per failed upload (those entities aren't drawn).
    pub fn sync(&mut self, gpu: &mut Gpu, world: &World, world_generation: u64) -> Vec<String> {
        let key = (world.mesh_epoch(), world_generation);
        if self.synced == Some(key) {
            return Vec::new();
        }
        self.synced = Some(key);
        let mut live = BTreeMap::new();
        for e in world.entities() {
            if let Some(p) = &e.profile {
                live.entry(p.key()).or_insert((p, e.name.as_str()));
            }
        }
        let stale: Vec<u64> = self.meshes.keys().copied().filter(|k| !live.contains_key(k)).collect();
        for k in stale {
            if let Some(mesh) = self.meshes.remove(&k) {
                gpu.destroy_mesh(mesh);
            }
        }
        self.failed.clear();
        let mut errors = Vec::new();
        for (key, (profile, name)) in live {
            if self.meshes.contains_key(&key) {
                continue;
            }
            let (vertices, indices) = lathe(&profile.points, profile.segments);
            match gpu.upload_mesh(&vertices, &indices) {
                Ok(mesh) => {
                    self.meshes.insert(key, mesh);
                }
                Err(err) => {
                    let msg = format!("lathe '{name}' not drawn: {err}");
                    errors.push(msg.clone());
                    self.failed.insert(key, msg);
                }
            }
        }
        errors
    }

    pub fn get(&self, profile_key: u64) -> Option<MeshId> {
        self.meshes.get(&profile_key).copied()
    }

    /// Meshes uploaded (one per distinct profile in use).
    pub fn count(&self) -> usize {
        self.meshes.len()
    }

    /// Messages for profiles whose upload failed.
    pub fn failures(&self) -> impl Iterator<Item = &String> {
        self.failed.values()
    }
}

/// GPU mesh for each built-in shape (lathes have their own, built from their profile).
pub struct ShapeMeshes {
    cube: MeshId,
    quad: MeshId,
    sphere: MeshId,
}

impl ShapeMeshes {
    pub fn upload(gpu: &mut Gpu) -> Result<ShapeMeshes, GpuError> {
        let (v, i) = cube();
        let cube = gpu.upload_mesh(&v, &i)?;
        let (v, i) = quad();
        let quad = gpu.upload_mesh(&v, &i)?;
        let (v, i) = sphere(32, 16);
        let sphere = gpu.upload_mesh(&v, &i)?;
        Ok(ShapeMeshes { cube, quad, sphere })
    }

    /// `None` for lathes.
    pub fn get(&self, shape: Shape) -> Option<MeshId> {
        match shape {
            Shape::Cube => Some(self.cube),
            Shape::Quad => Some(self.quad),
            Shape::Sphere => Some(self.sphere),
            Shape::Lathe | Shape::Model | Shape::Group | Shape::Sprite | Shape::Tilemap | Shape::Shape2d => None,
        }
    }
}

/// Unit cube: 4 vertices per face so each face has a flat normal.
pub fn cube() -> (Vec<Vertex>, Vec<u32>) {
    // (normal, tangent u, tangent v) per face; corners = n/2 ± u/2 ± v/2.
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ];
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (n, u, v) in faces {
        let base = vertices.len() as u32;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = |k: usize| 0.5 * (n[k] + su * u[k] + sv * v[k]);
            vertices.push(Vertex::new([p(0), p(1), p(2)], n));
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (vertices, indices)
}

/// 1×1 plane in XZ facing +Y.
pub fn quad() -> (Vec<Vertex>, Vec<u32>) {
    let n = [0.0, 1.0, 0.0];
    let vertices = [[-0.5, 0.0, 0.5], [0.5, 0.0, 0.5], [0.5, 0.0, -0.5], [-0.5, 0.0, -0.5]]
        .map(|position| Vertex::new(position, n))
        .to_vec();
    (vertices, vec![0, 1, 2, 0, 2, 3])
}

/// UV sphere of radius 0.5.
pub fn sphere(segments: u32, rings: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    for r in 0..=rings {
        let phi = PI * r as f32 / rings as f32; // 0 at +Y pole
        for s in 0..=segments {
            let theta = TAU * s as f32 / segments as f32;
            let n = [phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin()];
            vertices.push(Vertex::new(n.map(|c| c * 0.5), n));
        }
    }
    let row = segments + 1;
    let mut indices = Vec::new();
    for r in 0..rings {
        for s in 0..segments {
            let a = r * row + s;
            let b = a + row;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    (vertices, indices)
}

/// Profile corners sharper than this get a hard edge; gentler ones are smoothed.
const SMOOTH_ANGLE_COS: f32 = 0.82; // ≈ 35°

/// A surface of revolution: `profile` (`[radius, height]`, bottom to top) spun around +Y in
/// `segments` steps. Ends with a non-zero radius are closed with flat caps.
pub fn lathe(profile: &[[f32; 2]], segments: u32) -> (Vec<Vertex>, Vec<u32>) {
    let segments = segments.max(3);
    // Outward normal of each profile segment in (radius, height); degenerate segments skipped.
    let mut segs: Vec<([f32; 2], [f32; 2], [f32; 2])> = Vec::new();
    for w in profile.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (dr, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dr * dr + dy * dy).sqrt();
        if len > 1e-7 {
            segs.push((a, b, [dy / len, -dr / len]));
        }
    }
    let smooth = |n: [f32; 2], other: Option<[f32; 2]>| match other {
        Some(o) if n[0] * o[0] + n[1] * o[1] >= SMOOTH_ANGLE_COS => {
            let (x, y) = (n[0] + o[0], n[1] + o[1]);
            let len = (x * x + y * y).sqrt();
            [x / len, y / len]
        }
        _ => n,
    };
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let ring = |vertices: &mut Vec<Vertex>, p: [f32; 2], n: [f32; 2]| {
        for i in 0..=segments {
            let theta = TAU * i as f32 / segments as f32;
            let (s, c) = theta.sin_cos();
            vertices.push(Vertex::new([p[0] * c, p[1], p[0] * s], [n[0] * c, n[1], n[0] * s]));
        }
    };
    for (k, &(a, b, n)) in segs.iter().enumerate() {
        let na = smooth(n, k.checked_sub(1).map(|j| segs[j].2));
        let nb = smooth(n, segs.get(k + 1).map(|s| s.2));
        let base = vertices.len() as u32;
        ring(&mut vertices, a, na);
        ring(&mut vertices, b, nb);
        let row = segments + 1;
        for i in 0..segments {
            let (p, q) = (base + i, base + row + i);
            indices.extend_from_slice(&[p, q, p + 1, p + 1, q, q + 1]);
        }
    }
    let mut cap = |p: [f32; 2], up: bool| {
        if p[0] <= 1e-6 {
            return;
        }
        let n = if up { [0.0, 1.0] } else { [0.0, -1.0] };
        let centre = vertices.len() as u32;
        vertices.push(Vertex::new([0.0, p[1], 0.0], [0.0, n[1], 0.0]));
        ring(&mut vertices, p, n);
        for i in 0..segments {
            indices.extend_from_slice(&[centre, centre + 1 + i, centre + 2 + i]);
        }
    };
    if let (Some(first), Some(last)) = (segs.first(), segs.last()) {
        cap(first.0, false);
        cap(last.1, true);
    }
    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(vertices: &[Vertex], indices: &[u32]) {
        assert_eq!(indices.len() % 3, 0);
        assert!(indices.iter().all(|&i| (i as usize) < vertices.len()));
        for v in vertices {
            let n = v.normal;
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-4, "normal not unit: {n:?}");
            assert!(v.position.iter().all(|c| c.abs() <= 0.5 + 1e-5));
        }
    }

    #[test]
    fn shapes_are_well_formed() {
        let (v, i) = cube();
        assert_eq!((v.len(), i.len()), (24, 36));
        check(&v, &i);
        let (v, i) = quad();
        check(&v, &i);
        let (v, i) = sphere(8, 4);
        check(&v, &i);
    }

    #[test]
    fn lathe_is_well_formed_and_capped() {
        // A cylinder: radius 0.5, height 1 (bottom at -0.5), with flat caps.
        let (v, i) = lathe(&[[0.5, -0.5], [0.5, 0.5]], 16);
        check(&v, &i);
        assert_eq!(v.len(), 2 * 17 + 2 * (1 + 17));
        assert_eq!(i.len(), 16 * 6 + 2 * 16 * 3);
        let side = &v[0];
        assert!((side.normal[0] - 1.0).abs() < 1e-6 && side.normal[1].abs() < 1e-6, "outward: {:?}", side.normal);
        // A cone to a point: no top cap; a duplicate point is ignored.
        let (v, i) = lathe(&[[0.5, -0.5], [0.5, -0.5], [0.0, 0.5]], 8);
        check(&v, &i);
        assert_eq!(v.len(), 2 * 9 + (1 + 9));
    }

    #[test]
    fn lathe_smooths_gentle_bends_and_keeps_sharp_ones() {
        // Gentle bend (10°): the shared rings get the same normal.
        let (v, _) = lathe(&[[0.5, 0.0], [0.5, 1.0], [0.4, 1.6]], 4);
        assert_eq!(v[5].normal, v[10].normal);
        // Right-angle corner: hard edge.
        let (v, _) = lathe(&[[0.5, 0.0], [0.5, 1.0], [0.0, 1.0]], 4);
        assert_ne!(v[5].normal, v[10].normal);
    }

    /// Reviewer finding (round 3): counting glass by mesh topology double-counted the engine's
    /// sphere and lathes (seams that don't weld bit-exactly) and missed glass seen from inside.
    /// A glass mesh must count the pixels its opaque twin covers (its silhouette), whatever its
    /// shape: closed, capped, concave (cup, torus); on every adapter.
    #[test]
    fn glass_counts_like_its_opaque_twin() {
        use specula_gpu::{DrawItem, FrameDesc, Gpu, GpuOptions, Material, Validation};
        if !specula_gpu::is_available() {
            eprintln!("glass_counts_like_its_opaque_twin: SKIPPED — Vulkan is not available");
            return;
        }
        let circle: Vec<[f32; 2]> = (0..=16).map(|k| {
            let a = TAU * k as f32 / 16.0;
            [0.3 + 0.12 * a.cos(), 0.12 * a.sin()]
        }).collect();
        type Named = (&'static str, (Vec<Vertex>, Vec<u32>));
        let shapes: Vec<Named> = vec![
            ("sphere", sphere(32, 16)),
            ("vase (closed profile)", lathe(&[[0.0, -0.5], [0.3, -0.45], [0.4, -0.1], [0.15, 0.3], [0.25, 0.5], [0.0, 0.5]], 24)),
            ("tube (open profile, capped)", lathe(&[[0.3, -0.5], [0.35, 0.0], [0.3, 0.5]], 24)),
            ("cup (concave)", lathe(&[[0.0, -0.5], [0.4, -0.5], [0.45, 0.5], [0.4, 0.5], [0.35, -0.4], [0.0, -0.4]], 24)),
            ("torus (concave)", lathe(&circle, 32)),
        ];
        for a in specula_gpu::enumerate_adapters().unwrap() {
            let options = GpuOptions { validation: Validation::On, adapter: specula_gpu::AdapterPreference::Index(a.index), ..Default::default() };
            let mut gpu = match Gpu::new(&options) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("glass_counts_like_its_opaque_twin: SKIPPED on adapter {} — {e}", a.index);
                    continue;
                }
            };
            if !gpu.ids_count_blended() {
                eprintln!("glass_counts_like_its_opaque_twin: SKIPPED on adapter {} — no fragment atomics", a.index);
                continue;
            }
            let (w, h) = (320u32, 240u32);
            let target = gpu.create_target(w, h).unwrap();
            // Looking down at 35 degrees, so concave insides and rims show.
            let cam = crate::sight::Camera::new(specula_core::math::Vec3::new(0.0, 1.4, 2.0), specula_core::math::Vec3::ZERO, 45.0);
            let vp = cam.view_proj(w, h).to_cols_array();
            let model = specula_core::math::Mat4::from_trs(specula_core::math::Vec3::ZERO, specula_core::math::Quat::IDENTITY, specula_core::math::Vec3::new(1.5, 1.5, 1.5)).to_cols_array();
            for (name, (v, i)) in &shapes {
                let mesh = gpu.upload_mesh(v, i).unwrap();
                let mut count = |material: Material| {
                    let draw = [DrawItem { material, ..DrawItem::new(mesh, model, [0.8, 0.9, 1.0, if material == Material::Lit { 1.0 } else { 0.2 }]) }];
                    let frame = FrameDesc { view_proj: vp, draws: &draw, reverse_z: true, ..Default::default() };
                    gpu.render_with_ids(target, &frame, &[1]).unwrap();
                    gpu.id_coverage(target, 1).unwrap().per_id[0]
                };
                let (opaque, glass) = (count(Material::Lit), count(Material::Glass));
                let ratio = glass.blended_pixels as f64 / opaque.opaque_pixels as f64;
                assert!(opaque.opaque_pixels > 1000, "adapter {}: {name}: {opaque:?}", a.index);
                assert!((0.97..=1.03).contains(&ratio), "adapter {}: {name}: glass {} px vs opaque {} px ({ratio:.3})", a.index, glass.blended_pixels, opaque.opaque_pixels);
                assert_eq!((glass.min, glass.max), (opaque.min, opaque.max), "adapter {}: {name}: same exact box", a.index);
                gpu.destroy_mesh(mesh);
            }
            gpu.destroy_target(target);
            let warnings: Vec<_> = gpu.take_validation_messages().into_iter().filter(|m| m.severity >= specula_gpu::Severity::Warning).collect();
            assert!(warnings.is_empty(), "adapter {}: {warnings:#?}", a.index);
        }
    }

    /// Reviewer finding (round 4): with more blended ids at a pixel than the per-pixel sets hold,
    /// counts depended on fragment order (NVIDIA varied between identical frames). Now: identical
    /// frames give identical counts; ids within the sets are exact; only ids past them are
    /// approximate (lower bounds: never more than their true coverage).
    #[test]
    fn nested_glass_counts_are_reproducible() {
        use specula_core::math::{Mat4, Quat, Vec3};
        use specula_gpu::{DrawItem, FrameDesc, Gpu, GpuOptions, Material, Validation};
        if !specula_gpu::is_available() {
            eprintln!("nested_glass_counts_are_reproducible: SKIPPED — Vulkan is not available");
            return;
        }
        for a in specula_gpu::enumerate_adapters().unwrap() {
            let options = GpuOptions { validation: Validation::On, adapter: specula_gpu::AdapterPreference::Index(a.index), ..Default::default() };
            let mut gpu = match Gpu::new(&options) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("nested_glass_counts_are_reproducible: SKIPPED on adapter {} — {e}", a.index);
                    continue;
                }
            };
            if !gpu.ids_count_blended() {
                eprintln!("nested_glass_counts_are_reproducible: SKIPPED on adapter {} — no fragment atomics", a.index);
                continue;
            }
            let (v, i) = sphere(32, 16);
            let mesh = gpu.upload_mesh(&v, &i).unwrap();
            let (w, h) = (320u32, 240u32);
            let target = gpu.create_target(w, h).unwrap();
            let cam = crate::sight::Camera::new(Vec3::new(0.0, 0.5, 3.0), Vec3::ZERO, 45.0);
            let vp = cam.view_proj(w, h).to_cols_array();
            let model = |s: f32| Mat4::from_trs(Vec3::ZERO, Quat::IDENTITY, Vec3::new(s, s, s)).to_cols_array();
            for n in [3usize, 5] {
                // Concentric spheres, the largest first (id 1).
                let scales: Vec<f32> = (0..n).map(|k| 1.6 - 0.25 * k as f32).collect();
                let glass: Vec<DrawItem> = scales.iter().map(|&s| DrawItem { material: Material::Glass, ..DrawItem::new(mesh, model(s), [0.9, 0.9, 1.0, 0.2]) }).collect();
                let ids: Vec<u32> = (1..=n as u32).collect();
                let frame = FrameDesc { view_proj: vp, draws: &glass, reverse_z: true, ..Default::default() };
                let mut runs = Vec::new();
                for _ in 0..8 {
                    gpu.render_with_ids(target, &frame, &ids).unwrap();
                    runs.push(gpu.id_coverage(target, n as u32).unwrap().per_id);
                }
                assert!(runs.windows(2).all(|r| r[0] == r[1]), "adapter {}: {n} spheres: counts differ between identical frames: {runs:?}", a.index);
                for (k, &s) in scales.iter().enumerate() {
                    // Its silhouette alone, opaque.
                    let twin = [DrawItem::new(mesh, model(s), [1.0; 4])];
                    gpu.render_with_ids(target, &FrameDesc { view_proj: vp, draws: &twin, reverse_z: true, ..Default::default() }, &[1]).unwrap();
                    let truth = gpu.id_coverage(target, 1).unwrap().per_id[0].opaque_pixels as f64;
                    let got = &runs[0][k];
                    let ratio = got.blended_pixels as f64 / truth;
                    if k < 4 {
                        assert!(!got.approx && (0.97..=1.03).contains(&ratio), "adapter {}: {n} spheres, #{}: {got:?} vs {truth} ({ratio:.3})", a.index, k + 1);
                    } else {
                        assert!(got.approx && ratio <= 1.03, "adapter {}: {n} spheres, #{}: approximate, a lower bound: {got:?} vs {truth}", a.index, k + 1);
                    }
                }
            }
            gpu.destroy_target(target);
            gpu.destroy_mesh(mesh);
            let warnings: Vec<_> = gpu.take_validation_messages().into_iter().filter(|m| m.severity >= specula_gpu::Severity::Warning).collect();
            assert!(warnings.is_empty(), "adapter {}: {warnings:#?}", a.index);
        }
    }

    /// Reviewer finding (round 5): the overflow path counted every layer with contended atomics,
    /// so 12 full-view glass panes took 0.6 s at 720p on the iGPU and 16 at 2048² lost the
    /// device. Now nothing counts per fragment: 24 full-view layers at 2048² stay well under 2 s
    /// of GPU time, the 4 smallest ids are exact, the rest approximate lower bounds.
    #[test]
    fn many_glass_layers_stay_cheap() {
        use specula_core::math::{Mat4, Quat, Vec3};
        use specula_gpu::{DrawItem, FrameDesc, Gpu, GpuOptions, Material, Validation};
        if !specula_gpu::is_available() {
            eprintln!("many_glass_layers_stay_cheap: SKIPPED — Vulkan is not available");
            return;
        }
        for a in specula_gpu::enumerate_adapters().unwrap() {
            let options = GpuOptions { validation: Validation::On, adapter: specula_gpu::AdapterPreference::Index(a.index), ..Default::default() };
            let mut gpu = match Gpu::new(&options) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("many_glass_layers_stay_cheap: SKIPPED on adapter {} — {e}", a.index);
                    continue;
                }
            };
            if !gpu.ids_count_blended() {
                eprintln!("many_glass_layers_stay_cheap: SKIPPED on adapter {} — no fragment atomics", a.index);
                continue;
            }
            let (v, i) = sphere(32, 16);
            let mesh = gpu.upload_mesh(&v, &i).unwrap();
            let (w, h) = (2048u32, 2048u32);
            let target = gpu.create_target(w, h).unwrap();
            // The camera inside 24 concentric glass spheres: every pixel has 24 layers.
            let cam = crate::sight::Camera::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), 60.0);
            let vp = cam.view_proj(w, h).to_cols_array();
            let n = 24u32;
            let glass: Vec<DrawItem> = (0..n)
                .map(|k| {
                    let s = 4.0 + k as f32;
                    let model = Mat4::from_trs(Vec3::ZERO, Quat::IDENTITY, Vec3::new(s, s, s)).to_cols_array();
                    DrawItem { material: Material::Glass, ..DrawItem::new(mesh, model, [0.9, 0.9, 1.0, 0.1]) }
                })
                .collect();
            let ids: Vec<u32> = (1..=n).collect();
            let frame = FrameDesc { view_proj: vp, draws: &glass, reverse_z: true, ..Default::default() };
            gpu.render_with_ids(target, &frame, &ids).unwrap(); // warm-up (pipelines, buffers)
            gpu.render_with_ids(target, &frame, &ids).unwrap();
            let per_id = gpu.id_coverage(target, n).unwrap().per_id;
            // The ID passes' own GPU time (not wall-clock: the scene's glass costs about as much
            // again). Round 4 took seconds here or lost the device; ~0.13 s on a RaptorLake iGPU
            // now, so 2 s leaves room for older iGPUs. No timestamps (some devices): skip it.
            let stats = gpu.last_frame_stats_of(specula_gpu::FramePath::Offscreen).and_then(|s| s.gpu);
            if let Some(g) = stats {
                let ms: f64 = g.passes.iter().filter(|p| p.name == specula_gpu::PASS_ID_BUFFER).map(|p| p.gpu_ms).sum();
                assert!(ms < 2000.0, "adapter {}: {n} full-view layers at 2048²: ID passes took {ms:.0} ms of GPU time", a.index);
                eprintln!("many_glass_layers_stay_cheap: adapter {} ({}): ID passes {ms:.1} ms GPU", a.index, a.name);
            }
            for (k, c) in per_id.iter().enumerate() {
                if k < 4 {
                    assert_eq!((c.blended_pixels, c.approx), (u64::from(w * h), false), "adapter {}: id {}", a.index, k + 1);
                } else {
                    assert_eq!((c.blended_pixels, c.approx, c.rasterized), (0, true, Some(true)), "adapter {}: id {}", a.index, k + 1);
                }
            }
            gpu.destroy_target(target);
            gpu.destroy_mesh(mesh);
            let warnings: Vec<_> = gpu.take_validation_messages().into_iter().filter(|m| m.severity >= specula_gpu::Severity::Warning).collect();
            assert!(warnings.is_empty(), "adapter {}: {warnings:#?}", a.index);
        }
    }

    #[test]
    fn cube_faces_sit_on_their_normals() {
        let (v, _) = cube();
        for vert in v {
            let d: f32 = (0..3).map(|k| vert.position[k] * vert.normal[k]).sum();
            assert!((d - 0.5).abs() < 1e-6);
        }
    }
}
