//! `model_export`: a model recipe written as a glTF binary (`.glb`) for other engines and
//! tools (docs/design/EXPORT.md).
//!
//! The owner, 2026-11-13: assets are saved as data that is reused, copied and modified, and
//! are "easily exportable and transportable to any other game engines". A recipe is the asset
//! (text, in the project, built the same on every machine); a `.glb` is what another engine
//! reads. This tool builds the recipe through the engine's model library (the same build an
//! entity draws), writes it with `specula-gltf`'s exporter on the calling agent's thread, and
//! stores the file as a binary project file, as `project_upload` would: checked (it imports
//! the way `model_build` imports a `.glb`), claim-checked, versioned, journaled, undoable.
//!
//! What carries: the mesh of one LOD (positions, normals, vertex colours, UVs), one primitive
//! per material with its PBR factors, baked texture sets as PNG images, the rig as a skin, and
//! every clip as an animation.
//! What does not is said in the answer (`not_carried`), never left for the reader to find out.

use crate::blobs::{MAX_BLOB_BYTES, MAX_CHUNK_BYTES};
use crate::models::{Built, BuiltSlot};
use crate::tools::{bool_arg, check_keys, int_arg, params_arg, string_arg, Command, Tools};
use crate::world::{self, ModelSpec};
use specula_core::Json;
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};
use std::sync::Arc;

/// Longest `recipe` and `path` arguments.
pub const MAX_PATH_CHARS: usize = 256;
/// LODs a recipe can have at most (the argument's bound; the answer says how many this one has).
pub const MAX_LOD: u64 = 15;
/// The largest `expect_version` (the largest integer JSON carries exactly; `paint_tools::guards`).
pub const MAX_VERSION: u64 = (1 << 53) - 1;
/// Names listed in the answer (materials, clips) before the rest is counted.
const MAX_LISTED: usize = 32;
/// The folder exports go to when no path is given.
pub const EXPORT_FOLDER: &str = "assets/models/exports";

fn err(code: &'static str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_string())
}

/// What `model_export` was asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub spec: ModelSpec,
    pub lod: usize,
    pub animations: bool,
    /// The project path of the `.glb`.
    pub path: String,
    pub expect_version: Option<u64>,
    pub dry_run: bool,
}

/// A file name from a recipe's path: its stem, with what a file name can't hold replaced.
pub fn stem_of(recipe: &str) -> String {
    let name = recipe.rsplit(['/', '\\']).next().unwrap_or(recipe);
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    let clean: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') { c } else { '_' }).take(64).collect();
    if clean.is_empty() {
        "model".to_owned()
    } else {
        clean
    }
}

/// Reads the arguments (every one bounded: AGENTS rule 15).
pub fn parse(args: &Json) -> Result<Request, String> {
    check_keys(args, "model_export", &[&["recipe", "params", "seed", "lod", "animations", "path", "expect_version", "dry_run"]])?;
    let recipe = string_arg(args, "recipe")?.ok_or("'recipe' is required: a recipe file under the assets root, e.g. \"recipes/statue.json\"")?;
    if recipe.chars().count() > MAX_PATH_CHARS {
        return Err(format!("'recipe' is at most {MAX_PATH_CHARS} characters"));
    }
    let lower = recipe.to_ascii_lowercase();
    if lower.ends_with(".glb") || lower.ends_with(".gltf") {
        return Err(format!("{recipe} is a glTF file already: copy it as it is (project_read gives its bytes). model_export writes recipes as .glb"));
    }
    let seed = int_arg(args, "seed", 0, world::MAX_MODEL_SEED)?;
    let spec = ModelSpec { recipe: recipe.to_owned(), params: params_arg(args)?.unwrap_or_default(), seed };
    spec.check().map_err(|e| e.message)?;
    let lod = int_arg(args, "lod", 0, MAX_LOD)?.unwrap_or(0) as usize;
    let animations = bool_arg(args, "animations")?.unwrap_or(true);
    let path = match string_arg(args, "path")? {
        Some(p) if p.chars().count() > MAX_PATH_CHARS => return Err(format!("'path' is at most {MAX_PATH_CHARS} characters")),
        Some(p) => p.to_owned(),
        None => format!("{EXPORT_FOLDER}/{}.glb", stem_of(recipe)),
    };
    if !path.to_ascii_lowercase().ends_with(".glb") {
        return Err(format!("'path' is where the .glb goes, e.g. \"{EXPORT_FOLDER}/statue.glb\" (got '{path}')"));
    }
    crate::project::check_path(&path).map_err(|e| e.message)?;
    let (expect_version, dry_run) = crate::paint_tools::guards(args)?;
    Ok(Request { spec, lod, animations, path, expect_version, dry_run })
}

/// The exported file and what it holds.
pub struct Exported {
    pub glb: Vec<u8>,
    pub report: Json,
}

/// A list of names for the answer: the first [`MAX_LISTED`], and how many more.
fn listed<'a>(names: impl Iterator<Item = &'a str>) -> Json {
    let all: Vec<&str> = names.collect();
    let mut out: Vec<Json> = all.iter().take(MAX_LISTED).map(|n| Json::from(*n)).collect();
    if all.len() > MAX_LISTED {
        out.push(Json::from(format!("... and {} more", all.len() - MAX_LISTED)));
    }
    Json::arr(out)
}

