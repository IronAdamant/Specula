//! When the windowed loop serves agents and when it starts the next frame.
//!
//! FIFO presentation blocks in `present` until the display takes the frame. The loop used to
//! drain the request queue once per frame and then block there, so an agent that waits for each
//! reply before its next call got one call per refresh (60 a second on a 60 Hz display), and a
//! swarm's queue could hold a frame for seconds. Now the loop serves requests as they arrive
//! (waking on each one) for a *serve window* after every present, then builds and presents its
//! frame.
//!
//! **Sizing the window.** Each present reports how long it blocked; with the time served and the
//! gap since the previous present, that gives the frame's own work (`gap - served - blocked`). The
//! next window is the frame period minus that work (a decaying maximum, so spikes count) minus a
//! margin, so `present` still blocks about the margin: it absorbs frame work that varies and
//! timer wake-ups that come late. The margin is a quarter of the refresh interval (0.5 to 3 ms)
//! times a caution that each late frame raises (×1.5, at most ×4) and on-time frames lower.
//!
//! **The frame period** is the display's refresh interval while agents aren't queueing: the rate
//! the OS reports for the window's display (asked every second; variable-refresh displays report
//! their highest). Where it doesn't say, probes measure it: every 8th frame (and the first few)
//! gets the smallest window, and the median of the last 7 probes' gaps since the previous present
//! is the interval. (Measuring every gap instead would let an estimate keep itself too long:
//! windows sized for it stretch the gaps.)
//!
//! **Under a backlog** (at least two calls still queued as windows end, four windows running) the
//! loop presents on every n-th vblank, n the most that keeps the *frame floor* (60 fps by default:
//! 4 at 240 Hz, 1 at 60 Hz), and agents get the rest of each period, at least a quarter of it. The
//! floor is a setting ([`Schedule::set_frame_floor`]; `perf.toml` `[swarm] frame_floor_fps` and
//! the `swarm_settings` tool): the lead agent lowers it before deploying a swarm on a strong
//! machine (frames traded for agent throughput) and keeps it at 60 on a laptop. A present usually finds a free
//! image then and returns at once, so with n > 1 the window is steered by feedback instead: each
//! frame it moves half the way to calling `present` a margin before the period is up (an early
//! frame waits in the swapchain's queue for its vblank). One sequential agent never has two calls
//! queued, so however fast it is, the human's window keeps the display's full rate.
//!
//! **The frame cap** ([`Schedule::set_max_fps`]; `--max-fps`, `perf.toml` `[window] max_fps`)
//! presents at most that often, on the fewest whole vblanks that keep under it (60 fps: every 4th
//! at 240 Hz, every 3rd at 144 Hz), steered like a backlog's skipped vblanks; agents get the rest
//! of each period and the GPU idles in it. Off by default (every vblank). It wins over the floor.
//!
//! **Late frames** (the gap over the period plus half a refresh) raise the caution. A late frame
//! whose present blocked over a refresh was submitted before its vblank and skipped by the display
//! or compositor (it happens without any serving, e.g. on hybrid-GPU laptops): it isn't counted
//! against the window.
//!
//! **Calls.** A call starts only as the first of a window or when the window still has room for
//! what calls of its [`Class`] cost lately: a decaying maximum per class, which halves every
//! 50 ms down to a floor (50 µs for reads and edits; a capture's guess for renders, disk and big
//! work), so one slow call holds back neither other classes nor its own for long. A call that doesn't fit waits, at the head of the queue, for the next window. So a
//! capture lands where the old loop ran it (right after a present) rather than pushing a frame
//! past its vblank. The order calls are applied in never changes: held calls are the next handled.
//!
//! **Waiting.** A timed wait ends on the OS timer tick after its timeout (1 ms ticks with
//! `FineTimer`), and std rounds timeouts up to whole milliseconds, but a call wakes a wait at
//! once. So a window waits in whole milliseconds that end by its deadline plus half the base
//! margin (the caution left out, or a raised margin would let waits run later and frames be late
//! again); once less than that is left it takes only what has already arrived and ends. (Waiting
//! a last millisecond for an agent's next call was tried: the call often came too late to fit,
//! yet still pushed the frame back.) Never busy-waits.
//!
//! **Step jobs waiting on the GPU** ([`Schedule::serve_with`]). The frame pumps the step jobs
//! once (docs/design/WORKERS.md). A job whose render is still on the GPU waits, and a job looked
//! at only once a frame lands one render a frame: with a window held to 20 frames a second, a
//! `perf_light` report took 3.7 times as long as it did when the job spun on the fence (light
//! review 4). So while one waits on its render (or has just landed it and has more to do), the
//! window also *pulses*: it looks at the step jobs about every millisecond, between calls, in its
//! own whole-millisecond waits (never a busy wait). Any look may find a render landed, so a pulse
//! is charged what a *landing* costs: the last landing's cost, or the estimate before it less a
//! quarter when that is more, kept apart from what a look costs and never forgotten with time
//! (light review 5: an estimate that halved every 50 ms had forgotten a landing by the next one
//! when renders were slower than that, and a landing began with room for a look only). A pulse
//! starts only when that fits before the deadline, so a landing in a pulse ends by it unless it
//! costs more than the last ones did. Until a landing of the queued jobs has been timed (by the
//! frame's own pump, as every landing was before pulses) no pulse runs, and once no step job is
//! queued what a landing costs is forgotten. The frame's own pump ([`Schedule::frame_pump`])
//! leaves alone the jobs the pulses are for when its window pulsed, so a frame does not land a
//! render in its own time that the next window can land in its (light review 5: held to 20
//! frames a second a frame paid for two landings). Pulses stop as soon as no job waits on the
//! GPU, and with none waiting a window makes no wake it would not have made.
//!
//! Time comes from the [`Inbox`] the calls come from: real time in the engine, a simulated clock
//! in the tests, which run the whole loop against a simulated display deterministically.

use specula_core::Json;
use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};

/// The margin (how long `present` should still block) is a quarter of the refresh interval,
/// within these bounds, times the caution.
pub const MIN_MARGIN: Duration = Duration::from_micros(500);
pub const MAX_MARGIN: Duration = Duration::from_millis(3);
/// The caution's ceiling (the margin's largest multiple).
const MAX_CAUTION: f32 = 4.0;
/// Per on-time frame, the caution keeps this much of its excess over 1 (half-life ~34 frames).
const CAUTION_KEEP: f32 = 0.98;
/// The smallest serve window: agents get at least this much of every frame, even when frames
/// take longer than a refresh.
pub const MIN_SERVE: Duration = Duration::from_millis(1);
/// The largest serve window.
pub const MAX_SERVE: Duration = Duration::from_millis(50);
/// The serve window while the window is minimized (nothing is presented).
pub const MINIMIZED_SERVE: Duration = Duration::from_millis(16);
/// Under a backlog, frames come at least this often, on whole vblanks: the frame floor's default
/// (60 fps) and its range. Below 20 fps a human can't follow the window well enough to step in
/// (F5), and the serve window is at most [`MAX_SERVE`] anyway.
pub const DEFAULT_FRAME_FLOOR_FPS: u32 = 60;
pub const MIN_FRAME_FLOOR_FPS: u32 = 20;
pub const MAX_FRAME_FLOOR_FPS: u32 = 240;
/// The frame cap's range (`--max-fps`, `perf.toml` `[window] max_fps`).
pub const MIN_MAX_FPS: u32 = 20;
pub const MAX_MAX_FPS: u32 = 1000;
/// Windows ending with a backlog, in a row, that make one (and as many clear ones end it).
const BUSY_AFTER: u8 = 4;
/// Calls queued as a window ends that make it end with a backlog (one sequential agent has at
/// most one call queued).
const BACKLOG_CALLS: usize = 2;
/// A class's cost estimate halves this often (down to its floor).
const COST_HALF_LIFE: Duration = Duration::from_millis(50);
/// The refresh interval assumed before two presents have been timed.
const REFRESH_GUESS: Duration = Duration::from_micros(16_667);
/// Shorter gaps between presents are ignored (a burst after a stall).
const MIN_GAP: Duration = Duration::from_millis(2);
/// Longer gaps between presents (a minimized or stalled window) are ignored.
const MAX_GAP: Duration = Duration::from_millis(100);
/// How late a timed wait may end after its timeout (one tick of a 1 ms system timer).
const TIMER_SLOP: Duration = Duration::from_millis(1);
/// Probes' gaps kept for the refresh interval (their median).
const PROBES: usize = 7;
/// Without a backlog, every this many frames one is a probe: the smallest window, so its gap is
/// the display's own interval (or the frame's, when that is longer).
const PROBE_EVERY: u32 = 8;
/// Every frame is a probe until this many have been measured.
const FIRST_PROBES: usize = 3;
/// While a step job waits on its render, the window looks at the step jobs this often (waits
/// are whole milliseconds and end up to a timer tick late: every one to two milliseconds).
pub const PULSE_EVERY: Duration = Duration::from_millis(1);
/// What a look (a pump in which every job only waited) is assumed to cost before one has been
/// timed, and the least its estimate decays to.
const LOOK_GUESS: Duration = Duration::from_micros(200);
const LOOK_FLOOR: Duration = Duration::from_micros(50);

/// The windowed loop's side of a frame: what its serve window serves and its pumps step. The
/// engine's (`run_windowed`'s) is a few lines that each call one thing; the
/// tests' is a simulation. What decides when a pulse runs, with which deadline, and whether the
/// frame's own pump steps a job is the schedule's ([`Schedule::serve_frame`],
/// [`Schedule::frame_pump`]), so it is tested as the engine runs it.
pub trait Window<R> {
    /// What a call costs, roughly.
    fn class(&self, call: &R) -> Class;
    /// Applies a call.
    fn call(&mut self, call: R);
    /// One pass of the step pump. `leave_pulsed`: leave alone the jobs pulses are for (a job
    /// with a render on the GPU, or one that has just landed it and has more to do). Answers
    /// whether a job did more than wait (a render landed, a slice ran): a landing's cost.
    fn pump(&mut self, leave_pulsed: bool) -> bool;
    /// Whether a queued step job has a render on the GPU, or has just landed one and has more to
    /// do (`Steps::gpu_waiting`): what pulses are for.
    fn gpu_waiting(&self) -> bool;
    /// Whether any step job is queued.
    fn steps_queued(&self) -> bool;
    /// Whether to stop serving (checked between calls).
    fn stop(&mut self) -> bool;
}

/// The median of `v` (not empty, at most [`PROBES`] long).
fn median(v: &[Duration]) -> Duration {
    let mut sorted = [Duration::ZERO; PROBES];
    let sorted = &mut sorted[..v.len()];
    sorted.copy_from_slice(v);
    sorted.sort_unstable();
    sorted[v.len() / 2]
}

/// The longest period that still gives `fps` frames a second.
fn floor_period(fps: u32) -> Duration {
    Duration::from_nanos(1_000_000_000 / u64::from(fps.clamp(MIN_FRAME_FLOOR_FPS, MAX_FRAME_FLOOR_FPS)))
}

/// `d` rounded down to whole milliseconds.
fn whole_ms(d: Duration) -> Duration {
    Duration::from_millis(d.as_millis() as u64)
}

/// `d` rounded up to whole milliseconds, at least one.
fn whole_ms_up(d: Duration) -> Duration {
    Duration::from_millis((d.as_nanos().div_ceil(1_000_000) as u64).max(1))
}

/// A cost kept as the longest lately (`(cost, measured at)`), halving every
/// [`COST_HALF_LIFE`] since it was measured, never under `floor`.
fn decayed((cost, at): (Duration, Option<Instant>), floor: Duration, now: Instant) -> Duration {
    let age = at.map_or(Duration::ZERO, |at| now.saturating_duration_since(at));
    let halvings = age.as_secs_f64() / COST_HALF_LIFE.as_secs_f64();
    cost.mul_f64(0.5f64.powf(halvings.min(64.0))).max(floor)
}

