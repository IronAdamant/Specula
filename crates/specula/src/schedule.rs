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
//! windows sized for it stretch the gaps.) **Under
//! a backlog** (calls still queued as windows end, four windows running) the loop presents on
//! every n-th vblank, n the most that keeps 60 fps (4 at 240 Hz, 1 at 60 Hz), and agents get the
//! rest of each period, at least a quarter of it. A present usually finds a free image then and
//! returns at once, so with n > 1 the window is steered by feedback instead: each frame it moves
//! half the way to calling `present` a margin before the period is up (an early frame waits in
//! the swapchain's queue for its vblank). A single fast agent can
//! make a backlog too (its next call already waiting as windows end); one that pauses between
//! calls keeps the display's full rate.
//!
//! **Late frames** (the gap over the period plus half a refresh) raise the caution. A late frame
//! whose present blocked over a refresh was submitted before its vblank and skipped by the display
//! or compositor (it happens without any serving, e.g. on hybrid-GPU laptops): it isn't counted
//! against the window.
//!
//! **Calls.** A call starts only as the first of a window or when the window still has room for
//! the longest call of its kind lately (heavy ones, [`Schedule::serve`]'s `heavy`: captures,
//! saves, loads; and the rest); otherwise it waits, at the head of the queue, for the next window.
//! So a capture lands where the old loop ran it (right after a present) rather than pushing a frame
//! past its vblank. The order calls are applied in never changes: a held call is the next handled.

use specula_core::Json;
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
/// Under a backlog, frames come at least this often (60 fps), on whole vblanks.
const BUSY_FRAME: Duration = Duration::from_micros(16_667);
/// Windows ending with calls queued, in a row, that make a backlog (and as many clear ones end it).
const BUSY_AFTER: u8 = 4;
/// What a heavy call is assumed to cost before one has been timed.
const HEAVY_GUESS: Duration = Duration::from_millis(8);
/// What any other call is assumed to cost before one has been timed.
const LIGHT_GUESS: Duration = Duration::from_micros(200);
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

/// The median of `v` (not empty, at most [`PROBES`] long).
fn median(v: &[Duration]) -> Duration {
    let mut sorted = [Duration::ZERO; PROBES];
    let sorted = &mut sorted[..v.len()];
    sorted.copy_from_slice(v);
    sorted.sort_unstable();
    sorted[v.len() / 2]
}

/// `d` rounded down to whole milliseconds.
fn whole_ms(d: Duration) -> Duration {
    Duration::from_millis(d.as_millis() as u64)
}

/// Why [`Schedule::serve`] returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Served {
    /// The window ended (or a call is held for the next one).
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
    /// The longest heavy call lately (decays by 1/8 per heavy call).
    heavy_cost: Duration,
    /// The longest other call lately (decays by 1/16 per call).
    light_cost: Duration,
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
    /// Windows that ended with calls queued, counted up to `2 * BUSY_AFTER` and down by clear
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
}

impl Default for Schedule {
    fn default() -> Schedule {
        Schedule {
            serve: MIN_SERVE,
            sized: MIN_SERVE,
            sized_for: REFRESH_GUESS,
            heavy_cost: HEAVY_GUESS,
            light_cost: LIGHT_GUESS,
            last_frame: (0, None),
            display: None,
            refresh: None,
            probes: [Duration::ZERO; PROBES],
            probe_count: 0,
            probing: true,
            since_probe: 0,
            work: Duration::ZERO,
            caution: 1.0,
            pressure: 0,
            windows: 0,
            calls: 0,
            held: 0,
            late_presents: 0,
            skipped_presents: 0,
            unblocked_presents: 0,
            busy_windows: 0,
        }
    }
}

impl Schedule {
    /// perf_frame's `serve` section.
    pub fn to_json(self) -> Json {
        let ms = |d: Duration| Json::from((d.as_secs_f64() * 1e6).round() / 1e3);
        Json::obj([
            ("window_ms", ms(self.sized)),
            ("frame_period_ms", ms(self.period())),
            ("refresh_ms", ms(self.refresh())),
            ("refresh_from", Json::from(if self.display.is_some() { "display" } else if self.refresh.is_some() { "probes" } else { "guess" })),
            ("frame_work_ms", ms(self.work)),
            ("margin_ms", ms(self.margin())),
            ("backlog", Json::from(self.busy())),
            ("heavy_cost_ms", ms(self.heavy_cost)),
            ("light_cost_ms", ms(self.light_cost)),
            ("windows", Json::from(self.windows)),
            ("backlog_windows", Json::from(self.busy_windows)),
            ("calls", Json::from(self.calls)),
            ("held_for_next_window", Json::from(self.held)),
            ("late_presents", Json::from(self.late_presents)),
            ("late_but_ready", Json::from(self.skipped_presents)),
            ("unblocked_presents", Json::from(self.unblocked_presents)),
        ])
    }

