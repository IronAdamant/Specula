//! Tuning: seeded Bayesian optimisation over a rule set's `param` items toward the targets.
//!
//! The search space is the chosen params, each over its declared range (or a narrower one the
//! caller gives), on a grid in the param's stored unit (milli-points, millimetres, ticks,
//! permille): about a thousand steps by default, or the caller's `step`. The objective is the
//! targets' loss over the training seeds, modelled as `ln(1 + loss)` by a Gaussian process
//! ([`crate::gp`]: squared-exponential kernel, its length scale picked from a fixed grid by
//! marginal likelihood). The first points are the rules' current values, then a Halton design;
//! after that each point maximises expected improvement over a candidate set (a Halton set and
//! seeded perturbations of the best so far). Proposals depend only on the evaluations so far and
//! the job's seed, so a job is resumable from its data ([`TuneJob::to_json`]) and gives the
//! same sequence on any machine. The best point is then checked on held-out seeds
//! ([`TuneJob::validate`]).

use specula_core::rng::Rng;
use specula_core::Json;
use specula_lex::diag::{codes, did_you_mean};
use specula_lex::units::{dec_from_f64, human, parse_dec, scale, suffix_unit, Unit};
use specula_lex::{compile, Diagnostic, Document, Options};

use crate::gp::{expected_improvement, Gp};
use crate::sweep::{sweep, Point, PointResult, SweepOptions};
use crate::targets::Targets;

/// Most params tuned at once, and evaluations per job.
pub const MAX_PARAMS: usize = 8;
pub const MAX_EVALS: usize = 200;
/// Candidates scored per proposal.
const CANDIDATES: usize = 512;

/// One tuned param, in stored units.
#[derive(Clone, Debug, PartialEq)]
pub struct Space {
    pub id: String,
    pub unit: Unit,
    pub min: i64,
    pub max: i64,
    /// Its value in the rules as written.
    pub base: i64,
    /// The search grid: points are `min + k × step` (clamped to `max`).
    pub step: i64,
}

/// A grid of at least a thousand steps over `min..=max`, on a 1-2-5 step in stored units.
pub fn default_step(min: i64, max: i64) -> i64 {
    let span = (max.saturating_sub(min)).max(1) / 1000;
    let mut decade = 1i64;
    loop {
        for m in [1, 2, 5] {
            let s = decade.saturating_mul(m);
            if s >= span {
                return s.max(1);
            }
        }
        decade = decade.saturating_mul(10);
    }
}

/// A param to tune: its id, optionally a narrower range and a grid step (in the param's human
/// unit or as unit literals, like overrides: `55`, `"55pt"`, `"0.5s"`).
#[derive(Clone, Debug, PartialEq)]
pub struct TuneParam {
    pub id: String,
    pub range: Option<(Json, Json)>,
    pub step: Option<Json>,
}

impl TuneParam {
    pub fn new(id: &str) -> TuneParam {
        TuneParam { id: id.to_owned(), range: None, step: None }
    }

    pub fn range(mut self, lo: impl Into<Json>, hi: impl Into<Json>) -> TuneParam {
        self.range = Some((lo.into(), hi.into()));
        self
    }

