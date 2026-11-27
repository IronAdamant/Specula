//! Review 3 of S0, finding 1: the relation check takes the size as one number. Where a relation's
//! numbers follow the size by different exponents, the least of its sum is found over the size
//! itself (not each term at its own worse end of it), so a kind is refused only where a body it
//! can draw breaks the relation, and the refusal names that body.

use crate::draw::{pow, round, Specimen};
use crate::error::{Broken, Error};
use crate::kind::Kind;
use crate::plan::{resolve, resolve_among, MotionDef, NumberDef, OptionDef, Plan, RelationDef, Resolved, RoleDef, SizeDef, TraitDef, Unit};
use crate::plans::box_creature::PLAN as BOX;
use crate::write::Writing;

/// The box creature with no relations: what a kind would draw with no relation checked or kept.
struct Free;
impl Plan for Free {
    fn name(&self) -> &'static str {
        BOX.name()
    }
    fn about(&self) -> &'static str {
        BOX.about()
    }
    fn size(&self) -> &'static SizeDef {
        BOX.size()
    }
    fn numbers(&self) -> &'static [NumberDef] {
        BOX.numbers()
    }
    fn traits(&self) -> &'static [TraitDef] {
        BOX.traits()
    }
    fn roles(&self) -> &'static [RoleDef] {
        BOX.roles()
    }
    fn options(&self) -> &'static [OptionDef] {
        BOX.options()
    }
    fn motions(&self) -> &'static [MotionDef] {
        BOX.motions()
    }
    fn write(&self, s: &Specimen, w: &mut Writing) -> Result<(), Error> {
        BOX.write(s, w)
    }
}
static FREE: Free = Free;

/// A relation's rounding allowance: half a step a term.
fn allowance(rel: &RelationDef) -> f64 {
    rel.terms.iter().map(|(_, c)| c.abs() * Unit::Metre.step() / 2.0).sum()
}

/// The kind's size: mean, spread, and the window it is drawn in (±2.5 spreads, narrowed to its
/// range), each end on its step, as the draw writes it.
fn size_window(k: &Kind) -> (f64, [f64; 2]) {
    let def = BOX.size();
    let (mean, sd, range) = match &k.size {
        Some(s) => (s.mean, s.sd.or(s.range.map(|[a, b]| (b - a) / 4.0)).unwrap_or(0.0), s.range),
        None => (def.mean, def.sd, None),
    };
    let (mut lo, mut hi) = (mean - 2.5 * sd, mean + 2.5 * sd);
    if let Some([a, b]) = range {
        lo = lo.max(a);
        hi = hi.min(b);
    }
    (mean, [round(lo, def.unit), round(hi, def.unit)])
}

/// A number as the draw computes it (before rounding), from the kind's fields and the plan's
/// defaults: at the mean size, by the size through its exponent, each trait, and its own residual
/// `z` (in spreads). Written from the kind format, not from the draw's code.
fn number(k: &Kind, name: &str, size: f64, trait_r: &dyn Fn(&str) -> f64, z: f64) -> f64 {
    let def = BOX.numbers().iter().find(|n| n.name == name).unwrap();
    let mean = size_window(k).0;
    let set = k.numbers.get(name);
    let at = match set {
        Some(s) => s.mean.unwrap_or(s.share.unwrap_or(def.share) * mean),
        None => def.share * mean,
    };
    let e = set.and_then(|s| s.exponent).unwrap_or(def.exponent);
    let mut v = at * pow(size / mean, e);
    for (t, w) in def.traits {
        v *= 1.0 + w * trait_r(t);
    }
    let s = set.and_then(|s| s.spread).unwrap_or(0.0);
    if s > 0.0 {
        v * (1.0 + z * s)
    } else {
        v
    }
}

/// A number's own residual window at `v` (its value before it): ±2.5 spreads narrowed to its range.
fn own_window(k: &Kind, name: &str, v: f64) -> [f64; 2] {
    let set = k.numbers.get(name);
    let s = set.and_then(|s| s.spread).unwrap_or(0.0);
    let (mut a, mut b) = (-2.5f64, 2.5f64);
    if let Some([lo, hi]) = set.and_then(|s| s.range) {
        a = a.max((lo / v - 1.0) / s);
        b = b.min((hi / v - 1.0) / s);
    }
    [a, b]
}

