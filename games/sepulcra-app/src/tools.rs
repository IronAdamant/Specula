//! The game's MCP tools (`play_*`): agents play Sepulcra with the same commands as the mouse.
//!
//! Responses are bounded (lists are paged, logs capped) and errors are structured JSON with the
//! simulation's own rejection codes, so an agent acts on data rather than prose. Agents look with
//! the engine's `sight_frame` and drive time with its `time_control`.

use crate::game::{outcome_name, SepulcraGame, SPEEDS};
use sepulcra::error::DataError;
use sepulcra::script::{command_to_json, Player, Recording, ReplayError, ReplayStatus, RunEnd, Script};
use sepulcra::sim::Motion;
use sepulcra::{Command, Content, Rejection, State, TICK_HZ};
use specula::game::Ctx;
use specula_core::Json;
use specula_protocol::{ToolDef, ToolOutput};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// Built-in build orders (the same files the headless runner uses).
pub const LAYOUTS: [(&str, &str); 4] = [
    ("centre", include_str!("../../sepulcra/data/layouts/centre.json")),
    ("no_air", include_str!("../../sepulcra/data/layouts/no_air.json")),
    ("full_block", include_str!("../../sepulcra/data/layouts/full_block.json")),
    ("none", include_str!("../../sepulcra/data/layouts/none.json")),
];
const MAX_LIMIT: usize = 200;
const DEFAULT_LIMIT: usize = 50;
const MAX_BATCH: usize = 64;
const ACTIONS: [&str; 6] = ["place", "upgrade", "sell", "wall", "unwall", "call_wave"];

fn obj<const N: usize>(pairs: [(&str, Json); N]) -> Json {
    Json::obj(pairs)
}

fn prop(kind: &str, description: &str) -> Json {
    obj([("type", Json::from(kind)), ("description", Json::from(description))])
}

fn schema(props: Vec<(&str, Json)>, required: &[&str]) -> Json {
    obj([
        ("type", Json::from("object")),
        ("properties", Json::obj(props)),
        ("required", Json::arr(required.iter().map(|r| Json::from(*r)))),
        ("additionalProperties", Json::Bool(false)),
    ])
}

fn command_props() -> Vec<(&'static str, Json)> {
    vec![
        ("action", obj([("type", Json::from("string")), ("enum", Json::arr(ACTIONS.map(Json::from)))])),
        ("cell", prop("string", "Cell as \"face:col,row\", e.g. \"top:12,9\" or \"north:7,2\" (see play_map)")),
        ("stone", prop("string", "For place: warding, rune_pillar, brazier or sky_obelisk")),
    ]
}

