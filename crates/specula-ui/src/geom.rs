//! Pure geometry the widgets use: spatial focus navigation, radial-menu selection, and the
//! placement of edge threat arrows. Kept separate so it is easy to test exhaustively.

use crate::draw::Rect;
use std::f32::consts::{PI, TAU};

/// A navigation direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];

    pub fn index(self) -> usize {
        match self {
            Dir::Up => 0,
            Dir::Down => 1,
            Dir::Left => 2,
            Dir::Right => 3,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Dir::Up => "up",
            Dir::Down => "down",
            Dir::Left => "left",
            Dir::Right => "right",
        }
    }
}

/// The best focus target from `from` in direction `dir` among `candidates` (`(id, rect)`; the
/// source itself may be included, it is skipped by id). Scores favour targets that overlap the
/// source across the direction of travel, then the nearest edge distance, then centre offset.
pub fn neighbour(from_id: u32, from: Rect, dir: Dir, candidates: impl IntoIterator<Item = (u32, Rect)>) -> Option<u32> {
    let fc = from.center();
    let mut best: Option<(f32, u32)> = None;
    for (id, r) in candidates {
        if id == from_id || r.is_empty() {
            continue;
        }
        let c = r.center();
        // Distance travelled along `dir` (centre to centre, must be forward) and edge gap.
        let (forward, gap, overlap, ortho) = match dir {
            Dir::Right => (c[0] - fc[0], r.x - from.right(), span_overlap(from.y, from.bottom(), r.y, r.bottom()), (c[1] - fc[1]).abs()),
            Dir::Left => (fc[0] - c[0], from.x - r.right(), span_overlap(from.y, from.bottom(), r.y, r.bottom()), (c[1] - fc[1]).abs()),
            Dir::Down => (c[1] - fc[1], r.y - from.bottom(), span_overlap(from.x, from.right(), r.x, r.right()), (c[0] - fc[0]).abs()),
            Dir::Up => (fc[1] - c[1], from.y - r.bottom(), span_overlap(from.x, from.right(), r.x, r.right()), (c[0] - fc[0]).abs()),
        };
        if forward <= 0.5 {
            continue;
        }
        // Behind the source edge (overlapping along the travel axis) only counts if the centre is ahead.
        let gap = gap.max(0.0);
        let score = if overlap > 0.0 { gap + ortho * 0.25 } else { 1.0e5 + gap + ortho * 2.0 };
        if best.is_none_or(|(s, b)| score < s || (score == s && id < b)) {
            best = Some((score, id));
        }
    }
    best.map(|(_, id)| id)
}

