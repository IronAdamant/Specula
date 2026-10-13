//! Mouse and keyboard: window events in, camera motion and player [`Action`]s out.
//!
//! All input handling lives here so the Input lane's data-driven actions (and the gamepad) can
//! replace it in one place: the rest of the game only sees [`Action`]s and the hovered cell.
//! Nothing outside this module knows about keys or buttons (F6: nothing may assume a keyboard).

use crate::slab::SlabGeo;
use sepulcra::map::Map;
use sepulcra::CellId;
use specula::sight::Camera;
use specula_core::math::Vec3;
use specula_platform::{Event, MouseButton};

/// What the player asked for this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// 0-based stone index (keys 1–4), or `None` for the wall (key 5).
    Select(Option<u8>),
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

// Win32 virtual-key codes.
const VK_SPACE: u32 = 0x20;
const VK_ESCAPE: u32 = 0x1b;
const VK_F1: u32 = 0x70;
const VK_LEFT: u32 = 0x25;
const VK_UP: u32 = 0x26;
const VK_RIGHT: u32 = 0x27;
const VK_DOWN: u32 = 0x28;
const VK_NUMPAD1: u32 = 0x61;

fn key(c: char) -> u32 {
    c as u32
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
    /// Where Q/E is turning to.
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
        Orbit { target: Vec3::new(0.0, -3.0, 0.0), yaw: 35.0, pitch: 42.0, dist: 66.0, yaw_goal: None, fov_y_deg: 45.0 }
    }
}

impl Orbit {
    pub fn camera(&self) -> Camera {
        let (y, p) = (self.yaw.to_radians(), self.pitch.to_radians());
        let dir = Vec3::new(p.cos() * y.sin(), p.sin(), p.cos() * y.cos());
        Camera { eye: self.target + dir * self.dist, target: self.target, fov_y_deg: self.fov_y_deg }
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
}

fn same(a: &Camera, b: &Camera) -> bool {
    a.eye == b.eye && a.target == b.target && a.fov_y_deg == b.fov_y_deg
}

/// Input state carried between frames.
#[derive(Default)]
pub struct Controls {
    pub orbit: Orbit,
    mouse: Option<(f32, f32)>,
    right: bool,
    middle: bool,
    /// Pan keys held: left, right, forward, back.
    held: [bool; 4],
    /// The camera this module last wrote (to notice agents' changes).
    written: Option<Camera>,
    /// The cell under the mouse.
    pub hover: Option<CellId>,
}

impl Controls {
    /// Reads this frame's events, moves the camera, and returns the player's actions.
    pub fn frame(&mut self, input: &[Event], dt: f32, size: (u32, u32), geo: &SlabGeo, map: &Map) -> Vec<Action> {
        let mut actions = Vec::new();
        let pick = |orbit: &Orbit, x: f32, y: f32| {
            let (o, d) = orbit.camera().ray(x, y, size.0, size.1);
            geo.pick(map, o, d)
        };
        for ev in input {
            match *ev {
                Event::MouseMove { x, y } => {
                    let (x, y) = (x as f32, y as f32);
                    if let Some((px, py)) = self.mouse {
                        let (dx, dy) = (x - px, y - py);
                        if self.right {
                            self.orbit.yaw -= dx * 0.3;
                            self.orbit.pitch = (self.orbit.pitch + dy * 0.25).clamp(MIN_PITCH, MAX_PITCH);
                            self.orbit.yaw_goal = None;
                        }
                        if self.middle {
                            let k = self.orbit.dist * 0.0016;
                            self.orbit.pan(-dx * k, dy * k);
                        }
                    }
                    self.mouse = Some((x, y));
                }
                Event::MouseDown { button, x, y } => {
                    self.mouse = Some((x as f32, y as f32));
                    match button {
                        MouseButton::Right => self.right = true,
                        MouseButton::Middle => self.middle = true,
                        MouseButton::Left => {
                            if let Some(cell) = pick(&self.orbit, x as f32, y as f32) {
                                actions.push(Action::Build(cell));
                            }
                        }
                    }
                }
                Event::MouseUp { button, .. } => match button {
                    MouseButton::Right => self.right = false,
                    MouseButton::Middle => self.middle = false,
                    MouseButton::Left => {}
                },
                Event::MouseWheel { delta } => {
                    self.orbit.dist = (self.orbit.dist * 0.88f32.powf(delta)).clamp(MIN_DIST, MAX_DIST);
                }
                Event::Focused(false) => {
                    self.right = false;
                    self.middle = false;
                    self.held = [false; 4];
                }
                Event::KeyDown { code, repeat } => {
                    if let Some(i) = pan_key(code) {
                        self.held[i] = true;
                    }
                    if repeat {
                        continue;
                    }
                    let hovered = self.hover;
                    let on = |a: fn(CellId) -> Action| hovered.map(a);
                    let action = match code {
                        c if (key('1')..=key('4')).contains(&c) => Some(Action::Select(Some((c - key('1')) as u8))),
                        c if (VK_NUMPAD1..VK_NUMPAD1 + 4).contains(&c) => Some(Action::Select(Some((c - VK_NUMPAD1) as u8))),
                        c if c == key('5') || c == VK_NUMPAD1 + 4 => Some(Action::Select(None)),
                        c if c == key('U') => on(Action::Upgrade),
                        c if c == key('X') => on(Action::Sell),
                        c if c == key('R') => on(Action::Unwall),
                        c if c == key('P') || c == VK_ESCAPE => Some(Action::TogglePause),
                        c if c == key('F') => Some(Action::CycleSpeed),
                        c if c == key('N') => Some(Action::NewGame),
                        VK_SPACE => Some(Action::CallWave),
                        VK_F1 => Some(Action::ToggleHelp),
                        c if c == key('Q') || c == key('E') => {
                            let from = self.orbit.yaw_goal.unwrap_or(self.orbit.yaw);
                            let step = if c == key('Q') { -90.0 } else { 90.0 };
                            // Snap to the next face-centred view (0, 90, 180, 270).
                            let snapped = if c == key('Q') { ((from - 1.0) / 90.0).floor() * 90.0 } else { ((from + 1.0) / 90.0).ceil() * 90.0 };
                            let goal = if (snapped - from).abs() < 1.0 { from + step } else { snapped };
                            self.orbit.yaw_goal = Some(goal);
                            None
                        }
                        _ => None,
                    };
                    actions.extend(action);
                }
                Event::KeyUp { code } => {
                    if let Some(i) = pan_key(code) {
                        self.held[i] = false;
                    }
                }
                _ => {}
            }
        }
        // Keyboard pan.
        let speed = self.orbit.dist * 0.7 * dt;
        let (mut r, mut f) = (0.0, 0.0);
        if self.held[0] {
            r -= speed;
        }
        if self.held[1] {
            r += speed;
        }
        if self.held[2] {
            f += speed;
        }
        if self.held[3] {
            f -= speed;
        }
        if r != 0.0 || f != 0.0 {
            self.orbit.pan(r, f);
        }
        // Q/E turn smoothly.
        if let Some(goal) = self.orbit.yaw_goal {
            let k = 1.0 - (-dt * 9.0).exp();
            self.orbit.yaw += (goal - self.orbit.yaw) * k;
            if (goal - self.orbit.yaw).abs() < 0.05 {
                self.orbit.yaw = goal.rem_euclid(360.0);
                self.orbit.yaw_goal = None;
            }
        }
        self.hover = self.mouse.and_then(|(x, y)| pick(&self.orbit, x, y));
        actions
    }