pub fn defs() -> Vec<ToolDef> {
    let item = schema(command_props(), &["action"]);
    let mut batch = command_props();
    batch.push(("commands", obj([("type", Json::from("array")), ("items", item), ("description", Json::from("Up to 64 commands, applied in order; each is answered separately"))])));
    batch.push(("dry_run", prop("boolean", "Only check whether the command(s) would be accepted")));
    vec![
        ToolDef {
            name: "play_state".into(),
            description: "Sepulcra's state: wave, dust, tombstones, outcome, tick, timers, counts and the state hash. \
                          include lists any of enemies, stones, walls, waves, log (recent messages); lists are paged \
                          with offset/limit (default 50, max 200)."
                .into(),
            input_schema: schema(
                vec![
                    ("include", obj([("type", Json::from("array")), ("items", obj([("type", Json::from("string")), ("enum", Json::arr(["enemies", "stones", "walls", "waves", "log"].map(Json::from)))]))])),
                    ("offset", prop("integer", "First list item (default 0)")),
                    ("limit", prop("integer", "List items per list (default 50, max 200)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "play_command".into(),
            description: "Play a move, exactly as the mouse does: place a stone, upgrade, sell, wall, unwall (on a cell \
                          like \"top:12,9\") or call_wave. Applied at the current tick (also while paused) and recorded. \
                          A single command answers accepted or an error with the rejection code (not_enough_dust, \
                          occupied, not_buildable, not_wallable, enemy_in_the_way, ...); `commands` sends a batch."
                .into(),
            input_schema: schema(batch, &[]),
        },
        ToolDef {
            name: "play_map".into(),
            description: "The slab as text (top face with the cliffs unfolded around it) with stones, walls (=) and ground \
                          enemies drawn in, plus the legend. plain: true for the bare map."
                .into(),
            input_schema: schema(vec![("plain", prop("boolean", "Without stones, walls and enemies"))], &[]),
        },
        ToolDef {
            name: "play_new".into(),
            description: "Start a new run: seed (integer or decimal string), optional layout (a build order the game then \
                          follows: centre, no_air, full_block, none, or a JSON file path under the working directory) and \
                          optional data (a directory with modified slab_01/stones/enemies/waves.json)."
                .into(),
            input_schema: schema(
                vec![
                    ("seed", prop("string", "u64 seed (a number or a decimal string)")),
                    ("layout", prop("string", "Built-in layout name or a JSON file path")),
                    ("data", prop("string", "Content directory (default: the built-in data)")),
                ],
                &["seed"],
            ),
        },
        ToolDef {
            name: "play_record".into(),
            description: "Save the run so far as a self-checking recording (seed, content hash, every command with its tick, \
                          end tick and state hash) to a file under the engine's working directory (default \
                          recordings/sepulcra-<seed>-<tick>.json)."
                .into(),
            input_schema: schema(vec![("path", prop("string", "Relative path for the recording"))], &[]),
        },
        ToolDef {
            name: "play_replay".into(),
            description: "Replay a recording headlessly and verify it (end tick and state hash must match). load: true \
                          also makes the replayed end state the live game."
                .into(),
            input_schema: schema(
                vec![("path", prop("string", "Relative path of the recording")), ("load", prop("boolean", "Continue from the replayed state"))],
                &["path"],
            ),
        },
        ToolDef {
            name: "play_ui".into(),
            description: "The HUD as data and as buttons: the visible screens' widgets with an id or text (id, type, \
                          rect in physical px, text, enabled, focused), at most `limit` (1..200, default 60). `select` \
                          opens the panel of the stone or wall on a cell, as clicking it does. `press` activates a \
                          widget by id exactly as a click or a pad's accept would (refused when a player couldn't \
                          press it now, e.g. hidden, disabled or under a modal) and answers the action it sent. Ids: \
                          build_<warding|pillar|brazier|obelisk|wall> (the build bar; _on while selected), call_wave, \
                          pause_button, resume_button, speed_button, menu_button; on a selected stone or wall: upgrade, \
                          sell, unwall, deselect; in the pause menu: resume, settings, controls, restart (then \
                          confirm_yes / confirm_no); after the run: new_run."
                .into(),
            input_schema: schema(
                vec![
                    ("select", prop("string", "Open the panel of the stone or wall on this cell (as clicking it does), e.g. \"top:12,9\"")),
                    ("press", prop("string", "Widget id to activate (after `select`)")),
                    ("limit", prop("integer", "Widgets to list, 1..200 (default 60)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "play_control".into(),
            description: "The game's own pause (the player's P key: the simulation stops, building still works) and speed \
                          (1, 2 or 4 simulation steps per tick). For agents' time, prefer the engine's time_control."
                .into(),
            input_schema: schema(vec![("paused", prop("boolean", "Pause or resume")), ("speed", prop("integer", "1, 2 or 4"))], &[]),
        },
    ]
}

/// A structured error: `{"error": code, "message": ..., details...}`.
fn error(code: &str, message: impl Into<String>, details: Vec<(&str, Json)>) -> ToolOutput {
    let mut pairs = vec![("error", Json::from(code)), ("message", Json::from(message.into()))];
    pairs.extend(details);
    ToolOutput::error(Json::obj(pairs).to_pretty())
}

fn data_error(e: &DataError) -> ToolOutput {
    error(
        e.code.as_str(),
        e.to_string(),
        vec![("file", Json::from(e.file.as_str())), ("field", Json::from(e.field.as_str())), ("reason", Json::from(e.reason.as_str()))],
    )
}

fn rejection_json(r: &Rejection) -> Vec<(&'static str, Json)> {
    let mut d = vec![("code", Json::from(r.code()))];
    match *r {
        Rejection::NotEnoughDust { need, have } => {
            d.push(("need", Json::from(need)));
            d.push(("have", Json::from(have)));
        }
        Rejection::EnemyInTheWay { enemy } => d.push(("enemy", Json::from(enemy))),
        _ => {}
    }
    d
}

fn check_keys(args: &Json, allowed: &[&str]) -> Result<(), ToolOutput> {
    for (k, _) in args.as_object().unwrap_or(&[]) {
        if !allowed.contains(&k.as_str()) {
            return Err(error("bad_argument", format!("unknown argument '{k}' (allowed: {})", allowed.join(", ")), vec![("field", Json::from(k.as_str()))]));
        }
    }
    Ok(())
}

fn str_arg<'a>(args: &'a Json, key: &str) -> Result<Option<&'a str>, ToolOutput> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => v.as_str().map(Some).ok_or_else(|| error("bad_argument", format!("'{key}' must be a string"), vec![("field", Json::from(key))])),
    }
}

fn bool_arg(args: &Json, key: &str) -> Result<Option<bool>, ToolOutput> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => v.as_bool().map(Some).ok_or_else(|| error("bad_argument", format!("'{key}' must be true or false"), vec![("field", Json::from(key))])),
    }
}

fn uint_arg(args: &Json, key: &str, max: u64) -> Result<Option<u64>, ToolOutput> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => match v.as_u64() {
            Some(n) if n <= max => Ok(Some(n)),
            _ => Err(error("bad_argument", format!("'{key}' must be an integer 0..{max}"), vec![("field", Json::from(key))])),
        },
    }
}

/// A u64 seed from a number (exact up to 2^53) or a decimal string.
fn seed_arg(args: &Json) -> Result<u64, ToolOutput> {
    let bad = || error("bad_argument", "'seed' must be a u64: an integer below 2^53 or a decimal string", vec![("field", Json::from("seed"))]);
    match args.get("seed") {
        Some(Json::Str(s)) => s.trim().parse::<u64>().map_err(|_| bad()),
        Some(Json::Num(n)) if n.fract() == 0.0 && *n >= 0.0 && *n < 9_007_199_254_740_992.0 => Ok(*n as u64),
        _ => Err(bad()),
    }
}

/// Windows device names: a file called `NUL.json` or `CON` is a device, not a file.
const RESERVED: [&str; 24] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", "CONIN$", "CONOUT$",
];

