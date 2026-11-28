//! The shape `body` through the modeller (docs/design/DISTANCE_FIELDS.md section 7,
//! docs/design/FABRICA.md step 3): the four test skeletons each meshed as one closed surface,
//! the same bytes every time, refusals as data with their paths, a body past its work refused
//! by name before it runs long, and the places a body keeps through its meshing as anchors.

#![allow(clippy::disallowed_methods, reason = "a test harness: the build times printed are measurements outside every result")]

use std::collections::BTreeMap;

use specula_core::Json;
use specula_model::caro::{joints_of, Bone, Carnis, Flesh};
use specula_model::sdf::{mesh, Kind, MeshOptions, Modifiers, Prim, SdfNode};
use specula_model::{build, BuildParams, ErrorCode, MeshData, Model, RecipeError, Style};

const BODIES: [(&str, &str); 5] = [
    ("biped", include_str!("bodies/biped.json")),
    ("quadruped", include_str!("bodies/quadruped.json")),
    ("serpent", include_str!("bodies/serpent.json")),
    ("forked", include_str!("bodies/forked.json")),
    ("pelvis_legs", include_str!("bodies/pelvis_legs.json")),
];

fn style() -> Style {
    Style::parse(r##"{ "style": "t", "palette": { "dark": "#202020" }, "materials": { "skin": { "color": "#c8a08a", "roughness": 0.7 } } }"##).unwrap()
}

fn make(text: &str) -> Result<Model, RecipeError> {
    let s = style();
    let lib = BTreeMap::new();
    build(text, &BuildParams { library: Some(&lib), ..BuildParams::new(&s) })
}

/// The stats of the model's first part's shape.
fn stat(m: &Model) -> &Json {
    &m.extras.stats.iter().find(|(p, _)| p == "parts[0].shape").expect("the body's stats").1
}

/// How many pieces the mesh is in, its vertices welded by place.
fn pieces(l: &MeshData) -> usize {
    let mut at: BTreeMap<[u32; 3], usize> = BTreeMap::new();
    let ids: Vec<usize> = l
        .positions
        .iter()
        .map(|p| {
            let n = at.len();
            *at.entry(p.map(f32::to_bits)).or_insert(n)
        })
        .collect();
    let mut up: Vec<usize> = (0..at.len()).collect();
    fn root(up: &mut [usize], mut i: usize) -> usize {
        while up[i] != i {
            up[i] = up[up[i]];
            i = up[i];
        }
        i
    }
    for t in l.indices.chunks(3) {
        let (a, b, c) = (root(&mut up, ids[t[0] as usize]), root(&mut up, ids[t[1] as usize]), root(&mut up, ids[t[2] as usize]));
        up[b] = a;
        let c2 = root(&mut up, c);
        up[c2] = a;
    }
    (0..up.len()).filter(|&i| root(&mut up, i) == i).count()
}

/// Each of the five skeletons builds through a recipe into one closed surface: no open edge,
/// one piece (no rim where parts were stitched: there are none), the modeller's checks passed.
#[test]
fn each_test_body_meshes_closed_as_one_surface() {
    for (name, text) in BODIES {
        let start = std::time::Instant::now();
        let m = make(text).unwrap_or_else(|er| panic!("{name}: {er}"));
        let took = start.elapsed();
        let l = &m.lods[0];
        l.validate().unwrap_or_else(|er| panic!("{name}: {er:?}"));
        assert_eq!(l.open_edge_count(), 0, "{name}: open edges");
        assert_eq!(pieces(l), 1, "{name}: in more than one piece");
        let s = stat(&m);
        let num = |k: &str| s.get(k).and_then(|v| v.as_f64()).unwrap_or(-1.0);
        assert!(num("seams") > 0.0, "{name}: no joint narrows its width");
        // The mesher's blocks are eight cells a side over the grid's samples.
        let grid: Vec<f64> = s.get("grid").and_then(|g| g.as_array()).map(|g| g.iter().filter_map(|d| d.as_f64()).collect()).unwrap_or_default();
        let blocks: f64 = grid.iter().map(|d| ((d - 1.0) / 8.0).ceil()).product();
        assert!(num("blocks_skipped") > 0.0, "{name}: no block was skipped of {blocks}");
        println!(
            "{name}: {} triangles, {} work units, grid {grid:?}, cell {} m, {} samples evaluated, {} blocks skipped of {blocks}, {} parts, {} seams, built in {:.2} s (with its levels)",
            l.indices.len() / 3,
            num("work"),
            num("cell_m"),
            num("samples_evaluated"),
            num("blocks_skipped"),
            num("parts"),
            num("seams"),
            took.as_secs_f64()
        );
    }
}

/// What [`the_same_bytes_every_time`] read, in debug and in release (moved on 2026-11-28, on
/// purpose: DISTANCE_FIELDS.md 7.9's join, 7.1's sloped ends and their caps, 7.8's step and
/// softened width, the widened reach and its hold at 4K, which moved three of the four again;
/// then a tapering run's joint widened to the angle its bones' surfaces cross at, which moved
/// all but pelvis_legs, whose runs do not taper at their joints).
const HASHES: [u64; 5] = [0x6857_06a5_07bc_14cf, 0x790f_0996_c6fe_73c8, 0xc211_7686_5302_fa8e, 0x5494_67ce_db2b_8fd6, 0xf8a8_b30e_c37d_dc16];

/// The same recipe makes the same bytes twice, and the same in a debug and a release build.
#[test]
fn the_same_bytes_every_time() {
    let mut got = [0u64; 5];
    for (i, (name, text)) in BODIES.iter().enumerate() {
        let (a, b) = (make(text).unwrap(), make(text).unwrap());
        assert_eq!(a.lods[0].content_hash(), b.lods[0].content_hash(), "{name}: two builds differ");
        got[i] = a.lods[0].content_hash();
    }
    println!("hashes: {got:#x?}");
    assert_eq!(got, HASHES, "the bodies' bytes changed");
}

fn arm(extra: &str, hand: &str) -> String {
    format!(
        r#"{{ "name": "arm", "parts": [ {{ "name": "flesh", "material": "skin", "shape": {{ "op": "body", "resolution": 80, {extra} "bones": [
            {{ "name": "shoulder", "head": [0, 1.4, 0], "tail": [0.3, 1.4, 0], "sections": [[0, 0.05, 0.045], [1, 0.04, 0.035]] }},
            {{ "name": "forearm", "parent": "shoulder", "head": [0.3, 1.4, 0], "tail": [0.55, 1.4, 0], "sections": [[0, 0.04, 0.035], [1, 0.03, 0.025]] }},
            {{ "name": "hand", "parent": "forearm", "head": [0.55, 1.4, 0], "tail": {hand}, "sections": [[0, 0.03, 0.015], [1, 0.022, 0.01]], "square": 3,
              "places": [ {{ "name": "tip", "at": 1.0, "side": [0, 1] }}, {{ "name": "palm", "at": 0.5, "side": [0, -1], "depth": 0.004 }}, {{ "name": "edge", "at": 0.3, "side": [1, 0] }} ] }}
        ] }} }} ] }}"#
    )
}

