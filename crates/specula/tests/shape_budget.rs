//! Shape2d budgets are world state: charged from each shape's description alone (no
//! tessellation) as entities enter and leave a world, so the same spawns, sets, deletes and loads
//! get the same answers on every machine, whatever has or hasn't been drawn.

use specula::persist;
use specula::two_d::{self, Shape2dSpec, TwoD, MAX_WORLD_SHAPE_VERTICES};
use specula::world::{EntityKey, Op, Shape, SpawnSpec, UpdateSpec, World, WorldError};
use specula_core::Json;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

/// 90 filled circles of 100 m at a 1 mm tolerance (within one shape's point budget): about
/// 126,000 vertices. Distinct shapes differ only in z_layer (so they cost the same).
fn circles(tag: u32) -> String {
    let items: Vec<String> = (0..90).map(|_| r##"{"circle":[0,0,100],"fill":"#ffffff"}"##.to_owned()).collect();
    format!(r#"{{"items":[{}],"tolerance":0.001,"z_layer":{tag}}}"#, items.join(","))
}

/// Parsed as world files are: validated and charged, not tessellated.
fn deferred(text: &str) -> TwoD {
    two_d::deferred(|| two_d::parse("shape2d", &Json::parse(text).unwrap())).unwrap()
}

fn spec_of(t: &TwoD) -> Arc<Shape2dSpec> {
    match t {
        TwoD::Shape2d(s) => Arc::clone(s),
        _ => panic!("shape2d"),
    }
}

fn spawn(world: &mut World, name: &str, t: TwoD) -> Result<u64, WorldError> {
    let mut spec = SpawnSpec::new(Shape::Shape2d);
    spec.name = Some(name.into());
    spec.two_d = Some(t);
    world.spawn(spec, "test").map(|e| e.id)
}

fn assert_limit(e: &WorldError, value: usize) {
    assert_eq!(e.code, "limit", "{e}");
    assert_eq!(e.detail("field").and_then(Json::as_str), Some("shape2d"));
    assert_eq!(e.detail("limit").and_then(Json::as_u64), Some(MAX_WORLD_SHAPE_VERTICES as u64));
    assert_eq!(e.detail("value").and_then(Json::as_u64), Some(value as u64), "{e}");
}

/// A world holding `n` distinct heavy shapes (z_layer 1..=n).
fn full_world(n: u32) -> World {
    let mut w = World::default();
    for k in 1..=n {
        spawn(&mut w, &format!("c{k}"), deferred(&circles(k))).unwrap();
    }
    w
}

/// Vertices one heavy shape is charged, and how many distinct ones fit in a world.
fn heavy() -> (usize, u32) {
    let per = spec_of(&deferred(&circles(0))).vertices;
    (per, (MAX_WORLD_SHAPE_VERTICES / per) as u32)
}

#[test]
fn identical_shapes_count_once_and_distinct_ones_fill_the_budget() {
    // 100 entities with one shape: one tessellation, one charge.
    let mut world = World::default();
    let text = circles(0);
    let t = Instant::now();
    for i in 0..100 {
        spawn(&mut world, &format!("s{i}"), two_d::parse("shape2d", &Json::parse(&text).unwrap()).unwrap()).unwrap();
    }
    let elapsed = t.elapsed();
    let first = spec_of(world.entities().next().unwrap().two_d.as_ref().unwrap());
    let mesh = first.mesh().unwrap().vertices.len();
    assert!(mesh > 100_000 && first.vertices >= mesh, "bound {} vs mesh {mesh}", first.vertices);
    assert!(first.vertices < mesh + mesh / 10, "the bound stays close for fills: {} vs {mesh}", first.vertices);
    // One spec (another test parsing the same text at the same moment may intern a twin: equal
    // JSON, so the world still counts one).
    assert!(world.entities().all(|e| spec_of(e.two_d.as_ref().unwrap()).same(&first)), "one shape for all");
    assert_eq!((world.shape_budget().distinct(), world.shape_budget().vertices()), (1, first.vertices), "counted once");
    eprintln!("100 identical spawns in {:.0} ms", elapsed.as_secs_f64() * 1e3);
    assert!(elapsed.as_secs_f64() < 2.0);

    // Distinct shapes fill the budget, then the next is refused with a structured error. The
    // answer is the same whether the shapes were tessellated or not.
    let (per, fit) = heavy();
    assert_eq!(per, first.vertices);
    let mut refused = None;
    for k in 1..40 {
        if let Err(e) = spawn(&mut world, &format!("d{k}"), deferred(&circles(k))) {
            refused = Some((k, e));
            break;
        }
    }
    let (k, e) = refused.expect("the budget refuses at some point");
    assert_eq!(k, fit, "1 + {} distinct shapes fit", fit - 1);
    assert_limit(&e, (fit as usize + 1) * per);
    assert!(world.shape_budget().vertices() <= MAX_WORLD_SHAPE_VERTICES);
    eprintln!("refused distinct shape {k}: {}", e.message);

    // Refusals are never remembered by the shape: it tessellates fine, and fits once there's room.
    let refused_shape = deferred(&circles(k));
    assert!(spec_of(&refused_shape).ensure().is_ok());
    let again = spawn(&mut world, "again", refused_shape.clone()).unwrap_err();
    assert_limit(&again, (fit as usize + 1) * per);
    world.delete(&EntityKey::Path("/d1".into()), None).unwrap();
    spawn(&mut world, "again", refused_shape).unwrap();
    // Deleting every user gives the budget back.
    let names: Vec<String> = world.entities().map(|e| e.name.clone()).collect();
    for n in names {
        world.delete(&EntityKey::Path(format!("/{n}")), None).unwrap();
    }
    assert_eq!((world.shape_budget().distinct(), world.shape_budget().vertices()), (0, 0));
}

#[test]
fn sets_batches_and_rollbacks_keep_the_count_exact() {
    let (per, fit) = heavy();
    let mut w = full_world(fit);
    let full = w.shape_budget().vertices();
    assert_eq!(full, fit as usize * per);
    let tri = deferred(r##"{"items":[{"polygon":[[0,0],[1,0],[0,1]],"fill":"#ff0000"}]}"##);
    let small = spec_of(&tri).vertices;
    assert!(small <= 6, "{small}");
    // Room for a small one, not for another heavy one.
    spawn(&mut w, "tri", tri).unwrap();
    // A set that swaps a shape for another of the same cost fits (the old one is released).
    let set = |shape: TwoD| UpdateSpec { two_d: Some(shape), ..Default::default() };
    w.update(&EntityKey::Path("/c1".into()), set(deferred(&circles(500))), "test").unwrap();
    assert_eq!(w.shape_budget().vertices(), full + small);
    // A set that grows past the budget is refused and changes nothing.
    let e = w.update(&EntityKey::Path("/tri".into()), set(deferred(&circles(501))), "test").unwrap_err();
    assert_limit(&e, full + per);
    assert_eq!(spec_of(w.get(w.find_path("/tri").unwrap()).unwrap().two_d.as_ref().unwrap()).vertices, small);
    // A batch can free room and use it; a failing batch leaves the count as it was.
    let named = |name: &str, t: TwoD| {
        let mut s = SpawnSpec::new(Shape::Shape2d);
        s.name = Some(name.into());
        s.two_d = Some(t);
        s
    };
    let ops = vec![Op::Delete(EntityKey::Path("/c2".into()), None), Op::Spawn(named("n1", deferred(&circles(600))))];
    w.apply_batch(ops, "test").unwrap();
    assert_eq!(w.shape_budget().vertices(), full + small);
    let s2 = named("n2", deferred(&circles(601)));
    let before = w.shape_budget().vertices();
    let ops = vec![Op::Delete(EntityKey::Path("/c3".into()), None), Op::Spawn(s2), Op::Delete(EntityKey::Path("/nope".into()), None)];
    assert!(w.apply_batch(ops, "test").is_err());
    assert_eq!(w.shape_budget().vertices(), before, "rolled back");
    assert_eq!(w.shape_budget().distinct(), fit as usize + 1);
}

#[test]
fn loads_are_charged_and_a_world_over_budget_is_refused_whole() {
    let (per, fit) = heavy();
    let mut w = full_world(fit);
    // Shares the last shape: free.
    spawn(&mut w, "zz_extra", deferred(&circles(fit))).unwrap();
    let files: BTreeMap<String, String> = persist::render(&w).into_iter().collect();
    let load = |files: &BTreeMap<String, String>| {
        persist::load_with(|f| files.get(f).cloned().ok_or_else(|| persist::PersistError::new("not_found", f))).map(|(w, _)| w)
    };
    let mut back = load(&files).unwrap();
    assert_eq!(back.shape_budget().vertices(), fit as usize * per);
    assert_eq!(back.state_hash(), w.state_hash());
    // Loaded shapes hold their budget: a later spawn can't take it from them.
    let e = spawn(&mut back, "late", deferred(&circles(999))).unwrap_err();
    assert_limit(&e, (fit as usize + 1) * per);
    // A file edited to need more than the budget is refused as a whole, with the limit's details.
    let (name, text) = files.iter().find(|(_, t)| t.contains("zz_extra")).map(|(n, t)| (n.clone(), t.clone())).unwrap();
    let at = text.rfind("\"z_layer\"").unwrap();
    let digits = text[at + 9..].find(|c: char| c.is_ascii_digit()).unwrap() + at + 9;
    let end = text[digits..].find(|c: char| !c.is_ascii_digit()).unwrap() + digits;
    assert_eq!(&text[digits..end], fit.to_string(), "the extra's shape is the last in its file");
    let mut edited = files.clone();
    edited.insert(name, format!("{}{}{}", &text[..digits], 7777, &text[end..]));
    let err = load(&edited).unwrap_err();
    assert_eq!(err.code, "limit", "{err:?}");
    let j = err.to_json();
    assert_eq!(j.get("field").and_then(Json::as_str), Some("shape2d"));
    assert_eq!(j.get("limit").and_then(Json::as_u64), Some(MAX_WORLD_SHAPE_VERTICES as u64));
    assert_eq!(j.get("value").and_then(Json::as_u64), Some((fit as u64 + 1) * per as u64));
    assert!(j.get("entity").and_then(Json::as_str).is_some_and(|e| e.contains("zz_extra")), "{j}");
}

#[test]
fn bounds_come_from_the_description_and_never_change() {
    // A sharp miter (limit 100) reaches far past its points.
    let text = r##"{"items":[{"polyline":[[-1,0],[0,0],[-1,0.03]],"stroke":{"width":1,"join":"miter","miter_limit":100}}]}"##;
    let t = deferred(text);
    let s = spec_of(&t);
    let before = s.bounds();
    let mesh = Arc::clone(s.ensure().unwrap());
    assert_eq!(s.bounds(), before, "the same after tessellating");
    assert!(s.vertices >= mesh.vertices.len());
    for v in &mesh.vertices {
        let p = v.position;
        assert!(p[0] >= before.0.x && p[0] <= before.1.x && p[1] >= before.0.y && p[1] <= before.1.y, "{p:?} outside {before:?}");
    }
    let tip = mesh.vertices.iter().map(|v| v.position[0]).fold(f32::NEG_INFINITY, f32::max);
    assert!(tip > 10.0, "the miter tip reaches far: {tip}");
    // Arcs: centre ± radii (plus a hair), not the extent from the origin.
    let c = spec_of(&deferred(r##"{"items":[{"circle":[100,50,2],"fill":"#fff"}]}"##));
    let (lo, hi) = c.bounds();
    assert!((lo.x - 98.0).abs() < 1e-3 && (hi.x - 102.0).abs() < 1e-3 && (lo.y - 48.0).abs() < 1e-3 && (hi.y - 52.0).abs() < 1e-3, "{lo:?} {hi:?}");
    // A path that draws before any move starts at the origin: its box includes it.
    for text in [
        r##"{"items":[{"path":[["L",100,0],["L",101,0]],"stroke":{"width":1}}]}"##,
        r##"{"items":[{"path":[["Q",5,9,10,3],["L",12,8]],"stroke":{"width":0.2,"join":"round"}}]}"##,
    ] {
        let s = spec_of(&deferred(text));
        let (lo, hi) = s.bounds();
        assert!(lo.x <= 0.0 && lo.y <= 0.0 && hi.x > 0.0, "{text}: {lo:?} {hi:?}");
        let mesh = Arc::clone(s.ensure().unwrap());
        assert!(s.vertices >= mesh.vertices.len());
        for v in &mesh.vertices {
            let p = v.position;
            assert!(p[0] >= lo.x && p[0] <= hi.x && p[1] >= lo.y && p[1] <= hi.y, "{text}: {p:?} outside {lo:?}..{hi:?}");
        }
    }
    // Round joins are charged per corner, by how far each turns: a stroked circle costs what it
    // draws.
    for text in [
        r##"{"items":[{"circle":[0,0,10],"stroke":{"width":2,"join":"round"}}]}"##,
        r##"{"items":[{"circle":[0,0,10],"stroke":{"width":2,"join":"round"}}],"tolerance":0.0005}"##,
    ] {
        let s = spec_of(&deferred(text));
        let actual = s.ensure().unwrap().vertices.len();
        assert_eq!(s.vertices, actual, "{text}");
    }
    // What a world reports is that box, drawn or not.
    let mut w = World::default();
    let id = spawn(&mut w, "c", TwoD::Shape2d(Arc::clone(&c))).unwrap();
    assert_eq!(w.get(id).unwrap().local_bounds(), Some((lo, hi)));
}

#[test]
fn a_bad_shape_in_a_world_file_loads_and_fails_only_when_drawn() {
    let text = r##"{"items":[{"polygon":[[0,0],[5,0],[5,5],[2.5,1],[0,5]],"fill":"#123456"}],"z_layer":3}"##;
    let a = spec_of(&deferred(text));
    assert!(a.mesh().is_none() && a.mesh_error().is_none(), "nothing tessellated while loading");
    a.ensure().unwrap();
    // Geometry only tessellation finds (a bow-tie) loads, and says why when drawn.
    let bad = r##"{"items":[{"polygon":[[0,0],[1,1],[1,0],[0,1]],"fill":"#ffffff"}]}"##;
    let b = spec_of(&deferred(bad));
    assert!(b.ensure().is_err() && b.mesh_error().is_some_and(|e| e.contains("crosses itself") || e.contains("no area")));
    // Too many points is known from the description: refused while loading.
    let pts: Vec<String> = (0..70_000).map(|k| format!("[{k},{}]", k % 2)).collect();
    let huge = format!(r##"{{"items":[{{"polyline":[{}],"stroke":{{"width":0.1}}}}]}}"##, pts.join(","));
    assert!(two_d::deferred(|| two_d::parse("shape2d", &Json::parse(&huge).unwrap())).is_err());
}

#[test]
fn many_small_distinct_shapes_stay_quick() {
    // Charging is a map lookup per spawn: no scan of the world's shapes.
    let mut w = World::default();
    let t = Instant::now();
    let mut last = Instant::now();
    let mut laps = Vec::new();
    for k in 0..20_000 {
        let text = format!(r##"{{"items":[{{"polygon":[[0,0],[1,0],[0,1]],"fill":"#ffffff"}}],"z_layer":{}}}"##, k - 500_000);
        spawn(&mut w, &format!("t{k}"), two_d::parse("shape2d", &Json::parse(&text).unwrap()).unwrap()).unwrap();
        if (k + 1) % 5_000 == 0 {
            laps.push(last.elapsed().as_secs_f64());
            last = Instant::now();
        }
    }
    eprintln!("20k distinct shapes in {:.2} s, laps {laps:?}", t.elapsed().as_secs_f64());
    assert_eq!(w.shape_budget().distinct(), 20_000);
    assert!(laps[3] < laps[0] * 3.0 + 0.05, "no slowdown as the world fills: {laps:?}");
}

fn spawn_transient(world: &mut World, name: &str, t: TwoD) -> Result<u64, WorldError> {
    let mut spec = SpawnSpec::new(Shape::Shape2d);
    spec.name = Some(name.into());
    spec.two_d = Some(t);
    spec.transient = true;
    world.spawn(spec, "game").map(|e| e.id)
}

#[test]
fn transient_and_persistent_shapes_have_separate_budgets() {
    let (per, fit) = heavy();
    let limits = two_d::ShapeLimits::default();
    assert_eq!((limits.persistent, limits.transient), (MAX_WORLD_SHAPE_VERTICES, two_d::MAX_TRANSIENT_SHAPE_VERTICES));
    // A transient flood (a game's effects) fills its own pool, and can't block authored content.
    let mut w = World::default();
    let mut k = 0;
    let refused = loop {
        match spawn_transient(&mut w, &format!("fx{k}"), deferred(&circles(1000 + k))) {
            Ok(_) => k += 1,
            Err(e) => break e,
        }
    };
    assert_eq!(k as usize, limits.transient / per, "transient shapes that fit");
    assert_eq!(refused.code, "limit");
    assert_eq!(refused.detail("budget").and_then(Json::as_str), Some("transient_shape2d_vertices"), "{refused}");
    assert_eq!(refused.detail("limit").and_then(Json::as_u64), Some(limits.transient as u64));
    assert!(refused.message.contains("transient"), "{}", refused.message);
    for j in 1..=fit {
        spawn(&mut w, &format!("art{j}"), deferred(&circles(j))).unwrap();
    }
    assert_eq!(w.shape_budget().vertices(), fit as usize * per);
    assert_eq!(w.transient_shape_budget().vertices(), k as usize * per);
    // A persistent flood can't block the game's effects either.
    let mut w = World::default();
    let mut j = 0;
    let refused = loop {
        match spawn(&mut w, &format!("art{j}"), deferred(&circles(j))) {
            Ok(_) => j += 1,
            Err(e) => break e,
        }
    };
    assert_eq!(refused.detail("budget").and_then(Json::as_str), Some("shape2d_vertices"), "{refused}");
    spawn_transient(&mut w, "fx", deferred(&circles(5000))).unwrap();
    // Deleting a transient entity gives its own pool back, not the other one.
    let before = w.shape_budget().vertices();
    w.delete(&EntityKey::Path("/fx".into()), None).unwrap();
    assert_eq!((w.shape_budget().vertices(), w.transient_shape_budget().vertices()), (before, 0));
}

#[test]
fn configured_limits_apply_to_spawns_and_loads() {
    let (per, _) = heavy();
    let mut w = World::default();
    w.set_shape_limits(two_d::ShapeLimits { persistent: 3 * per, transient: 1 });
    for k in 0..3 {
        spawn(&mut w, &format!("c{k}"), deferred(&circles(k))).unwrap();
    }
    let e = spawn(&mut w, "c3", deferred(&circles(3))).unwrap_err();
    assert_eq!((e.detail("limit").and_then(Json::as_u64), e.detail("value").and_then(Json::as_u64)), (Some(3 * per as u64), Some(4 * per as u64)));
    assert!(spawn_transient(&mut w, "fx", deferred(&circles(9))).is_err(), "a 1-vertex transient budget refuses any shape");
    // A world saved under a bigger budget is refused whole by a smaller one, and loads under its own.
    let files: BTreeMap<String, String> = persist::render(&w).into_iter().collect();
    let read = |f: &str| files.get(f).cloned().ok_or_else(|| persist::PersistError::new("not_found", f));
    let err = persist::load_with_limits(two_d::ShapeLimits { persistent: 2 * per, transient: 1 }, read).unwrap_err();
    assert_eq!(err.code, "limit");
    let (back, _) = persist::load_with_limits(two_d::ShapeLimits { persistent: 3 * per, transient: 1 }, read).unwrap();
    assert_eq!(back.shape_limits().persistent, 3 * per, "a loaded world keeps the limits it was checked against");
    assert_eq!(back.shape_budget().vertices(), 3 * per);
}
