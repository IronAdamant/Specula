//! Filmstrip sight: every Sepulcra creature, every clip, as frames side by side plus two motion
//! trail views (side and top: onion-skinned poses moving over the ground at the clip's natural
//! speed, with dot trails for feet, hands and head). Meshes are CPU-skinned; rendering is
//! offscreen with `specula-gpu` (reverse Z) and fails on any validation warning. Skips the
//! rendering cleanly (exit 0) without Vulkan.
//!
//! ```text
//! cargo run -p specula-anim --example filmstrip --release -- \
//!     [--only shambler,wraith] [--clips walk,sprint] [--frames 8] [--tile 128] [--out docs/fidelity/sepulcra_anim_filmstrip.png]
//! ```
//!
//! Each row's label lists the clip, its duration, its events and (for locomotion) the measured
//! foot sliding at the matched speed. Trail dots trace bone ends: red = left foot, blue = right
//! foot, yellow = hands, white = head, green = tail or flame tip. Opaque parts use the recipes'
//! vertex colours; the moon casts shadows.

use std::collections::BTreeMap;
use std::path::PathBuf;

use specula_anim::blend::{self, Base};
use specula_anim::math::{math3, Affine, V3};
use specula_anim::{measure, skin, AnimSet, ClipId, Skeleton};
use specula_core::math::{Mat4, Vec3};
use specula_gpu::{DrawItem, Environment, FrameDesc, Gpu, GpuOptions, Material, MeshId, Severity, ShadowSettings, Surface, Validation, Vertex};
use specula_model::{build, BuildParams, MaterialKind, MaterialPreset, MeshData, Model, Shape, Style};

const CREATURES: [&str; 5] = ["shambler", "skeleton_runner", "wraith", "bone_colossus", "brazier_fire"];

struct Args {
    out: PathBuf,
    only: Vec<String>,
    clips: Vec<String>,
    frames: u32,
    tile: u32,
}

fn args() -> Args {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut a = Args { out: root.join("docs/fidelity/sepulcra_anim_filmstrip.png"), only: Vec::new(), clips: Vec::new(), frames: 8, tile: 128 };
    let v: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < v.len() {
        let val = v.get(i + 1).cloned().unwrap_or_default();
        match v[i].as_str() {
            "--out" => a.out = val.into(),
            "--only" => a.only = val.split(',').map(str::to_string).collect(),
            "--clips" => a.clips = val.split(',').map(str::to_string).collect(),
            "--frames" => a.frames = val.parse().expect("--frames N"),
            "--tile" => a.tile = val.parse().expect("--tile N"),
            other => panic!("unknown argument {other}"),
        }
        i += 2;
    }
    a
}

/// A creature: its built model, skeleton and anim set.
struct Creature {
    name: String,
    model: Model,
    set: AnimSet,
}

fn load_creatures(only: &[String]) -> Vec<Creature> {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
    let style = Style::parse(&std::fs::read_to_string(assets.join("style.json")).unwrap()).unwrap();
    let mut library = BTreeMap::new();
    for e in std::fs::read_dir(assets.join("recipes")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "json") {
            library.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
        }
    }
    let mut out = Vec::new();
    for name in CREATURES {
        if !only.is_empty() && !only.iter().any(|o| o == name) {
            continue;
        }
        let p = BuildParams { library: Some(&library), ..BuildParams::new(&style) };
        let model = build(&library[name], &p).unwrap_or_else(|e| panic!("{name}: {e}"));
        let skel = Skeleton::from_rig(model.rig.as_ref().expect("rigged")).unwrap();
        let text = std::fs::read_to_string(assets.join(format!("anims/{name}.json"))).unwrap();
        let set = AnimSet::load(&text, skel).unwrap_or_else(|e| panic!("anims/{name}.json: {e}"));
        out.push(Creature { name: name.to_string(), model, set });
    }
    out
}

