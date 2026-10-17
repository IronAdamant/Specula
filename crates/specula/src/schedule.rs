//! When the windowed loop serves agents and when it starts the next frame.
//!
//! FIFO presentation blocks in `present` until the display takes the frame. The loop used to
//! drain the request queue once per frame and then block there, so an agent that waits for each
//! reply before its next call got one call per refresh (60 a second on a 60 Hz display). Now the
//! loop serves requests as they arrive (waking on each one) for a *serve window* after every
//! present, then builds and presents its frame.
//!
//! The window is sized by feedback from each present:
//! - **On time** (the gap since the last present is about one refresh): `present` blocked for the
//!   slack the frame had left. Blocking longer than the margin means the frame could have started
//!   later, so the window grows by half the excess; blocking less shrinks it twice as fast.
//! - **Late** (the gap is over 1.5 refreshes: a vblank was missed) or **not blocked at all**: the
//!   window halves, and the margin grows (×1.5, at most ×4) for a while: frame work that varies a
//!   lot gets more room. Some drivers block until the *next* vblank after a late frame, so a long
//!   block alone doesn't prove the frame was early; the gap does.
//! - **Late but ready**: a late frame whose present blocked over a refresh was submitted before
//!   its vblank and skipped by the display or compositor (it happens without any serving, e.g.
//!   on hybrid-GPU laptops). The window isn't to blame and stays as it is.
//!
//! The margin is a quarter of the refresh interval (0.5 to 3 ms) times that caution. The refresh
//! interval is the median of the last 31 gaps between presents, so no rate is assumed: 60 or 240 Hz, VRR, a GPU-bound frame or an occluded window the compositor
//! throttles all show up in the gaps. In the steady state `present` still blocks about the margin,
//! so the frames keep their vblanks; the time the loop used to sleep in `present` goes to agents.
//!
//! Requests that may take long (captures, saves, loads: [`Schedule::serve`]'s `heavy`) only start
//! as the first request of a window or when the window still has room for the longest one seen
//! lately; otherwise they wait, at the head of the queue, for the next window. So a capture lands
//! where the old loop ran it (right after a present) rather than pushing a frame past its vblank.
//! The order requests are applied in never changes: a held request is the next one handled.

use specula_core::Json;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};

/// How long `present` should still block once the window has adapted (the slack for frame work
/// that varies between frames and for timer wake-ups that come late) is a quarter of the refresh
/// interval, within these bounds, times the caution that late frames raise.
pub const MIN_MARGIN: Duration = Duration::from_micros(500);
pub const MAX_MARGIN: Duration = Duration::from_millis(3);
/// The caution's ceiling (the margin's largest multiple).
const MAX_CAUTION: f32 = 4.0;
/// Per on-time frame, the caution keeps this much of its excess over 1 (half-life ~34 frames).
const CAUTION_KEEP: f32 = 0.98;
/// The smallest serve window: agents always get this much of every frame, even when frames take
/// longer than a refresh (the old loop's drain-everything would have slowed those frames further).
pub const MIN_SERVE: Duration = Duration::from_millis(1);
/// The largest serve window (a present that blocks very long, e.g. an occluded window).
pub const MAX_SERVE: Duration = Duration::from_millis(50);
/// The serve window while the window is minimized (nothing is presented).
pub const MINIMIZED_SERVE: Duration = Duration::from_millis(16);
/// What a heavy request is assumed to cost before one has been timed.
const HEAVY_GUESS: Duration = Duration::from_millis(8);
/// The refresh interval assumed before two presents have been timed.
const REFRESH_GUESS: Duration = Duration::from_micros(16_667);
/// Shorter gaps between presents don't count as the refresh interval (a burst after a stall).
const MIN_REFRESH: Duration = Duration::from_millis(2);
/// Longer gaps between presents (a minimized or stalled window) neither count as the refresh
/// interval nor as a late frame.
const MAX_GAP: Duration = Duration::from_millis(100);
/// How late a timed wait may end after its timeout (one tick of a 1 ms system timer).
const TIMER_SLOP: Duration = Duration::from_millis(1);

