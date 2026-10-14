//! An independent structural check of exported GLBs (it reads the bytes itself rather than
//! through our importer, so the exporter isn't only judged by its own reader): container and
//! padding, views inside the buffer, accessor alignment and bounds, POSITION / time min-max,
//! attribute targets and counts, unit normals and rotations, weights summing to one with no
//! repeated weighted joint, index ranges and targets, affine inverse binds, joints in the scene
//! under the skin's skeleton, unique animation channels, increasing times, output counts.
//! Ported from the glTF lane review's Python checker.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use specula_anim::{AnimSet, Skeleton};
use specula_core::Json;
use specula_gltf::{export_glb, ExportInput};
use specula_model::{build, BuildParams, Style};

struct Glb {
    doc: Json,
    bin: Vec<u8>,
}

fn u(v: Option<&Json>) -> usize {
    v.and_then(Json::as_u64).unwrap_or(0) as usize
}

fn arr<'a>(v: &'a Json, k: &str) -> &'a [Json] {
    v.get(k).and_then(Json::as_array).unwrap_or(&[])
}

fn ctype(c: usize) -> (usize, bool) {
    match c {
        5120 | 5121 => (1, c == 5120),
        5122 | 5123 => (2, c == 5122),
        5125 | 5126 => (4, false),
        _ => (0, false),
    }
}

fn dims(t: &str) -> usize {
    match t {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" | "MAT2" => 4,
        "MAT3" => 9,
        "MAT4" => 16,
        _ => 0,
    }
}

/// Elements of accessor `ai` as f64 (no normalization: raw component values).
fn read(g: &Glb, ai: usize, errs: &mut Vec<String>) -> Vec<Vec<f64>> {
    let a = &arr(&g.doc, "accessors")[ai];
    let c = u(a.get("componentType"));
    let (cs, _) = ctype(c);
    let n = dims(a.get("type").and_then(Json::as_str).unwrap_or(""));
    let v = &arr(&g.doc, "bufferViews")[u(a.get("bufferView"))];
    let off = u(v.get("byteOffset")) + u(a.get("byteOffset"));
    if cs == 0 || off % cs != 0 {
        errs.push(format!("accessor {ai} misaligned"));
        return Vec::new();
    }
    let elem = n * cs;
    let stride = v.get("byteStride").map_or(elem, |s| u(Some(s)));
    let count = u(a.get("count"));
    if u(a.get("byteOffset")) + stride * (count - 1) + elem > u(v.get("byteLength")) {
        errs.push(format!("accessor {ai} overruns its view"));
        return Vec::new();
    }
    (0..count)
        .map(|k| {
            (0..n)
                .map(|j| {
                    let p = off + k * stride + j * cs;
                    let b = &g.bin[p..p + cs];
                    match c {
                        5120 => b[0] as i8 as f64,
                        5121 => b[0] as f64,
                        5122 => i16::from_le_bytes([b[0], b[1]]) as f64,
                        5123 => u16::from_le_bytes([b[0], b[1]]) as f64,
                        5125 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
                        _ => f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
                    }
                })
                .collect()
        })
        .collect()
}

fn minmax(g: &Glb, ai: usize, errs: &mut Vec<String>) {
    let a = &arr(&g.doc, "accessors")[ai];
    let (Some(lo), Some(hi)) = (a.get("min").and_then(Json::as_array), a.get("max").and_then(Json::as_array)) else {
        errs.push(format!("accessor {ai} lacks min/max"));
        return;
    };
    let vals = read(g, ai, errs);
    for c in 0..lo.len() {
        let l = vals.iter().map(|v| v[c]).fold(f64::MAX, f64::min);
        let h = vals.iter().map(|v| v[c]).fold(f64::MIN, f64::max);
        if lo[c].as_f64().map(|x| x as f32 as f64) != Some(l) || hi[c].as_f64().map(|x| x as f32 as f64) != Some(h) {
            errs.push(format!("accessor {ai} min/max mismatch"));
        }
    }
}

