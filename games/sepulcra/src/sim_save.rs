//! Save games: the simulation [`State`] as bytes (docs/design/SAVES.md).
//!
//! Everything that changes during a run is written, integers as they are, in a fixed order;
//! what is derived from it (walls' blocked cells, the flow fields, rooted cells) is rebuilt on
//! load, exactly as the tick would have it. The content isn't saved: a save names its content
//! hash, and loading it against other content is refused (`content_changed`), because the same
//! state under other rules is another game. Every index a save holds (cells, kinds, tiers,
//! waves, ring positions) is checked against the content, so a damaged or hand-made save is an
//! error, never a panic later in the tick.

use super::*;
use specula_core::bytes::{ByteReader, ByteWriter, DecodeError};

/// Bumped when the layout changes; older layouts get a migration in [`State::from_bytes`].
/// 3: rubble keeps the tick it rose, and the cells whose rubble has gone are saved (a version 2
/// save loads with each rubble's life counted from its load and no cell spent).
pub const STATE_VERSION: u32 = 3;
/// The elements' layout before rubble was temporary by its own rules (still read).
const STATE_VERSION_2: u32 = 2;
/// The layout of content without the elements (the shipped maps), still written for it: a save of
/// the shipped game is byte for byte what it was before the elements existed.
pub const STATE_VERSION_PLAIN: u32 = 1;
/// Most enemies, projectiles or pending spawns a save may hold (far above any real run).
const MAX_ITEMS: usize = 1 << 20;

/// Why saved state couldn't be read: a machine-readable `code` (`bad_state`, `content_changed`,
/// `unsupported_version`), what, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateError {
    pub code: &'static str,
    pub message: String,
    pub offset: Option<usize>,
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.offset {
            Some(o) => write!(f, "{}: {} (byte {o})", self.code, self.message),
            None => write!(f, "{}: {}", self.code, self.message),
        }
    }
}

fn decode(e: DecodeError) -> StateError {
    StateError { code: "bad_state", message: format!("the saved game state is damaged: {e}"), offset: Some(e.offset) }
}

fn invalid(r: &ByteReader<'_>, what: String) -> StateError {
    StateError { code: "bad_state", message: format!("the saved game state doesn't fit this content: {what}"), offset: Some(r.offset()) }
}

fn w_pos(w: &mut ByteWriter, p: Pos) {
    w.i32(p.x);
    w.i32(p.y);
    w.i32(p.z);
}

/// Positions within 10 km (the slab is tens of metres): sums, differences and doublings of
/// positions and distances stay far from overflow.
const MAX_COORD: i32 = 10_000_000;
/// Hit points, distances, dust and the like within a hundred million of their unit.
const MAX_AMOUNT: i32 = 100_000_000;
/// Ticks and counters (ids, sequence numbers) the tick adds to: far below `u32::MAX` (a run of
/// 2^30 ticks is over a year at 30 Hz).
const MAX_COUNTER: u32 = 1 << 30;

/// A tick or counter in `0..=MAX_COUNTER`.
fn counter(r: &mut ByteReader<'_>, field: &str) -> Result<u32, StateError> {
    let v = r.u32(field).map_err(decode)?;
    if v > MAX_COUNTER {
        return Err(invalid(r, format!("{field} {v} is out of range")));
    }
    Ok(v)
}

fn opt_counter(r: &mut ByteReader<'_>, field: &str) -> Result<Option<u32>, StateError> {
    let v = r.opt_u32(field).map_err(decode)?;
    if v.is_some_and(|v| v > MAX_COUNTER) {
        return Err(invalid(r, format!("{field} is out of range")));
    }
    Ok(v)
}

fn r_pos(r: &mut ByteReader<'_>) -> Result<Pos, StateError> {
    let p = Pos { x: r.i32("pos").map_err(decode)?, y: r.i32("pos").map_err(decode)?, z: r.i32("pos").map_err(decode)? };
    if [p.x, p.y, p.z].iter().any(|c| c.unsigned_abs() > MAX_COORD as u32) {
        return Err(invalid(r, format!("position {p:?} is out of range")));
    }
    Ok(p)
}

/// An amount in `lo..=MAX_AMOUNT`.
fn amount(r: &mut ByteReader<'_>, field: &str, lo: i32) -> Result<i32, StateError> {
    let v = r.i32(field).map_err(decode)?;
    if !(lo..=MAX_AMOUNT).contains(&v) {
        return Err(invalid(r, format!("{field} {v} is out of range")));
    }
    Ok(v)
}

