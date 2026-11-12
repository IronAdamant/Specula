//! The human's keyboard in the window: typing into one focused text entity (keys, characters,
//! IME, the system clipboard), who may point the keyboard there, and the cue that says where it
//! goes (docs/design/TEXT.md, "The window's keyboard").
//!
//! **The keyboard and the clipboard are the human's** (review of cd954bd, major 2: any agent
//! could aim them at a hidden text and read what was typed, a paste included). Three review
//! rounds showed that working out on the CPU what the human sees under a click drifts from what
//! the renderer draws (review R3: glass or a higher 2D layer *behind* a text paints over it), so
//! the rule is small instead (review R3, lead direction):
//! - **only the operator** (`text_edit {window_focus: true}` from an owner or lead grant,
//!   [`crate::authority::Site::WindowFocus`]) points the keyboard at a world text. A click or a
//!   tap never does: any click anywhere ends the typing, and the game gets it. The human types
//!   into world texts through the editor's own fields (F7: the inspector's Text field);
//! - the focus drops when **anyone else writes** the text or one of its ancestors (their
//!   versions are recorded when it begins; the window's typing and the operator's own `text_edit`
//!   move them on), when its text is deleted, hidden, under half opaque, off screen or too small
//!   to read, when an engine screen takes the window, and on Escape;
//! - no key is applied until the window has drawn a **banner** naming the text, last in the
//!   frame's overlay (over everything: the world, 2D layers, glass, the game's interface and the
//!   engine's screens); a frame that can't draw it drops the focus. The banner is the human's
//!   guarantee of where their keys go: no occlusion test re-derives what they see;
//! - after the focus drops by itself, the keys the human goes on typing are withheld from the
//!   game for a moment ([`KEY_GUARD_GAP`]).
//!
//! The system clipboard is read and written on its own thread (a delayed-rendering owner can
//! block a read for as long as it likes): a paste or a cut waits for the clipboard's answer
//! through a [`crate::jobs::Reply`], the keys after it wait with it, and a clipboard that doesn't
//! answer within [`CLIPBOARD_WAIT`] is given up on and traced. Keys past a frame's
//! [`MAX_EDIT_OPS`] or layout budget are carried to the next frame, never dropped silently.

use crate::text_world::{apply_edits_with, faces, window_cost, TextSpec, MAX_EDIT_OPS, MAX_TEXT_BYTES};
use crate::world::{Entity, World};
use specula_core::math::Mat4;
use specula_gpu::{OverlayKind, OverlayQuad, TextureFormat, TextureId};
use specula_platform::Event;
use specula_text::edit::{EditOp, MemoryClipboard, Unit};
use specula_text::{AtlasConfig, GlyphAtlas, TextStyle};
use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

/// Undo steps the window keeps for the text it types into.
const WINDOW_UNDO: usize = 64;
/// Characters a window frame's typing may lay out (the engine thread's share; [`window_cost`]):
/// two layouts of the longest text, about 1 ms in release (Latin is the costliest per byte:
/// about 0.25 µs, CJK about 0.11). Keys past it wait for the next frame. A frame always takes
/// at least its first key (one key is at most [`crate::text_world::EDIT_LAYOUT_CHARS`]).
pub const WINDOW_LAYOUT_CHARS: usize = 2 * MAX_TEXT_BYTES;
/// Keys waiting for a later frame (past a frame's budget, or behind a clipboard answer); past
/// it, keys are dropped and traced (`text.window_typing_dropped`, reason `queue_full`).
pub const MAX_QUEUED_OPS: usize = 4096;
/// How long a paste or cut waits for the clipboard thread before giving up (traced).
pub const CLIPBOARD_WAIT: Duration = Duration::from_secs(2);
/// A focused text less opaque than this can't hold the keyboard (review R3: a builder faded an
/// opened field to 0.05 over a button of hers): the focus drops.
pub const MIN_FOCUS_OPACITY: f32 = 0.5;
/// A focused text whose box on screen is less than this tall (pixels) can't be read (or less
/// than a pixel wide).
pub const MIN_FOCUS_PX: f32 = 4.0;
/// After the focus drops by itself (its text hidden, written by another agent, deleted, or the
/// operator's stop),
/// the keys the human goes on typing are withheld from the game and its modules while they keep
/// coming at most this far apart: what was meant for the text never reaches a key logger
/// (review R2's sweep). Escape and a click end it at once.
pub const KEY_GUARD_GAP: Duration = Duration::from_millis(600);
/// The longest such a guard holds.
pub const KEY_GUARD_MAX: Duration = Duration::from_secs(5);
/// The banner's text size (pixels).
const CUE_PX: f32 = 15.0;
/// Longest path the banner shows (characters; a longer one is cut with an ellipsis).
const CUE_PATH_CHARS: usize = 64;

const VK_BACK: u32 = 0x08;
const VK_TAB: u32 = 0x09;
const VK_RETURN: u32 = 0x0D;
const VK_SHIFT: u32 = 0x10;
const VK_CONTROL: u32 = 0x11;
const VK_MENU: u32 = 0x12;
const VK_ESCAPE: u32 = 0x1B;
const VK_END: u32 = 0x23;
const VK_HOME: u32 = 0x24;
const VK_LEFT: u32 = 0x25;
const VK_UP: u32 = 0x26;
const VK_RIGHT: u32 = 0x27;
const VK_DOWN: u32 = 0x28;
const VK_DELETE: u32 = 0x2E;

/// What the clipboard thread answers: the text read (`None`: no text on it; always `None` for a
/// write), or why it failed.
pub type ClipAnswer = Result<Option<String>, String>;

/// A paste or cut waiting for the clipboard.
#[derive(Debug)]
struct ClipWait {
    cut: bool,
    started: Instant,
    rx: Receiver<ClipAnswer>,
}

/// Where the window's keyboard types: the focused text entity, the modifier keys held, the undo
/// history of this focus, keys not applied yet, and the clipboard answer awaited.
#[derive(Debug, Default)]
pub struct WindowTyping {
    pub focus: Option<u64>,
    /// The focused text's path (the banner and the title name it).
    path: String,
    shift: bool,
    ctrl: bool,
    alt: bool,
    /// The text before each change the window made (newest last), and undone ones.
    undo: Vec<TextSpec>,
    redo: Vec<TextSpec>,
    history_of: Option<u64>,
    /// The entity's version after the window's last change: a version that moved since (another
    /// writer) drops the history, so Ctrl+Z never writes back a stale copy over their change.
    version: Option<u64>,
    /// The last window frame's view-projection and size (focus checks and clicks use it).
    view: Option<(Mat4, (u32, u32))>,
    /// Keys not applied yet, oldest first.
    queued: VecDeque<EditOp>,
    clip: Option<ClipWait>,
    /// A clipboard request that timed out and hasn't answered since: until it does, paste, cut
    /// and copy are refused (traced) rather than queued behind it.
    stuck: Option<Receiver<ClipAnswer>>,
    /// The cut at the front of `queued` is one the clipboard already holds.
    cut_copied: bool,
    cue: FocusCue,
    /// The focused text and its ancestors (text first, then up the hierarchy) with their
    /// versions as the focus began or the operator or the window last changed them: a version
    /// that moved since is another writer's change, and the focus drops (review R3).
    watch: Vec<(u64, u64)>,
    /// The banner as the last window frame drew it (`None`: not drawn since the focus began).
    banner: Option<BannerDrawn>,
    /// Keys withheld after the focus dropped by itself: when the guard began, and the last key.
    guard: Option<(Instant, Instant)>,
    /// Who pointed the keyboard here (the input monitor judges the clipboard's accesses by it:
    /// a paste into a text a lead chose is an alert; docs/design/SECURITY.md).
    pointer: Option<crate::inputguard::Actor>,
}

/// The banner a window frame drew: for which focus, how many overlay quads, and whether they
/// were the last of the frame's overlay (drawn over everything else).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BannerDrawn {
    pub id: u64,
    pub quads: usize,
    pub last: bool,
    /// Window frames that drew it for this focus.
    pub frames: u64,
}

/// The refusal for an agent other than the operator putting a screen (`tool`) on the window
/// while the human types: into a text the operator pointed their keyboard at, or into the
/// editor's "Ask the agents" box.
pub fn typing_busy(tool: &str) -> crate::world::WorldError {
    crate::world::WorldError::new(
        "window_typing",
        format!("the human is typing in the window (into a text, or the editor's Ask the agents box): {tool} waits until they stop (Escape or a click), or the owner or a granted lead puts it there"),
    )
    .with("tool", tool)
}

/// The versions a held focus watches: text `id`, then each ancestor (at most the world's depth).
pub fn watch_of(world: &World, id: u64) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut at = Some(id);
    while let Some(i) = at {
        let Some(e) = world.get(i) else { break };
        out.push((i, e.version));
        if out.len() > crate::world::MAX_DEPTH {
            break;
        }
        at = e.parent;
    }
    out
}

impl WindowTyping {
    /// Ctrl held without Alt: a command. Ctrl+Alt is AltGr on many layouts (German '@' and '€',
    /// Polish 'ą'): those keys type characters, never commands.
    fn command(&self) -> bool {
        self.ctrl && !self.alt
    }

    /// The window's last frame drew with `view_proj` at `size` (physical pixels).
    pub fn set_view(&mut self, view_proj: Mat4, size: (u32, u32)) {
        self.view = Some((view_proj, size));
    }

    /// The last window frame's view-projection and size (`None` before the first, or headless).
    pub fn view(&self) -> Option<&(Mat4, (u32, u32))> {
        self.view.as_ref()
    }

    /// The banner the last window frame drew for the focus held now (`None`: none yet).
    pub fn banner(&self) -> Option<BannerDrawn> {
        self.banner.filter(|b| Some(b.id) == self.focus)
    }

    /// For engine_info's `window.typing`: the focused text and its banner (`null`: no focus).
    pub fn json(&self) -> specula_core::Json {
        use specula_core::Json;
        let Some(id) = self.focus else { return Json::Null };
        let banner = self.banner().map_or(Json::Null, |b| Json::obj([("quads", Json::from(b.quads)), ("last", Json::from(b.last)), ("frames", Json::from(b.frames))]));
        Json::obj([("id", Json::from(id)), ("path", Json::from(self.path.as_str())), ("banner", banner)])
    }

