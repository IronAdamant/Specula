//! Cross-platform environment detection for building and running Bevy apps.
//!
//! Bevy is a Cargo dependency, not a global binary. Readiness means: can this
//! machine compile and run a Bevy project with Rust/Cargo (and platform deps)?

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Supported host OS families for install guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OsFamily {
    Windows,
    Linux,
    Macos,
    Other,
}

impl OsFamily {
    /// Detect the host OS family at compile time / runtime.
    pub fn detect() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Other
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Macos => "macos",
            Self::Other => "other",
        }
    }
}

impl fmt::Display for OsFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Status of a single readiness check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ready,
    Missing,
    Failed,
    Skipped,
}

/// One named check in a readiness report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
    pub fix: Option<String>,
}

/// Full readiness report printed by `specula doctor`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadinessReport {
    pub os_family: OsFamily,
    pub os_label: String,
    pub ready: bool,
    pub checks: Vec<CheckResult>,
    pub summary: String,
    pub install_guidance: Vec<String>,
}

/// How to obtain process output (injectable for tests).
pub trait CommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, String>;
    fn looks_like_executable(&self, name: &str) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Production runner using `std::process::Command` and `which`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, String> {
        let output = Command::new(program)
            .args(args)
            .output()
            .map_err(|e| format!("failed to spawn `{program}`: {e}"))?;
        Ok(CommandOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }

    fn looks_like_executable(&self, name: &str) -> bool {
        which::which(name).is_ok()
    }
}

/// Options for readiness probing.
#[derive(Debug, Clone)]
pub struct DoctorOptions {
    /// When true, create a temporary Cargo project with Bevy and try to compile it.
    pub compile_probe: bool,
    /// Bevy version string used by the compile probe (e.g. "0.19").
    pub bevy_version: String,
    /// Timeout budget for the compile probe (informational; process is waited fully).
    pub compile_timeout: Duration,
    /// Working directory for probes (defaults to system temp).
    pub work_dir: Option<PathBuf>,
}

impl Default for DoctorOptions {
    fn default() -> Self {
        Self {
            compile_probe: false,
            bevy_version: "0.19".to_string(),
            compile_timeout: Duration::from_secs(600),
            work_dir: None,
        }
    }
}

/// Run readiness checks using the given command runner.
pub fn check_readiness(runner: &dyn CommandRunner, options: &DoctorOptions) -> ReadinessReport {
    let os = OsFamily::detect();
    let mut checks = Vec::new();

    checks.push(check_rustc(runner));
    checks.push(check_cargo(runner));
    checks.push(check_platform_notes(os));

    if options.compile_probe {
        checks.push(check_bevy_compile_probe(runner, options));
    } else {
        checks.push(CheckResult {
            name: "bevy_create_compile".into(),
            status: CheckStatus::Skipped,
            detail: "Compile probe skipped (pass --compile-probe to create and build a tiny Bevy app).".into(),
            fix: None,
        });
    }

    let ready = checks.iter().all(|c| {
        matches!(c.status, CheckStatus::Ready | CheckStatus::Skipped)
    });

    let mut install_guidance = Vec::new();
    if checks.iter().any(|c| c.name == "rustc" && c.status != CheckStatus::Ready)
        || checks.iter().any(|c| c.name == "cargo" && c.status != CheckStatus::Ready)
    {
        install_guidance.extend(rust_install_guidance(os));
    }
    install_guidance.extend(platform_dependency_guidance(os));

    if !ready {
        for c in &checks {
            if let Some(fix) = &c.fix {
                if !install_guidance.iter().any(|g| g == fix) {
                    install_guidance.push(fix.clone());
                }
            }
        }
    }

    let summary = if ready {
        format!(
            "Host OS family `{os}` looks ready to build Bevy apps with the installed Rust toolchain."
        )
    } else {
        format!(
            "Host OS family `{os}` is not fully ready. See checks and install guidance below."
        )
    };

    ReadinessReport {
        os_family: os,
        os_label: os_display_label(os),
        ready,
        checks,
        summary,
        install_guidance,
    }
}

fn os_display_label(os: OsFamily) -> String {
    match os {
        OsFamily::Windows => "Windows".into(),
        OsFamily::Linux => "Linux".into(),
        OsFamily::Macos => "macOS".into(),
        OsFamily::Other => std::env::consts::OS.to_string(),
    }
}

