//! The asset studio's tools (docs/design/ASSET_STUDIO.md): `studio_stage`, `studio_sheet`,
//! `studio_lineup`, `studio_compare`, `studio_check` and `style_check`. Parsing and schemas are
//! here; the stage, the renders and the measures are in [`crate::studio`].
//!
//! A call runs on its caller's connection thread (waiting on `model_build` for subjects still
//! building); only its renders go to the engine thread, which serves them round-robin between
//! frames within a render budget (see [`crate::studio`]). None of these tools changes the world
//! (the stage is isolated): nothing to claim, journal or undo.

use crate::response::Format;
use crate::tools::{argument_error, bool_arg, check_keys, int_arg, params_arg, string_arg, Tools};
use crate::world::{ModelSpec, MAX_MODEL_SEED};
use specula_core::Json;
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};

/// Subjects one sheet, line-up or check renders.
pub const MAX_SUBJECTS: usize = 12;
/// Subjects one `style_check` covers (variants included).
pub const MAX_STYLE_SUBJECTS: usize = 96;
/// Subjects one `studio_stage` frame shows.
pub const MAX_STAGE_SUBJECTS: usize = 6;
/// Tiles one sheet may have (subjects × views × passes).
pub const MAX_TILES: usize = 96;
/// Largest sheet or frame side (the sight tools' cap).
pub const MAX_SHEET_PX: u32 = 2048;
/// Views one sheet may have.
pub const MAX_VIEWS: usize = 16;

/// A model on the stage: a recipe (or glTF) with parameters and seed, and how sheets label it.
#[derive(Clone, Debug, PartialEq)]
pub struct SubjectSpec {
    pub spec: ModelSpec,
    pub label: Option<String>,
}

/// The standard shots (a shot set is all of them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shot {
    CloseUp,
    Gameplay,
    WorstLight,
    Phone,
    Igpu,
}

impl Shot {
    pub const ALL: [Shot; 5] = [Shot::CloseUp, Shot::Gameplay, Shot::WorstLight, Shot::Phone, Shot::Igpu];

    pub fn name(self) -> &'static str {
        match self {
            Shot::CloseUp => "close_up",
            Shot::Gameplay => "gameplay",
            Shot::WorstLight => "worst_light",
            Shot::Phone => "phone",
            Shot::Igpu => "igpu",
        }
    }

    fn parse(s: &str) -> Option<Shot> {
        Shot::ALL.into_iter().find(|x| x.name() == s)
    }
}

/// A named light rig.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RigName {
    Neutral,
    Game,
    Worst,
}

impl RigName {
    pub fn name(self) -> &'static str {
        match self {
            RigName::Neutral => "neutral",
            RigName::Game => "game",
            RigName::Worst => "worst",
        }
    }
}

/// One column of a sheet: an angle (azimuth 0 = the model's front, +Z; 90 = its left, +X) or a
/// standard shot.
#[derive(Clone, Debug, PartialEq)]
pub enum View {
    Angle { label: String, az: f32, el: f32 },
    Shot(Shot),
}

impl View {
    pub fn label(&self) -> String {
        match self {
            View::Angle { label, .. } => label.clone(),
            View::Shot(s) => s.name().to_string(),
        }
    }
}

/// What a tile shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    Colour,
    Value,
    Silhouette,
}

impl Pass {
    pub fn name(self) -> &'static str {
        match self {
            Pass::Colour => "colour",
            Pass::Value => "value",
            Pass::Silhouette => "silhouette",
        }
    }
}

