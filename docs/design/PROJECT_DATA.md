# Design: project data through the protocol

Status: designed and built 2026-10-18 (M3 lane "everything through the protocol", text project data). Foundations
**F1** (swarm-first), **F2** (determinism) and **F5** (humans optional, never locked out). Roadmap item: "Everything
through the protocol" in the swarm track of [ROADMAP.md](../ROADMAP.md). Binary assets (images, models, audio) are
M4, with the asset studio.

**Goal:** the text files a game is made of (model recipes, UI screens and themes, sound recipes, style files and
module sources) get what world entities already have: **claims, versions, the journal, undo, the change feed and the
board's attribution**. Before this, two agents editing one recipe file could silently clobber each other, and a
recipe changed on disk left no trace of who changed it or how to take it back.

Lex rules were the first project data to get this (2026-10-17): they are world entities under `/lex`, written only by
the `lex_*` tools ([LEX.md](LEX.md#tools-the-engine-side)). Everything else is here.

## The decision: a versioned file store inside the world, journaled like entities

Two shapes were possible:

1. **Hidden reserved entities**, as Lex does (`/project/ui/hud.json` as a group whose props hold the text).
2. **A parallel versioned store** whose changes are journal entries of their own kind.

We built **(2)**, held by the `World` (as agents' named cameras are), because files are not entities:

- **Size.** Entity props are capped at 16 KiB (`MAX_PROPS_BYTES`) so world files and snapshots stay small; Sepulcra's
  `ui/hud.json` is already 17.7 KB, and Rust sources grow far past it. Chunking text over child entities would make
  every edit touch several entities and every diff meaningless.
- **Text, not JSON values.** Files are hand-formatted text that humans read and diff. A store of text keeps every
  byte; props would canonicalise JSON and could not hold TOML or Rust at all.
- **Not world state.** World files (`world_save`), the world hash and region claims are about the world. Project files
  live on disk in the project's own folders; saving them into a world or a save game would fork them.

What the store shares with entities is everything the swarm layer gives:

| Entities | Project files |
|---|---|
| path `/district_3/tower` | path `assets/ui/hud.json`, journaled and claimed as `/project/assets/ui/hud.json` |
| `expect_version`, `stale_version` with `conflict {base, theirs, yours, overlap, rebase}` | the same, with text and JSON diffs |
| path and region claims | **path claims** on a file or a folder (`claim_take {path: "/project/assets/ui"}`); region claims never cover files (they have no place in the world); a whole-world claim (`/`) covers them |
| journal entries (`world_history`, `world_changes`, `world_rollup`, the board) | the same entries: kind `file`, ops `file_create`, `file_write`, `file_delete` |
| `world_undo` | the same: undo restores the earlier text **in the engine and on disk** (and hot-reloads it) |
| replay (`world_hash verify`, the journal-replay test) | every change carries the whole text before and after while the journal's file-text budget keeps it (see *Bounds*): a replay rebuilds the files exactly from changes whose text is kept, and skips older ones whose text was dropped |

`Change::File { path, before, after, import }` is the journal's record (text held as `Arc<str>`, so the store, the
journal and the replay base share one copy). File changes get ids `FILE_ID_BASE | hash(path)` (bit 63; entity ids
never get that high, cameras use bit 62), so undo conflicts and history filters treat each file as one thing.
`/project` is reserved: `world_spawn` and friends refuse entities there (`reserved`), and only the `project_*` tools
make file changes (`Swarm::commit_project`), so nothing can masquerade as a file.

The world hash leaves files out (like cameras): recipes, screens, sounds and styles are presentation, and module code
enters the simulation through its build (DETERMINISM.md rule 8: a module's build hash belongs in the journal). Game
data that drives the simulation is Lex (entities, hashed). The determinism requirement for files is that **replays
reproduce them**: every write, import and undo is a journal entry holding the full text, and a test replays a
journal of file edits and compares the store byte for byte.

## Mounts: which files

A **mount** maps a name to a folder on disk. Files are named by mount and relative path, with `/` separators.

| Mount | Folder | When |
|---|---|---|
| `assets` | the engine's assets root (`--assets`, else the hosted game's, else the working directory) | at start |
| `modules/<name>` | a module crate's folder (under the working directory, as `logic_build` requires) | `project_mount {module}` |

Only files of known kinds are mirrored; everything else on disk is left alone:

| Kind | Which files | Validated before commit by | Hot reload after commit |
|---|---|---|---|
| `recipe` | `*.json` in a folder named `recipes` | `specula_model::Recipe::parse` with the style that governs it (the first `style.json` walking up) | the model library re-reads it now; models using it rebuild (captures wait for them) |
| `style` | `style.json` | `Style::parse`, then every recipe it governs, or will (a new, closer style takes over the recipes below it), is parsed again: an edit that breaks a recipe that loaded is refused. Deleting one checks the recipes against the style they fall back to. With `--style FILE` every recipe is checked against that file instead | as recipes |
| `style_rules` | `style.toml` | the TOML parser and `MotionRules::from_toml` (the `[motion]` table) | read fresh by every `anim_check` |
| `ui_screen` | `*.json` in a folder named `ui`, except `theme.json` | `specula_ui::Screen::parse` with that folder's `theme.json` | the hosted game (`Game::assets_changed`): Sepulcra reloads its HUD (window and captures) |
| `ui_theme` | `ui/theme.json` | `Theme::from_json_str`, then every screen of the folder parsed again; deleting it is refused while screens use it | as screens |
| `sound` | `*.json` in a folder named `sounds` | `specula_audio`'s sound recipe parser | the hosted game: Sepulcra re-renders that sound |
| `anim_set` | `*.json` in a folder named `anims` | JSON syntax (the anim lane owns its schema) | the model library, as recipes |
| `rust` | `*.rs` under a module's `src/` | **`logic_build` only** (see below) | `build: true` on the write runs `logic_build` |
| `cargo` | a module's `Cargo.toml` | `check_module_manifest` (a module's manifest only), then `logic_build` | as `rust` |
| `json` | other `*.json` at a module's top level (`actions.json`) | JSON syntax | as `rust` |

Scans skip hidden entries (names starting with `.`, which is also where the engine's temporary files go), symbolic
links, and the folders `target`, `worlds`, `saves`, `captures`, `node_modules`; they go at most 8 folders deep and
look at most 20,000 entries. Files larger than 1 MiB, or not UTF-8, are listed under `skipped` and not mirrored.

**Rust sources are validated by building them.** The project tools don't parse Rust; a module edit is committed
(journaled, written, undoable) and `build: true` (or `"check"`) runs `logic_build` for the module straight after,
answering with its diagnostics as data. A build that fails never replaces the running module (MODULES.md), so the
world keeps running on the last good build; `world_undo` takes the source back. Validating a module before commit
would mean building a shadow copy of the crate, which its relative path dependencies make awkward; it is listed under
*Later*.

**Only real modules mount, and their manifests stay modules.** `project_mount` refuses any folder whose `Cargo.toml`
isn't a module's (a `cdylib` whose only dependency is `specula-sdk`, by path, and that path must lead to the SDK
crate), the engine's working directory, folders with a `build.rs`, and folders inside or holding another mount (the
assets root included: one disk file never gets two project paths, `mount_overlap`). The engine's own crates and the
root package are never project files. Every `Cargo.toml` commit is checked the same way (`check_module_manifest_at`):
no `build`, `links` or proc-macro (nor the `proc_macro` / `build_dependencies` spellings), no `[workspace]`,
`[patch]`, `[replace]`, `[target]` or `[build-dependencies]`, no renaming another crate to `specula-sdk` with
`package =`, `lib.path` inside `src/`, and only path dependencies, so a crates.io dependency or a build script never
reaches the disk. Every `.rs` commit has its compile-time paths checked (`#[path]`, `include_str!`,
`include_bytes!`, `include!` stay inside the module's folder, never `target/`, as `logic_build`'s vet requires;
ambulo includes its own `actions.json`).

**`logic_build {fix: true}` on a mounted module never writes the disk itself.** It is refused up front while another
agent claims any of the module's files (`Claims::check_subtree`). Each round takes the store's texts first, builds
without fixing, works rustc's fixes out against exactly those texts (`specula_logic::fix::plan`), and commits them as
the caller's change through the store (claims checked again, journaled, undoable; the engine then writes the disk, so
nothing is imported as `disk`). A file another agent or a human changed during the build keeps their text: its fixes
are skipped and reported (`fixes_journaled.skipped`), because their offsets belong to the old text.

