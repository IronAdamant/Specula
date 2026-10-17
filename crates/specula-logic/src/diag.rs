//! Cargo's `--message-format=json` output as data: diagnostics with exact spans and rustc's
//! suggested fixes, and the built artifacts.

use specula_core::Json;
use std::path::{Path, PathBuf};

/// One compiler message.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    /// `error`, `warning`, `note`, `help`, `failure-note`, `error: internal compiler error`.
    pub level: String,
    /// `E0308`, `unused_mut`... (`None` for messages without one).
    pub code: Option<String>,
    pub message: String,
    /// The primary span's file (absolute), line and column (1-based), if any.
    pub file: Option<PathBuf>,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    /// The primary span's label (`expected f32, found u32`).
    pub label: Option<String>,
    /// Child messages (`help: ...`, `note: ...`), without their spans.
    pub notes: Vec<String>,
    /// Every suggestion rustc made, with how safe it is to apply.
    pub fixes: Vec<Fix>,
    /// rustc's human rendering (what a terminal would show).
    pub rendered: Option<String>,
    /// The crate it is about when that isn't the module itself (an error in a dependency, e.g.
    /// the SDK failing to build); `None` for the module's own messages.
    pub package: Option<String>,
}

/// One suggested replacement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    pub file: PathBuf,
    pub byte_start: usize,
    pub byte_end: usize,
    pub line: u32,
    pub column: u32,
    pub replacement: String,
    /// `MachineApplicable` (safe to apply blind), `MaybeIncorrect`, `HasPlaceholders`,
    /// `Unspecified`.
    pub applicability: String,
    /// The suggestion's own message (`remove this mut`).
    pub message: String,
}

impl Fix {
    pub fn machine_applicable(&self) -> bool {
        self.applicability == "MachineApplicable"
    }
}

/// One line of Cargo's JSON output, as far as a build cares.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// A compiler message for the package whose manifest is `manifest` (its target `target`).
    Diagnostic { manifest: PathBuf, target: String, diagnostic: Diagnostic },
    /// A built target: its package name, crate kinds and files.
    Artifact { manifest: PathBuf, package_id: String, name: String, kinds: Vec<String>, files: Vec<PathBuf>, fresh: bool },
    /// A build script ran (for the package `package_id`).
    BuildScript { package_id: String },
    /// The build ended.
    Finished { success: bool },
    Other,
}

/// Parses one line; `root` resolves the relative file names rustc reports (the workspace root).
pub fn parse_line(line: &str, root: &Path) -> Message {
    let Ok(j) = Json::parse(line) else { return Message::Other };
    let s = |j: &Json, k: &str| j.get(k).and_then(Json::as_str).map(str::to_owned);
    let manifest = PathBuf::from(s(&j, "manifest_path").unwrap_or_default());
    match j.get("reason").and_then(Json::as_str) {
        Some("compiler-message") => match j.get("message") {
            Some(m) => {
                let target = j.get("target").and_then(|t| s(t, "name")).unwrap_or_default();
                Message::Diagnostic { manifest, target, diagnostic: diagnostic(m, root) }
            }
            None => Message::Other,
        },
        Some("compiler-artifact") => {
            let target = j.get("target");
            let name = target.and_then(|t| s(t, "name")).unwrap_or_default();
            let strings = |v: Option<&Json>| -> Vec<String> {
                v.and_then(Json::as_array).unwrap_or_default().iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()
            };
            let kinds = strings(target.and_then(|t| t.get("crate_types")));
            let files = strings(j.get("filenames")).into_iter().map(PathBuf::from).collect();
            let fresh = j.get("fresh").and_then(Json::as_bool).unwrap_or(false);
            let package_id = s(&j, "package_id").unwrap_or_default();
            Message::Artifact { manifest, package_id, name, kinds, files, fresh }
        }
        Some("build-script-executed") => Message::BuildScript { package_id: s(&j, "package_id").unwrap_or_default() },
        Some("build-finished") => Message::Finished { success: j.get("success").and_then(Json::as_bool).unwrap_or(false) },
        _ => Message::Other,
    }
}

