//! What the floating-point environment guard found and put right (DETERMINISM.md rule 6; the
//! guard itself is `specula_sys::fpenv`).
//!
//! The engine checks the CPU's floating-point control register at startup and as every tick
//! begins (`between_ticks`: rendering, a driver or an overlay DLL changed it since the last tick),
//! and the module host checks it around every module call (`module:<name>`: the module changed
//! it; `before_module_call`: something else did, found just before a call). Each change is
//! restored at once, so the simulation always runs in the default environment; this log is how
//! agents hear about it: `perf_frame` lists it under `problems`, `engine_info` under
//! `determinism`, and `logic_status` per module (`fp_restored`).

use specula_core::Json;
use specula_sys::fpenv::{self, Drift};
use std::collections::{BTreeMap, VecDeque};

/// Recent changes kept.
const RECENT: usize = 16;
/// Sources counted by name; past this many, the rest count as `other`.
const MAX_SOURCES: usize = 64;

/// One change found and restored.
#[derive(Clone, Debug, PartialEq)]
pub struct FpRecord {
    pub tick: u64,
    pub source: String,
    pub drift: Drift,
}

/// See the module docs.
#[derive(Default)]
pub struct FpLog {
    total: u64,
    by_source: BTreeMap<String, u64>,
    recent: VecDeque<FpRecord>,
}

/// A change as data: the register, what was found and expected, and what that means.
pub fn drift_json(d: &Drift) -> Json {
    Json::obj([
        ("register", Json::from(fpenv::REGISTER)),
        ("found", Json::from(format!("{:#x}", d.found))),
        ("expected", Json::from(format!("{:#x}", d.expected()))),
        ("rounding", Json::from(d.rounding().name())),
        ("flush_to_zero", Json::from(d.flush_to_zero())),
        ("denormals_are_zero", Json::from(d.denormals_are_zero())),
        ("exceptions_unmasked", Json::from(d.exceptions_unmasked())),
    ])
}

impl FpLog {
    /// Checks (and restores) the register now; records a change under `source`.
    pub fn check(&mut self, tick: u64, source: &str) {
        if let Some(d) = fpenv::check_and_restore() {
            self.note(tick, source, d);
        }
    }

    /// Records a change already restored. The first one from each source is also printed.
    pub fn note(&mut self, tick: u64, source: &str, drift: Drift) {
        self.total += 1;
        let key = if self.by_source.contains_key(source) || self.by_source.len() < MAX_SOURCES { source } else { "other" };
        let n = self.by_source.entry(key.to_owned()).or_default();
        *n += 1;
        if *n == 1 {
            eprintln!("specula: the floating-point environment was changed ({source}, tick {tick}): {}; restored", drift.describe());
        }
        if self.recent.len() == RECENT {
            self.recent.pop_front();
        }
        self.recent.push_back(FpRecord { tick, source: source.to_owned(), drift });
    }

    /// Changes restored so far.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Changes restored from `source`.
    pub fn count(&self, source: &str) -> u64 {
        self.by_source.get(source).copied().unwrap_or(0)
    }

    /// The newest change from `source`.
    pub fn last(&self, source: &str) -> Option<&FpRecord> {
        self.recent.iter().rev().find(|r| r.source == source)
    }

    /// For engine_info: what is checked, and everything found.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("register", Json::from(fpenv::REGISTER)),
            ("checked", Json::from(fpenv::SUPPORTED)),
            ("expected", Json::from(format!("{:#x}", fpenv::EXPECTED))),
            ("restored", Json::from(self.total)),
            ("by_source", Json::obj(self.by_source.iter().map(|(k, v)| (k.as_str(), Json::from(*v))))),
            ("recent", Json::arr(self.recent.iter().rev().take(4).map(record_json))),
        ])
    }

    /// For perf_frame's `problems`: one item when anything was restored.
    pub fn problem(&self) -> Option<Json> {
        let last = self.recent.back()?;
        Some(Json::obj([
            ("problem", Json::from("fp_env_restored")),
            (
                "message",
                Json::from(format!(
                    "the floating-point control register was changed {} time(s) and restored each time (last: {}, tick {}): \
                     simulation results stay deterministic, but find what changes it",
                    self.total, last.source, last.tick
                )),
            ),
            ("restored", Json::from(self.total)),
            ("by_source", Json::obj(self.by_source.iter().map(|(k, v)| (k.as_str(), Json::from(*v))))),
            ("last", record_json(last)),
        ]))
    }
}

fn record_json(r: &FpRecord) -> Json {
    let mut j = drift_json(&r.drift);
    j.set("tick", Json::from(r.tick));
    j.set("source", Json::from(r.source.as_str()));
    j
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_are_counted_by_source_and_bounded() {
        let mut log = FpLog::default();
        assert!(log.problem().is_none());
        let d = Drift { found: fpenv::EXPECTED ^ 0x6000 };
        for t in 0..40 {
            log.note(t, if t % 2 == 0 { "module:a" } else { "between_ticks" }, d);
        }
        assert_eq!((log.total(), log.count("module:a"), log.count("between_ticks")), (40, 20, 20));
        assert_eq!(log.last("module:a").map(|r| r.tick), Some(38));
        assert_eq!(log.recent.len(), RECENT);
        let p = log.problem().unwrap();
        assert_eq!(p.get("problem").and_then(Json::as_str), Some("fp_env_restored"));
        assert_eq!(p.get("restored").and_then(Json::as_u64), Some(40));
        for i in 0..100 {
            log.note(0, &format!("module:m{i}"), d);
        }
        assert!(log.by_source.len() <= MAX_SOURCES + 1, "bounded, with 'other'");
        assert!(log.count("other") > 0);
    }
}
