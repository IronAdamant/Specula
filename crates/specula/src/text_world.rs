//! In-world text: the `text` component (shape `text`), drawn from glyph outlines per pixel (Slug,
//! `specula_gpu` `Draw2d::Text`), and `text_edit`, typing into it with a caret and selection.
//!
//! A text entity lives in its local XY plane like the other 2D shapes (metres, +Y up, facing
//! +Z): its box is laid out from the bundled faces (`specula_text::Bundled`: `ui`, `ui-bold`,
//! `display`) with the system's CJK faces as fallbacks, and placed with its `anchor` (0..1 each
//! way, (0.5, 0.5) = centred) at the entity's origin. The entity's colour and opacity tint it;
//! `lit` text takes the sun, ambient and point lights like a lit sprite (carved inscriptions);
//! unlit text shows its colour (labels). Glyphs are sharp at any distance or angle: no atlas.
//!
//! The layout is worked out once per change, where the change is made (the agent's connection
//! thread for `world_set`, `world_spawn` and `world_batch`), at a fixed reference size and
//! scaled, so a text's lines and bounds don't depend on the camera. The component keeps it:
//! frames, culling and sight draw and measure that one layout (never laid out again on the
//! engine thread, except once when a CJK fallback face it was waiting for arrives). Sight reports text entities
//! like any 2D subject; their draws go through the frame record (one `Draw2d::Text` per entity,
//! plus the caret and selection while editing).
//!
//! Design: `docs/design/TEXT.md`.

use crate::two_d::{boolean, choice, int, keys, nums, num, opt};
use specula_core::math::{Mat4, Vec3};
use specula_core::Json;
use specula_gpu::{Draw2d, GlyphInstance, Sprite, SpriteBlend, TextFrame, TextRun};
use specula_text::{
    edit::{Composition, EditOp, MemoryClipboard, TextEdit, Unit},
    layout_rich_into, rich, Align, Bundled, Font, FontSet, SlugStore, Span, TextLayout, TextStyle,
};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

/// Longest text (bytes of markup or plain text).
pub const MAX_TEXT_BYTES: usize = 2048;
/// Most lines a text may ask for (`max_lines`).
pub const MAX_TEXT_LINES: u32 = 64;
/// Most operations one `text_edit` call applies.
pub const MAX_EDIT_OPS: usize = 256;
/// Characters one `text_edit` call (or one frame of the window's typing) may lay out, summed over
/// its layouts: the cost model ([`edit_cost`]) is worked out from the ops before any is applied,
/// and a call over it is refused with a `limit` error. About 4 layouts of the longest text: a
/// 2048-byte layout takes about 0.5 ms in release for Latin, 0.1 to 0.25 ms for CJK (measured
/// with Yu Gothic and Microsoft YaHei; ink boxes come from glyph headers and cost next to
/// nothing). `text_edit` lays out on the caller's thread; the window's typing on the engine
/// thread takes a smaller share a frame (`text_window::WINDOW_LAYOUT_CHARS`).
pub const EDIT_LAYOUT_CHARS: usize = 4 * MAX_TEXT_BYTES;
/// Longest IME composition kept in a text's editing state (bytes).
pub const MAX_COMPOSING_BYTES: usize = 1024;
/// Words of new glyphs a window frame prepares (the rest follow in later frames): a page of CJK
/// text appearing never holds one frame for tens of milliseconds. Preparing costs about 0.2 µs
/// a word in release (a CJK glyph is about 500 words, a Latin one 150 to 250), so about 2 ms.
pub const WINDOW_PREP_WORDS: usize = 8 * 1024;
/// Words of new glyphs a capture prepares when it renders, after preparing ahead while parked
/// ([`CAPTURE_PREP_WORDS_PER_PASS`]): what is left past it is drawn by a later capture and
/// reported in the packet as deferred (about 6 ms).
pub const CAPTURE_PREP_WORDS: usize = 32 * 1024;
/// Words of the glyph store a parked capture uploads to the GPU per pass (1 MiB words, 4 MiB:
/// about a millisecond), ahead of its frame.
pub const CAPTURE_UPLOAD_WORDS_PER_PASS: usize = 1 << 20;
/// Words of new glyphs a parked capture prepares per pass of the engine loop (about 3 ms: the
/// window's frames run between passes), until its views' text is ready or its wait runs out.
pub const CAPTURE_PREP_WORDS_PER_PASS: usize = 16 * 1024;
/// The size layouts are worked out at (pixels per em), then scaled to metres.
const REF_PX: f32 = 64.0;
/// Most glyphs in-world text draws per frame (the rest are reported as a problem).
pub const MAX_FRAME_GLYPHS: usize = 16_384;
/// The glyph store's starting cap (bytes).
pub const STORE_BYTES: usize = specula_text::slug::DEFAULT_STORE_BYTES;
/// The most the glyph store grows to (bytes): about 16,000 CJK glyphs, as many as a frame may
/// draw ([`MAX_FRAME_GLYPHS`]); within the GPU's [`specula_gpu::MAX_GLYPH_WORDS`].
pub const MAX_STORE_BYTES: usize = 32 << 20;

// ---------------------------------------------------------------------------------------------
// Faces
// ---------------------------------------------------------------------------------------------

/// The bundled faces, parsed once.
pub struct Faces {
    pub ui: Font,
    pub ui_bold: Font,
    pub display: Font,
}

/// The bundled faces (parsing compiled-in bytes: no disk).
pub fn faces() -> &'static Faces {
    static FACES: OnceLock<Faces> = OnceLock::new();
    FACES.get_or_init(|| Faces {
        ui: Bundled::UiRegular.load().expect("the bundled UI face parses (tested)"),
        ui_bold: Bundled::UiBold.load().expect("the bundled bold face parses (tested)"),
        display: Bundled::Display.load().expect("the bundled display face parses (tested)"),
    })
}

fn face_of(b: Bundled) -> &'static Font {
    let f = faces();
    match b {
        Bundled::UiRegular => &f.ui,
        Bundled::UiBold => &f.ui_bold,
        Bundled::Display => &f.display,
    }
}

/// A CJK fallback family, chosen by the text's locale (or, without one, by its characters).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cjk {
    Ja,
    Ko,
    ZhHans,
    ZhHant,
}

impl Cjk {
    const ALL: [Cjk; 4] = [Cjk::Ja, Cjk::Ko, Cjk::ZhHans, Cjk::ZhHant];

    fn locale(self) -> &'static str {
        match self {
            Cjk::Ja => "ja",
            Cjk::Ko => "ko",
            Cjk::ZhHans => "zh-Hans",
            Cjk::ZhHant => "zh-Hant",
        }
    }

    /// The family for `locale` (a BCP 47 tag) if it names a CJK language.
    pub fn of_locale(locale: &str) -> Option<Cjk> {
        let l = locale.to_ascii_lowercase();
        let lang = l.split(['-', '_']).next().unwrap_or("");
        match lang {
            "ja" => Some(Cjk::Ja),
            "ko" => Some(Cjk::Ko),
            "zh" if l.contains("hant") || l.ends_with("-tw") || l.ends_with("-hk") || l.ends_with("-mo") => Some(Cjk::ZhHant),
            "zh" => Some(Cjk::ZhHans),
            _ => None,
        }
    }

    /// The family `text` needs, from its characters: kana → Japanese, hangul → Korean, other Han
    /// → Simplified Chinese; `None` when it has no CJK characters.
    pub fn of_text(text: &str) -> Option<Cjk> {
        let mut han = false;
        for c in text.chars() {
            match c as u32 {
                0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F => return Some(Cjk::Ja),
                0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7A3 => return Some(Cjk::Ko),
                0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x3FFFD => han = true,
                _ => {}
            }
        }
        han.then_some(Cjk::ZhHans)
    }
}

static FALLBACKS: [OnceLock<Option<Font>>; 4] = [OnceLock::new(), OnceLock::new(), OnceLock::new(), OnceLock::new()];
static STARTED: [AtomicBool; 4] = [AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false)];
static LOADED: AtomicU32 = AtomicU32::new(0);

/// The fallback face for `cjk`, if loaded. The first ask starts loading it on its own thread
/// (CJK font files are 10–20 MB: never read on the engine thread); until it is in, text needing
/// it shows those characters as missing (and says so), and is laid out again once it arrives.
pub fn fallback(cjk: Cjk) -> Option<&'static Font> {
    let i = Cjk::ALL.iter().position(|c| *c == cjk).unwrap_or(0);
    if let Some(f) = FALLBACKS[i].get() {
        return f.as_ref();
    }
    if !STARTED[i].swap(true, Ordering::AcqRel) {
        let spawned = std::thread::Builder::new().name("specula-font-fallback".into()).spawn(move || {
            let font = load_fallback(cjk);
            let _ = FALLBACKS[i].set(font);
            relay_out(cjk);
            LOADED.fetch_add(1, Ordering::AcqRel);
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "text.fallback_loaded", locale = cjk.locale(), found = FALLBACKS[i].get().is_some_and(Option::is_some));
        });
        if spawned.is_err() {
            let _ = FALLBACKS[i].set(None);
            LOADED.fetch_add(1, Ordering::AcqRel);
        }
    }
    None
}

/// Changes whenever a fallback face finishes loading (layouts made before then are redone).
pub fn fallback_generation() -> u32 {
    LOADED.load(Ordering::Acquire)
}

/// Whether `cjk`'s fallback has finished loading (found or not); the first ask starts the load.
fn fallback_settled(cjk: Cjk) -> bool {
    let _ = fallback(cjk);
    FALLBACKS[Cjk::ALL.iter().position(|c| *c == cjk).unwrap_or(0)].get().is_some()
}

/// The first of `cjk`'s system faces that loads (on the loader thread). A read that fails for a
/// reason other than a missing file (a machine out of commit memory while builds run, a file
/// another program holds) is tried again twice, so one bad moment doesn't leave the text without
/// its face for the rest of the run; every failure is traced with its file and code.
fn load_fallback(cjk: Cjk) -> Option<Font> {
    let candidates: Vec<_> = specula_text::system_fallback_candidates(cjk.locale()).into_iter().filter(|p| p.is_file()).collect();
    for attempt in 0..3u32 {
        let mut transient = false;
        for p in &candidates {
            match specula_text::load_font_file(p) {
                Ok(f) => return Some(f),
                Err(e) => {
                    transient |= matches!(&e, specula_text::LoadError::Io { kind, .. } if *kind != std::io::ErrorKind::NotFound);
                    let file = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "text.fallback_failed", locale = cjk.locale(), file = file, code = e.code(), attempt = attempt);
                }
            }
        }
        if !transient || attempt == 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500 << (2 * attempt)));
    }
    None
}

/// Most texts waiting for their CJK face to be laid out again (each holds a copy of its
/// component until then); past it, a text keeps its first layout and a trace says so.
pub const MAX_PENDING_RELAYOUTS: usize = 65_536;

/// Texts laid out before their CJK face settled, with their family: the loader thread lays them
/// out again once the face is in (review of 50a4772: laying them out on first use put every such
/// text's layout on the engine thread in one frame).
static PENDING: Mutex<Vec<(Cjk, TextSpec)>> = Mutex::new(Vec::new());
/// Serialises the loaders' re-layout batches (two families can settle together).
static RELAYING: Mutex<()> = Mutex::new(());
/// Re-layout batches: the last one numbered, the last one finished, and the one frames use.
/// The engine adopts finished batches only between frames and calls (at the start of each loop
/// turn), so within a frame or a capture, and its sight packet, a text's box and its glyphs
/// always come from the same layout (review of e97e318: adopting at the start of the 2D draws
/// came after the capture had read its subjects).
static BATCH_NEXT: AtomicU64 = AtomicU64::new(0);
static BATCH_DONE: AtomicU64 = AtomicU64::new(0);
static BATCH_ADOPTED: AtomicU64 = AtomicU64::new(0);

/// Lays out again, on the loader thread, every pending text of `cjk` (its face just settled).
fn relay_out(cjk: Cjk) {
    let mine: Vec<TextSpec> = {
        let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
        let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut *pending).into_iter().partition(|(c, _)| *c == cjk);
        *pending = rest;
        mine.into_iter().map(|(_, s)| s).collect()
    };
    if fallback(cjk).is_none() {
        return; // No face was found: the first layouts are already what they will be.
    }
    let _serial = RELAYING.lock().unwrap_or_else(PoisonError::into_inner);
    let batch = BATCH_NEXT.fetch_add(1, Ordering::AcqRel) + 1;
    let mut n = 0u64;
    for spec in &mine {
        // Only texts something still holds (the world, the journal).
        if Arc::strong_count(&spec.laid) > 1 && spec.laid.later.set((spec.make_laid(), batch)).is_ok() {
            n += 1;
        }
    }
    BATCH_DONE.store(batch, Ordering::Release);
    specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "text.relaid", locale = cjk.locale(), texts = n, batch = batch);
}

/// The engine calls this between frames and calls: re-layouts finished since are used from now on.
pub fn adopt_relayouts() {
    BATCH_ADOPTED.store(BATCH_DONE.load(Ordering::Acquire), Ordering::Release);
}

/// The last re-layout batch frames adopted (sight's subject cache reads text again when it moves).
pub fn adopted_batch() -> u64 {
    BATCH_ADOPTED.load(Ordering::Acquire)
}

