//! Save games in the engine (docs/design/SAVES.md): `save_write`, `save_read`, `save_list`,
//! `save_delete`, and the hosted game's player-facing Save and Load ([`GameRequest`]).
//!
//! A save is the simulation's whole state at a tick boundary: the world (as world files), the
//! hosted game's own state, the module host's stores and RNG positions, physics' hidden state and
//! the clock. Loading it makes the save's tick the engine's, so the loaded run continues with the
//! same `world_hash` and subsystem hashes as the run that saved (`desync_compare` shows it).
//! Saves are files `<saves dir>/<name>.specsave` (`--saves`, default `./saves`), written to a
//! temporary file and renamed, so a crash leaves the old save or the new one.

use crate::game::GameRequest;
use crate::journal::Writer;
use crate::savegame::{self, SaveError, Section, EXTENSION};
use crate::tools::{bool_arg, check_keys, int_arg, string_arg};
use crate::Engine;
use specula_core::hash::hash_str;
use specula_core::Json;
use specula_protocol::{ToolDef, ToolOutput};
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Labels up to this many characters.
const MAX_LABEL: usize = 200;
/// save_list pages.
const MAX_LIST: u64 = 100;
/// Files save_list looks at (the rest are counted).
const MAX_SCANNED: usize = 4096;
/// Save writers running at once (each holds a copy of the world).
const MAX_WRITERS: usize = 2;
/// Requests a game may hand the engine per frame (more are answered `too_many_requests`).
const MAX_GAME_REQUESTS: usize = 16;

/// A header string as shown in answers: a string, clipped to `max` characters.
fn clip(v: Option<&Json>, max: usize) -> Json {
    match v.and_then(Json::as_str) {
        Some(s) => Json::from(s.chars().take(max).collect::<String>()),
        None => Json::Null,
    }
}

/// A header's fields as answers show them: known keys only, typed and clipped.
fn header_summary(h: &Json) -> Vec<(&'static str, Json)> {
    let num = |k: &str| h.get(k).and_then(Json::as_u64).map_or(Json::Null, Json::from);
    vec![
        ("label", clip(h.get("label"), MAX_LABEL)),
        ("game", clip(h.get("game"), 64)),
        ("tick", num("tick")),
        ("tick_hz", num("tick_hz")),
        ("entities", num("entities")),
        ("created_by", clip(h.get("created_by"), 128)),
        ("created_unix_s", num("created_unix_s")),
        ("engine", clip(h.get("engine"), 32)),
        ("world_hash", clip(h.get("world_hash"), 16)),
    ]
}

pub enum SaveCall {
    Write { name: String, label: Option<String>, overwrite: bool, force: bool },
    Read { name: String, force: bool, inspect: bool },
    List { cursor: usize, limit: usize },
    Delete { name: String, force: bool },
}

impl SaveCall {
    /// Saves read or write files and the whole world.
    pub fn class(&self) -> crate::schedule::Class {
        match self {
            SaveCall::List { .. } => crate::schedule::Class::Read,
            _ => crate::schedule::Class::Disk,
        }
    }
}

/// Save names: like world names, 1..64 of `A-Z a-z 0-9 _ -` (they are file names).
pub fn validate_save_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 64 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(format!("save name '{name}' must be 1..64 characters of A-Z a-z 0-9 _ - (e.g. \"slot_1\", \"before-wave-4\")"));
    }
    if crate::persist::validate_world_name(name).is_err() {
        return Err(format!("save name '{name}' is reserved on some systems (a device name); pick another"));
    }
    Ok(())
}

fn name_of(args: &Json) -> Result<String, String> {
    let name = string_arg(args, "name")?.ok_or("'name' is required: the save's name, e.g. \"slot_1\"")?;
    validate_save_name(name)?;
    Ok(name.to_owned())
}

pub fn parse(name: &str, args: &Json) -> Option<Result<SaveCall, String>> {
    let parsed = match name {
        "save_write" => (|| {
            check_keys(args, name, &[&["name", "label", "overwrite", "force"]])?;
            let label = string_arg(args, "label")?.map(str::to_owned);
            if label.as_ref().is_some_and(|l| l.chars().count() > MAX_LABEL) {
                return Err(format!("'label' is at most {MAX_LABEL} characters"));
            }
            Ok(SaveCall::Write {
                name: name_of(args)?,
                label,
                overwrite: bool_arg(args, "overwrite")?.unwrap_or(false),
                force: bool_arg(args, "force")?.unwrap_or(false),
            })
        })(),
        "save_read" => (|| {
            check_keys(args, name, &[&["name", "force", "inspect"]])?;
            Ok(SaveCall::Read { name: name_of(args)?, force: bool_arg(args, "force")?.unwrap_or(false), inspect: bool_arg(args, "inspect")?.unwrap_or(false) })
        })(),
        "save_list" => (|| {
            check_keys(args, name, &[&["cursor", "limit"]])?;
            Ok(SaveCall::List {
                cursor: int_arg(args, "cursor", 0, MAX_SCANNED as u64)?.unwrap_or(0) as usize,
                limit: int_arg(args, "limit", 1, MAX_LIST)?.unwrap_or(20) as usize,
            })
        })(),
        "save_delete" => (|| {
            check_keys(args, name, &[&["name", "force"]])?;
            Ok(SaveCall::Delete { name: name_of(args)?, force: bool_arg(args, "force")?.unwrap_or(false) })
        })(),
        _ => return None,
    };
    Some(parsed)
}

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([
            ("type", Json::from("object")),
            ("properties", Json::obj(props)),
            ("required", Json::arr(required.iter().map(|r| Json::from(*r)))),
        ])
    };
    let def = |name: &str, description: &str, input_schema: Json| ToolDef { name: name.into(), description: description.into(), input_schema };
    let name = || ("name", t("string", "The save's name: 1..64 of A-Z a-z 0-9 _ - (a file <saves dir>/<name>.specsave)"));
    vec![
        def(
            "save_write",
            "Save the game: the simulation's whole state at this tick (the world, the hosted game's state, game modules' \
             state stores and RNG positions, physics' hidden state: warm starts, sleep timers, contact memory) in one \
             versioned, checksummed file. save_read of it continues bit-identically: the same world_hash and subsystem \
             hashes tick for tick as this run (check with desync_trace + desync_compare). Physics is synced with the \
             world first (as the next tick would). Returns the tick, hashes, section sizes and timings. An existing save \
             is only replaced with overwrite: true (another agent's only with force: true too). At most two saves are \
             written at once (busy: call again).",
            schema(vec![name(), ("label", t("string", "A note shown by save_list (\"before wave 4\"), up to 200 characters")), ("overwrite", t("boolean", "Replace an existing save of that name")), ("force", t("boolean", "Replace another agent's save (lead authority only)"))], &["name"]),
        ),
        def(
            "save_read",
            "Load a save game: replaces the world, the hosted game's state, module state and physics with the save's, and \
             sets the clock to the save's tick, so the run continues exactly as the one that saved. Refused while other \
             agents hold claims (it replaces everything they are working on), when the live world has unsaved changes \
             (pass force: true to discard them), or when the save is from another game or tick rate. Returns hashes now \
             vs saved (each must match) and notes (e.g. a module that was running isn't loaded here). inspect: true only \
             reads and checks the file (header, sections, checksums) without loading.",
            schema(vec![name(), ("force", t("boolean", "Discard unsaved world changes")), ("inspect", t("boolean", "Check and describe the save without loading it"))], &["name"]),
        ),
        def(
            "save_list",
            "The save games there are, newest first: name, label, game, tick, who saved it, size, and whether the file \
             reads. Paged.",
            schema(vec![("cursor", t("integer", "next_cursor from the previous page")), ("limit", t("integer", "Page size, 1..100 (default 20)"))], &[]),
        ),
        def(
            "save_delete",
            "Delete a save game. Only the agent that wrote it may (a lead may with force: true).",
            schema(vec![name(), ("force", t("boolean", "Delete another agent's save (lead authority only)"))], &["name"]),
        ),
    ]
}

fn out(j: Json) -> ToolOutput {
    ToolOutput::text(j.to_string())
}

fn fail(e: &SaveError) -> ToolOutput {
    ToolOutput::error(e.to_json().to_pretty())
}

fn hex(h: u64) -> String {
    format!("{h:016x}")
}

fn io(path: &std::path::Path, e: std::io::Error) -> SaveError {
    let code = if e.kind() == std::io::ErrorKind::NotFound { "not_found" } else { "io" };
    SaveError::new(code, format!("{}: {e}", path.display())).with("file", path.display().to_string())
}