/// A path for people: without Windows' `\\?\` prefix that canonical paths carry.
fn show(p: &Path) -> String {
    let s = p.display().to_string();
    s.strip_prefix(r"\\?\").map_or(s.clone(), str::to_owned)
}

fn bad_path(rel: &str, why: &str) -> ToolOutput {
    error("bad_path", format!("'{rel}': {why}"), vec![("path", Json::from(rel))])
}

/// A file path inside the working directory, checked lexically and on disk: relative, no `..`,
/// no `:` (drives, alternate data streams), no device names, and after following junctions and
/// links it must still resolve inside the working directory. `write`: the parent folders are
/// created (only once the deepest existing one is known to be inside).
pub(crate) fn safe_path(rel: &str, write: bool) -> Result<PathBuf, ToolOutput> {
    let p = Path::new(rel);
    if rel.is_empty() || !p.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir)) {
        return Err(bad_path(rel, "must be a relative path inside the working directory (no '..', no drive or root)"));
    }
    if rel.contains(':') {
        return Err(bad_path(rel, "':' is not allowed (drives and alternate data streams)"));
    }
    for c in p.components() {
        if let Component::Normal(name) = c {
            let name = name.to_string_lossy();
            // Windows ignores trailing dots and spaces, and a device name keeps its meaning with
            // any extension (NUL.json is NUL).
            let stem = name.split('.').next().unwrap_or("").trim_end_matches([' ', '.']).to_ascii_uppercase();
            if RESERVED.contains(&stem.as_str()) {
                return Err(bad_path(rel, &format!("'{name}' is a Windows device name")));
            }
        }
    }
    let io = |e: std::io::Error| error("io", format!("{rel}: {e}"), vec![("path", Json::from(rel))]);
    let cwd = std::env::current_dir().and_then(|d| d.canonicalize()).map_err(io)?;
    let inside = |real: &Path| -> Result<(), ToolOutput> {
        if real.starts_with(&cwd) {
            Ok(())
        } else {
            Err(bad_path(rel, "resolves outside the working directory (through a link or junction)"))
        }
    };
    let full = cwd.join(p);
    if !write {
        let real = full.canonicalize().map_err(io)?;
        inside(&real)?;
        return Ok(real);
    }
    let parent = full.parent().unwrap_or(&cwd).to_path_buf();
    let mut existing = parent.as_path();
    while !existing.exists() {
        existing = existing.parent().unwrap_or(&cwd);
    }
    inside(&existing.canonicalize().map_err(io)?)?;
    std::fs::create_dir_all(&parent).map_err(io)?;
    let real_parent = parent.canonicalize().map_err(io)?;
    inside(&real_parent)?;
    let target = real_parent.join(full.file_name().unwrap_or_default());
    if target.exists() {
        inside(&target.canonicalize().map_err(io)?)?;
    }
    Ok(target)
}

