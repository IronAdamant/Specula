//! Animation in the engine (stage A3; docs/design/ANIMATION.md): graph plays, the ground feet
//! stand on, and the anim assets agents write into the world.
//!
//! - **Graph plays** live in an entity's `anim` (graph mode, [`crate::world::AnimSpec`]). A write
//!   of new parameters is settled first ([`settle_update`]): the old play is run forward to the
//!   write's time and the new values replace the old ones from there, so a creature that starts
//!   walking blends into it instead of popping, and parameters not given keep their values.
//! - **Ground**: entities tagged [`GROUND_TAG`] are what feet stand on (their world boxes' tops,
//!   with soft edges so a foot crossing a step moves smoothly; [`GroundGrid`]). A graph's `feet`
//!   IK plants feet on it at render time. Without ground entities feet keep their animated
//!   height. The physics lane's queries can replace the box scan later behind the same
//!   [`specula_anim::ik::Ground`] trait.
//! - **Anim assets**: clips and graphs agents write live (the `anim_set` tool) are ordinary Group
//!   entities tagged [`ASSET_TAG`] with `props.anim_asset = {for, kind, data}`, normally under
//!   `/anims/<source path, / as :>/`. So they are journaled, undoable, claimable, saved with the world
//!   and hashed like everything else. They lay over the recipe's own anim set (same-named clips
//!   and graphs replace, new ones add; see [`specula_anim::AnimSet::with_overlay`]) for every
//!   model whose recipe is `for`. Props hold at most 16 KiB, so a keyframed clip that big is
//!   better written as a generator spec.
//!
//! Poses, IK and events in the render path are cosmetic; root motion, which can feed a
//! simulation, comes from [`specula_anim::play::root_motion`] at the simulation's own ticks.

use crate::write_policy::WorldWrite;
use crate::world::{AnimSpec, Entity, Shape, World, WorldError};
use specula_anim::graph::{AnimGraph, Value};
use specula_anim::ik::Ground;
use specula_anim::play::{self, Graphed, Scratch, Timing};
use specula_anim::AnimSet;
use specula_core::hash::StableHasher;
use specula_core::Json;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Entities with this tag are ground for feet.
pub const GROUND_TAG: &str = "ground";
/// Entities with this tag hold an anim asset in `props.anim_asset`.
pub const ASSET_TAG: &str = "anim_asset";
/// The props key of an anim asset.
pub const ASSET_PROP: &str = "anim_asset";
/// Width (m) of a ground box's soft edge.
pub const SOFT_EDGE_M: f32 = 0.08;
/// Grid cell (m) of the ground index.
const CELL_M: f32 = 2.0;
/// Coarse cell (m) for boxes spanning more than [`FINE_MAX_CELLS`] fine cells.
const COARSE_M: f32 = 64.0;
/// Most cells (per axis) a box is filed under on either level; bigger ones go to a short list
/// every query looks at, so indexing one box is bounded work however large it is.
const FINE_MAX_CELLS: i32 = 8;
const COARSE_MAX_CELLS: i32 = 16;
/// Most boxes on that list (larger than about 1 km); the rest are left out and counted.
pub const MAX_HUGE_GROUND: usize = 64;
/// Most ground boxes indexed (the rest are ignored and reported).
pub const MAX_GROUND_BOXES: usize = 65_536;
/// Most anim assets read from the world.
pub const MAX_ASSETS: usize = 1024;

/// The anim set (with the world's assets laid over it) of entity `e`'s model, once built.
pub fn anim_set_of(world: &World, e: &Entity) -> Option<Arc<AnimSet>> {
    world.model_source()?.anim_set(e.resolved_model()?.handle)
}

/// The anim set a graph write to `e` settles against: built from the model's rig and anims file
/// alone (no mesh, no waiting: see `Library::write_set`), with the world's own assets (as of
/// now) over it, so the result never depends on build timing or on when the engine last looked
/// at the world.
pub(crate) fn write_set_of(world: &World, e: &Entity) -> Option<Arc<AnimSet>> {
    world.model_source()?.anim_set_for_write(e.resolved_model()?.handle, world.anim_assets())
}

