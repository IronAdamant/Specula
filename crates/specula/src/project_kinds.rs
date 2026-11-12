//! Validation of project files by kind, before a write is committed (see `project_tools` and
//! docs/design/PROJECT_DATA.md). Each check is the parser the engine (or the hosted game) uses
//! to load the file, so a file that passes loads.
//!
//! Some files are checked against others: a recipe against the `style.json` that governs it, a
//! UI screen against its folder's `theme.json`; and an edit to a style or a theme checks every
//! file depending on it, refusing the edit if it would break one that loaded before.

use crate::project::{FileKind, ProjectFile};
use specula_core::Json;
use std::collections::BTreeMap;
use std::sync::Arc;

/// One thing validation says about a file.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    /// The file it is about (the edited one, or a file depending on it).
    pub file: String,
    /// `error` (refuses the write) or `warning`.
    pub severity: &'static str,
    pub code: String,
    /// Where in the file: a JSON path, `line N`, or empty.
    pub at: String,
    pub message: String,
    /// Byte offset into the text (syntax errors).
    pub offset: Option<usize>,
}

impl Problem {
    pub fn to_json(&self) -> Json {
        let mut f = vec![("file", Json::from(self.file.as_str())), ("severity", Json::from(self.severity)), ("code", Json::from(self.code.as_str()))];
        if !self.at.is_empty() {
            f.push(("at", Json::from(self.at.as_str())));
        }
        f.push(("message", Json::from(self.message.as_str())));
        if let Some(o) = self.offset {
            f.push(("offset", Json::from(o)));
        }
        Json::obj(f)
    }

    pub fn is_error(&self) -> bool {
        self.severity == "error"
    }
}

fn from_json_error(file: &str, j: &Json) -> Problem {
    let s = |k: &str| j.get(k).and_then(Json::as_str).unwrap_or("").to_owned();
    let code = j.get("code").and_then(Json::as_str).or_else(|| j.get("error").and_then(Json::as_str)).unwrap_or("invalid").to_owned();
    Problem { file: file.to_owned(), severity: "error", code, at: s("path"), message: s("message"), offset: j.get("offset").and_then(Json::as_u64).map(|o| o as usize) }
}

fn warning(file: &str, code: &str, message: impl Into<String>) -> Problem {
    Problem { file: file.to_owned(), severity: "warning", code: code.to_owned(), at: String::new(), message: message.into(), offset: None }
}

fn json_syntax(file: &str, text: &str) -> Option<Problem> {
    Json::parse(text).err().map(|e| Problem { file: file.to_owned(), severity: "error", code: "json".into(), at: String::new(), message: e.message, offset: Some(e.offset) })
}

