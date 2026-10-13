//! Opens a window and prints action changes live: try a controller, the mouse, the keyboard or a
//! touchscreen against a binding file.
//!
//! ```text
//! cargo run -p specula-input --example input_probe [-- --map PATH] [--style xbox|playstation|steam_deck] [--seconds N]
//! ```
//!
//! Default map: `games/sepulcra/data/input.json`. Buttons print on press/release with the prompt
//! for the last-used device; axes print their summed amount four times a second while moving.
//! Keys inside the window: F1 rumbles every connected pad briefly, F5 rebinds `place` (press the
//! new input; Escape cancels). Close the window (or Ctrl+C) to quit.

use std::time::{Duration, Instant};

use specula_input::{ActionKind, ActionMap, BindingSlot, Input, PadStyle, RebindOutcome, RebindRequest};
use specula_platform::{keys, Event, GamepadEvent, Gamepads, Window, WindowDesc, MAX_GAMEPADS};

fn usage() -> ! {
    eprintln!("usage: input_probe [--map PATH] [--style xbox|playstation|steam_deck] [--seconds N]");
    std::process::exit(2);
}

fn main() {
    let mut map_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/data/input.json");
    let mut style = PadStyle::Xbox;
    let mut limit = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next().unwrap_or_else(|| usage());
        match arg.as_str() {
            "--map" => map_path = value.into(),
            "--style" => style = PadStyle::from_name(&value).unwrap_or_else(|| usage()),
            "--seconds" => limit = Some(Duration::from_secs_f64(value.parse().unwrap_or_else(|_| usage()))),
            _ => usage(),
        }
    }

    let text = std::fs::read_to_string(&map_path).unwrap_or_else(|e| {
        eprintln!("cannot read {}: {e}", map_path.display());
        std::process::exit(1);
    });
    let map = ActionMap::from_json_str(&text).unwrap_or_else(|e| {
        eprintln!("{}: {e}", map_path.display());
        std::process::exit(1);
    });
    println!("map: {} ({} actions)", map_path.display(), map.actions().len());
    for (a, b) in map.conflicts() {
        println!("warning: conflict {}[{}] = {}[{}]", a.action, a.binding, b.action, b.binding);
    }

    let desc = WindowDesc { title: "Specula input probe".into(), width: 800, height: 450, ..WindowDesc::default() };
    let mut window = Window::new(&desc).unwrap_or_else(|e| {
        eprintln!("cannot create window: {e}");
        std::process::exit(1);
    });
    let mut pads = match Gamepads::new() {
        Ok(p) => {
            println!("gamepads: {}", p.backend_name());
            Some(p)
        }
        Err(e) => {
            println!("gamepads unavailable: {e}");
            None
        }
    };

    let mut input = Input::new(map);
    input.set_pad_style(style);
    let (f1, f5) = (keys::from_name("F1").unwrap_or(0), keys::from_name("F5").unwrap_or(0));
    let start = Instant::now();
    let mut last_device = None;
    let mut sums: Vec<[f32; 2]> = vec![[0.0; 2]; input.map().actions().len()];
    let mut next_print = Instant::now();
    let mut rumble_until: Option<Instant> = None;

    'run: loop {
        let events = window.poll_events();
        for e in &events {
            match e {
                Event::CloseRequested => break 'run,
                Event::KeyDown { code, repeat: false } if *code == f1 => {
                    if let Some(p) = pads.as_mut() {
                        for i in 0..MAX_GAMEPADS {
                            if p.state(i).connected {
                                let _ = p.set_rumble(i, 0.6, 0.6);
                            }
                        }
                        rumble_until = Some(Instant::now() + Duration::from_millis(300));
                    }
                }
                Event::KeyDown { code, repeat: false } if *code == f5 => {
                    match input.start_rebind(RebindRequest::new("place", BindingSlot::Append)) {
                        Ok(()) => println!("rebind: press the new input for 'place' (Escape cancels)"),
                        Err(e) => println!("rebind: {e}"),
                    }
                }
                _ => {}
            }
        }
        if let Some(p) = pads.as_mut() {
            for e in p.poll() {
                match e {
                    GamepadEvent::Connected { pad } => println!("pad {pad} connected"),
                    GamepadEvent::Disconnected { pad } => println!("pad {pad} disconnected"),
                    _ => {}
                }
            }
            if rumble_until.is_some_and(|t| Instant::now() >= t) {
                for i in 0..MAX_GAMEPADS {
                    let _ = p.set_rumble(i, 0.0, 0.0);
                }
                rumble_until = None;
            }
        }
        let pad_states = pads.as_ref().map(|p| *p.states()).unwrap_or_default();
        input.update(start.elapsed().as_secs_f64(), &events, &pad_states);

        if input.last_device() != last_device {
            last_device = input.last_device();
            println!("device: {last_device:?}");
        }
        if let Some(outcome) = input.take_rebind_outcome() {
            match &outcome {
                RebindOutcome::Captured { binding, conflicts, .. } => {
                    println!("rebind: captured {} (conflicts: {conflicts:?})", binding.glyph(input.pad_style()));
                    if let Err(e) = input.apply_rebind(&outcome) {
                        println!("rebind: {e}");
                    }
                }
                RebindOutcome::Cancelled { .. } => println!("rebind: cancelled"),
            }
        }
        let names: Vec<(String, ActionKind)> = input.map().actions().iter().map(|a| (a.name.clone(), a.kind)).collect();
        for (i, (name, kind)) in names.iter().enumerate() {
            let s = input.states()[i];
            match kind {
                ActionKind::Button => {
                    if s.pressed {
                        let p = input.pointer().map(|p| format!(" at ({:.0}, {:.0})", p.x, p.y)).unwrap_or_default();
                        println!("{name}: pressed [{}]{p}", input.prompt(name).unwrap_or_else(|| "-".into()));
                    }
                    if s.released {
                        println!("{name}: released");
                    }
                }
                _ => {
                    sums[i][0] += s.value[0];
                    sums[i][1] += s.value[1];
                }
            }
        }
        if Instant::now() >= next_print {
            next_print = Instant::now() + Duration::from_millis(250);
            for (i, (name, kind)) in names.iter().enumerate() {
                let v = std::mem::take(&mut sums[i]);
                if v != [0.0, 0.0] {
                    match kind {
                        ActionKind::Axis => println!("{name}: {:+.3}", v[0]),
                        _ => println!("{name}: ({:+.1}, {:+.1})", v[0], v[1]),
                    }
                }
            }
        }
        if limit.is_some_and(|l| start.elapsed() >= l) {
            println!("time limit reached");
            break;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}
