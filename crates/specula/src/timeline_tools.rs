//! The timeline tools (`timeline_*`, docs/design/TIMELINE.md): timelines as project files
//! (`timeline_get`, `timeline_set`, `timeline_validate`), playback on the fixed tick
//! (`timeline_play`, `timeline_pause`, `timeline_scrub`, `timeline_stop`), a filmstrip from the
//! rail camera with sight packets (`timeline_preview`) and the rail checks (`timeline_check`:
//! the subject on screen and within size bounds, the camera never passing through geometry
//! (physics raycasts), no sudden jumps in position or rotation, never upside down).
//!
//! Previews and checks are step jobs (a tile or a slice of samples per step; the sheet is encoded
//! on a worker). Every argument is bounded in the schema; answers are bounded before they are built.

use super::*;
use crate::capture::Answer;
use crate::steps::{Step, StepJob, StepStart};
use crate::timeline::{camera_of, entity_centre, line_json, pose_json, round, Start};
use crate::tools::{bool_arg, check_keys, int_arg, string_arg};
use specula_anim::film;
use specula_core::math::Vec3;
use specula_protocol::ToolDef;
use specula_tempus::check::{self as tcheck, Limits, Sample};
use specula_tempus::{eval, Timeline, TrackData};
use std::collections::BTreeMap;

/// Problems `timeline_validate` lists at most.
const MAX_PROBLEMS: usize = 64;

/// A timeline checked off the engine thread: problems as data, its summary, and the entity paths
/// the world must answer for (`(track, path)`).
pub struct Checked {
    problems: Vec<Json>,
    summary: Json,
    targets: Vec<(usize, String)>,
    looks: Vec<(usize, String)>,
}

/// A parsed timeline and its anim and vfx problems.
type TimelineAndProblems = (Arc<Timeline>, Arc<Vec<(String, String)>>);

impl Checked {
    fn of(parsed: Result<TimelineAndProblems, specula_tempus::TimelineError>) -> Checked {
        match parsed {
            Err(e) => {
                let mut j = e.to_json();
                j.set("severity", Json::from("error"));
                Checked { problems: vec![j], summary: Json::Null, targets: Vec::new(), looks: Vec::new() }
            }
            Ok((tl, trig)) => {
                let problems = trig
                    .iter()
                    .take(MAX_PROBLEMS / 2)
                    .map(|(at, message)| Json::obj([("severity", Json::from("error")), ("error", Json::from("invalid_value")), ("field", Json::from(at.as_str())), ("message", Json::from(message.as_str()))]))
                    .collect();
                let targets = tl.tracks.iter().enumerate().filter_map(|(i, t)| t.target().map(|p| (i, p.to_owned()))).collect();
                let looks = tl
                    .cameras()
                    .filter_map(|(i, c)| match &c.look {
                        specula_tempus::Look::Entity { path, .. } => Some((i, path.clone())),
                        _ => None,
                    })
                    .collect();
                Checked { problems, summary: tl.summary(), targets, looks }
            }
        }
    }
}

/// Timelines `timeline_get` lists per page.
const LIST_LIMITS: (usize, usize) = (40, 200);
/// Preview frames and tile widths (height is 9/16 of the width).
pub const MAX_PREVIEW_FRAMES: u64 = 16;
const TILE_W: (u64, u64, u64) = (160, 320, 480);
/// Check samples per step (each a pose, a projection and two raycasts).
const CHECK_SLICE: usize = 128;
/// Failures listed per check.
const MAX_FAILURES: usize = 8;

pub enum TimelineCall {
    Get { path: Option<String>, at: Option<f64>, cursor: usize, limit: usize },
    /// A file in the project, or a definition checked on the caller's thread (format, the file
    /// size rule, anim and vfx values): the engine only looks its paths up.
    Validate { path: Option<String>, checked: Option<Box<Checked>> },
    Play { path: String, from: f64, paused: bool, window: Option<bool>, skippable: bool, restore: bool },
    Pause { path: String, paused: bool, force: bool },
    Scrub { path: String, t: f64, force: bool },
    Stop { path: String, force: bool },
    Preview { path: String, frames: u32, tile: u32, from: Option<f64>, to: Option<f64>, subject: Option<String> },
    Check(Box<CheckSpec>),
}

pub struct CheckSpec {
    path: String,
    subject: Option<String>,
    from: Option<f64>,
    to: Option<f64>,
    rate: f64,
    min_size: f64,
    max_size: f64,
    min_on_screen: f64,
    aspect: f64,
    limits: Limits,
}

pub fn call_class(call: &TimelineCall) -> schedule::Class {
    match call {
        TimelineCall::Preview { .. } => schedule::Class::Render,
        TimelineCall::Play { .. } | TimelineCall::Pause { .. } | TimelineCall::Scrub { .. } | TimelineCall::Stop { .. } => schedule::Class::Edit,
        TimelineCall::Get { .. } | TimelineCall::Validate { .. } | TimelineCall::Check(_) => schedule::Class::Read,
    }
}

fn error(code: &str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_string())
}

/// `intro` or `assets/timelines/intro.json` → the project path.
pub(crate) fn path_arg(args: &Json) -> Result<String, String> {
    let p = string_arg(args, "path")?.ok_or("'path' is required: a timeline's name (\"intro\": assets/timelines/intro.json) or its project path")?;
    timeline_path(p)
}

pub(crate) fn timeline_path(p: &str) -> Result<String, String> {
    let bad = || format!("'path' must be a name of 1..64 of A-Z a-z 0-9 _ - (assets/timelines/<name>.json) or a project path ending in /timelines/<name>.json (got '{}')", crate::bounds::echo(p));
    let name_ok = |n: &str| !n.is_empty() && n.len() <= 64 && n.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'));
    if name_ok(p) {
        return Ok(format!("assets/timelines/{p}.json"));
    }
    if p.len() > 256 || !p.starts_with("assets/") || p.contains('\\') || p.split('/').any(|s| s.is_empty() || s == "." || s == "..") {
        return Err(bad());
    }
    let mut parts = p.rsplit('/');
    let file = parts.next().unwrap_or("");
    let folder = parts.next().unwrap_or("");
    match file.strip_suffix(".json") {
        Some(stem) if folder == "timelines" && name_ok(stem) => Ok(p.to_owned()),
        _ => Err(bad()),
    }
}

fn num(args: &Json, key: &str, lo: f64, hi: f64) -> Result<Option<f64>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => match v.as_f64() {
            Some(x) if x.is_finite() && (lo..=hi).contains(&x) => Ok(Some(x)),
            _ => Err(format!("'{key}' must be a number from {lo} to {hi}")),
        },
    }
}

fn force(args: &Json) -> Result<bool, String> {
    Ok(bool_arg(args, "force")?.unwrap_or(false))
}