fn check_rustc(runner: &dyn CommandRunner) -> CheckResult {
    if !runner.looks_like_executable("rustc") {
        return CheckResult {
            name: "rustc".into(),
            status: CheckStatus::Missing,
            detail: "rustc was not found on PATH.".into(),
            fix: Some("Install Rust via https://rustup.rs (installs rustc + cargo).".into()),
        };
    }
    match runner.run("rustc", &["--version"]) {
        Ok(out) if out.success && !out.stdout.is_empty() => CheckResult {
            name: "rustc".into(),
            status: CheckStatus::Ready,
            detail: out.stdout,
            fix: None,
        },
        Ok(out) => CheckResult {
            name: "rustc".into(),
            status: CheckStatus::Failed,
            detail: format!("rustc --version failed: {}", first_nonempty(&out.stderr, &out.stdout)),
            fix: Some("Reinstall the Rust toolchain with rustup: `rustup default stable`.".into()),
        },
        Err(e) => CheckResult {
            name: "rustc".into(),
            status: CheckStatus::Failed,
            detail: e,
            fix: Some("Install Rust via https://rustup.rs".into()),
        },
    }
}

fn check_cargo(runner: &dyn CommandRunner) -> CheckResult {
    if !runner.looks_like_executable("cargo") {
        return CheckResult {
            name: "cargo".into(),
            status: CheckStatus::Missing,
            detail: "cargo was not found on PATH.".into(),
            fix: Some("Install Rust via https://rustup.rs (cargo is included).".into()),
        };
    }
    match runner.run("cargo", &["--version"]) {
        Ok(out) if out.success && !out.stdout.is_empty() => CheckResult {
            name: "cargo".into(),
            status: CheckStatus::Ready,
            detail: out.stdout,
            fix: None,
        },
        Ok(out) => CheckResult {
            name: "cargo".into(),
            status: CheckStatus::Failed,
            detail: format!("cargo --version failed: {}", first_nonempty(&out.stderr, &out.stdout)),
            fix: Some("Repair cargo with `rustup component add cargo` or reinstall rustup.".into()),
        },
        Err(e) => CheckResult {
            name: "cargo".into(),
            status: CheckStatus::Failed,
            detail: e,
            fix: Some("Install Rust via https://rustup.rs".into()),
        },
    }
}

fn check_platform_notes(os: OsFamily) -> CheckResult {
    let detail = match os {
        OsFamily::Windows => {
            "Windows: MSVC Build Tools (or full VS) and a GPU/driver capable of Vulkan/DX12 are typically required for Bevy."
        }
        OsFamily::Linux => {
            "Linux: install compiler + windowing/graphics packages (see install guidance)."
        }
        OsFamily::Macos => {
            "macOS: Xcode Command Line Tools are required (`xcode-select --install`)."
        }
        OsFamily::Other => "Unknown OS: ensure a working C linker and graphics stack for Bevy.",
    };
    CheckResult {
        name: "platform_notes".into(),
        status: CheckStatus::Ready,
        detail: detail.into(),
        fix: None,
    }
}

fn check_bevy_compile_probe(runner: &dyn CommandRunner, options: &DoctorOptions) -> CheckResult {
    if !runner.looks_like_executable("cargo") {
        return CheckResult {
            name: "bevy_create_compile".into(),
            status: CheckStatus::Missing,
            detail: "Cannot run compile probe without cargo.".into(),
            fix: Some("Install cargo via https://rustup.rs first.".into()),
        };
    }

    let base = options
        .work_dir
        .clone()
        .unwrap_or_else(std::env::temp_dir);
    let probe_dir = base.join(format!(
        "specula-probe-{}",
        std::process::id()
    ));

    if let Err(e) = std::fs::create_dir_all(&probe_dir) {
        return CheckResult {
            name: "bevy_create_compile".into(),
            status: CheckStatus::Failed,
            detail: format!("Could not create probe dir {}: {e}", probe_dir.display()),
            fix: None,
        };
    }

    let result = run_compile_probe(runner, &probe_dir, &options.bevy_version);
    let _ = std::fs::remove_dir_all(&probe_dir);
    result
}

