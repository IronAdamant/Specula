//! The window's review screen: the owner pages through pending submissions and approves or
//! rejects them (docs/design/REVIEW.md, "The human view").
//!
//! A `specula-ui` screen (`crates/specula/ui/review.json`, data like any game's screens) drawn
//! over the game on the overlay, plus a "Review (n)" tab while work is pending. Mouse, touch,
//! keys (F9 toggles, arrows and Enter, PageUp / PageDown) and gamepads (LB + RB + Y toggles,
//! focus navigation, LB / RB page) all work (F5, F6). While it is open the hosted game gets no
//! window input. The shown submission's first capture is decoded and scaled on a worker thread,
//! then drawn inside the screen's `rv_image` panel; one texture is held at a time.
//!
//! What a press asks for comes back as [`Act`]s for the engine to apply (decisions need the
//! world and the board); this module only holds presentation state.

use crate::review::{Review, Status, Submission};
use specula_core::Json;
use specula_gpu::{OverlayKind, OverlayQuad, OverlayTextures, TextureFormat, TextureId};
use specula_input::{ActionMap, Input};
use specula_platform::gamepad::GamepadState;
use specula_platform::Event;
use specula_ui::gpu::GpuOverlay;
use specula_ui::{FontSet, StateMap, Theme, Ui, Viewport};
use std::sync::mpsc::{self, Receiver, TryRecvError};

const THEME: &str = include_str!("../ui/review_theme.json");
const SCREEN: &str = include_str!("../ui/review.json");
const TAB: &str = include_str!("../ui/review_tab.json");
/// The longest side a capture is shown at (scaled down on the worker: VRAM and upload time on
/// an iGPU).
const MAX_IMAGE_SIDE: u32 = 1024;
/// Largest capture file decoded.
const MAX_IMAGE_FILE: u64 = 64 << 20;
/// Largest capture decoded, in pixels (checked in the PNG header first): 4096 x 4096.
const MAX_IMAGE_PIXELS: u64 = 1 << 24;
/// Seconds a newly shown submission must be on screen before a press decides it.
const SETTLE_S: f64 = 0.4;
/// Seconds after the human closes the screen during which agents can't reopen it.
const HUMAN_CLOSE_HOLD: std::time::Duration = std::time::Duration::from_secs(30);

/// The screen's own actions (beside the UI's focus, accept and back).
const ACTIONS: &str = r#"{
  "format": "specula.input.v1",
  "actions": [
    { "name": "review_toggle", "kind": "button", "group": "review", "description": "Open or close the review screen.", "bindings": ["Key.F9", "Pad.LB+Pad.RB+Pad.North"] },
    { "name": "review_next", "kind": "button", "group": "review", "description": "The next pending submission.", "bindings": ["Key.PageDown", "Pad.RB"] },
    { "name": "review_prev", "kind": "button", "group": "review", "description": "The previous pending submission.", "bindings": ["Key.PageUp", "Pad.LB"] }
  ]
}"#;

/// What a press on the screen asks the engine to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Approve(u64),
    Reject(u64, String),
    /// The batch's pending submissions as the button counted them (one that joined since isn't in it).
    ApproveBatch(Vec<u64>),
}

/// Which capture: the submission and its file (a resubmission may reuse a path whose file changed).
type ImageKey = (u64, String);

/// A capture on the GPU (or why it isn't).
struct Image {
    key: ImageKey,
    texture: Option<TextureId>,
    size: (u32, u32),
    problem: Option<String>,
}

/// A decode on the worker thread (at most one at a time).
type Loader = (ImageKey, Receiver<Result<specula_core::png::Image, String>>);

