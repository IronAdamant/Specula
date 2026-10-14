//! Rigs: a bone tree built with a recipe, and how mesh vertices bind to it (skinning data).
//!
//! A recipe's optional `rig` section declares bones in model space (pivot `head`, optional
//! `tail`); parts name the bones they may bind to. Every vertex binds to the **nearest bone
//! segment** among its part's bones, so mirrored or arrayed parts (both legs in one part) bind
//! each copy to its own bone, and vertices near a joint can blend between a bone and its parent
//! or child (`blend`, metres). Binding is a pure function of the vertex position, so split
//! vertices (hard edges, colour seams) at one position always move together: skinning can't open
//! cracks.
//!
//! Rest frames are **aligned with the model axes** (no rest rotation): a bone's rest local
//! transform is a translation `head - parent.head`. Animation rotations are therefore about the
//! model's own axes in the rest pose: +X rotation pitches a limb forward-down (knees, nods, jaw
//! open), Y yaws, Z rolls (see `specula-anim`).

use specula_core::hash::StableHasher;
use specula_core::Json;

use crate::math3::{self, V3};

/// Most bones a rig may have: joint indices are `u8`. How many bones a creature *should* have is a
/// performance budget (docs/design/ANIMATION.md: e.g. 64 for crowd NPCs), not this limit.
pub const MAX_BONES: usize = 255;

/// One bone, in the model's rest pose.
#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    pub name: String,
    /// Index of the parent bone; always lower than this bone's index. `None` only for bone 0.
    pub parent: Option<usize>,
    /// Pivot (joint position) in model space, metres.
    pub head: V3,
    /// End of the bone segment used for binding; equal to `head` for a point bone.
    pub tail: V3,
}

/// A bone tree stored parents-first (bone 0 is the root), so a single forward pass computes
/// model-space transforms.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rig {
    pub bones: Vec<Bone>,
}

/// Why a rig is invalid.
#[derive(Clone, Debug, PartialEq)]
pub struct RigError {
    /// Bone index involved, if any.
    pub bone: Option<usize>,
    pub message: String,
}

