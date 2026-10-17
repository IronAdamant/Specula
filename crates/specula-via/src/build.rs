//! Building one tile: voxelise triangles into spans, find the open space an agent fits in, link
//! neighbouring open spans an agent can step between, erode by the agent's radius, and merge what
//! is left into rectangles (docs/design/NAVIGATION.md, "Navmesh generation").
//!
//! Every step is integer arithmetic on the grid, except clipping triangles to columns (IEEE
//! `+ - * /` in a fixed order) and the floor/ceil quantisation (`dmath`), so a tile's result
//! depends only on its triangles and their order, never on the tiles around it or on the order
//! tiles are built in.

use crate::{Config, Derived, NavError, Source, Triangle, MAX_POLYS_PER_TILE, MAX_SPANS_PER_COLUMN, MAX_TILE_TRIANGLES};
use specula_core::dmath;
use specula_core::math::Vec3;

/// Neighbour directions: -x, +z, +x, -z (index = side of a cell or rectangle).
pub const DX: [i32; 4] = [-1, 0, 1, 0];
pub const DZ: [i32; 4] = [0, 1, 0, -1];

/// The opposite side.
pub fn opposite(side: usize) -> usize {
    (side + 2) % 4
}

const NO_LINK: u8 = 0xFF;
const NO_POLY: u32 = u32::MAX;
/// Bilinear height error a rectangle tolerates, in cell heights.
const HEIGHT_TOLERANCE: f32 = 2.0;
/// Walkable islands smaller than this many columns are dropped (unless they reach past the
/// tile's border, where the neighbouring tile may see more of them): the tops of wall junctions
/// and similar specks nobody can reach (Recast's minimum region area).
pub const MIN_ISLAND_CELLS: usize = 8;

/// A solid interval in one column, in cell heights above the bounds' floor.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Span {
    min: u16,
    max: u16,
    walk: bool,
}

/// The open space above a walkable span.
#[derive(Clone, Copy, Debug)]
struct Open {
    floor: u16,
    ceil: u16,
    link: [u8; 4],
    dist: u16,
    alive: bool,
}

/// One rectangle of a tile, in global column coordinates (inclusive).
#[derive(Clone, Debug, PartialEq)]
pub struct Rect {
    pub x0: i32,
    pub z0: i32,
    pub x1: i32,
    pub z1: i32,
    /// Floor heights (world Y) at the corner columns' centres: (x0, z0), (x1, z0), (x0, z1), (x1, z1).
    pub h: [f32; 4],
    /// Links to rectangles of the same tile: (other rectangle, side, first, last column along the side).
    pub links: Vec<(u32, u8, i32, i32)>,
}

/// An open span on a tile's edge, for linking with the neighbouring tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EdgeSpan {
    pub floor: u16,
    pub ceil: u16,
    pub poly: u32,
}

/// What building a tile produced.
#[derive(Clone, Debug, Default)]
pub struct TileBuild {
    pub rects: Vec<Rect>,
    /// Per side (see [`DX`]), per column along that edge: the edge spans and their rectangles.
    pub edges: [Vec<Vec<EdgeSpan>>; 4],
    pub triangles: usize,
    pub spans: usize,
    pub open: usize,
    pub eroded: usize,
    pub errors: Vec<NavError>,
    /// Wall-clock microseconds per stage (rasterise, open space and links, erode and islands,
    /// rectangles, rectangle links): a measurement, never an input.
    pub times_us: [f64; 5],
}

/// Up to 12 points: a triangle clipped by four lines has at most 7.
#[derive(Clone, Copy)]
struct Poly {
    v: [Vec3; 12],
    n: usize,
}

impl Poly {
    fn empty() -> Poly {
        Poly { v: [Vec3::ZERO; 12], n: 0 }
    }

    fn push(&mut self, p: Vec3) {
        if self.n < 12 {
            self.v[self.n] = p;
            self.n += 1;
        }
    }
}

fn axis(p: Vec3, a: usize) -> f32 {
    if a == 0 {
        p.x
    } else {
        p.z
    }
}

