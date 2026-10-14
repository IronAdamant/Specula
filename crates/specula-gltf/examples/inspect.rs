//! Prints a compact report of a glTF / GLB file: what the document holds and what the importer
//! made of it (mesh, materials, textures, skeleton, clips, warnings).
//!
//! ```text
//! cargo run -p specula-gltf --example inspect -- path/to/asset.glb [--json]
//! ```
//!
//! `--json` prints `Import::report()` (the bounded summary tools return) instead.

use std::path::PathBuf;
use std::process::ExitCode;

use specula_gltf::{import_file, load_document, ImportOptions, Limits};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let Some(path) = args.iter().find(|a| !a.starts_with("--")).map(PathBuf::from) else {
        eprintln!("usage: inspect <file.gltf|file.glb> [--json]");
        return ExitCode::from(2);
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: reading {}: {e}", path.display());
            return ExitCode::from(1);
        }
    };
    let base = path.parent().map(|p| if p.as_os_str().is_empty() { std::path::Path::new(".") } else { p });
    let t0 = std::time::Instant::now();
    let imp = match import_file(&path, &ImportOptions::default()) {
        Ok(i) => i,
        Err(e) => {
            if json {
                println!("{}", e.to_json().to_pretty());
            } else {
                eprintln!("error: {e}");
            }
            return ExitCode::from(1);
        }
    };
    let elapsed = t0.elapsed();
    if json {
        println!("{}", imp.report().to_pretty());
        return ExitCode::SUCCESS;
    }
    let (doc, _) = load_document(&bytes, base, &Limits::default()).expect("imported above");
    let glb = bytes.starts_with(b"glTF");
    println!("{} ({}, {} KB, glTF {}{}) imported in {:.1} ms", path.display(), if glb { "GLB" } else { "JSON" }, bytes.len() / 1024, doc.version, doc.generator.as_ref().map_or(String::new(), |g| format!(", generator \"{g}\"")), elapsed.as_secs_f64() * 1e3);
    let prims: usize = doc.meshes.iter().map(|m| m.primitives.len()).sum();
    println!(
        "document: {} nodes, {} scenes, {} meshes ({prims} primitives), {} materials, {} textures, {} images, {} skins, {} animations, {} accessors, {} buffers",
        doc.nodes.len(),
        doc.scenes.len(),
        doc.meshes.len(),
        doc.materials.len(),
        doc.textures.len(),
        doc.images.len(),
        doc.skins.len(),
        doc.animations.len(),
        doc.accessors.len(),
        doc.buffers.len()
    );
    if !doc.extensions_used.is_empty() {
        println!("extensions used: {}", doc.extensions_used.join(", "));
    }
    let m = &imp.model;
    let mesh = &m.lods[0];
    let size = mesh.bounds().size();
    println!(
        "model '{}': {} vertices, {} triangles, {} parts, {} instances; size {:.3} x {:.3} x {:.3} m{}{}",
        m.name,
        mesh.vertex_count(),
        mesh.triangle_count(),
        mesh.parts.len(),
        imp.stats.instances,
        size[0].max(0.0),
        size[1].max(0.0),
        size[2].max(0.0),
        if mesh.uvs.is_empty() { "" } else { ", UVs" },
        if mesh.is_skinned() { ", skinned" } else { "" }
    );
    println!("materials:");
    for mt in &imp.materials {
        let tex = |t: &Option<specula_gltf::TextureRef>| t.map_or("-".to_string(), |t| t.texture.to_string());
        let tris: u32 = mesh.parts.iter().filter(|p| p.material == mt.name).map(|p| p.index_count / 3).sum();
        println!(
            "  {:<24} {:<8} rough {:.2} metal {:.2} emissive {:<5.2} {:?}{} tris {:<6} tex base {} mr {} normal {} occl {} emis {}{}{}",
            mt.name,
            format!("{:?}", mt.preset.kind).to_lowercase(),
            mt.preset.roughness,
            mt.preset.metallic,
            mt.preset.emissive,
            mt.alpha_mode,
            if mt.double_sided { " 2-sided" } else { "" },
            tris,
            tex(&mt.base_color_texture),
            tex(&mt.metallic_roughness_texture),
            tex(&mt.normal_texture),
            tex(&mt.occlusion_texture),
            tex(&mt.emissive_texture),
            if mt.texture_baked { " (baked into vertex colours)" } else { "" },
            if mt.from_specula { " [specula preset]" } else { "" }
        );
    }
    if !imp.textures.is_empty() {
        println!("textures:");
        for (i, t) in imp.textures.iter().enumerate() {
            let img = t.image.and_then(|k| imp.images.get(k));
            let desc = img.map_or("no image".to_string(), |im| match &im.error {
                None => format!("{}x{} {}", im.width, im.height, im.mime),
                Some(e) => format!("not decoded: {e}"),
            });
            println!("  {i}: {desc} (wrap {}/{})", t.sampler.wrap_s, t.sampler.wrap_t);
        }
    }
    if let Some(s) = &imp.skeleton {
        let depth = (0..s.len())
            .map(|mut b| {
                let mut d = 0;
                while let Some(p) = s.parents[b] {
                    b = p as usize;
                    d += 1;
                }
                d
            })
            .max()
            .unwrap_or(0);
        let names: Vec<&str> = s.names.iter().take(12).map(String::as_str).collect();
        println!("skeleton: {} bones, depth {depth}: {}{}", s.len(), names.join(", "), if s.len() > 12 { ", ..." } else { "" });
        if imp.stats.pruned_bones > 0 {
            println!("  ({} unused leaf bones pruned)", imp.stats.pruned_bones);
        }
    }
    if !imp.clips.is_empty() {
        println!("clips:");
        for c in &imp.clips {
            let keys: usize = c.tracks.iter().map(|t| t.keys.times.len()).sum();
            println!("  {:<24} {:.3} s, {:?}, {} tracks, {keys} keys", c.name, c.duration, c.looping, c.tracks.len());
        }
    }
    if imp.warnings.is_empty() {
        println!("warnings: none");
    } else {
        println!("warnings ({}):", imp.warnings.len());
        for w in imp.warnings.iter().take(40) {
            println!("  {w}");
        }
    }
    ExitCode::SUCCESS
}
