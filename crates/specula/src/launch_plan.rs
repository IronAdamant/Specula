//! Pure helpers for MCP `bevy_launch_app` (warm binary vs cold cargo run).
//!
//! Unit-tested without spawning Bevy windows.

use std::path::{Path, PathBuf};

/// How the launcher should start the game process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchMode {
    /// `target/debug/<package>` already exists (warm rebuild path).
    WarmBinary { binary: PathBuf },
    /// No debug binary yet — first compile will be slow.
    ColdCargoRun,
}

/// Parse `name = "…"` from a Cargo.toml body (package section, first match).
pub fn parse_package_name_from_cargo_toml(contents: &str) -> Option<String> {
    let mut in_package = false;
    for line in contents.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some(rest) = t.strip_prefix("name") {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('=') {
                let rest = rest.trim();
                let name = rest
                    .trim_matches('"')
                    .trim_matches('\'')
                    .trim()
                    .to_string();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
    }
    None
}

/// Expected debug binary path for a package (`target/debug/<name>[.exe]`).
pub fn debug_binary_path(package_dir: &Path, package_name: &str) -> PathBuf {
    let mut p = package_dir.join("target").join("debug").join(package_name);
    if cfg!(windows) {
        p.set_extension("exe");
    }
    p
}

/// Prefer warm binary when present; otherwise cold `cargo run`.
pub fn resolve_launch_mode(package_dir: &Path, package_name: &str) -> LaunchMode {
    let binary = debug_binary_path(package_dir, package_name);
    if binary.is_file() {
        LaunchMode::WarmBinary { binary }
    } else {
        LaunchMode::ColdCargoRun
    }
}

/// Agent-facing note for spawn result (non-blocking contract messaging).
pub fn format_launch_spawn_message(
    mode: &LaunchMode,
    manifest: &str,
    features: &str,
    port: u16,
    target: &str,
    cwd: &Path,
    log_path: &Path,
    wait_secs: u64,
    pid: Option<u32>,
) -> String {
    let mode_s = match mode {
        LaunchMode::WarmBinary { binary } => {
            format!("mode=warm_binary binary={}", binary.display())
        }
        LaunchMode::ColdCargoRun => {
            "mode=cold_cargo_run note=no target/debug binary yet — first Bevy compile is slow; \
             prefer shell `cargo run --features remote,capture` then bevy_wait_brp; \
             MCP wait_secs must stay 0 on cold builds"
                .to_string()
        }
    };
    let wait_note = if wait_secs == 0 {
        "wait_secs=0 (non-blocking) → call bevy_wait_brp (timeout_secs 180 cold / 30 warm) before query/capture"
            .to_string()
    } else {
        format!("wait_secs={wait_secs} (capped; use bevy_wait_brp for longer waits)")
    };
    let pid_s = match pid {
        Some(p) => format!("pid={p}"),
        None => "pid=unknown".into(),
    };
    format!(
        "status=spawned {mode_s} {pid_s} manifest={manifest} features={features} port={port} target={target} \
         cwd={} log={} {wait_note}",
        cwd.display(),
        log_path.display()
    )
}

/// Message when a warm binary was required but missing (fail-fast guidance).
pub fn format_missing_warm_binary(package_dir: &Path, package_name: &str) -> String {
    let expected = debug_binary_path(package_dir, package_name);
    format!(
        "status=cold_path_missing_binary expected={} package_dir={} package={} \
         action=run `cargo build --manifest-path <Cargo.toml> --features remote,capture` (or shell cargo run) \
         then bevy_launch_app / bevy_wait_brp — do not block MCP 120s waiting for cold compile",
        expected.display(),
        package_dir.display(),
        package_name
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_package_name_simple() {
        let toml = r#"
[package]
name = "crystal_drift"
version = "0.1.0"
"#;
        assert_eq!(
            parse_package_name_from_cargo_toml(toml).as_deref(),
            Some("crystal_drift")
        );
    }

    #[test]
    fn resolve_cold_when_no_binary() {
        let dir = tempfile::tempdir().unwrap();
        let mode = resolve_launch_mode(dir.path(), "my_game");
        assert_eq!(mode, LaunchMode::ColdCargoRun);
        let msg = format_missing_warm_binary(dir.path(), "my_game");
        assert!(msg.contains("cold_path_missing_binary"));
        assert!(msg.contains("target"));
        assert!(msg.contains("bevy_wait_brp"));
    }

    #[test]
    fn resolve_warm_when_binary_exists() {
        let dir = tempfile::tempdir().unwrap();
        let bin = debug_binary_path(dir.path(), "my_game");
        fs::create_dir_all(bin.parent().unwrap()).unwrap();
        fs::write(&bin, b"fake").unwrap();
        match resolve_launch_mode(dir.path(), "my_game") {
            LaunchMode::WarmBinary { binary } => assert_eq!(binary, bin),
            LaunchMode::ColdCargoRun => panic!("expected warm"),
        }
        let msg = format_launch_spawn_message(
            &LaunchMode::WarmBinary { binary: bin.clone() },
            "Cargo.toml",
            "remote,capture",
            15702,
            "t",
            dir.path(),
            Path::new("/tmp/log"),
            0,
            Some(4242),
        );
        assert!(msg.contains("status=spawned"));
        assert!(msg.contains("warm_binary"));
        assert!(msg.contains("wait_secs=0"));
        assert!(msg.contains("bevy_wait_brp"));
        assert!(msg.contains("pid=4242"));
    }

    #[test]
    fn cold_spawn_message_mentions_shell() {
        let msg = format_launch_spawn_message(
            &LaunchMode::ColdCargoRun,
            "x/Cargo.toml",
            "remote,capture",
            15702,
            "game",
            Path::new("/proj"),
            Path::new("/tmp/l"),
            0,
            None,
        );
        assert!(msg.contains("cold_cargo_run"));
        assert!(msg.contains("shell"));
        assert!(msg.contains("bevy_wait_brp"));
        assert!(msg.contains("pid=unknown"));
    }
}
