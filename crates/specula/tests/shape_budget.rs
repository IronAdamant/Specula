//! Shape2d memory and time across entities (its own test binary: the registry of distinct shapes
//! and its vertex budget are process-wide, so no other test may run beside these).

use specula::two_d::{self, live_shape_vertices, Shape2dSpec, TwoD, MAX_WORLD_SHAPE_VERTICES};
use specula::world::{Shape, SpawnSpec, World};
use specula_core::Json;
use std::time::Instant;

/// 90 filled circles of 100 m at a 1 mm tolerance (within one shape's point budget): about 126,000 vertices.
fn circles(tag: u32) -> String {
    // Distinct shapes differ only in z_layer (the geometry, and so the vertex count, is the same).
    let items: Vec<String> = (0..90).map(|_| r##"{"circle":[0,0,100],"fill":"#ffffff"}"##.to_owned()).collect();
    format!(r#"{{"items":[{}],"tolerance":0.001,"z_layer":{tag}}}"#, items.join(","))
}

fn spawn(world: &mut World, name: &str, text: &str) -> Result<(), String> {
    let mut spec = SpawnSpec::new(Shape::Shape2d);
    spec.name = Some(name.into());
    spec.two_d = Some(two_d::parse("shape2d", &Json::parse(text).map_err(|e| format!("{e:?}"))?)?);
    world.spawn(spec, "test").map(|_| ()).map_err(|e| e.message)
}

#[test]
fn identical_shapes_share_and_distinct_ones_have_a_world_budget() {
    // 100 entities with one shape: one tessellation, one mesh.
    let mut world = World::default();
    let text = circles(0);
    let t = Instant::now();
    for i in 0..100 {
        spawn(&mut world, &format!("s{i}"), &text).unwrap();
    }
    let elapsed = t.elapsed();
    let first = match &world.entities().next().unwrap().two_d {
        Some(TwoD::Shape2d(s)) => s.clone(),
        _ => panic!("shape2d"),
    };
    let one = first.mesh().unwrap().vertices.len();
    assert!(one > 100_000, "{one}");
    assert!(world.entities().all(|e| matches!(&e.two_d, Some(TwoD::Shape2d(s)) if std::sync::Arc::ptr_eq(s, &first))), "one spec for all");
    assert_eq!(live_shape_vertices(), one, "counted once");
    eprintln!("100 identical spawns in {:.0} ms", elapsed.as_secs_f64() * 1e3);
    assert!(elapsed.as_secs_f64() < 2.0);
    // Distinct shapes fill the budget, then the next is refused with a structured message.
    let mut kept = Vec::new();
    let mut refused = None;
    for k in 1..40 {
        match Shape2dSpec::parse(&Json::parse(&circles(k)).unwrap()) {
            Ok(s) => kept.push(s),
            Err(e) => {
                refused = Some((k, e));
                break;
            }
        }
    }
    let (k, e) = refused.expect("the budget refuses at some point");
    assert!(e.starts_with("shape2d_budget"), "{e}");
    assert!(live_shape_vertices() <= MAX_WORLD_SHAPE_VERTICES);
    eprintln!("refused distinct shape {k}: {e}");
    // Freed shapes give their budget back.
    drop(kept);
    assert_eq!(live_shape_vertices(), one);
    assert!(Shape2dSpec::parse(&Json::parse(&circles(1000)).unwrap()).is_ok());
}

#[test]
fn loading_defers_tessellation() {
    // As world files load: validated now, tessellated later (here: on demand).
    let text = r##"{"items":[{"polygon":[[0,0],[5,0],[5,5],[2.5,1],[0,5]],"fill":"#123456"}],"z_layer":3}"##;
    let t = Instant::now();
    let a = two_d::deferred(|| Shape2dSpec::parse(&Json::parse(text).unwrap())).unwrap();
    let parse_ms = t.elapsed().as_secs_f64() * 1e3;
    assert!(a.mesh().is_none() && a.mesh_error().is_none(), "nothing tessellated while loading");
    let (lo, hi) = a.bounds();
    assert!(lo.x <= 0.0 && hi.x >= 5.0 && hi.y >= 5.0, "conservative bounds before tessellating");
    a.ensure().unwrap();
    assert!(a.mesh().is_some());
    let (lo, hi) = a.bounds();
    assert!((lo.x - 0.0).abs() < 1e-4 && (hi.x - 5.0).abs() < 1e-4, "exact bounds after");
    eprintln!("deferred parse {parse_ms:.2} ms");
    // A bad shape in a world file loads, and fails (with its reason) only when tessellated.
    let bad = r##"{"items":[{"polygon":[[0,0],[1,1],[1,0],[0,1]],"fill":"#ffffff"}]}"##;
    let b = two_d::deferred(|| Shape2dSpec::parse(&Json::parse(bad).unwrap())).unwrap();
    assert!(b.ensure().is_err() && b.mesh_error().is_some_and(|e| e.contains("crosses itself") || e.contains("no area")));
}