fn run_compile_probe(
    runner: &dyn CommandRunner,
    probe_dir: &Path,
    bevy_version: &str,
) -> CheckResult {
    // Create a minimal Cargo project that depends on Bevy with default features disabled
    // for a faster probe; we still validate resolution + compile of the dependency graph
    // for a tiny binary that only needs bevy's crate to load. Full DefaultPlugins compile
    // is heavy; we use a lightweight bevy feature set that still exercises cargo+bevy.
    let cargo_toml = format!(
        r#"[package]
name = "specula_probe"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
bevy = {{ version = "{bevy_version}", default-features = false }}
"#
    );
    let main_rs = r#"
fn main() {
    // Reference the bevy crate so cargo must compile the dependency.
    let _ = std::any::type_name::<bevy::app::App>();
    println!("specula probe ok");
}
"#;

    if let Err(e) = std::fs::write(probe_dir.join("Cargo.toml"), cargo_toml) {
        return CheckResult {
            name: "bevy_create_compile".into(),
            status: CheckStatus::Failed,
            detail: format!("write Cargo.toml: {e}"),
            fix: None,
        };
    }
    let src = probe_dir.join("src");
    if let Err(e) = std::fs::create_dir_all(&src) {
        return CheckResult {
            name: "bevy_create_compile".into(),
            status: CheckStatus::Failed,
            detail: format!("create src/: {e}"),
            fix: None,
        };
    }
    if let Err(e) = std::fs::write(src.join("main.rs"), main_rs) {
        return CheckResult {
            name: "bevy_create_compile".into(),
            status: CheckStatus::Failed,
            detail: format!("write main.rs: {e}"),
            fix: None,
        };
    }

    // Use cargo build in the probe directory via absolute cargo + --manifest-path.
    let manifest = probe_dir.join("Cargo.toml");
    let manifest_str = manifest.to_string_lossy();
    match runner.run(
        "cargo",
        &[
            "build",
            "--manifest-path",
            manifest_str.as_ref(),
            "--quiet",
        ],
    ) {
        Ok(out) if out.success => CheckResult {
            name: "bevy_create_compile".into(),
            status: CheckStatus::Ready,
            detail: format!(
                "Created and compiled a minimal Bevy {bevy_version} probe project successfully."
            ),
            fix: None,
        },
        Ok(out) => CheckResult {
            name: "bevy_create_compile".into(),
            status: CheckStatus::Failed,
            detail: format!(
                "Bevy compile probe failed:\n{}",
                first_nonempty(&out.stderr, &out.stdout)
            ),
            fix: Some(
                "Ensure platform C/C++ build tools and graphics-related system packages are installed (see OS guidance)."
                    .into(),
            ),
        },
        Err(e) => CheckResult {
            name: "bevy_create_compile".into(),
            status: CheckStatus::Failed,
            detail: e,
            fix: Some("Ensure cargo is installed and can spawn build processes.".into()),
        },
    }
}

/// OS-specific Rust install guidance.
pub fn rust_install_guidance(os: OsFamily) -> Vec<String> {
    match os {
        OsFamily::Windows => vec![
            "Windows: install Rust from https://rustup.rs (or `winget install Rustlang.Rustup`).".into(),
            "Windows: install Visual Studio Build Tools with the \"Desktop development with C++\" workload.".into(),
            "Then open a new shell and verify: `rustc --version` and `cargo --version`.".into(),
        ],
        OsFamily::Linux => vec![
            "Linux: install Rust with: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`.".into(),
            "Then: `source \"$HOME/.cargo/env\"` and verify `rustc --version` / `cargo --version`.".into(),
        ],
        OsFamily::Macos => vec![
            "macOS: install Rust with: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`.".into(),
            "macOS: install Xcode CLT: `xcode-select --install`.".into(),
            "Then verify: `rustc --version` and `cargo --version`.".into(),
        ],
        OsFamily::Other => vec![
            "Install Rust from https://rustup.rs and ensure a working C linker is available.".into(),
        ],
    }
}

