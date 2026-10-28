//! Tier-2 booleans: union, difference and intersection of closed shapes, exactly.
//!
//! **How:** every input triangle is snapped to one integer grid for the whole CSG tree (at most
//! ±2^19 steps across the tree's bounds: a few micrometres for a prop) and becomes a
//! *plane-based* polygon: a support plane and one plane per edge, with every vertex the
//! intersection of three planes ([`crate::exact`]). The booleans are BSP clipping (the classic
//! csg.js scheme: clip each solid by the other's tree, invert, clip again), with every
//! classification an exact orientation predicate, so coplanar faces (two boxes sharing a face, a
//! cutter flush with a wall) are decided exactly and nested booleans never lose precision: a
//! split vertex is three planes from the input, never rounded coordinates. Only the final
//! conversion back to metres rounds.
//!
//! **Output:** split polygons meet with T-junctions; they are welded (points within 10^-6 grid
//! steps) and every edge that lacks its reverse gets the welded points lying on it inserted,
//! then each convex polygon is triangulated without slivers. The result is checked watertight
//! (every edge used once each way) when the inputs were closed; anything else is an error with
//! the count. Colours and bevel tags come from the input triangle each piece was cut from.
//!
//! **Bounds:** the input triangles, the polygons created by splits and the classifications are
//! all charged to a [`Budget`]; a hostile tree gets a structured [`CsgError`], never unbounded
//! work. BSP traversals use explicit stacks (no recursion depth to overflow).
//!
//! **Determinism (F2):** integer predicates, a float filter whose answers are proven, and
//! traversals in a fixed order: the same tree gives the same bits everywhere.

#![allow(
    clippy::disallowed_types,
    reason = "every HashMap here is lookup-only (get, entry, insert); the edge-balance map is iterated only to count and the sliver map only into a BTreeSet, so no iteration order reaches the output"
)]

use std::collections::HashMap;

use crate::exact::{self, Plane, COORD_MAX};
use crate::math3::{self, V3};
use crate::shape::{Rgba, Shape, TAG_FACE};

/// A boolean operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CsgOp {
    Union,
    Difference,
    Intersection,
}

impl CsgOp {
    pub fn name(self) -> &'static str {
        match self {
            CsgOp::Union => "union",
            CsgOp::Difference => "difference",
            CsgOp::Intersection => "intersection",
        }
    }
}

/// A CSG tree: shapes (in metres) combined by operations. A difference subtracts every later
/// child from the first; union and intersection fold over all children.
#[derive(Clone, Debug)]
pub enum CsgTree {
    Leaf(Shape),
    Op(CsgOp, Vec<CsgTree>),
}

/// Why a boolean failed (the recipe layer adds the path).
#[derive(Clone, Debug, PartialEq)]
pub struct CsgError {
    pub kind: CsgErrorKind,
    pub message: String,
    pub measured: Option<f64>,
    pub limit: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CsgErrorKind {
    /// An operand isn't a closed solid, or the result is empty.
    Geometry,
    /// The work or polygon budget ran out.
    Limit,
    /// The result failed its watertightness check.
    InvalidMesh,
}

impl CsgError {
    fn new(kind: CsgErrorKind, message: impl Into<String>) -> CsgError {
        CsgError { kind, message: message.into(), measured: None, limit: None }
    }
    fn limit(message: impl Into<String>, measured: f64, limit: f64) -> CsgError {
        CsgError { kind: CsgErrorKind::Limit, message: message.into(), measured: Some(measured), limit: Some(limit) }
    }
}

impl std::fmt::Display for CsgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Input triangles over all operands of one tree.
pub const MAX_INPUT_TRIANGLES: usize = 24_000;
/// Polygons alive at once (each split makes two).
pub const MAX_POLYGONS: usize = 200_000;
/// Most work units one evaluation may spend on its own ([`crate::work`]: classifications weighed
/// by [`crate::work::CSG_CLASSIFY`], exact predicates by [`crate::work::CSG_EXACT`]); a
/// recipe's booleans get what is left of their build's budget.
pub const MAX_WORK: u64 = crate::work::MAX_BUILD;
/// Deepest nesting of operations.
pub const MAX_DEPTH: usize = 8;

/// What one evaluation may spend; counters are charged as it runs.
#[derive(Debug)]
pub struct Budget {
    /// Work units spent ([`crate::work`]).
    pub work: u64,
    pub max_work: u64,
    pub polygons: usize,
    pub max_polygons: usize,
    /// [`exact::exact_calls`] as last charged (when the budget was made, at first).
    pub exact_seen: u64,
    /// Exact predicates charged.
    pub exact: u64,
    /// Operand triangles that vanished on the grid.
    pub dropped: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Budget { work: 0, max_work: MAX_WORK, polygons: 0, max_polygons: MAX_POLYGONS, exact_seen: exact::exact_calls(), exact: 0, dropped: 0 }
    }
}

impl Budget {
    /// Charges `classifications` and the exact predicates taken since the last charge.
    fn charge(&mut self, classifications: u64) -> Result<(), CsgError> {
        let now = exact::exact_calls();
        let ex = now.saturating_sub(self.exact_seen);
        self.exact_seen = now;
        self.exact += ex;
        self.work += classifications * crate::work::CSG_CLASSIFY + ex * crate::work::CSG_EXACT;
        if self.work > self.max_work {
            return Err(CsgError::limit(
                format!("the boolean needs more than {} work units (classifications and exact predicates); use fewer or simpler operands (fewer sides, no subdivision)", self.max_work),
                self.work as f64,
                self.max_work as f64,
            ));
        }
        if crate::work::cancelled() {
            return Err(CsgError::limit(crate::work::CANCELLED, self.work as f64, self.max_work as f64));
        }
        Ok(())
    }
    fn grow(&mut self, n: usize) -> Result<(), CsgError> {
        self.polygons += n;
        if self.polygons > self.max_polygons {
            return Err(CsgError::limit(
                format!("the boolean splits into more than {} polygons; use fewer or simpler operands", self.max_polygons),
                self.polygons as f64,
                self.max_polygons as f64,
            ));
        }
        Ok(())
    }
}

/// What an evaluation did (for reports and perf numbers).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CsgStats {
    pub input_triangles: usize,
    pub output_triangles: usize,
    pub work: u64,
    pub polygons: usize,
    /// Edges that needed a T-junction fix.
    pub t_junctions: usize,
    /// Edges where the surface meets itself (used twice each way; still closed).
    pub non_manifold_edges: usize,
    /// Grid step in metres.
    pub step: f64,
    /// Operand triangles that vanished on the grid (smaller than a step), and pieces of the
    /// result dropped as slivers (thinner than [`MIN_THICKNESS_M`]).
    pub dropped_triangles: usize,
    pub slivers: usize,
}

// ---------------------------------------------------------------------------------------------
// Plane-based polygons

/// A polygon vertex: the plane of the edge leaving it, and its position (grid units, from the
/// exact intersection; see [`exact::orient`] for its error bound).
#[derive(Clone, Debug)]
struct Vtx {
    edge: Plane,
    p: [f64; 3],
}

/// A convex polygon: vertex `i` is `support ∩ v[i-1].edge ∩ v[i].edge`.
#[derive(Clone, Debug)]
struct Poly {
    support: Plane,
    v: Vec<Vtx>,
    /// The input triangle it was cut from.
    src: u32,
    /// A bounding sphere of the vertices (centre, radius; grid units): most splits are decided
    /// by it alone.
    centre: [f64; 3],
    radius: f64,
}

impl Poly {
    fn new(support: Plane, v: Vec<Vtx>, src: u32) -> Poly {
        let n = v.len() as f64;
        let mut c = [0.0f64; 3];
        for x in &v {
            for (ck, pk) in c.iter_mut().zip(x.p) {
                *ck += pk / n;
            }
        }
        let r = v.iter().map(|x| dot64(sub64(x.p, c), sub64(x.p, c)).sqrt()).fold(0.0f64, f64::max);
        Poly { support, v, src, centre: c, radius: r }
    }

    fn flip(&mut self) {
        self.support = self.support.flip();
        let n = self.v.len();
        // Reversed order: vertex j is old vertex n-1-j; the edge leaving it is the old edge
        // between old vertices n-2-j and n-1-j.
        let old = std::mem::take(&mut self.v);
        self.v = (0..n).map(|j| Vtx { edge: old[(2 * n - 2 - j) % n].edge, p: old[n - 1 - j].p }).collect();
    }

    fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        let mut lo = [f64::MAX; 3];
        let mut hi = [f64::MIN; 3];
        for v in &self.v {
            for k in 0..3 {
                lo[k] = lo[k].min(v.p[k]);
                hi[k] = hi[k].max(v.p[k]);
            }
        }
        (lo, hi)
    }
}