/// World metres per model metre of an entity (the mean of its axis scales), for `match_speed`.
pub fn entity_scale(e: &Entity) -> f32 {
    let m = e.model();
    let c = &m.cols;
    let len = |k: usize| (c[k][0] * c[k][0] + c[k][1] * c[k][1] + c[k][2] * c[k][2]).sqrt();
    ((len(0) + len(1) + len(2)) / 3.0).max(1e-6)
}

fn invalid(field: &str, message: String) -> WorldError {
    WorldError::new("invalid_value", message).with("field", field)
}

/// Checks a graph anim's parameters against the graph (names, types, enum values).
pub fn check_params(graph: &AnimGraph, name: &str, params: &[(String, Value)]) -> Result<(), WorldError> {
    for (n, v) in params {
        let field = format!("anim.params.{n}");
        let Some(i) = graph.param(n) else {
            let names: Vec<&str> = graph.params.iter().map(|p| p.name.as_str()).collect();
            return Err(invalid(&field, format!("graph '{name}' has no parameter '{n}' (params: {})", names.join(", "))));
        };
        graph.params[i as usize].accept(v).map_err(|m| invalid(&field, m))?;
    }
    Ok(())
}

/// What [`World::update`] does with a new graph anim on `e`: checks the graph and parameters
/// against the model's anim set (see [`write_set_of`]) and, when the entity already
/// plays the same graph and the write gives no `layers`, settles the old play at the write's
/// time and lays the new parameters over the old ones. Without an anim set (no rig, a failed
/// build, no model library) nothing can be checked or settled, but the parameters still merge.
pub(crate) fn settle_update(world: &World, e: &Entity, new: &mut AnimSpec) -> Result<(), WorldError> {
    let Some(set) = write_set_of(world, e) else {
        let gp = new.graph.as_mut().expect("graph mode");
        let old = e.anim.as_ref().and_then(|a| a.graph.as_deref().filter(|og| og.graph == gp.graph).map(|og| (a, og)));
        if let (Some((old_anim, old_play)), true) = (old, gp.layers.is_empty()) {
            let mut merged = old_play.clone();
            for (n, v) in &gp.params {
                merged.set_param(n, v.clone());
            }
            **gp = merged;
            // The play keeps its own anchor (its clock and states are as of it).
            new.start = old_anim.start;
            new.from = None;
            new.crossfade = 0.0;
        }
        return Ok(());
    };
    let gp = new.graph.as_mut().expect("graph mode");
    let Some(graph) = set.graph(&gp.graph) else {
        let names: Vec<&str> = set.graphs().map(|(n, _)| n).collect();
        return Err(invalid("anim.graph", format!("'{}' has no graph '{}' (graphs: {})", e.name, gp.graph, if names.is_empty() { "none".to_string() } else { names.join(", ") })));
    };
    check_params(graph, &gp.graph, &gp.params)?;
    let now = new.start.unwrap_or(world.world_time());
    let old = e.anim.as_ref().and_then(|a| a.graph.as_deref().filter(|og| og.graph == gp.graph).map(|og| (a, og)));
    if let (Some((old_anim, old_play)), true) = (old, gp.layers.is_empty()) {
        let timing = Timing { anchor: old_anim.start.unwrap_or(0.0), speed: old_anim.speed };
        let g = Graphed { set: &set, graph, scale: entity_scale(e) };
        let (settled, _) = play::settle(g, old_play, timing, now, &mut Scratch::default());
        let mut merged = settled;
        for (n, v) in &gp.params {
            merged.set_param(n, v.clone());
        }
        **gp = merged;
        new.start = Some(now);
        // The settled play already continues smoothly: no crossfade from the old one.
        new.from = None;
        new.crossfade = 0.0;
    }
    Ok(())
}

/// Re-settles the graph plays of the entities whose model is one of `handles` (their source
/// changed on disk: a hot reload or `model_reload`), at the current world time against their new
/// anim set. Called by the engine where it polls the files, so a changed anim set reaches the
/// world at a defined point, as journaled changes by `RELOAD_AGENT`. Returns how many plays it
/// wrote (a play its new set can't take keeps its old state and is reported by `anim_state`).
pub(crate) fn resettle(world: &mut dyn WorldWrite, handles: &BTreeSet<u32>) -> usize {
    let ids: Vec<(u64, AnimSpec)> = world
        .persistent_entities()
        .filter(|e| e.resolved_model().is_some_and(|r| handles.contains(&r.handle)))
        .filter_map(|e| e.anim.as_ref().filter(|a| a.graph.is_some()).map(|a| (e.id, a.clone())))
        .collect();
    let mut n = 0;
    for (id, old) in ids {
        let name = old.graph.as_ref().expect("filtered").graph.clone();
        let spec = AnimSpec { speed: old.speed, ..AnimSpec::graph(specula_anim::play::GraphPlay::new(name)) };
        let change = crate::world::UpdateSpec { anim: Some(Some(spec)), ..Default::default() };
        if world.update(&crate::world::EntityKey::Id(id), change, RELOAD_AGENT).is_ok() {
            n += 1;
        }
    }
    n
}

