//! Sepulcra's native hooks for its Lex rule set (`lex/sepulcra.lex.json`, docs/design/LEX.md).
//!
//! The rules (stones, tiers, enemies, waves, economy, walls, selling, calling waves, the
//! range-across-faces test, tombstones, win and lose) are Lex data. What stays native is
//! movement on the slab's cell graph, which Lex has no building block for yet:
//!
//! - `sepulcra.ground` (move phase): Dijkstra flow fields toward the graveyard that change with
//!   walls, breaking a wall when no route is open, climbing speed, roots, lane jumps (a seeded
//!   roll on the `lane_jump` stream), and the stranded safety net. It runs the behaviour's
//!   `on.arrive` (at the graveyard) and `on.stranded` action lists, so tombstones stay rules.
//! - `sepulcra.air` (move phase): flyers circle the ring a drawn number of hops, then dive;
//!   `on.arrive` at the end of the dive.
//! - `sepulcra.spot` (action, a stone's on_spawn): the stone's cell, face and distances to the
//!   rims, read from the slab, for the range-across-faces rule written in Lex.
//! - `sepulcra.wall_clear` (action, the wall command): refuses `enemy_in_the_way` while a ground
//!   enemy stands on or steps into the cell.
//!
//! Each hook reads the slab from the rule set's `slab` data item (the same JSON as
//! `data/slab_01.json`), so the level is data too. They are pure functions of the run's state
//! and their args: the same tick gives the same result in every run. Flow fields and roots are
//! derived from the walls and rooting stones the rules hold, cached on
//! [`NativeCtx::generation`] and the wall set (a rebuild costs a few Dijkstra passes).
//!
//! The port follows `sim.rs` step for step; `tests/lex_parity.rs` compares the two every tick.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use specula_core::Json;
use specula_lex::rules::{NativeDef, Rules, P3};
use specula_lex::{Document, Sim};
use specula_lex::{NativeCtx, NativeHook, Natives};

use crate::fixed::Pos;
use crate::map::{CellId, CellKind, Face, Map};
use crate::path::{Class, Fields, Route, UNREACHABLE};
use crate::sim::STRANDED_TICKS;

/// The hooks, by the names the rule set's native items use.
pub fn natives() -> Natives {
    let mut n = Natives::new();
    n.insert("sepulcra.ground".into(), Arc::new(Hook::Unbound(Kind::Ground)) as Arc<dyn NativeHook>);
    n.insert("sepulcra.air".into(), Arc::new(Hook::Unbound(Kind::Air)) as Arc<dyn NativeHook>);
    n.insert("sepulcra.spot".into(), Arc::new(Hook::Unbound(Kind::Spot)) as Arc<dyn NativeHook>);
    n.insert("sepulcra.wall_clear".into(), Arc::new(Hook::Unbound(Kind::WallClear)) as Arc<dyn NativeHook>);
    n
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Ground,
    Air,
    Spot,
    WallClear,
}

enum Hook {
    /// Registered, not yet bound to a rule set.
    Unbound(Kind),
    Bound(Box<Bound>),
}

/// Attribute ids the hooks read and write (resolved once per run).
struct Attrs {
    c_from: u16,
    c_to: u16,
    prog: u16,
    elen: u16,
    leap: u16,
    carry: u16,
    stuck: u16,
    cell: u16,
    face: u16,
    speed: u16,
    speed_scale: u16,
    climb_speed: u16,
    wall_dps: u16,
    jump_chance: u16,
    jump_detour: u16,
    hp: u16,
    ring: u16,
    dir: u16,
    hops: u16,
    diving: u16,
    rim_n: u16,
    rim_e: u16,
    rim_s: u16,
    rim_w: u16,
    rim_top: u16,
}

struct Tags {
    climber: u16,
    leaper: u16,
    wall: u16,
    roots: u16,
    ground: u16,
}

struct Bound {
    kind: Kind,
    map: Arc<Map>,
    /// Cell centre (mm) → cell, for entities placed on cells (stones, walls, spawns).
    at: BTreeMap<[i64; 3], CellId>,
    a: Attrs,
    t: Tags,
    lane_jump: u16,
    cache: Mutex<Cache>,
}

/// What the ground hook derives from a run's walls and rooting stones. Every entry is keyed by
/// everything it was derived from, so clones of a run stepping on several threads (lex_simulate
/// runs continued `from` one saved run) share entries only when their states agree; each call
/// takes everything it uses in one locked lookup.
#[derive(Default)]
struct Cache {
    /// Views by (generation, breach penalty, root radius), newest last.
    views: Vec<(ViewKey, View)>,
    /// Flow fields and blocked cells by (wall cells, breach penalty), newest last.
    fields: Vec<FieldsEntry>,
    /// Rooted cells by (rooting stones' cells, radius), newest last.
    rooted: Vec<(Vec<CellId>, i64, Arc<Vec<bool>>)>,
}

type ViewKey = ((u64, u64), i64, i64);
/// (wall cells, breach penalty) and what they give: flow fields and blocked cells.
type FieldsEntry = (Vec<CellId>, i64, Arc<Fields>, Arc<Vec<bool>>);

