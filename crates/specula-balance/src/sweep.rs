//! Sweeps: many param points × many seeds, across threads, with results that don't depend on
//! the thread count or on timing.
//!
//! Each point is compiled once (its params folded into the rules and their hash), then every
//! (point, seed) run is a job. Workers take jobs by index and results are put back by index, so
//! aggregation (means in seed order) is the same whatever ran where (determinism rule 7). A
//! wall-clock deadline stops workers between runs: a point is reported only when all its runs
//! finished, and the caller is told which points to send again.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use specula_core::Json;
use specula_lex::{compile, Diagnostic, Document, Options};

use crate::run::{RunResult, Runner};
use crate::targets::Targets;

/// A param point: param id → value in the param's human unit (a number, or a literal like
/// `"55pt"`), as `lex_simulate`'s `params`.
pub type Point = Vec<(String, Json)>;

/// Most runs one sweep may ask for.
pub const MAX_RUNS: usize = 100_000;
pub const MAX_POINTS: usize = 4096;

#[derive(Clone, Debug)]
pub struct SweepOptions {
    /// Worker threads (at least 1). Results don't depend on it.
    pub threads: usize,
    /// Stop starting runs past this instant.
    pub deadline: Option<Instant>,
}

impl Default for SweepOptions {
    fn default() -> SweepOptions {
        SweepOptions { threads: std::thread::available_parallelism().map_or(1, |n| n.get()), deadline: None }
    }
}

/// One target's numbers at one point.
#[derive(Clone, Debug, PartialEq)]
pub struct TargetStat {
    pub id: String,
    /// Mean over the runs where it was defined (`None`: in none).
    pub mean: Option<f64>,
    pub stdev: f64,
    pub min: f64,
    pub max: f64,
    /// Runs it was defined in, of `runs`.
    pub defined: usize,
    pub target: f64,
    pub tolerance: f64,
    pub hit: bool,
    /// `weight × ((mean − target) / tolerance)²`; an undefined metric costs as much as being
    /// three tolerances off.
    pub loss: f64,
}

impl TargetStat {
    pub fn to_json(&self) -> Json {
        let r = |v: f64| Json::from((v * 10_000.0).round() / 10_000.0);
        Json::obj([
            ("id", Json::from(self.id.as_str())),
            ("mean", self.mean.map_or(Json::Null, r)),
            ("stdev", r(self.stdev)),
            ("min", r(self.min)),
            ("max", r(self.max)),
            ("defined_in", Json::from(self.defined)),
            ("target", Json::from(self.target)),
            ("tolerance", Json::from(self.tolerance)),
            ("hit", Json::from(self.hit)),
            ("loss", r(self.loss)),
        ])
    }
}

/// A point's results (or why it couldn't run).
#[derive(Clone, Debug, PartialEq)]
pub struct PointResult {
    pub point: Point,
    /// The compiled rules' hash (params folded in): names exactly what ran.
    pub rules_hash: Option<u64>,
    pub runs: usize,
    pub stats: Vec<TargetStat>,
    /// Sum of the targets' losses.
    pub loss: f64,
    /// Every target hit.
    pub hit: bool,
    /// Outcome result → runs.
    pub outcomes: Vec<(String, usize)>,
    pub skipped_steps: u32,
    /// Compile or bind problems: the point didn't run.
    pub errors: Vec<Diagnostic>,
}

impl PointResult {
    pub fn to_json(&self, detailed: bool) -> Json {
        let mut f = vec![
            ("params", Json::Obj(self.point.clone())),
            ("rules_hash", self.rules_hash.map_or(Json::Null, |h| Json::from(format!("{h:016x}")))),
            ("runs", Json::from(self.runs)),
            ("loss", Json::from((self.loss * 10_000.0).round() / 10_000.0)),
            ("hit", Json::from(self.hit)),
            ("targets", Json::arr(self.stats.iter().map(TargetStat::to_json))),
            ("outcomes", Json::Obj(self.outcomes.iter().map(|(k, n)| (k.clone(), Json::from(*n))).collect())),
        ];
        if detailed || self.skipped_steps > 0 {
            f.push(("policy_steps_skipped", Json::from(self.skipped_steps)));
        }
        if !self.errors.is_empty() {
            f.push(("errors", Json::arr(self.errors.iter().take(20).map(Diagnostic::to_json))));
        }
        Json::obj(f)
    }
}

/// A sweep's results: complete points in order, and the indices of points not finished.
#[derive(Clone, Debug)]
pub struct SweepResult {
    pub points: Vec<Option<PointResult>>,
    pub unfinished: Vec<usize>,
    pub runs_done: usize,
}

