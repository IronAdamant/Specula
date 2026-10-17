//! The Sepulcra HUD live in a real window on the GPU: specula-ui → `GpuOverlay` → the overlay
//! pass of `Gpu::present`, with mouse, keyboard, touch and gamepad input. A scripted tour (radial
//! build menu, then the pause menu) runs unless you interact; `B` / pad North opens the radial
//! menu and `P` / Start pauses.
//!
//! ```text
//! cargo run -p specula-ui --example hud_window --release -- [--adapter N] [--frames N] [--out DIR]
//! ```
//!
//! `--frames 0` runs until the window is closed (default 300 frames). At three points it renders
//! the same frame offscreen and writes `hud_window_<step>_<adapter>.png` (default
//! `target/ui-window/`). Prints the window path's overlay GPU time, and exits non-zero on any
//! validation error. Resizing and minimizing the window are handled (minimized: no present).

use specula_core::json::Json;
use specula_gpu::{AdapterPreference, FrameDesc, FramePath, Gpu, GpuOptions, Severity, Validation};
use specula_input::{ActionMap, Input};
use specula_platform::{Event, Gamepads, Window, WindowDesc};
use specula_text::{find_system_ui_font, load_font_file};
use specula_ui::gpu::GpuOverlay;
use specula_ui::{add_ui_actions, FontSet, StateMap, Theme, ToastKind, Ui, UiEvent, Viewport};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn input_map() -> ActionMap {
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

fn fill_state(s: &mut StateMap, t: f64, w: f32, h: f32, dust: i64) {
    let next = (45.0 - t).max(0.0);
    s.set_int("wave", 4).set_int("waves", 10).set_num("next_wave_s", next).set_int("early_bonus", (next * 0.5) as i64);
    s.set_int("dust", dust).set_int("tombstones", 7).set_int("tombstones_max", 10).set_int("speed", 1).set_bool("paused", false);
    s.set_str("selected", "warding").set_str("selected_name", "Warding stone I").set_str("selected_role", "Single target · ground");
    s.set_str("selected_detail", "Homing bolts: 10 damage every 0.8 s, 7 m range.").set_int("selected_cost", 50).set_bool("affordable", dust >= 50);
    s.set_str("hover", "top:12,9 · Rune pillar II").set_int("kills", 37).set_num("run_time_s", t);
    let th = s.list_mut("threats");
    th.clear();
    // One threat sweeping around the screen edge, one behind the camera.
    let a = t as f32 * 0.6;
    let (sa, ca) = specula_core::dmath::sin_cos(a);
    th.push().set_num("x", f64::from(w * 0.5 + ca * w * 2.0)).set_num("y", f64::from(h * 0.5 + sa * h * 2.0)).set_str("label", "Wraiths");
    th.push().set_num("x", f64::from(w * 0.6)).set_num("y", f64::from(h * 0.55)).set_bool("behind", true).set_str("label", "North");
}

fn main() {
    if !cfg!(windows) {
        eprintln!("hud_window: needs a desktop window (Windows for now); skipping");
        return;
    }
    let frames: u64 = arg("--frames").and_then(|v| v.parse().ok()).unwrap_or(300);
    let adapter = match arg("--adapter").and_then(|v| v.parse().ok()) {
        Some(i) => AdapterPreference::Index(i),
        None => AdapterPreference::HighPerformance,
    };
    let out = arg("--out").map(PathBuf::from).unwrap_or_else(|| root().join("target/ui-window"));
    std::fs::create_dir_all(&out).expect("output dir");
    if !specula_gpu::is_available() {
        println!("SKIP: Vulkan is not available");
        return;
    }
    let Some((ui_path, ui_font)) = find_system_ui_font() else {
        println!("SKIP: no system UI font");
        return;
    };
    let dir = ui_path.parent().unwrap().to_path_buf();
    let mut fonts = FontSet::new().with("ui", ui_font);
    for (slot, files) in [("ui_bold", ["segoeuib.ttf", "arialbd.ttf"]), ("title", ["georgiab.ttf", "constanb.ttf"])] {
        if let Some(f) = files.iter().find_map(|f| load_font_file(dir.join(f)).ok()) {
            fonts.add(slot, f);
        }
    }
    let ui_dir = root().join("games/sepulcra/assets/ui");
    let theme = Theme::from_json_str(&std::fs::read_to_string(ui_dir.join("theme.json")).unwrap()).unwrap();
    let mut ui = Ui::new(theme, fonts);
    for n in ["hud", "radial", "pause"] {
        ui.load_screen(&std::fs::read_to_string(ui_dir.join(format!("{n}.json"))).unwrap()).unwrap();
    }
    ui.show("hud").unwrap();

    let mut window = match Window::new(&WindowDesc { title: "Sepulcra HUD (specula-ui on the GPU)".into(), width: 1280, height: 800, ..Default::default() }) {
        Ok(w) => w,
        Err(e) => {
            println!("SKIP: no window ({e})");
            return;
        }
    };
    let mut gpu = Gpu::new(&GpuOptions { validation: Validation::On, adapter, ..Default::default() })
        .or_else(|_| Gpu::new(&GpuOptions { validation: Validation::Off, adapter, ..Default::default() }))
        .expect("Gpu::new");
    let adapter_index = gpu.adapter().index;
    println!("adapter {}: {}", adapter_index, gpu.adapter().name);
    let (mut w, mut h) = window.client_size();
    let raw = window.raw();
    gpu.attach_window(raw.hinstance, raw.hwnd, w, h).expect("attach window");
    let capture = gpu.create_target(w.max(1), h.max(1)).expect("capture target");
    let mut pads = Gamepads::new().ok();
    let mut input = Input::new(input_map());
    let mut overlay = GpuOverlay::new();
    let mut state = StateMap::new();
    let start = Instant::now();
    let mut world_t = 0.0f64;
    let mut dust = 135i64;
    let mut touched = false;
    let mut overlay_ms: Vec<f64> = Vec::new();
    let mut frame_no = 0u64;
    ui.toast("Wave 4: wraiths circle the north face", ToastKind::Threat);
    loop {
        frame_no += 1;
        let events = window.poll_events();
        let mut close = false;
        for e in &events {
            match *e {
                Event::CloseRequested => close = true,
                Event::Resized { width, height } => {
                    (w, h) = (width, height);
                    if w > 0 && h > 0 {
                        gpu.resize_window(w, h).expect("resize");
                    }
                }
                Event::KeyDown { .. } | Event::MouseDown { .. } | Event::PointerDown { .. } => touched = true,
                _ => {}
            }
        }
        if close || (frames != 0 && frame_no > frames) {
            break;
        }
        let pad_states = match &mut pads {
            Some(p) => {
                p.poll();
                *p.states()
            }
            None => Default::default(),
        };
        // Time is the frame index at 60 Hz, not the wall clock: the same frame looks the same on
        // every adapter and every run (F2), however fast frames are presented.
        let t = frame_no as f64 / 60.0;
        input.update(t, &events, &pad_states);
        if w == 0 || h == 0 {
            continue; // minimized
        }
        // The scripted tour, until someone interacts.
        if !touched {
            match frame_no {
                60 => ui.show("radial").unwrap(),
                150 => {
                    ui.hide("radial").unwrap();
                    ui.show("pause").unwrap();
                }
                240 => ui.hide("pause").unwrap(),
                _ => {}
            }
        }
        if !ui.is_visible("pause") && input.pressed("pause") {
            ui.show("pause").unwrap();
        } else if !ui.is_visible("radial") && !ui.is_visible("pause") && input.pressed("build_menu") {
            ui.show("radial").unwrap();
        }
        // The world (and so the threats) stands still while paused.
        if !ui.is_visible("pause") {
            world_t += 1.0 / 60.0;
        }
        fill_state(&mut state, world_t, w as f32, h as f32, dust);
        let scale = ui.theme().scale.scale([w, h], None, 1.0);
        let view = Viewport::new([w, h], scale);
        let events_out = ui.frame(&input, &state, &view).events.clone();
        for e in &events_out.events {
            match e {
                UiEvent::Action { action, .. } => {
                    println!("action: {action}");
                    match &**action {
                        "resume" => ui.hide("pause").unwrap(),
                        "close_build_menu" => ui.hide("radial").unwrap(),
                        a if a.starts_with("build:") => {
                            dust -= 50;
                            ui.hide("radial").unwrap();
                            ui.toast(&format!("Carved a {}", &a[6..]), ToastKind::Good);
                        }
                        _ => {}
                    }
                }
                UiEvent::Refused { .. } => ui.toast("Not enough dust", ToastKind::Warn),
                UiEvent::Back { .. } => {}
            }
        }
        overlay.sync_atlas(&mut gpu, ui.atlas_mut()).expect("atlas");
        let quads = overlay.build(&ui).expect("overlay");
        let frame = FrameDesc { clear_color: [0.043, 0.063, 0.125, 1.0], overlay: quads, reverse_z: true, ..Default::default() };
        gpu.present(&frame).expect("present");
        if let Some(ms) = gpu.last_frame_stats_of(FramePath::Window).and_then(|s| s.gpu).and_then(|g| g.passes.iter().find(|p| p.name == specula_gpu::PASS_OVERLAY).map(|p| p.gpu_ms)) {
            overlay_ms.push(ms);
        }
        if [100u64, 200, 280].contains(&frame_no) && (w, h) == (1280, 800) {
            gpu.render(capture, &frame).expect("capture");
            let rgba = gpu.read_rgba8(capture).expect("read");
            let path = out.join(format!("hud_window_{frame_no}_{adapter_index}.png"));
            std::fs::write(&path, specula_core::png::encode_rgba8(w, h, &rgba, specula_core::deflate::Level::Fast)).unwrap();
            println!("captured {}", path.display());
        }
    }
    overlay_ms.sort_by(f64::total_cmp);
    if let Some(m) = overlay_ms.get(overlay_ms.len() / 2) {
        println!("window path: overlay pass median {m:.3} ms over {} frames", overlay_ms.len());
    }
    overlay.destroy(&mut gpu);
    gpu.destroy_target(capture);
    let messages = gpu.close();
    let errors = messages.iter().filter(|m| m.severity == Severity::Error).count();
    for m in &messages {
        println!("[{:?}] {} {}", m.severity, m.id_name, m.message);
    }
    println!("validation errors: {errors}; {frame_no} frames in {:.1} s", start.elapsed().as_secs_f64());
    // Closed before a possible `exit` (which skips destructors); off Windows the stub has no Drop.
    #[allow(clippy::drop_non_drop)]
    drop(window);
    if errors > 0 {
        std::process::exit(1);
    }
}
