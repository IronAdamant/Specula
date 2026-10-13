//! The heads-up display, as data. Until the engine draws text it renders into the window title;
//! a text renderer will draw the same fields as panels (STYLE.md: carved-stone panels, ivory
//! text, cyan highlights, green only for threats).

use sepulcra::{Outcome, State, TICK_HZ};

/// Everything the HUD shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hud {
    pub wave: u16,
    pub waves_total: u16,
    /// Seconds until the next wave starts by itself.
    pub next_wave_s: Option<u32>,
    pub dust: i32,
    pub tombstones: i32,
    pub tombstones_max: i32,
    /// "Warding stone (50)" or "Wall (15)".
    pub selected: String,
    /// The last thing that happened (or why an action was refused).
    pub message: String,
    pub paused: bool,
    pub speed: u32,
    pub outcome: Option<Outcome>,
    pub help: bool,
    /// What the cursor points at: "top:12,9 Warding stone II".
    pub hover: String,
}

pub const HELP: &str = "RMB drag orbit | wheel zoom | MMB drag/WASD pan | Q/E turn to next face | 1-4 stones, 5 wall | LMB build | U upgrade | X sell | R remove wall | Space next wave | P/Esc pause | F speed | N new game (after the end) | F1 help";

impl Hud {
    pub fn read(&mut self, state: &State) {
        self.wave = state.wave();
        self.waves_total = state.waves_total();
        self.next_wave_s = state.next_wave_tick().map(|t| t.saturating_sub(state.tick()).div_ceil(TICK_HZ));
        self.dust = state.dust();
        self.tombstones = state.tombstones();
        self.tombstones_max = state.content().rules.tombstones;
        self.outcome = match state.outcome() {
            Outcome::Running => None,
            o => Some(o),
        };
    }

    /// The HUD as one line (the window title for now).
    pub fn title(&self) -> String {
        if self.help {
            return format!("Sepulcra | {HELP}");
        }
        let mut t = format!("Sepulcra | Wave {}/{}", self.wave, self.waves_total);
        if let Some(s) = self.next_wave_s {
            t.push_str(&format!(" (next in {s}s)"));
        }
        t.push_str(&format!(" | Dust {} | Tombstones {}/{} | Selected: {}", self.dust, self.tombstones, self.tombstones_max, self.selected));
        if !self.hover.is_empty() {
            t.push_str(&format!(" | {}", self.hover));
        }
        match self.outcome {
            Some(Outcome::Won) => t.push_str(" | VICTORY - N for a new game"),
            Some(Outcome::Lost) => t.push_str(" | DEFEAT - N for a new game"),
            _ => {}
        }
        if self.paused {
            t.push_str(" | PAUSED");
        }
        if self.speed > 1 {
            t.push_str(&format!(" | x{}", self.speed));
        }
        if !self.message.is_empty() {
            t.push_str(&format!(" | {}", self.message));
        }
        t.push_str(" | F1 help");
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_reads_like_the_brief() {
        let hud = Hud {
            wave: 3,
            waves_total: 10,
            dust: 240,
            tombstones: 9,
            tombstones_max: 10,
            selected: "Warding stone (50)".into(),
            message: "Wave 3 started".into(),
            speed: 1,
            ..Hud::default()
        };
        let t = hud.title();
        assert!(t.starts_with("Sepulcra | Wave 3/10 | Dust 240 | Tombstones 9/10 | Selected: Warding stone (50)"), "{t}");
        assert!(t.contains("Wave 3 started"));
        assert!(Hud { help: true, ..hud }.title().contains("Space next wave"));
    }
}
