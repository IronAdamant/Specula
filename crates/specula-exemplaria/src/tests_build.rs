//! Specimens built by the modeller (review 2 of S0, finding 1): random accepted kinds at random
//! seeds, kinds written from published figures, and the plan at the limits of its ranges, each
//! body closed, of the volume of its boxes, with no two parts sharing a face and every leg under
//! its own corner; and the backstops counted: they never act on an accepted kind.

use crate::kind::Kind;
use crate::plan::resolve;
use crate::write::{expand, Roles};
use crate::Resolved;
use specula_core::Json;
use specula_model::{build, BuildParams, Style};

/// A style with the plan's roles as presets of their own names.
pub(crate) const STYLE: &str = r##"{ "style": "probe", "materials": { "coat": { "color": "#7a6a50" }, "claw": { "color": "#302820" }, "eye": { "color": "#d0a040" } } }"##;

pub(crate) struct ByStyle<'a>(pub(crate) &'a Style);

impl Roles for ByStyle<'_> {
    fn preset(&self, role: &str, hint: &str) -> Result<String, String> {
        self.0.role(role, Some(hint)).map(str::to_string).map_err(|e| e.to_string())
    }
}

/// Two faces of two parts closer than this are one face (the written grid is 0.05 mm).
const SAME: f64 = 2e-5;

/// Each part of a written recipe as its box: `[lo, hi]` on x, y and z (a block is a box standing
/// on its bottom face's centre, moved).
fn boxes(recipe: &str) -> Vec<(String, [[f64; 2]; 3])> {
    let j = Json::parse(recipe).unwrap();
    let num = |v: &Json, i: usize| v.as_array().unwrap()[i].as_f64().unwrap();
    j.get("parts")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .map(|p| {
            let shape = p.get("shape").unwrap();
            assert_eq!(shape.get("op").and_then(Json::as_str), Some("box"));
            let size = shape.get("size").unwrap();
            let by = p.get("ops").and_then(Json::as_array).unwrap()[0].get("by").unwrap().clone();
            let (sx, sy, sz) = (num(size, 0), num(size, 1), num(size, 2));
            let (x, y, z) = (num(&by, 0), num(&by, 1), num(&by, 2));
            (p.get("name").and_then(Json::as_str).unwrap().to_string(), [[x - sx / 2.0, x + sx / 2.0], [y, y + sy], [z - sz / 2.0, z + sz / 2.0]])
        })
        .collect()
}

/// No two parts share a face: two boxes either stand apart (a gap on some axis) or, where they
/// meet, no face of one lies in the plane of a face of the other (overlapping parts cross through
/// each other's faces, never along them). And every box has a volume.
pub(crate) fn no_shared_face(recipe: &str) -> Result<f64, String> {
    let b = boxes(recipe);
    let mut volume = 0.0;
    for (name, [x, y, z]) in &b {
        let v = (x[1] - x[0]) * (y[1] - y[0]) * (z[1] - z[0]);
        if !(x[1] - x[0] > SAME && y[1] - y[0] > SAME && z[1] - z[0] > SAME) {
            return Err(format!("{name} has no volume: {x:?} {y:?} {z:?}"));
        }
        volume += v;
    }
    for i in 0..b.len() {
        for j in i + 1..b.len() {
            let (p, q) = (&b[i].1, &b[j].1);
            let apart = (0..3).any(|k| p[k][1] < q[k][0] - SAME || q[k][1] < p[k][0] - SAME);
            if apart {
                continue;
            }
            for k in 0..3 {
                for fp in p[k] {
                    for fq in q[k] {
                        if (fp - fq).abs() <= SAME {
                            return Err(format!("{} and {} share a face on axis {k} at {fp}", b[i].0, b[j].0));
                        }
                    }
                }
            }
        }
    }
    Ok(volume)
}

/// The mesh's signed volume (positive: closed and facing out).
pub(crate) fn mesh_volume(m: &specula_model::MeshData) -> f64 {
    let p = |i: u32| m.positions[i as usize].map(f64::from);
    m.indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| {
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6.0
        })
        .sum()
}