/// Splits `poly` by `plane` into the four lists (coplanar ones by facing).
fn split(plane: &Plane, poly: Poly, cf: &mut Vec<Poly>, cb: &mut Vec<Poly>, f: &mut Vec<Poly>, b: &mut Vec<Poly>, budget: &mut Budget) -> Result<(), CsgError> {
    // The bounding sphere clear of the plane decides at once (with margins far above the float
    // error of the centre, the radius and the vertices).
    let nf = plane.n.map(|x| x as f64);
    let nlen = (nf[0] * nf[0] + nf[1] * nf[1] + nf[2] * nf[2]).sqrt();
    let c = poly.centre;
    let dist = nf[0] * c[0] + nf[1] * c[1] + nf[2] * c[2] + plane.d as f64;
    let m = c[0].abs().max(c[1].abs()).max(c[2].abs()) + poly.radius;
    let margin = nlen * (poly.radius * (1.0 + 1e-9) + 1e-6) + ((nf[0].abs() + nf[1].abs() + nf[2].abs()) * (m + 1.0) + (plane.d as f64).abs()) * 1e-12;
    budget.charge(1)?;
    if dist > margin {
        f.push(poly);
        return Ok(());
    }
    if dist < -margin {
        b.push(poly);
        return Ok(());
    }
    let n = poly.v.len();
    budget.charge(n as u64)?;
    let mut signs = Vec::with_capacity(n);
    let (mut pos, mut neg) = (0, 0);
    for i in 0..n {
        let prev = &poly.v[(i + n - 1) % n].edge;
        let s = exact::orient([&poly.support, prev, &poly.v[i].edge], &poly.v[i].p, plane);
        pos += (s > 0) as usize;
        neg += (s < 0) as usize;
        signs.push(s);
    }
    if pos == 0 && neg == 0 {
        if poly.support.facing(plane) {
            cf.push(poly);
        } else {
            cb.push(poly);
        }
        return Ok(());
    }
    if neg == 0 {
        f.push(poly);
        return Ok(());
    }
    if pos == 0 {
        b.push(poly);
        return Ok(());
    }
    budget.grow(1)?;
    // Crossing points, one per edge whose ends are strictly on opposite sides.
    let mut cross: Vec<Option<[f64; 3]>> = vec![None; n];
    for i in 0..n {
        let j = (i + 1) % n;
        if signs[i] * signs[j] < 0 {
            let h = exact::intersect(&poly.support, &poly.v[i].edge, plane)
                .ok_or_else(|| CsgError::new(CsgErrorKind::Geometry, "a split edge is parallel to its splitting plane (degenerate input)"))?;
            cross[i] = Some(h.approx());
        }
    }
    for (side, out) in [(1, &mut *f), (-1, &mut *b)] {
        let mut v = Vec::with_capacity(n + 2);
        for i in 0..n {
            let j = (i + 1) % n;
            let (a, bb) = (side * signs[i], side * signs[j]);
            let vi = &poly.v[i];
            if a >= 0 {
                v.push(Vtx { edge: if a == 0 && bb < 0 { *plane } else { vi.edge }, p: vi.p });
            }
            if a > 0 && bb < 0 {
                v.push(Vtx { edge: *plane, p: cross[i].expect("crossing") });
            } else if a < 0 && bb > 0 {
                v.push(Vtx { edge: vi.edge, p: cross[i].expect("crossing") });
            }
        }
        if v.len() >= 3 {
            out.push(Poly::new(poly.support, v, poly.src));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// BSP trees (arena, explicit stacks)

const NONE: u32 = u32::MAX;

#[derive(Debug)]
struct Node {
    plane: Option<Plane>,
    front: u32,
    back: u32,
    polys: Vec<Poly>,
}

impl Node {
    fn empty() -> Node {
        Node { plane: None, front: NONE, back: NONE, polys: Vec::new() }
    }
}

/// A splitting plane for a node: of a few candidates spread through the list, the one that
/// splits fewest polygons and balances front and back best (bounding spheres only: a cheap
/// estimate; any choice is correct, a good one keeps the tree shallow).
fn choose_plane(ps: &[Poly], budget: &mut Budget) -> Result<Plane, CsgError> {
    const CANDIDATES: usize = 5;
    if ps.len() < 8 {
        return Ok(ps[0].support);
    }
    let sample: Vec<usize> = (0..ps.len()).step_by((ps.len() / 64).max(1)).collect();
    let mut best = (u64::MAX, ps[0].support);
    for c in 0..CANDIDATES {
        let plane = ps[c * (ps.len() - 1) / (CANDIDATES - 1)].support;
        budget.charge(sample.len() as u64)?;
        let nf = plane.n.map(|x| x as f64);
        let nlen = (nf[0] * nf[0] + nf[1] * nf[1] + nf[2] * nf[2]).sqrt();
        let (mut front, mut back, mut span) = (0u64, 0u64, 0u64);
        for &i in &sample {
            let q = &ps[i];
            let d = (nf[0] * q.centre[0] + nf[1] * q.centre[1] + nf[2] * q.centre[2] + plane.d as f64) / nlen;
            if d > q.radius {
                front += 1;
            } else if d < -q.radius {
                back += 1;
            } else {
                span += 1;
            }
        }
        let score = 4 * span + front.abs_diff(back);
        if score < best.0 {
            best = (score, plane);
        }
    }
    Ok(best.1)
}

#[derive(Debug)]
struct Bsp {
    nodes: Vec<Node>,
    /// Inverted: the tree's solid is the complement of a bounded one.
    inverted: bool,
    /// Bounds of every polygon in the tree (grid units).
    lo: [f64; 3],
    hi: [f64; 3],
}

impl Bsp {
    fn new(polys: Vec<Poly>, budget: &mut Budget) -> Result<Bsp, CsgError> {
        let mut lo = [f64::MAX; 3];
        let mut hi = [f64::MIN; 3];
        for p in &polys {
            let (a, b) = p.bounds();
            for k in 0..3 {
                lo[k] = lo[k].min(a[k]);
                hi[k] = hi[k].max(b[k]);
            }
        }
        let mut t = Bsp { nodes: vec![Node::empty()], inverted: false, lo, hi };
        t.insert(polys, budget)?;
        Ok(t)
    }

    fn child(&mut self, ni: usize, front: bool) -> usize {
        let existing = if front { self.nodes[ni].front } else { self.nodes[ni].back };
        if existing != NONE {
            return existing as usize;
        }
        self.nodes.push(Node::empty());
        let id = (self.nodes.len() - 1) as u32;
        if front {
            self.nodes[ni].front = id;
        } else {
            self.nodes[ni].back = id;
        }
        id as usize
    }

    fn insert(&mut self, polys: Vec<Poly>, budget: &mut Budget) -> Result<(), CsgError> {
        let mut stack = vec![(0usize, polys)];
        while let Some((ni, ps)) = stack.pop() {
            if ps.is_empty() {
                continue;
            }
            if self.nodes[ni].plane.is_none() {
                self.nodes[ni].plane = Some(choose_plane(&ps, budget)?);
            }
            let plane = self.nodes[ni].plane.expect("set");
            let (mut co, mut f, mut b) = (Vec::new(), Vec::new(), Vec::new());
            let mut cb = Vec::new();
            for p in ps {
                split(&plane, p, &mut co, &mut cb, &mut f, &mut b, budget)?;
            }
            co.append(&mut cb);
            self.nodes[ni].polys.extend(co);
            if !b.is_empty() {
                let bi = self.child(ni, false);
                stack.push((bi, b));
            }
            if !f.is_empty() {
                let fi = self.child(ni, true);
                stack.push((fi, f));
            }
        }
        Ok(())
    }

    /// The parts of `polys` outside this tree's solid.
    fn clip(&self, polys: Vec<Poly>, budget: &mut Budget) -> Result<Vec<Poly>, CsgError> {
        let mut out = Vec::new();
        if self.nodes[0].plane.is_none() {
            return Ok(polys);
        }
        let mut stack = vec![(0usize, polys)];
        while let Some((ni, ps)) = stack.pop() {
            let node = &self.nodes[ni];
            let Some(plane) = node.plane else {
                out.extend(ps);
                continue;
            };
            let (mut f, mut b) = (Vec::new(), Vec::new());
            let (mut cf, mut cb) = (Vec::new(), Vec::new());
            for p in ps {
                split(&plane, p, &mut cf, &mut cb, &mut f, &mut b, budget)?;
            }
            f.append(&mut cf);
            b.append(&mut cb);
            if node.back != NONE {
                stack.push((node.back as usize, b));
            } else {
                budget.polygons = budget.polygons.saturating_sub(b.len());
            }
            if node.front != NONE {
                stack.push((node.front as usize, f));
            } else {
                out.extend(f);
            }
        }
        Ok(out)
    }

    /// Removes every polygon inside `other`'s solid. Polygons clear of `other`'s bounds skip
    /// the traversal: outside a bounded solid, inside an inverted one.
    fn clip_to(&mut self, other: &Bsp, budget: &mut Budget) -> Result<(), CsgError> {
        for ni in 0..self.nodes.len() {
            let polys = std::mem::take(&mut self.nodes[ni].polys);
            let (mut near, mut far) = (Vec::new(), Vec::new());
            for p in polys {
                let (lo, hi) = p.bounds();
                let clear = (0..3).any(|k| hi[k] < other.lo[k] - 1.0 || lo[k] > other.hi[k] + 1.0);
                if clear {
                    far.push(p);
                } else {
                    near.push(p);
                }
            }
            let mut kept = other.clip(near, budget)?;
            if other.inverted {
                budget.polygons = budget.polygons.saturating_sub(far.len());
            } else {
                kept.extend(far);
            }
            self.nodes[ni].polys = kept;
        }
        Ok(())
    }

    fn invert(&mut self) {
        for n in &mut self.nodes {
            n.polys.iter_mut().for_each(Poly::flip);
            n.plane = n.plane.map(Plane::flip);
            std::mem::swap(&mut n.front, &mut n.back);
        }
        self.inverted = !self.inverted;
    }

    fn polygons(self) -> Vec<Poly> {
        self.nodes.into_iter().flat_map(|n| n.polys).collect()
    }
}

fn apply_op(op: CsgOp, a: Vec<Poly>, b: Vec<Poly>, budget: &mut Budget) -> Result<Vec<Poly>, CsgError> {
    if b.is_empty() {
        return Ok(if op == CsgOp::Intersection { Vec::new() } else { a });
    }
    if a.is_empty() {
        return Ok(if op == CsgOp::Union { b } else { Vec::new() });
    }
    let mut a = Bsp::new(a, budget)?;
    let mut b = Bsp::new(b, budget)?;
    let flip_all = |mut v: Vec<Poly>| {
        v.iter_mut().for_each(Poly::flip);
        v
    };
    Ok(match op {
        CsgOp::Union => {
            a.clip_to(&b, budget)?;
            b.clip_to(&a, budget)?;
            b.invert();
            b.clip_to(&a, budget)?;
            b.invert();
            let mut out = a.polygons();
            out.extend(b.polygons());
            out
        }
        CsgOp::Difference => {
            a.invert();
            a.clip_to(&b, budget)?;
            b.clip_to(&a, budget)?;
            b.invert();
            b.clip_to(&a, budget)?;
            b.invert();
            let mut out = flip_all(a.polygons());
            out.extend(flip_all(b.polygons()));
            out
        }
        CsgOp::Intersection => {
            a.invert();
            b.clip_to(&a, budget)?;
            b.invert();
            a.clip_to(&b, budget)?;
            b.clip_to(&a, budget)?;
            let mut out = flip_all(a.polygons());
            out.extend(flip_all(b.polygons()));
            out
        }
    })
}

// ---------------------------------------------------------------------------------------------
// Input: shapes to plane-based polygons on one grid

/// An input triangle as it was (metres), for colours and tags of the pieces cut from it.
#[derive(Clone, Debug)]
struct Src {
    pos: [V3; 3],
    col: [Rgba; 3],
    tag: u8,
    leaf: u32,
}

/// The grid: metres = `origin + grid · step`.
#[derive(Clone, Copy, Debug)]
struct Grid {
    origin: [f64; 3],
    step: f64,
}

impl Grid {
    fn snap(&self, p: V3) -> [i64; 3] {
        [0, 1, 2].map(|k| crate::dmath::round_f64((p[k] as f64 - self.origin[k]) / self.step) as i64)
    }
    fn metres(&self, g: [f64; 3]) -> V3 {
        [0, 1, 2].map(|k| (self.origin[k] + g[k] * self.step) as f32)
    }
}

fn leaves<'a>(t: &'a CsgTree, out: &mut Vec<&'a Shape>, depth: usize) -> Result<(), CsgError> {
    if depth > MAX_DEPTH {
        return Err(CsgError::limit(format!("booleans nest deeper than {MAX_DEPTH}"), depth as f64, MAX_DEPTH as f64));
    }
    match t {
        CsgTree::Leaf(s) => out.push(s),
        CsgTree::Op(_, kids) => {
            for k in kids {
                leaves(k, out, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn leaf_polys(shape: &Shape, leaf: u32, grid: &Grid, srcs: &mut Vec<Src>, budget: &mut Budget) -> Vec<Poly> {
    let mut out = Vec::with_capacity(shape.tris.len());
    for (ti, t) in shape.tris.iter().enumerate() {
        let p = t.map(|i| shape.positions[i as usize]);
        let g = p.map(|q| grid.snap(q));
        let Some(support) = Plane::through(g[0], g[1], g[2]) else {
            budget.dropped += 1;
            continue;
        };
        let axis = support.dominant_axis();
        let edges: Option<Vec<Plane>> = (0..3).map(|k| Plane::edge(g[k], g[(k + 1) % 3], axis)).collect();
        let Some(edges) = edges else {
            budget.dropped += 1;
            continue;
        };
        srcs.push(Src { pos: p, col: t.map(|i| shape.colors[i as usize]), tag: shape.tags.get(ti).copied().unwrap_or(TAG_FACE), leaf });
        let src = (srcs.len() - 1) as u32;
        let v = (0..3).map(|k| Vtx { edge: edges[k], p: g[k].map(|c| c as f64) }).collect();
        out.push(Poly::new(support, v, src));
    }
    budget.polygons += out.len();
    out
}

/// Evaluates a tree. The result's per-triangle leaf index (DFS order of the tree's leaves) is
/// in the second value, for edge detection.
pub fn evaluate(tree: &CsgTree, budget: &mut Budget) -> Result<(Shape, Vec<u32>, CsgStats), CsgError> {
    let (mut ses, input) = Session::new(tree)?;
    let polys = eval_node(tree, &ses.grid, &mut ses.srcs, &mut ses.leaf_id, budget)?;
    let polygons = budget.polygons;
    let (shape, leaf, cl) = to_shape(&polys, &ses.grid, &ses.srcs, budget)?;
    let stats = CsgStats {
        input_triangles: input,
        output_triangles: shape.tris.len(),
        work: budget.work,
        polygons,
        t_junctions: cl.t_junctions,
        non_manifold_edges: cl.non_manifold_edges,
        step: ses.grid.step,
        dropped_triangles: budget.dropped,
        slivers: cl.slivers,
    };
    Ok((shape, leaf, stats))
}

/// One evaluation's grid and input triangles.
struct Session {
    grid: Grid,
    srcs: Vec<Src>,
    leaf_id: u32,
}

impl Session {
    /// Checks the tree's leaves (closed, within limits) and lays one grid over them.
    fn new(tree: &CsgTree) -> Result<(Session, usize), CsgError> {
        let mut ls = Vec::new();
        leaves(tree, &mut ls, 0)?;
        let input: usize = ls.iter().map(|s| s.tris.len()).sum();
        if input > MAX_INPUT_TRIANGLES {
            return Err(CsgError::limit(format!("the booleans' operands have {input} triangles; at most {MAX_INPUT_TRIANGLES}"), input as f64, MAX_INPUT_TRIANGLES as f64));
        }
        for (i, s) in ls.iter().enumerate() {
            if s.tris.is_empty() {
                return Err(CsgError::new(CsgErrorKind::Geometry, format!("operand {i} has no triangles")));
            }
            let open = s.open_edge_count();
            if open > 0 {
                return Err(CsgError {
                    kind: CsgErrorKind::Geometry,
                    message: format!("operand {i} is not a closed solid ({open} open edges): booleans need closed shapes (no plane, no lathe or sweep without caps)"),
                    measured: Some(open as f64),
                    limit: Some(0.0),
                });
            }
        }
        // One grid for the whole tree, centred on its bounds.
        let mut lo = [f64::MAX; 3];
        let mut hi = [f64::MIN; 3];
        for s in &ls {
            for p in &s.positions {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k] as f64);
                    hi[k] = hi[k].max(p[k] as f64);
                }
            }
        }
        let half = (0..3).map(|k| (hi[k] - lo[k]) * 0.5).fold(0.0f64, f64::max).max(1e-3);
        // Inputs use half the coordinate range: bevel tubes reach a little beyond.
        let step = half / (COORD_MAX / 2) as f64;
        let grid = Grid { origin: [0, 1, 2].map(|k| (lo[k] + hi[k]) * 0.5), step };
        Ok((Session { grid, srcs: Vec::new(), leaf_id: 0 }, input))
    }

    /// A shape added on this session's grid as polygons, when every point fits the grid.
    fn add(&mut self, shape: &Shape, budget: &mut Budget) -> Option<Vec<Poly>> {
        let fits = shape.positions.iter().all(|p| self.grid.snap(*p).iter().all(|c| c.abs() < COORD_MAX));
        if !fits {
            return None;
        }
        let id = self.leaf_id;
        self.leaf_id += 1;
        Some(leaf_polys(shape, id, &self.grid, &mut self.srcs, budget))
    }
}

fn eval_node(t: &CsgTree, grid: &Grid, srcs: &mut Vec<Src>, leaf_id: &mut u32, budget: &mut Budget) -> Result<Vec<Poly>, CsgError> {
    match t {
        CsgTree::Leaf(s) => {
            let id = *leaf_id;
            *leaf_id += 1;
            let before = budget.dropped;
            let polys = leaf_polys(s, id, grid, srcs, budget);
            // An operand that lost triangles on the grid and is only a few steps across: its
            // surface no longer closes (a 1 cm cut on a 10 km slab would just vanish).
            let small = || {
                let (lo, hi) = s.positions.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| (math3::vmin(lo, *p), math3::vmax(hi, *p)));
                (0..3).any(|k| ((hi[k] - lo[k]) as f64) < 4.0 * grid.step)
            };
            if polys.is_empty() || (budget.dropped > before && small()) {
                return Err(CsgError::new(
                    CsgErrorKind::Geometry,
                    format!("operand {id} is smaller than the grid this boolean's size allows (a step of {:.3} mm): scale it up, or cut the large part separately", grid.step * 1000.0),
                ));
            }
            Ok(polys)
        }
        CsgTree::Op(CsgOp::Union, kids) => {
            // The first plus the rest: the rest united in groups that overlap (disjoint groups
            // are just their surfaces together), then all added with one tree of the first.
            let mut it = kids.iter();
            let Some(first) = it.next() else { return Ok(Vec::new()) };
            let a = eval_node(first, grid, srcs, leaf_id, budget)?;
            let mut parts: Vec<Vec<Poly>> = Vec::with_capacity(kids.len());
            for k in it {
                parts.push(eval_node(k, grid, srcs, leaf_id, budget)?);
            }
            multi_union(a, disjoint_groups(parts, budget)?, budget)
        }
        CsgTree::Op(CsgOp::Difference, kids) => {
            // The first minus the rest: cutters that overlap are united first; disjoint groups
            // are all cut with one tree of the first (a wall's windows: one pass, not one each).
            let mut it = kids.iter();
            let Some(first) = it.next() else { return Ok(Vec::new()) };
            let a = eval_node(first, grid, srcs, leaf_id, budget)?;
            let mut parts: Vec<Vec<Poly>> = Vec::with_capacity(kids.len());
            for k in it {
                parts.push(eval_node(k, grid, srcs, leaf_id, budget)?);
            }
            multi_difference(a, disjoint_groups(parts, budget)?, budget)
        }
        CsgTree::Op(op, kids) => {
            let mut it = kids.iter();
            let Some(first) = it.next() else { return Ok(Vec::new()) };
            let mut acc = eval_node(first, grid, srcs, leaf_id, budget)?;
            for k in it {
                let b = eval_node(k, grid, srcs, leaf_id, budget)?;
                acc = apply_op(*op, acc, b, budget)?;
            }
            Ok(acc)
        }
    }
}

/// Groups operands whose bounds overlap (transitively) and unites each group: the groups are
/// disjoint solids, in the order of their first member.
fn disjoint_groups(parts: Vec<Vec<Poly>>, budget: &mut Budget) -> Result<Vec<Vec<Poly>>, CsgError> {
    let boxes: Vec<([f64; 3], [f64; 3])> = parts.iter().map(|p| polys_bounds(p)).collect();
    let n = parts.len();
    budget.charge((n * n / 8) as u64)?;
    let mut group: Vec<usize> = (0..n).collect();
    fn root(g: &mut [usize], mut i: usize) -> usize {
        while g[i] != i {
            g[i] = g[g[i]];
            i = g[i];
        }
        i
    }
    for i in 0..n {
        for j in i + 1..n {
            let (a, b) = (&boxes[i], &boxes[j]);
            if (0..3).all(|k| a.0[k] <= b.1[k] + 1.0 && b.0[k] <= a.1[k] + 1.0) {
                let (ri, rj) = (root(&mut group, i), root(&mut group, j));
                if ri != rj {
                    group[ri.max(rj)] = ri.min(rj);
                }
            }
        }
    }
    let mut acc_of: Vec<Option<Vec<Poly>>> = (0..n).map(|_| None).collect();
    for (i, p) in parts.into_iter().enumerate() {
        let r = root(&mut group, i);
        acc_of[r] = Some(match acc_of[r].take() {
            None => p,
            Some(acc) => apply_op(CsgOp::Union, acc, p, budget)?,
        });
    }
    Ok(acc_of.into_iter().flatten().filter(|g| !g.is_empty()).collect())
}

/// `a` united with disjoint solids `bs`, with one tree of `a` (as [`multi_difference`]).
fn multi_union(a: Vec<Poly>, bs: Vec<Vec<Poly>>, budget: &mut Budget) -> Result<Vec<Poly>, CsgError> {
    if bs.is_empty() {
        return Ok(a);
    }
    if a.is_empty() {
        return Ok(bs.into_iter().flatten().collect());
    }
    let mut a = Bsp::new(a, budget)?;
    let mut trees = Vec::with_capacity(bs.len());
    for b in bs {
        trees.push(Bsp::new(b, budget)?);
    }
    for b in &trees {
        a.clip_to(b, budget)?;
    }
    for b in &mut trees {
        b.clip_to(&a, budget)?;
        b.invert();
        b.clip_to(&a, budget)?;
        b.invert();
    }
    let mut out: Vec<Poly> = a.polygons();
    for b in trees {
        out.extend(b.polygons());
    }
    Ok(out)
}

/// `a` minus disjoint solids `bs`, with one tree of `a`: each cutter's tree only touches the
/// pieces of `a` near it, and `a`'s planes (all its tree classifies by) don't change as pieces
/// are clipped, so cutting by each in turn is the same as cutting by their union.
fn multi_difference(a: Vec<Poly>, bs: Vec<Vec<Poly>>, budget: &mut Budget) -> Result<Vec<Poly>, CsgError> {
    if a.is_empty() || bs.is_empty() {
        return Ok(a);
    }
    let mut a = Bsp::new(a, budget)?;
    a.invert();
    let mut trees = Vec::with_capacity(bs.len());
    for b in bs {
        trees.push(Bsp::new(b, budget)?);
    }
    for b in &trees {
        a.clip_to(b, budget)?;
    }
    for b in &mut trees {
        b.clip_to(&a, budget)?;
        b.invert();
        b.clip_to(&a, budget)?;
        b.invert();
    }
    let mut out: Vec<Poly> = a.polygons();
    for b in trees {
        out.extend(b.polygons());
    }
    out.iter_mut().for_each(Poly::flip);
    Ok(out)
}

/// Bounds of a set of polygons (grid units).
fn polys_bounds(p: &[Poly]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for q in p {
        let (a, b) = q.bounds();
        for k in 0..3 {
            lo[k] = lo[k].min(a[k]);
            hi[k] = hi[k].max(b[k]);
        }
    }
    (lo, hi)
}

// ---------------------------------------------------------------------------------------------
// Output: weld, fix T-junctions, triangulate

/// Candidate distance (grid steps, per axis) for welding and T-junction search: points this
/// close are tested *exactly*; float noise in a vertex is below 10^-9 steps.
const NEAR: f64 = 1e-5;

/// A welded point: its position and the three planes it was first made from.
struct Point {
    p: [f64; 3],
    planes: [Plane; 3],
}

/// Whether point `a` lies exactly on plane `q`.
fn on_plane(a: &Point, q: &Plane) -> bool {
    exact::orient([&a.planes[0], &a.planes[1], &a.planes[2]], &a.p, q) == 0
}

/// Welds exactly equal points: candidates within [`NEAR`] (a grid of cells), confirmed by
/// testing the new point against the three planes of the old one.
struct Welder {
    cells: HashMap<[i64; 3], Vec<u32>>,
    points: Vec<Point>,
}

impl Welder {
    fn id(&mut self, p: [f64; 3], planes: [Plane; 3], budget: &mut Budget) -> Result<u32, CsgError> {
        let c = p.map(|v| crate::dmath::floor_f64(v / NEAR) as i64);
        let me = Point { p, planes };
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(list) = self.cells.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                        for &i in list {
                            let q = &self.points[i as usize];
                            if (0..3).all(|k| (q.p[k] - p[k]).abs() <= NEAR) {
                                budget.charge(3)?;
                                if q.planes.iter().all(|pl| on_plane(&me, pl)) {
                                    return Ok(i);
                                }
                            }
                        }
                    }
                }
            }
        }
        self.points.push(me);
        let id = (self.points.len() - 1) as u32;
        self.cells.entry(c).or_default().push(id);
        Ok(id)
    }
}

