//! Scoped tool surfaces: each session lists only the tools its work needs.
//!
//! Every tool definition an agent's `tools/list` shows is context it pays for before its first
//! call (all of them: about 200 KB, some 50,000 tokens), and a swarm pays it once per agent. So a
//! session lists the **core** ([`CORE`]: `agent_hello` and the `tools_*` catalog tools) plus its
//! **scope**. The model (owner, 2026-10-29): the lead agent knows the engine and picks each
//! subagent's tools, or tells it to activate only what its task needs; subagents see names and
//! one-line descriptions (`tools_catalog`) until they activate something; a missing tool becomes a
//! request to the lead (`tools_request`), who builds or composes one.
//!
//! - **Families** ([`FAMILIES`]) group tools by name prefix (`world_`, `sight_`, ...), with a few
//!   tools placed by name (the journal's `world_history` etc.). A new tool joins its family by its
//!   prefix; one with a new prefix fails `every_tool_has_a_family_and_a_brief` until the table
//!   names it. The hosted game's tools are the family `game`.
//! - **Role templates** ([`ROLES`]) are data: which families (and single tools) a builder,
//!   reviewer, playtester, ... gets.
//! - A session's scope: its **active** set (what it lists beyond the core) and, optionally, a
//!   **cap** the lead set (the most it may list *or call*). Calls to any tool within the cap work
//!   whether it is listed or not (so clients that never re-read their list, and scripts, keep
//!   working); calls beyond the cap are refused with `not_in_scope`.
//! - Where it comes from: the engine's default (`--tools-default core|all`, core unless set); the
//!   connect URL (`/mcp?tools=world,sight&role=builder`), so a client gets its list on its first
//!   `tools/list`; `agent_hello {tools}` (replaces the active set); `agent_hello {role}` naming a
//!   template, but only while nothing explicit chose (the default, or an earlier role); and the
//!   lead's `tools_scope` for the agent's name, which wins over all of them (at its hello, the
//!   asked tools are added within the lead's cap). `tools_activate` and `tools_deactivate`
//!   change the active set at runtime, within the cap.
//!
//! **Not a security boundary yet.** Without per-agent auth tokens (M6) any client can open a new
//! session with `?tools=all`, and anyone can say they are the lead. The cap is kept by agent
//! name so that an M6 token can carry it instead: the token, not the name, will say which cap a
//! session gets. See `docs/design/TOOLS.md`.
//!
//! All of this lives on the protocol threads (tools/list must never wait for the engine thread);
//! nothing here touches the simulation.

use specula_core::Json;
use specula_protocol::ToolDef;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Always listed, never deactivated, never outside a cap.
pub const CORE: [&str; 6] = ["agent_hello", "tools_catalog", "tools_activate", "tools_deactivate", "tools_invoke", "tools_request"];

/// The hosted game's tools.
pub const GAME_FAMILY: &str = "game";

pub struct Family {
    pub name: &'static str,
    /// One line for the catalog.
    pub about: &'static str,
    /// Tools whose names start with one of these belong here (unless [`Family::names`] of
    /// another family places them).
    prefixes: &'static [&'static str],
    /// Tools placed here by name (they win over prefixes).
    names: &'static [&'static str],
}

const fn fam(name: &'static str, about: &'static str, prefixes: &'static [&'static str], names: &'static [&'static str]) -> Family {
    Family { name, about, prefixes, names }
}

pub const FAMILIES: &[Family] = &[
    fam("tools", "your tool list: catalog, activate, invoke, ask the lead (and the lead's tools_scope)", &["tools_"], &["agent_hello"]),
    fam("world", "entities: spawn, change, delete, find, place by relations, world files, lighting, tilemaps, text", &["world_", "tilemap_"], &["text_edit"]),
    fam("journal", "who changed what: history, undo, change feed, state hash, roll-ups, desync tracing", &["desync_"], &["world_history", "world_undo", "world_changes", "world_hash", "world_rollup"]),
    fam("sight", "look: frames with screen boxes, turntables, before/after, named cameras, the human's view", &["sight_", "camera_"], &["window_view"]),
    fam("claim", "leases on subtrees and regions so agents don't collide", &["claim_"], &[]),
    fam("board", "the task board: post, take, report, finish; plans for the lead", &["task_"], &[]),
    fam("swarm", "intents, handoff notes, resource leases, the lead's swarm settings", &["intent_", "note_", "lease_"], &["swarm_settings"]),
    fam("engine", "engine status and the simulation clock", &["engine_"], &["time_control"]),
    fam("game", "the hosted game: how its run stands (outcome, score, screens, settings), a press of its buttons by word, and its own tools", &["game_"], &[]),
    fam("perf", "GPU and CPU timings, memory, frame captures", &["perf_"], &[]),
    fam("model", "procedural model recipes and colliders fitted to them (named shapes go in by name: formae_list)", &["model_"], &[]),
    fam("procgen", "texture graphs, terrains, scatters", &["procgen_"], &[]),
    fam("terrain", "drawn terrains: how one is drawn, sculpt and paint its recipe (collider kept in step)", &["terrain_"], &[]),
    fam("physics", "queries, contacts, checks, impulses, joints, ragdolls", &["physics_", "ragdoll_"], &[]),
    fam("nav", "navmeshes, paths, nav agents, AI minds", &["nav_", "ai_"], &[]),
    fam("stream", "what is resident on the GPU, streaming settings, measured flights", &["stream_"], &[]),
    fam("playtest", "headless playtests as data, with replays", &["playtest_"], &[]),
    fam("lex", "gameplay rules as data: edit, validate, simulate, run live", &["lex_"], &[]),
    fam("project", "the game's files: recipes, UI, sounds, styles, module sources, images, models, audio", &["project_"], &[]),
    fam("disk", "disk use, the budget and pruning", &["disk_"], &[]),
    fam("save", "save games", &["save_"], &[]),
    fam("balance", "balance targets, sweeps and tuning over Lex params", &["balance_"], &[]),
    fam("anim", "clips, graphs, motion checks, filmstrips", &["anim_"], &[]),
    fam("studio", "the asset studio: stage, sheets, line-ups, taste and style checks", &["studio_", "style_"], &[]),
    fam("review", "the owner's review queue, critiques, merges", &["review_", "critique_", "merge_"], &[]),
    fam("taste", "galleries the owner picks from, the preference store, reference boards, the style configurator", &["gallery_", "taste_", "board_"], &["style_onboard"]),
    fam("vfx", "particles, effects and decals", &["vfx_"], &[]),
    fam("timeline", "cutscenes as data: camera rails, entity tracks, cues and subtitles; play, scrub, preview, rail checks", &["timeline_"], &[]),
    fam("gi", "baked global illumination: bake probe volumes, check them, the runtime tier", &["gi_"], &[]),
    fam("audio", "hearing: what played, loudness and peaks per bus, offline previews, bus settings", &["audio_"], &[]),
    fam("logic", "game logic modules (Rust): build, reload, inspect, drive", &["logic_"], &[]),
    fam("ui", "the interface as data: widget trees, outlined captures, a player's input, measured checks", &["ui_"], &[]),
    fam("memory", "what the project remembers: notes kept with it, the brief every agent is given, why a file is as it is", &["memory_"], &[]),
    fam("formae", "the shape library: named plane figures, curves and solids to start from by name, and the measured human figure", &["formae_"], &[]),
    fam("l10n", "localisation: string tables per locale, translations and reviews, checks in every language", &["l10n_"], &[]),
    fam("trace", "the structured log: events, one call's causal tree, crash reports, incidents", &["trace_"], &[]),
    fam("editor", "the window's human editor: what it shows, and driving it as the human (owner)", &["editor_"], &[]),
    fam("security", "the input monitor: keyboard, clipboard and screen takeovers, frozen sessions, swarm pauses (lead, owner)", &["security_"], &[]),
];

/// The catalog line of the hosted game's family.
const GAME_ABOUT: &str = "the hosted game's own tools";

pub struct Role {
    pub name: &'static str,
    pub about: &'static str,
    /// Every tool.
    pub all: bool,
    pub families: &'static [&'static str],
    /// Single tools beyond the families.
    pub tools: &'static [&'static str],
}

const fn role(name: &'static str, about: &'static str, families: &'static [&'static str], tools: &'static [&'static str]) -> Role {
    Role { name, about, all: false, families, tools }
}

