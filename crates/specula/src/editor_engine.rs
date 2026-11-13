//! The editor on the engine thread (docs/design/EDITOR.md): selection, gizmos, the outliner and
//! inspector data, the editor camera, and **every edit as a protocol command**.
//!
//! The window's human is an identity like any agent's: the session [`WINDOW_SESSION`], named
//! and keyed `window:owner` with the owner's authority (`Swarm::admit_window_operator`). An edit
//! is a tool call (`world_set`, `world_batch`, `world_undo`) parsed by `tools::parse` and handled
//! by `Engine::handle_traced` exactly as an agent's request: the write policy's `Rule::World`,
//! claims, versions (`expect_version`), the journal, and one commit per action. There is no other
//! path from the editor into the world.
//!
//! A gizmo drag writes nothing until it ends: the window draws the dragged entities at the
//! preview transform (their draws' matrices patched, [`Engine::editor_patch_draws`]), and the
//! release commits one `world_batch`.

use crate::capture::PickSpec;
use crate::editor::{num, parse_number, Btn, EditorUi, FieldKey, Inspect, OutlinerView, Row, UiAct, ViewIn, MAX_INJECTED, MAX_SELECTION, PAGE_ROWS};
use crate::editor_gizmo::{self as gz, Delta, Handle, Local, Mode, Space, View};
use crate::journal::Writer;
use crate::outline::{Budget, Cursor, Query, Scan};
use crate::sight::{self, Camera, Projection};
use crate::tools::Request;
use crate::Engine;
use specula_core::math::{Mat4, Quat, Vec3};
use specula_core::{dmath, Json};
use specula_gpu::{DrawItem, OverlayQuad};
use specula_platform::Event;
use specula_protocol::{Content, Server, SessionInfo, ToolOutput};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// The window operator's session id (protocol session ids are 32 hex characters: never this).
pub const WINDOW_SESSION: &str = "window:operator";
/// The editor's commands give up after this (as an agent's call would).
const DEADLINE: Duration = Duration::from_secs(10);
/// How long a draw tool's command may take (it validates and patches a recipe, and for a sculpt
/// builds the terrain's collider, on a thread of its own).
const TOOL_DEADLINE: Duration = Duration::from_secs(90);
/// Entities a drag's live preview moves (the dragged ones and everything below them); past it
/// the rest move on release.
pub const PREVIEW_MAX: usize = 4096;
/// A search reads entities for at most this long per window frame (it resumes on the next;
/// [`crate::outline`] checks the clock every few hundred entities and allocates nothing per entity).
pub const SCAN_BUDGET: Duration = Duration::from_micros(500);
/// And at most this many entities per frame, whatever the clock says.
pub const SCAN_MAX_STEPS: usize = 50_000;
/// While a search runs without new matches, the panel shows its progress this often.
const SCAN_SHOW_EVERY: Duration = Duration::from_millis(100);
/// The outliner and inspector are rebuilt at most this often while the world keeps changing.
const REFRESH_EVERY: Duration = Duration::from_millis(250);
/// Expanded outliner entities remembered.
const MAX_EXPANDED: usize = 4096;
/// A press that moves less than this (pixels) is a click; more, a box.
const CLICK_PX: f32 = 4.0;
/// A box selects an entity when at least this share of its visible pixels is inside the box.
pub const BOX_INSIDE: f64 = 0.5;
/// Pixels around a click searched for the nearest entity.
pub const PICK_RADIUS: i64 = 3;
/// Replies the editor waits on at once (its own commands; more are refused, never queued).
const MAX_WAITING: usize = 32;
/// `editor_input` calls waiting for their frames to run.
const MAX_INPUT_WAITERS: usize = 8;
/// A held `ui_act` answer is sent this long before its call's deadline (the caller gives up at
/// the deadline: the answer as it stood must reach it first).
const ACT_ANSWER_MARGIN: Duration = Duration::from_millis(400);

/// The window operator's session.
pub fn window_session() -> SessionInfo {
    SessionInfo { id: WINDOW_SESSION.into(), client_name: "window".into(), client_version: String::new(), protocol_version: String::new() }
}

/// Why the editor sent a command (what its answer means).
#[derive(Clone, Debug)]
enum Why {
    /// An edit the human can undo: what it did, in words.
    Edit(String),
    /// Undoing the human's journal entry.
    Undo(u64),
    /// Undoing an undo (redo).
    Redo(u64),
    /// "Ask the agents…": the board task the human's request became.
    Ask,
}

/// Where a brush would touch: the selection's surface under the pointer.
struct BrushTarget {
    entity: u64,
    /// The entity's recipe (the file a stroke is written into) and its path (a sculpt's
    /// collider, when the terrain has one).
    recipe: String,
    collider: Option<String>,
    /// The point, in the frame strokes are written in: the terrain's own, or the model's.
    point: [f32; 3],
    /// That frame in the world.
    to_world: Mat4,
    /// A terrain's offset (its strokes' `at` includes it).
    offset: [f32; 2],
    /// World metres per unit of that frame, along the surface and in height ([`frame_scale`]):
    /// the brush's sizes are world metres, a stroke is written in the frame's units.
    scale: (f32, f32),
}

/// A stroke in progress: its samples, and what it is on (kept from where it began: a stroke
/// never crosses to another entity).
struct BrushStroke {
    stroke: crate::editor_brush::Stroke,
    entity: u64,
    recipe: String,
    collider: Option<String>,
    to_world: Mat4,
    offset: [f32; 2],
    scale: (f32, f32),
}

