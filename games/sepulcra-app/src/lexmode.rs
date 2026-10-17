//! `--rules lex`: the game's rules run from Lex (`games/sepulcra/lex/*.lex.json` with the hooks
//! in `sepulcra::lex`), with the hand-written sim kept as the reference.
//!
//! The Lex run is the authority: it decides every player and agent command (a refusal there is
//! the game's refusal) and the outcome. The hand-written state steps in lockstep with the same
//! commands and feeds the presentation (scene, HUD, sounds), and every tick the two are compared
//! (dust, tombstones, wave, outcome, every enemy's and projectile's position and HP, the stones
//! and walls): a difference is a desync, kept with its tick and shown in `play_state`. A
//! scripted build order (`play_new` with a layout) is judged by both and must agree.
//!
//! Recordings stay the hand-written sim's format (the same commands); `play_replay` replays
//! them through Lex too and reports its hash.

use std::sync::Arc;

use sepulcra::lex::Snapshot;
use sepulcra::{Command, Content, Rejection, State};
use specula_core::Json;
use specula_lex::{Rules, Sim};

/// The Lex rules and the run that follows the game.
#[derive(Clone)]
pub struct LexRules {
    rules: Arc<Rules>,
    pub sim: Sim,
    /// The first difference from the reference, and the tick it showed.
    pub desync: Option<(u32, String)>,
    /// Ticks compared so far.
    pub checked: u64,
    /// Commands the Lex run accepted / refused.
    pub accepted: u64,
    pub refused: u64,
}

impl LexRules {
    /// The built-in rule set compiled once per process run (cheap to clone).
    pub fn rules() -> Result<Arc<Rules>, String> {
        sepulcra::lex::compiled(&[])
    }

    /// A Lex run for `content` (which must be the built-in content: the rule set is its Lex form).
    pub fn new(content: &Content, seed: u64) -> Result<LexRules, String> {
        let builtin = Content::builtin();
        if content.hash != builtin.hash {
            return Err(format!(
                "--rules lex plays the built-in rules (lex/sepulcra.lex.json), but this run's content differs (content hash {:016x}, built-in {:016x}); drop 'data', or edit the Lex rule set instead",
                content.hash, builtin.hash
            ));
        }
        let rules = Self::rules()?;
        let sim = sepulcra::lex::sim(Arc::clone(&rules), seed)?;
        Ok(LexRules { rules, sim, desync: None, checked: 0, accepted: 0, refused: 0 })
    }

    /// The Lex run's verdict on a command at its current tick (applied when accepted). `state`
    /// is the reference: if it would judge differently, that is a desync.
    pub fn decide(&mut self, state: &State, cmd: &Command) -> Result<(), Rejection> {
        let lex = sepulcra::lex::command_call(&self.sim, state.content(), cmd).and_then(|c| self.sim.apply(&c));
        let reference = state.check(cmd);
        match (lex, reference) {
            (Ok(()), Ok(())) => {
                self.accepted += 1;
                Ok(())
            }
            (Err(_), Err(r)) => {
                self.refused += 1;
                Err(r)
            }
            (Ok(()), Err(r)) => {
                self.note(state.tick(), format!("Lex accepted {cmd:?}, the reference refused it ({})", r.code()));
                Err(r)
            }
            (Err(l), Ok(())) => {
                self.refused += 1;
                self.note(state.tick(), format!("Lex refused {cmd:?} ({}), the reference would accept it", l.code));
                Err(Rejection::GameOver)
            }
        }
    }

    /// A command the reference already applied (a scripted build order): Lex must take it too.
    pub fn follow(&mut self, state: &State, cmd: &Command) {
        let r = sepulcra::lex::command_call(&self.sim, state.content(), cmd).and_then(|c| self.sim.apply(&c));
        match r {
            Ok(()) => self.accepted += 1,
            Err(e) => self.note(state.tick(), format!("the build order's {cmd:?} was refused by Lex ({}: {})", e.code, e.message)),
        }
    }

    /// One tick, then the comparison with the reference (which has stepped already).
    pub fn step(&mut self, state: &State) {
        self.sim.step(&[]);
        self.check(state);
    }

    pub fn check(&mut self, state: &State) {
        self.checked += 1;
        if self.desync.is_some() {
            return;
        }
        let (a, b) = (Snapshot::of_state(state), Snapshot::of_lex(&self.sim, &state.content().map));
        if let Some(d) = a.diff(&b) {
            self.note(state.tick(), d);
        }
    }

    fn note(&mut self, tick: u32, what: String) {
        if self.desync.is_none() {
            eprintln!("sepulcra: the Lex rules and the reference diverged at tick {tick}: {what}");
            self.desync = Some((tick, what));
        }
    }

    /// The outcome by the Lex rules.
    pub fn outcome(&self) -> Option<&str> {
        self.sim.outcome().map(|(r, _)| r)
    }

