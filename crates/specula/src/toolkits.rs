//! Tool kits: the tools for a job.
//!
//! Families ([`crate::toolscope::FAMILIES`]) sort the tools by what they are, as a workshop sorts
//! saws from chisels. An agent with a job to do needs the other index (owner, 2026-11-12:
//! "knowing which ones to get for a specific task", as a carpenter or a machinist knows their
//! tools): **which few tools does this job take, in what order, and what says it is done?**
//!
//! - A **kit** ([`KITS`]) names the tools a job starts with (`tools`: listed when the kit is
//!   activated, a handful, so the list stays a few thousand tokens), the ones it sometimes needs
//!   (`more`: named with their catalog lines, not listed until asked for), the order they are
//!   used in (`steps`) and the check that says the job is done (`done`: always something the
//!   engine measures, never "it looks right").
//! - A kit is an entry wherever families and tool names are: `tools_activate {tools:
//!   ["kit:make_menu"]}`, `agent_hello {tools}`, `/mcp?tools=kit:make_menu`, the lead's
//!   `tools_scope`. `tools_catalog {kits: true}` lists them; `tools_catalog {kit: NAME}` shows one.
//! - A kit **lists** its `tools`; a lead's **cap** by a kit allows its `tools` and its `more`
//!   ([`Kit::reach`]). The steps and the done check name `more` tools (release the claim, run
//!   the style check), so an agent capped to its kit must be able to call them (review R1 of
//!   m5-toolkits: it could take a claim and never release it).
//! - **Tags** ([`tags`]) cut across families: whether a tool reads, writes, checks, or needs a
//!   grant. They are derived from the authority table ([`crate::authority::access`]) and the
//!   tool's name, so they can't drift from what the engine enforces.
//! - Every engine tool is in a kit or in [`NOT_IN_A_KIT`] with the reason
//!   (`every_tool_is_in_a_kit_or_says_why`): a new tool has to say which job it is for.
//!
//! Two tools that take the same large arguments are listed once ([`shared_listing`]): beside
//! `world_spawn`, `world_set` lists its components by reference.
//!
//! Like the rest of the scope code this runs on the protocol threads and touches no simulation.

use crate::toolscope::{Catalog, ToolSet};
use specula_core::Json;
use specula_protocol::ToolDef;

/// What an entry that names a kit starts with (`kit:make_menu`).
pub const KIT_PREFIX: &str = "kit:";

/// The most tools (or families) a kit lists when activated: a kit is small or it is a role
/// (held by `kits_are_small_named_as_jobs_and_say_when_they_are_done`).
#[cfg(test)]
pub const MAX_KIT_TOOLS: usize = 6;

pub struct Kit {
    /// A job, verb first (`make_menu`), so it reads apart from the families (`ui`).
    pub name: &'static str,
    /// The job in a few words.
    pub about: &'static str,
    /// Listed when the kit is activated: tool names (or a family, for the hosted game's).
    pub tools: &'static [&'static str],
    /// Sometimes needed for this job: named in the kit's catalog entry, not listed until asked for.
    pub more: &'static [&'static str],
    /// The order the tools are used in.
    pub steps: &'static str,
    /// What says the job is done: a check the engine makes.
    pub done: &'static str,
}

