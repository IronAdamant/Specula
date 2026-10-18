//! `--rules lex`: the game's rules run from Lex (`games/sepulcra/lex/*.lex.json` with the hooks
//! in `sepulcra::lex`), with the hand-written sim kept as the reference.
//!
//! The Lex run is the authority: it decides every player and agent command (a refusal there is
//! the game's refusal, with Lex's own code and message under `lex` in the answer) and the outcome
//! (the victory and defeat screens, and when the run stops stepping, follow the Lex run). The
//! hand-written state steps in lockstep with the same commands and feeds the presentation
//! (scene, HUD, sounds), and every tick the two are compared (dust, tombstones, wave, outcome,
//! every enemy's and projectile's position and HP, the stones and walls). Any difference, or a
//! command the two judge differently, is a **desync**: the game stops there (neither steps
//! again), says so on screen, and `play_state.rules` has the tick and what differed. A command
//! Lex accepts but the reference refuses is taken back out of the Lex run, so the two never drift
//! apart silently. A scripted build order (`play_new` with a layout) is judged by both and must
//! agree.
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
    /// Lex's refusal of the last command decided (its code and message).
    pub last_refusal: Option<(String, String)>,
}

/// The game's id of the Lex run's enemy `lex_id`: the enemies of both runs, in id order, are
/// the same enemies (the parity check compares them so), so it is the one at the same rank.
fn enemy_of(sim: &Sim, state: &State, lex_id: i64) -> Option<sepulcra::sim::EnemyId> {
    let r = sim.rules();
    let enemy = r.tag("enemy")?;
    let rank = sim.entities().iter().filter(|e| e.alive && r.archetypes[usize::from(e.arch)].tags.binary_search(&enemy).is_ok()).position(|e| i64::from(e.id) == lex_id)?;
    state.enemies().get(rank).map(|e| e.id)
}