/// Entries each list keeps (a handful of states stepping at once, a few wall layouts).
const CACHED: usize = 16;

fn remember<T>(list: &mut Vec<T>, item: T) {
    if list.len() >= CACHED {
        list.remove(0);
    }
    list.push(item);
}

/// One call's view of the derived state: all of it from the same state.
#[derive(Clone)]
struct View {
    fields: Arc<Fields>,
    blocked: Arc<Vec<bool>>,
    rooted: Arc<Vec<bool>>,
    /// Wall cells with their entity, by cell.
    walls: Arc<Vec<(CellId, u32)>>,
}

impl View {
    fn wall_at(&self, cell: CellId) -> Option<u32> {
        self.walls.binary_search_by_key(&cell, |(w, _)| *w).ok().map(|i| self.walls[i].1)
    }
}

impl NativeHook for Hook {
    fn run(&self, cx: &mut NativeCtx<'_, '_>) -> Result<(), String> {
        match self {
            Hook::Unbound(_) => Err("sepulcra hooks bind to a rule set first (Sim::new does it)".into()),
            Hook::Bound(b) => match b.kind {
                Kind::Ground => b.ground(cx),
                Kind::Air => b.air(cx),
                Kind::Spot => b.spot(cx),
                Kind::WallClear => b.wall_clear(cx),
            },
        }
    }

    fn bind(&self, rules: &Rules, native: &NativeDef) -> Result<Option<Arc<dyn NativeHook>>, String> {
        let Hook::Unbound(kind) = self else { return Ok(None) };
        Ok(Some(Arc::new(Hook::Bound(Box::new(Bound::new(*kind, rules, native)?)))))
    }
}

fn p3(p: Pos) -> P3 {
    [i64::from(p.x), i64::from(p.y), i64::from(p.z)]
}

fn pos_of(p: P3) -> Pos {
    let c = |v: i64| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    Pos::new(c(p[0]), c(p[1]), c(p[2]))
}

fn face_index(f: Face) -> i64 {
    match f {
        Face::Top => 0,
        Face::North => 1,
        Face::East => 2,
        Face::South => 3,
        Face::West => 4,
    }
}

/// Bounds on what the rules feed the hooks (attributes are data an agent may set to anything):
/// speeds up to 1,000 km/s, ratios up to 1000x, edges up to 1 km. Past them a value is clamped
/// (speeds, ratios) or refused as an error that names it (movement state).
const MAX_SPEED: i64 = 1_000_000_000;
const MAX_RATIO: i64 = 1_000_000;
const MAX_EDGE: i64 = 1_000_000;
const MAX_HOPS: i64 = 1_000_000;

/// A movement state (`prog` of `len` mm along an edge) the hooks can trust.
fn edge_state(me: u32, prog: i64, len: i64) -> Result<(), String> {
    if (0..=MAX_EDGE).contains(&len) && (0..=len).contains(&prog) {
        Ok(())
    } else {
        Err(format!("entity {me}'s movement state is out of range (prog {prog} of elen {len} mm; 0 <= prog <= elen <= {MAX_EDGE}): an action set it?"))
    }
}

impl Bound {
    fn new(kind: Kind, rules: &Rules, native: &NativeDef) -> Result<Bound, String> {
        let data = native.data.first().and_then(|d| rules.data.get(usize::from(*d))).ok_or("declare the slab: \"data\": [\"slab\"] on the native item (a data item holding the slab JSON)")?;
        let map = Map::parse(&data.value.to_string(), &format!("data item '{}'", data.id)).map_err(|e| format!("the slab in data item '{}' is invalid: {e}", data.id))?;
        let at = map.cells().iter().map(|c| (p3(c.pos), c.id)).collect();
        let attr = |n: &str| rules.attr(n).ok_or_else(|| format!("the rules have no attribute '{n}', which {} reads (see games/sepulcra/src/lex.rs)", native.hook));
        // Only the hooks that use an attribute require it.
        let opt = |n: &str| rules.attr(n).unwrap_or(u16::MAX);
        let need = |n: &str, used: bool| if used { attr(n) } else { Ok(opt(n)) };
        let (g, air, spot) = (kind == Kind::Ground || kind == Kind::WallClear, kind == Kind::Air, kind == Kind::Spot);
        let a = Attrs {
            c_from: need("c_from", g)?,
            c_to: need("c_to", g)?,
            prog: need("prog", g || air)?,
            elen: need("elen", g || air)?,
            leap: need("leap", g)?,
            carry: need("carry", g || air)?,
            stuck: need("stuck", g)?,
            cell: need("cell", g || spot)?,
            face: need("face", g || air || spot)?,
            speed: need("speed", g || air)?,
            speed_scale: need("speed_scale", g || air)?,
            climb_speed: need("climb_speed", kind == Kind::Ground)?,
            wall_dps: need("wall_dps", kind == Kind::Ground)?,
            jump_chance: need("jump_chance", kind == Kind::Ground)?,
            jump_detour: need("jump_detour", kind == Kind::Ground)?,
            hp: need("hp", kind == Kind::Ground)?,
            ring: need("ring", air)?,
            dir: need("dir", air)?,
            hops: need("hops", air)?,
            diving: need("diving", air)?,
            rim_n: need("rim_n", spot)?,
            rim_e: need("rim_e", spot)?,
            rim_s: need("rim_s", spot)?,
            rim_w: need("rim_w", spot)?,
            rim_top: need("rim_top", spot)?,
        };
        let tag = |n: &str| rules.tag(n).unwrap_or(u16::MAX);
        let t = Tags { climber: tag("climber"), leaper: tag("leaper"), wall: tag("wall"), roots: tag("roots"), ground: tag("ground") };
        let lane_jump = if kind == Kind::Ground {
            native.streams.first().copied().ok_or("declare the jump stream: \"streams\": [\"lane_jump\"] on the native item")?
        } else {
            0
        };
        Ok(Bound { kind, map: Arc::new(map), at, a, t, lane_jump, cache: Mutex::new(Cache::default()) })
    }