/// Who the journal says re-settled plays after a reload.
pub const RELOAD_AGENT: &str = "anim_reload";

/// Why entity `e`'s graph anim can't play as written (unknown graph, parameters, states), for
/// `anim_problem`; `None` when it plays.
pub fn graph_problem(set: &AnimSet, e: &Entity, a: &AnimSpec) -> Option<String> {
    let gp = a.graph.as_ref()?;
    let Some(graph) = set.graph(&gp.graph) else {
        let names: Vec<&str> = set.graphs().map(|(n, _)| n).collect();
        return Some(format!("'{}' plays graph '{}', which its anim set lacks: drawn in its bind pose (graphs: {})", e.name, gp.graph, names.join(", ")));
    };
    let problems = play::bind_problems(graph, gp);
    (!problems.is_empty()).then(|| format!("'{}': {}", e.name, problems.join("; ")))
}

// ---------------------------------------------------------------------------------------------
// Ground

/// One ground box: its world rectangle in x/z and its top.
#[derive(Clone, Copy, Debug, PartialEq)]
struct GroundBox {
    min: [f32; 2],
    max: [f32; 2],
    top: f32,
}

/// The tops of the world's ground boxes, indexed on a grid. Heights have soft edges
/// ([`SOFT_EDGE_M`]): near a step the ground eases from one height to the other, so feet moving
/// over it never jump.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroundGrid {
    boxes: Vec<GroundBox>,
    cells: BTreeMap<(i32, i32), Vec<u32>>,
    coarse: BTreeMap<(i32, i32), Vec<u32>>,
    huge: Vec<u32>,
    /// Ground entities left out past [`MAX_GROUND_BOXES`] (or [`MAX_HUGE_GROUND`]).
    pub dropped: usize,
}

fn cell_of(v: f32) -> i32 {
    specula_core::dmath::floor(v / CELL_M) as i32
}

fn coarse_of(v: f32) -> i32 {
    specula_core::dmath::floor(v / COARSE_M) as i32
}

impl GroundGrid {
    /// The ground of `world`: every shown entity tagged [`GROUND_TAG`] with geometry, as its
    /// world box (rotated pieces count by their bounding box).
    pub fn of(world: &World) -> GroundGrid {
        let mut g = GroundGrid::default();
        for e in world.persistent_entities() {
            if !e.tags.contains(GROUND_TAG) || !e.shown() || e.local_bounds().is_none() || e.shape == Shape::Group {
                continue;
            }
            if g.boxes.len() >= MAX_GROUND_BOXES {
                g.dropped += 1;
                continue;
            }
            let (lo, hi) = e.world_aabb();
            g.insert(GroundBox { min: [lo.x, lo.z], max: [hi.x, hi.z], top: hi.y });
        }
        g
    }

    /// Files a box under the fine grid, the coarse one or the huge list, by its size.
    fn insert(&mut self, b: GroundBox) {
        if !(b.min.iter().chain(&b.max).all(|v| v.is_finite()) && b.top.is_finite()) {
            self.dropped += 1;
            return;
        }
        let r = SOFT_EDGE_M;
        let (x0, x1, z0, z1) = (b.min[0] - r, b.max[0] + r, b.min[1] - r, b.max[1] + r);
        let i = self.boxes.len() as u32;
        let span = |f: fn(f32) -> i32| (f(x1) as i64 - f(x0) as i64).max(f(z1) as i64 - f(z0) as i64) + 1;
        let (fine, coarse) = (span(cell_of), span(coarse_of));
        if fine <= FINE_MAX_CELLS as i64 {
            for cx in cell_of(x0)..=cell_of(x1) {
                for cz in cell_of(z0)..=cell_of(z1) {
                    self.cells.entry((cx, cz)).or_default().push(i);
                }
            }
        } else if coarse <= COARSE_MAX_CELLS as i64 {
            for cx in coarse_of(x0)..=coarse_of(x1) {
                for cz in coarse_of(z0)..=coarse_of(z1) {
                    self.coarse.entry((cx, cz)).or_default().push(i);
                }
            }
        } else if self.huge.len() < MAX_HUGE_GROUND {
            self.huge.push(i);
        } else {
            self.dropped += 1;
            return;
        }
        self.boxes.push(b);
    }