/// Platform dependency notes for running Bevy (graphics/windowing).
pub fn platform_dependency_guidance(os: OsFamily) -> Vec<String> {
    match os {
        OsFamily::Windows => vec![
            "Windows Bevy runtime: keep GPU drivers updated (DX12/Vulkan).".into(),
            "Optional full-stack MCP control: `cargo install bevy_brp_mcp --locked` (pairs with bevy_brp_extras 0.22.x).".into(),
        ],
        OsFamily::Linux => vec![
            "Debian/Ubuntu packages commonly needed for Bevy:".into(),
            "  sudo apt-get install -y build-essential pkg-config libx11-dev libasound2-dev libudev-dev libxkbcommon-x11-dev libwayland-dev libxkbcommon-dev".into(),
            "Fedora: `sudo dnf install gcc pkg-config libX11-devel alsa-lib-devel systemd-devel`.".into(),
            "Optional full-stack MCP control: `cargo install bevy_brp_mcp --locked`.".into(),
        ],
        OsFamily::Macos => vec![
            "macOS Bevy runtime: Metal-capable Mac; no extra package managers required beyond Xcode CLT.".into(),
            "Optional full-stack MCP control: `cargo install bevy_brp_mcp --locked`.".into(),
        ],
        OsFamily::Other => vec![
            "Consult Bevy setup docs for your platform: https://bevy.org/learn/quick-start/getting-started/setup/".into(),
        ],
    }
}

/// Format a report for human-readable CLI output.
pub fn format_report_text(report: &ReadinessReport) -> String {
    let mut out = String::new();
    out.push_str("Specula environment readiness\n");
    out.push_str("================================\n");
    out.push_str(&format!("OS family : {}\n", report.os_family));
    out.push_str(&format!("OS label  : {}\n", report.os_label));
    out.push_str(&format!(
        "Overall   : {}\n",
        if report.ready {
            "READY"
        } else {
            "NOT READY"
        }
    ));
    out.push_str(&format!("Summary   : {}\n\n", report.summary));
    out.push_str("Checks:\n");
    for c in &report.checks {
        out.push_str(&format!(
            "  [{:<8}] {} — {}\n",
            status_label(c.status),
            c.name,
            c.detail.replace('\n', "\n             ")
        ));
        if let Some(fix) = &c.fix {
            out.push_str(&format!("             fix: {fix}\n"));
        }
    }
    if !report.install_guidance.is_empty() {
        out.push_str("\nInstall / fix guidance:\n");
        for (i, g) in report.install_guidance.iter().enumerate() {
            out.push_str(&format!("  {}. {}\n", i + 1, g));
        }
    }
    out
}

fn status_label(s: CheckStatus) -> &'static str {
    match s {
        CheckStatus::Ready => "ready",
        CheckStatus::Missing => "missing",
        CheckStatus::Failed => "failed",
        CheckStatus::Skipped => "skipped",
    }
}

fn first_nonempty(a: &str, b: &str) -> String {
    if !a.trim().is_empty() {
        a.to_string()
    } else {
        b.to_string()
    }
}

/// Compatibility matrix constants for docs and CLI.
pub mod compat {
    pub const BEVY: &str = "0.19";
    pub const BEVY_BRP_MCP: &str = "0.22.1";
    pub const BEVY_BRP_EXTRAS: &str = "0.22.1";
    pub const DEFAULT_BRP_PORT: u16 = 15702;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct MockRunner {
        executables: Vec<String>,
        responses: Mutex<HashMap<String, CommandOutput>>,
    }

    impl MockRunner {
        fn new(execs: &[&str]) -> Self {
            Self {
                executables: execs.iter().map(|s| s.to_string()).collect(),
                responses: Mutex::new(HashMap::new()),
            }
        }

        fn with_response(self, key: &str, out: CommandOutput) -> Self {
            self.responses.lock().unwrap().insert(key.into(), out);
            self
        }
    }

    impl CommandRunner for MockRunner {
        fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, String> {
            let key = format!("{program} {}", args.join(" "));
            self.responses
                .lock()
                .unwrap()
                .get(&key)
                .cloned()
                .ok_or_else(|| format!("no mock for `{key}`"))
        }

        fn looks_like_executable(&self, name: &str) -> bool {
            self.executables.iter().any(|e| e == name)
        }
    }

    #[test]
    fn detects_three_os_families_in_guidance() {
        for os in [OsFamily::Windows, OsFamily::Linux, OsFamily::Macos] {
            let rust = rust_install_guidance(os);
            let plat = platform_dependency_guidance(os);
            assert!(!rust.is_empty(), "rust guidance empty for {os:?}");
            assert!(!plat.is_empty(), "platform guidance empty for {os:?}");
            let joined = rust.join("\n") + &plat.join("\n");
            match os {
                OsFamily::Windows => {
                    assert!(joined.to_lowercase().contains("windows"));
                    assert!(joined.contains("rustup") || joined.contains("Rustup"));
                }
                OsFamily::Linux => {
                    assert!(joined.to_lowercase().contains("linux") || joined.contains("apt"));
                }
                OsFamily::Macos => {
                    assert!(joined.to_lowercase().contains("macos") || joined.contains("xcode"));
                }
                OsFamily::Other => {}
            }
        }
    }

