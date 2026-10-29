//! The terrain tools (docs/design/TERRAIN.md): `terrain_info` (how a terrain entity is drawn,
//! its LOD, residency, layers, and whether its collider is the drawn surface) on the engine
//! thread, and `terrain_edit` (sculpt and paint strokes appended to a terrain recipe through the
//! project store, then the entity's heightfield collider rewritten from the edited heights) on
//! the calling agent's protocol thread. Water has no tools of its own: it is the `water`
//! component of `world_spawn` / `world_set`.

use crate::schedule::Class;
use crate::terrain::terrain_of;
use crate::tools::{check_keys, entity_key, Tools};
use crate::world::{EntityKey, WorldError};
use crate::Engine;
use specula_core::Json;
use specula_model::terrain::{EditOp, MAX_EDITS, MAX_SPLAT};
use specula_protocol::{Content, SessionInfo, ToolDef, ToolOutput};

/// What puts a heightfield collider on a terrain entity (terrain_info's hint; `collider` is the
/// entity's path, as terrain_edit takes it).
const COLLIDER_HINT: &str = "procgen_terrain {recipe, collider: {entity}} or terrain_edit {recipe, strokes, collider: \"/path/of/this/entity\"} puts one on";

/// Strokes one `terrain_edit` call may add.
pub const MAX_STROKES_PER_CALL: usize = 64;

pub enum TerrainCall {
    Info { entity: EntityKey },
    /// terrain_edit's collider target: the entity's path and model (`{id, path, model}`), or a
    /// structured error (not found, not a model). Not a tool of its own.
    Target { entity: EntityKey },
}

impl TerrainCall {
    pub fn class(&self) -> Class {
        Class::Read
    }
}

fn err(code: &'static str, message: impl Into<String>, extra: Vec<(&str, Json)>) -> ToolOutput {
    let mut f = vec![("error", Json::from(code)), ("message", Json::from(message.into()))];
    f.extend(extra);
    ToolOutput::error(Json::obj(f).to_string())
}

pub fn parse(name: &str, args: &Json) -> Option<Result<TerrainCall, String>> {
    match name {
        "terrain_info" => Some((|| {
            check_keys(args, name, &[&["id", "path", "name"]])?;
            Ok(TerrainCall::Info { entity: entity_key(args, name)? })
        })()),
        _ => None,
    }
}

fn num(v: f32) -> Json {
    Json::Num(((v as f64) * 1000.0).round() / 1000.0)
}

