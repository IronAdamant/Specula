//! Window tests through the public API only. They create hidden windows; without a desktop
//! session (or on platforms without a backend) creation fails and each test prints why and passes.
//!
//! Tests that need Win32 calls of their own live inside the crate (`src/win32.rs`): a second
//! crate declaring its own `user32` raw-dylib imports has them dropped at link time on
//! windows-gnu when linked together with this crate.

use std::time::{Duration, Instant};

use specula_platform::{Event, Window, WindowDesc};

fn hidden(width: u32, height: u32) -> WindowDesc {
    WindowDesc { title: "specula-platform test".into(), width, height, visible: false, resizable: true }
}

/// `None` (after printing the reason) when windows can't be created here.
fn try_window(desc: &WindowDesc) -> Option<Window> {
    match Window::new(desc) {
        Ok(window) => Some(window),
        Err(err) => {
            eprintln!("skipping: cannot create a window here: {err}");
            None
        }
    }
}

#[test]
fn hidden_window_has_requested_client_size() {
    let Some(window) = try_window(&hidden(640, 360)) else { return };
    assert_eq!(window.client_size(), (640, 360));
    let raw = window.raw();
    assert!(!raw.hwnd.is_null());
    assert!(!raw.hinstance.is_null());
}

#[test]
fn fixed_size_window_has_requested_client_size() {
    let desc = WindowDesc { resizable: false, ..hidden(321, 123) };
    let Some(window) = try_window(&desc) else { return };
    assert_eq!(window.client_size(), (321, 123));
}

#[test]
fn out_of_range_sizes_are_rejected() {
    for (w, h) in [(0, 100), (100, 0), (40_000, 100)] {
        let err = Window::new(&hidden(w, h)).err().expect("size must be rejected");
        assert!(!err.message.is_empty());
    }
}

#[test]
fn poll_events_returns_promptly() {
    let Some(mut window) = try_window(&hidden(320, 240)) else { return };
    let start = Instant::now();
    for _ in 0..10 {
        let _ = window.poll_events();
    }
    assert!(start.elapsed() < Duration::from_secs(1), "poll_events blocked: {:?}", start.elapsed());
    // An idle hidden window produces nothing once drained.
    assert_eq!(window.poll_events(), Vec::<Event>::new());
}

#[test]
fn set_title_and_hide_keep_the_window_usable() {
    let Some(mut window) = try_window(&hidden(200, 100)) else { return };
    window.set_title("renamed \u{1F600}");
    window.set_visible(false);
    let _ = window.poll_events();
    assert_eq!(window.client_size(), (200, 100));
}

#[test]
// `Window` only implements Drop where a backend exists; elsewhere it is uninhabited and these
// explicit drops (which document the order) are no-ops.
#[cfg_attr(not(windows), allow(clippy::drop_non_drop))]
fn several_windows_and_clean_drop() {
    let Some(a) = try_window(&hidden(160, 100)) else { return };
    let Some(mut b) = try_window(&hidden(150, 120)) else { return };
    drop(a);
    // The survivor keeps working after its sibling is gone.
    let _ = b.poll_events();
    assert_eq!(b.client_size(), (150, 120));
    drop(b);
    // Creating again after drops reuses the registered class.
    let Some(c) = try_window(&hidden(180, 90)) else { return };
    assert_eq!(c.client_size(), (180, 90));
}