/// World metres per unit of a frame: along its ground (the mean of its x and z axes; with
/// `solid`, of all three: a paint brush is a sphere) and in height (its y axis). An entity
/// scaled unevenly gets the mean: its brush is round in its own space.
/// (Read from the matrix's own axes: a point transformed and subtracted loses the scale's
/// digits when the entity is far from the origin. Review R2 of m5-eddraw, N5.)
fn frame_scale(to_world: &Mat4, solid: bool) -> (f32, f32) {
    let len = |c: [f32; 4]| (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    let (x, y, z) = (len(to_world.cols[0]), len(to_world.cols[1]), len(to_world.cols[2]));
    (if solid { (x + y + z) / 3.0 } else { (x + z) / 2.0 }, y)
}

#[derive(Default)]
struct Outliner {
    expanded: BTreeSet<u64>,
    /// Each page's starting cursor (page 0: the start, or the revealed entity).
    starts: Vec<Cursor>,
    page: usize,
    /// The page shows the tree from this entity (revealed from the viewport), not the start.
    from: Option<String>,
    /// Search: the scan in progress and the page's rows so far.
    scan: Option<(Scan, Vec<Row>)>,
    /// When the panel last showed the scan's progress.
    scan_shown: Option<Instant>,
    built: Option<(u64, String, usize, u64)>,
    built_at: Option<Instant>,
    /// Bumped when expand state or paging changes.
    epoch: u64,
}

/// The `editor_input` call whose frames drive the editor: its edits are charged to that call
/// (one call, one commit), traced under it, and journaled with the agent as `via`.
#[derive(Clone)]
struct Driver {
    /// The tool call (commit counting) and its trace context (the journal's `call`).
    call: u64,
    trace: specula_core::trace::Ctx,
    /// The agent that sent it.
    by: String,
    /// Frames it sent.
    frames: usize,
}

/// One call for the editor's tool thread: a caller-thread tool, as the window's owner.
struct ToolJob {
    tools: std::sync::Arc<crate::tools::Tools>,
    tool: &'static str,
    args: Json,
    call: u64,
    via: Option<String>,
    trace: specula_core::trace::Ctx,
    /// (A tool's answer goes through a `jobs::Reply`, as every other one does.)
    answer: crate::jobs::Reply,
    /// Counted down when the call is done (`Rt::tool_running`).
    running: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

/// A selected entity as a drag found it.
struct Item {
    id: u64,
    version: u64,
    start: Local,
    world: Mat4,
    parent: Mat4,
}

/// A gizmo drag in progress.
struct GizmoDrag {
    drag: gz::Drag,
    mode: Mode,
    space: Space,
    pivot: Vec3,
    items: Vec<Item>,
    /// Entity → index in `items` of the dragged entity it moves with (descendants included).
    preview: BTreeMap<u64, usize>,
    delta: Delta,
    /// The world generation it began in: a load replaces the entities it holds by id, so it
    /// never commits (or previews) across one.
    generation: u64,
}

/// The editor's orbit camera.
#[derive(Clone, Copy, Debug)]
struct Orbit {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
    fov: f32,
    /// An orthographic view's height (2D games): no orbit, pan and zoom only.
    ortho: Option<f32>,
}

impl Orbit {
    fn of(c: &Camera) -> Orbit {
        let d = c.eye - c.target;
        let distance = d.length().max(0.2);
        let pitch = dmath::asin((d.y / distance).clamp(-1.0, 1.0));
        let yaw = dmath::atan2(d.x, d.z);
        let ortho = match c.projection {
            Projection::Ortho { height_m } => Some(height_m),
            Projection::Perspective => None,
        };
        Orbit { target: c.target, yaw, pitch, distance, fov: c.fov_y_deg, ortho }
    }

    fn camera(&self) -> Camera {
        let (sy, cy) = dmath::sin_cos(self.yaw);
        let (sp, cp) = dmath::sin_cos(self.pitch);
        let dir = Vec3::new(sy * cp, sp, cy * cp);
        let mut c = Camera::new(self.target + dir * self.distance, self.target, self.fov);
        if let Some(h) = self.ortho {
            c.projection = Projection::Ortho { height_m: h };
        }
        c
    }
}

#[derive(Default)]
struct Rt {
    /// Commands to send at the pump, each with the `editor_input` call that drove it (`None`:
    /// the human's own input).
    outbox: Vec<(&'static str, Json, Why, Option<Driver>)>,
    /// Commands sent and not answered yet: the answer's channel, why it was sent, when to give
    /// up on it, and the tool call that drove it (0: the human's own hand).
    waiting: Vec<(Receiver<ToolOutput>, Why, Instant, u64, &'static str)>,
    /// Presses an agent's frames made that were not taken (the human's hand held the viewport,
    /// or the press took it back from a quiet hold), by the tool call that sent them: its
    /// answer says so. At most `MAX_INPUT_WAITERS * 4` calls are remembered.
    refused_presses: std::collections::BTreeMap<u64, u32>,
    /// Calls the editor's tool thread has not finished (queued or running). A world
    /// replacement empties `waiting`; this still counts the call that is running, so "one
    /// stroke in flight" holds across it.
    tool_running: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// `ui_act` calls on the editor whose answer waits for how their edit went: the reply, the
    /// answer as it stood (it says the outcome isn't known), the call and its deadline.
    act_waiters: Vec<(crate::jobs::Reply, ToolOutput, u64, Instant)>,
    pick: Option<(Receiver<ToolOutput>, bool, Instant)>,
    /// A pick asked for this frame: `(rect, click, add)`, sent at the pump.
    want_pick: Option<([u32; 4], bool, bool)>,
    drag: Option<GizmoDrag>,
    /// The draw tools: the stroke the brush is making, and where the brush is over the
    /// selection's surface (world space, with the brush's radius there: the overlay's ring).
    stroke: Option<BrushStroke>,
    /// The last place the brush in hand touched: the entity, the point (world space) and the
    /// brush's radius there. Kept while the pointer is elsewhere (on the toolbar's Ask button):
    /// "Ask the agents" with a brush in hand is about this spot.
    brush_spot: Option<(u64, Vec3, f32)>,
    /// The editor's tool thread (started at the first stroke): its queue of calls.
    tool_thread: Option<std::sync::mpsc::Sender<ToolJob>>,
    /// (`true`: on a terrain, where the ring lies on the ground; on a model it faces the camera.)
    brush_hover: Option<(Vec3, f32, bool)>,
    /// The left press in the viewport (a click or a box), and the right or middle drag's last
    /// pointer (orbit, pan).
    press: Option<[f32; 2]>,
    nav: Option<[f32; 2]>,
    cam: Option<Orbit>,
    /// The window camera when the editor opened (handed back on close).
    game_camera: Option<Option<Camera>>,
    outliner: Outliner,
    /// The inspector's entity as last filled: (id, version, world revision).
    inspected: Option<(u64, u64, u64)>,
    inspected_at: Option<Instant>,
    /// The last edit's answer, for editor_state.
    last: Option<Json>,
    /// `editor_input` calls answered once their frames have run (and one more).
    input_waiters: Vec<(crate::jobs::Reply, Instant, u64)>,
    /// The `editor_input` call whose frame this window frame runs (0: none) and its driver, and
    /// the calls that already made their one edit (one call, one commit: AGENTS rule 14).
    input_call: (u64, Option<Driver>),
    edited_calls: std::collections::VecDeque<u64>,
    /// Submissions that waited for review when the editor last looked (it says so when more do).
    pending_seen: usize,
    /// `editor_input` calls so far (their frames are tagged with it) and their drivers.
    input_seq: u64,
    input_tool_calls: BTreeMap<u64, Driver>,
    /// The gamepads as this frame read them (reused).
    pads: Vec<specula_platform::gamepad::GamepadState>,
    /// The window's size at the last frame, and whether a frame ran yet.
    size: (u32, u32),
    framed: bool,
    /// The view (camera, size) the gizmo was last drawn in, for agents.
    gizmo_px: Option<(View, gz::Frame, Mode)>,
    /// Names drawn on the viewport's swarm marks (claim holders, the highlighted entry's agent).
    labeler: crate::sight_overlay::Labeler,
}

/// The editor: what the window shows ([`EditorUi`]) and its engine-side state.
#[derive(Default)]
pub struct Editor {
    pub ui: EditorUi,
    rt: Rt,
}

impl Editor {
    /// Whether the input being handled now came from an owner's agent (`editor_input` or
    /// `ui_act` frames), not the window's own devices.
    pub(crate) fn input_is_injected(&self) -> bool {
        self.rt.input_call.1.is_some()
    }

    /// A drag is moving entities on screen (the window's draw list is patched).
    pub fn previewing(&self) -> bool {
        self.rt.drag.as_ref().is_some_and(|d| !d.delta.is_identity())
    }
}

/// The text of a tool's answer, parsed (an unparsable one as `{"message": text}`).
/// A held `ui_act` answer with how its edit went in place of "not known yet": `edit.done`,
/// `edit.status` (the editor's status line), and the journal entry or the refusal's code.
fn with_outcome(answer: &ToolOutput, out: Option<&ToolOutput>, status: &str) -> ToolOutput {
    let mut j = answer_json(answer);
    let Json::Obj(fields) = &mut j else { return ToolOutput::text(j.to_string()) };
    let queued = fields.iter().find(|(k, _)| k == "edit").and_then(|(_, e)| e.get("queued").cloned()).unwrap_or(Json::Null);
    let mut edit = vec![("queued", queued), ("done", Json::from(out.is_some_and(|o| !o.is_error))), ("status", Json::from(status))];
    match out {
        Some(o) => {
            let a = answer_json(o);
            for key in if o.is_error { ["error", "retry"] } else { ["journal", "version"] } {
                if let Some(v) = a.get(key) {
                    edit.push((key, v.clone()));
                }
            }
        }
        None => edit.push(("error", Json::from("no_answer"))),
    }
    fields.retain(|(k, _)| k != "edit");
    fields.push(("edit".to_owned(), Json::obj(edit)));
    ToolOutput::text(j.to_string())
}

fn answer_json(out: &ToolOutput) -> Json {
    let text = out.content.iter().find_map(|c| match c {
        Content::Text(t) => Some(t.as_str()),
        _ => None,
    });
    match text {
        Some(t) => Json::parse(t).unwrap_or_else(|_| Json::obj([("message", Json::from(t))])),
        None => Json::obj([("message", Json::from("(no text)"))]),
    }
}

/// A refusal in words: its code and message, plainly (whose claim, which version).
/// `text` cut to `max` characters (with an ellipsis).
fn short_text(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

pub fn explain(err: &Json) -> String {
    let code = err.get("error").and_then(Json::as_str).unwrap_or("refused");
    let message = err.get("message").and_then(Json::as_str).unwrap_or("");
    let lead = match code {
        "claimed" => "Refused: claimed".to_owned(),
        "claims_overlap" => "Refused: claims overlap".to_owned(),
        "stale_version" => "Refused: changed meanwhile".to_owned(),
        "transient" | "transient_entity" => "Refused: the game's own entity".to_owned(),
        "undo_conflict" => "Can't undo".to_owned(),
        other => format!("Refused ({other})"),
    };
    let mut s = format!("{lead}. {message}");
    // A file that would not load says what is wrong with it: the first problem, in the status
    // line (the draw tools' strokes go into recipes: "would not load (1 error)" alone told the
    // human nothing they could act on).
    if let Some(first) = err.get("problems").and_then(Json::as_array).and_then(|a| a.first()) {
        let what = first.get("message").and_then(Json::as_str).map_or_else(|| first.to_string(), str::to_owned);
        let at = first.get("path").or_else(|| first.get("at")).and_then(Json::as_str).map_or(String::new(), |p| format!(" (at {p})"));
        s.push_str(&format!(" First problem: {}{at}.", short_text(&what, 220)));
    }
    // A changed-meanwhile refusal names who changed it (conflict help's `theirs`).
    let changes = err.get("conflict").and_then(|c| c.get("theirs")).and_then(|t| t.get("changes")).and_then(Json::as_array);
    if let Some(changes) = changes.filter(|c| !c.is_empty()) {
        let who: Vec<String> = changes
            .iter()
            .take(3)
            .filter_map(|c| Some(format!("{} (journal {})", c.get("by")?.as_str()?, c.get("journal").and_then(Json::as_u64).unwrap_or(0))))
            .collect();
        if !who.is_empty() {
            s.push_str(&format!(" Changed since by {}; your value was not written.", who.join(", ")));
        }
    }
    if let Some(owner) = err.get("owner").and_then(Json::as_str) {
        if !message.contains(owner) {
            s.push_str(&format!(" (held by {owner})"));
        }
    }
    s
}

/// The pixels a pick reads (`[x0, y0, x1, y1)`, window pixels): around a click, the
/// [`PICK_RADIUS`] square; a box, the box.
pub fn pick_read_rect(spec: &PickSpec) -> [u32; 4] {
    if spec.click {
        let r = PICK_RADIUS as u32;
        let (cx, cy) = (spec.rect[0], spec.rect[1]);
        [cx.saturating_sub(r), cy.saturating_sub(r), cx.saturating_add(r + 1), cy.saturating_add(r + 1)]
    } else {
        spec.rect
    }
}

/// Entity ids seen in a pick. `img` holds the render's dense ids (`entities[i]` drew as id
/// `i + 1`) for the rectangle at `origin` ([`pick_read_rect`]); `totals[i]` is id `i + 1`'s
/// visible pixels in the whole view (a box only). A click takes the entity at the pixel, else the
/// nearest within [`PICK_RADIUS`]; a box every entity **mostly inside** it (at least
/// [`BOX_INSIDE`] of its visible pixels), most pixels first, at most [`MAX_SELECTION`]. HLOD
/// proxies (merged distant cells) are never picked. Work is the rectangle's pixels plus one slot
/// per drawn entity: never a walk of the whole image.
pub fn pick_ids(spec: &PickSpec, img: &specula_gpu::IdImage, origin: [u32; 2], entities: &[u64], totals: &[u64]) -> Json {
    let (w, h) = (i64::from(img.width), i64::from(img.height));
    let (ox, oy) = (i64::from(origin[0]), i64::from(origin[1]));
    let entity = |id: u32| -> Option<u64> {
        let e = *entities.get((id as usize).checked_sub(1)?)?;
        (!sight::is_proxy_owner(e)).then_some(e)
    };
    // A window pixel's id (0 outside what was read).
    let at = |x: i64, y: i64| -> u32 {
        let (lx, ly) = (x - ox, y - oy);
        if lx < 0 || ly < 0 || lx >= w || ly >= h {
            0
        } else {
            img.ids.get((ly * w + lx) as usize).copied().unwrap_or(0)
        }
    };
    let mut ids: Vec<u64> = Vec::new();
    let mut pixels: Vec<u64> = Vec::new();
    if spec.click {
        let (cx, cy) = (i64::from(spec.rect[0]), i64::from(spec.rect[1]));
        let mut best: Option<(i64, u64)> = None;
        for dy in -PICK_RADIUS..=PICK_RADIUS {
            for dx in -PICK_RADIUS..=PICK_RADIUS {
                let d = dx * dx + dy * dy;
                if d > PICK_RADIUS * PICK_RADIUS || best.is_some_and(|(bd, _)| bd <= d) {
                    continue;
                }
                if let Some(e) = entity(at(cx + dx, cy + dy)) {
                    best = Some((d, e));
                }
            }
        }
        if let Some((_, e)) = best {
            ids.push(e);
            pixels.push(1);
        }
    } else {
        // Counts per dense id (ids past the entities, or 0, are nothing).
        let mut counts = vec![0u64; entities.len() + 1];
        for &v in &img.ids {
            if let Some(c) = counts.get_mut(v as usize) {
                *c += 1;
            }
        }
        let mut found: Vec<(u64, u64)> = counts
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, n)| **n > 0)
            .filter(|(id, n)| totals.get(id - 1).is_none_or(|t| **n as f64 >= *t as f64 * BOX_INSIDE))
            .filter_map(|(id, n)| entity(id as u32).map(|e| (e, *n)))
            .collect();
        found.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        found.truncate(MAX_SELECTION);
        for (e, n) in found {
            ids.push(e);
            pixels.push(n);
        }
    }
    Json::obj([
        ("ids", Json::arr(ids.iter().map(|i| Json::from(*i)))),
        ("pixels", Json::arr(pixels.iter().map(|i| Json::from(*i)))),
        ("read", Json::arr([origin[0], origin[1], img.width, img.height].map(Json::from))),
        ("rect", Json::arr(spec.rect.map(Json::from))),
    ])
}

fn v3(v: Vec3) -> Json {
    Json::arr([v.x, v.y, v.z].map(|c| Json::from(f64::from(c))))
}

fn mat_of(cols: &[f32; 16]) -> Mat4 {
    Mat4 { cols: [[cols[0], cols[1], cols[2], cols[3]], [cols[4], cols[5], cols[6], cols[7]], [cols[8], cols[9], cols[10], cols[11]], [cols[12], cols[13], cols[14], cols[15]]] }
}

impl Engine {
    /// The window operator's writer (claims, undo ownership).
    fn window_writer(&self) -> (String, String) {
        let s = window_session();
        (self.swarm.agent_name(&s), self.swarm.key(&s))
    }

    /// One window frame of the editor, after the review and gallery screens (`blocked`: one of
    /// them holds the input). Returns whether it holds the window (the game then gets no input).
    pub(crate) fn editor_frame(&mut self, size: (u32, u32), blocked: bool) -> bool {
        let was_open = self.editor.ui.is_open();
        self.editor.rt.size = size;
        self.editor.rt.framed = true;
        let minimized = size.0 == 0 || size.1 == 0;
        // Simulated input (editor_input) is the editor's alone: never the review or gallery
        // screens' or the game's (their decisions would be recorded as the human's). It waits while
        // another screen holds the window or the window is minimized.
        // While the human types into the "Ask the agents…" box, injected frames wait (their
        // calls are refused at once, `window_held`): only the human's own input acts then.
        if self.editor.ui.asking() && blocked {
            // A screen took the window (a lead's or the owner's review or gallery screen): the
            // box closes and what was typed is dropped, never sent.
            self.editor.ui.ask.close("engine_screen");
            self.editor.ui.set_status("The Ask box closed: another screen took the window. Nothing was sent.", false);
        }
        let asking = self.editor.ui.asking();
        let injected = if blocked || minimized || asking { None } else { self.editor.ui.next_injected() };
        let (input_id, extra) = injected.unwrap_or((0, Vec::new()));
        // Only commands caused by the injected events carry their call's driver (review R2, probe
        // 9): the human's own events run in a pass of their own first, with none, and an empty
        // `wait` frame has no injected pass at all.
        let driver = if extra.is_empty() { None } else { self.editor.rt.input_tool_calls.get(&input_id).cloned() };
        self.editor.rt.input_call = (0, None);
        if !was_open && !self.editor.ui.loaded() && input_id == 0 && !self.editor_wants_input() {
            return false;
        }
        if was_open && (blocked || minimized) {
            // An interruption cancels a drag, never commits it half done.
            if self.editor.rt.drag.is_some() {
                self.editor_cancel_drag();
            }
            // A box or click in progress is let go too (it would pick on the next frame).
            self.editor.rt.press = None;
        }
        // Polled while open (the game gets none then); closed, the states the game's frame polled
        // last.
        let mut pads = std::mem::take(&mut self.editor.rt.pads);
        pads.clear();
        pads.extend_from_slice(self.logic.pad_states(was_open));
        // Pass 1: the window's own events (and the pads), as the human. Pass 2: the injected
        // frame's events, as the agent that sent them (its call is charged, its name is `via`).
        let mut ever_open = was_open;
        for injected_pass in [false, true] {
            if injected_pass && extra.is_empty() {
                break;
            }
            let open_before = self.editor.ui.is_open();
            self.editor.rt.input_call = if injected_pass { (input_id, driver.clone()) } else { (0, None) };
            self.editor.ui.injected_pass = injected_pass;
            let (acts, view) = if injected_pass {
                self.editor.ui.frame(&extra, &pads, size, blocked)
            } else if asking && !blocked && !minimized {
                // The box takes the human's keys and pointer; the panels lay out and act on none.
                let out = self.editor.ui.ask.frame_with_pads(&self.input, &pads, size, crate::editor::THEME, crate::editor::system_fonts);
                // (The pointer is the box's: the panels see it leave, so no tooltip stays drawn
                // under where it was. Review R1 of m5-edswarm, minor 12.)
                // (And their holds in the viewport are let go: its releases are not read here.)
                self.editor.ui.release_human_holds();
                let r = self.editor.ui.frame(&[Event::MouseMove { x: -1, y: -1 }], &[], size, blocked);
                if let Some(out) = out {
                    self.editor_ask_out(out);
                }
                r
            } else {
                self.editor.ui.frame(&self.input, &pads, size, blocked)
            };
            if self.editor.ui.is_open() && !open_before {
                self.editor_opened();
            }
            for a in acts {
                self.editor_act(a);
            }
            self.editor_note_holds(&view, injected_pass);
            // While one side holds a button the viewport is that side's: the other's pass
            // moves nothing in it (its pointer would bend the stroke, and its focus loss
            // would cancel the gizmo drag). The human's hand and an agent's frames, alike.
            let theirs = if injected_pass { self.editor.ui.human_holds() } else { self.editor.ui.agent_holds() };
            if self.editor.ui.is_open() && !blocked && !minimized && !theirs {
                if view.focus_lost {
                    self.editor_cancel_drag();
                }
                self.editor_viewport(&view, size);
            }
            ever_open |= self.editor.ui.is_open();
        }
        self.editor.ui.injected_pass = false;
        self.editor.rt.input_call = (0, None);
        self.editor.rt.pads = pads;
        let holds = ever_open;
        if holds {
            self.input.clear();
        }
        holds
    }

    /// Whether the editor, closed and not loaded yet, must read this frame's input: its toggle
    /// key, or a pad holding the toggle chord's buttons (loading the fonts waits for that).
    fn editor_wants_input(&mut self) -> bool {
        const VK_F7: u32 = 0x76;
        if self.input.iter().any(|e| matches!(e, Event::KeyDown { code: VK_F7, repeat: false })) {
            return true;
        }
        use specula_platform::gamepad::GamepadButton as B;
        self.logic.pad_states(false).iter().any(|p| p.held(B::LeftBumper) && p.held(B::RightBumper) && p.held(B::Start))
    }

    fn editor_opened(&mut self) {
        let cam = self.window_camera.unwrap_or_else(|| sight::auto_camera(&self.world, 50.0));
        self.editor.rt.game_camera = Some(self.window_camera);
        self.editor.rt.cam = Some(Orbit::of(&cam));
        self.editor.rt.outliner.built = None;
        self.editor.rt.inspected = None;
        if self.window_follow.is_some() {
            self.release_window_camera("editor");
        }
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "editor.open", by = crate::authority::WINDOW_OWNER);
    }

    fn editor_close(&mut self) {
        self.editor_cancel_drag();
        self.editor.ui.close();
        if let Some(cam) = self.editor.rt.game_camera.take() {
            self.window_camera = cam;
        }
        self.editor.rt.cam = None;
        self.editor.rt.gizmo_px = None;
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "editor.close", by = crate::authority::WINDOW_OWNER);
    }

    /// The world was replaced wholesale (`world_load`, `save_read`, a recovered writer's bug;
    /// called by `Engine::world_changed_wholesale`): every entity id and journal id the editor
    /// holds is the old world's. The drag is cancelled, the selection, inspector, outliner and
    /// pick in flight go, commands not yet sent are dropped, and undo and redo start over.
    pub(crate) fn editor_world_replaced(&mut self) {
        let rt = &mut self.editor.rt;
        let had = rt.drag.is_some() || rt.pick.is_some() || rt.want_pick.is_some() || !rt.outbox.is_empty() || !self.editor.ui.selection.is_empty() || !self.editor.ui.undo.is_empty();
        rt.drag = None;
        rt.press = None;
        rt.pick = None;
        rt.want_pick = None;
        rt.outbox.clear();
        rt.waiting.clear();
        // A stroke in hand, and where the brush last touched, name entities of the world that
        // was (held `ui_act` answers are sent as they stood at the next pump: their edit is gone).
        rt.stroke = None;
        rt.brush_hover = None;
        rt.brush_spot = None;
        rt.inspected = None;
        rt.gizmo_px = None;
        let epoch = rt.outliner.epoch + 1;
        rt.outliner = Outliner { epoch, ..Outliner::default() };
        self.editor.ui.world_replaced();
        if had {
            self.editor.ui.set_status("The world was replaced (world_load or save_read): selection, drag and undo history start over; nothing was written.", false);
        }
    }

    fn editor_cancel_drag(&mut self) {
        if self.editor.rt.drag.take().is_some() {
            self.editor.ui.set_status("Drag cancelled: nothing was written.", true);
        }
        self.editor.rt.press = None;
    }

    fn editor_act(&mut self, a: UiAct) {
        match a {
            UiAct::Close => self.editor_close(),
            UiAct::Mode(m) => {
                self.editor_cancel_drag();
                self.editor.ui.mode = m;
                self.editor.ui.changed();
            }
            UiAct::SpaceToggle => {
                self.editor.ui.space = if self.editor.ui.space == Space::World { Space::Local } else { Space::World };
                self.editor.ui.changed();
            }
            UiAct::SnapToggle => {
                self.editor.ui.snap.on = !self.editor.ui.snap.on;
                self.editor.ui.changed();
            }
            UiAct::Undo => match self.editor.ui.undo.pop() {
                Some(seq) => {
                    if !self.editor_send("world_undo", Json::obj([("ids", Json::arr([Json::from(seq)]))]), Why::Undo(seq)) {
                        // Not sent: still the human's to undo.
                        self.editor.ui.undo.push(seq);
                    }
                }
                None => self.editor.ui.set_status("Nothing of yours to undo.", true),
            },
            UiAct::Redo => match self.editor.ui.redo.pop() {
                Some(seq) => {
                    if !self.editor_send("world_undo", Json::obj([("ids", Json::arr([Json::from(seq)]))]), Why::Redo(seq)) {
                        self.editor.ui.redo.push(seq);
                    }
                }
                None => self.editor.ui.set_status("Nothing to redo.", true),
            },
            UiAct::FrameSelection => self.editor_frame_selection(),
            UiAct::Delete => self.editor_delete(),
            UiAct::Escape => {
                if self.editor.ui.brush_down() {
                    // The brush is put down first (a stroke in progress goes with it, unsent).
                    self.editor.ui.set_status("Brush put down.", true);
                    self.editor.ui.changed();
                } else if self.editor.rt.drag.is_some() || self.editor.rt.press.is_some() {
                    self.editor_cancel_drag();
                } else if !self.editor.ui.selection.is_empty() {
                    self.editor.ui.select(&[], false);
                }
            }
            UiAct::OlTop => {
                let o = &mut self.editor.rt.outliner;
                o.page = 0;
                o.from = None;
                o.starts.clear();
                o.built = None;
                o.scan = None;
            }
            UiAct::OlPrev => {
                let o = &mut self.editor.rt.outliner;
                o.page = o.page.saturating_sub(1);
                o.built = None;
                o.scan = None;
            }
            UiAct::OlNext => {
                let o = &mut self.editor.rt.outliner;
                if self.editor.ui.outliner.has_next {
                    o.page += 1;
                    o.built = None;
                    o.scan = None;
                }
            }
            UiAct::OlExpand(r) => {
                if let Some(row) = self.editor.ui.outliner.rows.get(r) {
                    let o = &mut self.editor.rt.outliner;
                    if !o.expanded.remove(&row.id) && o.expanded.len() < MAX_EXPANDED {
                        o.expanded.insert(row.id);
                    }
                    o.epoch += 1;
                    // Later pages start elsewhere now.
                    o.starts.truncate(o.page + 1);
                    o.built = None;
                }
            }
            UiAct::OlSelect(r, add) => {
                if let Some(id) = self.editor.ui.outliner.rows.get(r).map(|row| row.id) {
                    self.editor_cancel_drag();
                    self.editor.ui.select(&[id], add);
                    self.editor.rt.outliner.built = None;
                    // As a click in the viewport says (the line would otherwise go on showing
                    // the last edit's outcome, about another entity: found by the capture check).
                    let n = self.editor.ui.selection.len();
                    self.editor.ui.set_status(if n == 1 { format!("Selected {}.", self.world.path_of(id)) } else { format!("Selected {n} entities.") }, true);
                }
            }
            UiAct::Search => {
                let o = &mut self.editor.rt.outliner;
                o.page = 0;
                o.starts.clear();
                o.scan = None;
                o.built = None;
            }
            UiAct::Commit(f, text, base) => self.editor_commit_field(f, &text, base.map(|b| *b)),
            UiAct::Step(f, dir) => {
                let Some(i) = self.editor.ui.inspect.clone() else { return };
                let snap = self.editor.ui.snap;
                let step = match f {
                    FieldKey::Pos(_) => snap.translate,
                    FieldKey::Rot(_) => snap.rotate,
                    FieldKey::Scale(_) => snap.scale,
                    FieldKey::Color(_) => 0.05,
                    FieldKey::Name | FieldKey::Text => return,
                };
                let v = i.value(f) + dir * step;
                let text = num(if matches!(f, FieldKey::Pos(_) | FieldKey::Rot(_)) && snap.on { gz::snap_to(v, step) } else { v });
                // A step starts and ends with the press: its base is the inspector shown now.
                self.editor_commit_field(f, &text, Some(i));
            }
            UiAct::ToggleVisible => {
                let Some(i) = self.editor.ui.inspect.clone() else { return };
                let args = Json::obj([("id", Json::from(i.id)), ("visible", Json::from(!i.visible)), ("expect_version", Json::from(i.version))]);
                let what = format!("{} {}", if i.visible { "Hid" } else { "Showed" }, i.path);
                let _ = self.editor_send("world_set", args, Why::Edit(what));
            }
            UiAct::Swarm(s) => self.editor_swarm_act(s),
            UiAct::AskOpen => self.editor_ask_open(),
            UiAct::Brush(a) => {
                // A gizmo drag in progress is let go of first: the pointer changes hands.
                self.editor_cancel_drag();
                let path = matches!(a, crate::editor::BrushAct::Path);
                self.editor.ui.brush_act(a);
                if path {
                    self.editor_path_picked();
                } else if let Some(b) = &self.editor.ui.brush {
                    let on = if b.kind.on_terrain() { "the selected terrain" } else { "the selected model" };
                    let hint = if b.kind.is_path() { String::new() } else { format!(" Drag on {on}.") };
                    self.editor.ui.set_status(format!("{}.{hint}", b.label()), true);
                }
                self.editor.ui.changed();
            }
        }
    }

    /// A typed (or stepped) inspector value: one `world_set` whose `expect_version` (and other
    /// components) are `base`'s, the inspector as it was when the edit began: an agent's change
    /// made since refuses it (`stale_version`, naming who), never silently overwritten.
    fn editor_commit_field(&mut self, f: FieldKey, text: &str, base: Option<Inspect>) {
        let Some(i) = base else {
            self.editor.ui.set_status("Not set: nothing was inspected when you began typing.", false);
            return;
        };
        if self.editor.ui.primary() != Some(i.id) {
            self.editor.ui.set_status("Not set: the selection changed since you began typing.", false);
            return;
        }
        let mut args = Json::obj([("id", Json::from(i.id)), ("expect_version", Json::from(i.version))]);
        let what = if f == FieldKey::Text {
            // The text's component as it is now, with the words typed (and a caret, if it is
            // being edited, at their end); the base version refuses it if anyone changed it since.
            let Some(crate::two_d::TwoD::Text(spec)) = self.world.get(i.id).and_then(|e| e.two_d.as_ref()) else {
                self.editor.ui.set_status("Not set: it is no longer a text.", false);
                return;
            };
            if spec.text == text {
                self.editor.ui.set_status("The text is unchanged: nothing was sent.", true);
                return;
            }
            let mut j = spec.to_json();
            j.set("text", Json::from(text));
            if spec.edit.is_some() {
                let end = Json::from(text.len() as u64);
                j.set("editing", Json::obj([("caret", end.clone()), ("anchor", end)]));
            }
            args.set("text", j);
            format!("Set the text of {}", i.path)
        } else if f == FieldKey::Name {
            let name = text.trim();
            if name.is_empty() {
                self.editor.ui.set_status("A name can't be empty.", false);
                return;
            }
            args.set("rename", Json::from(name));
            format!("Renamed {} to {name}", i.path)
        } else {
            let v = match parse_number(text) {
                Ok(v) => v,
                Err(e) => {
                    self.editor.ui.set_status(format!("Not set: {e}."), false);
                    return;
                }
            };
            let (key, mut value) = match f {
                FieldKey::Pos(_) => ("position", i.position),
                FieldKey::Rot(_) => ("rotation_deg", i.rotation),
                FieldKey::Scale(_) => ("scale", i.scale),
                FieldKey::Color(_) => ("color", i.color),
                FieldKey::Name | FieldKey::Text => unreachable!("handled above"),
            };
            let c = match f {
                FieldKey::Pos(c) | FieldKey::Rot(c) | FieldKey::Scale(c) | FieldKey::Color(c) => c % 3,
                FieldKey::Name | FieldKey::Text => 0,
            };
            if matches!(f, FieldKey::Scale(_)) && v <= 0.0 {
                self.editor.ui.set_status("Not set: a scale must be above 0.", false);
                return;
            }
            if matches!(f, FieldKey::Color(_)) && !(0.0..=1.0).contains(&v) {
                self.editor.ui.set_status("Not set: colour components are 0..1.", false);
                return;
            }
            value[c] = v;
            args.set(key, Json::arr(value.map(|x| Json::from(f64::from(x)))));
            format!("Set {} of {} to {}", f.label(), i.path, num(v))
        };
        let _ = self.editor_send("world_set", args, Why::Edit(what));
    }

    fn editor_delete(&mut self) {
        let ids = self.editor.ui.selection.clone();
        if ids.is_empty() {
            return;
        }
        let mut ops = Vec::new();
        for id in &ids {
            if let Some(e) = self.world.get(*id) {
                ops.push(Json::obj([("op", Json::from("delete")), ("id", Json::from(*id)), ("expect_version", Json::from(e.version))]));
            }
        }
        // A child of a deleted parent goes with it: deleting it again would fail the batch.
        let set: BTreeSet<u64> = ids.iter().copied().collect();
        ops.retain(|op| {
            let id = op.get("id").and_then(Json::as_u64).unwrap_or(0);
            let mut at = self.world.get(id).and_then(|e| e.parent);
            while let Some(p) = at {
                if set.contains(&p) {
                    return false;
                }
                at = self.world.get(p).and_then(|e| e.parent);
            }
            true
        });
        if ops.is_empty() {
            return;
        }
        let n = ops.len();
        if self.editor_send("world_batch", Json::obj([("ops", Json::Arr(ops))]), Why::Edit(format!("Deleted {n} entit{}", if n == 1 { "y" } else { "ies" }))) {
            self.editor.ui.select(&[], false);
        }
    }

    /// "Ask the agents…": opens the box beside the selection. Only the human's own press opens
    /// it: an `editor_input` frame (an agent driving the editor) is refused, so no agent can put
    /// a box on the window that the human then types into.
    fn editor_ask_open(&mut self) {
        if self.editor.rt.input_call.0 != 0 {
            let by = self.editor.rt.input_call.1.as_ref().map_or_else(String::new, |d| d.by.clone());
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "editor.ask_refused", by = by.as_str(), why = "injected");
            self.editor.ui.set_status("Only the human opens the Ask box (agents post tasks with task_post).", false);
            return;
        }
        if self.editor.ui.asking() {
            return;
        }
        let sel: Vec<u64> = self.editor.ui.selection.iter().copied().filter(|id| self.world.get(*id).is_some()).collect();
        let Some(&primary) = sel.last() else {
            self.editor.ui.set_status("Highlight something first: click an entity (Shift adds more), then Ask.", false);
            return;
        };
        // The selection by identity, the primary last (at most the board's link cap: the newest).
        let skip = sel.len().saturating_sub(crate::editor_ask::MAX_ENTITIES);
        let refs: Vec<crate::entity_ref::EntityRef> = sel[skip..].iter().filter_map(|id| crate::entity_ref::EntityRef::of(&self.world, *id)).collect();
        let e = self.world.get(primary).expect("checked");
        let path = self.world.path_of(primary);
        let about = if refs.len() > 1 { format!("{path} and {} more", refs.len() - 1) } else { path };
        // What part of it the request may be about: its recipe's named parts (the library has
        // them parsed: no disk, no parse here), its words, or the ground around it.
        let mut choices = Vec::new();
        if let Some(m) = &e.model {
            for name in self.library.lock().part_names(&m.recipe, 32).unwrap_or_default() {
                choices.push(crate::editor_ask::Choice::Recipe(name));
            }
        }
        if let Some(crate::two_d::TwoD::Text(t)) = &e.two_d {
            choices.push(crate::editor_ask::Choice::Words(t.text.len() as u32));
        }
        let (lo, hi) = e.world_aabb();
        let center = (lo + hi) * 0.5;
        let radius = ((hi.x - lo.x).max(hi.z - lo.z) * 0.5).clamp(0.5, crate::ask::MAX_RADIUS_M);
        choices.push(crate::editor_ask::Choice::Area { center: [center.x, lo.y, center.z], radius });
        // What the human pointed at before asking, first in the list and chosen to begin with
        // (carried from the reviews of m5-edswarm: a part came only from a list, never from
        // what was highlighted). With a brush in hand that is the spot it last touched on this
        // entity; with the path tool, the sweep part it would redraw.
        let mut pointed = None;
        match self.editor.ui.brush.as_ref().map(|b| b.kind) {
            Some(crate::editor_brush::BrushKind::Path(k)) => {
                let sweep = e.model.as_ref().and_then(|m| self.library.lock().sweep_names(&m.recipe, crate::editor_brush::MAX_PATH_SWEEPS as usize)).and_then(|s| s.get(k as usize).cloned());
                if let Some(at) = sweep.and_then(|name| choices.iter().position(|c| *c == crate::editor_ask::Choice::Recipe(name.clone()))) {
                    let part = choices.remove(at);
                    choices.insert(0, part);
                    pointed = Some("the sweep part your path tool is on");
                }
            }
            Some(_) => {
                if let Some((_, at, r)) = self.editor.rt.brush_spot.filter(|(entity, _, _)| *entity == primary) {
                    choices.insert(0, crate::editor_ask::Choice::Spot { center: [at.x, at.y, at.z], radius: r.clamp(0.05, crate::ask::MAX_RADIUS_M) });
                    pointed = Some("the spot under your brush");
                }
            }
            None => {}
        }
        // Beside the selection as it is on screen now (it stays there: nothing moves it later).
        let size = self.editor.rt.size;
        let target = self.editor.rt.cam.and_then(|c| crate::sight_overlay::Painter::new(c.camera().view_proj(size.0, size.1), size.0, size.1).project(center));
        self.editor.ui.ask.open(about, refs, choices, target, size);
        match pointed {
            Some(what) => {
                self.editor.ui.ask.choose(0);
                self.editor.ui.set_status(format!("Asking about {what} (Tab changes what it is about). Type your request; Enter sends it to the task board, Esc cancels."), true);
            }
            None => self.editor.ui.set_status("Type your request; Enter sends it to the task board, Esc cancels. No agent reads it until you send.", true),
        }
    }

    /// The box's Send or Cancel.
    fn editor_ask_out(&mut self, out: crate::editor_ask::AskOut) {
        match out {
            crate::editor_ask::AskOut::Cancel => {
                self.editor.ui.ask.close("cancelled");
                self.editor.ui.set_status("Ask cancelled: nothing was sent.", true);
            }
            crate::editor_ask::AskOut::Send { text, entities, choice } => {
                // By identity: what is still in the world (a deleted one can't be asked about).
                let ids: Vec<u64> = entities.iter().filter_map(|r| r.attached(&self.world)).collect();
                if ids.is_empty() && !entities.is_empty() {
                    self.editor.ui.set_status("Not sent: what you highlighted is gone (deleted or replaced). Your words were kept in the box.", false);
                    return;
                }
                let primary_kept = entities.last().and_then(|r| r.attached(&self.world)).is_some();
                let mut part = if primary_kept { choice.part(ids.len() - 1) } else { None };
                // "Its words" are the words as they are now (the text may have changed while the
                // box was open: a span past it would be refused, `Engine::check_parts`).
                if let (Some(crate::ask::Part { kind: crate::ask::PartKind::TextSpan { start, end }, .. }), Some(crate::two_d::TwoD::Text(t))) =
                    (part.as_mut(), ids.last().and_then(|id| self.world.get(*id)).and_then(|e| e.two_d.as_ref()))
                {
                    (*start, *end) = (0, t.text.len() as u32);
                }
                let first_line = text.lines().next().unwrap_or("").trim();
                let title = crate::editor_swarm::short(&format!("Ask: {first_line}"), crate::board::MAX_TITLE);
                let mut detail = format!("{text}\n\nAsked by the window's owner in the editor (highlight and ask), about ");
                detail.push_str(&ids.iter().take(8).map(|id| self.world.path_of(*id)).collect::<Vec<_>>().join(", "));
                if let Some(p) = &part {
                    detail.push_str(&format!("; {}", p.describe()));
                }
                detail.push_str(".\nTake it (task_take), make the change through the normal tools, then review_submit with this task: the owner decides in the review screen.");
                let detail = crate::editor_swarm::short_keep_lines(&detail, crate::board::MAX_DETAIL);
                let mut args = Json::obj([
                    ("title", Json::from(title)),
                    ("detail", Json::from(detail)),
                    ("entities", Json::arr(ids.iter().map(|i| Json::from(*i)))),
                    ("tags", Json::arr([Json::from(crate::ask::TAG)])),
                    ("priority", Json::from(6)),
                ]);
                if let Some(p) = part {
                    args.set("parts", Json::arr([p.to_json()]));
                }
                // The box stays open, with the words, until the board has taken the request
                // (review R1 of m5-edswarm, minor 5: it closed when the request was queued, so a
                // refused post, a full board for one, lost what the human had typed).
                if self.editor_send("task_post", args, Why::Ask) {
                    self.editor.ui.ask.set_sending(true);
                    self.editor.ui.set_status("Sending your request to the board...", true);
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "editor.ask_sent", by = crate::authority::WINDOW_OWNER, entities = ids.len() as u64);
                }
            }
        }
    }

    fn editor_frame_selection(&mut self) {
        let mut bounds: Option<(Vec3, Vec3)> = None;
        for id in &self.editor.ui.selection {
            if let Some(e) = self.world.get(*id) {
                let (lo, hi) = e.world_aabb();
                bounds = Some(match bounds {
                    Some((a, b)) => (a.min(lo), b.max(hi)),
                    None => (lo, hi),
                });
            }
        }
        let (Some((lo, hi)), Some(cam)) = (bounds, self.editor.rt.cam.as_mut()) else { return };
        cam.target = (lo + hi) * 0.5;
        let r = ((hi - lo).length() * 0.5).max(0.25);
        let half = (cam.fov.to_radians() * 0.5).clamp(0.05, 1.4);
        cam.distance = (r * 1.6 / dmath::tan(half)).clamp(0.5, 5000.0);
        if let Some(h) = cam.ortho.as_mut() {
            *h = (r * 3.0).max(1.0);
        }
    }

    /// The viewport's pointer: gizmo hover and drags, click and box select, the camera.
    fn editor_viewport(&mut self, v: &ViewIn, size: (u32, u32)) {
        let rect = self.editor.ui.viewport_rect(size);
        let inside = |p: [f32; 2]| p[0] >= rect[0] && p[1] >= rect[1] && p[0] < rect[0] + rect[2] && p[1] < rect[1] + rect[3];
        let Some(cam) = self.editor.rt.cam else { return };
        let Some(view) = View::new(cam.camera(), size.0, size.1) else { return };
        // Camera: wheel zoom, right-drag orbit, middle-drag pan, pads and two fingers.
        if let Some(p) = v.pointer {
            if inside(p) && !v.over_ui && v.wheel != 0.0 {
                self.editor_zoom(v.wheel);
            }
        }
        if v.zoom != 0.0 {
            self.editor_zoom(v.zoom);
        }
        if v.orbit != [0.0, 0.0] {
            self.editor_orbit(v.orbit[0], v.orbit[1]);
        }
        let nav_button = |b: &Btn| b.held && !b.pressed;
        if let Some(p) = v.pointer {
            if (v.right.pressed || v.middle.pressed) && inside(p) && !v.over_ui {
                if v.right.pressed && self.editor.rt.drag.is_some() {
                    // A right click during a drag cancels it.
                    self.editor_cancel_drag();
                } else {
                    self.editor.rt.nav = Some(p);
                }
            }
            if let Some(last) = self.editor.rt.nav {
                let (dx, dy) = (p[0] - last[0], p[1] - last[1]);
                if nav_button(&v.right) {
                    self.editor_orbit(-dx * 0.006, -dy * 0.006);
                } else if nav_button(&v.middle) {
                    self.editor_pan(dx, dy, &view);
                }
                self.editor.rt.nav = Some(p);
            }
        }
        if !v.right.held && !v.middle.held {
            self.editor.rt.nav = None;
        }
        // The draw tools: with a brush in hand the left button strokes, and selects nothing.
        if self.editor.ui.brush.is_some() {
            let over = v.pointer.filter(|p| inside(*p) && !v.over_ui);
            self.editor_brush_input(&view, v, over);
            return;
        }
        self.editor.rt.stroke = None;
        self.editor.rt.brush_hover = None;
        self.editor.rt.brush_spot = None;
        // Selection and gizmos.
        let snap_on = self.editor.ui.snap.on != v.ctrl;
        let pointer = v.pointer;
        if let Some(d) = self.editor.rt.drag.as_mut() {
            if let Some(p) = pointer {
                let snap = self.editor.ui.snap;
                let raw = d.drag.delta(&view, d.mode, p, snap_on.then_some(&snap));
                d.delta = if snap_on { grid_delta(d, raw, &snap) } else { raw };
            }
            if v.left.released || !v.left.held {
                self.editor_commit_drag();
            }
            return;
        }
        let Some(p) = pointer else { return };
        let frame = self.editor_gizmo_frame(&view);
        let hover = if inside(p) && !v.over_ui { frame.and_then(|f| gz::hit(&view, &f, self.editor.ui.mode, p)) } else { None };
        if hover != self.editor.ui.hover {
            self.editor.ui.hover = hover;
        }
        if v.left.pressed && inside(p) && !v.over_ui {
            match (hover, frame) {
                (Some(h), Some(f)) => self.editor_begin_drag(&view, f, h, p),
                _ => self.editor.rt.press = Some(p),
            }
        }
        if let Some(start) = self.editor.rt.press {
            if v.left.released || !v.left.held {
                self.editor.rt.press = None;
                let moved = ((p[0] - start[0]).powi(2) + (p[1] - start[1]).powi(2)).sqrt();
                let clamp = |x: f32, hi: u32| x.clamp(0.0, hi.saturating_sub(1) as f32) as u32;
                if moved < CLICK_PX {
                    let (x, y) = (clamp(p[0], size.0), clamp(p[1], size.1));
                    self.editor.rt.want_pick = Some(([x, y, x + 1, y + 1], true, v.shift));
                } else {
                    // Within the viewport: nothing hidden under the panels is taken.
                    let r = gz::rect(start, p);
                    let r = [r[0].max(rect[0]), r[1].max(rect[1]), r[2].min(rect[0] + rect[2] - 1.0), r[3].min(rect[1] + rect[3] - 1.0)];
                    let r = [clamp(r[0], size.0), clamp(r[1], size.1), clamp(r[2], size.0) + 1, clamp(r[3], size.1) + 1];
                    self.editor.rt.want_pick = Some((r, false, v.shift));
                }
            }
        }
    }

    fn editor_zoom(&mut self, notches: f32) {
        if let Some(c) = self.editor.rt.cam.as_mut() {
            let k = 0.85f32.powf(notches.clamp(-10.0, 10.0));
            match c.ortho.as_mut() {
                Some(h) => *h = (*h * k).clamp(0.5, 20_000.0),
                None => c.distance = (c.distance * k).clamp(0.2, 20_000.0),
            }
        }
    }

    fn editor_orbit(&mut self, dyaw: f32, dpitch: f32) {
        if let Some(c) = self.editor.rt.cam.as_mut() {
            if c.ortho.is_some() {
                return;
            }
            c.yaw += dyaw.clamp(-1.0, 1.0);
            c.pitch = (c.pitch + dpitch.clamp(-1.0, 1.0)).clamp(-1.53, 1.53);
        }
    }

    fn editor_pan(&mut self, dx: f32, dy: f32, view: &View) {
        if let Some(c) = self.editor.rt.cam.as_mut() {
            let m = view.metres_per_px(c.target);
            let cam = c.camera();
            let fwd = (cam.target - cam.eye).normalize();
            let right = fwd.cross(Vec3::Y).normalize();
            let right = if right.length() < 0.5 { Vec3::X } else { right };
            let up = right.cross(fwd).normalize();
            c.target = c.target - right * (dx * m) + up * (dy * m);
        }
    }

    /// The path tool was picked (or stepped): it is pointed at one of the selection's sweep
    /// parts, by name, when the library has read the model's recipe (the button then steps
    /// through those only). A model with none is told here, not at the first drag.
    fn editor_path_picked(&mut self) {
        use crate::editor_brush::{BrushKind, MAX_PATH_SWEEPS};
        let Some(BrushKind::Path(k)) = self.editor.ui.brush.as_ref().map(|b| b.kind) else { return };
        let recipe = self.editor.ui.primary().and_then(|id| self.world.get(id)).and_then(|e| e.model.as_ref().map(|m| m.recipe.clone()));
        let Some(recipe) = recipe else {
            self.editor.ui.set_status("Path: select a model whose recipe has a sweep part (a rail, a pipe, a road), then drag on its ground plane.", true);
            return;
        };
        // (A recipe the library hasn't parsed, or a glTF: `model_path` answers at the drag.)
        let Some(sweeps) = self.library.lock().sweep_names(&recipe, MAX_PATH_SWEEPS as usize) else { return };
        if sweeps.is_empty() {
            self.editor.ui.set_status(format!("Path: {recipe} has no sweep part to sketch (a path is drawn for a part whose shape is a sweep)."), false);
            return;
        }
        let k = k as usize % sweeps.len();
        if let Some(b) = self.editor.ui.brush.as_mut() {
            b.kind = BrushKind::Path(k as u32);
        }
        self.editor.ui.set_status(format!("Path: drag on the model's ground plane to redraw '{}' (sweep {} of {}); it keeps its height.", sweeps[k], k + 1, sweeps.len()), true);
    }

    /// What the brush in hand would touch at `cursor`: the primary selection's surface. A
    /// terrain's brush wants a terrain the terrain renderer draws (its heights are read where
    /// the ray comes down: at most `editor_brush::MAX_MARCH` looks); the paint brush wants a
    /// model, tested against its coarsest LOD (at most `MAX_HIT_TRIANGLES` triangles); the path
    /// tool draws on the model's ground plane (y = 0 in its own frame).
    fn editor_brush_target(&self, view: &View, cursor: [f32; 2]) -> Option<BrushTarget> {
        use crate::editor_brush as eb;
        let brush = self.editor.ui.brush.as_ref()?;
        let id = self.editor.ui.primary()?;
        let e = self.world.get(id)?;
        let recipe = e.model.as_ref()?.recipe.clone();
        let ray = view.ray(cursor);
        // The ray in a frame: its origin, and a unit direction there.
        let local = |to_world: &Mat4| -> Option<([f32; 3], [f32; 3])> {
            let inv = to_world.inverse()?;
            let o = inv.transform_point(ray.origin);
            let d = inv.transform_point(ray.origin + ray.dir) - o;
            let len = d.length();
            (len > 1e-9 && len.is_finite()).then(|| ([o.x, o.y, o.z], [d.x / len, d.y / len, d.z / len]))
        };
        if brush.kind.is_path() {
            let to_world = e.model();
            let (o, d) = local(&to_world)?;
            let point = eb::plane_hit(o, d, eb::PATH_REACH)?;
            return Some(BrushTarget { entity: id, recipe, collider: None, point, scale: frame_scale(&to_world, false), to_world, offset: [0.0, 0.0] });
        }
        let lib = self.library.lock();
        if brush.kind.on_terrain() {
            let (_, t, model_from_terrain) = crate::terrain::terrain_of(e, &lib)?;
            let to_world = e.model() * model_from_terrain;
            let (o, d) = local(&to_world)?;
            let half = [(t.columns.saturating_sub(1)) as f32 * t.cell[0] * 0.5, (t.rows.saturating_sub(1)) as f32 * t.cell[1] * 0.5];
            let step = 0.5 * t.cell[0].min(t.cell[1]);
            let point = eb::terrain_hit(o, d, half, step, |x, z| t.height_at(x, z))?;
            // A sculpt moves the surface: a terrain that has a collider keeps it on the surface.
            let collider = e.collider.is_some().then(|| self.world.path_of(id));
            Some(BrushTarget { entity: id, recipe, collider, point, scale: frame_scale(&to_world, false), to_world, offset: t.offset })
        } else {
            let built = lib.built(e.resolved_model()?.handle)?;
            let mesh = built.model.lods.iter().rev().find(|m| m.indices.len() / 3 <= eb::MAX_HIT_TRIANGLES)?;
            let to_world = e.model();
            let (o, d) = local(&to_world)?;
            let point = eb::mesh_hit(o, d, &mesh.positions, &mesh.indices)?;
            Some(BrushTarget { entity: id, recipe, collider: None, point, scale: frame_scale(&to_world, true), to_world, offset: [0.0, 0.0] })
        }
    }

    /// The brush's frame of input: where it hovers, the stroke it makes while the left button
    /// is held (samples half a radius apart, one tool call's worth at most), and its release,
    /// which sends the stroke as one command. A right click, or the window losing focus, lets
    /// the stroke go unsent.
    fn editor_brush_input(&mut self, view: &View, v: &ViewIn, over: Option<[f32; 2]>) {
        let Some(brush) = self.editor.ui.brush.clone() else { return };
        let target = over.and_then(|p| self.editor_brush_target(view, p));
        // (The path tool's ring is the distance to its next point, flat on the ground plane.)
        let path = brush.kind.is_path();
        let ring = if path { brush.radius * 0.5 } else { brush.radius };
        let end = target.as_ref().map(|t| (t.entity, t.point));
        if let (Some(t), Some((at, _, _))) = (target.as_ref(), self.editor.rt.brush_hover) {
            self.editor.rt.brush_spot = Some((t.entity, at, brush.radius));
        }
        self.editor.rt.brush_hover = target.as_ref().map(|t| (t.to_world.transform_point(Vec3::new(t.point[0], t.point[1], t.point[2])), ring, brush.kind.on_terrain() || path));
        if v.focus_lost || v.right.pressed {
            if self.editor.rt.stroke.take().is_some() {
                // Which of the two it was is said and traced: a live run lost a stroke once
                // and nothing recorded why (the window's focus is the machine's, not the
                // engine's: someone at the desk takes it by clicking elsewhere).
                let why = if v.hold_lapsed && self.editor.rt.input_call.0 != 0 {
                    "your hold went quiet and an agent's press took the viewport"
                } else if v.hold_lapsed {
                    "the agent driving it went quiet"
                } else if v.focus_lost {
                    "the window lost focus"
                } else {
                    "right click"
                };
                specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "editor.stroke_let_go", why = why);
                self.editor.ui.set_status(format!("Stroke let go ({why}): nothing was sent."), true);
            }
            return;
        }
        let in_flight = !(self.editor.rt.outbox.is_empty() && self.editor.rt.waiting.is_empty()) || self.editor.rt.tool_running.load(std::sync::atomic::Ordering::SeqCst) != 0;
        if v.left.pressed && over.is_some() && in_flight {
            // One stroke in flight at a time: a stroke is written into the recipe the last one
            // is still changing (the second would be refused as stale, or drawn on a surface
            // about to be rebuilt).
            self.editor.ui.set_status("Your last edit is still being written: draw again in a moment.", false);
        } else if v.left.pressed && over.is_some() {
            match target {
                Some(t) => {
                    let mut stroke = crate::editor_brush::Stroke::default();
                    // (Sampled in the frame's own units: the ring is world metres.)
                    stroke.add(t.point, brush.radius / t.scale.0.max(1e-6));
                    self.editor.rt.stroke = Some(BrushStroke { stroke, entity: t.entity, recipe: t.recipe, collider: t.collider, to_world: t.to_world, offset: t.offset, scale: t.scale });
                    // (The line would otherwise still show the last stroke's outcome.)
                    self.editor.ui.set_status(format!("Drawing ({}): let go to send it; a right click lets it go.", brush.label()), true);
                }
                None => {
                    // A model being rebuilt (the last stroke changed its recipe) has no surface
                    // to draw on for a moment: said as that, not as "nothing there".
                    let building = self.editor.ui.primary().and_then(|id| self.world.get(id)).is_some_and(|e| e.model_state() == "building");
                    if building {
                        self.editor.ui.set_status("It is being rebuilt (your last stroke changed its recipe): draw again in a moment.", false);
                    } else if path {
                        self.editor.ui.set_status("Nothing to draw on there: select a model with a sweep part, then drag on its ground plane (the level plane through its origin).", false);
                    } else {
                        let what = if brush.kind.on_terrain() { "a terrain (a model the terrain renderer draws)" } else { "a model" };
                        self.editor.ui.set_status(format!("Nothing to draw on there: select {what}, then drag on it."), false);
                    }
                }
            }
        } else if v.left.held {
            if let (Some(s), Some(t)) = (self.editor.rt.stroke.as_mut(), target.as_ref()) {
                if t.entity == s.entity && s.stroke.add(t.point, brush.radius / s.scale.0.max(1e-6)) && s.stroke.full() {
                    let full = if path { "This path holds all it can (64 points): let go to send it (a wider Size reaches farther)." } else { "This stroke holds all it can (64 dabs): let go to send it, then go on." };
                    self.editor.ui.set_status(full, true);
                }
            }
        }
        if v.left.released || !v.left.held {
            if let Some(mut s) = self.editor.rt.stroke.take() {
                // A path ends where the pointer was let go, not at its last sample.
                if let Some((_, p)) = end.filter(|(entity, _)| path && *entity == s.entity) {
                    s.stroke.end_at(p, brush.radius / s.scale.0.max(1e-6));
                }
                self.editor_brush_send(&brush, s);
            }
        }
    }

    /// A finished stroke as its one command: `terrain_edit`, `model_paint` or `model_path`,
    /// through the editor's tool path (journaled as window:owner, claim-checked, on the undo
    /// stack).
    fn editor_brush_send(&mut self, brush: &crate::editor_brush::Brush, s: BrushStroke) {
        use crate::editor_brush as eb;
        let n = s.stroke.points.len();
        // The brush as the stroke's frame measures it (review R1, M4): what is written is what
        // the ring showed, on a scaled entity too. Labels and the status stay in world metres.
        let world = brush;
        let brush = &eb::in_frame(world, s.scale.0, s.scale.1);
        // (The tools' own refusals would speak in the entity's units: review R2, N4.)
        if let Some(why) = eb::out_of_range(world, brush, s.scale.0) {
            self.editor.ui.set_status(format!("{why} Nothing was sent."), false);
            return;
        }
        if brush.kind.is_path() {
            let Some(args) = eb::model_path_args(&s.recipe, brush, &s.stroke.points) else {
                self.editor.ui.set_status("A path needs a drag: two points or more. Nothing was sent.", false);
                return;
            };
            if self.editor_tool("model_path", args, Why::Edit(format!("Sketched the path ({n} points; {})", world.label()))) {
                self.editor.ui.set_status(format!("Sketched the path: sending {n} points..."), true);
            }
            return;
        }
        let (tool, args, what) = if brush.kind.on_terrain() {
            let what = if matches!(brush.kind, eb::BrushKind::Splat(_)) { "Painted the terrain" } else { "Sculpted the terrain" };
            ("terrain_edit", eb::terrain_edit_args(&s.recipe, s.collider.as_deref(), brush, &s.stroke.points, s.offset), what)
        } else {
            ("model_paint", eb::model_paint_args(&s.recipe, brush, &s.stroke.points), "Painted the model")
        };
        let Some(args) = args else { return };
        // Paint colours vertices: a stroke that reaches none would be written and change
        // nothing. (Checked against the model's own mesh when that is quick enough to do
        // here, `MAX_REACH_TESTS`; a denser one has vertices everywhere a brush can be.)
        if tool == "model_paint" {
            let reaches = self.world.get(s.entity).and_then(|e| e.resolved_model()).and_then(|m| {
                let lib = self.library.lock();
                let mesh = lib.built(m.handle)?.model.lods.first()?;
                eb::paint_reaches(&mesh.positions, &s.stroke.points, brush.radius)
            });
            if reaches == Some(false) {
                self.editor.ui.set_status(
                    format!("Nothing to paint there: no vertex of the model lies within {:.2} m of the stroke (paint colours vertices). Widen the brush, or paint where the model has detail. Nothing was sent.", world.radius),
                    false,
                );
                return;
            }
        }
        if self.editor_tool(tool, args, Why::Edit(format!("{what} ({n} dab{} of {})", if n == 1 { "" } else { "s" }, world.label()))) {
            self.editor.ui.set_status(format!("{what}: sending {n} dab{}...", if n == 1 { "" } else { "s" }), true);
        }
    }

    /// The gizmo's frame for the selection (at the primary's origin), if anything is selected.
    fn editor_gizmo_frame(&self, view: &View) -> Option<gz::Frame> {
        // (With a brush in hand the left button is the brush's: no gizmo is drawn or hit.)
        if self.editor.ui.brush.is_some() {
            return None;
        }
        let id = self.editor.ui.primary()?;
        let e = self.world.get(id)?;
        let m = e.model();
        let origin = m.transform_point(Vec3::ZERO);
        let rot = gz::rotation_of(&m);
        let space = if self.editor.ui.mode == Mode::Scale { Space::Local } else { self.editor.ui.space };
        Some(gz::Frame::new(view, origin, rot, space))
    }

    /// Starts a gizmo drag, unless another agent's claim (or the game) would refuse the edit: the
    /// status then says whose claim it is.
    fn editor_begin_drag(&mut self, view: &View, frame: gz::Frame, handle: Handle, p: [f32; 2]) {
        let (name, key) = self.window_writer();
        let w = Writer::new(&name, Some(&key));
        let now = Instant::now();
        let mut items = Vec::new();
        let ids = self.editor.ui.selection.clone();
        let set: BTreeSet<u64> = ids.iter().copied().collect();
        for id in &ids {
            let Some(e) = self.world.get(*id) else { continue };
            // An entity under another selected one moves with it.
            let mut up = e.parent;
            let mut under = false;
            while let Some(pid) = up {
                if set.contains(&pid) {
                    under = true;
                    break;
                }
                up = self.world.get(pid).and_then(|x| x.parent);
            }
            if under {
                continue;
            }
            let path = self.world.path_of(*id);
            if e.transient {
                self.editor.ui.set_status(format!("Can't move {path}: it is the hosted game's own entity (transient); no tool may change it."), false);
                return;
            }
            let pos = e.world_position();
            let (lo, hi) = e.world_aabb();
            if let Some(c) = self.swarm.claims.blocking(&path, &[pos, (lo + hi) * 0.5], self.world.has_children(*id), w, now) {
                let left = c.expires.saturating_duration_since(now).as_secs();
                let msg = format!("Can't move {path}: {} holds claim {} on {} ({left} s left). Ask them to release or share it.", c.owner, c.id, c.scope.describe());
                self.editor.ui.set_status(msg, false);
                return;
            }
            let parent = e.parent.and_then(|p| self.world.get(p)).map_or(Mat4::IDENTITY, |p| p.model());
            items.push(Item { id: *id, version: e.version, start: Local { position: e.position, rotation_deg: e.rotation_deg, scale: e.scale }, world: e.model(), parent });
        }
        if items.is_empty() {
            return;
        }
        // The live preview: the dragged entities and everything below them.
        let mut preview = BTreeMap::new();
        for (i, it) in items.iter().enumerate() {
            let mut stack = vec![it.id];
            while let Some(id) = stack.pop() {
                if preview.len() >= PREVIEW_MAX {
                    break;
                }
                preview.insert(id, i);
                stack.extend(self.world.children_of(Some(id)));
            }
        }
        let mode = self.editor.ui.mode;
        let delta = match mode {
            Mode::Translate => Delta::Move(Vec3::ZERO),
            Mode::Rotate => Delta::Turn(Quat::IDENTITY, 0.0),
            Mode::Scale => Delta::Grow(Vec3::ONE),
        };
        let drag = gz::Drag::begin(view, frame, handle, p);
        let generation = self.world_generation;
        self.editor.rt.drag = Some(GizmoDrag { drag, mode, space: frame_space(&frame), pivot: frame.origin, items, preview, delta, generation });
    }

    /// The drag ends: one `world_batch` of the moved entities (one journal entry), or nothing.
    fn editor_commit_drag(&mut self) {
        let Some(d) = self.editor.rt.drag.take() else { return };
        if d.delta.is_identity() {
            return;
        }
        if d.generation != self.world_generation {
            // Its ids and versions are the old world's: they would name other entities now.
            self.editor.ui.set_status("Drag not written: the world was replaced (world_load or save_read) while you dragged.", false);
            return;
        }
        // The delta is snapped already (`grid_delta`): what was previewed is what is written.
        let mut ops = Vec::new();
        for it in &d.items {
            let out = gz::apply(&it.start, &it.world, &it.parent, d.pivot, &d.delta, None, [false; 3]);
            let mut op = Json::obj([("op", Json::from("set")), ("id", Json::from(it.id)), ("expect_version", Json::from(it.version))]);
            if out.position != it.start.position {
                op.set("position", v3(out.position));
            }
            if out.rotation_deg != it.start.rotation_deg {
                op.set("rotation_deg", v3(out.rotation_deg));
            }
            if out.scale != it.start.scale {
                op.set("scale", v3(out.scale));
            }
            if op.get("position").is_some() || op.get("rotation_deg").is_some() || op.get("scale").is_some() {
                ops.push(op);
            }
        }
        if ops.is_empty() {
            return;
        }
        let verb = match d.mode {
            Mode::Translate => "Moved",
            Mode::Rotate => "Rotated",
            Mode::Scale => "Scaled",
        };
        let n = ops.len();
        let first = d.items.first().map(|i| self.world.path_of(i.id)).unwrap_or_default();
        let what = if n == 1 { format!("{verb} {first}") } else { format!("{verb} {n} entities") };
        let _ = self.editor_send("world_batch", Json::obj([("ops", Json::Arr(ops))]), Why::Edit(what));
    }

    /// Queues a protocol command as the window operator (sent at the pump).
    /// Queues a protocol command; false when it was refused here (busy, or the driving
    /// editor_input call already made its edit), with the status saying why.
    fn editor_send(&mut self, tool: &'static str, args: Json, why: Why) -> bool {
        if self.editor.rt.outbox.len() + self.editor.rt.waiting.len() >= MAX_WAITING {
            self.editor.ui.set_status("Busy: earlier edits are still being answered; try again.", false);
            return false;
        }
        let input_id = self.editor.rt.input_call.0;
        if input_id != 0 {
            // One tool call, one commit: an editor_input call makes at most one edit.
            if self.editor.rt.edited_calls.contains(&input_id) {
                self.editor.ui.set_status("Not sent: this editor_input call already made its one edit (one call, one commit); send the next edit in a call of its own.", false);
                return false;
            }
            self.editor.rt.edited_calls.push_back(input_id);
            while self.editor.rt.edited_calls.len() > MAX_INPUT_WAITERS * 4 {
                self.editor.rt.edited_calls.pop_front();
            }
        }
        let driver = self.editor.rt.input_call.1.clone();
        self.editor.rt.outbox.push((tool, args, why, driver));
        true
    }

    /// Sends a tool that runs on its caller's thread (`terrain_edit`, `model_paint`,
    /// `model_path`) as the
    /// window's owner: the draw tools' one command per stroke. Such a tool waits for the engine
    /// (it reads and patches a recipe through the project store), so it can't run here on the
    /// engine thread: it runs on a thread of its own, and its answer comes back through the
    /// editor's waiting list like any command's (journal id on the undo stack, the status line).
    /// Driven by an agent's injected frames, its commits are journaled `via` that agent and
    /// charged to its call, as `editor_pump` does. False when it wasn't sent (the status says why).
    fn editor_tool(&mut self, tool: &'static str, args: Json, why: Why) -> bool {
        if self.editor.rt.outbox.len() + self.editor.rt.waiting.len() >= MAX_WAITING {
            self.editor.ui.set_status("Busy: earlier edits are still being answered; try again.", false);
            return false;
        }
        let Some(tools) = self.tools.as_ref().and_then(std::sync::Weak::upgrade) else {
            self.editor.ui.set_status(format!("Not sent ({tool}): this engine serves no tools."), false);
            return false;
        };
        let input_id = self.editor.rt.input_call.0;
        if input_id != 0 {
            // One tool call, one commit: an editor_input call makes at most one edit.
            if self.editor.rt.edited_calls.contains(&input_id) {
                self.editor.ui.set_status("Not sent: this editor_input call already made its one edit (one call, one commit); send the next edit in a call of its own.", false);
                return false;
            }
            self.editor.rt.edited_calls.push_back(input_id);
            while self.editor.rt.edited_calls.len() > MAX_INPUT_WAITERS * 4 {
                self.editor.rt.edited_calls.pop_front();
            }
        }
        // (The thread works inside the driving call's trace context, as `editor_pump`'s
        // commands do: trace_call on an editor_input that drew a stroke follows it there.)
        let (call, via, trace) = match &self.editor.rt.input_call.1 {
            Some(d) => (d.call, Some(d.by.clone()), d.trace),
            None => (0, None, specula_core::trace::Ctx::current()),
        };
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "editor.command", tool = tool, by = crate::authority::WINDOW_OWNER, via = via.clone());
        let (answer, rx) = crate::jobs::channel();
        let running = std::sync::Arc::clone(&self.editor.rt.tool_running);
        let job = ToolJob { tools, tool, args, call, via, trace, answer, running };
        // One thread, started once, takes the calls in turn (one stroke is in flight at a
        // time). Such a tool waits on this thread's own answers and on a rebuild: it can't run
        // here, and it would hold one of the pool's two workers for as long. The thread keeps
        // the tool handler alive only while a call runs, and ends with the engine.
        if self.editor.rt.tool_thread.is_none() {
            let (jobs, queue) = std::sync::mpsc::channel::<ToolJob>();
            let spawned = std::thread::Builder::new().name("specula-editor-tool".into()).spawn(move || {
                for job in queue {
                    let in_call = job.trace.enter();
                    crate::tools::serve_as(job.call, job.via);
                    // A tool that panics answers as an error and the thread goes on, as a
                    // protocol handler's does (the panic is recorded as caught, not as a crash).
                    let ran = specula_core::trace::catch_unwind(std::panic::AssertUnwindSafe(|| crate::terrain_tools::call(&job.tools, &window_session(), job.tool, &job.args)));
                    let out = ran.unwrap_or_else(|_| {
                        ToolOutput::error(Json::obj([("error", Json::from("tool_panicked")), ("message", Json::from(format!("{} panicked; nothing more is known (trace_crash has the report)", job.tool)))]).to_string())
                    });
                    crate::tools::serve_as(0, None);
                    drop(in_call);
                    job.running.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                    let _ = job.answer.send(out);
                }
            });
            match spawned {
                Ok(_) => self.editor.rt.tool_thread = Some(jobs),
                Err(e) => {
                    self.editor.ui.set_status(format!("Not sent ({tool}): no thread for it ({e})."), false);
                    return false;
                }
            }
        }
        self.editor.rt.tool_running.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if !self.editor.rt.tool_thread.as_ref().is_some_and(|t| t.send(job).is_ok()) {
            // Its thread is gone: the next stroke starts another.
            self.editor.rt.tool_running.store(0, std::sync::atomic::Ordering::SeqCst);
            self.editor.rt.tool_thread = None;
            self.editor.ui.set_status(format!("Not sent ({tool}): the editor's tool thread stopped; draw again."), false);
            return false;
        }
        self.editor.rt.waiting.push((rx, why, Instant::now() + TOOL_DEADLINE, call, tool));
        true
    }

    /// `ui_act` on the editor made an edit: its answer waits here for how the edit went, so the
    /// agent that pressed a button learns the outcome from the same call (review R1 of
    /// m5-edswarm, minor 7: it answered before the edit was sent, and a second call had to ask).
    /// `output` is the answer as it stood (it names the edit and says its outcome isn't known):
    /// it is sent as it is at the call's deadline. Gives the answer back when nothing waits
    /// for it (no edit of this call is queued or in flight, or too many answers are held).
    pub(crate) fn editor_hold_answer(&mut self, call: u64, output: ToolOutput, reply: crate::jobs::Reply, deadline: Instant) -> Option<(ToolOutput, crate::jobs::Reply)> {
        let rt = &self.editor.rt;
        if call == 0 || output.is_error || !self.editor_edit_alive(call) || rt.act_waiters.len() >= MAX_INPUT_WAITERS {
            return Some((output, reply));
        }
        self.editor.rt.act_waiters.push((reply, output, call, deadline));
        None
    }

    /// Whether an edit driven by tool call `call` is queued or sent and not answered.
    fn editor_edit_alive(&self, call: u64) -> bool {
        let rt = &self.editor.rt;
        rt.outbox.iter().any(|(_, _, _, d)| d.as_ref().is_some_and(|d| d.call == call)) || rt.waiting.iter().any(|w| w.3 == call)
    }

    /// The edit driven by tool call `call` was answered (`out`), or got no answer: the `ui_act`
    /// answers held for it are sent, saying how it went (the editor's status line, the journal
    /// entry it made, the refusal's code).
    fn editor_act_outcome(&mut self, call: u64, out: Option<&ToolOutput>) {
        if call == 0 || self.editor.rt.act_waiters.is_empty() {
            return;
        }
        let status = self.editor.ui.status.0.clone();
        let (held, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.editor.rt.act_waiters).into_iter().partition(|w| w.2 == call);
        self.editor.rt.act_waiters = rest;
        for (reply, answer, _, _) in held {
            let _ = reply.send(with_outcome(&answer, out, &status));
        }
    }

    /// Held `ui_act` answers whose deadline is near, or whose edit is gone (the world was
    /// replaced with it still queued), are sent as they stood.
    fn editor_act_sweep(&mut self) {
        if self.editor.rt.act_waiters.is_empty() {
            return;
        }
        let soon = Instant::now() + ACT_ANSWER_MARGIN;
        let (keep, send): (Vec<_>, Vec<_>) = std::mem::take(&mut self.editor.rt.act_waiters).into_iter().partition(|w| soon < w.3 && self.editor_edit_alive(w.2));
        self.editor.rt.act_waiters = keep;
        for (reply, answer, _, _) in send {
            let _ = reply.send(answer);
        }
    }

    /// After the window frame's input: sends the editor's commands through the engine's request
    /// handler, reads their answers and the pick's, refreshes the panels, and points the window.
    pub(crate) fn editor_pump(&mut self, server: &Server) {
        let open = self.editor.ui.is_open();
        // Work that came in for review while the editor is open: said on its status line (the
        // window's "Review (n)" tab is not drawn over the editor).
        let pending = self.swarm.review.pending_count();
        if open && pending > self.editor.rt.pending_seen {
            let line = if pending == 1 { "1 submission waits".to_owned() } else { format!("{pending} submissions wait") };
            self.editor.ui.set_status(format!("{line} for your review: F9, or the Swarm tab's Review view."), true);
        }
        self.editor.rt.pending_seen = pending;
        // Commands, exactly as an agent's calls.
        for (tool, args, why, driver) in std::mem::take(&mut self.editor.rt.outbox) {
            let command = match crate::tools::parse(tool, &args) {
                Ok(c) => c,
                Err(e) => {
                    if matches!(why, Why::Ask) {
                        // Never sent: the words stay, to send again (review R2, N3).
                        self.editor.ui.ask.set_sending(false);
                    }
                    self.editor.ui.set_status(format!("Not sent ({tool}): {e}"), false);
                    continue;
                }
            };
            let (reply, rx) = crate::jobs::channel();
            // Driven by an editor_input call: charged to it, traced under it, and journaled with
            // its agent as `via` (the human's own input: none of these).
            let (call, trace, via) = match driver {
                Some(d) => (d.call, d.trace, Some(d.by)),
                None => (0, specula_core::trace::Ctx::current(), None),
            };
            let in_call = trace.enter();
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "editor.command", tool = tool, by = crate::authority::WINDOW_OWNER, via = via.clone());
            drop(in_call);
            let request = Request { session: window_session(), call, command, reply, deadline: Instant::now() + DEADLINE, trace, via };
            self.handle_traced(request, server);
            self.editor.rt.waiting.push((rx, why, Instant::now() + DEADLINE, call, tool));
        }
        // Answers (most come at once; a deferred one a frame or more later).
        let mut i = 0;
        while i < self.editor.rt.waiting.len() {
            let got = self.editor.rt.waiting[i].0.try_recv();
            match got {
                Ok(out) => {
                    let (_, why, _, call, _) = self.editor.rt.waiting.remove(i);
                    self.editor_answer(why, &out);
                    self.editor_act_outcome(call, Some(&out));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) if Instant::now() < self.editor.rt.waiting[i].2 => i += 1,
                Err(_) => {
                    let (_, why, _, call, _) = self.editor.rt.waiting.remove(i);
                    if matches!(why, Why::Ask) {
                        // No answer: the words stay, to send again.
                        self.editor.ui.ask.set_sending(false);
                    }
                    self.editor.ui.set_status("An edit got no answer (the engine dropped it); look at world_history for its outcome.", false);
                    self.editor_act_outcome(call, None);
                }
            }
        }
        self.editor_act_sweep();
        // The pick.
        if self.editor.rt.pick.is_none() {
            if let Some((rect, click, add)) = self.editor.rt.want_pick.take() {
                if let Some(cam) = self.editor.rt.cam {
                    let (w, h) = self.editor.rt.size;
                    let spec = PickSpec { camera: cam.camera(), width: w.max(1), height: h.max(1), rect, click };
                    let (reply, rx) = crate::jobs::channel();
                    self.pick_job(spec, &window_session(), reply, Instant::now() + DEADLINE);
                    self.editor.rt.pick = Some((rx, add, Instant::now() + DEADLINE));
                }
            }
        }
        if let Some((rx, add, until)) = self.editor.rt.pick.take() {
            match rx.try_recv() {
                Ok(out) => {
                    let j = answer_json(&out);
                    if out.is_error {
                        self.editor.ui.set_status(format!("Pick failed: {}", j.get("message").and_then(Json::as_str).unwrap_or("?")), false);
                    } else {
                        let ids: Vec<u64> = j.get("ids").and_then(Json::as_array).map(|a| a.iter().filter_map(Json::as_u64).collect()).unwrap_or_default();
                        let ids: Vec<u64> = ids.into_iter().filter(|id| self.world.get(*id).is_some()).collect();
                        let what = match (ids.len(), ids.first().map(|i| self.world.path_of(*i))) {
                            (0, _) => "Nothing there: selection cleared.".to_owned(),
                            (1, Some(p)) => format!("Selected {p}."),
                            (n, _) => format!("Selected {n} entities (those mostly inside the box)."),
                        };
                        if !(ids.is_empty() && add) {
                            self.editor.ui.set_status(what, true);
                        }
                        if ids.is_empty() && !add {
                            self.editor.ui.select(&[], false);
                        } else if !ids.is_empty() {
                            // A box's most-seen entity is the primary (last).
                            let mut ordered = ids.clone();
                            ordered.reverse();
                            self.editor.ui.select(&ordered, add);
                            self.editor_reveal();
                        }
                        self.editor.rt.outliner.built = None;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) if Instant::now() < until => self.editor.rt.pick = Some((rx, add, until)),
                Err(_) => self.editor.ui.set_status("The pick got no answer (the GPU was busy); click again.", false),
            }
        }
        if open {
            // Selected entities that are gone leave the selection.
            let before = self.editor.ui.selection.len();
            let world = &self.world;
            self.editor.ui.selection.retain(|id| world.get(*id).is_some());
            if self.editor.ui.selection.len() != before {
                self.editor.ui.changed();
            }
            self.editor_refresh_outliner();
            self.editor_refresh_inspector();
            self.editor_swarm_refresh();
            self.editor_window_camera();
        }
        // editor_input calls whose frames have all run (they were fed this frame or earlier) are
        // answered; one past its deadline is answered so and its frames are dropped (never run
        // later as the human).
        if !self.editor.rt.input_waiters.is_empty() {
            let now = Instant::now();
            let waiters = std::mem::take(&mut self.editor.rt.input_waiters);
            let mut state: Option<Json> = None;
            for (reply, deadline, id) in waiters {
                let pending = self.editor.ui.injected.iter().any(|(c, _)| *c == id);
                if !pending {
                    // (editor_input is the owner's: its answer may show the typing.)
                    let s = state.get_or_insert_with(|| self.editor_state_json(false, true)).clone();
                    let mut answer = vec![("fed", Json::from(true))];
                    let call = self.editor.rt.input_tool_calls.remove(&id).map(|d| d.call);
                    if let Some(refused) = call.and_then(|c| self.editor_refused_presses(c)) {
                        answer.push(("presses_refused", refused));
                    }
                    answer.push(("editor", s));
                    let _ = reply.send(ToolOutput::text(Json::obj(answer).to_string()));
                } else if now > deadline {
                    let dropped = self.editor.ui.injected.iter().filter(|(c, _)| *c == id).count();
                    self.editor.ui.drop_injected(id);
                    let driver = self.editor.rt.input_tool_calls.remove(&id);
                    // (Its refused presses go with it: said here, and not left in the tally.)
                    let refused = driver.as_ref().and_then(|d| self.editor.rt.refused_presses.remove(&d.call));
                    let sent = driver.map_or(dropped, |d| d.frames).max(dropped);
                    let held = self.editor_window_held();
                    let mut e = crate::world::WorldError::new(
                        "input_dropped",
                        format!(
                            "editor_input: {} of its {sent} frames ran before the call's deadline{}; the other {dropped} were dropped, never run as the human",
                            sent - dropped,
                            held.map_or(String::new(), |h| format!(" ({h})"))
                        ),
                    )
                    .with("frames_run", (sent - dropped) as u64)
                    .with("frames_dropped", dropped as u64)
                    .with("retry", false);
                    if let Some(n) = refused {
                        e = e.with("presses_refused", u64::from(n));
                    }
                    if let Some(h) = held {
                        e = e.with("held_by", h);
                    }
                    reply.refuse(&e);
                } else {
                    self.editor.rt.input_waiters.push((reply, deadline, id));
                }
            }
        }
    }

    /// While the editor is open the window looks through its camera: an agent's `window_view`
    /// or `stream_fly` is released (the human holds the window, F5), and picks and gizmos act
    /// through the camera on screen.
    pub(crate) fn editor_window_camera(&mut self) {
        if !self.editor.ui.is_open() {
            return;
        }
        if self.window_follow.is_some() {
            self.release_window_camera("editor");
        }
        if let Some(c) = self.editor.rt.cam {
            self.window_camera = Some(c.camera());
        }
    }

    fn editor_answer(&mut self, why: Why, out: &ToolOutput) {
        let j = answer_json(out);
        let journal = j.get("journal").and_then(Json::as_u64);
        self.editor.rt.last = Some(Json::obj([("ok", Json::from(!out.is_error)), ("answer", j.clone())]));
        if out.is_error {
            let msg = explain(&j);
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "editor.refused", code = j.get("error").and_then(Json::as_str).unwrap_or("?"));
            // A transient refusal (a claim, a busy engine) keeps the entry on top to try again; a
            // lasting one (already undone, too old, changed since, not yours) sets it aside, so
            // Ctrl+Z (or Ctrl+Y) goes on with the next one. Decided by the error's own retry flag
            // (`response::retryable`), never a list of codes here.
            let retry = crate::response::retryable(&j);
            match why {
                Why::Undo(seq) if retry => {
                    self.editor.ui.undo.push(seq);
                    self.editor.ui.set_status(format!("Undo of journal {seq} refused for now; try again. {msg}"), false);
                }
                Why::Undo(seq) => {
                    self.editor.ui.set_status(format!("Undo of journal {seq} refused and set aside (Ctrl+Z goes on with your earlier edits; world_history shows who changed it). {msg}"), false);
                }
                Why::Redo(seq) if retry => {
                    self.editor.ui.redo.push(seq);
                    self.editor.ui.set_status(format!("Redo (undoing journal {seq}) refused for now; try again. {msg}"), false);
                }
                Why::Redo(seq) => {
                    self.editor.ui.set_status(format!("Redo (undoing journal {seq}) refused and set aside (Ctrl+Y goes on with the next). {msg}"), false);
                }
                Why::Edit(what) => self.editor.ui.set_status(format!("{what}: not done. {msg}"), false),
                Why::Ask => {
                    self.editor.ui.ask.set_sending(false);
                    self.editor.ui.set_status(format!("Your request was not posted. {msg} Your words are still in the box."), false);
                }
            }
            self.editor.rt.inspected = None;
            return;
        }
        match why {
            Why::Edit(what) => {
                if let Some(seq) = journal {
                    self.editor.ui.did(seq);
                }
                self.editor.ui.set_status(format!("{what} (journal {}, as window:owner).", journal.map_or("?".into(), |s| s.to_string())), true);
            }
            Why::Undo(seq) => {
                if let Some(u) = journal {
                    self.editor.ui.redo.push(u);
                }
                self.editor.ui.set_status(format!("Undid journal {seq} (journal {}).", journal.map_or("?".into(), |s| s.to_string())), true);
            }
            Why::Redo(seq) => {
                if let Some(u) = journal {
                    self.editor.ui.undo.push(u);
                }
                self.editor.ui.set_status(format!("Redid it: undid the undo {seq} (journal {}).", journal.map_or("?".into(), |s| s.to_string())), true);
            }
            Why::Ask => {
                // The box this request was sent from (one opened since, with other words, stays:
                // review R2, N6).
                if self.editor.ui.ask.sending() {
                    self.editor.ui.ask.close("sent");
                }
                let task = j.get("id").and_then(Json::as_u64);
                self.editor.ui.set_status(
                    format!(
                        "Asked the agents: task {} is on the board (tag \"ask\"). An agent takes it, edits, and submits; you decide in the review screen (F9, or the Swarm tab's Review).",
                        task.map_or("?".into(), |t| t.to_string())
                    ),
                    true,
                );
                let p = &mut self.editor.ui.swarm;
                p.built = None;
            }
        }
        self.editor.rt.inspected = None;
        self.editor.rt.outliner.built = None;
    }

    /// The primary selection shows in the outliner: its ancestors expanded, the page from its
    /// top-level ancestor.
    fn editor_reveal(&mut self) {
        let Some(id) = self.editor.ui.primary() else { return };
        let mut chain = Vec::new();
        let mut at = self.world.get(id).and_then(|e| e.parent);
        while let Some(p) = at {
            chain.push(p);
            at = self.world.get(p).and_then(|e| e.parent);
        }
        if self.editor.ui.search.is_empty() && !self.editor.ui.outliner.rows.iter().any(|r| r.id == id) {
            let o = &mut self.editor.rt.outliner;
            for p in &chain {
                if o.expanded.len() < MAX_EXPANDED {
                    o.expanded.insert(*p);
                }
            }
            let top = chain.last().copied().unwrap_or(id);
            o.starts = vec![Cursor::at(&self.world, top)];
            o.page = 0;
            o.from = Some(self.world.path_of(top));
            o.epoch += 1;
            o.built = None;
        }
    }

    /// The outliner's page, rebuilt when the world, the search, the page or the expanded set
    /// changed (at most every [`REFRESH_EVERY`] while the world keeps changing). A page reads
    /// its rows only; a search reads entities for at most [`SCAN_BUDGET`] a frame.
    fn editor_refresh_outliner(&mut self) {
        let search = self.editor.ui.search.trim().to_ascii_lowercase();
        let rev = self.world.revision();
        let o = &self.editor.rt.outliner;
        let key = (rev, search.clone(), o.page, o.epoch);
        let fresh = o.built.as_ref() == Some(&key);
        let recent = o.built_at.is_some_and(|t| t.elapsed() < REFRESH_EVERY);
        let same_query = o.built.as_ref().is_some_and(|b| b.1 == search && b.2 == o.page && b.3 == o.epoch);
        // Claims and expiry change without the world: once a second regardless.
        let stale = o.built_at.is_none_or(|t| t.elapsed() >= Duration::from_secs(1));
        if o.scan.is_none() && (fresh && !stale || (same_query && recent)) {
            return;
        }
        let total = self.world.len();
        let (name, wkey) = self.window_writer();
        let w = Writer::new(&name, Some(&wkey));
        let now = Instant::now();
        let world = &self.world;
        let claims = &self.swarm.claims;
        let row_of = |id: u64, depth: usize, expanded: bool| -> Option<Row> {
            let e = world.get(id)?;
            let path = world.path_of(id);
            let pos = e.world_position();
            let claim = claims.blocking(&path, &[pos], false, w, now).map(|c| c.owner.clone());
            Some(Row { id, depth, name: e.name.clone(), path, has_children: world.has_children(id), expanded, claim, by: e.modified_by.to_string(), game: e.transient })
        };
        let o = &mut self.editor.rt.outliner;
        let mut view = OutlinerView { total, page: o.page, ..Default::default() };
        if search.is_empty() {
            o.scan = None;
            if o.starts.is_empty() {
                o.starts.push(Cursor::start());
            }
            // Pages before this one are walked once each to find where it starts.
            while o.starts.len() <= o.page {
                let mut c = o.starts.last().cloned().unwrap_or_else(Cursor::start);
                let expanded = &o.expanded;
                let mut n = 0;
                while n < PAGE_ROWS && c.next(world, &|id| expanded.contains(&id)).is_some() {
                    n += 1;
                }
                if n < PAGE_ROWS {
                    o.page = o.starts.len() - 1;
                    break;
                }
                o.starts.push(c);
            }
            view.page = o.page;
            let mut c = o.starts[o.page].clone();
            let expanded = &o.expanded;
            while view.rows.len() < PAGE_ROWS {
                let Some((id, depth)) = c.next(world, &|id| expanded.contains(&id)) else { break };
                if let Some(r) = row_of(id, depth, expanded.contains(&id)) {
                    view.rows.push(r);
                }
            }
            view.has_next = c.clone().next(world, &|id| expanded.contains(&id)).is_some();
            if view.has_next {
                o.starts.truncate(o.page + 1);
                o.starts.push(c);
            }
            if let Some(from) = &o.from {
                view.note = format!("From {from} (Top for the start).");
            }
            o.built = Some((rev, search, o.page, o.epoch));
            o.built_at = Some(now);
        } else {
            // A search: matches by name (or #tag) in path order, a slice a frame within
            // SCAN_BUDGET, allocating nothing per entity read (`outline`).
            let query = Query::parse(&search);
            let restart = o.built.as_ref().is_none_or(|b| b.1 != search || b.2 != o.page) || o.scan.is_none() && !fresh;
            if restart && o.scan.is_none() {
                if o.starts.is_empty() {
                    o.starts.push(Cursor::start());
                }
                let start = o.starts.get(o.page).cloned().unwrap_or_else(Cursor::start);
                o.scan = Some((Scan::new(start, PAGE_ROWS), Vec::with_capacity(PAGE_ROWS)));
                o.scan_shown = None;
            }
            let Some((mut scan, mut rows)) = o.scan.take() else { return };
            scan.step(world, &query, PAGE_ROWS, Budget { max_steps: SCAN_MAX_STEPS, max_time: SCAN_BUDGET });
            let found = rows.len();
            for &id in scan.matches.iter().skip(found) {
                rows.extend(row_of(id, 0, false));
            }
            let done = scan.done;
            // Progress alone is shown every SCAN_SHOW_EVERY (the panels are filled again then).
            if !done && rows.len() == found && o.scan_shown.is_some_and(|t| t.elapsed() < SCAN_SHOW_EVERY) {
                o.scan = Some((scan, rows));
                return;
            }
            view.matches = rows.len();
            view.scanning = !done;
            view.rows = rows.clone();
            view.note = format!("{} read", scan.read);
            if done {
                view.has_next = scan.next.is_some();
                if let Some(s) = scan.next.take() {
                    o.starts.truncate(o.page + 1);
                    o.starts.push(s);
                }
                o.built = Some((rev, search, o.page, o.epoch));
                o.built_at = Some(now);
            } else {
                o.scan = Some((scan, rows));
                o.scan_shown = Some(now);
            }
        }
        self.editor.ui.outliner = view;
        self.editor.ui.changed();
    }

    /// The inspector's entity, filled again when it (or its claims) changed.
    fn editor_refresh_inspector(&mut self) {
        let Some(id) = self.editor.ui.primary() else {
            if self.editor.ui.inspect.take().is_some() {
                self.editor.ui.changed();
            }
            self.editor.rt.inspected = None;
            return;
        };
        let Some(e) = self.world.get(id) else { return };
        let rev = self.world.revision();
        let key = (id, e.version, rev);
        let stale = self.editor.rt.inspected_at.is_none_or(|t| t.elapsed() >= Duration::from_secs(1));
        if self.editor.rt.inspected.is_some_and(|k| k.0 == id && k.1 == e.version) && !stale {
            return;
        }
        let path = self.world.path_of(id);
        let (name, wkey) = self.window_writer();
        let now = Instant::now();
        let w = Writer::new(&name, Some(&wkey));
        let claim = self.swarm.claims.blocking(&path, &[e.world_position()], false, w, now).map(|c| {
            format!("{} holds claim {} on {} ({} s left)", c.owner, c.id, c.scope.describe(), c.expires.saturating_duration_since(now).as_secs())
        });
        let mut parts: Vec<String> = vec![format!("shape {}", e.shape.name())];
        if !e.tags.is_empty() {
            let tags: Vec<&str> = e.tags.iter().take(8).map(String::as_str).collect();
            parts.push(format!("tags {}", tags.join(", ")));
        }
        if let Some(m) = &e.model {
            parts.push(format!("model {}", m.recipe));
        }
        for (has, label) in [(e.light.is_some(), "light"), (e.collider.is_some(), "collider"), (e.body.is_some(), "body"), (e.anim.is_some(), "anim"), (e.vfx.is_some(), "vfx"), (e.decal.is_some(), "decal"), (e.water.is_some(), "water")] {
            if has {
                parts.push(label.into());
            }
        }
        let mut components = parts.join(" - ");
        if components.len() > 300 {
            let cut = (0..=300).rev().find(|i| components.is_char_boundary(*i)).unwrap_or(0);
            components.truncate(cut);
        }
        let i = Inspect {
            id,
            path,
            name: e.name.clone(),
            version: e.version,
            created_by: e.created_by.to_string(),
            modified_by: e.modified_by.to_string(),
            position: [e.position.x, e.position.y, e.position.z],
            rotation: [e.rotation_deg.x, e.rotation_deg.y, e.rotation_deg.z],
            scale: [e.scale.x, e.scale.y, e.scale.z],
            color: e.color,
            visible: e.visible,
            game: e.transient,
            claim,
            components,
            text: match &e.two_d {
                Some(crate::two_d::TwoD::Text(t)) => Some(t.text.clone()),
                _ => None,
            },
        };
        self.editor.rt.inspected = Some(key);
        self.editor.rt.inspected_at = Some(now);
        if self.editor.ui.inspect.as_ref() != Some(&i) {
            self.editor.ui.inspect = Some(i);
            self.editor.ui.changed();
        }
    }

    /// The live preview: the dragged entities' draws (and their descendants') moved to where the
    /// drag puts them. Nothing in the world changes until the drag ends.
    pub(crate) fn editor_patch_draws(&self, draws: &mut [DrawItem], owners: &[u64]) {
        let Some(d) = self.editor.rt.drag.as_ref() else { return };
        if d.delta.is_identity() || d.generation != self.world_generation {
            return;
        }
        let deltas: Vec<Mat4> = d
            .items
            .iter()
            .map(|it| {
                let to = gz::preview_matrix(&it.world, d.pivot, &d.delta);
                to * it.world.inverse().unwrap_or(Mat4::IDENTITY)
            })
            .collect();
        for (draw, owner) in draws.iter_mut().zip(owners) {
            let Some(&i) = d.preview.get(owner) else { continue };
            let m = deltas[i];
            draw.model = (m * mat_of(&draw.model)).to_cols_array();
            if let Some(b) = draw.bounds.as_mut() {
                let c = m.transform_point(Vec3::new(b[0], b[1], b[2]));
                let s = [0, 1, 2].iter().map(|&k| Vec3::new(m.cols[k][0], m.cols[k][1], m.cols[k][2]).length()).fold(0.0f32, f32::max);
                *b = [c.x, c.y, c.z, b[3] * s.max(1e-3)];
            }
        }
    }

    /// The editor's overlay for one window frame: selection marks, the gizmo and the box under
    /// the panels.
    pub(crate) fn editor_overlay(&mut self, size: (u32, u32), out: &mut Vec<OverlayQuad>) {
        if !self.editor.ui.is_open() || size.0 == 0 || size.1 == 0 {
            return;
        }
        let Some(cam) = self.editor.rt.cam else { return };
        let camera = cam.camera();
        let vp = camera.view_proj(size.0, size.1);
        let mut p = crate::sight_overlay::Painter::new(vp, size.0, size.1);
        p.limit = 6000;
        let deltas: Option<(&GizmoDrag, Vec<Mat4>)> = self.editor.rt.drag.as_ref().map(|d| {
            let m = d.items.iter().map(|it| gz::preview_matrix(&it.world, d.pivot, &d.delta) * it.world.inverse().unwrap_or(Mat4::IDENTITY)).collect();
            (d, m)
        });
        // Selection marks: STYLE.md's highlight cyan (#3fe0d0), never the pale tone.
        const CYAN: [f32; 4] = [0.247, 0.878, 0.816, 1.0];
        const CYAN_DIM: [f32; 4] = [0.247, 0.878, 0.816, 0.55];
        let primary = self.editor.ui.primary();
        for id in self.editor.ui.selection.iter().rev().take(64) {
            let Some(e) = self.world.get(*id) else { continue };
            let (lo, hi) = match e.local_bounds() {
                Some(b) => b,
                None => (Vec3::new(-0.25, -0.25, -0.25), Vec3::new(0.25, 0.25, 0.25)),
            };
            let mut m = e.model();
            if let Some((d, ms)) = &deltas {
                if let Some(&i) = d.preview.get(id) {
                    m = ms[i] * m;
                }
            }
            let corners: [Vec3; 8] = std::array::from_fn(|k| {
                let c = Vec3::new(if k & 1 == 0 { lo.x } else { hi.x }, if k & 2 == 0 { lo.y } else { hi.y }, if k & 4 == 0 { lo.z } else { hi.z });
                m.transform_point(c)
            });
            p.box_edges(&corners, if Some(*id) == primary { CYAN } else { CYAN_DIM }, if Some(*id) == primary { 2.0 } else { 1.5 });
        }
        // The gizmo.
        if let Some(view) = View::new(camera, size.0, size.1) {
            let frame = match &self.editor.rt.drag {
                Some(d) => {
                    let mut f = d.drag.frame;
                    if let Delta::Move(m) = d.delta {
                        f.origin = f.origin + m;
                    }
                    Some(f)
                }
                None => self.editor_gizmo_frame(&view),
            };
            if let Some(f) = frame {
                let active = self.editor.rt.drag.as_ref().map(|d| d.drag.handle).or(self.editor.ui.hover);
                // (Its JSON is made only when editor_state asks.)
                self.editor.rt.gizmo_px = Some((view, f, self.editor.ui.mode));
                draw_gizmo(&mut p, &f, self.editor.ui.mode, active);
            } else {
                self.editor.rt.gizmo_px = None;
            }
        }
        // The box being dragged out.
        if let (Some(start), Some(cur)) = (self.editor.rt.press, self.input_pointer()) {
            let r = gz::rect(start, cur);
            if r[2] - r[0] >= CLICK_PX || r[3] - r[1] >= CLICK_PX {
                p.rect2d([r[0], r[1], r[2] - r[0], r[3] - r[1]], [0.247, 0.878, 0.816, 0.12]);
                for (a, b) in [([r[0], r[1]], [r[2], r[1]]), ([r[2], r[1]], [r[2], r[3]]), ([r[2], r[3]], [r[0], r[3]]), ([r[0], r[3]], [r[0], r[1]])] {
                    p.line2d(a, b, CYAN, 1.5);
                }
            }
        }
        // The draw tools: the brush's ring where it hovers, and the stroke so far.
        if let Some((centre, radius, on_ground)) = self.editor.rt.brush_hover {
            const RING: usize = 40;
            // On a terrain the ring lies on the ground; on a model it faces the camera.
            let fwd = (camera.target - camera.eye).normalize();
            let right = fwd.cross(Vec3::Y);
            let right = if right.length() < 1e-3 { Vec3::X } else { right.normalize() };
            let (u, w) = if on_ground { (Vec3::X, Vec3::Z) } else { (right, right.cross(fwd).normalize()) };
            let at = |k: usize| {
                let a = k as f32 / RING as f32 * std::f32::consts::TAU;
                centre + u * (a.cos() * radius) + w * (a.sin() * radius)
            };
            for k in 0..RING {
                p.line(at(k), at(k + 1), CYAN, 2.0);
            }
            p.dot(centre, 3.0, CYAN);
        }
        if let Some(s) = &self.editor.rt.stroke {
            let world = |q: &[f32; 3]| s.to_world.transform_point(Vec3::new(q[0], q[1], q[2]));
            // A sketched path is a line through its points; a brush's stroke is its dabs.
            if self.editor.ui.brush.as_ref().is_some_and(|b| b.kind.is_path()) {
                for w in s.stroke.points.windows(2) {
                    p.line(world(&w[0]), world(&w[1]), CYAN, 2.0);
                }
            }
            for q in &s.stroke.points {
                p.dot(world(q), 4.0, CYAN_DIM);
            }
        }
        // The swarm panels' marks: the selected row's entities, the claim map.
        self.editor_swarm_overlay(&mut p);
        // The Ask box's leader line to what it is about (from where the box is).
        if let (Some(t), Some(r)) = (self.editor.ui.ask.target(), self.editor.ui.ask.rect()) {
            let from = [r[0] + r[2] * 0.5, if t[1] < r[1] { r[1] } else { r[1] + r[3] }];
            p.line2d(from, t, CYAN, 2.0);
            p.rect2d([t[0] - 4.0, t[1] - 4.0, 8.0, 8.0], CYAN);
        }
        let room = specula_gpu::MAX_OVERLAY_QUADS.saturating_sub(out.len());
        let mut scene = std::mem::take(&mut p.quads);
        // Names (claim holders, the highlighted entry's agent) on the marks, under the panels.
        if !p.labels.is_empty() {
            let labels = std::mem::take(&mut p.labels);
            let labeler = &mut self.editor.rt.labeler;
            let (placed, _) = crate::sight_overlay::place_labels(labels, [size.0 as f32, size.1 as f32], 48, &mut |t| labeler.measure(t));
            if let Err(e) = labeler.draw(&mut self.gpu, &placed, &mut scene) {
                specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "editor.labels_failed", message = e.as_str());
            }
        }
        self.editor.ui.overlay(&mut self.gpu, size, room, &scene, out);
        // The Ask box over the panels (its own UI: its words never enter the editor screen's state).
        let room = specula_gpu::MAX_OVERLAY_QUADS.saturating_sub(out.len());
        self.editor.ui.ask.overlay(&mut self.gpu, size, room, out);
    }

    /// Where the editor's pointer is (the last input it read).
    fn input_pointer(&self) -> Option<[f32; 2]> {
        self.editor.ui.last_pointer()
    }

    /// `editor_state`: what the editor shows, for agents (bounded).
    /// `owner`: the caller is the owner. What the human is typing (a field's text, the outliner's
    /// search, and the rows that search filters while it is typed) is theirs alone (input sweep
    /// R3): anyone else gets `redacted` and nothing more (not even its length).
    pub(crate) fn editor_state_json(&mut self, detailed: bool, owner: bool) -> Json {
        let ui = &self.editor.ui;
        // What the human typed into the search box shows in what it found: for anyone but the
        // owner the rows, their count and their boxes are withheld while the search holds any
        // text (not only while the box has focus), and typed text is never measured for them (a
        // length polled per key gives the typing's rhythm and a secret's length away).
        let typing_search = !owner && !ui.search.is_empty();
        let sel: Vec<Json> = ui
            .selection
            .iter()
            .rev()
            .take(32)
            .filter_map(|id| self.world.get(*id).map(|e| Json::obj([("id", Json::from(*id)), ("path", Json::from(self.world.path_of(*id))), ("version", Json::from(e.version))])))
            .collect();
        let rows: Vec<Json> = ui
            .outliner
            .rows
            .iter()
            .take(if typing_search { 0 } else { usize::MAX })
            .enumerate()
            .map(|(i, r)| {
                let mut j = Json::obj([
                    ("row", Json::from(i)),
                    ("id", Json::from(r.id)),
                    ("path", Json::from(r.path.as_str())),
                    ("depth", Json::from(r.depth)),
                    ("has_children", Json::from(r.has_children)),
                    ("expanded", Json::from(r.expanded)),
                    ("last_changed_by", Json::from(r.by.as_str())),
                ]);
                if let Some(c) = &r.claim {
                    j.set("claimed_by", Json::from(c.as_str()));
                }
                if r.game {
                    j.set("game", Json::from(true));
                }
                j
            })
            .collect();
        let inspector = ui.inspect.as_ref().map_or(Json::Null, |i| {
            Json::obj([
                ("id", Json::from(i.id)),
                ("path", Json::from(i.path.as_str())),
                ("version", Json::from(i.version)),
                ("created_by", Json::from(i.created_by.as_str())),
                ("last_changed_by", Json::from(i.modified_by.as_str())),
                ("position", Json::arr(i.position.map(|v| Json::from(f64::from(v))))),
                ("rotation_deg", Json::arr(i.rotation.map(|v| Json::from(f64::from(v))))),
                ("scale", Json::arr(i.scale.map(|v| Json::from(f64::from(v))))),
                ("color", Json::arr(i.color.map(|v| Json::from(f64::from(v))))),
                ("visible", Json::from(i.visible)),
                ("claim", i.claim.as_deref().map_or(Json::Null, Json::from)),
            ])
        });
        let size = self.editor.rt.size;
        let mut j = Json::obj([
            ("open", Json::from(ui.is_open())),
            ("operator", Json::from(crate::authority::WINDOW_OWNER)),
            ("mode", Json::from(ui.mode.name())),
            ("space", Json::from(ui.space.name())),
            (
                "brush",
                match &ui.brush {
                    Some(b) => Json::obj([
                        ("kind", Json::from(b.kind.name())),
                        ("radius", Json::Num(f64::from(b.radius))),
                        ("amount", Json::Num(f64::from(b.amount))),
                        ("color", if b.kind == crate::editor_brush::BrushKind::Paint { Json::from(b.color.as_str()) } else { Json::Null }),
                        ("layer", if let crate::editor_brush::BrushKind::Splat(l) = b.kind { Json::from(l) } else { Json::Null }),
                        ("sweep", if let crate::editor_brush::BrushKind::Path(k) = b.kind { Json::from(k) } else { Json::Null }),
                        ("label", Json::from(b.label())),
                    ]),
                    None => Json::Null,
                },
            ),
            (
                "snap",
                Json::obj([("on", Json::from(ui.snap.on)), ("translate_m", Json::from(f64::from(ui.snap.translate))), ("rotate_deg", Json::from(f64::from(ui.snap.rotate))), ("scale", Json::from(f64::from(ui.snap.scale)))]),
            ),
            ("selected", Json::from(ui.selection.len())),
            ("selection", Json::Arr(sel)),
            ("hover", ui.hover.map_or(Json::Null, |h| Json::from(h.name()))),
            ("dragging", Json::from(self.editor.rt.drag.is_some())),
            ("gizmo", self.editor.rt.gizmo_px.as_ref().map_or(Json::Null, |(v, f, m)| gizmo_json(v, f, *m))),
            (
                "outliner",
                Json::obj([
                    ("page", if typing_search { Json::Null } else { Json::from(ui.outliner.page) }),
                    ("has_next", if typing_search { Json::Null } else { Json::from(ui.outliner.has_next) }),
                    (
                        "search",
                        if owner || ui.search.is_empty() {
                            Json::from(ui.search.as_str())
                        } else {
                            Json::obj([("redacted", Json::Bool(true))])
                        },
                    ),
                    ("rows_withheld", Json::from(typing_search)),
                    ("scanning", Json::from(ui.outliner.scanning)),
                    ("entities", if typing_search { Json::Null } else { Json::from(ui.outliner.total) }),
                    ("rows", Json::Arr(rows)),
                ]),
            ),
            ("inspector", inspector),
            (
                "typing",
                ui.typing().map_or(Json::Null, |(f, t)| {
                    if owner {
                        Json::obj([("field", Json::from(f.key())), ("text", Json::from(t))])
                    } else {
                        let _ = t;
                        Json::obj([("field", Json::from(f.key())), ("redacted", Json::Bool(true))])
                    }
                }),
            ),
            ("searching", Json::from(ui.searching())),
            ("panels_loaded", Json::from(ui.ui().is_some())),
            ("status", Json::obj([("text", Json::from(ui.status.0.as_str())), ("good", Json::from(ui.status.1))])),
            ("last_edit", self.editor.rt.last.clone().unwrap_or(Json::Null)),
            ("undo", Json::arr(ui.undo.iter().rev().take(16).map(|s| Json::from(*s)))),
            ("redo", Json::arr(ui.redo.iter().rev().take(16).map(|s| Json::from(*s)))),
            (
                "busy",
                Json::obj([
                    ("pick", Json::from(self.editor.rt.pick.is_some() || self.editor.rt.want_pick.is_some())),
                    ("edits", Json::from(self.editor.rt.outbox.len() + self.editor.rt.waiting.len())),
                    ("input_frames", Json::from(ui.injected.len())),
                ]),
            ),
            ("window", Json::arr([size.0, size.1].map(Json::from))),
            ("viewport", Json::arr(ui.viewport_rect(size).map(|v| Json::from(f64::from(v))))),
            ("driven_by", ui.driven_by.as_deref().map_or(Json::Null, Json::from)),
            // The swarm panels as shown (one page), and the Ask box: open or not and what it is
            // about, never the human's words.
            ("swarm", ui.swarm.to_json()),
            ("ask", ui.ask.status_json()),
        ]);
        if let Some(c) = self.editor.rt.cam {
            j.set("camera", c.camera().to_json());
        }
        if let Some(p) = &ui.problem {
            j.set("problem", Json::from(p.as_str()));
        }
        if detailed {
            j.set("widgets", ui.widget_boxes(160, !typing_search));
        }
        j
    }

    /// `editor_state` and `editor_input` (see `editor_tools`).
    pub(crate) fn editor_call(&mut self, call: crate::editor_tools::EditorCall, session: &SessionInfo, tool_call: u64, reply: crate::jobs::Reply, deadline: Instant) {
        match call {
            crate::editor_tools::EditorCall::State { detailed } => {
                let owner = crate::authority::authority(&self.swarm, &session.id) == crate::authority::Authority::Owner;
                let _ = reply.send(ToolOutput::text(self.editor_state_json(detailed, owner).to_string()));
            }
            crate::editor_tools::EditorCall::Input(frames) => {
                use crate::authority::{self, Authority, Site};
                use crate::inputguard::{done, refused, Actor, Channel};
                let have = authority::authority(&self.swarm, &session.id);
                let actor = Actor::of(&self.swarm, session);
                if let Err(e) = authority::require(have, Authority::Owner, Site::EditorInput, "editor_input (its edits are the window's human's; a refused try freezes your session: docs/design/SECURITY.md)") {
                    // A refused try at acting as the window's human: a security incident.
                    refused(Channel::EditorInput, "editor_input", &actor, e.code, &format!("{} frames", frames.len()));
                    reply.refuse(&e);
                    return;
                }
                // The owner's own (an --owner agent): seen and recorded, fine.
                let seen = done(Channel::EditorInput, "editor_input", &actor, &format!("{} frames", frames.len()));
                let by = self.swarm.agent_name(session);
                self.editor_input(frames, &by, tool_call, reply, deadline, seen);
            }
        }
    }

    /// What holds the window so the editor's input can't run (an engine screen above it, or the
    /// window minimized), if anything.
    fn editor_window_held(&self) -> Option<&'static str> {
        let (w, h) = self.editor.rt.size;
        if self.review_ui.is_open() {
            Some("the review screen (F9) is open")
        } else if self.taste.screen.is_open() {
            Some("the gallery screen (F8) is open")
        } else if self.editor.rt.framed && (w == 0 || h == 0) {
            Some("the window is minimized")
        } else if self.editor.ui.asking() {
            Some("the human is typing into the Ask the agents box (only their own input acts then)")
        } else {
            None
        }
    }

    /// The `window_held` refusal for an agent's input to the editor (`what`: the tool), if
    /// something holds the window; the input monitor is told who was turned away and why
    /// (review R1 of m5-edswarm, minor 11: these refusals never reached it).
    pub(crate) fn editor_held_refusal(&self, what: &str, by: &str) -> Option<crate::world::WorldError> {
        let held = self.editor_window_held()?;
        crate::inputguard::note(crate::inputguard::Channel::ScreenInput, "window_held", &format!("{what} by {by}: {held}"));
        Some(
            crate::world::WorldError::new("window_held", format!("{what}: the window is held ({held}); its frames would not run. Call again once it is free"))
                .with("held_by", held)
                .with("retry", true)
                .with("retry_after_s", 1),
        )
    }

    /// What a pass's frame said about the viewport's holds, traced and kept: a quiet hold that
    /// ended (the other side's), and presses that were not taken. An agent's refused presses
    /// are counted for its call (its answer says its drag did not start: review R2 of
    /// m5-eddraw, N1 and N3: the call answered as fed and the drag was a silent no-op); the
    /// human's are said on the status line.
    fn editor_note_holds(&mut self, view: &ViewIn, injected_pass: bool) {
        if view.hold_lapsed {
            let whose = if injected_pass { "human" } else { "agent" };
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "editor.hold_lapsed", whose = whose, driven_by = self.editor.ui.driven_by.clone().unwrap_or_default(), quiet_s = crate::editor::AGENT_HOLD_S);
        }
        if view.presses_refused == 0 {
            return;
        }
        if !injected_pass {
            // (The press that ended a quiet hold is not a press of its own: review R3, F6: it
            // was told the agent still held.)
            let line = if view.hold_lapsed {
                "You took the viewport back: the agent's drag had gone quiet and was let go, unsent. Press again to draw."
            } else {
                "An agent's drag holds the viewport: your press was not taken. It ends when the agent lets go; once the agent has been quiet for 5 s your press takes the viewport back."
            };
            self.editor.ui.set_status(line, view.hold_lapsed);
            return;
        }
        let call = self.editor.rt.input_call.1.as_ref().map_or(0, |d| d.call);
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "editor.press_refused", call = call, presses = u64::from(view.presses_refused), took_back = view.hold_lapsed);
        let kept = &mut self.editor.rt.refused_presses;
        *kept.entry(call).or_insert(0) += u32::from(view.presses_refused);
        while kept.len() > MAX_INPUT_WAITERS * 4 {
            kept.pop_first();
        }
    }

    /// The presses of tool call `call`'s frames that were not taken, as its answer says them
    /// (taken out: said once). `None`: all were taken.
    pub(crate) fn editor_refused_presses(&mut self, call: u64) -> Option<Json> {
        let n = self.editor.rt.refused_presses.remove(&call)?;
        Some(Json::obj([
            ("count", Json::from(n)),
            ("why", Json::from("a press in the viewport was not taken: the human's own hand held its buttons (or this press ended a hold of theirs that had gone quiet), so no drag or stroke began there. Presses on the panels are not counted here: they were the panels'")),
            ("retry", Json::from(true)),
        ]))
    }

    /// `ui_act {surface: "editor"}` (owner only; checked by the caller): its frames run on the
    /// editor as an agent's injected frames do (`editor_input`): its edit (one per call) is the
    /// window owner's, journaled with the agent as `via` and charged to the call. Returns the
    /// id its frames run under, or why it can't run now.
    pub(crate) fn editor_ui_act_begin(&mut self, by: &str) -> Result<u64, crate::world::WorldError> {
        if !self.windowed {
            return Err(crate::world::WorldError::new("no_window", "the editor surface is the window's editor: this engine runs headless (start it without --headless)"));
        }
        if let Some(e) = self.editor_held_refusal("ui_act on the editor", by) {
            return Err(e);
        }
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "editor.input", by = by, via = "ui_act");
        self.editor.ui.driven_by = Some(by.chars().take(64).collect());
        self.editor.rt.input_seq += 1;
        let id = self.editor.rt.input_seq;
        let driver = Driver { call: self.dispatch_call, trace: specula_core::trace::Ctx::current(), by: by.to_owned(), frames: 0 };
        self.editor.rt.input_tool_calls.insert(id, driver);
        Ok(id)
    }

    /// One `ui_act` frame on the editor (see [`Engine::editor_ui_act_begin`]): the events go to
    /// the editor alone, as `id`'s.
    pub(crate) fn editor_ui_act_frame(&mut self, id: u64, events: &[Event], pads: &[specula_platform::gamepad::GamepadState], size: (u32, u32)) {
        let driver = self.editor.rt.input_tool_calls.get(&id).cloned();
        if self.editor.ui.asking() || driver.is_none() {
            // The human opened the Ask box meanwhile: only their input acts now.
            return;
        }
        self.editor.rt.size = size;
        self.editor.rt.input_call = (id, driver);
        let open_before = self.editor.ui.is_open();
        self.editor.ui.injected_pass = true;
        let (acts, view) = self.editor.ui.frame(events, pads, size, false);
        self.editor.ui.injected_pass = false;
        self.editor_note_holds(&view, true);
        if self.editor.ui.is_open() && !open_before {
            self.editor_opened();
        }
        for a in acts {
            self.editor_act(a);
        }
        // (As the window frame's injected pass: not while the human's own hand holds a button.)
        if self.editor.ui.is_open() && !self.editor.ui.human_holds() {
            if view.focus_lost {
                self.editor_cancel_drag();
            }
            self.editor_viewport(&view, size);
        }
        self.editor.rt.input_call = (0, None);
    }

    /// The `ui_act` call is done: its frames ran (its edit is sent at the next pump, charged to it).
    /// The edits tool call `call`'s frames queued (their tools): each is sent by the window's
    /// next frame, so its outcome isn't known when the call answers.
    /// (And the ones already sent and not answered: a draw tool's stroke goes straight to the
    /// editor's tool thread. Review R1 of m5-eddraw, m3: `edit.queued` was null for those.)
    pub(crate) fn editor_queued_edits(&self, call: u64) -> Vec<&'static str> {
        let rt = &self.editor.rt;
        let queued = rt.outbox.iter().filter(|(_, _, _, d)| d.as_ref().is_some_and(|d| d.call == call)).map(|(tool, ..)| *tool);
        queued.chain(rt.waiting.iter().filter(|w| w.3 == call).map(|w| w.4)).collect()
    }

    pub(crate) fn editor_ui_act_end(&mut self, id: u64) {
        // Kept until its command is sent: the outbox holds its own copy of the driver.
        self.editor.rt.input_tool_calls.remove(&id);
    }

    /// `editor_input`: queues simulated window input as the window's human (owner only; checked
    /// by the caller) and answers once its frames have run.
    pub(crate) fn editor_input(&mut self, frames: Vec<Vec<Event>>, by: &str, tool_call: u64, reply: crate::jobs::Reply, deadline: Instant, _seen: crate::inputguard::Seen) {
        if !self.windowed {
            let _ = reply.send(ToolOutput::error(
                Json::obj([("error", Json::from("no_window")), ("message", Json::from("editor_input drives the window's editor: this engine runs headless (start it without --headless)"))]).to_string(),
            ));
            return;
        }
        // Refused at once (its frames would only wait out the call's deadline).
        if let Some(e) = self.editor_held_refusal("editor_input", by) {
            reply.refuse(&e);
            return;
        }
        if self.editor.ui.injected.len() + frames.len() > MAX_INJECTED || self.editor.rt.input_waiters.len() >= MAX_INPUT_WAITERS {
            let e = crate::authority::refusal(crate::authority::Site::JobBusy, format!("editor_input: {} frames already wait (at most {MAX_INJECTED}); call again when they have run", self.editor.ui.injected.len()))
                .with("retry_after_s", 1)
                .with("retry", true);
            reply.refuse(&e);
            return;
        }
        let n = frames.len();
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "editor.input", by = by, frames = n as u64);
        self.editor.ui.driven_by = Some(by.chars().take(64).collect());
        self.editor.ui.changed();
        self.editor.rt.input_seq += 1;
        let id = self.editor.rt.input_seq;
        // This call's trace context (the engine thread works inside it now): the edits its frames
        // make are journaled under it, with `by` as `via`.
        let driver = Driver { call: tool_call, trace: specula_core::trace::Ctx::current(), by: by.to_owned(), frames: n };
        self.editor.rt.input_tool_calls.insert(id, driver);
        self.editor.ui.injected.extend(frames.into_iter().map(|f| (id, f)));
        self.editor.rt.input_waiters.push((reply, deadline, id));
    }
}