/// Role templates: what each kind of agent lists. `agent_hello {role}` with one of these names
/// applies it (unless something else chose); the lead hands them out with `tools_scope`.
pub const ROLES: &[Role] = &[
    Role { name: crate::authority::ROLE_LEAD, about: "everything (picks the others' tools)", all: true, families: &[], tools: &[] },
    Role { name: crate::authority::ROLE_DIRECTOR, about: "everything (plans and reviews the swarm)", all: true, families: &[], tools: &[] },
    role("builder", "builds the world", &["world", "sight", "claim", "board", "journal", "model", "engine", "terrain"], &[]),
    role(crate::authority::ROLE_REVIEWER, "checks finished work", &["sight", "journal", "board", "review", "studio"], &["world_query", "world_list"]),
    role("playtester", "plays and records runs", &["game", "playtest", "sight", "ui", "board", "save", "engine", "audio"], &["world_query", "world_hash"]),
    role("art_director", "holds the look", &["sight", "studio", "review", "taste", "model", "procgen", "vfx", "gi", "terrain", "project", "board", "audio"], &["world_env", "world_query"]),
    role("sight_skeptic", "verifies visual claims", &["sight"], &["world_query", "world_list", "critique_add"]),
    role("performance", "measures frame cost and disk use", &["perf", "engine", "stream", "sight", "disk"], &["world_query", "world_list", "gi_info", "gi_settings"]),
    role("designer", "gameplay rules and balance", &["lex", "balance", "playtest", "game", "board", "save"], &["world_query"]),
    role("animator", "motion and ragdolls", &["anim", "physics", "sight", "world", "claim", "board"], &[]),
    role("coder", "game logic modules", &["logic", "project", "world", "sight", "engine", "claim", "board"], &[]),
    // docs/design/DEBUGGER.md: watches incidents, reproduces, reports; no world writes.
    role(
        "debugger",
        "triages incidents and files evidence (never fixes others' work)",
        &["trace", "review", "board", "journal", "sight", "playtest", "perf", "engine"],
        &["world_query", "world_list", "claim_list", "intent_list", "note_add", "note_list"],
    ),
];

/// One line per engine tool for the catalog: what it is for, in a few words. A new tool needs a
/// line here (`every_tool_has_a_family_and_a_brief` fails without one).
pub(crate) const BRIEFS: &[(&str, &str)] = &[
    ("agent_hello", "say who you are (name, role, tools)"),
    ("tools_catalog", "every tool by family or by job (kits), one line each"),
    ("tools_activate", "add tools to your list (returns schemas)"),
    ("tools_deactivate", "remove tools from your list"),
    ("tools_invoke", "call any tool by name (clients that don't refresh)"),
    ("tools_request", "ask the lead for a missing tool"),
    ("tools_scope", "lead: set or cap another agent's tools"),
    ("world_spawn", "add an entity (shape, model, 2D, components)"),
    ("world_set", "change an entity in place"),
    ("world_delete", "remove an entity and its subtree"),
    ("world_list", "whole entities, paged"),
    ("world_query", "find entities by filters, chosen fields"),
    ("world_batch", "many spawn/set/delete ops, all or nothing"),
    ("world_save", "save the world as text files"),
    ("world_load", "replace the world with a saved one"),
    ("world_env", "sun, ambient, sky, fog, post, physics settings"),
    ("world_place", "place by relations (on, along, near...)"),
    ("tilemap_set", "write tilemap cells"),
    ("tilemap_get", "read tilemap cells"),
    ("text_edit", "type into a text entity: caret, selection, IME"),
    ("world_history", "the journal: who changed what"),
    ("world_undo", "undo journal entries"),
    ("world_changes", "change feed since a cursor"),
    ("world_hash", "state hash per tick, replay check"),
    ("world_rollup", "journal summarised per agent or task"),
    ("desync_trace", "write this run's per-tick trace"),
    ("desync_compare", "first tick two runs differ"),
    ("sight_frame", "render: PNG + exact screen boxes, overlays"),
    ("sight_orbit", "turntable contact sheet of one entity"),
    ("sight_compare", "before/after diff of the world"),
    ("sight_query", "one entity's screen facts, no image"),
    ("camera_set", "make a named camera (fixed or following)"),
    ("camera_list", "named cameras"),
    ("camera_delete", "delete named cameras"),
    ("window_view", "point the human's window camera"),
    ("ui_tree", "the interface's widgets: rects, text, focus; paged"),
    ("ui_frame", "capture with every widget outlined and labelled"),
    ("ui_act", "drive the interface: gamepad, keys, mouse, touch"),
    ("ui_check", "legibility, contrast, overflow, gamepad reach per display"),
    ("l10n_get", "string tables and a locale's entries"),
    ("l10n_set", "write, remove or review a locale's strings"),
    ("l10n_check", "missing, stale, placeholders, overflow per language"),
    ("l10n_tasks", "post translate and review tasks per locale"),
    ("memory_note", "keep a note with the project"),
    ("memory_get", "read one note"),
    ("memory_list", "the project's notes, by kind, words, path"),
    ("memory_why", "notes about a file, and who changed it"),
    ("formae_list", "the shape library: every name in one short answer; rows to paste"),
    ("formae_get", "one form: snippets to paste first; measures and anchors on request"),
    ("claim_take", "claim a subtree or region"),
    ("claim_renew", "extend claim leases"),
    ("claim_release", "release claims"),
    ("claim_share", "let another agent write in your claim"),
    ("claim_list", "live claims"),
    ("task_post", "put a task on the board"),
    ("task_take", "take an open task"),
    ("task_update", "progress, blockers, questions, answers"),
    ("task_done", "finish or cancel a task"),
    ("task_list", "the board, filtered and paged"),
    ("task_get", "one task in full"),
    ("task_plan", "lead: a design-doc section into tasks"),
    ("task_progress", "lead: a plan's dashboard"),
    ("intent_declare", "say where you'll work before editing"),
    ("intent_list", "live intents"),
    ("intent_clear", "end intents"),
    ("note_add", "leave a handoff note"),
    ("note_list", "notes on entities, paths, tasks"),
    ("note_delete", "delete a note"),
    ("lease_take", "lease ports, GPU time, scratch folders"),
    ("lease_renew", "extend leases"),
    ("lease_release", "give leases back"),
    ("lease_list", "live leases and free pools"),
    ("swarm_settings", "operator hardware; lead sets frame floor"),
    ("engine_info", "engine, GPU, world, agents, clock"),
    ("game_state", "the run: outcome, score, screens, settings"),
    ("game_act", "press a button of the game by its word"),
    ("time_control", "pause, resume, step, scale the clock"),
    ("perf_frame", "GPU ms per pass, draws, memory, budgets"),
    ("perf_light", "what each light feature costs here, against a budget"),
    ("perf_zones", "CPU time per engine phase"),
    ("perf_memory", "heap use by zone"),
    ("perf_capture", "record frames to a trace file"),
    ("model_build", "build a recipe and report, no spawn"),
    ("model_reload", "re-read recipes and styles now"),
    ("model_collider", "fit a collider to a model"),
    ("model_paint", "paint a model recipe's vertex colours"),
    ("model_path", "redraw a sweep part's path in a model recipe"),
    ("model_export", "write a model recipe as a .glb for other engines"),
    ("procgen_texture", "bake a procedural texture graph"),
    ("procgen_terrain", "build a heightfield terrain"),
    ("procgen_scatter", "Poisson-disc placements in an area"),
    ("physics_raycast", "ray hits on colliders"),
    ("physics_cast", "sweep a shape, first hit"),
    ("physics_overlap", "colliders overlapping a shape"),
    ("physics_contacts", "contact pairs as data"),
    ("physics_events", "recent contact and trigger events"),
    ("physics_info", "physics world at a glance"),
    ("physics_explain", "why an entity moves or rests"),
    ("physics_check", "assert physics outcomes as data"),
    ("physics_impulse", "kick a dynamic body"),
    ("physics_joint", "make, change or remove a joint"),
    ("physics_joints", "joints as data"),
    ("ragdoll_set", "turn a rigged model into a ragdoll"),
    ("ragdoll_info", "a ragdoll's parts and state"),
    ("nav_build", "build a navmesh from the world"),
    ("nav_path", "path on a navmesh"),
    ("nav_point", "nearest or random navmesh points"),
    ("nav_agent", "make an entity walk the navmesh"),
    ("nav_info", "navmeshes and agents at a glance"),
    ("ai_check", "validate an AI definition"),
    ("ai_set", "give an entity a mind (BT or utility)"),
    ("ai_status", "what minds do and why"),
    ("stream_state", "what is resident on the GPU"),
    ("stream_settings", "read or change streaming budgets"),
    ("stream_fly", "fly the window camera, measure frames"),
    ("playtest_run", "run a headless playtest as data"),
    ("playtest_report", "a playtest's report"),
    ("lex_schema", "the Lex language described"),
    ("lex_list", "rule sets and their items"),
    ("lex_get", "Lex items in full"),
    ("lex_edit", "edit a rule set atomically"),
    ("lex_validate", "check a rule set"),
    ("lex_simulate", "run rules headless, deterministic"),
    ("lex_import", "write a whole document into a set"),
    ("lex_export", "a set as one canonical document"),
    ("lex_session", "saved simulation runs"),
    ("lex_live", "run a rule set live in the engine"),
    ("project_list", "the game's text files"),
    ("project_read", "a project file's text and version"),
    ("project_write", "write a whole project file"),
    ("project_patch", "edit a project file with ops"),
    ("project_delete", "delete a project file"),
    ("project_mount", "mirror a module crate's sources"),
    ("project_upload", "upload an image, model or sound (chunked)"),
    ("disk_status", "disk use by category, the budget, free space"),
    ("disk_prune", "prune old captures and cache now"),
    ("trace_query", "search the engine's structured log"),
    ("trace_follow", "new log events since your cursor"),
    ("trace_call", "everything one call caused"),
    ("trace_settings", "log levels, sinks and stall budgets"),
    ("trace_crash", "the last crash report"),
    ("trace_incidents", "open problems the watchdogs found"),
    ("trace_incident", "one incident; acknowledge, resolve or link it"),
    ("save_write", "save the game at this tick"),
    ("save_read", "load a save game"),
    ("save_list", "save games"),
    ("save_delete", "delete a save game"),
    ("balance_targets", "check designer targets as data"),
    ("balance_sweep", "score param points over seeds"),
    ("balance_tune", "tune params toward targets"),
    ("anim_get", "a model's skeleton, clips, graphs"),
    ("anim_set", "write a clip or graph live"),
    ("anim_state", "what an animated entity is doing"),
    ("anim_check", "motion checks as data"),
    ("anim_filmstrip", "contact sheet of poses over time"),
    ("studio_stage", "subjects on the stage with scale refs"),
    ("studio_sheet", "contact sheet: views and passes"),
    ("studio_lineup", "subjects at true relative scale"),
    ("studio_compare", "before/after on the stage"),
    ("studio_check", "the taste loop in one call"),
    ("style_check", "style.toml rules on recipes"),
    ("gallery_create", "show the owner variants to choose from"),
    ("gallery_get", "a gallery: round, picks, sheet"),
    ("gallery_delete", "delete a gallery and its picks"),
    ("gallery_list", "galleries by status"),
    ("gallery_open", "show a gallery in the window"),
    ("gallery_close", "close the window's gallery screen"),
    ("gallery_wait", "wait for the owner's next pick"),
    ("gallery_decide", "pick for the owner (role owner)"),
    ("gallery_round", "your own variants as the next round"),
    ("gallery_apply", "write the accepted values"),
    ("taste_query", "the owner's past picks and notes"),
    ("taste_suggest", "priors suggested by agreeing picks"),
    ("board_measure", "measure a reference board"),
    ("board_check", "a capture against a reference board"),
    ("style_onboard", "the first-run style configurator"),
    ("review_submit", "put work on the review queue"),
    ("review_list", "the review queue"),
    ("review_decide", "approve or reject submissions"),
    ("review_screen", "the window's review screen"),
    ("editor_state", "the window editor: selection, gizmo, panels, status"),
    ("editor_input", "drive the window editor as the human (owner only)"),
    ("security_incidents", "what the input monitor saw; waits for the next (lead)"),
    ("security_clear", "unfreeze sessions, lift a swarm pause (lead)"),
    ("security_allow_lead", "let a lead point the keyboard without alerts (owner)"),
    ("critique_add", "record review findings"),
    ("critique_list", "critiques"),
    ("critique_update", "work a critique through"),
    ("merge_suggest", "three-way merge of JSON data"),
    ("vfx_get", "the effect library"),
    ("vfx_set", "write an effect"),
    ("vfx_preview", "filmstrip of an effect"),
    ("vfx_check", "effect budget and readability checks"),
    ("vfx_info", "what effects cost now"),
    ("timeline_get", "timelines and what plays"),
    ("timeline_set", "write a timeline file"),
    ("timeline_validate", "check a timeline without playing"),
    ("timeline_play", "play a timeline on the tick"),
    ("timeline_pause", "pause or resume a playback"),
    ("timeline_scrub", "move a playback to a time"),
    ("timeline_stop", "stop a playback"),
    ("timeline_preview", "filmstrip from the rail camera"),
    ("timeline_check", "rail checks: framing, clearance, jumps"),
    ("gi_bake", "bake a GI probe volume (off the frame thread)"),
    ("gi_info", "GI volumes, staleness, jobs, residency, tier"),
    ("gi_check", "check a bake; sample its light at points"),
    ("gi_settings", "the GI tier (off/low/high) and residency"),
    ("audio_played", "what played lately: name, bus, who, when"),
    ("audio_levels", "LUFS, peaks, overs, silence per bus; streams"),
    ("audio_preview", "render a sound, piece or file offline; measure"),
    ("audio_settings", "bus volumes and duck rules (lead to change)"),
    ("terrain_info", "how a terrain is drawn; collider check"),
    ("terrain_edit", "sculpt and paint a terrain recipe"),
    ("logic_build", "build and hot-reload a module (lead; others propose)"),
    ("logic_reload", "reload or load a module library"),
    ("logic_unload", "stop and unload a module"),
    ("logic_status", "loaded modules and their state"),
    ("logic_input", "hold a module's input actions"),
    ("logic_hook", "call a module's native hook"),
];

