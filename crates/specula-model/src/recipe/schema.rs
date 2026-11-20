//! A recipe's format, said from the tables the parser itself reads ([`super::SHAPES`],
//! [`super::OPS`], the outline's fields, the generators', a recipe's and a part's own), so that
//! what is said is what is accepted: an agent asks once and writes a recipe that builds, where it
//! used to learn the format one refusal at a time or from a design document.
//!
//! Two answers: with nothing named, the whole format on a line a shape and a line an operation,
//! with an example that builds ([`SCHEMA_EXAMPLE`]), under [`SCHEMA_FIRST_BYTES`]; with a shape's,
//! an operation's or a generator's name, every field it takes with what each takes.

use specula_core::Json;

use super::procgen::GENERATORS;
use super::{Field, Kind, OPS, OUTLINE, PART_FIELDS, SHAPES, TOP_FIELDS};

/// The most the first answer may be, as compact JSON (held by a test).
pub const SCHEMA_FIRST_BYTES: usize = 6 * 1024;

/// A whole recipe that builds under any style that has a material called `metal`: a token with a
/// hole through it, standing upright on the ground. (A test builds it, so it cannot rot.)
pub const SCHEMA_EXAMPLE: &str = r#"{
  "name": "token",
  "description": "a round token with a hole through it, standing upright",
  "budget": { "max_triangles": 2000 },
  "parts": [
    {
      "name": "disc",
      "material": "metal",
      "shape": {
        "op": "difference",
        "of": [
          { "op": "cylinder", "radius": 0.15, "height": 0.04, "sides": 32 },
          { "op": "cylinder", "radius": 0.04, "height": 0.2, "sides": 16, "ops": [ { "op": "move", "by": [0, -0.08, 0] } ] }
        ]
      },
      "ops": [ { "op": "rotate", "deg": [90, 0, 0] }, { "op": "move", "by": [0, 0.15, 0] } ]
    }
  ]
}"#;

/// What each of a recipe's own fields is (every name of [`TOP_FIELDS`], held by a test).
const TOP_ABOUT: &[(&str, &str)] = &[
    ("recipe", "the format's version, 1 (may be left out)"),
    ("name", "the model's name (required)"),
    ("description", "a line of what it is"),
    ("category", "its kind in the project's style (budgets and checks go by it)"),
    ("seed", "whole number for its random parts (default 0)"),
    ("params", "{name: {default, min, max}}: numbers a caller may set; any number in the recipe may be an expression of them, as a string"),
    ("let", "{name: expression}: named values worked out once"),
    ("budget", "{max_triangles, min_triangles}: the build is refused outside them"),
    ("lods", "ratios of further levels of detail, e.g. [0.5, 0.25] (at most 4)"),
    ("collider", "none, aabb, capsule or convex_hull (the default)"),
    ("smooth_angle", "degrees under which neighbouring faces shade as one surface"),
    ("rig", "bones, for a model that moves"),
    ("parts", "the list of parts (required): each a shape, an include or a generator, with a material"),
    ("finish", "operations applied to the whole model after its parts, as a part's ops"),
    ("textures", "{name: a texture graph}: procedural textures a part names by `texture`"),
    ("wind", "how foliage sways"),
    ("edits", "painted vertex colours (model_paint writes them)"),
];

/// What each of a part's fields is (every name of [`PART_FIELDS`], held by a test).
const PART_ABOUT: &[(&str, &str)] = &[
    ("name", "the part's name (required)"),
    ("material", "a material of the project's style.json by name (required for a shape)"),
    ("color", "\"#rrggbb\" or a palette name of the style, over the material's colour"),
    ("when", "an expression: the part is built only when it is not 0"),
    ("shape", "one shape: {\"op\": NAME, ...its fields} (one of shape, include, generate)"),
    ("include", "another recipe's path, built in as this part"),
    ("generate", "a generator: {\"op\": building | scatter | terrain, ...}"),
    ("params", "{name: expression}: an included recipe's parameters"),
    ("ops", "operations applied in order: [{\"op\": NAME, ...its fields}]"),
    ("smooth_angle", "as the recipe's, for this part"),
    ("lod_detail", "how much of its detail the coarser levels keep"),
    ("bones", "the rig's bones this part moves with"),
    ("blend", "metres over which it blends between its bones"),
    ("texture", "a texture of the recipe's `textures` by name"),
    ("uv_scale", "how often the texture repeats"),
    ("note", "a line for whoever reads the recipe (not built)"),
];