    /// Why the held focus must drop because someone else wrote its text or an ancestor: a
    /// watched version that moved (`foreign_write`), or an ancestor gone (`deleted`). At most
    /// the hierarchy's depth of lookups: no walk of the world.
    pub fn foreign_write(&self, world: &World) -> Option<&'static str> {
        self.focus?;
        for (id, version) in &self.watch {
            match world.get(*id) {
                None => return Some("deleted"),
                Some(e) if e.version != *version => return Some("foreign_write"),
                Some(_) => {}
            }
        }
        None
    }

    /// The operator's own `text_edit`, or the window's typing, changed the focused text from
    /// version `before` to `after`: the watch moves on with it, but only from the version it
    /// recorded (another writer's change before it still drops the focus).
    pub fn wrote(&mut self, id: u64, before: u64, after: u64) {
        if let Some(w) = self.watch.first_mut().filter(|w| w.0 == id && w.1 == before) {
            w.1 = after;
        }
    }

    /// The focused text's path, while there is one (the window's title names it).
    pub fn focused_path(&self) -> Option<&str> {
        self.focus.map(|_| self.path.as_str())
    }

    /// Points the keyboard at text `id` for `by` (who did: the input monitor saw it first, `seen`),
    /// watching the versions `watch` ([`watch_of`]). Keys and a clipboard answer meant for another
    /// text are dropped; no key applies until a window frame drew the banner for it.
    pub fn focus_on(&mut self, id: u64, path: String, by: crate::inputguard::Actor, watch: Vec<(u64, u64)>, seen: crate::inputguard::Seen) {
        if self.focus != Some(id) {
            self.forget_pending("focus_moved");
            self.banner = None;
        }
        self.focus = Some(id);
        self.path = path;
        self.watch = watch;
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "text.window_focus", id = id, path = self.path.as_str(), by = by.name(), monitor = seen.seq());
        self.pointer = Some(by);
    }

    /// Takes the keyboard off its text (`reason` is traced).
    pub fn drop_focus(&mut self, reason: &str) {
        let Some(id) = self.focus.take() else { return };
        // The human's own leaving (a click, opening the editor) withholds nothing.
        if !matches!(reason, "clicked_elsewhere" | "editor_opened") {
            // Not the human's own doing: what they go on typing stays out of the game.
            let now = Instant::now();
            self.guard = Some((now, now));
        }
        self.forget_pending(reason);
        self.undo.clear();
        self.redo.clear();
        self.history_of = None;
        self.watch.clear();
        self.banner = None;
        self.pointer = None;
        crate::inputguard::note(crate::inputguard::Channel::Keyboard, "focus_dropped", reason);
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "text.window_focus_dropped", id = id, path = self.path.as_str(), reason = reason);
    }

    /// While a guard holds ([`KEY_GUARD_GAP`]), the frame's keys, characters and IME events
    /// are withheld from the game (traced); a click, Escape, or a pause in typing ends it.
    fn guard_keys(&mut self, events: &mut Vec<Event>) {
        self.guard_keys_at(events, Instant::now());
    }

    /// Withholds what the human goes on typing ([`TextTyping::guard_keys`]) for typing that
    /// ended outside a text entity: the editor's "Ask the agents" box, closed under them.
    pub fn hold_keys(&mut self, now: Instant, reason: &str) {
        self.guard = Some((now, now));
        crate::inputguard::note(crate::inputguard::Channel::Keyboard, "keys_held", reason);
    }

    /// [`TextTyping::guard_keys`] at `now` (one instant for a frame: a guard armed in a frame
    /// holds that frame's keys whatever the machine's speed).
    fn guard_keys_at(&mut self, events: &mut Vec<Event>, now: Instant) {
        let Some((began, last)) = self.guard else { return };
        let human = events.iter().any(|e| {
            matches!(e, Event::MouseDown { .. } | Event::PointerDown { .. } | Event::KeyDown { code: VK_ESCAPE, .. })
        });
        if human || now.duration_since(last) > KEY_GUARD_GAP || now.duration_since(began) > KEY_GUARD_MAX {
            self.guard = None;
            return;
        }
        let before = events.len();
        events.retain(|e| !matches!(e, Event::KeyDown { .. } | Event::Char(_) | Event::ImeComposition { .. } | Event::ImeCommit(_) | Event::ImeEnd));
        let withheld = before - events.len();
        if withheld > 0 {
            self.guard = Some((began, now));
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "text.window_keys_withheld", events = withheld as u64, path = self.path.as_str());
        }
    }

    /// The live world was replaced: its entity ids name other entities now.
    pub fn world_replaced(&mut self) {
        self.drop_focus("world_replaced");
    }

    fn forget_pending(&mut self, reason: &str) {
        let waiting = self.queued.len() + usize::from(self.clip.is_some());
        if waiting > 0 {
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "text.window_typing_dropped", ops = waiting as u64, reason = reason);
        }
        self.queued.clear();
        self.cut_copied = false;
        if let Some(w) = self.clip.take() {
            self.stuck.get_or_insert(w.rx);
        }
    }

    /// The edit operations `events` mean for the focused text, and whether Escape ended the
    /// focus. Keys, characters and IME events are consumed; everything else is left in `events`
    /// for the game. An IME's result arrives as `Char` events (after `ImeCommit`, which only ends
    /// the composition shown at the caret). Every key is returned: [`Engine::text_window_input`]
    /// carries what a frame can't apply to the next.
    pub fn take_ops(&mut self, events: &mut Vec<Event>) -> (Vec<EditOp>, bool) {
        let mut ops = Vec::new();
        let mut escape = false;
        events.retain(|e| {
            match e {
                Event::KeyDown { code, .. } => {
                    let cmd = self.command();
                    let unit = if cmd { Unit::Word } else { Unit::Char };
                    let select = self.shift;
                    let op = match *code {
                        VK_SHIFT | 0xA0 | 0xA1 => {
                            self.shift = true;
                            None
                        }
                        VK_CONTROL | 0xA2 | 0xA3 => {
                            self.ctrl = true;
                            None
                        }
                        VK_MENU | 0xA4 | 0xA5 => {
                            self.alt = true;
                            None
                        }
                        VK_ESCAPE => {
                            escape = true;
                            None
                        }
                        VK_BACK => Some(EditOp::Backspace(unit)),
                        VK_DELETE => Some(EditOp::Delete(unit)),
                        VK_LEFT => Some(EditOp::Left { unit, select }),
                        VK_RIGHT => Some(EditOp::Right { unit, select }),
                        VK_UP => Some(EditOp::Up { select }),
                        VK_DOWN => Some(EditOp::Down { select }),
                        VK_HOME => Some(EditOp::Left { unit: if cmd { Unit::All } else { Unit::Line }, select }),
                        VK_END => Some(EditOp::Right { unit: if cmd { Unit::All } else { Unit::Line }, select }),
                        VK_RETURN => Some(EditOp::Insert("\n".into())),
                        VK_TAB => None,
                        0x41 if cmd => Some(EditOp::SelectAll),
                        0x43 if cmd => Some(EditOp::Copy),
                        0x58 if cmd => Some(EditOp::Cut),
                        0x56 if cmd => Some(EditOp::Paste),
                        0x5A if cmd => Some(EditOp::Undo),
                        0x59 if cmd => Some(EditOp::Redo),
                        _ => None,
                    };
                    ops.extend(op);
                }
                Event::KeyUp { code } => match *code {
                    VK_SHIFT | 0xA0 | 0xA1 => self.shift = false,
                    VK_CONTROL | 0xA2 | 0xA3 => self.ctrl = false,
                    VK_MENU | 0xA4 | 0xA5 => self.alt = false,
                    _ => {}
                },
                Event::Focused(false) => {
                    (self.shift, self.ctrl, self.alt) = (false, false, false);
                    return true;
                }
                // Ctrl+letter never arrives as a character (the platform keeps control
                // characters out of `Char`), so every `Char` is text, AltGr's included.
                Event::Char(c) => ops.push(EditOp::Insert(c.to_string())),
                Event::ImeComposition { text, caret } => ops.push(EditOp::Compose { text: text.clone(), caret: *caret as usize }),
                Event::ImeCommit(_) | Event::ImeEnd => ops.push(EditOp::CancelComposition),
                _ => return true,
            }
            false
        });
        (ops, escape)
    }

    /// Queues `ops` behind the keys already waiting (past [`MAX_QUEUED_OPS`] they are dropped,
    /// and traced).
    fn queue(&mut self, ops: Vec<EditOp>) {
        let room = MAX_QUEUED_OPS.saturating_sub(self.queued.len());
        if ops.len() > room {
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "text.window_typing_dropped", ops = (ops.len() - room) as u64, reason = "queue_full", cap = MAX_QUEUED_OPS as u64);
        }
        self.queued.extend(ops.into_iter().take(room));
    }

    /// The next keys to apply this frame: at most [`MAX_EDIT_OPS`], within the layout budget for
    /// `spec` (the rest stay queued for the next frame), up to and including the first paste or
    /// cut (which waits for the clipboard: returned apart).
    fn next_batch(&mut self, spec: &TextSpec, ending: bool) -> (Vec<EditOp>, Option<EditOp>) {
        let mut batch: Vec<EditOp> = Vec::new();
        let mut barrier = None;
        while batch.len() < MAX_EDIT_OPS {
            let Some(op) = self.queued.front() else { break };
            let copied_cut = *op == EditOp::Cut && std::mem::take(&mut self.cut_copied);
            if matches!(op, EditOp::Paste | EditOp::Cut) && !copied_cut {
                barrier = self.queued.pop_front();
                break;
            }
            batch.push(op.clone());
            if batch.len() > 1 && window_cost(spec, &batch, ending).chars > WINDOW_LAYOUT_CHARS {
                batch.pop();
                break;
            }
            self.queued.pop_front();
        }
        (batch, barrier)
    }
}

// ---------------------------------------------------------------------------------------------
// The clipboard thread
// ---------------------------------------------------------------------------------------------

enum ClipReq {
    Get,
    Set(String),
}

type ClipQueue = Sender<(ClipReq, Option<crate::jobs::Reply<ClipAnswer>>)>;

/// The clipboard thread's queue (started on first use; `None` when it couldn't start).
fn clipboard_thread() -> Option<&'static Mutex<ClipQueue>> {
    static QUEUE: OnceLock<Option<Mutex<ClipQueue>>> = OnceLock::new();
    QUEUE
        .get_or_init(|| {
            let (tx, rx) = std::sync::mpsc::channel::<(ClipReq, Option<crate::jobs::Reply<ClipAnswer>>)>();
            let started = std::thread::Builder::new().name("specula-clipboard".into()).spawn(move || {
                for (req, reply) in rx {
                    let (op, answer) = match req {
                        ClipReq::Get => ("get", specula_platform::textinput::clipboard_text().map_err(|e| e.message)),
                        ClipReq::Set(text) => ("set", specula_platform::textinput::set_clipboard_text(&text).map(|()| None).map_err(|e| e.message)),
                    };
                    if let Err(m) = &answer {
                        specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "text.clipboard_failed", op = op, message = m.as_str());
                    }
                    if let Some(r) = reply {
                        r.send(answer);
                    }
                }
            });
            started.ok().map(|_| Mutex::new(tx))
        })
        .as_ref()
}

