//! The authority guard (`docs/design/AUTHORITY.md`): what the compiler can't see.
//!
//! - **Role literals** (`"lead"`, `"director"`, `"reviewer"`, and `"owner"` compared or listed as
//!   a role) appear only in `authority.rs`: authority is answered in one place, never by a
//!   string compare somewhere else.
//! - **Reply channels:** a `Sender` of a tool answer (any spelling, inside a `Result`, through a
//!   type alias or a renamed import) appears only in `jobs.rs`: every answer is a `jobs::Reply`,
//!   which only `jobs::channel` makes, so every deferred job goes through the jobs module.
//!
//! A text scan of the engine's sources: comments are dropped, and code after a
//! `#[cfg(test)]` module (and files named `tests.rs`) is skipped.

use std::path::{Path, PathBuf};

fn sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).expect("src dir").flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|e| e == "rs") && p.file_name().is_some_and(|n| n != "tests.rs") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
    out
}

/// The file's code lines without `//` comments, up to its first `#[cfg(test)]` module.
fn code_lines(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t.starts_with("#[cfg(test)]") && lines.get(i + 1).is_some_and(|n| n.trim_start().starts_with("mod ") || n.trim_start().starts_with("pub mod ")) {
            break;
        }
        // Drop a `//` comment that isn't inside a string literal (good enough: count quotes before it).
        let mut code = String::new();
        let mut in_str = false;
        let mut prev = ' ';
        let chars: Vec<char> = line.chars().collect();
        let mut k = 0;
        while k < chars.len() {
            let c = chars[k];
            if c == '"' && prev != '\\' {
                in_str = !in_str;
            }
            if !in_str && c == '/' && chars.get(k + 1) == Some(&'/') {
                break;
            }
            code.push(c);
            prev = if prev == '\\' && c == '\\' { ' ' } else { c };
            k += 1;
        }
        out.push((i + 1, code));
    }
    out
}

fn rel(p: &Path) -> String {
    p.strip_prefix(env!("CARGO_MANIFEST_DIR")).unwrap_or(p).display().to_string().replace('\\', "/")
}

/// Role literals in `text` that aren't JSON keys. `"lead"`, `"director"` and `"reviewer"` are
/// never JSON keys here, so any is flagged. `"owner"` is a JSON key in many answers: it passes
/// only as one, `("owner", ..)` (a tuple's first item), `.get("owner")`, `.with("owner", ..)`,
/// `.set("owner", ..)`, `.remove("owner")` or `k != "owner"` (a key compared in a retain), and
/// as the value of a `const ..._KEY` / `..._FIELD`. Anything else (`== "owner"`,
/// `Some("owner")`, `eq_ignore_ascii_case("owner")`, `["artist", "owner"]`) is a role compare.
fn role_literals(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for lit in ["\"lead\"", "\"director\"", "\"reviewer\""] {
        if text.contains(lit) {
            found.push(lit.to_owned());
        }
    }
    let owner = "\"owner\"";
    let mut from = 0;
    while let Some(at) = text[from..].find(owner) {
        let i = from + at;
        let before: String = text[..i].chars().filter(|c| !c.is_whitespace()).collect();
        let after: String = text[i + owner.len()..].chars().take(16).filter(|c| !c.is_whitespace()).collect();
        let key_call = [".get(", ".with(", ".set(", ".remove(", ".detail(", "k!=", "(args,"].iter().any(|k| before.ends_with(k));
        // A tuple's first item: `(` not preceded by an identifier or `!` (a call or a macro).
        let tuple = before.ends_with('(') && !before[..before.len() - 1].ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == '!');
        let tuple = tuple && (after.starts_with(',') || after.starts_with(".to_owned(),"));
        // A tool's list of argument keys (`check_keys(args, name, &[&["owner", ...]])`).
        let statement = before.rsplit([';', '{', '}']).next().unwrap_or("");
        let arg_keys = statement.contains("check_keys(");
        let key_const = before.ends_with('=') && {
            let decl = before.rsplit("const").next().unwrap_or("");
            before.contains("const") && (decl.contains("_KEY:") || decl.contains("_FIELD:"))
        };
        if !(key_call || tuple || key_const || arg_keys) {
            let start = text[..i].char_indices().rev().nth(30).map_or(0, |(k, _)| k);
            found.push(format!("\"owner\" not as a JSON key: ...{}", text[start..(i + owner.len()).min(text.len())].trim()));
        }
        from = i + owner.len();
    }
    found
}

