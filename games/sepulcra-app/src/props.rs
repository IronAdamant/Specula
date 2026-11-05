//! The map's props in the world: `/props/graveyard/<name>`, one model each, where the map data
//! (`data/slab_01.json`'s `props`, see `sepulcra::props`) puts it, standing on its cell.
//!
//! - **Ordinary entities.** Saved with the world, journaled and hashed like the slab. Each one's
//!   `props` say what the rules know: `blocks_build`, the `cells` it covers and `what` it is, so
//!   an agent's `world_query` answers "can I build here?" from the same data as the rules.
//! - **Adopted, not replaced.** A world that already holds a prop by that path (the approved
//!   M4 world, where the owner's promotion put them; a loaded save) keeps its entity and its
//!   history; only a missing one is spawned, and one more than 1 cm or 0.5° from its data place
//!   is moved back there (the rules' cells follow the data, so the drawing must too), taking the
//!   data's facts with the move. One in place is never written: the owner's approval stays its
//!   last write (`modified_by: window:owner`), even when its props lack the facts (an approved
//!   M4 entity predates them; the map data has them).
//! - **Dressing under a stone is hidden.** A prop with `blocks_build: false` is hidden while a
//!   stone stands on any cell it covers, and shown again when the stone is sold: never drawn
//!   through the stone. Big pieces can't have a stone on their cells (the rules refuse it), and
//!   props stand only on fragments, so a wall (lanes only) never meets one.
//! - **Cost.** Visibility is recomputed only when the set of stone cells changes (compared
//!   in place each tick, no allocation unless it changed).

use crate::ents;
use crate::slab::SlabGeo;
use sepulcra::map::Map;
use sepulcra::{CellId, State};
use specula::game::GameWorld;
use specula::world::{ModelSpec, UpdateSpec};
use specula_core::math::Vec3;
use specula_core::Json;

/// Where the props live.
pub const ROOT: &str = "/props/graveyard";
/// How far an adopted prop may stray from its data place before it is moved back.
const SNAP_M: f32 = 0.01;
const SNAP_DEG: f32 = 0.5;

/// The props' entities and what is shown.
#[derive(Clone, Default)]
pub struct Props {
    /// Entity per map prop (0: the world refused it), in map order.
    ids: Vec<u64>,
    /// Hidden now (dressing under a stone).
    hidden: Vec<bool>,
    /// The stone cells visibility was last computed for.
    stones: Vec<CellId>,
    synced: bool,
}

/// Where a prop stands: its data place, on the surface of its cell.
pub fn place_of(map: &Map, geo: &SlabGeo, i: usize) -> (Vec3, Vec3) {
    let p = &map.props[i];
    let [x, _, z] = p.at.to_f32();
    let y = p.cells.iter().find(|c| {
        let cell = map.cell(**c);
        let half = map.cell_mm / 2;
        (cell.pos.x - p.at.x).abs() <= half && (cell.pos.z - p.at.z).abs() <= half
    });
    let y = y.map_or(0.0, |c| geo.surface[c.index()]);
    (Vec3::new(x, y, z), Vec3::new(0.0, p.yaw_deg(), 0.0))
}

fn facts(map: &Map, i: usize) -> Json {
    let p = &map.props[i];
    Json::obj([
        ("prop", Json::from(p.name.as_str())),
        ("what", Json::from(p.what.as_str())),
        ("blocks_build", Json::Bool(p.blocks_build)),
        ("cells", Json::arr(p.cells.iter().map(|c| Json::from(map.cell_name(*c))))),
    ])
}

fn group_at(world: &mut GameWorld<'_>, agent: &str, parent: Option<u64>, path: &str, name: &str) -> u64 {
    match world.find_path(path) {
        Ok(id) => id,
        Err(_) => ents::group(world, agent, parent, name, Vec3::ZERO, Vec3::ZERO, &[], Json::Null),
    }
}

