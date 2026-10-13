//! Anim sets: one skeleton's clips (keyed or generated) and masks, shared by every creature
//! that uses the skeleton.
//!
//! ```text
//! { "anims": 1, "rig": "shambler",                       // the recipe whose rig this is
//!   "masks": { "upper": ["chest"] },                     // bones + their descendants
//!   "clips": [ { "name": "walk", "kind": "gait", ... },   // generator specs (see gen.rs)
//!              { "name": "flinch", "kind": "clip", "duration": 0.4, "tracks": [...] } ] }
//! ```

use specula_core::Json;

use crate::clip::{Channel, Clip};
use crate::error::{err, AnimError, AnimErrorCode, Obj};
use crate::gen;
use crate::skeleton::{BoneMask, Skeleton};

/// Index of a clip in its [`AnimSet`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClipId(pub u16);

/// Index of a mask in its [`AnimSet`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MaskId(pub u16);

/// The anim-set file format version.
pub const FORMAT_VERSION: u32 = 1;
pub const MAX_CLIPS: usize = 1024;
/// Most masks in one set.
pub const MAX_MASKS: usize = 64;

/// A clip resolved against the skeleton.
#[derive(Clone, Debug)]
pub(crate) struct Bound {
    pub clip: Clip,
    /// Skeleton bone of each track.
    pub bones: Vec<u16>,
    /// Index of the root bone's translation track, if any (root motion).
    pub root_t: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct AnimSet {
    pub skeleton: Skeleton,
    /// The recipe the skeleton came from (`rig` in the file), if given.
    pub rig: Option<String>,
    pub(crate) clips: Vec<Bound>,
    masks: Vec<(String, BoneMask)>,
}

impl AnimSet {
    pub fn new(skeleton: Skeleton) -> AnimSet {
        AnimSet { skeleton, rig: None, clips: Vec::new(), masks: Vec::new() }
    }

    /// Parses an anim-set file, generating every generator clip for `skeleton`.
    pub fn load(text: &str, skeleton: Skeleton) -> Result<AnimSet, AnimError> {
        let json = Json::parse(text).map_err(|e| {
            let mut er = err("", AnimErrorCode::Json, e.message.clone());
            er.offset = Some(e.offset);
            er
        })?;
        let mut o = Obj::new(&json, "", "an anim set")?;
        if o.req("anims")?.as_f64() != Some(FORMAT_VERSION as f64) {
            return Err(err("anims", AnimErrorCode::OutOfRange, format!("unsupported anim-set format (this build reads {FORMAT_VERSION})")));
        }
        let mut set = AnimSet::new(skeleton);
        set.rig = o.str("rig")?.map(str::to_string);
        if let Some(m) = o.get("masks") {
            let pairs = m.as_object().ok_or_else(|| err("masks", AnimErrorCode::WrongType, "expected {\"name\": [bone, ...]}"))?;
            if pairs.len() > MAX_MASKS {
                return Err(err("masks", AnimErrorCode::Limit, format!("{} masks; at most {MAX_MASKS}", pairs.len())).with_numbers(pairs.len() as f64, MAX_MASKS as f64));
            }
            for (name, bones) in pairs {
                let p = format!("masks.{name}");
                let list = bones.as_array().ok_or_else(|| err(&p, AnimErrorCode::WrongType, "expected a list of bone names"))?;
                let names: Vec<&str> = list.iter().enumerate().map(|(i, b)| b.as_str().ok_or_else(|| err(&format!("{p}[{i}]"), AnimErrorCode::WrongType, "expected a bone name"))).collect::<Result<_, _>>()?;
                set.add_mask(name, &names).map_err(|e| e.under(&p))?;
            }
        }
        let clips = o.arr("clips")?.ok_or_else(|| err("clips", AnimErrorCode::MissingField, "an anim set needs \"clips\""))?;
        if clips.len() > MAX_CLIPS {
            return Err(err("clips", AnimErrorCode::Limit, format!("{} clips; at most {MAX_CLIPS}", clips.len())));
        }
        for (i, c) in clips.iter().enumerate() {
            let p = format!("clips[{i}]");
            let kind = c.get("kind").and_then(|k| k.as_str()).unwrap_or("clip");
            let clip = if kind == "clip" { Clip::from_json(c, &p)? } else { gen::generate(c, &p, &set.skeleton)? };
            set.add(clip).map_err(|e| e.under(&p))?;
        }
        o.finish("anims, rig, masks, clips")?;
        Ok(set)
    }