## Tools

| Tool | What it does |
|---|---|
| `project_list` | The mounts, then files (path, kind, version, bytes, last writer, disk state), paged and filtered by prefix and kind; files whose last disk write failed and disk conflicts are listed first. |
| `project_read` | A file's text with its version (a line range, at most 64 KiB per call, `next_line` to continue), or one JSON value (`json_path`), or an **earlier version** from the journal (`version`); `problems` lists what validation says about it now. |
| `project_write` | The whole text. `expect_version` (0: must not exist yet) guards against overwriting someone's newer change. Creates the file (and its folders) when it is new. |
| `project_patch` | Atomic ops on the text: `{set: "parts[1].shape.size", value}` and `{remove: "notes"}` for JSON files (only the value's bytes change, so the file's formatting survives), `{find, replace}` (the text must occur exactly once) and `{lines: [from, to], text}` for any file. `dry_run` validates and returns the diff. |
| `project_delete` | Deletes a file (`expect_version` applies). |
| `project_mount` | Mirrors a module crate's sources (`games/ambulo`): `Cargo.toml`, top-level `*.json`, `src/**/*.rs`. |

Writes, patches and deletes are **claim-checked** (`claimed` names the owner, as for entities), **validated by kind**
(an edit that leaves the file invalid is refused with every problem: path, code, message, offset), **journaled**
with their author, written to disk, and **hot-reloaded**. Every answer is bounded: reads at 64 KiB, listings at
64 KiB with a cursor, conflict diffs at 8 KiB.

