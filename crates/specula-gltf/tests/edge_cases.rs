//! Hand-built glTF files for the cases our exporter never writes: strides, sparse accessors,
//! matrices, strips, flat normals, textures, bind-pose offsets, node animation, cubic splines,
//! quantized positions, malformed data, hostile URIs and limits.

mod common;

use common::*;
use specula_anim::blend::{self, Base};
use specula_anim::math::{qaxis, qrot};
use specula_anim::{skin_mesh, Interp};
use specula_core::{jpeg, png, Json};
use specula_gltf::color::linear_to_srgb8;
use specula_gltf::{import_bytes, import_file, GltfErrorCode as C, Import, ImportOptions, Limits};

fn import(g: &Gltf) -> Import {
    import_bytes(&g.glb(), None, &ImportOptions::default()).unwrap_or_else(|e| panic!("{e}"))
}

fn import_err(g: &Gltf) -> specula_gltf::GltfError {
    import_bytes(&g.glb(), None, &ImportOptions::default()).unwrap_err()
}

fn codes(i: &Import) -> Vec<&'static str> {
    i.warnings.iter().map(|w| w.code).collect()
}

/// Positions of the imported model posed by clip `clip` at time `t`.
fn posed(imp: &Import, clip: usize, t: f32) -> Vec<[f32; 3]> {
    let set = imp.anim_set().unwrap().unwrap();
    let id = set.clips().nth(clip).unwrap().0;
    let mut pose = set.skeleton.rest_pose();
    blend::sample(&set.skeleton, set.clip(id), set.track_bones(id), t, Base::Rest, &mut pose);
    let (mut m, mut s) = (Vec::new(), Vec::new());
    set.skeleton.model_space(&pose, &mut m);
    set.skeleton.skin_matrices(&m, &mut s);
    skin_mesh(&imp.model.lods[0], &s).positions
}

fn near(a: [f32; 3], b: [f32; 3], tol: f32) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= tol)
}

#[test]
fn quad_with_material_colours() {
    let mut g = Gltf::new();
    let (p, n, i) = quad(&mut g);
    let m = g.push("materials", Json::obj([("name", Json::Str("red paint".into())), ("pbrMetallicRoughness", Json::obj([("baseColorFactor", Json::arr([0.5f32, 0.25, 0.0, 1.0].map(Json::from))), ("metallicFactor", Json::from(0.0f32)), ("roughnessFactor", Json::from(0.6f32))]))]));
    let mesh = g.mesh(&[("POSITION", p), ("NORMAL", n)], Some(i), Some(m), None);
    let node = g.node(vec![("mesh", Json::from(mesh))]);
    g.scene(&[node]);
    let imp = import(&g);
    let mesh = &imp.model.lods[0];
    assert_eq!((mesh.vertex_count(), mesh.triangle_count()), (4, 2));
    assert_eq!(mesh.colors[0], [linear_to_srgb8(0.5), linear_to_srgb8(0.25), 0, 255]);
    let (name, preset) = &imp.model.materials[0];
    assert_eq!(name, "red paint");
    assert_eq!((preset.roughness, preset.metallic), (0.6, 0.0));
    assert!(!imp.materials[0].from_specula);
    assert!(imp.warnings.is_empty(), "{:?}", imp.warnings);
    assert!(imp.model.rig.is_none() && imp.skeleton.is_none());
    // The same file as .gltf with a data URI.
    let imp2 = import_bytes(g.embedded().as_bytes(), None, &ImportOptions::default()).unwrap();
    assert_eq!(imp2.model.lods[0].content_hash(), mesh.content_hash());
}

#[test]
fn interleaved_strides_normalized_colours_and_default_material() {
    let mut g = Gltf::new();
    // position f32×3 + colour u8×4 (normalized) per vertex, stride 16.
    let verts = [([0.0f32, 0.0, 0.0], [255u8, 0, 0, 255]), ([1.0, 0.0, 0.0], [0, 255, 0, 128]), ([0.0, 1.0, 0.0], [0, 0, 255, 0])];
    let mut bytes = Vec::new();
    for (p, c) in verts {
        p.iter().for_each(|v| bytes.extend_from_slice(&v.to_le_bytes()));
        bytes.extend_from_slice(&c);
    }
    let v = g.view(&bytes, Some(16));
    let pa = g.accessor(Some(v), 0, F32, 3, "VEC3", false);
    let ca = g.accessor(Some(v), 12, U8, 3, "VEC4", true);
    let mesh = g.mesh(&[("POSITION", pa), ("COLOR_0", ca)], None, None, None);
    let node = g.node(vec![("mesh", Json::from(mesh))]);
    g.scene(&[node]);
    let imp = import(&g);
    let m = &imp.model.lods[0];
    // No normals: flat (one vertex per corner), facing +Z.
    assert_eq!(m.vertex_count(), 3);
    assert!(m.normals.iter().all(|n| near(*n, [0.0, 0.0, 1.0], 1e-6)));
    assert_eq!(m.positions[1], [1.0, 0.0, 0.0]);
    assert_eq!((m.colors[0], m.colors[1][3], m.colors[2][3]), ([255, 0, 0, 255], 128, 0));
    // The glTF default material: white, metallic 1, roughness 1.
    assert_eq!(imp.model.materials[0].0, "default");
    assert_eq!(imp.model.materials[0].1.metallic, 1.0);
}

