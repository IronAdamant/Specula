//! The live presentation: mirrors the simulation [`State`] into the engine's world.
//!
//! Everything is **reconciled** against the state once per tick (stones, walls, roots,
//! tombstones, enemies, projectiles), so a restart, a loaded replay or an agent's command never
//! leaves a stale entity behind. Events only add flourishes (flashes) on top.
//!
//! Cost (foundation F3): structure changes touch only what changed; enemies and projectiles are
//! pooled (an entity per pool slot, reused by the next enemy of the same kind) and moved once per
//! frame, interpolated between the last two ticks with the frame's `alpha`. Entities that didn't
//! move aren't written.

use crate::ents::{self, PARK, PARK_SCALE};
use crate::looks::{self, palette::*, Part};
use crate::slab::{self, SlabGeo};
use sepulcra::content::Attack;
use sepulcra::map::{CellKind, Face};
use sepulcra::sim::{EnemyId, ProjectileId};
use sepulcra::{CellId, Enemy, Event, Motion, Projectile, State};
use specula::world::{LookPatch, UpdateSpec, World};
use specula_core::math::Vec3;
use specula_core::Json;
use specula_gpu::Material;
use std::collections::BTreeMap;

/// Most point lights the game uses (the engine renders 16; leave room for agents' lights).
const MAX_GAME_LIGHTS: usize = 12;
const FX_POOL: usize = 48;
const RING_DOTS: usize = 28;
/// Gameplay pieces read from 30–60 m away (STYLE.md: slightly oversized).
const ENEMY_SCALE: Vec3 = Vec3 { x: 1.25, y: 1.25, z: 1.25 };

/// A pooled moving thing (an enemy or a projectile).
struct Slot {
    id: u64,
    /// Enemy kind or stone kind index: slots are reused only for the same look.
    look: u8,
    sim: Option<u32>,
    prev: Vec3,
    cur: Vec3,
    prev_rot: Vec3,
    rot: Vec3,
    /// What was last written to the world.
    written: Option<(Vec3, Vec3)>,
    /// Projectiles: tier of the stone that fired it (for splash size).
    tier: u8,
    /// Flyers bob.
    flies: bool,
}

struct StoneView {
    id: u64,
    kind: u8,
    tier: u8,
    lit: bool,
}

struct WallView {
    id: u64,
    bucket: u8,
}

#[derive(Clone, Copy)]
struct Fx {
    id: u64,
    start: f32,
    dur: f32,
    from: Vec3,
    to: Vec3,
    active: bool,
}

/// What the cursor shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CursorLook {
    Hidden,
    Valid(CellId),
    Invalid(CellId),
}

pub struct Scene {
    pub geo: SlabGeo,
    agent: String,
    stones_g: u64,
    walls_g: u64,
    roots_g: u64,
    enemies_g: u64,
    bolts_g: u64,
    fx_g: u64,
    tombs_g: u64,
    ui_g: u64,
    stones: BTreeMap<CellId, StoneView>,
    walls: BTreeMap<CellId, WallView>,
    roots: BTreeMap<CellId, u64>,
    enemy_slots: Vec<Slot>,
    enemies: BTreeMap<EnemyId, usize>,
    bolt_slots: Vec<Slot>,
    bolts: BTreeMap<ProjectileId, usize>,
    fx: Vec<Fx>,
    tombs: Vec<(u64, Vec3, bool)>,
    cursor: u64,
    cursor_shown: CursorLook,
    ring: Vec<u64>,
    ring_shown: Option<(u32, u32, u32, u32)>,
    /// Scratch, reused per tick.
    seen: Vec<u32>,
    /// Scratch: live projectile ids, sorted.
    live: Vec<ProjectileId>,
}

fn props_obj(pairs: Vec<(&str, Json)>) -> Json {
    Json::obj(pairs)
}

fn lerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t
}

fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let mut d = (b - a) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    a + d * t
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t)
}