/// Longest a derived brief (a game's tools) may be.
const MAX_BRIEF: usize = 60;

/// Entries one selection may name (families or tools; there are about 160 tools).
pub const MAX_ENTRIES: usize = 256;
/// The longest entry looked at (tool names are at most 64 characters).
pub const MAX_ENTRY_CHARS: usize = 64;
/// Unknown entries an error names one by one (the rest are counted).
const MAX_UNKNOWN_SHOWN: usize = 8;

/// `s`, at most `n` characters (with "..." when cut): text an agent sent, echoed back.
pub fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(n).collect();
        out.push_str("...");
        out
    }
}

/// The family a tool belongs to (`None`: no family names it; the game's tools are [`GAME_FAMILY`]).
pub fn family_of(name: &str) -> Option<&'static str> {
    FAMILIES
        .iter()
        .find(|f| f.names.contains(&name))
        .or_else(|| FAMILIES.iter().filter(|f| f.prefixes.iter().any(|p| name.starts_with(p))).max_by_key(|f| f.prefixes.iter().filter(|p| name.starts_with(*p)).map(|p| p.len()).max()))
        .map(|f| f.name)
}

/// The catalog line of an engine tool (`None` without one).
pub fn brief_of(name: &str) -> Option<&'static str> {
    BRIEFS.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
}

/// A brief from a description: its first clause, at most [`MAX_BRIEF`] characters.
fn derive_brief(description: &str) -> String {
    let first = description.split(['.', ':', ';']).next().unwrap_or("").trim();
    let mut out: String = first.chars().take(MAX_BRIEF).collect();
    if first.chars().count() > MAX_BRIEF {
        out = out.rsplit_once(' ').map_or(out.clone(), |(a, _)| a.to_owned());
        out.push_str("...");
    }
    out
}

/// A set of tools (bits over the catalog's order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolSet(Vec<u64>);

impl ToolSet {
    pub fn empty(n: usize) -> ToolSet {
        ToolSet(vec![0; n.div_ceil(64)])
    }

    pub fn full(n: usize) -> ToolSet {
        let mut s = ToolSet::empty(n);
        (0..n).for_each(|i| s.insert(i));
        s
    }

    /// A set of these indices.
    pub fn from_indices(n: usize, indices: &[usize]) -> ToolSet {
        let mut s = ToolSet::empty(n);
        indices.iter().for_each(|&i| s.insert(i));
        s
    }

    pub fn insert(&mut self, i: usize) {
        self.0[i / 64] |= 1 << (i % 64);
    }

    pub fn contains(&self, i: usize) -> bool {
        self.0.get(i / 64).is_some_and(|w| w & (1 << (i % 64)) != 0)
    }

    pub fn union(&mut self, o: &ToolSet) {
        self.0.iter_mut().zip(&o.0).for_each(|(a, b)| *a |= b);
    }

    pub fn intersect(&mut self, o: &ToolSet) {
        self.0.iter_mut().zip(&o.0).for_each(|(a, b)| *a &= b);
    }

    pub fn minus(&mut self, o: &ToolSet) {
        self.0.iter_mut().zip(&o.0).for_each(|(a, b)| *a &= !b);
    }

    pub fn count(&self) -> usize {
        self.0.iter().map(|w| w.count_ones() as usize).sum()
    }

    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().enumerate().flat_map(|(wi, w)| (0..64).filter(move |b| w & (1 << b) != 0).map(move |b| wi * 64 + b))
    }
}

