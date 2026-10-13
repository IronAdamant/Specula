//! The Sepulcra sound set stays valid, levelled and deterministic.

use specula_audio::{analysis, synth, RenderOptions};
use std::path::PathBuf;

fn recipes() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets/sounds");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|p| (p.file_stem().unwrap().to_string_lossy().into_owned(), std::fs::read_to_string(&p).unwrap()))
        .collect();
    out.sort();
    out
}

/// Every sound the brief asks for exists.
#[test]
fn the_set_is_complete() {
    let names: Vec<String> = recipes().into_iter().map(|(n, _)| n).collect();
    for want in [
        "bolt_fire", "bolt_hit", "pillar_pulse", "brazier_whoosh", "brazier_crackle_loop", "obelisk_zap", "stone_place", "upgrade_chime",
        "sell", "wall_place", "wall_hit", "wall_broken", "shambler_death", "runner_death", "wraith_death", "colossus_collapse",
        "tombstone_lost", "wave_start", "wave_cleared", "victory", "defeat", "ui_click", "ui_hover", "ui_error", "night_ambience",
    ] {
        assert!(names.iter().any(|n| n == want), "missing sound {want}");
    }
}

#[test]
fn every_recipe_renders_levelled_without_clipping() {
    for (file, text) in recipes() {
        let recipe = synth::Recipe::parse(&text).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert_eq!(recipe.name, file, "recipe name must match its file name");
        let r = synth::render(&recipe, &RenderOptions::default()).unwrap_or_else(|e| panic!("{file}: {e}"));
        let m = analysis::analyze(&r.samples, r.channels as usize, r.rate);
        assert_eq!(m.clipped, 0, "{file} clips");
        assert!(m.peak_dbfs <= recipe.peak_ceiling_db + 0.01, "{file}: peak {}", m.peak_dbfs);
        let target = recipe.loudness.unwrap_or_else(|| panic!("{file}: every set sound declares a loudness target"));
        assert!((m.lufs - target).abs() <= 0.5, "{file}: {:.2} LUFS vs target {target}", m.lufs);
        assert!(m.dc_offset.abs() < 0.01, "{file}: DC offset {}", m.dc_offset);
        if r.looping {
            // Loops wrap without a jump bigger than the sound's own largest step.
            let ch = r.channels as usize;
            let s = &r.samples;
            let max_step = s.windows(2 * ch).map(|w| (w[ch] - w[0]).abs()).fold(0f32, f32::max);
            let wrap = (s[0] - s[s.len() - ch]).abs();
            assert!(wrap <= max_step, "{file}: loop seam jump {wrap} > max step {max_step}");
        }
        // Variations differ from the canonical render but are reproducible.
        if recipe.vary_pitch.is_some() || recipe.vary_gain_db.is_some() {
            let v = synth::render(&recipe, &RenderOptions { variation: 3, ..Default::default() }).unwrap();
            assert_ne!(v.hash, r.hash, "{file}: variation 3 equals the canonical render");
            assert_eq!(v.hash, synth::render(&recipe, &RenderOptions { variation: 3, ..Default::default() }).unwrap().hash);
        }
    }
}
