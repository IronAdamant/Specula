//! Agent tools for procedural tiers 2-4 (docs/design/PROCEDURAL_MODELLING.md): `procgen_texture`
//! (bake a texture graph and look at it), `procgen_terrain` (build a terrain, report it, put its
//! heightfield collider on an entity) and `procgen_scatter` (Poisson-disc placements with rules,
//! optionally spawned as model entities). Booleans, SDF sculpts and buildings are recipe shapes
//! and generators: `model_build` builds them on the model workers.
//!
//! Like the Lex tools, these work on the calling agent's protocol thread, never the engine
//! thread; at most [`MAX_CONCURRENT`] run at once (the rest answer `busy`). Whatever changes the
//! world goes through `world_set` / `world_batch`, so it is journaled, claim-checked and undoable
//! like any agent edit. Every answer is bounded (placements listed up to [`MAX_LISTED`], images
//! at most 512 px wide).

use crate::tools::{check_keys, Tools};
use specula_core::Json;
use specula_model::scatter::{self, Placement, ScatterSpec};
use specula_model::texgraph::{self, TexGraph};
use specula_model::{BuildParams, Model, RecipeError, Style};
use specula_protocol::{Content, SessionInfo, ToolDef, ToolOutput};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

/// Procgen calls running at once (all agents together).
pub const MAX_CONCURRENT: usize = 2;
/// Placements listed in an answer, and spawned in one call.
pub const MAX_LISTED: usize = 32;
pub const MAX_SPAWN: usize = 1000;
/// Files a recipe folder may hold for the tools, bytes per file and in all.
const MAX_FOLDER_FILES: usize = 512;
const MAX_FILE_BYTES: u64 = 1 << 20;
const MAX_FOLDER_BYTES: usize = 16 << 20;

static BUSY: AtomicUsize = AtomicUsize::new(0);

struct Slot;

impl Slot {
    fn take() -> Option<Slot> {
        let mut n = BUSY.load(Ordering::SeqCst);
        loop {
            if n >= MAX_CONCURRENT {
                return None;
            }
            match BUSY.compare_exchange(n, n + 1, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => return Some(Slot),
                Err(m) => n = m,
            }
        }
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        BUSY.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Where the tools find recipes and styles and write images.
#[derive(Clone, Debug)]
pub struct Context {
    pub assets: PathBuf,
    pub style: Option<PathBuf>,
    pub captures: PathBuf,
}

impl Default for Context {
    fn default() -> Self {
        let cwd = std::env::current_dir().unwrap_or_default();
        Context { assets: cwd.clone(), style: None, captures: cwd.join("captures") }
    }
}

fn err(code: &'static str, message: impl Into<String>, extra: Vec<(&str, Json)>) -> ToolOutput {
    let mut f = vec![("error", Json::from(code)), ("message", Json::from(message.into()))];
    f.extend(extra);
    ToolOutput::error(Json::obj(f).to_string())
}

fn recipe_err(e: &RecipeError, prefix: &str) -> ToolOutput {
    let mut j = e.to_json();
    if let Json::Obj(pairs) = &mut j {
        for (k, v) in pairs.iter_mut() {
            if k == "path" {
                if let Json::Str(p) = v {
                    *p = format!("{prefix}{p}");
                }
            }
        }
    }
    err("invalid_recipe", e.to_string(), vec![("detail", j)])
}

/// A relative path inside the assets root (no `..`, not absolute, and no link leading out).
fn inside(ctx: &Context, rel: &str, field: &str) -> Result<PathBuf, ToolOutput> {
    let p = Path::new(rel);
    let bad = || err("invalid_argument", format!("'{field}' must be a file under the assets root, as a relative path without '..' (got {rel})"), vec![("field", Json::from(field))]);
    if rel.is_empty() || p.is_absolute() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(bad());
    }
    let full = ctx.assets.join(p);
    // Links resolved: the file must really be inside the root.
    match (ctx.assets.canonicalize(), full.canonicalize()) {
        (Ok(root), Ok(real)) if real.starts_with(&root) => Ok(full),
        (Ok(_), Ok(_)) => Err(bad()),
        _ => Err(err("file_not_found", format!("'{field}': {rel} doesn't exist under the assets root"), vec![("field", Json::from(field))])),
    }
}

/// A list of `width`-number rows (`[[x, z], ...]`), each number finite; a structured error
/// naming the field otherwise.
fn rows(v: &Json, field: &str, width: usize, max: usize) -> Result<Vec<Vec<f32>>, ToolOutput> {
    let bad = |m: String| err("invalid_argument", m, vec![("field", Json::from(field))]);
    let list = v.as_array().ok_or_else(|| bad(format!("'{field}' must be a list of [{}] rows", if width == 2 { "x, z" } else { "x, z, radius" })))?;
    if list.len() > max {
        return Err(err("limit", format!("'{field}' has {} entries; at most {max}", list.len()), vec![("field", Json::from(field))]));
    }
    list.iter()
        .enumerate()
        .map(|(i, r)| match r.as_array() {
            Some(a) if a.len() == width && a.iter().all(|x| x.as_f64().is_some_and(f64::is_finite)) => Ok(a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()),
            _ => Err(bad(format!("'{field}[{i}]' must be {width} finite numbers"))),
        })
        .collect()
}

fn read_small(p: &Path) -> Result<String, String> {
    let meta = std::fs::symlink_metadata(p).map_err(|e| format!("{}: {e}", p.display()))?;
    if !meta.is_file() {
        return Err(format!("{} is not a regular file", p.display()));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!("{} is larger than {MAX_FILE_BYTES} bytes", p.display()));
    }
    std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))
}

/// The style for a file in the assets root: `--style`, or the first style.json walking up.
fn style_for(ctx: &Context, file: &Path) -> Result<Style, ToolOutput> {
    let path = match &ctx.style {
        Some(s) => s.clone(),
        None => {
            let mut dir = file.parent().map(Path::to_path_buf);
            let mut found = None;
            while let Some(d) = dir {
                let c = d.join(crate::models::STYLE_FILE);
                if c.is_file() {
                    found = Some(c);
                    break;
                }
                if d == ctx.assets || !d.starts_with(&ctx.assets) {
                    break;
                }
                dir = d.parent().map(Path::to_path_buf);
            }
            found.ok_or_else(|| err("style_not_found", format!("no {} next to {} or above it in the assets root", crate::models::STYLE_FILE, file.display()), vec![]))?
        }
    };
    let text = read_small(&path).map_err(|m| err("style_unreadable", m, vec![]))?;
    Style::parse(&text).map_err(|e| recipe_err(&e, "style."))
}

/// A recipe's folder as an include library (file stem -> text).
fn library(file: &Path) -> BTreeMap<String, String> {
    let mut lib = BTreeMap::new();
    let Some(dir) = file.parent() else { return lib };
    let Ok(entries) = std::fs::read_dir(dir) else { return lib };
    let mut names: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    names.sort();
    let mut bytes = 0usize;
    for p in names.into_iter().take(MAX_FOLDER_FILES) {
        if let (Some(stem), Ok(text)) = (p.file_stem().and_then(|s| s.to_str()), read_small(&p)) {
            bytes += text.len();
            if bytes > MAX_FOLDER_BYTES {
                break;
            }
            lib.insert(stem.to_string(), text);
        }
    }
    lib
}

fn params_of(args: &Json, field: &str) -> Result<Vec<(String, f64)>, ToolOutput> {
    match args.get(field) {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(Json::Obj(p)) => p
            .iter()
            .map(|(k, v)| v.as_f64().filter(|x| x.is_finite()).map(|x| (k.clone(), x)).ok_or_else(|| err("invalid_argument", format!("'{field}.{k}' must be a number"), vec![])))
            .collect(),
        Some(_) => Err(err("invalid_argument", format!("'{field}' must be an object of numbers"), vec![])),
    }
}

