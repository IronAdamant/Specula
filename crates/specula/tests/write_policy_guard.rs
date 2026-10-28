//! The write policy's guard (docs/design/WRITE_POLICY.md): the compiler keeps `&mut World` of the
//! engine's live world inside `write_policy` (it is a `Live`, with no `DerefMut`); this test
//! catches what the compiler can't see, so a regression fails even if someone loosens visibility
//! later:
//!
//! 1. **Escape hatches** (taking or hiding recorded changes, switching recording, raw puts that
//!    skip a write's validation, journal appends, rollbacks without a note) appear only in the files
//!    that own them, each with its reason.
//! 2. **Exempt writers and reserved rules** ([`Exempt`], [`Rule`], [`Opener`] variants, the module
//!    commits, disk imports) are used only by the code they belong to: a new path can't pick an
//!    exemption that isn't its own.
//! 3. **Swarm's write methods** (for worlds a caller holds: tests, detached worlds) aren't used
//!    by engine code.
//! 4. **No other live world:** no struct outside the allowed files has a field of type `World`,
//!    `Live` stays read-only (no `DerefMut`, a private field) and nothing hands out `&mut World`.
//!
//! Test code (`#[cfg(test)] mod tests` blocks and `tests.rs` files) is skipped: tests build their
//! own detached worlds.

use std::path::{Path, PathBuf};

/// A pattern and where it may appear (file paths relative to `crates/specula/src`, `/`-separated),
/// with why.
struct Rule {
    pattern: &'static str,
    allowed: &'static [&'static str],
    why: &'static str,
}

const HATCHES: &[Rule] = &[
    Rule {
        pattern: ".take_changes()",
        allowed: &["world.rs", "journal.rs", "write_policy.rs", "lex_live.rs"],
        why: "taking changes drops them from the journal and the checks: only the policy and the journal take them (lex_live: remove_presentation, on a loaded world before it goes live)",
    },
    Rule {
        pattern: "Recording::Off",
        allowed: &["world.rs", "journal.rs", "desync.rs"],
        why: "a world that stops recording hides its changes from the policy: only replays of detached copies (journal, desync) record nothing",
    },
    Rule {
        pattern: ".set_recording(",
        allowed: &["world.rs", "journal.rs", "write_policy.rs", "desync.rs", "lex_live.rs", "logic/mod.rs"],
        why: "the policy sets how the live world records; inside a session, live Lex's presentation records one change per entity (Snapshot) and module calls record full changes for their rollbacks, both journaled through the policy",
    },
    Rule {
        pattern: ".rollback_to(",
        allowed: &["world.rs", "journal.rs", "write_policy.rs", "logic/mod.rs"],
        why: "a rollback must be followed by the journal noting the ids it used (write_policy::rolled_back); the module host's call transactions note them through their commit",
    },
    Rule {
        pattern: ".recheck_steps(",
        allowed: &["world.rs", "journal.rs", "write_policy.rs"],
        why: "the step-by-step claim check is the policy's",
    },
    Rule {
        pattern: ".retain_retired(",
        allowed: &["world.rs", "journal.rs", "write_policy.rs"],
        why: "pruning removed versions is the policy's (prune_retired)",
    },
    Rule {
        pattern: ".adopt_files(",
        allowed: &["world.rs", "write_policy.rs"],
        why: "replacing the world is Live::replace",
    },
    Rule {
        pattern: ".take_files(",
        allowed: &["world.rs", "write_policy.rs"],
        why: "replacing the world is Live::replace",
    },
    Rule {
        pattern: ".put_raw(",
        allowed: &["world.rs", "journal.rs"],
        why: "raw puts skip a write's validation: the journal's undo and replay only",
    },
    Rule {
        pattern: ".put_file_raw(",
        allowed: &["world.rs", "journal.rs", "project_engine.rs"],
        why: "project files are written by the project tools and disk imports (Rule::Project, Live::disk); other writers hold the world while its files are lent out",
    },
    Rule {
        pattern: ".remove_raw(",
        allowed: &["world.rs", "journal.rs"],
        why: "raw removes skip a write's validation: the journal's undo and replay only",
    },
    Rule {
        pattern: ".replace_env(",
        allowed: &["world.rs", "journal.rs"],
        why: "raw environment puts: the journal's undo and replay only",
    },
    Rule {
        pattern: ".restore_camera(",
        allowed: &["world.rs", "journal.rs", "persist.rs"],
        why: "raw camera puts: undo, and reading a detached world from its files",
    },
    Rule {
        pattern: ".put_camera_raw(",
        allowed: &["world.rs"],
        why: "raw camera puts",
    },
    Rule {
        pattern: ".apply_after(",
        allowed: &["world.rs", "journal.rs", "desync.rs"],
        why: "replaying recorded states: detached replays only",
    },
    Rule {
        pattern: ".apply_pose(",
        allowed: &["world.rs"],
        why: "replaying poses",
    },
    Rule {
        pattern: ".replay_next_id(",
        allowed: &["world.rs", "journal.rs", "desync.rs"],
        why: "replaying the id counter: detached replays only",
    },
    Rule {
        pattern: ".set_next_id(",
        allowed: &["world.rs", "persist.rs", "write_policy.rs"],
        why: "the id counter of a detached world read from its files, or of a world the policy recovered (never below ids already handed out)",
    },
    Rule {
        pattern: "journal::commit(",
        allowed: &["journal.rs", "write_policy.rs"],
        why: "journaling a command is the policy's (commit, disk)",
    },
    Rule {
        pattern: "journal.append(",
        allowed: &["journal.rs"],
        why: "journal appends go through the policy",
    },
    Rule {
        pattern: ".append_game(",
        allowed: &["journal.rs", "write_policy.rs"],
        why: "journal appends go through the policy",
    },
    Rule {
        pattern: ".append_snapshot(",
        allowed: &["journal.rs", "write_policy.rs"],
        why: "journal appends go through the policy",
    },
    Rule {
        pattern: ".append_module(",
        allowed: &["journal.rs", "write_policy.rs"],
        why: "module events are journaled through write_policy::module_event",
    },
    Rule {
        pattern: "journal.undo(",
        allowed: &["journal.rs", "write_policy.rs"],
        why: "undo goes through the policy (claims and joint rules per step)",
    },
    Rule {
        pattern: ".rebase(",
        allowed: &["journal.rs", "write_policy.rs"],
        why: "the journal starts over only when Live::replace swaps the world",
    },
    Rule {
        pattern: ".note_next_id(",
        allowed: &["journal.rs", "write_policy.rs"],
        why: "ids used by rolled-back changes are noted by the policy (rolled_back, game_commit)",
    },
];

