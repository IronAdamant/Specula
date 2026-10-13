//! Games running inside the engine, and the fixed-tick clock they run on.
//!
//! A game is linked into its own binary for now (`specula::run_game`). The M3 module host will
//! load the same interface from a hot-reloadable library, so keep it narrow: the game owns its
//! simulation state, reads and writes the shared [`World`] through its public API (so agents see
//! and can edit everything the game builds), and adds its own MCP tools.
//!
//! Time (foundation F2): the simulation only knows the tick number. The clock turns real time into
//! whole ticks; rendering happens between ticks and gets `alpha` to interpolate.

use crate::sight::Camera;
use crate::world::World;
use specula_core::Json;
use specula_platform::Event;
use specula_protocol::{ToolDef, ToolOutput};
use std::time::Duration;

/// A game hosted by the engine. Every method runs on the engine thread.
pub trait Game {
    /// Shown in `engine_info`; also the agent name on every change the game makes to the world.
    fn name(&self) -> &str;

    /// Simulation ticks per second (fixed for the whole run).
    fn tick_hz(&self) -> u32 {
        30
    }

    /// Once, after the engine, window and world are ready (a loaded world may already hold
    /// the scene).
    fn start(&mut self, _ctx: &mut Ctx) {}

    /// One fixed simulation step. Not called while the clock is paused.
    fn tick(&mut self, _ctx: &mut Ctx) {}

    /// Once per window frame, after the ticks it caused and before rendering: read input, move
    /// the camera, update presentation. Headless engines never call it.
    fn frame(&mut self, _ctx: &mut Ctx, _frame: &mut Frame) {}

    /// Extra MCP tools (by convention `play_*`), answered by [`Game::call_tool`].
    fn tools(&self) -> Vec<ToolDef> {
        Vec::new()
    }

    fn call_tool(&mut self, _ctx: &mut Ctx, name: &str, _args: &Json) -> ToolOutput {
        ToolOutput::error(format!("unknown game tool '{name}'"))
    }

    /// A short status object for `engine_info` (wave, score…). Keep it small: every agent that
    /// asks for engine info pays for it.
    fn status(&self) -> Json {
        Json::Null
    }
}

/// What a game can touch.
///
/// Everything the game changes in the world is journaled under its name (see
/// [`crate::journal`]): per tick (and per frame) the changes are coalesced to one per entity and
/// kept replay-only, so hundreds of updates a tick stay cheap. The game is the simulation's rules,
/// so its writes are never checked against agents' claims.
pub struct Ctx<'a> {
    pub world: &'a mut World,
    pub clock: &'a Clock,
    /// Attribution for world changes: the game's name.
    pub agent: &'a str,
}

/// Per-frame input and presentation state.
pub struct Frame<'a> {
    /// Window input since the last frame, oldest first (mouse in client pixels, origin top-left).
    pub input: &'a [Event],
    /// Window client size in physical pixels. **0×0 while the window is minimized**: `frame` still
    /// runs then (input such as gamepads keeps arriving), so guard divisions and clamps on it.
    pub size: (u32, u32),
    /// How far between the last tick and the next one (0..1), for smooth motion.
    pub alpha: f32,
    /// Real time since the last frame, in seconds.
    pub dt_s: f32,
    /// The window camera; `None` = auto-frame the world. Agents' `window_view` writes the same slot.
    pub camera: &'a mut Option<Camera>,
    /// Set to replace the window title (the HUD until the engine has text).
    pub title: &'a mut Option<String>,
}

/// Fixed-tick simulation clock: real time in, whole ticks out.
#[derive(Clone, Debug)]
pub struct Clock {
    hz: u32,
    tick: u64,
    paused: bool,
    /// Real-time multiplier (fast-forward or slow motion).
    scale: f32,
    /// Real time not yet turned into ticks, in nanoseconds of simulated time.
    pending_ns: u64,
    /// Ticks requested by `time_step` while paused.
    steps: u64,
}

/// At most this many ticks per frame: a long stall (debugger, a slow load) drops time rather than
/// trying to catch up and stalling again.
const MAX_TICKS_PER_ADVANCE: u64 = 8;

impl Clock {
    pub fn new(hz: u32) -> Clock {
        assert!(hz > 0 && hz <= 1000, "tick rate {hz} out of range");
        Clock { hz, tick: 0, paused: false, scale: 1.0, pending_ns: 0, steps: 0 }
    }