impl Engine {
    pub(crate) fn terrain_call(&mut self, call: TerrainCall) -> ToolOutput {
        let entity = match call {
            TerrainCall::Info { entity } => entity,
            TerrainCall::Target { entity } => {
                let id = match self.world.resolve(&entity) {
                    Ok(id) => id,
                    Err(e) => return crate::world_error(&e),
                };
                let e = self.world.get(id).expect("resolved");
                let Some(spec) = &e.model else {
                    return crate::world_error(&WorldError::new("not_terrain", format!("'{}' is a {}, not a model entity: the collider goes on the terrain's model entity", e.name, e.shape.name())));
                };
                return ToolOutput::text(Json::obj([("id", Json::from(id)), ("path", Json::from(self.world.path_of(id))), ("model", spec.to_json())]).to_string());
            }
        };
        let id = match self.world.resolve(&entity) {
            Ok(id) => id,
            Err(e) => return crate::world_error(&e),
        };
        let e = self.world.get(id).expect("resolved");
        let lib = self.library.lock();
        let Some(spec) = &e.model else {
            return crate::world_error(&WorldError::new("not_terrain", format!("'{}' is a {}, not a model entity: a terrain is a model whose recipe's terrain generator says \"render\": \"terrain\"", e.name, e.shape.name())));
        };
        let built = e.resolved_model().and_then(|r| lib.built(r.handle)).cloned();
        let Some(built) = built else {
            return crate::world_error(&WorldError::new("not_built", format!("'{}' ({}) isn't built yet (model_state: {})", e.name, spec.recipe, e.model_state())));
        };
        let Some(t) = built.model.extras.terrain.clone() else {
            return crate::world_error(&WorldError::new("not_terrain", format!("{} has no terrain part (a part with \"generate\": {{\"op\": \"terrain\", ...}})", spec.recipe)));
        };
        let renderer = terrain_of(e, &lib);
        drop(lib);
        let (lo, hi) = t.heights.iter().fold((f32::MAX, f32::MIN), |(a, b), h| (a.min(*h), b.max(*h)));
        let mut f = vec![
            ("id", Json::from(e.id)),
            ("path", Json::from(self.world.path_of(e.id))),
            ("recipe", Json::from(spec.recipe.as_str())),
            ("samples", Json::arr([Json::from(t.columns), Json::from(t.rows)])),
            ("cell_m", Json::arr(t.cell.map(num))),
            ("height_range", Json::arr([num(lo), num(hi)])),
            ("offset", Json::arr(t.offset.map(num))),
        ];
        let drawn_by = if t.render.is_none() {
            ("mesh", "the recipe's terrain generator doesn't say \"render\": \"terrain\"")
        } else if renderer.is_none() {
            ("mesh", "an op on the terrain part reshapes it (displace, bend...): only moves and scales keep a heightfield")
        } else if self.terrain.disabled {
            ("mesh", "the terrain renderer is off (--terrain off)")
        } else if self.terrain.covers(e.id) {
            ("terrain", "")
        } else {
            ("mesh", "waiting: its textures or the terrain pipelines aren't on the GPU yet (it draws its stand-in mesh meanwhile)")
        };
        f.push(("drawn_by", Json::from(drawn_by.0)));
        if !drawn_by.1.is_empty() {
            f.push(("why", Json::from(drawn_by.1)));
        }
        if let Some(r) = &t.render {
            f.push((
                "layers",
                Json::arr(r.layers.iter().enumerate().map(|(k, l)| {
                    Json::obj([
                        ("name", t.splat_names.get(k).map_or(Json::Null, |n| Json::from(n.as_str()))),
                        ("texture", l.texture.as_deref().map_or(Json::Null, Json::from)),
                        ("uv_scale", num(l.uv_scale)),
                        ("roughness", num(l.roughness)),
                        ("triplanar_deg", num(l.triplanar_deg)),
                        ("coverage", Json::from((t.splat.iter().map(|w| w[k.min(3)] as u64).sum::<u64>() as f64 / (t.splat.len().max(1) as f64 * 255.0) * 1000.0).round() / 1000.0)),
                    ])
                })),
            ));
            f.push(("skirt_m", num(r.skirt)));
            let scale = e.scale.x.abs().max(e.scale.y.abs()).max(e.scale.z.abs());
            let r0 = specula_gpu::terrain::ranges(0.0, t.cell[0].max(t.cell[1]) * scale)[0];
            let d = self.terrain.drawn(e.id).unwrap_or_default();
            f.push((
                "lod",
                Json::obj([
                    ("full_resolution_within_m", num(r0)),
                    ("patches", Json::from(d.patches)),
                    ("coarse_copy", Json::from(d.coarse)),
                    ("levels", if d.patches > 0 { Json::arr([Json::from(d.levels.0), Json::from(d.levels.1)]) } else { Json::Null }),
                    ("note", Json::from("as of the last frame drawn (window or capture); within full_resolution_within_m of the camera the drawn triangles are the heightfield's, bit for bit")),
                ]),
            ));
            let (full, coarse, bytes) = self.terrain.residency(built.id);
            f.push(("residency", Json::obj([("full", Json::from(full)), ("coarse", Json::from(coarse)), ("gpu_bytes", Json::from(bytes))])));
        }
        f.push(("collider", collider_check(e, &t, renderer.as_ref().map(|r| r.2))));
        ToolOutput::text(Json::obj(f).to_string())
    }
}

