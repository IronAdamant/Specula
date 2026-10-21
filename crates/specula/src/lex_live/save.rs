//! Live Lex runs in save games: the `lex_live` section (docs/design/SAVES.md, "Lex").
//!
//! Each live run is saved whole: its Lex state (`Sim::save_state`, snapshot v2), the set's
//! document as it was compiled with the run's params, its seed, `present` rules and input
//! bindings, its anchor on the engine clock (`started`: the clock is restored to the save's
//! tick, so the anchor holds as it is), its credit, work and input counters, a halt, its
//! journal (the newest entries that fit a cap; the count left out is kept), what it presents
//! (each shown entity's Lex id, rule and last position), and its Lex hash. Agents' holds are
//! not saved: sessions aren't, so a holder couldn't come back; a load starts with none, and the
//! save says how many it left out. The window's device state isn't saved either (keys held).
//!
//! A load parses the section on the save reader's worker against the loaded world: it compiles
//! each document with the saved params (the rules' hash must be the one the state was saved
//! with), restores the state, checks the Lex hash, and adopts the presented entities from the
//! loaded world by path (`/lex_live/<set>/e<id>`), never respawning them, so the world hash
//! continues as the saving run's. Everything is bounded and checked, so a damaged or hand-made
//! section is an error, never a panic in a later tick.

use super::*;
use crate::savegame::SaveError;
use specula_core::bytes::{ByteReader, ByteWriter, DecodeError};

/// Bumped when the layout changes.
pub(crate) const SECTION_VERSION: u32 = 1;
/// One run's set document (a set is at most 10,000 items of 16 KB; real ones are far smaller).
const MAX_DOC_BYTES: usize = 32 << 20;
/// One run's journal in a save: the newest commands that fit.
pub(crate) const MAX_JOURNAL_BYTES: usize = 16 << 20;
/// One run's Lex state.
const MAX_STATE_BYTES: usize = 256 << 20;
/// Counters a run adds to stay far from overflow.
const MAX_COUNT: u64 = 1 << 62;

fn dec(e: DecodeError) -> SaveError {
    SaveError::new("corrupt_section", format!("the live Lex runs are damaged: {e}")).in_section("lex_live").at(e.offset)
}

fn bad(r: &ByteReader<'_>, what: String) -> SaveError {
    SaveError::new("corrupt_section", format!("the live Lex runs don't fit: {what}")).in_section("lex_live").at(r.offset())
}

/// A parsed section, ready for [`Live::apply_state`] (built on the save reader's worker).
pub(crate) struct LiveSnapshot {
    runs: Vec<(String, Run)>,
    next_first: usize,
    notes: Vec<Json>,
}

fn edge_byte(e: Edge) -> u8 {
    match e {
        Edge::Pressed => 0,
        Edge::Released => 1,
        Edge::Held => 2,
    }
}

fn edge_name(b: u8) -> Option<&'static str> {
    match b {
        0 => Some("pressed"),
        1 => Some("released"),
        2 => Some("held"),
        _ => None,
    }
}

impl Live {
    /// Agents' holds a save leaves out (they end with their sessions).
    pub(crate) fn holds_count(&self) -> usize {
        self.runs.values().filter_map(|r| r.input.as_ref()).map(|i| i.holders.len()).sum()
    }