#[test]
fn role_literals_appear_only_in_authority_rs() {
    let mut bad = Vec::new();
    for p in sources() {
        if p.file_name().is_some_and(|n| n == "authority.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap();
        // Whole statements: a compare split over two lines is one compare.
        let joined: String = code_lines(&text).into_iter().map(|(_, l)| l).collect::<Vec<_>>().join(" ");
        for lit in role_literals(&joined) {
            bad.push(format!("{}: {lit}", rel(&p)));
        }
    }
    assert!(bad.is_empty(), "authority roles are answered by authority::authority / Authority::of_role, never by a string compare:\n{}", bad.join("\n"));
}

/// A `Sender` of a tool answer, spelled any way: `Sender<ToolOutput>`, `mpsc::Sender<Result<_,
/// ToolOutput>>`, a `type` alias of a `Sender`, or `Sender` imported under another name.
fn holds_reply_sender(line: &str) -> Option<&'static str> {
    let squeezed: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    // What each `Sender<..>` carries (to its matching `>`).
    let mut from = 0;
    while let Some(at) = squeezed[from..].find("Sender<") {
        let start = from + at + "Sender<".len();
        let mut depth = 1;
        let mut end = squeezed.len();
        for (i, c) in squeezed[start..].char_indices() {
            match c {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        end = start + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        if squeezed[start..end].contains("ToolOutput") {
            return Some("a Sender of a tool answer");
        }
        from = start;
    }
    let t = line.trim_start();
    let alias = ["type ", "pub type ", "pub(crate) type "].iter().any(|k| t.starts_with(k));
    // A generic alias of a Sender (`type Tx<T> = Sender<T>`) would hide what it carries.
    let generic_alias = alias && squeezed.split('=').next().is_some_and(|lhs| lhs.contains('<'));
    if generic_alias && squeezed.contains("Sender<") {
        return Some("a generic type alias of a Sender (hides what it carries)");
    }
    if squeezed.contains("Senderas") && (t.starts_with("use ") || t.starts_with("pub use ")) {
        return Some("Sender imported under another name");
    }
    None
}

#[test]
fn reply_channels_live_only_in_jobs() {
    let mut bad = Vec::new();
    for p in sources() {
        let name = rel(&p);
        if name == "src/jobs.rs" {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap();
        let lines = code_lines(&text);
        for (n, line) in &lines {
            if let Some(why) = holds_reply_sender(line) {
                bad.push(format!("{name}:{n}: {why}: {}", line.trim()));
            }
        }
        // A `Sender<` and its `ToolOutput>` split over lines: the whole file as one statement.
        let joined: String = lines.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>().join(" ");
        if bad.iter().all(|b| !b.starts_with(&format!("{name}:"))) {
            if let Some(why @ "a Sender of a tool answer") = holds_reply_sender(&joined) {
                bad.push(format!("{name}: {why} (split over lines)"));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "a tool's answer goes through a jobs::Reply (made only by jobs::channel): a Sender of one elsewhere skips the jobs module's owner and busy rules:\n{}",
        bad.join("\n")
    );
}

#[test]
fn the_scanners_see_what_they_should() {
    assert_eq!(role_literals(r#"matches!(self.swarm.role(&s.id), "lead" | "director")"#).len(), 2);
    assert_eq!(role_literals(r#"if role == "owner" {"#).len(), 1);
    assert_eq!(role_literals(r#"pub const R: [&str; 2] = ["owner", "x"];"#).len(), 1);
    // Review round 2: these passed.
    assert_eq!(role_literals(r#"if role != Some("owner") {"#).len(), 1);
    assert_eq!(role_literals(r#"if role.eq_ignore_ascii_case("owner") {"#).len(), 1);
    assert_eq!(role_literals(r#"let ok = ["artist", "owner"].contains(&r);"#).len(), 1);
    assert_eq!(role_literals(r#"if role == "#).len() + role_literals(r#"if role ==  "owner" {"#).len(), 1, "split over lines: joined first");
    assert!(role_literals(r#"f.push(("owner", Json::from(o)));"#).is_empty());
    assert!(role_literals(r#"e.with("owner", c.owner.as_str())"#).is_empty());
    assert!(role_literals(r#"h.get("owner").and_then(Json::as_str)"#).is_empty());
    assert!(role_literals(r#"o.retain(|(k, _)| k != "owner");"#).is_empty(), "a key in a retain");
    assert!(role_literals(r#"const OWNER_KEY: &str = "owner";"#).is_empty());
    assert!(role_literals(r#"check_keys(args, name, &[&["owner", "mine"]])?;"#).is_empty(), "a tool's argument keys");
    assert!(role_literals(r#"owner: agent_arg(args, "owner")?,"#).is_empty(), "an argument read");
    assert!(role_literals(r#"let o = e.detail("owner");"#).is_empty());
    assert!(role_literals(r#"vec![("owner".to_owned(), Json::from(o))]"#).is_empty());
    assert!(holds_reply_sender("    reply: std::sync::mpsc::Sender< ToolOutput >,").is_some());
    assert!(holds_reply_sender("    Tool(mpsc::Sender<specula_protocol::ToolOutput>),").is_some());
    assert!(holds_reply_sender("    reply: mpsc::Sender<Result<Rendered, ToolOutput>>,").is_some(), "a tool answer inside a Result");
    assert!(holds_reply_sender("fn f(reply: Sender<ToolOutput>) {").is_some(), "parameters too");
    assert!(holds_reply_sender("type Answer<T> = mpsc::Sender<T>;").is_some(), "a generic alias");
    assert!(holds_reply_sender("type Answer = mpsc::Sender<ToolOutput>;").is_some(), "an alias");
    assert!(holds_reply_sender("type Loader = (Sender<Job>, Receiver<Done>);").is_none(), "an alias of another channel");
    assert!(holds_reply_sender("pub fn build(to_engine: &Sender<Request>, a: &Json) -> ToolOutput {").is_none(), "a Sender of requests");
    assert!(holds_reply_sender("use std::sync::mpsc::Sender as Tx;").is_some(), "a renamed import");
    assert!(holds_reply_sender("    reply: crate::jobs::Reply,").is_none());
    assert!(holds_reply_sender("    reply: Sender<Result<Vec<ModuleText>, Json>>,").is_none(), "not a tool answer");
    // Review round 2: split over two lines (the test joins a file's lines first).
    assert!(holds_reply_sender("reply: std::sync::mpsc::Sender<
        ToolOutput,
    >,").is_some());
    let lines = code_lines("let a = 1; // \"lead\"\n#[cfg(test)]\nmod tests {\n \"lead\"\n}");
    assert_eq!(lines.len(), 1);
    assert!(!lines[0].1.contains("lead"));
}
