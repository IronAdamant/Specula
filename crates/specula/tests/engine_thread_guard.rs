//! The engine thread's guard (docs/design/WORKERS.md): engine-thread code never touches the disk,
//! encodes an image, deletes a folder, reads back GPU pixels in a loop or starts a thread of its
//! own; that work goes to the worker pool (`Engine::offload`) or runs in budgeted slices
//! (`Engine::step_job`). This test counts those calls in the engine crate's non-test code, file by
//! file, against an allow-list with each file's reason (a worker-side module, a caller-thread
//! tool, start-up, a small bounded read). A call anywhere else fails, and so does a count that
//! grew; a count that shrank asks for the lower number, so the list only ever shrinks.
//!
//! Recurring bug class C (2026-10-29): a scratch delete, a world save, PNG encodes and project
//! fsyncs on the engine thread held it for 150 ms to 8.5 s; each looked like an ordinary call.
//!
//! Shared lexer: the same as the disk and write-policy guards (comments and literals blanked,
//! test items skipped by depth).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The blocking calls looked for (matched with whitespace removed, across lines).
const PATTERNS: &[(&str, &str)] = &[
    ("fs::", "file-system calls (reads, writes, metadata, deletes)"),
    ("sync_all", "an fsync"),
    ("remove_dir_all", "a recursive delete"),
    ("png::encode", "a PNG encode"),
    ("read_rgba8", "a GPU readback"),
    ("thread::Builder", "a thread of its own (use the worker pool)"),
    ("thread::spawn", "a thread of its own (use the worker pool)"),
    // Disk work through the engine's own helpers (review of 998af3d: desync_trace wrote 11.7 MB
    // through the disk book on the engine thread, unseen by the patterns above).
    ("book().write", "a file write through the disk book"),
    ("book().append", "a file append through the disk book"),
    ("book().copy", "a file copy through the disk book"),
    ("book().make_dir", "a folder made through the disk book"),
    ("remove_tree_inside", "a recursive delete"),
    ("remove_file_inside", "a file delete"),
    ("persist::execute", "a world write"),
    ("persist::load", "a world read"),
    ("persist::save", "a world write"),
    ("peek_entity_count", "a world file read"),
    (".exists()", "a file-system stat"),
    (".metadata()", "a file-system stat"),
    (".symlink_metadata()", "a file-system stat"),
    (".is_file()", "a file-system stat"),
    (".is_dir()", "a file-system stat"),
    (".canonicalize()", "a file-system path resolution"),
    (".read_dir()", "a folder listing"),
];

/// Imports that would hide file-system calls from the patterns above: refused everywhere.
const HIDDEN: &[(&str, &str)] = &[
    ("usestd::fsas", "std::fs renamed"),
    ("usestd::fs::", "std::fs items imported by name (call them as fs::...)"),
    ("usestd::{fs", "std::fs imported in a group"),
    ("fs::{", "std::fs items imported in a group"),
];

/// A file's allowance: how many of each pattern, and why they are not engine-thread stalls.
struct Allowed {
    file: &'static str,
    counts: &'static [(&'static str, usize)],
    why: &'static str,
}

