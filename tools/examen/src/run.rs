//! Running a step: a process whose whole output goes to a log as it is written, stopped with
//! what it started when it outlives its limit.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::testlog::Exit;

/// The repository's root: two folders above this crate.
pub fn root() -> PathBuf {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    here.parent().and_then(Path::parent).unwrap_or(here).to_path_buf()
}

/// Where cargo builds: `CARGO_TARGET_DIR`, or the checkout's `target`.
pub fn target_dir() -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => root().join("target"),
    }
}

fn command(cmd: &[String], cwd: &Path, env: &[(&str, String)]) -> Command {
    let mut c = Command::new(&cmd[0]);
    c.args(&cmd[1..]).current_dir(cwd).stdin(Stdio::null());
    for (k, v) in env {
        c.env(k, v);
    }
    // A group of its own, so that stopping it stops the test binaries it started too.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0);
    }
    c
}

/// Stops a process that outlived its limit, with what it started (by its own process id:
/// nothing else on the machine is touched).
pub fn stop_tree(child: &mut Child) {
    let pid = child.id().to_string();
    #[cfg(windows)]
    let stopped = Command::new("taskkill").args(["/T", "/F", "/PID", &pid]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    #[cfg(not(windows))]
    let stopped = Command::new("kill").args(["-KILL", "--", &format!("-{pid}")]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    if !stopped.is_ok_and(|s| s.success()) {
        let _ = child.kill();
    }
    let until = Instant::now() + Duration::from_secs(30);
    while Instant::now() < until {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
}

/// Waits for `child` until `limit` has passed since `started`.
fn wait(child: &mut Child, started: Instant, limit: Duration) -> Exit {
    loop {
        match child.try_wait() {
            // (A process ended by a signal has no code.)
            Ok(Some(status)) => return Exit::Code(status.code().unwrap_or(-1)),
            Ok(None) => {}
            Err(_) => return Exit::Code(-1),
        }
        if started.elapsed() >= limit {
            stop_tree(child);
            return Exit::TimedOut;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// What a step's run gave.
pub struct Ran {
    pub exit: Exit,
    /// The whole log, read back.
    pub log: String,
    pub seconds: f64,
}

/// Runs `cmd` in `cwd` with its whole output going to `log` as it is written. A run past
/// `limit` is stopped. A command that cannot be started is a run that exited with 127, and its
/// log says why.
pub fn run(cmd: &[String], cwd: &Path, log: &Path, env: &[(&str, String)], limit: Duration) -> Ran {
    let started = Instant::now();
    let exit = match File::create(log).and_then(|f| Ok((f.try_clone()?, f))) {
        Ok((out, err)) => match command(cmd, cwd, env).stdout(out).stderr(err).spawn() {
            Ok(mut child) => wait(&mut child, started, limit),
            Err(e) => {
                let _ = std::fs::write(log, format!("could not start {:?}: {e}\n", cmd[0]));
                Exit::Code(127)
            }
        },
        Err(_) => Exit::Code(127),
    };
    let text = std::fs::read(log).map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default();
    Ran { exit, log: text, seconds: started.elapsed().as_secs_f64() }
}

/// What `cmd` wrote to its standard output, with its exit code, or `None` when it could not be
/// started or did not end within `limit` (it is stopped then). Its standard error is dropped.
pub fn said(cmd: &[&str], cwd: &Path, limit: Duration) -> Option<(i32, Vec<u8>)> {
    let owned: Vec<String> = cmd.iter().map(|s| (*s).to_owned()).collect();
    let mut child = command(&owned, cwd, &[]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let mut pipe = child.stdout.take()?;
    // Read on a thread: a child that writes more than a pipe holds would never end otherwise.
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    });
    match wait(&mut child, Instant::now(), limit) {
        Exit::Code(code) => reader.join().ok().map(|bytes| (code, bytes)),
        Exit::TimedOut => None,
    }
}

/// `said`, as text, when the command ended with 0.
pub fn said_text(cmd: &[&str], cwd: &Path, limit: Duration) -> Option<String> {
    match said(cmd, cwd, limit)? {
        (0, bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("examen-run-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cargo(args: &[&str]) -> Vec<String> {
        std::iter::once("cargo").chain(args.iter().copied()).map(str::to_owned).collect()
    }

    #[test]
    fn a_runs_whole_output_is_in_its_log_with_its_exit_code() {
        let dir = scratch("log");
        let ran = run(&cargo(&["--version"]), &root(), &dir.join("v.log"), &[], Duration::from_secs(120));
        assert!(ran.exit.ok() && ran.log.starts_with("cargo "), "{:?}: {}", ran.exit, ran.log);
        // (Cargo answers an unknown command on its standard error: both streams are in the log.)
        let ran = run(&cargo(&["no-such-command-of-cargo"]), &root(), &dir.join("x.log"), &[], Duration::from_secs(120));
        assert!(matches!(ran.exit, Exit::Code(c) if c != 0) && ran.log.contains("no-such-command-of-cargo"), "{:?}: {}", ran.exit, ran.log);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_command_that_cannot_start_is_a_failed_run_that_says_why() {
        let dir = scratch("missing");
        let ran = run(&["examen-no-such-program".to_owned()], &root(), &dir.join("m.log"), &[], Duration::from_secs(5));
        assert_eq!(ran.exit, Exit::Code(127));
        assert!(ran.log.contains("could not start"), "{}", ran.log);
        assert!(said(&["examen-no-such-program"], &root(), Duration::from_secs(5)).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn what_a_command_said_is_read_whole() {
        let text = said_text(&["cargo", "--version"], &root(), Duration::from_secs(120)).expect("cargo runs: it is running this test");
        assert!(text.starts_with("cargo "), "{text}");
        // More than a pipe holds (64 KB): the workspace's own description of its packages. A
        // reader that waited for the process to end before reading would never see it end.
        let (code, bytes) = said(&["cargo", "metadata", "--format-version", "1", "--no-deps", "--offline"], &root(), Duration::from_secs(120)).expect("cargo metadata ended within its limit");
        assert!(code == 0 && bytes.len() > 65_536 && bytes.trim_ascii_end().ends_with(b"}"), "exit {code}, {} bytes", bytes.len());
    }

    /// A step past its limit is stopped, and the run says so.
    #[test]
    fn a_run_past_its_limit_is_stopped() {
        let dir = scratch("limit");
        let wait: &[&str] = if cfg!(windows) { &["ping", "-n", "60", "127.0.0.1"] } else { &["sleep", "60"] };
        let cmd: Vec<String> = wait.iter().map(|s| (*s).to_owned()).collect();
        let started = Instant::now();
        let ran = run(&cmd, &root(), &dir.join("w.log"), &[], Duration::from_secs(1));
        assert_eq!(ran.exit, Exit::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(45), "stopped, not waited for: {:?}", started.elapsed());
        assert!(said(wait, &root(), Duration::from_secs(1)).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
