//! Render-ready triangle meshes: the output of a recipe build.

use std::collections::HashMap;
use std::fmt;

use specula_core::hash::StableHasher;

use crate::math3::{self, Affine, V3};

/// A contiguous range of [`MeshData::indices`] drawn with one material preset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    /// Material preset name from the game's style (e.g. `carved_stone`, `rune_glow`).
    pub material: String,
    /// First index (a multiple of 3) into [`MeshData::indices`].
    pub first_index: u32,
    pub index_count: u32,
}

/// An indexed triangle mesh, ready to upload.
///
/// - `positions` in metres (right-handed, +Y up). Recipe assets rest on y = 0, centred in XZ.
/// - `normals` unit length; hard edges are split vertices.
/// - `colors` are **sRGB, 8 bits per channel, alpha linear** (`[r, g, b, a]`), matching the
///   engine's rule that colours given to APIs are sRGB. 4 bytes per vertex keeps memory low (F3);
///   the GPU decodes sRGB → linear for free with an `R8G8B8A8_SRGB` attribute (or in the shader).
/// - Triangles are counter-clockwise seen from outside.
/// - `parts` cover `indices` exactly, in order, one per material.
/// - `joints`/`weights`: skinning data for rigged models (see [`crate::rig`]), one entry per
///   vertex, or both empty. Up to 4 bone indices into the model's rig, and unorm8 weights that
///   sum to exactly 255 (a GPU reads them as `R8G8B8A8_UINT` + `R8G8B8A8_UNORM`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshData {
    pub positions: Vec<V3>,
    pub normals: Vec<V3>,
    pub colors: Vec<[u8; 4]>,
    pub indices: Vec<u32>,
    pub parts: Vec<Part>,
    pub joints: Vec<[u8; 4]>,
    pub weights: Vec<[u8; 4]>,
}

/// Axis-aligned bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: V3,
    pub max: V3,
}

impl Bounds {
    pub const EMPTY: Bounds = Bounds { min: [f32::MAX; 3], max: [f32::MIN; 3] };

    pub fn of(points: &[V3]) -> Bounds {
        let mut b = Bounds::EMPTY;
        for &p in points {
            // Canonical zeros: min/max of ±0.0 is unspecified across platforms.
            let p = crate::shape::canon(p);
            b.min = math3::vmin(b.min, p);
            b.max = math3::vmax(b.max, p);
        }
        b
    }

    pub fn is_empty(&self) -> bool {
        self.min[0] > self.max[0]
    }

    pub fn size(&self) -> V3 {
        math3::sub(self.max, self.min)
    }

    pub fn center(&self) -> V3 {
        math3::scale(math3::add(self.min, self.max), 0.5)
    }

    pub fn union(&self, o: &Bounds) -> Bounds {
        Bounds { min: math3::vmin(self.min, o.min), max: math3::vmax(self.max, o.max) }
    }
}

/// What a mesh check found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshErrorCode {
    /// Buffer lengths disagree (normals/colours vs positions, indices not a multiple of 3).
    LengthMismatch,
    /// A position or normal is NaN or infinite.
    NonFinite,
    /// An index points past the vertex buffer.
    IndexOutOfRange,
    /// A triangle repeats a vertex or has (near) zero area.
    DegenerateTriangle,
    /// A normal isn't unit length.
    BadNormal,
    /// Parts don't cover the index buffer exactly, in order.
    BadParts,
    /// Skinning data has the wrong length, or weights that don't sum to 255.
    BadSkin,
}

/// A failed mesh check: which rule, which element, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshError {
    pub code: MeshErrorCode,
    /// Vertex or triangle index involved, if any.
    pub element: Option<usize>,
    pub message: String,
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for MeshError {}

fn err(code: MeshErrorCode, element: Option<usize>, message: String) -> MeshError {
    MeshError { code, element, message }
}

/// Smallest triangle area (m²) accepted as non-degenerate.
pub const MIN_TRIANGLE_AREA: f32 = 1e-10;

impl MeshData {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn bounds(&self) -> Bounds {
        Bounds::of(&self.positions)
    }

