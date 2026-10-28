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
        allowed: &["world.rs", "persist.rs"],
        why: "the id counter of a detached world read from its files",
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
    Rule { pattern: "Exempt::Game {", allowed: &["lib.rs", "write_policy.rs"], why: "the hosted game's own calls go through Engine::game_call (and the game tool's repair)" },
    Rule { pattern: "Exempt::GameStart", allowed: &["lib.rs", "write_policy.rs"], why: "the hosted game's start" },
    Rule { pattern: "Exempt::Module", allowed: &["write_policy.rs"], why: "module ticks commit through write_policy::module_tick_commit" },
    Rule { pattern: "Exempt::LexLive", allowed: &["lex_live.rs", "write_policy.rs"], why: "live Lex runs' own ticks and input" },
    Rule { pattern: "Exempt::Physics", allowed: &["physics.rs", "write_policy.rs"], why: "the physics tick" },
    Rule { pattern: "Exempt::Nav", allowed: &["nav_tools.rs", "write_policy.rs"], why: "the navigation tick" },
    Rule { pattern: "Exempt::AnimReload", allowed: &["lib.rs", "write_policy.rs"], why: "anim sets reloaded from disk" },
    Rule { pattern: "Rule::Joint", allowed: &["joint_tools.rs", "write_policy.rs"], why: "joint data is physics_joint's" },
    Rule { pattern: "Rule::Nav", allowed: &["nav_tools.rs", "write_policy.rs"], why: "/nav and navmesh settings are the nav tools'" },
    Rule { pattern: "Rule::Lex", allowed: &["lex_tools.rs", "write_policy.rs"], why: "/lex is the Lex tools'" },
    Rule { pattern: "Rule::LexLive", allowed: &["lex_live.rs", "write_policy.rs"], why: "/lex_live is lex_live's" },
    Rule { pattern: "Rule::Project", allowed: &["project_engine.rs", "write_policy.rs"], why: "project files are the project tools'" },
    Rule { pattern: "Opener::Modules", allowed: &["lib.rs", "write_policy.rs"], why: "the engine opens the module host's sessions (ticks, logic tools)" },
    Rule { pattern: "Opener::LexLive", allowed: &["lex_live.rs", "write_policy.rs"], why: "live Lex's sessions" },
    Rule { pattern: "Opener::LexTools", allowed: &["lib.rs", "write_policy.rs"], why: "the Lex tools' session" },
    Rule { pattern: "module_tick_commit(", allowed: &["lib.rs", "write_policy.rs"], why: "module ticks" },
    Rule { pattern: "module_agent_commit(", allowed: &["logic/tools.rs", "write_policy.rs"], why: "agents' module loads, unloads and hooks" },
    Rule { pattern: "journal_exempt(", allowed: &["lex_live.rs", "write_policy.rs"], why: "exempt writers journal through Live::exempt; live Lex journals its runs' ticks as it goes" },
    Rule { pattern: "write_policy::rolled_back(", allowed: &["lex_live.rs"], why: "live Lex takes a refused command back itself" },
    Rule { pattern: "self.world.disk(", allowed: &["project_engine.rs"], why: "files read from disk" },
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

/// The non-test code of each file: `(relative path, [(line number, line)])`. Test modules
/// (`#[cfg(test)]` followed by `mod`) and `tests.rs` files are left out; comments too.
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
        let text = std::fs::read_to_string(&f).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        let mut kept = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let t = lines[i].trim();
            if t == "#[cfg(test)]" && i + 1 < lines.len() {
                // A test-only item (a module, a helper): skip to its end (a `mod x;` declaration
                // is one line).
                let decl = lines[i + 1];
                if decl.trim_end().ends_with(';') {
                    i += 2;
                    continue;
                }
                let indent = decl.len() - decl.trim_start().len();
                i += 2;
                while i < lines.len() && !(lines[i].len() - lines[i].trim_start().len() == indent && lines[i].trim() == "}") {
                    i += 1;
                }
                i += 1;
                continue;
            }
            if !t.starts_with("//") {
                // Code before a trailing comment.
                let code = lines[i].split(" //").next().unwrap_or("");
                kept.push((i + 1, code.to_owned()));
            }
            i += 1;
        }
        out.push((rel, kept));
    }
    out
}

/// Whether `line` holds `pattern` as a whole name (`Rule::Lex` doesn't match `Rule::LexLive`).
fn holds(line: &str, pattern: &str) -> bool {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let ends_in_name = pattern.chars().last().is_some_and(ident);
    line.match_indices(pattern).any(|(at, _)| !ends_in_name || !line[at + pattern.len()..].chars().next().is_some_and(ident))
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
                if line.contains(p) {
                    problems.push(format!("src/{file}:{n}: `{p}`: engine code writes the live world through write_policy::Live, not Swarm's methods (those are for detached worlds and tests)\n    {}", line.trim()));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_live_world_has_no_other_way_in() {
    let code = engine_code();
    let mut problems = Vec::new();
    for (file, lines) in &code {
        let allowed = WORLD_FIELDS.iter().any(|(f, _)| f == file);
        for (n, line) in lines {
            let t = line.trim();
            // A struct field holding a world (`name: World,` or `pub name: World,`).
            let field = t.ends_with(": World,") || t.ends_with(": crate::world::World,") || t.ends_with(": Option<World>,");
            if field && !allowed && !t.starts_with("fn ") && !t.contains('(') {
                problems.push(format!("src/{file}:{n}: a field of type World: the live world is write_policy::Live; another engine-held world must be listed in WORLD_FIELDS with why\n    {t}"));
            }
            if t.contains("-> &mut World") || t.contains("-> &mut crate::world::World") {
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