/// A refusal as data: `{error, message, ...fields}`.
#[derive(Clone, Debug, PartialEq)]
pub struct ScopeError {
    pub code: &'static str,
    pub message: String,
    pub fields: Vec<(&'static str, Json)>,
}

impl ScopeError {
    pub fn new(code: &'static str, message: impl Into<String>) -> ScopeError {
        ScopeError { code, message: message.into(), fields: Vec::new() }
    }

    pub fn with(mut self, key: &'static str, value: impl Into<Json>) -> ScopeError {
        self.fields.push((key, value.into()));
        self
    }

    pub fn to_json(&self) -> Json {
        let mut j = Json::obj([("error", Json::from(self.code)), ("message", Json::from(self.message.as_str()))]);
        for (k, v) in &self.fields {
            j.set(*k, v.clone());
        }
        j
    }
}

/// Every tool, with its family and catalog line (built once: the tool set is fixed for a run).
pub struct Catalog {
    defs: Vec<ToolDef>,
    /// Index into `families` per tool.
    family: Vec<usize>,
    /// (name, about) per family, in [`FAMILIES`] order, then the game's (and `other` for a tool no
    /// family names, which a test forbids).
    families: Vec<(String, String)>,
    briefs: Vec<String>,
    index: BTreeMap<String, usize>,
    core: ToolSet,
    /// Compact `tools/list` bytes of each definition.
    bytes: Vec<usize>,
    /// Tools listed by reference beside a sibling (`toolkits::SHARED`): (tool, sibling, the
    /// tool as it is listed then, its bytes then).
    shared: Vec<(usize, usize, ToolDef, usize)>,
}

impl Catalog {
    /// `defs`: every tool in listing order; `game`: the names of those the hosted game added.
    pub fn new(defs: Vec<ToolDef>, game: &[String]) -> Catalog {
        let mut families: Vec<(String, String)> = FAMILIES.iter().map(|f| (f.name.to_owned(), f.about.to_owned())).collect();
        let mut family_index = |name: &str, about: &str| match families.iter().position(|(n, _)| n == name) {
            Some(i) => i,
            None => {
                families.push((name.to_owned(), about.to_owned()));
                families.len() - 1
            }
        };
        let mut family = Vec::with_capacity(defs.len());
        let mut briefs = Vec::with_capacity(defs.len());
        for d in &defs {
            let f = if game.contains(&d.name) { family_index(GAME_FAMILY, GAME_ABOUT) } else {
                match family_of(&d.name) {
                    Some(f) => family_index(f, ""),
                    None => family_index("other", "tools no family names yet"),
                }
            };
            family.push(f);
            briefs.push(brief_of(&d.name).map_or_else(|| derive_brief(&d.description), str::to_owned));
        }
        let index = defs.iter().enumerate().map(|(i, d)| (d.name.clone(), i)).collect::<BTreeMap<_, _>>();
        let mut core = ToolSet::empty(defs.len());
        CORE.iter().filter_map(|n| index.get(*n)).for_each(|&i| core.insert(i));
        let bytes = defs.iter().map(|d| listed_json(d).to_string().len()).collect();
        let shared = crate::toolkits::SHARED
            .iter()
            .filter_map(|(tool, sibling)| {
                let (t, s) = (*index.get(*tool)?, *index.get(*sibling)?);
                let slim = crate::toolkits::shared_listing(&defs[t], &defs[s])?;
                let n = listed_json(&slim).to_string().len();
                Some((t, s, slim, n))
            })
            .collect();
        Catalog { defs, family, families, briefs, index, core, bytes, shared }
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn defs(&self) -> &[ToolDef] {
        &self.defs
    }

    pub fn index(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }

    pub fn def(&self, i: usize) -> &ToolDef {
        &self.defs[i]
    }

    pub fn name(&self, i: usize) -> &str {
        &self.defs[i].name
    }

    pub fn brief(&self, i: usize) -> &str {
        &self.briefs[i]
    }

    pub fn family_name(&self, i: usize) -> &str {
        &self.families[self.family[i]].0
    }

    /// (name, about) of each family that has tools, in catalog order.
    pub fn families(&self) -> impl Iterator<Item = (usize, &str, &str)> {
        self.families.iter().enumerate().filter(|(f, _)| self.family.contains(f)).map(|(f, (n, a))| (f, n.as_str(), a.as_str()))
    }

    /// Tools of family index `f`.
    pub fn members(&self, f: usize) -> impl Iterator<Item = usize> + '_ {
        (0..self.defs.len()).filter(move |&i| self.family[i] == f)
    }

    pub fn core(&self) -> &ToolSet {
        &self.core
    }

    pub fn empty(&self) -> ToolSet {
        ToolSet::empty(self.len())
    }

    pub fn full(&self) -> ToolSet {
        ToolSet::full(self.len())
    }

    /// The `tools/list` answer's bytes for `set` (compact JSON).
    pub fn list_bytes(&self, set: &ToolSet) -> usize {
        let n = set.count();
        let saved: usize = self.shared.iter().filter(|(t, s, ..)| set.contains(*t) && set.contains(*s)).map(|(t, _, _, slim)| self.bytes[*t].saturating_sub(*slim)).sum();
        r#"{"tools":[]}"#.len() + set.iter().map(|i| self.bytes[i]).sum::<usize>() + n.saturating_sub(1) - saved
    }

    /// The definitions of `set` as `tools/list` shows them: a tool whose sibling is listed too
    /// points at the arguments they share instead of repeating them (`toolkits::SHARED`).
    pub fn listing(&self, set: &ToolSet) -> Vec<ToolDef> {
        set.iter()
            .map(|i| match self.shared.iter().find(|(t, s, ..)| *t == i && set.contains(*s)) {
                Some((_, _, slim, _)) => slim.clone(),
                None => self.defs[i].clone(),
            })
            .collect()
    }

    /// The tools `entries` name: each one a family, a tool, a kit (`kit:make_menu`: the tools that
    /// job starts with, `toolkits::KITS`), `all` or `core`. Unknown entries are
    /// refused: the first [`MAX_UNKNOWN_SHOWN`] each with the closest valid name, then a count. At
    /// most [`MAX_ENTRIES`] entries, each at most [`MAX_ENTRY_CHARS`] (longer ones name nothing
    /// and get no suggestion), so the work and the answer stay small whatever an agent sends.
    pub fn select<'a>(&self, entries: impl IntoIterator<Item = &'a str>) -> Result<ToolSet, ScopeError> {
        self.select_as(entries, false)
    }