    /// Triangle `t`'s three positions.
    pub fn triangle(&self, t: usize) -> [V3; 3] {
        let i = &self.indices[3 * t..3 * t + 3];
        [self.positions[i[0] as usize], self.positions[i[1] as usize], self.positions[i[2] as usize]]
    }

    /// Checks every structural rule; the first violation is returned with its element index.
    pub fn validate(&self) -> Result<(), MeshError> {
        let n = self.positions.len();
        if self.normals.len() != n || self.colors.len() != n || self.indices.len() % 3 != 0 {
            return Err(err(
                MeshErrorCode::LengthMismatch,
                None,
                format!(
                    "{n} positions, {} normals, {} colours, {} indices",
                    self.normals.len(),
                    self.colors.len(),
                    self.indices.len()
                ),
            ));
        }
        if self.joints.len() != self.weights.len() || !(self.joints.is_empty() || self.joints.len() == n) {
            return Err(err(MeshErrorCode::BadSkin, None, format!("{n} positions, {} joints, {} weights", self.joints.len(), self.weights.len())));
        }
        if let Some(i) = self.weights.iter().position(|w| w.iter().map(|x| *x as u32).sum::<u32>() != 255) {
            return Err(err(MeshErrorCode::BadSkin, Some(i), format!("vertex {i}: weights {:?} don't sum to 255", self.weights[i])));
        }
        for (i, (p, nn)) in self.positions.iter().zip(&self.normals).enumerate() {
            if p.iter().chain(nn.iter()).any(|v| !v.is_finite()) {
                return Err(err(MeshErrorCode::NonFinite, Some(i), format!("vertex {i}: {p:?} normal {nn:?}")));
            }
            let l = math3::length(*nn);
            if (l - 1.0).abs() > 1e-3 {
                return Err(err(MeshErrorCode::BadNormal, Some(i), format!("vertex {i}: normal length {l}")));
            }
        }
        for t in 0..self.triangle_count() {
            let i = &self.indices[3 * t..3 * t + 3];
            if let Some(&bad) = i.iter().find(|&&v| v as usize >= n) {
                return Err(err(MeshErrorCode::IndexOutOfRange, Some(t), format!("triangle {t}: index {bad} ≥ {n} vertices")));
            }
            if i[0] == i[1] || i[1] == i[2] || i[0] == i[2] {
                return Err(err(MeshErrorCode::DegenerateTriangle, Some(t), format!("triangle {t} repeats a vertex: {i:?}")));
            }
            let [a, b, c] = self.triangle(t);
            let area = 0.5 * math3::length(math3::tri_normal(a, b, c));
            if area.is_nan() || area < MIN_TRIANGLE_AREA {
                return Err(err(MeshErrorCode::DegenerateTriangle, Some(t), format!("triangle {t} has area {area:e} m²")));
            }
        }
        let mut next = 0u32;
        for (k, p) in self.parts.iter().enumerate() {
            if p.first_index != next || p.index_count % 3 != 0 || p.index_count == 0 {
                return Err(err(MeshErrorCode::BadParts, Some(k), format!("part {k} ({}) range {}+{}", p.material, p.first_index, p.index_count)));
            }
            next += p.index_count;
        }
        if next as usize != self.indices.len() {
            return Err(err(MeshErrorCode::BadParts, None, format!("parts cover {next} of {} indices", self.indices.len())));
        }
        Ok(())
    }

    /// Applies `t` to positions and normals; mirrors flip the winding so faces stay outward.
    pub fn transform(&mut self, t: &Affine) {
        for p in &mut self.positions {
            *p = t.apply_point(*p);
        }
        for n in &mut self.normals {
            *n = t.apply_normal(*n);
        }
        if t.determinant() < 0.0 {
            for tri in self.indices.chunks_exact_mut(3) {
                tri.swap(1, 2);
            }
        }
    }

    /// True if the mesh carries skinning data.
    pub fn is_skinned(&self) -> bool {
        !self.joints.is_empty()
    }