fn w_rng(w: &mut ByteWriter, rng: &Rng) {
    let (s, i) = rng.to_parts();
    w.u64(s);
    w.u64(i);
}

fn r_rng(r: &mut ByteReader<'_>) -> Result<Rng, StateError> {
    Ok(Rng::from_parts(r.u64("rng").map_err(decode)?, r.u64("rng").map_err(decode)?))
}

fn w_enemy_el(w: &mut ByteWriter, x: &EnemyEl) {
    for v in [x.burn, x.chill, x.shatter, x.shove, x.shove_rate] {
        w.i32(v);
    }
    for v in [x.burn_until, x.frozen_until, x.wet_until, x.stagger_until, x.fuse_at] {
        w.u32(v);
    }
    w.u16(x.shoves);
}

fn r_enemy_el(r: &mut ByteReader<'_>) -> Result<EnemyEl, StateError> {
    let el = EnemyEl {
        burn: amount(r, "burn", 0)?,
        chill: amount(r, "chill", 0)?,
        shatter: amount(r, "shatter", 0)?,
        shove: amount(r, "shove", 0)?,
        shove_rate: amount(r, "shove_rate", 0)?,
        burn_until: counter(r, "burn_until")?,
        frozen_until: counter(r, "frozen_until")?,
        wet_until: counter(r, "wet_until")?,
        stagger_until: counter(r, "stagger_until")?,
        fuse_at: counter(r, "fuse_at")?,
        shoves: r.u16("shoves").map_err(decode)?,
    };
    // The rules set a burn and its end together and clear them together (catching, burning out,
    // put out), so a save holding one without the other was not written by them.
    if (el.burn == 0) != (el.burn_until == 0) {
        return Err(invalid(r, format!("burn {} with burn_until {}: the two are set together", el.burn, el.burn_until)));
    }
    Ok(el)
}

fn w_elements(w: &mut ByteWriter, w0: &World) {
    w.len_u32(w0.rubble.len());
    for (c, &(raised, until)) in &w0.rubble {
        w.u16(c.0);
        w.u32(raised);
        w.u32(until);
    }
    w.len_u32(w0.spent.len());
    for (c, bar) in &w0.spent {
        w.u16(c.0);
        w.u32(*bar);
    }
    let b = &w0.blaze;
    w.len_u32(b.cells.len());
    for c in &b.cells {
        w.i32(c.heat);
        w.u32(c.left);
        w.u8(c.phase as u8);
        w.u32(c.since);
        w.u32(c.covered_until);
    }
    w.len_u32(b.zones.len());
    for z in &b.zones {
        w_pos(w, z.at);
        w.i32(z.radius);
        w.i32(z.dir.0);
        w.i32(z.dir.1);
        w.u32(z.until);
    }
}

/// What the elements' part of a save holds besides the enemies' own: rubble, spent cells, fire.
type ElementsPart = (BTreeMap<CellId, (u32, u32)>, BTreeMap<CellId, EnemyId>, Blaze);

fn r_elements(r: &mut ByteReader<'_>, content: &Content, b: &Bounds, version: u32, tick: u32) -> Result<ElementsPart, StateError> {
    use crate::blaze::{FireCell, Phase, Zone, MAX_ZONES};
    let d = decode;
    let v2 = version == STATE_VERSION_2;
    let n = r.count("rubble", b.cells, if v2 { 6 } else { 10 }).map_err(d)?;
    let mut rubble = BTreeMap::new();
    for _ in 0..n {
        let cell = b.cell(r)?;
        // Version 2 kept no rising tick: its life is counted from the load.
        let raised = if v2 { tick } else { counter(r, "rubble.raised")? };
        let until = counter(r, "rubble.until")?;
        if raised > tick {
            return Err(invalid(r, format!("rubble on cell {} rose at {raised}, after the save's tick {tick}", cell.0)));
        }
        rubble.insert(cell, (raised, until));
    }
    let mut spent = BTreeMap::new();
    if !v2 {
        let n = r.count("spent", b.cells, 6).map_err(d)?;
        for _ in 0..n {
            let cell = b.cell(r)?;
            spent.insert(cell, counter(r, "spent.bar")?);
        }
    }
    let mut blaze = Blaze::new(content);
    let n = r.count("fire cells", MAX_ITEMS, 17).map_err(d)?;
    if n != blaze.cells.len() {
        return Err(invalid(r, format!("{n} fire cells for content with {} fuel cells", blaze.cells.len())));
    }
    for c in blaze.cells.iter_mut() {
        let heat = amount(r, "heat", 0)?;
        let left = counter(r, "fuel left")?;
        let phase = match r.u8("phase").map_err(d)? {
            0 => Phase::Fuel,
            1 => Phase::Burning,
            2 => Phase::Burnt,
            v => return Err(invalid(r, format!("fire phase {v}"))),
        };
        *c = FireCell { heat, left, phase, since: counter(r, "since")?, covered_until: counter(r, "covered_until")? };
    }
    let n = r.count("zones", MAX_ZONES, 28).map_err(d)?;
    for _ in 0..n {
        let at = r_pos(r)?;
        let radius = amount(r, "zone.radius", 0)?;
        let (dx, dz) = (amount(r, "zone.dir", -1000)?, amount(r, "zone.dir", -1000)?);
        if dx > 1000 || dz > 1000 {
            return Err(invalid(r, "a zone's direction is out of range".into()));
        }
        blaze.zones.push(Zone { at, radius, dir: (dx, dz), until: counter(r, "zone.until")? });
    }
    Ok((rubble, spent, blaze))
}

