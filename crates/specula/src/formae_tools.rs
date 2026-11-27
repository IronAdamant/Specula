//! The shape library in the engine (docs/design/FORMAE.md): the two read-only tools
//! `formae_list` (the catalogue, with the measured human figure beside it) and `formae_get`
//! (one form, with its measures and snippets ready to paste), and the `shape2d` items by a
//! form's name ([`items`]: a plane figure or a curve; a solid drawn in a named view or cut into
//! layers, as polygons in the 2D world).
//!
//! **Where the work runs.** Both tools are answered on the calling agent's own protocol thread
//! (`CallerThread::Formae`), never on the engine thread: they read nothing of the world, and the
//! library is pure, so nothing waits on the engine and the engine waits on nothing here. Each
//! answer is bounded before any work: `formae_get` counts a build before it makes it and builds
//! at most [`MAX_GET_POINTS`] points, draws at most [`MAX_DRAW_FACES`] faces, and the answer has
//! a ceiling of [`MAX_ANSWER_BYTES`]. A `shape2d` item by name is parsed where every `shape2d` is
//! (on the caller's thread for a tool, and where a world loads), held to the shape's own point
//! budget before it is built and to [`MAX_ITEM_SOLID_POINTS`] and [`MAX_DRAW_FACES`] for a solid:
//! a test builds the dearest item the limits allow and states what it did.

use crate::tools::{argument_error, check_keys, int_arg, string_arg};

/// A string argument of at most `max` bytes.
fn text(args: &Json, key: &str, max: usize) -> Result<Option<String>, String> {
    match string_arg(args, key)? {
        Some(s) if s.len() > max => Err(format!("'{key}' is {} bytes; at most {max}", s.len())),
        s => Ok(s.map(str::to_string)),
    }
}
use specula_core::Json;
use specula_formae::{self as formae, draw, slice, Built, Error, Found, Kind, Spec};
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};

/// Rows of the catalogue a page (and the default).
pub const MAX_LIST: usize = 200;
const DEFAULT_LIST: usize = 20;
/// The ceiling of a page of rows, bytes (about two thousand tokens): a page past it ends early,
/// with `next_cursor`.
pub const MAX_ROWS_BYTES: usize = 8 * 1024;
/// The ceiling of the first answer (`formae_list` with nothing asked), bytes: held by a test.
pub const MAX_INDEX_BYTES: usize = 3 * 1024;
/// What `formae_get` answers without `full` stays under, bytes: held by a test for every word.
pub const MAX_SHORT_BYTES: usize = 3 * 1024;
/// The ceiling of an answer of either tool, bytes.
pub const MAX_ANSWER_BYTES: usize = 64 * 1024;
/// The most points `formae_get` builds to measure a form (a larger count is answered without
/// the measures as built, with the detail that would fit).
pub const MAX_GET_POINTS: u64 = 16_384;
/// The most faces a solid may have to be drawn (by `formae_get`'s snippet or a `shape2d` item):
/// hidden surfaces are found pair by pair where the solid is not convex.
pub const MAX_DRAW_FACES: u64 = 4_096;
/// The most points a solid in a `shape2d` item may have.
pub const MAX_ITEM_SOLID_POINTS: u64 = 4_096;
/// The most layers a `shape2d` item cuts a solid into.
pub const MAX_ITEM_LAYERS: u32 = 64;
/// The most parameters given in `args`, and a word's length.
pub const MAX_ARGS: usize = 16;
pub const MAX_WORD: usize = 64;
/// The most anchors an answer lists.
pub const MAX_ANCHORS: usize = 64;

/// What the 2D world draws of an item by name: rings (closed: filled and stroked; open: a curve,
/// stroked only), with a shade in 0..1 for the drawn faces and layers of a solid (`None`: the
/// item's own colour).
pub struct Piece {
    pub rings: Vec<Vec<[f32; 2]>>,
    pub closed: bool,
    pub shade: Option<f32>,
}

/// An item's canonical fields, in order.
pub(crate) type Canon = Vec<(&'static str, Json)>;

/// A word looked up as `kind` (or any kind), with the nearest names when it is none.
fn look_up(word: &str, kind: Option<Kind>) -> Result<Found, String> {
    formae::find(word, kind).map_err(|e| match e {
        Error::Ambiguous { .. } => format!("'{word}' names more than one kind of form: say which ({e})"),
        _ => {
            let near = formae::nearest(word, kind);
            format!("no form is called '{word}'{} (formae_list has them all)", if near.is_empty() { String::new() } else { format!(": nearest {}", near.join(", ")) })
        }
    })
}

/// `args`: an object of the form's parameters, numbers; an unknown one is refused with the list.
fn given(found: &Found, v: Option<&Json>, what: &str) -> Result<Vec<(&'static str, f64)>, String> {
    let Some(v) = v.filter(|v| !v.is_null()) else { return Ok(Vec::new()) };
    let o = v.as_object().ok_or_else(|| format!("'{what}.args' must be an object of the form's parameters: {{name: number}}"))?;
    if o.len() > MAX_ARGS {
        return Err(format!("'{what}.args' has {} parameters; at most {MAX_ARGS}", o.len()));
    }
    let params = formae::params_of(found);
    let mut out = Vec::new();
    for (k, x) in o {
        let Some(p) = params.iter().find(|p| p.name == k) else {
            let list: Vec<&str> = params.iter().map(|p| p.name).collect();
            return Err(format!("'{what}.args.{k}': '{}' takes {}", found.form.name, if list.is_empty() { "no parameters".to_string() } else { list.join(", ") }));
        };
        let x = x.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("'{what}.args.{k}' must be a number"))?;
        out.push((p.name, x));
    }
    Ok(out)
}

