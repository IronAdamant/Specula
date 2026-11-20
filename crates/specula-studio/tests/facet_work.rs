//! **A count the advice names builds, the build's work limit included** (docs/design/FORM.md,
//! "What was mended, fourth round"). The most a round shape's count can be is held to the
//! build's work limit (`specula_model::work::MAX_BUILD`) by an inequality on what the build
//! spent at the count it has: `W(k) ≤ W(n) + (ρ(k) − 1)·S + P(k)`. Here that bound is held
//! against the work real builds spend, on recipes drawn from a fixed sequence (every kind of
//! round shape, with operations, far levels, colliders, textures and paint mixed), and the
//! advice is followed on the fourth review's three assets, where the work limit is what binds,
//! and built.

use specula_model::{build, BuildParams, Model, Style};
use specula_studio::facts::{AssetFacts, RecipeInfo};
use specula_studio::Bible;

const STYLE: &str = r##"{ "style": "test", "palette": { "pale": "#8c96a3" },
  "materials": { "stone": { "color": "#8c96a3", "roughness": 0.75 } } }"##;

const WORK: &str = "the build's work limit (1,500 million units)";

fn style() -> Style {
    Style::parse(STYLE).unwrap()
}

fn try_build(text: &str) -> Result<Model, String> {
    build(text, &BuildParams::new(&style())).map_err(|e| e.to_string())
}

fn facts_of(m: &Model, text: &str) -> AssetFacts {
    AssetFacts::from_model(m, RecipeInfo::parse(text).as_ref(), Some("t"), &[], 0)
}