/// Splits `p` by the line `axis = at` into the part below and the part above.
fn split(p: &Poly, a: usize, at: f32) -> (Poly, Poly) {
    let (mut below, mut above) = (Poly::empty(), Poly::empty());
    if p.n == 0 {
        return (below, above);
    }
    let mut d = [0.0f32; 12];
    for (di, v) in d.iter_mut().zip(&p.v[..p.n]) {
        *di = axis(*v, a) - at;
    }
    let mut j = p.n - 1;
    for i in 0..p.n {
        let (ina, inb) = (d[j] >= 0.0, d[i] >= 0.0);
        if ina != inb {
            let s = d[j] / (d[j] - d[i]);
            let (vj, vi) = (p.v[j], p.v[i]);
            let q = Vec3::new(vj.x + (vi.x - vj.x) * s, vj.y + (vi.y - vj.y) * s, vj.z + (vi.z - vj.z) * s);
            below.push(q);
            above.push(q);
        }
        if d[i] > 0.0 {
            above.push(p.v[i]);
        } else if d[i] < 0.0 {
            below.push(p.v[i]);
        } else {
            below.push(p.v[i]);
            above.push(p.v[i]);
        }
        j = i;
    }
    (below, above)
}

/// Adds a span to a column, merging it with the spans it overlaps. A merged span is walkable if
/// the span forming its top is, or if the tops are within `merge` of each other and either is.
fn add_span(col: &mut Vec<Span>, mut s: Span, merge: u16) -> bool {
    let mut i = 0;
    while i < col.len() {
        let c = col[i];
        if c.max < s.min {
            i += 1;
            continue;
        }
        if c.min > s.max {
            break;
        }
        let walk = if c.max.abs_diff(s.max) <= merge {
            c.walk || s.walk
        } else if c.max > s.max {
            c.walk
        } else {
            s.walk
        };
        s = Span { min: c.min.min(s.min), max: c.max.max(s.max), walk };
        col.remove(i);
    }
    if col.len() >= MAX_SPANS_PER_COLUMN {
        return false;
    }
    col.insert(i, s);
    true
}

/// The grid a tile is built on: its rectangle of columns plus a border.
struct Grid {
    /// Global column of local (0, 0).
    gx0: i32,
    gz0: i32,
    /// Local columns per side (tile + 2 × border).
    w: i32,
    /// Local range of the tile's own (inner) columns that lie inside the navmesh, `[lo, hi)`.
    inner_x: (i32, i32),
    inner_z: (i32, i32),
    cells_x: i32,
    cells_z: i32,
}

impl Grid {
    fn valid(&self, lx: i32, lz: i32) -> bool {
        let (gx, gz) = (self.gx0 + lx, self.gz0 + lz);
        lx >= 0 && lz >= 0 && lx < self.w && lz < self.w && gx >= 0 && gz >= 0 && gx < self.cells_x && gz < self.cells_z
    }

    fn inner(&self, lx: i32, lz: i32) -> bool {
        lx >= self.inner_x.0 && lx < self.inner_x.1 && lz >= self.inner_z.0 && lz < self.inner_z.1
    }

    fn col(&self, lx: i32, lz: i32) -> usize {
        (lz * self.w + lx) as usize
    }
}

/// Buffers a tile build reuses (so building tiles allocates little). Holds no state between
/// builds: every buffer is reset before use.
#[derive(Clone, Debug, Default)]
pub struct Scratch {
    cols: Vec<Vec<Span>>,
    pieces: Vec<(usize, u16, u16, bool)>,
    start: Vec<u32>,
    opens: Vec<Open>,
    seen: Vec<bool>,
    queue: Vec<(u32, i32, i32)>,
    poly_of: Vec<u32>,
}