/// The owner a save's header records: a hash of the writer's identity key (agent_hello's
/// identity, the same one claims go by), never the key itself.
fn owner_hash(key: &str) -> String {
    hex(hash_str(&format!("specula-save-owner:{key}")))
}

/// Reads the first bytes of a save: enough for its header (save_list never reads whole files).
fn read_head(path: &std::path::Path) -> Result<(u64, Vec<u8>), SaveError> {
    use std::io::Read as _;
    let mut f = std::fs::File::open(path).map_err(|e| io(path, e))?;
    let size = f.metadata().map_err(|e| io(path, e))?.len();
    let mut head = [0u8; 16];
    f.read_exact(&mut head).map_err(|_| SaveError::new("truncated", "too short to be a save"))?;
    let len = u32::from_le_bytes([head[12], head[13], head[14], head[15]]) as usize;
    let mut buf = head.to_vec();
    if len <= savegame::MAX_HEADER_BYTES {
        let mut rest = vec![0u8; len];
        f.read_exact(&mut rest).map_err(|_| SaveError::new("truncated", "the header is cut short"))?;
        buf.extend(rest);
        // A dummy section-less tail so `peek` sees a complete frame.
        buf.extend([0u8; 4]);
    }
    Ok((size, buf))
}

/// Whose a save is.
enum Owner {
    /// The caller's (its header's owner is the caller's identity).
    Mine,
    /// Another writer's (the name it was saved under, for messages).
    Theirs(String),
    /// The header doesn't read (or predates owners): nobody can be sure, so it's treated as theirs.
    Unknown,
}

/// `force` on save `name` by identity `key` with `authority`: allowed on the caller's own (or a
/// missing) save; on another agent's it needs lead authority (owner, 2026-10-29).
fn force_on_save(path: &std::path::Path, key: &str, force: bool, authority: crate::authority::Authority, what: &str) -> Result<bool, SaveError> {
    if !force || !path.exists() {
        return Ok(force);
    }
    let own = matches!(owner_of(path, key), Owner::Mine);
    crate::authority::may_force(authority, own, what).map(|()| true).map_err(|e| {
        let mut s = SaveError::new(e.code, e.message);
        s.details = e.details;
        s
    })
}

fn owner_of(path: &std::path::Path, key: &str) -> Owner {
    let Ok((_, h)) = read_head(path).and_then(|(_, h)| savegame::peek(&h)) else { return Owner::Unknown };
    match h.get("owner").and_then(Json::as_str) {
        Some(o) if o == owner_hash(key) => Owner::Mine,
        Some(_) => Owner::Theirs(clip(h.get("created_by"), 128).as_str().unwrap_or("another agent").to_owned()),
        None => Owner::Unknown,
    }
}

fn whose(o: &Owner) -> String {
    match o {
        Owner::Theirs(n) => n.clone(),
        _ => "an unknown writer (the header doesn't say)".into(),
    }
}

/// Where a background save's or load's answer goes.
pub(crate) enum Dest {
    /// An agent's tool call.
    Tool(crate::jobs::Reply),
    /// The hosted game's request (its menu): answered with `Game::request_done`.
    Game(GameRequest),
}

/// A save being written on a worker thread.
pub(crate) struct Writing {
    name: String,
    handle: std::thread::JoinHandle<Result<Json, SaveError>>,
    dest: Dest,
}

/// A save being read and checked on a worker thread.
pub(crate) struct Loading {
    name: String,
    force: bool,
    /// Who asked: name and identity key (`None`: the game, for the player).
    by: String,
    key: Option<String>,
    /// The world's revision when the load started (changes since then are unsaved ones).
    revision: u64,
    /// The world generation physics was restored for.
    generation: u64,
    started: Instant,
    handle: Option<std::thread::JoinHandle<Result<Prepared, SaveError>>>,
    dest: Dest,
}

/// A save read, decoded and checked off the engine thread, ready to swap in.
pub(crate) struct Prepared {
    meta: Json,
    tick: u64,
    world: crate::world::World,
    physics: crate::physics::Physics,
    physics_bytes: Option<Vec<u8>>,
    /// The game's section when the game has no worker-side parser (parsed at the swap).
    game_bytes: Option<Vec<u8>>,
    /// The game's section, parsed on the worker ([`crate::game::Game::state_parser`]).
    game_parsed: Option<Box<dyn std::any::Any + Send>>,
    logic: Option<crate::logic::LogicSnapshot>,
    /// The sounds the rules had asked for when the save was written (none: `None`).
    sounds: Option<crate::module_sounds::SoundsSaved>,
    /// Navigation's state, decoded and shape-checked on the worker (restored once the loaded
    /// navmeshes are built: [`crate::nav::Nav::restore_state`]).
    nav: Option<Json>,
    /// Live Lex runs, parsed and rebuilt on the worker.
    lex_live: Option<crate::lex_live::LiveSnapshot>,
    sections: Vec<String>,
    notes: Vec<Json>,
    /// The hashes may not match although the state is right: another dmath or Lex runtime.
    hashes_reliable: bool,
    read_ms: f64,
}

/// Reads a save file (bounded) and decodes it.
fn read_file(path: &std::path::Path) -> Result<savegame::SaveFile, SaveError> {
    let size = std::fs::metadata(path).map_err(|e| io(path, e))?.len();
    if size > savegame::MAX_FILE_BYTES {
        return Err(SaveError::new("too_large", format!("{} is {size} bytes; saves are read up to {}", path.display(), savegame::MAX_FILE_BYTES)));
    }
    let bytes = std::fs::read(path).map_err(|e| io(path, e))?;
    savegame::decode(&bytes).map_err(|e| e.with("file", path.display().to_string()))
}

/// All saves in a saves folder together at most this big: a swarm saving in a loop fills a
/// folder, not the disk (`saves_full` says how to make room).
pub const MAX_SAVES_DIR_BYTES: u64 = 4 << 30;

/// Bytes of saves being written now (not on disk yet), per saves folder: writers reserve their
/// size before writing, so two at once can't pass the cap together.
static IN_FLIGHT: std::sync::Mutex<Vec<(PathBuf, u64)>> = std::sync::Mutex::new(Vec::new());

/// Room reserved in a saves folder for a save being written; given back when dropped.
struct Reservation {
    dir: PathBuf,
    bytes: u64,
}

impl Reservation {
    /// Reserves `new_len` bytes for a save at `path` if the folder's saves, the saves being
    /// written into it now and this one stay within `cap` (`saves_full` otherwise).
    fn take(dir: &std::path::Path, path: &std::path::Path, new_len: u64, cap: u64) -> Result<Reservation, SaveError> {
        let mut flight = IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
        let writing: u64 = flight.iter().filter(|(d, _)| d == dir).map(|(_, n)| *n).sum();
        dir_room(dir, path, new_len.saturating_add(writing), cap).map_err(|e| e.with("being_written_bytes", writing))?;
        flight.push((dir.to_path_buf(), new_len));
        Ok(Reservation { dir: dir.to_path_buf(), bytes: new_len })
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut flight = IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = flight.iter().position(|(d, n)| *d == self.dir && *n == self.bytes) {
            flight.swap_remove(i);
        }
    }
}

/// Whether `dir` has room for a `new_len`-byte save at `path` (replacing what is there): the
/// other saves' sizes plus this one stay within `cap`.
fn dir_room(dir: &std::path::Path, path: &std::path::Path, new_len: u64, cap: u64) -> Result<(), SaveError> {
    let mut used = 0u64;
    let mut count = 0usize;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p == path || p.extension().and_then(|x| x.to_str()) != Some(EXTENSION) {
                continue;
            }
            used = used.saturating_add(e.metadata().map_or(0, |m| m.len()));
            count += 1;
        }
    }
    if used.saturating_add(new_len) > cap {
        return Err(SaveError::new(
            "saves_full",
            format!("the saves folder holds {used} bytes in {count} saves; with this {new_len}-byte save it would pass its cap of {cap} bytes: save_delete old saves (save_list shows them) or save over one"),
        )
        .with("used_bytes", used)
        .with("saves", count)
        .with("cap_bytes", cap)
        .with("this_save_bytes", new_len));
    }
    Ok(())
}

/// Inspections running at once (each streams its file with a small buffer).
static INSPECTING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
const MAX_INSPECTS: usize = 4;