/// Builds a recipe file (with its folder as library) for the tools.
fn build_recipe_file(ctx: &Context, spec: &Json, field: &str) -> Result<Model, ToolOutput> {
    let rel = spec.get("recipe").and_then(Json::as_str).ok_or_else(|| err("invalid_argument", format!("'{field}.recipe' must name a recipe file (e.g. recipes/ground.json)"), vec![]))?;
    let file = inside(ctx, rel, &format!("{field}.recipe"))?;
    let text = read_small(&file).map_err(|m| err("recipe_not_found", m, vec![]))?;
    let style = style_for(ctx, &file)?;
    let lib = library(&file);
    let kinds = crate::exemplar_tools::FolderKinds { library: &lib, from: rel };
    let vals = params_of(spec, "params")?;
    let refs: Vec<(&str, f64)> = vals.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let seed = match spec.get("seed") {
        Some(v) => Some(v.as_u64().ok_or_else(|| err("invalid_argument", format!("'{field}.seed' must be a whole number"), vec![]))?),
        None => None,
    };
    specula_model::build(&text, &BuildParams { values: &refs, seed, library: Some(&lib), specimens: Some(&kinds), ..BuildParams::new(&style) }).map_err(|e| recipe_err(&e, &format!("{rel}: ")))
}

/// A small PNG as MCP image content (at most `max_w` wide, nearest sampling).
fn image(w: u32, h: u32, rgba: &[u8], max_w: u32) -> Content {
    let k = w.div_ceil(max_w).max(1);
    let (ow, oh) = (w / k, h / k);
    let mut out = Vec::with_capacity((ow * oh * 4) as usize);
    for y in 0..oh {
        for x in 0..ow {
            let i = (((y * k) * w + x * k) * 4) as usize;
            out.extend_from_slice(&rgba[i..i + 4]);
        }
    }
    let png = specula_core::png::encode_rgba8(ow, oh, &out, specula_core::deflate::Level::Fast);
    Content::Image { mime_type: "image/png".into(), data_base64: specula_core::base64::encode(&png) }
}

