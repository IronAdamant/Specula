//! Fire on the slab: the game's own deterministic fire over its fuel cells, behind a seam.
//!
//! **The seam.** The rules call only [`Blaze::heat`], [`Blaze::cover`], [`Blaze::add_zone`],
//! [`Blaze::is_burning`], [`Blaze::burning`] and [`Blaze::step`], and hear [`BlazeEvent`]s. What
//! is behind them (one heat number, fuel left, a phase and a cover per fuel cell, in integers) is
//! this file's alone, so it can be replaced by the engine's simulation of the elements (Elementa:
//! each fuel cell a node of material `grass`, `heat` an `ignite` act, `cover` a `wet` act, the
//! gust's wind a wind zone, the events Elementa's `ignited` / `burnt` / `extinguished`) without
//! changing a caller. DESIGN.md, "Elemental stones", says what that hand-over needs.
//!
//! **The model** (permille, ticks, millimetres):
//! - A fuel cell is `Fuel` (unburnt), `Burning` or `Burnt` (spent: char, never burns again).
//! - Every `spread_ticks` a spread step: each burning cell gives `cell_heat` (× the wind's lean)
//!   to every fuel cell of its face whose centre is within `cell_reach` of its own; gains are
//!   summed first, then taken in, so no cell's place in the order changes another's result. A
//!   cell at 1000 heat or more catches. Stone (anything not fuel) takes no heat: a break of bare
//!   stone wider than `cell_reach` stops a fire.
//! - A covered cell (frost, earth, wet) takes no heat while covered, and a cover puts a burning
//!   cell out (its fuel kept).
//! - The lean: inside a gust's zone, heat moving along the wind is × (1 + lean), against it
//!   × (1 − lean), across it unchanged; the newest zone over the giving cell counts.
//! - A burning cell burns `cell_burn_ticks` of fuel, one a tick, then is spent.

use crate::content::Content;
use crate::fixed::{isqrt, Pos};
use crate::map::CellId;

/// Gust zones kept at once (the oldest goes first).
pub const MAX_ZONES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Fuel = 0,
    Burning = 1,
    Burnt = 2,
}

/// One fuel cell's fire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FireCell {
    /// Permille of what ignites (0 .. 1000 and over).
    pub heat: i32,
    /// Ticks of burning left.
    pub left: u32,
    pub phase: Phase,
    /// The tick its phase began (for the look).
    pub since: u32,
    /// Takes no heat before this tick (frost, earth, wet).
    pub covered_until: u32,
}

/// A gust's wind over an area: a horizontal direction (permille, x and z), until a tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Zone {
    pub at: Pos,
    pub radius: i32,
    pub dir: (i32, i32),
    pub until: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlazeEvent {
    Caught { cell: CellId },
    Spent { cell: CellId },
    Smothered { cell: CellId },
}

/// The slab's fire: one [`FireCell`] per fuel cell (in [`Content::fuel`]'s order) and the gusts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Blaze {
    pub cells: Vec<FireCell>,
    pub zones: Vec<Zone>,
    /// Gains of the spread step being taken (kept to reuse its room; empty between steps).
    gain: Vec<i32>,
}

/// A unit horizontal direction (permille) from `a` to `b`; (0, 0) when they coincide.
pub fn dir(a: Pos, b: Pos) -> (i32, i32) {
    let (dx, dz) = (i64::from(b.x) - i64::from(a.x), i64::from(b.z) - i64::from(a.z));
    let len = isqrt((dx * dx + dz * dz) as u64) as i64;
    if len == 0 {
        return (0, 0);
    }
    ((dx * 1000 / len) as i32, (dz * 1000 / len) as i32)
}

impl Blaze {
    pub fn new(content: &Content) -> Blaze {
        let left = content.rules.elements.cell_burn_ticks;
        let cell = FireCell { heat: 0, left, phase: Phase::Fuel, since: 0, covered_until: 0 };
        Blaze { cells: vec![cell; content.fuel.len()], zones: Vec::new(), gain: Vec::new() }
    }

    fn index(content: &Content, cell: CellId) -> Option<usize> {
        content.fuel.binary_search(&cell).ok()
    }

    /// Adds `amount` heat (permille) to `cell` if it is fuel, unburnt and uncovered; it catches
    /// at the next spread step if that brings it to 1000.
    pub fn heat(&mut self, content: &Content, cell: CellId, amount: i32, tick: u32) {
        if let Some(i) = Blaze::index(content, cell) {
            let c = &mut self.cells[i];
            if c.phase == Phase::Fuel && tick >= c.covered_until {
                c.heat = c.heat.saturating_add(amount.max(0)).min(2000);
            }
        }
    }

