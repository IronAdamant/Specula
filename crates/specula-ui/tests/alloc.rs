//! Once warm, building a frame allocates nothing (F3): the Sepulcra HUD with the radial menu,
//! threat arrows, toasts and changing bound values, counted by a wrapping global allocator.

mod common;
use common::*;
use specula_ui::{FontSet, StateMap, Theme, ToastKind, Ui, Viewport};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Counting;

thread_local! {
    static COUNT: Cell<u64> = const { Cell::new(0) };
}

// SAFETY: forwards to the system allocator unchanged; only counts calls on this thread.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        COUNT.with(|c| c.set(c.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        COUNT.with(|c| c.set(c.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static A: Counting = Counting;

fn count() -> u64 {
    COUNT.with(Cell::get)
}

fn fill(s: &mut StateMap, k: i64) {
    s.set_int("wave", 4)
        .set_int("waves", 10)
        .set_num("next_wave_s", 20.0 + (k % 10) as f64)
        .set_int("early_bonus", 11)
        .set_int("dust", 100 + k % 10)
        .set_int("tombstones", 7)
        .set_int("tombstones_max", 10)
        .set_bool("paused", false)
        .set_int("speed", 2)
        .set_str("selected", "warding")
        .set_str("selected_name", "Warding stone I")
        .set_str("selected_role", "Single target")
        .set_str("selected_detail", "Homing bolts: 10 damage every 0.8 s, 7 m range.")
        .set_int("selected_cost", 50)
        .set_bool("affordable", k % 2 == 0)
        .set_str("hover", if k % 2 == 0 { "top:12,9" } else { "top:3,4 fissure" });
    let t = s.list_mut("threats");
    t.clear();
    t.push().set_num("x", 1900.0).set_num("y", 300.0).set_bool("behind", false).set_str("label", "East cliff");
    t.push().set_num("x", 700.0 + (k % 7) as f64).set_num("y", 500.0).set_bool("behind", true).set_str("label", "North");
}

#[test]
fn warm_frames_do_not_allocate() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets/ui");
    let theme = Theme::from_json_str(&std::fs::read_to_string(dir.join("theme.json")).unwrap()).unwrap();
    let mut ui = Ui::new(theme, FontSet::new().with("ui", specula_ui::testfont::box_font_parsed()));
    for n in ["hud", "radial"] {
        ui.load_screen(&std::fs::read_to_string(dir.join(format!("{n}.json"))).unwrap()).unwrap();
        ui.show(n).unwrap();
    }
    let mut h = Harness::new();
    let mut s = StateMap::new();
    let v = Viewport::new([1280, 800], 1.0);
    ui.toast("Wave 4: wraiths circle the north face", ToastKind::Threat);
    // Warm up through every state the measured frames will see (all digits, both strings).
    for k in 0..40 {
        fill(&mut s, k);
        let e = mv(400.0 + (k % 5) as f32 * 40.0, 300.0);
        h.frame(&mut ui, &[e], &s, &v);
    }
    let before = count();
    for k in 0..200 {
        fill(&mut s, k);
        h.step(&[]);
        let f = ui.frame(&h.input, &s, &v);
        assert!(!f.draw.cmds.is_empty());
    }
    let n = count() - before;
    // `Harness::step` (the input crate) is outside the UI; measure it alone and subtract.
    let base = count();
    for _ in 0..200 {
        h.step(&[]);
    }
    let input_allocs = count() - base;
    assert_eq!(n - input_allocs, 0, "{n} allocations in 200 warm frames ({input_allocs} from input updates)");
}