fn diagnostic(m: &Json, root: &Path) -> Diagnostic {
    let s = |j: &Json, k: &str| j.get(k).and_then(Json::as_str).map(str::to_owned);
    let n = |j: &Json, k: &str| j.get(k).and_then(Json::as_u64).unwrap_or(0) as u32;
    let spans = m.get("spans").and_then(Json::as_array).unwrap_or_default();
    let primary = spans.iter().find(|sp| sp.get("is_primary").and_then(Json::as_bool).unwrap_or(false)).or(spans.first());
    let mut fixes = Vec::new();
    collect_fixes(m, root, &mut fixes);
    let children = m.get("children").and_then(Json::as_array).unwrap_or_default();
    for c in children {
        collect_fixes(c, root, &mut fixes);
    }
    let notes = children
        .iter()
        .filter_map(|c| Some(format!("{}: {}", s(c, "level")?, s(c, "message")?)))
        .filter(|t| !t.ends_with(": "))
        .collect();
    Diagnostic {
        level: s(m, "level").unwrap_or_default(),
        code: m.get("code").and_then(|c| s(c, "code")),
        message: s(m, "message").unwrap_or_default(),
        file: primary.and_then(|p| s(p, "file_name")).map(|f| resolve(root, &f)),
        line: primary.map_or(0, |p| n(p, "line_start")),
        column: primary.map_or(0, |p| n(p, "column_start")),
        end_line: primary.map_or(0, |p| n(p, "line_end")),
        end_column: primary.map_or(0, |p| n(p, "column_end")),
        label: primary.and_then(|p| s(p, "label")),
        notes,
        fixes,
        rendered: s(m, "rendered"),
        package: None,
    }
}

/// The suggestions in one message's spans (each span with a `suggested_replacement`).
fn collect_fixes(m: &Json, root: &Path, out: &mut Vec<Fix>) {
    let message = m.get("message").and_then(Json::as_str).unwrap_or_default();
    for sp in m.get("spans").and_then(Json::as_array).unwrap_or_default() {
        let Some(replacement) = sp.get("suggested_replacement").and_then(Json::as_str) else { continue };
        let (Some(file), Some(start), Some(end)) = (
            sp.get("file_name").and_then(Json::as_str),
            sp.get("byte_start").and_then(Json::as_u64),
            sp.get("byte_end").and_then(Json::as_u64),
        ) else {
            continue;
        };
        out.push(Fix {
            file: resolve(root, file),
            byte_start: start as usize,
            byte_end: end as usize,
            line: sp.get("line_start").and_then(Json::as_u64).unwrap_or(0) as u32,
            column: sp.get("column_start").and_then(Json::as_u64).unwrap_or(0) as u32,
            replacement: replacement.to_owned(),
            applicability: sp.get("suggestion_applicability").and_then(Json::as_str).unwrap_or("Unspecified").to_owned(),
            message: message.to_owned(),
        });
    }
}

fn resolve(root: &Path, file: &str) -> PathBuf {
    let p = Path::new(file);
    if p.is_absolute() {
        p.to_owned()
    } else {
        root.join(p)
    }
}

impl Diagnostic {
    pub fn is_error(&self) -> bool {
        self.level.starts_with("error")
    }

