//! Anim sets: one skeleton's clips (keyed or generated), masks and animation graphs, shared by
//! every creature that uses the skeleton.
//!
//! ```text
//! { "anims": 1, "rig": "shambler",                       // the recipe whose rig this is
//!   "masks": { "upper": ["chest"] },                     // bones + their descendants
//!   "clips": [ { "name": "walk", "kind": "gait", ... },   // generator specs (see gen.rs)
//!              { "name": "flinch", "kind": "clip", "duration": 0.4, "tracks": [...] } ],
//!   "graphs": { "creature": { "params": ..., "states": ..., "transitions": ... } } }  // graph.rs
//! ```
//!
//! Clips come first (generators may build on earlier clips, e.g. secondary motion over a walk),
//! then graphs, bound to the clips by name. [`AnimSet::with_overlay`] layers clips and graphs
//! written elsewhere (an agent's, kept in the world) over a set: same-named ones replace, new
//! ones are added, and every graph is bound again.

use specula_core::Json;

use crate::clip::{Channel, Clip};
use crate::error::{err, is_ident, AnimError, AnimErrorCode, Obj};
use crate::gen;
use crate::graph::{self, AnimGraph};
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
/// Most graphs in one set.
pub const MAX_GRAPHS: usize = 32;

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
    /// Graphs by name, bound to the clips, with the JSON they came from (rebound by overlays).
    graphs: Vec<(String, AnimGraph)>,
    /// Generation work its generated clips took so far (frames × bones; see
    /// [`gen::MAX_SET_WORK`]).
    pub(crate) generated: u64,
}

/// One clip or graph an overlay could not use (the set's own stays).
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayProblem {
    /// `clip` or `graph`.
    pub kind: &'static str,
    pub name: String,
    pub error: AnimError,
}