/// Every refusal names the field and the bone it is about.
#[test]
fn hostile_body_recipes_are_refused_with_a_reason() {
    assert!(make(&arm("", "[0.72, 1.4, 0]")).is_ok());
    let refused = |text: String| make(&text).err().map(|er| (er.code, er.path)).unwrap_or_else(|| panic!("accepted: {text}"));
    let bones = |b: &str| format!(r#"{{ "name": "t", "parts": [ {{ "name": "p", "material": "skin", "shape": {{ "op": "body", "bones": [ {b} ] }} }} ] }}"#);
    let root = r#"{ "name": "root", "head": [0, 0, 0], "tail": [0, 0.5, 0], "sections": [[0, 0.05, 0.05], [1, 0.05, 0.05]] }"#;
    let bone = |s: &str| format!(r#"{root}, {{ "name": "b", "parent": "root", "head": [0, 0.5, 0], "tail": [0, 1, 0], {s} }}"#);
    assert_eq!(refused(bones(&format!(r#"{root}, {{ "name": "root", "parent": "root", "head": [0, 1, 0] }}"#))), (ErrorCode::Duplicate, "parts[0].shape.bones[1].name".into()));
    assert_eq!(refused(bones(&format!(r#"{root}, {{ "name": "b", "parent": "nobody", "head": [0, 1, 0] }}"#))).0, ErrorCode::UnknownName);
    assert_eq!(refused(bones(&format!(r#"{root}, {{ "name": "b", "head": [0, 1, 0] }}"#))).0, ErrorCode::MissingField);
    assert_eq!(refused(bones(&format!(r#"{root}, {{ "name": "b", "parent": "c", "head": [0, 1, 0] }}, {{ "name": "c", "parent": "b", "head": [0, 1, 0] }}"#))).0, ErrorCode::InvalidRig);
    assert_eq!(refused(bones(&bone(r#""square": 1.5"#))).0, ErrorCode::OutOfRange);
    assert_eq!(refused(bones(&bone(r#""side": [0, 0, 0]"#))).0, ErrorCode::OutOfRange);
    assert_eq!(refused(bones(&format!(r#"{root}, {{ "name": "b", "parent": "root", "head": [0, 0.5, 0], "tail": [0, 0.5, 0.0001] }}"#))).0, ErrorCode::Geometry);
    let (code, path) = refused(bones(&bone(r#""sections": [[0, 0.05, 0.05], [0.5, 9, 0.05]]"#)));
    assert_eq!((code, path.starts_with("parts[0].shape.bones[1]")), (ErrorCode::OutOfRange, true), "{path}");
    assert_eq!(refused(bones(&bone(r#""places": [{ "name": "x", "at": 0.5, "side": [0, 0] }]"#))).0, ErrorCode::OutOfRange);
    assert_eq!(refused(bones(&bone(r#""places": [{ "name": "x", "at": 0.5 }, { "name": "x", "at": 0.6 }]"#))).0, ErrorCode::Duplicate);
    assert_eq!(refused(bones(&bone(r#""mirror": true"#))).0, ErrorCode::InvalidRig);
    let many: Vec<String> = (0..70).map(|i| format!(r#"{{ "name": "b{i}", "parent": "root", "head": [0, 0.5, 0], "tail": [0, 1, 0] }}"#)).collect();
    assert_eq!(refused(bones(&format!("{root}, {}", many.join(", ")))).0, ErrorCode::Limit);
    assert_eq!(refused(arm(r#""smooth": 0.9,"#, "[0.72, 1.4, 0]")).0, ErrorCode::OutOfRange);
    assert_eq!(refused(arm(r#""fine": [{ "bone": "elbow" }],"#, "[0.72, 1.4, 0]")).0, ErrorCode::UnknownName);
}

/// A body past the work it is given is refused by name, as a limit, with what it had reached
/// and the limit; it stops at the charge that passes the limit (charged before it is done), so
/// it never runs long past it.
#[test]
fn a_body_over_its_work_limit_is_refused_by_name() {
    let ring = |t: f32, r: f32| [t, r, r];
    let bones = vec![
        Bone { head: [0.0, 0.0, 0.0], tail: [0.0, 0.5, 0.0], side: None, square: 2.0, sections: vec![ring(0.0, 0.1), ring(1.0, 0.08)] },
        Bone { head: [0.0, 0.5, 0.0], tail: [0.0, 1.0, 0.0], side: None, square: 3.0, sections: vec![ring(0.0, 0.08), ring(1.0, 0.06)] },
    ];
    let flesh = Flesh::jointed(&bones, &[], 0.02, 0.05, &joints_of(&bones, &[None, Some(0)])).unwrap();
    let tree = SdfNode { kind: Kind::Prim(Prim::Body(Carnis::new(flesh))), m: Modifiers::default() };
    let opts = |max_work: u64| MeshOptions { resolution: 120, bounds: None, seed: 1, max_triangles: 1 << 22, max_work, color: [0.8, 0.6, 0.55, 1.0] };
    let (_, st) = mesh(&tree, &opts(u64::MAX / 4)).unwrap();
    let limit = st.work / 3;
    let er = mesh(&tree, &opts(limit)).unwrap_err();
    assert!(er.limit, "{er:?}");
    assert_eq!(er.max, Some(limit as f64));
    let reached = er.measured.unwrap();
    assert!(reached > limit as f64 && reached < limit as f64 + 1e6, "stopped at {reached} of {limit}");
    assert!(er.message.contains("work units"), "{}", er.message);
}

/// The anchors of a model's body, by name: where, how it lies, its depth.
fn anchors(m: &Model) -> BTreeMap<String, ([f64; 3], String, f64)> {
    let list = stat(m).get("anchors").and_then(|a| a.as_array()).expect("anchors");
    list.iter()
        .map(|a| {
            let at = a.get("at").and_then(|v| v.as_array()).unwrap();
            let p = [0, 1, 2].map(|k| at[k].as_f64().unwrap());
            (a.get("name").and_then(|n| n.as_str()).unwrap().to_string(), (p, a.get("on").and_then(|o| o.as_str()).unwrap().to_string(), a.get("depth_m").and_then(|d| d.as_f64()).unwrap()))
        })
        .collect()
}

/// The distance from `p` to the nearest triangle of the mesh.
fn to_mesh(l: &MeshData, p: [f64; 3]) -> f64 {
    let v = |i: u32| l.positions[i as usize].map(f64::from);
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let mut best = f64::INFINITY;
    for t in l.indices.chunks(3) {
        let (a, b, c) = (v(t[0]), v(t[1]), v(t[2]));
        // The nearest point of the triangle, by its region (Ericson's closest point).
        let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
        let (d1, d2) = (dot(ab, ap), dot(ac, ap));
        let q = if d1 <= 0.0 && d2 <= 0.0 {
            a
        } else {
            let bp = sub(p, b);
            let (d3, d4) = (dot(ab, bp), dot(ac, bp));
            let cp = sub(p, c);
            let (d5, d6) = (dot(ab, cp), dot(ac, cp));
            let (vc, vb, va) = (d1 * d4 - d3 * d2, d5 * d2 - d1 * d6, d3 * d6 - d5 * d4);
            let lerp = |s: [f64; 3], e: [f64; 3], t: f64| [s[0] + t * e[0], s[1] + t * e[1], s[2] + t * e[2]];
            if d3 >= 0.0 && d4 <= d3 {
                b
            } else if d6 >= 0.0 && d5 <= d6 {
                c
            } else if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
                lerp(a, ab, d1 / (d1 - d3))
            } else if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
                lerp(a, ac, d2 / (d2 - d6))
            } else if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
                lerp(b, sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6)))
            } else {
                let den = 1.0 / (va + vb + vc);
                let (s, w) = (vb * den, vc * den);
                [a[0] + ab[0] * s + ac[0] * w, a[1] + ab[1] * s + ac[1] * w, a[2] + ab[2] * s + ac[2] * w]
            }
        };
        let d = sub(p, q);
        best = best.min(dot(d, d).sqrt());
    }
    best
}

/// A body's places come out of its build as anchors: each bone's two ends and each joint's
/// middle inside it (at the depth the field says), each place it names on the surface within
/// a third of a cell of the mesh, or under it at its depth to within a third of a cell; and
/// they move with the recipe's numbers (a longer hand moves its tail and its tip by exactly
/// that, and the shoulder not at all).
#[test]
fn a_bodys_places_are_kept_as_anchors_through_its_meshing() {
    let m = make(&arm("", "[0.72, 1.4, 0]")).unwrap();
    let a = anchors(&m);
    let cell = stat(&m).get("cell_m").and_then(|c| c.as_f64()).unwrap();
    let l = &m.lods[0];
    for name in ["shoulder.head", "shoulder.tail", "forearm.tail", "hand.head", "shoulder~forearm", "forearm~hand"] {
        let (_, on, depth) = &a[name];
        assert!(on == "inside" && *depth > 0.0, "{name}: {on} at {depth}");
    }
    assert_eq!(a.len(), 2 * 3 + 2 + 3, "{:?}", a.keys().collect::<Vec<_>>());
    for (name, (at, on, depth)) in &a {
        let d = to_mesh(l, *at);
        match on.as_str() {
            "surface" => assert!(d <= cell / 3.0, "{name}: {d} from the mesh (cell {cell})"),
            "inside" if name.contains("palm") => assert!((d - depth).abs() <= cell / 3.0, "{name}: {d} from the mesh, {depth} deep"),
            "inside" => assert!(*depth <= d + cell / 3.0, "{name}: said {depth} deep, {d} from the mesh"),
            _ => panic!("{name}: {on}"),
        }
    }
    // A hand 0.02 m longer: its tail anchor moves by exactly that along it; the shoulder's do
    // not move.
    let longer = anchors(&make(&arm("", "[0.74, 1.4, 0]")).unwrap());
    let moved = |n: &str| [0, 1, 2].map(|k| longer[n].0[k] - a[n].0[k]);
    let tail = moved("hand.tail");
    assert!((tail[0] - 0.02).abs() < 1e-5 && tail[1].abs() < 1e-5 && tail[2].abs() < 1e-5, "{tail:?}");
    assert_eq!(moved("shoulder.head"), [0.0; 3]);
    let tip = moved("hand.tip");
    assert!((tip[0] - 0.02).abs() < 0.002, "the tip moves with the hand's end: {tip:?}");
}