/// Texts world files brought in, waiting for the load thread to lay them out (each holds a copy
/// of its component until then). It grows with the load, never past it: the world being loaded
/// holds every one of them anyway, and a load past the queue's size would otherwise lay its
/// texts out on the loading (engine) thread (review R2 of m5-text, minor 2).
static LOAD_QUEUE: Mutex<Vec<TextSpec>> = Mutex::new(Vec::new());
/// Texts laid out on the loading thread because the load thread couldn't start (traced).
static LAID_INLINE: AtomicU64 = AtomicU64::new(0);
/// The load thread is running.
static LOADING: AtomicBool = AtomicBool::new(false);
/// Texts one batch of the load thread lays out (frames adopt each batch as it finishes).
const LOAD_BATCH: usize = 256;

/// Queues a world file's text for the load thread, starting it if it isn't running.
fn queue_loaded(spec: TextSpec) {
    LOAD_QUEUE.lock().unwrap_or_else(PoisonError::into_inner).push(spec);
    if !LOADING.swap(true, Ordering::AcqRel) {
        let started = std::thread::Builder::new().name("specula-text-load".into()).spawn(lay_out_loaded);
        if started.is_err() {
            // No thread: laid out here, as before (traced).
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "text.load_thread_failed");
            let n = LOAD_QUEUE.lock().unwrap_or_else(PoisonError::into_inner).len() as u64;
            LAID_INLINE.fetch_add(n, Ordering::AcqRel);
            lay_out_loaded();
        }
    }
}

/// The load thread: lays out queued world-file texts, a batch at a time, until none are left.
fn lay_out_loaded() {
    loop {
        let batch_specs: Vec<TextSpec> = {
            let mut q = LOAD_QUEUE.lock().unwrap_or_else(PoisonError::into_inner);
            let n = q.len().min(LOAD_BATCH);
            q.drain(..n).collect()
        };
        if batch_specs.is_empty() {
            LOADING.store(false, Ordering::Release);
            // A text queued between the empty look and the flag: take it up (or another thread has).
            if LOAD_QUEUE.lock().unwrap_or_else(PoisonError::into_inner).is_empty() || LOADING.swap(true, Ordering::AcqRel) {
                return;
            }
            continue;
        }
        let _serial = RELAYING.lock().unwrap_or_else(PoisonError::into_inner);
        let batch = BATCH_NEXT.fetch_add(1, Ordering::AcqRel) + 1;
        let mut n = 0u64;
        for spec in &batch_specs {
            // Only texts something still holds (the world, the journal).
            if Arc::strong_count(&spec.laid) < 2 {
                continue;
            }
            let cjk = spec.cjk();
            let complete = cjk.is_none_or(fallback_settled);
            if spec.laid.loaded.set((spec.make_laid(), batch)).is_ok() {
                n += 1;
            }
            if let (false, Some(c)) = (complete, cjk) {
                // Its face is still loading: laid out again when it arrives (as any text).
                let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
                if fallback_settled(c) {
                    drop(pending);
                    let _ = spec.laid.later.set((spec.make_laid(), batch));
                } else if pending.len() < MAX_PENDING_RELAYOUTS {
                    pending.push((c, spec.clone()));
                }
            }
        }
        BATCH_DONE.fetch_max(batch, Ordering::AcqRel);
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "text.loaded_laid_out", texts = n, batch = batch);
    }
}

/// World-file texts queued for the load thread and not laid out yet.
pub fn loads_queued() -> usize {
    LOAD_QUEUE.lock().unwrap_or_else(PoisonError::into_inner).len()
}

// ---------------------------------------------------------------------------------------------
// The component
// ---------------------------------------------------------------------------------------------

/// A caret and selection, and an IME composition in progress, while the text is being edited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditState {
    /// Bytes into the text.
    pub caret: u32,
    pub anchor: u32,
    pub composing: Option<(String, u32)>,
}

/// The `text` component.
#[derive(Clone, Debug)]
pub struct TextSpec {
    /// The text (markup when `rich`).
    pub text: String,
    pub font: Bundled,
    /// Metres per em.
    pub size: f32,
    pub align: Align,
    /// Wrap width in metres (`None`: lines end only at line breaks).
    pub max_width: Option<f32>,
    pub line_height: f32,
    pub max_lines: Option<u32>,
    /// The box point at the entity's origin: (0, 0) bottom left, (0.5, 0.5) centre.
    pub anchor: [f32; 2],
    pub lit: bool,
    /// `text` is markup ([b], [color=#rrggbb], [icon=name]; see `specula_text::rich`).
    pub rich: bool,
    pub z_layer: i32,
    /// BCP 47 locale for fallback faces (CJK); without one, chosen from the characters.
    pub locale: Option<String>,
    pub edit: Option<EditState>,
    /// The plain text and its spans (derived from `text` and `rich`).
    plain: Arc<(String, Vec<Span>)>,
    /// Its layout and drawn box, made at the last refresh (shared by clones: the same text).
    laid: Arc<Layouts>,
}

/// A layout of a [`TextSpec`] and the box everything it draws covers (metres in the plane).
#[derive(Debug, Default)]
pub struct Laid {
    pub layout: TextLayout,
    pub ink: [f32; 4],
}

/// [`TextSpec`]'s layouts. Frames, culling, sight packets and `text_edit`'s answer all use the
/// same one, so what is drawn and what is measured never disagree.
#[derive(Debug, Default)]
struct Layouts {
    /// Made at the last refresh.
    first: Laid,
    /// `first` used every face it ever will: no family is needed, or its load had settled.
    complete: bool,
    /// Made once by the loader thread after the family's face settled (when `first` wasn't
    /// complete), with its batch: used from the first frame that adopted the batch.
    later: OnceLock<(Laid, u64)>,
    /// A text a world file brought in (`two_d::deferred`): `first` is empty, and the load
    /// thread lays it out ([`lay_out_loaded`]) into `loaded`, used from the first frame that
    /// adopted its batch (review of cd954bd, major 4: a `world_load` of 2000 texts laid them all
    /// out on the engine thread, 1.95 s).
    deferred: bool,
    loaded: OnceLock<(Laid, u64)>,
}

/// Layouts made since start-up (a steady scene makes none).
static LAYOUTS_MADE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Layouts made since start-up, by every text (a steady scene makes none).
pub fn layouts_made() -> u64 {
    LAYOUTS_MADE.load(Ordering::Relaxed)
}