impl Scene {
    /// Builds the slab and the empty groups the game fills (replacing any from an earlier run).
    pub fn build(world: &mut World, agent: &str, state: &State) -> Scene {
        let map = &state.content().map;
        let geo = SlabGeo::new(map);
        slab::build(world, agent, map, &geo);
        for name in ["units", "fx", "ui"] {
            if let Some(old) = ents::top_level(world, name) {
                ents::delete(world, old);
            }
        }
        let units = ents::group(world, agent, None, "units", Vec3::ZERO, Vec3::ZERO, &["game"], Json::Null);
        let sub = |world: &mut World, name: &str| ents::group(world, agent, Some(units), name, Vec3::ZERO, Vec3::ZERO, &[], Json::Null);
        let (stones_g, walls_g, roots_g, enemies_g, bolts_g, tombs_g) =
            (sub(world, "stones"), sub(world, "walls"), sub(world, "roots"), sub(world, "enemies"), sub(world, "bolts"), sub(world, "tombstones"));
        let fx_g = ents::group(world, agent, None, "fx", Vec3::ZERO, Vec3::ZERO, &["effects"], Json::Null);
        let ui_g = ents::group(world, agent, None, "ui", Vec3::ZERO, Vec3::ZERO, &["ui"], Json::Null);
        let cursor_part = Part::cube("cursor", PARK, PARK_SCALE, RUNE_GLOW).glow(1.4);
        let cursor = ents::part(world, agent, ui_g, &cursor_part);
        let ring = (0..RING_DOTS)
            .map(|i| ents::part_named(world, agent, ui_g, &format!("range_{i:02}"), &Part::cube("", PARK, PARK_SCALE, RUNE_PALE).glow(2.0)))
            .collect();
        let mut scene = Scene {
            geo,
            agent: agent.to_owned(),
            stones_g,
            walls_g,
            roots_g,
            enemies_g,
            bolts_g,
            fx_g,
            tombs_g,
            ui_g,
            stones: BTreeMap::new(),
            walls: BTreeMap::new(),
            roots: BTreeMap::new(),
            enemy_slots: Vec::new(),
            enemies: BTreeMap::new(),
            bolt_slots: Vec::new(),
            bolts: BTreeMap::new(),
            fx: Vec::new(),
            tombs: Vec::new(),
            cursor,
            cursor_shown: CursorLook::Hidden,
            ring,
            ring_shown: None,
            seen: Vec::new(),
            live: Vec::new(),
        };
        scene.build_tombstones(world, state);
        scene
    }

    fn build_tombstones(&mut self, world: &mut World, state: &State) {
        let map = &state.content().map;
        let n = state.content().rules.tombstones.max(0) as usize;
        let [cx, _, cz] = map.graveyard_centre.to_f32();
        let cols = (n as f32).sqrt().ceil().max(1.0) as usize;
        let rows = n.div_ceil(cols);
        for i in 0..n {
            let (r, c) = (i / cols, i % cols);
            let in_row = if r + 1 == rows { n - r * cols } else { cols };
            let x = cx + (c as f32 - (in_row as f32 - 1.0) / 2.0) * 1.7;
            let z = cz + (r as f32 - (rows as f32 - 1.0) / 2.0) * 2.2;
            let pos = Vec3::new(x, slab::GRAVEYARD_HEIGHT, z);
            let rot = Vec3::new(0.0, ((i * 37) % 13) as f32 - 6.0, 0.0);
            let id = ents::model(world, &self.agent, Some(self.tombs_g), &format!("tomb_{:02}", i + 1), pos, rot, &looks::tombstone_model(i), &["tombstone"], Json::Null);
            self.tombs.push((id, pos, false));
        }
    }

    // ---- per tick ----

    /// Brings the world in line with `state` (structure and the motion targets), and turns this
    /// tick's `events` into flashes. `now` is presentation time in seconds.
    pub fn sync(&mut self, world: &mut World, state: &State, events: &[Event], now: f32) {
        self.sync_structure(world, state, events, now);
        self.sync_enemies(world, state);
        self.sync_bolts(world, state);
    }

    /// Everything but motion: stones, walls, roots, tombstones and flashes. Cheap enough to run
    /// right after a command, so a build shows at once even while the clock is paused.
    pub fn sync_structure(&mut self, world: &mut World, state: &State, events: &[Event], now: f32) {
        // Flashes first: they read the slots of enemies and projectiles that are about to go.
        self.flourishes(world, state, events, now);
        self.sync_stones(world, state);
        self.sync_walls(world, state);
        self.sync_roots(world, state);
        self.sync_tombstones(world, state);
    }

