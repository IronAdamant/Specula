//! Built-in shape meshes (object space, metres, centred on the origin).

use crate::world::Shape;
use specula_gpu::{Gpu, GpuError, MeshId, Vertex};
use std::f32::consts::{PI, TAU};

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
            Shape::Lathe | Shape::Group => None,
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
            vertices.push(Vertex { position: [p(0), p(1), p(2)], normal: n });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (vertices, indices)
}

/// 1×1 plane in XZ facing +Y.
pub fn quad() -> (Vec<Vertex>, Vec<u32>) {
    let n = [0.0, 1.0, 0.0];
    let vertices = [[-0.5, 0.0, 0.5], [0.5, 0.0, 0.5], [0.5, 0.0, -0.5], [-0.5, 0.0, -0.5]]
        .map(|position| Vertex { position, normal: n })
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
            vertices.push(Vertex { position: n.map(|c| c * 0.5), normal: n });
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
            vertices.push(Vertex { position: [p[0] * c, p[1], p[0] * s], normal: [n[0] * c, n[1], n[0] * s] });
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
        vertices.push(Vertex { position: [0.0, p[1], 0.0], normal: [0.0, n[1], 0.0] });
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