    /// Covers `cell` until `until` (frost, earth or wet): it takes no heat, and if it burns it
    /// goes out (its fuel kept, its heat gone).
    pub fn cover(&mut self, content: &Content, cell: CellId, until: u32, tick: u32, out: &mut Vec<BlazeEvent>) {
        if let Some(i) = Blaze::index(content, cell) {
            let c = &mut self.cells[i];
            c.covered_until = c.covered_until.max(until);
            c.heat = 0;
            if c.phase == Phase::Burning {
                c.phase = Phase::Fuel;
                c.since = tick;
                out.push(BlazeEvent::Smothered { cell });
            }
        }
    }

    pub fn add_zone(&mut self, z: Zone) {
        if self.zones.len() >= MAX_ZONES {
            self.zones.remove(0);
        }
        self.zones.push(z);
    }

    pub fn is_burning(&self, content: &Content, cell: CellId) -> bool {
        Blaze::index(content, cell).is_some_and(|i| self.cells[i].phase == Phase::Burning)
    }

    /// The cell's fire, if it is fuel.
    pub fn cell(&self, content: &Content, cell: CellId) -> Option<&FireCell> {
        Blaze::index(content, cell).map(|i| &self.cells[i])
    }

    /// The burning cells, in cell order.
    pub fn burning<'a>(&'a self, content: &'a Content) -> impl Iterator<Item = CellId> + 'a {
        self.cells.iter().zip(&content.fuel).filter(|(c, _)| c.phase == Phase::Burning).map(|(_, id)| *id)
    }

    /// The wind's lean on heat going from `from` to `to` (permille, 1000 = none).
    fn lean(&self, content: &Content, from: Pos, to: Pos, tick: u32) -> i64 {
        let lean = i64::from(content.rules.elements.lean);
        let Some(z) = self.zones.iter().rev().find(|z| tick < z.until && from.dist2(z.at) <= i64::from(z.radius) * i64::from(z.radius)) else {
            return 1000;
        };
        let (dx, dz) = dir(from, to);
        let along = (i64::from(z.dir.0) * i64::from(dx) + i64::from(z.dir.1) * i64::from(dz)) / 1000;
        1000 + lean * along / 1000
    }

    /// One tick: burning cells burn fuel down; on a spread step, heat moves and cells catch.
    pub fn step(&mut self, content: &Content, tick: u32, out: &mut Vec<BlazeEvent>) {
        if self.cells.is_empty() {
            return;
        }
        let el = &content.rules.elements;
        for (c, id) in self.cells.iter_mut().zip(&content.fuel) {
            if c.phase == Phase::Burning {
                c.left = c.left.saturating_sub(1);
                if c.left == 0 {
                    c.phase = Phase::Burnt;
                    c.since = tick;
                    c.heat = 0;
                    out.push(BlazeEvent::Spent { cell: *id });
                }
            }
        }
        self.zones.retain(|z| tick < z.until);
        if !tick.is_multiple_of(el.spread_ticks.max(1)) {
            return;
        }
        let map = &content.map;
        let r2 = i64::from(el.cell_reach) * i64::from(el.cell_reach);
        let mut gain = std::mem::take(&mut self.gain);
        gain.clear();
        gain.resize(self.cells.len(), 0);
        for (i, c) in self.cells.iter().enumerate() {
            if c.phase != Phase::Burning {
                continue;
            }
            let from = map.cell(content.fuel[i]);
            for (j, d) in self.cells.iter().enumerate() {
                if j == i || d.phase != Phase::Fuel || tick < d.covered_until {
                    continue;
                }
                let to = map.cell(content.fuel[j]);
                if to.face != from.face || from.pos.dist2(to.pos) > r2 {
                    continue;
                }
                let g = i64::from(el.cell_heat) * self.lean(content, from.pos, to.pos, tick) / 1000;
                gain[j] = gain[j].saturating_add(g.clamp(0, 2000) as i32);
            }
        }
        for (k, c) in self.cells.iter_mut().enumerate() {
            if c.phase == Phase::Fuel {
                c.heat = c.heat.saturating_add(gain[k]).min(2000);
                if c.heat >= 1000 && c.left > 0 {
                    c.phase = Phase::Burning;
                    c.since = tick;
                    out.push(BlazeEvent::Caught { cell: content.fuel[k] });
                }
            }
        }
        self.gain = gain;
    }

    /// Hashes the fire in a fixed order.
    pub fn hash_into(&self, h: &mut specula_core::hash::StableHasher) {
        h.write_u64(self.cells.len() as u64);
        for c in &self.cells {
            h.write_i64(i64::from(c.heat));
            h.write_u32(c.left);
            h.write_u8(c.phase as u8);
            h.write_u32(c.since);
            h.write_u32(c.covered_until);
        }
        h.write_u64(self.zones.len() as u64);
        for z in &self.zones {
            for v in [z.at.x, z.at.y, z.at.z, z.radius, z.dir.0, z.dir.1] {
                h.write_i64(i64::from(v));
            }
            h.write_u32(z.until);
        }
    }
}