/// Recent gaps between presents kept for the refresh interval (their median).
const GAPS: usize = 31;

/// The median of `v` (not empty, at most [`GAPS`] long).
fn median(v: &[Duration]) -> Duration {
    let mut sorted = [Duration::ZERO; GAPS];
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
    /// The window ended (or a heavy request is held for the next one).
    Window,
    /// `stop` said so.
    Stopped,
    /// Every sender is gone.
    Disconnected,
}

/// The windowed loop's serve window and what it learned about the frames.
#[derive(Clone, Copy, Debug)]
pub struct Schedule {
    serve: Duration,
    /// The longest heavy request lately (decays by 1/8 per heavy request).
    heavy_cost: Duration,
    /// The last presented frame's number (presents that showed nothing don't adapt the window)
    /// and when it was presented.
    last_frame: (u64, Option<Instant>),
    /// The refresh interval: the median of the last [`GAPS`] gaps between presents (`None` before
    /// the first). Late frames and the short ones that catch up after them don't move it.
    refresh: Option<Duration>,
    gaps: [Duration; GAPS],
    gap_count: usize,
    /// The margin's multiple, 1 to [`MAX_CAUTION`].
    caution: f32,
    /// Since start: serve windows, calls handled in them, heavy calls held for the next window,
    /// late presents (a missed vblank), the late ones that were ready in time (the display
    /// skipped them) and presents that didn't block.
    windows: u64,
    calls: u64,
    held: u64,
    late_presents: u64,
    skipped_presents: u64,
    unblocked_presents: u64,
}

impl Default for Schedule {
    fn default() -> Schedule {
        Schedule {
            serve: MIN_SERVE,
            heavy_cost: HEAVY_GUESS,
            last_frame: (0, None),
            refresh: None,
            gaps: [Duration::ZERO; GAPS],
            gap_count: 0,
            caution: 1.0,
            windows: 0,
            calls: 0,
            held: 0,
            late_presents: 0,
            skipped_presents: 0,
            unblocked_presents: 0,
        }
    }
}

impl Schedule {
    /// perf_frame's `serve` section.
    pub fn to_json(self) -> Json {
        let ms = |d: Duration| Json::from((d.as_secs_f64() * 1e6).round() / 1e3);
        Json::obj([
            ("window_ms", ms(self.serve)),
            ("margin_ms", ms(self.margin())),
            ("refresh_ms", self.refresh.map_or(Json::Null, ms)),
            ("heavy_cost_ms", ms(self.heavy_cost)),
            ("windows", Json::from(self.windows)),
            ("calls", Json::from(self.calls)),
            ("heavy_held", Json::from(self.held)),
            ("late_presents", Json::from(self.late_presents)),
            ("late_but_ready", Json::from(self.skipped_presents)),
            ("unblocked_presents", Json::from(self.unblocked_presents)),
        ])
    }

    /// The current serve window.
    pub fn window(&self) -> Duration {
        self.serve
    }

    /// How long `present` should still block: a quarter of the refresh interval (0.5 to 3 ms)
    /// times the caution, at most the refresh interval.
    pub fn margin(&self) -> Duration {
        let refresh = self.refresh.unwrap_or(REFRESH_GUESS);
        (refresh / 4).clamp(MIN_MARGIN, MAX_MARGIN).mul_f32(self.caution).min(refresh)
    }