impl AnimSet {
    pub fn new(skeleton: Skeleton) -> AnimSet {
        AnimSet { skeleton, rig: None, clips: Vec::new(), masks: Vec::new(), graphs: Vec::new(), generated: 0 }
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
            let clip = set.make_clip(c, &p)?;
            set.add(clip).map_err(|e| e.under(&p))?;
        }
        if let Some(g) = o.get("graphs") {
            let pairs = g.as_object().ok_or_else(|| err("graphs", AnimErrorCode::WrongType, "expected {\"name\": graph, ...}"))?;
            if pairs.len() > MAX_GRAPHS {
                return Err(err("graphs", AnimErrorCode::Limit, format!("{} graphs; at most {MAX_GRAPHS}", pairs.len())).with_numbers(pairs.len() as f64, MAX_GRAPHS as f64));
            }
            for (name, gv) in pairs {
                set.add_graph(name, gv).map_err(|e| e.under(&format!("graphs.{name}")))?;
            }
        }
        o.finish("anims, rig, masks, clips, graphs")?;
        Ok(set)
    }

    /// A clip from JSON: a keyed clip (`kind` absent or `"clip"`) or a generator spec, which may
    /// build on this set's clips. A generator's work counts against the set's budget
    /// ([`gen::MAX_SET_WORK`]; each clip within [`gen::MAX_CLIP_WORK`]): past it, a `Limit`
    /// error at the clip's path and nothing is generated.
    pub fn make_clip(&mut self, c: &Json, path: &str) -> Result<Clip, AnimError> {
        let kind = c.get("kind").and_then(|k| k.as_str()).unwrap_or("clip");
        if kind == "clip" {
            Clip::from_json(c, path)
        } else {
            let (clip, work) = gen::generate_counted(c, path, self)?;
            self.generated += work;
            Ok(clip)
        }
    }

    /// The generation work this set's generated clips took (frames × bones).
    pub fn generation_work(&self) -> u64 {
        self.generated
    }

    /// About how many bytes the set's clips hold (keys and their times), for caches.
    pub fn approx_bytes(&self) -> usize {
        self.clips.iter().map(|b| b.clip.tracks.iter().map(|t| t.keys.times.len() * 20).sum::<usize>() + 64).sum::<usize>() + 256
    }

    /// Binds and adds (or replaces) graph `name`.
    pub fn add_graph(&mut self, name: &str, v: &Json) -> Result<(), AnimError> {
        if name.len() > graph::MAX_NAME || !is_ident(name) {
            return Err(err("", AnimErrorCode::WrongType, format!("graph names are identifiers of at most {} characters: '{name}'", graph::MAX_NAME)));
        }
        let g = AnimGraph::from_json(v, "", self)?;
        match self.graphs.iter().position(|(n, _)| n == name) {
            Some(i) => self.graphs[i].1 = g,
            None if self.graphs.len() >= MAX_GRAPHS => return Err(err("", AnimErrorCode::Limit, format!("at most {MAX_GRAPHS} graphs"))),
            None => self.graphs.push((name.to_string(), g)),
        }
        Ok(())
    }

    pub fn graph(&self, name: &str) -> Option<&AnimGraph> {
        self.graphs.iter().find(|(n, _)| n == name).map(|(_, g)| g)
    }

    /// Every graph with its name, in insertion order.
    pub fn graphs(&self) -> impl Iterator<Item = (&str, &AnimGraph)> {
        self.graphs.iter().map(|(n, g)| (n.as_str(), g))
    }

    /// Replaces the clip of the same name (keeping its id, so bound graphs stay valid) or adds it.
    pub fn put(&mut self, clip: Clip) -> Result<ClipId, AnimError> {
        match self.id(&clip.name) {
            Some(id) => {
                let mut tmp = AnimSet::new(self.skeleton.clone());
                tmp.add(clip)?;
                self.clips[id.0 as usize] = tmp.clips.pop().expect("added");
                Ok(id)
            }
            None => self.add(clip),
        }
    }

    /// This set with `clips` and then `graphs` laid over it (`(name, JSON)`; a clip's JSON is a
    /// keyed clip or a generator spec, whose own `name` is replaced by `name`). Same-named clips
    /// and graphs replace the set's, new ones are added, and every graph is bound again (a graph
    /// can use an overlay clip). What fails to parse or bind is left out and reported; the
    /// set's own version stays.
    pub fn with_overlay(&self, clips: &[(String, Json)], graphs: &[(String, Json)]) -> (AnimSet, Vec<OverlayProblem>) {
        let mut out = self.clone();
        let mut problems = Vec::new();
        for (name, c) in clips {
            let mut spec = c.clone();
            spec.set("name", Json::Str(name.clone()));
            match out.make_clip(&spec, "").and_then(|clip| out.put(clip)) {
                Ok(_) => {}
                Err(error) => problems.push(OverlayProblem { kind: "clip", name: name.clone(), error }),
            }
        }
        // Rebind the set's own graphs (clips they use may have changed), then the overlay's.
        let own: Vec<(String, Json)> = self.graphs.iter().map(|(n, g)| (n.clone(), g.source.clone())).collect();
        out.graphs.clear();
        for (name, g) in own.iter().filter(|(n, _)| !graphs.iter().any(|(m, _)| m == n)) {
            match out.add_graph(name, g) {
                Ok(()) => {}
                Err(error) => {
                    // An overlay clip broke the set's own graph: keep the old binding visible as a problem.
                    problems.push(OverlayProblem { kind: "graph", name: name.clone(), error });
                }
            }
        }
        for (name, g) in graphs {
            if let Err(error) = out.add_graph(name, g) {
                problems.push(OverlayProblem { kind: "graph", name: name.clone(), error });
                // The set's own graph of that name, if it binds, stays.
                if let Some((_, own_g)) = own.iter().find(|(n, _)| n == name) {
                    let _ = out.add_graph(name, own_g);
                }
            }
        }
        (out, problems)
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

    /// Every mask's name, in id order.
    pub fn mask_names(&self) -> Vec<&str> {
        self.masks.iter().map(|(n, _)| n.as_str()).collect()
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
