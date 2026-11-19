//! **Advice is checked before it is given** (docs/design/FORM.md, "Faceting, from what the
//! recipe names round"): the check `faceting` names a count only once it has measured that the
//! shape, rebuilt with it and moved as it was, holds the limit. Here every kind's advice is
//! followed, at limits just above and just below the turn each count gives, and the asset built
//! with the count named must pass; where the advice says no count holds, the most it allows
//! must fail.

use specula_model::{build, BuildParams, Style};
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

/// Where no count holds, the advice says so and names none: a capsule's ends need more than 32
/// rings for a limit of 2°.
#[test]
fn advice_says_when_no_count_holds() {
    let f = facts(&one(r#"{"op":"capsule","radius":0.3,"height":1.5,"segments":256,"rings":2}"#, ""));
    let (failed, fix) = verdict(&f, 2.0);
    assert!(failed && named(&fix).is_none() && fix.contains("cannot hold the limit: at its most (32)"), "{fix}");
}
