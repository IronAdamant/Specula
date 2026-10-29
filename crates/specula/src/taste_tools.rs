//! The taste tools (docs/design/GALLERIES.md): galleries and the pick flow, the preference
//! store, reference boards and the style configurator. Arguments are parsed and spaces validated
//! here, on the caller's connection thread; waits (a round being made, a decision) poll the
//! engine here too, so the engine thread never blocks. Board images are read and measured here.

use crate::taste::{Decision, TasteCall};
use crate::tools::{argument_error, bool_arg, check_keys, int_arg, string_arg, Command, Tools};
use specula_core::{base64, Json};
use specula_gustus::board::{self, Board};
use specula_gustus::gallery::{Gallery, Kind, Mode, DEFAULT_VARIANTS, MAX_VARIANTS};
use specula_gustus::prefs as preferences;
use preferences::Query;
use specula_gustus::space::Space;
use specula_protocol::{Content, SessionInfo, ToolDef, ToolOutput};
use std::time::{Duration, Instant};

/// The longest a call waits (for a round, or a decision).
const MAX_WAIT: Duration = Duration::from_secs(25);
const POLL: Duration = Duration::from_millis(150);
/// Board images: per file and per board.
const MAX_IMAGE_FILE: u64 = 64 << 20;

pub const NAMES: [&str; 15] =
    ["gallery_create", "gallery_get", "gallery_delete", "gallery_list", "gallery_open", "gallery_close", "gallery_wait", "gallery_decide", "gallery_round", "gallery_apply", "taste_query", "taste_suggest", "board_measure", "board_check", "style_onboard"];

pub fn is_taste_tool(name: &str) -> bool {
    NAMES.contains(&name)
}

fn bad(message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(argument_error(&message.into()).to_pretty())
}

fn err(code: &str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_string())
}

fn text(o: &ToolOutput) -> Option<Json> {
    o.content.iter().find_map(|c| match c {
        Content::Text(t) => Json::parse(t).ok(),
        Content::Image { .. } => None,
    })
}

fn send(tools: &Tools, session: &SessionInfo, call: TasteCall) -> ToolOutput {
    tools.send(session, Command::Taste(call))
}

/// The gallery as data, polled until `done` says so (or the wait runs out).
fn poll(tools: &Tools, session: &SessionInfo, id: &str, wait: Duration, done: impl Fn(&Json) -> bool) -> ToolOutput {
    let until = Instant::now() + wait.min(MAX_WAIT);
    loop {
        let out = send(tools, session, TasteCall::Get { id: id.to_owned(), round: None });
        if out.is_error || text(&out).is_some_and(|j| done(&j)) || Instant::now() >= until {
            return out;
        }
        std::thread::sleep(POLL);
    }
}

/// Adds the round's sheet to an answer as an image.
/// Largest sheet attached as an image (tokens are a cost; the path is always given).
const MAX_IMAGE_BYTES: u64 = 2 << 20;

fn with_image(out: ToolOutput) -> ToolOutput {
    let Some(mut j) = text(&out) else { return out };
    let Some(path) = j.get("round").and_then(|r| r.get("sheet")).and_then(Json::as_str).map(str::to_owned) else { return out };
    let too_big = std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_IMAGE_BYTES);
    if too_big {
        j.set("image_note", Json::from(format!("the sheet is over {} MiB, so it isn't attached: read it from round.sheet", MAX_IMAGE_BYTES >> 20)));
        return ToolOutput::text(j.to_string());
    }
    match std::fs::read(&path) {
        Ok(bytes) => {
            let mut o = ToolOutput::text(j.to_string());
            o.content.push(Content::Image { mime_type: "image/png".into(), data_base64: base64::encode(&bytes) });
            o
        }
        Err(_) => out,
    }
}

/// A capture the engine wrote: a `.png` inside this run's captures folder, checked by the
/// engine's one capture rule ([`crate::safe_path::capture_file`]: no UNC, drive-relative or
/// stream paths, no '..', no links) before anything is opened.
fn capture_file(captures: &std::path::Path, path: &str) -> Result<std::path::PathBuf, ToolOutput> {
    if !std::path::Path::new(path).extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        return Err(err("not_a_capture", "capture: a PNG the engine wrote in this run's captures folder (sight_frame's image.path)"));
    }
    crate::safe_path::capture_file(captures, captures, path, "capture").map_err(|r| err(r.code, r.message))
}

/// The board's images that changed on disk since it was measured (names), or the error reading
/// the folder.
fn stale_images(assets: &std::path::Path, b: &Board) -> Vec<String> {
    let dir = assets.join("taste").join("boards").join(&b.name);
    let mut out = Vec::new();
    for i in &b.images {
        let p = dir.join(&i.file);
        let same = std::fs::metadata(&p).is_ok_and(|m| m.len() <= MAX_IMAGE_FILE) && std::fs::read(&p).is_ok_and(|bytes| format!("{:016x}", specula_core::hash::hash_bytes(&bytes)) == i.hash);
        if !same {
            out.push(i.file.clone());
        }
    }
    out
}

