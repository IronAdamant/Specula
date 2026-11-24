//! The elements in the rules (DESIGN.md, "Elemental stones"): fire on the dead and on the
//! slab's fuel, lightning's chain, frost's chill and freeze, the wind's shove and recharge, the
//! earth's spikes and rubble, and the exploding dead. Integers, ordered containers and id order,
//! as the rest of the tick; every part is inert on content without elements (the shipped maps),
//! so their runs, hashes and saves are as they were.
//!
//! **Two seams** keep what the engine may later own behind one door each:
//! - the shove ([`World::shove_back`]): how far back along its path a pushed enemy goes. Today
//!   the rules move it back along the lane graph, millimetre for millimetre; a physics body
//!   (a character controller taking the push as a velocity) can take this one function's place.
//! - the slab's fire ([`crate::blaze::Blaze`]): heat, fuel, cover, burning out, the wind's lean;
//!   Elementa's nodes can take its place.

use super::*;
use crate::blaze::{dir, BlazeEvent, Zone};
use crate::content::TierEl;

/// What the elements did to an enemy. All zero on the shipped maps. Ticks are absolute
/// (`0`: not set), so a save holds them as they are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EnemyEl {
    /// Milli-HP per tick while `tick < burn_until`.
    pub burn: i32,
    pub burn_until: u32,
    /// Permille; at 1000 it freezes. Fades by `chill_fade` a tick.
    pub chill: i32,
    /// Frozen solid while `tick < frozen_until` (it stands, breaks no wall, cannot burst).
    pub frozen_until: u32,
    /// The bonus (permille) of the hit that shatters this freeze.
    pub shatter: i32,
    /// Wet while `tick < wet_until` (after a thaw): it does not catch fire, and lightning hurts it more.
    pub wet_until: u32,
    /// mm still to be shoved back along its path, at `shove_rate` mm a tick.
    pub shove: i32,
    pub shove_rate: i32,
    /// Shoves taken so far (each later one is weaker: `elements.shove_fade`).
    pub shoves: u16,
    /// Staggered by earth while `tick < stagger_until`: it stands, and it grounds a lightning chain.
    pub stagger_until: u32,
    /// An exploding enemy alight: it bursts at this tick (0: no fuse burning).
    pub fuse_at: u32,
}

impl EnemyEl {
    pub fn burning(&self, tick: u32) -> bool {
        self.burn > 0 && tick < self.burn_until
    }
    pub fn frozen(&self, tick: u32) -> bool {
        tick < self.frozen_until
    }
    pub fn wet(&self, tick: u32) -> bool {
        tick < self.wet_until
    }
    pub fn staggered(&self, tick: u32) -> bool {
        tick < self.stagger_until
    }
}

/// The most shoves one enemy takes: after this many, no gust or blast moves it again.
pub const MAX_SHOVES: u16 = u16::MAX;

/// In the air between two cells of a leap edge (a climber's jump): no shove reaches it.
fn mid_leap(e: &Enemy) -> bool {
    matches!(e.motion, Motion::Ground { progress, len, leap: true, .. } if len > 0 && progress > 0 && progress < len)
}

fn blaze_event(e: BlazeEvent) -> Event {
    match e {
        BlazeEvent::Caught { cell } => Event::CellCaught { cell },
        BlazeEvent::Spent { cell } => Event::CellSpent { cell },
        BlazeEvent::Smothered { cell } => Event::CellSmothered { cell },
    }
}

impl World {
    /// Frozen or staggered, the enemy stands this tick; shoved, it goes back. True when the
    /// elements took its move.
    pub(super) fn held_by_elements(&self, content: &Content, e: &mut Enemy, def: &EnemyDef) -> bool {
        let tick = self.tick;
        if e.el.frozen(tick) || e.el.staggered(tick) {
            e.carry = 0;
            return true;
        }
        // A leaper in mid-air is not caught by a gust: it lands, and the shove waits for it
        // there (it is neither spent in the air nor lost).
        if e.el.shove > 0 && !mid_leap(e) {
            let d = e.el.shove_rate.max(1).min(e.el.shove);
            e.el.shove -= d;
            if e.el.shove == 0 {
                e.el.shove_rate = 0;
            }
            self.shove_back(content, e, def, d);
            return true;
        }
        false
    }

