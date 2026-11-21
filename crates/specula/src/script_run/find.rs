//! `specula find`: the engine looks for a script of inputs that reaches a condition, so that
//! nobody writes a program to play the game for a check that says "there is a script that
//! clears the board" (docs/design/FIRST_HOUR.md, section 15).
//!
//! Both arms of the comparison of 2026-11-21 wrote such a program: the engine's builders a
//! player that read `--follow` (nine requests and 135,000 weighted tokens in one run of the 3D
//! game), the plain builders a `bot.py` and a `search.py` over their own simulation.
//!
//! **How it looks.** The search keeps situations it has reached, each as a save of the whole
//! game ([`crate::save_tools`]: the world, the modules' state and random streams, physics'
//! hidden state, the project shell) with the script that led there. A situation is told from
//! another by a **cell**: the lines of the report that say where the game is, rounded. A try
//! picks a kept situation (one tried less and standing better is picked more often), loads it,
//! and plays on from it a few stretches, an input held through each; whatever it comes to that
//! is new, or is a better way to a cell already known, is kept. The method is Go-Explore's
//! first phase (Ecoffet and others, 2019): return, then explore.
//!
//! **What it is told.** The condition to reach, the inputs it may give, how long an input is
//! held, and, where the game has one, the line that rises as the game goes well (`--rise`):
//! situations with more of it are picked more often. Without that line it has only what is
//! new to go by, which finds what is near.
//!
//! **Nothing is taken on trust.** A script found is played again from the search's first
//! situation, straight through, before it is answered: a load that did not put the game back
//! exactly would show there, as a script that no longer reaches the condition, and that is
//! answered as the fault it is (`not_reproduced`), never as a script. The seed of the search
//! is the run's: the same command finds the same script.

use super::*;
use specula_core::rng::Rng;

/// What a search is asked ([`Spec::find`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Find {
    pub until: Until,
    /// The inputs it may give, a stretch each.
    pub words: Vec<String>,
    /// The report's line that rises as the game goes well.
    pub rise: Option<String>,
    /// The report's lines that tell one situation from another; empty: every line with a
    /// number, a word or a list in it, but the run's own count of ticks.
    pub cells: Vec<String>,
    /// How many ticks an input is held before the next is chosen.
    pub step: u64,
    /// Most stretches played.
    pub tries: u64,
    /// What the game is never to be in on the way (`--never "lives<3"`): a situation where it
    /// holds is not kept, and the search does not play on from it.
    pub never: Option<Until>,
}

/// The ticks an input is held when the command names none.
pub const DEFAULT_STEP: u64 = 15;
/// The stretches a search plays when the command names none.
pub const DEFAULT_FIND_TRIES: u64 = 2000;
/// Most stretches a search plays, and most runs a tuning makes (`--tries`).
pub const MAX_TRIES: u64 = 20_000;
/// Most values of one parameter a run is played for (`--set grip=0.2,0.4,...`).
pub const MAX_SWEEP: usize = 32;
/// Most words a search chooses among.
pub const MAX_WORDS: usize = 16;
/// Most situations kept (each a save in the run's scratch folder).
const MAX_KEPT: usize = 512;
/// Stretches played on from a situation before another is picked.
const STRETCHES: u64 = 8;
/// A number of a cell is rounded to this (half a metre; a score, a count and a tick are whole
/// numbers and stay themselves).
const GRAIN: f64 = 0.5;
/// Most characters of a cell's name.
const MAX_CELL: usize = 600;

impl Find {
    pub fn to_json(&self) -> Json {
        let mut j = Json::obj([
            ("until", Json::from(self.until.text().as_str())),
            ("words", Json::arr(self.words.iter().map(|w| Json::from(w.as_str())))),
            ("step", Json::from(self.step)),
            ("tries", Json::from(self.tries)),
        ]);
        if let Some(rise) = &self.rise {
            j.set("rise", Json::from(rise.as_str()));
        }
        if !self.cells.is_empty() {
            j.set("cells", Json::arr(self.cells.iter().map(|c| Json::from(c.as_str()))));
        }
        if let Some(never) = &self.never {
            j.set("never", Json::from(never.text().as_str()));
        }
        j
    }