/// What a field takes, in a word or two (a plain number says nothing).
fn takes(kind: Kind) -> String {
    match kind {
        Kind::Num => String::new(),
        Kind::Choice(list) => format!(" ({})", list.join(" | ")),
        Kind::Obj(fields) | Kind::Objs(fields) => format!(" {{{}}}", fields.iter().map(|f| f.name).collect::<Vec<_>>().join(", ")),
        other => format!(" ({})", other.word()),
    }
}

/// One line for a shape, an operation or a generator: what it needs, then what it may take.
fn line(name: &str, fields: &[Field]) -> String {
    let say = |f: &Field| format!("{}{}", f.name, takes(f.kind));
    let need: Vec<String> = fields.iter().filter(|f| f.required).map(say).collect();
    let may: Vec<String> = fields.iter().filter(|f| !f.required).map(|f| f.name.to_owned()).collect();
    let mut out = format!("{name}: {}", if need.is_empty() { "nothing required".to_owned() } else { need.join(", ") });
    if !may.is_empty() {
        out.push_str(&format!("; may take {}", may.join(", ")));
    }
    out
}

fn lines(table: &[(&str, &[Field])]) -> Json {
    Json::arr(table.iter().map(|(n, f)| Json::from(line(n, f))))
}

fn about(table: &[(&str, &str)]) -> Json {
    Json::arr(table.iter().map(|(n, a)| Json::from(format!("{n}: {a}"))))
}

/// Every field of one named thing, with what each takes.
fn one(kind: &str, name: &str, fields: &[Field]) -> Json {
    let field = |f: &Field| {
        let what = match f.kind {
            Kind::Num => "number".to_owned(),
            Kind::Choice(list) => format!("one of: {}", list.join(", ")),
            Kind::Obj(sub) | Kind::Objs(sub) => format!("{} with: {}", f.kind.word(), sub.iter().map(|s| format!("{}{}{}", s.name, takes(s.kind), if s.required { " (required)" } else { "" })).collect::<Vec<_>>().join(", ")),
            other => other.word().to_owned(),
        };
        Json::from(format!("{}{}: {what}", f.name, if f.required { " (required)" } else { "" }))
    };
    Json::obj([("kind", Json::from(kind)), ("name", Json::from(name)), ("fields", Json::arr(fields.iter().map(field)))])
}