    fn cell_at(&self, p: P3) -> Option<CellId> {
        self.at.get(&p).copied()
    }

    /// The flow fields, blocked cells, rooted cells and walls for this state's walls and stones.
    fn view(&self, cx: &mut NativeCtx<'_, '_>, breach: i64, radius: i64) -> Result<View, String> {
        let key: ViewKey = (cx.generation(), breach, radius);
        let mut c = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((_, v)) = c.views.iter().rev().find(|(k, _)| *k == key) {
            return Ok(v.clone());
        }
        let (mut walls, mut roots) = (Vec::new(), Vec::new());
        let scanned = cx.each_tagged(self.t.wall, |id, p| {
            if let Some(cell) = self.cell_at(p) {
                walls.push((cell, id));
            }
        }) && cx.each_tagged(self.t.roots, |_, p| {
            if let Some(cell) = self.cell_at(p) {
                roots.push(cell);
            }
        });
        if !scanned {
            return Err("work_limit: the tick's work budget ran out while finding walls".into());
        }
        walls.sort();
        roots.sort();
        let wall_cells: Vec<CellId> = walls.iter().map(|(w, _)| *w).collect();
        let (fields, blocked) = match c.fields.iter().rev().find(|(w, b, _, _)| *w == wall_cells && *b == breach) {
            Some((_, _, f, bl)) => (Arc::clone(f), Arc::clone(bl)),
            None => {
                let mut blocked = vec![false; self.map.cells().len()];
                for w in &wall_cells {
                    blocked[w.index()] = true;
                }
                let penalty = breach.clamp(0, i64::from(i32::MAX)) as i32;
                let (f, b) = (Arc::new(Fields::build(&self.map, &blocked, penalty)), Arc::new(blocked));
                remember(&mut c.fields, (wall_cells, breach, Arc::clone(&f), Arc::clone(&b)));
                (f, b)
            }
        };
        let rooted = match c.rooted.iter().rev().find(|(r, rad, _)| *r == roots && *rad == radius) {
            Some((_, _, r)) => Arc::clone(r),
            None => {
                let mut rooted = vec![false; self.map.cells().len()];
                let r2 = i64::from(radius.clamp(0, i64::from(i32::MAX)) as i32).pow(2);
                for &s in &roots {
                    let sc = self.map.cell(s);
                    for other in self.map.cells() {
                        if other.face == sc.face && other.kind.walkable(true) && other.pos.dist2(sc.pos) <= r2 {
                            rooted[other.id.index()] = true;
                        }
                    }
                }
                let r = Arc::new(rooted);
                remember(&mut c.rooted, (roots, radius, Arc::clone(&r)));
                r
            }
        };
        let v = View { fields, blocked, rooted, walls: Arc::new(walls) };
        remember(&mut c.views, (key, v.clone()));
        Ok(v)
    }

    fn cell_id(&self, v: i64, what: &str) -> Result<CellId, String> {
        if (0..self.map.cells().len() as i64).contains(&v) {
            Ok(CellId(v as u16))
        } else {
            Err(format!("{what} {v} is not a cell of the slab"))
        }
    }

    /// Converts this tick's speed into whole millimetres, carrying the remainder (`sim.rs`).
    /// Inputs are clamped (speed to [0, MAX_SPEED], permille to [0, MAX_RATIO], the carry to a
    /// valid remainder), so nothing overflows.
    fn budget(speed: i64, permille: i64, carry: &mut i64, hz: u32) -> i64 {
        let den = i64::from(hz.max(1)) * 1000;
        let speed = speed.clamp(0, MAX_SPEED);
        let permille = permille.clamp(0, MAX_RATIO);
        let num = speed.saturating_mul(permille).saturating_add((*carry).clamp(0, den - 1));
        *carry = num % den;
        num / den
    }