    /// The current serve window.
    pub fn window(&self) -> Duration {
        self.serve
    }

    fn refresh(&self) -> Duration {
        self.display.or(self.refresh).unwrap_or(REFRESH_GUESS)
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
    /// backlog the most whole refreshes that keep 60 fps.
    pub fn period(&self) -> Duration {
        let refresh = self.refresh();
        if !self.busy() {
            return refresh;
        }
        // A little slack, so 240 Hz measured at 4.17 ms still gives 4.
        let n = (BUSY_FRAME + refresh / 8).as_nanos() / refresh.as_nanos();
        refresh * n.max(1) as u32
    }

    /// How long `present` should still block: a quarter of the refresh interval (0.5 to 3 ms)
    /// times the caution, at most a refresh.
    pub fn margin(&self) -> Duration {
        let refresh = self.refresh();
        (refresh / 4).clamp(MIN_MARGIN, MAX_MARGIN).mul_f32(self.caution).min(refresh)
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
            let base_margin = (refresh / 4).clamp(MIN_MARGIN, MAX_MARGIN);
            let ready = gap.saturating_sub(blocked).as_secs_f64();
            let error = (period - base_margin).as_secs_f64() - ready;
            Duration::from_secs_f64((self.sized.as_secs_f64() + error / 2.0).max(0.0))
        } else {
            period.saturating_sub(self.work + self.margin())
        };
        self.sized = sized.clamp(least, MAX_SERVE.max(least));
        self.sized_for = period;
        self.serve = self.sized;
        // Under a backlog frames skip vblanks on purpose: no probes (the interval is kept).
        self.since_probe += 1;
        let measure = self.display.is_none() && !self.busy();
        if measure && (self.since_probe >= PROBE_EVERY || self.probe_count < FIRST_PROBES) {
            self.since_probe = 0;
            self.serve = MIN_SERVE;
            self.probing = true;
        }
    }

    /// A window ended; `backlog`: calls were still queued.
    fn ended(&mut self, backlog: bool) -> Served {
        if self.busy() {
            self.busy_windows += 1;
        }
        self.pressure = if backlog { (self.pressure + 1).min(2 * BUSY_AFTER) } else { self.pressure.saturating_sub(1) };
        Served::Window
    }

