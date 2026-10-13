//! Renders the Sepulcra HUD (top bar, selected stone, radial build menu open, two threat arrows,
//! a toast) and the pause menu with the software preview, at 1280×800 (Steam Deck, gamepad
//! prompts) and 1920×1080 (mouse prompts), over a stand-in night scene, and runs `ui_check` on
//! every Sepulcra screen.
//!
//! ```text
//! cargo run -p specula-ui --example hud_preview --release [-- OUT_DIR]
//! ```
//!
//! Writes PNGs and `ui_check.json` to `target/ui-preview/` (or OUT_DIR). Uses a system font
//! (Segoe UI on Windows); skips cleanly if there is none.

use specula_core::png::Image;
use specula_input::{ActionMap, Input};
use specula_platform::{DeadZones, Event, GamepadState, RawGamepad};
use specula_text::{find_system_ui_font, load_font_file};
use specula_ui::check::{check, report_json, Severity, Target};
use specula_ui::color::Rgba;
use specula_ui::draw::{fill_outline, DrawList, Fill, Rect, RectCmd};
use specula_ui::preview::{render, render_onto, to_png};
use specula_ui::{add_ui_actions, FontSet, StateMap, Theme, ToastKind, Ui, Viewport};
use std::path::{Path, PathBuf};
use std::time::Instant;

