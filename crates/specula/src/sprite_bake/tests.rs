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
    assert_eq!(parse(&args(r#"{"cancel": true}"#)), Ok(Call::Cancel(None)));
    assert_eq!(parse(&args(r#"{"cancel": true, "job": 3}"#)), Ok(Call::Cancel(Some(3))));
    assert_eq!(parse(&args(r#"{"job": 3, "wait_s": 5}"#)), Ok(Call::Job { id: 3, wait_s: 5 }));
    assert_eq!(bake_of(r#"{"recipe": "recipes/a.json", "wait_s": 0}"#).wait_s, 0);
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
        (r#"{"recipe": "recipes/a.json", "wait_s": 21}"#, "wait_s"),
        (r#"{"job": 3, "recipe": "recipes/a.json"}"#, "'job'"),
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
    let alpha: Vec<u8> = px.as_chunks::<4>().0.iter().map(|p| p[3]).collect();
    bleed(&mut px, [5, 5], 2);
    assert_eq!(px.as_chunks::<4>().0.iter().map(|p| p[3]).collect::<Vec<_>>(), alpha, "alpha unchanged");
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

/// Many short strips (a cold try: every clip of the wraith at 4 directions, 6 frames each) were
/// a 768x5632 atlas, refused for its height though its pixels fit: packed, they fit, and each
/// strip's frames are still consecutive sheet frames.
#[test]
fn many_short_strips_are_packed_when_a_row_each_is_too_tall() {
    let counts = vec![6u32; 44];
    let (columns, firsts, atlas) = layout(&counts, [128, 128]).unwrap();
    assert!(atlas[0] <= MAX_ATLAS_SIDE && atlas[1] <= MAX_ATLAS_SIDE && u64::from(atlas[0]) * u64::from(atlas[1]) <= MAX_ATLAS_PIXELS, "{atlas:?}");
    assert_eq!(firsts, (0..44).map(|i| i * 6).collect::<Vec<u32>>(), "packed one after another");
    assert!(columns * 128 == atlas[0] && (44 * 6u32).div_ceil(columns) * 128 == atlas[1]);
    // A layout that fits a row a strip keeps it.
    let (columns, firsts, _) = layout(&[6, 6, 6], [128, 128]).unwrap();
    assert_eq!((columns, firsts), (6, vec![0, 6, 12]));
}

/// One clip that reaches far (the wraith's swoop) sets the whole sheet's scale; the plan names
/// it, so the agent can bake it apart. A sheet of one clip names none.
#[test]
fn the_clip_that_sets_the_scale_is_named() {
    let wraith = built("wraith");
    let set = wraith.anims.as_deref();
    let p = plan(&wraith, set, &ask(Clips::All, 4, Rate::Frames(6), 128, 1)).unwrap();
    let (clip, times) = p.scale_set_by.clone().expect("a clip sets the scale");
    assert!(clip.starts_with("swoop") && times > 1.5, "{clip} {times}");
    assert!(plan(&wraith, set, &ask(Clips::Named(vec!["float".into()]), 4, Rate::Frames(2), 64, 1)).unwrap().scale_set_by.is_none());
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
    assert!(blob.as_chunks::<4>().0.iter().all(|p| p[..3] == [0, 0, 0]), "alpha only");
    assert!(a(16, 16) > a(20, 16) && a(20, 16) > a(24, 16) && a(24, 16) > 0, "soft from the middle out");
    assert_eq!((a(27, 16), a(16, 21), a(0, 0)), (0, 0, 0), "nothing outside the ellipse");
    let hard = shadow_blob([32, 32], [16, 16], [10.0, 4.0], true);
    assert!(hard.as_chunks::<4>().0.iter().all(|p| p[3] == 0 || p[3] == 128), "pixel art's blob: one alpha");
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
    // Version 2 lists no cells (the grid and the strips' times give them); a version 1 file,
    // which listed them, still reads the same; a list that lies about the grid is refused,
    // saying where.
    assert!(!text.contains("\"rect\""), "no cell rectangles in a regular grid");
    let cell_json = |c: &super::sheet::CellData, rect: [u32; 4]| {
        let s = &data.strips[c.strip as usize];
        Json::obj([
            ("rect", Json::arr(rect.map(Json::from))),
            ("clip", s.clip.as_deref().map_or(Json::Null, Json::from)),
            ("direction", Json::from(s.direction.as_str())),
            ("frame", Json::from(c.frame)),
            ("strip", Json::from(c.strip)),
            ("time_s", Json::from(c.time_s)),
        ])
    };
    let mut v1 = j.clone();
    v1.set("version", Json::from(1));
    v1.set("cells", Json::arr(data.cells.iter().map(|c| cell_json(c, c.rect))));
    assert_eq!(SheetData::parse(&v1.to_pretty()).unwrap_or_else(|e| panic!("{}: {}", e.at, e.message)), data, "a version 1 file reads as before");
    let mut lying = v1.clone();
    let c0 = &data.cells[0];
    lying.set("cells", Json::arr([cell_json(c0, [c0.rect[0] + 7, c0.rect[1], c0.rect[2], c0.rect[3]])]));
    let e = SheetData::parse(&lying.to_string()).unwrap_err();
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
fn a_bake_is_a_job_of_its_owner_and_its_answer_is_kept_for_them() {
    let r = Running::default();
    let code = |o: &ToolOutput| match &o.content[0] {
        specula_protocol::Content::Text(t) => Json::parse(t).ok().and_then(|j| j.get("error").and_then(Json::as_str).map(str::to_owned)).unwrap_or_default(),
        _ => String::new(),
    };
    assert_eq!(r.cancel("cy", None), Ok(0), "nothing running: nothing to cancel, nobody to name");
    let a = r.start("ann", "assets/sprites/a.png", 4).unwrap();
    let b = r.start("bob", "assets/sprites/b.png", 4).unwrap();
    a.name.set("Ann".into()).unwrap();
    b.name.set("Bob".into()).unwrap();
    // One bake writes an atlas at a time: a second is refused, naming the bake's owner.
    let busy = r.start("cy", "assets/sprites/a.png", 4).err().unwrap();
    assert_eq!(code(&busy), "job_busy");
    assert!(format!("{busy:?}").contains("Ann"), "{busy:?}");
    let a2 = r.start("ann", "assets/sprites/a2.png", 4).unwrap();
    assert!(r.start("ann", "assets/sprites/a3.png", 4).is_err(), "at most two of an owner's at once");
    // Cancel is the owner's: by job or all of theirs; another's is refused naming the owner.
    assert_eq!(code(&r.cancel("bob", Some(a.id)).unwrap_err()), "not_job_owner");
    assert!(!a.cancel.load(Ordering::Relaxed));
    assert_eq!(r.cancel("bob", None), Ok(1));
    assert!(b.cancel.load(Ordering::Relaxed) && !a.cancel.load(Ordering::Relaxed), "bob's cancel stops bob's bake only");
    r.end(b.id);
    assert_eq!(code(&r.cancel("cy", None).unwrap_err()), "not_job_owner");
    assert_eq!(r.cancel("ann", Some(a2.id)), Ok(1));
    // A bake whose call stopped waiting: its progress, then its answer, for its owner only.
    let (done, answer) = crate::jobs::channel();
    r.park(a.id, answer);
    a.progress.cells.store(3, Ordering::Relaxed);
    let asked = r.ask("ann", a.id, Duration::ZERO);
    assert!(!asked.is_error && format!("{asked:?}").contains("running") && format!("{asked:?}").contains("cells_done"), "{asked:?}");
    assert_eq!(code(&r.ask("bob", a.id, Duration::ZERO)), "not_job_owner", "the progress is the owner's");
    assert!(done.send(ToolOutput::text("{\"journal\": 7}")));
    for _ in 0..2 {
        let got = r.ask("ann", a.id, Duration::from_millis(1));
        assert!(!got.is_error && format!("{got:?}").contains("journal"), "the result is kept for its owner: {got:?}");
    }
    assert_eq!(code(&r.ask("bob", a.id, Duration::ZERO)), "not_job_owner", "the result is the owner's");
    // A finished bake no longer holds its atlas.
    assert!(r.start("cy", "assets/sprites/a.png", 4).is_ok());
    assert_eq!(code(&r.ask("ann", 999, Duration::ZERO)), "unknown_job");
    // An engine that dropped a parked bake unanswered says so.
    let (done, answer) = crate::jobs::channel();
    r.park(a2.id, answer);
    drop(done);
    assert_eq!(code(&r.ask("ann", a2.id, Duration::ZERO)), "reply_dropped");
}

/// The lead's question (2026-11-24): a level view shows no top face. The cameras the renders use,
/// fitted for a box, project the box's top face to a line at elevation 0 (no area: nothing of it
/// is drawn), and to a face of area at the three-quarter view's 30 degrees and from the top.
#[test]
fn a_level_view_shows_no_top_face_and_the_angle_is_said() {
    let shambler = built("shambler");
    let area_of_top = |elevation_deg: f32| {
        let a = Ask { elevation_deg, ..ask(Clips::Still, 4, Rate::Frames(1), 64, 1) };
        let p = plan(&shambler, None, &a).unwrap();
        let mut most = 0.0f32;
        for cam in &p.cameras {
            let m = cam.view_proj;
            // A box's top face, half a metre round the pivot, half a metre up.
            let corners = [[-0.5, 0.5, -0.5], [0.5, 0.5, -0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5]].map(|[x, y, z]| {
                let c = |i: usize| m[i] * x + m[4 + i] * y + m[8 + i] * z + m[12 + i];
                [c(0) / c(3) * 64.0, c(1) / c(3) * 64.0]
            });
            let mut twice = 0.0f32;
            for i in 0..4 {
                let (p0, p1) = (corners[i], corners[(i + 1) % 4]);
                twice += p0[0] * p1[1] - p1[0] * p0[1];
            }
            most = most.max(twice.abs() / 2.0);
        }
        most
    };
    assert!(area_of_top(0.0) < 1e-3, "a level view sees the top face: {} square pixels", area_of_top(0.0));
    assert!(area_of_top(30.0) > 10.0 && area_of_top(90.0) > area_of_top(30.0));
    // The angle is the view's, named and bounded, and is written where a game reads it.
    for (v, deg) in [("side", 0.0), ("three_quarter", 30.0), ("top", 90.0)] {
        let req = bake_of(&format!(r#"{{"recipe": "recipes/shambler.json", "view": "{v}", "directions": 1, "cell": 32}}"#));
        assert_eq!(req.ask.elevation_deg, deg);
        let p = plan(&shambler, None, &req.ask).unwrap();
        let data = sheet_of(&p, &req, &shambler, "neutral");
        let text = data.to_json().to_pretty();
        assert!(text.contains(&format!("\"view\": \"{v}\"")) && SheetData::parse(&text).unwrap().elevation_deg == deg);
        let ans = answer(&p, &data, &req, None, None, Instant::now());
        assert_eq!(ans.get("elevation_deg").and_then(Json::as_f64), Some(f64::from(deg)));
    }
    assert_eq!(bake_of(r#"{"recipe": "recipes/a.json", "view": "side", "elevation_deg": 12}"#).ask.elevation_deg, 12.0);
}

/// The outline is never on a cell's outermost pixels, even round a silhouette that reaches the
/// edge (the fit keeps two clear, the outline one).
#[test]
fn the_outline_never_touches_a_cells_outermost_pixels() {
    let cell = [16u32, 12];
    let mut px = vec![0u8; (cell[0] * cell[1] * 4) as usize];
    // A silhouette from the second pixel in to the second from the edge, and one pixel on it.
    for y in 1..cell[1] - 1 {
        for x in 1..cell[0] - 1 {
            if x == 1 || y == 1 || x == cell[0] - 2 || y == cell[1] - 2 || (x, y) == (7, 6) {
                px[((y * cell[0] + x) * 4 + 3) as usize] = 255;
            }
        }
    }
    outline(&mut px, cell, [1, 2, 3], MARGIN_PX - 1);
    let a = |x: u32, y: u32| px[((y * cell[0] + x) * 4 + 3) as usize];
    for x in 0..cell[0] {
        assert_eq!((a(x, 0), a(x, cell[1] - 1)), (0, 0), "column {x}");
    }
    for y in 0..cell[1] {
        assert_eq!((a(0, y), a(cell[0] - 1, y)), (0, 0), "row {y}");
    }
    assert_eq!(a(6, 6), 255, "the outline is drawn inside");
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

/// A bake's job as the engine runs it, up to the atlas (before its encode and commit).
fn job_of(e: &mut crate::Engine, built: &Arc<Built>, ask: &Ask, palette: Option<Vec<[u8; 3]>>, outline: bool) -> (job::BakeJob, Plan, Arc<AtomicBool>) {
    let p = plan(built, built.anims.as_deref(), ask).unwrap();
    let mut req = bake_of(r#"{"recipe": "recipes/shambler.json"}"#);
    req.ask = ask.clone();
    let cancel = Arc::new(AtomicBool::new(false));
    let (done, _answer) = crate::jobs::channel();
    let bake = job::Bake {
        req,
        plan: p.clone(),
        built: Arc::clone(built),
        bases: [(0, 0); 2],
        assets: std::path::PathBuf::new(),
        store: None,
        cancel: Arc::clone(&cancel),
        owner_name: Arc::default(),
        progress: Arc::default(),
        done,
        started: Instant::now(),
    };
    let outline = if outline { palette.as_deref().and_then(darkest) } else { None };
    let look = job::Look { rig: specula_studio::rules::neutral_rig(), light: "neutral".into(), palette, outline, notes: Vec::new() };
    let owner = crate::jobs::JobOwner { name: "sprite-test".into(), key: "sprite-test".into() };
    (job::BakeJob::new(e, bake, look, owner, "sprite-test", 1), p, cancel)
}

/// Steps a bake's job (and the workers its cells resolve on) until its atlas is whole, it ends,
/// or `cancel_after` steps have passed (then it is cancelled).
fn drive(e: &mut crate::Engine, j: &mut job::BakeJob, cancel: &AtomicBool, cancel_after: Option<usize>) -> Result<job::Baked, ToolOutput> {
    use crate::steps::{Step, StepJob};
    let (reply, _answer) = crate::jobs::channel();
    let give_up = Instant::now() + std::time::Duration::from_secs(120);
    let mut steps = 0;
    loop {
        assert!(Instant::now() < give_up, "the bake never finished");
        if let Some(b) = j.rendered() {
            return Ok(b);
        }
        if cancel_after == Some(steps) {
            cancel.store(true, Ordering::Relaxed);
        }
        steps += 1;
        match j.step(e, &reply) {
            Step::More => {}
            Step::Wait => e.workers_wait_one(std::time::Duration::from_millis(5)),
            Step::Done(o) => return Err(o),
            Step::Handed => panic!("a bake answers in its steps"),
        }
        e.pump_workers(None);
    }
}

fn bake_on(e: &mut crate::Engine, built: &Arc<Built>, ask: &Ask, palette: Option<Vec<[u8; 3]>>, cancel_after: Option<usize>) -> (Result<job::Baked, ToolOutput>, Plan) {
    let (mut j, p, cancel) = job_of(e, built, ask, palette, false);
    (drive(e, &mut j, &cancel, cancel_after), p)
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
/// Read from the renders the job landed (submitted without a wait, landed by a poll).
#[test]
fn the_alpha_is_the_silhouette_in_the_picture() {
    let _one = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(mut e) = gpu_engine() else { return };
    for (recipe, clips) in [("recipes/statue_angel.json", Clips::Still), ("recipes/shambler.json", Clips::Named(vec!["walk".into()]))] {
        let built = engine_built(&mut e, recipe);
        let a = ask(clips, 8, Rate::Frames(2), 64, 2);
        let (mut j, p, cancel) = job_of(&mut e, &built, &a, None, false);
        let tap = j.tap();
        drive(&mut e, &mut j, &cancel, None).unwrap_or_else(|o| panic!("{o:?}"));
        let landed = tap.borrow();
        let mut seen: Vec<usize> = landed.iter().map(|(at, _)| *at).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), p.cells.len(), "every cell landed");
        for (at, (colour, ids)) in landed.iter() {
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
    }
}

/// The same inputs give the same bytes; no cell has the model on its edge (the fit held on the
/// GPU); every cell has the model in it; pixel art is all palette and all-or-nothing alpha, and
/// its outline is never on a cell's outermost pixels.
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
    let art_ask = Ask { supersample: 1, ..a };
    let (mut j, p, cancel) = job_of(&mut e, &built, &art_ask, Some(palette.clone()), true);
    let art = drive(&mut e, &mut j, &cancel, None).unwrap_or_else(|o| panic!("{o:?}"));
    for px in art.rgba.as_chunks::<4>().0.iter() {
        assert!(px[3] == 0 || px[3] == 255);
        if px[3] == 255 {
            assert!(palette.contains(&[px[0], px[1], px[2]]), "{px:?}");
        }
    }
    let alpha = |x: u32, y: u32| art.rgba[((y * p.atlas[0] + x) * 4 + 3) as usize];
    let darkest = darkest(&palette).unwrap();
    let mut outlined = 0;
    for c in &p.cells {
        let [x0, y0, w, h] = c.rect;
        for x in x0..x0 + w {
            assert_eq!((alpha(x, y0), alpha(x, y0 + h - 1)), (0, 0), "an outermost pixel of cell {:?} is drawn", c.rect);
        }
        for y in y0..y0 + h {
            assert_eq!((alpha(x0, y), alpha(x0 + w - 1, y)), (0, 0), "an outermost pixel of cell {:?} is drawn", c.rect);
        }
        outlined += (y0..y0 + h).flat_map(|y| (x0..x0 + w).map(move |x| (x, y))).filter(|&(x, y)| alpha(x, y) == 255 && art.rgba[((y * p.atlas[0] + x) * 4) as usize..][..3] == darkest).count();
    }
    assert!(outlined > p.cells.len() * 10, "the outline is drawn ({outlined} pixels)");
}

#[test]
fn a_cancelled_bake_stops_and_hands_nothing_over() {
    let _one = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(mut e) = gpu_engine() else { return };
    let built = engine_built(&mut e, "recipes/statue_angel.json");
    let (r, _) = bake_on(&mut e, &built, &ask(Clips::Still, 8, Rate::Frames(1), 32, 1), None, Some(2));
    let out = r.err().expect("cancelled");
    assert!(format!("{out:?}").contains("cancelled") && format!("{out:?}").contains("nothing was written"), "{out:?}");
    // Its own target went with it, and nothing of it is left on the GPU.
    assert!(!e.gpu.offscreen_pending(), "a cancelled bake left a render in flight");
}