    /// [`Catalog::select`] for a cap: a kit entry reaches the tools it sometimes needs too
    /// (`toolkits::Kit::reach`), so an agent capped to its kit can do the kit's own steps.
    pub fn select_reach<'a>(&self, entries: impl IntoIterator<Item = &'a str>) -> Result<ToolSet, ScopeError> {
        self.select_as(entries, true)
    }

    fn select_as<'a>(&self, entries: impl IntoIterator<Item = &'a str>, reach: bool) -> Result<ToolSet, ScopeError> {
        let mut set = self.empty();
        let mut unknown = Vec::new();
        let mut unknown_count = 0usize;
        for (n, e) in entries.into_iter().enumerate() {
            if n >= MAX_ENTRIES {
                return Err(ScopeError::new("too_many_entries", format!("at most {MAX_ENTRIES} families or tool names at a time")).with("max", MAX_ENTRIES));
            }
            let e = e.trim();
            if e.is_empty() {
                continue;
            }
            if e.chars().count() > MAX_ENTRY_CHARS {
                unknown_count += 1;
                if unknown.len() < MAX_UNKNOWN_SHOWN {
                    unknown.push(format!("'{}' (longer than any name: {MAX_ENTRY_CHARS} characters at most)", clip(e, 24)));
                }
                continue;
            }
            if e == "all" {
                set = self.full();
            } else if e == "core" {
                set.union(&self.core);
            } else if let Some(i) = self.index(e) {
                set.insert(i);
            } else if let Some(k) = e.strip_prefix(crate::toolkits::KIT_PREFIX) {
                match crate::toolkits::kit(k) {
                    Some(kit) => set.union(&if reach { kit.reach(self) } else { kit.tools(self) }),
                    None => {
                        unknown_count += 1;
                        if unknown.len() < MAX_UNKNOWN_SHOWN {
                            let hint = closest(k, crate::toolkits::KITS.iter().map(|k| k.name)).map(|c| format!(" (did you mean '{}{c}'?)", crate::toolkits::KIT_PREFIX)).unwrap_or_default();
                            unknown.push(format!("'{e}'{hint}"));
                        }
                    }
                }
            } else if let Some((f, _, _)) = self.families().find(|(_, n, _)| *n == e) {
                self.members(f).collect::<Vec<_>>().into_iter().for_each(|i| set.insert(i));
            } else {
                unknown_count += 1;
                if unknown.len() >= MAX_UNKNOWN_SHOWN {
                    continue;
                }
                // A name with '_' is most likely a tool's (anim_chek: anim_check, not the family anim).
                let families = || self.families().map(|(_, n, _)| n).chain(["all", "core"]);
                let tools = || self.defs.iter().map(|d| d.name.as_str());
                let guess = if e.contains('_') { closest(e, tools()).or_else(|| closest(e, families())) } else { closest(e, families()).or_else(|| closest(e, tools())) };
                let hint = guess.map(|c| format!(" (did you mean '{c}'?)")).unwrap_or_default();
                unknown.push(format!("'{e}'{hint}"));
            }
        }
        if unknown_count == 0 {
            Ok(set)
        } else {
            let more = unknown_count - unknown.len();
            let more = if more > 0 { format!(" and {more} more") } else { String::new() };
            Err(ScopeError::new(
                "unknown_tools",
                format!(
                    "{}{more} {} no family, tool or kit: tools_catalog lists them (entries are families like world, sight, board, tool names like anim_check, or kits like kit:make_menu; or all, core)",
                    unknown.join(", "),
                    if unknown_count == 1 { "names" } else { "name" }
                ),
            )
            .with("unknown", unknown_count))
        }
    }

    /// A role template's tools (`None`: no template has that name).
    pub fn role(&self, name: &str) -> Option<ToolSet> {
        let r = ROLES.iter().find(|r| r.name == name)?;
        if r.all {
            return Some(self.full());
        }
        // A template may name a family the run doesn't have (game, without a hosted game).
        let mut set = self.empty();
        for e in r.families.iter().chain(r.tools) {
            if let Ok(s) = self.select([*e]) {
                set.union(&s);
            }
        }
        Some(set)
    }

    /// The names in `set`, in catalog order.
    pub fn names(&self, set: &ToolSet) -> Vec<String> {
        set.iter().map(|i| self.defs[i].name.clone()).collect()
    }

    /// Tools whose name, family, brief or description hold the words of `query` (any case):
    /// with `all`, every word; else at least one, ranked by how many and where (name and brief
    /// hits count most). Best first, then catalog order.
    pub fn search(&self, query: &str, all: bool) -> Vec<usize> {
        let words = query_words(query);
        if words.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<(usize, usize)> = Vec::new();
        for i in 0..self.len() {
            let head = format!("{} {} {}", self.defs[i].name, self.family_name(i), self.briefs[i]).to_ascii_lowercase().replace('_', " ");
            let body = self.defs[i].description.to_ascii_lowercase();
            let (mut found, mut score) = (0, 0);
            for w in &words {
                let in_head = head.contains(w.as_str());
                if in_head || body.contains(w.as_str()) {
                    found += 1;
                    score += if in_head { 3 } else { 1 };
                }
            }
            if (all && found == words.len()) || (!all && found > 0) {
                hits.push((i, score));
            }
        }
        hits.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        hits.into_iter().map(|(i, _)| i).collect()
    }
}

/// Words that carry no meaning in a search.
const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "to", "of", "on", "in", "for", "and", "or", "with", "my", "i", "me", "is", "it", "that", "this", "from", "by", "at",
    "be", "can", "how", "want", "need", "tool", "tools", "some", "any", "all", "so", "as", "do", "into", "its",
];

/// Lower-case search words of `query` (letters, digits and `_`; stop words left out).
fn query_words(query: &str) -> Vec<String> {
    let lower = query.to_ascii_lowercase();
    let mut out: Vec<String> = Vec::new();
    for w in lower.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').filter(|w| !w.is_empty() && !STOP_WORDS.contains(w)) {
        for part in w.split('_').filter(|p| !p.is_empty()) {
            if !out.iter().any(|o| o == part) {
                out.push(part.to_owned());
            }
        }
    }
    out.truncate(16);
    out
}

/// A definition as `tools/list` shows it.
pub fn listed_json(d: &ToolDef) -> Json {
    Json::obj([
        ("name", Json::from(d.name.as_str())),
        ("description", Json::from(d.description.as_str())),
        ("inputSchema", d.input_schema.clone()),
    ])
}

/// The valid name `key` was most likely meant to be: a prefix match or the nearest by edit
/// distance, if near enough.
fn closest<'a>(key: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let distance = |a: &str, b: &str| -> usize {
        if a.starts_with(b) || b.starts_with(a) {
            return 1;
        }
        let b: Vec<char> = b.chars().collect();
        let mut row: Vec<usize> = (0..=b.len()).collect();
        for (i, ca) in a.chars().enumerate() {
            let mut prev = row[0];
            row[0] = i + 1;
            for (j, cb) in b.iter().enumerate() {
                let next = (row[j + 1] + 1).min(row[j] + 1).min(prev + usize::from(ca != *cb));
                prev = row[j + 1];
                row[j + 1] = next;
            }
        }
        row[b.len()]
    };
    let limit = (key.chars().count() / 3).max(2);
    candidates.map(|c| (distance(key, c), c)).filter(|(d, _)| *d <= limit).min().map(|(_, c)| c)
}

/// Where a session's scope came from (stronger ones are not overridden by weaker).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The engine's default (`--tools-default`).
    Default,
    /// `agent_hello {role}` naming a template.
    Role,
    /// The connect URL (`?tools=`, `?role=`).
    Connect,
    /// `agent_hello {tools}`, `tools_activate` or `tools_deactivate`.
    Agent,
    /// The lead's `tools_scope`.
    Lead,
}

impl Origin {
    fn as_str(self) -> &'static str {
        match self {
            Origin::Default => "default",
            Origin::Role => "role",
            Origin::Connect => "connect",
            Origin::Agent => "agent",
            Origin::Lead => crate::authority::ROLE_LEAD,
        }
    }
}

#[derive(Clone, Debug)]
struct Scope {
    active: ToolSet,
    cap: Option<ToolSet>,
    origin: Origin,
    /// How it was chosen, for the agent ("role builder", "url tools=world,sight", ...).
    label: String,
    /// Changes when its list does (from the store's clock, so never repeats).
    version: u64,
    used: u64,
}

/// A lead's assignment, kept by agent name for sessions that say hello later.
#[derive(Clone, Debug)]
struct Assignment {
    active: ToolSet,
    cap: Option<ToolSet>,
    label: String,
    by: String,
    used: u64,
}

#[derive(Default)]
struct Inner {
    clock: u64,
    sessions: BTreeMap<String, Scope>,
    named: BTreeMap<String, Assignment>,
}

/// Sessions and assignments kept at most (sessions: four times the protocol's 1,024 open ones);
/// past it the least recently used goes, a capped session's last (a flood of new sessions must not
/// push a cap out: the session would fall back to the uncapped default).
const MAX_SCOPES: usize = 4096;
const MAX_ASSIGNMENTS: usize = 1024;
/// Longest label of where a scope came from.
const MAX_LABEL: usize = 200;

/// Inserts `k`, then evicts by `used` (least first) past `max`, never `k` itself (an entry just
/// stored must survive its own insertion).
fn put<V, K: Ord>(map: &mut BTreeMap<String, V>, k: &str, v: V, max: usize, used: impl Fn(&V) -> K) {
    map.insert(k.to_owned(), v);
    while map.len() > max {
        match map.iter().filter(|(key, _)| key.as_str() != k).min_by_key(|(_, v)| used(v)).map(|(k, _)| k.clone()) {
            Some(k) => map.remove(&k),
            None => break,
        };
    }
}

/// What a change did to a session's list.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    /// Tools newly listed / no longer listed.
    pub added: Vec<usize>,
    pub removed: Vec<usize>,
    /// The session's listed tools after it.
    pub listed: ToolSet,
}

/// Every session's scope and the lead's assignments, shared by the protocol threads (and the
/// engine thread, which checks playtests' calls against their starter's cap).
pub struct Scopes {
    pub catalog: Catalog,
    /// `--tools-default all`: sessions nobody configured list every tool.
    default_all: bool,
    inner: Mutex<Inner>,
    /// Held while a tools_request checks the board for duplicates and posts (see
    /// `toolscope_tools::request`).
    requests: Mutex<()>,
}

impl Scopes {
    pub fn new(catalog: Catalog, default_all: bool) -> Scopes {
        Scopes { catalog, default_all, inner: Mutex::new(Inner::default()), requests: Mutex::new(()) }
    }