/// `save_read {inspect}`: the header and section table of a save, its framing, table and file
/// checksum checked by streaming the file; nothing is decompressed (`savegame::inspect`), so an
/// inspection costs the header plus 64 KB whatever the file.
fn inspect(path: &std::path::Path, name: &str) -> Result<Json, SaveError> {
    let len = std::fs::metadata(path).map_err(|e| io(path, e))?.len();
    if len > savegame::MAX_FILE_BYTES {
        return Err(SaveError::new("too_large", format!("{} is {len} bytes; saves are read up to {}", path.display(), savegame::MAX_FILE_BYTES)));
    }
    let mut file = std::fs::File::open(path).map_err(|e| io(path, e))?;
    let f = savegame::inspect(&mut file, len).map_err(|e| e.with("file", path.display().to_string()))?;
    let mut j = Json::obj(header_summary(&f.meta));
    j.set("name", Json::from(name));
    j.set("version", Json::from(f.version));
    j.set("bytes", Json::from(len));
    j.set("sections", Json::obj(f.table.iter().map(|t| (t.name.as_str(), Json::obj([("bytes", Json::from(t.bytes)), ("stored", Json::from(t.stored)), ("codec", Json::from(t.codec.as_str()))])))));
    j.set("reads", Json::from(true));
    j.set("checked", Json::from("framing, header, section table and the file's checksum; sections aren't decompressed (save_read loads and checks them)"));
    Ok(j)
}

/// Everything about a load that can run off the engine thread: read, decode, check the header
/// against the engine, load the world, parse module state, restore physics against the world.
#[allow(clippy::too_many_arguments)]
fn prepare_load(
    path: &std::path::Path,
    limits: crate::two_d::ShapeLimits,
    hosted: Option<String>,
    hz: u32,
    generation: u64,
    parser: Option<crate::game::StateParser>,
    natives: specula_lex::Natives,
    models: Option<std::sync::Arc<dyn crate::world::ModelSource>>,
) -> Result<Prepared, SaveError> {
    let t0 = Instant::now();
    let f = read_file(path)?;
    let meta = f.meta;
    let game = meta.get("game").and_then(Json::as_str).map(str::to_owned);
    if game != hosted {
        return Err(SaveError::new(
            "wrong_game",
            format!("the save is from {} but this engine hosts {}", game.as_deref().unwrap_or("no game"), hosted.as_deref().unwrap_or("no game")),
        ));
    }
    let save_hz = meta.get("tick_hz").and_then(Json::as_u64).unwrap_or(0);
    if save_hz != u64::from(hz) {
        return Err(SaveError::new("wrong_tick_rate", format!("the save ticks at {save_hz} Hz, this engine at {hz} Hz")));
    }
    let Some(tick) = meta.get("tick").and_then(Json::as_u64).filter(|t| *t < 1 << 53) else {
        return Err(SaveError::new("invalid_header", "the header's 'tick' must be a whole number"));
    };
    // Versions that change results: a mismatch means the hashes can't be expected to match.
    let mut notes = Vec::new();
    let mut hashes_reliable = true;
    for (key, now) in [("dmath", u64::from(specula_core::dmath::VERSION)), ("lex_runtime", u64::from(specula_lex::RUNTIME_VERSION))] {
        let saved = meta.get(key).and_then(Json::as_u64);
        if saved != Some(now) {
            hashes_reliable = false;
            notes.push(Json::obj([
                ("warning", Json::from("version_mismatch")),
                ("what", Json::from(key)),
                ("saved", Json::from(saved)),
                ("now", Json::from(now)),
                ("note", Json::from("saved with another version of code that changes results: the run continues, but it isn't expected to match the saving run's hashes")),
            ]));
        }
    }
    let sections: Vec<String> = f.sections.iter().map(|s| s.name.clone()).collect();
    let mut by_name: std::collections::BTreeMap<String, Vec<u8>> = f.sections.into_iter().map(|s| (s.name, s.data)).collect();
    // The world: unpacked (its section's bytes dropped at once), each file handed over, not copied.
    let world_bytes = by_name.remove("world").ok_or_else(|| SaveError::new("missing_section", "the save has no world section").in_section("world"))?;
    let mut files = savegame::unpack_files(&world_bytes)?;
    drop(world_bytes);
    let (world, _) = crate::persist::load_with_limits(limits, |file| match files.iter_mut().find(|(n, _)| n == file) {
        Some((_, text)) => Ok(std::mem::take(text)),
        None => Err(crate::persist::PersistError::new("not_found", format!("the save's world has no file {file}"))),
    })
    .map_err(|e| SaveError::new("corrupt_section", format!("the save's world doesn't load: {e}")).in_section("world").with("cause", e.to_json()))?;
    drop(files);
    // Live Lex runs, rebuilt against the loaded world (their presentation adopted from it).
    let lex_live = match by_name.remove("lex_live") {
        Some(b) => Some(crate::lex_live::parse_state(&b, &world, tick, &natives)?),
        None => None,
    };
    let logic = match by_name.get("logic") {
        Some(b) => Some(crate::logic::LogicHost::parse_state(b).map_err(|e| SaveError::new(e.code, e.message.clone()).in_section("logic"))?),
        None => None,
    };
    let sounds = match by_name.remove("sounds") {
        Some(b) => Some(crate::module_sounds::ModuleSounds::parse_section(&b).map_err(|m| SaveError::new("corrupt_section", m).in_section("sounds"))?),
        None => None,
    };
    let nav = match by_name.remove("nav") {
        Some(b) => Some(crate::nav::parse_section(&b).map_err(|(code, m)| SaveError::new(code, m).in_section("nav"))?),
        None => None,
    };
    let physics_bytes = by_name.remove("physics");
    let mut physics = crate::physics::Physics::default();
    let mut world = world;
    if physics_bytes.is_some() {
        let _ = world.set_model_source(models);
    }
    if let Some(b) = &physics_bytes {
        // Saved ragdolls are planned again from their models' rigs: the world needs the engine's
        // models to read them (replace_world attaches them again and reports what didn't build).
        physics.restore_state(&world, generation, tick, b).map_err(|e| {
            let mut s = SaveError::new(e.code, e.message.clone()).in_section("physics");
            // The section is the SaveError's own field: not again among the details.
            s.details = e.details.iter().filter(|(k, _)| *k != "section").cloned().collect();
            s
        })?;
    }
    let mut game_bytes = by_name.remove("game");
    if game_bytes.is_some() && hosted.is_none() {
        return Err(SaveError::new("wrong_game", "the save has game state but no game is hosted"));
    }
    // The game's state, parsed here too when the game can (its parser checks it against the game).
    let game_parsed = match (&game_bytes, parser) {
        (Some(b), Some(parse)) => Some(parse(b).map_err(|m| SaveError::new("game_state", format!("the game can't load this save: {m}")).in_section("game"))?),
        _ => None,
    };
    if game_parsed.is_some() {
        game_bytes = None;
    }
    Ok(Prepared { meta, tick, world, physics, physics_bytes, game_bytes, game_parsed, logic, sounds, nav, lex_live, sections, notes, hashes_reliable, read_ms: t0.elapsed().as_secs_f64() * 1000.0 })
}

/// A save taken between ticks, ready to render and write on any thread.
pub(crate) struct SaveJob {
    name: String,
    dir: PathBuf,
    path: PathBuf,
    tick: u64,
    world: crate::persist::Plan,
    meta: Json,
    /// Everything but the world.
    sections: Vec<Section>,
    world_hash: u64,
    subsystems: Json,
    notes: Vec<Json>,
    t0: Instant,
    /// Time spent on the engine thread.
    engine_ms: f64,
}

