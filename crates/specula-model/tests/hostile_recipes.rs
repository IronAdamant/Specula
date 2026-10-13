//! Recipes are agent-written input: a crafted recipe must get a structured error (or a correct
//! model) in bounded time and memory, never abort the process. Each case here is a regression
//! from the lane review's probe harness.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use specula_model::{build, BuildParams, ErrorCode, Model, RecipeError, Style};

const STYLE: &str = r##"{"palette":{"a":"#ffffff"},"materials":{"m":{"color":"#808080"}}}"##;

/// Generous enough for an unoptimised debug build on a slow CI machine; the failures this
/// guards against took minutes or aborted.
const TIME: Duration = Duration::from_secs(20);

fn run(text: &str, lib: Option<&BTreeMap<String, String>>) -> Result<Model, RecipeError> {
    let style = Style::parse(STYLE).unwrap();
    let t = Instant::now();
    let r = build(text, &BuildParams { library: lib, ..BuildParams::new(&style) });
    assert!(t.elapsed() < TIME, "took {:?}", t.elapsed());
    r
}

fn part(k: usize, shape: &str, ops: &str) -> String {
    format!(r#"{{"name":"p{k}","material":"m","shape":{shape},"ops":[{ops}]}}"#)
}

fn one(shape: &str, ops: &str) -> String {
    format!(r#"{{"name":"x","parts":[{}]}}"#, part(0, shape, ops))
}

fn code(r: Result<Model, RecipeError>) -> ErrorCode {
    match r {
        Ok(m) => panic!("expected an error, built {:?}", m.lods.iter().map(|l| l.triangle_count()).collect::<Vec<_>>()),
        Err(e) => e.code,
    }
}

#[test]
fn chain() {
    // A 40 KB flat operator chain: stack overflow in eval/Drop before the fix.
    let e = format!("1{}", "+1".repeat(20_000));
    let text = format!(r#"{{"name":"x","parts":[{{"name":"a","material":"m","shape":{{"op":"box","size":["{e}",1,1]}}}}]}}"#);
    assert_eq!(code(run(&text, None)), ErrorCode::Expression);
}

#[test]
fn hull() {
    // Sphere 5 + subdivide 1 is over the part limit now; sphere 5 alone (20,480 triangles)
    // must hull quickly (it aborted with a 13 GB allocation).
    let big = format!(r#"{{"name":"x","lods":[],"parts":[{}]}}"#, part(0, r#"{"op":"sphere","radius":1,"subdivisions":5}"#, r#"{"op":"subdivide","levels":1}"#));
    assert_eq!(code(run(&big, None)), ErrorCode::Limit);
    let five = format!(r#"{{"name":"x","lods":[],"parts":[{}]}}"#, part(0, r#"{"op":"sphere","radius":1,"subdivisions":5}"#, ""));
    let m = run(&five, None).unwrap();
    assert!(matches!(m.collider, specula_model::Collider::ConvexHull { .. }));
}

#[test]
fn parts() {
    let p: Vec<String> = (0..300).map(|k| part(k, r#"{"op":"sphere","radius":1,"subdivisions":5}"#, r#"{"op":"subdivide","levels":1}"#)).collect();
    let text = format!(r#"{{"name":"x","collider":"none","lods":[],"parts":[{}]}}"#, p.join(","));
    assert_eq!(code(run(&text, None)), ErrorCode::Limit);
    // Many parts each within the part limit: the running model total stops it.
    let p: Vec<String> = (0..100).map(|k| part(k, r#"{"op":"sphere","radius":1,"subdivisions":5}"#, "")).collect();
    let text = format!(r#"{{"name":"x","collider":"none","lods":[],"parts":[{}]}}"#, p.join(","));
    let e = run(&text, None).unwrap_err();
    assert_eq!(e.code, ErrorCode::Limit);
    assert!(e.message.contains("model"), "{e}");
}

#[test]
fn include_fan_out() {
    for width in [30, 2] {
        let mut lib = BTreeMap::new();
        let names = ["a", "b", "c", "d", "e"];
        for (i, nm) in names.iter().enumerate() {
            let text = if i == 4 {
                format!(r#"{{"name":"e","collider":"none","parts":[{}]}}"#, part(0, r#"{"op":"box","size":[1,1,1]}"#, ""))
            } else {
                let ps: Vec<String> = (0..width).map(|k| format!(r#"{{"name":"i{k}","include":"{}"}}"#, names[i + 1])).collect();
                format!(r#"{{"name":"{nm}","collider":"none","lods":[],"parts":[{}]}}"#, ps.join(","))
            };
            lib.insert(nm.to_string(), text);
        }
        let r = run(&lib["a"].clone(), Some(&lib));
        if width == 30 {
            assert_eq!(code(r), ErrorCode::Limit);
        } else {
            // 2 + 4 + 8 + 16 = 30 expansions: allowed.
            assert_eq!(r.unwrap().lods[0].triangle_count(), 16 * 12);
        }
    }
}

#[test]
fn sweep_and_lathe_sizes() {
    let path: Vec<String> = (0..500).map(|i| format!("[0,{},{}]", i, (i % 2) as f32 * 0.5)).collect();
    let shape = format!(r#"{{"op":"sweep","profile":{{"sides":256,"radius":0.2,"resample":4}},"path":[{}],"smooth":16}}"#, path.join(","));
    assert_eq!(code(run(&one(&shape, ""), None)), ErrorCode::Limit);
    let prof: Vec<String> = (0..600).map(|i| format!("[1,{i}]")).collect();
    let shape = format!(r#"{{"op":"lathe","profile":[{}],"segments":256}}"#, prof.join(","));
    assert_eq!(code(run(&one(&shape, ""), None)), ErrorCode::Limit);
    let prof: Vec<String> = (0..300).map(|i| format!("[1,{}]", i as f32 * 0.01)).collect();
    let shape = format!(r#"{{"op":"lathe","profile":[{}],"segments":256}}"#, prof.join(","));
    assert_eq!(code(run(&one(&shape, ""), None)), ErrorCode::Limit);
}

#[test]
fn outline_sizes_and_triangulation_time() {
    // 1000 points resampled 64x, and a corner bevel doubling past the cap: errors before work.
    let pts: Vec<String> = (0..1000)
        .map(|i| {
            let a = i as f64 / 1000.0 * std::f64::consts::TAU;
            format!("[{:.6},{:.6}]", a.cos(), a.sin())
        })
        .collect();
    let shape = format!(r#"{{"op":"extrude","outline":{{"points":[{}],"resample":64}},"height":1}}"#, pts.join(","));
    assert_eq!(code(run(&one(&shape, ""), None)), ErrorCode::Limit);
    let shape = format!(r#"{{"op":"extrude","outline":{{"points":[{}]}},"height":1,"corner_bevel":0.0001}}"#, pts.join(","));
    assert_eq!(code(run(&one(&shape, ""), None)), ErrorCode::Limit);
    // The 150-byte recipe that took 12 s: 1024-point outline at the cap, now fast.
    let shape = r#"{"op":"extrude","outline":{"sides":256,"radius":1,"resample":4},"height":1}"#;
    let t = Instant::now();
    let m = run(&one(shape, ""), None).unwrap();
    assert!(m.lods[0].is_watertight());
    assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
}

#[test]
fn thin_boxes_keep_their_lods_honest() {
    for s in ["[2,0.1,0.1]", "[2,0.9,0.6]", "[1,1,1]", "[10,0.3,0.3]", "[6,0.2,0.2]"] {
        let text = format!(r#"{{"name":"x","collider":"none","lods":[0.5,0.25],"parts":[{}]}}"#, part(0, &format!(r#"{{"op":"box","size":{s},"bevel":0.0}}"#), ""));
        let m = run(&text, None).unwrap();
        // A 12-triangle box can't be simplified honestly: no empty LODs, just notes.
        assert!(m.lods.iter().all(|l| l.triangle_count() > 0), "{s}");
        assert_eq!(m.lods.len(), 1, "{s}: {:?}", m.notes);
        assert_eq!(m.notes.len(), 2);
    }
}

#[test]
fn hostile_numbers() {
    let cases = [
        (r#"{"op":"box","size":[1e39,1,1]}"#, ErrorCode::OutOfRange),
        (r#"{"op":"extrude","outline":{"rect":[1e300,1]},"height":1}"#, ErrorCode::OutOfRange),
        (r#"{"op":"box","size":["0/0",1,1]}"#, ErrorCode::OutOfRange),
        (r#"{"op":"box","size":["5 % 0",1,1]}"#, ErrorCode::OutOfRange),
        (r#"{"op":"box","size":["1e400",1,1]}"#, ErrorCode::OutOfRange),
        (r#"{"op":"extrude","outline":[[0,0],[1,0],[1,1],[0.5,0],[0,1]],"height":1}"#, ErrorCode::Geometry),
        (r#"{"op":"extrude","outline":[[0,0],[1,0],[1,1],[0,0],[0,1]],"height":1}"#, ErrorCode::Geometry),
    ];
    for (shape, want) in cases {
        assert_eq!(code(run(&one(shape, ""), None)), want, "{shape}");
    }
    // sin of a huge angle is finite: builds.
    run(&one(r#"{"op":"box","size":["1.5+sin(1e300)",1,1]}"#, ""), None).unwrap();
    // A collinear vertex is a valid outline and stays watertight.
    let m = run(&one(r#"{"op":"extrude","outline":[[0,0],[1,0],[2,0],[2,1],[0,1]],"height":1}"#, ""), None).unwrap();
    assert!(m.lods[0].is_watertight());
    let ops = [
        (r#"{"op":"lean","x":1e30}"#, ErrorCode::OutOfRange),
        (r#"{"op":"twist","deg_per_m":1e300}"#, ErrorCode::OutOfRange),
        (r#"{"op":"radial","count":3,"angle":1e300}"#, ErrorCode::OutOfRange),
        (r#"{"op":"move","by":[1e9,0,0]}"#, ErrorCode::OutOfRange),
        (r#"{"op":"scale","by":1000},{"op":"scale","by":1000}"#, ErrorCode::Limit),
    ];
    for (op, want) in ops {
        assert_eq!(code(run(&one(r#"{"op":"box","size":[1,2,1]}"#, op), None)), want, "{op}");
    }
    // A vanishingly small bend is a no-op, not a precision blow-up.
    let m = run(&one(r#"{"op":"box","size":[1,2,1]}"#, r#"{"op":"bend","deg_per_m":1e-30}"#), None).unwrap();
    let b = m.bounds;
    assert!((b.max[1] - 2.0).abs() < 1e-5 && (b.max[0] - 0.5).abs() < 1e-5, "{b:?}");
}

#[test]
fn closed_helix_sweep_builds() {
    let m = run(&one(r#"{"op":"sweep","profile":{"sides":6,"radius":0.1},"path":[[1,0,0],[0,0.5,1],[-1,1,0],[0,1.5,-1]],"closed":true}"#, ""), None).unwrap();
    m.lods[0].validate().unwrap();
}

#[test]
fn negative_zero_apex_welds() {
    for op in [r#"{"op":"taper","to":0}"#, r#"{"op":"taper","to":0.000001}"#] {
        let text = format!(r#"{{"name":"x","lods":[],"collider":"none","parts":[{}]}}"#, part(0, r#"{"op":"box","size":[1,2,1]}"#, op));
        let m = run(&text, None).unwrap();
        assert_eq!(m.lods[0].open_edge_count(), 0, "{op}");
        assert!(m.lods[0].positions.iter().flatten().all(|v| v.to_bits() != (-0.0f32).to_bits()), "no -0.0 in output");
    }
}

#[test]
fn mirror_and_orientation() {
    // A half touching the plane, mirrored: no internal back-to-back faces, so closed.
    let m = run(&one(r#"{"op":"box","size":[1,2,1]}"#, r#"{"op":"move","by":[0.5,0,0]},{"op":"mirror","axis":"x"}"#), None).unwrap();
    assert!(m.lods[0].is_watertight());
    assert!((m.lods[0].signed_volume() - 4.0).abs() < 1e-4);
    // Open lathes and open sweeps face outward whichever way their profile runs.
    for shape in [
        r#"{"op":"lathe","profile":[[1,0],[0.5,1]],"caps":false}"#,
        r#"{"op":"lathe","profile":[[0.5,1],[1,0]],"caps":false}"#,
        r#"{"op":"sweep","profile":{"sides":6,"radius":0.1},"path":[[0,0,0],[0,1,0]],"caps":false}"#,
    ] {
        let l = &run(&one(shape, ""), None).unwrap().lods[0];
        let mut out = 0.0f32;
        for t in l.indices.chunks_exact(3) {
            let (a, b, c) = (l.positions[t[0] as usize], l.positions[t[1] as usize], l.positions[t[2] as usize]);
            let n = cross(sub(b, a), sub(c, a));
            let mid = [(a[0] + b[0] + c[0]) / 3.0, 0.0, (a[2] + b[2] + c[2]) / 3.0];
            out += n[0] * mid[0] + n[2] * mid[2];
        }
        assert!(out > 0.0, "{shape} faces inward");
    }
    // Closed shapes built in reverse still face outward.
    for shape in [r#"{"op":"lathe","profile":[[0,1],[1,0.5],[0,0]]}"#, r#"{"op":"loft","outline":{"rect":[1,1]},"stations":[[1,1],[1,0]]}"#] {
        assert!(run(&one(shape, ""), None).unwrap().lods[0].signed_volume() > 0.0, "{shape}");
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[test]
fn strictness() {
    let dup_top = r#"{"name":"x","parts":[{"name":"a","material":"m","shape":{"op":"box","size":[1,1,1]}}],"parts":[{"name":"zz","material":"nope"}]}"#;
    let e = run(dup_top, None).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::WrongType, "parts"));
    let dup_field = r#"{"name":"x","parts":[{"name":"a","material":"m","shape":{"op":"box","size":[1,1,1]},"ops":[{"op":"move","by":[1,2,3],"by":[9,9,9]}]}]}"#;
    assert_eq!(run(dup_field, None).unwrap_err().path, "parts[0].ops[0].by");
    let int_default = r#"{"name":"x","params":{"t":{"default":1.5,"integer":true}},"parts":[{"name":"a","material":"m","shape":{"op":"box","size":["t",1,1]}}]}"#;
    assert_eq!(run(int_default, None).unwrap_err().path, "params.t.default");
    let extra_row = r#"{"name":"x","parts":[{"name":"a","material":"m","shape":{"op":"lathe","profile":[[1,0,99,99],[1,1]]}}]}"#;
    assert_eq!(run(extra_row, None).unwrap_err().path, "parts[0].shape.profile[0]");
    let smooth = r#"{"name":"x","smooth_angle":1e300,"parts":[{"name":"a","material":"m","shape":{"op":"box","size":[1,1,1]}}]}"#;
    assert_eq!(run(smooth, None).unwrap_err().code, ErrorCode::OutOfRange);
    let bad_style = r#"{"materials":{"m":{"roughness":7}}}"#;
    assert_eq!(Style::parse(bad_style).unwrap_err().code, ErrorCode::OutOfRange);
    assert_eq!(Style::parse(r#"{"materials":{"m":{"opacity":-1}}}"#).unwrap_err().path, "materials.m.opacity");
    assert_eq!(Style::parse(r##"{"palette":{"a":"#000000","a":"#ffffff"}}"##).unwrap_err().path, "palette.a");
    // Too many ops in a part.
    let ops: Vec<&str> = std::iter::repeat_n(r#"{"op":"move","by":[0,0,0]}"#, 65).collect();
    assert_eq!(code(run(&one(r#"{"op":"box","size":[1,1,1]}"#, &ops.join(",")), None)), ErrorCode::Limit);
}