#[cfg(test)]
thread_local! {
    /// Layouts made on this thread (tests: where layouts happen).
    static LAID_HERE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Layouts made on the calling thread.
#[cfg(test)]
fn laid_here() -> u64 {
    LAID_HERE.with(std::cell::Cell::get)
}

impl PartialEq for TextSpec {
    fn eq(&self, o: &TextSpec) -> bool {
        self.text == o.text
            && self.font == o.font
            && self.size == o.size
            && self.align == o.align
            && self.max_width == o.max_width
            && self.line_height == o.line_height
            && self.max_lines == o.max_lines
            && self.anchor == o.anchor
            && self.lit == o.lit
            && self.rich == o.rich
            && self.z_layer == o.z_layer
            && self.locale == o.locale
            && self.edit == o.edit
    }
}

const TEXT_KEYS: &[&str] = &["text", "font", "size", "align", "max_width", "line_height", "max_lines", "anchor", "lit", "rich", "z_layer", "locale", "editing"];
const FONTS: [(&str, Bundled); 3] = [("ui", Bundled::UiRegular), ("ui-bold", Bundled::UiBold), ("display", Bundled::Display)];
const ALIGNS: [(&str, Align); 3] = [("left", Align::Left), ("center", Align::Center), ("right", Align::Right)];

fn name_of<T: PartialEq + Copy>(options: &[(&'static str, T)], v: T) -> &'static str {
    options.iter().find(|o| o.1 == v).map_or("", |o| o.0)
}

impl TextSpec {
    /// A plain text with the defaults (tests, games).
    pub fn new(text: &str) -> TextSpec {
        let mut s = TextSpec {
            text: text.to_owned(),
            font: Bundled::UiRegular,
            size: 0.25,
            align: Align::Center,
            max_width: None,
            line_height: 1.0,
            max_lines: None,
            anchor: [0.5, 0.5],
            lit: false,
            rich: false,
            z_layer: 0,
            locale: None,
            edit: None,
            plain: Arc::new((text.to_owned(), Vec::new())),
            laid: Arc::default(),
        };
        s.refresh().expect("plain text is always valid");
        s
    }

    /// Re-derives the plain text and spans after `text` or `rich` changed.
    pub fn refresh(&mut self) -> Result<(), String> {
        if self.text.len() > MAX_TEXT_BYTES {
            return Err(format!("'text.text' is {} bytes; at most {MAX_TEXT_BYTES}", self.text.len()));
        }
        let plain = if self.rich {
            let r = rich::parse(&self.text).map_err(|e| format!("'text.text' markup: {e}"))?;
            (r.text, r.spans)
        } else {
            (self.text.clone(), Vec::new())
        };
        if let Some(e) = &self.edit {
            let len = plain.0.len() as u32;
            if e.caret > len || e.anchor > len || !plain.0.is_char_boundary(e.caret as usize) || !plain.0.is_char_boundary(e.anchor as usize) {
                return Err(format!("'text.editing' caret {} / anchor {} must be character boundaries within the text's {len} bytes", e.caret, e.anchor));
            }
        }
        self.plain = Arc::new(plain);
        if crate::two_d::is_deferred() {
            // A world file's text: laid out by the load thread, never on the loading thread.
            self.laid = Arc::new(Layouts { deferred: true, ..Layouts::default() });
            queue_loaded(self.clone());
            return Ok(());
        }
        // Settled before laying out: a face arriving in between only costs a second layout.
        let cjk = self.cjk();
        let complete = cjk.is_none_or(fallback_settled);
        self.laid = Arc::new(Layouts { first: self.make_laid(), complete, ..Layouts::default() });
        if let (false, Some(c)) = (complete, cjk) {
            // Waits for the loader, unless the face settled meanwhile: then it is laid out
            // again here, before anyone else sees it (the loader has taken its list already).
            let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
            if fallback_settled(c) {
                drop(pending);
                self.laid = Arc::new(Layouts { first: self.make_laid(), complete: true, ..Layouts::default() });
            } else if pending.len() < MAX_PENDING_RELAYOUTS {
                pending.push((c, self.clone()));
            } else {
                specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "text.relayout_queue_full", locale = c.locale(), cap = MAX_PENDING_RELAYOUTS);
            }
        }
        Ok(())
    }

    /// A text a world file brought in whose layout the load thread hasn't made (or frames haven't
    /// adopted) yet: it draws nothing, and captures wait for it.
    pub fn layout_pending(&self) -> bool {
        let l = &*self.laid;
        l.deferred && l.loaded.get().is_none_or(|(_, batch)| *batch > BATCH_ADOPTED.load(Ordering::Acquire))
    }

    fn make_laid(&self) -> Laid {
        LAYOUTS_MADE.fetch_add(1, Ordering::Relaxed);
        #[cfg(test)]
        LAID_HERE.with(|c| c.set(c.get() + 1));
        let mut layout = self.lay_out();
        layout.compact();
        let ink = self.ink_box(&layout);
        Laid { layout, ink }
    }

    /// The text's layout and drawn box: the one made at its last refresh, or, from the first
    /// frame after the loader laid it out again with the CJK face it was waiting for, that one.
    /// Never lays anything out.
    pub fn laid(&self) -> &Laid {
        let l = &*self.laid;
        if !l.complete {
            let adopted = BATCH_ADOPTED.load(Ordering::Acquire);
            if let Some((later, batch)) = l.later.get() {
                if *batch <= adopted {
                    return later;
                }
            }
            if let Some((loaded, batch)) = l.loaded.get() {
                if *batch <= adopted {
                    return loaded;
                }
            }
        }
        &l.first
    }

    pub fn parse(v: &Json) -> Result<TextSpec, String> {
        let w = "text";
        keys(v, w, TEXT_KEYS)?;
        let text = match opt(v, "text") {
            Some(t) => t.as_str().ok_or("'text.text' must be a string")?.to_owned(),
            None => return Err("'text.text' is required: the words to show".into()),
        };
        let locale = match opt(v, "locale") {
            None => None,
            Some(l) => {
                let s = l.as_str().ok_or("'text.locale' must be a BCP 47 tag like \"ja\" or \"zh-Hant\"")?;
                if s.is_empty() || s.len() > 16 || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                    return Err("'text.locale' must be a BCP 47 tag like \"ja\" or \"zh-Hant\" (1-16 of a-z, 0-9, '-')".into());
                }
                Some(s.to_owned())
            }
        };
        let edit = match opt(v, "editing") {
            None => None,
            Some(e) => {
                keys(e, "text.editing", &["caret", "anchor", "composing", "composing_caret"])?;
                let caret = int(e, "caret", "text.editing", 0, MAX_TEXT_BYTES as i64)?.unwrap_or(0) as u32;
                let anchor = int(e, "anchor", "text.editing", 0, MAX_TEXT_BYTES as i64)?.map_or(caret, |a| a as u32);
                let composing = match opt(e, "composing") {
                    None => None,
                    Some(c) => {
                        let s = c.as_str().ok_or("'text.editing.composing' must be a string")?;
                        if s.len() > MAX_COMPOSING_BYTES {
                            return Err(format!("'text.editing.composing' is at most {MAX_COMPOSING_BYTES} bytes"));
                        }
                        let at = int(e, "composing_caret", "text.editing", 0, MAX_COMPOSING_BYTES as i64)?.unwrap_or(s.len() as i64) as u32;
                        (!s.is_empty()).then(|| (s.to_owned(), at.min(s.len() as u32)))
                    }
                };
                Some(EditState { caret, anchor, composing })
            }
        };
        let mut s = TextSpec {
            text,
            font: choice(v, "font", w, &FONTS)?.unwrap_or(Bundled::UiRegular),
            size: num(v, "size", w, 0.005, 100.0)?.unwrap_or(0.25),
            align: choice(v, "align", w, &ALIGNS)?.unwrap_or(Align::Center),
            max_width: num(v, "max_width", w, 0.01, 1000.0)?,
            line_height: num(v, "line_height", w, 0.5, 4.0)?.unwrap_or(1.0),
            max_lines: int(v, "max_lines", w, 1, MAX_TEXT_LINES as i64)?.map(|n| n as u32),
            anchor: nums::<2>(v, "anchor", w, 0.0, 1.0)?.unwrap_or([0.5, 0.5]),
            lit: boolean(v, "lit", w)?.unwrap_or(false),
            rich: boolean(v, "rich", w)?.unwrap_or(false),
            z_layer: int(v, "z_layer", w, -1000, 1000)?.unwrap_or(0) as i32,
            locale,
            edit,
            plain: Arc::new((String::new(), Vec::new())),
            laid: Arc::default(),
        };
        if s.rich && s.edit.is_some() {
            return Err("'text.editing' needs plain text: rich text is changed with world_set".into());
        }
        s.refresh()?;
        if let Some(e) = &mut s.edit {
            // Snap to caret stops (never inside a character's combining marks).
            let mut t = TextEdit::with_limit(&s.plain.0, MAX_TEXT_BYTES);
            t.set_selection(e.anchor as usize, e.caret as usize);
            e.caret = t.caret() as u32;
            e.anchor = t.anchor() as u32;
        }
        Ok(s)
    }

    /// Canonical JSON (defaults left out).
    pub fn to_json(&self) -> Json {
        let mut f = vec![("text", Json::from(self.text.as_str()))];
        if self.font != Bundled::UiRegular {
            f.push(("font", Json::from(name_of(&FONTS, self.font))));
        }
        if self.size != 0.25 {
            f.push(("size", Json::from(self.size as f64)));
        }
        if self.align != Align::Center {
            f.push(("align", Json::from(name_of(&ALIGNS, self.align))));
        }
        if let Some(m) = self.max_width {
            f.push(("max_width", Json::from(m as f64)));
        }
        if self.line_height != 1.0 {
            f.push(("line_height", Json::from(self.line_height as f64)));
        }
        if let Some(n) = self.max_lines {
            f.push(("max_lines", Json::from(n as u64)));
        }
        if self.anchor != [0.5, 0.5] {
            f.push(("anchor", Json::arr(self.anchor.map(|a| Json::from(a as f64)))));
        }
        if self.lit {
            f.push(("lit", Json::from(true)));
        }
        if self.rich {
            f.push(("rich", Json::from(true)));
        }
        if self.z_layer != 0 {
            f.push(("z_layer", Json::from(self.z_layer as i64)));
        }
        if let Some(l) = &self.locale {
            f.push(("locale", Json::from(l.as_str())));
        }
        if let Some(e) = &self.edit {
            let mut ef = vec![("caret", Json::from(e.caret as u64)), ("anchor", Json::from(e.anchor as u64))];
            if let Some((c, at)) = &e.composing {
                ef.push(("composing", Json::from(c.as_str())));
                ef.push(("composing_caret", Json::from(*at as u64)));
            }
            f.push(("editing", Json::obj(ef)));
        }
        Json::obj(f)
    }

    /// The plain text (markup removed) and its spans.
    pub fn plain(&self) -> &str {
        &self.plain.0
    }

    /// The text as shown: an IME composition (if any) inserted at the caret, and its range.
    pub fn shown(&self) -> (String, Option<std::ops::Range<usize>>) {
        match self.edit.as_ref().and_then(|e| e.composing.as_ref().map(|c| (e, c))) {
            None => (self.plain.0.clone(), None),
            Some((e, (c, _))) => {
                let at = e.caret.min(e.anchor) as usize;
                let end = e.caret.max(e.anchor) as usize;
                let p = &self.plain.0;
                let (at, end) = (at.min(p.len()), end.min(p.len()));
                (format!("{}{}{}", &p[..at], c, &p[end..]), Some(at..at + c.len()))
            }
        }
    }

    fn cjk(&self) -> Option<Cjk> {
        let composing = self.edit.as_ref().and_then(|e| e.composing.as_ref()).map_or("", |(c, _)| c.as_str());
        self.locale.as_deref().and_then(Cjk::of_locale).or_else(|| Cjk::of_text(&self.plain.0)).or_else(|| Cjk::of_text(composing))
    }

    /// The faces this text draws from (the CJK fallback when loaded).
    pub fn font_set(&self) -> FontSet<'static> {
        static NONE: [&Font; 0] = [];
        let regular = face_of(self.font);
        let bold = (self.font == Bundled::UiRegular).then(|| &faces().ui_bold);
        let fallback: &'static [&'static Font] = match self.cjk().and_then(fallback) {
            Some(f) => {
                // One leaked slice per loaded face (at most four, ever).
                static SLICES: OnceLock<[OnceLock<[&'static Font; 1]>; 4]> = OnceLock::new();
                let slices = SLICES.get_or_init(Default::default);
                let i = Cjk::ALL.iter().position(|c| Some(*c) == self.cjk()).unwrap_or(0);
                slices[i].get_or_init(|| [f]).as_slice()
            }
            None => &NONE,
        };
        FontSet { regular, bold, fallback }
    }

    fn style(&self) -> TextStyle {
        let k = REF_PX / self.size;
        TextStyle {
            size_px: REF_PX,
            scale: 1.0,
            line_height: self.line_height,
            max_width: self.max_width.map(|w| w * k),
            align: self.align,
            kerning: true,
            wrap: true,
            max_lines: self.max_lines,
            ellipsis: self.max_lines.is_some(),
        }
    }

    /// Lays the shown text out (reference pixels, y down) into `out`.
    pub fn lay_out_into(&self, out: &mut TextLayout) {
        let (shown, _) = self.shown();
        let spans: &[Span] = if self.edit.as_ref().is_some_and(|e| e.composing.is_some()) { &[] } else { &self.plain.1 };
        layout_rich_into(&self.font_set(), &shown, spans, &self.style(), out);
    }

    pub fn lay_out(&self) -> TextLayout {
        let mut out = TextLayout::default();
        self.lay_out_into(&mut out);
        out
    }

    /// Metres per reference pixel.
    fn k(&self) -> f32 {
        self.size / REF_PX
    }

    /// The box in the plane (metres): x0, y0, x1, y1, for a layout of this text.
    pub fn plane_box(&self, l: &TextLayout) -> [f32; 4] {
        let k = self.k();
        let (w, h) = (l.width * k, l.height * k);
        let [ax, ay] = self.anchor;
        [-ax * w, -ay * h, (1.0 - ax) * w, (1.0 - ay) * h]
    }

    /// The box everything drawn covers (metres in the plane): the layout box, grown to every
    /// glyph's ink (accents above capitals, descenders, overhangs) and, while editing, the caret.
    /// Sight's projected box must contain every pixel the ID buffer counts.
    pub fn ink_box(&self, l: &TextLayout) -> [f32; 4] {
        let mut b = self.plane_box(l);
        let k = self.k();
        let set = self.font_set();
        // Each glyph's box from its `glyf` header (ten bytes, no outline parse: review of
        // cd954bd, major 3, where a CJK paragraph's ~680 outline parses overran text_edit's
        // engine-thread budget on every call).
        for g in &l.glyphs {
            let font = set.face(g.face);
            let Some([x0, y0, x1, y1]) = font.glyph_box(g.glyph).map(|b| b.map(f32::from)) else { continue };
            // Grown by the slack a rounded composite box may have.
            let slack = specula_text::font::GLYPH_BOX_SLACK;
            let [x0, y0, x1, y1] = [x0 - slack, y0 - slack, x1 + slack, y1 + slack];
            let s = font.scale_for_px(l.size_px);
            let p0 = self.plane(l, g.x + x0 * s, g.y - y0 * s);
            let p1 = self.plane(l, g.x + x1 * s, g.y - y1 * s);
            b = [b[0].min(p0[0]).min(p1[0]), b[1].min(p0[1]).min(p1[1]), b[2].max(p0[0]).max(p1[0]), b[3].max(p0[1]).max(p1[1])];
        }
        if self.edit.is_some() {
            let w = REF_PX * 0.03 * k;
            b = [b[0] - w, b[1], b[2] + w, b[3]];
        }
        b
    }

    /// A layout point (reference px, y down from the box's top-left) in the plane (metres).
    fn plane(&self, l: &TextLayout, x: f32, y: f32) -> [f32; 2] {
        let b = self.plane_box(l);
        let k = self.k();
        [b[0] + x * k, b[3] - y * k]
    }

    /// Local bounds: the box everything drawn covers.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let b = self.laid().ink;
        (Vec3::new(b[0], b[1], 0.0), Vec3::new(b[2], b[3], 0.0))
    }

    /// Facts for sight subjects: the text (cut to 64 characters), its lines, and what didn't fit
    /// or has no glyph.
    pub fn subject_json(&self) -> Json {
        let l = &self.laid().layout;
        let shown: String = self.plain.0.chars().take(64).collect();
        let mut f = vec![
            ("text", Json::from(shown.as_str())),
            ("font", Json::from(name_of(&FONTS, self.font))),
            ("lines", Json::from(l.lines.len())),
            ("z_layer", Json::from(self.z_layer as i64)),
        ];
        if self.plain.0.chars().count() > 64 {
            f.push(("text_cut", Json::from(true)));
        }
        if let Some(t) = l.truncated {
            f.push(("truncated_at", Json::from(t)));
        }
        if l.overflow {
            f.push(("overflow", Json::from(true)));
        }
        if !l.missing.is_empty() {
            let m: String = l.missing.iter().take(16).collect();
            f.push(("missing_glyphs", Json::from(m.as_str())));
        }
        if let Some(e) = &self.edit {
            f.push(("editing", Json::obj([("caret", Json::from(e.caret as u64)), ("anchor", Json::from(e.anchor as u64))])));
        }
        Json::obj(f)
    }

    /// Bytes this copy accounts for: its own fields, and its share of the layouts every copy
    /// shares (the world's and the journal's copies of one text hold one layout; review of
    /// 50a4772: counting it whole in each copy filled the journal's ring).
    pub fn heap_bytes(&self) -> usize {
        fn layout_bytes(l: &TextLayout) -> usize {
            l.glyphs.len() * std::mem::size_of::<specula_text::PositionedGlyph>()
                + l.lines.len() * std::mem::size_of::<specula_text::Line>()
                + l.missing.len() * 4
                + l.inlines.len() * std::mem::size_of::<specula_text::InlineBox>()
        }
        let shared = std::mem::size_of::<Layouts>()
            + layout_bytes(&self.laid.first.layout)
            + self.laid.later.get().map_or(0, |(l, _)| layout_bytes(&l.layout))
            + self.laid.loaded.get().map_or(0, |(l, _)| layout_bytes(&l.layout));
        std::mem::size_of::<TextSpec>() + self.text.len() + self.plain.0.len() + self.plain.1.len() * 48 + shared / Arc::strong_count(&self.laid).max(1)
    }
}

// ---------------------------------------------------------------------------------------------
// Editing (text_edit)
// ---------------------------------------------------------------------------------------------

/// What one `text_edit` call did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditReport {
    /// Text the call's copy and cut operations took, last first-come (the agent's clipboard: an
    /// agent never touches the human's).
    pub copied: Option<String>,
    /// Bytes of input dropped (past the text's limit, or control characters).
    pub dropped: usize,
    pub ops: usize,
    /// Layouts made (line movement and the refresh): never more than [`edit_cost`] said.
    pub layouts: usize,
}

fn unit_of(v: &Json) -> Result<Unit, String> {
    choice(v, "unit", "text_edit op", &[("char", Unit::Char), ("word", Unit::Word), ("line", Unit::Line), ("all", Unit::All)]).map(|u| u.unwrap_or(Unit::Char))
}