    pub fn step(mut self, step: impl Into<Json>) -> TuneParam {
        self.step = Some(step.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TuneOptions {
    /// Total evaluations (each runs every training seed).
    pub budget: usize,
    /// Points before the model takes over (default `2 × params + 1`, at least 3).
    pub init: Option<usize>,
    pub seed: u64,
}

/// One evaluated point.
#[derive(Clone, Debug, PartialEq)]
pub struct Eval {
    pub x: Vec<i64>,
    pub loss: f64,
    pub hit: bool,
    pub result: Option<PointResult>,
}

/// A tuning job's whole state (see the module docs).
#[derive(Clone, Debug, PartialEq)]
pub struct TuneJob {
    pub space: Vec<Space>,
    pub opts: TuneOptions,
    pub tick_hz: u32,
    pub evals: Vec<Eval>,
}

/// The `i`-th element of the Halton sequence in `base`.
fn halton(mut i: u64, base: u64) -> f64 {
    let (mut f, mut r) = (1.0, 0.0);
    while i > 0 {
        f /= base as f64;
        r += f * (i % base) as f64;
        i /= base;
    }
    r
}

const PRIMES: [u64; MAX_PARAMS] = [2, 3, 5, 7, 11, 13, 17, 19];

/// A step literal in stored units: a bare number in the param's human unit, or a unit literal
/// of the param's unit.
fn step_of(lit: &Json, unit: Unit, hz: u32) -> Option<i64> {
    let (dec, per) = match lit {
        Json::Num(v) => (dec_from_f64(*v)?, unit.per_human(hz)),
        Json::Str(s) => {
            let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
            let (num, suffix) = s.split_at(split);
            if suffix.is_empty() {
                (parse_dec(num)?, unit.per_human(hz))
            } else {
                let (u, n, d) = suffix_unit(suffix, hz)?;
                if u != unit {
                    return None;
                }
                (parse_dec(num)?, n / d)
            }
        }
        _ => return None,
    };
    scale(dec, per, 1).map(|s| s.value).filter(|v| *v > 0)
}

impl TuneJob {
    /// A job over `params` on the rule set `doc` as written. Every problem is reported.
    pub fn new(doc: &Document, params: &[TuneParam], opts: TuneOptions) -> Result<TuneJob, Vec<Diagnostic>> {
        let rules = compile(doc, &Options::default())?.rules;
        let mut diags = Vec::new();
        if params.is_empty() || params.len() > MAX_PARAMS {
            diags.push(Diagnostic::error(codes::OUT_OF_RANGE, "", "tune", format!("tune 1..{MAX_PARAMS} params")));
        }
        if opts.budget == 0 || opts.budget > MAX_EVALS {
            diags.push(Diagnostic::error(codes::OUT_OF_RANGE, "", "budget", format!("'budget' is 1..{MAX_EVALS} evaluations")));
        }
        // A literal's stored value for a param, by compiling with it as an override (so units and
        // the declared range are checked exactly as lex_simulate's params are).
        let value_of = |id: &str, lit: &Json, diags: &mut Vec<Diagnostic>| -> Option<i64> {
            match compile(doc, &Options { overrides: vec![(id.to_owned(), lit.clone())], natives: None }) {
                Ok(c) => c.rules.params.iter().find(|q| q.id == id).map(|q| q.value),
                Err(ds) => {
                    for mut d in ds.into_iter().filter(Diagnostic::is_error) {
                        d.item = id.to_owned();
                        d.path = "range".into();
                        diags.push(d);
                    }
                    None
                }
            }
        };
        let mut space: Vec<Space> = Vec::new();
        for TuneParam { id, range, step } in params {
            let Some(p) = rules.params.iter().find(|p| &p.id == id) else {
                diags.push(
                    Diagnostic::error(codes::UNKNOWN_REF, id.as_str(), "tune", format!("there is no param '{id}' to tune"))
                        .suggest_opt(did_you_mean(id, rules.params.iter().map(|p| p.id.as_str()))),
                );
                continue;
            };
            if space.iter().any(|s| &s.id == id) {
                diags.push(Diagnostic::error(codes::DUPLICATE, id.as_str(), "tune", format!("'{id}' is listed twice")));
                continue;
            }
            let (mut min, mut max) = (p.min, p.max);
            if let Some((lo, hi)) = range {
                min = value_of(id, lo, &mut diags).unwrap_or(min);
                max = value_of(id, hi, &mut diags).unwrap_or(max);
                if min >= max {
                    diags.push(Diagnostic::error(codes::OUT_OF_RANGE, id.as_str(), "range", "the range's low end must be below its high end"));
                }
            }
            if min == i64::MIN || max == i64::MAX {
                diags.push(Diagnostic::error(codes::INVALID, id.as_str(), "range", format!("param '{id}' has no declared min and max: give a range to search")));
            }
            let step = match step {
                None => default_step(min, max),
                Some(lit) => step_of(lit, p.unit, rules.tick_hz).unwrap_or_else(|| {
                    diags.push(
                        Diagnostic::error(codes::WRONG_TYPE, id.as_str(), "step", format!("'step' is a positive size in the param's unit ({})", p.unit.name()))
                            .got(lit.to_string()),
                    );
                    1
                }),
            };
            space.push(Space { id: id.clone(), unit: p.unit, min, max, base: p.value.clamp(min, max), step });
        }
        if diags.iter().any(Diagnostic::is_error) {
            return Err(diags);
        }
        Ok(TuneJob { space, opts, tick_hz: rules.tick_hz, evals: Vec::new() })
    }

    fn init_count(&self) -> usize {
        self.opts.init.unwrap_or(2 * self.space.len() + 1).max(3).min(self.opts.budget)
    }

    fn norm(&self, x: &[i64]) -> Vec<f64> {
        x.iter().zip(&self.space).map(|(v, s)| (*v - s.min) as f64 / (s.max - s.min) as f64).collect()
    }

    /// From normalised coordinates to the grid.
    fn denorm(&self, u: &[f64]) -> Vec<i64> {
        u.iter()
            .zip(&self.space)
            .map(|(v, s)| {
                let k = specula_core::dmath::round_f64(v.clamp(0.0, 1.0) * (s.max - s.min) as f64 / s.step as f64) as i64;
                s.min.saturating_add(k.saturating_mul(s.step)).clamp(s.min, s.max)
            })
            .collect()
    }

    /// The overrides for a point (exact unit literals, so they fold in bit for bit).
    pub fn point(&self, x: &[i64]) -> Point {
        self.space.iter().zip(x).map(|(s, v)| (s.id.clone(), Json::from(human(*v, s.unit, self.tick_hz)))).collect()
    }

    pub fn done(&self) -> bool {
        self.evals.len() >= self.opts.budget
    }

    /// The best evaluation so far (lowest loss; ties to the earliest).
    pub fn best(&self) -> Option<&Eval> {
        self.evals.iter().fold(None, |b: Option<&Eval>, e| match b {
            Some(b) if b.loss <= e.loss => Some(b),
            _ => Some(e),
        })
    }

    /// The next points to evaluate: the rest of the initial design, or one model-chosen point.
    pub fn propose(&self) -> Vec<Vec<i64>> {
        if self.done() {
            return Vec::new();
        }
        let seen = |x: &[i64], extra: &[Vec<i64>]| self.evals.iter().any(|e| e.x == x) || extra.iter().any(|e| e == x);
        let init = self.init_count();
        if self.evals.len() < init {
            let mut out: Vec<Vec<i64>> = Vec::new();
            let mut i = self.evals.len() as u64;
            let mut tries = 0;
            while self.evals.len() + out.len() < init && tries < 10_000 {
                tries += 1;
                let x = if i == 0 {
                    self.space.iter().map(|s| s.base).collect()
                } else {
                    let u: Vec<f64> = (0..self.space.len()).map(|d| halton(i, PRIMES[d])).collect();
                    self.denorm(&u)
                };
                i += 1;
                if !seen(&x, &out) {
                    out.push(x);
                }
            }
            return out;
        }
        // The model over everything seen so far.
        let xs: Vec<Vec<f64>> = self.evals.iter().map(|e| self.norm(&e.x)).collect();
        let ys: Vec<f64> = self.evals.iter().map(|e| specula_core::dmath::ln_f64(1.0 + e.loss.min(1e12))).collect();
        let Some(gp) = Gp::fit_best(&xs, &ys) else { return Vec::new() };
        let best_y = ys.iter().copied().fold(f64::MAX, f64::min);
        let best_x = self.best().map(|b| self.norm(&b.x)).unwrap_or_else(|| vec![0.5; self.space.len()]);
        // Candidates: a Halton set offset by the evaluation count, and perturbations of the best.
        let mut rng = Rng::new(self.opts.seed, self.evals.len() as u64);
        let mut cands: Vec<Vec<f64>> = Vec::with_capacity(CANDIDATES);
        let offset = 1000 + 97 * self.evals.len() as u64;
        for i in 0..CANDIDATES as u64 * 3 / 4 {
            cands.push((0..self.space.len()).map(|d| halton(offset + i, PRIMES[d])).collect());
        }
        while cands.len() < CANDIDATES {
            let width = [0.01, 0.03, 0.08, 0.2][cands.len() % 4];
            cands.push(best_x.iter().map(|v| v + width * (2.0 * f64::from(rng.f32()) - 1.0)).collect());
        }
        let mut pick: Option<(f64, Vec<i64>)> = None;
        for u in &cands {
            let x = self.denorm(u);
            if seen(&x, &[]) {
                continue;
            }
            let (mu, sigma) = gp.predict(&self.norm(&x));
            let ei = expected_improvement(mu, sigma, best_y);
            if pick.as_ref().is_none_or(|(b, _)| ei > *b) {
                pick = Some((ei, x));
            }
        }
        match pick {
            Some((_, x)) => vec![x],
            // Every candidate was seen (a small grid): the nearest unseen neighbour of the best.
            None => {
                let b = self.best().map(|b| b.x.clone()).unwrap_or_else(|| self.space.iter().map(|s| s.base).collect());
                for k in 1..=1000i64 {
                    for d in 0..self.space.len() {
                        for sign in [1, -1] {
                            let mut x = b.clone();
                            x[d] = (x[d] + sign * k * self.space[d].step).clamp(self.space[d].min, self.space[d].max);
                            if !seen(&x, &[]) {
                                return vec![x];
                            }
                        }
                    }
                }
                Vec::new()
            }
        }
    }

    /// Records a point's training result.
    pub fn record(&mut self, x: Vec<i64>, result: PointResult) {
        let (loss, hit) = (result.loss, result.hit);
        self.evals.push(Eval { x, loss, hit, result: Some(result) });
    }

    /// Evaluates proposals until the budget, `max_evals` more, or the deadline. Returns how many
    /// were evaluated now, and whether the deadline cut it short.
    pub fn advance(&mut self, doc: &Document, targets: &Targets, max_evals: usize, opts: &SweepOptions) -> (usize, bool) {
        let mut n = 0;
        while n < max_evals && !self.done() {
            if opts.deadline.is_some_and(|d| std::time::Instant::now() > d) {
                return (n, true);
            }
            let mut xs = self.propose();
            xs.truncate(max_evals - n);
            if xs.is_empty() {
                break;
            }
            let points: Vec<Point> = xs.iter().map(|x| self.point(x)).collect();
            let res = sweep(doc, targets, &points, &targets.train, opts);
            let cut = !res.unfinished.is_empty();
            // Keep what finished, in order; the rest is proposed again next call.
            for (x, r) in xs.into_iter().zip(res.points) {
                match r {
                    Some(r) => {
                        self.record(x, r);
                        n += 1;
                    }
                    None => break,
                }
            }
            if cut {
                return (n, true);
            }
        }
        (n, false)
    }

    /// The best point on the held-out seeds (never used for tuning).
    pub fn validate(&self, doc: &Document, targets: &Targets, opts: &SweepOptions) -> Option<PointResult> {
        let best = self.best()?;
        let res = sweep(doc, targets, &[self.point(&best.x)], &targets.holdout, opts);
        res.points.into_iter().next().flatten()
    }

    /// The job as data (to keep, show or resume from).
    pub fn to_json(&self) -> Json {
        let params = self.space.iter().map(|s| {
            Json::obj([
                ("id", Json::from(s.id.as_str())),
                ("unit", Json::from(s.unit.name())),
                ("min", Json::from(human(s.min, s.unit, self.tick_hz))),
                ("max", Json::from(human(s.max, s.unit, self.tick_hz))),
                ("base", Json::from(human(s.base, s.unit, self.tick_hz))),
                ("step", Json::from(human(s.step, s.unit, self.tick_hz))),
                ("stored", Json::arr([s.min, s.max, s.base, s.step].map(Json::from))),
            ])
        });
        let evals = self.evals.iter().map(|e| {
            Json::obj([
                ("x", Json::arr(e.x.iter().map(|v| Json::from(*v)))),
                ("params", Json::Obj(self.point(&e.x))),
                ("loss", Json::from((e.loss * 10_000.0).round() / 10_000.0)),
                ("loss_bits", Json::from(format!("{:016x}", e.loss.to_bits()))),
                ("hit", Json::from(e.hit)),
            ])
        });
        Json::obj([
            ("params", Json::arr(params)),
            ("budget", Json::from(self.opts.budget)),
            ("init", Json::from(self.init_count())),
            ("seed", Json::from(self.opts.seed.to_string())),
            ("tick_hz", Json::from(self.tick_hz)),
            ("evals", Json::arr(evals)),
        ])
    }

    /// Reads [`TuneJob::to_json`] (evaluations keep their exact losses; their per-target details
    /// aren't kept).
    pub fn from_json(j: &Json) -> Result<TuneJob, String> {
        let int = |v: &Json| v.as_f64().filter(|f| f.fract() == 0.0 && f.abs() < 9.0e15).map(|f| f as i64);
        let mut space = Vec::new();
        for p in j.get("params").and_then(Json::as_array).ok_or("'params' is missing")?.iter().take(MAX_PARAMS + 1) {
            let stored: Vec<i64> = p.get("stored").and_then(Json::as_array).and_then(|a| a.iter().map(int).collect()).ok_or("param.stored is [min, max, base, step]")?;
            let [min, max, base, step] = stored[..] else { return Err("param.stored is [min, max, base, step]".into()) };
            let unit = p.get("unit").and_then(Json::as_str).and_then(Unit::from_name).ok_or("param.unit is a Lex unit name")?;
            let id = p.get("id").and_then(Json::as_str).ok_or("param.id is missing")?.to_owned();
            if min >= max || step < 1 || !(min..=max).contains(&base) {
                return Err(format!("param '{id}': min below max, base inside, step at least 1"));
            }
            space.push(Space { id, unit, min, max, base, step });
        }
        if space.is_empty() || space.len() > MAX_PARAMS {
            return Err(format!("1..{MAX_PARAMS} params"));
        }
        let budget = j.get("budget").and_then(Json::as_u64).filter(|b| (1..=MAX_EVALS as u64).contains(b)).ok_or("'budget' is 1..200")? as usize;
        let seed = j.get("seed").and_then(Json::as_str).and_then(|s| s.parse().ok()).ok_or("'seed' is a decimal string")?;
        let tick_hz = j.get("tick_hz").and_then(Json::as_u64).filter(|h| (1..=1000).contains(h)).ok_or("'tick_hz' is 1..1000")? as u32;
        let init = j.get("init").and_then(Json::as_u64).map(|n| n.min(MAX_EVALS as u64) as usize);
        let mut evals = Vec::new();
        for e in j.get("evals").and_then(Json::as_array).unwrap_or_default().iter().take(MAX_EVALS) {
            let x: Vec<i64> = e.get("x").and_then(Json::as_array).and_then(|a| a.iter().map(int).collect()).ok_or("eval.x holds integers")?;
            if x.len() != space.len() {
                return Err("an evaluation has the wrong number of params".into());
            }
            let loss = e.get("loss_bits").and_then(Json::as_str).and_then(|h| u64::from_str_radix(h, 16).ok()).map(f64::from_bits).ok_or("eval.loss_bits is 16 hex digits")?;
            evals.push(Eval { x, loss, hit: e.get("hit").and_then(Json::as_bool).unwrap_or(false), result: None });
        }
        Ok(TuneJob { space, opts: TuneOptions { budget, init, seed }, tick_hz, evals })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grids_are_one_two_five() {
        assert_eq!(default_step(0, 1000), 1);
        assert_eq!(default_step(20_000, 90_000), 100);
        assert_eq!(default_step(0, 1_500_000), 2000);
        assert_eq!(step_of(&Json::from(1), Unit::Points, 30), Some(1000));
        assert_eq!(step_of(&Json::from("0.5s"), Unit::Duration, 30), Some(15));
        assert_eq!(step_of(&Json::from("2m"), Unit::Points, 30), None, "wrong unit");
        assert_eq!(step_of(&Json::from(0), Unit::Int, 30), None);
    }
}