/// The jobs. Order: building a game's parts, then checking them, then working with others.
pub const KITS: &[Kit] = &[
    Kit {
        name: "build_place",
        about: "build and dress a place in the world",
        tools: &["world_spawn", "world_set", "world_query", "claim_take", "sight_frame"],
        more: &["world_batch", "world_place", "world_delete", "world_list", "claim_release", "sight_compare", "sight_query", "style_check", "tilemap_get", "tilemap_set", "text_edit"],
        steps: "world_query for what is there; claim_take the subtree or region; world_spawn and world_set (world_batch for many at once, world_place to place by relations instead of coordinates); sight_frame and read its packet, not only the picture; claim_release",
        done: "sight_frame's packet lists every subject you built with visible_px above 0 and not occluded, and style_check reports no errors",
    },
    Kit {
        name: "make_asset",
        about: "a prop or building from a recipe (a model as code)",
        tools: &["model_schema", "project_write", "model_build", "studio_sheet", "studio_check", "review_submit"],
        more: &["project_read", "project_patch", "project_list", "project_delete", "project_upload", "model_reload", "model_collider", "model_paint", "model_path", "model_export", "studio_stage", "studio_lineup", "studio_compare", "style_check", "procgen_texture", "formae_list", "formae_get"],
        steps: "model_schema for how a recipe is written (with an example that builds); formae_list and formae_get for a named shape to start from (the snippet to paste); project_write the recipe; model_build it; studio_sheet to see it from every side; studio_check and style_check; review_submit it with the sheet",
        done: "studio_check and style_check report no errors, and review_submit put it in the owner's queue",
    },
    Kit {
        name: "make_menu",
        about: "a menu or HUD screen (interface as data)",
        tools: &["project_read", "project_write", "ui_tree", "ui_frame", "ui_act", "ui_check"],
        more: &["project_patch", "l10n_get", "l10n_set", "l10n_check"],
        steps: "project_read a screen the game already has as the pattern; project_write yours; ui_tree for what was laid out; ui_frame to see it; ui_act to drive it as a player would (keys, gamepad, touch); ui_check",
        done: "ui_check reports 0 errors at every target and scale, and ui_act reaches every widget with a gamepad",
    },
    Kit {
        name: "animate",
        about: "a character's motion: clips and graphs as data",
        tools: &["anim_get", "anim_set", "anim_check", "anim_filmstrip"],
        more: &["anim_state", "ragdoll_set", "ragdoll_info"],
        steps: "anim_get a clip or graph the game has as the pattern; anim_set yours; anim_filmstrip to see it frame by frame; anim_check",
        done: "anim_check passes its motion checks (foot slide, pops, reach) for every clip you made",
    },
    Kit {
        name: "write_rules",
        about: "gameplay rules and balance as data (Lex)",
        tools: &["lex_schema", "lex_get", "lex_edit", "lex_validate", "lex_simulate"],
        more: &["lex_list", "lex_session", "lex_import", "lex_export", "lex_live", "balance_targets", "balance_sweep", "balance_tune"],
        steps: "lex_schema for what a rule may say; lex_get the set; lex_edit; lex_validate; lex_simulate a game with it (balance_sweep over many seeds when numbers changed)",
        done: "lex_validate is clean, and lex_simulate or balance_sweep gives the outcome the design asks for (balance_targets)",
    },
    Kit {
        name: "run_playtest",
        about: "play the game headless and prove what happened",
        tools: &["playtest_run", "playtest_report", "world_hash", "game"],
        more: &["save_write", "save_read", "save_list", "save_delete", "time_control", "world_save", "world_load", "desync_trace", "desync_compare"],
        steps: "write the playtest as data (steps and what you expect); playtest_run; playtest_report; a replay of the same run must end on the same world_hash",
        done: "playtest_report shows every expectation met, and the replay's world_hash equals the run's",
    },
    Kit {
        name: "make_cutscene",
        about: "a cutscene: camera rails, entity tracks, cues and subtitles as data",
        tools: &["timeline_get", "timeline_set", "timeline_validate", "timeline_check", "timeline_preview"],
        more: &["timeline_play", "timeline_pause", "timeline_scrub", "timeline_stop", "camera_set", "camera_list", "camera_delete", "window_view", "sight_orbit"],
        steps: "timeline_get one the game has as the pattern; timeline_set yours; timeline_validate; timeline_check (the rails: speed, turns, what the camera passes through); timeline_preview for a sheet of its moments",
        done: "timeline_check reports no errors, and the preview sheet shows each moment framed as the design says",
    },
    Kit {
        name: "translate",
        about: "the game's words in another language",
        tools: &["l10n_get", "l10n_set", "l10n_check", "ui_check"],
        more: &["l10n_tasks", "ui_frame", "ui_tree"],
        steps: "l10n_get the source table and the notes on each text; l10n_set your translations; l10n_check (missing, placeholders, plurals); ui_check in that language for overflow",
        done: "l10n_check reports no errors for the locale, and ui_check in it reports 0 errors",
    },
    Kit {
        name: "make_sound",
        about: "a sound or music piece from a recipe, heard through the engine's own meters",
        tools: &["project_write", "audio_preview", "audio_played", "audio_levels"],
        more: &["project_read", "audio_settings"],
        steps: "project_write the recipe; audio_preview renders and measures it offline; once the game plays it (a timeline cue, a module's sound, the game's own), audio_played and audio_levels say what was heard and how loud",
        done: "audio_preview's loudness and peak are inside the game's targets, and audio_played lists the sound where the game should play it",
    },
    Kit {
        name: "make_effect",
        about: "particles, effects and decals as data",
        tools: &["vfx_get", "vfx_set", "vfx_preview", "vfx_check"],
        more: &["vfx_info", "sight_frame"],
        steps: "vfx_get an effect the game has as the pattern; vfx_set yours; vfx_preview for a sheet over its lifetime; vfx_check",
        done: "vfx_check passes (its budget, and the photosensitivity limits)",
    },
    Kit {
        name: "shape_terrain",
        about: "terrain, scatter and what grows on it",
        tools: &["procgen_terrain", "terrain_info", "terrain_edit", "sight_frame"],
        more: &["procgen_scatter", "procgen_texture", "stream_state", "stream_fly"],
        steps: "procgen_terrain makes it from a recipe; terrain_info for how it is drawn; terrain_edit to sculpt and paint; procgen_scatter for what stands on it; sight_frame",
        done: "terrain_info shows it inside its budget, and sight_frame's packet shows what you scattered standing on it",
    },
    Kit {
        name: "find_paths",
        about: "navmeshes, paths and creatures that think",
        tools: &["nav_build", "nav_path", "nav_agent", "ai_set", "ai_check"],
        more: &["nav_point", "nav_info", "ai_status"],
        steps: "nav_build the navmesh over what was built; nav_path between the places the design needs joined; nav_agent for who walks; ai_set their minds; ai_check",
        done: "nav_path finds every route the design needs, and ai_check is clean",
    },
    Kit {
        name: "check_physics",
        about: "colliders, bodies and joints that behave",
        tools: &["physics_check", "physics_explain", "physics_raycast", "physics_overlap", "physics_contacts"],
        more: &["world_set", "time_control", "physics_cast", "physics_events", "physics_info", "physics_impulse", "physics_joint", "physics_joints"],
        steps: "give entities collider and body components (world_set); time_control steps some ticks; physics_check asserts the outcomes you expect (at_rest, no_penetration, no_tunnelling ...); physics_explain one that misbehaves; physics_raycast, physics_overlap and physics_contacts ask what is where and what touches",
        done: "physics_check passes every outcome you asserted for the scene",
    },
    Kit {
        name: "light_scene",
        about: "lighting, sky and baked light",
        tools: &["world_env", "gi_bake", "gi_check", "sight_compare"],
        more: &["gi_info", "gi_settings", "sight_frame"],
        steps: "world_env for sun, sky, fog and grading; gi_bake the probe volume; gi_check it; sight_compare before and after",
        done: "gi_check passes, and sight_compare shows the change you meant and nothing else",
    },
    Kit {
        name: "write_logic",
        about: "game logic as a Rust module, hot-reloaded",
        tools: &["project_read", "project_write", "logic_build", "logic_status"],
        more: &["project_patch", "project_mount", "logic_reload", "logic_unload", "logic_input", "logic_hook"],
        steps: "project_read the module's source; project_write your change; logic_build (native code is the owner's or a granted lead's: anyone else's build files a proposal for them to approve); logic_status",
        done: "logic_build's diagnostics are empty and logic_status shows the new build loaded (or your proposal filed)",
    },
    Kit {
        name: "find_slow_frame",
        about: "why a frame is slow, on the reference hardware",
        tools: &["perf_frame", "perf_zones", "trace_incidents", "trace_query"],
        more: &["perf_light", "perf_capture", "perf_memory", "disk_status", "engine_info"],
        steps: "perf_frame for the passes, draws and budget errors; perf_zones for where the CPU time went; trace_incidents for stalls the engine caught itself; trace_query for the events around one; perf_light when the GPU's time is the light's (what each feature costs on this machine, and which to give up for the budget)",
        done: "perf_frame reports no budget errors with the game running (the budgets are the reference hardware's)",
    },
    Kit {
        name: "find_cause",
        about: "why something failed or changed",
        tools: &["trace_call", "trace_query", "trace_incidents", "world_history"],
        more: &["trace_incident", "trace_follow", "trace_crash", "trace_settings", "world_changes", "world_undo"],
        steps: "trace_call for one call's whole causal tree; trace_query for events by code; trace_incidents for what the engine flagged; world_history for who changed what",
        done: "trace_call shows the event that caused it and the call that made it",
    },
    Kit {
        name: "review_work",
        about: "check finished work that someone else made",
        tools: &["review_list", "review_decide", "critique_add", "sight_compare"],
        more: &["review_screen", "critique_list", "critique_update", "merge_suggest", "studio_compare", "sight_frame"],
        steps: "review_list the queue; look with sight_compare (and the evidence the engine recorded with the submission); critique_add what is wrong, on the entity; review_decide (the reviewer's grant)",
        done: "review_list shows nothing you took still waiting: each has a decision, or a critique_add that says what to change",
    },
    Kit {
        name: "keep_memory",
        about: "what the project remembers: read before you change, write before you stop",
        tools: &["memory_list", "memory_get", "memory_why", "memory_note"],
        more: &[],
        steps: "memory_list; memory_get what bears on your work; memory_why PATH before you change a file you did not write; when you decide or learn what the next agent needs, memory_note it (supersedes: the note it replaces)",
        done: "memory_list shows your note, and memory_get of a note you replaced answers with yours",
    },
    Kit {
        name: "work_together",
        about: "build beside other agents without colliding",
        tools: &["claim_take", "claim_release", "task_list", "task_take", "task_done", "note_add"],
        more: &[
            "claim_list", "claim_share", "claim_renew", "task_post", "task_update", "task_get", "intent_declare", "intent_list", "intent_clear", "note_list", "note_delete", "world_rollup",
            "lease_take", "lease_renew", "lease_release", "lease_list",
        ],
        steps: "task_list and task_take your work; claim_take what you will change; build; note_add what the next agent must know; task_done; claim_release",
        done: "task_list shows your task done, claim_list none of your claims, and note_list the note you left",
    },
    Kit {
        name: "direct_swarm",
        about: "the lead's job: plan, hand out tools, follow progress",
        tools: &["task_plan", "task_progress", "tools_scope", "world_rollup", "review_list"],
        more: &["swarm_settings", "stream_settings", "security_incidents", "security_clear", "disk_prune"],
        steps: "task_plan turns the design into tasks; tools_scope gives each agent the kit its task needs; task_progress and world_rollup say how it goes; review_list for what waits on the owner",
        done: "task_progress shows every planned task done or blocked with a reason",
    },
    Kit {
        name: "ask_human",
        about: "let the owner choose: galleries of options, and what they liked before",
        tools: &["gallery_create", "gallery_wait", "gallery_apply", "taste_query"],
        more: &["gallery_get", "gallery_list", "gallery_round", "gallery_open", "gallery_close", "gallery_decide", "gallery_delete", "taste_suggest", "board_measure", "board_check", "style_onboard"],
        steps: "taste_query for what they chose before; gallery_create a few real options; gallery_wait for their pick (they may be away: do other work); gallery_apply it",
        done: "gallery_get shows the owner's pick applied, and taste_query returns it next time",
    },
];

