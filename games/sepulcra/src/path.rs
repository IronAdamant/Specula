//! Ground pathing: Dijkstra flow fields toward the graveyard.
//!
//! One field per movement class (walkers, climbers) and mode:
//!
//! - **open**: walls block. An enemy on a cell with a finite open distance has an open route and
//!   always takes it, however long (owner rule).
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

/// The four distance fields (mm to the nearest graveyard cell; [`UNREACHABLE`] if none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fields {
    pub open: [Vec<i32>; 2],
    pub breach: [Vec<i32>; 2],
}

/// Where a ground enemy standing on a cell goes next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Step(CellId),
    /// No open route: break this wall (adjacent to the enemy).
    Attack(CellId),
    /// Cut off even with walls broken (only on malformed maps).
    Stuck,
}

impl Fields {
    pub fn build(map: &Map, walls: &[bool], breach_penalty: i32) -> Fields {
        let f = |class: Class, penalty: Option<i32>| field(map, class, walls, penalty);
        Fields {
            open: [f(Class::Walker, None), f(Class::Climber, None)],
            breach: [f(Class::Walker, Some(breach_penalty)), f(Class::Climber, Some(breach_penalty))],
        }
    }

    pub fn open(&self, class: Class) -> &[i32] {
        &self.open[class as usize]
    }

    pub fn breach(&self, class: Class) -> &[i32] {
        &self.breach[class as usize]
    }

    /// The distance an enemy at `cell` still has to go (open route if any, else breach route).
    pub fn remaining(&self, class: Class, cell: CellId) -> i32 {
        let o = self.open(class)[cell.index()];
        if o != UNREACHABLE {
            o
        } else {
            self.breach(class)[cell.index()]
        }
    }

    /// The next move from `at`: downhill on the open field when it reaches the graveyard,
    /// otherwise downhill on the breach field, attacking a wall when the next cell holds one.
    pub fn route(&self, map: &Map, class: Class, walls: &[bool], at: CellId) -> Route {
        if self.open(class)[at.index()] != UNREACHABLE {
            if let Some(n) = downhill(map, class, self.open(class), at, |c| if walls[c.index()] { None } else { Some(0) }) {
                return Route::Step(n);
            }
        }
        if self.breach(class)[at.index()] != UNREACHABLE {
            // The breach field already counts each wall's penalty in the wall cell's distance.
            if let Some(n) = downhill(map, class, self.breach(class), at, |_| Some(0)) {
                return if walls[n.index()] { Route::Attack(n) } else { Route::Step(n) };
            }
        }
        Route::Stuck
    }
}

/// The neighbour minimising `dist[n] + cost + extra(n)`; ties to the lowest id (links are
/// sorted by id and only a strictly better value replaces the best).
fn downhill(map: &Map, class: Class, dist: &[i32], at: CellId, extra: impl Fn(CellId) -> Option<i32>) -> Option<CellId> {
    let mut best: Option<(i64, CellId)> = None;
    for l in map.links(at) {
        if !map.cell(l.to).kind.walkable(class.climbs()) || dist[l.to.index()] == UNREACHABLE {
            continue;
        }
        let Some(x) = extra(l.to) else { continue };
        let v = i64::from(dist[l.to.index()]) + i64::from(l.cost) + i64::from(x);
        if best.is_none_or(|(b, _)| v < b) {
            best = Some((v, l.to));
        }
    }
    best.map(|(_, c)| c)
}

fn field(map: &Map, class: Class, walls: &[bool], penalty: Option<i32>) -> Vec<i32> {
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
        for l in map.links(c) {
            let n = l.to;
            if !map.cell(n).kind.walkable(class.climbs()) {
                continue;
            }
            let extra = match (walls[n.index()], penalty) {
                (false, _) => 0,
                (true, None) => continue,
                (true, Some(p)) => p,
            };
            let nd = d.saturating_add(l.cost).saturating_add(extra);
            if nd < dist[n.index()] {
                dist[n.index()] = nd;
                heap.push(Reverse((nd, n)));
            }
        }
    }
    dist
}
