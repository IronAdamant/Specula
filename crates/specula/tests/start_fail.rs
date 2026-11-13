//! A start that fails after the port answers: calls waiting at the start gate are refused at
//! once and the process ends (review R1 of m5-perf).
//!
//! The server listens before the engine's loop begins, and a call sent in between waits at the
//! start gate (`startgate.rs`). When the start then fails (the window, the swapchain or the
//! pipelines refused, a panic in the game's start), nothing was left to open the gate: the call
//! sat out its two minutes, and the server, which joins its connection threads when it stops,
//! kept the failed process and its port alive with it. The first fix released the callers when
//! the start's value was dropped, but declared that value before the server, so the server was
//! still dropped first, waiting for the very calls the value would have released. This runs the
//! real executable: the order of two declarations in `run_with` is what it holds.
//!
//! The engine is started headless on a port of the system's choosing with
//! `SPECULA_TEST_FAIL_START` set (tests only: the start returns an error after the port
//! answers, once a call is waiting at the gate). The call is the executable's own
//! `specula call`, which finds the engine through a settings folder of this test's.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Much less than the start gate's 120 s: a failed start that still held its callers would
/// run past it. (Far more than the second or two it takes: a test run shares the machine.)
const SOON: Duration = Duration::from_secs(60);

fn wait_for_exit(child: &mut Child, limit: Duration) -> Option<std::process::ExitStatus> {
    let asked = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if asked.elapsed() >= limit {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_start_that_fails_after_the_port_answers_refuses_its_callers_and_ends() {
    let exe = env!("CARGO_BIN_EXE_specula");
    let dir = std::env::temp_dir().join(format!("specula-start-fail-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let at = |name: &str| dir.join(name).display().to_string();
    let settings = at("settings");
    let mut engine = Command::new(exe)
        .args(["--headless", "--port", "0", "--settings", &settings, "--captures", &at("captures"), "--worlds", &at("worlds"), "--cache", &at("cache"), "--trace", &at("trace")])
        .env("SPECULA_TEST_FAIL_START", "1")
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the engine's own executable starts");
    // Its console lines, read to the end on a thread (the pipe never fills).
    let (line_tx, lines) = mpsc::channel::<String>();
    let stderr = engine.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if line_tx.send(line).is_err() {
                break;
            }
        }
    });
    // It listens (or it could not start at all: no Vulkan here).
    let mut seen = Vec::new();
    let listening = loop {
        match lines.recv_timeout(SOON) {
            Ok(line) => {
                let found = line.contains("MCP at http://127.0.0.1:");
                seen.push(line);
                if found {
                    break true;
                }
            }
            Err(_) => break false,
        }
    };
    if !listening {
        let _ = engine.kill();
        let _ = engine.wait();
        let _ = reader.join();
        let _ = std::fs::remove_dir_all(&dir);
        eprintln!("skipped: the engine did not get as far as listening (no Vulkan or no GPU here?): {}", seen.join(" | "));
        return;
    }
    // A call, sent while the engine is still starting: it waits at the gate, the start fails,
    // and it is told so. The engine ends.
    let asked = Instant::now();
    let call = Command::new(exe).args(["call", "engine_info", "--settings", &settings]).current_dir(&dir).stdin(Stdio::null()).output().expect("the command runs");
    let answered = asked.elapsed();
    let said = String::from_utf8_lossy(&call.stdout).into_owned();
    let ended = wait_for_exit(&mut engine, SOON);
    let rest: Vec<String> = lines.iter().collect();
    let _ = reader.join();
    if ended.is_none() {
        let _ = engine.kill();
        let _ = engine.wait();
    }
    let _ = std::fs::remove_dir_all(Path::new(&dir));
    assert!(said.contains("engine_start_failed"), "the waiting call is told the start failed: {said} (the engine said: {} | {})", seen.join(" | "), rest.join(" | "));
    assert_eq!(call.status.code(), Some(1), "a refused call: {said}");
    assert!(answered < SOON, "and told at once, not after its two minutes at the gate: {answered:?}");
    let status = ended.unwrap_or_else(|| panic!("the failed engine is still running {SOON:?} after its start failed: its server waited for a call at the gate"));
    assert!(!status.success(), "a failed start is a failed run: {status:?}");
    assert!(rest.iter().chain(&seen).any(|l| l.contains("SPECULA_TEST_FAIL_START")), "it failed where this test asked it to: {}", rest.join(" | "));
}