impl Props {
    /// The map's props in `world`: found where they already stand (kept, moved back to their data
    /// place if they strayed), spawned where missing.
    pub fn build(world: &mut GameWorld<'_>, agent: &str, map: &Map, geo: &SlabGeo) -> Props {
        if map.props.is_empty() {
            return Props::default();
        }
        let top = group_at(world, agent, None, "/props", "props");
        let root = group_at(world, agent, Some(top), ROOT, "graveyard");
        let mut ids = Vec::with_capacity(map.props.len());
        for (i, p) in map.props.iter().enumerate() {
            let (pos, rot) = place_of(map, geo, i);
            let recipe = format!("recipes/{}.json", p.recipe);
            let path = format!("{ROOT}/{}", p.name);
            let found = world.find_path(&path).ok().filter(|id| world.get(*id).and_then(|e| e.model.as_ref()).is_some_and(|m| m.recipe == recipe));
            let id = match found {
                Some(id) => {
                    let e = world.get(id).expect("found");
                    let turned = e.rotation_deg - rot;
                    let strayed = (e.position - pos).length() > SNAP_M || turned.x.abs().max(turned.y.abs()).max(turned.z.abs()) > SNAP_DEG;
                    // Facts it carries that the data contradicts are corrected (absent ones are no
                    // claim: an approved entity keeps its owner's last write).
                    let wrong = |k: &str, v: Option<&Json>| e.props.get(k).is_some_and(|x| Some(x) != v);
                    let data = facts(map, i);
                    let props_differ = wrong("blocks_build", data.get("blocks_build")) || wrong("cells", data.get("cells"));
                    if strayed || props_differ {
                        let mut merged = e.props.clone();
                        if !matches!(merged, Json::Obj(_)) {
                            merged = Json::obj::<&str>([]);
                        }
                        if let Json::Obj(pairs) = facts(map, i) {
                            for (k, v) in pairs {
                                merged.set(k, v);
                            }
                        }
                        let change = UpdateSpec { position: strayed.then_some(pos), rotation_deg: strayed.then_some(rot), props: Some(merged), ..UpdateSpec::default() };
                        ents::update(world, agent, id, change);
                    }
                    id
                }
                None => {
                    if let Ok(old) = world.find_path(&path) {
                        ents::delete(world, old);
                    }
                    let kind = if p.blocks_build { "blocks_build" } else { "dressing" };
                    let spec = ents::model_spec(root, &p.name, ModelSpec::new(recipe), pos, rot, 1.0, &["prop", kind], facts(map, i));
                    ents::spawn(world, agent, spec)
                }
            };
            ids.push(id);
        }
        let n = ids.len();
        Props { ids, hidden: vec![false; n], stones: Vec::new(), synced: false }
    }

    /// Hides dressing under stones (and shows it again), when the stones changed.
    pub fn sync(&mut self, world: &mut GameWorld<'_>, agent: &str, state: &State) {
        if self.ids.is_empty() {
            return;
        }
        if self.synced && self.stones.iter().copied().eq(state.stones().map(|(c, _)| c)) {
            return;
        }
        self.stones.clear();
        self.stones.extend(state.stones().map(|(c, _)| c));
        let map = &state.content().map;
        for (i, p) in map.props.iter().enumerate().take(self.ids.len()) {
            let hide = !p.blocks_build && p.cells.iter().any(|c| self.stones.binary_search(c).is_ok());
            // Written only where the world differs (an adopted world may hold a hidden one), so a
            // rebuild journals nothing for props already right.
            if hide != self.hidden[i] || !self.synced {
                if world.get(self.ids[i]).is_some_and(|e| e.visible == hide) {
                    ents::show(world, agent, self.ids[i], !hide);
                }
                self.hidden[i] = hide;
            }
        }
        self.synced = true;
    }

    /// Which props are hidden now (by map index), for tests and tools.
    pub fn hidden(&self) -> &[bool] {
        &self.hidden
    }

    /// The prop entities, in map order.
    pub fn ids(&self) -> &[u64] {
        &self.ids
    }
}
