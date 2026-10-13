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
//! A **journal** is what actually happened: every applied command with its tick. Replaying a
//! journal with the same seed and content reproduces the run bit for bit.

use std::sync::Arc;

use specula_core::Json;

use crate::content::Content;
use crate::error::{DataError, Obj};
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
        return Err(o.err("", format!("needs exactly one of: {}", COMMAND_KEYS.join(", "))));
    }
    let cell = |key: &str| {
        let s = o.str(key)?;
        content.map.cell_ref(s).ok_or_else(|| o.err(key, format!("no cell {s:?} (expected \"face:col,row\", e.g. \"top:12,9\")")))
    };
    Ok(match present[0] {
        "place" => {
            let id = o.str("place")?;
            let stone = content.stone_kind(id).ok_or_else(|| {
                let ids: Vec<&str> = content.stones.iter().map(|s| s.id.as_str()).collect();
                o.err("place", format!("no stone {id:?} (stones: {})", ids.join(", ")))
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
        let json = Json::parse(text).map_err(|e| DataError::new(file, format!("byte {}", e.offset), e.message))?;
        let root = Obj::new(&json, file, "")?;
        root.only(&["name", "notes", "commands"])?;
        let mut entries = Vec::new();
        for (i, c) in root.arr("commands")?.iter().enumerate() {
            let o = Obj::new(c, file, format!("commands[{i}]"))?;
            let when = match (o.get("tick"), o.get("wave")) {
                (Some(_), Some(_)) => return Err(o.err("tick", "use either \"tick\" or \"wave\", not both")),
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

pub fn journal_to_json(seed: u64, journal: &Journal, content: &Content) -> Json {
    let cmds = journal.iter().map(|(t, c)| {
        let mut j = command_to_json(c, content);
        j.set("tick", Json::from(*t));
        j
    });
    Json::obj([
        ("seed", Json::from(seed)),
        ("content_hash", Json::from(format!("{:016x}", content.hash))),
        ("commands", Json::arr(cmds)),
    ])
}

/// Parses a journal; returns `(seed, journal)`.
pub fn journal_from_json(text: &str, content: &Content, file: &str) -> Result<(u64, Journal), DataError> {
    let json = Json::parse(text).map_err(|e| DataError::new(file, format!("byte {}", e.offset), e.message))?;
    let root = Obj::new(&json, file, "")?;
    root.only(&["seed", "content_hash", "commands"])?;
    let seed = root.req("seed")?.as_u64().ok_or_else(|| root.err("seed", "expected an integer"))?;
    let mut journal = Vec::new();
    for (i, c) in root.arr("commands")?.iter().enumerate() {
        let o = Obj::new(c, file, format!("commands[{i}]"))?;
        let tick = o.int("tick", 0, i64::from(u32::MAX))? as u32;
        if journal.last().is_some_and(|(t, _)| *t > tick) {
            return Err(o.err("tick", "ticks must not decrease"));
        }
        journal.push((tick, command_from_obj(&o, content, &["tick"])?));
    }
    Ok((seed, journal))
}

/// Replays a journal from a fresh state up to (not including) tick `until`, or until the game
/// ends. `on_tick` sees the state after each step.
pub fn replay(content: Arc<Content>, seed: u64, journal: &Journal, until: u32, mut on_tick: impl FnMut(&State, &[Event])) -> State {
    let mut state = State::new(content, seed);
    let mut k = 0;
    while state.tick() < until && state.outcome() == Outcome::Running {
        let start = k;
        while k < journal.len() && journal[k].0 <= state.tick() {
            k += 1;
        }
        let cmds: Vec<Command> = journal[start..k].iter().map(|(_, c)| *c).collect();
        let ev = state.step(&cmds);
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
}

/// Plays `script` from a fresh state until the game ends, or until `max_waves` waves have been
/// played out (the run stops just before wave `max_waves + 1` would start). A safety cap stops
/// runs that stall (e.g. a boss that can never be killed) 20 minutes after the last wave.
pub fn run(content: Arc<Content>, seed: u64, script: &Script, max_waves: Option<u16>, mut on_tick: impl FnMut(&State, &[Event])) -> RunReport {
    let cap = content.scheduled_ticks() + 20 * 60 * crate::fixed::TICK_HZ;
    let mut state = State::new(content, seed);
    let mut player = Player::new(script.clone());
    let mut journal = Journal::new();
    while state.outcome() == Outcome::Running && state.tick() < cap {
        if let Some(n) = max_waves {
            let w = state.wave();
            let next_due = state.next_wave_tick().is_some_and(|t| state.tick() >= t);
            let cleared = w > 0 && state.wave_stats()[usize::from(w) - 1].cleared_at.is_some();
            if w > n || (w == n && (next_due || cleared)) {
                break;
            }
        }
        let mut ev = Vec::new();
        let tick = state.tick();
        for c in player.drive(&mut state, &mut ev) {
            journal.push((tick, c));
        }
        ev.extend(state.step(&[]));
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
