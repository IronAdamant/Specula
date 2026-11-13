//! `model_path` (docs/design/EDITOR.md, "Draw tools"): the path of a model recipe's sweep part
//! (a rail, a pipe, a road's ribbon) redrawn through the project store, on the calling agent's
//! protocol thread, as `model_paint` appends paint. The path is data in the recipe's file
//! (`parts[i].shape.path`): validated with the whole recipe and the style, journaled,
//! claim-checked (a claim on `/project/assets/recipes/...` is respected) and undone with
//! `world_undo`; the model library rebuilds the model. The editor's path tool sends this same
//! call: the human drags where the part runs on the model's ground plane, and the path keeps
//! its height.
//!
//! An agent could do the same with `project_read` and `project_patch`; this is one call that
//! needs none of the recipe's text: it finds the sweep part, keeps the path's height for
//! `[x, z]` points, and resets per-point `scales` that would no longer match.

use crate::paint_tools::{err, text_of};
use crate::tools::{check_keys, Tools};
use specula_core::Json;
use specula_model::PAINT_REACH;
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};

/// Points one `model_path` call may give (a recipe's path holds at most 512 rows).
pub const MAX_POINTS: usize = 256;
/// The most `smooth` a sweep takes (rings added between path points).
const MAX_SMOOTH: u64 = 16;
/// The longest part name.
const MAX_PART_CHARS: usize = 64;
/// The longest recipe path.
const MAX_RECIPE_CHARS: usize = 256;
/// A recipe's parts (`specula_model::recipe::MAX_PARTS`): the range of `sweep`.
const MAX_PARTS: u64 = 256;

/// The points of a call: rows of `[x, z]` or `[x, y, z]`, every row the same width, each
/// number within the reach every recipe coordinate has. The width (2 or 3) and the rows.
pub(crate) fn points(args: &Json) -> Result<(usize, Vec<Vec<f64>>), String> {
    let what = format!("'points' is required: 2..{MAX_POINTS} rows of [x, z] (the path keeps its height) or [x, y, z], in the model's own metres");
    let list = args.get("points").and_then(Json::as_array).ok_or_else(|| what.clone())?;
    if list.len() < 2 || list.len() > MAX_POINTS {
        return Err(format!("'points' takes 2..{MAX_POINTS} rows (got {})", list.len()));
    }
    let mut rows = Vec::with_capacity(list.len());
    let mut width = 0;
    for (i, r) in list.iter().enumerate() {
        let row: Option<Vec<f64>> = r.as_array().filter(|a| a.len() == 2 || a.len() == 3).and_then(|a| a.iter().map(Json::as_f64).collect());
        let row = row.filter(|row| row.iter().all(|v| v.is_finite() && v.abs() <= PAINT_REACH)).ok_or_else(|| format!("'points[{i}]' must be [x, z] or [x, y, z], each within {PAINT_REACH} m of the model's origin"))?;
        if i == 0 {
            width = row.len();
        } else if row.len() != width {
            return Err(format!("'points[{i}]' has {} numbers and 'points[0]' has {width}: give every point the same way", row.len()));
        }
        if let Some(prev) = rows.last() {
            let prev: &Vec<f64> = prev;
            if prev.iter().zip(&row).all(|(a, b)| (a - b).abs() < 1e-6) {
                return Err(format!("'points[{i}]' is where 'points[{}]' is: consecutive path points must differ", i - 1));
            }
        }
        rows.push(row);
    }
    Ok((width, rows))
}

