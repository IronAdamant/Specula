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
//! on the GPU; each plays its anim set's `creature` graph (the world's `anim` component in graph
//! mode): the scene only sets parameters (what it does: `act` = idle, move, attack, special,
//! death; its ground `speed` in m/s, slow included, which the graph's locomotion blend tree and
//! `match_speed` turn into planted feet; a `hit` trigger for the flinch layer; the look target),
//! and the graph, as data, picks the clips, blends them and plants the feet on the slab (whose
//! top pieces are tagged `ground`). The game's pace is the anim's speed.
//!
//! Cost (foundation F3): structure changes touch only what changed; moving things are written
//! once per frame, interpolated between the last two ticks with the frame's `alpha`, and only
//! when they moved. Animation is a pure function of world time: a clip is written only when what
//! the creature does (or how fast) changes.

use crate::ents;
use crate::looks::{self, highlight, palette::*, Act, Creature, Part};
use crate::slab::{self, SlabGeo};
use sepulcra::content::Attack;
use sepulcra::map::{CellKind, Face};
use sepulcra::sim::{EnemyId, ProjectileId};
use sepulcra::{CellId, Enemy, Event, Motion, Projectile, State};
use specula::game::GameWorld;
use specula::world::{AnimSpec, EntityKey, Layer, LookPatch, SpawnSpec, UpdateSpec, World};
use specula_anim::graph::Value;
use specula_anim::play::GraphPlay;
use specula_core::math::Vec3;
use specula_core::Json;
use specula_gpu::Material;
use std::collections::BTreeMap;

#[path = "scene_adopt.rs"]
mod adopt;
#[path = "scene_vfx.rs"]
mod effects;

/// Most point lights the game uses (the engine renders 16; leave room for agents' lights).
const MAX_GAME_LIGHTS: usize = 12;
/// Flashes alive at once (the oldest is reused past this).
const FX_MAX: usize = 48;
const RING_DOTS: usize = 28;
/// The anim graph every creature plays.
const GRAPH: &str = "creature";
/// Ground speed (m/s) changes smaller than this aren't written (each write settles the graph).
const SPEED_STEP: f32 = 0.05;
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
    /// What its graph was last told: `act` and ground speed (m/s).
    act: Act,
    speed: f32,
    /// A one-shot clip (a summon) plays until this presentation time.
    busy_until: f32,
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

/// The rubble bar's length in the world (metres) and the height above the cell it stands at:
/// over the rubble, under a stone's muzzle. Close up it is drawn this long.
pub const BAR_W: f32 = 1.2;
pub const BAR_Y: f32 = 1.6;
/// Where on its rubble a bar's stem ends and its anchor mark sits: this far over the cell's
/// anchor, on the rubble's stones (a wall segment stands about a metre high).
pub const RUBBLE_MARK_Y: f32 = 0.6;
/// **Its least size on screen**, as parts of the window's shorter side (so it grows with the
/// window): 7.8% long and 1.4% high (56 by 10 pixels at 720 high, 84 by 15 at 1080), and never
/// under 24 by 4 pixels. At 4.5% by 0.75% (35 by 8 at 720) the independent picture check of
/// 2026-11-28 still found the bar too small to read from the game's usual camera.
pub const BAR_MIN_LEN: f32 = 0.078;
pub const BAR_MIN_HIGH: f32 = 0.014;
pub const BAR_MIN_PX: (f32, f32) = (24.0, 4.0);

/// What a rubble's bar shows (0..1): the lesser of the health and the life it has left, as the
/// rules count them ([`sepulcra::RubbleStand::left_permille`], from its `hp` of `max_hp` and
/// its `life_left` of `life`).
pub fn bar_fraction(r: &sepulcra::RubbleStand, tick: u32) -> f32 {
    r.left_permille(tick) as f32 / 1000.0
}

/// The bar turns amber below this share (a half) and red below [`BAR_RED_BELOW`] (a quarter),
/// so full, half and low differ by colour at the frame's own size, where 15 to 20 pixels of fill
/// alone did not (the independent picture check of 2026-11-28).
pub const BAR_AMBER_BELOW: f32 = 0.5;
pub const BAR_RED_BELOW: f32 = 0.25;

/// The bar's amber: a clear orange (`#ff9f1a`). The palette's warm colours are the fire's:
/// `FIRE_CORE` is a pale cream that the picture check found close to the bar's pale rim, and
/// `FIRE_FLAME` an orange too near the bar's red to tell apart at a glance.
pub const BAR_AMBER: [f32; 3] = [1.0, 0.624, 0.102];
/// The bar's red: a bright saturated red (`#ff2a2a`). The palette's `INVALID` is a muted brick
/// (kept clear of the cursor's colours) that the picture check found close to the dark ground.
pub const BAR_RED: [f32; 3] = [1.0, 0.165, 0.165];
/// The bar's back, under its fill: the slab's darkest stone.
pub const BAR_BACK: [f32; 3] = FISSURE;
/// The bar's pale rim, its stem and its anchor mark.
pub const BAR_RIM: [f32; 3] = [0.82, 0.84, 0.86];

/// A rubble bar's fill colour at `fraction` full: the game's own green ([`GRAVE_GLOW`]) from full
/// down to a half, [`BAR_AMBER`] under [`BAR_AMBER_BELOW`], and [`BAR_RED`] under
/// [`BAR_RED_BELOW`].
pub fn bar_colour(fraction: f32) -> [f32; 3] {
    if fraction < BAR_RED_BELOW {
        BAR_RED
    } else if fraction < BAR_AMBER_BELOW {
        BAR_AMBER
    } else {
        GRAVE_GLOW
    }
}

/// Where a rubble's bar is drawn on screen (pixels: x, y, width, height), in the interface
/// layer: its dark back, and its fill, which empties from the right end towards the left; a
/// stem from the bar's bottom down to its own rubble, and a small anchor mark on the rubble
/// where the stem ends ([`RUBBLE_MARK_Y`] over the cell), so it is always clear which rubble a
/// bar belongs to, lifted or not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarRects {
    pub back: [f32; 4],
    pub fill: [f32; 4],
    pub stem: [f32; 4],
    pub mark: [f32; 4],
}

impl BarRects {
    /// The gap kept between two bars' backs: a third of a back's height, at least 2 pixels.
    fn gap(&self) -> f32 {
        (self.back[3] / 3.0).max(2.0)
    }

    /// Do the two backs overlap, or come within a gap of each other (side by side with no gap,
    /// two bars read as one)?
    pub fn crowds(&self, o: &BarRects) -> bool {
        let g = self.gap().max(o.gap());
        let (a, b) = (self.back, o.back);
        a[0] < b[0] + b[2] + g && b[0] < a[0] + a[2] + g && a[1] < b[1] + b[3] + g && b[1] < a[1] + a[3] + g
    }