    fn sync_stones(&mut self, world: &mut World, state: &State) {
        let content = state.content();
        let gone: Vec<CellId> = self.stones.keys().copied().filter(|c| state.stone(*c).is_none()).collect();
        for c in gone {
            if let Some(v) = self.stones.remove(&c) {
                ents::delete(world, v.id);
            }
        }
        for (cell, s) in state.stones() {
            if self.stones.get(&cell).is_some_and(|v| v.kind == s.kind.0 && v.tier == s.tier) {
                continue;
            }
            if let Some(v) = self.stones.remove(&cell) {
                ents::delete(world, v.id);
            }
            let def = content.stone(s.kind);
            let mut parts = looks::stone_model(&def.id, s.tier);
            let lights = self.stones.values().filter(|v| v.lit).count();
            let mut lit = false;
            for p in &mut parts {
                if p.light.is_some() {
                    if lights >= MAX_GAME_LIGHTS {
                        p.light = None;
                    } else {
                        lit = true;
                    }
                }
            }
            let (anchor, _) = self.geo.anchor(&content.map, cell);
            let name = content.map.cell_name(cell);
            let props = props_obj(vec![
                ("cell", Json::from(name.as_str())),
                ("stone", Json::from(def.id.as_str())),
                ("tier", Json::from(u32::from(s.tier) + 1)),
            ]);
            let id = ents::model(world, &self.agent, Some(self.stones_g), &name, anchor, Vec3::ZERO, &parts, &["stone", &def.id], props);
            self.stones.insert(cell, StoneView { id, kind: s.kind.0, tier: s.tier, lit });
        }
    }

    fn sync_walls(&mut self, world: &mut World, state: &State) {
        let map = &state.content().map;
        let gone: Vec<CellId> = self.walls.keys().copied().filter(|c| state.wall(*c).is_none()).collect();
        for c in gone {
            if let Some(v) = self.walls.remove(&c) {
                ents::delete(world, v.id);
            }
        }
        for (cell, w) in state.walls() {
            let frac = w.hp as f32 / w.max_hp.max(1) as f32;
            let bucket = (frac * 4.0).ceil().clamp(1.0, 4.0) as u8;
            if self.walls.get(&cell).is_some_and(|v| v.bucket == bucket) {
                continue;
            }
            if let Some(v) = self.walls.remove(&cell) {
                ents::delete(world, v.id);
            }
            let c = map.cell(cell);
            let walk = |dc: i32, dr: i32| {
                let (nc, nr) = (i32::from(c.col) + dc, i32::from(c.row) + dr);
                nc >= 0 && nr >= 0 && map.at(c.face, nc as u8, nr as u8).is_some_and(|n| map.cell(n).kind.walkable(false))
            };
            // The lane runs along X when its neighbours are east/west: the wall spans it along Z.
            let along_x = (walk(1, 0) || walk(-1, 0)) && !(walk(0, 1) || walk(0, -1));
            let rot = Vec3::new(0.0, if along_x { 90.0 } else { 0.0 }, 0.0);
            let (anchor, _) = self.geo.anchor(map, cell);
            let name = map.cell_name(cell);
            let props = props_obj(vec![("cell", Json::from(name.as_str())), ("hp", Json::from(w.hp / 1000)), ("max_hp", Json::from(w.max_hp / 1000))]);
            let parts = looks::wall_model(u32::from(cell.0), frac);
            let id = ents::model(world, &self.agent, Some(self.walls_g), &name, anchor, rot, &parts, &["wall"], props);
            self.walls.insert(cell, WallView { id, bucket });
        }
    }

    fn sync_roots(&mut self, world: &mut World, state: &State) {
        let map = &state.content().map;
        let gone: Vec<CellId> = self.roots.keys().copied().filter(|c| !state.is_rooted(*c)).collect();
        for c in gone {
            if let Some(id) = self.roots.remove(&c) {
                ents::delete(world, id);
            }
        }
        for c in map.cells() {
            if c.face != Face::Top || self.roots.contains_key(&c.id) || !state.is_rooted(c.id) {
                continue;
            }
            let (anchor, _) = self.geo.anchor(map, c.id);
            let k = (c.id.0 % 7) as f32;
            let p = Part::cube("", anchor + Vec3::new(0.0, 0.03, 0.0), Vec3::new(1.7, 0.06, 1.7), MOSS).rot(0.0, k * 9.0, 0.0).rough(1.0);
            let g = ents::part_named(world, &self.agent, self.roots_g, &map.cell_name(c.id), &p);
            let root = Part::cube("tendril", Vec3::new(0.0, 0.6, 0.0), Vec3::new(0.9, 1.4, 0.12), looks::hex(0x5a4632)).rot(0.0, 35.0 + k * 20.0, 0.0);
            ents::part(world, &self.agent, g, &root);
            self.roots.insert(c.id, g);
        }
    }

