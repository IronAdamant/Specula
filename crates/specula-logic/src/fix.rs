//! Applying rustc's machine-applicable suggestions, as `cargo fix` does, but only inside one
//! module's folder.

use crate::diag::{Diagnostic, Fix};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A fix that was written to disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    pub file: PathBuf,
    pub line: u32,
    pub column: u32,
    pub replacement: String,
    pub message: String,
}

/// Why fixes were not (all) applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixError {
    /// `read_failed`, `write_failed`.
    pub code: &'static str,
    pub file: PathBuf,
    pub message: String,
}

/// Applies every machine-applicable fix in `diagnostics` whose file is under `root` (others are
/// skipped: a module's build may warn about code it doesn't own). Overlapping fixes keep the
/// first; spans that don't fit the file as it is now (it changed since the build) are skipped.
/// Each file is rewritten once, back to front, so byte offsets stay valid.
pub fn apply(diagnostics: &[Diagnostic], root: &Path) -> Result<Vec<Applied>, FixError> {
    let read = &mut |file: &Path| std::fs::read_to_string(file).map_err(|e| FixError { code: "read_failed", file: file.to_owned(), message: e.to_string() });
    let mut applied = Vec::new();
    for p in plan(diagnostics, root, read)? {
        std::fs::write(&p.file, &p.text).map_err(|e| FixError { code: "write_failed", file: p.file.clone(), message: e.to_string() })?;
        applied.extend(p.applied);
    }
    Ok(applied)
}

/// One file's fixes, worked out but not written: its new text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Planned {
    /// Canonical path.
    pub file: PathBuf,
    pub text: String,
    pub applied: Vec<Applied>,
}

/// [`apply`] without writing anything: the fixed texts, from the texts `read` gives (the
/// engine passes the texts the build compiled, so offsets always match them).
pub fn plan(diagnostics: &[Diagnostic], root: &Path, read: &mut dyn FnMut(&Path) -> Result<String, FixError>) -> Result<Vec<Planned>, FixError> {
    let root = canonical(root);
    let mut by_file: BTreeMap<PathBuf, Vec<&Fix>> = BTreeMap::new();
    for fix in diagnostics.iter().flat_map(|d| &d.fixes).filter(|f| f.machine_applicable()) {
        let file = canonical(&fix.file);
        if file.starts_with(&root) {
            by_file.entry(file).or_default().push(fix);
        }
    }
    let mut planned = Vec::new();
    for (file, mut fixes) in by_file {
        let text = read(&file)?;
        fixes.sort_by_key(|f| (f.byte_start, f.byte_end));
        fixes.dedup_by(|a, b| a.byte_start == b.byte_start && a.byte_end == b.byte_end && a.replacement == b.replacement);
        // Keep non-overlapping ones, first come first served.
        let mut kept: Vec<&Fix> = Vec::new();
        for f in fixes {
            let fits = f.byte_start <= f.byte_end
                && f.byte_end <= text.len()
                && text.is_char_boundary(f.byte_start)
                && text.is_char_boundary(f.byte_end);
            let overlaps = kept.last().is_some_and(|k| f.byte_start < k.byte_end || (f.byte_start == k.byte_start && f.byte_start == f.byte_end));
            if fits && !overlaps {
                kept.push(f);
            }
        }
        if kept.is_empty() {
            continue;
        }
        let mut out = text.clone();
        for f in kept.iter().rev() {
            out.replace_range(f.byte_start..f.byte_end, &f.replacement);
        }
        let applied = kept.iter().map(|f| Applied { file: file.clone(), line: f.line, column: f.column, replacement: f.replacement.clone(), message: f.message.clone() }).collect();
        planned.push(Planned { file, text: out, applied });
    }
    Ok(planned)
}

/// Canonical when it exists (resolves `..`, links and case), else as given. On Windows the
/// `\\?\` prefix `canonicalize` adds is dropped again (Cargo rejects such paths in manifests,
/// and people don't write them).
pub fn canonical(p: &Path) -> PathBuf {
    let c = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_owned());
    let s = c.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(file: &Path, start: usize, end: usize, with: &str, how: &str) -> Fix {
        Fix {
            file: file.to_owned(),
            byte_start: start,
            byte_end: end,
            line: 1,
            column: start as u32 + 1,
            replacement: with.to_owned(),
            applicability: how.to_owned(),
            message: "m".into(),
        }
    }

    fn diag(fixes: Vec<Fix>) -> Diagnostic {
        Diagnostic {
            level: "warning".into(),
            code: None,
            message: String::new(),
            file: None,
            line: 0,
            column: 0,
            end_line: 0,
            end_column: 0,
            label: None,
            notes: vec![],
            fixes,
            rendered: None,
            package: None,
        }
    }

    #[test]
    fn applies_safe_fixes_back_to_front_and_only_inside_the_root() {
        let dir = std::env::temp_dir().join(format!("specula-fix-{}", std::process::id()));
        let inside = dir.join("m").join("src");
        std::fs::create_dir_all(&inside).unwrap();
        let file = inside.join("lib.rs");
        let outside = dir.join("other.rs");
        let src = "let mut x = 1; let mut y = 2; let é = 3;";
        std::fs::write(&file, src).unwrap();
        std::fs::write(&outside, src).unwrap();
        let d = diag(vec![
            fix(&file, 4, 8, "", "MachineApplicable"),
            fix(&file, 19, 23, "", "MachineApplicable"),
            // Same fix reported twice (once per child): applied once.
            fix(&file, 19, 23, "", "MachineApplicable"),
            // Overlaps the first: skipped.
            fix(&file, 6, 10, "zz", "MachineApplicable"),
            // Not safe: skipped.
            fix(&file, 0, 3, "const", "MaybeIncorrect"),
            // Splits a character / past the end: skipped.
            fix(&file, 35, 36, "e", "MachineApplicable"),
            fix(&file, 400, 401, "", "MachineApplicable"),
            fix(&outside, 4, 8, "", "MachineApplicable"),
        ]);
        let applied = apply(&[d], &dir.join("m")).unwrap();
        assert_eq!(applied.len(), 2);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "let x = 1; let y = 2; let é = 3;");
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), src, "outside the module: untouched");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