/// A command from `item`'s action, cell and stone; `allowed` are the keys `item` may have.
fn parse_command(content: &Content, item: &Json, allowed: &[&str]) -> Result<Command, ToolOutput> {
    check_keys(item, allowed)?;
    let action = str_arg(item, "action")?.ok_or_else(|| error("bad_argument", format!("'action' is required: one of {}", ACTIONS.join(", ")), vec![("field", Json::from("action"))]))?;
    let cell = || -> Result<sepulcra::CellId, ToolOutput> {
        let name = str_arg(item, "cell")?.ok_or_else(|| error("bad_argument", format!("'{action}' needs a 'cell' like \"top:12,9\""), vec![("field", Json::from("cell"))]))?;
        content.map.cell_ref(name).ok_or_else(|| {
            error("unknown_cell", format!("no cell {name:?}: expected \"face:col,row\" (faces: top, north, east, west), see play_map"), vec![("cell", Json::from(name))])
        })
    };
    Ok(match action {
        "place" => {
            let id = str_arg(item, "stone")?.ok_or_else(|| error("bad_argument", "'place' needs a 'stone'", vec![("field", Json::from("stone"))]))?;
            let stone = content.stone_kind(id).ok_or_else(|| {
                let ids: Vec<Json> = content.stones.iter().map(|s| Json::from(s.id.as_str())).collect();
                error("unknown_stone", format!("no stone {id:?}"), vec![("stones", Json::Arr(ids))])
            })?;
            Command::PlaceStone { cell: cell()?, stone }
        }
        "upgrade" => Command::Upgrade { cell: cell()? },
        "sell" => Command::Sell { cell: cell()? },
        "wall" => Command::PlaceWall { cell: cell()? },
        "unwall" => Command::RemoveWall { cell: cell()? },
        "call_wave" => Command::CallWave,
        other => return Err(error("bad_argument", format!("unknown action '{other}' (one of {})", ACTIONS.join(", ")), vec![("field", Json::from("action"))])),
    })
}

pub fn call(game: &mut SepulcraGame, ctx: &mut Ctx, name: &str, args: &Json) -> ToolOutput {
    let result = match name {
        "play_state" => state(game, args),
        "play_command" => command(game, ctx, args),
        "play_map" => map(game, args),
        "play_new" => new(game, ctx, args),
        "play_record" => record(game, args),
        "play_replay" => replay(game, ctx, args),
        "play_control" => control(game, ctx, args),
        "play_ui" => ui(game, ctx, args),
        other => Err(error("unknown_tool", format!("no game tool '{other}'"), vec![])),
    };
    result.unwrap_or_else(|e| e)
}

fn page<T>(items: impl Iterator<Item = T>, offset: usize, limit: usize, f: impl Fn(T) -> Json) -> Json {
    let mut total = 0;
    let mut out = Vec::new();
    for (i, it) in items.enumerate() {
        total += 1;
        if i >= offset && out.len() < limit {
            out.push(f(it));
        }
    }
    obj([("total", Json::from(total)), ("offset", Json::from(offset)), ("items", Json::Arr(out))])
}