    pub fn len(&self) -> usize {
        self.boxes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }
}

impl Ground for GroundGrid {
    fn height(&self, x: f32, z: f32, top: f32, depth: f32) -> Option<f32> {
        let none: &[u32] = &[];
        let fine = self.cells.get(&(cell_of(x), cell_of(z))).map_or(none, Vec::as_slice);
        let coarse = self.coarse.get(&(coarse_of(x), coarse_of(z))).map_or(none, Vec::as_slice);
        let cand = || fine.iter().chain(coarse).chain(&self.huge);
        let r = SOFT_EDGE_M;
        let reach = |b: &GroundBox| b.top <= top && b.top >= top - depth;
        // Signed distance from (x, z) to a box's rectangle (negative inside).
        let dist = |b: &GroundBox| {
            let dx = (b.min[0] - x).max(x - b.max[0]);
            let dz = (b.min[1] - z).max(z - b.max[1]);
            if dx > 0.0 && dz > 0.0 {
                (dx * dx + dz * dz).sqrt()
            } else {
                dx.max(dz)
            }
        };
        // The base: the highest box the point is on. Then every higher box within r eases the
        // ground up towards its top (reaching it at its edge), so the height is continuous both
        // stepping up and stepping off.
        let base = cand().map(|&i| &self.boxes[i as usize]).filter(|b| reach(b) && dist(b) <= 0.0).map(|b| b.top).fold(None, |m: Option<f32>, t| Some(m.map_or(t, |m| m.max(t))))?;
        let mut h = base;
        for &i in cand() {
            let b = &self.boxes[i as usize];
            let d = dist(b);
            if b.top > base && reach(b) && d > 0.0 && d < r {
                h = h.max(base + (b.top - base) * specula_anim::math::smoothstep(1.0 - d / r));
            }
        }
        Some(h)
    }
}

// ---------------------------------------------------------------------------------------------
// Anim assets

/// The world's anim assets for one model source (`for`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssetSet {
    /// `(name, clip JSON or generator spec)`, in path order.
    pub clips: Vec<(String, Json)>,
    pub graphs: Vec<(String, Json)>,
    /// The entities they came from: `(path, id, version)`.
    pub entities: Vec<(String, u64, u64)>,
    /// A hash of the content (overlay caches key on it).
    pub revision: u64,
}

/// Every model source's assets.
pub type Assets = BTreeMap<String, AssetSet>;

/// One asset entity's content: `{for, kind: "clip" | "graph", data}`.
pub struct AssetRef<'a> {
    pub source: &'a str,
    pub kind: &'a str,
    pub data: &'a Json,
}

/// Reads an asset entity's `props.anim_asset`, or why it isn't one.
pub fn asset_of(e: &Entity) -> Result<AssetRef<'_>, String> {
    let a = e.props.get(ASSET_PROP).ok_or_else(|| format!("'{}' is tagged {ASSET_TAG} but has no props.{ASSET_PROP}", e.name))?;
    let source = a.get("for").and_then(Json::as_str).ok_or("props.anim_asset.for must name the recipe (or glTF) it is for")?;
    let kind = a.get("kind").and_then(Json::as_str).filter(|k| *k == "clip" || *k == "graph").ok_or("props.anim_asset.kind must be \"clip\" or \"graph\"")?;
    let data = a.get("data").filter(|d| d.as_object().is_some()).ok_or("props.anim_asset.data must be the clip or graph (an object)")?;
    Ok(AssetRef { source, kind, data })
}

