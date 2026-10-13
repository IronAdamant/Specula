//! Ground pathing: Dijkstra flow fields toward the graveyard.
//!
//! Per movement class (walkers, climbers) there are three fields:
//!
//! - **open**: walls block. An enemy on a cell with a finite open distance has an open route and
//!   always takes it, however long (owner rule).
//! - **open with leaps**: the open field plus the cracked ledges between jump points, for
//!   lane-jumpers. A leap counts as an open route: a runner whose only way out of a walled lane
//!   is a ledge walks to it and leaps, instead of breaking a wall.
//! - **breach**: walls are passable at their length plus a per-wall penalty. Only used where no
//!   open route exists: the enemy follows it to the first wall on the cheapest route and breaks
//!   it **(proposal)**. The penalty makes "break one wall" beat "break two" on similar routes.
//!
//! Fields are recomputed only when walls change. Ties go to the lowest cell id, and the heap
//! orders `(distance, cell)` completely, so the result never depends on insertion history.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::map::{CellId, Map};

pub const UNREACHABLE: i32 = i32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    Walker = 0,
    Climber = 1,
}

impl Class {
    pub fn climbs(self) -> bool {
        self == Class::Climber
    }
}

/// The distance fields (mm to the nearest graveyard cell; [`UNREACHABLE`] if none), indexed
/// by [`Class`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fields {
    pub open: [Vec<i32>; 2],
    pub open_leaps: [Vec<i32>; 2],
    pub breach: [Vec<i32>; 2],
}

/// Where a ground enemy standing on a cell goes next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Step(CellId),
    /// Leap the cracked ledge to this jump point: the only open route (lane-jumpers only).
    Leap(CellId),
    /// No open route: break this wall (adjacent to the enemy).
    Attack(CellId),
    /// Cut off even with walls broken (only on malformed maps).
    Stuck,
}

impl Fields {
    pub fn build(map: &Map, walls: &[bool], breach_penalty: i32) -> Fields {
        let f = |class: Class, leaps: bool, penalty: Option<i32>| field(map, class, leaps, walls, penalty);
        Fields {
            open: [f(Class::Walker, false, None), f(Class::Climber, false, None)],
            open_leaps: [f(Class::Walker, true, None), f(Class::Climber, true, None)],
            breach: [f(Class::Walker, false, Some(breach_penalty)), f(Class::Climber, false, Some(breach_penalty))],
        }
    }

    pub fn open(&self, class: Class) -> &[i32] {
        &self.open[class as usize]
    }

    pub fn open_leaps(&self, class: Class) -> &[i32] {
        &self.open_leaps[class as usize]
    }

    pub fn breach(&self, class: Class) -> &[i32] {
        &self.breach[class as usize]
    }

    /// The distance an enemy at `cell` still has to go: its open route (with leaps for
    /// jumpers) if it has one, else the breach route.
    pub fn remaining(&self, class: Class, leaps: bool, cell: CellId) -> i32 {
        let i = cell.index();
        let o = self.open(class)[i];
        if o != UNREACHABLE {
            return o;
        }
        if leaps && self.open_leaps(class)[i] != UNREACHABLE {
            return self.open_leaps(class)[i];
        }
        self.breach(class)[i]
    }

    /// The next move from `at`, in order of preference:
    /// 1. downhill on the open field (walking only);
    /// 2. for lane-jumpers, downhill on the open-with-leaps field, leaping where that is the way;
    /// 3. downhill on the breach field, attacking a wall when the next cell holds one.
    pub fn route(&self, map: &Map, class: Class, leaps: bool, walls: &[bool], at: CellId) -> Route {
        let open_only = |c: CellId| if walls[c.index()] { None } else { Some(0) };
        if self.open(class)[at.index()] != UNREACHABLE {
            if let Some((n, _)) = downhill(map, class, self.open(class), false, at, open_only) {
                return Route::Step(n);
            }
        }
        if leaps && self.open_leaps(class)[at.index()] != UNREACHABLE {
            if let Some((n, leap)) = downhill(map, class, self.open_leaps(class), true, at, open_only) {
                return if leap { Route::Leap(n) } else { Route::Step(n) };
            }
        }
        if self.breach(class)[at.index()] != UNREACHABLE {
            // The breach field already counts each wall's penalty in the wall cell's distance.
            if let Some((n, _)) = downhill(map, class, self.breach(class), false, at, |_| Some(0)) {
                return if walls[n.index()] { Route::Attack(n) } else { Route::Step(n) };
            }
        }
        Route::Stuck
    }
}

