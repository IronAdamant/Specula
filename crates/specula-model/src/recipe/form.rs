//! Recipe glue for the shape library (`specula-formae`, docs/design/PROCEDURAL_MODELLING.md,
//! "Shapes by name"): a plane figure as an outline (`{"form": "heart", "args": {...}}` where an
//! outline goes), and a solid or a raised plane figure as a shape (`{"op": "form", "form":
//! "icosahedron", "args": {...}}`).
//!
//! **What is said and where.** A form's parameters are its own and are given in `args`, so that
//! none can be taken for a field of the outline or the shape around it (a figure's `scale` is
//! not the outline's `scale`). Each is a number or an expression of the recipe's parameters and
//! lets, like any number in a recipe. At parse time the word is looked up in the library's table
//! (a plane figure for an outline, a solid for a shape, a plane figure for a shape that is
//! `raise`d) and every key of `args` is held against that form's parameters: an unknown one is
//! refused with the list the form takes, and nothing is ignored. Ranges are the library's, held
//! when the numbers are known, at build.
//!
//! **The library builds in `f64` and the modeller holds `f32`** (`docs/design/FORM.md`, the
//! decisions of stage 2). A form is refused as data where what it is made of would not survive
//! the step: two of its points nearer than the modeller's floor at its size (the floor is the
//! greater of twice the weld that [`Shape::finalize`] makes at 10 µm, and 2⁻¹⁸ of the form's
//! reach, 64 of `f32`'s steps there), or a triangle that turns over or loses its area when its
//! points are rounded to `f32`. An outline's points are held to the same floor, and then by the
//! checks every outline meets (simple, enclosing an area).
//!
//! **What it records.** A shape made by `form` is a named shape like any other: its outlines
//! (the convex hulls of its points seen along its three axes; for a raised figure, the figure's
//! own outer ring and the boxes it is raised to, as `extrude`'s) and every triangle are its
//! record's ([`crate::made`]); each of its smooth surfaces gives a faceting record
//! ([`crate::Round`]) of the turn measured between its neighbouring faces and the surface's
//! radius (`Mesh::rounds`); a curved outline by name gives one of the turn between its
//! neighbouring chords and its arcs' radius (`Spec::facets`), on the shape made of it. Each is
//! measured in the finished model as every round shape is ([`crate::facet`]): under a uniform
//! scale as built and scaled; under an uneven stretch, a taper, a twist or a bend not measured,
//! and said so. **A solid's record of what it is built of is its hull** along each axis: a
//! torus reads as its disc seen from above, a star solid as its hull.

use specula_formae::{self as formae, raise, Built, Found, Kind as FKind, Mesh as FMesh};

use super::*;

/// The fields of a `raise`: what each raised form takes is said by [`raise_takes`].
pub(super) const RAISE: &[Field] = &[
    r("as", Choice(raise::RAISED)),
    o("thickness", Num),
    o("width", Num),
    o("round", Bool),
    o("both", Bool),
    o("rise", Num),
    o("inset", Num),
    o("steps", Int),
    o("run", Num),
    o("height", Num),
    o("margin", Num),
    o("plate", Num),
];

/// The `form` shape's fields (the form's own parameters are in `args`).
pub(super) const FORM_FIELDS: &[Field] = &[r("form", Name), o("args", FormArgs), o("detail", Int), o("raise", Obj(RAISE))];

/// What each raised form takes (the first `need` of them are required).
fn raise_takes(kind: &str) -> (&'static [&'static str], usize) {
    match kind {
        "slab" => (&["thickness"], 1),
        "bevelled" => (&["thickness", "width", "round", "both"], 2),
        "pillow" => (&["thickness", "rise", "inset"], 3),
        "stepped" => (&["steps", "rise", "run"], 3),
        _ => (&["height", "margin", "plate"], 3),
    }
}

/// Work units ([`crate::work`]) charged for each point of a form before it is built: the
/// library's build and its checks, the step to `f32`, the floor's grid and the outlines' hulls.
/// What every generator's mesh costs after that (finishing, levels of detail, the collider) is
/// charged where it is done, as for any generator.
///
/// **Measured** (2026-11-20, a release build on the reference laptop while other builds ran on
/// it: busy-machine times, so read the figures as an upper side, not a tight one). The library's
/// build of every curved solid at the detail nearest 32,000 points, with its checks and its cut
/// into triangles, took 2,800 to 6,300 ns a point (the icosphere the most; the torus knot 6,400,
/// the helix 4,700); every raised form of eleven figures at details 64 to 1,024 that built, 2,000
/// to 4,500. Through a whole recipe, a form shape cost 7,700 to 35,000 ns a point more than the
/// modeller's own sphere and lathe at the same number of triangles (7,000 to 11,600 ns a
/// triangle on that machine, measured twice: the noise of a busy machine is that wide), the
/// most for the cones (a fan of 4,096 thin triangles round one corner, which the modeller's
/// smooth normals visit for each). 10,000 covers every one of the library's builds with room for
/// the modeller's steps; the Möbius band is weighed apart ([`HEAVY`]).
pub const FORM_POINT: u64 = 10_000;