/// Parses `text_edit`'s `ops`: `{"insert": "…"}`, `{"paste": "…"}`, `{"key": "left|right|up|down|home|end|backspace|delete|select_all|copy|cut|paste|undo|redo|cancel_composition", "unit": "char|word|line|all", "select": bool}`, `{"caret": byte, "select": bool}`, `{"select": [anchor, caret]}`, `{"compose": "…", "caret": byte}`, `{"commit": "…"}`.
pub fn parse_ops(v: &Json) -> Result<Vec<EditOp>, String> {
    let arr = v.as_array().ok_or("'ops' must be an array of edit operations")?;
    if arr.is_empty() || arr.len() > MAX_EDIT_OPS {
        return Err(format!("'ops' holds 1 to {MAX_EDIT_OPS} operations"));
    }
    let text = |o: &Json, k: &str| -> Result<String, String> {
        let s = o.get(k).and_then(Json::as_str).ok_or_else(|| format!("'{k}' must be a string"))?;
        if s.len() > MAX_TEXT_BYTES {
            return Err(format!("'{k}' is at most {MAX_TEXT_BYTES} bytes"));
        }
        Ok(s.to_owned())
    };
    let mut out = Vec::with_capacity(arr.len());
    for (i, o) in arr.iter().enumerate() {
        let at = |e: String| format!("ops[{i}]: {e}");
        let Json::Obj(pairs) = o else { return Err(at("an operation is an object".into())) };
        let first = pairs.first().map(|(k, _)| k.as_str()).unwrap_or("");
        let select = || boolean(o, "select", "text_edit op").map(|b| b.unwrap_or(false));
        let op = match first {
            "insert" => {
                keys(o, "an insert", &["insert"]).map_err(at)?;
                EditOp::Insert(text(o, "insert").map_err(at)?)
            }
            "paste" => {
                keys(o, "a paste", &["paste"]).map_err(at)?;
                EditOp::Insert(text(o, "paste").map_err(at)?)
            }
            "commit" => {
                keys(o, "a commit", &["commit"]).map_err(at)?;
                EditOp::Commit(text(o, "commit").map_err(at)?)
            }
            "compose" => {
                keys(o, "a compose", &["compose", "caret"]).map_err(at)?;
                let t = text(o, "compose").map_err(at)?;
                if t.len() > MAX_COMPOSING_BYTES {
                    return Err(at(format!("a composition is at most {MAX_COMPOSING_BYTES} bytes")));
                }
                let caret = int(o, "caret", "compose", 0, MAX_COMPOSING_BYTES as i64).map_err(at)?.map_or(t.len(), |c| c as usize);
                EditOp::Compose { text: t, caret }
            }
            "caret" => {
                keys(o, "a caret move", &["caret", "select"]).map_err(at)?;
                let b = int(o, "caret", "op", 0, MAX_TEXT_BYTES as i64).map_err(at)?.unwrap_or(0) as usize;
                EditOp::SetCaret { byte: b, select: select().map_err(at)? }
            }
            "select" if o.get("select").and_then(Json::as_array).is_some() => {
                keys(o, "a selection", &["select"]).map_err(at)?;
                let a = o.get("select").and_then(Json::as_array).filter(|a| a.len() == 2).ok_or_else(|| at("'select' is [anchor, caret]".into()))?;
                let n = |j: &Json| j.as_u64().filter(|&x| x <= MAX_TEXT_BYTES as u64).map(|x| x as usize).ok_or_else(|| at(format!("selection offsets are 0..={MAX_TEXT_BYTES}")));
                EditOp::Select { anchor: n(&a[0])?, caret: n(&a[1])? }
            }
            "key" => {
                keys(o, "a key", &["key", "unit", "select"]).map_err(at)?;
                let unit = unit_of(o).map_err(at)?;
                let select = select().map_err(at)?;
                let k = o.get("key").and_then(Json::as_str).unwrap_or("");
                match k {
                    "left" => EditOp::Left { unit, select },
                    "right" => EditOp::Right { unit, select },
                    "home" => EditOp::Left { unit: Unit::Line, select },
                    "end" => EditOp::Right { unit: Unit::Line, select },
                    "up" => EditOp::Up { select },
                    "down" => EditOp::Down { select },
                    "backspace" => EditOp::Backspace(unit),
                    "delete" => EditOp::Delete(unit),
                    "select_all" => EditOp::SelectAll,
                    "copy" => EditOp::Copy,
                    "cut" => EditOp::Cut,
                    "paste" => EditOp::Paste,
                    "undo" => EditOp::Undo,
                    "redo" => EditOp::Redo,
                    "cancel_composition" => EditOp::CancelComposition,
                    other => {
                        let shown: String = other.chars().take(24).collect();
                        return Err(at(format!(
                            "'{shown}' is not a key: left, right, up, down, home, end, backspace, delete, select_all, copy, cut, paste, undo, redo, cancel_composition"
                        )));
                    }
                }
            }
            other => {
                let shown: String = other.chars().take(24).collect();
                return Err(at(format!("'{shown}': an operation is {{insert}}, {{paste}}, {{key}}, {{caret}}, {{select: [a, b]}}, {{compose}} or {{commit}}")));
            }
        };
        out.push(op);
    }
    Ok(out)
}

/// What a run of edit operations costs in layout work, worked out from the ops alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditCost {
    /// Layouts it may make: one per line movement after the text changed (or first), plus the
    /// refreshes (after the ops, and again when the edit ends).
    pub layouts: usize,
    /// The longest the text can get (bytes: at least its characters).
    pub longest: usize,
    /// Characters it may lay out: `layouts` × `longest`.
    pub chars: usize,
}

impl EditCost {
    /// The structured refusal of a call over [`EDIT_LAYOUT_CHARS`].
    pub fn refusal(&self) -> crate::world::WorldError {
        crate::world::WorldError::new(
            "limit",
            format!(
                "these ops could lay the text out {} times ({} characters of up to {} bytes each), past the {EDIT_LAYOUT_CHARS} characters one call may lay out: \
                 send fewer line movements (up, down, home, end with unit line) after changes per call, or split the ops over calls",
                self.layouts, self.chars, self.longest
            ),
        )
        .with("field", "ops")
        .with("limit", EDIT_LAYOUT_CHARS as u64)
        .with("got", self.chars as u64)
    }
}

fn moves_by_line(op: &EditOp) -> bool {
    matches!(op, EditOp::Up { .. } | EditOp::Down { .. } | EditOp::Left { unit: Unit::Line, .. } | EditOp::Right { unit: Unit::Line, .. })
}

/// The layout work `ops` may do on `spec` ([`apply_edits_with`] never does more): a line
/// movement lays the text out when it changed since the last layout; anything but a caret move,
/// a selection or a copy may change it. `ending`: the edit ends after (one more refresh).
pub fn edit_cost(spec: &TextSpec, ops: &[EditOp], ending: bool) -> EditCost {
    let composing = spec.edit.as_ref().and_then(|e| e.composing.as_ref()).map_or(0, |(c, _)| c.len());
    let mut longest = spec.plain().len() + composing;
    let mut layouts = 1 + usize::from(ending);
    let mut changed = true;
    for op in ops {
        match op {
            op if moves_by_line(op) => {
                if changed {
                    layouts += 1;
                    changed = false;
                }
            }
            EditOp::Left { .. } | EditOp::Right { .. } | EditOp::SetCaret { .. } | EditOp::Select { .. } | EditOp::SelectAll | EditOp::Copy => {}
            EditOp::Insert(s) | EditOp::Commit(s) => {
                longest += s.len();
                changed = true;
            }
            EditOp::Compose { text, .. } => {
                longest += text.len();
                changed = true;
            }
            // A paste brings what the clipboard holds: the human's, in the window, can hold
            // anything (review of 50a4772), so the longest the text may then be.
            EditOp::Paste => {
                longest = longest.max(MAX_TEXT_BYTES);
                changed = true;
            }
            // Deletions shorten it; undo and redo bring back an earlier text of this call.
            _ => changed = true,
        }
    }
    let longest = longest.min(MAX_TEXT_BYTES + MAX_COMPOSING_BYTES);
    EditCost { layouts, longest, chars: layouts.saturating_mul(longest) }
}

/// [`edit_cost`] for a frame of the window's keys: each undo or redo (the window's own history,
/// any text of up to the longest) ends a run of edits, which is laid out.
pub fn window_cost(spec: &TextSpec, ops: &[EditOp], ending: bool) -> EditCost {
    let mut c = edit_cost(spec, ops, ending);
    let steps = ops.iter().filter(|o| matches!(o, EditOp::Undo | EditOp::Redo)).count();
    if steps > 0 {
        c.layouts += steps;
        c.longest = c.longest.max(MAX_TEXT_BYTES);
        c.chars = c.layouts.saturating_mul(c.longest);
    }
    c
}

/// Applies `ops` to `spec` (plain text only): the new component (with its caret, selection and
/// composition kept in `editing`) and what happened. Pure: the caller commits it.
pub fn apply_edits(spec: &TextSpec, ops: &[EditOp]) -> Result<(TextSpec, EditReport), String> {
    apply_edits_with(spec, ops, &mut MemoryClipboard::default())
}

/// [`apply_edits`] with `clipboard` for copy, cut and paste (the window's typing passes the
/// system clipboard; agents get an empty one of their own per call).
pub fn apply_edits_with(spec: &TextSpec, ops: &[EditOp], cb: &mut dyn specula_text::Clipboard) -> Result<(TextSpec, EditReport), String> {
    if spec.rich {
        return Err("rich text is changed with world_set (text_edit types into plain text)".into());
    }
    let mut t = TextEdit::with_limit(&spec.plain.0, MAX_TEXT_BYTES);
    if let Some(e) = &spec.edit {
        t.set_selection(e.anchor as usize, e.caret as usize);
    }
    if let Some((c, at)) = spec.edit.as_ref().and_then(|e| e.composing.clone()) {
        t.apply(&EditOp::Compose { text: c, caret: at as usize }, cb, None);
    }
    // Cost before work (review of 3eefaa2: 256 `down` ops between edits of a long text held the
    // engine thread 1.9 s): the whole run's layout work is known before any op applies.
    let cost = edit_cost(spec, ops, false);
    if cost.chars > EDIT_LAYOUT_CHARS {
        return Err(cost.refusal().message);
    }
    let mut report = EditReport { ops: ops.len(), ..EditReport::default() };
    let mut layout = TextLayout::default();
    // The text `layout` was made from (`None`: none yet). Line movement lays the text out only
    // when it changed since; the model above counted each one, and the work never passes it.
    let mut laid: Option<String> = None;
    let mut layouts_left = cost.layouts - 1;
    let (set, style) = (spec.font_set(), spec.style());
    for op in ops {
        let needs_layout = moves_by_line(op);
        let mut have = needs_layout && laid.as_deref() == Some(t.text());
        if needs_layout && !have && layouts_left > 0 {
            layouts_left -= 1;
            report.layouts += 1;
            layout_rich_into(&set, t.text(), &[], &style, &mut layout);
            laid = Some(t.text().to_owned());
            have = true;
        }
        let r = t.apply(op, cb, have.then_some(&layout));
        report.dropped += r.dropped;
        if matches!(op, EditOp::Copy | EditOp::Cut) && !r.noop {
            report.copied = cb.get();
        }
    }
    let mut next = spec.clone();
    next.text = t.text().to_owned();
    next.edit = Some(EditState {
        caret: t.caret() as u32,
        anchor: t.anchor() as u32,
        composing: t.composition().map(|Composition { text, caret }| (text.clone(), *caret as u32)),
    });
    next.refresh()?;
    report.layouts += 1;
    Ok((next, report))
}

// ---------------------------------------------------------------------------------------------
// GPU side
// ---------------------------------------------------------------------------------------------

/// The engine's in-world text: the glyph store (shared by every text) and this frame's glyph
/// instances. Layouts live in the components ([`TextSpec::laid`]).
pub struct TextGpu {
    store: SlugStore,
    glyphs: Vec<GlyphInstance>,
    frame: u64,
    /// Glyphs left out this frame (the frame's glyph cap or a full store).
    pub dropped: usize,
    /// Words of new glyphs a frame may prepare (`None`: a capture's [`CAPTURE_PREP_WORDS`];
    /// window frames pass [`WINDOW_PREP_WORDS`]), what is left of it this frame, and the glyphs
    /// left to later frames (review of cd954bd, major 1: captures prepared without limit, about
    /// 0.1 ms a CJK glyph, and held the engine thread for 0.8 s).
    pub prep_words: Option<usize>,
    /// A window frame: the editing marks (caret, selection, composition) are drawn only on the
    /// text the window's keyboard types into, `Some(None)`: on none (review R4 of m5-text: any
    /// agent may set `editing`, and a caret drawn with no focus imitates a live field the human's
    /// keys never reach). `None`: a capture, every text's marks drawn (agents check them).
    pub window_marks: Option<Option<u64>>,
    new_left: usize,
    pub deferred: usize,
    /// The store's words as last snapshot: id, length, words.
    shared: (u64, usize, Arc<[u32]>),
}

/// [`TextGpu::snapshot`]: a frame's text, owned.
pub struct TextSnapshot {
    pub store: u64,
    pub words: Arc<[u32]>,
    pub glyphs: Vec<GlyphInstance>,
}

impl TextSnapshot {
    pub fn frame(&self) -> TextFrame<'_> {
        TextFrame { store: self.store, words: &self.words, glyphs: &self.glyphs }
    }
}

impl Default for TextGpu {
    fn default() -> TextGpu {
        TextGpu { store: SlugStore::growing(STORE_BYTES, MAX_STORE_BYTES), glyphs: Vec::new(), frame: 0, dropped: 0, prep_words: None, window_marks: None, new_left: 0, deferred: 0, shared: (0, 0, Arc::from(&[][..])) }
    }
}

