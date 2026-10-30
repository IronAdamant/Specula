//! The flash watch: WCAG 2.3.1 (three flashes or below) checked on the final image itself, so
//! every source counts together: several effect instances, an entity's scale, a close camera,
//! lights, agents changing the world every tick, game code and the interface.
//!
//! **What it measures.** The final image is read whole (the window: `shaders/flash_*.frag` on the
//! GPU, read back a frame or two late; a capture: its own pixels on the CPU, every second pixel
//! across and down). Each texel of a half-resolution state (a 2x2 block) keeps its luminance and
//! red value at their last extreme and the direction it was moving, and makes a *transition* when
//! it moves back from an extreme:
//!
//! - **general:** relative luminance by at least 0.1, with the darker side under 0.8;
//! - **red:** `(R − G − B) × 320` (negatives as 0; R, G, B linear) by more than 20, while
//!   saturated red (`R / (R + G + B) ≥ 0.8`).
//!
//! The image is then cut into cells 32 pixels across (`CELL` pixels; partial at odd edges). Each
//! cell's mean luminance and red value (every pixel weighted exactly once) are watched the same
//! way, so a uniform strobe counts; and each cell counts a **pattern transition** in a frame where
//! at least a quarter of its pixels made one, so fine patterns that average out (a checkerboard
//! or stripes inverting, a red/green checker) count too. A flash is a pair of transitions; a cell
//! *fails* with more than three flashes in the last second. The frame fails when the failing
//! cells together cover the threshold's area: a quarter of a 10° field, which on the 1024 × 768
//! screen WCAG assumes is 341 × 256 pixels, so about 2.8% of the viewport (scaled to it).
//!
//! **Limits** (docs/design/VFX.md): the area is summed over the whole view, not any one 10°
//! field (stricter: it can fail where WCAG wouldn't); patterns finer than 2 pixels average out
//! in the half-resolution state (the window's red value is each 2x2 block's mean colour's);
//! captures read every second pixel.
//!
//! **Warn only, for now.** The report says how many flashes, where and how much area; agents see
//! it in `perf_frame`, `vfx_info` and sight packets, and an event is recorded when a failure
//! starts. A *protect* mode for shipped games (dimming the contributions in the failing cells)
//! is a seam: [`Mode::Protect`] is reserved (docs/design/VFX.md, an M6 shipping item).
//!
//! **Deterministic for captures:** a capture's state comes from its own pixels and its time is
//! the simulation's (`time_s`), and each view (up to [`MAX_VIEWS`], least recently used out
//! first) keeps its own history, so the same captures give the same report.

use specula_core::Json;
use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, Sender};

/// Seconds a count looks back.
const WINDOW_S: f64 = 1.0;
/// Transitions kept per cell and kind (more than enough to see over three flashes a second).
const KEEP: usize = 16;
/// The threshold's area as a share of the viewport: a quarter of 341 × 256 on 1024 × 768.
pub const AREA: f32 = 0.25 * (341.0 * 256.0) / (1024.0 * 768.0);
/// Pixels across one block of the window's readback, and one cell.
pub const BLOCK: u32 = 8;
pub const CELL: u32 = 32;
/// A cell makes a pattern transition when at least this share of its pixels made one.
pub const PATTERN_SHARE: f32 = 0.25;
/// Views whose capture histories are kept.
pub const MAX_VIEWS: usize = 8;
/// Events kept.
const MAX_EVENTS: usize = 32;
/// Seconds without a new window grid before its report is stale (minimised, hidden).
const STALE_S: f64 = 1.0;

/// What the watch does with a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Off,
    /// Report it (perf_frame, vfx_info, sight packets, an event).
    Warn,
    /// Reserved: dim what flashes (M6, for shipped games).
    Protect,
}

/// A frame's cells: mean relative luminance and red value, and the share of their pixels that
/// made a transition (general, red).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cells {
    pub width: u32,
    pub height: u32,
    pub lum: Vec<f32>,
    /// `(R − G − B)` while saturated red (× 320 is the WCAG red value), else 0.
    pub red: Vec<f32>,
    pub lum_events: Vec<f32>,
    pub red_events: Vec<f32>,
}

/// sRGB 8-bit to linear (a table; `dmath`, so the same bits everywhere).
fn linear(c: u8) -> f32 {
    static LUT: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        std::array::from_fn(|i| {
            let c = i as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                specula_core::dmath::pow((c + 0.055) / 1.055, 2.4)
            }
        })
    })[c as usize]
}

/// `(relative luminance, red value / 320)` of a linear colour (`shaders/flash.glsl`).
fn measure(r: f32, g: f32, b: f32) -> (f32, f32) {
    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let sum = r + g + b;
    let red = if sum > 0.0 && r / sum >= 0.8 { (r - g - b).max(0.0) } else { 0.0 };
    (lum, red)
}