    /// Adapts the window after a present that returned at `now`. `frame` is the presented
    /// frame's number (unchanged = nothing was presented: minimized, or an out-of-date swapchain)
    /// and `blocked` how long that present waited for the GPU and the display.
    pub fn presented(&mut self, frame: u64, blocked: Duration, now: Instant) {
        if frame == self.last_frame.0 {
            return;
        }
        let gap = self.last_frame.1.map(|last| now.saturating_duration_since(last));
        self.last_frame = (frame, Some(now));
        let late = match gap {
            Some(gap) if (MIN_REFRESH..=MAX_GAP).contains(&gap) => {
                let late = self.refresh.is_some_and(|r| gap > r * 3 / 2);
                self.gaps[self.gap_count % GAPS] = gap;
                self.gap_count += 1;
                self.refresh = Some(median(&self.gaps[..self.gap_count.min(GAPS)]));
                late
            }
            _ => false,
        };
        let margin = self.margin();
        if late {
            self.late_presents += 1;
        }
        // A late frame whose present still blocked over a refresh was ready before its vblank (a
        // driver that blocks until the vblank after a late frame blocks less than a refresh): the
        // display or compositor skipped it, not the window. Nothing to learn from it.
        let skipped = late && self.refresh.is_some_and(|r| blocked >= r + margin / 2);
        if skipped {
            self.skipped_presents += 1;
        } else if late || blocked < margin / 4 {
            if !late {
                self.unblocked_presents += 1;
            }
            self.caution = (self.caution * 1.5).min(MAX_CAUTION);
            self.serve /= 2;
        } else {
            self.caution = 1.0 + (self.caution - 1.0) * CAUTION_KEEP;
            self.serve = if blocked >= margin {
                // Approach the slack halfway each frame: smooth, and settled within a few frames.
                self.serve + (blocked - margin) / 2
            } else {
                self.serve.saturating_sub((margin - blocked) * 2)
            };
        }
        self.serve = self.serve.clamp(MIN_SERVE, MAX_SERVE);
    }

