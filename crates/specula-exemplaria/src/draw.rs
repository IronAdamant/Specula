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
//! - **One uniform a stream, one residual**: every residual is a standard normal **truncated**
//!   to its window by the inverse of the distribution ([`crate::normalis::truncated`]): no
//!   clip, so nothing piles at a window's ends, and no loop. A trait's window is ±2.5 spreads.
//!   The size's is ±2.5 spreads narrowed to the kind's measured range (a quarter of that range is
//!   its spread where only a range is printed: the engine's assumption, said in
//!   [`Specimen::assumed`]). A number's own residual's window is ±2.5 spreads narrowed to where
//!   its value stays inside its measured range, given what the size and traits gave it.
//! - **In order**: the size first; every number then follows the size by the plan's exponent;
//!   then each trait as a residual at the kind's `vary` spread or the plan's; then a number's
//!   own residual where the kind gives it one.
//! - **Reach** ([`Resolved::reach`]): when a kind is resolved, the least and most every drawn
//!   number can be over all seeds is worked out by the draw's own arithmetic (the functions
//!   below that the draw calls, at every corner of the residuals' windows, on unrounded values),
//!   and the kind is refused (`outside_plan`) if any number could leave what its plan can write
//!   ([`crate::plan::NumberDef::range`], the size's range), naming the kind's own field that takes
//!   it there, or if any of the plan's relations between numbers
//!   ([`crate::plan::RelationDef`]) could break. As backstops a drawn value is held inside the
//!   plan's range and every relation is kept by moving its yielding number, whatever the kind
//!   says; neither acts on an accepted kind (the tests count them).
//! - **Rounded** to the unit's step (0.1 mm, 0.01 degree) before it is written, so the text is
//!   the same bytes everywhere. All arithmetic is `f64` through `dmath` and IEEE square roots.
//! - **No seed**: the mean (every residual zero). **A seed is at most [`MAX_SEED`]** (2^53 - 1):
//!   a larger one cannot be written exactly in a recipe's JSON, so [`crate::expand`] refuses it.

use crate::error::{Broken, Error};
use crate::kind::Kind;
use crate::normalis;
use crate::plan::{NumberDef, RelationDef, Resolved, Unit};
use specula_core::dmath;
use specula_core::hash::stream_seed;
use specula_core::rng::Rng;

/// A residual's window: at most this many spreads either side of the mean.
pub const CLIP_SPREADS: f64 = 2.5;
/// The largest seed a specimen is written with: the largest integer a JSON number holds exactly.
pub const MAX_SEED: u64 = (1 << 53) - 1;
/// The most corners the reach visits for one relation: each choice of how its ranged terms enter
/// times every corner of its residuals' windows (`2^(traits + own residuals + ranged ones)`). A
/// kind past it is refused `reach_too_dear` before any is visited; a plan's relations are held
/// by a test to stay under it with every residual ranged.
pub const MAX_RELATION_CORNERS: usize = 1 << 14;
/// The most work the reach does for one kind, in evaluations: a relation's terms at one size (at
/// most three terms, a test holds it), or one corner of a number's windows (they cost about the
/// same, under a microsecond each here). Past it the kind is refused `reach_too_dear`, counted
/// as the work is done. The work of a relation is at most its corners × (4,096 pieces + 2)
/// evaluations, so this, not the corners' ceiling, is what bounds it. The reach can run on
/// the engine thread (a gallery applied to a kind file validates it there), so it is set from a
/// measurement to keep a kind at the ceiling under 2 ms (`the_reach_at_its_ceiling_is_cheap`).
pub const MAX_REACH_WORK: u64 = 1 << 10;

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

/// The least and the most a drawn number can be over every seed (before it is rounded to its
/// step), and its value at the mean (as written).
#[derive(Clone, Debug, PartialEq)]
pub struct Bound {
    pub name: &'static str,
    pub unit: Unit,
    pub mean: f64,
    pub lo: f64,
    pub hi: f64,
}

impl Bound {
    /// The bounds as printed: the least rounded down and the most rounded up to the unit's step,
    /// so every written value lies inside what is printed.
    pub fn printed(&self) -> (f64, f64) {
        (round_down(self.lo, self.unit), round_up(self.hi, self.unit))
    }
}

/// Steps in one unit (10,000 for a metre at 0.1 mm), as an exact whole number.
fn per(unit: Unit) -> f64 {
    dmath::round_f64(1.0 / unit.step())
}

/// `x` rounded to the unit's step.
pub fn round(x: f64, unit: Unit) -> f64 {
    // (A whole number of steps divided by the steps in a unit is the double nearest the decimal,
    // so it is written as `0.6123`, never `0.6123000000000001`.)
    let per = per(unit);
    dmath::round_f64(x * per) / per
}

/// `x` rounded down to the unit's step.
pub fn round_down(x: f64, unit: Unit) -> f64 {
    let per = per(unit);
    dmath::floor_f64(x * per) / per
}

/// `x` rounded up to the unit's step.
pub fn round_up(x: f64, unit: Unit) -> f64 {
    let per = per(unit);
    dmath::ceil_f64(x * per) / per
}