/// The exact least of a relation's sum over every seed of `k` (unrounded values), less its
/// rounding allowance: the size scanned at 257 points and refined around the least, each trait at
/// 17 points of its window (its two ends among them) where a measured range narrows a residual
/// and at its two ends otherwise (the sum is then linear along it), each own residual at the two
/// ends of its window at that body.
fn exact_least(k: &Kind, rel: &RelationDef) -> f64 {
    let (_, [s0, s1]) = size_window(k);
    let mut traits: Vec<&'static str> = Vec::new();
    for (n, _) in rel.terms {
        for (t, _) in BOX.numbers().iter().find(|d| d.name == *n).unwrap().traits {
            if !traits.contains(t) {
                traits.push(t);
            }
        }
    }
    let spread = |t: &str| k.vary.get(t).map_or(BOX.traits().iter().find(|x| x.name == t).unwrap().spread, |v| v.spread);
    let own: Vec<&'static str> = rel.terms.iter().map(|(n, _)| *n).filter(|n| k.numbers.get(*n).and_then(|s| s.spread).unwrap_or(0.0) > 0.0).collect();
    let ranged = own.iter().any(|n| k.numbers.get(*n).and_then(|s| s.range).is_some());
    let steps = if ranged { 16 } else { 1 };
    let sum_at = |size: f64, r: &[f64], ups: usize| -> f64 {
        let trait_r = |t: &str| traits.iter().position(|x| *x == t).map_or(0.0, |i| r[i]);
        rel.terms
            .iter()
            .map(|(n, c)| {
                let z = match own.iter().position(|o| o == n) {
                    None => 0.0,
                    Some(j) => {
                        let w = own_window(k, n, number(k, n, size, &trait_r, 0.0));
                        if ups >> j & 1 == 1 {
                            w[1]
                        } else {
                            w[0]
                        }
                    }
                };
                c * number(k, n, size, &trait_r, z)
            })
            .sum()
    };
    let mut least = f64::INFINITY;
    let combos = (steps + 1usize).pow(traits.len() as u32);
    for t in 0..combos {
        let mut q = t;
        let r: Vec<f64> = traits
            .iter()
            .map(|name| {
                let i = q % (steps + 1);
                q /= steps + 1;
                round((-2.5 + 5.0 * i as f64 / steps as f64) * spread(name), Unit::Ratio)
            })
            .collect();
        for ups in 0..1usize << own.len() {
            let at = |i: usize, n: usize| s0 + (s1 - s0) * i as f64 / n as f64;
            let (mut best, mut bi) = (f64::INFINITY, 0);
            for i in 0..=256 {
                let v = sum_at(at(i, 256), &r, ups);
                if v < best {
                    (best, bi) = (v, i);
                }
            }
            // Refined between the neighbours of the least.
            let (a, b) = (at(bi.saturating_sub(1), 256), at((bi + 1).min(256), 256));
            for i in 0..=256 {
                best = best.min(sum_at(a + (b - a) * i as f64 / 256.0, &r, ups));
            }
            least = least.min(best);
        }
    }
    least - allowance(rel)
}

/// The refusal's body put back through the draw's arithmetic (written here from the kind format):
/// its numbers and its sum are what the refusal reports.
fn reproduces(k: &Kind, b: &Broken) -> Result<(), String> {
    let rel = BOX.relations().iter().find(|r| r.name == b.relation).unwrap();
    let get = |key: &str| b.at.iter().find(|(n, _)| n == key).map(|(_, v)| *v);
    let size = get(BOX.size().name).ok_or("no size in the body")?;
    if round(size, Unit::Metre) != size {
        return Err(format!("size {size} not on its step"));
    }
    let trait_r = |t: &str| get(t).unwrap_or(0.0);
    let mut sum = 0.0;
    for ((n, c), (vn, v)) in rel.terms.iter().zip(&b.values) {
        let z = get(&format!("{n}.own")).unwrap_or(0.0);
        let x = number(k, n, size, &trait_r, z);
        if vn != n || (x - v).abs() > 1e-12 * x.abs() {
            return Err(format!("{n}: the body gives {x}, the refusal says {v}"));
        }
        sum += c * x;
    }
    let sum = sum - allowance(rel);
    if (sum - b.sum).abs() > 1e-12 {
        return Err(format!("the body sums to {sum}, the refusal says {}", b.sum));
    }
    if b.bound > b.sum + 1e-12 {
        return Err(format!("the bound {} is above the body's sum {}", b.bound, b.sum));
    }
    Ok(())
}