/// Forms that cost far more a point than [`FORM_POINT`], with their own weight: the Möbius band
/// searches its twisted strips for faces that cross (its wedge check does not prove them apart),
/// measured at 99,000 ns a point at 16,384 points on the same busy machine (the helix and the
/// torus knot, which also sweep, 4,700 and 6,400).
pub const HEAVY: &[(&str, u64)] = &[("mobius_band", 150_000)];

/// The work weight of a point of the form named `form` (its table name).
pub fn point_weight(form: &str) -> u64 {
    HEAVY.iter().find(|(n, _)| *n == form).map_or(FORM_POINT, |h| h.1)
}

/// `f32` keeps a coordinate to 2⁻²⁴ of its size; two points are kept 2⁻¹⁸ of the form's reach
/// apart (64 of those steps) so that no edge turns by more than about a degree when its ends
/// are rounded.
pub const F32_FLOOR: f64 = 1.0 / 262_144.0;

/// [`Shape::finalize`] welds points nearer than this (metres): two points of a form are kept at
/// least twice as far apart.
pub const WELD_M: f64 = 1e-5;

/// The floor at a reach: the least distance two points of a form may be apart.
pub fn floor_at(reach: f64) -> f64 {
    (2.0 * WELD_M).max(F32_FLOOR * reach)
}

/// A library refusal as the recipe's error at `path`.
fn refused(path: &str, er: formae::Error) -> RecipeError {
    match &er {
        formae::Error::OutOfRange { value, min, max, .. } => e(path, ErrorCode::OutOfRange, er.to_string()).with_numbers(*value, if value > max { *max } else { *min }),
        formae::Error::NotInteger { value, .. } => e(path, ErrorCode::OutOfRange, er.to_string()).with_numbers(*value, dmath::round_f64(*value)),
        formae::Error::TooManyPoints { points, ceiling, .. } => e(path, ErrorCode::Limit, er.to_string()).with_numbers(*points as f64, *ceiling as f64),
        formae::Error::UnknownParam { .. } | formae::Error::FixedByAlias { .. } | formae::Error::DuplicateParam { .. } => e(path, ErrorCode::UnknownField, er.to_string()),
        formae::Error::UnknownForm { .. } | formae::Error::Ambiguous { .. } => e(path, ErrorCode::UnknownName, er.to_string()),
        formae::Error::DetailOutOfRange { .. } => e(path, ErrorCode::OutOfRange, er.to_string()),
        // A raise that cannot go as deep as asked: say the way out. (A measured comparison's
        // builder spent nine builds finding that a heart takes no soft edge, then asked for a
        // tenth of a millimetre and called the square edge rounded.)
        formae::Error::Constraint { rule, .. } if *rule == "inset_too_deep" => e(
            path,
            ErrorCode::Geometry,
            format!(
                "{er}. What to do: a figure that narrows to a point or has a sharp notch (a heart, a crescent, a thin tooth) takes no deeper inset than the one named, so it cannot be \
                 given a soft edge this way; for a soft edge on such a figure, sculpt it (model_schema {{\"of\": \"sdf\"}} has a soft heart to start from), or raise it as a slab"
            ),
        ),
        _ => e(path, ErrorCode::Geometry, er.to_string()),
    }
}

/// The word looked up as `want` (a plane figure or a solid), with words that say why not where
/// it is another kind.
fn look_up(path: &str, word: &str, want: FKind, shape: bool) -> Result<Found, RecipeError> {
    if let Ok(f) = formae::find(word, Some(want)) {
        return Ok(f);
    }
    let other = |k: FKind| formae::find(word, Some(k)).is_ok();
    let msg = match want {
        FKind::Plane if other(FKind::Solid) => format!("'{word}' is a solid: an outline is a plane figure; build it as a shape, {{\"op\": \"form\", \"form\": \"{word}\"}}"),
        FKind::Plane if other(FKind::Curve) => format!("'{word}' is an open curve, which has no inside: an outline is a closed plane figure (a curve's band, such as arc_band, is one)"),
        FKind::Solid if other(FKind::Plane) => {
            format!("'{word}' is a plane figure: as a shape it is raised, with \"raise\": {{\"as\": one of {}, ...}}; or it is an outline, {{\"form\": \"{word}\"}}, for extrude, loft, lathe's and sweep's makers", raise::RAISED.join(", "))
        }
        FKind::Solid if other(FKind::Curve) => format!("'{word}' is an open curve, not a solid"),
        _ => {
            let near = formae::nearest(word, Some(want));
            format!("no {} is called '{word}' (nearest: {})", if want == FKind::Solid { "solid" } else { "plane figure" }, near.join(", "))
        }
    };
    let code = if shape && want == FKind::Solid && other(FKind::Plane) { ErrorCode::MissingField } else { ErrorCode::UnknownName };
    Err(e(path, code, msg))
}

