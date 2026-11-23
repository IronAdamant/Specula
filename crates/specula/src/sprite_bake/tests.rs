use super::plan::*;
use super::sheet::SheetData;
use super::*;
use crate::models::Built;
use specula_core::math::{Mat4, Vec3, Vec4};

fn args(text: &str) -> Json {
    Json::parse(text).unwrap()
}

fn bake_of(text: &str) -> Request {
    match parse(&args(text)) {
        Ok(Call::Bake(r)) => *r,
        other => panic!("{text}: {other:?}"),
    }
}

/// A recipe of Sepulcra's, built as the model library builds it (its style, its includes, its
/// animation set when it has one): as `model_export`'s tests build them.
fn built(name: &str) -> Built {
    use specula_model::{build, BuildParams, Style};
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
    let style = Style::parse(&std::fs::read_to_string(root.join("style.json")).unwrap()).unwrap();
    let mut library = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(root.join("recipes")).unwrap() {
        let path = entry.unwrap().path();
        library.insert(path.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&path).unwrap());
    }
    let model = build(&library[name], &BuildParams { library: Some(&library), ..BuildParams::new(&style) }).unwrap();
    let anims = match (&model.rig, std::fs::read_to_string(root.join(format!("anims/{name}.json")))) {
        (Some(rig), Ok(text)) => Some(Arc::new(specula_anim::AnimSet::load(&text, specula_anim::Skeleton::from_rig(rig).unwrap()).unwrap())),
        _ => None,
    };
    Built { id: 1, model, content: 0, build_ms: 0.0, style: root.join("style.json"), anims, anim_note: None, textures: Vec::new(), materials: Default::default(), warnings: Vec::new() }
}

fn ask(clips: Clips, dirs: u32, rate: Rate, cell: u32, ss: u32) -> Ask {
    Ask { clips, directions: directions_around(dirs), elevation_deg: 30.0, rate, cell: [cell, cell], supersample: ss, in_place: false, shadow_blob: false }
}