fn state(game: &SepulcraGame, args: &Json) -> Result<ToolOutput, ToolOutput> {
    check_keys(args, &["include", "offset", "limit"])?;
    let offset = uint_arg(args, "offset", 1 << 20)?.unwrap_or(0) as usize;
    let limit = (uint_arg(args, "limit", MAX_LIMIT as u64)?.unwrap_or(DEFAULT_LIMIT as u64) as usize).clamp(1, MAX_LIMIT);
    let s: &State = &game.state;
    let content = s.content();
    let map = &content.map;
    let mut by_kind: Vec<(String, u32)> = content.enemies.iter().map(|e| (e.id.clone(), 0)).collect();
    for e in s.enemies() {
        by_kind[usize::from(e.kind.0)].1 += 1;
    }
    let mut out = game.status_json();
    out.set("seed", Json::from(s.seed().to_string()));
    out.set("seconds", Json::from(f64::from(s.tick()) / f64::from(TICK_HZ)));
    out.set("next_wave_in_s", s.next_wave_tick().map_or(Json::Null, |t| Json::from(f64::from(t.saturating_sub(s.tick())) / f64::from(TICK_HZ))));
    out.set("tombstones_max", Json::from(content.rules.tombstones));
    out.set("enemies_by_kind", Json::obj(by_kind.into_iter().map(|(k, n)| (k, Json::from(n)))));
    out.set("pending_spawns", Json::from(s.pending_spawns()));
    out.set("stones", Json::from(s.stones().count()));
    out.set("walls", Json::from(s.walls().count()));
    out.set("projectiles", Json::from(s.projectiles().len()));
    out.set("commands_recorded", Json::from(game.journal.len()));
    out.set("layout", game.autoplay.as_ref().map_or(Json::Null, |(n, _)| Json::from(n.as_str())));
    out.set("state_hash", Json::from(format!("{:016x}", s.hash())));
    out.set("content_hash", Json::from(format!("{:016x}", content.hash)));
    let mut presentation = game.presentation_json();
    if let Some(scene) = &game.scene {
        presentation.set("scene", scene.counts());
    }
    out.set("presentation", presentation);
    let include: Vec<&str> = match args.get("include") {
        None | Some(Json::Null) => Vec::new(),
        Some(Json::Arr(a)) => a.iter().filter_map(Json::as_str).collect(),
        Some(_) => return Err(error("bad_argument", "'include' must be a list", vec![("field", Json::from("include"))])),
    };
    for what in include {
        let list = match what {
            "enemies" => page(s.enemies().iter(), offset, limit, |e| {
                let [x, y, z] = e.world_pos_f32();
                let r = |v: f32| Json::from((v * 100.0).round() / 100.0);
                let mut j = obj([
                    ("id", Json::from(e.id)),
                    ("kind", Json::from(content.enemy(e.kind).id.as_str())),
                    ("hp", Json::from(e.hp as f64 / 1000.0)),
                    ("max_hp", Json::from(e.max_hp as f64 / 1000.0)),
                    ("pos", Json::arr([r(x), r(y), r(z)])),
                    ("remaining_m", Json::from(e.remaining as f64 / 1000.0)),
                ]);
                j.set("cell", e.cell().map_or(Json::Null, |c| Json::from(map.cell_name(c))));
                j.set("flying", Json::from(matches!(e.motion, Motion::Air { .. })));
                if e.is_slowed(s.tick()) {
                    j.set("slowed", Json::Bool(true));
                }
                if let Some(w) = e.attacking {
                    j.set("attacking_wall", Json::from(map.cell_name(w)));
                }
                j
            }),
            "stones" => page(s.stones(), offset, limit, |(c, st)| {
                let def = content.stone(st.kind);
                let next = def.tiers.get(usize::from(st.tier) + 1).map_or(Json::Null, |t| Json::from(t.cost));
                obj([
                    ("cell", Json::from(map.cell_name(c))),
                    ("stone", Json::from(def.id.as_str())),
                    ("tier", Json::from(u32::from(st.tier) + 1)),
                    ("range_m", Json::from(def.tiers[usize::from(st.tier)].range as f64 / 1000.0)),
                    ("upgrade_cost", next),
                    ("sell_refund", Json::from(st.invested * content.rules.refund_percent / 100)),
                ])
            }),
            "walls" => page(s.walls(), offset, limit, |(c, w)| {
                obj([("cell", Json::from(map.cell_name(c))), ("hp", Json::from(w.hp as f64 / 1000.0)), ("max_hp", Json::from(w.max_hp as f64 / 1000.0))])
            }),
            "waves" => page(s.wave_stats().iter().enumerate(), offset, limit, |(i, w)| {
                obj([
                    ("wave", Json::from(i + 1)),
                    ("started_at", w.started_at.map_or(Json::Null, Json::from)),
                    ("cleared_at", w.cleared_at.map_or(Json::Null, Json::from)),
                    ("spawned", Json::from(w.spawned)),
                    ("killed", Json::from(w.killed)),
                    ("leaked", Json::from(w.leaked)),
                    ("tombstones_lost", Json::from(w.tombstones_lost)),
                    ("alive", Json::from(w.alive)),
                    ("pending", Json::from(w.pending)),
                ])
            }),
            "log" => page(game.log.iter().rev(), offset, limit, |(t, m)| obj([("tick", Json::from(*t)), ("message", Json::from(m.as_str()))])),
            other => return Err(error("bad_argument", format!("cannot include '{other}' (enemies, stones, walls, waves, log)"), vec![("field", Json::from("include"))])),
        };
        out.set(what, list);
    }
    Ok(ToolOutput::text(out.to_pretty()))
}

