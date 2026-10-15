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
    /// Moves whenever the profile → mesh map may have changed (see [`LatheMeshes::version`]).
    version: u64,
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
        self.version += 1;
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

    /// Unchanged means every profile still maps to the same mesh (draw lists cached across
    /// frames key on it).
    pub fn version(&self) -> u64 {
        self.version
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

    /// Meshes uploaded elsewhere (tests: ids that draw lists carry but nothing renders).
    pub fn from_ids(cube: MeshId, quad: MeshId, sphere: MeshId) -> ShapeMeshes {
        ShapeMeshes { cube, quad, sphere }
    }

    /// `None` for lathes.
    pub fn get(&self, shape: Shape) -> Option<MeshId> {
        match shape {
            Shape::Cube => Some(self.cube),
            Shape::Quad => Some(self.quad),
            Shape::Sphere => Some(self.sphere),
            Shape::Lathe | Shape::Model | Shape::Group => None,
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

    #[test]
    fn cube_faces_sit_on_their_normals() {
        let (v, _) = cube();
        for vert in v {
            let d: f32 = (0..3).map(|k| vert.position[k] * vert.normal[k]).sum();
            assert!((d - 0.5).abs() < 1e-6);
        }
    }
}