/// Bounds checks against the content.
struct Bounds {
    cells: usize,
    enemies: usize,
    stones: Vec<usize>,
    waves: usize,
    ring: usize,
    entries: usize,
}

impl Bounds {
    fn of(c: &Content) -> Bounds {
        Bounds {
            cells: c.map.cells().len(),
            enemies: c.enemies.len(),
            stones: c.stones.iter().map(|s| s.tiers.len()).collect(),
            waves: c.waves.len(),
            ring: c.map.air.ring.len(),
            entries: c.map.air.entries.len(),
        }
    }

    fn cell(&self, r: &mut ByteReader<'_>) -> Result<CellId, StateError> {
        let c = r.u16("cell").map_err(decode)?;
        if usize::from(c) >= self.cells {
            return Err(invalid(r, format!("cell {c} (the map has {})", self.cells)));
        }
        Ok(CellId(c))
    }

    fn enemy_kind(&self, r: &mut ByteReader<'_>) -> Result<EnemyKind, StateError> {
        let k = r.u8("enemy kind").map_err(decode)?;
        if usize::from(k) >= self.enemies {
            return Err(invalid(r, format!("enemy kind {k} (the content has {})", self.enemies)));
        }
        Ok(EnemyKind(k))
    }

    fn stone(&self, r: &mut ByteReader<'_>) -> Result<(StoneKind, u8), StateError> {
        let (k, tier) = (r.u8("stone kind").map_err(decode)?, r.u8("tier").map_err(decode)?);
        match self.stones.get(usize::from(k)) {
            Some(&tiers) if usize::from(tier) < tiers => Ok((StoneKind(k), tier)),
            Some(&tiers) => Err(invalid(r, format!("tier {tier} of stone {k} (it has {tiers})"))),
            None => Err(invalid(r, format!("stone kind {k} (the content has {})", self.stones.len()))),
        }
    }

    /// An enemy's or a pending spawn's wave: 1-based (wave 1 is the first), so `1..=waves`.
    fn wave(&self, r: &mut ByteReader<'_>) -> Result<u16, StateError> {
        let w = r.u16("wave").map_err(decode)?;
        if w == 0 || usize::from(w) > self.waves {
            return Err(invalid(r, format!("wave {w} (waves are 1..={})", self.waves)));
        }
        Ok(w)
    }
}

