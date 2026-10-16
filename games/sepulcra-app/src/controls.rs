//! Input: window events and gamepads in, camera motion and player [`Action`]s out.
//!
//! Driven by `specula-input` actions, not keys (F6): the bindings are data,
//! `games/sepulcra/data/input.json` (the game's shared actions: place, cancel, faces, pause,
//! orbit, zoom, cursor) plus `games/sepulcra-app/data/input_app.json` (selection, upgrade, sell,
//! waves, pan, ...), merged at start-up. Mouse, keyboard, gamepad (XInput) and touch all go
//! through the same actions. All input handling lives in this module: the rest of the game only
//! sees [`Action`]s and the hovered cell.

use crate::slab::SlabGeo;
use sepulcra::map::Map;
use sepulcra::CellId;
use specula::sight::Camera;
use specula_core::math::Vec3;
use specula_core::Json;
use specula_input::{ActionMap, Device, Input};
use specula_ui::Consumed;
use specula_platform::gamepad::Gamepads;
use specula_platform::{Event, GamepadState, MAX_GAMEPADS};
use std::time::Instant;

/// The game's shared bindings and the app's additions.
pub const GAME_BINDINGS: &str = include_str!("../../sepulcra/data/input.json");
pub const APP_BINDINGS: &str = include_str!("../data/input_app.json");

/// What the player asked for this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// 0-based stone index, or `None` for the wall.
    Select(Option<u8>),
    /// The next stone (then the wall).
    SelectNext,
    /// Open the radial build menu at the cursor.
    BuildMenu,
    /// Close what's open (a selection), else pause.
    Cancel,
    Build(CellId),
    Upgrade(CellId),
    Sell(CellId),
    Unwall(CellId),
    CallWave,
    TogglePause,
    ToggleHelp,
    CycleSpeed,
    NewGame,
}

/// The merged action map: the game's file with the app's actions appended (a name used twice is
/// an error from the map's own validation).
pub fn bindings() -> Result<ActionMap, String> {
    let mut game = Json::parse(GAME_BINDINGS).map_err(|e| format!("sepulcra/data/input.json: {}", e.message))?;
    let app = Json::parse(APP_BINDINGS).map_err(|e| format!("sepulcra-app/data/input_app.json: {}", e.message))?;
    let mut actions: Vec<Json> = game.get("actions").and_then(Json::as_array).unwrap_or(&[]).to_vec();
    actions.extend(app.get("actions").and_then(Json::as_array).unwrap_or(&[]).iter().cloned());
    game.set("actions", Json::Arr(actions));
    let mut map = ActionMap::from_json(&game).map_err(|e| format!("input bindings: {e}"))?;
    // The HUD's own actions (focus, accept, back) for whatever the files leave out.
    specula_ui::add_ui_actions(&mut map).map_err(|e| format!("ui bindings: {e}"))?;
    Ok(map)
}

/// An orbit camera around the slab.
#[derive(Clone, Copy, Debug)]
pub struct Orbit {
    pub target: Vec3,
    /// Degrees about +Y; 0 looks from the south (+Z) towards the north.
    pub yaw: f32,
    /// Degrees above the horizon.
    pub pitch: f32,
    pub dist: f32,
    /// Where the next/previous-face buttons are turning to.
    pub yaw_goal: Option<f32>,
    pub fov_y_deg: f32,
}

pub const MIN_DIST: f32 = 16.0;
pub const MAX_DIST: f32 = 130.0;
const MIN_PITCH: f32 = 6.0;
const MAX_PITCH: f32 = 86.0;
const PAN_LIMIT: f32 = 30.0;

impl Default for Orbit {
    /// A three-quarter view from the south-east that shows the top and the east cliff.
    fn default() -> Self {
        Orbit { target: Vec3::new(0.0, -2.0, 1.0), yaw: 35.0, pitch: 42.0, dist: 56.0, yaw_goal: None, fov_y_deg: 45.0 }
    }
}

impl Orbit {
    pub fn camera(&self) -> Camera {
        let (y, p) = (self.yaw.to_radians(), self.pitch.to_radians());
        let dir = Vec3::new(p.cos() * y.sin(), p.sin(), p.cos() * y.cos());
        Camera::new(self.target + dir * self.dist, self.target, self.fov_y_deg)
    }

    /// Takes over a camera set from outside (an agent's `window_view`).
    pub fn adopt(cam: &Camera) -> Orbit {
        let off = cam.eye - cam.target;
        let dist = off.length().clamp(MIN_DIST * 0.5, MAX_DIST * 2.0);
        let pitch = (off.y / off.length().max(1e-6)).clamp(-1.0, 1.0).asin().to_degrees();
        let yaw = off.x.atan2(off.z).to_degrees();
        Orbit { target: cam.target, yaw, pitch, dist, yaw_goal: None, fov_y_deg: cam.fov_y_deg }
    }

