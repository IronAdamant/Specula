//! The kind format's refusals, the draws' spread, bases, the plans' tables and hostile input.

use crate::kind::{Kind, MAX_KIND_BYTES};
use crate::library::{self, ENTRIES};
use crate::plan::{resolve, Unit};
use crate::write::{expand, AsNamed, Roles};
use crate::{gaits, plans};


fn parse_err(text: &str) -> crate::Error {
    Kind::parse(text).expect_err(text)
}

/// A small kind of the test plan, with `extra` fields spliced in.
fn small(extra: &str) -> String {
    format!(r#"{{"kind": 1, "name": "probe", "plan": "box_creature", "about": "a probe"{extra}}}"#)
}

#[test]
fn the_library_kind_parses_resolves_and_writes() {
    let e = library::entry("box_creature").expect("box_creature");
    let r = e.resolved().unwrap();
    assert_eq!(r.plan.name(), "box_creature");
    let x = expand(&r, Some(7), &AsNamed, &e.from()).unwrap();
    assert!(x.recipe.contains("\"seed\": 7") && x.recipe.contains("seed 7") && x.recipe.contains(&e.from()), "{}", x.recipe);
    assert_eq!(x.counts.bones, 13, "root, body, 8 leg bones, neck, head, tail");
    assert_eq!(x.counts.clips, 3);
    assert_eq!(x.roles.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), ["claw", "coat", "eye"]);
    assert!(library::find("box_beast").is_some_and(|f| f.name() == "box_creature"), "an alias finds it");
    assert!(e.from().starts_with("exemplaria/box_creature/box_creature@"));
}