#[test]
fn arguments_are_read_bounded_and_defaulted() {
    let r = bake_of(r#"{"recipe": "recipes/shambler.json"}"#);
    assert_eq!(r.path, "assets/sprites/shambler.png");
    assert_eq!((r.ask.clips.clone(), r.ask.directions.len(), r.ask.cell, r.ask.supersample, r.view, r.pixel_art, r.light), (Clips::Still, 8, [128, 128], 2, View::ThreeQuarter, false, Light::Neutral));
    assert_eq!(r.ask.rate, Rate::Fps(12.0));
    let r = bake_of(r#"{"recipe": "recipes/shambler.json", "view": "side", "pixel_art": true, "clips": ["walk", "walk", "idle"], "frames": 6}"#);
    assert_eq!((r.ask.directions.len(), r.ask.directions[0].name.as_str(), r.ask.elevation_deg, r.ask.cell, r.ask.supersample), (1, "e", 0.0, [64, 64], 1));
    assert_eq!((r.ask.clips.clone(), r.ask.rate), (Clips::Named(vec!["walk".into(), "idle".into()]), Rate::Frames(6)));
    let r = bake_of(r#"{"recipe": "recipes/a.json", "directions": [0, 45, 100], "cell": [32, 48], "clips": "all", "light": "project", "path": "assets/art/sprites/a.png"}"#);
    assert_eq!(r.ask.directions.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["s", "se", "100deg"]);
    assert_eq!((r.ask.cell, r.ask.clips.clone(), r.light, frames_path(&r.path)), ([32, 48], Clips::All, Light::Project, "assets/art/sprites/a.json".to_owned()));
    assert_eq!(parse(&args(r#"{"cancel": true}"#)), Ok(Call::Cancel));
    for (bad, says) in [
        (r#"{}"#, "'recipe' is required"),
        (r#"{"recipe": "recipes/a.json", "frames": 4, "fps": 8}"#, "not both"),
        (r#"{"recipe": "recipes/a.json", "frames": 65}"#, "frames"),
        (r#"{"recipe": "recipes/a.json", "fps": 61}"#, "fps"),
        (r#"{"recipe": "recipes/a.json", "cell": 513}"#, "cell"),
        (r#"{"recipe": "recipes/a.json", "cell": 7}"#, "cell"),
        (r#"{"recipe": "recipes/a.json", "cell": 256, "supersample": 4}"#, "at most 512"),
        (r#"{"recipe": "recipes/a.json", "supersample": 3}"#, "supersample"),
        (r#"{"recipe": "recipes/a.json", "directions": 3}"#, "1, 4, 8 or 16"),
        (r#"{"recipe": "recipes/a.json", "directions": [0, 0]}"#, "twice"),
        (r#"{"recipe": "recipes/a.json", "directions": [360]}"#, "degrees"),
        (r#"{"recipe": "recipes/a.json", "view": "iso"}"#, "view"),
        (r#"{"recipe": "recipes/a.json", "elevation_deg": 91}"#, "elevation_deg"),
        (r#"{"recipe": "recipes/a.json", "light": "sun"}"#, "light"),
        (r#"{"recipe": "recipes/a.json", "path": "assets/images/a.png"}"#, "folder named sprites"),
        (r#"{"recipe": "recipes/a.json", "path": "assets/sprites/a.json"}"#, ".png"),
        (r#"{"recipe": "recipes/a.json", "path": "sprites/a.png"}"#, "under assets/"),
        (r#"{"recipe": "recipes/a.json", "clips": "walk"}"#, "'clips'"),
        (r#"{"recipe": "recipes/a.json", "format": "gif"}"#, "format"),
        (r#"{"recipe": "recipes/a.json", "cancel": true}"#, "stands alone"),
    ] {
        let e = parse(&args(bad)).unwrap_err();
        assert!(e.contains(says), "{bad}: {e}");
    }
    let many = format!(r#"{{"recipe": "recipes/a.json", "clips": [{}]}}"#, (0..17).map(|i| format!("\"c{i}\"")).collect::<Vec<_>>().join(","));
    assert!(parse(&args(&many)).unwrap_err().contains("clips"));
    let long = format!(r#"{{"recipe": "recipes/{}.json"}}"#, "x".repeat(MAX_PATH_CHARS));
    assert!(parse(&args(&long)).unwrap_err().contains("at most"));
}

#[test]
fn directions_are_named_round_the_screen_and_the_front_faces_its_way() {
    let names = |n: u32| directions_around(n).into_iter().map(|d| d.name).collect::<Vec<_>>();
    assert_eq!(names(1), ["e"]);
    assert_eq!(names(4), ["s", "e", "n", "w"]);
    assert_eq!(names(8), ["s", "se", "e", "ne", "n", "nw", "w", "sw"]);
    assert_eq!(names(16).len(), 16);
    // The model's front (+Z) shows on the screen the way the direction says.
    let front = Vec3::new(0.0, 0.0, 1.0);
    for (facing, x, y) in [(0.0f32, 0.0f32, 0.0f32), (90.0, 1.0, 0.0), (270.0, -1.0, 0.0)] {
        let (_, r, u) = camera_axes(facing, 0.0);
        assert!((front.dot(r) - x).abs() < 1e-5 && (front.dot(u) - y).abs() < 1e-5, "facing {facing}: {r:?} {u:?}");
    }
    // Facing the viewer from above, the front points down the screen; +Y is up in a side view.
    let (_, _, u) = camera_axes(0.0, 90.0);
    assert!((front.dot(u) + 1.0).abs() < 1e-5);
    let (_, _, u) = camera_axes(45.0, 0.0);
    assert!((Vec3::Y.dot(u) - 1.0).abs() < 1e-5);
}

#[test]
fn frames_follow_the_clip_and_a_clip_past_the_ceiling_is_refused() {
    assert_eq!(frame_times(1.0, true, 4), [0.0, 0.25, 0.5, 0.75]);
    assert_eq!(frame_times(1.0, false, 3), [0.0, 0.5, 1.0]);
    assert_eq!(frame_times(2.0, true, 1), [0.0]);
    assert_eq!(frames_for("walk", 1.0, Rate::Fps(12.0)), Ok(12));
    assert_eq!(frames_for("blink", 0.01, Rate::Fps(12.0)), Ok(1));
    assert_eq!(frames_for("walk", 9.0, Rate::Frames(5)), Ok(5));
    let r = frames_for("dance", 6.0, Rate::Fps(12.0)).unwrap_err();
    assert_eq!(r.code, "clip_too_long");
    assert!(r.message.contains("72 frames") && r.message.contains("64"), "{}", r.message);
}

/// The fit keeps every extent inside the margin on all four sides, the pivot is a whole pixel,
/// and one scale serves both axes.
#[test]
fn the_fit_keeps_every_extent_inside_the_margin() {
    let cases = [[-0.3f32, 0.4, 0.0, 1.8], [-2.0, 0.1, -0.5, 0.5], [-0.01, 0.01, 0.0, 0.02], [-5.0, 7.0, -3.0, 9.0], [0.0, 0.0, 0.0, 0.0]];
    for ext in cases {
        for cell in [[16u32, 16u32], [64, 32], [128, 128], [33, 101]] {
            let (s, p) = fit(ext, cell, MARGIN_PX);
            let [l, r, b, t] = ext;
            let (x0, x1) = (p[0] as f32 + l / s, p[0] as f32 + r / s);
            let (y0, y1) = (p[1] as f32 - t / s, p[1] as f32 - b / s);
            let m = MARGIN_PX as f32 - 1e-3;
            assert!(x0 >= m && x1 <= cell[0] as f32 - m && y0 >= m && y1 <= cell[1] as f32 - m, "{ext:?} in {cell:?}: s {s} pivot {p:?} -> x {x0}..{x1} y {y0}..{y1}");
            // Not wastefully small: one axis fills all but the margins and the rounding's slack.
            let fill = ((r - l) / s / cell[0] as f32).max((t - b) / s / cell[1] as f32);
            assert!(ext == [0.0; 4] || fill > 0.6, "{ext:?} in {cell:?} fills {fill}");
        }
    }
}

#[test]
fn strips_start_rows_and_the_atlas_is_refused_before_work_with_its_size() {
    let (cols, firsts, size) = layout(&[8, 3, 8], [64, 64]).unwrap();
    assert_eq!((cols, firsts, size), (8, vec![0, 8, 16], [512, 192]));
    // A strip wider than the atlas wraps onto the next row, its frames still consecutive.
    let (cols, firsts, size) = layout(&[100, 2], [64, 64]).unwrap();
    assert_eq!((cols, firsts, size), (64, vec![0, 128], [4096, 192]));
    let r = layout(&[64; 40], [128, 128]).unwrap_err();
    assert_eq!(r.code, "atlas_too_large");
    assert_eq!(r.would_be, Some([4096, 40 * 2 * 128, 4096 * 40 * 2 * 128 * 4]), "{}", r.message);
    assert!(r.message.contains("4096x10240"), "{}", r.message);
}

#[test]
fn resolve_gives_alpha_from_the_id_buffer_alone_and_no_background_colour() {
    // A 2x2 cell at 2 samples a side: 4x4 samples. The background is magenta.
    let rw = 4usize;
    let mut colour = vec![0u8; rw * rw * 4];
    let mut ids = vec![0u32; rw * rw];
    for i in 0..rw * rw {
        colour[i * 4..i * 4 + 4].copy_from_slice(&[255, 0, 255, 255]);
    }
    let mut cover = |x: usize, y: usize, c: [u8; 3]| {
        ids[y * rw + x] = 1;
        colour[(y * rw + x) * 4..(y * rw + x) * 4 + 3].copy_from_slice(&c);
    };
    // Pixel (0,0): all four samples, grey. Pixel (1,0): one sample, red. Pixel (0,1): two
    // samples, one black, one white. Pixel (1,1): none.
    for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        cover(x, y, [100, 100, 100]);
    }
    cover(3, 0, [200, 10, 10]);
    cover(0, 2, [0, 0, 0]);
    cover(1, 2, [255, 255, 255]);
    let out = resolve(&colour, &ids, [2, 2], 2);
    assert_eq!(&out[0..4], &[100, 100, 100, 255]);
    assert_eq!(&out[4..8], &[200, 10, 10, 64], "a lone sample keeps its colour exactly; a quarter covered is alpha 64");
    // Black and white averaged in linear light, not with the magenta round them.
    assert_eq!(out[8 + 3], 128);
    assert!(out[8] == out[9] && out[9] == out[10] && out[8] > 180 && out[8] < 195, "{:?}", &out[8..12]);
    assert_eq!(&out[12..16], &[0, 0, 0, 0], "nothing covered: fully transparent");
    // One sample a pixel: alpha is the ID buffer's exactly.
    let out = resolve(&colour, &ids, [4, 4], 1);
    for i in 0..16 {
        assert_eq!(out[i * 4 + 3] == 255, ids[i] != 0, "pixel {i}");
        assert!(out[i * 4 + 3] == 0 || out[i * 4 + 3] == 255);
        if ids[i] == 0 {
            assert_eq!(&out[i * 4..i * 4 + 4], &[0, 0, 0, 0]);
        }
    }
    for v in [0u8, 1, 17, 128, 254, 255] {
        assert_eq!(to_srgb8(specula_studio::colour::linear8()[v as usize]), v);
    }
}

#[test]
fn the_bleed_and_the_palette_never_change_where_the_model_is() {
    let mut px = vec![0u8; 5 * 5 * 4];
    px[(2 * 5 + 2) * 4..(2 * 5 + 2) * 4 + 4].copy_from_slice(&[90, 40, 20, 200]);
    let alpha: Vec<u8> = px.chunks_exact(4).map(|p| p[3]).collect();
    bleed(&mut px, [5, 5], 2);
    assert_eq!(px.chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>(), alpha, "alpha unchanged");
    assert_eq!(&px[(2 * 5 + 3) * 4..(2 * 5 + 3) * 4 + 3], &[90, 40, 20], "the edge colour carried out");
    assert_eq!(&px[0..4], &[0, 0, 0, 0], "beyond two pixels: untouched");
    let palette = [[10u8, 10, 10], [200, 50, 50], [240, 240, 240]];
    let mut px = vec![180, 60, 60, 255, 250, 250, 250, 100, 20, 20, 20, 128, 0, 0, 0, 0];
    snap_to_palette(&mut px, &palette);
    assert_eq!(px, vec![200, 50, 50, 255, 0, 0, 0, 0, 10, 10, 10, 255, 0, 0, 0, 0]);
}

/// Every posed vertex of every frame, through each direction's own camera, lands inside its
/// cell with the margin clear: a swung limb is never cut off, and the scale is one.
#[test]
fn the_camera_holds_every_posed_vertex_of_every_frame_inside_the_margin() {
    let shambler = built("shambler");
    let set = shambler.anims.clone().unwrap();
    let p = plan(&shambler, Some(&set), &ask(Clips::Named(vec!["walk".into(), "attack".into(), "death".into()]), 8, Rate::Frames(8), 96, 2)).unwrap();
    assert_eq!((p.cells.len(), p.strips.len(), p.atlas), (3 * 8 * 8, 24, [768, 24 * 96]));
    let mesh = &shambler.model.lods[0];
    for (ci, cell) in p.cells.iter().enumerate() {
        let skin = &p.poses[cell.pose];
        let a = p.cameras[p.strips[cell.strip].direction].view_proj;
        let vp = Mat4 { cols: [[a[0], a[1], a[2], a[3]], [a[4], a[5], a[6], a[7]], [a[8], a[9], a[10], a[11]], [a[12], a[13], a[14], a[15]]] };
        for (i, &q) in mesh.positions.iter().enumerate() {
            let (j, w) = (mesh.joints[i], mesh.weights[i]);
            let mut v = [0.0f32; 3];
            for k in 0..4 {
                let r = skin[j[k] as usize].apply_point(q);
                for a in 0..3 {
                    v[a] += r[a] * f32::from(w[k]) / 255.0;
                }
            }
            let c = vp.mul_vec4(Vec4::new(v[0], v[1], v[2], 1.0));
            let (x, y) = ((c.x / c.w + 1.0) * 0.5 * 96.0, (c.y / c.w + 1.0) * 0.5 * 96.0);
            let m = MARGIN_PX as f32 - 0.01;
            assert!(x >= m && x <= 96.0 - m && y >= m && y <= 96.0 - m, "cell {ci} vertex {i} at ({x}, {y})");
            assert!(c.z / c.w > 0.0 && c.z / c.w < 1.0, "inside the depth range");
        }
        // The origin (the ground under the model) is the pivot pixel in every cell.
        let o = vp.mul_vec4(Vec4::new(0.0, 0.0, 0.0, 1.0));
        let (x, y) = ((o.x / o.w + 1.0) * 0.5 * 96.0, (o.y / o.w + 1.0) * 0.5 * 96.0);
        assert!((x - p.pivot[0] as f32).abs() < 1e-3 && (y - p.pivot[1] as f32).abs() < 1e-3, "cell {ci}: the origin at ({x}, {y}), the pivot {:?}", p.pivot);
    }
    // A clip's events land on its frames.
    assert!(p.strips.iter().all(|s| s.events.iter().all(|(_, f)| *f < s.count)));
}

#[test]
fn a_model_with_no_clips_is_a_still_and_named_clips_are_refused() {
    let statue = built("statue_angel");
    assert!(statue.anims.is_none());
    let p = plan(&statue, None, &ask(Clips::Still, 4, Rate::Fps(12.0), 64, 2)).unwrap();
    assert_eq!((p.cells.len(), p.strips.len(), p.columns, p.atlas), (4, 4, 1, [64, 256]));
    assert!(p.strips.iter().all(|s| s.clip.is_none() && s.count == 1));
    assert!(p.poses[0].is_empty(), "no rig: nothing to skin");
    let r = plan(&statue, None, &ask(Clips::Named(vec!["walk".into()]), 4, Rate::Fps(12.0), 64, 2)).unwrap_err();
    assert_eq!(r.code, "no_clips");
    // "all" on a model without clips is the still.
    assert_eq!(plan(&statue, None, &ask(Clips::All, 1, Rate::Fps(12.0), 64, 2)).unwrap().cells.len(), 1);
    // An unknown clip names the ones there are.
    let shambler = built("shambler");
    let r = plan(&shambler, shambler.anims.as_deref(), &ask(Clips::Named(vec!["fly".into()]), 1, Rate::Fps(12.0), 64, 2)).unwrap_err();
    assert!(r.code == "unknown_clip" && r.message.contains("walk"), "{}", r.message);
}

#[test]
fn too_many_cells_and_too_large_an_atlas_are_refused_before_anything_renders() {
    let shambler = built("shambler");
    let set = shambler.anims.as_deref();
    let r = plan(&shambler, set, &ask(Clips::All, 16, Rate::Frames(16), 64, 1)).unwrap_err();
    assert_eq!(r.code, "too_many_cells", "{}", r.message);
    let r = plan(&shambler, set, &ask(Clips::Named(vec!["walk".into(), "idle".into()]), 16, Rate::Frames(16), 512, 1)).unwrap_err();
    assert_eq!(r.code, "atlas_too_large");
    let [w, h, bytes] = r.would_be.unwrap();
    assert_eq!((w, h, bytes), (4096, 32 * 2 * 512, 4096 * 32 * 2 * 512 * 4));
}

/// Which feet are down: the shambler's walk has feet anim_check knows, and over a cycle each
/// foot is down in some frames and up in others. In place: every frame's root stands over its
/// rest place sideways (the skinned root head's x and z are the rest's).
#[test]
fn feet_down_follow_the_walk_and_in_place_holds_the_root_over_its_rest() {
    let shambler = built("shambler");
    let set = shambler.anims.as_deref().unwrap();
    let a = ask(Clips::Named(vec!["walk".into()]), 1, Rate::Frames(16), 64, 1);
    let p = plan(&shambler, Some(set), &a).unwrap();
    let s = &p.strips[0];
    assert!(!s.feet.is_empty(), "the shambler's walk has feet");
    assert_eq!(s.feet_down.len(), 16);
    for f in 0..s.feet.len() as u8 {
        let down = s.feet_down.iter().filter(|d| d.contains(&f)).count();
        assert!(down > 0 && down < 16, "{} is down in {down} of 16 frames", s.feet[f as usize]);
    }
    // The still has no feet to speak of.
    assert!(plan(&shambler, Some(set), &ask(Clips::Still, 1, Rate::Frames(1), 64, 1)).unwrap().strips[0].feet.is_empty());
    // In place.
    let mut rest_model = Vec::new();
    set.skeleton.model_space(&set.skeleton.rest_pose(), &mut rest_model);
    let head = rest_model[0].t;
    let placed = plan(&shambler, Some(set), &Ask { in_place: true, ..a.clone() }).unwrap();
    for skin in &placed.poses {
        let q = skin[0].apply_point(head);
        assert!((q[0] - head[0]).abs() < 1e-4 && (q[2] - head[2]).abs() < 1e-4, "{q:?} vs {head:?}");
    }
    assert_eq!(placed.root_drift_m, p.root_drift_m, "the drift is said as the clip has it either way");
}

/// Pixel art's outline: one colour on every transparent pixel beside the silhouette, never on
/// the silhouette and never within the margin of the cell's edge. The blob: alpha only, darkest
/// at its pivot, nothing outside its ellipse.
#[test]
fn the_outline_rings_the_silhouette_and_the_blob_is_alpha_only() {
    let cell = [8u32, 8];
    let mut px = vec![0u8; 8 * 8 * 4];
    for (x, y) in [(3usize, 3usize), (4, 3), (3, 4), (4, 4), (4, 6)] {
        px[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4].copy_from_slice(&[200, 100, 50, 255]);
    }
    let before = px.clone();
    outline(&mut px, cell, [1, 2, 3], 1);
    for y in 0..8usize {
        for x in 0..8usize {
            let i = (y * 8 + x) * 4;
            let was = before[i + 3] > 0;
            let near = [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                (0..8).contains(&nx) && (0..8).contains(&ny) && before[((ny * 8 + nx) * 4 + 3) as usize] > 0
            });
            let edge = x == 0 || y == 0 || x == 7 || y == 7;
            if was {
                assert_eq!(px[i..i + 4], before[i..i + 4], "the silhouette is kept at {x},{y}");
            } else if near && !edge {
                assert_eq!(px[i..i + 4], [1, 2, 3, 255], "outlined at {x},{y}");
            } else {
                assert_eq!(px[i + 3], 0, "clear at {x},{y}");
            }
        }
    }
    assert_eq!(px[(7 * 8 + 4) * 4 + 3], 0, "the pixel below (4,6) is on the cell's edge: never drawn on");
    assert_eq!(darkest(&[[200, 200, 200], [10, 12, 30], [90, 90, 90]]), Some([10, 12, 30]));
    let blob = shadow_blob([32, 32], [16, 16], [10.0, 4.0], false);
    let a = |x: usize, y: usize| blob[(y * 32 + x) * 4 + 3];
    assert!(blob.chunks_exact(4).all(|p| p[..3] == [0, 0, 0]), "alpha only");
    assert!(a(16, 16) > a(20, 16) && a(20, 16) > a(24, 16) && a(24, 16) > 0, "soft from the middle out");
    assert_eq!((a(27, 16), a(16, 21), a(0, 0)), (0, 0, 0), "nothing outside the ellipse");
    let hard = shadow_blob([32, 32], [16, 16], [10.0, 4.0], true);
    assert!(hard.chunks_exact(4).all(|p| p[3] == 0 || p[3] == 128), "pixel art's blob: one alpha");
}

/// The blob's cell and the feet go into the frame data and read back; a blob that is not where
/// the grid puts it is refused.
#[test]
fn the_blob_and_the_feet_round_trip_in_the_frame_data() {
    let shambler = built("shambler");
    let req = bake_of(r#"{"recipe": "recipes/shambler.json", "clips": ["walk"], "directions": 4, "frames": 6, "cell": 48, "shadow": "blob", "in_place": true}"#);
    let p = plan(&shambler, shambler.anims.as_deref(), &req.ask).unwrap();
    let sh = p.shadow.clone().expect("a blob");
    assert_eq!(sh.frame, p.strips.last().map(|s| s.first + s.count.div_ceil(p.columns) * p.columns).unwrap(), "the blob begins the row after the strips");
    assert!(sh.radii_px[0] >= 1.0 && sh.radii_px[0] <= 22.0 && sh.radii_px[1] < sh.radii_px[0], "{:?}", sh.radii_px);
    let data = sheet_of(&p, &req, &shambler, "neutral");
    let text = data.to_json().to_pretty();
    let back = SheetData::parse(&text).unwrap_or_else(|e| panic!("{}: {}", e.at, e.message));
    assert_eq!(back, data);
    assert!(back.in_place && back.shadow.is_some() && !back.strips[0].feet.is_empty());
    let j = Json::parse(&text).unwrap();
    let mut bad = j.clone();
    let mut shadow = bad.get("shadow").unwrap().clone();
    shadow.set("frame", Json::from(sh.frame + 1));
    bad.set("shadow", shadow);
    assert!(SheetData::parse(&bad.to_string()).unwrap_err().at.starts_with("shadow"));
    assert!(parse(&args(r#"{"recipe": "recipes/a.json", "outline": true}"#)).unwrap_err().contains("pixel_art"));
    assert!(parse(&args(r#"{"recipe": "recipes/a.json", "shadow": "cast"}"#)).unwrap_err().contains("blob"));
}

/// The frame data reads back exactly as written, and each strip's sprite object is one the 2D
/// path takes: played at any time inside a frame, it shows exactly that frame's cell, and its
/// pivot is the model's ground.
#[test]
fn frame_data_round_trips_and_the_2d_path_plays_each_strip_from_it() {
    let shambler = built("shambler");
    let req = bake_of(r#"{"recipe": "recipes/shambler.json", "clips": ["walk", "death"], "directions": 4, "frames": 6, "cell": 64}"#);
    let p = plan(&shambler, shambler.anims.as_deref(), &req.ask).unwrap();
    let data = sheet_of(&p, &req, &shambler, "neutral");
    let text = data.to_json().to_pretty();
    let back = SheetData::parse(&text).unwrap_or_else(|e| panic!("{}: {}", e.at, e.message));
    assert_eq!(back, data);
    assert_eq!(back.to_json().to_pretty(), text, "the same text again");
    assert!(text.len() < crate::project::MAX_FILE_BYTES);
    let j = Json::parse(&text).unwrap();
    for (i, strip) in data.strips.iter().enumerate() {
        let spec = crate::two_d::SpriteSpec::parse(j.get("strips").unwrap().as_array().unwrap()[i].get("sprite").unwrap()).unwrap();
        assert_eq!(spec.texture.as_deref(), Some("sprites/shambler.png"));
        for f in 0..strip.count {
            let t = (f as f64 + 0.5) / f64::from(strip.fps);
            let (uv, problem) = crate::two_d::sprite_uv(&spec, data.size, t);
            assert!(problem.is_none());
            let cell = data.cells.iter().find(|c| c.strip == i as u32 && c.frame == f).unwrap();
            let want = [cell.rect[0] as f32 / data.size[0] as f32, cell.rect[1] as f32 / data.size[1] as f32, (cell.rect[0] + cell.rect[2]) as f32 / data.size[0] as f32, (cell.rect[1] + cell.rect[3]) as f32 / data.size[1] as f32];
            for k in 0..4 {
                assert!((uv[k] - want[k]).abs() < 1e-6, "strip {i} frame {f}: {uv:?} != {want:?}");
            }
        }
        // The pivot: the quad's pivot point is the pivot pixel, a pixel metres_per_pixel wide.
        let px = [spec.pivot[0] * data.cell[0] as f32, (1.0 - spec.pivot[1]) * data.cell[1] as f32];
        assert!((px[0] - data.pivot[0] as f32).abs() < 1e-3 && (px[1] - data.pivot[1] as f32).abs() < 1e-3);
        assert!((spec.size[0] - data.cell[0] as f32 * data.metres_per_pixel).abs() < 1e-6);
        assert_eq!(spec.anim.map(|a| a.playback), Some(if strip.looping { specula_gpu::Playback::Loop } else { specula_gpu::Playback::Once }));
    }
    // The project's validator reads it; a file that lies about its grid is refused, saying where.
    assert!(sheet::validate(&text).is_ok());
    let p = crate::project_kinds::validate("assets/sprites/shambler.json", FileKind::SpriteSheet, &text, None, &[]);
    assert!(p.is_empty(), "{:?}", p.iter().map(|p| &p.message).collect::<Vec<_>>());
    let cells_at = text.find("\"cells\"").unwrap();
    let rect_at = cells_at + text[cells_at..].find("\"rect\": [").unwrap();
    let digit_at = rect_at + text[rect_at..].find(|c: char| c.is_ascii_digit()).unwrap();
    let moved = format!("{}7{}", &text[..digit_at], &text[digit_at..]);
    let e = SheetData::parse(&moved).unwrap_err();
    assert!(e.at.starts_with("cells[") && e.message.contains("grid"), "{}: {}", e.at, e.message);
    let bad = crate::project_kinds::validate("assets/sprites/shambler.json", FileKind::SpriteSheet, "{\"format\": \"x\"}", None, &[]);
    assert!(bad.iter().any(|p| p.code == "invalid_sprite_sheet"));
    // Where the files go: kinds the project mirrors.
    assert_eq!(crate::project::kind_for(crate::project::MountKind::Assets, "sprites/shambler.json"), Some(FileKind::SpriteSheet));
    assert_eq!(crate::project::kind_for(crate::project::MountKind::Assets, "sprites/shambler.png"), Some(FileKind::Png));
}

#[test]
fn the_same_plan_twice_and_its_bounds_on_the_answer() {
    let shambler = built("shambler");
    let a = ask(Clips::Named(vec!["walk".into()]), 8, Rate::Fps(12.0), 64, 2);
    let (p, q) = (plan(&shambler, shambler.anims.as_deref(), &a).unwrap(), plan(&shambler, shambler.anims.as_deref(), &a).unwrap());
    assert_eq!(p, q);
    let req = bake_of(r#"{"recipe": "recipes/shambler.json", "clips": "all", "directions": 16, "frames": 2, "cell": 32}"#);
    let p = plan(&shambler, shambler.anims.as_deref(), &req.ask).unwrap();
    let data = sheet_of(&p, &req, &shambler, "neutral");
    let ans = answer(&p, &data, &req, None, None, Instant::now());
    assert!(data.strips.len() > MAX_LISTED);
    assert_eq!(ans.get("listed").and_then(Json::as_array).map(<[Json]>::len), Some(MAX_LISTED));
    assert!(ans.to_string().len() < 8 * 1024, "{}", ans.to_string().len());
}

#[test]
fn a_bake_is_cancelled_by_its_owner_only() {
    let r = Running::default();
    assert_eq!(r.cancel("cy"), Ok(0), "nothing running: nothing to cancel, nobody to name");
    let (a, fa, na) = r.start("ann").unwrap();
    let (b, fb, nb) = r.start("bob").unwrap();
    let (a2, _, _) = r.start("ann").unwrap();
    na.set("Ann".into()).unwrap();
    nb.set("Bob".into()).unwrap();
    assert!(r.start("ann").unwrap_err().contains("at most"));
    assert_eq!(r.cancel("bob"), Ok(1));
    assert!(fb.load(Ordering::Relaxed) && !fa.load(Ordering::Relaxed), "bob's cancel stops bob's bake only");
    r.end(a);
    assert_eq!(r.cancel("ann"), Ok(1));
    // Another agent's cancel is refused and names the owners; nothing of theirs is cancelled.
    r.end(a2);
    let (_c, fc, _) = r.start("ann").unwrap();
    assert_eq!(r.cancel("cy"), Err(vec!["Bob".to_owned(), "an agent whose bake is starting".to_owned()]));
    assert!(!fc.load(Ordering::Relaxed));
    r.end(b);
}

// ---------------------------------------------------------------------------------------------
// On the GPU (each skips cleanly without one: AGENTS rule 7)

/// A model of Sepulcra's as the engine's library builds it.
fn engine_built(e: &mut crate::Engine, recipe: &str) -> Arc<Built> {
    let give_up = Instant::now() + std::time::Duration::from_secs(120);
    let spec = ModelSpec::new(recipe);
    loop {
        let r = e.library.lock().request(&spec);
        match r {
            Ok(crate::models::BuildRequest::Ready(b, _)) => return b,
            Ok(crate::models::BuildRequest::Pending(_)) => {
                assert!(Instant::now() < give_up, "{recipe} never built");
                e.pump_builds(Some(std::time::Duration::from_millis(50)));
            }
            Err(err) => panic!("{recipe}: {}", err.message),
        }
    }
}

fn bake_on(e: &mut crate::Engine, built: &Arc<Built>, ask: &Ask, palette: Option<Vec<[u8; 3]>>, cancel_after: Option<usize>) -> (Result<job::Baked, ToolOutput>, Plan) {
    use crate::steps::{Step, StepJob};
    let p = plan(built, built.anims.as_deref(), ask).unwrap();
    let out = Arc::new(Mutex::new(None));
    let cancel = Arc::new(AtomicBool::new(false));
    let bake = job::Bake { plan: p.clone(), built: Arc::clone(built), rig: specula_studio::rules::neutral_rig(), palette, outline: None, cancel: Arc::clone(&cancel), owner_name: Arc::default(), out: Arc::clone(&out) };
    let mut j = job::BakeJob::new(e, bake, "sprite-test");
    let (reply, _answer) = crate::jobs::channel();
    let give_up = Instant::now() + std::time::Duration::from_secs(120);
    let mut steps = 0;
    loop {
        assert!(Instant::now() < give_up, "the bake never finished");
        if cancel_after == Some(steps) {
            cancel.store(true, Ordering::Relaxed);
        }
        steps += 1;
        match j.step(e, &reply) {
            Step::More | Step::Wait => {}
            Step::Done(o) if o.is_error => return (Err(o), p),
            Step::Done(_) => return (Ok(out.lock().unwrap().take().expect("the atlas")), p),
            Step::Handed => panic!("a bake answers in its steps"),
        }
    }
}

fn gpu_engine() -> Option<crate::Engine> {
    let mut e = crate::workers::charge_tests::engine()?;
    e.gpu.prepare_variants(true, true).expect("pipelines");
    e.models.ready = e.gpu.pipeline_variants();
    Some(e)
}

/// Bakes on the GPU run one at a time (each holds a GPU engine).
static GPU: Mutex<()> = Mutex::new(());

/// The picture and the ID buffer agree pixel for pixel: wherever the model's id is, the render
/// is not the background, and wherever it is not, the render is the background exactly. So the
/// alpha taken from the ID buffer is the silhouette in the picture, with no halo and no offset.
#[test]
fn the_alpha_is_the_silhouette_in_the_picture() {
    let _one = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(mut e) = gpu_engine() else { return };
    for (recipe, clips) in [("recipes/statue_angel.json", Clips::Still), ("recipes/shambler.json", Clips::Named(vec!["walk".into()]))] {
        let built = engine_built(&mut e, recipe);
        let a = ask(clips, 8, Rate::Frames(2), 64, 2);
        let p = plan(&built, built.anims.as_deref(), &a).unwrap();
        let bake = job::Bake { plan: p.clone(), built: Arc::clone(&built), rig: specula_studio::rules::neutral_rig(), palette: None, outline: None, cancel: Arc::default(), owner_name: Arc::default(), out: Arc::default() };
        e.pin_for_job(1 << 60, "sprite-test", built.id);
        for at in 0..p.cells.len() {
            let give_up = Instant::now() + std::time::Duration::from_secs(60);
            let (colour, ids) = loop {
                if let Some(r) = job::render_raw(&mut e, &bake, at).unwrap_or_else(|o| panic!("{o:?}")) {
                    break r;
                }
                assert!(Instant::now() < give_up, "the pipelines never came");
            };
            let bg = [colour[0], colour[1], colour[2]];
            assert_eq!(ids[0], 0, "the corner is background");
            let (mut model, mut mismatched) = (0, 0);
            for (i, id) in ids.iter().enumerate() {
                let px = [colour[i * 4], colour[i * 4 + 1], colour[i * 4 + 2]];
                if *id != 0 {
                    model += 1;
                }
                if (*id != 0) == (px == bg) {
                    mismatched += 1;
                }
            }
            assert!(model > 50, "{recipe} cell {at}: {model} model pixels");
            assert_eq!(mismatched, 0, "{recipe} cell {at}: {mismatched} pixels where the picture and the ID buffer disagree (background {bg:?})");
        }
        e.studio_done(1 << 60);
    }
}

/// The same inputs give the same bytes; no cell has the model on its edge (the fit held on the
/// GPU); every cell has the model in it; pixel art is all palette and all-or-nothing alpha.
#[test]
fn the_same_bake_twice_gives_the_same_bytes() {
    let _one = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(mut e) = gpu_engine() else { return };
    let built = engine_built(&mut e, "recipes/shambler.json");
    let a = ask(Clips::Named(vec!["walk".into()]), 8, Rate::Frames(4), 48, 2);
    let (first, p) = bake_on(&mut e, &built, &a, None, None);
    let first = first.unwrap_or_else(|o| panic!("{o:?}"));
    let (second, _) = bake_on(&mut e, &built, &a, None, None);
    let second = second.unwrap_or_else(|o| panic!("{o:?}"));
    assert_eq!(first.renders, p.cells.len());
    assert!(first.rgba == second.rgba, "the same bake twice differs");
    assert!(first.touching.is_empty(), "cells with the model on their edge: {:?}", first.touching);
    for c in &p.cells {
        let mut seen = 0;
        for y in c.rect[1]..c.rect[1] + c.rect[3] {
            for x in c.rect[0]..c.rect[0] + c.rect[2] {
                seen += usize::from(first.rgba[((y * p.atlas[0] + x) * 4 + 3) as usize] > 0);
            }
        }
        assert!(seen > 20, "cell {:?} shows {seen} pixels", c.rect);
    }
    let palette = vec![[20u8, 20, 24], [90, 80, 70], [160, 150, 130], [220, 210, 190]];
    let (art, _) = bake_on(&mut e, &built, &Ask { supersample: 1, ..a }, Some(palette.clone()), None);
    let art = art.unwrap_or_else(|o| panic!("{o:?}"));
    for px in art.rgba.chunks_exact(4) {
        assert!(px[3] == 0 || px[3] == 255);
        if px[3] == 255 {
            assert!(palette.contains(&[px[0], px[1], px[2]]), "{px:?}");
        }
    }
}

#[test]
fn a_cancelled_bake_stops_and_hands_nothing_over() {
    let _one = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(mut e) = gpu_engine() else { return };
    let built = engine_built(&mut e, "recipes/statue_angel.json");
    let (r, _) = bake_on(&mut e, &built, &ask(Clips::Still, 8, Rate::Frames(1), 32, 1), None, Some(2));
    let out = r.err().expect("cancelled");
    assert!(format!("{out:?}").contains("cancelled"), "{out:?}");
}