impl SaveJob {
    /// Renders the world, packs and compresses the sections, writes the file (temporary file,
    /// sync, rename) and reports.
    pub(crate) fn finish(self) -> Result<Json, SaveError> {
        let t1 = Instant::now();
        let files = crate::persist::render_plan(&self.world);
        let mut sections = vec![Section { name: "world".into(), data: savegame::pack_files(&files) }];
        drop(files);
        sections.extend(self.sections);
        let sizes = Json::obj(sections.iter().map(|s| (s.name.as_str(), Json::from(s.data.len()))));
        // What a load would refuse isn't written.
        let total: u64 = sections.iter().map(|s| s.data.len() as u64).sum();
        if total > savegame::MAX_DECODED_BYTES || sections.iter().any(|s| s.data.len() > savegame::MAX_SECTION_BYTES) {
            return Err(SaveError::new("too_large", format!("this save would hold {total} bytes; saves load up to {} (and {} per section)", savegame::MAX_DECODED_BYTES, savegame::MAX_SECTION_BYTES))
                .with("sections", sizes));
        }
        let bytes = savegame::encode(&self.meta, &sections);
        let encode_ms = t1.elapsed().as_secs_f64() * 1000.0;
        let t2 = Instant::now();
        // Held until the file is renamed into place (then it's on disk and counted there).
        let _room = Reservation::take(&self.dir, &self.path, bytes.len() as u64, MAX_SAVES_DIR_BYTES)?;
        // Through the disk budget: refused below the free-space floor (saves are never pruned).
        let old = crate::disk::file_len(&self.path);
        crate::disk::book().admit(crate::disk::Category::Saves, bytes.len() as u64, crate::disk::Need::Agent).map_err(|e| SaveError { code: e.code, message: e.message, section: None, offset: None, details: e.detail })?;
        std::fs::create_dir_all(&self.dir).map_err(|e| io(&self.dir, e))?;
        let tmp = self.path.with_extension(format!("{EXTENSION}.tmp"));
        {
            use std::io::Write as _;
            let mut f = std::fs::File::create(&tmp).map_err(|e| io(&tmp, e))?;
            f.write_all(&bytes).and_then(|()| f.sync_all()).map_err(|e| {
                let _ = std::fs::remove_file(&tmp);
                io(&tmp, e)
            })?;
        }
        std::fs::rename(&tmp, &self.path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            io(&self.path, e)
        })?;
        crate::disk::book().replaced(crate::disk::Category::Saves, "save", old, bytes.len() as u64);
        let write_ms = t2.elapsed().as_secs_f64() * 1000.0;
        let ms = |v: f64| Json::from((v * 100.0).round() / 100.0);
        Ok(Json::obj([
            ("saved", Json::from(self.name.as_str())),
            ("file", Json::from(self.path.display().to_string())),
            ("tick", Json::from(self.tick)),
            ("world_hash", Json::from(hex(self.world_hash))),
            ("subsystems", self.subsystems),
            ("bytes", Json::from(bytes.len())),
            ("sections", sizes),
            (
                "timings_ms",
                Json::obj([
                    ("engine_thread", ms(self.engine_ms)),
                    ("render_and_encode", ms(encode_ms)),
                    ("write", ms(write_ms)),
                    ("total", ms(self.t0.elapsed().as_secs_f64() * 1000.0)),
                ]),
            ),
            ("notes", Json::Arr(self.notes)),
        ]))
    }
}

/// How often the game's save is tried again after a momentary refusal (`nav_building`), and
/// how many wait at once.
const MAX_GAME_SAVE_RETRIES: u32 = 20;
const MAX_WAITING_SAVES: usize = 8;

/// A game save waiting to be tried again.
pub(crate) struct RetrySave {
    at: Instant,
    info: RetryInfo,
    req: GameRequest,
}

/// The tick a game save was asked at and how often it was tried again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RetryInfo {
    requested_tick: u64,
    tries: u32,
}

/// Whether a refused game request is tried again, and after how long: only saves refused for a
/// moment (`nav_building`), at most [`MAX_GAME_SAVE_RETRIES`] times, [`MAX_WAITING_SAVES`] at
/// once, after the refusal's `retry_after_ms` (50 ms to 5 s).
fn retry_delay(e: &SaveError, is_save: bool, tries: u32, waiting: usize) -> Option<std::time::Duration> {
    if e.code != "nav_building" || !is_save || tries >= MAX_GAME_SAVE_RETRIES || waiting >= MAX_WAITING_SAVES {
        return None;
    }
    let ms = e.details.iter().find(|(k, _)| *k == "retry_after_ms").and_then(|(_, v)| v.as_u64()).unwrap_or(500).clamp(50, 5_000);
    Some(std::time::Duration::from_millis(ms))
}

impl Engine {
    fn save_path(&self, name: &str) -> PathBuf {
        self.saves_dir.join(format!("{name}.{EXTENSION}"))
    }

    /// The subsystem hashes as the desync detector records them, right now.
    fn subsystem_hashes(&mut self) -> Vec<(&'static str, u64)> {
        let mut subs = Vec::new();
        if let Some(g) = &self.game {
            g.state_hashes(&mut subs);
        }
        if let Some(h) = self.physics.hash() {
            subs.push(("physics", h));
        }
        if let Some(h) = self.logic.state_hash() {
            subs.push(("logic", h));
        }
        if !self.nav.is_empty() {
            // What a save records (a synced copy's hash); after a load, the restored state's.
            subs.push(("nav", self.nav.save_hash(&self.world, self.world_generation)));
        }
        if let Some(h) = self.lex_live.state_hash() {
            subs.push(("lex_live", h));
        }
        subs
    }

    /// Whether a save of `name` is being written or read now.
    fn save_busy(&self, name: &str) -> Option<SaveError> {
        let writing = self.save_writes.iter().any(|w| w.name == name);
        let loading = self.save_load.as_ref().is_some_and(|l| l.name == name);
        (writing || loading).then(|| SaveError::new("busy", format!("the save '{name}' is being {} now; call again in a moment", if writing { "written" } else { "read" })).with("retry_after_ms", 100))
    }

    /// The save tools answered on the engine thread at once (`save_list`, `save_delete`).
    pub(crate) fn handle_save(&mut self, call: SaveCall, w: Writer, by: &str, authority: crate::authority::Authority) -> ToolOutput {
        match call {
            SaveCall::List { cursor, limit } => out(self.list_saves(cursor, limit)),
            SaveCall::Delete { name, force } => match force_on_save(&self.save_path(&name), w.session.unwrap_or(by), force, authority, "save_delete").and_then(|force| self.delete_save(&name, force, w.session.unwrap_or(by))) {
                Ok(j) => out(j),
                Err(e) => fail(&e),
            },
            // Writes and reads run in the background (handle_save_request).
            SaveCall::Write { .. } | SaveCall::Read { .. } => fail(&SaveError::new("internal", "save_write and save_read are answered in the background")),
        }
    }

    /// `save_write` and `save_read` from agents: started here, finished on worker threads, answered
    /// when done (see [`Engine::poll_saves`]); the window keeps drawing meanwhile.
    pub(crate) fn handle_save_request(&mut self, request: crate::tools::Request) {
        let crate::tools::Command::SaveGame(call) = request.command else { return };
        let by = self.swarm.agent_name(&request.session);
        let key = self.swarm.key(&request.session);
        let authority = crate::authority::authority(&self.swarm, &request.session.id);
        let reply = request.reply;
        let started = match call {
            SaveCall::Write { name, label, overwrite, force } => match force_on_save(&self.save_path(&name), &key, force && overwrite, authority, "save_write") {
                Ok(force) => self.start_save(&name, label, overwrite, force, &by, &key, Dest::Tool(reply.clone())),
                Err(e) => Err(e),
            },
            SaveCall::Read { name, inspect: true, .. } => {
                // Reading and checking a file needs nothing of the engine's: on a worker, at most
                // MAX_INSPECTS at once (each streams with a small buffer).
                use std::sync::atomic::Ordering;
                let path = self.save_path(&name);
                let tx = reply.clone();
                if let Err(m) = validate_save_name(&name) {
                    Err(SaveError::new("invalid_argument", m))
                } else if INSPECTING.fetch_add(1, Ordering::SeqCst) >= MAX_INSPECTS {
                    INSPECTING.fetch_sub(1, Ordering::SeqCst);
                    Err(SaveError::new("busy", format!("{MAX_INSPECTS} saves are being inspected; call again in a moment")).with("retry_after_ms", 200))
                } else {
                    std::thread::Builder::new()
                        .name("specula-save-inspect".into())
                        .spawn(move || {
                            let r = inspect(&path, &name);
                            INSPECTING.fetch_sub(1, Ordering::SeqCst);
                            let _ = tx.send(match r {
                                Ok(j) => out(j),
                                Err(e) => fail(&e),
                            });
                        })
                        .map(drop)
                        .map_err(|e| {
                            INSPECTING.fetch_sub(1, Ordering::SeqCst);
                            SaveError::new("io", format!("could not start a reader: {e}"))
                        })
                }
            }
            SaveCall::Read { name, force, .. } => self.start_load(&name, force, &by, Some(key), Dest::Tool(reply.clone())),
            other => {
                let _ = reply.send(self.handle_save(other, Writer::new(&by, Some(&key)), &by, authority));
                return;
            }
        };
        if let Err(e) = started {
            let _ = reply.send(fail(&e));
        }
    }

