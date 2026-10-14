//! Small hostile files must not turn into large allocations: shared image sources decode once
//! and within a budget, buffers read only their declared bytes, key budgets are checked before
//! key data is read, and vertex estimates follow the primitive mode. Also the remaining limits,
//! bone counts above 64, empty exports and bounded reports.

mod common;

use common::*;
use specula_core::{png, Json};
use specula_gltf::{export_glb, import_bytes, import_file, ExportInput, GltfErrorCode as C, ImportOptions, Limits};

fn opts(l: Limits) -> ImportOptions {
    ImportOptions { limits: l, ..ImportOptions::default() }
}

/// `n` materials, each using its own texture, every texture's image the same 512² PNG view
/// (or `distinct` views of the same bytes).
fn image_bomb(n: usize, distinct: bool) -> Gltf {
    let mut g = Gltf::new();
    let (p, nn, i) = quad(&mut g);
    let uv = g.floats(&[0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0], "VEC2");
    let pngb = png::encode_rgba8(512, 512, &vec![200u8; 512 * 512 * 4], specula_core::deflate::Level::Best);
    let shared = g.view(&pngb, None);
    for k in 0..n {
        let v = if distinct && k > 0 { g.view(&pngb, None) } else { shared };
        g.push("images", Json::obj([("bufferView", Json::from(v)), ("mimeType", Json::Str("image/png".into()))]));
        g.push("textures", Json::obj([("source", Json::from(k))]));
        g.push("materials", Json::obj([("pbrMetallicRoughness", Json::obj([("baseColorTexture", Json::obj([("index", Json::from(k))]))]))]));
    }
    let mesh = g.mesh(&[("POSITION", p), ("NORMAL", nn), ("TEXCOORD_0", uv)], Some(i), Some(0), None);
    let nd = g.node(vec![("mesh", Json::from(mesh))]);
    g.scene(&[nd]);
    g
}

#[test]
fn shared_image_sources_decode_once_within_a_budget() {
    let g = image_bomb(24, false);
    let imp = import_bytes(&g.glb(), None, &ImportOptions::default()).unwrap();
    let decoded: usize = imp.images.iter().map(|i| i.rgba.len()).sum();
    assert_eq!(decoded, 512 * 512 * 4, "one decode for 24 references");
    assert!(imp.images[1..].iter().all(|i| i.duplicate_of == Some(0) && i.width == 512));
    assert!(imp.textures.iter().all(|t| t.image == Some(0)));
    // Distinct sources count against max_image_bytes, checked from the header.
    let g = image_bomb(3, true);
    let e = import_bytes(&g.glb(), None, &opts(Limits { max_image_bytes: 2 * 512 * 512 * 4, ..Limits::default() })).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (C::Limit, "images[2]"), "{e}");
    // Images no material uses are not decoded.
    let mut g = image_bomb(1, false);
    let v = g.view(&png::encode_rgba8(8, 8, &[0u8; 256], specula_core::deflate::Level::Fastest), None);
    g.push("images", Json::obj([("bufferView", Json::from(v)), ("mimeType", Json::Str("image/png".into()))]));
    g.push("textures", Json::obj([("source", Json::from(1u32))]));
    let imp = import_bytes(&g.glb(), None, &ImportOptions::default()).unwrap();
    assert!(imp.images[1].rgba.is_empty() && imp.images[1].error.as_deref() == Some("not used by any material"));
}