fn sub64(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot64(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross64(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// A polygon's boundary as welded points, each with the plane of the edge leaving it.
#[derive(Clone, Copy, Debug)]
struct LoopV {
    id: u32,
    edge: Plane,
}

struct Loop {
    v: Vec<LoopV>,
    support: Plane,
    poly: usize,
}

/// A static k-d tree over some points (ids into a point list), for box queries.
struct KdTree {
    /// Point ids, ordered so each node's points are a contiguous range.
    ids: Vec<u32>,
    /// Nodes: (start, end, split axis, split value, left child, right child); leaves have no
    /// children (`u32::MAX`).
    nodes: Vec<(u32, u32, u8, f64, u32, u32)>,
}

impl KdTree {
    fn new(ids: &[u32], pts: &[Point]) -> KdTree {
        let mut t = KdTree { ids: ids.to_vec(), nodes: Vec::new() };
        let mut stack = vec![(0usize, ids.len(), usize::MAX, false)];
        while let Some((start, end, parent, left)) = stack.pop() {
            let me = t.nodes.len() as u32;
            if parent != usize::MAX {
                if left {
                    t.nodes[parent].4 = me;
                } else {
                    t.nodes[parent].5 = me;
                }
            }
            if end - start <= 8 {
                t.nodes.push((start as u32, end as u32, 0, 0.0, u32::MAX, u32::MAX));
                continue;
            }
            let mut lo = [f64::MAX; 3];
            let mut hi = [f64::MIN; 3];
            for &i in &t.ids[start..end] {
                let p = pts[i as usize].p;
                for c in 0..3 {
                    lo[c] = lo[c].min(p[c]);
                    hi[c] = hi[c].max(p[c]);
                }
            }
            let axis = (0..3).max_by(|&x, &y| (hi[x] - lo[x]).total_cmp(&(hi[y] - lo[y])).then(y.cmp(&x))).expect("three");
            let slice = &mut t.ids[start..end];
            slice.sort_by(|&x, &y| pts[x as usize].p[axis].total_cmp(&pts[y as usize].p[axis]).then(x.cmp(&y)));
            let mid = start + (end - start) / 2;
            let split = pts[t.ids[mid] as usize].p[axis];
            t.nodes.push((start as u32, end as u32, axis as u8, split, u32::MAX, u32::MAX));
            stack.push((mid, end, me as usize, false));
            stack.push((start, mid, me as usize, true));
        }
        t
    }

    /// Points in the box go to `out`; returns the nodes visited.
    fn query(&self, lo: [f64; 3], hi: [f64; 3], pts: &[Point], out: &mut Vec<u32>) -> usize {
        if self.nodes.is_empty() {
            return 0;
        }
        let mut visited = 0;
        let mut stack = vec![0u32];
        while let Some(ni) = stack.pop() {
            visited += 1;
            let (start, end, axis, split, l, r) = self.nodes[ni as usize];
            if l == u32::MAX {
                for &i in &self.ids[start as usize..end as usize] {
                    let p = pts[i as usize].p;
                    if (0..3).all(|c| p[c] >= lo[c] && p[c] <= hi[c]) {
                        out.push(i);
                    }
                }
                continue;
            }
            let a = axis as usize;
            if lo[a] <= split {
                stack.push(l);
            }
            if hi[a] >= split {
                stack.push(r);
            }
        }
        visited
    }
}

/// Inserts, into each loop edge without a reverse, the welded points lying on it: candidates
/// near the segment are confirmed exactly (on the polygon's support plane and on the edge's
/// plane). Returns how many edges were split.
fn fix_t_junctions(loops: &mut [Loop], pts: &[Point], budget: &mut Budget) -> Result<usize, CsgError> {
    let mut count: HashMap<(u32, u32), u32> = HashMap::new();
    for l in loops.iter() {
        let n = l.v.len();
        for i in 0..n {
            *count.entry((l.v[i].id, l.v[(i + 1) % n].id)).or_default() += 1;
        }
    }
    let unmatched = |a: u32, b: u32| count.get(&(b, a)).copied().unwrap_or(0) != count.get(&(a, b)).copied().unwrap_or(0);
    let mut cand: Vec<u32> = Vec::new();
    let mut total_len = 0.0;
    let mut n_edges = 0usize;
    for l in loops.iter() {
        let n = l.v.len();
        for i in 0..n {
            let (a, b) = (l.v[i].id, l.v[(i + 1) % n].id);
            if unmatched(a, b) {
                cand.push(a);
                cand.push(b);
                let d = sub64(pts[b as usize].p, pts[a as usize].p);
                total_len += dot64(d, d).sqrt();
                n_edges += 1;
            }
        }
    }
    if n_edges == 0 {
        return Ok(0);
    }
    cand.sort_unstable();
    cand.dedup();
    let h = (total_len / n_edges as f64).max(1.0);
    let tree = KdTree::new(&cand, pts);
    let mut stamp: HashMap<u32, usize> = HashMap::new();
    let mut fixed = 0;
    let mut serial = 0usize;
    let mut found: Vec<u32> = Vec::new();
    for l in loops.iter_mut() {
        let n = l.v.len();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let (va, vb) = (l.v[i], l.v[(i + 1) % n]);
            out.push(va);
            if !unmatched(va.id, vb.id) {
                continue;
            }
            serial += 1;
            let (pa, pb) = (pts[va.id as usize].p, pts[vb.id as usize].p);
            let d = sub64(pb, pa);
            let len2 = dot64(d, d);
            if len2 <= 0.0 {
                continue;
            }
            let len = len2.sqrt();
            // The edge in pieces, each a box query: thin boxes along a diagonal edge.
            let pieces = ((len / (8.0 * h)) as usize).clamp(1, 64);
            let mut on: Vec<(f64, u32)> = Vec::new();
            for k in 0..pieces {
                let (t0, t1) = (k as f64 / pieces as f64, (k + 1) as f64 / pieces as f64);
                let (p0, p1) = ([0, 1, 2].map(|c| pa[c] + d[c] * t0), [0, 1, 2].map(|c| pa[c] + d[c] * t1));
                let lo = [0, 1, 2].map(|c| p0[c].min(p1[c]) - 2.0 * NEAR);
                let hi = [0, 1, 2].map(|c| p0[c].max(p1[c]) + 2.0 * NEAR);
                found.clear();
                let visited = tree.query(lo, hi, pts, &mut found);
                budget.charge(visited as u64 + 1)?;
                for &q in &found {
                    if q == va.id || q == vb.id || stamp.get(&q) == Some(&serial) {
                        continue;
                    }
                    stamp.insert(q, serial);
                    let pq = sub64(pts[q as usize].p, pa);
                    let t = dot64(pq, d) / len2;
                    if t <= 0.0 || t >= 1.0 {
                        continue;
                    }
                    let off = sub64(pq, [d[0] * t, d[1] * t, d[2] * t]);
                    if dot64(off, off).sqrt() > NEAR {
                        continue;
                    }
                    budget.charge(2)?;
                    let point = &pts[q as usize];
                    if on_plane(point, &l.support) && on_plane(point, &va.edge) {
                        on.push((t, q));
                    }
                }
            }
            if !on.is_empty() {
                on.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
                out.extend(on.iter().map(|&(_, q)| LoopV { id: q, edge: va.edge }));
                fixed += 1;
            }
        }
        l.v = out;
    }
    Ok(fixed)
}


/// Re-triangulates the pieces cut from one input triangle as one region (an outer boundary and
/// its holes), so a wall face pierced by windows becomes a few dozen triangles instead of the
/// strips the BSP splits leave. Its boundary keeps every point (T-junction points included), so
/// it still meets its neighbours exactly. `None` (use the pieces as they are) when the region
/// isn't simple enough: a pinched boundary or no clean ear.
/// Faces as point rings: an outer ring and its holes.
type Rings = Vec<(Vec<u32>, Vec<Vec<u32>>)>;

fn merge_group(loops: &[&Loop], pts: &[Point], normal: [f64; 3]) -> Option<Rings> {
    let mut count: HashMap<(u32, u32), i32> = HashMap::new();
    for l in loops {
        let n = l.v.len();
        for i in 0..n {
            *count.entry((l.v[i].id, l.v[(i + 1) % n].id)).or_default() += 1;
        }
    }
    // Boundary edges, in a fixed order (loop order).
    let mut next: HashMap<u32, u32> = HashMap::new();
    let mut order: Vec<u32> = Vec::new();
    for l in loops {
        let n = l.v.len();
        for i in 0..n {
            let (a, b) = (l.v[i].id, l.v[(i + 1) % n].id);
            if count.get(&(b, a)).copied().unwrap_or(0) == 0 {
                if count[&(a, b)] != 1 || next.insert(a, b).is_some() {
                    return None; // a pinch: two boundary edges leave one point
                }
                order.push(a);
            }
        }
    }
    // Chains into loops.
    let mut seen: HashMap<u32, bool> = HashMap::new();
    let mut rings: Vec<Vec<u32>> = Vec::new();
    for &start in &order {
        if seen.contains_key(&start) {
            continue;
        }
        let mut ring = vec![start];
        seen.insert(start, true);
        let mut at = start;
        loop {
            let b = *next.get(&at)?;
            if b == start {
                break;
            }
            if seen.insert(b, true).is_some() || ring.len() > order.len() {
                return None;
            }
            ring.push(b);
            at = b;
        }
        if ring.len() < 3 {
            return None;
        }
        rings.push(ring);
    }
    let (u, v) = projection(normal);
    let p2 = |id: u32| -> [f64; 2] {
        let p = pts[id as usize].p;
        [p[u], p[v]]
    };
    let area = |r: &[u32]| -> f64 {
        let mut a = 0.0;
        for i in 0..r.len() {
            let (p, q) = (p2(r[i]), p2(r[(i + 1) % r.len()]));
            a += p[0] * q[1] - q[0] * p[1];
        }
        a * 0.5
    };
    let (mut outers, mut holes): (Vec<Vec<u32>>, Vec<Vec<u32>>) = (Vec::new(), Vec::new());
    for r in rings {
        if area(&r) > 0.0 {
            outers.push(r);
        } else {
            holes.push(r);
        }
    }
    if outers.is_empty() {
        return None;
    }
    let inside = |pt: [f64; 2], r: &[u32]| -> bool {
        let mut c = false;
        for i in 0..r.len() {
            let (a, b) = (p2(r[i]), p2(r[(i + 1) % r.len()]));
            if (a[1] > pt[1]) != (b[1] > pt[1]) && pt[0] < a[0] + (b[0] - a[0]) * (pt[1] - a[1]) / (b[1] - a[1]) {
                c = !c;
            }
        }
        c
    };
    // Each hole goes to the smallest outer containing it.
    let mut per_outer: Vec<Vec<Vec<u32>>> = vec![Vec::new(); outers.len()];
    for h in holes {
        let probe = p2(h[0]);
        let host = (0..outers.len()).filter(|&i| inside(probe, &outers[i])).min_by(|&a, &b| area(&outers[a]).total_cmp(&area(&outers[b])).then(a.cmp(&b)))?;
        per_outer[host].push(h);
    }
    let mut scratch = Vec::new();
    let mut regions = Vec::new();
    for (outer, hs) in outers.into_iter().zip(per_outer) {
        let ring = bridge_holes(outer.clone(), hs.clone(), &p2)?;
        ear_clip(&ring, &p2, &mut scratch)?;
        regions.push((outer, hs));
    }
    Some(regions)
}

/// The two axes a plane with this normal projects onto, counter-clockwise kept so.
fn projection(normal: [f64; 3]) -> (usize, usize) {
    let ax = (0..3).max_by(|&x, &y| normal[x].abs().total_cmp(&normal[y].abs()).then(y.cmp(&x))).expect("three");
    let (u, v) = ((ax + 1) % 3, (ax + 2) % 3);
    if normal[ax] < 0.0 {
        (v, u)
    } else {
        (u, v)
    }
}

/// A face to triangulate: an outer ring and its holes (point ids), on one plane.
struct Region {
    outer: Vec<u32>,
    holes: Vec<Vec<u32>>,
    poly: usize,
    normal: [f64; 3],
}

/// Points that are a corner of no ring (they only split a straight edge, on every face that
/// uses them) and appear once per ring: removing them from every ring keeps the faces meeting
/// exactly and saves their triangles.
fn removable_points(regions: &[Region], pts: &[Point]) -> Vec<bool> {
    let mut corner = vec![false; pts.len()];
    for r in regions {
        for ring in std::iter::once(&r.outer).chain(r.holes.iter()) {
            let n = ring.len();
            let mut seen: HashMap<u32, u32> = HashMap::new();
            for &id in ring {
                *seen.entry(id).or_default() += 1;
            }
            for i in 0..n {
                let id = ring[i];
                if seen[&id] > 1 {
                    corner[id as usize] = true;
                    continue;
                }
                let (a, b, c) = (pts[ring[(i + n - 1) % n] as usize].p, pts[id as usize].p, pts[ring[(i + 1) % n] as usize].p);
                let (e1, e2) = (sub64(b, a), sub64(c, b));
                let (l1, l2) = (dot64(e1, e1).sqrt(), dot64(e2, e2).sqrt());
                let sin = dot64(cross64(e1, e2), r.normal).abs() / (l1 * l2).max(1e-300);
                if sin > 1e-9 || dot64(e1, e2) <= 0.0 {
                    corner[id as usize] = true;
                }
            }
        }
    }
    corner.into_iter().map(|c| !c).collect()
}

/// Triangulates a region (holes bridged, ears clipped); a convex ring that ear clipping can't
/// do gets a fan from its centre.
fn triangulate_region(r: &Region, pts: &[Point], extra: &mut Vec<[f64; 3]>, out: &mut Vec<[u32; 3]>) -> bool {
    let (u, v) = projection(r.normal);
    let p2 = |id: u32| -> [f64; 2] {
        if (id as usize) < pts.len() {
            let p = pts[id as usize].p;
            [p[u], p[v]]
        } else {
            let p = extra[id as usize - pts.len()];
            [p[u], p[v]]
        }
    };
    let before = out.len();
    if let Some(ring) = bridge_holes(r.outer.clone(), r.holes.clone(), &p2) {
        if ear_clip(&ring, &p2, out).is_some() {
            return true;
        }
    }
    out.truncate(before);
    if !r.holes.is_empty() {
        return false;
    }
    let n = r.outer.len() as f64;
    let mut centre = [0.0f64; 3];
    for &id in &r.outer {
        let p = pts[id as usize].p;
        for k in 0..3 {
            centre[k] += p[k] / n;
        }
    }
    extra.push(centre);
    let cid = (pts.len() + extra.len() - 1) as u32;
    let m = r.outer.len();
    for i in 0..m {
        out.push([cid, r.outer[i], r.outer[(i + 1) % m]]);
    }
    true
}

/// Splices each hole into the outer ring through a bridge from the hole's rightmost point to a
/// visible outer point (Eberly's method), holes in order of their rightmost x.
fn bridge_holes(mut outer: Vec<u32>, mut holes: Vec<Vec<u32>>, p2: &dyn Fn(u32) -> [f64; 2]) -> Option<Vec<u32>> {
    let rightmost = |h: &[u32]| -> usize { (0..h.len()).max_by(|&a, &b| p2(h[a])[0].total_cmp(&p2(h[b])[0]).then(p2(h[b])[1].total_cmp(&p2(h[a])[1])).then(b.cmp(&a))).expect("non-empty") };
    holes.sort_by(|a, b| p2(b[rightmost(b)])[0].total_cmp(&p2(a[rightmost(a)])[0]));
    for h in holes {
        let mi = rightmost(&h);
        let m = p2(h[mi]);
        // The nearest outer edge the ray +x from m crosses.
        let mut best: Option<(f64, usize)> = None;
        let n = outer.len();
        for i in 0..n {
            let (a, b) = (p2(outer[i]), p2(outer[(i + 1) % n]));
            if (a[1] > m[1]) == (b[1] > m[1]) && !(a[1] == m[1] && b[1] == m[1]) {
                continue;
            }
            if a[1] == b[1] {
                continue;
            }
            let x = a[0] + (b[0] - a[0]) * (m[1] - a[1]) / (b[1] - a[1]);
            if x >= m[0] && best.is_none_or_lt(x) {
                best = Some((x, i));
            }
        }
        let (x, ei) = best?;
        let (a, b) = (outer[ei], outer[(ei + 1) % n]);
        // The edge end further along +x, unless an outer reflex point inside the triangle
        // (m, hit, end) is closer in angle.
        let mut pick = if p2(a)[0] > p2(b)[0] { ei } else { (ei + 1) % n };
        let hit = [x, m[1]];
        let pp = p2(outer[pick]);
        let mut best_ang = f64::MAX;
        for j in 0..n {
            let q = p2(outer[j]);
            if j == pick || !in_triangle(q, m, hit, pp) {
                continue;
            }
            let (prev, nxt) = (p2(outer[(j + n - 1) % n]), p2(outer[(j + 1) % n]));
            let reflex = (q[0] - prev[0]) * (nxt[1] - q[1]) - (q[1] - prev[1]) * (nxt[0] - q[0]) <= 0.0;
            if !reflex {
                continue;
            }
            let d = [q[0] - m[0], q[1] - m[1]];
            let ang = d[1].abs() / (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-300);
            if ang < best_ang {
                best_ang = ang;
                pick = j;
            }
        }
        // outer[..=pick], hole from m around back to m, outer[pick..].
        let mut ring = Vec::with_capacity(outer.len() + h.len() + 2);
        ring.extend_from_slice(&outer[..=pick]);
        for k in 0..=h.len() {
            ring.push(h[(mi + k) % h.len()]);
        }
        ring.extend_from_slice(&outer[pick..]);
        outer = ring;
    }
    Some(outer)
}

trait NoneOrLt {
    fn is_none_or_lt(&self, x: f64) -> bool;
}

impl NoneOrLt for Option<(f64, usize)> {
    fn is_none_or_lt(&self, x: f64) -> bool {
        match self {
            None => true,
            Some((b, _)) => x < *b,
        }
    }
}

/// `p` in the closed triangle `a b c` (either winding).
fn in_triangle(p: [f64; 2], a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> bool {
    in_triangle_near(p, a, b, c, 0.0)
}

/// `p` in the triangle or within `tol` of it (distance; points on an edge up to float noise
/// count as on it).
fn in_triangle_near(p: [f64; 2], a: [f64; 2], b: [f64; 2], c: [f64; 2], tol: f64) -> bool {
    // Twice the signed area of (q, r, p): the edge's length times p's distance from its line.
    let s = |q: [f64; 2], r: [f64; 2]| -> (f64, f64) {
        let e = ((r[0] - q[0]) * (r[0] - q[0]) + (r[1] - q[1]) * (r[1] - q[1])).sqrt();
        ((r[0] - q[0]) * (p[1] - q[1]) - (r[1] - q[1]) * (p[0] - q[0]), e * tol)
    };
    let d = [s(a, b), s(b, c), s(c, a)];
    let neg = d.iter().any(|(v, t)| *v < -*t);
    let pos = d.iter().any(|(v, t)| *v > *t);
    !(neg && pos)
}

/// Ear clipping of a counter-clockwise ring that may repeat points (hole bridges) and hold
/// collinear points: an ear's tip must turn strictly left and its triangle hold no other point
/// of the ring (points with the same id as its corners excepted). `None` if it gets stuck.
fn ear_clip(ring: &[u32], p2: &dyn Fn(u32) -> [f64; 2], out: &mut Vec<[u32; 3]>) -> Option<()> {
    let n = ring.len();
    if n < 3 {
        return None;
    }
    let pts: Vec<[f64; 2]> = ring.iter().map(|&i| p2(i)).collect();
    let scale = pts.iter().fold(0.0f64, |m, p| m.max(p[0].abs()).max(p[1].abs())).max(1.0);
    let eps = scale * scale * 1e-14;
    let mut prev: Vec<usize> = (0..n).map(|i| (i + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|i| (i + 1) % n).collect();
    let mut alive = vec![true; n];
    let turn = |a: usize, b: usize, c: usize| -> f64 {
        let (a, b, c) = (pts[a], pts[b], pts[c]);
        (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
    };
    // Points that may lie inside an ear: those not turning strictly left (reflex, collinear,
    // bridge copies). Convex points can't be inside an ear of a simple ring.
    let mut reflex: Vec<bool> = (0..n).map(|i| turn(prev[i], i, next[i]) <= eps).collect();
    let mut remaining = n;
    let mut cur = 0usize;
    let mut since = 0usize;
    while remaining > 3 {
        if since > remaining {
            return None; // a full lap without an ear
        }
        let (ip, inx) = (prev[cur], next[cur]);
        let mut ear = !reflex[cur];
        if ear {
            let (a, b, c) = (pts[ip], pts[cur], pts[inx]);
            let mut j = next[inx];
            while j != ip {
                if reflex[j] && ring[j] != ring[ip] && ring[j] != ring[cur] && ring[j] != ring[inx] && in_triangle_near(pts[j], a, b, c, scale * 1e-9) {
                    ear = false;
                    break;
                }
                j = next[j];
            }
        }
        if !ear {
            cur = inx;
            since += 1;
            continue;
        }
        out.push([ring[ip], ring[cur], ring[inx]]);
        alive[cur] = false;
        next[ip] = inx;
        prev[inx] = ip;
        remaining -= 1;
        reflex[ip] = turn(prev[ip], ip, next[ip]) <= eps;
        reflex[inx] = turn(prev[inx], inx, next[inx]) <= eps;
        cur = ip;
        since = 0;
    }
    let first = (0..n).find(|&i| alive[i])?;
    let (a, b, c) = (first, next[first], next[next[first]]);
    if turn(a, b, c) <= eps {
        return None;
    }
    out.push([ring[a], ring[b], ring[c]]);
    Some(())
}

/// How pieces may share a face: one flat colour, or (gradients) only their own input triangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum GroupColour {
    Flat([u32; 4]),
    Source(u32),
}

/// The plane with its coefficients divided by their greatest common divisor: equal planes (same
/// points, same facing) get equal keys.
fn canonical(p: &Plane) -> Plane {
    fn gcd(mut a: u64, mut b: u64) -> u64 {
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a
    }
    let g = [p.n[0], p.n[1], p.n[2], p.d].iter().fold(0u64, |g, v| gcd(g, v.unsigned_abs())).max(1) as i64;
    Plane { n: p.n.map(|v| v / g), d: p.d / g }
}

/// The result shape, each triangle's operand, and (T-junctions split, non-manifold edges).
type Converted = (Shape, Vec<u32>, Cleanup);

fn to_shape(polys: &[Poly], grid: &Grid, srcs: &[Src], budget: &mut Budget) -> Result<Converted, CsgError> {
    if polys.is_empty() {
        return Err(CsgError::new(CsgErrorKind::Geometry, "the boolean's result is empty (the operands don't overlap the way the operation needs)"));
    }
    let mut w = Welder { cells: HashMap::new(), points: Vec::new() };
    let mut loops: Vec<Loop> = Vec::with_capacity(polys.len());
    for (pi, p) in polys.iter().enumerate() {
        let n = p.v.len();
        let mut v = Vec::with_capacity(n);
        for i in 0..n {
            let planes = [p.support, p.v[(i + n - 1) % n].edge, p.v[i].edge];
            let id = w.id(p.v[i].p, planes, budget)?;
            v.push(LoopV { id, edge: p.v[i].edge });
        }
        loops.push(Loop { v, support: p.support, poly: pi });
    }
    let t_junctions = fix_t_junctions(&mut loops, &w.points, budget)?;
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut tri_poly: Vec<usize> = Vec::new();
    let mut extra: Vec<[f64; 3]> = Vec::new();
    // Pieces on one plane with one flat colour and tag from one operand (or else of one input
    // triangle) become one face (an outer ring and holes) where they can.
    let mut groups: std::collections::BTreeMap<(Plane, GroupColour, u8, u32), Vec<usize>> = std::collections::BTreeMap::new();
    for (i, l) in loops.iter().enumerate() {
        let src = &srcs[polys[l.poly].src as usize];
        let colour = if src.col[0] == src.col[1] && src.col[1] == src.col[2] { GroupColour::Flat(src.col[0].map(f32::to_bits)) } else { GroupColour::Source(polys[l.poly].src) };
        // One operand per face too: which operand a triangle came from finds the edges booleans
        // made (bevels).
        groups.entry((canonical(&l.support), colour, src.tag, src.leaf)).or_default().push(i);
    }
    let mut regions: Vec<Region> = Vec::new();
    for members in groups.values() {
        let first = &loops[members[0]];
        let normal = first.support.unit_normal();
        if members.len() > 1 {
            let ls: Vec<&Loop> = members.iter().map(|&i| &loops[i]).collect();
            budget.charge(ls.iter().map(|l| l.v.len()).sum::<usize>() as u64 * 16)?;
            if let Some(rs) = merge_group(&ls, &w.points, normal) {
                regions.extend(rs.into_iter().map(|(outer, holes)| Region { outer, holes, poly: first.poly, normal }));
                continue;
            }
        }
        for &i in members {
            let l = &loops[i];
            regions.push(Region { outer: l.v.iter().map(|x| x.id).collect(), holes: Vec::new(), poly: l.poly, normal });
        }
    }
    // Drop straight-edge points everywhere; a face that then won't triangulate keeps all of its
    // points (and so do its neighbours), until every face triangulates. Faces triangulated as
    // they are always do (merged ones were checked; single pieces are convex).
    let mut gone = removable_points(&regions, &w.points);
    let filtered = |r: &Region, gone: &[bool]| -> Region {
        let keep = |ring: &Vec<u32>| -> Vec<u32> {
            let kept: Vec<u32> = ring.iter().copied().filter(|&id| !gone[id as usize]).collect();
            if kept.len() >= 3 {
                kept
            } else {
                ring.clone()
            }
        };
        Region { outer: keep(&r.outer), holes: r.holes.iter().map(keep).collect(), poly: r.poly, normal: r.normal }
    };
    // Ear clipping tests each candidate against the ring: up to n² per face.
    budget.charge(regions.iter().map(|r| { let n = (r.outer.len() + r.holes.iter().map(Vec::len).sum::<usize>()) as u64; n * n }).sum::<u64>())?;
    for _round in 0..=regions.len() {
        tris.clear();
        tri_poly.clear();
        extra.clear();
        let mut pinned = false;
        for r in &regions {
            budget.charge(1)?;
            let f = filtered(r, &gone);
            let before = tris.len();
            if triangulate_region(&f, &w.points, &mut extra, &mut tris) {
                tri_poly.extend(std::iter::repeat_n(r.poly, tris.len() - before));
                continue;
            }
            tris.truncate(before);
            let mut mine = false;
            for ring in std::iter::once(&r.outer).chain(r.holes.iter()) {
                for &id in ring {
                    mine |= gone[id as usize];
                    gone[id as usize] = false;
                }
            }
            pinned |= mine;
            if !mine {
                return Err(CsgError::new(CsgErrorKind::InvalidMesh, "a face of the boolean's result couldn't be triangulated (a degenerate cut); move a cutter slightly"));
            }
        }
        if !pinned {
            break;
        }
    }
    let mut points: Vec<[f64; 3]> = w.points.iter().map(|p| p.p).collect();
    points.extend(extra);
    // Features below what a mesh may hold (MeshData's smallest triangle) are cleaned away.
    let tau = (MIN_FEATURE_M / grid.step).max(3.0);
    let points = clean_small(&points, &mut tris, &mut tri_poly, tau, budget)?;
    // Closed: every directed edge is matched by as many reverse edges (an edge where two
    // pieces of the surface meet, as where solids touch along a line, is used twice each way).
    let mut edges: HashMap<(u32, u32), i32> = HashMap::new();
    for t in &tris {
        for k in 0..3 {
            *edges.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
        }
    }
    let open = edges.iter().filter(|(&(a, b), &n)| edges.get(&(b, a)).copied().unwrap_or(0) != n).count();
    let non_manifold = edges.iter().filter(|(&(a, b), &n)| n > 1 && a < b).count();
    if open > 0 {
        return Err(CsgError {
            kind: CsgErrorKind::InvalidMesh,
            message: format!("the boolean's result has {open} open edges (operands that self-intersect or touch in a degenerate way); move a cutter slightly"),
            measured: Some(open as f64),
            limit: Some(0.0),
        });
    }
    // Slivers: a connected piece whose mean thickness (2·|volume| / area) is below
    // MIN_THICKNESS_M is what's left where operands almost cancel (a solid minus itself nudged
    // by a micron): it is dropped, and a result that is all slivers is refused.
    let slivers = drop_slivers(&points, &mut tris, &mut tri_poly, grid.step, budget)?;
    if tris.is_empty() {
        return Err(CsgError::new(
            CsgErrorKind::Geometry,
            format!("the boolean's result has no volume: what is left is thinner than {} µm (operands that almost cancel out)", MIN_THICKNESS_M * 1e6),
        ));
    }
    // Shape: positions in metres, colours by barycentric interpolation in the source triangle.
    let mut s = Shape { positions: points.iter().map(|p| grid.metres(*p)).collect(), ..Shape::default() };
    let mut colors: Vec<Option<Rgba>> = vec![None; s.positions.len()];
    let mut leaf = Vec::with_capacity(tris.len());
    // A vertex shared by pieces of different sources takes the first one's colour; pieces with
    // other colours get their own vertex.
    let mut dup: HashMap<(u32, [u32; 4]), u32> = HashMap::new();
    for (t, &pi) in tris.iter().zip(&tri_poly) {
        let src = &srcs[polys[pi].src as usize];
        let mut tri = [0u32; 3];
        for k in 0..3 {
            let v = t[k];
            let c = bary_color(src, s.positions[v as usize]);
            tri[k] = match colors[v as usize] {
                None => {
                    colors[v as usize] = Some(c);
                    v
                }
                Some(c0) if c0 == c => v,
                Some(_) => *dup.entry((v, c.map(f32::to_bits))).or_insert_with(|| {
                    s.positions.push(s.positions[v as usize]);
                    colors.push(Some(c));
                    (s.positions.len() - 1) as u32
                }),
            };
        }
        s.tris.push(tri);
        s.tags.push(src.tag);
        leaf.push(src.leaf);
    }
    s.colors = colors.into_iter().map(|c| c.unwrap_or([1.0; 4])).collect();
    Ok((s, leaf, Cleanup { t_junctions, non_manifold_edges: non_manifold, slivers }))
}

/// What a result's cleanup found.
#[derive(Clone, Copy, Debug, Default)]
struct Cleanup {
    t_junctions: usize,
    non_manifold_edges: usize,
    slivers: usize,
}

/// Edges shorter than this, and triangles thinner, are collapsed or flipped away (metres):
/// [`crate::mesh::MIN_TRIANGLE_AREA`] is 1e-10 m², so what is left always passes validation.
pub const MIN_FEATURE_M: f64 = 2e-5;
/// Thinnest mean thickness (metres) a connected piece of a result may have.
pub const MIN_THICKNESS_M: f64 = 4e-5;

/// Drops connected pieces thinner on average than [`MIN_THICKNESS_M`]; returns how many.
fn drop_slivers(points: &[[f64; 3]], tris: &mut Vec<[u32; 3]>, owner: &mut Vec<usize>, step: f64, budget: &mut Budget) -> Result<usize, CsgError> {
    budget.charge(tris.len() as u64 / 8 + 1)?;
    let mut parent: Vec<u32> = (0..points.len() as u32).collect();
    fn find(p: &mut [u32], mut x: u32) -> u32 {
        while p[x as usize] != x {
            p[x as usize] = p[p[x as usize] as usize];
            x = p[x as usize];
        }
        x
    }
    for t in tris.iter() {
        let a = find(&mut parent, t[0]);
        for &v in &t[1..] {
            let b = find(&mut parent, v);
            if a != b {
                parent[b as usize] = a;
            }
        }
    }
    // Per component (by root): signed volume and area, in grid units.
    let mut vol: HashMap<u32, (f64, f64)> = HashMap::new();
    for t in tris.iter() {
        let root = find(&mut parent, t[0]);
        // Relative to the component's root point: small numbers, little cancellation.
        let r = points[root as usize];
        let [a, b, c] = t.map(|i| [0, 1, 2].map(|k| points[i as usize][k] - r[k]));
        let cross = [(b[1] - a[1]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[1] - a[1]), (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]), (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])];
        let area = 0.5 * (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let v6 = a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2];
        let e = vol.entry(root).or_default();
        e.0 += v6 / 6.0;
        e.1 += area;
    }
    let thin: std::collections::BTreeSet<u32> = vol.iter().filter(|(_, &(v, a))| a <= 0.0 || 2.0 * v.abs() / a * step < MIN_THICKNESS_M).map(|(r, _)| *r).collect();
    if thin.is_empty() {
        return Ok(0);
    }
    let mut keep_t = Vec::with_capacity(tris.len());
    let mut keep_o = Vec::with_capacity(owner.len());
    for (t, &o) in tris.iter().zip(owner.iter()) {
        if !thin.contains(&find(&mut parent, t[0])) {
            keep_t.push(*t);
            keep_o.push(o);
        }
    }
    *tris = keep_t;
    *owner = keep_o;
    Ok(thin.len())
}

/// Collapses edges shorter than `tau` (grid units) by welding their ends, then flips away caps
/// (a vertex within `tau` of its opposite edge) with the neighbour across that edge. Both keep
/// every directed edge balanced, so a closed mesh stays closed. Returns the new points.
fn clean_small(points: &[[f64; 3]], tris: &mut Vec<[u32; 3]>, owner: &mut Vec<usize>, tau: f64, budget: &mut Budget) -> Result<Vec<[f64; 3]>, CsgError> {
    // 1. Collapse short edges: weld points within tau, in vertex order (deterministic).
    let mut cells: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
    let mut reps: Vec<[f64; 3]> = Vec::new();
    let remap: Vec<u32> = points
        .iter()
        .map(|&p| {
            let c = p.map(|v| crate::dmath::floor_f64(v / tau) as i64);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        if let Some(list) = cells.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                            if let Some(&i) = list.iter().find(|&&i| (0..3).all(|k| (reps[i as usize][k] - p[k]).abs() <= tau)) {
                                return i;
                            }
                        }
                    }
                }
            }
            reps.push(p);
            let id = (reps.len() - 1) as u32;
            cells.entry(c).or_default().push(id);
            id
        })
        .collect();
    let mut kept = Vec::with_capacity(tris.len());
    let mut kept_owner = Vec::with_capacity(tris.len());
    for (t, &o) in tris.iter().zip(owner.iter()) {
        let m = t.map(|i| remap[i as usize]);
        if m[0] != m[1] && m[1] != m[2] && m[0] != m[2] {
            kept.push(m);
            kept_owner.push(o);
        }
    }
    *tris = kept;
    *owner = kept_owner;
    let mut pts = reps;
    // 2. Flip caps; collapse what can't be flipped; again until nothing changes.
    for _round in 0..6 {
        let flipped = flip_caps(&pts, tris, owner, tau, budget)?;
        let collapsed = collapse_tiny(&mut pts, tris, owner, tau, budget)?;
        if !flipped && !collapsed {
            break;
        }
    }
    Ok(pts)
}

