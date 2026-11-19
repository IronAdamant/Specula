//! **Faceting held against a truth of its own** (docs/design/FORM.md, "Faceting, from what the
//! recipe names round"; the second review's probe, kept). Random chains of operations
//! (a turn, an uneven and an even scale, a mirror, a lean, a move) through a part's ops, an
//! include's ops and the finish, on every round kind: the record (`ModelExtras::round`) against
//! the finished mesh, measured here.
//!
//! The truth is this file's own: it carries every map itself, from the operations' own
//! definitions, and measures the mesh itself; it calls nothing of what it checks (only
//! `build`, for the mesh and the record). For each face of the round part:
//! - **stand-in:** how far the true surface stands out of the face's plane, from the parallel
//!   plane that touches it (for a ring: the surface of revolution between the face's two rims,
//!   carried through the map; for a ball: the mapped sphere);
//! - **turn:** the largest angle between neighbouring faces (for a ring: faces that meet along an
//!   edge of the profile, that is, an edge both of whose ends lie at one angle round the axis);
//! - **fit:** every corner of every face lies on the true surface carried through the map.
//!
//! Copies whose maps are drawn from a seed (an array's jitter, a scatter's placements) are not
//! carried: each copy's map is fitted to the mesh, from the shape built alone, and must fit
//! every corner of it.

use std::collections::BTreeMap;

use specula_model::dmath;
use specula_model::{build, BuildParams, Model, Round, Style};

type M = [[f64; 3]; 3]; // rows
type V = [f64; 3];

