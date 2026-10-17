//! Save games: what the module host keeps for modules (docs/design/SAVES.md).
//!
//! The engine owns all module state (MODULES.md), so a save holds it whole: each module's
//! state store and RNG stream positions (with its seed and load count), the events emitted
//! during the last tick (delivered at the next one), and, per running module, its input (what
//! agents hold, the edges the next tick derives from) and whether it was disabled. The code is
//! not in the save: a save names each module's build hash, and a load reports modules that are
//! missing or built differently (DETERMINISM.md rule 8: a mismatch is reported, not a failure).

use super::*;
use specula_core::bytes::{ByteReader, ByteWriter, DecodeError};
use specula_core::hash::StableHasher;

/// Bumped when the layout changes.
pub const LOGIC_VERSION: u32 = 1;

fn bad(e: DecodeError) -> WorldError {
    WorldError::new("bad_save", format!("the save's logic section is damaged: {e}"))
        .with("section", "logic")
        .with("decode", e.code)
        .with("offset", e.offset)
        .with("field", e.field.as_str())
}

fn value(r: &mut ByteReader<'_>, field: &str) -> Result<Value, WorldError> {
    let at = r.offset();
    let b = r.blob(field, 1 << 22).map_err(bad)?;
    Value::decode(b).map_err(|e| bad(DecodeError::new("invalid", at, field, format!("{e:?}"))))
}

fn event(w: &mut ByteWriter, e: &EventRec) {
    w.str(&e.name);
    w.blob(&e.data.encode());
    w.str(&e.from);
    w.u64(e.tick);
}

/// A parsed logic section (see [`LogicHost::parse_state`]).
pub struct LogicSnapshot {
    shared: BTreeMap<String, Shared>,
    events: Vec<EventRec>,
    /// `(module, build hash, generation, disabled json or "", input bytes)`.
    slots: Vec<(String, u64, u64, String, Vec<u8>)>,
}

impl LogicHost {
    /// The host's state for a save, or `None` when no module ever ran.
    pub fn save_state(&self) -> Option<Vec<u8>> {
        if self.slots.is_empty() && self.shared.is_empty() {
            return None;
        }
        let mut w = ByteWriter::new();
        w.u32(LOGIC_VERSION);
        w.len_u32(self.shared.len());
        for (name, s) in &self.shared {
            w.str(name);
            w.u64(s.seed);
            w.u64(s.loads);
            w.len_u32(s.store.len());
            for (k, v) in &s.store {
                w.str(k);
                w.blob(&v.encode());
            }
            w.len_u32(s.rngs.len());
            for (stream, rng) in &s.rngs {
                let (state, inc) = rng.to_parts();
                w.str(stream);
                w.u64(state);
                w.u64(inc);
            }
        }
        w.len_u32(self.pending_events.len());
        self.pending_events.iter().for_each(|e| event(&mut w, e));
        w.len_u32(self.slots.len());
        for (name, slot) in &self.slots {
            w.str(name);
            w.u64(slot.code.build_hash());
            w.u64(slot.generation);
            w.str(&slot.disabled.as_ref().map(Json::to_string).unwrap_or_default());
            let mut input = ByteWriter::new();
            slot.input.save(&mut input);
            w.blob(input.bytes());
        }
        Some(w.into_bytes())
    }

    /// Puts [`LogicHost::save_state`]'s state back ([`LogicHost::parse_state`] then
    /// [`LogicHost::apply_state`]).
    pub fn restore_state(&mut self, data: &[u8]) -> Result<Vec<Json>, WorldError> {
        let snap = LogicHost::parse_state(data)?;
        Ok(self.apply_state(snap))
    }

