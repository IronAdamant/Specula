//! The start gate: a tool call that arrives while the engine is still starting waits for the
//! start to end, then runs.
//!
//! The server listens before the engine's loop begins (the port has to be this engine's before
//! it opens a window or tidies old captures), and what comes between can take seconds: with the
//! Vulkan validation layer installed, building the start-up pipelines takes about 2 s on a quiet
//! machine (about 12 ms without the layer) and several times that beside a compiler. A call sent
//! as soon as the port answered used to queue behind all of it with its 25 s deadline already
//! running, and on a loaded machine came back `engine_busy` (performance pass, 2026-11-14:
//! `scripts` had grown retries around their first `agent_hello`). Start-up is not the caller's
//! time: the call waits here, on its own connection thread, and its deadline starts when the
//! engine can serve it.
//!
//! A start that ends without the loop (an error, a panic) fails the gate ([`Starting`]): every
//! waiting call is told at once, and the process ends as quickly as it did before there was a
//! gate (the server joins its connection threads when it stops, so a call left waiting here
//! would hold a failed engine and its port for the rest of [`START_WAIT`]: review R1). The
//! refusal reaches the caller because the server answers every call that is running before it
//! closes a connection (`specula-protocol`'s `server.rs`; it shut the sockets first, and a
//! refused call whose thread ran late was read as an empty answer: tests/start_fail.rs).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use specula_core::Json;

/// How long a call waits for the engine to start before it is told so. A start that takes this
/// long is broken (a driver that hangs, a disk that does not answer), and says so in the trace.
/// (`scripts/specula_mcp.py` waits longer than this plus a call's own 30 s for its answer.)
pub const START_WAIT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    /// The engine is starting.
    Closed,
    /// Its loop began.
    Open,
    /// Its start ended without the loop.
    Failed,
}

/// Why a call that waited was not let through.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Refused {
    /// The engine had not started within the wait's limit (how long it waited).
    Slow(Duration),
    /// The engine's start failed: it is stopping.
    Failed,
}

/// Closed until the engine's loop begins.
pub struct StartGate {
    /// (Read first, without the lock: after the start every call takes this path.)
    open: AtomicBool,
    /// Where the start is, and how many calls are waiting for it.
    state: Mutex<(State, usize)>,
    changed: Condvar,
}

impl StartGate {
    pub fn closed() -> Arc<StartGate> {
        Arc::new(StartGate { open: AtomicBool::new(false), state: Mutex::new((State::Closed, 0)), changed: Condvar::new() })
    }

    fn settle(&self, to: State) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        // (An open gate stays open: the loop began, whatever ends later.)
        if state.0 == State::Closed {
            state.0 = to;
            self.open.store(to == State::Open, Ordering::Release);
        }
        self.changed.notify_all();
    }

    /// The engine's loop begins: every waiting call goes on, and no later call waits.
    pub fn open(&self) {
        self.settle(State::Open);
    }

    /// The start ended without the loop: every waiting call is refused now, and so is every
    /// later one.
    pub fn fail(&self) {
        self.settle(State::Failed);
    }

    /// Calls waiting for the start now.
    pub fn waiting(&self) -> usize {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).1
    }

    /// Waits until the start ends, at most `limit`.
    pub fn wait(&self, limit: Duration) -> Result<(), Refused> {
        if self.open.load(Ordering::Acquire) {
            return Ok(());
        }
        let asked = Instant::now();
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.1 += 1;
        while state.0 == State::Closed {
            let Some(left) = limit.checked_sub(asked.elapsed()).filter(|d| !d.is_zero()) else { break };
            state = self.changed.wait_timeout(state, left).unwrap_or_else(PoisonError::into_inner).0;
        }
        state.1 -= 1;
        match state.0 {
            State::Open => Ok(()),
            State::Failed => Err(Refused::Failed),
            State::Closed => Err(Refused::Slow(asked.elapsed())),
        }
    }
}

/// The engine's start, as a value: [`Starting::done`] opens the gate when the loop begins, and
/// dropping it any other way (an error returned from the start, a panic unwinding through it)
/// fails the gate, so no call is left waiting for a loop that will not come.
pub struct Starting {
    gate: Arc<StartGate>,
}

impl Starting {
    pub fn new(gate: Arc<StartGate>) -> Starting {
        Starting { gate }
    }

    /// The loop begins.
    pub fn done(self) {
        self.gate.open();
    }
}