fn rendered(j: &Json) -> bool {
    j.get("problem").is_some() || (j.get("status").and_then(Json::as_str) != Some("rendering") && j.get("round").is_some())
}

/// Answers a taste tool (on the caller's connection thread).
pub fn call(tools: &Tools, session: &SessionInfo, name: &str, args: &Json) -> ToolOutput {
    match run(tools, session, name, args) {
        Ok(o) => o,
        Err(o) => o,
    }
}

fn s(args: &Json, key: &str) -> Result<Option<String>, ToolOutput> {
    string_arg(args, key).map(|o| o.map(str::to_owned)).map_err(bad)
}

fn need(args: &Json, key: &str) -> Result<String, ToolOutput> {
    s(args, key)?.ok_or_else(|| bad(format!("'{key}' is required")))
}

fn uint(args: &Json, key: &str, lo: u64, hi: u64) -> Result<Option<u64>, ToolOutput> {
    int_arg(args, key, lo, hi).map_err(bad)
}

fn flag(args: &Json, key: &str) -> Result<Option<bool>, ToolOutput> {
    bool_arg(args, key).map_err(bad)
}

fn user_of(args: &Json) -> Result<String, ToolOutput> {
    let u = s(args, "user")?.unwrap_or_else(|| crate::authority::ROLE_OWNER.into());
    if !specula_gustus::valid_name(&u) {
        return Err(bad("user is a label [a-z0-9_-] (\"owner\" by default)"));
    }
    Ok(u)
}