    /// Appends `other` (its parts follow ours). Call [`MeshData::group_by_material`] to merge
    /// parts that share a material. If only one side is skinned, the other side's vertices are
    /// bound to bone 0 so the buffers stay aligned.
    pub fn merge(&mut self, other: &MeshData) {
        let base = self.positions.len() as u32;
        if self.is_skinned() || other.is_skinned() {
            let rigid = ([0u8; 4], [255u8, 0, 0, 0]);
            if !self.is_skinned() {
                self.joints = vec![rigid.0; self.positions.len()];
                self.weights = vec![rigid.1; self.positions.len()];
            }
            if other.is_skinned() {
                self.joints.extend_from_slice(&other.joints);
                self.weights.extend_from_slice(&other.weights);
            } else {
                self.joints.extend(std::iter::repeat_n(rigid.0, other.positions.len()));
                self.weights.extend(std::iter::repeat_n(rigid.1, other.positions.len()));
            }
        }
        let ibase = self.indices.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.colors.extend_from_slice(&other.colors);
        self.indices.extend(other.indices.iter().map(|i| i + base));
        self.parts.extend(other.parts.iter().map(|p| Part { first_index: p.first_index + ibase, ..p.clone() }));
    }

    /// Reorders triangles so each material is one contiguous part (fewer draws), in order of the
    /// material's first appearance. Stable within a material.
    pub fn group_by_material(&mut self) {
        let mut order: Vec<String> = Vec::new();
        for p in &self.parts {
            if !order.contains(&p.material) {
                order.push(p.material.clone());
            }
        }
        let mut indices = Vec::with_capacity(self.indices.len());
        let mut parts = Vec::with_capacity(order.len());
        for m in &order {
            let first = indices.len() as u32;
            for p in self.parts.iter().filter(|p| &p.material == m) {
                let r = p.first_index as usize..(p.first_index + p.index_count) as usize;
                indices.extend_from_slice(&self.indices[r]);
            }
            let count = indices.len() as u32 - first;
            if count > 0 {
                parts.push(Part { material: m.clone(), first_index: first, index_count: count });
            }
        }
        self.indices = indices;
        self.parts = parts;
    }

    /// Volume enclosed (positive when faces point outward; only meaningful for closed meshes).
    pub fn signed_volume(&self) -> f32 {
        let mut v = 0.0f64;
        for t in 0..self.triangle_count() {
            let [a, b, c] = self.triangle(t);
            v += math3::dot(a, math3::cross(b, c)) as f64;
        }
        (v / 6.0) as f32
    }

    /// Open edges after welding equal positions: 0 for a watertight (closed, 2-manifold-edged)
    /// mesh. Counts edges not matched by exactly one opposite edge.
    pub fn open_edge_count(&self) -> usize {
        // Weld by exact position bits so split normals don't count as seams.
        let mut ids: HashMap<[u32; 3], u32> = HashMap::new();
        let weld: Vec<u32> = self
            .positions
            .iter()
            .map(|p| {
                let c = crate::shape::canon(*p);
                let key = [c[0].to_bits(), c[1].to_bits(), c[2].to_bits()];
                let next = ids.len() as u32;
                *ids.entry(key).or_insert(next)
            })
            .collect();
        let mut edges: HashMap<(u32, u32), i32> = HashMap::new();
        for tri in self.indices.chunks_exact(3) {
            for k in 0..3 {
                let (a, b) = (weld[tri[k] as usize], weld[tri[(k + 1) % 3] as usize]);
                *edges.entry((a, b)).or_insert(0) += 1;
            }
        }
        let mut open = 0;
        // Order doesn't matter: we only count.
        for (&(a, b), &n) in &edges {
            let back = edges.get(&(b, a)).copied().unwrap_or(0);
            if n != 1 || back != 1 {
                open += 1;
            }
        }
        open
    }

    pub fn is_watertight(&self) -> bool {
        self.open_edge_count() == 0
    }

    /// A stable 64-bit hash of every buffer (exact float bits): the same recipe and seed must
    /// give the same hash on every platform.
    pub fn content_hash(&self) -> u64 {
        let mut h = StableHasher::new();
        h.write_u64(self.positions.len() as u64);
        for p in &self.positions {
            p.iter().for_each(|v| h.write_f32(*v));
        }
        for n in &self.normals {
            n.iter().for_each(|v| h.write_f32(*v));
        }
        for c in &self.colors {
            h.write_bytes(c);
        }
        h.write_u64(self.indices.len() as u64);
        for i in &self.indices {
            h.write_u32(*i);
        }
        for p in &self.parts {
            h.write_str(&p.material);
            h.write_u32(p.first_index);
            h.write_u32(p.index_count);
        }
        // Unskinned meshes hash exactly as they did before skinning existed.
        if self.is_skinned() {
            h.write_u64(self.joints.len() as u64);
            for (j, w) in self.joints.iter().zip(&self.weights) {
                h.write_bytes(j);
                h.write_bytes(w);
            }
        }
        h.finish()
    }