fn span_overlap(a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

/// Which of `n` radial items (item 0 centred at the top, clockwise) a direction points at.
/// `v` is a screen-space vector (+y down). `None` inside the dead zone (`|v| < dead`).
pub fn radial_pick(v: [f32; 2], n: usize, dead: f32) -> Option<usize> {
    let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if n == 0 || !len.is_finite() || len < dead {
        return None;
    }
    // Angle from straight up, clockwise, in 0..TAU.
    let a = specula_core::dmath::atan2(v[0], -v[1]).rem_euclid(TAU);
    let sector = TAU / n as f32;
    Some((((a + sector * 0.5) / sector) as usize) % n)
}

/// Where an edge arrow goes for a world point projected to screen position `p` (physical px).
///
/// `behind`: the point is behind the camera (clip-space w < 0). Projecting such a point divides
/// by a negative w, which mirrors it through the screen centre; the arrow then points the other
/// way, which is the true direction. Returns `None` when the point is on screen (inside `bounds`
/// shrunk by `margin`) and in front, else the arrow centre and its angle (radians, y down, 0 =
/// pointing right).
pub fn threat_arrow(bounds: Rect, margin: f32, p: [f32; 2], behind: bool) -> Option<([f32; 2], f32)> {
    if !(p[0].is_finite() && p[1].is_finite()) || bounds.is_empty() {
        return None;
    }
    let inner = bounds.inset(margin, margin, margin, margin);
    if !behind && inner.contains(p) {
        return None;
    }
    let c = bounds.center();
    let mut d = [p[0] - c[0], p[1] - c[1]];
    if behind {
        d = [-d[0], -d[1]];
    }
    let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
    if len < 1e-3 {
        // Exactly behind the camera: point down (towards the player).
        d = [0.0, 1.0];
    }
    let hw = (inner.w * 0.5).max(0.0);
    let hh = (inner.h * 0.5).max(0.0);
    let tx = if d[0].abs() > 1e-6 { hw / d[0].abs() } else { f32::INFINITY };
    let ty = if d[1].abs() > 1e-6 { hh / d[1].abs() } else { f32::INFINITY };
    let t = tx.min(ty);
    let pos = [c[0] + d[0] * t, c[1] + d[1] * t];
    let angle = specula_core::dmath::atan2(d[1], d[0]);
    Some((pos, angle))
}

/// Shortest signed difference between two angles, in `-π..π`.
pub fn angle_diff(a: f32, b: f32) -> f32 {
    (a - b + PI).rem_euclid(TAU) - PI
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(x, y, w, h)
    }

    #[test]
    fn navigation_in_a_column_and_a_grid() {
        // A vertical menu.
        let items = [(0, r(100.0, 100.0, 200.0, 40.0)), (1, r(100.0, 150.0, 200.0, 40.0)), (2, r(100.0, 200.0, 200.0, 40.0))];
        assert_eq!(neighbour(0, items[0].1, Dir::Down, items), Some(1));
        assert_eq!(neighbour(1, items[1].1, Dir::Down, items), Some(2));
        assert_eq!(neighbour(2, items[2].1, Dir::Down, items), None);
        assert_eq!(neighbour(2, items[2].1, Dir::Up, items), Some(1));
        assert_eq!(neighbour(1, items[1].1, Dir::Left, items), None);
        // A 3×2 grid: right moves along the row, down to the same column.
        let mut grid = Vec::new();
        for row in 0..2 {
            for col in 0..3 {
                grid.push((row * 3 + col, r(col as f32 * 60.0, row as f32 * 60.0, 50.0, 50.0)));
            }
        }
        assert_eq!(neighbour(0, grid[0].1, Dir::Right, grid.clone()), Some(1));
        assert_eq!(neighbour(1, grid[1].1, Dir::Down, grid.clone()), Some(4));
        assert_eq!(neighbour(5, grid[5].1, Dir::Up, grid.clone()), Some(2));
        assert_eq!(neighbour(3, grid[3].1, Dir::Left, grid.clone()), None);
    }

    #[test]
    fn navigation_prefers_aligned_targets_over_closer_diagonal_ones() {
        let from = r(0.0, 0.0, 100.0, 30.0);
        let diag = (1, r(110.0, 40.0, 30.0, 30.0));
        let aligned = (2, r(300.0, 0.0, 100.0, 30.0));
        assert_eq!(neighbour(0, from, Dir::Right, [diag, aligned]), Some(2));
        // With nothing aligned, the diagonal one is still reachable.
        assert_eq!(neighbour(0, from, Dir::Right, [diag]), Some(1));
    }

    #[test]
    fn radial_selection_by_angle() {
        // 4 items: up, right, down, left.
        assert_eq!(radial_pick([0.0, -1.0], 4, 0.5), Some(0));
        assert_eq!(radial_pick([1.0, 0.0], 4, 0.5), Some(1));
        assert_eq!(radial_pick([0.0, 1.0], 4, 0.5), Some(2));
        assert_eq!(radial_pick([-1.0, 0.0], 4, 0.5), Some(3));
        // Just either side of the up/right boundary at 45°.
        assert_eq!(radial_pick([0.70, -0.72], 4, 0.5), Some(0));
        assert_eq!(radial_pick([0.72, -0.70], 4, 0.5), Some(1));
        // Up-left wraps to item 0, not out of range.
        assert_eq!(radial_pick([-0.1, -1.0], 4, 0.5), Some(0));
        // Dead zone.
        assert_eq!(radial_pick([0.1, 0.1], 4, 0.5), None);
        assert_eq!(radial_pick([f32::NAN, 0.0], 4, 0.5), None);
        // 5 items: 72° sectors.
        for k in 0..5 {
            let a = crate::layout::radial_angle(k, 5);
            assert_eq!(radial_pick([a.cos(), a.sin()], 5, 0.5), Some(k), "{k}");
        }
    }

    #[test]
    fn threat_arrows_in_every_direction() {
        let b = r(0.0, 0.0, 1000.0, 500.0);
        let m = 20.0;
        // On screen and in front: no arrow.
        assert_eq!(threat_arrow(b, m, [500.0, 250.0], false), None);
        // Off each edge: on that edge, pointing out.
        let cases = [([1500.0, 250.0], [980.0, 250.0], 0.0), ([-300.0, 250.0], [20.0, 250.0], PI), ([500.0, 900.0], [500.0, 480.0], PI / 2.0), ([500.0, -50.0], [500.0, 20.0], -PI / 2.0)];
        for (p, want, angle) in cases {
            let (pos, a) = threat_arrow(b, m, p, false).unwrap();
            assert!((pos[0] - want[0]).abs() < 1e-3 && (pos[1] - want[1]).abs() < 1e-3, "{p:?} -> {pos:?}");
            assert!(angle_diff(a, angle).abs() < 1e-4, "{p:?}: {a}");
        }
        // A corner: lands on the inner rect boundary, pointing diagonally.
        let (pos, a) = threat_arrow(b, m, [2000.0, 1250.0], false).unwrap();
        assert!((pos[1] - 480.0).abs() < 1e-3 || (pos[0] - 980.0).abs() < 1e-3, "{pos:?}");
        assert!(a > 0.0 && a < PI / 2.0);
        // Behind the camera: projected left of centre means it is really to the right.
        let (pos, a) = threat_arrow(b, m, [400.0, 250.0], true).unwrap();
        assert!((pos[0] - 980.0).abs() < 1e-3, "{pos:?}");
        assert!(angle_diff(a, 0.0).abs() < 1e-4);
        // Behind and dead centre: points down.
        let (pos, a) = threat_arrow(b, m, [500.0, 250.0], true).unwrap();
        assert!((pos[1] - 480.0).abs() < 1e-3);
        assert!(angle_diff(a, PI / 2.0).abs() < 1e-4);
        // Garbage in: nothing.
        assert_eq!(threat_arrow(b, m, [f32::INFINITY, 0.0], false), None);
        // Every direction lands on the boundary of the inner rect.
        let inner = b.inset(m, m, m, m);
        for k in 0..72 {
            let t = k as f32 * TAU / 72.0;
            for behind in [false, true] {
                let p = [500.0 + 3000.0 * t.cos(), 250.0 + 3000.0 * t.sin()];
                let (pos, _) = threat_arrow(b, m, p, behind).unwrap();
                let on_x = (pos[0] - inner.x).abs() < 1e-2 || (pos[0] - inner.right()).abs() < 1e-2;
                let on_y = (pos[1] - inner.y).abs() < 1e-2 || (pos[1] - inner.bottom()).abs() < 1e-2;
                assert!(on_x || on_y, "{k} {behind}: {pos:?}");
                assert!(inner.grow(0.01).contains(pos) || on_x || on_y);
            }
        }
    }
}