    /// **The push seam.** Moves `e` `d` mm back along its path: back along the edge it walks,
    /// then onto the cell it most likely came from (a neighbour upstream of it on the flow
    /// field, on a shortest route through it, ties to the lowest id), and so on. It never enters
    /// a wall, never leaves the lanes, is not pushed in mid-leap, and stops pinned where nothing
    /// is upstream (a spawn).
    pub(super) fn shove_back(&self, content: &Content, e: &mut Enemy, def: &EnemyDef, d: i32) {
        let map = &content.map;
        let class = class_of(def);
        let (open, breach) = (self.fields.open(class), self.fields.breach(class));
        let mut d = d;
        for _ in 0..8 {
            if mid_leap(e) {
                return;
            }
            let Motion::Ground { from, to, progress, len, leap } = e.motion else { return };
            if progress >= d {
                e.motion = Motion::Ground { from, to, progress: progress - d, len, leap };
                return;
            }
            d -= progress;
            let at = from;
            let field = if open[at.index()] != UNREACHABLE { open } else { breach };
            let here = field[at.index()];
            let mut best: Option<(i64, CellId, i32)> = None;
            for l in map.links(at) {
                let n = l.to;
                let fd = field[n.index()];
                if !map.cell(n).kind.walkable(class.climbs()) || self.blocked[n.index()] || fd == UNREACHABLE || fd <= here {
                    continue;
                }
                let Some(back) = map.links(n).iter().find(|b| b.to == at) else { continue };
                let off = (i64::from(fd) - i64::from(here) - i64::from(back.cost)).abs();
                if best.is_none_or(|(k, bn, _)| (off, n) < (k, bn)) {
                    best = Some((off, n, back.cost));
                }
            }
            let Some((_, n, cost)) = best else {
                e.motion = Motion::Ground { from: at, to: at, progress: 0, len: 0, leap: true };
                return;
            };
            e.motion = Motion::Ground { from: n, to: at, progress: cost, len: cost, leap: false };
        }
    }

    /// Gives enemy `i` a shove of `push` mm (mass 1) over `ticks`. The n-th shove an enemy
    /// takes is divided by 1 + n x `shove_fade`, and one shoved [`MAX_SHOVES`] times is not
    /// moved again: no wind holds one of the dead for ever.
    fn shove(&mut self, content: &Content, i: usize, push: i32, ticks: u32) {
        let e = &mut self.enemies[i];
        if e.el.shoves == MAX_SHOVES {
            return;
        }
        let mass = content.enemy(e.kind).mass.max(1);
        let fade = 1000 + i64::from(content.rules.elements.shove_fade) * i64::from(e.el.shoves);
        let s = (i64::from(push) * 1000 / i64::from(mass) * 1000 / fade) as i32;
        e.el.shoves = e.el.shoves.saturating_add(1);
        if s <= 0 {
            return;
        }
        e.el.shove = e.el.shove.max(s);
        e.el.shove_rate = (e.el.shove + ticks.max(1) as i32 - 1) / ticks.max(1) as i32;
    }

    /// The hit that shatters a frozen enemy: the freeze ends, the hit gains its bonus.
    pub(super) fn shatter(&mut self, i: usize, dmg: i32, ev: &mut Vec<Event>) -> i32 {
        let e = &mut self.enemies[i];
        let total = (i64::from(dmg) * (1000 + i64::from(e.el.shatter)) / 1000).min(i64::from(i32::MAX)) as i32;
        e.el.frozen_until = 0;
        e.el.shatter = 0;
        ev.push(Event::Shattered { enemy: e.id, damage: total });
        total
    }

    /// Sets enemy `i` alight (fire on a frozen enemy thaws it instead; the wet do not catch).
    fn ignite(&mut self, content: &Content, i: usize, burn: i32, ticks: u32, ev: &mut Vec<Event>) {
        let tick = self.tick;
        let wet = content.rules.elements.wet_ticks;
        let e = &mut self.enemies[i];
        let def = content.enemy(e.kind);
        if !e.alive || def.flies() || burn <= 0 {
            return;
        }
        if e.el.frozen(tick) {
            e.el.frozen_until = 0;
            e.el.chill = 0;
            e.el.wet_until = tick.saturating_add(wet);
            ev.push(Event::Thawed { enemy: e.id, steam: true });
            return;
        }
        if e.el.wet(tick) {
            return;
        }
        if !e.el.burning(tick) {
            ev.push(Event::Ignited { enemy: e.id });
            if let Some(b) = def.burst {
                if e.el.fuse_at == 0 {
                    e.el.fuse_at = tick.saturating_add(b.fuse.max(1));
                }
            }
            e.el.burn = 0;
        }
        e.el.burn = e.el.burn.max(burn);
        e.el.burn_until = e.el.burn_until.max(tick.saturating_add(ticks));
    }

