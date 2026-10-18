//! One headless run: a compiled rule set, a seed, the policy playing; the targets' metrics.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use specula_lex::expr::Ty;
use specula_lex::{Natives, Rules, Sim, Val};

use crate::targets::{Bound, BoundMetric};

/// What one run measured.
#[derive(Clone, Debug, PartialEq)]
pub struct RunResult {
    pub seed: u64,
    /// Which policy played (an index into the targets' policies).
    pub policy: usize,
    /// One value per target (`None`: undefined in this run, e.g. no kills to time).
    pub values: Vec<Option<f64>>,
    /// The outcome's result and the tick it was decided.
    pub outcome: Option<(String, u32)>,
    pub ticks: u32,
    /// Policy steps applied and skipped (refused for a code not in `wait_on`).
    pub applied: u32,
    pub skipped: u32,
    /// When each applied step was applied: (tick, step index), in order. What the policy did,
    /// as plain timed commands (a playtest replays them in a game engine).
    pub trace: Vec<(u32, u16)>,
    /// Every resource at the end, in its human unit (id, value).
    pub resources: Vec<(String, f64)>,
    /// Stopped by the wall clock: not a result (see [`Runner::run`]).
    pub interrupted: bool,
}

/// A rule set with its targets bound: runs any seed.
pub struct Runner {
    pub rules: Arc<Rules>,
    pub bound: Bound,
    spawned: usize,
    died: usize,
    removed: usize,
}

/// A stored value in its human unit (points, metres, seconds...).
fn human(v: i64, ty: Ty, hz: u32) -> f64 {
    match ty {
        Ty::Num(u) => v as f64 / u.per_human(hz) as f64,
        _ => v as f64,
    }
}

impl Runner {
    pub fn new(rules: Arc<Rules>, bound: Bound) -> Runner {
        let event = |name: &str| rules.events.iter().position(|e| e.name == name).unwrap_or(usize::MAX);
        let (spawned, died, removed) = (event("spawned"), event("died"), event("removed"));
        Runner { rules, bound, spawned, died, removed }
    }

