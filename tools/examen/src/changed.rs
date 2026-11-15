//! What a change needs verified. The gate is sized to what changed: a change to documents does
//! not need what a change to the engine needs (owner, 2026-11-15: "someone will be very
//! concerned when they're pressed on project time"; decided 2026-11-16).
//!
//! Every changed path is put in a tier, and the change's tier is the highest of them. **A path
//! nobody listed here is the engine's**: sizing may only ever leave a step out for a path that
//! is known not to need it. A file is known by what it is (a Markdown document, a picture under
//! `docs/`), never by the folder it happens to be in: `docs/` also holds assets that tests read.
//!
//! There is no tier for a game. The engine's own library tests read Sepulcra's assets, so a
//! change under `games/` gets what the engine gets (review R1 of this lane, finding 9).

use std::path::Path;
use std::time::Duration;

use crate::run::said_text;

/// What a change needs, least first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Documents and pictures that no test reads: the document guards.
    Docs,
    /// Tests, tooling and data that tests read: the workspace's tests once, and clippy.
    Tests,
    /// The engine, a game, or anything this file does not know: everything.
    Engine,
}

impl Tier {
    pub const ALL: [Tier; 3] = [Tier::Docs, Tier::Tests, Tier::Engine];

    pub fn name(self) -> &'static str {
        match self {
            Tier::Docs => "docs",
            Tier::Tests => "tests",
            Tier::Engine => "engine",
        }
    }

    pub fn named(name: &str) -> Option<Tier> {
        Tier::ALL.into_iter().find(|t| t.name() == name)
    }

    /// The gate's steps a change of this tier needs (`gate::STEPS` names them).
    pub fn steps(self) -> &'static [&'static str] {
        match self {
            Tier::Docs => &["docs"],
            Tier::Tests => &["tests", "clippy"],
            Tier::Engine => &["tests", "repeat", "slow", "speed", "clippy", "builds"],
        }
    }
}

/// Documents and folders under `docs/` that a test reads, so that a change to one is a change
/// to what a test checks: a path, or a folder ending in `/`. `tests::what_sources_read_under_docs_is_listed`
/// holds this list against the sources.
pub const READ_BY_TESTS: &[&str] = &[
    // crates/specula/tests/dll_owners.rs, games/sepulcra-app/tests/review.rs and the playtest
    // engine's tests read the rules.
    "AGENTS.md",
    // crates/specula/tests/gallery.rs reads the visual history's index and its pictures.
    "docs/gallery/",
    // crates/specula-studio/tests/fidelity.rs and crates/specula-scintilla/tests/fidelity_stage.rs
    // read the fidelity stage's recipes, styles and effects.
    "docs/fidelity/assets/",
    // games/sepulcra-app/tests/prop_cells.rs holds the game's props to its design.
    "games/sepulcra/DESIGN.md",
    // crates/specula/src/timeline.rs holds the timeline's design to its schema.
    "docs/design/TIMELINE.md",
];

/// Folders whose every file is read by tests, by the tooling or by the live checks, whatever
/// its kind (crates/specula/tests/evals_suite.rs reads the Markdown under two of them).
const READ_WHOLE: &[&str] = &["tools/", "scripts/", "evals/", "perf/"];

/// Folders of a package that hold tests and examples only.
const TEST_FOLDERS: &[&str] = &["/tests/", "/examples/", "/benches/"];

/// What decides a change's own size and what CI runs: never sized by itself.
const DECIDES_THE_SIZE: &[&str] = &[".github/", "tools/examen/src/changed.rs"];

fn listed(path: &str) -> bool {
    READ_BY_TESTS.iter().any(|d| if d.ends_with('/') { path.starts_with(d) } else { path == *d })
}