    fn ground(&self, cx: &mut NativeCtx<'_, '_>) -> Result<(), String> {
        let me = cx.entity().ok_or("sepulcra.ground runs for an entity")?;
        let a = &self.a;
        let map = &*self.map;
        let base = |cx: &NativeCtx, attr: u16| cx.base(me, attr).ok_or_else(|| format!("entity {me} lacks an attribute sepulcra.ground needs (see games/sepulcra/src/lex.rs)"));
        let breach = cx.arg("breach").unwrap_or(0);
        let root_slow = cx.arg("root_slow").unwrap_or(0).clamp(0, 1000);
        let radius = cx.arg("root_radius").unwrap_or(0);
        let missing = || format!("entity {me} lacks an attribute sepulcra.ground needs (see games/sepulcra/src/lex.rs)");
        let [mut from, mut to, mut prog, mut len, leap0, mut carry, mut stuck] = cx.base_many(me, [a.c_from, a.c_to, a.prog, a.elen, a.leap, a.carry, a.stuck]).ok_or_else(missing)?;
        let mut leap = leap0 != 0;
        if to < 0 {
            // Fresh: stand still on the spawn cell (a summoner hands over its cell in c_from).
            let start = if from >= 0 { self.cell_id(from, "c_from")? } else { self.cell_at(cx.position(me).unwrap_or_default()).ok_or_else(|| format!("entity {me} spawned off the slab's cells"))? };
            (from, to, prog, len, leap) = (i64::from(start.0), i64::from(start.0), 0, 0, true);
        }
        edge_state(me, prog, len)?;
        let mut stuck_ticks = stuck.max(0);
        let mut from_c = self.cell_id(from, "c_from")?;
        let mut to_c = self.cell_id(to, "c_to")?;
        let class = if cx.has(me, self.t.climber) { Class::Climber } else { Class::Walker };
        let jumper = cx.has(me, self.t.leaper);
        let mut v = self.view(cx, breach, radius)?;
        // Speed: slows (speed_scale, with effects), then roots on the cell it is nearer to, then
        // climbing. Each factor is clamped, so the products stay far inside i64.
        let mut f = cx.get(me, a.speed_scale).unwrap_or(1000).clamp(0, MAX_RATIO);
        let near = if prog * 2 < len { from_c } else { to_c };
        if v.rooted[near.index()] {
            f = f * (1000 - root_slow) / 1000;
        }
        let climbing = map.cell(from_c).kind.is_climb() || map.cell(to_c).kind.is_climb();
        if climbing {
            f = f * base(cx, a.climb_speed)?.clamp(0, MAX_RATIO) / 1000;
        }
        let speed = cx.get(me, a.speed).unwrap_or(0);
        let mut left = Self::budget(speed, f, &mut carry, cx.tick_hz());
        let mut end: Option<&str> = None;
        // A few edges per tick at most; the bound only guards against zero-length loops.
        for _ in 0..8 {
            if prog < len {
                let adv = left.min(len - prog);
                left -= adv;
                prog += adv;
                if prog < len {
                    break;
                }
                continue;
            }
            let at = to_c;
            let kind = map.cell(at).kind;
            if kind == CellKind::Graveyard {
                end = Some("arrive");
                break;
            }
            let chance = base(cx, a.jump_chance)?;
            // The seeded roll: an optional shortcut while a walking route exists.
            if !leap && kind == CellKind::Jump && jumper && chance > 0 {
                if let Some(p) = self.jump_target(&v, class, at, base(cx, a.jump_detour)?) {
                    if cx.chance(self.lane_jump, chance) {
                        (from_c, to_c, prog, len, leap) = (at, p, 0, i64::from(map.jump_cost(at, p)), true);
                        continue;
                    }
                }
            }
            let route = v.fields.route(map, class, jumper, &v.blocked, at);
            if route != Route::Stuck {
                stuck_ticks = 0;
            }
            match route {
                Route::Step(n) => {
                    let l = map.links(at).iter().find(|l| l.to == n).map_or(0, |l| l.cost);
                    (from_c, to_c, prog, len, leap) = (at, n, 0, i64::from(l), false);
                    if left == 0 {
                        break;
                    }
                }
                Route::Attack(w) => {
                    (from_c, to_c, prog, len, leap) = (at, at, 0, 0, true);
                    carry = 0;
                    // Per second in the rules, per tick here, truncated like content.rs does.
                    let dmg = base(cx, a.wall_dps)?.max(0) / i64::from(cx.tick_hz().max(1));
                    if let Some(wall) = v.wall_at(w) {
                        let hp = cx.base(wall, a.hp).ok_or_else(|| format!("wall entity {wall} has no hp"))?;
                        cx.set(wall, a.hp, hp.saturating_sub(dmg))?;
                        // A broken wall reopens routes for everyone after this enemy, and for it.
                        v = self.view(cx, breach, radius)?;
                    }
                    break;
                }
                Route::Leap(p) => {
                    (from_c, to_c, prog, len, leap) = (at, p, 0, i64::from(map.jump_cost(at, p)), true);
                }
                Route::Stuck => {
                    (from_c, to_c, prog, len, leap) = (at, at, 0, 0, true);
                    carry = 0;
                    stuck_ticks = stuck_ticks.saturating_add(1);
                    if stuck_ticks >= i64::from(STRANDED_TICKS) {
                        end = Some("stranded");
                    }
                    break;
                }
            }
        }
        stuck = stuck_ticks;
        // Where it stands now, and how far it still has to go ("first" targeting sorts by it).
        // prog and len are within [0, MAX_EDGE] here (validated, or a slab edge).
        let p = Pos::lerp(map.cell(from_c).pos, map.cell(to_c).pos, prog as i32, len as i32);
        let rest = v.fields.remaining(class, jumper, to_c);
        let remaining = if rest == UNREACHABLE { UNREACHABLE } else { rest.saturating_add((len - prog) as i32) };
        let near = if prog * 2 < len { from_c } else { to_c };
        cx.set_many(
            me,
            &[
                (a.c_from, i64::from(from_c.0)),
                (a.c_to, i64::from(to_c.0)),
                (a.prog, prog),
                (a.elen, len),
                (a.leap, i64::from(leap)),
                (a.carry, carry),
                (a.stuck, stuck),
                (a.cell, i64::from(near.0)),
                (a.face, face_index(map.cell(near).face)),
            ],
        )?;
        cx.set_pos(me, p3(p))?;
        cx.set_path_remaining(me, i64::from(remaining))?;
        if let Some(list) = end {
            cx.trigger(list)?;
        }
        Ok(())
    }