/// Flips caps (a vertex within `tau` of its opposite edge) with the neighbour across that edge,
/// when that makes both triangles better. Returns whether any flipped.
fn flip_caps(pts: &[[f64; 3]], tris: &mut [[u32; 3]], owner: &mut [usize], tau: f64, budget: &mut Budget) -> Result<bool, CsgError> {
    let height = |t: &[u32; 3], k: usize| -> (f64, f64) {
        let (v, a, b) = (pts[t[k] as usize], pts[t[(k + 1) % 3] as usize], pts[t[(k + 2) % 3] as usize]);
        let e = sub64(b, a);
        let l = dot64(e, e).sqrt();
        let c = cross64(e, sub64(v, a));
        (dot64(c, c).sqrt() / l.max(1e-300), l)
    };
    let min_height = |t: &[u32; 3]| (0..3).map(|k| height(t, k).0).fold(f64::MAX, f64::min);
    // Directed edge -> its triangle, and how many triangles use it (flips only touch edges
    // used once each way).
    let mut edge: HashMap<(u32, u32), usize> = HashMap::new();
    let mut uses: HashMap<(u32, u32), u32> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        for k in 0..3 {
            edge.insert((t[k], t[(k + 1) % 3]), i);
            *uses.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
        }
    }
    let has_edge = |t: &[u32; 3], a: u32, b: u32| (0..3).any(|q| t[q] == a && t[(q + 1) % 3] == b);
    let mut any = false;
    for _pass in 0..8 {
        let mut changed = false;
        for ti in 0..tris.len() {
            let t = tris[ti];
            // The vertex opposite the longest edge: the cap's apex if the triangle is a cap.
            let k = (0..3).max_by(|&x, &y| height(&t, x).1.total_cmp(&height(&t, y).1).then(y.cmp(&x))).expect("three");
            let (h, _) = height(&t, k);
            if h >= tau {
                continue;
            }
            budget.charge(16)?;
            let (v, a, b) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
            let Some(&tj) = edge.get(&(b, a)) else { continue };
            if tj == ti {
                continue;
            }
            let u = tris[tj];
            if !has_edge(&u, b, a) || uses.get(&(a, b)) != Some(&1) || uses.get(&(b, a)) != Some(&1) {
                continue;
            }
            let Some(w) = u.iter().copied().find(|&x| x != a && x != b) else { continue };
            if w == v {
                continue;
            }
            let (n1, n2) = ([v, a, w], [v, w, b]);
            // Only when the flip makes both triangles better than the cap was, and keeps them
            // facing the way the pair did.
            let face = |t: &[u32; 3]| cross64(sub64(pts[t[1] as usize], pts[t[0] as usize]), sub64(pts[t[2] as usize], pts[t[0] as usize]));
            let (f0, f1) = (face(&t), face(&u));
            let ref_n = [f0[0] + f1[0], f0[1] + f1[1], f0[2] + f1[2]];
            if dot64(face(&n1), ref_n) <= 0.0 || dot64(face(&n2), ref_n) <= 0.0 {
                continue;
            }
            if min_height(&n1) <= h || min_height(&n2) <= h {
                continue;
            }
            if uses.contains_key(&(v, w)) || uses.contains_key(&(w, v)) {
                continue; // the diagonal exists already: flipping would make it non-manifold
            }
            for tt in [t, u] {
                for q in 0..3 {
                    let e = (tt[q], tt[(q + 1) % 3]);
                    edge.remove(&e);
                    if let Some(c) = uses.get_mut(&e) {
                        *c -= 1;
                        if *c == 0 {
                            uses.remove(&e);
                        }
                    }
                }
            }
            tris[ti] = n1;
            tris[tj] = n2;
            owner[ti] = owner[tj];
            for (idx, tt) in [(ti, n1), (tj, n2)] {
                for q in 0..3 {
                    edge.insert((tt[q], tt[(q + 1) % 3]), idx);
                    *uses.entry((tt[q], tt[(q + 1) % 3])).or_default() += 1;
                }
            }
            changed = true;
        }
        any |= changed;
        if !changed {
            break;
        }
    }
    Ok(any)
}