fn run(tools: &Tools, session: &SessionInfo, name: &str, args: &Json) -> Result<ToolOutput, ToolOutput> {
    const FMT: &str = "response_format";
    let keys = |allowed: &[&str]| check_keys(args, name, &[allowed, &[FMT]]).map_err(bad);
    Ok(match name {
        "gallery_create" => {
            keys(&["id", "title", "subject", "space", "mode", "variants", "seed", "tile", "start", "apply_on_accept", "open", "user", "wait", "image"])?;
            let id = s(args, "id")?.unwrap_or_default();
            if !id.is_empty() && !specula_gustus::valid_name(&id) {
                return Err(bad("id: [a-z0-9_-], 1-40 characters (or leave it out)"));
            }
            let title = s(args, "title")?.unwrap_or_else(|| "Which looks right?".into());
            let mut subject = args.get("subject").cloned().ok_or_else(|| bad("'subject' is required: {recipe}, {scene: {camera}}, {ui: {panel, lines}} or {stage: {}}"))?;
            if !matches!(subject, Json::Obj(_)) {
                return Err(bad("subject is an object"));
            }
            if let Some(t) = uint(args, "tile", 96, u64::from(crate::studio::gallery::MAX_TILE))? {
                subject.set("tile", Json::from(t));
            }
            let space = Space::parse(args.get("space").unwrap_or(&Json::Null)).map_err(|e| ToolOutput::error(e.to_json().to_string()))?;
            let mode = match s(args, "mode")? {
                Some(m) => Mode::parse(&m).ok_or_else(|| bad("mode: tune (the default: picks drive preferential Bayesian optimisation), line (a 1-D subspace per round) or spread"))?,
                None => Mode::Tune,
            };
            let variants = uint(args, "variants", 2, MAX_VARIANTS as u64)?.map_or(DEFAULT_VARIANTS, |v| v as usize);
            let seed = uint(args, "seed", 0, u64::MAX >> 1)?.unwrap_or_else(|| specula_core::hash::hash_str(&format!("{id}:{title}")));
            let mut g = Gallery::new(if id.is_empty() { "pending" } else { &id }, &title, subject, space, mode, seed, variants, &session.id).map_err(|e| ToolOutput::error(e.to_json().to_string()))?;
            if id.is_empty() {
                g.id.clear();
            }
            g.apply_on_accept = flag(args, "apply_on_accept")?.unwrap_or(false);
            let user = user_of(args)?;
            let wait = flag(args, "wait")?.unwrap_or(true);
            let out = send(tools, session, TasteCall::Create { gallery: Box::new(g), start: args.get("start").cloned(), open: flag(args, "open")?.unwrap_or(false), user, wait });
            if out.is_error || !wait {
                return Ok(out);
            }
            let Some(id) = text(&out).and_then(|j| j.get("id").and_then(Json::as_str).map(str::to_owned)) else { return Ok(out) };
            let o = poll(tools, session, &id, MAX_WAIT, rendered);
            if flag(args, "image")? == Some(true) {
                with_image(o)
            } else {
                o
            }
        }
        "gallery_get" => {
            keys(&["id", "round", "image"])?;
            let out = send(tools, session, TasteCall::Get { id: need(args, "id")?, round: uint(args, "round", 1, 32)?.map(|r| r as usize) });
            if flag(args, "image")? == Some(true) {
                with_image(out)
            } else {
                out
            }
        }
        "gallery_delete" => {
            keys(&["id"])?;
            send(tools, session, TasteCall::Delete { id: need(args, "id")? })
        }
        "gallery_list" => {
            keys(&["status", "after", "limit"])?;
            let status = s(args, "status")?;
            if status.as_deref().is_some_and(|x| !matches!(x, "open" | "rendering" | "accepted")) {
                return Err(bad("status: open, rendering or accepted"));
            }
            send(tools, session, TasteCall::List { status, after: s(args, "after")?, limit: uint(args, "limit", 1, 32)?.unwrap_or(16) as usize })
        }
        "gallery_open" => {
            keys(&["id", "user"])?;
            send(tools, session, TasteCall::Open { id: need(args, "id")?, user: user_of(args)? })
        }
        "gallery_close" => {
            keys(&[])?;
            send(tools, session, TasteCall::Close)
        }
        "gallery_wait" => {
            keys(&["id", "after", "timeout_s", "image"])?;
            let id = need(args, "id")?;
            let after = uint(args, "after", 0, u64::MAX >> 1)?;
            let wait = Duration::from_secs(uint(args, "timeout_s", 0, MAX_WAIT.as_secs())?.unwrap_or(20));
            let first = send(tools, session, TasteCall::Get { id: id.clone(), round: None });
            let Some(j0) = text(&first) else { return Ok(first) };
            let after = after.unwrap_or_else(|| j0.get("seq").and_then(Json::as_u64).unwrap_or(0));
            // A newer decision, and the round it asked for once made (or the gallery done).
            let o = poll(tools, session, &id, wait, |j| {
                let seq = j.get("seq").and_then(Json::as_u64).unwrap_or(0);
                seq > after && (j.get("status").and_then(Json::as_str) != Some("rendering") || j.get("problem").is_some())
            });
            let o = match text(&o) {
                Some(mut j) => {
                    let seq = j.get("seq").and_then(Json::as_u64).unwrap_or(0);
                    j.set("decided", Json::Bool(seq > after));
                    if seq <= after {
                        j.set("note", Json::from("no new decision yet: call again (the human may be looking)"));
                    }
                    ToolOutput::text(j.to_string())
                }
                None => o,
            };
            if flag(args, "image")? == Some(true) {
                with_image(o)
            } else {
                o
            }
        }
        "gallery_decide" => {
            keys(&["id", "round", "decision", "variant", "note", "chips", "to", "at", "user"])?;
            let variant = uint(args, "variant", 0, MAX_VARIANTS as u64 - 1)?.map(|v| v as usize);
            let note = s(args, "note")?.unwrap_or_default();
            let need_variant = || variant.ok_or_else(|| bad("this decision needs 'variant' (0-based, as gallery_get lists them)"));
            let decision = match need(args, "decision")?.as_str() {
                "accept" => Decision::Kind(Kind::Accept { variant: need_variant()? }),
                "edit" => {
                    let chips = args.get("chips").and_then(Json::as_array).unwrap_or_default().iter().map(|c| c.as_str().map(str::to_owned).ok_or_else(|| bad("chips are names"))).collect::<Result<Vec<_>, _>>()?;
                    Decision::Kind(Kind::Edit { variant: need_variant()?, note, chips })
                }
                "none" => Decision::Kind(Kind::None { note }),
                "back" => Decision::Kind(Kind::Back { to: uint(args, "to", 1, 32)?.ok_or_else(|| bad("back needs 'to': an earlier round"))? as usize }),
                "dislike" => {
                    let at = args.get("at").and_then(Json::as_array).filter(|a| a.len() == 2).and_then(|a| Some([a[0].as_f64()?, a[1].as_f64()?]));
                    Decision::Dislike { variant: need_variant()?, at, note }
                }
                other => return Err(bad(format!("decision '{other}': accept, edit, none, back or dislike"))),
            };
            let round = uint(args, "round", 1, 32)?.ok_or_else(|| bad("'round' is required: the round you decide (gallery_get says which is open)"))? as usize;
            send(tools, session, TasteCall::Decide { id: need(args, "id")?, round, decision, user: user_of(args)? })
        }
        "gallery_round" => {
            keys(&["id", "variants", "centre", "spread"])?;
            let id = need(args, "id")?;
            // The gallery's space and rounds, from its file (read through the engine).
            let raw = read_gallery(tools, session, &id)?;
            let n = raw.rounds.len();
            let mut points = Vec::new();
            if let Some(vs) = args.get("variants").and_then(Json::as_array) {
                if vs.is_empty() || vs.len() > MAX_VARIANTS {
                    return Err(bad(format!("variants: 1..={MAX_VARIANTS} value sets")));
                }
                for v in vs {
                    let vals = raw.space.parse_values(v).map_err(|e| ToolOutput::error(e.to_json().to_string()))?;
                    points.push(raw.space.point_of(&vals));
                }
            } else {
                let centre = match args.get("centre") {
                    Some(c) => raw.space.point_of(&raw.space.parse_values(c).map_err(|e| ToolOutput::error(e.to_json().to_string()))?),
                    None => raw.space.default_point(),
                };
                let spread = args.get("spread").and_then(Json::as_f64).unwrap_or(0.2).clamp(0.01, 1.0);
                let mut rng = specula_core::rng::Rng::new(raw.seed, 0x6167_656e ^ n as u64);
                points = specula_gustus::tune::next_spread(&centre, spread, None, &[], raw.per_round, &mut rng);
            }
            send(tools, session, TasteCall::Round { id, after: n, points })
        }
        "gallery_apply" => {
            keys(&["id"])?;
            send(tools, session, TasteCall::Apply { id: need(args, "id")? })
        }
        "taste_query" => {
            keys(&["subject", "target", "user", "kind", "text", "gallery", "offset", "limit"])?;
            let kind = s(args, "kind")?;
            if kind.as_deref().is_some_and(|k| !matches!(k, "accept" | "pick" | "none" | "dislike" | "back" | "reject")) {
                return Err(bad("kind: accept, pick, none, dislike, back, or reject (none and dislikes)"));
            }
            let q = Query {
                subject: s(args, "subject")?,
                target: s(args, "target")?,
                user: s(args, "user")?,
                kind,
                text: s(args, "text")?.map(|t| specula_gustus::cut(&t, 80)),
                gallery: s(args, "gallery")?,
                offset: uint(args, "offset", 0, 1 << 20)?.unwrap_or(0) as usize,
                limit: uint(args, "limit", 1, preferences::MAX_PAGE as u64)?.unwrap_or(16) as usize,
            };
            send(tools, session, TasteCall::Query(q))
        }
        "taste_suggest" => {
            keys(&["action", "id"])?;
            let action = s(args, "action")?.unwrap_or_else(|| "list".into());
            if !matches!(action.as_str(), "list" | "submit" | "apply" | "dismiss") {
                return Err(bad("action: list, submit, apply or dismiss"));
            }
            send(tools, session, TasteCall::Suggest { action, id: s(args, "id")? })
        }
        "board_measure" => {
            keys(&["board"])?;
            let board = need(args, "board")?;
            if !specula_gustus::valid_name(&board) {
                return Err(bad("board: [a-z0-9_-], 1-40 characters (its images are in assets/taste/boards/<board>/)"));
            }
            let b = measure_board(&tools.procgen.assets, &board)?;
            let summary = board_summary(&b);
            let put = send(tools, session, TasteCall::BoardPut { board: board.clone(), text: b.to_text() });
            if put.is_error {
                return Ok(put);
            }
            let mut j = summary;
            j.set("file", Json::from(crate::taste::board_path(&board)));
            if let Some(v) = text(&put).and_then(|p| p.get("version").cloned()) {
                j.set("version", v);
            }
            ToolOutput::text(j.to_string())
        }
        "board_check" => {
            keys(&["board", "camera", "of", "capture", "width", "height"])?;
            let board = need(args, "board")?;
            let got = send(tools, session, TasteCall::BoardGet { board: board.clone() });
            if got.is_error {
                return Ok(got);
            }
            let b = text(&got).and_then(|j| j.get("text").and_then(Json::as_str).map(Board::parse)).ok_or_else(|| err("board_unreadable", "the board file couldn't be read"))?.map_err(|e| ToolOutput::error(e.to_json().to_string()))?;
            let (px, w, h, source) = match (s(args, "capture")?, s(args, "camera")?) {
                (Some(path), None) => {
                    // Confined before anything is opened (a review probe read outside it).
                    let real = capture_file(&tools.procgen.captures, &path)?;
                    let (rgba, w, h) = read_image(&real)?;
                    (rgba, w, h, Json::obj([("capture", Json::from(path))]))
                }
                (None, Some(cam)) => {
                    let got = send(tools, session, TasteCall::Camera { name: cam.clone(), of: s(args, "of")? });
                    if got.is_error {
                        return Ok(got);
                    }
                    let c = text(&got).and_then(|j| crate::studio::gallery::camera_of(&j)).ok_or_else(|| err("camera_unreadable", "the camera couldn't be resolved"))?;
                    let w = uint(args, "width", 64, 640)?.unwrap_or(384) as u32;
                    let h = uint(args, "height", 64, 640)?.unwrap_or(216) as u32;
                    let shared = tools.studio().ok_or_else(|| err("studio_unavailable", "this server has no asset studio"))?;
                    let px = crate::studio::gallery::render_scene_once(tools, shared, session, &c, w, h)?;
                    (px, w, h, Json::obj([("camera", Json::from(cam))]))
                }
                _ => return Err(bad("give one of camera (a named camera, or \"window\") or capture (a PNG path the engine wrote)")),
            };
            let m = board::measure(&px, w, h).map_err(|e| ToolOutput::error(e.to_json().to_string()))?;
            let findings = b.check(&m);
            let failed = findings.iter().filter(|f| !f.passed).count();
            let stale = stale_images(&tools.procgen.assets, &b);
            let mut j = Json::obj([("board", Json::from(board.as_str()))]);
            if !stale.is_empty() {
                j.set("stale", Json::arr(stale.iter().map(|s| Json::from(s.as_str()))));
                j.set("stale_note", Json::from("these images changed (or are gone) since board_measure: measure the board again"));
            }
            ToolOutput::text(
                Json::obj([
                    ("board", Json::from(board.as_str())),
                    ("stale", j.get("stale").cloned().unwrap_or(Json::Arr(vec![]))),
                    ("source", source),
                    ("passed", Json::Bool(failed == 0)),
                    ("failed", Json::from(failed)),
                    ("findings", Json::arr(findings.iter().map(|f| f.to_json(&board)))),
                    ("measures", m.to_json()),
                ])
                .to_string(),
            )
        }
        "style_onboard" => {
            keys(&["action", "subject", "preset", "user", "open", "quote_notes"])?;
            let action = s(args, "action")?.unwrap_or_else(|| "status".into());
            if !matches!(action.as_str(), "start" | "status" | "finish" | "cancel") {
                return Err(bad("action: start, status, finish or cancel"));
            }
            let preset = s(args, "preset")?;
            if preset.as_deref().is_some_and(|p| specula_gustus::presets::find(p).is_none()) {
                return Err(bad(format!("preset: one of {}", specula_gustus::presets::names().join(", "))));
            }
            send(tools, session, TasteCall::Onboard { action, subject: args.get("subject").cloned(), preset, user: user_of(args)?, open: flag(args, "open")?.unwrap_or(true), quote_notes: flag(args, "quote_notes")? })
        }
        other => return Err(bad(format!("unknown tool '{other}'"))),
    })
}

