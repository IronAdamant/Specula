//! Pick-driven tuning: what the next round shows.
//!
//! Every tuned round fits the choice GP ([`crate::gp`]) to every pick so far and scores seeded
//! candidates by expected improvement inside a trust region around the incumbent (the last
//! pick) that shrinks as the rounds go ([`tune_radius`]); chips restrict the candidates to a cone
//! along their direction.
//!
//! - [`next_batch`] (`mode: "tune"`, the default): the incumbent plus points chosen greedily by
//!   EI with local penalisation. Measured on synthetic preferences (the `tune_beats_random_spreads`
//!   test): regret after 8 rounds of 9 is about a tenth of random spreads'.
//! - [`next_line`] (`mode: "line"`): Sequential Line Search, the 1-D subspace per round of
//!   Koyama et al.: points on the line through the incumbent and the EI maximiser. With a 9-tile
//!   gallery (not a continuous slider) it measured no better than random spreads in the same
//!   test, so it is offered, not the default.
//! - [`next_spread`] (`mode: "spread"`): a dispersed spread around the incumbent whose radius
//!   shrinks each round; chips shift its centre.

use crate::gp::{self, Choice, Kernel};
use crate::spread::{self, dist2, Region};
use specula_core::dmath;
use specula_core::rng::Rng;

/// Expected-improvement candidates per round (uniform, around the incumbent, around the best).
const UNIFORM: usize = 320;
const NEAR_INCUMBENT: usize = 96;
const NEAR_BEST: usize = 24;
const TOP: usize = 4;
const REFINE: usize = 24;

/// Everything shown and picked so far: points (exact duplicates merged) and choices.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Obs {
    pub points: Vec<Vec<f64>>,
    pub choices: Vec<Choice>,
}

impl Obs {
    /// The index of `p`, added if new.
    pub fn index(&mut self, p: &[f64]) -> usize {
        if let Some(i) = self.points.iter().position(|q| q.len() == p.len() && q.iter().zip(p).all(|(a, b)| a.to_bits() == b.to_bits())) {
            return i;
        }
        self.points.push(p.to_vec());
        self.points.len() - 1
    }

    /// A round: `shown` points, `winner` (an index into `shown`) picked over the rest.
    pub fn add_round(&mut self, shown: &[Vec<f64>], winner: Option<usize>) {
        let idx: Vec<usize> = shown.iter().map(|p| self.index(p)).collect();
        if let Some(w) = winner.filter(|w| *w < idx.len()) {
            let others: Vec<usize> = idx.iter().enumerate().filter(|(i, x)| *i != w && **x != idx[w]).map(|(_, x)| *x).collect();
            if !others.is_empty() {
                self.choices.push(Choice { winner: idx[w], others });
            }
        }
    }

    /// Keeps the fit bounded: the most recent points (and the choices among them).
    pub fn bounded(&self) -> Obs {
        if self.points.len() <= gp::MAX_POINTS {
            return self.clone();
        }
        let first = self.points.len() - gp::MAX_POINTS;
        let points = self.points[first..].to_vec();
        let choices = self
            .choices
            .iter()
            .filter(|c| c.winner >= first)
            .map(|c| Choice { winner: c.winner - first, others: c.others.iter().filter(|o| **o >= first).map(|o| o - first).collect() })
            .filter(|c| !c.others.is_empty())
            .collect();
        Obs { points, choices }
    }
}

/// A standard normal sample (Box-Muller with `dmath`).
fn gauss(rng: &mut Rng) -> f64 {
    let u1 = (f64::from(rng.next_u32()) + 1.0) / 4_294_967_297.0;
    let u2 = f64::from(rng.next_u32()) / 4_294_967_296.0;
    (-2.0 * dmath::ln_f64(u1)).sqrt() * dmath::cos_f64(2.0 * std::f64::consts::PI * u2)
}

fn uniform(rng: &mut Rng, dims: usize) -> Vec<f64> {
    (0..dims).map(|_| f64::from(rng.next_u32()) / 4_294_967_296.0).collect()
}

