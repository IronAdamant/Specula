//! Seeded draws: one seed of a kind is one specimen, the same on every machine
//! (docs/design/EXEMPLARIA.md, A.2).
//!
//! - **Each number has its own stream**: a generator seeded by
//!   `specula_core::hash::stream_seed(seed, "<key>/<number>")` (the modeller's own scheme, one
//!   in the engine), so adding a number to a kind never moves the draws of the others, and the
//!   streams of two names, or of two neighbouring seeds, are unrelated (a test measures it).
//!   `<key>` is the kind's name, or for a project's copy the library kind it was taken from
//!   (`taken.streams`), so a copy renamed by `exemplar_take {"as"}` draws the specimens the
//!   library's entry showed ([`Kind::streams`]).
//! - **In order**: the size first (normal, at the size row's spread, or at a quarter of its range
//!   where only a range is printed: the engine's assumption, said in [`Specimen::assumed`]);
//!   every number then follows the size by the plan's exponent; then each trait as a residual,
//!   normal, at the kind's `vary` spread or the plan's; then a number's own residual where the
//!   kind gives it one.
//! - **Clipped** at 2.5 spreads and at the measured range, whichever is narrower: a specimen is
//!   always a body that was measured or lies between measured ones. A size with no range of the
//!   kind's keeps the plan's.
//! - **Reach** ([`Resolved::reach`]): when a kind is resolved, the least and most every drawn
//!   number can be over all seeds is worked out (every residual at its clip, through the
//!   exponents and every trait that moves it), and the kind is refused (`outside_plan`) if any
//!   number could leave what its plan can write ([`crate::plan::NumberDef::range`], the size's
//!   range), naming the kind's own field that takes it there. As a backstop a drawn value is held
//!   inside the plan's range whatever the kind says.
//! - **Rounded** to the unit's step (0.1 mm, 0.01 degree) before it is written, so the text is
//!   the same bytes everywhere. Normal draws are Box–Muller in `f64` through `dmath` (`ln_f64`,
//!   `cos_f64`) and IEEE square roots only.
//! - **No seed**: the mean (every residual zero). **A seed is at most [`MAX_SEED`]** (2^53 - 1):
//!   a larger one cannot be written exactly in a recipe's JSON, so [`crate::expand`] refuses it.

use crate::error::Error;
use crate::kind::Kind;
use crate::plan::{NumberDef, Resolved, Unit};
use specula_core::dmath;
use specula_core::hash::stream_seed;
use specula_core::rng::Rng;

/// Draws past this many spreads are clipped to it.
pub const CLIP_SPREADS: f64 = 2.5;
/// The largest seed a specimen is written with: the largest integer a JSON number holds exactly.
pub const MAX_SEED: u64 = (1 << 53) - 1;

/// One drawn (or mean) number of a specimen, rounded as it is written.
#[derive(Clone, Debug, PartialEq)]
pub struct Value {
    pub name: &'static str,
    pub value: f64,
    pub unit: Unit,
}