#[test]
fn sparse_accessors() {
    let mut g = Gltf::new();
    let base = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], "VEC3");
    // Move vertex 2 to (0, 2, 0) with a sparse substitution.
    let iv = g.view(&2u16.to_le_bytes(), None);
    let vals: Vec<u8> = [0.0f32, 2.0, 0.0].iter().flat_map(|v| v.to_le_bytes()).collect();
    let vv = g.view(&vals, None);
    let sparse = Json::obj([
        ("count", Json::from(1u32)),
        ("indices", Json::obj([("bufferView", Json::from(iv)), ("componentType", Json::from(U16))])),
        ("values", Json::obj([("bufferView", Json::from(vv))])),
    ]);
    let Json::Obj(pairs) = &mut g.root else { unreachable!() };
    let accs = &mut pairs.iter_mut().find(|(k, _)| k == "accessors").unwrap().1;
    let Json::Arr(list) = accs else { unreachable!() };
    list[base].set("sparse", sparse.clone());
    // An accessor with no bufferView at all: zeros plus the sparse value.
    let zeros = g.accessor(None, 0, F32, 3, "VEC3", false);
    let Json::Obj(pairs) = &mut g.root else { unreachable!() };
    let Json::Arr(list) = &mut pairs.iter_mut().find(|(k, _)| k == "accessors").unwrap().1 else { unreachable!() };
    list[zeros].set("sparse", sparse);
    let m0 = g.mesh(&[("POSITION", base)], None, None, None);
    let n0 = g.node(vec![("mesh", Json::from(m0))]);
    g.scene(&[n0]);
    let imp = import(&g);
    assert_eq!(imp.model.lods[0].positions[2], [0.0, 2.0, 0.0]);
    let (doc, _) = specula_gltf::load_document(&g.glb(), None, &Limits::default()).unwrap();
    let bufs = {
        let glb = g.glb();
        let mut b = g.bin.clone();
        b.resize(doc.buffers[0].byte_length, 0);
        let _ = glb;
        vec![b]
    };
    let z = specula_gltf::accessor::floats(&doc, &bufs, zeros, "test").unwrap();
    assert_eq!(z, vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0]);
}

#[test]
fn matrix_columns_are_padded() {
    // MAT2 of UNSIGNED_BYTE: each 2-byte column padded to 4 bytes.
    let mut g = Gltf::new();
    let v = g.view(&[1, 2, 0, 0, 3, 4, 0, 0, 5, 6, 0, 0, 7, 8, 0, 0], None);
    let a = g.accessor(Some(v), 0, U8, 2, "MAT2", false);
    let (doc, _) = specula_gltf::load_document(&g.glb(), None, &Limits::default()).unwrap();
    let f = specula_gltf::accessor::floats(&doc, std::slice::from_ref(&g.bin), a, "test").unwrap();
    assert_eq!(f, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
}

/// A closed tetrahedron (outward CCW) for winding checks.
fn tetra(g: &mut Gltf) -> usize {
    let p = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0], "VEC3");
    let i = g.u16s(&[0, 2, 1, 0, 1, 3, 1, 2, 3, 2, 0, 3]);
    g.mesh(&[("POSITION", p)], Some(i), None, None)
}

#[test]
fn matrix_nodes_mirrors_and_instancing() {
    let mut g = Gltf::new();
    let mesh = tetra(&mut g);
    // Parent: mirror in X and move +5 X (as a matrix); two children instance the same mesh.
    let child_a = g.node(vec![("mesh", Json::from(mesh))]);
    let child_b = g.node(vec![("mesh", Json::from(mesh)), ("translation", Json::arr([0.0f32, 3.0, 0.0].map(Json::from)))]);
    let m = [-1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 5.0, 0.0, 0.0, 1.0];
    let parent = g.node(vec![("matrix", Json::arr(m.map(Json::from))), ("children", Json::arr([Json::from(child_a), Json::from(child_b)]))]);
    g.scene(&[parent]);
    let imp = import(&g);
    let mesh = &imp.model.lods[0];
    assert_eq!(imp.stats.instances, 2);
    assert_eq!(mesh.triangle_count(), 8);
    // Mirrored, yet still outward-facing (winding flipped back).
    assert!(mesh.signed_volume() > 0.0, "{}", mesh.signed_volume());
    assert!(mesh.positions.contains(&[4.0, 0.0, 0.0]) && mesh.positions.contains(&[5.0, 4.0, 0.0]));
}

#[test]
fn strips_and_fans() {
    let mut g = Gltf::new();
    let p = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0], "VEC3");
    let strip = g.mesh(&[("POSITION", p)], None, None, Some(5));
    let fan = g.mesh(&[("POSITION", p)], None, None, Some(6));
    let a = g.node(vec![("mesh", Json::from(strip))]);
    let b = g.node(vec![("mesh", Json::from(fan))]);
    g.scene(&[a, b]);
    let imp = import(&g);
    let m = &imp.model.lods[0];
    assert_eq!(m.triangle_count(), 4);
    // Strip triangles alternate winding so every triangle faces +Z.
    assert!(m.normals[..6].iter().all(|n| near(*n, [0.0, 0.0, 1.0], 1e-6)), "{:?}", &m.normals[..6]);
}