pub fn parse(name: &str, args: &Json) -> Option<Result<TimelineCall, String>> {
    let max_t = specula_tempus::timeline::MAX_DURATION;
    let r = match name {
        "timeline_get" => (|| {
            check_keys(args, name, &[&["path", "at", "cursor", "limit"]])?;
            let path = match string_arg(args, "path")? {
                Some(p) => Some(timeline_path(p)?),
                None => None,
            };
            let at = num(args, "at", 0.0, max_t)?;
            if at.is_some() && path.is_none() {
                return Err("'at' evaluates one timeline: give 'path' too".into());
            }
            let cursor = int_arg(args, "cursor", 0, 1 << 20)?.unwrap_or(0) as usize;
            let limit = int_arg(args, "limit", 1, LIST_LIMITS.1 as u64)?.map_or(LIST_LIMITS.0, |l| l as usize);
            Ok(TimelineCall::Get { path, at, cursor, limit })
        })(),
        "timeline_validate" => (|| {
            check_keys(args, name, &[&["path", "definition"]])?;
            let path = string_arg(args, "path")?.map(timeline_path).transpose()?;
            let definition = args.get("definition").filter(|d| !d.is_null());
            if path.is_some() == definition.is_some() {
                return Err("give 'path' (a timeline in the project) or 'definition' (a timeline object), not both".into());
            }
            // Checked here, on the caller's thread, by the file rule (its saved text, at most
            // 256 KiB: lead review, a 1.8 MB definition validated that could never be saved);
            // the engine only looks its paths up.
            let checked = definition.map(|d| {
                let text = d.to_pretty();
                Box::new(Checked::of(Timeline::parse_text(&text).map(|tl| {
                    let problems = Arc::new(crate::timeline::trigger_problems(&tl));
                    (Arc::new(tl), problems)
                })))
            });
            Ok(TimelineCall::Validate { path, checked })
        })(),
        "timeline_play" => (|| {
            check_keys(args, name, &[&["path", "from", "paused", "window", "skippable", "restore"]])?;
            Ok(TimelineCall::Play {
                path: path_arg(args)?,
                from: num(args, "from", 0.0, max_t)?.unwrap_or(0.0),
                paused: bool_arg(args, "paused")?.unwrap_or(false),
                window: bool_arg(args, "window")?,
                skippable: bool_arg(args, "skippable")?.unwrap_or(true),
                restore: bool_arg(args, "restore")?.unwrap_or(true),
            })
        })(),
        "timeline_pause" => (|| {
            check_keys(args, name, &[&["path", "paused", "force"]])?;
            Ok(TimelineCall::Pause { path: path_arg(args)?, paused: bool_arg(args, "paused")?.unwrap_or(true), force: force(args)? })
        })(),
        "timeline_scrub" => (|| {
            check_keys(args, name, &[&["path", "t", "force"]])?;
            let t = num(args, "t", 0.0, max_t)?.ok_or("'t' is required: the time to move to, seconds")?;
            Ok(TimelineCall::Scrub { path: path_arg(args)?, t, force: force(args)? })
        })(),
        "timeline_stop" => (|| {
            check_keys(args, name, &[&["path", "force"]])?;
            Ok(TimelineCall::Stop { path: path_arg(args)?, force: force(args)? })
        })(),
        "timeline_preview" => (|| {
            check_keys(args, name, &[&["path", "frames", "tile", "from", "to", "subject"]])?;
            let from = num(args, "from", 0.0, max_t)?;
            let to = num(args, "to", 0.0, max_t)?;
            if let (Some(a), Some(b)) = (from, to) {
                if b < a {
                    return Err("'to' comes after 'from'".into());
                }
            }
            Ok(TimelineCall::Preview {
                path: path_arg(args)?,
                frames: int_arg(args, "frames", 2, MAX_PREVIEW_FRAMES)?.unwrap_or(8) as u32,
                tile: int_arg(args, "tile", TILE_W.0, TILE_W.2)?.unwrap_or(TILE_W.1) as u32,
                from,
                to,
                subject: subject_arg(args)?,
            })
        })(),
        "timeline_check" => (|| {
            check_keys(args, name, &[&["path", "subject", "from", "to", "rate", "min_size", "max_size", "min_on_screen", "aspect", "max_speed", "max_jump", "max_turn", "max_turn_jump", "max_pitch", "max_roll"]])?;
            let d = Limits::default();
            let from = num(args, "from", 0.0, max_t)?;
            let to = num(args, "to", 0.0, max_t)?;
            if let (Some(a), Some(b)) = (from, to) {
                if b < a {
                    return Err("'to' comes after 'from'".into());
                }
            }
            let min_size = num(args, "min_size", 0.0, 1.0)?.unwrap_or(0.05);
            let max_size = num(args, "max_size", 0.0, 4.0)?.unwrap_or(1.0);
            if max_size < min_size {
                return Err("'max_size' is at least 'min_size'".into());
            }
            Ok(TimelineCall::Check(Box::new(CheckSpec {
                path: path_arg(args)?,
                subject: subject_arg(args)?,
                from,
                to,
                rate: num(args, "rate", 1.0, 120.0)?.unwrap_or(30.0),
                min_size,
                max_size,
                min_on_screen: num(args, "min_on_screen", 0.0, 1.0)?.unwrap_or(1.0),
                aspect: num(args, "aspect", 0.25, 4.0)?.unwrap_or(16.0 / 9.0),
                limits: Limits {
                    max_speed: num(args, "max_speed", 0.0, 10_000.0)?.unwrap_or(d.max_speed),
                    max_accel: num(args, "max_jump", 0.0, 100_000.0)?.unwrap_or(d.max_accel),
                    max_turn: num(args, "max_turn", 0.0, 100_000.0)?.unwrap_or(d.max_turn),
                    max_turn_accel: num(args, "max_turn_jump", 0.0, 1_000_000.0)?.unwrap_or(d.max_turn_accel),
                    max_pitch: num(args, "max_pitch", 0.0, 90.0)?.unwrap_or(d.max_pitch),
                    max_roll: num(args, "max_roll", 0.0, 180.0)?.unwrap_or(d.max_roll),
                },
            })))
        })(),
        _ => return None,
    };
    Some(r)
}

fn subject_arg(args: &Json) -> Result<Option<String>, String> {
    match string_arg(args, "subject")? {
        None => Ok(None),
        Some(s) if s.starts_with('/') && s.len() <= 256 && !s.contains("//") => Ok(Some(s.to_owned())),
        Some(_) => Err("'subject' must be an entity path such as \"/slab\" (at most 256 bytes)".into()),
    }
}

/// `timeline_set` on the caller's connection thread: the timeline object is checked here (its
/// structured error answers at once), then written as a project file through `project_write`
/// (claims, `expect_version`, the journal, disk sync, undo).
pub(crate) fn set_tool(tools: &crate::tools::Tools, session: &SessionInfo, args: &Json) -> ToolOutput {
    let parsed = (|| {
        check_keys(args, "timeline_set", &[&["path", "timeline", "expect_version"]])?;
        let path = path_arg(args)?;
        let t = args.get("timeline").filter(|t| !t.is_null()).ok_or("'timeline' is required: the whole timeline object (timeline_get shows the format; project_patch edits one in place)")?;
        Ok::<_, String>((path, t.clone()))
    })();
    let (path, timeline) = match parsed {
        Ok(p) => p,
        Err(m) => return error("invalid_argument", m),
    };
    let text = timeline.to_pretty();
    if let Err(e) = Timeline::parse_text(&text) {
        return ToolOutput::error(e.to_json().to_string());
    }
    let mut w = Json::obj([("path", Json::from(path.as_str())), ("text", Json::from(text))]);
    if let Some(v) = args.get("expect_version").filter(|v| !v.is_null()) {
        w.set("expect_version", v.clone());
    }
    crate::project_tools::call(tools, session, "project_write", &w)
}