fn main() {
    let a = args();
    let creatures = load_creatures(&a.only);
    // Metrics first (CPU only), so they print even without Vulkan.
    let mut rows: Vec<(usize, ClipId, String)> = Vec::new();
    for (ci, c) in creatures.iter().enumerate() {
        for (id, clip) in c.set.clips().filter(|(_, k)| a.clips.is_empty() || a.clips.contains(&k.name)) {
            let events: Vec<String> = clip.events.iter().map(|e| format!("{} {:.2}", e.name, e.time)).collect();
            let feet = foot_bones(c, id);
            let mut label = format!("{} {} {:.2}S", c.name, clip.name, clip.duration);
            if clip.distance > 0.0 && clip.looping == specula_anim::LoopMode::Loop && !feet.is_empty() {
                let m = measure::foot_contact(&c.set, id, &feet, 0.001, 480);
                label.push_str(&format!(" {:.1}M/S SLIDE {:.1}CM/S CONTACT {:.0}PCT", clip.speed(), m.max_slide * 100.0, m.contact * 100.0));
            }
            if let Some(seam) = measure::loop_seam(&c.set, id) {
                label.push_str(&format!(" SEAM {:.1}MM", seam * 1000.0));
            }
            println!("{label:<70} events [{}]", events.join(", "));
            rows.push((ci, id, label));
        }
    }
    if !specula_gpu::is_available() {
        println!("filmstrip: SKIPPED rendering - Vulkan is not available");
        return;
    }
    let mut gpu = match Gpu::new(&GpuOptions { validation: Validation::Auto, ..GpuOptions::default() }) {
        Ok(g) => g,
        Err(e) => {
            println!("filmstrip: SKIPPED rendering - {e}");
            return;
        }
    };
    println!("rendering on {}", gpu.adapter().name);
    let tile = a.tile;
    let n = a.frames.max(2);
    let label_h = 14u32;
    let cols = n + 2;
    let (sw, sh) = (cols * tile + 6, rows.len() as u32 * (tile + label_h));
    let mut sheet = vec![0u8; (sw * sh * 4) as usize];
    for px in sheet.chunks_exact_mut(4) {
        px.copy_from_slice(&[14, 17, 26, 255]);
    }
    let target = gpu.create_target(tile, tile).expect("target");
    let ground = upload_static(&mut gpu, &Shape::plane([400.0, 400.0], [1, 1]).finalize(0.0, "ground"));
    let dot = upload_static(&mut gpu, &Shape::sphere(1.0, 1).finalize(60.0, "dot"));
    for (row, (ci, id, label)) in rows.iter().enumerate() {
        let c = &creatures[*ci];
        let mesh = &c.model.lods[0];
        let buckets = buckets(mesh, &c.model.materials);
        let clip = c.set.clip(*id);
        let y0 = row as u32 * (tile + label_h);
        // Poses for the frames.
        let times: Vec<f32> = (0..n).map(|k| clip.duration * k as f32 / if clip.looping == specula_anim::LoopMode::Loop { n as f32 } else { (n - 1) as f32 }).collect();
        let skinned: Vec<(Vec<V3>, Vec<V3>)> = times.iter().map(|&t| skin_at(c, *id, t, mesh)).collect();
        // One camera for the whole strip: fit every frame.
        let mut b = specula_model::Bounds::EMPTY;
        for (p, _) in &skinned {
            b = b.union(&specula_model::Bounds::of(p));
        }
        let (centre, radius) = (b.center(), 0.5 * math3::length(b.size()));
        for (k, (pos, nor)) in skinned.iter().enumerate() {
            let meshes = upload_pose(&mut gpu, mesh, &buckets, pos, nor);
            let cam = camera(centre, radius, 58.0, 12.0, 30.0);
            let draws = draw_list(&meshes, &buckets, ground, 1.0);
            render_tile(&mut gpu, target, cam, &draws, &mut sheet, sw, 3 + k as u32 * tile, y0, tile, true);
            meshes.into_iter().for_each(|m| gpu.destroy_mesh(m));
        }
        // Motion trails: onion skins moving at the clip's speed, plus joint dots.
        let v = clip.speed();
        let ghosts = 5usize;
        let mut trail_b = specula_model::Bounds::EMPTY;
        let samples = 48usize;
        let marks = trail_joints(c);
        let mut dots: Vec<(V3, [f32; 4])> = Vec::new();
        for s in 0..=samples {
            let t = clip.duration * s as f32 / samples as f32;
            let j = bone_tips(c, *id, t);
            for (bone, col) in &marks {
                let p = math3::add(j[*bone], [0.0, 0.0, v * t]);
                dots.push((p, *col));
                trail_b = trail_b.union(&specula_model::Bounds::of(&[p]));
            }
        }
        let ghost_poses: Vec<(f32, Vec<V3>, Vec<V3>)> = (0..ghosts)
            .map(|g| {
                let t = clip.duration * g as f32 / (ghosts - 1) as f32;
                let (p, nn) = skin_at(c, *id, t, mesh);
                let p: Vec<V3> = p.iter().map(|q| math3::add(*q, [0.0, 0.0, v * t])).collect();
                (t, p, nn)
            })
            .collect();
        for (_, p, _) in &ghost_poses {
            trail_b = trail_b.union(&specula_model::Bounds::of(p));
        }
        let (tc, tr) = (trail_b.center(), 0.5 * math3::length(trail_b.size()));
        for (view, (az, el)) in [(90.0f32, 4.0f32), (90.0, 89.0)].into_iter().enumerate() {
            let mut all_meshes = Vec::new();
            let mut draws = Vec::new();
            let g = DrawItem::new(ground, Affine::IDENTITY.to_cols_array(), srgb(0x30, 0x35, 0x3d));
            draws.push(DrawItem { surface: Surface { roughness: 0.9, specular: 0.05, ..Surface::default() }, ..g });
            for (gi, (_, p, nn)) in ghost_poses.iter().enumerate() {
                let meshes = upload_pose(&mut gpu, mesh, &buckets, p, nn);
                let last = gi + 1 == ghost_poses.len();
                let fade = 0.35 + 0.65 * gi as f32 / (ghosts - 1) as f32;
                draws.extend(draw_list(&meshes, &buckets, ground, fade).into_iter().skip(1).map(|mut d| {
                    if !last {
                        d.material = Material::Glass;
                        d.color[3] = 0.45;
                    }
                    d
                }));
                all_meshes.extend(meshes);
            }
            let ds = (tr * 0.012).max(0.012);
            for (p, col) in &dots {
                let m = Affine { cols: [[ds, 0.0, 0.0], [0.0, ds, 0.0], [0.0, 0.0, ds]], t: *p };
                let mut d = DrawItem::new(dot, m.to_cols_array(), *col);
                d.surface = Surface { emissive: 1.5, ..Surface::default() };
                draws.push(d);
            }
            let cam = camera(tc, tr, az, el, 20.0);
            render_tile(&mut gpu, target, cam, &draws, &mut sheet, sw, 3 + (n + view as u32) * tile, y0, tile, false);
            all_meshes.into_iter().for_each(|m| gpu.destroy_mesh(m));
        }
        draw_text(&mut sheet, sw, 5, y0 + tile + 3, label, [230, 226, 214]);
        // Separator before the trail views.
        for y in y0..y0 + tile {
            let o = ((y * sw + 3 + n * tile - 1) * 4) as usize;
            sheet[o..o + 3].copy_from_slice(&[90, 96, 110]);
        }
    }
    let validation = gpu.validation_enabled();
    let msgs = gpu.close();
    for m in &msgs {
        eprintln!("[{:?}] {} {}", m.severity, m.id_name, m.message);
    }
    let png = specula_core::png::encode_rgba8(sw, sh, &sheet, specula_core::deflate::Level::Best);
    if let Some(dir) = a.out.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(&a.out, &png).expect("write png");
    println!("wrote {} ({} x {}, {} KB), validation {}", a.out.display(), sw, sh, png.len() / 1024, if validation { "on" } else { "unavailable" });
    let bad = msgs.iter().filter(|m| m.severity >= Severity::Warning).count();
    if bad > 0 {
        eprintln!("filmstrip: FAILED - {bad} validation warnings/errors");
        std::process::exit(1);
    }
}