    /// Takes the state (engine thread) and starts a writer.
    #[allow(clippy::too_many_arguments)]
    fn start_save(&mut self, name: &str, label: Option<String>, overwrite: bool, force: bool, by: &str, key: &str, dest: Dest) -> Result<(), SaveError> {
        validate_save_name(name).map_err(|m| SaveError::new("invalid_argument", m))?;
        if let Some(e) = self.save_busy(name) {
            return Err(e);
        }
        // Each writer holds a copy of the world: at most MAX_WRITERS at once.
        if self.save_writes.len() >= MAX_WRITERS {
            return Err(SaveError::new("busy", format!("{MAX_WRITERS} saves are being written; call again in a moment"))
                .with("writing", Json::arr(self.save_writes.iter().map(|w| Json::from(w.name.as_str()))))
                .with("retry_after_ms", 200));
        }
        let job = self.begin_save(name, label, overwrite, force, by, key)?;
        // The writer is part of the call that asked for the save (docs/design/TRACE.md).
        let ctx = specula_core::trace::Ctx::current();
        let handle = std::thread::Builder::new()
            .name("specula-save".into())
            .spawn(move || {
                let _c = ctx.enter();
                let _s = crate::trace_engine::job_span("save.write");
                job.finish()
            })
            .map_err(|e| SaveError::new("io", format!("could not start a save writer: {e}")))?;
        self.save_writes.push(Writing { name: name.to_owned(), handle, dest });
        Ok(())
    }

    /// The engine-thread part of a save: the state's bytes and owned copies of the world (the
    /// costly rendering, compression and writing happen in [`SaveJob::finish`], anywhere).
    pub(crate) fn begin_save(&mut self, name: &str, label: Option<String>, overwrite: bool, force: bool, by: &str, key: &str) -> Result<SaveJob, SaveError> {
        let path = self.save_path(name);
        if path.exists() {
            let owner = owner_of(&path, key);
            if !overwrite {
                return Err(SaveError::new("save_exists", format!("a save named '{name}' exists; pass overwrite: true to replace it, or pick another name"))
                    .with("name", name)
                    .with("written_by", whose(&owner)));
            }
            if !matches!(owner, Owner::Mine) && !force {
                return Err(SaveError::new("not_yours", format!("the save '{name}' was written by {}; pass force: true to replace it anyway, or pick another name", whose(&owner))));
            }
        }
        // Live Lex runs: their own section (lex_live/save.rs). Agents' holds end with their
        // sessions, which a save doesn't keep: left out, and said so.
        let lex_live = self.lex_live.save_state()?;
        let holds = self.lex_live.holds_count();
        let t0 = Instant::now();
        let tick = self.clock.tick();
        // The game first: it may refuse.
        let game = match self.game.as_ref().and_then(|g| g.save_state()) {
            None => None,
            Some(Ok(b)) => Some(b),
            Some(Err(m)) => return Err(SaveError::new("game_refused", format!("the game can't be saved now: {m}"))),
        };
        let physics = self.physics.save_state(&self.world, self.world_generation);
        // Physics' hash after the sync: what a load will compute.
        if physics.is_some() {
            let h = self.physics.space.hash();
            if self.physics.hashes.back().is_some_and(|(_, x)| *x != h) {
                if self.physics.hashes.len() == crate::physics::HASHES {
                    self.physics.hashes.pop_front();
                }
                self.physics.hashes.push_back((tick, h));
            }
        }
        let logic = self.logic.save_state();
        // Navigation's state as it is (its build state as data: nothing is built for a save, so
        // saving never changes the run). Only a switch to new settings or a load still being
        // prepared is refused (`nav_building`, retry).
        let nav = if self.nav.is_empty() {
            None
        } else {
            let as_save = |e: crate::world::WorldError| {
                let mut s = SaveError::new(e.code, e.message.clone()).in_section("nav");
                s.details = e.details.clone();
                s
            };
            // On a copy synced to the world (what the next tick's sync would mark): the live
            // navigation is never touched, so saving changes nothing about the run.
            Some(self.nav.save_section(&self.world, self.world_generation).map_err(as_save)?)
        };
        let world = crate::persist::plan(&self.world, std::path::Path::new(""), None);
        let world_hash = self.state_hash();
        let subs = self.subsystem_hashes();
        let created = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let mut meta = vec![
            ("format", Json::from(savegame::FORMAT)),
            ("engine", Json::from(env!("CARGO_PKG_VERSION"))),
            ("game", self.game.as_ref().map_or(Json::Null, |_| Json::from(self.game_name.as_str()))),
            ("tick", Json::from(tick)),
            ("tick_hz", Json::from(self.clock.hz())),
            ("world_hash", Json::from(hex(world_hash))),
            ("next_id", Json::from(self.world.next_id())),
            ("entities", Json::from(self.world.persistent_len())),
            ("subsystems", Json::obj(subs.iter().map(|(n, h)| (*n, Json::from(hex(*h)))))),
            ("dmath", Json::from(specula_core::dmath::VERSION)),
            ("lex_runtime", Json::from(specula_lex::RUNTIME_VERSION)),
            ("created_by", Json::from(by)),
            ("owner", Json::from(owner_hash(key))),
            ("created_unix_s", Json::from(created)),
        ];
        if let Some(l) = &label {
            meta.push(("label", Json::from(l.as_str())));
        }
        let mut sections = Vec::new();
        let sounds = self.module_sounds.save_section();
        for (n, d) in [("game", game), ("physics", physics), ("logic", logic), ("nav", nav), ("lex_live", lex_live), ("sounds", sounds)] {
            if let Some(data) = d {
                sections.push(Section { name: n.into(), data });
            }
        }
        let mut notes = Vec::new();
        if holds > 0 {
            notes.push(Json::obj([
                ("note", Json::from("agents' holds on live Lex runs' input aren't saved (sessions aren't): a load starts with none")),
                ("holds_left_out", Json::from(holds)),
            ]));
        }
        if self.game.is_some() && !sections.iter().any(|s| s.name == "game") {
            notes.push(Json::from("the hosted game keeps no save state: a load restores the world, physics and modules, and the game starts from what the world shows"));
        }
        // What this save holds: work up to here is kept somewhere (a forced load discards only
        // what came after; see `Engine::others_unsaved`).
        self.checkpoint_seq = self.swarm.journal().last_seq();
        Ok(SaveJob {
            name: name.to_owned(),
            dir: self.saves_dir.clone(),
            path,
            tick,
            world,
            meta: Json::obj(meta),
            sections,
            world_hash,
            subsystems: Json::obj(subs.iter().map(|(n, h)| (*n, Json::from(hex(*h))))),
            notes,
            t0,
            engine_ms: t0.elapsed().as_secs_f64() * 1000.0,
        })
    }

    /// The refusal for a `world_load` while a save game is being read (`None`: none is):
    /// `job_busy` with the reader as `owner` (AGENTS rule 14) and `retry`. One replacement of
    /// the world at a time (`Engine::load_busy` is the same rule from the other side).
    pub(crate) fn save_read_busy(&self) -> Option<crate::world::WorldError> {
        let l = self.save_load.as_ref()?;
        Some(
            crate::authority::refusal(crate::authority::Site::JobBusy, format!("{}'s save_read of `{}` is in progress (one replacement of the world at a time): call again when it has answered", l.by, l.name))
                .with("job", "save_read")
                .with("owner", l.by.as_str())
                .with("retry_after_s", 1)
                .with("retry", true),
        )
    }

    /// A save game's read in flight, for tests of what must wait for one (no reader thread).
    #[cfg(test)]
    pub(crate) fn save_read_in_flight_for_test(&mut self, name: &str, by: &str, reply: crate::jobs::Reply) {
        self.save_load = Some(Loading {
            name: name.to_owned(),
            force: false,
            by: by.to_owned(),
            key: Some(by.to_owned()),
            revision: self.world.revision(),
            generation: self.world_generation + 1,
            started: Instant::now(),
            handle: None,
            dest: Dest::Tool(reply),
        });
    }

    /// The checks a load can do now (claims, unsaved changes), then a reader on a worker thread.
    fn start_load(&mut self, name: &str, force: bool, by: &str, key: Option<String>, dest: Dest) -> Result<(), SaveError> {
        validate_save_name(name).map_err(|m| SaveError::new("invalid_argument", m))?;
        if let Some(e) = self.save_busy(name) {
            return Err(e);
        }
        if self.save_load.is_some() {
            return Err(SaveError::new("busy", "another save is being loaded; call again in a moment").with("retry_after_ms", 200));
        }
        self.check_load_allowed(force, by, key.as_deref(), None)?;
        let path = self.save_path(name);
        let (limits, hz) = (self.world.shape_limits(), self.clock.hz());
        let hosted = self.game.as_ref().map(|_| self.game_name.clone());
        let generation = self.world_generation + 1;
        let parser = self.game.as_ref().and_then(|g| g.state_parser());
        let natives = self.lex_natives.clone();
        let models = Some(std::sync::Arc::clone(&self.library) as std::sync::Arc<dyn crate::world::ModelSource>);
        let handle = std::thread::Builder::new()
            .name("specula-load".into())
            .spawn(move || prepare_load(&path, limits, hosted, hz, generation, parser, natives, models))
            .map_err(|e| SaveError::new("io", format!("could not start a save reader: {e}")))?;
        self.save_load = Some(Loading {
            name: name.to_owned(),
            force,
            by: by.to_owned(),
            key,
            revision: self.world.revision(),
            generation,
            started: Instant::now(),
            handle: Some(handle),
            dest,
        });
        Ok(())
    }

