//! Every Sepulcra recipe asset (stones at every tier, enemies with their rigs and clips) goes
//! through export → import and comes back equal: identical mesh buffers (content hash), the same
//! material presets, the same bone hierarchy, and the same sampled poses and skinned vertices
//! for every clip.

use std::collections::BTreeMap;
use std::path::PathBuf;

use specula_anim::blend::{self, Base};
use specula_anim::math::Affine;
use specula_anim::{skin_mesh, AnimSet, Skeleton};
use specula_gltf::{export_glb, export_gltf, import_bytes, import_file, ExportInput, Import, ImportOptions};
use specula_model::{build, BuildParams, Model, Style};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets")
}

fn library() -> (Style, BTreeMap<String, String>) {
    let style = Style::parse(&std::fs::read_to_string(assets().join("style.json")).unwrap()).unwrap();
    let mut lib = BTreeMap::new();
    for e in std::fs::read_dir(assets().join("recipes")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "json") {
            lib.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
        }
    }
    (style, lib)
}

/// Every recipe; those with a `tier` parameter at tiers 1..=3.
fn all_models() -> Vec<(String, Model)> {
    let (style, lib) = library();
    let mut out = Vec::new();
    for (name, text) in &lib {
        let tiered = text.contains("\"tier\"");
        for tier in if tiered { vec![Some(1.0), Some(2.0), Some(3.0)] } else { vec![None] } {
            let vals: Vec<(&str, f64)> = tier.map(|t| vec![("tier", t)]).unwrap_or_default();
            let m = build(text, &BuildParams { values: &vals, library: Some(&lib), ..BuildParams::new(&style) }).unwrap_or_else(|e| panic!("{name}: {e}"));
            out.push((format!("{name}{}", tier.map_or(String::new(), |t| format!(" tier {t}"))), m));
        }
    }
    out
}

fn anim_set(name: &str, model: &Model) -> Option<AnimSet> {
    let text = std::fs::read_to_string(assets().join(format!("anims/{name}.json"))).ok()?;
    let skel = Skeleton::from_rig(model.rig.as_ref()?).unwrap();
    Some(AnimSet::load(&text, skel).unwrap())
}

fn close(a: &Affine, b: &Affine, tol: f32) -> bool {
    (0..3).all(|c| (0..3).all(|k| (a.cols[c][k] - b.cols[c][k]).abs() <= tol) && (a.t[c] - b.t[c]).abs() <= tol)
}

fn pose(set: &AnimSet, clip: usize, t: f32) -> Vec<Affine> {
    let id = set.clips().nth(clip).unwrap().0;
    let mut p = set.skeleton.rest_pose();
    blend::sample(&set.skeleton, set.clip(id), set.track_bones(id), t, Base::Rest, &mut p);
    let mut m = Vec::new();
    set.skeleton.model_space(&p, &mut m);
    m
}

/// The first difference between two meshes, for failure messages.
fn diff(a: &specula_model::MeshData, b: &specula_model::MeshData) -> String {
    fn first<T: PartialEq + std::fmt::Debug>(name: &str, x: &[T], y: &[T]) -> Option<String> {
        if x.len() != y.len() {
            return Some(format!("{name}: {} vs {} entries", x.len(), y.len()));
        }
        x.iter().zip(y).position(|(p, q)| p != q).map(|i| format!("{name}[{i}]: {:?} vs {:?}", x[i], y[i]))
    }
    first("positions", &a.positions, &b.positions)
        .or_else(|| first("normals", &a.normals, &b.normals))
        .or_else(|| first("colors", &a.colors, &b.colors))
        .or_else(|| first("indices", &a.indices, &b.indices))
        .or_else(|| first("parts", &a.parts, &b.parts))
        .or_else(|| first("joints", &a.joints, &b.joints))
        .or_else(|| first("weights", &a.weights, &b.weights))
        .or_else(|| first("uvs", &a.uvs, &b.uvs))
        .unwrap_or_else(|| "no difference".into())
}