    pub fn from_json(j: &Json) -> Result<Find, String> {
        let words = |key: &str| -> Option<Vec<String>> { j.get(key).map_or(Some(Vec::new()), |w| w.as_array().and_then(|w| w.iter().map(|w| w.as_str().map(str::to_owned)).collect())) };
        let find = Find {
            until: Until::parse(j.get("until").and_then(Json::as_str).ok_or("find.until: a condition")?)?,
            words: words("words").ok_or("find.words: a list of inputs")?,
            rise: j.get("rise").and_then(Json::as_str).map(str::to_owned),
            cells: words("cells").ok_or("find.cells: a list of the report's lines")?,
            step: j.get("step").and_then(Json::as_u64).unwrap_or(DEFAULT_STEP),
            tries: j.get("tries").and_then(Json::as_u64).unwrap_or(DEFAULT_FIND_TRIES),
            never: j.get("never").and_then(Json::as_str).map(Until::parse).transpose()?,
        };
        find.check()?;
        Ok(find)
    }

    pub fn check(&self) -> Result<(), String> {
        if self.words.is_empty() || self.words.len() > MAX_WORDS {
            return Err(format!("find.words: 1 to {MAX_WORDS} inputs the search may give"));
        }
        check_inputs(&self.words.iter().map(|w| (0, w.clone())).collect::<Vec<_>>())?;
        check_keys(&self.cells)?;
        if !(1..=3600).contains(&self.step) || !(1..=MAX_TRIES).contains(&self.tries) {
            return Err(format!("find: a step of 1 to 3600 ticks, and 1 to {MAX_TRIES} tries"));
        }
        if self.rise.as_ref().is_some_and(|r| r.is_empty() || r.len() > 128) {
            return Err("find.rise: the name of one of the report's lines".to_owned());
        }
        Ok(())
    }
}

/// A situation the search can go back to.
struct Kept {
    /// The save's name.
    save: String,
    /// The run's tick it stands at.
    tick: u64,
    /// The inputs given since the search began, each at the tick it was given.
    inputs: Vec<(u64, String)>,
    /// What the script holds there ([`Run::held`]).
    held: Vec<(String, String, Json)>,
    rise: f64,
    /// How often it was played on from.
    picked: u64,
}