/// Collapses the shortest edge of every triangle still thinner than `tau` whose shortest edge is
/// short enough (64 `tau`) to move: a needle or a cap no flip could fix. Merging two points keeps
/// every directed edge balanced. Returns whether any collapsed.
fn collapse_tiny(pts: &mut [[f64; 3]], tris: &mut Vec<[u32; 3]>, owner: &mut Vec<usize>, tau: f64, budget: &mut Budget) -> Result<bool, CsgError> {
    let mut into: Vec<u32> = (0..pts.len() as u32).collect();
    let mut moved = vec![false; pts.len()];
    let mut any = false;
    for t in tris.iter() {
        let p = t.map(|i| pts[i as usize]);
        let n = cross64(sub64(p[1], p[0]), sub64(p[2], p[0]));
        let area2 = dot64(n, n).sqrt();
        let lens = [0, 1, 2].map(|k| {
            let d = sub64(p[(k + 1) % 3], p[k]);
            dot64(d, d).sqrt()
        });
        let longest = lens[0].max(lens[1]).max(lens[2]);
        if area2 / longest.max(1e-300) >= tau {
            continue;
        }
        budget.charge(4)?;
        let k = (0..3).min_by(|&x, &y| lens[x].total_cmp(&lens[y]).then(x.cmp(&y))).expect("three");
        if lens[k] > 64.0 * tau {
            continue;
        }
        let (u, v) = (t[k], t[(k + 1) % 3]);
        if moved[u as usize] || moved[v as usize] || into[u as usize] != u || into[v as usize] != v {
            continue;
        }
        // v joins u at their midpoint.
        let (pu, pv) = (pts[u as usize], pts[v as usize]);
        pts[u as usize] = [0, 1, 2].map(|c| (pu[c] + pv[c]) * 0.5);
        into[v as usize] = u;
        moved[u as usize] = true;
        moved[v as usize] = true;
        any = true;
    }
    if any {
        let mut kept = Vec::with_capacity(tris.len());
        let mut kept_owner = Vec::with_capacity(tris.len());
        for (t, &o) in tris.iter().zip(owner.iter()) {
            let m = t.map(|i| into[i as usize]);
            if m[0] != m[1] && m[1] != m[2] && m[0] != m[2] {
                kept.push(m);
                kept_owner.push(o);
            }
        }
        *tris = kept;
        *owner = kept_owner;
    }
    Ok(any)
}