/// Engine tools that belong to no job's kit, and why. Everything else is in a kit
/// (`every_tool_is_in_a_kit_or_says_why`).
#[cfg(test)]
pub const NOT_IN_A_KIT: &[(&str, &str)] = &[
    ("agent_hello", "core: always listed"),
    ("tools_catalog", "core: always listed"),
    ("tools_activate", "core: always listed"),
    ("tools_deactivate", "core: always listed"),
    ("tools_invoke", "core: always listed"),
    ("tools_request", "core: always listed"),
    ("editor_state", "the human's editor at the window, not an agent's job"),
    ("editor_input", "the human's editor at the window: the owner's alone"),
    ("security_allow_lead", "the owner's alone"),
];

/// The tags, with what each means. A tool has exactly one of the first seven (from the authority
/// table) and `checks` when it measures work and reports findings.
pub const TAGS: &[(&str, &str)] = &[
    ("reads", "changes nothing"),
    ("writes", "changes the world, rules or project files (claims apply; journaled, undoable)"),
    ("owned", "changes items that have owners (tasks, notes, claims, cameras ...): yours, or the lead's call"),
    ("open", "anyone may: no owner, claim or grant applies (its description says what it changes)"),
    (crate::authority::ROLE_LEAD, "changes need the lead (an operator grant: --lead NAME)"),
    (crate::authority::ROLE_REVIEWER, "changes need a reviewer (an operator grant: --reviewer NAME)"),
    (crate::authority::ROLE_OWNER, "the owner's alone"),
    ("game", "the hosted game's own tool"),
    ("checks", "measures your work and reports findings: what says a job is done"),
];