/// A contact sheet's request (also what a mark repeats).
#[derive(Clone, Debug, PartialEq)]
pub struct SheetSpec {
    pub subjects: Vec<SubjectSpec>,
    pub views: Vec<View>,
    pub passes: Vec<Pass>,
    pub rig: RigName,
    pub tile: u32,
    pub references: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CompareFrom {
    /// A mark this agent made with `studio_sheet {mark}`.
    Mark(String),
    /// Two subjects now, with the sheet's views and passes.
    Pair { before: SubjectSpec, after: SubjectSpec },
}

#[derive(Clone, Debug, PartialEq)]
pub enum StudioCall {
    /// No subjects: the stage's description.
    Stage { subjects: Vec<SubjectSpec>, view: View, rig: Option<RigName>, size: [u32; 2], references: bool, format: Format },
    Sheet { sheet: SheetSpec, mark: Option<String>, format: Format },
    Lineup { subjects: Vec<SubjectSpec>, rig: RigName, width: u32, format: Format },
    Compare { from: CompareFrom, sheet: SheetSpec, format: Format },
    Check { subject: SubjectSpec, siblings: Option<Vec<SubjectSpec>>, tile: u32, image: bool, format: Format },
    Style { subjects: Vec<SubjectSpec>, folder: Option<String>, variants: bool, format: Format },
}

// ---------------------------------------------------------------------------------------------
// Parsing

fn label_arg(v: &Json, path: &str) -> Result<Option<String>, String> {
    match v.get("label") {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Str(s)) if !s.is_empty() && s.chars().count() <= 48 && !s.chars().any(char::is_control) => Ok(Some(s.clone())),
        Some(_) => Err(format!("'{path}.label' must be a short name (1..48 characters)")),
    }
}

fn subject_of(v: &Json, path: &str) -> Result<SubjectSpec, String> {
    let (spec, label) = match v {
        Json::Str(r) => (ModelSpec::new(r.as_str()), None),
        Json::Obj(_) => {
            check_keys(v, &format!("'{path}'"), &[&["recipe", "params", "seed", "label"]])?;
            let recipe = string_arg(v, "recipe")?.ok_or(format!("'{path}.recipe' is required: a recipe (or .glb/.gltf) under the assets root"))?;
            let seed = int_arg(v, "seed", 0, MAX_MODEL_SEED)?;
            (ModelSpec { recipe: recipe.to_string(), params: params_arg(v)?.unwrap_or_default(), seed }, label_arg(v, path)?)
        }
        _ => return Err(format!("'{path}' is a recipe path or {{recipe, params?, seed?, label?}}")),
    };
    spec.check().map_err(|e| format!("'{path}': {}", e.message))?;
    Ok(SubjectSpec { spec, label })
}

/// `subjects` (a list), or one subject from `recipe`/`params`/`seed`.
fn subjects_arg(args: &Json, max: usize, required: bool) -> Result<Vec<SubjectSpec>, String> {
    let one = args.get("recipe").is_some_and(|v| !v.is_null());
    match args.get("subjects").filter(|v| !v.is_null()) {
        Some(_) if one => Err("give 'subjects' or 'recipe' (with params, seed), not both".into()),
        Some(Json::Arr(items)) if items.len() > max => Err(format!("'subjects' has {} items; at most {max} per call", items.len())),
        Some(Json::Arr(items)) if items.is_empty() && required => Err("'subjects' is empty: give at least one recipe".into()),
        Some(Json::Arr(items)) => items.iter().enumerate().map(|(i, s)| subject_of(s, &format!("subjects[{i}]"))).collect(),
        Some(_) => Err("'subjects' must be a list of recipe paths or {recipe, params?, seed?, label?}".into()),
        None if one => {
            let mut o = Json::obj::<&str>([]);
            for k in ["recipe", "params", "seed", "label"] {
                if let Some(v) = args.get(k) {
                    o.set(k, v.clone());
                }
            }
            Ok(vec![subject_of(&o, "recipe")?])
        }
        None if required => Err("'subjects' (a list of recipes) or 'recipe' is required".into()),
        None => Ok(Vec::new()),
    }
}

fn rig_arg(args: &Json) -> Result<Option<RigName>, String> {
    match string_arg(args, "rig")? {
        None => Ok(None),
        Some("neutral") => Ok(Some(RigName::Neutral)),
        Some("game") => Ok(Some(RigName::Game)),
        Some("worst") => Ok(Some(RigName::Worst)),
        Some(o) => Err(format!("'rig' is \"neutral\", \"game\" or \"worst\", not '{o}'")),
    }
}