/// What the shape library says of a word a recipe used where it knows no such shape (an `op`)
/// or outline (a field of an outline): how to write it by name when it is one of the library's,
/// and the library's nearest names otherwise (`shape`: for an `op`; a field is hinted only when
/// it is a word of the library, never by nearness, so a mistyped field is not sent to a form).
/// So a guess ("heart", "gear") costs one request and not three.
pub(super) fn library_hint(word: &str, shape: bool) -> Option<String> {
    let get = format!("formae_get {{\"name\": \"{word}\"}} gives its parameters");
    if formae::find(word, Some(FKind::Solid)).is_ok() {
        return Some(format!("'{word}' is a solid of the shape library: {{\"op\": \"form\", \"form\": \"{word}\", \"args\": {{...}}}} ({get})"));
    }
    if formae::find(word, Some(FKind::Plane)).is_ok() {
        return Some(if shape {
            format!("'{word}' is a plane figure of the shape library: as a shape it is raised, {{\"op\": \"form\", \"form\": \"{word}\", \"raise\": {{\"as\": \"slab\", \"thickness\": 0.1}}}}, or extruded, {{\"op\": \"extrude\", \"outline\": {{\"form\": \"{word}\"}}, \"height\": 0.1}} ({get})")
        } else {
            format!("'{word}' is a plane figure of the shape library: an outline by name is {{\"form\": \"{word}\", \"args\": {{...}}}} ({get})")
        });
    }
    if formae::find(word, Some(FKind::Curve)).is_ok() {
        return Some(format!("'{word}' is an open curve of the shape library, which has no inside: its band (formae_get {{\"name\": \"{word}\"}} names it) is a plane figure"));
    }
    if !shape {
        return None;
    }
    let near = formae::nearest(word, None);
    (!near.is_empty()).then(|| format!("the shape library's nearest: {} (a solid is {{\"op\": \"form\", \"form\": NAME}}; formae_list names them all)", near.join(", ")))
}

impl Parser<'_> {
    /// A form's `args`: an object of the form's parameter names, each a number or an
    /// expression (checked against the form by [`Parser::check_form`]).
    pub(super) fn form_args(&self, path: &str, v: &Json) -> Result<Val, RecipeError> {
        let o = v.as_object().ok_or_else(|| type_err(path, "an object of the form's parameters: {name: number or expression}"))?;
        let mut node = Node { path: path.to_string(), op: "args".into(), when: None, fields: Vec::new() };
        for (k, x) in o {
            let fp = format!("{path}.{k}");
            if !is_ident(k) {
                return Err(type_err(&fp, "a parameter's name"));
            }
            node.fields.push((k.clone(), Val::Num(self.expr(&fp, x)?)));
        }
        Ok(Val::Obj(node))
    }

    /// A `form` node (a shape, or an outline when `outline`): the word names a form of the kind
    /// it needs, its `args` are that form's, and a `raise` is complete.
    pub(super) fn check_form(&self, n: &Node, outline: bool) -> Result<(), RecipeError> {
        let Some(Val::Str(word)) = n.get("form") else { return Ok(()) };
        let raised = n.get("raise").is_some();
        let found = look_up(&n.at("form"), word, if outline || raised { FKind::Plane } else { FKind::Solid }, !outline)?;
        if let Some(Val::Obj(args)) = n.get("args") {
            let takes: Vec<&'static str> = formae::params_of(&found).iter().map(|p| p.name).collect();
            for (k, _) in &args.fields {
                if !takes.contains(&k.as_str()) {
                    let list = if takes.is_empty() { "no parameters".to_string() } else { takes.join(", ") };
                    return Err(e(&args.at(k), ErrorCode::UnknownField, format!("'{k}' is not a parameter of '{word}'; it takes: {list}")));
                }
            }
        }
        if let Some(Val::Obj(r)) = n.get("raise") {
            let Some(Val::Str(kind)) = r.get("as") else { return Err(e(&r.at("as"), ErrorCode::MissingField, "a raise needs \"as\"")) };
            let (takes, need) = raise_takes(kind);
            for (k, _) in &r.fields {
                if k != "as" && !takes.contains(&k.as_str()) {
                    return Err(e(&r.at(k), ErrorCode::UnknownField, format!("'{k}' is not a field of a raise as {kind}; it takes: as, {}", takes.join(", "))));
                }
            }
            for k in &takes[..need] {
                if r.get(k).is_none() {
                    return Err(e(&r.at(k), ErrorCode::MissingField, format!("a raise as {kind} needs {}", takes[..need].join(", "))));
                }
            }
        }
        Ok(())
    }
}