/// Sends `req` to the clipboard thread; the answer comes through the returned receiver when
/// `answer` (`None`: the thread couldn't start). The input monitor saw it first (`_seen`): the
/// system clipboard is reached nowhere else (inputguard_guard).
fn clipboard(req: ClipReq, answer: bool, _seen: crate::inputguard::Seen) -> Option<Option<Receiver<ClipAnswer>>> {
    let queue = clipboard_thread()?;
    let (reply, rx) = if answer {
        let (r, rx) = crate::jobs::channel();
        (Some(r), Some(rx))
    } else {
        (None, None)
    };
    queue.lock().unwrap_or_else(PoisonError::into_inner).send((req, reply)).ok()?;
    Some(rx)
}

/// The clipboard's text cut to what a text holds (at a character boundary).
fn fit(mut s: String) -> String {
    if s.len() > MAX_TEXT_BYTES {
        let mut end = MAX_TEXT_BYTES;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
    s
}

// ---------------------------------------------------------------------------------------------
// Whether the focused text can hold the keyboard (the text itself only: O(1), no world walk)
// ---------------------------------------------------------------------------------------------

/// Why text entity `e` can't hold the keyboard in a frame drawn with `view` (`None`: no window
/// frame yet, only the entity itself is judged): `hidden`, `transparent` (under
/// [`MIN_FOCUS_OPACITY`]), `off_screen` or `too_small`. What else the frame drew over it isn't
/// judged here (review R3: the banner is the human's guarantee, not a CPU occlusion test).
pub fn unseen(e: &Entity, view: Option<&(Mat4, (u32, u32))>) -> Option<&'static str> {
    if !crate::sight::in_frame(e, true) {
        return Some("hidden");
    }
    if e.look.opacity < MIN_FOCUS_OPACITY {
        return Some("transparent");
    }
    let (vp, (w, h)) = view?;
    if !crate::sight::in_view(e, vp) {
        return Some("off_screen");
    }
    // Its box on screen (corners behind the camera make it large: readable).
    let [x0, y0, x1, y1] = screen_box(e, vp, (*w, *h))?;
    let (vis_w, vis_h) = (x1.min(*w as f32) - x0.max(0.0), y1.min(*h as f32) - y0.max(0.0));
    // Lines shorter than a few pixels can't be read; an empty field is as wide as its caret.
    if vis_h < MIN_FOCUS_PX || vis_w < 1.0 {
        return Some("too_small");
    }
    None
}

/// Entity `e`'s box in window pixels `[x0, y0, x1, y1]` for `view_proj` at `size` (`None`: no
/// bounds, or a corner behind the camera).
pub fn screen_box(e: &Entity, view_proj: &Mat4, size: (u32, u32)) -> Option<[f32; 4]> {
    let (mut y0, mut y1, mut x0, mut x1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for c in e.world_corners()? {
        let p = view_proj.mul_vec4(c.extend(1.0));
        if p.w <= 1e-6 {
            return None;
        }
        let (sx, sy) = ((p.x / p.w * 0.5 + 0.5) * size.0 as f32, (p.y / p.w * 0.5 + 0.5) * size.1 as f32);
        (x0, x1, y0, y1) = (x0.min(sx), x1.max(sx), y0.min(sy), y1.max(sy));
    }
    Some([x0, y0, x1, y1])
}

/// Why the focus on `id` can't stay (`deleted`, `not_text`, or [`unseen`]'s reasons).
pub fn focus_problem(world: &World, id: u64, view: Option<&(Mat4, (u32, u32))>) -> Option<&'static str> {
    let Some(e) = world.get(id) else { return Some("deleted") };
    if !matches!(e.two_d, Some(crate::two_d::TwoD::Text(_))) {
        return Some("not_text");
    }
    unseen(e, view)
}

// ---------------------------------------------------------------------------------------------
// The cue
// ---------------------------------------------------------------------------------------------

/// The banner naming the focused text (the bundled UI face, its own small atlas).
#[derive(Default)]
struct FocusCue {
    atlas: Option<GlyphAtlas>,
    pages: Vec<TextureId>,
    scratch: Vec<u8>,
    /// The banner's quads as the last window frame drew them.
    quads: Vec<OverlayQuad>,
}

impl std::fmt::Debug for FocusCue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FocusCue").field("pages", &self.pages.len()).finish()
    }
}

/// The banner's words for `path`.
pub fn cue_text(path: &str) -> String {
    let shown: String = if path.chars().count() > CUE_PATH_CHARS {
        let tail: Vec<char> = path.chars().rev().take(CUE_PATH_CHARS - 1).collect();
        std::iter::once('…').chain(tail.into_iter().rev()).collect()
    } else {
        path.to_owned()
    };
    format!("Typing into {shown} \u{2014} Esc or any click stops")
}

const CUE_BG: [f32; 4] = [0.05, 0.06, 0.08, 1.0];
/// STYLE.md: cyan marks what is selected.
const CUE_EDGE: [f32; 4] = [0.247, 0.878, 0.816, 1.0];
const CUE_INK: [f32; 4] = [0.925, 0.902, 0.839, 1.0];

impl FocusCue {
    /// Draws the banner just above the text's box on screen (`near`), below it when there is no
    /// room above, or at the top of the window when its box isn't known; opaque, so a game's
    /// interface under it never mixes with its words.
    fn draw(&mut self, gpu: &mut specula_gpu::Gpu, size: (u32, u32), path: &str, near: Option<[f32; 4]>, out: &mut Vec<OverlayQuad>) -> Result<(), String> {
        let font = &faces().ui;
        let text = cue_text(path);
        let style = TextStyle::new(CUE_PX);
        let layout = specula_text::layout(font, &text, &style);
        let (tw, th) = (layout.width.ceil(), layout.height.ceil());
        let (bw, bh) = (tw + 20.0, th + 10.0);
        let (sw, sh) = (size.0 as f32, size.1 as f32);
        let (x, y) = match near {
            Some([x0, y0, x1, y1]) => {
                let y = if y0 - bh - 10.0 >= 8.0 { y0 - bh - 10.0 } else { y1 + 10.0 };
                ((x0 + x1) * 0.5 - bw * 0.5, y)
            }
            None => ((sw - bw) * 0.5, 8.0),
        };
        let rect = [x.clamp(0.0, (sw - bw).max(0.0)).round(), y.clamp(0.0, (sh - bh).max(0.0)).round(), bw, bh];
        let atlas = self.atlas.get_or_insert_with(|| GlyphAtlas::new(AtlasConfig { page_size: 256, max_pages: 2, padding: 1, evict: true }));
        atlas.begin_frame();
        let mut glyphs = Vec::new();
        layout.append_quads(font, atlas, [rect[0] + 10.0, rect[1] + 5.0], CUE_INK, &mut glyphs).map_err(|e| format!("focus cue glyphs: {e}"))?;
        let page = atlas.page_size();
        let fresh = self.pages.len();
        while self.pages.len() < atlas.page_count() {
            let pixels = atlas.page_pixels(self.pages.len()).unwrap_or(&[]);
            self.pages.push(gpu.upload_texture(page, page, TextureFormat::R8, pixels).map_err(|e| format!("focus cue atlas: {e}"))?);
        }
        for r in atlas.take_dirty() {
            let p = r.page as usize;
            if p >= fresh || r.width == 0 || r.height == 0 {
                continue;
            }
            atlas.copy_rect(&r, &mut self.scratch);
            gpu.update_texture(self.pages[p], r.x, r.y, r.width, r.height, &self.scratch).map_err(|e| format!("focus cue atlas: {e}"))?;
        }
        out.push(OverlayQuad { radius: 4.0, border: 2.0, border_color: [CUE_EDGE; 2], ..OverlayQuad::solid(rect, CUE_BG) });
        for g in &glyphs {
            let Some(&tex) = self.pages.get(g.page as usize) else { continue };
            out.push(OverlayQuad::textured([g.rect[0], g.rect[1], g.rect[2] - g.rect[0], g.rect[3] - g.rect[1]], g.uv, tex, OverlayKind::AlphaMask, g.color));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// The engine's side
// ---------------------------------------------------------------------------------------------

impl crate::Engine {
    /// The window's typing into the focused text entity, once per window frame: any click or
    /// tap ends it (the game gets the click: a click never points the keyboard), a text that
    /// can't hold it or that someone else wrote loses it, and this frame's keys, characters and
    /// IME events (with any carried from earlier frames) become one change by the window's owner
    /// once a window frame has shown the banner. Ctrl+Z and Ctrl+Y step through the window's own
    /// history of this text; copy, cut and paste use the system clipboard on its own thread.
    /// Escape ends the edit: the focus goes and the caret and selection are taken off the text.
    pub(crate) fn text_window_input(&mut self) {
        if self.text_typing.focus.is_some() && self.input.iter().any(|e| matches!(e, Event::MouseDown { .. } | Event::PointerDown { .. })) {
            self.text_typing.drop_focus("clicked_elsewhere");
        }
        if let Some(id) = self.text_typing.focus {
            if let Some(reason) = self.text_focus_problem(id, true) {
                self.text_typing.drop_focus(reason);
            }
        }
        let Some(id) = self.text_typing.focus else {
            self.text_typing.guard_keys(&mut self.input);
            return;
        };
        self.text_typing.guard = None;
        let (ops, escape) = self.text_typing.take_ops(&mut self.input);
        self.text_typing.queue(ops);
        if !self.text_typing.shown() {
            // No key lands before a window frame showed the human where it goes (it waits).
            if escape {
                self.text_typing.forget_pending("escape");
                self.text_window_apply(id, Vec::new(), true);
            }
            return;
        }
        self.text_window_clipboard_answer();
        if self.text_typing.clip.is_some() {
            // Keys wait behind the paste or cut in flight (Escape forgets them and ends the edit).
            if escape {
                self.text_typing.forget_pending("escape");
                self.text_window_apply(id, Vec::new(), true);
            }
            return;
        }
        let Some(crate::two_d::TwoD::Text(spec)) = self.world.get(id).and_then(|e| e.two_d.as_ref()) else { return };
        let spec: TextSpec = (**spec).clone();
        let (batch, barrier) = self.text_typing.next_batch(&spec, escape);
        if escape {
            // Escape after this frame's keys: what couldn't apply this frame is forgotten.
            self.text_typing.forget_pending("escape");
        }
        let after = self.text_window_apply(id, batch, escape);
        let Some(op) = barrier.filter(|_| !escape) else { return };
        if self.text_typing.stuck.is_some() {
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "text.clipboard_failed", op = if op == EditOp::Paste { "get" } else { "set" }, message = "the clipboard hasn't answered an earlier request");
            return;
        }
        let cut = op == EditOp::Cut;
        let req = if cut {
            let Some(selected) = after.as_ref().and_then(selection) else { return };
            ClipReq::Set(selected)
        } else {
            ClipReq::Get
        };
        let seen = self.text_typing.clip_seen(if cut { "cut" } else { "paste" });
        match clipboard(req, true, seen) {
            Some(Some(rx)) => self.text_typing.clip = Some(ClipWait { cut, started: Instant::now(), rx }),
            _ => specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "text.clipboard_failed", op = if cut { "set" } else { "get" }, message = "the clipboard thread couldn't start"),
        }
    }

    /// Why text `id` can't take (a `window_focus` request) or hold (`held`) the window's
    /// keyboard: [`focus_problem`]'s reasons, and for a held focus another writer's change to
    /// the text or an ancestor ([`WindowTyping::foreign_write`]). The text and its ancestors
    /// only: a few lookups, never a walk of the world (review R3, major 2).
    pub(crate) fn text_focus_problem(&self, id: u64, held: bool) -> Option<&'static str> {
        if let Some(why) = focus_problem(&self.world, id, self.text_typing.view.as_ref()) {
            return Some(why);
        }
        if held {
            return self.text_typing.foreign_write(&self.world);
        }
        None
    }