fn save_png(ctx: &Context, name: &str, w: u32, h: u32, rgba: &[u8]) -> Result<PathBuf, String> {
    let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    let path = ctx.captures.join(format!("{safe}.png"));
    let png = specula_core::png::encode_rgba8(w, h, rgba, specula_core::deflate::Level::Fast);
    crate::disk::book().write(crate::disk::Category::Captures, "procgen", &path, &png, crate::disk::Need::Agent).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Answers a `procgen_*` call (on the calling agent's protocol thread).
pub fn call(tools: &Tools, session: &SessionInfo, name: &str, args: &Json) -> ToolOutput {
    let Some(_slot) = Slot::take() else {
        return err("busy", format!("{MAX_CONCURRENT} procgen calls are running; try again in a moment"), vec![]);
    };
    let ctx = &tools.procgen;
    match name {
        "procgen_texture" => texture(ctx, args),
        "procgen_terrain" => terrain(tools, session, ctx, args),
        "procgen_scatter" => scatter_tool(tools, session, ctx, args),
        other => err("unknown_tool", format!("unknown procgen tool '{other}'"), vec![]),
    }
}

// ---------------------------------------------------------------------------------------------
// procgen_texture

fn texture(ctx: &Context, args: &Json) -> ToolOutput {
    if let Err(m) = check_keys(args, "procgen_texture", &[&["graph", "file", "size", "save", "name"]]) {
        return err("invalid_argument", m, vec![]);
    }
    let started = Instant::now();
    let (json, style, label) = match (args.get("graph"), args.get("file").and_then(Json::as_str)) {
        (Some(g), None) => {
            // Inline: the style of the assets root's recipes folder (or --style).
            let probe = ctx.assets.join("recipes").join("_.json");
            let style = match style_for(ctx, &probe) {
                Ok(s) => s,
                Err(_) => match style_for(ctx, &ctx.assets.join("_.json")) {
                    Ok(s) => s,
                    Err(e) => return e,
                },
            };
            (g.clone(), style, "graph".to_string())
        }
        (None, Some(rel)) => {
            let file = match inside(ctx, rel, "file") {
                Ok(f) => f,
                Err(e) => return e,
            };
            let text = match read_small(&file) {
                Ok(t) => t,
                Err(m) => return err("file_not_found", m, vec![]),
            };
            let json = match Json::parse(&text) {
                Ok(j) => j,
                Err(e) => return err("invalid_json", format!("{rel}: {}", e.message), vec![("offset", Json::from(e.offset))]),
            };
            match style_for(ctx, &file) {
                Ok(s) => (json, s, rel.to_string()),
                Err(e) => return e,
            }
        }
        _ => return err("invalid_argument", "give 'graph' (an inline texture graph) or 'file' (a texture graph file in the assets root)", vec![]),
    };
    let mut graph = match TexGraph::parse_json(&json, &style, "") {
        Ok(g) => g,
        Err(e) => return recipe_err(&e, ""),
    };
    if let Some(s) = args.get("size") {
        match s.as_u64().filter(|v| (texgraph::MIN_SIZE as u64..=texgraph::MAX_SIZE as u64).contains(v) && v.is_power_of_two()) {
            Some(v) => graph.size = v as u32,
            None => return err("invalid_argument", format!("'size' is a power of two {}..{}", texgraph::MIN_SIZE, texgraph::MAX_SIZE), vec![("field", Json::from("size"))]),
        }
    }
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 4);
    let set = match texgraph::bake(&graph, threads) {
        Ok(s) => s,
        Err(e) => return recipe_err(&e, ""),
    };
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let (w, h, strip) = set.strip();
    let mean = |img: &[u8], c: usize| img.chunks(4).map(|p| p[c] as f64).sum::<f64>() / (img.len() / 4) as f64 / 255.0;
    let mut fields = vec![
        ("name", Json::from(graph.name.as_str())),
        ("source", Json::from(label)),
        ("size", Json::from(set.size)),
        ("gpu_bytes", Json::from(set.gpu_bytes())),
        ("hash", Json::from(format!("{:016x}", set.hash))),
        ("work", Json::from(set.work)),
        ("bake_ms", Json::Num((ms * 10.0).round() / 10.0)),
        ("mean_base_srgb", Json::arr([0, 1, 2].map(|c| Json::Num((mean(&set.base_color, c) * 1000.0).round() / 1000.0)))),
        ("mean_roughness", Json::Num((mean(&set.metallic_roughness, 1) * 1000.0).round() / 1000.0)),
        ("mean_metallic", Json::Num((mean(&set.metallic_roughness, 2) * 1000.0).round() / 1000.0)),
        ("normal_map", Json::from(set.normal.is_some())),
        ("image", Json::from("base colour | normal | roughness | metallic")),
    ];
    if args.get("save").and_then(Json::as_bool).unwrap_or(false) {
        let name = args.get("name").and_then(Json::as_str).unwrap_or(&graph.name).to_string();
        match save_png(ctx, &format!("texture_{name}"), w, h, &strip) {
            Ok(p) => fields.push(("saved", Json::from(p.display().to_string()))),
            Err(m) => fields.push(("save_failed", Json::from(m))),
        }
    }
    ToolOutput { content: vec![Content::Text(Json::obj(fields).to_string()), image(w, h, &strip, 512)], is_error: false }
}

// ---------------------------------------------------------------------------------------------
// procgen_terrain

/// How a terrain model's heights sit in the model: the move and the per-axis scale its part's
/// ops (and the recipe's `finish`) gave it, or why its heights can't be used (rotated, mirrored
/// or reshaped).
fn terrain_frame(m: &Model, field: &str) -> Result<([f32; 3], [f32; 3]), ToolOutput> {
    let Some(a) = m.extras.terrain_transform else {
        return Err(err(
            "terrain_reshaped",
            format!("'{}': an op on the terrain part reshapes it (displace, bend, twist, ...), so its heights no longer describe the mesh; keep the terrain part to colour ops, move and scale", m.name),
            vec![("field", Json::from(field))],
        ));
    };
    let c = a.cols;
    let s = [c[0][0], c[1][1], c[2][2]];
    if [c[0][1], c[0][2], c[1][0], c[1][2], c[2][0], c[2][1]].iter().any(|v| v.abs() > 1e-6) || s.iter().any(|v| *v <= 0.0) {
        return Err(err(
            "terrain_rotated",
            format!("'{}': the terrain part's ops rotate or mirror it, which a heightfield can't follow; rotate the entity instead (world_set rotation_deg)", m.name),
            vec![("field", Json::from(field))],
        ));
    }
    Ok((a.t, s))
}

/// `procgen_terrain`'s collider: one `world_set` of a heightfield collider on `path` (the engine
/// commits it as the calling agent, `Rule::World`). Kept apart so the write policy's regression
/// table commits exactly what the tool sends.
pub(crate) fn collider_command(path: &str, cols: usize, rows: usize, cell: [f32; 2], heights: &[f32], offset: [f32; 3]) -> Result<crate::tools::Command, String> {
    let set = Json::obj([
        ("path", Json::from(path)),
        (
            "collider",
            Json::obj([
                ("shape", Json::from("heightfield")),
                ("columns", Json::from(cols)),
                ("rows", Json::from(rows)),
                ("cell", Json::arr(cell.map(|v| Json::Num(v as f64)))),
                ("heights", Json::arr(heights.iter().map(|h| Json::Num(*h as f64)))),
                ("offset", Json::arr(offset.map(|v| Json::Num(v as f64)))),
            ]),
        ),
    ]);
    crate::tools::parse("world_set", &set)
}

/// terrain_edit's collider: the heightfield collider `procgen_terrain` would put on `entity` for
/// the recipe at `rel` (under the assets root) if its text were `text`, built with the entity's
/// model `params` and `seed` (its style and include folder as the file's). Nothing is sent: the
/// change is committed with the recipe ([`crate::project_tools::patch_with`]). Returns the
/// world_set change and its summary.
pub(crate) fn collider_for_text(ctx: &Context, rel: &str, text: &str, params: &Json, seed: Option<u64>, entity: &str) -> Result<(crate::world::EntityKey, crate::world::UpdateSpec, Json), ToolOutput> {
    let Some(_slot) = Slot::take() else {
        return Err(err("busy", format!("{MAX_CONCURRENT} procgen calls are running; try again in a moment (nothing was changed)"), vec![]));
    };
    let file = inside(ctx, rel, "recipe")?;
    let style = style_for(ctx, &file)?;
    let lib = library(&file);
    let kinds = crate::exemplar_tools::FolderKinds { library: &lib, from: rel };
    let vals = params_of(&Json::obj([("params", params.clone())]), "params")?;
    let refs: Vec<(&str, f64)> = vals.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let model = specula_model::build(text, &BuildParams { values: &refs, seed, library: Some(&lib), specimens: Some(&kinds), ..BuildParams::new(&style) }).map_err(|e| recipe_err(&e, &format!("{rel}: ")))?;
    let Some(t) = model.extras.terrain.as_ref() else {
        return Err(err("no_terrain", format!("'{}' has no terrain part", model.name), vec![]));
    };
    let (offset, scale) = terrain_frame(&model, "collider")?;
    let (cols, rows, heights, cell) = t.physics_heightfield(COLLIDER_SIDE);
    let cell = [cell[0] * scale[0], cell[1] * scale[2]];
    let heights: Vec<f32> = heights.iter().map(|h| h * scale[1]).collect();
    let (lo, hi) = heights.iter().fold((f32::MAX, f32::MIN), |(a, b), h| (a.min(*h), b.max(*h)));
    match collider_command(entity, cols, rows, cell, &heights, offset) {
        Ok(crate::tools::Command::Set(key, change)) => Ok((
            key,
            change,
            Json::obj([
                ("entity", Json::from(entity)),
                ("columns", Json::from(cols)),
                ("rows", Json::from(rows)),
                ("cell", Json::arr(cell.map(|v| Json::Num(v as f64)))),
                ("offset", Json::arr(offset.map(|v| Json::Num(v as f64)))),
                ("height_range_m", Json::arr([Json::Num(lo as f64), Json::Num(hi as f64)])),
            ]),
        )),
        Ok(_) => Err(err("internal", "the collider isn't one world_set", vec![])),
        Err(m) => Err(err("invalid_argument", m, vec![("field", Json::from("collider"))])),
    }
}

/// `procgen_scatter`'s spawns: ONE `world_batch` with a model spawn per placement (each item's
/// params sampled per placement), committed as the calling agent (`Rule::World`, all or nothing:
/// a claimed region or reserved parent refuses the whole batch). `spawn` is `{parent?, name?}`.
/// `integers[i]` names item `i`'s recipe parameters declared `integer` (a sampled range lands on
/// whole numbers there; missing = none).
pub(crate) fn spawn_command(items: &[Json], integers: &[BTreeSet<String>], places: &[Placement], at: [f32; 3], spawn: &Json) -> Result<crate::tools::Command, String> {
    let parent = spawn.get("parent").and_then(Json::as_str);
    let prefix = spawn.get("name").and_then(Json::as_str).unwrap_or("scatter");
    let mut ops = Vec::with_capacity(places.len());
    for (i, p) in places.iter().enumerate() {
        let item = items.get(p.item as usize).ok_or_else(|| format!("placement {i} names item {} of {}", p.item, items.len()))?;
        let model = item.get("model").ok_or_else(|| format!("items[{}] has no model", p.item))?;
        let ints = integers.get(p.item as usize);
        let mut params = Vec::new();
        if let Some(Json::Obj(ps)) = model.get("params") {
            let mut rng = specula_core::rng::Rng::new(p.variant, 91);
            for (k, v) in ps {
                let x = match v.as_array() {
                    Some(r) if r.len() == 2 => {
                        let (a, b) = (r[0].as_f64().unwrap_or(0.0), r[1].as_f64().unwrap_or(0.0));
                        // One draw per range either way, so the other params' samples don't move.
                        let t = rng.f32() as f64;
                        if ints.is_some_and(|s| s.contains(k)) {
                            sample_whole(a, b, t)
                        } else {
                            a + (b - a) * t
                        }
                    }
                    _ => v.as_f64().unwrap_or(0.0),
                };
                params.push((k.clone(), Json::Num(x)));
            }
        }
        let seed = model.get("seed").and_then(Json::as_u64).unwrap_or(p.variant % 1_000_000);
        let world = [p.position[0] + at[0], p.position[1] + at[1], p.position[2] + at[2]];
        let mut op = vec![
            ("op", Json::from("spawn")),
            ("shape", Json::from("model")),
            ("recipe", model.get("recipe").cloned().unwrap_or(Json::Null)),
            ("seed", Json::from(seed)),
            ("name", Json::from(format!("{prefix}_{i}"))),
            ("position", Json::arr(world.map(|v| Json::Num(v as f64)))),
            ("rotation_deg", Json::arr(p.rotation_deg.map(|v| Json::Num(v as f64)))),
            ("scale", Json::Num(p.scale as f64 * item.get("scale").and_then(Json::as_f64).unwrap_or(1.0))),
        ];
        if !params.is_empty() {
            op.push(("params", Json::obj(params)));
        }
        if let Some(par) = parent {
            op.push(("parent", Json::from(par)));
        }
        ops.push(Json::obj(op));
    }
    crate::tools::parse("world_batch", &Json::obj([("ops", Json::arr(ops))]))
}

/// A whole number in `a..=b` (either order; ends rounded inward) from `t` in `0..1`, each with the
/// same chance. A range holding no whole number gives its lower end rounded up (the recipe's
/// range check then says why).
fn sample_whole(a: f64, b: f64, t: f64) -> f64 {
    let (lo, hi) = (a.min(b).ceil(), a.max(b).floor());
    if hi < lo {
        return lo;
    }
    let n = hi - lo + 1.0;
    (lo + (t * n).floor()).min(hi)
}

/// Item by item, its recipe's integer parameters, read only for items that sample a range.
fn integer_params(ctx: &Context, items: &[Json]) -> Result<Vec<BTreeSet<String>>, ToolOutput> {
    let mut out = Vec::with_capacity(items.len());
    for (i, it) in items.iter().enumerate() {
        let model = it.get("model");
        let ranged = matches!(model.and_then(|m| m.get("params")), Some(Json::Obj(ps)) if ps.iter().any(|(_, v)| v.as_array().is_some()));
        let Some(rel) = model.and_then(|m| m.get("recipe")).and_then(Json::as_str).filter(|_| ranged) else {
            out.push(BTreeSet::new());
            continue;
        };
        let field = format!("items[{i}].model.recipe");
        let file = inside(ctx, rel, &field)?;
        let text = read_small(&file).map_err(|m| err("recipe_not_found", m, vec![("field", Json::from(field.as_str()))]))?;
        let style = style_for(ctx, &file)?;
        let recipe = specula_model::Recipe::parse(&text, &style).map_err(|e| recipe_err(&e, &format!("{rel}: ")))?;
        out.push(recipe.params.iter().filter(|p| p.integer).map(|p| p.name.clone()).collect());
    }
    Ok(out)
}

/// An optional whole-number argument.
fn whole(args: &Json, field: &str) -> Result<Option<u64>, ToolOutput> {
    match args.get(field) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => v.as_u64().map(Some).ok_or_else(|| err("invalid_argument", format!("'{field}' must be a whole number >= 0"), vec![("field", Json::from(field))])),
    }
}