/// The world's anim assets by model source, and what was wrong with the entities left out.
pub fn assets_of(world: &World) -> (Assets, Vec<String>) {
    let mut out = Assets::new();
    let mut problems = Vec::new();
    let mut ents: Vec<(String, &Entity)> = world.persistent_entities().filter(|e| e.tags.contains(ASSET_TAG)).map(|e| (world.path_of(e.id), e)).collect();
    ents.sort_by(|a, b| a.0.cmp(&b.0));
    if ents.len() > MAX_ASSETS {
        problems.push(format!("{} anim assets; only the first {MAX_ASSETS} (by path) are used", ents.len()));
        ents.truncate(MAX_ASSETS);
    }
    for (path, e) in ents {
        match asset_of(e) {
            Err(m) => problems.push(format!("{path}: {m}")),
            Ok(a) => {
                let set = out.entry(a.source.to_string()).or_default();
                let list = if a.kind == "clip" { &mut set.clips } else { &mut set.graphs };
                if list.iter().any(|(n, _)| *n == e.name) {
                    problems.push(format!("{path}: a second {} '{}' for {} (the first by path is used)", a.kind, e.name, a.source));
                    continue;
                }
                list.push((e.name.clone(), a.data.clone()));
                set.entities.push((path, e.id, e.version));
            }
        }
    }
    for set in out.values_mut() {
        let mut h = StableHasher::with_seed(0x616e_696d_0000_0001);
        for (n, j) in set.clips.iter().chain(&set.graphs) {
            h.write_str(n);
            h.write_str(&j.to_string());
        }
        h.write_u64(set.clips.len() as u64);
        set.revision = h.finish();
    }
    (out, problems)
}

/// The world's anim assets, read again only when an asset entity changed (from the world's
/// change log). The world keeps one, refreshed inside its own mutations, so a graph write
/// settles against the assets as they are at that write, whatever ran before it (a batch, an
/// undo, a load).
#[derive(Clone, Debug, Default)]
pub struct AssetCache {
    /// The draw epoch last looked at.
    seen: Option<u64>,
    ids: BTreeSet<u64>,
    pub assets: Arc<Assets>,
}

impl AssetCache {
    pub fn refresh(&mut self, world: &World) {
        let epoch = world.draw_epoch();
        if self.seen == Some(epoch) {
            return;
        }
        let stale = match self.seen.and_then(|s| world.draw_changes_since(s)) {
            None => true,
            Some(mut changed) => changed.any(|id| self.ids.contains(&id) || world.get(id).is_some_and(|x| x.tags.contains(ASSET_TAG))),
        };
        self.seen = Some(epoch);
        if stale {
            self.ids = world.persistent_entities().filter(|e| e.tags.contains(ASSET_TAG)).map(|e| e.id).collect();
            let (a, _) = assets_of(world);
            if *self.assets != a {
                self.assets = Arc::new(a);
            }
        }
    }
}

/// What the engine last built ground and assets from, so a frame only rebuilds them when an
/// entity that matters changed.
#[derive(Default)]
pub struct AnimEnv {
    /// `(world generation, draw epoch)` last looked at.
    seen: Option<(u64, u64)>,
    ground_ids: BTreeSet<u64>,
    asset_ids: BTreeSet<u64>,
    pub ground: Option<Arc<GroundGrid>>,
    pub assets: Arc<Assets>,
    pub asset_problems: Vec<String>,
}