    pub fn requests_lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.requests.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn default_all(&self) -> bool {
        self.default_all
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn default_scope(&self) -> Scope {
        let (active, label) = if self.default_all { (self.catalog.full(), "default (all)") } else { (self.catalog.empty(), "default (core)") };
        Scope { active, cap: None, origin: Origin::Default, label: label.into(), version: 0, used: 0 }
    }

    /// `scope`'s listed tools: the core, plus its active ones within its cap.
    fn listed_of(&self, scope: &Scope) -> ToolSet {
        let mut s = scope.active.clone();
        if let Some(cap) = &scope.cap {
            s.intersect(cap);
        }
        s.union(self.catalog.core());
        s
    }

    /// `session`'s scope (the default when it has none), touched.
    fn get(&self, g: &mut Inner, session: &str) -> Scope {
        g.clock += 1;
        let now = g.clock;
        match g.sessions.get_mut(session) {
            Some(s) => {
                s.used = now;
                s.clone()
            }
            None => self.default_scope(),
        }
    }

    /// Stores `scope` for `session`, bumping its version when its list changed from `before`.
    fn store(&self, g: &mut Inner, session: &str, mut scope: Scope, before: &ToolSet) -> Change {
        g.clock += 1;
        let listed = self.listed_of(&scope);
        if &listed != before {
            scope.version = g.clock;
        }
        scope.used = g.clock;
        let mut added = listed.clone();
        added.minus(before);
        let mut removed = before.clone();
        removed.minus(&listed);
        put(&mut g.sessions, session, scope, MAX_SCOPES, |s| (s.cap.is_some(), s.used));
        Change { added: added.iter().collect(), removed: removed.iter().collect(), listed }
    }

    /// A new session's connect-time options: `tools=<entries>` and/or `role=<template>` (the
    /// union of both). Anything else is refused, so a typo shows when the client connects.
    pub fn opened(&self, session: &str, query: &str) -> Result<(), String> {
        let mut active: Option<ToolSet> = None;
        let mut label = Vec::new();
        for pair in query.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let (k, v) = (percent_decode(k), percent_decode(v));
            let set = match k.as_str() {
                "tools" => self.catalog.select(v.split(',')).map_err(|e| format!("?tools: {}", e.message))?,
                "role" => self.catalog.role(&v).ok_or_else(|| format!("?role: '{}' is no role template (roles: {})", clip(&v, MAX_ENTRY_CHARS), role_names()))?,
                other => {
                    return Err(format!(
                        "'{}' is not an option of the MCP URL: use ?tools=<families or tools, comma-separated> and/or ?role=<{}>",
                        clip(other, MAX_ENTRY_CHARS),
                        role_names()
                    ))
                }
            };
            label.push(format!("{k}={v}"));
            active.get_or_insert_with(|| self.catalog.empty()).union(&set);
        }
        let mut g = self.lock();
        let scope = match active {
            Some(active) => {
                let label: String = format!("url {}", label.join("&")).chars().take(MAX_LABEL).collect();
                Scope { active, cap: None, origin: Origin::Connect, label, version: 0, used: 0 }
            }
            None => self.default_scope(),
        };
        let before = self.listed_of(&scope);
        self.store(&mut g, session, scope, &before);
        Ok(())
    }

    /// `session`'s listed tools.
    pub fn listed(&self, session: &str) -> ToolSet {
        let mut g = self.lock();
        let s = self.get(&mut g, session);
        self.listed_of(&s)
    }

    /// `session`'s `tools/list`.
    pub fn tools_for(&self, session: &str) -> Vec<ToolDef> {
        self.catalog.listing(&self.listed(session))
    }

    pub fn version(&self, session: &str) -> u64 {
        self.lock().sessions.get(session).map_or(0, |s| s.version)
    }

    /// Whether `session` may call tool `i` (core, or within its cap).
    pub fn allowed(&self, session: &str, i: usize) -> bool {
        if self.catalog.core().contains(i) {
            return true;
        }
        let mut g = self.lock();
        g.clock += 1;
        let now = g.clock;
        match g.sessions.get_mut(session) {
            Some(s) => {
                s.used = now;
                s.cap.as_ref().is_none_or(|cap| cap.contains(i))
            }
            None => true,
        }
    }

    /// Whether `session` may call the tool named `name` (tools the catalog doesn't know are the
    /// protocol's to refuse).
    pub fn allowed_name(&self, session: &str, name: &str) -> bool {
        self.catalog.index(name).is_none_or(|i| self.allowed(session, i))
    }

    /// The part of `set` outside `session`'s cap.
    pub fn outside_cap(&self, session: &str, set: &ToolSet) -> ToolSet {
        let g = self.lock();
        match g.sessions.get(session).and_then(|s| s.cap.clone()) {
            Some(cap) => {
                let mut out = set.clone();
                out.minus(&cap);
                out.minus(self.catalog.core());
                out
            }
            None => self.catalog.empty(),
        }
    }

    /// Adds `set` to `session`'s active tools; refused (nothing changes) when any is outside its cap.
    pub fn activate(&self, session: &str, set: &ToolSet) -> Result<Change, ScopeError> {
        let mut g = self.lock();
        let mut s = self.get(&mut g, session);
        if let Some(cap) = &s.cap {
            let mut beyond = set.clone();
            beyond.minus(cap);
            beyond.minus(self.catalog.core());
            if beyond.count() > 0 {
                return Err(ScopeError::new(
                    "not_in_scope",
                    format!(
                        "the lead capped your tools ({}); {} outside it: ask for them with tools_request {{need, why}}",
                        s.label,
                        self.catalog.names(&beyond).join(", ")
                    ),
                )
                .with("tools", Json::arr(self.catalog.names(&beyond).into_iter().map(Json::from))));
            }
        }
        let before = self.listed_of(&s);
        s.active.union(set);
        if s.origin != Origin::Lead {
            s.origin = Origin::Agent;
            s.label = "chosen by you (tools_activate)".into();
        }
        Ok(self.store(&mut g, session, s, &before))
    }

    /// Removes `set` from `session`'s active tools (the core stays).
    pub fn deactivate(&self, session: &str, set: &ToolSet) -> Change {
        let mut g = self.lock();
        let mut s = self.get(&mut g, session);
        let before = self.listed_of(&s);
        s.active.minus(set);
        if s.origin != Origin::Lead {
            s.origin = Origin::Agent;
            s.label = "chosen by you (tools_deactivate)".into();
        }
        self.store(&mut g, session, s, &before)
    }

    /// A successful `agent_hello` of `session` as `name` with `role`, asking for `tools` if it
    /// did. A lead's assignment for the name applies first (its cap bounds what the hello asks);
    /// else asked tools replace the active set; else a role template applies while nothing
    /// stronger chose. Answers what the session lists now, for the hello's reply.
    pub fn hello(&self, session: &str, name: &str, role: &str, tools: Option<&ToolSet>) -> Json {
        let mut g = self.lock();
        let mut s = self.get(&mut g, session);
        let before = self.listed_of(&s);
        g.clock += 1;
        let now = g.clock;
        let assigned = g.named.get_mut(name).map(|a| {
            a.used = now;
            a.clone()
        });
        let mut refused = self.catalog.empty();
        if let Some(a) = &assigned {
            // A hello never widens a session's cap: one capped as `will` that says hello as `bob`
            // (assigned without a cap, or a wider one) keeps will's cap. Only the lead's own
            // tools_scope / clear on the session's agent can widen it.
            let cap = match (s.cap.take(), a.cap.clone()) {
                (Some(mut mine), Some(theirs)) => {
                    mine.intersect(&theirs);
                    Some(mine)
                }
                (mine, theirs) => mine.or(theirs),
            };
            s = Scope { active: a.active.clone(), cap, origin: Origin::Lead, label: a.label.clone(), version: s.version, used: now };
        }
        if let Some(t) = tools {
            let mut want = t.clone();
            if let Some(cap) = &s.cap {
                refused = want.clone();
                refused.minus(cap);
                refused.minus(self.catalog.core());
                want.intersect(cap);
            }
            if assigned.is_some() {
                s.active.union(&want);
            } else {
                s.active = want;
                s.origin = Origin::Agent;
                s.label = "chosen by you (agent_hello tools)".into();
            }
        } else if assigned.is_none() && matches!(s.origin, Origin::Default | Origin::Role) {
            if let Some(r) = self.catalog.role(role) {
                s.active = r;
                s.origin = Origin::Role;
                s.label = format!("role {role}");
            }
        }
        let label = s.label.clone();
        let capped = s.cap.is_some();
        let change = self.store(&mut g, session, s, &before);
        let mut j = Json::obj([
            ("listed", Json::from(change.listed.count())),
            ("of", Json::from(self.catalog.len())),
            ("from", Json::from(label)),
            ("capped", Json::from(capped)),
        ]);
        if refused.count() > 0 {
            j.set("outside_cap", Json::arr(self.catalog.names(&refused).into_iter().map(Json::from)));
        }
        j
    }