    /// An engine screen (the editor, the gallery or the review screen) took the window: the keys
    /// go to it, so the typing ends (the banner would name a text the keys don't reach). The
    /// editor opens only from the human's own keys or pad: their leaving, no key guard.
    pub(crate) fn text_window_modal(&mut self) {
        if self.text_typing.focus.is_some() {
            let agents_screen = self.review_ui.is_open() || self.taste.screen.is_open();
            self.text_typing.drop_focus(if agents_screen { "engine_screen" } else { "editor_opened" });
        }
    }

    /// Before the engine screens read a window frame's input: when a screen an agent can open
    /// (the gallery or the review screen) is on the window while the human types into a text,
    /// the typing ends there, and while the key guard holds, the keys the human goes on typing
    /// are withheld from those screens too (review R4 of m5-text: keys meant for the text picked
    /// a gallery variant). Clicks, taps and pads pass: they are the human's choice.
    pub(crate) fn text_window_screens(&mut self) {
        let agents_screen = self.review_ui.is_open() || self.taste.screen.is_open();
        let now = Instant::now();
        if agents_screen && self.text_typing.focus.is_some() {
            self.text_typing.drop_focus("engine_screen");
        }
        // The editor's "Ask the agents" box is the human typing too. It closes here, before
        // the screens read this frame, and the keys the human goes on typing are withheld the
        // same way. (Review R1 of m5-edswarm, B2: the box was closed inside the editor's own
        // frame, after the review screen had read the keys: a lead opened the review screen
        // while the human typed a request, the human pressed Enter to send it, and Enter
        // approved a submission as window:owner.)
        let typing = agents_screen && (self.text_typing.focus.is_some() || self.editor.ui.asking());
        if agents_screen && self.editor.ui.asking() {
            self.editor.ui.ask.close("engine_screen");
            self.editor.ui.set_status("The Ask box closed: another screen took the window. Nothing was sent.", false);
            self.text_typing.hold_keys(now, "ask_box_closed");
        }
        if typing {
            // The screen took the window from under the human's typing: whoever opened it, its
            // focus goes to Close. The hold on the keys only covers what they type without a
            // pause; this covers the Enter after one (review R2 of m5-edswarm, N1).
            self.review_ui.defuse();
            self.taste.screen.defuse();
        }
        if self.text_typing.focus.is_none() {
            self.text_typing.guard_keys_at(&mut self.input, now);
        }
    }

    /// A paste's or a cut's answer from the clipboard thread, if it came (or the wait ran out).
    fn text_window_clipboard_answer(&mut self) {
        if let Some(rx) = &self.text_typing.stuck {
            if !matches!(rx.try_recv(), Err(TryRecvError::Empty)) {
                self.text_typing.stuck = None;
            }
        }
        let Some(w) = &self.text_typing.clip else { return };
        let answer = match w.rx.try_recv() {
            Ok(a) => a,
            Err(TryRecvError::Empty) if w.started.elapsed() < CLIPBOARD_WAIT => return,
            Err(TryRecvError::Empty) => Err(format!("no answer within {} s", CLIPBOARD_WAIT.as_secs())),
            Err(TryRecvError::Disconnected) => Err("the clipboard thread stopped".into()),
        };
        let w = self.text_typing.clip.take().expect("waiting");
        let op = match (w.cut, answer) {
            // Copied: the selection goes now (as it was when Ctrl+X was pressed: the keys after
            // it waited).
            (true, Ok(_)) => Some(EditOp::Cut),
            (false, Ok(Some(text))) => Some(EditOp::Insert(fit(text))),
            (false, Ok(None)) => None,
            (cut, Err(m)) => {
                // The thread traced its own failures; a wait that ran out is traced here, and the
                // request is remembered until it answers.
                if w.started.elapsed() >= CLIPBOARD_WAIT {
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "text.clipboard_failed", op = if cut { "set" } else { "get" }, message = m.as_str());
                    self.text_typing.stuck = Some(w.rx);
                }
                None
            }
        };
        if let Some(op) = op {
            // A cut the clipboard holds now applies as an ordinary op (with a clipboard of its
            // own), not as another barrier.
            self.text_typing.cut_copied = op == EditOp::Cut;
            self.text_typing.queued.push_front(op);
        }
    }

    /// Applies `ops` (no paste or cut among them) to text `id` as one change by the window's
    /// owner; `ending`: the edit ends after them. Returns the text as it is after.
    fn text_window_apply(&mut self, id: u64, ops: Vec<EditOp>, ending: bool) -> Option<TextSpec> {
        let Some(crate::two_d::TwoD::Text(spec)) = self.world.get(id).and_then(|e| e.two_d.as_ref()) else {
            self.text_typing.drop_focus("not_text");
            return None;
        };
        let start: TextSpec = (**spec).clone();
        let version = self.world.get(id).map(|e| e.version);
        let typing = &mut self.text_typing;
        if ending {
            typing.focus = None;
            typing.watch.clear();
            typing.banner = None;
        }
        if ops.is_empty() && !ending {
            return Some(start);
        }
        if typing.history_of != Some(id) || typing.version != version {
            typing.undo.clear();
            typing.redo.clear();
            typing.history_of = Some(id);
        }
        let mut cur = start.clone();
        let mut copied: Option<String> = None;
        // Runs of ordinary ops apply as one edit (one layout, however many characters an IME
        // result brought); Ctrl+Z / Ctrl+Y step the window's history between them. The batch
        // is within the frame's layout budget ([`WindowTyping::next_batch`]).
        for run in ops.split_inclusive(|op| matches!(op, EditOp::Undo | EditOp::Redo)) {
            let (edits, step) = match run.last() {
                Some(EditOp::Undo | EditOp::Redo) => (&run[..run.len() - 1], run.last()),
                _ => (run, None),
            };
            if !edits.is_empty() {
                // A cut here is one the clipboard already holds; a copy is sent to it after.
                let mut cb = MemoryClipboard::default();
                match apply_edits_with(&cur, edits, &mut cb) {
                    Ok((next, report)) => {
                        if edits.contains(&EditOp::Copy) {
                            copied = report.copied.or(copied);
                        }
                        if next.text != cur.text {
                            if typing.undo.len() >= WINDOW_UNDO {
                                typing.undo.remove(0);
                            }
                            typing.undo.push(cur.clone());
                            typing.redo.clear();
                        }
                        cur = next;
                    }
                    Err(m) => specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "text.window_typing_dropped", id = id, ops = edits.len() as u64, reason = m.as_str()),
                }
            }
            match step {
                Some(EditOp::Undo) => {
                    if let Some(prev) = typing.undo.pop() {
                        typing.redo.push(std::mem::replace(&mut cur, prev));
                    }
                }
                Some(EditOp::Redo) => {
                    if let Some(next) = typing.redo.pop() {
                        typing.undo.push(std::mem::replace(&mut cur, next));
                    }
                }
                _ => {}
            }
        }
        if let Some(text) = copied {
            if typing.stuck.is_some() {
                specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "text.clipboard_failed", op = "set", message = "the clipboard hasn't answered an earlier request");
            } else if clipboard(ClipReq::Set(text), false, typing.clip_seen("copy")).is_none() {
                specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "text.clipboard_failed", op = "set", message = "the clipboard thread couldn't start");
            }
        }
        if ending {
            // The edit ends: its history goes with it.
            typing.undo.clear();
            typing.redo.clear();
            typing.history_of = None;
            typing.pointer = None;
            crate::inputguard::note(crate::inputguard::Channel::Keyboard, "focus_dropped", "escape");
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "text.window_focus_dropped", id = id, path = typing.path.as_str(), reason = "escape");
            if cur.edit.is_some() {
                cur.edit = None;
                if cur.refresh().is_err() {
                    return Some(start);
                }
            }
        }
        if cur == start {
            return Some(cur);
        }
        let owner = crate::authority::WINDOW_OWNER;
        let w = crate::journal::Writer::new(owner, Some(owner));
        let change = crate::world::UpdateSpec { two_d: Some(crate::two_d::TwoD::Text(Box::new(cur.clone()))), ..Default::default() };
        let tick = self.clock.tick();
        let done = self.world.commit(&mut self.swarm, tick, w, crate::journal::Kind::Set, crate::write_policy::Rule::World, |wd| {
            wd.update(&crate::world::EntityKey::Id(id), change, owner).map(|e| e.version)
        });
        match done {
            Ok((after, _)) => {
                self.text_typing.version = Some(after);
                // The window's own change: the watch moves on (from the version it recorded).
                self.text_typing.wrote(id, version.unwrap_or(0), after);
            }
            Err(e) => {
                self.text_typing.version = None;
                specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "text.window_typing_refused", id = id, code = e.code);
                return Some(start);
            }
        }
        Some(cur)
    }

    /// The banner naming the focused text, for a window frame: the last of its overlay, so it
    /// draws over everything (the world, 2D layers, glass, the game's interface and the engine's
    /// screens). Room for it comes first: when the overlay is full, what was drawn before it
    /// yields. A frame that can't draw it drops the focus (`no_banner`): the keyboard never types
    /// into a text the human wasn't shown.
    pub(crate) fn text_focus_overlay(&mut self, size: (u32, u32), out: &mut Vec<OverlayQuad>) {
        let Some(id) = self.text_typing.focus else { return };
        if size.0 == 0 || size.1 == 0 {
            return;
        }
        let typing = &mut self.text_typing;
        let near = match (self.world.get(id), &typing.view) {
            (Some(e), Some((vp, s))) if *s == size => screen_box(e, vp, size),
            _ => None,
        };
        let mut quads = std::mem::take(&mut typing.cue.quads);
        quads.clear();
        let drawn = typing.cue.draw(&mut self.gpu, size, &typing.path, near, &mut quads);
        match drawn {
            Ok(()) if !quads.is_empty() && quads.len() <= specula_gpu::MAX_OVERLAY_QUADS => {
                let keep = specula_gpu::MAX_OVERLAY_QUADS - quads.len();
                if out.len() > keep {
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "text.focus_cue_room", dropped = (out.len() - keep) as u64);
                    out.truncate(keep);
                }
                out.extend_from_slice(&quads);
                let frames = typing.banner.filter(|b| b.id == id).map_or(0, |b| b.frames);
                // `last` is measured on the overlay the frame sends ([`WindowTyping::overlay_sent`]).
                typing.banner = Some(BannerDrawn { id, quads: quads.len(), last: false, frames });
            }
            other => {
                let message = match other {
                    Err(e) => e,
                    Ok(()) => format!("{} quads", quads.len()),
                };
                specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "text.focus_cue_failed", message = message.as_str());
                typing.drop_focus("no_banner");
            }
        }
        typing.cue.quads = quads;
    }
}