fn near(rng: &mut Rng, c: &[f64], sigma: f64) -> Vec<f64> {
    c.iter().map(|v| (v + sigma * gauss(rng)).clamp(0.0, 1.0)).collect()
}

/// What chips allow (a review probe: "darker" at the dark bound showed lighter variants). Every
/// chip dimension moves only the chips' way (`step_k * d_k >= 0`); while any chip dimension can
/// still move, a candidate must move along the chips; when none can (the incumbent sits at the
/// bound: [`chips_exhausted`]), the chip dimensions are held and the others vary.
#[derive(Clone, Copy)]
struct ChipRule<'a> {
    inc: &'a [f64],
    dir: Option<&'a [f64]>,
    exhausted: bool,
}

/// No chip dimension can move further the chips' way from `inc` (it is at the cube's bound).
pub fn chips_exhausted(inc: &[f64], dir: &[f64]) -> bool {
    !inc.iter().zip(dir).any(|(x, d)| (*d > 1e-9 && *x < 1.0 - 1e-6) || (*d < -1e-9 && *x > 1e-6))
}

/// `x` kept to the chips' side of `inc` (see the chip rule above); `None` when it doesn't move
/// along them while they still can.
pub fn chip_apply(inc: &[f64], dir: Option<&[f64]>, x: Vec<f64>) -> Option<Vec<f64>> {
    ChipRule::new(inc, dir).apply(x)
}

impl<'a> ChipRule<'a> {
    fn new(inc: &'a [f64], dir: Option<&'a [f64]>) -> ChipRule<'a> {
        ChipRule { inc, dir, exhausted: dir.is_some_and(|d| chips_exhausted(inc, d)) }
    }

    /// `x` with every chip dimension clipped to the chips' side of the incumbent (held, when
    /// exhausted); `None` when it then doesn't move along the chips (and they can still move).
    fn apply(&self, mut x: Vec<f64>) -> Option<Vec<f64>> {
        let Some(d) = self.dir else { return Some(x) };
        let mut along = 0.0;
        for (k, v) in x.iter_mut().enumerate() {
            if d[k].abs() <= 1e-9 {
                continue;
            }
            if self.exhausted || (*v - self.inc[k]) * d[k] < 0.0 {
                *v = self.inc[k];
            }
            along += (*v - self.inc[k]) * d[k];
        }
        (self.exhausted || along > 1e-3).then_some(x)
    }
}

/// The line round after a pick (see the module docs): `n` points, the incumbent exactly among
/// them. Falls back to a spread around the incumbent when the fit fails.
pub fn next_line(obs: &Obs, incumbent: &[f64], chips: Option<&[f64]>, n: usize, radius: f64, rng: &mut Rng) -> Vec<Vec<f64>> {
    let dims = incumbent.len();
    let b = obs.bounded();
    let Some(fit) = gp::fit(b.points, &b.choices, Kernel::for_dims(dims)) else {
        return next_spread(incumbent, radius * 0.5, chips, &obs.points, n, rng);
    };
    let (_, best) = fit.best();
    let rule = ChipRule::new(incumbent, chips);
    let (lo, hi, scored) = ei_pool(&fit, best, rule, radius, rng);
    let boxed = |p: Vec<f64>| -> Vec<f64> { p.iter().enumerate().map(|(d, v)| v.clamp(lo[d], hi[d])).collect() };
    let mut top: Option<(f64, Vec<f64>)> = None;
    for (ei, c) in scored.into_iter().take(TOP) {
        // Local refinement: random steps that shrink, kept when EI rises.
        let (mut e, mut x) = (ei, c);
        for k in 0..REFINE {
            let sigma = radius * 0.2 * dmath::exp2_f64(-((k / 8) as f64));
            if let Some(y) = rule.apply(boxed(near(rng, &x, sigma))) {
                let ey = fit.ei(&y, best);
                if ey > e {
                    e = ey;
                    x = y;
                }
            }
        }
        if top.as_ref().is_none_or(|t| e > t.0) {
            top = Some((e, x));
        }
    }
    // The line's direction keeps every chip dimension on the chips' side (the top candidate
    // already does; a fallback direction is projected the same way).
    let fallback = |rng: &mut Rng| -> Vec<f64> {
        let raw: Vec<f64> = uniform(rng, dims).iter().map(|v| v - 0.5).collect();
        match chips {
            Some(d) => raw.iter().zip(d).zip(incumbent).map(|((r, dk), x)| if dk.abs() > 1e-9 { if rule.exhausted || (*dk > 0.0 && *x >= 1.0) || (*dk < 0.0 && *x <= 0.0) { 0.0 } else { dk.abs().max(r.abs()) * dk.signum() } } else { *r }).collect(),
            None => raw,
        }
    };
    let mut dir: Vec<f64> = match &top {
        Some((_, x)) => x.iter().zip(incumbent).map(|(a, b)| a - b).collect(),
        None => fallback(rng),
    };
    if dir.iter().map(|v| v * v).sum::<f64>().sqrt() < 1e-3 {
        dir = fallback(rng);
    }
    let pts = line_points(incumbent, &dir, chips.is_some(), n, (&lo, &hi));
    // A degenerate line (the incumbent pinned in a corner): a spread on the chips' side instead.
    if pts.iter().all(|p| dist2(p, incumbent) < 1e-12) {
        return next_spread(incumbent, radius * 0.5, chips, &obs.points, n, rng);
    }
    pts
}