pub fn definitions() -> Vec<ToolDef> {
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::arr(required.iter().map(|r| Json::from(*r))))])
    };
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let path = || ("path", t("string", "The timeline: a name (\"intro\" is assets/timelines/intro.json) or its project path"));
    let force = || ("force", t("boolean", "Act on another agent's (or the game's) playback: needs lead authority"));
    vec![
        ToolDef {
            name: "timeline_get".into(),
            description: "Timelines (cutscenes, sequences: project files assets/timelines/*.json, kind timeline) and what plays. \
                          Without 'path': a page of the project's timelines {path, version, duration, tracks, problems} and the \
                          playbacks (state playing|paused|blocked, t, who, window, camera, subtitles) with the last ended. With \
                          'path': its tracks {name, kind, target, keys, span}, its playback, and with 'at' its state at that time \
                          (the rail camera, entity values, the anim and vfx keys in force, subtitles), evaluated without playing. \
                          The format: {schema: \"specula.timeline.v1\", description?, duration?, tracks: [camera {rail: {points \
                          [[x,y,z]...], closed?}, move: [{t, at (0..1 of the rail's arc length), ease?}], look: {point} | {entity, \
                          offset?} | {rail} | {ahead: m}, fov?: number | [{t, value}], roll?: (0 unless asked)} | entity {target \
                          \"/path\", position | rotation | scale | visible | color | emissive: [{t, value, ease?}]} | anim {target, \
                          keys [{t, anim: world_set's anim value}]} | vfx {target, keys [{t, vfx}]} | audio {cues [{t, sound, \
                          volume?, at?, bus?}]} | subtitles {lines [{t, duration, text, speaker?}]} | events {events [{t, name, \
                          data?}]}]}. Eases: linear (default), in, out, in_out, smooth, step. Several camera tracks cut between \
                          their spans."
                .into(),
            input_schema: schema(
                vec![
                    path(),
                    ("at", t("number", "Evaluate the timeline at this time, seconds (needs path)")),
                    ("cursor", t("integer", "next_cursor from the previous page")),
                    ("limit", t("integer", "Timelines per page, 1..200 (default 40)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "timeline_set".into(),
            description: "Writes a whole timeline (the timeline_get format) as the project file assets/timelines/<name>.json, \
                          validated first (structured errors name the field and what is allowed), then through project_write: claims \
                          on /project/<path>, expect_version (0: must not exist yet), the journal, disk and world_undo. Small edits: \
                          project_patch {path, ops: [{set: \"tracks[0].move[1].t\", value}]}."
                .into(),
            input_schema: schema(vec![path(), ("timeline", t("object", "The timeline object")), ("expect_version", t("integer", "Only if the file is at this version (0: it must not exist)"))], &["path", "timeline"]),
        },
        ToolDef {
            name: "timeline_validate".into(),
            description: "Checks a timeline without playing it: the format, anim and vfx values as world_set takes them, and \
                          against the world now: entities its tracks write (missing ones refuse play) and entities cameras look at \
                          (missing ones look along the rail). Answers {ok, problems [{severity, code, at, message}], summary}."
                .into(),
            input_schema: schema(vec![path(), ("definition", t("object", "A timeline object to check instead of a file"))], &[]),
        },
        ToolDef {
            name: "timeline_play".into(),
            description: "Plays a timeline on the engine's fixed tick (it moves only while the clock runs; time_control steps it). \
                          Entity, anim and vfx tracks write the world as you, through the write policy: claims hold (a claim \
                          refusing a write pauses it as 'blocked', named in timeline_get and the trace), one journal entry per tick \
                          that changes something, undoable. Its state at the start is written by this call. Events, audio cues and \
                          subtitles reach the hosted game on their tick. window (default true with a window open): the human's \
                          window looks through the rail camera until it ends; a human's input skips it when skippable (default \
                          true), else just takes the view back; restore (default true) puts the window's camera back after. Playing \
                          a timeline you already play restarts it; the hosted game's own request for it takes it over (yours ends as 'replaced'). \
                          At most 4 play at once (2 each), and two never write one entity."
                .into(),
            input_schema: schema(
                vec![
                    path(),
                    ("from", t("number", "Start at this time, seconds (default 0)")),
                    ("paused", t("boolean", "Start paused (scrub, then timeline_pause {paused: false})")),
                    ("window", t("boolean", "Drive the human's window camera (default: true when there is a window)")),
                    ("skippable", t("boolean", "A human's input ends it (default true)")),
                    ("restore", t("boolean", "Put the window camera back when it ends (default true)")),
                ],
                &["path"],
            ),
        },
        ToolDef {
            name: "timeline_pause".into(),
            description: "Pauses a playback (paused: false resumes it, and clears 'blocked' to try its writes again). Yours, or \
                          another's with force (lead)."
                .into(),
            input_schema: schema(vec![path(), ("paused", t("boolean", "true pauses (default), false resumes")), force()], &["path"]),
        },
        ToolDef {
            name: "timeline_scrub".into(),
            description: "Moves a playback to time t (rounded to a tick) and writes its state there as you (one commit): the same \
                          state playing to t gives (values are a function of time; anim and vfx keys start at their own times). \
                          Events, cues and subtitles in between don't fire. Without a playback it makes a paused one at t."
                .into(),
            input_schema: schema(vec![path(), ("t", t("number", "Seconds")), force()], &["path", "t"]),
        },
        ToolDef {
            name: "timeline_stop".into(),
            description: "Stops a playback: entities keep the values it last wrote; the window gets its camera back; the game hears \
                          Ended {how: stopped}."
                .into(),
            input_schema: schema(vec![path(), force()], &["path"]),
        },
        ToolDef {
            name: "timeline_preview".into(),
            description: "A filmstrip from the timeline's rail camera: frames evenly over from..to (default its whole duration), \
                          the world as it is now seen from where the camera is at each time (entity tracks aren't applied: play or \
                          scrub to see them), one PNG sheet plus a packet saved in the captures folder like sight captures. Per \
                          frame: t, the camera track and pose, the subject's screen box, visible_px and on_screen (from the ID \
                          buffer), and the largest subjects drawn. Rendered a tile per step between frames."
                .into(),
            input_schema: schema(
                vec![
                    path(),
                    ("frames", t("integer", "Frames, 2..16 (default 8)")),
                    ("tile", t("integer", "Tile width in pixels, 160..480 (default 320; height 9/16 of it)")),
                    ("from", t("number", "First frame's time, seconds")),
                    ("to", t("number", "Last frame's time, seconds")),
                    ("subject", t("string", "An entity path to report on per frame (default: the camera's look-at entity)")),
                ],
                &["path"],
            ),
        },
        ToolDef {
            name: "timeline_check".into(),
            description: "Rail checks, measured on the camera path sampled at 'rate' (default 30/s): subject_on_screen (the \
                          subject's box centre in frame for at least min_on_screen of the samples, default all), subject_size (its \
                          box height as a fraction of the view's, min_size..max_size, default 0.05..1), clearance (physics \
                          raycasts both ways along every step of the camera: it never passes through a collider; the hit entity is \
                          named), line_of_sight (warning: colliders between the camera and the subject), speed (m/s), jump \
                          (m/s2: steps and kinks), turn_rate (deg/s), turn_jump (deg/s2), upright (pitch never past max_pitch, \
                          default 80: the camera never flips), roll (default 0.5 deg) and horizon_spin (deg/s the picture turns, max_turn). Cuts between camera tracks aren't \
                          measured across. Answers {ok, errors, warnings, checks [{check, ok, value, limit, unit, at_t, \
                          message}]}. Colliders only: geometry without one isn't seen (colliders_considered says how many)."
                .into(),
            input_schema: schema(
                vec![
                    path(),
                    ("subject", t("string", "The entity the shot is about (default: the camera's look-at entity)")),
                    ("from", t("number", "Seconds")),
                    ("to", t("number", "Seconds")),
                    ("rate", t("number", "Samples per second, 1..120 (default 30; at most 4096 samples)")),
                    ("min_size", t("number", "Smallest subject height, fraction of the view (default 0.05)")),
                    ("max_size", t("number", "Largest subject height, fraction of the view (default 1)")),
                    ("min_on_screen", t("number", "Fraction of samples the subject must be in frame (default 1)")),
                    ("aspect", t("number", "View width / height (default 16/9)")),
                    ("max_speed", t("number", "m/s (default 40)")),
                    ("max_jump", t("number", "m/s2 (default 80)")),
                    ("max_turn", t("number", "deg/s (default 90)")),
                    ("max_turn_jump", t("number", "deg/s2 (default 720)")),
                    ("max_pitch", t("number", "deg (default 80)")),
                    ("max_roll", t("number", "deg (default 0.5)")),
                ],
                &["path"],
            ),
        },
    ]
}

/// A timeline's summary for listings (and its problems, when it doesn't parse).
///
/// Never parses on the engine thread (review: 40 files of 255 KB took 1.4 s): the summary comes
/// from timelines already parsed (when their files were validated, or played); others list
/// without one (`timeline_get {path}` or `timeline_validate` reads them).
fn file_line(path: &str, version: u64, text: &str, hash: u64) -> Json {
    let mut j = Json::obj([("path", Json::from(path)), ("version", Json::from(version)), ("bytes", Json::from(text.len()))]);
    match crate::timeline::summary_of(hash, text.len()) {
        Some(tl) => {
            let kinds: std::collections::BTreeSet<&str> = tl.tracks.iter().map(|t| t.kind()).collect();
            j.set("duration", Json::from(round(tl.duration, 3)));
            j.set("tracks", Json::from(tl.tracks.len()));
            j.set("kinds", Json::arr(kinds.into_iter().map(Json::from)));
            j.set("writes_world", Json::from(tl.writes_world()));
        }
        None => {
            j.set("summary", Json::from("not parsed yet: timeline_get {path} reads it"));
        }
    }
    j
}

impl Engine {
    pub(crate) fn timeline_call(&mut self, call: TimelineCall, session: &SessionInfo, w: Writer) -> ToolOutput {
        let _z = profile::zone("timeline_tool");
        let result = match call {
            TimelineCall::Get { path, at, cursor, limit } => self.timeline_get(path.as_deref(), at, cursor, limit),
            TimelineCall::Validate { path, checked } => self.timeline_validate(path.as_deref(), checked),
            TimelineCall::Play { path, from, paused, window, skippable, restore } => {
                let start = Start { path, from_s: from, paused, window: window.unwrap_or(self.windowed), skippable, restore, by: w.name.to_owned(), key: w.session.unwrap_or(w.name).to_owned(), from_game: false };
                self.timeline_start(start).map(|j| ToolOutput::text(j.to_string()))
            }
            TimelineCall::Pause { path, paused, force } => self.timeline_owned(&path, force, session, w).map(|i| {
                self.timeline_set_paused(i, paused);
                ToolOutput::text(self.timeline_status_of(i).to_string())
            }),
            TimelineCall::Scrub { path, t, force } => {
                if self.timeline.playing.iter().all(|p| p.path != path) {
                    // A paused playback made at t, the caller's: its start writes the state at t
                    // (the call's one commit).
                    let start = Start { path, from_s: t, paused: true, window: false, skippable: true, restore: true, by: w.name.to_owned(), key: w.session.unwrap_or(w.name).to_owned(), from_game: false };
                    self.timeline_start(start).map(|j| ToolOutput::text(j.to_string()))
                } else {
                    match self.timeline_owned(&path, force, session, w) {
                        Ok(i) => self.timeline_scrub(i, t, w).map(|j| ToolOutput::text(j.to_string())),
                        Err(e) => Err(e),
                    }
                }
            }
            TimelineCall::Stop { path, force } => self.timeline_owned(&path, force, session, w).map(|i| {
                let j = self.timeline_end(i, "stopped", None);
                self.timeline_deliver();
                ToolOutput::text(j.to_string())
            }),
            TimelineCall::Preview { .. } | TimelineCall::Check(_) => Ok(error("internal", "timeline_preview and timeline_check run as step jobs")),
        };
        result.unwrap_or_else(|e| world_error(&e))
    }

    fn timeline_status_of(&self, i: usize) -> Json {
        let mut s = self.timeline_status();
        if let Some(p) = s.get("playing").and_then(Json::as_array).and_then(|a| a.get(i)).cloned() {
            s = p;
        }
        s
    }

    /// The playback at `path`, if the caller may change it: its starter, or anyone with lead
    /// authority and `force`.
    fn timeline_owned(&mut self, path: &str, force: bool, session: &SessionInfo, w: Writer) -> Result<usize, WorldError> {
        let Some(i) = self.timeline.playing.iter().position(|p| p.path == path) else {
            let playing: Vec<Json> = self.timeline.playing.iter().map(|p| Json::from(p.path.as_str())).collect();
            return Err(WorldError::new("not_playing", format!("{path} isn't playing (timeline_play starts it)")).with("playing", Json::Arr(playing)));
        };
        let p = &self.timeline.playing[i];
        let mine = w.session.unwrap_or(w.name) == p.key;
        if mine {
            return Ok(i);
        }
        if !force {
            return Err(crate::authority::refusal(crate::authority::Site::JobBusy, format!("{path} is {}'s playback: ask them, or pass force (lead authority)", p.by)).with("owner", p.by.clone()));
        }
        crate::authority::may_force(crate::authority::authority(&self.swarm, &session.id), false, &format!("changing {}'s playback of {path}", p.by))?;
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "timeline.forced", path = path, owner = p.by.as_str(), by = w.name);
        Ok(i)
    }

    fn timeline_get(&mut self, path: Option<&str>, at: Option<f64>, cursor: usize, limit: usize) -> Result<ToolOutput, WorldError> {
        let hz = f64::from(self.clock.hz());
        let Some(path) = path else {
            let files: Vec<(&String, &crate::project::ProjectFile)> = self.world.files().iter().filter(|(_, f)| f.kind == crate::project::FileKind::Timeline).collect();
            let total = files.len();
            let page: Vec<Json> = files.iter().skip(cursor).take(limit).map(|(p, f)| file_line(p, f.version, &f.text, f.hash)).collect();
            let next = (cursor + page.len() < total).then_some(cursor + page.len());
            let out = Json::obj([
                ("total", Json::from(total)),
                ("timelines", Json::Arr(page)),
                ("next_cursor", next.map_or(Json::Null, Json::from)),
                ("player", self.timeline_status()),
            ]);
            return Ok(ToolOutput::text(out.to_string()));
        };
        let (tl, version) = self.timeline_load(path)?;
        let mut out = Json::obj([("path", Json::from(path)), ("version", Json::from(version))]);
        if !tl.description.is_empty() {
            out.set("description", Json::from(tl.description.as_str()));
        }
        let summary = tl.summary();
        for (k, v) in summary.as_object().unwrap_or_default() {
            out.set(k.as_str(), v.clone());
        }
        if let Some(p) = self.timeline.playing.iter().find(|p| p.path == path) {
            let _ = p;
            let i = self.timeline.playing.iter().position(|p| p.path == path).unwrap_or(0);
            out.set("playback", self.timeline_status_of(i));
        }
        if let Some(t) = at {
            let world = &self.world;
            let resolve = |p: &str| entity_centre(world, p);
            let mut state = Json::obj([("t", Json::from(t)), ("tick", Json::from(round(t * hz, 3)))]);
            if let Some(pose) = tl.camera_at(t, &resolve) {
                state.set("camera", pose_json(&pose, &tl));
            }
            let mut tracks = Vec::new();
            for tr in &tl.tracks {
                match &tr.data {
                    TrackData::Entity(e) => {
                        let s = eval::entity_at(e, t);
                        let mut j = Json::obj([("track", Json::from(tr.name.as_str())), ("target", Json::from(e.target.as_str()))]);
                        let v = |a: [f64; 3]| Json::arr(a.iter().map(|c| Json::from(round(*c, 4))));
                        if let Some(p) = s.position {
                            j.set("position", v(p));
                        }
                        if let Some(r) = s.rotation {
                            j.set("rotation", v(r));
                        }
                        if let Some(sc) = s.scale {
                            j.set("scale", v(sc));
                        }
                        if let Some(b) = s.visible {
                            j.set("visible", Json::from(b));
                        }
                        if let Some(c) = s.color {
                            j.set("color", v(c));
                        }
                        if let Some(em) = s.emissive {
                            j.set("emissive", Json::from(round(em, 4)));
                        }
                        tracks.push(j);
                    }
                    TrackData::Anim { target, keys } | TrackData::Vfx { target, keys } => {
                        let k = eval::trigger_at(keys, t);
                        tracks.push(Json::obj([
                            ("track", Json::from(tr.name.as_str())),
                            ("target", Json::from(target.as_str())),
                            ("key", k.map_or(Json::Null, Json::from)),
                            ("value", k.map_or(Json::Null, |k| keys[k].value.clone())),
                        ]));
                    }
                    _ => {}
                }
            }
            state.set("tracks", Json::Arr(tracks));
            state.set("subtitles", Json::arr(tl.lines_at(t).into_iter().take(8).map(|(_, l)| line_json(l))));
            out.set("at", state);
        }
        Ok(ToolOutput::text(out.to_string()))
    }

    /// What validation found, plus the world's say now: missing targets and look-at entities.
    /// The format and anim/vfx checks ran off the engine thread (a definition on the caller's
    /// thread; a file when it was validated or read: lead review, a 1.8 MB definition held the
    /// engine thread 52 ms); only path lookups run here, at most 64 problems.
    fn timeline_validate(&mut self, path: Option<&str>, checked: Option<Box<Checked>>) -> Result<ToolOutput, WorldError> {
        let (checked, name) = match (path, checked) {
            (Some(p), _) => match self.world.files().get(p) {
                Some(f) => (Checked::of(crate::timeline::ready(&Arc::clone(&f.text), f.hash).map(|r| (r.tl, r.problems))), p.to_owned()),
                None => return Err(WorldError::new("not_found", format!("no file at {p} (timeline_get lists timelines)"))),
            },
            (None, Some(c)) => (*c, "(definition)".to_owned()),
            (None, None) => return Err(WorldError::new("invalid_argument", "give path or definition")),
        };
        let Checked { mut problems, summary, targets, looks } = checked;
        for (i, target) in targets.iter().take(MAX_PROBLEMS) {
            if problems.len() >= MAX_PROBLEMS {
                break;
            }
            if self.world.find_path(target).is_err() {
                problems.push(Json::obj([
                    ("severity", Json::from("warning")),
                    ("error", Json::from("target_missing")),
                    ("field", Json::from(format!("tracks[{i}].target"))),
                    ("message", Json::from(format!("{target} isn't in the world now: timeline_play refuses until it is"))),
                ]));
            }
        }
        for (i, path) in looks.iter().take(MAX_PROBLEMS) {
            if problems.len() >= MAX_PROBLEMS {
                break;
            }
            if self.world.find_path(path).is_err() {
                problems.push(Json::obj([
                    ("severity", Json::from("warning")),
                    ("error", Json::from("look_missing")),
                    ("field", Json::from(format!("tracks[{i}].look.entity"))),
                    ("message", Json::from(format!("{path} isn't in the world now: the camera would look along its rail"))),
                ]));
            }
        }
        let errors = problems.iter().filter(|p| p.get("severity").and_then(Json::as_str) == Some("error")).count();
        let out = Json::obj([("timeline", Json::from(name)), ("ok", Json::from(errors == 0)), ("errors", Json::from(errors)), ("problems", Json::Arr(problems)), ("summary", summary)]);
        Ok(ToolOutput::text(out.to_string()))
    }

    /// `timeline_preview` and `timeline_check`: set up now (refusals answered at once), then
    /// run in steps.
    pub(crate) fn start_timeline_job(&mut self, request: Request) {
        let Request { command: Command::Timeline(call), session, reply, deadline, call: call_id, .. } = request else { unreachable!("only timeline jobs come here") };
        let owner = crate::jobs::JobOwner { name: self.swarm.agent_name(&session), key: self.swarm.key(&session) };
        let start = |class| StepStart { owner: owner.clone(), session: session.id.clone(), call: call_id, reply: reply.clone(), deadline, class };
        let job: Result<(Box<dyn StepJob>, schedule::Class), ToolOutput> = match call {
            TimelineCall::Preview { path, frames, tile, from, to, subject } => {
                let tile_h = (tile * 9 / 16).max(64);
                let (cols, rows) = crate::orbit::grid(frames);
                let bytes = u64::from(cols * tile) * u64::from(rows * (tile_h + LABEL_H)) * 4;
                if let Err(e) = self.workers_room(&owner, call_id, bytes) {
                    reply.refuse(&e);
                    return;
                }
                PreviewJob::new(self, &path, frames, tile, tile_h, from, to, subject, &session, call_id).map(|j| (Box::new(j) as Box<dyn StepJob>, schedule::Class::Render))
            }
            TimelineCall::Check(spec) => CheckJob::new(self, *spec).map(|j| (Box::new(j) as Box<dyn StepJob>, schedule::Class::Read)),
            _ => unreachable!("only preview and check are jobs"),
        };
        match job {
            Ok((job, class)) => self.step_job(start(class), job),
            Err(out) => {
                let _ = reply.send(out);
            }
        }
    }
}

/// The label strip under each preview tile.
const LABEL_H: u32 = 2 * film::CHAR_H + 6;

/// The subject a preview or check reports on: the given path, else the first camera's look-at
/// entity.
fn default_subject(tl: &Timeline, given: Option<String>) -> Option<String> {
    given.or_else(|| {
        tl.cameras().find_map(|(_, c)| match &c.look {
            specula_tempus::Look::Entity { path, .. } => Some(path.clone()),
            _ => None,
        })
    })
}

/// `timeline_preview`: a tile per step from the rail camera, then the sheet encoded on a worker.
struct PreviewJob {
    path: String,
    version: u64,
    tl: Arc<Timeline>,
    times: Vec<f64>,
    tile: (u32, u32),
    cols: u32,
    sheet: Vec<u8>,
    sw: u32,
    sh: u32,
    frames: Vec<Json>,
    subject: Option<String>,
    session: SessionInfo,
    call: u64,
    /// Its own slot for the tile in flight ([`crate::capture::flight::CaptureQueue::step_flight`]
    /// holds it weakly, so no other job's tile can push this one out).
    slot: std::rc::Rc<crate::capture::flight::StepTile>,
    /// The pose of the tile in flight.
    flying: Option<specula_tempus::Pose>,
}

impl PreviewJob {
    #[allow(clippy::too_many_arguments)]
    fn new(e: &mut Engine, path: &str, frames: u32, tw: u32, th: u32, from: Option<f64>, to: Option<f64>, subject: Option<String>, session: &SessionInfo, call: u64) -> Result<PreviewJob, ToolOutput> {
        let (tl, version) = e.timeline_load(path).map_err(|err| world_error(&err))?;
        if tl.cameras().next().is_none() {
            return Err(error("no_camera", format!("{path} has no camera track to film")));
        }
        let _ = e.before_capture();
        let from = from.unwrap_or(0.0).min(tl.duration);
        let to = to.unwrap_or(tl.duration).clamp(from, tl.duration);
        let n = frames.max(2) as usize;
        let times: Vec<f64> = (0..n).map(|k| from + (to - from) * k as f64 / (n - 1) as f64).collect();
        let (cols, rows) = crate::orbit::grid(n as u32);
        let (sw, sh) = (cols * tw, rows * (th + LABEL_H));
        let mut sheet = vec![0u8; (sw * sh * 4) as usize];
        film::fill(&mut sheet, sw, 0, 0, sw, sh, [14, 17, 26, 255]);
        let subject = default_subject(&tl, subject);
        Ok(PreviewJob { path: path.to_owned(), version, tl, times, tile: (tw, th), cols, sheet, sw, sh, frames: Vec::new(), subject, session: session.clone(), call, slot: std::rc::Rc::default(), flying: None })
    }

    /// Submits the next tile's view (no wait for the GPU: it lands in a later step).
    fn submit(&mut self, e: &mut Engine) -> Result<(), ToolOutput> {
        let i = self.frames.len();
        let t = self.times[i];
        let world = &e.world;
        let Some(pose) = self.tl.camera_at(t, &|p| entity_centre(world, p)) else { return Err(error("no_camera", "no camera at that time")) };
        let camera = camera_of(&pose);
        let (tw, th) = self.tile;
        let overlays = crate::sight_overlay::Spec::default();
        let opts = capture::ViewOpts { include_ui: false, lod: None, time_s: e.world.world_time(), overlays: &overlays, caption: None, ids: true, hud: true, fill: None };
        match e.render_view_submit(camera, tw, th, &opts).map_err(ToolOutput::error)? {
            capture::Submitted::Done(r) => {
                *self.slot.borrow_mut() = Some(Ok(*r));
            }
            capture::Submitted::Flying(v) => e.captures.step_flight = Some((std::rc::Rc::downgrade(&self.slot), v)),
        }
        self.flying = Some(pose);
        Ok(())
    }

    /// The tile in flight once the GPU has finished it (`Ok(false)`: not yet, try the next step).
    fn land(&mut self, e: &mut Engine) -> Result<bool, ToolOutput> {
        let taken = self.slot.borrow_mut().take();
        let landed = match taken {
            Some(r) => r,
            None => {
                if !e.captures.step_flight.as_ref().is_some_and(|(s, _)| std::ptr::eq(s.as_ptr(), std::rc::Rc::as_ptr(&self.slot))) {
                    return Err(error("render_unavailable", "the preview's tile was lost in flight"));
                }
                match e.gpu.settle_offscreen_within(Some(std::time::Duration::ZERO)) {
                    Ok(false) => return Ok(false),
                    Ok(true) => {
                        let (_, v) = e.captures.step_flight.take().expect("checked above");
                        e.render_view_land(*v)
                    }
                    Err(err) => Err(capture::gpu_failure("render", &err, e.windowed)),
                }
            }
        };
        let mut rend = landed.map_err(ToolOutput::error)?;
        let pose = self.flying.take().expect("a tile in flight");
        let (tw, th) = self.tile;
        if let Some(feed) = rend.feed.take() {
            e.flash_job(feed, rend.rgba.clone(), tw, th);
        }
        self.place(e, pose, rend);
        Ok(true)
    }

    fn place(&mut self, e: &mut Engine, pose: specula_tempus::Pose, rend: capture::Rendered) {
        let i = self.frames.len();
        let t = self.times[i];
        let (tw, th) = self.tile;
        let (x0, y0) = ((i as u32 % self.cols) * tw, (i as u32 / self.cols) * (th + LABEL_H));
        film::blit(&mut self.sheet, self.sw, x0, y0, &rend.rgba, tw, th);
        let track = self.tl.tracks[pose.track].name.as_str();
        film::draw_text(&mut self.sheet, self.sw, x0 + 3, y0 + th + 3, &format!("t={t:.2} {track}"), [220, 216, 200]);
        let sub_id = self.subject.as_deref().and_then(|p| e.world.find_path(p).ok());
        let mut frame = Json::obj([
            ("frame", Json::from(i + 1)),
            ("t", Json::from(round(t, 4))),
            ("camera", pose_json(&pose, &self.tl)),
            ("tile", Json::arr([x0, y0, tw, th].map(Json::from))),
        ]);
        if let Some(path) = &self.subject {
            // The subject and everything below it (a group such as /slab has no geometry of its
            // own): in frame if any part is, its visible pixels summed, its box the union.
            let parts: Vec<&crate::sight::Subject> = match sub_id {
                Some(id) => rend.subjects.iter().filter(|s| is_under(&e.world, s.id, id)).collect(),
                None => Vec::new(),
            };
            let on_screen = parts.iter().any(|s| s.on_screen);
            let visible: Option<u64> = parts.iter().filter_map(|s| s.visible_px()).fold(None, |a, v| Some(a.unwrap_or(0) + v));
            let screen = parts.iter().filter_map(|s| s.screen).fold(None, |a: Option<[u32; 4]>, b| {
                Some(match a {
                    None => b,
                    Some(a) => {
                        let (x0, y0) = (a[0].min(b[0]), a[1].min(b[1]));
                        let (x1, y1) = ((a[0] + a[2]).max(b[0] + b[2]), (a[1] + a[3]).max(b[1] + b[3]));
                        [x0, y0, x1 - x0, y1 - y0]
                    }
                })
            });
            // From the frame record: whether any part reached the image.
            let drawn = parts.iter().map(|s| rend.record.drawn(s.id).drawn()).fold(None, |a: Option<bool>, d| match (a, d) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (None, d) => d,
                (a, _) => a,
            });
            frame.set(
                "subject",
                Json::obj([
                    ("path", Json::from(path.as_str())),
                    ("found", Json::from(sub_id.is_some())),
                    ("parts", Json::from(parts.len())),
                    ("on_screen", Json::from(on_screen)),
                    ("screen", screen.map_or(Json::Null, |b| Json::arr(b.map(Json::from)))),
                    ("visible_px", visible.map_or(Json::Null, Json::from)),
                    ("drawn", drawn.map_or(Json::Null, Json::from)),
                ]),
            );
        }
        let mut top: Vec<&crate::sight::Subject> = rend.subjects.iter().filter(|s| s.on_screen).collect();
        top.sort_by(|a, b| b.projected_area.cmp(&a.projected_area).then(a.id.cmp(&b.id)));
        frame.set("subjects_on_screen", Json::from(top.len()));
        frame.set(
            "largest",
            Json::arr(top.iter().take(5).map(|s| {
                Json::obj([("path", Json::from(s.base.path.as_str())), ("screen", s.screen.map_or(Json::Null, |b| Json::arr(b.map(Json::from)))), ("visible_px", s.visible_px().map_or(Json::Null, Json::from))])
            })),
        );
        frame.set("draws_submitted", Json::from(rend.record.submitted().len()));
        self.frames.push(frame);
    }

    fn finish(&mut self, e: &mut Engine, reply: &crate::jobs::Reply) -> Step {
        let png = e.next_capture_path();
        let number = e.frames_captured;
        let (sw, sh) = (self.sw, self.sh);
        let (cols, rows) = crate::orbit::grid(self.times.len() as u32);
        let mut packet = Json::obj([
            ("schema", Json::from(sight::SCHEMA)),
            ("kind", Json::from("timeline_preview")),
            ("timeline", Json::from(self.path.as_str())),
            ("version", Json::from(self.version)),
            ("frames", Json::Arr(std::mem::take(&mut self.frames))),
            ("grid", Json::arr([cols, rows].map(Json::from))),
            ("tile", Json::arr([self.tile.0, self.tile.1].map(Json::from))),
            ("note", Json::from("the world as it is now, seen from the rail camera at each time (entity tracks aren't applied: play or scrub to see them)")),
        ]);
        let pixels = std::mem::take(&mut self.sheet);
        e.offload_capture(&self.session, Answer { reply: reply.clone(), call: self.call }, (sw, sh, pixels), png, "timeline", move |png_bytes, saved, warnings| {
            let packet_path = saved.as_ref().map(|p| p.with_extension("json"));
            packet.set(
                "image",
                Json::obj([
                    ("width", Json::from(sw)),
                    ("height", Json::from(sh)),
                    ("png_bytes", Json::from(png_bytes.len())),
                    ("path", saved.as_ref().map(|p| p.display().to_string()).into()),
                    ("packet_path", packet_path.as_ref().map(|p| p.display().to_string()).into()),
                ]),
            );
            packet.set("frame", Json::from(number));
            let mut warnings = warnings;
            if let Some(p) = &packet_path {
                if let Err(err) = crate::disk::book().write(crate::disk::Category::Captures, "timeline", p, packet.to_pretty().as_bytes(), crate::disk::Need::Agent) {
                    warnings.push(Json::from(format!("packet not saved to disk: {err}")));
                }
            }
            packet.set("warnings", Json::Arr(warnings));
            ToolOutput { content: vec![Content::Image { mime_type: "image/png".into(), data_base64: base64::encode(&png_bytes) }, Content::Text(packet.to_string())], is_error: false }
        });
        Step::Handed
    }
}

impl StepJob for PreviewJob {
    fn kind(&self) -> &'static str {
        "timeline_preview"
    }

    fn step(&mut self, e: &mut Engine, reply: &crate::jobs::Reply) -> Step {
        // A tile per two steps at least: submitted, then landed once the GPU is done (lead
        // review: rendering and reading a tile in one step held the engine thread 80-113 ms in a
        // 20,000-entity world).
        if self.flying.is_some() {
            return match self.land(e) {
                Ok(_) => Step::More,
                Err(out) => Step::Done(out),
            };
        }
        if self.frames.len() < self.times.len() {
            // The subject cache brought up to date a slice per step first, as the capture flight
            // does (a world change between tiles would rebuild it for every entity in the submit).
            if !e.subject_cache.warm(&e.world, e.world_generation, std::time::Duration::from_millis(3)) {
                return Step::More;
            }
            return match self.submit(e) {
                Ok(()) => Step::More,
                Err(out) => Step::Done(out),
            };
        }
        self.finish(e, reply)
    }
}