/// Aggregates one point's runs (in seed order) against its targets.
pub fn aggregate(point: Point, rules_hash: u64, targets: &Targets, runs: &[RunResult]) -> PointResult {
    let mut stats = Vec::with_capacity(targets.targets.len());
    let mut loss = 0.0;
    let mut hit = true;
    for (i, t) in targets.targets.iter().enumerate() {
        let vals: Vec<f64> = runs.iter().filter_map(|r| r.values.get(i).copied().flatten()).collect();
        let n = vals.len();
        let mean = (n > 0).then(|| vals.iter().sum::<f64>() / n as f64);
        let (stdev, min, max) = match mean {
            Some(m) if n > 1 => ((vals.iter().map(|v| (v - m) * (v - m)).sum::<f64>() / (n - 1) as f64).sqrt(), vals.iter().copied().fold(f64::MAX, f64::min), vals.iter().copied().fold(f64::MIN, f64::max)),
            Some(m) => (0.0, m, m),
            None => (0.0, 0.0, 0.0),
        };
        let z = mean.map_or(3.0, |m| (m - t.target) / t.tolerance);
        let l = t.weight * z * z;
        let h = mean.is_some_and(|m| (m - t.target).abs() <= t.tolerance);
        hit &= h;
        loss += l;
        stats.push(TargetStat { id: t.id.clone(), mean, stdev, min, max, defined: n, target: t.target, tolerance: t.tolerance, hit: h, loss: l });
    }
    let mut outcomes: Vec<(String, usize)> = Vec::new();
    for r in runs {
        let k = r.outcome.as_ref().map_or("none", |(k, _)| k.as_str()).to_owned();
        match outcomes.iter_mut().find(|(o, _)| *o == k) {
            Some((_, n)) => *n += 1,
            None => outcomes.push((k, 1)),
        }
    }
    outcomes.sort();
    let skipped_steps = runs.iter().map(|r| r.skipped).sum();
    PointResult { point, rules_hash: Some(rules_hash), runs: runs.len(), stats, loss, hit, outcomes, skipped_steps, errors: Vec::new() }
}

/// Compiles `doc` at a point and binds the targets to it.
pub fn prepare(doc: &Document, targets: &Targets, point: &Point) -> Result<Runner, Vec<Diagnostic>> {
    let compiled = compile(doc, &Options { overrides: point.clone(), natives: None })?;
    let rules = Arc::new(compiled.rules);
    // Native hooks run in the engine's live tick, not headless (as lex_simulate).
    if let Err(m) = specula_lex::Sim::new(Arc::clone(&rules), 0, &specula_lex::Natives::new()) {
        return Err(vec![Diagnostic::error(specula_lex::diag::codes::NATIVE_HEADLESS, "", "", m)]);
    }
    let bound = targets.bind(&rules)?;
    Ok(Runner::new(rules, bound))
}

/// Runs every point on every seed (see the module docs).
pub fn sweep(doc: &Document, targets: &Targets, points: &[Point], seeds: &[u64], opts: &SweepOptions) -> SweepResult {
    let prepared: Vec<Result<Runner, Vec<Diagnostic>>> = points.iter().map(|p| prepare(doc, targets, p)).collect();
    let jobs: Vec<(usize, usize)> = (0..points.len()).filter(|i| prepared[*i].is_ok()).flat_map(|i| (0..seeds.len()).map(move |s| (i, s))).collect();
    let results: Vec<Mutex<Option<RunResult>>> = (0..jobs.len()).map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let threads = opts.threads.clamp(1, 64).min(jobs.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                if opts.deadline.is_some_and(|d| Instant::now() > d) {
                    break;
                }
                let j = next.fetch_add(1, Ordering::Relaxed);
                let Some(&(p, s)) = jobs.get(j) else { break };
                let Ok(runner) = &prepared[p] else { continue };
                let r = runner.run(seeds[s], opts.deadline);
                *results[j].lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
            });
        }
    });
    let results: Vec<Option<RunResult>> = results.into_iter().map(|m| m.into_inner().unwrap_or_else(|e| e.into_inner())).collect();
    let mut out: Vec<Option<PointResult>> = Vec::with_capacity(points.len());
    let mut unfinished = Vec::new();
    let mut runs_done = 0;
    let mut k = 0;
    for (i, p) in points.iter().enumerate() {
        match &prepared[i] {
            Err(diags) => out.push(Some(PointResult {
                point: p.clone(),
                rules_hash: None,
                runs: 0,
                stats: Vec::new(),
                loss: f64::INFINITY,
                hit: false,
                outcomes: Vec::new(),
                skipped_steps: 0,
                errors: diags.clone(),
            })),
            Ok(runner) => {
                let mine = &results[k..k + seeds.len()];
                k += seeds.len();
                let done: Vec<RunResult> = mine.iter().flatten().filter(|r| !r.interrupted).cloned().collect();
                runs_done += done.len();
                if done.len() == seeds.len() {
                    out.push(Some(aggregate(p.clone(), runner.rules.hash, targets, &done)));
                } else {
                    unfinished.push(i);
                    out.push(None);
                }
            }
        }
    }
    SweepResult { points: out, unfinished, runs_done }
}