fn kind(text: &str) -> Kind {
    Kind::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

/// A dog-like kind (the review's): shoulder height 0.55 m with a spread of 0.09 (a coefficient
/// of variation of 16 per cent), a leg's girth 0.12 of it growing by `exponent`, the rest the
/// plan's.
fn dog(share: f64, exponent: f64) -> Kind {
    kind(&format!(
        r#"{{"kind": 1, "name": "dog", "plan": "box_creature", "about": "a probe", "size": {{"shoulder_height": {{"mean": 0.55, "sd": 0.09, "unit": "m", "own": "x"}}}}, "numbers": {{"leg_girth": {{"share": {share}, "exponent": {exponent}, "own": "x"}}}}}}"#
    ))
}

/// The review's dog: a girth that grows 5, 10 or 20 per cent faster than the trunk's width is
/// whole at every seed (its feet are at least 0.0146 m apart), and is accepted; before the mend
/// it was refused at -0.128 m, a trunk of the smallest size paired with a girth of the largest.
/// Its least, found independently over the size and the build: 0.0125, 0.0146, 0.0165, 0.0166 m.
/// A wider girth that does break is refused, naming a body of its own seeds.
#[test]
fn a_dog_whose_girths_grow_faster_than_its_trunk_is_accepted() {
    let rel = BOX.relations().iter().find(|r| r.name == "legs_apart_sideways").unwrap();
    for (e, least) in [(1.0, 0.012_50), (1.05, 0.014_55), (1.1, 0.016_53), (1.2, 0.016_55)] {
        let k = dog(0.12, e);
        let exact = exact_least(&k, rel) + allowance(rel);
        assert!((exact - least).abs() < 0.000_05, "exponent {e}: the least is {exact}, not {least}");
        resolve(&k, None).unwrap_or_else(|err| panic!("exponent {e}: {err}"));
    }
    // A girth 0.135 of the size at an exponent of 1.2: at the largest size the feet would cross.
    let k = dog(0.135, 1.2);
    let e = resolve(&k, None).unwrap_err();
    let b = e.relation.as_ref().unwrap_or_else(|| panic!("{e}"));
    assert_eq!(b.relation, "legs_apart_sideways", "{e}");
    reproduces(&k, b).unwrap_or_else(|m| panic!("{m}: {e}"));
    assert_eq!(b.at[0], ("shoulder_height".to_string(), 0.775), "the largest dog: {e}");
    let exact = exact_least(&k, rel);
    assert!(exact < rel.margin && (b.sum - exact).abs() < 1e-9 && (b.bound - b.sum).abs() < 1e-9, "the least {exact}, the refusal's {} (bound {}): {e}", b.sum, b.bound);
    assert!(e.message.contains("shoulder_height 0.775") && e.to_json().get("at").is_some(), "{e}");
}

/// Review 4, minor 1: a kind on the very edge of a relation is told the cause it has. Bisected
/// onto the edge (to the last bit of a girth's share), the first refused kind's body is whole to
/// within the floating-point margin the check takes off: with no measured range among the
/// relation's residuals it is told of that margin, never to "narrow the range's number" (before
/// the mend: 443 of the review's 500 such kinds); with a ranged residual it is told of its range.
#[test]
fn a_refusal_on_the_edge_names_the_cause_it_has() {
    let edge = |make: &dyn Fn(f64) -> Kind, mut ok: f64, mut bad: f64| -> Error {
        assert!(resolve(&make(ok), None).is_ok() && resolve(&make(bad), None).is_err(), "the edge lies between {ok} and {bad}");
        loop {
            let mid = 0.5 * (ok + bad);
            if mid == ok || mid == bad {
                break;
            }
            if resolve(&make(mid), None).is_ok() {
                ok = mid;
            } else {
                bad = mid;
            }
        }
        resolve(&make(bad), None).unwrap_err()
    };
    let ranged = |share: f64| kind(&format!(r#"{{"kind": 1, "name": "dog", "plan": "box_creature", "about": "a probe", "size": {{"shoulder_height": {{"mean": 0.55, "sd": 0.09, "unit": "m", "own": "x"}}}}, "numbers": {{"leg_girth": {{"share": {share}, "exponent": 1.2, "spread": 0.05, "range": [0.01, 0.5], "own": "x"}}}}}}"#));
    for (make, is_ranged) in [(&(|s| dog(s, 1.2)) as &dyn Fn(f64) -> Kind, false), (&ranged, true)] {
        let e = edge(make, 0.1, 0.135);
        let b = e.relation.as_ref().unwrap_or_else(|| panic!("{e}"));
        assert!(b.sum >= b.margin && b.sum < b.margin + 1e-9, "on the edge, whole to within floating point: {e}");
        assert_eq!(e.message.contains("a measured range narrows a residual"), is_ranged, "{e}");
        assert_eq!(e.message.contains("within the floating-point margin of breaking"), !is_ranged, "{e}");
    }
}

/// Review 4, minor 5: a refusal's value is not rounded onto the limit it is outside of. A size
/// whose mean is 0.00005 m reads 0.00005, not 0.0001 (on its step, the plan's least).
#[test]
fn a_refused_value_is_shown_apart_from_its_limit() {
    let e = resolve(&kind(r#"{"kind": 1, "name": "mite", "plan": "box_creature", "about": "a probe", "size": {"shoulder_height": {"mean": 0.00005, "unit": "m", "own": "x"}}}"#), None).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), ("outside_plan", "size.shoulder_height.mean"), "{e}");
    assert!(e.message.contains("could give shoulder_height 0.00005 m, outside"), "{e}");
}

/// The dearest box creature to check: every number with an exponent of its own (unequal, so the
/// size is searched between its ends), a spread of its own and a measured range, every trait
/// varied: every relation at its most corners and range choices.
fn dearest() -> Kind {
    let mean = 0.6;
    let nums: Vec<String> = BOX
        .numbers()
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let at = n.share * mean;
            format!(r#""{}": {{"own": "x", "share": {}, "exponent": {}, "spread": 0.03, "range": [{}, {}]}}"#, n.name, n.share, 0.95 + 0.03 * i as f64, at * 0.6, at * 1.6)
        })
        .collect();
    let vary: Vec<String> = BOX.traits().iter().map(|t| format!(r#""{}": {{"spread": 0.04, "own": "x"}}"#, t.name)).collect();
    kind(&format!(
        r#"{{"kind": 1, "name": "dear", "plan": "box_creature", "about": "a probe", "size": {{"shoulder_height": {{"mean": {mean}, "sd": 0.05, "unit": "m", "own": "x"}}}}, "numbers": {{{}}}, "vary": {{{}}}}}"#,
        nums.join(", "),
        vary.join(", ")
    ))
}

/// The reach's work is counted as it is done: the dearest box creature is accepted on its merits
/// under a ceiling of exactly the work it takes, and refused `reach_too_dear` one evaluation
/// under it, naming the work and the ceiling; it is well under the library's ceiling.
#[test]
fn a_kind_past_the_reach_work_is_refused_by_name() {
    let r = resolve(&dearest(), None).unwrap_or_else(|e| panic!("the dearest kind: {e}"));
    let (bounds, work) = r.reach_within(u64::MAX).unwrap();
    assert!(work > 0 && work * 4 <= crate::draw::MAX_REACH_WORK, "{work} evaluations");
    assert_eq!(r.reach_within(work).unwrap(), (bounds, work));
    let e = r.reach_within(work - 1).unwrap_err();
    assert_eq!(e.code, "reach_too_dear", "{e}");
    let w = e.work.as_ref().unwrap();
    assert!(w.what == "evaluations" && w.done == work && w.ceiling == work - 1, "{w:?}");
    let j = e.to_json();
    assert!(j.get("work").is_some() && j.get("max_work").is_some(), "{e}");
}

const fn probe_trait(name: &'static str) -> TraitDef {
    TraitDef { name, spread: 0.001, about: "a probe" }
}
static MANY_TRAITS: [TraitDef; 13] = [
    probe_trait("t0"),
    probe_trait("t1"),
    probe_trait("t2"),
    probe_trait("t3"),
    probe_trait("t4"),
    probe_trait("t5"),
    probe_trait("t6"),
    probe_trait("t7"),
    probe_trait("t8"),
    probe_trait("t9"),
    probe_trait("t10"),
    probe_trait("t11"),
    probe_trait("t12"),
];
const MANY_WEIGHTS: &[(&str, f64)] = &[("t0", 0.1), ("t1", 0.1), ("t2", 0.1), ("t3", 0.1), ("t4", 0.1), ("t5", 0.1), ("t6", 0.1), ("t7", 0.1), ("t8", 0.1), ("t9", 0.1), ("t10", 0.1), ("t11", 0.1), ("t12", 0.1)];
static MANY_NUMBERS: [NumberDef; 2] = [
    NumberDef { name: "outer", unit: Unit::Metre, share: 1.0, exponent: 1.0, traits: MANY_WEIGHTS, range: [0.001, 60.0], about: "a probe" },
    NumberDef { name: "inner", unit: Unit::Metre, share: 0.5, exponent: 1.0, traits: MANY_WEIGHTS, range: [0.001, 60.0], about: "a probe" },
];
static MANY_RELATIONS: [RelationDef; 1] = [RelationDef { name: "inside", terms: &[("outer", 1.0), ("inner", -1.0)], margin: 0.0003, yields: "inner", about: "a probe" }];

/// A plan of a test's own, at the corners' ceiling: one relation of two numbers that follow
/// thirteen traits (2^13 corners of the residuals, 2^14 of each number's windows).
struct Many;
impl Plan for Many {
    fn name(&self) -> &'static str {
        "many_traits"
    }
    fn about(&self) -> &'static str {
        "a probe"
    }
    fn size(&self) -> &'static SizeDef {
        BOX.size()
    }
    fn numbers(&self) -> &'static [NumberDef] {
        &MANY_NUMBERS
    }
    fn traits(&self) -> &'static [TraitDef] {
        &MANY_TRAITS
    }
    fn roles(&self) -> &'static [RoleDef] {
        &[]
    }
    fn options(&self) -> &'static [OptionDef] {
        &[]
    }
    fn motions(&self) -> &'static [MotionDef] {
        &[]
    }
    fn relations(&self) -> &'static [RelationDef] {
        &MANY_RELATIONS
    }
    fn write(&self, _: &Specimen, _: &mut Writing) -> Result<(), Error> {
        Ok(())
    }
}
static MANY: Many = Many;