    pub fn hz(&self) -> u32 {
        self.hz
    }

    /// Ticks completed so far: the only time the simulation knows.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Simulated seconds so far (for presentation and animation only).
    pub fn seconds(&self) -> f64 {
        self.tick as f64 / f64::from(self.hz)
    }

    fn tick_ns(&self) -> u64 {
        1_000_000_000 / u64::from(self.hz)
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        self.pending_ns = 0;
    }

    /// Clamped to 0.05..=64.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = if scale.is_finite() { scale.clamp(0.05, 64.0) } else { 1.0 };
    }

    /// Queues `n` ticks to run while paused.
    pub fn step(&mut self, n: u64) {
        self.steps = self.steps.saturating_add(n);
    }

    /// Adds real time; returns how many ticks to run now.
    pub fn advance(&mut self, real: Duration) -> u64 {
        if self.paused {
            let n = self.steps.min(MAX_TICKS_PER_ADVANCE * 64);
            self.steps -= n;
            return n;
        }
        let scaled = (real.as_nanos() as f64 * f64::from(self.scale)) as u64;
        self.pending_ns = self.pending_ns.saturating_add(scaled);
        // Fast-forward needs proportionally more ticks per frame.
        let max = MAX_TICKS_PER_ADVANCE * (self.scale.ceil() as u64).max(1);
        let n = (self.pending_ns / self.tick_ns()).min(max);
        self.pending_ns = if n == max { 0 } else { self.pending_ns - n * self.tick_ns() };
        n
    }

    /// Marks one tick as done (call after the game's `tick`).
    pub fn finish_tick(&mut self) {
        self.tick += 1;
    }

    /// Fraction of the next tick already elapsed (0 while paused).
    pub fn alpha(&self) -> f32 {
        if self.paused {
            0.0
        } else {
            (self.pending_ns as f64 / self.tick_ns() as f64).clamp(0.0, 1.0) as f32
        }
    }

    /// Real time until the next tick is due (headless engines sleep this long at most).
    pub fn until_next(&self) -> Duration {
        if self.paused {
            return Duration::from_millis(100);
        }
        let left = self.tick_ns().saturating_sub(self.pending_ns);
        Duration::from_nanos((left as f64 / f64::from(self.scale)) as u64)
    }

    pub fn to_json(&self) -> Json {
        Json::obj([
            ("tick", Json::from(self.tick)),
            ("tick_hz", Json::from(self.hz)),
            ("seconds", Json::from(self.seconds())),
            ("paused", Json::from(self.paused)),
            ("scale", Json::from(self.scale)),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_time_becomes_whole_ticks() {
        let mut c = Clock::new(30);
        assert_eq!(c.advance(Duration::from_millis(20)), 0);
        assert_eq!(c.advance(Duration::from_millis(20)), 1); // 40 ms ≥ 33.3 ms
        assert!(c.alpha() > 0.15 && c.alpha() < 0.25, "{}", c.alpha());
        assert_eq!(c.advance(Duration::from_millis(100)), 3);
    }

    #[test]
    fn long_stalls_drop_time_instead_of_spiralling() {
        let mut c = Clock::new(30);
        assert_eq!(c.advance(Duration::from_secs(10)), MAX_TICKS_PER_ADVANCE);
        assert_eq!(c.advance(Duration::from_millis(1)), 0);
    }

    #[test]
    fn pause_step_and_scale() {
        let mut c = Clock::new(30);
        c.set_paused(true);
        assert_eq!(c.advance(Duration::from_secs(1)), 0);
        c.step(3);
        assert_eq!(c.advance(Duration::ZERO), 3);
        assert_eq!(c.advance(Duration::ZERO), 0);
        c.set_paused(false);
        c.set_scale(4.0);
        assert_eq!(c.advance(Duration::from_millis(100)), 12);
        c.set_scale(f32::NAN);
        assert_eq!(c.scale(), 1.0);
    }

    #[test]
    fn ticks_count_up() {
        let mut c = Clock::new(20);
        for _ in 0..c.advance(Duration::from_millis(100)) {
            c.finish_tick();
        }
        assert_eq!((c.tick(), c.seconds()), (2, 0.1));
    }
}
