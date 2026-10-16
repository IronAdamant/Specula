//! The live presentation: mirrors the simulation [`State`] into the engine's world.
//!
//! Everything is **reconciled** against the state once per tick (stones, walls, roots,
//! tombstones, enemies, projectiles), so a restart, a loaded replay or an agent's command never
//! leaves a stale entity behind. Events only add flourishes (flashes, death clips) on top.
//!
//! **What is saved.** The authored graveyard and what the player built (the slab, stones, walls,
//! roots, tombstones, under `/slab` and `/units`) are ordinary entities: saved with the world,
//! journaled (agents see who built what) and hashed. Everything that moves (enemies, the dying,
//! projectiles, flashes, the cursor; under `/live` and `/cursor`) is **transient**: game
//! presentation derived from the simulation, never saved, journaled or hashed, so moving it
//! every frame costs no journal records and never dirties the world or a draw cache.
//!
//! **Models.** Stones, walls, tombstones and creatures are procedural recipes from
//! `games/sepulcra/assets/recipes/` (see [`looks`]): one shared GPU mesh per recipe, parameters
//! and seed, drawn at the level of detail that fits the screen. Creatures are rigged and skinned
//! on the GPU; each plays a clip of its anim set (the world's `anim` component) chosen from what
//! it does (walk, attack a wall, leap, dive, summon, die), crossfading on every change, at a rate
//! that follows the game's speed and the creature's slow.
//!
//! Cost (foundation F3): structure changes touch only what changed; moving things are written
//! once per frame, interpolated between the last two ticks with the frame's `alpha`, and only
//! when they moved. Animation is a pure function of world time: a clip is written only when what
//! the creature does (or how fast) changes.

use crate::ents;
use crate::looks::{self, palette::*, Act, Creature, Part};
use crate::slab::{self, SlabGeo};
use sepulcra::content::Attack;
use sepulcra::map::{CellKind, Face};
use sepulcra::sim::{EnemyId, ProjectileId};
use sepulcra::{CellId, Enemy, Event, Motion, Projectile, State};
use specula::world::{AnimSpec, EntityKey, Layer, LookPatch, SpawnSpec, UpdateSpec, World};
use specula_core::math::Vec3;
use specula_core::Json;
use specula_gpu::Material;
use std::collections::BTreeMap;

/// Most point lights the game uses (the engine renders 16; leave room for agents' lights).
const MAX_GAME_LIGHTS: usize = 12;
/// Flashes alive at once (the oldest is reused past this).
const FX_MAX: usize = 48;
const RING_DOTS: usize = 28;
/// Seconds a crossfade between two clips takes.
const CROSSFADE_S: f32 = 0.2;
/// Seconds a body stays after its death clip ends.
const LINGER_S: f32 = 0.6;

/// An enemy on the slab.
#[derive(Clone)]
struct Mob {
    id: u64,
    creature: Creature,
    prev: Vec3,
    cur: Vec3,
    prev_rot: Vec3,
    rot: Vec3,
    /// What was last written to the world.
    written: Option<(Vec3, Vec3)>,
    flies: bool,
    /// What its clip shows, and the clip rate last written.
    act: Act,
    rate: f32,
    /// A one-shot clip (a summon) plays until this presentation time.
    busy_until: f32,
    /// Its own phase, so a pack doesn't walk in step.
    phase: f32,
}

/// A body playing its death clip until `until` (presentation seconds).
#[derive(Clone, Copy)]
struct Corpse {
    id: u64,
    until: f32,
}

/// A projectile in flight.
#[derive(Clone)]
struct Bolt {
    id: u64,
    prev: Vec3,
    cur: Vec3,
    prev_rot: Vec3,
    rot: Vec3,
    /// Tier of the stone that fired it (for splash size).
    tier: u8,
}

#[derive(Clone)]
struct StoneView {
    id: u64,
    kind: u8,
    tier: u8,
    lit: bool,
}

#[derive(Clone)]
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
}