/// A value as a cell sees it: numbers rounded, lists by their length and their items, text as
/// it is.
fn rounded(v: &Json, out: &mut String) {
    match v {
        Json::Num(n) => {
            let whole = n.fract() == 0.0;
            let cell = if whole { *n } else { (n / GRAIN).floor() };
            out.push_str(&format!("{cell}{}", if whole { "" } else { "~" }));
        }
        Json::Arr(items) => {
            out.push('[');
            for item in items.iter().take(64) {
                rounded(item, out);
                out.push(',');
            }
            out.push_str(&format!("]{}", items.len()));
        }
        Json::Obj(fields) => {
            out.push('{');
            for (key, value) in fields.iter().take(64) {
                out.push_str(key);
                out.push(':');
                rounded(value, out);
                out.push(',');
            }
            out.push('}');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// The cell a report stands in: its lines named by `cells` (all of them but `skip`, the run's
/// own tick, when none is named), rounded.
fn cell_of(report: &Json, cells: &[String], skip: &[String]) -> String {
    let mut out = String::new();
    let lines: Vec<(&str, &Json)> = match report {
        Json::Obj(fields) if cells.is_empty() => fields.iter().filter(|(name, _)| !skip.contains(name)).map(|(name, value)| (name.as_str(), value)).collect(),
        _ => cells.iter().filter_map(|name| Until::read(report, name).map(|value| (name.as_str(), value))).collect(),
    };
    for (name, value) in lines {
        out.push_str(name);
        out.push('=');
        rounded(value, &mut out);
        out.push(';');
        if out.len() > MAX_CELL {
            break;
        }
    }
    // (A cell is a name: a long one is cut at a character's edge, and two long ones that
    // differ only past the cut are one cell.)
    let cut = (0..=MAX_CELL.min(out.len())).rev().find(|i| out.is_char_boundary(*i)).unwrap_or(0);
    out.truncate(cut);
    out
}

/// The inputs of a script as a command line takes them.
fn as_inputs(inputs: &[(u64, String)]) -> String {
    inputs.iter().map(|(tick, word)| format!("{tick}:{word}")).collect::<Vec<_>>().join(",")
}

impl Run<'_> {
    fn keep(&self, save: &str) -> Result<(), Json> {
        self.call("save_write", Json::obj([("name", Json::from(save)), ("overwrite", Json::Bool(true))])).map(drop)
    }

    /// Puts the game back as `kept` saved it, with what the script held there held again.
    fn back_to(&mut self, kept: &Kept) -> Result<(), Json> {
        self.call("save_read", Json::obj([("name", Json::from(kept.save.as_str())), ("force", Json::Bool(true))]))?;
        self.held = kept.held.clone();
        if !self.held.is_empty() {
            self.hold()?;
        }
        Ok(())
    }
}

/// Looks for a script that reaches `find.until`, from where the run's own script (`--inputs`)
/// left the game at `tick`, within `spec.ticks` ticks in all. The answer: `found`, and when it
/// is, `inputs` (the whole script, the run's own first), `ticks` (the tick the condition held
/// at) and `state` (the report there, read from the straight replay).
pub(super) fn find(run: &mut Run, game: &GameFile, spec: &Spec, find: &Find, tick: u64, before: &[(u64, String)]) -> Result<Json, Json> {
    // The run's own count of ticks is no part of where the game is.
    let skip: Vec<String> = game.report.iter().filter(|(_, key)| key == "run.ticks" || key == "game.tick").map(|(name, _)| name.clone()).collect();
    let first = run.stand(game, tick, spec.seed)?.report;
    let mut unread = find.until.unread(&first);
    unread.extend(find.never.iter().flat_map(|never| never.unread(&first)));
    unread.extend(find.rise.iter().chain(&find.cells).filter(|name| Until::read(&first, name).is_none()).cloned());
    if let Some(line) = unread.first() {
        let mut e = refusal("unknown_line", format!("'{line}' is not a line of this game's report, or has no value where the search begins (game.json, \"report\"): a search reads the report, so what it is to reach, what rises and what tells situations apart must be lines of it"));
        e.set("lines", Json::arr(game.report.iter().map(|(name, _)| Json::from(name.as_str()))));
        e.set("state", first);
        return Err(e);
    }
    for word in &find.words {
        run.check_word(word, None)?;
    }
    let rise_of = |report: &Json| find.rise.as_ref().and_then(|name| Until::read(report, name)).and_then(Json::as_f64).unwrap_or(0.0);
    let script = |inputs: &[(u64, String)]| -> Vec<(u64, String)> { before.iter().cloned().chain(inputs.iter().cloned()).collect() };
    // (The sounds asked for are no part of a save: in a search they are every way's, so the
    // line is left out of the state answered. A run of the script says them.)
    let sounds: Vec<String> = game.report.iter().filter(|(_, key)| key == "run.sounds").map(|(name, _)| name.clone()).collect();
    let found = |inputs: &[(u64, String)], at: u64, state: Json, stretches: u64, kept: usize| {
        let all = script(inputs);
        let state = match state {
            Json::Obj(lines) => Json::Obj(lines.into_iter().filter(|(name, _)| !sounds.contains(name)).collect()),
            other => other,
        };
        Json::obj([
            ("found", Json::Bool(true)),
            ("until", Json::from(find.until.text().as_str())),
            ("ticks", Json::from(at)),
            ("inputs", Json::from(as_inputs(&all).as_str())),
            ("stretches", Json::from(stretches)),
            ("situations", Json::from(kept as u64)),
            ("state", state),
        ])
    };
    if find.until.holds(&first) {
        return Ok(found(&[], tick, first, 0, 1));
    }
    if !run.held.is_empty() {
        run.hold()?;
    }
    let held_first = run.held.clone();
    // The situations kept, by cell, in the order of their names (the order a pick walks).
    let mut kept: std::collections::BTreeMap<String, Kept> = std::collections::BTreeMap::new();
    // (The first situation twice: once to go back to for the straight replay, never written
    // over, and once as a situation like any other.)
    run.keep("find-start")?;
    run.keep("find-0")?;
    kept.insert(cell_of(&first, &find.cells, &skip), Kept { save: "find-0".into(), tick, inputs: Vec::new(), held: run.held.clone(), rise: rise_of(&first), picked: 0 });
    let mut saves = 1usize;
    let mut rng = Rng::named(spec.seed.unwrap_or(DEFAULT_SEED), "specula find");
    let mut stretches = 0u64;
    let mut best = (rise_of(&first), tick);
    let mut reached: Option<(Vec<(u64, String)>, u64)> = None;
    'search: while stretches < find.tries {
        // Pick: three times in four among the situations that stand at the most of what
        // rises (a game that has got somewhere is played on from there), else among them all;
        // and among those, one picked less is picked more. The pick is the run's seed's.
        let top = kept.values().map(|k| k.rise).fold(f64::MIN, f64::max);
        let among_top = find.rise.is_some() && rng.chance_ratio(3, 4);
        let weight = |k: &Kept| -> u32 {
            if among_top && k.rise < top {
                return 0;
            }
            (1000.0 / ((k.picked + 1) as f64).sqrt()) as u32 + 1
        };
        let total: u64 = kept.values().map(|k| u64::from(weight(k))).sum();
        let mut roll = (u64::from(rng.next_u32()) * total) >> 32;
        let Some(cell) = kept.iter().find(|(_, k)| {
            let w = u64::from(weight(k));
            if roll < w {
                return true;
            }
            roll -= w;
            false
        }).map(|(cell, _)| cell.clone()) else { break };
        let (mut at, mut inputs) = {
            let from = kept.get_mut(&cell).expect("picked among them");
            from.picked += 1;
            (from.tick, from.inputs.clone())
        };
        if at + find.step > spec.ticks {
            // (Nothing can be played on from the run's last tick. A search whose every
            // situation stands there ends: the weights of the others see to the rest.)
            if kept.values().all(|k| k.tick + find.step > spec.ticks) {
                break;
            }
            continue;
        }
        run.back_to(&kept[&cell])?;
        let mut word = rng.pick(&find.words).cloned().unwrap_or_default();
        for _ in 0..STRETCHES {
            if stretches >= find.tries || at + find.step > spec.ticks {
                break;
            }
            // (An input is kept through the next stretch three times in four: a thing that
            // rolls, walks or flies gets somewhere only when it is pushed one way a while.)
            if rng.chance_ratio(1, 4) {
                word = rng.pick(&find.words).cloned().unwrap_or_default();
            }
            run.input(&word)?;
            inputs.push((at, word.clone()));
            run.play(find.step)?;
            at += find.step;
            stretches += 1;
            let report = run.stand(game, at, spec.seed)?.report;
            if find.until.holds(&report) {
                reached = Some((inputs, at));
                break 'search;
            }
            // A situation the game is never to be in (`--never`: a life lost, a fall) is not
            // kept and not played on from: this way ends here.
            if find.never.as_ref().is_some_and(|never| never.holds(&report)) {
                break;
            }
            let rise = rise_of(&report);
            if rise > best.0 {
                best = (rise, at);
            }
            let here = cell_of(&report, &find.cells, &skip);
            // Kept: a cell not known, or a better way to a known one (more of what rises, or
            // the same sooner). When as many are kept as may be, the worst of them gives way to
            // one that is no worse: the least of what rises, and of those the most played on from.
            let (keep, save, picked) = match kept.get(&here) {
                Some(known) => (rise > known.rise || (rise == known.rise && at < known.tick), known.save.clone(), known.picked),
                None if kept.len() < MAX_KEPT => {
                    saves += 1;
                    (true, format!("find-{}", saves - 1), 0)
                }
                None => {
                    let worst = kept.iter().min_by(|a, b| a.1.rise.total_cmp(&b.1.rise).then(b.1.picked.cmp(&a.1.picked))).map(|(cell, k)| (cell.clone(), k.rise, k.save.clone()));
                    match worst {
                        Some((cell, least, save)) if rise >= least => {
                            kept.remove(&cell);
                            (true, save, 0)
                        }
                        _ => (false, String::new(), 0),
                    }
                }
            };
            if keep {
                run.keep(&save)?;
                kept.insert(here, Kept { save, tick: at, inputs: inputs.clone(), held: run.held.clone(), rise, picked });
            }
        }
    }
    let Some((inputs, at)) = reached else {
        let mut answer = Json::obj([
            ("found", Json::Bool(false)),
            ("until", Json::from(find.until.text().as_str())),
            ("stretches", Json::from(stretches)),
            ("situations", Json::from(kept.len() as u64)),
            ("why", Json::from(if stretches >= find.tries { "the tries ran out" } else { "every situation reached stands at the run's last tick" })),
        ]);
        if find.rise.is_some() {
            answer.set("best", Json::obj([("rise", Json::Num(best.0)), ("at_tick", Json::from(best.1))]));
            // The way to the best it came to, for whoever goes on from there.
            if let Some(k) = kept.values().filter(|k| k.rise >= best.0).min_by_key(|k| k.tick) {
                answer.set("best_inputs", Json::from(as_inputs(&script(&k.inputs)).as_str()));
                answer.set("best_ticks", Json::from(k.tick));
            }
        }
        answer.set("try", Json::from("more --tries; a --rise line that grows as the game goes well; --cells naming the lines that say where the game is (a place, a score); a --step nearer to how long an input takes to matter; or words the game needs that --words does not have"));
        return Ok(answer);
    };
    // Played again from the first situation, straight through: what is answered is what a run
    // of the script does, and a load that did not put the game back exactly shows here.
    run.back_to(&Kept { save: "find-start".into(), tick, inputs: Vec::new(), held: held_first, rise: 0.0, picked: 0 })?;
    let mut now = tick;
    for (when, word) in &inputs {
        run.play(when - now)?;
        now = *when;
        run.input(word)?;
    }
    run.play(at - now)?;
    let state = run.stand(game, at, spec.seed)?.report;
    if !find.until.holds(&state) {
        let mut e = refusal("not_reproduced", "the search reached the condition, and the same inputs played again from the search's start did not: a game put back from a save did not play on as the game that was saved (an engine fault: the script is given so that it can be shown)");
        e.set("inputs", Json::from(as_inputs(&script(&inputs)).as_str()));
        e.set("ticks", Json::from(at));
        e.set("state", state);
        return Err(e);
    }
    let mut answer = found(&inputs, at, state, stretches, kept.len());
    answer.set("replayed", Json::Bool(true));
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_is_the_reports_lines_rounded() {
        let j = |text: &str| Json::parse(text).unwrap();
        let skip = vec!["tick".to_owned()];
        let a = cell_of(&j(r#"{"tick": 30, "score": 10, "ball": {"x": 1.2, "y": 0.25}, "coins": [{"x": 2, "z": 0}], "screen": "play"}"#), &[], &skip);
        // The tick is no part of it; a place within the same half metre is the same cell.
        let b = cell_of(&j(r#"{"tick": 45, "score": 10, "ball": {"x": 1.4, "y": 0.26}, "coins": [{"x": 2, "z": 0}], "screen": "play"}"#), &[], &skip);
        assert_eq!(a, b);
        let moved = cell_of(&j(r#"{"tick": 45, "score": 10, "ball": {"x": 1.6, "y": 0.26}, "coins": [{"x": 2, "z": 0}], "screen": "play"}"#), &[], &skip);
        let scored = cell_of(&j(r#"{"tick": 45, "score": 20, "ball": {"x": 1.4, "y": 0.26}, "coins": [], "screen": "play"}"#), &[], &skip);
        assert!(a != moved && a != scored && moved != scored);
        // Named lines alone, a part of a line among them.
        let named = |text: &str| cell_of(&j(text), &["score".to_owned(), "ball.x".to_owned()], &skip);
        assert_eq!(named(r#"{"score": 10, "ball": {"x": 1.2, "y": 9}, "screen": "a"}"#), named(r#"{"score": 10, "ball": {"x": 1.3, "y": 0}, "screen": "b"}"#));
        assert_eq!(named(r#"{"score": 10, "ball": {"x": 1.2}}"#), "score=10;ball.x=2~;");
        // A whole number and a number rounded to it are told apart (a score of 2 is not a place near 1).
        assert!(named(r#"{"score": 2, "ball": {"x": 0}}"#) != named(r#"{"score": 2.4, "ball": {"x": 0}}"#));
        // A long cell is cut where a character ends.
        let long = cell_of(&j(&format!(r#"{{"texts": "{}"}}"#, "é".repeat(700))), &[], &skip);
        assert!(long.len() <= MAX_CELL);
    }

    #[test]
    fn a_search_is_read_back_as_it_was_written() {
        let find = Find { until: Until::parse("screen==won&&score>=30").unwrap(), words: vec!["left".into(), "right".into(), "none".into()], rise: Some("score".into()), cells: vec!["ball".into()], step: 20, tries: 500, never: Some(Until::parse("lives<3").unwrap()) };
        assert_eq!(Find::from_json(&find.to_json()).unwrap(), find);
        let bare = Find::from_json(&Json::parse(r#"{"until": "won==true", "words": ["tap"]}"#).unwrap()).unwrap();
        assert_eq!((bare.step, bare.tries, bare.rise, bare.cells.len()), (DEFAULT_STEP, DEFAULT_FIND_TRIES, None, 0));
        let bad = |text: &str| Find::from_json(&Json::parse(text).unwrap()).unwrap_err();
        assert!(bad(r#"{"until": "won", "words": ["tap"]}"#).contains("not a condition"));
        assert!(bad(r#"{"until": "won==true", "words": []}"#).contains("find.words"));
        assert!(bad(r#"{"until": "won==true", "words": ["tap"], "step": 0}"#).contains("step"));
        assert_eq!(as_inputs(&[(0, "enter".into()), (15, "right".into())]), "0:enter,15:right");
    }
}