/// The convex hull of points (Andrew's monotone chain), counter-clockwise.
fn hull(mut p: Vec<[f64; 2]>) -> Vec<V2> {
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup();
    if p.len() < 3 {
        return p.iter().map(|q| [q[0] as f32, q[1] as f32]).collect();
    }
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut h: Vec<[f64; 2]> = Vec::with_capacity(2 * p.len());
    for pass in 0..2 {
        let start = h.len();
        let it: Box<dyn Iterator<Item = &[f64; 2]>> = if pass == 0 { Box::new(p.iter()) } else { Box::new(p.iter().rev()) };
        for &q in it {
            while h.len() >= start + 2 && cross(h[h.len() - 2], h[h.len() - 1], q) <= 0.0 {
                h.pop();
            }
            h.push(q);
        }
        h.pop();
    }
    h.iter().map(|q| [q[0] as f32, q[1] as f32]).collect()
}

/// The first two points nearer each other than `floor`, by a grid of cells `floor` wide (each
/// point against the 27 cells about its own): `(i, j, distance)`.
fn too_near(points: &[[f64; 3]], floor: f64) -> Option<(usize, usize, f64)> {
    let cell = |p: [f64; 3]| [dmath::floor_f64(p[0] / floor) as i64, dmath::floor_f64(p[1] / floor) as i64, dmath::floor_f64(p[2] / floor) as i64];
    let mut grid: BTreeMap<[i64; 3], Vec<usize>> = BTreeMap::new();
    for (i, p) in points.iter().enumerate() {
        let c = cell(*p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(list) = grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                        for &j in list {
                            let q = points[j];
                            let d = ((p[0] - q[0]) * (p[0] - q[0]) + (p[1] - q[1]) * (p[1] - q[1]) + (p[2] - q[2]) * (p[2] - q[2])).sqrt();
                            if d < floor {
                                return Some((j, i, d));
                            }
                        }
                    }
                }
            }
        }
        grid.entry(c).or_default().push(i);
    }
    None
}

/// The greatest size of a coordinate.
fn reach(points: &[[f64; 3]]) -> f64 {
    points.iter().flatten().fold(0.0f64, |a, x| a.max(x.abs()))
}

/// Points nearer than the floor at their reach, refused at `path`.
fn hold_floor(points: &[[f64; 3]], path: &str, what: &str) -> Result<(), RecipeError> {
    let r = reach(points);
    let floor = floor_at(r);
    if let Some((i, j, d)) = too_near(points, floor) {
        return Err(e(
            path,
            ErrorCode::Geometry,
            format!("{what}: points {i} and {j} are {d:e} m apart, under the modeller's floor of {floor:e} m at its reach of {r} m (the modeller holds points in f32, to about 2^-24 of their size, and welds those within 10 µm): give it larger parts against its size, or a coarser detail"),
        )
        .with_numbers(d, floor));
    }
    Ok(())
}