    fn extinguish(&mut self, i: usize, ev: &mut Vec<Event>) {
        let tick = self.tick;
        let e = &mut self.enemies[i];
        if e.el.burning(tick) {
            ev.push(Event::Extinguished { enemy: e.id });
        }
        e.el.burn = 0;
        e.el.burn_until = 0;
        e.el.fuse_at = 0;
    }

    /// Frost on enemy `i`: its fire goes out, its chill rises (÷ its mass), and at 1000 it freezes.
    fn chill(&mut self, content: &Content, i: usize, el: &TierEl, ev: &mut Vec<Event>) {
        let tick = self.tick;
        if !self.enemies[i].alive || self.enemies[i].el.frozen(tick) {
            return;
        }
        if self.enemies[i].el.burning(tick) {
            self.extinguish(i, ev);
        }
        let e = &mut self.enemies[i];
        let mass = content.enemy(e.kind).mass.max(1);
        e.el.chill = e.el.chill.saturating_add((i64::from(el.chill) * 1000 / i64::from(mass)) as i32);
        if e.el.chill >= 1000 {
            e.el.chill = 0;
            e.el.frozen_until = tick.saturating_add(el.freeze_ticks);
            e.el.shatter = el.shatter;
            e.el.fuse_at = 0;
            ev.push(Event::Frozen { enemy: e.id });
        }
    }

    /// A projectile's elemental part, after its damage: fire catches, frost chills and lays
    /// rime on the cell (fire does not cross it while it lasts).
    pub(super) fn hit_elements(&mut self, content: &Content, i: usize, el: &TierEl, ev: &mut Vec<Event>) {
        if !self.enemies[i].alive {
            return;
        }
        if el.burn > 0 {
            self.ignite(content, i, el.burn, el.burn_ticks, ev);
        }
        if el.chill > 0 {
            self.chill(content, i, el, ev);
            if let Some(cell) = self.enemies[i].cell() {
                let until = self.tick.saturating_add(content.rules.elements.wet_ticks);
                self.cover(content, cell, until, ev);
            }
        }
    }

    fn cover(&mut self, content: &Content, cell: CellId, until: u32, ev: &mut Vec<Event>) {
        let mut out = Vec::new();
        self.blaze.cover(content, cell, until, self.tick, &mut out);
        ev.extend(out.into_iter().map(blaze_event));
    }

    /// A fire lob landed: the fuel under its splash takes a full heat (it catches at the next
    /// spread step unless covered).
    pub(super) fn splash_fire(&mut self, content: &Content, at: Pos, radius: i32) {
        let r2 = i64::from(radius) * i64::from(radius);
        for &f in &content.fuel {
            if content.map.cell(f).pos.dist2(at) <= r2 {
                self.blaze.heat(content, f, 1000, self.tick);
            }
        }
    }