pub struct ReviewUi {
    open: bool,
    /// The submission shown (a pending one, while any is).
    shown: Option<u64>,
    /// When (input clock, seconds) `shown` last changed: a decision press shortly after is
    /// refused, so a double press or a change under the pointer never decides unseen work.
    shown_at: f64,
    rejecting: bool,
    ui: Option<Ui>,
    overlay: GpuOverlay,
    /// Why the screen can't draw (no font), if so.
    problem: Option<String>,
    loaded: bool,
    input: Option<Input>,
    /// Captures lay the UI out with no input.
    idle: Option<Input>,
    state: StateMap,
    /// The capture shown, and the one wanted next.
    image: Option<Image>,
    want: Option<ImageKey>,
    loader: Option<Loader>,
    /// The last action's outcome, and whether it went well.
    status: (String, bool),
    /// The size the UI was last laid out for by a window frame.
    laid_out: Option<(u32, u32)>,
    started: std::time::Instant,
    /// When the human last closed the screen (agents may not reopen it at once: F5).
    human_closed: Option<std::time::Instant>,
    /// Set when the queue changed: the state is filled again.
    dirty: bool,
    /// The widget to focus at the end of the next window frame.
    focus: Option<&'static str>,
    /// The shown submission's batch as "Approve batch (n)" counted it, and when that changed.
    batch_ids: Vec<u64>,
    batch_at: f64,
    /// Textures to free (on the next overlay call, which has the GPU).
    retired: Vec<TextureId>,
    /// Where a review's files may be: the engine's captures and the queue's evidence (a
    /// review.json may name any path; M4 audit). `None` decodes nothing.
    roots: Option<crate::safe_path::ReviewRoots>,
}

impl Default for ReviewUi {
    fn default() -> Self {
        ReviewUi {
            open: false,
            shown: None,
            shown_at: f64::NEG_INFINITY,
            rejecting: false,
            ui: None,
            overlay: GpuOverlay::new(),
            problem: None,
            loaded: false,
            input: None,
            idle: None,
            state: StateMap::new(),
            image: None,
            want: None,
            loader: None,
            status: (String::new(), true),
            laid_out: None,
            started: std::time::Instant::now(),
            human_closed: None,
            dirty: true,
            focus: None,
            batch_ids: Vec::new(),
            batch_at: f64::NEG_INFINITY,
            retired: Vec::new(),
            roots: None,
        }
    }
}

impl ReviewUi {
    /// The screen for an engine whose captures are under `root`.
    pub fn with_captures(root: &std::path::Path) -> ReviewUi {
        let roots = crate::safe_path::ReviewRoots { captures: root.to_path_buf(), run: root.to_path_buf(), ..Default::default() };
        ReviewUi { roots: Some(roots), ..ReviewUi::default() }
    }

    /// Where the current queue's files are (its evidence folder moves with the world).
    pub fn set_roots(&mut self, roots: crate::safe_path::ReviewRoots) {
        if self.roots.as_ref() != Some(&roots) {
            self.roots = Some(roots);
            self.image = None;
            self.dirty = true;
        }
    }
}

fn actions() -> Result<ActionMap, String> {
    let mut map = ActionMap::from_json_str(ACTIONS).map_err(|e| format!("review actions: {e}"))?;
    specula_ui::add_ui_actions(&mut map).map_err(|e| format!("ui actions: {e}"))?;
    Ok(map)
}

/// The UI with the engine's review theme and screens (a system font, else an error).
fn load(fonts: FontSet) -> Result<Ui, String> {
    let theme = Theme::from_json_str(THEME).map_err(|e| format!("review_theme.json: {e}"))?;
    let mut ui = Ui::new(theme, fonts);
    ui.load_screen(SCREEN).map_err(|e| format!("review.json: {e}"))?;
    ui.load_screen(TAB).map_err(|e| format!("review_tab.json: {e}"))?;
    ui.show("review_tab").map_err(|e| e.to_string())?;
    Ok(ui)
}

fn system_fonts() -> Result<FontSet, String> {
    let (path, font) = specula_text::find_system_ui_font().ok_or("no system UI font found")?;
    let mut fonts = FontSet::new().with("ui", font);
    if let Some(dir) = path.parent() {
        if let Some(f) = ["segoeuib.ttf", "arialbd.ttf", "DejaVuSans-Bold.ttf"].iter().find_map(|f| specula_text::load_font_file(dir.join(f)).ok()) {
            fonts.add("ui_bold", f);
        }
    }
    Ok(fonts)
}

fn viewport(ui: &Ui, size: (u32, u32)) -> Viewport {
    let size = [size.0.max(1), size.1.max(1)];
    Viewport::new(size, ui.theme().scale.scale(size, None, 1.0))
}

