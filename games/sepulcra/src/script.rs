//! Scripts, journals and headless runs: the tools for replays and balance swarms.
//!
//! A **script** (a layout file) is a build order an agent writes by hand:
//!
//! ```text
//! { "name": "centre-heavy",
//!   "commands": [
//!     { "wave": 0, "place": "warding", "at": "top:12,9" },
//!     { "wave": 0, "wall": "top:9,14" },
//!     { "wave": 3, "upgrade": "top:12,9" },
//!     { "wave": 6, "sell": "top:12,9" },
//!     { "tick": 2400, "unwall": "top:9,14" },
//!     { "wave": 2, "call_wave": true } ] }
//! ```
//!
//! `"wave": N` means "once wave N has started" (0 = from the first tick); `"tick": N` means "at
//! or after tick N". Entries run strictly in order, like a player following a build order: an
//! entry refused for lack of dust waits (and so does everything after it); an entry refused for
//! any other reason is skipped and reported.
//!
//! A **journal** is what actually happened: every applied command with its tick. A
//! [`Recording`] adds the seed, the content hash and the final tick and state hash, so a replay
//! checks itself:
//!
//! ```text
//! { "seed": "18446744073709551615", "content_hash": "073b05285342fa20",
//!   "end": { "tick": 21838, "hash": "89c57734ef694a1b" },
//!   "commands": [ { "tick": 0, "place": "warding", "at": "top:8,12" }, ... ] }
//! ```
//!
//! The seed and hashes are strings because JSON numbers are doubles: a seed above 2^53 would
//! silently change. The forms are strict, so every value has one spelling: the seed is a
//! decimal `u64` without sign or leading zeros (`0|[1-9][0-9]*`), and hashes are exactly 16
//! lowercase hex digits with no `0x`.

use std::fmt;
use std::sync::Arc;

use specula_core::Json;