/// The engine's world heightfield colliders hold at most this many samples per side.
const COLLIDER_SIDE: usize = crate::physics::MAX_HEIGHTFIELD_SIDE as usize;

fn terrain(tools: &Tools, session: &SessionInfo, ctx: &Context, args: &Json) -> ToolOutput {
    if let Err(m) = check_keys(args, "procgen_terrain", &[&["recipe", "params", "seed", "terrain", "collider", "max_slope", "preview"]]) {
        return err("invalid_argument", m, vec![]);
    }
    let started = Instant::now();
    let seed = match whole(args, "seed") {
        Ok(s) => s,
        Err(e) => return e,
    };
    let model = if let Some(spec) = args.get("terrain") {
        // An inline spec: built as a one-part recipe so it is checked exactly like one.
        let probe = ctx.assets.join("recipes").join("_.json");
        let style = match style_for(ctx, &probe).or_else(|_| style_for(ctx, &ctx.assets.join("_.json"))) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let Some(mat) = style.materials.keys().next().cloned() else { return err("style_invalid", "the style has no material presets", vec![]) };
        let mut gen = match spec {
            Json::Obj(p) => p.clone(),
            _ => return err("invalid_argument", "'terrain' is a terrain generator's fields (size, samples, layers, ...)", vec![("field", Json::from("terrain"))]),
        };
        gen.insert(0, ("op".into(), Json::from("terrain")));
        let recipe = Json::obj([
            ("name", Json::from("terrain")),
            ("seed", Json::from(seed.unwrap_or(0))),
            ("parts", Json::arr([Json::obj([("name", Json::from("ground")), ("material", Json::from(mat)), ("generate", Json::Obj(gen))])])),
        ]);
        match specula_model::build(&recipe.to_string(), &BuildParams::new(&style)) {
            Ok(m) => m,
            Err(e) => {
                let mut e2 = e.clone();
                e2.path = e.path.replace("parts[0].generate", "terrain");
                return recipe_err(&e2, "");
            }
        }
    } else if args.get("recipe").is_some() {
        match build_recipe_file(ctx, args, "") {
            Ok(m) => m,
            Err(e) => return e,
        }
    } else {
        return err("invalid_argument", "give 'recipe' (a recipe with a terrain part) or 'terrain' (a terrain generator's fields)", vec![]);
    };
    let Some(t) = model.extras.terrain.clone() else {
        return err("no_terrain", format!("'{}' has no terrain part (a part with \"generate\": {{\"op\": \"terrain\", ...}})", model.name), vec![]);
    };
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    // The same default as nav_build's agents.
    let max_slope = match num(args.get("max_slope"), "max_slope", 45.0, 0.0, 90.0) {
        Ok(v) => v as f32,
        Err(e) => return e,
    };
    let walk = t.walkable_cells(max_slope);
    let walkable = walk.iter().filter(|w| **w).count() as f64 / walk.len().max(1) as f64;
    let (lo, hi) = t.heights.iter().fold((f32::MAX, f32::MIN), |(a, b), h| (a.min(*h), b.max(*h)));
    let mut coverage = Vec::new();
    for (k, n) in t.splat_names.iter().enumerate() {
        let share = t.splat.iter().map(|w| w[k] as f64).sum::<f64>() / (t.splat.len() as f64 * 255.0);
        coverage.push((n.clone(), Json::Num((share * 1000.0).round() / 1000.0)));
    }
    let mut fields = vec![
        ("samples", Json::arr([Json::from(t.columns), Json::from(t.rows)])),
        ("cell_m", Json::arr(t.cell.map(|v| Json::Num(v as f64)))),
        ("size_m", Json::arr(t.size().map(|v| Json::Num(v as f64)))),
        ("height_range_m", Json::arr([Json::Num(lo as f64), Json::Num(hi as f64)])),
        ("splat_coverage", Json::obj(coverage)),
        ("walkable_share", Json::Num((walkable * 1000.0).round() / 1000.0)),
        ("max_slope_deg", Json::Num(max_slope as f64)),
        ("model_hash", Json::from(format!("{:016x}", model.hash))),
        ("triangles", Json::from(model.lods[0].triangle_count())),
        ("work", Json::from(t.work)),
        ("build_ms", Json::Num((ms * 10.0).round() / 10.0)),
        (
            "convention",
            Json::from("centred on the model's origin; sample (x, z) at ((x - (columns-1)/2) cell.x, h, (z - (rows-1)/2) cell.z); cells split v10-v01, as the physics heightfield"),
        ),
    ];
    let mut content = Vec::new();
    // A heightfield collider on an entity, through world_set (journaled, claim-checked).
    if let Some(c) = args.get("collider") {
        if let Err(m) = check_keys(c, "collider", &[&["entity", "max_side"]]) {
            return err("invalid_argument", m, vec![("field", Json::from("collider"))]);
        }
        let Some(path) = c.get("entity").and_then(Json::as_str) else {
            return err("invalid_argument", "'collider' is {entity: path, max_side?: 2..129}", vec![("field", Json::from("collider.entity"))]);
        };
        let side = match num(c.get("max_side"), "collider.max_side", COLLIDER_SIDE as f64, 2.0, COLLIDER_SIDE as f64) {
            Ok(v) => v as usize,
            Err(e) => return e,
        };
        // The heights where the terrain part's ops put them (a move, a scale), or a refusal.
        let (offset, scale) = match terrain_frame(&model, "collider") {
            Ok(f) => f,
            Err(e) => return e,
        };
        let (cols, rows, heights, cell) = t.physics_heightfield(side);
        let cell = [cell[0] * scale[0], cell[1] * scale[2]];
        let heights: Vec<f32> = heights.iter().map(|h| h * scale[1]).collect();
        let cmd = match collider_command(path, cols, rows, cell, &heights, offset) {
            Ok(c) => c,
            Err(m) => return err("invalid_argument", m, vec![]),
        };
        let out = tools.send(session, cmd);
        if out.is_error {
            return out;
        }
        fields.push((
            "collider",
            Json::obj([
                ("entity", Json::from(path)),
                ("columns", Json::from(cols)),
                ("rows", Json::from(rows)),
                ("cell", Json::arr(cell.map(|v| Json::Num(v as f64)))),
                ("offset", Json::arr(offset.map(|v| Json::Num(v as f64)))),
            ]),
        ));
    }
    content.insert(0, Content::Text(Json::obj(fields).to_string()));
    if args.get("preview").and_then(Json::as_bool).unwrap_or(false) {
        // Heights (grey) beside the splat weights (as colour).
        let (w, h) = (t.columns as u32, t.rows as u32);
        let mut rgba = vec![0u8; (2 * w * h * 4) as usize];
        for z in 0..t.rows {
            for x in 0..t.columns {
                let v = ((t.heights[z * t.columns + x] - lo) / (hi - lo).max(1e-6) * 255.0) as u8;
                let i = ((z as u32 * 2 * w + x as u32) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[v, v, v, 255]);
                let s = t.splat[z * t.columns + x];
                let j = ((z as u32 * 2 * w + w + x as u32) * 4) as usize;
                rgba[j..j + 4].copy_from_slice(&[s[0], s[1], s[2], 255]);
            }
        }
        content.push(image(2 * w, h, &rgba, 512));
    }
    ToolOutput { content, is_error: false }
}

