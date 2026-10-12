//! Scaffold BRP-enabled Bevy games from in-repo templates.
//!
//! Templates under `<repo>/templates/{game-2d,game-3d,sample-app}` are the source of
//! truth at development time. They are also **embedded** into the CLI binary (G6) so
//! `cargo install` / installed MCP can scaffold without a monorepo checkout.
//!
//! Resolution order for the template root:
//! 1. `SPECULA_TEMPLATE_ROOT` (override)
//! 2. Monorepo `templates/` next to the crate (dev / `cargo install --path`)
//! 3. Embedded templates extracted to a versioned cache directory

use anyhow::{bail, Context, Result};
use include_dir::{include_dir, Dir};
use std::fs;
use std::path::{Path, PathBuf};

/// Embedded copy of monorepo `templates/` (baked at compile time).
static EMBEDDED_TEMPLATES: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/../../templates");

/// Which project template to materialize.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaffoldKind {
    /// Production 2D vertical slice (`templates/game-2d`).
    TwoD,
    /// Production 3D vertical slice (`templates/game-3d`).
    ThreeD,
    /// BRP integration fixture (`templates/sample-app`) — not a production game.
    Demo,
}

impl ScaffoldKind {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "2d" | "two-d" | "twod" => Ok(Self::TwoD),
            "3d" | "three-d" | "threed" => Ok(Self::ThreeD),
            "demo" | "sample" | "fixture" => Ok(Self::Demo),
            other => bail!(
                "unknown scaffold kind '{other}' (expected 2d, 3d, or demo)"
            ),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::TwoD => "2d",
            Self::ThreeD => "3d",
            Self::Demo => "demo",
        }
    }

    fn template_dir_name(self) -> &'static str {
        match self {
            Self::TwoD => "game-2d",
            Self::ThreeD => "game-3d",
            Self::Demo => "sample-app",
        }
    }
}

/// How templates were resolved (for tests and mcp-config messaging).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateOrigin {
    /// `SPECULA_TEMPLATE_ROOT`
    Env,
    /// Monorepo / compile-time path relative to crate
    Disk,
    /// Extracted from binary-embedded templates
    Embedded,
}

/// Disk-only template root: env override, then monorepo path. No embedded fallback.
///
/// Used by tests that need to prove disk discovery still works, and by
/// `template_root` before falling back to embed.
pub fn template_root_disk() -> Result<(PathBuf, TemplateOrigin)> {
    // GROK_BEVY_TEMPLATE_ROOT is the pre-rename name; honoured when the new one is unset.
    let env_root = std::env::var("SPECULA_TEMPLATE_ROOT")
        .or_else(|_| std::env::var("GROK_BEVY_TEMPLATE_ROOT"));
    if let Ok(p) = env_root {
        let path = PathBuf::from(p);
        if path.is_dir() {
            return Ok((path, TemplateOrigin::Env));
        }
        bail!(
            "SPECULA_TEMPLATE_ROOT={} is not a directory",
            path.display()
        );
    }
    let from_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templates");
    if from_manifest.is_dir() {
        let canon = from_manifest
            .canonicalize()
            .with_context(|| format!("canonicalize {}", from_manifest.display()))?;
        return Ok((canon, TemplateOrigin::Disk));
    }
    bail!(
        "disk templates not found at {}",
        from_manifest.display()
    )
}

/// Cache directory for extracted embedded templates (`…/specula/templates/<version>`).
pub fn embedded_templates_cache_dir() -> PathBuf {
    let version = env!("CARGO_PKG_VERSION");
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg)
            .join("specula")
            .join("templates")
            .join(version);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("specula")
            .join("templates")
            .join(version);
    }
    std::env::temp_dir()
        .join("specula")
        .join("templates")
        .join(version)
}