    /// Lightning: the target, then up to `chain` jumps to the nearest others not yet struck
    /// within `chain_m` of the last, each keeping `chain_keep` of the damage; the wet take
    /// `wet_bonus` more; a staggered (grounded) enemy ends the chain; an exploder struck bursts.
    pub(super) fn strike(&mut self, content: &Content, cell: CellId, c: &Cell, def: &StoneDef, t: &Tier, ev: &mut Vec<Event>) -> bool {
        let Some(first) = self.pick_target(content, c, def, t) else { return false };
        let (el, tick) = (t.el, self.tick);
        let r2 = i64::from(el.chain_range) * i64::from(el.chain_range);
        let mut struck: Vec<usize> = Vec::with_capacity(usize::from(el.chain) + 1);
        let (mut idx, mut from, mut dmg) = (first, c.pos.up(MUZZLE), t.damage);
        for link in 0..=el.chain {
            let e = self.enemies[idx];
            let d = if e.el.wet(tick) { (i64::from(dmg) * (1000 + i64::from(el.wet_bonus)) / 1000).min(i64::from(i32::MAX)) as i32 } else { dmg };
            ev.push(Event::Struck { stone: cell, enemy: e.id, from, damage: d, link });
            struck.push(idx);
            // Frozen when the strike lands, it cannot burst: the hit shatters the ice (and, if
            // it dies of it, it dies whole), so whether it was frozen is read before the hurt.
            let was_frozen = e.el.frozen(tick);
            self.hurt(content, idx, d, ev);
            if self.enemies[idx].alive && content.enemy(e.kind).burst.is_some() && !was_frozen {
                self.kill(content, idx, true, ev);
            }
            dmg = (i64::from(dmg) * i64::from(el.chain_keep) / 1000) as i32;
            if e.el.staggered(tick) || link == el.chain || dmg <= 0 {
                break;
            }
            from = e.pos;
            let mut best: Option<(i64, usize)> = None;
            for (j, o) in self.enemies.iter().enumerate() {
                if !o.alive || struck.contains(&j) || !def.targets.hits(content.enemy(o.kind).flies()) {
                    continue;
                }
                let d2 = o.pos.dist2(from);
                if d2 <= r2 && best.is_none_or(|(b, _)| d2 < b) {
                    best = Some((d2, j));
                }
            }
            let Some((_, j)) = best else { break };
            idx = j;
        }
        true
    }

    /// Wind: no damage. When any enemy is in range (one alone too: the owner's words are that
    /// the enemy "gets pushed back in their lane"), every one in range is shoved back along its
    /// path (÷ its mass), a burning one is fanned (its fire renewed), the wind lingers over the
    /// area and leans the slab's fire, and the stone's charge is spent.
    pub(super) fn gust(&mut self, content: &Content, cell: CellId, c: &Cell, def: &StoneDef, t: &Tier, ev: &mut Vec<Event>) -> bool {
        let el = t.el;
        let idx: Vec<usize> = (0..self.enemies.len()).filter(|&i| self.eligible(content, c, def, t, &self.enemies[i])).collect();
        if idx.is_empty() {
            return false;
        }
        let Some(first) = self.pick_target(content, c, def, t) else { return false };
        let tick = self.tick;
        let els = content.rules.elements;
        let wind = dir(c.pos, self.enemies[first].pos);
        for &i in &idx {
            self.shove(content, i, el.push, el.push_ticks);
            let e = &mut self.enemies[i];
            if e.el.burning(tick) {
                e.el.burn_until = e.el.burn_until.max(tick.saturating_add(els.catch_ticks));
            }
        }
        self.blaze.add_zone(Zone { at: c.pos, radius: t.range, dir: wind, until: tick.saturating_add(els.gust_ticks) });
        ev.push(Event::Gusted { stone: cell, affected: idx.len().min(usize::from(u16::MAX)) as u16, dir: wind });
        true
    }

    /// Earth: spikes erupt along `line` lane cells from the target's cell down its route; every
    /// ground enemy on them is hurt and staggered and its fire smothered, the fuel on them is
    /// covered, and (from tier II) rubble rises on the farthest of them a wall can stand on.
    pub(super) fn heave(&mut self, content: &Content, cell: CellId, c: &Cell, def: &StoneDef, t: &Tier, ev: &mut Vec<Event>) -> bool {
        let Some(first) = self.pick_target(content, c, def, t) else { return false };
        let target = self.enemies[first];
        let Some(start) = target.cell() else { return false };
        let (el, tick, map) = (t.el, self.tick, &content.map);
        let class = class_of(content.enemy(target.kind));
        let mut line = vec![start];
        let mut at = start;
        while line.len() < usize::from(el.line) {
            match self.fields.route(map, class, false, &self.blocked, at) {
                Route::Step(n) if !line.contains(&n) && map.cell(n).kind != CellKind::Graveyard => {
                    line.push(n);
                    at = n;
                }
                _ => break,
            }
        }
        let mut n = 0u16;
        for i in 0..self.enemies.len() {
            let o = self.enemies[i];
            if !o.alive || content.enemy(o.kind).flies() || !line.iter().any(|&l| o.touches(l)) {
                continue;
            }
            n = n.saturating_add(1);
            self.extinguish(i, ev);
            self.enemies[i].el.stagger_until = self.enemies[i].el.stagger_until.max(tick.saturating_add(el.stagger_ticks));
            self.hurt(content, i, t.damage, ev);
        }
        let until = tick.saturating_add(content.rules.elements.wet_ticks);
        for &l in &line {
            self.cover(content, l, until, ev);
        }
        if el.rubble_hp > 0 {
            let spot = line.iter().rev().copied().find(|&l| {
                map.cell(l).kind.wallable() && !self.walls.contains_key(&l) && !self.enemies.iter().any(|e| e.alive && e.touches(l))
            });
            if let Some(l) = spot {
                self.walls.insert(l, Wall { hp: el.rubble_hp, max_hp: el.rubble_hp, cost: 0 });
                self.rubble.insert(l, tick.saturating_add(el.rubble_ticks));
                self.set_blocked(content, l, true);
                ev.push(Event::RubbleRaised { cell: l });
            }
        }
        ev.push(Event::Heaved { stone: cell, at: start, cells: line.len() as u8, affected: n });
        true
    }