    /// Adopts a camera an agent set since the last frame (`window_view`), then writes this
    /// frame's camera to the engine's slot.
    pub fn sync_camera(&mut self, slot: &mut Option<Camera>) {
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

/// Pan keys: WASD and the arrows → left, right, forward, back.
fn pan_key(code: u32) -> Option<usize> {
    match code {
        c if c == key('A') || c == VK_LEFT => Some(0),
        c if c == key('D') || c == VK_RIGHT => Some(1),
        c if c == key('W') || c == VK_UP => Some(2),
        c if c == key('S') || c == VK_DOWN => Some(3),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sepulcra::Content;

    #[test]
    fn adopting_a_camera_round_trips() {
        let o = Orbit { target: Vec3::new(1.0, -2.0, 3.0), yaw: 120.0, pitch: 30.0, dist: 50.0, yaw_goal: None, fov_y_deg: 45.0 };
        let back = Orbit::adopt(&o.camera());
        assert!((back.yaw - 120.0).abs() < 1e-3 && (back.pitch - 30.0).abs() < 1e-3 && (back.dist - 50.0).abs() < 1e-3);
    }

    #[test]
    fn keys_become_actions_and_clicks_pick_cells() {
        let content = Content::builtin();
        let geo = SlabGeo::new(&content.map);
        let mut c = Controls::default();
        let size = (1280, 720);
        // Point the mouse at the middle of the screen: the camera looks at the slab centre.
        let input = [
            Event::MouseMove { x: 640, y: 360 },
            Event::KeyDown { code: key('2'), repeat: false },
            Event::KeyDown { code: key('2'), repeat: true },
            Event::KeyDown { code: VK_SPACE, repeat: false },
            Event::MouseDown { button: MouseButton::Left, x: 640, y: 360 },
        ];
        let actions = c.frame(&input, 0.016, size, &geo, &content.map);
        assert_eq!(actions[0], Action::Select(Some(1)));
        assert_eq!(actions[1], Action::CallWave);
        assert!(matches!(actions[2], Action::Build(_)), "{actions:?}");
        assert!(c.hover.is_some());
        // Q turns to a face-centred view.
        c.frame(&[Event::KeyDown { code: key('Q'), repeat: false }], 0.016, size, &geo, &content.map);
        assert_eq!(c.orbit.yaw_goal, Some(0.0));
    }

    #[test]
    fn agents_window_view_is_adopted() {
        let mut c = Controls::default();
        let mut slot = None;
        c.sync_camera(&mut slot);
        let agent = Camera { eye: Vec3::new(0.0, 40.0, 1.0), target: Vec3::ZERO, fov_y_deg: 50.0 };
        slot = Some(agent);
        c.sync_camera(&mut slot);
        let now = slot.unwrap();
        assert!((now.eye - agent.eye).length() < 1e-3, "{now:?}");
        slot = None;
        c.sync_camera(&mut slot);
        assert!((slot.unwrap().eye - Orbit::default().camera().eye).length() < 1e-3);
    }
}