/// A library mesh as the modeller's shape: its faces cut into triangles, its points in `f32`.
/// Refused where the step to `f32` would change it ([`hold_floor`], and a triangle that turns
/// over or loses its area when rounded).
fn to_shape(m: &FMesh, path: &str) -> Result<Shape, RecipeError> {
    hold_floor(&m.points, path, "the form")?;
    let positions: Vec<V3> = m.points.iter().map(|p| crate::shape::canon([p[0] as f32, p[1] as f32, p[2] as f32])).collect();
    let tris = m.triangles();
    let normal = |a: [f64; 3], b: [f64; 3], c: [f64; 3]| {
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
    };
    let wide = |p: V3| [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
    let mut out = Vec::with_capacity(tris.len());
    for (k, (_, t)) in tris.iter().enumerate() {
        let n64 = normal(m.points[t[0] as usize], m.points[t[1] as usize], m.points[t[2] as usize]);
        let n32 = normal(wide(positions[t[0] as usize]), wide(positions[t[1] as usize]), wide(positions[t[2] as usize]));
        let d = n64[0] * n32[0] + n64[1] * n32[1] + n64[2] * n32[2];
        if d.is_nan() || d <= 0.0 {
            return Err(e(path, ErrorCode::Geometry, format!("the form's triangle {k} turns over or loses its area when its points are held in f32: give it larger parts against its size, or a coarser detail")).with_numbers(d, 0.0));
        }
        // (The modeller drops a triangle under its least area when it finalizes: the mesh would open.)
        let area = 0.5 * crate::math3::length(crate::math3::tri_normal(positions[t[0] as usize], positions[t[1] as usize], positions[t[2] as usize]));
        if area.is_nan() || area < crate::mesh::MIN_TRIANGLE_AREA {
            return Err(e(path, ErrorCode::Geometry, format!("the form's triangle {k} has {area:e} m2 in f32, under the {:e} m2 the modeller keeps (it would drop it, and the mesh would open): give it larger parts, or a coarser detail", crate::mesh::MIN_TRIANGLE_AREA)).with_numbers(f64::from(area), f64::from(crate::mesh::MIN_TRIANGLE_AREA)));
        }
        out.push(*t);
    }
    let n = out.len();
    Ok(Shape { colors: vec![[1.0; 4]; positions.len()], positions, tris: out, tags: vec![crate::shape::TAG_FACE; n], bevel_tint: None, owners: Vec::new() })
}

/// Open edges of a shape as it was made: points told apart by their exact bits, every triangle
/// counted (none dropped for its area, as [`Shape::open_edge_count`] does), each directed edge
/// matched by one reverse.
fn open_edges_as_made(s: &Shape) -> usize {
    let mut ids: BTreeMap<[u32; 3], u32> = BTreeMap::new();
    let id: Vec<u32> = s
        .positions
        .iter()
        .map(|p| {
            let c = crate::shape::canon(*p);
            let k = [c[0].to_bits(), c[1].to_bits(), c[2].to_bits()];
            let next = ids.len() as u32;
            *ids.entry(k).or_insert(next)
        })
        .collect();
    let mut edges: BTreeMap<(u32, u32), i32> = BTreeMap::new();
    for t in &s.tris {
        for k in 0..3 {
            *edges.entry((id[t[k] as usize], id[t[(k + 1) % 3] as usize])).or_insert(0) += 1;
        }
    }
    edges.iter().filter(|(&(a, b), &c)| a == b || c != 1 || edges.get(&(b, a)).copied().unwrap_or(0) != 1).count()
}

/// The field of `n` that is an outline by name (`outline` for extrude and loft, `profile` for
/// sweep), if one is.
pub(super) fn form_outline_field(n: &Node) -> Option<&'static str> {
    ["outline", "profile"].into_iter().find(|k| matches!(n.get(k), Some(Val::Obj(o)) if o.get("form").is_some()))
}