/// Writes `built` as a `.glb` (pure: no disk, no engine). `Err`: a refusal's code and words.
pub fn export(built: &Built, lod: usize, animations: bool) -> Result<Exported, (&'static str, String)> {
    let model = &built.model;
    if lod >= model.lods.len() {
        return Err(("no_such_lod", format!("{} has {} LOD(s): lod is 0..{}", model.name, model.lods.len(), model.lods.len().saturating_sub(1))));
    }
    let mesh = &model.lods[lod];
    let set = built.anims.as_deref();
    let clips: Vec<specula_anim::Clip> = match set {
        Some(set) if animations => set.clips().map(|(_, c)| c.clone()).collect(),
        _ => Vec::new(),
    };
    // A clip with no tracks (a neutral base for additive layers) moves nothing, and a glTF
    // animation needs at least one channel: the exporter writes none for it, and the answer
    // says so instead of counting it.
    let (written, still): (Vec<&specula_anim::Clip>, Vec<&specula_anim::Clip>) = clips.iter().partition(|c| !c.tracks.is_empty());
    let input = specula_gltf::ExportInput { model, lod, skeleton: set.map(|s| &s.skeleton), clips: &clips };
    let glb = specula_gltf::export_glb(&input).map_err(|e| ("export_failed", format!("{} could not be written as glTF: {e}", model.name)))?;
    if glb.len() as u64 > MAX_BLOB_BYTES {
        return Err((
            "too_large",
            format!("{} at LOD {lod} is {} bytes as .glb; a binary project file holds at most {MAX_BLOB_BYTES}: export a higher lod (fewer triangles) or without animations", model.name, glb.len()),
        ));
    }
    // What a reader of the file should not have to find out for themselves.
    let mut not_carried: Vec<Json> = Vec::new();
    if model.lods.len() > 1 {
        not_carried.push(Json::from(format!("the other {} LOD(s): one file holds one (lod {lod}); export each you need", model.lods.len() - 1)));
    }
    // The baked texture sets the exported LOD's parts use, each once: their images are in the file.
    let mut sets: Vec<&specula_model::texgraph::TextureSet> = Vec::new();
    for part in &mesh.parts {
        if let Some((_, set)) = model.extras.textures.iter().find(|(key, _)| key == &part.material) {
            if !sets.iter().any(|s| s.hash == set.hash) {
                sets.push(set);
            }
        }
    }
    // (As the exporter writes them: an emissive image only when a preset that uses the set emits.)
    let emits = |set: &specula_model::texgraph::TextureSet| {
        mesh.parts.iter().any(|p| model.extras.textures.iter().any(|(key, s)| key == &p.material && s.hash == set.hash) && model.materials.iter().any(|(name, preset)| name == &p.material && preset.emissive > 0.0))
    };
    let images: usize = sets.iter().map(|s| 2 + usize::from(s.normal.is_some()) + usize::from(s.emissive.is_some() && emits(s))).sum();
    // Sets no part of this LOD is drawn with: a terrain's layer textures (the terrain renderer
    // blends them over the ground; the mesh carries each layer's colour, not its texture).
    let unused: Vec<&str> = model.extras.textures.iter().filter(|(key, _)| !mesh.parts.iter().any(|p| &p.material == key)).map(|(key, _)| key.as_str()).collect();
    if !unused.is_empty() {
        let names: Vec<&str> = unused.iter().take(8).copied().collect();
        not_carried.push(Json::from(format!(
            "{} texture set(s) no part of this mesh is drawn with ({}): a terrain's layer textures are blended by this engine's terrain renderer; the mesh carries each layer's colour, not its texture",
            unused.len(),
            names.join(", ")
        )));
    }
    if model.wind.is_some() {
        not_carried.push(Json::from("the wind (foliage sway and its edge fade): it is this engine's shader, so foliage looks plainer in another engine; the term and any painted weights are kept in the file (extras.specula.wind, the _WIND attribute) for the way back"));
    }
    if model.extras.terrain.is_some() {
        not_carried.push(Json::from("the terrain's heights and splat layers as data: the mesh carries the shape and colour; the recipe is the source"));
    }
    if model.rig.is_some() && set.is_none() {
        not_carried.push(Json::from(format!("clips: the model has a rig and no animation set{}", built.anim_note.as_ref().map_or(String::new(), |n| format!(" ({n})")))));
    }
    if !still.is_empty() {
        let names: Vec<&str> = still.iter().take(8).map(|c| c.name.as_str()).collect();
        not_carried.push(Json::from(format!("{} clip(s) with no tracks ({}): they move nothing, and a glTF animation needs at least one channel", still.len(), names.join(", "))));
    }
    if set.is_some() && !animations {
        not_carried.push(Json::from("clips: left out as asked (animations: false); the rig is written"));
    }
    not_carried.push(Json::from("the collider: other engines make their own from the mesh"));
    let report = Json::obj([
        ("model", Json::from(model.name.as_str())),
        ("recipe_hash", Json::from(format!("{:016x}", model.hash))),
        ("lod", Json::from(lod)),
        ("lods", Json::from(model.lods.len())),
        ("triangles", Json::from(mesh.triangle_count())),
        ("vertices", Json::from(mesh.positions.len())),
        ("materials", listed(model.materials.iter().map(|(n, _)| n.as_str()))),
        ("bones", Json::from(set.map_or(model.rig.as_ref().map_or(0, |r| r.len()), |s| s.skeleton.len()))),
        ("clips", listed(written.iter().map(|c| c.name.as_str()))),
        ("texture_sets", listed(sets.iter().map(|s| s.name.as_str()))),
        ("images", Json::from(images)),
        ("bytes", Json::from(glb.len())),
        (
            "carried",
            Json::arr(
                [
                    "positions, normals, vertex colours (COLOR_0, linear) and UVs when the mesh has them",
                    "one primitive per material, with its metallic, roughness, emissive and glass opacity as glTF PBR factors",
                    "baked texture sets as PNG images in the file: base colour (a cut-out's coverage in its alpha, alphaMode MASK), normal, metallic-roughness, emissive; a textured part's vertices are white",
                    "the rig as one skin (a node per bone) and each clip as an animation",
                    "Specula's own data (material presets, bone tails, clip loop modes, events, curves) in extras.specula",
                ]
                .map(Json::from),
            ),
        ),
        ("not_carried", Json::arr(not_carried)),
        ("units", Json::from("metres, +Y up, right-handed, counter-clockwise triangles: glTF's own conventions")),
    ]);
    Ok(Exported { glb, report })
}