/// What the cursor shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CursorLook {
    Hidden,
    Valid(CellId),
    Invalid(CellId),
    /// A stone or wall the player selected (its panel is open).
    Selected(CellId),
}

/// Cloneable so the game can checkpoint it (see `Game::checkpoint`): the entity ids it caches
/// must go back with the world when a rejected game tool is rolled back.
#[derive(Clone)]
pub struct Scene {
    pub geo: SlabGeo,
    agent: String,
    stones_g: u64,
    walls_g: u64,
    roots_g: u64,
    tombs_g: u64,
    enemies_g: u64,
    bolts_g: u64,
    fx_g: u64,
    stones: BTreeMap<CellId, StoneView>,
    walls: BTreeMap<CellId, WallView>,
    roots: BTreeMap<CellId, u64>,
    enemies: BTreeMap<EnemyId, Mob>,
    corpses: Vec<Corpse>,
    bolts: BTreeMap<ProjectileId, Bolt>,
    fx: Vec<Fx>,
    tombs: Vec<(u64, Vec3, Vec3, bool)>,
    cursor: u64,
    cursor_shown: CursorLook,
    ring_g: u64,
    ring: Vec<u64>,
    ring_shown: Option<(u32, u32)>,
    /// How fast the game runs (0 paused, else its speed): creatures' clips follow it.
    pace: f32,
    /// Scratch, reused per tick.
    seen: Vec<u32>,
    /// Scratch: live projectile ids, sorted.
    live: Vec<ProjectileId>,
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

fn tint(c: [f32; 3], k: f32) -> [f32; 3] {
    [(c[0] * k).min(1.0), (c[1] * k).min(1.0), (c[2] * k).min(1.0)]
}

impl Scene {
    /// Builds the slab and the empty groups the game fills (replacing any from an earlier run).
    pub fn build(world: &mut World, agent: &str, state: &State) -> Scene {
        let map = &state.content().map;
        let geo = SlabGeo::new(map);
        slab::build(world, agent, map, &geo);
        // `fx` and `ui` are where earlier versions kept their pools.
        for name in ["units", "live", "cursor", "fx", "ui"] {
            if let Some(old) = ents::top_level(world, name) {
                ents::delete(world, old);
            }
        }
        let units = ents::group(world, agent, None, "units", Vec3::ZERO, Vec3::ZERO, &["game"], Json::Null);
        let sub = |world: &mut World, parent: u64, name: &str| ents::group(world, agent, Some(parent), name, Vec3::ZERO, Vec3::ZERO, &[], Json::Null);
        let (stones_g, walls_g, roots_g, tombs_g) = (sub(world, units, "stones"), sub(world, units, "walls"), sub(world, units, "roots"), sub(world, units, "tombstones"));
        let live = ents::transient_group(world, agent, "live", Layer::World, &["game", "transient"]);
        let (enemies_g, bolts_g, fx_g) = (sub(world, live, "enemies"), sub(world, live, "bolts"), sub(world, live, "fx"));
        let ui = ents::transient_group(world, agent, "cursor", Layer::Ui, &["ui"]);
        let cursor_part = Part::cube("cell", Vec3::ZERO, Vec3::ONE, RUNE_GLOW).glow(1.4);
        let cursor = ents::part(world, agent, ui, &cursor_part);
        ents::show(world, agent, cursor, false);
        let ring_g = sub(world, ui, "range");
        let ring = (0..RING_DOTS)
            .map(|i| ents::part_named(world, agent, ring_g, &format!("dot_{i:02}"), &Part::cube("", Vec3::ZERO, Vec3::new(0.16, 0.16, 0.5), RUNE_PALE).glow(2.0)))
            .collect();
        ents::show(world, agent, ring_g, false);
        let mut scene = Scene {
            geo,
            agent: agent.to_owned(),
            stones_g,
            walls_g,
            roots_g,
            tombs_g,
            enemies_g,
            bolts_g,
            fx_g,
            stones: BTreeMap::new(),
            walls: BTreeMap::new(),
            roots: BTreeMap::new(),
            enemies: BTreeMap::new(),
            corpses: Vec::new(),
            bolts: BTreeMap::new(),
            fx: Vec::new(),
            tombs: Vec::new(),
            cursor,
            cursor_shown: CursorLook::Hidden,
            ring_g,
            ring,
            ring_shown: None,
            pace: 1.0,
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
            let x = cx + (c as f32 - (in_row as f32 - 1.0) / 2.0) * 1.7 + 0.15 * (((i * 53) % 7) as f32 / 3.0 - 1.0);
            let z = cz + (r as f32 - (rows as f32 - 1.0) / 2.0) * 2.2;
            let pos = Vec3::new(x, slab::GRAVEYARD_HEIGHT, z);
            let rot = Vec3::new(0.0, ((i * 37) % 13) as f32 - 6.0, 0.0);
            let props = Json::obj([("index", Json::from(i + 1))]);
            let spec = ents::model_spec(self.tombs_g, &format!("tomb_{:02}", i + 1), looks::tombstone_spec(i), pos, rot, 1.25, &["tombstone"], props);
            let id = ents::spawn(world, &self.agent, spec);
            self.tombs.push((id, pos, rot, false));
        }
    }

