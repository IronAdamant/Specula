//! The save section `elementa` and the hash (docs/design/SAVES.md).
//!
//! The section is self-contained: it carries the config it ran under (materials, tags, grid,
//! limits), so a load needs nothing else and the run continues bit for bit. Floats are their
//! exact bits. On load every count is checked against its cap and the bytes left before anything
//! is allocated, every number against its range, and every cross-reference (a material index, a
//! phase the material allows, ids in order, a tick not in the future), so a damaged or hand-made
//! section is an error with a code, never a panic or a hang in a later step.

use crate::config::{Config, Limits, Material, MAX_FUEL_S, MAX_MATERIALS, MAX_NAME, MAX_TAGS};
use crate::error::ElementaError;
use crate::grid::cells_in;
use crate::sim::{check_box, query_box, Cause, Cursor, Elementa, Event, EventKind, Events, Node, Phase, EVENT_KINDS, MAX_EVENTS_PER_KIND, MAX_TICK, NOT_READY};
use specula_core::bytes::{ByteReader, ByteWriter};
use specula_core::hash::StableHasher;

/// The section's version. Others are refused with `unsupported_version`. (Its layout changed
/// on 2026-11-24 while still version 1: the per-node `owed` went, the cursor resumes by id, and
/// the stream's two words became the ember key. Nothing had shipped, so no save of the earlier
/// layout exists to read.)
pub const SECTION_VERSION: u32 = 1;

/// Bytes a material takes at least (an empty name's length, flags and floats).
const MATERIAL_BYTES: usize = 4 + 1 + 4 * 5 + 1 + 1 + 4 * 3 + 4;
const TAG_BYTES: usize = 4 + 2;
const NODE_BYTES: usize = 8 + 2 + 12 + 12 + 4 * 3 + 1 + 8 + 8;
const EVENT_BYTES: usize = 1 + 8 + 8 + 12 + 1;

fn invalid(message: impl Into<String>) -> ElementaError {
    ElementaError::new("snapshot_invalid", message)
}

impl Elementa {
    /// Writes the section (without the engine's framing).
    pub fn save_state(&self, w: &mut ByteWriter) {
        w.u32(SECTION_VERSION);
        let c = &self.config;
        w.len_u32(c.materials.len());
        for m in &c.materials {
            w.str(&m.name);
            w.bool(m.ignite.is_some());
            w.f32(m.ignite.unwrap_or(0.0));
            for x in [m.burn_s, m.heat_out, m.flame, m.cool, m.embers] {
                w.f32(x);
            }
            w.bool(m.conducts);
            w.bool(m.freeze.is_some());
            w.f32(m.freeze.unwrap_or(0.0));
            w.f32(m.quench);
            w.f32(m.start_heat);
        }
        w.len_u32(c.tags.len());
        for (t, m) in &c.tags {
            w.str(t);
            w.u16(*m);
        }
        w.f32(c.cell_m);
        w.f32(c.reach_m);
        w.u32(c.spread_hz);
        w.u32(c.limits.nodes);
        w.u32(c.limits.burning);
        w.u32(c.limits.work);

        w.u32(self.tick_hz);
        w.u64(self.tick);
        w.u64(self.ember_key);
        w.u64(self.cursor.id);
        w.u32(self.cursor.cell);
        w.u64(self.cursor.next);
        w.u64(self.refused);
        w.vec3(self.wind);
        w.f32(self.ambient);

        w.len_u32(self.nodes.len());
        for n in &self.nodes {
            w.u64(n.id);
            w.u16(n.material);
            w.vec3(n.center);
            w.vec3(n.half);
            w.f32(n.heat);
            w.f32(n.fuel);
            w.f32(n.wet);
            w.u8(n.phase as u8);
            w.u64(n.since);
            w.u64(n.ready_at);
        }
        w.len_u32(self.events.list.len());
        for e in &self.events.list {
            w.u8(e.kind as u8);
            w.u64(e.tick);
            w.u64(e.id);
            w.vec3(e.at);
            w.u8(e.by.map_or(0, |b| b as u8));
        }
        for u in self.events.unnamed {
            w.u32(u);
        }
    }

    /// The section as bytes.
    pub fn save(&self) -> Vec<u8> {
        let mut w = ByteWriter::with_capacity(64 + self.nodes.len() * NODE_BYTES);
        self.save_state(&mut w);
        w.into_bytes()
    }

    /// Reads a whole section: [`Elementa::restore_state`], then nothing may be left over.
    pub fn load(bytes: &[u8]) -> Result<Elementa, ElementaError> {
        let mut r = ByteReader::new(bytes);
        let e = Elementa::restore_state(&mut r)?;
        r.finish()?;
        Ok(e)
    }