/// One failure a check lists.
fn failure(t: f64, message: String) -> Json {
    Json::obj([("at_t", Json::from(round(t, 4))), ("message", Json::from(message))])
}

/// `timeline_check`: samples measured a slice per step (poses, projections, raycasts), then the
/// motion checks.
struct CheckJob {
    spec: CheckSpec,
    tl: Arc<Timeline>,
    version: u64,
    /// The span and how many samples it takes ([`tcheck::sample_count`]).
    from: f64,
    to: f64,
    n: usize,
    /// Steps done before the samples: 0 the subject's bounds and look-at centres, 1 physics.
    prepared: u8,
    subject_path: Option<String>,
    /// Look-at entities' centres, resolved once per job (lead review: every one of 4,096 samples
    /// looked its entity up by path, all in the request).
    centres: BTreeMap<String, [f64; 3]>,
    cuts: Vec<f64>,
    samples: Vec<Sample>,
    at: usize,
    subject: Option<(u64, String, Vec3, Vec3)>,
    on_screen: usize,
    off: Vec<Json>,
    size: (f64, f64),
    size_bad: Vec<Json>,
    clearance: Vec<Json>,
    blocked_los: usize,
    los_first: Option<f64>,
    colliders: usize,
}

impl CheckJob {
    /// The job, checked: the samples are taken a slice per step, after a step for the subject and
    /// look-at entities and one for physics (lead review: all 4,096 poses and the physics sync in
    /// the request held the engine thread 8-16 ms).
    fn new(e: &mut Engine, spec: CheckSpec) -> Result<CheckJob, ToolOutput> {
        let (tl, version) = e.timeline_load(&spec.path).map_err(|err| world_error(&err))?;
        if tl.cameras().next().is_none() {
            return Err(error("no_camera", format!("{} has no camera track to check", spec.path)));
        }
        let from = spec.from.unwrap_or(0.0).min(tl.duration);
        let to = spec.to.unwrap_or(tl.duration).clamp(from, tl.duration);
        let n = tcheck::sample_count(from, to, spec.rate);
        let subject_path = default_subject(&tl, spec.subject.clone());
        if let Some(p) = &subject_path {
            e.world.find_path(p).map_err(|err| world_error(&err))?;
        }
        let cuts = tl.camera_cuts();
        Ok(CheckJob {
            spec,
            tl,
            version,
            from,
            to,
            n,
            prepared: 0,
            subject_path,
            centres: BTreeMap::new(),
            cuts,
            samples: Vec::with_capacity(n),
            at: 0,
            subject: None,
            on_screen: 0,
            off: Vec::new(),
            size: (f64::INFINITY, 0.0),
            size_bad: Vec::new(),
            clearance: Vec::new(),
            blocked_los: 0,
            los_first: None,
            colliders: 0,
        })
    }