    fn sync_tombstones(&mut self, world: &mut World, state: &State) {
        let left = state.tombstones().max(0) as usize;
        for (i, (id, pos, down)) in self.tombs.iter_mut().enumerate() {
            let should = i >= left;
            if *down == should {
                continue;
            }
            *down = should;
            let (p, r) = if should { (*pos + Vec3::new(0.0, -0.05, 0.25), Vec3::new(-78.0, 8.0, 0.0)) } else { (*pos, Vec3::ZERO) };
            ents::place(world, &self.agent, *id, Some(p), Some(r), None);
            let tags = if should { vec!["raised".to_owned()] } else { Vec::new() };
            let untag = if should { Vec::new() } else { vec!["raised".to_owned()] };
            ents::update(world, &self.agent, *id, UpdateSpec { add_tags: tags, remove_tags: untag, ..UpdateSpec::default() });
            // The dead rose here: the stone glows with necromancy once it falls.
            for child in world.children_of(Some(*id)).collect::<Vec<_>>() {
                // Enemy green on purpose: the dead rose here (a necromantic mark), but kept dim.
                let (c, e) = if should { (GRAVE_DEEP, 0.25) } else { (GRAVE_STONE, 0.0) };
                ents::recolor(world, &self.agent, child, c, LookPatch { emissive: Some(e), ..LookPatch::default() });
            }
        }
    }

    fn enemy_pos(&self, e: &Enemy) -> Vec3 {
        let [x, y, z] = e.world_pos_f32();
        let mut p = Vec3::new(x, y, z);
        if let Motion::Ground { progress, len, leap, .. } = e.motion {
            if y < -0.01 {
                // On a cliff: stand on the rock, not inside it.
                let out = if x.abs() - self.geo.half_x > z.abs() - self.geo.half_z { Vec3::new(x.signum(), 0.0, 0.0) } else { Vec3::new(0.0, 0.0, z.signum()) };
                p = p + out * (0.55 * (-y).min(1.0));
            }
            if leap && len > 0 {
                let t = progress as f32 / len as f32;
                p.y += 1.8 * 4.0 * t * (1.0 - t);
            }
            if y > -0.01 {
                if let Some(c) = e.cell() {
                    p.y += self.geo.surface[c.index()].min(slab::GRAVEYARD_HEIGHT);
                }
            }
        }
        p
    }

