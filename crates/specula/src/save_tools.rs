//! Save games in the engine (docs/design/SAVES.md): `save_write`, `save_read`, `save_list`,
//! `save_delete`, and the hosted game's player-facing Save and Load ([`GameRequest`]).
//!
//! A save is the simulation's whole state at a tick boundary: the world (as world files), the
//! hosted game's own state, the module host's stores and RNG positions, physics' hidden state and
//! the clock. Loading it makes the save's tick the engine's, so the loaded run continues with the
//! same `world_hash` and subsystem hashes as the run that saved (`desync_compare` shows it).
//! Saves are files `<saves dir>/<name>.specsave` (`--saves`, default `./saves`), written to a
//! temporary file and renamed, so a crash leaves the old save or the new one.

use crate::game::{Ctx, GameRequest};
use crate::journal::{Kind, Writer};
use crate::savegame::{self, SaveError, Section, EXTENSION};
use crate::tools::{bool_arg, check_keys, int_arg, string_arg};
use crate::world::Recording;
use crate::Engine;
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

pub enum SaveCall {
    Write { name: String, label: Option<String>, overwrite: bool },
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
            check_keys(args, name, &[&["name", "label", "overwrite"]])?;
            let label = string_arg(args, "label")?.map(str::to_owned);
            if label.as_ref().is_some_and(|l| l.chars().count() > MAX_LABEL) {
                return Err(format!("'label' is at most {MAX_LABEL} characters"));
            }
            Ok(SaveCall::Write { name: name_of(args)?, label, overwrite: bool_arg(args, "overwrite")?.unwrap_or(false) })
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
             is only replaced with overwrite: true.",
            schema(vec![name(), ("label", t("string", "A note shown by save_list (\"before wave 4\"), up to 200 characters")), ("overwrite", t("boolean", "Replace an existing save of that name"))], &["name"]),
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
            "Delete a save game. Only the agent that wrote it may, unless force: true.",
            schema(vec![name(), ("force", t("boolean", "Delete another agent's save"))], &["name"]),
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
        subs
    }

    pub(crate) fn handle_save(&mut self, call: SaveCall, w: Writer, by: &str) -> ToolOutput {
        match call {
            SaveCall::Write { name, label, overwrite } => match self.write_save(&name, label, overwrite, by) {
                Ok(j) => out(j),
                Err(e) => fail(&e),
            },
            SaveCall::Read { name, inspect: true, .. } => match self.inspect_save(&name) {
                Ok(j) => out(j),
                Err(e) => fail(&e),
            },
            SaveCall::Read { name, force, .. } => match self.read_save(&name, force, w) {
                Ok(j) => out(j),
                Err(e) => fail(&e),
            },
            SaveCall::List { cursor, limit } => out(self.list_saves(cursor, limit)),
            SaveCall::Delete { name, force } => match self.delete_save(&name, force, by) {
                Ok(j) => out(j),
                Err(e) => fail(&e),
            },
        }
    }

    /// `save_write`.
    pub(crate) fn write_save(&mut self, name: &str, label: Option<String>, overwrite: bool, by: &str) -> Result<Json, SaveError> {
        validate_save_name(name).map_err(|m| SaveError::new("invalid_argument", m))?;
        let path = self.save_path(name);
        if path.exists() && !overwrite {
            let by = read_head(&path).ok().and_then(|(_, h)| savegame::peek(&h).ok()).and_then(|(_, h)| h.get("created_by").cloned()).unwrap_or(Json::Null);
            return Err(SaveError::new("save_exists", format!("a save named '{name}' exists; pass overwrite: true to replace it, or pick another name"))
                .with("name", name)
                .with("created_by", by));
        }
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
        let files = crate::persist::render(&self.world);
        let world = savegame::pack_files(&files);
        let snapshot_ms = t0.elapsed().as_secs_f64() * 1000.0;
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
            ("created_unix_s", Json::from(created)),
        ];
        if let Some(l) = &label {
            meta.push(("label", Json::from(l.as_str())));
        }
        let mut sections = vec![Section { name: "world".into(), data: world }];
        for (n, d) in [("game", game), ("physics", physics), ("logic", logic)] {
            if let Some(data) = d {
                sections.push(Section { name: n.into(), data });
            }
        }
        let sizes = Json::obj(sections.iter().map(|s| (s.name.as_str(), Json::from(s.data.len()))));
        let t1 = Instant::now();
        let bytes = savegame::encode(&Json::obj(meta), &sections);
        let encode_ms = t1.elapsed().as_secs_f64() * 1000.0;
        let t2 = Instant::now();
        std::fs::create_dir_all(&self.saves_dir).map_err(|e| io(&self.saves_dir, e))?;
        let tmp = path.with_extension(format!("{EXTENSION}.tmp"));
        {
            use std::io::Write as _;
            let mut f = std::fs::File::create(&tmp).map_err(|e| io(&tmp, e))?;
            f.write_all(&bytes).and_then(|()| f.sync_all()).map_err(|e| {
                let _ = std::fs::remove_file(&tmp);
                io(&tmp, e)
            })?;
        }
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            io(&path, e)
        })?;
        let write_ms = t2.elapsed().as_secs_f64() * 1000.0;
        let ms = |v: f64| Json::from((v * 100.0).round() / 100.0);
        let mut notes = Vec::new();
        if self.game.is_some() && !sections.iter().any(|s| s.name == "game") {
            notes.push(Json::from("the hosted game keeps no save state: a load restores the world, physics and modules, and the game starts from what the world shows"));
        }
        Ok(Json::obj([
            ("saved", Json::from(name)),
            ("file", Json::from(path.display().to_string())),
            ("tick", Json::from(tick)),
            ("world_hash", Json::from(hex(world_hash))),
            ("subsystems", Json::obj(subs.iter().map(|(n, h)| (*n, Json::from(hex(*h)))))),
            ("bytes", Json::from(bytes.len())),
            ("sections", sizes),
            ("timings_ms", Json::obj([("snapshot", ms(snapshot_ms)), ("encode", ms(encode_ms)), ("write", ms(write_ms)), ("total", ms(t0.elapsed().as_secs_f64() * 1000.0))])),
            ("notes", Json::Arr(notes)),
        ]))
    }

    fn read_file(&self, name: &str) -> Result<savegame::SaveFile, SaveError> {
        let path = self.save_path(name);
        let size = std::fs::metadata(&path).map_err(|e| io(&path, e))?.len();
        if size > savegame::MAX_FILE_BYTES {
            return Err(SaveError::new("too_large", format!("{} is {size} bytes; saves are read up to {}", path.display(), savegame::MAX_FILE_BYTES)));
        }
        let bytes = std::fs::read(&path).map_err(|e| io(&path, e))?;
        savegame::decode(&bytes).map_err(|e| e.with("file", path.display().to_string()))
    }

    /// `save_read {inspect}`: the header and sections of a save that reads cleanly.
    fn inspect_save(&self, name: &str) -> Result<Json, SaveError> {
        let f = self.read_file(name)?;
        let mut j = f.meta.clone();
        j.set("name", Json::from(name));
        j.set("version", Json::from(f.version));
        j.set("sections", Json::obj(f.sections.iter().map(|s| (s.name.as_str(), Json::from(s.data.len())))));
        j.set("reads", Json::from(true));
        Ok(j)
    }

    /// `save_read`: see the module docs. Everything that can fail is checked before anything
    /// live changes.
    pub(crate) fn read_save(&mut self, name: &str, force: bool, w: Writer) -> Result<Json, SaveError> {
        let t0 = Instant::now();
        self.swarm.check_replace_world(w).map_err(|e| {
            let mut s = SaveError::new("claimed", e.message.clone());
            s.details = e.details.clone();
            s
        })?;
        self.finish_autosave(true);
        self.save_board_now();
        if self.dirty() && !force && self.world_name.is_some() {
            return Err(SaveError::new("unsaved_changes", "the live world has unsaved changes; world_save first, or pass force: true to discard them")
                .with("current_world", self.world_name.clone().map_or(Json::Null, Json::from)));
        }
        let f = self.read_file(name)?;
        let meta = &f.meta;
        let game = meta.get("game").and_then(Json::as_str).map(str::to_owned);
        let hosted = self.game.as_ref().map(|_| self.game_name.clone());
        if game != hosted {
            return Err(SaveError::new(
                "wrong_game",
                format!("the save is from {} but this engine hosts {}", game.as_deref().unwrap_or("no game"), hosted.as_deref().unwrap_or("no game")),
            ));
        }
        let hz = meta.get("tick_hz").and_then(Json::as_u64).unwrap_or(0);
        if hz != u64::from(self.clock.hz()) {
            return Err(SaveError::new("wrong_tick_rate", format!("the save ticks at {hz} Hz, this engine at {} Hz", self.clock.hz())));
        }
        let Some(tick) = meta.get("tick").and_then(Json::as_u64).filter(|t| *t < 1 << 53) else {
            return Err(SaveError::new("invalid_header", "the header's 'tick' must be a whole number"));
        };
        // The world.
        let world_bytes = f.section("world").ok_or_else(|| SaveError::new("missing_section", "the save has no world section").in_section("world"))?;
        let files = savegame::unpack_files(world_bytes)?;
        let (world, _) = crate::persist::load_with_limits(self.world.shape_limits(), |file| {
            files
                .iter()
                .find(|(n, _)| n == file)
                .map(|(_, t)| t.clone())
                .ok_or_else(|| crate::persist::PersistError::new("not_found", format!("the save's world has no file {file}")))
        })
        .map_err(|e| SaveError::new("corrupt_section", format!("the save's world doesn't load: {e}")).in_section("world").with("cause", e.to_json()))?;
        // The game's state.
        let parsed = match (f.section("game"), self.game.as_ref()) {
            (Some(bytes), Some(g)) => Some(g.parse_state(bytes).map_err(|m| SaveError::new("game_state", format!("the game can't load this save: {m}")).in_section("game"))?),
            (Some(_), None) => return Err(SaveError::new("wrong_game", "the save has game state but no game is hosted")),
            (None, _) => None,
        };
        // Modules and physics, checked against the save's world.
        let logic = match f.section("logic") {
            Some(b) => Some(crate::logic::LogicHost::parse_state(b).map_err(|e| SaveError::new(e.code, e.message.clone()).in_section("logic"))?),
            None => None,
        };
        let generation = self.world_generation + 1;
        let mut physics = crate::physics::Physics::default();
        if let Some(b) = f.section("physics") {
            physics.restore_state(&world, generation, tick, b).map_err(|e| {
                let mut s = SaveError::new(e.code, e.message.clone()).in_section("physics");
                s.details = e.details.clone();
                s
            })?;
        }
        let parse_ms = t0.elapsed().as_secs_f64() * 1000.0;
        // Everything checks: make it live.
        let t1 = Instant::now();
        self.clock.set_tick(tick);
        let mut notes: Vec<Json> = self.replace_world(world, &format!("save:{name}")).into_iter().map(Json::from).collect();
        debug_assert_eq!(self.world_generation, generation);
        self.physics = physics;
        if let Some(snap) = logic {
            notes.extend(self.logic.apply_state(snap));
        } else if !self.logic.is_empty() {
            notes.push(Json::from("the save has no module state; running modules keep theirs"));
        }
        if let (Some(state), Some(game)) = (parsed, self.game.as_mut()) {
            self.world.set_recording(Recording::Snapshot);
            game.apply_state(&mut Ctx { world: &mut self.world, clock: &self.clock, agent: &self.game_name, windowed: self.windowed }, state);
            self.world.set_recording(Recording::Full);
            self.journal_game(Kind::GameStart);
        }
        // The loaded world isn't a world folder: world_save names one.
        self.world_name = None;
        self.saved = None;
        self.world_lock = None;
        self.saved_revision = self.world.revision();
        self.hash_cache = None;
        self.last_advance = Instant::now();
        let apply_ms = t1.elapsed().as_secs_f64() * 1000.0;
        // Hashes now against the saved ones.
        let world_now = self.state_hash();
        let world_saved = meta.get("world_hash").and_then(Json::as_str).unwrap_or("");
        let subs_now = self.subsystem_hashes();
        let saved_subs = meta.get("subsystems").and_then(Json::as_object).unwrap_or_default();
        let mut all_match = hex(world_now) == world_saved;
        let mut subsystems = Vec::new();
        let mut names: Vec<String> = saved_subs.iter().map(|(k, _)| k.clone()).collect();
        for (n, _) in &subs_now {
            if !names.iter().any(|x| x == n) {
                names.push((*n).to_owned());
            }
        }
        for n in names {
            let saved = saved_subs.iter().find(|(k, _)| *k == n).and_then(|(_, v)| v.as_str()).map(str::to_owned);
            let now = subs_now.iter().find(|(k, _)| *k == n).map(|(_, h)| hex(*h));
            let same = saved == now;
            all_match &= same;
            subsystems.push(Json::obj([("name", Json::from(n)), ("saved", Json::from(saved)), ("now", Json::from(now)), ("match", Json::from(same))]));
        }
        let ms = |v: f64| Json::from((v * 100.0).round() / 100.0);
        Ok(Json::obj([
            ("loaded", Json::from(name)),
            ("tick", Json::from(tick)),
            ("game", Json::from(game)),
            ("label", meta.get("label").cloned().unwrap_or(Json::Null)),
            ("created_by", meta.get("created_by").cloned().unwrap_or(Json::Null)),
            ("restored", Json::obj(f.sections.iter().map(|s| (s.name.as_str(), Json::from(true))))),
            ("world_hash", Json::obj([("saved", Json::from(world_saved)), ("now", Json::from(hex(world_now))), ("match", Json::from(hex(world_now) == world_saved))])),
            ("subsystems", Json::Arr(subsystems)),
            ("match", Json::from(all_match)),
            ("entities", Json::from(self.world.persistent_len())),
            ("timings_ms", Json::obj([("read_and_check", ms(parse_ms)), ("apply", ms(apply_ms)), ("total", ms(t0.elapsed().as_secs_f64() * 1000.0))])),
            ("notes", Json::arr(notes.into_iter().take(20))),
            ("next", Json::from("world_save {name} keeps the loaded world as world files; desync_trace from this tick on compares with the saving run's")),
        ]))
    }

    /// `save_list`.
    fn list_saves(&self, cursor: usize, limit: usize) -> Json {
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
                let item = match read_head(&p).and_then(|(size, h)| savegame::peek(&h).map(|(v, h)| (size, v, h))) {
                    Ok((size, version, h)) => {
                        let created = h.get("created_unix_s").and_then(Json::as_u64).unwrap_or(0);
                        let j = Json::obj([
                            ("name", Json::from(name.as_str())),
                            ("label", h.get("label").cloned().unwrap_or(Json::Null)),
                            ("game", h.get("game").cloned().unwrap_or(Json::Null)),
                            ("tick", h.get("tick").cloned().unwrap_or(Json::Null)),
                            ("created_by", h.get("created_by").cloned().unwrap_or(Json::Null)),
                            ("created_unix_s", Json::from(created)),
                            ("bytes", Json::from(size)),
                            ("version", Json::from(version)),
                        ]);
                        (created, name, j)
                    }
                    Err(e) => (0, name.clone(), Json::obj([("name", Json::from(name.as_str())), ("unreadable", e.to_json())])),
                };
                found.push(item);
            }
        }
        // Newest first, then by name.
        found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let total = found.len();
        let page: Vec<Json> = found.into_iter().skip(cursor).take(limit).map(|(_, _, j)| j).collect();
        let next = (cursor + page.len() < total).then_some(cursor + page.len());
        Json::obj([
            ("saves", Json::Arr(page)),
            ("total", Json::from(total)),
            ("not_listed", Json::from(skipped)),
            ("next_cursor", Json::from(next)),
            ("dir", Json::from(self.saves_dir.display().to_string())),
        ])
    }

    /// `save_delete`.
    fn delete_save(&mut self, name: &str, force: bool, by: &str) -> Result<Json, SaveError> {
        let path = self.save_path(name);
        if !path.exists() {
            return Err(SaveError::new("not_found", format!("there is no save named '{name}'")).with("suggestion", "save_list"));
        }
        let owner = read_head(&path).ok().and_then(|(_, h)| savegame::peek(&h).ok()).and_then(|(_, h)| h.get("created_by").and_then(Json::as_str).map(str::to_owned));
        if !force && owner.as_deref().is_some_and(|o| o != by) {
            return Err(SaveError::new("not_yours", format!("the save '{name}' was written by {}; pass force: true to delete it anyway", owner.unwrap_or_default())));
        }
        std::fs::remove_file(&path).map_err(|e| io(&path, e))?;
        Ok(Json::obj([("deleted", Json::from(name))]))
    }

    /// Serves the hosted game's player-facing requests (its menu's Save and Load).
    pub(crate) fn serve_game_requests(&mut self) {
        let Some(game) = self.game.as_mut() else { return };
        let mut requests = Vec::new();
        game.take_requests(&mut requests);
        for req in requests.into_iter().take(4) {
            let by = self.game_name.clone();
            let result = match &req {
                GameRequest::Save { slot, label } => self.write_save(slot, label.clone(), true, &by).map_err(|e| e.to_json()),
                GameRequest::Load { slot } => self.read_save(slot, true, Writer::new(&by, None)).map_err(|e| e.to_json()),
                GameRequest::List => Ok(self.list_saves(0, MAX_LIST as usize)),
            };
            let Some(game) = self.game.as_mut() else { return };
            self.world.set_recording(Recording::Snapshot);
            game.request_done(&mut Ctx { world: &mut self.world, clock: &self.clock, agent: &self.game_name, windowed: self.windowed }, &req, result);
            self.world.set_recording(Recording::Full);
            self.journal_game(Kind::GameFrame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_arguments_are_checked() {
        assert!(validate_save_name("slot_1").is_ok());
        assert!(validate_save_name("before-wave-4").is_ok());
        for bad in ["", "a/b", "..", "con", "x".repeat(65).as_str(), "sp ace"] {
            assert!(validate_save_name(bad).is_err(), "{bad}");
        }
        let args = Json::parse(r#"{"name": "slot_1", "label": "before wave 4"}"#).unwrap();
        assert!(matches!(parse("save_write", &args), Some(Ok(SaveCall::Write { overwrite: false, .. }))));
        let bad = Json::parse(r#"{"name": "slot_1", "lable": "x"}"#).unwrap();
        assert!(parse("save_write", &bad).unwrap().is_err());
        assert!(parse("world_spawn", &args).is_none());
        for d in definitions() {
            assert!(d.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') && d.name.len() <= 64);
        }
    }
}