/// The uniform in [0, 1) of the stream `stream` of `seed`: the first 53 bits of its generator
/// seeded by `stream_seed(seed, stream)`.
fn uniform(seed: u64, stream: &str) -> f64 {
    let mut r = Rng::new(stream_seed(seed, stream), 0);
    (r.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// The residual of the stream `stream` of `seed`: a standard normal truncated to `window`
/// (inside ±[`CLIP_SPREADS`]), from one uniform of the stream.
pub fn residual(seed: u64, stream: &str, window: [f64; 2]) -> f64 {
    normalis::truncated(uniform(seed, stream), window)
}

/// The residual of a stream at its widest window, ±[`CLIP_SPREADS`] (what a trait draws).
pub fn normal(seed: u64, stream: &str) -> f64 {
    residual(seed, stream, [-CLIP_SPREADS, CLIP_SPREADS])
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

/// How often a backstop acted in this thread: a value held inside the plan's range, a relation
/// kept by moving its yielding number. Neither acts on an accepted kind; the tests read these.
#[cfg(test)]
pub(crate) mod backstops {
    use std::cell::Cell;
    thread_local! {
        pub(crate) static HELD: Cell<u64> = const { Cell::new(0) };
        pub(crate) static KEPT: Cell<u64> = const { Cell::new(0) };
    }
    /// (held, kept) so far in this thread.
    pub(crate) fn counts() -> (u64, u64) {
        (HELD.with(Cell::get), KEPT.with(Cell::get))
    }
}

/// `x` inside the plan's `[lo, hi]` (a value that is not a number at all goes to `lo`): the
/// backstop that keeps every draw inside what its plan can write.
fn hold(x: f64, [lo, hi]: [f64; 2]) -> f64 {
    let held = if x.is_nan() { lo } else { x.clamp(lo, hi) };
    #[cfg(test)]
    if held.to_bits() != x.to_bits() {
        backstops::HELD.with(|c| c.set(c.get() + 1));
    }
    held
}

/// A trait's residual as the draw uses it: `z` spreads, rounded to the ratio's step.
fn trait_residual(z: f64, spread: f64) -> f64 {
    round(z * spread, Unit::Ratio)
}

/// How the size is drawn.
struct SizeLaw {
    mean: f64,
    sd: f64,
    /// Where it is drawn: ±2.5 spreads, narrowed to the kind's range.
    window: [f64; 2],
    /// That window as standard residuals.
    z: [f64; 2],
    plan: [f64; 2],
    /// The kind's field whose spread moves it (`size.X.sd`, or `size.X.range` when the spread
    /// is read from the range).
    spread_path: String,
    assumed: Option<String>,
}

impl SizeLaw {
    fn new(mean: f64, sd: f64, range: Option<[f64; 2]>, plan: [f64; 2], spread_path: String, assumed: Option<String>) -> SizeLaw {
        let (mut lo, mut hi) = (mean - CLIP_SPREADS * sd, mean + CLIP_SPREADS * sd);
        if let Some([a, b]) = range {
            lo = lo.max(a);
            hi = hi.min(b);
        }
        let z = if sd > 0.0 { [(lo - mean) / sd, (hi - mean) / sd] } else { [0.0, 0.0] };
        SizeLaw { mean, sd, window: [lo, hi], z, plan, spread_path, assumed }
    }

    /// The size at residual `z` (inside [`SizeLaw::z`]), as the draw writes it.
    fn at(&self, z: f64, unit: Unit) -> f64 {
        let [lo, hi] = self.window;
        // (`mean + z sd` at an end of `z` is the window's end to the last bit: held there.)
        let v = if lo <= hi { (self.mean + z * self.sd).clamp(lo, hi) } else { self.mean + z * self.sd };
        round(hold(v, self.plan), unit)
    }
}

/// How a number is drawn: its value at the mean, then the size's factor, the traits' and its
/// own. The draw and the reach both call [`NumberLaw::base`], [`NumberLaw::window`] and
/// [`NumberLaw::value`], so the reach is the draw's arithmetic at the corners of the windows.
struct NumberLaw {
    at_mean: f64,
    at_mean_path: String,
    at_mean_whose: &'static str,
    exponent: f64,
    exponent_path: Option<String>,
    /// (trait, weight, spread) for each trait that moves it.
    traits: Vec<(&'static str, f64, f64)>,
    /// Its own spread (`None` or 0: none).
    spread: Option<f64>,
    /// Its measured range (absolute).
    range: Option<[f64; 2]>,
}

impl NumberLaw {
    fn own_spread(&self) -> f64 {
        self.spread.unwrap_or(0.0)
    }

    /// Its value before its own residual: at the mean, by the size through its exponent, then
    /// each trait (`trait_r`: the trait's residual).
    fn base(&self, size: f64, size_mean: f64, trait_r: &dyn Fn(&str) -> f64) -> f64 {
        let mut v = self.at_mean * pow(size / size_mean, self.exponent);
        for (t, w, _) in &self.traits {
            v *= 1.0 + w * trait_r(t);
        }
        v
    }

    /// Where its own residual is drawn at `base`: ±2.5 spreads, narrowed to keep its value inside
    /// its measured range. With no spread: `[0, 0]`, or an empty window (`[1, 0]`) when `base`
    /// itself is outside that range.
    fn window(&self, base: f64) -> [f64; 2] {
        let s = self.own_spread();
        let (mut a, mut b) = if s > 0.0 { (-CLIP_SPREADS, CLIP_SPREADS) } else { (0.0, 0.0) };
        if let Some([lo, hi]) = self.range {
            if s > 0.0 {
                a = a.max((lo / base - 1.0) / s);
                b = b.min((hi / base - 1.0) / s);
            } else if !(base >= lo && base <= hi) {
                return [1.0, 0.0];
            }
        }
        [a, b]
    }

    /// Its value at `base` and own residual `z`.
    fn value(&self, base: f64, z: f64) -> f64 {
        let s = self.own_spread();
        if s > 0.0 {
            base * (1.0 + z * s)
        } else {
            base
        }
    }
}

fn fmt_range([lo, hi]: [f64; 2], unit: Unit) -> String {
    format!("{lo}..{hi} {}", unit.name())
}

/// Every combination of two values for each of `n` coordinates, as bit patterns.
fn corners(n: usize) -> impl Iterator<Item = usize> {
    0..(1usize << n)
}

/// One number's least and most over every corner, and what took it to each.
struct Extremes {
    lo: f64,
    hi: f64,
    /// The factors (path, value, whose) at the corner of the least and of the most.
    lo_factors: Vec<(String, f64, String)>,
    hi_factors: Vec<(String, f64, String)>,
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
                SizeLaw::new(s.mean, sd, s.range, def.range, spread_path, assumed)
            }
            None => SizeLaw::new(def.mean, def.sd, None, def.range, path("sd"), None),
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
        let z = seed.map_or(0.0, |s| residual(s, &stream(def.name), law.z));
        let size = law.at(z, def.unit);
        let mean = law.mean;
        let traits: Vec<(&'static str, f64)> = plan.traits().iter().map(|t| (t.name, trait_residual(seed.map_or(0.0, |s| normal(s, &stream(t.name))), self.trait_spread(t.name)))).collect();
        let trait_r = |t: &str| traits.iter().find(|x| x.0 == t).map_or(0.0, |x| x.1);
        let mut numbers: Vec<Value> = plan
            .numbers()
            .iter()
            .map(|n| {
                let l = self.number_law(n, mean);
                let base = l.base(size, mean, &trait_r);
                let z = match seed {
                    Some(s) if l.own_spread() > 0.0 => residual(s, &stream(n.name), l.window(base)),
                    _ => 0.0,
                };
                Value { name: n.name, value: round(hold(l.value(base, z), n.range), n.unit), unit: n.unit }
            })
            .collect();
        keep(plan.relations(), plan.numbers(), &mut numbers);
        let options = plan.options().iter().map(|o| (o.name, kind.options.get(o.name).cloned().unwrap_or_else(|| o.default.to_string()))).collect();
        Specimen { kind: kind.name.clone(), plan: plan.name(), seed, size: Value { name: def.name, value: size, unit: def.unit }, numbers, traits, options, assumed }
    }

    /// The least and most every drawn number can be over all seeds (the size first, then the
    /// plan's numbers), or why the kind is refused: a draw could leave what the plan can write,
    /// or break one of its relations, or the check would cost more than [`MAX_REACH_WORK`]
    /// (`reach_too_dear`). See the module's notes.
    pub fn reach(&self) -> Result<Vec<Bound>, Error> {
        self.reach_within(MAX_REACH_WORK).map(|(b, _)| b)
    }

    /// [`Resolved::reach`] under a ceiling of `max_work` evaluations of the relations, and the
    /// evaluations it made.
    pub(crate) fn reach_within(&self, max_work: u64) -> Result<(Vec<Bound>, u64), Error> {
        let plan = self.plan;
        let def = plan.size();
        let law = self.size_law();
        let inside = |x: f64, [lo, hi]: [f64; 2]| x.is_finite() && x >= lo && x <= hi;
        let refuse = |path: &str, number: &str, worst: f64, bound: [f64; 2], unit: Unit, why: String| {
            // (Rounded to its step unless that puts it inside what it is outside of: then whole.)
            let shown = if inside(round(worst, unit), bound) { format!("{worst}") } else { format!("{}", round(worst, unit)) };
            Error::new("outside_plan", path, format!("a seed of this kind could give {number} {shown} {}, outside what the plan {} can write ({}): {why}", unit.name(), plan.name(), fmt_range(bound, unit)))
                .with_reach(number, worst, bound)
        };
        let sp = |f: &str| format!("size.{}.{f}", def.name);
        // The size.
        if !inside(law.mean, law.plan) {
            return Err(refuse(&sp("mean"), def.name, law.mean, law.plan, def.unit, "its mean".into()));
        }
        match self.kind.size.as_ref().and_then(|s| s.range) {
            Some([lo, hi]) => {
                if !inside(lo, law.plan) || !inside(hi, law.plan) {
                    let worst = if lo < law.plan[0] { lo } else { hi };
                    return Err(refuse(&sp("range"), def.name, worst, law.plan, def.unit, "its range".into()));
                }
            }
            None => {
                // No range of its own: 2.5 spreads must stay inside the plan's (the plan's range is
                // where its geometry breaks, not a measured end of the kind's).
                let (lo, hi) = (law.mean - CLIP_SPREADS * law.sd, law.mean + CLIP_SPREADS * law.sd);
                if !inside(lo, law.plan) || !inside(hi, law.plan) {
                    let worst = if lo < law.plan[0] { lo } else { hi };
                    return Err(refuse(&law.spread_path, def.name, worst, law.plan, def.unit, format!("its mean {} at 2.5 spreads of {}; narrow the spread, or give the size a measured range", law.mean, law.sd)));
                }
            }
        }
        let sizes = [law.at(law.z[0], def.unit), law.at(law.z[1], def.unit)];
        let mut out = vec![Bound { name: def.name, unit: def.unit, mean: round(law.mean, def.unit), lo: sizes[0], hi: sizes[1] }];
        let laws: Vec<NumberLaw> = plan.numbers().iter().map(|n| self.number_law(n, law.mean)).collect();
        // The work so far, in evaluations: a corner of a number's windows, or a relation's terms
        // at one size (each a few powers and products: they cost about the same).
        let mut work = 0u64;
        for (n, l) in plan.numbers().iter().zip(&laws) {
            if !inside(l.at_mean, n.range) {
                return Err(refuse(&l.at_mean_path, n.name, l.at_mean, n.range, n.unit, format!("its value at the mean size ({})", l.at_mean_whose)));
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
            // A factor that is not positive at its window's end is no body's number.
            for (t, w, spread) in &l.traits {
                for z in [-CLIP_SPREADS, CLIP_SPREADS] {
                    let f = 1.0 + w * trait_residual(z, *spread);
                    if !(f > 0.0 && f.is_finite()) {
                        return Err(refuse(&format!("vary.{t}.spread"), n.name, l.at_mean * f, n.range, n.unit, format!("at 2.5 spreads the trait {t} alone takes it to {f:.4} of its mean (weight {w}, spread {spread}); a body's number stays positive")));
                    }
                }
            }
            let x = self.extremes(n, l, &law, sizes, &mut work, max_work)?;
            if x.lo < n.range[0] {
                let (path, _, whose) = x.lo_factors.iter().min_by(|a, b| a.1.total_cmp(&b.1)).cloned().expect("the size's factor");
                return Err(refuse(&path, n.name, x.lo, n.range, n.unit, format!("every residual at the end of its window; most of it from {path} ({whose}); narrow it, or give the number a measured range")));
            }
            if x.hi > n.range[1] {
                let (path, _, whose) = x.hi_factors.iter().max_by(|a, b| a.1.total_cmp(&b.1)).cloned().expect("the size's factor");
                return Err(refuse(&path, n.name, x.hi, n.range, n.unit, format!("every residual at the end of its window; most of it from {path} ({whose}); narrow it, or give the number a measured range")));
            }
            out.push(Bound { name: n.name, unit: n.unit, mean: round(l.at_mean, n.unit), lo: x.lo, hi: x.hi });
        }
        for rel in plan.relations() {
            self.relation_holds(rel, &laws, &law, sizes, &mut work, max_work)?;
        }
        Ok((out, work))
    }

    /// A number's least and most over every corner of its windows (the size's two ends, each
    /// trait that moves it at ±2.5 spreads, its own residual at the two ends of its window at
    /// that corner's base), by the draw's own functions, before rounding and before the backstop.
    /// The refusal of a kind whose reach would cost more than its ceiling, checking `what_for`.
    fn too_dear(&self, law: &SizeLaw, what_for: &str, what: &'static str, done: u64, ceiling: u64) -> Error {
        Error::new(
            "reach_too_dear",
            law.spread_path.as_str(),
            format!("checking {what_for} of the plan {} for this kind would take more than {ceiling} {what} (it came to {done}): give fewer of its numbers a spread or a measured range of their own", self.plan.name()),
        )
        .with_work(what, done, ceiling)
    }

    /// (Each corner it visits is one evaluation of the reach's work, counted into `work`.)
    fn extremes(&self, n: &NumberDef, l: &NumberLaw, law: &SizeLaw, sizes: [f64; 2], work: &mut u64, max_work: u64) -> Result<Extremes, Error> {
        let mut x = Extremes { lo: f64::INFINITY, hi: f64::NEG_INFINITY, lo_factors: Vec::new(), hi_factors: Vec::new() };
        let size_path = |size: f64| match &l.exponent_path {
            Some(p) => (p.clone(), format!("the kind's exponent {} over sizes {}..{}", l.exponent, sizes[0], sizes[1]), size),
            None => (law.spread_path.clone(), format!("the size's spread (sizes {}..{}) through the exponent {}", sizes[0], sizes[1], l.exponent), size),
        };
        for c in corners(1 + l.traits.len()) {
            *work += 1;
            if *work > max_work {
                return Err(self.too_dear(law, &format!("the number {}", n.name), "evaluations", *work, max_work));
            }
            let size = sizes[c & 1];
            let r: Vec<f64> = l.traits.iter().enumerate().map(|(i, (_, _, s))| trait_residual(if c >> (i + 1) & 1 == 1 { CLIP_SPREADS } else { -CLIP_SPREADS }, *s)).collect();
            let trait_r = |t: &str| l.traits.iter().position(|x| x.0 == t).map_or(0.0, |i| r[i]);
            let base = l.base(size, law.mean, &trait_r);
            let [a, b] = l.window(base);
            if a > b {
                let path = format!("numbers.{}.range", n.name);
                let [lo, hi] = l.range.unwrap_or([0.0, 0.0]);
                return Err(Error::new(
                    "out_of_range",
                    path,
                    format!(
                        "its size and traits could take it to {} {} (before its own spread of {}), outside its own range {}, where its own spread cannot bring it back: give it a larger spread, or widen its range",
                        round(base, n.unit),
                        n.unit.name(),
                        l.own_spread(),
                        fmt_range([lo, hi], n.unit)
                    ),
                )
                .with_reach(n.name, base, [lo, hi]));
            }
            for z in [a, b] {
                let v = l.value(base, z);
                if !(v.is_finite() && v > 0.0) {
                    return Err(Error::new("outside_plan", law.spread_path.as_str(), format!("a seed of this kind could give {} {v}: no body's number", n.name)).with_reach(n.name, v, n.range));
                }
                let factors = || {
                    let (p, whose, s) = size_path(size);
                    let mut f = vec![(p, pow(s / law.mean, l.exponent), whose)];
                    for (i, (t, w, spread)) in l.traits.iter().enumerate() {
                        let whose = if self.kind.vary.contains_key(*t) { format!("the kind's spread {spread}, weight {w}") } else { format!("the plan's default spread {spread}, weight {w}: set vary.{t} in the kind") };
                        f.push((format!("vary.{t}.spread"), 1.0 + w * r[i], whose));
                    }
                    if l.own_spread() > 0.0 {
                        f.push((format!("numbers.{}.spread", n.name), v / base, format!("the kind's own spread {}", l.own_spread())));
                    }
                    f
                };
                if v < x.lo {
                    x.lo = v;
                    x.lo_factors = factors();
                }
                if v > x.hi {
                    x.hi = v;
                    x.hi_factors = factors();
                }
            }
        }
        Ok(x)
    }

    /// Whether a relation holds at every seed: its least over every seed, less what rounding each
    /// number to its step can take off, is at least its margin.
    ///
    /// - **The residuals at their corners.** For one size, every term is a product of factors each
    ///   moved by one residual, so the sum is linear along each residual and its least is at a
    ///   corner of their windows (exact).
    /// - **The size as one number.** At each corner the sum is a sum of powers of the size, and its
    ///   least over the size's window is found by [`least_over_size`]: at an end, or at a
    ///   stationary point bracketed to a billionth of the size and bounded there term by term
    ///   (sound, and within about a billionth of the terms of the least). So the size of the trunk
    ///   and the size of a girth in one sum are the same size.
    /// - **A number with a measured range** narrows its own residual's window by its value; it
    ///   enters at its range's end or at its unnarrowed value (two sums, the larger kept: each is
    ///   never above the least, so the larger is not either). This is the one place the check is a
    ///   bound and not exact.
    /// - **Floating point**: the least found is lowered by a millionth of a millionth of the
    ///   terms' size before it is compared (the draw's arithmetic is good to a few units in the
    ///   last place, ten thousand times less).
    ///
    /// A refusal names a body a seed can draw: the size (on its step) and the residuals at which
    /// the sum was least, its numbers there, and their sum ([`Broken`]).
    ///
    /// **Its work is bounded as it is done**: the corners it would visit (each choice of how the
    /// ranged terms enter, times the corners of the residuals) are refused past
    /// [`MAX_RELATION_CORNERS`] before any is visited, and every evaluation of the relation's
    /// terms at a size is counted into `work` before it is made, and the one that would pass
    /// `max_work` is not made: the kind is refused with its count, `max_work + 1`. So the work
    /// done never passes the ceiling, inside a corner's search of the size as between corners.
    fn relation_holds(&self, rel: &RelationDef, laws: &[NumberLaw], law: &SizeLaw, sizes: [f64; 2], work: &mut u64, max_work: u64) -> Result<(), Error> {
        let plan = self.plan;
        let terms: Vec<(usize, f64)> = rel.terms.iter().map(|(name, c)| (plan.numbers().iter().position(|n| n.name == *name).expect("a relation names the plan's numbers (a test holds it)"), *c)).collect();
        let mut traits: Vec<&'static str> = Vec::new();
        for (i, _) in &terms {
            for (t, _, _) in &laws[*i].traits {
                if !traits.contains(t) {
                    traits.push(t);
                }
            }
        }
        let own: Vec<usize> = terms.iter().enumerate().filter(|(_, (i, _))| laws[*i].own_spread() > 0.0).map(|(k, _)| k).collect();
        let ranged: Vec<usize> = own.iter().copied().filter(|k| laws[terms[*k].0].range.is_some()).collect();
        let spreads: Vec<f64> = traits.iter().map(|t| self.trait_spread(t)).collect();
        let dear = |what: &'static str, done: u64, ceiling: u64| self.too_dear(law, &format!("its relation {}", rel.name), what, done, ceiling);
        let bits = traits.len() + own.len() + ranged.len();
        if bits > MAX_RELATION_CORNERS.trailing_zeros() as usize {
            return Err(dear("corners", 1u64.checked_shl(bits as u32).unwrap_or(u64::MAX), MAX_RELATION_CORNERS as u64));
        }
        let allowance: f64 = terms.iter().map(|(i, c)| c.abs() * plan.numbers()[*i].unit.step() / 2.0).sum();
        let trait_corner = |c: usize| -> Vec<f64> { (0..traits.len()).map(|i| trait_residual(if c >> i & 1 == 1 { CLIP_SPREADS } else { -CLIP_SPREADS }, spreads[i])).collect() };
        // Each term (its coefficient times its number) at `size`, at corner `c` of the residuals
        // (`r`: the traits' residuals there), with the ranged terms entering as `choice` says.
        let at = |choice: usize, c: usize, r: &[f64], size: f64| -> Vec<f64> {
            let trait_r = |t: &str| traits.iter().position(|x| *x == t).map_or(0.0, |i| r[i]);
            terms
                .iter()
                .enumerate()
                .map(|(k, (i, coef))| {
                    let l = &laws[*i];
                    let base = l.base(size, law.mean, &trait_r);
                    coef * match own.iter().position(|o| *o == k) {
                        None => l.value(base, 0.0),
                        Some(j) => {
                            let up = c >> (traits.len() + j) & 1 == 1;
                            let z = if up { CLIP_SPREADS } else { -CLIP_SPREADS };
                            match ranged.iter().position(|q| *q == k) {
                                // A range's end: the low end where the term counts for the sum, the high
                                // where against it (the residual's own corner does not matter then).
                                Some(q) if choice >> q & 1 == 1 => {
                                    let [lo, hi] = l.range.expect("ranged");
                                    if *coef > 0.0 {
                                        lo
                                    } else {
                                        hi
                                    }
                                }
                                _ => l.value(base, z),
                            }
                        }
                    }
                })
                .collect()
        };
        // For each choice of how the ranged terms enter, the least over every corner and the size
        // (worst after rounding, the size, the corner); a choice that holds is enough.
        let mut best: Option<(f64, f64, usize, usize)> = None;
        for choice in corners(ranged.len()) {
            // How each term follows the size: a range's end does not.
            let exponents: Vec<f64> = terms.iter().enumerate().map(|(k, (i, _))| if ranged.iter().position(|q| *q == k).is_some_and(|q| choice >> q & 1 == 1) { 0.0 } else { laws[*i].exponent }).collect();
            let mut least: Option<(f64, f64, usize)> = None;
            for c in corners(traits.len() + own.len()) {
                let r = trait_corner(c);
                let f = |size: f64| at(choice, c, &r, size);
                // (The evaluations left under the ceiling: the search stops before passing it.)
                let before = max_work.saturating_sub(*work);
                let mut left = before;
                let Some((worst, size)) = corner_least(&f, &exponents, sizes, rel.margin, allowance, &mut left) else {
                    return Err(dear("evaluations", max_work.saturating_add(1), max_work));
                };
                *work += before - left;
                if least.is_none_or(|(x, _, _)| worst < x || worst.is_nan()) {
                    least = Some((worst, size, c));
                }
            }
            let least = least.expect("at least one corner");
            if least.0 >= rel.margin {
                return Ok(());
            }
            if best.is_none_or(|b| least.0 > b.0) {
                best = Some((least.0, least.1, choice, least.2));
            }
        }
        let (bound, size, choice, c) = best.expect("at least one choice");
        // The body there, as a seed draws it: the size on its step, each trait at its corner, each
        // own residual at the end of its window (narrowed by a measured range) the corner names.
        let def = plan.size();
        let size = round(size, def.unit).clamp(sizes[0], sizes[1]);
        let r = trait_corner(c);
        let trait_r = |t: &str| traits.iter().position(|x| *x == t).map_or(0.0, |i| r[i]);
        let mut at_body: Vec<(String, f64)> = vec![(def.name.to_string(), size)];
        at_body.extend(traits.iter().zip(&r).map(|(t, x)| (t.to_string(), *x)));
        let mut values: Vec<f64> = Vec::new();
        for (k, (i, coef)) in terms.iter().enumerate() {
            let l = &laws[*i];
            let base = l.base(size, law.mean, &trait_r);
            let v = match own.iter().position(|o| *o == k) {
                None => l.value(base, 0.0),
                Some(j) => {
                    let up = match ranged.iter().position(|q| *q == k) {
                        Some(q) if choice >> q & 1 == 1 => *coef < 0.0,
                        _ => c >> (traits.len() + j) & 1 == 1,
                    };
                    let [a, b] = l.window(base);
                    let z = if up { b } else { a };
                    at_body.push((format!("{}.own", plan.numbers()[*i].name), z));
                    l.value(base, z)
                }
            };
            values.push(v);
        }
        let worst = terms.iter().zip(&values).map(|((_, c), x)| c * x).sum::<f64>() - allowance;
        let names: Vec<&str> = terms.iter().map(|(i, _)| plan.numbers()[*i].name).collect();
        let unit = plan.numbers()[terms[0].0].unit;
        let formula = terms.iter().map(|(i, c)| format!("{c:+} {}", plan.numbers()[*i].name)).collect::<Vec<_>>().join(" ");
        let shown = names.iter().zip(&values).map(|(n, v)| format!("{n} {} {}", round(*v, unit), unit.name())).collect::<Vec<_>>().join(", ");
        let at_shown = at_body.iter().map(|(n, v)| format!("{n} {v}")).collect::<Vec<_>>().join(", ");
        // The kind's own field nearest the cause: a number of the relation it sets, else a trait
        // it sets that moves one of them, else its size.
        let path = names
            .iter()
            .find(|n| self.kind.numbers.contains_key(**n))
            .map(|n| format!("numbers.{n}"))
            .or_else(|| traits.iter().find(|t| self.kind.vary.contains_key(**t)).map(|t| format!("vary.{t}.spread")))
            .unwrap_or_else(|| law.spread_path.clone());
        // The wording by its cause: a body that breaks it; else, with no measured range among its
        // residuals, a least within the floating-point margin of the relation's; else the range.
        let message = if worst < rel.margin || ranged.is_empty() {
            let (breaks, close) = if worst < rel.margin {
                ("which breaks", String::new())
            } else {
                ("which comes within the floating-point margin of breaking", format!("; the check takes {ROUNDOFF:e} of its terms' size off for floating point, and a least this close cannot be told from breaking it: move the kind's numbers a little away from it"))
            };
            format!(
                "a seed of this kind could give {shown} (at {at_shown}), {breaks} the plan {}'s relation {} ({formula} must be at least {} {}; here {:.4} after rounding){close}: {}",
                plan.name(),
                rel.name,
                rel.margin,
                unit.name(),
                worst,
                rel.about
            )
        } else {
            // (Only where a measured range narrows a residual: the bound there is below the least.)
            format!(
                "the plan {}'s relation {} ({formula} must be at least {} {}) cannot be shown to hold at every seed of this kind: a measured range narrows a residual, and the bound on its least is {bound:.6} after rounding; the nearest body found, {shown} (at {at_shown}), gives {worst:.6}: narrow the range's number, or widen its range: {}",
                plan.name(),
                rel.name,
                rel.margin,
                unit.name(),
                rel.about
            )
        };
        Err(Error::new("outside_plan", path, message).with_relation(Broken {
            relation: rel.name.to_string(),
            at: at_body,
            values: names.iter().map(|n| n.to_string()).zip(values).collect(),
            sum: worst,
            bound,
            margin: rel.margin,
        }))
    }
}

/// Around a stationary point of a relation's sum the size is cut until a piece is this share of
/// the size wide (a billionth).
const BRACKET: f64 = 1e-9;
/// The most pieces the size is cut into at one corner of a relation: past it each piece left is
/// bounded term by term (sound; never reached by the plan's relations, a backstop on the work).
const MAX_PIECES: usize = 4096;
/// The outward margin for floating point, as a share of the size of a relation's terms: the
/// draw's arithmetic is good to a few units in the last place (about 1e-16), ten thousand times
/// less than this.
const ROUNDOFF: f64 = 1e-12;

/// A relation's least over the size at one corner of its residuals, as the check compares it with
/// the relation's `margin`: the bound of [`least_over_size`], less the rounding `allowance` and the
/// floating-point margin ([`ROUNDOFF`] of the terms' size at the window's ends). Returns it and the
/// size of the least value evaluated. The relation holds at that corner when it is at least `margin`.
/// `left`: the evaluations of `f` it may make, less those it made; `None` when it would need more
/// (the one past them not made).
fn corner_least(f: &dyn Fn(f64) -> Vec<f64>, exponents: &[f64], sizes: [f64; 2], margin: f64, allowance: f64, left: &mut u64) -> Option<(f64, f64)> {
    if *left < 2 {
        return None;
    }
    *left -= 2;
    let ends = [f(sizes[0]), f(sizes[1])];
    let roundoff = ROUNDOFF * ends[0].iter().zip(&ends[1]).map(|(a, b)| a.abs().max(b.abs())).sum::<f64>();
    let (bound, size) = least_over_size(f, exponents, sizes, ends, margin + allowance + roundoff, left)?;
    Some((bound - allowance - roundoff, size))
}

/// The term-by-term bound of a sum over a piece of the size: each term at its own worse end
/// (each term is a constant times a power of the size, so its least on a piece is at an end).
fn term_by_term(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x.min(*y)).sum()
}

/// The least of a relation's sum over the size in `[s0, s1]`, at one corner of its residuals.
/// `terms(s)` gives each term at size `s` (its coefficient times its number), each a constant
/// times a power of the size (`exponents`: 0 for a term the size does not move); `ends` is
/// `terms` at `s0` and `s1`. Returns a value never above the least (a bound) and the size of the
/// least value it evaluated (a size the kind can draw). Where every piece's term-by-term bound is
/// at least `enough`, the value returned is at least `enough` and is not refined further.
///
/// - **One exponent** among the terms the size moves: the sum is that power times a constant,
///   plus a constant, so it is monotone and its least is at an end (exact).
/// - **Several**: the least is at an end or at a stationary point (with two or three terms there
///   are at most two). The window is halved until, on each piece, the sign of the sum's
///   derivative is proved: its terms (`exponent × term / size`) are each a power of the size, so
///   each is least and most at the piece's ends, and a sum of their least above zero (by more
///   than [`ROUNDOFF`] of their size) proves the sum rises there, its least at the piece's low
///   end. A piece where no sign is proved holds a stationary point: it is halved until it is a
///   [`BRACKET`] of the size wide and then bounded term by term. That bound is sound whatever the
///   floating point did in finding the point, and is within about a billionth of the terms of the
///   least (a term moves by its exponent times the piece's width over the size across the piece).
/// - Every value a piece's end gives is a value of the sum at a size inside the window, so the
///   least of those is a body's, and the bound is the least of them and of the brackets' bounds.
fn least_over_size(terms: &dyn Fn(f64) -> Vec<f64>, exponents: &[f64], [s0, s1]: [f64; 2], ends: [Vec<f64>; 2], enough: f64, left: &mut u64) -> Option<(f64, f64)> {
    let sum = |v: &[f64]| v.iter().sum::<f64>();
    let [v0, v1] = ends;
    // The least value evaluated and its size; the bound (never above the least).
    let mut low = (sum(&v0), s0);
    let mut bound = low.0;
    fn lower(bound: &mut f64, x: f64) {
        if x < *bound || x.is_nan() {
            *bound = x;
        }
    }
    let seen = |low: &mut (f64, f64), bound: &mut f64, s: f64, x: f64| {
        if x < low.0 || x.is_nan() {
            *low = (x, s);
        }
        lower(bound, x);
    };
    if s1 <= s0 {
        return Some((bound, s0));
    }
    seen(&mut low, &mut bound, s1, sum(&v1));
    let mut moving: Vec<f64> = Vec::new();
    for e in exponents {
        if *e != 0.0 && !moving.contains(e) {
            moving.push(*e);
        }
    }
    if moving.len() <= 1 {
        return Some((bound, low.1));
    }
    let slope = |s: f64, v: &[f64]| -> Vec<f64> { v.iter().zip(exponents).map(|(x, e)| e * x / s).collect() };
    let mut stack = vec![(s0, v0, s1, v1)];
    let mut pieces = 0usize;
    while let Some((p, vp, q, vq)) = stack.pop() {
        let each = term_by_term(&vp, &vq);
        if each >= enough {
            // (Nothing here can take the least under `enough`.)
            lower(&mut bound, each);
            continue;
        }
        let (dp, dq) = (slope(p, &vp), slope(q, &vq));
        let (mut lo, mut hi, mut scale) = (0.0, 0.0, 0.0);
        for (a, b) in dp.iter().zip(&dq) {
            lo += a.min(*b);
            hi += a.max(*b);
            scale += a.abs().max(b.abs());
        }
        if lo > ROUNDOFF * scale || hi < -ROUNDOFF * scale {
            // Monotone: its least is at an end, a value seen already.
            continue;
        }
        pieces += 1;
        if q - p <= BRACKET * q || pieces >= MAX_PIECES {
            lower(&mut bound, each);
            continue;
        }
        let m = 0.5 * (p + q);
        if *left == 0 {
            return None;
        }
        *left -= 1;
        let vm = terms(m);
        seen(&mut low, &mut bound, m, sum(&vm));
        stack.push((m, vm.clone(), q, vq));
        stack.push((p, vp, m, vm));
    }
    Some((bound, low.1))
}

/// The backstop of the relations: each, in the plan's order, kept by moving its yielding number
/// to where the sum is its margin (rounded to the safe side of its step), inside its range.
fn keep(relations: &[RelationDef], defs: &[NumberDef], numbers: &mut [Value]) {
    for rel in relations {
        let get = |numbers: &[Value], name: &str| numbers.iter().find(|v| v.name == name).map_or(0.0, |v| v.value);
        let sum: f64 = rel.terms.iter().map(|(n, c)| c * get(numbers, n)).sum();
        if sum >= rel.margin {
            continue;
        }
        #[cfg(test)]
        backstops::KEPT.with(|c| c.set(c.get() + 1));
        let Some(&(_, cy)) = rel.terms.iter().find(|(n, _)| *n == rel.yields) else { continue };
        let others = sum - cy * get(numbers, rel.yields);
        let want = (rel.margin - others) / cy;
        let Some(i) = numbers.iter().position(|v| v.name == rel.yields) else { continue };
        let unit = numbers[i].unit;
        // (Half a step past where the sum is the margin, so the written sum is past it however the
        // last bit of the arithmetic falls.)
        let moved = if cy > 0.0 { round_up(want + unit.step() / 2.0, unit) } else { round_down(want - unit.step() / 2.0, unit) };
        let range = defs.iter().find(|d| d.name == rel.yields).map_or([moved, moved], |d| d.range);
        numbers[i].value = moved.clamp(range[0], range[1]);
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

    /// No ceiling on a search's evaluations.
    fn unlimited() -> u64 {
        u64::MAX
    }

    /// The search that used to pass the ceiling by a whole corner: a sum whose slope no piece can
    /// prove (`s - s^(1 + 1e-13)` on `[0.5, 2]`: its two slope terms cancel to within the
    /// floating-point margin everywhere), so the window is cut until its cap of 4,096 pieces (2,061
    /// evaluations: a piece at the bracket's width costs none), twice the ceiling. Given fewer, it makes exactly as many as it is
    /// given and no more, and says it could not finish; given enough, it finishes.
    #[test]
    fn a_search_cut_into_every_piece_stops_at_its_ceiling() {
        let made = std::cell::Cell::new(0u64);
        let f = |s: f64| {
            made.set(made.get() + 1);
            vec![s, -pow(s, 1.0 + 1e-13)]
        };
        let e = [1.0, 1.0 + 1e-13];
        let mut left = unlimited();
        assert!(corner_least(&f, &e, [0.5, 2.0], 1e9, 0.0, &mut left).is_some());
        let whole = made.get();
        assert!(whole > 2 * MAX_REACH_WORK && unlimited() - left == whole, "{whole} evaluations");
        for given in [0u64, 1, 2, 3, 100, 1_024, whole - 1] {
            made.set(0);
            let mut left = given;
            assert!(corner_least(&f, &e, [0.5, 2.0], 1e9, 0.0, &mut left).is_none(), "{given}");
            assert!(made.get() <= given && made.get() == given - left, "given {given}: made {}, left {left}", made.get());
        }
    }

    #[test]
    fn residuals_are_standard_truncated_and_fixed() {
        let n = 20_000;
        let xs: Vec<f64> = (0..n).map(|s| normal(s, "t")).collect();
        let mean = xs.iter().sum::<f64>() / n as f64;
        let var = xs.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n as f64;
        // A standard normal truncated at ±2.5 has a spread of 0.9546.
        assert!(mean.abs() < 0.03 && (var.sqrt() - 0.9546).abs() < 0.03, "mean {mean}, sd {}", var.sqrt());
        assert!(xs.iter().all(|x| x.abs() <= CLIP_SPREADS));
        assert_eq!(normal(7, "a"), normal(7, "a"));
        assert_ne!(normal(7, "a"), normal(7, "b"), "streams differ");
        assert_eq!(round(0.612_349_9, Unit::Metre), 0.6123);
        assert_eq!(round(12.345_6, Unit::Degree), 12.35);
        assert_eq!((round_down(0.612_39, Unit::Metre), round_up(0.612_31, Unit::Metre)), (0.6123, 0.6124));
        assert_eq!(pow(2.0, 1.0), 2.0);
        assert!((pow(2.0, 0.5) - std::f64::consts::SQRT_2).abs() < 1e-15);
        assert_eq!(hold(f64::NAN, [1.0, 2.0]), 1.0);
        assert_eq!(hold(f64::INFINITY, [1.0, 2.0]), 2.0);
    }

    /// The least of a sum of powers of the size over a window, where it is at a stationary point
    /// between the ends (where each term at its own worse end would be far below it).
    #[test]
    fn the_least_over_the_size_is_found_between_its_ends() {
        let least = |f: &dyn Fn(f64) -> Vec<f64>, e: &[f64], w: [f64; 2], enough: f64| least_over_size(f, e, w, [f(w[0]), f(w[1])], enough, &mut unlimited()).unwrap();
        // (s - 1)² + 0.5 on [0.5, 2]: 0.5 at s = 1 (term by term: -2.25).
        let f = |s: f64| vec![s * s, -2.0 * s, 1.5];
        let (b, s) = least(&f, &[2.0, 1.0, 0.0], [0.5, 2.0], f64::INFINITY);
        assert!(b <= 0.5 && b > 0.5 - 1e-8 && (s - 1.0).abs() < 1e-4, "{b} at {s}");
        // s⁴/4 - s³ + s² on [0.5, 3]: a most at 1 and a least of 0 at 2, the ends 0.14 and 2.25.
        let f = |s: f64| vec![pow(s, 4.0) / 4.0, -pow(s, 3.0), s * s];
        let (b, s) = least(&f, &[4.0, 3.0, 2.0], [0.5, 3.0], f64::INFINITY);
        // (The bracket bounded term by term: within a billionth or two of the terms, 16 here.)
        assert!(b <= 1e-15 && b > -3e-8 && (s - 2.0).abs() < 1e-4, "{b} at {s}");
        // Monotone with two exponents: the least at an end, exactly.
        let f = |s: f64| vec![0.32 * s, -0.1518 * pow(s / 0.55, 1.05)];
        let (b, s) = least(&f, &[1.0, 1.05], [0.325, 0.775], f64::INFINITY);
        assert_eq!((b, s), (f(0.325).iter().sum::<f64>(), 0.325));
        // Enough already: the window is not looked into, and what is returned is at least enough.
        let f = |s: f64| vec![s * s, -2.0 * s, 1.5];
        assert!(least(&f, &[2.0, 1.0, 0.0], [0.5, 2.0], -3.0).0 >= -3.0);
    }

    /// Review 4, minor 2: a relation of two positive terms and one negative whose least is
    /// strictly inside the size's window and sits on the margin. `s² + 16/s - (12 - m)` on
    /// `[1, 3]` is stationary where `2s = 16/s²`, at `s = 2`, and its least there is `m` (0.0003);
    /// its ends are 5.0003 and 2.3336. The bound at the stationary point is taken term by term over
    /// its bracket, so it is a bound and not a sample: never above the least, and below it by more
    /// than floating point could account for (1e-12 of the terms' size, 24 here), and within two
    /// brackets' share of the terms' slopes (`Σ |exponent × term|` at the least: 16) of it. Sitting
    /// on the margin, the relation is not shown to hold there.
    #[test]
    fn the_bound_at_a_stationary_point_is_below_the_least_and_near_it() {
        let m = 0.0003;
        let k = 12.0 - m;
        let f = move |s: f64| vec![s * s, 16.0 / s, -k];
        let e = [2.0, -1.0, 0.0];
        let w = [1.0, 3.0];
        let least = 4.0 + 8.0 - k;
        let (size, slopes) = (4.0 + 8.0 + k, 2.0 * 4.0 + 8.0);
        let (b, s) = least_over_size(&f, &e, w, [f(w[0]), f(w[1])], m, &mut unlimited()).unwrap();
        assert!(least - b >= 1e-12 * size, "the bound {b} is not below the least {least} by more than floating point");
        assert!(least - b <= 2.0 * BRACKET * slopes, "the bound {b} is {} under the least {least}", least - b);
        assert!((s - 2.0).abs() < 1e-6, "the least evaluated at {s}");
        let (worst, _) = corner_least(&f, &e, w, m, 0.0, &mut unlimited()).unwrap();
        assert!(worst < m, "{worst}");
    }

    /// Review 4, minor 2: the floating-point margin decides at its edge. `s² + 16/s - k` on
    /// `[1, 1.5]` falls all the way (its stationary point is at 2), so its least is the sum at
    /// 1.5, exactly. With an allowance `a` for rounding, a corner holds where that least, less `a`
    /// and 1e-12 of the terms' size at the window's ends (`2.25 + 16 + k`), is at least the
    /// margin: `k` set so that this is 1e-13 above the margin holds, 1e-13 below does not.
    #[test]
    fn the_floating_point_margin_is_taken_off_at_its_edge() {
        let (m, a, w, e) = (0.0003, 0.0001, [1.0, 1.5], [2.0, -1.0, 0.0]);
        for (shift, holds) in [(1e-13, true), (-1e-13, false)] {
            let mut k = 12.0;
            for _ in 0..4 {
                k = 2.25 + 16.0 / 1.5 - (m + a + 1e-12 * (2.25 + 16.0 + k) + shift);
            }
            let f = move |s: f64| vec![s * s, 16.0 / s, -k];
            let (worst, s) = corner_least(&f, &e, w, m, a, &mut unlimited()).unwrap();
            assert_eq!(s, 1.5);
            assert_eq!(worst >= m, holds, "{shift:e} from the edge: {worst} against {m}");
        }
    }
}