/// A recipe's format. `of`: nothing for the whole of it in one short answer; a shape's, an
/// operation's or a generator's name (or `outline`, `part`, `recipe`) for every field of that one
/// thing. An unknown name is refused with the names there are.
pub fn schema(of: Option<&str>) -> Result<Json, String> {
    let Some(name) = of else {
        let example = Json::parse(SCHEMA_EXAMPLE).map_err(|e| format!("the example is not JSON: {e:?}"))?;
        return Ok(Json::obj([
            (
                "recipe",
                Json::from(
                    "A model recipe is one JSON file under the project's assets (assets/models/NAME.json); model_build {\"recipe\": \"models/NAME.json\"} builds it and says its \
                     triangles, its bounds and whether it is closed; studio_sheet pictures it. Metres, +Y up. A mesh shape stands on y = 0, centred on the Y axis; a named form \
                     (shape `form`, outline `form`: formae_list names them) lies flat in the ground plane, so a raised figure is stood up with \
                     {\"op\": \"rotate\", \"deg\": [90, 0, 0]} and moved up by half its height. Any number may be an expression in a string (\"2 * k\").",
                ),
            ),
            ("fields", about(TOP_ABOUT)),
            ("part", about(PART_ABOUT)),
            ("shapes", lines(SHAPES)),
            ("ops", lines(OPS)),
            ("generators", lines(GENERATORS)),
            ("outline", Json::from(format!("an outline is one object with one way of saying it, of: {}", OUTLINE.iter().map(|f| f.name).collect::<Vec<_>>().join(", ")))),
            ("example", example),
            ("more", Json::from("model_schema {\"of\": NAME} says every field of one shape, operation or generator (or of: outline, part, recipe) with what each takes. A refusal from model_build names the field and what the shape accepts.")),
        ]));
    };
    let fields_about = |table: &[(&str, &str)], kind: &str| Json::obj([("kind", Json::from(kind)), ("name", Json::from(kind)), ("fields", about(table))]);
    match name {
        "recipe" => return Ok(fields_about(TOP_ABOUT, "recipe")),
        "part" => return Ok(fields_about(PART_ABOUT, "part")),
        "outline" => return Ok(one("outline", "outline", OUTLINE)),
        _ => {}
    }
    for (kind, table) in [("shape", SHAPES), ("op", OPS), ("generator", GENERATORS)] {
        if let Some((n, fields)) = table.iter().find(|(n, _)| *n == name) {
            return Ok(one(kind, n, fields));
        }
    }
    let names = |table: &[(&str, &[Field])]| table.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ");
    Err(format!("'{name}' is not a shape, an operation or a generator of a recipe. Shapes: {}. Operations: {}. Generators: {}. Also: outline, part, recipe.", names(SHAPES), names(OPS), names(GENERATORS)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build, BuildParams, Style};

    #[test]
    fn every_field_the_parser_reads_is_said_and_no_other() {
        fn said(table: &'static [(&'static str, &'static str)]) -> Vec<&'static str> {
            table.iter().map(|(n, _)| *n).collect()
        }
        assert_eq!(said(TOP_ABOUT), TOP_FIELDS, "a recipe's fields and what is said of them have drifted apart");
        assert_eq!(said(PART_ABOUT), PART_FIELDS, "a part's fields and what is said of them have drifted apart");
    }

    #[test]
    fn the_first_answer_is_whole_and_under_its_ceiling() {
        let first = schema(None).unwrap();
        let text = first.to_string();
        assert!(text.len() <= SCHEMA_FIRST_BYTES, "{} bytes of {SCHEMA_FIRST_BYTES}", text.len());
        // Every shape, operation and generator has its line, by its own name.
        for (key, names) in [("shapes", crate::recipe::shape_names()), ("ops", crate::recipe::op_names()), ("generators", crate::recipe::procgen::generator_names())] {
            let rows = first.get(key).and_then(|v| v.as_array()).unwrap_or_else(|| panic!("no {key}"));
            let starts: Vec<String> = rows.iter().map(|r| r.as_str().unwrap().split(':').next().unwrap().to_owned()).collect();
            assert_eq!(starts, names, "{key}");
        }
        // What a shape needs is said before what it may take.
        assert!(text.contains("box: size ([x, y, z]); may take bevel"), "{text}");
        assert!(text.contains("rotate: deg ([x, y, z]); may take pivot"), "{text}");
    }

    #[test]
    fn the_example_builds_closed_and_upright() {
        let style = Style::parse(r##"{ "style": "t", "palette": { "ink": "#202020" }, "materials": { "metal": { "color": "#9aa3ad", "roughness": 0.4 } } }"##).unwrap();
        let model = build(SCHEMA_EXAMPLE, &BuildParams::new(&style)).unwrap_or_else(|e| panic!("the example does not build: {e:?}"));
        assert_eq!(model.open_edges, 0, "the example is not closed");
        let b = model.bounds;
        // Upright: 0.3 across and tall, 0.04 thick, standing on the ground.
        assert!((b.max[0] - b.min[0] - 0.3).abs() < 0.01 && (b.max[1] - b.min[1] - 0.3).abs() < 0.01 && (b.max[2] - b.min[2] - 0.04).abs() < 0.005, "{b:?}");
        assert!(b.min[1].abs() < 0.005, "it does not stand on the ground: {b:?}");
        // And it has its hole: more triangles than a plain disc of 32 sides (124).
        assert!(model.lods[0].triangle_count() > 150, "{} triangles: no hole", model.lods[0].triangle_count());
    }

    #[test]
    fn one_thing_is_said_field_by_field_and_a_wrong_name_names_the_right_ones() {
        let extrude = schema(Some("extrude")).unwrap().to_string();
        assert!(extrude.contains("outline (required): outline") && extrude.contains("height (required): number") && extrude.contains("segments: whole number"), "{extrude}");
        let form = schema(Some("form")).unwrap().to_string();
        assert!(form.contains("form (required): name") && form.contains("raise: object with: as"), "{form}");
        assert!(schema(Some("collider")).is_err());
        let wrong = schema(Some("extrud")).unwrap_err();
        assert!(wrong.contains("extrude") && wrong.contains("rotate") && wrong.contains("scatter"), "{wrong}");
        for of in ["recipe", "part", "outline", "rotate", "scatter", "sdf"] {
            assert!(schema(Some(of)).is_ok(), "{of}");
        }
    }
}