/// Extract embedded templates into `dest` (must not already contain a complete tree).
pub fn extract_embedded_templates(dest: &Path) -> Result<()> {
    fs::create_dir_all(dest).with_context(|| format!("create {}", dest.display()))?;
    EMBEDDED_TEMPLATES
        .extract(dest)
        .with_context(|| format!("extract embedded templates to {}", dest.display()))?;
    // Sanity: required kits present after extract.
    for kind in ["game-2d", "game-3d", "sample-app"] {
        let p = dest.join(kind);
        if !p.is_dir() {
            bail!(
                "embedded extract incomplete: missing {} under {}",
                kind,
                dest.display()
            );
        }
    }
    fs::write(dest.join(".specula-templates-complete"), env!("CARGO_PKG_VERSION"))
        .with_context(|| format!("write marker under {}", dest.display()))?;
    Ok(())
}

/// Ensure embedded templates are available on disk (cache), return that root.
pub fn ensure_embedded_template_root() -> Result<PathBuf> {
    let cache = embedded_templates_cache_dir();
    let marker = cache.join(".specula-templates-complete");
    if marker.is_file()
        && cache.join("game-2d").is_dir()
        && cache.join("game-3d").is_dir()
        && cache.join("sample-app").is_dir()
    {
        return Ok(cache);
    }
    if cache.exists() {
        let _ = fs::remove_dir_all(&cache);
    }
    extract_embedded_templates(&cache)?;
    Ok(cache)
}

/// Resolve the templates directory (source of truth for scaffold).
///
/// Order: `SPECULA_TEMPLATE_ROOT` → monorepo `templates/` → embedded cache (G6).
pub fn template_root() -> Result<PathBuf> {
    Ok(template_root_with_origin()?.0)
}

/// Like [`template_root`] but also reports how the path was obtained.
pub fn template_root_with_origin() -> Result<(PathBuf, TemplateOrigin)> {
    match template_root_disk() {
        Ok(pair) => Ok(pair),
        Err(_) => {
            let path = ensure_embedded_template_root()?;
            Ok((path, TemplateOrigin::Embedded))
        }
    }
}

/// Whether the binary has the embedded template set (compile-time).
pub fn embedded_templates_available() -> bool {
    EMBEDDED_TEMPLATES.get_dir("game-2d").is_some()
        && EMBEDDED_TEMPLATES.get_dir("game-3d").is_some()
        && EMBEDDED_TEMPLATES.get_dir("sample-app").is_some()
}

/// Normalize a user package name to a valid Cargo/Rust crate name (snake_case).
pub fn normalize_package_name(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == '_' || ch == ' ' {
            if !out.ends_with('_') && !out.is_empty() {
                out.push('_');
            }
        }
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        return "my_bevy_game".into();
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return format!("game_{out}");
    }
    out
}

/// Default package name from destination path or kind.
pub fn default_package_name(path: &Path, kind: ScaffoldKind) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(normalize_package_name)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| match kind {
            ScaffoldKind::TwoD => "my_bevy_2d_game".into(),
            ScaffoldKind::ThreeD => "my_bevy_3d_game".into(),
            ScaffoldKind::Demo => "specula_sample".into(),
        })
}

/// Scaffold a game project into `dest`.
pub fn scaffold_app(
    dest: &Path,
    kind: ScaffoldKind,
    package_name: Option<&str>,
    force: bool,
) -> Result<()> {
    scaffold_app_with_root(dest, kind, package_name, force, None)
}

/// True when scaffolding would wipe the current working directory (or `.`).
///
/// Used to refuse `scaffold --path . --force` which previously called
/// `remove_dir_all(".")` and failed opaquely / dangerously.
pub fn is_cwd_or_dot_dest(dest: &Path) -> bool {
    if dest.as_os_str().is_empty() {
        return true;
    }
    if dest == Path::new(".") || dest == Path::new("./") {
        return true;
    }
    if dest.components().all(|c| matches!(c, std::path::Component::CurDir)) {
        return true;
    }
    let Ok(cwd) = std::env::current_dir() else {
        return false;
    };
    if dest == cwd {
        return true;
    }
    if let (Ok(d), Ok(c)) = (dest.canonicalize(), cwd.canonicalize()) {
        return d == c;
    }
    false
}