    /// The step before the samples: the subject's bounds (a walk of its subtree) and the look-at
    /// entities' centres, as the world has them now.
    fn prepare(&mut self, e: &mut Engine) -> Result<(), ToolOutput> {
        if let Some(p) = &self.subject_path {
            let id = e.world.find_path(p).map_err(|err| world_error(&err))?;
            let (lo, hi) = capture::subtree_bounds(&e.world, id).unwrap_or_default();
            self.subject = Some((id, p.clone(), lo, hi));
        }
        for (_, c) in self.tl.cameras() {
            if let specula_tempus::Look::Entity { path, .. } = &c.look {
                if let Some(at) = entity_centre(&e.world, path) {
                    self.centres.insert(path.clone(), at);
                }
            }
        }
        Ok(())
    }

    fn slice(&mut self, e: &mut Engine) {
        let end = (self.at + CHECK_SLICE).min(self.n);
        let filter = specula_physics::Filter::new(u64::MAX, false, None);
        let v = |a: [f64; 3]| Vec3::new(a[0] as f32, a[1] as f32, a[2] as f32);
        let cuts = std::mem::take(&mut self.cuts);
        let (vw, vh) = ((1000.0 * self.spec.aspect).round() as u32, 1000u32);
        for k in self.at..end {
            let t = tcheck::sample_time(self.from, self.to, self.n, k);
            let centres = &self.centres;
            let Some(pose) = self.tl.camera_at(t, &|p| centres.get(p).copied()) else { continue };
            self.samples.push(Sample { t, pose });
            let i = self.samples.len() - 1;
            let s = self.samples[i];
            let eye = v(s.pose.eye);
            // The subject: in frame, and its size.
            if let Some((id, path, lo, hi)) = &self.subject {
                let cam = camera_of(&s.pose);
                let vp = cam.view_proj(vw, vh);
                let mut rect: Option<(f32, f32, f32, f32)> = None;
                let mut behind = false;
                for k in 0..8 {
                    let c = Vec3::new(if k & 1 == 0 { lo.x } else { hi.x }, if k & 2 == 0 { lo.y } else { hi.y }, if k & 4 == 0 { lo.z } else { hi.z });
                    let p = vp.mul_vec4(c.extend(1.0));
                    if p.w <= 1e-4 {
                        behind = true;
                        continue;
                    }
                    let (x, y) = (p.x / p.w, p.y / p.w);
                    rect = Some(rect.map_or((x, y, x, y), |r| (r.0.min(x), r.1.min(y), r.2.max(x), r.3.max(y))));
                }
                let centre = vp.mul_vec4(((*lo + *hi) * 0.5).extend(1.0));
                let in_frame = centre.w > 1e-4 && (centre.x / centre.w).abs() <= 1.0 && (centre.y / centre.w).abs() <= 1.0;
                if in_frame {
                    self.on_screen += 1;
                } else if self.off.len() < MAX_FAILURES {
                    self.off.push(failure(s.t, format!("{path}'s centre is out of frame")));
                }
                if let (Some(r), false) = (rect, behind) {
                    let h = f64::from((r.3 - r.1) * 0.5);
                    self.size = (self.size.0.min(h), self.size.1.max(h));
                    if in_frame && (h < self.spec.min_size || h > self.spec.max_size) && self.size_bad.len() < MAX_FAILURES {
                        self.size_bad.push(failure(s.t, format!("{path} fills {:.3} of the view's height", h)));
                    }
                }
                // Line of sight: a collider between the eye and the subject's centre that isn't
                // the subject (or part of it).
                let c = (*lo + *hi) * 0.5;
                let d = c - eye;
                let len = d.length();
                if len > 0.01 {
                    let (hits, _) = e.physics.space.raycast(eye, d * (1.0 / len), len, &filter, 4);
                    let blocker = hits.iter().find(|h| h.key != *id && !e.world.get(h.key).is_some_and(|x| is_under(&e.world, x.id, *id)));
                    if blocker.is_some() {
                        self.blocked_los += 1;
                        self.los_first.get_or_insert(s.t);
                    }
                }
            }
            // Clearance: the step from the previous sample, both ways (a ray starting inside a
            // collider doesn't hit its own surface).
            if i > 0 && !cuts.iter().any(|c| *c > self.samples[i - 1].t && *c <= s.t) {
                let a = v(self.samples[i - 1].pose.eye);
                let d = eye - a;
                let len = d.length();
                if len > 1e-5 && self.clearance.len() < MAX_FAILURES {
                    let dir = d * (1.0 / len);
                    let (fwd, _) = e.physics.space.raycast(a, dir, len, &filter, 1);
                    let (back, _) = e.physics.space.raycast(eye, dir * -1.0, len, &filter, 1);
                    if let Some(h) = fwd.first().or(back.first()) {
                        let what = e.world.get(h.key).map_or_else(|| format!("body {}", h.key), |x| e.world.path_of(x.id));
                        self.clearance.push(Json::obj([
                            ("at_t", Json::from(round(s.t, 4))),
                            ("entity", Json::from(what.as_str())),
                            ("point", Json::arr([h.point.x, h.point.y, h.point.z].map(|c| Json::from(round(f64::from(c), 3))))),
                            ("message", Json::from(format!("the camera passes through {what} between t = {} and {} s", round(self.samples[i - 1].t, 3), round(s.t, 3)))),
                        ]));
                    }
                }
            }
        }
        self.cuts = cuts;
        self.at = end;
    }