#[test]
fn every_number_needs_a_source_or_an_own_and_not_both() {
    let none = parse_err(&small(r#", "numbers": {"leg_length": {"share": 0.6}}"#));
    assert_eq!((none.code, none.path.as_str()), ("no_source", "numbers.leg_length"));
    let both = parse_err(&small(r#", "numbers": {"leg_length": {"share": 0.6, "source": "a", "own": "b"}}"#));
    assert_eq!(both.code, "two_sources");
    let blank = parse_err(&small(r#", "numbers": {"leg_length": {"share": 0.6, "own": "  "}}"#));
    assert_eq!(blank.code, "no_source");
    for (field, row) in [("size", r#"{"shoulder_height": {"mean": 0.6, "unit": "m"}}"#), ("vary", r#"{"build": {"spread": 0.05}}"#), ("motion", r#"{"walk": {"froude": 0.3}}"#)] {
        let e = parse_err(&small(&format!(r#", "{field}": {row}"#)));
        assert_eq!(e.code, "no_source", "{field}: {e}");
        assert!(e.path.starts_with(field), "{e}");
    }
    // A clip that sets no number needs no source.
    assert!(Kind::parse(&small(r#", "motion": {"idle": {}}"#)).is_ok());
}

#[test]
fn parsing_refuses_with_paths() {
    let cases: &[(&str, &str, &str)] = &[
        (r#"{"name": "x", "plan": "box_creature", "about": "a"}"#, "missing_field", "kind"),
        (r#"{"kind": 2, "name": "x", "plan": "box_creature", "about": "a"}"#, "out_of_range", "kind"),
        (r#"{"kind": 1, "name": "X-1", "plan": "box_creature", "about": "a"}"#, "bad_name", "name"),
        (r#"{"kind": 1, "name": "x", "plan": "box_creature"}"#, "missing_field", "about"),
        ("{", "json", ""),
        ("[1]", "wrong_type", ""),
    ];
    for (text, code, path) in cases {
        let e = parse_err(text);
        assert_eq!((e.code, e.path.as_str()), (*code, *path), "{text}: {e}");
    }
    let rows: &[(&str, &str, &str)] = &[
        (r#", "numbers": {"leg_length": {"share": 0.6, "mean": 0.3, "own": "x"}}"#, "missing_field", "numbers.leg_length"),
        (r#", "numbers": {"leg_length": {"share": -1, "own": "x"}}"#, "out_of_range", "numbers.leg_length.share"),
        (r#", "numbers": {"leg_length": {"share": 0.6, "owns": "x"}}"#, "unknown_field", "numbers.leg_length.owns"),
        (r#", "size": {"shoulder_height": {"mean": 0.6, "range": [0.7, 0.9], "unit": "m", "own": "x"}}"#, "out_of_range", "size.shoulder_height.range"),
        (r#", "roles": {"coat": "brown"}"#, "wrong_type", "roles.coat"),
        (r##", "roles": {"a": "#000000", "b": "#000000", "c": "#000000", "d": "#000000", "e": "#000000"}"##, "too_many", "roles"),
        (r#", "levels": {"huge": 10}"#, "unknown_field", "levels.huge"),
        (r#", "levels": {"near": 2.5}"#, "wrong_type", "levels.near"),
        (r#", "colour": 3"#, "unknown_field", "colour"),
        (r#", "name": "again""#, "duplicate_key", "name"),
    ];
    for (extra, code, path) in rows {
        let e = parse_err(&small(extra));
        assert_eq!((e.code, e.path.as_str()), (*code, *path), "{extra}: {e}");
    }
}

#[test]
fn a_kind_is_checked_against_its_plan_with_the_nearest_names() {
    let k = |extra: &str| Kind::parse(&small(extra)).unwrap();
    let e = resolve(&k(r#", "numbers": {"leg_lenght": {"share": 0.6, "own": "x"}}"#), None).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), ("unknown_name", "numbers.leg_lenght"));
    assert_eq!(e.near.first().map(String::as_str), Some("leg_length"), "{e}");
    let e = resolve(&Kind::parse(&small("").replace("box_creature", "box_creatur")).unwrap(), None).unwrap_err();
    assert_eq!((e.code, e.near.first().map(String::as_str)), ("unknown_plan", Some("box_creature")));
    assert_eq!(resolve(&k(r#", "tail_carriage": "sideways""#), None).unwrap_err().code, "unknown_name");
    assert_eq!(resolve(&k(r#", "tail_carriag": "low""#), None).unwrap_err().near.first().map(String::as_str), Some("tail_carriage"));
    assert_eq!(resolve(&k(r#", "motion": {"walk": {"pattern": "gallop"}}"#), None).unwrap_err().path, "motion.walk.pattern");
    assert_eq!(resolve(&k(r#", "motion": {"idle": {"froude": 1, "own": "x"}}"#), None).unwrap_err().code, "unknown_field");
    let e = resolve(&k(r##", "roles": {"cot": "#000000"}"##), None).unwrap_err();
    assert_eq!((e.path.as_str(), e.near.first().map(String::as_str)), ("roles.cot", Some("coat")));
    assert_eq!(resolve(&k(r#", "size": {"height": {"mean": 1, "unit": "m", "own": "x"}}"#), None).unwrap_err().code, "unknown_name");
}

#[test]
fn a_base_is_one_level_of_the_same_plan() {
    let base = Kind::parse(&small(r#", "numbers": {"leg_length": {"share": 0.7, "own": "long"}}, "tail_carriage": "high""#)).unwrap();
    let child = Kind::parse(&small(r#", "numbers": {"body_length": {"share": 1.5, "own": "longer"}}"#).replace(r#""name": "probe""#, r#""name": "child", "base": "probe""#)).unwrap();
    let r = resolve(&child, Some(&base)).unwrap();
    assert_eq!(r.kind.name, "child");
    assert_eq!((r.kind.numbers["leg_length"].share, r.kind.numbers["body_length"].share), (Some(0.7), Some(1.5)), "the base's numbers under the kind's");
    assert_eq!(r.kind.options["tail_carriage"], "high");
    assert_eq!(resolve(&child, None).unwrap_err().code, "base_not_found");
    // A base with a base of its own: refused (and a ring is named as one).
    let mut deep = base.clone();
    deep.base = Some("other".into());
    assert_eq!(resolve(&child, Some(&deep)).unwrap_err().code, "base_too_deep");
    let mut ring = base.clone();
    ring.base = Some("child".into());
    assert_eq!(resolve(&child, Some(&ring)).unwrap_err().code, "base_cycle");
    let mut own = child.clone();
    own.base = Some("child".into());
    let mut same = child.clone();
    same.base = None;
    assert_eq!(resolve(&own, Some(&same)).unwrap_err().code, "base_cycle");
    let mut other = base.clone();
    other.plan = "tree".into();
    assert_eq!(resolve(&child, Some(&other)).unwrap_err().code, "base_other_plan");
    // A copy is the merged kind, with no base: what a project takes.
    let flat = Kind::parse(&r.kind.to_json().to_pretty()).unwrap();
    assert_eq!(flat, r.kind);
}

#[test]
fn hostile_kind_files_are_refused_cheaply() {
    // 1 MB of text: refused by its size, before it is read.
    let big = small(&format!(r#", "about2": "{}""#, "x".repeat(1 << 20)));
    assert_eq!(parse_err(&big).code, "too_large");
    let padded = format!("{}{}", " ".repeat(MAX_KIND_BYTES), small(""));
    assert_eq!(parse_err(&padded).code, "too_large");
    // Deep nesting: the JSON reader's depth limit.
    let deep = small(&format!(r#", "numbers": {}{}"#, "[".repeat(10_000), "]".repeat(10_000)));
    assert_eq!(parse_err(&deep).code, "json");
    let deep_obj = small(&format!(r#", "numbers": {}1{}"#, "{\"a\":".repeat(200), "}".repeat(200)));
    assert_eq!(parse_err(&deep_obj).code, "json");
    // Control characters, in a string or a key.
    assert_eq!(parse_err(&small(r#", "aliases": ["ok", "b\u0007ell"]"#)).code, "control_character");
    assert_eq!(parse_err(&small(r#", "about": "two\nlines""#).replacen(r#""about": "a probe", "#, "", 1)).code, "control_character");
    assert_eq!(parse_err(&small(r#", "k\u001bey": "x""#)).code, "control_character");
    // A long text.
    assert_eq!(parse_err(&small(&format!(r#", "numbers": {{"leg_length": {{"share": 0.6, "own": "{}"}}}}"#, "y".repeat(500)))).code, "too_large");
    // Too many entries.
    let many: Vec<String> = (0..65).map(|i| format!("\"a{i}\"")).collect();
    assert_eq!(parse_err(&small(&format!(r#", "aliases": [{}]"#, many.join(",")))).code, "too_many");
}

/// Every kind and alias of the library, and every plan's tables, as a kind file is held to.
#[test]
fn the_plans_tables_are_sound() {
    for p in plans::PLANS {
        let mut seen = std::collections::BTreeSet::new();
        assert!(crate::words::valid_name(p.name()));
        for n in p.numbers() {
            assert!(crate::words::valid_name(n.name) && seen.insert(n.name), "{}: {}", p.name(), n.name);
            assert!(n.share > 0.0 && (-3.0..=3.0).contains(&n.exponent));
            for (t, _) in n.traits {
                assert!(p.traits().iter().any(|x| x.name == *t), "{}: {} moves an unknown trait {t}", p.name(), n.name);
            }
        }
        assert!(p.roles().len() <= crate::kind::MAX_ROLES);
        for m in p.motions() {
            assert!(specula_anim::gen::KINDS.contains(&m.generator), "{}: {} is not a generator", p.name(), m.generator);
            for pat in m.patterns {
                assert!(gaits::pattern(pat).is_some(), "{pat}");
            }
        }
        let s = p.size();
        assert!(s.range[0] <= s.mean && s.mean <= s.range[1] && s.sd >= 0.0);
    }
    let mut names = std::collections::BTreeSet::new();
    for e in ENTRIES {
        let k = e.kind().unwrap();
        assert_eq!(k.name, e.name(), "a kind's file is named after it");
        assert!(e.path.starts_with(&format!("{}/", k.plan)), "{}: filed under its plan", e.path);
        assert!(names.insert(k.name.clone()), "{}", k.name);
        for a in &k.aliases {
            assert!(names.insert(a.clone()), "alias {a} names two kinds");
        }
        e.resolved().unwrap();
    }
}

#[test]
fn draws_spread_as_the_kind_says_over_a_thousand_seeds() {
    let r = library::entry("box_creature").unwrap().resolved().unwrap();
    let n = 1000;
    let specimens: Vec<_> = (0..n).map(|s| r.draw(Some(s))).collect();
    let stats = |xs: &[f64]| {
        let mean = xs.iter().sum::<f64>() / xs.len() as f64;
        (mean, (xs.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / xs.len() as f64).sqrt())
    };
    let size = r.kind.size.as_ref().unwrap();
    let (mean, sd) = stats(&specimens.iter().map(|s| s.size.value).collect::<Vec<_>>());
    let want = size.sd.unwrap();
    assert!((mean - size.mean).abs() < want / 10.0, "size mean {mean} against {}", size.mean);
    assert!((sd - want).abs() < want / 10.0, "size spread {sd} against {want}");
    for s in &specimens {
        assert!(s.size.value >= size.mean - 2.5 * want - 1e-9 && s.size.value <= size.mean + 2.5 * want + 1e-9, "clipped at 2.5 spreads");
    }
    for (t, v) in &r.kind.vary {
        let (m, sd) = stats(&specimens.iter().map(|s| s.traits.iter().find(|x| x.0 == t).unwrap().1).collect::<Vec<_>>());
        assert!(m.abs() < v.spread / 10.0 && (sd - v.spread).abs() < v.spread / 10.0, "{t}: mean {m}, spread {sd} against {}", v.spread);
    }
    // A number's own spread: the head's length over what size gives it.
    let head = r.kind.numbers["head_length"].spread.unwrap();
    let ratios: Vec<f64> = specimens.iter().map(|s| s.get("head_length") / (0.36 * s.size.value)).collect();
    let (m, sd) = stats(&ratios);
    assert!((m - 1.0).abs() < head / 10.0 && (sd - head).abs() < head / 10.0, "head_length: mean {m}, spread {sd} against {head}");
    // Every drawn number is written rounded to its unit's step.
    for s in specimens.iter().take(50) {
        for v in s.numbers.iter().chain(std::iter::once(&s.size)) {
            assert_eq!(v.value, crate::draw::round(v.value, v.unit), "{}", v.name);
            assert_eq!(v.unit, Unit::Metre);
        }
    }
    // The mean: no seed, every residual zero.
    let mean = r.draw(None);
    assert_eq!(mean.size.value, size.mean);
    assert!(mean.traits.iter().all(|t| t.1 == 0.0));
    assert_eq!(mean.get("leg_length"), crate::draw::round(0.62 * 0.6, Unit::Metre));
}

#[test]
fn a_number_added_to_a_kind_moves_no_other_draw() {
    let r = library::entry("box_creature").unwrap().resolved().unwrap();
    let mut more = r.clone();
    more.kind.numbers.insert("neck_length".into(), crate::kind::Number { share: Some(0.3), mean: None, exponent: None, spread: Some(0.1), range: None, why: crate::Why::Own("a probe".into()) });
    for seed in 0..32 {
        let (a, b) = (r.draw(Some(seed)), more.draw(Some(seed)));
        assert_eq!(a.size, b.size);
        assert_eq!(a.traits, b.traits);
        for (x, y) in a.numbers.iter().zip(&b.numbers) {
            if x.name != "neck_length" {
                assert_eq!(x, y);
            }
        }
    }
    // Thirty-two seeds, thirty-two recipes.
    let texts: std::collections::BTreeSet<String> = (0..32).map(|s| expand(&r, Some(s), &AsNamed, "t").unwrap().recipe).collect();
    assert_eq!(texts.len(), 32);
}

#[test]
fn a_role_the_style_cannot_resolve_refuses_the_specimen() {
    struct Only;
    impl Roles for Only {
        fn preset(&self, role: &str, hint: &str) -> Result<String, String> {
            if role == "coat" {
                Ok("fur".into())
            } else {
                Err(format!("no preset for {role} (hint {hint}); nearest: bone"))
            }
        }
    }
    let r = library::entry("box_creature").unwrap().resolved().unwrap();
    let e = expand(&r, Some(1), &Only, "t").unwrap_err();
    assert_eq!((e.code, e.path.as_str()), ("role_unresolved", "roles.claw"));
    assert!(e.message.contains("#2a2420"), "the kind's hint is handed to the style: {e}");
}

#[test]
fn nearest_names_of_the_library() {
    assert_eq!(library::nearest("box_creatur", 5), ["box_creature"]);
    assert_eq!(library::nearest("beast", 5), ["box_beast"]);
    assert!(library::nearest("qqqq", 5).is_empty());
    assert!(library::version() != 0);
}

/// A project's copy records its taking in the file itself (`taken`), and reads back as it was
/// written; a version that is not the library's form is refused with its path.
#[test]
fn a_copy_records_its_taking_and_reads_back() {
    let mut k = ENTRIES[0].kind().unwrap();
    k.from = Some(ENTRIES[0].from());
    k.taken = Some(crate::kind::Taken { library: format!("{:016x}", library::version()), base: Some("box_creature".into()), checked: "at the take: written at its mean".into(), own: 3, streams: Some("box_creature".into()) });
    let text = k.to_json().to_pretty();
    assert_eq!(Kind::parse(&text).unwrap(), k);
    let bad = text.replace(&format!("{:016x}", library::version()), "not-a-version");
    assert_eq!((parse_err(&bad).code, parse_err(&bad).path.as_str()), ("wrong_type", "taken.library"));
    let extra = text.replace("\"own\": 3", "\"own\": 3, \"signed\": \"me\"");
    assert_eq!(parse_err(&extra).path, "taken.signed");
}

// ---------------------------------------------------------------------------------------------
// Reach: a kind whose draws could leave what its plan can write is refused (review 1, finding 1)
// ---------------------------------------------------------------------------------------------

fn resolve_err(extra: &str) -> crate::Error {
    resolve(&Kind::parse(&small(extra)).unwrap(), None).expect_err(extra)
}

/// The review's impossible kinds, each refused with the kind's own field, the worst value a seed
/// would reach and the bound it leaves.
#[test]
fn a_kind_that_could_draw_an_impossible_body_is_refused_with_its_own_field() {
    // The live case: a size spread as large as its mean, with no range. The parser refuses a
    // spread whose 2.5 spreads pass zero; one inside that but past the plan's range is refused
    // when resolved.
    let e = parse_err(&small(r#", "size": {"shoulder_height": {"mean": 0.6, "sd": 0.6, "unit": "m", "own": "x"}}"#));
    assert_eq!((e.code, e.path.as_str()), ("out_of_range", "size.shoulder_height.sd"), "{e}");
    let e = resolve_err(r#", "size": {"shoulder_height": {"mean": 0.6, "sd": 0.2, "unit": "m", "own": "x"}}"#);
    assert_eq!((e.code, e.path.as_str()), ("outside_plan", "size.shoulder_height.sd"), "{e}");
    let r = e.reach.as_ref().unwrap();
    assert_eq!((r.number.as_str(), r.worst, r.bound), ("shoulder_height", 0.1, [0.3, 1.2]), "{e}");
    assert!(e.to_json().get("worst").is_some() && e.message.contains("0.3..1.2 m"), "{e}");
    // A size outside the plan, by its mean or its range.
    assert_eq!(resolve_err(r#", "size": {"shoulder_height": {"mean": 2.0, "unit": "m", "own": "x"}}"#).path, "size.shoulder_height.mean");
    assert_eq!(resolve_err(r#", "size": {"shoulder_height": {"mean": 0.6, "range": [0.1, 0.9], "unit": "m", "own": "x"}}"#).path, "size.shoulder_height.range");
    // A trait's spread that takes a girth below what the plan can write.
    let e = resolve_err(r#", "vary": {"build": {"spread": 0.36, "own": "x"}}"#);
    assert_eq!((e.code, e.path.as_str()), ("outside_plan", "vary.build.spread"), "{e}");
    assert!(e.reach.as_ref().unwrap().worst < e.reach.as_ref().unwrap().bound[0]);
    // Past the parser's own limit: a spread above 0.36 (a factor at 2.5 spreads below a tenth).
    assert_eq!(parse_err(&small(r#", "vary": {"build": {"spread": 0.5, "own": "x"}}"#)).path, "vary.build.spread");
    assert_eq!(parse_err(&small(r#", "numbers": {"head_length": {"share": 0.36, "spread": 0.5, "own": "x"}}"#)).path, "numbers.head_length.spread");
    // A negative mean is no length (the parser); a mean past the plan's range (resolve).
    assert_eq!(parse_err(&small(r#", "numbers": {"leg_length": {"mean": -1, "own": "x"}}"#)).path, "numbers.leg_length.mean");
    let e = resolve_err(r#", "numbers": {"leg_length": {"mean": 5, "own": "x"}}"#);
    assert_eq!((e.code, e.path.as_str()), ("outside_plan", "numbers.leg_length.mean"), "{e}");
    assert_eq!(resolve_err(r#", "numbers": {"leg_length": {"share": 4, "own": "x"}}"#).path, "numbers.leg_length.share");
    // An exponent that, over the size's spread, leaves the plan's range.
    let e = resolve_err(r#", "size": {"shoulder_height": {"mean": 0.6, "sd": 0.2, "range": [0.3, 1.2], "unit": "m", "own": "x"}}, "numbers": {"tail_length": {"share": 0.5, "exponent": -3, "own": "x"}}"#);
    assert_eq!((e.code, e.path.as_str()), ("outside_plan", "numbers.tail_length.exponent"), "{e}");
    // A range from zero (the review's infinities with an exponent of -1): refused by the parser.
    assert_eq!(parse_err(&small(r#", "size": {"shoulder_height": {"mean": 0.6, "range": [0, 0.9], "unit": "m", "own": "x"}}"#)).path, "size.shoulder_height.range[0]");
    // A number's own range outside the plan's, or not holding its mean.
    assert_eq!(resolve_err(r#", "numbers": {"leg_length": {"share": 0.62, "range": [0.01, 0.5], "own": "x"}}"#).path, "numbers.leg_length.range");
    assert_eq!(resolve_err(r#", "numbers": {"leg_length": {"share": 0.62, "range": [0.1, 0.2], "own": "x"}}"#).code, "out_of_range");
    // A field the base sets is named, and said to be the base's.
    let base = Kind::parse(&small(r#", "vary": {"build": {"spread": 0.36, "own": "x"}}"#)).unwrap();
    let child = Kind::parse(&small("").replace(r#""name": "probe""#, r#""name": "child", "base": "probe""#)).unwrap();
    let e = resolve(&child, Some(&base)).unwrap_err();
    assert!(e.path == "vary.build.spread" && e.message.contains("its base 'probe'"), "{e}");
}

/// A family of hostile kinds of the test plan: wide spreads, sizes at the edges, exponents at
/// their limits, a number's own spread at the parser's most. Some resolve and some do not.
pub(crate) fn hostile_kinds() -> Vec<String> {
    let mut out = Vec::new();
    for (sd, mean) in [("0.11", "0.6"), ("0.0", "0.3"), ("0.0", "1.2"), ("0.05", "0.45"), ("0.12", "0.9")] {
        for spread in ["0.0", "0.1", "0.2", "0.3", "0.36"] {
            for exponent in ["-3", "0.5", "1", "3"] {
                for head in ["0.36", "0.0"] {
                    out.push(small(&format!(
                        r#", "size": {{"shoulder_height": {{"mean": {mean}, "sd": {sd}, "unit": "m", "own": "x"}}}},
                        "vary": {{"build": {{"spread": {spread}, "own": "x"}}, "legs": {{"spread": {spread}, "own": "x"}}, "tail": {{"spread": {spread}, "own": "x"}}}},
                        "numbers": {{"leg_length": {{"share": 0.62, "exponent": {exponent}, "own": "x"}}, "head_length": {{"share": 0.36, "spread": {head}, "own": "x"}}}}"#
                    )));
                }
            }
        }
    }
    out
}

/// The backstop: over 100,000 seeds of the hostile kinds that resolve, no drawn number leaves
/// its plan's range or the kind's own reach, and every one is a finite positive number.
#[test]
fn no_seed_of_an_accepted_kind_leaves_its_reach() {
    let all = hostile_kinds();
    let accepted: Vec<_> = all.iter().filter_map(|t| resolve(&Kind::parse(t).unwrap(), None).ok()).collect();
    assert!(accepted.len() >= 20 && accepted.len() < all.len(), "{} of {} hostile kinds accepted: the family holds both", accepted.len(), all.len());
    let reaches: Vec<_> = accepted.iter().map(|r| r.reach().unwrap()).collect();
    let plan = accepted[0].plan;
    for seed in 0..100_000u64 {
        let i = (seed % accepted.len() as u64) as usize;
        let s = accepted[i].draw(Some(seed));
        for (v, b) in std::iter::once(&s.size).chain(&s.numbers).zip(&reaches[i]) {
            assert_eq!(v.name, b.name);
            assert!(v.value.is_finite() && v.value > 0.0 && v.value >= b.lo && v.value <= b.hi, "seed {seed}, kind {i}: {} {} outside {}..{}", v.name, v.value, b.lo, b.hi);
            let range = plan.numbers().iter().find(|n| n.name == v.name).map_or(plan.size().range, |n| n.range);
            assert!(v.value >= range[0] && v.value <= range[1], "seed {seed}: {} {} outside the plan's {range:?}", v.name, v.value);
        }
    }
}

/// A draw holds a value inside the plan's range even for a kind no resolve would accept (made by
/// hand around the checks): the backstop.
#[test]
fn a_kind_made_around_the_checks_still_draws_inside_the_plan() {
    let mut r = library::entry("box_creature").unwrap().resolved().unwrap();
    r.kind.vary.get_mut("build").unwrap().spread = 3.0;
    r.kind.numbers.get_mut("head_length").unwrap().spread = Some(5.0);
    assert!(r.reach().is_err());
    for seed in 0..2_000 {
        let s = r.draw(Some(seed));
        for (v, n) in s.numbers.iter().zip(r.plan.numbers()) {
            assert!(v.value >= n.range[0] && v.value <= n.range[1], "seed {seed}: {} {}", v.name, v.value);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Streams (review 1, finding 2): each number's own, unrelated to the others' and to the next seed's
// ---------------------------------------------------------------------------------------------

/// Pearson's correlation of two equally long series.
fn corr(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        sab += (x - ma) * (y - mb);
        saa += (x - ma) * (x - ma);
        sbb += (y - mb) * (y - mb);
    }
    sab / (saa * sbb).sqrt()
}

/// The largest correlation, in absolute value, among every pair of a plan's streams at one seed
/// and at consecutive seeds (seed n against seed n + 1, every stream with every stream), over
/// `seeds`; and the pair it is.
fn worst_correlation(names: &[String], seeds: &[u64], normal: &dyn Fn(u64, &str) -> f64) -> (f64, String) {
    let z: Vec<Vec<f64>> = names.iter().map(|n| seeds.iter().map(|s| normal(*s, n)).collect()).collect();
    let mut worst = (0.0f64, String::new());
    let mut see = |r: f64, what: String| {
        if r.abs() > worst.0.abs() {
            worst = (r, what);
        }
    };
    let len = seeds.len() - 1;
    for i in 0..names.len() {
        for j in 0..names.len() {
            if i < j {
                see(corr(&z[i], &z[j]), format!("{} x {}", names[i], names[j]));
            }
            see(corr(&z[i][..len], &z[j][1..]), format!("{} at n x {} at n+1", names[i], names[j]));
        }
    }
    worst
}

/// Every stream of the library's kinds (the size's, each trait's and each number's, keyed as
/// the draw keys them), over 40,000 consecutive seeds from 0 and 40,000 up to the largest seed,
/// correlates with no other at the same seed, or at the next seed, past 0.025.
///
/// **The bound**: for independent normals the sampling error of a correlation over n pairs is
/// 1/sqrt(n): 0.005 at n = 40,000, so 0.025 is five of them (a chance of about 6 in 10 million a
/// pair, against 247 pairs a kind and block), and a correlation of 0.03 lies six errors above
/// zero: it is seen. The seeds are fixed, so the test is the same every run. The scheme this
/// replaced (`Rng::named`: the seed as PCG32's state and the name as its increment) is shown to
/// fail it over the same seeds, so the test is known to see what it was written for.
#[test]
fn the_streams_of_a_kind_are_unrelated_at_one_seed_and_at_the_next() {
    const N: u64 = 40_000;
    const BOUND: f64 = 0.025;
    for e in ENTRIES {
        let r = e.resolved().unwrap();
        let key = r.stream_key().to_string();
        let names: Vec<String> = std::iter::once(r.plan.size().name)
            .chain(r.plan.traits().iter().map(|t| t.name))
            .chain(r.plan.numbers().iter().map(|n| n.name))
            .map(|n| format!("{key}/{n}"))
            .collect();
        for seeds in [(0..N).collect::<Vec<u64>>(), (crate::draw::MAX_SEED - N + 1..=crate::draw::MAX_SEED).collect()] {
            let (r, pair) = worst_correlation(&names, &seeds, &crate::draw::normal);
            assert!(r.abs() < BOUND, "{}: {pair} correlate at {r:.4} from seed {}", e.name(), seeds[0]);
        }
        // The old scheme, over the same seeds: past the bound.
        let old = |s: u64, n: &str| {
            let mut g = specula_core::rng::Rng::named(s, n);
            let unit = |x: u64| (x >> 11) as f64 * (1.0 / (1u64 << 53) as f64);
            let u1 = unit(g.next_u64()) + 1.0 / (1u64 << 53) as f64;
            let u2 = unit(g.next_u64());
            (-2.0 * specula_core::dmath::ln_f64(u1)).sqrt() * specula_core::dmath::cos_f64(2.0 * std::f64::consts::PI * u2)
        };
        let (r, pair) = worst_correlation(&names, &(0..N).collect::<Vec<u64>>(), &old);
        assert!(r.abs() > BOUND, "the old scheme's worst pair {pair} is {r:.4}: the test would not have seen it");
    }
    // Seed 0 and the largest seed are seeds like any other: their specimens differ from their
    // neighbours' and from the mean, and their residuals are not all one value.
    let r = library::entry("box_creature").unwrap().resolved().unwrap();
    let mean = r.draw(None);
    for s in [0, 1, crate::draw::MAX_SEED - 1, crate::draw::MAX_SEED] {
        let x = r.draw(Some(s));
        assert_ne!(x.numbers, mean.numbers, "seed {s}");
        assert!(x.traits.windows(2).any(|w| w[0].1 != w[1].1), "seed {s}: {:?}", x.traits);
    }
    assert_ne!(r.draw(Some(0)).numbers, r.draw(Some(1)).numbers);
    assert_ne!(r.draw(Some(crate::draw::MAX_SEED)).numbers, r.draw(Some(crate::draw::MAX_SEED - 1)).numbers);
}

/// A trait and a number of one name would draw one normal for both: every name of a plan's
/// size, traits and numbers is unique across the three tables (review 1, finding 7). And every
/// plan's own defaults resolve (a kind that sets nothing is a sound kind), its ranges are
/// positive and lie on their unit's step.
#[test]
fn a_plans_names_are_unique_across_its_tables_and_its_defaults_resolve() {
    for p in plans::PLANS {
        let mut seen = std::collections::BTreeSet::new();
        let names = std::iter::once(p.size().name).chain(p.traits().iter().map(|t| t.name)).chain(p.numbers().iter().map(|n| n.name));
        for n in names {
            assert!(seen.insert(n), "{}: '{n}' names two of its size, traits and numbers: they would share a stream", p.name());
        }
        for n in p.numbers() {
            assert!(n.range[0] > 0.0 && n.range[0] < n.range[1], "{}: {}", p.name(), n.name);
            for x in n.range {
                assert_eq!(crate::draw::round(x, n.unit), x, "{}: {}'s range on its step", p.name(), n.name);
            }
        }
        let bare = Kind::parse(&format!(r#"{{"kind": 1, "name": "bare", "plan": "{}", "about": "the plan's defaults"}}"#, p.name())).unwrap();
        resolve(&bare, None).unwrap_or_else(|e| panic!("{}: its own defaults: {e}", p.name()));
    }
}

#[test]
fn a_seed_a_recipe_cannot_hold_exactly_is_refused_and_unbuilt_levels_are_said() {
    let e = library::entry("box_creature").unwrap();
    let r = e.resolved().unwrap();
    assert!(expand(&r, Some(crate::draw::MAX_SEED), &AsNamed, "t").unwrap().recipe.contains("\"seed\": 9007199254740991"));
    let x = expand(&r, Some(crate::draw::MAX_SEED + 1), &AsNamed, "t").unwrap_err();
    assert_eq!((x.code, x.path.as_str()), ("out_of_range", "seed"));
    assert_eq!(expand(&r, Some(u64::MAX), &AsNamed, "t").unwrap_err().path, "seed");
    for level in ["mid", "crowd", "far"] {
        let e = parse_err(&small(&format!(r#", "levels": {{"near": 600, "{level}": 100}}"#)));
        assert_eq!((e.code, e.path), ("not_built", format!("levels.{level}")));
    }
}

/// A copy keyed on the library's kind (`taken.streams`) draws the library's specimens whatever
/// it is called; a change to one of its numbers moves only that number; a copy given its own key
/// draws anew (review 1, finding 3).
#[test]
fn a_renamed_copy_draws_the_library_entrys_specimens() {
    let lib = library::entry("box_creature").unwrap().resolved().unwrap();
    let mut copy = lib.kind.clone();
    copy.name = "my_beast".into();
    copy.taken = Some(crate::kind::Taken { library: "0".repeat(16), base: None, checked: String::new(), own: 0, streams: Some("box_creature".into()) });
    let text = copy.to_json().to_pretty();
    let c = resolve(&Kind::parse(&text).unwrap(), None).unwrap();
    for seed in [0, 7, 42, 123_456] {
        let (a, b) = (lib.draw(Some(seed)), c.draw(Some(seed)));
        assert_eq!((a.size, a.numbers, a.traits), (b.size, b.numbers, b.traits), "seed {seed}");
    }
    let edited = text.replace(r#""share": 1.2"#, r#""share": 1.3"#);
    assert_ne!(edited, text);
    let d = resolve(&Kind::parse(&edited).unwrap(), None).unwrap();
    for seed in [7, 42] {
        let (a, b) = (c.draw(Some(seed)), d.draw(Some(seed)));
        for (x, y) in a.numbers.iter().zip(&b.numbers) {
            assert_eq!(x == y, x.name != "body_length", "seed {seed}: {}", x.name);
        }
    }
    let own = text.replace(r#""streams": "box_creature""#, r#""streams": "my_beast""#);
    let o = resolve(&Kind::parse(&own).unwrap(), None).unwrap();
    assert_ne!(o.draw(Some(7)).numbers, lib.draw(Some(7)).numbers, "its own key: its own specimens");
}
