//! Contact sheet of recipe assets, rendered offscreen with `specula-gpu` (the taste loop's
//! review image, docs/design/PROCEDURAL_MODELLING.md).
//!
//! ```text
//! cargo run -p specula-model --example preview --release -- \
//!     [--recipes games/sepulcra/assets/recipes] [--style games/sepulcra/assets/style.json] \
//!     [--out docs/fidelity/sepulcra_t1_contact.png] [--only name,name] [--tile 160] [--cols 2]
//! ```
//!
//! Every recipe (each tier 1–3 when it has a `tier` parameter) gets one strip:
//! silhouette, three turntable views under the style's moonlight, and a **game-distance** view
//! (45 m away, LOD chosen by distance, pixel density of a 1080p screen with a 40° lens), plus a
//! label with triangle counts per LOD. Skips cleanly (exit 0) without Vulkan.
//!
//! The GPU `Vertex` has no colour yet, so each part is drawn as one mesh per vertex-colour
//! bucket (triangles grouped by their average colour, 5 bits per channel).

use std::collections::BTreeMap;
use std::path::PathBuf;

use specula_gpu::{DrawItem, Environment, FrameDesc, Gpu, GpuOptions, Material, MeshId, PointLight, Severity, Surface, Validation, Vertex};
use specula_core::math::{Mat4, Vec3};
use specula_model::math3::{self, Affine, V3};
use specula_model::{build, BuildParams, MaterialKind, MeshData, Model, Shape, Style};

struct Args {
    recipes: PathBuf,
    style: PathBuf,
    out: PathBuf,
    only: Vec<String>,
    tile: u32,
    cols: u32,
}

fn args() -> Args {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut a = Args {
        recipes: root.join("games/sepulcra/assets/recipes"),
        style: root.join("games/sepulcra/assets/style.json"),
        out: root.join("docs/fidelity/sepulcra_t1_contact.png"),
        only: Vec::new(),
        tile: 128,
        cols: 2,
    };
    let v: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < v.len() {
        let val = v.get(i + 1).cloned().unwrap_or_default();
        match v[i].as_str() {
            "--recipes" => a.recipes = val.into(),
            "--style" => a.style = val.into(),
            "--out" => a.out = val.into(),
            "--only" => a.only = val.split(',').map(str::to_string).collect(),
            "--tile" => a.tile = val.parse().expect("--tile N"),
            "--cols" => a.cols = val.parse().expect("--cols N"),
            other => panic!("unknown argument {other}"),
        }
        i += 2;
    }
    a
}