/// The sweep part a call means: by `part` (its name), else the `sweep`-th of the recipe's sweep
/// parts (the first by default). Its index in `parts`, and its name.
pub(crate) fn pick(parts: &[Json], part: Option<&str>, sweep: Option<usize>) -> Result<(usize, String), String> {
    let op = |p: &Json| p.get("shape").and_then(|s| s.get("op")).and_then(Json::as_str).map(str::to_owned);
    let name = |p: &Json| p.get("name").and_then(Json::as_str).unwrap_or("").to_owned();
    let sweeps: Vec<(usize, String)> = parts.iter().enumerate().filter(|(_, p)| op(p).as_deref() == Some("sweep")).map(|(i, p)| (i, name(p))).collect();
    let listed = || {
        let names: Vec<String> = sweeps.iter().take(16).map(|(_, n)| crate::bounds::echo(n)).collect();
        format!("{}{}", names.join(", "), if sweeps.len() > 16 { ", ..." } else { "" })
    };
    if sweeps.is_empty() {
        return Err("the recipe has no sweep part: a path is drawn for a part whose shape is {\"op\": \"sweep\", \"profile\": ..., \"path\": [...]} (add one with project_patch)".to_owned());
    }
    match part {
        Some(want) => match sweeps.iter().find(|(_, n)| n == want) {
            Some(found) => Ok(found.clone()),
            None => Err(match parts.iter().find(|p| name(p) == want) {
                Some(p) => format!("part '{}' is {}, not a sweep (the recipe's sweep parts: {})", crate::bounds::echo(want), op(p).map_or("not a shape".to_owned(), |o| format!("a {}", crate::bounds::echo(&o))), listed()),
                None => format!("the recipe has no part named '{}' (its sweep parts: {})", crate::bounds::echo(want), listed()),
            }),
        },
        None => {
            let k = sweep.unwrap_or(0);
            sweeps.get(k).cloned().ok_or_else(|| format!("'sweep' {k}: the recipe has {} sweep part(s), counted from 0 ({})", sweeps.len(), listed()))
        }
    }
}

/// What the path's change is, as `project_patch` ops on part `i`: the new rows, `closed` and
/// `smooth` when given, and `scales` reset to its ends when it held one value per old point.
/// With whether the scales were reset.
pub(crate) fn plan(shape: &Json, i: usize, width: usize, rows: &[Vec<f64>], closed: Option<bool>, smooth: Option<u64>) -> Result<(Vec<Json>, bool), String> {
    let at = |field: &str| format!("parts[{i}].shape.{field}");
    let set = |field: &str, value: Json| Json::obj([("set", Json::from(at(field))), ("value", value)]);
    let path: Vec<Json> = if width == 3 {
        rows.iter().map(|r| Json::arr(r.iter().map(|v| Json::Num(*v)))).collect()
    } else {
        // [x, z]: the path keeps the height of its first point.
        let y = shape.get("path").and_then(Json::as_array).and_then(|p| p.first()).and_then(Json::as_array).and_then(|r| r.get(1)).and_then(Json::as_f64);
        let y = y.ok_or("the path's height isn't a plain number in the recipe (its first point's y is an expression, or it has no path yet): give the points as [x, y, z]")?;
        rows.iter().map(|r| Json::arr([Json::Num(r[0]), Json::Num(y), Json::Num(r[1])])).collect()
    };
    let is_closed = closed.unwrap_or_else(|| shape.get("closed").and_then(Json::as_bool).unwrap_or(false));
    if is_closed && rows.len() < 3 {
        return Err("a closed path needs at least 3 points".to_owned());
    }
    let mut ops = vec![set("path", Json::Arr(path))];
    if let Some(c) = closed {
        ops.push(set("closed", Json::Bool(c)));
    }
    if let Some(s) = smooth {
        ops.push(set("smooth", Json::from(s)));
    }
    // One scale per old path point no longer matches: the ends are kept.
    let mut reset = false;
    if let Some(scales) = shape.get("scales").and_then(Json::as_array) {
        if scales.len() != 2 && scales.len() != rows.len() {
            if let (Some(a), Some(b)) = (scales.first(), scales.last()) {
                ops.push(set("scales", Json::arr([a.clone(), b.clone()])));
                reset = true;
            }
        }
    }
    Ok((ops, reset))
}

