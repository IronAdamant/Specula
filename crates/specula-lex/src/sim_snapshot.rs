//! Save games: a running rule set's state as bytes (docs/design/SAVES.md).
//!
//! [`Sim::save_state`] writes everything in [`State`] (integers as they are, in a fixed order)
//! with the compiled rules' hash; [`Sim::restore`] reads it back against the same rules, so a
//! restored run has the same [`Sim::hash`] and steps exactly like the saved one. Every index the
//! bytes hold (archetypes, effects, behaviours, machine states, paths, sites, waves, streams) is
//! checked against the rules first, so damaged or hand-made bytes are an error, never a panic in
//! a later tick. Rules that changed since the save (another hash, params included) are refused:
//! the same state under other rules is another game.
//!
//! Sepulcra's `--rules lex` mode saves its Lex run through this (in the game's section); live
//! runs (`lex_live`) aren't in save games yet (SAVES.md, "Lex").

use super::*;
use specula_core::bytes::{ByteReader, ByteWriter, DecodeError};

/// Bumped when the layout changes (2: entities' `order` and pending spawns' `with`; version 1
/// reads forward).
pub const SNAPSHOT_VERSION: u32 = 2;

/// Why saved Lex state couldn't be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotError {
    /// `bad_state`, `rules_changed`, `unsupported_version`, `native_unbound`.
    pub code: &'static str,
    pub message: String,
    pub offset: Option<usize>,
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

fn dec(e: DecodeError) -> SnapshotError {
    SnapshotError { code: "bad_state", message: format!("the saved Lex state is damaged: {e}"), offset: Some(e.offset) }
}

fn bad(r: &ByteReader<'_>, what: String) -> SnapshotError {
    SnapshotError { code: "bad_state", message: format!("the saved Lex state doesn't fit these rules: {what}"), offset: Some(r.offset()) }
}

/// Run-time diagnostic codes, so a restored count keeps its static name (counts with a code
/// this build doesn't know are dropped: diagnostics are reports, never state the run reads).
const CODES: [&str; 18] = [
    "abstract", "division_by_zero", "effect_limit", "entity_limit", "event_overflow", "native_failed", "native_unbound", "no_archetype",
    "no_attribute", "no_entity", "no_health", "no_machine", "no_more_waves", "no_path", "no_site", "no_waves", "state_limit", "work_limit",
];

fn w_p3(w: &mut ByteWriter, p: P3) {
    p.iter().for_each(|v| w.i64(*v));
}

/// Coordinates in mm: the compiler allows ±10⁹ m, so anything beyond is damage (and squares of
/// sums stay inside an `i64`'s range of what the sim computes).
const MAX_COORD_MM: i64 = 1_000_000_000_000;
/// Movement remainders stay below one tick's worth of any real speed.
const MAX_CARRY: i64 = 1_000_000_000_000;

fn r_p3(r: &mut ByteReader<'_>) -> Result<P3, SnapshotError> {
    let p = [r.i64("pos").map_err(dec)?, r.i64("pos").map_err(dec)?, r.i64("pos").map_err(dec)?];
    if p.iter().any(|v| v.unsigned_abs() > MAX_COORD_MM as u64) {
        return Err(bad(r, format!("position {p:?} is out of range")));
    }
    Ok(p)
}

fn r_carry(r: &mut ByteReader<'_>) -> Result<i64, SnapshotError> {
    let c = r.i64("carry").map_err(dec)?;
    if c.unsigned_abs() > MAX_CARRY as u64 {
        return Err(bad(r, format!("movement remainder {c} is out of range")));
    }
    Ok(c)
}

fn w_opt32(w: &mut ByteWriter, v: Option<u32>) {
    w.opt_u32(v);
}

fn w_event(w: &mut ByteWriter, e: &Event) {
    w.u16(e.kind);
    w_opt32(w, e.subject);
    w_opt32(w, e.other);
    w.i64(e.value);
}

fn r_event(r: &mut ByteReader<'_>, kinds: usize) -> Result<Event, SnapshotError> {
    let kind = r.u16("event.kind").map_err(dec)?;
    if usize::from(kind) >= kinds {
        return Err(bad(r, format!("event kind {kind} (the rules have {kinds})")));
    }
    Ok(Event { kind, subject: r.opt_u32("event.subject").map_err(dec)?, other: r.opt_u32("event.other").map_err(dec)?, value: r.i64("event.value").map_err(dec)? })
}