/// A gallery's file, parsed (the space and rounds an agent's round is planned from).
fn read_gallery(tools: &Tools, session: &SessionInfo, id: &str) -> Result<Gallery, ToolOutput> {
    let (tx, rx) = std::sync::mpsc::channel();
    let out = tools.send(session, Command::Project(crate::project_tools::ProjectCall::Snapshot { path: crate::taste::gallery_path(id), version: None, context: false, reply: tx }));
    if out.is_error {
        return Err(out);
    }
    let snap = rx.try_recv().map_err(|_| err("no_answer", "the engine didn't answer"))?;
    let f = snap.current.ok_or_else(|| err("gallery_not_found", format!("there is no gallery '{id}'")))?;
    Gallery::parse(&f.text).map_err(|e| ToolOutput::error(e.to_json().to_string()))
}

/// Reads and decodes a PNG or JPEG (bounded).
fn read_image(p: &std::path::Path) -> Result<(Vec<u8>, u32, u32), ToolOutput> {
    let meta = std::fs::metadata(p).map_err(|e| err("image_unreadable", format!("{}: {e}", p.display())))?;
    if meta.len() > MAX_IMAGE_FILE {
        return Err(err("image_too_large", format!("{}: {} MB; images up to {} MB are measured", p.display(), meta.len() >> 20, MAX_IMAGE_FILE >> 20)));
    }
    let bytes = std::fs::read(p).map_err(|e| err("image_unreadable", format!("{}: {e}", p.display())))?;
    if bytes.len() >= 24 && &bytes[12..16] == b"IHDR" {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        if u64::from(w) * u64::from(h) > board::MAX_IMAGE_PIXELS {
            return Err(err("image_too_large", format!("{}: {w}x{h}; images up to 4096 x 4096 are measured", p.display())));
        }
    }
    let img = if bytes.starts_with(&[0xFF, 0xD8]) {
        specula_core::jpeg::decode(&bytes).map_err(|e| err("image_unreadable", format!("{}: {e}", p.display())))?
    } else {
        specula_core::png::decode(&bytes).map_err(|e| err("image_unreadable", format!("{}: {e}", p.display())))?
    };
    Ok((img.rgba, img.width, img.height))
}