/// One specimen built and held to everything the plan promises; its description on failure.
fn whole(r: &Resolved, seed: Option<u64>, style: &Style) -> Result<(), String> {
    let x = expand(r, seed, &ByStyle(style), "t").map_err(|e| format!("{e}"))?;
    let model = build(&x.recipe, &BuildParams { seed, ..BuildParams::new(style) }).map_err(|e| format!("{e}\n{}", x.recipe))?;
    if model.open_edges != 0 {
        return Err(format!("{} open edges\n{}", model.open_edges, x.recipe));
    }
    let boxes = no_shared_face(&x.recipe).map_err(|m| format!("{m}\n{}", x.recipe))?;
    let mesh = mesh_volume(&model.lods[0]);
    if !(mesh > 0.0 && (mesh - boxes).abs() <= 1e-4 * boxes) {
        return Err(format!("mesh volume {mesh} against its boxes' {boxes}"));
    }
    if f64::from(model.bounds.min[1]).abs() > 1e-3 {
        return Err(format!("it does not stand on the ground: {}", model.bounds.min[1]));
    }
    // Every leg under its own corner of the trunk (left = +X, front = +Z), from the built rig.
    let s = &x.specimen;
    let (bw, bl) = (s.get("body_width"), s.get("body_length"));
    let rig = model.rig.as_ref().ok_or("no rig")?;
    for (leg, sx, sz) in [("lf", 1.0, 1.0), ("rf", -1.0, 1.0), ("lh", 1.0, -1.0), ("rh", -1.0, -1.0)] {
        let thigh = rig.bones.iter().find(|b| b.name == format!("thigh_{leg}")).ok_or("no thigh")?;
        let [hx, _, hz] = thigh.head.map(f64::from);
        if !(hx * sx > 0.0 && hz * sz > 0.0 && hx.abs() < bw / 2.0 && hz.abs() < bl / 2.0) {
            return Err(format!("thigh_{leg} at ({hx}, {hz}) is not under its corner of a trunk {bw} wide and {bl} long"));
        }
    }
    Ok(())
}

/// Review 2, finding 1: thousands of random accepted kinds (any size from a mouse's to past an
/// elephant's, shares, exponents, spreads and ranges far from the plan's defaults, a fifth over a
/// base) at random seeds, each built by the modeller: every one closed, of its boxes' volume, no
/// two parts sharing a face, each leg under its own corner, and no backstop acting.
#[test]
fn random_accepted_kinds_build_whole_at_random_seeds() {
    let style = Style::parse(STYLE).unwrap();
    let kinds = crate::tests::accepted_random_kinds(1_000, 500_000);
    let mut g = specula_core::rng::Rng::new(3, 0);
    let before = crate::draw::backstops::counts();
    let mut built = 0;
    let mut sizes = (f64::INFINITY, 0.0f64);
    for (i, r) in kinds.iter().enumerate() {
        for k in 0..3 {
            let seed = if i == 0 && k == 0 { crate::draw::MAX_SEED } else { g.next_u64() >> 11 };
            whole(r, Some(seed), &style).unwrap_or_else(|m| panic!("kind {i} ({}) seed {seed}: {m}\n{}", r.kind.name, r.kind.to_json()));
            built += 1;
        }
        let h = r.draw(None).size.value;
        sizes = (sizes.0.min(h), sizes.1.max(h));
    }
    let after = crate::draw::backstops::counts();
    assert_eq!((after.0 - before.0, after.1 - before.1), (0, 0), "the backstops acted (held, kept) over {built} builds");
    assert!(sizes.0 < 0.06 && sizes.1 > 8.0, "the kinds span sizes {sizes:?}");
}