/// Validate destination before copy. Returns clear guidance instead of wiping `.`.
pub fn validate_scaffold_destination(dest: &Path, force: bool) -> Result<()> {
    if is_cwd_or_dot_dest(dest) {
        bail!(
            "refusing to scaffold into the current directory (path `{}`). \
             Scaffolding with --force would wipe this folder. \
             Use a subdirectory instead, e.g. `specula scaffold --kind 2d --path ./my-game --name my-game`",
            dest.display()
        );
    }
    if dest.exists() {
        if !force {
            bail!(
                "destination {} already exists. Pass --force to overwrite a *dedicated* game dir, \
                 or use a new subdirectory (e.g. --path ./my-game). \
                 Never point --path at a monorepo root or `.`",
                dest.display()
            );
        }
        if !dest.is_dir() {
            bail!("{} exists and is not a directory", dest.display());
        }
    }
    Ok(())
}

/// Scaffold using an explicit template root (tests / forced embedded path).
///
/// When `template_root_override` is `None`, uses [`template_root`].
pub fn scaffold_app_with_root(
    dest: &Path,
    kind: ScaffoldKind,
    package_name: Option<&str>,
    force: bool,
    template_root_override: Option<&Path>,
) -> Result<()> {
    validate_scaffold_destination(dest, force)?;

    let pkg = normalize_package_name(package_name.unwrap_or(&default_package_name(dest, kind)));
    let title = match kind {
        ScaffoldKind::TwoD => format!("{pkg} (2D)"),
        ScaffoldKind::ThreeD => format!("{pkg} (3D)"),
        ScaffoldKind::Demo => format!("{pkg} (demo)"),
    };

    let templates = match template_root_override {
        Some(p) => p.to_path_buf(),
        None => template_root()?,
    };
    let src = templates.join(kind.template_dir_name());
    if !src.is_dir() {
        bail!("template missing: {}", src.display());
    }

    if dest.exists() {
        // force already validated; safe dedicated dir only
        fs::remove_dir_all(dest)
            .with_context(|| format!("remove existing {}", dest.display()))?;
    }
    fs::create_dir_all(dest).with_context(|| format!("create {}", dest.display()))?;

    copy_template(&src, dest, &pkg, &title, kind)?;

    // Ensure asset layout contract even for demo.
    ensure_asset_dirs(dest, kind)?;

    // Demo template may lack AGENTS; always write production-oriented project rules.
    write_project_agents(dest, kind, &pkg)?;

    println!(
        "Scaffolded Bevy {} project at {}",
        kind.as_str(),
        dest.display()
    );
    println!("  package: {pkg}");
    println!("  Features: `remote` (BRP), `capture` (screenshots); optional `physics` (Avian 0.7)");
    println!(
        "  Run: cargo run --manifest-path {} --features remote,capture",
        dest.join("Cargo.toml").display()
    );
    if matches!(kind, ScaffoldKind::Demo) {
        println!("  Note: demo is a BRP fixture, not a production game template.");
    }
    Ok(())
}

fn ensure_asset_dirs(dest: &Path, kind: ScaffoldKind) -> Result<()> {
    let dirs: &[&str] = match kind {
        ScaffoldKind::TwoD => &["assets/sprites", "assets/ui", "assets/audio"],
        ScaffoldKind::ThreeD => &[
            "assets/models",
            "assets/ui",
            "assets/audio",
            "assets/sprites",
        ],
        ScaffoldKind::Demo => &["assets/sprites", "assets/models", "assets/ui", "assets/audio"],
    };
    for d in dirs {
        let p = dest.join(d);
        fs::create_dir_all(&p).with_context(|| format!("create {}", p.display()))?;
        let keep = p.join(".gitkeep");
        if !keep.exists() && fs::read_dir(&p).map(|mut i| i.next().is_none()).unwrap_or(true) {
            let _ = fs::write(&keep, b"");
        }
    }
    Ok(())
}