/// Selection highlight and caret colours (STYLE.md: cyan marks what is selected).
const SELECTION: [f32; 4] = [0.247, 0.878, 0.816, 0.35];
const CARET: [f32; 4] = [0.925, 0.902, 0.839, 1.0];

impl TextGpu {
    /// Starts a frame's text: the store may start over (between frames only), the glyph list
    /// empties.
    pub fn begin_frame(&mut self) {
        // A capture of another view uses little of the store and must not make the window's
        // text look stale (review of 787f9fb): only window frames count as the scene's.
        if self.prep_words.is_some() {
            self.store.begin_frame();
        } else {
            self.store.begin_capture();
        }
        self.glyphs.clear();
        self.dropped = 0;
        self.deferred = 0;
        self.new_left = self.prep_words.unwrap_or(CAPTURE_PREP_WORDS);
        self.frame += 1;
    }

    /// The frame's text for `FrameDesc::text`.
    pub fn frame(&self) -> TextFrame<'_> {
        TextFrame { store: self.store.id(), words: self.store.words(), glyphs: &self.glyphs }
    }

    /// The frame's text as owned data (captures, whose frame outlives other engine calls): the
    /// store's words are shared, copied only when the store changed since the last snapshot.
    pub fn snapshot(&mut self) -> TextSnapshot {
        let (id, len) = (self.store.id(), self.store.words().len());
        if self.shared.0 != id || self.shared.1 != len {
            self.shared = (id, len, Arc::from(self.store.words()));
        }
        TextSnapshot { store: id, words: Arc::clone(&self.shared.2), glyphs: self.glyphs.clone() }
    }

    /// Bytes of the glyph store, glyphs stored, and its resets.
    pub fn store_stats(&self) -> (usize, usize, u64) {
        (self.store.bytes(), self.store.glyph_count(), self.store.resets)
    }

    /// The glyph store (its counters: grows, resets, glyphs prepared and refused).
    pub fn store(&self) -> &SlugStore {
        &self.store
    }

    /// Prepares, within `budget` words, glyphs `spec` draws that aren't stored yet (a parked
    /// capture, between frames); returns how many are still to prepare. Refused and broken glyphs
    /// aren't waited for (the frame reports them).
    pub fn prepare_ahead(&mut self, spec: &TextSpec, budget: &mut usize) -> usize {
        let layout = &spec.laid().layout;
        let set = spec.font_set();
        let mut left = 0;
        for g in &layout.glyphs {
            let face = set.face(g.face);
            if self.store.contains(face, g.glyph) || self.store.is_refused(face, g.glyph) {
                continue;
            }
            if *budget == 0 {
                left += 1;
                continue;
            }
            let before = self.store.prepared_words;
            let _ = self.store.glyph(face, g.glyph);
            // Charged what it took to prepare, stored or refused (at least a header's worth).
            *budget = budget.saturating_sub(((self.store.prepared_words - before) as usize).max(specula_text::slug::HEADER_WORDS));
        }
        left
    }

    /// Makes the store's words the capture snapshot shares ([`TextGpu::snapshot`]) current: a
    /// parked capture does it once its view's glyphs are prepared, so its frame doesn't copy the
    /// store (up to 32 MiB) when it renders.
    pub fn share_words(&mut self) {
        let (id, len) = (self.store.id(), self.store.words().len());
        if self.shared.0 != id || self.shared.1 != len {
            self.shared = (id, len, Arc::from(self.store.words()));
        }
    }

    /// Uploads, ahead of the capture that draws them, up to [`CAPTURE_UPLOAD_WORDS_PER_PASS`] of
    /// the store's words the GPU doesn't hold yet (a parked capture's pass), so the capture's own
    /// frame copies little.
    pub fn preload(&self, gpu: &mut specula_gpu::Gpu) {
        if !self.store.words().is_empty() {
            gpu.preload_glyphs(self.store.id(), self.store.words(), self.store.cap_bytes() / 4, CAPTURE_UPLOAD_WORDS_PER_PASS);
        }
    }

    /// What this frame left out, for its packet: glyphs deferred to a later frame (the frame's
    /// preparation budget) and glyphs dropped (the frame's cap, a full store, a broken glyph).
    pub fn frame_problem(&self) -> Option<String> {
        let mut parts = Vec::new();
        if self.deferred > 0 {
            parts.push(format!(
                "{} in-world glyph(s) not drawn yet: a capture prepares at most {CAPTURE_PREP_WORDS} words of new glyphs when it renders (after preparing ahead while it waits); capture again to draw them",
                self.deferred
            ));
        }
        if self.dropped > 0 {
            parts.push(format!(
                "{} in-world glyph(s) dropped (the frame's {MAX_FRAME_GLYPHS} glyphs, a glyph store full at its {} MiB, or a glyph that can't be drawn; {} refused for want of room)",
                self.dropped,
                self.store.max_bytes() >> 20,
                self.store.refused_count()
            ));
        }
        (!parts.is_empty()).then(|| parts.join("; "))
    }

    /// Appends entity `id`'s draws (world transform `model`, sRGB `tint`): its glyphs as one
    /// text run, and while editing its selection, IME composition underline and caret. Returns a
    /// problem to report, if any.
    pub fn draws(&mut self, id: u64, spec: &TextSpec, model: &Mat4, tint: [f32; 4], out: &mut Vec<Draw2d>) -> Option<String> {
        // The component's own layout (made where the text was set): no layout work here.
        let layout = &spec.laid().layout;
        let cols = model.to_cols_array();
        let rect = spec.plane_box(layout);
        let set = spec.font_set();
        let first = self.glyphs.len() as u32;
        let mut problem = None;
        let k = spec.k();
        let [bx0, _, _, by1] = rect;
        let spans = &spec.plain.1;
        let composing = spec.edit.as_ref().is_some_and(|e| e.composing.is_some());
        for g in &layout.glyphs {
            if g.glyph == 0 && layout.inlines.iter().any(|b| b.byte == g.byte) {
                continue; // an inline icon: in-world text leaves its box empty
            }
            if self.glyphs.len() >= MAX_FRAME_GLYPHS {
                self.dropped += 1;
                problem.get_or_insert_with(|| format!("text {id}: the frame's {MAX_FRAME_GLYPHS} in-world glyphs are used up"));
                continue;
            }
            let face = set.face(g.face);
            let before = self.store.prepared_words;
            let new = !self.store.contains(face, g.glyph) && !self.store.is_refused(face, g.glyph);
            if new && self.new_left == 0 {
                self.deferred += 1;
                continue;
            }
            let stored = self.store.glyph(face, g.glyph);
            if new {
                // Charged what it took to prepare, stored or refused (at least a header's worth:
                // an empty or broken glyph is still read).
                self.new_left = self.new_left.saturating_sub(((self.store.prepared_words - before) as usize).max(specula_text::slug::HEADER_WORDS));
            }
            match stored {
                Ok(Some(s)) => {
                    let color = if composing { None } else { g.span.checked_sub(1).and_then(|i| spans.get(i as usize)).and_then(|sp| sp.color) };
                    self.glyphs.push(GlyphInstance {
                        word: s.word,
                        origin: [bx0 + g.x * k, by1 - g.y * k],
                        size: spec.size,
                        bounds: s.bounds,
                        // A span's colour replaces the entity's (multiplying would hide cyan in dark carved
                        // text); the run carries only the entity's opacity.
                        color: color.unwrap_or([tint[0], tint[1], tint[2], 1.0]),
                    });
                }
                Ok(None) => {}
                Err(e) => {
                    self.dropped += 1;
                    problem.get_or_insert_with(|| format!("text {id}: {e}"));
                }
            }
        }
        let count = self.glyphs.len() as u32 - first;
        let marks = self.window_marks.is_none_or(|focus| focus == Some(id));
        if let Some(e) = spec.edit.as_ref().filter(|_| marks) {
            // Selection behind the glyphs, the caret (and the composition's underline) in front.
            let (_, comp) = spec.shown();
            let sel = (e.caret.min(e.anchor) as usize)..(e.caret.max(e.anchor) as usize);
            if comp.is_none() {
                for r in layout.selection_rects(sel) {
                    out.push(Draw2d::Sprite(quad(spec, layout, &cols, r, SELECTION, -1e-3)));
                }
            }
            if let Some(c) = &comp {
                for r in layout.selection_rects(c.clone()) {
                    let base = r[3] - (layout.descent * 0.6);
                    out.push(Draw2d::Sprite(quad(spec, layout, &cols, [r[0], base - 2.0, r[2], base], CARET, 1e-3)));
                }
            }
            let caret_byte = match (&comp, &e.composing) {
                (Some(c), Some((_, at))) => c.start + *at as usize,
                _ => e.caret as usize,
            };
            let p = layout.caret(caret_byte);
            let w = (REF_PX * 0.06).max(1.0);
            out.push(Draw2d::Sprite(quad(spec, layout, &cols, [p.x - w * 0.5, p.top, p.x + w * 0.5, p.top + p.height], CARET, 2e-3)));
        }
        if count > 0 {
            out.push(Draw2d::Text(TextRun { model: cols, rect, first, count, color: [1.0, 1.0, 1.0, tint[3]], layer: spec.z_layer, lit: spec.lit }));
        }
        problem
    }

}

// ---------------------------------------------------------------------------------------------
// The text_edit tool
// ---------------------------------------------------------------------------------------------

/// `text_edit`'s schema: the agent's documentation.
pub fn tool_def() -> specula_protocol::ToolDef {
    use specula_protocol::ToolDef;
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    ToolDef {
        name: "text_edit".into(),
        description: "Type into a text entity (shape 'text', plain text) with a caret and selection, as a person \
                      would: ops apply in order and the result is one journaled, claim-checked change (world_undo \
                      reverts it). ops: {insert: \"...\"} (replaces the selection), {paste: \"...\"}, {key: left|right|\
                      up|down|home|end|backspace|delete|select_all|copy|cut|paste|undo|redo|cancel_composition, unit: \
                      char|word|line|all, select: bool (extend the selection)}, {caret: byte, select}, {select: \
                      [anchor, caret]}, {compose: \"...\", caret} (an IME composition shown at the caret, not yet \
                      text), {commit: \"...\"}. Offsets are UTF-8 bytes, snapped so a letter and its accents move \
                      together. copy and cut fill your own clipboard (answered as copied; the human's clipboard is \
                      never touched) and {key: paste} pastes it within the call. Up to 256 ops (an empty list only with window_focus or done); the text holds at \
                      most 2048 bytes (the rest is dropped and counted). A call may lay the text out about 4 times (each up, down, home or end after a change is one layout): more is refused up front with a limit error, so split such ops over calls. undo and redo step back within one \
                      call; across calls use world_undo (every call is one journal entry). The caret and \
                      selection stay in the component (text.editing) and are drawn, the selection in cyan and the \
                      caret in ivory, until a call with done: true."
            .into(),
        input_schema: Json::obj([
            ("type", Json::from("object")),
            (
                "properties",
                Json::obj([
                    ("id", t("integer", "Entity id")),
                    ("path", t("string", "Entity path")),
                    ("name", t("string", "Entity name (unique in the world)")),
                    ("expect_version", t("integer", "Refuse unless the entity is still at this version")),
                    ("window_focus", t("boolean", "The owner or an operator-granted lead only. From anyone else it is refused (lead_only) whatever the text, and the refused try is a security incident that freezes the caller's session (docs/design/SECURITY.md). true: the human's keyboard in the window (keys, characters, IME, the system clipboard) types into this text, under a banner naming it drawn over everything (no key lands before it shows), until Escape, any click, an engine screen, anyone else's write to the text or an ancestor, or the text is hidden, deleted, under half opaque, off screen or tiny (such a text is refused: not_visible, with reason). false: stop. A click never points the keyboard at a world text; the human types into one through the editor's Text field (F7)")),
                    ("done", t("boolean", "true: end the edit after the ops (the caret and selection are no longer drawn)")),
                    (
                        "ops",
                        Json::obj([
                            ("type", Json::from("array")),
                            ("description", Json::from("Edit operations, applied in order (see the tool description)")),
                            ("items", Json::obj([("type", Json::from("object"))])),
                        ]),
                    ),
                ]),
            ),
            ("required", Json::arr([Json::from("ops")])),
        ]),
    }
}

/// A `text_edit` worked out on the caller's thread, for the engine to commit
/// ([`crate::Engine::text_commit`]): the text after the ops, laid out already.
#[derive(Debug)]
pub struct TextCommit {
    pub id: u64,
    pub next: TextSpec,
    pub report: EditReport,
    /// The entity's version the ops were applied to: the commit is refused (`stale_version`)
    /// if another writer changed it since, and the caller applies them again to the new text.
    pub base_version: u64,
    /// The agent's own `expect_version` (a refusal then is the agent's to handle).
    pub expect_version: Option<u64>,
    pub window_focus: Option<bool>,
}

