//! The GPU overlay draws a UI draw list like the software preview does: the Sepulcra HUD with the
//! radial menu open, threat arrows and toasts, rendered offscreen on every Vulkan adapter and
//! compared pixel by pixel with `preview::render` of the same draw list. Zero validation errors.
//! Skips cleanly without Vulkan.
//!
//! Tolerance: the two renderers evaluate the same coverage formulas in different precisions
//! (shader `f32` with hardware interpolation, GPU blending into 8-bit sRGB targets), so edges
//! and gradients differ by a few levels. The mean absolute difference must stay below 0.5 levels
//! per channel and at most 0.5 % of pixels may differ by more than 16 levels.
//!
//! Set `VK_LAYER_ENABLES=VK_VALIDATION_FEATURE_ENABLE_GPU_ASSISTED_EXT` to run it under
//! GPU-assisted validation as well.

mod common;
use common::*;
use specula_gpu::{AdapterPreference, FrameDesc, Gpu, GpuOptions, Severity, Validation};
use specula_ui::gpu::GpuOverlay;
use specula_ui::preview::{render, to_png};
use specula_ui::{FontSet, StateMap, Theme, ToastKind, Ui, Viewport};

fn sepulcra_ui() -> Ui {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets/ui");
    let theme = Theme::from_json_str(&std::fs::read_to_string(dir.join("theme.json")).unwrap()).unwrap();
    // A real font when there is one (more realistic glyph edges); the built-in box font otherwise.
    let fonts = match specula_text::find_system_ui_font() {
        Some((_, f)) => FontSet::new().with("ui", f),
        None => FontSet::new().with("ui", specula_ui::testfont::box_font_parsed()),
    };
    let mut ui = Ui::new(theme, fonts);
    for n in ["hud", "radial"] {
        ui.load_screen(&std::fs::read_to_string(dir.join(format!("{n}.json"))).unwrap()).unwrap();
        ui.show(n).unwrap();
    }
    ui
}

fn state() -> StateMap {
    let mut s = StateMap::new();
    s.set_int("wave", 4).set_int("waves", 10).set_num("next_wave_s", 23.0).set_int("early_bonus", 11).set_int("dust", 135);
    s.set_int("tombstones", 7).set_int("tombstones_max", 10).set_int("speed", 2).set_str("selected", "warding");
    s.set_str("selected_name", "Warding stone I").set_str("selected_role", "Single target").set_int("selected_cost", 50);
    s.set_str("selected_detail", "Homing bolts: 10 damage every 0.8 s, 7 m range.").set_bool("affordable", true);
    s.set_str("hover", "Rune pillar II");
    let t = s.list_mut("threats");
    t.push().set_num("x", 1900.0).set_num("y", 330.0).set_str("label", "East cliff");
    t.push().set_num("x", 1000.0).set_num("y", 500.0).set_bool("behind", true).set_str("label", "North");
    s
}