    fn finish(&mut self) -> ToolOutput {
        let n = self.samples.len().max(1);
        let mut checks: Vec<Json> = Vec::new();
        let mut push = |name: &str, ok: bool, severity: &str, value: Json, limit: Json, unit: &str, message: String, failures: Vec<Json>| {
            let mut j = Json::obj([
                ("check", Json::from(name)),
                ("ok", Json::from(ok)),
                ("severity", Json::from(severity)),
                ("value", value),
                ("limit", limit),
                ("unit", Json::from(unit)),
                ("message", Json::from(message)),
            ]);
            if !failures.is_empty() {
                j.set("failures", Json::Arr(failures));
            }
            checks.push(j);
        };
        match &self.subject {
            Some((_, path, _, _)) => {
                let frac = self.on_screen as f64 / n as f64;
                let ok = frac + 1e-9 >= self.spec.min_on_screen;
                push("subject_on_screen", ok, "error", Json::from(round(frac, 4)), Json::from(self.spec.min_on_screen), "fraction", format!("{path} is in frame for {:.1}% of {n} samples", frac * 100.0), std::mem::take(&mut self.off));
                let (lo, hi) = if self.size.0.is_finite() { self.size } else { (0.0, 0.0) };
                let ok = self.size_bad.is_empty();
                push(
                    "subject_size",
                    ok,
                    "error",
                    Json::arr([Json::from(round(lo, 4)), Json::from(round(hi, 4))]),
                    Json::arr([Json::from(self.spec.min_size), Json::from(self.spec.max_size)]),
                    "fraction of view height",
                    format!("{path} fills {:.3}..{:.3} of the view's height", lo, hi),
                    std::mem::take(&mut self.size_bad),
                );
                let blocked = self.blocked_los as f64 / n as f64;
                push(
                    "line_of_sight",
                    self.blocked_los == 0,
                    "warning",
                    Json::from(round(blocked, 4)),
                    Json::from(0.0),
                    "fraction blocked",
                    match self.los_first {
                        Some(t) => format!("a collider stands between the camera and {path} in {:.1}% of samples (first at t = {})", blocked * 100.0, round(t, 3)),
                        None => format!("nothing with a collider stands between the camera and {path}"),
                    },
                    Vec::new(),
                );
            }
            None => push("subject_on_screen", true, "skipped", Json::Null, Json::Null, "", "no subject: give 'subject' (or look at an entity) to check framing".into(), Vec::new()),
        }
        let hits = std::mem::take(&mut self.clearance);
        push(
            "clearance",
            hits.is_empty(),
            "error",
            Json::from(hits.len()),
            Json::from(0),
            "passes through colliders",
            if hits.is_empty() { format!("the camera never passes through a collider ({} considered)", self.colliders) } else { "the camera passes through geometry (see failures)".into() },
            hits,
        );
        for c in tcheck::motion(&self.samples, &self.cuts, &self.spec.limits) {
            let mut j = c.to_json();
            j.set("severity", Json::from("error"));
            checks.push(j);
        }
        let errors = checks.iter().filter(|c| c.get("ok").and_then(Json::as_bool) == Some(false) && c.get("severity").and_then(Json::as_str) == Some("error")).count();
        let warnings = checks.iter().filter(|c| c.get("ok").and_then(Json::as_bool) == Some(false) && c.get("severity").and_then(Json::as_str) == Some("warning")).count();
        let out = Json::obj([
            ("timeline", Json::from(self.spec.path.as_str())),
            ("version", Json::from(self.version)),
            ("ok", Json::from(errors == 0)),
            ("errors", Json::from(errors)),
            ("warnings", Json::from(warnings)),
            ("samples", Json::from(self.samples.len())),
            ("cuts", Json::arr(self.cuts.iter().map(|c| Json::from(round(*c, 4))))),
            ("colliders_considered", Json::from(self.colliders)),
            ("checks", Json::Arr(checks)),
        ]);
        ToolOutput::text(out.to_string())
    }
}

