//! **Advice is checked before it is given** (docs/design/FORM.md, "Faceting, from what the
//! recipe names round"): the check `faceting` names a count only once it has measured that the
//! shape, rebuilt with it and moved as it was, holds the limit. Here every kind's advice is
//! followed, at limits just above and just below the turn each count gives, and the asset built
//! with the count named must pass; where the advice says no count holds, the most it allows
//! must fail.

use specula_model::{build, BuildParams, Style};
use specula_studio::check::{faceting_advice_work, MOST_ADVICE_WORK};
use specula_studio::facts::{AssetFacts, RecipeInfo};
use specula_studio::Bible;

const STYLE: &str = r##"{ "style": "test", "palette": { "pale": "#8c96a3" },
  "materials": { "stone": { "color": "#8c96a3", "roughness": 0.75 } } }"##;

fn one(shape: &str, ops: &str) -> String {
    format!(r#"{{ "name": "t", "parts": [ {{ "name": "p", "material": "stone", "shape": {shape}, "ops": [{ops}] }} ] }}"#)
}

fn facts(text: &str) -> AssetFacts {
    let s = Style::parse(STYLE).unwrap();
    let m = build(text, &BuildParams::new(&s)).unwrap_or_else(|e| panic!("{text}: {e}"));
    AssetFacts::from_model(&m, RecipeInfo::parse(text).as_ref(), Some("t"), &[], 0)
}

/// The check `faceting` at `limit` on what was measured (its first finding: a shape not
/// measured is said in a finding of its own, which no count can mend): whether it failed, and
/// its fix.
fn verdict(f: &AssetFacts, limit: f32) -> (bool, String) {
    let s = Style::parse(STYLE).unwrap();
    let bible = Bible::load(&s, Some(&format!("[form]\nmax_facet_deg = {limit}\n")), "t.toml").unwrap();
    let r = bible.check(f);
    let main = r.findings.iter().find(|x| x.check == "faceting").expect("checked");
    (!main.passed, main.fix.clone())
}

/// The count a fix names: after "give the X " or "\"resolution\" to ".
fn named(fix: &str) -> Option<u32> {
    let rest = if let Some(i) = fix.find("\"resolution\" to ") {
        &fix[i + 16..]
    } else {
        let i = fix.find("give the ")?;
        let after = &fix[i + 9..];
        &after[after.find(' ')? + 1..]
    };
    rest.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()
}

/// Follows the advice for `shape` (with `{N}` for its count, built with `first`) moved by `ops`,
/// at limits just above and just below the turn of every count in `counts`: each failure's
/// advice, followed once, passes; a failure that names no count fails at `most` too.
fn follow(shape: &str, ops: &str, first: u32, counts: &[u32], most: u32) -> usize {
    let at = |n: u32| facts(&one(&shape.replace("{N}", &n.to_string()), ops));
    let start = at(first);
    let mut followed = 0;
    for &k in counts {
        let f = at(k);
        let w = f.round.iter().filter(|r| r.measured()).map(|r| r.turn_deg).fold(0.0f32, f32::max);
        for limit in [w * 1.0005 + 0.002, w * 0.9995 - 0.002] {
            let (failed, fix) = verdict(&start, limit);
            if !failed {
                continue;
            }
            match named(&fix) {
                Some(n) => {
                    let (still, again) = verdict(&at(n), limit);
                    assert!(!still, "{shape} [{ops}] at {limit}°: told {n} ({fix}), which fails: {again}");
                    followed += 1;
                }
                None => {
                    assert!(fix.contains("cannot hold") || fix.contains("at its most"), "{shape} [{ops}] at {limit}°: no count named: {fix}");
                    // (At the most its facets may be too small to show, and pass: what the advice
                    // says is that they still turn more than the limit there.)
                    let t = at(most).round.iter().filter(|r| r.measured()).map(|r| r.turn_deg).fold(0.0f32, f32::max);
                    assert!(t > limit, "{shape} [{ops}] at {limit}°: told no count holds, but {most} turns {t}°");
                }
            }
        }
    }
    followed
}

/// A sphere, plain and stretched every way, at every count: the turn does not halve with each
/// subdivision, and a stretched sphere's falls more slowly still.
#[test]
fn a_sphere_is_told_the_subdivisions_that_hold() {
    let sphere = r#"{"op":"sphere","radius":0.5,"subdivisions":{N}}"#;
    let mut n = 0;
    for ops in ["", r#"{"op":"scale","by":[1,3,1]}"#, r#"{"op":"scale","by":[2,1,0.5]}"#, r#"{"op":"rotate","deg":[20,0,35]},{"op":"scale","by":[1,2.5,1]}"#] {
        n += follow(sphere, ops, 0, &[0, 1, 2, 3, 4, 5], 5);
    }
    assert!(n >= 30, "{n} followed");
}

/// Every other kind, moved so that it is measured face by face and moved not at all, at limits
/// round the turns of a few counts each.
#[test]
fn every_kind_is_told_a_count_that_holds() {
    let mut n = 0;
    for (shape, ops, first, counts, most) in [
        (r#"{"op":"cylinder","radius":0.5,"height":1,"sides":{N}}"#, r#"{"op":"scale","by":[3,1,1]}"#, 12, &[12u32, 17, 36, 37][..], 256),
        (r#"{"op":"cylinder","radius":0.5,"height":1,"sides":{N}}"#, r#"{"op":"lean","x":1.2}"#, 8, &[8, 23, 24][..], 256),
        (r#"{"op":"cylinder","radius":0.5,"height":1,"sides":{N}}"#, r#"{"op":"rotate","deg":[0,0,45]},{"op":"scale","by":[1,2,1]}"#, 7, &[7, 19, 20][..], 256),
        (r#"{"op":"cone","radius":0.5,"height":0.4,"sides":{N}}"#, r#"{"op":"scale","by":[1,1,2.5]}"#, 5, &[5, 13, 30][..], 256),
        (r#"{"op":"torus","radius":0.6,"tube":0.2,"segments":{N},"sides":24}"#, r#"{"op":"scale","by":[3,1,1]}"#, 9, &[9, 40, 41][..], 512),
        (r#"{"op":"torus","radius":0.6,"tube":0.2,"segments":48,"sides":{N}}"#, r#"{"op":"scale","by":2}"#, 3, &[3, 7, 17][..], 64),
        (r#"{"op":"capsule","radius":0.3,"height":1.5,"segments":{N},"rings":12}"#, r#"{"op":"rotate","deg":[0,0,60]},{"op":"scale","by":[1,0.5,1]}"#, 6, &[6, 15, 33][..], 256),
        (r#"{"op":"capsule","radius":0.3,"height":1.5,"segments":64,"rings":{N}}"#, "", 1, &[1, 3, 4, 9][..], 32),
        (r#"{"op":"lathe","segments":{N},"profile":[[0,0],[0.5,0],[0.42,0.15],[0.2,0.5],[0.06,1],[0,1.05]]}"#, r#"{"op":"scale","by":[1,1,4]},{"op":"lean","z":0.5}"#, 10, &[10, 21, 50][..], 256),
        (r#"{"op":"sdf","resolution":{N},"tree":{"op":"sphere","radius":0.5}}"#, "", 8, &[8, 13, 20][..], 160),
    ] {
        n += follow(shape, ops, first, counts, most);
    }
    assert!(n >= 30, "{n} followed");
}

/// Whether `text` builds.
fn builds(text: &str) -> bool {
    let s = Style::parse(STYLE).unwrap();
    build(text, &BuildParams::new(&s)).is_ok()
}

/// **Advice is only ever a recipe that builds** (the third review's first finding). The
/// review's lathe of 301 rows: past 108 segments the build refuses it (a part of more than
/// 65,536 triangles). At limits of 1.2° and 2° no count it can be built with holds, and the
/// advice says so: at 108 its faces turn 3.4° and its rim, seen down its axis (an ellipse of
/// 1 by 0.6), 5.6°. (By its faces alone, which the review measured, 2° held at 98.) At 6° it
/// names a count, which builds and holds.
#[test]
fn the_reviews_lathe_is_told_only_what_builds() {
    let mut prof = String::from("[[0,0]");
    for k in 1..300 {
        let y = f64::from(k) * 0.01;
        let r = if k % 2 == 0 { 0.5 } else { 0.45 } + 0.0003 * f64::from(k);
        prof.push_str(&format!(",[{r:.4},{y:.4}]"));
    }
    prof.push_str(",[0,3.0]]");
    let lathe = |n: u32| one(&format!(r#"{{"op":"lathe","segments":{n},"profile":{prof}}}"#), r#"{"op":"scale","by":[1,1.7,0.6]}"#);
    let f = facts(&lathe(8));
    let r = f.round.iter().find(|r| r.what == "lathe").unwrap();
    assert!(r.most == 108 && r.most_by.contains("one part") && r.most_estimated.is_none(), "{r:?}");
    assert!(builds(&lathe(108)) && !builds(&lathe(109)));
    for limit in [1.2, 2.0] {
        let (failed, fix) = verdict(&f, limit);
        assert!(failed && named(&fix).is_none() && fix.contains("at the most it can be built with (108; past it the triangles one part may have would be exceeded) it still turns 5.6°"), "{limit}: {fix}");
    }
    let (failed, fix) = verdict(&f, 6.0);
    let k = named(&fix).unwrap_or_else(|| panic!("{fix}"));
    assert!(failed && k <= 108 && builds(&lathe(k)) && !verdict(&facts(&lathe(k)), 6.0).0, "told {k}: {fix}");
}

thread_local! {
    /// Triangles built by this test thread's builds (what a test of builds costs, counted).
    static BUILT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// `text` built, or the reason it is refused; its triangles counted in [`BUILT`].
fn try_build(text: &str) -> Result<specula_model::Model, String> {
    let s = Style::parse(STYLE).unwrap();
    let m = build(text, &BuildParams::new(&s)).map_err(|e| e.to_string())?;
    BUILT.with(|b| b.set(b.get() + m.lods.iter().map(|l| l.triangle_count() as u64).sum::<u64>()));
    Ok(m)
}

/// One row of [`counts_that_build`]: what it is, the recipe at a count, the count's name, the
/// count it is built with first, and the limit that must stop its most.
type Row = (&'static str, Box<dyn Fn(u32) -> String>, &'static str, u32, &'static str);

/// Each row: the most it can be built with is stopped by the limit the row names, builds, and
/// one more is refused (but for a sculpt, whose most is estimated); and at a limit just above
/// and just below what that most turns, the advice names a count that builds and holds, or
/// says that the most does not hold. Returns how many advices were followed.
fn counts_that_build(rows: Vec<Row>) -> usize {
    let mut followed = 0;
    for (label, at, count, first, by) in rows {
        println!("{label}: from {} triangles built", BUILT.with(std::cell::Cell::get));
        let m = try_build(&at(first)).unwrap_or_else(|e| panic!("{label}: {e}"));
        let start = AssetFacts::from_model(&m, RecipeInfo::parse(&at(first)).as_ref(), Some("t"), &[], 0);
        let r = start.round.iter().find(|r| r.count == count && r.count_value == first).unwrap_or_else(|| panic!("{label}: {:?}", start.round));
        let most = r.most;
        assert!(r.most_by == by, "{label}: the most is stopped by {:?}, not {by:?}: {r:?}", r.most_by);
        assert!(try_build(&at(most)).is_ok(), "{label}: refused at its most {most}");
        if r.most_estimated.is_none() {
            let past = try_build(&at(most + 1));
            assert!(past.is_err(), "{label}: {} builds, past the most it is said to build with", most + 1);
            let why = past.err().unwrap_or_default();
            let names = match by {
                "the recipe's budget.max_triangles" => why.contains("budget"),
                "the triangles the model may have" => why.contains("the model"),
                "the triangles one part may have" => why.contains("a part of"),
                _ => true,
            };
            assert!(names, "{label}: one past the most is refused for another reason than {by}: {why}");
        }
        // (About what it turns at its most: the limits are put either side of it, and what each
        // advice says is then built and checked.)
        let t = r.turn_at(most).unwrap();
        for limit in [t * 1.0005 + 0.002, t * 0.9995 - 0.002] {
            let (failed, fix) = verdict(&start, limit);
            assert!(failed, "{label} at {limit}°: it passes as it is");
            match named(&fix) {
                Some(k) => {
                    let mk = try_build(&at(k)).unwrap_or_else(|e| panic!("{label} at {limit}°: told {k} ({fix}), refused: {e}"));
                    assert!(k <= most, "{label} at {limit}°: told {k}, past {most}: {fix}");
                    let fk = AssetFacts::from_model(&mk, RecipeInfo::parse(&at(k)).as_ref(), Some("t"), &[], 0);
                    let rk = fk.round.iter().find(|x| x.count == count && x.count_value == k).unwrap();
                    assert!(rk.turn_at(k).unwrap() <= limit, "{label} at {limit}°: told {k} ({fix}), which turns {:?}", rk.turn_at(k));
                    followed += 1;
                }
                None => assert!(limit < t && fix.contains(&format!("at the most{} it can be built with ({most};", if count == "subdivisions" { " \"subdivisions\"" } else { "" })), "{label} at {limit}°: {fix}"),
            }
        }
    }
    followed
}

/// A recipe of one part, `shape` with `{N}` for its count, moved by `ops`, under a budget of
/// `budget` triangles.
fn budgeted(shape: &'static str, ops: &'static str, budget: u32) -> Box<dyn Fn(u32) -> String> {
    Box::new(move |n: u32| format!(r#"{{ "name": "t", "budget": {{"max_triangles": {budget}}}, "parts": [ {{ "name": "p", "material": "stone", "shape": {}, "ops": [{ops}] }} ] }}"#, shape.replace("{N}", &n.to_string())))
}

fn plain(shape: &'static str, ops: &'static str) -> Box<dyn Fn(u32) -> String> {
    Box::new(move |n: u32| one(&shape.replace("{N}", &n.to_string()), ops))
}

const BUDGET: &str = "the recipe's budget.max_triangles";
const PART: &str = "the triangles one part may have";
const MODEL: &str = "the triangles the model may have";

/// **Every kind, and every limit the most holds a count to, is told only counts that build.**
/// Each kind with copies stacked in one place (so that it stays small and its facets show)
/// under a budget that bites at a small size (each kind's own growth, through the budget's
/// branch); a cylinder's copies at a part's limit; the fourth review's three: a recipe's
/// budget beside a second part, the model's room after two more parts, and a scatter's copies
/// (the second finding: no default test held these). The work limit is held in
/// `facet_work.rs`.
#[test]
fn every_kind_is_told_only_counts_that_build() {
    BUILT.with(|b| b.set(0));
    let rows: Vec<Row> = vec![
        ("cylinder", budgeted(r#"{"op":"cylinder","radius":0.5,"height":1,"sides":{N}}"#, r#"{"op":"array","count":4,"offset":[0,0,0]}"#, 3000), "sides", 6, BUDGET),
        ("cone", budgeted(r#"{"op":"cone","radius":0.5,"height":0.4,"sides":{N}}"#, r#"{"op":"array","count":4,"offset":[0,0,0]},{"op":"scale","by":[1,1,2]}"#, 1200), "sides", 5, BUDGET),
        ("torus segments", budgeted(r#"{"op":"torus","radius":0.6,"tube":0.2,"segments":{N},"sides":64}"#, r#"{"op":"array","count":2,"offset":[0,0,0]}"#, 6000), "segments", 7, BUDGET),
        ("torus sides", budgeted(r#"{"op":"torus","radius":0.6,"tube":0.2,"segments":64,"sides":{N}}"#, r#"{"op":"array","count":2,"offset":[0,0,0]}"#, 3000), "sides", 3, BUDGET),
        ("capsule segments", budgeted(r#"{"op":"capsule","radius":0.3,"height":1.5,"segments":{N},"rings":32}"#, r#"{"op":"array","count":2,"offset":[0,0,0]}"#, 4000), "segments", 5, BUDGET),
        ("capsule rings", budgeted(r#"{"op":"capsule","radius":0.3,"height":1.5,"segments":256,"rings":{N}}"#, "", 10000), "rings", 1, BUDGET),
        ("lathe", budgeted(r#"{"op":"lathe","segments":{N},"profile":[[0,0],[0.5,0],[0.42,0.15],[0.2,0.5],[0.06,1],[0,1.05]]}"#, r#"{"op":"array","count":4,"offset":[0,0,0]},{"op":"lean","z":0.5}"#, 3000), "segments", 6, BUDGET),
        ("sphere", budgeted(r#"{"op":"sphere","radius":0.5,"subdivisions":{N}}"#, r#"{"op":"array","count":4,"offset":[0,0,0]}"#, 6000), "subdivisions", 0, BUDGET),
        ("sculpt", budgeted(r#"{"op":"sdf","resolution":{N},"tree":{"op":"sphere","radius":0.5}}"#, r#"{"op":"array","count":2,"offset":[0,0,0]}"#, 6000), "resolution", 8, BUDGET),
        ("cylinder at a part's limit", plain(r#"{"op":"cylinder","radius":0.5,"height":1,"sides":{N}}"#, r#"{"op":"array","count":128,"offset":[0,0,0]}"#), "sides", 6, PART),
        (
            "a budget beside another part (review 4)",
            Box::new(|n: u32| format!(r#"{{ "name": "t", "budget": {{"max_triangles": 600}}, "parts": [ {{ "name": "p", "material": "stone", "shape": {{"op":"cylinder","radius":0.5,"height":1,"sides":{n}}} }}, {{ "name": "q", "material": "stone", "shape": {{"op":"cylinder","radius":0.05,"height":1,"sides":64}}, "ops": [{{"op":"move","by":[2,0,0]}}] }} ] }}"#)),
            "sides",
            6,
            BUDGET,
        ),
        (
            "the model's room after two more parts (review 4)",
            Box::new(|n: u32| format!(r#"{{ "name": "t", "parts": [ {{ "name": "p", "material": "stone", "shape": {{"op":"cylinder","radius":0.5,"height":1,"sides":{n}}}, "ops": [{{"op":"array","count":128,"offset":[0,0,0]}}] }}, {{ "name": "q", "material": "stone", "shape": {{"op":"sphere","radius":0.3,"subdivisions":5}}, "ops": [{{"op":"array","count":3,"offset":[3,0,0]}}] }}, {{ "name": "r", "material": "stone", "shape": {{"op":"sphere","radius":0.3,"subdivisions":5}}, "ops": [{{"op":"array","count":3,"offset":[0,3,0]}}] }} ] }}"#)),
            "sides",
            6,
            MODEL,
        ),
        (
            "a scatter's copies (review 4)",
            Box::new(|n: u32| format!(r#"{{ "name": "sc", "seed": 3, "parts": [ {{ "name": "things", "material": "stone", "generate": {{ "op": "scatter", "area": {{ "rect": [40, 40] }}, "min_distance": 0.8, "count": 400, "items": [ {{ "shape": {{ "op": "cylinder", "radius": 1.5, "height": 0.4, "sides": {n} }} }} ] }} }} ] }}"#)),
            "sides",
            6,
            PART,
        ),
    ];
    let followed = counts_that_build(rows);
    assert!(followed >= 9, "{followed} followed");
    println!("every_kind_is_told_only_counts_that_build: {} triangles built", BUILT.with(std::cell::Cell::get));
}

/// The same with each kind at a part's limit (copies enough to fill a part: about 65,000
/// triangles a build): what this test was before the fourth review, kept for its figures.
#[test]
#[ignore = "slow: parts of about 65,000 triangles, some 60 builds (about 90 s in a debug build); every rule it holds is held by every_kind_is_told_only_counts_that_build"]
fn every_kind_is_told_only_counts_that_build_at_a_parts_limit() {
    BUILT.with(|b| b.set(0));
    let rows: Vec<Row> = vec![
        ("cylinder", plain(r#"{"op":"cylinder","radius":0.5,"height":1,"sides":{N}}"#, r#"{"op":"array","count":128,"offset":[0,0,0]}"#), "sides", 6, PART),
        ("cone", plain(r#"{"op":"cone","radius":0.5,"height":0.4,"sides":{N}}"#, r#"{"op":"array","count":200,"offset":[0,0,0]},{"op":"scale","by":[1,1,2]}"#), "sides", 5, PART),
        ("torus segments", plain(r#"{"op":"torus","radius":0.6,"tube":0.2,"segments":{N},"sides":64}"#, r#"{"op":"array","count":2,"offset":[0,0,0]}"#), "segments", 7, PART),
        ("torus sides", plain(r#"{"op":"torus","radius":0.6,"tube":0.2,"segments":512,"sides":{N}}"#, r#"{"op":"array","count":4,"offset":[0,0,0]}"#), "sides", 3, PART),
        ("capsule segments", plain(r#"{"op":"capsule","radius":0.3,"height":1.5,"segments":{N},"rings":32}"#, r#"{"op":"array","count":4,"offset":[0,0,0]}"#), "segments", 5, PART),
        ("capsule rings", plain(r#"{"op":"capsule","radius":0.3,"height":1.5,"segments":256,"rings":{N}}"#, r#"{"op":"array","count":4,"offset":[0,0,0]}"#), "rings", 1, PART),
        ("lathe", plain(r#"{"op":"lathe","segments":{N},"profile":[[0,0],[0.5,0],[0.42,0.15],[0.2,0.5],[0.06,1],[0,1.05]]}"#, r#"{"op":"array","count":200,"offset":[0,0,0]},{"op":"lean","z":0.5}"#), "segments", 6, PART),
        ("sphere", plain(r#"{"op":"sphere","radius":0.5,"subdivisions":{N}}"#, r#"{"op":"array","count":4,"offset":[0,0,0]}"#), "subdivisions", 0, PART),
        ("sculpt", plain(r#"{"op":"sdf","resolution":{N},"tree":{"op":"sphere","radius":0.5}}"#, r#"{"op":"array","count":24,"offset":[0,0,0]}"#), "resolution", 8, PART),
    ];
    let followed = counts_that_build(rows);
    assert!(followed >= 9, "{followed} followed");
    println!("every_kind_is_told_only_counts_that_build_at_a_parts_limit: {} triangles built", BUILT.with(std::cell::Cell::get));
}

/// **The whole check's advice work is bounded** ([`MOST_ADVICE_WORK`] faces and rim corners
/// measured, all of one check's advice together; past it no count is named). Sepulcra's 56 at
/// limits of 20°, 5° and 1° spend a small share of it (their figures are in FORM.md); a recipe
/// made to spend as much as the limits on a part allow (a torus of 21 copies each stretched its
/// own way, so 21 maps, and a profile of three edges, whose segments can be built up to 512)
/// stays within it, at a time held as a budget.
#[test]
fn the_advice_work_is_bounded_for_the_whole_check() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
    let style = Style::parse(&std::fs::read_to_string(root.join("style.json")).unwrap()).unwrap();
    let mut lib = std::collections::BTreeMap::new();
    for e in std::fs::read_dir(root.join("recipes")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "json") {
            lib.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
        }
    }
    let (mut assets, mut most) = (0, 0u64);
    for (stem, text) in &lib {
        if specula_core::Json::parse(text).ok().is_some_and(|j| j.get("texgraph").is_some()) {
            continue;
        }
        assets += 1;
        let m = build(text, &BuildParams { library: Some(&lib), ..BuildParams::new(&style) }).unwrap_or_else(|e| panic!("{stem}: {e}"));
        let f = AssetFacts::from_model(&m, RecipeInfo::parse(text).as_ref(), Some(stem), &[], 0);
        for limit in [20.0, 5.0, 1.0] {
            most = most.max(faceting_advice_work(&f, limit));
        }
    }
    assert!(assets == 56 && most > 0 && most < MOST_ADVICE_WORK / 10, "{assets} assets; the most one check's advice measured: {most}");
    let heavy = one(r#"{"op":"torus","radius":0.6,"tube":0.2,"segments":3,"sides":3}"#, r#"{"op":"array","count":21,"offset":[0,0,0],"jitter":{"rotate":[40,30,50],"scale":0.4}},{"op":"scale","by":[1,2.5,0.4]}"#);
    let f = facts(&heavy);
    let r = f.round.iter().find(|r| r.count == "segments").unwrap();
    assert!(r.most == 512 && r.recount_maps() == 21, "{r:?}");
    for limit in [0.9, 2.0, 7.0] {
        let t = std::time::Instant::now();
        let w = faceting_advice_work(&f, limit);
        let took = t.elapsed();
        assert!(w > 0 && w <= MOST_ADVICE_WORK, "{limit}°: {w}");
        assert!(took < specula_core::speed_budget(std::time::Duration::from_millis(1500)), "{limit}°: {took:?} for {w}");
    }
}

/// **A ring is held to its rim seen down its axis** where that turns more than its faces (the
/// third review's second finding): a round tower of 32 sides under a shallow roof of 8 passed
/// a limit of 20° by the roof's faces (12.3°); from above the roof is an octagon, 45° a corner.
/// It fails now, saying which turn it is held to, and the advice followed builds and passes.
#[test]
fn a_shallow_roof_is_held_by_its_rim() {
    let roof = |n: u32| {
        format!(
            r#"{{ "name": "t", "parts": [
        {{ "name": "tower", "material": "stone", "shape": {{"op":"cylinder","radius":1.0,"height":3.0,"sides":32}} }},
        {{ "name": "roof", "material": "stone", "shape": {{"op":"cone","radius":1.3,"height":0.35,"sides":{n}}}, "ops": [{{"op":"move","by":[0,3,0]}}] }} ] }}"#
        )
    };
    let f = facts(&roof(8));
    let s = Style::parse(STYLE).unwrap();
    let bible = Bible::load(&s, Some("[form]\nmax_facet_deg = 20\n"), "t.toml").unwrap();
    let main = bible.check(&f).findings.into_iter().find(|x| x.check == "faceting").unwrap();
    assert!(!main.passed && (main.value - 45.0).abs() < 1e-3 && main.message.contains("cone at parts[1].shape: its rim, seen along its axis, turns 45.0° at a corner") && main.message.contains("its faces turn 12.3°"), "{main:?}");
    let k = named(&main.fix).unwrap_or_else(|| panic!("{}", main.fix));
    assert!(k == 18 && builds(&roof(k)) && !verdict(&facts(&roof(k)), 20.0).0, "{}", main.fix);
    let j = f.to_json(0);
    assert_eq!(j.get("faceting").and_then(|x| x.get("by")).and_then(specula_core::Json::as_str), Some("its rim, seen along its axis"));
}

/// Where no count holds, the advice says so and names none: a capsule's ends need more than 32
/// rings for a limit of 2°.
#[test]
fn advice_says_when_no_count_holds() {
    let f = facts(&one(r#"{"op":"capsule","radius":0.3,"height":1.5,"segments":256,"rings":2}"#, ""));
    let (failed, fix) = verdict(&f, 2.0);
    assert!(failed && named(&fix).is_none() && fix.contains("cannot hold the limit: at its most (32)"), "{fix}");
}
