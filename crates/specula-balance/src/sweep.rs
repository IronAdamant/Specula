//! Sweeps: many param points × many seeds, across threads, with results that don't depend on
//! the thread count or on timing.
//!
//! Each point is compiled once (its params folded into the rules and their hash), then every
//! (point, policy, seed) run is a job. Workers take jobs by index and results are put back by index, so
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

/// One policy's own numbers at a point (a play style: see the targets' `policies`).
#[derive(Clone, Debug, PartialEq)]
pub struct PolicyResult {
    pub name: String,
    pub weight: f64,
    /// The targets that apply to it, over its runs.
    pub stats: Vec<TargetStat>,
    /// Its targets' losses (unweighted by the policy's weight).
    pub loss: f64,
    pub hit: bool,
    pub outcomes: Vec<(String, usize)>,
    pub skipped_steps: u32,
}

impl PolicyResult {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("policy", Json::from(self.name.as_str())),
            ("weight", Json::from(self.weight)),
            ("loss", Json::from((self.loss * 10_000.0).round() / 10_000.0)),
            ("hit", Json::from(self.hit)),
            ("targets", Json::arr(self.stats.iter().map(TargetStat::to_json))),
            ("outcomes", Json::Obj(self.outcomes.iter().map(|(k, n)| (k.clone(), Json::from(*n))).collect())),
            ("policy_steps_skipped", Json::from(self.skipped_steps)),
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
    /// Each policy's own numbers (with several policies; one policy's are the point's).
    pub policies: Vec<PolicyResult>,
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
        if self.policies.len() > 1 {
            f.push(("policies", Json::arr(self.policies.iter().map(PolicyResult::to_json))));
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

/// One target's numbers over some runs' values (unweighted loss).
fn stat(t: &crate::targets::Target, vals: &[f64]) -> TargetStat {
    let n = vals.len();
    let mean = (n > 0).then(|| vals.iter().sum::<f64>() / n as f64);
    let (stdev, min, max) = match mean {
        Some(m) if n > 1 => ((vals.iter().map(|v| (v - m) * (v - m)).sum::<f64>() / (n - 1) as f64).sqrt(), vals.iter().copied().fold(f64::MAX, f64::min), vals.iter().copied().fold(f64::MIN, f64::max)),
        Some(m) => (0.0, m, m),
        None => (0.0, 0.0, 0.0),
    };
    let z = mean.map_or(3.0, |m| (m - t.target) / t.tolerance);
    let hit = mean.is_some_and(|m| (m - t.target).abs() <= t.tolerance);
    TargetStat { id: t.id.clone(), mean, stdev, min, max, defined: n, target: t.target, tolerance: t.tolerance, hit, loss: t.weight * z * z }
}

fn outcomes_of<'a>(runs: impl Iterator<Item = &'a RunResult>) -> Vec<(String, usize)> {
    let mut outcomes: Vec<(String, usize)> = Vec::new();
    for r in runs {
        let k = r.outcome.as_ref().map_or("none", |(k, _)| k.as_str()).to_owned();
        match outcomes.iter_mut().find(|(o, _)| *o == k) {
            Some((_, n)) => *n += 1,
            None => outcomes.push((k, 1)),
        }
    }
    outcomes.sort();
    outcomes
}

/// Aggregates one point's runs (each policy's in seed order) against its targets.
///
/// With several policies, each target is scored under every policy it applies to: it **hits**
/// only when it hits under each, its loss is the policies' weighted sum, and its mean, stdev,
/// min and max pool those runs. Each policy's own numbers are in [`PointResult::policies`].
/// With one policy (weight 1) this is the plain mean over the seeds.
pub fn aggregate(point: Point, rules_hash: u64, targets: &Targets, runs: &[RunResult]) -> PointResult {
    let mut per_policy = Vec::with_capacity(targets.policies.len());
    for (p, pol) in targets.policies.iter().enumerate() {
        let mine: Vec<&RunResult> = runs.iter().filter(|r| r.policy == p).collect();
        let mut stats = Vec::new();
        for (i, t) in targets.targets.iter().enumerate() {
            if targets.applies(i, p) {
                let vals: Vec<f64> = mine.iter().filter_map(|r| r.values.get(i).copied().flatten()).collect();
                stats.push(stat(t, &vals));
            }
        }
        let loss = stats.iter().map(|s| s.loss).sum();
        let hit = stats.iter().all(|s| s.hit);
        per_policy.push(PolicyResult {
            name: pol.name.clone(),
            weight: pol.weight,
            stats,
            loss,
            hit,
            outcomes: outcomes_of(mine.iter().copied()),
            skipped_steps: mine.iter().map(|r| r.skipped).sum(),
        });
    }
    let mut stats = Vec::with_capacity(targets.targets.len());
    for (i, t) in targets.targets.iter().enumerate() {
        let under: Vec<usize> = (0..targets.policies.len()).filter(|p| targets.applies(i, *p)).collect();
        let vals: Vec<f64> = runs.iter().filter(|r| under.contains(&r.policy)).filter_map(|r| r.values.get(i).copied().flatten()).collect();
        let mut s = stat(t, &vals);
        if targets.policies.len() > 1 {
            let mine = |p: usize| per_policy[p].stats.iter().find(|x| x.id == t.id);
            s.hit = under.iter().all(|p| mine(*p).is_some_and(|x| x.hit));
            s.loss = under.iter().map(|p| per_policy[*p].weight * mine(*p).map_or(0.0, |x| x.loss)).sum();
        }
        stats.push(s);
    }
    let loss = stats.iter().map(|s| s.loss).sum();
    let hit = stats.iter().all(|s| s.hit);
    let skipped_steps = runs.iter().map(|r| r.skipped).sum();
    PointResult { point, rules_hash: Some(rules_hash), runs: runs.len(), stats, loss, hit, outcomes: outcomes_of(runs.iter()), skipped_steps, policies: per_policy, errors: Vec::new() }
}

/// Compiles `doc` at a point and binds the targets to it.
pub fn prepare(doc: &Document, targets: &Targets, point: &Point) -> Result<Runner, Vec<Diagnostic>> {
    let compiled = compile(doc, &Options { overrides: point.clone(), ..Default::default() })?;
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
    sweep_keeping(doc, targets, points, seeds, opts, false).0
}

/// [`sweep`], also handing back each finished point's runs (policy-major, each policy's in seed
/// order; empty for points that didn't finish) when `keep_runs`: what each run did and measured,
/// to replay elsewhere (a playtest in a game engine) and compare.
pub fn sweep_keeping(doc: &Document, targets: &Targets, points: &[Point], seeds: &[u64], opts: &SweepOptions, keep_runs: bool) -> (SweepResult, Vec<Vec<RunResult>>) {
    // Compiling a big rule set takes a while: past the deadline, points aren't compiled at all
    // (they are reported unfinished, like points whose runs didn't all finish).
    let late = || opts.deadline.is_some_and(|d| Instant::now() > d);
    let prepared: Vec<Option<Result<Runner, Vec<Diagnostic>>>> = points.iter().map(|p| if late() { None } else { Some(prepare(doc, targets, p)) }).collect();
    // Jobs in (point, policy, seed) order: a point's runs are contiguous, each policy's in seed order.
    let policies = targets.policies.len().max(1);
    let per_point = policies * seeds.len();
    let jobs: Vec<(usize, usize, usize)> = (0..points.len())
        .filter(|i| matches!(prepared[*i], Some(Ok(_))))
        .flat_map(|i| (0..policies).flat_map(move |p| (0..seeds.len()).map(move |s| (i, p, s))))
        .collect();
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
                let Some(&(p, policy, s)) = jobs.get(j) else { break };
                let Some(Ok(runner)) = &prepared[p] else { continue };
                let r = runner.run(policy, seeds[s], opts.deadline);
                *results[j].lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
            });
        }
    });
    let results: Vec<Option<RunResult>> = results.into_iter().map(|m| m.into_inner().unwrap_or_else(|e| e.into_inner())).collect();
    let mut out: Vec<Option<PointResult>> = Vec::with_capacity(points.len());
    let mut unfinished = Vec::new();
    let mut runs_done = 0;
    let mut kept = Vec::new();
    let mut k = 0;
    for (i, p) in points.iter().enumerate() {
        match &prepared[i] {
            None => {
                unfinished.push(i);
                out.push(None);
            }
            Some(Err(diags)) => out.push(Some(PointResult {
                point: p.clone(),
                rules_hash: None,
                runs: 0,
                stats: Vec::new(),
                loss: f64::INFINITY,
                hit: false,
                outcomes: Vec::new(),
                skipped_steps: 0,
                policies: Vec::new(),
                errors: diags.clone(),
            })),
            Some(Ok(runner)) => {
                let mine = &results[k..k + per_point];
                k += per_point;
                let done: Vec<RunResult> = mine.iter().flatten().filter(|r| !r.interrupted).cloned().collect();
                runs_done += done.len();
                if done.len() == per_point {
                    out.push(Some(aggregate(p.clone(), runner.rules.hash, targets, &done)));
                    if keep_runs {
                        kept.push(done);
                    }
                } else {
                    unfinished.push(i);
                    out.push(None);
                    if keep_runs {
                        kept.push(Vec::new());
                    }
                }
            }
        }
    }
    if keep_runs {
        // Points that never ran (compile errors, past the deadline) keep their place.
        let mut by_point = Vec::with_capacity(points.len());
        let mut it = kept.into_iter();
        for p in &prepared {
            by_point.push(if matches!(p, Some(Ok(_))) { it.next().unwrap_or_default() } else { Vec::new() });
        }
        return (SweepResult { points: out, unfinished, runs_done }, by_point);
    }
    (SweepResult { points: out, unfinished, runs_done }, Vec::new())
}