/// Foot bones named by the clip's footstep events.
fn foot_bones(c: &Creature, id: ClipId) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for e in &c.set.clip(id).events {
        if let Some(b) = c.set.skeleton.find(&e.data) {
            if (e.name == "footstep" || e.name == "stomp") && !out.contains(&b) {
                out.push(b);
            }
        }
    }
    out
}

/// Bones to trace in the trail views, with their dot colours.
fn trail_joints(c: &Creature) -> Vec<(usize, [f32; 4])> {
    let s = &c.set.skeleton;
    let mut out = Vec::new();
    let pick = |name: &str| s.find(name);
    for (names, col) in [
        (&["foot_l"][..], [1.0, 0.2, 0.15, 1.0]),
        (&["foot_r"][..], [0.2, 0.45, 1.0, 1.0]),
        (&["hand_l", "forearm_l"][..], [1.0, 0.85, 0.2, 1.0]),
        (&["hand_r", "forearm_r"][..], [1.0, 0.85, 0.2, 1.0]),
        (&["head", "core"][..], [1.0, 1.0, 1.0, 1.0]),
        (&["tail_1", "tongue_0"][..], [0.7, 1.0, 0.4, 1.0]),
    ] {
        if let Some(b) = names.iter().find_map(|n| pick(n)) {
            out.push((b, col));
        }
    }
    out
}