fn write_project_agents(dest: &Path, kind: ScaffoldKind, pkg: &str) -> Result<()> {
    let path = dest.join("AGENTS.md");
    // Prefer template AGENTS when present (already token-replaced); refresh always for contract.
    let body = match kind {
        ScaffoldKind::TwoD => format!(
            r#"# Project agent rules — 2D Bevy game (`{pkg}`)

## Pins
- Bevy **0.19**, bevy_brp_extras **0.22.1**, BRP port **15702**
- Features: `remote`, `capture`; optional `physics` → avian2d **0.7**

## Skills
1. `bevy-production` + `bevy-2d-game`
2. Art → `game-asset-core` (+ specialist)
3. Live verify → `bevy-agent-loop`

## Assets
`assets/sprites/`, `assets/ui/`, `assets/audio/` (paths relative to `assets/` for AssetServer).
Debug builds use crate-root assets via `AssetPlugin` + `CARGO_MANIFEST_DIR`; release expects `assets/` beside the binary (or `BEVY_ASSET_ROOT`).

## ECS queries (B0001)
Overlapping `Query<&mut T>` systems panic at runtime (Bevy **B0001**). Prefer marker components, `Without`, `ParamSet`, or split systems.

## States
`Loading` → `MainMenu` → `Playing` / `Paused`

## Ship
```bash
cargo build --release
```
See Specula `docs/ASSET_CONVENTIONS.md` and `docs/SHIPPING.md` when available.
"#
        ),
        ScaffoldKind::ThreeD => format!(
            r#"# Project agent rules — 3D Bevy game (`{pkg}`)

## Pins
- Bevy **0.19**, bevy_brp_extras **0.22.1**, BRP port **15702**
- Features: `remote`, `capture`; optional `physics` → avian3d **0.7**

## Skills
1. `bevy-production` + `bevy-3d-game`
2. UI/art → `game-asset-core` (+ specialist)
3. Live verify → `bevy-agent-loop`

## Assets
`assets/models/`, `assets/ui/`, `assets/audio/` (optional `sprites/`).
Debug builds use crate-root assets via `AssetPlugin` + `CARGO_MANIFEST_DIR`; release expects `assets/` beside the binary (or `BEVY_ASSET_ROOT`).

## ECS queries (B0001)
Overlapping `Query<&mut T>` systems panic at runtime (Bevy **B0001**). Prefer marker components, `Without`, `ParamSet`, or split systems.

## States
`Loading` → `MainMenu` → `Playing` / `Paused`

## Ship
```bash
cargo build --release
```
See Specula `docs/ASSET_CONVENTIONS.md` and `docs/SHIPPING.md` when available.
"#
        ),
        ScaffoldKind::Demo => format!(
            r#"# Project agent rules — BRP demo fixture (`{pkg}`)

This is a **demo/fixture** for BRP smoke tests, not a production game.

For a real game:
```bash
specula scaffold --kind 2d --path ./my-game
# or --kind 3d
```

Load skills: `bevy-production` + dimensional skill + `bevy-agent-loop`.

Ship (if you expand this into a game): `cargo build --release`
"#
        ),
    };
    fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn copy_template(
    src: &Path,
    dest: &Path,
    package_name: &str,
    window_title: &str,
    kind: ScaffoldKind,
) -> Result<()> {
    copy_dir_recursive(src, dest, package_name, window_title, kind)
}

fn copy_dir_recursive(
    src: &Path,
    dest: &Path,
    package_name: &str,
    window_title: &str,
    kind: ScaffoldKind,
) -> Result<()> {
    fs::create_dir_all(dest).with_context(|| format!("create {}", dest.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("read_dir {}", src.display()))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        // Skip target/ and hidden cargo lock noise if any.
        if name == "target" || name == "Cargo.lock" {
            continue;
        }
        let from = entry.path();
        let to = dest.join(&name);
        if file_type.is_dir() {
            copy_dir_recursive(&from, &to, package_name, window_title, kind)?;
        } else if file_type.is_file() {
            copy_file_substituted(&from, &to, package_name, window_title, kind)?;
        }
    }
    Ok(())
}

fn is_text_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("rs" | "toml" | "md" | "txt" | "json" | "yml" | "yaml")
    )
}