    /// Reads the section [`Elementa::save_state`] wrote, checking everything.
    pub fn restore_state(r: &mut ByteReader<'_>) -> Result<Elementa, ElementaError> {
        let version = r.u32("version")?;
        if version != SECTION_VERSION {
            return Err(ElementaError::new("unsupported_version", format!("elementa section version {version}; this engine reads {SECTION_VERSION}"))
                .with("version", version)
                .with("supported", SECTION_VERSION));
        }
        let n = r.count("materials", MAX_MATERIALS, MATERIAL_BYTES)?;
        let mut materials = Vec::with_capacity(n);
        for _ in 0..n {
            let f = "materials";
            let name = r.str(f, MAX_NAME)?;
            let has_ignite = r.bool(f)?;
            let ignite = r.f32(f)?;
            let mut v = [0.0f32; 5];
            for x in &mut v {
                *x = r.f32(f)?;
            }
            let conducts = r.bool(f)?;
            let has_freeze = r.bool(f)?;
            let freeze = r.f32(f)?;
            let quench = r.f32(f)?;
            let start_heat = r.f32(f)?;
            materials.push(Material {
                name,
                ignite: has_ignite.then_some(ignite),
                burn_s: v[0],
                heat_out: v[1],
                flame: v[2],
                cool: v[3],
                embers: v[4],
                conducts,
                freeze: has_freeze.then_some(freeze),
                quench,
                start_heat,
            });
        }
        let n = r.count("tags", MAX_TAGS, TAG_BYTES)?;
        let mut tags = Vec::with_capacity(n);
        for _ in 0..n {
            let t = r.str("tags", MAX_NAME)?;
            tags.push((t, r.u16("tags")?));
        }
        let cell_m = r.f32("cell_m")?;
        let reach_m = r.f32("reach_m")?;
        let spread_hz = r.u32("spread_hz")?;
        let limits = Limits { nodes: r.u32("limits")?, burning: r.u32("limits")?, work: r.u32("limits")? };
        let config = Config { materials, tags, cell_m, reach_m, spread_hz, limits };
        config.validate()?;

        let tick_hz = r.u32("tick_hz")?;
        let tick = r.u64("tick")?;
        let ember_key = r.u64("ember_key")?;
        let cursor = Cursor { id: r.u64("cursor")?, cell: r.u32("cursor")?, next: r.u64("cursor")? };
        let refused = r.u64("refused")?;
        let wind = r.vec3("wind")?;
        let ambient = r.f32("ambient")?;

        if tick > MAX_TICK {
            return Err(invalid(format!("the save is at tick {tick}; at most {MAX_TICK}")).with("tick", tick).with("max", MAX_TICK));
        }
        let mut e = Elementa::new(config, tick_hz, 0)?;
        e.set_wind(wind)?;
        e.set_ambient(ambient)?;
        e.tick = tick;
        e.ember_key = ember_key;
        e.cursor = cursor;
        e.refused = refused;

        let count = r.count("nodes", e.config.limits.nodes as usize, NODE_BYTES)?;
        let mut nodes: Vec<Node> = Vec::with_capacity(count);
        for i in 0..count {
            let f = "nodes";
            let id = r.u64(f)?;
            let material = r.u16(f)?;
            let center = r.vec3(f)?;
            let half = r.vec3(f)?;
            let heat = r.f32(f)?;
            let fuel = r.f32(f)?;
            let wet = r.f32(f)?;
            let phase = r.u8(f)?;
            let since = r.u64(f)?;
            let ready_at = r.u64(f)?;
            if i > 0 && nodes[i - 1].id >= id {
                return Err(invalid(format!("node ids must rise: {} then {id}", nodes[i - 1].id)).with("id", id));
            }
            let m = e.config.materials.get(usize::from(material)).ok_or_else(|| {
                invalid(format!("node {id} names material {material}; there are {}", e.config.materials.len())).with("id", id)
            })?;
            check_box(&e.config, id, center, half)?;
            let ranged = |name: &str, x: f32, lo: f32, hi: f32| -> Result<(), ElementaError> {
                if !x.is_finite() || x < lo || x > hi {
                    return Err(ElementaError::range(&format!("node {id} {name}"), f64::from(x), f64::from(lo), f64::from(hi)).with("id", id));
                }
                Ok(())
            };
            ranged("heat", heat, -1.0, 1.0)?;
            ranged("fuel", fuel, 0.0, MAX_FUEL_S)?;
            ranged("wet", wet, 0.0, 1.0)?;
            let phase = Phase::from_u8(phase).ok_or_else(|| invalid(format!("node {id} has phase {phase}; there are 4")).with("id", id))?;
            if (phase == Phase::Burning && m.ignite.is_none()) || (phase == Phase::Frozen && m.freeze.is_none()) {
                return Err(invalid(format!("node {id} is {} but its material {} cannot be", phase.name(), m.name)).with("id", id));
            }
            if since > tick {
                return Err(invalid(format!("node {id}'s phase began at tick {since}, after the save's tick {tick}")).with("id", id));
            }
            if ready_at != NOT_READY && (phase != Phase::Intact || ready_at > tick) {
                return Err(invalid(format!("node {id} waits to catch since tick {ready_at} but is {} (save at {tick})", phase.name())).with("id", id));
            }
            nodes.push(Node { id, material, center, half, heat, fuel, wet, phase, since, ready_at });
        }
        let burning = nodes.iter().filter(|n| n.phase == Phase::Burning).count();
        if burning > e.config.limits.burning as usize {
            return Err(invalid(format!("{burning} nodes burn; the limit is {}", e.config.limits.burning)));
        }
        // A query cut part-way names a burning node, and a cell of that node's query.
        if cursor.is_mid() {
            let cut = nodes.binary_search_by_key(&cursor.id, |n| n.id).ok().map(|i| &nodes[i]).filter(|n| n.phase == Phase::Burning);
            let Some(cut) = cut else {
                return Err(invalid(format!("the cursor is part-way through node {}'s query, but no such node burns", cursor.id)).with("id", cursor.id));
            };
            let (lo, hi) = query_box(cut, &e.config);
            let cells = cells_in(lo, hi);
            if u64::from(cursor.cell) >= cells {
                return Err(invalid(format!("the cursor is at cell {} of node {}'s query, which has {cells}", cursor.cell, cursor.id))
                    .with("id", cursor.id)
                    .with("cell", cursor.cell)
                    .with("cells", cells));
            }
        }
        e.nodes = nodes;
        e.mark_all_changed();
        e.report.burning = burning as u32;

        let count = r.count("events", MAX_EVENTS_PER_KIND * EVENT_KINDS.len(), EVENT_BYTES)?;
        let mut events = Events::default();
        for _ in 0..count {
            let f = "events";
            let kind = r.u8(f)?;
            let at_tick = r.u64(f)?;
            let id = r.u64(f)?;
            let at = r.vec3(f)?;
            let by = r.u8(f)?;
            let kind = *EVENT_KINDS.get(usize::from(kind)).ok_or_else(|| invalid(format!("event kind {kind}; there are 5")))?;
            let by = match (kind, by) {
                (EventKind::Extinguished, 1) => Some(Cause::Cold),
                (EventKind::Extinguished, 2) => Some(Cause::Wet),
                (EventKind::Extinguished, _) => return Err(invalid(format!("an extinguished event's cause is {by}"))),
                (_, 0) => None,
                _ => return Err(invalid(format!("only an extinguished event has a cause; a {} has {by}", kind.name()))),
            };
            if at_tick > tick || !(at.x.is_finite() && at.y.is_finite() && at.z.is_finite()) {
                return Err(invalid(format!("event for node {id} at tick {at_tick} (save at {tick}) or at a point not finite")));
            }
            if events.list.iter().filter(|x| x.kind == kind).count() >= MAX_EVENTS_PER_KIND {
                return Err(invalid(format!("more than {MAX_EVENTS_PER_KIND} named {} events", kind.name())));
            }
            events.push(Event { tick: at_tick, kind, id, at, by });
        }
        for u in &mut events.unnamed {
            *u = r.u32("events.unnamed")?;
        }
        e.events = events;
        Ok(e)
    }