/// What a call costs, roughly: each class's recent cost is tracked apart, so a slow call holds
/// back only calls like it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Reads: queries, listings, history, info, perf.
    Read,
    /// Edits: spawns, sets, deletes, small batches, undo, claims, environment, the window camera.
    Edit,
    /// Renders: captures and every other sight call.
    Render,
    /// Disk: saves, loads, model builds and reloads.
    Disk,
    /// Big work: large batches, a journal replay check, a decision applied (`review_decide`,
    /// `gallery_apply`: several project files checked, committed and reloaded in one call).
    Big,
    /// Simulation: stepping the clock.
    Sim,
    /// The hosted game's own tools.
    Game,
}

const CLASSES: usize = 7;

impl Class {
    const ALL: [Class; CLASSES] = [Class::Read, Class::Edit, Class::Render, Class::Disk, Class::Big, Class::Sim, Class::Game];

    /// What a call of this class is assumed to cost before one has been timed.
    fn guess(self) -> Duration {
        match self {
            Class::Read | Class::Edit => Duration::from_micros(200),
            Class::Game => Duration::from_micros(500),
            Class::Sim => Duration::from_millis(1),
            Class::Render | Class::Disk | Class::Big => Duration::from_millis(8),
        }
    }

    /// The least its estimate decays to: 50 µs for the small classes (what reads and edits
    /// really cost), the guess for renders, disk and big work (rare, so a long-ago capture
    /// still counts as one).
    fn floor(self) -> Duration {
        match self {
            Class::Read | Class::Edit | Class::Game | Class::Sim => Duration::from_micros(50),
            Class::Render | Class::Disk | Class::Big => self.guess(),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Class::Read => "read",
            Class::Edit => "edit",
            Class::Render => "render",
            Class::Disk => "disk",
            Class::Big => "big",
            Class::Sim => "sim",
            Class::Game => "game",
        }
    }
}

/// Where calls come from, and the clock they are timed by.
pub trait Inbox<R> {
    fn now(&self) -> Instant;
    fn try_recv(&mut self) -> Result<R, TryRecvError>;
    fn recv_timeout(&mut self, timeout: Duration) -> Result<R, RecvTimeoutError>;
}

/// The engine's: a channel, and real time.
impl<R> Inbox<R> for &Receiver<R> {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn try_recv(&mut self) -> Result<R, TryRecvError> {
        Receiver::try_recv(self)
    }

    fn recv_timeout(&mut self, timeout: Duration) -> Result<R, RecvTimeoutError> {
        Receiver::recv_timeout(self, timeout)
    }
}

/// Why [`Schedule::serve`] returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Served {
    /// The window ended (calls may be held for the next one).
    Window,
    /// `stop` said so.
    Stopped,
    /// Every sender is gone.
    Disconnected,
}

/// The windowed loop's serve window and what it learned about the frames and the calls.
#[derive(Clone, Copy, Debug)]
pub struct Schedule {
    /// The next window, and the size frames call for (`serve` is smaller on a probe).
    serve: Duration,
    sized: Duration,
    /// The frame period `sized` was computed for.
    sized_for: Duration,
    /// Per [`Class`]: the longest call lately and when it was measured (see [`Schedule::cost`]).
    costs: [(Duration, Option<Instant>); CLASSES],
    /// What a look cost lately (a pump in which every job only waited), kept as a class's is.
    look_cost: (Duration, Option<Instant>),
    /// What a landing costs ([`Schedule::landed`]): what every pulse is charged (`None`: no
    /// landing of this run of step jobs has been timed yet).
    landing_cost: Option<Duration>,
    /// The last presented frame's number (presents that showed nothing don't count) and when it
    /// was presented.
    last_frame: (u64, Option<Instant>),
    /// The display's refresh interval as the OS reports it; else `refresh`, the [`median`] of the
    /// probes' gaps (`None` before the first).
    display: Option<Duration>,
    refresh: Option<Duration>,
    probes: [Duration; PROBES],
    probe_count: usize,
    /// The coming frame is a probe; frames since the last probe.
    probing: bool,
    since_probe: u32,
    /// A frame's own work: the longest lately (decays by 1/16 per frame).
    work: Duration,
    /// The margin's multiple, 1 to [`MAX_CAUTION`].
    caution: f32,
    /// Under a backlog, frames come at least this often (the frame floor, as a period).
    busy_frame: Duration,
    /// The frame cap as a period: frames come at most this often (`None`: every vblank).
    cap_frame: Option<Duration>,
    /// Windows that ended with a backlog, counted up to `2 * BUSY_AFTER` and down by clear
    /// ones; a backlog from [`BUSY_AFTER`].
    pressure: u8,
    /// Since start: serve windows, calls handled in them, calls held for the next window, late
    /// presents, the late ones that were ready in time (the display skipped them), presents that
    /// didn't block, and windows under a backlog.
    windows: u64,
    calls: u64,
    held: u64,
    late_presents: u64,
    skipped_presents: u64,
    unblocked_presents: u64,
    busy_windows: u64,
    /// Since start: pulses run (step jobs looked at within a window), and the pulses that had no
    /// room left before a deadline.
    pulses: u64,
    pulses_left_out: u64,
    /// A pulse ran in this frame's window: the frame's own pump leaves alone the jobs pulses are
    /// for.
    pulsed: bool,
    /// Since start: frame pumps that left those jobs alone; landings (in pulses and in frame
    /// pumps), the last one's cost and the most one cost.
    frame_pumps_left: u64,
    landings: u64,
    landing_last: Duration,
    landing_most: Duration,
}

impl Default for Schedule {
    fn default() -> Schedule {
        Schedule {
            serve: MIN_SERVE,
            sized: MIN_SERVE,
            sized_for: REFRESH_GUESS,
            costs: Class::ALL.map(|c| (c.guess(), None)),
            look_cost: (LOOK_GUESS, None),
            landing_cost: None,
            last_frame: (0, None),
            display: None,
            refresh: None,
            probes: [Duration::ZERO; PROBES],
            probe_count: 0,
            probing: true,
            since_probe: 0,
            work: Duration::ZERO,
            caution: 1.0,
            busy_frame: floor_period(DEFAULT_FRAME_FLOOR_FPS),
            cap_frame: None,
            pressure: 0,
            windows: 0,
            calls: 0,
            held: 0,
            late_presents: 0,
            skipped_presents: 0,
            unblocked_presents: 0,
            busy_windows: 0,
            pulses: 0,
            pulses_left_out: 0,
            pulsed: false,
            frame_pumps_left: 0,
            landings: 0,
            landing_last: Duration::ZERO,
            landing_most: Duration::ZERO,
        }
    }
}

impl Schedule {
    /// perf_frame's `serve` section.
    pub fn to_json(self) -> Json {
        let ms = |d: Duration| Json::from((d.as_secs_f64() * 1e6).round() / 1e3);
        let now = Instant::now();
        Json::obj([
            ("window_ms", ms(self.sized)),
            ("frame_period_ms", ms(self.period())),
            ("refresh_ms", ms(self.refresh())),
            ("refresh_from", Json::from(if self.display.is_some() { "display" } else if self.refresh.is_some() { "probes" } else { "guess" })),
            ("frame_work_ms", ms(self.work)),
            ("margin_ms", ms(self.margin())),
            ("backlog", Json::from(self.busy())),
            ("frame_floor_fps", Json::from(self.frame_floor_fps())),
            ("max_fps", self.max_fps().map_or(Json::Null, Json::from)),
            ("call_cost_ms", Json::obj(Class::ALL.map(|c| (c.name(), ms(self.cost(c, now)))))),
            ("windows", Json::from(self.windows)),
            ("backlog_windows", Json::from(self.busy_windows)),
            ("calls", Json::from(self.calls)),
            ("held_for_next_window", Json::from(self.held)),
            ("late_presents", Json::from(self.late_presents)),
            ("late_but_ready", Json::from(self.skipped_presents)),
            ("unblocked_presents", Json::from(self.unblocked_presents)),
            ("step_pulses", Json::from(self.pulses)),
            ("step_pulses_left_out", Json::from(self.pulses_left_out)),
            ("step_landing_ms", self.landing_cost.map_or(Json::Null, ms)),
            ("step_look_ms", ms(self.look_cost_at(now))),
            ("step_landings", Json::from(self.landings)),
            ("step_landing_last_ms", ms(self.landing_last)),
            ("step_landing_most_ms", ms(self.landing_most)),
            ("step_frame_pumps_left", Json::from(self.frame_pumps_left)),
        ])
    }

    /// Sets the frame floor: under a backlog the window still gets at least `fps` frames a second
    /// (clamped to [`MIN_FRAME_FLOOR_FPS`]..=[`MAX_FRAME_FLOOR_FPS`]; a floor over the display's
    /// rate means every vblank).
    pub fn set_frame_floor(&mut self, fps: u32) {
        self.busy_frame = floor_period(fps);
    }

    /// The frame floor in frames a second.
    pub fn frame_floor_fps(&self) -> u32 {
        (1_000_000_000 / self.busy_frame.as_nanos().max(1)) as u32
    }

    /// Sets the frame cap: the window presents at most `fps` frames a second (clamped to
    /// [`MIN_MAX_FPS`]..=[`MAX_MAX_FPS`]), on whole vblanks: the fewest that keep under it (60 at
    /// 240 Hz: every 4th; at 144 Hz: every 3rd, 48 fps). `None`: every vblank. The time saved goes
    /// to agents' calls, and the GPU idles in it (F3: a laptop's iGPU shares its power and memory
    /// with the CPU and the desktop, so rendering frames nobody needs costs the frames that count).
    pub fn set_max_fps(&mut self, fps: Option<u32>) {
        self.cap_frame = fps.map(|fps| Duration::from_nanos(1_000_000_000 / u64::from(fps.clamp(MIN_MAX_FPS, MAX_MAX_FPS))));
    }

    /// The frame cap in frames a second (`None`: none).
    pub fn max_fps(&self) -> Option<u32> {
        self.cap_frame.map(|p| (1_000_000_000 / p.as_nanos().max(1)) as u32)
    }

    /// The current serve window.
    pub fn window(&self) -> Duration {
        self.serve
    }

    fn refresh(&self) -> Duration {
        self.display.or(self.refresh).unwrap_or(REFRESH_GUESS)
    }

    /// The display's refresh interval as known so far (reported by the OS, or measured), and
    /// where it came from (`display`, `probes`); `None` before any frame.
    pub fn refresh_known(&self) -> Option<(Duration, &'static str)> {
        match (self.display, self.refresh) {
            (Some(d), _) => Some((d, "display")),
            (None, Some(r)) => Some((r, "probes")),
            _ => None,
        }
    }

    /// The display's refresh interval as the OS reports it (`None`: unknown, so probes measure
    /// it). Ask now and then: the window may move to another display.
    pub fn set_display_refresh(&mut self, interval: Option<Duration>) {
        self.display = interval.filter(|d| (MIN_GAP..=MAX_GAP).contains(d));
    }

    /// Agents keep calls queued: frames come every [`Schedule::period`], not every vblank.
    pub fn busy(&self) -> bool {
        self.pressure >= BUSY_AFTER
    }

