//! Seeded draws: one seed of a kind is one specimen, the same on every machine
//! (docs/design/EXEMPLARIA.md, A.2).
//!
//! - **Each number has its own stream**, `Rng::named(seed, "<kind>/<number>")`, so adding a
//!   number to a kind never moves the draws of the others.
//! - **In order**: the size first (normal, at the size row's spread, or at a quarter of its range
//!   where only a range is printed: the engine's assumption, said in [`Specimen::assumed`]);
//!   every number then follows the size by the plan's exponent; then each trait as a residual,
//!   normal, at the kind's `vary` spread or the plan's; then a number's own residual where the
//!   kind gives it one.
//! - **Clipped** at 2.5 spreads and at the measured range, whichever is narrower: a specimen is
//!   always a body that was measured or lies between measured ones.
//! - **Rounded** to the unit's step (0.1 mm, 0.01 degree) before it is written, so the text is
//!   the same bytes everywhere. Normal draws are Box–Muller in `f64` through `dmath` (`ln_f64`,
//!   `cos_f64`) and IEEE square roots only.
//! - **No seed**: the mean (every residual zero).

use crate::plan::{Resolved, Unit};
use specula_core::dmath;
use specula_core::rng::Rng;

/// Draws past this many spreads are clipped to it.
pub const CLIP_SPREADS: f64 = 2.5;

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

/// `x` rounded to the unit's step.
pub fn round(x: f64, unit: Unit) -> f64 {
    // (A whole number of steps divided by the steps in a unit is the double nearest the decimal,
    // so it is written as `0.6123`, never `0.6123000000000001`.)
    let per = dmath::round_f64(1.0 / unit.step());
    dmath::round_f64(x * per) / per
}

/// A standard normal draw from the stream `stream` of `seed` (Box–Muller, `f64`, `dmath`).
pub fn normal(seed: u64, stream: &str) -> f64 {
    let mut r = Rng::named(seed, stream);
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

impl Resolved {
    /// The specimen of `seed` (`None`: the mean).
    pub fn draw(&self, seed: Option<u64>) -> Specimen {
        let plan = self.plan;
        let kind = &self.kind;
        let stream = |n: &str| format!("{}/{n}", kind.name);
        let def = plan.size();
        let mut assumed = Vec::new();
        let (mean, sd, range) = match &kind.size {
            Some(s) => {
                let sd = match (s.sd, s.range) {
                    (Some(sd), _) => sd,
                    (None, Some([lo, hi])) => {
                        assumed.push(format!("{}: no spread printed, so a quarter of its range ({lo}..{hi}) is taken as one spread", s.name));
                        (hi - lo) / 4.0
                    }
                    (None, None) => 0.0,
                };
                (s.mean, sd, s.range)
            }
            None => (def.mean, def.sd, Some(def.range)),
        };
        let z = clipped(seed, &stream(def.name));
        let mut size = mean + z * sd;
        if let Some([lo, hi]) = range {
            size = size.clamp(lo, hi);
        }
        let size = round(size, def.unit);
        // Traits: residuals at their spreads.
        let traits: Vec<(&'static str, f64)> = plan
            .traits()
            .iter()
            .map(|t| {
                let spread = kind.vary.get(t.name).map_or(t.spread, |v| v.spread);
                let r = clipped(seed, &stream(t.name)) * spread;
                (t.name, round(r, Unit::Ratio))
            })
            .collect();
        let numbers = plan
            .numbers()
            .iter()
            .map(|n| {
                let set = kind.numbers.get(n.name);
                let exponent = set.and_then(|s| s.exponent).unwrap_or(n.exponent);
                let at_mean = match set {
                    Some(s) => s.mean.unwrap_or_else(|| s.share.unwrap_or(n.share) * mean),
                    None => n.share * mean,
                };
                let mut v = at_mean * pow(size / mean, exponent);
                for (t, w) in n.traits {
                    let r = traits.iter().find(|x| x.0 == *t).map_or(0.0, |x| x.1);
                    v *= 1.0 + w * r;
                }
                if let Some(spread) = set.and_then(|s| s.spread) {
                    v *= 1.0 + clipped(seed, &stream(n.name)) * spread;
                }
                if let Some([lo, hi]) = set.and_then(|s| s.range) {
                    v = v.clamp(lo, hi);
                }
                Value { name: n.name, value: round(v, n.unit), unit: n.unit }
            })
            .collect();
        let options = plan.options().iter().map(|o| (o.name, kind.options.get(o.name).cloned().unwrap_or_else(|| o.default.to_string()))).collect();
        Specimen { kind: kind.name.clone(), plan: plan.name(), seed, size: Value { name: def.name, value: size, unit: def.unit }, numbers, traits, options, assumed }
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
    }
}