const OWNERS: &[Rule] = &[
    Rule { pattern: "Exempt::Game {", allowed: &["sim_writes.rs", "write_policy.rs"], why: "the hosted game's own calls go through sim_writes::game (Engine::game_call, frames, overlays, the game tool's repair)" },
    Rule { pattern: "Exempt::GameStart", allowed: &["sim_writes.rs", "write_policy.rs"], why: "the hosted game's start" },
    Rule { pattern: "Exempt::Module", allowed: &["write_policy.rs"], why: "module ticks commit through write_policy::module_tick_commit" },
    Rule { pattern: "Exempt::LexLive", allowed: &["lex_live.rs", "write_policy.rs"], why: "live Lex runs' own ticks and input" },
    Rule { pattern: "Exempt::Physics", allowed: &["physics.rs", "write_policy.rs"], why: "the physics tick" },
    Rule { pattern: "Exempt::Nav", allowed: &["nav_tools.rs", "write_policy.rs"], why: "the navigation tick" },
    Rule { pattern: "Exempt::AnimReload", allowed: &["sim_writes.rs", "write_policy.rs"], why: "anim sets reloaded from disk" },
    Rule { pattern: "Rule::Joint", allowed: &["joint_tools.rs", "write_policy.rs"], why: "joint data is physics_joint's" },
    Rule { pattern: "Rule::Nav", allowed: &["nav_tools.rs", "write_policy.rs"], why: "/nav and navmesh settings are the nav tools'" },
    Rule { pattern: "Rule::Lex", allowed: &["lex_tools.rs", "write_policy.rs"], why: "/lex is the Lex tools'" },
    Rule { pattern: "Rule::LexLive", allowed: &["lex_live.rs", "write_policy.rs"], why: "/lex_live is lex_live's" },
    Rule { pattern: "Rule::Project", allowed: &["project_engine.rs", "write_policy.rs"], why: "project files are the project tools'" },
    Rule { pattern: "Opener::Modules", allowed: &["sim_writes.rs", "write_policy.rs"], why: "the engine opens the module host's sessions (ticks, logic tools)" },
    Rule { pattern: "Opener::LexLive", allowed: &["lex_live.rs", "write_policy.rs"], why: "live Lex's sessions" },
    Rule { pattern: "Opener::LexTools", allowed: &["sim_writes.rs", "write_policy.rs"], why: "the Lex tools' session" },
    Rule { pattern: "module_tick_commit(", allowed: &["sim_writes.rs", "write_policy.rs"], why: "module ticks" },
    Rule { pattern: "module_agent_commit(", allowed: &["logic/tools.rs", "write_policy.rs"], why: "agents' module loads, unloads and hooks" },
    Rule { pattern: "journal_exempt(", allowed: &["lex_live.rs", "write_policy.rs"], why: "exempt writers journal through Live::exempt; live Lex journals its runs' ticks as it goes" },
    Rule { pattern: "write_policy::rolled_back(", allowed: &["lex_live.rs"], why: "live Lex takes a refused command back itself" },
    Rule { pattern: "self.world.disk(", allowed: &["project_engine.rs"], why: "files read from disk" },
    // Variants are named where they are used: importing them bare would hide the owner check.
    Rule { pattern: "Exempt::*", allowed: &["write_policy.rs"], why: "name Exempt variants in full where they are used" },
    Rule { pattern: "Exempt::{", allowed: &["write_policy.rs"], why: "name Exempt variants in full where they are used" },
    Rule { pattern: "Rule::*", allowed: &["write_policy.rs"], why: "name Rule variants in full where they are used" },
    Rule { pattern: "Rule::{", allowed: &["write_policy.rs"], why: "name Rule variants in full where they are used" },
    Rule { pattern: "Opener::*", allowed: &["write_policy.rs"], why: "name Opener variants in full where they are used" },
    Rule { pattern: "Opener::{", allowed: &["write_policy.rs"], why: "name Opener variants in full where they are used" },
    Rule {
        pattern: "Ctx { world",
        allowed: &["sim_writes.rs", "lib.rs", "game.rs"],
        why: "the hosted game gets the live world only inside sim_writes (its own calls) or the game tool's Live::game_tool (lib.rs)",
    },
    Rule { pattern: "write_policy::disk(", allowed: &[], why: "files read from disk go through Live::disk" },
];