/// A half float's value.
pub fn half(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from((h >> 10) & 0x1f);
    let frac = f32::from(h & 0x3ff);
    sign * match exp {
        0 => frac * (1.0 / 1024.0) * (1.0 / 16384.0),
        31 => f32::INFINITY,
        e => (1.0 + frac / 1024.0) * f32::from_bits(((e - 15 + 127) as u32) << 23),
    }
}

impl Cells {
    fn reset(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        let n = (width * height) as usize;
        for v in [&mut self.lum, &mut self.red, &mut self.lum_events, &mut self.red_events] {
            v.clear();
            v.resize(n, 0.0);
        }
    }

    /// Cells from the window's blocks (`shaders/flash_cells.frag`: per 8x8 block, its mean
    /// luminance and red value and its transition shares, each over its own pixels), each block
    /// weighted by its pixels: every pixel of the image counts exactly once in its cell.
    pub fn from_blocks(&mut self, halfs: &[u16], blocks: (u32, u32), extent: (u32, u32)) {
        let per = CELL / BLOCK;
        let (cw, ch) = (blocks.0.div_ceil(per), blocks.1.div_ceil(per));
        self.reset(cw, ch);
        let mut weight = vec![0.0f32; (cw * ch) as usize];
        for by in 0..blocks.1 {
            let rows = (extent.1 - (by * BLOCK).min(extent.1)).min(BLOCK);
            for bx in 0..blocks.0 {
                let cols = (extent.0 - (bx * BLOCK).min(extent.0)).min(BLOCK);
                let n = (rows * cols) as f32;
                let i = ((by * blocks.0 + bx) * 4) as usize;
                let Some(px) = halfs.get(i..i + 4) else { continue };
                let c = ((by / per) * cw + bx / per) as usize;
                self.lum[c] += half(px[0]) * n;
                self.red[c] += half(px[1]) * n;
                self.lum_events[c] += half(px[2]) * n;
                self.red_events[c] += half(px[3]) * n;
                weight[c] += n;
            }
        }
        self.normalise(&weight);
    }