fn command(game: &mut SepulcraGame, ctx: &mut Ctx, args: &Json) -> Result<ToolOutput, ToolOutput> {
    const KEYS: [&str; 5] = ["action", "cell", "stone", "commands", "dry_run"];
    check_keys(args, &KEYS)?;
    let dry = bool_arg(args, "dry_run")?.unwrap_or(false);
    let content = game.content();
    // A dry run plays the commands on a copy of the state, so a batch is judged exactly as it
    // would be for real (an upgrade after a placement in the same batch, dust running out).
    let mut shadow = dry.then(|| game.state.clone());
    let mut scratch = Vec::new();
    let mut one = |game: &mut SepulcraGame, ctx: &mut Ctx, cmd: Command| -> Result<Json, Rejection> {
        let tick = game.state.tick();
        match &mut shadow {
            Some(s) => {
                scratch.clear();
                s.apply(&cmd, &mut scratch)?;
            }
            None => game.command(ctx.world, cmd)?,
        }
        let mut j = command_to_json(&cmd, &content);
        j.set("tick", Json::from(tick));
        Ok(j)
    };
    let answer = |game: &SepulcraGame, mut j: Json| {
        j.set("dust", Json::from(game.state.dust()));
        if dry {
            j.set("dry_run", Json::Bool(true));
        }
        ToolOutput::text(j.to_pretty())
    };
    match args.get("commands") {
        None | Some(Json::Null) => {
            let cmd = parse_command(&content, args, &KEYS)?;
            match one(game, ctx, cmd) {
                Ok(j) => Ok(answer(game, obj([("accepted", Json::Bool(true)), ("command", j)]))),
                Err(r) => {
                    let msg = game.explain(&cmd, r);
                    let mut d = rejection_json(&r);
                    d.push(("command", command_to_json(&cmd, &content)));
                    d.push(("tick", Json::from(game.state.tick())));
                    d.push(("dust", Json::from(game.state.dust())));
                    Err(error("rejected", msg, d))
                }
            }
        }
        Some(Json::Arr(items)) => {
            if ["action", "cell", "stone"].iter().any(|k| args.get(k).is_some()) {
                return Err(error("bad_argument", "give either one command (action, cell, stone) or 'commands', not both", vec![]));
            }
            if items.len() > MAX_BATCH {
                return Err(error("too_many", format!("at most {MAX_BATCH} commands per call"), vec![("max", Json::from(MAX_BATCH))]));
            }
            // Parse everything first: a malformed batch applies nothing.
            let mut cmds = Vec::new();
            for (i, item) in items.iter().enumerate() {
                cmds.push(parse_command(&content, item, &["action", "cell", "stone"]).map_err(|e| {
                    let text = e.content.first().map(|c| match c {
                        specula_protocol::Content::Text(t) => t.clone(),
                        _ => String::new(),
                    });
                    error("bad_command", format!("commands[{i}] is malformed; nothing was applied"), vec![("index", Json::from(i)), ("cause", Json::parse(&text.unwrap_or_default()).unwrap_or(Json::Null))])
                })?);
            }
            let (mut accepted, mut results) = (0, Vec::new());
            for cmd in cmds {
                results.push(match one(game, ctx, cmd) {
                    Ok(j) => {
                        accepted += 1;
                        obj([("accepted", Json::Bool(true)), ("command", j)])
                    }
                    Err(r) => {
                        let mut d = vec![("accepted", Json::Bool(false)), ("command", command_to_json(&cmd, &content))];
                        d.extend(rejection_json(&r));
                        d.push(("message", Json::from(game.explain(&cmd, r))));
                        Json::obj(d)
                    }
                });
            }
            let total = results.len();
            Ok(answer(game, obj([("accepted", Json::from(accepted)), ("rejected", Json::from(total - accepted)), ("results", Json::Arr(results))])))
        }
        Some(_) => Err(error("bad_argument", "'commands' must be a list", vec![("field", Json::from("commands"))])),
    }
}

fn map(game: &SepulcraGame, args: &Json) -> Result<ToolOutput, ToolOutput> {
    check_keys(args, &["plain"])?;
    let plain = bool_arg(args, "plain")?.unwrap_or(false);
    let content = game.state.content();
    let text = if plain { content.map.ascii() } else { game.state.ascii() };
    let stones: Vec<String> = content.stones.iter().map(|s| format!("{} {}", s.glyph, s.id)).collect();
    let enemies: Vec<String> = content.enemies.iter().map(|e| format!("{} {}", e.glyph, e.id)).collect();
    let legend = format!(
        "top:   # fragment (stones)  . fissure lane (walls)  j jump point  G graveyard  S gorge spawn  x void\n\
         cliff: - rock  | climb lane  # ledge (stones)  B cliff-base spawn\n\
         drawn: stones {}; = wall; enemies {} (a digit = several on one cell)\n\
         cells are named face:col,row; the top face's col runs west to east, row north to south; cliffs' row 0 is at the rim",
        stones.join(", "),
        enemies.join(", ")
    );
    Ok(ToolOutput::text(format!("{text}\n{legend}")))
}

