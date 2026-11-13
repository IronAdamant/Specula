//! `model_paint` (docs/design/EDITOR.md, "Draw tools"): paint strokes appended to a model
//! recipe's `"edits"` through the project store, on the calling agent's protocol thread, as
//! `terrain_edit` appends a terrain's. A stroke is data in the recipe's file
//! ([`specula_model::PaintEdit`]): validated with the whole recipe and the style, journaled,
//! claim-checked (a claim on `/project/assets/recipes/...` is respected) and undone with
//! `world_undo`; the model library rebuilds the model, and every entity that draws it shows the
//! paint. The editor's vertex-paint tool sends this same call.

use crate::tools::{check_keys, Tools};
use specula_core::Json;
use specula_model::{MAX_PAINT_EDITS, PAINT_RADIUS, PAINT_REACH};
use specula_protocol::{Content, SessionInfo, ToolDef, ToolOutput};

/// Strokes one `model_paint` call may add (one release of the editor's brush).
pub const MAX_STROKES_PER_CALL: usize = 64;
/// The longest colour name (`#rrggbb` or a palette or material name of the style).
const MAX_COLOR_CHARS: usize = 64;
/// The longest recipe path.
const MAX_RECIPE_CHARS: usize = 256;

pub(crate) fn err(code: &'static str, message: impl Into<String>, extra: Vec<(&str, Json)>) -> ToolOutput {
    let mut f = vec![("error", Json::from(code)), ("message", Json::from(message.into()))];
    f.extend(extra);
    ToolOutput::error(Json::obj(f).to_string())
}

/// The text of a protocol-thread tool's answer (its first text content).
pub(crate) fn text_of(out: &ToolOutput) -> String {
    out.content.iter().find_map(|c| if let Content::Text(t) = c { Some(t.clone()) } else { None }).unwrap_or_default()
}

/// `expect_version` and `dry_run`, read as the project store's own tools read them: a
/// wrong-typed one is refused, never ignored. (Review R1 of m5-eddraw, M1: `dry_run: "true"`
/// and `dry_run: 1` committed, and a stale `expect_version: "3"` was dropped without a word;
/// `terrain_edit` had the same pattern.)
pub(crate) fn guards(args: &Json) -> Result<(Option<u64>, bool), String> {
    /// The largest integer JSON carries exactly.
    const MAX_VERSION: u64 = (1 << 53) - 1;
    let expect = crate::tools::int_arg(args, "expect_version", 0, MAX_VERSION)?;
    let dry = crate::tools::bool_arg(args, "dry_run")?.unwrap_or(false);
    Ok((expect, dry))
}

/// A stroke's number as the recipe keeps it: a tenth of a millimetre (a thousandth for
/// `amount`). The file stays small enough to read back whatever precision a caller sends
/// (review R1, m2: 452 strokes of 17-digit numbers filled a read's 64 KiB).
fn kept(v: f64, places: f64) -> Json {
    Json::Num((v * places).round() / places)
}

/// Validates one stroke and returns it as recipe JSON (canonical keys only). The colour's
/// name is checked against the style when the project store validates the recipe.
pub(crate) fn stroke(v: &Json, i: usize) -> Result<Json, String> {
    let at = format!("strokes[{i}]");
    check_keys(v, &at, &[&["op", "at", "radius", "color", "amount"]])?;
    if let Some(op) = v.get("op") {
        if op.as_str() != Some("paint") {
            return Err(format!("'{at}.op' is \"paint\" (the only op; it may be left out)"));
        }
    }
    let pt = v.get("at").and_then(Json::as_array).filter(|a| a.len() == 3).and_then(|a| Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?])).filter(|p| p.iter().all(|x| x.is_finite() && x.abs() <= PAINT_REACH));
    let pt = pt.ok_or_else(|| format!("'{at}.at' must be [x, y, z] in the model's own metres (within {PAINT_REACH} m of its origin)"))?;
    let radius = v
        .get("radius")
        .and_then(Json::as_f64)
        .filter(|r| (PAINT_RADIUS.0..=PAINT_RADIUS.1).contains(r))
        .ok_or_else(|| format!("'{at}.radius' must be {}..{} metres", PAINT_RADIUS.0, PAINT_RADIUS.1))?;
    let color = v
        .get("color")
        .and_then(Json::as_str)
        .filter(|c| !c.is_empty() && c.chars().count() <= MAX_COLOR_CHARS)
        .ok_or_else(|| format!("'{at}.color' is required: #rrggbb, or a colour or material name of the style (at most {MAX_COLOR_CHARS} characters)"))?;
    let mut f = vec![("op", Json::from("paint")), ("at", Json::arr(pt.map(|x| kept(x, 1e4)))), ("radius", kept(radius.max(PAINT_RADIUS.0), 1e4)), ("color", Json::from(color))];
    if let Some(a) = v.get("amount") {
        let a = a.as_f64().filter(|x| (0.0..=1.0).contains(x)).ok_or_else(|| format!("'{at}.amount' must be 0..1 (how much of the colour the centre takes; 1 by default)"))?;
        f.push(("amount", kept(a, 1e3)));
    }
    Ok(Json::obj(f))
}