    /// Serves calls from `requests` (oldest first; `held` before any) until `deadline`, waking
    /// as each arrives; the window ends by the deadline (a timed wait may end up to half the
    /// margin after it) unless a call is still being handled.
    /// `heavy` marks calls that may take long; `handle` applies one; `stop` is checked between
    /// calls (a wait never outlasts the window). Never busy-waits.
    pub fn serve<R>(
        &mut self,
        requests: &Receiver<R>,
        held: &mut Option<R>,
        deadline: Instant,
        heavy: impl Fn(&R) -> bool,
        mut handle: impl FnMut(R),
        mut stop: impl FnMut() -> bool,
    ) -> Served {
        self.windows += 1;
        let mut first = true;
        loop {
            if stop() {
                return Served::Stopped;
            }
            let now = Instant::now();
            let request = match held.take() {
                Some(r) => r,
                None if now >= deadline => {
                    // Over: whether calls are queued decides the backlog (the next window starts
                    // with the oldest).
                    return match requests.try_recv() {
                        Ok(r) => {
                            *held = Some(r);
                            self.ended(true)
                        }
                        Err(TryRecvError::Empty) => self.ended(false),
                        Err(TryRecvError::Disconnected) => Served::Disconnected,
                    };
                }
                None => {
                    // A timed wait ends on the OS timer tick after its timeout (up to a tick
                    // late; 1 ms with `FineTimer`), and std rounds timeouts up to whole
                    // milliseconds: wait whole milliseconds that end by the deadline plus half the
                    // margin (which is there for late wake-ups), then only take what has already
                    // arrived. A call wakes a wait at once, whatever the timer.
                    let latest = deadline + self.margin() / 2;
                    let wait = whole_ms((latest - now).saturating_sub(TIMER_SLOP));
                    if wait.is_zero() {
                        match requests.try_recv() {
                            Ok(r) => r,
                            Err(TryRecvError::Empty) => return self.ended(false),
                            Err(TryRecvError::Disconnected) => return Served::Disconnected,
                        }
                    } else {
                        match requests.recv_timeout(wait) {
                            Ok(r) => r,
                            Err(RecvTimeoutError::Timeout) => continue,
                            Err(RecvTimeoutError::Disconnected) => return Served::Disconnected,
                        }
                    }
                }
            };
            // A call that would likely run past the deadline waits for the next window (unless
            // it is this window's first: every window makes progress).
            let is_heavy = heavy(&request);
            let cost = if is_heavy { self.heavy_cost } else { self.light_cost };
            if !first && Instant::now() + cost > deadline {
                *held = Some(request);
                self.held += 1;
                return self.ended(true);
            }
            first = false;
            let started = Instant::now();
            handle(request);
            self.calls += 1;
            let took = started.elapsed();
            if is_heavy {
                self.heavy_cost = took.max(self.heavy_cost - self.heavy_cost / 8);
            } else {
                self.light_cost = took.max(self.light_cost - self.light_cost / 16);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn a_heavy_call_waits_at_the_head_of_the_queue_and_order_is_kept() {
        let (tx, rx) = mpsc::channel();
        // 1 light, 2 heavy (it doesn't fit the window), 3 light.
        for r in [1, 2, 3] {
            tx.send(r).unwrap();
        }
        let mut s = Schedule { heavy_cost: Duration::from_secs(5), ..Schedule::default() };
        let mut held = None;
        let mut order = Vec::new();
        let deadline = Instant::now() + ms(50.0);
        let end = s.serve(&rx, &mut held, deadline, |r| *r == 2, |r| order.push(r), || false);
        assert_eq!((end, order.as_slice(), held), (Served::Window, &[1][..], Some(2)));
        // The next window starts with it, however heavy, then goes on in order.
        let deadline = Instant::now() + ms(5.0);
        let end = s.serve(&rx, &mut held, deadline, |r| *r == 2, |r| order.push(r), || false);
        assert_eq!((end, order.as_slice(), held), (Served::Window, &[1, 2, 3][..], None));
        assert_eq!((s.calls, s.held, s.windows, s.pressure), (3, 1, 2, 0));
    }

    #[test]
    fn any_call_that_would_run_past_the_deadline_waits_unless_it_is_the_first() {
        let (tx, rx) = mpsc::channel();
        for r in [1, 2] {
            tx.send(r).unwrap();
        }
        let mut s = Schedule { light_cost: Duration::from_secs(5), ..Schedule::default() };
        let (mut held, mut order) = (None, Vec::new());
        let end = s.serve(&rx, &mut held, Instant::now() + ms(50.0), |_| false, |r| order.push(r), || false);
        assert_eq!((end, order.as_slice(), held), (Served::Window, &[1][..], Some(2)));
        // What it really costs is learned (a decaying maximum).
        assert!(s.light_cost < Duration::from_secs(5));
    }

    #[test]
    fn windows_ending_with_calls_queued_make_a_backlog() {
        let (tx, rx) = mpsc::channel();
        let mut s = Schedule::default();
        let mut held = None;
        for round in 0..BUSY_AFTER {
            assert!(!s.busy());
            for r in 0..3 {
                tx.send(round * 10 + r).unwrap();
            }
            // A deadline already past: one call, and the next is held.
            let end = s.serve(&rx, &mut held, Instant::now(), |_| false, |_| {}, || false);
            assert_eq!(end, Served::Window);
            assert!(held.is_some());
            while rx.try_recv().is_ok() {}
            held = None;
        }
        assert!(s.busy());
        // Clear windows end it.
        for _ in 0..BUSY_AFTER {
            s.serve(&rx, &mut held, Instant::now(), |_| false, |_| {}, || false);
        }
        assert!(!s.busy());
    }

    #[test]
    fn serving_stops_when_asked_and_when_every_sender_is_gone() {
        let (tx, rx) = mpsc::channel::<u32>();
        let mut s = Schedule::default();
        let far = Instant::now() + Duration::from_secs(10);
        assert_eq!(s.serve(&rx, &mut None, far, |_| false, |_| {}, || true), Served::Stopped);
        drop(tx);
        assert_eq!(s.serve(&rx, &mut None, far, |_| false, |_| {}, || false), Served::Disconnected);
    }

    /// With nothing to serve, a window is a wait or two (no spinning) that ends by its deadline.
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
        let end = s.serve(&rx, &mut None, start + ms(30.0), |_| false, |_| {}, stop);
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

    /// A call and where its reply goes, like the engine's.
    struct Call {
        n: u64,
        reply: Sender<u64>,
    }

    /// The windowed loop against a fake display: frame work takes 1 to 1.5 ms, and `present`
    /// blocks until the next vblank (FIFO; a late frame waits for the vblank after). `clients`
    /// threads call back to back, each waiting for its reply, as MCP agents do; handling a call
    /// takes `cost`. Returns (calls per second, frames, frames later than their period).
    fn fake_display(refresh: Duration, clients: usize, cost: Duration, run_for: Duration) -> (f64, u64, u64) {
        let (to_engine, requests) = mpsc::channel::<Call>();
        let threads: Vec<_> = (0..clients)
            .map(|_| {
                let to_engine = to_engine.clone();
                std::thread::spawn(move || {
                    let (reply_tx, replies) = mpsc::channel();
                    let start = Instant::now();
                    let mut calls = 0u64;
                    while start.elapsed() < run_for {
                        to_engine.send(Call { n: calls, reply: reply_tx.clone() }).unwrap();
                        assert_eq!(replies.recv().unwrap(), calls);
                        calls += 1;
                    }
                    calls
                })
            })
            .collect();
        drop(to_engine);
        let mut s = Schedule::default();
        let mut held = None;
        let origin = Instant::now();
        let (mut frames, mut late, mut last_vblank) = (0u64, 0u64, None);
        let mut seed = 0x2545_f491_u32;
        let handle = |c: Call| {
            if !cost.is_zero() {
                let t = Instant::now();
                while t.elapsed() < cost {}
            }
            c.reply.send(c.n).unwrap();
        };
        loop {
            let served_from = Instant::now();
            let end = s.serve(&requests, &mut held, served_from + s.window(), |_| false, handle, || false);
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

    /// Before this schedule, a sequential client got one call per frame (60/s at 60 Hz). Frame
    /// pacing must hold meanwhile: nearly every frame lands on the next vblank (a loaded CI machine
    /// may miss a few).
    #[test]
    fn one_sequential_client_gets_over_300_calls_a_second() {
        // Timed waits wake on the OS timer tick (15.6 ms by default on Windows): ask for 1 ms, as
        // the engine does while its window is open.
        let _timer = specula_platform::timer::FineTimer::request();
        for refresh in [HZ60, HZ240] {
            let (rate, frames, late) = fake_display(refresh, 1, Duration::ZERO, Duration::from_millis(1500));
            eprintln!("{refresh:?} display: one sequential client {rate:.0} calls/s; {frames} frames, {late} late");
            assert!(rate > 300.0, "{rate:.0} calls/s at a {refresh:?} refresh");
            assert!(late * 10 <= frames, "{late} of {frames} frames late at a {refresh:?} refresh");
        }
    }

    /// Eight clients whose calls take 0.3 ms each keep the queue full: at 240 Hz the loop drops to
    /// 60 fps on whole vblanks and agents get most of the time.
    #[test]
    fn a_swarm_backlog_trades_the_display_rate_down_to_60_fps() {
        let _timer = specula_platform::timer::FineTimer::request();
        let run_for = Duration::from_millis(1500);
        let (rate, frames, late) = fake_display(HZ240, 8, ms(0.3), run_for);
        let fps = frames as f64 / run_for.as_secs_f64();
        eprintln!("240 Hz display, 8 clients at 0.3 ms a call: {rate:.0} calls/s; {fps:.0} fps, {late} of {frames} late");
        // Serving all the time would be 3333 calls/s; serving only the margin of every vblank
        // about half that.
        assert!(rate > 2000.0, "{rate:.0} calls/s");
        assert!((45.0..=80.0).contains(&fps), "{fps:.0} fps");
        assert!(late * 10 <= frames, "{late} of {frames} frames late");
    }
}