/// The detail asked for (the library's range), or the default.
fn detail_of(v: Option<&Json>, what: &str) -> Result<u32, String> {
    match v.filter(|v| !v.is_null()) {
        None => Ok(formae::DETAIL_DEFAULT),
        Some(d) => d
            .as_u64()
            .filter(|d| (u64::from(formae::DETAIL_MIN)..=u64::from(formae::DETAIL_MAX)).contains(d))
            .map(|d| d as u32)
            .ok_or_else(|| format!("'{what}.detail' must be a whole number from {} to {}", formae::DETAIL_MIN, formae::DETAIL_MAX)),
    }
}

fn refusal(e: &Error) -> String {
    format!("{e} (code {})", e.code())
}

/// The canonical `args`: the given parameters in the form's own order.
fn args_json(found: &Found, given: &[(&'static str, f64)]) -> Json {
    Json::obj(formae::params_of(found).iter().filter_map(|p| given.iter().find(|g| g.0 == p.name).map(|g| (p.name, Json::from(g.1)))))
}

/// A `shape2d` item by name (`two_d`): its pieces, and its canonical fields (what is saved and
/// hashed: the word, the given numbers, the detail and placing, never the points). `budget` is
/// the shape's own count of input points ([`crate::two_d::MAX_SHAPE_INPUT`]): what the item
/// makes is taken from it before it is built.
pub(crate) fn items(item: &Json, what: &str, budget: &mut usize) -> Result<(Vec<Piece>, Canon), String> {
    let solid = item.get("solid").is_some();
    let key = if solid { "solid" } else { "form" };
    let word = item.get(key).and_then(Json::as_str).filter(|w| w.len() <= MAX_WORD).ok_or_else(|| format!("'{what}.{key}' must be a form's name (at most {MAX_WORD} bytes)"))?;
    let found = if solid { look_up(word, Some(Kind::Solid)) } else { formae::find(word, Some(Kind::Plane)).or_else(|_| formae::find(word, Some(Kind::Curve))).map_err(|_| look_up(word, None).err().unwrap_or_else(|| format!("'{word}' is a solid: use 'solid', drawn in a view or in layers"))) }.map_err(|e| format!("'{what}.{key}': {e}"))?;
    let given = given(&found, item.get("args"), what)?;
    let spec = Spec::of(found, &given).map_err(|e| format!("'{what}.args': {}", refusal(&e)))?;
    let detail = detail_of(item.get("detail"), what)?;
    let at = match item.get("at") {
        None => [0.0, 0.0],
        Some(a) => {
            let v: Vec<f64> = a.as_array().map(|a| a.iter().filter_map(Json::as_f64).collect()).unwrap_or_default();
            if v.len() != 2 || v.iter().any(|x| !x.is_finite() || x.abs() > 1e6) {
                return Err(format!("'{what}.at' must be [x, y], within ±1e6"));
            }
            [v[0], v[1]]
        }
    };
    let scale = match item.get("scale") {
        None => 1.0,
        Some(s) => s.as_f64().filter(|s| *s > 0.0 && *s <= 1e4).ok_or_else(|| format!("'{what}.scale' must be a number in (0, 10000]"))?,
    };
    let rotate = match item.get("rotate") {
        None => 0.0,
        Some(r) => r.as_f64().filter(|r| r.abs() <= 3600.0).ok_or_else(|| format!("'{what}.rotate' must be degrees within ±3600"))?,
    };
    if solid && item.get("rotate").is_some() {
        return Err(format!("'{what}.rotate' turns a plane figure; a solid is turned by its 'view'"));
    }
    let (sin, cos) = specula_core::dmath::sin_cos_f64(rotate.to_radians());
    let place = |p: [f64; 2]| -> Result<[f32; 2], String> {
        let (x, y) = (scale * (p[0] * cos - p[1] * sin) + at[0], scale * (p[0] * sin + p[1] * cos) + at[1]);
        if !(x.is_finite() && y.is_finite() && x.abs() <= 1e6 && y.abs() <= 1e6) {
            return Err(format!("'{what}': a point of '{word}' falls at ({x}, {y}), outside ±1e6"));
        }
        Ok([x as f32, y as f32])
    };
    let take = |budget: &mut usize, n: u64| -> Result<(), String> {
        if n > *budget as u64 {
            return Err(format!("'{what}': '{word}' at detail {detail} makes {n} points, and the shape has {} of its {} left: a lower detail", *budget, crate::two_d::MAX_SHAPE_INPUT));
        }
        *budget -= n as usize;
        Ok(())
    };
    let mut canon: Vec<(&'static str, Json)> = vec![(key, Json::from(spec.name()))];
    if !given.is_empty() {
        canon.push(("args", args_json(&found, &given)));
    }
    if detail != formae::DETAIL_DEFAULT {
        canon.push(("detail", Json::from(detail)));
    }
    let mut pieces = Vec::new();
    if !solid {
        let count = spec.count(detail).map_err(|e| format!("'{what}': {}", refusal(&e)))?;
        take(budget, count.points)?;
        match spec.build(detail).map_err(|e| format!("'{what}': {}", refusal(&e)))? {
            Built::Outline(o) => pieces.push(Piece { rings: o.rings.iter().map(|r| r.iter().map(|p| place(*p)).collect()).collect::<Result<_, _>>()?, closed: true, shade: None }),
            Built::Curve(c) => {
                let mut ring: Vec<[f32; 2]> = c.points.iter().map(|p| place(*p)).collect::<Result<_, _>>()?;
                if c.closed {
                    ring.push(ring[0]);
                }
                pieces.push(Piece { rings: vec![ring], closed: false, shade: None });
            }
            _ => return Err(format!("'{what}.form': '{word}' does not build to a plane figure or a curve")),
        }
        if rotate != 0.0 {
            canon.push(("rotate", Json::from(rotate)));
        }
    } else {
        if item.get("view").is_some() == item.get("layers").is_some() {
            return Err(format!("'{what}': a solid in the 2D world is drawn in a 'view' ({}) or cut into 'layers' ({{count, axis, step}}): one of them", view_names().join(", ")));
        }
        let count = spec.count(detail).map_err(|e| format!("'{what}': {}", refusal(&e)))?;
        if count.points > MAX_ITEM_SOLID_POINTS || count.parts > MAX_DRAW_FACES {
            return Err(format!(
                "'{what}': '{word}' at detail {detail} has {} points and {} faces; a solid in the 2D world has at most {MAX_ITEM_SOLID_POINTS} points and {MAX_DRAW_FACES} faces: a lower detail",
                count.points, count.parts
            ));
        }
        let Built::Mesh(mesh) = spec.build(detail).map_err(|e| format!("'{what}': {}", refusal(&e)))? else {
            return Err(format!("'{what}.solid': '{word}' does not build to a solid"));
        };
        match (item.get("view"), item.get("layers")) {
            (Some(v), None) => {
                let name = v.as_str().filter(|n| n.len() <= MAX_WORD).unwrap_or("");
                let view = draw::view(name).ok_or_else(|| format!("'{what}.view' must be one of {}", view_names().join(", ")))?;
                let drawing = draw::draw(&mesh, view.projection).map_err(|e| format!("'{what}': {}", refusal(&e)))?;
                take(budget, drawing.polygons.iter().map(|p| p.points.len() as u64).sum())?;
                for p in &drawing.polygons {
                    pieces.push(Piece { rings: vec![p.points.iter().map(|q| place(*q)).collect::<Result<_, _>>()?], closed: true, shade: Some(p.shade as f32) });
                }
                canon.push(("view", Json::from(view.name)));
            }
            (None, Some(l)) => {
                check_keys(l, &format!("{what}.layers"), &[&["count", "axis", "step"]])?;
                let count = l.get("count").and_then(Json::as_u64).filter(|c| (1..=u64::from(MAX_ITEM_LAYERS)).contains(c)).ok_or_else(|| format!("'{what}.layers.count' must be 1..{MAX_ITEM_LAYERS}"))? as u32;
                let axis_name = l.get("axis").map_or(Some("y"), Json::as_str).unwrap_or("");
                let axis = match axis_name {
                    "x" => slice::Axis::X,
                    "y" => slice::Axis::Y,
                    "z" => slice::Axis::Z,
                    _ => return Err(format!("'{what}.layers.axis' must be x, y or z")),
                };
                let step = match l.get("step") {
                    None => [0.0, 0.0],
                    Some(s) => {
                        let v: Vec<f64> = s.as_array().map(|a| a.iter().filter_map(Json::as_f64).collect()).unwrap_or_default();
                        if v.len() != 2 || v.iter().any(|x| !x.is_finite() || x.abs() > 1e4) {
                            return Err(format!("'{what}.layers.step' must be [dx, dy], each within ±10000: how far each layer is drawn from the one under it"));
                        }
                        [v[0], v[1]]
                    }
                };
                let cuts = slice::layers(&mesh, axis, count).map_err(|e| format!("'{what}': {}", refusal(&e)))?;
                take(budget, cuts.iter().flat_map(|c| &c.rings).map(|r| r.len() as u64).sum())?;
                for (k, cut) in cuts.iter().enumerate() {
                    let shift = [step[0] * k as f64 / scale, step[1] * k as f64 / scale];
                    let shade = 0.55 + 0.45 * (k as f32 + 1.0) / count as f32;
                    for region in &cut.regions {
                        let rings = region.rings.iter().map(|r| r.iter().map(|p| place([p[0] + shift[0], p[1] + shift[1]])).collect()).collect::<Result<_, _>>()?;
                        pieces.push(Piece { rings, closed: true, shade: Some(shade) });
                    }
                }
                let mut lj: Vec<(&'static str, Json)> = vec![("count", Json::from(count)), ("axis", Json::from(axis_name))];
                if step != [0.0, 0.0] {
                    lj.push(("step", Json::arr(step.map(Json::from))));
                }
                canon.push(("layers", Json::obj(lj)));
            }
            _ => return Err(format!("'{what}': a solid in the 2D world is drawn in a 'view' ({}) or cut into 'layers' ({{count, axis, step}}): one of them", view_names().join(", "))),
        }
    }
    if at != [0.0, 0.0] {
        canon.push(("at", Json::arr(at.map(Json::from))));
    }
    if scale != 1.0 {
        canon.push(("scale", Json::from(scale)));
    }
    Ok((pieces, canon))
}

fn view_names() -> Vec<&'static str> {
    draw::views().iter().map(|v| v.name).collect()
}

// ---------------------------------------------------------------------------------------------
// The tools
// ---------------------------------------------------------------------------------------------

/// Answers a `formae_*` tool call (on the calling agent's protocol thread).
pub fn call(_session: &SessionInfo, name: &str, args: &Json) -> ToolOutput {
    let out = match name {
        "formae_list" => list_tool(args),
        "formae_get" => get_tool(args),
        "model_schema" => schema_tool(args),
        other => Err(format!("unknown formae tool '{other}'")),
    };
    match out {
        Ok(j) => ToolOutput::text(j.to_string()),
        Err(m) => ToolOutput::error(argument_error(&m).to_pretty()),
    }
}

fn kind_arg(args: &Json) -> Result<Option<Kind>, String> {
    Ok(match text(args, "kind", 16)?.as_deref() {
        None => None,
        Some("plane") => Some(Kind::Plane),
        Some("curve") => Some(Kind::Curve),
        Some("solid") => Some(Kind::Solid),
        Some(other) => return Err(format!("'kind' must be plane, curve or solid (got '{other}')")),
    })
}

/// A parameter in a few words: `radius=1 m`, `sides=6`, `angle=60 deg`, `height_ratio=1.5
/// ratio`. (It compared `Unit::name()`, which says `m` and `deg`, with `metres` and `degrees`,
/// so no row showed a unit: review 1 of stage 4, finding 3.)
fn param_short(p: &formae::Param) -> String {
    let unit = match p.unit {
        formae::Unit::Metres => " m",
        formae::Unit::Degrees => " deg",
        formae::Unit::Ratio => " ratio",
        formae::Unit::Count => "",
    };
    format!("{}={}{unit}", p.name, p.default)
}

/// What a form is pasted as: a solid's recipe shape; a plane figure's outline in an extrusion,
/// and its `shape2d` item; a curve's `shape2d` item (stroked).
fn paste(f: &formae::Form) -> Vec<(&'static str, Json)> {
    let name = Json::from(f.name);
    match f.kind {
        Kind::Solid => vec![("recipe_shape", Json::obj([("op", Json::from("form")), ("form", name)]))],
        Kind::Plane => vec![
            ("recipe_shape", Json::obj([("op", Json::from("extrude")), ("outline", Json::obj([("form", name.clone())])), ("height", Json::from(0.1))])),
            ("shape2d_item", Json::obj([("form", name), ("fill", Json::from("#8c96a3"))])),
        ],
        _ => vec![("shape2d_item", Json::obj([("form", name), ("stroke", Json::obj([("width", Json::from(0.02))]))]))],
    }
}

/// One row of the catalogue: what pastes, with the parameters' defaults (a parameter goes in
/// `args`: `{"op": "form", "form": "gear", "args": {"teeth": 12}}`).
fn row(f: &formae::Form) -> Json {
    let mut o = vec![
        ("name", Json::from(f.name)),
        ("kind", Json::from(f.kind.name())),
        ("family", Json::from(f.family.name())),
        ("about", Json::from(f.about)),
        ("args", Json::from(f.params.iter().map(param_short).collect::<Vec<_>>().join(", "))),
    ];
    o.extend(paste(f));
    Json::obj(o)
}

/// The first answer, with nothing asked: every name by kind and family, one line a family, and
/// how to ask for more. A few hundred tokens: the rows are asked for.
fn index() -> Json {
    index_of(&commonest_first(formae::forms()), formae::aliases().len())
}

/// Within each family longer than the first answer's cut, the names an agent building a game
/// reaches for first, in that order, set by hand (stage 4's review found the cut kept
/// `lune` and `bicone` and hid `stadium`, `rounded_rectangle` and `capsule`). A family's other
/// names follow in the table's order. The first answer keeps a family's first names, so these are
/// the ones a cold agent sees.
pub(crate) const COMMONEST: &[&str] = &[
    // circle
    "circle", "semicircle", "rounded_rectangle", "stadium", "annulus", "sector", "lens",
    // oval
    "ellipse", "oval", "heart", "teardrop", "superellipse", "ovoid",
    // band
    "sine_band", "zigzag_band", "battlement_band", "sawtooth_band", "catenary_band", "archimedean_spiral_band",
    // symbol
    "arrow", "cross", "shield", "banner", "chevron", "double_arrow",
    // cylinder
    "cylinder", "cone", "tube", "capsule", "rounded_cylinder", "frustum",
    // sphere
    "sphere", "hemisphere", "icosphere", "ellipsoid", "spherical_cap", "cube_sphere",
    // revolution
    "barrel", "hourglass", "spindle", "egg_of_revolution", "teardrop_of_revolution", "paraboloid",
    // prismatoid
    "prism", "pyramid", "pyramid_frustum", "bipyramid", "antiprism", "trapezohedron",
    // platonic
    "tetrahedron", "octahedron", "icosahedron", "dodecahedron",
    // archimedean
    "truncated_icosahedron", "cuboctahedron", "truncated_cube", "truncated_octahedron", "rhombicuboctahedron", "icosidodecahedron",
    // catalan (the dice among them first)
    "rhombic_dodecahedron", "rhombic_triacontahedron", "deltoidal_icositetrahedron", "pentakis_dodecahedron", "tetrakis_hexahedron", "triakis_tetrahedron",
];

/// The table's rows, each family's names reordered commonest first ([`COMMONEST`]), the families
/// in the table's order.
fn commonest_first(forms: &[formae::Form]) -> Vec<(Kind, &'static str, &'static str)> {
    let mut rows: Vec<(usize, usize, (Kind, &'static str, &'static str))> = Vec::with_capacity(forms.len());
    let mut first: Vec<(Kind, &'static str)> = Vec::new();
    for (i, f) in forms.iter().enumerate() {
        let fam = (f.kind, f.family.name());
        let at = first.iter().position(|x| *x == fam).unwrap_or_else(|| {
            first.push(fam);
            first.len() - 1
        });
        let rank = COMMONEST.iter().position(|n| *n == f.name).unwrap_or(COMMONEST.len() + i);
        rows.push((at, rank, (f.kind, fam.1, f.name)));
    }
    rows.sort_by_key(|r| (r.0, r.1));
    rows.into_iter().map(|r| r.2).collect()
}

/// A kind's families in the table's order, each with its forms' names.
type Families<'a> = Vec<(&'a str, Vec<&'a str>)>;

/// The first answer for a table of `(kind, family, name)` rows (the library's, or a test's),
/// always under [`MAX_INDEX_BYTES`] however large the table grows. While it fits, every name,
/// one line a family (`"block: box wedge"`). When it does not, each family's line keeps its
/// first names, as many as fit for every family alike, and says how many it has and how many
/// more there are (`"archimedean (13): truncated_tetrahedron +12 more"`); a family's rows are
/// one call away. When not even a line a family fits, a line a kind says how many forms and
/// families it has (`"81 forms in 14 families: ..."`), which is a few dozen bytes a kind.
fn index_of(rows: &[(Kind, &str, &str)], aliases: usize) -> Json {
    // (Each kind's families in the table's order, wherever their forms stand in it.)
    let mut kinds: Vec<(Kind, Families)> = Vec::new();
    for kind in [Kind::Plane, Kind::Curve, Kind::Solid] {
        let mut families: Vec<(&str, Vec<&str>)> = Vec::new();
        for &(_, family, name) in rows.iter().filter(|r| r.0 == kind) {
            match families.iter_mut().find(|f| f.0 == family) {
                Some(f) => f.1.push(name),
                None => families.push((family, vec![name])),
            }
        }
        kinds.push((kind, families));
    }
    let rest = |out: &mut Vec<(&'static str, Json)>| {
        out.push(("aliases", Json::from(format!("{aliases} more words name these (cube, ball, donut, hexagon, pill...): formae_get takes them too"))));
        out.push(("figure", Json::from("the measured human body: {\"op\": \"figure\"} as a recipe shape, or \"rig\": {\"figure\": {...}}; formae_list {\"figures\": true} gives its fields")));
        out.push((
            "ask",
            Json::from("formae_get {\"name\": \"gear\"}: one form's parameters and snippets to paste. formae_list {\"family\": \"gear\"} or {\"word\": \"star\"}: rows to paste. A solid is a recipe shape {\"op\": \"form\", \"form\": NAME, \"args\": {...}}; a plane figure an outline {\"form\": NAME} (extrude, loft, lathe, sweep) or a shape2d item."),
        ));
    };
    // `shown`: the names a family's line keeps (`None`: all of them).
    let answer = |shown: Option<usize>| {
        let mut out: Vec<(&'static str, Json)> = Vec::new();
        for (kind, families) in &kinds {
            let lines = families.iter().map(|(family, names)| match shown {
                Some(k) if names.len() > k => {
                    let mut line = format!("{family} ({}):", names.len());
                    for n in &names[..k] {
                        line.push(' ');
                        line.push_str(n);
                    }
                    Json::from(format!("{line} +{} more", names.len() - k))
                }
                _ => Json::from(format!("{family}: {}", names.join(" "))),
            });
            out.push((kind.name(), Json::arr(lines)));
        }
        rest(&mut out);
        Json::obj(out)
    };
    let fits = |j: &Json| j.to_string().len() <= MAX_INDEX_BYTES;
    let all = answer(None);
    if fits(&all) {
        return all;
    }
    let widest = kinds.iter().flat_map(|k| k.1.iter().map(|f| f.1.len())).max().unwrap_or(0);
    for k in (0..widest).rev() {
        let cut = answer(Some(k));
        if fits(&cut) {
            return cut;
        }
    }
    let mut out: Vec<(&'static str, Json)> = Vec::new();
    for (kind, families) in &kinds {
        let forms: usize = families.iter().map(|f| f.1.len()).sum();
        out.push((kind.name(), Json::arr([Json::from(format!("{forms} forms in {} families: formae_list {{\"kind\": \"{}\", \"rows\": true}} gives their rows", families.len(), kind.name()))])));
    }
    rest(&mut out);
    Json::obj(out)
}

/// What starts from the measured human figure, read from the modeller's own field tables.
fn figures() -> Json {
    let fields = |shape: &str| Json::arr(specula_model::shape_fields(shape).unwrap_or_default().into_iter().map(|(n, req, takes)| Json::obj([("name", Json::from(n)), ("required", Json::from(req)), ("takes", Json::from(takes))])));
    Json::arr([Json::obj([
        ("name", Json::from("figure")),
        ("what", Json::from("a measured human body (specula-corpus): a sex, a stature, a weight and named traits give its surface and its skeleton (docs/design/FIGURE.md)")),
        ("as_shape", Json::from("{\"op\": \"figure\", ...}: a recipe part's shape")),
        ("as_rig", Json::from("\"rig\": {\"figure\": {...}}: the same fields give the recipe's skeleton, eighteen bones that the motion tools know by name")),
        ("fields", fields("figure")),
    ])])
}

fn list_tool(args: &Json) -> Result<Json, String> {
    check_keys(args, "formae_list", &[&["kind", "family", "word", "cursor", "limit", "rows", "figures"]])?;
    let flag = |key: &str| -> Result<bool, String> {
        match args.get(key) {
            None | Some(Json::Null) => Ok(false),
            Some(v) => v.as_bool().ok_or_else(|| format!("'{key}' is true or false")),
        }
    };
    let (rows_asked, figures_asked) = (flag("rows")?, flag("figures")?);
    let filtered = ["kind", "family", "word", "cursor", "limit"].iter().any(|k| args.get(k).is_some_and(|v| !v.is_null()));
    if !filtered && !rows_asked {
        let mut out = index();
        if figures_asked {
            out.set("figures", figures());
        }
        return Ok(out);
    }
    let kind = kind_arg(args)?;
    let family = text(args, "family", 32)?;
    let word = text(args, "word", MAX_WORD)?.map(|w| w.to_ascii_lowercase());
    let limit = int_arg(args, "limit", 1, MAX_LIST as u64)?.map_or(DEFAULT_LIST, |l| l as usize);    let cursor = match text(args, "cursor", 16)? {
        None => 0,
        Some(c) => c.parse::<usize>().map_err(|_| "'cursor' is the next_cursor of the page before".to_string())?,
    };
    if let Some(f) = &family {
        if !formae::forms().iter().any(|x| x.family.name() == f.as_str()) {
            let mut all: Vec<&str> = formae::forms().iter().map(|x| x.family.name()).collect();
            all.dedup();
            return Err(format!("no family is called '{f}': {}", all.join(", ")));
        }
    }
    let matches = |f: &formae::Form| {
        kind.is_none_or(|k| f.kind == k)
            && family.as_deref().is_none_or(|fam| f.family.name() == fam)
            && word.as_deref().is_none_or(|w| f.name.contains(w) || f.about.to_ascii_lowercase().contains(w) || formae::aliases().iter().any(|a| a.of == f.name && a.name.contains(w)))
    };
    let all: Vec<&formae::Form> = formae::forms().iter().filter(|f| matches(f)).collect();
    let mut rows = Vec::new();
    let mut bytes = 0usize;
    let mut next = None;
    for (i, f) in all.iter().enumerate().skip(cursor) {
        let r = row(f);
        let n = r.to_string().len();
        if rows.len() >= limit || bytes + n > MAX_ROWS_BYTES {
            next = Some(i);
            break;
        }
        bytes += n;
        rows.push(r);
    }
    let mut out = vec![("forms", Json::arr(rows)), ("matched", Json::from(all.len())), ("of", Json::from(formae::forms().len()))];
    if figures_asked {
        out.push(("figures", figures()));
    }
    out.push(("next", Json::from("a parameter goes in \"args\" ({\"op\": \"form\", \"form\": \"gear\", \"args\": {\"teeth\": 12}}); formae_get {\"name\": NAME} for one form's ranges, the raised and drawn forms, and with \"full\": true its measures and anchors")));
    if let Some(n) = next {
        out.push(("next_cursor", Json::from(n.to_string())));
    }
    Ok(Json::obj(out))
}

fn measures_json(m: &formae::Measures) -> Json {
    let mut f: Vec<(&'static str, Json)> = Vec::new();
    if let Some(s) = m.size {
        f.push(("size", Json::from(s)));
    }
    if let Some(b) = m.boundary {
        f.push(("boundary", Json::from(b)));
    }
    if let Some(c) = m.centroid {
        f.push(("centroid", Json::arr(c.map(Json::from))));
    }
    if let Some((lo, hi)) = m.bounds {
        f.push(("bounds", Json::arr([Json::arr(lo.map(Json::from)), Json::arr(hi.map(Json::from))])));
    }
    if let Some(s) = m.series {
        f.push(("series", Json::from(s)));
    }
    Json::obj(f)
}

fn get_tool(args: &Json) -> Result<Json, String> {
    check_keys(args, "formae_get", &[&["name", "kind", "args", "detail", "view", "full"]])?;
    let name = text(args, "name", MAX_WORD)?.ok_or("'name' (a form's name or alias) is required")?;
    let kind = kind_arg(args)?;
    let found = look_up(&name, kind)?;
    let given = given(&found, args.get("args"), "formae_get")?;
    let detail = detail_of(args.get("detail"), "formae_get")?;
    let view_name = text(args, "view", 32)?.unwrap_or_else(|| "isometric".to_string());
    let view = draw::view(&view_name).ok_or_else(|| format!("'view' must be one of {}", view_names().join(", ")))?;
    let f = found.form;
    let full = match args.get("full") {
        None | Some(Json::Null) => false,
        Some(v) => v.as_bool().ok_or("'full' is true or false")?,
    };
    if !full {
        return short_answer(found, &given, detail, &view);
    }
    let params: Vec<Json> = formae::params_of(&found)
        .iter()
        .map(|p| {
            let mut o = vec![("name", Json::from(p.name)), ("about", Json::from(p.about)), ("unit", Json::from(p.unit.name())), ("default", Json::from(p.default)), ("min", Json::from(p.min)), ("max", Json::from(p.max))];
            if p.integer {
                o.push(("integer", Json::from(true)));
            }
            if let Some(g) = given.iter().find(|g| g.0 == p.name) {
                o.push(("given", Json::from(g.1)));
            }
            Json::obj(o)
        })
        .collect();
    let mut out: Vec<(&'static str, Json)> = vec![
        ("name", Json::from(f.name)),
        ("kind", Json::from(f.kind.name())),
        ("family", Json::from(f.family.name())),
        ("about", Json::from(f.about)),
        ("origin", Json::from(f.origin)),
        ("curved", Json::from(f.curved)),
        ("params", Json::arr(params)),
        ("detail", Json::obj([("asked", Json::from(detail)), ("min", Json::from(formae::DETAIL_MIN)), ("max", Json::from(formae::DETAIL_MAX)), ("default", Json::from(formae::DETAIL_DEFAULT))])),
        ("kin", Json::arr(f.kin.iter().map(|k| Json::from(*k)))),
    ];
    if let Some(a) = found.alias {
        out.push(("alias", Json::obj([("word", Json::from(a.name)), ("of", Json::from(a.of)), ("about", Json::from(a.about))])));
    }
    if let Some(b) = f.band {
        out.push(("band", Json::from(b)));
    }
    let spec = match Spec::of(found, &given) {
        Ok(s) => s,
        Err(e) => {
            out.push(("refused", Json::from(refusal(&e))));
            return Ok(Json::obj(out));
        }
    };
    if let Some(g) = spec.genus() {
        out.push(("genus", Json::from(g)));
    }
    let count = spec.count(detail).map_err(|e| refusal(&e))?;
    out.push(("count", Json::obj([("points", Json::from(count.points)), ("parts", Json::from(count.parts))])));
    if let Ok(Some(e)) = spec.error(detail) {
        out.push(("error_at_detail_m", Json::from(e)));
    }
    if let Ok(m) = spec.closed() {
        out.push(("closed", measures_json(&m)));
    }
    let built = if count.points > MAX_GET_POINTS {
        out.push(("built", Json::obj([("not_built", Json::from(format!("{} points at detail {detail}; formae_get builds at most {MAX_GET_POINTS}: ask at a lower detail", count.points)))])));
        None
    } else {
        match spec.build(detail) {
            Ok(b) => {
                let mut m = vec![("measures", measures_json(&b.measures())), ("chord_error_m", Json::from(b.chord_error()))];
                if let Built::Mesh(mesh) = &b {
                    m.push(("faceting", Json::arr(mesh.rounds().iter().map(|r| Json::obj([("turn_deg", Json::from(r.turn_deg)), ("radius_m", Json::from(r.radius))])))));
                }
                out.push(("built", Json::obj(m)));
                Some(b)
            }
            Err(e) => {
                out.push(("built", Json::obj([("refused", Json::from(refusal(&e)))])));
                None
            }
        }
    };
    if let Ok(anchors) = spec.anchors() {
        out.push((
            "anchors",
            Json::arr(anchors.iter().take(MAX_ANCHORS).map(|a| {
                let mut o = vec![("name", Json::from(a.name.as_str())), ("at", Json::arr(a.at.map(Json::from)))];
                if let Some(d) = a.dir {
                    o.push(("dir", Json::arr(d.map(Json::from))));
                }
                Json::obj(o)
            })),
        ));
    }
    out.push(("snippets", snippets(&spec, &found, &given, detail, &view, built.as_ref())));
    let answer = Json::obj(out);
    if answer.to_string().len() > MAX_ANSWER_BYTES {
        return Err(format!("the answer for '{}' would pass {MAX_ANSWER_BYTES} bytes: ask at a lower detail", f.name));
    }
    Ok(answer)
}

/// `formae_get`'s answer unless `full` is asked: the snippets first, then the parameters in a
/// line each (name, default and unit, range), the count at the detail, and what `full` adds.
/// Nothing is built: the snippets are the name and the numbers.
fn short_answer(found: Found, given: &[(&'static str, f64)], detail: u32, view: &draw::View) -> Result<Json, String> {
    let f = found.form;
    let spec = match Spec::of(found, given) {
        Ok(s) => s,
        Err(e) => return Ok(Json::obj([("name", Json::from(f.name)), ("refused", Json::from(refusal(&e)))])),
    };
    let count = spec.count(detail).map_err(|e| refusal(&e))?;
    let mut out: Vec<(&'static str, Json)> = vec![
        ("name", Json::from(spec.name())),
        ("about", Json::from(f.about)),
        ("snippets", snippets(&spec, &found, given, detail, view, None)),
        (
            "args",
            Json::arr(formae::params_of(&found).iter().map(|p| {
                let given = given.iter().find(|g| g.0 == p.name).map_or(String::new(), |g| format!(", given {}", g.1));
                Json::from(format!("{} ({}..{}{}){given}: {}", param_short(p), p.min, p.max, if p.integer { ", whole" } else { "" }, p.about))
            })),
        ),
    ];
    if let Some(a) = found.alias {
        out.push(("alias_of", Json::from(a.of)));
    }
    if f.curved {
        out.push(("detail", Json::from(format!("{detail}: segments in a full turn, {}..{}", formae::DETAIL_MIN, formae::DETAIL_MAX))));
    }
    out.push(("count", Json::from(format!("{} points, {} parts", count.points, count.parts))));
    out.push(("full", Json::from("\"full\": true adds the measures in closed form and as built, the error at this detail, the anchors, the kin and a drawing")));
    let answer = Json::obj(out);
    // (Its ceiling: every word's short answer is held under it by a test.)
    if answer.to_string().len() > MAX_SHORT_BYTES {
        return Err(format!("the short answer for '{}' would pass {MAX_SHORT_BYTES} bytes: ask with \"full\": true", f.name));
    }
    Ok(answer)
}

/// Snippets ready to paste: the recipe part, the recipe outline, the shape2d item, the raised
/// forms of a plane figure, a solid drawn in a view.
fn snippets(spec: &Spec, found: &Found, given: &[(&'static str, f64)], detail: u32, view: &draw::View, built: Option<&Built>) -> Json {
    let name = spec.name();
    let args = args_json(found, given);
    let base = |extra: Vec<(&'static str, Json)>| {
        let mut o = vec![("form", Json::from(name))];
        if !given.is_empty() {
            o.push(("args", args.clone()));
        }
        o.push(("detail", Json::from(detail)));
        o.extend(extra);
        Json::obj(o)
    };
    let mut s: Vec<(&'static str, Json)> = Vec::new();
    match found.form.kind {
        Kind::Solid => {
            let mut part = vec![("op", Json::from("form")), ("form", Json::from(name))];
            if !given.is_empty() {
                part.push(("args", args.clone()));
            }
            part.push(("detail", Json::from(detail)));
            s.push(("recipe_shape", Json::obj(part)));
            let item = |extra: (&'static str, Json)| {
                let mut o = vec![("solid", Json::from(name))];
                if !given.is_empty() {
                    o.push(("args", args.clone()));
                }
                o.push(("detail", Json::from(detail)));
                o.push(extra);
                o.push(("fill", Json::from("#8c96a3")));
                Json::obj(o)
            };
            s.push(("shape2d_drawn", Json::obj([("items", Json::arr([item(("view", Json::from(view.name)))]))])));
            s.push(("shape2d_layers", Json::obj([("items", Json::arr([item(("layers", Json::obj([("count", Json::from(8)), ("axis", Json::from("y")), ("step", Json::arr([Json::from(0.0), Json::from(0.02)]))])))]))])));
            if let Some(Built::Mesh(mesh)) = built {
                if (mesh.faces.len() as u64) <= MAX_DRAW_FACES {
                    match draw::draw(mesh, view.projection) {
                        Ok(d) => s.push(("drawing", Json::obj([("view", Json::from(view.name)), ("polygons", Json::from(d.polygons.len())), ("convex", Json::from(d.convex))]))),
                        Err(e) => s.push(("drawing", Json::obj([("refused", Json::from(refusal(&e)))]))),
                    }
                }
            }
        }
        Kind::Plane => {
            s.push(("recipe_outline", base(Vec::new())));
            s.push(("recipe_raised", Json::obj([("op", Json::from("form")), ("form", Json::from(name)), ("detail", Json::from(detail)), ("raise", Json::obj([("as", Json::from("slab")), ("thickness", Json::from(0.1))]))])));
            s.push(("raised_as", Json::arr(formae::raise::RAISED.iter().map(|r| Json::from(*r)))));
            s.push(("shape2d_item", Json::obj([("items", Json::arr([base(vec![("fill", Json::from("#8c96a3"))])]))])));
        }
        _ => {
            let mut o = vec![("form", Json::from(name))];
            if !given.is_empty() {
                o.push(("args", args.clone()));
            }
            o.push(("detail", Json::from(detail)));
            o.push(("stroke", Json::obj([("width", Json::from(0.02)), ("color", Json::from("#8c96a3"))])));
            s.push(("shape2d_item", Json::obj([("items", Json::arr([Json::obj(o)]))])));
            if let Some(b) = found.form.band {
                s.push(("band", Json::from(format!("'{b}' is this curve with a width: a plane figure, to fill, raise or extrude"))));
            }
        }
    }
    Json::obj(s)
}

pub fn definitions() -> Vec<ToolDef> {
    let s = |max: usize, d: &str| Json::obj([("type", Json::from("string")), ("maxLength", Json::from(max)), ("description", Json::from(d))]);
    let int = |lo: u64, hi: u64, d: &str| Json::obj([("type", Json::from("integer")), ("minimum", Json::from(lo)), ("maximum", Json::from(hi)), ("description", Json::from(d))]);
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::arr(required.iter().map(|r| Json::from(*r))))]);
    let kind = || s(16, "plane, curve or solid.");
    vec![
        ToolDef {
            name: "formae_list".into(),
            description: format!(
                "The shape library: named plane figures, curves and solids to build from by name and change by numbers (a gear, a heart, \
                 an arch, a star, a torus, a gem, a capsule...), cheaper than writing their points. With nothing asked: every name by kind \
                 and family on a few lines (under {MAX_INDEX_BYTES} bytes) and how to ask for more. With kind, family or word (in a name, \
                 an alias or its line), or rows: true: rows that paste (the recipe shape, a plane figure's outline in an extrusion and its \
                 shape2d item), with each parameter's default; at most {MAX_LIST} rows and {MAX_ROWS_BYTES} bytes a page, next_cursor for \
                 the next. figures: true adds the measured human figure's fields (the modeller's own). Read-only."
            ),
            input_schema: schema(
                vec![
                    ("kind", kind()),
                    ("family", s(32, "Only this family (triangle, polygon, circle, arch, spiral, platonic, archimedean, catalan, sphere, torus, sweep...: formae_list with nothing asked names them).")),
                    ("word", s(MAX_WORD, "Only forms whose name, an alias or its line has this word.")),
                    ("cursor", s(16, "next_cursor of the page before.")),
                    ("limit", int(1, MAX_LIST as u64, "Rows a page (default 20).")),
                    ("rows", Json::obj([("type", Json::from("boolean")), ("description", Json::from("Rows of every form, paged, with no filter."))])),
                    ("figures", Json::obj([("type", Json::from("boolean")), ("description", Json::from("Add the measured human figure's fields."))])),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "formae_get".into(),
            description: format!(
                "One form of the shape library by its name or an alias, short: snippets to paste first (the recipe shape or outline, a \
                 plane figure raised, the shape2d item, a solid drawn in a view or cut into layers), with the arguments given (defaults \
                 otherwise) and the detail (segments in a full turn, {}..{}); then each parameter's default, unit and range, and the \
                 point count. full: true adds its measures in closed form and as built, the error at that detail, its anchors, its kin \
                 in the other dimensions and a drawing (it builds at most {MAX_GET_POINTS} points). An unknown name answers with the \
                 nearest. Read-only.",
                formae::DETAIL_MIN,
                formae::DETAIL_MAX
            ),
            input_schema: schema(
                vec![
                    ("name", s(MAX_WORD, "The form's name or an alias (heart, cube, donut...).")),
                    ("kind", kind()),
                    ("args", Json::obj([("type", Json::from("object")), ("maxProperties", Json::from(MAX_ARGS)), ("description", Json::from("The form's parameters by name, numbers (formae_get with no args lists them)."))])),
                    ("detail", int(u64::from(formae::DETAIL_MIN), u64::from(formae::DETAIL_MAX), "Segments in a full turn (default 64).")),
                    ("view", s(32, "The view a solid's drawing snippet uses (default isometric): top, front, side, isometric, game_isometric, dimetric, cavalier, cabinet.")),
                    ("full", Json::obj([("type", Json::from("boolean")), ("description", Json::from("Add the measures, the error, the anchors, the kin and a drawing."))])),
                ],
                &["name"],
            ),
        },
        ToolDef {
            name: "model_schema".into(),
            description: format!(
                "How a model recipe is written, from the table the builder itself reads. With nothing asked: the whole format in one \
                 short answer (under {} bytes): what a recipe file is and where it goes, a recipe's fields, a part's fields, every \
                 shape, operation and generator on a line each (what it needs, then what it may take), and a whole recipe that builds, \
                 to start from. With of: a shape's, an operation's or a generator's name (or outline, part, recipe): every field of \
                 that one thing with what each takes. Ask this before writing a recipe: a wrong field costs a refused build. Read-only.",
                specula_model::SCHEMA_FIRST_BYTES
            ),
            input_schema: schema(vec![("of", s(MAX_WORD, "One shape, operation or generator by name (extrude, sdf, form, rotate, scatter...), or outline, part, recipe. Left out: the whole format, short."))], &[]),
        },
    ]
}

/// `model_schema`: a recipe's format ([`specula_model::recipe_schema`]).
fn schema_tool(args: &Json) -> Result<Json, String> {
    let of = text(args, "of", MAX_WORD)?;
    specula_model::recipe_schema(of.as_deref())
}

#[cfg(test)]
mod tests;