/// Whether the entity's heightfield collider is the terrain's surface (the heights the renderer
/// draws, through the terrain part's move and scale).
fn collider_check(e: &crate::world::Entity, t: &specula_model::terrain::Terrain, local: Option<specula_core::math::Mat4>) -> Json {
    let Some(c) = &e.collider else {
        return Json::obj([("present", Json::from(false)), ("hint", Json::from(COLLIDER_HINT))]);
    };
    let crate::physics::ColliderShape::Heightfield { columns, rows, cell, heights } = &c.shape else {
        return Json::obj([("present", Json::from(true)), ("shape", Json::from(c.shape.name())), ("matches", Json::from(false))]);
    };
    let Some(local) = local else {
        return Json::obj([("present", Json::from(true)), ("matches", Json::Null), ("note", Json::from("the terrain isn't a heightfield in the model (reshaped)"))]);
    };
    // The terrain part's scale and move (what procgen_terrain's collider applies).
    let (sx, sy, sz) = (local.cols[0][0], local.cols[1][1], local.cols[2][2]);
    let off = [local.cols[3][0], local.cols[3][1], local.cols[3][2]];
    let exact = *columns as usize == t.columns && *rows as usize == t.rows && cell[0] == t.cell[0] * sx && cell[1] == t.cell[1] * sz;
    let mut worst = 0.0f32;
    let (cw, rw) = ((*columns - 1) as f32, (*rows - 1) as f32);
    for j in 0..*rows as usize {
        for i in 0..*columns as usize {
            let ch = heights[j * *columns as usize + i] + c.offset.y - off[1];
            let th = if exact {
                t.heights[j * t.columns + i] * sy
            } else {
                let (x, z) = ((i as f32 - cw * 0.5) * cell[0] / sx, (j as f32 - rw * 0.5) * cell[1] / sz);
                t.height_at(x, z).unwrap_or(f32::NAN) * sy
            };
            worst = worst.max((ch - th).abs());
        }
    }
    let matches = worst <= 1e-4;
    let mut f = vec![
        ("present", Json::from(true)),
        ("columns", Json::from(*columns)),
        ("rows", Json::from(*rows)),
        ("same_grid", Json::from(exact)),
        ("max_error_m", Json::Num((worst as f64 * 1e5).round() / 1e5)),
        ("matches", Json::from(matches)),
    ];
    if !exact {
        f.push(("note", Json::from(format!("the collider is resampled ({columns} x {rows}; the engine's colliders hold at most {} a side): within the full-resolution ring the drawn surface is the terrain's {} x {} samples, so they differ by up to max_error_m; tiles of 129 samples match exactly", crate::physics::MAX_HEIGHTFIELD_SIDE, t.columns, t.rows))));
    } else if !matches {
        f.push(("hint", Json::from("the collider is stale (the recipe changed since it was written): terrain_edit with collider, or procgen_terrain {recipe, collider: {entity}}, writes it again")));
    }
    Json::obj(f)
}

/// The text of a protocol-thread tool's answer (its first text content).
fn text_of(out: &ToolOutput) -> String {
    out.content.iter().find_map(|c| if let Content::Text(t) = c { Some(t.clone()) } else { None }).unwrap_or_default()
}

/// Validates one stroke and returns it as recipe JSON (canonical keys only).
fn stroke(v: &Json, i: usize) -> Result<Json, String> {
    let at = format!("strokes[{i}]");
    check_keys(v, &at, &[&["op", "at", "radius", "amount", "height", "layer"]])?;
    let op = v.get("op").and_then(Json::as_str).ok_or_else(|| format!("'{at}.op' is required: raise, lower, flatten, smooth or paint"))?;
    let op = EditOp::parse(op).ok_or_else(|| format!("'{at}.op' must be raise, lower, flatten, smooth or paint (not '{op}')"))?;
    let pt = v.get("at").and_then(Json::as_array).filter(|a| a.len() == 2).and_then(|a| Some([a[0].as_f64()?, a[1].as_f64()?])).filter(|p| p.iter().all(|x| x.is_finite() && x.abs() <= 1e5));
    let pt = pt.ok_or_else(|| format!("'{at}.at' must be [x, z] (the terrain's local metres plus its offset)"))?;
    let radius = v.get("radius").and_then(Json::as_f64).filter(|r| r.is_finite() && *r > 0.0 && *r <= 1e4).ok_or_else(|| format!("'{at}.radius' must be metres > 0 (at most a quarter of the terrain's size)"))?;
    let mut f = vec![("op", Json::from(op.name())), ("at", Json::arr(pt.map(Json::Num))), ("radius", Json::Num(radius))];
    if let Some(a) = v.get("amount") {
        let a = a.as_f64().filter(|x| x.is_finite() && x.abs() <= 1000.0).ok_or_else(|| format!("'{at}.amount' must be a number (metres for raise/lower, 0..1 for flatten/smooth/paint)"))?;
        f.push(("amount", Json::Num(a)));
    }
    if op == EditOp::Flatten {
        let h = v.get("height").and_then(Json::as_f64).filter(|x| x.is_finite() && x.abs() <= 1e4).ok_or_else(|| format!("'{at}.height' is required for flatten: the target height (metres, the terrain's local)"))?;
        f.push(("height", Json::Num(h)));
    }
    if op == EditOp::Paint {
        let l = v.get("layer").and_then(Json::as_u64).filter(|l| (*l as usize) < MAX_SPLAT).ok_or_else(|| format!("'{at}.layer' is required for paint: a splat layer 0..{}", MAX_SPLAT - 1))?;
        f.push(("layer", Json::from(l)));
    }
    Ok(Json::obj(f))
}