use crate::content::Content;
use crate::error::{DataError, ErrorCode, Obj};
use crate::sim::{Command, Event, Outcome, Rejection, State, WaveStats};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    Tick(u32),
    /// Once this many waves have started.
    Wave(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub when: When,
    pub command: Command,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Script {
    pub name: String,
    pub entries: Vec<Entry>,
}

const COMMAND_KEYS: [&str; 6] = ["place", "upgrade", "sell", "wall", "unwall", "call_wave"];

/// A command as JSON (`{"place": "warding", "at": "top:12,9"}` and so on).
pub fn command_to_json(cmd: &Command, content: &Content) -> Json {
    let name = |c| Json::from(content.map.cell_name(c));
    match *cmd {
        Command::PlaceStone { cell, stone } => Json::obj([("place", Json::from(content.stone(stone).id.as_str())), ("at", name(cell))]),
        Command::Upgrade { cell } => Json::obj([("upgrade", name(cell))]),
        Command::Sell { cell } => Json::obj([("sell", name(cell))]),
        Command::PlaceWall { cell } => Json::obj([("wall", name(cell))]),
        Command::RemoveWall { cell } => Json::obj([("unwall", name(cell))]),
        Command::CallWave => Json::obj([("call_wave", Json::Bool(true))]),
    }
}

/// Parses a command object (extra keys allowed in `extra`, e.g. `tick`).
fn command_from_obj(o: &Obj, content: &Content, extra: &[&str]) -> Result<Command, DataError> {
    let mut allowed: Vec<&str> = COMMAND_KEYS.to_vec();
    allowed.extend_from_slice(extra);
    allowed.extend_from_slice(&["at", "note"]);
    o.only(&allowed)?;
    let present: Vec<&str> = COMMAND_KEYS.iter().copied().filter(|k| o.get(k).is_some()).collect();
    if present.len() != 1 {
        return Err(o.err("", ErrorCode::Invalid, format!("needs exactly one of: {}", COMMAND_KEYS.join(", "))));
    }
    let cell = |key: &str| {
        let s = o.str(key)?;
        content.map.cell_ref(s).ok_or_else(|| o.err(key, ErrorCode::UnknownRef, format!("no cell {s:?} (expected \"face:col,row\", e.g. \"top:12,9\")")))
    };
    Ok(match present[0] {
        "place" => {
            let id = o.str("place")?;
            let stone = content.stone_kind(id).ok_or_else(|| {
                let ids: Vec<&str> = content.stones.iter().map(|s| s.id.as_str()).collect();
                o.err("place", ErrorCode::UnknownRef, format!("no stone {id:?} (stones: {})", ids.join(", ")))
            })?;
            Command::PlaceStone { cell: cell("at")?, stone }
        }
        "upgrade" => Command::Upgrade { cell: cell("upgrade")? },
        "sell" => Command::Sell { cell: cell("sell")? },
        "wall" => Command::PlaceWall { cell: cell("wall")? },
        "unwall" => Command::RemoveWall { cell: cell("unwall")? },
        _ => Command::CallWave,
    })
}

impl Script {
    /// Parses a layout file. `file` names it in errors.
    pub fn parse(text: &str, content: &Content, file: &str) -> Result<Script, DataError> {
        let json = Json::parse(text).map_err(|e| DataError::new(file, format!("byte {}", e.offset), ErrorCode::Syntax, e.message))?;
        let root = Obj::new(&json, file, "")?;
        root.only(&["name", "notes", "commands"])?;
        let mut entries = Vec::new();
        for (i, c) in root.arr("commands")?.iter().enumerate() {
            let o = Obj::new(c, file, format!("commands[{i}]"))?;
            let when = match (o.get("tick"), o.get("wave")) {
                (Some(_), Some(_)) => return Err(o.err("tick", ErrorCode::Invalid, "use either \"tick\" or \"wave\", not both")),
                (Some(_), None) => When::Tick(o.int("tick", 0, i64::from(u32::MAX))? as u32),
                (None, Some(_)) => When::Wave(o.int("wave", 0, 1000)? as u16),
                (None, None) => When::Wave(0),
            };
            entries.push(Entry { when, command: command_from_obj(&o, content, &["tick", "wave"])? });
        }
        Ok(Script { name: root.str("name")?.to_owned(), entries })
    }

    pub fn to_json(&self, content: &Content) -> Json {
        let cmds = self.entries.iter().map(|e| {
            let mut j = command_to_json(&e.command, content);
            match e.when {
                When::Tick(t) => j.set("tick", Json::from(t)),
                When::Wave(w) => j.set("wave", Json::from(u32::from(w))),
            };
            j
        });
        Json::obj([("name", Json::from(self.name.as_str())), ("commands", Json::arr(cmds))])
    }
}

/// Follows a [`Script`] against a running state.
#[derive(Clone, Debug)]
pub struct Player {
    script: Script,
    next: usize,
    /// Entries skipped, with why.
    pub skipped: Vec<(usize, Rejection)>,
}

impl Player {
    pub fn new(script: Script) -> Player {
        Player { script, next: 0, skipped: Vec::new() }
    }

    /// Applies every entry that is due and affordable now (via [`State::apply`]) and returns
    /// the commands applied, in order, for the journal.
    pub fn drive(&mut self, state: &mut State, ev: &mut Vec<Event>) -> Vec<Command> {
        let mut applied = Vec::new();
        while let Some(e) = self.script.entries.get(self.next) {
            let due = match e.when {
                When::Tick(t) => state.tick() >= t,
                When::Wave(w) => state.wave() >= w,
            };
            if !due {
                break;
            }
            match state.apply(&e.command, ev) {
                Ok(()) => applied.push(e.command),
                Err(Rejection::NotEnoughDust { .. }) => break,
                Err(Rejection::GameOver) => break,
                Err(r) => self.skipped.push((self.next, r)),
            }
            self.next += 1;
        }
        applied
    }

    pub fn finished(&self) -> bool {
        self.next >= self.script.entries.len()
    }
}

/// One applied command and the tick it was applied at (before that tick advanced).
pub type Journal = Vec<(u32, Command)>;

/// A replayable run: seed, content, commands, and (when the run is finished) where it ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recording {
    pub seed: u64,
    pub content_hash: u64,
    pub journal: Journal,
    pub end: Option<RunEnd>,
}

/// Where a recorded run stopped: the tick and the state hash there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunEnd {
    pub tick: u32,
    pub hash: u64,
}

/// Why a recording failed to replay. Every variant means the recording was edited, truncated,
/// or made by different rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayError {
    /// A recorded command was refused on replay (a recorder only journals accepted commands).
    Rejected { tick: u32, command: Command, reason: Rejection },
    /// A command is stamped at or after the point where the run ended, so it never ran.
    NotApplied { tick: u32, ended_at: u32 },
    /// The replay ended at a different tick or with a different state hash.
    Mismatch { expected: RunEnd, got: RunEnd },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplayError::Rejected { tick, command, reason } => write!(f, "replay_rejected: {command:?} at tick {tick} was refused ({reason})"),
            ReplayError::NotApplied { tick, ended_at } => write!(f, "replay_not_applied: a command at tick {tick} never ran (the run ended at tick {ended_at})"),
            ReplayError::Mismatch { expected, got } => write!(
                f,
                "replay_mismatch: recorded tick {} hash {:016x}, replayed tick {} hash {:016x}",
                expected.tick, expected.hash, got.tick, got.hash
            ),
        }
    }
}