/// Model-space end point (tail) of every bone at clip time `t`: a foot's toe, a tail's tip.
fn bone_tips(c: &Creature, id: ClipId, t: f32) -> Vec<V3> {
    let s = &c.set.skeleton;
    let mut pose = s.rest_pose();
    blend::sample(s, c.set.clip(id), c.set.track_bones(id), t, Base::Rest, &mut pose);
    let mut model = Vec::new();
    s.model_space(&pose, &mut model);
    // Rest frames are model-aligned: the tail sits at (tail − head) in the bone's own frame.
    model.iter().enumerate().map(|(b, m)| m.apply_point(math3::sub(s.tails[b], s.heads[b]))).collect()
}

fn skin_at(c: &Creature, id: ClipId, t: f32, mesh: &MeshData) -> (Vec<V3>, Vec<V3>) {
    let mut pose = c.set.skeleton.rest_pose();
    blend::sample(&c.set.skeleton, c.set.clip(id), c.set.track_bones(id), t, Base::Rest, &mut pose);
    let (mut model, mut sk) = (Vec::new(), Vec::new());
    c.set.skeleton.model_space(&pose, &mut model);
    c.set.skeleton.skin_matrices(&model, &mut sk);
    let (mut p, mut n) = (Vec::new(), Vec::new());
    skin::skin_into(mesh, &sk, &mut p, &mut n);
    (p, n)
}

/// One draw per material part. Opaque parts carry the recipe's real vertex colours (the GPU's
/// vertex-colour path) under a white draw colour; blended parts (wraith glass) ignore vertex
/// colour, so they get the part's average colour.
struct Bucket {
    indices: Vec<u32>,
    color: [f32; 4],
    preset: MaterialPreset,
}

fn buckets(m: &MeshData, presets: &[(String, MaterialPreset)]) -> Vec<Bucket> {
    let mut out: Vec<Bucket> = Vec::new();
    for (pi, part) in m.parts.iter().enumerate() {
        let preset = presets.iter().find(|(n, _)| n == &part.material).map(|(_, p)| p.clone()).expect("preset");
        let idx = m.part_indices(pi);
        let color = if preset.kind == MaterialKind::Glass {
            let avg = [0, 1, 2].map(|k| idx.iter().map(|&i| m.colors[i as usize][k] as f32).sum::<f32>() / (idx.len() as f32 * 255.0));
            [avg[0], avg[1], avg[2], preset.opacity]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        };
        out.push(Bucket { indices: idx.to_vec(), color, preset });
    }
    out
}

fn upload_pose(gpu: &mut Gpu, m: &MeshData, buckets: &[Bucket], pos: &[V3], nor: &[V3]) -> Vec<MeshId> {
    let v: Vec<Vertex> = (0..pos.len()).map(|i| Vertex::with_color(pos[i], nor[i], m.colors[i])).collect();
    buckets.iter().map(|b| gpu.upload_mesh(&v, &b.indices).expect("upload")).collect()
}

fn upload_static(gpu: &mut Gpu, m: &MeshData) -> MeshId {
    let v: Vec<Vertex> = m.positions.iter().zip(&m.normals).map(|(p, n)| Vertex::new(*p, *n)).collect();
    gpu.upload_mesh(&v, &m.indices).expect("upload")
}

/// Ground first, then one draw per bucket, darkened by `fade` (onion skins).
fn draw_list(meshes: &[MeshId], buckets: &[Bucket], ground: MeshId, fade: f32) -> Vec<DrawItem> {
    let mut draws = Vec::with_capacity(meshes.len() + 1);
    let g = DrawItem::new(ground, Affine::IDENTITY.to_cols_array(), srgb(0x30, 0x35, 0x3d));
    draws.push(DrawItem { surface: Surface { roughness: 0.9, specular: 0.05, ..Surface::default() }, ..g });
    for (m, b) in meshes.iter().zip(buckets) {
        let mut d = DrawItem::new(*m, Affine::IDENTITY.to_cols_array(), [b.color[0] * fade, b.color[1] * fade, b.color[2] * fade, b.color[3]]);
        d.surface = Surface { emissive: b.preset.emissive, metallic: b.preset.metallic, roughness: b.preset.roughness, specular: b.preset.specular, ..Surface::default() };
        if b.preset.kind == MaterialKind::Glass {
            d.material = Material::Glass;
        }
        draws.push(d);
    }
    draws
}