impl Ctx<'_> {
    /// A shape made from an outline by name is built closed or refused as data: what the
    /// modeller does to every shape when it finishes a model (it welds points within 10 µm and
    /// drops a triangle under [`crate::mesh::MIN_TRIANGLE_AREA`]) must open none of its edges.
    /// Held at the size the generator made it, as the `form` shape's floor is. The outline's own
    /// points are kept apart by [`hold_floor`]; what this adds is the shape the outline was made
    /// into (a cap's thin ears, a wall shorter than the floor), which the outline alone cannot
    /// say. The modeller's own outlines (`sides`, `rect`, `points` ...) are not held to it: no
    /// existing recipe's outcome changes (docs/design/PROCEDURAL_MODELLING.md, "Shapes by name").
    pub(super) fn hold_closed(&self, n: &Node, s: &Shape) -> Result<(), RecipeError> {
        let Some(field) = form_outline_field(n) else { return Ok(()) };
        let before = open_edges_as_made(s);
        let mut w = s.clone();
        w.weld(WELD_M as f32);
        let after = w.open_edge_count();
        // (An extrusion, a loft and a capped or closed sweep are closed solids by design: none of
        // their edges may be open, as made or when finished. An open sweep keeps the edges it
        // was made with, and must open no more.)
        let closed_by_design = !(n.op == "sweep" && !self.bool(n, "caps", true) && !self.bool(n, "closed", false));
        if after <= before && !(closed_by_design && after > 0) {
            return Ok(());
        }
        let least = s
            .tris
            .iter()
            .map(|t| 0.5 * crate::math3::length(crate::math3::tri_normal(s.positions[t[0] as usize], s.positions[t[1] as usize], s.positions[t[2] as usize])))
            .fold(f32::INFINITY, f32::min);
        let merged = s.positions.len() - w.positions.len().min(s.positions.len());
        let word = match n.get(field) {
            Some(Val::Obj(o)) => match o.get("form") {
                Some(Val::Str(w)) => w.clone(),
                _ => String::new(),
            },
            _ => String::new(),
        };
        Err(e(
            &n.at(field),
            ErrorCode::Geometry,
            format!(
                "the {} made of the outline '{word}' is not closed: {before} of its edges are open as made (in f32), and {after} when the modeller finishes it, which welds points within 10 µm ({merged} merged here) and drops triangles under {:e} m2 (its least has {least:e} m2): give the outline larger parts against its size and detail (a coarser detail, a larger size, parts nearer in size), or the {} more height",
                n.op,
                crate::mesh::MIN_TRIANGLE_AREA,
                n.op
            ),
        )
        .with_numbers(f64::from(least), f64::from(crate::mesh::MIN_TRIANGLE_AREA)))
    }

    /// The faceting records of a form (`what`, its table name) built at `detail`: one for each
    /// `(turn between neighbouring faces or chords in degrees, radius)` its build measured,
    /// noted on the shape being built ([`Ctx::round`]), its number `detail`. None is a ring
    /// round the shape's own Y axis of `detail` sides (a form's surfaces are not one), so under
    /// a stretch that is not the same every way the finished model says it is not measured;
    /// under a move, a turn or a uniform scale it is measured, the radius and the sag scaled.
    ///
    /// What its count may be at most (the record's `most`, which advice never passes, and the
    /// prediction it is checked by): `size(k)` is the shape's triangles and points at detail
    /// `k`, from the library's count (exact, and no build). A form's counts step (a piece has
    /// `max(2, ⌈detail · turns⌉)` segments) and grow as a line or as the square of the detail,
    /// so no line through two counts and no square of a ratio bounds them. The record's
    /// prediction is a line from the built detail `n`: `T(n)`, and `per` the steepest rise
    /// over every `k` from `n` to the most of `⌈(G(k) − T(n)) / (k − n)⌉`, where `G(k)` is the
    /// greatest over every detail `j ≤ k` of `T(j)` and of `T(n)·V(j)/V(n)` (rounded up)
    /// ([`crate::facet::Grows::Above`]). So the line is no less than what the build makes at
    /// any detail up to `k`, and its growth from `T(n)` is no less than the points' growth: the
    /// faceting record bounds the build's work at `k` by its elements' growth (`facet.rs`,
    /// `Spent`), and a coarse far level built at a detail under `k` is under the line too
    /// (`T(j)` steps, and a closed mesh of genus over one has more points to its triangles as
    /// it is coarser, so neither bounds the other). The most is the library's
    /// [`formae::DETAIL_MAX`], or the first detail past a part's limit (where the advice
    /// stops, by that limit), or the last whose count the library answers; the build's work
    /// limit then stops it where the finished model's work says (`facet.rs`, `most_count`).
    ///
    /// An outline by name (`outline`: `size` is then a measure its triangles grow no faster
    /// than, not the triangles: what takes the outline makes them) is predicted by the square of
    /// the ratio from what was built ([`crate::facet::Grows::Square`]), and its most is the last
    /// detail up to which `size(k) · n² <= size(n) · k²` holds at every detail from `n` (a step
    /// past the square ends it there; the advice stops at a part's limit by the square itself).
    fn form_rounds(&self, what: &'static str, detail: u32, outline: bool, size: &dyn Fn(u32) -> Option<(u64, u64)>, list: impl IntoIterator<Item = (f64, f64)>) {
        let list: Vec<(f64, f64)> = list.into_iter().collect();
        if list.is_empty() || self.coarse {
            return;
        }
        let (at_n, points_n) = size(detail).unwrap_or((0, 0));
        // (`G(j)` of the doc above, for a solid: its triangles, or its points' growth from the
        // built detail in triangles, whichever is greater.)
        let grown = |(t, v): (u64, u64)| {
            let by_points = (u128::from(at_n) * u128::from(v)).div_ceil(u128::from(points_n.max(1)));
            u128::from(t).max(by_points)
        };
        let mut top = 0u128;
        if !outline {
            for j in formae::DETAIL_MIN..detail {
                if let Some(c) = size(j) {
                    top = top.max(grown(c));
                }
            }
        }
        let (mut most, mut per) = (detail, 0u64);
        let n2 = u128::from(detail) * u128::from(detail);
        for k in detail.saturating_add(1)..=formae::DETAIL_MAX {
            let Some((t, v)) = size(k) else { break };
            let k2 = u128::from(k) * u128::from(k);
            if outline && u128::from(t) * n2 > u128::from(at_n) * k2 {
                break;
            }
            let rise = if outline {
                u128::from(t)
            } else {
                top = top.max(grown((t, v)));
                top
            };
            let step = rise.saturating_sub(u128::from(at_n)).div_ceil(u128::from(k - detail));
            per = per.max(u64::try_from(step).unwrap_or(u64::MAX));
            most = k;
            if !outline && t > MAX_PART_TRIANGLES as u64 {
                break;
            }
        }
        let grows = if outline { crate::facet::Grows::Square } else { crate::facet::Grows::Above { n: detail, at_n, per } };
        let alone = |k: u32| grows.at(k) as usize;
        for (turn, radius) in list {
            let c = Counted { value: detail, range: (i64::from(formae::DETAIL_MIN), i64::from(most)), grows, alone: &alone };
            self.round(what, "detail", c, turn as f32, radius as f32, false);
        }
    }

    /// The form's parameters as given, evaluated: `(name, value)` for each the recipe gives.
    fn form_values(&self, n: &Node, found: &Found) -> Result<Vec<(&'static str, f64)>, RecipeError> {
        let Some(Val::Obj(args)) = n.get("args") else { return Ok(Vec::new()) };
        let mut out = Vec::new();
        for p in formae::params_of(found) {
            if let Some(v) = args.get(p.name) {
                out.push((p.name, self.val_num(v, &args.at(p.name))?));
            }
        }
        Ok(out)
    }

    /// The detail asked for (the library's range), halved for the coarse far level as other
    /// generators halve their segments.
    fn form_detail(&self, n: &Node) -> Result<u32, RecipeError> {
        let d = self.int(n, "detail", i64::from(formae::DETAIL_DEFAULT), i64::from(formae::DETAIL_MIN), i64::from(formae::DETAIL_MAX))?;
        Ok(if self.coarse { (d / 2).max(formae::DETAIL_MIN) } else { d })
    }

    /// An outline by name: the plane figure's one ring as `[x, z]` points (the figure's `y` is
    /// `-z`, as the library's own raised forms place it), counted against `max_points` (times
    /// `later`) before it is built. Its faceting is recorded at `path`.
    pub(super) fn form_outline(&self, n: &Node, path: &str, max_points: usize, later: usize) -> Result<Vec<V2>, RecipeError> {
        let Some(Val::Str(word)) = n.get("form") else { return Err(e(path, ErrorCode::MissingField, "no form")) };
        let found = look_up(&n.at("form"), word, FKind::Plane, false)?;
        let spec = formae::Spec::of(found, &self.form_values(n, &found)?).map_err(|er| refused(&n.at("args"), er))?;
        let detail = self.form_detail(n)?;
        let c = spec.count(detail).map_err(|er| refused(path, er))?;
        let want = (c.points as usize).saturating_mul(later);
        if want > max_points {
            return Err(e(path, ErrorCode::Limit, format!("'{word}' at detail {detail} has {} points (times {later} for what is done to it after); an outline has at most {max_points}: a lower detail", c.points)).with_numbers(want as f64, max_points as f64));
        }
        let built = spec.build(detail).map_err(|er| refused(path, er))?;
        let Built::Outline(o) = built else { return Err(e(path, ErrorCode::Geometry, format!("'{word}' does not build to an outline"))) };
        if o.rings.len() != 1 {
            return Err(e(
                path,
                ErrorCode::Geometry,
                format!("'{word}' has {} holes, and an outline is one ring: as a solid with its holes it is a shape, {{\"op\": \"form\", \"form\": \"{word}\", \"raise\": {{\"as\": \"slab\", \"thickness\": ...}}}}", o.rings.len() - 1),
            )
            .with_numbers(o.rings.len() as f64 - 1.0, 0.0));
        }
        let ring: Vec<[f64; 3]> = o.rings[0].iter().map(|p| [p[0], 0.0, -p[1]]).collect();
        hold_floor(&ring, path, &format!("the outline '{word}'"))?;
        if let Ok(Some((turn, radius))) = spec.facets(detail) {
            // (An outline's triangles are made by what takes it: `a·points + b` with `b` no
            // less than `-2a` (an extrusion's walls and caps, a lathe's bands, a cap alone),
            // whose ratio from one detail to a greater is at most that of `points - 2`.)
            let size = |k: u32| spec.count(k).ok().map(|c| (c.points.saturating_sub(2), c.points.saturating_sub(2)));
            self.form_rounds(found.form.name, detail, true, &size, [(turn, radius)]);
        }
        Ok(ring.iter().map(|p| [p[0] as f32, p[2] as f32]).collect())
    }

    /// The raised form of `spec` that `r` (a `raise` node) asks for.
    fn raised(&self, spec: &formae::Spec, r: &Node) -> Result<formae::ops::Solid, RecipeError> {
        let kind = self.str(r, "as", "slab");
        let num = |k: &str| self.num(r, k, 0.0);
        let made = match kind {
            "slab" => raise::slab(spec, num("thickness")?),
            "bevelled" => raise::bevelled(spec, num("thickness")?, num("width")?, self.bool(r, "round", false), self.bool(r, "both", false)),
            "pillow" => raise::pillow(spec, num("thickness")?, num("rise")?, num("inset")?),
            "stepped" => {
                let steps = self.int(r, "steps", 1, 1, i64::from(raise::MAX_STEPS))?;
                raise::stepped(spec, steps, num("rise")?, num("run")?)
            }
            _ => raise::relief(spec, num("height")?, num("margin")?, num("plate")?),
        };
        made.map_err(|er| refused(&r.path, er))
    }

    /// A shape by name: a solid of the library, or a plane figure raised. Counted (its triangles
    /// against the part's and the model's limits, its points as work) before it is built; held
    /// to the modeller's floor in `f32`; recorded as what it is made of, with a faceting record
    /// for each of its smooth surfaces.
    pub(super) fn form_shape(&self, n: &Node) -> Result<Shape, RecipeError> {
        let Some(Val::Str(word)) = n.get("form") else { return Err(e(&n.path, ErrorCode::MissingField, "a form shape needs \"form\"")) };
        let raise_node = match n.get("raise") {
            Some(Val::Obj(r)) => Some(r),
            _ => None,
        };
        let found = look_up(&n.at("form"), word, if raise_node.is_some() { FKind::Plane } else { FKind::Solid }, true)?;
        let spec = formae::Spec::of(found, &self.form_values(n, &found)?).map_err(|er| refused(&n.at("args"), er))?;
        let detail = self.form_detail(n)?;
        let solid = raise_node.map(|r| self.raised(&spec, r)).transpose()?;
        let (points, genus) = match &solid {
            Some(s) => (s.count(detail).map_err(|er| refused(&n.path, er))?.points, s.genus()),
            None => (spec.count(detail).map_err(|er| refused(&n.path, er))?.points, spec.genus().unwrap_or(0)),
        };
        // (A closed mesh of V points and genus g whose faces are cut into triangles has
        // 2V − 4 + 4g of them, whatever its faces' sizes.)
        let predicted = (2 * points as usize + 4 * genus as usize).saturating_sub(4);
        self.tally.grow(&n.path, 0, predicted)?;
        let one = points.saturating_mul(point_weight(found.form.name));
        charge(self.tally, &n.path, one)?;
        let mesh = match &solid {
            // A refused raise may build itself again to name an inset that holds
            // (`raise::SEARCH_BUILDS` builds of its own size): it makes no more of those than the
            // model's budget has room for, so a refusal costs no more than the budget either.
            Some(s) => {
                let left = procgen::MAX_BUILD_WORK.saturating_sub(self.tally.work.get());
                let room = u32::try_from(left / one.max(1)).unwrap_or(u32::MAX);
                s.with_search(room.min(s.search_builds())).build(detail).map_err(|er| refused(&n.path, er))?
            }
            None => match spec.build(detail).map_err(|er| refused(&n.path, er))? {
                Built::Mesh(m) => m,
                _ => return Err(e(&n.path, ErrorCode::Geometry, format!("'{word}' does not build to a solid"))),
            },
        };
        let mut s = to_shape(&mesh, &n.path)?;
        let what = found.form.name;
        let size = |k: u32| {
            let points = match &solid {
                Some(s) => s.count(k).ok()?.points,
                None => spec.count(k).ok()?.points,
            };
            Some(((2 * points + 4 * u64::from(genus)).saturating_sub(4), points))
        };
        self.form_rounds(what, detail, false, &size, mesh.rounds().iter().map(|r| (r.turn_deg, r.radius)));
        // Its outlines: a raised figure's own outer ring and the boxes it is raised to (as an
        // extruded outline's); a solid's hull seen along each of its axes.
        let views = match (&solid, spec.build(detail)) {
            (Some(_), Ok(Built::Outline(o))) => {
                let ring: Vec<V2> = o.rings[0].iter().map(|p| [p[0] as f32, -p[1] as f32]).collect();
                let half = made::half_extents(&ring);
                let h = mesh.bounds().map_or(0.0, |(lo, hi)| (hi[1] - lo[1]) as f32);
                vec![(ring, 1.0), (made::trapezoid(half[0], half[0], h), 1.0), (made::trapezoid(half[1], half[1], h), 1.0)]
            }
            _ => {
                let along = |a: usize, b: usize| hull(mesh.points.iter().map(|p| [p[a], p[b]]).collect());
                vec![(along(0, 2), 1.0), (along(0, 1), 1.0), (along(2, 1), 1.0)]
            }
        };
        self.made(what, views);
        self.tally.made_now(&n.path, &mut s);
        self.tally.settle(predicted, s.triangle_count());
        check_extent(&s, &n.path)?;
        Ok(s)
    }
}