/// The source triangle's colour at `p` (barycentric, clamped).
fn bary_color(src: &Src, p: V3) -> Rgba {
    if src.col[0] == src.col[1] && src.col[1] == src.col[2] {
        return src.col[0];
    }
    let [a, b, c] = src.pos;
    let n = math3::tri_normal(a, b, c);
    let area = math3::dot(n, n);
    if area <= 0.0 {
        return src.col[0];
    }
    let wa = math3::dot(math3::tri_normal(p, b, c), n) / area;
    let wb = math3::dot(math3::tri_normal(a, p, c), n) / area;
    let (wa, wb) = (wa.clamp(0.0, 1.0), wb.clamp(0.0, 1.0));
    let wc = (1.0 - wa - wb).max(0.0);
    let sum = wa + wb + wc;
    [0, 1, 2, 3].map(|k| (src.col[0][k] * wa + src.col[1][k] * wb + src.col[2][k] * wc) / sum)
}

mod bevel;
pub use bevel::{Bevel, BevelStats, MAX_ROUND_SEGMENTS};

/// [`evaluate`], then the boolean edges bevelled when `bevel` is given: the result's boolean
/// edges found, chamfer tubes swept along them ([`bevel`]), and the tubes cut away (convex
/// edges) or added (concave ones) in the same exact plane space, so faces stay exactly flat and
/// merge back into few triangles.
pub fn evaluate_bevelled(tree: &CsgTree, bevel: Option<&Bevel>, budget: &mut Budget) -> Result<(Shape, CsgStats, BevelStats), CsgError> {
    let (mut ses, input) = Session::new(tree)?;
    let polys = eval_node(tree, &ses.grid, &mut ses.srcs, &mut ses.leaf_id, budget)?;
    let (shape, leaf, cl) = to_shape(&polys, &ses.grid, &ses.srcs, budget)?;
    let dropped = budget.dropped;
    let step = ses.grid.step;
    let stats = |s: &Shape, budget: &Budget| CsgStats {
        input_triangles: input,
        output_triangles: s.tris.len(),
        work: budget.work,
        polygons: budget.polygons,
        t_junctions: cl.t_junctions,
        non_manifold_edges: cl.non_manifold_edges,
        step,
        dropped_triangles: dropped,
        slivers: cl.slivers,
    };
    let Some(b) = bevel.filter(|b| b.width > 0.0) else {
        let st = stats(&shape, budget);
        return Ok((shape, st, BevelStats::default()));
    };
    let (cutters, fillers, mut bs) = bevel::tubes(&shape, &leaf, b, budget)?;
    let add = |tubes: Vec<Shape>, ses: &mut Session, budget: &mut Budget, bs: &mut BevelStats| -> Vec<Vec<Poly>> {
        tubes
            .iter()
            .filter_map(|t| {
                let p = ses.add(t, budget);
                if p.is_none() {
                    bs.skipped += 1;
                }
                p
            })
            .collect()
    };
    let cut = add(cutters, &mut ses, budget, &mut bs);
    let fill = add(fillers, &mut ses, budget, &mut bs);
    let apply = |cut: Vec<Vec<Poly>>, fill: Vec<Vec<Poly>>, budget: &mut Budget| -> Result<Shape, CsgError> {
        let mut out = multi_difference(polys.clone(), disjoint_groups(cut, budget)?, budget)?;
        if !fill.is_empty() {
            out = multi_union(out, disjoint_groups(fill, budget)?, budget)?;
        }
        Ok(to_shape(&out, &ses.grid, &ses.srcs, budget)?.0)
    };
    let mut done = apply(cut.clone(), fill.clone(), budget);
    if matches!(&done, Err(e) if e.kind != CsgErrorKind::Limit) {
        // One bad tube (a loop whose mitres fold where it turns tightly) spoils the whole pass:
        // find the tubes that fail on their own, leave those edges sharp and keep the rest.
        let alone = |tubes: Vec<Vec<Poly>>, is_cut: bool, budget: &mut Budget| -> Result<(Vec<Vec<Poly>>, usize), CsgError> {
            let mut keep = Vec::with_capacity(tubes.len());
            let mut dropped = 0;
            for t in tubes {
                let r = if is_cut { apply(vec![t.clone()], Vec::new(), budget) } else { apply(Vec::new(), vec![t.clone()], budget) };
                match r {
                    Ok(_) => keep.push(t),
                    Err(e) if e.kind == CsgErrorKind::Limit => return Err(e),
                    Err(_) => dropped += 1,
                }
            }
            Ok((keep, dropped))
        };
        let retried = (|| -> Result<Option<(Shape, usize)>, CsgError> {
            let (cut, dc) = alone(cut, true, budget)?;
            let (fill, df) = alone(fill, false, budget)?;
            if dc + df == 0 || cut.len() + fill.len() == 0 {
                return Ok(None);
            }
            match apply(cut, fill, budget) {
                Ok(s) => Ok(Some((s, dc + df))),
                Err(e) if e.kind == CsgErrorKind::Limit => Err(e),
                Err(_) => Ok(None),
            }
        })();
        match retried {
            Ok(Some((s, dropped))) => {
                bs.skipped += dropped;
                done = Ok(s);
            }
            Ok(None) => {}
            Err(e) => done = Err(e),
        }
    }
    match done {
        Ok(s) => {
            let st = stats(&s, budget);
            Ok((s, st, bs))
        }
        // The bevel is a finish: when its booleans can't be done the edges stay sharp (the stats
        // say so); running out of budget is still an error.
        Err(e) if e.kind != CsgErrorKind::Limit => {
            let st = stats(&shape, budget);
            Ok((shape, st, BevelStats { failed: Some(e.message), ..bs }))
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
pub(crate) mod tests;