/// A snapped drag's delta, so the preview shows exactly what the release writes and the gizmo's
/// entity lands on the grid: a world-space move puts the pivot on grid lines along the dragged
/// axes; a scale puts the gizmo entity's scale on the step. (The delta itself was snapped by
/// `Drag::delta`: steps along local axes, angles.) Every selected entity moves by the same delta.
fn grid_delta(d: &GizmoDrag, raw: Delta, snap: &gz::Snap) -> Delta {
    match raw {
        Delta::Move(m) if d.space == Space::World => {
            let grid = gz::grid_axes(d.drag.handle, Space::World);
            let to = d.pivot + m;
            let c = |i: usize, v: f32, p: f32| if grid[i] { gz::snap_to(v, snap.translate) - p } else { v - p };
            Delta::Move(Vec3::new(c(0, to.x, d.pivot.x), c(1, to.y, d.pivot.y), c(2, to.z, d.pivot.z)))
        }
        Delta::Grow(k) => {
            // The gizmo sits on the primary (the last item).
            let Some(it) = d.items.last() else { return raw };
            let s = it.start.scale;
            let fit = |k: f32, s: f32| {
                if (k - 1.0).abs() < 1e-7 || s.abs() < 1e-6 {
                    k
                } else {
                    gz::snap_to(s * k, snap.scale).max(snap.scale.max(1e-3)) / s
                }
            };
            Delta::Grow(Vec3::new(fit(k.x, s.x), fit(k.y, s.y), fit(k.z, s.z)))
        }
        other => other,
    }
}