impl std::error::Error for ReplayError {}

/// Whether a replay could be checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayStatus {
    /// It reached the recorded end tick with the recorded state hash.
    Verified,
    /// The recording has no `end`, so there was nothing to compare against.
    Unverified,
}

#[derive(Clone, Debug)]
pub struct Replayed {
    pub state: State,
    pub status: ReplayStatus,
}

fn hex_u64(o: &Obj, key: &str) -> Result<u64, DataError> {
    let s = o.str(key)?;
    let canonical = s.len() == 16 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    match u64::from_str_radix(s, 16) {
        Ok(v) if canonical => Ok(v),
        _ => Err(o.err(key, ErrorCode::BadType, format!("expected exactly 16 lowercase hex digits (no 0x), got {s:?}"))),
    }
}

fn seed_u64(o: &Obj, key: &str) -> Result<u64, DataError> {
    let Some(s) = o.req(key)?.as_str() else {
        return Err(o.err(key, ErrorCode::BadType, "expected the seed as a decimal string, e.g. \"42\""));
    };
    let canonical = !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && (s == "0" || !s.starts_with('0'));
    if !canonical {
        return Err(o.err(key, ErrorCode::BadType, format!("expected a decimal u64 without sign or leading zeros, got {s:?}")));
    }
    s.parse::<u64>().map_err(|_| o.err(key, ErrorCode::OutOfRange, format!("{s} is larger than a u64")))
}

impl Recording {
    pub fn to_json(&self, content: &Content) -> Json {
        let cmds = self.journal.iter().map(|(t, c)| {
            let mut j = command_to_json(c, content);
            j.set("tick", Json::from(*t));
            j
        });
        let mut pairs = vec![
            ("seed", Json::from(self.seed.to_string())),
            ("content_hash", Json::from(format!("{:016x}", self.content_hash))),
        ];
        if let Some(end) = self.end {
            pairs.push(("end", Json::obj([("tick", Json::from(end.tick)), ("hash", Json::from(format!("{:016x}", end.hash)))])));
        }
        pairs.push(("commands", Json::arr(cmds)));
        Json::obj(pairs)
    }

    /// Parses a recording made against `content`. A different content hash is refused
    /// ([`ErrorCode::ContentMismatch`]): its commands would mean something else.
    pub fn from_json(text: &str, content: &Content, file: &str) -> Result<Recording, DataError> {
        let json = Json::parse(text).map_err(|e| DataError::new(file, format!("byte {}", e.offset), ErrorCode::Syntax, e.message))?;
        let root = Obj::new(&json, file, "")?;
        root.only(&["seed", "content_hash", "end", "commands"])?;
        let seed = seed_u64(&root, "seed")?;
        let content_hash = hex_u64(&root, "content_hash")?;
        if content_hash != content.hash {
            return Err(root.err(
                "content_hash",
                ErrorCode::ContentMismatch,
                format!("recorded against content {content_hash:016x}, loaded content is {:016x}", content.hash),
            ));
        }
        let end = match root.get("end") {
            None => None,
            Some(_) => {
                let e = root.obj("end")?;
                e.only(&["tick", "hash"])?;
                Some(RunEnd { tick: e.int("tick", 0, i64::from(u32::MAX))? as u32, hash: hex_u64(&e, "hash")? })
            }
        };
        let mut journal = Vec::new();
        for (i, c) in root.arr("commands")?.iter().enumerate() {
            let o = Obj::new(c, file, format!("commands[{i}]"))?;
            let tick = o.int("tick", 0, i64::from(u32::MAX))? as u32;
            if journal.last().is_some_and(|(t, _)| *t > tick) {
                return Err(o.err("tick", ErrorCode::Invalid, "ticks must not decrease"));
            }
            journal.push((tick, command_from_obj(&o, content, &["tick"])?));
        }
        Ok(Recording { seed, content_hash, journal, end })
    }