    /// The lead `by` sets agent `name`'s tools to `active` (and its cap to the same with `cap`),
    /// now on `sessions` (its open ones) and for any session that later says hello as `name`.
    #[cfg(test)]
    pub fn assign(&self, by: &str, name: &str, sessions: &[String], active: ToolSet, cap: bool, label: String) -> Vec<Change> {
        let cap = cap.then(|| active.clone());
        self.assign_capped(by, name, sessions, active, cap, label)
    }

    /// [`Scopes::assign`] with the cap given apart from the list: a cap by kit allows more than
    /// the kit lists (`Catalog::select_reach`).
    pub fn assign_capped(&self, by: &str, name: &str, sessions: &[String], active: ToolSet, cap: Option<ToolSet>, label: String) -> Vec<Change> {
        let mut g = self.lock();
        g.clock += 1;
        let used = g.clock;
        let label = clip(&format!("{label}, by the lead {by}"), MAX_LABEL);
        let a = Assignment { active, cap, label, by: by.to_owned(), used };
        put(&mut g.named, name, a.clone(), MAX_ASSIGNMENTS, |a| a.used);
        sessions
            .iter()
            .map(|sid| {
                let s = self.get(&mut g, sid);
                let before = self.listed_of(&s);
                let next = Scope { active: a.active.clone(), cap: a.cap.clone(), origin: Origin::Lead, label: a.label.clone(), version: s.version, used };
                self.store(&mut g, sid, next, &before)
            })
            .collect()
    }

    /// The protocol closed `session` (DELETE, or evicted past its session limit): its scope goes,
    /// so the store holds open sessions only (at most the protocol's 1,024, well under
    /// [`MAX_SCOPES`], so a live scope is never evicted by leftovers).
    pub fn closed(&self, session: &str) {
        self.lock().sessions.remove(session);
    }

    /// Removes the assignment for `name` (who made it, if any); its open `sessions` go back to the
    /// default scope.
    pub fn clear(&self, name: &str, sessions: &[String]) -> Option<String> {
        let mut g = self.lock();
        let by = g.named.remove(name).map(|a| a.by);
        for sid in sessions {
            let s = self.get(&mut g, sid);
            if s.origin == Origin::Lead {
                let before = self.listed_of(&s);
                let mut next = self.default_scope();
                next.version = s.version;
                self.store(&mut g, sid, next, &before);
            }
        }
        by
    }

    /// `session`'s scope for the agent: what it lists, from where, and its cap.
    pub fn describe(&self, session: &str) -> Json {
        let mut g = self.lock();
        let s = self.get(&mut g, session);
        let listed = self.listed_of(&s);
        let mut j = Json::obj([
            ("listed", Json::from(listed.count())),
            ("of", Json::from(self.catalog.len())),
            ("list_bytes", Json::from(self.catalog.list_bytes(&listed))),
            ("from", Json::from(s.label.as_str())),
            ("origin", Json::from(s.origin.as_str())),
        ]);
        if let Some(cap) = &s.cap {
            let mut c = cap.clone();
            c.union(self.catalog.core());
            j.set("cap", Json::from(c.count()));
        }
        j
    }
}

/// The role templates' names, for messages.
pub fn role_names() -> String {
    ROLES.iter().map(|r| r.name).collect::<Vec<_>>().join(", ")
}

/// `%XX` escapes and `+` (a space) decoded; bad escapes are kept as they are.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                match std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(v) => {
                        out.push(v);
                        i += 3;
                        continue;
                    }
                    None => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Adds `tools` to `agent_hello`'s schema.