fn frame_space(f: &gz::Frame) -> Space {
    if f.axes == [Vec3::X, Vec3::Y, Vec3::Z] {
        Space::World
    } else {
        Space::Local
    }
}

/// Axis colours (sight's) and the active handle's highlight.
fn draw_gizmo(p: &mut crate::sight_overlay::Painter, f: &gz::Frame, mode: Mode, active: Option<Handle>) {
    use crate::sight_overlay::{AXIS_X, AXIS_Y, AXIS_Z};
    const HOT: [f32; 4] = [1.0, 0.86, 0.3, 1.0];
    let colours = [AXIS_X, AXIS_Y, AXIS_Z];
    let pick = |h: Handle, c: [f32; 4]| if active == Some(h) { HOT } else { c };
    match mode {
        Mode::Translate | Mode::Scale => {
            for (i, &colour) in colours.iter().enumerate() {
                let c = pick(Handle::Axis(i), colour);
                let tip = f.origin + f.axes[i] * f.length;
                p.line(f.origin, tip, c, 3.0);
                if mode == Mode::Translate {
                    // An arrow head: two short strokes back from the tip.
                    let (a, _) = gz::Frame::plane_axes(i);
                    let back = f.axes[i] * (f.length * 0.14);
                    let side = f.axes[a] * (f.length * 0.06);
                    p.line(tip, tip - back + side, c, 3.0);
                    p.line(tip, tip - back - side, c, 3.0);
                } else {
                    p.dot(tip, 5.0, c);
                }
            }
            if mode == Mode::Translate {
                for (i, &colour) in colours.iter().enumerate() {
                    let q = f.plane_quad(i);
                    let mut c = pick(Handle::Plane(i), colour);
                    c[3] = if active == Some(Handle::Plane(i)) { 0.6 } else { 0.32 };
                    p.polygon(&q, c);
                }
            } else {
                p.dot(f.origin, 7.0, pick(Handle::Uniform, [0.9, 0.9, 0.9, 1.0]));
            }
        }
        Mode::Rotate => {
            for (i, &colour) in colours.iter().enumerate() {
                let c = pick(Handle::Ring(i), colour);
                let pts = f.ring(i);
                for w in pts.windows(2) {
                    p.line(w[0], w[1], c, 2.5);
                }
            }
        }
    }
}