/// The ops applied to `spec` (and the edit ended when `done`), within the cost model: the text
/// after, and what happened. Pure: `text_edit` runs it on the caller's thread (review of
/// cd954bd, major 3: on the engine thread, a CJK text's layouts overran its budget every call).
pub fn prepare_edit(spec: &TextSpec, ops: &[EditOp], done: bool) -> Result<(TextSpec, EditReport), crate::world::WorldError> {
    use crate::world::WorldError;
    // Cost before work: the call's layout work, from the ops, before any is applied.
    let cost = edit_cost(spec, ops, done);
    if cost.chars > EDIT_LAYOUT_CHARS {
        return Err(cost.refusal());
    }
    let (mut next, report) = apply_edits(spec, ops).map_err(|m| WorldError::new("invalid_value", m).with("field", "ops"))?;
    if done {
        next.edit = None;
        next.refresh().map_err(|m| WorldError::new("invalid_value", m).with("field", "ops"))?;
    }
    Ok((next, report))
}

/// Times `text_edit` applies its ops again when another writer changed the text in between.
const EDIT_TRIES: usize = 3;

/// `text_edit`, on the calling agent's connection thread: the text is read from the engine, the
/// ops are applied and laid out here (within the cost model), and the engine only commits the
/// result (no layout on the engine thread, whatever the script).
pub fn call(tools: &crate::tools::Tools, session: &specula_protocol::SessionInfo, args: &Json) -> specula_protocol::ToolOutput {
    use crate::tools::{bool_arg, check_keys, entity_key, version_arg, Command};
    use specula_protocol::ToolOutput;
    let error = |e: &crate::world::WorldError| ToolOutput::error(e.to_json().to_pretty());
    let parsed = (|| -> Result<_, String> {
        check_keys(args, "text_edit", &[&["id", "path", "name", "expect_version", "ops", "window_focus", "done"]])?;
        let key = entity_key(args, "text_edit")?;
        let ops = args.get("ops").filter(|o| !o.is_null()).ok_or("'ops' is required: [{insert: \"...\"}, {key: \"left\"}, ...]")?;
        let (window_focus, done) = (bool_arg(args, "window_focus")?, bool_arg(args, "done")?.unwrap_or(false));
        // No ops: only to point or stop the keyboard, or to end the edit.
        let ops = match ops.as_array() {
            Some([]) if window_focus.is_some() || done => Vec::new(),
            _ => parse_ops(ops)?,
        };
        Ok((key, ops, version_arg(args)?, window_focus, done))
    })();
    let (key, ops, expect_version, window_focus, done) = match parsed {
        Ok(p) => p,
        Err(m) => return ToolOutput::error(crate::tools::argument_error(&m).to_pretty()),
    };
    // Pointing the human's keyboard is the operator's: refused before any work (the engine checks
    // again where it commits).
    if window_focus.is_some() {
        if let Err(e) = crate::authority::require(crate::authority::trusted_of_session(&session.id), crate::authority::Authority::Lead, crate::authority::Site::WindowFocus, WINDOW_FOCUS_WHAT) {
            // A refused try at the human's keyboard: a security incident (docs/design/SECURITY.md).
            crate::inputguard::refused(crate::inputguard::Channel::Keyboard, "text_edit.window_focus", &crate::inputguard::Actor::caller(session), e.code, &key.to_string());
            return error(&e);
        }
    }
    let mut last = None;
    for _ in 0..EDIT_TRIES {
        let got = tools.send(session, Command::TextGet { key: key.clone() });
        if got.is_error {
            return got;
        }
        let j = crate::lex_tools::output_json(&got);
        let (Some(id), Some(version), Some(text)) = (j.get("id").and_then(Json::as_u64), j.get("version").and_then(Json::as_u64), j.get("text")) else {
            return ToolOutput::error(crate::world::WorldError::new("internal", "the engine's answer about the text is incomplete").to_json().to_pretty());
        };
        let spec = match TextSpec::parse(text) {
            Ok(s) => s,
            Err(m) => return error(&crate::world::WorldError::new("invalid_component", m).with("field", "text")),
        };
        let (next, report) = match prepare_edit(&spec, &ops, done) {
            Ok(r) => r,
            Err(e) => return error(&e),
        };
        let commit = TextCommit { id, next, report, base_version: version, expect_version, window_focus };
        let out = tools.send(session, Command::TextEdit(Box::new(commit)));
        let stale = out.is_error && crate::lex_tools::output_json(&out).get("error").and_then(Json::as_str) == Some("stale_version");
        if !(stale && expect_version.is_none()) {
            return out;
        }
        last = Some(out);
    }
    last.expect("tried")
}

const WINDOW_FOCUS_WHAT: &str = "text_edit window_focus (pointing the human's keyboard and clipboard at a text; a click never does: the human types into world texts through the editor's Text field)";

impl crate::Engine {
    /// The text entity `key`'s component, for `text_edit` on the caller's thread: its id,
    /// version and `text` JSON (no layout here).
    pub(crate) fn text_get(&self, key: &crate::world::EntityKey) -> specula_protocol::ToolOutput {
        use crate::world::WorldError;
        use specula_protocol::ToolOutput;
        let error = |e: &WorldError| ToolOutput::error(e.to_json().to_pretty());
        let id = match self.world.resolve(key) {
            Ok(id) => id,
            Err(e) => return error(&e),
        };
        let e = self.world.get(id).expect("resolved");
        let Some(crate::two_d::TwoD::Text(spec)) = &e.two_d else {
            return error(&WorldError::new("invalid_component", format!("'{}' is a {}, not a text entity", e.name, e.shape.name())).with("field", "id"));
        };
        ToolOutput::text(Json::obj([("id", Json::from(id)), ("version", Json::from(e.version)), ("text", spec.to_json())]).to_string())
    }

    /// Commits a `text_edit` the caller's thread worked out ([`TextCommit`]) as one change, and
    /// points the window's keyboard when asked (the operator's only).
    pub(crate) fn text_commit(&mut self, c: TextCommit, w: crate::journal::Writer, tick: u64, by: &str, actor: crate::inputguard::Actor) -> specula_protocol::ToolOutput {
        use crate::world::{EntityKey, UpdateSpec, WorldError};
        use specula_protocol::ToolOutput;
        let error = |e: &WorldError| ToolOutput::error(e.to_json().to_pretty());
        let TextCommit { id, next, report, base_version, expect_version, window_focus } = c;
        let have = actor.authority();
        // The human's keyboard and clipboard are theirs: only the operator (an owner or lead
        // grant) points them at a text (review of cd954bd, major 2: a builder aimed them at a
        // hidden text another agent read). Refused before anything else; nothing changes. A
        // refused try is a security incident (docs/design/SECURITY.md).
        if window_focus.is_some() {
            if let Err(e) = crate::authority::require(have, crate::authority::Authority::Lead, crate::authority::Site::WindowFocus, WINDOW_FOCUS_WHAT) {
                crate::inputguard::refused(crate::inputguard::Channel::Keyboard, "text_edit.window_focus", &actor, e.code, &self.world.path_of(id));
                return error(&e);
            }
        }
        // The human must see what they type into: a hidden, faint, off-screen or tiny text can't
        // take the keyboard (and loses it when it becomes one: text_window). The banner names it.
        if window_focus == Some(true) && self.world.get(id).is_some() {
            if let Some(why) = self.text_focus_problem(id, false) {
                let name = self.world.get(id).map(|e| e.name.clone()).unwrap_or_default();
                return error(&WorldError::new("not_visible", format!("'{name}' can't take the window's keyboard: the human can't see it ({why}); show it in the window first")).with("field", "window_focus").with("reason", why));
            }
        }
        let change = UpdateSpec { two_d: Some(crate::two_d::TwoD::Text(Box::new(next.clone()))), expect_version: Some(expect_version.unwrap_or(base_version)), ..Default::default() };
        let yours = crate::conflict::Yours::of_update(&change);
        let by = by.to_owned();
        let committed = self.world.commit(&mut self.swarm, tick, w, crate::journal::Kind::Set, crate::write_policy::Rule::World, |wd| {
            wd.update(&EntityKey::Id(id), change, &by).map(|e| (e.id, e.version))
        });
        match committed {
            Ok(((id, version), seq)) => {
                match window_focus {
                    Some(true) => {
                        let path = self.world.path_of(id);
                        let watch = crate::text_window::watch_of(&self.world, id);
                        // The monitor sees it first: a lead's pointing is an alert the owner is
                        // told of (unless allowed for this session); the owner's is fine.
                        let seen = crate::inputguard::done(crate::inputguard::Channel::Keyboard, "text_edit.window_focus", &actor, &path);
                        self.text_typing.focus_on(id, path, actor, watch, seen);
                    }
                    Some(false) => {
                        if self.text_typing.focus == Some(id) {
                            self.text_typing.drop_focus("text_edit");
                        }
                    }
                    // The operator's own edit of the focused text keeps the focus (anyone
                    // else's drops it at the next window frame: text_window).
                    None if have >= crate::authority::Authority::Lead => self.text_typing.wrote(id, expect_version.unwrap_or(base_version), version),
                    None => {}
                }
                let len = next.text.len() as u32;
                let edit = next.edit.clone().unwrap_or(EditState { caret: len, anchor: len, composing: None });
                let sel = (edit.caret.min(edit.anchor) as usize)..(edit.caret.max(edit.anchor) as usize);
                let layout = &next.laid().layout;
                let mut f = vec![
                    ("id", Json::from(id)),
                    ("version", Json::from(version)),
                    ("journal", Json::from(seq)),
                    ("text", Json::from(next.text.as_str())),
                    ("caret", Json::from(edit.caret as u64)),
                    ("anchor", Json::from(edit.anchor as u64)),
                    ("selected", Json::from(next.text.get(sel).unwrap_or(""))),
                    ("lines", Json::from(layout.lines.len())),
                    ("ops", Json::from(report.ops)),
                    ("window_focus", Json::from(self.text_typing.focus == Some(id))),
                    ("editing", Json::from(next.edit.is_some())),
                ];
                if let Some((c, at)) = &edit.composing {
                    f.push(("composing", Json::obj([("text", Json::from(c.as_str())), ("caret", Json::from(*at as u64))])));
                }
                if let Some(c) = &report.copied {
                    let shown: String = c.chars().take(512).collect();
                    f.push(("copied", Json::from(shown.as_str())));
                }
                if report.dropped > 0 {
                    f.push(("dropped_bytes", Json::from(report.dropped)));
                }
                if let Some(t) = layout.truncated {
                    f.push(("truncated_at", Json::from(t)));
                }
                ToolOutput::text(Json::obj(f).to_pretty())
            }
            Err(e) => error(&self.conflict_help(e, Some(&yours))),
        }
    }
}