fn w_beh(w: &mut ByteWriter, b: &BehState) {
    match b {
        BehState::None => w.u8(0),
        BehState::Path { path, seg, progress, carry, fwd, done } => {
            w.u8(1);
            w.bool(path.is_some());
            w.u16(path.unwrap_or(0));
            w.u32(*seg);
            w.i64(*progress);
            w.i64(*carry);
            w.bool(*fwd);
            w.bool(*done);
        }
        BehState::Chase { target, carry, reached } => {
            w.u8(2);
            w_opt32(w, *target);
            w.i64(*carry);
            w.bool(*reached);
        }
        BehState::Attack { ready_at } => {
            w.u8(3);
            w.u32(*ready_at);
        }
        BehState::Projectile(p) => {
            w.u8(4);
            w_opt32(w, p.target);
            w_p3(w, p.aim);
            w.len_u32(p.payload.len());
            for (a, b) in &p.payload {
                w.i64(*a);
                w.i64(*b);
            }
            w.u16(p.attack.0);
            w.u16(p.attack.1);
            w_opt32(w, p.source);
            w.i64(p.carry);
        }
        BehState::Spawner { next } => {
            w.u8(5);
            w.u32(*next);
        }
        BehState::Lifetime { expires } => {
            w.u8(6);
            w.u32(*expires);
        }
    }
}

impl Sim {
    /// The run's whole state as bytes (see the module docs).
    pub fn save_state(&self) -> Vec<u8> {
        self.write_state(SNAPSHOT_VERSION)
    }

    /// The state in layout `version` (1 has no `order` or `with`: what earlier builds wrote,
    /// for the tests that read it forward).
    fn write_state(&self, version: u32) -> Vec<u8> {
        let st = &self.st;
        let mut w = ByteWriter::with_capacity(256 + st.ents.len() * 96);
        w.u32(version);
        w.u64(self.rules.hash);
        w.u32(st.tick);
        w.u64(st.seed);
        match &st.outcome {
            Some((s, t)) => {
                w.bool(true);
                w.str(s);
                w.u32(*t);
            }
            None => w.bool(false),
        }
        w.len_u32(st.res.len());
        st.res.iter().for_each(|v| w.i64(*v));
        w.len_u32(st.rngs.len());
        for g in &st.rngs {
            let (a, b) = g.to_parts();
            w.u64(a);
            w.u64(b);
        }
        w.u32(st.next_id);
        w.len_u32(st.ents.len());
        for e in &st.ents {
            w.u32(e.id);
            w.u16(e.arch);
            w.bool(e.alive);
            w_p3(&mut w, e.pos);
            w.len_u32(e.base.len());
            e.base.iter().for_each(|v| w.i64(*v));
            w.len_u32(e.effects.len());
            for a in &e.effects {
                w.u16(a.effect);
                w.i64(a.magnitude);
                w.u32(a.applied);
                w.u32(a.expires);
                w.u32(a.next_period);
                w.u16(a.stacks);
                w.u32(a.seq);
                w_opt32(&mut w, a.source);
                w.len_u32(a.mods.len());
                a.mods.iter().for_each(|v| w.i64(*v));
            }
            w.len_u32(e.beh.len());
            e.beh.iter().for_each(|b| w_beh(&mut w, b));
            w.bool(e.state.is_some());
            if let Some((s, t)) = e.state {
                w.u16(s);
                w.u32(t);
            }
            for v in [e.wave, e.site, e.spawn_path] {
                w.bool(v.is_some());
                w.u16(v.unwrap_or(0));
            }
            w.i64(e.path_remaining);
            if version >= 2 {
                w.i64(e.order);
            }
        }
        w.u16(st.wave);
        w_opt32(&mut w, st.next_wave_tick);
        w.len_u32(st.wave_stats.len());
        for s in &st.wave_stats {
            w_opt32(&mut w, s.started_at);
            w_opt32(&mut w, s.cleared_at);
            for v in [s.spawned, s.died, s.removed, s.pending, s.alive] {
                w.u32(v);
            }
        }
        w.len_u32(st.pending.len());
        for p in &st.pending {
            w.u32(p.tick);
            w.u32(p.seq);
            w.u16(p.arch);
            w.u16(p.wave);
            match p.at {
                SpawnAt::Path(x) => {
                    w.u8(0);
                    w.u16(x);
                }
                SpawnAt::Site(x) => {
                    w.u8(1);
                    w.u16(x);
                }
            }
            if version >= 2 {
                w.len_u32(p.with.len());
                for (a, v) in &p.with {
                    w.u16(*a);
                    w.i64(*v);
                }
            }
        }
        w.u32(st.next_seq);
        w.len_u32(st.timers.len());
        for t in &st.timers {
            w_opt32(&mut w, t.next);
            w.u32(t.fired);
        }
        w.len_u32(st.queue.len());
        st.queue.iter().for_each(|e| w_event(&mut w, e));
        w.len_u32(st.events.len());
        st.events.iter().for_each(|e| w_event(&mut w, e));
        w.len_u32(st.event_counts.len());
        st.event_counts.iter().for_each(|v| w.u64(*v));
        w.len_u32(st.diag.counts.len());
        for (k, v) in &st.diag.counts {
            w.str(k);
            w.u64(*v);
        }
        w.len_u32(st.diag.first.len());
        st.diag.first.iter().for_each(|s| w.str(s));
        w.len_u32(st.native_calls.len());
        st.native_calls.iter().for_each(|v| w.u64(*v));
        for b in [st.dirty, st.dirty_full, st.interrupted] {
            w.bool(b);
        }
        w.u32(st.effect_seq);
        for v in [st.dropped, st.work, st.command_work, st.cells] {
            w.u64(v);
        }
        w.u64(st.dispatched as u64);
        w.into_bytes()
    }