    /// An exploding sapper reached a wall: it bursts (no dust: it was not killed).
    pub(super) fn burst_on_wall(&mut self, e: &mut Enemy) {
        e.alive = false;
        let s = &mut self.wave_stats[usize::from(e.wave) - 1];
        s.alive -= 1;
        s.detonated += 1;
        self.bursts.push(e.id);
    }

    /// Every burst queued this tick, in order (a blast that kills another exploder queues it):
    /// walls and rubble within the radius are destroyed (no refund) and the fields rebuilt once,
    /// ground enemies in it are hurt and shoved, and the fuel in it takes a full heat.
    pub(super) fn run_bursts(&mut self, content: &Content, ev: &mut Vec<Event>) {
        let mut k = 0;
        while k < self.bursts.len() {
            let id = self.bursts[k];
            k += 1;
            let Some(i) = self.enemy_index(id) else { continue };
            let e = self.enemies[i];
            let Some(b) = content.enemy(e.kind).burst else { continue };
            let r2 = i64::from(b.radius) * i64::from(b.radius);
            let gone: Vec<CellId> = self.walls.keys().copied().filter(|c| content.map.cell(*c).pos.dist2(e.pos) <= r2).collect();
            let hit: Vec<usize> = (0..self.enemies.len())
                .filter(|&j| {
                    let o = &self.enemies[j];
                    o.alive && !content.enemy(o.kind).flies() && o.pos.dist2(e.pos) <= r2
                })
                .collect();
            ev.push(Event::Detonated { enemy: id, pos: e.pos, walls: gone.len() as u16, hit: hit.len().min(usize::from(u16::MAX)) as u16 });
            for c in &gone {
                self.walls.remove(c);
                self.rubble.remove(c);
                self.blocked[c.index()] = false;
                ev.push(Event::WallBroken { cell: *c });
            }
            if !gone.is_empty() {
                self.fields = Fields::build(&content.map, &self.blocked, content.rules.wall.breach_penalty);
            }
            for j in hit {
                self.hurt(content, j, b.damage, ev);
                if self.enemies[j].alive && b.push > 0 {
                    self.shove(content, j, b.push, 6);
                }
            }
            for &f in &content.fuel {
                if content.map.cell(f).pos.dist2(e.pos) <= r2 {
                    self.blaze.heat(content, f, 1000, self.tick);
                }
            }
        }
        self.bursts.clear();
    }

