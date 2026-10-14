//! Builds a recipe (with its rig and `anims/<name>.json` clips) and writes it as a **textured**
//! GLB: cylindrical UVs, an embedded JPEG base colour (weathered bone bands), a PNG normal map
//! (ridges and pits) and a PNG metallic-roughness map, on every opaque material. For checking the
//! engine's glTF path end to end (scripts/anim_check.py).
//!
//! ```text
//! cargo run -p specula-gltf --example textured_creature -- games/sepulcra/assets shambler out/shambler_tex.glb
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use specula_anim::{AnimSet, Skeleton};
use specula_core::jpeg::encode_baseline;
use specula_core::deflate::Level;
use specula_core::png::encode_rgba8;
use specula_core::Json;
use specula_gltf::{export_glb, ExportInput};
use specula_model::{build, BuildParams, MaterialKind, Style};

const N: usize = 256;

/// Deterministic value noise in 0..1 on an `N`-periodic grid of `cell` texels.
fn noise(x: usize, y: usize, cell: usize, seed: u32) -> f32 {
    let h = |i: usize, j: usize| {
        let mut v = (i as u32 % (N / cell) as u32).wrapping_mul(374_761_393) ^ (j as u32 % (N / cell) as u32).wrapping_mul(668_265_263) ^ seed;
        v = (v ^ (v >> 13)).wrapping_mul(1_274_126_177);
        (v ^ (v >> 16)) as f32 / u32::MAX as f32
    };
    let (i, j) = (x / cell, y / cell);
    let (fx, fy) = ((x % cell) as f32 / cell as f32, (y % cell) as f32 / cell as f32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (a, b) = (h(i, j) + (h(i + 1, j) - h(i, j)) * s(fx), h(i, j + 1) + (h(i + 1, j + 1) - h(i, j + 1)) * s(fx));
    a + (b - a) * s(fy)
}

/// Height of the surface detail: horizontal ridges (bands), pits, fine grain.
fn height(x: usize, y: usize) -> f32 {
    let band = ((y as f32 / N as f32 * 8.0 * std::f32::consts::TAU).sin() * 0.5 + 0.5).powf(3.0);
    let pits = (noise(x, y, 16, 7) - 0.62).max(0.0) * 3.0;
    0.6 * band - pits + 0.15 * noise(x, y, 4, 3)
}

fn textures() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let (mut base, mut normal, mut mr) = (Vec::with_capacity(N * N * 4), Vec::with_capacity(N * N * 4), Vec::with_capacity(N * N * 4));
    for y in 0..N {
        for x in 0..N {
            let h = height(x, y);
            let grime = noise(x, y, 32, 11);
            // Pale bone, darker in the pits and grooves, with vertical streaks of grime.
            let streak = noise(x, y / 8, 8, 5) * 0.25;
            let tone = (0.62 + 0.3 * h - 0.25 * grime - streak).clamp(0.08, 1.0);
            base.extend_from_slice(&[(255.0 * tone) as u8, (238.0 * tone) as u8, (205.0 * tone) as u8, 255]);
            let (xl, xr, yu, yd) = ((x + N - 1) % N, (x + 1) % N, (y + N - 1) % N, (y + 1) % N);
            let (dx, dy) = (height(xr, y) - height(xl, y), height(x, yd) - height(x, yu));
            let n = [-dx * 3.0, -dy * 3.0, 1.0];
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            normal.extend_from_slice(&[(127.5 + 127.5 * n[0] / l) as u8, (127.5 + 127.5 * n[1] / l) as u8, (127.5 + 127.5 * n[2] / l) as u8, 255]);
            // Glossy where raised (polished bone), rough in the pits; no metal.
            mr.extend_from_slice(&[255, (255.0 * (0.85 - 0.45 * h.max(0.0))).clamp(30.0, 255.0) as u8, 0, 255]);
        }
    }
    (base, normal, mr)
}

/// Splits a GLB into its JSON and BIN chunks.
fn split(glb: &[u8]) -> Result<(Json, Vec<u8>), String> {
    let word = |i: usize| u32::from_le_bytes([glb[i], glb[i + 1], glb[i + 2], glb[i + 3]]) as usize;
    if glb.len() < 20 || &glb[0..4] != b"glTF" {
        return Err("not a GLB".into());
    }
    let jlen = word(12);
    let json = std::str::from_utf8(&glb[20..20 + jlen]).map_err(|e| e.to_string())?;
    let json = Json::parse(json.trim_end()).map_err(|e| e.message)?;
    let bin = if glb.len() > 28 + jlen { glb[28 + jlen..28 + jlen + word(20 + jlen)].to_vec() } else { Vec::new() };
    Ok((json, bin))
}

fn join(json: &Json, bin: &[u8]) -> Vec<u8> {
    let mut j = json.to_string().into_bytes();
    while j.len() % 4 != 0 {
        j.push(b' ');
    }
    let mut b = bin.to_vec();
    while b.len() % 4 != 0 {
        b.push(0);
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&((12 + 8 + j.len() + 8 + b.len()) as u32).to_le_bytes());
    out.extend_from_slice(&(j.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&j);
    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&b);
    out
}