    /// Where the stem ends: the anchor mark's centre, on the rubble.
    pub fn foot(&self) -> (f32, f32) {
        (self.mark[0] + self.mark[2] / 2.0, self.mark[1] + self.mark[3] / 2.0)
    }

    fn lift(&mut self, by: f32) {
        self.back[1] -= by;
        self.fill[1] -= by;
        self.set_stem();
    }

    /// The stem runs from the back's bottom edge straight down to the foot.
    fn set_stem(&mut self) {
        let w = (self.back[3] / 5.0).max(2.0);
        let (x, y) = self.foot();
        let top = self.back[1] + self.back[3];
        self.stem = [x - w / 2.0, top, w, (y - top).max(0.0)];
    }
}

/// **Bars of neighbouring rubble never cover one another.** From the game's usual camera
/// neighbouring cells stand 10 to 20 pixels apart on screen and a bar is longer than that, so
/// they stacked and one hid the other's level. Each bar keeps its place across (centred over its
/// own rubble) and is lifted straight up, a back's height and a gap at a time, until it crowds
/// no bar placed before it; the nearest rubble's bar (lowest on screen) is placed first, so a
/// farther rubble's bar stays above a nearer one's, as the rubble do. Its stem still reaches its
/// own rubble's mark. This was chosen over one bar for a cluster, which would show one level for
/// several rubble and hide the rest.
pub fn lay_out_bars(bars: &mut [BarRects]) {
    let mut order: Vec<usize> = (0..bars.len()).collect();
    let bottom = |b: &BarRects| b.back[1] + b.back[3];
    order.sort_by(|&i, &j| bottom(&bars[j]).total_cmp(&bottom(&bars[i])).then(bars[i].back[0].total_cmp(&bars[j].back[0])).then(i.cmp(&j)));
    for (k, &i) in order.iter().enumerate() {
        let step = bars[i].back[3] + bars[i].gap();
        // Each bar placed before blocks at most two steps, so this many always finds room.
        for _ in 0..2 * k + 2 {
            let b = bars[i];
            if !order[..k].iter().any(|&j| b.crowds(&bars[j])) {
                break;
            }
            bars[i].lift(step);
        }
    }
}

/// The bar of a rubble whose bar point (the cell's anchor, [`BAR_Y`] up) is `at`, seen through
/// `view_proj` in an image of `size`, `fraction` full. As long as [`BAR_W`] projects to, and
/// never smaller than the least size; its bottom edge stands a bar's height above the point, so
/// it covers neither the rubble nor the dead clawing at it; its stem and mark reach down to the
/// rubble ([`RUBBLE_MARK_Y`] over the anchor). `None` behind the camera or off the image.
pub fn bar_rects(view_proj: &specula_core::math::Mat4, size: (u32, u32), at: Vec3, fraction: f32) -> Option<BarRects> {
    use specula_core::math::project_to_screen;
    let (w, h) = (size.0.max(1), size.1.max(1));
    let (x, y, _) = project_to_screen(view_proj, at, w, h)?;
    let half = Vec3::new(BAR_W / 2.0, 0.0, 0.0);
    let world = match (project_to_screen(view_proj, at - half, w, h), project_to_screen(view_proj, at + half, w, h)) {
        (Some(a), Some(b)) => ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt(),
        _ => 0.0,
    };
    let short = w.min(h) as f32;
    let len = world.max(short * BAR_MIN_LEN).max(BAR_MIN_PX.0);
    let high = (len / 12.0).max(short * BAR_MIN_HIGH).max(BAR_MIN_PX.1);
    let edge = (high / 4.0).max(1.0);
    let (left, top) = (x - len / 2.0, y - 2.0 * high);
    let back = [left - edge, top - edge, len + 2.0 * edge, high + 2.0 * edge];
    if back[0] > w as f32 || back[1] > h as f32 || back[0] + back[2] < 0.0 || back[1] + back[3] < 0.0 {
        return None;
    }
    // The foot is straight below the bar point, on the rubble (under the point if it does not
    // project).
    let foot = project_to_screen(view_proj, at - Vec3::new(0.0, BAR_Y - RUBBLE_MARK_Y, 0.0), w, h).map_or((x, y), |p| (p.0, p.1));
    let m = (high * 0.7).max(4.0);
    let mut b = BarRects { back, fill: [left, top, len * fraction.clamp(0.0, 1.0), high], stem: [0.0; 4], mark: [foot.0 - m / 2.0, foot.1 - m / 2.0, m, m] };
    b.set_stem();
    Some(b)
}

/// The quads that draw `bars`, in the order they are drawn. They go in the game's interface
/// layer (`Game::overlay`), which the engine draws after the whole 3D scene, window and captures
/// alike, so no enemy, stone or effect is ever drawn over a bar. Within them: every stem and
/// anchor mark first, then each bar's opaque back (so nothing under it shows through) with its
/// pale rim, then its fill, so a bar covers any other bar's stem and never the other way round.
pub fn bar_quads(bars: &[(BarRects, f32)]) -> Vec<specula_gpu::OverlayQuad> {
    use specula_gpu::OverlayQuad;
    let rim = [BAR_RIM[0], BAR_RIM[1], BAR_RIM[2], 1.0];
    let mut out = Vec::with_capacity(bars.len() * 4);
    for (b, _) in bars {
        if b.stem[3] > 0.0 {
            out.push(OverlayQuad::solid(b.stem, rim));
        }
        out.push(OverlayQuad { border: 1.0, border_color: [[BAR_BACK[0], BAR_BACK[1], BAR_BACK[2], 1.0]; 2], ..OverlayQuad::solid(b.mark, rim) });
    }
    for (b, f) in bars {
        let back = OverlayQuad::solid(b.back, [BAR_BACK[0], BAR_BACK[1], BAR_BACK[2], 1.0]);
        out.push(OverlayQuad { border: (b.fill[1] - b.back[1]).max(1.0), border_color: [rim; 2], ..back });
        let [r, g, bl] = bar_colour(*f);
        out.push(OverlayQuad::solid(b.fill, [r, g, bl, 1.0]));
    }
    out
}