fn angle(name: &str) -> Option<(f32, f32)> {
    Some(match name {
        "front" => (0.0, 8.0),
        "back" => (180.0, 8.0),
        "left" => (90.0, 8.0),
        "right" => (-90.0, 8.0),
        "three_quarter" => (35.0, 20.0),
        "top" => (0.0, 85.0),
        _ => return None,
    })
}

const VIEW_HELP: &str = "a view is \"front\", \"back\", \"left\", \"right\", \"three_quarter\", \"top\", a shot (\"close_up\", \"gameplay\", \"worst_light\", \"phone\", \"igpu\") or {az, el} in degrees";

fn view_of(v: &Json, path: &str) -> Result<View, String> {
    match v {
        Json::Str(s) => {
            if let Some((az, el)) = angle(s) {
                return Ok(View::Angle { label: s.clone(), az, el });
            }
            Shot::parse(s).map(View::Shot).ok_or(format!("'{path}': {VIEW_HELP}"))
        }
        Json::Obj(_) => {
            check_keys(v, &format!("'{path}'"), &[&["az", "el"]])?;
            let az = v.get("az").and_then(Json::as_f64).filter(|x| x.abs() <= 360.0).ok_or(format!("'{path}.az' is degrees, -360..360"))? as f32;
            let el = v.get("el").and_then(Json::as_f64).unwrap_or(10.0);
            if !(-30.0..=89.0).contains(&el) {
                return Err(format!("'{path}.el' is degrees, -30..89"));
            }
            Ok(View::Angle { label: format!("az{az:.0} el{el:.0}"), az, el: el as f32 })
        }
        _ => Err(format!("'{path}': {VIEW_HELP}")),
    }
}

/// `views`: a turntable count, `"shots"` (the shot set) or a list.
fn views_arg(args: &Json, default: Vec<View>) -> Result<Vec<View>, String> {
    match args.get("views").filter(|v| !v.is_null()) {
        None => Ok(default),
        Some(Json::Str(s)) if s == "shots" => Ok(Shot::ALL.into_iter().map(View::Shot).collect()),
        Some(Json::Num(n)) => {
            let n = *n;
            if n.fract() != 0.0 || !(1.0..=MAX_VIEWS as f64).contains(&n) {
                return Err(format!("'views' as a number is a turntable of 1..{MAX_VIEWS} angles"));
            }
            Ok((0..n as u32).map(|k| {
                let az = 360.0 * k as f32 / n as f32;
                View::Angle { label: format!("az{az:.0}"), az, el: 15.0 }
            })
            .collect())
        }
        Some(Json::Arr(items)) if !items.is_empty() && items.len() <= MAX_VIEWS => items.iter().enumerate().map(|(i, v)| view_of(v, &format!("views[{i}]"))).collect(),
        Some(_) => Err(format!("'views' is a turntable count, \"shots\", or a list of 1..{MAX_VIEWS} views ({VIEW_HELP})")),
    }
}

fn passes_arg(args: &Json) -> Result<Vec<Pass>, String> {
    match args.get("passes").filter(|v| !v.is_null()) {
        None => Ok(vec![Pass::Colour]),
        Some(Json::Arr(items)) if !items.is_empty() && items.len() <= 3 => items
            .iter()
            .map(|p| match p.as_str() {
                Some("colour" | "color") => Ok(Pass::Colour),
                Some("value") => Ok(Pass::Value),
                Some("silhouette") => Ok(Pass::Silhouette),
                _ => Err("'passes' lists \"colour\", \"value\" and \"silhouette\"".to_string()),
            })
            .collect(),
        Some(_) => Err("'passes' is a list of \"colour\", \"value\", \"silhouette\"".into()),
    }
}

fn mark_arg(args: &Json, key: &str) -> Result<Option<String>, String> {
    match string_arg(args, key)? {
        None => Ok(None),
        Some(m) if !m.is_empty() && m.len() <= 64 && m.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') => Ok(Some(m.to_string())),
        Some(_) => Err(format!("'{key}' is a name (letters, digits, _ and -; at most 64)")),
    }
}