    /// The partner a jumper may leap to by chance from jump point `at` while it still has a
    /// walking route: open, reaching the graveyard on foot, at most `detour` farther from it.
    fn jump_target(&self, v: &View, class: Class, at: CellId, detour: i64) -> Option<CellId> {
        let open = v.fields.open(class);
        let here = open[at.index()];
        if here == UNREACHABLE {
            return None;
        }
        let mut best: Option<CellId> = None;
        for &p in self.map.jump_partners(at) {
            let d = open[p.index()];
            if v.blocked[p.index()] || d == UNREACHABLE || i64::from(d) > i64::from(here).saturating_add(detour) {
                continue;
            }
            if best.is_none_or(|b| d < open[b.index()]) {
                best = Some(p);
            }
        }
        best
    }

    /// The air leg from ring point `at`: the next ring point while hops remain, else the dive.
    /// Returns (ring index of the leg's end, or of its start when diving; hops left; diving; length).
    fn air_leg(&self, at: usize, dir: i64, hops: i64) -> (usize, i64, bool, i64) {
        let air = &self.map.air;
        let n = air.ring.len() as i64;
        if hops > 0 {
            let next = (at as i64 + dir).rem_euclid(n) as usize;
            (next, hops - 1, false, i64::from(air.ring[at].dist(air.ring[next])))
        } else {
            (at, 0, true, i64::from(air.ring[at].dist(air.dive_target)))
        }
    }

    fn air(&self, cx: &mut NativeCtx<'_, '_>) -> Result<(), String> {
        let me = cx.entity().ok_or("sepulcra.air runs for an entity")?;
        let a = &self.a;
        let air = &self.map.air;
        let n = air.ring.len() as i64;
        let missing = || format!("entity {me} lacks an attribute sepulcra.air needs (see games/sepulcra/src/lex.rs)");
        let [mut ring, dir, hops0, diving0, mut prog, mut len, mut carry] = cx.base_many(me, [a.ring, a.dir, a.hops, a.diving, a.prog, a.elen, a.carry]).ok_or_else(missing)?;
        let mut diving = diving0 != 0;
        let dir = if dir < 0 { -1 } else { 1 };
        let mut hops = hops0.clamp(0, MAX_HOPS);
        if ring < 0 {
            let here = cx.position(me).unwrap_or_default();
            let start = air.ring.iter().position(|p| p3(*p) == here).ok_or_else(|| format!("flyer {me} must spawn on a ring point (an air entry site)"))?;
            let (r, h, d, l) = self.air_leg(start, dir, hops);
            (ring, hops, diving, prog, len) = (r as i64, h, d, 0, l);
        }
        if !(0..n).contains(&ring) {
            return Err(format!("ring {ring} is not a ring point"));
        }
        edge_state(me, prog, len)?;
        let f = cx.get(me, a.speed_scale).unwrap_or(1000);
        let speed = cx.get(me, a.speed).unwrap_or(0);
        let mut left = Self::budget(speed, f, &mut carry, cx.tick_hz());
        let mut arrived = false;
        for _ in 0..8 {
            if prog < len {
                let adv = left.min(len - prog);
                left -= adv;
                prog += adv;
                if prog < len {
                    break;
                }
                continue;
            }
            if diving {
                arrived = true;
                break;
            }
            let (r, h, d, l) = self.air_leg(ring as usize, dir, hops);
            (ring, hops, diving, prog, len) = (r as i64, h, d, 0, l);
        }
        let to = if diving { air.dive_target } else { air.ring[ring as usize] };
        let from = if diving { air.ring[ring as usize] } else { air.ring[(ring - dir).rem_euclid(n) as usize] };
        let p = Pos::lerp(from, to, prog as i32, len as i32);
        let seg = i64::from(air.ring[0].dist(air.ring[1]));
        let remaining = (len - prog).saturating_add(hops.saturating_mul(seg)).saturating_add(if diving { 0 } else { i64::from(air.ring_half) });
        cx.set_many(me, &[(a.ring, ring), (a.hops, hops), (a.diving, i64::from(diving)), (a.prog, prog), (a.elen, len), (a.carry, carry), (a.face, -1)])?;
        cx.set_pos(me, p3(p))?;
        cx.set_path_remaining(me, remaining)?;
        if arrived {
            cx.trigger("arrive")?;
        }
        Ok(())
    }