    fn pan(&mut self, right: f32, forward: f32) {
        let y = self.yaw.to_radians();
        // Screen right and screen "into the slab", flat on the ground.
        let r = Vec3::new(y.cos(), 0.0, -y.sin());
        let f = Vec3::new(-y.sin(), 0.0, -y.cos());
        let t = self.target + r * right + f * forward;
        self.target = Vec3::new(t.x.clamp(-PAN_LIMIT, PAN_LIMIT), t.y, t.z.clamp(-PAN_LIMIT, PAN_LIMIT));
    }

    /// Starts turning to the next face-centred view (yaw 0, 90, 180, 270) in direction `step`.
    fn turn(&mut self, step: f32) {
        let from = self.yaw_goal.unwrap_or(self.yaw);
        let snapped = if step < 0.0 { ((from - 1.0) / 90.0).floor() * 90.0 } else { ((from + 1.0) / 90.0).ceil() * 90.0 };
        self.yaw_goal = Some(if (snapped - from).abs() < 1.0 { from + 90.0 * step } else { snapped });
    }
}

fn same(a: &Camera, b: &Camera) -> bool {
    a.eye == b.eye && a.target == b.target && a.fov_y_deg == b.fov_y_deg
}

/// Input state carried between frames.
pub struct Controls {
    pub orbit: Orbit,
    input: Input,
    pads: Option<Gamepads>,
    started: Instant,
    /// The gamepad's cursor, in window pixels.
    pad_cursor: Option<(f32, f32)>,
    /// The camera this module last wrote (to notice agents' changes).
    written: Option<Camera>,
    /// The cell under the cursor.
    pub hover: Option<CellId>,
}

impl Default for Controls {
    fn default() -> Self {
        Controls::new()
    }
}

impl Controls {
    pub fn new() -> Controls {
        let map = bindings().expect("the built-in bindings are valid (tested)");
        Controls {
            orbit: Orbit::default(),
            input: Input::new(map),
            pads: None,
            started: Instant::now(),
            pad_cursor: None,
            written: None,
            hover: None,
        }
    }

    /// Starts polling gamepads (optional: without XInput the game is mouse, touch and keyboard).
    pub fn enable_gamepads(&mut self) {
        match Gamepads::new() {
            Ok(p) => self.pads = Some(p),
            Err(e) => eprintln!("sepulcra: no gamepads: {e}"),
        }
    }

    /// Reads this frame's events (and pads). Then the HUD sees the input
    /// ([`Controls::input`]), and [`Controls::act`] turns what's left into play.
    pub fn begin(&mut self, events: &[Event]) {
        let pads = match &mut self.pads {
            Some(p) => {
                p.poll();
                *p.states()
            }
            None => [GamepadState::default(); MAX_GAMEPADS],
        };
        // This clock only measures frame time for input (taps, holds), never the simulation.
        let now = self.started.elapsed().as_secs_f64();
        self.input.update(now, events, &pads);
    }

    /// [`Controls::begin`] with an explicit input clock and pad states (tests, playtests).
    pub fn begin_at(&mut self, now_s: f64, events: &[Event], pads: &[GamepadState]) {
        self.input.update(now_s, events, pads);
    }

    /// This frame's input (the HUD reads it before the game acts on it).
    pub fn input(&self) -> &Input {
        &self.input
    }

    /// Mutable input (settings: glyph style; controls: rebinding).
    pub fn input_mut(&mut self) -> &mut Input {
        &mut self.input
    }

    /// [`Controls::begin_at`] then [`Controls::act`] with nothing taken by a HUD (tests).
    #[allow(clippy::too_many_arguments)]
    pub fn frame_at(&mut self, now_s: f64, events: &[Event], pads: &[GamepadState], dt: f32, size: (u32, u32), geo: &SlabGeo, map: &Map) -> Vec<Action> {
        self.begin_at(now_s, events, pads);
        self.act(Consumed::default(), dt, size, geo, map)
    }