/// A flat quad over layout rect `r` (reference px), pushed `lift` metres off the plane.
fn quad(spec: &TextSpec, layout: &TextLayout, cols: &[f32; 16], r: [f32; 4], color: [f32; 4], lift: f32) -> Sprite {
    let a = spec.plane(layout, r[0], r[3]);
    let b = spec.plane(layout, r[2], r[1]);
    let (cx, cy) = ((a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5);
    let ax = Vec3::new(cols[0], cols[1], cols[2]);
    let ay = Vec3::new(cols[4], cols[5], cols[6]);
    let n = ax.cross(ay);
    let n = if n.length() > 1e-12 { n.normalize() } else { Vec3::Z };
    let o = Vec3::new(cols[12], cols[13], cols[14]) + ax * cx + ay * cy + n * lift;
    Sprite {
        position: [o.x, o.y, o.z],
        size: [(b[0] - a[0]).abs() * ax.length(), (b[1] - a[1]).abs() * ay.length()],
        pivot: [0.5, 0.5],
        axes: [[ax.x, ax.y, ax.z], [ay.x, ay.y, ay.z]],
        color,
        layer: spec.z_layer,
        blend: SpriteBlend::Blended,
        ..Default::default()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    /// A copy of `spec` whose layout was redone by a loader batch `batch` with drawn box `ink`.
    pub(crate) fn with_later(spec: &TextSpec, ink: [f32; 4], batch: u64) -> TextSpec {
        let mut s = spec.clone();
        let later = OnceLock::new();
        let _ = later.set((Laid { layout: spec.laid().layout.clone(), ink }, batch));
        s.laid = Arc::new(Layouts { first: Laid { layout: spec.laid().layout.clone(), ink: spec.laid().ink }, complete: false, later, ..Layouts::default() });
        s
    }

    /// A batch number no frame has adopted yet.
    pub(crate) fn reserve_batch() -> u64 {
        BATCH_NEXT.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Marks `batch` finished and adopts it, as a loader and the next frame would.
    pub(crate) fn finish_batch(batch: u64) {
        let _serial = RELAYING.lock().unwrap_or_else(PoisonError::into_inner);
        BATCH_DONE.fetch_max(batch, Ordering::AcqRel);
        adopt_relayouts();
    }

    use super::*;

    fn spec(j: &str) -> Result<TextSpec, String> {
        TextSpec::parse(&Json::parse(j).unwrap())
    }

    #[test]
    fn parses_round_trips_and_bounds_follow_the_anchor() {
        let s = spec(r#"{"text":"Here lies Ada","font":"display","size":0.1,"align":"left","anchor":[0,0],"lit":true,"z_layer":2}"#).unwrap();
        assert_eq!(TextSpec::parse(&s.to_json()).unwrap(), s);
        let (lo, hi) = s.bounds();
        assert_eq!((lo.x, lo.y), (0.0, 0.0));
        assert!(hi.x > 0.4 && hi.x < 1.0 && hi.y > 0.08 && hi.y < 0.2, "{hi:?}");
        let c = spec(r#"{"text":"x"}"#).unwrap();
        let (lo, hi) = c.bounds();
        assert!((lo.x + hi.x).abs() < 1e-5 && (lo.y + hi.y).abs() < 1e-5, "centred");
        assert_eq!(c.to_json().to_string(), r#"{"text":"x"}"#);
    }

    #[test]
    fn bad_components_say_what_to_change() {
        for (j, needle) in [
            (r#"{}"#, "required"),
            (r#"{"text":"a","font":"comic"}"#, "font"),
            (r#"{"text":"a","size":0}"#, "size"),
            (r#"{"text":"a","colour":1}"#, "colour"),
            (r#"{"text":"[b]a","rich":true}"#, "markup"),
            (r#"{"text":"a","locale":"日本"}"#, "locale"),
            (r#"{"text":"ab","editing":{"caret":9}}"#, "editing"),
            (r#"{"text":"[b]a[/b]","rich":true,"editing":{"caret":0}}"#, "plain text"),
        ] {
            let e = spec(j).unwrap_err();
            assert!(e.contains(needle), "{j}: {e}");
        }
        let long = format!(r#"{{"text":"{}"}}"#, "x".repeat(MAX_TEXT_BYTES + 1));
        assert!(spec(&long).unwrap_err().contains("at most"));
        // Carets inside a character's marks snap back.
        let s = spec(r#"{"text":"éx","editing":{"caret":2}}"#).unwrap_err();
        assert!(s.contains("boundar"), "{s}");
        let s = spec(r#"{"text":"éx","editing":{"caret":1}}"#).unwrap();
        assert_eq!(s.edit.unwrap().caret, 0, "snapped before the cluster");
    }

    #[test]
    fn edits_type_select_copy_and_compose() {
        let s = spec(r#"{"text":"Here lies"}"#).unwrap();
        let ops = parse_ops(&Json::parse(r#"[{"key":"end"},{"insert":" Ada"},{"key":"left","unit":"word","select":true},{"key":"copy"},{"key":"right"},{"insert":", "},{"key":"paste"}]"#).unwrap()).unwrap();
        let (n, r) = apply_edits(&s, &ops).unwrap();
        assert_eq!(n.text, "Here lies Ada, Ada");
        assert_eq!(r.copied.as_deref(), Some("Ada"));
        assert_eq!(n.edit.as_ref().map(|e| (e.caret, e.anchor)), Some((18, 18)));
        // Select a word, then compose over it and commit.
        let ops = parse_ops(&Json::parse(r#"[{"select":[10,13]},{"compose":"やま","caret":6}]"#).unwrap()).unwrap();
        let (c, _) = apply_edits(&n, &ops).unwrap();
        assert_eq!(c.text, "Here lies Ada, Ada", "a composition isn't text yet");
        assert_eq!(c.shown().0, "Here lies やま, Ada");
        let (d, _) = apply_edits(&c, &parse_ops(&Json::parse(r#"[{"commit":"山"}]"#).unwrap()).unwrap()).unwrap();
        assert_eq!(d.text, "Here lies 山, Ada");
        assert_eq!(TextSpec::parse(&d.to_json()).unwrap(), d, "editing state round-trips");
        assert!(apply_edits(&spec(r#"{"text":"[b]x[/b]","rich":true}"#).unwrap(), &ops).is_err());
    }

    /// Review of 3eefaa2: 256 `down` ops on a 2 KB wrapped text laid it out (with ink bounds)
    /// twice per op and held the engine thread 1.9 s. Cost before work (lead, 2026-11-07): the
    /// call's layout work is worked out from the ops first; over [`EDIT_LAYOUT_CHARS`] it is
    /// refused with a `limit` error naming what is too big, and the work never passes the model.
    #[test]
    fn line_movement_costs_what_the_model_says_and_too_much_is_refused() {
        let words = "Here lies Ada of the north gate ".repeat(70);
        let s = TextSpec::parse(&Json::obj([("text", Json::from(&words[..1990])), ("max_width", Json::from(5.0)), ("editing", Json::obj([("caret", Json::from(0u64))]))])).unwrap();
        // An unchanged text is laid out once, however many moves.
        let downs = vec![EditOp::Down { select: false }; MAX_EDIT_OPS];
        let c = edit_cost(&s, &downs, false);
        assert_eq!(c.layouts, 2, "one for the moves, one refresh");
        let (n, r) = apply_edits(&s, &downs).unwrap();
        assert_eq!(n.edit.unwrap().caret as usize, n.text.len(), "down far enough reaches the end");
        assert!(r.layouts <= c.layouts);
        // Typing between moves: every move after a change is a layout. The worst a call may send
        // (256 ops on the longest text) is refused before any op applies.
        let mixed: Vec<EditOp> = (0..MAX_EDIT_OPS).map(|i| if i % 2 == 0 { EditOp::Insert("x".into()) } else { EditOp::Up { select: false } }).collect();
        let c = edit_cost(&s, &mixed, false);
        assert_eq!(c.layouts, 1 + MAX_EDIT_OPS / 2);
        let e = c.refusal();
        assert_eq!(e.code, "limit");
        let j = e.to_json();
        assert_eq!(j.get("field").and_then(Json::as_str), Some("ops"));
        assert_eq!(j.get("limit").and_then(Json::as_u64), Some(EDIT_LAYOUT_CHARS as u64));
        assert_eq!(j.get("got").and_then(Json::as_u64), Some(c.chars as u64));
        let started = std::time::Instant::now();
        assert!(apply_edits(&s, &mixed).unwrap_err().contains("past the"));
        assert!(started.elapsed().as_millis() < 50, "refused before the work: {:?}", started.elapsed());
        // The most a call may do: changes and moves up to the budget on the longest text, then
        // the edit ends. It finishes, and makes no more layouts than the model said.
        let full = TextSpec::parse(&Json::obj([("text", Json::from(&words[..MAX_TEXT_BYTES])), ("max_width", Json::from(5.0)), ("editing", Json::obj([("caret", Json::from(0u64))]))])).unwrap();
        let mut ops = Vec::new();
        loop {
            let mut more = ops.clone();
            more.extend([EditOp::Backspace(Unit::Char), EditOp::Down { select: false }]);
            if edit_cost(&full, &more, true).chars > EDIT_LAYOUT_CHARS {
                break;
            }
            ops = more;
        }
        while ops.len() < MAX_EDIT_OPS {
            ops.push(EditOp::Down { select: false });
        }
        let c = edit_cost(&full, &ops, true);
        assert!(c.chars <= EDIT_LAYOUT_CHARS && c.layouts >= 3, "{c:?}");
        let started = std::time::Instant::now();
        let (_, r) = apply_edits(&full, &ops).unwrap();
        let took = started.elapsed();
        assert!(r.layouts < c.layouts, "{} layouts, the model said {}", r.layouts, c.layouts);
        // About 3 ms in release; a debug build is ~15x slower.
        assert!(took.as_millis() < 1000, "{took:?}");
        // A paste may bring the longest text (the window's clipboard holds anything).
        let paste = [EditOp::SelectAll, EditOp::Copy, EditOp::Paste, EditOp::Paste, EditOp::Up { select: false }];
        assert_eq!(edit_cost(&s, &paste, false).longest, MAX_TEXT_BYTES);
        let empty = TextSpec::parse(&Json::obj([("text", Json::from("")), ("editing", Json::obj([("caret", Json::from(0u64))]))])).unwrap();
        let mut pasted = vec![EditOp::Paste];
        for _ in 0..127 {
            pasted.extend([EditOp::Down { select: false }, EditOp::Backspace(Unit::Char)]);
        }
        assert!(edit_cost(&empty, &pasted, false).chars > EDIT_LAYOUT_CHARS, "review of 50a4772: an empty text and a 2 KB clipboard");
        // The window's frame is costed whole: every Ctrl+Z / Ctrl+Y ends a run that is laid out.
        let mut frame = Vec::new();
        for _ in 0..85 {
            frame.extend([EditOp::Insert("x".into()), EditOp::Down { select: false }, EditOp::Undo]);
        }
        let c = window_cost(&empty, &frame, false);
        assert!(c.chars > EDIT_LAYOUT_CHARS && c.layouts > 85, "{c:?}");
        assert_eq!(window_cost(&empty, &[EditOp::Insert("hi".into())], false), edit_cost(&empty, &[EditOp::Insert("hi".into())], false));
    }

    /// A text laid out before its CJK face arrived is laid out again by the loader thread (never
    /// on first use on the engine thread), and frames switch to that layout only at their start.
    #[test]
    fn a_later_layout_is_used_only_once_a_frame_adopted_it() {
        let s = spec(r#"{"text":"Here lies Ada"}"#).unwrap();
        let first = Laid { layout: s.laid().layout.clone(), ink: s.laid().ink };
        let mut w = s.clone();
        let later = OnceLock::new();
        let _ = later.set((Laid { layout: TextLayout::default(), ink: [9.0; 4] }, u64::MAX));
        w.laid = Arc::new(Layouts { first, complete: false, later, ..Layouts::default() });
        adopt_relayouts();
        assert_eq!(w.laid().ink, s.laid().ink, "a batch no frame adopted isn't used");
        let mut v = w.clone();
        let first = Laid { layout: s.laid().layout.clone(), ink: s.laid().ink };
        let later = OnceLock::new();
        let _ = later.set((Laid { layout: TextLayout::default(), ink: [9.0; 4] }, 0));
        v.laid = Arc::new(Layouts { first, complete: false, later, ..Layouts::default() });
        assert_eq!(v.laid().ink, [9.0; 4], "an adopted batch is");
        assert_eq!(v.bounds().0.x, 9.0, "bounds follow the same layout");
        // Copies share the layouts and split their bytes (the journal's ring isn't charged per copy).
        let alone = v.heap_bytes();
        let copy = v.clone();
        assert!(copy.heap_bytes() < alone && v.heap_bytes() == copy.heap_bytes());
    }

    /// Cost before work: the worst component the schema allows (2048 bytes of markup with as many
    /// spans as fit, the widest glyph per line, the most lines) is bounded by the caps alone and
    /// laid out once, on the caller's thread; drawing it makes no layout.
    #[test]
    fn the_largest_component_lays_out_once_within_its_cost() {
        let markup: String = "[b]W[/b][color=#3fe0d0]M[/color]".repeat(MAX_TEXT_BYTES / 32);
        assert!(markup.len() <= MAX_TEXT_BYTES);
        let worst = [
            Json::obj([("text", Json::from(markup.as_str())), ("rich", Json::Bool(true)), ("max_width", Json::from(0.01)), ("size", Json::from(100.0))]),
            Json::obj([("text", Json::from("\n".repeat(MAX_TEXT_BYTES).as_str())), ("max_lines", Json::from(MAX_TEXT_LINES as u64))]),
            Json::obj([("text", Json::from("W".repeat(MAX_TEXT_BYTES).as_str())), ("max_width", Json::from(0.01)), ("max_lines", Json::from(MAX_TEXT_LINES as u64))]),
        ];
        for v in worst {
            let started = std::time::Instant::now();
            let s = TextSpec::parse(&v).unwrap();
            let took = started.elapsed();
            // About 2 ms in release (one layout and one ink box).
            assert!(took.as_millis() < 250, "{took:?}");
            assert!(s.laid().layout.lines.len() <= MAX_TEXT_BYTES + 1);
            let mut g = TextGpu::default();
            g.begin_frame();
            let laid: *const Laid = s.laid();
            g.draws(1, &s, &Mat4::IDENTITY, [1.0; 4], &mut Vec::new());
            assert!(std::ptr::eq(laid, s.laid()));
            assert!(g.frame().glyphs.len() <= MAX_TEXT_BYTES);
        }
        // Over the caps: refused before any layout.
        assert!(TextSpec::parse(&Json::obj([("text", Json::from("x".repeat(MAX_TEXT_BYTES + 1).as_str()))])).unwrap_err().contains("at most"));
    }

    /// Layouts are made where the text is set and kept in the component: frames and bounds use
    /// that one (they can't disagree), and drawing makes none.
    #[test]
    fn frames_and_bounds_draw_the_components_own_layout() {
        let s = spec(r#"{"text":"Here lies Ada\nwho kept the lamps","size":0.5}"#).unwrap();
        let laid: *const Laid = s.laid();
        let mut g = TextGpu::default();
        let mut out = Vec::new();
        for _ in 0..3 {
            g.begin_frame();
            out.clear();
            g.draws(1, &s, &Mat4::IDENTITY, [1.0; 4], &mut out);
            assert!(std::ptr::eq(laid, s.laid()), "the same layout every frame");
        }
        // Every inked glyph of the component's layout, once (spaces have no ink).
        let inked = s.laid().layout.glyphs.iter().filter(|x| !s.plain()[x.byte as usize..].starts_with(char::is_whitespace)).count();
        assert_eq!(g.frame().glyphs.len(), inked);
        let (lo, hi) = s.bounds();
        assert_eq!([lo.x, lo.y, hi.x, hi.y], s.laid().ink);
        // Clones share it (the journal's copies cost no layout); a change makes a new one.
        let c = s.clone();
        assert!(std::ptr::eq(laid, c.laid()));
        let (e, _) = apply_edits(&s, &[EditOp::Insert("!".into())]).unwrap();
        assert!(!std::ptr::eq(laid, e.laid()));
        assert!(s.heap_bytes() > s.laid().layout.glyphs.len() * std::mem::size_of::<specula_text::PositionedGlyph>());
    }

    #[test]
    fn bad_ops_are_refused_with_their_index() {
        for (j, needle) in [
            (r#"[]"#, "1 to"),
            (r#"[{"key":"jump"}]"#, "ops[0]"),
            (r#"[{"insert":"a"},{"fly":1}]"#, "ops[1]"),
            (r#"[{"select":[1]}]"#, "anchor, caret"),
            (r#"[{"key":"left","unit":"page"}]"#, "unit"),
            (r#"[{"insert":"a","extra":1}]"#, "extra"),
            (r#"{"insert":"a"}"#, "array"),
        ] {
            let e = parse_ops(&Json::parse(j).unwrap()).unwrap_err();
            assert!(e.contains(needle), "{j}: {e}");
        }
        let many = format!("[{}]", vec![r#"{"key":"left"}"#; MAX_EDIT_OPS + 1].join(","));
        assert!(parse_ops(&Json::parse(&many).unwrap()).is_err());
    }


    /// Every distinct glyph the bundled UI face draws for Latin, Cyrillic and Greek: about 300
    /// glyphs, 200 KB of store words.
    fn many_glyphs() -> String {
        ('A'..='Z').chain('a'..='z').chain('\u{410}'..='\u{44F}').chain('\u{391}'..='\u{3A9}').chain('\u{3B1}'..='\u{3C9}').chain('0'..='9').chain("!?&@#%".chars()).collect()
    }

    /// Review of cd954bd, major 1: a capture prepares new glyphs within its budget (the rest
    /// are deferred, said in its packet, and drawn by the next capture), and a parked capture
    /// prepares ahead a pass at a time until its text is ready.
    #[test]
    fn captures_prepare_glyphs_within_their_budget_and_say_what_waits() {
        let s = TextSpec::parse(&Json::obj([("text", Json::from(many_glyphs().repeat(4).as_str())), ("max_width", Json::from(4.0))])).unwrap();
        let mut g = TextGpu::default();
        g.begin_frame();
        g.draws(1, &s, &Mat4::IDENTITY, [1.0; 4], &mut Vec::new());
        let prepared = g.store().prepared;
        let words = g.store().words().len();
        assert!(words <= CAPTURE_PREP_WORDS + specula_text::slug::MAX_GLYPH_CURVES * 16, "{words} words prepared in one capture");
        assert!(g.deferred > 0, "the rest wait for a later capture");
        let problem = g.frame_problem().expect("the packet says so");
        assert!(problem.contains("not drawn yet") && problem.contains("capture again"), "{problem}");
        // Captures converge: each prepares more, none prepares a glyph twice.
        let mut captures = 1;
        while g.deferred > 0 {
            g.begin_frame();
            g.draws(1, &s, &Mat4::IDENTITY, [1.0; 4], &mut Vec::new());
            captures += 1;
            assert!(captures < 20, "captures converge");
        }
        assert!(g.store().prepared > prepared && g.frame_problem().is_none());
        assert_eq!(g.store().prepared as usize, g.store().glyph_count(), "each glyph prepared once");
        // Parked: passes of the engine loop prepare ahead, within their budget, until ready.
        let mut g = TextGpu::default();
        let mut passes = 0;
        loop {
            let mut budget = CAPTURE_PREP_WORDS_PER_PASS;
            let before = g.store().words().len();
            let left = g.prepare_ahead(&s, &mut budget);
            assert!(g.store().words().len() - before <= CAPTURE_PREP_WORDS_PER_PASS + specula_text::slug::MAX_GLYPH_CURVES * 16);
            passes += 1;
            if left == 0 {
                break;
            }
            assert!(passes < 40);
        }
        assert!(passes > 1, "more than one pass's worth");
        g.begin_frame();
        g.draws(1, &s, &Mat4::IDENTITY, [1.0; 4], &mut Vec::new());
        assert_eq!((g.deferred, g.dropped), (0, 0), "ready: the capture draws it all");
    }

    /// Review of cd954bd, major 1 (the flicker): window frames of a scene larger than the
    /// largest store converge: after the store settles, the same glyphs draw every frame, none
    /// is prepared again and the store never starts over.
    #[test]
    fn window_frames_of_a_scene_larger_than_the_store_converge() {
        let s = TextSpec::parse(&Json::obj([("text", Json::from(many_glyphs().as_str())), ("max_width", Json::from(4.0))])).unwrap();
        let mut g = TextGpu { store: SlugStore::growing(16 * 1024, 64 * 1024), prep_words: Some(WINDOW_PREP_WORDS), ..TextGpu::default() };
        let mut last: Option<(usize, u64, u64)> = None;
        let mut steady = 0;
        let mut settled = None;
        for frame in 0..200 {
            g.begin_frame();
            g.draws(1, &s, &Mat4::IDENTITY, [1.0; 4], &mut Vec::new());
            let now = (g.frame().glyphs.len(), g.store().prepared, g.store().resets);
            match (settled, last) {
                // Once settled, every later frame is the same.
                (Some(at), Some(before)) => assert_eq!(now, before, "frame {frame} (settled at {at}): the same glyphs, nothing prepared again, no reset"),
                (None, Some(before)) if before == now && g.deferred == 0 => {
                    steady += 1;
                    if steady == 3 {
                        settled = Some(frame);
                    }
                }
                _ => steady = 0,
            }
            last = Some(now);
        }
        let at = settled.expect("it settles");
        assert!(at < 30, "settled at frame {at}");
        assert!(g.dropped > 0, "the scene is larger than the store (dropped, and said so)");
        assert!(g.frame_problem().unwrap().contains("dropped"));
        assert_eq!(g.store().cap_bytes(), 64 * 1024, "grown to its largest");
    }

    /// Review of cd954bd, major 4: a world file's texts are laid out by the load thread, never
    /// on the thread that loads them (the engine thread, for world_load), and frames adopt them
    /// between loop turns; captures wait for them (layout_pending).
    #[test]
    fn a_world_files_texts_are_laid_out_by_the_load_thread_not_where_they_load() {
        let words = "Here lies one whose name was writ in water and whose stone remembers more. ".repeat(30);
        let v = Json::obj([("text", Json::from(&words[..2000])), ("max_width", Json::from(4.0))]);
        let here = laid_here();
        let specs: Vec<TextSpec> = (0..2000).map(|_| crate::two_d::deferred(|| TextSpec::parse(&v)).unwrap()).collect();
        assert_eq!(laid_here(), here, "no layout on the loading thread");
        assert!(specs.iter().all(TextSpec::layout_pending));
        assert!(specs[0].laid().layout.glyphs.is_empty(), "nothing is drawn before its layout");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while specs.iter().any(|s| s.laid.loaded.get().is_none()) {
            assert!(std::time::Instant::now() < until, "the load thread lays them out ({} queued)", loads_queued());
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        adopt_relayouts();
        assert!(specs.iter().all(|s| !s.layout_pending()));
        let direct = TextSpec::parse(&v).unwrap();
        assert_eq!(specs[0].laid().layout.glyphs.len(), direct.laid().layout.glyphs.len());
        assert_eq!(specs[0].laid().ink, direct.laid().ink);
        assert_eq!(specs[1999].bounds(), direct.bounds());
    }

    /// Review R2 of m5-text, minor 2: a load past the queue's old size (65,536) still lays out
    /// none of its texts where it loads; the queue grows with the load.
    #[test]
    fn a_load_past_65536_texts_lays_out_none_where_it_loads() {
        let v = Json::obj([("text", Json::from("Here lies"))]);
        let here = laid_here();
        let inline = LAID_INLINE.load(Ordering::Acquire);
        let specs: Vec<TextSpec> = (0..70_000).map(|_| crate::two_d::deferred(|| TextSpec::parse(&v)).unwrap()).collect();
        assert_eq!(laid_here(), here, "no layout on the loading thread");
        assert_eq!(LAID_INLINE.load(Ordering::Acquire), inline, "the load thread took every one");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(300);
        while specs.iter().any(|s| s.laid.loaded.get().is_none()) {
            assert!(std::time::Instant::now() < until, "the load thread lays them out ({} queued)", loads_queued());
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// Review of cd954bd, major 3: ink boxes read each glyph's `glyf` header, not its outline:
    /// a text of 2048 distinct-ish glyphs gets its box in well under a millisecond's worth of
    /// work, and the box still holds every glyph's ink.
    #[test]
    fn ink_boxes_come_from_glyph_headers_and_hold_the_ink() {
        let s = TextSpec::parse(&Json::obj([("text", Json::from(many_glyphs().as_str())), ("max_width", Json::from(3.0)), ("size", Json::from(0.5))])).unwrap();
        let l = &s.laid().layout;
        let ink = s.laid().ink;
        let set = s.font_set();
        let k = s.k();
        for g in &l.glyphs {
            let font = set.face(g.face);
            let Some(b) = font.glyph_outline(g.glyph).unwrap().bounds() else { continue };
            let sc = font.scale_for_px(l.size_px);
            let p0 = s.plane(l, g.x + b[0] * sc, g.y - b[1] * sc);
            let p1 = s.plane(l, g.x + b[2] * sc, g.y - b[3] * sc);
            let eps = 1e-4 + k * 2.0;
            assert!(p0[0] >= ink[0] - eps && p1[0] <= ink[2] + eps && p0[1] >= ink[1] - eps && p1[1] <= ink[3] + eps, "glyph {} outside the ink box", g.glyph);
        }
    }

    #[test]
    fn locales_and_characters_pick_the_cjk_family() {
        assert_eq!(Cjk::of_locale("ja-JP"), Some(Cjk::Ja));
        assert_eq!(Cjk::of_locale("zh-TW"), Some(Cjk::ZhHant));
        assert_eq!(Cjk::of_locale("zh"), Some(Cjk::ZhHans));
        assert_eq!(Cjk::of_locale("en"), None);
        assert_eq!(Cjk::of_text("墓地のよる"), Some(Cjk::Ja));
        assert_eq!(Cjk::of_text("묘지"), Some(Cjk::Ko));
        assert_eq!(Cjk::of_text("墓地"), Some(Cjk::ZhHans));
        assert_eq!(Cjk::of_text("Здесь"), None);
    }

    #[test]
    fn draws_make_one_run_and_editing_adds_caret_and_selection() {
        let mut g = TextGpu::default();
        g.begin_frame();
        let s = spec(r#"{"text":"Rest in peace","size":0.2}"#).unwrap();
        let mut out = Vec::new();
        assert_eq!(g.draws(1, &s, &Mat4::IDENTITY, [1.0; 4], &mut out), None);
        assert_eq!(out.len(), 1);
        let Draw2d::Text(run) = out[0] else { panic!("a text run") };
        assert_eq!(run.count, 11, "11 inked glyphs (spaces have no outline)");
        let f = g.frame();
        assert_eq!(f.glyphs.len(), 11);
        assert!(f.glyphs.iter().all(|gl| (gl.word as usize) < f.words.len()));
        let e = spec(r#"{"text":"Rest in peace","editing":{"caret":7,"anchor":5}}"#).unwrap();
        out.clear();
        g.draws(2, &e, &Mat4::IDENTITY, [1.0; 4], &mut out);
        assert_eq!(out.iter().filter(|d| matches!(d, Draw2d::Sprite(_))).count(), 2, "one selection rect and the caret");
        // Review R4 of m5-text: in a window frame, `editing` with no window focus draws no caret
        // or selection (a builder's fake live field); the focused text keeps them.
        for (focus, marks) in [(None, 0), (Some(9), 0), (Some(2), 2)] {
            g.window_marks = Some(focus);
            out.clear();
            g.draws(2, &e, &Mat4::IDENTITY, [1.0; 4], &mut out);
            assert_eq!(out.iter().filter(|d| matches!(d, Draw2d::Sprite(_))).count(), marks, "window focus {focus:?}");
        }
        g.window_marks = None;
        // Rich colour replaces the entity's (sight-skeptic, 787f9fb: cyan on dark carved text came
        // out dark); the entity's opacity stays on the run.
        let r = spec(r#"{"text":"a [color=#3fe0d0]b[/color]","rich":true}"#).unwrap();
        out.clear();
        let first = g.frame().glyphs.len();
        g.draws(3, &r, &Mat4::IDENTITY, [0.2, 0.2, 0.2, 0.5], &mut out);
        let Some(Draw2d::Text(run)) = out.last().copied() else { panic!("a text run") };
        assert_eq!(run.color, [1.0, 1.0, 1.0, 0.5]);
        let gl = &g.frame().glyphs[first..];
        assert_eq!(gl[0].color, [0.2, 0.2, 0.2, 1.0], "plain glyphs take the entity's colour");
        assert!(gl[1].color[1] > 0.8 && gl[1].color[0] < 0.3, "the span's cyan: {:?}", gl[1].color);
    }
}