**Threads.** The main thread only snapshots (cheap: text is shared, not copied) and commits. Patching, validation and
diffs run on the calling agent's protocol thread: an edit takes a snapshot, applies its ops, validates the result
and sends a commit carrying the version it validated against. If the file changed meanwhile, the commit is refused
and the edit is redone on the new text (four tries, then `busy`), unless the caller passed `expect_version`, in which
case it gets `stale_version`.

### Conflict help

A write whose `expect_version` is stale is refused with `stale_version` and a `conflict` object, the same shape as
for entities ([conflict.rs](../../crates/specula/src/conflict.rs)):

- `base`: the version the agent expected (its text comes from the journal; `kept: false` if it has been evicted);
- `theirs`: each later change (journal id, agent, tick, version; "disk" for edits made outside the engine) and the
  diff from base to the current text;
- `yours`: the diff from base to what the refused write would have made;
- `overlap`: where both diffs touch the same thing (JSON paths for JSON files, base line ranges for other text);
  empty means the agent can re-apply its edit on the current version;
- `rebase`: what to do, with the version to pass next.

JSON files are diffed by **path** (`parts[2].shape.size: [0.5, 0.8] -> [0.6, 0.8]`), other files by **line hunks**
(`{base_lines: [12, 14], removed: [...], added: [...]}`); values and hunks are cut to keep the object under 8 KiB.
M4 adds suggested three-way merges.

## Disk and store: who writes, when, and how

The engine is the **only writer** of mounted files while it runs. The store is the protocol's view; the disk is what
humans, editors, Git and the next engine see. They are kept equal like this:

- **After every commit** (a write, a patch, a delete, an undo, an import), on the main thread, before the tool
  answers: each file whose text differs from what was last synced is written **atomically**: to a hidden temporary
  file in the same folder (`.<name>.specula-tmp-<pid>-<n>`), flushed, then renamed over the target. The file is read
  back and hashed: if what is there isn't what was written, someone wrote at the same moment, and their text is
  imported (below). A deleted file is removed. The answer says `disk: "written"`, or the error.
- **A failed write** (permissions, a full disk, a locked file) leaves the commit in place (it is journaled and
  undoable), is retried after every later commit and once a second, and is listed first by `project_list` with its
  error (`disk: "pending"`). Nothing is silently dropped: the store holds the text until the disk takes it.
- **Edits made outside the engine** (a human with an editor, `git checkout`): a watcher thread stats the mounted
  files once a second, and the main thread stats a file again right before any agent writes it. A file whose size,
  time or content changed is read, and if its text differs from the store it is **imported as a journaled change by
  `disk`** (a reserved agent name), with its version bumped: agents see it in `world_changes`, `world_history` and
  `project_list`, and an agent holding an older version gets `stale_version` with the human's edit as `theirs`.
  New files are imported as created (`import: true`), deleted ones as deleted.