    /// A stone's cell, face and rim distances (for the range-across-faces rule).
    fn spot(&self, cx: &mut NativeCtx<'_, '_>) -> Result<(), String> {
        let me = cx.entity().ok_or("sepulcra.spot runs for an entity (a stone's on_spawn)")?;
        let a = &self.a;
        let p = cx.position(me).unwrap_or_default();
        let cell = self.cell_at(p).ok_or_else(|| format!("entity {me} stands off the slab's cells"))?;
        let c = self.map.cell(cell);
        let far = i64::from(i32::MAX);
        let rim = |other: Face| self.map.rim_distance(c.pos, c.face, other).map_or(far, i64::from);
        cx.set_many(
            me,
            &[
                (a.cell, i64::from(cell.0)),
                (a.face, face_index(c.face)),
                (a.rim_n, rim(Face::North)),
                (a.rim_e, rim(Face::East)),
                (a.rim_s, rim(Face::South)),
                (a.rim_w, rim(Face::West)),
                (a.rim_top, rim(Face::Top)),
            ],
        )
    }

    /// Refuses a wall on a cell a ground enemy stands on or is stepping into.
    fn wall_clear(&self, cx: &mut NativeCtx<'_, '_>) -> Result<(), String> {
        let (Some(x), Some(y), Some(z)) = (cx.arg("x"), cx.arg("y"), cx.arg("z")) else {
            return Err("sepulcra.wall_clear needs args x, y and z (the site's position: arg.site.x ...)".into());
        };
        let cell = self.cell_at([x, y, z]).ok_or("the site is not a cell of the slab")?;
        let a = &self.a;
        let mut ground = Vec::new();
        if !cx.each_tagged(self.t.ground, |id, p| ground.push((id, p))) {
            return Err("work_limit: the work budget ran out while looking for enemies".into());
        }
        let blocker = ground.into_iter().find_map(|(id, p)| {
            let (from, to) = (cx_base(cx, id, a.c_from), cx_base(cx, id, a.c_to));
            let (from, to) = if to < 0 {
                let c = if from >= 0 { from } else { self.cell_at(p).map_or(-1, |c| i64::from(c.0)) };
                (c, c)
            } else {
                (from, to)
            };
            (from == i64::from(cell.0) || to == i64::from(cell.0)).then_some(id)
        });
        match blocker {
            Some(id) => Err(cx.refuse("enemy_in_the_way", format!("enemy {id} stands on or is stepping into {}", self.map.cell_name(cell)))),
            None => Ok(()),
        }
    }
}
fn cx_base(cx: &NativeCtx<'_, '_>, id: u32, attr: u16) -> i64 {
    cx.base(id, attr).unwrap_or(-1)
}

// ---- commands and comparisons (the parity tests and the app's Lex mode) ----

/// A hand-written sim command as the rule set's command name and args.
pub fn command_json(content: &crate::Content, cmd: &crate::Command) -> (&'static str, Json) {
    use crate::Command as C;
    let cell = |c: CellId| Json::from(content.map.cell_name(c));
    match *cmd {
        C::PlaceStone { cell: c, stone } => ("place", Json::obj([("site", cell(c)), ("stone", Json::from(format!("{}_1", content.stone(stone).id)))])),
        C::Upgrade { cell: c } => ("upgrade", Json::obj([("stone", cell(c))])),
        C::Sell { cell: c } => ("sell", Json::obj([("stone", cell(c))])),
        C::PlaceWall { cell: c } => ("wall", Json::obj([("site", cell(c))])),
        C::RemoveWall { cell: c } => ("unwall", Json::obj([("wall", cell(c))])),
        C::CallWave => ("call_wave", Json::Obj(Vec::new())),
    }
}

/// A hand-written sim command for a Lex run (an arg that names nothing, like a stone on a lane
/// cell, is the run's refusal).
pub fn command_call(sim: &Sim, content: &crate::Content, cmd: &crate::Command) -> Result<specula_lex::CommandCall, specula_lex::Rejection> {
    let (name, args) = command_json(content, cmd);
    sim.parse_command(name, &args)
}

/// What both sims show each tick, in the same units: the parity tests and the app compare it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub tick: u32,
    pub dust: i64,
    pub tombstones: i64,
    pub wave: u16,
    pub outcome: Option<String>,
    /// (position mm, hp milli) per living enemy, by id.
    pub enemies: Vec<([i64; 3], i64)>,
    pub projectiles: Vec<[i64; 3]>,
    /// (cell, archetype id) per stone, by cell.
    pub stones: Vec<(u16, String)>,
    /// (cell, hp milli) per wall, by cell.
    pub walls: Vec<(u16, i64)>,
}