    /// Claims and unsaved changes: checked when a load starts and again when it swaps in
    /// (`since`: the world's revision when it started; changes after it are unsaved too).
    pub(crate) fn check_load_allowed(&mut self, force: bool, by: &str, key: Option<&str>, since: Option<u64>) -> Result<crate::write_policy::MayReplace, SaveError> {
        // One replacement of the world at a time: a `world_load` waiting or reading is replacing
        // it (and its worker may be writing the board, which this path writes on the engine
        // thread below). Refused like a second load, naming the loader (`Engine::load_busy`).
        if let Some(e) = self.load_busy("one replacement of the world at a time") {
            let mut s = SaveError::new(e.code, e.message.clone());
            s.details = e.details.clone();
            return Err(s);
        }
        // A playtest owns the clock and the world until it ends: a load would move both under it
        // (and its replay would record a run nobody can reproduce). Not even with force. Nor
        // while other agents hold claims.
        let may = crate::write_policy::may_replace(&self.swarm, Writer::new(by, key), &self.playtests).map_err(|e| {
            let mut s = SaveError::new(e.code, e.message.clone());
            s.details = e.details.clone();
            s
        })?;
        if force {
            // Coordinator, 2026-10-29: force discarding another agent's unsaved work is the
            // lead's call (the simulation's own changes and the caller's are anyone's; the
            // game's own menu, `key` None, is the player's).
            if let Some(k) = key.filter(|_| self.dirty()) {
                if let Some(other) = self.others_unsaved(k) {
                    let have = self.swarm.authority_of_key(k);
                    crate::authority::may_force(have, false, "save_read {force} (it discards another agent's unsaved changes)").map_err(|e| {
                        let mut s = SaveError::new(e.code, e.message);
                        s.details = e.details;
                        s.details.push(("unsaved_by", Json::from(other.as_str())));
                        s
                    })?;
                }
            }
            return Ok(may);
        }
        if since.is_some_and(|r| r != self.world.revision()) {
            return Err(SaveError::new("unsaved_changes", "the world changed while the save was loading; save_write first, or load with force: true to discard the changes"));
        }
        self.finish_autosave(true);
        self.save_board_now();
        if self.dirty() {
            return Err(SaveError::new("unsaved_changes", "the live world has changes not in a world folder or save; world_save or save_write first, or pass force: true to discard them")
                .with("current_world", self.world_name.clone().map_or(Json::Null, Json::from)));
        }
        Ok(may)
    }

    /// Swaps a prepared save in (engine thread): checks again, then the clock, world, physics,
    /// modules and game, and reports the hashes now against the saved ones.
    fn finish_load(&mut self, l: &Loading, p: Prepared) -> Result<Json, SaveError> {
        // Saves asked for before this load (one may be mid-plan) are of the world it replaces:
        // written first, as world_load does (review of 07f3276: the plan went on with the loaded
        // world and saved it under the old name).
        self.flush_world_saves();
        let t1 = Instant::now();
        let may = self.check_load_allowed(l.force, &l.by, l.key.as_deref(), Some(l.revision))?;
        // Parsed on the worker when the game has a parser; otherwise (a game without one) here.
        let parsed: Option<Box<dyn std::any::Any>> = match (p.game_parsed, &p.game_bytes, self.game.as_ref()) {
            (Some(state), _, _) => Some(state),
            (None, Some(bytes), Some(g)) => Some(g.parse_state(bytes).map_err(|m| SaveError::new("game_state", format!("the game can't load this save: {m}")).in_section("game"))?),
            _ => None,
        };
        let mut physics = p.physics;
        if self.world_generation + 1 != l.generation {
            // Another world came in meanwhile: restore for the generation this load will make.
            physics = crate::physics::Physics::default();
            if let Some(b) = &p.physics_bytes {
                physics.restore_state(&p.world, self.world_generation + 1, p.tick, b).map_err(|e| SaveError::new(e.code, e.message.clone()).in_section("physics"))?;
            }
        }
        // Everything checks: make it live.
        self.clock.set_tick(p.tick);
        let mut notes: Vec<Json> = p.notes;
        // Live Lex runs presented into the world being replaced: they stop (the save brings its own).
        let lex_before = self.lex_live.stop_all();
        if !lex_before.is_empty() {
            notes.push(Json::obj([
                ("note", Json::from("the live Lex runs running before the load stopped (the world they presented into was replaced)")),
                ("stopped", Json::arr(lex_before.iter().map(|s| Json::from(s.as_str())))),
            ]));
        }
        notes.extend(self.replace_world(may, p.world, &format!("save:{}", l.name)).into_iter().map(Json::from));
        // The machine's thread count stays (it isn't in saves: results never depend on it).
        physics.threads = self.physics.threads;
        self.physics = physics;
        // Live Lex runs: the save's (those running before stop: their world is gone).
        notes.extend(self.lex_live.apply_state(p.lex_live));
        if let Some(snap) = p.logic {
            notes.extend(self.logic.apply_state(snap));
        } else if !self.logic.is_empty() {
            notes.push(Json::from("the save has no module state; running modules keep theirs"));
        }
        // What the rules had sounded is the save's, as the rest of the run is.
        self.module_sounds.restore(p.sounds);
        // Navigation comes back once the loaded navmeshes are built (the clock is held until
        // then); its hash is checked then (nav_info last_restore).
        let mut nav_pending = false;
        if let Some(state) = &p.nav {
            match self.nav.restore_state(&self.world, self.world_generation, state) {
                Ok(Some(_)) => {}
                Ok(None) => nav_pending = true,
                Err(e) => notes.push(Json::obj([("warning", Json::from("nav_not_restored")), ("error", Json::from(e.code)), ("message", Json::from(e.message.as_str()))])),
            }
        }
        if let Some(state) = parsed {
            self.game_call(crate::sim_writes::GameCall::ApplyState(state));
        }
        // A quit the game asked for before the load was asked in another run: the game the
        // save brought back has asked for none (`specula find` loads a save after a way that
        // quit, and the next way took that quit as its own).
        self.script_quit = false;
        // The loaded world isn't a world folder: world_save names one.
        self.world_name = None;
        self.saved = None;
        self.world_lock = None;
        self.saved_revision = self.world.revision();
        self.saved_seq = self.swarm.journal().last_seq();
        self.hash_cache = None;
        self.last_advance = Instant::now();
        let apply_ms = t1.elapsed().as_secs_f64() * 1000.0;
        // Hashes now against the saved ones.
        let meta = &p.meta;
        let world_now = self.state_hash();
        let world_saved = meta.get("world_hash").and_then(Json::as_str).unwrap_or("");
        let subs_now = self.subsystem_hashes();
        let saved_subs = meta.get("subsystems").and_then(Json::as_object).unwrap_or_default();
        let mut all_match = hex(world_now) == world_saved;
        let mut subsystems = Vec::new();
        let mut names: Vec<String> = saved_subs.iter().take(32).map(|(k, _)| k.chars().take(64).collect()).collect();
        for (n, _) in &subs_now {
            if !names.iter().any(|x| x == n) {
                names.push((*n).to_owned());
            }
        }
        for n in names {
            let saved = saved_subs.iter().find(|(k, _)| *k == n).and_then(|(_, v)| v.as_str()).map(|s| s.chars().take(16).collect::<String>());
            if n == "nav" && nav_pending {
                // Checked when the restore completes (before the first tick).
                subsystems.push(Json::obj([("name", Json::from(n)), ("saved", Json::from(saved)), ("now", Json::Null), ("pending", Json::from(true))]));
                continue;
            }
            let now = subs_now.iter().find(|(k, _)| *k == n).map(|(_, h)| hex(*h));
            let same = saved == now;
            all_match &= same;
            subsystems.push(Json::obj([("name", Json::from(n)), ("saved", Json::from(saved)), ("now", Json::from(now)), ("match", Json::from(same))]));
        }

        let ms = |v: f64| Json::from((v * 100.0).round() / 100.0);
        Ok(Json::obj([
            ("loaded", Json::from(l.name.as_str())),
            ("tick", Json::from(p.tick)),
            ("game", clip(meta.get("game"), 64)),
            ("label", clip(meta.get("label"), MAX_LABEL)),
            ("created_by", clip(meta.get("created_by"), 128)),
            ("restored", Json::obj(p.sections.iter().map(|s| (s.as_str(), Json::from(true))))),
            ("world_hash", Json::obj([("saved", clip(meta.get("world_hash"), 16)), ("now", Json::from(hex(world_now))), ("match", Json::from(hex(world_now) == world_saved))])),
            ("subsystems", Json::Arr(subsystems)),
            ("match", Json::from(all_match)),
            ("hash_match_reliable", Json::from(p.hashes_reliable)),
            ("entities", Json::from(self.world.persistent_len())),
            (
                "timings_ms",
                Json::obj([
                    ("read_and_check", ms(p.read_ms)),
                    ("apply", ms(apply_ms)),
                    ("total", ms(l.started.elapsed().as_secs_f64() * 1000.0)),
                ]),
            ),
            ("notes", Json::arr(notes.into_iter().take(20))),
            ("next", Json::from("world_save {name} keeps the loaded world as world files; desync_trace from this tick on compares with the saving run's")),
        ]))
    }