    // ---- per tick ----

    /// Brings the world in line with `state` (structure and the motion targets), and turns this
    /// tick's `events` into flourishes. `now` is presentation time in seconds.
    pub fn sync(&mut self, world: &mut World, state: &State, events: &[Event], now: f32) {
        self.sync_structure(world, state, events, now);
        self.sync_enemies(world, state, events, now);
        self.sync_bolts(world, state);
    }

    /// Everything but motion: stones, walls, roots, tombstones and flashes. Cheap enough to run
    /// right after a command, so a build shows at once even while the clock is paused.
    pub fn sync_structure(&mut self, world: &mut World, state: &State, events: &[Event], now: f32) {
        // Flourishes first: they read the enemies and projectiles that are about to go.
        self.flourishes(world, state, events, now);
        self.sync_stones(world, state);
        self.sync_walls(world, state);
        self.sync_roots(world, state);
        self.sync_tombstones(world, state);
    }

    fn sync_stones(&mut self, world: &mut World, state: &State) {
        let content = state.content();
        let gone: Vec<CellId> = self.stones.iter().filter(|(c, v)| state.stone(**c).is_none_or(|s| s.kind.0 != v.kind)).map(|(c, _)| *c).collect();
        for c in gone {
            if let Some(v) = self.stones.remove(&c) {
                ents::delete(world, v.id);
            }
        }
        for (cell, s) in state.stones() {
            if self.stones.get(&cell).is_some_and(|v| v.tier == s.tier) {
                continue;
            }
            let def = content.stone(s.kind);
            let lights = self.stones.iter().filter(|(c, v)| **c != cell && v.lit).count();
            let light = looks::stone_light(&def.id, s.tier).filter(|_| lights < MAX_GAME_LIGHTS);
            let name = content.map.cell_name(cell);
            let props = Json::obj([
                ("cell", Json::from(name.as_str())),
                ("stone", Json::from(def.id.as_str())),
                ("tier", Json::from(u32::from(s.tier) + 1)),
            ]);
            let model = looks::stone_spec(&def.id, s.tier);
            if let Some(v) = self.stones.get_mut(&cell) {
                // Carved further: the same entity, a new tier (its history stays in the journal).
                let change = UpdateSpec { params: Some(model.params.clone()), light: Some(light), props: Some(props), ..UpdateSpec::default() };
                ents::update(world, &self.agent, v.id, change);
                (v.tier, v.lit) = (s.tier, light.is_some());
                continue;
            }
            let (anchor, _) = self.geo.anchor(&content.map, cell);
            // A little turn per cell, so rows of stones don't look stamped.
            let yaw = ((cell.0 as u32 * 7919) % 11) as f32 - 5.0;
            let mut spec = ents::model_spec(self.stones_g, &name, model, anchor, Vec3::new(0.0, yaw, 0.0), 1.0, &["stone", &def.id], props);
            spec.light = light;
            let id = ents::spawn(world, &self.agent, spec);
            self.stones.insert(cell, StoneView { id, kind: s.kind.0, tier: s.tier, lit: light.is_some() });
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
            // Damaged walls lose courses and darken.
            let k = 0.4 + 0.15 * f32::from(bucket);
            let scale = Vec3::new(1.0, k, 1.0);
            let color = tint([1.0; 3], 0.55 + 0.45 * k);
            let props = Json::obj([("hp", Json::from(w.hp / 1000)), ("max_hp", Json::from(w.max_hp / 1000))]);
            if let Some(v) = self.walls.get_mut(&cell) {
                ents::update(world, &self.agent, v.id, UpdateSpec { scale: Some(scale), color: Some(color), props: Some(props), ..UpdateSpec::default() });
                v.bucket = bucket;
                continue;
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
            let mut props = props;
            props.set("cell", Json::from(name.as_str()));
            let mut spec = ents::model_spec(self.walls_g, &name, looks::wall_spec(u32::from(cell.0) * 13 + 5), anchor, rot, 1.0, &["wall"], props);
            spec.scale = scale;
            spec.color = color;
            let id = ents::spawn(world, &self.agent, spec);
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
            // Thin roots creeping over the moss (children of the flat patch: its scale flattens them).
            for (name, a, off) in [("root_a", 25.0, -0.18), ("root_b", 80.0, 0.12), ("root_c", 140.0, 0.25)] {
                let root = Part::cube(name, Vec3::new(off, 0.9, 0.05), Vec3::new(0.06, 1.0, 0.85), ROOTS).rot(0.0, a + k * 23.0, 0.0).rough(1.0);
                ents::part(world, &self.agent, g, &root);
            }
            self.roots.insert(c.id, g);
        }
    }

    fn sync_tombstones(&mut self, world: &mut World, state: &State) {
        let left = state.tombstones().max(0) as usize;
        for (i, (id, pos, rot, down)) in self.tombs.iter_mut().enumerate() {
            let should = i >= left;
            if *down == should {
                continue;
            }
            *down = should;
            // The dead rose here: the stone lies toppled, tinted with the enemy's green.
            let (p, r, c) = if should {
                (*pos + Vec3::new(0.0, -0.05, 0.3), Vec3::new(-80.0, rot.y + 9.0, 4.0), hex_tint(GRAVE_DEEP))
            } else {
                (*pos, *rot, [1.0; 3])
            };
            let tags = if should { vec!["raised".to_owned()] } else { Vec::new() };
            let untag = if should { Vec::new() } else { vec!["raised".to_owned()] };
            let change = UpdateSpec { position: Some(p), rotation_deg: Some(r), color: Some(c), add_tags: tags, remove_tags: untag, ..UpdateSpec::default() };
            ents::update(world, &self.agent, *id, change);
        }
    }

    fn enemy_pos(&self, e: &Enemy) -> Vec3 {
        let [x, y, z] = e.world_pos_f32();
        let mut p = Vec3::new(x, y, z);
        if let Motion::Ground { progress, len, leap, .. } = e.motion {
            if y < -0.01 {
                // On a cliff: stand on the rock, not inside it.
                let out = if x.abs() - self.geo.half_x > z.abs() - self.geo.half_z { Vec3::new(x.signum(), 0.0, 0.0) } else { Vec3::new(0.0, 0.0, z.signum()) };
                p = p + out * (0.6 * (-y).min(1.0));
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

    /// What an enemy's clip should show this tick.
    fn act_of(e: &Enemy) -> Act {
        match e.motion {
            Motion::Ground { leap: true, .. } => Act::Special,
            Motion::Air { diving: true, .. } => Act::Special,
            _ if e.attacking.is_some() => Act::Attack,
            Motion::Ground { len: 0, .. } => Act::Idle,
            _ => Act::Move,
        }
    }

    fn sync_enemies(&mut self, world: &mut World, state: &State, events: &[Event], now: f32) {
        let content = state.content();
        // Gone without dying (they reached the graveyard, or were stranded): no body.
        self.seen.clear();
        self.seen.extend(self.enemies.keys().copied().filter(|id| state.enemy(*id).is_none()));
        for k in 0..self.seen.len() {
            if let Some(m) = self.enemies.remove(&self.seen[k]) {
                ents::delete(world, m.id);
            }
        }
        // The colossus raises shamblers: it plays its summon while they climb out.
        for ev in events {
            if let Event::Spawned { pos, .. } = *ev {
                let [x, _, z] = pos.to_f32();
                for m in self.enemies.values_mut() {
                    let d = m.cur - Vec3::new(x, m.cur.y, z);
                    if m.creature.recipe == looks::creature("colossus").recipe && d.length() < 5.0 {
                        m.busy_until = now + m.creature.special.duration / m.creature.special.rate.max(0.1);
                    }
                }
            }
        }
        let tick = state.tick();
        let world_now = world.world_time();
        for e in state.enemies() {
            let pos = self.enemy_pos(e);
            if !self.enemies.contains_key(&e.id) {
                let def = content.enemy(e.kind);
                let creature = looks::creature(&def.id);
                let phase = (e.id % 17) as f32 * 0.137;
                let act = Scene::act_of(e);
                let clip = creature.clip(act);
                let rate = self.rate(clip.rate, e, tick);
                let yaw = facing(Vec3::ZERO - pos, 0.0);
                let props = Json::obj([("enemy", Json::from(e.id)), ("kind", Json::from(def.id.as_str()))]);
                let model = specula::world::ModelSpec::new(creature.recipe);
                let rot = Vec3::new(0.0, yaw, 0.0);
                let mut spec = ents::model_spec(self.enemies_g, &format!("{}_{}", def.id, e.id), model, pos, rot, creature.scale, &["enemy", &def.id], props);
                spec.anim = Some(AnimSpec {
                    speed: rate,
                    looping: Some(clip.looping),
                    time_offset: if clip.looping { phase } else { 0.0 },
                    start: Some(world_now),
                    ..AnimSpec::new(clip.name)
                });
                let id = ents::spawn(world, &self.agent, spec);
                let flies = def.flies();
                self.enemies.insert(e.id, Mob { id, creature, prev: pos, cur: pos, prev_rot: rot, rot, written: Some((pos, rot)), flies, act, rate, busy_until: 0.0, phase });
                continue;
            }
            let busy = self.enemies.get(&e.id).is_some_and(|m| m.busy_until > now);
            let act = if busy { Act::Special } else { Scene::act_of(e) };
            let base = self.enemies[&e.id].creature.clip(act).rate;
            let rate = self.rate(base, e, tick);
            let Some(m) = self.enemies.get_mut(&e.id) else { continue };
            m.prev = m.cur;
            m.prev_rot = m.rot;
            m.cur = pos;
            let d = m.cur - m.prev;
            let mut yaw = m.rot.y;
            if (d.x * d.x + d.z * d.z) > 1e-6 {
                yaw = facing(d, yaw);
            } else if pos.y < -0.5 && !m.flies {
                // Climbing: face the rock.
                yaw = facing(Vec3::new(-pos.x.signum() * f32::from(pos.x.abs() > pos.z.abs()), 0.0, -pos.z.signum() * f32::from(pos.z.abs() >= pos.x.abs())), yaw);
            }
            m.rot = Vec3::new(0.0, yaw, 0.0);
            if act != m.act || (rate - m.rate).abs() > 0.02 {
                play(world, &self.agent, m, act, rate, world_now);
            }
        }
    }

    /// A clip's rate for an enemy: its own rate, slowed with the enemy, at the game's pace.
    fn rate(&self, base: f32, e: &Enemy, tick: u32) -> f32 {
        let slow = if e.is_slowed(tick) { (1000 - e.slow.clamp(0, 900)) as f32 / 1000.0 } else { 1.0 };
        (base * slow * self.pace).clamp(0.0, 8.0)
    }

    /// Sets how fast the game runs (0 while paused, else 1, 2 or 4 steps per tick): every
    /// creature's clip follows, keeping its place in the clip.
    pub fn set_pace(&mut self, world: &mut World, pace: f32) {
        if pace == self.pace {
            return;
        }
        let old = self.pace;
        self.pace = pace;
        let now = world.world_time();
        for m in self.enemies.values_mut() {
            let rate = if old > 0.0 { m.rate / old * pace } else { m.creature.clip(m.act).rate * pace };
            let act = m.act;
            play(world, &self.agent, m, act, rate.clamp(0.0, 8.0), now);
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
            if let Some(b) = self.bolts.remove(&self.seen[k]) {
                ents::delete(world, b.id);
            }
        }
        for p in live {
            let pos = bolt_pos(p);
            if let Some(b) = self.bolts.get_mut(&p.id) {
                b.prev = b.cur;
                b.prev_rot = b.rot;
                b.cur = pos;
                let d = b.cur - b.prev;
                if d.length() > 1e-4 {
                    b.rot = aim(d);
                }
                continue;
            }
            let def = content.stone(p.stone);
            let [ox, oy, oz] = p.origin.to_f32();
            let from = Vec3::new(ox, oy, oz);
            let rot = aim(pos - from);
            let mut part = looks::projectile_model(&def.id);
            part.pos = from;
            part.rot = rot;
            let id = ents::part_named(world, &self.agent, self.bolts_g, &format!("{}_{}", def.id, p.id), &part);
            self.bolts.insert(p.id, Bolt { id, prev: from, cur: pos, prev_rot: rot, rot, tier: p.tier });
        }
    }

    fn flourishes(&mut self, world: &mut World, state: &State, events: &[Event], now: f32) {
        let content = state.content();
        let map = &content.map;
        let world_now = world.world_time();
        for ev in events {
            match *ev {
                Event::Died { enemy, pos, kind } => {
                    let [x, y, z] = pos.to_f32();
                    let big = if content.enemy(kind).id == "colossus" { 4.0 } else { 1.0 };
                    self.flash(world, now, Vec3::new(x, y + 0.9 * big, z), 0.2 * big, 1.1 * big, 0.35, GRAVE_GLOW, 1.0, Some(0.25));
                    // The body plays its death where it fell, then goes.
                    if let Some(mut m) = self.enemies.remove(&enemy) {
                        let pace = self.pace.max(1.0);
                        let clip = m.creature.death;
                        play(world, &self.agent, &mut m, Act::Death, (clip.rate * pace).min(8.0), world_now);
                        ents::place(world, &self.agent, m.id, Some(m.cur), Some(m.rot), None);
                        ents::update(world, &self.agent, m.id, UpdateSpec { add_tags: vec!["dead".into()], ..UpdateSpec::default() });
                        self.corpses.push(Corpse { id: m.id, until: now + clip.duration / (clip.rate * pace) + LINGER_S });
                    }
                }
                Event::Hit { enemy, .. } => {
                    if let Some(m) = self.enemies.get(&enemy) {
                        let p = m.cur + Vec3::new(0.0, 1.1, 0.0);
                        self.flash(world, now, p, 0.15, 0.6, 0.15, RUNE_PALE, 3.0, None);
                    }
                }
                Event::Exploded { projectile, pos } => {
                    let tier = self.bolts.get(&projectile).map_or(0, |b| b.tier);
                    let splash = content
                        .stones
                        .iter()
                        .find(|s| s.attack == Attack::Lob)
                        .and_then(|s| s.tiers.get(usize::from(tier)))
                        .map_or(1.8, |t| t.splash as f32 / 1000.0);
                    let [x, y, z] = pos.to_f32();
                    // A low burst of fire over the splash: wide and flat, a hot core that dies fast.
                    let flat = |k: f32| Vec3::new(k, k * 0.35, k);
                    self.effect(world, now, Vec3::new(x, y + 0.15, z), flat(0.5), flat(splash * 2.0), 0.3, FIRE_FLAME, 1.6, Some(0.3));
                    self.effect(world, now, Vec3::new(x, y + 0.1, z), flat(0.3), flat(splash * 0.9), 0.18, FIRE_CORE, 3.5, None);
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
        let mut part = Part::sphere("", pos, from, color).glow(emissive);
        if let Some(o) = opacity {
            part = part.glass(o);
        }
        if self.fx.len() >= FX_MAX {
            // All busy: the oldest becomes this one.
            let i = self.fx.iter().enumerate().min_by(|a, b| a.1.start.total_cmp(&b.1.start)).map_or(0, |(i, _)| i);
            let f = &mut self.fx[i];
            *f = Fx { id: f.id, start: now, dur, from, to };
            let look = LookPatch {
                material: Some(if opacity.is_some() { Material::Glass } else { Material::Lit }),
                opacity: Some(opacity.unwrap_or(1.0)),
                emissive: Some(emissive),
                ..LookPatch::default()
            };
            ents::update(world, &self.agent, f.id, UpdateSpec { position: Some(pos), scale: Some(from), color: Some(color), look, ..UpdateSpec::default() });
            return;
        }
        let mut spec = SpawnSpec::new(part.shape);
        spec.parent = Some(EntityKey::Id(self.fx_g));
        spec.position = pos;
        spec.scale = from;
        spec.color = color;
        spec.look = part.look;
        let id = ents::spawn(world, &self.agent, spec);
        self.fx.push(Fx { id, start: now, dur, from, to });
    }

    // ---- per frame ----

    /// Moves enemies and projectiles to `alpha` of the way between the last two ticks, advances
    /// the flashes and clears away finished bodies. `now` is presentation time in seconds.
    pub fn animate(&mut self, world: &mut World, alpha: f32, now: f32) {
        let a = alpha.clamp(0.0, 1.0);
        for (eid, m) in self.enemies.iter_mut() {
            let mut p = lerp(m.prev, m.cur, a);
            if m.flies {
                p.y += 0.18 * (now * 2.6 + *eid as f32).sin();
            }
            let r = Vec3::new(lerp_angle(m.prev_rot.x, m.rot.x, a), lerp_angle(m.prev_rot.y, m.rot.y, a), 0.0);
            if m.written.is_some_and(|(wp, wr)| (wp - p).length() < 1e-3 && (wr - r).length() < 0.05) {
                continue;
            }
            m.written = Some((p, r));
            ents::place(world, &self.agent, m.id, Some(p), Some(r), None);
        }
        for b in self.bolts.values() {
            let p = lerp(b.prev, b.cur, a);
            let r = Vec3::new(lerp_angle(b.prev_rot.x, b.rot.x, a), lerp_angle(b.prev_rot.y, b.rot.y, a), 0.0);
            ents::place(world, &self.agent, b.id, Some(p), Some(r), None);
        }
        let agent = &self.agent;
        self.fx.retain(|f| {
            // A flash born in a tick may start a little after the frame being drawn (frames lag
            // the newest tick by up to one tick): it shows at its first size until then.
            let t = ((now - f.start) / f.dur.max(1e-3)).clamp(0.0, 1.0);
            if t >= 1.0 {
                ents::delete(world, f.id);
                return false;
            }
            ents::place(world, agent, f.id, None, None, Some(lerp(f.from, f.to, ease_out(t))));
            true
        });
        self.corpses.retain(|c| {
            if now >= c.until {
                ents::delete(world, c.id);
                return false;
            }
            true
        });
    }

    /// Shows the build cursor on a cell (valid, invalid or selected), or hides it.
    pub fn show_cursor(&mut self, world: &mut World, state: &State, look: CursorLook) {
        if look == self.cursor_shown {
            return;
        }
        self.cursor_shown = look;
        let (cell, color) = match look {
            CursorLook::Hidden => {
                ents::show(world, &self.agent, self.cursor, false);
                return;
            }
            CursorLook::Valid(c) => (c, RUNE_GLOW),
            CursorLook::Invalid(c) => (c, INVALID),
            CursorLook::Selected(c) => (c, RUNE_PALE),
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
        let change = UpdateSpec { position: Some(pos), scale: Some(scale), color: Some(color), visible: Some(true), ..UpdateSpec::default() };
        ents::update(world, &self.agent, self.cursor, change);
    }

    /// A ring of dots showing a range around a cell (`None` hides it).
    pub fn show_range(&mut self, world: &mut World, state: &State, ring: Option<(CellId, f32)>) {
        let key = ring.map(|(c, r)| (u32::from(c.0), r.to_bits()));
        if key == self.ring_shown {
            return;
        }
        self.ring_shown = key;
        let Some((cell, radius)) = ring else {
            ents::show(world, &self.agent, self.ring_g, false);
            return;
        };
        let (anchor, _) = self.geo.anchor(&state.content().map, cell);
        let change = UpdateSpec { position: Some(anchor), visible: Some(true), ..UpdateSpec::default() };
        ents::update(world, &self.agent, self.ring_g, change);
        for (i, &d) in self.ring.iter().enumerate() {
            let a = i as f32 / RING_DOTS as f32 * std::f32::consts::TAU;
            let p = Vec3::new(a.cos() * radius, 0.25, a.sin() * radius);
            ents::place(world, &self.agent, d, Some(p), Some(Vec3::new(0.0, -a.to_degrees(), 0.0)), None);
        }
    }

    /// Counts for status lines and perf notes.
    pub fn counts(&self) -> Json {
        Json::obj([
            ("enemies", Json::from(self.enemies.len())),
            ("bodies", Json::from(self.corpses.len())),
            ("bolts", Json::from(self.bolts.len())),
            ("flashes", Json::from(self.fx.len())),
            ("lit_stones", Json::from(self.stones.values().filter(|v| v.lit).count())),
        ])
    }

    /// Where an enemy is drawn now (for the HUD's threat markers).
    pub fn enemy_at(&self, enemy: EnemyId) -> Option<Vec3> {
        self.enemies.get(&enemy).map(|m| m.written.map_or(m.cur, |(p, _)| p))
    }

    /// Whether a cell holds something buildable for the cursor (fragment or ledge).
    pub fn buildable(state: &State, cell: CellId) -> bool {
        matches!(state.content().map.cell(cell).kind, CellKind::Fragment | CellKind::Ledge)
    }
}

/// A colour multiplier that tints white towards `c` (models are white × their vertex colours).
fn hex_tint(c: [f32; 3]) -> [f32; 3] {
    [0.55 + 0.45 * c[0], 0.55 + 0.45 * c[1], 0.55 + 0.45 * c[2]]
}

/// Plays what `act` shows at `rate`: a new clip crossfades in; the same clip at a new rate keeps
/// its place (no jump).
fn play(world: &mut World, agent: &str, m: &mut Mob, act: Act, rate: f32, now: f64) {
    let clip = m.creature.clip(act);
    let same_clip = m.creature.clip(m.act).name == clip.name;
    let current = world.get(m.id).and_then(|e| e.anim.clone());
    let anim = match current {
        Some(cur) if same_clip && cur.clip == clip.name => AnimSpec {
            speed: rate,
            looping: Some(clip.looping),
            time_offset: cur.clip_time(now) as f32,
            start: Some(now),
            ..AnimSpec::new(clip.name)
        },
        _ => AnimSpec {
            speed: rate,
            looping: Some(clip.looping),
            time_offset: if clip.looping { m.phase } else { 0.0 },
            start: Some(now),
            crossfade: CROSSFADE_S,
            ..AnimSpec::new(clip.name)
        },
    };
    m.act = act;
    m.rate = rate;
    ents::update(world, agent, m.id, UpdateSpec { anim: Some(Some(anim)), ..UpdateSpec::default() });
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