    #[test]
    fn missing_toolchain_reports_not_ready_with_guidance() {
        let runner = MockRunner::new(&[]);
        let report = check_readiness(&runner, &DoctorOptions::default());
        assert!(!report.ready);
        assert_eq!(report.os_family, OsFamily::detect());
        assert!(report.checks.iter().any(|c| c.name == "rustc" && c.status == CheckStatus::Missing));
        assert!(report.checks.iter().any(|c| c.name == "cargo" && c.status == CheckStatus::Missing));
        assert!(!report.install_guidance.is_empty());
        let text = format_report_text(&report);
        assert!(text.contains("NOT READY"));
        assert!(text.contains(report.os_family.as_str()) || text.contains(&report.os_label));
    }

    #[test]
    fn installed_toolchain_is_ready_without_compile_probe() {
        let runner = MockRunner::new(&["rustc", "cargo"])
            .with_response(
                "rustc --version",
                CommandOutput {
                    success: true,
                    stdout: "rustc 1.85.0".into(),
                    stderr: String::new(),
                },
            )
            .with_response(
                "cargo --version",
                CommandOutput {
                    success: true,
                    stdout: "cargo 1.85.0".into(),
                    stderr: String::new(),
                },
            );
        let report = check_readiness(&runner, &DoctorOptions::default());
        assert!(report.ready, "{report:?}");
        assert!(report.checks.iter().any(|c| {
            c.name == "bevy_create_compile" && c.status == CheckStatus::Skipped
        }));
        let text = format_report_text(&report);
        assert!(text.contains("READY"));
        assert!(text.contains("rustc 1.85.0"));
    }

    #[test]
    fn failed_rustc_version_is_failed_not_ready() {
        let runner = MockRunner::new(&["rustc", "cargo"])
            .with_response(
                "rustc --version",
                CommandOutput {
                    success: false,
                    stdout: String::new(),
                    stderr: "boom".into(),
                },
            )
            .with_response(
                "cargo --version",
                CommandOutput {
                    success: true,
                    stdout: "cargo 1.85.0".into(),
                    stderr: String::new(),
                },
            );
        let report = check_readiness(&runner, &DoctorOptions::default());
        assert!(!report.ready);
        let rustc = report.checks.iter().find(|c| c.name == "rustc").unwrap();
        assert_eq!(rustc.status, CheckStatus::Failed);
    }

    #[test]
    fn os_family_display_and_serde_roundtrip() {
        for os in [
            OsFamily::Windows,
            OsFamily::Linux,
            OsFamily::Macos,
            OsFamily::Other,
        ] {
            let s = serde_json::to_string(&os).unwrap();
            let back: OsFamily = serde_json::from_str(&s).unwrap();
            assert_eq!(os, back);
            assert!(!os.as_str().is_empty());
        }
    }

    #[test]
    fn report_json_includes_os_and_ready_flag() {
        let runner = MockRunner::new(&["rustc", "cargo"])
            .with_response(
                "rustc --version",
                CommandOutput {
                    success: true,
                    stdout: "rustc 1.85.0".into(),
                    stderr: String::new(),
                },
            )
            .with_response(
                "cargo --version",
                CommandOutput {
                    success: true,
                    stdout: "cargo 1.85.0".into(),
                    stderr: String::new(),
                },
            );
        let report = check_readiness(&runner, &DoctorOptions::default());
        let v = serde_json::to_value(&report).unwrap();
        assert!(v.get("os_family").is_some());
        assert_eq!(v["ready"], true);
        assert!(v["checks"].as_array().unwrap().len() >= 3);
    }

    #[test]
    fn compat_constants_match_pinned_stack() {
        assert_eq!(compat::BEVY, "0.19");
        assert_eq!(compat::BEVY_BRP_MCP, "0.22.1");
        assert_eq!(compat::BEVY_BRP_EXTRAS, "0.22.1");
        assert_eq!(compat::DEFAULT_BRP_PORT, 15702);
    }
}