/// A tool's tags: who may use it and whether it changes anything (from the authority table), and
/// whether it is a check (by its name).
pub fn tags(name: &str) -> Vec<&'static str> {
    use crate::authority::Access;
    let mut t = vec![match crate::authority::access(name) {
        Some(Access::ReadOnly) => "reads",
        Some(Access::World(_) | Access::ReplacesWorld) => "writes",
        Some(Access::Owned(_)) => "owned",
        Some(Access::Open(_)) => "open",
        // (The grants' own names: a role is never spelt apart from the authority module.)
        Some(Access::Lead) => crate::authority::ROLE_LEAD,
        Some(Access::Reviewer) => crate::authority::ROLE_REVIEWER,
        Some(Access::Owner) => crate::authority::ROLE_OWNER,
        None => "game",
    }];
    if name.ends_with("_check") || name.ends_with("_validate") {
        t.push("checks");
    }
    t
}

pub fn kit(name: &str) -> Option<&'static Kit> {
    KITS.iter().find(|k| k.name == name)
}

/// "build_place, make_asset, ..." for messages.
pub fn kit_names() -> String {
    KITS.iter().map(|k| k.name).collect::<Vec<_>>().join(", ")
}

impl Kit {
    /// The tools the kit lists when activated. An entry the run doesn't have (the hosted game's
    /// family, without a game) names nothing.
    pub fn tools(&self, c: &Catalog) -> ToolSet {
        let mut set = c.empty();
        for e in self.tools {
            if let Ok(s) = c.select([*e]) {
                set.union(&s);
            }
        }
        set
    }

