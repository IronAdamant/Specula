//! GPU skinning matches CPU skinning: each Sepulcra creature, posed mid-clip by an `Animator`,
//! renders the same pixels (within rounding) when skinned on the GPU (`write_gpu_matrices` + the
//! mesh's joints/weights) as its `skin_mesh` copy uploaded unskinned. Runs on every adapter under
//! validation; skips cleanly without Vulkan.

use std::collections::BTreeMap;
use std::path::PathBuf;

use specula_anim::math::Affine;
use specula_anim::{skin_mesh, write_gpu_matrices, AnimSet, Animator, Skeleton};
use specula_gpu::{AdapterPreference, DrawItem, Environment, FrameDesc, Gpu, GpuOptions, MeshStreams, Severity, SkinVertex, Validation, Vertex};
use specula_model::{build, BuildParams, MeshData, Style};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets")
}

fn creature(name: &str) -> (MeshData, AnimSet) {
    let style = Style::parse(&std::fs::read_to_string(assets().join("style.json")).unwrap()).unwrap();
    let mut lib = BTreeMap::new();
    for e in std::fs::read_dir(assets().join("recipes")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "json") {
            lib.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
        }
    }
    let model = build(&lib[name], &BuildParams { library: Some(&lib), ..BuildParams::new(&style) }).unwrap();
    let skel = Skeleton::from_rig(model.rig.as_ref().unwrap()).unwrap();
    let set = AnimSet::load(&std::fs::read_to_string(assets().join(format!("anims/{name}.json"))).unwrap(), skel).unwrap();
    (model.lods[0].clone(), set)
}

type M4 = [f32; 16];

fn mul(a: &M4, b: &M4) -> M4 {
    let mut o = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

fn view_proj(center: [f32; 3], size: f32) -> M4 {
    let eye = [center[0] + size * 1.2, center[1] + size * 0.5, center[2] + size * 1.6];
    let f = {
        let d = [center[0] - eye[0], center[1] - eye[1], center[2] - eye[2]];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        d.map(|v| v / l)
    };
    let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let s = {
        let s = cross(f, [0.0, 1.0, 0.0]);
        let l = dot(s, s).sqrt();
        s.map(|v| v / l)
    };
    let u = cross(s, f);
    let view = [s[0], u[0], -f[0], 0.0, s[1], u[1], -f[1], 0.0, s[2], u[2], -f[2], 0.0, -dot(s, eye), -dot(u, eye), dot(f, eye), 1.0];
    let t = 1.0 / (25f32.to_radians()).tan();
    let proj = [t, 0.0, 0.0, 0.0, 0.0, -t, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.05, 0.0];
    mul(&proj, &view)
}

fn vertices(m: &MeshData) -> Vec<Vertex> {
    m.positions.iter().zip(&m.normals).zip(&m.colors).map(|((p, n), c)| Vertex::with_color(*p, *n, *c)).collect()
}

const SIZE: u32 = 256;

#[test]
fn gpu_skinning_matches_cpu_skinning_for_every_creature() {
    if !specula_gpu::is_available() {
        eprintln!("gpu_skinning: SKIPPED — Vulkan is not available");
        return;
    }
    let creatures = [("shambler", "walk", 0.37), ("skeleton_runner", "leap", 0.3), ("wraith", "swoop", 0.5), ("bone_colossus", "death", 0.8)];
    let posed: Vec<(MeshData, Vec<[f32; 16]>, MeshData)> = creatures
        .iter()
        .map(|(name, clip, t)| {
            let (mesh, set) = creature(name);
            let mut a = Animator::new(&set, set.id(clip).unwrap());
            a.advance(&set, *t);
            a.evaluate(&set);
            let (mut model, mut skin) = (Vec::<Affine>::new(), Vec::<Affine>::new());
            a.skin(&set, &mut model, &mut skin);
            let mut flat = Vec::new();
            write_gpu_matrices(&skin, &mut flat);
            let bones: Vec<[f32; 16]> = flat.chunks_exact(16).map(|c| c.try_into().unwrap()).collect();
            let cpu = skin_mesh(&mesh, &skin);
            (mesh, bones, cpu)
        })
        .collect();
    for adapter in specula_gpu::enumerate_adapters().unwrap() {
        let mut gpu = Gpu::new(&GpuOptions { validation: Validation::On, adapter: AdapterPreference::Index(adapter.index), ..Default::default() })
            .or_else(|_| Gpu::new(&GpuOptions { validation: Validation::Off, adapter: AdapterPreference::Index(adapter.index), ..Default::default() }))
            .unwrap();
        let target = gpu.create_target(SIZE, SIZE).unwrap();
        // Skinned pipelines now (loading time), not on a worker after the first upload.
        gpu.prepare_variants(true, false).unwrap();
        for ((name, clip, _), (mesh, bones, cpu)) in creatures.iter().zip(&posed) {
            let skin: Vec<SkinVertex> = mesh.joints.iter().zip(&mesh.weights).map(|(j, w)| SkinVertex { joints: *j, weights: *w }).collect();
            let gpu_mesh = gpu.upload_mesh_with(&vertices(mesh), &mesh.indices, &MeshStreams { skin: Some(&skin), ..Default::default() }).unwrap();
            let cpu_mesh = gpu.upload_mesh(&vertices(cpu), &cpu.indices).unwrap();
            let b = cpu.bounds();
            let (c, size) = (b.center(), b.size().iter().fold(0.0f32, |m, v| m.max(*v)));
            let vp = view_proj(c, size);
            let environment = Environment { sun_direction: [-0.4, 0.7, 0.6], ..Default::default() };
            let mut render = |draw: DrawItem, bones: &[[f32; 16]]| {
                let frame = FrameDesc { view_proj: vp, clear_color: [0.05, 0.05, 0.1, 1.0], draws: &[draw], reverse_z: true, environment, bones, ..Default::default() };
                gpu.render(target, &frame).unwrap();
                gpu.read_rgba8(target).unwrap()
            };
            let mut skinned = DrawItem::new(gpu_mesh, [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0], [1.0; 4]);
            skinned.skin = Some(0);
            let a = render(skinned, bones);
            let reference = render(DrawItem { mesh: cpu_mesh, skin: None, ..skinned }, &[]);
            let covered = reference.chunks(4).filter(|p| p[2] > 40 || p[0] > 40).count();
            let off = a.chunks(4).zip(reference.chunks(4)).filter(|(p, q)| p.iter().zip(q.iter()).any(|(x, y)| x.abs_diff(*y) > 4)).count();
            eprintln!("{} {name}/{clip}: {covered} px covered, {off} differ", adapter.name);
            assert!(covered > 2000, "{name}: the creature fills the frame ({covered} px)");
            assert!(off * 200 <= covered, "{} {name}/{clip}: {off} of {covered} pixels differ", adapter.name);
            gpu.destroy_mesh(gpu_mesh);
            gpu.destroy_mesh(cpu_mesh);
        }
        let errors: Vec<_> = gpu.close().into_iter().filter(|m| m.severity == Severity::Error).collect();
        assert!(errors.is_empty(), "{errors:#?}");
    }
}