fn unit_bad(v: &[f64], tol: f64) -> bool {
    (v.iter().map(|x| x * x).sum::<f64>().sqrt() - 1.0).abs() > tol
}

fn check(bytes: &[u8]) -> Vec<String> {
    let mut errs = Vec::new();
    let le = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap()) as usize;
    if le(0) != 0x4654_6C67 || le(4) != 2 || le(8) != bytes.len() {
        errs.push("header".into());
    }
    let (jl, jt) = (le(12), le(16));
    if jt != 0x4E4F_534A || jl % 4 != 0 {
        errs.push("JSON chunk type/length".into());
    }
    let doc = Json::parse(std::str::from_utf8(&bytes[20..20 + jl]).unwrap().trim_end_matches(' ')).unwrap();
    let p = 20 + jl;
    let mut bin = Vec::new();
    if p < bytes.len() {
        let (bl, bt) = (le(p), le(p + 4));
        if bt != 0x004E_4942 || bl % 4 != 0 || p + 8 + bl != bytes.len() {
            errs.push("BIN chunk".into());
        }
        bin = bytes[p + 8..p + 8 + bl].to_vec();
    }
    let b0 = &arr(&doc, "buffers")[0];
    let blen = u(b0.get("byteLength"));
    if b0.get("uri").is_some() || !(blen <= bin.len() && bin.len() < blen + 4) || bin[blen..].iter().any(|&b| b != 0) {
        errs.push("buffer 0 vs BIN chunk".into());
    }
    for v in arr(&doc, "bufferViews") {
        if u(v.get("byteLength")) < 1 || u(v.get("byteOffset")) + u(v.get("byteLength")) > blen {
            errs.push("view empty or past the buffer".into());
        }
    }
    let g = Glb { doc, bin };
    let accs = arr(&g.doc, "accessors");
    let views = arr(&g.doc, "bufferViews");
    let mut users: BTreeMap<usize, usize> = BTreeMap::new();
    for a in accs {
        if u(a.get("count")) < 1 {
            errs.push("accessor count 0".into());
        }
        *users.entry(u(a.get("bufferView"))).or_default() += 1;
    }
    let nodes = arr(&g.doc, "nodes");
    let mut parent = BTreeMap::new();
    for (i, n) in nodes.iter().enumerate() {
        for c in arr(n, "children") {
            if parent.insert(u(Some(c)), i).is_some() {
                errs.push("node with two parents".into());
            }
        }
        if let Some(r) = n.get("rotation").and_then(Json::as_array) {
            if unit_bad(&r.iter().map(|x| x.as_f64().unwrap()).collect::<Vec<_>>(), 5e-6) {
                errs.push(format!("node {i} rotation not unit"));
            }
        }
    }
    let scene = &arr(&g.doc, "scenes")[u(g.doc.get("scene"))];
    let mut in_scene = BTreeSet::new();
    let mut st: Vec<usize> = arr(scene, "nodes").iter().map(|x| u(Some(x))).collect();
    if st.iter().any(|r| parent.contains_key(r)) {
        errs.push("scene root has a parent".into());
    }
    while let Some(x) = st.pop() {
        in_scene.insert(x);
        st.extend(arr(&nodes[x], "children").iter().map(|c| u(Some(c))));
    }
    let skins = arr(&g.doc, "skins");
    for m in arr(&g.doc, "meshes") {
        for pr in arr(m, "primitives") {
            let at = pr.get("attributes").and_then(Json::as_object).unwrap();
            let attr = |k: &str| at.iter().find(|(n, _)| n == k).map(|(_, v)| u(Some(v)));
            let counts: BTreeSet<usize> = at.iter().map(|(_, v)| u(accs[u(Some(v))].get("count"))).collect();
            if counts.len() != 1 {
                errs.push("attribute counts differ".into());
            }
            let pos = attr("POSITION").unwrap();
            let nv = u(accs[pos].get("count"));
            minmax(&g, pos, &mut errs);
            for (k, v) in at {
                let ai = u(Some(v));
                let view = &views[u(accs[ai].get("bufferView"))];
                if !matches!(view.get("target").map(|t| u(Some(t))), None | Some(34962)) {
                    errs.push(format!("{k} view target"));
                }
                if (u(view.get("byteOffset")) + u(accs[ai].get("byteOffset"))) % 4 != 0 {
                    errs.push(format!("{k} not 4-byte aligned"));
                }
                if users[&u(accs[ai].get("bufferView"))] > 1 && view.get("byteStride").is_none() {
                    errs.push("shared attribute view without byteStride".into());
                }
            }
            let bad = read(&g, attr("NORMAL").unwrap(), &mut errs).iter().filter(|n| unit_bad(n, 4e-4)).count();
            if bad > 0 {
                errs.push(format!("{bad} non-unit normals"));
            }
            if let Some(c) = attr("COLOR_0") {
                if read(&g, c, &mut errs).iter().flatten().any(|x| !(0.0..=1.0).contains(x)) {
                    errs.push("colour out of 0..1".into());
                }
            }
            if let (Some(wa), Some(ja)) = (attr("WEIGHTS_0"), attr("JOINTS_0")) {
                let nj = arr(&skins[0], "joints").len();
                for (w, j) in read(&g, wa, &mut errs).iter().zip(read(&g, ja, &mut errs)) {
                    let used: Vec<u64> = j.iter().zip(w).filter(|(_, w)| **w > 0.0).map(|(j, _)| *j as u64).collect();
                    if w.iter().sum::<f64>() != 255.0 || used.iter().collect::<BTreeSet<_>>().len() != used.len() || j.iter().any(|&x| x as usize >= nj) {
                        errs.push("bad skin weights or joints".into());
                    }
                }
            }
            let ia = u(pr.get("indices"));
            if views[u(accs[ia].get("bufferView"))].get("target").map(|t| u(Some(t))) != Some(34963) {
                errs.push("index view target".into());
            }
            let idx = read(&g, ia, &mut errs);
            if idx.iter().any(|x| x[0] as usize >= nv) || idx.len() % 3 != 0 || (u(accs[ia].get("componentType")) == 5123 && idx.iter().any(|x| x[0] == 65535.0)) {
                errs.push("bad indices".into());
            }
        }
    }
    for s in skins {
        let joints: Vec<usize> = arr(s, "joints").iter().map(|j| u(Some(j))).collect();
        let ibm = read(&g, u(s.get("inverseBindMatrices")), &mut errs);
        if ibm.len() != joints.len() || ibm.iter().any(|m| (m[3], m[7], m[11], m[15]) != (0.0, 0.0, 0.0, 1.0)) {
            errs.push("inverse bind matrices".into());
        }
        let sk = s.get("skeleton").map(|x| u(Some(x)));
        for &j in &joints {
            if !in_scene.contains(&j) {
                errs.push("joint not in the scene".into());
            }
            if let Some(sk) = sk {
                let mut x = j;
                while x != sk {
                    match parent.get(&x) {
                        Some(&p) => x = p,
                        None => break,
                    }
                }
                if x != sk {
                    errs.push("skeleton is not a common root".into());
                }
            }
        }
    }
    for an in arr(&g.doc, "animations") {
        let mut seen = BTreeSet::new();
        for c in arr(an, "channels") {
            let t = c.get("target").unwrap();
            let path = t.get("path").and_then(Json::as_str).unwrap();
            if !seen.insert((u(t.get("node")), path.to_string())) {
                errs.push("duplicate channel".into());
            }
            let s = &arr(an, "samplers")[u(c.get("sampler"))];
            let input = u(s.get("input"));
            minmax(&g, input, &mut errs);
            let times: Vec<f64> = read(&g, input, &mut errs).iter().map(|v| v[0]).collect();
            if times[0] < 0.0 || times.windows(2).any(|w| w[1] <= w[0]) {
                errs.push("key times".into());
            }
            let out = read(&g, u(s.get("output")), &mut errs);
            let cubic = s.get("interpolation").and_then(Json::as_str) == Some("CUBICSPLINE");
            if out.len() != times.len() * if cubic { 3 } else { 1 } {
                errs.push("output count".into());
            }
            if path == "rotation" {
                let vals: Vec<&Vec<f64>> = if cubic { out.iter().skip(1).step_by(3).collect() } else { out.iter().collect() };
                if vals.iter().any(|v| unit_bad(v, 5e-6)) {
                    errs.push("rotation output not unit".into());
                }
            }
            if out.iter().flatten().any(|x| !x.is_finite()) {
                errs.push("non-finite output".into());
            }
        }
    }
    errs.sort();
    errs.dedup();
    errs
}