impl State {
    /// The whole simulation state as bytes (see the module docs). The same state always gives
    /// the same bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let w0 = &self.w;
        let mut w = ByteWriter::with_capacity(512 + w0.enemies.len() * 80);
        let el = self.content.elemental();
        w.u32(if el { STATE_VERSION } else { STATE_VERSION_PLAIN });
        w.u64(self.content.hash);
        w.u64(w0.seed);
        w.u32(w0.tick);
        w.u16(w0.wave);
        w.opt_u32(w0.next_wave_tick);
        w.i32(w0.dust);
        w.i32(w0.tombstones);
        w.u8(match w0.outcome {
            Outcome::Running => 0,
            Outcome::Won => 1,
            Outcome::Lost => 2,
        });
        for rng in [&w0.rng_waves, &w0.rng_jump, &w0.rng_drops] {
            w_rng(&mut w, rng);
        }
        w.u32(w0.next_enemy);
        w.u32(w0.next_projectile);
        w.u32(w0.next_seq);
        w.len_u32(w0.pending.len());
        for p in &w0.pending {
            w.u32(p.tick);
            w.u32(p.seq);
            w.u8(p.enemy.0);
            w.u16(p.wave);
            match p.at {
                SpawnAt::Ground(c) => {
                    w.u8(0);
                    w.u16(c.0);
                }
                SpawnAt::Air { entry, dir, hops } => {
                    w.u8(1);
                    w.u16(entry);
                    w.i8(dir);
                    w.u16(hops);
                }
            }
        }
        w.len_u32(w0.enemies.len());
        for e in &w0.enemies {
            w.u32(e.id);
            w.u8(e.kind.0);
            w.u16(e.wave);
            w.i32(e.hp);
            w.i32(e.max_hp);
            w_pos(&mut w, e.pos);
            match e.motion {
                Motion::Ground { from, to, progress, len, leap } => {
                    w.u8(0);
                    w.u16(from.0);
                    w.u16(to.0);
                    w.i32(progress);
                    w.i32(len);
                    w.bool(leap);
                }
                Motion::Air { from, to, progress, len, ring, dir, hops_left, diving } => {
                    w.u8(1);
                    w_pos(&mut w, from);
                    w_pos(&mut w, to);
                    w.i32(progress);
                    w.i32(len);
                    w.u16(ring);
                    w.i8(dir);
                    w.u16(hops_left);
                    w.bool(diving);
                }
            }
            w.i32(e.slow);
            w.u32(e.slow_until);
            w.bool(e.attacking.is_some());
            if let Some(c) = e.attacking {
                w.u16(c.0);
            }
            w.i32(e.remaining);
            w.bool(e.alive);
            w.i64(e.carry);
            w.u32(e.next_summon);
            w.u32(e.stuck);
            if el {
                w_enemy_el(&mut w, &e.el);
            }
        }
        w.len_u32(w0.stones.len());
        for (c, s) in &w0.stones {
            w.u16(c.0);
            w.u8(s.kind.0);
            w.u8(s.tier);
            w.i32(s.invested);
            w.u32(s.ready_at);
        }
        w.len_u32(w0.walls.len());
        for (c, x) in &w0.walls {
            w.u16(c.0);
            w.i32(x.hp);
            w.i32(x.max_hp);
            w.i32(x.cost);
        }
        w.len_u32(w0.projectiles.len());
        for p in &w0.projectiles {
            w.u32(p.id);
            w.u8(p.stone.0);
            w.u8(p.tier);
            w_pos(&mut w, p.origin);
            w_pos(&mut w, p.pos);
            w_pos(&mut w, p.aim);
            w.opt_u32(p.target);
            w.bool(p.homing);
            w.bool(p.done);
        }
        w.len_u32(w0.wave_stats.len());
        for s in &w0.wave_stats {
            w.opt_u32(s.started_at);
            w.opt_u32(s.cleared_at);
            for v in [s.spawned, s.killed, s.leaked] {
                w.u32(v);
            }
            w.i32(s.tombstones_lost);
            w.u32(s.stranded);
            w.i32(s.dust_from_kills);
            w.u32(s.pending);
            w.u32(s.alive);
            if el {
                w.u32(s.detonated);
            }
        }
        if el {
            w_elements(&mut w, w0);
        }
        w.into_bytes()
    }

    /// Reads [`State::to_bytes`]'s bytes for `content` (which must be the content the state was
    /// saved with). Derived state is rebuilt, so the result steps exactly like the saved state.
    pub fn from_bytes(content: Arc<Content>, bytes: &[u8]) -> Result<State, StateError> {
        let mut r = ByteReader::new(bytes);
        let version = r.u32("version").map_err(decode)?;
        if ![STATE_VERSION, STATE_VERSION_2, STATE_VERSION_PLAIN].contains(&version) {
            return Err(StateError {
                code: "unsupported_version",
                message: format!("game state version {version}; this build reads {STATE_VERSION_PLAIN}, {STATE_VERSION_2} and {STATE_VERSION}"),
                offset: Some(0),
            });
        }
        let hash = r.u64("content hash").map_err(decode)?;
        if hash != content.hash {
            // A save from before the map's props (the same content otherwise) says so: its
            // stones may stand where a statue now does, so it isn't migrated.
            let before_props = hash == crate::content::content_hash_without_props(&content);
            let why = if before_props { " (it was made before this map's props, which block building on some cells; start a new game)" } else { "" };
            return Err(StateError {
                code: "content_changed",
                message: format!(
                    "the save was made with other content (hash {hash:016x}, now {:016x}){why}: the same state under other rules is another game, so it isn't loaded",
                    content.hash
                ),
                offset: Some(4),
            });
        }
        // Versions 2 and 3 are the layouts with the elements, written exactly for content that has them.
        let el = version != STATE_VERSION_PLAIN;
        if el != content.elemental() {
            return Err(StateError {
                code: "bad_state",
                message: format!("game state version {version} does not fit this content (elements {})", content.elemental()),
                offset: Some(0),
            });
        }
        let b = Bounds::of(&content);
        let d = decode;
        let seed = r.u64("seed").map_err(d)?;
        let tick = counter(&mut r, "tick")?;
        let wave = r.u16("wave").map_err(d)?;
        if usize::from(wave) > b.waves {
            return Err(invalid(&r, format!("wave {wave} (the content has {})", b.waves)));
        }
        let next_wave_tick = opt_counter(&mut r, "next_wave_tick")?;
        let dust = amount(&mut r, "dust", -MAX_AMOUNT)?;
        let tombstones = amount(&mut r, "tombstones", -MAX_AMOUNT)?;
        let outcome = match r.u8("outcome").map_err(d)? {
            0 => Outcome::Running,
            1 => Outcome::Won,
            2 => Outcome::Lost,
            v => return Err(invalid(&r, format!("outcome {v}"))),
        };
        let (rng_waves, rng_jump, rng_drops) = (r_rng(&mut r)?, r_rng(&mut r)?, r_rng(&mut r)?);
        let next_enemy = counter(&mut r, "next_enemy")?;
        let next_projectile = counter(&mut r, "next_projectile")?;
        let next_seq = counter(&mut r, "next_seq")?;
        let n = r.count("pending", MAX_ITEMS, 12).map_err(d)?;
        let mut pending = Vec::with_capacity(n);
        for _ in 0..n {
            let tick = counter(&mut r, "pending.tick")?;
            let seq = counter(&mut r, "pending.seq")?;
            let enemy = b.enemy_kind(&mut r)?;
            let wave = b.wave(&mut r)?;
            let at = match r.u8("pending.at").map_err(d)? {
                0 => SpawnAt::Ground(b.cell(&mut r)?),
                1 => {
                    let entry = r.u16("entry").map_err(d)?;
                    if usize::from(entry) >= b.entries {
                        return Err(invalid(&r, format!("air entry {entry} (the map has {})", b.entries)));
                    }
                    SpawnAt::Air { entry, dir: r.i8("dir").map_err(d)?, hops: r.u16("hops").map_err(d)? }
                }
                v => return Err(invalid(&r, format!("spawn kind {v}"))),
            };
            pending.push(Pending { tick, seq, enemy, wave, at });
        }
        let n = r.count("enemies", MAX_ITEMS, 60).map_err(d)?;
        let mut enemies = Vec::with_capacity(n);
        for _ in 0..n {
            let id = counter(&mut r, "enemy.id")?;
            let kind = b.enemy_kind(&mut r)?;
            let wave = b.wave(&mut r)?;
            let hp = amount(&mut r, "hp", -MAX_AMOUNT)?;
            let max_hp = amount(&mut r, "max_hp", 0)?;
            let pos = r_pos(&mut r)?;
            let motion = match r.u8("motion").map_err(d)? {
                0 => Motion::Ground {
                    from: b.cell(&mut r)?,
                    to: b.cell(&mut r)?,
                    progress: amount(&mut r, "progress", -MAX_AMOUNT)?,
                    len: amount(&mut r, "len", 0)?,
                    leap: r.bool("leap").map_err(d)?,
                },
                1 => {
                    let (from, to) = (r_pos(&mut r)?, r_pos(&mut r)?);
                    let (progress, len) = (amount(&mut r, "progress", -MAX_AMOUNT)?, amount(&mut r, "len", 0)?);
                    let ring = r.u16("ring").map_err(d)?;
                    if usize::from(ring) >= b.ring.max(1) {
                        return Err(invalid(&r, format!("ring position {ring} (the ring has {})", b.ring)));
                    }
                    Motion::Air { from, to, progress, len, ring, dir: r.i8("dir").map_err(d)?, hops_left: r.u16("hops").map_err(d)?, diving: r.bool("diving").map_err(d)? }
                }
                v => return Err(invalid(&r, format!("motion kind {v}"))),
            };
            let slow = amount(&mut r, "slow", -MAX_AMOUNT)?;
            let slow_until = counter(&mut r, "slow_until")?;
            let attacking = if r.bool("attacking").map_err(d)? { Some(b.cell(&mut r)?) } else { None };
            let remaining = amount(&mut r, "remaining", -MAX_AMOUNT)?;
            let alive = r.bool("alive").map_err(d)?;
            let carry = r.i64("carry").map_err(d)?;
            // The movement remainder is always under one tick's worth (see `World::budget`).
            if carry.unsigned_abs() >= u64::from(TICK_HZ) * 1000 {
                return Err(invalid(&r, format!("movement remainder {carry} is out of range")));
            }
            let next_summon = counter(&mut r, "next_summon")?;
            let stuck = counter(&mut r, "stuck")?;
            let enemy_el = if el { r_enemy_el(&mut r)? } else { EnemyEl::default() };
            if enemies.last().is_some_and(|e: &Enemy| e.id >= id) {
                return Err(invalid(&r, "enemy ids must increase".into()));
            }
            enemies.push(Enemy { id, kind, wave, hp, max_hp, pos, motion, slow, slow_until, attacking, remaining, alive, carry, next_summon, stuck, el: enemy_el });
        }
        let n = r.count("stones", b.cells, 12).map_err(d)?;
        let mut stones = BTreeMap::new();
        for _ in 0..n {
            let cell = b.cell(&mut r)?;
            let (kind, tier) = b.stone(&mut r)?;
            let invested = amount(&mut r, "invested", -MAX_AMOUNT)?;
            let ready_at = counter(&mut r, "ready_at")?;
            stones.insert(cell, Stone { kind, tier, invested, ready_at });
        }
        let n = r.count("walls", b.cells, 14).map_err(d)?;
        let mut walls = BTreeMap::new();
        for _ in 0..n {
            let cell = b.cell(&mut r)?;
            walls.insert(cell, Wall { hp: amount(&mut r, "wall.hp", -MAX_AMOUNT)?, max_hp: amount(&mut r, "wall.max_hp", 0)?, cost: amount(&mut r, "wall.cost", -MAX_AMOUNT)? });
        }
        let n = r.count("projectiles", MAX_ITEMS, 44).map_err(d)?;
        let mut projectiles = Vec::with_capacity(n);
        for _ in 0..n {
            let id = counter(&mut r, "projectile.id")?;
            let (stone, tier) = b.stone(&mut r)?;
            let (origin, pos, aim) = (r_pos(&mut r)?, r_pos(&mut r)?, r_pos(&mut r)?);
            let target = r.opt_u32("target").map_err(d)?;
            let homing = r.bool("homing").map_err(d)?;
            let done = r.bool("done").map_err(d)?;
            projectiles.push(Projectile { id, stone, tier, origin, pos, aim, target, homing, done });
        }
        let n = r.count("wave_stats", b.waves, 30).map_err(d)?;
        if n != b.waves {
            return Err(invalid(&r, format!("{n} waves of statistics for content with {} waves", b.waves)));
        }
        let mut wave_stats = Vec::with_capacity(n);
        for _ in 0..n {
            wave_stats.push(WaveStats {
                started_at: opt_counter(&mut r, "started_at")?,
                cleared_at: opt_counter(&mut r, "cleared_at")?,
                spawned: counter(&mut r, "spawned")?,
                killed: counter(&mut r, "killed")?,
                leaked: counter(&mut r, "leaked")?,
                tombstones_lost: amount(&mut r, "tombstones_lost", -MAX_AMOUNT)?,
                stranded: counter(&mut r, "stranded")?,
                dust_from_kills: amount(&mut r, "dust_from_kills", -MAX_AMOUNT)?,
                pending: r.u32("pending").map_err(d)?,
                alive: r.u32("alive").map_err(d)?,
                detonated: if el { counter(&mut r, "detonated")? } else { 0 },
            });
        }
        let (rubble, spent, blaze) = if el { r_elements(&mut r, &content, &b, version, tick)? } else { (BTreeMap::new(), BTreeMap::new(), Blaze::new(&content)) };
        r.finish().map_err(d)?;
        // Cross-checks: ids below the next ids, and every counter the tick counts down agrees with
        // the lists it counts (the tick subtracts from them).
        if enemies.iter().any(|e| e.id >= next_enemy) || projectiles.iter().any(|p| p.id >= next_projectile) || pending.iter().any(|p| p.seq >= next_seq) {
            return Err(invalid(&r, "an id is not below the next id".into()));
        }
        // A spent cell is barred against the dead living when its rubble went, so its bar is at
        // most the next id: one above it would bar the cell against enemies not yet born.
        if let Some((cell, bar)) = spent.iter().find(|&(_, &bar)| bar > next_enemy) {
            return Err(invalid(&r, format!("spent cell {} is barred against enemy {bar}, which is not born yet (next id {next_enemy})", cell.0)));
        }
        if usize::from(wave) > b.waves {
            return Err(invalid(&r, format!("wave {wave} (the content has {})", b.waves)));
        }
        for (i, s) in wave_stats.iter().enumerate() {
            let w = (i + 1) as u16;
            let pending_n = pending.iter().filter(|p| p.wave == w).count() as u32;
            let alive_n = enemies.iter().filter(|e| e.alive && e.wave == w).count() as u32;
            if s.pending != pending_n || s.alive != alive_n {
                return Err(invalid(&r, format!("wave {w}'s counts (pending {}, alive {}) don't match its spawns and enemies ({pending_n}, {alive_n})", s.pending, s.alive)));
            }
        }
        // Derived state, as the tick keeps it.
        let cells = b.cells;
        let mut blocked = vec![false; cells];
        for c in walls.keys() {
            blocked[c.index()] = true;
        }
        let fields = Fields::build(&content.map, &blocked, content.rules.wall.breach_penalty);
        let mut w = World {
            seed,
            tick,
            wave,
            next_wave_tick,
            dust,
            tombstones,
            outcome,
            rng_waves,
            rng_jump,
            rng_drops,
            next_enemy,
            next_projectile,
            next_seq,
            pending,
            enemies,
            stones,
            walls,
            projectiles,
            wave_stats,
            blaze,
            rubble,
            spent,
            bursts: Vec::new(),
            blocked,
            fields,
            rooted: vec![false; cells],
            #[cfg(test)]
            no_furor: false,
        };
        w.refresh_roots(&content);
        Ok(State { content, w })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{Player, Script};

    fn content() -> Arc<Content> {
        Arc::new(Content::builtin())
    }

    /// A run with stones, walls, enemies and projectiles in flight.
    fn played(content: &Arc<Content>, seed: u64, ticks: u32) -> State {
        let mut s = State::new(Arc::clone(content), seed);
        let layout = Script::parse(include_str!("../data/layouts/centre.json"), content, "centre.json").expect("layout");
        let mut p = Player::new(layout);
        let mut ev = Vec::new();
        for _ in 0..ticks {
            let _ = p.drive(&mut s, &mut ev);
            s.step_into(&[], &mut ev);
            ev.clear();
        }
        s
    }

    #[test]
    fn a_saved_state_steps_exactly_like_the_original() {
        let c = content();
        for (seed, at) in [(7u64, 1200u32), (11, 4000), (3, 9000)] {
            let mut a = played(&c, seed, at);
            let bytes = a.to_bytes();
            let mut b = State::from_bytes(Arc::clone(&c), &bytes).unwrap();
            assert_eq!(a.hash(), b.hash(), "seed {seed}: the state reads back whole");
            assert_eq!(b.to_bytes(), bytes, "bytes are canonical");
            let mut ev = Vec::new();
            for t in 0..3000 {
                a.step_into(&[], &mut ev);
                b.step_into(&[], &mut ev);
                assert_eq!(a.hash(), b.hash(), "seed {seed}: diverged {t} ticks after the load");
            }
        }
    }

    /// Mutates `bytes` `rounds` times (bit flips, and whole 1/2/4/8-byte fields set to 0, -1,
    /// MIN, MAX and the like), loads each, and steps whatever loads `ticks` ticks.
    fn fuzz(c: &Arc<Content>, bytes: &[u8], mut seed: u64, rounds: usize, ticks: u32) {
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let extremes: [u64; 8] = [0, u64::MAX, 1, 0x7fff_ffff, 0x8000_0000, 0xffff_ffff, 0x8000_0000_0000_0000, 0x7fff_ffff_ffff_ffff];
        let mut loaded = 0;
        // SPECULA_FUZZ_ROUNDS raises the rounds for a long local run.
        let rounds = std::env::var("SPECULA_FUZZ_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(rounds);
        for _ in 0..rounds {
            let mut x = bytes.to_vec();
            for _ in 0..1 + next() % 3 {
                let i = 12 + (next() as usize) % (x.len() - 12);
                if next() % 2 == 0 {
                    x[i] ^= (next() >> 40) as u8 | 1;
                } else {
                    let width = [1usize, 2, 4, 8][(next() % 4) as usize];
                    let v = extremes[(next() % 8) as usize].to_le_bytes();
                    let end = (i + width).min(x.len());
                    x[i..end].copy_from_slice(&v[..end - i]);
                }
            }
            if let Ok(mut s) = State::from_bytes(Arc::clone(c), &x) {
                loaded += 1;
                let mut ev = Vec::new();
                for _ in 0..ticks {
                    s.step_into(&[], &mut ev);
                    ev.clear();
                }
            }
        }
        assert!(loaded > 0, "some damage is harmless (a dust amount, a position): the fuzzer must reach the tick");
    }

    /// Saves in the last wave and on the tick before victory load and step on identically (wave
    /// numbers are 1-based: the last wave is `waves`, which a load once refused).
    #[test]
    fn saves_in_the_last_wave_and_before_victory_load() {
        let c = content();
        let mut s = State::new(Arc::clone(&c), 7);
        let layout = Script::parse(include_str!("../data/layouts/centre.json"), &c, "centre.json").expect("layout");
        let mut p = Player::new(layout);
        let mut ev = Vec::new();
        let mut last_wave = None;
        while s.outcome() == Outcome::Running {
            let before = s.clone();
            let _ = p.drive(&mut s, &mut ev);
            s.step_into(&[], &mut ev);
            ev.clear();
            if s.wave() == s.waves_total() && last_wave.is_none() {
                last_wave = Some(s.clone());
            }
            if s.outcome() == Outcome::Won {
                // The tick before victory.
                let bytes = before.to_bytes();
                let mut b = State::from_bytes(Arc::clone(&c), &bytes).expect("a save right before victory loads");
                let mut a = before;
                a.step_into(&[], &mut ev);
                b.step_into(&[], &mut ev);
                assert_eq!(a.hash(), b.hash());
                assert_eq!(b.outcome(), Outcome::Won);
            }
        }
        assert_eq!(s.outcome(), Outcome::Won, "the centre layout wins seed 7");
        let mut a = last_wave.expect("the last wave started");
        let mut b = State::from_bytes(Arc::clone(&c), &a.to_bytes()).expect("a save in the last wave loads");
        for _ in 0..3000 {
            a.step_into(&[], &mut ev);
            b.step_into(&[], &mut ev);
            assert_eq!(a.hash(), b.hash());
        }
    }

    /// A save whose per-wave counters disagree with its lists is refused (the tick counts them
    /// down: a zeroed pending count once overflowed).
    #[test]
    fn counters_that_disagree_with_their_lists_are_refused() {
        let c = content();
        let mut s = played(&c, 7, 1);
        let layout = Script::parse(include_str!("../data/layouts/centre.json"), &c, "centre.json").expect("layout");
        let mut p = Player::new(layout);
        let mut ev = Vec::new();
        while !(s.wave() == 2 && s.pending_spawns() > 2) {
            let _ = p.drive(&mut s, &mut ev);
            s.step_into(&[], &mut ev);
            ev.clear();
        }
        let mut bad = s.clone();
        bad.w.wave_stats[1].pending = 0;
        let e = State::from_bytes(Arc::clone(&c), &bad.to_bytes()).unwrap_err();
        assert!(e.message.contains("counts"), "{e}");
        let mut bad = s.clone();
        bad.w.wave_stats[1].alive += 1;
        assert!(State::from_bytes(Arc::clone(&c), &bad.to_bytes()).is_err());
        let mut bad = s.clone();
        bad.w.enemies[0].carry = i64::MIN;
        assert!(State::from_bytes(Arc::clone(&c), &bad.to_bytes()).is_err(), "the minimum carry is out of range, not an overflow");
        let mut bad = s.clone();
        bad.w.pending[0].wave = 0;
        assert!(State::from_bytes(Arc::clone(&c), &bad.to_bytes()).is_err(), "wave 0 doesn't exist");
        let mut bad = s;
        bad.w.enemies[0].pos.x = i32::MIN;
        assert!(State::from_bytes(Arc::clone(&c), &bad.to_bytes()).is_err());
    }

    #[test]
    fn damaged_or_foreign_state_is_an_error_never_a_panic() {
        let c = content();
        let a = played(&c, 5, 3000);
        let bytes = a.to_bytes();
        for cut in 0..bytes.len() {
            assert!(State::from_bytes(Arc::clone(&c), &bytes[..cut]).is_err(), "cut {cut}");
        }
        // Random damage: errors, or a state that steps on without panicking, for a long while
        // (counters run down, waves end and start, the run can be won or lost).
        fuzz(&c, &bytes, 0xdead_beef, 1500, 400);
        // A state with enemies alive and spawns pending in several waves, fuzzed hard on whole
        // fields (extreme values where the tick adds, subtracts or indexes).
        let busy = played(&c, 7, 2400);
        fuzz(&c, &busy.to_bytes(), 0x5eed_0002, 1200, 3000);
        // Other content.
        let mut other = Content::builtin();
        other.hash ^= 1;
        let e = State::from_bytes(Arc::new(other), &bytes).unwrap_err();
        assert_eq!(e.code, "content_changed");
    }
}