- **Neither side is ever lost.** A human edit replaces the store's text, but every earlier version is in the
  journal: `project_read {version}` shows it and `world_undo` (with `force`, as it is someone else's change) puts it
  back, on disk too. If the store held a change the disk hadn't taken yet when the human edited, the human's text wins
  on disk and in the store, the unwritten version stays in the journal, and `project_list` reports a `disk_conflict`.
  An import that creates a file can't be undone (that would delete a file the agent never made): `project_delete`
  removes it on purpose.
- **Files that can't be mirrored.** An outside edit that leaves a mirrored file non-UTF-8 or over 1 MiB is **held**:
  listed by `project_list` (`disk_unreadable`), never overwritten by a write, an undo or a retry, and refused to
  agents (`disk_unreadable`) until it is fixed on disk (then it is imported) or an agent passes
  `replace_unreadable: true`, which first copies the bytes to a hidden backup next to the file (never mirrored; the
  answer names it; it works even when the new text is the one the store already has). A file put back on disk to
  exactly the store's text is released.
- **Links.** A write or delete whose folder is (or passes through) a link or junction is refused before anything is
  committed (`linked_folder`).
- **At start**, the `assets` mount is scanned and its files enter the store as one `disk` entry at tick 0 (imports,
  not undoable).
- **World loads and save games** replace the world but keep the files: they are the project, not the world.
- **Case.** On Windows two paths that differ only in case are one file: creating one is refused (`case_clash`),
  comparing with Unicode case folding (`Ä` and `ä` clash), and device names (`CON`, `COM1`-`COM9`, `COM¹`,
  `CONIN$`, ...) are refused as names.
- **Versions never repeat at a path.** A world remembers each deleted file's last version (the 16,384 most recently
  deleted paths; past that the oldest tombstone goes and its version raises a floor for every path), and a file made
  there again continues from it, so an `expect_version` read from the old file can never match the new one.

## Bounds

| What | Limit | Past it |
|---|---|---|
| One file | 1 MiB of UTF-8 text | `too_large` (writes); `skipped` (scans) |
| Files in the store | 4,096 | `store_full` |
| Text in the store | 32 MiB | `store_full` |
| Ops per patch | 200 | argument error |
| Module mounts | 16 | `mount_limit` |
| Scan | depth 8, 20,000 entries | the rest is not mirrored (`scan_truncated`) |
| Response | 64 KiB (reads, listings), 8 KiB (conflict diffs) | cursors, `next_line`, cut diffs |
| File text in the journal | 64 MiB | oldest texts dropped (`too_old` on undo) |

**File text has its own journal budget** (64 MiB, `MAX_FILE_TEXT_BYTES`), apart from the rings' 48 and 16 MiB: a
file change counts only its bookkeeping there. Past the budget the oldest entries' texts are dropped (`kept: false`):
those changes stay in the history but can't be undone (`too_old`), read back (`project_read {version}` answers
`not_kept`; conflict help reports the base as `kept: false`) or replayed, while world history, undo and the replay
base (`world_hash verify`) are untouched. The list of entries holding text follows the rings: entries they drop leave
it too. So one agent rewriting a 1 MiB file can never push anyone's world
history out. Diffs stay bounded too: JSON diffs match objects through a sorted index and stop at their cut (a
70,000-key object diffs in about 4 ms), and texts over 512 KiB are diffed by lines. Edits keep a file's own line
endings (CRLF stays CRLF).

## Proof

`python scripts/project_check.py --window --adapter integrated` runs Sepulcra on a scratch copy of its assets and
checks, through the tools alone:

- two agents patch `tombstone.json` from the same version (one recolours the slab, the other raises the triangle
  budget, 400 to 420): the second gets `stale_version` with `overlap: []`, rebases on the version `rebase` names and
  succeeds; the file on disk holds both edits with its formatting kept (one line added for the new member);
- a claim on `/project/assets/ui/hud.json` refuses another agent (`claimed`, naming the owner); the owner's edit
  ("Wave" to "Night") hot-reloads into the HUD (window and captures);
- `world_undo` puts the old text back on disk and in the HUD;
- a sound recipe edited on disk is imported as a change by `disk` within a second or two, and an agent still on the
  old version gets the human's edit as `theirs` (overlap: `loudness`);
- an edit that would break the recipe is refused with the recipe parser's error;
- captures before and after: the HUD's top bar, and all ten tombstones rebuilt with the new slab colour.

Unit tests cover the store under the swarm layer (claims, undo conflicts, imports that can't be undone, `reserved`,
replay reproducing the files), formatting-preserving JSON edits, diffs and their overlap, validation of Sepulcra's
real files, and disk sync (atomic writes, deletes, an outside edit never overwritten).

Found on the way: the model library's hot reload re-keyed only the **first** handle of a changed recipe (working out
its key made the recipe known again, so the rest were skipped); Sepulcra's ten tombstones, each its own seed, showed
it. Every handle of a changed recipe now rebuilds (`models.rs`, with a test).

## Later

- Validating module sources before commit (a shadow copy of the crate checked with `cargo check`).
- Suggested three-way merges (M4, conflict help).
- Binary assets (M4): the same store and journal, with content-addressed blobs instead of text.
- Anim sets' schema (the anim lane), game data files other than Lex.