const SWARM_WRITES: &[&str] = &[
    "swarm.commit(",
    "swarm.undo(",
    "swarm.game_tool(",
    ".commit_joint(",
    ".commit_nav(",
    ".commit_lex(",
    ".commit_project(",
    ".commit_lex_live(",
    ".commit_disk(",
    ".check_game_changes(",
    ".game_changes(",
    ".prune_retired_now(",
];

/// Files whose structs may hold a `World`: the live one, and detached ones.
const WORLD_FIELDS: &[(&str, &str)] = &[
    ("write_policy.rs", "Live: the engine's live world"),
    ("journal.rs", "the journal's base (a detached copy replays start from)"),
    ("save_tools.rs", "a save game read and checked off the engine thread (detached until Live::replace)"),
];

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).expect("src is readable").map(|e| e.expect("entry").path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

/// `text` as code only: comments, string and character literals' contents blanked (newlines
/// kept, so lines still line up). Handles `//` and nested `/* */` comments, strings over several
/// lines with escapes, raw strings (`r#"..."#`), byte strings and char literals (`'{'`, `'"'`),
/// telling them from lifetimes (`'a`).
fn code_only(text: &str) -> Vec<String> {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        let next = c.get(i + 1).copied();
        if ch == '/' && next == Some('/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if ch == '/' && next == Some('*') {
            let mut depth = 0;
            while i < c.len() {
                if c[i] == '/' && c.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if c[i] == '\n' {
                        out.push('\n');
                    }
                    i += 1;
                }
            }
            continue;
        }
        // Raw strings: r"..", r#".."#, br#".."#.
        let raw_at = if ch == 'r' { Some(i + 1) } else if ch == 'b' && next == Some('r') { Some(i + 2) } else { None };
        let ident_before = i > 0 && (c[i - 1].is_alphanumeric() || c[i - 1] == '_');
        if let (Some(mut j), false) = (raw_at, ident_before) {
            let mut hashes = 0;
            while c.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if c.get(j) == Some(&'"') {
                out.push('"');
                j += 1;
                loop {
                    if j >= c.len() {
                        break;
                    }
                    if c[j] == '"' && (0..hashes).all(|h| c.get(j + 1 + h) == Some(&'#')) {
                        j += 1 + hashes;
                        break;
                    }
                    if c[j] == '\n' {
                        out.push('\n');
                    }
                    j += 1;
                }
                out.push('"');
                i = j;
                continue;
            }
        }
        if ch == '"' {
            out.push('"');
            i += 1;
            while i < c.len() {
                if c[i] == '\\' {
                    i += 2;
                    continue;
                }
                if c[i] == '"' {
                    i += 1;
                    break;
                }
                if c[i] == '\n' {
                    out.push('\n');
                }
                i += 1;
            }
            out.push('"');
            continue;
        }
        if ch == '\'' {
            // A char literal: 'x', '\n', '\u{..}', '\''. Otherwise a lifetime: keep the quote.
            let close = if next == Some('\\') {
                (i + 2..(i + 12).min(c.len())).find(|&k| c[k] == '\'')
            } else if c.get(i + 2) == Some(&'\'') {
                Some(i + 2)
            } else {
                None
            };
            if let Some(k) = close {
                out.push_str("' '");
                i = k + 1;
                continue;
            }
        }
        out.push(ch);
        i += 1;
    }
    out.split('\n').map(str::to_owned).collect()
}

