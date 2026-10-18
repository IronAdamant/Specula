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

/// Files a cross-check reads at most (a style's recipes, a theme's screens).
pub const MAX_DEPENDENTS: usize = 256;

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
        let n = rest.find('/').map_or(rest.len(), |i| i);
        &path[..8 + n]
    } else {
        path.split('/').next().unwrap_or("")
    }
}

/// The `style.json` governing a recipe: the nearest one from its folder up to its mount's root.
pub fn style_for(recipe: &str, files: &BTreeMap<String, ProjectFile>) -> Option<String> {
    let mount = mount_of(recipe);
    let mut dir = folder(recipe);
    loop {
        let candidate = format!("{dir}/style.json");
        if files.get(&candidate).is_some_and(|f| f.kind == FileKind::Style) {
            return Some(candidate);
        }
        if dir == mount || dir.is_empty() {
            return None;
        }
        dir = folder(dir);
    }
}

/// The other files validating `path` reads (for snapshots): `(path, text)`.
pub fn context(path: &str, kind: FileKind, files: &BTreeMap<String, ProjectFile>) -> Vec<(String, FileKind, Arc<str>)> {
    let get = |p: &str| files.get(p).map(|f| (p.to_owned(), f.kind, Arc::clone(&f.text)));
    match kind {
        FileKind::Recipe => style_for(path, files).and_then(|s| get(&s)).into_iter().collect(),
        FileKind::UiScreen => get(&format!("{}/theme.json", folder(path))).into_iter().collect(),
        FileKind::Style => {
            let dir = folder(path);
            files
                .iter()
                .filter(|(p, f)| f.kind == FileKind::Recipe && p.starts_with(&format!("{dir}/")) && style_for(p, files).as_deref() == Some(path))
                .take(MAX_DEPENDENTS)
                .map(|(p, f)| (p.clone(), f.kind, Arc::clone(&f.text)))
                .collect()
        }
        FileKind::UiTheme => {
            let dir = folder(path);
            files.iter().filter(|(p, f)| f.kind == FileKind::UiScreen && folder(p) == dir).take(MAX_DEPENDENTS).map(|(p, f)| (p.clone(), f.kind, Arc::clone(&f.text))).collect()
        }
        _ => Vec::new(),
    }
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
                let before = old.and_then(|o| specula_model::Style::parse(o).ok());
                for (rp, _, rt) in context.iter().filter(|c| c.1 == FileKind::Recipe) {
                    if let Some(mut p) = recipe(rp, rt, &style) {
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
        FileKind::AnimSet | FileKind::Json => out.extend(json_syntax(path, text)),
        FileKind::Rust | FileKind::Cargo => {}
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
        let ctx = context("assets/recipes/tombstone.json", FileKind::Recipe, &files);
        assert_eq!(ctx.len(), 1);
        assert!(validate("assets/recipes/tombstone.json", FileKind::Recipe, &tomb, None, &ctx).is_empty());
        let broken = tomb.replace("\"graveyard_stone\"", "\"no_such_material\"");
        let p = validate("assets/recipes/tombstone.json", FileKind::Recipe, &broken, None, &ctx);
        assert!(p.iter().any(Problem::is_error), "{p:?}");
        // A style edit that breaks a recipe is an error; one that leaves them all loading isn't.
        let sctx = context("assets/style.json", FileKind::Style, &files);
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
        let uctx = context("assets/ui/hud.json", FileKind::UiScreen, &files);
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
    }
}