/// A kind of the test plan from a size row and fields, every number `source` or `own`.
fn kind(name: &str, size: &str, rest: &str) -> Resolved {
    let text = format!(r#"{{"kind": 1, "name": "{name}", "plan": "box_creature", "about": "a box creature of measured size", "size": {{"shoulder_height": {size}}}{rest}}}"#);
    resolve(&Kind::parse(&text).unwrap_or_else(|e| panic!("{name}: {e}")), None).unwrap_or_else(|e| panic!("{name} is refused: {e}"))
}

/// Review 2, finding 6: kinds written from published figures (research C1 and the round-seven
/// check), with lengths' coefficients of variation of 4 to 12 per cent, are accepted and build
/// whole at 16 seeds each. (The box creature is not any of these animals: these are its numbers
/// at their sizes and spreads, the test that the plan's ranges are its geometry's and not a
/// typical size.)
#[test]
fn kinds_of_published_sizes_are_accepted_and_build_whole() {
    let style = Style::parse(STYLE).unwrap();
    let legs = |cv: f64| format!(r#""legs": {{"spread": {cv}, "own": "a length's spread of {cv}: research R7-CHECK (f), lengths' CV 4 to 12 per cent"}}"#);
    let kinds = [
        // A cat: Poradowski et al. 2024, Table 1 (all cats, n 37: 25.98 cm, SD 2.57: CV 9.9);
        // the trunk as Day and Jayne 2007's intergirdle distance (32 cm, range 26 to 44).
        kind(
            "cat",
            r#"{"mean": 0.2598, "sd": 0.0257, "unit": "m", "source": "Poradowski et al. 2024, Vet. Sci. 11:522, Table 1, all cats (research C1, 1.3)"}"#,
            &format!(r#", "numbers": {{"body_length": {{"mean": 0.32, "spread": 0.1, "source": "Day and Jayne 2007, J. Exp. Biol. 210:642, Table 1, intergirdle distance 32 cm, range 26 to 44 (research C1, 1.3)"}}}}, "vary": {{"build": {{"spread": 0.08, "own": "a girth's spread of 8 in a hundred"}}, {}}}"#, legs(0.06)),
        ),
        // The reviewer's small cat and the cat a little larger.
        kind("cat_small", r#"{"mean": 0.26, "sd": 0.0257, "unit": "m", "own": "a cat's size with Poradowski's spread"}"#, ""),
        kind("cat_large", r#"{"mean": 0.31, "sd": 0.0307, "unit": "m", "own": "CV 9.9"}"#, ""),
        // A wolf: Gatta et al. 2016, p. 948 (62.9 cm, range 57.9 to 67.2; no spread printed: a
        // quarter of the range); its tail 32.9 per cent of its body length (round seven).
        kind(
            "wolf",
            r#"{"mean": 0.629, "range": [0.579, 0.672], "unit": "m", "source": "Gatta et al. 2016, C. R. Palevol 15:941, p. 948 (research C1, 1.2)"}"#,
            &format!(r#", "numbers": {{"tail_length": {{"share": 0.55, "spread": 0.06, "own": "32.9 per cent of a trunk of 1.05 m (research R7-CHECK (f)), on the box creature's shorter trunk"}}}}, "vary": {{"build": {{"spread": 0.25, "own": "the reviewer's widest wolf"}}, {}}}"#, legs(0.048)),
        ),
        // A large dog: 0.75 m, CV 8 (no breed's withers are delivered: the engine's own).
        kind("large_dog", r#"{"mean": 0.75, "sd": 0.06, "unit": "m", "own": "a large dog's withers, CV 8: no breed table delivered (research C1, Not found)"}"#, &format!(r#", "vary": {{{}}}"#, legs(0.08))),
        // A horse-like kind: Möller 2017's radius (37.24 cm, SD 3.59, n 194) times May 1985's
        // factor 4.111: 1.531 m, CV 9.6; the reviewer's horse-like 0.9 m at CV 12 with build 0.2.
        kind(
            "horse",
            r#"{"mean": 1.531, "sd": 0.147, "unit": "m", "source": "Moller 2017 Tabla 1 radius 37.24 cm SD 3.59 times May 1985's withers factor 4.111 (research C1, 1.5)"}"#,
            &format!(r#", "numbers": {{"leg_length": {{"share": 0.6, "spread": 0.05, "own": "a horse's legs a little over half its withers"}}, "neck_length": {{"share": 0.42, "source": "a horse's neck 0.42 of its withers (research R7-CHECK (f))"}}}}, "vary": {{{}}}"#, legs(0.04)),
        ),
        kind("horse_like", r#"{"mean": 0.9, "sd": 0.108, "unit": "m", "own": "CV 12"}"#, r#", "vary": {"build": {"spread": 0.2, "own": "the reviewer's"}}"#),
        kind("tall", r#"{"mean": 1.1, "sd": 0.044, "unit": "m", "own": "CV 4"}"#, ""),
        kind("small", r#"{"mean": 0.33, "sd": 0.0198, "unit": "m", "own": "CV 6"}"#, ""),
        // An Asian elephant, male: Larramendi 2016, Table 8, Grade I, 2.61 to 2.89 m (mean 2.75).
        kind("elephant", r#"{"mean": 2.75, "range": [2.61, 2.89], "unit": "m", "source": "Larramendi 2016, Acta Palaeontol. Pol. 61:537, Table 8, E. maximus males (research C1)"}"#, r#", "vary": {"build": {"spread": 0.1, "own": "a girth's spread"}}"#),
        // A small rodent's size (5 cm at the shoulder; the engine's own: no rodent is delivered).
        kind("rodent", r#"{"mean": 0.05, "sd": 0.003, "unit": "m", "own": "a small rodent's size, the engine's own"}"#, r#", "vary": {"build": {"spread": 0.05, "own": "x"}}"#),
    ];
    let before = crate::draw::backstops::counts();
    for r in &kinds {
        for seed in (0..16u64).map(|k| k * 104_729 + 3) {
            whole(r, Some(seed), &style).unwrap_or_else(|m| panic!("{} seed {seed}: {m}", r.kind.name));
        }
        whole(r, None, &style).unwrap_or_else(|m| panic!("{} at its mean: {m}", r.kind.name));
    }
    assert_eq!(crate::draw::backstops::counts(), before, "no backstop acted");
}

/// Review 3, finding 2: a size has no limit of its own. A mouse of 3 cm at the shoulder (spread
/// 2 mm) whose numbers are all set by their own means (each well above its least) is accepted and
/// builds whole at its mean and 16 seeds; before, it was refused for its size alone, because
/// 0.039 m is where the plan's default shares put a girth at its least.
#[test]
fn a_mouse_whose_numbers_are_its_own_is_accepted_and_builds_whole() {
    let style = Style::parse(STYLE).unwrap();
    let means = [("body_length", 0.08), ("body_depth", 0.025), ("body_width", 0.03), ("leg_length", 0.02), ("leg_girth", 0.005), ("neck_length", 0.02), ("head_length", 0.025), ("head_depth", 0.015), ("tail_length", 0.08)];
    let rows: Vec<String> = means.iter().map(|(n, m)| format!(r#""{n}": {{"mean": {m}, "own": "a mouse's, the engine's own"}}"#)).collect();
    let mouse = kind("mouse", r#"{"mean": 0.03, "sd": 0.002, "unit": "m", "own": "a mouse's, the engine's own"}"#, &format!(r#", "numbers": {{{}}}"#, rows.join(", ")));
    let before = crate::draw::backstops::counts();
    for seed in std::iter::once(None).chain((0..16u64).map(|k| Some(k * 7_919 + 1))) {
        whole(&mouse, seed, &style).unwrap_or_else(|m| panic!("mouse {seed:?}: {m}"));
    }
    assert_eq!(crate::draw::backstops::counts(), before, "no backstop acted");
}

/// The plan's ranges are where its geometry breaks, found by building at them: the smallest body
/// its numbers allow (each at its least) and
/// the largest (lengths of 59 m), the size's least and most at the plan's default shares, all
/// whole; and just past each least, the reach refuses it.
#[test]
fn the_box_creature_builds_whole_at_its_limits() {
    let style = Style::parse(STYLE).unwrap();
    let plan = &crate::plans::box_creature::PLAN;
    let all = |f: &dyn Fn(&crate::plan::NumberDef) -> f64| {
        let rows: Vec<String> = crate::Plan::numbers(plan).iter().map(|n| format!(r#""{}": {{"mean": {}, "own": "x"}}"#, n.name, f(n))).collect();
        format!(r#", "numbers": {{{}}}, "vary": {{"build": {{"spread": 0.0, "own": "x"}}, "legs": {{"spread": 0.0, "own": "x"}}, "tail": {{"spread": 0.0, "own": "x"}}}}"#, rows.join(", "))
    };
    let fixed = r#"{"mean": 0.6, "unit": "m", "own": "x"}"#;
    let smallest = kind("smallest", fixed, &all(&|n| n.range[0]));
    let largest = kind("largest", fixed, &all(&|n| if n.name == "leg_girth" { 15.0 } else { 59.0 }));
    // The size has no limit of its own: at the plan's default shares its least and most are
    // where a number reaches its own (the girth's 3 mm at 0.09 of 0.039 m, less its build; the
    // trunk's 60 m at 1.2 of 50 m), and just past them the number is named.
    let at_size = |name: &str, size: f64| Kind::parse(&format!(r#"{{"kind": 1, "name": "{name}", "plan": "box_creature", "about": "x", "size": {{"shoulder_height": {{"mean": {size}, "unit": "m", "own": "x"}}}}}}"#)).unwrap();
    let least = resolve(&at_size("least_size", 0.039), None).unwrap();
    let most = resolve(&at_size("most_size", 50.0), None).unwrap();
    for (size, number) in [(0.038, "leg_girth"), (50.01, "body_length")] {
        let e = resolve(&at_size("past", size), None).unwrap_err();
        assert_eq!((e.code, e.reach.as_ref().map(|r| r.number.as_str())), ("outside_plan", Some(number)), "{e}");
    }
    for r in [&smallest, &largest, &least, &most] {
        for seed in [None, Some(1), Some(2)] {
            whole(r, seed, &style).unwrap_or_else(|m| panic!("{} {seed:?}: {m}", r.kind.name));
        }
    }
    let tiny = smallest.draw(None);
    assert!(tiny.get("leg_girth") == 0.003 && tiny.get("leg_length") == 0.01, "{tiny:?}");
    // One step under a number's least: refused, by name.
    for n in crate::Plan::numbers(plan) {
        let text = format!(r#"{{"kind": 1, "name": "under", "plan": "box_creature", "about": "x", "numbers": {{"{}": {{"mean": {}, "own": "x"}}}}}}"#, n.name, n.range[0] - 0.0001);
        let e = resolve(&Kind::parse(&text).unwrap(), None).unwrap_err();
        assert_eq!((e.code, e.path.clone()), ("outside_plan", format!("numbers.{}.mean", n.name)), "{e}");
    }
}