/// How many lines the item starting at `code[at]` spans: to its `;` or `,` when it has no
/// body (a declaration, a field, a statement), else to the `}` closing its first `{`.
fn item_len(code: &[String], at: usize) -> usize {
    let (mut depth, mut parens) = (0i64, 0i64);
    let mut opened = false;
    for (k, l) in code[at..].iter().enumerate() {
        for ch in l.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => depth -= 1,
                '(' | '[' => parens += 1,
                ')' | ']' => parens -= 1,
                _ => {}
            }
        }
        let t = l.trim_end();
        if (opened && depth <= 0) || (!opened && parens <= 0 && (t.ends_with(';') || t.ends_with(','))) {
            return k + 1;
        }
    }
    code.len() - at
}

/// The non-test code of each file: `(relative path, [(line number, code)])`, each line without
/// comments or literals' contents. Test-only items (`#[cfg(test)]` on a module, a function, a
/// field, a statement: the whole item, by brace depth) and `tests.rs` files are left out.
fn engine_code() -> Vec<(String, Vec<(usize, String)>)> {
    let root = src();
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    let mut out = Vec::new();
    for f in files {
        let rel = f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        if rel.ends_with("tests.rs") || rel.contains("/tests/") {
            continue;
        }
        out.push((rel, non_test_code(&std::fs::read_to_string(&f).unwrap())));
    }
    out
}

/// One file's non-test code lines, numbered from 1 (see [`engine_code`]).
fn non_test_code(text: &str) -> Vec<(usize, String)> {
    let code = code_only(&text.replace("\r\n", "\n"));
    let mut kept = Vec::new();
    let mut i = 0;
    while i < code.len() {
        let t = code[i].trim_start();
        if let Some(rest) = t.strip_prefix("#[cfg(test)]") {
            if !rest.trim().is_empty() {
                // The item on the same line: skip from here (the attribute ends where it starts).
                i += item_len(&code, i);
                continue;
            }
            // A test-only item: skip it (and any further attributes on it) whole.
            let mut at = i + 1;
            while at < code.len() && code[at].trim_start().starts_with("#[") {
                at += 1;
            }
            i = if at < code.len() { at + item_len(&code, at) } else { at };
            continue;
        }
        if !code[i].trim().is_empty() {
            kept.push((i + 1, code[i].clone()));
        }
        i += 1;
    }
    kept
}