    fn sync_enemies(&mut self, world: &mut World, state: &State) {
        let content = state.content();
        // Release slots whose enemy is gone.
        self.seen.clear();
        self.seen.extend(self.enemies.keys().copied().filter(|id| state.enemy(*id).is_none()));
        for k in 0..self.seen.len() {
            let id = self.seen[k];
            if let Some(i) = self.enemies.remove(&id) {
                release(world, &self.agent, &mut self.enemy_slots[i]);
            }
        }
        for e in state.enemies() {
            let pos = self.enemy_pos(e);
            let i = match self.enemies.get(&e.id) {
                Some(&i) => i,
                None => {
                    let def = content.enemy(e.kind);
                    let i = match self.enemy_slots.iter().position(|s| s.sim.is_none() && s.look == e.kind.0) {
                        Some(i) => i,
                        None => {
                            let n = self.enemy_slots.iter().filter(|s| s.look == e.kind.0).count();
                            let name = format!("{}_{:02}", def.id, n + 1);
                            let parts = looks::enemy_model(&def.id);
                            let props = props_obj(vec![("kind", Json::from(def.id.as_str()))]);
                            let id = ents::model(world, &self.agent, Some(self.enemies_g), &name, PARK, Vec3::ZERO, &parts, &["enemy", &def.id], props);
                            ents::place(world, &self.agent, id, None, None, Some(PARK_SCALE));
                            self.enemy_slots.push(Slot {
                                id,
                                look: e.kind.0,
                                sim: None,
                                prev: pos,
                                cur: pos,
                                prev_rot: Vec3::ZERO,
                                rot: Vec3::ZERO,
                                written: None,
                                tier: 0,
                                flies: def.flies(),
                            });
                            self.enemy_slots.len() - 1
                        }
                    };
                    let s = &mut self.enemy_slots[i];
                    s.sim = Some(e.id);
                    s.prev = pos;
                    s.cur = pos;
                    s.written = None;
                    let yaw = facing(Vec3::ZERO - pos, 0.0);
                    s.prev_rot = Vec3::new(0.0, yaw, 0.0);
                    s.rot = s.prev_rot;
                    ents::update(world, &self.agent, s.id, UpdateSpec {
                        scale: Some(ENEMY_SCALE),
                        props: Some(Json::obj([("enemy", Json::from(e.id))])),
                        remove_tags: vec!["idle".into()],
                        ..UpdateSpec::default()
                    });
                    self.enemies.insert(e.id, i);
                    i
                }
            };
            let s = &mut self.enemy_slots[i];
            s.prev = s.cur;
            s.prev_rot = s.rot;
            s.cur = pos;
            let d = s.cur - s.prev;
            let mut yaw = s.rot.y;
            if (d.x * d.x + d.z * d.z) > 1e-6 {
                yaw = facing(d, yaw);
            } else if pos.y < -0.5 && !s.flies {
                // Climbing: face the rock.
                yaw = facing(Vec3::new(-pos.x.signum() * f32::from(pos.x.abs() > pos.z.abs()), 0.0, -pos.z.signum() * f32::from(pos.z.abs() >= pos.x.abs())), yaw);
            }
            s.rot = Vec3::new(0.0, yaw, 0.0);
        }
    }

    fn sync_bolts(&mut self, world: &mut World, state: &State) {
        let content = state.content();
        self.seen.clear();
        // After the end the simulation stops, so its last projectiles would hang in the air: hide them.
        let live: &[Projectile] = if state.outcome() == sepulcra::Outcome::Running { state.projectiles() } else { &[] };
        // Both id-sorted lists, one merge pass (no per-bolt scan).
        self.live.clear();
        self.live.extend(live.iter().map(|p| p.id));
        self.live.sort_unstable();
        let ids = &self.live;
        self.seen.extend(self.bolts.keys().copied().filter(|id| ids.binary_search(id).is_err()));
        for k in 0..self.seen.len() {
            let id = self.seen[k];
            if let Some(i) = self.bolts.remove(&id) {
                release(world, &self.agent, &mut self.bolt_slots[i]);
            }
        }
        for p in live {
            let pos = bolt_pos(p);
            let i = match self.bolts.get(&p.id) {
                Some(&i) => i,
                None => {
                    let def = content.stone(p.stone);
                    let i = match self.bolt_slots.iter().position(|s| s.sim.is_none() && s.look == p.stone.0) {
                        Some(i) => i,
                        None => {
                            let n = self.bolt_slots.iter().filter(|s| s.look == p.stone.0).count();
                            let name = format!("{}_{:02}", def.id, n + 1);
                            let part = looks::projectile_model(&def.id);
                            let id = ents::part_named(world, &self.agent, self.bolts_g, &name, &part);
                            ents::place(world, &self.agent, id, Some(PARK), None, Some(PARK_SCALE));
                            self.bolt_slots.push(Slot {
                                id,
                                look: p.stone.0,
                                sim: None,
                                prev: pos,
                                cur: pos,
                                prev_rot: Vec3::ZERO,
                                rot: Vec3::ZERO,
                                written: None,
                                tier: p.tier,
                                flies: false,
                            });
                            self.bolt_slots.len() - 1
                        }
                    };
                    let part = looks::projectile_model(&def.id);
                    let s = &mut self.bolt_slots[i];
                    s.sim = Some(p.id);
                    s.tier = p.tier;
                    let [ox, oy, oz] = p.origin.to_f32();
                    s.prev = Vec3::new(ox, oy, oz);
                    s.cur = pos;
                    s.written = None;
                    s.rot = aim(s.cur - s.prev);
                    s.prev_rot = s.rot;
                    ents::place(world, &self.agent, s.id, None, None, Some(part.scale));
                    self.bolts.insert(p.id, i);
                    continue;
                }
            };
            let s = &mut self.bolt_slots[i];
            s.prev = s.cur;
            s.prev_rot = s.rot;
            s.cur = pos;
            let d = s.cur - s.prev;
            if d.length() > 1e-4 {
                s.rot = aim(d);
            }
        }
    }