    /// A run from [`Sim::save_state`]'s bytes against `rules` (which must be the rules it was
    /// saved with) and the hooks the rules need.
    pub fn restore(rules: Arc<Rules>, natives: &Natives, bytes: &[u8]) -> Result<Sim, SnapshotError> {
        let mut sim = Sim::new(Arc::clone(&rules), 0, natives).map_err(|m| SnapshotError { code: "native_unbound", message: m, offset: None })?;
        let r0 = &*rules;
        let mut r = ByteReader::new(bytes);
        let d = dec;
        let version = r.u32("version").map_err(d)?;
        // Version 1 (before `order` and `with`) reads forward: its runs had neither, so they
        // restore with the defaults and hash as they did (the hash covers both only for rules
        // that use them). Rules that use them can't have been saved as version 1.
        if version == 0 || version > SNAPSHOT_VERSION {
            return Err(SnapshotError { code: "unsupported_version", message: format!("Lex state version {version}; this build reads 1..={SNAPSHOT_VERSION}"), offset: Some(0) });
        }
        if version < 2 && (r0.ordered || r0.with_groups) {
            return Err(SnapshotError { code: "unsupported_version", message: "Lex state version 1 can't hold `order` or `with`, which these rules use".into(), offset: Some(0) });
        }
        let hash = r.u64("rules hash").map_err(d)?;
        if hash != r0.hash {
            return Err(SnapshotError {
                code: "rules_changed",
                message: format!("the state was saved with other rules (hash {hash:016x}, now {:016x}); compile with the params it used", r0.hash),
                offset: Some(4),
            });
        }
        let exact = |r: &ByteReader<'_>, what: &str, n: usize, want: usize| if n == want { Ok(()) } else { Err(bad(r, format!("{n} {what}, the rules have {want}"))) };
        let tick = r.u32("tick").map_err(d)?;
        let seed = r.u64("seed").map_err(d)?;
        let outcome = if r.bool("outcome").map_err(d)? { Some((r.str("outcome.result", 256).map_err(d)?, r.u32("outcome.tick").map_err(d)?)) } else { None };
        let n = r.count("res", 1 << 16, 8).map_err(d)?;
        exact(&r, "resources", n, r0.resources.len())?;
        let res: Vec<i64> = (0..n).map(|_| r.i64("res").map_err(d)).collect::<Result<_, _>>()?;
        let n = r.count("rngs", 1 << 16, 16).map_err(d)?;
        exact(&r, "RNG streams", n, r0.streams.len())?;
        let rngs: Vec<Rng> = (0..n).map(|_| Ok(Rng::from_parts(r.u64("rng").map_err(d)?, r.u64("rng").map_err(d)?))).collect::<Result<_, SnapshotError>>()?;
        let next_id = r.u32("next_id").map_err(d)?;
        let n = r.count("entities", MAX_ENTITIES, 40).map_err(d)?;
        let mut ents: Vec<Entity> = Vec::with_capacity(n);
        for _ in 0..n {
            let id = r.u32("entity.id").map_err(d)?;
            if ents.last().is_some_and(|e| e.id >= id) || id >= next_id {
                return Err(bad(&r, format!("entity id {id} out of order (or not below next_id {next_id})")));
            }
            let arch = r.u16("entity.arch").map_err(d)?;
            let Some(a) = r0.archetypes.get(usize::from(arch)) else {
                return Err(bad(&r, format!("archetype {arch} (the rules have {})", r0.archetypes.len())));
            };
            let alive = r.bool("alive").map_err(d)?;
            let pos = r_p3(&mut r)?;
            let k = r.count("base", 1 << 16, 8).map_err(d)?;
            exact(&r, &format!("attributes on a {}", a.id), k, a.attrs.len())?;
            let base: Vec<i64> = (0..k).map(|_| r.i64("base").map_err(d)).collect::<Result<_, _>>()?;
            let k = r.count("effects", MAX_EFFECTS_PER_ENTITY, 40).map_err(d)?;
            let mut effects = Vec::with_capacity(k);
            for _ in 0..k {
                let effect = r.u16("effect").map_err(d)?;
                let Some(def) = r0.effects.get(usize::from(effect)) else {
                    return Err(bad(&r, format!("effect {effect} (the rules have {})", r0.effects.len())));
                };
                let (magnitude, applied, expires, next_period) = (r.i64("magnitude").map_err(d)?, r.u32("applied").map_err(d)?, r.u32("expires").map_err(d)?, r.u32("next_period").map_err(d)?);
                let (stacks, seq, source) = (r.u16("stacks").map_err(d)?, r.u32("seq").map_err(d)?, r.opt_u32("source").map_err(d)?);
                let m = r.count("mods", 64, 8).map_err(d)?;
                exact(&r, &format!("modifiers of {}", def.id), m, def.modifiers.len())?;
                let mods: Vec<i64> = (0..m).map(|_| r.i64("mod").map_err(d)).collect::<Result<_, _>>()?;
                effects.push(Active { effect, magnitude, applied, expires, next_period, stacks, seq, source, mods });
            }
            let k = r.count("behaviours", 1 << 10, 1).map_err(d)?;
            exact(&r, &format!("behaviours on a {}", a.id), k, a.behaviours.len())?;
            let mut beh = Vec::with_capacity(k);
            for b in &a.behaviours {
                let s = match r.u8("beh").map_err(d)? {
                    0 => BehState::None,
                    1 => {
                        let has = r.bool("path").map_err(d)?;
                        let p = r.u16("path").map_err(d)?;
                        let seg = r.u32("seg").map_err(d)?;
                        let path = has.then_some(p);
                        let progress = r.i64("progress").map_err(d)?;
                        // On its segment: a segment index the path has, progress along it.
                        let seg_len = match path {
                            Some(p) => match r0.paths.get(usize::from(p)).and_then(|pd| pd.seg.get(seg as usize)) {
                                Some(len) => *len,
                                None => return Err(bad(&r, format!("path {p} segment {seg}"))),
                            },
                            None => 0,
                        };
                        if !(0..=seg_len.max(0)).contains(&progress) {
                            return Err(bad(&r, format!("progress {progress} along a segment {seg_len} mm long")));
                        }
                        BehState::Path { path, seg, progress, carry: r_carry(&mut r)?, fwd: r.bool("fwd").map_err(d)?, done: r.bool("done").map_err(d)? }
                    }
                    2 => BehState::Chase { target: r.opt_u32("target").map_err(d)?, carry: r_carry(&mut r)?, reached: r.bool("reached").map_err(d)? },
                    3 => BehState::Attack { ready_at: r.u32("ready_at").map_err(d)? },
                    4 => {
                        let target = r.opt_u32("target").map_err(d)?;
                        let aim = r_p3(&mut r)?;
                        let m = r.count("payload", 64, 16).map_err(d)?;
                        let payload: Vec<(i64, i64)> = (0..m).map(|_| Ok((r.i64("payload").map_err(d)?, r.i64("payload").map_err(d)?))).collect::<Result<_, SnapshotError>>()?;
                        let attack = (r.u16("attack.arch").map_err(d)?, r.u16("attack.beh").map_err(d)?);
                        let fired_by = r0.archetypes.get(usize::from(attack.0)).and_then(|x| x.behaviours.get(usize::from(attack.1)));
                        match fired_by.map(|x| &x.kind) {
                            Some(BehKind::Attack(def)) if def.projectile.is_some() && def.payload.len() == payload.len() => {}
                            _ => return Err(bad(&r, format!("a projectile fired by archetype {} behaviour {} that doesn't fire one like it", attack.0, attack.1))),
                        }
                        BehState::Projectile(Box::new(Proj { target, aim, payload, attack, source: r.opt_u32("source").map_err(d)?, carry: r_carry(&mut r)? }))
                    }
                    5 => BehState::Spawner { next: r.u32("next").map_err(d)? },
                    6 => BehState::Lifetime { expires: r.u32("expires").map_err(d)? },
                    v => return Err(bad(&r, format!("behaviour state kind {v}"))),
                };
                if std::mem::discriminant(&s) != std::mem::discriminant(&initial_state(b, 0, None, true)) {
                    return Err(bad(&r, format!("a {} behaviour's state doesn't match its kind", a.id)));
                }
                beh.push(s);
            }
            let state = if r.bool("machine").map_err(d)? {
                let (s, t) = (r.u16("machine.state").map_err(d)?, r.u32("machine.entered").map_err(d)?);
                match a.machine.and_then(|m| r0.machines.get(usize::from(m))) {
                    Some(m) if usize::from(s) < m.states.len() => Some((s, t)),
                    _ => return Err(bad(&r, format!("machine state {s} on a {}", a.id))),
                }
            } else {
                None
            };
            // `first` is the lowest valid value: waves are 1-based (wave 1 is the first).
            let mut opt = |what: &str, first: usize, limit: usize| -> Result<Option<u16>, SnapshotError> {
                let has = r.bool(what).map_err(d)?;
                let v = r.u16(what).map_err(d)?;
                if has && (usize::from(v) < first || usize::from(v) >= limit + first) {
                    return Err(bad(&r, format!("{what} {v} (valid: {first}..{})", limit + first)));
                }
                Ok(has.then_some(v))
            };
            let waves = r0.waves.as_ref().map_or(0, |w| w.waves.len());
            let (wave, site, spawn_path) = (opt("wave", 1, waves)?, opt("site", 0, r0.sites.len())?, opt("spawn_path", 0, r0.paths.len())?);
            let path_remaining = r.i64("path_remaining").map_err(d)?;
            // Version 1 had no `order` (0: no rules could give one then).
            let order = if version >= 2 { r.i64("order").map_err(d)? } else { 0 };
            ents.push(Entity { id, arch, alive, pos, base, effects, beh, state, wave, site, spawn_path, path_remaining, order });
        }
        let waves = r0.waves.as_ref().map_or(0, |w| w.waves.len());
        let wave = r.u16("wave").map_err(d)?;
        if usize::from(wave) > waves {
            return Err(bad(&r, format!("wave {wave} (the rules have {waves})")));
        }
        let next_wave_tick = r.opt_u32("next_wave_tick").map_err(d)?;
        let n = r.count("wave_stats", 10_000, 30).map_err(d)?;
        exact(&r, "waves of statistics", n, waves)?;
        let mut wave_stats = Vec::with_capacity(n);
        for _ in 0..n {
            let (started_at, cleared_at) = (r.opt_u32("started_at").map_err(d)?, r.opt_u32("cleared_at").map_err(d)?);
            let v: Vec<u32> = (0..5).map(|_| r.u32("wave_stat").map_err(d)).collect::<Result<_, _>>()?;
            wave_stats.push(WaveStats { started_at, cleared_at, spawned: v[0], died: v[1], removed: v[2], pending: v[3], alive: v[4] });
        }
        let n = r.count("pending", MAX_PENDING_SPAWNS, 13).map_err(d)?;
        let mut pending = Vec::with_capacity(n);
        for _ in 0..n {
            let (tick, seq, arch, wave) = (r.u32("pending.tick").map_err(d)?, r.u32("pending.seq").map_err(d)?, r.u16("pending.arch").map_err(d)?, r.u16("pending.wave").map_err(d)?);
            if usize::from(arch) >= r0.archetypes.len() || wave == 0 || usize::from(wave) > waves {
                return Err(bad(&r, format!("a pending spawn of archetype {arch} in wave {wave}")));
            }
            let at = match (r.u8("pending.at").map_err(d)?, r.u16("pending.at").map_err(d)?) {
                (0, x) if usize::from(x) < r0.paths.len() => SpawnAt::Path(x),
                (1, x) if usize::from(x) < r0.sites.len() => SpawnAt::Site(x),
                (k, x) => return Err(bad(&r, format!("spawn place {k}:{x}"))),
            };
            // Version 1 had no `with` (no rules could give one then).
            let mut with = Vec::new();
            if version >= 2 {
                let k = r.count("pending.with", 1 << 10, 10).map_err(d)?;
                for _ in 0..k {
                    let (a, v) = (r.u16("with.attr").map_err(d)?, r.i64("with.value").map_err(d)?);
                    if usize::from(a) >= r0.attrs.len() {
                        return Err(bad(&r, format!("attribute {a} in a pending spawn's with (the rules have {})", r0.attrs.len())));
                    }
                    with.push((a, v));
                }
            }
            pending.push(Pending { tick, seq, arch, wave, at, with });
        }
        let next_seq = r.u32("next_seq").map_err(d)?;
        let n = r.count("timers", 1 << 16, 2).map_err(d)?;
        exact(&r, "timers", n, r0.timers.len())?;
        let timers: Vec<TimerState> = (0..n).map(|_| Ok(TimerState { next: r.opt_u32("timer.next").map_err(d)?, fired: r.u32("timer.fired").map_err(d)? })).collect::<Result<_, SnapshotError>>()?;
        let kinds = r0.events.len();
        let n = r.count("queue", MAX_EVENTS_PER_TICK, 12).map_err(d)?;
        let queue: Vec<Event> = (0..n).map(|_| r_event(&mut r, kinds)).collect::<Result<_, _>>()?;
        let n = r.count("events", MAX_EVENTS_PER_TICK, 12).map_err(d)?;
        let events: Vec<Event> = (0..n).map(|_| r_event(&mut r, kinds)).collect::<Result<_, _>>()?;
        let n = r.count("event_counts", 1 << 16, 8).map_err(d)?;
        exact(&r, "event counts", n, kinds)?;
        let event_counts: Vec<u64> = (0..n).map(|_| r.u64("event_count").map_err(d)).collect::<Result<_, _>>()?;
        let n = r.count("diag", 4096, 12).map_err(d)?;
        let mut diag = RtDiag::default();
        for _ in 0..n {
            let code = r.str("diag.code", 64).map_err(d)?;
            let v = r.u64("diag.count").map_err(d)?;
            if let Some(c) = CODES.iter().find(|c| **c == code) {
                diag.counts.insert(c, v);
            }
        }
        let n = r.count("diag.first", MAX_DIAG_MESSAGES, 4).map_err(d)?;
        diag.first = (0..n).map(|_| r.str("diag.message", 4096).map_err(d)).collect::<Result<_, _>>()?;
        let n = r.count("native_calls", 1 << 16, 8).map_err(d)?;
        exact(&r, "native call counts", n, r0.natives.len())?;
        let native_calls: Vec<u64> = (0..n).map(|_| r.u64("native_calls").map_err(d)).collect::<Result<_, _>>()?;
        let (dirty, dirty_full, interrupted) = (r.bool("dirty").map_err(d)?, r.bool("dirty_full").map_err(d)?, r.bool("interrupted").map_err(d)?);
        let effect_seq = r.u32("effect_seq").map_err(d)?;
        let (dropped, work, command_work, cells) = (r.u64("dropped").map_err(d)?, r.u64("work").map_err(d)?, r.u64("command_work").map_err(d)?, r.u64("cells").map_err(d)?);
        let dispatched = r.u64("dispatched").map_err(d)?.min(MAX_EVENTS_PER_TICK as u64) as usize;
        r.finish().map_err(d)?;
        if interrupted {
            return Err(bad(&r, "the saved run was cut short by a wall clock: it isn't a result to continue".into()));
        }
        // Ticks and counters the run adds to stay far from overflow; ids below the next ones.
        const MAX_COUNTER: u32 = 1 << 30;
        if [tick, next_id, next_seq, effect_seq].iter().any(|v| *v > MAX_COUNTER) || pending.iter().any(|p| p.seq >= next_seq || p.tick > MAX_COUNTER) {
            return Err(bad(&r, "a tick, id or sequence number is out of range".into()));
        }
        // Counts the run adds to (per wave, per event, per hook, per diagnostic).
        const MAX_COUNT: u64 = 1 << 62;
        let wave_counts = wave_stats.iter().flat_map(|s| [s.spawned, s.died, s.removed]).any(|v| v > MAX_COUNTER);
        let counts = event_counts.iter().chain(&native_calls).chain(diag.counts.values()).chain([&dropped, &work, &command_work]).any(|v| *v > MAX_COUNT);
        let ticks = wave_stats.iter().flat_map(|s| [s.started_at, s.cleared_at]).chain([next_wave_tick]).flatten().any(|t| t > MAX_COUNTER);
        if wave_counts || counts || ticks {
            return Err(bad(&r, "a count or tick is out of range".into()));
        }
        // Every counter the tick counts down agrees with the lists it counts.
        for (i, s) in wave_stats.iter().enumerate() {
            let w = (i + 1) as u16;
            let pending_n = pending.iter().filter(|p| p.wave == w).count() as u32;
            let alive_n = ents.iter().filter(|e| e.alive && e.wave == Some(w)).count() as u32;
            if s.pending != pending_n || s.alive != alive_n {
                return Err(bad(&r, format!("wave {w}'s counts (pending {}, alive {}) don't match its spawns and entities ({pending_n}, {alive_n})", s.pending, s.alive)));
            }
        }
        // When granted tags run out next (as `begin` keeps it): derived from the effects.
        let tag_expiry = ents
            .iter()
            .flat_map(|e| e.effects.iter())
            .filter(|a| a.expires >= tick && !r0.effects[usize::from(a.effect)].grant_tags.is_empty())
            .map(|a| a.expires)
            .min()
            .unwrap_or(u32::MAX);
        sim.st = State {
            tick,
            seed,
            outcome,
            res,
            rngs,
            next_id,
            ents,
            wave,
            next_wave_tick,
            wave_stats,
            pending,
            next_seq,
            timers,
            queue,
            events,
            event_counts,
            diag,
            native_calls,
            dirty,
            dirty_full,
            effect_seq,
            dropped,
            work,
            command_work,
            cells: cells.min(MAX_STATE_CELLS),
            interrupted,
            dispatched,
            // A restored run is a new state value: caches keyed by (instance, gen) miss once,
            // and natives charge their work the same either way (LEX.md, "Caching").
            gen: 0,
            tag_expiry,
            instance: Instance::fresh(),
        };
        Ok(sim)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: &str = r#"{"lex": 1, "name": "t", "tick_hz": 30, "items": {
        "gold": {"kind": "resource", "start": 20, "min": 0},
        "lane": {"kind": "path", "points": [[0, 0, 0], [0, 0, 30]]},
        "spots": {"kind": "sites", "points": {"a": [1, 0, 10], "b": [-1, 0, 20]}},
        "slow": {"kind": "effect", "duration": "1s", "magnitude": "ratio", "stacking": "strongest",
                 "modifiers": [{"attribute": "speed_scale", "op": "mul", "value": "100% - magnitude"}]},
        "grunt": {"kind": "archetype", "tags": ["enemy"],
                  "attributes": {"hp": {"base": 30, "unit": "points"}, "speed": {"base": 1.5, "unit": "speed"}, "speed_scale": "100%"},
                  "behaviours": [
                    {"type": "health", "attribute": "hp", "on_death": [{"add": "gold", "amount": "rand(drops, 1, 4)"}]},
                    {"type": "follow_path", "path": "@spawn", "speed": "self.speed", "scale": "self.speed_scale",
                     "on_arrive": [{"despawn": "self"}]}]},
        "bolt": {"kind": "archetype", "tags": ["projectile"], "attributes": {"speed": "8m/s"},
                 "behaviours": [{"type": "projectile", "speed": "self.speed", "homing": true}]},
        "tower": {"kind": "archetype", "tags": ["tower"], "behaviours": [
                    {"type": "attack", "targets": ["enemy"], "range": "6m", "cooldown": "0.5s", "policy": "first",
                     "projectile": {"archetype": "bolt"}, "payload": [{"damage": "4pt"}, {"effect": "slow", "magnitude": "30%"}]}]},
        "build": {"kind": "command", "args": {"site": "site"}, "do": [{"spawn": "tower", "at": "arg.site"}]},
        "waves": {"kind": "waves", "start": "1s", "rng": "waves",
                  "waves": [{"duration": "8s", "groups": [{"archetype": "grunt", "count": 6, "at": ["lane"], "start": "0s", "every": "0.7s"}]},
                            {"groups": [{"archetype": "grunt", "count": 9, "at": ["lane"], "start": "0s", "every": "0.4s"}]}]},
        "win": {"kind": "outcome", "result": "won", "when": "waves.all_cleared"}
    }}"#;

    fn sim(seed: u64) -> Sim {
        let doc = crate::Document::parse(RULES).unwrap();
        let c = crate::compile(&doc, &Default::default()).unwrap_or_else(|d| panic!("{d:#?}"));
        Sim::new(Arc::new(c.rules), seed, &Natives::new()).unwrap()
    }

    #[test]
    fn a_restored_run_steps_exactly_like_the_saved_one() {
        let mut a = sim(3);
        let build = a.parse_command("build", &Json::parse(r#"{"site": "a"}"#).unwrap()).unwrap();
        a.step(&[build]);
        for _ in 0..200 {
            a.step(&[]);
        }
        assert!(a.entities().len() > 2, "towers, grunts and bolts are alive");
        let bytes = a.save_state();
        let mut b = Sim::restore(Arc::clone(a.rules()), &Natives::new(), &bytes).unwrap();
        assert_eq!(b.hash(), a.hash());
        assert_eq!(b.save_state(), bytes, "bytes are canonical");
        for t in 0..600 {
            a.step(&[]);
            b.step(&[]);
            assert_eq!(a.hash(), b.hash(), "diverged {t} ticks after the restore");
        }
        assert_eq!(a.outcome(), b.outcome());
    }

    #[test]
    fn damaged_or_foreign_state_is_refused_never_a_panic() {
        let mut a = sim(5);
        let build = a.parse_command("build", &Json::parse(r#"{"site": "b"}"#).unwrap()).unwrap();
        a.step(&[build]);
        for _ in 0..150 {
            a.step(&[]);
        }
        let bytes = a.save_state();
        let rules = Arc::clone(a.rules());
        for cut in 0..bytes.len() {
            assert!(Sim::restore(Arc::clone(&rules), &Natives::new(), &bytes[..cut]).is_err(), "cut {cut}");
        }
        // Bit flips and whole fields set to extremes; whatever loads steps on a long while.
        let mut x = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let extremes: [u64; 8] = [0, u64::MAX, 1, 0x7fff_ffff, 0x8000_0000, 0xffff_ffff, 0x8000_0000_0000_0000, 0x7fff_ffff_ffff_ffff];
        let mut loaded = 0;
        // SPECULA_FUZZ_ROUNDS raises the rounds for a long local run.
        let rounds = std::env::var("SPECULA_FUZZ_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(2000usize);
        for _ in 0..rounds {
            let mut m = bytes.clone();
            for _ in 0..1 + next() % 3 {
                let i = 12 + (next() as usize) % (m.len() - 12);
                if next() % 2 == 0 {
                    m[i] ^= (next() >> 32) as u8 | 1;
                } else {
                    let width = [1usize, 2, 4, 8][(next() % 4) as usize];
                    let v = extremes[(next() % 8) as usize].to_le_bytes();
                    let end = (i + width).min(m.len());
                    m[i..end].copy_from_slice(&v[..end - i]);
                }
            }
            if let Ok(mut s) = Sim::restore(Arc::clone(&rules), &Natives::new(), &m) {
                loaded += 1;
                for _ in 0..300 {
                    s.step(&[]);
                }
            }
        }
        assert!(loaded > 0, "the fuzzer must reach the tick");
        // Counters that disagree with their lists, and wave 0, are refused.
        let mut bad = a.clone();
        if let Some(w) = bad.st.wave_stats.iter_mut().find(|w| w.alive > 0) {
            w.alive += 1;
            assert!(Sim::restore(Arc::clone(&rules), &Natives::new(), &bad.save_state()).is_err());
        }
        let mut bad = a.clone();
        if let Some(e) = bad.st.ents.iter_mut().find(|e| e.wave.is_some()) {
            e.wave = Some(0);
            assert!(Sim::restore(Arc::clone(&rules), &Natives::new(), &bad.save_state()).is_err());
        }
        // Other rules (another param value is another hash).
        let other = RULES.replace("\"start\": 20", "\"start\": 21");
        let c = crate::compile(&crate::Document::parse(&other).unwrap(), &Default::default()).unwrap();
        let e = Sim::restore(Arc::new(c.rules), &Natives::new(), &bytes).err().unwrap();
        assert_eq!(e.code, "rules_changed");
    }

    /// Version 1 (before `order` and `with`) reads forward: a run of rules without them
    /// restores to the same hash and steps the same; rules with them refuse version 1.
    #[test]
    fn version_1_state_reads_forward() {
        let mut a = sim(3);
        let build = a.parse_command("build", &Json::parse(r#"{"site": "a"}"#).unwrap()).unwrap();
        a.step(&[build]);
        for _ in 0..150 {
            a.step(&[]);
        }
        let v1 = a.write_state(1);
        assert_ne!(v1, a.save_state());
        let mut b = Sim::restore(Arc::clone(a.rules()), &Natives::new(), &v1).unwrap();
        assert_eq!(b.hash(), a.hash(), "the defaults reproduce the hash");
        assert_eq!(b.save_state(), a.save_state(), "and read forward to today's bytes");
        for _ in 0..300 {
            a.step(&[]);
            b.step(&[]);
            assert_eq!((a.hash(), a.tick_work()), (b.hash(), b.tick_work()));
        }
        let ordered = RULES.replace(r#""tower": {"kind": "archetype", "tags": ["tower"],"#, r#""tower": {"kind": "archetype", "tags": ["tower"], "order": "1","#);
        assert_ne!(ordered, RULES);
        let c = crate::compile(&crate::Document::parse(&ordered).unwrap(), &Default::default()).unwrap();
        let o = Sim::new(Arc::new(c.rules), 1, &Natives::new()).unwrap();
        let e = Sim::restore(Arc::clone(o.rules()), &Natives::new(), &o.write_state(1)).err().unwrap();
        assert_eq!(e.code, "unsupported_version");
        let back = Sim::restore(Arc::clone(o.rules()), &Natives::new(), &o.save_state()).unwrap();
        assert_eq!(back.hash(), o.hash());
    }
}