    pub fn status_json(&self) -> Json {
        Json::obj([
            ("rules", Json::from("lex")),
            ("set", Json::from(self.rules.name.as_str())),
            ("rules_hash", Json::from(format!("{:016x}", self.rules.hash))),
            ("tick", Json::from(self.sim.tick())),
            ("hash", Json::from(format!("{:016x}", self.sim.hash()))),
            ("outcome", self.outcome().map_or(Json::Null, Json::from)),
            ("accepted", Json::from(self.accepted)),
            ("refused", Json::from(self.refused)),
            ("checked_ticks", Json::from(self.checked)),
            ("parity", Json::from(if self.desync.is_none() { "ok" } else { "desync" })),
            (
                "desync",
                self.desync.as_ref().map_or(Json::Null, |(t, d)| Json::obj([("tick", Json::from(*t)), ("what", Json::from(d.as_str()))])),
            ),
            ("native_calls", Json::arr(self.sim.state().native_calls.iter().map(|n| Json::from(*n)))),
        ])
    }

    /// Replays a journal (the hand-written sim's commands with their ticks) through a fresh Lex
    /// run up to `until` (or its outcome): its tick, hash and outcome there, or the first refusal.
    pub fn replay(content: &Content, seed: u64, journal: &[(u32, Command)], until: u32) -> Result<(u32, u64, Option<String>), String> {
        let l = LexRules::replayed(content, seed, journal, until)?;
        Ok((l.sim.tick(), l.sim.hash(), l.sim.outcome().map(|(r, _)| r.to_owned())))
    }

    /// [`LexRules::replay`], keeping the run (a loaded replay continues from it).
    pub fn replayed(content: &Content, seed: u64, journal: &[(u32, Command)], until: u32) -> Result<LexRules, String> {
        let mut l = LexRules::new(content, seed)?;
        let mut k = 0;
        while l.sim.tick() < until && l.sim.outcome().is_none() {
            let mut now = Vec::new();
            while k < journal.len() && journal[k].0 <= l.sim.tick() {
                let call = sepulcra::lex::command_call(&l.sim, content, &journal[k].1).map_err(|r| format!("tick {}: {:?} refused by Lex ({})", journal[k].0, journal[k].1, r.code))?;
                now.push(call);
                k += 1;
            }
            let refused = l.sim.step(&now);
            if let Some((i, r)) = refused.first() {
                return Err(format!("tick {}: command {i} refused by Lex ({}: {})", l.sim.tick() - 1, r.code, r.message));
            }
            l.accepted += now.len() as u64;
        }
        Ok(l)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sepulcra::script::{run, Script};

    #[test]
    fn the_lex_rules_follow_a_whole_game_and_replay_it() {
        let content = Arc::new(Content::builtin());
        let script = Script::parse(crate::tools::LAYOUTS[0].1, &content, "centre").unwrap();
        let mut player = sepulcra::script::Player::new(script.clone());
        let mut state = State::new(Arc::clone(&content), 5);
        let mut lex = LexRules::new(&content, 5).unwrap();
        let mut ev = Vec::new();
        let mut journal = Vec::new();
        while state.outcome() == sepulcra::Outcome::Running && state.tick() < 40_000 {
            let tick = state.tick();
            for c in player.drive(&mut state, &mut ev) {
                lex.follow(&state, &c);
                journal.push((tick, c));
            }
            state.step_into(&[], &mut ev);
            ev.clear();
            lex.step(&state);
        }
        assert_eq!(lex.desync, None);
        assert_eq!(lex.outcome(), Some("won"));
        let reference = run(Arc::clone(&content), 5, &script, None, |_, _| {});
        assert_eq!(reference.ticks, state.tick());
        let (tick, hash, outcome) = LexRules::replay(&content, 5, &journal, u32::MAX).unwrap();
        assert_eq!((tick, hash, outcome.as_deref()), (lex.sim.tick(), lex.sim.hash(), Some("won")), "the replay reaches the same Lex hash");
    }

    #[test]
    fn lex_decides_commands_and_agrees_with_the_reference() {
        let content = Arc::new(Content::builtin());
        let state = State::new(Arc::clone(&content), 1);
        let mut lex = LexRules::new(&content, 1).unwrap();
        let cell = |s: &str| content.map.cell_ref(s).unwrap();
        let warding = content.stone_kind("warding").unwrap();
        assert!(lex.decide(&state, &Command::PlaceStone { cell: cell("top:9,14"), stone: warding }).is_err(), "a lane is not a build spot");
        assert!(lex.decide(&state, &Command::Upgrade { cell: cell("top:8,12") }).is_err(), "no stone there");
        assert!(lex.decide(&state, &Command::PlaceStone { cell: cell("top:8,12"), stone: warding }).is_ok());
        assert_eq!(lex.desync, None, "{:?}", lex.desync);
        let custom = Content::from_texts(sepulcra::content::SLAB_01_JSON, sepulcra::content::STONES_JSON, sepulcra::content::ENEMIES_JSON, &sepulcra::content::WAVES_JSON.replace("\"start_dust\": 150", "\"start_dust\": 151")).unwrap();
        assert!(LexRules::new(&custom, 1).err().is_some_and(|e| e.contains("content differs")));
    }
}