    /// The `lex_live` section: `None` with no live run; an error when a run can't be saved (it
    /// was started without its document: only tests start runs so).
    pub(crate) fn save_state(&self) -> Result<Option<Vec<u8>>, SaveError> {
        if self.runs.is_empty() {
            return Ok(None);
        }
        let mut w = ByteWriter::new();
        w.u32(SECTION_VERSION);
        w.u64(self.next_first as u64);
        w.len_u32(self.runs.len());
        for (set, run) in &self.runs {
            let Some(doc) = &run.doc else {
                return Err(SaveError::new("live_lex_unsaveable", format!("the live run '{set}' has no source document to save; stop it and start it with lex_live")));
            };
            w.str(set);
            let doc = doc.to_string();
            if doc.len() > MAX_DOC_BYTES {
                return Err(SaveError::new("too_large", format!("set '{set}' is {} bytes; saves hold a live run's set up to {MAX_DOC_BYTES}", doc.len())));
            }
            w.str(&doc);
            w.u64(run.seed);
            w.str(&run.params.to_string());
            w.u64(run.started);
            w.len_u32(run.present_src.len());
            for p in &run.present_src {
                w.len_u32(p.tags.len());
                p.tags.iter().for_each(|t| w.str(t));
                w.str(&p.spawn.to_string());
            }
            match &run.input {
                Some(i) => {
                    w.bool(true);
                    w.str(&i.spec.actions);
                    w.len_u32(i.spec.bindings.len());
                    for b in &i.spec.bindings {
                        w.str(&b.action);
                        w.u8(edge_byte(b.on));
                        w.str(&b.command);
                        w.str(&b.args.to_string());
                    }
                }
                None => w.bool(false),
            }
            w.i64(run.credit);
            w.u64(run.last_work);
            w.u64(run.spent);
            w.u64(run.behind);
            match &run.halted {
                Some(h) => {
                    w.bool(true);
                    w.str(&h.to_string());
                }
                None => w.bool(false),
            }
            for v in [run.input_journaled as u64, run.input_dropped, run.hook_calls, run.hook_failures] {
                w.u64(v);
            }
            // The newest journal entries that fit; the rest are counted.
            let mut bytes = 0;
            let mut keep = 0;
            for (_, n, a, by) in run.journal.iter().rev() {
                let size = 16 + n.len() + a.to_string().len() + by.len();
                if bytes + size > MAX_JOURNAL_BYTES {
                    break;
                }
                bytes += size;
                keep += 1;
            }
            let first = run.journal.len() - keep;
            w.u64((run.journal_dropped + first) as u64);
            w.len_u32(keep);
            for (t, n, a, by) in &run.journal[first..] {
                w.u32(*t);
                w.str(n);
                w.str(&a.to_string());
                w.str(by);
            }
            w.opt_u32(run.presented);
            w.u64(run.hidden as u64);
            w.len_u32(run.shown.len());
            for (lid, s) in &run.shown {
                w.u32(*lid);
                w.u32(s.rule as u32);
                s.pos.iter().for_each(|c| w.i64(*c));
            }
            let state = run.sim.save_state();
            if state.len() > MAX_STATE_BYTES {
                return Err(SaveError::new("too_large", format!("the live run '{set}''s state is {} bytes; saves hold up to {MAX_STATE_BYTES}", state.len())));
            }
            w.blob(&state);
            w.u64(run.sim.hash());
        }
        Ok(Some(w.into_bytes()))
    }

    /// Takes a parsed section (the engine thread, at the load's swap): the runs live before
    /// the load stop (the world they presented into is gone); the engine's own limits stay.
    pub(crate) fn apply_state(&mut self, snap: Option<LiveSnapshot>) -> Vec<Json> {
        let mut notes = Vec::new();
        let before: Vec<String> = self.runs.keys().cloned().collect();
        self.runs.clear();
        self.next_first = 0;
        if !before.is_empty() {
            notes.push(Json::obj([
                ("note", Json::from("the live Lex runs running before the load stopped (the world they presented into was replaced)")),
                ("stopped", Json::arr(before.iter().map(|s| Json::from(s.as_str())))),
            ]));
        }
        if let Some(s) = snap {
            self.next_first = s.next_first;
            self.runs = s.runs.into_iter().collect();
            notes.extend(s.notes);
        }
        notes
    }
}

