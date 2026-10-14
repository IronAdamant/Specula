//! Mutation fuzzing: byte-level damage to GLBs and structural damage to their JSON (numbers,
//! types, missing keys) must give `Ok` or a structured error, never a panic, and stay fast.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::*;
use specula_anim::math::qaxis;
use specula_anim::{AnimSet, Skeleton};
use specula_core::rng::Rng;
use specula_core::{jpeg, png, Json};
use specula_gltf::{export_glb, import_bytes, ExportInput, ImportOptions, Limits};
use specula_model::{build, BuildParams, Style};

fn seeds() -> Vec<Vec<u8>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
    let style = Style::parse(&std::fs::read_to_string(root.join("style.json")).unwrap()).unwrap();
    let mut lib = BTreeMap::new();
    for e in std::fs::read_dir(root.join("recipes")).unwrap() {
        let p = e.unwrap().path();
        lib.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
    }
    let mut out = Vec::new();
    for name in ["skeleton_runner", "tombstone"] {
        let model = build(&lib[name], &BuildParams { library: Some(&lib), ..BuildParams::new(&style) }).unwrap();
        let set = model.rig.as_ref().map(|r| AnimSet::load(&std::fs::read_to_string(root.join(format!("anims/{name}.json"))).unwrap(), Skeleton::from_rig(r).unwrap()).unwrap());
        let clips: Vec<_> = set.as_ref().map(|s| s.clips().map(|(_, c)| c.clone()).collect()).unwrap_or_default();
        out.push(export_glb(&ExportInput { model: &model, lod: 0, skeleton: set.as_ref().map(|s| &s.skeleton), clips: &clips }).unwrap());
    }
    // Hand-built: textures (PNG + JPEG), sparse, strides, skin, node animation, cubic spline.
    let mut g = Gltf::new();
    let (p, n, i) = quad(&mut g);
    let uv = g.floats(&[0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0], "VEC2");
    let px: Vec<u8> = (0..64).flat_map(|k| [k as u8 * 4, 255 - k as u8 * 4, 128, 255]).collect();
    let pngv = g.view(&png::encode_rgba8(8, 8, &px, specula_core::deflate::Level::Fastest), None);
    let jpgv = g.view(&jpeg::encode(8, 8, &px, &jpeg::EncodeOptions { progressive: true, ..Default::default() }).unwrap(), None);
    g.push("images", Json::obj([("bufferView", Json::from(pngv)), ("mimeType", Json::Str("image/png".into()))]));
    g.push("images", Json::obj([("bufferView", Json::from(jpgv)), ("mimeType", Json::Str("image/jpeg".into()))]));
    g.push("textures", Json::obj([("source", Json::from(0u32))]));
    g.push("textures", Json::obj([("source", Json::from(1u32))]));
    let m0 = g.push("materials", Json::obj([("pbrMetallicRoughness", Json::obj([("baseColorTexture", Json::obj([("index", Json::from(0u32))]))]))]));
    let m1 = g.push("materials", Json::obj([("alphaMode", Json::Str("BLEND".into())), ("emissiveTexture", Json::obj([("index", Json::from(1u32))])), ("pbrMetallicRoughness", Json::obj([("baseColorTexture", Json::obj([("index", Json::from(1u32))]))]))]));
    let mesh_a = g.mesh(&[("POSITION", p), ("NORMAL", n), ("TEXCOORD_0", uv)], Some(i), Some(m0), None);
    let mesh_b = g.mesh(&[("POSITION", p), ("TEXCOORD_0", uv)], None, Some(m1), Some(5));
    let jv = g.view(&[0u8, 1, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0], None);
    let ja = g.accessor(Some(jv), 0, U8, 4, "VEC4", false);
    let wa = g.floats(&[0.5, 0.5, 0.0, 0.0, 0.25, 0.75, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0], "VEC4");
    let mesh_s = g.mesh(&[("POSITION", p), ("NORMAL", n), ("JOINTS_0", ja), ("WEIGHTS_0", wa)], Some(i), None, None);
    let j1 = g.node(vec![("name", Json::Str("tip".into())), ("translation", Json::arr([0.0f32, 1.0, 0.0].map(Json::from)))]);
    let j0 = g.node(vec![("name", Json::Str("base".into())), ("children", Json::arr([Json::from(j1)]))]);
    let ibm = g.floats(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 1.0], "MAT4");
    g.push("skins", Json::obj([("joints", Json::arr([Json::from(j0), Json::from(j1)])), ("inverseBindMatrices", Json::from(ibm))]));
    let na = g.node(vec![("mesh", Json::from(mesh_a)), ("matrix", Json::arr([1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 2.0, 0.0, 0.0, 1.0].map(Json::from)))]);
    let nb = g.node(vec![("mesh", Json::from(mesh_b)), ("scale", Json::arr([1.0f32, -1.0, 1.0].map(Json::from)))]);
    let ns = g.node(vec![("mesh", Json::from(mesh_s)), ("skin", Json::from(0u32))]);
    let times = g.floats(&[0.0, 0.5, 1.0], "SCALAR");
    let q = qaxis([0.0, 0.0, 1.0], 120.0);
    let rots = g.floats(&[0.0, 0.0, 0.0, 1.0, q[0], q[1], q[2], q[3], 0.0, 0.0, 0.0, 1.0], "VEC4");
    let cub = g.floats(&[0.0; 27], "VEC3");
    g.push(
        "animations",
        Json::obj([
            ("samplers", Json::arr([Json::obj([("input", Json::from(times)), ("output", Json::from(rots))]), Json::obj([("input", Json::from(times)), ("output", Json::from(cub)), ("interpolation", Json::Str("CUBICSPLINE".into()))])])),
            (
                "channels",
                Json::arr([
                    Json::obj([("sampler", Json::from(0u32)), ("target", Json::obj([("node", Json::from(j1)), ("path", Json::Str("rotation".into()))]))]),
                    Json::obj([("sampler", Json::from(1u32)), ("target", Json::obj([("node", Json::from(nb)), ("path", Json::Str("translation".into()))]))]),
                ]),
            ),
        ]),
    );
    g.scene(&[na, nb, ns, j0]);
    // Sparse positions on a second copy of the quad's positions.
    let iv = g.view(&[1u8, 3], None);
    let vv = g.view(&[0u8; 24], None);
    let sp = g.accessor(Some(0), 0, F32, 4, "VEC3", false);
    let Json::Obj(pairs) = &mut g.root else { unreachable!() };
    let Json::Arr(accs) = &mut pairs.iter_mut().find(|(k, _)| k == "accessors").unwrap().1 else { unreachable!() };
    accs[sp].set(
        "sparse",
        Json::obj([
            ("count", Json::from(2u32)),
            ("indices", Json::obj([("bufferView", Json::from(iv)), ("componentType", Json::from(U8))])),
            ("values", Json::obj([("bufferView", Json::from(vv))])),
        ]),
    );
    let mesh_p = g.mesh(&[("POSITION", sp)], Some(i), None, None);
    let np = g.node(vec![("mesh", Json::from(mesh_p))]);
    let Json::Obj(pairs) = &mut g.root else { unreachable!() };
    let Json::Arr(scenes) = &mut pairs.iter_mut().find(|(k, _)| k == "scenes").unwrap().1 else { unreachable!() };
    let Json::Obj(sc) = &mut scenes[0] else { unreachable!() };
    let Json::Arr(list) = &mut sc.iter_mut().find(|(k, _)| k == "nodes").unwrap().1 else { unreachable!() };
    list.push(Json::from(np));
    let glb = g.glb();
    import_bytes(&glb, None, &ImportOptions::default()).unwrap_or_else(|e| panic!("hand-built seed must import: {e}"));
    out.push(glb);
    out
}