/// The folder of a project path (`assets/ui` for `assets/ui/hud.json`).
fn folder(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

/// The mount name a path starts with (`assets`, `modules/<name>`).
fn mount_of(path: &str) -> &str {
    if let Some(rest) = path.strip_prefix("modules/") {
        let n = rest.find('/').unwrap_or(rest.len());
        &path[..8 + n]
    } else {
        path.split('/').next().unwrap_or("")
    }
}

/// Problems no `allow_errors` overrides: for a module's `Cargo.toml` and `.rs` files, anything
/// that would let a crates.io crate, a build script or files from outside into a build; and any
/// string table that doesn't parse. A broken table shows the game no words, and it hides every
/// review it held from the review gate: one agent's broken draft locked everyone but reviewers
/// out of fixing the file (review R2 of m5-l10n, minor 5).
pub fn never_allowed(kind: FileKind, code: &str) -> bool {
    matches!(kind, FileKind::Strings) || (matches!(kind, FileKind::Cargo | FileKind::Rust) && matches!(code, "not_a_module" | "third_party_crate" | "build_script" | "not_allowed" | "source_path"))
}

/// The nearest `style.json` from `recipe`'s folder up to its mount's root, among those `exists`
/// says are there.
fn nearest_style(recipe: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let mount = mount_of(recipe);
    let mut dir = folder(recipe);
    loop {
        let candidate = format!("{dir}/style.json");
        if exists(&candidate) {
            return Some(candidate);
        }
        if dir == mount || dir.is_empty() {
            return None;
        }
        dir = folder(dir);
    }
}

/// The `style.json` governing a recipe: the nearest one from its folder up to its mount's root.
pub fn style_for(recipe: &str, files: &BTreeMap<String, ProjectFile>) -> Option<String> {
    nearest_style(recipe, |c| files.get(c).is_some_and(|f| f.kind == FileKind::Style))
}

/// The label of the engine's `--style FILE` in validation context (it governs every recipe).
pub const STYLE_OVERRIDE: &str = "(--style)";

/// The other files validating `path` reads (for snapshots): `(path, kind, text)`. `style_override`:
/// the engine's `--style FILE` text, which then governs every recipe.
///
/// For a style: every recipe it governs (or will, if it is new: a closer style takes over the
/// recipes below it), and the style each of them falls back to without it.
pub fn context(path: &str, kind: FileKind, files: &BTreeMap<String, ProjectFile>, style_override: Option<&Arc<str>>) -> Vec<(String, FileKind, Arc<str>)> {
    let get = |p: &str| files.get(p).map(|f| (p.to_owned(), f.kind, Arc::clone(&f.text)));
    match kind {
        FileKind::Recipe => match style_override {
            Some(t) => vec![(STYLE_OVERRIDE.to_owned(), FileKind::Style, Arc::clone(t))],
            None => style_for(path, files).and_then(|s| get(&s)).into_iter().collect(),
        },
        FileKind::UiScreen => get(&format!("{}/theme.json", folder(path))).into_iter().collect(),
        FileKind::Style if style_override.is_some() => Vec::new(),
        FileKind::Style => {
            let dir = folder(path);
            let is_style = |c: &str| files.get(c).is_some_and(|f| f.kind == FileKind::Style);
            let mut out = Vec::new();
            let mut others: Vec<String> = Vec::new();
            for (p, f) in files.iter().filter(|(p, f)| f.kind == FileKind::Recipe && p.starts_with(&format!("{dir}/"))) {
                if nearest_style(p, |c| c == path || is_style(c)).as_deref() != Some(path) {
                    continue;
                }
                out.push((p.clone(), f.kind, Arc::clone(&f.text)));
                if let Some(o) = nearest_style(p, |c| c != path && is_style(c)) {
                    if !others.contains(&o) {
                        others.push(o);
                    }
                }
            }
            out.extend(others.iter().filter_map(|o| get(o)));
            out
        }
        FileKind::UiTheme => {
            let dir = folder(path);
            files.iter().filter(|(p, f)| f.kind == FileKind::UiScreen && folder(p) == dir).map(|(p, f)| (p.clone(), f.kind, Arc::clone(&f.text))).collect()
        }
        _ => Vec::new(),
    }
}

/// [`check_module_manifest`], plus what needs the module's folder: `specula-sdk`'s path must lead
/// to the SDK crate itself (a `Cargo.toml` whose package is `specula-sdk`).
pub fn check_module_manifest_at(dir: &std::path::Path, file: &str, text: &str) -> Vec<Problem> {
    let mut out = check_module_manifest(file, text);
    if !out.is_empty() {
        return out;
    }
    let Ok(doc) = specula_core::toml::parse(text) else { return out };
    if let Some(p) = doc.get("dependencies").and_then(|d| d.get("specula-sdk")).and_then(|s| s.get("path")).and_then(Json::as_str) {
        let target = std::fs::read_to_string(dir.join(p).join("Cargo.toml")).ok().and_then(|t| specula_core::toml::parse(&t).ok());
        let name = target.as_ref().and_then(|t| t.get("package")).and_then(|p| p.get("name")).and_then(Json::as_str);
        if name != Some("specula-sdk") {
            out.push(Problem {
                file: file.to_owned(),
                severity: "error",
                code: "not_a_module".into(),
                at: "dependencies.specula-sdk.path".into(),
                message: format!("specula-sdk's path ({p}) doesn't lead to the specula-sdk crate"),
                offset: None,
            });
        }
    }
    out
}

/// Checks a module crate's `Cargo.toml` as text (what `logic_build` would refuse, without
/// running Cargo): a `cdylib` whose only dependency is `specula-sdk` by path; no build script
/// (`build` key), no `links`, no `[workspace]`, `[patch]`, `[replace]`, `[target]`,
/// `[build-dependencies]` or non-path dependencies. Every problem is returned.
pub fn check_module_manifest(file: &str, text: &str) -> Vec<Problem> {
    let err = |code: &str, at: &str, message: String| Problem { file: file.to_owned(), severity: "error", code: code.to_owned(), at: at.to_owned(), message, offset: None };
    let doc = match specula_core::toml::parse(text) {
        Ok(d) => d,
        Err(e) => return vec![Problem { file: file.to_owned(), severity: "error", code: e.code.to_owned(), at: format!("line {}", e.line), message: e.message, offset: None }],
    };
    let mut out = Vec::new();
    let Some(package) = doc.get("package") else {
        return vec![err("not_a_module", "package", "a module's Cargo.toml needs a [package] (a workspace root isn't a module)".into())];
    };
    for k in ["build", "links", "proc-macro"] {
        if package.get(k).is_some() {
            out.push(err("build_script", &format!("package.{k}"), format!("modules may not have package.{k}: nothing in a module's build may run code at build time")));
        }
    }
    for t in ["workspace", "patch", "replace", "target", "build-dependencies", "build_dependencies", "dev_dependencies", "bin", "example", "bench"] {
        if doc.get(t).is_some() {
            out.push(err("not_allowed", t, format!("[{t}] isn't allowed in a module's Cargo.toml (a module is one cdylib depending only on specula-sdk)")));
        }
    }
    let cdylib = doc.get("lib").and_then(|l| l.get("crate-type")).and_then(Json::as_array).is_some_and(|a| a.iter().any(|t| t.as_str() == Some("cdylib")));
    if !cdylib {
        out.push(err("not_a_module", "lib.crate-type", "a module is a cdylib: [lib] crate-type = [\"cdylib\"]".into()));
    }
    for k in ["proc-macro", "proc_macro", "crate_type", "plugin"] {
        if doc.get("lib").and_then(|l| l.get(k)).is_some() {
            out.push(err("build_script", &format!("lib.{k}"), format!("lib.{k} isn't allowed in a module (a cdylib, written crate-type; never a proc-macro)")));
        }
    }
    // The library's source stays in src/ (lib.path could compile any file).
    if let Some(p) = doc.get("lib").and_then(|l| l.get("path")) {
        let ok = p.as_str().is_some_and(|p| {
            let p = p.replace('\\', "/");
            p.starts_with("src/") && !p.contains(':') && !p.split('/').any(|s| s == "..")
        });
        if !ok {
            out.push(err("source_path", "lib.path", "lib.path must stay inside the module's src/ folder".into()));
        }
    }
    for table in ["dependencies", "dev-dependencies"] {
        for (name, spec) in doc.get(table).and_then(Json::as_object).unwrap_or_default() {
            let path = spec.get("path").and_then(Json::as_str);
            let foreign = ["git", "registry", "version", "registry-index"].iter().find(|k| spec.get(k).is_some());
            let renamed = spec.get("package").and_then(Json::as_str).unwrap_or(name);
            if path.is_none() || foreign.is_some() || spec.as_str().is_some() {
                out.push(err("third_party_crate", &format!("{table}.{name}"), format!("'{name}' must be a path dependency (zero third-party crates): {{ path = \"...\" }} only")));
            } else if table == "dependencies" && (name != "specula-sdk" || renamed != "specula-sdk") {
                out.push(err("not_a_module", &format!("{table}.{name}"), format!("modules depend on specula-sdk only, not '{renamed}' (engine crates are never module dependencies; no renaming with package =)")));
            }
        }
    }
    if doc.get("dependencies").and_then(|d| d.get("specula-sdk")).is_none() {
        out.push(err("not_a_module", "dependencies", "a module depends on specula-sdk (by path)".into()));
    }
    out
}

fn recipe(file: &str, text: &str, style: &specula_model::Style) -> Option<Problem> {
    specula_model::Recipe::parse(text, style).err().map(|e| from_json_error(file, &e.to_json()))
}

fn screen(file: &str, text: &str, theme: &specula_ui::Theme) -> Option<Problem> {
    specula_ui::Screen::parse(text, theme).err().map(|e| from_json_error(file, &e.to_json()))
}

/// Everything validation says about `text` as file `path` of `kind`, given the files it depends
/// on or that depend on it (`context`, from [`context`]). For a style or a theme, a dependent that
/// is broken now *and was fine with `old`* (the file's text before the edit) is an error; one that
/// was already broken is a warning, so fixing it isn't blocked.
pub fn validate(path: &str, kind: FileKind, text: &str, old: Option<&str>, context: &[(String, FileKind, Arc<str>)]) -> Vec<Problem> {
    let mut out = Vec::new();
    match kind {
        FileKind::Recipe => match context.iter().find(|c| c.1 == FileKind::Style) {
            Some((sp, _, st)) => match specula_model::Style::parse(st) {
                Ok(style) => out.extend(recipe(path, text, &style)),
                Err(e) => {
                    out.extend(json_syntax(path, text));
                    out.push(warning(path, "style_invalid", format!("its style {sp} doesn't parse ({}), so only JSON syntax was checked", e.message)));
                }
            },
            None => {
                out.extend(json_syntax(path, text));
                out.push(warning(path, "style_not_found", "no style.json governs this recipe in the project (next to it or above it), so only JSON syntax was checked; the engine will refuse to build it"));
            }
        },
        FileKind::Style => match specula_model::Style::parse(text) {
            Err(e) => out.push(from_json_error(path, &e.to_json())),
            Ok(style) => {
                let old_style = old.and_then(|o| specula_model::Style::parse(o).ok());
                for (rp, _, rt) in context.iter().filter(|c| c.1 == FileKind::Recipe) {
                    if let Some(mut p) = recipe(rp, rt, &style) {
                        // Fine before: with this file's old text, or (a new, closer style) with the
                        // style that governed the recipe until now.
                        let before = match (&old_style, old) {
                            (Some(b), _) => Some(b.clone()),
                            (None, Some(_)) => None,
                            (None, None) => other_style(rp, path, context),
                        };
                        let was_fine = before.as_ref().is_some_and(|b| recipe(rp, rt, b).is_none());
                        if !was_fine {
                            p.severity = "warning";
                            p.message = format!("(already broken before this edit) {}", p.message);
                        }
                        out.push(p);
                    }
                }
            }
        },
        FileKind::StyleRules => {
            if let Err(e) = specula_anim::checks::MotionRules::from_toml(text, path) {
                out.push(from_json_error(path, &e.to_json()));
            }
        }
        FileKind::UiScreen => match context.iter().find(|c| c.1 == FileKind::UiTheme) {
            Some((tp, _, tt)) => match specula_ui::Theme::from_json_str(tt) {
                Ok(theme) => out.extend(screen(path, text, &theme)),
                Err(e) => {
                    out.extend(json_syntax(path, text));
                    out.push(warning(path, "theme_invalid", format!("its theme {tp} doesn't parse ({}), so only JSON syntax was checked", e.message)));
                }
            },
            None => {
                out.extend(json_syntax(path, text));
                out.push(warning(path, "theme_not_found", "no theme.json in this folder, so only JSON syntax was checked"));
            }
        },
        FileKind::UiTheme => match specula_ui::Theme::from_json_str(text) {
            Err(e) => out.push(from_json_error(path, &e.to_json())),
            Ok(theme) => {
                let before = old.and_then(|o| specula_ui::Theme::from_json_str(o).ok());
                for (sp, _, st) in context.iter().filter(|c| c.1 == FileKind::UiScreen) {
                    if let Some(mut p) = screen(sp, st, &theme) {
                        let was_fine = before.as_ref().is_some_and(|b| screen(sp, st, b).is_none());
                        if !was_fine {
                            p.severity = "warning";
                            p.message = format!("(already broken before this edit) {}", p.message);
                        }
                        out.push(p);
                    }
                }
            }
        },
        FileKind::Sound => {
            if let Err(e) = specula_audio::Recipe::parse(text) {
                out.push(from_json_error(path, &e.to_json()));
            }
        }
        // A piece is parsed and validated whole (instrument recipes included); rendering waits
        // for whoever plays it.
        FileKind::Music => {
            if let Err(e) = specula_audio::music::Music::parse(text) {
                out.push(from_json_error(path, &e.to_json()));
            }
        }
        FileKind::AnimSet | FileKind::Json => out.extend(json_syntax(path, text)),
        FileKind::Taste => {
            if let Err(e) = crate::taste::validate_file(path, text) {
                out.push(Problem { file: path.to_owned(), severity: "error", code: e.code.into(), at: e.field.clone().unwrap_or_default(), message: e.message, offset: None });
            }
        }
        FileKind::StyleData => {
            if let Err(e) = specula_gustus::Priors::parse(text) {
                out.push(Problem { file: path.to_owned(), severity: "error", code: e.code.into(), at: e.field.clone().unwrap_or_default(), message: e.message, offset: None });
            }
        }
        // String tables: the format, keys, placeholders and plural rules (specula-lingua);
        // checks across tables (missing keys, stale translations) are l10n_check's.
        FileKind::Strings => {
            if let Err(e) = specula_lingua::Table::parse(text) {
                out.push(Problem { file: path.to_owned(), severity: "error", code: e.code.into(), at: e.field.clone(), message: e.to_string(), offset: e.offset });
            }
        }
        // Timelines: the format, then what the engine plays (anim and vfx values as world_set
        // takes them; docs/design/TIMELINE.md).
        FileKind::Timeline => out.extend(crate::timeline::validate_file(path, text)),
        // Notes are prose (the style configurator's): any text.
        FileKind::StyleNotes => {}
        // Binary files' bytes are checked by project_upload (project_binary::inspect); their
        // store text must name content.
        FileKind::Png | FileKind::Glb | FileKind::Gltf | FileKind::Wav | FileKind::Ogg => {
            if crate::blobs::parse_pointer(text).is_none() {
                out.push(Problem { file: path.to_owned(), severity: "error", code: "not_a_pointer".into(), at: String::new(), message: "a binary file's entry must name its content (write it with project_upload)".into(), offset: None });
            }
        }
        FileKind::Cargo => out.extend(check_module_manifest(path, text)),
        // Compile-time paths (`#[path]`, `include_str!`...) stay in the module's folder.
        FileKind::Rust => {
            let rel = path.strip_prefix(mount_of(path)).map_or(path, |r| r.trim_start_matches('/'));
            out.extend(specula_logic::paths::check_source(rel, text).into_iter().map(|p| Problem {
                file: path.to_owned(),
                severity: "error",
                code: "source_path".into(),
                at: format!("line {}", p.line),
                message: p.message,
                offset: None,
            }));
        }
    }
    out
}

/// The style a recipe falls back to without `path` (the nearest other style in `context`).
fn other_style(recipe_path: &str, path: &str, context: &[(String, FileKind, Arc<str>)]) -> Option<specula_model::Style> {
    let others: Vec<&(String, FileKind, Arc<str>)> = context.iter().filter(|c| c.1 == FileKind::Style && c.0 != path).collect();
    let p = nearest_style(recipe_path, |c| others.iter().any(|o| o.0 == c))?;
    others.iter().find(|o| o.0 == p).and_then(|o| specula_model::Style::parse(&o.2).ok())
}

/// What deleting `path` (whose text is `old`) breaks: a style's recipes are parsed with the style
/// they fall back to (none: they can't build), a theme's screens lose their theme. Only files that
/// loaded before count as errors.
pub fn validate_delete(path: &str, kind: FileKind, old: &str, context: &[(String, FileKind, Arc<str>)]) -> Vec<Problem> {
    let mut out = Vec::new();
    match kind {
        FileKind::Style => {
            let Ok(old_style) = specula_model::Style::parse(old) else { return out };
            for (rp, _, rt) in context.iter().filter(|c| c.1 == FileKind::Recipe) {
                if recipe(rp, rt, &old_style).is_some() {
                    continue; // already broken
                }
                match other_style(rp, path, context) {
                    Some(next) => out.extend(recipe(rp, rt, &next)),
                    None => out.push(Problem {
                        file: rp.clone(),
                        severity: "error",
                        code: "style_not_found".into(),
                        at: String::new(),
                        message: format!("deleting {path} leaves this recipe without a style.json (next to it or above it): it would no longer build"),
                        offset: None,
                    }),
                }
            }
        }
        FileKind::UiTheme => {
            let Ok(theme) = specula_ui::Theme::from_json_str(old) else { return out };
            for (sp, _, st) in context.iter().filter(|c| c.1 == FileKind::UiScreen) {
                if screen(sp, st, &theme).is_none() {
                    out.push(Problem {
                        file: sp.clone(),
                        severity: "error",
                        code: "theme_not_found".into(),
                        at: String::new(),
                        message: format!("deleting {path} leaves this screen without its theme: it would no longer load"),
                        offset: None,
                    });
                }
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sepulcra(rel: &str) -> String {
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets").join(rel)).unwrap()
    }

    fn file(kind: FileKind, text: &str) -> ProjectFile {
        ProjectFile::new(kind, Arc::from(text), 1, Arc::from("t"))
    }

    #[test]
    fn sepulcra_files_pass_and_breakage_is_caught() {
        let style = sepulcra("style.json");
        let tomb = sepulcra("recipes/tombstone.json");
        let mut files = BTreeMap::new();
        files.insert("assets/style.json".to_owned(), file(FileKind::Style, &style));
        files.insert("assets/recipes/tombstone.json".to_owned(), file(FileKind::Recipe, &tomb));
        files.insert("assets/ui/theme.json".to_owned(), file(FileKind::UiTheme, &sepulcra("ui/theme.json")));
        files.insert("assets/ui/hud.json".to_owned(), file(FileKind::UiScreen, &sepulcra("ui/hud.json")));
        assert_eq!(style_for("assets/recipes/tombstone.json", &files).as_deref(), Some("assets/style.json"));
        let ctx = context("assets/recipes/tombstone.json", FileKind::Recipe, &files, None);
        assert_eq!(ctx.len(), 1);
        assert!(validate("assets/recipes/tombstone.json", FileKind::Recipe, &tomb, None, &ctx).is_empty());
        let broken = tomb.replace("\"graveyard_stone\"", "\"no_such_material\"");
        let p = validate("assets/recipes/tombstone.json", FileKind::Recipe, &broken, None, &ctx);
        assert!(p.iter().any(Problem::is_error), "{p:?}");
        // A style edit that breaks a recipe is an error; one that leaves them all loading isn't.
        let sctx = context("assets/style.json", FileKind::Style, &files, None);
        assert_eq!(sctx.len(), 1);
        assert!(validate("assets/style.json", FileKind::Style, &style, Some(&style), &sctx).is_empty());
        let s2 = Json::parse(&style).unwrap();
        let mut s2 = s2;
        if let Some(Json::Obj(mats)) = s2.as_object().and_then(|o| o.iter().find(|(k, _)| k == "materials")).map(|(_, v)| v.clone()) {
            let kept: Vec<(String, Json)> = mats.into_iter().filter(|(k, _)| k != "graveyard_stone").collect();
            s2.set("materials", Json::Obj(kept));
            let p = validate("assets/style.json", FileKind::Style, &s2.to_pretty(), Some(&style), &sctx);
            assert!(p.iter().any(|p| p.is_error() && p.file == "assets/recipes/tombstone.json"), "{p:?}");
        }
        // Screens against their theme.
        let hud = sepulcra("ui/hud.json");
        let uctx = context("assets/ui/hud.json", FileKind::UiScreen, &files, None);
        assert!(validate("assets/ui/hud.json", FileKind::UiScreen, &hud, None, &uctx).is_empty());
        let p = validate("assets/ui/hud.json", FileKind::UiScreen, &hud.replace("specula.ui.screen.v1", "v0"), None, &uctx);
        assert!(p.iter().any(Problem::is_error));
        // Sounds, style rules, JSON syntax.
        assert!(validate("assets/sounds/ui_click.json", FileKind::Sound, &sepulcra("sounds/ui_click.json"), None, &[]).is_empty());
        assert!(!validate("assets/sounds/x.json", FileKind::Sound, "{\"sound\": 2}", None, &[]).is_empty());
        assert!(validate("assets/style.toml", FileKind::StyleRules, &sepulcra("style.toml"), None, &[]).is_empty());
        assert!(!validate("assets/style.toml", FileKind::StyleRules, "[motion]\nbogus = 1\n", None, &[]).is_empty());
        assert!(!validate("assets/anims/a.json", FileKind::AnimSet, "{", None, &[]).is_empty());
        assert!(validate("modules/m/src/lib.rs", FileKind::Rust, "fn broken(", None, &[]).is_empty(), "rust is logic_build's");
        // String tables: the format, keys and placeholders (docs/design/LOCALISATION.md).
        let en = sepulcra("strings/en.json");
        assert!(validate("assets/strings/en.json", FileKind::Strings, &en, None, &[]).is_empty());
        let p = validate("assets/strings/en.json", FileKind::Strings, &en.replace("\"Wave {wave}/{waves}\"", "\"Wave {wave/{waves}\""), None, &[]);
        assert!(p.iter().any(|p| p.is_error() && p.code == "bad_placeholder" && p.at.starts_with("strings.")), "{p:?}");
    }

    fn broken_style() -> String {
        let mut s = Json::parse(&sepulcra("style.json")).unwrap();
        let mats: Vec<(String, Json)> = s.get("materials").and_then(Json::as_object).unwrap().iter().filter(|(k, _)| k != "graveyard_stone").cloned().collect();
        s.set("materials", Json::Obj(mats));
        s.to_pretty()
    }

    fn sepulcra_store() -> BTreeMap<String, ProjectFile> {
        let mut files = BTreeMap::new();
        files.insert("assets/style.json".to_owned(), file(FileKind::Style, &sepulcra("style.json")));
        for r in ["tombstone", "wall_segment", "rune_glyph"] {
            files.insert(format!("assets/recipes/{r}.json"), file(FileKind::Recipe, &sepulcra(&format!("recipes/{r}.json"))));
        }
        files.insert("assets/ui/theme.json".to_owned(), file(FileKind::UiTheme, &sepulcra("ui/theme.json")));
        files.insert("assets/ui/hud.json".to_owned(), file(FileKind::UiScreen, &sepulcra("ui/hud.json")));
        files
    }

    /// Review findings: a new, closer style.json wasn't checked against the recipes it takes over;
    /// deleting a style or theme wasn't validated; the cross-check stopped at 256 files; --style
    /// was ignored.
    #[test]
    fn style_and_theme_holes_are_closed() {
        let files = sepulcra_store();
        // A new style next to the recipes, missing a material the tombstone uses: refused.
        let near = "assets/recipes/style.json";
        let ctx = context(near, FileKind::Style, &files, None);
        assert_eq!(ctx.iter().filter(|c| c.1 == FileKind::Recipe).count(), 3, "it takes over every recipe below it");
        assert!(ctx.iter().any(|c| c.0 == "assets/style.json"), "and knows what they fall back to");
        let p = validate(near, FileKind::Style, &broken_style(), None, &ctx);
        assert!(p.iter().any(|p| p.is_error() && p.file == "assets/recipes/tombstone.json"), "{p:?}");
        assert!(validate(near, FileKind::Style, &sepulcra("style.json"), None, &ctx).iter().all(|p| !p.is_error()));
        // Deleting the only style breaks every recipe; deleting the theme breaks the screens.
        let sctx = context("assets/style.json", FileKind::Style, &files, None);
        let p = validate_delete("assets/style.json", FileKind::Style, &sepulcra("style.json"), &sctx);
        assert_eq!(p.iter().filter(|p| p.code == "style_not_found").count(), 3, "{p:?}");
        let tctx = context("assets/ui/theme.json", FileKind::UiTheme, &files, None);
        let p = validate_delete("assets/ui/theme.json", FileKind::UiTheme, &sepulcra("ui/theme.json"), &tctx);
        assert!(p.iter().any(|p| p.file == "assets/ui/hud.json" && p.is_error()));
        // Deleting a closer style falls back to the one above: fine when that one works.
        let mut with_near = files.clone();
        with_near.insert(near.to_owned(), file(FileKind::Style, &sepulcra("style.json")));
        let nctx = context(near, FileKind::Style, &with_near, None);
        assert!(validate_delete(near, FileKind::Style, &sepulcra("style.json"), &nctx).is_empty());
        // Every dependent is checked, however many.
        let mut many = files.clone();
        for i in 0..300 {
            many.insert(format!("assets/recipes/t{i:03}.json"), file(FileKind::Recipe, &sepulcra("recipes/tombstone.json")));
        }
        assert_eq!(context("assets/style.json", FileKind::Style, &many, None).iter().filter(|c| c.1 == FileKind::Recipe).count(), 303);
        // --style: recipes are checked against it, and a style.json in the assets governs nothing.
        let over: Arc<str> = Arc::from(broken_style());
        let rctx = context("assets/recipes/tombstone.json", FileKind::Recipe, &files, Some(&over));
        assert_eq!(rctx[0].0, STYLE_OVERRIDE);
        assert!(validate("assets/recipes/tombstone.json", FileKind::Recipe, &sepulcra("recipes/tombstone.json"), None, &rctx).iter().any(Problem::is_error));
        assert!(context("assets/style.json", FileKind::Style, &files, Some(&over)).is_empty());
    }

    /// Review finding: project_mount took any folder with a Cargo.toml (engine crates, the root
    /// package), and Cargo.toml edits adding crates.io dependencies or a build script landed unchecked.
    #[test]
    fn only_module_manifests_pass() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let ambulo = std::fs::read_to_string(root.join("games/ambulo/Cargo.toml")).unwrap();
        assert!(check_module_manifest("m", &ambulo).is_empty(), "{:?}", check_module_manifest("m", &ambulo));
        let engine = std::fs::read_to_string(root.join("crates/specula/Cargo.toml")).unwrap();
        assert!(check_module_manifest("m", &engine).iter().any(|p| p.code == "not_a_module"), "an engine crate isn't a module");
        let workspace = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
        assert!(!check_module_manifest("m", &workspace).is_empty(), "the workspace root isn't a module");
        let codes = |t: &str| check_module_manifest("m", t).into_iter().map(|p| p.code).collect::<Vec<_>>();
        let build = ambulo.replace("publish = false", "publish = false\nbuild = \"src/build.rs\"");
        assert!(codes(&build).contains(&"build_script".to_owned()));
        // Extra dependencies go in [dependencies] (the manifest ends with [lints]).
        let sdk = "specula-sdk = { path = \"../../crates/specula-sdk\" }";
        let dep = |extra: &str| ambulo.replace(sdk, &format!("{sdk}\n{extra}"));
        let serde = dep("serde = \"1\"");
        assert!(codes(&serde).contains(&"third_party_crate".to_owned()), "{:?}", codes(&serde));
        let git = dep("x = { git = \"https://example.invalid/x\" }");
        assert!(codes(&git).contains(&"third_party_crate".to_owned()));
        let engine_dep = dep("specula-core = { path = \"../../crates/specula-core\" }");
        assert!(codes(&engine_dep).contains(&"not_a_module".to_owned()));
        assert!(codes(&format!("{ambulo}[build-dependencies]\n")).contains(&"not_allowed".to_owned()));
        assert!(!validate("modules/ambulo/Cargo.toml", FileKind::Cargo, &serde, None, &[]).is_empty(), "every Cargo.toml commit is checked");
        // Review round 3: these can't be stored as drafts.
        for p in validate("modules/ambulo/Cargo.toml", FileKind::Cargo, &serde, None, &[]).iter().chain(&validate("modules/ambulo/Cargo.toml", FileKind::Cargo, &build, None, &[])) {
            assert!(never_allowed(FileKind::Cargo, &p.code), "{p:?}");
        }
        assert!(!never_allowed(FileKind::Recipe, "not_a_module") && !never_allowed(FileKind::Rust, "json"));
        // Review round 2: renaming another crate to specula-sdk, lib.path and the underscore aliases.
        let renamed = ambulo.replace(sdk, "specula-sdk = { path = \"../../crates/specula-core\", package = \"specula-core\" }");
        assert!(codes(&renamed).contains(&"not_a_module".to_owned()), "{:?}", codes(&renamed));
        let dir = root.join("games/ambulo");
        let moved = ambulo.replace(sdk, "specula-sdk = { path = \"../../crates/specula-core\" }");
        assert!(check_module_manifest_at(&dir, "m", &moved).iter().any(|p| p.at == "dependencies.specula-sdk.path"), "the path must lead to the SDK");
        assert!(check_module_manifest_at(&dir, "m", &ambulo).is_empty());
        assert!(codes(&ambulo.replace("crate-type = [\"cdylib\", \"rlib\"]", "crate-type = [\"cdylib\", \"rlib\"]\npath = \"../../crates/specula/src/lib.rs\"")).contains(&"source_path".to_owned()));
        assert!(codes(&ambulo.replace("crate-type = [\"cdylib\", \"rlib\"]", "crate-type = [\"cdylib\", \"rlib\"]\npath = \"src/lib.rs\"")).is_empty());
        assert!(codes(&format!("{ambulo}[build_dependencies]\n")).contains(&"not_allowed".to_owned()));
        assert!(codes(&ambulo.replace("crate-type = [\"cdylib\", \"rlib\"]", "crate-type = [\"cdylib\", \"rlib\"]\nproc_macro = true")).contains(&"build_script".to_owned()));
        // .rs commits: compile-time paths stay in the module's folder.
        assert!(validate("modules/ambulo/src/lib.rs", FileKind::Rust, "const X: &str = include_str!(\"C:/Windows/win.ini\");", None, &[]).iter().any(|p| p.code == "source_path"));
        assert!(validate("modules/ambulo/src/lib.rs", FileKind::Rust, "#[path = \"../../../crates/specula/src/lib.rs\"]\nmod e;", None, &[]).iter().any(|p| p.code == "source_path"));
        assert!(validate("modules/ambulo/src/lib.rs", FileKind::Rust, "const D: &str = include_str!(\"data.txt\");", None, &[]).is_empty());
    }
}