    /// Whether background saves or loads are running (the headless loop wakes for them).
    pub(crate) fn saves_in_flight(&self) -> bool {
        !self.save_writes.is_empty() || self.save_load.is_some()
    }

    /// Answers background saves and loads that finished (every loop iteration).
    pub(crate) fn poll_saves(&mut self) {
        if self.save_writes.iter().any(|w| w.handle.is_finished()) {
            let (done, running): (Vec<Writing>, Vec<Writing>) = std::mem::take(&mut self.save_writes).into_iter().partition(|w| w.handle.is_finished());
            self.save_writes = running;
            for w in done {
                let result = w.handle.join().unwrap_or_else(|_| Err(SaveError::new("internal", "the save writer stopped unexpectedly")));
                self.deliver(w.dest, result);
            }
            // What the game asked for as it heard back (its list refresh).
            self.serve_game_requests();
        }
        if self.save_load.as_ref().is_some_and(|l| l.handle.as_ref().is_none_or(|h| h.is_finished())) {
            let mut l = self.save_load.take().expect("checked");
            let Some(handle) = l.handle.take() else { return };
            let result = match handle.join() {
                Ok(Ok(p)) => self.finish_load(&l, p),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(SaveError::new("internal", "the save reader stopped unexpectedly")),
            };
            self.deliver(l.dest, result);
            self.serve_game_requests();
        }
    }

    /// Waits for every background save and load and answers them (shutdown).
    pub(crate) fn join_saves(&mut self) {
        for w in std::mem::take(&mut self.save_writes) {
            let result = w.handle.join().unwrap_or_else(|_| Err(SaveError::new("internal", "the save writer stopped unexpectedly")));
            self.deliver(w.dest, result);
        }
        if let Some(mut l) = self.save_load.take() {
            if let Some(h) = l.handle.take() {
                let _ = h.join();
            }
            self.deliver(l.dest, Err(SaveError::new("shutting_down", "the engine is shutting down; the save wasn't loaded")));
        }
    }

    fn deliver(&mut self, dest: Dest, result: Result<Json, SaveError>) {
        match dest {
            Dest::Tool(tx) => {
                let _ = tx.send(match result {
                    Ok(j) => out(j),
                    Err(e) => fail(&e),
                });
            }
            Dest::Game(req) => {
                // A save that was retried says which tick it was asked at and how often it was
                // tried again (it holds a later tick).
                let result = match (result, self.game_save_notes.iter().position(|(r, _)| *r == req)) {
                    (Ok(mut j), Some(k)) => {
                        let (_, info) = self.game_save_notes.remove(k);
                        j.set("requested_tick", Json::from(info.requested_tick));
                        j.set("retried", Json::from(info.tries));
                        Ok(j)
                    }
                    (r, Some(k)) => {
                        self.game_save_notes.remove(k);
                        r
                    }
                    (r, None) => r,
                };
                if self.game.is_none() {
                    return;
                }
                self.game_call(crate::sim_writes::GameCall::RequestDone(&req, result.map_err(|e| e.to_json())));
            }
        }
    }