/// Splits a GLB into (JSON, BIN) (seeds are well formed).
fn split(glb: &[u8]) -> (Json, Vec<u8>) {
    let jlen = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
    let json = Json::parse(std::str::from_utf8(&glb[20..20 + jlen]).unwrap().trim_end()).unwrap();
    let bin = if glb.len() > 20 + jlen { glb[28 + jlen..].to_vec() } else { Vec::new() };
    (json, bin)
}

fn join(json: &Json, bin: &[u8]) -> Vec<u8> {
    let mut text = json.to_string().into_bytes();
    while text.len() % 4 != 0 {
        text.push(b' ');
    }
    let total = 12 + 8 + text.len() + 8 + bin.len();
    let mut out = Vec::new();
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
    out.extend_from_slice(&text);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x004E_4942u32.to_le_bytes());
    out.extend_from_slice(bin);
    out
}

fn count(v: &Json) -> usize {
    1 + match v {
        Json::Arr(a) => a.iter().map(count).sum(),
        Json::Obj(o) => o.iter().map(|(_, x)| count(x)).sum(),
        _ => 0,
    }
}

/// Replaces (or removes) the `k`-th value in pre-order.
fn mutate(v: &mut Json, k: &mut usize, rng: &mut Rng) -> bool {
    if *k == 0 {
        let numbers = [-1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 7.0, 255.0, 65535.0, 65536.0, 1e9, 4294967295.0, 0.5, -1e30, 1e30, 5120.0, 5121.0, 5123.0, 5125.0, 5126.0, 16.0];
        *v = match rng.below(7) {
            0..=2 => Json::Num(*rng.pick(&numbers).unwrap()),
            3 => Json::Null,
            4 => Json::Str(["", "x", "SCALAR", "MAT4", "VEC2", "BLEND", "STEP", "CUBICSPLINE", "weights", "../x.bin", "data:,", "data:a;base64,!!"][rng.below(12) as usize].into()),
            5 => Json::Arr(Vec::new()),
            _ => Json::Obj(Vec::new()),
        };
        return true;
    }
    *k -= 1;
    match v {
        Json::Arr(a) => {
            for x in a.iter_mut() {
                if mutate(x, k, rng) {
                    return true;
                }
            }
        }
        Json::Obj(o) => {
            for i in 0..o.len() {
                if *k == 0 && rng.chance(0.3) {
                    o.remove(i);
                    return true;
                }
                if mutate(&mut o[i].1, k, rng) {
                    return true;
                }
            }
        }
        _ => {}
    }
    false
}