/// Every file with blocking calls, and why they don't stall the engine thread. Sorted by file.
const ALLOWED: &[Allowed] = &[
    Allowed { file: "approved.rs", counts: &[(".is_dir()", 1), (".is_file()", 1), ("fs::", 2)], why: "which module sources an owner or lead last built: one small file a project (at most 64 KiB), read by the start-up build's own thread (run_modules) and written by logic_build on its caller's connection thread after a load, and by the command line's own process (specula new): never on the engine thread" },
    Allowed { file: "anim_tools.rs", counts: &[("fs::", 1), ("read_rgba8", 1)], why: "a style.toml read (a few KiB, once per anim call); one filmstrip tile readback per step job slice" },
    Allowed { file: "anim_tools/anim_steps.rs", counts: &[("book().write", 1)], why: "the filmstrip packet, written inside offload_capture's worker job" },
    Allowed { file: "audio_tools.rs", counts: &[("book().write", 1)], why: "caller-thread tool (CallerThread::Audio): audio_preview writes its WAV and picture on the agent's connection" },
    Allowed { file: "balance_tools.rs", counts: &[(".is_file()", 1), ("fs::", 2)], why: "caller-thread tool (CallerThread::Balance): targets files read on the agent's connection" },
    Allowed { file: "blobs.rs", counts: &[(".is_file()", 1), ("book().make_dir", 2), ("fs::", 9), ("remove_file_inside", 2), ("remove_tree_inside", 1), ("sync_all", 1)], why: "the content store: written by project_upload on the caller's connection thread" },
    Allowed { file: "capture.rs", counts: &[("book().write", 2), ("png::encode", 1), ("read_rgba8", 1)], why: "PNG and packet writes and the encode run inside offload_capture's worker job; one readback per view, a copy out of the readback buffer once the capture flight found its render done (no fence wait)" },
    Allowed { file: "desync.rs", counts: &[("fs::", 2)], why: "desync_compare reads a trace file the agent names, bounded by desync::MAX_TRACE_BYTES; rare, lead-level debugging" },
    Allowed { file: "disk.rs", counts: &[(".canonicalize()", 3), (".exists()", 1), (".is_dir()", 8), (".is_file()", 6), ("fs::", 34), ("remove_dir_all", 1), ("remove_file_inside", 2), ("remove_tree_inside", 2), ("sync_all", 1), ("thread::Builder", 1)], why: "the disk book itself: its walker thread, and the write and delete helpers its callers run (each call site is counted where it is); write_synced syncs for its caller, on the caller's thread" },
    Allowed { file: "editor_engine.rs", counts: &[("thread::Builder", 1)], why: "the editor's tool thread (started once, at the first stroke): the draw tools' caller-thread tools (terrain_edit, model_paint, model_path) wait on the engine thread's own answers and on a rebuild, so they can run neither on it nor on one of the pool's two workers; one stroke is in flight at a time" },
    Allowed { file: "evidence.rs", counts: &[(".exists()", 1), (".is_dir()", 1), (".is_file()", 7), ("book().copy", 1), ("book().make_dir", 2), ("book().write", 2), ("fs::", 13), ("remove_file_inside", 1), ("remove_tree_inside", 1), ("thread::Builder", 1)], why: "review evidence is copied on the caller's connection thread and its own copy thread (evidence::prepare, carry_evidence)" },
    Allowed { file: "gi.rs", counts: &[(".exists()", 2), (".metadata()", 1), ("fs::", 5), ("remove_file_inside", 1), ("thread::Builder", 2)], why: "bakes write, trim and read their files on the bake and loader threads; on the engine thread only a bake's small input entry and one stat before a load" },
    Allowed { file: "hlod.rs", counts: &[("thread::Builder", 1)], why: "the HLOD proxy builder's own long-lived worker (a queue of merges, started once)" },
    Allowed { file: "launch_engine.rs", counts: &[(".canonicalize()", 1), (".is_dir()", 3), (".is_file()", 2), ("book().write", 1), ("fs::", 4), ("thread::Builder", 1)], why: "the launcher's engine side (docs/design/LAUNCH.md): a project opened, its lock taken and the engine announced at start-up, taken back at shutdown, in run_with before and after the loop; never a frame. project_locked (a lock asked for, nothing written) is called by the command line's own process, by the watcher thread and inside the start screen's Engine::offload closures. Its one thread (started once) looks for a stop request, and for a project handed to a start screen (whose folder it checks), a few times a second" },
    Allowed { file: "launch_instances.rs", counts: &[(".exists()", 3), (".is_file()", 1), ("fs::", 6)], why: "running engines' instance files: written at start-up, removed at shutdown, and read by the command line's own process (specula list / open / call), which is never an engine; a stop request and a hand-over are read by the engine's watcher thread (launch_engine.rs), never the engine thread" },
    Allowed { file: "launch_run.rs", counts: &[(".is_dir()", 5), (".is_file()", 1), ("book().make_dir", 1), ("fs::", 2)], why: "the command line's commands (specula projects, open, new, call...): they run in a short process of their own that answers and exits; no engine thread exists there (`specula sdk` looks at the folder it writes the SDK into)" },
    Allowed { file: "launch_store.rs", counts: &[(".canonicalize()", 3), (".exists()", 1), (".is_file()", 1), ("book().make_dir", 1), ("book().write", 1), ("fs::", 7), ("remove_file_inside", 1)], why: "the project list and markers on disk: read and written by the command line's own process, and by an engine only at start-up (the project opened) and shutdown (its last world), in run_with outside the loop" },
    Allowed { file: "launch_window.rs", counts: &[(".is_dir()", 3)], why: "the start screen's disk work: the listed folders checked when the screen opens (start-up, before the first frame) and, for everything a press asks for, inside the work closures given to Engine::offload (JobKind::Launch)" },
    Allowed { file: "leases.rs", counts: &[("book().make_dir", 1)], why: "a scratch lease's folder, made when it is taken (one folder)" },
    Allowed { file: "lex_tools.rs", counts: &[("book().write", 1), ("fs::", 1)], why: "caller-thread tool (CallerThread::Lex): lex_import reads and lex_export writes on the agent's connection" },
    Allowed { file: "lib.rs", counts: &[(".exists()", 1), (".is_dir()", 1), ("book().write", 1), ("fs::", 2), ("persist::execute", 1), ("persist::load", 1), ("remove_tree_inside", 1)], why: "start-up and shutdown (the --world load, the pipeline cache, old capture runs pruned); the autosave's execute runs inside its world-lane job" },
    Allowed { file: "logic/tools.rs", counts: &[(".exists()", 1), (".is_dir()", 3), (".is_file()", 1), ("fs::", 6), ("remove_file_inside", 1), ("remove_tree_inside", 2)], why: "caller-thread tools (logic_build, logic_reload run on the agent's connection; drop_stale after a build, for both of Cargo's layouts; a module's manifest read for its package name, to know how deep its build will be)" },
    Allowed { file: "luts.rs", counts: &[(".canonicalize()", 2), (".is_file()", 2), ("fs::", 5)], why: "a grading LUT file the world names (at most 16 MiB, read once per change of name or file stamp, cached)" },
    Allowed { file: "module_sounds.rs", counts: &[(".is_file()", 1), ("fs::", 2)], why: "a module sound's recipe found and read (at most 64 KiB) inside its render's worker job (Offload::engine(ModuleSound))" },
    Allowed { file: "models.rs", counts: &[(".canonicalize()", 9), (".is_dir()", 1), (".is_file()", 6), (".metadata()", 1), ("fs::", 12), ("thread::Builder", 1)], why: "recipe and glTF reads on the model build workers; the asset poll's scan runs on a worker (ScanPlan::run, M5; there too the read of an anim set whose time is too fresh to compare with, which model_reload's own poll makes where it runs, as it does every read of its scan); a build request's read runs on its caller's thread or a worker, never under the library's lock (RequestPlan::run: whether a folder leads outside the root); resolves of folders not cached yet and model_reload's synchronous poll" },
    Allowed { file: "perf.rs", counts: &[("book().write", 1), ("fs::", 1)], why: "perf_capture files are written by write_capture on a worker (Engine::pump_perf_writes); perf.toml is read at start-up" },
    Allowed { file: "persist.rs", counts: &[(".is_file()", 1), ("fs::", 18), ("peek_entity_count", 1), ("sync_all", 1)], why: "world files: written by persist::execute on the world lane (autosave, world_save); loads are world_load's, a rare lead-gated call" },
    Allowed { file: "playtest.rs", counts: &[(".canonicalize()", 4), (".exists()", 1), ("book().make_dir", 1), ("fs::", 2)], why: "a playtest spec file read and its record folder checked when it starts (at most 2 MiB, playtest::MAX_FILE_BYTES)" },
    Allowed { file: "playtest_engine.rs", counts: &[(".canonicalize()", 1), (".exists()", 1), ("book().write", 1)], why: "the replay's path checked when the playtest starts; its file written on a worker (Host::write_replay)" },
    Allowed { file: "procgen_tools.rs", counts: &[(".canonicalize()", 2), (".is_file()", 2), ("book().write", 1), ("fs::", 3), ("png::encode", 2)], why: "caller-thread tool (CallerThread::Procgen): bakes and their PNGs on the agent's connection" },
    Allowed { file: "project.rs", counts: &[(".is_dir()", 2), (".is_file()", 2), ("book().copy", 1), ("fs::", 12), ("sync_all", 1), ("thread::Builder", 1)], why: "the project mirror: SyncPlan::execute runs on the project lane, the watcher on its own thread; refresh's one-file stat and read before a commit; a mount's scan (rare)" },
    Allowed { file: "project_engine.rs", counts: &[(".exists()", 1), ("fs::", 1)], why: "project_mount reads the module's Cargo.toml and looks for a build.rs (one small file, a rare call)" },
    Allowed { file: "project_kinds.rs", counts: &[("fs::", 1)], why: "validation on the caller's connection thread (project_tools)" },
    Allowed { file: "project_tools.rs", counts: &[("fs::", 1)], why: "caller-thread tool (CallerThread::Project)" },
    Allowed { file: "records.rs", counts: &[("fs::", 2)], why: "records::verify runs on the caller's connection thread (evidence::prepare, review_submit): it digests the evidence copies of recorded captures (a stat and a read each)" },
    Allowed { file: "review_engine.rs", counts: &[(".exists()", 2), ("fs::", 5), ("sync_all", 1)], why: "review.json written with the board on its write thread; an unreadable one moved aside when a world loads (rare)" },
    Allowed { file: "review_screen.rs", counts: &[("fs::", 2), ("thread::Builder", 1)], why: "the review screen decodes its capture on its own image thread" },
    Allowed { file: "safe_path.rs", counts: &[(".canonicalize()", 6), (".is_file()", 3), ("fs::", 7)], why: "path confinement checks (canonicalize, metadata) of paths agents name: a few stats per call" },
    Allowed { file: "timeline_audio.rs", counts: &[(".is_file()", 1), ("fs::", 2)], why: "a cue's sound recipe is found, read and rendered in `render`, which only runs on a worker (Engine::offload, JobKind::TimelineSound)" },
    Allowed { file: "user_settings.rs", counts: &[(".is_file()", 1), ("fs::", 3)], why: "the player's settings file is read once at start-up, before the game starts and the loop runs (at most 64 KiB, size checked first); writes (synced, through the disk book) and the rename run in `write` on a worker (JobKind::SettingsWrite), and once more at shutdown after the workers drained (finish_settings)" },
    Allowed { file: "shell.rs", counts: &[(".is_file()", 2), ("fs::", 3)], why: "the project shell's own files (the game file, a theme, screens and string tables: a bounded count, each at most 512 KiB, its size read first) are read once at start-up, before the game starts and the loop runs. A later change to one reaches the shell as the project store's text (Game::asset_texts, Game::game_file_changed): nothing is read from the disk after the start" },
    Allowed { file: "run_modules.rs", counts: &[("thread::Builder", 1)], why: "the start-up build's own thread (--run; started once, before the loop): it is a caller of the engine, as an agent's connection thread is, and waits for the loop, asks the gate, runs Cargo and hands over the library from there; a worker of the pool would hold a slot for a whole Cargo build" },
    Allowed { file: "scaffold.rs", counts: &[(".exists()", 1)], why: "a new project's files are written by the command line's own process (specula new), never an engine's, and from the start screen on a worker (launch_window offloads the whole creation): the engine thread never runs this" },
    Allowed { file: "save_tools.rs", counts: &[(".exists()", 3), (".metadata()", 2), ("fs::", 13), ("persist::load", 1), ("sync_all", 1), ("thread::Builder", 3)], why: "save games are written and read on their own worker threads (save_tools::Writing, Loading); a stat or two when a save starts" },
    Allowed { file: "studio.rs", counts: &[(".is_file()", 3), ("book().write", 2), ("fs::", 11), ("png::encode", 1), ("read_rgba8", 1)], why: "the studio's sheets are made, encoded and written on the caller's connection thread; one render's readback per budgeted studio step; a recorded check's style files are re-read there (Shared::current_inputs)" },
    Allowed { file: "swarm_engine.rs", counts: &[(".exists()", 2), ("book().write", 1), ("fs::", 5), ("sync_all", 1), ("thread::Builder", 1)], why: "board.json written on the board's write thread (and inside world_save's and world_load's worker jobs); desync_trace's file inside its worker job; a stat for the board's folder; the board and review files read at start-up (world_load reads them on its worker)" },
    Allowed { file: "taste.rs", counts: &[(".is_file()", 1), ("fs::", 1), ("thread::Builder", 1)], why: "the gallery worker (plans and renders rounds through the studio's queue; started once); a round's sheet packet is read back on the worker pool (Engine::taste_load_sheet, JobKind::SheetRead)" },
    Allowed { file: "taste_tools.rs", counts: &[(".is_file()", 1), ("fs::", 8)], why: "caller-thread tools (CallerThread::Taste): reference boards' images and captures are read on the agent's connection" },
    Allowed { file: "studio/gallery.rs", counts: &[("book().write", 2), ("fs::", 3), ("png::encode", 1), ("read_rgba8", 1)], why: "a round's sheet is made, encoded and written on the gallery worker's thread (render_round); one render's readback per budgeted studio step, as studio.rs" },
    Allowed { file: "gallery_screen.rs", counts: &[("fs::", 1), ("thread::Builder", 1)], why: "the gallery screen decodes its sheet on its own image thread, as the review screen does" },
    Allowed { file: "terrain.rs", counts: &[("thread::Builder", 1)], why: "the terrain texel worker (started once)" },
    Allowed { file: "text_window.rs", counts: &[("thread::Builder", 1)], why: "the system clipboard's own thread (a delayed-rendering owner can block a read indefinitely), started once; the window's paste and cut wait for its answer through a jobs::Reply" },
    Allowed { file: "text_world.rs", counts: &[(".is_file()", 1), ("thread::Builder", 2)], why: "a CJK fallback font (10-20 MB) loads on its own thread (which also checks its candidate files exist), at most once per family; world files' texts are laid out on the text load thread; never on the engine thread" },
    Allowed { file: "trace_sink.rs", counts: &[(".exists()", 2), ("book().append", 1), ("book().make_dir", 1), ("book().write", 1), ("fs::", 3), ("remove_file_inside", 2), ("thread::Builder", 1)], why: "the trace's file sink thread and crash reports" },
    Allowed { file: "trace_tools.rs", counts: &[("fs::", 1)], why: "caller-thread tool (CallerThread::Trace)" },
    Allowed { file: "two_d.rs", counts: &[("fs::", 4), ("thread::Builder", 1)], why: "sprite and tileset files load on the 2D loader thread" },
    Allowed { file: "ui.rs", counts: &[("thread::Builder", 1)], why: "the window's own thread (Win32 message loop)" },
    Allowed { file: "vfx.rs", counts: &[("fs::", 4), ("thread::Builder", 1)], why: "effect textures load on the vfx loader thread" },
    Allowed { file: "timeline_tools.rs", counts: &[("book().write", 1)], why: "the timeline preview's packet, written inside offload_capture's worker job" },
    Allowed { file: "vfx_tools.rs",counts: &[("book().write", 2), ("png::encode", 1), ("read_rgba8", 1)], why: "a preview's sheet is encoded and written on the worker pool (PreviewEncode::finish); one frame's readback per studio step" },
    Allowed { file: "workers.rs", counts: &[("remove_tree_inside", 1), ("thread::Builder", 1)], why: "the worker pool itself, and the trash emptied inside its trash-lane job" },
    Allowed { file: "world_load.rs", counts: &[("fs::", 2), ("persist::load", 1)], why: "the world, its board and review queue read inside world_load's world-lane worker job" },
    Allowed { file: "world_save.rs", counts: &[(".exists()", 2), ("peek_entity_count", 1), ("persist::execute", 1)], why: "the write, the world_exists check and its count run inside the world-lane job; one stat when a save under another name asks for the lead" },
];

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
fn engine_code(root: &Path) -> Vec<(String, Vec<(usize, String)>)> {
    let root = root.to_path_buf();
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    let rel = |f: &Path| f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
    let texts: Vec<(String, String)> = files.iter().map(|f| (rel(f), std::fs::read_to_string(f).unwrap())).collect();
    let tests = test_only_files(&texts);
    texts.iter().filter(|(r, _)| !tests.iter().any(|t| r == t || r.starts_with(&format!("{}/", t.trim_end_matches(".rs"))))).map(|(r, t)| (r.clone(), non_test_code(t))).collect()
}