/// Decodes and scales a capture (on a worker thread): only a capture or the queue's evidence
/// ([`crate::safe_path::review_file`]).
fn decode(path: &str, roots: Option<&crate::safe_path::ReviewRoots>) -> Result<specula_core::png::Image, String> {
    let roots = roots.ok_or_else(|| format!("{path}: no captures folder to show captures from"))?;
    let file = crate::safe_path::review_file(roots, path, "the capture").map_err(|r| r.message)?;
    let path = file.display().to_string();
    let path = path.as_str();
    let meta = std::fs::metadata(path).map_err(|e| format!("{path}: {e}"))?;
    if meta.len() > MAX_IMAGE_FILE {
        return Err(format!("{path} is {} MB; captures up to {} MB are shown", meta.len() >> 20, MAX_IMAGE_FILE >> 20));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    // The header's size first: a huge image is refused before anything is decoded.
    if bytes.len() >= 24 && &bytes[12..16] == b"IHDR" {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        if u64::from(w) * u64::from(h) > MAX_IMAGE_PIXELS {
            return Err(format!("{path} is {w}x{h}; captures up to {MAX_IMAGE_PIXELS} pixels are shown"));
        }
    }
    let img = specula_core::png::decode(&bytes).map_err(|e| format!("{path}: {e}"))?;
    Ok(shrink(img, MAX_IMAGE_SIDE))
}

/// A box-filtered copy at most `side` px on its longest side (the image itself when smaller).
pub fn shrink(img: specula_core::png::Image, side: u32) -> specula_core::png::Image {
    let (w, h) = (img.width, img.height);
    let k = w.max(h).div_ceil(side.max(1)).max(1);
    if k == 1 {
        return img;
    }
    let (nw, nh) = (w.div_ceil(k), h.div_ceil(k));
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for sy in (y * k)..((y + 1) * k).min(h) {
                for sx in (x * k)..((x + 1) * k).min(w) {
                    let i = ((sy * w + sx) * 4) as usize;
                    for (a, v) in acc.iter_mut().zip(&img.rgba[i..i + 4]) {
                        *a += u32::from(*v);
                    }
                    n += 1;
                }
            }
            let o = ((y * nw + x) * 4) as usize;
            for (dst, a) in out[o..o + 4].iter_mut().zip(acc) {
                *dst = (a / n.max(1)) as u8;
            }
        }
    }
    specula_core::png::Image { width: nw, height: nh, rgba: out }
}

impl ReviewUi {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The submission shown, if any.
    #[cfg(test)]
    pub fn shown(&self) -> Option<u64> {
        self.shown
    }

    /// Loads the UI on first use (fonts, theme, screens). `false` when it can't be drawn.
    fn ensure(&mut self) -> bool {
        if !self.loaded {
            self.loaded = true;
            match system_fonts().and_then(load) {
                Ok(ui) => self.ui = Some(ui),
                Err(e) => {
                    eprintln!("specula: no review screen: {e}");
                    self.problem = Some(e);
                }
            }
            match actions() {
                Ok(map) => {
                    self.input = Some(Input::new(map.clone()));
                    self.idle = Some(Input::new(map));
                }
                Err(e) => self.problem = Some(e),
            }
        }
        self.ui.is_some() && self.input.is_some()
    }

    /// The queue changed (a submission or decision): the screen shows it at the next frame.
    pub fn changed(&mut self, review: &Review) {
        self.dirty = true;
        self.settle(review);
    }

    /// Keeps `shown` on a pending submission: the same one, else the next after it, else the
    /// last pending one.
    fn settle(&mut self, review: &Review) {
        let pending: Vec<u64> = review.pending().map(|s| s.id).collect();
        let shown = match self.shown {
            Some(id) if pending.contains(&id) => Some(id),
            Some(id) => pending.iter().copied().find(|p| *p > id).or_else(|| pending.last().copied()),
            None => pending.first().copied(),
        };
        self.show_id(shown);
        if self.shown.is_none() {
            self.rejecting = false;
        }
    }

    /// Shows `id`, noting when the shown submission changed.
    fn show_id(&mut self, id: Option<u64>) {
        if id != self.shown {
            self.shown = id;
            self.shown_at = self.started.elapsed().as_secs_f64();
            self.rejecting = false;
            self.dirty = true;
        }
    }

    /// Whether the human closed the screen moments ago (agents then can't reopen it).
    pub fn closed_by_human_recently(&self) -> bool {
        self.human_closed.is_some_and(|t| t.elapsed() < HUMAN_CLOSE_HOLD)
    }