/// The scanner itself (review round 1 found it skipping real code after a `#[cfg(test)]`
/// field, and missing path-form calls and spacing variants): what it must see and must not.
#[test]
fn the_scanner_sees_through_test_items_literals_and_spelling() {
    let src = r##"
struct Workers {
    #[cfg(test)]
    inject: Sender<Done>,
    other: u8,
}
fn start(w: &mut World) {
    let _a = w.take_changes();
    let _b = World::take_changes(w);
    let open = '{';
    let quote = '"';
    let _s = "w.take_changes() in a string";
    let _r = r#"{"joint": ".take_changes()"}"#;
    // w.take_changes() in a comment
    let _g = Exempt::Game{ name: "x", kind };
}
#[cfg(test)]
mod tests {
    fn t(w: &mut World) { w.take_changes(); }
}
fn after(w: &mut World) { w.set_recording(Recording::Off); }
fn hand(&mut self) -> Option<&'a mut World> { None }
fn ok() -> impl FnMut(&mut World) { |_| {} }
"##;
    let code = non_test_code(src);
    let lines_with = |p: &str| code.iter().filter(|(_, l)| holds(l, p)).map(|(n, _)| *n).collect::<Vec<_>>();
    assert_eq!(lines_with(".take_changes("), vec![8, 9], "both call forms; not in strings, raw strings, comments or the test module");
    assert_eq!(lines_with("Exempt::Game {"), vec![15], "spacing doesn't matter");
    assert_eq!(lines_with(".set_recording("), vec![21], "code after a test module (and after brace and quote char literals) is seen");
    assert!(code.iter().any(|(_, l)| l.contains("other: u8")), "a #[cfg(test)] field skips only itself");
    let hands_out: Vec<usize> = code.iter().filter(|(_, l)| {
        let s = squeeze(l);
        s.contains("fn") && s.split_once("->").is_some_and(|(_, ret)| !ret.contains("Fn") && ret.contains("mutWorld"))
    }).map(|(n, _)| *n).collect();
    assert_eq!(hands_out, vec![22]);
}