#[test]
fn external_files_read_only_declared_bytes() {
    let dir = std::env::temp_dir().join(format!("specula-gltf-amp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("big.bin"), vec![7u8; 4 << 20]).unwrap();
    let mut g = Gltf::new();
    let (p, nn, i) = quad(&mut g);
    let mesh = g.mesh(&[("POSITION", p), ("NORMAL", nn)], Some(i), None, None);
    let nd = g.node(vec![("mesh", Json::from(mesh))]);
    g.scene(&[nd]);
    let with_buffers = |extra: Vec<Json>| {
        let mut root = Json::parse(&g.embedded()).unwrap();
        let mut bufs = root.get("buffers").unwrap().as_array().unwrap().to_vec();
        bufs.extend(extra);
        root.set("buffers", Json::Arr(bufs));
        std::fs::write(dir.join("a.gltf"), root.to_string()).unwrap();
    };
    // Twenty 4-byte buffers over a 4 MB file: only 80 bytes are read (a 2 KB budget suffices).
    with_buffers((0..20).map(|_| Json::obj([("byteLength", Json::from(4u32)), ("uri", Json::Str("big.bin".into()))])).collect());
    import_file(&dir.join("a.gltf"), &opts(Limits { max_total_bytes: 2048, ..Limits::default() })).unwrap();
    // A file shorter than byteLength is a bounds error.
    with_buffers(vec![Json::obj([("byteLength", Json::from((4u32 << 20) + 1)), ("uri", Json::Str("big.bin".into()))])]);
    let e = import_file(&dir.join("a.gltf"), &ImportOptions::default()).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (C::Bounds, "buffers[1].byteLength"), "{e}");
    // Declared sizes over max_total_bytes fail before reading.
    with_buffers(vec![Json::obj([("byteLength", Json::from(4u32 << 20)), ("uri", Json::Str("big.bin".into()))])]);
    let e = import_file(&dir.join("a.gltf"), &opts(Limits { max_total_bytes: 1 << 20, ..Limits::default() })).unwrap_err();
    assert_eq!(e.code, C::Limit, "{e}");
    // External image files count too; a missing one is a warning, not a failure.
    let mut g2 = image_bomb(1, false);
    let Json::Obj(pairs) = &mut g2.root else { unreachable!() };
    let Json::Arr(images) = &mut pairs.iter_mut().find(|(k, _)| k == "images").unwrap().1 else { unreachable!() };
    images[0] = Json::obj([("uri", Json::Str("tex.png".into()))]);
    std::fs::write(dir.join("b.bin"), &g2.bin).unwrap();
    std::fs::write(dir.join("b.gltf"), g2.external("b.bin")).unwrap();
    let imp = import_file(&dir.join("b.gltf"), &ImportOptions::default()).unwrap();
    assert_eq!(imp.warnings[0].code, "image_unreadable");
    assert_eq!(imp.warnings[0].path, "images[0].uri");
    std::fs::write(dir.join("tex.png"), png::encode_rgba8(64, 64, &vec![9u8; 64 * 64 * 4], specula_core::deflate::Level::Fastest)).unwrap();
    let imp = import_file(&dir.join("b.gltf"), &ImportOptions::default()).unwrap();
    assert_eq!(imp.images[0].width, 64);
    let tight = g2.bin.len() as u64 + 100;
    let e = import_file(&dir.join("b.gltf"), &opts(Limits { max_total_bytes: tight, ..Limits::default() })).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (C::Limit, "images[0].uri"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn key_budget_is_checked_before_reading_keys() {
    // 50 channels sharing one 100k-key sampler: 5M keys > the 4M budget.
    let keys = 100_000;
    let mut g = Gltf::new();
    let times: Vec<f32> = (0..keys).map(|k| k as f32 * 1e-3).collect();
    let ti = g.floats(&times, "SCALAR");
    let vi = g.floats(&vec![0.0f32; keys * 3], "VEC3");
    let kids: Vec<usize> = (0..50).map(|k| g.node(vec![("name", Json::Str(format!("n{k}")))])).collect();
    let root = g.node(vec![("children", Json::arr(kids.iter().map(|k| Json::from(*k))))]);
    let chans: Vec<Json> = kids.iter().map(|&k| Json::obj([("sampler", Json::from(0u32)), ("target", Json::obj([("node", Json::from(k)), ("path", Json::Str("translation".into()))]))])).collect();
    g.push("animations", Json::obj([("samplers", Json::arr([Json::obj([("input", Json::from(ti)), ("output", Json::from(vi))])])), ("channels", Json::Arr(chans))]));
    g.scene(&[root]);
    let e = import_bytes(&g.glb(), None, &ImportOptions::default()).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (C::Limit, "animations[0].channels[41]"), "{e}"); // 42 × 100k > 4Mi
    // Within budget the shared sampler is fine, and its times are read once.
    let imp = import_bytes(&g.glb(), None, &opts(Limits { max_total_keys: 6_000_000, ..Limits::default() })).unwrap();
    assert_eq!(imp.clips[0].tracks.len(), 50);
}

#[test]
fn vertex_estimates_follow_the_primitive_mode() {
    // A 2M-index strip without normals bakes to ~6M vertices: refused before reading indices.
    let mut g = Gltf::new();
    let p = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], "VEC3");
    let ia = g.accessor(None, 0, U32, 2_000_000, "SCALAR", false);
    let mesh = g.mesh(&[("POSITION", p)], Some(ia), None, Some(5));
    let nd = g.node(vec![("mesh", Json::from(mesh))]);
    g.scene(&[nd]);
    let e = import_bytes(&g.glb(), None, &ImportOptions::default()).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (C::Limit, "meshes"), "{e}");
    // max_triangles on its own.
    let mut g = Gltf::new();
    let (p, nn, i) = quad(&mut g);
    let mesh = g.mesh(&[("POSITION", p), ("NORMAL", nn)], Some(i), None, None);
    let nd = g.node(vec![("mesh", Json::from(mesh))]);
    g.scene(&[nd]);
    assert_eq!(import_bytes(&g.glb(), None, &opts(Limits { max_triangles: 1, ..Limits::default() })).unwrap_err().code, C::Limit);
    import_bytes(&g.glb(), None, &opts(Limits { max_triangles: 2, ..Limits::default() })).unwrap();
}