    fn normalise(&mut self, weight: &[f32]) {
        for (i, w) in weight.iter().enumerate() {
            let k = 1.0 / w.max(1.0);
            self.lum[i] *= k;
            self.red[i] *= k;
            self.lum_events[i] *= k;
            self.red_events[i] *= k;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The per-texel state (the CPU mirror of `shaders/flash.glsl`, for captures)

/// A texel's state: luminance and red value at their last extremes and their directions.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Texel {
    lum: f32,
    lum_dir: i8,
    red: f32,
    red_dir: i8,
    started: bool,
}

fn counts_lum(a: f32, b: f32) -> bool {
    (a - b).abs() >= 0.1 && a.min(b) < 0.8
}

fn counts_red(a: f32, b: f32) -> bool {
    (a - b).abs() > 20.0 / 320.0
}

/// Moves extreme `ext` (direction `dir`) on to `v`; whether that was a transition.
fn step(ext: &mut f32, dir: &mut i8, v: f32, counts: fn(f32, f32) -> bool) -> bool {
    let before = *dir;
    match *dir {
        0 => {
            if counts(*ext, v) {
                *dir = if v > *ext { 1 } else { -1 };
                *ext = v;
            }
        }
        1 => {
            if v > *ext {
                *ext = v;
            } else if counts(*ext, v) {
                (*dir, *ext) = (-1, v);
            }
        }
        _ => {
            if v < *ext {
                *ext = v;
            } else if counts(*ext, v) {
                (*dir, *ext) = (1, v);
            }
        }
    }
    *dir != before
}

/// One view's per-texel state, at half resolution.
#[derive(Clone, Debug, Default)]
pub struct History {
    size: (u32, u32),
    texels: Vec<Texel>,
}

impl History {
    /// Moves every texel on from `rgba` (sRGB RGBA8, `width × height`, top row first; each
    /// texel reads the top-left pixel of its 2x2 block) and fills `cells` (32-pixel cells,
    /// every texel weighted by the pixels it stands for).
    pub fn update(&mut self, rgba: &[u8], width: u32, height: u32, cells: &mut Cells) {
        let (hw, hh) = (width.div_ceil(2), height.div_ceil(2));
        let fresh = self.size != (width, height);
        if fresh {
            self.size = (width, height);
            self.texels.clear();
            self.texels.resize((hw * hh) as usize, Texel::default());
        }
        let per = CELL / 2;
        let (cw, ch) = (hw.div_ceil(per), hh.div_ceil(per));
        cells.reset(cw, ch);
        let mut weight = vec![0.0f32; (cw * ch) as usize];
        for y in 0..hh {
            let rows = (height - 2 * y).min(2);
            for x in 0..hw {
                let n = ((width - 2 * x).min(2) * rows) as f32;
                let i = (((2 * y) * width + 2 * x) * 4) as usize;
                let (lum, red) = match rgba.get(i..i + 3) {
                    Some(p) => measure(linear(p[0]), linear(p[1]), linear(p[2])),
                    None => (0.0, 0.0),
                };
                let t = &mut self.texels[(y * hw + x) as usize];
                let (mut el, mut er) = (false, false);
                if t.started {
                    el = step(&mut t.lum, &mut t.lum_dir, lum, counts_lum);
                    er = step(&mut t.red, &mut t.red_dir, red, counts_red);
                } else {
                    *t = Texel { lum, red, started: true, ..Texel::default() };
                }
                let c = ((y / per) * cw + x / per) as usize;
                cells.lum[c] += lum * n;
                cells.red[c] += red * n;
                cells.lum_events[c] += if el { n } else { 0.0 };
                cells.red_events[c] += if er { n } else { 0.0 };
                weight[c] += n;
            }
        }
        cells.normalise(&weight);
    }
}

// ---------------------------------------------------------------------------------------------
// Counting

/// Transitions of one signal in one cell.
#[derive(Clone, Copy, Debug, Default)]
struct Track {
    started: bool,
    dir: i8,
    ext: f32,
    lo: f32,
    hi: f32,
    times: Ring,
}

/// The last [`KEEP`] transition times.
#[derive(Clone, Copy, Debug, Default)]
struct Ring {
    times: [f64; KEEP],
    next: usize,
    len: usize,
}

impl Ring {
    fn push(&mut self, t: f64) {
        self.times[self.next] = t;
        self.next = (self.next + 1) % KEEP;
        self.len = (self.len + 1).min(KEEP);
    }

    /// Flashes (pairs of transitions) in `(now − WINDOW_S, now]`.
    fn flashes(&self, now: f64) -> u32 {
        let n = (0..self.len).filter(|&k| self.times[(self.next + KEEP - 1 - k) % KEEP] > now - WINDOW_S).count();
        (n / 2) as u32
    }
}

impl Track {
    /// Feeds a cell's mean; `counts(a, b)` says whether going from `a` to `b` is a transition.
    fn feed(&mut self, v: f32, t: f64, counts: impl Fn(f32, f32) -> bool) {
        if !self.started {
            (self.started, self.ext, self.lo, self.hi) = (true, v, v, v);
            return;
        }
        match self.dir {
            0 => {
                self.lo = self.lo.min(v);
                self.hi = self.hi.max(v);
                if v > self.lo && counts(self.lo, v) {
                    (self.dir, self.ext) = (1, v);
                    self.times.push(t);
                } else if v < self.hi && counts(self.hi, v) {
                    (self.dir, self.ext) = (-1, v);
                    self.times.push(t);
                }
            }
            1 => {
                if v > self.ext {
                    self.ext = v;
                } else if counts(self.ext, v) {
                    (self.dir, self.ext) = (-1, v);
                    self.times.push(t);
                }
            }
            _ => {
                if v < self.ext {
                    self.ext = v;
                } else if counts(self.ext, v) {
                    (self.dir, self.ext) = (1, v);
                    self.times.push(t);
                }
            }
        }
    }
}

/// One cell's watch: its mean luminance and red value, and its pattern transitions.
#[derive(Clone, Copy, Debug, Default)]
struct CellWatch {
    lum: Track,
    red: Track,
    lum_pattern: Ring,
    red_pattern: Ring,
}

/// One frame's verdict.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Report {
    /// Time of the last frame fed (seconds: wall time for the window, simulation time for captures).
    pub time_s: f64,
    /// Frames fed in the last second.
    pub samples: u32,
    /// Most flashes any cell made in the last second (general, red), from its mean or its pattern.
    pub flashes: u32,
    pub red_flashes: u32,
    /// The most from patterns alone (texels flashing while the cell's mean stays).
    pub pattern_flashes: u32,
    /// Share of the viewport whose cells made more than three (general, red).
    pub area: f32,
    pub red_area: f32,
    /// A failure: either share reaches [`AREA`].
    pub over: bool,
    /// No new frame for a while (the window minimised or hidden): `over` is cleared.
    pub stale: bool,
    /// The failing cells' bounding box, in the grid (`[x, y, w, h]` of `grid`).
    pub region: Option<[u32; 4]>,
    pub grid: [u32; 2],
}

impl Report {
    pub fn to_json(&self) -> Json {
        let r3 = |v: f32| Json::from((v * 1000.0).round() / 1000.0);
        let mut j = Json::obj([
            ("over", Json::from(self.over)),
            ("flashes", Json::from(self.flashes)),
            ("pattern_flashes", Json::from(self.pattern_flashes)),
            ("area", r3(self.area)),
            ("red_flashes", Json::from(self.red_flashes)),
            ("red_area", r3(self.red_area)),
            ("threshold_area", r3(AREA)),
            ("samples_last_second", Json::from(self.samples)),
            ("grid", Json::arr(self.grid.map(Json::from))),
            ("time_s", Json::from((self.time_s * 1000.0).round() / 1000.0)),
        ]);
        if self.stale {
            j.set("stale", Json::from(true));
        }
        if let Some(r) = self.region {
            j.set("region", Json::arr(r.map(Json::from)));
        }
        j
    }
}

/// A recorded failure.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub seq: u64,
    pub source: &'static str,
    pub report: Report,
}