impl Drop for Starting {
    fn drop(&mut self) {
        // (After `done` this changes nothing: an open gate stays open.)
        self.gate.fail();
    }
}

/// The answer to a call the gate refused: nothing was applied.
pub fn refusal(why: Refused) -> Json {
    match why {
        Refused::Slow(waited) => Json::obj([
            ("error", Json::from("engine_starting")),
            ("message", Json::from(format!("the engine has not finished starting after {:.0} s; the call was not applied: call again (trace_query {{code: \"engine.started\"}} says what its start spent time on, once it has)", waited.as_secs_f64()))),
            ("retry", Json::Bool(true)),
        ]),
        Refused::Failed => Json::obj([
            ("error", Json::from("engine_start_failed")),
            ("message", Json::from("the engine's start failed and it is stopping; the call was not applied: its console output and trace say why")),
            ("retry", Json::Bool(false)),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_open_gate_holds_nobody_and_a_closed_one_says_how_long_it_waited() {
        let gate = StartGate::closed();
        let Err(Refused::Slow(waited)) = gate.wait(Duration::from_millis(5)) else { panic!("closed: the wait ends at its limit") };
        assert!(waited >= Duration::from_millis(5), "{waited:?}");
        assert_eq!(gate.waiting(), 0, "and it is no longer counted as waiting");
        gate.open();
        assert_eq!(gate.wait(Duration::ZERO), Ok(()), "open: no wait at all, whatever the limit");
        let slow = refusal(Refused::Slow(waited));
        assert_eq!(slow.get("error").and_then(Json::as_str), Some("engine_starting"));
        assert_eq!(slow.get("retry"), Some(&Json::Bool(true)));
        // An open gate stays open, whatever ends later (the start's value is dropped at the
        // end of the engine's run).
        gate.fail();
        assert_eq!(gate.wait(Duration::ZERO), Ok(()));
    }

    /// Every call that was waiting goes on when the gate opens (none is left behind by a
    /// wake-up meant for another).
    #[test]
    fn opening_lets_every_waiting_call_through() {
        let gate = StartGate::closed();
        let callers: Vec<_> = (0..4)
            .map(|_| {
                let gate = Arc::clone(&gate);
                std::thread::spawn(move || gate.wait(START_WAIT))
            })
            .collect();
        // (All four are waiting before it opens: what is tested is the wake-up, not a late arrival.)
        while gate.waiting() < 4 {
            std::thread::yield_now();
        }
        Starting::new(Arc::clone(&gate)).done();
        for c in callers {
            assert_eq!(c.join().unwrap(), Ok(()));
        }
        assert_eq!(gate.waiting(), 0);
    }

    /// Review R1 of m5-perf: a start that fails after the port listens (the window, the
    /// swapchain or the pipelines refused, a panic in the game's start) left waiting calls to
    /// sit out their 120 s, and the server joins those threads before the process can end. The
    /// start's value dropped without `done` releases them at once, with a refusal that says
    /// the engine is stopping; calls that come later are refused at once too.
    #[test]
    fn a_start_that_fails_releases_every_waiting_call_at_once() {
        let gate = StartGate::closed();
        let starting = Starting::new(Arc::clone(&gate));
        let callers: Vec<_> = (0..3)
            .map(|_| {
                let gate = Arc::clone(&gate);
                // (The whole wait: only the failure can end it within the test's time.)
                std::thread::spawn(move || gate.wait(START_WAIT))
            })
            .collect();
        while gate.waiting() < 3 {
            std::thread::yield_now();
        }
        // The start returns an error: its value is dropped, as `?` drops it.
        drop(starting);
        for c in callers {
            assert_eq!(c.join().unwrap(), Err(Refused::Failed));
        }
        assert_eq!(gate.wait(START_WAIT), Err(Refused::Failed), "a later call does not wait either");
        let failed = refusal(Refused::Failed);
        assert_eq!(failed.get("error").and_then(Json::as_str), Some("engine_start_failed"));
        assert_eq!(failed.get("retry"), Some(&Json::Bool(false)), "the same call will not succeed: the engine is going");
        // A panic unwinding through the start does the same.
        let gate = StartGate::closed();
        let in_start = Arc::clone(&gate);
        let unwound = std::thread::spawn(move || {
            let _starting = Starting::new(in_start);
            panic!("the game's start panicked (this test's own panic)");
        })
        .join();
        assert!(unwound.is_err());
        assert_eq!(gate.wait(START_WAIT), Err(Refused::Failed));
    }
}