    /// A stable hash of the whole state: config, clock, stream, wind, every node, waiting events.
    /// The grid and the last report are derived and left out.
    pub fn hash(&self) -> u64 {
        let mut h = StableHasher::with_seed(0x656c_656d_656e_7461);
        h.write_u64(self.config.hash());
        h.write_u32(self.tick_hz);
        h.write_u64(self.tick);
        h.write_u64(self.ember_key);
        h.write_u64(self.cursor.id);
        h.write_u32(self.cursor.cell);
        h.write_u64(self.cursor.next);
        h.write_u64(self.refused);
        for v in [self.wind.x, self.wind.y, self.wind.z, self.ambient] {
            h.write_f32(v);
        }
        h.write_u64(self.nodes.len() as u64);
        for n in &self.nodes {
            h.write_u64(n.id);
            h.write_u32(u32::from(n.material));
            for v in [n.center.x, n.center.y, n.center.z, n.half.x, n.half.y, n.half.z, n.heat, n.fuel, n.wet] {
                h.write_f32(v);
            }
            h.write_u8(n.phase as u8);
            h.write_u64(n.since);
            h.write_u64(n.ready_at);
        }
        h.write_u64(self.events.list.len() as u64);
        for e in &self.events.list {
            h.write_u8(e.kind as u8);
            h.write_u64(e.tick);
            h.write_u64(e.id);
            for v in [e.at.x, e.at.y, e.at.z] {
                h.write_f32(v);
            }
            h.write_u8(e.by.map_or(0, |b| b as u8));
        }
        for u in self.events.unnamed {
            h.write_u32(u);
        }
        h.finish()
    }
}