fn general(a: f32, b: f32) -> bool {
    counts_lum(a, b)
}

fn red(a: f32, b: f32) -> bool {
    counts_red(a, b)
}

/// The watch over one stream of frames (the window's, or one view's captures).
#[derive(Clone, Debug)]
pub struct Watch {
    pub source: &'static str,
    cells: Vec<CellWatch>,
    grid: [u32; 2],
    times: VecDeque<f64>,
    pub report: Report,
}

impl Watch {
    pub fn new(source: &'static str) -> Watch {
        Watch { source, cells: Vec::new(), grid: [0, 0], times: VecDeque::new(), report: Report::default() }
    }

    /// Forgets the history (a resized view, time going back).
    pub fn reset(&mut self) {
        self.cells.clear();
        self.times.clear();
        self.report = Report::default();
    }

    /// Feeds a frame's cells at time `t`; returns whether a failure started.
    pub fn feed(&mut self, cells: &Cells, t: f64) -> bool {
        let grid = [cells.width, cells.height];
        if grid != self.grid || self.times.back().is_some_and(|&last| t < last) {
            self.reset();
            self.grid = grid;
        }
        if self.times.back() == Some(&t) {
            return false; // the same moment again (a repeated capture): nothing new
        }
        let n = (grid[0] * grid[1]) as usize;
        self.cells.resize(n, CellWatch::default());
        for (i, c) in self.cells.iter_mut().enumerate() {
            c.lum.feed(cells.lum[i], t, general);
            c.red.feed(cells.red[i], t, red);
            if cells.lum_events[i] >= PATTERN_SHARE {
                c.lum_pattern.push(t);
            }
            if cells.red_events[i] >= PATTERN_SHARE {
                c.red_pattern.push(t);
            }
        }
        self.times.push_back(t);
        while self.times.front().is_some_and(|&f| f <= t - WINDOW_S) {
            self.times.pop_front();
        }
        let was = self.report.over;
        let per = 1.0 / n.max(1) as f32;
        let mut rep = Report { time_s: t, samples: self.times.len() as u32, grid, ..Report::default() };
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for (i, c) in self.cells.iter().enumerate() {
            let (pl, pr) = (c.lum_pattern.flashes(t), c.red_pattern.flashes(t));
            let fl = c.lum.times.flashes(t).max(pl);
            let fr = c.red.times.flashes(t).max(pr);
            rep.flashes = rep.flashes.max(fl);
            rep.red_flashes = rep.red_flashes.max(fr);
            rep.pattern_flashes = rep.pattern_flashes.max(pl.max(pr));
            if fl > 3 {
                rep.area += per;
            }
            if fr > 3 {
                rep.red_area += per;
            }
            if fl > 3 || fr > 3 {
                let (x, y) = (i as u32 % grid[0], i as u32 / grid[0]);
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
        if x0 != u32::MAX {
            rep.region = Some([x0, y0, x1 - x0 + 1, y1 - y0 + 1]);
        }
        rep.over = rep.area >= AREA || rep.red_area >= AREA;
        self.report = rep;
        rep.over && !was
    }
}

/// One view's captures: their watch and per-texel history. It travels to the worker that feeds
/// a capture's pixels into it ([`Feed`]) and on to the next capture of the view.
struct ViewState {
    watch: Watch,
    history: History,
}

impl ViewState {
    fn fresh() -> Box<ViewState> {
        Box::new(ViewState { watch: Watch::new("capture"), history: History::default() })
    }
}

/// Where a view's state is: here, or with the feed of its latest capture (it comes back, or is
/// handed on, through the receiver).
enum Baton {
    Here(Box<ViewState>),
    Out(Receiver<Pass>),
}

/// What a feed hands on: the state, or (a feed dropped before it ran) where the state will be.
enum Pass {
    State(Box<ViewState>),
    Follow(Receiver<Pass>),
}

/// One view's slot.
struct View {
    key: u64,
    used: u64,
    baton: Baton,
}

/// A finished capture feed, for the engine's report and events ([`FlashWatch::absorb`]).
struct Fed {
    report: Report,
    ms: f64,
    failed: bool,
}

/// One capture's feed, taken in order on the engine thread when the capture lands
/// ([`FlashWatch::feed_for`]) and run with its pixels on the worker that holds them
/// ([`Feed::run`]): the per-pixel work stays off the engine thread, and captures of one view are
/// fed in the order they landed (a feed waits for the previous one's state). Dropped unrun (a
/// refused or cancelled job), it hands the state on unchanged, so a later feed never waits for it.
pub struct Feed {
    from: Option<Source>,
    to: Sender<Pass>,
    t: f64,
    done: Sender<Fed>,
}

enum Source {
    Ready(Box<ViewState>),
    Wait(Receiver<Pass>),
}

/// How long a feed waits for the previous feed of its view: far past any feed's time (every
/// earlier feed was handed to a worker first; a dropped one hands on at once).
const HANDOVER_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

impl Feed {
    /// Feeds `rgba` (sRGB RGBA8, `width × height`) into its view's watch; the report.
    pub fn run(mut self, rgba: &[u8], width: u32, height: u32) -> Report {
        let started = std::time::Instant::now();
        let mut state = match self.from.take() {
            Some(Source::Ready(s)) => s,
            Some(Source::Wait(rx)) => wait_for(rx),
            None => ViewState::fresh(),
        };
        let fresh_moment = state.watch.report.time_s == self.t && state.watch.report.samples > 0;
        let failed = if fresh_moment {
            false
        } else {
            let mut cells = Cells::default();
            state.history.update(rgba, width, height, &mut cells);
            state.watch.feed(&cells, self.t)
        };
        let report = state.watch.report;
        let _ = self.to.send(Pass::State(state));
        let _ = self.done.send(Fed { report, ms: started.elapsed().as_secs_f64() * 1e3, failed });
        report
    }
}

impl Drop for Feed {
    fn drop(&mut self) {
        let _ = match self.from.take() {
            Some(Source::Ready(s)) => self.to.send(Pass::State(s)),
            Some(Source::Wait(rx)) => self.to.send(Pass::Follow(rx)),
            None => Ok(()),
        };
    }
}

/// The state handed on through `rx` (following dropped feeds); a fresh one if it was lost (a
/// feed that panicked).
fn wait_for(mut rx: Receiver<Pass>) -> Box<ViewState> {
    loop {
        match rx.recv_timeout(HANDOVER_WAIT) {
            Ok(Pass::State(s)) => return s,
            Ok(Pass::Follow(next)) => rx = next,
            Err(_) => return ViewState::fresh(),
        }
    }
}

/// The engine's watches: the window's and each view's captures, and the failures they saw.
pub struct FlashWatch {
    pub mode: Mode,
    pub window: Watch,
    views: Vec<View>,
    /// The last capture's report (any view), once its feed has run.
    pub capture: Report,
    pub events: VecDeque<Event>,
    seq: u64,
    uses: u64,
    cells: Cells,
    /// The window grid last fed (its frame number) and when.
    window_frame: u64,
    window_at: f64,
    /// CPU ms the last window and capture feeds took.
    pub window_ms: f64,
    pub capture_ms: f64,
    /// Capture feeds that ran on workers.
    fed: (Sender<Fed>, Receiver<Fed>),
}

impl Default for FlashWatch {
    fn default() -> Self {
        FlashWatch {
            mode: Mode::Warn,
            window: Watch::new("window"),
            views: Vec::new(),
            capture: Report::default(),
            events: VecDeque::new(),
            seq: 0,
            uses: 0,
            cells: Cells::default(),
            window_frame: 0,
            window_at: 0.0,
            window_ms: 0.0,
            capture_ms: 0.0,
            fed: std::sync::mpsc::channel(),
        }
    }
}

impl FlashWatch {
    /// The feed of a capture of view `view` (a hash of its camera and size) at simulation time
    /// `t`, to run where its pixels are ([`Feed::run`]); `None` when the watch is off. Each view
    /// keeps its own history (the least recently used of [`MAX_VIEWS`] goes).
    pub fn feed_for(&mut self, view: u64, t: f64) -> Option<Feed> {
        if self.mode == Mode::Off {
            return None;
        }
        self.uses += 1;
        let at = match self.views.iter().position(|v| v.key == view) {
            Some(i) => i,
            None => {
                if self.views.len() == MAX_VIEWS {
                    let oldest = self.views.iter().enumerate().min_by_key(|(_, v)| v.used).map(|(i, _)| i).unwrap_or(0);
                    self.views.swap_remove(oldest);
                }
                self.views.push(View { key: view, used: 0, baton: Baton::Here(ViewState::fresh()) });
                self.views.len() - 1
            }
        };
        let v = &mut self.views[at];
        v.used = self.uses;
        let (to, back) = std::sync::mpsc::channel();
        let from = match std::mem::replace(&mut v.baton, Baton::Out(back)) {
            Baton::Here(s) => Source::Ready(s),
            Baton::Out(mut rx) => loop {
                // Back already (the usual case: the last feed ran): no waiting on the worker.
                match rx.try_recv() {
                    Ok(Pass::State(s)) => break Source::Ready(s),
                    Ok(Pass::Follow(next)) => rx = next,
                    Err(std::sync::mpsc::TryRecvError::Empty) => break Source::Wait(rx),
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break Source::Ready(ViewState::fresh()),
                }
            },
        };
        Some(Feed { from: Some(from), to, t, done: self.fed.0.clone() })
    }

    /// Takes in the capture feeds that ran since: the latest report, and an event per failure.
    pub fn absorb(&mut self) {
        while let Ok(f) = self.fed.1.try_recv() {
            self.capture = f.report;
            self.capture_ms = f.ms;
            if f.failed {
                self.record("capture", f.report);
            }
        }
    }

    fn record(&mut self, source: &'static str, report: Report) {
        self.seq += 1;
        if self.events.len() == MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(Event { seq: self.seq, source, report });
    }

    /// Feeds the window's latest blocks (a frame or two late), at wall time `t`. Called every
    /// loop: without a new grid for a while (minimised), the report goes stale and `over` clears.
    pub fn feed_window(&mut self, grid: &specula_gpu::FlashGrid, t: f64) {
        if self.mode == Mode::Off {
            return;
        }
        if grid.frame == 0 || grid.frame == self.window_frame {
            if self.window_frame != 0 && t - self.window_at > STALE_S && !self.window.report.stale {
                self.window.report.over = false;
                self.window.report.stale = true;
                self.window.reset_counts();
            }
            return;
        }
        let started = std::time::Instant::now();
        self.window_frame = grid.frame;
        self.window_at = t;
        self.cells.from_blocks(&grid.halfs, (grid.width, grid.height), grid.extent);
        let cells = std::mem::take(&mut self.cells);
        if self.window.feed(&cells, t) {
            let r = self.window.report;
            self.record("window", r);
        }
        self.cells = cells;
        self.window_ms = started.elapsed().as_secs_f64() * 1e3;
    }

    /// [`FlashWatch::feed_for`] and [`Feed::run`] here, at once (tests: on the engine it could wait
    /// for a worker's feed of the same view).
    #[cfg(test)]
    pub fn feed_capture(&mut self, rgba: &[u8], width: u32, height: u32, t: f64, view: u64) -> Report {
        let Some(feed) = self.feed_for(view, t) else { return Report::default() };
        let report = feed.run(rgba, width, height);
        self.absorb();
        report
    }

    pub fn to_json(&self, unsupported: Option<&str>) -> Json {
        let mut j = Json::obj([
            ("mode", Json::from(match self.mode {
                Mode::Off => "off",
                Mode::Warn => "warn",
                Mode::Protect => "protect",
            })),
            ("window", self.window.report.to_json()),
            ("capture", self.capture.to_json()),
            ("views", Json::from(self.views.len())),
            ("cpu_ms", Json::obj([("window", Json::from((self.window_ms * 1000.0).round() / 1000.0)), ("capture", Json::from((self.capture_ms * 1000.0).round() / 1000.0))])),
            (
                "events",
                Json::arr(self.events.iter().rev().take(8).map(|e| {
                    let mut j = e.report.to_json();
                    j.set("seq", Json::from(e.seq));
                    j.set("source", Json::from(e.source));
                    j
                })),
            ),
        ]);
        if let Some(u) = unsupported {
            j.set("window_unsupported", Json::from(u));
        }
        j
    }
}

impl Watch {
    /// Forgets the counts but keeps the report (marked stale by the caller).
    fn reset_counts(&mut self) {
        self.cells.clear();
        self.times.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let c = f(x, y);
                v.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        v
    }

    /// Captures of `f(x, y, frame)` at 60 a second for 1.5 s: whether the watch failed, and its
    /// last report.
    fn run(w: u32, h: u32, f: impl Fn(u32, u32, u32) -> [u8; 3]) -> (bool, Report, usize) {
        let mut fw = FlashWatch::default();
        let mut over = false;
        for i in 0..90u32 {
            let px = frame(w, h, |x, y| f(x, y, i));
            fw.feed_capture(&px, w, h, f64::from(i) / 60.0, 1);
            over |= fw.capture.over;
        }
        (over, fw.capture, fw.events.len())
    }

    /// A 10 Hz on/off phase (3 frames on, 3 off at 60 a second).
    fn on(i: u32) -> bool {
        (i / 3).is_multiple_of(2)
    }

    #[test]
    fn a_big_strobe_fails_and_a_small_or_slow_one_doesnt() {
        let (w, h) = (320u32, 180u32);
        let (over, rep, events) = run(w, h, |x, y, i| if on(i) && x < 120 && y < 120 { [255; 3] } else { [20; 3] });
        assert!(over && rep.flashes >= 6 && events == 1, "{rep:?}");
        // Slow (2 a second): fine. Small (a 12 px square, under 2.8% of the view): fine.
        assert!(!run(w, h, |x, y, i| if (i / 15).is_multiple_of(2) && x < 120 && y < 120 { [255; 3] } else { [20; 3] }).0);
        assert!(!run(w, h, |x, y, i| if on(i) && x < 12 && y < 12 { [255; 3] } else { [20; 3] }).0);
        // A red strobe (red flash).
        let (over, rep, _) = run(w, h, |x, y, i| if on(i) && x < 120 && y < 120 { [255, 0, 0] } else { [20; 3] });
        assert!(over && rep.red_flashes >= 6, "{rep:?}");
    }

    /// Round 3: fine patterns average out in cell means; their texels don't.
    #[test]
    fn checkerboards_stripes_and_dots_that_invert_fail() {
        let (w, h) = (320u32, 180u32);
        let flip = |i: u32, a: bool| if a ^ on(i) { [255; 3] } else { [0; 3] };
        type Pattern = Box<dyn Fn(u32, u32, u32) -> [u8; 3]>;
        let cases: [(&str, Pattern); 4] = [
            ("8 px checkerboard", Box::new(move |x, y, i| flip(i, (x / 8 + y / 8) % 2 == 0))),
            ("4 px stripes", Box::new(move |x, _, i| flip(i, (x / 4) % 2 == 0))),
            ("4x4 dots", Box::new(move |x, y, i| flip(i, x % 8 < 4 && y % 8 < 4))),
            ("red/green checker", Box::new(move |x, y, i| if ((x / 8 + y / 8) % 2 == 0) ^ on(i) { [255, 0, 0] } else { [0, 120, 0] })),
        ];
        for (name, f) in cases {
            let (over, rep, _) = run(w, h, f);
            assert!(over && rep.pattern_flashes >= 6, "{name}: {rep:?}");
        }
        // A still checkerboard: no flashes.
        let (over, rep, _) = run(w, h, |x, y, _| if (x / 8 + y / 8) % 2 == 0 { [200; 3] } else { [40; 3] });
        assert!(!over && rep.flashes == 0, "{rep:?}");
    }

    /// Captures' feeds taken in landing order and run on other threads in any order (as encode
    /// workers run them) give exactly the reports of feeding in order; a feed dropped unrun (a
    /// refused or cancelled job) hands its view's state on unchanged; the engine's own report and
    /// events follow once absorbed.
    #[test]
    fn feeds_run_on_workers_keep_each_views_order() {
        let (w, h) = (64u32, 36u32);
        let px = move |i: u32| frame(w, h, |_, _| if on(i) { [255; 3] } else { [0; 3] });
        let t = |i: u32| f64::from(i) / 60.0;
        let mut seq = FlashWatch::default();
        let expected: Vec<Report> = (0..40).map(|i| seq.feed_capture(&px(i), w, h, t(i), 7)).collect();
        assert!(expected.last().unwrap().over, "the strobe fails: {:?}", expected.last());
        let mut fw = FlashWatch::default();
        let mut feeds: Vec<(u32, Feed)> = Vec::new();
        for i in 0..40 {
            feeds.push((i, fw.feed_for(7, t(i)).unwrap()));
            // A capture of another view between them, and one dropped unrun.
            drop(fw.feed_for(8, t(i)));
        }
        // The latest capture's feed starts first: each waits for the one before it.
        let threads: Vec<_> = feeds.into_iter().rev().map(|(i, f)| { let pixels = px(i); std::thread::spawn(move || (i, f.run(&pixels, w, h))) }).collect();
        let mut got: Vec<(u32, Report)> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        got.sort_by_key(|(i, _)| *i);
        assert_eq!(got.into_iter().map(|(_, r)| r).collect::<Vec<_>>(), expected, "the same reports as feeding in order");
        fw.absorb();
        assert_eq!(fw.events.len(), seq.events.len(), "failures become events once absorbed");
        // The view's state came back: the next feed doesn't wait.
        let next = fw.feed_for(7, t(40)).unwrap();
        assert!(matches!(next.from, Some(Source::Ready(_))), "its state is back");
    }

    #[test]
    fn views_keep_their_own_history_and_repeat_moments_change_nothing() {
        let (w, h) = (64u32, 36u32);
        let mut fw = FlashWatch::default();
        let white = frame(w, h, |_, _| [255, 255, 255]);
        let black = frame(w, h, |_, _| [0, 0, 0]);
        for i in 0..20 {
            let t = f64::from(i) * 0.05;
            fw.feed_capture(if i % 2 == 0 { &white } else { &black }, w, h, t, 7);
            // Another view captured in between doesn't reset view 7's history.
            fw.feed_capture(&white, w, h, t, 8);
        }
        let seven = fw.feed_capture(&black, w, h, 0.99, 7);
        assert!(seven.over, "{seven:?}");
        let again = fw.feed_capture(&black, w, h, 0.99, 7);
        assert_eq!(again, seven, "the same moment again is no new sample");
        // Least recently used views go first, past MAX_VIEWS.
        for v in 100..100 + MAX_VIEWS as u64 {
            fw.feed_capture(&white, w, h, 2.0, v);
        }
        assert!(fw.views.iter().all(|v| v.key != 7) && fw.views.len() == MAX_VIEWS);
    }

    /// Round 3: the window's cells weigh every pixel exactly once, at any size (blind rows
    /// came from halving blits that skipped rows at odd sizes).
    #[test]
    fn window_cells_weigh_every_pixel_once() {
        for (w, h) in [(1280u32, 720u32), (1920, 1080), (1366, 768), (333, 177), (1, 1), (7, 9), (1023, 767)] {
            // Pixel values from a hash; the blocks as `flash_cells.frag` makes them (each block's
            // own pixels), encoded as halves.
            let value = |x: u32, y: u32| ((x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(40_503)) % 1000) as f32 / 1000.0;
            let (bw, bh) = (w.div_ceil(BLOCK), h.div_ceil(BLOCK));
            let mut halfs = vec![0u16; (bw * bh * 4) as usize];
            for by in 0..bh {
                for bx in 0..bw {
                    let (mut sum, mut n) = (0.0f64, 0.0f64);
                    for y in by * BLOCK..((by + 1) * BLOCK).min(h) {
                        for x in bx * BLOCK..((bx + 1) * BLOCK).min(w) {
                            sum += f64::from(value(x, y));
                            n += 1.0;
                        }
                    }
                    halfs[((by * bw + bx) * 4) as usize] = specula_gpu::f16_bits((sum / n) as f32);
                }
            }
            let mut cells = Cells::default();
            cells.from_blocks(&halfs, (bw, bh), (w, h));
            // Every cell's mean is the exact mean of its own pixels (within half precision).
            for cy in 0..cells.height {
                for cx in 0..cells.width {
                    let (mut sum, mut n) = (0.0f64, 0.0f64);
                    for y in cy * CELL..((cy + 1) * CELL).min(h) {
                        for x in cx * CELL..((cx + 1) * CELL).min(w) {
                            sum += f64::from(value(x, y));
                            n += 1.0;
                        }
                    }
                    let got = f64::from(cells.lum[(cy * cells.width + cx) as usize]);
                    assert!((got - sum / n).abs() <= 2e-3, "{w}x{h} cell {cx},{cy}: {got} vs {}", sum / n);
                }
            }
            // The weight model: each pixel's weight in its cell is 1 / the cell's pixels (±5%).
            for cy in 0..cells.height {
                for cx in 0..cells.width {
                    let cell_px = (((cx + 1) * CELL).min(w) - cx * CELL) * (((cy + 1) * CELL).min(h) - cy * CELL);
                    let mut total = 0.0f64;
                    for by in (cy * CELL / BLOCK)..(((cy + 1) * CELL / BLOCK).min(bh)) {
                        for bx in (cx * CELL / BLOCK)..(((cx + 1) * CELL / BLOCK).min(bw)) {
                            let n = f64::from((((bx + 1) * BLOCK).min(w) - bx * BLOCK) * (((by + 1) * BLOCK).min(h) - by * BLOCK));
                            // A pixel's weight = (1 / n) within its block × n / cell pixels.
                            let weight = (1.0 / n) * n / f64::from(cell_px);
                            assert!((weight * f64::from(cell_px) - 1.0).abs() <= 0.05);
                            total += n;
                        }
                    }
                    assert_eq!(total as u32, cell_px, "{w}x{h}: every pixel of the cell in exactly one block");
                }
            }
        }
    }

    #[test]
    fn a_window_without_new_frames_goes_stale() {
        // A full-view strobe in the window's blocks (8x8 blocks of a 64x32 image).
        let mut fw = FlashWatch::default();
        let (bw, bh) = (8u32, 4u32);
        let mut grid = specula_gpu::FlashGrid { width: bw, height: bh, extent: (64, 32), ..Default::default() };
        for i in 0..40u64 {
            let v = specula_gpu::f16_bits(if i % 2 == 0 { 0.9 } else { 0.0 });
            grid.halfs = (0..bw * bh).flat_map(|_| [v, 0, 0, 0]).collect();
            grid.frame = i + 1;
            fw.feed_window(&grid, i as f64 * 0.05);
        }
        assert!(fw.window.report.over && !fw.window.report.stale);
        // Minimised: the same grid for over a second.
        fw.feed_window(&grid, 40.0 * 0.05 + 1.5);
        assert!(!fw.window.report.over && fw.window.report.stale, "{:?}", fw.window.report);
    }

    #[test]
    fn halves_decode() {
        for v in [0.0f32, 1.0, 0.5, 0.123, 7.25, -2.0, 1e-5] {
            let h = specula_gpu::f16_bits(v);
            assert!((half(h) - v).abs() <= v.abs() * 1e-3 + 1e-6, "{v} -> {}", half(h));
        }
    }
}
