//! The shape library through the modeller (docs/design/PROCEDURAL_MODELLING.md, "Shapes by
//! name"): every solid word as a `form` shape, every one-ring plane figure as an outline by name,
//! every plane figure raised; a form's numbers as expressions; refusals as data, with the names;
//! the floor in `f32`, with arguments drawn at their corners together.

use std::collections::BTreeMap;

use specula_formae::{aliases, find, forms, params_of, Kind, Param};
use specula_model::{build, BuildParams, ErrorCode, Model, RecipeError, Style};

fn style() -> Style {
    Style::parse(r##"{ "style": "t", "palette": { "dark": "#202020" }, "materials": { "stone": { "color": "#8c96a3", "roughness": 0.8 } } }"##).unwrap()
}

fn try_make(text: &str) -> Result<Model, RecipeError> {
    let s = style();
    let lib = BTreeMap::new();
    build(text, &BuildParams { library: Some(&lib), ..BuildParams::new(&s) })
}

fn recipe(shape: &str) -> String {
    format!(r#"{{ "name": "t", "params": {{ "k": {{ "default": 1, "min": 0.1, "max": 10 }} }}, "parts": [ {{ "name": "p", "material": "stone", "shape": {shape} }} ] }}"#)
}

/// The model's finest level is a mesh the modeller accepts, closed.
fn sound(m: &Model, what: &str) {
    let l = &m.lods[0];
    l.validate().unwrap_or_else(|er| panic!("{what}: {er:?}"));
    assert!(l.is_watertight(), "{what}: not watertight");
}

/// A JSON object of arguments.
fn args(given: &[(&str, f64)]) -> String {
    let inner: Vec<String> = given.iter().map(|(k, v)| format!("\"{k}\": {v:e}")).collect();
    format!("{{{}}}", inner.join(", "))
}

fn solid_words() -> Vec<&'static str> {
    let mut w: Vec<&'static str> = forms().iter().filter(|f| f.kind == Kind::Solid).map(|f| f.name).collect();
    w.extend(aliases().iter().filter(|a| a.kind == Kind::Solid).map(|a| a.name));
    w
}

/// Every solid word, as given, builds through a recipe into a closed mesh the modeller accepts,
/// recorded as what it is made of; a curved one leaves a faceting record of its own.
#[test]
fn every_solid_builds_as_a_form_shape() {
    let mut built = 0;
    for w in solid_words() {
        let text = recipe(&format!(r#"{{"op": "form", "form": "{w}", "detail": 16}}"#));
        let m = match try_make(&text) {
            Ok(m) => m,
            // (A refusal as data, never a panic: the limits of a part, at its defaults.)
            Err(er) if matches!(er.code, ErrorCode::Limit | ErrorCode::Geometry) => continue,
            Err(er) => panic!("{w}: {er}"),
        };
        sound(&m, w);
        built += 1;
        assert!(m.extras.made.iter().any(|x| x.path == "parts[0].shape"), "{w}: not recorded");
        let found = find(w, Some(Kind::Solid)).unwrap();
        if found.form.curved {
            assert!(m.extras.round.iter().any(|r| r.path == "parts[0].shape" && r.turn_deg > 0.0 && r.radius_m > 0.0), "{w}: no faceting record");
        }
    }
    assert!(built > 140, "only {built} solid words built");
}

/// Every one-ring plane figure as an outline by name extrudes into a closed mesh; a curved one
/// leaves a faceting record at its outline; one with holes is refused with words that say what
/// to do instead, and is raised as a slab by the form shape, holes and all.
#[test]
fn every_plane_figure_extrudes_or_is_raised() {
    let (mut extruded, mut holes) = (0, 0);
    for f in forms().iter().filter(|f| f.kind == Kind::Plane) {
        let text = recipe(&format!(r#"{{"op": "extrude", "outline": {{"form": "{}", "detail": 24}}, "height": 0.2}}"#, f.name));
        match try_make(&text) {
            Ok(m) => {
                sound(&m, f.name);
                extruded += 1;
                if f.curved {
                    assert!(m.extras.round.iter().any(|r| r.path == "parts[0].shape" && r.what == f.name && r.count == "detail" && r.turn_deg > 0.0), "{}: no faceting record", f.name);
                }
            }
            Err(er) if er.message.contains("holes") => {
                holes += 1;
                let slab = recipe(&format!(r#"{{"op": "form", "form": "{}", "detail": 24, "raise": {{"as": "slab", "thickness": 0.2}}}}"#, f.name));
                sound(&try_make(&slab).unwrap_or_else(|er| panic!("{} as a slab: {er}", f.name)), f.name);
            }
            Err(er) => panic!("{}: {er}", f.name),
        }
    }
    assert!(extruded > 55 && holes >= 3, "{extruded} extruded, {holes} with holes");
    // Each raised form, on an ellipse.
    for raise in [r#"{"as": "slab", "thickness": 0.1}"#, r#"{"as": "bevelled", "thickness": 0.1, "width": 0.003, "round": true}"#, r#"{"as": "pillow", "thickness": 0.05, "rise": 0.05, "inset": 0.1}"#, r#"{"as": "stepped", "steps": 3, "rise": 0.03, "run": 0.02}"#, r#"{"as": "relief", "height": 0.05, "margin": 0.1, "plate": 0.02}"#] {
        let m = try_make(&recipe(&format!(r#"{{"op": "form", "form": "ellipse", "detail": 32, "raise": {raise}}}"#))).unwrap_or_else(|er| panic!("{raise}: {er}"));
        sound(&m, raise);
    }
}

/// A form's numbers are expressions of the recipe's parameters: a ball of radius `0.5 * k` built
/// with k = 2 is the same as one of radius 1.
#[test]
fn a_forms_numbers_are_expressions() {
    let s = style();
    let lib = BTreeMap::new();
    let by_k = recipe(r#"{"op": "form", "form": "sphere", "args": {"radius": "0.5 * k"}, "detail": "8 * k"}"#);
    let plain = recipe(r#"{"op": "form", "form": "sphere", "args": {"radius": 1}, "detail": 16}"#);
    let a = build(&by_k, &BuildParams { library: Some(&lib), values: &[("k", 2.0)], ..BuildParams::new(&s) }).unwrap();
    let b = try_make(&plain).unwrap();
    assert_eq!(a.lods[0].positions, b.lods[0].positions);
    let out = recipe(r#"{"op": "extrude", "outline": {"form": "regular_polygon", "args": {"sides": "3 + k", "radius": 0.5}, "rotate": 15, "scale": [1, 2]}, "height": 0.1}"#);
    sound(&try_make(&out).unwrap(), "a pentagon by an expression");
}

fn refused(text: &str) -> RecipeError {
    match try_make(text) {
        Ok(_) => panic!("built: {text}"),
        Err(er) => er,
    }
}

/// Refusals as data, each with what to do: an unknown word (with the nearest), an unknown
/// parameter (with the form's list), a plane figure as a shape with no raise, a raise's field
/// it does not take or lacks, a solid or a curve as an outline, a field beside a form that would
/// do nothing, a number out of the form's range, a count over the outline's limit.
#[test]
fn what_a_form_refuses() {
    let er = refused(&recipe(r#"{"op": "form", "form": "icosahedrn"}"#));
    assert!(er.code == ErrorCode::UnknownName && er.message.contains("icosahedron"), "{er}");
    let er = refused(&recipe(r#"{"op": "form", "form": "torus", "args": {"radius": 1, "tub": 0.2}}"#));
    assert!(er.code == ErrorCode::UnknownField && er.path.ends_with("args.tub") && er.message.contains("tube"), "{er}");
    let er = refused(&recipe(r#"{"op": "form", "form": "heart"}"#));
    assert!(er.code == ErrorCode::MissingField && er.message.contains("raise"), "{er}");
    let er = refused(&recipe(r#"{"op": "form", "form": "heart", "raise": {"as": "slab", "thickness": 0.1, "rise": 1}}"#));
    assert!(er.code == ErrorCode::UnknownField && er.path.ends_with("raise.rise"), "{er}");
    let er = refused(&recipe(r#"{"op": "form", "form": "heart", "raise": {"as": "pillow", "thickness": 0.1}}"#));
    assert!(er.code == ErrorCode::MissingField, "{er}");
    let er = refused(&recipe(r#"{"op": "extrude", "outline": {"form": "sphere"}, "height": 1}"#));
    assert!(er.message.contains("solid"), "{er}");
    let er = refused(&recipe(r#"{"op": "extrude", "outline": {"form": "sine_wave"}, "height": 1}"#));
    assert!(er.message.contains("open curve"), "{er}");
    let er = refused(&recipe(r#"{"op": "extrude", "outline": {"form": "circle", "radius": 2}, "height": 1}"#));
    assert!(er.code == ErrorCode::Geometry && er.path.ends_with("radius"), "{er}");
    let er = refused(&recipe(r#"{"op": "extrude", "outline": {"rect": [1, 1], "detail": 9}, "height": 1}"#));
    assert!(er.path.ends_with("detail"), "{er}");
    let er = refused(&recipe(r#"{"op": "form", "form": "sphere", "args": {"radius": 1e6}}"#));
    assert!(er.code == ErrorCode::OutOfRange && er.measured == Some(1e6), "{er}");
    let er = refused(&recipe(r#"{"op": "extrude", "outline": {"form": "circle", "detail": 4096}, "height": 1}"#));
    assert!(er.code == ErrorCode::Limit, "{er}");
    let er = refused(&recipe(r#"{"op": "form", "form": "sphere", "detail": 4096}"#));
    assert!(er.code == ErrorCode::Limit, "{er}");
}

/// The floor in `f32`: a torus whose tube is a ten-millionth of its reach builds in the library
/// and is refused here with its numbers; one at a thousandth builds.
#[test]
fn the_f32_floor_is_refused_as_data() {
    let er = refused(&recipe(r#"{"op": "form", "form": "torus", "args": {"radius": 1000, "tube": 0.0001}, "detail": 8}"#));
    assert!(er.code == ErrorCode::Geometry && er.measured.is_some() && er.limit.is_some_and(|l| l > er.measured.unwrap()), "{er}");
    sound(&try_make(&recipe(r#"{"op": "form", "form": "torus", "args": {"radius": 10, "tube": 0.01}, "detail": 8}"#)).unwrap(), "a thin torus");
}

/// xorshift64*.
struct Seq(u64);
impl Seq {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn corner(p: &Param, s: &mut Seq) -> f64 {
    let v = match (s.next() * 5.0) as u32 {
        0 => p.min,
        1 => p.max,
        2 => p.default,
        _ => p.min + (p.max - p.min) * s.next(),
    };
    if p.integer {
        specula_core::dmath::floor_f64(v)
    } else {
        v
    }
}

/// Every solid word with its arguments drawn at their corners together (least, greatest,
/// default or anywhere), at details 8, 9 and 30: each builds into a closed mesh the modeller
/// accepts, or is refused as data (a range, a limit, the library's or the floor's geometry);
/// never a panic, never another kind of error. Every refusal by the floor names its numbers.
fn corners(sets: usize) {
    let (mut built, mut floor) = (0usize, 0usize);
    for w in solid_words() {
        let found = find(w, Some(Kind::Solid)).unwrap();
        let params = params_of(&found);
        let mut s = Seq(w.bytes().fold(0x9E37_79B9_7F4A_7C15u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)) | 1);
        for _ in 0..sets {
            let given: Vec<(&str, f64)> = params.iter().map(|p| (p.name, corner(p, &mut s))).collect();
            for detail in [8u32, 9, 30] {
                let text = recipe(&format!(r#"{{"op": "form", "form": "{w}", "args": {}, "detail": {detail}}}"#, args(&given)));
                match try_make(&text) {
                    Ok(m) => {
                        sound(&m, &text);
                        built += 1;
                    }
                    Err(er) => {
                        assert!(matches!(er.code, ErrorCode::OutOfRange | ErrorCode::Limit | ErrorCode::Geometry), "{text}: {er}");
                        if er.message.contains("f32") {
                            assert!(er.measured.is_some(), "{text}: {er}");
                            floor += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(built > 100 && floor > 0, "{built} built, {floor} refused by the floor");
}

#[test]
fn solids_at_their_corners_build_or_are_refused_as_data() {
    corners(2);
}

#[test]
#[ignore = "eight corner draws of every solid word through the modeller: minutes in the gate's build"]
fn solids_at_their_corners_full() {
    corners(8);
}

/// Every plane word (names and aliases) as an outline by name, its arguments drawn at their
/// corners together, at details 8, 9, 24 and 64, extruded by a height drawn from a millimetre to
/// fifty metres: each builds into a mesh the modeller accepts and its own `is_watertight` holds,
/// or is refused as data. Never a model that opens when the modeller finishes it (the review of
/// stage 2: a semicircle of radius 0.1 mm at detail 24 extruded by a millimetre built open).
fn outlines_at_corners(sets: usize) {
    let mut words: Vec<&'static str> = forms().iter().filter(|f| f.kind == Kind::Plane).map(|f| f.name).collect();
    words.extend(aliases().iter().filter(|a| a.kind == Kind::Plane).map(|a| a.name));
    let (mut built, mut opened) = (0usize, 0usize);
    for w in words {
        let found = find(w, Some(Kind::Plane)).unwrap();
        let params = params_of(&found);
        let mut s = Seq(w.bytes().fold(0xC2B2_AE3D_27D4_EB4Fu64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)) | 1);
        for _ in 0..sets {
            let given: Vec<(&str, f64)> = params.iter().map(|p| (p.name, corner(p, &mut s))).collect();
            let height = [1e-3, 0.2, 50.0][(s.next() * 3.0) as usize % 3];
            for detail in [8u32, 9, 24, 64] {
                let text = recipe(&format!(r#"{{"op": "extrude", "outline": {{"form": "{w}", "args": {}, "detail": {detail}}}, "height": {height:e}}}"#, args(&given)));
                match try_make(&text) {
                    Ok(m) => {
                        sound(&m, &text);
                        built += 1;
                    }
                    Err(er) => {
                        assert!(matches!(er.code, ErrorCode::OutOfRange | ErrorCode::Limit | ErrorCode::Geometry), "{text}: {er}");
                        if er.message.contains("not closed") {
                            assert!(er.measured.is_some() && er.limit.is_some(), "{text}: {er}");
                            opened += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(built > 200, "{built} built, {opened} refused as opening");
}

#[test]
fn outlines_at_their_corners_build_closed_or_are_refused() {
    outlines_at_corners(2);
}

#[test]
#[ignore = "eight corner draws of every plane word as an extruded outline: minutes in the gate's build"]
fn outlines_at_their_corners_full() {
    outlines_at_corners(8);
}

/// The review's cases of an outline by name that built open: each is now refused as data with
/// its numbers (the least triangle against the modeller's), or builds closed.
#[test]
fn the_reviews_open_outlines_are_closed_or_refused() {
    let cases = [
        (r#"{"form": "semicircle", "args": {"radius": 1e-4}, "detail": 24}"#, 1e-3),
        (r#"{"form": "lens", "args": {"radius": 1e-4, "separation": 0.05}, "detail": 24}"#, 1e-3),
        (r#"{"form": "limacon", "args": {"radius": 0.0001001, "ratio": 4e-6}, "detail": 24}"#, 1e-3),
        (r#"{"form": "sine_band", "args": {"length": 1767.9, "amplitude": 1e-4, "cycles": 1.885, "phase": 360, "thickness": 9990.01}, "detail": 8}"#, 1e-3),
        (r#"{"form": "sine_band", "args": {"length": 1767.9, "amplitude": 1e-4, "cycles": 1.885, "phase": 360, "thickness": 9990.01}, "detail": 9}"#, 1e-3),
    ];
    let mut refused_open = 0;
    for (outline, height) in cases {
        let text = recipe(&format!(r#"{{"op": "extrude", "outline": {outline}, "height": {height:e}}}"#));
        match try_make(&text) {
            Ok(m) => sound(&m, &text),
            Err(er) => {
                assert!(er.code == ErrorCode::Geometry && er.measured.is_some(), "{text}: {er}");
                refused_open += usize::from(er.message.contains("not closed"));
            }
        }
    }
    assert!(refused_open >= 2, "{refused_open}");
    // The modeller's own outlines are older than this rule and keep their outcome: this one
    // still builds (open), as it did (docs/design/PROCEDURAL_MODELLING.md, "Shapes by name").
    let plain = try_make(&recipe(r#"{"op": "extrude", "outline": {"sides": 24, "radius": 1e-4}, "height": 1e-3}"#)).unwrap();
    assert!(!plain.lods[0].is_watertight());
}

/// A refusal names the field it is about, in its message as well as its path: a `detail` out of
/// range says `detail` (the review of stage 2: "100000 is outside 8..4096" named nothing).
#[test]
fn a_refusal_names_its_field() {
    for (text, value) in [(recipe(r#"{"op": "form", "form": "sphere", "detail": 100000}"#), "100000"), (recipe(r#"{"op": "extrude", "outline": {"form": "circle", "detail": 3}, "height": 1}"#), "3")] {
        let er = refused(&text);
        assert!(er.code == ErrorCode::OutOfRange && er.path.ends_with("detail") && er.message.contains("\"detail\"") && er.message.contains(value), "{er}");
        assert!(er.message.contains("detail"), "{er}");
    }
    let er = refused(&recipe(r#"{"op": "form", "form": "sphere", "args": {"radius": 1e6}}"#));
    assert!(er.message.contains("radius"), "{er}");
}