/// The bars of rubble on `cells` (each with the share it shows), [`BAR_Y`] over each cell's
/// anchor, laid out clear of one another ([`lay_out_bars`]); those off the image are left out.
pub fn rubble_bars(geo: &SlabGeo, map: &sepulcra::map::Map, cells: impl Iterator<Item = (CellId, f32)>, view_proj: &specula_core::math::Mat4, size: (u32, u32)) -> Vec<(BarRects, f32)> {
    let mut out: Vec<(BarRects, f32)> = cells.filter_map(|(cell, f)| bar_rects(view_proj, size, geo.anchor(map, cell).0 + Vec3::new(0.0, BAR_Y, 0.0), f).map(|b| (b, f))).collect();
    let mut rects: Vec<BarRects> = out.iter().map(|b| b.0).collect();
    lay_out_bars(&mut rects);
    for (o, r) in out.iter_mut().zip(rects) {
        o.0 = r;
    }
    out
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
    /// Particles and decals (transient presentation; see `scene_vfx`).
    vfx: effects::Effects,
    /// The map's props (`crate::props`): dressing hidden under stones.
    props: crate::props::Props,
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
    pub fn build(world: &mut GameWorld<'_>, agent: &str, state: &State) -> Scene {
        let map = &state.content().map;
        let geo = SlabGeo::new(map);
        slab::build(world, agent, map, &state.content().fuel, &geo);
        let props = crate::props::Props::build(world, agent, map, &geo);
        // `fx` and `ui` are where earlier versions kept their pools.
        for name in ["units", "live", "cursor", "fx", "ui"] {
            if let Some(old) = ents::top_level(world, name) {
                ents::delete(world, old);
            }
        }
        let units = ents::group(world, agent, None, "units", Vec3::ZERO, Vec3::ZERO, &["game"], Json::Null);
        let sub = |world: &mut GameWorld<'_>, parent: u64, name: &str| ents::group(world, agent, Some(parent), name, Vec3::ZERO, Vec3::ZERO, &[], Json::Null);
        let (stones_g, walls_g, roots_g, tombs_g) = (sub(world, units, "stones"), sub(world, units, "walls"), sub(world, units, "roots"), sub(world, units, "tombstones"));
        let live = ents::transient_group(world, agent, "live", Layer::World, &["game", "transient"]);
        let (enemies_g, bolts_g, fx_g) = (sub(world, live, "enemies"), sub(world, live, "bolts"), sub(world, live, "fx"));
        let vfx = effects::Effects::new(world, agent, live);
        let ui = ents::transient_group(world, agent, "cursor", Layer::Ui, &["ui"]);
        let cursor_part = Part::cube("cell", Vec3::ZERO, Vec3::ONE, highlight::VALID.color).glow(highlight::VALID.emissive);
        let cursor = ents::part(world, agent, ui, &cursor_part);
        ents::show(world, agent, cursor, false);
        let ring_g = sub(world, ui, "range");
        let ring = (0..RING_DOTS)
            .map(|i| ents::part_named(world, agent, ring_g, &format!("dot_{i:02}"), &Part::cube("", Vec3::ZERO, Vec3::new(0.16, 0.16, 0.5), highlight::RANGE_DOT.color).glow(highlight::RANGE_DOT.emissive)))
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
            vfx,
            props,
        };
        scene.build_tombstones(world, state);
        scene
    }

    fn build_tombstones(&mut self, world: &mut GameWorld<'_>, state: &State) {
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
            crate::epitaphs::carve(world, &self.agent, id, i);
            self.tombs.push((id, pos, rot, false));
        }
        // The map's name, cut into a stone behind the graveyard.
        let back = cz - (rows as f32 * 2.2) / 2.0 - 1.4;
        crate::epitaphs::title_stone(world, &self.agent, self.tombs_g, Vec3::new(cx, slab::GRAVEYARD_HEIGHT, back), 0.0, "I · The Graveyard");
    }

    // ---- per tick ----

    /// Brings the world in line with `state` (structure and the motion targets), and turns this
    /// tick's `events` into flourishes. `now` is presentation time in seconds.
    pub fn sync(&mut self, world: &mut GameWorld<'_>, state: &State, events: &[Event], now: f32) {
        self.sync_structure(world, state, events, now);
        self.sync_enemies(world, state, events, now);
        self.sync_bolts(world, state);
    }

    /// Everything but motion: stones, walls, roots, tombstones and flashes. Cheap enough to run
    /// right after a command, so a build shows at once even while the clock is paused.
    pub fn sync_structure(&mut self, world: &mut GameWorld<'_>, state: &State, events: &[Event], now: f32) {
        // Flourishes first: they read the enemies and projectiles that are about to go.
        self.flourishes(world, state, events, now);
        self.sync_stones(world, state);
        self.props.sync(world, &self.agent, state);
        self.sync_walls(world, state);
        self.sync_roots(world, state);
        self.sync_tombstones(world, state);
    }

    /// The map's props in the world (which are hidden under stones).
    pub fn props(&self) -> &crate::props::Props {
        &self.props
    }

    fn sync_stones(&mut self, world: &mut GameWorld<'_>, state: &State) {
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
        self.vfx.sync_stones(world, &self.agent, state, &self.geo, &self.stones);
    }

    fn sync_walls(&mut self, world: &mut GameWorld<'_>, state: &State) {
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

    /// A health bar over every standing rubble (DESIGN.md, "Saxum": the owner's "health bar to
    /// that stone"), drawn in the interface layer at the rubble's projected place so that it
    /// can be read from the game's usual camera ([`bar_rects`]): it shows from the tick the
    /// rubble rises, full, and empties as the dead break it or as its life runs out, whichever
    /// leaves less ([`bar_fraction`]); it goes with the rubble. Each is the bar's rects and the
    /// fraction it shows, in cell order; neighbours' bars are laid out clear of one another
    /// ([`lay_out_bars`]) and each is coloured by its level ([`bar_colour`]).
    pub fn bars(&self, state: &State, view_proj: &specula_core::math::Mat4, size: (u32, u32)) -> Vec<(BarRects, f32)> {
        let tick = state.tick();
        rubble_bars(&self.geo, &state.content().map, state.rubble().map(|r| (r.cell, bar_fraction(&r, tick))), view_proj, size)
    }

    fn sync_roots(&mut self, world: &mut GameWorld<'_>, state: &State) {
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
            let size = Vec3::new(1.7, 0.06, 1.7);
            let p = Part::cube("", anchor + Vec3::new(0.0, 0.03, 0.0), size, MOSS).rot(0.0, k * 9.0, 0.0).rough(1.0);
            let g = ents::part_named(world, &self.agent, self.roots_g, &map.cell_name(c.id), &p);
            // Real grass on the moss (its child, undoing the patch's flat scale).
            crate::grass::on_root(world, &self.agent, g, size, c.id);
            // Thin roots creeping over the moss (children of the flat patch: its scale flattens them).
            for (name, a, off) in [("root_a", 25.0, -0.18), ("root_b", 80.0, 0.12), ("root_c", 140.0, 0.25)] {
                let root = Part::cube(name, Vec3::new(off, 0.9, 0.05), Vec3::new(0.06, 1.0, 0.85), ROOTS).rot(0.0, a + k * 23.0, 0.0).rough(1.0);
                ents::part(world, &self.agent, g, &root);
            }
            self.roots.insert(c.id, g);
        }
    }

    fn sync_tombstones(&mut self, world: &mut GameWorld<'_>, state: &State) {
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

    /// What an enemy's clip should show this tick: its special is a runner's leap, a wraith's
    /// dive and a climber hauling itself up a cliff (the colossus's summon comes from events).
    fn act_of(e: &Enemy, map: &sepulcra::map::Map) -> Act {
        match e.motion {
            Motion::Ground { leap: true, .. } => Act::Special,
            Motion::Air { diving: true, .. } => Act::Special,
            _ if e.attacking.is_some() => Act::Attack,
            // The same test the simulation's climb speed uses.
            Motion::Ground { from, to, len, .. } if len > 0 && (map.cell(from).kind.is_climb() || map.cell(to).kind.is_climb()) => Act::Special,
            Motion::Ground { len: 0, .. } => Act::Idle,
            _ => Act::Move,
        }
    }

    fn sync_enemies(&mut self, world: &mut GameWorld<'_>, state: &State, events: &[Event], now: f32) {
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
                        m.busy_until = now + m.creature.special_s / self.pace.max(0.1);
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
                let act = Scene::act_of(e, &content.map);
                let speed = Scene::ground_speed(state, e, pos, tick);
                let yaw = facing(Vec3::ZERO - pos, 0.0);
                let props = Json::obj([("enemy", Json::from(e.id)), ("kind", Json::from(def.id.as_str()))]);
                let model = specula::world::ModelSpec::new(creature.recipe);
                let rot = Vec3::new(0.0, yaw, 0.0);
                let mut spec = ents::model_spec(self.enemies_g, &format!("{}_{}", def.id, e.id), model, pos, rot, creature.scale, &["enemy", &def.id], props);
                // Anchored `phase` seconds back, so a pack doesn't walk in step.
                let mut play = graph_params(act, speed, false);
                play.set_param("look_at", Value::Vec3(looks::LOOK_TARGET));
                play.set_param("look", Value::Num(creature.look));
                spec.anim = Some(AnimSpec { speed: self.pace, start: Some(world_now - phase as f64), ..AnimSpec::graph(play) });
                let id = ents::spawn(world, &self.agent, spec);
                let flies = def.flies();
                if flies && id != 0 {
                    self.vfx.trail(world, &self.agent, id);
                }
                self.enemies.insert(e.id, Mob { id, creature, prev: pos, cur: pos, prev_rot: rot, rot, written: Some((pos, rot)), flies, act, speed, busy_until: 0.0 });
                continue;
            }
            let busy = self.enemies.get(&e.id).is_some_and(|m| m.busy_until > now);
            let act = if busy { Act::Special } else { Scene::act_of(e, &content.map) };
            let speed = Scene::ground_speed(state, e, pos, tick);
            let pace = self.pace;
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
            if act != m.act || (speed - m.speed).abs() > SPEED_STEP {
                tell(world, &self.agent, m, act, speed, false, pace);
            }
        }
    }

    /// An enemy's ground speed in m/s as the simulation moves it (slowed, climbing): the graph
    /// picks the gait and rate that keep its feet planted at it.
    fn ground_speed(state: &State, e: &Enemy, pos: Vec3, tick: u32) -> f32 {
        let def = state.content().enemy(e.kind);
        let slow = if e.is_slowed(tick) { (1000 - e.slow.clamp(0, 900)) as f32 / 1000.0 } else { 1.0 };
        let climb = if pos.y < -0.5 && !def.flies() { def.climb_speed as f32 / 1000.0 } else { 1.0 };
        def.speed as f32 / 1000.0 * slow * climb
    }

    /// Sets how fast the game runs (0 while paused, else 1, 2 or 4 steps per tick): every
    /// creature's graph follows (its time scale; the play settles, so nothing jumps).
    pub fn set_pace(&mut self, world: &mut GameWorld<'_>, pace: f32) {
        if pace == self.pace {
            return;
        }
        self.pace = pace;
        for m in self.enemies.values_mut() {
            let (act, speed) = (m.act, m.speed);
            tell(world, &self.agent, m, act, speed, false, pace);
        }
        // The dying play on at the new pace too.
        for c in &self.corpses {
            tell_id(world, &self.agent, c.id, graph_params(Act::Death, 0.0, false), pace);
        }
    }

    fn sync_bolts(&mut self, world: &mut GameWorld<'_>, state: &State) {
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

    fn flourishes(&mut self, world: &mut GameWorld<'_>, state: &State, events: &[Event], now: f32) {
        let content = state.content();
        let map = &content.map;
        for ev in events {
            match *ev {
                Event::Died { enemy, pos, kind } => {
                    let [x, y, z] = pos.to_f32();
                    let big = if content.enemy(kind).id == "colossus" { 4.0 } else { 1.0 };
                    self.flash(world, now, Vec3::new(x, y + 0.9 * big, z), 0.2 * big, 1.1 * big, 0.35, GRAVE_GLOW, 1.0, Some(0.25));
                    if !content.enemy(kind).flies() {
                        self.vfx.died(world, &self.agent, now, Vec3::new(x, y, z), big);
                    }
                    // The body plays its death where it fell (or falls as a ragdoll), then goes.
                    if let Some(mut m) = self.enemies.remove(&enemy) {
                        let pace = self.pace.max(1.0);
                        if !m.creature.ragdoll {
                            tell(world, &self.agent, &mut m, Act::Death, 0.0, false, pace);
                        }
                        ents::place(world, &self.agent, m.id, Some(m.cur), Some(m.rot), None);
                        ents::update(world, &self.agent, m.id, UpdateSpec { add_tags: vec!["dead".into()], ..UpdateSpec::default() });
                        if m.creature.ragdoll {
                            ragdoll_death(world, &self.agent, &m, pace);
                        }
                        self.corpses.push(Corpse { id: m.id, until: now + m.creature.death_s / pace + LINGER_S });
                    }
                }
                Event::Hit { enemy, .. } => {
                    if let Some(m) = self.enemies.get_mut(&enemy) {
                        let p = m.cur + Vec3::new(0.0, 1.1, 0.0);
                        // The flinch layer: a trigger the graph consumes.
                        let (act, speed, pace) = (m.act, m.speed, self.pace);
                        tell(world, &self.agent, m, act, speed, true, pace);
                        self.flash(world, now, p, 0.15, 0.6, 0.15, RUNE_PALE, 3.0, None);
                        self.vfx.hit(world, &self.agent, now, p);
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
                    self.vfx.splash(world, &self.agent, now, Vec3::new(x, y, z), splash);
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
                Event::WallPlaced { cell } => {
                    let (a, _) = self.geo.anchor(map, cell);
                    self.vfx.wall_built(world, &self.agent, now, a);
                }
                Event::WallBroken { cell } => {
                    let (a, _) = self.geo.anchor(map, cell);
                    self.vfx.wall_broken(world, &self.agent, now, a);
                    self.flash(world, now, a + Vec3::new(0.0, 0.5, 0.0), 0.6, 2.4, 0.5, BONE_SHADOW, 0.3, Some(0.6));
                }
                Event::TombstonesLost { .. } => {
                    let [x, _, z] = map.graveyard_centre.to_f32();
                    self.flash(world, now, Vec3::new(x, 1.5, z), 1.0, 6.0, 0.7, GRAVE_GLOW, 1.0, Some(0.2));
                }
                // ---- The elements (proving ground). PLACEHOLDER looks, made only of what the
                // working line has (glow spheres, the old built-in effects and decals), so that
                // a person can see that each rule fired; what each will look like is DESIGN.md,
                // "Elemental stones", and waits for the effects library (Effectus). ----
                Event::Struck { enemy, .. } | Event::Shattered { enemy, .. } => {
                    if let Some(e) = state.enemy(enemy) {
                        let [x, y, z] = e.pos.to_f32();
                        let p = Vec3::new(x, y + 1.0, z);
                        self.vfx.hit(world, &self.agent, now, p);
                        self.flash(world, now, p, 0.2, 0.9, 0.2, RUNE_PALE, 2.0, None);
                    }
                }
                Event::Ignited { enemy } => {
                    if let Some(e) = state.enemy(enemy) {
                        let [x, y, z] = e.pos.to_f32();
                        self.flash(world, now, Vec3::new(x, y + 0.9, z), 0.3, 1.0, 0.4, FIRE_FLAME, 1.6, Some(0.4));
                    }
                }
                Event::Frozen { enemy } | Event::Thawed { enemy, .. } | Event::Extinguished { enemy } => {
                    if let Some(e) = state.enemy(enemy) {
                        let [x, y, z] = e.pos.to_f32();
                        self.flash(world, now, Vec3::new(x, y + 0.9, z), 0.6, 1.3, 0.6, RUNE_PALE, 0.6, Some(0.35));
                    }
                }
                Event::Gusted { stone, .. } => {
                    let (a, _) = self.geo.anchor(map, stone);
                    self.disc(world, now, a + Vec3::new(0.0, 0.4, 0.0), 4.0, 0.5);
                }
                Event::Heaved { at, .. } | Event::RubbleRaised { cell: at } | Event::WallCrumbled { cell: at } => {
                    let (a, _) = self.geo.anchor(map, at);
                    self.vfx.wall_broken(world, &self.agent, now, a);
                }
                Event::Detonated { pos, .. } => {
                    let [x, y, z] = pos.to_f32();
                    let p = Vec3::new(x, y + 0.5, z);
                    self.flash(world, now, p, 0.5, 6.0, 0.45, FIRE_FLAME, 2.0, Some(0.35));
                    self.vfx.splash(world, &self.agent, now, Vec3::new(x, y, z), 3.0);
                }
                Event::CellCaught { cell } => {
                    let (a, _) = self.geo.anchor(map, cell);
                    self.vfx.splash(world, &self.agent, now, a, 1.0);
                    self.flash(world, now, a + Vec3::new(0.0, 0.3, 0.0), 0.4, 1.6, 0.6, FIRE_FLAME, 1.4, Some(0.4));
                }
                _ => {}
            }
        }
    }

    /// A growing glow sphere (`opacity`: glass when given).
    #[allow(clippy::too_many_arguments)]
    fn flash(&mut self, world: &mut GameWorld<'_>, now: f32, pos: Vec3, from: f32, to: f32, dur: f32, color: [f32; 3], emissive: f32, opacity: Option<f32>) {
        let (s0, s1) = (Vec3::new(from, from, from), Vec3::new(to, to, to));
        self.effect(world, now, pos, s0, s1, dur, color, emissive, opacity);
    }

    /// A flat ring of light spreading to `radius` (a rune pillar's pulse).
    fn disc(&mut self, world: &mut GameWorld<'_>, now: f32, pos: Vec3, radius: f32, dur: f32) {
        let (s0, s1) = (Vec3::new(0.6, 0.06, 0.6), Vec3::new(radius * 2.0, 0.06, radius * 2.0));
        self.effect(world, now, pos, s0, s1, dur, RUNE_GLOW, 1.0, Some(0.12));
    }

    #[allow(clippy::too_many_arguments)]
    fn effect(&mut self, world: &mut GameWorld<'_>, now: f32, pos: Vec3, from: Vec3, to: Vec3, dur: f32, color: [f32; 3], emissive: f32, opacity: Option<f32>) {
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
    pub fn animate(&mut self, world: &mut GameWorld<'_>, alpha: f32, now: f32) {
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
        self.vfx.expire(world, now);
        self.corpses.retain(|c| {
            if now >= c.until {
                ents::delete(world, c.id);
                return false;
            }
            true
        });
    }

    /// Shows the build cursor on a cell (valid, invalid or selected), or hides it.
    pub fn show_cursor(&mut self, world: &mut GameWorld<'_>, state: &State, look: CursorLook) {
        if look == self.cursor_shown {
            return;
        }
        self.cursor_shown = look;
        let (cell, glow) = match look {
            CursorLook::Hidden => {
                ents::show(world, &self.agent, self.cursor, false);
                return;
            }
            CursorLook::Valid(c) => (c, highlight::VALID),
            CursorLook::Invalid(c) => (c, highlight::INVALID),
            CursorLook::Selected(c) => (c, highlight::SELECTED),
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
        let change = UpdateSpec {
            position: Some(pos),
            scale: Some(scale),
            color: Some(glow.color),
            look: LookPatch { emissive: Some(glow.emissive), ..LookPatch::default() },
            visible: Some(true),
            ..UpdateSpec::default()
        };
        ents::update(world, &self.agent, self.cursor, change);
    }

    /// A ring of dots showing a range around a cell (`None` hides it).
    pub fn show_range(&mut self, world: &mut GameWorld<'_>, state: &State, ring: Option<(CellId, f32)>) {
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
            ("effects", Json::from(self.vfx.count())),
        ])
    }

    /// Where an enemy is drawn now (for the HUD's threat markers).
    pub fn enemy_at(&self, enemy: EnemyId) -> Option<Vec3> {
        self.enemies.get(&enemy).map(|m| m.written.map_or(m.cur, |(p, _)| p))
    }

    /// Whether a cell holds something buildable for the cursor (fragment or ledge that no big
    /// prop covers).
    pub fn buildable(state: &State, cell: CellId) -> bool {
        let map = &state.content().map;
        matches!(map.cell(cell).kind, CellKind::Fragment | CellKind::Ledge) && map.blocking_prop(cell).is_none()
    }
}

/// A colour multiplier that tints white towards `c` (models are white × their vertex colours).
fn hex_tint(c: [f32; 3]) -> [f32; 3] {
    [0.55 + 0.45 * c[0], 0.55 + 0.45 * c[1], 0.55 + 0.45 * c[2]]
}

/// The graph parameters for what a creature does.
fn graph_params(act: Act, speed: f32, hit: bool) -> GraphPlay {
    let mut p = GraphPlay::new(GRAPH);
    p.set_param("act", Value::Name(act.name().into()));
    p.set_param("speed", Value::Num(speed));
    if hit {
        p.set_param("hit", Value::Bool(true));
    }
    p
}

/// Tells a creature's graph what it does now: the world settles the play at the write and the
/// graph blends (parameters not given, like the look target, keep their values).
fn tell(world: &mut GameWorld<'_>, agent: &str, m: &mut Mob, act: Act, speed: f32, hit: bool, pace: f32) {
    m.act = act;
    m.speed = speed;
    tell_id(world, agent, m.id, graph_params(act, speed, hit), pace);
}

/// A ragdoll death (A4): the body goes limp and is knocked back, away from where it faced (the
/// stones are ahead of it), with a little lift; its graph blends into the physics pose. The
/// corpse is transient, so its ragdoll is cosmetic: it lands on the slab's colliders and never
/// touches the simulation. Its play stays what it was (only `ragdoll` changes): the ragdoll
/// moves on at the velocity of what the body was doing, and a death clip blending in at the
/// same tick would read as a fling (the shambler's hands left at 17 m/s).
fn ragdoll_death(world: &mut GameWorld<'_>, agent: &str, m: &Mob, pace: f32) {
    let yaw = m.rot.y.to_radians();
    let back = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    // N·s on the chest: about 1.2 m/s on the whole 69 kg body, enough to topple a hunched one onto
    // its back (26 N·s sat it down on its hump).
    let knock = back * 80.0 + Vec3::new(0.0, 12.0, 0.0);
    let ragdoll = Json::obj([
        ("mode", Json::from("limp")),
        ("hit", Json::obj([("bone", Json::from("chest")), ("impulse", Json::arr([knock.x, knock.y, knock.z].map(Json::from))), ("seq", Json::from(1))])),
    ]);
    ents::update(world, agent, m.id, UpdateSpec { props: Some(Json::obj([("ragdoll", ragdoll)])), ..UpdateSpec::default() });
    let mut play = graph_params(m.act, m.speed, false);
    play.set_param("ragdoll", Value::Bool(true));
    tell_id(world, agent, m.id, play, pace);
}

fn tell_id(world: &mut GameWorld<'_>, agent: &str, id: u64, play: GraphPlay, pace: f32) {
    let anim = AnimSpec { speed: pace.clamp(0.0, 8.0), ..AnimSpec::graph(play) };
    ents::update(world, agent, id, UpdateSpec { anim: Some(Some(anim)), ..UpdateSpec::default() });
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

#[cfg(test)]
mod tests {
    use super::*;
    use sepulcra::content::Content;
    use std::sync::Arc;

    /// A rubble's bar shows what the rules say is left of it: the lesser of its health (its
    /// wall's HP of its full HP) and its life (the ticks to its crumbling of the ticks it was
    /// raised for), from the tick it rises to the tick it goes; and the fill is that share of the
    /// bar, emptying from the right.
    #[test]
    fn a_rubble_bar_shows_the_lesser_of_its_health_and_its_life() {
        let content = Arc::new(Content::proving());
        let mut s = State::new(Arc::clone(&content), 3);
        let saxum = content.stone_kind("saxum").unwrap();
        let cell = content.map.cell_ref("top:10,15").unwrap();
        let mut ev = Vec::new();
        s.apply(&sepulcra::Command::PlaceStone { cell, stone: saxum }, &mut ev).unwrap();
        for _ in 0..2 {
            s.apply(&sepulcra::Command::Upgrade { cell }, &mut ev).unwrap();
        }
        let life = content.stone(saxum).tiers[2].el.rubble_ticks;
        let (mut raised, mut seen, mut damaged) = (BTreeMap::new(), 0u32, 0u32);
        for _ in 0..(120 * sepulcra::TICK_HZ) {
            let tick = s.tick();
            for e in s.step(&[]) {
                if let Event::RubbleRaised { cell } = e {
                    raised.insert(cell, tick);
                }
            }
            for r in s.rubble() {
                let w = s.wall(r.cell).expect("rubble is a wall");
                let health = w.hp as f64 / w.max_hp as f64;
                let rose = raised[&r.cell];
                let left = f64::from(rose + life - s.tick()) / f64::from(life);
                let want = health.min(left);
                let got = f64::from(bar_fraction(&r, s.tick()));
                assert!((got - want).abs() <= 0.0011, "{}: bar {got}, want {want} (health {health}, life {left})", content.map.cell_name(r.cell));
                // The rules' answer says the same life the events do.
                assert_eq!((r.life(), r.life_left(s.tick())), (life, rose + life - s.tick()));
                seen += 1;
                damaged += u32::from(w.hp < w.max_hp);
            }
        }
        assert!(!raised.is_empty() && seen > 0 && damaged > 0, "rubble rose ({raised:?}), was seen ({seen}) and was broken into ({damaged})");
    }

    /// The bar can be read from the game's usual camera: seen from far off it is never smaller
    /// than its least size (4.5% by 0.75% of the window's shorter side, at least 24 by 4
    /// pixels), which grows with the window; close up it is as long as it stands in the world;
    /// its fill is the shown share of it from the left; and it stands above its point, so it
    /// covers neither the rubble nor the dead at it.
    #[test]
    fn a_rubble_bar_keeps_its_least_size_on_screen_and_grows_with_the_window() {
        use specula::sight::Camera;
        let at = Vec3::new(0.0, BAR_Y, 0.0);
        let far = Camera::new(Vec3::new(0.0, 60.0, 45.0), Vec3::ZERO, 45.0);
        let near = Camera::new(Vec3::new(0.0, 3.0, 3.0), at, 45.0);
        for size in [(1280u32, 720u32), (1920, 1080), (2560, 1440), (640, 360)] {
            let short = size.0.min(size.1) as f32;
            let (want_len, want_high) = ((short * BAR_MIN_LEN).max(BAR_MIN_PX.0), (short * BAR_MIN_HIGH).max(BAR_MIN_PX.1));
            let b = bar_rects(&far.view_proj(size.0, size.1), size, at, 1.0).expect("on screen");
            // From far off, the world's 1.2 m would be a few pixels: the least size holds.
            let world = {
                let vp = far.view_proj(size.0, size.1);
                let p = |v: Vec3| specula_core::math::project_to_screen(&vp, v, size.0, size.1).unwrap();
                let (a, c) = (p(at - Vec3::new(BAR_W / 2.0, 0.0, 0.0)), p(at + Vec3::new(BAR_W / 2.0, 0.0, 0.0)));
                c.0 - a.0
            };
            assert!(world < want_len, "{size:?}: the far camera shows the world bar {world} px, under the least {want_len}");
            assert!((b.fill[2] - want_len).abs() < 1e-3 && (b.fill[3] - want_high).abs() < 1e-3, "{size:?}: {b:?}, want {want_len} by {want_high}");
            let (x, y, _) = specula_core::math::project_to_screen(&far.view_proj(size.0, size.1), at, size.0, size.1).unwrap();
            assert!((b.fill[0] + b.fill[2] / 2.0 - x).abs() < 1e-3, "centred over its point");
            assert!(b.back[1] + b.back[3] < y, "{size:?}: the bar stands above its point: {b:?} over y {y}");
            let half = bar_rects(&far.view_proj(size.0, size.1), size, at, 0.5).unwrap();
            assert_eq!((half.fill[0], half.back), (b.fill[0], b.back));
            assert!((half.fill[2] - want_len / 2.0).abs() < 1e-3, "half full is half the length, from the left");
            assert_eq!(bar_rects(&far.view_proj(size.0, size.1), size, at, 0.0).unwrap().fill[2], 0.0);
            // Close up it is the world's length, longer than the least.
            let c = bar_rects(&near.view_proj(size.0, size.1), size, at, 1.0).unwrap();
            assert!(c.fill[2] > want_len * 2.0, "{size:?}: close up {c:?}");
        }
        // From far off at 1280 by 720 the least is about 56 by 10 pixels (the picture check's ask).
        let least = bar_rects(&far.view_proj(1280, 720), (1280, 720), at, 1.0).unwrap();
        assert!((least.fill[2] - 56.0).abs() < 1.0 && (least.fill[3] - 10.0).abs() < 0.5, "{least:?}");
        // It grows with the window: twice the window, twice the least size.
        let small = bar_rects(&far.view_proj(1280, 720), (1280, 720), at, 1.0).unwrap();
        let big = bar_rects(&far.view_proj(2560, 1440), (2560, 1440), at, 1.0).unwrap();
        assert!((big.fill[2] - 2.0 * small.fill[2]).abs() < 1e-3 && (big.fill[3] - 2.0 * small.fill[3]).abs() < 1e-3);
        // Behind the camera: nothing.
        assert!(bar_rects(&far.view_proj(1280, 720), (1280, 720), Vec3::new(0.0, 120.0, 90.0), 1.0).is_none());
    }

    /// The bar's colour says its level: the game's green full and at 60%, amber at 40% (under a
    /// half), red at 20% (under a quarter); the two thresholds themselves are the colour above.
    #[test]
    fn a_rubble_bar_is_green_then_amber_under_a_half_then_red_under_a_quarter() {
        assert_eq!((BAR_AMBER_BELOW, BAR_RED_BELOW), (0.5, 0.25));
        assert_eq!(bar_colour(1.0), GRAVE_GLOW);
        assert_eq!(bar_colour(0.6), GRAVE_GLOW);
        assert_eq!(bar_colour(0.4), BAR_AMBER);
        assert_eq!(bar_colour(0.2), BAR_RED);
        assert_eq!((bar_colour(0.5), bar_colour(0.25), bar_colour(0.0)), (GRAVE_GLOW, BAR_AMBER, BAR_RED));
    }

    /// The three fill colours read on the dark slab and apart from one another, by stated
    /// measures: a WCAG luminance contrast ratio of at least 3 (the bar for interface parts)
    /// against the bar's own back, which each fill sits on; an sRGB distance of at least 0.5 from
    /// the slab's top colour and from the bar's pale rim; and at least 0.4 between any two of
    /// them. The palette's INVALID and FIRE_CORE, the colours of the round before, fail it.
    #[test]
    fn the_bars_colours_stand_out_from_the_slab_and_from_one_another() {
        fn lum(c: [f32; 3]) -> f32 {
            let l = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
            0.2126 * l(c[0]) + 0.7152 * l(c[1]) + 0.0722 * l(c[2])
        }
        let ratio = |a: [f32; 3], b: [f32; 3]| {
            let (x, y) = (lum(a), lum(b));
            (x.max(y) + 0.05) / (x.min(y) + 0.05)
        };
        let dist = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        let ok = |c: [f32; 3]| ratio(c, BAR_BACK) >= 3.0 && dist(c, SLAB_TOP) >= 0.5 && dist(c, BAR_RIM) >= 0.5;
        for c in [GRAVE_GLOW, BAR_AMBER, BAR_RED] {
            assert!(ok(c), "{c:?}: contrast {} with the back, {} from the slab, {} from the rim", ratio(c, BAR_BACK), dist(c, SLAB_TOP), dist(c, BAR_RIM));
        }
        assert!(dist(GRAVE_GLOW, BAR_AMBER) >= 0.4 && dist(BAR_AMBER, BAR_RED) >= 0.4 && dist(BAR_RED, GRAVE_GLOW) >= 0.4);
        assert!(!ok(INVALID) && !ok(FIRE_CORE), "the measure tells the dull brick and the cream apart");
    }

    /// The bar is drawn over everything: its quads go in the interface layer, which the engine
    /// draws after the 3D scene (the enemies among it); within them every stem and mark comes
    /// before any back, each back is opaque, and each fill follows its own back.
    #[test]
    fn a_rubble_bar_is_drawn_over_the_dead_and_over_other_bars_stems() {
        use crate::controls::{Orbit, MAX_DIST};
        let content = Content::builtin();
        let map = &content.map;
        let geo = SlabGeo::new(map);
        let cam = Orbit { dist: MAX_DIST, ..Orbit::default() }.camera();
        let cells: Vec<CellId> = ["top:9,15", "top:9,16", "top:9,14", "top:7,17"].iter().map(|n| map.cell_ref(n).unwrap()).collect();
        let bars = rubble_bars(&geo, map, cells.iter().map(|&c| (c, 0.4)), &cam.view_proj(1280, 720), (1280, 720));
        assert_eq!(bars.len(), 4);
        let q = bar_quads(&bars);
        let back = [BAR_BACK[0], BAR_BACK[1], BAR_BACK[2], 1.0];
        let first_back = q.iter().position(|x| x.color == back).unwrap();
        assert!(q[..first_back].iter().all(|x| x.color[..3] == BAR_RIM), "stems and marks first");
        assert_eq!(q[..first_back].len(), 2 * bars.len(), "a stem and a mark for every bar");
        for (k, (b, _)) in bars.iter().enumerate() {
            let (bk, fill) = (&q[first_back + 2 * k], &q[first_back + 2 * k + 1]);
            assert_eq!((bk.rect, bk.color), (b.back, back), "an opaque back");
            assert_eq!((fill.rect, fill.color), (b.fill, [BAR_AMBER[0], BAR_AMBER[1], BAR_AMBER[2], 1.0]));
        }
    }

    /// From the game's usual camera (the orbit it starts with), and that camera zoomed out to
    /// 90 m and to its farthest (where neighbours' bars would meet), at 720 and 1080 high, no
    /// two rubble bars cover one another: for rubble on every pair of neighbouring cells of the
    /// slab (side by side and diagonal), and for every rubble of a five by five block of cells
    /// together. Each bar keeps its place across, over its own rubble, and is only lifted; every
    /// bar has a stem from its bottom edge straight down to a mark on its own rubble.
    #[test]
    fn bars_of_rubble_on_neighbouring_cells_never_cover_one_another() {
        use crate::controls::{Orbit, MAX_DIST};
        let content = Content::builtin();
        let map = &content.map;
        let geo = SlabGeo::new(map);
        let cell = |c: i32, r: i32| if c < 0 || r < 0 { None } else { map.cell_ref(&format!("top:{c},{r}")).filter(|&id| map.cell(id).kind.wallable()) };
        let mut pairs = 0;
        let mut lifted_any = false;
        for dist in [Orbit::default().dist, 90.0, MAX_DIST] {
            let cam = Orbit { dist, ..Orbit::default() }.camera();
            let check = |cells: &[CellId], size: (u32, u32)| {
                let vp = cam.view_proj(size.0, size.1);
                let natural: Vec<BarRects> = cells.iter().filter_map(|&c| bar_rects(&vp, size, geo.anchor(map, c).0 + Vec3::new(0.0, BAR_Y, 0.0), 0.5)).collect();
                let bars = rubble_bars(&geo, map, cells.iter().map(|&c| (c, 0.5)), &vp, size);
                assert_eq!(bars.len(), natural.len());
                for (i, (a, _)) in bars.iter().enumerate() {
                    let n = natural[i];
                    assert_eq!((a.back[0], a.back[2], a.back[3]), (n.back[0], n.back[2], n.back[3]), "kept its place across");
                    assert!(a.back[1] <= n.back[1], "only ever lifted");
                    // Every bar, lifted or not, has a stem from its bottom edge to its own
                    // rubble's mark, where the cell's anchor RUBBLE_MARK_Y up projects.
                    let foot = specula_core::math::project_to_screen(&vp, geo.anchor(map, cells[i]).0 + Vec3::new(0.0, RUBBLE_MARK_Y, 0.0), size.0, size.1).unwrap();
                    let (fx, fy) = a.foot();
                    assert!((fx - foot.0).abs() < 1e-3 && (fy - foot.1).abs() < 1e-3, "the mark sits on its own rubble: {a:?}");
                    assert!(a.stem[3] > 0.0 && (a.stem[1] - (a.back[1] + a.back[3])).abs() < 1e-3 && (a.stem[1] + a.stem[3] - fy).abs() < 1e-3, "the stem runs from the bar to its rubble: {a:?}");
                    assert!((a.stem[0] + a.stem[2] / 2.0 - fx).abs() < 1e-3, "straight down onto it");
                    for (b, _) in &bars[i + 1..] {
                        assert!(!a.crowds(b), "{size:?} {cells:?}: {a:?} and {b:?}");
                        let (p, q) = (a.back, b.back);
                        assert!(p[0] + p[2] <= q[0] || q[0] + q[2] <= p[0] || p[1] + p[3] <= q[1] || q[1] + q[3] <= p[1], "the rects do not intersect");
                    }
                }
            };
            for size in [(1280u32, 720u32), (1920, 1080)] {
                for c in 0..40 {
                    for r in 0..40 {
                        let Some(a) = cell(c, r) else { continue };
                        for (dc, dr) in [(1, 0), (0, 1), (1, 1), (1, -1)] {
                            if let Some(b) = cell(c + dc, r + dr) {
                                check(&[a, b], size);
                                pairs += 1;
                            }
                        }
                    }
                }
                let block: Vec<CellId> = (7..12).flat_map(|c| (13..18).filter_map(move |r| cell(c, r))).collect();
                assert!(block.len() >= 6, "{block:?}");
                check(&block, size);
                let vp = cam.view_proj(size.0, size.1);
                let natural: Vec<f32> = block.iter().filter_map(|&c| bar_rects(&vp, size, geo.anchor(map, c).0 + Vec3::new(0.0, BAR_Y, 0.0), 0.5)).map(|b| b.back[1]).collect();
                lifted_any |= rubble_bars(&geo, map, block.iter().map(|&c| (c, 0.5)), &vp, size).iter().zip(natural).any(|(b, y)| b.0.back[1] < y);
            }
        }
        assert!(pairs > 100, "{pairs} pairs");
        assert!(lifted_any, "zoomed out, neighbours' bars would have met: some are lifted");
    }

    /// The cursor and the range dots wear the `looks::highlight` glows (colour and emissive),
    /// whatever they showed before: the colour check in `looks::tests` only holds if these are
    /// what the world gets.
    #[test]
    fn cursor_and_range_dots_wear_the_highlight_glows() {
        let state = State::new(Arc::new(Content::builtin()), 7);
        let mut world = World::default();
        let mut scene = Scene::build(&mut GameWorld::new(&mut world), "test", &state);
        let cell = state.content().map.cell_ref("top:10,12").unwrap();
        let glow_of = |world: &World, id: u64| {
            let e = world.get(id).expect("entity");
            (e.color, e.look.surface.emissive, e.shown())
        };
        for (look, want) in [
            (CursorLook::Valid(cell), highlight::VALID),
            (CursorLook::Selected(cell), highlight::SELECTED),
            (CursorLook::Invalid(cell), highlight::INVALID),
            (CursorLook::Selected(cell), highlight::SELECTED),
            (CursorLook::Valid(cell), highlight::VALID),
        ] {
            scene.show_cursor(&mut GameWorld::new(&mut world), &state, look);
            assert_eq!(glow_of(&world, scene.cursor), (want.color, want.emissive, true), "{look:?}");
        }
        scene.show_range(&mut GameWorld::new(&mut world), &state, Some((cell, 6.5)));
        for &d in &scene.ring {
            assert_eq!(glow_of(&world, d), (highlight::RANGE_DOT.color, highlight::RANGE_DOT.emissive, true));
        }
    }
}
