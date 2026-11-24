//! Facets through the modeller (review 2 of stage 4, finding 1): only a form faceted by what it
//! is (`specula_formae::Form::faceted`) is shaded flat whatever the recipe's default smooth angle;
//! every other form is shaded by the smooth angle exactly as it was before the facet tag existed,
//! byte for byte; no two faces within a degree of each other crease; a part's own `smooth_angle`
//! wins over the tag.

use std::collections::BTreeMap;

use specula_formae::{forms, params_of, Built, Found, Kind, Spec};
use specula_model::{build, BuildParams, Model, Style};

fn style() -> Style {
    Style::parse(r##"{ "style": "t", "palette": { "dark": "#202020" }, "materials": { "stone": { "color": "#8c96a3", "roughness": 0.8 } } }"##).unwrap()
}

/// A model of one part whose shape is `shape`; `top` is put first in the recipe (its smooth angle,
/// say) and `part` in the part.
fn make(shape: &str, top: &str, part: &str) -> Option<Model> {
    let s = style();
    let lib = BTreeMap::new();
    let text = format!(r#"{{ "name": "t", {top}"parts": [ {{ "name": "p", "material": "stone", {part}"shape": {shape} }} ] }}"#);
    build(&text, &BuildParams { library: Some(&lib), ..BuildParams::new(&s) }).ok()
}

/// The shape of a form by name: a solid as itself, a plane figure raised as a slab 5 cm thick.
fn shape_of(name: &str, kind: Kind, args: &str) -> Option<String> {
    match kind {
        Kind::Solid => Some(format!(r#"{{"op": "form", "form": "{name}", "args": {args}}}"#)),
        Kind::Plane => Some(format!(r#"{{"op": "form", "form": "{name}", "args": {args}, "raise": {{"as": "slab", "thickness": 0.05}}}}"#)),
        _ => None,
    }
}

/// Every level's positions, normals and indices, hashed (the digest the pins were made with at
/// 939185c7).
fn digest(m: &Model) -> u64 {
    let mut v = Vec::new();
    for l in &m.lods {
        for p in &l.positions {
            for x in p {
                v.extend(x.to_bits().to_le_bytes());
            }
        }
        for n in &l.normals {
            for x in n {
                v.extend(x.to_bits().to_le_bytes());
            }
        }
        for i in &l.indices {
            v.extend(i.to_le_bytes());
        }
        v.extend(u32::MAX.to_le_bytes());
    }
    specula_core::hash::hash_bytes(&v)
}

/// Pairs of normals at one position of the finest level under one degree apart (a seam: two
/// faces shaded as two surfaces where they are one), and the least angle of any pair.
fn seams(m: &Model) -> (usize, f64) {
    let l = &m.lods[0];
    let mut by: BTreeMap<[u32; 3], Vec<[f32; 3]>> = BTreeMap::new();
    for (p, n) in l.positions.iter().zip(&l.normals) {
        by.entry(p.map(f32::to_bits)).or_default().push(*n);
    }
    let (mut under, mut least) = (0, 180.0f64);
    for ns in by.values() {
        for i in 0..ns.len() {
            for j in i + 1..ns.len() {
                let d = f64::from(ns[i][0] * ns[j][0] + ns[i][1] * ns[j][1] + ns[i][2] * ns[j][2]).clamp(-1.0, 1.0);
                let ang = specula_core::dmath::acos_f64(d).to_degrees();
                least = least.min(ang);
                if ang < 1.0 {
                    under += 1;
                }
            }
        }
    }
    (under, least)
}

/// Whether every corner of every triangle carries that triangle's own normal.
fn flat_shaded(m: &Model) -> bool {
    let l = &m.lods[0];
    l.indices.chunks(3).all(|t| {
        let [a, b, c] = [0, 1, 2].map(|k| l.positions[t[k] as usize]);
        let n = [(b[1] - a[1]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[1] - a[1]), (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]), (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        t.iter().all(|&i| {
            let v = l.normals[i as usize];
            (v[0] * n[0] + v[1] * n[1] + v[2] * n[2]) / len > 0.9999
        })
    })
}

fn lcg(s: &mut u64) -> f64 {
    *s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (*s >> 11) as f64 / (1u64 << 53) as f64
}

/// A form's arguments as JSON: none (`set` 0), or the `set`th set drawn over its ranges from a
/// seed of its name (the lane's probe draws the same).
fn drawn(found: Found, set: usize) -> String {
    let params = params_of(&found);
    let mut seed = found.form.name.bytes().fold(0xcbf29ce484222325u64, |a, b| (a ^ u64::from(b)).wrapping_mul(0x100000001b3));
    let mut given = Vec::new();
    for _ in 0..set {
        given = params
            .iter()
            .map(|p| {
                let u = lcg(&mut seed);
                let v = if p.integer {
                    (p.min + specula_core::dmath::floor_f64(u * ((p.max - p.min).min(12.0) + 1.0))).min(p.max)
                } else if p.min > 0.0 && p.max / p.min > 100.0 {
                    specula_core::dmath::exp_f64(specula_core::dmath::ln_f64(p.default) + (u - 0.5) * 3.0).clamp(p.min, p.max)
                } else {
                    p.min + (p.max - p.min) * u
                };
                format!("\"{}\": {v:?}", p.name)
            })
            .collect();
    }
    format!("{{{}}}", given.join(", "))
}

/// Every form that is not faceted makes the same model, byte for byte, as at 939185c7 (the
/// working line before the facet tag), at its defaults, at the recipe's default smooth angle and
/// at 60 (`PINS`, made there by the lane's probe: every level's positions, normals and indices).
/// A form whose model changes on purpose has its pin made again, and says why in its commit.
#[test]
fn every_form_not_faceted_is_shaded_as_before_the_tag() {
    for &(name, at30, at60) in PINS {
        let f = specula_formae::form(name).unwrap_or_else(|| panic!("{name}: no longer a form"));
        assert!(!f.faceted(), "{name} is faceted now: its pin goes");
        let shape = shape_of(name, f.kind, "{}").unwrap();
        for (top, want) in [("", at30), (r#""smooth_angle": 60, "#, at60)] {
            let got = make(&shape, top, "").map_or(0, |m| digest(&m));
            assert_eq!(got, want, "{name} ({top}): its model is not the one it was at 939185c7");
        }
    }
    assert_eq!(PINS.len(), 110);
}

/// No form's model, at its defaults or at drawn arguments, at the recipe's default smooth angle
/// or at 60, has two normals under a degree apart at one position: the tangent joins a facet tag
/// once cut (a raised stadium's walls into its ends, a rounded box's faces into its rounds, an
/// arch's legs into its arch) are one surface. The spindle at 60 is held apart: its tips' fans
/// were split so at 939185c7, by the modeller's smoothing (each corner takes the faces within the
/// angle of its own, so a fan of faces turning past it gives each corner its own normal), which
/// the facet tag does not touch.
#[test]
fn no_two_faces_within_a_degree_crease() {
    let mut models = 0;
    for f in forms() {
        for set in 0..2 {
            let found = Found { form: f, alias: None };
            let args = drawn(found, set);
            let Some(shape) = shape_of(f.name, f.kind, &args) else { continue };
            for top in ["", r#""smooth_angle": 60, "#] {
                let Some(m) = make(&shape, top, "") else { continue };
                models += 1;
                if f.name == "spindle" && !top.is_empty() {
                    continue;
                }
                let (under, least) = seams(&m);
                assert_eq!(under, 0, "{} {args} ({top}): {under} pairs of normals under a degree apart at one position (least {least:.3} degrees)", f.name);
            }
        }
    }
    assert!(models > 500, "{models} models");
}

/// A faceted mesh's vertices in the modeller: each point once for each plane its faces lie in
/// (faces in one plane, as the stella octangula's pairs of triangles, share their corners; planes
/// told apart by their unit normals to 1e-4).
fn corners(mesh: &specula_formae::Mesh) -> usize {
    let mut keys = std::collections::BTreeSet::new();
    for face in &mesh.faces {
        let p = |k: usize| mesh.points[face.verts[k] as usize];
        let (a, b, c) = (p(0), p(1), p(2));
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let plane = n.map(|x| specula_core::dmath::round_f64(x / l * 1e4) as i64);
        for &i in &face.verts {
            keys.insert((i, plane));
        }
    }
    keys.len()
}

/// The faceted forms (`Form::faceted`) are the polyhedra of four families and three others. Each
/// is shaded flat at the recipe's default smooth angle, at 60 and at 120: every corner carries its
/// face's normal, so its vertices are its faces' corners, each point once a plane (the count
/// FORMAE.md gives). A part's own smooth angle takes the facets off; the recipe's does not.
#[test]
fn a_faceted_form_is_flat_unless_its_part_says() {
    use specula_formae::Family;
    let faceted: Vec<&str> = forms().iter().filter(|f| f.faceted()).map(|f| f.name).collect();
    assert_eq!(faceted.len(), 40, "{faceted:?}");
    for f in forms().iter().filter(|f| f.faceted()) {
        assert!(matches!(f.family, Family::Platonic | Family::Archimedean | Family::Catalan | Family::Stellated) || specula_formae::FACETED_NAMES.contains(&f.name), "{}", f.name);
        let found = Found { form: f, alias: None };
        for set in 0..3 {
            let args = drawn(found, set);
            let shape = shape_of(f.name, f.kind, &args).unwrap();
            for top in ["", r#""smooth_angle": 60, "#, r#""smooth_angle": 120, "#] {
                let Some(m) = make(&shape, top, "") else { continue };
                assert!(flat_shaded(&m), "{} {args} ({top}): a corner's normal is not its face's", f.name);
                assert_eq!(seams(&m).0, 0, "{} {args} ({top})", f.name);
                if set == 0 {
                    let Ok(Built::Mesh(mesh)) = Spec::of(found, &[]).unwrap().build(specula_formae::DETAIL_DEFAULT) else { panic!("{}", f.name) };
                    assert_eq!(m.lods[0].positions.len(), corners(&mesh), "{}: the modeller's vertices are not its faces' corners", f.name);
                }
            }
        }
    }
    // The cut stone, the crystal and the chamfered box at their defaults: FORMAE.md's numbers.
    for (name, want) in COUNTS {
        let m = make(&format!(r#"{{"op": "form", "form": "{name}"}}"#), "", "").unwrap();
        let Ok(Built::Mesh(lib)) = Spec::of(Found { form: specula_formae::form(name).unwrap(), alias: None }, &[]).unwrap().build(specula_formae::DETAIL_DEFAULT) else { panic!("{name}") };
        assert_eq!((lib.points.len(), lib.faces.len(), m.lods[0].positions.len(), m.lods[0].indices.len() / 3), *want, "{name}: library points, faces; modeller's vertices, triangles");
    }
    let gem = make(r#"{"op": "form", "form": "gem"}"#, "", "").unwrap();
    let smoothed = make(r#"{"op": "form", "form": "gem"}"#, "", r#""smooth_angle": 60, "#).unwrap();
    assert!(!flat_shaded(&smoothed), "a part's own smooth angle left the gem faceted");
    assert!(smoothed.lods[0].positions.len() < gem.lods[0].positions.len());
    assert!(flat_shaded(&make(r#"{"op": "form", "form": "gem"}"#, r#""smooth_angle": 60, "#, "").unwrap()), "the recipe's smooth angle smoothed the gem");
    // A round form is still smooth, and a part's smooth angle on a form that is not faceted is
    // the smooth angle as it always was.
    let rc = make(r#"{"op": "form", "form": "rounded_cylinder"}"#, "", "").unwrap();
    assert!(!flat_shaded(&rc), "the rounded cylinder's round is shaded flat");
    assert_eq!(digest(&rc), digest(&make(r#"{"op": "form", "form": "rounded_cylinder"}"#, "", r#""smooth_angle": 30, "#).unwrap()));
}

/// The tangent joins of review 2 and a polygon of many sides, each as at 939185c7 (their counts
/// there; their bytes are in `PINS` where they are a form's defaults): at 60, no seam, and a
/// prism of 64 sides or a 48-gon raised is smooth round its side.
#[test]
fn tangent_joins_and_many_sides_follow_the_smooth_angle() {
    for (shape, verts) in [
        (r#"{"op": "form", "form": "stadium", "raise": {"as": "slab", "thickness": 0.05}}"#, 264),
        (r#"{"op": "form", "form": "rounded_rectangle", "raise": {"as": "slab", "thickness": 0.05}}"#, 272),
        (r#"{"op": "form", "form": "round_arch", "raise": {"as": "slab", "thickness": 0.05}}"#, 144),
        (r#"{"op": "form", "form": "rounded_box"}"#, 6536),
        (r#"{"op": "form", "form": "prism", "args": {"sides": 64}}"#, 256),
        (r#"{"op": "form", "form": "regular_polygon", "args": {"sides": 48}, "raise": {"as": "slab", "thickness": 0.05}}"#, 192),
    ] {
        let m = make(shape, r#""smooth_angle": 60, "#, "").unwrap();
        assert_eq!((m.lods[0].positions.len(), seams(&m).0), (verts, 0), "{shape}");
    }
}

/// The faceted forms outside the polyhedra at their defaults: (name, (library points, library
/// faces, the modeller's vertices, its triangles)).
const COUNTS: &[(&str, (usize, usize, usize, usize))] = &[("gem", (25, 33, 112, 46)), ("crystal", (14, 18, 60, 24)), ("chamfered_box", (24, 26, 96, 44))];

/// Each form that is not faceted, as its model was at 939185c7: (name, digest at the recipe's
/// default smooth angle, digest at 60).
const PINS: &[(&str, u64, u64)] = &[
    ("triangle_sss", 0xe5530f3e4d9c0210, 0xe5530f3e4d9c0210),
    ("triangle_sas", 0x8634040e8f691135, 0x8634040e8f691135),
    ("triangle", 0xbbeca9c1faf83e33, 0xbbeca9c1faf83e33),
    ("rectangle", 0xef8d57cff3ca996a, 0xef8d57cff3ca996a),
    ("parallelogram", 0x32f77c9100425fbf, 0x526b122ef534b257),
    ("rhombus", 0xa42d6a1c2eb50907, 0xa42d6a1c2eb50907),
    ("kite", 0xd7878f74f11f81f1, 0xd7878f74f11f81f1),
    ("dart", 0xeefaa200d8940a81, 0xeefaa200d8940a81),
    ("trapezium", 0x779fa07fe374aeca, 0x779fa07fe374aeca),
    ("regular_polygon", 0x509a988b1a70f38f, 0xed5168f4ecb0474e),
    ("star", 0xdcb66e13e2248ac4, 0xdcb66e13e2248ac4),
    ("star_polygon", 0x877b29da38ac29c7, 0x877b29da38ac29c7),
    ("cross", 0x5067d523c1b560a2, 0x5067d523c1b560a2),
    ("saltire", 0xc75a623b6862db8a, 0xc75a623b6862db8a),
    ("arrow", 0xd8676d748764ff24, 0xd8676d748764ff24),
    ("double_arrow", 0x4c5e2a33b71faaec, 0x4c5e2a33b71faaec),
    ("chevron", 0x494ae7bad724ea5c, 0x1aff555e72331121),
    ("shield", 0xce02765aaef8b82d, 0xce02765aaef8b82d),
    ("l_section", 0xf20d38fad52e5972, 0xf20d38fad52e5972),
    ("t_section", 0x73de266386407ecb, 0x73de266386407ecb),
    ("u_section", 0x49e3670080693051, 0x49e3670080693051),
    ("h_section", 0xe7197fcfa87ea4e4, 0xe7197fcfa87ea4e4),
    ("z_section", 0x7d8a2e12ab3aa12c, 0x7d8a2e12ab3aa12c),
    ("frame", 0xec39e85d21119c2f, 0xec39e85d21119c2f),
    ("polygon_ring", 0x41d0fe3d7030eb70, 0x0a7a92f50ffdd872),
    ("gear", 0x156e45645a64b725, 0x156e45645a64b725),
    ("sawtooth_band", 0x3332e76714edb017, 0x93205dcd5bd6102a),
    ("battlement_band", 0x0cdaf1f6299ffa8d, 0x0cdaf1f6299ffa8d),
    ("circle", 0xa4a970dec75d3511, 0xa4a970dec75d3511),
    ("semicircle", 0x68674044a054032d, 0x68674044a054032d),
    ("sector", 0xf16b240e7ec672e3, 0xf16b240e7ec672e3),
    ("circular_segment", 0xe4988ebaa5050eff, 0xe4988ebaa5050eff),
    ("annulus", 0x84a22ca12dad8aa0, 0x84a22ca12dad8aa0),
    ("annular_sector", 0x0c54d76533e5bc8d, 0x0c54d76533e5bc8d),
    ("lune", 0xb8f5f18110cb564e, 0xb8f5f18110cb564e),
    ("lens", 0xd3d1146db4cbbab6, 0xd3d1146db4cbbab6),
    ("stadium", 0x8c6913020bc3bd1f, 0x8c6913020bc3bd1f),
    ("rounded_rectangle", 0xa74c5739bee9c420, 0xa74c5739bee9c420),
    ("ellipse", 0x0b5d281c77261b8d, 0x0b5d281c77261b8d),
    ("superellipse", 0xca47625e548bdff1, 0xca47625e548bdff1),
    ("moss_egg", 0x9fef786afec57cee, 0x9fef786afec57cee),
    ("oval", 0x9361920952cd4713, 0x9361920952cd4713),
    ("ovoid", 0x0e7fd849b9fdf697, 0x0e7fd849b9fdf697),
    ("teardrop", 0x77c6698fac95ecdd, 0x77c6698fac95ecdd),
    ("heart", 0xd6c1402373757134, 0xd6c1402373757134),
    ("round_arch", 0xc32754a11ff1d26b, 0xc32754a11ff1d26b),
    ("pointed_arch", 0x48497ea7d129ae9d, 0x48497ea7d129ae9d),
    ("horseshoe_arch", 0xd6d3e1cf7580ad84, 0xd6d3e1cf7580ad84),
    ("ogee_arch", 0x82ced98a046932eb, 0x82ced98a046932eb),
    ("keyhole", 0xbba3049c2f5bd94e, 0xbba3049c2f5bd94e),
    ("reuleaux_polygon", 0x89088be5f77ad3a0, 0x89088be5f77ad3a0),
    ("foil", 0x0824dee6297ee0a0, 0x0824dee6297ee0a0),
    ("flower", 0xe85b152b9960e275, 0xa4804d410b600e67),
    ("limacon", 0x4ef687b80c5ae760, 0x4ef687b80c5ae760),
    ("epicycloid", 0x2a40adf50068e9cf, 0x2a40adf50068e9cf),
    ("hypocycloid", 0xd0fcb2fd8b20fbd4, 0xd0fcb2fd8b20fbd4),
    ("superformula", 0xb6b7163ad6c9d6c5, 0xe4e4079474c9f763),
    ("zigzag_band", 0x5efa9d406e56e564, 0x8a2e604efec45508),
    ("elliptical_arc_band", 0x6a89a870745bedce, 0x6a89a870745bedce),
    ("parabola_band", 0x685f54347b57d886, 0x685f54347b57d886),
    ("catenary_band", 0x0b4fa86e91079658, 0x8993b9dcbe4c0862),
    ("sine_band", 0x151a7547939e16ba, 0x0a59f45c7beb681e),
    ("archimedean_spiral_band", 0x3d3b6eded8270833, 0x3d3b6eded8270833),
    ("logarithmic_spiral_band", 0x65b45eea28764cd7, 0x15feb7e1ac2e7a46),
    ("involute_band", 0x564ddaab8c2912ce, 0x564ddaab8c2912ce),
    ("box", 0xfbb46f658a94fee2, 0xfbb46f658a94fee2),
    ("wedge", 0xd844c3d3e0297a8a, 0xd844c3d3e0297a8a),
    ("prism", 0xdba2521f86da3748, 0x014977d598cbf2d4),
    ("antiprism", 0x2b5aff751f2690bd, 0x7777a7a97255fb6c),
    ("pyramid", 0x44e8bdc8f667f784, 0xeafb4f33cff99699),
    ("pyramid_frustum", 0x2ca8e83137cf402d, 0x2ca8e83137cf402d),
    ("bipyramid", 0x5258ca8060009196, 0x9b26afdaeed71054),
    ("trapezohedron", 0xc3f30f0dc8cd8137, 0xcf3d571e7ec25534),
    ("star_prism", 0xf1a45ce88f6a5084, 0xf1a45ce88f6a5084),
    ("sphere", 0xe878b2e0cc09dad1, 0xed839659ca9c2d91),
    ("icosphere", 0xb7ecf5068ff3dcad, 0x74beb09642813668),
    ("cube_sphere", 0xf45eb67ab27d0d84, 0x2eb89953b0e02ee1),
    ("ellipsoid", 0xb2356e56efdff3a1, 0x0250ceaca4f3f333),
    ("hemisphere", 0xbf2cc903c4621a22, 0x1fdb8d9ecc0ecd6f),
    ("spherical_cap", 0x253e232c66a4f383, 0x3188db4b600ea179),
    ("spherical_segment", 0x97312b4d96c8bd0d, 0x73c143302ec773e8),
    ("spherical_wedge", 0x99708586ed3fb6d9, 0xadd2190f80549904),
    ("cylinder", 0xc6436194d9001101, 0xc6436194d9001101),
    ("elliptic_cylinder", 0x5c3c37069d6c65b9, 0x2ac172274a44043f),
    ("cylinder_sector", 0xb22f40e115d852ab, 0xb22f40e115d852ab),
    ("tube", 0x44701139505f6652, 0x911b01d9391ad4a5),
    ("cone", 0xa7ed6a67dc6fe898, 0x3ef0fc13c1a3d200),
    ("frustum", 0x88f8343a4a0b3253, 0xe9398d13b2dba999),
    ("bicone", 0xd9ed72e7187d6c30, 0x36df651085f9336d),
    ("capsule", 0xfbd5db5cc20eb9da, 0xfdd5068a7f2f00f6),
    ("torus", 0xed4fef69f43a1ab5, 0x9f49c59fb0b92625),
    ("torus_sector", 0xfb8d6aa95cffc1dc, 0xb9bb9bc36feea677),
    ("elliptic_torus", 0x7a375128e65efb0f, 0x303d54e3ad3ffec6),
    ("biconvex_lens", 0xac7757cf039b318e, 0xab1d7664827934d0),
    ("spindle", 0x7ef96918d9164ab0, 0xba7565579ed6fed8),
    ("barrel", 0xa09d64573352f6b4, 0x9b78eace503fe258),
    ("hourglass", 0x0be8f70dd486656c, 0xe6badde381ec7ef4),
    ("egg_of_revolution", 0x897096a40fec4e0b, 0xbd0a42aecd5d792a),
    ("teardrop_of_revolution", 0xe6b7ceb7cfd6a346, 0x3e3e8ea51d52631c),
    ("paraboloid", 0x52f1c05f05f4580e, 0x52f1c05f05f4580e),
    ("hyperboloid", 0xeac3671416a50c34, 0x73c2b16b6b33d1d6),
    ("superellipsoid", 0x68091465f447b406, 0x1c1ca2b42161d3c6),
    ("rounded_box", 0xe0e8bea034cea1b5, 0x9d17005538ab0a83),
    ("dome", 0xd47c581c64fa478e, 0xdb212798230d94ce),
    ("bowl", 0xe67be4c407a40a7a, 0x797e906b1ace700a),
    ("helix", 0x3107ee93a0bf6655, 0x099ce5560b232280),
    ("torus_knot", 0x78207994e0edd05d, 0x2e6e5356fc7b80a6),
    ("mobius_band", 0xd82eca450d2d44d7, 0xe0d2ee62f80f9c7f),
    ("steinmetz_solid", 0x0ac53299c44ac909, 0x0df1b34fa546bf85),
    ("round_arrow", 0xca0541609a870cc5, 0x5c8779204d93d427),
];
