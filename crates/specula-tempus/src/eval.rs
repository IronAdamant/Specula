//! Evaluating a timeline at a time. Everything here is a **pure function of the time** (and, for
//! cameras looking at an entity, of where the world puts that entity): the state at `t` doesn't
//! depend on how playback got there, so scrubbing to `t` gives exactly what playing up to `t`
//! gives. Triggers that happen once (audio cues, events, a subtitle line starting) are listed
//! for a span of time with [`Timeline::fired`]; scrubbing jumps over them on purpose.

use crate::rail::{add, length, scale, sub, V3};
use crate::timeline::{CameraTrack, Cue, EntityTrack, GameEvent, Line, Look, Timeline, Trigger, TrackData};

/// Where the rail camera is and what it sees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub eye: V3,
    pub target: V3,
    pub fov_deg: f64,
    pub roll_deg: f64,
    /// The camera track in use (index into `tracks`).
    pub track: usize,
    /// False when the look target is an entity the world doesn't have (the camera then looks
    /// along its rail).
    pub look_resolved: bool,
}

impl Pose {
    /// The unit view direction.
    pub fn forward(&self) -> V3 {
        let d = sub(self.target, self.eye);
        let l = length(d);
        if l > 0.0 {
            scale(d, 1.0 / l)
        } else {
            [0.0, 0.0, -1.0]
        }
    }
}

/// An entity track's values at a time (each `None` when the track doesn't animate it).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EntityState {
    pub position: Option<V3>,
    pub rotation: Option<V3>,
    pub scale: Option<V3>,
    pub visible: Option<bool>,
    pub color: Option<V3>,
    pub emissive: Option<f64>,
}