    /// The time between presents the window is sized for: the refresh interval, or under a
    /// backlog the most whole refreshes that keep the frame floor; with a frame cap, at least the
    /// fewest whole refreshes that keep under it (the cap wins over the floor).
    pub fn period(&self) -> Duration {
        let refresh = self.refresh();
        // A little slack, so 240 Hz measured at 4.17 ms still gives 4 at 60 fps; small enough
        // (1/256 of a refresh) that no whole-hertz display up to 360 Hz falls below the floor
        // (1/8 let a floor of 52 at 100 Hz give 50 fps).
        let slack = refresh / 256;
        let mut n = if self.busy() { (self.busy_frame + slack).as_nanos() / refresh.as_nanos() } else { 1 };
        if let Some(cap) = self.cap_frame {
            n = n.max(cap.saturating_sub(slack).as_nanos().div_ceil(refresh.as_nanos()));
        }
        refresh * n.max(1) as u32
    }

    /// How long `present` should still block: a quarter of the refresh interval (0.5 to 3 ms)
    /// times the caution, at most a refresh.
    pub fn margin(&self) -> Duration {
        self.base_margin().mul_f32(self.caution).min(self.refresh())
    }

    /// The margin without the caution: a quarter of the refresh interval, 0.5 to 3 ms.
    fn base_margin(&self) -> Duration {
        (self.refresh() / 4).clamp(MIN_MARGIN, MAX_MARGIN)
    }

    /// What a call of `class` is expected to cost at `now`: the longest lately, halving every
    /// [`COST_HALF_LIFE`] since it was measured, never under the class's floor.
    fn cost(&self, class: Class, now: Instant) -> Duration {
        decayed(self.costs[class as usize], class.floor(), now)
    }

    /// A call of `class` took `took`, measured at `now`.
    fn costed(&mut self, class: Class, took: Duration, now: Instant) {
        let decayed = self.cost(class, now);
        self.costs[class as usize] = (took.max(decayed - decayed / 16), Some(now));
    }

    /// What a look is expected to cost at `now`, as a class's call is.
    fn look_cost_at(&self, now: Instant) -> Duration {
        decayed(self.look_cost, LOOK_FLOOR, now)
    }

    /// What a pulse is charged at `now`: what a landing costs (any look may find a render
    /// landed, and whether one has is known only by looking), or a look's cost when that is more.
    /// `None` before a landing of this run of step jobs has been timed: no pulse runs then.
    fn pulse_charge(&self, now: Instant) -> Option<Duration> {
        self.landing_cost.map(|c| c.max(self.look_cost_at(now)))
    }

    /// Whether pulses are wanted after work that answered `waits` (a job's render on the GPU).
    fn pulses_wanted(&self, waits: bool) -> bool {
        waits && self.landing_cost.is_some()
    }

    /// A pump in which a job did more than wait took `took`. The estimate is the last landing's
    /// cost, or what it was less a quarter when that is more: a costly landing is remembered for
    /// a few landings, however far apart they come (light review 5: an estimate that halved every
    /// 50 ms had forgotten a landing by the next one when renders took longer than that, and a
    /// landing started with room for a look). It is forgotten when a frame's pump finds no step
    /// job queued ([`Schedule::frame_pump`]): until one of the next jobs has landed in a frame's
    /// own pump, as every landing did before pulses, no pulse runs.
    fn landed(&mut self, took: Duration) {
        self.landing_cost = Some(self.landing_cost.map_or(took, |c| took.max(c - c / 4)));
        self.landings += 1;
        self.landing_last = took;
        self.landing_most = self.landing_most.max(took);
    }

    /// A pump in which every job only waited took `took`, measured at `now`.
    fn looked(&mut self, took: Duration, now: Instant) {
        self.look_cost = (took.max(self.look_cost_at(now)), Some(now));
    }

    /// Sizes the next window after a present that returned at `now`. `frame` is the presented
    /// frame's number (unchanged = nothing was presented: minimized, or an out-of-date swapchain),
    /// `served` how long this frame's serve window lasted and `blocked` how long the present waited
    /// for the GPU and the display.
    pub fn presented(&mut self, frame: u64, served: Duration, blocked: Duration, now: Instant) {
        if frame == self.last_frame.0 {
            return;
        }
        let gap = self.last_frame.1.map(|last| now.saturating_duration_since(last));
        self.last_frame = (frame, Some(now));
        let was_probe = std::mem::take(&mut self.probing);
        let Some(gap) = gap.filter(|g| (MIN_GAP..=MAX_GAP).contains(g)) else {
            // The first frame, or back from a stall: measure afresh before serving long.
            self.serve = MIN_SERVE;
            self.sized_for = self.period();
            self.probing = true;
            return;
        };
        let (refresh, margin) = (self.refresh(), self.margin());
        // The period this frame's window was sized for; over two refreshes it skipped vblanks on
        // purpose, and a present that has free images needn't block.
        let sized_for = self.sized_for;
        let skipping = sized_for > refresh * 3 / 2;
        self.work = gap.saturating_sub(served + blocked).max(self.work - self.work / 16);
        let late = gap > sized_for + refresh / 2;
        if late {
            self.late_presents += 1;
        }
        // A late frame whose present still blocked over a refresh was ready before its vblank (a
        // driver that blocks until the vblank after a late frame blocks less than a refresh).
        if late && blocked >= refresh + margin / 2 {
            self.skipped_presents += 1;
        } else if late || (!skipping && blocked < margin / 4) {
            if !late {
                self.unblocked_presents += 1;
            }
            self.caution = (self.caution * 1.5).min(MAX_CAUTION);
        } else {
            self.caution = 1.0 + (self.caution - 1.0) * CAUTION_KEEP;
        }
        if was_probe {
            self.probes[self.probe_count % PROBES] = gap;
            self.probe_count += 1;
            self.refresh = Some(median(&self.probes[..self.probe_count.min(PROBES)]));
        }
        let period = self.period();
        let least = if self.busy() { (period / 4).max(MIN_SERVE) } else { MIN_SERVE };
        let sized = if period > refresh * 3 / 2 && sized_for == period {
            // Skipping vblanks: steer the time from the last present to calling `present` (the
            // gap less what it blocked) to the period less the base margin. A present with free
            // images returns at once, and a frame presented early waits in the queue for its
            // vblank, so early is safe and the caution isn't needed.
            let base_margin = self.base_margin();
            let ready = gap.saturating_sub(blocked).as_secs_f64();
            let error = (period - base_margin).as_secs_f64() - ready;
            Duration::from_secs_f64((self.sized.as_secs_f64() + error / 2.0).max(0.0))
        } else {
            period.saturating_sub(self.work + self.margin())
        };
        self.sized = sized.clamp(least, MAX_SERVE.max(least));
        self.sized_for = period;
        self.serve = self.sized;
        // Under a backlog or a cap frames skip vblanks on purpose: no probes (the interval is
        // kept; a probe's early present would show at the next vblank, over the cap).
        self.since_probe += 1;
        let measure = self.display.is_none() && period == self.refresh();
        if measure && (self.since_probe >= PROBE_EVERY || self.probe_count < FIRST_PROBES) {
            self.since_probe = 0;
            self.serve = MIN_SERVE;
            self.probing = true;
        }
    }

    /// A window ended. Calls already queued are taken into `held` (in order) until it holds
    /// [`BACKLOG_CALLS`]; that many make a backlog.
    fn ended<R>(&mut self, inbox: &mut impl Inbox<R>, held: &mut VecDeque<R>) -> Served {
        while held.len() < BACKLOG_CALLS {
            match inbox.try_recv() {
                Ok(r) => held.push_back(r),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Served::Disconnected,
            }
        }
        if self.busy() {
            self.busy_windows += 1;
        }
        let backlog = held.len() >= BACKLOG_CALLS;
        self.pressure = if backlog { (self.pressure + 1).min(2 * BUSY_AFTER) } else { self.pressure.saturating_sub(1) };
        Served::Window
    }

    /// Serves calls from `inbox` (oldest first; `held` before any) until `deadline`, waking as
    /// each arrives (see the module docs for how waits end). `class` tells what a call costs;
    /// `handle` applies one; `stop` is checked between calls. Calls still queued at the end may be
    /// moved to `held`, which the next window starts with. (No step job: the engine's loop uses
    /// [`Schedule::serve_frame`]; the tests, this.)
    #[cfg(test)]
    pub fn serve<R>(
        &mut self,
        inbox: &mut impl Inbox<R>,
        held: &mut VecDeque<R>,
        deadline: Instant,
        class: impl Fn(&R) -> Class,
        handle: impl FnMut(R),
        stop: impl FnMut() -> bool,
    ) -> Served {
        /// Calls only: no step job.
        struct Calls<C, H, S>(C, H, S);
        impl<R, C: Fn(&R) -> Class, H: FnMut(R), S: FnMut() -> bool> Window<R> for Calls<C, H, S> {
            fn class(&self, call: &R) -> Class {
                (self.0)(call)
            }
            fn call(&mut self, call: R) {
                (self.1)(call)
            }
            fn pump(&mut self, _: bool) -> bool {
                false
            }
            fn gpu_waiting(&self) -> bool {
                false
            }
            fn steps_queued(&self) -> bool {
                false
            }
            fn stop(&mut self) -> bool {
                (self.2)()
            }
        }
        self.serve_with(inbox, held, deadline, &mut Calls(class, handle, stop))
    }

    /// One frame's serve window, as the windowed loop serves it: from now for the window the
    /// last present sized (or [`MINIMIZED_SERVE`] while the window is minimized). Returns why it
    /// ended and how long it lasted.
    pub fn serve_frame<R>(&mut self, inbox: &mut impl Inbox<R>, held: &mut VecDeque<R>, minimized: bool, w: &mut impl Window<R>) -> (Served, Duration) {
        let start = inbox.now();
        let window = if minimized { MINIMIZED_SERVE } else { self.window() };
        let ended = self.serve_with(inbox, held, start + window, w);
        (ended, inbox.now().saturating_duration_since(start))
    }

    /// The frame's own pump of the step jobs, once a frame after its serve window. When a pulse
    /// ran in that window, it leaves alone the jobs pulses are for: they were looked at within
    /// the window, and a render that lands after the window's last pulse is landed by the next
    /// window's first, where the window has room for it, not here in the frame's own time (light
    /// review 5: held to 20 frames a second a pulse landed one render in the window and this
    /// pump a second, and the frame paid for two). Its landings are timed as a pulse's are;
    /// with no step job queued after it, what a landing costs is forgotten.
    pub fn frame_pump<R>(&mut self, inbox: &impl Inbox<R>, w: &mut impl Window<R>) {
        let leave = std::mem::take(&mut self.pulsed);
        self.frame_pumps_left += u64::from(leave);
        let start = inbox.now();
        let worked = w.pump(leave);
        let done = inbox.now();
        if worked {
            self.landed(done.saturating_duration_since(start));
        }
        if !w.steps_queued() {
            self.landing_cost = None;
        }
    }