impl Snapshot {
    /// The hand-written sim's view.
    pub fn of_state(s: &crate::State) -> Snapshot {
        let c = s.content();
        Snapshot {
            tick: s.tick(),
            dust: i64::from(s.dust()),
            tombstones: i64::from(s.tombstones()),
            wave: s.wave(),
            outcome: match s.outcome() {
                crate::Outcome::Running => None,
                crate::Outcome::Won => Some("won".into()),
                crate::Outcome::Lost => Some("lost".into()),
            },
            enemies: s.enemies().iter().map(|e| (p3(e.pos), i64::from(e.hp))).collect(),
            projectiles: s.projectiles().iter().map(|p| p3(p.pos)).collect(),
            stones: s.stones().map(|(cell, st)| (cell.0, format!("{}_{}", c.stone(st.kind).id, st.tier + 1))).collect(),
            walls: s.walls().map(|(cell, w)| (cell.0, i64::from(w.hp))).collect(),
        }
    }

    /// A Lex run's view (`map`: the slab, to name cells).
    pub fn of_lex(l: &Sim, map: &Map) -> Snapshot {
        let r = l.rules();
        let has = |e: &specula_lex::sim::Entity, t: Option<u16>| t.is_some_and(|t| r.archetypes[usize::from(e.arch)].tags.binary_search(&t).is_ok());
        let (enemy, projectile, stone, wall) = (r.tag("enemy"), r.tag("projectile"), r.tag("stone"), r.tag("wall"));
        let cell_of = |p: P3| map.cells().iter().find(|c| p3(c.pos) == p).map_or(u16::MAX, |c| c.id.0);
        let hp = |id: u32| l.attr(id, "hp").unwrap_or(0);
        let live = || l.entities().iter().filter(|e| e.alive);
        let mut stones: Vec<(u16, String)> = live().filter(|e| has(e, stone)).map(|e| (cell_of(e.pos), r.archetypes[usize::from(e.arch)].id.clone())).collect();
        stones.sort();
        let mut walls: Vec<(u16, i64)> = live().filter(|e| has(e, wall)).map(|e| (cell_of(e.pos), hp(e.id))).collect();
        walls.sort();
        Snapshot {
            tick: l.tick(),
            dust: l.resource("dust").unwrap_or(0),
            tombstones: l.resource("tombstones").unwrap_or(0),
            wave: l.state().wave,
            outcome: l.outcome().map(|(res, _)| res.to_owned()),
            enemies: live().filter(|e| has(e, enemy)).map(|e| (e.pos, hp(e.id))).collect(),
            projectiles: live().filter(|e| has(e, projectile)).map(|e| e.pos).collect(),
            stones,
            walls,
        }
    }

    /// The first difference, said briefly (`None`: equal).
    pub fn diff(&self, lex: &Snapshot) -> Option<String> {
        if self == lex {
            return None;
        }
        let what = if self.enemies.len() != lex.enemies.len() {
            format!("enemy count {} vs {}", self.enemies.len(), lex.enemies.len())
        } else if let Some(i) = (0..self.enemies.len()).find(|i| self.enemies[*i] != lex.enemies[*i]) {
            format!("enemy #{i}: rust {:?} lex {:?}", self.enemies[i], lex.enemies[i])
        } else if self.projectiles != lex.projectiles {
            format!("projectiles: rust {:?} lex {:?}", self.projectiles, lex.projectiles)
        } else if self.stones != lex.stones {
            format!("stones: rust {:?} lex {:?}", self.stones, lex.stones)
        } else if self.walls != lex.walls {
            format!("walls: rust {:?} lex {:?}", self.walls, lex.walls)
        } else {
            format!("dust {} vs {}, tombstones {} vs {}, wave {} vs {}, outcome {:?} vs {:?}", self.dust, lex.dust, self.tombstones, lex.tombstones, self.wave, lex.wave, self.outcome, lex.outcome)
        };
        Some(what)
    }
}

// ---- the documents ----

/// The rules half of the rule set (hand-written).
pub const RULES_LEX: &str = include_str!("../lex/sepulcra.lex.json");
/// The slab half: the level as Lex items (generated by `examples/lex_slab.rs`).
pub const SLAB_LEX: &str = include_str!("../lex/slab_01.lex.json");

/// The whole rule set: the rules with the slab's items merged in.
pub fn document() -> Result<Document, String> {
    let show = |d: Vec<specula_lex::Diagnostic>| d.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("\n");
    let mut rules = Document::parse(RULES_LEX).map_err(show)?;
    let slab = Document::parse(SLAB_LEX).map_err(show)?;
    for (id, item) in slab.items {
        if rules.items.insert(id.clone(), item).is_some() {
            return Err(format!("item '{id}' is in both lex/sepulcra.lex.json and lex/slab_01.lex.json"));
        }
    }
    Ok(rules)
}

/// The compiled rule set (with `params` overrides), or every diagnostic.
pub fn compiled(params: &[(&str, Json)]) -> Result<Arc<Rules>, String> {
    let doc = document()?;
    let opts = specula_lex::Options { overrides: params.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect(), ..Default::default() };
    match specula_lex::compile(&doc, &opts) {
        Ok(c) => Ok(Arc::new(c.rules)),
        Err(d) => Err(d.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("\n")),
    }
}