// ---------------------------------------------------------------------------------------------
// procgen_scatter

fn num(v: Option<&Json>, field: &str, default: f64, lo: f64, hi: f64) -> Result<f64, ToolOutput> {
    match v {
        None | Some(Json::Null) => Ok(default),
        Some(x) => x.as_f64().filter(|x| *x >= lo && *x <= hi).ok_or_else(|| err("invalid_argument", format!("'{field}' must be a number in {lo}..{hi}"), vec![("field", Json::from(field))])),
    }
}

fn pair(v: Option<&Json>, field: &str) -> Result<Option<[f32; 2]>, ToolOutput> {
    match v {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Arr(a)) if a.len() == 2 && a.iter().all(|x| x.as_f64().is_some_and(f64::is_finite)) => Ok(Some([a[0].as_f64().unwrap_or(0.0) as f32, a[1].as_f64().unwrap_or(0.0) as f32])),
        _ => Err(err("invalid_argument", format!("'{field}' must be [a, b]"), vec![("field", Json::from(field))])),
    }
}

fn scatter_tool(tools: &Tools, session: &SessionInfo, ctx: &Context, args: &Json) -> ToolOutput {
    const KEYS: &[&str] = &[
        "area", "at", "min_distance", "count", "seed", "attempts", "slope", "height", "mask", "avoid", "items", "scale", "yaw", "tilt", "align", "sink", "ground", "spawn",
    ];
    if let Err(m) = check_keys(args, "procgen_scatter", &[KEYS]) {
        return err("invalid_argument", m, vec![]);
    }
    let run = || -> Result<ToolOutput, ToolOutput> {
        // Nested objects are checked too: a typo never falls back to a default.
        let nested = |v: Option<&Json>, what: &str, keys: &[&str]| -> Result<(), ToolOutput> {
            match v {
                Some(o) => check_keys(o, what, &[keys]).map_err(|m| err("invalid_argument", m, vec![("field", Json::from(what))])),
                None => Ok(()),
            }
        };
        nested(args.get("area"), "area", &["rect", "radius", "points"])?;
        nested(args.get("mask"), "mask", &["scale", "threshold", "octaves"])?;
        nested(args.get("ground"), "ground", &["recipe", "params", "seed", "at"])?;
        nested(args.get("spawn"), "spawn", &["parent", "name"])?;
        if let Some(a) = args.get("area") {
            let given = ["rect", "radius", "points"].iter().filter(|k| a.get(k).is_some()).count();
            if given != 1 {
                return Err(err("invalid_argument", "'area' takes exactly one of rect, radius or points", vec![("field", Json::from("area"))]));
            }
        }
        let area = match args.get("area") {
            Some(a) if a.get("rect").is_some() => scatter::Area::Rect(pair(a.get("rect"), "area.rect")?.unwrap_or([1.0, 1.0])),
            Some(a) if a.get("radius").is_some() => scatter::Area::Circle(num(a.get("radius"), "area.radius", 1.0, 0.01, 1e4)? as f32),
            Some(a) if a.get("points").is_some() => scatter::Area::Polygon(rows(a.get("points").expect("checked"), "area.points", 2, 1024)?.into_iter().map(|r| [r[0], r[1]]).collect()),
            _ => return Err(err("invalid_argument", "'area' is {rect: [width, depth]}, {radius: r} or {points: [[x, z], ...]} (metres, centred on 'at')", vec![("field", Json::from("area"))])),
        };
        let at = match args.get("at") {
            Some(v) => {
                let r = rows(&Json::arr([v.clone()]), "at", 3, 1)?;
                [r[0][0], r[0][1], r[0][2]]
            }
            None => [0.0; 3],
        };
        let items = args.get("items").and_then(Json::as_array).ok_or_else(|| err("invalid_argument", "'items' is [{model: {recipe, params?: {name: n | [min, max]}, seed?}, weight?, scale?}]", vec![("field", Json::from("items"))]))?;
        if items.is_empty() || items.len() > 16 {
            return Err(err("invalid_argument", "1..16 items", vec![("field", Json::from("items"))]));
        }
        let mut weights = Vec::new();
        for (i, it) in items.iter().enumerate() {
            nested(Some(it), &format!("items[{i}]"), &["model", "weight", "scale"])?;
            nested(it.get("model"), &format!("items[{i}].model"), &["recipe", "params", "seed"])?;
            if let Some(m) = it.get("model") {
                whole(m, "seed").map_err(|_| err("invalid_argument", format!("'items[{i}].model.seed' must be a whole number >= 0"), vec![("field", Json::from(format!("items[{i}].model.seed")))]))?;
            }
            num(it.get("scale"), &format!("items[{i}].scale"), 1.0, 1e-3, 1e3)?;
            let model = it.get("model");
            if model.and_then(|m| m.get("recipe")).and_then(Json::as_str).is_none() {
                return Err(err("invalid_argument", format!("items[{i}].model.recipe must name a recipe file"), vec![("field", Json::from(format!("items[{i}].model.recipe")))]));
            }
            weights.push(num(it.get("weight"), &format!("items[{i}].weight"), 1.0, 0.0, 1e4)? as f32);
        }
        let avoid = match args.get("avoid") {
            Some(v) => rows(v, "avoid", 3, scatter::MAX_AVOID)?.into_iter().map(|a| [a[0] - at[0], a[1] - at[2], a[2]]).collect(),
            None => Vec::new(),
        };
        let spec = ScatterSpec {
            area,
            offset: [0.0, 0.0],
            min_distance: num(args.get("min_distance"), "min_distance", 1.0, 0.01, 1e4)? as f32,
            max_count: num(args.get("count"), "count", 64.0, 0.0, scatter::MAX_POINTS as f64)? as u32,
            seed: whole(args, "seed")?.unwrap_or(0),
            attempts: num(args.get("attempts"), "attempts", 30.0, 1.0, scatter::MAX_ATTEMPTS as f64)? as u32,
            slope: pair(args.get("slope"), "slope")?,
            height: pair(args.get("height"), "height")?,
            mask: match args.get("mask") {
                Some(m) => Some(scatter::Mask { scale: num(m.get("scale"), "mask.scale", 5.0, 0.01, 1e4)? as f32, threshold: num(m.get("threshold"), "mask.threshold", 0.5, 0.0, 1.0)? as f32, octaves: num(m.get("octaves"), "mask.octaves", 3.0, 1.0, 8.0)? as u32 }),
                None => None,
            },
            splat: None,
            avoid,
            weights,
            scale: pair(args.get("scale"), "scale")?.unwrap_or([1.0, 1.0]),
            yaw: pair(args.get("yaw"), "yaw")?.unwrap_or([0.0, 360.0]),
            tilt: num(args.get("tilt"), "tilt", 0.0, 0.0, 90.0)? as f32,
            align: num(args.get("align"), "align", 0.0, 0.0, 1.0)? as f32,
            sink: num(args.get("sink"), "sink", 0.0, -100.0, 100.0)? as f32,
        };
        // Ground: a terrain recipe standing at ground.at, or flat at at.y.
        let ground = match args.get("ground") {
            Some(g) => {
                let m = build_recipe_file(ctx, g, "ground")?;
                let t = m.extras.terrain.clone().ok_or_else(|| err("no_terrain", format!("ground recipe '{}' has no terrain part", m.name), vec![]))?;
                let gat = match g.get("at") {
                    Some(v) => {
                        let r = rows(&Json::arr([v.clone()]), "ground.at", 3, 1)?;
                        [r[0][0], r[0][1], r[0][2]]
                    }
                    None => [0.0; 3],
                };
                let (off, sc) = terrain_frame(&m, "ground")?;
                Some((t, [gat[0] + off[0], gat[1] + off[1], gat[2] + off[2]], sc))
            }
            None => None,
        };
        // Placements are local to `at`; the ground is sampled in world space.
        let ground_fn = |x: f32, z: f32| -> Option<scatter::Ground> {
            let (wx, wz) = (x + at[0], z + at[2]);
            match &ground {
                None => Some(scatter::Ground { height: 0.0, normal: [0.0, 1.0, 0.0], splat: None }),
                Some((t, g, s)) => {
                    // Into the terrain's own frame (its part moved it by g, scaled it by s).
                    let (lx, lz) = ((wx - g[0]) / s[0], (wz - g[2]) / s[2]);
                    let n = t.normal_at(lx, lz)?;
                    let n = specula_model::math3::normalize([n[0] / s[0], n[1] / s[1], n[2] / s[2]]);
                    Some(scatter::Ground { height: t.height_at(lx, lz)? * s[1] + g[1] - at[1], normal: n, splat: t.splat_at(lx, lz) })
                }
            }
        };
        let (places, st) = scatter::scatter(&spec, &ground_fn, scatter::MAX_WORK).map_err(|e| err(if e.limit { "limit" } else { "invalid_argument" }, e.message, vec![("field", Json::from(e.field))]))?;
        let world = |p: &Placement| [p.position[0] + at[0], p.position[1] + at[1], p.position[2] + at[2]];
        let listed: Vec<Json> = places
            .iter()
            .take(MAX_LISTED)
            .map(|p| {
                let w = world(p);
                Json::obj([
                    ("item", Json::from(p.item)),
                    ("position", Json::arr(w.map(|v| Json::Num((v as f64 * 1000.0).round() / 1000.0)))),
                    ("rotation_deg", Json::arr(p.rotation_deg.map(|v| Json::Num((v as f64 * 10.0).round() / 10.0)))),
                    ("scale", Json::Num((p.scale as f64 * 1000.0).round() / 1000.0)),
                ])
            })
            .collect();
        let mut fields = vec![
            ("placed", Json::from(places.len())),
            ("sampled", Json::from(st.sampled)),
            ("rejected", Json::obj([("outside", st.rejected[0]), ("slope", st.rejected[1]), ("height", st.rejected[2]), ("mask", st.rejected[3]), ("avoid", st.rejected[5])].map(|(k, v)| (k, Json::from(v))))),
            ("placements", Json::arr(listed)),
        ];
        if places.len() > MAX_LISTED {
            fields.push(("placements_omitted", Json::from(places.len() - MAX_LISTED)));
        }
        // Spawn: one model entity per placement, in one world_batch: the engine commits it as the
        // calling agent (`Rule::World`: claims, reserved data, all or nothing, journaled, undoable).
        if let Some(sp) = args.get("spawn") {
            if places.len() > MAX_SPAWN {
                return Err(err("limit", format!("{} placements; one call spawns at most {MAX_SPAWN} (lower count)", places.len()), vec![("field", Json::from("count"))]));
            }
            if !places.is_empty() {
                let integers = integer_params(ctx, items)?;
                let cmd = spawn_command(items, &integers, &places, at, sp).map_err(|m| err("invalid_argument", m, vec![]))?;
                let out = tools.send(session, cmd);
                if out.is_error {
                    return Err(out);
                }
                fields.push(("spawned", Json::from(places.len())));
            }
        }
        Ok(ToolOutput::text(Json::obj(fields).to_string()))
    };
    match run() {
        Ok(o) | Err(o) => o,
    }
}