    /// Everything the job may need: its `tools` and its `more`. What a lead's cap by this kit
    /// allows (the list stays [`Kit::tools`]).
    pub fn reach(&self, c: &Catalog) -> ToolSet {
        let mut set = self.tools(c);
        for e in self.more {
            if let Ok(s) = c.select([*e]) {
                set.union(&s);
            }
        }
        set
    }

    /// The kit's catalog entry: its tools with their lines and tags (and whether `listed` has
    /// them), the ones it sometimes needs, the steps, the done check and what listing it costs.
    pub fn to_json(&self, c: &Catalog, listed: &ToolSet) -> Json {
        let line = |e: &str| match c.index(e) {
            Some(i) => c.brief(i).to_owned(),
            None => c.families().find(|(_, n, _)| *n == e).map_or_else(|| "not in this run".to_owned(), |(_, _, about)| format!("family: {about}")),
        };
        let tools = self.tools.iter().map(|e| {
            let mut j = Json::obj([("line", Json::from(line(e)))]);
            if let Some(i) = c.index(e) {
                j.set("tags", Json::arr(tags(e).into_iter().map(Json::from)));
                j.set("listed", Json::from(listed.contains(i)));
            }
            ((*e).to_owned(), j)
        });
        let mut with_core = self.tools(c);
        with_core.union(c.core());
        Json::obj([
            ("name", Json::from(self.name)),
            ("about", Json::from(self.about)),
            ("tools", Json::obj(tools)),
            ("more", Json::obj(self.more.iter().map(|e| ((*e).to_owned(), Json::from(line(e)))))),
            ("steps", Json::from(self.steps)),
            ("done", Json::from(self.done)),
            ("list_bytes", Json::from(c.list_bytes(&with_core))),
            ("activate", Json::from(format!("tools_activate {{tools: [\"{KIT_PREFIX}{}\"]}}", self.name))),
        ])
    }
}