impl std::fmt::Display for RigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.bone {
            Some(b) => write!(f, "bone {b}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for RigError {}

impl Rig {
    pub fn len(&self) -> usize {
        self.bones.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bones.is_empty()
    }

    /// Index of the bone called `name`.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }

    /// Checks the structural rules: 1..=[`MAX_BONES`] bones, unique names, bone 0 the only
    /// root, parents before children, finite positions.
    pub fn validate(&self) -> Result<(), RigError> {
        let bad = |bone: Option<usize>, message: String| Err(RigError { bone, message });
        if self.bones.is_empty() || self.bones.len() > MAX_BONES {
            return bad(None, format!("{} bones; a rig has 1..={MAX_BONES}", self.bones.len()));
        }
        for (i, b) in self.bones.iter().enumerate() {
            if self.bones[..i].iter().any(|o| o.name == b.name) {
                return bad(Some(i), format!("duplicate bone name '{}'", b.name));
            }
            match (i, b.parent) {
                (0, None) => {}
                (0, Some(_)) => return bad(Some(0), "bone 0 is the root and has no parent".into()),
                (_, None) => return bad(Some(i), format!("'{}' has no parent; only bone 0 may be a root", b.name)),
                (_, Some(p)) if p >= i => return bad(Some(i), format!("'{}' has parent {p}, which is not an earlier bone", b.name)),
                _ => {}
            }
            if b.head.iter().chain(b.tail.iter()).any(|v| !v.is_finite()) {
                return bad(Some(i), format!("'{}' has a non-finite head or tail", b.name));
            }
        }
        Ok(())
    }

    /// True if `a` is `b`'s parent or child.
    pub fn adjacent(&self, a: usize, b: usize) -> bool {
        self.bones[a].parent == Some(b) || self.bones[b].parent == Some(a)
    }

    /// Distance from `p` to bone `i`'s segment (head → tail).
    pub fn distance(&self, i: usize, p: V3) -> f32 {
        let b = &self.bones[i];
        let ab = math3::sub(b.tail, b.head);
        let l2 = math3::dot(ab, ab);
        let t = if l2 > 1e-12 { (math3::dot(math3::sub(p, b.head), ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
        math3::length(math3::sub(p, math3::add(b.head, math3::scale(ab, t))))
    }

    /// Skinning data for `positions`: each vertex binds to the nearest of `allowed` bones (ties go
    /// to the earlier entry); within `blend` metres of an adjacent (parent/child) allowed bone it
    /// shares its weight with it, 50/50 exactly where both are equally near. An empty `allowed`
    /// binds everything to the root. Weights are unorm8 summing to exactly 255.
    pub fn bind(&self, positions: &[V3], allowed: &[usize], blend: f32) -> (Vec<[u8; 4]>, Vec<[u8; 4]>) {
        let root = [0usize];
        let allowed = if allowed.is_empty() { &root[..] } else { allowed };
        let mut joints = Vec::with_capacity(positions.len());
        let mut weights = Vec::with_capacity(positions.len());
        let mut d = vec![0.0f32; allowed.len()];
        for &p in positions {
            for (k, &b) in allowed.iter().enumerate() {
                d[k] = self.distance(b, p);
            }
            let mut best = 0;
            for k in 1..allowed.len() {
                if d[k] < d[best] {
                    best = k;
                }
            }
            let b0 = allowed[best];
            let mut second: Option<usize> = None;
            for k in 0..allowed.len() {
                if k != best && self.adjacent(b0, allowed[k]) && second.is_none_or(|s| d[k] < d[s]) {
                    second = Some(k);
                }
            }
            let mut j = [b0 as u8, 0, 0, 0];
            let mut w = [255u8, 0, 0, 0];
            if let (Some(s), true) = (second, blend > 0.0) {
                let gap = d[s] - d[best];
                if gap < blend {
                    // 0.5 at equal distance, rising linearly to 1 at `blend`.
                    let w0 = 0.5 + 0.5 * gap / blend;
                    let q0 = crate::dmath::round(w0 * 255.0).clamp(128.0, 255.0) as u8;
                    if q0 < 255 {
                        j[1] = allowed[s] as u8;
                        w = [q0, 255 - q0, 0, 0];
                    }
                }
            }
            joints.push(j);
            weights.push(w);
        }
        (joints, weights)
    }

    /// Feeds every bone (name, parent, head and tail bits) into `h`.
    pub fn hash_into(&self, h: &mut StableHasher) {
        h.write_u64(self.bones.len() as u64);
        for b in &self.bones {
            h.write_str(&b.name);
            h.write_u64(b.parent.map_or(u64::MAX, |p| p as u64));
            b.head.iter().chain(b.tail.iter()).for_each(|v| h.write_f32(*v));
        }
    }

    /// A bounded summary for tool responses: bone names, parents and heads (at most
    /// [`MAX_BONES`] entries).
    pub fn report(&self) -> Json {
        let v3 = |v: V3| Json::arr(v.iter().map(|x| Json::Num((*x as f64 * 1000.0).round() / 1000.0)));
        Json::arr(self.bones.iter().map(|b| {
            Json::obj([
                ("name", Json::Str(b.name.clone())),
                ("parent", b.parent.map_or(Json::Null, |p| Json::Str(self.bones[p].name.clone()))),
                ("head", v3(b.head)),
                ("tail", v3(b.tail)),
            ])
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leg() -> Rig {
        let bone = |name: &str, parent: Option<usize>, head: V3, tail: V3| Bone { name: name.into(), parent, head, tail };
        Rig {
            bones: vec![
                bone("root", None, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
                bone("thigh", Some(0), [0.0, 1.0, 0.0], [0.0, 0.5, 0.0]),
                bone("shin", Some(1), [0.0, 0.5, 0.0], [0.0, 0.0, 0.0]),
                bone("other", Some(0), [1.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
            ],
        }
    }

    #[test]
    fn validation_rules() {
        let mut r = leg();
        r.validate().unwrap();
        r.bones[2].parent = Some(3);
        assert_eq!(r.validate().unwrap_err().bone, Some(2));
        let mut r = leg();
        r.bones[3].name = "thigh".into();
        assert!(r.validate().unwrap_err().message.contains("duplicate"));
        let mut r = leg();
        r.bones[1].parent = None;
        assert!(r.validate().is_err());
        let mut r = leg();
        r.bones[1].head[0] = f32::NAN;
        assert!(r.validate().is_err());
        assert!(Rig::default().validate().is_err());
    }

    #[test]
    fn rigid_and_blended_binding() {
        let r = leg();
        let pts = [[0.05, 0.9, 0.0], [0.05, 0.1, 0.0], [0.05, 0.5, 0.0], [0.9, 0.5, 0.0]];
        // Rigid: nearest segment wins, full weight.
        let (j, w) = r.bind(&pts, &[1, 2, 3], 0.0);
        assert_eq!(j.iter().map(|j| j[0]).collect::<Vec<_>>(), vec![1, 2, 1, 3]);
        assert!(w.iter().all(|w| *w == [255, 0, 0, 0]));
        // Blended: the knee point is equally near thigh and shin → 50/50 (128/127); far points rigid.
        let (j, w) = r.bind(&pts, &[1, 2, 3], 0.2);
        assert_eq!((j[2], w[2]), ([1, 2, 0, 0], [128, 127, 0, 0]));
        assert_eq!(w[0], [255, 0, 0, 0]);
        // Never blends between unrelated bones (thigh and `other` are siblings' children).
        assert_eq!(w[3], [255, 0, 0, 0]);
        for w in &w {
            assert_eq!(w.iter().map(|x| *x as u32).sum::<u32>(), 255);
        }
        // No bones listed → root.
        let (j, _) = r.bind(&pts, &[], 0.1);
        assert!(j.iter().all(|j| j[0] == 0));
    }
}