    /// For agents: paths relative to `base` when inside it; `rendered` only when `detailed`.
    pub fn to_json(&self, base: &Path, detailed: bool) -> Json {
        let rel = |p: &Path| -> String {
            let shown = p.strip_prefix(base).unwrap_or(p);
            shown.to_string_lossy().replace('\\', "/")
        };
        let mut f = vec![("level", Json::from(self.level.as_str())), ("message", Json::from(self.message.as_str()))];
        if let Some(p) = &self.package {
            f.push(("package", Json::from(p.as_str())));
        }
        if let Some(c) = &self.code {
            f.push(("code", Json::from(c.as_str())));
        }
        if let Some(file) = &self.file {
            f.push(("file", Json::from(rel(file))));
            f.push(("line", Json::from(self.line)));
            f.push(("column", Json::from(self.column)));
            f.push(("end_line", Json::from(self.end_line)));
            f.push(("end_column", Json::from(self.end_column)));
        }
        if let Some(l) = &self.label {
            f.push(("label", Json::from(l.as_str())));
        }
        if !self.notes.is_empty() {
            f.push(("notes", Json::arr(self.notes.iter().map(|n| Json::from(n.as_str())))));
        }
        if !self.fixes.is_empty() {
            let fixes = self.fixes.iter().map(|x| {
                Json::obj([
                    ("file", Json::from(rel(&x.file))),
                    ("line", Json::from(x.line)),
                    ("column", Json::from(x.column)),
                    ("replacement", Json::from(x.replacement.as_str())),
                    ("applicability", Json::from(x.applicability.as_str())),
                    ("message", Json::from(x.message.as_str())),
                ])
            });
            f.push(("fixes", Json::arr(fixes)));
        }
        if detailed {
            if let Some(r) = &self.rendered {
                f.push(("rendered", Json::from(r.as_str())));
            }
        }
        Json::obj(f)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A real `unused_mut` warning from rustc 1.99 (`cargo build --message-format=json`).
    pub(crate) const UNUSED_MUT: &str = r#"{"reason":"compiler-message","package_id":"path+file:///D:/w/m#0.1.0","manifest_path":"D:\\w\\m\\Cargo.toml","target":{"kind":["cdylib"],"crate_types":["cdylib"],"name":"m","src_path":"D:\\w\\m\\src\\lib.rs","edition":"2021","doctest":false,"test":true},"message":{"rendered":"warning: variable does not need to be mutable\n --> src\\lib.rs:3:9\n","$message_type":"diagnostic","children":[{"children":[],"code":null,"level":"note","message":"`#[warn(unused_mut)]` (part of `#[warn(unused)]`) on by default","rendered":null,"spans":[]},{"children":[],"code":null,"level":"help","message":"remove this `mut`","rendered":null,"spans":[{"byte_end":34,"byte_start":30,"column_end":13,"column_start":9,"expansion":null,"file_name":"src\\lib.rs","is_primary":true,"label":null,"line_end":3,"line_start":3,"suggested_replacement":"","suggestion_applicability":"MachineApplicable","text":[{"highlight_end":13,"highlight_start":9,"text":"    let mut x = 1;"}]}]}],"code":{"code":"unused_mut","explanation":null},"level":"warning","message":"variable does not need to be mutable","spans":[{"byte_end":35,"byte_start":30,"column_end":14,"column_start":9,"expansion":null,"file_name":"src\\lib.rs","is_primary":true,"label":null,"line_end":3,"line_start":3,"suggested_replacement":null,"suggestion_applicability":null,"text":[{"highlight_end":14,"highlight_start":9,"text":"    let mut x = 1;"}]}]}}"#;

    #[test]
    fn parses_a_warning_with_a_machine_applicable_fix() {
        let root = Path::new("D:/w/m");
        let Message::Diagnostic { diagnostic: d, manifest, target } = parse_line(UNUSED_MUT, root) else { panic!("not a diagnostic") };
        assert_eq!(target, "m");
        assert_eq!(manifest, PathBuf::from("D:\\w\\m\\Cargo.toml"));
        assert_eq!((d.level.as_str(), d.code.as_deref()), ("warning", Some("unused_mut")));
        assert_eq!((d.line, d.column, d.end_column), (3, 9, 14));
        assert_eq!(d.file, Some(root.join("src\\lib.rs")));
        assert_eq!(d.fixes.len(), 1);
        let f = &d.fixes[0];
        assert!(f.machine_applicable());
        assert_eq!((f.byte_start, f.byte_end, f.replacement.as_str()), (30, 34, ""));
        assert_eq!(d.notes.len(), 2);
        let j = d.to_json(root, false);
        assert_eq!(j.get("file").and_then(Json::as_str), Some("src/lib.rs"));
        assert!(j.get("rendered").is_none());
        assert!(d.to_json(root, true).get("rendered").is_some());
    }

    #[test]
    fn parses_artifacts_and_the_end() {
        let line = r#"{"reason":"compiler-artifact","package_id":"x","manifest_path":"D:\\w\\m\\Cargo.toml","target":{"kind":["cdylib"],"crate_types":["cdylib"],"name":"ambulo","src_path":"x"},"profile":{},"features":[],"filenames":["D:\\w\\target\\debug\\ambulo.dll","D:\\w\\target\\debug\\libambulo.dll.a"],"executable":null,"fresh":false}"#;
        match parse_line(line, Path::new(".")) {
            Message::Artifact { name, kinds, files, fresh, package_id, .. } => {
                assert_eq!(package_id, "x");
                assert_eq!(name, "ambulo");
                assert_eq!(kinds, vec!["cdylib".to_owned()]);
                assert_eq!(files.len(), 2);
                assert!(!fresh);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(parse_line(r#"{"reason":"build-finished","success":false}"#, Path::new(".")), Message::Finished { success: false });
        assert_eq!(parse_line("not json", Path::new(".")), Message::Other);
        assert_eq!(
            parse_line(r#"{"reason":"build-script-executed","package_id":"p"}"#, Path::new(".")),
            Message::BuildScript { package_id: "p".into() }
        );
        assert_eq!(parse_line(r#"{"reason":"compiler-moonbeam"}"#, Path::new(".")), Message::Other);
    }
}