    /// Plays one seed with policy `policy` (an index into the targets' policies; out of range
    /// plays nothing). Deterministic: the same rules, targets, policy and seed give the same
    /// result on every machine and thread. A `deadline` stops the run between (or inside) ticks;
    /// it then comes back `interrupted` and must not be counted.
    pub fn run(&self, policy: usize, seed: u64, deadline: Option<Instant>) -> RunResult {
        let b = &self.bound;
        let none = crate::targets::BoundPolicy { name: String::new(), weight: 0.0, every: 1, wait_on: Vec::new(), steps: Vec::new() };
        let plays = b.policies.get(policy).unwrap_or(&none);
        let hz = self.rules.tick_hz;
        let mut sim = match Sim::new(Arc::clone(&self.rules), seed, &Natives::new()) {
            Ok(s) => s,
            Err(_) => return RunResult { seed, policy, values: vec![None; b.metrics.len()], outcome: None, ticks: 0, applied: 0, skipped: 0, trace: Vec::new(), resources: Vec::new(), interrupted: true },
        };
        sim.set_deadline(deadline);
        // Resource samples: (metric index, tick) → value.
        let mut samples: Vec<Option<f64>> = vec![None; b.metrics.len()];
        let sample = |sim: &Sim, samples: &mut Vec<Option<f64>>, now: u32| {
            for (i, m) in b.metrics.iter().enumerate() {
                if let BoundMetric::Resource { index, at: Some(at), unit } = m {
                    if u64::from(now) == *at {
                        let v = sim.state().res[usize::from(*index)];
                        samples[i] = Some(v as f64 / unit.per_human(hz) as f64);
                    }
                }
            }
        };
        // Time to kill: spawn tick and archetype of every entity seen spawning.
        let wants_ttk = b.metrics.iter().any(|m| matches!(m, BoundMetric::TimeToKill { .. }));
        let mut born: BTreeMap<u32, (u32, u16)> = BTreeMap::new();
        let mut ttk: Vec<(f64, u32)> = vec![(0.0, 0); b.metrics.len()];
        let mut next = 0usize;
        let (mut applied, mut skipped) = (0u32, 0u32);
        let mut trace = Vec::new();
        sample(&sim, &mut samples, 0);
        let every = plays.every.max(1);
        let note_events = |sim: &Sim, born: &mut BTreeMap<u32, (u32, u16)>, ttk: &mut Vec<(f64, u32)>| {
            if !wants_ttk {
                return;
            }
            let now = sim.tick();
            for e in sim.events() {
                let Some(id) = e.subject else { continue };
                let k = usize::from(e.kind);
                if k == self.spawned {
                    if let Some(ent) = sim.entity(id) {
                        born.insert(id, (now, ent.arch));
                    }
                } else if k == self.removed {
                    // Gone without dying: no time to kill (and nothing to keep).
                    born.remove(&id);
                } else if k == self.died {
                    if let Some((t0, arch)) = born.remove(&id) {
                        let tags = &self.rules.archetypes[usize::from(arch)].tags;
                        for (i, m) in b.metrics.iter().enumerate() {
                            if let BoundMetric::TimeToKill { arch: a, tag } = m {
                                let fits = a.is_none_or(|a| a == arch) && tag.is_none_or(|t| tags.binary_search(&t).is_ok());
                                if fits {
                                    ttk[i].0 += f64::from(now.saturating_sub(t0)) / f64::from(hz);
                                    ttk[i].1 += 1;
                                }
                            }
                        }
                    }
                }
            }
        };
        while sim.tick() < b.ticks && sim.outcome().is_none() && !sim.interrupted() {
            let now = sim.tick();
            if now % every == 0 {
                // The policy: due steps in order; a wait stops the try, a refusal skips the step.
                while let Some(s) = plays.steps.get(next) {
                    let wave = sim.state().wave;
                    let due = s.at.is_none_or(|a| u64::from(now) >= a)
                        && s.after_wave.is_none_or(|w| wave >= w)
                        && s.when.as_ref().is_none_or(|q| matches!(sim.query(&q.ex), Ok(Val::Bool(true))));
                    if !due {
                        break;
                    }
                    match sim.apply(&s.call) {
                        Ok(()) => {
                            applied += 1;
                            trace.push((now, next as u16));
                            note_events(&sim, &mut born, &mut ttk);
                        }
                        Err(r) if plays.wait_on.contains(&r.code) => break,
                        Err(_) => skipped += 1,
                    }
                    next += 1;
                }
            }
            sim.step(&[]);
            note_events(&sim, &mut born, &mut ttk);
            sample(&sim, &mut samples, sim.tick());
        }
        sim.set_deadline(None);
        let interrupted = sim.interrupted();
        let outcome = sim.outcome().map(|(r, t)| (r.to_owned(), t));
        let (res, stats) = (sim.state().res.clone(), sim.state().wave_stats.clone());
        let stats = &stats;
        let per_wave = |w: Option<u16>, f: &dyn Fn(&specula_lex::sim::WaveStats) -> u32| -> f64 {
            match w {
                Some(w) => stats.get(usize::from(w) - 1).map_or(0.0, |s| f64::from(f(s))),
                None => stats.iter().map(|s| f64::from(f(s))).sum(),
            }
        };
        let mut values = Vec::with_capacity(b.metrics.len());
        for (i, m) in b.metrics.iter().enumerate() {
            let v = match m {
                BoundMetric::Outcome(result) => Some(if outcome.as_ref().is_some_and(|(r, _)| r == result) { 1.0 } else { 0.0 }),
                BoundMetric::WaveCleared(w) => Some(if stats.get(usize::from(*w) - 1).is_some_and(|s| s.cleared_at.is_some()) { 1.0 } else { 0.0 }),
                BoundMetric::WaveClearTime(w) => stats
                    .get(usize::from(*w) - 1)
                    .and_then(|s| Some(f64::from(s.cleared_at?.saturating_sub(s.started_at?)) / f64::from(hz))),
                BoundMetric::Leaks(w) => Some(per_wave(*w, &|s| s.removed)),
                BoundMetric::Kills(w) => Some(per_wave(*w, &|s| s.died)),
                BoundMetric::Resource { index, at, unit } => match at {
                    // Not reached (the run ended first): the value at the end.
                    Some(_) => samples[i].or(Some(res[usize::from(*index)] as f64 / unit.per_human(hz) as f64)),
                    None => Some(res[usize::from(*index)] as f64 / unit.per_human(hz) as f64),
                },
                BoundMetric::TimeToKill { .. } => (ttk[i].1 > 0).then(|| ttk[i].0 / f64::from(ttk[i].1)),
                BoundMetric::EndTime => outcome.as_ref().map(|(_, t)| f64::from(*t) / f64::from(hz)),
                BoundMetric::Expr(q) => match sim.query(&q.ex) {
                    Ok(Val::Int(v)) => Some(human(v, q.ty, hz)),
                    Ok(Val::Bool(b)) => Some(if b { 1.0 } else { 0.0 }),
                    _ => None,
                },
            };
            values.push(v);
        }
        let resources = self.rules.resources.iter().zip(&res).map(|(r, v)| (r.id.clone(), *v as f64 / r.unit.per_human(hz) as f64)).collect();
        RunResult { seed, policy, values, outcome, ticks: sim.tick(), applied, skipped, trace, resources, interrupted }
    }
}