/// The batch round after a pick (`mode: "tune"`): the incumbent and `n - 1` points chosen
/// greedily by expected improvement with local penalisation (each chosen point damps EI near
/// it), inside the trust region and, with chips, their cone. More informative per round than a
/// line for a discrete gallery, where the human picks one of `n` tiles rather than sliding.
pub fn next_batch(obs: &Obs, incumbent: &[f64], chips: Option<&[f64]>, n: usize, radius: f64, rng: &mut Rng) -> Vec<Vec<f64>> {
    let dims = incumbent.len();
    let b = obs.bounded();
    let Some(fit) = gp::fit(b.points, &b.choices, Kernel::for_dims(dims)) else {
        return next_spread(incumbent, radius * 0.5, chips, &obs.points, n, rng);
    };
    let (_, best) = fit.best();
    let (_, _, pool) = ei_pool(&fit, best, ChipRule::new(incumbent, chips), radius, rng);
    let mut out = vec![incumbent.to_vec()];
    let rho2 = (radius * 0.3) * (radius * 0.3);
    let mut weight: Vec<f64> = pool.iter().map(|(_, c)| 1.0 - dmath::exp_f64(-dist2(c, incumbent) / (2.0 * rho2))).collect();
    while out.len() < n {
        let mut pick: Option<(usize, f64)> = None;
        for (i, (ei, _)) in pool.iter().enumerate() {
            let s = ei.max(0.0) * weight[i];
            if s > 0.0 && pick.is_none_or(|(_, b)| s > b) {
                pick = Some((i, s));
            }
        }
        let Some((i, _)) = pick else { break };
        let p = pool[i].1.clone();
        for (j, (_, c)) in pool.iter().enumerate() {
            weight[j] *= 1.0 - dmath::exp_f64(-dist2(c, &p) / (2.0 * rho2));
        }
        out.push(p);
    }
    if out.len() < n {
        // EI vanished (everything near is known): fill with a spread.
        let more = next_spread(incumbent, radius, chips, &out, n - out.len() + 1, rng);
        out.extend(more.into_iter().skip(1));
    }
    out
}

/// `mode: "tune"` round `k`: the incumbent, then expected-improvement picks and a shrinking
/// spread around the incumbent, alternating. Measured through the gallery API (the
/// `production_tuning_beats_both_baselines` test), the spread hill-climb wins in few dimensions
/// and batch EI in many; mixing them keeps the better of both.
pub fn next_mixed(obs: &Obs, inc: &[f64], chips: Option<&[f64]>, n: usize, k: usize, rng: &mut Rng) -> Vec<Vec<f64>> {
    let batch = next_batch(obs, inc, chips, n, tune_radius(k), rng);
    let around = next_spread(inc, spread_radius(k), chips, &[], n, rng);
    let mut out = vec![inc.to_vec()];
    let (mut a, mut b) = (batch.into_iter().skip(1), around.into_iter().skip(1));
    while out.len() < n {
        let (x, y) = (a.next(), b.next());
        if x.is_none() && y.is_none() {
            break;
        }
        for p in [x, y].into_iter().flatten() {
            if out.len() < n && !out.iter().any(|q| dist2(q, &p) < 1e-12) {
                out.push(p);
            }
        }
    }
    out
}