impl WindowTyping {
    /// The overlay a window frame sends to the GPU: whether the banner is its last quads (drawn
    /// over everything), measured on what is sent. A frame whose overlay doesn't end with it
    /// doesn't count as showing it.
    pub fn overlay_sent(&mut self, overlay: &[OverlayQuad]) {
        let Some(b) = self.banner.as_mut() else { return };
        let q = &self.cue.quads;
        b.last = !q.is_empty() && overlay.len() >= q.len() && overlay[overlay.len() - q.len()..] == q[..];
        if b.last {
            b.frames += 1;
        }
    }

    /// The input monitor sees a clipboard access (`what`: paste, cut or copy), judged by who pointed
    /// the keyboard at the text (the human's key, in a text an agent may have chosen).
    fn clip_seen(&self, what: &str) -> crate::inputguard::Seen {
        let by = self.pointer.clone().unwrap_or(crate::inputguard::Actor::Human);
        crate::inputguard::done(crate::inputguard::Channel::Clipboard, what, &by, &self.path)
    }

    /// The banner the last window frame showed for the held focus (drawn last in its overlay).
    fn shown(&self) -> bool {
        self.banner().is_some_and(|b| b.last && b.frames > 0)
    }
}

/// The selected text of `spec` (`None`: nothing selected).
fn selection(spec: &TextSpec) -> Option<String> {
    let e = spec.edit.as_ref()?;
    let (a, b) = (e.caret.min(e.anchor) as usize, e.caret.max(e.anchor) as usize);
    spec.plain().get(a..b).filter(|s| !s.is_empty()).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula_core::math::Vec3;
    use specula_platform::MouseButton;

    #[test]
    fn window_keys_become_edits_and_leave_the_rest_to_the_game() {
        let mut t = WindowTyping { focus: Some(1), ..Default::default() };
        let mut ev = vec![
            Event::Char('H'),
            Event::KeyDown { code: VK_SHIFT, repeat: false },
            Event::KeyDown { code: VK_LEFT, repeat: false },
            Event::KeyUp { code: VK_SHIFT },
            Event::MouseMove { x: 1, y: 2 },
            Event::KeyDown { code: VK_CONTROL, repeat: false },
            Event::KeyDown { code: 0x43, repeat: false },
            Event::KeyUp { code: VK_CONTROL },
            Event::ImeComposition { text: "やま".into(), caret: 6 },
            Event::ImeCommit("山".into()),
            Event::Char('山'),
            Event::ImeEnd,
            // AltGr (Ctrl+Alt) + A on a Polish layout: a letter, never Select All.
            Event::KeyDown { code: VK_CONTROL, repeat: false },
            Event::KeyDown { code: VK_MENU, repeat: false },
            Event::KeyDown { code: 0x41, repeat: false },
            Event::Char('ą'),
            Event::KeyUp { code: VK_MENU },
            Event::KeyUp { code: VK_CONTROL },
            Event::KeyDown { code: VK_ESCAPE, repeat: false },
        ];
        let (ops, escape) = t.take_ops(&mut ev);
        assert!(escape);
        assert_eq!(ev, vec![Event::MouseMove { x: 1, y: 2 }], "only the mouse is left for the game");
        assert_eq!(
            ops,
            vec![
                EditOp::Insert("H".into()),
                EditOp::Left { unit: Unit::Char, select: true },
                EditOp::Copy,
                EditOp::Compose { text: "やま".into(), caret: 6 },
                EditOp::CancelComposition,
                EditOp::Insert("山".into()),
                EditOp::CancelComposition,
                EditOp::Insert("ą".into()),
            ]
        );
    }

    /// Keys after a focus dropped by itself are withheld while the human keeps typing; a pause
    /// longer than KEY_GUARD_GAP gives the game its keys back, and so does a click at once. A
    /// click elsewhere (the human's own leaving) sets no guard.
    #[test]
    fn keys_after_an_automatic_drop_are_withheld_until_a_pause_or_a_click() {
        let mut t = WindowTyping::default();
        t.focus_on(7, "/f".into(), crate::inputguard::Actor::Human, Vec::new(), crate::inputguard::seen_for_test());
        t.drop_focus("occluded");
        let mut keys = vec![Event::Char('a'), Event::KeyDown { code: 0x41, repeat: false }, Event::KeyUp { code: 0x41 }];
        t.guard_keys(&mut keys);
        assert_eq!(keys.len(), 1, "only the key-up passes");
        std::thread::sleep(KEY_GUARD_GAP + Duration::from_millis(50));
        let mut keys = vec![Event::Char('b')];
        t.guard_keys(&mut keys);
        assert_eq!(keys.len(), 1, "after a pause the game gets its keys");
        assert!(t.guard.is_none());
        t.focus_on(7, "/f".into(), crate::inputguard::Actor::Human, Vec::new(), crate::inputguard::seen_for_test());
        t.drop_focus("hidden");
        let mut keys = vec![Event::MouseDown { button: MouseButton::Left, x: 0, y: 0 }, Event::Char('c')];
        t.guard_keys(&mut keys);
        assert_eq!(keys.len(), 2, "a click ends it at once");
        t.focus_on(7, "/f".into(), crate::inputguard::Actor::Human, Vec::new(), crate::inputguard::seen_for_test());
        t.drop_focus("clicked_elsewhere");
        let mut keys = vec![Event::Char('d')];
        t.guard_keys(&mut keys);
        assert_eq!(keys.len(), 1, "the human's own leaving withholds nothing");
    }

    fn text(s: &str) -> TextSpec {
        TextSpec::parse(&specula_core::Json::parse(&format!("{{\"text\": {}}}", specula_core::Json::from(s))).unwrap()).unwrap()
    }

    /// Review of cd954bd, minor 6: an IME result longer than a frame's ops kept its tail for the
    /// next frame; nothing is dropped below the queue's cap, and past it the drop is counted.
    #[test]
    fn keys_past_a_frame_are_carried_to_the_next_never_dropped_silently() {
        let mut t = WindowTyping { focus: Some(1), ..Default::default() };
        let spec = text("");
        let mut ev: Vec<Event> = "あ".repeat(600).chars().map(Event::Char).collect();
        let (ops, _) = t.take_ops(&mut ev);
        assert_eq!(ops.len(), 600, "every key is returned");
        t.queue(ops);
        let mut seen = 0;
        for frame in 0..4 {
            let (batch, barrier) = t.next_batch(&spec, false);
            assert!(barrier.is_none());
            assert!(batch.len() <= MAX_EDIT_OPS, "frame {frame}");
            seen += batch.len();
        }
        assert_eq!(seen, 600, "the tail arrives in later frames");
        assert!(t.queued.is_empty());
        t.queue(vec![EditOp::Insert("x".into()); MAX_QUEUED_OPS + 10]);
        assert_eq!(t.queued.len(), MAX_QUEUED_OPS, "past the cap keys are dropped (and traced)");
    }

    /// A paste or a cut waits for the clipboard: the batch stops there and the keys after it
    /// stay queued; a frame's batch also stops at the layout budget (the rest carried).
    #[test]
    fn a_paste_is_a_barrier_and_the_layout_budget_carries_the_rest() {
        let mut t = WindowTyping { focus: Some(1), ..Default::default() };
        let spec = text("hello");
        t.queue(vec![EditOp::Insert("a".into()), EditOp::Paste, EditOp::Insert("b".into())]);
        let (batch, barrier) = t.next_batch(&spec, false);
        assert_eq!(batch, vec![EditOp::Insert("a".into())]);
        assert_eq!(barrier, Some(EditOp::Paste));
        assert_eq!(t.queued.iter().cloned().collect::<Vec<_>>(), vec![EditOp::Insert("b".into())]);
        // Line moves after changes of a long text: each a layout; the batch stops within budget.
        let long = text(&"word ".repeat(400));
        let mut t = WindowTyping { focus: Some(1), ..Default::default() };
        t.queue((0..20).flat_map(|_| [EditOp::Insert("x".into()), EditOp::Down { select: false }]).collect());
        let (batch, _) = t.next_batch(&long, false);
        assert!(window_cost(&long, &batch, false).chars <= WINDOW_LAYOUT_CHARS);
        assert!(!batch.is_empty() && !t.queued.is_empty(), "within budget now, the rest next frame");
    }

    #[test]
    fn the_banner_names_the_text_and_cuts_long_paths() {
        assert!(cue_text("/hud/name").contains("Typing into /hud/name") && cue_text("/a").ends_with("Esc or any click stops"));
        let long = format!("/{}", "a".repeat(300));
        let c = cue_text(&long);
        assert!(c.contains('…') && c.chars().count() < 140, "{c}");
        assert_eq!(fit("é".repeat(2000)).len(), MAX_TEXT_BYTES, "a paste is cut at a character boundary");
    }

    // -----------------------------------------------------------------------------------------
    // With an engine (a GPU; skipped cleanly without one): review of cd954bd, major 2.
    // -----------------------------------------------------------------------------------------

    use crate::authority::Authority;
    use crate::journal::{self, Writer};
    use crate::sight::Camera;
    use crate::world::{EntityKey, UpdateSpec};
    use crate::Engine;
    use specula_core::Json;

    const W: u32 = 640;
    const H: u32 = 360;

    fn engine(tag: &str) -> Option<Engine> {
        if !specula_gpu::is_available() {
            crate::skip_test(&format!("text_window::{tag}"), "Vulkan is not available");
            return None;
        }
        let gpu = match specula_gpu::Gpu::new(&specula_gpu::GpuOptions { validation: specula_gpu::Validation::Auto, ..Default::default() }) {
            Ok(g) => g,
            Err(e) => {
                crate::skip_test(&format!("text_window::{tag}"), &e.to_string());
                return None;
            }
        };
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        let captures = std::env::temp_dir().join(format!("specula-text-window-{tag}-{}", std::process::id()));
        Some(Engine::new(gpu, &captures, crate::models::Library::new(&assets, None)).expect("engine"))
    }

    fn spawn(e: &mut Engine, spec: &str) -> u64 {
        let spec = crate::tools::parse_spawn(&Json::parse(spec).unwrap()).unwrap();
        let w = Writer::new("t", Some("t"));
        e.world.commit(&mut e.swarm, 0, w, journal::Kind::Spawn, crate::write_policy::Rule::World, |wd| wd.spawn(spec, "t").map(|x| x.id)).unwrap().0
    }

    fn set(e: &mut Engine, id: u64, change: UpdateSpec) {
        let w = Writer::new("t", Some("t"));
        e.world.commit(&mut e.swarm, 0, w, journal::Kind::Set, crate::write_policy::Rule::World, |wd| wd.update(&EntityKey::Id(id), change, "t").map(|_| ())).unwrap();
    }

    fn focus(e: &mut Engine, id: u64, who: &str, have: Authority) -> Result<(), String> {
        let Some(crate::two_d::TwoD::Text(spec)) = e.world.get(id).and_then(|x| x.two_d.as_ref()) else { return Err("not_text".into()) };
        let (next, report) = crate::text_world::prepare_edit(spec, &[], false).map_err(|x| x.code.to_string())?;
        let base_version = e.world.get(id).map_or(0, |x| x.version);
        let commit = crate::text_world::TextCommit { id, next, report, base_version, expect_version: None, window_focus: Some(true) };
        let out = e.text_commit(commit, Writer::new(who, Some(who)), 0, who, crate::inputguard::Actor::agent(who, who, have));
        let text = out.content.iter().map(|c| match c { specula_protocol::Content::Text(t) => t.as_str(), _ => "" }).collect::<String>();
        let body = Json::parse(&text).unwrap_or(Json::Null);
        if out.is_error {
            Err(body.get("error").and_then(Json::as_str).unwrap_or("?").to_owned())
        } else {
            Ok(())
        }
    }

    fn plain(e: &Engine, id: u64) -> String {
        match e.world.get(id).and_then(|x| x.two_d.as_ref()) {
            Some(crate::two_d::TwoD::Text(t)) => t.plain().to_owned(),
            _ => String::new(),
        }
    }

    /// The window as it looks at the text at the origin.
    fn look_at_origin(e: &mut Engine) {
        let cam = Camera::new(Vec3::new(0.0, 0.0, 6.0), Vec3::ZERO, 50.0);
        e.text_typing.set_view(cam.view_proj(W, H), (W, H));
    }

    /// A window frame: the game's interface `hud` (none: an empty overlay), then the banner, sent
    /// as the frame's overlay. Returns the overlay.
    fn frame_with(e: &mut Engine, hud: Vec<OverlayQuad>) -> Vec<OverlayQuad> {
        let mut quads = hud;
        e.text_focus_overlay((W, H), &mut quads);
        e.text_typing.overlay_sent(&quads);
        quads
    }

    fn show(e: &mut Engine) {
        frame_with(e, Vec::new());
    }

    fn middle(e: &Engine) -> (i32, i32) {
        let (vp, (w, h)) = *e.text_typing.view().unwrap();
        let c = vp.mul_vec4(Vec3::ZERO.extend(1.0));
        (((c.x / c.w * 0.5 + 0.5) * w as f32) as i32, ((c.y / c.w * 0.5 + 0.5) * h as f32) as i32)
    }

    fn click_at(x: i32, y: i32) -> Event {
        Event::MouseDown { button: MouseButton::Left, x, y }
    }

    #[test]
    fn a_builder_cannot_take_the_humans_keyboard_and_it_drops_when_the_text_is_unseen() {
        let Some(mut e) = engine("focus") else { return };
        look_at_origin(&mut e);
        let id = spawn(&mut e, r#"{"name": "pw", "shape": "text", "text": {"text": "x", "size": 0.5}}"#);
        // mallory, a builder: refused before anything changes (the probe's hunter2 never lands).
        assert_eq!(focus(&mut e, id, "mallory", Authority::Agent), Err("lead_only".into()));
        assert_eq!(e.text_typing.focus, None);
        e.input = "hunter2".chars().map(Event::Char).collect();
        e.text_window_input();
        assert_eq!(plain(&e, id), "x", "no keys reach a text nobody focused");
        assert_eq!(e.input.len(), 7, "they stay the game's");
        // The lead may; typed keys arrive once a frame showed the banner naming the text.
        focus(&mut e, id, "lee", Authority::Lead).expect("the lead points the keyboard");
        assert_eq!(e.text_typing.focused_path(), Some("/pw"));
        e.input = vec![Event::Char('a')];
        e.text_window_input();
        assert_eq!((plain(&e, id).as_str(), e.input.len()), ("x", 0), "no key lands before the banner was shown (it waits; the game doesn't get it)");
        show(&mut e);
        e.text_window_input();
        assert_eq!(plain(&e, id), "xa", "shown: the key waiting lands");
        // Hidden: the focus drops before the frame's keys. What the human goes on typing was
        // meant for the text: it is withheld from the game (and its modules) while they type.
        set(&mut e, id, UpdateSpec { visible: Some(false), ..Default::default() });
        e.input = vec![Event::Char('b')];
        e.text_window_input();
        assert_eq!((e.text_typing.focus, plain(&e, id).as_str(), e.input.len()), (None, "xa", 0));
        // A click ends the guard: the game gets it, and keys again.
        e.input = vec![click_at(1, 1), Event::Char('c')];
        e.text_window_input();
        assert_eq!(e.input.len(), 2, "after the human's click the game gets everything");
        // A hidden text can't take it at all, even from the lead.
        assert_eq!(focus(&mut e, id, "lee", Authority::Lead), Err("not_visible".into()));
        set(&mut e, id, UpdateSpec { visible: Some(true), ..Default::default() });
        // Off screen (the window turned away), and deleted.
        focus(&mut e, id, "lee", Authority::Lead).unwrap();
        let away = Camera::new(Vec3::new(0.0, 0.0, 6.0), Vec3::new(0.0, 0.0, 12.0), 50.0);
        e.text_typing.set_view(away.view_proj(W, H), (W, H));
        e.text_window_input();
        assert_eq!(e.text_typing.focus, None, "off screen");
        look_at_origin(&mut e);
        focus(&mut e, id, "lee", Authority::Lead).unwrap();
        let w = Writer::new("t", Some("t"));
        e.world.commit(&mut e.swarm, 0, w, journal::Kind::Delete, crate::write_policy::Rule::World, |wd| wd.delete(&EntityKey::Id(id), None).map(|_| ())).unwrap();
        e.text_window_input();
        assert_eq!(e.text_typing.focus, None, "deleted");
        // Too small to read, and too faint (review R3: the floor is half opaque).
        let tiny = spawn(&mut e, r#"{"name": "tiny", "shape": "text", "text": {"text": "x", "size": 0.005}}"#);
        assert_eq!(focus(&mut e, tiny, "lee", Authority::Lead), Err("not_visible".into()));
        let faint = spawn(&mut e, r#"{"name": "faint", "shape": "text", "opacity": 0.4, "text": {"text": "x", "size": 0.5}}"#);
        assert_eq!(focus(&mut e, faint, "lee", Authority::Lead), Err("not_visible".into()));
        assert_eq!(e.text_focus_problem(faint, false), Some("transparent"));
    }

    /// Review R3 (lead direction 1): a click or a tap never points the keyboard at a world text,
    /// not even one being edited that the lead pointed it at before; any click anywhere ends the
    /// typing, and the click is the game's.
    #[test]
    fn a_click_never_takes_the_keyboard_and_any_click_ends_the_typing() {
        let Some(mut e) = engine("click") else { return };
        look_at_origin(&mut e);
        let field = spawn(&mut e, r#"{"name": "field", "shape": "text", "text": {"text": "name", "size": 0.5, "anchor": [0.5, 0.5], "editing": {"caret": 4}}}"#);
        let (cx, cy) = middle(&e);
        e.input = vec![click_at(cx, cy), Event::Char('!')];
        e.text_window_input();
        assert_eq!((e.text_typing.focus, e.input.len(), plain(&e, field).as_str()), (None, 2, "name"), "a click on a field being edited is the game's");
        // The lead points the keyboard there; a click on the field itself ends it.
        focus(&mut e, field, "lee", Authority::Lead).unwrap();
        show(&mut e);
        e.input = vec![click_at(cx, cy), Event::Char('?')];
        e.text_window_input();
        assert_eq!((e.text_typing.focus, e.input.len(), plain(&e, field).as_str()), (None, 2, "name"), "the click and the key after it are the game's");
        // And back on it: still the game's (only the operator points the keyboard again).
        e.input = vec![click_at(cx, cy), Event::Char('?')];
        e.text_window_input();
        assert_eq!((e.text_typing.focus, e.input.len()), (None, 2));
        // A tap (F6: phones), a right click: the same.
        focus(&mut e, field, "lee", Authority::Lead).unwrap();
        e.input = vec![Event::PointerDown { id: 1, kind: specula_platform::PointerKind::Touch, x: cx, y: cy }];
        e.text_window_input();
        assert_eq!((e.text_typing.focus, e.input.len()), (None, 1));
        focus(&mut e, field, "lee", Authority::Lead).unwrap();
        e.input = vec![Event::MouseDown { button: MouseButton::Right, x: 3, y: 3 }];
        e.text_window_input();
        assert_eq!(e.text_typing.focus, None);
        // The human's own leaving withholds nothing.
        e.input = vec![Event::Char('k')];
        e.text_window_input();
        assert_eq!(e.input.len(), 1);
    }

    /// Review R2 of m5-text, major 1 (the reviewer's probe_click2): mallory, a builder, hides an
    /// editable text behind her cube and points the window there; the human's click on the cube
    /// and the keys after it reach the game.
    #[test]
    fn probe_click2_a_builders_text_behind_a_cube_never_takes_the_humans_click() {
        let Some(mut e) = engine("behind") else { return };
        look_at_origin(&mut e);
        spawn(&mut e, r#"{"name": "wall", "shape": "cube", "position": [0, 0, 0], "scale": [6, 4, 0.2]}"#);
        let sink = spawn(&mut e, r#"{"name": "sink", "shape": "text", "position": [0, 0, -1], "text": {"text": "MMMMMMMMMMMM", "size": 0.4, "max_width": 3, "anchor": [0.5, 0.5], "editing": {"caret": 0}}}"#);
        show(&mut e);
        let (cx, cy) = middle(&e);
        let mut input: Vec<Event> = vec![click_at(cx, cy)];
        input.extend("hunter2".chars().map(Event::Char));
        e.input = input;
        e.text_window_input();
        assert_eq!(e.text_typing.focus, None, "focus stays empty");
        assert_eq!(e.input.len(), 8, "the game gets the click and the keys");
        assert_eq!(plain(&e, sink), "MMMMMMMMMMMM");
        // mallory can't point it there either (probe_focus).
        assert_eq!(focus(&mut e, sink, "mallory", Authority::Agent), Err("lead_only".into()));
    }

    /// The reviewer's probe_focus (review of cd954bd, major 2): a builder points the keyboard at
    /// her hidden, tiny text far below; refused, and no key typed reaches it.
    #[test]
    fn probe_focus_a_builders_hidden_text_never_takes_the_keyboard() {
        let Some(mut e) = engine("probe-focus") else { return };
        look_at_origin(&mut e);
        let sink = spawn(&mut e, r#"{"name": "sink", "shape": "text", "visible": false, "position": [0, -500, 0], "text": {"text": "", "size": 0.01}}"#);
        assert_eq!(focus(&mut e, sink, "mallory", Authority::Agent), Err("lead_only".into()));
        show(&mut e);
        e.input = "hunter2 my password".chars().map(Event::Char).collect();
        e.text_window_input();
        assert_eq!((plain(&e, sink).as_str(), e.input.len()), ("", 19));
        assert_eq!(focus(&mut e, sink, "lee", Authority::Lead), Err("not_visible".into()), "hidden: not even the lead");
    }

    /// The reviewer's probe_cover (review R3, major 1): the lead pointed the keyboard at /field
    /// and the human clicked away; mallory then paints over the field from *behind* it (glass,
    /// a 2D shape and a text on a higher z layer: the renderer draws them over a text, which
    /// writes no depth) and the human clicks the field and types. No click points the keyboard,
    /// so nothing she placed catches a key, however the frame layered it.
    #[test]
    fn probe_cover_nothing_behind_a_field_catches_the_humans_click() {
        let Some(mut e) = engine("cover") else { return };
        look_at_origin(&mut e);
        let field = spawn(&mut e, r##"{"name": "field", "shape": "text", "color": "#ffffff", "text": {"text": "name", "size": 0.5, "anchor": [0.5, 0.5], "editing": {"caret": 4}}}"##);
        focus(&mut e, field, "lee", Authority::Lead).unwrap();
        show(&mut e);
        e.input = vec![click_at(5, 5)];
        e.text_window_input();
        assert_eq!(e.text_typing.focus, None, "the human clicked away");
        let (cx, cy) = middle(&e);
        let covers = [
            ("glass_behind", r##"{"name": "gb", "shape": "cube", "position": [0, 0, -0.3], "scale": [5, 3, 0.1], "material": "glass", "opacity": 1.0, "color": "#707070"}"##),
            ("shape_layer_behind", r##"{"name": "sb", "shape": "shape2d", "position": [0, 0, -0.3], "shape2d": {"items": [{"rect": [-2.5, -1.5, 5, 3], "fill": "#707070"}], "z_layer": 5}}"##),
            ("text_layer_behind", r##"{"name": "tb", "shape": "text", "position": [0, 0, -0.3], "color": "#707070", "text": {"text": "██████", "size": 1.5, "anchor": [0.5, 0.5], "z_layer": 5}}"##),
        ];
        for (tag, spec) in covers {
            let cover = spawn(&mut e, spec);
            show(&mut e);
            let mut input = vec![click_at(cx, cy)];
            input.extend("AAA".chars().map(Event::Char));
            e.input = input;
            e.text_window_input();
            assert_eq!((e.text_typing.focus, e.input.len(), plain(&e, field).as_str()), (None, 4, "name"), "{tag}: the game gets the click and the keys");
            let w = Writer::new("t", Some("t"));
            e.world.commit(&mut e.swarm, 0, w, journal::Kind::Delete, crate::write_policy::Rule::World, |wd| wd.delete(&EntityKey::Id(cover), None).map(|_| ())).unwrap();
        }
    }

    /// The reviewer's probe_cover case D (review R3, major 1): mallory rewrites the field the
    /// lead pointed the keyboard at (huge, nearly transparent, dark, over her button). Any write
    /// by anyone but the operator or the window's typing drops the focus at the next frame,
    /// before its keys, and the keys the human goes on typing are withheld from the game. The
    /// same for a write to an ancestor; the operator's own text_edit keeps it.
    #[test]
    fn probe_cover_a_builder_rewriting_the_focused_field_drops_the_focus() {
        let Some(mut e) = engine("rewrite") else { return };
        look_at_origin(&mut e);
        let parent = spawn(&mut e, r#"{"name": "sign", "shape": "group"}"#);
        let field = spawn(&mut e, &format!(r#"{{"name": "field", "shape": "text", "parent": {parent}, "text": {{"text": "name", "size": 0.5, "anchor": [0.5, 0.5], "editing": {{"caret": 4}}}}}}"#));
        let button = spawn(&mut e, r#"{"name": "button", "shape": "cube", "position": [0, 0, -1], "scale": [3, 2, 0.2]}"#);
        let _ = button;
        focus(&mut e, field, "lee", Authority::Lead).unwrap();
        show(&mut e);
        e.input = vec![Event::Char('a')];
        e.text_window_input();
        assert_eq!(plain(&e, field), "namea");
        // The operator's own text_edit keeps the focus (and the window's typing did too).
        let Some(crate::two_d::TwoD::Text(spec)) = e.world.get(field).and_then(|x| x.two_d.as_ref()) else { panic!("text") };
        let (next, report) = crate::text_world::prepare_edit(spec, &[EditOp::Insert("b".into())], false).unwrap();
        let base_version = e.world.get(field).unwrap().version;
        let c = crate::text_world::TextCommit { id: field, next, report, base_version, expect_version: None, window_focus: None };
        assert!(!e.text_commit(c, Writer::new("lee", Some("lee")), 0, "lee", crate::inputguard::Actor::agent("lee", "lee", Authority::Lead)).is_error);
        e.input = vec![Event::Char('c')];
        e.text_window_input();
        assert_eq!((e.text_typing.focus, plain(&e, field).as_str()), (Some(field), "nameabc"));
        // mallory's rewrite: scale 12, opacity 0.05... (any write at all does it).
        set(&mut e, field, UpdateSpec { scale: Some(Vec3::new(12.0, 12.0, 1.0)), ..Default::default() });
        e.input = "DDD".chars().map(Event::Char).collect();
        e.text_window_input();
        assert_eq!((e.text_typing.focus, plain(&e, field).as_str(), e.input.len()), (None, "nameabc", 0), "dropped before the keys; they are withheld from the game");
        // A builder's text_edit of the focused field (no window_focus) drops it too.
        set(&mut e, field, UpdateSpec { scale: Some(Vec3::ONE), ..Default::default() });
        focus(&mut e, field, "lee", Authority::Lead).unwrap();
        show(&mut e);
        let Some(crate::two_d::TwoD::Text(spec)) = e.world.get(field).and_then(|x| x.two_d.as_ref()) else { panic!("text") };
        let (next, report) = crate::text_world::prepare_edit(spec, &[EditOp::Insert("z".into())], false).unwrap();
        let base_version = e.world.get(field).unwrap().version;
        let c = crate::text_world::TextCommit { id: field, next, report, base_version, expect_version: None, window_focus: None };
        assert!(!e.text_commit(c, Writer::new("mallory", Some("mallory")), 0, "mallory", crate::inputguard::Actor::agent("mallory", "mallory", Authority::Agent)).is_error);
        e.text_window_input();
        assert_eq!(e.text_typing.focus, None, "a builder's text_edit");
        // A write to an ancestor (she moves the sign the field hangs on).
        focus(&mut e, field, "lee", Authority::Lead).unwrap();
        show(&mut e);
        set(&mut e, parent, UpdateSpec { position: Some(Vec3::new(0.0, 0.1, 0.0)), ..Default::default() });
        e.text_window_input();
        assert_eq!(e.text_typing.focus, None, "an ancestor's write");
        // An unrelated write (her button) keeps it.
        focus(&mut e, field, "lee", Authority::Lead).unwrap();
        show(&mut e);
        set(&mut e, button, UpdateSpec { position: Some(Vec3::new(0.0, 0.0, -2.0)), ..Default::default() });
        e.text_window_input();
        assert_eq!(e.text_typing.focus, Some(field), "another entity's write");
        // A foreign write before the operator's own edit still drops it (the watch moves on only
        // from the version it recorded).
        set(&mut e, field, UpdateSpec { position: Some(Vec3::new(0.0, 0.0, 0.01)), ..Default::default() });
        let Some(crate::two_d::TwoD::Text(spec)) = e.world.get(field).and_then(|x| x.two_d.as_ref()) else { panic!("text") };
        let (next, report) = crate::text_world::prepare_edit(spec, &[EditOp::Insert("y".into())], false).unwrap();
        let base_version = e.world.get(field).unwrap().version;
        let c = crate::text_world::TextCommit { id: field, next, report, base_version, expect_version: None, window_focus: None };
        assert!(!e.text_commit(c, Writer::new("lee", Some("lee")), 0, "lee", crate::inputguard::Actor::agent("lee", "lee", Authority::Lead)).is_error);
        e.text_window_input();
        assert_eq!(e.text_typing.focus, None, "the builder's write before the lead's");
        // An engine screen taking the window ends it too.
        focus(&mut e, field, "lee", Authority::Lead).unwrap();
        e.text_window_modal();
        assert_eq!(e.text_typing.focus, None, "an engine screen");
    }

    /// Review R4 (the reviewer's probe_screen2): a gallery an agent put on the window mid-typing
    /// ends the typing before the screen reads the frame, and the keys the human goes on typing
    /// reach neither the screen nor the game; a click is theirs. Opening the editor withholds
    /// nothing (the human's own leaving).
    #[test]
    fn keys_meant_for_a_text_never_reach_a_screen_an_agent_opened() {
        let Some(mut e) = engine("screen") else { return };
        look_at_origin(&mut e);
        let field = spawn(&mut e, r#"{"name": "field", "shape": "text", "text": {"text": "", "size": 0.5, "anchor": [0.5, 0.5], "editing": {"caret": 0}}}"#);
        focus(&mut e, field, "lee", Authority::Lead).unwrap();
        show(&mut e);
        e.taste.screen.open("anns", "owner", crate::inputguard::seen_for_test());
        e.input = vec![Event::KeyDown { code: 0x32, repeat: false }, Event::Char('2'), Event::KeyDown { code: VK_RETURN, repeat: false }];
        e.text_window_screens();
        assert_eq!((e.text_typing.focus, e.input.len()), (None, 0), "the screen gets none of the keys");
        e.input = vec![Event::KeyDown { code: VK_RETURN, repeat: false }];
        e.text_window_screens();
        assert!(e.input.is_empty(), "still typing: still withheld");
        e.input = vec![click_at(3, 3), Event::KeyDown { code: VK_RETURN, repeat: false }];
        e.text_window_screens();
        assert_eq!(e.input.len(), 2, "the human's click ends the guard");
        e.taste.screen.close();
        // The editor: the human's own leaving, no guard.
        focus(&mut e, field, "lee", Authority::Lead).unwrap();
        e.text_window_modal();
        assert!(e.text_typing.focus.is_none() && e.text_typing.guard.is_none());
    }

    /// Review R3, lead direction 4: the banner is the last of the window frame's overlay (drawn
    /// over everything: the world, 2D layers, glass, the game's interface, the engine's screens),
    /// even when the overlay is full; it is in every frame while the focus holds; engine_info
    /// reports it from the overlay the frame sent.
    #[test]
    fn the_banner_is_the_last_of_every_frames_overlay_while_the_focus_holds() {
        let Some(mut e) = engine("banner") else { return };
        look_at_origin(&mut e);
        let field = spawn(&mut e, r#"{"name": "field", "shape": "text", "text": {"text": "name", "size": 0.5, "anchor": [0.5, 0.5], "editing": {"caret": 4}}}"#);
        assert!(frame_with(&mut e, Vec::new()).is_empty(), "no focus: no banner");
        focus(&mut e, field, "lee", Authority::Lead).unwrap();
        assert_eq!(e.text_typing.json().get("banner"), Some(&Json::Null), "not shown yet");
        // A game whose interface fills the whole overlay with opaque panels over the window.
        let panel = OverlayQuad::solid([0.0, 0.0, W as f32, H as f32], [0.1, 0.1, 0.1, 1.0]);
        for n in [0, 3, specula_gpu::MAX_OVERLAY_QUADS] {
            let quads = frame_with(&mut e, vec![panel; n]);
            let b = e.text_typing.banner().expect("drawn");
            assert!(quads.len() <= specula_gpu::MAX_OVERLAY_QUADS && b.quads > 1 && b.last, "{n}: {b:?}");
            let box_ = quads[quads.len() - b.quads];
            assert_eq!((box_.color[3], box_.border_color[0]), (1.0, CUE_EDGE), "{n}: the banner's opaque box leads its quads");
            assert!(quads[quads.len() - b.quads + 1..].iter().all(|q| q.texture.is_some()), "{n}: then its words");
        }
        assert_eq!(e.text_typing.banner().map(|b| b.frames), Some(3));
        let j = e.text_typing.json();
        assert_eq!((j.get("path").and_then(Json::as_str), j.get("banner").and_then(|b| b.get("last")).and_then(Json::as_bool)), (Some("/field"), Some(true)));
        // An overlay whose tail isn't the banner (something drawn after it) doesn't count.
        let mut quads = frame_with(&mut e, Vec::new());
        quads.push(panel);
        e.text_typing.overlay_sent(&quads);
        assert!(!e.text_typing.banner().unwrap().last);
        e.input = vec![Event::Char('x')];
        e.text_window_input();
        assert_eq!(plain(&e, field), "name", "no key lands while the last frame didn't show the banner last");
        show(&mut e);
        e.text_window_input();
        assert_eq!(plain(&e, field), "namex");
    }

    /// Review R3, major 2 (the reviewer's probe_focuscost and probe_cost100k): pointing the
    /// keyboard and holding it at 100,000 entities with the world changing between frames costs
    /// the text and its ancestors only, never a walk of the world (it was 8-11 ms per request and
    /// 1.8 ms per held re-judgement in release).
    #[test]
    fn window_focus_and_held_focus_cost_no_walk_of_a_100k_world() {
        let Some(mut e) = engine("cost100k") else { return };
        look_at_origin(&mut e);
        let cube = Json::parse(r#"{"shape": "cube", "position": [0, 0, 60]}"#).unwrap();
        let w = Writer::new("t", Some("t"));
        e.world
            .commit(&mut e.swarm, 0, w, journal::Kind::Spawn, crate::write_policy::Rule::World, |wd| {
                for _ in 0..100_000 {
                    wd.spawn(crate::tools::parse_spawn(&cube).expect("spec"), "t")?;
                }
                Ok(())
            })
            .unwrap();
        let field = spawn(&mut e, r#"{"name": "field", "shape": "text", "text": {"text": "name", "size": 0.5, "anchor": [0.5, 0.5], "editing": {"caret": 4}}}"#);
        assert!(e.world.len() > 100_000);
        let mut checks = Vec::new();
        for _ in 0..20 {
            let t = Instant::now();
            assert_eq!(e.text_focus_problem(field, false), None);
            assert_eq!(e.text_focus_problem(field, true), None);
            checks.push(t.elapsed());
        }
        focus(&mut e, field, "lee", Authority::Lead).unwrap();
        show(&mut e);
        let mut held = Vec::new();
        for i in 0..20 {
            // The world churns between frames (a builder's spawn), as in probe_cost100k.
            let far = crate::tools::parse_spawn(&Json::parse(&format!(r#"{{"shape": "cube", "position": [0, 0, {}]}}"#, 100 + i)).unwrap()).unwrap();
            let w = Writer::new("churn", Some("churn"));
            e.world.commit(&mut e.swarm, 0, w, journal::Kind::Spawn, crate::write_policy::Rule::World, |wd| wd.spawn(far, "churn").map(|_| ())).unwrap();
            e.input = vec![Event::Char('a')];
            let t = Instant::now();
            e.text_window_input();
            held.push(t.elapsed());
        }
        assert_eq!(e.text_typing.focus, Some(field));
        checks.sort();
        held.sort();
        // A walk of 100k entities takes several milliseconds even in release (about 70 ns each);
        // the text's own checks take microseconds, even in a debug build.
        assert!(checks[10] < specula_core::speed_budget(std::time::Duration::from_micros(500)), "focus checks at 100k: median {:?}", checks[10]);
        assert!(held[10] < specula_core::speed_budget(std::time::Duration::from_millis(4)), "a held frame's typing at 100k: median {:?}", held[10]);
    }

    /// Review of cd954bd, minor 5: a paste waits for the clipboard thread's answer (never the
    /// engine thread), the keys after it wait with it, and an answer that never comes is given
    /// up on; a cut deletes only once the clipboard holds the text.
    #[test]
    fn a_paste_and_a_cut_wait_for_the_clipboard_thread_and_keep_the_keys_in_order() {
        let Some(mut e) = engine("clipboard") else { return };
        look_at_origin(&mut e);
        let id = spawn(&mut e, r#"{"name": "f", "shape": "text", "text": {"text": "ab", "size": 0.5, "editing": {"caret": 2}}}"#);
        focus(&mut e, id, "lee", Authority::Lead).unwrap();
        show(&mut e);
        // Ctrl+V then "c": the paste waits on a (stand-in) clipboard answer; "c" waits behind it.
        e.text_typing.queue(vec![EditOp::Insert("c".into())]);
        let (reply, rx) = crate::jobs::channel::<ClipAnswer>();
        e.text_typing.clip = Some(ClipWait { cut: false, started: Instant::now(), rx });
        e.text_window_input();
        assert_eq!(plain(&e, id), "ab", "nothing applies while the paste waits");
        reply.send(Ok(Some("XY".into())));
        e.text_window_input();
        assert_eq!(plain(&e, id), "abXYc", "the paste, then the key typed after it");
        // A cut: the selection goes only when the clipboard took it; a failed copy keeps it.
        e.text_typing.queue(vec![EditOp::SelectAll]);
        e.text_window_input();
        let (reply, rx) = crate::jobs::channel::<ClipAnswer>();
        e.text_typing.clip = Some(ClipWait { cut: true, started: Instant::now(), rx });
        reply.send(Err("another program holds the clipboard".into()));
        e.text_window_input();
        assert_eq!(plain(&e, id), "abXYc", "a cut whose copy failed deletes nothing");
        let (reply, rx) = crate::jobs::channel::<ClipAnswer>();
        e.text_typing.clip = Some(ClipWait { cut: true, started: Instant::now(), rx });
        reply.send(Ok(None));
        e.text_window_input();
        assert_eq!(plain(&e, id), "", "copied: the selection is cut");
        // No answer within the wait: given up on, remembered, and typing goes on.
        let (_reply, rx) = crate::jobs::channel::<ClipAnswer>();
        e.text_typing.clip = Some(ClipWait { cut: false, started: Instant::now() - CLIPBOARD_WAIT, rx });
        e.input = vec![Event::Char('z')];
        e.text_window_input();
        assert!(e.text_typing.clip.is_none() && e.text_typing.stuck.is_some());
        e.text_window_input();
        assert_eq!(plain(&e, id), "z", "typing goes on");
    }
}