#[test]
fn rigs_above_64_bones_import_without_pruning() {
    let mut g = Gltf::new();
    let (p, nn, i) = quad(&mut g);
    let mesh = g.mesh(&[("POSITION", p), ("NORMAL", nn)], Some(i), None, None);
    let mut child = g.node(vec![("name", Json::Str("leaf".into())), ("mesh", Json::from(mesh)), ("translation", Json::arr([0.0f32, 0.1, 0.0].map(Json::from)))]);
    let leaf = child;
    for k in 0..99 {
        child = g.node(vec![("name", Json::Str(format!("b{k}"))), ("children", Json::arr([Json::from(child)])), ("translation", Json::arr([0.0f32, 0.1, 0.0].map(Json::from)))]);
    }
    let times = g.floats(&[0.0, 1.0], "SCALAR");
    let vals = g.floats(&[0.0, 0.1, 0.0, 0.0, 0.2, 0.0], "VEC3");
    g.push("animations", Json::obj([("samplers", Json::arr([Json::obj([("input", Json::from(times)), ("output", Json::from(vals))])])), ("channels", Json::arr([Json::obj([("sampler", Json::from(0u32)), ("target", Json::obj([("node", Json::from(leaf)), ("path", Json::Str("translation".into()))]))])]))]));
    g.scene(&[child]);
    let imp = import_bytes(&g.glb(), None, &ImportOptions::default()).unwrap();
    assert_eq!((imp.skeleton.as_ref().unwrap().len(), imp.stats.pruned_bones), (100, 0));
    assert!(imp.warnings.is_empty(), "{:?}", imp.warnings);
}

#[test]
fn empty_meshes_do_not_export_and_reports_stay_bounded() {
    let mut g = Gltf::new();
    let p = g.floats(&[0.0; 9], "VEC3");
    let nn = g.floats(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0], "VEC3");
    let long = "x".repeat(10_000);
    let m = g.push("materials", Json::obj([("name", Json::Str(long.clone()))]));
    let mesh = g.mesh(&[("POSITION", p), ("NORMAL", nn)], None, Some(m), None);
    let nd = g.node(vec![("mesh", Json::from(mesh))]);
    g.scene(&[nd]);
    let imp = import_bytes(&g.glb(), None, &ImportOptions { name: Some(long), ..ImportOptions::default() }).unwrap();
    assert_eq!(imp.model.lods[0].triangle_count(), 0);
    let e = export_glb(&ExportInput::new(&imp.model)).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (C::InvalidData, "mesh"), "{e}");
    assert!(imp.report().to_string().len() < 4096);
}
