//! A hung wait under a `Watchdog` names itself in the gate's log: the test harness captures
//! a test's output by default (the gate runs no step with `--nocapture`), so the watchdog's
//! words must reach standard error past that capture, and say which test hung, before the
//! process aborts.
//!
//! The hang runs in a child: this same test binary, run again on the one test
//! `a_wait_that_never_ends` with `SPECULA_WATCHDOG_CHILD` set. Without the variable that test
//! passes at once, so a plain run of the suite never hangs.

use std::process::Command;
use std::time::Duration;

const CHILD: &str = "SPECULA_WATCHDOG_CHILD";

/// The child's test: arms a watchdog of a short bound, then waits on a channel whose sender
/// it keeps, so the wait never ends and only the watchdog can end the process.
#[test]
fn a_wait_that_never_ends() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let _watchdog = specula_core::Watchdog::arm_for("the probe's channel", Duration::from_millis(200));
    let (_tx, rx) = std::sync::mpsc::channel::<()>();
    let _ = rx.recv();
}

#[test]
fn a_hung_wait_says_what_it_waited_for_and_which_test_hung_under_capture() {
    let exe = std::env::current_exe().expect("this test binary's path");
    // Output capture on (no --nocapture), as the gate runs it.
    let out = Command::new(exe)
        .args(["--exact", "a_wait_that_never_ends", "--test-threads=1"])
        .env(CHILD, "1")
        .output()
        .expect("run this test binary again as the hung child");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "the watchdog ended the hung child: {:?}\n{stderr}", out.status);
    assert!(
        stderr.contains("hung: the probe's channel has not ended in 200ms"),
        "the child's standard error says what it waited for: {stderr}"
    );
    assert!(stderr.contains("armed by a_wait_that_never_ends"), "and which test hung: {stderr}");
}
