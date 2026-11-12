//! Sepulcra's cutscenes: the intro flyover of the slab before wave 1 and the victory shot, both
//! timelines in the assets (`timelines/intro.json`, `timelines/victory.json`) played by the
//! engine (docs/design/TIMELINE.md) through [`specula::game::Game::timeline_requests`].
//!
//! - **Presentation only.** Their tracks are a camera rail, subtitles, a sound cue and events:
//!   nothing they do writes the world or the simulation (the engine refuses entity tracks in a
//!   game's timeline), so `world_hash` never depends on them.
//! - **The intro holds the run.** Wave 1's countdown doesn't start until the flyover ends (the
//!   simulation isn't stepped, exactly as while paused), so the player keeps their build time.
//!   A replay is by simulation tick, so it verifies however long the intro ran.
//! - **Skippable by any input:** the engine ends it on a key, click, scroll, touch or gamepad button, and that
//!   input doesn't also act in the game. An agent's acting `play_*` call ends it too, so agents
//!   are never held. Headless runs, `--intro off` and reduced motion have no intro.
//! - **The camera rule:** both rails stay upright (never past 80 degrees of pitch, no roll) and
//!   keep the slab on screen (`timeline_check` proves it; `scripts/timeline_check.py`).

use specula::game::{TimelineEvent, TimelineRequest, TimelineWhat};

pub const INTRO: &str = "assets/timelines/intro.json";
pub const VICTORY: &str = "assets/timelines/victory.json";

/// The game's cutscene state.
#[derive(Debug, Default)]
pub struct Cutscenes {
    /// `--intro off` turns the intro off.
    pub intro_off: bool,
    /// The cutscene playing (its path), and whether it holds the simulation.
    playing: Option<(&'static str, bool)>,
    requests: Vec<TimelineRequest>,
    /// The input of the frame that skipped a cutscene goes nowhere else.
    swallow: bool,
    /// What happened, for play_state: (path, how) of the last ended.
    pub last_ended: Option<(String, String)>,
    pub played: u32,
}

impl Cutscenes {
    /// Whether the simulation waits for a cutscene.
    pub fn holds(&self) -> bool {
        self.playing.is_some_and(|(_, h)| h)
    }

    pub fn playing(&self) -> Option<&'static str> {
        self.playing.map(|(p, _)| p)
    }

    fn play(&mut self, path: &'static str, holds: bool) {
        self.requests.push(TimelineRequest::Play { path: path.to_owned(), window: true, skippable: true, restore: true });
        self.playing = Some((path, holds));
        self.played += 1;
    }

    /// The intro, at the start of a windowed run.
    pub fn intro(&mut self, windowed: bool, reduced_motion: bool) {
        if windowed && !self.intro_off && !reduced_motion {
            self.play(INTRO, true);
        }
    }

    /// The victory shot (windowed only).
    pub fn victory(&mut self, windowed: bool) {
        if windowed {
            self.play(VICTORY, false);
        }
    }

    /// An agent acted (a new run, a command, a control): the run is never held for it.
    pub fn agent_acted(&mut self) {
        if let Some((path, _)) = self.playing.take() {
            self.requests.push(TimelineRequest::Stop { path: path.to_owned() });
        }
    }

    pub fn take_requests(&mut self, out: &mut Vec<TimelineRequest>) {
        out.append(&mut self.requests);
    }

    /// Whether this frame's input was the skip (and so goes nowhere else).
    pub fn take_swallow(&mut self) -> bool {
        std::mem::take(&mut self.swallow)
    }

    /// The engine's word on a timeline; returns a line for the message log, if any. Subtitle
    /// texts and speakers that start with `@` are string-table keys (shown in the player's
    /// language); others are shown as written.
    pub fn event(&mut self, e: &TimelineEvent) -> Option<crate::l10n::Said> {
        use crate::l10n::Said;
        let words = |s: &str| match s.strip_prefix('@').filter(|k| !k.starts_with('@')) {
            Some(key) => Said::new(key),
            None => Said::new("common.text").text("text", s),
        };
        let ours = self.playing.is_some_and(|(p, _)| p == e.path);
        match &e.what {
            TimelineWhat::Ended { how, error } => {
                // `replaced`: the path plays on (played again, or the game's request took it over
                // from an agent), so the cutscene still holds.
                if ours && *how != "replaced" {
                    self.playing = None;
                }
                if *how == "skipped" {
                    self.swallow = true;
                }
                self.last_ended = Some((e.path.clone(), (*how).to_owned()));
                // A cutscene that couldn't start (a broken file) says so and the run goes on.
                error.as_ref().filter(|_| *how == "refused").map(|m| Said::new("toast.cutscene_skipped").text("error", m.as_str()))
            }
            TimelineWhat::Subtitle(line) => Some(match &line.speaker {
                Some(s) => Said::new("subtitle.line").said("speaker", words(s)).said("text", words(&line.text)),
                None => words(&line.text),
            }),
            _ => None,
        }
    }

