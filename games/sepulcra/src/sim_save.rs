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
pub const STATE_VERSION: u32 = 1;
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

fn r_pos(r: &mut ByteReader<'_>) -> Result<Pos, StateError> {
    let p = Pos { x: r.i32("pos").map_err(decode)?, y: r.i32("pos").map_err(decode)?, z: r.i32("pos").map_err(decode)? };
    if [p.x, p.y, p.z].iter().any(|c| c.abs() > MAX_COORD) {
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

    fn wave(&self, r: &mut ByteReader<'_>) -> Result<u16, StateError> {
        let w = r.u16("wave").map_err(decode)?;
        if usize::from(w) >= self.waves.max(1) {
            return Err(invalid(r, format!("wave {w} (the content has {})", self.waves)));
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
        w.u32(STATE_VERSION);
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
        }
        w.into_bytes()
    }

    /// Reads [`State::to_bytes`]'s bytes for `content` (which must be the content the state was
    /// saved with). Derived state is rebuilt, so the result steps exactly like the saved state.
    pub fn from_bytes(content: Arc<Content>, bytes: &[u8]) -> Result<State, StateError> {
        let mut r = ByteReader::new(bytes);
        let version = r.u32("version").map_err(decode)?;
        if version != STATE_VERSION {
            return Err(StateError {
                code: "unsupported_version",
                message: format!("game state version {version}; this build reads {STATE_VERSION}"),
                offset: Some(0),
            });
        }
        let hash = r.u64("content hash").map_err(decode)?;
        if hash != content.hash {
            return Err(StateError {
                code: "content_changed",
                message: format!(
                    "the save was made with other content (hash {hash:016x}, now {:016x}): the same state under other rules is another game, so it isn't loaded",
                    content.hash
                ),
                offset: Some(4),
            });
        }
        let b = Bounds::of(&content);
        let d = decode;
        let seed = r.u64("seed").map_err(d)?;
        let tick = r.u32("tick").map_err(d)?;
        let wave = r.u16("wave").map_err(d)?;
        if usize::from(wave) > b.waves {
            return Err(invalid(&r, format!("wave {wave} (the content has {})", b.waves)));
        }
        let next_wave_tick = r.opt_u32("next_wave_tick").map_err(d)?;
        let dust = amount(&mut r, "dust", -MAX_AMOUNT)?;
        let tombstones = amount(&mut r, "tombstones", -MAX_AMOUNT)?;
        let outcome = match r.u8("outcome").map_err(d)? {
            0 => Outcome::Running,
            1 => Outcome::Won,
            2 => Outcome::Lost,
            v => return Err(invalid(&r, format!("outcome {v}"))),
        };
        let (rng_waves, rng_jump, rng_drops) = (r_rng(&mut r)?, r_rng(&mut r)?, r_rng(&mut r)?);
        let next_enemy = r.u32("next_enemy").map_err(d)?;
        let next_projectile = r.u32("next_projectile").map_err(d)?;
        let next_seq = r.u32("next_seq").map_err(d)?;
        let n = r.count("pending", MAX_ITEMS, 12).map_err(d)?;
        let mut pending = Vec::with_capacity(n);
        for _ in 0..n {
            let tick = r.u32("pending.tick").map_err(d)?;
            let seq = r.u32("pending.seq").map_err(d)?;
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
            let id = r.u32("enemy.id").map_err(d)?;
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
            let slow_until = r.u32("slow_until").map_err(d)?;
            let attacking = if r.bool("attacking").map_err(d)? { Some(b.cell(&mut r)?) } else { None };
            let remaining = amount(&mut r, "remaining", -MAX_AMOUNT)?;
            let alive = r.bool("alive").map_err(d)?;
            let carry = r.i64("carry").map_err(d)?;
            // The movement remainder is always under one tick's worth (see `World::budget`).
            if carry.abs() >= i64::from(TICK_HZ) * 1000 {
                return Err(invalid(&r, format!("movement remainder {carry} is out of range")));
            }
            let next_summon = r.u32("next_summon").map_err(d)?;
            let stuck = r.u32("stuck").map_err(d)?;
            if enemies.last().is_some_and(|e: &Enemy| e.id >= id) {
                return Err(invalid(&r, "enemy ids must increase".into()));
            }
            enemies.push(Enemy { id, kind, wave, hp, max_hp, pos, motion, slow, slow_until, attacking, remaining, alive, carry, next_summon, stuck });
        }
        let n = r.count("stones", b.cells, 12).map_err(d)?;
        let mut stones = BTreeMap::new();
        for _ in 0..n {
            let cell = b.cell(&mut r)?;
            let (kind, tier) = b.stone(&mut r)?;
            let invested = amount(&mut r, "invested", -MAX_AMOUNT)?;
            let ready_at = r.u32("ready_at").map_err(d)?;
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
            let id = r.u32("projectile.id").map_err(d)?;
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
                started_at: r.opt_u32("started_at").map_err(d)?,
                cleared_at: r.opt_u32("cleared_at").map_err(d)?,
                spawned: r.u32("spawned").map_err(d)?,
                killed: r.u32("killed").map_err(d)?,
                leaked: r.u32("leaked").map_err(d)?,
                tombstones_lost: r.i32("tombstones_lost").map_err(d)?,
                stranded: r.u32("stranded").map_err(d)?,
                dust_from_kills: r.i32("dust_from_kills").map_err(d)?,
                pending: r.u32("pending").map_err(d)?,
                alive: r.u32("alive").map_err(d)?,
            });
        }
        r.finish().map_err(d)?;
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
            blocked,
            fields,
            rooted: vec![false; cells],
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

    #[test]
    fn damaged_or_foreign_state_is_an_error_never_a_panic() {
        let c = content();
        let a = played(&c, 5, 3000);
        let bytes = a.to_bytes();
        for cut in 0..bytes.len() {
            assert!(State::from_bytes(Arc::clone(&c), &bytes[..cut]).is_err(), "cut {cut}");
        }
        // Random damage: errors, or a state that steps without panicking.
        let mut seed = 0xdead_beef_u64;
        for _ in 0..1500 {
            let mut x = bytes.clone();
            for _ in 0..2 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let i = 12 + (seed as usize) % (x.len() - 12);
                x[i] ^= (seed >> 40) as u8 | 1;
            }
            if let Ok(mut s) = State::from_bytes(Arc::clone(&c), &x) {
                let mut ev = Vec::new();
                for _ in 0..30 {
                    s.step_into(&[], &mut ev);
                }
            }
        }
        // Other content.
        let mut other = Content::builtin();
        other.hash ^= 1;
        let e = State::from_bytes(Arc::new(other), &bytes).unwrap_err();
        assert_eq!(e.code, "content_changed");
    }
}