/// The board's images (`<assets>/taste/boards/<board>/*.png|jpg`), measured. The seam for binary
/// assets through the protocol: a board's source is a folder today; uploaded blobs join here.
fn measure_board(assets: &std::path::Path, board: &str) -> Result<Board, ToolOutput> {
    let dir = assets.join("taste").join("boards").join(board);
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| err("board_not_found", format!("no folder for board '{board}' ({e}): put the reference images in assets/taste/boards/{board}/ (PNG or JPEG)")))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|x| x.to_str()).is_some_and(|x| matches!(x.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg")))
        .collect();
    files.sort();
    if files.is_empty() {
        return Err(err("board_empty", format!("assets/taste/boards/{board}/ has no PNG or JPEG images")));
    }
    if files.len() > board::MAX_IMAGES {
        return Err(err("board_too_big", format!("{} images; a board measures at most {}", files.len(), board::MAX_IMAGES)));
    }
    let mut images = Vec::new();
    for p in files {
        let bytes = std::fs::read(&p).map_err(|e| err("image_unreadable", format!("{}: {e}", p.display())))?;
        let (rgba, w, h) = read_image(&p)?;
        let measures = board::measure(&rgba, w, h).map_err(|e| ToolOutput::error(e.to_json().to_string()))?;
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        images.push(board::Image { file: name, hash: format!("{:016x}", specula_core::hash::hash_bytes(&bytes)), size: [w, h], measures });
    }
    Board::new(board, images).map_err(|e| ToolOutput::error(e.to_json().to_string()))
}

fn board_summary(b: &Board) -> Json {
    let targets = Json::obj(board::SCALARS.iter().map(|(n, ..)| ((*n).to_owned(), Json::arr(b.target(n).map(specula_gustus::r3)))));
    Json::obj([
        ("board", Json::from(b.name.as_str())),
        ("images", Json::arr(b.images.iter().map(|i| Json::obj([("file", Json::from(i.file.as_str())), ("size", Json::arr(i.size.map(Json::from))), ("value_p50", specula_gustus::r3(i.measures.value_p50)), ("chroma_mean", specula_gustus::r3(i.measures.chroma_mean))])))),
        ("targets", targets),
        ("palette", Json::arr(b.palette().iter().take(8).map(|(c, w)| Json::obj([("color", Json::from(specula_gustus::colour::hex(*c))), ("share", specula_gustus::r3(*w))])))),
    ])
}

/// The tools' schemas (their documentation for agents).
pub fn definitions() -> Vec<ToolDef> {
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::arr(required.iter().map(|r| Json::from(*r))))]);
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let any = |d: &str| Json::obj([("description", Json::from(d))]);
    let consult = "Consult the preference store first (taste_query): what this human picked and rejected before, and their notes.";
    vec![
        ToolDef {
            name: "gallery_create".into(),
            description: format!(
                "Choose, don't describe: a gallery of variants spread across a parameter space, for a human to pick from (docs/design/GALLERIES.md). Round 1 is a dispersed spread from the \
                 start point; after each pick the next round is planned by preferential Bayesian optimisation (mode tune) from every pick so far. Variants render through the asset \
                 studio's queue (never the live world's state: scenes are read only); the round's sheet and packet go to the captures folder. The gallery is a project file \
                 (assets/taste/galleries/<id>.json: claims, journal, undo). Waits for round 1 (at most 25 s; wait: false to return at once). {consult}"
            ),
            input_schema: schema(
                vec![
                    ("id", t("string", "Gallery id [a-z0-9_-] (default g<n>)")),
                    ("title", t("string", "The question the human sees, e.g. \"Which moonlight?\"")),
                    ("subject", any("What each variant shows: {recipe: \"recipes/tombstone.json\", params?, seed?, az?, el?, references?: true, rig?: \"game\"|\"neutral\", look?: <base style>}; {scene: {camera: <named camera> | \"window\", of?} or {eye, target, fov_y_deg}} (the live world, read only); {ui: {panel: \"#2b3038\", background?, lines: [{text, color: \"#hex\" | \"$<param>\"}], camera?}} (interface colours such as \"can't afford\" on the game's panel); {stage: {}, look?} (the stage alone)")),
                    ("space", any("1-8 parameters (12 dimensions at most): {name, target, kind: number|colour|choice, min, max, step?, default?} | {base: \"#hex\", spread: {l, c, h°}} (Oklch) | {options: [...], default?}; optional apply: {file: \"style.toml\", toml: \"stage.game.key_color\"} or {file: \"ui/theme.json\", json: \"colors.cant_afford\"} (where the accepted value lives; gallery_apply writes it); optional chips: {\"darker\": -1}. Targets: light.key_color, light.key_intensity, light.key_elevation_deg, light.key_azimuth_deg, light.ambient_color, light.ambient_intensity, exposure, fog.density, fog.color, backdrop, ground, scale.x|y|z|all, recipe.<param>, ui.<name>, preset")),
                    ("mode", t("string", "tune (default: batch expected improvement from the picks), line (Sequential Line Search: one line per round), spread (a shrinking spread around each pick)")),
                    ("variants", t("integer", "Variants per round, 2-12 (default 9: a 3 x 3 sheet)")),
                    ("seed", t("integer", "Seed (default from id and title): the same picks plan the same rounds")),
                    ("tile", t("integer", "Tile size in px, 96-384 (default 256); scene and ui tiles are 16:9")),
                    ("start", any("Start values {name: value} for round 1's first variant (default: approved priors, else the defaults)")),
                    ("apply_on_accept", t("boolean", "\"This one\" also writes the values to their apply targets, as the human (default false)")),
                    ("open", t("boolean", "Also put it on the human's window now (gallery_open)")),
                    ("user", t("string", "Who picks (a label, default \"owner\")")),
                    ("wait", t("boolean", "Wait for round 1 (default true)")),
                    ("image", t("boolean", "Add round 1's sheet as an image (default false: values and measures only, to save tokens)")),
                ],
                &["subject", "space"],
            ),
        },
        ToolDef {
            name: "gallery_get".into(),
            description: "One gallery as data: status (open, rendering, accepted), the round's variants (index, values, measures: value_mean, contrast, chroma_mean; ui tiles' text contrast_ratio and delta_l), its decision, the breadcrumb, the chips, the sheet's path, the accepted values and where they apply, and the window's screen state (with widget rects) when it shows this gallery. image: true adds the sheet.".into(),
            input_schema: schema(vec![("id", t("string", "Gallery id")), ("round", t("integer", "A round to show (default the last)")), ("image", t("boolean", "Add the round's sheet as an image"))], &["id"]),
        },
        ToolDef {
            name: "gallery_delete".into(),
            description: "Deletes a gallery and its picks (its maker's to delete, or any as the owner or a granted lead or reviewer: the owner deletes their private history so). Journaled; the deleter can undo it with world_undo. Not a running style configurator's step.".into(),
            input_schema: schema(vec![("id", t("string", "Gallery id"))], &["id"]),
        },
        ToolDef {
            name: "gallery_list".into(),
            description: "Galleries, newest first: id, title, status, rounds, subject, maker. Paged (after: the last id seen).".into(),
            input_schema: schema(vec![("status", t("string", "open, rendering or accepted")), ("after", t("string", "Continue after this id")), ("limit", t("integer", "1-32 (default 16)"))], &[]),
        },
        ToolDef {
            name: "gallery_open".into(),
            description: "Puts a gallery on the human's window (the gallery screen: modal, mouse, touch, keys and pads; F8 or LB+RB+X toggles it). The gallery's maker, the owner, or a granted lead or reviewer. After the human closes it, agents can't reopen it for 30 s.".into(),
            input_schema: schema(vec![("id", t("string", "Gallery id")), ("user", t("string", "Who will pick (a label, default \"owner\")"))], &["id"]),
        },
        ToolDef { name: "gallery_close".into(), description: "Closes the human's gallery screen (its gallery's maker, the owner, or a granted lead or reviewer).".into(), input_schema: schema(vec![], &[]) },
        ToolDef {
            name: "gallery_wait".into(),
            description: "Waits (at most 25 s) for a decision newer than 'after' (default: the gallery's decision count now) and for the round it asked for; answers the gallery as gallery_get does, with decided: true/false. Poll it while the human picks.".into(),
            input_schema: schema(vec![("id", t("string", "Gallery id")), ("after", t("integer", "A decision count (gallery_get's seq) to wait past")), ("timeout_s", t("integer", "0-25 (default 20)")), ("image", t("boolean", "Add the new round's sheet"))], &["id"]),
        },
        ToolDef {
            name: "gallery_decide".into(),
            description: "A decision as the human would make it at the window, for an agent speaking for them (the owner only: an agent the operator granted owner with --owner NAME; picks are the human's taste). accept (\"This one\"), edit (\"This one, with edits\": note and chips; a new round follows), none (\"None of these\": a new spread), back (to an earlier round), dislike (a variant, optionally the point at [u, v] on its tile; the round stays open).".into(),
            input_schema: schema(
                vec![
                    ("id", t("string", "Gallery id")),
                    ("round", t("integer", "The open round (gallery_get)")),
                    ("decision", t("string", "accept, edit, none, back or dislike")),
                    ("variant", t("integer", "0-based variant (accept, edit, dislike)")),
                    ("note", t("string", "What to change, in a few words (edit, none, dislike; at most 280 characters)")),
                    ("chips", t("array", "Edit chips the gallery offers (gallery_get's chips): darker, brighter, bigger, smaller, simpler, more detail, warmer, cooler, bolder, subtler, foggier, clearer")),
                    ("to", t("integer", "back: the round to show again")),
                    ("at", t("array", "dislike: [u, v] 0..1 on the tile")),
                    ("user", t("string", "Who decided (a label, default \"owner\")")),
                ],
                &["id", "round", "decision"],
            ),
        },
        ToolDef {
            name: "gallery_round".into(),
            description: "An agent's own next round, after reading the human's note: explicit variants [{name: value}...], or a spread (radius 'spread', 0..1 of each range) around 'centre' {name: value}. An undecided round is superseded (the optimiser learns nothing from it).".into(),
            input_schema: schema(vec![("id", t("string", "Gallery id")), ("variants", t("array", "1-12 value sets {name: value}")), ("centre", any("Centre values {name: value} (default the defaults)")), ("spread", t("number", "Spread radius 0.01-1 (default 0.2)"))], &["id"]),
        },
        ToolDef {
            name: "gallery_apply".into(),
            description: "Writes an accepted gallery's values to where their parameters say they live (apply: a style.toml key or a JSON path in a project file), as you: a project-file commit (claims, journal, undo), validated like any edit. The human accepted exactly these values (\"This one\").".into(),
            input_schema: schema(vec![("id", t("string", "Gallery id"))], &["id"]),
        },
        ToolDef {
            name: "taste_query".into(),
            description: "The preference store: every pick, accept, rejection (none of these) and dislike of every gallery in this project, with notes, chips and the values, newest first; filtered by subject (\"recipe:recipes/tombstone.json\", \"scene:overview\", \"ui\"), target (\"light.key_color\"), user, kind, note text, gallery; paged. Also the approved priors and how many suggestions wait. Read it before proposing anything visual.".into(),
            input_schema: schema(
                vec![
                    ("subject", t("string", "Subject label (or part of it)")),
                    ("target", t("string", "A parameter target")),
                    ("user", t("string", "Whose picks")),
                    ("kind", t("string", "accept, pick, none, dislike, back, or reject (none and dislikes)")),
                    ("text", t("string", "Words in the note")),
                    ("gallery", t("string", "One gallery")),
                    ("offset", t("integer", "Skip this many (next_offset)")),
                    ("limit", t("integer", "1-64 (default 16)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "taste_suggest".into(),
            description: "Suggestions from repeated consistent picks (3 or more agreeing for one target): a project-file edit where the parameter says where its value lives (a style.toml key, a theme colour), else a prior for later galleries. Never written silently: submit puts one on the review queue (the human approves it there: the window's review screen or review_decide), apply needs the owner, dismiss records no.".into(),
            input_schema: schema(vec![("action", t("string", "list (default), submit, apply or dismiss")), ("id", t("string", "The suggestion (list shows ids)"))], &[]),
        },
        ToolDef {
            name: "board_measure".into(),
            description: "Measures a reference board: the images the human put in assets/taste/boards/<board>/ (PNG or JPEG, at most 32, 4096 x 4096 each): value histogram and percentiles, saturation (Oklch chroma), a 6-colour palette, contrast, edge density, shadow softness. Their spread becomes targets, written to assets/taste/boards/<board>.json (a project file).".into(),
            input_schema: schema(vec![("board", t("string", "Board name [a-z0-9_-]"))], &["board"]),
        },
        ToolDef {
            name: "board_check".into(),
            description: "Checks a scene against a board's targets, reported like the studio's taste checks (value, target range, rule board:<name>.<measure>, a hint that says which way to go). The scene: a named camera rendered now through the studio (read only), or a capture the engine wrote.".into(),
            input_schema: schema(
                vec![
                    ("board", t("string", "Board name")),
                    ("camera", t("string", "A named camera, or \"window\"")),
                    ("of", t("string", "Whose camera (default yours)")),
                    ("capture", t("string", "A PNG the engine wrote (sight_frame's image.path)")),
                    ("width", t("integer", "Render width 64-640 (default 384)")),
                    ("height", t("integer", "Render height 64-640 (default 216)")),
                ],
                &["board"],
            ),
        },
        ToolDef {
            name: "style_onboard".into(),
            description: "The style configurator and first-run onboarding: from a base style (moonlit, storybook, muted, noir, clean), a short sequence of galleries on the human's window: the base style, the key light, the atmosphere (fog), the interface colours (accent and \"can't afford\"). Each \"This one\" starts the next; after the last the result (a style.toml patched key by key and STYLE.md-style notes) goes on the review queue, and approving it writes both. start (subject: the project's own {recipe} or {scene: {camera}}, default the stage), status, finish (propose now), cancel.".into(),
            input_schema: schema(
                vec![
                    ("action", t("string", "start, status (default), finish or cancel")),
                    ("subject", any("What the galleries show (as gallery_create's subject)")),
                    ("preset", t("string", "The base style to start from (default moonlit when style.toml exists, else clean)")),
                    ("user", t("string", "Who picks (default \"owner\")")),
                    ("open", t("boolean", "Put the first gallery on the window (default true)")),
                    ("quote_notes", t("boolean", "Quote the owner's notes into the tracked notes.md (start or finish; default false: notes.md says what was chosen, never the owner's words). The review submission shows the quoted notes before the owner approves")),
                ],
                &[],
            ),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_schema_and_a_name_agents_can_use() {
        let defs = definitions();
        assert_eq!(defs.len(), NAMES.len());
        for d in &defs {
            assert!(NAMES.contains(&d.name.as_str()), "{}", d.name);
            assert!(d.name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') && d.name.len() <= 64);
            assert!(d.description.len() > 40, "{}: describe it", d.name);
        }
    }

    /// Review probe (minor 1): board_check read files outside the captures folder.
    #[test]
    fn captures_are_confined_before_they_are_opened() {
        let root = std::env::temp_dir().join(format!("specula-capture-test-{}", std::process::id()));
        let run = root.join("run-1");
        std::fs::create_dir_all(&run).unwrap();
        std::fs::write(run.join("a.png"), b"x").unwrap();
        std::fs::write(root.join("outside.png"), b"x").unwrap();
        std::fs::write(run.join("a.txt"), b"x").unwrap();
        assert!(capture_file(&run, &run.join("a.png").display().to_string()).is_ok());
        assert!(capture_file(&run, &root.join("outside.png").display().to_string()).is_err(), "the captures root is outside this run's folder");
        assert!(capture_file(&run, &run.join("..").join("outside.png").display().to_string()).is_err());
        assert!(capture_file(&run, &run.join("a.txt").display().to_string()).is_err());
        assert!(capture_file(&run, &run.join("missing.png").display().to_string()).is_err());
        assert!(capture_file(&run, r"\\server\share\a.png").is_err(), "UNC paths are refused before the disk is touched");
        assert!(capture_file(&run, "a.png").is_ok(), "a name relative to this run's captures");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn boards_are_read_from_their_folder() {
        let dir = std::env::temp_dir().join(format!("specula-board-test-{}", std::process::id()));
        let b = dir.join("taste").join("boards").join("night");
        std::fs::create_dir_all(&b).unwrap();
        let px: Vec<u8> = (0..64 * 64).flat_map(|i| if i % 64 < 32 { [20u8, 24, 40, 255] } else { [180, 190, 230, 255] }).collect();
        std::fs::write(b.join("a.png"), specula_core::png::encode_rgba8(64, 64, &px, specula_core::deflate::Level::Fastest)).unwrap();
        let board = measure_board(&dir, "night").unwrap();
        assert_eq!(board.images.len(), 1);
        assert!(board.images[0].measures.contrast > 0.3);
        assert!(measure_board(&dir, "none").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
