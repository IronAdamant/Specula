//! Touch gestures from pointer contacts: tap, two-finger tap, long press, one-finger drag,
//! two-finger drag, pinch and rotate.
//!
//! A *session* runs from the first contact down to the last contact up. Taps are judged on the
//! whole session (short, still, never more than one / two fingers, not cancelled). A one-finger
//! drag starts once the contact leaves the tap slop circle, and only in sessions that never had a
//! second finger (lifting one finger after a pinch must not swing the camera). Two-finger values
//! come from the first two contacts and are reported as per-frame changes.

use crate::map::Settings;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Contact {
    pub id: u32,
    pub start: [f32; 2],
    pub pos: [f32; 2],
}

#[derive(Clone, Copy, Debug)]
struct Session {
    start_time: f64,
    max_contacts: usize,
    /// Some contact left its slop circle.
    moved: bool,
    long_fired: bool,
    cancelled: bool,
    dragging: bool,
}

#[derive(Clone, Copy, Debug)]
struct TwoRef {
    ids: (u32, u32),
    dist: f32,
    angle: f32,
    center: [f32; 2],
}

/// What the gestures produced this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct GestureOut {
    pub tap: bool,
    pub two_finger_tap: bool,
    pub long_press: bool,
    pub drag: [f32; 2],
    pub two_finger_drag: [f32; 2],
    pub pinch: f32,
    pub rotate: f32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Gestures {
    contacts: Vec<Contact>,
    session: Option<Session>,
    two: Option<TwoRef>,
    pub out: GestureOut,
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn len(v: [f32; 2]) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

/// Wraps an angle difference into `(-pi, pi]`.
fn wrap(mut a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    while a > PI {
        a -= TAU;
    }
    while a <= -PI {
        a += TAU;
    }
    a
}

impl Gestures {
    pub fn begin_frame(&mut self) {
        self.out = GestureOut::default();
    }

    pub fn contacts(&self) -> &[Contact] {
        &self.contacts
    }

    pub fn down(&mut self, id: u32, pos: [f32; 2], now: f64) {
        let session = self.session.get_or_insert(Session {
            start_time: now,
            max_contacts: 0,
            moved: false,
            long_fired: false,
            cancelled: false,
            dragging: false,
        });
        match self.contacts.iter_mut().find(|c| c.id == id) {
            Some(c) => *c = Contact { id, start: pos, pos },
            None => self.contacts.push(Contact { id, start: pos, pos }),
        }
        session.max_contacts = session.max_contacts.max(self.contacts.len());
        session.dragging = false;
        self.reset_two();
    }

    pub fn moved(&mut self, id: u32, pos: [f32; 2], settings: &Settings) {
        let Some(c) = self.contacts.iter_mut().find(|c| c.id == id) else { return };
        let last = c.pos;
        c.pos = pos;
        let left_slop = len(sub(pos, c.start)) > settings.tap_slop_px;
        let Some(session) = self.session.as_mut() else { return };
        session.moved |= left_slop;
        if self.contacts.len() == 1 && session.max_contacts == 1 {
            if session.dragging {
                let d = sub(pos, last);
                self.out.drag = [self.out.drag[0] + d[0], self.out.drag[1] + d[1]];
            } else if left_slop {
                // Start dragging from here: no jump by the slop distance.
                session.dragging = true;
            }
        }
        self.update_two();
    }

    /// A contact lifted (`cancelled`: abnormally, never a tap).
    pub fn up(&mut self, id: u32, pos: Option<[f32; 2]>, now: f64, cancelled: bool, settings: &Settings) {
        if let Some(pos) = pos {
            self.moved(id, pos, settings);
        }
        let Some(i) = self.contacts.iter().position(|c| c.id == id) else { return };
        self.contacts.remove(i);
        let Some(session) = self.session.as_mut() else { return };
        session.cancelled |= cancelled;
        if self.contacts.is_empty() {
            let s = *session;
            self.session = None;
            let quick = now - s.start_time <= f64::from(settings.tap_max_s);
            if quick && !s.moved && !s.long_fired && !s.cancelled {
                match s.max_contacts {
                    1 => self.out.tap = true,
                    2 => self.out.two_finger_tap = true,
                    _ => {}
                }
            }
        }
        self.reset_two();
    }

    /// Time-based gestures (long press). Call once per frame after the events.
    pub fn tick(&mut self, now: f64, settings: &Settings) {
        if let Some(s) = self.session.as_mut() {
            let still = !s.moved && !s.long_fired && !s.cancelled;
            if still && s.max_contacts == 1 && now - s.start_time >= f64::from(settings.long_press_s) {
                s.long_fired = true;
                self.out.long_press = true;
            }
        }
    }

    /// Forget every contact (focus lost): nothing fires.
    pub fn clear(&mut self) {
        self.contacts.clear();
        self.session = None;
        self.two = None;
    }

    fn two_now(&self) -> Option<TwoRef> {
        let [a, b] = [self.contacts.first()?, self.contacts.get(1)?];
        let d = sub(b.pos, a.pos);
        Some(TwoRef {
            ids: (a.id, b.id),
            dist: len(d),
            angle: d[1].atan2(d[0]),
            center: [(a.pos[0] + b.pos[0]) * 0.5, (a.pos[1] + b.pos[1]) * 0.5],
        })
    }

    fn reset_two(&mut self) {
        self.two = self.two_now();
    }

    fn update_two(&mut self) {
        let Some(now) = self.two_now() else { return };
        if let Some(prev) = self.two.filter(|p| p.ids == now.ids) {
            if prev.dist > 1.0 && now.dist > 1.0 {
                self.out.pinch += (now.dist / prev.dist).ln();
                self.out.rotate += wrap(now.angle - prev.angle);
            }
            let c = sub(now.center, prev.center);
            self.out.two_finger_drag = [self.out.two_finger_drag[0] + c[0], self.out.two_finger_drag[1] + c[1]];
        }
        self.two = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_keeps_angles_in_range() {
        use std::f32::consts::PI;
        assert!((wrap(1.5 * PI) + 0.5 * PI).abs() < 1e-5);
        assert!((wrap(-1.5 * PI) - 0.5 * PI).abs() < 1e-5);
        assert_eq!(wrap(0.25), 0.25);
    }

    #[test]
    fn three_finger_sessions_never_tap() {
        let s = Settings::default();
        let mut g = Gestures::default();
        g.begin_frame();
        for id in 0..3 {
            g.down(id, [10.0 * id as f32, 0.0], 0.0);
        }
        for id in 0..3 {
            g.up(id, None, 0.1, false, &s);
        }
        assert_eq!(g.out, GestureOut::default());
    }
}