/// `model_export` (on the calling agent's protocol thread).
pub fn call(tools: &Tools, session: &SessionInfo, args: &Json) -> ToolOutput {
    match run(tools, session, args) {
        Ok(out) => out,
        Err(m) => ToolOutput::error(crate::tools::argument_error(&m).to_string()),
    }
}

fn run(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    let req = parse(args)?;
    // Where it goes must be a binary project file (a .glb under a models folder): said before
    // anything is built.
    let snap = match crate::project_tools::snapshot(tools, session, &req.path, None, false) {
        Ok(s) => s,
        Err(out) => return Ok(out),
    };
    if snap.kind != Some(crate::project::FileKind::Glb) {
        return Ok(err("not_a_model_path", format!("{} isn't a model file of the project: a .glb goes under a folder named {} in the assets mount, e.g. \"{EXPORT_FOLDER}/statue.glb\"", req.path, crate::project::MODEL_FOLDERS.join(" / "))));
    }
    // The build is the engine's own (the model library: the same build an entity draws); the
    // built model is lent to this thread, where the export and the file's check run.
    let slot = BuiltSlot::default();
    let built_report = tools.send(session, Command::ModelExport(req.spec.clone(), Arc::clone(&slot)));
    if built_report.is_error {
        return Ok(built_report);
    }
    let Some(built) = slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() else { return Ok(err("no_answer", "the engine built the model and did not hand it over")) };
    let exported = match export(&built, req.lod, req.animations) {
        Ok(e) => e,
        Err((code, message)) => return Ok(err(code, message)),
    };
    let mut report = exported.report;
    report.set("recipe", Json::from(req.spec.recipe.as_str()));
    report.set("path", Json::from(req.path.as_str()));
    if req.dry_run {
        report.set("dry_run", Json::Bool(true));
        report.set("note", Json::from("nothing was written: call again without dry_run to write the file"));
        return Ok(ToolOutput::text(report.to_string()));
    }
    // Stored as project_upload stores a file: staged, checked by kind (it must import as
    // model_build imports a .glb), then committed (claims, versions, the journal, undo).
    let Some(store) = tools.content.as_ref().map(Arc::clone) else { return Ok(err("no_content_store", "this engine has no content store (binary project files need one)")) };
    let Some(by) = crate::project_binary::owner(tools, session) else { return Ok(err("no_answer", "the engine didn't say who you are")) };
    let ups = &tools.uploads;
    let disk = |e: &crate::disk::DiskError| ToolOutput::error(e.to_json().to_string());
    let crc = specula_core::checksum::crc32(&exported.glb);
    let mut upload = match ups.begin(&store, &by, &req.path, exported.glb.len() as u64, crc, req.expect_version) {
        Ok(u) => u,
        Err(e) => return Ok(disk(&e)),
    };
    for (n, chunk) in exported.glb.chunks(MAX_CHUNK_BYTES).enumerate() {
        match ups.chunk(&by, &upload.id, (n * MAX_CHUNK_BYTES) as u64, chunk, None) {
            Ok(u) => upload = u,
            Err(e) => {
                ups.finish(&store, &by, &upload.id);
                return Ok(disk(&e));
            }
        }
    }
    let committed = crate::project_binary::commit_upload(tools, session, &store, &by, &upload);
    // (A refused commit leaves an upload open for project_upload's caller to try again; this
    // tool's caller has no use for one: the export is made again in a call. Whatever the
    // commit said, no upload of this call's stays open: an agent has four.)
    ups.finish(&store, &by, &upload.id);
    let committed = committed?;
    let said = committed.content.iter().find_map(|c| match c {
        specula_protocol::Content::Text(t) => Some(t.as_str()),
        _ => None,
    });
    let commit = said.and_then(|t| Json::parse(t).ok()).unwrap_or_else(|| Json::obj([("error", Json::from("no_answer")), ("message", Json::from(said.unwrap_or_default()))]));
    Ok(finish(report, commit, committed.is_error, &req.path))
}