#[test]
fn every_sepulcra_export_is_structurally_valid() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
    let style = Style::parse(&std::fs::read_to_string(root.join("style.json")).unwrap()).unwrap();
    let mut lib = BTreeMap::new();
    for e in std::fs::read_dir(root.join("recipes")).unwrap() {
        let p = e.unwrap().path();
        lib.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
    }
    let mut checked = 0;
    for (name, text) in &lib {
        let model = build(text, &BuildParams { library: Some(&lib), ..BuildParams::new(&style) }).unwrap();
        let set = model.rig.as_ref().and_then(|r| std::fs::read_to_string(root.join(format!("anims/{name}.json"))).ok().map(|a| AnimSet::load(&a, Skeleton::from_rig(r).unwrap()).unwrap()));
        let clips: Vec<_> = set.as_ref().map(|s| s.clips().map(|(_, c)| c.clone()).collect()).unwrap_or_default();
        for lod in 0..model.lods.len() {
            let glb = export_glb(&ExportInput { model: &model, lod, skeleton: set.as_ref().map(|s| &s.skeleton), clips: &clips }).unwrap();
            let errs = check(&glb);
            assert!(errs.is_empty(), "{name} lod {lod}: {errs:?}");
            checked += 1;
        }
    }
    assert!(checked >= 15);
}