#[test]
fn the_gpu_overlay_matches_the_software_preview() {
    if !specula_gpu::is_available() {
        eprintln!("gpu_overlay: SKIPPED — Vulkan is not available");
        return;
    }
    let adapters = match specula_gpu::enumerate_adapters() {
        Ok(a) if !a.is_empty() => a,
        other => {
            eprintln!("gpu_overlay: SKIPPED — no Vulkan adapters ({other:?})");
            return;
        }
    };
    let (w, h) = (1280u32, 800u32);
    for a in adapters {
        let test = format!("gpu_overlay[{} {}]", a.index, a.name);
        let opts = |validation| GpuOptions { validation, adapter: AdapterPreference::Index(a.index), ..Default::default() };
        let mut gpu = match Gpu::new(&opts(Validation::On)) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("{test}: validation unavailable ({e}); running without it");
                Gpu::new(&opts(Validation::Off)).expect("Gpu::new")
            }
        };
        let mut ui = sepulcra_ui();
        ui.toast("Wave 4: wraiths circle the north face", ToastKind::Threat);
        let mut input = Harness::new();
        let st = state();
        let view = Viewport::new([w, h], 1.0);
        ui.set_time(Some(0.5));
        for _ in 0..12 {
            input.frame(&mut ui, &[mv(640.0, 260.0)], &st, &view);
        }
        let software = render(ui.draw_list(), ui.atlas(), &[], [0.0, 0.0, 0.0, 1.0]);

        let mut overlay = GpuOverlay::new();
        overlay.sync_atlas(&mut gpu, ui.atlas_mut()).expect("atlas upload");
        let quads = overlay.build(&ui).expect("overlay");
        let n = quads.len();
        let target = gpu.create_target(w, h).unwrap();
        let frame = FrameDesc { clear_color: [0.0, 0.0, 0.0, 1.0], overlay: quads, reverse_z: true, ..Default::default() };
        // The overlay pass's GPU time: the median of 15 frames.
        let mut times = Vec::new();
        for _ in 0..15 {
            gpu.render(target, &frame).expect("render");
            let stats = gpu.last_frame_stats();
            assert_eq!(stats.draws.overlay_quads as usize, n);
            if let Some(ms) = stats.gpu.as_ref().and_then(|t| t.passes.iter().find(|p| p.name == specula_gpu::PASS_OVERLAY)).map(|p| p.gpu_ms) {
                times.push(ms);
            }
        }
        times.sort_by(f64::total_cmp);
        let median = times.get(times.len() / 2).copied();
        let hardware = gpu.read_rgba8(target).unwrap();
        eprintln!("{test}: {n} overlay records; overlay pass median {median:?} ms (min {:?}, max {:?})", times.first(), times.last());

        // Compare.
        let mut sum = 0u64;
        let mut big = 0usize;
        let mut worst = 0u8;
        let mut diff = vec![0u8; hardware.len()];
        for (i, (g, s)) in hardware.chunks_exact(4).zip(software.rgba.chunks_exact(4)).enumerate() {
            let d = (0..3).map(|k| g[k].abs_diff(s[k])).max().unwrap_or(0);
            sum += (0..3).map(|k| u64::from(g[k].abs_diff(s[k]))).sum::<u64>();
            worst = worst.max(d);
            if d > 16 {
                big += 1;
            }
            let v = d.saturating_mul(8);
            diff[i * 4..i * 4 + 4].copy_from_slice(&[v, v, v, 255]);
        }
        let mean = sum as f64 / (w as f64 * h as f64 * 3.0);
        let frac = big as f64 / (w as f64 * h as f64);
        let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        let save = |name: &str, rgba: &[u8]| std::fs::write(dir.join(name), specula_core::png::encode_rgba8(w, h, rgba, specula_core::deflate::Level::Fast)).unwrap();
        save(&format!("ui_gpu_{}.png", a.index), &hardware);
        std::fs::write(dir.join(format!("ui_software_{}.png", a.index)), to_png(&software)).unwrap();
        save(&format!("ui_diff_x8_{}.png", a.index), &diff);
        eprintln!("{test}: mean |diff| {mean:.3} levels, {big} px ({:.3} %) > 16, worst {worst}; images in {}", frac * 100.0, dir.display());
        assert!(mean < 0.5, "{test}: mean difference {mean:.3}");
        assert!(frac < 0.005, "{test}: {:.3} % of pixels differ by more than 16", frac * 100.0);

        // An image icon with a texture nobody registered: skipped and reported, and the GPU frame
        // still renders (it used to fail with "unknown texture id").
        let mut bad = Ui::new(
            Theme::from_json_str(&std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets/ui/theme.json")).unwrap()).unwrap(),
            FontSet::new().with("ui", specula_ui::testfont::box_font_parsed()),
        );
        let icon = r#"{"format": "specula.ui.screen.v1", "name": "pics", "root": {"type": "box", "children": [
            {"type": "icon", "id": "pic", "image": {"texture": 4242}, "anchor": "center"},
            {"type": "icon", "id": "page", "image": {"texture": PAGE}, "anchor": "top_left"}]}}"#;
        bad.load_screen(&icon.replace("PAGE", &overlay.atlas_textures()[0].0.to_string())).unwrap();
        bad.show("pics").unwrap();
        bad.set_textures(&[overlay.atlas_textures()[0].0]);
        input.frame(&mut bad, &[], &st, &view);
        let quads = overlay.build(&bad).expect("overlay");
        let frame = FrameDesc { clear_color: [0.0, 0.0, 0.0, 1.0], overlay: quads, reverse_z: true, ..Default::default() };
        gpu.render(target, &frame).expect("an unknown image texture must not fail the frame");
        let skipped: Vec<String> = overlay.skipped().iter().map(|s| s.to_json(&bad).to_pretty()).collect();
        assert_eq!(skipped.len(), 2, "{skipped:?}");
        assert!(skipped[0].contains("unknown_texture") && skipped[0].contains("\"pic\""), "{}", skipped[0]);
        assert!(skipped[1].contains("atlas_page_texture") && skipped[1].contains("\"page\""), "{}", skipped[1]);

        overlay.destroy(&mut gpu);
        gpu.destroy_target(target);
        let messages = gpu.close();
        for m in &messages {
            eprintln!("{test}: [{:?}] {} {}", m.severity, m.id_name, m.message);
        }
        let errors = messages.iter().filter(|m| m.severity == Severity::Error).count();
        assert_eq!(errors, 0, "{test}: validation errors");
    }
}
