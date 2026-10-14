//! Builds a recipe from a game's asset folder and writes it as GLB (with its rig and the clips
//! of `anims/<name>.json` when there are any), for viewing in other tools.
//!
//! ```text
//! cargo run -p specula-gltf --example export_recipe -- games/sepulcra/assets shambler out/shambler.glb [tier=3]
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use specula_anim::{AnimSet, Skeleton};
use specula_gltf::{export_glb, ExportInput};
use specula_model::{build, BuildParams, Style};

fn run() -> Result<String, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [root, name, out, rest @ ..] = args.as_slice() else {
        return Err("usage: export_recipe <assets folder> <recipe name> <out.glb> [param=value ...]".into());
    };
    let root = PathBuf::from(root);
    let style = Style::parse(&std::fs::read_to_string(root.join("style.json")).map_err(|e| format!("style.json: {e}"))?).map_err(|e| e.to_string())?;
    let mut lib = BTreeMap::new();
    for e in std::fs::read_dir(root.join("recipes")).map_err(|e| format!("recipes: {e}"))? {
        let p = e.map_err(|e| e.to_string())?.path();
        if p.extension().is_some_and(|x| x == "json") {
            lib.insert(p.file_stem().unwrap_or_default().to_string_lossy().to_string(), std::fs::read_to_string(&p).map_err(|e| e.to_string())?);
        }
    }
    let text = lib.get(name).ok_or_else(|| format!("no recipe '{name}'"))?;
    let mut values = Vec::new();
    for kv in rest {
        let (k, v) = kv.split_once('=').ok_or_else(|| format!("'{kv}' is not param=value"))?;
        values.push((k, v.parse::<f64>().map_err(|e| format!("{kv}: {e}"))?));
    }
    let model = build(text, &BuildParams { values: &values, library: Some(&lib), ..BuildParams::new(&style) }).map_err(|e| e.to_string())?;
    let set = match (&model.rig, std::fs::read_to_string(root.join(format!("anims/{name}.json")))) {
        (Some(rig), Ok(anims)) => Some(AnimSet::load(&anims, Skeleton::from_rig(rig).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?),
        _ => None,
    };
    let clips: Vec<_> = set.as_ref().map(|s| s.clips().map(|(_, c)| c.clone()).collect()).unwrap_or_default();
    let glb = export_glb(&ExportInput { model: &model, lod: 0, skeleton: set.as_ref().map(|s| &s.skeleton), clips: &clips }).map_err(|e| e.to_string())?;
    if let Some(dir) = PathBuf::from(out).parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(out, &glb).map_err(|e| format!("{out}: {e}"))?;
    Ok(format!("{out}: {} bytes, {} triangles, {} bones, {} clips", glb.len(), model.lods[0].triangle_count(), model.rig.as_ref().map_or(0, |r| r.len()), clips.len()))
}

fn main() -> ExitCode {
    match run() {
        Ok(msg) => {
            println!("{msg}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