    pub fn open(&mut self, id: Option<u64>, review: &Review) {
        self.open = true;
        self.rejecting = false;
        if let Some(id) = id {
            self.show_id(Some(id));
        }
        if id.is_none_or(|id| review.get(id).is_some_and(|s| s.status == Status::Pending)) {
            self.settle(review);
        }
        self.dirty = true;
        self.focus = Some("rv_approve");
        // Opening is when it is first seen: decisions wait the settle time from now.
        self.shown_at = self.started.elapsed().as_secs_f64();
        if let Some(ui) = self.ui.as_mut() {
            let _ = ui.show("review");
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.rejecting = false;
        if let Some(ui) = self.ui.as_mut() {
            let _ = ui.hide("review");
        }
    }

    /// Pages through pending submissions (`by` = +1 next, -1 previous).
    pub fn step(&mut self, by: i32, review: &Review) {
        let pending: Vec<u64> = review.pending().map(|s| s.id).collect();
        if pending.is_empty() {
            self.shown = None;
            return;
        }
        let at = self.shown.and_then(|id| pending.iter().position(|p| *p == id));
        let next = match (at, by > 0) {
            (Some(i), true) => (i + 1).min(pending.len() - 1),
            (Some(i), false) => i.saturating_sub(1),
            (None, _) => 0,
        };
        self.show_id(Some(pending[next]));
        self.rejecting = false;
        self.dirty = true;
    }

    /// What the screen shows, as data (`review_screen`, `review_list`).
    pub fn status_json(&self, review: &Review) -> Json {
        let pending: Vec<u64> = review.pending().map(|s| s.id).collect();
        let mut f = vec![
            ("open", Json::Bool(self.open)),
            ("shown", self.shown.map_or(Json::Null, Json::from)),
            ("pending", Json::from(pending.len())),
            ("rejecting", Json::Bool(self.rejecting)),
        ];
        if let Some(i) = self.shown.and_then(|id| pending.iter().position(|p| *p == id)) {
            f.push(("position", Json::from(format!("{} of {}", i + 1, pending.len()))));
        }
        if !self.status.0.is_empty() {
            f.push(("last_action", Json::from(self.status.0.as_str())));
        }
        if let Some(p) = &self.problem {
            f.push(("problem", Json::from(p.as_str())));
        }
        if let Some(img) = &self.image {
            f.push(("capture", Json::from(img.key.1.as_str())));
            f.push(("capture_shown", Json::Bool(img.texture.is_some() && Some(img.key.0) == self.shown)));
            if let Some(p) = &img.problem {
                f.push(("capture_problem", Json::from(p.as_str())));
            }
        }
        Json::obj(f)
    }

    /// The outcome of an [`Act`], shown in the status line.
    pub fn report(&mut self, result: &Json) {
        let decided = result.get("decided").and_then(Json::as_u64).unwrap_or(0);
        let items = result.get("results").and_then(Json::as_array).unwrap_or_default();
        let first_error = items.iter().find_map(|r| r.get("message").and_then(Json::as_str));
        self.status = match first_error {
            Some(m) => (format!("Not done: {m}"), false),
            None => {
                let moved: usize = items.iter().filter_map(|r| r.get("promoted").and_then(|p| p.get("moved")).and_then(Json::as_array)).map(<[Json]>::len).sum();
                let verdict = items.first().and_then(|r| r.get("status")).and_then(Json::as_str).unwrap_or("decided");
                let task = items.first().and_then(|r| r.get("rework_task")).and_then(Json::as_u64);
                let mut s = format!("{} {decided} submission{}", capital(verdict), if decided == 1 { "" } else { "s" });
                if moved > 0 {
                    s.push_str(&format!("; moved {moved} into the game"));
                }
                if let Some(t) = task {
                    s.push_str(&format!("; rework task {t} for the agent"));
                }
                (s, true)
            }
        };
        self.dirty = true;
    }

    /// Fills the screen's state from the shown submission.
    fn fill(&mut self, review: &Review) {
        let pending: Vec<&Submission> = review.pending().collect();
        let st = &mut self.state;
        st.reset();
        st.set_int("pending", pending.len() as i64);
        st.set_bool("rejecting", self.rejecting);
        st.set_str("status_line", &self.status.0);
        st.set_bool("status_good", self.status.1);
        let at = self.shown.and_then(|id| pending.iter().position(|s| s.id == id));
        let Some(i) = at else {
            st.set_bool("has_item", false);
            st.set_str("position", if pending.is_empty() { "nothing pending" } else { "" });
            return;
        };
        let s = pending[i];
        st.set_bool("has_item", true);
        st.set_bool("has_prev", i > 0);
        st.set_bool("has_next", i + 1 < pending.len());
        st.set_str("position", &format!("{} of {} pending", i + 1, pending.len()));
        st.set_str("title", &format!("#{} {}", s.id, s.title));
        let mut meta = format!("{} by {}", s.kind, s.by.name);
        if let Some(b) = &s.batch {
            meta.push_str(&format!(" \u{b7} batch {b}"));
        }
        if let Some(t) = s.task {
            meta.push_str(&format!(" \u{b7} task {t}"));
        }
        let ents: usize = s.roots.iter().map(|r| r.entities).sum();
        if ents > 0 {
            meta.push_str(&format!(" \u{b7} {ents} entities"));
        }
        if !s.journal.is_empty() {
            meta.push_str(&format!(" \u{b7} {} journal entries", s.journal.len()));
        }
        st.set_str("meta", &meta);
        let failed = s.failed();
        // Engine results and the submitter's reported ones apart (never "all N passed"), and in
        // different colours: the engine's green (amber when one failed), the reported neutral.
        let ((_, ef), (_, rf)) = s.check_counts();
        let (engine, reported) = s.checks_parts();
        st.set_str("checks_line", &s.checks_line());
        st.set_str("checks_engine", &engine);
        st.set_bool("engine_failed", ef > 0);
        st.set_str("checks_reported", &reported);
        st.set_bool("reported_failed", rf > 0);
        for (k, c) in failed.iter().take(4).enumerate() {
            let mut line = format!("{} {}:{}", c.severity, c.kind, c.name);
            if let (Some(m), Some(v)) = (&c.metric, c.value) {
                line.push_str(&format!(" ({m} {v}{})", c.limit.map(|l| format!(", limit {l}")).unwrap_or_default()));
            }
            if !c.detail.is_empty() {
                line.push_str(&format!(" \u{2014} {}", c.detail));
            }
            st.set_str(&format!("fail_{}", k + 1), &line);
        }
        let open: Vec<_> = review.critiques_on(s.id).into_iter().filter(|c| c.status.open()).collect();
        if !open.is_empty() {
            st.set_str("critiques_line", &format!("{} open critique{}: {}", open.len(), if open.len() == 1 { "" } else { "s" }, open[0].text));
        }
        st.set_str("summary", &s.summary);
        if let Some(p) = &s.promote {
            st.set_str("promote_line", &format!("Approving moves it under {}", p.to));
        }
        let batch: Vec<u64> = s.batch.as_ref().map_or_else(Vec::new, |b| pending.iter().filter(|x| x.batch.as_ref() == Some(b)).map(|x| x.id).collect());
        st.set_int("batch_pending", batch.len() as i64);
        if batch != self.batch_ids {
            self.batch_ids = batch;
            self.batch_at = self.started.elapsed().as_secs_f64();
        }
        let note = match (&self.image, s.captures.first()) {
            (_, None) => "No capture was submitted".to_owned(),
            (Some(img), Some(c)) if img.key.1 == c.path && img.key.0 == s.id => match (&img.problem, img.texture) {
                (Some(p), _) => format!("Capture not shown: {p}"),
                (None, Some(_)) => String::new(),
                (None, None) => "Loading the capture\u{2026}".to_owned(),
            },
            _ => "Loading the capture\u{2026}".to_owned(),
        };
        st.set_str("image_note", &note);
    }

    /// The capture the shown submission wants (its first), noted for the loader.
    fn want_image(&mut self, review: &Review) {
        self.want = self.shown.and_then(|id| review.get(id)).and_then(|s| s.captures.first()).map(|c| (s_id(self.shown), c.path.clone()));
        self.start_load();
    }

    /// Starts decoding the wanted capture when it isn't shown and no decode is running (one
    /// thread at most: paging fast only replaces what is wanted next).
    fn start_load(&mut self) {
        if self.loader.is_some() {
            return;
        }
        let Some(key) = self.want.clone() else { return };
        if self.image.as_ref().is_some_and(|i| i.key == key) {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let path = key.1.clone();
        let roots = self.roots.clone();
        match std::thread::Builder::new().name("specula-review-image".into()).spawn(move || {
            let _ = tx.send(decode(&path, roots.as_ref()));
        }) {
            Ok(_) => self.loader = Some((key, rx)),
            Err(e) => {
                let old = self.image.replace(Image { key, texture: None, size: (0, 0), problem: Some(format!("could not start the image loader: {e}")) });
                self.retired.extend(old.and_then(|i| i.texture));
            }
        }
        self.dirty = true;
    }

    /// Takes a finished decode: uploaded when it's still what is wanted, dropped otherwise (the
    /// next wanted one starts).
    fn poll_load<G: OverlayTextures + ?Sized>(&mut self, gpu: &mut G) {
        let Some((key, rx)) = &self.loader else { return };
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("the image loader stopped".into()),
        };
        let key = key.clone();
        self.loader = None;
        if self.want.as_ref() == Some(&key) {
            let mut img = Image { key, texture: None, size: (0, 0), problem: None };
            match result {
                Ok(pixels) => match gpu.upload_texture(pixels.width, pixels.height, TextureFormat::Rgba8Srgb, &pixels.rgba) {
                    Ok(t) => {
                        img.texture = Some(t);
                        img.size = (pixels.width, pixels.height);
                    }
                    Err(e) => img.problem = Some(format!("upload: {e}")),
                },
                Err(e) => img.problem = Some(e),
            }
            let old = self.image.replace(img);
            self.retired.extend(old.and_then(|i| i.texture));
            self.dirty = true;
        }
        self.start_load();
    }

    /// Whether a decision press on the shown submission counts: it has been on screen a moment,
    /// and its capture (if any) is drawn or failed to load. Else the status line says why not.
    fn may_decide(&mut self, review: &Review) -> bool {
        let now = self.started.elapsed().as_secs_f64();
        let Some(s) = self.shown.and_then(|id| review.get(id)) else { return false };
        let seen = match (s.captures.first(), &self.image) {
            (None, _) => true,
            (Some(c), Some(img)) => img.key == (s.id, c.path.clone()) && (img.texture.is_some() || img.problem.is_some()),
            (Some(_), None) => false,
        };
        if now - self.shown_at < SETTLE_S || !seen {
            self.status = ("Not decided: look at it first (it has only just appeared)".into(), false);
            self.dirty = true;
            return false;
        }
        true
    }

    /// One window frame: reads input (toggle, paging, the screen's buttons) and returns what the
    /// presses ask for, whether the screen took this frame's input (open now or when the frame
    /// began: modal), and whether it took the pointer (the tab).
    pub fn frame(&mut self, events: &[Event], pads: &[GamepadState], size: (u32, u32), review: &Review) -> (Vec<Act>, bool, bool) {
        let mut acts = Vec::new();
        let was_open = self.open;
        let pending = review.pending_count();
        // Nothing to show and closed: don't even load fonts.
        if !self.open && pending == 0 && !self.loaded {
            return (acts, false, false);
        }
        if !self.ensure() || size.0 == 0 || size.1 == 0 {
            return (acts, was_open, false);
        }
        let now = self.started.elapsed().as_secs_f64();
        let input = self.input.as_mut().expect("ensured");
        input.update(now, events, pads);
        if input.pressed("review_toggle") {
            if self.open {
                self.close_by_human();
            } else {
                self.open(None, review);
            }
        }
        if !self.open && pending == 0 {
            if self.laid_out.is_some() {
                // The tab goes with the last pending submission: lay out once more without it.
                self.dirty = true;
                self.laid_out = None;
            }
            return (acts, was_open, false);
        }
        let (next, prev) = {
            let i = self.input.as_ref().expect("ensured");
            (i.pressed("review_next"), i.pressed("review_prev"))
        };
        if self.open && next {
            self.step(1, review);
        }
        if self.open && prev {
            self.step(-1, review);
        }
        if self.dirty {
            self.settle(review);
            if self.open {
                self.want_image(review);
            }
            self.fill(review);
            self.dirty = false;
        }
        let ui = self.ui.as_mut().expect("ensured");
        let input = self.input.as_ref().expect("ensured");
        let view = viewport(ui, size);
        let events = ui.frame(input, &self.state, &view).events.clone();
        self.laid_out = Some(size);
        let mut close = false;
        for a in events.actions() {
            match a {
                "review_open" => {
                    self.open(None, review);
                }
                "review_close" => close = true,
                "review_next" => self.step(1, review),
                "review_prev" => self.step(-1, review),
                "review_approve" => {
                    if self.may_decide(review) {
                        acts.extend(self.shown.map(Act::Approve));
                    }
                }
                "review_reject" => {
                    self.rejecting = true;
                    self.focus = Some("rv_r_style");
                    self.dirty = true;
                }
                "reject_cancel" => {
                    self.rejecting = false;
                    self.focus = Some("rv_reject");
                    self.dirty = true;
                }
                "review_batch" => {
                    // The batch as counted on the button, unchanged for a moment (one that just
                    // joined it is never approved unseen).
                    let settled = self.started.elapsed().as_secs_f64() - self.batch_at >= SETTLE_S;
                    if self.may_decide(review) && settled && !self.batch_ids.is_empty() {
                        acts.push(Act::ApproveBatch(self.batch_ids.clone()));
                    } else if !settled {
                        self.status = ("Not decided: the batch just changed; look again".into(), false);
                        self.dirty = true;
                    }
                }
                other => {
                    if let (Some(reason), Some(id)) = (other.strip_prefix("reject:"), self.shown) {
                        if self.may_decide(review) {
                            acts.push(Act::Reject(id, reason.to_owned()));
                        }
                        self.rejecting = false;
                        self.dirty = true;
                    }
                }
            }
        }
        if !acts.is_empty() {
            // After a decision the next submission appears: focus moves off the decision buttons,
            // so a second press can't decide what was never looked at.
            self.focus = Some("rv_next");
        }
        if close || (self.open && events.events.iter().any(|e| matches!(e, specula_ui::UiEvent::Back { .. }))) {
            self.close_by_human();
        }
        if let Some(ui) = self.ui.as_mut() {
            let want = self.open;
            if want != ui.is_visible("review") {
                let _ = if want { ui.show("review") } else { ui.hide("review") };
                self.dirty = true;
            }
            if let Some(id) = self.focus.take().filter(|_| self.open) {
                let refused = ui.widget("review", id).is_none_or(|w| ui.focus(w).is_err());
                if refused {
                    if let Some(w) = ui.widget("review", "rv_close") {
                        let _ = ui.focus(w);
                    }
                }
            }
        }
        (acts, was_open || self.open, events.consumed.pointer)
    }

    /// The human closed it (keys, pad, pointer).
    fn close_by_human(&mut self) {
        self.close();
        self.human_closed = Some(std::time::Instant::now());
    }

    /// The overlay for one image (`capture`: a sight capture, laid out here with no input) into
    /// `out`, appended after the game's, within `room` quads.
    pub fn overlay<G: OverlayTextures + ?Sized>(&mut self, gpu: &mut G, size: (u32, u32), capture: bool, review: &Review, room: usize, out: &mut Vec<OverlayQuad>) {
        for t in self.retired.drain(..) {
            gpu.destroy_texture(t);
        }
        // A finished decode goes up now (or the next wanted one starts).
        self.poll_load(gpu);
        if size.0 == 0 || size.1 == 0 || (!self.open && review.pending_count() == 0) {
            return;
        }
        if capture && !self.open && !self.loaded {
            return;
        }
        if !self.ensure() {
            return;
        }
        if capture || self.laid_out != Some(size) || self.dirty {
            // Captures (and a window size the last frame didn't lay out) lay out here, with no input.
            self.settle(review);
            if self.open {
                self.want_image(review);
            }
            self.fill(review);
            self.dirty = false;
            let ui = self.ui.as_mut().expect("ensured");
            let want = self.open;
            if want != ui.is_visible("review") {
                let _ = if want { ui.show("review") } else { ui.hide("review") };
            }
            let view = viewport(ui, size);
            ui.frame(self.idle.as_ref().expect("ensured"), &self.state, &view);
            self.laid_out = if capture { None } else { Some(size) };
        }
        let ui = self.ui.as_mut().expect("ensured");
        if let Err(e) = self.overlay.sync_atlas(gpu, ui.atlas_mut()) {
            self.problem = Some(format!("glyph atlas: {e}"));
            return;
        }
        let image_rect = ui.widget("review", "rv_image").and_then(|w| ui.rect(w)).filter(|_| self.open);
        let quads = match self.overlay.build(ui) {
            Ok(q) => q,
            Err(e) => {
                self.problem = Some(format!("overlay: {e}"));
                return;
            }
        };
        let room = room.saturating_sub(1);
        out.extend_from_slice(&quads[..quads.len().min(room)]);
        // Only the shown submission's own capture is drawn (never the last one's while it loads).
        let img = self.image.as_ref().filter(|i| Some(i.key.0) == self.shown && self.want.as_ref() == Some(&i.key));
        if let (Some(r), Some(img)) = (image_rect, img) {
            if let (Some(t), (w, h)) = (img.texture, img.size) {
                if w > 0 && h > 0 && out.len() < room + 1 {
                    // Fitted inside the panel's padding, centred.
                    let pad = 6.0;
                    let (bw, bh) = ((r.w - 2.0 * pad).max(1.0), (r.h - 2.0 * pad).max(1.0));
                    let s = (bw / w as f32).min(bh / h as f32);
                    let (dw, dh) = (w as f32 * s, h as f32 * s);
                    let rect = [r.x + pad + (bw - dw) / 2.0, r.y + pad + (bh - dh) / 2.0, dw, dh];
                    out.push(OverlayQuad::textured(rect, [0.0, 0.0, 1.0, 1.0], t, OverlayKind::Rgba, [1.0; 4]));
                }
            }
        }
    }
}

fn s_id(shown: Option<u64>) -> u64 {
    shown.unwrap_or(0)
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::Writer;
    use crate::review::{Capture, SubmitSpec};

    fn review_with(n: usize) -> Review {
        let mut r = Review::default();
        for i in 0..n {
            let spec = SubmitSpec {
                kind: "asset".into(),
                title: format!("prop {i}"),
                batch: Some("crypt".into()),
                journal: vec![1],
                captures: vec![Capture { path: format!("missing_{i}.png"), kind: "frame".into(), packet: None, caption: String::new() }],
                ..Default::default()
            };
            r.submit(Writer { name: "ann", session: Some("k") }, 0, spec).unwrap();
        }
        r
    }

    fn test_ui() -> ReviewUi {
        let mut s = ReviewUi::default();
        let fonts = FontSet::new().with("ui", specula_ui::testfont::box_font_parsed());
        s.ui = Some(load(fonts).expect("the built-in screens load"));
        let map = actions().unwrap();
        s.input = Some(Input::new(map.clone()));
        s.idle = Some(Input::new(map));
        s.loaded = true;
        s
    }

    #[test]
    fn the_screens_and_actions_load() {
        let s = test_ui();
        assert!(s.ui.as_ref().unwrap().widget("review", "rv_image").is_some());
        assert!(s.ui.as_ref().unwrap().widget("review_tab", "rv_tab").is_some());
    }

    #[test]
    fn paging_follows_the_pending_submissions() {
        let mut r = review_with(3);
        let mut s = test_ui();
        s.open(None, &r);
        assert_eq!(s.shown(), Some(1));
        s.step(1, &r);
        s.step(1, &r);
        s.step(1, &r);
        assert_eq!(s.shown(), Some(3), "stops at the last");
        s.step(-1, &r);
        assert_eq!(s.shown(), Some(2));
        // Deciding the shown one moves to the next.
        let d = crate::review::Decision { verdict: Status::Approved, by: "bob".into(), note: String::new(), tick: 1, promoted: None, task: None, reverted: None };
        r.decide(2, d).unwrap();
        s.changed(&r);
        assert_eq!(s.shown(), Some(3));
        s.fill(&r);
        assert_eq!(s.state.get("position"), Some(&specula_ui::Value::Str("2 of 2 pending".into())));
        assert_eq!(s.state.get("batch_pending"), Some(&specula_ui::Value::Int(2)));
        let j = s.status_json(&r);
        assert_eq!(j.get("shown"), Some(&Json::from(3)));
        // Frames: a click-free frame lays out and asks for nothing.
        let (acts, open, _) = s.frame(&[], &[], (1280, 800), &r);
        assert!(acts.is_empty() && open);
        // A decision press right after the shown submission changed is refused.
        s.step(-1, &r);
        assert!(!s.may_decide(&r), "it has only just appeared (and its capture isn't loaded)");
        s.close();
        assert!(!s.is_open());
    }

    #[test]
    fn images_shrink_by_box_filter() {
        let img = specula_core::png::Image { width: 4, height: 2, rgba: [[0u8, 0, 0, 255], [255, 255, 255, 255]].repeat(4).concat() };
        let s = shrink(img, 2);
        assert_eq!((s.width, s.height), (2, 1));
        assert_eq!(&s.rgba[..4], &[127, 127, 127, 255]);
        let same = shrink(specula_core::png::Image { width: 2, height: 2, rgba: vec![9; 16] }, 8);
        assert_eq!(same.width, 2);
    }
}