fn check(label: &str, model: &Model, set: Option<&AnimSet>, imp: &Import) {
    assert!(imp.warnings.is_empty(), "{label}: warnings {:?}", imp.warnings);
    let (a, b) = (&model.lods[0], &imp.model.lods[0]);
    assert_eq!(a.content_hash(), b.content_hash(), "{label}: mesh differs: {}", diff(a, b));
    assert_eq!(model.materials, imp.model.materials, "{label}: material presets");
    assert!(imp.materials.iter().all(|m| m.from_specula), "{label}");
    match (&model.rig, &imp.model.rig) {
        (None, None) => {}
        (Some(r0), Some(r1)) => {
            assert_eq!(r0.bones.len(), r1.bones.len(), "{label}: bones");
            for (x, y) in r0.bones.iter().zip(&r1.bones) {
                assert_eq!((&x.name, x.parent, x.tail), (&y.name, y.parent, y.tail), "{label}: bone");
                assert!((0..3).all(|k| (x.head[k] - y.head[k]).abs() < 1e-5), "{label}: head of {}", x.name);
            }
        }
        _ => panic!("{label}: rig presence differs"),
    }
    let Some(set) = set else {
        assert!(imp.clips.is_empty());
        return;
    };
    let iset = imp.anim_set().expect("rigged").unwrap_or_else(|e| panic!("{label}: {e}"));
    assert_eq!(set.len(), iset.len(), "{label}: clips");
    let (mut worst_joint, mut worst_vertex) = (0.0f32, 0.0f32);
    for (k, ((_, c0), (_, c1))) in set.clips().zip(iset.clips()).enumerate() {
        assert_eq!((&c0.name, c0.duration, c0.looping, c0.distance), (&c1.name, c1.duration, c1.looping, c1.distance), "{label}: clip");
        assert_eq!((&c0.events, &c0.curves), (&c1.events, &c1.curves), "{label}/{}: events and curves", c0.name);
        assert_eq!(c0.tracks.len(), c1.tracks.len());
        for (t0, t1) in c0.tracks.iter().zip(&c1.tracks) {
            assert_eq!((&t0.bone, t0.channel, t0.keys.interp, &t0.keys.times), (&t1.bone, t1.channel, t1.keys.interp, &t1.keys.times), "{label}/{}", c0.name);
        }
        let steps = 48;
        for s in 0..=steps {
            let t = c0.duration * s as f32 / steps as f32;
            let (p0, p1) = (pose(set, k, t), pose(&iset, k, t));
            for (j, (x, y)) in p0.iter().zip(&p1).enumerate() {
                assert!(close(x, y, 1e-4), "{label}/{} t={t}: bone {} {x:?} vs {y:?}", c0.name, set.skeleton.names[j]);
                worst_joint = worst_joint.max((0..3).map(|q| (x.t[q] - y.t[q]).abs()).fold(0.0, f32::max));
            }
            // End to end: the skinned meshes agree.
            let (mut s0, mut s1) = (Vec::new(), Vec::new());
            set.skeleton.skin_matrices(&p0, &mut s0);
            iset.skeleton.skin_matrices(&p1, &mut s1);
            let (m0, m1) = (skin_mesh(a, &s0), skin_mesh(b, &s1));
            for (v0, v1) in m0.positions.iter().zip(&m1.positions) {
                worst_vertex = worst_vertex.max((0..3).map(|q| (v0[q] - v1[q]).abs()).fold(0.0, f32::max));
            }
        }
    }
    assert!(worst_vertex < 1e-4, "{label}: skinned vertices differ by {worst_vertex} m");
    println!("{label}: {} clips, worst joint {worst_joint:.2e} m, worst skinned vertex {worst_vertex:.2e} m", set.len());
}

#[test]
fn every_sepulcra_asset_round_trips_through_glb() {
    let models = all_models();
    assert!(models.len() >= 20, "{} models", models.len());
    let mut rigged = 0;
    for (label, model) in &models {
        let base = label.split(' ').next().unwrap();
        let set = anim_set(base, model);
        let clips: Vec<_> = set.as_ref().map(|s| s.clips().map(|(_, c)| c.clone()).collect()).unwrap_or_default();
        let input = ExportInput { model, lod: 0, skeleton: set.as_ref().map(|s| &s.skeleton), clips: &clips };
        let glb = export_glb(&input).unwrap_or_else(|e| panic!("{label}: {e}"));
        let imp = import_bytes(&glb, None, &ImportOptions { name: Some(model.name.clone()), ..ImportOptions::default() }).unwrap_or_else(|e| panic!("{label}: {e}"));
        check(label, model, set.as_ref(), &imp);
        rigged += usize::from(set.is_some());
        // Lower LODs export too.
        for lod in 1..model.lods.len() {
            let glb = export_glb(&ExportInput { lod, ..input }).unwrap();
            let imp = import_bytes(&glb, None, &ImportOptions::default()).unwrap();
            assert_eq!(imp.model.lods[0].content_hash(), model.lods[lod].content_hash(), "{label} lod {lod}");
        }
    }
    assert_eq!(rigged, 5, "rigged creatures with anim sets");
}

#[test]
fn gltf_with_external_bin_and_embedded_buffers() {
    let (style, lib) = library();
    let model = build(&lib["shambler"], &BuildParams { library: Some(&lib), ..BuildParams::new(&style) }).unwrap();
    let set = anim_set("shambler", &model).unwrap();
    let clips: Vec<_> = set.clips().map(|(_, c)| c.clone()).collect();
    let input = ExportInput { model: &model, lod: 0, skeleton: Some(&set.skeleton), clips: &clips };
    // External .bin in a folder of its own.
    let dir = std::env::temp_dir().join(format!("specula-gltf-test-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    let (json, bin) = export_gltf(&input, Some("sub/shambler%20data.bin")).unwrap();
    std::fs::write(dir.join("shambler.gltf"), &json).unwrap();
    std::fs::write(dir.join("sub/shambler data.bin"), &bin).unwrap();
    let imp = import_file(&dir.join("shambler.gltf"), &ImportOptions::default()).unwrap();
    assert_eq!(imp.model.name, "shambler");
    check("shambler.gltf", &model, Some(&set), &imp);
    // Without a base folder, external files are refused.
    let e = import_bytes(json.as_bytes(), None, &ImportOptions::default()).unwrap_err();
    assert_eq!(e.code, specula_gltf::GltfErrorCode::Uri, "{e}");
    let _ = std::fs::remove_dir_all(&dir);
    // Embedded data URI.
    let (json, bin) = export_gltf(&input, None).unwrap();
    assert!(bin.is_empty() && json.contains("data:application/octet-stream;base64,"));
    let imp = import_bytes(json.as_bytes(), None, &ImportOptions::default()).unwrap();
    check("shambler embedded", &model, Some(&set), &imp);
}

#[test]
fn import_report_is_bounded() {
    let (style, lib) = library();
    let model = build(&lib["wraith"], &BuildParams { library: Some(&lib), ..BuildParams::new(&style) }).unwrap();
    let glb = export_glb(&ExportInput::new(&model)).unwrap();
    let imp = import_bytes(&glb, None, &ImportOptions::default()).unwrap();
    let r = imp.report();
    assert_eq!(r.get("triangles").and_then(|t| t.as_u64()), Some(model.lods[0].triangle_count() as u64));
    assert_eq!(r.get("bones").and_then(|t| t.as_u64()), Some(model.rig.as_ref().unwrap().len() as u64));
    assert!(r.to_string().len() < 8192);
}
