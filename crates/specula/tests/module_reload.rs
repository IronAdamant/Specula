//! The module host end to end, with real Cargo builds and a real library: the example module
//! (games/ambulo) is built, loaded and played; then its source is edited and rebuilt while the
//! world keeps running, and the world, the character's state and the module's state store
//! survive. Compile errors come back as data, machine-applicable fixes are applied, and a
//! panicking build is disabled with the world intact.
//!
//! Works on a copy of the module (under `<target dir>/module-tests`, its own workspace and target
//! folder, with the project's Cargo configuration beside it), so the repository's sources are
//! never touched and the running `cargo test` doesn't hold the build lock it needs. Skips (passing) where module libraries can't be loaded (not
//! Windows yet) or Cargo isn't available.

use specula::logic::{LogicHost, Origin, Phase};
use specula::world::{Mark, World, WorldError};
use specula_core::Json;
use specula_logic::{BuildRequest, BuildService, ModuleCode};
use std::path::{Path, PathBuf};
use std::time::Instant;

const HZ: u32 = 30;

struct Run {
    host: LogicHost,
    world: World,
    tick: u64,
}

fn journal() -> impl FnMut(&mut World, Mark, &str, Phase) -> Result<(), WorldError> {
    |w: &mut World, _: Mark, _: &str, _: Phase| {
        w.take_changes();
        Ok(())
    }
}

impl Run {
    fn load(&mut self, library: &Path, copies: &Path) -> Json {
        let code = ModuleCode::open(library, copies).expect("the module library loads");
        let origin = Origin { dir: None, library: Some(library.to_owned()) };
        self.host.load(code, origin, None, &mut self.world, self.tick, HZ, &mut journal()).expect("load")
    }

    fn ticks(&mut self, n: u64) {
        for _ in 0..n {
            self.host.tick(&mut self.world, self.tick, HZ, &mut journal());
            self.tick += 1;
        }
    }

    fn player(&self) -> &specula::world::Entity {
        let id = self.world.find_path("/ambulo/player").expect("the player exists");
        self.world.get(id).expect("player")
    }

    fn hold(&mut self, actions: &str, ticks: u64) {
        self.host.hold("ambulo", &Json::parse(actions).unwrap(), ticks).unwrap();
    }

    fn status(&self) -> Json {
        let s = self.host.status(Some("ambulo"), true).unwrap();
        s.get("modules").and_then(Json::as_array).unwrap()[0].clone()
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            if e.file_name() != "target" {
                copy_dir(&src, &dst);
            }
        } else {
            // Read and write, not fs::copy: a copy keeps the old modification time on Windows, so
            // Cargo would think the restored source older than the last build and not rebuild.
            std::fs::write(&dst, std::fs::read(&src).unwrap()).unwrap();
        }
    }
}

/// The ambulo crate as its own workspace, depending on this repository's SDK.
/// Where the copy goes: under Cargo's target folder (`CARGO_TARGET_DIR` if set), next to a copy
/// of the project's `.cargo` (the toolchain linker), as the root its build may use.
fn module_base(repo: &Path) -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| repo.join("target"), PathBuf::from);
    let base = specula_logic::fix::canonical(&target).join("module-tests");
    std::fs::create_dir_all(base.join(".cargo")).unwrap();
    let cfg = repo.join(".cargo").join("config.toml");
    if cfg.exists() {
        std::fs::write(base.join(".cargo").join("config.toml"), std::fs::read(cfg).unwrap()).unwrap();
    }
    base
}

fn module_copy(repo: &Path, base: &Path) -> PathBuf {
    let dir = base.join("ambulo");
    let _ = std::fs::remove_dir_all(dir.join("src"));
    copy_dir(&repo.join("games").join("ambulo"), &dir);
    let sdk = repo.join("crates").join("specula-sdk");
    let manifest = format!(
        "[package]\nname = \"ambulo\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n\
         [dependencies]\nspecula-sdk = {{ path = {:?} }}\n\n[workspace]\n\n[profile.dev]\ndebug = 0\nstrip = \"debuginfo\"\npanic = \"abort\"\n",
        sdk.to_string_lossy().replace('\\', "/")
    );
    std::fs::write(dir.join("Cargo.toml"), manifest).unwrap();
    dir
}