    /// Indices of one part.
    pub fn part_indices(&self, part: usize) -> &[u32] {
        let p = &self.parts[part];
        &self.indices[p.first_index as usize..(p.first_index + p.index_count) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tetra() -> MeshData {
        let positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]];
        // Outward CCW.
        let indices = vec![0, 2, 1, 0, 1, 3, 1, 2, 3, 2, 0, 3];
        MeshData {
            normals: vec![[0.0, 1.0, 0.0]; 4],
            colors: vec![[255; 4]; 4],
            positions,
            indices,
            parts: vec![Part { material: "a".into(), first_index: 0, index_count: 12 }],
            ..MeshData::default()
        }
    }

    #[test]
    fn tetra_is_valid_closed_and_outward() {
        let t = tetra();
        t.validate().unwrap();
        assert!(t.is_watertight());
        assert!((t.signed_volume() - 1.0 / 6.0).abs() < 1e-6);
    }

    #[test]
    fn validation_finds_problems() {
        let mut t = tetra();
        t.indices[0] = 9;
        assert_eq!(t.validate().unwrap_err().code, MeshErrorCode::IndexOutOfRange);
        let mut t = tetra();
        t.positions[3] = [0.0, 0.0, 0.0];
        assert_eq!(t.validate().unwrap_err().code, MeshErrorCode::DegenerateTriangle);
        let mut t = tetra();
        t.positions[1][0] = f32::NAN;
        assert_eq!(t.validate().unwrap_err().code, MeshErrorCode::NonFinite);
        let mut t = tetra();
        t.parts[0].index_count = 9;
        assert_eq!(t.validate().unwrap_err().code, MeshErrorCode::BadParts);
        let mut t = tetra();
        t.indices.truncate(9);
        t.parts[0].index_count = 9;
        assert!(!t.is_watertight());
    }

    #[test]
    fn mirror_transform_keeps_outward_winding() {
        let mut t = tetra();
        t.transform(&Affine::scale([-1.0, 1.0, 1.0]));
        assert!(t.signed_volume() > 0.0);
    }

    #[test]
    fn merge_and_group() {
        let mut a = tetra();
        let mut b = tetra();
        b.parts[0].material = "b".into();
        let c = tetra();
        a.merge(&b);
        a.merge(&c);
        assert_eq!(a.parts.len(), 3);
        a.group_by_material();
        assert_eq!(a.parts.len(), 2);
        assert_eq!(a.parts[0].index_count, 24);
        a.validate().unwrap();
        assert_ne!(a.content_hash(), tetra().content_hash());
    }

    #[test]
    fn merge_keeps_skin_aligned() {
        let plain = tetra();
        let mut skinned = tetra();
        skinned.joints = vec![[3, 1, 0, 0]; 4];
        skinned.weights = vec![[200, 55, 0, 0]; 4];
        skinned.validate().unwrap();
        let mut a = plain.clone();
        a.merge(&skinned);
        a.merge(&plain);
        a.group_by_material();
        a.validate().unwrap();
        assert_eq!(a.joints.len(), 12);
        assert_eq!(a.joints[0], [0, 0, 0, 0]);
        assert_eq!(a.joints[4], [3, 1, 0, 0]);
        assert_eq!(a.weights[8], [255, 0, 0, 0]);
        assert_ne!(skinned.content_hash(), plain.content_hash());
        skinned.weights[2] = [200, 0, 0, 0];
        assert_eq!(skinned.validate().unwrap_err().code, MeshErrorCode::BadSkin);
        skinned.weights.pop();
        assert_eq!(skinned.validate().unwrap_err().code, MeshErrorCode::BadSkin);
    }
}