/// Pairs (tool, sibling): when both are listed, `tool`'s properties that are exactly the
/// sibling's are listed by reference. `world_set` takes the same component objects as
/// `world_spawn` (about 10 KB of schema, some 2,500 tokens, that a builder would otherwise read
/// twice).
pub const SHARED: &[(&str, &str)] = &[("world_set", "world_spawn")];

/// A property smaller than this is cheaper to repeat than to point at.
const SHARE_MIN_BYTES: usize = 120;

/// `d` as it is listed beside `sibling`: each property whose schema is byte for byte the
/// sibling's becomes a one-line reference to it. `None` when nothing is shared. Only the
/// **listing** changes: arguments are checked against the tool's own full schema as before, and
/// `tools_activate` still answers the full definition.
pub fn shared_listing(d: &ToolDef, sibling: &ToolDef) -> Option<ToolDef> {
    let props = d.input_schema.get("properties")?.as_object()?;
    let theirs = sibling.input_schema.get("properties")?;
    let mut shared = 0usize;
    let slim: Vec<(String, Json)> = props
        .iter()
        .map(|(k, v)| {
            let text = v.to_string();
            if text.len() < SHARE_MIN_BYTES || theirs.get(k).is_none_or(|o| o.to_string() != text) {
                return (k.clone(), v.clone());
            }
            shared += 1;
            let mut r = Json::obj::<&str>([]);
            if let Some(t) = v.get("type") {
                r.set("type", t.clone());
            }
            r.set("description", Json::from(format!("as {}'s {k}", sibling.name)));
            (k.clone(), r)
        })
        .collect();
    if shared == 0 {
        return None;
    }
    let mut schema = d.input_schema.clone();
    schema.set("properties", Json::obj(slim));
    Some(ToolDef {
        name: d.name.clone(),
        description: format!("{} (Listed beside {}: a property described \"as {}'s\" takes exactly what that tool's property takes.)", d.description, sibling.name, sibling.name),
        input_schema: schema,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str, props: Json) -> ToolDef {
        ToolDef { name: name.into(), description: format!("{name}."), input_schema: Json::obj([("type", Json::from("object")), ("properties", props)]) }
    }

    #[test]
    fn kits_are_small_named_as_jobs_and_say_when_they_are_done() {
        let mut seen = std::collections::BTreeSet::new();
        for k in KITS {
            assert!(seen.insert(k.name), "two kits are named '{}'", k.name);
            assert!(k.name.len() <= 24 && k.name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'), "'{}' is a short lower-case job name", k.name);
            assert!(crate::toolscope::FAMILIES.iter().all(|f| f.name != k.name), "the kit '{}' has a family's name: name the job (verb first)", k.name);
            assert!((2..=MAX_KIT_TOOLS).contains(&k.tools.len()), "the kit '{}' lists {} tools: a kit lists 2 to {MAX_KIT_TOOLS}, the rest go in `more`", k.name, k.tools.len());
            assert!(!k.about.is_empty() && k.about.chars().count() <= 80, "'{}': about is one short line", k.name);
            assert!(!k.steps.is_empty() && k.steps.chars().count() <= 320, "'{}': steps are one short paragraph ({} chars)", k.name, k.steps.chars().count());
            assert!(!k.done.is_empty() && k.done.chars().count() <= 200, "'{}': done is one sentence ({} chars)", k.name, k.done.chars().count());
            // The steps start with the kit's own tools: each one the kit lists is named in them or
            // in the done check (a tool nobody is told to use isn't part of the job's start).
            for t in k.tools.iter().filter(|t| **t != crate::toolscope::GAME_FAMILY) {
                assert!(k.steps.contains(t) || k.done.contains(t), "the kit '{}' lists {t} but neither its steps nor its done check use it", k.name);
            }
            for t in k.more {
                assert!(!k.tools.contains(t), "the kit '{}' names {t} twice", k.name);
            }
            // Done is measured: it names a tool of the kit whose answer says so.
            assert!(k.tools.iter().chain(k.more).any(|t| k.done.contains(t)), "the kit '{}': its done check names no tool of the kit that measures it", k.name);
        }
        for (name, why) in NOT_IN_A_KIT {
            assert!(!why.is_empty(), "{name}");
            assert!(KITS.iter().all(|k| !k.tools.contains(name) && !k.more.contains(name)), "{name} is in a kit and in NOT_IN_A_KIT");
        }
    }

    #[test]
    fn tags_follow_the_authority_table() {
        assert_eq!(tags("world_query"), ["reads"]);
        assert_eq!(tags("world_spawn"), ["writes"]);
        assert_eq!(tags("ui_check"), ["reads", "checks"]);
        assert_eq!(tags("lex_validate"), ["reads", "checks"]);
        assert_eq!(tags("task_take"), ["owned"]);
        assert_eq!(tags("security_allow_lead"), [crate::authority::ROLE_OWNER]);
        assert_eq!(tags("play_command"), ["game"]);
        for (tool, _) in crate::toolscope::BRIEFS {
            let t = tags(tool);
            assert!(t.iter().all(|x| TAGS.iter().any(|(n, _)| n == x)), "{tool}: {t:?}");
            assert_ne!(t[0], "game", "{tool} is an engine tool: the authority table names it");
        }
    }

    #[test]
    fn a_tool_listed_beside_its_sibling_points_at_what_they_share() {
        let big = || Json::obj([("type", Json::from("object")), ("description", Json::from("x".repeat(200)))]);
        let spawn = def("world_spawn", Json::obj([("light", big()), ("name", Json::obj([("type", Json::from("string"))])), ("shape", big())]));
        let mut other = big();
        other.set("description", Json::from("y".repeat(200)));
        let set = def("world_set", Json::obj([("id", Json::obj([("type", Json::from("string"))])), ("light", big()), ("shape", other.clone())]));
        let slim = shared_listing(&set, &spawn).expect("light is shared");
        let props = slim.input_schema.get("properties").unwrap();
        // The shared property is a reference that keeps its type; the others are untouched, in order.
        assert_eq!(props.get("light").unwrap().to_string(), r#"{"type":"object","description":"as world_spawn's light"}"#);
        assert_eq!(props.get("shape"), Some(&other), "a property that differs is listed in full");
        assert_eq!(props.as_object().unwrap().iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), ["id", "light", "shape"]);
        assert!(slim.description.contains("Listed beside world_spawn"), "{}", slim.description);
        assert!(slim.input_schema.to_string().len() + 150 < set.input_schema.to_string().len());
        // Nothing shared, nothing changed.
        assert!(shared_listing(&def("a", Json::obj([("id", big())])), &spawn).is_none());
        assert!(shared_listing(&def("a", Json::obj([("name", Json::obj([("type", Json::from("string"))]))])), &spawn).is_none(), "small properties are repeated");
    }
}