/// `model_paint` (on the calling agent's protocol thread).
pub fn call(tools: &Tools, session: &SessionInfo, args: &Json) -> ToolOutput {
    let name = "model_paint";
    if let Err(m) = check_keys(args, name, &[&["recipe", "strokes", "expect_version", "dry_run"]]) {
        return ToolOutput::error(crate::tools::argument_error(&m).to_string());
    }
    let (expect, dry) = match guards(args) {
        Ok(g) => g,
        Err(m) => return ToolOutput::error(crate::tools::argument_error(&m).to_string()),
    };
    let Some(recipe) = args.get("recipe").and_then(Json::as_str).filter(|r| !r.is_empty() && r.len() <= MAX_RECIPE_CHARS) else {
        return err("invalid_argument", format!("'recipe' is required: the model's recipe under the assets folder (\"recipes/statue.json\"; at most {MAX_RECIPE_CHARS} bytes)"), vec![("field", Json::from("recipe"))]);
    };
    let Some(list) = args.get("strokes").and_then(Json::as_array) else {
        return err("invalid_argument", format!("'strokes' is required: 1..{MAX_STROKES_PER_CALL} of {{at: [x, y, z], radius, color, amount?}}"), vec![("field", Json::from("strokes"))]);
    };
    if list.is_empty() || list.len() > MAX_STROKES_PER_CALL {
        return err("too_many", format!("'strokes' takes 1..{MAX_STROKES_PER_CALL} strokes a call (got {})", list.len()), vec![("field", Json::from("strokes")), ("limit", Json::from(MAX_STROKES_PER_CALL))]);
    }
    let mut strokes = Vec::with_capacity(list.len());
    for (i, s) in list.iter().enumerate() {
        match stroke(s, i) {
            Ok(j) => strokes.push(j),
            Err(m) => return ToolOutput::error(crate::tools::argument_error(&m).to_string()),
        }
    }
    // The recipe as the project store holds it: its name (every recipe has one) gives the
    // version that guards the patch; its edits, if it has any yet.
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
    let had = read("edits");
    let mut edits = if had.is_error {
        // No "edits" yet is the usual case; anything else wrong is the caller's to hear.
        let said = text_of(&had);
        if said.contains("\"too_large\"") {
            // (The list is read back whole to append to it; a read returns 64 KiB. Strokes
            // are kept short, so this is strokes that name long colours. Said as what it is:
            // the read's own advice, "a deeper json_path", is about reading.)
            return err(
                "too_many",
                format!("{path}'s paint strokes fill what one read returns (64 KiB) before the cap of {MAX_PAINT_EDITS}: paint with fewer, wider strokes or shorter colour names, or set the parts' own colours"),
                vec![("limit", Json::from(MAX_PAINT_EDITS))],
            );
        }
        if !said.contains("doesn't exist in") {
            return had;
        }
        Vec::new()
    } else {
        let j = Json::parse(&text_of(&had)).unwrap_or(Json::Null);
        if j.get("version").and_then(Json::as_u64) != Some(version) {
            return err("stale_version", format!("{path} changed while its edits were read: call again"), vec![("retry", Json::Bool(true))]);
        }
        j.get("value").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default()
    };
    if edits.len() + strokes.len() > MAX_PAINT_EDITS {
        return err(
            "too_many",
            format!("the recipe would hold {} strokes; at most {MAX_PAINT_EDITS} (paint larger areas with fewer, wider strokes, or set the parts' own colours)", edits.len() + strokes.len()),
            vec![("limit", Json::from(MAX_PAINT_EDITS)), ("has", Json::from(edits.len()))],
        );
    }
    let added = strokes.len();
    edits.extend(strokes);
    let total = edits.len();
    let ops = Json::arr([Json::obj([("set", Json::from("edits")), ("value", Json::arr(edits))])]);
    let patch = Json::obj([("path", Json::from(path.as_str())), ("ops", ops), ("expect_version", Json::from(version)), ("dry_run", Json::from(dry))]);
    // Validated (the whole recipe, with the style: an unknown colour is refused here) and
    // committed by the project store: one journal entry; the model library rebuilds it.
    let out = crate::project_tools::patch_with(tools, session, &patch, &|_| Ok(None));
    if out.is_error || dry {
        return out;
    }
    let committed = Json::parse(&text_of(&out)).unwrap_or(Json::Null);
    ToolOutput::text(
        Json::obj([
            ("recipe", Json::from(recipe)),
            ("added", Json::from(added)),
            ("strokes", Json::from(total)),
            ("version", committed.get("version").cloned().unwrap_or(Json::Null)),
            ("journal", committed.get("journal").cloned().unwrap_or(Json::Null)),
        ])
        .to_string(),
    )
}

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let stroke = Json::obj([
        ("type", Json::from("object")),
        (
            "properties",
            Json::obj([
                ("op", Json::obj([("type", Json::from("string")), ("enum", Json::arr([Json::from("paint")]))])),
                ("at", Json::obj([("type", Json::from("array")), ("items", Json::obj([("type", Json::from("number")), ("minimum", Json::Num(-PAINT_REACH)), ("maximum", Json::Num(PAINT_REACH))])), ("minItems", Json::from(3)), ("maxItems", Json::from(3))])),
                ("radius", Json::obj([("type", Json::from("number")), ("minimum", Json::Num(PAINT_RADIUS.0)), ("maximum", Json::Num(PAINT_RADIUS.1))])),
                ("color", Json::obj([("type", Json::from("string")), ("maxLength", Json::from(MAX_COLOR_CHARS))])),
                ("amount", Json::obj([("type", Json::from("number")), ("minimum", Json::Num(0.0)), ("maximum", Json::Num(1.0))])),
            ]),
        ),
        ("required", Json::arr([Json::from("at"), Json::from("radius"), Json::from("color")])),
    ]);
    vec![ToolDef {
        name: "model_paint".into(),
        description: "Paint a model's vertex colours as data: appends strokes to the recipe's \"edits\" (applied in order to the built model, in \
                      its own metres) through the project store (journaled, claim-checked: a claim on /project/assets/recipes/... is \
                      respected; undo with world_undo), then the model library rebuilds it and every entity that draws it shows the paint. \
                      Each stroke {at: [x, y, z], radius (m, 0.005..64), color (#rrggbb, or a colour or material of the style), amount \
                      (0..1, 1 by default)}: vertices within the radius take the colour, fully at the centre, easing to nothing at the rim. \
                      Alpha (the wind's painted weight) is untouched; textured parts don't read vertex colours. 1..64 strokes a call, 512 a \
                      recipe. dry_run validates and returns the diff."
            .into(),
        input_schema: Json::obj([
            ("type", Json::from("object")),
            (
                "properties",
                Json::obj([
                    ("recipe", Json::obj([("type", Json::from("string")), ("maxLength", Json::from(MAX_RECIPE_CHARS)), ("description", Json::from("The model's recipe under the assets folder (\"recipes/statue.json\")"))])),
                    ("strokes", Json::obj([("type", Json::from("array")), ("items", stroke), ("minItems", Json::from(1)), ("maxItems", Json::from(MAX_STROKES_PER_CALL)), ("description", Json::from("1..64 strokes"))])),
                    ("expect_version", t("integer", "Refuse unless the recipe is at this project version")),
                    ("dry_run", t("boolean", "Validate and show the diff; change nothing")),
                ]),
            ),
            ("required", Json::arr([Json::from("recipe"), Json::from("strokes")])),
        ]),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strokes_are_checked_and_canonical() {
        let ok = stroke(&Json::parse(r##"{"at":[1,2,3],"radius":0.5,"color":"#ff0000","amount":0.25}"##).unwrap(), 0).unwrap();
        assert_eq!(ok.to_string(), r##"{"op":"paint","at":[1,2,3],"radius":0.5,"color":"#ff0000","amount":0.25}"##);
        let bad = |t: &str| stroke(&Json::parse(t).unwrap(), 2).unwrap_err();
        assert!(bad(r#"{"op":"smear","at":[0,0,0],"radius":1,"color":"a"}"#).contains("strokes[2].op"));
        assert!(bad(r#"{"at":[0,0],"radius":1,"color":"a"}"#).contains("strokes[2].at"));
        assert!(bad(r#"{"at":[0,0,1e9],"radius":1,"color":"a"}"#).contains("strokes[2].at"));
        assert!(bad(r#"{"at":[0,0,0],"radius":0,"color":"a"}"#).contains("radius"));
        assert!(bad(r#"{"at":[0,0,0],"radius":1e9,"color":"a"}"#).contains("radius"));
        assert!(bad(r#"{"at":[0,0,0],"radius":1}"#).contains("color"));
        assert!(bad(&format!(r#"{{"at":[0,0,0],"radius":1,"color":"{}"}}"#, "x".repeat(65))).contains("color"));
        assert!(bad(r#"{"at":[0,0,0],"radius":1,"color":"a","amount":1.5}"#).contains("amount"));
        assert!(bad(r#"{"at":[0,0,0],"radius":1,"color":"a","layer":1}"#).contains("layer"));
        // The schema says every limit the checks hold (rule 15).
        let def = &definitions()[0];
        let strokes = def.input_schema.get("properties").and_then(|p| p.get("strokes")).unwrap();
        assert_eq!(strokes.get("maxItems").and_then(Json::as_u64), Some(MAX_STROKES_PER_CALL as u64));
        // Numbers are kept to a tenth of a millimetre (amount: a thousandth), whatever came:
        // 512 strokes then fit what a read returns.
        let fine = stroke(&Json::parse(r##"{"at":[1.2345678901234567,-2.00004,3.99996],"radius":0.123456789,"color":"#ff0000","amount":0.123456}"##).unwrap(), 0).unwrap();
        assert_eq!(fine.to_string(), r##"{"op":"paint","at":[1.2346,-2,4],"radius":0.1235,"color":"#ff0000","amount":0.123}"##);
        let tiny = stroke(&Json::parse(r##"{"at":[0,0,0],"radius":0.005,"color":"a"}"##).unwrap(), 0).unwrap();
        assert_eq!(tiny.get("radius").and_then(Json::as_f64), Some(0.005), "the smallest radius is not rounded under its limit");
        assert!(fine.to_string().len() * MAX_PAINT_EDITS < 64 * 1024, "512 such strokes are read back in one read");
    }

    /// Review R1 of m5-eddraw, M1: a wrong-typed `dry_run` or `expect_version` is refused (it
    /// was ignored, and the call committed). `model_paint`, `model_path` and `terrain_edit`
    /// all read them here.
    #[test]
    fn a_wrong_typed_dry_run_or_expect_version_is_refused_never_ignored() {
        let g = |t: &str| guards(&Json::parse(t).unwrap());
        assert_eq!(g("{}"), Ok((None, false)));
        assert_eq!(g(r#"{"dry_run": true, "expect_version": 3}"#), Ok((Some(3), true)));
        assert_eq!(g(r#"{"dry_run": false, "expect_version": 0}"#), Ok((Some(0), false)));
        for bad in [r#"{"dry_run": "true"}"#, r#"{"dry_run": 1}"#, r#"{"dry_run": "yes"}"#, r#"{"dry_run": 0}"#] {
            assert!(g(bad).unwrap_err().contains("'dry_run' must be true or false"), "{bad}");
        }
        for bad in [r#"{"expect_version": "3"}"#, r#"{"expect_version": -1}"#, r#"{"expect_version": 1.5}"#, r#"{"expect_version": true}"#, r#"{"expect_version": 1e30}"#] {
            assert!(g(bad).unwrap_err().contains("'expect_version' must be an integer"), "{bad}");
        }
    }
}