fn copy_file_substituted(
    from: &Path,
    to: &Path,
    package_name: &str,
    window_title: &str,
    kind: ScaffoldKind,
) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    if is_text_file(from) {
        let mut text = fs::read_to_string(from)
            .with_context(|| format!("read {}", from.display()))?;
        text = text.replace("__PACKAGE_NAME__", package_name);
        text = text.replace("__WINDOW_TITLE__", window_title);
        // Demo template lives in the workspace; rewrite for a standalone project.
        if matches!(kind, ScaffoldKind::Demo) {
            text = text.replace("specula_sample", package_name);
            text = text.replace("Specula Sample", window_title);
            text = text.replace("Specula sample", window_title);
            text = text.replace(
                "bevy = { workspace = true, features = [\"png\"] }",
                "bevy = { version = \"0.19\", default-features = true, features = [\"png\"] }",
            );
            text = text.replace(
                "bevy_brp_extras = { workspace = true, optional = true }",
                "bevy_brp_extras = { version = \"0.22.1\", optional = true }",
            );
        }
        fs::write(to, text).with_context(|| format!("write {}", to.display()))?;
    } else {
        fs::copy(from, to).with_context(|| format!("copy {} → {}", from.display(), to.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_kinds() {
        assert_eq!(ScaffoldKind::parse("2d").unwrap(), ScaffoldKind::TwoD);
        assert_eq!(ScaffoldKind::parse("3D").unwrap(), ScaffoldKind::ThreeD);
        assert_eq!(ScaffoldKind::parse("demo").unwrap(), ScaffoldKind::Demo);
        assert!(ScaffoldKind::parse("4d").is_err());
    }

    #[test]
    fn package_name_normalization() {
        assert_eq!(normalize_package_name("My-Cool Game"), "my_cool_game");
        assert_eq!(normalize_package_name("123go"), "game_123go");
    }

    #[test]
    fn template_root_exists() {
        let root = template_root().expect("template root");
        assert!(root.join("game-2d").is_dir());
        assert!(root.join("game-3d").is_dir());
        assert!(root.join("sample-app").is_dir());
    }

    #[test]
    fn embedded_templates_are_baked_into_binary() {
        assert!(
            embedded_templates_available(),
            "include_dir must bake game-2d, game-3d, sample-app"
        );
        assert!(EMBEDDED_TEMPLATES
            .get_file("game-2d/Cargo.toml")
            .is_some());
        assert!(EMBEDDED_TEMPLATES
            .get_file("game-3d/Cargo.toml")
            .is_some());
        let cargo_2d = EMBEDDED_TEMPLATES
            .get_file("game-2d/Cargo.toml")
            .unwrap()
            .contents_utf8()
            .unwrap();
        assert!(
            cargo_2d.contains("physics") && cargo_2d.contains("avian2d"),
            "embedded 2d kit must declare optional physics/avian2d"
        );
        let cargo_3d = EMBEDDED_TEMPLATES
            .get_file("game-3d/Cargo.toml")
            .unwrap()
            .contents_utf8()
            .unwrap();
        assert!(
            cargo_3d.contains("physics") && cargo_3d.contains("avian3d"),
            "embedded 3d kit must declare optional physics/avian3d"
        );
    }

    #[test]
    fn scaffold_from_embedded_extract_without_env_or_monorepo_path() {
        // Simulate install: only embedded bytes → extract to private dir → scaffold.
        // Do not rely on SPECULA_TEMPLATE_ROOT or monorepo disk layout for the copy source.
        assert!(embedded_templates_available());
        let dir = tempfile::tempdir().unwrap();
        let embedded_root = dir.path().join("embedded_templates");
        extract_embedded_templates(&embedded_root).expect("extract embedded");

        // No env, no monorepo path used — only the extracted embedded tree.
        let dest_2d = dir.path().join("g6_2d");
        scaffold_app_with_root(
            &dest_2d,
            ScaffoldKind::TwoD,
            Some("g6_2d_game"),
            true,
            Some(&embedded_root),
        )
        .expect("scaffold 2d from embedded");
        assert!(dest_2d.join("Cargo.toml").is_file());
        assert!(dest_2d.join("src/main.rs").is_file());
        assert!(dest_2d.join("src/lib.rs").is_file());
        assert!(dest_2d.join("assets/sprites/player.png").is_file());
        let cargo = fs::read_to_string(dest_2d.join("Cargo.toml")).unwrap();
        assert!(cargo.contains("name = \"g6_2d_game\""));
        assert!(cargo.contains("avian2d"));
        assert!(cargo.contains("physics"));
        // Default features must not enable physics
        assert!(
            cargo.contains("default = []") || !cargo.contains("default = [\"physics\"]"),
            "physics must not be default"
        );

        let dest_3d = dir.path().join("g6_3d");
        scaffold_app_with_root(
            &dest_3d,
            ScaffoldKind::ThreeD,
            Some("g6_3d_game"),
            true,
            Some(&embedded_root),
        )
        .expect("scaffold 3d from embedded");
        assert!(dest_3d.join("assets/models/ground_tint.png").is_file());
        let cargo3 = fs::read_to_string(dest_3d.join("Cargo.toml")).unwrap();
        assert!(cargo3.contains("avian3d"));
        assert!(cargo3.contains("physics"));
    }

    #[test]
    fn ensure_embedded_root_creates_cache_when_disk_missing_path_forced() {
        // Direct extract path used when disk resolution fails in production.
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("cache");
        extract_embedded_templates(&dest).unwrap();
        assert!(dest.join("game-2d/src/plugins/core.rs").is_file());
        let core = fs::read_to_string(dest.join("game-2d/src/plugins/core.rs")).unwrap();
        assert!(core.contains("AssetPlugin"));
    }

    #[test]
    fn scaffold_2d_writes_playable_tree() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("cool_game");
        scaffold_app(&dest, ScaffoldKind::TwoD, Some("cool_game"), true).unwrap();

        assert!(dest.join("Cargo.toml").is_file());
        assert!(dest.join("src/main.rs").is_file());
        assert!(dest.join("src/lib.rs").is_file());
        assert!(dest.join("src/systems/gameplay.rs").is_file());
        assert!(dest.join("assets/sprites/player.png").is_file());
        assert!(dest.join("AGENTS.md").is_file());

        let cargo = fs::read_to_string(dest.join("Cargo.toml")).unwrap();
        assert!(cargo.contains("name = \"cool_game\""));
        assert!(cargo.contains("remote") && cargo.contains("capture"));
        assert!(cargo.contains("physics") && cargo.contains("avian2d"));

        let gameplay = fs::read_to_string(dest.join("src/systems/gameplay.rs")).unwrap();
        assert!(gameplay.contains("player_movement"));
        assert!(gameplay.contains("KeyCode"));
        assert!(gameplay.contains("ButtonInput"));
        assert!(
            gameplay.contains("Without<"),
            "scaffolded 2d gameplay must include Without filters"
        );

        let states = fs::read_to_string(dest.join("src/states.rs")).unwrap();
        assert!(states.contains("MainMenu") && states.contains("Playing"));

        let loading = fs::read_to_string(dest.join("src/systems/loading.rs")).unwrap();
        assert!(loading.contains("sprites/player.png"));
        assert!(loading.contains("AssetServer"));
        assert!(loading.contains("LoadingTimeout") || loading.contains("timeout"));

        let core = fs::read_to_string(dest.join("src/plugins/core.rs")).unwrap();
        assert!(core.contains("AssetPlugin"));
        assert!(core.contains("CARGO_MANIFEST_DIR") || core.contains("asset_root"));

        let agents = fs::read_to_string(dest.join("AGENTS.md")).unwrap();
        assert!(agents.contains("B0001") || agents.contains("ParamSet"));

        let main = fs::read_to_string(dest.join("src/main.rs")).unwrap();
        assert!(main.contains("cool_game::GamePlugin"));
        assert!(!main.contains("__PACKAGE_NAME__"));

        let lib = fs::read_to_string(dest.join("src/lib.rs")).unwrap();
        assert!(
            lib.contains("feature = \"physics\"") || lib.contains("cfg(feature = \"physics\")"),
            "lib must gate PhysicsPlugins"
        );
    }

    #[test]
    fn scaffold_3d_writes_playable_tree() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("space_run");
        scaffold_app(&dest, ScaffoldKind::ThreeD, Some("space_run"), true).unwrap();

        assert!(dest.join("assets/models/ground_tint.png").is_file());
        let gameplay = fs::read_to_string(dest.join("src/systems/gameplay.rs")).unwrap();
        assert!(gameplay.contains("player_movement"));
        assert!(gameplay.contains("Camera3d"));
        let loading = fs::read_to_string(dest.join("src/systems/loading.rs")).unwrap();
        assert!(loading.contains("models/ground_tint.png"));
        let core = fs::read_to_string(dest.join("src/plugins/core.rs")).unwrap();
        assert!(core.contains("AssetPlugin"));
        let agents = fs::read_to_string(dest.join("AGENTS.md")).unwrap();
        assert!(agents.contains("cargo build --release"));
        assert!(agents.contains("assets/models"));
        assert!(agents.contains("B0001") || agents.contains("ParamSet"));
        let cargo = fs::read_to_string(dest.join("Cargo.toml")).unwrap();
        assert!(cargo.contains("avian3d") && cargo.contains("physics"));
        let lib = fs::read_to_string(dest.join("src/lib.rs")).unwrap();
        assert!(lib.contains("cfg(feature = \"physics\")"));
    }

    #[test]
    fn scaffold_demo_rewrites_package() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("fixture_app");
        scaffold_app(&dest, ScaffoldKind::Demo, Some("fixture_app"), true).unwrap();
        let cargo = fs::read_to_string(dest.join("Cargo.toml")).unwrap();
        assert!(cargo.contains("name = \"fixture_app\""));
        assert!(dest.join("src/main.rs").is_file());
        assert!(dest.join("AGENTS.md").is_file());
        assert!(dest.join("assets/sprites").is_dir());
    }

    #[test]
    fn refuse_scaffold_into_dot_even_with_force() {
        let err = validate_scaffold_destination(Path::new("."), true).unwrap_err();
        let s = err.to_string();
        assert!(
            s.contains("current directory") || s.contains("subdirectory"),
            "expected clear subdir guidance, got: {s}"
        );
        assert!(s.contains("my-game") || s.contains("--path"));
    }

    #[test]
    fn refuse_scaffold_existing_without_force_mentions_subdir() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("taken");
        fs::create_dir_all(&dest).unwrap();
        let err = validate_scaffold_destination(&dest, false).unwrap_err();
        let s = err.to_string();
        assert!(s.contains("already exists"));
        assert!(s.contains("subdirectory") || s.contains("--force") || s.contains("my-game"));
    }

    #[test]
    fn force_overwrite_dedicated_subdir_ok() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("fresh_game");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("junk.txt"), b"x").unwrap();
        scaffold_app(&dest, ScaffoldKind::TwoD, Some("fresh_game"), true).unwrap();
        assert!(dest.join("Cargo.toml").is_file());
        assert!(!dest.join("junk.txt").exists());
    }
}