    /// Serves requests from `requests` (oldest first; `held` before any) until `deadline`, waking
    /// as each arrives; the window ends by the deadline unless a request is still being handled.
    /// `heavy` marks requests that may take long; `handle` applies one; `stop` is checked between
    /// requests (a wait never outlasts the window). Never busy-waits.
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
                None if now >= deadline => return Served::Window,
                None => {
                    // A timed wait ends on the OS timer tick after its timeout (up to a tick
                    // late; 1 ms with `FineTimer`), and std rounds timeouts up to whole
                    // milliseconds: wait whole milliseconds that end by the deadline, then only
                    // take what has already arrived. A window ends up to ~2 ms early, never late
                    // (the window adapts to the time actually served).
                    let wait = whole_ms((deadline - now).saturating_sub(TIMER_SLOP));
                    if wait.is_zero() {
                        match requests.try_recv() {
                            Ok(r) => r,
                            Err(TryRecvError::Empty) => return Served::Window,
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
            let is_heavy = heavy(&request);
            if is_heavy && !first && Instant::now() + self.heavy_cost > deadline {
                *held = Some(request);
                self.held += 1;
                return Served::Window;
            }
            first = false;
            let started = Instant::now();
            handle(request);
            self.calls += 1;
            if is_heavy {
                let took = started.elapsed();
                self.heavy_cost = took.max(self.heavy_cost - self.heavy_cost / 8);
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

    /// Frames at `refresh` whose present has `slack` of idle time before the vblank once the
    /// window is subtracted: present blocks `slack - window` (or not at all when it overran).
    fn run(s: &mut Schedule, refresh: Duration, slack: Duration, frames: u64) {
        let start = s.last_frame.1.unwrap_or_else(Instant::now);
        for f in 1..=frames {
            let blocked = slack.saturating_sub(s.window());
            s.presented(s.last_frame.0 + 1, blocked, start + refresh * f as u32);
        }
    }

    const HZ60: Duration = Duration::from_micros(16_667);
    const HZ240: Duration = Duration::from_micros(4_167);

    #[test]
    fn the_window_grows_until_present_blocks_about_the_margin() {
        for (refresh, slack, margin) in [(HZ60, ms(13.0), MAX_MARGIN), (HZ240, ms(3.2), HZ240 / 4)] {
            let mut s = Schedule::default();
            run(&mut s, refresh, slack, 200);
            let blocked = slack - s.window();
            let m = s.margin();
            assert!(m.abs_diff(margin) < ms(0.05), "margin {m:?} at {refresh:?}");
            assert!(blocked >= m && blocked < m + ms(0.1), "blocked {blocked:?}, margin {m:?}, window {:?}", s.window());
            assert_eq!(s.refresh, Some(refresh));
        }
    }

    #[test]
    fn the_window_shrinks_fast_when_frames_get_heavier() {
        let mut s = Schedule::default();
        run(&mut s, HZ60, ms(13.0), 30);
        // The frame now takes 6 ms more: the window was too long and present barely blocks.
        run(&mut s, HZ60, ms(7.0), 3);
        assert!(ms(7.0).saturating_sub(s.window()) >= s.margin() / 4, "window {:?}", s.window());
        run(&mut s, HZ60, ms(7.0), 200);
        assert!(ms(7.0) - s.window() >= s.margin() - ms(0.1), "window {:?}", s.window());
    }

    #[test]
    fn a_missed_vblank_halves_the_window_however_long_present_blocked() {
        let mut s = Schedule::default();
        run(&mut s, HZ240, ms(3.2), 200);
        let (w0, m0) = (s.window(), s.margin());
        // Late: the gap is two refreshes, and the driver blocked until the vblank after (less
        // than a refresh, as the frame was submitted after its vblank).
        let at = s.last_frame.1.unwrap() + HZ240 * 2;
        s.presented(s.last_frame.0 + 1, ms(3.5), at);
        assert_eq!(s.window(), (w0 / 2).max(MIN_SERVE));
        assert!(s.margin() > m0, "the margin grows after a late frame");
        assert_eq!((s.late_presents, s.refresh), (1, Some(HZ240)));
        // A late frame that was ready (present blocked over a refresh) changes nothing.
        let (w, m) = (s.window(), s.margin());
        let at = s.last_frame.1.unwrap() + HZ240 * 2;
        s.presented(s.last_frame.0 + 1, HZ240 + ms(1.0), at);
        assert_eq!((s.window(), s.margin(), s.late_presents, s.skipped_presents), (w, m, 2, 1));
        // On-time frames bring the caution back down.
        run(&mut s, HZ240, ms(3.2), 400);
        assert!(s.margin().abs_diff(m0) < ms(0.01), "margin {:?} vs {m0:?}", s.margin());
    }

    #[test]
    fn the_refresh_interval_is_the_median_gap() {
        assert_eq!(median(&[ms(4.0), ms(9.0), ms(1.0)]), ms(4.0));
        let mut s = Schedule::default();
        run(&mut s, HZ240, ms(3.0), 40);
        // A late frame and the short one that catches up after it.
        let at = s.last_frame.1.unwrap();
        s.presented(s.last_frame.0 + 1, ms(3.0), at + ms(7.0));
        s.presented(s.last_frame.0 + 1, ms(3.0), at + ms(8.4));
        assert_eq!(s.refresh, Some(HZ240));
        // The display changes to 60 Hz: within a median's worth of frames it is seen.
        run(&mut s, HZ60, ms(12.0), GAPS as u64 / 2 + 1);
        assert_eq!(s.refresh, Some(HZ60));
    }

    #[test]
    fn frames_that_never_block_keep_the_smallest_window() {
        let mut s = Schedule::default();
        run(&mut s, HZ60, ms(12.0), 30);
        run(&mut s, HZ60, Duration::ZERO, 10);
        assert_eq!(s.window(), MIN_SERVE);
    }

    #[test]
    fn presents_that_showed_nothing_do_not_adapt_the_window() {
        let mut s = Schedule::default();
        let now = Instant::now();
        s.presented(1, ms(20.0), now);
        let w = s.window();
        s.presented(1, Duration::ZERO, now + HZ60);
        assert_eq!(s.window(), w);
    }

    #[test]
    fn a_window_never_passes_the_largest() {
        let mut s = Schedule::default();
        run(&mut s, Duration::from_millis(60), Duration::from_secs(1), 100);
        assert_eq!(s.window(), MAX_SERVE);
    }

    #[test]
    fn long_gaps_are_neither_the_refresh_nor_late() {
        let mut s = Schedule::default();
        run(&mut s, HZ240, ms(3.0), 60);
        let (w, m) = (s.window(), s.margin());
        // Minimized for a second, then back.
        let at = s.last_frame.1.unwrap() + Duration::from_secs(1);
        s.presented(s.last_frame.0 + 1, ms(3.0) - w, at);
        assert_eq!((s.margin(), s.late_presents, s.refresh), (m, 0, Some(HZ240)));
    }

    #[test]
    fn a_heavy_request_waits_at_the_head_of_the_queue_and_order_is_kept() {
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
        assert_eq!((s.calls, s.held, s.windows), (3, 1, 2));
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
    fn waits_are_whole_milliseconds_that_end_by_the_deadline() {
        assert_eq!(whole_ms(ms(2.7).saturating_sub(TIMER_SLOP)), ms(1.0));
        assert_eq!(whole_ms(ms(1.9).saturating_sub(TIMER_SLOP)), Duration::ZERO);
    }

    /// A request and where its reply goes, like the engine's.
    struct Call {
        n: u64,
        reply: Sender<u64>,
    }

    /// The windowed loop against a fake display: frame work takes 1 to 1.5 ms, and `present`
    /// blocks until the next vblank (FIFO; a late frame waits for the vblank after). One client
    /// calls back to back, waiting for each reply, as an MCP agent does. Returns (client calls per
    /// second, frames, frames that skipped a vblank).
    fn fake_display(refresh: Duration, run_for: Duration) -> (f64, u64, u64) {
        let (to_engine, requests) = mpsc::channel::<Call>();
        let client = std::thread::spawn(move || {
            let (reply_tx, replies) = mpsc::channel();
            let start = Instant::now();
            let mut calls = 0u64;
            while start.elapsed() < run_for {
                to_engine.send(Call { n: calls, reply: reply_tx.clone() }).unwrap();
                assert_eq!(replies.recv().unwrap(), calls);
                calls += 1;
            }
            calls as f64 / start.elapsed().as_secs_f64()
        });
        let mut s = Schedule::default();
        let mut held = None;
        let origin = Instant::now();
        let (mut frames, mut late, mut last_vblank) = (0u64, 0u64, None);
        let mut seed = 0x2545_f491_u32;
        loop {
            let deadline = Instant::now() + s.window();
            let end = s.serve(&requests, &mut held, deadline, |_| false, |c| c.reply.send(c.n).unwrap(), || false);
            if end == Served::Disconnected {
                break;
            }
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            std::thread::sleep(Duration::from_micros(1000 + u64::from(seed % 500))); // frame work
            let vblank = (origin.elapsed().as_nanos() / refresh.as_nanos()) as u32 + 1;
            let blocked = (origin + refresh * vblank).saturating_duration_since(Instant::now());
            std::thread::sleep(blocked);
            frames += 1;
            late += last_vblank.is_some_and(|v| vblank > v + 1) as u64;
            last_vblank = Some(vblank);
            s.presented(frames, blocked, Instant::now());
        }
        (client.join().unwrap(), frames, late)
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
            let (rate, frames, late) = fake_display(refresh, Duration::from_millis(1500));
            eprintln!("{refresh:?} display: one sequential client {rate:.0} calls/s; {frames} frames, {late} skipped a vblank");
            assert!(rate > 300.0, "{rate:.0} calls/s at a {refresh:?} refresh");
            assert!(late * 10 <= frames, "{late} of {frames} frames skipped a vblank at a {refresh:?} refresh");
        }
    }
}