/// The trust region's bounds and the scored candidates (best first).
#[allow(clippy::type_complexity)]
fn ei_pool(fit: &gp::Fit, best: f64, rule: ChipRule<'_>, radius: f64, rng: &mut Rng) -> (Vec<f64>, Vec<f64>, Vec<(f64, Vec<f64>)>) {
    let incumbent = rule.inc;
    let dims = incumbent.len();
    // The trust region: a box of half-width `radius` around the incumbent (it shrinks as the
    // rounds go, so a discrete gallery keeps resolution where the picks are).
    let lo: Vec<f64> = incumbent.iter().map(|x| (x - radius).max(0.0)).collect();
    let hi: Vec<f64> = incumbent.iter().map(|x| (x + radius).min(1.0)).collect();
    let boxed = |p: Vec<f64>| -> Vec<f64> { p.iter().enumerate().map(|(d, v)| v.clamp(lo[d], hi[d])).collect() };
    // Candidates: uniform in the region, around the incumbent, around the best means.
    let mut cands: Vec<Vec<f64>> = (0..UNIFORM).map(|_| uniform(rng, dims).iter().enumerate().map(|(d, u)| lo[d] + u * (hi[d] - lo[d])).collect()).collect();
    cands.extend((0..NEAR_INCUMBENT).map(|_| boxed(near(rng, incumbent, radius * 0.4))));
    let mut ranked: Vec<(usize, f64)> = fit.points.iter().enumerate().map(|(i, p)| (i, fit.predict(p).0)).collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    for (i, _) in ranked.iter().take(TOP) {
        let c = fit.points[*i].clone();
        cands.extend((0..NEAR_BEST).map(|_| boxed(near(rng, &c, radius * 0.25))));
    }
    let mut scored: Vec<(f64, Vec<f64>)> = cands.into_iter().filter_map(|c| rule.apply(c)).map(|c| (fit.ei(&c, best), c)).collect();
    // Stable order: by EI, ties by position (deterministic).
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.iter().zip(&b.1).map(|(x, y)| x.total_cmp(y)).find(|o| o.is_ne()).unwrap_or(std::cmp::Ordering::Equal)));
    (lo, hi, scored)
}

/// The trust region's half-width in round `k` (1-based) of `mode: "tune"`.
pub fn tune_radius(k: usize) -> f64 {
    (0.5 * dmath::exp2_f64(-((k.saturating_sub(1)) as f64) * 0.32)).max(0.08)
}

/// `n` evenly spaced points on the chord of the box `lo`..`hi` through `inc` along `dir` (only
/// the half beyond `inc` when `forward`), with `inc` itself replacing the nearest.
pub fn line_points(inc: &[f64], dir: &[f64], forward: bool, n: usize, (blo, bhi): (&[f64], &[f64])) -> Vec<Vec<f64>> {
    let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for (k, (x, d)) in inc.iter().zip(dir).enumerate() {
        if d.abs() < 1e-12 {
            continue;
        }
        let (a, b) = ((blo[k] - x) / d, (bhi[k] - x) / d);
        lo = lo.max(a.min(b));
        hi = hi.min(a.max(b));
    }
    if !lo.is_finite() || !hi.is_finite() || hi - lo < 1e-9 {
        return vec![inc.to_vec(); n.max(1)];
    }
    if forward {
        lo = 0.0;
    }
    let n = n.max(2);
    let mut ts: Vec<f64> = (0..n).map(|i| lo + (hi - lo) * i as f64 / (n - 1) as f64).collect();
    // The incumbent itself is shown (it is the slider's current position).
    let nearest = (0..n).min_by(|a, b| ts[*a].abs().total_cmp(&ts[*b].abs())).unwrap_or(0);
    ts[nearest] = 0.0;
    ts.iter().map(|t| inc.iter().zip(dir).map(|(x, d)| (x + t * d).clamp(0.0, 1.0)).collect()).collect()
}