fn edit(file: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(file).unwrap();
    assert!(text.contains(from), "the module source has `{from}`");
    std::fs::write(file, text.replacen(from, to, 1)).unwrap();
}

#[test]
fn edit_rebuild_reload_while_the_world_runs() {
    if !cfg!(windows) {
        println!("skipped: loading module libraries is Windows-only for now");
        return;
    }
    let cargo = specula_logic::build::cargo_program();
    if std::process::Command::new(&cargo).arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
        println!("skipped: cargo is not available");
        return;
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let repo = specula_logic::fix::canonical(&repo);
    let base = module_base(&repo);
    let dir = module_copy(&repo, &base);
    let src = dir.join("src").join("lib.rs");
    let copies = base.join("loaded");
    let builds = BuildService::default();
    let build = |fix: bool| {
        let mut req = BuildRequest::new(&dir);
        req.fix = fix;
        // The copy, and the SDK it depends on in the repository.
        req.roots = vec![base.clone(), repo.clone()];
        req.target_dir = Some(base.join("target"));
        builds.build(req)
    };

    // The copy's profile says panic = "abort" (the reviewer's abortmod): built by plain Cargo the
    // SDK refuses to compile; logic_build's build overrides it to unwind (all builds below), so a
    // panicking module can't take the engine down (the panic step below).
    let plain = specula_logic::build::cargo_command()
        .args(["check", "--offline", "--lib", "--color", "never", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(base.join("target"))
        .current_dir(&dir)
        .output()
        .expect("cargo runs");
    let said = String::from_utf8_lossy(&plain.stderr);
    assert!(!plain.status.success() && said.contains("panic = \"unwind\""), "the SDK refuses panic = abort: {said}");

    // A toolchain file in the module folder (the reviewer's tcmod) is refused before anything runs.
    std::fs::write(dir.join("rust-toolchain.toml"), "[toolchain]\npath = \"C:/nowhere/faketc\"\n").unwrap();
    let refused = build(false);
    assert_eq!(refused.error.as_ref().map(|e| e.0), Some("toolchain_file"), "{:?}", refused.error);
    std::fs::remove_file(dir.join("rust-toolchain.toml")).unwrap();

    // First build (may compile the SDK into the copy's target folder once).
    let first = build(false);
    assert!(first.ok, "first build: {:?} {:?} {:?}", first.error, first.diagnostics, first.stderr_tail);
    let library = first.library.clone().expect("a cdylib");
    println!("first build: {:.0} ms", first.build_time.as_secs_f64() * 1000.0);

    let mut r = Run { host: LogicHost::new(), world: World::default(), tick: 0 };
    let report = r.load(&library, &copies);
    assert_eq!(report.get("status").and_then(Json::as_str), Some("running"), "{report}");
    let id = r.player().id;
    let ground = r.world.find_path("/ambulo/ground").expect("the module built a ground slab");

    // Walk east for a second, then jump.
    r.hold(r#"{"move": [1, 0]}"#, 30);
    r.ticks(30);
    let x1 = r.player().position.x;
    assert!((x1 - 4.0).abs() < 0.2, "4 m/s for 1 s: x = {x1}");
    r.hold(r#"{"jump": true}"#, 1);
    r.ticks(5);
    assert!(r.player().position.y > 0.3, "in the air: y = {}", r.player().position.y);
    r.ticks(40);
    assert!(r.player().position.y.abs() < 1e-5, "landed: y = {}", r.player().position.y);
    let at = r.player().position;
    let status = r.status();
    assert_eq!(status.get("state").and_then(|s| s.get("jumps")).and_then(Json::as_u64), Some(1));

    // Another agent edits the module (double the walking speed) and rebuilds it; the world keeps
    // its entities and the module its state.
    let t0 = Instant::now();
    edit(&src, "const WALK_SPEED: f32 = 4.0;", "const WALK_SPEED: f32 = 8.0;");
    let second = build(false);
    assert!(second.ok, "rebuild: {:?} {:?}", second.error, second.diagnostics);
    let report = r.load(second.library.as_ref().unwrap(), &copies);
    let edit_to_reload = t0.elapsed();
    println!(
        "edit -> reload: {:.0} ms (cargo {:.0} ms, load + start {:.1} ms)",
        edit_to_reload.as_secs_f64() * 1000.0,
        second.build_time.as_secs_f64() * 1000.0,
        report.get("timings_ms").and_then(|t| t.get("load")).and_then(Json::as_f64).unwrap_or(0.0)
    );
    assert_eq!(report.get("event").and_then(Json::as_str), Some("reload"));
    assert_eq!(report.get("generation").and_then(Json::as_u64), Some(2));
    assert_ne!(report.get("build_hash"), status.get("build_hash"), "a new build");
    assert_eq!(r.player().id, id, "the same player entity");
    assert_eq!(r.player().position, at, "nothing moved across the reload");
    assert!(r.world.get(ground).is_some(), "the module's world is still there (not rebuilt)");
    let status = r.status();
    assert_eq!(status.get("state").and_then(|s| s.get("jumps")).and_then(Json::as_u64), Some(1), "the state store survived");
    assert_eq!(status.get("state").and_then(|s| s.get("loads")).and_then(Json::as_u64), Some(2));
    r.hold(r#"{"move": [1, 0]}"#, 15);
    r.ticks(15);
    let dx = r.player().position.x - at.x;
    assert!((dx - 4.0).abs() < 0.2, "the new code runs: 8 m/s for 0.5 s, moved {dx}");

    // A compile error: data, not a wall of text; the running module is untouched.
    edit(&src, "let dt = cx.dt();", "let dt: u32 = cx.dt();");
    let broken = build(false);
    assert!(!broken.ok);
    assert_eq!(broken.error.as_ref().map(|e| e.0), Some("compile_errors"));
    let e = broken.diagnostics.iter().find(|d| d.code.as_deref() == Some("E0308")).expect("a mismatched-types error");
    assert!(e.is_error());
    assert!(e.file.as_ref().is_some_and(|f| f.ends_with("src/lib.rs") || f.ends_with("src\\lib.rs")), "{e:?}");
    let line = std::fs::read_to_string(&src).unwrap().lines().position(|l| l.contains("let dt: u32")).unwrap() as u32 + 1;
    assert_eq!(e.line, line);
    assert!(r.host.running("ambulo"));
    edit(&src, "let dt: u32 = cx.dt();", "let dt = cx.dt();");

    // A warning with a machine-applicable fix: applied in place, then rebuilt clean.
    edit(&src, "let dt = cx.dt();", "let mut dt = cx.dt();");
    let fixed = build(true);
    assert!(fixed.ok, "{:?}", fixed.error);
    assert_eq!(fixed.fixes_applied.len(), 1, "{:?}", fixed.fixes_applied);
    assert_eq!(fixed.warnings(), 0, "rebuilt without the warning: {:?}", fixed.diagnostics);
    assert!(std::fs::read_to_string(&src).unwrap().contains("let dt = cx.dt();"), "the fix was written to the source");

    // A build that panics mid-tick is disabled; the tick changed nothing (and the process lives:
    // the manifest asks for panic = "abort", the build unwinds anyway).
    edit(&src, "let dt = cx.dt();", "let dt = cx.dt();\n        if cx.tick() >= 200 { panic!(\"probe panic\"); }");
    let bomb = build(false);
    assert!(bomb.ok, "{:?}", bomb.diagnostics);
    r.load(bomb.library.as_ref().unwrap(), &copies);
    while r.tick < 200 {
        r.ticks(1);
    }
    let before = r.player().clone();
    r.hold(r#"{"move": [1, 0]}"#, 10);
    r.ticks(1);
    assert!(!r.host.running("ambulo"));
    let why = r.status().get("disabled_by").cloned().unwrap();
    assert_eq!(why.get("message").and_then(Json::as_str), Some("probe panic"), "{why}");
    assert!(why.get("location").and_then(Json::as_str).is_some_and(|l| l.contains("lib.rs")), "{why}");
    assert_eq!(r.player().position, before.position);
    assert_eq!(r.player().version, before.version);

    // Unloading runs the module's stop (it removes what it built) and frees the library.
    r.load(&library, &copies);
    r.host.unload("ambulo", false, &mut r.world, r.tick, HZ, &mut journal()).unwrap();
    assert!(r.world.find_path("/ambulo").is_err(), "stop(unload) removed the module's entities");
    drop(r);
    let left = std::fs::read_dir(&copies).map(|d| d.count()).unwrap_or(0);
    assert_eq!(left, 0, "every loaded copy was deleted after unloading");
}