/// The neighbour (or, with `leaps`, ledge partner) minimising `dist[n] + cost + extra(n)`,
/// ties to the lowest id. Returns it and whether it is a leap.
fn downhill(map: &Map, class: Class, dist: &[i32], leaps: bool, at: CellId, extra: impl Fn(CellId) -> Option<i32>) -> Option<(CellId, bool)> {
    let mut best: Option<(i64, CellId, bool)> = None;
    let mut consider = |n: CellId, cost: i32, leap: bool| {
        if !map.cell(n).kind.walkable(class.climbs()) || dist[n.index()] == UNREACHABLE {
            return;
        }
        let Some(x) = extra(n) else { return };
        let v = i64::from(dist[n.index()]) + i64::from(cost) + i64::from(x);
        if best.is_none_or(|(b, bn, _)| (v, n) < (b, bn)) {
            best = Some((v, n, leap));
        }
    };
    for l in map.links(at) {
        consider(l.to, l.cost, false);
    }
    if leaps {
        for &p in map.jump_partners(at) {
            consider(p, map.jump_cost(at, p), true);
        }
    }
    best.map(|(_, c, leap)| (c, leap))
}

fn field(map: &Map, class: Class, leaps: bool, walls: &[bool], penalty: Option<i32>) -> Vec<i32> {
    let mut dist = vec![UNREACHABLE; map.cells().len()];
    let mut heap = BinaryHeap::new();
    for &g in &map.graveyard {
        dist[g.index()] = 0;
        heap.push(Reverse((0i32, g)));
    }
    while let Some(Reverse((d, c))) = heap.pop() {
        if d > dist[c.index()] {
            continue;
        }
        let jumps: &[CellId] = if leaps { map.jump_partners(c) } else { &[] };
        let edges = map.links(c).iter().map(|l| (l.to, l.cost)).chain(jumps.iter().map(|&p| (p, map.jump_cost(c, p))));
        for (n, cost) in edges {
            if !map.cell(n).kind.walkable(class.climbs()) {
                continue;
            }
            let extra = match (walls[n.index()], penalty) {
                (false, _) => 0,
                (true, None) => continue,
                (true, Some(p)) => p,
            };
            let nd = d.saturating_add(cost).saturating_add(extra);
            if nd < dist[n.index()] {
                dist[n.index()] = nd;
                heap.push(Reverse((nd, n)));
            }
        }
    }
    dist
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::SLAB_01_JSON;

    fn setup() -> (Map, Vec<bool>) {
        let m = Map::parse(SLAB_01_JSON, "slab_01.json").unwrap();
        let walls = vec![false; m.cells().len()];
        (m, walls)
    }

    #[test]
    fn distances_follow_the_lanes() {
        let (m, walls) = setup();
        let f = Fields::build(&m, &walls, 8_000);
        let r = |s: &str| m.cell_ref(s).unwrap().index();
        assert_eq!(f.open(Class::Walker)[r("top:9,9")], 0);
        // gorge_mid walks straight up the central lane: 8 cells of 2 m.
        assert_eq!(f.open(Class::Walker)[r("top:9,19")], 16_000);
        // Walkers can't climb; climbers can (6 rows of 2.5 m + the rim step + the top route).
        assert_eq!(f.open(Class::Walker)[r("north:4,5")], UNREACHABLE);
        assert!(f.open(Class::Climber)[r("north:4,5")] > 15_000);
        assert_eq!(f.open(Class::Walker), f.breach(Class::Walker), "no walls: breach == open");
        // Leaping never makes a route longer; from top:4,13 the ledge to the west lane is 2 m longer.
        assert!(f.open_leaps(Class::Walker).iter().zip(f.open(Class::Walker)).all(|(l, o)| l <= o));
        assert_eq!(f.open_leaps(Class::Walker)[r("top:4,13")], f.open(Class::Walker)[r("top:4,13")]);
    }

    #[test]
    fn walls_reroute_and_full_blocks_fall_back_to_breaching() {
        let (m, mut walls) = setup();
        let c = |s: &str| m.cell_ref(s).unwrap();
        let gorge = c("top:9,19");
        walls[c("top:9,14").index()] = true;
        let f = Fields::build(&m, &walls, 8_000);
        let open = f.open(Class::Walker)[gorge.index()];
        assert!(open > 16_000 && open != UNREACHABLE, "a longer open route: {open}");
        assert_eq!(f.open(Class::Walker)[c("top:9,14").index()], UNREACHABLE, "the wall cell itself");
        assert_eq!(f.route(&m, Class::Walker, false, &walls, gorge), Route::Step(c("top:9,18")));
        assert_eq!(f.route(&m, Class::Walker, false, &walls, c("top:9,15")), Route::Step(c("top:9,16")), "turns back");

        for gate in ["top:10,7", "top:12,10", "top:9,12", "top:7,9"] {
            walls[c(gate).index()] = true;
        }
        walls[c("top:9,14").index()] = false;
        let f = Fields::build(&m, &walls, 8_000);
        assert_eq!(f.open(Class::Walker)[gorge.index()], UNREACHABLE);
        assert_eq!(f.breach(Class::Walker)[gorge.index()], 16_000 + 8_000, "straight through the south gate, plus the penalty");
        assert_eq!(f.route(&m, Class::Walker, false, &walls, c("top:9,13")), Route::Attack(c("top:9,12")));
        assert_eq!(f.route(&m, Class::Walker, true, &walls, c("top:9,13")), Route::Attack(c("top:9,12")), "no ledge helps");
        assert_eq!(f.route(&m, Class::Walker, false, &walls, gorge), Route::Step(c("top:9,18")));
        assert_eq!(f.remaining(Class::Walker, false, gorge), 24_000);
    }

    #[test]
    fn a_sealed_lane_with_a_ledge_is_an_open_route_for_jumpers() {
        let (m, mut walls) = setup();
        let c = |s: &str| m.cell_ref(s).unwrap();
        // Seal the west gorge lane: the connector east of it and the lane's exit to the ring.
        walls[c("top:5,17").index()] = true;
        walls[c("top:5,13").index()] = true;
        let f = Fields::build(&m, &walls, 8_000);
        let gorge = c("top:4,19");
        assert_eq!(f.open(Class::Walker)[gorge.index()], UNREACHABLE);
        assert_ne!(f.open_leaps(Class::Walker)[gorge.index()], UNREACHABLE);
        // Walkers that can't leap break a wall; jumpers walk up the lane and leap.
        assert!(matches!(f.route(&m, Class::Walker, false, &walls, c("top:4,14")), Route::Step(_) | Route::Attack(_)));
        assert!(matches!(f.route(&m, Class::Walker, false, &walls, c("top:4,13")), Route::Attack(_)));
        assert_eq!(f.route(&m, Class::Walker, true, &walls, gorge), Route::Step(c("top:4,18")));
        assert_eq!(f.route(&m, Class::Walker, true, &walls, c("top:4,17")), Route::Step(c("top:4,16")), "not the wall at 5,17");
        assert_eq!(f.route(&m, Class::Walker, true, &walls, c("top:4,13")), Route::Leap(c("top:4,11")));
        assert_eq!(f.remaining(Class::Walker, true, c("top:4,13")), 4_000 + f.open(Class::Walker)[c("top:4,11").index()]);
    }
}
