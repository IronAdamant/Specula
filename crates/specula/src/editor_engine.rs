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
    waiting: Vec<(Receiver<ToolOutput>, Why, Instant)>,
    pick: Option<(Receiver<ToolOutput>, bool, Instant)>,
    /// A pick asked for this frame: `(rect, click, add)`, sent at the pump.
    want_pick: Option<([u32; 4], bool, bool)>,
    drag: Option<GizmoDrag>,
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
}

/// The editor: what the window shows ([`EditorUi`]) and its engine-side state.
#[derive(Default)]
pub struct Editor {
    pub ui: EditorUi,
    rt: Rt,
}

impl Editor {
    /// A drag is moving entities on screen (the window's draw list is patched).
    pub fn previewing(&self) -> bool {
        self.rt.drag.as_ref().is_some_and(|d| !d.delta.is_identity())
    }
}

/// The text of a tool's answer, parsed (an unparsable one as `{"message": text}`).
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
        let injected = if blocked || minimized { None } else { self.editor.ui.next_injected() };
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
            let (acts, view) = if injected_pass { self.editor.ui.frame(&extra, &pads, size, blocked) } else { self.editor.ui.frame(&self.input, &pads, size, blocked) };
            if self.editor.ui.is_open() && !open_before {
                self.editor_opened();
            }
            for a in acts {
                self.editor_act(a);
            }
            if self.editor.ui.is_open() && !blocked && !minimized {
                if view.focus_lost {
                    self.editor_cancel_drag();
                }
                self.editor_viewport(&view, size);
            }
            ever_open |= self.editor.ui.is_open();
        }
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
                if self.editor.rt.drag.is_some() || self.editor.rt.press.is_some() {
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
                    FieldKey::Name => return,
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
        let what = if f == FieldKey::Name {
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
                FieldKey::Name => unreachable!("handled above"),
            };
            let c = match f {
                FieldKey::Pos(c) | FieldKey::Rot(c) | FieldKey::Scale(c) | FieldKey::Color(c) => c % 3,
                FieldKey::Name => 0,
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

    /// The gizmo's frame for the selection (at the primary's origin), if anything is selected.
    fn editor_gizmo_frame(&self, view: &View) -> Option<gz::Frame> {
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

    /// After the window frame's input: sends the editor's commands through the engine's request
    /// handler, reads their answers and the pick's, refreshes the panels, and points the window.
    pub(crate) fn editor_pump(&mut self, server: &Server) {
        let open = self.editor.ui.is_open();
        // Commands, exactly as an agent's calls.
        for (tool, args, why, driver) in std::mem::take(&mut self.editor.rt.outbox) {
            let command = match crate::tools::parse(tool, &args) {
                Ok(c) => c,
                Err(e) => {
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
            self.editor.rt.waiting.push((rx, why, Instant::now() + DEADLINE));
        }
        // Answers (most come at once; a deferred one a frame or more later).
        let mut i = 0;
        while i < self.editor.rt.waiting.len() {
            let got = self.editor.rt.waiting[i].0.try_recv();
            match got {
                Ok(out) => {
                    let (_, why, _) = self.editor.rt.waiting.remove(i);
                    self.editor_answer(why, &out);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) if Instant::now() < self.editor.rt.waiting[i].2 => i += 1,
                Err(_) => {
                    self.editor.rt.waiting.remove(i);
                    self.editor.ui.set_status("An edit got no answer (the engine dropped it); look at world_history for its outcome.", false);
                }
            }
        }
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
                    let s = state.get_or_insert_with(|| self.editor_state_json(false)).clone();
                    let _ = reply.send(ToolOutput::text(Json::obj([("fed", Json::from(true)), ("editor", s)]).to_string()));
                    self.editor.rt.input_tool_calls.remove(&id);
                } else if now > deadline {
                    let dropped = self.editor.ui.injected.iter().filter(|(c, _)| *c == id).count();
                    self.editor.ui.drop_injected(id);
                    let sent = self.editor.rt.input_tool_calls.remove(&id).map_or(dropped, |d| d.frames).max(dropped);
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
        let room = specula_gpu::MAX_OVERLAY_QUADS.saturating_sub(out.len());
        let scene = std::mem::take(&mut p.quads);
        self.editor.ui.overlay(&mut self.gpu, size, room, &scene, out);
    }

    /// Where the editor's pointer is (the last input it read).
    fn input_pointer(&self) -> Option<[f32; 2]> {
        self.editor.ui.last_pointer()
    }

    /// `editor_state`: what the editor shows, for agents (bounded).
    pub(crate) fn editor_state_json(&mut self, detailed: bool) -> Json {
        let ui = &self.editor.ui;
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
                    ("page", Json::from(ui.outliner.page)),
                    ("has_next", Json::from(ui.outliner.has_next)),
                    ("search", Json::from(ui.search.as_str())),
                    ("scanning", Json::from(ui.outliner.scanning)),
                    ("entities", Json::from(ui.outliner.total)),
                    ("rows", Json::Arr(rows)),
                ]),
            ),
            ("inspector", inspector),
            ("typing", ui.typing().map_or(Json::Null, |(f, t)| Json::obj([("field", Json::from(f.key())), ("text", Json::from(t))]))),
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
        ]);
        if let Some(c) = self.editor.rt.cam {
            j.set("camera", c.camera().to_json());
        }
        if let Some(p) = &ui.problem {
            j.set("problem", Json::from(p.as_str()));
        }
        if detailed {
            j.set("widgets", ui.widget_boxes(160));
        }
        j
    }

    /// `editor_state` and `editor_input` (see `editor_tools`).
    pub(crate) fn editor_call(&mut self, call: crate::editor_tools::EditorCall, session: &SessionInfo, tool_call: u64, reply: crate::jobs::Reply, deadline: Instant) {
        match call {
            crate::editor_tools::EditorCall::State { detailed } => {
                let _ = reply.send(ToolOutput::text(self.editor_state_json(detailed).to_string()));
            }
            crate::editor_tools::EditorCall::Input(frames) => {
                use crate::authority::{self, Authority, Site};
                let have = authority::authority(&self.swarm, &session.id);
                if let Err(e) = authority::require(have, Authority::Owner, Site::EditorInput, "editor_input (its edits are the window's human's)") {
                    reply.refuse(&e);
                    return;
                }
                let by = self.swarm.agent_name(session);
                self.editor_input(frames, &by, tool_call, reply, deadline);
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
        } else {
            None
        }
    }

    /// `editor_input`: queues simulated window input as the window's human (owner only; checked
    /// by the caller) and answers once its frames have run.
    pub(crate) fn editor_input(&mut self, frames: Vec<Vec<Event>>, by: &str, tool_call: u64, reply: crate::jobs::Reply, deadline: Instant) {
        if !self.windowed {
            let _ = reply.send(ToolOutput::error(
                Json::obj([("error", Json::from("no_window")), ("message", Json::from("editor_input drives the window's editor: this engine runs headless (start it without --headless)"))]).to_string(),
            ));
            return;
        }
        if let Some(held) = self.editor_window_held() {
            // Refused at once (its frames would only wait out the call's deadline).
            let e = crate::world::WorldError::new(
                "window_held",
                format!("editor_input: the window is held ({held}); its frames would not run. Call again once it is free"),
            )
            .with("held_by", held)
            .with("retry", true)
            .with("retry_after_s", 1);
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
        e.editor_input(vec![vec![], vec![]], "own", 6, reply, deadline);
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
        e.editor_input(vec![vec![Event::MouseMove { x: 5, y: 5 }]], "own", 7, reply, deadline);
        e.input = delete();
        e.editor_frame(size, false);
        assert_eq!(sent(&mut e), vec![("world_batch", None)]);
        // An injected Delete is the agent's, charged to its call.
        e.editor.ui.select(&[c], false);
        let (reply, _rx3) = crate::jobs::channel();
        e.editor_input(vec![delete()], "own", 8, reply, deadline);
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

    #[test]
    fn the_orbit_camera_round_trips() {
        let c = Camera::new(Vec3::new(3.0, 4.0, 5.0), Vec3::new(1.0, 0.0, 0.0), 50.0);
        let back = Orbit::of(&c).camera();
        assert!((back.eye - c.eye).length() < 1e-3 && (back.target - c.target).length() < 1e-5, "{:?}", back.eye);
    }
}
