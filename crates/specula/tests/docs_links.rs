//! The Markdown docs stay navigable: every relative link points at a file that exists, and the open owner
//! decisions live in exactly one list (HANDOFF.md) instead of drifting copies.

use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every `*.md` under the repository, skipping build output, local state, dot-directories
/// (`.git`, `.claude/worktrees`) and `docs/archive` (frozen history that links to the old layout).
fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            let skip = name.starts_with('.')
                || ["target", "captures", "worlds", "tmp", "__pycache__"].contains(&name.as_str())
                || path.ends_with("docs/archive");
            if !skip {
                markdown_files(&path, out);
            }
        } else if name.ends_with(".md") {
            out.push(path);
        }
    }
}

/// The targets of inline links (`[text](target)` and `![alt](target)`) outside code fences and
/// code spans, with their line numbers.
fn link_targets(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut fenced = false;
    for (n, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        // Drop code spans so `[a](b)` written as code isn't read as a link.
        let mut plain = String::with_capacity(line.len());
        let mut in_code = false;
        for c in line.chars() {
            if c == '`' {
                in_code = !in_code;
            } else if !in_code {
                plain.push(c);
            }
        }
        let mut rest = plain.as_str();
        while let Some(i) = rest.find("](") {
            let after = &rest[i + 2..];
            let Some(end) = after.find(')') else { break };
            // `[text](target "title")`: the target is the first word.
            let target = after[..end].split_whitespace().next().unwrap_or("").trim_matches(|c| c == '<' || c == '>');
            out.push((n + 1, target.to_owned()));
            rest = &after[end..];
        }
    }
    out
}

fn decode(s: &str) -> String {
    s.replace("%20", " ")
}

#[test]
fn relative_markdown_links_resolve() {
    let root = repo();
    let mut files = Vec::new();
    markdown_files(&root, &mut files);
    assert!(files.len() > 10, "found only {} markdown files under {}", files.len(), root.display());
    let mut broken = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        for (line, target) in link_targets(&text) {
            if target.is_empty() || target.starts_with('#') || target.contains("://") || target.starts_with("mailto:") {
                continue;
            }
            let path = decode(target.split('#').next().unwrap());
            let resolved = if let Some(abs) = path.strip_prefix('/') { root.join(abs) } else { file.parent().unwrap().join(&path) };
            if !resolved.exists() {
                let shown = file.strip_prefix(&root).unwrap_or(file).display().to_string();
                broken.push(format!("{shown}:{line}: {target}"));
            }
        }
    }
    assert!(broken.is_empty(), "broken relative links:\n{}", broken.join("\n"));
}

#[test]
fn open_owner_decisions_live_in_one_list() {
    let root = repo();
    let mut files = Vec::new();
    markdown_files(&root, &mut files);
    let mut lists = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        for line in text.lines() {
            let heading = line.trim_start_matches('#');
            let lower = heading.trim().to_ascii_lowercase();
            if heading.len() < line.len() && (lower.contains("owner decision") || lower.contains("questions for the owner")) {
                lists.push(format!("{}: {line}", file.strip_prefix(&root).unwrap_or(file).display()));
            }
        }
    }
    let shown: Vec<_> = lists.iter().map(|l| l.replace('\\', "/")).collect();
    assert_eq!(shown, ["HANDOFF.md: ## Open owner decisions"], "open owner decisions must be one list, in HANDOFF.md");
}