    /// `save_list` (never waits for writes: saves being written are listed as such).
    pub(crate) fn list_saves(&self, cursor: usize, limit: usize) -> Json {
        let mut found = Vec::new();
        let mut skipped = 0usize;
        if let Ok(dir) = std::fs::read_dir(&self.saves_dir) {
            for e in dir.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some(EXTENSION) {
                    continue;
                }
                if found.len() >= MAX_SCANNED {
                    skipped += 1;
                    continue;
                }
                let Some(name) = p.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else { continue };
                let writing = self.save_writes.iter().any(|w| w.name == name);
                let item = match read_head(&p).and_then(|(size, h)| savegame::peek(&h).map(|(v, h)| (size, v, h))) {
                    Ok((size, version, h)) => {
                        let created = h.get("created_unix_s").and_then(Json::as_u64).unwrap_or(0);
                        let mut f = vec![("name", Json::from(name.as_str()))];
                        f.extend(header_summary(&h));
                        f.extend([("bytes", Json::from(size)), ("version", Json::from(version)), ("writing", Json::from(writing))]);
                        (created, name, Json::obj(f))
                    }
                    Err(e) => (0, name.clone(), Json::obj([("name", Json::from(name.as_str())), ("unreadable", e.to_json()), ("writing", Json::from(writing))])),
                };
                found.push(item);
            }
        }
        // Newest first, then by name.
        found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let total = found.len();
        let page: Vec<Json> = found.into_iter().skip(cursor).take(limit).map(|(_, _, j)| j).collect();
        let next = (cursor + page.len() < total).then_some(cursor + page.len());
        let writing: Vec<Json> = self.save_writes.iter().map(|w| Json::from(w.name.as_str())).collect();
        Json::obj([
            ("saves", Json::Arr(page)),
            ("total", Json::from(total)),
            ("not_listed", Json::from(skipped)),
            ("next_cursor", Json::from(next)),
            ("writing", Json::Arr(writing)),
            ("dir", Json::from(self.saves_dir.display().to_string())),
        ])
    }

    /// `save_delete` (`key`: the caller's identity, which must be the save's owner unless `force`).
    pub(crate) fn delete_save(&mut self, name: &str, force: bool, key: &str) -> Result<Json, SaveError> {
        if let Some(e) = self.save_busy(name) {
            return Err(e);
        }
        let path = self.save_path(name);
        if !path.exists() {
            return Err(SaveError::new("not_found", format!("there is no save named '{name}'")).with("suggestion", "save_list"));
        }
        let owner = owner_of(&path, key);
        if !force && !matches!(owner, Owner::Mine) {
            return Err(SaveError::new("not_yours", format!("the save '{name}' was written by {}; pass force: true to delete it anyway", whose(&owner))));
        }
        let old = crate::disk::file_len(&path);
        std::fs::remove_file(&path).map_err(|e| io(&path, e))?;
        crate::disk::book().wrote(crate::disk::Category::Saves, "save", -(old.unwrap_or(0) as i64), -1);
        Ok(Json::obj([("deleted", Json::from(name))]))
    }

    /// Serves the hosted game's player-facing requests (its menu's Save, Load, List and Delete).
    /// Saves and loads run in the background like agents' and are answered when done.
    pub(crate) fn serve_game_requests(&mut self) {
        // Its settings, its wish for the clock and a quit ride along (the same places a
        // request is taken).
        self.serve_game_settings();
        self.serve_game_clock();
        if self.windowed && self.game.as_mut().is_some_and(|g| g.take_quit()) {
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "game.quit", game = self.game_name.as_str());
            self.game_quit = true;
        }
        let Some(game) = self.game.as_mut() else { return };
        let mut requests = Vec::new();
        game.take_requests(&mut requests);
        let by = self.game_name.clone();
        let key = format!("game:{by}");
        let now = Instant::now();
        // The game's new requests (capped a frame), then its saves refused for a moment that
        // are due again (not counted against the cap: they were accepted once already).
        let (due, waiting): (Vec<RetrySave>, Vec<RetrySave>) = std::mem::take(&mut self.game_save_retry).into_iter().partition(|r| r.at <= now);
        self.game_save_retry = waiting;
        let tick = self.clock.tick();
        let fresh = requests.into_iter().enumerate().map(|(i, req)| (req, i >= MAX_GAME_REQUESTS, RetryInfo { requested_tick: tick, tries: 0 }));
        let again = due.into_iter().map(|r| (r.req, false, r.info));
        let all: Vec<(GameRequest, bool, RetryInfo)> = fresh.chain(again).collect();
        for (req, over_cap, info) in all {
            let started = match &req {
                _ if over_cap => Err(SaveError::new("too_many_requests", format!("at most {MAX_GAME_REQUESTS} requests a frame"))),
                // The player's own slots: their Save replaces what is there (they confirmed it).
                GameRequest::Save { slot, label } => self.start_save(slot, label.clone(), true, true, &by, &key, Dest::Game(req.clone())).map(|()| None),
                GameRequest::Load { slot } => self.start_load(slot, true, &by, None, Dest::Game(req.clone())).map(|()| None),
                GameRequest::List => Ok(Some(self.list_saves(0, MAX_LIST as usize))),
                GameRequest::Delete { slot } => self.delete_save(slot, true, &key).map(Some),
            };
            match started {
                Ok(None) => {
                    // A retried save says so when it lands (it saves a later tick than asked).
                    if info.tries > 0 {
                        self.game_save_notes.push((req, info));
                        if self.game_save_notes.len() > 16 {
                            self.game_save_notes.remove(0);
                        }
                    }
                }
                Ok(Some(j)) => self.deliver(Dest::Game(req), Ok(j)),
                Err(e) => match retry_delay(&e, matches!(req, GameRequest::Save { .. }), info.tries, self.game_save_retry.len()) {
                    // Refused only for a moment (a navmesh switching settings, a load being
                    // prepared): tried again when due, a few times, before the game hears of it.
                    Some(wait) => self.game_save_retry.push(RetrySave { at: now + wait, info: RetryInfo { requested_tick: info.requested_tick, tries: info.tries + 1 }, req }),
                    None => self.deliver(Dest::Game(req), Err(e)),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review round 2, finding 1 (probe savep.py): a save holding a saved entity's ragdoll is
    /// read the way `save_read` reads it (the worker's `prepare_load`, from a file): the world it
    /// loads has no models attached yet, so the ragdoll couldn't be planned and the save was
    /// refused. With the engine's models handed over, physics comes back at the saved hash.
    #[test]
    fn a_save_with_a_ragdoll_loads_through_prepare_load() {
        use crate::physics::ragdoll::tests::{ragdoll, run, shambler_world};
        let (mut w, id) = shambler_world();
        let mut p = crate::physics::Physics::default();
        run(&mut w, &mut p, 0, 30);
        ragdoll(&mut w, id, r#"{"mode": "limp", "hit": {"bone": "chest", "impulse": [0, 10, -40], "seq": 1}}"#);
        run(&mut w, &mut p, 30, 10);
        let physics = p.save_state(&w, 0).expect("physics ran");
        let meta = Json::obj([
            ("format", Json::from(savegame::FORMAT)),
            ("game", Json::Null),
            ("tick", Json::from(40u64)),
            ("tick_hz", Json::from(30u64)),
            ("dmath", Json::from(specula_core::dmath::VERSION)),
            ("lex_runtime", Json::from(specula_lex::RUNTIME_VERSION)),
        ]);
        let world = savegame::pack_files(&crate::persist::render(&w));
        let bytes = savegame::encode(&meta, &[Section { name: "world".into(), data: world }, Section { name: "physics".into(), data: physics }]);
        let path = std::env::temp_dir().join(format!("specula-ragdoll-save-{}.specsave", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let load = |models| prepare_load(&path, w.shape_limits(), None, 30, 1, None, specula_lex::Natives::new(), models);
        let prepared = load(w.model_source().cloned()).map_err(|e| e.to_json().to_string()).expect("loads with the engine's models");
        assert_eq!(prepared.physics.space.hash(), p.space.hash());
        assert!(prepared.physics.ragdolls.live.contains_key(&id));
        // Without models it can't (and says so once, with the section as the error's own field).
        let e = load(None).err().expect("no rig without models");
        assert_eq!(e.code, "save_mismatch");
        let text = e.to_json().to_string();
        assert_eq!(text.matches("\"section\"").count(), 1, "{text}");
        let _ = std::fs::remove_file(&path);
    }

    /// A saves folder has a cap: a save that would pass it is `saves_full` (saving over a save
    /// counts only the new one).
    #[test]
    fn a_full_saves_folder_is_refused() {
        let dir = std::env::temp_dir().join(format!("specula_saves_full_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (n, len) in [("a", 400usize), ("b", 300)] {
            std::fs::write(dir.join(format!("{n}.{EXTENSION}")), vec![0u8; len]).unwrap();
        }
        std::fs::write(dir.join("notes.txt"), vec![0u8; 5000]).unwrap();
        let c = dir.join(format!("c.{EXTENSION}"));
        assert!(dir_room(&dir, &c, 300, 1000).is_ok(), "700 + 300 fits 1000 (other files don't count)");
        let e = dir_room(&dir, &c, 301, 1000).unwrap_err();
        assert_eq!(e.code, "saves_full");
        assert!(e.to_json().to_string().contains("\"used_bytes\":700"), "{}", e.to_json());
        // Saving over 'a' replaces its 400 bytes.
        assert!(dir_room(&dir, &dir.join(format!("a.{EXTENSION}")), 700, 1000).is_ok());
        assert!(dir_room(&dir.join("missing"), &c, 10, 1000).is_ok(), "a folder not made yet is empty");
        // Two writers at once: the second sees the first's reservation (the review's two
        // concurrent saves that passed the cap together).
        let first = Reservation::take(&dir, &c, 200, 1000).unwrap();
        let e = Reservation::take(&dir, &dir.join(format!("d.{EXTENSION}")), 200, 1000).map(|_| ()).unwrap_err();
        assert_eq!(e.code, "saves_full", "700 on disk + 200 being written + 200 > 1000");
        assert!(e.to_json().to_string().contains("\"being_written_bytes\":200"), "{}", e.to_json());
        drop(first);
        assert!(Reservation::take(&dir, &dir.join(format!("d.{EXTENSION}")), 200, 1000).is_ok(), "room again once the first is done");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_momentary_save_refusals_are_retried_and_boundedly() {
        let building = SaveError::new("nav_building", "a navmesh is switching settings").with("retry_after_ms", 300);
        assert_eq!(retry_delay(&building, true, 0, 0), Some(std::time::Duration::from_millis(300)));
        assert_eq!(retry_delay(&building, false, 0, 0), None, "only saves");
        assert_eq!(retry_delay(&building, true, MAX_GAME_SAVE_RETRIES, 0), None, "a few times");
        assert_eq!(retry_delay(&building, true, 0, MAX_WAITING_SAVES), None, "a few at once");
        assert_eq!(retry_delay(&SaveError::new("saves_full", "full"), true, 0, 0), None, "other refusals reach the game");
        let quick = SaveError::new("nav_building", "x").with("retry_after_ms", 1);
        assert_eq!(retry_delay(&quick, true, 0, 0), Some(std::time::Duration::from_millis(50)), "clamped");
        assert_eq!(retry_delay(&SaveError::new("nav_building", "x"), true, 3, 2), Some(std::time::Duration::from_millis(500)), "a default wait");
    }

    #[test]
    fn names_and_arguments_are_checked() {
        assert!(validate_save_name("slot_1").is_ok());
        assert!(validate_save_name("before-wave-4").is_ok());
        for bad in ["", "a/b", "..", "con", "x".repeat(65).as_str(), "sp ace"] {
            assert!(validate_save_name(bad).is_err(), "{bad}");
        }
        let args = Json::parse(r#"{"name": "slot_1", "label": "before wave 4"}"#).unwrap();
        assert!(matches!(parse("save_write", &args), Some(Ok(SaveCall::Write { overwrite: false, force: false, .. }))));
        let bad = Json::parse(r#"{"name": "slot_1", "lable": "x"}"#).unwrap();
        assert!(parse("save_write", &bad).unwrap().is_err());
        assert!(parse("world_spawn", &args).is_none());
        for d in definitions() {
            assert!(d.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') && d.name.len() <= 64);
        }
    }

    #[test]
    fn owners_are_identities_not_names() {
        let dir = std::env::temp_dir().join(format!("specula-owner-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.specsave");
        let meta = Json::obj([("format", Json::from(savegame::FORMAT)), ("created_by", Json::from("ann")), ("owner", Json::from(owner_hash("key-1")))]);
        std::fs::write(&path, savegame::encode(&meta, &[])).unwrap();
        assert!(matches!(owner_of(&path, "key-1"), Owner::Mine));
        assert!(matches!(owner_of(&path, "key-2"), Owner::Theirs(n) if n == "ann"), "another agent, even one now called ann");
        std::fs::write(&path, b"not a save").unwrap();
        assert!(matches!(owner_of(&path, "key-1"), Owner::Unknown));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