/// The tier of one changed path (relative to the repository's root, with `/`).
pub fn tier_of(path: &str) -> Tier {
    if DECIDES_THE_SIZE.iter().any(|d| if d.ends_with('/') { path.starts_with(d) } else { path == *d }) {
        return Tier::Engine;
    }
    if listed(path) || READ_WHOLE.iter().any(|f| path.starts_with(f)) {
        return Tier::Tests;
    }
    // A document: Markdown anywhere, a picture under docs/, a licence, git's ignore list.
    let picture = path.starts_with("docs/") && [".png", ".jpg", ".jpeg", ".gif", ".svg"].iter().any(|e| path.ends_with(e));
    if path.ends_with(".md") || picture || path.starts_with("LICENSE") || path == ".gitignore" {
        return Tier::Docs;
    }
    // Anything else under docs/ is data that something may read.
    if path.starts_with("docs/") {
        return Tier::Tests;
    }
    // A crate's tests and examples, when they are Rust sources. Data beside them is not: library
    // tests compile some of it in (`include_bytes!` of the Vorbis fixtures, `include_str!` of
    // `determinism.json`), and those tests are repeated (review R2 of this lane).
    if path.starts_with("crates/") && path.ends_with(".rs") && TEST_FOLDERS.iter().any(|f| path.contains(f)) {
        return Tier::Tests;
    }
    Tier::Engine
}

/// What was found to have changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub tier: Tier,
    /// What the change is measured from, in words (a commit, or why none could be used).
    pub since: String,
    /// The changed paths, sorted.
    pub paths: Vec<String>,
    /// One path of the change's tier: why the tier is what it is.
    pub because: Option<String>,
}

impl Change {
    /// Everything, with the reason nothing less could be chosen.
    pub fn everything(why: impl Into<String>) -> Change {
        Change { tier: Tier::Engine, since: why.into(), paths: Vec::new(), because: None }
    }

    pub fn of(since: impl Into<String>, mut paths: Vec<String>) -> Change {
        paths.sort();
        paths.dedup();
        let tier = paths.iter().map(|p| tier_of(p)).max().unwrap_or(Tier::Docs);
        let because = paths.iter().find(|p| tier_of(p) == tier).cloned();
        Change { tier, since: since.into(), paths, because }
    }

    /// One line for a person.
    pub fn said(&self) -> String {
        match &self.because {
            Some(path) => format!("{}: {} path(s) changed since {}; the highest is {path}", self.tier.name(), self.paths.len(), self.since),
            None if self.paths.is_empty() && self.tier == Tier::Docs => format!("docs: nothing changed since {}", self.since),
            None => format!("{}: {}", self.tier.name(), self.since),
        }
    }
}

/// The names of a `-z` listing from git.
pub fn names(listing: &str) -> Vec<String> {
    listing.split('\0').map(|l| l.trim().replace('\\', "/")).filter(|l| !l.is_empty()).collect()
}