/// Builds tile `(tx, tz)` from the sources that touch it (in a fixed order).
pub fn build_tile(cfg: &Config, d: &Derived, tx: u32, tz: u32, sources: &[&Source], scratch: &mut Scratch) -> TileBuild {
    let t = cfg.tile_cells as i32;
    let b = d.border as i32;
    let w = t + 2 * b;
    let grid = Grid {
        gx0: tx as i32 * t - b,
        gz0: tz as i32 * t - b,
        w,
        inner_x: (b, (b + t).min(d.cells_x as i32 - (tx as i32 * t - b))),
        inner_z: (b, (b + t).min(d.cells_z as i32 - (tz as i32 * t - b))),
        cells_x: d.cells_x as i32,
        cells_z: d.cells_z as i32,
    };
    let mut out = TileBuild { edges: std::array::from_fn(|_| vec![Vec::new(); t as usize]), ..TileBuild::default() };
    let mut clock = std::time::Instant::now();
    let mut lap = |out: &mut TileBuild, i: usize| {
        let now = std::time::Instant::now();
        out.times_us[i] = (now - clock).as_secs_f64() * 1.0e6;
        clock = now;
    };
    let mut cols = std::mem::take(&mut scratch.cols);
    cols.resize_with((w * w) as usize, Vec::new);
    for c in cols.iter_mut() {
        c.clear();
    }
    let mut dropped = 0usize;
    let mut budget = MAX_TILE_TRIANGLES;
    let mut pieces = std::mem::take(&mut scratch.pieces);
    pieces.clear();
    for src in sources {
        if src.triangle_count() > budget {
            let total: usize = sources.iter().map(|s| s.triangle_count()).sum();
            out.errors.push(
                NavError::new("budget", format!("a tile takes at most {MAX_TILE_TRIANGLES} triangles; {total} touched it, the rest were left out"))
                    .with("tile", crate::Json::arr([crate::Json::from(tx), crate::Json::from(tz)])),
            );
            break;
        }
        budget -= src.triangle_count();
        if src.aligned {
            if fill_box(cfg, d, &grid, src.lo, src.hi, &mut cols) {
                out.triangles += 12;
            }
        } else if src.solid {
            // A convex solid fills each column from its lowest to its highest point; the top's
            // walkability is the highest piece's.
            pieces.clear();
            for tri in &src.triangles {
                if rasterise(cfg, d, &grid, tri, &mut |c, lo, hi, walk| pieces.push((c, lo, hi, walk))) {
                    out.triangles += 1;
                }
            }
            pieces.sort_by_key(|p| p.0);
            let mut i = 0;
            while i < pieces.len() {
                let c = pieces[i].0;
                let (mut lo, mut hi, mut walk) = (pieces[i].1, pieces[i].2, pieces[i].3);
                i += 1;
                while i < pieces.len() && pieces[i].0 == c {
                    let p = pieces[i];
                    lo = lo.min(p.1);
                    if p.2 > hi {
                        hi = p.2;
                        walk = p.3;
                    } else if p.2 == hi {
                        walk |= p.3;
                    }
                    i += 1;
                }
                add_span(&mut cols[c], Span { min: lo, max: hi, walk }, d.climb_q);
            }
        } else {
            for tri in &src.triangles {
                if rasterise(cfg, d, &grid, tri, &mut |c, lo, hi, walk| {
                    add_span(&mut cols[c], Span { min: lo, max: hi, walk }, d.climb_q);
                }) {
                    out.triangles += 1;
                }
            }
        }
    }
    // Low obstacles (a curb, the lip of a step) are walkable when the span below is and the
    // step up is climbable.
    for col in &mut cols {
        let (mut prev_walk, mut prev_max) = (false, 0u16);
        for s in col.iter_mut() {
            let walk = s.walk;
            if !s.walk && prev_walk && s.max.abs_diff(prev_max) <= d.climb_q {
                s.walk = true;
            }
            prev_walk = walk;
            prev_max = s.max;
        }
        out.spans += col.len();
        if col.len() >= MAX_SPANS_PER_COLUMN {
            dropped += 1;
        }
    }
    if dropped > 0 {
        out.errors.push(
            NavError::new("budget", format!("{dropped} columns reached {MAX_SPANS_PER_COLUMN} spans; higher geometry there was left out"))
                .with("tile", crate::Json::arr([crate::Json::from(tx), crate::Json::from(tz)])),
        );
    }
    lap(&mut out, 0);
    // Open space above walkable spans, tall enough for the agent.
    let mut start = std::mem::take(&mut scratch.start);
    start.clear();
    start.resize((w * w + 1) as usize, 0);
    let mut opens = std::mem::take(&mut scratch.opens);
    opens.clear();
    for lz in 0..w {
        for lx in 0..w {
            let c = grid.col(lx, lz);
            start[c] = opens.len() as u32;
            if !grid.valid(lx, lz) {
                continue;
            }
            let col = &cols[c];
            for (i, s) in col.iter().enumerate() {
                let ceil = col.get(i + 1).map_or(u16::MAX, |n| n.min);
                if s.walk && ceil.saturating_sub(s.max) >= d.height_q && s.max <= d.max_q {
                    opens.push(Open { floor: s.max, ceil, link: [NO_LINK; 4], dist: u16::MAX, alive: true });
                }
            }
        }
    }
    start[(w * w) as usize] = opens.len() as u32;
    out.open = opens.len();
    // Links: the lowest open span next door an agent can step to and stand in.
    for lz in 0..w {
        for lx in 0..w {
            let c = grid.col(lx, lz);
            for g in start[c]..start[c + 1] {
                let o = opens[g as usize];
                for dir in 0..4 {
                    let (nx, nz) = (lx + DX[dir], lz + DZ[dir]);
                    if nx < 0 || nz < 0 || nx >= w || nz >= w {
                        continue;
                    }
                    let nc = grid.col(nx, nz);
                    let (ns, ne) = (start[nc], start[nc + 1]);
                    for k in ns..ne {
                        let n = opens[k as usize];
                        let gap = o.ceil.min(n.ceil).saturating_sub(o.floor.max(n.floor));
                        if o.floor.abs_diff(n.floor) <= d.climb_q && gap >= d.height_q {
                            let idx = k - ns;
                            if idx < u32::from(NO_LINK) {
                                opens[g as usize].link[dir] = idx as u8;
                            }
                            break;
                        }
                    }
                }
            }
        }
    }
    let neighbour = |opens: &[Open], g: u32, lx: i32, lz: i32, dir: usize| -> Option<(u32, i32, i32)> {
        let l = opens[g as usize].link[dir];
        if l == NO_LINK {
            return None;
        }
        let (nx, nz) = (lx + DX[dir], lz + DZ[dir]);
        Some((start[grid.col(nx, nz)] + u32::from(l), nx, nz))
    };
    lap(&mut out, 1);
    // Erode: chamfer distance (2 straight, 3 diagonal) from spans missing a neighbour.
    for o in opens.iter_mut() {
        if o.link.contains(&NO_LINK) {
            o.dist = 0;
        }
    }
    let relax = |opens: &mut [Open], g: u32, n: u32, add: u16| {
        let v = opens[n as usize].dist.saturating_add(add);
        if v < opens[g as usize].dist {
            opens[g as usize].dist = v;
        }
    };
    for lz in 0..w {
        for lx in 0..w {
            let c = grid.col(lx, lz);
            for g in start[c]..start[c + 1] {
                if let Some((n, nx, nz)) = neighbour(&opens, g, lx, lz, 0) {
                    relax(&mut opens, g, n, 2);
                    if let Some((nn, _, _)) = neighbour(&opens, n, nx, nz, 3) {
                        relax(&mut opens, g, nn, 3);
                    }
                }
                if let Some((n, nx, nz)) = neighbour(&opens, g, lx, lz, 3) {
                    relax(&mut opens, g, n, 2);
                    if let Some((nn, _, _)) = neighbour(&opens, n, nx, nz, 2) {
                        relax(&mut opens, g, nn, 3);
                    }
                }
            }
        }
    }
    for lz in (0..w).rev() {
        for lx in (0..w).rev() {
            let c = grid.col(lx, lz);
            for g in start[c]..start[c + 1] {
                if let Some((n, nx, nz)) = neighbour(&opens, g, lx, lz, 2) {
                    relax(&mut opens, g, n, 2);
                    if let Some((nn, _, _)) = neighbour(&opens, n, nx, nz, 1) {
                        relax(&mut opens, g, nn, 3);
                    }
                }
                if let Some((n, nx, nz)) = neighbour(&opens, g, lx, lz, 1) {
                    relax(&mut opens, g, n, 2);
                    if let Some((nn, _, _)) = neighbour(&opens, n, nx, nz, 0) {
                        relax(&mut opens, g, nn, 3);
                    }
                }
            }
        }
    }
    let threshold = (d.radius_cells * 2).min(u32::from(u16::MAX)) as u16;
    for o in opens.iter_mut() {
        if o.dist < threshold {
            o.alive = false;
        }
    }
    // Drop tiny islands that don't reach the grid's outer ring.
    let mut seen = std::mem::take(&mut scratch.seen);
    seen.clear();
    seen.resize(opens.len(), false);
    let mut queue = std::mem::take(&mut scratch.queue);
    for lz in 0..w {
        for lx in 0..w {
            let c = grid.col(lx, lz);
            for g in start[c]..start[c + 1] {
                if seen[g as usize] || !opens[g as usize].alive {
                    continue;
                }
                seen[g as usize] = true;
                queue.clear();
                queue.push((g, lx, lz));
                let mut i = 0;
                let mut edge = false;
                while i < queue.len() {
                    let (s, x, z) = queue[i];
                    i += 1;
                    edge |= x == 0 || z == 0 || x == w - 1 || z == w - 1;
                    for dir in 0..4 {
                        if let Some((n, nx, nz)) = neighbour(&opens, s, x, z, dir) {
                            if opens[n as usize].alive && !seen[n as usize] {
                                seen[n as usize] = true;
                                queue.push((n, nx, nz));
                            }
                        }
                    }
                    if queue.len() >= MIN_ISLAND_CELLS {
                        edge = true;
                    }
                }
                if !edge {
                    for &(s, _, _) in &queue {
                        opens[s as usize].alive = false;
                    }
                }
            }
        }
    }
    out.eroded = opens.iter().filter(|o| !o.alive).count();
    lap(&mut out, 2);
    // Rectangles over the tile's own columns, rows first, in a fixed order.
    let floor_y = |q: u16| cfg.bounds_min.y + f32::from(q) * cfg.cell_height;
    let mut poly_of = std::mem::take(&mut scratch.poly_of);
    poly_of.clear();
    poly_of.resize(opens.len(), NO_POLY);
    let mut grids: Vec<Vec<Vec<u32>>> = Vec::new();
    let mut origin: Vec<(i32, i32)> = Vec::new();
    let mut truncated = false;
    'rows: for lz in grid.inner_z.0..grid.inner_z.1 {
        for lx in grid.inner_x.0..grid.inner_x.1 {
            let c = grid.col(lx, lz);
            for g in start[c]..start[c + 1] {
                if !opens[g as usize].alive || poly_of[g as usize] != NO_POLY {
                    continue;
                }
                if out.rects.len() >= MAX_POLYS_PER_TILE {
                    truncated = true;
                    break 'rows;
                }
                let free = |opens: &[Open], poly_of: &[u32], n: u32| opens[n as usize].alive && poly_of[n as usize] == NO_POLY;
                let mut row = vec![g];
                loop {
                    let last = *row.last().expect("not empty");
                    let x = lx + row.len() as i32 - 1;
                    if x + 1 >= grid.inner_x.1 {
                        break;
                    }
                    let Some((n, _, _)) = neighbour(&opens, last, x, lz, 2) else { break };
                    if !free(&opens, &poly_of, n) {
                        break;
                    }
                    row.push(n);
                    if !fits(&opens, std::slice::from_ref(&row)) {
                        row.pop();
                        break;
                    }
                }
                let mut rows = vec![row];
                loop {
                    let z = lz + rows.len() as i32 - 1;
                    if z + 1 >= grid.inner_z.1 {
                        break;
                    }
                    let prev = rows.last().expect("not empty");
                    let mut next: Vec<u32> = Vec::with_capacity(prev.len());
                    let mut ok = true;
                    for (k, &p) in prev.iter().enumerate() {
                        let x = lx + k as i32;
                        match neighbour(&opens, p, x, z, 1) {
                            Some((n, _, _)) if free(&opens, &poly_of, n) => {
                                if k > 0 && neighbour(&opens, next[k - 1], x - 1, z + 1, 2).map(|(m, _, _)| m) != Some(n) {
                                    ok = false;
                                    break;
                                }
                                next.push(n);
                            }
                            _ => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if !ok {
                        break;
                    }
                    rows.push(next);
                    if !fits(&opens, &rows) {
                        rows.pop();
                        break;
                    }
                }
                let id = out.rects.len() as u32;
                for r in &rows {
                    for &s in r {
                        poly_of[s as usize] = id;
                    }
                }
                let (wd, ht) = (rows[0].len(), rows.len());
                let f = |r: usize, c: usize| floor_y(opens[rows[r][c] as usize].floor);
                out.rects.push(Rect {
                    x0: grid.gx0 + lx,
                    z0: grid.gz0 + lz,
                    x1: grid.gx0 + lx + wd as i32 - 1,
                    z1: grid.gz0 + lz + ht as i32 - 1,
                    h: [f(0, 0), f(0, wd - 1), f(ht - 1, 0), f(ht - 1, wd - 1)],
                    links: Vec::new(),
                });
                grids.push(rows);
                origin.push((lx, lz));
            }
        }
    }
    if truncated {
        out.errors.push(
            NavError::new("budget", format!("a tile holds at most {MAX_POLYS_PER_TILE} polygons; the rest of it was left out"))
                .with("tile", crate::Json::arr([crate::Json::from(tx), crate::Json::from(tz)])),
        );
    }
    lap(&mut out, 3);
    // Links inside the tile, and the tile's edge spans for its neighbours.
    for (pi, rows) in grids.iter().enumerate() {
        let (lx0, lz0) = origin[pi];
        let (wd, ht) = (rows[0].len() as i32, rows.len() as i32);
        let mut links: Vec<(u32, u8, i32, i32)> = Vec::new();
        for side in 0..4usize {
            let cells: Vec<(u32, i32, i32)> = match side {
                0 => (0..ht).map(|j| (rows[j as usize][0], lx0, lz0 + j)).collect(),
                2 => (0..ht).map(|j| (rows[j as usize][(wd - 1) as usize], lx0 + wd - 1, lz0 + j)).collect(),
                3 => (0..wd).map(|i| (rows[0][i as usize], lx0 + i, lz0)).collect(),
                _ => (0..wd).map(|i| (rows[(ht - 1) as usize][i as usize], lx0 + i, lz0 + ht - 1)).collect(),
            };
            let mut run: Option<(u32, i32, i32)> = None;
            for (s, lx, lz) in cells {
                let along = if side == 0 || side == 2 { grid.gz0 + lz } else { grid.gx0 + lx };
                let (nx, nz) = (lx + DX[side], lz + DZ[side]);
                let mut to = None;
                if grid.inner(nx, nz) {
                    if let Some((n, _, _)) = neighbour(&opens, s, lx, lz, side) {
                        if opens[n as usize].alive && poly_of[n as usize] != NO_POLY {
                            to = Some(poly_of[n as usize]);
                        }
                    }
                } else {
                    // Past the tile's edge: the neighbouring tile links to it (see `mesh`).
                    let o = opens[s as usize];
                    let i = if side == 0 || side == 2 { along - (grid.gz0 + b) } else { along - (grid.gx0 + b) };
                    if (0..t).contains(&i) {
                        out.edges[side][i as usize].push(EdgeSpan { floor: o.floor, ceil: o.ceil, poly: pi as u32 });
                    }
                }
                match (run, to) {
                    (Some((q, lo, hi)), Some(n)) if q == n && hi + 1 == along => run = Some((q, lo, along)),
                    (r, n) => {
                        if let Some((q, lo, hi)) = r {
                            links.push((q, side as u8, lo, hi));
                        }
                        run = n.map(|n| (n, along, along));
                    }
                }
            }
            if let Some((q, lo, hi)) = run {
                links.push((q, side as u8, lo, hi));
            }
        }
        out.rects[pi].links = links;
    }
    *scratch = Scratch { cols, pieces, start, opens, seen, queue, poly_of };
    lap(&mut out, 4);
    out
}

/// Whether every column of the rectangle lies within [`HEIGHT_TOLERANCE`] of the bilinear
/// surface through its corners.
fn fits(opens: &[Open], rows: &[Vec<u32>]) -> bool {
    let (h, w) = (rows.len(), rows[0].len());
    let f = |r: usize, c: usize| f32::from(opens[rows[r][c] as usize].floor);
    let (f00, f10, f01, f11) = (f(0, 0), f(0, w - 1), f(h - 1, 0), f(h - 1, w - 1));
    for (j, row) in rows.iter().enumerate() {
        let v = if h > 1 { j as f32 / (h - 1) as f32 } else { 0.0 };
        for (i, &s) in row.iter().enumerate() {
            let u = if w > 1 { i as f32 / (w - 1) as f32 } else { 0.0 };
            let top = f00 + (f10 - f00) * u;
            let bottom = f01 + (f11 - f01) * u;
            let want = top + (bottom - top) * v;
            if (f32::from(opens[s as usize].floor) - want).abs() > HEIGHT_TOLERANCE {
                return false;
            }
        }
    }
    true
}

/// Fills the tile's columns an axis-aligned solid box covers: columns it overlaps get a span
/// from its bottom to its top, walkable (its top is flat); columns it only touches along a
/// column line get a non-walkable span, as its side faces would. False when it misses them.
fn fill_box(cfg: &Config, d: &Derived, grid: &Grid, lo: Vec3, hi: Vec3, cols: &mut [Vec<Span>]) -> bool {
    let (bmin, cs, ch) = (cfg.bounds_min, cfg.cell_size, cfg.cell_height);
    if hi.y < bmin.y || lo.y > cfg.bounds_max.y {
        return false;
    }
    let to_col = |v: f32, o: f32| dmath::floor((v - o) / cs) as i64;
    let x0 = to_col(lo.x, bmin.x).max(i64::from(grid.gx0)).max(0);
    let z0 = to_col(lo.z, bmin.z).max(i64::from(grid.gz0)).max(0);
    let x1 = to_col(hi.x, bmin.x).min(i64::from(grid.gx0 + grid.w - 1)).min(i64::from(grid.cells_x - 1));
    let z1 = to_col(hi.z, bmin.z).min(i64::from(grid.gz0 + grid.w - 1)).min(i64::from(grid.cells_z - 1));
    if x0 > x1 || z0 > z1 {
        return false;
    }
    let inv_ch = 1.0 / ch;
    let smin = dmath::floor((lo.y - bmin.y) * inv_ch + 1.0e-3) as i64;
    let smax = dmath::ceil((hi.y - bmin.y) * inv_ch - 1.0e-3) as i64;
    if smax < 0 || smin > i64::from(d.max_q) {
        return false;
    }
    let smin = smin.clamp(0, i64::from(d.max_q)) as u16;
    let smax = smax.clamp(i64::from(smin), i64::from(d.max_q)) as u16;
    for gz in z0..=z1 {
        let (c0, c1) = (bmin.z + gz as f32 * cs, bmin.z + (gz + 1) as f32 * cs);
        let inside_z = c1 > lo.z && c0 < hi.z;
        for gx in x0..=x1 {
            let (r0, r1) = (bmin.x + gx as f32 * cs, bmin.x + (gx + 1) as f32 * cs);
            let inside = inside_z && r1 > lo.x && r0 < hi.x;
            let (lx, lz) = (gx as i32 - grid.gx0, gz as i32 - grid.gz0);
            add_span(&mut cols[grid.col(lx, lz)], Span { min: smin, max: smax, walk: inside }, d.climb_q);
        }
    }
    true
}

/// Rasterises one triangle into the tile's columns, handing each covered column's piece to
/// `emit(column, min, max, walkable)`; false when it misses them.
fn rasterise(cfg: &Config, d: &Derived, grid: &Grid, tri: &Triangle, emit: &mut dyn FnMut(usize, u16, u16, bool)) -> bool {
    let (lo, hi) = tri.aabb();
    let (bmin, cs, ch) = (cfg.bounds_min, cfg.cell_size, cfg.cell_height);
    if hi.y < bmin.y || lo.y > cfg.bounds_max.y {
        return false;
    }
    let to_col = |v: f32, o: f32| dmath::floor((v - o) / cs) as i64;
    let clamp_lo = |v: i64, g0: i32| v.max(i64::from(g0)).max(0);
    let x0 = clamp_lo(to_col(lo.x, bmin.x), grid.gx0);
    let z0 = clamp_lo(to_col(lo.z, bmin.z), grid.gz0);
    let x1 = to_col(hi.x, bmin.x).min(i64::from(grid.gx0 + grid.w - 1)).min(i64::from(grid.cells_x - 1));
    let z1 = to_col(hi.z, bmin.z).min(i64::from(grid.gz0 + grid.w - 1)).min(i64::from(grid.cells_z - 1));
    if x0 > x1 || z0 > z1 {
        return false;
    }
    let [a, b, c] = tri.0;
    let n = (b - a).cross(c - a);
    let len = n.length();
    let walk = len > 0.0 && n.y.abs() / len >= d.walkable_y;
    let mut poly = Poly::empty();
    poly.push(a);
    poly.push(b);
    poly.push(c);
    // Cut off what lies before the first row.
    let (_, rest) = split(&poly, 1, bmin.z + z0 as f32 * cs);
    let mut rest = rest;
    let inv_ch = 1.0 / ch;
    let mut any = false;
    for gz in z0..=z1 {
        let (row, next) = split(&rest, 1, bmin.z + (gz + 1) as f32 * cs);
        rest = next;
        if row.n < 3 {
            continue;
        }
        let (_, mut cells) = split(&row, 0, bmin.x + x0 as f32 * cs);
        for gx in x0..=x1 {
            let (cell, next) = split(&cells, 0, bmin.x + (gx + 1) as f32 * cs);
            cells = next;
            if cell.n < 3 {
                continue;
            }
            let (mut ymin, mut ymax) = (cell.v[0].y, cell.v[0].y);
            for p in &cell.v[1..cell.n] {
                ymin = ymin.min(p.y);
                ymax = ymax.max(p.y);
            }
            // Within a thousandth of a step counts as on it, so a floor at a whole height keeps it.
            let smin = dmath::floor((ymin - bmin.y) * inv_ch + 1.0e-3) as i64;
            let smax = dmath::ceil((ymax - bmin.y) * inv_ch - 1.0e-3) as i64;
            if smax < 0 || smin > i64::from(d.max_q) {
                continue;
            }
            let smin = smin.clamp(0, i64::from(d.max_q)) as u16;
            let smax = smax.clamp(i64::from(smin), i64::from(d.max_q)) as u16;
            let (lx, lz) = (gx as i32 - grid.gx0, gz as i32 - grid.gz0);
            emit(grid.col(lx, lz), smin, smax, walk);
            any = true;
        }
    }
    any
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(x0: f32, z0: f32, x1: f32, z1: f32, y: f32) -> [Triangle; 2] {
        let (a, b, c, d) = (Vec3::new(x0, y, z0), Vec3::new(x1, y, z0), Vec3::new(x1, y, z1), Vec3::new(x0, y, z1));
        [Triangle([a, c, b]), Triangle([a, d, c])]
    }

    #[test]
    fn split_keeps_area() {
        let mut p = Poly::empty();
        p.push(Vec3::new(0.0, 0.0, 0.0));
        p.push(Vec3::new(2.0, 0.0, 0.0));
        p.push(Vec3::new(0.0, 0.0, 2.0));
        let (a, b) = split(&p, 0, 1.0);
        assert_eq!((a.n, b.n), (4, 3));
        assert!(a.v[..a.n].iter().all(|v| v.x <= 1.0));
        assert!(b.v[..b.n].iter().all(|v| v.x >= 1.0));
    }

    #[test]
    fn spans_merge_by_top() {
        let mut col = Vec::new();
        add_span(&mut col, Span { min: 0, max: 10, walk: true }, 1);
        add_span(&mut col, Span { min: 5, max: 20, walk: false }, 1);
        assert_eq!(col, vec![Span { min: 0, max: 20, walk: false }]);
        add_span(&mut col, Span { min: 30, max: 31, walk: true }, 1);
        add_span(&mut col, Span { min: 19, max: 21, walk: true }, 1);
        assert_eq!(col, vec![Span { min: 0, max: 21, walk: true }, Span { min: 30, max: 31, walk: true }]);
    }

    #[test]
    fn aligned_boxes_fill_like_their_triangles() {
        let cfg = Config { bounds_min: Vec3::new(-3.0, -1.0, -3.0), bounds_max: Vec3::new(3.0, 4.0, 3.0), tile_cells: 48, ..Config::default() };
        let d = Derived::of(&cfg);
        let floor = Source::new(quad(-3.0, -3.0, 3.0, 3.0, 0.0).to_vec(), false);
        let (lo, hi) = (Vec3::new(-1.03, 0.0, -0.61), Vec3::new(0.77, 1.1, 1.24));
        let mut s = Scratch::default();
        let by_tris = build_tile(&cfg, &d, 0, 0, &[&floor, &Source::new(crate::mesh::tests::cuboid(lo, hi), true)], &mut s);
        let by_box = build_tile(&cfg, &d, 0, 0, &[&floor, &Source::aligned_box(lo, hi)], &mut s);
        assert_eq!(by_tris.rects, by_box.rects);
    }

    #[test]
    fn wall_tops_leave_no_specks() {
        let cfg = Config { bounds_min: Vec3::new(-3.0, -1.0, -3.0), bounds_max: Vec3::new(3.0, 4.0, 3.0), tile_cells: 48, ..Config::default() };
        let d = Derived::of(&cfg);
        let floor = Source::new(quad(-3.0, -3.0, 3.0, 3.0, 0.0).to_vec(), false);
        // A T of 0.4 m walls, 2 m tall: their tops are too narrow, even where they meet.
        let a = Source::new(crate::mesh::tests::cuboid(Vec3::new(-3.0, 0.0, -0.2), Vec3::new(3.0, 2.0, 0.2)), true);
        let b = Source::new(crate::mesh::tests::cuboid(Vec3::new(-0.2, 0.0, 0.2), Vec3::new(0.2, 2.0, 3.0)), true);
        let t = build_tile(&cfg, &d, 0, 0, &[&floor, &a, &b], &mut Scratch::default());
        assert!(!t.rects.is_empty());
        assert!(t.rects.iter().all(|r| r.h.iter().all(|h| *h < 1.0)), "nothing on the walls: {:?}", t.rects.iter().filter(|r| r.h[0] > 1.0).collect::<Vec<_>>());
    }

    #[test]
    fn flat_floor_is_one_rectangle_eroded_by_the_radius() {
        let cfg = Config { bounds_min: Vec3::new(0.0, -1.0, 0.0), bounds_max: Vec3::new(4.8, 3.0, 4.8), tile_cells: 32, ..Config::default() };
        let d = Derived::of(&cfg);
        let floor = Source::new(quad(0.0, 0.0, 4.8, 4.8, 0.0).to_vec(), false);
        let t = build_tile(&cfg, &d, 0, 0, &[&floor], &mut Scratch::default());
        assert_eq!(t.rects.len(), 1, "{:?}", t.rects);
        let r = &t.rects[0];
        // Radius 0.3 m at 0.15 m cells: two columns eroded from each side.
        assert_eq!((r.x0, r.z0, r.x1, r.z1), (2, 2, 29, 29));
        assert!(r.h.iter().all(|h| h.abs() < 1.0e-5), "{:?}", r.h);
        assert!(t.errors.is_empty());
    }
}