/// View-projection fitting a sphere (`centre`, `radius`) seen from azimuth/elevation (degrees;
/// azimuth 0 = from +Z, the creature's front; 90 = from +X, its left side).
fn camera(centre: V3, radius: f32, az: f32, el: f32, fov: f32) -> [f32; 16] {
    let dist = radius.max(0.05) / (fov.to_radians() * 0.5).sin() * 1.02;
    let (sa, ca) = (az.to_radians().sin(), az.to_radians().cos());
    let (se, ce) = (el.to_radians().sin(), el.to_radians().cos());
    let eye = [centre[0] + dist * ce * sa, centre[1] + dist * se, centre[2] + dist * ce * ca];
    let v3 = |a: V3| Vec3::new(a[0], a[1], a[2]);
    // Reverse Z (AGENTS.md rule 6): depth 1 at the 0.05 m near plane.
    (Mat4::perspective_vk_reverse_z(fov.to_radians(), 1.0, 0.05) * Mat4::look_at_rh(v3(eye), v3(centre), Vec3::new(0.0, 1.0, 0.0))).to_cols_array()
}

#[allow(clippy::too_many_arguments)]
fn render_tile(gpu: &mut Gpu, target: specula_gpu::TargetId, view_proj: [f32; 16], draws: &[DrawItem], sheet: &mut [u8], sw: u32, x0: u32, y0: u32, tile: u32, shadows: bool) {
    let frame = FrameDesc {
        view_proj,
        clear_color: srgb(0x0b, 0x10, 0x20),
        draws,
        reverse_z: true,
        environment: moonlight(shadows),
        lights: &[],
        time_s: 0.0,
        overlay: &[],
        draws_2d: &[],
    };
    gpu.render(target, &frame).expect("render");
    let px = gpu.read_rgba8(target).expect("read");
    for y in 0..tile {
        let d = (((y0 + y) * sw + x0) * 4) as usize;
        let o = (y * tile * 4) as usize;
        sheet[d..d + (tile * 4) as usize].copy_from_slice(&px[o..o + (tile * 4) as usize]);
    }
}

/// STYLE.md moonlight, a little brighter so motion reads in small tiles, with the moon's
/// shadow on so feet visibly meet the ground.
fn moonlight(shadows: bool) -> Environment {
    let (el, az) = (35.0f32.to_radians(), 50.0f32.to_radians());
    Environment {
        sun_direction: [el.cos() * az.sin(), el.sin(), el.cos() * az.cos()],
        sun_color: rgb3(0xc9, 0xd6, 0xff),
        sun_intensity: 1.8,
        ambient_color: rgb3(0x2a, 0x34, 0x48),
        ambient_intensity: 3.0,
        exposure: 1.2,
        tonemap: true,
        shadow: ShadowSettings { enabled: shadows, resolution: 2048, cascades: 1, distance: 120.0, softness: 1.5, bias: 1.0 },
        ..Environment::default()
    }
}

fn srgb(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

fn rgb3(r: u8, g: u8, b: u8) -> [f32; 3] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

// ---- 5x7 bitmap font for labels ----

fn glyph(c: char) -> [u8; 7] {
    match c.to_ascii_uppercase() {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 2, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 17, 25, 21, 19, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [31, 2, 4, 2, 1, 17, 14],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 30, 1, 1, 17, 14],
        '6' => [6, 8, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 2, 12],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        _ => [0; 7],
    }
}

fn draw_text(img: &mut [u8], w: u32, x: u32, y: u32, text: &str, color: [u8; 3]) {
    for (i, c) in text.chars().enumerate() {
        let g = glyph(c);
        for (row, bits) in g.iter().enumerate() {
            for col in 0..5 {
                if bits & (16 >> col) != 0 {
                    let (px, py) = (x + i as u32 * 6 + col, y + row as u32);
                    let o = ((py * w + px) * 4) as usize;
                    if o + 3 < img.len() && px < w {
                        img[o..o + 3].copy_from_slice(&color);
                    }
                }
            }
        }
    }
}
