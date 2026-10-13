//! CPU linear-blend skinning of [`MeshData`]: previews, tests and a fallback. The GPU path
//! takes the same skinning matrices (see `skeleton::write_gpu_matrices`) and the mesh's
//! `joints`/`weights` as vertex attributes.

use specula_model::MeshData;

use crate::math::{math3, Affine, V3};

/// Skins `mesh` by `skin` matrices (one per bone): positions and normals are moved, everything
/// else is copied. An unskinned mesh is copied unchanged.
pub fn skin_mesh(mesh: &MeshData, skin: &[Affine]) -> MeshData {
    let mut out = mesh.clone();
    skin_into(mesh, skin, &mut out.positions, &mut out.normals);
    out
}

/// Writes skinned positions and normals of `mesh` into `positions`/`normals` (resized).
pub fn skin_into(mesh: &MeshData, skin: &[Affine], positions: &mut Vec<V3>, normals: &mut Vec<V3>) {
    positions.clear();
    normals.clear();
    if !mesh.is_skinned() {
        positions.extend_from_slice(&mesh.positions);
        normals.extend_from_slice(&mesh.normals);
        return;
    }
    for i in 0..mesh.positions.len() {
        let (j, w) = (mesh.joints[i], mesh.weights[i]);
        let m = if w[0] == 255 {
            skin[j[0] as usize]
        } else {
            let mut m = Affine { cols: [[0.0; 3]; 3], t: [0.0; 3] };
            for k in 0..4 {
                if w[k] == 0 {
                    continue;
                }
                let s = &skin[j[k] as usize];
                let f = w[k] as f32 * (1.0 / 255.0);
                for c in 0..3 {
                    m.cols[c] = math3::add(m.cols[c], math3::scale(s.cols[c], f));
                }
                m.t = math3::add(m.t, math3::scale(s.t, f));
            }
            m
        };
        positions.push(m.apply_point(mesh.positions[i]));
        normals.push(m.apply_normal(mesh.normals[i]));
    }
}