// ---------------------------------------------------------------------------------------------
// Definitions

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let schema = |props: Vec<(&str, Json)>| Json::obj([("type", Json::from("object")), ("properties", Json::obj(props))]);
    let def = |name: &str, description: &str, input_schema: Json| ToolDef { name: name.into(), description: description.into(), input_schema };
    vec![
        def(
            "procgen_texture",
            "Bake a procedural texture graph (a text recipe of nodes: noise value|perlin|fbm|ridged|billow|worley|worley_edge, \
             bricks, gradient, constant, blend, curve, levels, invert, warp, blur, normal, ao, curvature, gradient_map; outputs \
             base_color, height/normal, roughness, metallic, ao, emissive) to a tiling PBR texture set on CPU threads, and see it: \
             stats (size, GPU bytes, hash, means) plus an image strip base | normal | roughness | metallic (<= 512 px wide). \
             Deterministic; sizes are powers of two 16..1024 (256 suits the iGPU). Use the same graph in a recipe's \"textures\" \
             and a part's \"texture\" (box-projected UVs, uv_scale metres per repeat). save: true writes the full strip to the \
             captures folder. Errors carry the node path (nodes.<name>.<field>).",
            schema(vec![
                ("graph", t("object", "An inline texture graph {texgraph: 1, size?, seed?, nodes: {name: {op, ...}}, outputs: {base_color, ...}}")),
                ("file", t("string", "A texture graph file under the assets root (e.g. recipes/tex_crypt_stone.json)")),
                ("size", t("integer", "Override the size (power of two 16..1024)")),
                ("save", t("boolean", "Also write the strip PNG to the captures folder")),
                ("name", t("string", "File name for save")),
            ]),
        ),
        def(
            "procgen_terrain",
            "Build a terrain (heightfield noise layers fbm|ridged|billow, warp, island falloff, terraces, flat pads, hydraulic \
             droplet and thermal erosion, up to 4 splat layers by slope/height/noise) from a recipe with a terrain part or an \
             inline 'terrain' spec (the generator's fields), and report it: samples, cell, height range, splat coverage, walkable \
             share at max_slope (for navmesh building). collider: {entity} puts the terrain's heightfield collider on that \
             entity through world_set (journaled, claim-checked; resampled to at most 129 per side, the physics convention: \
             centred, cells split v10-v01, so place the entity where the terrain model stands; a move or scale on the terrain \
             part is carried into the collider, a rotation or reshaping op is refused). preview: true adds a heights | splat \
             image. Deterministic in the spec and seed; bounded by the model build budget (about 1.5 s).",
            schema(vec![
                ("recipe", t("string", "A recipe file under the assets root whose first terrain part is used")),
                ("params", t("object", "The recipe's parameter values")),
                ("seed", t("integer", "Seed (recipe or inline spec)")),
                ("terrain", t("object", "Inline terrain fields: size [x, z] m, samples [cols, rows] <= 257, base, layers [{kind, scale, octaves, amplitude}], warp, island, terrace [step, sharpness], pads [{at, radius, height, blend}], erosion {droplets, strength, steps}, thermal {iterations, talus}, splat [{name, color, slope, height, soft, noise, noise_scale}], depth, mesh_step")),
                ("collider", t("object", "{entity: path, max_side?: 2..129}")),
                ("max_slope", t("number", "Walkable slope for the report (degrees, default 45, as nav_build's agents)")),
                ("preview", t("boolean", "Add a heights | splat image")),
            ]),
        ),
        def(
            "procgen_scatter",
            "Poisson-disc placements (Bridson; min_distance apart) in an area (rect [w, d] | radius | points [[x, z]...], centred \
             on 'at'), kept by rules: ground slope and height ranges, a noise mask {scale, threshold, octaves}, circles to avoid \
             [[x, z, r] world], then each given an item by weight, a yaw range, scale range, tilt and lean to the ground \
             (align 0..1), sunk by sink. ground: {recipe, params?, seed?, at} samples a terrain recipe standing at 'at' (flat at \
             at.y otherwise). Deterministic in seed. Lists up to 32 placements; spawn: {parent?, name?} spawns one model entity \
             per placement (each item's model: {recipe, params?: {name: n | [min, max]} sampled per placement, seed?}) in one \
             world_batch: journaled, claim-checked, undoable; same recipes instance on the GPU. At most 1000 per call. Stops \
             at count; bounded by the model build budget (about 1.5 s; polygon areas are tested on a grid of their edges). \
             Every nested object's keys are checked.",
            schema(vec![
                ("area", t("object", "{rect: [w, d]} | {radius: r} | {points: [[x, z], ...]} (metres)")),
                ("at", t("array", "Centre [x, y, z] (world)")),
                ("min_distance", t("number", "Least spacing (metres)")),
                ("count", t("integer", "Most placements (<= 4096)")),
                ("seed", t("integer", "Seed")),
                ("attempts", t("integer", "Bridson candidates per point (1..64, default 30)")),
                ("slope", t("array", "Allowed slope [min, max] degrees")),
                ("height", t("array", "Allowed ground height [min, max] (relative to at.y)")),
                ("mask", t("object", "{scale m, threshold 0..1, octaves}")),
                ("avoid", t("array", "[[x, z, radius], ...] (world x, z), kept clear")),
                ("items", t("array", "[{model: {recipe, params?, seed?}, weight?, scale?}]")),
                ("scale", t("array", "Random scale [min, max]")),
                ("yaw", t("array", "Yaw range [min, max] degrees")),
                ("tilt", t("number", "Random lean up to this many degrees")),
                ("align", t("number", "0..1 lean towards the ground normal")),
                ("sink", t("number", "Metres into the ground")),
                ("ground", t("object", "{recipe, params?, seed?, at: [x, y, z]}: a terrain recipe")),
                ("spawn", t("object", "{parent?: path, name?: prefix}: spawn the placements")),
            ]),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        Context { assets: root, style: None, captures: std::env::temp_dir().join("specula-procgen-test") }
    }

    fn text(o: &ToolOutput) -> Json {
        match &o.content[0] {
            Content::Text(t) => Json::parse(t).unwrap(),
            _ => panic!("text first"),
        }
    }

    #[test]
    fn bakes_an_inline_texture_and_returns_an_image() {
        let args = Json::parse(
            r#"{"graph": {"size": 32, "nodes": {"n": {"op": "noise", "kind": "worley_edge", "scale": 4}, "c": {"op": "gradient_map", "in": "n", "stops": [[0, "strata_dark"], [1, "carved_highlight"]]}}, "outputs": {"base_color": "c", "height": "n"}}}"#,
        )
        .unwrap();
        let out = texture(&ctx(), &args);
        assert!(!out.is_error, "{:?}", out.content);
        assert_eq!(text(&out).get("size").and_then(Json::as_u64), Some(32));
        assert!(matches!(out.content[1], Content::Image { .. }));
        // A bad node is an error with its path.
        let bad = Json::parse(r#"{"graph": {"nodes": {"n": {"op": "noise", "octaves": 99}}, "outputs": {"base_color": "n"}}}"#).unwrap();
        let out = texture(&ctx(), &bad);
        assert!(out.is_error);
        match &out.content[0] {
            Content::Text(t) => assert!(t.contains("nodes.n.octaves"), "{t}"),
            _ => panic!(),
        }
        // Paths can't leave the assets root.
        assert!(texture(&ctx(), &Json::parse(r#"{"file": "../secret.json"}"#).unwrap()).is_error);
    }

    /// M4 audit: headstone_carved's `variant: [1, 3]` (declared integer) was sampled as a float, so
    /// the spawn batch failed (batch_failed). Integer params sample whole numbers, deterministically.
    #[test]
    fn scatter_samples_integer_params_as_whole_numbers() {
        assert_eq!([0.0, 0.33, 0.34, 0.99, 0.999_999].map(|t| sample_whole(1.0, 3.0, t)), [1.0, 1.0, 2.0, 3.0, 3.0]);
        assert_eq!((sample_whole(3.0, 1.0, 0.5), sample_whole(0.5, 2.5, 0.99), sample_whole(1.2, 1.8, 0.5)), (2.0, 2.0, 2.0));
        let items = vec![Json::parse(r#"{"model": {"recipe": "recipes/headstone_carved.json", "params": {"variant": [1, 3], "tilt": [-1, 1]}}}"#).unwrap()];
        let ints = integer_params(&ctx(), &items).unwrap();
        assert!(ints[0].contains("variant") && !ints[0].contains("tilt"), "{ints:?}");
        let places: Vec<Placement> = (0..60).map(|v| Placement { position: [v as f32, 0.0, 0.0], rotation_deg: [0.0; 3], scale: 1.0, item: 0, variant: v }).collect();
        let params = |ints: &[BTreeSet<String>]| -> Vec<Vec<(String, f64)>> {
            let Ok(crate::tools::Command::Batch(ops)) = spawn_command(&items, ints, &places, [0.0; 3], &Json::obj::<&str>([])) else { panic!("one batch") };
            ops.into_iter().map(|op| match op {
                crate::world::Op::Spawn(s) => s.model.expect("a model spawn").params,
                _ => panic!("spawns only"),
            }).collect()
        };
        let (a, b) = (params(&ints), params(&ints));
        assert_eq!(a, b, "deterministic");
        let variants: BTreeSet<i64> = a.iter().map(|p| p.iter().find(|(k, _)| k == "variant").unwrap().1).inspect(|v| assert_eq!(v.fract(), 0.0)).map(|v| v as i64).collect();
        assert_eq!(variants, BTreeSet::from([1, 2, 3]), "every variant in the range turns up");
        assert!(a.iter().any(|p| p.iter().any(|(k, v)| k == "tilt" && v.fract() != 0.0)), "float params still sample the range");
        // The float params' samples don't move when integers are known (one draw per range).
        let floats = params(&[]);
        let tilt = |p: &[Vec<(String, f64)>]| p.iter().map(|x| x.iter().find(|(k, _)| k == "tilt").unwrap().1).collect::<Vec<f64>>();
        assert_eq!(tilt(&a), tilt(&floats));
    }

    #[test]
    fn scatter_spec_errors_are_structured() {
        let tools = Tools::new(std::sync::mpsc::channel().0, Vec::new());
        let s = SessionInfo { id: "t".into(), client_name: "c".into(), client_version: "1".into(), protocol_version: "x".into() };
        let out = scatter_tool(&tools, &s, &ctx(), &Json::parse(r#"{"area": {"rect": [10, 10]}, "items": []}"#).unwrap());
        assert!(out.is_error);
        // No spawn: placements only, deterministic.
        let args = Json::parse(r#"{"area": {"rect": [10, 6]}, "min_distance": 1.5, "seed": 3, "items": [{"model": {"recipe": "recipes/tombstone.json"}}]}"#).unwrap();
        let a = scatter_tool(&tools, &s, &ctx(), &args);
        assert!(!a.is_error, "{:?}", a.content);
        let b = scatter_tool(&tools, &s, &ctx(), &args);
        assert_eq!(a.content, b.content);
        assert!(text(&a).get("placed").and_then(Json::as_u64).unwrap() > 5);
    }

    fn field_of(o: &ToolOutput) -> String {
        assert!(o.is_error, "{:?}", o.content);
        text(o).get("field").and_then(Json::as_str).unwrap_or("").to_string()
    }

    #[test]
    fn nested_typos_and_fractional_seeds_are_refused() {
        let tools = Tools::new(std::sync::mpsc::channel().0, Vec::new());
        let s = SessionInfo { id: "t".into(), client_name: "c".into(), client_version: "1".into(), protocol_version: "x".into() };
        let base = r#""min_distance": 1.5, "items": [{"model": {"recipe": "recipes/tombstone.json"}}]"#;
        for (bad, field) in [
            (r#"{"area": {"rect": [10, 6], "radius": 3}, MIN}"#, "area"),
            (r#"{"area": {"rekt": [10, 6]}, MIN}"#, "area"),
            (r#"{"area": {"rect": [10, 6]}, "mask": {"treshold": 0.5}, MIN}"#, "mask"),
            (r#"{"area": {"rect": [10, 6]}, "spawn": {"nme": "x"}, MIN}"#, "spawn"),
            (r#"{"area": {"rect": [10, 6]}, "ground": {"recipe": "recipes/graveyard_ground.json", "att": [0, 0, 0]}, MIN}"#, "ground"),
            (r#"{"area": {"rect": [10, 6]}, "min_distance": 1.5, "items": [{"model": {"recipe": "recipes/tombstone.json"}, "weigth": 2}]}"#, "items[0]"),
            (r#"{"area": {"rect": [10, 6]}, "min_distance": 1.5, "items": [{"model": {"recpie": "recipes/tombstone.json"}}]}"#, "items[0].model"),
            (r#"{"area": {"rect": [10, 6]}, "seed": 2.5, MIN}"#, "seed"),
            (r#"{"area": {"rect": [10, 6]}, "min_distance": 1.5, "items": [{"model": {"recipe": "recipes/tombstone.json", "seed": 1.5}}]}"#, "items[0].model.seed"),
        ] {
            let args = Json::parse(&bad.replace("MIN", base)).unwrap();
            assert_eq!(field_of(&scatter_tool(&tools, &s, &ctx(), &args)), field, "{bad}");
        }
        let t = Json::parse(r#"{"terrain": {"size": [10, 10], "samples": [17, 17], "layers": [{"scale": 4, "amplitude": 1}]}, "seed": 0.5}"#).unwrap();
        assert_eq!(field_of(&terrain(&tools, &s, &ctx(), &t)), "seed");
        let t = Json::parse(r#"{"terrain": {"size": [10, 10], "samples": [17, 17], "layers": [{"scale": 4, "amplitude": 1}]}, "collider": {"entity": "/g", "max_sid": 9}}"#).unwrap();
        assert_eq!(field_of(&terrain(&tools, &s, &ctx(), &t)), "collider");
        // The report's walkable slope defaults to nav_build's 45 degrees.
        let t = Json::parse(r#"{"terrain": {"size": [10, 10], "samples": [17, 17], "layers": [{"scale": 4, "amplitude": 1}]}}"#).unwrap();
        assert_eq!(text(&terrain(&tools, &s, &ctx(), &t)).get("max_slope_deg").and_then(Json::as_f64), Some(45.0));
    }

    #[test]
    fn terrain_part_moves_follow_and_rotations_are_refused() {
        // A scratch assets folder: the Sepulcra style, and one terrain recipe per op.
        let dir = std::env::temp_dir().join(format!("specula-procgen-frame-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("recipes")).unwrap();
        let sepulcra = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets/style.json");
        std::fs::copy(sepulcra, dir.join("style.json")).unwrap();
        for (name, ops) in [("flat", "[]"), ("up", r#"[{"op": "move", "by": [0, 3, 0]}]"#), ("turned", r#"[{"op": "rotate", "deg": [0, 30, 0]}]"#), ("lumpy", r#"[{"op": "displace", "amount": 0.2, "scale": 1}]"#)] {
            let r = format!(r#"{{ "name": "{name}", "lods": [], "parts": [ {{ "name": "g", "material": "slab_stone", "generate": {{ "op": "terrain", "size": [10, 10], "samples": [17, 17], "layers": [{{ "scale": 4, "amplitude": 0.5 }}] }}, "ops": {ops} }} ] }}"#);
            std::fs::write(dir.join("recipes").join(format!("{name}.json")), r).unwrap();
        }
        let c = Context { assets: dir.clone(), style: None, captures: dir.join("captures") };
        let tools = Tools::new(std::sync::mpsc::channel().0, Vec::new());
        let s = SessionInfo { id: "t".into(), client_name: "c".into(), client_version: "1".into(), protocol_version: "x".into() };
        let scat = |g: &str| {
            let a = Json::parse(&format!(r#"{{"area": {{"rect": [6, 6]}}, "min_distance": 1.5, "seed": 3, "ground": {{"recipe": "recipes/{g}.json"}}, "items": [{{"model": {{"recipe": "recipes/flat.json"}}}}]}}"#)).unwrap();
            scatter_tool(&tools, &s, &c, &a)
        };
        let ys = |o: &ToolOutput| -> Vec<f64> {
            assert!(!o.is_error, "{:?}", o.content);
            text(o).get("placements").and_then(Json::as_array).unwrap().iter().map(|p| p.get("position").and_then(Json::as_array).unwrap()[1].as_f64().unwrap()).collect()
        };
        let (flat, up) = (ys(&scat("flat")), ys(&scat("up")));
        assert_eq!(flat.len(), up.len());
        assert!(flat.iter().zip(&up).all(|(a, b)| (b - a - 3.0).abs() < 1e-3), "{flat:?} {up:?}");
        for (g, code) in [("turned", "terrain_rotated"), ("lumpy", "terrain_reshaped")] {
            let o = scat(g);
            assert!(o.is_error);
            assert_eq!(text(&o).get("error").and_then(Json::as_str), Some(code), "{g}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