/// A fresh run of the rule set with Sepulcra's hooks.
pub fn sim(rules: Arc<Rules>, seed: u64) -> Result<Sim, String> {
    Sim::new(rules, seed, &natives())
}

/// Lex positions are millimetres, like the hand-written sim's.
fn metres(mm: i32) -> Json {
    let s = format!("{}.{:03}", mm / 1000, (mm % 1000).abs());
    let s = s.trim_end_matches('0').trim_end_matches('.');
    let s = if mm < 0 && !s.starts_with('-') { format!("-{s}") } else { s.to_owned() };
    Json::parse(&s).expect("a decimal")
}

fn point(p: Pos) -> Json {
    Json::arr([metres(p.x), metres(p.y), metres(p.z)])
}

/// The slab's items as a Lex document's text: the map as a `data` item, plus sites for build
/// spots (fragments and ledges), wall spots (top-face lanes), spawn points and air entries.
pub fn slab_document_text(slab_json: &str) -> Result<String, String> {
    let map = Map::parse(slab_json, "slab_01.json").map_err(|e| e.to_string())?;
    let value = Json::parse(slab_json).map_err(|e| format!("slab_01.json: {}", e.message))?;
    let sites = |f: &dyn Fn(CellKind) -> bool| Json::Obj(map.cells().iter().filter(|c| f(c.kind)).map(|c| (map.cell_name(c.id), point(c.pos))).collect());
    let spawns = Json::Obj(map.spawns.iter().map(|s| (s.name.clone(), point(map.cell(s.cell).pos))).collect());
    let entries = Json::Obj(map.air.entries.iter().enumerate().map(|(i, e)| (format!("air_{i}"), point(map.air.ring[usize::from(*e)]))).collect());
    let item = |kind: &str, notes: &str, key: &str, v: Json| Json::obj([("kind", Json::from(kind)), ("notes", Json::from(notes)), (key, v)]);
    let items = Json::obj([
        ("air_entries", item("sites", "Where flyers appear: the air ring's entry points (air.entries in the slab), 4 m up.", "points", entries)),
        ("build_spots", item("sites", "Every cell a stone can be carved on: top-face fragments (#) and cliff ledges, named face:col,row like the hand-written sim's cells.", "points", sites(&|k| k.buildable()))),
        ("slab", item("data", "The slab (data/slab_01.json): the cell graph the movement hooks walk, the cliffs, the jump pairs and the air ring.", "value", value)),
        ("spawn_points", item("sites", "The gorges and cliff bases enemies emerge from, in the slab's order (waves draw from these).", "points", spawns)),
        ("wall_spots", item("sites", "Every cell a wall can stand on: top-face lanes (fissures and jump points).", "points", sites(&|k| k.wallable()))),
    ]);
    let doc = Json::obj([
        ("lex", Json::from(1u32)),
        ("name", Json::from("sepulcra_slab_01")),
        ("tick_hz", Json::from(crate::fixed::TICK_HZ)),
        ("notes", Json::from("Generated from data/slab_01.json by examples/lex_slab.rs (don't edit by hand): Shattered Slab I as Lex items, merged with lex/sepulcra.lex.json (the rules) into one rule set.")),
        ("items", items),
    ]);
    let mut out = String::new();
    pretty(&doc, 0, &mut out);
    out.push('\n');
    Ok(out)
}

/// JSON with every value that fits in 110 characters on one line.
pub fn pretty(v: &Json, indent: usize, out: &mut String) {
    let compact = v.to_string();
    let (open, close, pairs): (char, char, Vec<(Option<&str>, &Json)>) = match v {
        Json::Obj(p) => ('{', '}', p.iter().map(|(k, x)| (Some(k.as_str()), x)).collect()),
        Json::Arr(a) => ('[', ']', a.iter().map(|x| (None, x)).collect()),
        _ => {
            out.push_str(&compact);
            return;
        }
    };
    if compact.len() + indent <= 110 || pairs.is_empty() {
        // Spaced like a person would write it.
        out.push_str(&spaced(v));
        return;
    }
    out.push(open);
    for (i, (k, x)) in pairs.iter().enumerate() {
        out.push('\n');
        out.push_str(&" ".repeat(indent + 2));
        if let Some(k) = k {
            out.push_str(&Json::from(*k).to_string());
            out.push_str(": ");
        }
        pretty(x, indent + 2, out);
        if i + 1 < pairs.len() {
            out.push(',');
        }
    }
    out.push('\n');
    out.push_str(&" ".repeat(indent));
    out.push(close);
}

fn spaced(v: &Json) -> String {
    match v {
        Json::Obj(p) if !p.is_empty() => format!("{{ {} }}", p.iter().map(|(k, x)| format!("{}: {}", Json::from(k.as_str()), spaced(x))).collect::<Vec<_>>().join(", ")),
        Json::Arr(a) => format!("[{}]", a.iter().map(spaced).collect::<Vec<_>>().join(", ")),
        other => other.to_string(),
    }
}

/// Where a Lex position is on the slab, for presentation and tests.
pub fn pos(p: P3) -> Pos {
    pos_of(p)
}