fn new(game: &mut SepulcraGame, ctx: &mut Ctx, args: &Json) -> Result<ToolOutput, ToolOutput> {
    check_keys(args, &["seed", "layout", "data"])?;
    let seed = seed_arg(args)?;
    let content = match str_arg(args, "data")? {
        Some(dir) => Arc::new(Content::load_dir(&safe_path(dir, false)?).map_err(|e| data_error(&e))?),
        None => game.content(),
    };
    let autoplay = match str_arg(args, "layout")? {
        None => None,
        Some(name) => {
            let (text, file) = match LAYOUTS.iter().find(|(n, _)| *n == name) {
                Some((n, t)) => ((*t).to_owned(), format!("layouts/{n}.json")),
                None => {
                    let path = safe_path(name, false)?;
                    let text = std::fs::read_to_string(&path).map_err(|e| {
                        let names: Vec<Json> = LAYOUTS.iter().map(|(n, _)| Json::from(*n)).collect();
                        error("io", format!("layout '{name}' is not built in and can't be read: {e}"), vec![("layouts", Json::Arr(names))])
                    })?;
                    (text, name.to_owned())
                }
            };
            let script = Script::parse(&text, &content, &file).map_err(|e| data_error(&e))?;
            Some((name.to_owned(), Player::new(script)))
        }
    };
    game.restart(ctx.world, ctx.agent, content, seed, autoplay);
    let mut out = game.status_json();
    out.set("seed", Json::from(seed.to_string()));
    out.set("layout", game.autoplay.as_ref().map_or(Json::Null, |(n, _)| Json::from(n.as_str())));
    out.set("content_hash", Json::from(format!("{:016x}", game.state.content().hash)));
    Ok(ToolOutput::text(out.to_pretty()))
}

/// The run so far as a recording that verifies. Commands applied at the current tick haven't
/// been stepped yet, so then the recording ends one tick later (the state stepped once more on a
/// copy: applying then stepping is exactly what a replay does at that tick).
pub fn recording(game: &SepulcraGame) -> (Recording, bool) {
    let s = &game.state;
    let content = s.content();
    let unstepped = game.journal.last().is_some_and(|(t, _)| *t >= s.tick());
    let end = if unstepped {
        let mut copy = s.clone();
        copy.step(&[]);
        RunEnd { tick: copy.tick(), hash: copy.hash() }
    } else {
        RunEnd { tick: s.tick(), hash: s.hash() }
    };
    (Recording { seed: s.seed(), content_hash: content.hash, journal: game.journal.clone(), end: Some(end) }, unstepped)
}

fn record(game: &SepulcraGame, args: &Json) -> Result<ToolOutput, ToolOutput> {
    check_keys(args, &["path"])?;
    let default = format!("recordings/sepulcra-{}-{}.json", game.state.seed(), game.state.tick());
    let rel = str_arg(args, "path")?.map_or(default, str::to_owned);
    let path = safe_path(&rel, true)?;
    let (rec, advanced) = recording(game);
    let text = rec.to_json(game.state.content()).to_pretty();
    std::fs::write(&path, text).map_err(|e| error("io", format!("{}: {e}", show(&path)), vec![]))?;
    let end = rec.end.expect("set above");
    let mut out = obj([
        ("path", Json::from(show(&path))),
        ("seed", Json::from(rec.seed.to_string())),
        ("commands", Json::from(rec.journal.len())),
        ("end_tick", Json::from(end.tick)),
        ("end_hash", Json::from(format!("{:016x}", end.hash))),
    ]);
    if advanced {
        out.set("note", Json::from("commands were applied at the current tick, so the recording ends one tick later (where they take effect)"));
    }
    Ok(ToolOutput::text(out.to_pretty()))
}