#[test]
fn mutated_files_never_panic() {
    let seeds = seeds();
    // `SPECULA_FUZZ_SCALE=100` runs a hundred times as many mutants (with a fresh seed each scale).
    let scale: usize = std::env::var("SPECULA_FUZZ_SCALE").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut rng = Rng::new(0xF022 + scale as u64 - 1, 3);
    // Tight limits too, so limit paths are exercised.
    let small = ImportOptions { limits: Limits { max_vertices: 2000, max_image_pixels: 32, max_accessor_count: 5000, ..Limits::default() }, ..ImportOptions::default() };
    let normal = ImportOptions::default();
    let (mut ok, mut err) = (0, 0);
    let mut slowest = Duration::ZERO;
    let mut run = |bytes: &[u8], opts: &ImportOptions| {
        let t = Instant::now();
        match import_bytes(bytes, None, opts) {
            Ok(_) => ok += 1,
            Err(e) => {
                assert!(!e.message.is_empty());
                err += 1;
            }
        }
        slowest = slowest.max(t.elapsed());
    };
    for i in 0..2000 * scale {
        // Byte-level damage.
        let mut m = seeds[i % seeds.len()].clone();
        for _ in 0..1 + rng.below(3) {
            let at = rng.below(m.len() as u32) as usize;
            match rng.below(5) {
                0 => m[at] ^= 1 << rng.below(8),
                1 => m[at] = rng.below(256) as u8,
                2 => m.insert(at, rng.below(256) as u8),
                3 => {
                    m.remove(at);
                }
                _ => m.truncate(at),
            }
            if m.is_empty() {
                break;
            }
        }
        run(&m, if i % 4 == 0 { &small } else { &normal });
    }
    for i in 0..4000 * scale {
        // Structural damage to the JSON, binary chunk intact (with occasional bit flips).
        let (mut json, mut bin) = split(&seeds[i % seeds.len()]);
        for _ in 0..1 + rng.below(3) {
            let mut k = rng.below(count(&json) as u32) as usize;
            mutate(&mut json, &mut k, &mut rng);
        }
        if rng.chance(0.2) && !bin.is_empty() {
            let at = rng.below(bin.len() as u32) as usize;
            bin[at] ^= 1 << rng.below(8);
        }
        run(&join(&json, &bin), if i % 4 == 0 { &small } else { &normal });
    }
    println!("{ok} imported, {err} rejected; slowest {slowest:?}");
    assert!(ok > 300 && err > 300, "{ok} ok / {err} errors: mutations should exercise both paths");
    assert!(slowest < Duration::from_secs(5), "an import took {slowest:?}");
}