    /// Moves the camera and returns the player's actions, leaving out what the HUD took this
    /// frame (`taken`): nothing while a modal menu is open, no buttons while a menu has focus,
    /// no pointer play while the mouse or a finger is on the HUD.
    pub fn act(&mut self, taken: Consumed, dt: f32, size: (u32, u32), geo: &SlabGeo, map: &Map) -> Vec<Action> {
        if taken.all {
            return Vec::new();
        }
        let input = &self.input;
        let pad = matches!(input.last_device(), Some(Device::Gamepad(_)));
        let pointer_taken = taken.pointer && !pad;
        let buttons_taken = taken.navigation;
        // Camera.
        let [ox, oy] = if pointer_taken { [0.0; 2] } else { input.value2("orbit") };
        if ox != 0.0 || oy != 0.0 {
            self.orbit.yaw -= ox * 0.3;
            self.orbit.pitch = (self.orbit.pitch + oy * 0.25).clamp(MIN_PITCH, MAX_PITCH);
            self.orbit.yaw_goal = None;
        }
        let zoom = if pointer_taken { 0.0 } else { input.value("zoom") };
        if zoom != 0.0 {
            self.orbit.dist = (self.orbit.dist * 0.88f32.powf(zoom)).clamp(MIN_DIST, MAX_DIST);
        }
        let [px, py] = if pointer_taken { [0.0; 2] } else { input.value2("pan") };
        if px != 0.0 || py != 0.0 {
            let k = self.orbit.dist;
            self.orbit.pan(px * k, py * k);
        }
        if input.pressed("prev_face") {
            self.orbit.turn(-1.0);
        }
        if input.pressed("next_face") {
            self.orbit.turn(1.0);
        }
        if let Some(goal) = self.orbit.yaw_goal {
            let k = 1.0 - (-dt * 9.0).exp();
            self.orbit.yaw += (goal - self.orbit.yaw) * k;
            if (goal - self.orbit.yaw).abs() < 0.05 {
                self.orbit.yaw = goal.rem_euclid(360.0);
                self.orbit.yaw_goal = None;
            }
        }
        // The cursor: the pointer (mouse, touch), or the gamepad's own cursor. A minimized window
        // is 0x0: there is nothing to point at, so the cursor and the hovered cell stay as they are.
        if size.0 > 0 && size.1 > 0 {
            let [cx, cy] = if buttons_taken { [0.0; 2] } else { input.value2("cursor_move") };
            let (w, h) = (size.0 as f32, size.1 as f32);
            if cx != 0.0 || cy != 0.0 {
                let (x, y) = self.pad_cursor.unwrap_or((w / 2.0, h / 2.0));
                self.pad_cursor = Some(((x + cx).clamp(0.0, (w - 1.0).max(0.0)), (y + cy).clamp(0.0, (h - 1.0).max(0.0))));
            }
            let at = match input.last_device() {
                Some(Device::Gamepad(_)) => Some(self.pad_cursor.unwrap_or((w / 2.0, h / 2.0))),
                _ => input.pointer().map(|p| (p.x, p.y)),
            };
            let orbit = self.orbit;
            self.hover = at.filter(|_| !pointer_taken).and_then(|(x, y)| {
                let (o, d) = orbit.camera().ray(x, y, size.0, size.1);
                geo.pick(map, o, d)
            });
        }
        // Buttons, in a fixed order.
        let mut actions = Vec::new();
        if buttons_taken {
            return actions;
        }
        for (i, name) in ["select_1", "select_2", "select_3", "select_4"].iter().enumerate() {
            if input.pressed(name) {
                actions.push(Action::Select(Some(i as u8)));
            }
        }
        let simple = [
            ("select_wall", Action::Select(None)),
            ("build_menu", Action::BuildMenu),
            ("call_wave", Action::CallWave),
            ("pause", Action::TogglePause),
            // Deselects, else opens the pause (as the binding file says).
            ("cancel", Action::Cancel),
            ("help", Action::ToggleHelp),
            ("speed", Action::CycleSpeed),
            ("new_game", Action::NewGame),
        ];
        for (name, action) in simple {
            if input.pressed(name) {
                actions.push(action);
            }
        }
        type OnCell = fn(CellId) -> Action;
        let on_cell: [(&str, OnCell); 4] =
            [("place", Action::Build), ("upgrade", Action::Upgrade), ("sell", Action::Sell), ("unwall", Action::Unwall)];
        for (name, action) in on_cell {
            if input.pressed(name) && !(pointer_taken && name == "place") {
                actions.extend(self.hover.map(action));
            }
        }
        actions
    }

    /// The prompt for an action on the device in use ("LMB", "A", "Tap"), for the HUD.
    pub fn prompt(&self, action: &str) -> String {
        self.input.prompt(action).unwrap_or_default()
    }