/// A Lex refusal as the game's [`Rejection`] (Lex's codes are the game's for the rules it
/// states; the generic ones map by the command), with Lex's own details: `need` and `have`
/// from the rule's cost or requirement, the enemy in the way from the hook (as the game's id).
fn rejection_of(l: &specula_lex::Rejection, cmd: &Command, sim: &Sim, state: &State) -> Rejection {
    let detail = |k: &str| l.details.iter().find(|(n, _)| n == k).and_then(|(_, v)| v.as_i64());
    let walls = matches!(cmd, Command::PlaceWall { .. } | Command::RemoveWall { .. });
    match l.code.as_str() {
        "unknown_cell" | "unknown_site" => Rejection::UnknownCell,
        "unknown_stone" | "unknown_archetype" => Rejection::UnknownStone,
        "not_buildable" => Rejection::NotBuildable,
        "not_wallable" => Rejection::NotWallable,
        "wrong_site" if walls => Rejection::NotWallable,
        "wrong_site" => Rejection::NotBuildable,
        "occupied" => Rejection::Occupied,
        "no_stone" => Rejection::NoStone,
        "no_wall" => Rejection::NoWall,
        "no_entity" if walls => Rejection::NoWall,
        "no_entity" => Rejection::NoStone,
        "max_tier" => Rejection::MaxTier,
        "not_enough_dust" => {
            let clamp = |v: i64| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
            Rejection::NotEnoughDust { need: detail("need").map_or(0, clamp), have: detail("have").map_or(state.dust(), clamp) }
        }
        "wall_exists" => Rejection::WallExists,
        "enemy_in_the_way" => Rejection::EnemyInTheWay { enemy: detail("enemy").and_then(|id| enemy_of(sim, state, id)).unwrap_or(0) },
        "no_more_waves" => Rejection::NoMoreWaves,
        // game_over, and Lex's own failures (work_limit, runtime_error): nothing the player can
        // fix by choosing differently; `lex` in the answer has the real code.
        _ => Rejection::GameOver,
    }
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
        Ok(LexRules { rules, sim, desync: None, checked: 0, accepted: 0, refused: 0, last_refusal: None })
    }

    /// The Lex run's verdict on a command at its current tick (applied when accepted). `state`
    /// is the reference, asked first: if it judges differently, that is a desync (and a command
    /// the reference can't take is taken back out of the Lex run).
    pub fn decide(&mut self, state: &State, cmd: &Command) -> Result<(), Rejection> {
        self.last_refusal = None;
        if let Some((t, d)) = &self.desync {
            self.last_refusal = Some(("desync".into(), format!("the rules diverged at tick {t} ({d}); the run is stopped")));
            return Err(Rejection::GameOver);
        }
        let reference = state.check(cmd);
        // Kept only when the reference refuses (Lex may accept, and must then be undone).
        let before = reference.is_err().then(|| self.sim.clone());
        let lex = sepulcra::lex::command_call(&self.sim, state.content(), cmd).and_then(|c| self.sim.apply(&c));
        match (lex, reference) {
            (Ok(()), Ok(())) => {
                self.accepted += 1;
                Ok(())
            }
            (Err(l), Err(_)) => {
                self.refused += 1;
                let mine = rejection_of(&l, cmd, &self.sim, state);
                self.last_refusal = Some((l.code, l.message));
                Err(mine)
            }
            (Ok(()), Err(r)) => {
                if let Some(b) = before {
                    self.sim = b;
                }
                self.note(state.tick(), format!("Lex accepted {cmd:?}, the reference refused it ({})", r.code()));
                self.last_refusal = Some(("desync".into(), "Lex accepted the command but the reference refused it; the run is stopped".into()));
                Err(r)
            }
            (Err(l), Ok(())) => {
                self.refused += 1;
                self.note(state.tick(), format!("Lex refused {cmd:?} ({}), the reference would accept it", l.code));
                let mine = rejection_of(&l, cmd, &self.sim, state);
                self.last_refusal = Some((l.code, l.message));
                Err(mine)
            }
        }
    }

    /// The game's outcome by the Lex rules.
    pub fn result(&self) -> sepulcra::Outcome {
        match self.outcome() {
            Some("won") => sepulcra::Outcome::Won,
            Some(_) => sepulcra::Outcome::Lost,
            None => sepulcra::Outcome::Running,
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

    /// Sepulcra's rule set, run live (`lex_live`), keeps pace with the engine at the default
    /// live limits: every Lex tick of a whole game (the work it does, plus one per entity) is
    /// replayed through the live credit model at 30 and 60 engine ticks a second.
    #[test]
    fn sepulcra_keeps_pace_live_at_the_default_limits() {
        let limits = specula::LEX_LIVE_LIMITS;
        let content = Arc::new(Content::builtin());
        let script = Script::parse(crate::tools::LAYOUTS[0].1, &content, "centre").unwrap();
        let mut player = sepulcra::script::Player::new(script);
        let mut state = State::new(Arc::clone(&content), 5);
        let mut lex = LexRules::new(&content, 5).unwrap();
        let mut ev = Vec::new();
        let mut costs = Vec::new();
        while state.outcome() == sepulcra::Outcome::Running && state.tick() < 40_000 {
            for c in player.drive(&mut state, &mut ev) {
                lex.follow(&state, &c);
            }
            state.step_into(&[], &mut ev);
            ev.clear();
            lex.step(&state);
            costs.push(lex.sim.tick_work() + lex.sim.entities().len() as u64 + 1);
        }
        assert_eq!(lex.outcome(), Some("won"));
        let max = *costs.iter().max().unwrap();
        let mean = costs.iter().sum::<u64>() / costs.len() as u64;
        println!("Sepulcra live cost per Lex tick: mean {mean}, max {max} (allowance {} per engine tick, step cap {})", limits.work_per_tick, limits.step_work);
        assert!(max < limits.step_work, "no Lex tick of Sepulcra comes near the step cap: {max}");
        for engine_hz in [30u64, 60] {
            // The credit model of lex_live's tick (one run).
            let share = limits.work_per_tick as i64;
            let (mut credit, mut done, mut worst_behind) = (0i64, 0usize, 0u64);
            let mut tick = 0u64;
            while done < costs.len() {
                credit = (credit + share).min(share);
                let target = ((tick + 1) * 30 / engine_hz) as usize;
                while done < target.min(costs.len()) && credit > 0 {
                    credit -= costs[done] as i64;
                    done += 1;
                }
                worst_behind = worst_behind.max(target.min(costs.len()).saturating_sub(done) as u64);
                tick += 1;
            }
            assert!(worst_behind <= 2, "at {engine_hz} Hz the live run falls {worst_behind} Lex ticks behind");
        }
    }

    /// Lex mode's refusals carry Lex's own details: the dust an upgrade needs and has, and the
    /// enemy in a wall's way (as the game's id), equal to what the reference would say.
    #[test]
    fn lex_refusals_carry_need_have_and_the_enemy() {
        let content = Arc::new(Content::builtin());
        let mut state = State::new(Arc::clone(&content), 3);
        let mut lex = LexRules::new(&content, 3).unwrap();
        let cell = |s: &str| content.map.cell_ref(s).unwrap();
        let warding = content.stone_kind("warding").unwrap();
        // Spend dust on stones, then ask for an upgrade it can't pay for.
        let mut ev = Vec::new();
        for c in ["top:8,12", "top:12,8", "top:7,12", "top:12,11", "top:7,7", "top:11,7"] {
            let cmd = Command::PlaceStone { cell: cell(c), stone: warding };
            if lex.decide(&state, &cmd).is_ok() {
                state.apply(&cmd, &mut ev).unwrap();
            }
        }
        let up = Command::Upgrade { cell: cell("top:8,12") };
        let reference = state.check(&up);
        assert!(matches!(reference, Err(Rejection::NotEnoughDust { .. })), "{reference:?}");
        assert_eq!(lex.decide(&state, &up), reference, "need and have from Lex");
        assert_eq!(lex.desync, None);
        // Walk until an enemy stands in a wall's way (a fresh run: dust for a wall, since Lex
        // checks costs before the hook and the reference checks the enemy first).
        let mut state = State::new(Arc::clone(&content), 3);
        let mut lex = LexRules::new(&content, 3).unwrap();
        let mut found = false;
        'ticks: for _ in 0..6000 {
            state.step_into(&[], &mut ev);
            ev.clear();
            lex.step(&state);
            for c in content.map.cells() {
                let wall = Command::PlaceWall { cell: c.id };
                if let r @ Err(Rejection::EnemyInTheWay { .. }) = state.check(&wall) {
                    assert_eq!(lex.decide(&state, &wall), r, "the same enemy, by the game's id");
                    found = true;
                    break 'ticks;
                }
            }
        }
        assert!(found, "an enemy got in a wall's way");
        assert_eq!(lex.desync, None, "{:?}", lex.desync);
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