/// The checker itself catches broken files.
#[test]
fn the_checker_finds_defects() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
    let style = Style::parse(&std::fs::read_to_string(root.join("style.json")).unwrap()).unwrap();
    let text = std::fs::read_to_string(root.join("recipes/tombstone.json")).unwrap();
    let model = build(&text, &BuildParams::new(&style)).unwrap();
    let glb = export_glb(&ExportInput::new(&model)).unwrap();
    assert!(check(&glb).is_empty());
    let jl = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
    let mut doc = Json::parse(std::str::from_utf8(&glb[20..20 + jl]).unwrap().trim_end()).unwrap();
    let Json::Obj(pairs) = &mut doc else { unreachable!() };
    let Json::Arr(accs) = &mut pairs.iter_mut().find(|(k, _)| k == "accessors").unwrap().1 else { unreachable!() };
    let n = accs[1].get("count").and_then(Json::as_u64).unwrap();
    accs[1].set("count", Json::from(n + 1)); // NORMAL now overruns its view
    let mut text = doc.to_string().into_bytes();
    while text.len() % 4 != 0 {
        text.push(b' ');
    }
    let mut out = glb[..12].to_vec();
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(&glb[16..20]);
    out.extend_from_slice(&text);
    out.extend_from_slice(&glb[20 + jl..]);
    let total = out.len() as u32;
    out[8..12].copy_from_slice(&total.to_le_bytes());
    let errs = check(&out);
    assert!(errs.contains(&"accessor 1 overruns its view".to_string()) && errs.contains(&"attribute counts differ".to_string()), "{errs:?}");
}