    fn flourishes(&mut self, world: &mut World, state: &State, events: &[Event], now: f32) {
        let content = state.content();
        let map = &content.map;
        for ev in events {
            match *ev {
                Event::Died { pos, kind, .. } => {
                    let [x, y, z] = pos.to_f32();
                    let big = if content.enemy(kind).id == "colossus" { 5.0 } else { 1.0 };
                    self.flash(world, now, Vec3::new(x, y + 0.9 * big, z), 0.2 * big, 1.2 * big, 0.35, GRAVE_GLOW, 1.2, Some(0.3));
                }
                Event::Hit { enemy, .. } => {
                    if let Some(&i) = self.enemies.get(&enemy) {
                        let p = self.enemy_slots[i].cur + Vec3::new(0.0, 1.0, 0.0);
                        self.flash(world, now, p, 0.2, 0.7, 0.15, RUNE_PALE, 3.0, None);
                    }
                }
                Event::Exploded { projectile, pos } => {
                    let tier = self.bolts.get(&projectile).map_or(0, |&i| self.bolt_slots[i].tier);
                    let splash = content
                        .stones
                        .iter()
                        .find(|s| s.attack == Attack::Lob)
                        .and_then(|s| s.tiers.get(usize::from(tier)))
                        .map_or(1.8, |t| t.splash as f32 / 1000.0);
                    let [x, y, z] = pos.to_f32();
                    self.flash(world, now, Vec3::new(x, y + 0.3, z), 0.4, splash * 2.0, 0.35, FIRE_FLAME, 1.2, Some(0.3));
                }
                Event::Pulsed { stone, .. } => {
                    if let Some(s) = state.stone(stone) {
                        let range = content.stone(s.kind).tiers.get(usize::from(s.tier)).map_or(5.0, |t| t.range as f32 / 1000.0);
                        let (a, _) = self.geo.anchor(map, stone);
                        self.disc(world, now, a + Vec3::new(0.0, 0.25, 0.0), range, 0.45);
                    }
                }
                Event::StonePlaced { cell, .. } | Event::StoneUpgraded { cell, .. } => {
                    let (a, _) = self.geo.anchor(map, cell);
                    self.disc(world, now, a + Vec3::new(0.0, 0.15, 0.0), 1.6, 0.5);
                }
                Event::WallBroken { cell } => {
                    let (a, _) = self.geo.anchor(map, cell);
                    self.flash(world, now, a + Vec3::new(0.0, 0.5, 0.0), 0.6, 2.4, 0.5, BONE_SHADOW, 0.3, Some(0.6));
                }
                Event::TombstonesLost { .. } => {
                    let [x, _, z] = map.graveyard_centre.to_f32();
                    self.flash(world, now, Vec3::new(x, 1.5, z), 1.0, 6.0, 0.7, GRAVE_GLOW, 1.0, Some(0.2));
                }
                _ => {}
            }
        }
    }

    /// A growing glow sphere (`opacity`: glass when given).
    #[allow(clippy::too_many_arguments)]
    fn flash(&mut self, world: &mut World, now: f32, pos: Vec3, from: f32, to: f32, dur: f32, color: [f32; 3], emissive: f32, opacity: Option<f32>) {
        let (s0, s1) = (Vec3::new(from, from, from), Vec3::new(to, to, to));
        self.effect(world, now, pos, s0, s1, dur, color, emissive, opacity);
    }

    /// A flat ring of light spreading to `radius` (a rune pillar's pulse).
    fn disc(&mut self, world: &mut World, now: f32, pos: Vec3, radius: f32, dur: f32) {
        let (s0, s1) = (Vec3::new(0.6, 0.06, 0.6), Vec3::new(radius * 2.0, 0.06, radius * 2.0));
        self.effect(world, now, pos, s0, s1, dur, RUNE_GLOW, 1.0, Some(0.12));
    }