/// `s` without whitespace (so `Exempt::Game{` and `Exempt::Game {` read the same).
fn squeeze(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Whether `line` holds `pattern` as a whole name (`Rule::Lex` doesn't match `Rule::LexLive`),
/// whitespace ignored. A method pattern (`.take_changes(`) also matches its path form
/// (`World::take_changes(w)`).
fn holds(line: &str, pattern: &str) -> bool {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let (line, pattern) = (squeeze(line), squeeze(pattern));
    let found = |p: &str| {
        let ends_in_name = p.chars().last().is_some_and(ident);
        line.match_indices(p).any(|(at, _)| !ends_in_name || !line[at + p.len()..].chars().next().is_some_and(ident))
    };
    found(&pattern) || pattern.strip_prefix('.').is_some_and(|m| found(&format!("::{m}")))
}

fn check(rules: &[Rule], code: &[(String, Vec<(usize, String)>)], problems: &mut Vec<String>) {
    for r in rules {
        for (file, lines) in code {
            if r.allowed.contains(&file.as_str()) {
                continue;
            }
            for (n, line) in lines {
                if holds(line, r.pattern) {
                    problems.push(format!("src/{file}:{n}: `{}` outside {:?} ({})\n    {}", r.pattern, r.allowed, r.why, line.trim()));
                }
            }
        }
    }
}

#[test]
fn escape_hatches_stay_where_they_belong() {
    let code = engine_code();
    assert!(code.len() > 50, "found the engine's sources ({} files)", code.len());
    let mut problems = Vec::new();
    check(HATCHES, &code, &mut problems);
    assert!(problems.is_empty(), "world writes outside the write policy (docs/design/WRITE_POLICY.md):\n{}", problems.join("\n"));
}

#[test]
fn exemptions_and_reserved_rules_are_used_only_by_their_owners() {
    let code = engine_code();
    let mut problems = Vec::new();
    check(OWNERS, &code, &mut problems);
    assert!(problems.is_empty(), "a write path uses an exemption or reserved rule it doesn't own (docs/design/WRITE_POLICY.md):\n{}", problems.join("\n"));
}

#[test]
fn engine_code_never_uses_swarms_write_methods() {
    let code = engine_code();
    let mut problems = Vec::new();
    for (file, lines) in &code {
        if file == "write_policy.rs" {
            continue;
        }
        for (n, line) in lines {
            for p in SWARM_WRITES {
                if holds(line, p) {
                    problems.push(format!("src/{file}:{n}: `{p}`: engine code writes the live world through write_policy::Live, not Swarm's methods (those are for detached worlds and tests)\n    {}", line.trim()));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Whether a (squeezed) line replaces a world wholesale: assigns through a world reference
/// (`*world = ..`, `*wd = ..`, `*w = ..`) or swaps, takes or replaces one (`mem::swap(world, ..)`).
fn replaces_world(s: &str) -> bool {
    let worldish = |name: &str| {
        let last = name.rsplit('.').next().unwrap_or(name);
        last.contains("world") || last == "w" || last == "wd"
    };
    let ident = |t: &str| t.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.').collect::<String>();
    if let Some(rest) = s.strip_prefix('*') {
        let name = ident(rest);
        let after = &rest[name.len()..];
        if !name.is_empty() && worldish(&name) && after.starts_with('=') && !after.starts_with("==") {
            return true;
        }
    }
    ["mem::swap(", "mem::take(", "mem::replace("].iter().any(|m| {
        s.match_indices(m).any(|(at, _)| {
            let arg = &s[at + m.len()..];
            let arg = arg.strip_prefix('&').unwrap_or(arg);
            let arg = arg.strip_prefix("mut").unwrap_or(arg);
            worldish(&ident(arg))
        })
    })
}

#[test]
fn the_live_world_is_never_replaced_wholesale() {
    // Review round 2 (probe2.rs): `*wd = World::default()` in a writer replaced the live world.
    // The hosted game can't (GameWorld); in the crate, only Live itself swaps worlds.
    let code = engine_code();
    let mut problems = Vec::new();
    for (file, lines) in &code {
        if file == "write_policy.rs" {
            continue;
        }
        for (n, line) in lines {
            if replaces_world(&squeeze(line)) {
                problems.push(format!("src/{file}:{n}: replaces a world wholesale: only write_policy::Live may (Live::replace)\n    {}", line.trim()));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    for (bad, line) in [(true, "*wd = World::default();"), (true, "*world = other;"), (true, "std::mem::swap(wd, &mut w2);"), (true, "let old = std::mem::take(&mut self.world);"), (true, "mem::replace(world, World::default())"), (false, "*count = 3;"), (false, "let w = std::mem::take(&mut self.files);"), (false, "*w == x")] {
        assert_eq!(replaces_world(&squeeze(line)), bad, "{line}");
    }
}

#[test]
fn the_live_world_has_no_other_way_in() {
    let code = engine_code();
    let mut problems = Vec::new();
    for (file, lines) in &code {
        let allowed = WORLD_FIELDS.iter().any(|(f, _)| f == file);
        for (n, line) in lines {
            let (t, s) = (line.trim(), squeeze(line));
            // A struct field holding a world (`name: World,`, `pub name: Box<World>,`...): the
            // type after the field's own `:` (not a path's `::`).
            let field_type = |x: &str| -> Option<String> {
                let b = x.as_bytes();
                let at = (0..b.len()).find(|&k| b[k] == b':' && b.get(k + 1) != Some(&b':') && (k == 0 || b[k - 1] != b':'))?;
                Some(x[at + 1..].trim_end_matches(',').to_owned())
            };
            let world_type = |t: &str| t == "World" || t == "crate::world::World" || t.contains("<World>") || t.contains("<World,") || t.contains("<crate::world::World");
            let field = s.ends_with(',') && !s.contains('(') && !s.starts_with("fn") && field_type(&s).is_some_and(|t| world_type(&t));
            if field && !allowed {
                problems.push(format!("src/{file}:{n}: a field holding a World: the live world is write_policy::Live; another engine-held world must be listed in WORLD_FIELDS with why\n    {t}"));
            }
            // A function handing out a mutable world (`-> &mut World`, `-> &'a mut World`,
            // `-> Option<&mut World>`...).
            // (A returned closure that takes one, `-> impl FnMut(&mut World, ..)`, hands out nothing.)
            let hands_out = s.contains("fn") && s.split_once("->").is_some_and(|(_, ret)| !ret.contains("Fn") && (ret.contains("mutWorld") || ret.contains("mutcrate::world::World")));
            if hands_out {
                problems.push(format!("src/{file}:{n}: hands out &mut World: writes go through write_policy's entry points\n    {t}"));
            }
        }
    }
    let policy = &code.iter().find(|(f, _)| f == "write_policy.rs").expect("write_policy.rs").1;
    let text: String = policy.iter().map(|(_, l)| format!("{l}\n")).collect();
    assert!(!text.contains("DerefMut"), "Live must stay read-only: no DerefMut");
    assert!(text.contains("pub struct Live {\n    world: World,"), "Live's world field must stay private");
    assert!(!text.contains("fn world_mut"), "Live must not hand out its world");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