    /// The elements' part of the tick, after the stones and projectiles: burning hurts, chill
    /// fades, freezes thaw (wet after), fuses burst, rubble crumbles, and on a spread step fire
    /// moves between the dead and the fuel; then the slab's fire steps.
    pub(super) fn elements_tick(&mut self, content: &Content, ev: &mut Vec<Event>) {
        let tick = self.tick;
        let els = content.rules.elements;
        for i in 0..self.enemies.len() {
            let e = &mut self.enemies[i];
            if !e.alive || e.el == EnemyEl::default() {
                continue;
            }
            if e.el.chill > 0 {
                e.el.chill = (e.el.chill - els.chill_fade).max(0);
            }
            if e.el.frozen_until != 0 && tick >= e.el.frozen_until {
                e.el.frozen_until = 0;
                e.el.shatter = 0;
                e.el.wet_until = tick.saturating_add(els.wet_ticks);
                ev.push(Event::Thawed { enemy: e.id, steam: false });
            }
            if e.el.burn != 0 {
                if tick >= e.el.burn_until {
                    // Burnt out: the fire is gone, and a fuse with it.
                    e.el.burn = 0;
                    e.el.burn_until = 0;
                    e.el.fuse_at = 0;
                } else {
                    let b = e.el.burn;
                    self.hurt(content, i, b, ev);
                }
            }
            let e = &mut self.enemies[i];
            if e.alive && e.el.fuse_at != 0 && e.el.frozen(tick) {
                // Ice puts a fuse out (a freeze clears it; this holds it whatever set it).
                e.el.fuse_at = 0;
            }
            if e.alive && e.el.fuse_at != 0 && tick >= e.el.fuse_at {
                e.el.fuse_at = 0;
                self.kill(content, i, true, ev);
            }
        }
        let crumbled: Vec<CellId> = self.rubble.iter().filter(|(_, &t)| tick >= t).map(|(c, _)| *c).collect();
        for c in crumbled {
            self.rubble.remove(&c);
            if self.walls.remove(&c).is_some() {
                ev.push(Event::WallCrumbled { cell: c });
                self.set_blocked(content, c, false);
            }
        }
        if els.on && tick.is_multiple_of(els.spread_ticks.max(1)) {
            self.spread_on_the_dead(content, ev);
        }
        let mut out = Vec::new();
        self.blaze.step(content, tick, &mut out);
        ev.extend(out.into_iter().map(blaze_event));
    }

    /// A spread step's part for the dead: a burning enemy heats the fuel near it, and an enemy
    /// near a burning one (`mob_reach`) or a burning cell (`cell_reach`) catches. Decided from
    /// the state before the step (collected first, then applied), so order changes nothing.
    fn spread_on_the_dead(&mut self, content: &Content, ev: &mut Vec<Event>) {
        let (tick, els, map) = (self.tick, content.rules.elements, &content.map);
        let burning: Vec<(usize, Pos)> = self.enemies.iter().enumerate().filter(|(_, e)| e.alive && e.el.burning(tick)).map(|(i, e)| (i, e.pos)).collect();
        let cells: Vec<Pos> = self.blaze.burning(content).map(|c| map.cell(c).pos).collect();
        if burning.is_empty() && cells.is_empty() {
            return;
        }
        let rc = i64::from(els.cell_reach) * i64::from(els.cell_reach);
        let rm = i64::from(els.mob_reach) * i64::from(els.mob_reach);
        for &(_, p) in &burning {
            for &f in &content.fuel {
                if map.cell(f).pos.dist2(p) <= rc {
                    self.blaze.heat(content, f, els.cell_heat, tick);
                }
            }
        }
        let catch: Vec<usize> = (0..self.enemies.len())
            .filter(|&j| {
                let o = &self.enemies[j];
                o.alive
                    && !o.el.burning(tick)
                    && !content.enemy(o.kind).flies()
                    && (burning.iter().any(|&(k, p)| k != j && p.dist2(o.pos) <= rm) || cells.iter().any(|p| p.dist2(o.pos) <= rc))
            })
            .collect();
        for j in catch {
            self.ignite(content, j, els.cell_burn, els.catch_ticks, ev);
        }
    }

    pub(super) fn hash_elements(&self, h: &mut StableHasher) {
        h.write_str("elements");
        for e in &self.enemies {
            let x = &e.el;
            for v in [x.burn, x.chill, x.shatter, x.shove, x.shove_rate] {
                h.write_i64(i64::from(v));
            }
            for v in [x.burn_until, x.frozen_until, x.wet_until, x.stagger_until, x.fuse_at] {
                h.write_u32(v);
            }
            h.write_u32(u32::from(x.shoves));
        }
        h.write_u64(self.rubble.len() as u64);
        for (c, t) in &self.rubble {
            h.write_u32(u32::from(c.0));
            h.write_u32(*t);
        }
        for s in &self.wave_stats {
            h.write_u32(s.detonated);
        }
        self.blaze.hash_into(h);
    }
}

#[cfg(test)]
#[path = "sim_elements_tests.rs"]
mod tests;