    #[allow(clippy::too_many_arguments)]
    fn effect(&mut self, world: &mut World, now: f32, pos: Vec3, from: Vec3, to: Vec3, dur: f32, color: [f32; 3], emissive: f32, opacity: Option<f32>) {
        let i = match self.fx.iter().position(|f| !f.active) {
            Some(i) => i,
            None if self.fx.len() < FX_POOL => {
                let name = format!("flash_{:02}", self.fx.len() + 1);
                let id = ents::part_named(world, &self.agent, self.fx_g, &name, &Part::sphere("", PARK, PARK_SCALE, color));
                self.fx.push(Fx { id, start: 0.0, dur: 0.0, from, to, active: false });
                self.fx.len() - 1
            }
            // All busy: reuse the oldest.
            None => self.fx.iter().enumerate().min_by(|a, b| a.1.start.total_cmp(&b.1.start)).map_or(0, |(i, _)| i),
        };
        let f = &mut self.fx[i];
        *f = Fx { id: f.id, start: now, dur, from, to, active: true };
        let look = LookPatch {
            material: Some(if opacity.is_some() { Material::Glass } else { Material::Lit }),
            opacity: Some(opacity.unwrap_or(1.0)),
            emissive: Some(emissive),
            ..LookPatch::default()
        };
        ents::update(world, &self.agent, f.id, UpdateSpec { position: Some(pos), scale: Some(from), color: Some(color), look, ..UpdateSpec::default() });
    }

    // ---- per frame ----

    /// Moves enemies and projectiles to `alpha` of the way between the last two ticks and
    /// advances the flashes. `now` is presentation time in seconds.
    pub fn animate(&mut self, world: &mut World, alpha: f32, now: f32) {
        let a = alpha.clamp(0.0, 1.0);
        for s in self.enemy_slots.iter_mut().chain(self.bolt_slots.iter_mut()) {
            if s.sim.is_none() {
                continue;
            }
            let mut p = lerp(s.prev, s.cur, a);
            if s.flies {
                p.y += 0.18 * (now * 2.6 + s.sim.unwrap_or(0) as f32).sin();
            }
            let r = Vec3::new(lerp_angle(s.prev_rot.x, s.rot.x, a), lerp_angle(s.prev_rot.y, s.rot.y, a), 0.0);
            if s.written.is_some_and(|(wp, wr)| (wp - p).length() < 1e-3 && (wr - r).length() < 0.05) {
                continue;
            }
            s.written = Some((p, r));
            ents::place(world, &self.agent, s.id, Some(p), Some(r), None);
        }
        for f in &mut self.fx {
            if !f.active {
                continue;
            }
            // A flash born in a tick may start a little after the frame being drawn (frames lag
            // the newest tick by up to one tick): it shows at its first size until then.
            let t = ((now - f.start) / f.dur.max(1e-3)).clamp(0.0, 1.0);
            if t >= 1.0 {
                f.active = false;
                ents::place(world, &self.agent, f.id, Some(PARK), None, Some(PARK_SCALE));
                continue;
            }
            let k = ease_out(t);
            let scale = lerp(f.from, f.to, k);
            ents::place(world, &self.agent, f.id, None, None, Some(scale));
        }
    }

    /// Shows the build cursor on a cell (valid or not), or hides it.
    pub fn show_cursor(&mut self, world: &mut World, state: &State, look: CursorLook) {
        if look == self.cursor_shown {
            return;
        }
        self.cursor_shown = look;
        let (cell, color) = match look {
            CursorLook::Hidden => {
                ents::place(world, &self.agent, self.cursor, Some(PARK), Some(Vec3::ZERO), Some(PARK_SCALE));
                return;
            }
            CursorLook::Valid(c) => (c, RUNE_GLOW),
            CursorLook::Invalid(c) => (c, INVALID),
        };
        let map = &state.content().map;
        let (anchor, normal) = self.geo.anchor(map, cell);
        let c = map.cell(cell);
        let w = self.geo.cell * 0.94;
        let (pos, scale) = if normal.y > 0.5 {
            (anchor + Vec3::new(0.0, 0.03, 0.0), Vec3::new(w, 0.06, w))
        } else {
            let h = self.geo.row * 0.94;
            match c.face {
                Face::North | Face::South => (anchor + normal * 0.03, Vec3::new(w, h, 0.06)),
                _ => (anchor + normal * 0.03, Vec3::new(0.06, h, w)),
            }
        };
        ents::place(world, &self.agent, self.cursor, Some(pos), Some(Vec3::ZERO), Some(scale));
        ents::recolor(world, &self.agent, self.cursor, color, LookPatch::default());
    }