/// Parses the `lex_live` section against the loaded `world` and the save's `tick` (the save
/// reader's worker), with the hosted game's in-process hooks.
pub(crate) fn parse_state(bytes: &[u8], world: &World, tick: u64, natives: &specula_lex::Natives) -> Result<LiveSnapshot, SaveError> {
    let mut r = ByteReader::new(bytes);
    let d = dec;
    let version = r.u32("version").map_err(d)?;
    if version != SECTION_VERSION {
        return Err(SaveError::new("unsupported_version", format!("live Lex section version {version}; this build reads {SECTION_VERSION}")).in_section("lex_live"));
    }
    let next_first = r.u64("next_first").map_err(d)?.min(MAX_RUNS as u64) as usize;
    let n = r.count("runs", MAX_RUNS, 64).map_err(d)?;
    let mut runs: Vec<(String, Run)> = Vec::with_capacity(n);
    let mut notes = Vec::new();
    for _ in 0..n {
        let set = r.str("set", 64).map_err(d)?;
        crate::lex_tools::set_arg(&Json::obj([("set", Json::from(set.as_str()))]), true).map_err(|m| bad(&r, m))?;
        if runs.iter().any(|(s, _)| *s == set) {
            return Err(bad(&r, format!("set '{set}' twice")));
        }
        let doc_text = r.str("doc", MAX_DOC_BYTES).map_err(d)?;
        let doc_json = Json::parse(&doc_text).map_err(|e| bad(&r, format!("set '{set}''s document: {e}")))?;
        let doc = specula_lex::Document::from_json(&doc_json).map_err(|ds| bad(&r, format!("set '{set}''s document: {}", ds.first().map_or(String::new(), |x| x.message.clone()))))?;
        let seed = r.u64("seed").map_err(d)?;
        let params = Json::parse(&r.str("params", 1 << 20).map_err(d)?).map_err(|e| bad(&r, format!("params: {e}")))?;
        let overrides = crate::lex_tools::params_arg(&Json::obj([("params", params.clone())])).map_err(|m| bad(&r, m))?;
        let opts = specula_lex::Options { overrides, ..Default::default() };
        let rules = match specula_lex::compile(&doc, &opts) {
            Ok(c) => Arc::new(c.rules),
            Err(ds) => return Err(bad(&r, format!("set '{set}' doesn't compile: {}", ds.first().map_or(String::new(), |x| x.message.clone())))),
        };
        let started = r.u64("started").map_err(d)?;
        if started > tick {
            return Err(bad(&r, format!("set '{set}' started at tick {started}, after the save's tick {tick}")));
        }
        // present: checked as `start` checks it.
        let k = r.count("present", MAX_PRESENT, 8).map_err(d)?;
        let mut present_json = Vec::with_capacity(k);
        for _ in 0..k {
            let t = r.count("tags", 16, 4).map_err(d)?;
            let tags: Vec<Json> = (0..t).map(|_| r.str("tag", 256).map(Json::from).map_err(d)).collect::<Result<_, _>>()?;
            let spawn = Json::parse(&r.str("spawn", 64 * 1024).map_err(d)?).map_err(|e| bad(&r, format!("present spawn: {e}")))?;
            present_json.push(Json::obj([("tags", Json::Arr(tags)), ("spawn", spawn)]));
        }
        let present_src = present_arg(&Json::obj([("present", Json::Arr(present_json))])).map_err(|m| bad(&r, m))?;
        let present: Vec<(TagFilter, Json)> = present_src.iter().map(|p| (filter_of(&rules, &p.tags), p.spawn.clone())).collect();
        // input: checked as `start` checks it (actions and commands must exist).
        let input = if r.bool("input").map_err(d)? {
            let actions = r.str("input.actions", 64 * 1024).map_err(d)?;
            let actions = Json::parse(&actions).map_err(|e| bad(&r, format!("input.actions: {e}")))?;
            let k = r.count("bindings", MAX_BINDINGS, 8).map_err(d)?;
            let mut bindings = Vec::with_capacity(k);
            for _ in 0..k {
                let action = r.str("binding.action", 256).map_err(d)?;
                let on = edge_name(r.u8("binding.on").map_err(d)?).ok_or_else(|| bad(&r, "a binding's edge".into()))?;
                let command = r.str("binding.command", 256).map_err(d)?;
                let args = Json::parse(&r.str("binding.args", MAX_ARGS_BYTES).map_err(d)?).map_err(|e| bad(&r, format!("binding args: {e}")))?;
                bindings.push(Json::obj([("action", Json::from(action)), ("on", Json::from(on)), ("command", Json::from(command)), ("args", args)]));
            }
            let spec = input_arg(&Json::obj([("input", Json::obj([("actions", actions), ("bindings", Json::Arr(bindings))]))]), &rules).map_err(|m| bad(&r, m))?;
            match spec {
                Some(spec) => Some(LiveInput::new(&spec).map_err(|e| bad(&r, format!("input.actions: {e}")))?),
                None => None,
            }
        } else {
            None
        };
        let credit = r.i64("credit").map_err(d)?;
        let (last_work, spent, behind) = (r.u64("last_work").map_err(d)?, r.u64("spent").map_err(d)?, r.u64("behind").map_err(d)?);
        if credit.unsigned_abs() > MAX_COUNT || [last_work, spent, behind].iter().any(|v| *v > MAX_COUNT) {
            return Err(bad(&r, format!("set '{set}''s credit or work is out of range")));
        }
        let halted = if r.bool("halted").map_err(d)? {
            let h = Json::parse(&r.str("halted", 8 * 1024).map_err(d)?).map_err(|e| bad(&r, format!("halted: {e}")))?;
            if !matches!(h, Json::Obj(_)) {
                return Err(bad(&r, "a halt must be an object".into()));
            }
            Some(h)
        } else {
            None
        };
        let v: Vec<u64> = (0..4).map(|_| r.u64("counter").map_err(d)).collect::<Result<_, _>>()?;
        if v.iter().any(|x| *x > MAX_COUNT) || v[0] > MAX_INPUT_JOURNAL as u64 {
            return Err(bad(&r, format!("set '{set}''s counters are out of range")));
        }
        let journal_dropped = r.u64("journal_dropped").map_err(d)?;
        let k = r.count("journal", MAX_JOURNAL, 13).map_err(d)?;
        if journal_dropped > (MAX_JOURNAL - k) as u64 {
            return Err(bad(&r, format!("set '{set}''s journal is past {MAX_JOURNAL} commands")));
        }
        let mut journal = Vec::with_capacity(k);
        let mut last = 0;
        for _ in 0..k {
            let t = r.u32("journal.tick").map_err(d)?;
            let name = r.str("journal.command", 256).map_err(d)?;
            let args = Json::parse(&r.str("journal.args", MAX_ARGS_BYTES).map_err(d)?).map_err(|e| bad(&r, format!("journal args: {e}")))?;
            let by = r.str("journal.by", 256).map_err(d)?;
            if t < last {
                return Err(bad(&r, "journal ticks out of order".into()));
            }
            last = t;
            journal.push((t, name, args, by));
        }
        let presented = r.opt_u32("presented").map_err(d)?;
        let hidden = r.u64("hidden").map_err(d)?.min(MAX_ENTITIES_SHOWN_OR_HIDDEN) as usize;
        let k = r.count("shown", MAX_SHOWN, 32).map_err(d)?;
        let mut shown_raw = Vec::with_capacity(k);
        for _ in 0..k {
            let lid = r.u32("shown.id").map_err(d)?;
            let rule = r.u32("shown.rule").map_err(d)? as usize;
            let pos = [r.i64("pos").map_err(d)?, r.i64("pos").map_err(d)?, r.i64("pos").map_err(d)?];
            if rule >= present.len() {
                return Err(bad(&r, format!("a shown entity's present rule {rule} (the run has {})", present.len())));
            }
            shown_raw.push((lid, rule, pos));
        }
        let state = r.blob("state", MAX_STATE_BYTES).map_err(d)?;
        let saved_hash = r.u64("lex hash").map_err(d)?;
        let sim = specula_lex::Sim::restore_hosted(Arc::clone(&rules), natives, state).map_err(|e| {
            SaveError::new(if e.code == "rules_changed" { "lex_rules_changed" } else { "corrupt_section" }, format!("set '{set}''s Lex state: {e}")).in_section("lex_live")
        })?;
        if sim.hash() != saved_hash {
            return Err(SaveError::new("save_mismatch", format!("set '{set}''s restored Lex state hashes {:016x}, saved {saved_hash:016x}", sim.hash())).in_section("lex_live"));
        }
        if journal.last().is_some_and(|j| j.0 > sim.tick()) || presented.is_some_and(|p| p > sim.tick()) {
            return Err(bad(&r, format!("set '{set}''s journal or presentation is past its Lex tick {}", sim.tick())));
        }
        // Adopt what the run presents from the loaded world (by path), never respawning.
        let group = world.find_path(&format!("/{ROOT}/{set}")).unwrap_or(u64::MAX);
        let mut shown = BTreeMap::new();
        let mut missing = 0;
        for (lid, rule, pos) in shown_raw {
            match world.find_path(&format!("/{ROOT}/{set}/e{lid}")) {
                Ok(id) if sim.entity(lid).is_some() => {
                    shown.insert(lid, Shown { id, rule, pos });
                }
                _ => missing += 1,
            }
        }
        if missing > 0 {
            notes.push(Json::obj([
                ("warning", Json::from("live_lex_presentation")),
                ("set", Json::from(set.as_str())),
                ("note", Json::from(format!("{missing} presented entities weren't in the saved world; the run shows them again"))),
            ]));
        }
        let run = Run {
            sim,
            agent: format!("lex:{set}"),
            seed,
            params,
            started,
            present,
            present_src,
            input,
            group,
            shown,
            presented,
            hidden,
            present_error: None,
            behind,
            last_work,
            spent,
            credit,
            input_journaled: v[0] as usize,
            input_dropped: v[1],
            halted,
            journal,
            recent: VecDeque::new(),
            hook_calls: v[2],
            hook_failures: v[3],
            doc: Some(Arc::new(doc_json)),
            journal_dropped: journal_dropped as usize,
            args: Json::Null,
        };
        runs.push((set, run));
    }
    r.finish().map_err(d)?;
    Ok(LiveSnapshot { runs, next_first, notes })
}

/// Hidden counts are reports; they stay in a sane range.
const MAX_ENTITIES_SHOWN_OR_HIDDEN: u64 = 1 << 24;