fn many(numbers: &str) -> Resolved {
    let kind = kind(&format!(r#"{{"kind": 1, "name": "probe", "plan": "many_traits", "about": "a probe"{numbers}}}"#));
    Resolved { kind, plan: &MANY, base: None }
}

/// The reach at its ceiling is cheap enough for the engine thread: on the plan of thirteen
/// traits (the dearest evaluation: thirteen factors a number), a kind whose relation is least
/// inside the size's window, so the reach would search the size at each of its 8,192 corners, is
/// refused `reach_too_dear` having done [`crate::draw::MAX_REACH_WORK`] evaluations (and at most
/// one corner's more), in under 2 ms.
///
/// Measured on the development laptop (2026-11-28, quiet; the tests' own build and `--release`
/// alike, the crate being built at opt-level 2 in both), `EXEMPLARIA_SHOW_REACH=1`: at the
/// ceiling (1,025 evaluations) 0.84 to 0.99 ms in six runs, about 0.9 µs an evaluation; 1,024 of
/// the relation's own evaluations, by difference, 0.3 to 1.7 ms (two runs of 30 ms each, so
/// noisy); the same kind without a ceiling 0.63 to 0.70 s; the dearest box creature accepted
/// (every number ranged, spread and given its own exponent, every trait varied) 190 evaluations
/// in 0.15 to 0.17 ms. Not measured under load.
#[test]
#[allow(clippy::disallowed_methods, reason = "a speed budget measures wall-clock time; nothing drawn reads it")]
fn the_reach_at_its_ceiling_is_cheap() {
    const MS: u64 = 2;
    let r = many(r#", "numbers": {"outer": {"share": 1.0, "exponent": 2.0, "own": "x"}, "inner": {"share": 2.0, "own": "x"}}"#);
    let start = std::time::Instant::now();
    let e = r.reach_within(crate::draw::MAX_REACH_WORK).unwrap_err();
    let took = start.elapsed();
    let w = e.work.as_ref().unwrap_or_else(|| panic!("{e}"));
    assert!(e.code == "reach_too_dear" && w.what == "evaluations" && w.done > w.ceiling && w.done <= w.ceiling + 4_098, "{e}");
    if std::env::var_os("EXEMPLARIA_SHOW_REACH").is_some() {
        let start = std::time::Instant::now();
        let all = r.reach_within(u64::MAX).map(|x| x.1).map_err(|e| e.message);
        let whole = start.elapsed();
        let d = resolve(&dearest(), None).unwrap();
        let start = std::time::Instant::now();
        let (_, dear) = d.reach_within(u64::MAX).unwrap();
        let box_took = start.elapsed();
        // (The numbers' 32,768 corners first, then 1,024 of the relation's evaluations.)
        let time = |c: u64| {
            let s = std::time::Instant::now();
            let _ = r.reach_within(c);
            s.elapsed()
        };
        let relation = time(32_768 + 1_024).saturating_sub(time(32_768));
        panic!("at the ceiling: {} evaluations in {took:?}; 1,024 of the relation's evaluations: {relation:?}; the whole reach without a ceiling: {all:?} in {whole:?}; the dearest accepted box creature: {dear} evaluations in {box_took:?}", w.done);
    }
    assert!(took < specula_core::speed_budget(std::time::Duration::from_millis(MS)), "{} evaluations in {took:?}", w.done);
}

/// A relation past its corners' ceiling is refused by that name before any corner is visited:
/// on the plan of thirteen traits, a kind with no spread of its own has 2^13 corners (checked
/// on its merits), and one with a spread on each number 2^15 (refused).
#[test]
fn a_relation_past_its_corners_is_refused_by_name() {
    many("").reach_within(u64::MAX).unwrap_or_else(|e| panic!("2^13 corners: {e}"));
    let e = many(r#", "numbers": {"outer": {"share": 1.0, "spread": 0.01, "own": "x"}, "inner": {"share": 0.5, "spread": 0.01, "own": "x"}}"#).reach_within(u64::MAX).unwrap_err();
    let w = e.work.as_ref().unwrap_or_else(|| panic!("{e}"));
    assert_eq!((e.code, w.what, w.done, w.ceiling), ("reach_too_dear", "corners", 1 << 15, crate::draw::MAX_RELATION_CORNERS as u64), "{e}");
    let e = resolve_among(&many(r#", "numbers": {"outer": {"share": 1.0, "spread": 0.01, "own": "x"}, "inner": {"share": 0.5, "spread": 0.01, "own": "x"}}"#).kind, None, &[&MANY]).unwrap_err();
    assert_eq!(e.code, "reach_too_dear", "resolved: {e}");
}

/// A random kind of the box creature, weighted to the relations' hard cases (the review's
/// generator): any size from 0.2 to 3 m, its numbers' means half to 1.8 times the plan's, half of
/// them with an exponent of their own from -1 to 3, spreads to 0.3 and measured ranges.
fn random_text(g: &mut specula_core::rng::Rng, name: &str) -> String {
    let mut u = || (g.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
    let r = |a: f64, b: f64, u: &mut dyn FnMut() -> f64| a + (b - a) * u();
    let mut parts = vec![format!(r#""kind": 1, "name": "{name}", "plan": "box_creature", "about": "a probe""#)];
    let mut smean = BOX.size().mean;
    if u() < 0.85 {
        smean = r(0.2, 3.0, &mut u);
        let mut row = vec![format!(r#""mean": {smean}, "unit": "m", "own": "x""#)];
        let sd = if u() < 0.85 { Some(r(0.0, 0.3, &mut u) * smean / 2.5) } else { None };
        if let Some(sd) = sd {
            row.push(format!(r#""sd": {sd}"#));
        }
        if u() < 0.35 || sd.is_none() {
            row.push(format!(r#""range": [{}, {}]"#, smean * r(0.5, 0.99, &mut u), smean * r(1.01, 1.6, &mut u)));
        }
        parts.push(format!(r#""size": {{"shoulder_height": {{{}}}}}"#, row.join(", ")));
    }
    let mut nums = Vec::new();
    for n in BOX.numbers() {
        if u() >= 0.75 {
            continue;
        }
        let m = n.share * smean * r(0.5, 1.8, &mut u);
        let mut row = vec![format!(r#""own": "x", "mean": {m}"#)];
        if u() < 0.5 {
            row.push(format!(r#""exponent": {}"#, r(-1.0, 3.0, &mut u)));
        }
        if u() < 0.55 {
            row.push(format!(r#""spread": {}"#, r(0.0, 0.3, &mut u)));
        }
        if u() < 0.4 {
            row.push(format!(r#""range": [{}, {}]"#, m * r(0.4, 0.99, &mut u), m * r(1.01, 2.5, &mut u)));
        }
        nums.push(format!(r#""{}": {{{}}}"#, n.name, row.join(", ")));
    }
    if !nums.is_empty() {
        parts.push(format!(r#""numbers": {{{}}}"#, nums.join(", ")));
    }
    let vary: Vec<String> = BOX.traits().iter().filter_map(|t| if u() < 0.7 { Some(format!(r#""{}": {{"spread": {}, "own": "x"}}"#, t.name, r(0.0, 0.36, &mut u))) } else { None }).collect();
    if !vary.is_empty() {
        parts.push(format!(r#""vary": {{{}}}"#, vary.join(", ")));
    }
    format!("{{{}}}", parts.join(", "))
}

/// How strict the relation check is, measured: of 300 random kinds refused by a relation alone
/// (every number inside its range at every seed), how many are whole by an exact check of that
/// relation over every seed. Where no measured range narrows one of the relation's residuals the
/// check is exact, and **none** is (before the mend: 56 of 222 kinds with unequal exponents, by
/// the review's own count). Where a range does, the check is a bound, and **at most a twentieth** are
/// (measured: 2 of 104). Every refusal's body, put back through the
/// draw's arithmetic, gives what the refusal reports, and the bound is not above it.
#[test]
fn a_kind_refused_by_a_relation_has_a_seed_that_breaks_it() {
    let mut g = specula_core::rng::Rng::new(77, 0);
    let (mut exact_cases, mut exact_whole, mut ranged_cases, mut ranged_whole, mut unequal) = (0, 0, 0, 0, 0);
    let mut tried = 0;
    while exact_cases + ranged_cases < 300 {
        tried += 1;
        assert!(tried < 100_000, "only {} refusals in {tried} kinds", exact_cases + ranged_cases);
        let k = kind(&random_text(&mut g, &format!("st{tried}")));
        let Err(e) = resolve(&k, None) else { continue };
        let Some(b) = &e.relation else { continue };
        if resolve_among(&k, None, &[&FREE]).is_err() {
            continue;
        }
        reproduces(&k, b).unwrap_or_else(|m| panic!("{m}: {e}"));
        let rel = BOX.relations().iter().find(|r| r.name == b.relation).unwrap();
        let exponent = |n: &str| k.numbers.get(n).and_then(|s| s.exponent).unwrap_or(1.0);
        if rel.terms.windows(2).any(|w| exponent(w[0].0) != exponent(w[1].0)) {
            unequal += 1;
        }
        let ranged = rel.terms.iter().any(|(n, _)| k.numbers.get(*n).is_some_and(|s| s.spread.unwrap_or(0.0) > 0.0 && s.range.is_some()));
        let whole = exact_least(&k, rel) >= rel.margin;
        if ranged {
            ranged_cases += 1;
            ranged_whole += usize::from(whole);
        } else {
            exact_cases += 1;
            assert!(!whole, "whole by the exact check ({}), refused: {e}", exact_least(&k, rel));
            assert!((b.bound - b.sum).abs() < 1e-6, "the bound {} far from the body's sum {}: {e}", b.bound, b.sum);
            exact_whole += usize::from(whole);
        }
    }
    assert!(unequal >= 100, "{unequal} of the refusals have unequal exponents");
    assert_eq!(exact_whole, 0);
    assert!(ranged_whole * 20 <= ranged_cases, "{ranged_whole} of {ranged_cases} refused kinds with a ranged residual are whole");
    if std::env::var_os("EXEMPLARIA_SHOW_STRICTNESS").is_some() {
        panic!("refused by a relation alone: {exact_cases} exact cases (whole: {exact_whole}), {ranged_cases} with a ranged residual (whole: {ranged_whole}); {unequal} with unequal exponents; {tried} kinds tried");
    }
}