/// One seed of a kind: every number drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Specimen {
    pub kind: String,
    pub plan: &'static str,
    /// `None`: the kind's mean.
    pub seed: Option<u64>,
    /// The size, then the plan's numbers in its table's order.
    pub size: Value,
    pub numbers: Vec<Value>,
    /// Each trait's residual (a fraction: 0.03 is three in a hundred), in the plan's order.
    pub traits: Vec<(&'static str, f64)>,
    /// The plan's options as the kind sets them (the plan's defaults otherwise).
    pub options: Vec<(&'static str, String)>,
    /// What the engine assumed in drawing (a spread read from a range).
    pub assumed: Vec<String>,
}

impl Specimen {
    /// A number by name (the size included).
    pub fn get(&self, name: &str) -> f64 {
        if self.size.name == name {
            return self.size.value;
        }
        self.numbers.iter().find(|n| n.name == name).map_or(0.0, |n| n.value)
    }

    /// An option by name.
    pub fn option(&self, name: &str) -> &str {
        self.options.iter().find(|o| o.0 == name).map_or("", |o| o.1.as_str())
    }
}

/// The least and the most a drawn number can be over every seed, and its value at the mean.
#[derive(Clone, Debug, PartialEq)]
pub struct Bound {
    pub name: &'static str,
    pub unit: Unit,
    pub mean: f64,
    pub lo: f64,
    pub hi: f64,
}

/// `x` rounded to the unit's step.
pub fn round(x: f64, unit: Unit) -> f64 {
    // (A whole number of steps divided by the steps in a unit is the double nearest the decimal,
    // so it is written as `0.6123`, never `0.6123000000000001`.)
    let per = dmath::round_f64(1.0 / unit.step());
    dmath::round_f64(x * per) / per
}

/// A standard normal draw from the stream `stream` of `seed` (Box–Muller, `f64`, `dmath`), its
/// generator seeded by `stream_seed(seed, stream)`.
pub fn normal(seed: u64, stream: &str) -> f64 {
    let mut r = Rng::new(stream_seed(seed, stream), 0);
    let unit = |x: u64| (x >> 11) as f64 * (1.0 / (1u64 << 53) as f64);
    // u1 in (0, 1]: its logarithm is finite.
    let u1 = unit(r.next_u64()) + 1.0 / (1u64 << 53) as f64;
    let u2 = unit(r.next_u64());
    (-2.0 * dmath::ln_f64(u1)).sqrt() * dmath::cos_f64(2.0 * std::f64::consts::PI * u2)
}

/// A clipped normal residual: `z` within ±[`CLIP_SPREADS`].
fn clipped(seed: Option<u64>, stream: &str) -> f64 {
    seed.map_or(0.0, |s| normal(s, stream).clamp(-CLIP_SPREADS, CLIP_SPREADS))
}

/// `x^p` for `x > 0` through `dmath` (`exp(p ln x)`), exact for the common exponents.
pub fn pow(x: f64, p: f64) -> f64 {
    if p == 1.0 {
        x
    } else if p == 0.0 {
        1.0
    } else {
        dmath::exp_f64(p * dmath::ln_f64(x))
    }
}

/// `x` inside `[lo, hi]` (a value that is not a number at all goes to `lo`): the backstop that
/// keeps every draw inside what its plan can write.
fn hold(x: f64, [lo, hi]: [f64; 2]) -> f64 {
    if x.is_nan() {
        lo
    } else {
        x.clamp(lo, hi)
    }
}

/// How the size is drawn: mean, spread, the clip (the kind's range, else the plan's), and the
/// plan's own range.
struct SizeLaw {
    mean: f64,
    sd: f64,
    clip: [f64; 2],
    plan: [f64; 2],
    /// The kind's field whose spread moves it (`size.X.sd`, or `size.X.range` when the spread
    /// is read from the range).
    spread_path: String,
    assumed: Option<String>,
}

/// One factor of a drawn number, with the kind's field that sets it.
struct Factor {
    path: String,
    lo: f64,
    hi: f64,
    whose: String,
}

/// How a number is drawn.
struct NumberLaw {
    at_mean: f64,
    at_mean_path: String,
    at_mean_whose: &'static str,
    exponent: f64,
    exponent_path: Option<String>,
    /// (trait, weight, spread) for each trait that moves it.
    traits: Vec<(&'static str, f64, f64)>,
    spread: Option<f64>,
    range: Option<[f64; 2]>,
}

fn fmt_range([lo, hi]: [f64; 2], unit: Unit) -> String {
    format!("{lo}..{hi} {}", unit.name())
}

impl Resolved {
    /// The name every stream of this kind is keyed on ([`Kind::streams`]).
    pub fn stream_key(&self) -> &str {
        self.kind.streams()
    }

    fn size_law(&self) -> SizeLaw {
        let def = self.plan.size();
        let path = |f: &str| format!("size.{}.{f}", def.name);
        match &self.kind.size {
            Some(s) => {
                let (sd, spread_path, assumed) = match (s.sd, s.range) {
                    (Some(sd), _) => (sd, path("sd"), None),
                    (None, Some([lo, hi])) => ((hi - lo) / 4.0, path("range"), Some(format!("{}: no spread printed, so a quarter of its range ({lo}..{hi}) is taken as one spread", s.name))),
                    (None, None) => (0.0, path("sd"), None),
                };
                SizeLaw { mean: s.mean, sd, clip: s.range.unwrap_or(def.range), plan: def.range, spread_path, assumed }
            }
            None => SizeLaw { mean: def.mean, sd: def.sd, clip: def.range, plan: def.range, spread_path: path("sd"), assumed: None },
        }
    }

    fn trait_spread(&self, name: &str) -> f64 {
        let def = self.plan.traits().iter().find(|t| t.name == name).map_or(0.0, |t| t.spread);
        self.kind.vary.get(name).map_or(def, |v| v.spread)
    }

    fn number_law(&self, n: &NumberDef, size_mean: f64) -> NumberLaw {
        let set = self.kind.numbers.get(n.name);
        let p = |f: &str| format!("numbers.{}.{f}", n.name);
        let (at_mean, at_mean_path, at_mean_whose) = match set {
            Some(s) => match s.mean {
                Some(m) => (m, p("mean"), "the kind's"),
                None => (s.share.unwrap_or(n.share) * size_mean, p("share"), "the kind's share of its mean size"),
            },
            None if self.kind.size.is_some() => (n.share * size_mean, format!("size.{}.mean", self.plan.size().name), "the plan's share of the kind's mean size"),
            None => (n.share * size_mean, p("share"), "the plan's default"),
        };
        let exponent_set = set.and_then(|s| s.exponent);
        NumberLaw {
            at_mean,
            at_mean_path,
            at_mean_whose,
            exponent: exponent_set.unwrap_or(n.exponent),
            exponent_path: exponent_set.map(|_| p("exponent")),
            traits: n.traits.iter().map(|(t, w)| (*t, *w, self.trait_spread(t))).collect(),
            spread: set.and_then(|s| s.spread),
            range: set.and_then(|s| s.range),
        }
    }

    /// The specimen of `seed` (`None`: the mean).
    pub fn draw(&self, seed: Option<u64>) -> Specimen {
        let plan = self.plan;
        let kind = &self.kind;
        let key = self.stream_key();
        let stream = |n: &str| format!("{key}/{n}");
        let def = plan.size();
        let law = self.size_law();
        let assumed: Vec<String> = law.assumed.iter().cloned().collect();
        let z = clipped(seed, &stream(def.name));
        let size = round(hold(hold(law.mean + z * law.sd, law.clip), law.plan), def.unit);
        let mean = law.mean;
        // Traits: residuals at their spreads.
        let traits: Vec<(&'static str, f64)> = plan
            .traits()
            .iter()
            .map(|t| {
                let r = clipped(seed, &stream(t.name)) * self.trait_spread(t.name);
                (t.name, round(r, Unit::Ratio))
            })
            .collect();
        let numbers = plan
            .numbers()
            .iter()
            .map(|n| {
                let l = self.number_law(n, mean);
                let mut v = l.at_mean * pow(size / mean, l.exponent);
                for (t, w, _) in &l.traits {
                    let r = traits.iter().find(|x| x.0 == *t).map_or(0.0, |x| x.1);
                    v *= 1.0 + w * r;
                }
                if let Some(spread) = l.spread {
                    v *= 1.0 + clipped(seed, &stream(n.name)) * spread;
                }
                if let Some(r) = l.range {
                    v = hold(v, r);
                }
                Value { name: n.name, value: round(hold(v, n.range), n.unit), unit: n.unit }
            })
            .collect();
        let options = plan.options().iter().map(|o| (o.name, kind.options.get(o.name).cloned().unwrap_or_else(|| o.default.to_string()))).collect();
        Specimen { kind: kind.name.clone(), plan: plan.name(), seed, size: Value { name: def.name, value: size, unit: def.unit }, numbers, traits, options, assumed }
    }

    /// The least and most every drawn number can be over all seeds (the size first, then the
    /// plan's numbers), or why the kind is refused: a draw could leave what the plan can write.
    /// See the module's notes.
    pub fn reach(&self) -> Result<Vec<Bound>, Error> {
        let plan = self.plan;
        let def = plan.size();
        let law = self.size_law();
        let refuse = |path: &str, number: &str, worst: f64, bound: [f64; 2], unit: Unit, why: String| {
            Error::new("outside_plan", path, format!("a seed of this kind could give {number} {worst} {}, outside what the plan {} can write ({}): {why}", unit.name(), plan.name(), fmt_range(bound, unit)))
                .with_reach(number, worst, bound)
        };
        let inside = |x: f64, [lo, hi]: [f64; 2]| x.is_finite() && x >= lo && x <= hi;
        let sp = |f: &str| format!("size.{}.{f}", def.name);
        // The size.
        if !inside(law.mean, law.plan) {
            return Err(refuse(&sp("mean"), def.name, law.mean, law.plan, def.unit, "its mean".into()));
        }
        if let Some(s) = &self.kind.size {
            if let Some([lo, hi]) = s.range {
                let worst = if lo < law.plan[0] { lo } else { hi };
                if !inside(lo, law.plan) || !inside(hi, law.plan) {
                    return Err(refuse(&sp("range"), def.name, worst, law.plan, def.unit, "its range".into()));
                }
            } else {
                // No range of its own: 2.5 spreads must stay inside the plan's (a body piled at
                // the plan's limits by its clip is no measured body).
                let (lo, hi) = (law.mean - CLIP_SPREADS * law.sd, law.mean + CLIP_SPREADS * law.sd);
                if !inside(lo, law.plan) || !inside(hi, law.plan) {
                    let worst = if lo < law.plan[0] { lo } else { hi };
                    return Err(refuse(&law.spread_path, def.name, round(worst, def.unit), law.plan, def.unit, format!("its mean {} at 2.5 spreads of {}; narrow the spread, or give the size a measured range", law.mean, law.sd)));
                }
            }
        }
        let s_lo = round(hold(hold(law.mean - CLIP_SPREADS * law.sd, law.clip), law.plan), def.unit);
        let s_hi = round(hold(hold(law.mean + CLIP_SPREADS * law.sd, law.clip), law.plan), def.unit);
        let mut out = vec![Bound { name: def.name, unit: def.unit, mean: round(law.mean, def.unit), lo: s_lo, hi: s_hi }];
        for n in plan.numbers() {
            let l = self.number_law(n, law.mean);
            if !inside(l.at_mean, n.range) {
                return Err(refuse(&l.at_mean_path, n.name, round(l.at_mean, n.unit), n.range, n.unit, format!("its value at the mean size ({})", l.at_mean_whose)));
            }
            if let Some([lo, hi]) = l.range {
                let path = format!("numbers.{}.range", n.name);
                if !inside(lo, n.range) || !inside(hi, n.range) {
                    return Err(refuse(&path, n.name, if lo < n.range[0] { lo } else { hi }, n.range, n.unit, "its measured range".into()));
                }
                if l.at_mean < lo || l.at_mean > hi {
                    return Err(Error::new("out_of_range", path, format!("its value at the mean size, {}, lies outside its own range {}", round(l.at_mean, n.unit), fmt_range([lo, hi], n.unit))));
                }
            }
            // Each factor at its clip.
            let mut factors = Vec::new();
            let (a, b) = (pow(s_lo / law.mean, l.exponent), pow(s_hi / law.mean, l.exponent));
            let (size_path, size_whose) = match &l.exponent_path {
                Some(p) => (p.clone(), format!("the kind's exponent {} over sizes {s_lo}..{s_hi}", l.exponent)),
                None => (law.spread_path.clone(), format!("the size's spread (sizes {s_lo}..{s_hi}) through the exponent {}", l.exponent)),
            };
            factors.push(Factor { path: size_path, lo: a.min(b), hi: a.max(b), whose: size_whose });
            for (t, w, spread) in &l.traits {
                let r = round(CLIP_SPREADS * spread, Unit::Ratio);
                let (a, b) = (1.0 - w * r, 1.0 + w * r);
                let whose = if self.kind.vary.contains_key(*t) { format!("the kind's spread {spread}, weight {w}") } else { format!("the plan's default spread {spread}, weight {w}: set vary.{t} in the kind") };
                factors.push(Factor { path: format!("vary.{t}.spread"), lo: a.min(b), hi: a.max(b), whose });
            }
            if let Some(spread) = l.spread {
                factors.push(Factor { path: format!("numbers.{}.spread", n.name), lo: 1.0 - CLIP_SPREADS * spread, hi: 1.0 + CLIP_SPREADS * spread, whose: format!("the kind's own spread {spread}") });
            }
            if let Some(f) = factors.iter().find(|f| !(f.lo > 0.0 && f.lo.is_finite() && f.hi.is_finite())) {
                return Err(refuse(&f.path, n.name, round(l.at_mean * f.lo, n.unit), n.range, n.unit, format!("at 2.5 spreads this factor alone reaches {:.4} ({}); a body's number stays positive", f.lo, f.whose)));
            }
            let mut lo = l.at_mean * factors.iter().map(|f| f.lo).product::<f64>();
            let mut hi = l.at_mean * factors.iter().map(|f| f.hi).product::<f64>();
            if let Some(r) = l.range {
                lo = hold(lo, r);
                hi = hold(hi, r);
            }
            let (lo, hi) = (round(lo, n.unit), round(hi, n.unit));
            if lo < n.range[0] {
                let f = factors.iter().min_by(|x, y| x.lo.total_cmp(&y.lo)).expect("the size's factor");
                return Err(refuse(&f.path, n.name, lo, n.range, n.unit, format!("every factor at 2.5 spreads; most of it from {} ({}); narrow it, or give the number a measured range", f.path, f.whose)));
            }
            if hi > n.range[1] {
                let f = factors.iter().max_by(|x, y| x.hi.total_cmp(&y.hi)).expect("the size's factor");
                return Err(refuse(&f.path, n.name, hi, n.range, n.unit, format!("every factor at 2.5 spreads; most of it from {} ({}); narrow it, or give the number a measured range", f.path, f.whose)));
            }
            out.push(Bound { name: n.name, unit: n.unit, mean: round(l.at_mean, n.unit), lo, hi });
        }
        Ok(out)
    }
}

impl Kind {
    /// The name every stream of this kind's draws is keyed on: `taken.streams` for a project's
    /// copy (the library kind it was taken from, so a renamed copy draws the library entry's
    /// specimens), else the kind's own name. Editing a copy's numbers keeps the same stream for
    /// each number: a change to one number moves only what depends on it. To make a copy draw
    /// anew, as a kind of its own, set `taken.streams` to its own name.
    pub fn streams(&self) -> &str {
        self.taken.as_ref().and_then(|t| t.streams.as_deref()).unwrap_or(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normals_are_standard_and_fixed() {
        let n = 20_000;
        let xs: Vec<f64> = (0..n).map(|s| normal(s, "t")).collect();
        let mean = xs.iter().sum::<f64>() / n as f64;
        let var = xs.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n as f64;
        assert!(mean.abs() < 0.03 && (var.sqrt() - 1.0).abs() < 0.03, "mean {mean}, sd {}", var.sqrt());
        assert_eq!(normal(7, "a"), normal(7, "a"));
        assert_ne!(normal(7, "a"), normal(7, "b"), "streams differ");
        assert_eq!(round(0.612_349_9, Unit::Metre), 0.6123);
        assert_eq!(round(12.345_6, Unit::Degree), 12.35);
        assert_eq!(pow(2.0, 1.0), 2.0);
        assert!((pow(2.0, 0.5) - std::f64::consts::SQRT_2).abs() < 1e-15);
        assert_eq!(hold(f64::NAN, [1.0, 2.0]), 1.0);
        assert_eq!(hold(f64::INFINITY, [1.0, 2.0]), 2.0);
    }
}