/// The tool's answer, from the export's report and what the project store said of the commit.
///
/// A refusal is said in this tool's own words: the store words its refusals for
/// `project_upload`'s caller ("the upload stays open: commit again... or abort it"), and this
/// tool has no upload to continue. An unchanged file is said as that: the answer never points
/// at `world_undo` for a call that wrote nothing (it would undo the agent's change before
/// this one; review R1-2).
pub fn finish(mut report: Json, mut commit: Json, refused: bool, path: &str) -> ToolOutput {
    if refused {
        if let Json::Obj(fields) = &mut commit {
            fields.retain(|(k, _)| k != "upload" && k != "note");
        }
        if let Some(message) = commit.get("message").and_then(Json::as_str).map(str::to_owned) {
            let about_upload = [" (the upload stays open", "; the upload stays open", " (abort the upload"].iter().filter_map(|t| message.find(t)).min();
            if let Some(at) = about_upload {
                commit.set("message", Json::from(message[..at].trim_end()));
            }
        }
        // (An unanswered commit may still land: never "nothing was written" of one. A stale
        // version is put right by naming the version that is there now.)
        let note = match commit.get("error").and_then(Json::as_str) {
            Some("no_answer") => format!("the engine did not confirm the write: project_read {{path: \"{path}\"}} shows whether the file is there"),
            Some("stale_version") => "nothing was written: export again with expect_version set to the file's version now (\"version\" above), or without expect_version to replace what is there".to_owned(),
            _ => "nothing was written: export again once the problem is gone".to_owned(),
        };
        commit.set("note", Json::from(note));
        return ToolOutput::error(commit.to_string());
    }
    // The file's own facts (its version, its content id, what its check found), then what was
    // exported.
    for key in ["version", "unchanged", "content", "journal", "info"] {
        if let Some(v) = commit.get(key) {
            report.set(key, v.clone());
        }
    }
    let back = format!("model_build {{recipe: \"{}\"}} imports it back to check it", path.strip_prefix("assets/").unwrap_or(path));
    let unchanged = commit.get("unchanged").and_then(Json::as_bool) == Some(true);
    report.set(
        "next",
        Json::from(if unchanged {
            format!("{path} already held exactly this export: nothing was written, and this call left nothing to undo; {back}")
        } else {
            format!("the file is {path} in the project; world_undo takes this write back; {back}")
        }),
    );
    ToolOutput::text(report.to_string())
}

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let text = |d: &str| Json::obj([("type", Json::from("string")), ("maxLength", Json::from(MAX_PATH_CHARS)), ("description", Json::from(d))]);
    vec![ToolDef {
        name: "model_export".into(),
        description: format!(
            "Write a model recipe as a glTF binary (.glb) that other engines and tools read (Blender, Godot, Unity, Unreal). \
             Builds the recipe with params and seed as model_build does, and writes one LOD: positions, normals, vertex \
             colours and UVs, one primitive per material with its PBR factors, baked texture sets as PNG images, the rig as \
             a skin and each clip as an animation (animations: false leaves the clips out). The file is stored as a binary project file ({EXPORT_FOLDER}/<recipe's \
             name>.glb unless path says where): checked (it imports back as model_build imports a .glb), claim-checked \
             (/project/<path>), versioned (expect_version), journaled (world_undo takes it back), at most {} MiB. The answer says \
             what the file holds (triangles, vertices, materials, bones, clips, images, bytes) and what was not carried \
             (other LODs, the wind, the collider). dry_run reports without writing. The recipe stays the source: export again \
             after changing it.",
            MAX_BLOB_BYTES >> 20
        ),
        input_schema: Json::obj([
            ("type", Json::from("object")),
            (
                "properties",
                Json::obj([
                    ("recipe", text("The model's recipe under the assets folder (\"recipes/statue.json\")")),
                    ("params", Json::obj([("type", Json::from("object")), ("maxProperties", Json::from(world::MAX_MODEL_PARAMS)), ("description", Json::from("Recipe parameters, e.g. {\"tier\": 3}"))])),
                    ("seed", Json::obj([("type", Json::from("integer")), ("minimum", Json::from(0)), ("maximum", Json::from(world::MAX_MODEL_SEED)), ("description", Json::from("Variant seed (default: the recipe's)"))])),
                    ("lod", Json::obj([("type", Json::from("integer")), ("minimum", Json::from(0)), ("maximum", Json::from(MAX_LOD)), ("description", Json::from("Which LOD to write (0, full detail, by default)"))])),
                    ("animations", t("boolean", "Write the clips (true by default; the rig is written either way)")),
                    ("path", text("Where the .glb goes in the project (default \"assets/models/exports/<recipe's name>.glb\")")),
                    ("expect_version", Json::obj([("type", Json::from("integer")), ("minimum", Json::from(0)), ("maximum", Json::from(MAX_VERSION)), ("description", Json::from("Refuse unless the .glb is at this project version (0: it must not exist yet)"))])),
                    ("dry_run", t("boolean", "Build and export, report what the file would hold, write nothing")),
                ]),
            ),
            ("required", Json::arr([Json::from("recipe")])),
        ]),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Json {
        Json::parse(text).unwrap()
    }

    #[test]
    fn arguments_are_read_bounded_and_defaulted() {
        let r = parse(&args(r#"{"recipe": "recipes/warding_stone.json"}"#)).unwrap();
        assert_eq!((r.path.as_str(), r.lod, r.animations, r.expect_version, r.dry_run), ("assets/models/exports/warding_stone.glb", 0, true, None, false));
        let r = parse(&args(r#"{"recipe": "recipes/shambler.json", "params": {"tier": 2}, "seed": 7, "lod": 1, "animations": false, "path": "assets/models/out/s.glb", "expect_version": 0, "dry_run": true}"#)).unwrap();
        assert_eq!((r.spec.seed, r.spec.params.len(), r.lod, r.animations, r.path.as_str(), r.expect_version, r.dry_run), (Some(7), 1, 1, false, "assets/models/out/s.glb", Some(0), true));
        // Refusals say what to do.
        for (bad, says) in [
            (r#"{}"#, "'recipe' is required"),
            (r#"{"recipe": "models/wraith.glb"}"#, "is a glTF file already"),
            (r#"{"recipe": "models/wraith.GLTF"}"#, "is a glTF file already"),
            (r#"{"recipe": "recipes/a.json", "lod": 16}"#, "lod"),
            (r#"{"recipe": "recipes/a.json", "lod": -1}"#, "lod"),
            (r#"{"recipe": "recipes/a.json", "seed": 99999999999}"#, "seed"),
            (r#"{"recipe": "recipes/a.json", "path": "assets/models/a.txt"}"#, "where the .glb goes"),
            (r#"{"recipe": "recipes/a.json", "path": "../a.glb"}"#, ""),
            (r#"{"recipe": "recipes/a.json", "dry_run": "yes"}"#, "dry_run"),
            (r#"{"recipe": "recipes/a.json", "expect_version": "1"}"#, "expect_version"),
            (r#"{"recipe": "recipes/a.json", "animations": 1}"#, "animations"),
            (r#"{"recipe": "recipes/a.json", "format": "gltf"}"#, "format"),
            (r#"{"recipe": "/abs.json"}"#, ""),
        ] {
            let e = parse(&args(bad)).unwrap_err();
            assert!(e.contains(says), "{bad}: {e}");
        }
        let long = format!(r#"{{"recipe": "recipes/{}.json"}}"#, "x".repeat(MAX_PATH_CHARS));
        assert!(parse(&args(&long)).unwrap_err().contains("at most"));
        let long = format!(r#"{{"recipe": "recipes/a.json", "path": "assets/models/{}.glb"}}"#, "x".repeat(MAX_PATH_CHARS));
        assert!(parse(&args(&long)).unwrap_err().contains("at most"));
    }

    /// A recipe of Sepulcra's, built as the model library builds it (its style, its includes,
    /// its animation set when it has one).
    fn built(name: &str) -> Built {
        use specula_model::{build, BuildParams, Style};
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        let style = Style::parse(&std::fs::read_to_string(root.join("style.json")).unwrap()).unwrap();
        let mut library = std::collections::BTreeMap::new();
        for entry in std::fs::read_dir(root.join("recipes")).unwrap() {
            let path = entry.unwrap().path();
            library.insert(path.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&path).unwrap());
        }
        let model = build(&library[name], &BuildParams { library: Some(&library), ..BuildParams::new(&style) }).unwrap();
        let anims = match (&model.rig, std::fs::read_to_string(root.join(format!("anims/{name}.json")))) {
            (Some(rig), Ok(text)) => Some(Arc::new(specula_anim::AnimSet::load(&text, specula_anim::Skeleton::from_rig(rig).unwrap()).unwrap())),
            _ => None,
        };
        Built { id: 1, model, content: 0, build_ms: 0.0, style: root.join("style.json"), anims, anim_note: None, textures: Vec::new(), materials: Default::default(), warnings: Vec::new() }
    }

    fn text(j: &Json, key: &str) -> String {
        j.get(key).map(Json::to_string).unwrap_or_default()
    }

    /// The answer says what the file holds: counted here by importing the file back, the way
    /// another program reads it.
    #[test]
    fn the_answer_says_what_the_file_holds_and_what_it_does_not() {
        let shambler = built("shambler");
        let set = shambler.anims.clone().expect("the shambler has clips");
        let all = set.clips().count();
        let moving = set.clips().filter(|(_, c)| !c.tracks.is_empty()).count();
        assert!(moving < all, "one of its clips has no tracks (the case the live check found: 8 said, 7 written)");
        let e = export(&shambler, 0, true).unwrap_or_else(|(c, m)| panic!("{c}: {m}"));
        let back = specula_gltf::import_bytes(&e.glb, None, &specula_gltf::ImportOptions::default()).expect("the file imports back");
        let said = |key: &str| e.report.get(key).and_then(Json::as_u64);
        assert_eq!(said("triangles"), Some(back.model.lods[0].triangle_count() as u64));
        assert_eq!(said("triangles"), Some(shambler.model.lods[0].triangle_count() as u64));
        assert_eq!(said("bones"), Some(back.skeleton.as_ref().map_or(0, |s| s.len()) as u64));
        assert_eq!(said("bytes"), Some(e.glb.len() as u64));
        let clips = e.report.get("clips").and_then(Json::as_array).unwrap();
        assert_eq!((clips.len(), back.clips.len()), (moving, moving), "the clips listed are the animations in the file");
        assert!(text(&e.report, "not_carried").contains("clip(s) with no tracks"), "{}", text(&e.report, "not_carried"));
        assert!(text(&e.report, "not_carried").contains("the other") && text(&e.report, "not_carried").contains("collider"));
        // Without animations: the rig is written, no clip, and that is said.
        let e = export(&shambler, 0, false).unwrap_or_else(|(c, m)| panic!("{c}: {m}"));
        let back = specula_gltf::import_bytes(&e.glb, None, &specula_gltf::ImportOptions::default()).unwrap();
        assert!(back.clips.is_empty() && back.skeleton.is_some());
        assert_eq!(e.report.get("clips").and_then(Json::as_array).map(<[Json]>::len), Some(0));
        assert!(text(&e.report, "not_carried").contains("left out as asked"));
        // Another LOD is another, smaller file; one the model does not have is refused.
        let lods = shambler.model.lods.len();
        assert!(lods > 1);
        let coarse = export(&shambler, lods - 1, true).unwrap_or_else(|(c, m)| panic!("{c}: {m}"));
        assert!(coarse.report.get("triangles").and_then(Json::as_u64) < said("triangles"));
        let Err((code, message)) = export(&shambler, lods, true) else { panic!("an LOD past the last is refused") };
        assert!(code == "no_such_lod" && message.contains(&format!("{lods} LOD(s)")), "{message}");
        // A prop: no bones, no clips, nothing said about clips.
        let statue = built("statue_angel");
        let e = export(&statue, 0, true).unwrap_or_else(|(c, m)| panic!("{c}: {m}"));
        assert_eq!((e.report.get("bones").and_then(Json::as_u64), e.report.get("clips").and_then(Json::as_array).map(<[Json]>::len)), (Some(0), Some(0)));
        assert!(!text(&e.report, "not_carried").contains("clip"));
        // Foliage: its baked cut-out texture is in the file (the live check's first picture
        // showed the grass as solid dark cards without it), and the wind's weights travel as
        // their own attribute, never as COLOR_0's alpha (which other engines read as opacity).
        let mut grass = built("grass_patch");
        // (Painted wind weights, as model_paint's alpha leaves them: every third vertex held still.)
        for c in grass.model.lods[0].colors.iter_mut().step_by(3) {
            c[3] = 64;
        }
        let e = export(&grass, 0, true).unwrap_or_else(|(c, m)| panic!("{c}: {m}"));
        assert_eq!(text(&e.report, "texture_sets"), r#"["grass_blades"]"#);
        let back = specula_gltf::import_bytes(&e.glb, None, &specula_gltf::ImportOptions::default()).unwrap();
        assert_eq!(e.report.get("images").and_then(Json::as_u64), Some(back.images.len() as u64));
        assert!(back.images.len() >= 2 && back.materials.iter().any(|m| m.base_color_texture.is_some() && m.alpha_mode == specula_gltf::AlphaMode::Mask));
        assert!(!text(&e.report, "not_carried").contains("baked texture") && text(&e.report, "not_carried").contains("_WIND"));
        let weights: Vec<u8> = grass.model.lods[0].colors.iter().map(|c| c[3]).collect();
        assert_eq!(back.model.lods[0].colors.iter().map(|c| c[3]).collect::<Vec<_>>(), weights, "and they come back as they were");
        // A terrain is a mesh in the file, and the answer says its heights are not data there.
        let ground = built("graveyard_ground");
        let e = export(&ground, 0, true).unwrap_or_else(|(c, m)| panic!("{c}: {m}"));
        assert!(e.report.get("triangles").and_then(Json::as_u64).unwrap_or(0) > 1000 && text(&e.report, "not_carried").contains("terrain's heights"));
    }

    fn said(out: &ToolOutput) -> Json {
        let text = out.content.iter().find_map(|c| match c {
            specula_protocol::Content::Text(t) => Some(t.as_str()),
            _ => None,
        });
        Json::parse(text.unwrap()).unwrap()
    }

    /// Review R1-2 and R1-6: what the tool says of a commit.
    #[test]
    fn the_answer_never_offers_to_undo_what_it_did_not_write() {
        let report = || Json::obj([("triangles", Json::from(12))]);
        let path = "assets/models/exports/statue.glb";
        // Written: the file's facts, and how to take the write back.
        let out = finish(report(), args(r#"{"path": "assets/models/exports/statue.glb", "version": 3, "content": "ab12", "journal": 41}"#), false, path);
        let j = said(&out);
        assert!(!out.is_error && j.get("version").and_then(Json::as_u64) == Some(3) && j.get("triangles").and_then(Json::as_u64) == Some(12));
        let next = j.get("next").and_then(Json::as_str).unwrap();
        assert!(next.contains("world_undo takes this write back") && next.contains(r#"model_build {recipe: "models/exports/statue.glb"}"#), "{next}");
        // Unchanged: nothing was written, so nothing of this call's is there to undo (an agent
        // following the old hint undid the change it had made before the export).
        let out = finish(report(), args(r#"{"path": "assets/models/exports/statue.glb", "version": 3, "unchanged": true, "content": "ab12"}"#), false, path);
        let j = said(&out);
        let next = j.get("next").and_then(Json::as_str).unwrap();
        assert!(!out.is_error && j.get("unchanged").and_then(Json::as_bool) == Some(true));
        assert!(!next.contains("world_undo") && next.contains("nothing was written") && next.contains("model_build"), "{next}");
        // Refused: in this tool's words. There is no upload to continue or abort.
        let stale = r#"{"error": "stale_version", "message": "assets/models/exports/statue.glb is at version 4, not 7: someone changed it since you read it; nothing was changed (the upload stays open: commit again with expect_version 4, or abort it)", "version": 4, "upload": "u-9"}"#;
        let out = finish(report(), args(stale), true, path);
        let j = said(&out);
        assert!(out.is_error && j.get("error").and_then(Json::as_str) == Some("stale_version") && j.get("version").and_then(Json::as_u64) == Some(4));
        assert!(!j.to_string().contains("upload") && !j.to_string().contains("abort"), "{j}");
        let note = j.get("note").and_then(Json::as_str).unwrap();
        assert!(j.get("message").and_then(Json::as_str).unwrap().ends_with("nothing was changed") && note.starts_with("nothing was written"));
        assert!(note.contains("expect_version set to the file's version now"), "a stale version is put right by the version that is there: {note}");
        let claimed = said(&finish(report(), args(r#"{"error": "claimed", "message": "bob holds it"}"#), true, path));
        assert_eq!(claimed.get("note").and_then(Json::as_str), Some("nothing was written: export again once the problem is gone"));
        let invalid = r#"{"error": "invalid", "message": "x.glb is not a loadable glb file; nothing was changed (abort the upload, or fix the file and upload again)", "upload": "u-9", "note": "the upload stays open"}"#;
        let j = said(&finish(report(), args(invalid), true, path));
        assert!(!j.to_string().contains("upload"), "{j}");
        // A commit the engine did not confirm may still land: never "nothing was written".
        let j = said(&finish(report(), args(r#"{"error": "no_answer", "message": "the engine didn't answer the commit"}"#), true, path));
        let note = j.get("note").and_then(Json::as_str).unwrap();
        assert!(!note.contains("nothing was written") && note.contains("project_read") && note.contains(path), "{note}");
    }

    /// Review R2 (the gap left in R1-9): a commit the project store refuses leaves no upload
    /// of this call's open. An agent has four; a leak would turn the fifth refused export into
    /// "too many uploads" for every project_upload after it. The engine here is a stub that
    /// answers what the tool asks: a model path, who the caller is, the built model, and a
    /// commit refused for another agent's claim.
    #[test]
    fn a_refused_commit_leaves_no_upload_open() {
        use crate::project_tools::{Committed, ProjectCall, Snap};
        let dir = std::env::temp_dir().join(format!("specula-model-export-slots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Arc::new(crate::blobs::ContentStore::open(&dir.join("content")).unwrap());
        let (tx, rx) = std::sync::mpsc::channel::<crate::tools::Request>();
        let statue = Arc::new(built("statue_angel"));
        // What the store holds at the path, and whether another agent claims it (set below).
        let held: Arc<std::sync::Mutex<Option<crate::project::ProjectFile>>> = Arc::default();
        let (seen, claim) = (Arc::clone(&held), crate::world::WorldError::new("claimed", "/project/assets/models is claimed by bob; nothing was changed"));
        std::thread::spawn(move || {
            for r in rx {
                match r.command {
                    Command::Project(ProjectCall::Snapshot { reply, .. }) => {
                        let current = seen.lock().unwrap().clone();
                        let claimed = current.is_some().then(|| claim.clone());
                        let _ = reply.send(Snap { kind: Some(crate::project::FileKind::Glb), current, claimed, ..Snap::default() });
                    }
                    Command::Project(ProjectCall::Owner { reply }) => {
                        let _ = reply.send("ann".to_owned());
                    }
                    Command::ModelExport(_, slot) => *slot.lock().unwrap() = Some(Arc::clone(&statue)),
                    Command::Project(ProjectCall::Commit { reply, .. }) => {
                        let _ = reply.send(Committed::Refused(crate::world::WorldError::new("claimed", "/project/assets/models is claimed by bob; nothing was changed")));
                    }
                    _ => {}
                }
                let _ = r.reply.send(ToolOutput::text("{}"));
            }
        });
        let tools = Tools::new(tx, Vec::new()).with_content(Arc::clone(&store));
        let session = SessionInfo { id: "exp00001".into(), client_name: "test".into(), client_version: "1".into(), protocol_version: "x".into() };
        // More refused exports than an agent has upload slots: each is refused for the claim,
        // in the tool's own words, and none for a full slot table.
        for n in 0..crate::blobs::MAX_UPLOADS_PER_AGENT + 2 {
            let out = call(&tools, &session, &args(r#"{"recipe": "recipes/statue_angel.json"}"#));
            let j = said(&out);
            assert!(out.is_error && j.get("error").and_then(Json::as_str) == Some("claimed"), "export {n}: {j}");
            assert!(!j.to_string().contains("upload") && j.get("note").and_then(Json::as_str).is_some_and(|n| n.starts_with("nothing was written")), "{j}");
        }
        // Review R2, N3: the file already holds exactly this export, and another agent claims
        // the path. Nothing would change, and it is refused all the same, as a write would be
        // (it answered `unchanged`, the claim unread).
        let glb = export(&built("statue_angel"), 0, true).unwrap_or_else(|(c, m)| panic!("{c}: {m}")).glb;
        let pointer = crate::blobs::pointer(store.put(&glb).unwrap(), glb.len() as u64);
        *held.lock().unwrap() = Some(crate::project::ProjectFile::new(crate::project::FileKind::Glb, Arc::from(pointer), 3, Arc::from("bob")));
        let out = call(&tools, &session, &args(r#"{"recipe": "recipes/statue_angel.json"}"#));
        let j = said(&out);
        assert!(out.is_error && j.get("error").and_then(Json::as_str) == Some("claimed") && j.get("unchanged").is_none(), "{j}");
        // And every slot is free: the agent starts as many uploads as an agent may hold.
        let by = crate::project_binary::owner(&tools, &session).expect("the stub says who the caller is");
        for n in 0..crate::blobs::MAX_UPLOADS_PER_AGENT {
            let started = tools.uploads.begin(&store, &by, &format!("assets/models/slot{n}.glb"), 4, 0, None);
            assert!(started.is_ok(), "slot {n} is free: {:?}", started.err().map(|e| e.to_json().to_string()));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Review R1-4 and R1-5: an image is counted only if the file has it, and a texture set no
    /// part is drawn with is said.
    #[test]
    fn images_counted_are_images_written_and_unused_sets_are_said() {
        // A set with an emissive map on a preset that does not emit: the file has no emissive
        // image, and the answer does not count one.
        let mut grass = built("grass_patch");
        let key = grass.model.extras.textures[0].0.clone();
        let mut glowing = (*grass.model.extras.textures[0].1).clone();
        glowing.emissive = Some(glowing.base_color.clone());
        glowing.hash ^= 7;
        let plain_images = 2 + usize::from(glowing.normal.is_some());
        grass.model.extras.textures[0].1 = Arc::new(glowing);
        assert!(grass.model.materials.iter().any(|(name, p)| name == &key && p.emissive == 0.0), "the grass preset does not emit");
        let e = export(&grass, 0, true).unwrap_or_else(|(c, m)| panic!("{c}: {m}"));
        let back = specula_gltf::import_bytes(&e.glb, None, &specula_gltf::ImportOptions::default()).unwrap();
        assert_eq!((e.report.get("images").and_then(Json::as_u64), back.images.len()), (Some(plain_images as u64), plain_images));
        // The preset emits: the image is written, and counted.
        for (name, preset) in &mut grass.model.materials {
            if name == &key {
                preset.emissive = 1.5;
            }
        }
        let e = export(&grass, 0, true).unwrap_or_else(|(c, m)| panic!("{c}: {m}"));
        let back = specula_gltf::import_bytes(&e.glb, None, &specula_gltf::ImportOptions::default()).unwrap();
        assert_eq!((e.report.get("images").and_then(Json::as_u64), back.images.len()), (Some(plain_images as u64 + 1), plain_images + 1));
        assert!(back.materials.iter().any(|m| m.emissive_texture.is_some()));
        // A set under a key no part is drawn with (a terrain's layer texture): not in the
        // file, and said.
        let set = Arc::clone(&grass.model.extras.textures[0].1);
        grass.model.extras.textures.push(("terrain+moss".to_owned(), set));
        let e = export(&grass, 0, true).unwrap_or_else(|(c, m)| panic!("{c}: {m}"));
        assert!(text(&e.report, "not_carried").contains("no part of this mesh is drawn with (terrain+moss)"), "{}", text(&e.report, "not_carried"));
        let plain = built("statue_angel");
        assert!(!text(&export(&plain, 0, true).unwrap_or_else(|(c, m)| panic!("{c}: {m}")).report, "not_carried").contains("texture set"));
    }

    /// A real engine (GPU-backed; skips cleanly without Vulkan).
    fn engine() -> Option<crate::Engine> {
        if !specula_gpu::is_available() {
            crate::skip_test("model_export", "Vulkan is not available");
            return None;
        }
        let gpu = match specula_gpu::Gpu::new(&specula_gpu::GpuOptions { validation: specula_gpu::Validation::Auto, ..Default::default() }) {
            Ok(g) => g,
            Err(e) => {
                crate::skip_test("model_export", &e.to_string());
                return None;
            }
        };
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        let captures = std::env::temp_dir().join(format!("specula-model-export-tests-{}", std::process::id()));
        Some(crate::Engine::new(gpu, &captures, crate::models::Library::new(&assets, None)).expect("engine"))
    }

    /// Review R1-9: the engine lends the built model on both of its paths, a build that is
    /// waited for and one the library already has; a model_build lends nothing; a recipe that
    /// fails to build lends nothing and says why.
    #[test]
    fn the_engine_lends_the_built_model_whether_built_now_or_before() {
        use std::time::{Duration, Instant};
        let Some(mut e) = engine() else { return };
        let ask = |e: &mut crate::Engine, command: Command| -> ToolOutput {
            let (reply, rx) = crate::jobs::channel();
            let request = crate::tools::Request { session: crate::editor_engine::window_session(), call: 0, command, reply, deadline: Instant::now() + Duration::from_secs(120), trace: specula_core::trace::Ctx::current(), via: None };
            e.handle_model_call(request);
            let give_up = Instant::now() + Duration::from_secs(120);
            loop {
                // (This test is the engine thread too: it waits a moment, then pumps the builds.)
                if let Ok(out) = rx.recv_timeout(Duration::from_millis(2)) {
                    return out;
                }
                assert!(Instant::now() < give_up, "the build never answered");
                // (The source is read on a worker first: its return is taken here.)
                e.pump_workers(None);
                e.pump_builds(Some(Duration::from_millis(20)));
            }
        };
        let spec = ModelSpec { recipe: "recipes/statue_angel.json".to_owned(), params: Vec::new(), seed: None };
        let cached = |out: &ToolOutput| said(out).get("cached").and_then(Json::as_bool);
        // Built now (the caller waits for the job).
        let slot = BuiltSlot::default();
        let out = ask(&mut e, Command::ModelExport(spec.clone(), Arc::clone(&slot)));
        assert!(!out.is_error && cached(&out) == Some(false), "{out:?}");
        let first = slot.lock().unwrap().take().expect("lent after the build finished");
        assert_eq!(first.model.name, "statue_angel");
        // Built before (the library has it).
        let slot = BuiltSlot::default();
        let out = ask(&mut e, Command::ModelExport(spec.clone(), Arc::clone(&slot)));
        assert!(!out.is_error && cached(&out) == Some(true), "{out:?}");
        let second = slot.lock().unwrap().take().expect("lent from the library's cache");
        assert!(Arc::ptr_eq(&first, &second), "the one build, lent twice");
        // What is lent exports.
        assert!(export(&second, 0, true).is_ok());
        // A recipe that is not there: an error, nothing lent.
        let slot = BuiltSlot::default();
        let out = ask(&mut e, Command::ModelExport(ModelSpec { recipe: "recipes/no_such_recipe.json".to_owned(), params: Vec::new(), seed: None }, Arc::clone(&slot)));
        assert!(out.is_error && slot.lock().unwrap().is_none(), "{out:?}");
    }

    #[test]
    fn a_file_name_comes_from_the_recipes_name() {
        assert_eq!(stem_of("recipes/warding_stone.json"), "warding_stone");
        assert_eq!(stem_of("recipes/props/iron fence (tall).json"), "iron_fence__tall_");
        assert_eq!(stem_of("recipes\\a.b.json"), "a_b");
        assert_eq!(stem_of(".json"), "model");
        assert_eq!(stem_of(&format!("recipes/{}.json", "n".repeat(200))).len(), 64);
    }

    #[test]
    fn the_schema_bounds_every_argument() {
        let def = &definitions()[0];
        let props = def.input_schema.get("properties").unwrap();
        for key in ["recipe", "path"] {
            assert_eq!(props.get(key).and_then(|p| p.get("maxLength")).and_then(Json::as_u64), Some(MAX_PATH_CHARS as u64), "{key}");
        }
        assert_eq!(props.get("lod").and_then(|p| p.get("maximum")).and_then(Json::as_u64), Some(MAX_LOD));
        assert_eq!(props.get("seed").and_then(|p| p.get("maximum")).and_then(Json::as_u64), Some(world::MAX_MODEL_SEED));
        assert_eq!(props.get("params").and_then(|p| p.get("maxProperties")).and_then(Json::as_u64), Some(world::MAX_MODEL_PARAMS as u64));
        assert_eq!(props.get("expect_version").and_then(|p| p.get("maximum")).and_then(Json::as_u64), Some(MAX_VERSION));
        // Every property is bounded: a length, a largest value, a count of members, or a boolean.
        let Json::Obj(fields) = props else { panic!("properties") };
        for (key, p) in fields {
            let bounded = ["maxLength", "maximum", "maxProperties"].iter().any(|b| p.get(b).is_some()) || p.get("type").and_then(Json::as_str) == Some("boolean");
            assert!(bounded, "{key} has no bound in the schema");
        }
        // And the parser holds the same bounds.
        let many: Vec<String> = (0..=world::MAX_MODEL_PARAMS).map(|i| format!("\"p{i}\": 1")).collect();
        assert!(parse(&args(&format!(r#"{{"recipe": "recipes/a.json", "params": {{{}}}}}"#, many.join(", ")))).is_err(), "more parameters than a recipe takes");
        assert!(parse(&args(r#"{"recipe": "recipes/a.json", "expect_version": 9007199254740992}"#)).is_err());
        // Every argument the parser takes is in the schema, and the other way round.
        let mut named: Vec<&str> = match props {
            Json::Obj(fields) => fields.iter().map(|(k, _)| k.as_str()).collect(),
            _ => Vec::new(),
        };
        named.sort_unstable();
        assert_eq!(named, ["animations", "dry_run", "expect_version", "lod", "params", "path", "recipe", "seed"]);
    }
}