fn verdict(f: &AssetFacts, limit: f32) -> (bool, String) {
    let s = style();
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

/// The review's lathe profile: 301 rows.
fn profile() -> String {
    let mut prof = String::from("[[0,0]");
    for k in 1..300 {
        let y = f64::from(k) * 0.01;
        let r = if k % 2 == 0 { 0.5 } else { 0.45 } + 0.0003 * f64::from(k);
        prof.push_str(&format!(",[{r:.4},{y:.4}]"));
    }
    prof.push_str(",[0,3.0]]");
    prof
}

/// A texture graph inline: `nodes` fbm noises blended, at `size` pixels.
fn texture(size: u32, nodes: usize) -> String {
    let mut ns = Vec::new();
    for i in 0..nodes {
        ns.push(format!(r#""n{i}": {{ "op": "noise", "kind": "fbm", "scale": {}, "octaves": 6 }}"#, 3 + i));
    }
    let mut last = "n0".to_string();
    for i in 1..nodes {
        ns.push(format!(r#""b{i}": {{ "op": "blend", "a": "{last}", "b": "n{i}", "mode": "multiply", "amount": 0.3 }}"#));
        last = format!("b{i}");
    }
    ns.push(format!(r#""albedo": {{ "op": "gradient_map", "in": "{last}", "stops": [[0, "pale"], [1, "pale"]] }}"#));
    format!(r#"{{ "size": {size}, "seed": 1, "nodes": {{ {} }}, "outputs": {{ "base_color": "albedo", "height": "{last}" }} }}"#, ns.join(","))
}

/// The review's three assets: built with `first`, the most the work limit allows is said to
/// be by work and builds; and the advice, followed at a limit just above what that most turns,
/// names a count that builds. Returns (label, most, what bound it).
/// A recipe at a count.
type At = Box<dyn Fn(u32) -> String>;

fn reviews_cases() -> Vec<(&'static str, At)> {
    let prof = profile();
    let p2 = prof.clone();
    let p3 = prof.clone();
    let tex512 = texture(512, 2);
    let tex256 = texture(256, 10);
    vec![
        (
            "a textured lathe, 512 px, two noises, two far levels",
            Box::new(move |n: u32| format!(r#"{{ "name": "t", "lods": [0.5, 0.25], "textures": {{ "skin": {tex512} }}, "parts": [ {{ "name": "p", "material": "stone", "texture": "skin", "shape": {{"op":"lathe","segments":{n},"profile":{prof}}} }} ] }}"#)),
        ),
        (
            "a textured lathe, 256 px, ten noises, two far levels",
            Box::new(move |n: u32| format!(r#"{{ "name": "t", "lods": [0.5, 0.25], "textures": {{ "skin": {tex256} }}, "parts": [ {{ "name": "p", "material": "stone", "texture": "skin", "shape": {{"op":"lathe","segments":{n},"profile":{p2}}} }} ] }}"#)),
        ),
        (
            "a lathe, no texture, four far levels, flat shaded, eight ao",
            Box::new(move |n: u32| {
                let ops: Vec<&str> = std::iter::repeat_n(r#"{"op":"ao","amount":0.3}"#, 8).collect();
                format!(r#"{{ "name": "t", "lods": [0.5, 0.25, 0.12, 0.06], "smooth_angle": 0, "parts": [ {{ "name": "p", "material": "stone", "shape": {{"op":"lathe","segments":{n},"profile":{p3}}}, "ops": [{}] }} ] }}"#, ops.join(","))
            }),
        ),
    ]
}

/// **The fourth review's three assets are told counts that build.** Each was told 108 (the
/// part's limit) and refused at 108 for work; now the work limit stops its most, the most
/// builds, the work the build spends there is within what the bound said, and the advice at
/// a limit just above what the most turns names a count that builds and holds.
#[test]
fn the_reviews_assets_are_told_counts_that_build() {
    for (label, at) in reviews_cases() {
        let text = at(8);
        let m = try_build(&text).unwrap_or_else(|e| panic!("{label}: {e}"));
        let f = facts_of(&m, &text);
        let r = f.round.iter().find(|r| r.count == "segments").unwrap_or_else(|| panic!("{label}: {:?}", f.round));
        assert!(r.most_by == WORK && r.most_estimated.is_none() && r.most < 108, "{label}: {r:?}");
        let built = try_build(&at(r.most)).unwrap_or_else(|e| panic!("{label}: told it builds with {}: {e}", r.most));
        let bound = r.work_at(r.most).unwrap();
        assert!(built.extras.work <= bound && bound <= specula_model::work::MAX_BUILD, "{label}: at {} it spent {}, the bound said {bound}", r.most, built.extras.work);
        let t = r.turn_at(r.most).unwrap();
        let (failed, fix) = verdict(&f, t * 1.0005 + 0.002);
        let k = named(&fix).unwrap_or_else(|| panic!("{label}: {fix}"));
        assert!(failed && k <= r.most, "{label}: told {k}: {fix}");
        let mk = try_build(&at(k)).unwrap_or_else(|e| panic!("{label}: told {k} ({fix}), refused: {e}"));
        assert!(!verdict(&facts_of(&mk, &at(k)), t * 1.0005 + 0.002).0, "{label}: told {k}, which fails");
        // (Below the limit no count holds: the advice says the most, and why it is the most.)
        let (_, fix) = verdict(&f, t * 0.9995 - 0.002);
        assert!(named(&fix).is_none() && fix.contains(&format!("({}; past it {WORK} would be exceeded)", r.most)), "{label}: {fix}");
    }
}

/// Every kind of round shape where the work limit binds: stacked copies of it, four far
/// levels, flat shading and fourteen `ao` ops, with the count it is built with first.
fn kinds() -> Vec<(&'static str, At, u32)> {
    let ao: Vec<&str> = std::iter::repeat_n(r#"{"op":"ao","amount":0.3}"#, 14).collect();
    let ao = ao.join(",");
    [
        (r#"{"op":"cylinder","radius":0.5,"height":1,"sides":{N}}"#, "sides", 50, 6u32),
        (r#"{"op":"cone","radius":0.5,"height":0.6,"sides":{N}}"#, "sides", 100, 6),
        (r#"{"op":"lathe","segments":{N},"profile":[[0,0],[0.5,0],[0.42,0.15],[0.2,0.5],[0.06,1],[0,1.05]]}"#, "segments", 40, 6),
        (r#"{"op":"torus","radius":0.6,"tube":0.2,"segments":{N},"sides":12}"#, "segments", 16, 6),
        (r#"{"op":"torus","radius":0.6,"tube":0.2,"segments":48,"sides":{N}}"#, "sides", 11, 3),
        (r#"{"op":"capsule","radius":0.3,"height":1.5,"segments":{N},"rings":4}"#, "segments", 24, 6),
        (r#"{"op":"capsule","radius":0.3,"height":1.5,"segments":32,"rings":{N}}"#, "rings", 16, 1),
        (r#"{"op":"sphere","radius":0.5,"subdivisions":{N}}"#, "subdivisions", 3, 1),
        (r#"{"op":"sdf","resolution":{N},"tree":{"op":"sphere","radius":0.5}}"#, "resolution", 8, 8),
    ]
    .into_iter()
    .map(|(shape, count, copies, first)| {
        let ao = ao.clone();
        let at: At = Box::new(move |n: u32| {
            format!(
                r#"{{ "name": "t", "lods": [0.5, 0.25, 0.12, 0.06], "smooth_angle": 0, "parts": [ {{ "name": "p", "material": "stone", "shape": {}, "ops": [{{"op":"array","count":{copies},"offset":[0,0,0]}},{ao}] }} ] }}"#,
                shape.replace("{N}", &n.to_string())
            )
        });
        (count, at, first)
    })
    .collect()
}

/// **Every kind, where the work limit binds, is told only counts that build**: the most is
/// said to be by the work limit (a sculpt's, estimated), it builds, the work spent there is
/// within the bound, and the advice at a limit just above what the most turns names a count
/// that builds and holds.
#[test]
fn every_kind_where_work_binds_is_told_counts_that_build() {
    for (count, at, first) in kinds() {
        let text = at(first);
        let m = try_build(&text).unwrap_or_else(|e| panic!("{count}: {e}"));
        let f = facts_of(&m, &text);
        let r = f.round.iter().find(|r| r.count == count).unwrap_or_else(|| panic!("{count}: {:?}", f.round));
        assert!(r.most_by == WORK && r.most_estimated.is_some() == (count == "resolution"), "{count}: {r:?}");
        let built = try_build(&at(r.most)).unwrap_or_else(|e| panic!("{count}: told it builds with {}: {e}", r.most));
        assert!(built.extras.work <= r.work_at(r.most).unwrap(), "{count}: at {} it spent {}", r.most, built.extras.work);
        let limit = r.turn_at(r.most).unwrap() * 1.0005 + 0.002;
        let (failed, fix) = verdict(&f, limit);
        let k = named(&fix).unwrap_or_else(|| panic!("{count}: {fix}"));
        assert!(failed && k <= r.most, "{count}: told {k}: {fix}");
        let mk = if k == r.most { built } else { try_build(&at(k)).unwrap_or_else(|e| panic!("{count}: told {k} ({fix}), refused: {e}")) };
        // (This shape holds the limit there; another round shape of the asset, a tube's sides,
        // may not, and is its own advice.)
        let fk = facts_of(&mk, &at(k));
        let rk = fk.round.iter().find(|x| x.count == count && x.count_value == k).unwrap();
        assert!(rk.turn_at(k).unwrap() <= limit, "{count}: told {k}, which turns {:?}", rk.turn_at(k));
    }
}

/// **How low the bound is** (slow: about a minute and a half in release; run by hand): for
/// each kind and the review's three, the most said against the most that builds, found by
/// halving between them. Every most said builds; the bound gives up the rest to be sure.
#[test]
#[ignore = "slow: about eight builds at the work limit a case; run by hand for the figures FORM.md states"]
fn how_low_the_work_bound_is() {
    let mut cases: Vec<(String, At, u32, &'static str)> = kinds().into_iter().map(|(c, at, first)| (c.to_string(), at, first, c)).collect();
    for (label, at) in reviews_cases() {
        cases.push((label.to_string(), at, 8, "segments"));
    }
    for (label, at, first, count) in cases {
        let text = at(first);
        let f = facts_of(&try_build(&text).unwrap(), &text);
        let r = f.round.iter().find(|r| r.count == count).unwrap();
        assert!(try_build(&at(r.most)).is_ok(), "{label}: {}", r.most);
        let (mut lo, mut hi) = (r.most, 513u32);
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if try_build(&at(mid)).is_ok() {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        println!("{label}: most said {} ({}), builds up to {lo}", r.most, r.most_by);
    }
}

/// A fixed sequence (a linear congruential generator: the same draws on every machine).
struct Draw(u64);

impl Draw {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// One drawn recipe with `{N}` for its round shape's count, the count's name and the count it
/// is built with first.
fn drawn(d: &mut Draw) -> (String, &'static str, u32) {
    let (shape, count, first) = match d.below(9) {
        0 => (r#"{"op":"cylinder","radius":0.5,"height":1,"sides":{N}}"#, "sides", 6 + d.below(10) as u32),
        1 => (r#"{"op":"cone","radius":0.5,"height":0.6,"sides":{N}}"#, "sides", 5 + d.below(10) as u32),
        2 => (r#"{"op":"lathe","segments":{N},"profile":[[0,0],[0.5,0],[0.42,0.15],[0.2,0.5],[0.06,1],[0,1.05]]}"#, "segments", 6 + d.below(10) as u32),
        3 => (r#"{"op":"torus","radius":0.6,"tube":0.2,"segments":{N},"sides":12}"#, "segments", 6 + d.below(10) as u32),
        4 => (r#"{"op":"torus","radius":0.6,"tube":0.2,"segments":24,"sides":{N}}"#, "sides", 3 + d.below(8) as u32),
        5 => (r#"{"op":"capsule","radius":0.3,"height":1.5,"segments":{N},"rings":4}"#, "segments", 6 + d.below(10) as u32),
        6 => (r#"{"op":"capsule","radius":0.3,"height":1.5,"segments":16,"rings":{N}}"#, "rings", 1 + d.below(4) as u32),
        7 => (r#"{"op":"sphere","radius":0.5,"subdivisions":{N}}"#, "subdivisions", d.below(3) as u32),
        _ => (r#"{"op":"cylinder","radius":0.5,"radius_top":0.3,"height":1,"sides":{N},"bevel":0.05}"#, "sides", 6 + d.below(10) as u32),
    };
    let pool = [
        r#"{"op":"ao","amount":0.3}"#,
        r#"{"op":"weld","eps":0.0001}"#,
        r#"{"op":"subdivide","levels":1}"#,
        r#"{"op":"mirror","axis":"x"}"#,
        r#"{"op":"noise_tint","amount":0.1}"#,
        r#"{"op":"scale","by":[1,1.5,0.7]}"#,
        r#"{"op":"rotate","deg":[10,30,0]}"#,
        r#"{"op":"array","count":3,"offset":[1.2,0,0]}"#,
        r#"{"op":"displace","amount":0.01,"scale":0.3}"#,
        r#"{"op":"move","by":[0,0.5,0]}"#,
    ];
    let ops: Vec<&str> = (0..d.below(4)).map(|_| pool[d.below(pool.len() as u64) as usize]).collect();
    let lods = ["[]", "[0.5]", "[0.5, 0.25]", "[0.5, 0.25, 0.12]"][d.below(4) as usize];
    let smooth = [0, 30, 60][d.below(3) as usize];
    let collider = ["aabb", "convex_hull", "none"][d.below(3) as usize];
    let textured = d.below(4) == 0;
    let (textures, tex) = if textured { (format!(r#""textures": {{ "skin": {} }},"#, texture(32, 1 + d.below(2) as usize)), r#""texture": "skin","#) } else { (String::new(), "") };
    let edits = if d.below(4) == 0 { r##""edits": [{"at": [0, 0.5, 0.5], "radius": 0.3, "color": "#8c96a3"}],"## } else { "" };
    // (A second part: plain, or round too, or a boolean beside it.)
    let other = match d.below(4) {
        0 => r#", { "name": "q", "material": "stone", "shape": {"op":"box","size":[0.4,0.4,0.4]}, "ops": [{"op":"move","by":[2,0,0]}] }"#.to_string(),
        1 => r#", { "name": "q", "material": "stone", "shape": {"op":"cylinder","radius":0.2,"height":0.5,"sides":10}, "ops": [{"op":"move","by":[-2,0,0]}] }"#.to_string(),
        2 => r#", { "name": "q", "material": "stone", "shape": {"op":"difference","of":[{"op":"box","size":[0.6,0.6,0.6]},{"op":"sphere","radius":0.35,"subdivisions":2}]}, "ops": [{"op":"move","by":[0,0,2]}] }"#.to_string(),
        _ => String::new(),
    };
    let text = format!(
        r#"{{ "name": "t", "lods": {lods}, "smooth_angle": {smooth}, "collider": "{collider}", {textures} {edits} "parts": [ {{ "name": "p", "material": "stone", {tex} "shape": {shape}, "ops": [{}] }}{other} ] }}"#,
        ops.join(",")
    );
    (text, count, first)
}

/// **The bound holds against the work real builds spend**: on 150 recipes drawn from a fixed
/// sequence, each round shape's bound at the count it was built with and at counts up to its
/// most (the next one, one between, and the most) is never below the work its build spends
/// there, and every one of those counts builds. Drawn small, so that it is cheap: the bound
/// is the same inequality wherever the limit falls. Returns nothing; says how close the bound
/// came (its worst and median over-estimate) where it fails.
#[test]
fn the_work_bound_holds_on_drawn_recipes() {
    let mut d = Draw(0x5eed_f00d);
    let (mut checked, mut over) = (0, Vec::new());
    for i in 0..150 {
        let (text, count, first) = drawn(&mut d);
        let at = |n: u32| text.replace("{N}", &n.to_string());
        let m = try_build(&at(first)).unwrap_or_else(|e| panic!("{i}: {}: {e}", at(first)));
        let f = facts_of(&m, &at(first));
        let Some(r) = f.round.iter().find(|r| r.path.starts_with("parts[0]") && r.count == count && r.count_value == first) else { continue };
        let bound = |k: u32| r.work_at(k).unwrap();
        assert!(bound(first) >= m.extras.work, "{i}: at the count built the bound {} is below what was spent, {}: {}", bound(first), m.extras.work, at(first));
        let mut ks = vec![first + 1, first + (r.most.saturating_sub(first)) / 2, r.most];
        ks.retain(|&k| k > first && k <= r.most);
        ks.dedup();
        for k in ks {
            // (A sculpt and a shape through a boolean are estimated: held all the same here.)
            let mk = try_build(&at(k)).unwrap_or_else(|e| panic!("{i}: {k} is within the most {} ({}), refused: {e}: {}", r.most, r.most_by, at(k)));
            assert!(mk.extras.work <= bound(k), "{i}: at {k} it spent {}, above the bound {}: {}", mk.extras.work, bound(k), at(k));
            over.push(bound(k) as f64 / mk.extras.work.max(1) as f64);
            checked += 1;
        }
    }
    over.sort_by(f64::total_cmp);
    assert!(checked >= 150, "{checked} counts checked");
    println!("{checked} counts checked; the bound over the work: median {:.2}, worst {:.2}", over[over.len() / 2], over[over.len() - 1]);
    // (How close: over-estimates are what the bound gives up for being sure.)
    let med = over[over.len() / 2];
    assert!(med < 4.0, "the bound is {med:.2} times the work at the median: too loose to be of use");
}