    /// Reads and checks [`LogicHost::save_state`]'s bytes without changing anything.
    pub fn parse_state(data: &[u8]) -> Result<LogicSnapshot, WorldError> {
        let mut r = ByteReader::new(data);
        let version = r.u32("version").map_err(bad)?;
        if version != LOGIC_VERSION {
            return Err(WorldError::new("unsupported_version", format!("logic save version {version}; this build reads {LOGIC_VERSION}")).with("section", "logic"));
        }
        let n = r.count("modules", 4096, 24).map_err(bad)?;
        let mut shared = BTreeMap::new();
        for _ in 0..n {
            let name = r.str("module", 256).map_err(bad)?;
            let mut s = Shared { seed: r.u64("seed").map_err(bad)?, loads: r.u64("loads").map_err(bad)?, ..Shared::default() };
            let keys = r.count("store", 65_536, 8).map_err(bad)?;
            for _ in 0..keys {
                let k = r.str("store.key", 4096).map_err(bad)?;
                let v = value(&mut r, "store.value")?;
                s.put(&k, Some(v));
            }
            let streams = r.count("rngs", 65_536, 20).map_err(bad)?;
            for _ in 0..streams {
                let stream = r.str("rngs.stream", 4096).map_err(bad)?;
                let (state, inc) = (r.u64("rngs.state").map_err(bad)?, r.u64("rngs.inc").map_err(bad)?);
                s.rngs.insert(stream, Rng::from_parts(state, inc));
            }
            shared.insert(name, s);
        }
        let n = r.count("events", MAX_PENDING_EVENTS * 4, 20).map_err(bad)?;
        let mut events = Vec::with_capacity(n);
        for _ in 0..n {
            let name = r.str("events.name", 4096).map_err(bad)?;
            let data = value(&mut r, "events.data")?;
            let from = r.str("events.from", 4096).map_err(bad)?;
            let tick = r.u64("events.tick").map_err(bad)?;
            events.push(EventRec { name, data, from, tick });
        }
        let n = r.count("slots", 4096, 24).map_err(bad)?;
        let mut slots = Vec::with_capacity(n);
        for _ in 0..n {
            let name = r.str("slot", 256).map_err(bad)?;
            let hash = r.u64("slot.build_hash").map_err(bad)?;
            let generation = r.u64("slot.generation").map_err(bad)?;
            let disabled = r.str("slot.disabled", 1 << 16).map_err(bad)?;
            let input = r.blob("slot.input", 1 << 22).map_err(bad)?;
            slots.push((name, hash, generation, disabled, input));
        }
        r.finish().map_err(bad)?;
        // Check the inputs' bytes before changing anything (a scratch input with no actions
        // reads them all and keeps nothing).
        for (_, _, _, _, input) in &slots {
            let mut probe = ModuleInput::new("").map_err(|e| WorldError::new("internal", e.to_string()))?;
            let mut check = ByteReader::new(input);
            probe.restore(&mut check).map_err(bad)?;
            check.finish().map_err(bad)?;
        }
        Ok(LogicSnapshot { shared, events, slots: slots.into_iter().map(|(n, h, g, d, i)| (n, h, g, d, i.to_vec())).collect() })
    }

    /// Applies a parsed snapshot. Stores and RNG positions replace the host's (for loaded
    /// modules and for modules loaded later alike). Returns notes for the caller: modules the
    /// save had running that aren't loaded here, or are another build.
    pub fn apply_state(&mut self, snap: LogicSnapshot) -> Vec<Json> {
        let mut notes = Vec::new();
        self.shared = snap.shared;
        self.pending_events = snap.events;
        for (name, hash, generation, disabled, input) in snap.slots {
            match self.slots.get_mut(&name) {
                None => notes.push(Json::obj([
                    ("module", Json::from(name.as_str())),
                    ("note", Json::from("was running when saved but isn't loaded here: its state is restored and waits for logic_build or logic_reload; until then the run won't match the saving run")),
                    ("build_hash", Json::from(format!("{hash:016x}"))),
                ])),
                Some(slot) => {
                    if slot.code.build_hash() != hash {
                        notes.push(Json::obj([
                            ("module", Json::from(name.as_str())),
                            ("note", Json::from("another build than the one saved: the run is only expected to match with the same build")),
                            ("saved_build_hash", Json::from(format!("{hash:016x}"))),
                            ("build_hash", Json::from(format!("{:016x}", slot.code.build_hash()))),
                        ]));
                    }
                    slot.generation = generation;
                    slot.disabled = if disabled.is_empty() { None } else { Some(Json::parse(&disabled).unwrap_or(Json::from(disabled))) };
                    // (Checked by parse_state on a scratch input: the bytes read cleanly.)
                    let _ = slot.input.restore(&mut ByteReader::new(&input));
                }
            }
        }
        for name in self.slots.keys() {
            if !self.shared.contains_key(name) {
                notes.push(Json::obj([
                    ("module", Json::from(name.as_str())),
                    ("note", Json::from("loaded here but not in the save: it keeps running with an empty state store")),
                ]));
            }
        }
        notes
    }

    /// A hash of the state modules keep in the engine (stores, RNG positions, pending events),
    /// for the desync detector; `None` without modules.
    pub fn state_hash(&mut self) -> Option<u64> {
        if self.slots.is_empty() && self.shared.is_empty() {
            return None;
        }
        let mut buf = std::mem::take(&mut self.hash_scratch);
        let mut bytes = |h: &mut StableHasher, v: &Value| {
            buf.clear();
            v.encode_into(&mut buf);
            h.write_bytes(&buf);
        };
        let mut h = StableHasher::with_seed(0x6c6f_6769_6331);
        for (name, s) in &self.shared {
            h.write_str(name);
            h.write_u64(s.seed);
            // The store (up to a MiB) is hashed again only after it changed.
            let store = match s.store_hash.get() {
                Some(x) => x,
                None => {
                    let mut sh = StableHasher::with_seed(0x6c6f_6769_6332);
                    for (k, v) in &s.store {
                        sh.write_str(k);
                        bytes(&mut sh, v);
                    }
                    let x = sh.finish();
                    s.store_hash.set(Some(x));
                    x
                }
            };
            h.write_u64(store);
            for (stream, rng) in &s.rngs {
                let (a, b) = rng.to_parts();
                h.write_str(stream);
                h.write_u64(a);
                h.write_u64(b);
            }
        }
        for e in &self.pending_events {
            h.write_str(&e.name);
            bytes(&mut h, &e.data);
            h.write_u64(e.tick);
        }
        for (name, slot) in &self.slots {
            h.write_str(name);
            h.write_bool(slot.disabled.is_some());
        }
        self.hash_scratch = buf;
        Some(h.finish())
    }
}