/// Whether `id` is `root` or below it.
fn is_under(world: &World, id: u64, root: u64) -> bool {
    let mut cur = Some(id);
    for _ in 0..256 {
        match cur {
            Some(c) if c == root => return true,
            Some(c) => cur = world.get(c).and_then(|e| e.parent),
            None => return false,
        }
    }
    false
}

impl StepJob for CheckJob {
    fn kind(&self) -> &'static str {
        "timeline_check"
    }

    fn step(&mut self, e: &mut Engine, _reply: &crate::jobs::Reply) -> Step {
        match self.prepared {
            0 => {
                self.prepared = 1;
                return match self.prepare(e) {
                    Ok(()) => Step::More,
                    Err(out) => Step::Done(out),
                };
            }
            1 => {
                // Queries see the world as it is now (a step of its own).
                e.physics.sync(&e.world, e.world_generation);
                self.colliders = e.physics.space.len();
                self.prepared = 2;
                return Step::More;
            }
            _ => {}
        }
        if self.at < self.n {
            self.slice(e);
            return Step::More;
        }
        Step::Done(self.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(name: &str, args: &str) -> TimelineCall {
        parse(name, &Json::parse(args).unwrap()).unwrap().unwrap()
    }

    fn bad(name: &str, args: &str) -> String {
        match parse(name, &Json::parse(args).unwrap()).unwrap() {
            Err(e) => e,
            Ok(_) => panic!("{name} {args} parsed"),
        }
    }

    #[test]
    fn arguments_are_bounded_and_paths_are_timelines() {
        assert!(matches!(ok("timeline_play", r#"{"path": "intro"}"#), TimelineCall::Play { ref path, .. } if path == "assets/timelines/intro.json"));
        assert!(matches!(ok("timeline_play", r#"{"path": "assets/sepulcra/timelines/x-1.json"}"#), TimelineCall::Play { .. }));
        for p in ["../x", "assets/timelines/../style.json", "assets/ui/hud.json", "assets/timelines/a b.json", "C:/x/timelines/a.json", "assets//timelines/a.json", "assets\\timelines\\a.json"] {
            assert!(bad("timeline_play", &format!("{{\"path\": {}}}", Json::from(p))).contains("'path'"), "{p}");
        }
        assert!(bad("timeline_play", &format!("{{\"path\": \"{}\"}}", "a".repeat(65))).contains("'path'"));
        assert!(bad("timeline_preview", r#"{"path": "a", "frames": 40}"#).contains("frames"));
        assert!(bad("timeline_preview", r#"{"path": "a", "from": 5, "to": 2}"#).contains("after"));
        assert!(bad("timeline_scrub", r#"{"path": "a"}"#).contains("'t'"));
        assert!(bad("timeline_scrub", r#"{"path": "a", "t": 1e9}"#).contains("'t'"));
        assert!(bad("timeline_check", r#"{"path": "a", "min_size": 0.5, "max_size": 0.2}"#).contains("max_size"));
        assert!(bad("timeline_check", r#"{"path": "a", "subject": "slab"}"#).contains("subject"));
        assert!(bad("timeline_get", r#"{"at": 2}"#).contains("path"));
        assert!(bad("timeline_validate", r#"{}"#).contains("path"));
        assert!(bad("timeline_play", r#"{"path": "a", "speed": 2}"#).contains("speed"));
        assert!(parse("world_spawn", &Json::Null).is_none());
    }

    #[test]
    fn every_tool_has_a_definition_and_a_bounded_schema() {
        let defs = definitions();
        for d in &defs {
            assert!(d.name.starts_with("timeline_"));
            assert!(d.description.len() > 40);
        }
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["timeline_get", "timeline_set", "timeline_validate", "timeline_play", "timeline_pause", "timeline_scrub", "timeline_stop", "timeline_preview", "timeline_check"]);
    }

    /// Lead review R2, major 1 (probe6): each submit lands the tile before it, and a shared map
    /// kept only the newest 8, so with more than about 9 previews at once the oldest jobs' tiles
    /// were evicted and they failed with "the preview's tile was lost in flight". Every job owns
    /// its slot now: 16 previews stepped round-robin all finish. (GPU; skips cleanly without it.)
    #[test]
    fn sixteen_previews_round_robin_all_finish() {
        use crate::timeline::tests::{engine, put, session, spawn};
        let Some(mut e) = engine("sixteen_previews_round_robin_all_finish") else { return };
        spawn(&mut e, "pillar", 0.0);
        put(&mut e, "assets/timelines/orbit.json", r#"{"schema": "specula.timeline.v1", "duration": 2, "tracks": [
          {"kind": "camera", "name": "orbit", "rail": {"points": [[6, 2, 0], [0, 2, 6], [-6, 2, 0]]},
           "move": [{"t": 0, "at": 0}, {"t": 2, "at": 1}], "look": {"entity": "/pillar"}}]}"#);
        // Sixteen agents, one preview each (as probe6's callers; the workers take 4 encodes per agent).
        let callers: Vec<SessionInfo> = (0..16).map(|i| session(&format!("dd{i:06}"))).collect();
        for (i, c) in callers.iter().enumerate() {
            e.swarm.hello(c, format!("viewer{i}"), "builder".into(), None, &callers).unwrap();
        }
        let mut jobs: Vec<(PreviewJob, crate::jobs::Reply, std::sync::mpsc::Receiver<ToolOutput>, Option<bool>)> = (0..16u64)
            .map(|i| {
                let job = PreviewJob::new(&mut e, "assets/timelines/orbit.json", 4, 96, 64, None, None, None, &callers[i as usize], 100 + i).unwrap_or_else(|_| panic!("preview {i} starts"));
                let (reply, rx) = crate::jobs::channel();
                (job, reply, rx, None)
            })
            .collect();
        // Round-robin, a step each, as `pump_steps` runs them: every job submits before any lands.
        for _ in 0..10_000 {
            if jobs.iter().all(|j| j.3.is_some()) {
                break;
            }
            for (job, reply, _, done) in jobs.iter_mut().filter(|j| j.3.is_none()) {
                match job.step(&mut e, reply) {
                    Step::More => {}
                    Step::Handed => *done = Some(true),
                    Step::Done(out) => {
                        assert!(!out.is_error, "a preview failed: {:?}", out.content);
                        *done = Some(true);
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(jobs.iter().all(|j| j.3 == Some(true)), "every preview finished its tiles");
        // Every sheet encoded and answered, with all its frames.
        e.workers_drain(std::time::Duration::from_secs(60));
        for (i, (_, _, rx, _)) in jobs.iter().enumerate() {
            let out = rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap_or_else(|_| panic!("preview {i} answered"));
            assert!(!out.is_error, "preview {i}: {:?}", out.content);
            let packet = out.content.iter().find_map(|c| match c {
                Content::Text(t) => Json::parse(t).ok(),
                _ => None,
            });
            let frames = packet.as_ref().and_then(|p| p.get("frames")).and_then(Json::as_array).map_or(0, |f| f.len());
            assert_eq!(frames, 4, "preview {i} has every frame");
        }
    }
}
