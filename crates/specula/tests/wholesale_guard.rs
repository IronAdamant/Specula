//! One hook for the old world's entity ids (review R1 of the editor lane: a drag that spanned an
//! agent's `world_load` wrote to whichever entity had its ids in the loaded world).
//!
//! When the live world is replaced wholesale (`world_load`, `save_read`, a writer's bug the write
//! policy recovered from), entity ids restart: anything engine-side that still holds one names a
//! different entity. Every path goes through `Engine::world_changed_wholesale`, and this test
//! holds **every `Engine` field** to one of four answers:
//!
//! - `Reset(token)`: the hook resets it (`token` must appear in the hook's body);
//! - `Keyed(why)`: it is keyed on `world_generation` (which the hook bumps), so old entries are
//!   never read as the new world's;
//! - `NoIds(why)`: it holds no entity ids that outlive a frame or the call that made them;
//! - `Identity(why)`: it holds entities by identity (`entity_ref::EntityRef`: the lineage an
//!   entity was born in and its id), which attach only to that very entity in any world and
//!   detach otherwise, so a replacement needs nothing (docs/design/EDITOR.md).
//!
//! A new field fails here until it is classified: decide, and reset it in the hook if it holds ids.

use std::path::Path;

enum Class {
    Reset(&'static str),
    Keyed(&'static str),
    NoIds(&'static str),
    Identity(&'static str),
}
use Class::*;

const FIELDS: &[(&str, Class)] = &[
    ("gpu", NoIds("the device")),
    ("meshes", NoIds("shape meshes, by shape")),
    ("lathes", NoIds("meshes by lathe profile")),
    ("library", NoIds("recipes and builds")),
    ("models", Keyed("its sync key and the anim env include world_generation; per-entity frames are re-checked by build")),
    ("lod_window", NoIds("counts")),
    ("lod_capture", NoIds("counts")),
    ("draws", NoIds("the frame's draw list, rebuilt each frame")),
    ("draw_cache", Keyed("its key includes world_generation; a new world carries no LOD levels over")),
    ("lights", NoIds("the frame's lights")),
    ("two_d", NoIds("tile cells compare their stored grid: a reused id only uploads again")),
    ("draws_2d", NoIds("the frame's 2D draws")),
    ("last_2d_gc", NoIds("a time and a revision")),
    ("windowed", NoIds("a flag")),
    ("build_waiters", NoIds("build job ids and model specs")),
    ("capture_waiters", NoIds("jobs whose views are worked out when they start")),
    ("captures", Keyed("views_hint is keyed on world_generation; a view in flight answers from the world it rendered; the editor's pick is dropped by its reset")),
    ("subject_cache", Keyed("ready/refresh/warm compare world_generation")),
    ("watch", Keyed("(world_generation, revision)")),
    ("cameras", Reset("self.cameras")),
    ("marks", Keyed("each mark records world_generation: a diff across a replacement matches no ids")),
    ("studio", NoIds("build ids and pins")),
    ("labeler", NoIds("the font atlas")),
    ("last_gc", NoIds("a time and a mesh epoch")),
    ("last_unwanted", NoIds("a time and a count")),
    ("world", NoIds("the world itself, replaced")),
    ("luts", NoIds("grading LUTs")),
    ("window_camera", NoIds("a camera pose")),
    ("window_follow", NoIds("agent and camera names, resolved every frame")),
    ("window_released_by", NoIds("a reason")),
    ("targets", NoIds("offscreen targets by size")),
    ("frames_captured", NoIds("a count")),
    ("capture_dir", NoIds("a folder")),
    ("worlds_dir", NoIds("a folder")),
    ("saves_dir", NoIds("a folder")),
    ("save_writes", NoIds("worker-side snapshots")),
    ("save_load", NoIds("the save being read: it is what replaces the world")),
    ("game_save_retry", NoIds("game save requests")),
    ("game_save_notes", NoIds("game save requests")),
    ("world_name", NoIds("set by the load paths")),
    ("saved_revision", NoIds("set by the load paths")),
    ("saved_seq", NoIds("a journal id, set by replace_world")),
    ("checkpoint_seq", NoIds("a journal id, set by replace_world")),
    ("last_autosave", NoIds("a time")),
    ("saved", NoIds("world files by path; the load paths replace it")),
    ("world_lock", NoIds("a folder lock")),
    ("autosave_job", NoIds("a worker's save of a revision")),
    ("autosave_error", NoIds("a message")),
    ("leftovers", NoIds("file names")),
    ("clock", NoIds("the tick")),
    ("game", NoIds("the hosted game's own state: save_read restores it with the world")),
    ("game_name", NoIds("a name")),
    ("pacing", NoIds("timings")),
    ("schedule", NoIds("timings")),
    ("profiler", NoIds("CPU zones")),
    ("perf_cfg", NoIds("budgets")),
    ("last_advance", NoIds("a time")),
    ("input", NoIds("the frame's window events")),
    ("game_title", NoIds("a title")),
    ("game_quads", NoIds("the frame's interface quads")),
    ("game_overlay_stats", NoIds("counts")),
    // Board notes, critiques and tasks' `entities` hold entities by identity (`EntityRef`);
    // their last known paths are remembered once in `replace_world`, before the swap.
    ("swarm", Identity("notes, critiques and task entities are EntityRefs; claims and intents hold paths and regions")),
    ("tick_hashes", Reset("self.tick_hashes")),
    ("hash_cache", Reset("self.hash_cache")),
    ("world_generation", Reset("self.world_generation += 1")),
    ("last_claim_sweep", NoIds("a time")),
    ("trace", Reset("self.trace")),
    ("traces_written", NoIds("a count")),
    ("board_io", NoIds("the board's writer")),
    ("frame_floor", NoIds("a setting")),
    ("max_fps", NoIds("a setting")),
    // Modules get entity ids through the SDK and may keep them in their state store across a
    // world_load. That memory is outside the engine's reach (it can't tell an id from any other
    // value), so modules re-find entities by path (`Cx::find`); save_read restores the store with
    // the world it was saved with, ids and all. A world-replaced event for modules would be a
    // versioned ABI change: not taken here (review R2 of the editor lane).
    ("logic", NoIds("engine side, none (the mover keys on world_generation); module memory is outside the engine's reach: modules re-find by path")),
    ("fp", NoIds("floating-point environment log")),
    ("physics", Reset("self.physics.reset()")),
    ("nav", Reset("self.nav.reset()")),
    ("lex_live", Reset("self.lex_live.stop_all()")),
    ("lex_natives", NoIds("the game's Lex hooks")),
    ("stream", Reset("self.stream.world_replaced")),
    ("terrain", Keyed("its index key includes world_generation; what it draws is rebuilt each frame")),
    ("fly", NoIds("a camera path")),
    ("clock_set_by", NoIds("a name")),
    ("world_folders", NoIds("folder names")),
    ("commits", NoIds("commit counts per call")),
    ("multi_commit_calls", NoIds("a count")),
    ("dispatch_call", NoIds("a call id")),
    ("playtests", NoIds("a load is refused while one runs; reports hold no live ids")),
    ("vfx", Reset("self.vfx.reset()")),
    ("gi", Reset("self.gi.reset()")),
    ("flash", NoIds("image measures")),
    ("flash_epoch", NoIds("a time")),
    ("formats", NoIds("response formats")),
    ("scopes", NoIds("tool scopes")),
    ("project", NoIds("project files by path")),
    ("project_retry", NoIds("a time")),
    ("content", NoIds("file bytes by hash")),
    ("disk_seen", NoIds("revisions")),
    ("disk_last", NoIds("a time")),
    ("evidence", NoIds("paths; re-adopted on load")),
    ("review_ui", Reset("self.review_ui.world_replaced")),
    // The uitools lane: the displays the interface lays out for, agents' display overrides and
    // the camera of the game's last synthetic frame; ui_tree and ui_frame read layouts per call.
    ("ui_state", NoIds("displays, sessions' display overrides and a camera pose")),
    ("editor", Reset("self.editor_world_replaced()")),
    ("editor_owners", NoIds("the frame's draw owners (a reused buffer)")),
    ("taste", NoIds("gallery ids are names")),
    ("modal_open", NoIds("a flag")),
    ("game_pads", NoIds("the frame's pad states")),
    ("pad_gate", NoIds("button masks")),
    ("workers", NoIds("the worker pool")),
    ("world_save_running", NoIds("a flag")),
    ("save_planning", Keyed("a plan checks world_generation before it is written")),
    ("waiting_saves", NoIds("flushed before world_load and save_read replace the world")),
    ("project_syncing", NoIds("a flag")),
    // The model library's disk scan: recipe files by path.
    ("assets_scan", NoIds("a flag for the library's disk scan on a worker")),
    // Timelines and camera rails name entities (rails, look-ats, tracks) and may drive the
    // window: the hook ends every playback ("world_replaced") and drops the parsed cache. Their
    // preview and check jobs name entities by path and resolve them per step.
    ("timeline", Reset("self.timeline_world_replaced()")),
    ("project_deferred", NoIds("project calls, by path")),
    ("project_waiters", NoIds("project calls, by path")),
    ("steps", Keyed("a job that holds an entity id records world_generation and stops (changed_during_job)")),
];

fn engine_fields(lib: &str) -> Vec<String> {
    let start = lib.find("\nstruct Engine {\n").expect("struct Engine in lib.rs");
    let body = &lib[start + 1..];
    let end = body.find("\n}\n").expect("end of struct Engine");
    body[..end]
        .lines()
        .skip(1)
        .filter_map(|l| {
            let l = l.strip_prefix("    ")?;
            if l.starts_with(' ') || l.starts_with("//") || l.starts_with('#') {
                return None;
            }
            let l = l.strip_prefix("pub(crate) ").or_else(|| l.strip_prefix("pub ")).unwrap_or(l);
            let (name, _) = l.split_once(':')?;
            name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_').then(|| name.to_owned())
        })
        .collect()
}

fn hook_body(lib: &str) -> &str {
    let start = lib.find("    fn world_changed_wholesale(&mut self) {").expect("the hook in lib.rs");
    let body = &lib[start..];
    let end = body.find("\n    }\n").expect("end of the hook");
    &body[..end]
}

#[test]
fn every_engine_field_is_reset_keyed_or_holds_no_ids() {
    // Line endings as checked out (a Windows checkout may have CRLF).
    let lib = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs")).unwrap().replace("\r\n", "\n");
    let fields = engine_fields(&lib);
    assert!(fields.len() > 100, "parsed {} fields: the parser lost the struct", fields.len());
    let hook = hook_body(&lib);
    let mut problems = Vec::new();
    for f in &fields {
        match FIELDS.iter().filter(|(n, _)| n == f).count() {
            0 => problems.push(format!(
                "Engine.{f} is new: classify it in tests/wholesale_guard.rs (reset it in world_changed_wholesale if it holds entity ids)"
            )),
            1 => {}
            n => problems.push(format!("Engine.{f} is listed {n} times")),
        }
    }
    for (name, class) in FIELDS {
        if !fields.iter().any(|f| f == name) {
            problems.push(format!("{name} is no Engine field any more: drop it from the table"));
        }
        match class {
            Reset(token) if !hook.contains(token) => problems.push(format!("{name} is said to be reset, but world_changed_wholesale has no `{token}`")),
            Keyed(why) | NoIds(why) | Reset(why) | Identity(why) if why.is_empty() => problems.push(format!("{name}: say why")),
            _ => {}
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Every wholesale replacement reaches the hook: the load paths through `replace_world`, the
/// recovery path through `check_world_replaced`.
#[test]
fn every_replacement_goes_through_the_hook() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let lib = std::fs::read_to_string(src.join("lib.rs")).unwrap().replace("\r\n", "\n");
    let replace = &lib[lib.find("    fn replace_world(").expect("replace_world")..];
    assert!(replace[..replace.find("\n    }\n").unwrap()].contains("self.world_changed_wholesale();"));
    let check = &lib[lib.find("    fn check_world_replaced(").expect("check_world_replaced")..];
    assert!(check[..check.find("\n    }\n").unwrap()].contains("self.world_changed_wholesale();"));
    // Nothing else swaps the live world: `.replace(may, ...)` is called from replace_world only.
    let mut callers = Vec::new();
    for entry in walk(&src) {
        let text = std::fs::read_to_string(&entry).unwrap();
        for (i, line) in text.lines().enumerate() {
            if line.contains("self.world.replace(") {
                callers.push(format!("{}:{}", entry.display(), i + 1));
            }
        }
    }
    assert_eq!(callers.len(), 1, "the live world is replaced only in replace_world: {callers:?}");
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
    out
}