    pub fn to_json(&self) -> specula_core::Json {
        use specula_core::Json;
        Json::obj([
            ("playing", self.playing.map_or(Json::Null, |(p, _)| Json::from(p))),
            ("holds_run", Json::from(self.holds())),
            ("intro", Json::from(if self.intro_off { "off" } else { "on" })),
            ("played", Json::from(self.played)),
            ("last_ended", self.last_ended.as_ref().map_or(Json::Null, |(p, h)| Json::obj([("path", Json::from(p.as_str())), ("how", Json::from(h.as_str()))]))),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula::game::TimelineEvent;

    fn ended(path: &str, how: &'static str) -> TimelineEvent {
        TimelineEvent { path: path.into(), t: 1.0, what: TimelineWhat::Ended { how, error: None } }
    }

    #[test]
    fn the_intro_holds_until_it_ends_and_agents_are_never_held() {
        let mut c = Cutscenes::default();
        c.intro(false, false);
        assert!(!c.holds(), "headless: no intro");
        c.intro(true, true);
        assert!(!c.holds(), "reduced motion: no flyover");
        c.intro(true, false);
        assert!(c.holds());
        let mut out = Vec::new();
        c.take_requests(&mut out);
        assert!(matches!(&out[0], TimelineRequest::Play { path, window: true, skippable: true, .. } if path == INTRO));
        // Skipped by the player: the run goes on, and that frame's input is swallowed.
        assert_eq!(c.event(&ended(INTRO, "skipped")), None);
        assert!(!c.holds());
        assert!(c.take_swallow() && !c.take_swallow());
        // An agent acting stops it.
        c.intro(true, false);
        c.agent_acted();
        assert!(!c.holds());
        out.clear();
        c.take_requests(&mut out);
        assert!(matches!(out.last(), Some(TimelineRequest::Stop { path }) if path == INTRO));
        // A broken file: refused, and said.
        c.intro(true, false);
        let e = TimelineEvent { path: INTRO.into(), t: 0.0, what: TimelineWhat::Ended { how: "refused", error: Some("bad".into()) } };
        assert!(crate::l10n::L10n::load(None, None).text(&c.event(&e).unwrap()).contains("bad"));
        assert!(!c.holds());
        // An agent was playing the intro's file: the game's request took it over, and the
        // agent's playback ending as replaced doesn't end the hold (lead review R2, minor 3).
        c.intro(true, false);
        assert_eq!(c.event(&ended(INTRO, "replaced")), None);
        assert!(c.holds(), "the intro plays on");
        assert_eq!(c.event(&ended(INTRO, "finished")), None);
        assert!(!c.holds());
        // The victory shot never holds the run.
        c.victory(true);
        assert!(!c.holds() && c.playing() == Some(VICTORY));
        let mut off = Cutscenes { intro_off: true, ..Cutscenes::default() };
        off.intro(true, false);
        assert!(!off.holds());
    }

    /// The shipped timelines parse, pass the engine's checks of the format, and hold no tracks
    /// that write the world (a game's timeline is presentation).
    #[test]
    fn the_shipped_timelines_are_presentation() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../sepulcra/assets/timelines");
        for name in ["intro.json", "victory.json"] {
            let text = std::fs::read_to_string(dir.join(name)).unwrap();
            let tl = specula_tempus::Timeline::parse_text(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(!tl.writes_world(), "{name}");
            assert!(tl.cameras().next().is_some(), "{name} has a camera");
            // Upright, smooth, no roll (the world-free part of timeline_check).
            let s = specula_tempus::check::sample(&tl, 0.0, tl.duration, 30.0, &|_| None);
            for c in specula_tempus::check::motion(&s, &tl.camera_cuts(), &specula_tempus::check::Limits::default()) {
                assert!(c.ok, "{name}: {}", c.message);
            }
        }
    }
}