#[test]
fn points_and_lines_are_rejected() {
    for mode in [0u32, 1, 2, 3] {
        let mut g = Gltf::new();
        let p = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0], "VEC3");
        let mesh = g.mesh(&[("POSITION", p)], None, None, Some(mode));
        let n = g.node(vec![("mesh", Json::from(mesh))]);
        g.scene(&[n]);
        let e = import_err(&g);
        assert_eq!((e.code, e.path.as_str()), (C::Unsupported, "meshes[0].primitives[0].mode"), "{e}");
    }
}

#[test]
fn malformed_files_give_structured_errors() {
    let good = || {
        let mut g = Gltf::new();
        let (p, n, i) = quad(&mut g);
        let mesh = g.mesh(&[("POSITION", p), ("NORMAL", n)], Some(i), None, None);
        let node = g.node(vec![("mesh", Json::from(mesh))]);
        g.scene(&[node]);
        g
    };
    let glb = good().glb();
    // Container.
    let mut b = glb.clone();
    b[0] = b'x';
    assert_eq!(import_bytes(&b, None, &ImportOptions::default()).unwrap_err().code, C::Json); // not GLB → parsed as JSON text
    let mut b = glb.clone();
    b[8] ^= 1;
    let e = import_bytes(&b, None, &ImportOptions::default()).unwrap_err();
    assert_eq!((e.code, e.offset), (C::Container, Some(8)), "{e}");
    let e = import_bytes(&glb[..glb.len() - 4], None, &ImportOptions::default()).unwrap_err();
    assert_eq!(e.code, C::Container, "{e}");
    // JSON syntax error with its offset.
    let e = import_bytes(b"{\"asset\": {\"version\": \"2.0\"},, }", None, &ImportOptions::default()).unwrap_err();
    assert_eq!((e.code, e.offset), (C::Json, Some(29)), "{e}");
    // Missing asset, wrong version, required extensions.
    assert_eq!(import_bytes(b"{}", None, &ImportOptions::default()).unwrap_err().code, C::MissingField);
    let e = import_bytes(br#"{"asset":{"version":"1.0"}}"#, None, &ImportOptions::default()).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (C::Unsupported, "asset.version"));
    let e = import_bytes(br#"{"asset":{"version":"2.0"},"extensionsRequired":["KHR_draco_mesh_compression"]}"#, None, &ImportOptions::default()).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (C::Unsupported, "extensionsRequired[0]"));
    // Bad references and types.
    let e = import_bytes(br#"{"asset":{"version":"2.0"},"nodes":[{"mesh":3}]}"#, None, &ImportOptions::default()).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (C::BadReference, "nodes[0].mesh"), "{e}");
    let e = import_bytes(br#"{"asset":{"version":"2.0"},"nodes":[{"translation":[0,"x",0]}]}"#, None, &ImportOptions::default()).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (C::WrongType, "nodes[0].translation[1]"), "{e}");
    // Cycles and shared children.
    let e = import_bytes(br#"{"asset":{"version":"2.0"},"nodes":[{"children":[1]},{"children":[0]}],"scenes":[{"nodes":[0]}]}"#, None, &ImportOptions::default()).unwrap_err();
    assert_eq!(e.code, C::InvalidData, "{e}");
    let e = import_bytes(br#"{"asset":{"version":"2.0"},"nodes":[{"children":[2]},{"children":[2]},{}],"scenes":[{"nodes":[0,1]}]}"#, None, &ImportOptions::default()).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (C::InvalidData, "nodes[1].children[0]"), "{e}");
    // Index out of range.
    let mut g = good();
    g.bin[48 + 48 + 2] = 9; // second index → 9
    let e = import_err(&g);
    assert_eq!((e.code, e.path.as_str()), (C::InvalidData, "meshes[0].primitives[0].indices"), "{e}");
    // NaN position.
    let mut g = good();
    g.bin[4..8].copy_from_slice(&f32::NAN.to_le_bytes());
    let e = import_err(&g);
    assert_eq!((e.code, e.path.as_str()), (C::InvalidData, "meshes[0].primitives[0].attributes.POSITION (accessor 0)"), "{e}");
    // Accessor past its buffer view.
    let mut g = good();
    let Json::Obj(pairs) = &mut g.root else { unreachable!() };
    let Json::Arr(list) = &mut pairs.iter_mut().find(|(k, _)| k == "accessors").unwrap().1 else { unreachable!() };
    list[0].set("count", Json::from(5u32));
    list[1].set("count", Json::from(5u32));
    let e = import_err(&g);
    assert_eq!((e.code, e.offset), (C::Bounds, Some(0)), "{e}");
    assert_eq!((e.measured, e.limit), (Some(60.0), Some(48.0)));
    // Buffer view past its buffer.
    let mut g = good();
    let Json::Obj(pairs) = &mut g.root else { unreachable!() };
    let Json::Arr(list) = &mut pairs.iter_mut().find(|(k, _)| k == "bufferViews").unwrap().1 else { unreachable!() };
    list[2].set("byteLength", Json::from(4000u32));
    let e = import_err(&g);
    assert_eq!((e.code, e.path.as_str()), (C::Bounds, "bufferViews[2]"), "{e}");
    // Every error serialises for tool responses.
    assert!(e.to_json().get("code").is_some());
}

#[test]
fn uris_stay_inside_the_asset_folder() {
    let dir = std::env::temp_dir().join(format!("specula-gltf-uri-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("asset/sub")).unwrap();
    let mut g = Gltf::new();
    let (p, n, i) = quad(&mut g);
    let mesh = g.mesh(&[("POSITION", p), ("NORMAL", n)], Some(i), None, None);
    let node = g.node(vec![("mesh", Json::from(mesh))]);
    g.scene(&[node]);
    std::fs::write(dir.join("outside.bin"), &g.bin).unwrap();
    std::fs::write(dir.join("asset/sub/in side.bin"), &g.bin).unwrap();
    let ok = dir.join("asset/ok.gltf");
    std::fs::write(&ok, g.external("sub/in%20side.bin")).unwrap();
    import_file(&ok, &ImportOptions::default()).unwrap();
    let abs = dir.join("outside.bin").to_string_lossy().replace('\\', "/");
    for uri in ["../outside.bin", "sub/../../outside.bin", "%2E%2E/outside.bin", "/outside.bin", abs.as_str(), "C:/outside.bin", "http://example.com/x.bin", "file:///x.bin", "sub\\..\\..\\outside.bin", "sub/%ZZ.bin", ""] {
        let f = dir.join("asset/bad.gltf");
        std::fs::write(&f, g.external(uri)).unwrap();
        let e = import_file(&f, &ImportOptions::default()).unwrap_err();
        assert_eq!((e.code, e.path.as_str()), (C::Uri, "buffers[0].uri"), "{uri}: {e}");
    }
    // A missing file is an I/O error with the path.
    std::fs::write(dir.join("asset/missing.gltf"), g.external("nope.bin")).unwrap();
    assert_eq!(import_file(&dir.join("asset/missing.gltf"), &ImportOptions::default()).unwrap_err().code, C::Io);
    // A link inside the folder that points outside it is refused (when links can be made).
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_file(dir.join("outside.bin"), dir.join("asset/link.bin")).is_ok();
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(dir.join("outside.bin"), dir.join("asset/link.bin")).is_ok();
    #[cfg(not(any(windows, unix)))]
    let linked = false;
    if linked {
        std::fs::write(dir.join("asset/link.gltf"), g.external("link.bin")).unwrap();
        assert_eq!(import_file(&dir.join("asset/link.gltf"), &ImportOptions::default()).unwrap_err().code, C::Uri);
    } else {
        println!("symlinks unavailable here; link containment not exercised");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A 4×4 texture: left half red, right half blue.
fn texture_pixels() -> Vec<u8> {
    (0..16).flat_map(|i| if i % 4 < 2 { [255, 0, 0, 255] } else { [0, 0, 255, 255] }).collect()
}

#[test]
fn png_and_jpeg_textures_decode_and_bake() {
    for (kind, bytes) in [("png", png::encode_rgba8(4, 4, &texture_pixels(), specula_core::deflate::Level::Default)), ("jpeg", jpeg::encode(4, 4, &texture_pixels(), &jpeg::EncodeOptions { quality: 100, subsampling: jpeg::Subsampling::S444, ..Default::default() }).unwrap())] {
        let mut g = Gltf::new();
        let (p, n, i) = quad(&mut g);
        let uv = g.floats(&[0.1, 0.5, 0.9, 0.5, 0.9, 0.5, 0.1, 0.5], "VEC2");
        let iv = g.view(&bytes, None);
        let mime = if kind == "png" { "image/png" } else { "image/jpeg" };
        g.push("images", Json::obj([("bufferView", Json::from(iv)), ("mimeType", Json::Str(mime.into()))]));
        g.push("samplers", Json::obj([("wrapS", Json::from(33071u32)), ("wrapT", Json::from(33071u32))]));
        g.push("textures", Json::obj([("source", Json::from(0u32)), ("sampler", Json::from(0u32))]));
        let m = g.push("materials", Json::obj([("pbrMetallicRoughness", Json::obj([("baseColorTexture", Json::obj([("index", Json::from(0u32))]))]))]));
        let mesh = g.mesh(&[("POSITION", p), ("NORMAL", n), ("TEXCOORD_0", uv)], Some(i), Some(m), None);
        let node = g.node(vec![("mesh", Json::from(mesh))]);
        g.scene(&[node]);
        let imp = import(&g);
        let img = &imp.images[0];
        assert_eq!((img.width, img.height, img.mime.as_str(), img.error.as_deref()), (4, 4, mime, None), "{kind}");
        assert_eq!(imp.textures[0].sampler.wrap_s, 33071);
        let mesh = &imp.model.lods[0];
        assert_eq!(mesh.uvs[1], [0.9, 0.5]);
        assert!(imp.materials[0].texture_baked);
        // Vertex 0 samples the red half, vertex 1 the blue half.
        let (c0, c1) = (mesh.colors[0], mesh.colors[1]);
        assert!(c0[0] > 200 && c0[2] < 60 && c1[2] > 200 && c1[0] < 60, "{kind}: {c0:?} {c1:?}");
        // With baking off the colours are the factor (white) and the image is still decoded.
        let imp = import_bytes(&g.glb(), None, &ImportOptions { bake_texture_colors: false, ..ImportOptions::default() }).unwrap();
        assert_eq!(imp.model.lods[0].colors[0], [255; 4]);
        assert!(!imp.images[0].rgba.is_empty());
        // Image limits are checked before decoding.
        let lim = ImportOptions { limits: Limits { max_image_pixels: 15, ..Limits::default() }, ..ImportOptions::default() };
        assert_eq!(import_bytes(&g.glb(), None, &lim).unwrap_err().code, C::Limit);
    }
    // An unsupported image format is a warning, not a failure.
    let mut g = Gltf::new();
    let (p, n, i) = quad(&mut g);
    let iv = g.view(b"RIFF\0\0\0\0WEBPVP8 ", None);
    g.push("images", Json::obj([("bufferView", Json::from(iv)), ("mimeType", Json::Str("image/webp".into()))]));
    g.push("textures", Json::obj([("source", Json::from(0u32))]));
    let m = g.push("materials", Json::obj([("pbrMetallicRoughness", Json::obj([("baseColorTexture", Json::obj([("index", Json::from(0u32))]))]))]));
    let mesh = g.mesh(&[("POSITION", p), ("NORMAL", n)], Some(i), Some(m), None);
    let node = g.node(vec![("mesh", Json::from(mesh))]);
    g.scene(&[node]);
    let imp = import(&g);
    assert_eq!(codes(&imp), vec!["image_not_decoded"]);
}

#[test]
fn skin_whose_bind_space_differs_from_the_rest_pose() {
    // glTF: world = G_joint(t) · IBM · p. The joint rests at (0, 1, 0), but the inverse bind
    // matrix was authored for a joint at (1, 1, 0): the rest mesh is shifted by −1 in X.
    let mut g = Gltf::new();
    let p = g.floats(&[0.0, 1.0, 0.0, 0.5, 1.0, 0.0, 0.0, 2.0, 0.0], "VEC3");
    let n = g.floats(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0], "VEC3");
    let jv = g.view(&[0u8; 12], None);
    let j = g.accessor(Some(jv), 0, U8, 3, "VEC4", false);
    let w = g.floats(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0], "VEC4");
    let mut ibm = [0.0f32; 16];
    ibm[0] = 1.0;
    ibm[5] = 1.0;
    ibm[10] = 1.0;
    ibm[15] = 1.0;
    ibm[12] = -1.0;
    ibm[13] = -1.0;
    let ibm_a = g.floats(&ibm, "MAT4");
    let mesh = g.mesh(&[("POSITION", p), ("NORMAL", n), ("JOINTS_0", j), ("WEIGHTS_0", w)], None, None, None);
    let joint = g.node(vec![("name", Json::Str("arm".into())), ("translation", Json::arr([0.0f32, 1.0, 0.0].map(Json::from)))]);
    let skinned = g.node(vec![("mesh", Json::from(mesh)), ("skin", Json::from(0u32)), ("translation", Json::arr([50.0f32, 0.0, 0.0].map(Json::from)))]);
    g.push("skins", Json::obj([("joints", Json::arr([Json::from(joint)])), ("inverseBindMatrices", Json::from(ibm_a))]));
    // Rotate the joint 90° about Z over one second.
    let times = g.floats(&[0.0, 1.0], "SCALAR");
    let q = qaxis([0.0, 0.0, 1.0], 90.0);
    let rots = g.floats(&[0.0, 0.0, 0.0, 1.0, q[0], q[1], q[2], q[3]], "VEC4");
    g.push(
        "animations",
        Json::obj([
            ("name", Json::Str("raise".into())),
            ("samplers", Json::arr([Json::obj([("input", Json::from(times)), ("output", Json::from(rots))])])),
            ("channels", Json::arr([Json::obj([("sampler", Json::from(0u32)), ("target", Json::obj([("node", Json::from(joint)), ("path", Json::Str("rotation".into()))]))])])),
        ]),
    );
    g.scene(&[joint, skinned]);
    let imp = import(&g);
    assert!(imp.warnings.is_empty(), "{:?}", imp.warnings);
    let skel = imp.skeleton.as_ref().unwrap();
    assert_eq!(skel.names, vec!["arm".to_string()]);
    // Baked to the rest pose (the skinned node's own transform is ignored, as glTF says).
    assert_eq!(imp.model.lods[0].positions[0], [-1.0, 1.0, 0.0]);
    // Posed: compare with glTF's formula at several times.
    for t in [0.0f32, 0.25, 0.5, 1.0] {
        let angle = 90.0 * t;
        let rot = qaxis([0.0, 0.0, 1.0], angle);
        let got = posed(&imp, 0, t);
        for (k, src) in [[0.0f32, 1.0, 0.0], [0.5, 1.0, 0.0], [0.0, 2.0, 0.0]].iter().enumerate() {
            // G(t) · IBM · p = T(0,1,0) · R · (p − (1, 1, 0)).
            let local = [src[0] - 1.0, src[1] - 1.0, src[2]];
            let r = qrot(rot, local);
            let want = [r[0], r[1] + 1.0, r[2]];
            // Specula nlerps between slerp keys ≤ 20° apart: within ~1.2e-4 of a true slerp.
            assert!(near(got[k], want, 2e-4), "t={t} vertex {k}: {:?} vs {want:?}", got[k]);
        }
    }
    // The 90° linear step got slerp keys (Specula interpolates with nlerp).
    assert!(imp.clips[0].tracks[0].keys.times.len() > 2);
}

#[test]
fn animated_nodes_bind_rigidly_with_a_synthetic_root() {
    let mut g = Gltf::new();
    let (p, n, i) = quad(&mut g);
    let mesh = g.mesh(&[("POSITION", p), ("NORMAL", n)], Some(i), None, None);
    let still = g.node(vec![("name", Json::Str("still".into())), ("mesh", Json::from(mesh)), ("translation", Json::arr([-3.0f32, 0.0, 0.0].map(Json::from)))]);
    let spin = g.node(vec![("name", Json::Str("blade".into())), ("mesh", Json::from(mesh)), ("translation", Json::arr([3.0f32, 0.0, 0.0].map(Json::from)))]);
    let times = g.floats(&[0.0, 1.0], "SCALAR");
    let q = qaxis([0.0, 1.0, 0.0], 90.0);
    let rots = g.floats(&[0.0, 0.0, 0.0, 1.0, q[0], q[1], q[2], q[3]], "VEC4");
    g.push(
        "animations",
        Json::obj([
            ("samplers", Json::arr([Json::obj([("input", Json::from(times)), ("output", Json::from(rots))])])),
            ("channels", Json::arr([Json::obj([("sampler", Json::from(0u32)), ("target", Json::obj([("node", Json::from(spin)), ("path", Json::Str("rotation".into()))]))])])),
        ]),
    );
    g.scene(&[still, spin]);
    let imp = import(&g);
    let skel = imp.skeleton.as_ref().unwrap();
    assert_eq!(skel.names, vec!["root".to_string(), "blade".to_string()]);
    assert_eq!(imp.clips[0].name, "animation_0");
    let m = &imp.model.lods[0];
    assert!(m.joints[..4].iter().all(|j| j[0] == 0) && m.joints[4..].iter().all(|j| j[0] == 1));
    // At t = 0.25 the blade has turned 22.5° about Y around its own origin.
    let got = posed(&imp, 0, 0.25);
    let r = qrot(qaxis([0.0, 1.0, 0.0], 22.5), [1.0, 0.0, 0.0]);
    assert!(near(got[5], [3.0 + r[0], r[1], r[2]], 2e-4), "{:?}", got[5]);
    assert_eq!(got[1], [-2.0, 0.0, 0.0]);
}

/// A translation channel with explicit CUBICSPLINE tangents.
fn cubic(tangents: &dyn Fn(usize) -> [f32; 3]) -> Gltf {
    let mut g = Gltf::new();
    let (p, n, i) = quad(&mut g);
    let mesh = g.mesh(&[("POSITION", p), ("NORMAL", n)], Some(i), None, None);
    let node = g.node(vec![("name", Json::Str("mover".into())), ("mesh", Json::from(mesh))]);
    let times = g.floats(&[0.0, 1.0, 2.0], "SCALAR");
    let vals = [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
    let mut data = Vec::new();
    for (k, v) in vals.iter().enumerate() {
        let t = tangents(k);
        data.extend_from_slice(&t);
        data.extend_from_slice(v);
        data.extend_from_slice(&t);
    }
    let out = g.floats(&data, "VEC3");
    g.push(
        "animations",
        Json::obj([
            ("name", Json::Str("move".into())),
            ("samplers", Json::arr([Json::obj([("input", Json::from(times)), ("output", Json::from(out)), ("interpolation", Json::Str("CUBICSPLINE".into()))])])),
            ("channels", Json::arr([Json::obj([("sampler", Json::from(0u32)), ("target", Json::obj([("node", Json::from(node)), ("path", Json::Str("translation".into()))]))])])),
        ]),
    );
    g.scene(&[node]);
    g
}

#[test]
fn cubic_splines_keep_catmull_rom_and_resample_the_rest() {
    // Zero tangents (ease in/out): resampled to linear keys that follow the Hermite curve.
    let imp = import(&cubic(&|_| [0.0; 3]));
    assert_eq!(codes(&imp), vec!["cubic_resampled"]);
    let keys = &imp.clips[0].tracks[0].keys;
    assert_eq!(keys.interp, Interp::Linear);
    for t in [0.37f32, 0.5, 1.3] {
        let (k, s) = if t < 1.0 { (0, t) } else { (1, t - 1.0) };
        let (a, b) = ([[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]][k], [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]][k + 1]);
        let h = -2.0 * s * s * s + 3.0 * s * s;
        let want = [a[0] + (b[0] - a[0]) * h, a[1] + (b[1] - a[1]) * h, 0.0];
        let got = keys.sample(t, false, None);
        assert!(near([got[0], got[1], got[2]], want, 1e-3), "t={t}: {got:?} vs {want:?}");
    }
    // Catmull–Rom tangents import as cubic keys, exactly.
    let cr = |k: usize| match k {
        0 => [1.0, 0.0, 0.0],
        1 => [0.5, 0.5, 0.0],
        _ => [0.0, 1.0, 0.0],
    };
    let imp = import(&cubic(&cr));
    assert!(imp.warnings.is_empty(), "{:?}", imp.warnings);
    let keys = &imp.clips[0].tracks[0].keys;
    assert_eq!((keys.interp, keys.times.len()), (Interp::Cubic, 3));
}

#[test]
fn quantized_positions_and_ignored_features() {
    let mut g = Gltf::new();
    g.set("extensionsUsed", Json::arr([Json::Str("KHR_mesh_quantization".into()), Json::Str("KHR_lights_punctual".into())]));
    g.set("extensionsRequired", Json::arr([Json::Str("KHR_mesh_quantization".into())]));
    // UNSIGNED_SHORT normalized positions, scaled by the node.
    let pos: Vec<u8> = [0u16, 0, 0, 65535, 0, 0, 0, 65535, 0, 0].iter().flat_map(|v| v.to_le_bytes()).collect();
    let pv = g.view(&pos[..18], None);
    let pa = g.accessor(Some(pv), 0, U16, 3, "VEC3", true);
    let target = g.floats(&[0.0, 0.0, 0.1, 0.0, 0.0, 0.1, 0.0, 0.0, 0.1], "VEC3");
    let mut mesh = Json::obj([("primitives", Json::arr([Json::obj([("attributes", Json::obj([("POSITION", Json::from(pa))])), ("targets", Json::arr([Json::obj([("POSITION", Json::from(target))])]))])]))]);
    mesh.set("weights", Json::arr([Json::from(0.0f32)]));
    let mi = g.push("meshes", mesh);
    let node = g.node(vec![("mesh", Json::from(mi)), ("scale", Json::arr([2.0f32, 2.0, 2.0].map(Json::from)))]);
    g.scene(&[node]);
    let imp = import(&g);
    let m = &imp.model.lods[0];
    assert!(m.positions.contains(&[2.0, 0.0, 0.0]) && m.positions.contains(&[0.0, 2.0, 0.0]));
    assert_eq!(codes(&imp), vec!["ignored_extension", "morph_targets_ignored"]);
}

#[test]
fn degenerate_triangles_are_dropped() {
    let mut g = Gltf::new();
    let p = g.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 2.0, 0.0, 0.0], "VEC3");
    let i = g.u16s(&[0, 1, 2, 0, 0, 1, 0, 1, 3]);
    let mesh = g.mesh(&[("POSITION", p)], Some(i), None, None);
    let node = g.node(vec![("mesh", Json::from(mesh))]);
    g.scene(&[node]);
    let imp = import(&g);
    assert_eq!((imp.model.lods[0].triangle_count(), imp.stats.degenerate_triangles), (1, 2));
    assert_eq!(codes(&imp), vec!["degenerate_triangles"]);
}

#[test]
fn limits_are_enforced() {
    let mut g = Gltf::new();
    let (p, n, i) = quad(&mut g);
    let mesh = g.mesh(&[("POSITION", p), ("NORMAL", n)], Some(i), None, None);
    // A chain of 40 nodes ending in the mesh.
    let mut child = g.node(vec![("mesh", Json::from(mesh))]);
    for _ in 0..39 {
        child = g.node(vec![("children", Json::arr([Json::from(child)]))]);
    }
    g.scene(&[child]);
    let glb = g.glb();
    import_bytes(&glb, None, &ImportOptions::default()).unwrap();
    let with = |l: Limits| import_bytes(&glb, None, &ImportOptions { limits: l, ..ImportOptions::default() }).unwrap_err();
    let d = Limits::default();
    let e = with(Limits { max_node_depth: 20, ..d });
    assert_eq!((e.code, e.limit), (C::Limit, Some(20.0)), "{e}");
    assert_eq!(with(Limits { max_vertices: 3, ..d }).code, C::Limit);
    assert_eq!(with(Limits { max_file_bytes: 100, ..d }).code, C::Limit);
    assert_eq!(with(Limits { max_items: 10, ..d }).code, C::Limit);
    assert_eq!(with(Limits { max_accessor_count: 3, ..d }).code, C::Limit);
}

#[test]
fn channels_specula_cannot_hold_are_dropped_with_a_warning() {
    let mut g = Gltf::new();
    let (p, n, i) = quad(&mut g);
    let mesh = g.mesh(&[("POSITION", p), ("NORMAL", n)], Some(i), None, None);
    let node = g.node(vec![("name", Json::Str("flip".into())), ("mesh", Json::from(mesh))]);
    let times = g.floats(&[0.0, 1.0], "SCALAR");
    let scales = g.floats(&[1.0, 1.0, 1.0, -1.0, 1.0, 1.0], "VEC3");
    let moves = g.floats(&[0.0, 0.0, 0.0, 0.0, 5.0, 0.0], "VEC3");
    let ch = |s: u32, path: &str| Json::obj([("sampler", Json::from(s)), ("target", Json::obj([("node", Json::from(node)), ("path", Json::Str(path.into()))]))]);
    g.push(
        "animations",
        Json::obj([
            ("samplers", Json::arr([Json::obj([("input", Json::from(times)), ("output", Json::from(scales))]), Json::obj([("input", Json::from(times)), ("output", Json::from(moves))])])),
            ("channels", Json::arr([ch(0, "scale"), ch(1, "translation")])),
        ]),
    );
    g.scene(&[node]);
    let imp = import(&g);
    assert_eq!(codes(&imp), vec!["channel_out_of_range"]);
    assert_eq!(imp.clips[0].tracks.len(), 1);
    assert_eq!(imp.clips[0].tracks[0].channel, specula_anim::Channel::Translation);
}

#[test]
fn zero_scale_nodes_and_the_warning_cap() {
    let mut g = Gltf::new();
    let (p, n, i) = quad(&mut g);
    let mesh = g.mesh(&[("POSITION", p), ("NORMAL", n)], Some(i), None, None);
    let hidden = g.node(vec![("mesh", Json::from(mesh)), ("scale", Json::arr([0.0f32, 0.0, 0.0].map(Json::from)))]);
    let shown = g.node(vec![("mesh", Json::from(mesh))]);
    g.scene(&[hidden, shown]);
    let imp = import(&g);
    assert_eq!(imp.model.lods[0].triangle_count(), 2);
    assert_eq!(codes(&imp), vec!["degenerate_triangles"]);
    // 300 instances of a morph-target mesh: one warning each, capped.
    let mut g = Gltf::new();
    let (p, n, i) = quad(&mut g);
    let prim = Json::obj([("attributes", Json::obj([("POSITION", Json::from(p)), ("NORMAL", Json::from(n))])), ("indices", Json::from(i)), ("targets", Json::arr([Json::obj([("POSITION", Json::from(n))])]))]);
    let mi = g.push("meshes", Json::obj([("primitives", Json::arr([prim]))]));
    let nodes: Vec<usize> = (0..300).map(|k| g.node(vec![("mesh", Json::from(mi)), ("translation", Json::arr([k as f32, 0.0, 0.0].map(Json::from)))])).collect();
    g.scene(&nodes);
    let imp = import(&g);
    assert_eq!(imp.warnings.len(), specula_gltf::error::MAX_WARNINGS + 1);
    assert_eq!(imp.warnings.last().unwrap().code, "warnings_truncated");
    assert_eq!(imp.model.lods[0].triangle_count(), 600);
    // JSON and key budgets.
    let lim = |l: Limits| ImportOptions { limits: l, ..ImportOptions::default() };
    assert_eq!(import_bytes(&g.glb(), None, &lim(Limits { max_json_bytes: 1000, ..Limits::default() })).unwrap_err().code, C::Limit);
    let anim = cubic(&|_| [0.0; 3]);
    assert_eq!(import_bytes(&anim.glb(), None, &lim(Limits { max_total_keys: 50, ..Limits::default() })).unwrap_err().code, C::Limit);
}

/// A quad with TANGENT (1, 0, 0, +1) on every vertex under a node with `fields`.
fn tangent_quad(fields: Vec<(&str, Json)>) -> Import {
    let mut g = Gltf::new();
    let (p, n, i) = quad(&mut g);
    let t = g.floats(&[1.0, 0.0, 0.0, 1.0].repeat(4), "VEC4");
    let mesh = g.mesh(&[("POSITION", p), ("NORMAL", n), ("TANGENT", t)], Some(i), None, None);
    let mut fields = fields;
    fields.push(("mesh", Json::from(mesh)));
    let node = g.node(fields);
    g.scene(&[node]);
    import(&g)
}

#[test]
fn tangents_are_kept_and_follow_the_node_transform() {
    let s = std::f32::consts::FRAC_1_SQRT_2;
    let rotated = tangent_quad(vec![("rotation", Json::arr([0.0, s, 0.0, s].map(Json::from)))]);
    let m = &rotated.model.lods[0];
    assert_eq!(m.tangents.len(), 4, "TANGENT is kept");
    // +X rotated 90° about +Y is −Z.
    assert!(m.tangents.iter().all(|t| (t[2] + 1.0).abs() < 1e-5 && t[0].abs() < 1e-5 && t[3] == 1.0), "{:?}", m.tangents);
    let mirrored = tangent_quad(vec![("scale", Json::arr([-1.0f32, 1.0, 1.0].map(Json::from)))]);
    let m = &mirrored.model.lods[0];
    assert!(m.tangents.iter().all(|t| (t[0] + 1.0).abs() < 1e-5 && t[3] == -1.0), "a mirror flips the bitangent sign: {:?}", m.tangents);
    // Without TANGENT there are none.
    let mut g = Gltf::new();
    let (p, n, i) = quad(&mut g);
    let mesh = g.mesh(&[("POSITION", p), ("NORMAL", n)], Some(i), None, None);
    let node = g.node(vec![("mesh", Json::from(mesh))]);
    g.scene(&[node]);
    assert!(import(&g).model.lods[0].tangents.is_empty());
}
