//! Opens a 960×540 window and prints every event. Esc or closing the window exits.
//!
//! `cargo run -p specula-platform --example window [-- --seconds N]` — `--seconds` exits on its
//! own after N seconds (for scripted smoke runs).

use std::time::{Duration, Instant};

use specula_platform::{Event, Window, WindowDesc};

const VK_ESCAPE: u32 = 0x1B;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut limit = None;
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next().and_then(|v| v.parse::<f64>().ok())) {
            ("--seconds", Some(secs)) if secs >= 0.0 => limit = Some(Duration::from_secs_f64(secs)),
            _ => {
                eprintln!("usage: window [--seconds N]");
                std::process::exit(2);
            }
        }
    }

    let desc =
        WindowDesc { title: "Specula — platform example".into(), width: 960, height: 540, ..WindowDesc::default() };
    let mut window = match Window::new(&desc) {
        Ok(window) => window,
        Err(err) => {
            eprintln!("cannot create window: {err}");
            std::process::exit(1);
        }
    };
    println!("window created: client {:?}, {:?}", window.client_size(), window.raw());

    let start = Instant::now();
    'run: loop {
        for event in window.poll_events() {
            println!("{event:?}");
            match event {
                Event::CloseRequested => break 'run,
                Event::KeyDown { code: VK_ESCAPE, .. } => break 'run,
                _ => {}
            }
        }
        if limit.is_some_and(|limit| start.elapsed() >= limit) {
            println!("time limit reached");
            break;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    println!("exiting after {:.1}s", start.elapsed().as_secs_f64());
}
