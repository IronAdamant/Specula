//! Built-in shape meshes (object space, metres, centred on the origin).

use crate::world::Shape;
use specula_gpu::{Gpu, GpuError, MeshId, Vertex};
use std::f32::consts::{PI, TAU};

/// GPU mesh for each built-in shape.
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

    pub fn get(&self, shape: Shape) -> MeshId {
        match shape {
            Shape::Cube => self.cube,
            Shape::Quad => self.quad,
            Shape::Sphere => self.sphere,
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
    fn cube_faces_sit_on_their_normals() {
        let (v, _) = cube();
        for vert in v {
            let d: f32 = (0..3).map(|k| vert.position[k] * vert.normal[k]).sum();
            assert!((d - 0.5).abs() < 1e-6);
        }
    }
}