impl AnimEnv {
    /// Brings the ground and the assets up to date with `world`. Returns true when the assets
    /// changed (the library's anim sets must be laid over again).
    pub fn refresh(&mut self, world: &World, generation: u64) -> bool {
        let epoch = world.draw_epoch();
        if self.seen == Some((generation, epoch)) {
            return false;
        }
        let (mut ground, mut assets) = (true, true);
        if let Some((_, e)) = self.seen.filter(|(g, _)| *g == generation) {
            if let Some(changed) = world.draw_changes_since(e) {
                // Only changes to ground or asset entities (now or before) matter.
                let (mut gr, mut asr) = (false, false);
                for id in changed {
                    let now = world.get(id);
                    gr |= self.ground_ids.contains(&id) || now.is_some_and(|x| x.tags.contains(GROUND_TAG));
                    asr |= self.asset_ids.contains(&id) || now.is_some_and(|x| x.tags.contains(ASSET_TAG));
                }
                (ground, assets) = (gr, asr);
            }
        }
        self.seen = Some((generation, epoch));
        if ground {
            self.ground_ids = world.persistent_entities().filter(|e| e.tags.contains(GROUND_TAG)).map(|e| e.id).collect();
            let grid = GroundGrid::of(world);
            self.ground = (!grid.is_empty()).then(|| Arc::new(grid));
        }
        if assets {
            self.asset_ids = world.persistent_entities().filter(|e| e.tags.contains(ASSET_TAG)).map(|e| e.id).collect();
            let (a, problems) = assets_of(world);
            self.asset_problems = problems;
            if *self.assets != a {
                self.assets = Arc::new(a);
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(boxes: &[([f32; 2], [f32; 2], f32)]) -> GroundGrid {
        let mut g = GroundGrid::default();
        for (min, max, top) in boxes {
            g.insert(GroundBox { min: *min, max: *max, top: *top });
        }
        g
    }

    /// Review round 1, finding 5: every box used to be filed under every 2 m cell it covers (a
    /// 1 km plane took 60 ms to index, 8 km 8.6 s). Large boxes go to a coarse grid or a short list.
    #[test]
    fn large_ground_is_indexed_in_bounded_work() {
        for side in [1000.0f32, 4000.0, 8000.0, 100_000.0] {
            let mut world = World::default();
            let mut s = crate::world::SpawnSpec::new(Shape::Cube);
            s.scale = specula_core::math::Vec3::new(side, 1.0, side);
            s.tags.insert(GROUND_TAG.into());
            world.spawn(s, "t").unwrap();
            let t = std::time::Instant::now();
            let g = GroundGrid::of(&world);
            let took = t.elapsed();
            assert_eq!(g.len(), 1);
            assert!(g.cells.len() + g.coarse.len() <= 256, "{side} m: {} cells", g.cells.len() + g.coarse.len());
            assert!(took < specula_core::speed_budget(std::time::Duration::from_millis(20)), "{side} m: {took:?}");
            // The cube's top is 0.5 m: anywhere on it, edges included.
            assert_eq!(g.height(0.0, 0.0, 1.0, 1.0), Some(0.5));
            assert_eq!(g.height(side * 0.49, -side * 0.49, 1.0, 1.0), Some(0.5));
            assert!(g.height(side, 0.0, 1.0, 1.0).is_none());
        }
        // Mixed sizes: a step on a big plane still eases (fine and coarse boxes together).
        let g = grid(&[([-300.0, -300.0], [300.0, 300.0], 0.0), ([1.0, -5.0], [5.0, 5.0], 0.12)]);
        assert_eq!(g.height(3.0, 0.0, 0.5, 1.0), Some(0.12));
        assert_eq!(g.height(-3.0, 0.0, 0.5, 1.0), Some(0.0));
        let mid = g.height(1.0 - SOFT_EDGE_M * 0.5, 0.0, 0.5, 1.0).unwrap();
        assert!(mid > 0.0 && mid < 0.12, "{mid}");
    }

    #[test]
    fn ground_steps_are_soft_and_bounded() {
        // A lane at 0 and a 0.12 m graveyard step from x = 1.
        let g = grid(&[([-5.0, -5.0], [1.0, 5.0], 0.0), ([1.0, -5.0], [5.0, 5.0], 0.12)]);
        assert_eq!(g.height(-1.0, 0.0, 0.5, 1.0), Some(0.0));
        assert_eq!(g.height(3.0, 0.0, 0.5, 1.0), Some(0.12));
        // Crossing the edge: continuous and monotonic.
        let mut prev = g.height(0.8, 0.0, 0.5, 1.0).unwrap();
        for k in 1..=40 {
            let x = 0.8 + k as f32 * 0.01;
            let h = g.height(x, 0.0, 0.5, 1.0).unwrap();
            assert!(h >= prev - 1e-6 && h - prev < 0.03, "x {x}: {prev} -> {h}");
            prev = h;
        }
        assert_eq!(prev, 0.12);
        // Too high to step onto, or too far down: not ground.
        assert!(g.height(3.0, 0.0, 0.05, 1.0).is_none(), "a 0.12 m step is above a 0.05 m reach");
        assert!(g.height(3.0, 0.0, 0.05, 0.02).is_none());
        assert!(g.height(50.0, 0.0, 0.5, 1.0).is_none(), "off the grid");
    }
}
