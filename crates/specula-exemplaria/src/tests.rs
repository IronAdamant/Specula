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
            assert!(specula_anim_kinds().contains(&m.generator), "{}: {} is not a generator", p.name(), m.generator);
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

/// The generator kinds of `specula-anim` (`gen::KINDS`), as text: this crate does not depend on
/// the anim library; the box_creature test generates every clip through it.
fn specula_anim_kinds() -> [&'static str; 12] {
    ["gait", "idle", "hover", "swing", "leap", "collapse", "wobble", "keys", "pose", "recoil", "breathe", "chain"]
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