const SCREENS: [&str; 9] = ["hud", "radial", "pause", "controls", "settings", "victory", "defeat", "title", "confirm"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn input_map() -> ActionMap {
    // Merged as sepulcra-app does: the game's actions plus the app's.
    use specula_core::json::Json;
    let read = |p: PathBuf| Json::parse(&std::fs::read_to_string(p).expect("input file")).expect("input json");
    let mut game = read(root().join("games/sepulcra/data/input.json"));
    let app = read(root().join("games/sepulcra-app/data/input_app.json"));
    let mut actions: Vec<Json> = game.get("actions").and_then(Json::as_array).unwrap_or(&[]).to_vec();
    actions.extend(app.get("actions").and_then(Json::as_array).unwrap_or(&[]).iter().cloned());
    game.set("actions", Json::Arr(actions));
    let mut map = ActionMap::from_json(&game).expect("input map");
    add_ui_actions(&mut map).expect("ui actions");
    map
}

fn state(w: f32, h: f32) -> StateMap {
    let mut s = StateMap::new();
    s.set_int("wave", 4)
        .set_int("waves", 10)
        .set_num("next_wave_s", 23.0)
        .set_int("early_bonus", 11)
        .set_int("dust", 135)
        .set_int("tombstones", 7)
        .set_int("tombstones_max", 10)
        .set_bool("paused", false)
        .set_int("speed", 2)
        .set_str("selected", "warding")
        .set_str("selected_name", "Warding stone I")
        .set_str("selected_role", "Single target · ground")
        .set_str("selected_detail", "Homing bolts: 10 damage every 0.8 s, 7 m range. Carve more runes for tier II.")
        .set_int("selected_cost", 50)
        .set_bool("affordable", true)
        .set_str("hover", "top:12,9 · Rune pillar II · slows 45 %")
        .set_int("kills", 212)
        .set_num("run_time_s", 431.0)
        .set_num("ui_scale", 1.0)
        .set_num("volume", 0.8)
        .set_str("glyph_style", "Auto")
        .set_str("reduced_motion_text", "Off")
        .set_bool("rebinding", false)
        .set_str("rebinding_label", "")
        .set_str("confirm_title", "Quit to the title screen?")
        .set_str("confirm_body", "The run will be lost. The graves will not remember you.");
    let t = s.list_mut("threats");
    t.clear();
    // East cliff: far off the right edge.
    t.push().set_num("x", f64::from(w + 600.0)).set_num("y", f64::from(h * 0.42)).set_bool("behind", false).set_str("label", "East cliff");
    // North face: behind the camera (projected right of centre, so really to the left).
    t.push().set_num("x", f64::from(w * 0.78)).set_num("y", f64::from(h * 0.62)).set_bool("behind", true).set_str("label", "North");
    let b = s.list_mut("bindings");
    b.clear();
    for (label, keys) in [
        ("Carve / place", "LMB / A"),
        ("Cancel", "Esc / B"),
        ("Build menu", "B / Y"),
        ("Next face", "E / RB"),
        ("Previous face", "Q / LB"),
        ("Upgrade", "U / X"),
        ("Sell", "X / View"),
        ("Call wave", "Space / L3"),
        ("Pause", "P / Menu"),
        ("Speed", "F"),
    ] {
        b.push().set_str("label", label).set_str("keys", keys);
    }
    s
}

/// A stand-in for the game frame: night sky, mist, the slab with a pale graveyard.
fn backdrop(w: u32, h: u32) -> Image {
    let mut d = DrawList { size: [w, h], scale: 1.0, ..DrawList::default() };
    let (fw, fh) = (w as f32, h as f32);
    let hex = |s: &str| specula_ui::color::parse_hex(s).unwrap();
    d.rect(RectCmd { rect: Rect::new(0.0, 0.0, fw, fh), radius: 0.0, fill: [hex("#0b1020"), hex("#2a3448")], border: 0.0, border_color: [[0.0; 4]; 2], feather: 1.0 });
    let mut scratch = Vec::new();
    let mut poly = |d: &mut DrawList, pts: &[[f32; 2]], c: Rgba| {
        let s = d.begin_mesh();
        let pts: Vec<[f32; 2]> = pts.iter().map(|p| [p[0] * fw, p[1] * fh]).collect();
        fill_outline(&mut d.vertices, &mut scratch, &pts, c, 1.0, Fill::Fan);
        d.end_mesh(s);
    };
    // Moon glow.
    d.rect(RectCmd { rect: Rect::new(fw * 0.78, fh * 0.08, fh * 0.1, fh * 0.1), radius: fh * 0.05, fill: [hex("#c9d6ff"); 2], border: 0.0, border_color: [[0.0; 4]; 2], feather: 3.0 });
    // Cliffs (lit and shadow side), then the top face, then the graveyard.
    poly(&mut d, &[[0.12, 0.5], [0.5, 0.78], [0.5, 1.0], [0.16, 0.8]], hex("#3d434d"));
    poly(&mut d, &[[0.5, 0.78], [0.88, 0.5], [0.84, 0.8], [0.5, 1.0]], hex("#4a515c"));
    poly(&mut d, &[[0.5, 0.26], [0.88, 0.5], [0.5, 0.78], [0.12, 0.5]], hex("#5b6470"));
    poly(&mut d, &[[0.5, 0.44], [0.58, 0.5], [0.5, 0.56], [0.42, 0.5]], hex("#a7a39a"));
    // Fissures.
    poly(&mut d, &[[0.3, 0.37], [0.31, 0.37], [0.51, 0.52], [0.5, 0.52]], hex("#262a31"));
    poly(&mut d, &[[0.7, 0.4], [0.71, 0.4], [0.5, 0.58], [0.49, 0.58]], hex("#262a31"));
    // A few rune glows.
    for (x, y) in [(0.36, 0.47), (0.62, 0.45), (0.55, 0.62)] {
        d.rect(RectCmd { rect: Rect::new(fw * x, fh * y, fh * 0.012, fh * 0.03), radius: 2.0, fill: [hex("#3fe0d0"); 2], border: 0.0, border_color: [[0.0; 4]; 2], feather: 2.0 });
    }
    let atlas = specula_text::GlyphAtlas::new(specula_text::AtlasConfig::default());
    render(&d, &atlas, &[], [0.0, 0.0, 0.0, 1.0])
}

fn main() {
    let out_dir = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| root().join("target/ui-preview"));
    let Some((ui_path, ui_font)) = find_system_ui_font() else {
        println!("SKIP: no system UI font found (specula_text::find_system_ui_font)");
        return;
    };
    std::fs::create_dir_all(&out_dir).expect("create output dir");
    let dir = ui_path.parent().unwrap().to_path_buf();
    let mut fonts = FontSet::new().with("ui", ui_font);
    for (slot, files) in [("ui_bold", ["segoeuib.ttf", "arialbd.ttf"]), ("title", ["georgiab.ttf", "constanb.ttf"])] {
        if let Some(f) = files.iter().find_map(|f| load_font_file(dir.join(f)).ok()) {
            fonts.add(slot, f);
        }
    }
    println!("fonts: {}", fonts.names().collect::<Vec<_>>().join(", "));
    let ui_dir = root().join("games/sepulcra/assets/ui");
    let theme = Theme::from_json_str(&std::fs::read_to_string(ui_dir.join("theme.json")).expect("theme")).unwrap_or_else(|e| panic!("theme: {e}"));
    let mut ui = Ui::new(theme, fonts);
    for name in SCREENS {
        let text = std::fs::read_to_string(ui_dir.join(format!("{name}.json"))).expect("screen file");
        ui.load_screen(&text).unwrap_or_else(|e| panic!("{name}.json: {e}"));
    }
    let map = input_map();

    // ---- previews ----
    for (size, pad) in [([1280u32, 800u32], true), ([1920, 1080], false)] {
        let (w, h) = (size[0] as f32, size[1] as f32);
        let st = state(w, h);
        let scale = ui.theme().scale.scale(size, None, 1.0);
        let view = Viewport::new(size, scale);
        let base = backdrop(size[0], size[1]);
        for scene in ["hud", "pause"] {
            for n in SCREENS {
                ui.hide(n).unwrap();
            }
            ui.show("hud").unwrap();
            let mut input = Input::new(map.clone());
            let centre = [w * 0.5, h * 0.5];
            let mut events = Vec::new();
            let mut pads = [GamepadState::default(); 4];
            ui.clear_toasts();
            if scene == "hud" {
                ui.show("radial").unwrap();
                ui.toast("Wave 4: wraiths circle the north face", ToastKind::Threat);
                ui.toast("+11 dust for calling the wave early", ToastKind::Good);
            } else {
                ui.show("pause").unwrap();
            }
            if pad {
                // Deck: the left stick points at the brazier (third item, lower right).
                let a = specula_ui::layout::radial_angle(2, 5);
                let raw = RawGamepad { left_x: (a.cos() * 30000.0) as i16, left_y: (-a.sin() * 30000.0) as i16, ..RawGamepad::default() };
                if scene == "hud" {
                    pads[0] = GamepadState::from_raw(raw, &DeadZones::default());
                } else {
                    pads[0] = GamepadState::from_raw(RawGamepad { buttons: specula_platform::GamepadButton::DpadDown.bit(), ..RawGamepad::default() }, &DeadZones::default());
                }
            } else if scene == "hud" {
                // Mouse over the warding wedge (top).
                let y = centre[1] - (150.0 + 62.0) * 0.5 * scale;
                events.push(Event::MouseMove { x: centre[0] as i32, y: y as i32 });
            } else {
                let r = ui.widget("pause", "settings").and_then(|w| ui.rect(w));
                // Lay out once to find the button, then hover it.
                if r.is_none() {
                    let _ = ui.frame(&input, &st, &view);
                }
                let r = ui.widget("pause", "settings").and_then(|w| ui.rect(w)).unwrap();
                events.push(Event::MouseMove { x: r.center()[0] as i32, y: r.center()[1] as i32 });
            }
            input.update(0.0, &events, &pads);
            ui.set_time(Some(0.0));
            let _ = ui.frame(&input, &st, &view);
            // Release the d-pad (so the pause focus moves exactly once) and let toasts fade in.
            for k in 1..=20 {
                input.update(f64::from(k) / 60.0, &[], &[]);
                let _ = ui.frame(&input, &st, &view);
            }
            ui.set_time(Some(0.6));
            let frame = ui.frame(&input, &st, &view);
            let stats = frame.draw.stats();
            let img = render_onto(&base, ui.draw_list(), ui.atlas(), &[]);
            let path = out_dir.join(format!("sepulcra_{scene}_{}x{}.png", size[0], size[1]));
            std::fs::write(&path, to_png(&img)).expect("write png");
            println!(
                "{}: scale {scale:.2}, {} rects, {} glyph runs / {} glyphs, {} meshes / {} triangles, {} clips",
                path.display(),
                stats.rects,
                stats.glyph_runs,
                stats.glyphs,
                stats.meshes,
                stats.triangles,
                stats.clips
            );
        }
    }

    // ---- every other screen, at the Deck and on a phone ----
    for (size, ppi, safe) in [([1280u32, 800u32], None, [0.0; 4]), ([1080, 2400], Some(420.0), [96.0, 0.0, 64.0, 0.0])] {
        let st = state(size[0] as f32, size[1] as f32);
        let scale = ui.theme().scale.scale(size, ppi, 1.0);
        let view = Viewport::new(size, scale).with_safe_area(safe);
        let base = backdrop(size[0], size[1]);
        let sets: [&[&str]; 8] = [&["hud"], &["controls"], &["settings"], &["victory"], &["defeat"], &["title"], &["confirm"], &["hud", "radial"]];
        for set in sets {
            if size[0] == 1280 && (set == ["hud"] || set == ["hud", "radial"]) {
                continue;
            }
            for n in SCREENS {
                ui.hide(n).unwrap();
            }
            ui.clear_toasts();
            for n in set {
                ui.show(n).unwrap();
            }
            let input = Input::new(map.clone());
            ui.set_time(Some(0.0));
            let _ = ui.frame(&input, &st, &view);
            let img = render_onto(&base, ui.draw_list(), ui.atlas(), &[]);
            let path = out_dir.join(format!("sepulcra_{}_{}x{}.png", set.join("+"), size[0], size[1]));
            std::fs::write(&path, to_png(&img)).expect("write png");
            println!("{}", path.display());
        }
    }

    // ---- frame build time: the HUD with radial, threats and toasts, warm ----
    {
        for n in SCREENS {
            ui.hide(n).unwrap();
        }
        ui.show("hud").unwrap();
        ui.show("radial").unwrap();
        ui.set_time(None);
        let mut input = Input::new(map.clone());
        let mut st = state(1280.0, 800.0);
        let view = Viewport::new([1280, 800], 1.0);
        for k in 0..30 {
            input.update(f64::from(k) / 60.0, &[], &[]);
            let _ = ui.frame(&input, &st, &view);
        }
        let n: i32 = 2000;
        let t0 = Instant::now();
        for k in 0..n {
            // Change a bound value every frame, as a live game does.
            st.set_int("dust", i64::from(100 + k % 50));
            input.update(1.0 + f64::from(k) / 60.0, &[], &[]);
            let _ = ui.frame(&input, &st, &view);
        }
        let per = t0.elapsed().as_secs_f64() / f64::from(n) * 1e6;
        println!("frame build (HUD + radial + 2 threats + toasts, 1280x800): {per:.1} us/frame over {n} frames");
    }

    // ---- ui_check on every screen ----
    let st = state(1280.0, 800.0);
    let sets: [&[&str]; 9] = [&["hud"], &["hud", "radial"], &["hud", "pause"], &["controls"], &["settings"], &["victory"], &["defeat"], &["title"], &["confirm"]];
    let mut all = Vec::new();
    let mut report = Vec::new();
    for set in sets {
        let findings = check(&mut ui, set, &st, Some(&map), &Target::defaults()).unwrap_or_else(|e| panic!("check: {e}"));
        let errors = findings.iter().filter(|f| f.severity == Severity::Error).count();
        println!("ui_check {:<16} {} errors, {} warnings", set.join("+"), errors, findings.len() - errors);
        for f in &findings {
            println!("    [{}] {} {} {}:{} {}", if f.severity == Severity::Error { "E" } else { "w" }, f.rule, f.target, f.screen, f.widget, f.message);
        }
        report.push(specula_core::json::Json::obj([("screens", specula_core::json::Json::Str(set.join("+"))), ("report", report_json(&findings))]));
        all.extend(findings);
    }
    std::fs::write(out_dir.join("ui_check.json"), specula_core::json::Json::Arr(report).to_pretty()).expect("write report");
    let errors = all.iter().filter(|f| f.severity == Severity::Error).count();
    println!("ui_check total: {errors} errors, {} warnings", all.len() - errors);
}