    /// [`Schedule::serve`], and pulses while a step job waits on its render (the module docs):
    /// `w` applies calls and pumps the step jobs, and says whether one waits on the GPU. A pulse
    /// is due at once when one waits as the window starts, and [`PULSE_EVERY`] after a call or
    /// a pulse that leaves one waiting; it runs between calls, and starts only when what a
    /// landing costs fits before `deadline` (a pulse left out then is the last of the window,
    /// unless a later call starts them again), never before a landing of this run of jobs has
    /// been timed, and never when no job waits.
    pub fn serve_with<R>(&mut self, inbox: &mut impl Inbox<R>, held: &mut VecDeque<R>, deadline: Instant, w: &mut impl Window<R>) -> Served {
        self.windows += 1;
        let mut first = true;
        // When the step jobs are next looked at (`None`: no job waits on the GPU).
        let mut next_pulse = self.pulses_wanted(w.gpu_waiting()).then(|| inbox.now());
        loop {
            if w.stop() {
                return Served::Stopped;
            }
            let now = inbox.now();
            if next_pulse.is_some_and(|at| now >= at) {
                if self.pulse_charge(now).is_none_or(|charge| now + charge > deadline) {
                    // No room before the frame for what a landing costs.
                    self.pulses_left_out += 1;
                    next_pulse = None;
                } else {
                    let worked = w.pump(false);
                    let waits = w.gpu_waiting();
                    self.pulses += 1;
                    self.pulsed = true;
                    let done = inbox.now();
                    let took = done.saturating_duration_since(now);
                    if worked {
                        self.landed(took);
                    } else {
                        self.looked(took, done);
                    }
                    next_pulse = self.pulses_wanted(waits).then_some(done + PULSE_EVERY);
                    continue;
                }
            }
            let request = match held.pop_front() {
                Some(r) => r,
                None if now >= deadline => return self.ended(inbox, held),
                None => {
                    // Never tied to the caution: a margin raised by late frames shrinks the window
                    // by as much, and must not let waits run later in turn.
                    let latest = deadline + self.base_margin() / 2;
                    let mut wait = whole_ms((latest - now).saturating_sub(TIMER_SLOP));
                    if let Some(at) = next_pulse.filter(|_| !wait.is_zero()) {
                        // Woken for the next pulse (a call still wakes it at once).
                        wait = wait.min(whole_ms_up(at.saturating_duration_since(now)));
                    }
                    let received = if wait.is_zero() {
                        inbox.try_recv().map_err(|e| match e {
                            TryRecvError::Empty => RecvTimeoutError::Timeout,
                            TryRecvError::Disconnected => RecvTimeoutError::Disconnected,
                        })
                    } else {
                        inbox.recv_timeout(wait)
                    };
                    match received {
                        Ok(r) => r,
                        Err(RecvTimeoutError::Timeout) if !wait.is_zero() => continue,
                        Err(RecvTimeoutError::Timeout) => return self.ended(inbox, held),
                        Err(RecvTimeoutError::Disconnected) => return Served::Disconnected,
                    }
                }
            };
            // A call that would likely run past the deadline waits for the next window (unless
            // it is this window's first: every window makes progress).
            let kind = w.class(&request);
            let now = inbox.now();
            if !first && now + self.cost(kind, now) > deadline {
                held.push_front(request);
                // Counted only when it came in time but didn't fit; one that woke a wait past the
                // deadline just queues for the next window (the frame starts no later for it).
                if now < deadline {
                    self.held += 1;
                }
                return self.ended(inbox, held);
            }
            first = false;
            w.call(request);
            let waits = self.pulses_wanted(w.gpu_waiting());
            self.calls += 1;
            let done = inbox.now();
            self.costed(kind, done - now, done);
            // A call may have started a job that now waits on its render (or ended the one that
            // did): pulses start or stop with it.
            next_pulse = match (waits, next_pulse) {
                (false, _) => None,
                (true, Some(at)) => Some(at),
                (true, None) => Some(done + PULSE_EVERY),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::mpsc::{self, Sender};

    fn ms(v: f64) -> Duration {
        Duration::from_secs_f64(v / 1000.0)
    }

    const HZ60: Duration = Duration::from_micros(16_667);
    const HZ240: Duration = Duration::from_micros(4_167);

    /// `frames` frames on a display refreshing every `refresh` whose own work takes `work`; each
    /// serves its whole window, and `present` blocks until the next vblank (FIFO). Returns how
    /// many skipped a vblank they were sized for.
    fn run(s: &mut Schedule, refresh: Duration, work: Duration, frames: u64) -> u64 {
        let mut t = s.last_frame.1.unwrap_or_else(Instant::now);
        let mut late = 0;
        for _ in 0..frames {
            let (period, busy) = (s.period(), s.window() + work);
            let gap = refresh * (busy.as_nanos() / refresh.as_nanos() + 1) as u32;
            late += (gap > period) as u64;
            t += gap;
            s.presented(s.last_frame.0 + 1, s.window(), gap - busy, t);
        }
        late
    }

    #[test]
    fn the_window_fills_the_frame_but_the_margin() {
        for (refresh, work, margin) in [(HZ60, ms(2.0), MAX_MARGIN), (HZ240, ms(1.0), HZ240 / 4)] {
            let mut s = Schedule::default();
            run(&mut s, refresh, work, 40);
            assert_eq!(run(&mut s, refresh, work, 100), 0, "no late frames at {refresh:?}");
            assert_eq!((s.refresh, s.margin()), (Some(refresh), margin));
            assert!(s.sized.abs_diff(refresh - work - margin) < ms(0.01), "window {:?} at {refresh:?}", s.sized);
        }
    }

    #[test]
    fn heavier_frames_shrink_the_window_at_once_and_late_ones_raise_the_margin() {
        let mut s = Schedule::default();
        run(&mut s, HZ60, ms(2.0), 60);
        assert_ne!(s.since_probe, 0, "the next frame is no probe");
        let (w, m) = (s.sized, s.margin());
        // One frame's work jumps by 5 ms: it misses its vblank.
        assert_eq!(run(&mut s, HZ60, ms(7.0), 1), 1);
        assert!(s.sized <= w - ms(5.0), "window {:?}", s.sized);
        assert!(s.margin() > m);
        // Back to normal: the window regrows as the spike decays, without late frames.
        assert_eq!(run(&mut s, HZ60, ms(2.0), 200), 0);
        assert!(s.sized.abs_diff(w) < ms(0.05), "window {:?} vs {w:?}", s.sized);
    }

    #[test]
    fn a_late_frame_that_was_ready_teaches_nothing() {
        let mut s = Schedule::default();
        run(&mut s, HZ240, ms(1.0), 60);
        assert_ne!(s.since_probe, 0, "the next frame is no probe");
        let (w, m) = (s.window(), s.margin());
        // Two refreshes, and present blocked over a refresh: the compositor skipped a vblank.
        let at = s.last_frame.1.unwrap() + HZ240 * 2;
        s.presented(s.last_frame.0 + 1, w, HZ240 * 2 - w - ms(1.0), at);
        assert_eq!((s.window(), s.margin(), s.late_presents, s.skipped_presents), (w, m, 1, 1));
    }

    #[test]
    fn a_backlog_presents_at_60_fps_on_whole_vblanks_and_serves_the_rest() {
        let mut s = Schedule::default();
        run(&mut s, HZ240, ms(1.0), 60);
        s.pressure = BUSY_AFTER;
        assert_eq!(s.period(), HZ240 * 4);
        run(&mut s, HZ240, ms(1.0), 20);
        assert_eq!(run(&mut s, HZ240, ms(1.0), 100), 0);
        assert!(s.window().abs_diff(HZ240 * 4 - ms(1.0) - HZ240 / 4) < ms(0.01), "window {:?}", s.window());
        // The display's own rate is kept: once the backlog clears, frames come every vblank (the
        // frame served with the backlog's window is the only late one).
        assert_eq!(s.refresh, Some(HZ240));
        s.pressure = 0;
        assert!(run(&mut s, HZ240, ms(1.0), 100) <= 1);
        assert_eq!(run(&mut s, HZ240, ms(1.0), 100), 0);
        assert_eq!(s.period(), HZ240);
        // At 60 Hz a backlog changes nothing; with frames over the period agents still get a
        // quarter of it.
        let mut s = Schedule::default();
        run(&mut s, HZ60, ms(2.0), 60);
        s.pressure = BUSY_AFTER;
        assert_eq!(s.period(), HZ60);
        run(&mut s, HZ60, ms(20.0), 30);
        assert_eq!(s.window(), HZ60 / 4);
    }

    /// Owner decision 2026-10-17: the frame floor is a setting the lead agent picks from the
    /// operator's hardware. Lower floors present on fewer vblanks under a backlog and give agents
    /// the rest; the default stays 60 fps, and out-of-range values are clamped.
    #[test]
    fn the_frame_floor_sets_how_many_vblanks_a_backlog_skips() {
        let mut s = Schedule::default();
        assert_eq!(s.frame_floor_fps(), DEFAULT_FRAME_FLOOR_FPS);
        run(&mut s, HZ60, ms(2.0), 60);
        s.pressure = BUSY_AFTER;
        assert_eq!(s.period(), HZ60, "60 fps at 60 Hz: every vblank");
        s.set_frame_floor(30);
        assert_eq!((s.frame_floor_fps(), s.period()), (30, HZ60 * 2));
        run(&mut s, HZ60, ms(2.0), 30);
        assert_eq!(run(&mut s, HZ60, ms(2.0), 60), 0, "no frame misses its every-other vblank");
        assert!(s.window() > HZ60, "agents get most of two refreshes: {:?}", s.window());
        s.set_frame_floor(20);
        assert_eq!(s.period(), HZ60 * 3);
        s.set_frame_floor(1);
        assert_eq!(s.frame_floor_fps(), MIN_FRAME_FLOOR_FPS, "clamped");
        s.set_frame_floor(1000);
        assert_eq!((s.frame_floor_fps(), s.period()), (MAX_FRAME_FLOOR_FPS, HZ60), "a floor over the display's rate: every vblank");
        // At 240 Hz the default skips to every 4th vblank; 30 fps to every 8th.
        let mut s = Schedule::default();
        run(&mut s, HZ240, ms(1.0), 60);
        s.pressure = BUSY_AFTER;
        assert_eq!(s.period(), HZ240 * 4);
        s.set_frame_floor(30);
        assert_eq!(s.period(), HZ240 * 8);
        // Without a backlog the floor changes nothing.
        s.pressure = 0;
        assert_eq!(s.period(), HZ240);
    }

    /// The frame cap presents on the fewest whole vblanks that keep under it, with or without a
    /// backlog (it wins over the floor), steers the gap to its period, and is clamped to its range.
    #[test]
    fn the_frame_cap_skips_vblanks_to_stay_under_it() {
        let mut s = Schedule::default();
        s.set_display_refresh(Some(HZ240));
        run(&mut s, HZ240, ms(1.0), 10);
        assert_eq!((s.max_fps(), s.period()), (None, HZ240));
        s.set_max_fps(Some(60));
        assert_eq!((s.max_fps(), s.period()), (Some(60), HZ240 * 4));
        let mut t = s.last_frame.1.unwrap();
        let mut gap = Duration::ZERO;
        for _ in 0..60 {
            // A free image: present returns at once, the frame waits in the queue for its vblank.
            gap = s.window() + ms(1.2);
            t += gap;
            s.presented(s.last_frame.0 + 1, s.window(), Duration::ZERO, t);
        }
        assert!(gap.abs_diff(HZ240 * 4 - HZ240 / 4) < ms(0.05), "gap {gap:?}");
        assert!(s.window() > HZ240 * 3, "agents get most of the period: {:?}", s.window());
        // 144 Hz: two vblanks would be 72 fps, so three (48 fps); 60 Hz: every vblank.
        s.set_display_refresh(Some(Duration::from_nanos(1_000_000_000 / 144)));
        assert_eq!(s.period(), Duration::from_nanos(1_000_000_000 / 144) * 3);
        s.set_display_refresh(Some(HZ60));
        assert_eq!(s.period(), HZ60);
        // A cap under the floor wins under a backlog; over it, the floor's skipping stays.
        s.set_display_refresh(Some(HZ240));
        s.pressure = BUSY_AFTER;
        s.set_max_fps(Some(30));
        assert_eq!(s.period(), HZ240 * 8);
        s.set_max_fps(Some(120));
        assert_eq!(s.period(), HZ240 * 4, "the 60 fps floor's skipping");
        s.set_max_fps(Some(1));
        assert_eq!(s.max_fps(), Some(MIN_MAX_FPS), "clamped");
        s.set_max_fps(None);
        assert_eq!(s.period(), HZ240 * 4);
        // On every whole-hertz display the cap holds and is as close as whole vblanks allow.
        for hz in 24u64..=360 {
            let refresh = Duration::from_nanos(1_000_000_000 / hz);
            let mut s = Schedule::default();
            s.set_display_refresh(Some(refresh));
            for fps in [MIN_MAX_FPS, 30, 48, 60, 90, 120, 144, 240] {
                s.set_max_fps(Some(fps));
                let (period, cap) = (s.period(), Duration::from_nanos(1_000_000_000 / u64::from(fps)));
                assert!(period + refresh / 128 >= cap, "{fps} fps at {hz} Hz: period {period:?} < {cap:?}");
                assert!(period == refresh || period < cap + refresh, "{fps} fps at {hz} Hz: one vblank too many");
            }
        }
    }

    /// Review round 3: the floor is a promise ("at least frame_floor_fps"): on every whole-hertz
    /// display from 24 to 360 Hz and every floor, a backlog presents at least that often (every
    /// vblank when the floor is above the display's rate), and skips as many vblanks as it can.
    #[test]
    fn the_frame_floor_holds_on_every_display_rate() {
        for hz in 24u64..=360 {
            let refresh = Duration::from_nanos(1_000_000_000 / hz);
            let mut s = Schedule::default();
            s.set_display_refresh(Some(refresh));
            s.pressure = BUSY_AFTER;
            for fps in MIN_FRAME_FLOOR_FPS..=MAX_FRAME_FLOOR_FPS {
                s.set_frame_floor(fps);
                let (period, floor) = (s.period(), floor_period(fps));
                if u64::from(fps) > hz {
                    assert_eq!(period, refresh, "{fps} fps at {hz} Hz: every vblank");
                } else {
                    assert!(period <= floor, "{fps} fps at {hz} Hz: period {period:?} > {floor:?}");
                    assert!(period + refresh > floor, "{fps} fps at {hz} Hz: could skip another vblank");
                }
            }
        }
    }

    /// Drivers whose present returns at once while a swapchain image is free (frames slower than
    /// the display): under a backlog the gap between presents is steered to a margin under the
    /// period (the frame then waits for its vblank in the queue).
    #[test]
    fn a_backlog_steers_the_gap_to_the_period_when_present_does_not_block() {
        let mut s = Schedule::default();
        s.set_display_refresh(Some(HZ240));
        run(&mut s, HZ240, ms(1.0), 10);
        s.pressure = BUSY_AFTER;
        let mut t = s.last_frame.1.unwrap();
        let mut gap = Duration::ZERO;
        for _ in 0..60 {
            gap = s.window() + ms(1.2);
            t += gap;
            s.presented(s.last_frame.0 + 1, s.window(), Duration::ZERO, t);
        }
        assert!(gap.abs_diff(HZ240 * 4 - HZ240 / 4) < ms(0.05), "gap {gap:?}");
        // Not blocking is expected here (bar the first frame, still sized for every vblank).
        assert!(s.unblocked_presents <= 1, "{} unblocked", s.unblocked_presents);
    }

    #[test]
    fn the_refresh_interval_is_the_median_probe_gap() {
        assert_eq!(median(&[ms(4.0), ms(9.0), ms(1.0)]), ms(4.0));
        let mut s = Schedule::default();
        run(&mut s, HZ240, ms(1.0), 4);
        assert_eq!((s.refresh, s.probe_count), (Some(HZ240), FIRST_PROBES));
        // A late and a short gap (one probe each) don't move it.
        for odd in [HZ240 * 2, ms(3.0)] {
            run(&mut s, HZ240, ms(1.0), PROBE_EVERY as u64 - 1);
            assert!(s.probing);
            let at = s.last_frame.1.unwrap() + odd;
            s.presented(s.last_frame.0 + 1, MIN_SERVE, ms(1.0), at);
        }
        assert_eq!(s.refresh, Some(HZ240));
        // The display changes to 60 Hz: within a median's worth of probes it is seen.
        let probes = (PROBES / 2 + 1) as u64;
        run(&mut s, HZ60, ms(2.0), probes * PROBE_EVERY as u64);
        assert_eq!(s.refresh, Some(HZ60));
        // And back to 240 Hz: windows sized for 60 Hz keep the other gaps at 4 refreshes, but
        // the probes' frames land on the next vblank.
        run(&mut s, HZ240, ms(1.0), probes * PROBE_EVERY as u64);
        assert_eq!(s.refresh, Some(HZ240));
    }

    #[test]
    fn a_refresh_rate_the_display_reports_needs_no_probes() {
        let mut s = Schedule::default();
        s.set_display_refresh(Some(HZ240));
        // The first frames are small (no gap yet); from then on every window is sized.
        run(&mut s, HZ240, ms(1.0), 2);
        let probes = s.probe_count;
        assert_eq!(run(&mut s, HZ240, ms(1.0), 100), 0);
        assert_eq!((s.probe_count, s.window(), s.period()), (probes, s.sized, HZ240));
        assert!(s.window() > MIN_SERVE);
        s.pressure = BUSY_AFTER;
        assert_eq!(s.period(), HZ240 * 4);
        // Nonsense is ignored.
        s.set_display_refresh(Some(Duration::from_secs(1)));
        assert_eq!(s.display, None);
    }

    #[test]
    fn presents_that_showed_nothing_and_long_gaps_teach_nothing() {
        let mut s = Schedule::default();
        run(&mut s, HZ240, ms(1.0), 60);
        let (w, m, r) = (s.window(), s.margin(), s.refresh);
        let at = s.last_frame.1.unwrap();
        s.presented(s.last_frame.0, w, Duration::ZERO, at + HZ240);
        assert_eq!(s.window(), w);
        // Minimized for a second, then back: nothing learned, and the next frame is a probe.
        s.presented(s.last_frame.0 + 1, w, Duration::ZERO, at + Duration::from_secs(1));
        assert_eq!((s.margin(), s.refresh, s.late_presents), (m, r, 0));
        assert_eq!((s.window(), s.probing), (MIN_SERVE, true));
    }

    #[test]
    fn a_window_never_passes_the_largest() {
        let mut s = Schedule::default();
        run(&mut s, Duration::from_millis(90), ms(1.0), 60);
        assert_eq!(s.sized, MAX_SERVE);
    }


    // ---------------------------------------------------------------------------------------
    // The loop against a simulated clock and display: deterministic, so fit for any CI machine.

    /// A call in the simulation: when it arrives, what handling it costs, its class, and the
    /// client that sends its next call a round trip after the reply (if any).
    #[derive(Clone, Copy, Debug)]
    struct Call {
        id: u64,
        at: Duration,
        cost: Duration,
        class: Class,
        client: Option<usize>,
    }

    /// Simulated time and calls. Timed waits end like Windows' under a 1 ms timer: the timeout
    /// rounded up to whole milliseconds, then mostly up to 1 ms late, now and then 2; a call arriving first wakes
    /// them. Agents' round trips vary too.
    struct Sim {
        origin: Instant,
        t: Duration,
        /// Arrivals, in order.
        queue: VecDeque<Call>,
        /// Clients' typical round trip from a reply to their next call (each one 0.5 to 1.5 times
        /// it).
        rtt: Duration,
        handled: Vec<u64>,
        next_id: u64,
        /// Pseudo-random state (xorshift): round trips and timer wake-ups vary, deterministically.
        seed: u32,
        /// Timed waits begun (each one wake of the engine thread).
        wakes: u64,
    }

    type Shared = Rc<RefCell<Sim>>;

    impl Sim {
        fn new(rtt: Duration) -> Shared {
            let sim = Sim { origin: Instant::now(), t: Duration::ZERO, queue: VecDeque::new(), rtt, handled: Vec::new(), next_id: 0, seed: 0x2545_f491, wakes: 0 };
            Rc::new(RefCell::new(sim))
        }

        /// A number in [0, 1).
        fn random(&mut self) -> f64 {
            self.seed ^= self.seed << 13;
            self.seed ^= self.seed >> 17;
            self.seed ^= self.seed << 5;
            f64::from(self.seed) / (f64::from(u32::MAX) + 1.0)
        }

        fn push(&mut self, at: Duration, cost: Duration, class: Class, client: Option<usize>) {
            let call = Call { id: self.next_id, at, cost, class, client };
            self.next_id += 1;
            let i = self.queue.iter().position(|c| c.at > at).unwrap_or(self.queue.len());
            self.queue.insert(i, call);
        }

        fn handle(&mut self, c: Call) {
            self.t += c.cost;
            self.handled.push(c.id);
            if c.client.is_some() {
                let spread = 0.5 + self.random();
                let at = self.t + self.rtt.mul_f64(spread);
                self.push(at, c.cost, c.class, c.client);
            }
        }
    }

    struct SimInbox(Shared);

    impl Inbox<Call> for SimInbox {
        fn now(&self) -> Instant {
            let s = self.0.borrow();
            s.origin + s.t
        }

        fn try_recv(&mut self) -> Result<Call, TryRecvError> {
            let mut s = self.0.borrow_mut();
            match s.queue.front() {
                Some(c) if c.at <= s.t => Ok(s.queue.pop_front().expect("front")),
                _ => Err(TryRecvError::Empty),
            }
        }

        fn recv_timeout(&mut self, timeout: Duration) -> Result<Call, RecvTimeoutError> {
            let mut s = self.0.borrow_mut();
            s.wakes += 1;
            // Measured on Windows with a 1 ms timer: mostly within 1 ms after the rounded-up
            // timeout, now and then up to 2 ms.
            let late = if s.random() < 0.9 { ms(s.random()) } else { ms(1.0 + s.random()) };
            let fires = s.t + Duration::from_millis(timeout.as_nanos().div_ceil(1_000_000) as u64) + late;
            match s.queue.front().copied() {
                Some(c) if c.at <= fires => {
                    s.t = s.t.max(c.at);
                    Ok(s.queue.pop_front().expect("front"))
                }
                _ => {
                    s.t = fires;
                    Err(RecvTimeoutError::Timeout)
                }
            }
        }
    }

    /// What one stretch of the simulated loop did.
    #[derive(Debug)]
    struct Run {
        calls_per_s: f64,
        fps: f64,
        late: u64,
        frames: u64,
        /// Calls that arrived in a window but had to wait for the next one.
        held: u64,
        /// The caution at the end.
        caution: f32,
    }

    /// Runs the windowed loop for `seconds` of simulated time: serve, `work` of frame work, then a
    /// FIFO present that blocks until the next vblank of a display refreshing every `refresh`.
    fn sim_loop(s: &mut Schedule, sim: &Shared, refresh: Duration, work: Duration, seconds: f64) -> Run {
        sim_frames(s, sim, refresh, work, seconds, &mut SimWindow::new(sim, None, true))
    }

    /// [`sim_loop`], with a step job whose renders the frame's own pump looks at once a frame
    /// (as the engine's does) and the serve windows pulse for.
    fn sim_loop_with(s: &mut Schedule, sim: &Shared, refresh: Duration, work: Duration, seconds: f64, job: Option<&RefCell<SimJob>>) -> Run {
        sim_frames(s, sim, refresh, work, seconds, &mut SimWindow::new(sim, job, true))
    }

    /// The windowed loop as `run_windowed` runs it ([`Schedule::serve_frame`], then the frame's
    /// own pump with [`Schedule::frame_pump`], the frame's work, a present), over `w`.
    fn sim_frames(s: &mut Schedule, sim: &Shared, refresh: Duration, work: Duration, seconds: f64, w: &mut SimWindow) -> Run {
        let mut inbox = SimInbox(Rc::clone(sim));
        let mut held = VecDeque::new();
        let (start, calls, held_before) = (sim.borrow().t, sim.borrow().handled.len(), s.held);
        let (mut frames, mut late, mut last) = (0, 0, None);
        while sim.borrow().t < start + Duration::from_secs_f64(seconds) {
            let period = s.period();
            let (end, served) = s.serve_frame(&mut inbox, &mut held, false, w);
            assert_eq!(end, Served::Window);
            // The frame's own pump: frame work, as in the engine (not served time).
            w.in_frame = true;
            s.frame_pump(&inbox, w);
            w.in_frame = false;
            let mut st = sim.borrow_mut();
            st.t += work;
            let at = refresh * (st.t.as_nanos() / refresh.as_nanos() + 1) as u32;
            let blocked = at - st.t;
            st.t = at;
            late += last.is_some_and(|prev| at - prev > period + refresh / 2) as u64;
            last = Some(at);
            frames += 1;
            let now = st.origin + st.t;
            drop(st);
            s.presented(s.last_frame.0 + 1, served, blocked, now);
        }
        let elapsed = (sim.borrow().t - start).as_secs_f64();
        let calls = (sim.borrow().handled.len() - calls) as f64;
        // Calls held for the next window go back to the head of the queue for the next stretch.
        while let Some(c) = held.pop_back() {
            sim.borrow_mut().queue.push_front(c);
        }
        Run { calls_per_s: calls / elapsed, fps: frames as f64 / elapsed, late, frames, held: s.held - held_before, caution: s.caution }
    }

    /// A simulated step job with renders on the GPU, one after another, as `perf_light`'s: each
    /// takes `gpu` to land; a look that finds it still in flight costs `look`, one that lands it
    /// (and submits the next) `land`.
    struct SimJob {
        gpu: Duration,
        look: Duration,
        land: Duration,
        /// When the render in flight lands (`None`: none in flight, the job is done).
        lands_at: Option<Duration>,
        renders_left: usize,
        /// Every render landed: when it landed and when a look saw it.
        landed: Vec<(Duration, Duration)>,
        /// Looks, and the looks that found the render still in flight.
        looks: u64,
        waited: u64,
        /// When each look ended (the pulses' own time included).
        ends: Vec<Duration>,
    }

    impl SimJob {
        fn new(sim: &Shared, gpu: Duration, renders: usize) -> RefCell<SimJob> {
            let t = sim.borrow().t;
            RefCell::new(SimJob { gpu, look: ms(0.02), land: ms(0.6), lands_at: Some(t + gpu), renders_left: renders, landed: Vec::new(), looks: 0, waited: 0, ends: Vec::new() })
        }

        fn on_gpu(&self) -> bool {
            self.lands_at.is_some()
        }
    }

    /// The step pump over `job` at the simulated time: lands its render if the GPU is done (and
    /// says so: a landing), or only looks.
    fn look(sim: &Shared, job: &RefCell<SimJob>) -> bool {
        let mut st = sim.borrow_mut();
        let mut j = job.borrow_mut();
        let Some(at) = j.lands_at else { return false };
        j.looks += 1;
        let landed = st.t >= at;
        if landed {
            j.landed.push((at, st.t));
            st.t += j.land;
            j.renders_left -= 1;
            j.lands_at = (j.renders_left > 0).then_some(st.t + j.gpu);
        } else {
            st.t += j.look;
            j.waited += 1;
        }
        j.ends.push(st.t);
        landed
    }

    /// The simulation's side of a frame ([`Window`]): calls the simulation handles, and one step
    /// job. `pulses`: whether it asks for pulses while the job's render is on the GPU (`false`:
    /// the job is looked at only by the frame's own pump, as before pulses).
    struct SimWindow<'a> {
        sim: Shared,
        job: Option<&'a RefCell<SimJob>>,
        pulses: bool,
        /// Set by the loop around the frame's own pump.
        in_frame: bool,
        /// When each pulse started (simulated), and the frame pumps that left the job alone.
        pulse_starts: Vec<Duration>,
        left_alone: u64,
    }

    impl<'a> SimWindow<'a> {
        fn new(sim: &Shared, job: Option<&'a RefCell<SimJob>>, pulses: bool) -> SimWindow<'a> {
            SimWindow { sim: Rc::clone(sim), job, pulses, in_frame: false, pulse_starts: Vec::new(), left_alone: 0 }
        }
    }

    impl Window<Call> for SimWindow<'_> {
        fn class(&self, call: &Call) -> Class {
            call.class
        }

        fn call(&mut self, call: Call) {
            self.sim.borrow_mut().handle(call);
        }

        fn pump(&mut self, leave_pulsed: bool) -> bool {
            let Some(j) = self.job else { return false };
            if leave_pulsed && j.borrow().on_gpu() {
                self.left_alone += 1;
                return false;
            }
            if !self.in_frame {
                self.pulse_starts.push(self.sim.borrow().t);
            }
            look(&self.sim, j)
        }

        fn gpu_waiting(&self) -> bool {
            self.pulses && self.job.is_some_and(|j| j.borrow().on_gpu())
        }

        fn steps_queued(&self) -> bool {
            self.job.is_some_and(|j| j.borrow().on_gpu())
        }

        fn stop(&mut self) -> bool {
            false
        }
    }

    /// One serve window of `len` from now with `job`; returns the window's deadline (simulated).
    /// The schedule has timed one of the job's landings, as a frame's own pump would have.
    fn one_window(s: &mut Schedule, sim: &Shared, len: Duration, job: Option<&RefCell<SimJob>>) -> Duration {
        one_window_in(s, &mut SimWindow::new(sim, job, true), len)
    }

    fn one_window_in(s: &mut Schedule, w: &mut SimWindow, len: Duration) -> Duration {
        if let (Some(j), None) = (w.job, s.landing_cost) {
            s.landed(j.borrow().land);
        }
        let mut inbox = SimInbox(Rc::clone(&w.sim));
        let deadline_t = w.sim.borrow().t + len;
        let deadline = w.sim.borrow().origin + deadline_t;
        let end = s.serve_with(&mut inbox, &mut VecDeque::new(), deadline, w);
        assert_eq!(end, Served::Window);
        deadline_t
    }

    /// Light review 4: with a window held to 20 frames a second a step job was looked at once a
    /// frame, so one render landed a frame. Now, in one window of a 20 fps frame, a render that
    /// lands inside it is stepped again inside it, by the next look; while it is in flight it is
    /// looked at about once a millisecond (at most one look a whole millisecond of its flight,
    /// and one more); and no look runs past the window's deadline. Counted, on a simulated
    /// clock (no real time).
    #[test]
    fn a_render_that_lands_in_a_window_is_stepped_again_within_it() {
        let sim = Sim::new(Duration::ZERO);
        let mut s = Schedule::default();
        let job = SimJob::new(&sim, ms(3.0), 1000);
        let deadline = one_window(&mut s, &sim, ms(45.0), Some(&job));
        let j = job.borrow();
        // Every render that landed with room left for a look and its landing was seen in the
        // window, at the first look after it landed.
        let room = PULSE_EVERY + TIMER_SLOP * 2 + j.land;
        for &(landed, seen) in &j.landed {
            assert!(seen <= deadline, "a look ran past the deadline");
            assert!(seen - landed <= PULSE_EVERY + TIMER_SLOP * 2, "landed at {landed:?}, seen at {seen:?}: not by the next look");
        }
        let in_window = j.landed.len();
        let fit = (ms(45.0).as_nanos() / (j.gpu + room).as_nanos()) as usize;
        assert!(in_window >= fit, "{in_window} renders landed in a 45 ms window; at least {fit} fit");
        // At most one look a whole millisecond of a render's flight, and one more.
        let per_render = (j.gpu.as_nanos().div_ceil(PULSE_EVERY.as_nanos()) + 1) as u64;
        assert!(j.waited <= per_render * (in_window as u64 + 1), "{} looks found a render in flight, for {in_window} renders", j.waited);
        // (A look starts only while what looks cost lately fits; a landing may end a little past
        // that, never past where a window's own waits may end.)
        assert!(j.ends.iter().all(|&e| e <= deadline + s.base_margin() / 2), "a look ended past the window's end");
        assert_eq!(s.pulses, j.looks);
        // Before (looked at once a frame): one render a frame.
        assert!(in_window > 1);
    }

    /// A render that does not land within the window: the job is looked at no more than once at
    /// the window's start and once a whole millisecond after (the stated cost of a job that
    /// waits), and never past its deadline.
    #[test]
    fn a_render_that_does_not_land_costs_at_most_a_look_a_millisecond() {
        let sim = Sim::new(Duration::ZERO);
        let mut s = Schedule::default();
        let job = SimJob::new(&sim, ms(500.0), 1);
        let len = ms(45.0);
        let deadline = one_window(&mut s, &sim, len, Some(&job));
        let j = job.borrow();
        assert!(j.landed.is_empty());
        let most = (len.as_nanos() / PULSE_EVERY.as_nanos()) as u64 + 1;
        assert!(j.looks >= 2 && j.looks <= most, "{} looks in a 45 ms window (at most {most})", j.looks);
        assert!(j.ends.iter().all(|&e| e <= deadline + s.base_margin() / 2));
    }

    /// With no job on the GPU a window makes no wake it would not have made: the same waits, the
    /// same calls in the same order, no pulse; and a pulse that finds nothing waiting is the
    /// last (one look, then the waits are a plain window's).
    #[test]
    fn with_no_job_waiting_a_window_makes_no_extra_wake() {
        for rtt in [None, Some(ms(0.7))] {
            let run = |pulse: Option<bool>| {
                let sim = Sim::new(rtt.unwrap_or(Duration::ZERO));
                if rtt.is_some() {
                    sim.borrow_mut().push(Duration::ZERO, ms(0.1), Class::Edit, Some(0));
                }
                let mut s = Schedule::default();
                let mut inbox = SimInbox(Rc::clone(&sim));
                let deadline = inbox.now() + ms(45.0);
                let handler = Rc::clone(&sim);
                // A job that waits as the window starts and has landed by its first look (its
                // look and landing take no time, so that only wakes and calls are compared).
                let job = SimJob::new(&sim, Duration::ZERO, 1);
                job.borrow_mut().land = Duration::ZERO;
                job.borrow_mut().look = Duration::ZERO;
                match pulse {
                    None => {
                        s.serve(&mut inbox, &mut VecDeque::new(), deadline, |c| c.class, |c| handler.borrow_mut().handle(c), || false);
                    }
                    Some(waiting) => {
                        s.landed(Duration::ZERO);
                        s.serve_with(&mut inbox, &mut VecDeque::new(), deadline, &mut SimWindow::new(&sim, Some(&job), waiting));
                    }
                }
                let st = sim.borrow();
                (st.wakes, st.handled.clone(), s.pulses)
            };
            let (wakes, calls, _) = run(None);
            assert_eq!(run(Some(false)), (wakes, calls.clone(), 0), "nothing waiting, round trip {rtt:?}");
            assert_eq!(run(Some(true)), (wakes, calls, 1), "one look, then a plain window, round trip {rtt:?}");
        }
    }

    /// A sequential agent and a job with renders on the GPU share the window: neither starves
    /// (the calls a plain window serves, less what the looks cost; renders landing as they do
    /// alone).
    #[test]
    fn calls_and_a_waiting_job_share_the_window() {
        let window = |with_job: bool| {
            let sim = Sim::new(ms(0.05));
            sim.borrow_mut().push(Duration::ZERO, ms(0.1), Class::Edit, Some(0));
            let job = SimJob::new(&sim, ms(3.0), 1000);
            let mut s = Schedule::default();
            one_window(&mut s, &sim, ms(45.0), with_job.then_some(&job));
            let renders = job.borrow().landed.len();
            let calls = sim.borrow().handled.len();
            (calls, renders)
        };
        let (alone, _) = window(false);
        let (calls, renders) = window(true);
        assert!(calls * 10 >= alone * 7, "{calls} calls with the job, {alone} without");
        assert!(renders >= 6, "{renders} renders landed among the calls");
    }

    /// Over whole runs: a job with renders on the GPU keeps the display's rate at 60 Hz and the
    /// 60 fps floor under a swarm at 240 Hz, frames no later than without it, and lands many
    /// renders a frame at a 20 fps cap.
    #[test]
    fn a_waiting_job_keeps_frames_on_time_and_the_floor() {
        // 60 Hz, one sequential agent.
        let sim = Sim::new(ms(0.7));
        sim.borrow_mut().push(Duration::ZERO, ms(0.1), Class::Edit, Some(0));
        let job = SimJob::new(&sim, ms(3.0), usize::MAX);
        let mut s = Schedule::default();
        s.set_display_refresh(Some(HZ60));
        sim_loop_with(&mut s, &sim, HZ60, ms(1.0), 0.5, Some(&job));
        let run = sim_loop_with(&mut s, &sim, HZ60, ms(1.0), 2.0, Some(&job));
        assert!(run.fps > 60.0 * 0.97 && run.late * 50 <= run.frames && run.calls_per_s > 300.0, "{run:?}");
        assert!(job.borrow().landed.len() as u64 > run.frames * 2, "{} renders in {} frames", job.borrow().landed.len(), run.frames);
        // 240 Hz, a swarm of eight: the floor holds.
        let sim = Sim::new(ms(0.7));
        for client in 0..8 {
            sim.borrow_mut().push(Duration::ZERO, ms(0.3), Class::Edit, Some(client));
        }
        let job = SimJob::new(&sim, ms(3.0), usize::MAX);
        let mut s = Schedule::default();
        s.set_display_refresh(Some(HZ240));
        sim_loop_with(&mut s, &sim, HZ240, ms(1.0), 0.5, Some(&job));
        let run = sim_loop_with(&mut s, &sim, HZ240, ms(1.0), 2.0, Some(&job));
        assert!(s.busy() && (55.0..=65.0).contains(&run.fps) && run.late * 20 <= run.frames, "{run:?}");
        // A 20 fps cap at 60 Hz: many renders a frame, where once a frame landed one.
        let sim = Sim::new(Duration::ZERO);
        let job = SimJob::new(&sim, ms(3.0), usize::MAX);
        let mut s = Schedule::default();
        s.set_display_refresh(Some(HZ60));
        s.set_max_fps(Some(20));
        sim_loop_with(&mut s, &sim, HZ60, ms(1.0), 0.5, Some(&job));
        let before = job.borrow().landed.len();
        let run = sim_loop_with(&mut s, &sim, HZ60, ms(1.0), 2.0, Some(&job));
        let renders = (job.borrow().landed.len() - before) as u64;
        assert!(run.late * 50 <= run.frames && renders > run.frames * 5, "{renders} renders in {} frames: {run:?}", run.frames);
    }

    /// Before this schedule, one agent that waits for each reply got one call per frame (60/s
    /// at 60 Hz). Now it gets many, and however fast it is, it never makes a backlog: the
    /// human's window keeps the display's full rate and its vblanks.
    #[test]
    fn one_sequential_agent_gets_over_300_calls_a_second_and_keeps_the_display_rate() {
        for refresh in [HZ60, HZ240] {
            for rtt in [ms(0.05), ms(0.7)] {
                let sim = Sim::new(rtt);
                sim.borrow_mut().push(Duration::ZERO, ms(0.1), Class::Edit, Some(0));
                let mut s = Schedule::default();
                s.set_display_refresh(Some(refresh));
                sim_loop(&mut s, &sim, refresh, ms(1.0), 0.5);
                let run = sim_loop(&mut s, &sim, refresh, ms(1.0), 2.0);
                eprintln!("{refresh:?} display, round trip {rtt:?}: {run:?}");
                let hz = 1.0 / refresh.as_secs_f64();
                assert!(run.calls_per_s > 300.0, "{run:?} at {refresh:?}, round trip {rtt:?}");
                assert!(run.fps > hz * 0.97, "{run:?} at {refresh:?}, round trip {rtt:?}");
                assert!(run.late * 50 <= run.frames, "{run:?} at {refresh:?}, round trip {rtt:?}");
                assert_eq!(s.busy_windows, 0, "one agent made a backlog at {refresh:?}, round trip {rtt:?}");
                // Reviewer finding: waiting past the window for calls that then can't fit costs
                // frame time for nothing, and a caution kept high takes window time. (A call that
                // comes in just before the deadline and doesn't fit is held, and the frame starts
                // at once, no later than it would have: a few of those are fine.)
                assert!(run.held * 5 <= run.frames, "{run:?} at {refresh:?}, round trip {rtt:?}");
                assert!(run.caution < 1.5, "{run:?} at {refresh:?}, round trip {rtt:?}");
            }
        }
    }

    /// A slow agent (round trips around 1.2 ms, about a 240 Hz window) is served about once a
    /// frame, without calls held back or frames pushed late.
    #[test]
    fn a_slow_agent_is_served_without_holding_frames_back() {
        let sim = Sim::new(ms(1.2));
        sim.borrow_mut().push(Duration::ZERO, ms(0.1), Class::Edit, Some(0));
        let mut s = Schedule::default();
        s.set_display_refresh(Some(HZ240));
        sim_loop(&mut s, &sim, HZ240, ms(1.0), 0.5);
        let run = sim_loop(&mut s, &sim, HZ240, ms(1.0), 2.0);
        eprintln!("240 Hz display, round trip about 1.2 ms: {run:?}");
        assert!(run.calls_per_s > 0.9 * run.fps, "{run:?}");
        assert!(run.late * 50 <= run.frames && run.held * 20 <= run.frames && run.caution < 1.5, "{run:?}");
    }

    /// A raised caution shrinks the window; waits must not then run later in turn, or frames
    /// stay late and the caution never comes down (seen live on a hybrid-GPU laptop).
    #[test]
    fn a_raised_caution_comes_back_down() {
        for rtt in [None, Some(ms(1.2))] {
            let sim = Sim::new(rtt.unwrap_or(Duration::ZERO));
            if rtt.is_some() {
                sim.borrow_mut().push(Duration::ZERO, ms(0.1), Class::Edit, Some(0));
            }
            let mut s = Schedule::default();
            s.set_display_refresh(Some(HZ240));
            sim_loop(&mut s, &sim, HZ240, ms(1.0), 0.1);
            s.caution = MAX_CAUTION;
            let run = sim_loop(&mut s, &sim, HZ240, ms(1.0), 2.0);
            // (A wake-up 2 ms late now and then still nudges it up a little.)
            assert!(run.late * 50 <= run.frames, "{run:?}, round trip {rtt:?}");
            assert!(s.caution < 1.5, "caution {}, round trip {rtt:?}", s.caution);
        }
    }

    /// Eight agents whose calls take 0.3 ms each keep the queue full: at 240 Hz the loop drops to
    /// 60 fps on whole vblanks and agents get most of the time.
    #[test]
    fn a_swarm_backlog_trades_the_display_rate_down_to_60_fps() {
        let sim = Sim::new(ms(0.7));
        for client in 0..8 {
            sim.borrow_mut().push(Duration::ZERO, ms(0.3), Class::Edit, Some(client));
        }
        let mut s = Schedule::default();
        s.set_display_refresh(Some(HZ240));
        sim_loop(&mut s, &sim, HZ240, ms(1.0), 0.5);
        let run = sim_loop(&mut s, &sim, HZ240, ms(1.0), 2.0);
        eprintln!("240 Hz display, 8 agents: {run:?}");
        assert!(s.busy());
        // Serving all the time would be 3333 calls/s.
        assert!(run.calls_per_s > 2500.0, "{run:?}");
        assert!((55.0..=65.0).contains(&run.fps), "{run:?}");
        assert!(run.late * 20 <= run.frames, "{run:?}");
        // And when the swarm stops, the display's rate comes back.
        sim.borrow_mut().queue.clear();
        sim_loop(&mut s, &sim, HZ240, ms(1.0), 0.2);
        let idle = sim_loop(&mut s, &sim, HZ240, ms(1.0), 1.0);
        assert!(!s.busy() && idle.fps > 235.0, "{idle:?}");
    }

    /// 13 ms windows on a 60 Hz display serving a queue: how many it takes to handle everything.
    fn windows_to_drain(sim: &Shared, s: &mut Schedule) -> u32 {
        let mut inbox = SimInbox(Rc::clone(sim));
        let mut held = VecDeque::new();
        let mut windows = 0;
        while !sim.borrow().queue.is_empty() || !held.is_empty() {
            let deadline = inbox.now() + ms(13.0);
            let handler = Rc::clone(sim);
            s.serve(&mut inbox, &mut held, deadline, |c| c.class, |c| handler.borrow_mut().handle(c), || false);
            // Frame work, then a present that blocks until the next 60 Hz vblank.
            let mut st = sim.borrow_mut();
            st.t = HZ60 * ((st.t + ms(3.0)).as_nanos() / HZ60.as_nanos() + 1) as u32;
            drop(st);
            windows += 1;
            assert!(windows < 1000, "never drained");
        }
        windows
    }

    /// Reviewer finding: one 500 ms call then 2,000 cheap ones queued got exactly one call per
    /// window for ~56 windows (one cost estimate for every light call, decaying per call).
    #[test]
    fn one_slow_call_does_not_hold_back_the_queue_behind_it() {
        // Another class: not held back at all.
        let sim = Sim::new(Duration::ZERO);
        sim.borrow_mut().push(Duration::ZERO, ms(500.0), Class::Sim, None);
        for _ in 0..2000 {
            sim.borrow_mut().push(Duration::ZERO, ms(0.01), Class::Read, None);
        }
        let windows = windows_to_drain(&sim, &mut Schedule::default());
        eprintln!("drained in {windows} windows");
        assert!(windows <= 4, "{windows} windows");
        // The same class: its estimate halves every 50 ms, so the queue moves within a few frames.
        let sim = Sim::new(Duration::ZERO);
        for i in 0..2001 {
            sim.borrow_mut().push(Duration::ZERO, if i == 0 { ms(500.0) } else { ms(0.01) }, Class::Edit, None);
        }
        let windows = windows_to_drain(&sim, &mut Schedule::default());
        eprintln!("drained in {windows} windows");
        assert!(windows <= 20, "{windows} windows");
        let order = sim.borrow().handled.clone();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "calls ran out of order");
    }

    #[test]
    fn a_call_that_would_run_past_the_deadline_waits_at_the_head_of_the_queue_unless_first() {
        let sim = Sim::new(Duration::ZERO);
        for class in [Class::Read, Class::Render, Class::Read] {
            sim.borrow_mut().push(Duration::ZERO, ms(0.1), class, None);
        }
        let mut s = Schedule::default();
        s.costs[Class::Render as usize] = (Duration::from_secs(5), Some(sim.borrow().origin));
        let (mut inbox, mut held) = (SimInbox(Rc::clone(&sim)), VecDeque::new());
        let handler = Rc::clone(&sim);
        let mut handle = |c: Call| handler.borrow_mut().handle(c);
        let deadline = inbox.now() + ms(50.0);
        s.serve(&mut inbox, &mut held, deadline, |c| c.class, &mut handle, || false);
        assert_eq!((sim.borrow().handled.clone(), held.iter().map(|c| c.id).collect::<Vec<_>>()), (vec![0], vec![1, 2]));
        // The next window starts with it, however costly, then goes on in order.
        let deadline = inbox.now() + ms(50.0);
        s.serve(&mut inbox, &mut held, deadline, |c| c.class, &mut handle, || false);
        assert_eq!((sim.borrow().handled.clone(), held.len()), (vec![0, 1, 2], 0));
        assert_eq!((s.calls, s.held, s.windows), (3, 1, 2));
    }

    #[test]
    fn two_calls_queued_as_windows_end_make_a_backlog_and_one_does_not() {
        for (queued, busy) in [(1, false), (2, true)] {
            let sim = Sim::new(Duration::ZERO);
            let mut s = Schedule::default();
            let (mut inbox, mut held) = (SimInbox(Rc::clone(&sim)), VecDeque::new());
            for _ in 0..BUSY_AFTER {
                // A window whose deadline has passed handles nothing new: `queued` calls wait.
                for _ in 0..queued {
                    let t = sim.borrow().t;
                    sim.borrow_mut().push(t, ms(0.1), Class::Read, None);
                }
                let handler = Rc::clone(&sim);
                let deadline = inbox.now();
                s.serve(&mut inbox, &mut held, deadline, |c| c.class, |c| handler.borrow_mut().handle(c), || false);
                assert_eq!(held.len(), queued);
                held.clear();
            }
            assert_eq!(s.busy(), busy, "{queued} queued");
        }
    }

    #[test]
    fn class_costs_decay_to_their_guess() {
        let mut s = Schedule::default();
        let t = Instant::now();
        s.costed(Class::Read, ms(100.0), t);
        assert_eq!(s.cost(Class::Read, t), ms(100.0));
        assert!(s.cost(Class::Read, t + COST_HALF_LIFE).abs_diff(ms(50.0)) < ms(0.01));
        assert_eq!(s.cost(Class::Read, t + Duration::from_secs(10)), Class::Read.floor());
        assert_eq!(Class::Read.floor(), Duration::from_micros(50));
        assert_eq!(s.cost(Class::Render, t), Class::Render.guess());
    }

    #[test]
    fn serving_stops_when_asked_and_when_every_sender_is_gone() {
        let (tx, rx) = mpsc::channel::<u32>();
        let mut s = Schedule::default();
        let far = Instant::now() + Duration::from_secs(10);
        let mut inbox = &rx;
        assert_eq!(s.serve(&mut inbox, &mut VecDeque::new(), far, |_| Class::Read, |_| {}, || true), Served::Stopped);
        drop(tx);
        assert_eq!(s.serve(&mut inbox, &mut VecDeque::new(), far, |_| Class::Read, |_| {}, || false), Served::Disconnected);
    }

    /// With nothing to serve, a window is a wait or two (no spinning) that ends near its deadline.
    #[test]
    fn an_idle_window_sleeps_until_its_deadline() {
        let _timer = specula_platform::timer::FineTimer::request();
        let (_tx, rx) = mpsc::channel::<u32>();
        let mut s = Schedule::default();
        let mut wakes = 0;
        let start = Instant::now();
        let stop = || {
            wakes += 1;
            false
        };
        let end = s.serve(&mut &rx, &mut VecDeque::new(), start + ms(30.0), |_| Class::Read, |_| {}, stop);
        let took = start.elapsed();
        assert_eq!(end, Served::Window);
        assert!(took >= ms(27.0), "{took:?}");
        assert!(wakes <= 3, "{wakes} wakes");
    }

    #[test]
    fn waits_are_whole_milliseconds() {
        assert_eq!(whole_ms(ms(2.7).saturating_sub(TIMER_SLOP)), ms(1.0));
        assert_eq!(whole_ms(ms(1.9).saturating_sub(TIMER_SLOP)), Duration::ZERO);
    }

    // ---------------------------------------------------------------------------------------
    // The same against real time and threads. Timing-sensitive (a loaded or small machine fails
    // them), so ignored by default: `cargo test -p specula --lib schedule -- --ignored --nocapture`.

    /// A call and where its reply goes, like the engine's.
    struct RealCall {
        n: u64,
        reply: Sender<u64>,
    }

    /// The windowed loop against a fake display in real time: frame work takes 1 to 1.5 ms, and
    /// `present` sleeps until the next vblank. `clients` threads call back to back, each waiting
    /// for its reply; handling a call takes `cost`. Returns (calls per second, frames, frames
    /// later than their period).
    fn real_display(refresh: Duration, clients: usize, cost: Duration, run_for: Duration) -> (f64, u64, u64) {
        let (to_engine, requests) = mpsc::channel::<RealCall>();
        let threads: Vec<_> = (0..clients)
            .map(|_| {
                let to_engine = to_engine.clone();
                std::thread::spawn(move || {
                    let (reply_tx, replies) = mpsc::channel();
                    let start = Instant::now();
                    let mut calls = 0u64;
                    while start.elapsed() < run_for {
                        to_engine.send(RealCall { n: calls, reply: reply_tx.clone() }).unwrap();
                        assert_eq!(replies.recv().unwrap(), calls);
                        calls += 1;
                    }
                    calls
                })
            })
            .collect();
        drop(to_engine);
        let mut s = Schedule::default();
        s.set_display_refresh(Some(refresh));
        let mut held = VecDeque::new();
        let origin = Instant::now();
        let (mut frames, mut late, mut last_vblank) = (0u64, 0u64, None);
        let mut seed = 0x2545_f491_u32;
        let handle = |c: RealCall| {
            let t = Instant::now();
            while t.elapsed() < cost {}
            c.reply.send(c.n).unwrap();
        };
        loop {
            let served_from = Instant::now();
            let end = s.serve(&mut &requests, &mut held, served_from + s.window(), |_| Class::Edit, handle, || false);
            if end == Served::Disconnected {
                break;
            }
            let served = served_from.elapsed();
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let period = s.period();
            std::thread::sleep(Duration::from_micros(1000 + u64::from(seed % 500))); // frame work
            let vblank = (origin.elapsed().as_nanos() / refresh.as_nanos()) as u32 + 1;
            let blocked = (origin + refresh * vblank).saturating_duration_since(Instant::now());
            std::thread::sleep(blocked);
            frames += 1;
            late += last_vblank.is_some_and(|v| refresh * (vblank - v) > period + refresh / 2) as u64;
            last_vblank = Some(vblank);
            s.presented(frames, served, blocked, Instant::now());
        }
        let calls: u64 = threads.into_iter().map(|t| t.join().unwrap()).sum();
        (calls as f64 / origin.elapsed().as_secs_f64(), frames, late)
    }

    #[test]
    #[ignore = "real time: run on an idle machine with -- --ignored --nocapture"]
    fn real_time_one_sequential_client() {
        let _timer = specula_platform::timer::FineTimer::request();
        for refresh in [HZ60, HZ240] {
            let (rate, frames, late) = real_display(refresh, 1, Duration::ZERO, Duration::from_millis(1500));
            eprintln!("{refresh:?} display: one sequential client {rate:.0} calls/s; {frames} frames, {late} late");
            assert!(rate > 300.0, "{rate:.0} calls/s at a {refresh:?} refresh");
            assert!(late * 10 <= frames, "{late} of {frames} frames late at a {refresh:?} refresh");
        }
    }

    #[test]
    #[ignore = "real time: run on an idle machine with -- --ignored --nocapture"]
    fn real_time_swarm_backlog() {
        let _timer = specula_platform::timer::FineTimer::request();
        let run_for = Duration::from_millis(1500);
        let (rate, frames, late) = real_display(HZ240, 8, ms(0.3), run_for);
        let fps = frames as f64 / run_for.as_secs_f64();
        eprintln!("240 Hz display, 8 clients at 0.3 ms a call: {rate:.0} calls/s; {fps:.0} fps, {late} of {frames} late");
        assert!(rate > 2000.0, "{rate:.0} calls/s");
        assert!((45.0..=80.0).contains(&fps), "{fps:.0} fps");
        assert!(late * 10 <= frames, "{late} of {frames} frames late");
    }

    /// Light review 5, finding 1: "a pulse starts only when it fits before the deadline" was
    /// held by no test (with the check taken out every test passed: they looked at where looks
    /// end, and a 0.6 ms landing fitted in the margin they allowed). Here a landing costs 5 ms,
    /// more than any margin, and a pulse comes due about every millisecond of four 45 ms windows:
    /// none starts later than the deadline less what a landing costs, so every landing ends by
    /// the deadline; a pulse due after that line is left out (counted); and the last pulse of
    /// each window started within a landing and a look of the line (it had something to refuse).
    #[test]
    fn no_pulse_starts_without_room_for_a_landing() {
        let sim = Sim::new(Duration::ZERO);
        let job = SimJob::new(&sim, ms(3.0), usize::MAX);
        let land = ms(5.0);
        job.borrow_mut().land = land;
        let mut s = Schedule::default();
        let mut w = SimWindow::new(&sim, Some(&job), true);
        for window in 0..4 {
            let (left_out, start) = (s.pulses_left_out, sim.borrow().t);
            let deadline = one_window_in(&mut s, &mut w, ms(45.0));
            let line = deadline - land;
            let starts: Vec<Duration> = w.pulse_starts.iter().copied().filter(|t| *t >= start).collect();
            assert!(starts.len() >= 8, "window {window}: {} pulses", starts.len());
            assert!(starts.iter().all(|t| *t <= line), "window {window}: a pulse started past {line:?}, the deadline less a landing: {starts:?}");
            assert!(job.borrow().ends.iter().all(|e| *e <= deadline), "window {window}: a landing ended past the deadline");
            assert!(s.pulses_left_out > left_out, "window {window}: no pulse was left out");
            let last = *starts.last().unwrap();
            assert!(last + land + job.borrow().gpu + TIMER_SLOP * 2 >= line, "window {window}: the last pulse ({last:?}) was not near the line ({line:?})");
        }
    }

    /// Light review 5, finding 3: "frames are no later for pulses" was not true when renders are
    /// slow and landing one is costly. A pulse was charged what pulses cost lately, halving every
    /// 50 ms, so a render slower than that landed in a pulse that had room for a look only; and
    /// held to 20 frames a second a pulse landed one render in the window and the frame's own
    /// pump a second, so the frame paid for two. Now a pulse is charged what a landing costs
    /// (the last landing's cost, not forgotten with time), and the frame's own pump leaves alone
    /// a job the window's pulses looked at. The review's rows and more, four seconds each after
    /// half a second: at 60 Hz and held to 20 frames a second, with pulses no more frames are
    /// late than with the frame's own pump alone, and no fewer renders land. (The simulation runs
    /// the frame's pump as the engine does: after the window, in the frame's own time.)
    #[test]
    fn pulses_make_no_frame_later_than_the_frames_own_pump() {
        let mut table = String::new();
        for (gpu, land) in [(3.0, 0.6), (3.0, 4.0), (40.0, 4.0), (100.0, 6.0), (100.0, 10.0), (300.0, 10.0)] {
            for cap in [None, Some(20)] {
                let run = |pulses: bool| {
                    let sim = Sim::new(Duration::ZERO);
                    let job = SimJob::new(&sim, ms(gpu), usize::MAX);
                    job.borrow_mut().land = ms(land);
                    let mut s = Schedule::default();
                    s.set_display_refresh(Some(HZ60));
                    s.set_max_fps(cap);
                    let mut w = SimWindow::new(&sim, Some(&job), pulses);
                    sim_frames(&mut s, &sim, HZ60, ms(1.0), 0.5, &mut w);
                    let before = job.borrow().landed.len();
                    let run = sim_frames(&mut s, &sim, HZ60, ms(1.0), 4.0, &mut w);
                    let renders = job.borrow().landed.len() - before;
                    (run.late, run.frames, renders, s.pulses)
                };
                let (alone, with) = (run(false), run(true));
                table.push_str(&format!("render {gpu} ms, landing {land} ms, cap {cap:?}: pump alone {} of {} late, {} renders; with pulses {} of {} late, {} renders, {} pulses\n", alone.0, alone.1, alone.2, with.0, with.1, with.2, with.3));
                assert!(with.0 <= alone.0 && with.2 >= alone.2, "render {gpu} ms, landing {land} ms, cap {cap:?}:\n{table}");
            }
        }
        eprintln!("{table}");
    }
}