pub fn add_to_schemas(defs: &mut [ToolDef]) {
    if let Some(d) = defs.iter_mut().find(|d| d.name == "agent_hello") {
        let mut props = d.input_schema.get("properties").cloned().unwrap_or_else(|| Json::obj::<&str>([]));
        props.set(
            "tools",
            Json::obj([
                ("type", Json::from("array")),
                ("items", Json::obj([("type", Json::from("string"))])),
                (
                    "description",
                    Json::from(
                        "The tools this session lists beyond the core: families (world, sight, board ...), tool names, a job's kit \
                         (kit:make_menu; tools_catalog {kits: true} lists them) or \"all\". Without it, a role named like a template (builder, reviewer, playtester ...) picks the list unless the URL or \
                         the lead chose. tools_catalog lists families and roles.",
                    ),
                ),
            ]),
        );
        d.input_schema.set("properties", props);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str) -> ToolDef {
        ToolDef { name: name.into(), description: format!("{name} does things. More."), input_schema: Json::obj([("type", Json::from("object"))]) }
    }

    fn catalog() -> Catalog {
        let names = [
            "agent_hello", "tools_catalog", "tools_activate", "tools_deactivate", "tools_invoke", "tools_request", "tools_scope", "world_spawn",
            "world_set", "world_query", "world_history", "sight_frame", "camera_set", "window_view", "task_post", "anim_check", "play_state",
        ];
        Catalog::new(names.iter().map(|n| def(n)).collect(), &["play_state".to_owned()])
    }

    fn names(c: &Catalog, s: &ToolSet) -> Vec<String> {
        c.names(s)
    }

    #[test]
    fn families_by_prefix_and_by_name() {
        assert_eq!(family_of("world_spawn"), Some("world"));
        assert_eq!(family_of("world_history"), Some("journal"), "placed by name over the world_ prefix");
        assert_eq!(family_of("window_view"), Some("sight"));
        assert_eq!(family_of("style_check"), Some("studio"));
        assert_eq!(family_of("style_onboard"), Some("taste"), "placed by name over the style_ prefix");
        assert_eq!(family_of("board_check"), Some("taste"));
        assert_eq!(family_of("agent_hello"), Some("tools"));
        assert_eq!(family_of("zzz_new"), None);
        let c = catalog();
        assert_eq!(c.family_name(c.index("play_state").unwrap()), "game");
    }

    #[test]
    fn selections_parse_and_refuse_with_suggestions() {
        let c = catalog();
        let s = c.select(["world", "anim_check"]).unwrap();
        assert_eq!(names(&c, &s), ["world_spawn", "world_set", "world_query", "anim_check"]);
        assert_eq!(c.select(["all"]).unwrap().count(), c.len());
        assert_eq!(c.select(["core"]).unwrap().count(), CORE.len());
        let e = c.select(["wrld", "anim_chek", "sight"]).unwrap_err();
        assert_eq!(e.code, "unknown_tools");
        assert!(e.message.contains("'wrld' (did you mean 'world'?)"), "{}", e.message);
        assert!(e.message.contains("'anim_chek' (did you mean 'anim_check'?)"), "every unknown entry is named: {}", e.message);
    }

    #[test]
    fn query_options_at_connect() {
        let sc = Scopes::new(catalog(), false);
        sc.opened("a", "").unwrap();
        assert_eq!(names(&sc.catalog, &sc.listed("a")), CORE, "the default lists the core only");
        sc.opened("b", "tools=sight%2Canim_check&role=builder").unwrap();
        let b = names(&sc.catalog, &sc.listed("b"));
        assert!(b.contains(&"anim_check".to_owned()) && b.contains(&"camera_set".to_owned()) && b.contains(&"world_spawn".to_owned()), "{b:?}");
        assert!(sc.opened("c", "tool=world").unwrap_err().contains("'tool' is not an option"));
        assert!(sc.opened("c", "tools=wrld").unwrap_err().contains("did you mean 'world'"));
        assert!(sc.opened("c", "role=bulider").unwrap_err().contains("no role template"));
        let all = Scopes::new(catalog(), true);
        all.opened("a", "").unwrap();
        assert_eq!(all.listed("a").count(), all.catalog.len(), "--tools-default all");
        assert_eq!(all.listed("never-opened").count(), all.catalog.len());
    }

    #[test]
    fn activation_changes_the_list_and_its_version() {
        let sc = Scopes::new(catalog(), false);
        sc.opened("a", "").unwrap();
        let v0 = sc.version("a");
        let ch = sc.activate("a", &sc.catalog.select(["world"]).unwrap()).unwrap();
        assert_eq!(names(&sc.catalog, &ToolSet::from_indices(sc.catalog.len(), &ch.added)), ["world_spawn", "world_set", "world_query"]);
        let v1 = sc.version("a");
        assert_ne!(v0, v1);
        sc.activate("a", &sc.catalog.select(["world_set"]).unwrap()).unwrap();
        assert_eq!(sc.version("a"), v1, "no change to the list: no new version (no list_changed)");
        let ch = sc.deactivate("a", &sc.catalog.select(["world", "agent_hello"]).unwrap());
        assert_eq!(ch.removed.len(), 3);
        assert!(sc.listed("a").contains(sc.catalog.index("agent_hello").unwrap()), "the core stays");
    }

    #[test]
    fn roles_apply_only_while_nothing_stronger_chose() {
        let sc = Scopes::new(catalog(), false);
        sc.opened("a", "").unwrap();
        sc.hello("a", "ann", "builder", None);
        assert!(sc.listed("a").contains(sc.catalog.index("world_spawn").unwrap()));
        sc.hello("a", "ann", "sight_skeptic", None);
        assert!(!sc.listed("a").contains(sc.catalog.index("world_spawn").unwrap()), "a role may follow a role");
        sc.opened("b", "tools=anim_check").unwrap();
        sc.hello("b", "bob", "builder", None);
        assert_eq!(sc.listed("b").count(), CORE.len() + 1, "the URL's choice stands over the role");
        sc.hello("b", "bob", "builder", Some(&sc.catalog.select(["sight"]).unwrap()));
        assert!(sc.listed("b").contains(sc.catalog.index("sight_frame").unwrap()) && !sc.listed("b").contains(sc.catalog.index("anim_check").unwrap()));
    }

    #[test]
    fn the_leads_cap_bounds_lists_and_calls_and_waits_for_the_name() {
        let sc = Scopes::new(catalog(), true);
        sc.opened("w", "").unwrap();
        let world = sc.catalog.select(["world"]).unwrap();
        // Before the worker connects: kept by name.
        sc.assign("lead", "worker", &[], world.clone(), true, "tools world".into());
        let j = sc.hello("w", "worker", "builder", Some(&sc.catalog.select(["sight", "world_set"]).unwrap()));
        assert_eq!(j.get("capped").and_then(Json::as_bool), Some(true));
        assert!(j.get("outside_cap").unwrap().to_string().contains("sight_frame"), "{j}");
        assert_eq!(sc.listed("w").count(), CORE.len() + 3);
        assert!(sc.allowed_name("w", "world_spawn") && sc.allowed_name("w", "tools_request") && !sc.allowed_name("w", "sight_frame"));
        let e = sc.activate("w", &sc.catalog.select(["sight"]).unwrap()).unwrap_err();
        assert_eq!(e.code, "not_in_scope");
        // Uncapped assignment: the agent may add more.
        sc.assign("lead", "worker", &["w".to_owned()], world.clone(), false, "tools world".into());
        assert!(sc.allowed_name("w", "sight_frame"));
        sc.activate("w", &sc.catalog.select(["sight"]).unwrap()).unwrap();
        assert_eq!(sc.clear("worker", &["w".to_owned()]).as_deref(), Some("lead"));
        assert_eq!(sc.listed("w").count(), sc.catalog.len(), "back to the default (all, here)");
    }

    /// A flood of new sessions evicts uncapped scopes first: a capped one never falls back to the
    /// uncapped default while others can go.
    #[test]
    fn a_flood_of_sessions_does_not_push_out_a_cap() {
        let sc = Scopes::new(catalog(), true);
        sc.opened("w", "").unwrap();
        sc.assign("lead", "worker", &["w".to_owned()], sc.catalog.select(["world"]).unwrap(), true, "tools world".into());
        for i in 0..MAX_SCOPES + 100 {
            sc.opened(&format!("flood-{i}"), "").unwrap();
        }
        assert!(sc.lock().sessions.len() <= MAX_SCOPES);
        assert!(!sc.allowed_name("w", "sight_frame"), "the cap survived the flood");
    }

    /// Review probe: a session capped as `will` that says hello as `bob` (assigned without a
    /// cap) kept nothing of will's cap. A hello never widens a session's cap.
    #[test]
    fn a_rename_never_widens_the_cap() {
        let sc = Scopes::new(catalog(), false);
        sc.opened("w", "").unwrap();
        let world = sc.catalog.select(["world"]).unwrap();
        sc.assign("lead", "will", &[], world.clone(), true, "tools world".into());
        sc.assign("lead", "bob", &[], sc.catalog.select(["world", "sight"]).unwrap(), false, "tools world,sight".into());
        sc.hello("w", "will", "", None);
        assert!(!sc.allowed_name("w", "sight_frame"), "capped as will");
        let j = sc.hello("w", "bob", "", None);
        assert!(!sc.allowed_name("w", "sight_frame"), "still capped after hello as bob: {j}");
        assert_eq!(j.get("capped").and_then(Json::as_bool), Some(true));
        assert!(!sc.listed("w").contains(sc.catalog.index("sight_frame").unwrap()), "nor listed");
        // A narrower cap for the new name narrows it further.
        sc.assign("lead", "cy", &[], sc.catalog.select(["world_spawn"]).unwrap(), true, "tools world_spawn".into());
        sc.hello("w", "cy", "", None);
        assert!(sc.allowed_name("w", "world_spawn") && !sc.allowed_name("w", "world_set"));
        // The lead's own tools_scope on the session's agent may still widen it.
        sc.assign("lead", "cy", &["w".to_owned()], sc.catalog.select(["world", "sight"]).unwrap(), false, "tools world,sight".into());
        assert!(sc.allowed_name("w", "sight_frame"));
    }

    /// Review probes: 200,000 entries, a 20,000-character entry and a 600 KB lead label each took
    /// seconds or made answers of hundreds of KB.
    #[test]
    fn selections_and_labels_are_bounded() {
        let c = catalog();
        let long = "z".repeat(20_000);
        let e = c.select([long.as_str()]).unwrap_err();
        assert!(e.message.len() < 400, "{} bytes", e.message.len());
        let many = vec!["q"; 200_000].join(",");
        let e = c.select(many.split(',')).unwrap_err();
        assert_eq!(e.code, "too_many_entries");
        let unknown = vec!["q"; MAX_ENTRIES].join(",");
        let e = c.select(unknown.split(',')).unwrap_err();
        assert!(e.message.len() < 1024 && e.message.contains(&format!("and {} more", MAX_ENTRIES - MAX_UNKNOWN_SHOWN)), "{}", e.message);
        assert_eq!(e.to_json().get("unknown").and_then(Json::as_u64), Some(MAX_ENTRIES as u64));
        let sc = Scopes::new(catalog(), false);
        sc.opened("w", "").unwrap();
        sc.assign("lead", "will", &["w".to_owned()], sc.catalog.select(["world"]).unwrap(), true, "world,".repeat(100_000));
        assert!(sc.describe("w").to_string().len() < 600, "{}", sc.describe("w"));
        assert!(sc.opened("x", &format!("{}=1", "k".repeat(10_000))).unwrap_err().len() < 400);
        assert!(sc.opened("x", &format!("role={}", "r".repeat(10_000))).unwrap_err().len() < 400);
    }

    /// Review probe: scopes of closed sessions (an agent's earlier, reconnected sessions) stayed,
    /// and capped leftovers could evict a live cap; a fresh scope could be evicted as it was
    /// stored. Closed sessions now leave the store, and a new entry never evicts itself.
    #[test]
    fn closed_sessions_leave_and_new_entries_survive_a_full_store() {
        let sc = Scopes::new(catalog(), false);
        sc.opened("w", "").unwrap();
        sc.assign("lead", "will", &[], sc.catalog.select(["world"]).unwrap(), true, "tools world".into());
        sc.hello("w", "will", "", None);
        for i in 0..MAX_SCOPES + 50 {
            let s = format!("old-{i}");
            sc.opened(&s, "").unwrap();
            sc.hello(&s, "will", "", None);
            sc.closed(&s); // as the protocol does on DELETE and on its own eviction
        }
        assert_eq!(sc.lock().sessions.len(), 1);
        assert!(!sc.allowed_name("w", "sight_frame"), "the live cap holds");
        // A store full of capped entries (more than the protocol ever keeps open) still keeps
        // what was just stored.
        for i in 0..MAX_SCOPES {
            let s = format!("capped-{i}");
            sc.opened(&s, "").unwrap();
            sc.hello(&s, "will", "", None);
        }
        sc.opened("fresh", "tools=sight").unwrap();
        assert_eq!(sc.listed("fresh").count(), CORE.len() + 3, "sight_frame, camera_set, window_view");
        assert!(sc.lock().sessions.len() <= MAX_SCOPES);
    }

    #[test]
    fn search_finds_by_words() {
        let c = catalog();
        let hits = c.search("spawn world", true);
        assert_eq!(c.name(hits[0]), "world_spawn");
        assert!(c.search("paint vertex colours", true).is_empty());
        assert!(c.search("the a of", false).is_empty(), "stop words alone find nothing");
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("a%2Cb+c%zz%4"), "a,b c%zz%4");
    }

    #[test]
    fn list_bytes_match_the_json() {
        let c = catalog();
        let set = c.select(["world", "core"]).unwrap();
        let json = Json::obj([("tools", Json::arr(c.listing(&set).iter().map(listed_json)))]).to_string();
        assert_eq!(c.list_bytes(&set), json.len());
    }
}