fn sheet_arg(args: &Json, subjects: Vec<SubjectSpec>) -> Result<SheetSpec, String> {
    let sheet = SheetSpec {
        subjects,
        views: views_arg(args, ["front", "left", "three_quarter", "back"].map(|n| View::Angle { label: n.to_string(), az: angle(n).expect("known").0, el: angle(n).expect("known").1 }).to_vec())?,
        passes: passes_arg(args)?,
        rig: rig_arg(args)?.unwrap_or(RigName::Neutral),
        tile: int_arg(args, "tile", 64, 512)?.unwrap_or(192) as u32,
        references: bool_arg(args, "references")?.unwrap_or(false),
    };
    let tiles = sheet.subjects.len().max(1) * sheet.views.len() * sheet.passes.len();
    if tiles > MAX_TILES {
        return Err(format!("{} subjects x {} views x {} passes is {tiles} tiles; at most {MAX_TILES} per sheet (fewer views, passes or subjects)", sheet.subjects.len(), sheet.views.len(), sheet.passes.len()));
    }
    Ok(sheet)
}

/// The studio call `name` names, if it is one of these tools.
pub fn parse(name: &str, args: &Json) -> Option<Result<StudioCall, String>> {
    let format = |args: &Json| Format::arg(args, Format::Concise);
    let r = match name {
        "studio_stage" => (|| {
            check_keys(args, name, &[&["subjects", "recipe", "params", "seed", "label", "view", "rig", "width", "height", "references", "response_format"]])?;
            let subjects = subjects_arg(args, MAX_STAGE_SUBJECTS, false)?;
            let view = match args.get("view").filter(|v| !v.is_null()) {
                Some(v) => view_of(v, "view")?,
                None => View::Shot(Shot::CloseUp),
            };
            // Rounded up to whole 64s, so the studio reuses a few offscreen targets.
            let round = |v: u64, max: u32| (v as u32).div_ceil(64).saturating_mul(64).min(max);
            let size = [round(int_arg(args, "width", 64, 1920)?.unwrap_or(640), 1920), round(int_arg(args, "height", 64, 1080)?.unwrap_or(480), 1080)];
            Ok(StudioCall::Stage { subjects, view, rig: rig_arg(args)?, size, references: bool_arg(args, "references")?.unwrap_or(true), format: format(args)? })
        })(),
        "studio_sheet" => (|| {
            check_keys(args, name, &[&["subjects", "recipe", "params", "seed", "label", "views", "passes", "rig", "tile", "references", "mark", "response_format"]])?;
            let subjects = subjects_arg(args, MAX_SUBJECTS, true)?;
            Ok(StudioCall::Sheet { sheet: sheet_arg(args, subjects)?, mark: mark_arg(args, "mark")?, format: format(args)? })
        })(),
        "studio_lineup" => (|| {
            check_keys(args, name, &[&["subjects", "rig", "width", "response_format"]])?;
            let subjects = subjects_arg(args, MAX_SUBJECTS, true)?;
            if subjects.is_empty() {
                return Err("a line-up needs 'subjects'".into());
            }
            Ok(StudioCall::Lineup { subjects, rig: rig_arg(args)?.unwrap_or(RigName::Neutral), width: int_arg(args, "width", 256, MAX_SHEET_PX as u64)?.unwrap_or(1600) as u32, format: format(args)? })
        })(),
        "studio_compare" => (|| {
            check_keys(args, name, &[&["mark", "before", "after", "views", "passes", "rig", "tile", "references", "response_format"]])?;
            let mark = mark_arg(args, "mark")?;
            let pair = (args.get("before").filter(|v| !v.is_null()), args.get("after").filter(|v| !v.is_null()));
            let from = match (mark, pair) {
                (Some(m), (None, None)) => {
                    for k in ["views", "passes", "rig", "tile", "references"] {
                        if args.get(k).is_some_and(|v| !v.is_null()) {
                            return Err(format!("a mark repeats its own sheet: leave '{k}' out"));
                        }
                    }
                    CompareFrom::Mark(m)
                }
                (None, (Some(b), Some(a))) => CompareFrom::Pair { before: subject_of(b, "before")?, after: subject_of(a, "after")? },
                _ => return Err("studio_compare takes 'mark' (a studio_sheet you marked: rendered again now) or 'before' and 'after' (two subjects now)".into()),
            };
            // The views and passes for a pair (a mark brings its own sheet).
            Ok(StudioCall::Compare { from, sheet: sheet_arg(args, Vec::new())?, format: format(args)? })
        })(),
        "studio_check" => (|| {
            check_keys(args, name, &[&["subject", "recipe", "params", "seed", "label", "siblings", "tile", "image", "response_format"]])?;
            let subject = match args.get("subject").filter(|v| !v.is_null()) {
                Some(s) if args.get("recipe").is_none_or(Json::is_null) => subject_of(s, "subject")?,
                Some(_) => return Err("give 'subject' or 'recipe', not both".into()),
                None => subjects_arg(args, 1, true)?.remove(0),
            };
            let siblings = match args.get("siblings").filter(|v| !v.is_null()) {
                None => None,
                Some(Json::Arr(items)) if items.len() < MAX_SUBJECTS => Some(items.iter().enumerate().map(|(i, s)| subject_of(s, &format!("siblings[{i}]"))).collect::<Result<Vec<_>, _>>()?),
                Some(_) => return Err(format!("'siblings' is a list of at most {} subjects ([] for none; default: the other recipes of its category in its folder)", MAX_SUBJECTS - 1)),
            };
            Ok(StudioCall::Check { subject, siblings, tile: int_arg(args, "tile", 64, 256)?.unwrap_or(128) as u32, image: bool_arg(args, "image")?.unwrap_or(true), format: format(args)? })
        })(),
        "style_check" => (|| {
            check_keys(args, name, &[&["subjects", "recipe", "params", "seed", "label", "folder", "variants", "response_format"]])?;
            let subjects = subjects_arg(args, MAX_STYLE_SUBJECTS, false)?;
            let folder = match string_arg(args, "folder")? {
                Some(f) if !f.is_empty() && f.len() <= 256 && !f.contains("..") && !f.starts_with(['/', '\\']) && !f.contains(':') => Some(f.trim_end_matches(['/', '\\']).to_string()),
                Some(_) => return Err("'folder' is a folder under the assets root, like \"recipes\"".into()),
                None => None,
            };
            if subjects.is_empty() == folder.is_none() {
                return Err("style_check takes 'subjects' (or 'recipe') or 'folder' (every recipe in it), not both".into());
            }
            Ok(StudioCall::Style { subjects, folder, variants: bool_arg(args, "variants")?.unwrap_or(true), format: format(args)? })
        })(),
        _ => return None,
    };
    Some(r)
}