fn style() -> Style {
    Style::parse(r##"{ "style": "t", "palette": { "pale": "#b8fff6" }, "materials": { "stone": { "color": "#8c96a3", "roughness": 0.8 } } }"##).unwrap()
}

fn make(text: &str, lib: &BTreeMap<String, String>) -> Model {
    let s = style();
    build(text, &BuildParams { library: Some(lib), ..BuildParams::new(&s) }).unwrap_or_else(|e| panic!("{text}: {e}"))
}

fn mm(a: &M, b: &M) -> M {
    let mut o = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            o[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    o
}
fn tr(a: &M) -> M {
    [[a[0][0], a[1][0], a[2][0]], [a[0][1], a[1][1], a[2][1]], [a[0][2], a[1][2], a[2][2]]]
}
fn mv(a: &M, v: V) -> V {
    [0, 1, 2].map(|i| a[i][0] * v[0] + a[i][1] * v[1] + a[i][2] * v[2])
}
fn det(a: &M) -> f64 {
    a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0]) + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0])
}
fn inv(a: &M) -> M {
    let d = det(a);
    let c = |r0: usize, c0: usize, r1: usize, c1: usize| a[r0][c0] * a[r1][c1] - a[r0][c1] * a[r1][c0];
    [
        [c(1, 1, 2, 2) / d, -c(0, 1, 2, 2) / d, c(0, 1, 1, 2) / d],
        [-c(1, 0, 2, 2) / d, c(0, 0, 2, 2) / d, -c(0, 0, 1, 2) / d],
        [c(1, 0, 2, 1) / d, -c(0, 0, 2, 1) / d, c(0, 0, 1, 1) / d],
    ]
}
const I3: M = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
fn rx(d: f64) -> M {
    let (s, c) = dmath::sin_cos_f64(d.to_radians());
    [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]
}
fn ry(d: f64) -> M {
    let (s, c) = dmath::sin_cos_f64(d.to_radians());
    [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
}
fn rz(d: f64) -> M {
    let (s, c) = dmath::sin_cos_f64(d.to_radians());
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}
/// A `rotate` op's turn: about X, then Y, then Z.
fn euler(d: [f64; 3]) -> M {
    mm(&rz(d[2]), &mm(&ry(d[1]), &rx(d[0])))
}
fn sc(v: V) -> M {
    [[v[0], 0.0, 0.0], [0.0, v[1], 0.0], [0.0, 0.0, v[2]]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn len(a: V) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: V) -> V {
    let l = len(a);
    a.map(|x| x / l)
}
fn angle(a: V, b: V) -> f64 {
    dmath::atan2_f64(len(cross(a, b)), dot(a, b)).to_degrees()
}

struct Lcg(u64);
impl Lcg {
    fn f(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn r(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.f()
    }
    fn pick(&mut self, n: usize) -> usize {
        ((self.f() * n as f64) as usize).min(n - 1)
    }
}

fn milli(x: f64) -> f64 {
    dmath::round_f64(x * 1000.0) / 1000.0
}

/// One random op: its JSON and its linear map, from the op's own definition.
fn op(g: &mut Lcg) -> (String, M) {
    let r3 = |g: &mut Lcg, a: f64, b: f64| [g.r(a, b), g.r(a, b), g.r(a, b)].map(milli);
    match g.pick(6) {
        0 => {
            let d = r3(g, -180.0, 180.0);
            (format!(r#"{{"op":"rotate","deg":[{},{},{}]}}"#, d[0], d[1], d[2]), euler(d))
        }
        1 => {
            let s = r3(g, 0.3, 3.0);
            (format!(r#"{{"op":"scale","by":[{},{},{}]}}"#, s[0], s[1], s[2]), sc(s))
        }
        2 => {
            let s = milli(g.r(0.1, 5.0));
            (format!(r#"{{"op":"scale","by":{s}}}"#), sc([s; 3]))
        }
        3 => {
            let a = g.pick(3);
            let mut v = [1.0; 3];
            v[a] = -1.0;
            (format!(r#"{{"op":"mirror","axis":"{}","keep":false}}"#, ["x", "y", "z"][a]), sc(v))
        }
        4 => {
            // (A lean moves x and z by y times its two numbers.)
            let (x, z) = (milli(g.r(-1.0, 1.0)), milli(g.r(-1.0, 1.0)));
            (format!(r#"{{"op":"lean","x":{x},"z":{z}}}"#), [[1.0, x, 0.0], [0.0, 1.0, 0.0], [0.0, z, 1.0]])
        }
        _ => {
            let d = r3(g, -3.0, 3.0);
            (format!(r#"{{"op":"move","by":[{},{},{}]}}"#, d[0], d[1], d[2]), I3)
        }
    }
}

fn key(p: V) -> [i64; 3] {
    p.map(|x| dmath::round_f64(x * 2.0e4) as i64)
}

/// The triangles of a mesh's finest level, as corners.
fn tris(m: &Model) -> Vec<[V; 3]> {
    let l = &m.lods[0];
    l.indices.chunks(3).map(|t| [0, 1, 2].map(|k| l.positions[t[k] as usize].map(f64::from))).collect()
}

/// What a shape is round: a ring round its own Y axis (its surface between profile rows), or a
/// ball of radius `r` about its centre.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Ring,
    Ball(f64),
}

/// The truth for one piece of mesh `ts`, whose shape frame is mapped to the model by `x ↦ m·x +
/// t`: the furthest any face stands in from the true surface, the largest turn between
/// neighbouring faces of what is round, and the worst misfit of a corner (relative to `scale`).
/// `rows` lists the shape's profile rows `(ρ, y)` (a ring's), each corner must be on one.
struct Truth {
    sag: f64,
    turn: f64,
    fit: f64,
}

fn truth(ts: &[[V; 3]], m: &M, t: V, kind: Kind, rows: &[[f64; 2]]) -> Truth {
    let mi = inv(m);
    let mt = tr(m);
    // (A ball rests on y = 0 in its own frame: its centre is its radius up.)
    let t = match kind {
        Kind::Ball(r) => {
            let up = mv(m, [0.0, r, 0.0]);
            [t[0] + up[0], t[1] + up[1], t[2] + up[2]]
        }
        Kind::Ring => t,
    };
    let local = |p: V| mv(&mi, sub(p, t));
    let scale = rows.iter().map(|r| r[0].abs().max(r[1].abs())).fold(match kind { Kind::Ball(r) => r, Kind::Ring => 0.0 }, f64::max);
    let (mut sag, mut fit) = (0.0f64, 0.0f64);
    // (Each face's outward normal in the model, and its edges along the profile.)
    let mut faces: Vec<(usize, V)> = Vec::new();
    let mut profile_edges: BTreeMap<([i64; 3], [i64; 3]), Vec<V>> = BTreeMap::new();
    let mut ball_edges: BTreeMap<([i64; 3], [i64; 3]), Vec<V>> = BTreeMap::new();
    for (i, tri) in ts.iter().enumerate() {
        let n = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]));
        if len(n) < 1e-14 {
            continue;
        }
        let l = tri.map(local);
        match kind {
            Kind::Ball(r) => {
                for q in &l {
                    fit = fit.max((len(*q) - r).abs() / r);
                }
                let mut nm = unit(n);
                if dot(nm, sub(tri[0], t)) < 0.0 {
                    nm = nm.map(|x| -x);
                }
                // (Support of the mapped sphere along the face's normal, less the face's
                // offset: both from the centre.)
                let w = mv(&mt, nm);
                sag = sag.max(r * len(w) - dot(nm, sub(tri[0], t)));
                faces.push((i, nm));
                for k in 0..3 {
                    let (a, b) = (key(tri[k]), key(tri[(k + 1) % 3]));
                    ball_edges.entry((a.min(b), a.max(b))).or_default().push(nm);
                }
            }
            Kind::Ring => {
                let rho = l.map(|q| dmath::hypot_f64(q[0], q[2]));
                for (q, r) in l.iter().zip(rho) {
                    let miss = rows.iter().map(|w| dmath::hypot_f64(w[0] - r, w[1] - q[1])).fold(f64::INFINITY, f64::min);
                    fit = fit.max(miss / scale);
                }
                // The local normal: flat (square to the axis) faces are caps, not round.
                let nl = cross(sub(l[1], l[0]), sub(l[2], l[0]));
                let radial = dmath::hypot_f64(nl[0], nl[2]) / len(nl);
                if radial < 1e-4 {
                    continue;
                }
                // (Outward from the axis at the face's middle angle.)
                let mid = [(l[0][0] + l[1][0] + l[2][0]) / 3.0, (l[0][2] + l[1][2] + l[2][2]) / 3.0];
                let mut nm = unit(n);
                let w0 = mv(&mt, nm);
                if w0[0] * mid[0] + w0[2] * mid[1] < 0.0 {
                    nm = nm.map(|x| -x);
                }
                let w = mv(&mt, nm);
                // The face's rims: its corners' rows (two; a triangle at the axis has a point).
                let off = dot(w, l[0]);
                let mut top = f64::NEG_INFINITY;
                for (q, r) in l.iter().zip(rho) {
                    top = top.max(r * dmath::hypot_f64(w[0], w[2]) + q[1] * w[1]);
                }
                sag = sag.max(top - off);
                faces.push((i, nm));
                // Edges along the profile: both ends at one angle round the axis (or one on it).
                for k in 0..3 {
                    let (a, b) = (l[k], l[(k + 1) % 3]);
                    let (ra, rb) = (rho[k], rho[(k + 1) % 3]);
                    let on = |r: f64| r <= 1e-6 * scale;
                    let along = on(ra) || on(rb) || ((a[0] * b[2] - a[2] * b[0]).abs() <= 1e-3 * ra * rb && a[0] * b[0] + a[2] * b[2] > 0.0);
                    if along && !(on(ra) && on(rb)) {
                        let (ka, kb) = (key(tri[k]), key(tri[(k + 1) % 3]));
                        profile_edges.entry((ka.min(kb), ka.max(kb))).or_default().push(nm);
                    }
                }
            }
        }
    }
    let edges = if matches!(kind, Kind::Ball(_)) { &ball_edges } else { &profile_edges };
    let mut turn = 0.0f64;
    for ns in edges.values() {
        for a in 0..ns.len() {
            for b in a + 1..ns.len() {
                turn = turn.max(angle(ns[a], ns[b]));
            }
        }
    }
    let _ = faces;
    Truth { sag, turn, fit }
}

/// A shape drawn for a case: its JSON, its kind, its profile rows (a ring's), the count that
/// makes its turn round the axis (the record held).
struct Drawn {
    json: String,
    kind: Kind,
    rows: Vec<[f64; 2]>,
    count: &'static str,
}

fn draw(g: &mut Lcg) -> Drawn {
    let odd = [3u32, 5, 9, 31, 101];
    let n = odd[g.pick(5)];
    match g.pick(8) {
        0 => Drawn { json: format!(r#"{{"op":"cylinder","radius":0.5,"height":1.3,"sides":{n}}}"#), kind: Kind::Ring, rows: vec![[0.0, 0.0], [0.5, 0.0], [0.5, 1.3], [0.0, 1.3]], count: "sides" },
        1 => Drawn { json: format!(r#"{{"op":"sphere","radius":0.7,"subdivisions":{}}}"#, g.pick(4)), kind: Kind::Ball(0.7), rows: Vec::new(), count: "subdivisions" },
        2 => {
            let n = odd[g.pick(4)];
            // (A capsule's rows: its ends' quarter circles in 8 bands and its wall.)
            let mut rows = vec![[0.0, 0.0], [0.0, 2.0]];
            for k in 0..=8 {
                let (s, c) = dmath::sin_cos_f64((90.0 * f64::from(k) / 8.0).to_radians());
                rows.push([0.4 * s, 0.4 - 0.4 * c]);
                rows.push([0.4 * c, 1.6 + 0.4 * s]);
            }
            Drawn { json: format!(r#"{{"op":"capsule","radius":0.4,"height":2.0,"segments":{n},"rings":8}}"#), kind: Kind::Ring, rows, count: "segments" }
        }
        3 => Drawn { json: format!(r#"{{"op":"lathe","segments":{n},"profile":[[0,0],[0.6,0],[0.6,1.1],[0,1.1]]}}"#), kind: Kind::Ring, rows: vec![[0.0, 0.0], [0.6, 0.0], [0.6, 1.1], [0.0, 1.1]], count: "segments" },
        4 => Drawn { json: format!(r#"{{"op":"cone","radius":0.5,"height":0.9,"sides":{n}}}"#), kind: Kind::Ring, rows: vec![[0.0, 0.0], [0.5, 0.0], [0.0, 0.9]], count: "sides" },
        5 => Drawn { json: format!(r#"{{"op":"cylinder","radius":0.3,"radius_top":0.7,"height":1.0,"sides":{n}}}"#), kind: Kind::Ring, rows: vec![[0.0, 0.0], [0.3, 0.0], [0.7, 1.0], [0.0, 1.0]], count: "sides" },
        // A lathe whose profile slopes every way: a bell.
        6 => Drawn {
            json: format!(r#"{{"op":"lathe","segments":{n},"profile":[[0,0],[0.5,0],[0.42,0.15],[0.2,0.5],[0.18,0.8],[0.06,1.0],[0,1.05]]}}"#),
            kind: Kind::Ring,
            rows: vec![[0.0, 0.0], [0.5, 0.0], [0.42, 0.15], [0.2, 0.5], [0.18, 0.8], [0.06, 1.0], [0.0, 1.05]],
            count: "segments",
        },
        // A torus round its axis (its tube's rows are its profile).
        _ => {
            let sides = [3u32, 4, 6, 9][g.pick(4)];
            let rows = (0..sides)
                .map(|k| {
                    let (s, c) = dmath::sin_cos_f64(std::f64::consts::TAU * f64::from(k) / f64::from(sides));
                    [0.6 + 0.2 * c, 0.2 + 0.2 * s]
                })
                .collect();
            Drawn { json: format!(r#"{{"op":"torus","radius":0.6,"tube":0.2,"segments":{n},"sides":{sides}}}"#), kind: Kind::Ring, rows, count: "segments" }
        }
    }
}

/// The centre of the shape's frame in the model: the mean of its distinct corners (a ring's
/// and a ball's corners are spread evenly round their axis or centre), less the shape frame's
/// own centre carried through the map.
fn centre(ts: &[[V; 3]], m: &M, kind: Kind, rows: &[[f64; 2]], json: &str) -> V {
    let mut uniq: BTreeMap<[i64; 3], V> = BTreeMap::new();
    for t in ts {
        for c in t {
            uniq.insert(key(*c), *c);
        }
    }
    let n = uniq.len() as f64;
    let c = uniq.values().fold([0.0; 3], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n, a[2] + p[2] / n]);
    // (The mean of the corners in the shape's own frame: on its axis, at a height read from
    // the shape built alone.)
    let alone = make(&format!(r#"{{ "name": "a", "parts": [ {{ "name": "p", "material": "stone", "shape": {json} }} ] }}"#), &BTreeMap::new());
    let mut u0: BTreeMap<[i64; 3], V> = BTreeMap::new();
    for t in tris(&alone) {
        for q in t {
            u0.insert(key(q), q);
        }
    }
    let n0 = u0.len() as f64;
    let c0 = u0.values().fold([0.0; 3], |a, p| [a[0] + p[0] / n0, a[1] + p[1] / n0, a[2] + p[2] / n0]);
    let _ = (kind, rows);
    sub(c, mv(m, c0))
}

fn rec<'a>(m: &'a Model, path: &str, count: &str) -> Option<&'a Round> {
    m.extras.round.iter().find(|r| r.path == path && r.count == count)
}

/// How the record stood against the truth, one case.
struct Case {
    what: String,
    sag_ratio: f64,
    turn_off: f64,
}

/// Runs the cases `0..n` of the drawn chains (every `every`th), holding each record measured
/// against the truth; answers what each kind's worst was.
fn chains(n: usize, every: usize) -> Vec<Case> {
    let mut g = Lcg(0x5eed_0010);
    let mut out = Vec::new();
    for case in 0..n {
        let sh = draw(&mut g);
        let chain = |g: &mut Lcg, k: usize| {
            let mut js = Vec::new();
            let mut m = I3;
            for _ in 0..k {
                let (j, l) = op(g);
                js.push(j);
                m = mm(&l, &m);
            }
            (js.join(","), m)
        };
        let n1 = 1 + g.pick(3);
        let (pj, pm) = chain(&mut g, n1);
        let n2 = g.pick(3);
        let (ij, im) = chain(&mut g, n2);
        let n3 = g.pick(2);
        let (fj, fm) = chain(&mut g, n3);
        if case % every != 0 {
            continue;
        }
        let m = mm(&fm, &mm(&im, &pm));
        let mut lib = BTreeMap::new();
        lib.insert("inc".to_string(), format!(r#"{{ "name": "inc", "parts": [ {{ "name": "q", "material": "stone", "shape": {}, "ops": [{pj}] }} ] }}"#, sh.json));
        let text = format!(r#"{{ "name": "t", "parts": [ {{ "name": "b", "include": "inc", "ops": [{ij}] }} ], "finish": [{fj}] }}"#);
        let model = make(&text, &lib);
        let ts = tris(&model);
        let t = centre(&ts, &m, sh.kind, &sh.rows, &sh.json);
        let tr = truth(&ts, &m, t, sh.kind, &sh.rows);
        let at = format!("case {case}: {} | part [{pj}] include [{ij}] finish [{fj}]", sh.json);
        assert!(tr.fit < 1e-4, "{at}: the truth's own map does not fit the mesh ({:.2e})", tr.fit);
        let r = rec(&model, "parts[0].include(inc).parts[0].shape", sh.count).unwrap_or_else(|| panic!("{at}: no record in {:?}", model.extras.round));
        assert!(r.measured(), "{at}: not measured: {:?}", r.unmeasured);
        let (rs, rt) = (f64::from(r.sag_m()), f64::from(r.turn_deg));
        // (The record is f32, the mesh f32: a part in a few thousand, and a few hundred
        // thousandths of a degree.)
        assert!((rs - tr.sag).abs() <= 2e-3 * tr.sag + 1e-7 && (rt - tr.turn).abs() <= 0.02, "{at}: record stands in {rs:.6} and turns {rt:.3}; the mesh {:.6} and {:.3}", tr.sag, tr.turn);
        out.push(Case { what: sh.json.split('"').nth(3).unwrap_or("").to_string(), sag_ratio: rs / tr.sag, turn_off: rt - tr.turn });
    }
    out
}

/// A fixed share of the chains (one in four), inside the crate's time.
#[test]
fn the_record_holds_against_the_mesh_on_drawn_chains() {
    let cases = chains(400, 4);
    assert!(cases.len() == 100, "{}", cases.len());
    // (Every kind was drawn.)
    for what in ["cylinder", "sphere", "capsule", "lathe", "cone", "torus"] {
        assert!(cases.iter().any(|c| c.what == what), "no {what} drawn");
    }
}

/// All 400 chains of the review, and as many again on a second seed's shapes.
#[test]
#[ignore = "the full run: about four times the default's time (cargo test --release -p specula-model --test facet_truth -- --ignored)"]
fn the_record_holds_against_the_mesh_on_every_chain() {
    let cases = chains(400, 1);
    let worst = |what: &str| cases.iter().filter(|c| c.what == what).fold((1.0f64, 0.0f64), |(r, t), c| (if (c.sag_ratio - 1.0).abs() > (r - 1.0).abs() { c.sag_ratio } else { r }, if c.turn_off.abs() > t.abs() { c.turn_off } else { t }));
    for what in ["cylinder", "sphere", "capsule", "lathe", "cone", "torus"] {
        let (r, t) = worst(what);
        assert!((r - 1.0).abs() < 2e-3 && t.abs() < 0.02, "{what}: {r} {t}");
    }
}

/// The review's named cases: a ring leant, and one turned by an angle that is not a quarter and
/// then scaled unevenly, with the limit of 20° they were read against.
#[test]
fn a_leant_ring_is_measured_in_its_section() {
    let lib = BTreeMap::new();
    let cyl = r#"{"op":"cylinder","radius":0.5,"height":1.3,"sides":24}"#;
    let rows = [[0.0, 0.0], [0.5, 0.0], [0.5, 1.3], [0.0, 1.3]];
    for (ops, m, turn) in [
        (r#"{"op":"lean","x":1.0}"#, [[1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], 20.75),
        (r#"{"op":"lean","x":0.4}"#, [[1.0, 0.4, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], 16.10),
        (r#"{"op":"rotate","deg":[0,0,45]},{"op":"scale","by":[1,2,1]}"#, mm(&sc([1.0, 2.0, 1.0]), &rz(45.0)), 18.72),
    ] {
        let model = make(&format!(r#"{{ "name": "t", "parts": [ {{ "name": "p", "material": "stone", "shape": {cyl}, "ops": [{ops}] }} ] }}"#), &lib);
        let ts = tris(&model);
        let tr = truth(&ts, &m, centre(&ts, &m, Kind::Ring, &rows, cyl), Kind::Ring, &rows);
        let r = rec(&model, "parts[0].shape", "sides").unwrap();
        assert!(r.measured() && (f64::from(r.turn_deg) - tr.turn).abs() < 0.02 && (tr.turn - turn).abs() < 0.01 && (f64::from(r.sag_m()) - tr.sag).abs() < 2e-3 * tr.sag, "{ops}: {r:?} against {:.3} {:.6}", tr.turn, tr.sag);
    }
}

/// Fits the affine map that takes `reference` (a shape built alone) to `copy` (the same
/// triangles, in order, a corner's order turned or reversed): answers it and the worst misfit
/// of any corner, relative to the shape's size.
fn fit(reference: &[[V; 3]], copy: &[[V; 3]]) -> (M, V, f64) {
    let size = reference.iter().flatten().map(|p| len(*p)).fold(0.0f64, f64::max);
    let orders: [[usize; 3]; 6] = [[0, 1, 2], [1, 2, 0], [2, 0, 1], [0, 2, 1], [2, 1, 0], [1, 0, 2]];
    // (Four corners not in one plane: a triangle's and a corner of another off its plane.)
    let a = reference[0];
    let na = cross(sub(a[1], a[0]), sub(a[2], a[0]));
    let (j, k) = (0..reference.len()).flat_map(|j| (0..3).map(move |k| (j, k))).max_by(|x, y| dot(sub(reference[x.0][x.1], a[0]), na).abs().total_cmp(&dot(sub(reference[y.0][y.1], a[0]), na).abs())).unwrap();
    let mut best = (I3, [0.0; 3], f64::INFINITY);
    for o in orders {
        for o2 in orders {
            let src = [a[0], a[1], a[2], reference[j][k]];
            let dst = [copy[0][o[0]], copy[0][o[1]], copy[0][o[2]], copy[j][o2[k]]];
            let s = [sub(src[1], src[0]), sub(src[2], src[0]), sub(src[3], src[0])];
            let d = [sub(dst[1], dst[0]), sub(dst[2], dst[0]), sub(dst[3], dst[0])];
            // m · S = D, with S and D holding the differences as columns.
            let sm: M = [[s[0][0], s[1][0], s[2][0]], [s[0][1], s[1][1], s[2][1]], [s[0][2], s[1][2], s[2][2]]];
            let dm: M = [[d[0][0], d[1][0], d[2][0]], [d[0][1], d[1][1], d[2][1]], [d[0][2], d[1][2], d[2][2]]];
            if det(&sm).abs() < 1e-12 {
                continue;
            }
            let m = mm(&dm, &inv(&sm));
            let t = sub(dst[0], mv(&m, src[0]));
            let mut miss = 0.0f64;
            for (r, c) in reference.iter().zip(copy) {
                for p in r {
                    let q = mv(&m, *p);
                    let q = [q[0] + t[0], q[1] + t[1], q[2] + t[2]];
                    miss = miss.max(c.iter().map(|x| len(sub(*x, q))).fold(f64::INFINITY, f64::min));
                }
                if miss > 1e-3 * size {
                    break;
                }
            }
            if miss < best.2 {
                best = (m, t, miss);
            }
        }
    }
    (best.0, best.1, best.2 / size)
}

/// Copies whose maps come from a seed: an array with a jitter of turns and scales, and a
/// scatter's placements. Each copy's map is fitted to the mesh; the record is the worst copy's.
#[test]
fn copies_drawn_from_a_seed_are_measured_as_placed() {
    let lib = BTreeMap::new();
    let cyl = r#"{"op":"cylinder","radius":0.3,"height":1,"sides":7}"#;
    let rows = [[0.0, 0.0], [0.3, 0.0], [0.3, 1.0], [0.0, 1.0]];
    let alone = tris(&make(&format!(r#"{{ "name": "a", "parts": [ {{ "name": "p", "material": "stone", "shape": {cyl} }} ] }}"#), &lib));
    let worst_of = |ts: &[[V; 3]], copies: usize| {
        assert_eq!(ts.len(), alone.len() * copies, "copies in order, whole");
        let (mut sag, mut turn) = (0.0f64, 0.0f64);
        for chunk in ts.chunks(alone.len()) {
            let (m, t, miss) = fit(&alone, chunk);
            assert!(miss < 1e-5, "a copy's map does not fit: {miss:.2e}");
            let tr = truth(chunk, &m, t, Kind::Ring, &rows);
            assert!(tr.fit < 1e-4);
            sag = sag.max(tr.sag);
            turn = turn.max(tr.turn);
        }
        (sag, turn)
    };
    // (Jittered turns about every axis, with an uneven scale after: the copies' rings lean
    // each its own way.)
    for (ops, copies) in [
        (r#"{"op":"array","count":5,"offset":[2,0,0],"jitter":{"rotate":[40,30,50],"scale":0.4}},{"op":"scale","by":[1,2.5,0.7]}"#, 5),
        (r#"{"op":"radial","count":6,"jitter":{"rotate":[30,0,30]}},{"op":"lean","x":0.6,"z":-0.3}"#, 6),
    ] {
        let model = make(&format!(r#"{{ "name": "t", "parts": [ {{ "name": "p", "material": "stone", "shape": {cyl}, "ops": [{ops}] }} ] }}"#), &lib);
        let (sag, turn) = worst_of(&tris(&model), copies);
        let r = rec(&model, "parts[0].shape", "sides").unwrap();
        assert!(r.measured() && (f64::from(r.turn_deg) - turn).abs() < 0.02 && (f64::from(r.sag_m()) - sag).abs() <= 2e-3 * sag, "{ops}: {r:?} against {turn:.3} {sag:.6}");
    }
    // A scatter: each item placed (turned about the vertical, scaled evenly), then the whole
    // leant.
    let text = format!(
        r#"{{ "name": "sc", "seed": 3, "parts": [ {{ "name": "things", "material": "stone", "generate": {{ "op": "scatter", "area": {{ "rect": [8, 8] }}, "min_distance": 1.2, "count": 9,
          "items": [ {{ "shape": {cyl} }} ] }} }} ], "finish": [{{"op":"lean","x":0.8}}] }}"#
    );
    let model = make(&text, &lib);
    let ts = tris(&model);
    let copies = ts.len() / alone.len();
    assert!(copies > 2, "{copies}");
    let (sag, turn) = worst_of(&ts, copies);
    let r = rec(&model, "parts[0].generate.items[0].shape", "sides").unwrap();
    assert!(r.measured() && (f64::from(r.turn_deg) - turn).abs() < 0.02 && (f64::from(r.sag_m()) - sag).abs() <= 2e-3 * sag, "scatter: {r:?} against {turn:.3} {sag:.6}");
}

/// **A sculpt's figure is an estimate, held within bounds**: its record is what a cell of its
/// grid turns the surface by round the primitive's radius (twice the arc sine of the cell over
/// twice the radius), and how far a chord of a cell stands in from that curve. The mesh's own
/// facets, measured against the true sphere, turn from about as much (at a resolution of 8) to
/// half as much again (at 48) and stand in from 0.94 to 1.4 times as far: the dual contouring's
/// corners are not on the surface, and its faces are not one cell wide. Held here so that a
/// change to either the mesher or the figure is seen; documented in FORM.md as a limit.
#[test]
fn a_sculpts_figure_is_held_against_its_mesh() {
    let lib = BTreeMap::new();
    for res in [8u32, 12, 16, 24, 48] {
        for r in [0.5f64, 0.2] {
            let text = format!(r#"{{ "name": "t", "parts": [ {{ "name": "p", "material": "stone", "shape": {{"op":"sdf","resolution":{res},"tree":{{"op":"sphere","radius":{r}}}}} }} ] }}"#);
            let model = make(&text, &lib);
            let ts = tris(&model);
            // (The ball's centre: the mean of its corners, which spread evenly round it.)
            let mut u: BTreeMap<[i64; 3], V> = BTreeMap::new();
            for t in &ts {
                for c in t {
                    u.insert(key(*c), *c);
                }
            }
            let n = u.len() as f64;
            let c = u.values().fold([0.0; 3], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n, a[2] + p[2] / n]);
            let tr = truth(&ts, &I3, [c[0], c[1] - r, c[2]], Kind::Ball(r), &[]);
            let rec = &model.extras.round[0];
            let (s, t) = (tr.sag / f64::from(rec.sag_m()), tr.turn / f64::from(rec.turn_deg));
            assert!(rec.measured() && tr.fit < 0.02 && (0.9..1.45).contains(&s) && (1.0..1.6).contains(&t), "resolution {res}, radius {r}: the mesh stands in {s:.2} and turns {t:.2} times the record");
        }
    }
}