    /// A ring of dots showing a range around a cell (`None` hides it).
    pub fn show_range(&mut self, world: &mut World, state: &State, ring: Option<(CellId, f32)>) {
        let key = ring.map(|(c, r)| (u32::from(c.0), r.to_bits(), 0, 0));
        if key == self.ring_shown {
            return;
        }
        self.ring_shown = key;
        let Some((cell, radius)) = ring else {
            for &d in &self.ring {
                ents::place(world, &self.agent, d, Some(PARK), None, Some(PARK_SCALE));
            }
            return;
        };
        let (anchor, _) = self.geo.anchor(&state.content().map, cell);
        for (i, &d) in self.ring.iter().enumerate() {
            let a = i as f32 / RING_DOTS as f32 * std::f32::consts::TAU;
            let p = anchor + Vec3::new(a.cos() * radius, 0.25, a.sin() * radius);
            ents::place(world, &self.agent, d, Some(p), Some(Vec3::new(0.0, -a.to_degrees(), 0.0)), Some(Vec3::new(0.16, 0.16, 0.5)));
        }
    }

    /// Counts for status lines and perf notes.
    pub fn counts(&self) -> Json {
        Json::obj([
            ("enemy_slots", Json::from(self.enemy_slots.len())),
            ("bolt_slots", Json::from(self.bolt_slots.len())),
            ("flash_slots", Json::from(self.fx.len())),
            ("lit_stones", Json::from(self.stones.values().filter(|v| v.lit).count())),
        ])
    }

    /// The `ui` group's id (cursor and range ring).
    pub fn ui_group(&self) -> u64 {
        self.ui_g
    }

    /// Whether a cell holds something buildable for the cursor (fragment or ledge).
    pub fn buildable(state: &State, cell: CellId) -> bool {
        matches!(state.content().map.cell(cell).kind, CellKind::Fragment | CellKind::Ledge)
    }
}

/// Parks a slot's entity and frees it for the next enemy or projectile of its look.
fn release(world: &mut World, agent: &str, s: &mut Slot) {
    s.sim = None;
    s.written = None;
    ents::update(world, agent, s.id, UpdateSpec {
        position: Some(PARK),
        scale: Some(PARK_SCALE),
        props: Some(Json::obj([("enemy", Json::Null)])),
        add_tags: vec!["idle".into()],
        ..UpdateSpec::default()
    });
}

/// Yaw (degrees about +Y) that turns +Z towards `d`; `keep` when `d` has no horizontal part.
fn facing(d: Vec3, keep: f32) -> f32 {
    if d.x.abs() + d.z.abs() < 1e-6 {
        keep
    } else {
        d.x.atan2(d.z).to_degrees()
    }
}

/// Rotation that points +Z along `d`.
fn aim(d: Vec3) -> Vec3 {
    let horiz = (d.x * d.x + d.z * d.z).sqrt();
    Vec3::new(-d.y.atan2(horiz).to_degrees(), facing(d, 0.0), 0.0)
}

/// Where to draw a projectile: lobs arc over their straight simulated path.
fn bolt_pos(p: &Projectile) -> Vec3 {
    let [x, y, z] = p.world_pos_f32();
    let mut pos = Vec3::new(x, y, z);
    if !p.homing {
        let [ox, _, oz] = p.origin.to_f32();
        let [ax, _, az] = p.aim.to_f32();
        let total = ((ax - ox).powi(2) + (az - oz).powi(2)).sqrt();
        if total > 0.1 {
            let done = ((x - ox).powi(2) + (z - oz).powi(2)).sqrt();
            let t = (done / total).clamp(0.0, 1.0);
            pos.y += 0.35 * total * 4.0 * t * (1.0 - t);
        }
    }
    pos
}