fn replay(game: &mut SepulcraGame, ctx: &mut Ctx, args: &Json) -> Result<ToolOutput, ToolOutput> {
    check_keys(args, &["path", "load"])?;
    let rel = str_arg(args, "path")?.ok_or_else(|| error("bad_argument", "'path' is required", vec![("field", Json::from("path"))]))?;
    let load = bool_arg(args, "load")?.unwrap_or(false);
    let path = safe_path(rel, false)?;
    let text = std::fs::read_to_string(&path).map_err(|e| error("io", format!("{}: {e}", show(&path)), vec![("path", Json::from(rel))]))?;
    let content = game.content();
    let rec = Recording::from_json(&text, &content, rel).map_err(|e| data_error(&e))?;
    let replayed = rec.verify(Arc::clone(&content)).map_err(|e| {
        let code = match e {
            ReplayError::Rejected { .. } => "replay_rejected",
            ReplayError::NotApplied { .. } => "replay_not_applied",
            ReplayError::Mismatch { .. } => "replay_mismatch",
        };
        let mut d = vec![];
        match e {
            ReplayError::Rejected { tick, command, reason } => {
                d.push(("tick", Json::from(tick)));
                d.push(("command", command_to_json(&command, &content)));
                d.extend(rejection_json(&reason));
            }
            ReplayError::NotApplied { tick, ended_at } => {
                d.push(("tick", Json::from(tick)));
                d.push(("ended_at", Json::from(ended_at)));
            }
            ReplayError::Mismatch { expected, got } => {
                d.push(("expected_tick", Json::from(expected.tick)));
                d.push(("expected_hash", Json::from(format!("{:016x}", expected.hash))));
                d.push(("got_tick", Json::from(got.tick)));
                d.push(("got_hash", Json::from(format!("{:016x}", got.hash))));
            }
        }
        error(code, e.to_string(), d)
    })?;
    let s = &replayed.state;
    let out = obj([
        ("status", Json::from(match replayed.status {
            ReplayStatus::Verified => "verified",
            ReplayStatus::Unverified => "unverified",
        })),
        ("seed", Json::from(rec.seed.to_string())),
        ("commands", Json::from(rec.journal.len())),
        ("end_tick", Json::from(s.tick())),
        ("end_hash", Json::from(format!("{:016x}", s.hash()))),
        ("outcome", Json::from(outcome_name(s.outcome()))),
        ("wave", Json::from(u32::from(s.wave()))),
        ("tombstones", Json::from(s.tombstones())),
        ("loaded", Json::from(load)),
    ]);
    if load {
        game.load(ctx.world, ctx.agent, replayed.state, rec.journal);
        game.say(format!("Loaded replay {rel}"));
    }
    Ok(ToolOutput::text(out.to_pretty()))
}

fn control(game: &mut SepulcraGame, ctx: &mut Ctx, args: &Json) -> Result<ToolOutput, ToolOutput> {
    check_keys(args, &["paused", "speed"])?;
    let speed = uint_arg(args, "speed", 64)?;
    if let Some(sp) = speed {
        if !SPEEDS.contains(&(sp as u32)) {
            return Err(error("bad_argument", "'speed' must be 1, 2 or 4", vec![("field", Json::from("speed"))]));
        }
    }
    if let Some(p) = bool_arg(args, "paused")? {
        game.set_paused(ctx.world, p);
    }
    if let Some(sp) = speed {
        game.set_speed(ctx.world, sp as u32);
    }
    Ok(ToolOutput::text(game.status_json().to_pretty()))
}

fn ui(game: &mut SepulcraGame, ctx: &mut Ctx, args: &Json) -> Result<ToolOutput, ToolOutput> {
    check_keys(args, &["select", "press", "limit"])?;
    let limit = uint_arg(args, "limit", MAX_LIMIT as u64)?.unwrap_or(60) as usize;
    if limit == 0 {
        return Err(error("bad_argument", format!("'limit' must be an integer 1..{MAX_LIMIT}"), vec![("field", Json::from("limit"))]));
    }
    let mut out = obj([]);
    if let Some(name) = str_arg(args, "select")? {
        let content = game.content();
        let cell = content.map.cell_ref(name).ok_or_else(|| error("unknown_cell", format!("no cell {name:?}"), vec![("cell", Json::from(name))]))?;
        if !game.select_cell(cell) {
            return Err(error("nothing_there", format!("no stone or wall on {name}"), vec![("cell", Json::from(name))]));
        }
        out.set("selected", Json::from(name));
    }
    if let Some(id) = str_arg(args, "press")? {
        match game.press(ctx.world, ctx.agent, id) {
            Ok(action) => {
                out.set("pressed", Json::from(id));
                out.set("action", action.map_or(Json::Null, Json::from));
            }
            Err(e) => return Err(error("ui_refused", e, vec![("widget", Json::from(id))])),
        }
    }
    out.set("status", game.status_json());
    out.set("ui", game.ui_tree(limit));
    // Compact: a full HUD is dozens of widgets.
    Ok(ToolOutput::text(out.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As the engine's own tools: no runs of spaces (a missing line-continuation backslash) in
    /// what agents read.
    #[test]
    fn tool_texts_have_no_runs_of_spaces() {
        for t in defs() {
            let schema = t.input_schema.to_string();
            for text in [&t.description, &schema] {
                assert!(!text.contains("   "), "{}: a run of spaces in {text:?}", t.name);
            }
        }
    }
}