/// `model_path` (on the calling agent's protocol thread).
pub fn call(tools: &Tools, session: &SessionInfo, args: &Json) -> ToolOutput {
    let name = "model_path";
    let bad = |m: String| ToolOutput::error(crate::tools::argument_error(&m).to_string());
    if let Err(m) = check_keys(args, name, &[&["recipe", "part", "sweep", "points", "closed", "smooth", "expect_version", "dry_run"]]) {
        return bad(m);
    }
    let (expect, dry) = match crate::paint_tools::guards(args) {
        Ok(g) => g,
        Err(m) => return bad(m),
    };
    let Some(recipe) = args.get("recipe").and_then(Json::as_str).filter(|r| !r.is_empty() && r.len() <= MAX_RECIPE_CHARS) else {
        return err("invalid_argument", format!("'recipe' is required: the model's recipe under the assets folder (\"recipes/fence.json\"; at most {MAX_RECIPE_CHARS} bytes)"), vec![("field", Json::from("recipe"))]);
    };
    let part = match args.get("part") {
        None => None,
        Some(p) => match p.as_str().filter(|p| !p.is_empty() && p.chars().count() <= MAX_PART_CHARS) {
            Some(p) => Some(p),
            None => return bad(format!("'part' is the sweep part's name (at most {MAX_PART_CHARS} characters)")),
        },
    };
    let sweep = match args.get("sweep") {
        None => None,
        Some(k) => match k.as_u64().filter(|k| *k < MAX_PARTS) {
            Some(k) => Some(k as usize),
            None => return bad(format!("'sweep' is which of the recipe's sweep parts, counted from 0 (0..{})", MAX_PARTS - 1)),
        },
    };
    if part.is_some() && sweep.is_some() {
        return bad("give 'part' (the sweep part's name) or 'sweep' (its index among the sweep parts), not both".to_owned());
    }
    let closed = match args.get("closed") {
        None => None,
        Some(c) => match c.as_bool() {
            Some(c) => Some(c),
            None => return bad("'closed' is true or false".to_owned()),
        },
    };
    let smooth = match args.get("smooth") {
        None => None,
        Some(s) => match s.as_u64().filter(|s| (1..=MAX_SMOOTH).contains(s)) {
            Some(s) => Some(s),
            None => return bad(format!("'smooth' is 1..{MAX_SMOOTH} (rings between path points; 1 is straight segments)")),
        },
    };
    let (width, rows) = match points(args) {
        Ok(p) => p,
        Err(m) => return bad(m),
    };
    // The recipe as the project store holds it: its name (every recipe has one) gives the
    // version that guards the patch; its parts say which is the sweep.
    let path = format!("assets/{recipe}");
    let read = |json_path: &str| crate::project_tools::call(tools, session, "project_read", &Json::obj([("path", Json::from(path.as_str())), ("json_path", Json::from(json_path)), ("problems", Json::from(false))]));
    let head = read("name");
    if head.is_error {
        return head;
    }
    let version = Json::parse(&text_of(&head)).ok().and_then(|j| j.get("version").and_then(Json::as_u64)).unwrap_or(0);
    if let Some(want) = expect {
        if want != version {
            return err("stale_version", format!("{path} is at version {version}, not {want}"), vec![("version", Json::from(version))]);
        }
    }
    let had = read("parts");
    if had.is_error {
        if text_of(&had).contains("too_large") {
            // (A recipe whose parts are over a read's ceiling: rare, and the generic tools do it.)
            return err("too_large", format!("{path}'s parts are too large to read in one call: set the path with project_patch ({{\"set\": \"parts[i].shape.path\", \"value\": [[x, y, z], ...]}})"), vec![]);
        }
        return had;
    }
    let j = Json::parse(&text_of(&had)).unwrap_or(Json::Null);
    if j.get("version").and_then(Json::as_u64) != Some(version) {
        return err("stale_version", format!("{path} changed while its parts were read: call again"), vec![("retry", Json::Bool(true))]);
    }
    let parts = j.get("value").and_then(Json::as_array).unwrap_or(&[]);
    let (i, part_name) = match pick(parts, part, sweep) {
        Ok(p) => p,
        Err(m) => return err("no_sweep", format!("{path}: {m}"), vec![("field", Json::from(if part.is_some() { "part" } else { "sweep" }))]),
    };
    let (ops, scales_reset) = match plan(parts[i].get("shape").unwrap_or(&Json::Null), i, width, &rows, closed, smooth) {
        Ok(p) => p,
        Err(m) => return bad(m),
    };
    let patch = Json::obj([("path", Json::from(path.as_str())), ("ops", Json::Arr(ops)), ("expect_version", Json::from(version)), ("dry_run", Json::from(dry))]);
    // Validated (the whole recipe, with the style: a sweep over the recipe's triangle budget,
    // or too many rings, is refused here) and committed by the project store: one journal
    // entry; the model library rebuilds it. (A path that folds back on itself is not refused:
    // the sweep is built as drawn.)
    let out = crate::project_tools::patch_with(tools, session, &patch, &|_| Ok(None));
    if out.is_error || dry {
        return out;
    }
    let committed = Json::parse(&text_of(&out)).unwrap_or(Json::Null);
    let mut f = vec![
        ("recipe", Json::from(recipe)),
        ("part", Json::from(part_name)),
        ("points", Json::from(rows.len())),
        ("version", committed.get("version").cloned().unwrap_or(Json::Null)),
        ("journal", committed.get("journal").cloned().unwrap_or(Json::Null)),
    ];
    if scales_reset {
        f.push(("scales_reset", Json::Bool(true)));
    }
    ToolOutput::text(Json::obj(f).to_string())
}

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let number = Json::obj([("type", Json::from("number")), ("minimum", Json::Num(-PAINT_REACH)), ("maximum", Json::Num(PAINT_REACH))]);
    let row = Json::obj([("type", Json::from("array")), ("items", number), ("minItems", Json::from(2)), ("maxItems", Json::from(3))]);
    vec![ToolDef {
        name: "model_path".into(),
        description: "Redraw the path of a sweep part of a model recipe (a rail, a pipe, a road's ribbon): sets parts[i].shape.path through the \
                      project store (validated with the whole recipe, journaled, claim-checked: a claim on /project/assets/recipes/... is \
                      respected; undo with world_undo), then the model library rebuilds it and every entity that draws it follows. points: \
                      2..256 rows in the model's own metres, [x, z] (the path keeps the height of its first point) or [x, y, z]. part \
                      names the sweep part; or sweep is its index among the recipe's sweep parts (0, the first, by default). closed and \
                      smooth (1..16 rings between points: a spline through them) are set when given; scales given one per old point are \
                      reset to their first and last (scales_reset). dry_run validates and returns the diff."
            .into(),
        input_schema: Json::obj([
            ("type", Json::from("object")),
            (
                "properties",
                Json::obj([
                    ("recipe", Json::obj([("type", Json::from("string")), ("maxLength", Json::from(MAX_RECIPE_CHARS)), ("description", Json::from("The model's recipe under the assets folder (\"recipes/fence.json\")"))])),
                    ("part", Json::obj([("type", Json::from("string")), ("maxLength", Json::from(MAX_PART_CHARS)), ("description", Json::from("The sweep part's name"))])),
                    ("sweep", Json::obj([("type", Json::from("integer")), ("minimum", Json::from(0)), ("maximum", Json::from(MAX_PARTS - 1)), ("description", Json::from("Or: which of the recipe's sweep parts, counted from 0"))])),
                    ("points", Json::obj([("type", Json::from("array")), ("items", row), ("minItems", Json::from(2)), ("maxItems", Json::from(MAX_POINTS)), ("description", Json::from("2..256 rows of [x, z] or [x, y, z]"))])),
                    ("closed", t("boolean", "Set whether the path is a loop (3 points or more)")),
                    ("smooth", Json::obj([("type", Json::from("integer")), ("minimum", Json::from(1)), ("maximum", Json::from(MAX_SMOOTH)), ("description", Json::from("Set the rings between path points (1: straight segments)"))])),
                    ("expect_version", t("integer", "Refuse unless the recipe is at this project version")),
                    ("dry_run", t("boolean", "Validate and show the diff; change nothing")),
                ]),
            ),
            ("required", Json::arr([Json::from("recipe"), Json::from("points")])),
        ]),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn j(t: &str) -> Json {
        Json::parse(t).unwrap()
    }

    #[test]
    fn points_are_rows_of_one_width_within_reach_and_differ() {
        let (w, rows) = points(&j(r#"{"points": [[0, 0], [1, 0.5], [2, 0]]}"#)).unwrap();
        assert_eq!((w, rows.len()), (2, 3));
        assert_eq!(points(&j(r#"{"points": [[0, 1, 0], [1, 1, 0]]}"#)).unwrap().0, 3);
        let bad = |t: &str| points(&j(t)).unwrap_err();
        assert!(bad(r#"{}"#).contains("'points' is required"));
        assert!(bad(r#"{"points": [[0, 0]]}"#).contains("2..256"));
        assert!(bad(&format!(r#"{{"points": [{}]}}"#, (0..257).map(|i| format!("[{i}, 0]")).collect::<Vec<_>>().join(","))).contains("2..256"));
        assert!(bad(r#"{"points": [[0, 0], [1]]}"#).contains("points[1]"));
        assert!(bad(r#"{"points": [[0, 0], [1, 2, 3, 4]]}"#).contains("points[1]"));
        assert!(bad(r#"{"points": [[0, 0], [1, "a"]]}"#).contains("points[1]"));
        assert!(bad(r#"{"points": [[0, 0], [1e9, 0]]}"#).contains("points[1]"));
        assert!(bad(r#"{"points": [[0, 0], [1, 0, 0]]}"#).contains("the same way"));
        assert!(bad(r#"{"points": [[0, 0], [1, 0], [1, 0]]}"#).contains("points[2]"), "a point on the one before it");
        // The schema says every limit the checks hold (rule 15).
        let def = &definitions()[0];
        let p = def.input_schema.get("properties").and_then(|p| p.get("points")).unwrap();
        assert_eq!(p.get("maxItems").and_then(Json::as_u64), Some(MAX_POINTS as u64));
        assert_eq!(p.get("minItems").and_then(Json::as_u64), Some(2));
    }

    fn parts() -> Vec<Json> {
        j(r#"[
            {"name": "post", "shape": {"op": "box", "size": [1, 1, 1]}},
            {"name": "rail", "shape": {"op": "sweep", "profile": {"rect": [0.1, 0.1]}, "path": [[0, 0.7, 0], [1, 0.7, 0], [2, 0.9, 0]], "scales": [1, 0.8, 0.5]}},
            {"name": "ground", "generate": {"op": "terrain"}},
            {"name": "pipe", "shape": {"op": "sweep", "profile": {"circle": 0.1}, "path": [["w", "h", 0], [1, 1, 0]], "closed": true}}
        ]"#)
        .as_array()
        .unwrap()
        .to_vec()
    }

    #[test]
    fn the_sweep_part_is_found_by_name_or_by_its_place_among_the_sweeps() {
        let p = parts();
        assert_eq!(pick(&p, None, None).unwrap(), (1, "rail".to_owned()), "the first sweep by default");
        assert_eq!(pick(&p, None, Some(1)).unwrap(), (3, "pipe".to_owned()));
        assert_eq!(pick(&p, Some("pipe"), None).unwrap(), (3, "pipe".to_owned()));
        let e = pick(&p, None, Some(2)).unwrap_err();
        assert!(e.contains("2 sweep part(s)") && e.contains("rail, pipe"), "{e}");
        let e = pick(&p, Some("post"), None).unwrap_err();
        assert!(e.contains("a box, not a sweep") && e.contains("rail, pipe"), "{e}");
        let e = pick(&p, Some("ground"), None).unwrap_err();
        assert!(e.contains("not a shape, not a sweep"), "{e}");
        assert!(pick(&p, Some("nope"), None).unwrap_err().contains("no part named 'nope'"));
        assert!(pick(&p[..1], None, None).unwrap_err().contains("no sweep part"));
        assert!(pick(&[], None, None).unwrap_err().contains("no sweep part"));
    }

    #[test]
    fn a_path_keeps_its_height_and_scales_that_no_longer_match_are_reset() {
        let p = parts();
        let rail = p[1].get("shape").unwrap();
        let (_, rows) = points(&j(r#"{"points": [[0, 0], [1, 1], [2, 1], [3, 0]]}"#)).unwrap();
        let (ops, reset) = plan(rail, 1, 2, &rows, None, Some(4)).unwrap();
        assert!(reset, "three scales for four points");
        assert_eq!(
            Json::Arr(ops).to_string(),
            r#"[{"set":"parts[1].shape.path","value":[[0,0.7,0],[1,0.7,1],[2,0.7,1],[3,0.7,0]]},{"set":"parts[1].shape.smooth","value":4},{"set":"parts[1].shape.scales","value":[1,0.5]}]"#
        );
        // As many points as before: the scales still match and stay.
        let (_, three) = points(&j(r#"{"points": [[0, 0], [1, 1], [2, 1]]}"#)).unwrap();
        let (ops, reset) = plan(rail, 1, 2, &three, None, None).unwrap();
        assert!(!reset && ops.len() == 1);
        // [x, y, z] rows are taken as given, and closed is set when asked.
        let (_, xyz) = points(&j(r#"{"points": [[0, 2, 0], [1, 2, 1], [2, 3, 1]]}"#)).unwrap();
        let (ops, _) = plan(rail, 1, 3, &xyz, Some(true), None).unwrap();
        assert!(Json::Arr(ops).to_string().contains(r#"[2,3,1]]},{"set":"parts[1].shape.closed","value":true}"#));
        // A path whose height is an expression can't be kept by [x, z] rows: said, not guessed.
        let pipe = p[3].get("shape").unwrap();
        assert!(plan(pipe, 3, 2, &three, None, None).unwrap_err().contains("[x, y, z]"));
        // A loop needs three points: the recipe's own `closed` counts.
        let (_, two) = points(&j(r#"{"points": [[0, 1, 0], [1, 1, 1]]}"#)).unwrap();
        assert!(plan(pipe, 3, 3, &two, None, None).unwrap_err().contains("at least 3"));
        assert!(plan(pipe, 3, 3, &two, Some(false), None).is_ok(), "opened in the same call");
    }
}