/// What differs between `base` and this checkout as it stands: the commits since the two
/// parted, what is changed and not committed, and files git does not track (and does not
/// ignore). A file that was moved is two paths, the one it left and the one it came to
/// (`--no-renames`: git would name the new one alone, and a file moved into `docs/` would be a
/// document). When git cannot say, the answer is everything.
pub fn since(base: &str, root: &Path) -> Change {
    let limit = Duration::from_secs(120);
    // (An all-zero commit is what a push of a new branch names as its "before".)
    if base.is_empty() || base.bytes().all(|b| b == b'0') {
        return Change::everything(format!("no commit to measure from ({base:?})"));
    }
    let Some(parted) = said_text(&["git", "merge-base", base, "HEAD"], root, limit) else {
        return Change::everything(format!("git knows no commit {base:?} that this checkout shares"));
    };
    let parted = parted.trim().to_owned();
    let Some(tracked) = said_text(&["git", "diff", "--no-renames", "--name-only", "-z", &parted], root, limit) else {
        return Change::everything("git did not say what changed");
    };
    let Some(untracked) = said_text(&["git", "ls-files", "--others", "--exclude-standard", "-z"], root, limit) else {
        return Change::everything("git did not say which files it does not track");
    };
    let paths = names(&tracked).into_iter().chain(names(&untracked)).collect();
    let short: String = parted.chars().take(8).collect();
    Change::of(format!("{base} ({short})"), paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::root;

    #[test]
    fn a_path_nobody_listed_is_the_engines() {
        for path in [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            ".cargo/config.toml",
            ".gitattributes",
            ".mcp.json",
            "crates/specula/src/lib.rs",
            "crates/specula-gpu/shaders/mesh.frag",
            "crates/specula-gpu/shaders/mesh.frag.spv",
            "crates/specula-physics/clippy.toml",
            "new-folder/file",
            "crates/specula-core/Cargo.toml",
            // Data beside a crate's tests: library tests compile some of it in.
            "crates/specula-audio/tests/data/vorbis/tone.ogg",
            "crates/specula-audio/tests/data/determinism.json",
            // A game is the engine's: its library tests read the game's assets.
            "games/sepulcra/src/lib.rs",
            "games/sepulcra/assets/recipes/tower.json",
            "games/sepulcra-app/tests/review.rs",
            "games/ambulo/Cargo.toml",
            // What decides the size is never sized by itself.
            ".github/workflows/ci.yml",
            ".github/workflows/newest.yml",
            "tools/examen/src/changed.rs",
        ] {
            assert_eq!(tier_of(path), Tier::Engine, "{path}");
        }
    }

    #[test]
    fn each_kind_of_path_has_its_tier() {
        for path in ["HANDOFF.md", "README.md", "docs/ROADMAP.md", "docs/fidelity/sepulcra_slice_hud.png", "games/sepulcra/STYLE.md", "crates/specula-gpu/README.md", "LICENSE-MIT", ".gitignore"] {
            assert_eq!(tier_of(path), Tier::Docs, "{path}");
        }
        for path in [
            "AGENTS.md",
            "docs/gallery/index.json",
            "docs/gallery/thumbs/a.jpg",
            "docs/gallery/README.md",
            "games/sepulcra/DESIGN.md",
            "docs/design/TIMELINE.md",
            // (Review R1, finding 1: tests read the fidelity stage's assets; they are not documents.)
            "docs/fidelity/assets/recipes/barrel_t3.json",
            "docs/fidelity/assets/style.toml",
            "docs/fidelity/assets/vfx/x.json",
            "docs/fidelity/assets/notes.md",
            // Data under docs/ that nobody listed is not a document either.
            "docs/some/new/data.json",
            "scripts/window_check.py",
            "tools/examen/src/gate.rs",
            "evals/leverage/settings.json",
            // (The same finding: a test reads the Markdown under evals/ and perf/.)
            "evals/public/README.md",
            "perf/README.md",
            "perf/drills/a.json",
            "crates/specula/tests/zero_dep.rs",
            "crates/specula-gpu/examples/bench.rs",
        ] {
            assert_eq!(tier_of(path), Tier::Tests, "{path}");
        }
    }

    #[test]
    fn a_changes_tier_is_its_highest_paths() {
        let paths = |list: &[&str]| list.iter().map(|p| (*p).to_owned()).collect::<Vec<_>>();
        let c = Change::of("x", paths(&["HANDOFF.md", "docs/ROADMAP.md"]));
        assert!(c.tier == Tier::Docs && c.because.as_deref() == Some("HANDOFF.md"), "{c:?}");
        let c = Change::of("x", paths(&["HANDOFF.md", "crates/specula/tests/zero_dep.rs", "scripts/window_check.py"]));
        assert!(c.tier == Tier::Tests && c.because.as_deref() == Some("crates/specula/tests/zero_dep.rs"), "{c:?}");
        let c = Change::of("x", paths(&["HANDOFF.md", "crates/specula-core/src/json.rs", "scripts/window_check.py"]));
        assert_eq!(c.tier, Tier::Engine);
        // (Review R1, finding 2: a moved file is the path it left as well as the one it came to.)
        let c = Change::of("x", paths(&["docs/tower.md", "games/sepulcra/assets/tower.json"]));
        assert_eq!(c.tier, Tier::Engine);
        assert_eq!(Change::of("x", Vec::new()).tier, Tier::Docs, "nothing changed: the document guards");
        assert_eq!(Change::everything("why").tier, Tier::Engine);
        assert!(Tier::Docs < Tier::Tests && Tier::Tests < Tier::Engine);
        for tier in Tier::ALL {
            assert_eq!(Tier::named(tier.name()), Some(tier));
        }
        assert_eq!(Tier::named("game"), None);
    }

    #[test]
    fn a_base_git_cannot_use_means_everything() {
        let root = root();
        assert_eq!(since("", &root).tier, Tier::Engine);
        assert_eq!(since("0000000000000000000000000000000000000000", &root).tier, Tier::Engine);
        assert_eq!(since("no-such-branch-or-commit-anywhere", &root).tier, Tier::Engine);
        assert_eq!(names("a.rs\0dir\\b c.rs\0\0"), ["a.rs", "dir/b c.rs"]);
    }

    /// A quoted path in a Rust source that names something under `docs/`, or a Markdown file, is
    /// something code may read. Each is covered by `READ_BY_TESTS` (a change to it then runs the
    /// tests) or named here, with the reason, as not read. The first form of this guard looked
    /// for Markdown names only, and missed the fidelity stage's assets (review R1, finding 1).
    #[test]
    fn what_sources_read_under_docs_is_listed() {
        // (source, quoted text): not something a test reads from this repository.
        const NOT_READ: &[(&str, &str, &str)] = &[
            ("crates/specula/src/review_tools.rs", "docs/ROADMAP.md", "a tool's argument in a test of its refusals"),
            ("crates/specula/src/review_tools.rs", "a.md", "the same"),
            ("crates/specula/src/project.rs", "docs/clip.ogg", "a path in a made-up project"),
            ("crates/specula/src/project.rs", "docs/gallery/shot.png", "a path in a made-up project"),
            ("crates/specula/src/project.rs", "taste/style_notes.md", "a path in a made-up project"),
            ("crates/specula/src/project.rs", "style/notes.md", "a path in a made-up project"),
            ("crates/specula/src/project.rs", "recipes/readme.md", "a path in a made-up project"),
            ("crates/specula/src/taste.rs", "assets/style/notes.md", "a project's own file"),
            ("crates/specula-anim/examples/filmstrip.rs", "docs/fidelity/sepulcra_anim_filmstrip.png", "a picture the example writes"),
            ("crates/specula-model/examples/preview.rs", "docs/fidelity/sepulcra_t1_contact.png", "a picture the example writes"),
        ];
        // (source, quoted text, the listed path it is): a file named without its folder.
        const IS: &[(&str, &str, &str)] = &[
            ("crates/specula/tests/gallery.rs", "README.md", "docs/gallery/README.md"),
            ("crates/specula/tests/dll_owners.rs", "AGENTS.md", "AGENTS.md"),
            ("games/sepulcra-app/tests/review.rs", "AGENTS.md", "AGENTS.md"),
            ("games/sepulcra-app/tests/prop_cells.rs", "sepulcra/DESIGN.md", "games/sepulcra/DESIGN.md"),
        ];
        // The tooling names documents and has a step called "docs"; its tests are the document
        // guards, which read every document: that is what they do.
        const GUARDS: &[&str] = &["tools/examen/"];
        let root = root();
        let (mut unlisted, mut seen) = (Vec::new(), 0);
        let mut stack = vec![root.join("crates"), root.join("games"), root.join("tools")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())).flatten() {
                let path = entry.path();
                let rel = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
                if path.is_dir() {
                    if !rel.ends_with("/target") {
                        stack.push(path);
                    }
                    continue;
                }
                if !rel.ends_with(".rs") || GUARDS.iter().any(|g| rel.starts_with(g)) {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                // Every quoted piece of every line: the text between two quotes.
                for line in text.lines() {
                    for quoted in line.split('"').skip(1).step_by(2) {
                        let name = quoted.trim_start_matches("../").trim_start_matches("./");
                        let under_docs = name.starts_with("docs/");
                        if !(under_docs || name.ends_with(".md")) || name.contains(char::is_whitespace) || name.contains('{') {
                            continue;
                        }
                        seen += 1;
                        let folder = format!("{}/", name.trim_end_matches('/'));
                        let covered = listed(name) || listed(&folder) || READ_BY_TESTS.contains(&folder.as_str()) || tier_of(name) == Tier::Tests && READ_WHOLE.iter().any(|f| name.starts_with(f));
                        let known = IS.iter().any(|(src, q, real)| *src == rel && *q == name && listed(real)) || NOT_READ.iter().any(|(src, q, _)| *src == rel && *q == name);
                        if !covered && !known {
                            unlisted.push(format!("{rel}: \"{quoted}\""));
                        }
                    }
                }
            }
        }
        assert!(seen >= 10, "the guard found {seen} quoted paths under docs/ or of documents: has their form changed?");
        assert!(
            unlisted.is_empty(),
            "Rust sources name these under docs/ (or as documents) and changed.rs does not know them: add each to READ_BY_TESTS, or to NOT_READ or IS in this test with the reason:\n{}",
            unlisted.join("\n")
        );
        // Every entry of the list is there.
        for entry in READ_BY_TESTS {
            assert!(root.join(entry).exists(), "READ_BY_TESTS names {entry}, which is gone");
        }
    }
}