// ---------------------------------------------------------------------------------------------
// Calls

/// A studio tool, on the caller's connection thread: the call runs here (staging, measures,
/// rules, sheets, encoding) and only its renders go to the engine thread (see [`crate::studio`]).
pub fn call(tools: &Tools, session: &SessionInfo, name: &str, args: &Json) -> ToolOutput {
    let call = match parse(name, args) {
        Some(Ok(c)) => c,
        Some(Err(e)) => return ToolOutput::error(argument_error(&e).to_pretty()),
        None => return ToolOutput::error(argument_error(&format!("unknown tool '{name}'")).to_pretty()),
    };
    match tools.studio() {
        Some(shared) => crate::studio::run(tools, shared, session, call),
        None => ToolOutput::error(Json::obj([("error", Json::from("studio_unavailable")), ("message", Json::from("this server has no asset studio"))]).to_string()),
    }
}

// ---------------------------------------------------------------------------------------------
// Tool definitions

pub fn definitions() -> Vec<ToolDef> {
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::arr(required.iter().map(|r| Json::from(*r))))]);
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let any = |d: &str| Json::obj([("description", Json::from(d))]);
    let fmt = || ("response_format", Format::schema());
    let subj = "subjects: recipe paths under the assets root (\"recipes/warding_stone.json\", or a .glb/.gltf) or {recipe, params: {tier: 3}, seed, label}";
    let one = || {
        vec![
            ("recipe", t("string", "One subject: its recipe path (instead of 'subjects')")),
            ("params", t("object", "Recipe parameters for 'recipe', like {\"tier\": 3}")),
            ("seed", t("integer", "Variant seed for 'recipe'")),
            ("label", t("string", "How sheets name 'recipe' (default: its name and parameters)")),
        ]
    };
    let views = "Columns: a turntable count (8), \"shots\" (the shot set: close_up, gameplay, worst_light, phone, igpu), or a list of views: front, back, left, right, three_quarter, top, a shot name, or {az, el} degrees (az 0 = the model's front, +Z; 90 = its left, +X). Default front, left, three_quarter, back";
    let stage_note = "On the asset studio's isolated stage (never the world: no claims, nothing journaled): a ground plane and backdrop, fixed light rigs (neutral: white key over 18% grey; game: the game's light from style.toml [stage.game]; worst: the game light from behind the subject with less fill), and scale references (a 1.8 m figure and a 1 m cube). Builds are waited for. Image plus a packet saved in the captures folder (image.packet_path).";
    vec![
        ToolDef {
            name: "studio_stage".into(),
            description: format!(
                "Renders 1..6 subjects side by side on the stage from one view or shot, with the scale references beside them (references: false to hide). {stage_note} \
                 Shots: close_up (three-quarter, framed to fit, neutral light, LOD 0), gameplay (the game's default camera from style.toml [shots.gameplay], at true pixel size, \
                 LOD by screen size), worst_light (gameplay camera, worst light), phone (gameplay camera on an 854x480 screen), igpu (gameplay camera at 1280x720, low shadows, \
                 no bloom). With no subjects: the stage's description (rigs, shots, references, rules source)."
            ),
            input_schema: schema(
                {
                    let mut p = vec![("subjects", t("array", subj))];
                    p.extend(one());
                    p.extend([
                        ("view", any("A view or shot (default close_up): front, back, left, right, three_quarter, top, close_up, gameplay, worst_light, phone, igpu, or {az, el}")),
                        ("rig", t("string", "neutral, game or worst (default: the shot's own; neutral for angles)")),
                        ("width", t("integer", "Image width, 64..1920, rounded up to a multiple of 64 (default 640)")),
                        ("height", t("integer", "Image height, 64..1080, rounded up to a multiple of 64 (default 480)")),
                        ("references", t("boolean", "Show the 1.8 m figure and the 1 m cube (default true)")),
                        fmt(),
                    ]);
                    p
                },
                &[],
            ),
        },
        ToolDef {
            name: "studio_sheet".into(),
            description: format!(
                "Contact sheet: one row per subject (1..12), one column per view and pass (colour; value = greyscale lightness; silhouette = black on white), at most 96 tiles. \
                 {stage_note} The packet has a record per tile: subject, view, pass, tile box, the subject's box in the tile, its height in pixels, lightness, LOD, and per \
                 shot its measures (px_height in the shot's viewport, contrast against the ring around it). Concise: tiles and style.toml errors per subject; detailed: \
                 every measure and check. mark: keep this sheet (pixels and numbers) under a name for studio_compare after you edit the recipe."
            ),
            input_schema: schema(
                {
                    let mut p = vec![("subjects", t("array", subj))];
                    p.extend(one());
                    p.extend([
                        ("views", any(views)),
                        ("passes", t("array", "Any of colour, value, silhouette (default [colour])")),
                        ("rig", t("string", "Light for angle views: neutral (default), game or worst (shots bring their own)")),
                        ("tile", t("integer", "Tile side, 64..512 (default 192)")),
                        ("references", t("boolean", "Draw the 1.8 m figure and 1 m cube in angle tiles (default false)")),
                        ("mark", t("string", "Keep this sheet under this name for studio_compare (8 per session)")),
                        fmt(),
                    ]);
                    p
                },
                &[],
            ),
        },
        ToolDef {
            name: "studio_lineup".into(),
            description: format!(
                "Line-up: subjects (1..12) side by side at true relative scale beside the 1.8 m figure and the 1 m cube, front and three-quarter, on one image. {stage_note} \
                 The packet gives each subject's height (m and as a ratio to the figure), its box in each view, and the silhouette distinctness matrix (IoU of 64 px \
                 silhouettes fitted to their own boxes, mean of front, side and three-quarter; over style.toml [taste] silhouette_max_iou reads as the same thing)."
            ),
            input_schema: schema(vec![("subjects", t("array", subj)), ("rig", t("string", "neutral (default), game or worst")), ("width", t("integer", "Image width, 256..2048 (default 1600)")), fmt()], &["subjects"]),
        },
        ToolDef {
            name: "studio_compare".into(),
            description: "Before/after on the stage: 'mark' (a studio_sheet you kept with mark: rendered again now, after your edit) or 'before' and 'after' (two subjects \
                          now: two parameter sets, two recipe files) with the sheet arguments. The image is before over after; the answer has per tile the changed pixel share \
                          and lightness change, the facts that changed (triangles, size), and the style.toml findings fixed, new and still failing."
                .into(),
            input_schema: schema(
                vec![
                    ("mark", t("string", "A mark you made with studio_sheet {mark}")),
                    ("before", any("The 'before' subject: a recipe path or {recipe, params, seed, label}")),
                    ("after", any("The 'after' subject")),
                    ("views", any(views)),
                    ("passes", t("array", "Any of colour, value, silhouette (default [colour])")),
                    ("rig", t("string", "neutral (default), game or worst")),
                    ("tile", t("integer", "Tile side, 64..512 (default 192)")),
                    ("references", t("boolean", "Draw the scale references (default false)")),
                    fmt(),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "studio_check".into(),
            description: format!(
                "The taste loop in one call: the subject and its siblings (default: the other recipes of its category in its folder; [] for none) through the silhouette, \
                 value and colour passes at thumbnail size and the shot set, then every style.toml rule and taste check, reported like budget errors: \
                 silhouette_distinct (IoU with the closest sibling), thumb_solidity (thin parts vanish at 64 px), px_gameplay / px_phone (height in pixels from the game \
                 camera), value_contrast / value_contrast_worst (lightness against the ground around it, normal and backlit), flat_lighting (lightness spread under the game \
                 light), plus budgets, scale, palette distance, neutral chroma, forbidden families, emissive presets and naming. Each error has the measured value, the \
                 limit, the rule's path in style.toml and what to change; 'critique' groups them by pass with the bible's rules for the category and the revise hints in \
                 order. Revise the recipe and call again (or studio_sheet {{mark}} and studio_compare). The sheet: one row per subject (silhouettes, value, colour, gameplay, \
                 worst light, phone). {stage_note}"
            ),
            input_schema: schema(
                {
                    let mut p = vec![("subject", any("The subject: a recipe path or {recipe, params, seed, label} (or give 'recipe')"))];
                    p.extend(one());
                    p.extend([
                        ("siblings", t("array", "Siblings to compare silhouettes with (default: same category, same folder; [] for none)")),
                        ("tile", t("integer", "Sheet tile side, 64..256 (default 128)")),
                        ("image", t("boolean", "Return the sheet image (default true; the packet is saved either way)")),
                        fmt(),
                    ]);
                    p
                },
                &[],
            ),
        },
        ToolDef {
            name: "style_check".into(),
            description: "The game's style.toml rules (found next to the recipe or above it in the assets root) on recipes, without rendering: triangle and texture \
                          budgets, material count, scale ranges and aspect (height over footprint), palette distance (Oklab, from the nearest blend of style.json palette \
                          colours), neutral chroma (saturation belongs to the families), forbidden families per category, emissive presets, and naming (snake_case, length, \
                          file name, category, named parts). A rule broken is an error with the measured value, the limit, the rule's path and a fix. 'folder' checks every \
                          recipe in a folder; 'variants' (default true) checks each small integer parameter (a stone's tiers) at every value. Concise: errors per subject; \
                          detailed: every check and the facts measured."
                .into(),
            input_schema: schema(
                {
                    let mut p = vec![("subjects", t("array", subj))];
                    p.extend(one());
                    p.extend([("folder", t("string", "Check every recipe in this folder under the assets root (\"recipes\")")), ("variants", t("boolean", "Every value of small integer parameters (default true)")), fmt()]);
                    p
                },
                &[],
            ),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(name: &str, args: &str) -> StudioCall {
        parse(name, &Json::parse(args).unwrap()).unwrap().unwrap_or_else(|e| panic!("{name} {args}: {e}"))
    }

    fn bad(name: &str, args: &str) -> String {
        parse(name, &Json::parse(args).unwrap()).unwrap().expect_err("should fail")
    }

    #[test]
    fn subjects_views_and_bounds() {
        let StudioCall::Sheet { sheet, mark, .. } = ok("studio_sheet", r#"{"subjects": ["recipes/a.json", {"recipe": "recipes/b.json", "params": {"tier": 3}, "label": "B3"}], "views": 8, "passes": ["colour", "silhouette"], "mark": "v1"}"#) else { panic!() };
        assert_eq!((sheet.subjects.len(), sheet.views.len(), sheet.passes.len(), mark.as_deref()), (2, 8, 2, Some("v1")));
        assert_eq!(sheet.subjects[1].spec.params, vec![("tier".to_string(), 3.0)]);
        let StudioCall::Sheet { sheet, .. } = ok("studio_sheet", r#"{"recipe": "recipes/a.json", "views": "shots"}"#) else { panic!() };
        assert_eq!(sheet.views.len(), 5);
        assert!(bad("studio_sheet", r#"{"subjects": ["a.json","b.json","c.json","d.json","e.json","f.json","g.json"], "views": 16, "passes": ["colour"]}"#).contains("at most 96"));
        assert!(bad("studio_sheet", r#"{"recipe": "../x.json"}"#).contains("recipe"));
        assert!(bad("studio_sheet", r#"{"recipe": "a.json", "views": ["sideways"]}"#).contains("a view is"));
        assert!(bad("studio_sheet", r#"{"recipe": "a.json", "subjects": ["b.json"]}"#).contains("not both"));
        assert!(bad("studio_sheet", "{}").contains("required"));
    }

    #[test]
    fn compare_check_and_style() {
        assert!(matches!(ok("studio_compare", r#"{"mark": "v1"}"#), StudioCall::Compare { from: CompareFrom::Mark(_), .. }));
        assert!(bad("studio_compare", r#"{"mark": "v1", "views": 4}"#).contains("its own sheet"));
        assert!(matches!(ok("studio_compare", r#"{"before": "recipes/a.json", "after": {"recipe": "recipes/a.json", "params": {"tier": 2}}}"#), StudioCall::Compare { from: CompareFrom::Pair { .. }, .. }));
        assert!(bad("studio_compare", r#"{"before": "recipes/a.json"}"#).contains("'before' and 'after'"));
        let StudioCall::Check { siblings, tile, .. } = ok("studio_check", r#"{"recipe": "recipes/a.json", "siblings": []}"#) else { panic!() };
        assert_eq!((siblings.map(|s| s.len()), tile), (Some(0), 128));
        assert!(matches!(ok("style_check", r#"{"folder": "recipes/"}"#), StudioCall::Style { folder: Some(ref f), .. } if f == "recipes"));
        assert!(bad("style_check", r#"{"folder": "../x"}"#).contains("folder"));
        assert!(bad("style_check", "{}").contains("not both"));
        assert!(matches!(ok("studio_stage", "{}"), StudioCall::Stage { ref subjects, .. } if subjects.is_empty()));
        assert!(bad("studio_stage", r#"{"rig": "noon"}"#).contains("rig"));
        assert!(parse("studio_other", &Json::Null).is_none());
    }
}
