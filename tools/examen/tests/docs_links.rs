//! The Markdown docs stay navigable: every relative link points at a file that exists (with the exact
//! case, so Windows and Linux agree), and the open owner decisions live in exactly one list (HANDOFF.md)
//! instead of drifting copies.

use std::path::{Component, Path, PathBuf};

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

/// The lines outside code fences (``` and ~~~), with code spans removed so `[a](b)` written as code
/// isn't read as a link. Each comes with its 1-based line number.
fn prose_lines(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut fence: Option<&str> = None;
    for (n, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let marker = ["```", "~~~"].into_iter().find(|m| trimmed.starts_with(m));
        match (fence, marker) {
            (None, Some(m)) => {
                fence = Some(m);
                continue;
            }
            (Some(f), Some(m)) if f == m => {
                fence = None;
                continue;
            }
            (Some(_), _) => continue,
            (None, None) => {}
        }
        let mut plain = String::with_capacity(line.len());
        let mut in_code = false;
        for c in line.chars() {
            if c == '`' {
                in_code = !in_code;
            } else if !in_code {
                plain.push(c);
            }
        }
        out.push((n + 1, plain));
    }
    out
}

/// The target of a link destination starting at `s` (just after `](`): `<...>` taken whole,
/// otherwise up to the parenthesis that closes it (nested pairs allowed) or a title.
fn destination(s: &str) -> Option<&str> {
    if let Some(rest) = s.strip_prefix('<') {
        return rest.find('>').map(|end| &rest[..end]);
    }
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(&s[..i]),
            ')' => depth -= 1,
            ' ' if depth == 0 => return Some(&s[..i]),
            _ => {}
        }
    }
    None
}

/// The targets of inline links (`[text](target)`, `![alt](target)`) and reference definitions
/// (`[label]: target`), with their line numbers.
fn link_targets(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (n, line) in prose_lines(text) {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            if let Some(i) = trimmed.find("]: ") {
                if let Some(target) = trimmed[i + 3..].split_whitespace().next() {
                    out.push((n, target.trim_matches(|c| c == '<' || c == '>').to_owned()));
                }
                continue;
            }
        }
        let mut rest = line.as_str();
        while let Some(i) = rest.find("](") {
            let after = &rest[i + 2..];
            let Some(target) = destination(after) else { break };
            out.push((n, target.to_owned()));
            rest = &after[target.len()..];
        }
    }
    out
}

/// Whether `rel` (relative to `base`) exists with exactly this case in every component.
fn exists_exact(base: &Path, rel: &Path) -> bool {
    let mut dir = base.to_path_buf();
    for part in rel.components() {
        match part {
            Component::Normal(name) => {
                let Ok(entries) = std::fs::read_dir(&dir) else { return false };
                if !entries.flatten().any(|e| e.file_name() == name) {
                    return false;
                }
                dir.push(name);
            }
            Component::ParentDir => {
                dir.pop();
            }
            Component::CurDir => {}
            _ => return false,
        }
    }
    true
}

fn decode(s: &str) -> String {
    s.replace("%20", " ")
}

#[test]
fn link_parser_handles_markdown_forms() {
    let text = "[a](x.md) ![i](img/p.png \"title\") `[c](code.md)` [n](foo_(bar).md)\n\
                [s](<with space.md>) [h](#anchor)\n```\n[f](fenced.md)\n```\n~~~\n[t](tilde.md)\n~~~\n\
                [ref]: ref.md\n";
    let got: Vec<String> = link_targets(text).into_iter().map(|(_, t)| t).collect();
    assert_eq!(got, ["x.md", "img/p.png", "foo_(bar).md", "with space.md", "#anchor", "ref.md"]);
}

#[test]
fn relative_markdown_links_resolve() {
    let root = repo();
    let mut files = Vec::new();
    markdown_files(&root, &mut files);
    assert!(files.len() > 10, "found only {} markdown files under {}", files.len(), root.display());
    let mut checked = 0;
    let mut broken = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        for (line, target) in link_targets(&text) {
            if target.is_empty() || target.starts_with('#') || target.contains("://") || target.starts_with("mailto:") {
                continue;
            }
            checked += 1;
            let path = decode(target.split('#').next().unwrap());
            let ok = match path.strip_prefix('/') {
                Some(abs) => exists_exact(&root, Path::new(abs)),
                None => exists_exact(file.parent().unwrap(), Path::new(&path)),
            };
            if !ok {
                let shown = file.strip_prefix(&root).unwrap_or(file).display().to_string();
                broken.push(format!("{shown}:{line}: {target}"));
            }
        }
    }
    assert!(checked > 50, "only {checked} relative links found: is the link parser broken?");
    assert!(broken.is_empty(), "broken relative links (paths are case-sensitive):\n{}", broken.join("\n"));
}

/// Whether a heading or a bold lead-in line names a list of owner questions or decisions.
fn names_owner_list(line: &str) -> bool {
    let t = line.trim_start();
    if !(t.starts_with('#') || t.starts_with("**")) {
        return false;
    }
    let lower = t.to_ascii_lowercase();
    ["owner decision", "for the owner", "still open", "owner question"].iter().any(|k| lower.contains(k))
}

#[test]
fn open_owner_decisions_live_in_one_list() {
    let root = repo();
    let mut files = Vec::new();
    markdown_files(&root, &mut files);
    let mut lists = Vec::new();
    for file in &files {
        let shown = file.strip_prefix(&root).unwrap_or(file).display().to_string().replace('\\', "/");
        let text = std::fs::read_to_string(file).unwrap();
        let lines = prose_lines(&text);
        for (i, (_, line)) in lines.iter().enumerate() {
            if !names_owner_list(line) {
                continue;
            }
            // Its section runs to the next heading. A section that only points at the one list is fine.
            let section: String = lines[i + 1..]
                .iter()
                .map(|(_, l)| l.as_str())
                .take_while(|l| !l.trim_start().starts_with('#'))
                .collect::<Vec<_>>()
                .join("\n");
            let points_home = section.contains("HANDOFF.md#open-owner-decisions") || line.contains("HANDOFF.md#open-owner-decisions");
            if !points_home {
                lists.push(format!("{shown}: {line}"));
            }
        }
    }
    assert_eq!(
        lists,
        ["HANDOFF.md: ## Open owner decisions"],
        "open owner decisions must be one list, in HANDOFF.md (other docs link to HANDOFF.md#open-owner-decisions)"
    );
}