fn arr_mut<'a>(json: &'a mut Json, key: &str) -> &'a mut Vec<Json> {
    let Json::Obj(pairs) = json else { panic!("not an object") };
    if !pairs.iter().any(|(k, _)| k == key) {
        pairs.push((key.into(), Json::Arr(Vec::new())));
    }
    match pairs.iter_mut().find(|(k, _)| k == key) {
        Some((_, Json::Arr(a))) => a,
        _ => panic!("'{key}' is not an array"),
    }
}

fn run() -> Result<String, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [root, name, out] = args.as_slice() else {
        return Err("usage: textured_creature <assets folder> <recipe name> <out.glb>".into());
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
    let mut model = build(text, &BuildParams { library: Some(&lib), ..BuildParams::new(&style) }).map_err(|e| e.to_string())?;
    let set = match (&model.rig, std::fs::read_to_string(root.join(format!("anims/{name}.json")))) {
        (Some(rig), Ok(anims)) => Some(AnimSet::load(&anims, Skeleton::from_rig(rig).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?),
        _ => None,
    };
    // Cylindrical UVs around the model's vertical axis: two wraps around, once up.
    let b = model.bounds;
    let (cx, cz, y0, h) = ((b.min[0] + b.max[0]) / 2.0, (b.min[2] + b.max[2]) / 2.0, b.min[1], (b.max[1] - b.min[1]).max(1e-3));
    let mesh = &mut model.lods[0];
    mesh.uvs = mesh
        .positions
        .iter()
        .map(|p| [2.0 * ((p[0] - cx).atan2(p[2] - cz) / std::f32::consts::TAU + 0.5), 1.5 * (1.0 - (p[1] - y0) / h)])
        .collect();
    let clips: Vec<_> = set.as_ref().map(|s| s.clips().map(|(_, c)| c.clone()).collect()).unwrap_or_default();
    let glb = export_glb(&ExportInput { model: &model, lod: 0, skeleton: set.as_ref().map(|s| &s.skeleton), clips: &clips }).map_err(|e| e.to_string())?;
    let (mut json, mut bin) = split(&glb)?;
    // The images, in the BIN chunk.
    let (base, normal, mr) = textures();
    let images = [("image/jpeg", encode_baseline(N as u32, N as u32, &base, 90)), ("image/png", encode_rgba8(N as u32, N as u32, &normal, Level::Default)), ("image/png", encode_rgba8(N as u32, N as u32, &mr, Level::Default))];
    for (i, (mime, bytes)) in images.iter().enumerate() {
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let views = arr_mut(&mut json, "bufferViews");
        views.push(Json::obj([("buffer", Json::from(0u32)), ("byteOffset", Json::from(bin.len())), ("byteLength", Json::from(bytes.len()))]));
        let view = views.len() - 1;
        bin.extend_from_slice(bytes);
        arr_mut(&mut json, "images").push(Json::obj([("bufferView", Json::from(view)), ("mimeType", Json::from(*mime)), ("name", Json::from(["bone_albedo", "bone_normal", "bone_mr"][i]))]));
    }
    // Trilinear, repeating.
    arr_mut(&mut json, "samplers").push(Json::obj([("magFilter", Json::from(9729u32)), ("minFilter", Json::from(9987u32)), ("wrapS", Json::from(10497u32)), ("wrapT", Json::from(10497u32))]));
    let first_image = arr_mut(&mut json, "images").len() - 3;
    let tex_base = arr_mut(&mut json, "textures").len();
    for k in 0..3 {
        arr_mut(&mut json, "textures").push(Json::obj([("sampler", Json::from(0u32)), ("source", Json::from(first_image + k))]));
    }
    let mut textured = 0;
    for (m, (_, preset)) in arr_mut(&mut json, "materials").iter_mut().zip(&model.materials) {
        if preset.kind != MaterialKind::Standard || preset.emissive > 0.0 {
            continue; // glass and glowing parts stay as authored
        }
        let mut pbr = m.get("pbrMetallicRoughness").cloned().unwrap_or(Json::obj(Vec::<(String, Json)>::new()));
        pbr.set("baseColorTexture", Json::obj([("index", Json::from(tex_base))]));
        pbr.set("metallicRoughnessTexture", Json::obj([("index", Json::from(tex_base + 2))]));
        m.set("pbrMetallicRoughness", pbr);
        m.set("normalTexture", Json::obj([("index", Json::from(tex_base + 1)), ("scale", Json::from(1.0))]));
        textured += 1;
    }
    if let Some(Json::Arr(buffers)) = json.get("buffers").cloned() {
        let mut buffers = buffers;
        if let Some(b0) = buffers.first_mut() {
            b0.set("byteLength", Json::from(bin.len()));
        }
        json.set("buffers", Json::Arr(buffers));
    }
    let out_bytes = join(&json, &bin);
    if let Some(dir) = PathBuf::from(out).parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(out, &out_bytes).map_err(|e| format!("{out}: {e}"))?;
    Ok(format!(
        "{out}: {} bytes, {} triangles, {} bones, {} clips, {textured} textured materials",
        out_bytes.len(),
        model.lods[0].triangle_count(),
        model.rig.as_ref().map_or(0, |r| r.len()),
        clips.len()
    ))
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