/// A spread of `n` around `inc` (half-width `radius`), shifted along `chips`, away from `avoid`;
/// `inc` is the first point.
pub fn next_spread(inc: &[f64], radius: f64, chips: Option<&[f64]>, avoid: &[Vec<f64>], n: usize, rng: &mut Rng) -> Vec<Vec<f64>> {
    let centre: Vec<f64> = match chips {
        Some(d) => inc.iter().zip(d).map(|(x, v)| (x + v * radius * 0.9).clamp(0.0, 1.0)).collect(),
        None => inc.to_vec(),
    };
    let far: Vec<Vec<f64>> = avoid.iter().filter(|p| dist2(p, inc) > 1e-12).cloned().collect();
    let rule = ChipRule::new(inc, chips);
    spread::dispersed_where(n, inc.len(), &[inc.to_vec()], &far, &Region { centre: Some(&centre), radius }, rng, &|p| rule.apply(p))
}

/// The spread radius of round `k` (1-based) in `mode: "spread"`.
pub fn spread_radius(k: usize) -> f64 {
    (0.35 * dmath::exp2_f64(-((k.saturating_sub(1)) as f64) * 0.75)).max(0.04)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic preference: a bump at `opt`, each dimension weighted.
    struct Pref {
        opt: Vec<f64>,
        w: Vec<f64>,
    }

    impl Pref {
        fn f(&self, x: &[f64]) -> f64 {
            let d2: f64 = x.iter().zip(&self.opt).zip(&self.w).map(|((a, b), w)| w * (a - b) * (a - b)).sum();
            dmath::exp_f64(-d2 / (2.0 * 0.22 * 0.22))
        }

        /// The simulated human: the best of what is shown, or (noisy) a Gumbel-perturbed best.
        fn pick(&self, shown: &[Vec<f64>], noise: f64, rng: &mut Rng) -> usize {
            let mut best = (0, f64::NEG_INFINITY);
            for (i, p) in shown.iter().enumerate() {
                let g = if noise > 0.0 { -dmath::ln_f64(-dmath::ln_f64((f64::from(rng.next_u32()) + 1.0) / 4_294_967_297.0)) * noise } else { 0.0 };
                let v = self.f(p) + g;
                if v > best.1 {
                    best = (i, v);
                }
            }
            best.0
        }
    }

    const N: usize = 9;
    const ROUNDS: usize = 8;

    fn tune_run(p: &Pref, seed: u64, noise: f64, line: bool) -> f64 {
        let dims = p.opt.len();
        let mut rng = Rng::new(seed, 1);
        let mut picker = Rng::new(seed, 2);
        let mut obs = Obs::default();
        let mut shown = spread::dispersed(N, dims, &[vec![0.5; dims]], &[], &Region { centre: None, radius: 1.0 }, &mut rng);
        let mut best = f64::NEG_INFINITY;
        for r in 0..ROUNDS {
            let w = p.pick(&shown, noise, &mut picker);
            best = best.max(p.f(&shown[w]));
            obs.add_round(&shown, Some(w));
            let inc = shown[w].clone();
            shown = if line { next_line(&obs, &inc, None, N, tune_radius(r + 2), &mut rng) } else { next_batch(&obs, &inc, None, N, tune_radius(r + 2), &mut rng) };
        }
        1.0 - best
    }

    fn random_run(p: &Pref, seed: u64, noise: f64) -> f64 {
        let dims = p.opt.len();
        let mut rng = Rng::new(seed, 3);
        let mut picker = Rng::new(seed, 2);
        let mut best = f64::NEG_INFINITY;
        for _ in 0..ROUNDS {
            let shown: Vec<Vec<f64>> = (0..N).map(|_| uniform(&mut rng, dims)).collect();
            let w = p.pick(&shown, noise, &mut picker);
            best = best.max(p.f(&shown[w]));
        }
        1.0 - best
    }

    /// Mean regret over 12 seeds: (batch, line, random spreads).
    fn compare(dims: usize, weights: impl Fn(usize) -> f64, noise: f64) -> (f64, f64, f64) {
        let (mut t, mut l, mut r) = (0.0, 0.0, 0.0);
        for seed in 0..12u64 {
            let mut g = Rng::new(seed * 7919 + 13, 9);
            let p = Pref { opt: (0..dims).map(|_| 0.15 + 0.7 * f64::from(g.next_u32()) / 4_294_967_296.0).collect(), w: (0..dims).map(&weights).collect() };
            t += tune_run(&p, seed, noise, false);
            l += tune_run(&p, seed, noise, true);
            r += random_run(&p, seed, noise);
        }
        (t / 12.0, l / 12.0, r / 12.0)
    }

    #[test]
    fn tune_beats_random_spreads() {
        // A round bump in 4 dimensions, an anisotropic one in 6 (two dimensions barely matter,
        // like a colour's hue when only lightness does), and a noisy picker.
        for (name, dims, w, noise) in [("bump4", 4usize, &(|_| 1.0) as &dyn Fn(usize) -> f64, 0.0), ("aniso6", 6, &|i| if i < 4 { 1.0 } else { 0.1 }, 0.0), ("noisy4", 4, &|_| 1.0, 0.03)] {
            let (tune, line, random) = compare(dims, w, noise);
            assert!(tune < 0.6 * random, "{name}: mean regret after {ROUNDS} rounds: tuned {tune:.4} vs random spreads {random:.4} (line {line:.4})");
        }
    }

    #[test]
    fn lines_hold_the_incumbent_and_follow_chips() {
        let inc = vec![0.3, 0.6];
        let unit = (&[0.0, 0.0][..], &[1.0, 1.0][..]);
        let pts = line_points(&inc, &[1.0, 0.0], false, 9, unit);
        assert_eq!(pts.len(), 9);
        assert!(pts.contains(&inc));
        assert!(pts.iter().any(|p| p[0] < 0.05) && pts.iter().any(|p| p[0] > 0.95), "the whole chord");
        let fwd = line_points(&inc, &[1.0, 0.0], true, 5, unit);
        assert!(fwd.iter().all(|p| p[0] >= 0.3 - 1e-12), "chips: only beyond the incumbent");
        // A chip direction keeps the next round on its side.
        let mut obs = Obs::default();
        let mut rng = Rng::new(3, 3);
        let shown = spread::dispersed(9, 2, std::slice::from_ref(&inc), &[], &Region { centre: None, radius: 1.0 }, &mut rng);
        obs.add_round(&shown, Some(0));
        let next = next_line(&obs, &inc, Some(&[-1.0, 0.0]), 9, 0.5, &mut rng);
        assert!(next.contains(&inc));
        assert!(next.iter().all(|p| p[0] <= 0.3 + 1e-9), "darker means lower: {next:?}");
        // Seeded: the same inputs give the same round.
        let mut a = Rng::new(5, 5);
        let mut b = Rng::new(5, 5);
        assert_eq!(next_line(&obs, &inc, None, 9, 0.5, &mut a), next_line(&obs, &inc, None, 9, 0.5, &mut b));
    }

    #[test]
    fn spreads_shrink_and_shift() {
        assert!(spread_radius(1) > spread_radius(3) && spread_radius(30) >= 0.04);
        let mut rng = Rng::new(1, 1);
        let inc = vec![0.5, 0.5, 0.5];
        let s = next_spread(&inc, 0.1, Some(&[1.0, 0.0, 0.0]), &[], 6, &mut rng);
        assert_eq!(s[0], inc);
        assert!(s[1..].iter().all(|p| p[0] > 0.45), "shifted towards +x: {s:?}");
    }

    #[test]
    fn observations_merge_duplicates() {
        let mut o = Obs::default();
        o.add_round(&[vec![0.1], vec![0.2]], Some(1));
        o.add_round(&[vec![0.2], vec![0.3]], Some(0));
        assert_eq!(o.points.len(), 3);
        assert_eq!(o.choices, vec![Choice { winner: 1, others: vec![0] }, Choice { winner: 1, others: vec![2] }]);
    }
}