/// Where the gizmo's handles are on screen (grab points), for agents driving the editor.
fn gizmo_json(view: &View, f: &gz::Frame, mode: Mode) -> Json {
    let px = |p: Vec3| view.project(p).map_or(Json::Null, |s| Json::arr([s[0], s[1]].map(|v| Json::from((f64::from(v) * 10.0).round() / 10.0))));
    let mut handles: Vec<(String, Json)> = Vec::new();
    match mode {
        Mode::Translate | Mode::Scale => {
            for i in 0..3 {
                handles.push((Handle::Axis(i).name(), px(f.origin + f.axes[i] * (f.length * 0.7))));
            }
            if mode == Mode::Translate {
                for i in 0..3 {
                    let q = f.plane_quad(i);
                    handles.push((Handle::Plane(i).name(), px((q[0] + q[2]) * 0.5)));
                }
            } else {
                handles.push((Handle::Uniform.name(), px(f.origin)));
            }
        }
        Mode::Rotate => {
            for i in 0..3 {
                let ring = f.ring(i);
                // A point on the ring facing the camera most (nearest the eye).
                let best = ring.iter().copied().min_by(|a, b| {
                    let da = (*a - view.camera.eye).length();
                    let db = (*b - view.camera.eye).length();
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                });
                handles.push((Handle::Ring(i).name(), best.map_or(Json::Null, px)));
            }
        }
    }
    Json::obj([
        ("origin", px(f.origin)),
        ("length_m", Json::from((f64::from(f.length) * 1000.0).round() / 1000.0)),
        ("handles", Json::Obj(handles)),
        ("axes", Json::arr(f.axes.iter().map(|a| v3(*a)))),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{SpawnSpec, World};

    /// What the flight does: read the pick's rectangle out of the whole image, and (a box) each
    /// id's total from it.
    fn pick_full(spec: &PickSpec, full: &specula_gpu::IdImage, entities: &[u64]) -> Json {
        let r = pick_read_rect(spec);
        let (x0, y0) = (r[0].min(full.width), r[1].min(full.height));
        let (x1, y1) = (r[2].clamp(x0, full.width), r[3].clamp(y0, full.height));
        let mut ids = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                ids.push(full.ids[(y * full.width + x) as usize]);
            }
        }
        let sub = specula_gpu::IdImage { width: x1 - x0, height: y1 - y0, ids };
        let mut totals = vec![0u64; entities.len()];
        for &v in &full.ids {
            if v >= 1 && (v as usize) <= entities.len() {
                totals[v as usize - 1] += 1;
            }
        }
        let totals = if spec.click { Vec::new() } else { totals };
        pick_ids(spec, &sub, [r[0], r[1]], entities, &totals)
    }

    fn img(w: u32, h: u32, f: impl Fn(u32, u32) -> u32) -> specula_gpu::IdImage {
        let mut ids = Vec::new();
        for y in 0..h {
            for x in 0..w {
                ids.push(f(x, y));
            }
        }
        specula_gpu::IdImage { width: w, height: h, ids }
    }

    fn spec(rect: [u32; 4], click: bool) -> PickSpec {
        PickSpec { camera: Camera::new(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, 50.0), width: 20, height: 10, rect, click }
    }

    #[test]
    fn a_click_takes_the_pixel_or_the_nearest() {
        // Left half entity 7 (dense 1), a 2-px gap, right part entity 9 (dense 2); proxies never.
        let i = img(20, 10, |x, _| if x < 8 { 1 } else if (10..18).contains(&x) { 2 } else if x >= 18 { 3 } else { 0 });
        let entities = [7, 9, 1 << 62];
        let ids = |j: Json| j.get("ids").unwrap().as_array().unwrap().iter().filter_map(Json::as_u64).collect::<Vec<u64>>();
        assert_eq!(ids(pick_full(&spec([2, 5, 3, 6], true), &i, &entities)), vec![7]);
        assert_eq!(ids(pick_full(&spec([8, 5, 9, 6], true), &i, &entities)), vec![7], "one pixel off: the nearest");
        assert_eq!(ids(pick_full(&spec([19, 5, 20, 6], true), &i, &entities)), vec![9], "a proxy's pixels are skipped; 9 is 2 px away");
        let empty = img(20, 10, |_, _| 0);
        assert!(ids(pick_full(&spec([5, 5, 6, 6], true), &empty, &entities)).is_empty());
        // Out of range ids and pixels outside the image are ignored.
        let odd = img(4, 4, |_, _| 99);
        assert!(ids(pick_full(&spec([1, 1, 2, 2], true), &odd, &entities)).is_empty());
        assert!(ids(pick_full(&spec([400, 400, 401, 401], true), &i, &entities)).is_empty());
    }

    #[test]
    fn a_box_lists_what_it_saw_most_first() {
        let i = img(20, 10, |x, y| if y < 2 { 0 } else if x < 4 { 1 } else if x < 16 { 2 } else { 0 });
        let entities = [7, 9];
        let j = pick_full(&spec([0, 0, 20, 10], false), &i, &entities);
        let ids: Vec<u64> = j.get("ids").unwrap().as_array().unwrap().iter().filter_map(Json::as_u64).collect();
        assert_eq!(ids, vec![9, 7]);
        let px: Vec<u64> = j.get("pixels").unwrap().as_array().unwrap().iter().filter_map(Json::as_u64).collect();
        assert_eq!(px, vec![12 * 8, 4 * 8]);
        // A rectangle past the image is clipped.
        let j = pick_full(&spec([4, 2, 9999, 9999], false), &i, &entities);
        assert_eq!(j.get("ids").unwrap().as_array().unwrap().iter().filter_map(Json::as_u64).collect::<Vec<_>>(), vec![9]);
        // An entity the box only overlaps (a sixth of its pixels inside) isn't taken along.
        let j = pick_full(&spec([0, 0, 6, 10], false), &i, &entities);
        assert_eq!(j.get("ids").unwrap().as_array().unwrap().iter().filter_map(Json::as_u64).collect::<Vec<_>>(), vec![7]);
        // A huge box over many entities keeps MAX_SELECTION.
        let many = img(600, 1, |x, _| x + 1);
        let ents: Vec<u64> = (0..600).collect();
        let j = pick_full(&PickSpec { rect: [0, 0, 600, 1], ..spec([0; 4], false) }, &many, &ents);
        assert_eq!(j.get("ids").unwrap().as_array().unwrap().len(), MAX_SELECTION);
    }

    fn cube_spec(name: &str) -> SpawnSpec {
        let mut s = SpawnSpec::new(crate::world::Shape::Cube);
        s.name = Some(name.into());
        s
    }

    /// A real engine (GPU-backed; skips cleanly without Vulkan).
    fn engine() -> Option<Engine> {
        if !specula_gpu::is_available() {
            crate::skip_test("editor_engine", "Vulkan is not available");
            return None;
        }
        let gpu = match specula_gpu::Gpu::new(&specula_gpu::GpuOptions { validation: specula_gpu::Validation::Auto, ..Default::default() }) {
            Ok(g) => g,
            Err(e) => {
                crate::skip_test("editor_engine", &e.to_string());
                return None;
            }
        };
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        let captures = std::env::temp_dir().join(format!("specula-editor-tests-{}", std::process::id()));
        Some(Engine::new(gpu, &captures, crate::models::Library::new(&assets, None)).expect("engine"))
    }

    fn spawn(e: &mut Engine, name: &str, by: &str) -> u64 {
        let w = Writer::new(by, Some(by));
        e.world.commit(&mut e.swarm, 0, w, crate::journal::Kind::Spawn, crate::write_policy::Rule::World, |wd| wd.spawn(cube_spec(name), by).map(|x| x.id)).unwrap().0
    }

    /// Starts a translate drag on the selection's X handle and moves it 1 m.
    fn drag(e: &mut Engine) {
        let view = View::new(Camera::new(Vec3::new(0.0, 2.0, 6.0), Vec3::ZERO, 50.0), 800, 600).unwrap();
        let frame = e.editor_gizmo_frame(&view).expect("a selection");
        let p = view.project(frame.origin + frame.axes[0] * (frame.length * 0.7)).unwrap();
        e.editor_begin_drag(&view, frame, Handle::Axis(0), p);
        e.editor.rt.drag.as_mut().expect("a drag").delta = Delta::Move(Vec3::X);
    }

    /// Review R1 (probe 1): a drag that spanned an agent's world_load committed its old ids and
    /// versions to whatever entity had them in the loaded world. A wholesale change resets every
    /// id the editor holds through the one hook, and a drag never commits across one.
    #[test]
    fn a_world_replacement_resets_the_editor_and_no_drag_commits_across_one() {
        let Some(mut e) = engine() else { return };
        let id = spawn(&mut e, "crate", "bob");
        e.editor.ui.select(&[id], false);
        e.editor.ui.did(7);
        e.editor.ui.redo.push(8);
        drag(&mut e);
        assert!(e.editor_send("world_set", Json::obj([("id", Json::from(id)), ("visible", Json::from(false))]), Why::Edit("hid".into())));
        e.editor.rt.want_pick = Some(([0, 0, 1, 1], true, false));
        e.editor.rt.outliner.expanded.insert(id);
        e.world_changed_wholesale();
        let rt = &e.editor.rt;
        assert!(rt.drag.is_none() && rt.outbox.is_empty() && rt.want_pick.is_none() && rt.pick.is_none() && rt.outliner.expanded.is_empty());
        let ui = &e.editor.ui;
        assert!(ui.selection.is_empty() && ui.inspect.is_none() && ui.undo.is_empty() && ui.redo.is_empty() && ui.edit_base.is_none());
        assert!(!e.editor.previewing());
        // The guard behind it: a drag begun in an older world generation writes nothing.
        e.editor.ui.select(&[id], false);
        drag(&mut e);
        e.world_generation += 1;
        e.editor_commit_drag();
        assert!(e.editor.rt.outbox.is_empty(), "nothing sent across a replacement");
        assert!(e.editor.ui.status.0.contains("not written"), "{}", e.editor.ui.status.0);
        // And in the same generation it commits one world_batch with the version it began at.
        drag(&mut e);
        e.editor_commit_drag();
        let (tool, args, ..) = e.editor.rt.outbox.first().expect("one command");
        assert_eq!(*tool, "world_batch");
        assert_eq!(args.get("ops").and_then(|o| o.as_array()).and_then(|o| o[0].get("expect_version")).and_then(Json::as_u64), Some(1));
    }

    /// Review R1 (probe 4): once an agent undid the human's top entry, Ctrl+Z was refused
    /// `already_undone` for ever. Lasting refusals set the entry aside, for undo and redo alike;
    /// transient ones keep it to try again: decided by the error's retry flag.
    #[test]
    fn lasting_undo_and_redo_refusals_set_the_entry_aside() {
        let Some(mut e) = engine() else { return };
        let refusal = |code: &str, retry: Option<bool>| {
            let mut j = Json::obj([("error", Json::from(code)), ("message", Json::from("m"))]);
            if let Some(r) = retry {
                j.set("retry", Json::from(r));
            }
            ToolOutput::error(j.to_string())
        };
        for (code, retry) in [
            ("already_undone", None),
            ("too_old", None),
            ("not_found", None),
            ("not_undoable", None),
            ("undo_conflict", None),
            ("not_yours", None),
            ("force_needs_lead", None),
            ("claimed", Some(false)),
        ] {
            for redo in [false, true] {
                e.editor.ui.undo = vec![1, 2];
                e.editor.ui.redo = vec![3, 4];
                let why = if redo { Why::Redo(e.editor.ui.redo.pop().unwrap()) } else { Why::Undo(e.editor.ui.undo.pop().unwrap()) };
                e.editor_answer(why, &refusal(code, retry));
                let stack = if redo { &e.editor.ui.redo } else { &e.editor.ui.undo };
                assert_eq!(stack, &vec![if redo { 3 } else { 1 }], "{code} (redo: {redo}) is lasting: set aside");
                assert!(e.editor.ui.status.0.contains("set aside"), "{}", e.editor.ui.status.0);
            }
        }
        for (code, retry) in [("claimed", None), ("engine_busy", None), ("not_found", Some(true))] {
            for redo in [false, true] {
                e.editor.ui.undo = vec![1, 2];
                e.editor.ui.redo = vec![3, 4];
                let why = if redo { Why::Redo(e.editor.ui.redo.pop().unwrap()) } else { Why::Undo(e.editor.ui.undo.pop().unwrap()) };
                e.editor_answer(why, &refusal(code, retry));
                let stack = if redo { &e.editor.ui.redo } else { &e.editor.ui.undo };
                assert_eq!(stack, &vec![if redo { 3 } else { 1 }, if redo { 4 } else { 2 }], "{code} (redo: {redo}) is transient: kept on top");
            }
        }
    }

    /// Review R1 (probe 6): the human typed over x at version 1, an agent set x = 5 (version 2),
    /// and Enter wrote the human's value at the latest version, losing the agent's silently. The
    /// edit's base (version and values) is the inspector when typing began.
    #[test]
    fn a_typed_value_is_sent_at_the_version_typing_began_at() {
        let Some(mut e) = engine() else { return };
        let id = spawn(&mut e, "a", "bob");
        e.editor.ui.select(&[id], false);
        e.editor_refresh_inspector();
        let base = e.editor.ui.inspect.clone();
        assert_eq!(base.as_ref().map(|i| i.version), Some(1));
        // Bob moves it meanwhile; the inspector shows his change.
        let set = crate::tools::parse_set(&Json::parse(r#"{"id": 0, "position": [5, 0, 2]}"#).unwrap()).unwrap().1;
        e.world.commit(&mut e.swarm, 0, Writer::new("bob", Some("bob")), crate::journal::Kind::Set, crate::write_policy::Rule::World, |wd| wd.update(&crate::world::EntityKey::Id(id), set, "bob").map(|x| x.id)).unwrap();
        e.editor.rt.inspected = None;
        e.editor_refresh_inspector();
        assert_eq!(e.editor.ui.inspect.as_ref().map(|i| i.version), Some(2));
        e.editor_commit_field(FieldKey::Pos(0), "3", base);
        let (tool, args, ..) = e.editor.rt.outbox.pop().expect("sent");
        assert_eq!(tool, "world_set");
        assert_eq!(args.get("expect_version").and_then(Json::as_u64), Some(1), "the version typing began at: bob's change refuses it");
        let pos: Vec<f64> = args.get("position").and_then(Json::as_array).unwrap().iter().filter_map(Json::as_f64).collect();
        assert_eq!(pos, vec![3.0, 0.0, 0.0], "the other components as the human saw them");
        // No base (typing began with nothing inspected) or another selection: not sent.
        e.editor_commit_field(FieldKey::Pos(0), "3", None);
        let other = spawn(&mut e, "b", "bob");
        let base = e.editor.ui.inspect.clone();
        e.editor.ui.select(&[other], false);
        e.editor_commit_field(FieldKey::Pos(0), "3", base);
        assert!(e.editor.rt.outbox.is_empty());
    }

    /// Review R1 (probe 5): an edit an agent drove through editor_input was journaled as the
    /// human with no trace of the agent. It is journaled with the agent as `via` and the
    /// editor_input call as `call`, still authored (and undoable) by the window's human.
    #[test]
    fn an_edit_driven_by_an_agent_names_it_and_its_call() {
        use crate::swarm::Swarm;
        use crate::write_policy::{Live, Rule};
        let mut sw = Swarm::default();
        sw.admit_window_operator(WINDOW_SESSION);
        let mut live = Live::new(World::default());
        let wo = crate::authority::WINDOW_OWNER;
        let w = Writer::new(wo, Some(wo)).via(Some("bob"));
        let _call = specula_core::trace::Ctx { call: 4242, span: 0 }.enter();
        let (_, seq) = live.commit(&mut sw, 0, w, crate::journal::Kind::Spawn, Rule::World, |wd| wd.spawn(cube_spec("a"), wo).map(|x| x.id)).unwrap();
        let entry = sw.journal().get(seq.unwrap()).unwrap();
        assert_eq!((&*entry.by, entry.via.as_deref(), entry.call), (wo, Some("bob"), 4242));
        let j = entry.to_json(false, None);
        assert_eq!(j.get("via").and_then(Json::as_str), Some("bob"));
    }

    /// Review R2 (probe 9): the human's own Delete, pressed while an agent's `editor_input` frames
    /// ran, was journaled with the agent as `via` and used up its call's one commit. Only commands
    /// caused by injected events carry the driver; the window's own events and empty `wait`
    /// frames carry none.
    #[test]
    fn only_injected_events_carry_their_agents_driver() {
        let Some(mut e) = engine() else { return };
        let a = spawn(&mut e, "a", "bob");
        let b = spawn(&mut e, "b", "bob");
        let c = spawn(&mut e, "c", "bob");
        e.windowed = true;
        let size = (800, 600);
        const VK_DELETE: u32 = 0x2E;
        let delete = || vec![Event::KeyDown { code: VK_DELETE, repeat: false }, Event::KeyUp { code: VK_DELETE }];
        e.input = vec![Event::KeyDown { code: 0x76, repeat: false }, Event::KeyUp { code: 0x76 }];
        e.editor_frame(size, false);
        if !e.editor.ui.is_open() {
            crate::skip_test("editor_engine", "the editor's UI did not load (no system fonts)");
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(60);
        let sent = |e: &mut Engine| std::mem::take(&mut e.editor.rt.outbox).into_iter().map(|(t, _, _, d)| (t, d.map(|d| (d.by, d.call)))).collect::<Vec<_>>();
        // The human presses Delete during an agent's two `wait` frames.
        e.editor.ui.select(&[a], false);
        let (reply, _rx) = crate::jobs::channel();
        e.editor_input(vec![vec![], vec![]], "own", 6, reply, deadline, crate::inputguard::seen_for_test());
        e.input = delete();
        e.editor_frame(size, false);
        assert_eq!(sent(&mut e), vec![("world_batch", None)], "the human's delete is the human's");
        assert!(e.editor.rt.edited_calls.is_empty(), "and uses none of the agent's call");
        e.input.clear();
        e.editor_frame(size, false);
        assert!(sent(&mut e).is_empty(), "an empty wait frame sends nothing");
        assert!(e.editor.rt.input_call.0 == 0 && e.editor.rt.input_call.1.is_none());
        // The human's Delete in the same frame as an injected (non-empty) one: still the human's.
        e.editor.ui.select(&[b], false);
        let (reply, _rx2) = crate::jobs::channel();
        e.editor_input(vec![vec![Event::MouseMove { x: 5, y: 5 }]], "own", 7, reply, deadline, crate::inputguard::seen_for_test());
        e.input = delete();
        e.editor_frame(size, false);
        assert_eq!(sent(&mut e), vec![("world_batch", None)]);
        // An injected Delete is the agent's, charged to its call.
        e.editor.ui.select(&[c], false);
        let (reply, _rx3) = crate::jobs::channel();
        e.editor_input(vec![delete()], "own", 8, reply, deadline, crate::inputguard::seen_for_test());
        e.input.clear();
        e.editor_frame(size, false);
        assert_eq!(sent(&mut e), vec![("world_batch", Some(("own".to_owned(), 8)))]);
        assert_eq!(e.editor.rt.input_call.0, 0, "no driver outlives its pass");
    }

    #[test]
    fn refusals_read_plainly() {
        let e = Json::obj([("error", Json::from("claimed")), ("message", Json::from("world_set: ann holds claim 3 on /a, which covers /a/b")), ("owner", Json::from("ann"))]);
        let s = explain(&e);
        assert!(s.starts_with("Refused: claimed.") && s.contains("ann holds claim 3"), "{s}");
        let e = Json::obj([("error", Json::from("stale_version")), ("message", Json::from("expected version 2, it is 3"))]);
        assert!(explain(&e).contains("changed meanwhile"));
        // It names who changed it (conflict help's theirs).
        let mut e = e;
        e.set("conflict", Json::parse(r#"{"theirs": {"version": 3, "changes": [{"journal": 9, "by": "bob", "tick": 1, "kind": "set"}]}}"#).unwrap());
        assert!(explain(&e).contains("Changed since by bob (journal 9)"), "{}", explain(&e));
    }

    /// The window's human is an identity like an agent's: owner authority, a reserved name, its
    /// own undo, and bound by other agents' claims (authority overrides objects, never claims).
    #[test]
    fn the_window_operator_is_an_owner_that_claims_bind() {
        use crate::claims::{ClaimSpec, Scope};
        use crate::journal::{Kind, UndoRequest};
        use crate::swarm::Swarm;
        use crate::write_policy::{Live, Rule};
        let alice = SessionInfo { id: "aaaaaa11".into(), client_name: "t".into(), client_version: "0".into(), protocol_version: "x".into() };
        let mut sw = Swarm::default();
        sw.admit_window_operator(WINDOW_SESSION);
        sw.hello(&alice, "alice".into(), "builder".into(), None, std::slice::from_ref(&alice)).unwrap();
        let win = window_session();
        assert_eq!(sw.agent_name(&win), crate::authority::WINDOW_OWNER);
        assert_eq!(sw.key(&win), crate::authority::WINDOW_OWNER);
        assert_eq!(crate::authority::authority(&sw, WINDOW_SESSION), crate::authority::Authority::Owner);
        let mallory = SessionInfo { id: "mmmmmm22".into(), ..alice.clone() };
        let e = sw.hello(&mallory, "window:owner".into(), "builder".into(), None, &[alice.clone(), mallory.clone()]).unwrap_err();
        assert!(matches!(e.code, "name_reserved" | "name_invalid"), "no agent says hello as the window's human: {}", e.code);
        for look_alike in ["Window:Owner", "WINDOW:OWNER"] {
            let s = SessionInfo { id: format!("lookalike-{look_alike}"), ..alice.clone() };
            let r = sw.hello(&s, look_alike.into(), "builder".into(), None, std::slice::from_ref(&s));
            assert!(r.is_err(), "{look_alike}: {r:?}");
        }
        let mut live = Live::new(World::default());
        let a = Writer::new("alice", Some("aaaaaa11"));
        let wo = Writer::new(crate::authority::WINDOW_OWNER, Some(crate::authority::WINDOW_OWNER));
        let (crate_id, _) = live.commit(&mut sw, 0, a, Kind::Spawn, Rule::World, |wd| wd.spawn(cube_spec("crate"), "alice").map(|e| e.id)).unwrap();
        let (other, _) = live.commit(&mut sw, 0, a, Kind::Spawn, Rule::World, |wd| wd.spawn(cube_spec("other"), "alice").map(|e| e.id)).unwrap();
        sw.take_claim(&live, &alice, ClaimSpec { scope: Scope::path("/crate").unwrap(), lease: Duration::from_secs(60), note: String::new() }).unwrap();
        let set = |pos: f32| {
            let args = Json::obj([("id", Json::from(0u64)), ("position", Json::arr([Json::from(f64::from(pos)), Json::from(0.0), Json::from(0.0)]))]);
            crate::tools::parse_set(&args).unwrap().1
        };
        let refused = live.commit(&mut sw, 0, wo, Kind::Set, Rule::World, |wd| wd.update(&crate::world::EntityKey::Id(crate_id), set(2.0), "window:owner").map(|e| e.id)).unwrap_err();
        assert_eq!(refused.code, "claimed");
        assert!(refused.message.contains("alice"), "{}", refused.message);
        let b = sw.claims.blocking("/crate", &[Vec3::ZERO], false, wo, Instant::now()).map(|c| c.owner.clone());
        assert_eq!(b.as_deref(), Some("alice"), "the look-ahead names the holder");
        // An unclaimed one: the human's edit, then its undo by journal id.
        let (_, seq) = live.commit(&mut sw, 0, wo, Kind::Set, Rule::World, |wd| wd.update(&crate::world::EntityKey::Id(other), set(3.0), "window:owner").map(|e| e.id)).unwrap();
        let seq = seq.expect("journaled");
        assert_eq!(&*sw.journal().get(seq).unwrap().by, crate::authority::WINDOW_OWNER);
        let u = live.undo(&mut sw, 0, wo, &UndoRequest { ids: Some(vec![seq]), count: 1, force: false }).unwrap();
        assert_eq!(u.undone, vec![seq]);
        assert_eq!(live.get(other).unwrap().position.x, 0.0);
        // Alice can't undo the human's edit without force.
        let (_, seq2) = live.commit(&mut sw, 0, wo, Kind::Set, Rule::World, |wd| wd.update(&crate::world::EntityKey::Id(other), set(4.0), "window:owner").map(|e| e.id)).unwrap();
        let e = live.undo(&mut sw, 0, a, &UndoRequest { ids: Some(vec![seq2.unwrap()]), count: 1, force: false }).unwrap_err();
        assert_eq!(e.code, "not_yours");
        // Many dead sessions later the window's human is still there.
        for i in 0..1100 {
            let s = SessionInfo { id: format!("{i:032x}"), ..alice.clone() };
            let _ = sw.hello(&s, format!("a{i}"), "builder".into(), None, std::slice::from_ref(&s));
        }
        assert_eq!(sw.agent_name(&win), crate::authority::WINDOW_OWNER, "never pruned");
    }

    /// Highlight and ask: only the human's own input opens the box and types into it; an agent's
    /// injected press is refused; while it is open editor_input is refused (`window_held`) and no
    /// agent read (editor_state, the editor screen's state that ui_tree reads) holds the words;
    /// Send posts one task_post as window:owner with the selection and the part.
    #[test]
    fn the_ask_box_is_the_humans_alone() {
        let Some(mut e) = engine() else { return };
        let id = spawn(&mut e, "statue", "bob");
        e.windowed = true;
        let size = (800, 600);
        e.input = vec![Event::KeyDown { code: 0x76, repeat: false }, Event::KeyUp { code: 0x76 }];
        e.editor_frame(size, false);
        if !e.editor.ui.is_open() {
            crate::skip_test("editor_engine", "the editor's UI did not load (no system fonts)");
            return;
        }
        e.editor.ui.select(&[id], false);
        // An agent's injected press (editor_input, ui_act) can't open it.
        e.editor.rt.input_call = (9, Some(Driver { call: 77, trace: specula_core::trace::Ctx::current(), by: "mallory".into(), frames: 1 }));
        e.editor_act(UiAct::AskOpen);
        e.editor.rt.input_call = (0, None);
        assert!(!e.editor.ui.asking(), "an injected press opens nothing");
        // The human's own press opens it.
        e.editor_act(UiAct::AskOpen);
        assert!(e.editor.ui.asking());
        const WORDS: &str = "secret plan for the head";
        e.input = WORDS.chars().map(Event::Char).collect();
        e.editor_frame(size, false);
        let state = e.editor_state_json(true, true).to_string();
        assert!(!state.contains("secret"), "editor_state never holds the words: {state}");
        assert!(state.contains("\"ask\":{\"open\":true"), "{state}");
        let screen = format!("{:?}", e.editor.ui.ui_parts().map(|(_, s, _)| s.clone()));
        assert!(!screen.contains("secret"), "the editor screen's state (what ui_tree reads) never holds them");
        // editor_input is refused while it is open, at once.
        let (reply, rx) = crate::jobs::channel();
        e.editor_input(vec![vec![]], "owner-proxy", 5, reply, Instant::now() + Duration::from_secs(5), crate::inputguard::seen_for_test());
        let out = rx.recv_timeout(Duration::from_secs(1)).expect("answered at once");
        assert!(out.is_error && answer_json(&out).get("error").and_then(Json::as_str) == Some("window_held"), "{:?}", answer_json(&out));
        assert_eq!(e.editor_ui_act_begin("owner-proxy").unwrap_err().code, "window_held", "and ui_act on the editor");
        // Enter: one task_post as window:owner, with the statue by identity and nothing else.
        e.input = vec![Event::KeyDown { code: 0x0D, repeat: false }];
        e.editor_frame(size, false);
        assert!(e.editor.ui.asking() && e.editor.ui.ask.sending(), "it stays open, with the words, until the board has answered");
        let (tool, args, why, driver) = e.editor.rt.outbox.pop().expect("sent");
        assert_eq!((tool, driver.is_none(), matches!(why, Why::Ask)), ("task_post", true, true), "the human's own command");
        assert_eq!(args.get("entities"), Some(&Json::arr([Json::from(id)])));
        assert_eq!(args.get("tags"), Some(&Json::arr([Json::from(crate::ask::TAG)])));
        assert!(args.get("detail").and_then(Json::as_str).is_some_and(|d| d.starts_with(WORDS)));
        assert!(args.get("parts").is_none(), "the whole statue (no part picked)");
        // Enter again while the answer is awaited sends nothing more.
        e.input = vec![Event::KeyDown { code: 0x0D, repeat: false }];
        e.editor_frame(size, false);
        assert!(e.editor.rt.outbox.is_empty(), "one request at a time");
        // Refused (a full board): the words are kept, and Enter sends them again (review R1,
        // minor 5: the box closed when the request was queued, and a refusal lost them).
        e.editor_answer(Why::Ask, &ToolOutput::error(r#"{"error": "board_full", "message": "the board is full"}"#.to_owned()));
        assert!(e.editor.ui.asking() && !e.editor.ui.ask.sending(), "still open after a refusal");
        assert_eq!(e.editor.ui.ask.typed_for_test(), WORDS, "with the words");
        e.input = vec![Event::KeyDown { code: 0x0D, repeat: false }];
        e.editor_frame(size, false);
        assert_eq!(e.editor.rt.outbox.len(), 1, "sent again");
        e.editor.rt.outbox.clear();
        // Posted: now it closes.
        e.editor_answer(Why::Ask, &ToolOutput::text(r#"{"id": 1}"#.to_owned()));
        assert!(!e.editor.ui.asking(), "posted: the box closes");
        // A screen taking the window closes it, dropping the words.
        e.editor_act(UiAct::AskOpen);
        e.input = "abc".chars().map(Event::Char).collect();
        e.editor_frame(size, false);
        e.editor_frame(size, true);
        assert!(!e.editor.ui.asking() && e.editor.rt.outbox.is_empty(), "closed, nothing sent");
    }

    /// Review R1 of m5-edswarm, B2: a lead put the review screen on the window while the human
    /// typed into the Ask box, and the human's Enter, meant to send the request, approved a
    /// submission as window:owner (the screens read a frame's keys before the editor's frame
    /// closed the box). The box closes before any screen reads the frame, and the keys of that
    /// frame reach no screen; Escape (or a click) is the human's own answer to what they see
    /// now, and gives the keys back.
    #[test]
    fn keys_typed_into_the_ask_box_never_reach_a_screen_an_agent_opened() {
        let Some(mut e) = engine() else { return };
        let id = spawn(&mut e, "statue", "bob");
        e.windowed = true;
        let size = (800, 600);
        e.input = vec![Event::KeyDown { code: 0x76, repeat: false }, Event::KeyUp { code: 0x76 }];
        e.editor_frame(size, false);
        if !e.editor.ui.is_open() {
            crate::skip_test("editor_engine", "the editor's UI did not load (no system fonts)");
            return;
        }
        e.editor.ui.select(&[id], false);
        e.editor_act(UiAct::AskOpen);
        e.input = "make it red".chars().map(Event::Char).collect();
        e.editor_frame(size, false);
        assert!(e.editor.ui.asking());
        // An agent's screen takes the window (the gallery here; the review screen comes the
        // same way), and the human, still typing, presses Enter to send.
        e.review_ui.open(None, &e.swarm.review, crate::inputguard::seen_for_test());
        e.input = vec![Event::Char('!'), Event::KeyDown { code: 0x0D, repeat: false }, Event::KeyUp { code: 0x0D }];
        e.text_window_screens();
        // Review R2, N1: the hold on the keys runs out 0.6 s after the last one; the Enter the
        // human presses after a pause must decide nothing either. The screen that took the
        // window has its focus on Close, not on Approve.
        assert_eq!(e.review_ui.pending_focus(), Some("rv_close"), "a screen over the human's typing: its focus is on Close");
        assert!(!e.editor.ui.asking(), "the box closed before any screen read the frame");
        assert!(!e.input.iter().any(|ev| matches!(ev, Event::KeyDown { .. } | Event::Char(_))), "and the keys meant for it reach no screen");
        assert!(e.editor.rt.outbox.is_empty(), "nothing was sent");
        // Escape is the human's answer to the screen they now see: it passes, and so do the
        // keys after it.
        e.input = vec![Event::KeyDown { code: 0x1B, repeat: false }];
        e.text_window_screens();
        assert_eq!(e.input.len(), 1, "Escape reaches the screen");
        e.input = vec![Event::KeyDown { code: 0x0D, repeat: false }];
        e.text_window_screens();
        assert_eq!(e.input.len(), 1, "and the keys are the screen's again");
    }

    /// Review R1 of m5-edswarm, minors 1 and 2. The journal's pages count from the first and
    /// go back and forth (the second page was labelled "page 1" with Newer and Newest off, so
    /// the newest page couldn't be reached again); and an entity the timeline is filtered to
    /// keeps its history when it is deleted, with the entry that deleted it (it said "No entries
    /// match" at the moment the human most wants to know who did it).
    #[test]
    fn the_timeline_pages_both_ways_and_keeps_a_deleted_entitys_history() {
        use crate::editor_swarm::{Place, SwAct};
        let Some(mut e) = engine() else { return };
        let ids: Vec<u64> = (0..25).map(|i| spawn(&mut e, &format!("n{i}"), "ann")).collect();
        e.editor.ui.swarm.shown = true;
        e.editor_swarm_refresh();
        let page = |e: &Engine| (e.editor.ui.swarm.page_label(), e.editor.ui.swarm.has_prev(), e.editor.ui.swarm.has_next);
        assert_eq!(page(&e), ("page 1".to_owned(), false, true));
        for want in [("page 2", true, true), ("page 3", true, false)] {
            e.editor_swarm_act(SwAct::Older);
            e.editor_swarm_refresh();
            assert_eq!(page(&e), (want.0.to_owned(), want.1, want.2));
        }
        for want in [("page 2", true, true), ("page 1", false, true)] {
            e.editor_swarm_act(SwAct::Newer);
            e.editor_swarm_refresh();
            assert_eq!(page(&e), (want.0.to_owned(), want.1, want.2), "Newer goes back a page, to the newest");
        }
        // The timeline of one entity, then bob deletes it.
        e.editor.ui.select(&[ids[3]], false);
        e.editor_swarm_act(SwAct::CyclePlace);
        assert!(matches!(e.editor.ui.swarm.place, Place::Entity(_)));
        e.editor_swarm_refresh();
        assert_eq!(e.editor.ui.swarm.rows.len(), 1);
        let bob = Writer::new("bob", Some("bob"));
        e.world
            .commit(&mut e.swarm, 0, bob, crate::journal::Kind::Delete, crate::write_policy::Rule::World, |wd| wd.delete(&crate::world::EntityKey::Id(ids[3]), None).map(|_| ()))
            .unwrap();
        e.editor.ui.swarm.built = None;
        e.editor_swarm_refresh();
        let labels: Vec<String> = e.editor.ui.swarm.rows.iter().map(|r| r.label.clone()).collect();
        assert_eq!(labels.len(), 2, "its history stays, with who deleted it: {labels:?}");
        assert!(labels[0].contains("bob: delete") && labels[1].contains("ann: spawn"), "{labels:?}");
    }

    /// Review R1 of m5-edswarm, minor 7: `ui_act` on the editor answers with how its edit went.
    /// Its answer is held while the edit its frames queued is unanswered, sent with the outcome
    /// when the edit is, and sent as it stood when the deadline nears or the edit is gone.
    #[test]
    fn a_ui_act_answer_on_the_editor_waits_for_its_edits_outcome() {
        let Some(mut e) = engine() else { return };
        let stood = || ToolOutput::text(r#"{"surface":"editor","frames":3,"edit":{"queued":["world_delete"],"outcome":"not known yet"}}"#.to_owned());
        let driver = |call: u64| Some(Driver { call, trace: specula_core::trace::Ctx::current(), by: "owner-proxy".into(), frames: 1 });
        let far = Instant::now() + Duration::from_secs(30);
        let text = |out: &ToolOutput| answer_json(out).to_string();
        // No edit of this call anywhere: nothing to wait for.
        let (reply, _rx) = crate::jobs::channel();
        assert!(e.editor_hold_answer(7, stood(), reply, far).is_some());
        // Presses that were not taken are said to the call that made them, once (review R2 of
        // m5-eddraw, N1: the call answered as fed and its drag was a silent no-op).
        e.editor.rt.input_call = (3, driver(40));
        e.editor_note_holds(&ViewIn { presses_refused: 2, ..ViewIn::default() }, true);
        e.editor_note_holds(&ViewIn { presses_refused: 1, hold_lapsed: true, ..ViewIn::default() }, true);
        e.editor.rt.input_call = (0, None);
        let said = e.editor_refused_presses(40).expect("counted for its call").to_string();
        assert!(said.contains(r#""count":3"#) && said.contains(r#""retry":true"#), "{said}");
        assert!(e.editor_refused_presses(40).is_none() && e.editor_refused_presses(41).is_none());
        // (The human's own refused press is said on their status line, and counted for no call;
        // the press that took the viewport back says that instead: review R3, F6.)
        e.editor_note_holds(&ViewIn { presses_refused: 1, ..ViewIn::default() }, false);
        assert!(e.editor.rt.refused_presses.is_empty() && e.editor.ui.status.0.contains("An agent's drag holds the viewport"));
        e.editor_note_holds(&ViewIn { presses_refused: 1, hold_lapsed: true, focus_lost: true, ..ViewIn::default() }, false);
        assert!(e.editor.ui.status.0.contains("You took the viewport back") && e.editor.ui.status.1, "{}", e.editor.ui.status.0);
        // A draw tool's stroke goes straight to the tool thread: it is an edit of the call
        // too, and named (review R1 of m5-eddraw, m3: `queued` was null for these).
        let (_tool_reply, tool_rx) = crate::jobs::channel();
        e.editor.rt.waiting.push((tool_rx, Why::Edit("Painted the model".into()), far, 12, "model_paint"));
        assert_eq!(e.editor_queued_edits(12), vec!["model_paint"]);
        let (reply, rx12) = crate::jobs::channel();
        assert!(e.editor_hold_answer(12, stood(), reply, far).is_none());
        e.editor.rt.waiting.clear();
        e.editor_act_sweep();
        assert!(rx12.try_recv().is_ok() && e.editor.rt.act_waiters.is_empty(), "its edit gone, it is answered as it stood");
        // Its edit is queued: the answer is held, and sent with the outcome when it comes.
        e.editor.rt.outbox.push(("world_delete", Json::Null, Why::Edit("Deleted /a".into()), driver(7)));
        assert_eq!(e.editor_queued_edits(7), vec!["world_delete"]);
        let (reply, rx) = crate::jobs::channel();
        assert!(e.editor_hold_answer(7, stood(), reply, far).is_none());
        // (Another call's, and the human's own, wait for nothing.)
        let (other, _rx2) = crate::jobs::channel();
        assert!(e.editor_hold_answer(8, stood(), other, far).is_some());
        let (human, _rx3) = crate::jobs::channel();
        assert!(e.editor_hold_answer(0, stood(), human, far).is_some());
        e.editor_act_sweep();
        assert!(rx.try_recv().is_err(), "held while its edit waits to be sent");
        e.editor.rt.outbox.clear();
        e.editor.ui.set_status("Deleted /a (journal 12, as window:owner).", true);
        e.editor_act_outcome(7, Some(&ToolOutput::text(r#"{"journal":12,"deleted":1}"#.to_owned())));
        let got = text(&rx.try_recv().expect("answered with the outcome"));
        assert!(got.contains(r#""edit":{"queued":["world_delete"],"done":true,"status":"Deleted /a (journal 12, as window:owner).","journal":12}"#), "{got}");
        assert!(got.contains(r#""frames":3"#) && !got.contains("not known yet"), "{got}");
        assert!(e.editor.rt.act_waiters.is_empty());
        // A refused edit says so, with the refusal's code.
        e.editor.rt.outbox.push(("world_delete", Json::Null, Why::Edit("Deleted /a".into()), driver(9)));
        let (reply, rx) = crate::jobs::channel();
        assert!(e.editor_hold_answer(9, stood(), reply, far).is_none());
        e.editor.rt.outbox.clear();
        e.editor.ui.set_status("Deleted /a: not done. Refused (claimed).", false);
        e.editor_act_outcome(9, Some(&ToolOutput::error(r#"{"error":"claimed","message":"bob holds it","retry":true}"#.to_owned())));
        let got = text(&rx.try_recv().unwrap());
        assert!(got.contains(r#""done":false"#) && got.contains(r#""error":"claimed""#) && got.contains(r#""retry":true"#), "{got}");
        // The deadline nears: sent as it stood. And an edit that is gone (the world was replaced
        // with it queued) leaves nothing to wait for.
        e.editor.rt.outbox.push(("world_delete", Json::Null, Why::Edit("x".into()), driver(10)));
        let (reply, rx) = crate::jobs::channel();
        assert!(e.editor_hold_answer(10, stood(), reply, Instant::now() + Duration::from_millis(50)).is_none());
        e.editor_act_sweep();
        assert!(text(&rx.try_recv().expect("sent before its deadline")).contains("not known yet"));
        let (reply, rx) = crate::jobs::channel();
        assert!(e.editor_hold_answer(10, stood(), reply, far).is_none());
        e.editor.rt.outbox.clear();
        e.editor_act_sweep();
        assert!(text(&rx.try_recv().expect("its edit is gone")).contains("not known yet"));
        // An error answer is never held; nor one past what is held at once.
        e.editor.rt.outbox.push(("world_delete", Json::Null, Why::Edit("x".into()), driver(11)));
        let (reply, _rx4) = crate::jobs::channel();
        assert!(e.editor_hold_answer(11, ToolOutput::error("{}".to_owned()), reply, far).is_some());
        let mut kept = Vec::new();
        for _ in 0..MAX_INPUT_WAITERS {
            let (reply, rx) = crate::jobs::channel();
            assert!(e.editor_hold_answer(11, stood(), reply, far).is_none());
            kept.push(rx);
        }
        let (reply, _rx5) = crate::jobs::channel();
        assert!(e.editor_hold_answer(11, stood(), reply, far).is_some(), "at most {MAX_INPUT_WAITERS} held");
    }

    /// Review R1 of m5-eddraw, M4: world metres per unit of a stroke's frame, so a brush's
    /// size on a scaled entity is what its ring shows.
    #[test]
    fn a_frames_scale_is_read_from_its_matrix() {
        use specula_core::math::Quat;
        let m = Mat4::from_trs(Vec3::new(5.0, 1.0, -2.0), Quat::IDENTITY, Vec3::new(3.0, 3.0, 3.0));
        let (across, up) = frame_scale(&m, true);
        assert!((across - 3.0).abs() < 1e-5 && (up - 3.0).abs() < 1e-5);
        // Uneven: the ground's mean (or all three for a paint brush), and the height's own.
        let m = Mat4::from_trs(Vec3::ZERO, Quat::IDENTITY, Vec3::new(2.0, 5.0, 4.0));
        let (ground, up) = frame_scale(&m, false);
        assert!((ground - 3.0).abs() < 1e-5 && (up - 5.0).abs() < 1e-5);
        assert!((frame_scale(&m, true).0 - 11.0 / 3.0).abs() < 1e-5);
        assert_eq!(frame_scale(&Mat4::IDENTITY, false), (1.0, 1.0));
        // Far from the origin the scale is read as exactly (review R2, N5: by a transformed
        // point it lost its digits there).
        let far = Mat4::from_trs(Vec3::new(9_000_000.0, 0.0, -9_000_000.0), Quat::IDENTITY, Vec3::new(0.01, 0.01, 0.01));
        let (across, up) = frame_scale(&far, true);
        assert!((across - 0.01).abs() < 1e-7 && (up - 0.01).abs() < 1e-7, "{across} {up}");
    }

    /// Review R1 of m5-edswarm, minor 13: a task's parts are checked against the entities they
    /// name, not only for shape.
    #[test]
    fn a_tasks_parts_are_checked_against_the_entities_they_name() {
        use crate::ask::{Part, PartKind};
        let Some(mut e) = engine() else { return };
        let id = spawn(&mut e, "plain", "ann");
        let span = [Part { entity: Some(0), kind: PartKind::TextSpan { start: 0, end: 5 } }];
        let refused = e.check_parts(&[id], &span).unwrap_err();
        assert!(refused.code == "invalid_argument" && refused.message.contains("parts[0]") && refused.message.contains("has no text"), "{}", refused.message);
        let head = [Part { entity: Some(0), kind: PartKind::RecipePart { name: "head".into() } }];
        let refused = e.check_parts(&[id], &head).unwrap_err();
        assert!(refused.message.contains("has no model"), "{}", refused.message);
        // A place needs no entity's leave.
        let area = [Part { entity: None, kind: PartKind::Area { center: [0.0; 3], radius: 2.0 } }];
        assert!(e.check_parts(&[id], &area).is_ok());
    }

    /// The draw tools: with a brush in hand the viewport's left button is the brush's. On
    /// something it can't draw on it says so and sends nothing, and it never selects; pressing a
    /// brush's button again steps it; Escape puts the brush down before it clears a selection;
    /// closing the Draw bar puts it down too; `editor_state` says what is in hand.
    #[test]
    fn a_brush_in_hand_has_the_left_button_and_selects_nothing() {
        use crate::editor::BrushAct;
        use crate::editor_brush::BrushKind;
        let Some(mut e) = engine() else { return };
        let id = spawn(&mut e, "cube", "bob");
        e.windowed = true;
        let size = (800, 600);
        e.input = vec![Event::KeyDown { code: 0x76, repeat: false }, Event::KeyUp { code: 0x76 }];
        e.editor_frame(size, false);
        if !e.editor.ui.is_open() {
            crate::skip_test("editor_engine", "the editor's UI did not load (no system fonts)");
            return;
        }
        e.editor.ui.select(&[id], false);
        e.editor_act(UiAct::Brush(BrushAct::Sculpt));
        assert_eq!(e.editor.ui.brush.as_ref().map(|b| b.kind), Some(BrushKind::Raise));
        assert!(e.editor.ui.draw_bar, "a brush opens its bar");
        // Its size by keys: P wider, O narrower (with no brush in hand they do nothing).
        let radius = |e: &Engine| e.editor.ui.brush.as_ref().map(|b| b.radius);
        let r0 = radius(&e).unwrap();
        e.input = vec![Event::KeyDown { code: 0x50, repeat: false }, Event::KeyUp { code: 0x50 }];
        e.editor_frame(size, false);
        assert_eq!(radius(&e), Some(r0 * 1.25), "P: a wider brush");
        e.input = vec![Event::KeyDown { code: 0x4F, repeat: false }, Event::KeyUp { code: 0x4F }];
        e.editor_frame(size, false);
        assert!((radius(&e).unwrap() - r0).abs() < 1e-5, "O: narrower again");
        // A press and release in the middle of the viewport: a cube is no terrain.
        let rect = e.editor.ui.viewport_rect(size);
        let p = [rect[0] + rect[2] * 0.5, rect[1] + rect[3] * 0.5];
        let button = |pressed: bool, held: bool, released: bool| ViewIn { pointer: Some(p), left: Btn { pressed, held, released }, ..ViewIn::default() };
        e.editor_viewport(&button(true, true, false), size);
        assert!(e.editor.rt.stroke.is_none() && e.editor.rt.press.is_none(), "no stroke begins, and no click or box select");
        assert!(e.editor_state_json(true, true).to_string().contains("Nothing to draw on there"), "it says why");
        e.editor_viewport(&button(false, false, true), size);
        assert!(e.editor.rt.want_pick.is_none(), "the brush's click selects nothing");
        assert_eq!(e.editor.ui.selection.to_vec(), vec![id], "the selection is as it was");
        assert!(e.editor.rt.outbox.is_empty() && e.editor.rt.waiting.is_empty(), "nothing was sent");
        // The button again: the next op. The state names what is in hand.
        e.editor_act(UiAct::Brush(BrushAct::Sculpt));
        assert_eq!(e.editor.ui.brush.as_ref().map(|b| b.kind), Some(BrushKind::Lower));
        e.editor_act(UiAct::Brush(BrushAct::Splat));
        e.editor_act(UiAct::Brush(BrushAct::Splat));
        let state = e.editor_state_json(true, true).to_string();
        assert!(state.contains(r#""brush":{"kind":"splat""#) && state.contains(r#""layer":1"#), "{state}");
        // Escape: the brush first, the selection only after it.
        e.editor_act(UiAct::Escape);
        assert!(e.editor.ui.brush.is_none() && !e.editor.ui.selection.is_empty());
        e.editor_act(UiAct::Escape);
        assert!(e.editor.ui.selection.is_empty());
        // "Ask the agents" with a brush in hand is about the spot the brush last touched on
        // the selection (carried from the reviews of m5-edswarm): chosen to begin with, sent
        // as an area part. A spot on another entity is nobody's.
        e.editor.ui.select(&[id], false);
        e.editor_act(UiAct::Brush(BrushAct::Sculpt));
        e.editor.rt.brush_spot = Some((id + 1000, Vec3::new(9.0, 9.0, 9.0), 1.0));
        e.editor_act(UiAct::AskOpen);
        if e.editor.ui.asking() {
            assert!(e.editor.ui.ask.status_json().to_string().contains("the whole selection"), "another entity's spot is not offered");
            e.editor.ui.ask.close("test");
            e.editor.rt.brush_spot = Some((id, Vec3::new(1.0, 0.5, -2.0), 1.5));
            e.editor_act(UiAct::AskOpen);
            let ask = e.editor.ui.ask.status_json().to_string();
            assert!(ask.contains("the spot under your brush (3.0 m wide)"), "{ask}");
            assert!(e.editor_state_json(true, true).to_string().contains("Asking about the spot under your brush"));
            let spot = crate::editor_ask::Choice::Spot { center: [1.0, 0.5, -2.0], radius: 1.5 };
            let part = spot.part(0).unwrap();
            assert_eq!(part.to_json().to_string(), r#"{"kind":"area","center":[1,0.5,-2],"radius":1.5}"#);
            assert!(e.check_parts(&[id], &[part]).is_ok());
            e.editor.ui.ask.close("test");
        }
        e.editor_act(UiAct::Escape);
        assert!(e.editor.ui.brush.is_none());
        // (Putting the brush down forgets its spot at the next viewport frame.)
        e.editor_viewport(&button(false, false, false), size);
        assert!(e.editor.rt.brush_spot.is_none());
        // The path tool on a model with no sweep part: said when it is picked, and a drag on
        // its ground plane is still a stroke (the tool answers for a recipe not read yet).
        e.editor.ui.select(&[id], false);
        e.editor_act(UiAct::Brush(BrushAct::Path));
        assert_eq!(e.editor.ui.brush.as_ref().map(|b| b.kind), Some(BrushKind::Path(0)));
        let state = e.editor_state_json(true, true).to_string();
        assert!(state.contains(r#""kind":"path""#) && state.contains(r#""sweep":0"#), "{state}");
        e.editor_act(UiAct::Brush(BrushAct::Path));
        assert!(matches!(e.editor.ui.brush.as_ref().map(|b| b.kind), Some(BrushKind::Path(_))), "pressed again: still the path tool");
        // A click is no path: nothing is sent, and it says so.
        e.editor_viewport(&button(true, true, false), size);
        if e.editor.rt.stroke.is_some() {
            e.editor_viewport(&button(false, false, true), size);
            assert!(e.editor.rt.outbox.is_empty() && e.editor.rt.waiting.is_empty(), "one point is no path");
            assert!(e.editor_state_json(true, true).to_string().contains("A path needs a drag"), "it says why");
        }
        e.editor_act(UiAct::Escape);
        // Closing the bar puts the brush down.
        e.editor_act(UiAct::Brush(BrushAct::Paint));
        assert_eq!(e.editor.ui.brush.as_ref().map(|b| b.kind), Some(BrushKind::Paint));
        e.editor_act(UiAct::Brush(BrushAct::Bar));
        assert!(e.editor.ui.brush.is_none() && !e.editor.ui.draw_bar);
        assert!(e.editor_state_json(true, true).to_string().contains(r#""brush":null"#));
    }

    /// The swarm panels fill from the engine's stores: the journal timeline names who changed
    /// what (filtered by the selected entity, by identity) and highlights it; the claims view
    /// lists claims with their holder; the cost view counts calls and commits per agent.
    #[test]
    fn the_swarm_panels_show_who_did_what() {
        use crate::editor_swarm::{Place, RowKey, SwAct, View};
        let Some(mut e) = engine() else { return };
        let a = spawn(&mut e, "a", "ann");
        let b = spawn(&mut e, "b", "bob");
        e.editor.ui.swarm.shown = true;
        e.editor_swarm_refresh();
        let labels = |e: &Engine| e.editor.ui.swarm.rows.iter().map(|r| r.label.clone()).collect::<Vec<_>>();
        assert!(labels(&e).iter().any(|l| l.contains("ann: spawn /a")) && labels(&e).iter().any(|l| l.contains("bob: spawn /b")), "{:?}", labels(&e));
        // The place filter: the selected entity (by identity).
        e.editor.ui.select(&[a], false);
        e.editor_swarm_act(SwAct::CyclePlace);
        assert!(matches!(e.editor.ui.swarm.place, Place::Entity(_)));
        e.editor_swarm_refresh();
        assert_eq!(labels(&e).len(), 1, "{:?}", labels(&e));
        e.editor_swarm_act(SwAct::Select(0));
        assert_eq!(e.editor.ui.swarm.highlight, vec![a]);
        assert!(e.editor.ui.swarm.detail.contains("by ann"), "{}", e.editor.ui.swarm.detail);
        // Claims, with their holders.
        let s = SessionInfo { id: "c".repeat(32), client_name: "c".into(), client_version: String::new(), protocol_version: String::new() };
        e.swarm.hello(&s, "carol".into(), "builder".into(), None, std::slice::from_ref(&s)).unwrap();
        let spec = crate::claims::ClaimSpec { scope: crate::claims::Scope::path("/b").unwrap(), lease: Duration::from_secs(60), note: String::new() };
        e.swarm.take_claim(&e.world, &s, spec).unwrap();
        e.editor_swarm_act(SwAct::View(View::Claims));
        e.editor_swarm_refresh();
        assert!(labels(&e).iter().any(|l| l == "carol: /b"), "{:?}", labels(&e));
        e.editor_swarm_act(SwAct::Select(0));
        assert_eq!(e.editor.ui.swarm.highlight, vec![b]);
        // Cost: calls and commits per agent.
        e.swarm.count_call("carol");
        e.swarm.count_commits("carol", 2);
        e.editor_swarm_act(SwAct::View(View::Cost));
        e.editor_swarm_refresh();
        assert!(labels(&e).iter().any(|l| l.starts_with("carol: 1 calls, 2 commits")), "{:?}", labels(&e));
        assert!(e.editor.ui.swarm.rows.iter().any(|r| r.key == RowKey::Agent("carol".into()) && r.badge == "tokens not reported"));
        // A world replacement drops what names the old world's entities.
        e.editor_swarm_act(SwAct::View(View::Journal));
        e.editor.ui.swarm.place = Place::All;
        e.editor.ui.select(&[a], false);
        e.editor_swarm_act(SwAct::CyclePlace);
        e.editor_swarm_refresh();
        e.editor_swarm_act(SwAct::Select(0));
        assert!(matches!(e.editor.ui.swarm.place, Place::Entity(_)) && e.editor.ui.swarm.highlight == vec![a]);
        e.world_changed_wholesale();
        assert!(e.editor.ui.swarm.highlight.is_empty() && e.editor.ui.swarm.place == Place::All, "an entity filter is the old world's");
        // A path filter is only a path: it stays.
        e.editor.ui.swarm.place = Place::Path("/a".into());
        e.world_changed_wholesale();
        assert_eq!(e.editor.ui.swarm.place, Place::Path("/a".into()));
    }

    /// The journal timeline pages back without gaps or repeats, filtered by agent too.
    #[test]
    fn the_journal_timeline_pages_back_through_every_entry() {
        use crate::editor_swarm::{RowKey, SwAct, ROWS};
        let Some(mut e) = engine() else { return };
        for i in 0..40 {
            spawn(&mut e, &format!("e{i:02}"), if i % 2 == 0 { "ann" } else { "bob" });
        }
        e.editor.ui.swarm.shown = true;
        let seqs = |e: &Engine| e.editor.ui.swarm.rows.iter().filter_map(|r| if let RowKey::Entry(s) = r.key { Some(s) } else { None }).collect::<Vec<u64>>();
        let mut all = Vec::new();
        for _ in 0..10 {
            e.editor.ui.swarm.built = None;
            e.editor_swarm_refresh();
            all.extend(seqs(&e));
            if !e.editor.ui.swarm.has_next {
                break;
            }
            e.editor_swarm_act(SwAct::Older);
        }
        let want: Vec<u64> = e.swarm.journal().entries_upto_rev(u64::MAX, false).map(|x| x.seq).collect();
        assert!(want.len() >= 40);
        assert_eq!(all, want, "every entry once, newest first, {ROWS} a page");
        e.editor_swarm_act(SwAct::Newest);
        e.editor.ui.swarm.agent = Some("bob".into());
        e.editor_swarm_refresh();
        assert!(e.editor.ui.swarm.rows.iter().all(|r| r.label.contains(" bob: ")), "only bob's");
    }

    #[test]
    fn the_orbit_camera_round_trips() {
        let c = Camera::new(Vec3::new(3.0, 4.0, 5.0), Vec3::new(1.0, 0.0, 0.0), 50.0);
        let back = Orbit::of(&c).camera();
        assert!((back.eye - c.eye).length() < 1e-3 && (back.target - c.target).length() < 1e-5, "{:?}", back.eye);
    }
}