    /// Adds a clip, resolving its tracks' bones. Names must be unique in the set.
    pub fn add(&mut self, clip: Clip) -> Result<ClipId, AnimError> {
        clip.validate()?;
        if self.clips.iter().any(|b| b.clip.name == clip.name) {
            return Err(err("name", AnimErrorCode::Duplicate, format!("the set already has a clip '{}'", clip.name)));
        }
        if self.clips.len() >= MAX_CLIPS {
            return Err(err("", AnimErrorCode::Limit, format!("at most {MAX_CLIPS} clips")));
        }
        let mut bones = Vec::with_capacity(clip.tracks.len());
        let mut root_t = None;
        for (i, t) in clip.tracks.iter().enumerate() {
            let b = self.skeleton.find(&t.bone).ok_or_else(|| {
                err(&format!("tracks[{i}].bone"), AnimErrorCode::UnknownBone, format!("no bone '{}' (bones: {})", t.bone, self.skeleton.names.join(", ")))
            })?;
            if b == 0 && t.channel == Channel::Translation {
                root_t = Some(i);
            }
            bones.push(b as u16);
        }
        self.clips.push(Bound { clip, bones, root_t });
        Ok(ClipId(self.clips.len() as u16 - 1))
    }

    /// Adds a mask covering `roots` and their descendants.
    pub fn add_mask(&mut self, name: &str, roots: &[&str]) -> Result<MaskId, AnimError> {
        if self.masks.len() >= MAX_MASKS {
            return Err(err("", AnimErrorCode::Limit, format!("at most {MAX_MASKS} masks")));
        }
        if roots.len() > specula_model::rig::MAX_BONES {
            return Err(err("", AnimErrorCode::Limit, format!("{} bones; a mask lists at most {}", roots.len(), specula_model::rig::MAX_BONES)));
        }
        if self.masks.iter().any(|(n, _)| n == name) {
            return Err(err("", AnimErrorCode::Duplicate, format!("a second mask '{name}'")));
        }
        let m = BoneMask::branches(&self.skeleton, roots)?;
        self.masks.push((name.to_string(), m));
        Ok(MaskId(self.masks.len() as u16 - 1))
    }

    pub fn id(&self, name: &str) -> Option<ClipId> {
        self.clips.iter().position(|b| b.clip.name == name).map(|i| ClipId(i as u16))
    }

    pub fn mask_id(&self, name: &str) -> Option<MaskId> {
        self.masks.iter().position(|(n, _)| n == name).map(|i| MaskId(i as u16))
    }

    pub fn mask(&self, id: MaskId) -> &BoneMask {
        &self.masks[id.0 as usize].1
    }

    pub fn clip(&self, id: ClipId) -> &Clip {
        &self.clips[id.0 as usize].clip
    }

    pub(crate) fn bound(&self, id: ClipId) -> &Bound {
        &self.clips[id.0 as usize]
    }

    /// Skeleton bone of each of the clip's tracks.
    pub fn track_bones(&self, id: ClipId) -> &[u16] {
        &self.clips[id.0 as usize].bones
    }

    pub fn len(&self) -> usize {
        self.clips.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }

    /// Every clip with its id, in insertion order.
    pub fn clips(&self) -> impl Iterator<Item = (ClipId, &Clip)> {
        self.clips.iter().enumerate().map(|(i, b)| (ClipId(i as u16), &b.clip))
    }
}