fn main() {
    let a = args();
    let style_text = std::fs::read_to_string(&a.style).expect("style file");
    let style = Style::parse(&style_text).unwrap_or_else(|e| panic!("style: {e}"));
    // Library: every recipe by file stem (for includes), in name order.
    let mut library = BTreeMap::new();
    for entry in std::fs::read_dir(&a.recipes).expect("recipes dir") {
        let p = entry.unwrap().path();
        if p.extension().is_some_and(|e| e == "json") {
            let stem = p.file_stem().unwrap().to_string_lossy().to_string();
            library.insert(stem, std::fs::read_to_string(&p).unwrap());
        }
    }
    // Build every entry first (CPU, no GPU needed), so recipe errors show even without Vulkan.
    let mut entries: Vec<(String, Model, bool)> = Vec::new();
    let category_of =
        |text: &str| specula_core::Json::parse(text).ok().and_then(|j| j.get("category").and_then(|c| c.as_str()).map(str::to_string));
    // Sheet order: stones, walls, graveyard, enemies, boss, effects, slab; then by name.
    const ORDER: &[&str] = &["stone", "wall", "graveyard", "enemy", "boss", "vfx_placeholder", "slab"];
    let mut order: Vec<(usize, &String, &String)> = library
        .iter()
        .map(|(n, t)| (category_of(t).and_then(|c| ORDER.iter().position(|o| *o == c)).unwrap_or(ORDER.len()), n, t))
        .collect();
    order.sort();
    for (_, name, text) in order {
        if !a.only.is_empty() && !a.only.contains(name) {
            continue;
        }
        let category = category_of(text);
        if category.as_deref() == Some("component") {
            continue; // parts of other assets (runes), shown through them
        }
        let tiers: Vec<Option<f64>> = if text.contains("\"tier\"") { vec![Some(1.0), Some(2.0), Some(3.0)] } else { vec![None] };
        for t in tiers {
            let vals: Vec<(&str, f64)> = t.map(|t| vec![("tier", t)]).unwrap_or_default();
            let p = BuildParams { values: &vals, library: Some(&library), ..BuildParams::new(&style) };
            match build(text, &p) {
                Ok(m) => {
                    let label = match t {
                        Some(t) => format!("{name} {}", ["I", "II", "III"][t as usize - 1]),
                        None => name.clone(),
                    };
                    let tris: Vec<String> = m.lods.iter().map(|l| l.triangle_count().to_string()).collect();
                    println!("{label:<28} tris {:<18} verts {:<6} size {:.2} x {:.2} x {:.2} m  hash {:016x}", tris.join("/"), m.lods[0].vertex_count(), m.bounds.size()[0], m.bounds.size()[1], m.bounds.size()[2], m.hash);
                    entries.push((label, m, category.as_deref() == Some("slab")));
                }
                Err(e) => {
                    eprintln!("{name} (tier {t:?}): {e}");
                    std::process::exit(1);
                }
            }
        }
    }
    if !specula_gpu::is_available() {
        println!("preview: SKIPPED rendering - Vulkan is not available");
        return;
    }
    // Validation on when the layer is installed (Auto), so the run fails on any message.
    let mut gpu = match Gpu::new(&GpuOptions { validation: Validation::Auto, ..GpuOptions::default() }) {
        Ok(g) => g,
        Err(e) => {
            println!("preview: SKIPPED rendering - {e}");
            return;
        }
    };
    println!("rendering on {}", gpu.adapter().name);
    let tile = a.tile;
    let tiles_per_entry = 5u32;
    let label_h = 16u32;
    let cols = a.cols.max(1);
    let rows = (entries.len() as u32).div_ceil(cols);
    let (sw, sh) = (cols * tiles_per_entry * tile + (cols - 1) * 8, rows * (tile + label_h));
    let mut sheet = vec![0u8; (sw * sh * 4) as usize];
    for px in sheet.chunks_exact_mut(4) {
        px.copy_from_slice(&[14, 17, 26, 255]);
    }
    let target = gpu.create_target(tile, tile).expect("target");
    let ground = upload_shape(&mut gpu, &Shape::plane([200.0, 200.0], [1, 1]));
    let env = moonlight();
    for (k, (label, model, floating)) in entries.iter().enumerate() {
        let (col, row) = (k as u32 % cols, k as u32 / cols);
        let x0 = col * (tiles_per_entry * tile + 8);
        let y0 = row * (tile + label_h);
        let lods: Vec<Vec<Bucket>> = model.lods.iter().map(|m| upload_buckets(&mut gpu, m, &model.materials)).collect();
        let b = model.bounds;
        let centre = b.center();
        let radius = 0.5 * math3::length(b.size());
        let lights = lights_for(model);
        let fov: f32 = 35.0;
        for t in 0..tiles_per_entry {
            let (az, el, dist, lod, silhouette, fov_t) = match t {
                0 => (35.0f32, 12.0f32, radius / (fov.to_radians() * 0.5).sin() * 1.05, 0, true, fov),
                1 => (35.0, 15.0, radius / (fov.to_radians() * 0.5).sin() * 1.1, 0, false, fov),
                2 => (150.0, 22.0, radius / (fov.to_radians() * 0.5).sin() * 1.1, 0, false, fov),
                3 => (260.0, 35.0, radius / (fov.to_radians() * 0.5).sin() * 1.1, 0, false, fov),
                _ => {
                    // Game camera: 45 m away, 1080p pixel density with a 40° vertical lens.
                    let fov_t = 2.0 * ((20.0f32.to_radians()).tan() * tile as f32 / 1080.0).atan().to_degrees();
                    let d = if b.size()[1] > 5.0 { 60.0 } else { 45.0 };
                    let lod = if d > 40.0 { model.lods.len().min(2) - 1 } else { 0 };
                    (35.0, 40.0, d, lod, false, fov_t)
                }
            };
            let (sa, ca) = (az.to_radians().sin(), az.to_radians().cos());
            // Slab chunks float over mist: no ground, seen from higher up so the top shows.
            let el = if *floating { el + 20.0 } else { el };
            let (se, ce) = (el.to_radians().sin(), el.to_radians().cos());
            let eye = [centre[0] + dist * ce * sa, centre[1] + dist * se, centre[2] + dist * ce * ca];
            let v3 = |a: V3| Vec3::new(a[0], a[1], a[2]);
            // The engine renders with reversed Z (AGENTS.md rule 6): depth 1 at the 0.05 m near plane.
            let view_proj = (Mat4::perspective_vk_reverse_z(fov_t.to_radians(), 1.0, 0.05) * Mat4::look_at_rh(v3(eye), v3(centre), Vec3::new(0.0, 1.0, 0.0))).to_cols_array();
            let mut draws = Vec::new();
            if !silhouette && !floating {
                let mut g = DrawItem::new(ground, Affine::IDENTITY.to_cols_array(), srgb(0x5b, 0x64, 0x70));
                g.surface = Surface { roughness: 0.9, specular: 0.05, ..Surface::default() };
                draws.push(g);
            }
            for bk in &lods[lod] {
                let mut d = DrawItem::new(bk.mesh, Affine::IDENTITY.to_cols_array(), [bk.color[0], bk.color[1], bk.color[2], bk.preset.opacity]);
                if silhouette {
                    d.color = [0.0, 0.0, 0.0, 1.0];
                } else {
                    d.surface = Surface {
                        emissive: bk.preset.emissive,
                        metallic: bk.preset.metallic,
                        roughness: bk.preset.roughness,
                        specular: bk.preset.specular,
                        ..Surface::default()
                    };
                    if bk.preset.kind == MaterialKind::Glass {
                        d.material = Material::Glass;
                    }
                }
                draws.push(d);
            }
            let frame = FrameDesc {
                view_proj,
                clear_color: if silhouette { [1.0, 1.0, 1.0, 1.0] } else { srgb(0x0b, 0x10, 0x20) },
                draws: &draws,
                reverse_z: true,
                environment: if silhouette { Environment { sun_intensity: 0.0, ambient_intensity: 0.0, ..env } } else { env },
                lights: if silhouette { &[] } else { &lights },
                time_s: 0.0,
            };
            gpu.render(target, &frame).expect("render");
            let px = gpu.read_rgba8(target).expect("read");
            blit(&mut sheet, sw, &px, tile, x0 + t * tile, y0);
        }
        let tris: Vec<String> = model.lods.iter().map(|l| l.triangle_count().to_string()).collect();
        let text = format!("{label}  {} TRIS  {:.1}M", tris.join("/"), b.size()[1]);
        draw_text(&mut sheet, sw, x0 + 4, y0 + tile + 4, &text, [230, 226, 214]);
        for l in lods {
            for bk in l {
                gpu.destroy_mesh(bk.mesh);
            }
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
        eprintln!("preview: FAILED - {bad} validation warnings/errors");
        std::process::exit(1);
    }
}

/// STYLE.md: moon key `#c9d6ff` about 30° up, blue-violet ambient `#2a3448`, ACES, exposure 1.1.
fn moonlight() -> Environment {
    let (el, az) = (30.0f32.to_radians(), 60.0f32.to_radians());
    Environment {
        sun_direction: [el.cos() * az.sin(), el.sin(), el.cos() * az.cos()],
        sun_color: rgb3(0xc9, 0xd6, 0xff),
        sun_intensity: 1.5,
        ambient_color: rgb3(0x2a, 0x34, 0x48),
        ambient_intensity: 3.0,
        exposure: 1.1,
        tonemap: true,
    }
}

fn lights_for(model: &Model) -> Vec<PointLight> {
    let mut out = Vec::new();
    let m = &model.lods[0];
    for p in &m.parts {
        let (color, intensity, range) = match p.material.as_str() {
            "fire" => (rgb3(0xff, 0xa0, 0x50), 3.0, 6.0),
            // STYLE.md: rune point lights at tier III only.
            "rune_glow" if model.params.iter().any(|(k, v)| k == "tier" && *v >= 3.0) => (rgb3(0x3f, 0xe0, 0xd0), 0.5, 3.0),
            _ => continue,
        };
        let idx = m.part_indices(m.parts.iter().position(|q| q.material == p.material).unwrap());
        let mut c = [0.0f32; 3];
        for i in idx {
            c = math3::add(c, m.positions[*i as usize]);
        }
        let c = math3::scale(c, 1.0 / idx.len() as f32);
        // Lift the light out of the geometry a little so it lights the object, not just itself.
        out.push(PointLight { position: [c[0], c[1] + 0.3, c[2] + 0.6], color, intensity, range });
    }
    out
}

struct Bucket {
    mesh: MeshId,
    color: [f32; 4],
    preset: specula_model::MaterialPreset,
}

fn upload_shape(gpu: &mut Gpu, s: &Shape) -> MeshId {
    let m = s.finalize(0.0, "ground");
    let v: Vec<Vertex> = m.positions.iter().zip(&m.normals).map(|(p, n)| Vertex { position: *p, normal: *n }).collect();
    gpu.upload_mesh(&v, &m.indices).expect("upload")
}

/// One mesh per (part, quantised average triangle colour).
fn upload_buckets(gpu: &mut Gpu, m: &MeshData, presets: &[(String, specula_model::MaterialPreset)]) -> Vec<Bucket> {
    let style_preset = |name: &str| presets.iter().find(|(n, _)| n == name).map(|(_, p)| p.clone()).expect("preset");
    let mut out = Vec::new();
    for (pi, part) in m.parts.iter().enumerate() {
        let idx = m.part_indices(pi);
        // Each triangle is split into N² sub-triangles with barycentric colours, then grouped
        // by colour (5 bits per channel): a close stand-in for per-vertex colour interpolation.
        const N: u32 = 4;
        let mut groups: BTreeMap<[u8; 3], (Vec<Vertex>, [f32; 3])> = BTreeMap::new();
        for t in idx.chunks_exact(3) {
            let (i0, i1, i2) = (t[0] as usize, t[1] as usize, t[2] as usize);
            let at = |u: u32, v: u32| -> (Vertex, [f32; 3]) {
                let (b1, b2) = (u as f32 / N as f32, v as f32 / N as f32);
                let b0 = 1.0 - b1 - b2;
                let mix3 = |a: [f32; 3], b: [f32; 3], c: [f32; 3]| [0, 1, 2].map(|k| a[k] * b0 + b[k] * b1 + c[k] * b2);
                let col = |i: usize| [0, 1, 2].map(|k| m.colors[i][k] as f32 / 255.0);
                let p = mix3(m.positions[i0], m.positions[i1], m.positions[i2]);
                let n = math3::normalize(mix3(m.normals[i0], m.normals[i1], m.normals[i2]));
                (Vertex { position: p, normal: n }, mix3(col(i0), col(i1), col(i2)))
            };
            for u in 0..N {
                for v in 0..N - u {
                    let mut subs = vec![[at(u, v), at(u + 1, v), at(u, v + 1)]];
                    if u + v + 1 < N {
                        subs.push([at(u + 1, v), at(u + 1, v + 1), at(u, v + 1)]);
                    }
                    for s in subs {
                        let c = [0, 1, 2].map(|k| (s[0].1[k] + s[1].1[k] + s[2].1[k]) / 3.0);
                        let key = c.map(|x| ((x * 255.0) as u8) & 0xf8);
                        let g = groups.entry(key).or_insert((Vec::new(), c));
                        g.0.extend([s[0].0, s[1].0, s[2].0]);
                    }
                }
            }
        }
        for (verts, c) in groups.into_values() {
            let indices: Vec<u32> = (0..verts.len() as u32).collect();
            let mesh = gpu.upload_mesh(&verts, &indices).expect("upload");
            out.push(Bucket { mesh, color: [c[0], c[1], c[2], 1.0], preset: style_preset(&part.material) });
        }
    }
    out
}

fn srgb(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

fn rgb3(r: u8, g: u8, b: u8) -> [f32; 3] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

fn blit(dst: &mut [u8], dw: u32, src: &[u8], s: u32, x0: u32, y0: u32) {
    for y in 0..s {
        let d = (((y0 + y) * dw + x0) * 4) as usize;
        let o = (y * s * 4) as usize;
        dst[d..d + (s * 4) as usize].copy_from_slice(&src[o..o + (s * 4) as usize]);
    }
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