    /// Adopts a camera an agent set since the last frame (`window_view`), then writes this
    /// frame's camera to the engine's slot. While the window looks through an agent's camera
    /// (`agent_view`) it does neither: the view is the agent's (and may be closer or farther than
    /// the orbit allows); when that ends, the camera it left is adopted (clamped) and play goes on
    /// from there.
    pub fn sync_camera(&mut self, slot: &mut Option<Camera>, agent_view: bool) {
        if agent_view {
            return;
        }
        match (slot.as_ref(), self.written.as_ref()) {
            (Some(now), Some(mine)) if !same(now, mine) => self.orbit = Orbit::adopt(now),
            (Some(now), None) => self.orbit = Orbit::adopt(now),
            // An agent asked for auto-framing: back to the default view.
            (None, Some(_)) => self.orbit = Orbit::default(),
            _ => {}
        }
        let cam = self.orbit.camera();
        *slot = Some(cam);
        self.written = Some(cam);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sepulcra::Content;
    use specula_platform::MouseButton;

    #[test]
    fn the_merged_bindings_are_valid() {
        let map = bindings().unwrap();
        for name in ["place", "orbit", "zoom", "pan", "select_1", "select_wall", "upgrade", "sell", "unwall", "call_wave", "help"] {
            assert!(map.to_json().to_string().contains(&format!("\"{name}\"")), "{name} missing");
        }
    }

    #[test]
    fn adopting_a_camera_round_trips() {
        let o = Orbit { target: Vec3::new(1.0, -2.0, 3.0), yaw: 120.0, pitch: 30.0, dist: 50.0, yaw_goal: None, fov_y_deg: 45.0 };
        let back = Orbit::adopt(&o.camera());
        assert!((back.yaw - 120.0).abs() < 1e-3 && (back.pitch - 30.0).abs() < 1e-3 && (back.dist - 50.0).abs() < 1e-3);
    }

    #[test]
    fn input_becomes_actions_and_clicks_pick_cells() {
        let content = Content::builtin();
        let geo = SlabGeo::new(&content.map);
        let mut c = Controls::new();
        let size = (1280, 720);
        let key = |code: u32| Event::KeyDown { code, repeat: false };
        // The mouse at the middle of the screen: the camera looks at the slab.
        let input = [Event::MouseMove { x: 640, y: 360 }, key(0x32), key(0x20), Event::MouseDown { button: MouseButton::Left, x: 640, y: 360 }];
        let actions = c.frame_at(0.0, &input, &[], 0.016, size, &geo, &content.map);
        assert_eq!(actions[0], Action::Select(Some(1)), "{actions:?}");
        assert_eq!(actions[1], Action::CallWave);
        assert!(matches!(actions[2], Action::Build(_)), "{actions:?}");
        assert!(c.hover.is_some());
        let up = [Event::KeyUp { code: 0x32 }, Event::KeyUp { code: 0x20 }, Event::MouseUp { button: MouseButton::Left, x: 640, y: 360 }];
        c.frame_at(0.02, &up, &[], 0.016, size, &geo, &content.map);
        // Q turns to a face-centred view.
        c.frame_at(0.04, &[key(0x51)], &[], 0.016, size, &geo, &content.map);
        assert_eq!(c.orbit.yaw_goal, Some(0.0));
        // Right-drag orbits.
        let yaw = c.orbit.yaw;
        let drag = [Event::MouseDown { button: MouseButton::Right, x: 640, y: 360 }, Event::MouseMove { x: 700, y: 360 }];
        c.frame_at(0.06, &drag, &[], 0.016, size, &geo, &content.map);
        assert!(c.orbit.yaw < yaw, "dragging right turns the view: {} -> {}", yaw, c.orbit.yaw);
    }

    #[test]
    fn agents_window_view_is_adopted() {
        let mut c = Controls::new();
        let mut slot = None;
        c.sync_camera(&mut slot, false);
        let agent = Camera::new(Vec3::new(0.0, 40.0, 1.0), Vec3::ZERO, 50.0);
        slot = Some(agent);
        c.sync_camera(&mut slot, false);
        let now = slot.unwrap();
        assert!((now.eye - agent.eye).length() < 1e-3, "{now:?}");
        slot = None;
        c.sync_camera(&mut slot, false);
        assert!((slot.unwrap().eye - Orbit::default().camera().eye).length() < 1e-3);
        // Round 8: an agent camera the engine keeps (window_view {camera}) is neither adopted nor
        // overwritten, however close; when it ends, play continues from it (clamped).
        let close = Camera::new(Vec3::new(0.0, 2.0, 5.0), Vec3::ZERO, 50.0);
        slot = Some(close);
        for _ in 0..3 {
            c.sync_camera(&mut slot, true);
            assert_eq!(slot.map(|s| s.eye), Some(close.eye), "the agent's view stays");
        }
        c.sync_camera(&mut slot, false);
        let back = slot.unwrap();
        assert!((back.target - close.target).length() < 1e-3, "continues from the agent's view: {back:?}");
    }
}