    /// Replays the recording and checks it: every command must be accepted, none may be stamped
    /// at or after the end, and the run must reach the recorded end tick with the recorded
    /// state hash. Without a recorded `end` the replay runs until the game ends and comes back
    /// [`ReplayStatus::Unverified`].
    pub fn verify(&self, content: Arc<Content>) -> Result<Replayed, ReplayError> {
        let until = self.end.map_or(u32::MAX, |e| e.tick);
        if let Some(&(tick, _)) = self.journal.iter().find(|(t, _)| *t >= until) {
            return Err(ReplayError::NotApplied { tick, ended_at: until });
        }
        let mut state = State::new(content, self.seed);
        let mut k = 0;
        let (mut cmds, mut ev) = (Vec::new(), Vec::new());
        while state.tick() < until && state.outcome() == Outcome::Running {
            let tick = state.tick();
            cmds.clear();
            ev.clear();
            while k < self.journal.len() && self.journal[k].0 <= tick {
                cmds.push(self.journal[k].1);
                k += 1;
            }
            state.step_into(&cmds, &mut ev);
            if let Some(&Event::Rejected { index, reason }) = ev.iter().find(|e| matches!(e, Event::Rejected { .. })) {
                return Err(ReplayError::Rejected { tick, command: cmds[usize::from(index)], reason });
            }
        }
        if let Some(&(tick, _)) = self.journal.get(k) {
            return Err(ReplayError::NotApplied { tick, ended_at: state.tick() });
        }
        let Some(expected) = self.end else {
            return Ok(Replayed { state, status: ReplayStatus::Unverified });
        };
        let got = RunEnd { tick: state.tick(), hash: state.hash() };
        if got != expected {
            return Err(ReplayError::Mismatch { expected, got });
        }
        Ok(Replayed { state, status: ReplayStatus::Verified })
    }
}

/// Replays a journal from a fresh state up to (not including) tick `until`, or until the game
/// ends. `on_tick` sees the state after each step.
pub fn replay(content: Arc<Content>, seed: u64, journal: &Journal, until: u32, mut on_tick: impl FnMut(&State, &[Event])) -> State {
    let mut state = State::new(content, seed);
    let mut k = 0;
    let (mut cmds, mut ev) = (Vec::new(), Vec::new());
    while state.tick() < until && state.outcome() == Outcome::Running {
        cmds.clear();
        ev.clear();
        while k < journal.len() && journal[k].0 <= state.tick() {
            cmds.push(journal[k].1);
            k += 1;
        }
        state.step_into(&cmds, &mut ev);
        on_tick(&state, &ev);
    }
    state
}

/// The result of a headless run.
#[derive(Clone, Debug)]
pub struct RunReport {
    pub seed: u64,
    pub outcome: Outcome,
    pub ticks: u32,
    pub wave: u16,
    pub tombstones: i32,
    pub dust: i32,
    pub hash: u64,
    pub waves: Vec<WaveStats>,
    pub journal: Journal,
    pub skipped: Vec<(usize, Rejection)>,
}

impl RunReport {
    pub fn leaks(&self) -> u32 {
        self.waves.iter().map(|w| w.leaked).sum()
    }

    pub fn kills(&self) -> u32 {
        self.waves.iter().map(|w| w.killed).sum()
    }

    /// Enemies removed with no route at all: always 0 on validated content.
    pub fn stranded(&self) -> u32 {
        self.waves.iter().map(|w| w.stranded).sum()
    }

    /// The run as a self-checking [`Recording`].
    pub fn recording(&self, content: &Content) -> Recording {
        Recording {
            seed: self.seed,
            content_hash: content.hash,
            journal: self.journal.clone(),
            end: Some(RunEnd { tick: self.ticks, hash: self.hash }),
        }
    }
}

/// Plays `script` from a fresh state until the game ends, or until `max_waves` waves have been
/// played out (the run stops just before wave `max_waves + 1` would start). A safety cap stops
/// runs that stall (e.g. a boss that can never be killed) 20 minutes after the last wave.
pub fn run(content: Arc<Content>, seed: u64, script: &Script, max_waves: Option<u16>, mut on_tick: impl FnMut(&State, &[Event])) -> RunReport {
    let cap = content.scheduled_ticks() + 20 * 60 * crate::fixed::TICK_HZ;
    let mut state = State::new(content, seed);
    let mut player = Player::new(script.clone());
    let mut journal = Journal::new();
    let mut ev = Vec::new();
    while state.outcome() == Outcome::Running && state.tick() < cap {
        if let Some(n) = max_waves {
            let w = state.wave();
            let next_due = state.next_wave_tick().is_some_and(|t| state.tick() >= t);
            let cleared = w > 0 && state.wave_stats()[usize::from(w) - 1].cleared_at.is_some();
            if w > n || (w == n && (next_due || cleared)) {
                break;
            }
        }
        ev.clear();
        let tick = state.tick();
        for c in player.drive(&mut state, &mut ev) {
            journal.push((tick, c));
        }
        state.step_into(&[], &mut ev);
        on_tick(&state, &ev);
    }
    RunReport {
        seed,
        outcome: state.outcome(),
        ticks: state.tick(),
        wave: state.wave(),
        tombstones: state.tombstones(),
        dust: state.dust(),
        hash: state.hash(),
        waves: state.wave_stats().to_vec(),
        journal,
        skipped: player.skipped,
    }
}