/// `terrain_edit` (on the calling agent's protocol thread).
pub fn call(tools: &Tools, session: &SessionInfo, name: &str, args: &Json) -> ToolOutput {
    if name != "terrain_edit" {
        return err("unknown_tool", format!("unknown terrain tool '{name}'"), vec![]);
    }
    if let Err(m) = check_keys(args, name, &[&["recipe", "part", "strokes", "collider", "expect_version", "dry_run"]]) {
        return ToolOutput::error(crate::tools::argument_error(&m).to_string());
    }
    let Some(recipe) = args.get("recipe").and_then(Json::as_str) else {
        return err("invalid_argument", "'recipe' is required: the terrain recipe under the assets folder (\"recipes/field.json\")", vec![("field", Json::from("recipe"))]);
    };
    let Some(list) = args.get("strokes").and_then(Json::as_array) else {
        return err("invalid_argument", format!("'strokes' is required: 1..{MAX_STROKES_PER_CALL} of {{op, at, radius, amount?, height?, layer?}}"), vec![("field", Json::from("strokes"))]);
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
    let collider = match args.get("collider") {
        None | Some(Json::Null) | Some(Json::Bool(false)) => None,
        Some(Json::Str(p)) => Some(p.clone()),
        Some(_) => return err("invalid_argument", "'collider' is the terrain entity's path: its heightfield collider is written again from the edited heights (in the same journal entry as the recipe)", vec![("field", Json::from("collider"))]),
    };
    // The collider's entity first: it must exist and be a model of this recipe (its params and
    // seed build the collider). Nothing is changed when it isn't.
    let target = match &collider {
        None => None,
        Some(entity) => {
            let key = if entity.starts_with('/') { EntityKey::Path(entity.clone()) } else { EntityKey::Name(entity.clone()) };
            let out = tools.send(session, crate::tools::Command::Terrain(TerrainCall::Target { entity: key }));
            if out.is_error {
                let mut j = Json::parse(&text_of(&out)).unwrap_or(Json::Null);
                j.set("field", Json::from("collider"));
                j.set("recipe_committed", Json::Bool(false));
                return ToolOutput::error(j.to_string());
            }
            let t = Json::parse(&text_of(&out)).unwrap_or(Json::Null);
            let model = t.get("model").cloned().unwrap_or(Json::Null);
            if model.get("recipe").and_then(Json::as_str) != Some(recipe) {
                return err(
                    "collider_not_this_terrain",
                    format!("'{entity}' draws {}, not {recipe}: its collider would not be this terrain's surface; nothing was changed", model.get("recipe").and_then(Json::as_str).unwrap_or("no recipe")),
                    vec![("field", Json::from("collider")), ("recipe_committed", Json::Bool(false))],
                );
            }
            let path = t.get("path").and_then(Json::as_str).unwrap_or(entity).to_owned();
            Some((path, model.get("params").cloned().unwrap_or(Json::Null), model.get("seed").and_then(Json::as_u64)))
        }
    };
    // The recipe as the project store holds it (its version guards the patch).
    let path = format!("assets/{recipe}");
    let read = crate::project_tools::call(tools, session, "project_read", &Json::obj([("path", Json::from(path.as_str())), ("json_path", Json::from("parts")), ("problems", Json::from(false))]));
    if read.is_error {
        return read;
    }
    let read = Json::parse(&text_of(&read)).unwrap_or(Json::Null);
    let version = read.get("version").and_then(Json::as_u64).unwrap_or(0);
    if let Some(want) = args.get("expect_version").and_then(Json::as_u64) {
        if want != version {
            return err("stale_version", format!("{path} is at version {version}, not {want}"), vec![("version", Json::from(version))]);
        }
    }
    let parts = read.get("value").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default();
    let want = args.get("part").and_then(Json::as_str);
    let found = parts.iter().position(|p| p.get("generate").and_then(|g| g.get("op")).and_then(Json::as_str) == Some("terrain") && want.is_none_or(|w| p.get("name").and_then(Json::as_str) == Some(w)));
    let Some(k) = found else {
        return err("no_terrain", format!("{path} has no terrain part{}", want.map_or(String::new(), |w| format!(" named '{w}'"))), vec![]);
    };
    let mut edits = parts[k].get("generate").and_then(|g| g.get("edits")).and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default();
    if edits.len() + strokes.len() > MAX_EDITS {
        return err("too_many", format!("the terrain would hold {} strokes; at most {MAX_EDITS} (smooth or flatten larger areas with fewer strokes)", edits.len() + strokes.len()), vec![("limit", Json::from(MAX_EDITS)), ("has", Json::from(edits.len()))]);
    }
    let added = strokes.len();
    edits.extend(strokes);
    let ops = Json::arr([Json::obj([("set", Json::from(format!("parts[{k}].generate.edits"))), ("value", Json::arr(edits.clone()))])]);
    let dry = args.get("dry_run").and_then(Json::as_bool).unwrap_or(false);
    let patch = Json::obj([("path", Json::from(path.as_str())), ("ops", ops), ("expect_version", Json::from(version)), ("dry_run", Json::from(dry))]);
    // The collider from the edited text (built here, before anything is committed), committed
    // with the recipe in one journal entry: claims are checked on both, and one world_undo takes
    // both back, so the collider never parts from the drawn surface.
    let summary = std::cell::RefCell::new(None);
    let also = |text: &str| -> Result<Option<crate::project_tools::Also>, ToolOutput> {
        let Some((entity, params, seed)) = &target else { return Ok(None) };
        let (key, change, s) = crate::procgen_tools::collider_for_text(&tools.procgen, recipe, text, params, *seed, entity).map_err(|out| {
            let mut j = Json::parse(&text_of(&out)).unwrap_or(Json::Null);
            j.set("field", Json::from("collider"));
            j.set("recipe_committed", Json::Bool(false));
            ToolOutput::error(j.to_string())
        })?;
        *summary.borrow_mut() = Some(s);
        Ok(Some(Box::new((key, change))))
    };
    // Validated (the whole recipe, with the style) and committed by the project store: journaled,
    // claim-checked, undoable with world_undo; the model library rebuilds it.
    let out = crate::project_tools::patch_with(tools, session, &patch, &also);
    if out.is_error || dry {
        return out;
    }
    let committed = Json::parse(&text_of(&out)).unwrap_or(Json::Null);
    let mut f = vec![
        ("recipe", Json::from(recipe)),
        ("part", Json::from(k)),
        ("added", Json::from(added)),
        ("strokes", Json::from(edits.len())),
        ("version", committed.get("version").cloned().unwrap_or(Json::Null)),
        ("journal", committed.get("journal").cloned().unwrap_or(Json::Null)),
    ];
    if let Some(mut s) = summary.into_inner() {
        let range = s.get("height_range_m").cloned().unwrap_or(Json::Null);
        if let Json::Obj(p) = &mut s {
            p.retain(|(k, _)| k != "height_range_m");
        }
        f.push(("collider", s));
        f.push(("height_range", range));
    }
    ToolOutput::text(Json::obj(f).to_string())
}

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::arr(required.iter().map(|r| Json::from(*r))))])
    };
    let def = |name: &str, d: &str, input_schema: Json| ToolDef { name: name.into(), description: d.into(), input_schema };
    vec![
        def(
            "terrain_info",
            "How a terrain entity is drawn. A terrain is a model entity whose recipe's terrain generator says \"render\": \"terrain\" \
             (CDLOD heightfield patches with splat materials instead of the recipe's mesh; the splat layers take texture (a recipe \
             texture graph), uv_scale, roughness, specular, triplanar (slope degrees), normal_strength; the generator takes offset \
             [x, z] (tiles of one terrain), edge_lock (samples; tiles share their border heights), skirt (m), grain, edits). Reports \
             samples, cell, height range, drawn_by (terrain or mesh, and why), layers with coverage, LOD (the full-resolution ring, \
             patches and levels of the last frame), GPU residency, and the collider check: whether the entity's heightfield \
             collider is the drawn surface (max_error_m).",
            schema(vec![("id", t("integer", "Entity id")), ("path", t("string", "Entity path")), ("name", t("string", "Unique entity name"))], &[]),
        ),
        def(
            "terrain_edit",
            "Sculpt and paint a terrain as data: appends strokes to the recipe's terrain part (\"edits\": applied in order after \
             erosion, in the terrain's local metres plus its offset) through the project store (journaled, claim-checked: a claim \
             on /project/assets/recipes/... is respected; undo with world_undo), then the model library rebuilds it. Each stroke \
             {op: raise|lower|flatten|smooth|paint, at: [x, z], radius (m, at most a quarter of the terrain), amount (m for \
             raise/lower; 0..1 for flatten/smooth/paint), height (flatten's target, m), layer (paint's splat layer 0..3)}; strength \
             eases from the centre to the rim. 1..64 strokes a call, 512 a terrain. collider: the terrain entity's path, whose \
             heightfield collider is then written again from the edited heights (as procgen_terrain does), so physics stays the \
             drawn surface. dry_run validates and returns the diff.",
            schema(
                vec![
                    ("recipe", t("string", "The terrain recipe under the assets folder (\"recipes/field.json\")")),
                    ("part", t("string", "The terrain part's name (default: the first terrain part)")),
                    ("strokes", Json::obj([("type", Json::from("array")), ("items", Json::obj([("type", Json::from("object"))])), ("description", Json::from("1..64 strokes"))])),
                    ("collider", t("string", "The terrain entity's path: its collider is rewritten from the edited heights")),
                    ("expect_version", t("integer", "Refuse unless the recipe is at this project version")),
                    ("dry_run", t("boolean", "Validate and show the diff; change nothing")),
                ],
                &["recipe", "strokes"],
            ),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strokes_are_checked_and_canonical() {
        let ok = stroke(&Json::parse(r#"{"op":"flatten","at":[1,2],"radius":3,"height":0.5,"amount":1}"#).unwrap(), 0).unwrap();
        assert_eq!(ok.get("op").and_then(Json::as_str), Some("flatten"));
        let bad = |t: &str| stroke(&Json::parse(t).unwrap(), 2).unwrap_err();
        assert!(bad(r#"{"op":"dig","at":[0,0],"radius":1}"#).contains("strokes[2].op"));
        assert!(bad(r#"{"op":"raise","at":[0],"radius":1}"#).contains("strokes[2].at"));
        assert!(bad(r#"{"op":"raise","at":[0,0],"radius":-1}"#).contains("radius"));
        assert!(bad(r#"{"op":"flatten","at":[0,0],"radius":1}"#).contains("height"));
        assert!(bad(r#"{"op":"paint","at":[0,0],"radius":1,"layer":7}"#).contains("layer"));
        assert!(bad(r#"{"op":"raise","at":[0,0],"radius":1,"colour":1}"#).contains("colour"));
        // Parses back into the recipe's schema (specula-model checks the rest).
        assert!(parse("terrain_info", &Json::parse(r#"{"path":"/t"}"#).unwrap()).unwrap().is_ok());
        assert!(parse("terrain_info", &Json::parse(r#"{"what":1}"#).unwrap()).unwrap().is_err());
        // The hint names the argument as terrain_edit takes it (a path; `true` is refused).
        assert!(!COLLIDER_HINT.contains("collider: true") && COLLIDER_HINT.contains("collider: \"/"));
    }
}