/// The files only tests compile: those a `#[cfg(test)] mod name;` declares (review round 3:
/// files were skipped by name, so a `sneaky_tests.rs` compiled into the engine went unscanned,
/// and a test module in a file not named `tests.rs` was scanned).
fn test_only_files(texts: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for (rel, text) in texts {
        let code = code_only(&text.replace("\r\n", "\n"));
        // The folder a file's `mod x;` declarations live in.
        let (dir, file) = rel.rsplit_once('/').map_or(("", rel.as_str()), |(d, f)| (d, f));
        let own = if matches!(file, "lib.rs" | "main.rs" | "mod.rs") { dir.to_owned() } else { format!("{}{}", if dir.is_empty() { String::new() } else { format!("{dir}/") }, file.trim_end_matches(".rs")) };
        for (i, line) in code.iter().enumerate() {
            let t = line.trim();
            let decl = match t.strip_prefix("#[cfg(test)]") {
                Some(rest) if !rest.trim().is_empty() => rest.trim().to_owned(),
                Some(_) => code.get(i + 1).map(|n| n.trim().to_owned()).unwrap_or_default(),
                None => continue,
            };
            let decl = decl.strip_prefix("pub(crate) ").or_else(|| decl.strip_prefix("pub ")).unwrap_or(&decl).to_owned();
            if let Some(name) = decl.strip_prefix("mod ").and_then(|n| n.strip_suffix(';')) {
                let base = if own.is_empty() { name.trim().to_owned() } else { format!("{own}/{}", name.trim()) };
                out.push(format!("{base}.rs"));
                out.push(format!("{base}/mod.rs"));
            }
        }
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


/// One file's code with all whitespace removed, joined across lines, and where each line starts
/// in it (so a match spanning lines, `std::fs::\n write(`, is still seen and reported).
fn squeezed(lines: &[(usize, String)]) -> (String, Vec<(usize, usize)>) {
    let mut text = String::new();
    let mut starts = Vec::with_capacity(lines.len());
    for (n, l) in lines {
        starts.push((text.len(), *n));
        text.extend(l.chars().filter(|c| !c.is_whitespace()));
    }
    (text, starts)
}



/// `(file, pattern) -> count` in the engine crate's non-test code.
fn counts() -> BTreeMap<(String, &'static str), usize> {
    let engine = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = BTreeMap::new();
    for (file, lines) in engine_code(&engine) {
        let (text, _) = squeezed(&lines);
        let text = text.replace("::std::", "std::");
        for (p, _) in PATTERNS {
            let n = text.matches(p).count();
            if n > 0 {
                out.insert((file.clone(), *p), n);
            }
        }
    }
    out
}

/// Imports hiding file-system calls: `(file, import)`.
fn hidden() -> Vec<(String, &'static str)> {
    let engine = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    for (file, lines) in engine_code(&engine) {
        let (text, _) = squeezed(&lines);
        let text = text.replace("::std::", "std::").replace("use::std", "usestd");
        for (p, why) in HIDDEN {
            if text.contains(p) {
                out.push((file.clone(), *why));
            }
        }
    }
    out
}

#[test]
fn no_imports_hide_file_system_calls() {
    let found = hidden();
    assert!(found.is_empty(), "imports that hide file-system calls from the engine-thread guard: {found:?}");
}

#[test]
fn engine_thread_code_never_blocks() {
    let found = counts();
    let mut problems = Vec::new();
    for ((file, p), n) in &found {
        let why = PATTERNS.iter().find(|(q, _)| q == p).map_or("", |(_, w)| w);
        match ALLOWED.iter().find(|a| a.file == file.as_str()) {
            None => problems.push(format!("{file}: {n} x `{p}` ({why}) in a file with no allowance: offload it (Engine::offload) or step it (Engine::step_job), or add the file with its reason")),
            Some(a) => {
                let allowed = a.counts.iter().find(|(q, _)| q == p).map_or(0, |(_, c)| *c);
                if *n > allowed {
                    problems.push(format!("{file}: {n} x `{p}` ({why}), {allowed} allowed ({}): a new blocking call; offload or step it", a.why));
                }
            }
        }
    }
    // A count that shrank (or a file that left) asks for the lower number: the list only shrinks.
    for a in ALLOWED {
        assert!(!a.why.is_empty(), "{} needs its reason", a.file);
        for (p, c) in a.counts {
            let n = found.get(&(a.file.to_owned(), *p)).copied().unwrap_or(0);
            if n < *c {
                problems.push(format!("{}: `{p}` is down to {n} (allowed {c}): lower its allowance", a.file));
            }
        }
    }
    if !problems.is_empty() {
        let mut listing = String::new();
        let mut by_file: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for ((f, p), n) in &found {
            by_file.entry(f.as_str()).or_default().push(format!("(\"{p}\", {n})"));
        }
        for (f, c) in by_file {
            listing.push_str(&format!("    {f}: {}\n", c.join(", ")));
        }
        panic!("engine-thread blocking calls:\n{}\n\nfound now:\n{listing}", problems.join("\n"));
    }
}

/// The scanner sees calls through spacing and across lines, and skips comments, literals and
/// tests.
#[test]
fn the_scanner_sees_blocking_calls_and_skips_tests() {
    let text = "fn a() {\n    // std::fs::write(p, b)\n    let s = \"png::encode_rgba8(\";\n    std :: thread :: Builder :: new();\n    let b = png ::\n encode_rgba8(1, 1, &[], l);\n}\n#[cfg(test)]\nmod tests {\n    fn t() { std::fs::remove_dir_all(p).unwrap(); }\n}\n";
    let lines = non_test_code(text);
    let (code, _) = squeezed(&lines);
    assert_eq!(code.matches("thread::Builder").count(), 1);
    assert_eq!(code.matches("png::encode").count(), 1);
    assert_eq!(code.matches("fs::").count(), 0, "{code}");
    assert_eq!(code.matches("remove_dir_all").count(), 0);
    // Disk work through helpers and stats are seen too.
    let text = "fn b() {\n    crate::disk::book()\n        .write(cat, \"x\", &p, &b, need);\n    if dir.join(\"world.json\").exists() {}\n}\n";
    let (seen, _) = squeezed(&non_test_code(text));
    assert_eq!(seen.matches("book().write").count(), 1, "{seen}");
    assert_eq!(seen.matches(".exists()").count(), 1);
    for (alias, hit) in [("use std::fs as f;", "usestd::fsas"), ("use std::fs::{read, write};", "usestd::fs::"), ("use std::{fs, io};", "usestd::{fs")] {
        let (s, _) = squeezed(&non_test_code(alias));
        assert!(s.contains(hit), "{alias}");
    }
}