/// Something that happens once, at its time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fired<'a> {
    Cue { track: usize, cue: &'a Cue },
    Event { track: usize, event: &'a GameEvent },
    Line { track: usize, line: &'a Line },
}

impl Fired<'_> {
    pub fn t(&self) -> f64 {
        match self {
            Fired::Cue { cue, .. } => cue.t,
            Fired::Event { event, .. } => event.t,
            Fired::Line { line, .. } => line.t,
        }
    }

    fn track(&self) -> usize {
        match self {
            Fired::Cue { track, .. } | Fired::Event { track, .. } | Fired::Line { track, .. } => *track,
        }
    }
}

/// The pose of one camera track at `t`. `resolve` gives an entity's centre by path.
pub fn pose_of(cam: &CameraTrack, track: usize, t: f64, resolve: &dyn Fn(&str) -> Option<V3>) -> Pose {
    let at = cam.moves.sample(t);
    let (eye, dir) = cam.rail.frame_at(at);
    let ahead = |d: f64| add(eye, scale(dir, d));
    let (target, look_resolved) = match &cam.look {
        Look::Point(p) => (*p, true),
        Look::Entity { path, offset } => match resolve(path) {
            Some(c) => (add(c, *offset), true),
            None => (ahead(10.0), false),
        },
        Look::Rail(r) => (r.point_at(at), true),
        Look::Ahead(d) => (ahead(*d), true),
    };
    // A target on the eye would leave no direction: look along the rail instead.
    let target = if length(sub(target, eye)) < 1e-4 { ahead(1.0) } else { target };
    Pose { eye, target, fov_deg: cam.fov.sample(t), roll_deg: cam.roll.sample(t), track, look_resolved }
}

/// The values of an entity track at `t`.
pub fn entity_at(e: &EntityTrack, t: f64) -> EntityState {
    EntityState {
        position: e.position.as_ref().map(|c| c.sample(t)),
        rotation: e.rotation.as_ref().map(|c| c.sample(t)),
        scale: e.scale.as_ref().map(|c| c.sample(t)),
        visible: e.visible.as_ref().map(|c| c.sample(t)),
        color: e.color.as_ref().map(|c| c.sample(t)),
        emissive: e.emissive.as_ref().map(|c| c.sample(t)),
    }
}

/// The trigger in force at `t`: the last key at or before it.
pub fn trigger_at(keys: &[Trigger], t: f64) -> Option<usize> {
    keys.partition_point(|k| k.t <= t).checked_sub(1)
}

impl Timeline {
    /// The camera track in use at `t`: the last listed whose span (first to last move key)
    /// holds `t`; before every span, the first to start (at its start); between or after spans,
    /// the last to end (at its end). Changing tracks is a **cut**.
    pub fn camera_track_at(&self, t: f64) -> Option<usize> {
        // No allocation: the window evaluates this every frame.
        let cams = || self.cameras().map(|(i, c)| (i, c.span()));
        if let Some((i, _)) = cams().filter(|(_, (a, b))| *a <= t && t <= *b).last() {
            return Some(i);
        }
        // The last to end (ties: the later listed), else the first to start (ties: the earlier).
        let ended = cams().filter(|(_, (_, b))| *b < t).fold(None, |best: Option<(usize, f64)>, (i, (_, b))| match best {
            Some((_, bb)) if bb > b => best,
            _ => Some((i, b)),
        });
        match ended {
            Some((i, _)) => Some(i),
            None => cams().fold(None, |best: Option<(usize, f64)>, (i, (a, _))| match best {
                Some((_, ba)) if ba <= a => best,
                _ => Some((i, a)),
            }).map(|(i, _)| i),
        }
    }

    /// The rail camera at `t` (`None` without a camera track).
    pub fn camera_at(&self, t: f64, resolve: &dyn Fn(&str) -> Option<V3>) -> Option<Pose> {
        let i = self.camera_track_at(t)?;
        let TrackData::Camera(c) = &self.tracks[i].data else { return None };
        Some(pose_of(c, i, t, resolve))
    }

    /// The subtitle lines on screen at `t`: `(track, line)`.
    pub fn lines_at(&self, t: f64) -> Vec<(usize, &Line)> {
        let mut out = Vec::new();
        for (i, tr) in self.tracks.iter().enumerate() {
            if let TrackData::Subtitles(lines) = &tr.data {
                out.extend(lines.iter().filter(|l| l.t <= t && t < l.t + l.duration).map(|l| (i, l)));
            }
        }
        out
    }

    /// What happens once in `(from, to]` (or `[from, to]` with `include_from`: the first span
    /// of a playback, so keys at its start fire), in time order, then track order.
    pub fn fired(&self, from: f64, to: f64, include_from: bool) -> Vec<Fired<'_>> {
        let inside = |t: f64| (t > from || (include_from && t >= from)) && t <= to;
        let mut out = Vec::new();
        for (i, tr) in self.tracks.iter().enumerate() {
            match &tr.data {
                TrackData::Audio(cues) => out.extend(cues.iter().filter(|c| inside(c.t)).map(|cue| Fired::Cue { track: i, cue })),
                TrackData::Events(events) => out.extend(events.iter().filter(|e| inside(e.t)).map(|event| Fired::Event { track: i, event })),
                TrackData::Subtitles(lines) => out.extend(lines.iter().filter(|l| inside(l.t)).map(|line| Fired::Line { track: i, line })),
                _ => {}
            }
        }
        // Stable: equal times keep track order, then key order.
        out.sort_by(|a, b| a.t().total_cmp(&b.t()).then(a.track().cmp(&b.track())));
        out
    }

    /// The times the camera cuts from one track to another, in `(0, duration]`.
    pub fn camera_cuts(&self) -> Vec<f64> {
        let mut starts: Vec<f64> = self.cameras().map(|(_, c)| c.span().0).filter(|t| *t > 0.0).collect();
        let ends: Vec<f64> = self.cameras().map(|(_, c)| c.span().1).collect();
        starts.extend(ends);
        starts.sort_by(f64::total_cmp);
        starts.dedup();
        let mut cuts = Vec::new();
        let mut eps_prev = self.camera_track_at(0.0);
        for t in starts {
            let after = self.camera_track_at(t + 1e-6);
            if after != eps_prev && t <= self.duration {
                cuts.push(t);
            }
            eps_prev = after;
        }
        cuts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::tests::sample;

    fn none(_: &str) -> Option<V3> {
        None
    }

    #[test]
    fn the_camera_follows_its_rail_and_keys() {
        let t = sample();
        let p0 = t.camera_at(0.0, &none).unwrap();
        assert_eq!(p0.eye, [0.0, 5.0, 40.0]);
        assert_eq!(p0.target, [0.0, 0.0, 0.0]);
        assert_eq!(p0.fov_deg, 55.0);
        assert_eq!(p0.roll_deg, 0.0, "roll stays zero unless asked for");
        let p10 = t.camera_at(10.0, &none).unwrap();
        assert!(length(sub(p10.eye, [20.0, 40.0, -30.0])) < 1e-6);
        assert_eq!(p10.fov_deg, 45.0);
        // Holds its last pose after its span.
        assert_eq!(t.camera_at(11.0, &none).unwrap().eye, p10.eye);
        // The same bits every evaluation.
        assert_eq!(t.camera_at(3.3, &none), t.camera_at(3.3, &none));
    }

    #[test]
    fn entities_and_triggers_are_functions_of_time() {
        let t = sample();
        let TrackData::Entity(gate) = &t.tracks[1].data else { panic!() };
        let s = entity_at(gate, 0.5);
        assert_eq!(s.visible, Some(false));
        assert_eq!(s.position, Some([0.0, 0.0, 0.0]), "before the first key: its value");
        assert_eq!(entity_at(gate, 1.0).visible, Some(true));
        let mid = entity_at(gate, 3.0).position.unwrap();
        assert!((mid[1] - 1.5).abs() < 1e-12, "in_out is halfway at the middle");
        assert_eq!(entity_at(gate, 99.0).position, Some([0.0, 3.0, 0.0]));
        assert_eq!(s.scale, None);
        let TrackData::Anim { keys, .. } = &t.tracks[2].data else { panic!() };
        assert_eq!(trigger_at(keys, 0.5), None);
        assert_eq!(trigger_at(keys, 1.0), Some(0));
        assert_eq!(trigger_at(keys, 7.0), Some(1));
    }

    #[test]
    fn once_only_things_fire_in_their_span() {
        let t = sample();
        let names = |f: &[Fired]| f.iter().map(|x| format!("{}@{}", match x {
            Fired::Cue { cue, .. } => cue.sound.as_str(),
            Fired::Event { event, .. } => event.name.as_str(),
            Fired::Line { .. } => "line",
        }, x.t())).collect::<Vec<_>>();
        assert_eq!(names(&t.fired(0.0, 1.0, true)), vec!["start@0", "bell@0.5", "line@1"]);
        assert_eq!(names(&t.fired(0.0, 1.0, false)), vec!["bell@0.5", "line@1"], "the start is fired only once");
        // Playing tick by tick fires each exactly once.
        let mut all = Vec::new();
        let hz = 30.0;
        for k in 0..400 {
            let (a, b) = (k as f64 / hz, (k + 1) as f64 / hz);
            all.extend(names(&t.fired(a, b, k == 0)));
        }
        assert_eq!(all, names(&t.fired(0.0, 20.0, true)));
        assert_eq!(t.lines_at(2.0).len(), 1);
        assert!(t.lines_at(4.0).is_empty(), "a line leaves after its duration");
    }

    #[test]
    fn several_cameras_cut_between_spans() {
        let text = r#"{"schema": "specula.timeline.v1", "tracks": [
          {"kind": "camera", "name": "a", "rail": {"points": [[0,0,0],[10,0,0]]}, "move": [{"t": 0, "at": 0}, {"t": 2, "at": 1}], "look": {"ahead": 5}},
          {"kind": "camera", "name": "b", "rail": {"points": [[0,50,0],[0,60,0]]}, "move": [{"t": 3, "at": 0}, {"t": 5, "at": 1}], "look": {"point": [0,0,0]}}]}"#;
        let t = Timeline::parse_text(text).unwrap();
        assert_eq!(t.camera_track_at(1.0), Some(0));
        assert_eq!(t.camera_track_at(2.5), Some(0), "between spans: the last to end holds");
        assert_eq!(t.camera_track_at(4.0), Some(1));
        assert_eq!(t.camera_track_at(9.0), Some(1));
        assert_eq!(t.camera_cuts(), vec![3.0]);
        // A camera looking at a missing entity looks along its rail, and says so.
        let e = r#"{"schema": "specula.timeline.v1", "tracks": [{"kind": "camera", "rail": {"points": [[0,0,0],[10,0,0]]}, "move": [{"t": 0, "at": 0}, {"t": 2, "at": 1}], "look": {"entity": "/gone"}}]}"#;
        let t = Timeline::parse_text(e).unwrap();
        let p = t.camera_at(1.0, &none).unwrap();
        assert!(!p.look_resolved);
        assert!(p.forward()[0] > 0.99);
        let p = t.camera_at(1.0, &|path| (path == "/gone").then_some([5.0, 10.0, 0.0])).unwrap();
        assert!(p.look_resolved && p.target == [5.0, 10.0, 0.0]);
    }
}
