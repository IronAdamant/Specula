//! Skeletons, poses, local → model transforms and skinning matrices.

use specula_model::Rig;

use crate::error::{err, AnimError, AnimErrorCode};
use crate::math::{self, compose, Affine, Transform, V3, QID};

/// Bones stored parents-first (bone 0 is the root), with their rest pose.
#[derive(Clone, Debug, PartialEq)]
pub struct Skeleton {
    pub names: Vec<String>,
    /// Parent index of each bone (`None` only for bone 0); always lower than the bone's index.
    pub parents: Vec<Option<u16>>,
    /// Rest local transform of each bone (relative to its parent).
    pub rest: Vec<Transform>,
    /// Rest model-space joint position (head) and segment end (tail) of each bone.
    pub heads: Vec<V3>,
    pub tails: Vec<V3>,
    /// Inverse of each bone's rest model-space transform (moves a bind-pose vertex into the
    /// bone's space).
    pub inverse_bind: Vec<Affine>,
}

/// Local transforms of every bone: **absolute** (rest included), indexed like the skeleton.
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub local: Vec<Transform>,
}

impl Skeleton {
    /// The skeleton of a recipe rig: rest frames aligned with the model axes, so each bone's
    /// rest local transform is the translation `head - parent.head`.
    pub fn from_rig(rig: &Rig) -> Result<Skeleton, AnimError> {
        rig.validate().map_err(|e| err("rig", AnimErrorCode::InvalidRig, e.to_string()))?;
        let n = rig.bones.len();
        let mut s = Skeleton {
            names: Vec::with_capacity(n),
            parents: Vec::with_capacity(n),
            rest: Vec::with_capacity(n),
            heads: Vec::with_capacity(n),
            tails: Vec::with_capacity(n),
            inverse_bind: Vec::with_capacity(n),
        };
        for b in &rig.bones {
            let base = b.parent.map_or([0.0; 3], |p| rig.bones[p].head);
            s.names.push(b.name.clone());
            s.parents.push(b.parent.map(|p| p as u16));
            s.rest.push(Transform { t: math::math3::sub(b.head, base), r: QID, s: [1.0; 3] });
            s.heads.push(b.head);
            s.tails.push(b.tail);
            s.inverse_bind.push(Affine::translate(math::math3::scale(b.head, -1.0)));
        }
        Ok(s)
    }

    /// A skeleton from explicit rest local transforms (e.g. an imported file). Fails on bad
    /// parents, non-finite or singular rest transforms.
    pub fn new(names: Vec<String>, parents: Vec<Option<u16>>, rest: Vec<Transform>) -> Result<Skeleton, AnimError> {
        let n = names.len();
        if n == 0 || n > specula_model::rig::MAX_BONES || parents.len() != n || rest.len() != n {
            return Err(err("", AnimErrorCode::InvalidRig, format!("{n} names, {} parents, {} rest transforms (1..={} bones)", parents.len(), rest.len(), specula_model::rig::MAX_BONES)));
        }
        for (i, p) in parents.iter().enumerate() {
            if (i == 0) != p.is_none() || p.is_some_and(|p| p as usize >= i) {
                return Err(err(&format!("bones[{i}]"), AnimErrorCode::InvalidRig, "bone 0 is the only root and parents come before children"));
            }
        }
        let mut model: Vec<Affine> = Vec::with_capacity(n);
        let mut s = Skeleton { names, parents, rest, heads: Vec::new(), tails: Vec::new(), inverse_bind: Vec::new() };
        for i in 0..n {
            let local = s.rest[i].to_affine();
            let m = match s.parents[i] {
                Some(p) => compose(&model[p as usize], &local),
                None => local,
            };
            let inv = math::inverse(&m).ok_or_else(|| err(&format!("bones[{i}]"), AnimErrorCode::InvalidRig, "singular rest transform"))?;
            if m.t.iter().any(|v| !v.is_finite()) {
                return Err(err(&format!("bones[{i}]"), AnimErrorCode::InvalidRig, "non-finite rest transform"));
            }
            s.heads.push(m.t);
            s.inverse_bind.push(inv);
            model.push(m);
        }
        s.tails = s.heads.clone();
        Ok(s)
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    pub fn rest_pose(&self) -> Pose {
        Pose { local: self.rest.clone() }
    }

    /// True if every rest rotation is the identity (recipe rigs): generators rely on it.
    pub fn model_aligned(&self) -> bool {
        self.rest.iter().all(|t| t.r == QID && t.s == [1.0; 3])
    }

    /// `out[j]` = bone j's model-space transform (parents-first single pass).
    pub fn model_space(&self, pose: &Pose, out: &mut Vec<Affine>) {
        out.clear();
        for (i, l) in pose.local.iter().enumerate() {
            let local = l.to_affine();
            let m = match self.parents[i] {
                Some(p) => compose(&out[p as usize], &local),
                None => local,
            };
            out.push(m);
        }
    }

    /// `out[j] = model[j] · inverse_bind[j]`: moves bind-pose vertices to the pose.
    pub fn skin_matrices(&self, model: &[Affine], out: &mut Vec<Affine>) {
        out.clear();
        out.extend(model.iter().zip(&self.inverse_bind).map(|(m, ib)| compose(m, ib)));
    }

    /// True if bone `b` is `ancestor` or one of its descendants.
    pub fn is_under(&self, b: usize, ancestor: usize) -> bool {
        let mut k = Some(b as u16);
        while let Some(i) = k {
            if i as usize == ancestor {
                return true;
            }
            k = self.parents[i as usize];
        }
        false
    }
}

/// Column-major 4×4 matrices, 16 floats per bone, for a GPU storage buffer (std430 `mat4[]`,
/// one region per creature instance).
pub fn write_gpu_matrices(skin: &[Affine], out: &mut Vec<f32>) {
    out.clear();
    for m in skin {
        out.extend_from_slice(&m.to_cols_array());
    }
}

/// Per-bone weights 0..1 that restrict a layer or blend to part of the body.
#[derive(Clone, Debug, PartialEq)]
pub struct BoneMask {
    pub weights: Vec<f32>,
}

impl BoneMask {
    /// Weight 1 for the named bones and all their descendants, 0 elsewhere.
    pub fn branches(skel: &Skeleton, roots: &[&str]) -> Result<BoneMask, AnimError> {
        let mut idx = Vec::new();
        for (i, r) in roots.iter().enumerate() {
            idx.push(skel.find(r).ok_or_else(|| err(&format!("[{i}]"), AnimErrorCode::UnknownBone, format!("no bone '{r}' (bones: {})", skel.names.join(", "))))?);
        }
        Ok(BoneMask { weights: (0..skel.len()).map(|b| if idx.iter().any(|&r| skel.is_under(b, r)) { 1.0 } else { 0.0 }).collect() })
    }
}
