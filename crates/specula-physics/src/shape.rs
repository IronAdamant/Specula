//! Collision shapes: sphere, capsule, box and convex hull (any body), and triangle mesh and
//! heightfield (static only), with their bounds, support functions and mass properties.
//!
//! Shapes are given in their collider's local frame, already scaled (the engine bakes an
//! entity's scale in). Heavy shape data (hulls, meshes) is shared through `Arc`, so cloning a
//! shape is cheap.

use crate::math::{abs, len_sq, mul, normalize_or, v3, Aabb, Mat3, Pose, Vec3};
use crate::PhysicsError;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Most points a convex hull may be built from (its cost grows with them).
pub const MAX_HULL_POINTS: usize = 64;
/// Most triangles of a mesh collider.
pub const MAX_MESH_TRIANGLES: usize = 65_536;
/// Most samples per side of a heightfield.
pub const MAX_HEIGHTFIELD_SIDE: usize = 257;

/// A collision shape in its collider's local frame.
#[derive(Clone, Debug)]
pub enum Shape {
    Sphere { radius: f32 },
    /// A segment along local Y from `-half_height` to `+half_height`, swept by `radius`.
    Capsule { radius: f32, half_height: f32 },
    /// Half extents; `hull` is the same box as a polytope (for the polytope tests).
    Box { half: Vec3, hull: Arc<Hull> },
    Hull(Arc<Hull>),
    /// Static only.
    Mesh(Arc<TriMesh>),
    /// Static only: a grid of heights, centred on the local origin.
    Heightfield(Arc<Heightfield>),
}

impl Shape {
    pub fn sphere(radius: f32) -> Shape {
        Shape::Sphere { radius }
    }

    /// `half_height` is half the length of the core segment (the total height is
    /// `2 * (half_height + radius)`).
    pub fn capsule(radius: f32, half_height: f32) -> Shape {
        Shape::Capsule { radius, half_height: half_height.max(0.0) }
    }

    pub fn cuboid(half: Vec3) -> Shape {
        let h = half;
        let pts: Vec<Vec3> = (0..8)
            .map(|i| v3(if i & 1 == 0 { -h.x } else { h.x }, if i & 2 == 0 { -h.y } else { h.y }, if i & 4 == 0 { -h.z } else { h.z }))
            .collect();
        let hull = Hull::new(&pts).expect("a box is a valid hull");
        Shape::Box { half, hull: Arc::new(hull) }
    }

    pub fn hull(points: &[Vec3]) -> Result<Shape, PhysicsError> {
        Ok(Shape::Hull(Arc::new(Hull::new(points)?)))
    }

    pub fn mesh(vertices: Vec<Vec3>, triangles: Vec<[u32; 3]>) -> Result<Shape, PhysicsError> {
        Ok(Shape::Mesh(Arc::new(TriMesh::new(vertices, triangles)?)))
    }

    /// `heights[z * columns + x]`; `cell` is the spacing along x and z.
    pub fn heightfield(columns: usize, rows: usize, heights: Vec<f32>, cell: (f32, f32)) -> Result<Shape, PhysicsError> {
        Ok(Shape::Heightfield(Arc::new(Heightfield::new(columns, rows, heights, cell)?)))
    }

    pub fn name(&self) -> &'static str {
        match self {
            Shape::Sphere { .. } => "sphere",
            Shape::Capsule { .. } => "capsule",
            Shape::Box { .. } => "box",
            Shape::Hull(_) => "hull",
            Shape::Mesh(_) => "mesh",
            Shape::Heightfield(_) => "heightfield",
        }
    }

    /// Meshes and heightfields have no volume: they can only be static.
    pub fn is_static_only(&self) -> bool {
        matches!(self, Shape::Mesh(_) | Shape::Heightfield(_))
    }

    /// The polytope, for boxes and hulls.
    pub fn polytope(&self) -> Option<&Hull> {
        match self {
            Shape::Box { hull, .. } | Shape::Hull(hull) => Some(hull),
            _ => None,
        }
    }

    pub fn triangles(&self) -> Option<&TriMesh> {
        match self {
            Shape::Mesh(m) => Some(m),
            Shape::Heightfield(h) => Some(&h.mesh),
            _ => None,
        }
    }

    pub fn local_aabb(&self) -> Aabb {
        match self {
            Shape::Sphere { radius } => Aabb::around(Vec3::ZERO, v3(*radius, *radius, *radius)),
            Shape::Capsule { radius, half_height } => Aabb::around(Vec3::ZERO, v3(*radius, half_height + radius, *radius)),
            Shape::Box { half, .. } => Aabb::around(Vec3::ZERO, *half),
            Shape::Hull(h) => h.aabb,
            Shape::Mesh(m) => m.aabb,
            Shape::Heightfield(h) => h.mesh.aabb,
        }
    }

    /// World bounds at `pose` (exact for spheres and capsules, conservative otherwise).
    pub fn aabb(&self, pose: &Pose) -> Aabb {
        match self {
            Shape::Sphere { radius } => Aabb::around(pose.p, v3(*radius, *radius, *radius)),
            Shape::Capsule { radius, half_height } => {
                let a = pose.point(v3(0.0, *half_height, 0.0));
                let b = pose.point(v3(0.0, -half_height, 0.0));
                Aabb::new(a.min(b), a.max(b)).expand(*radius)
            }
            _ => {
                let local = self.local_aabb();
                let m = Mat3::from_quat(pose.q);
                let c = pose.point(local.center());
                let e = local.extents();
                let half = abs(m.c[0]) * e.x + abs(m.c[1]) * e.y + abs(m.c[2]) * e.z;
                Aabb::around(c, half)
            }
        }
    }

    /// The radius of a sphere the shape surely contains, around its centre of mass: how far it
    /// may move in one step before it could pass through something thin (the tunnelling guard).
    pub fn inner_radius(&self) -> f32 {
        match self {
            Shape::Sphere { radius } | Shape::Capsule { radius, .. } => *radius,
            Shape::Box { half, .. } => half.x.min(half.y).min(half.z),
            Shape::Hull(h) => h.inner_radius,
            Shape::Mesh(_) | Shape::Heightfield(_) => 0.0,
        }
    }

    /// Distance from the centre of mass to the farthest point (for angular sleep checks).
    pub fn outer_radius(&self) -> f32 {
        match self {
            Shape::Sphere { radius } => *radius,
            Shape::Capsule { radius, half_height } => radius + half_height,
            Shape::Box { half, .. } => half.length(),
            Shape::Hull(h) => h.outer_radius,
            Shape::Mesh(m) => m.aabb.extents().length(),
            Shape::Heightfield(h) => h.mesh.aabb.extents().length(),
        }
    }

    /// Mass properties for `mass` kilograms of uniform density: `(centre of mass, inertia
    /// about it)` in the shape's frame. Meshes and heightfields have none (static only).
    pub fn mass_properties(&self, mass: f32) -> (Vec3, Mat3) {
        match self {
            Shape::Sphere { radius } => (Vec3::ZERO, Mat3::diag(Vec3::ONE * (0.4 * mass * radius * radius))),
            Shape::Capsule { radius, half_height } => {
                let (r, h) = (*radius, 2.0 * half_height);
                let pi = std::f32::consts::PI;
                let vc = pi * r * r * h;
                let vs = 4.0 / 3.0 * pi * r * r * r;
                let (mc, ms) = (mass * vc / (vc + vs), mass * vs / (vc + vs));
                let iy = mc * r * r * 0.5 + ms * 0.4 * r * r;
                let ix = mc * (h * h / 12.0 + r * r / 4.0) + ms * (0.4 * r * r + h * h / 4.0 + 3.0 * h * r / 8.0);
                (Vec3::ZERO, Mat3::diag(v3(ix, iy, ix)))
            }
            Shape::Box { half, .. } => {
                let s = mul(*half, *half);
                let k = mass / 3.0;
                (Vec3::ZERO, Mat3::diag(v3(k * (s.y + s.z), k * (s.x + s.z), k * (s.x + s.y))))
            }
            Shape::Hull(h) => (h.centroid, h.unit_inertia.scale(mass / h.volume.max(1e-12))),
            Shape::Mesh(_) | Shape::Heightfield(_) => (Vec3::ZERO, Mat3::ZERO),
        }
    }
}

/// An edge of a hull: its two vertices and the faces either side (for the Gauss-map test).
#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub v: [u16; 2],
    pub faces: [u16; 2],
}

/// A convex polyhedron: vertices, faces (outward planes with counter-clockwise vertex loops)
/// and edges.
#[derive(Clone, Debug)]
pub struct Hull {
    pub points: Vec<Vec3>,
    /// Outward unit normals.
    pub normals: Vec<Vec3>,
    /// Plane offsets: `normals[f] · x = offsets[f]` on face `f`.
    pub offsets: Vec<f32>,
    /// Face `f`'s vertex loop is `loops[starts[f]..starts[f + 1]]`, counter-clockwise seen
    /// from outside.
    pub loops: Vec<u16>,
    pub starts: Vec<u32>,
    pub edges: Vec<Edge>,
    pub centroid: Vec3,
    pub volume: f32,
    /// Inertia about the centroid at density 1.
    pub unit_inertia: Mat3,
    pub aabb: Aabb,
    pub inner_radius: f32,
    pub outer_radius: f32,
}

impl Hull {
    /// The convex hull of `points` (4 to [`MAX_HULL_POINTS`] points, not all in a plane).
    /// Points inside the hull or on its faces are dropped.
    pub fn new(input: &[Vec3]) -> Result<Hull, PhysicsError> {
        if input.len() < 4 || input.len() > MAX_HULL_POINTS {
            return Err(PhysicsError::new("invalid_hull", format!("a hull needs 4 to {MAX_HULL_POINTS} points (got {})", input.len()))
                .with("points", input.len()));
        }
        if !input.iter().all(|p| crate::math::is_finite(*p)) {
            return Err(PhysicsError::new("invalid_hull", "hull points must be finite"));
        }
        let bounds = Aabb::of_points(input.iter().copied());
        let size = (bounds.max - bounds.min).length();
        if size <= 1e-6 {
            return Err(PhysicsError::new("invalid_hull", "hull points are all (nearly) the same point"));
        }
        let eps = size * 1.0e-5;
        // Drop duplicates (first one wins: deterministic).
        let mut pts: Vec<Vec3> = Vec::with_capacity(input.len());
        for p in input {
            if !pts.iter().any(|q| len_sq(*q - *p) <= eps * eps) {
                pts.push(*p);
            }
        }
        // Every plane through three points with all points on one side is a face plane.
        let n = pts.len();
        let mut planes: Vec<(Vec3, f32)> = Vec::new();
        for i in 0..n {
            for j in i + 1..n {
                for k in j + 1..n {
                    let nrm = (pts[j] - pts[i]).cross(pts[k] - pts[i]);
                    let l = nrm.length();
                    if l <= size * size * 1.0e-6 {
                        continue;
                    }
                    let nrm = nrm * (1.0 / l);
                    let d = nrm.dot(pts[i]);
                    let (mut above, mut below) = (false, false);
                    for p in &pts {
                        let s = nrm.dot(*p) - d;
                        above |= s > eps;
                        below |= s < -eps;
                        if above && below {
                            break;
                        }
                    }
                    let plane = match (above, below) {
                        (false, _) => (nrm, d),
                        (true, false) => (-nrm, -d),
                        (true, true) => continue,
                    };
                    if !planes.iter().any(|(m, e)| m.dot(plane.0) > 1.0 - 1.0e-5 && (e - plane.1).abs() <= eps * 2.0) {
                        planes.push(plane);
                    }
                }
            }
        }
        if planes.len() < 4 {
            return Err(PhysicsError::new("invalid_hull", "hull points are flat (all in one plane) or collinear: a hull needs volume"));
        }
        // Each face's polygon: the 2D convex hull of the points on its plane.
        let mut used: BTreeSet<usize> = BTreeSet::new();
        let mut faces: Vec<(Vec3, f32, Vec<usize>)> = Vec::new();
        for (nrm, d) in &planes {
            let on: Vec<usize> = (0..n).filter(|&i| (nrm.dot(pts[i]) - d).abs() <= eps * 2.0).collect();
            let (t1, t2) = crate::math::basis(*nrm);
            let loop_ = convex_loop(&on, |i| (t1.dot(pts[i]), t2.dot(pts[i])));
            if loop_.len() < 3 {
                continue;
            }
            used.extend(loop_.iter().copied());
            faces.push((*nrm, *d, loop_));
        }
        // Keep only hull vertices, renumbered in input order.
        let remap: BTreeMap<usize, u16> = used.iter().enumerate().map(|(new, &old)| (old, new as u16)).collect();
        let points: Vec<Vec3> = used.iter().map(|&i| pts[i]).collect();
        let mut normals = Vec::with_capacity(faces.len());
        let mut offsets = Vec::with_capacity(faces.len());
        let mut loops = Vec::new();
        let mut starts = vec![0u32];
        for (nrm, d, l) in &faces {
            normals.push(*nrm);
            offsets.push(*d);
            loops.extend(l.iter().map(|i| remap[i]));
            starts.push(loops.len() as u32);
        }
        // Edges: each undirected loop edge, with the faces on either side.
        let mut edge_faces: BTreeMap<(u16, u16), Vec<u16>> = BTreeMap::new();
        for f in 0..normals.len() {
            let l = &loops[starts[f] as usize..starts[f + 1] as usize];
            for k in 0..l.len() {
                let (a, b) = (l[k], l[(k + 1) % l.len()]);
                edge_faces.entry((a.min(b), a.max(b))).or_default().push(f as u16);
            }
        }
        let edges: Vec<Edge> = edge_faces
            .into_iter()
            .filter(|(_, f)| f.len() == 2)
            .map(|((a, b), f)| Edge { v: [a, b], faces: [f[0], f[1]] })
            .collect();
        let mut hull = Hull {
            points,
            normals,
            offsets,
            loops,
            starts,
            edges,
            centroid: Vec3::ZERO,
            volume: 0.0,
            unit_inertia: Mat3::ZERO,
            aabb: bounds,
            inner_radius: 0.0,
            outer_radius: 0.0,
        };
        hull.compute_mass();
        if hull.volume <= size * size * size * 1.0e-9 {
            return Err(PhysicsError::new("invalid_hull", "hull has (almost) no volume"));
        }
        hull.aabb = Aabb::of_points(hull.points.iter().copied());
        let c = hull.centroid;
        hull.inner_radius = (0..hull.normals.len()).map(|f| hull.offsets[f] - hull.normals[f].dot(c)).fold(f32::MAX, f32::min).max(0.0);
        hull.outer_radius = hull.points.iter().map(|p| (*p - c).length()).fold(0.0, f32::max);
        Ok(hull)
    }

    pub fn face_count(&self) -> usize {
        self.normals.len()
    }

    pub fn face(&self, f: usize) -> &[u16] {
        &self.loops[self.starts[f] as usize..self.starts[f + 1] as usize]
    }

    /// Index of the vertex farthest along `d` (local); ties go to the lowest index.
    pub fn support_index(&self, d: Vec3) -> usize {
        let mut best = 0;
        let mut best_dot = self.points[0].dot(d);
        for (i, p) in self.points.iter().enumerate().skip(1) {
            let x = p.dot(d);
            if x > best_dot {
                best = i;
                best_dot = x;
            }
        }
        best
    }

    /// Volume, centroid and inertia by summing tetrahedra from an interior point to each face
    /// triangle (covariance method).
    fn compute_mass(&mut self) {
        let r = self.points.iter().fold(Vec3::ZERO, |a, p| a + *p) * (1.0 / self.points.len() as f32);
        let mut volume = 0.0f64;
        let mut first = [0.0f64; 3];
        // Covariance (second moments) about `r`.
        let mut cov = [[0.0f64; 3]; 3];
        for f in 0..self.face_count() {
            let l = self.face(f);
            for k in 1..l.len() - 1 {
                let a = self.points[l[0] as usize] - r;
                let b = self.points[l[k] as usize] - r;
                let c = self.points[l[k + 1] as usize] - r;
                let det = f64::from(a.dot(b.cross(c)));
                volume += det / 6.0;
                let s = [f64::from(a.x + b.x + c.x), f64::from(a.y + b.y + c.y), f64::from(a.z + b.z + c.z)];
                for (i, si) in s.iter().enumerate() {
                    first[i] += det / 24.0 * si;
                }
                let m = [[a.x, b.x, c.x], [a.y, b.y, c.y], [a.z, b.z, c.z]].map(|row| row.map(f64::from));
                // A · C_canonical · Aᵀ with C_canonical = [[2,1,1],[1,2,1],[1,1,2]] / 120.
                for i in 0..3 {
                    for j in 0..3 {
                        let mut acc = 0.0;
                        for p in 0..3 {
                            for q in 0..3 {
                                let cc = if p == q { 2.0 } else { 1.0 };
                                acc += m[i][p] * cc * m[j][q];
                            }
                        }
                        cov[i][j] += det * acc / 120.0;
                    }
                }
            }
        }
        if volume <= 0.0 {
            self.volume = 0.0;
            return;
        }
        let com = [first[0] / volume, first[1] / volume, first[2] / volume];
        // Covariance about the centroid, then the inertia tensor.
        for i in 0..3 {
            for j in 0..3 {
                cov[i][j] -= volume * com[i] * com[j];
            }
        }
        let tr = cov[0][0] + cov[1][1] + cov[2][2];
        let at = |i: usize, j: usize| -> f32 { ((if i == j { tr } else { 0.0 }) - cov[i][j]) as f32 };
        self.unit_inertia = Mat3 { c: [v3(at(0, 0), at(1, 0), at(2, 0)), v3(at(0, 1), at(1, 1), at(2, 1)), v3(at(0, 2), at(1, 2), at(2, 2))] };
        self.volume = volume as f32;
        self.centroid = r + v3(com[0] as f32, com[1] as f32, com[2] as f32);
    }
}

/// Indices of the 2D convex hull of `ids` (projected by `at`), counter-clockwise, collinear
/// points dropped (Andrew's monotone chain: comparisons and cross products only).
fn convex_loop(ids: &[usize], at: impl Fn(usize) -> (f32, f32)) -> Vec<usize> {
    let mut pts: Vec<(f32, f32, usize)> = ids.iter().map(|&i| {
        let (x, y) = at(i);
        (x, y, i)
    }).collect();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
    if pts.len() < 3 {
        return pts.iter().map(|p| p.2).collect();
    }
    let span = {
        let (lo, hi) = pts.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.0).min(p.1), hi.max(p.0).max(p.1)));
        (hi - lo).max(1e-6)
    };
    let eps = span * span * 1.0e-7;
    let cross = |o: (f32, f32, usize), a: (f32, f32, usize), b: (f32, f32, usize)| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let mut lower: Vec<(f32, f32, usize)> = Vec::new();
    for p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], *p) <= eps {
            lower.pop();
        }
        lower.push(*p);
    }
    let mut upper: Vec<(f32, f32, usize)> = Vec::new();
    for p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], *p) <= eps {
            upper.pop();
        }
        upper.push(*p);
    }
    lower.pop();
    upper.pop();
    lower.into_iter().chain(upper).map(|p| p.2).collect()
}

/// A static triangle mesh with a bounding-volume hierarchy and per-edge flags that tell the
/// contact code which edges are real (convex or open) and which are seams between triangles
/// (flat or concave), so bodies sliding over a seam don't snag on it.
#[derive(Clone, Debug)]
pub struct TriMesh {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
    /// Unit normals (counter-clockwise winding faces out).
    pub normals: Vec<Vec3>,
    /// Bit `k`: edge `k` (vertex `k` to `k + 1`) is a real edge; clear: an internal seam.
    pub edge_flags: Vec<u8>,
    pub aabb: Aabb,
    nodes: Vec<BvhNode>,
    /// Triangle indices in leaf order.
    order: Vec<u32>,
}

#[derive(Clone, Copy, Debug)]
struct BvhNode {
    aabb: Aabb,
    /// Leaves: first index into `order`; inner nodes: the right child (the left is next).
    index: u32,
    /// Leaves: triangle count (> 0); inner nodes: 0.
    count: u32,
}

const BVH_LEAF: usize = 4;

impl TriMesh {
    pub fn new(vertices: Vec<Vec3>, triangles: Vec<[u32; 3]>) -> Result<TriMesh, PhysicsError> {
        if triangles.is_empty() || triangles.len() > MAX_MESH_TRIANGLES {
            return Err(PhysicsError::new("invalid_mesh", format!("a mesh collider needs 1 to {MAX_MESH_TRIANGLES} triangles (got {})", triangles.len()))
                .with("triangles", triangles.len()));
        }
        if !vertices.iter().all(|p| crate::math::is_finite(*p)) {
            return Err(PhysicsError::new("invalid_mesh", "mesh vertices must be finite"));
        }
        if let Some((i, _)) = triangles.iter().enumerate().find(|(_, t)| t.iter().any(|&v| v as usize >= vertices.len())) {
            return Err(PhysicsError::new("invalid_mesh", format!("triangle {i} uses a vertex index past the {} vertices", vertices.len()))
                .with("triangle", i));
        }
        let normals: Vec<Vec3> = triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| vertices[i as usize]);
                normalize_or((b - a).cross(c - a), Vec3::Y)
            })
            .collect();
        // Edge flags from adjacency.
        let mut by_edge: BTreeMap<(u32, u32), Vec<(u32, u8)>> = BTreeMap::new();
        for (ti, t) in triangles.iter().enumerate() {
            for k in 0..3u8 {
                let (a, b) = (t[k as usize], t[(k as usize + 1) % 3]);
                by_edge.entry((a.min(b), a.max(b))).or_default().push((ti as u32, k));
            }
        }
        let mut edge_flags = vec![0b111u8; triangles.len()];
        for list in by_edge.values() {
            if list.len() != 2 {
                continue;
            }
            let (t1, k1) = list[0];
            let (t2, k2) = list[1];
            let n1 = normals[t1 as usize];
            let n2 = normals[t2 as usize];
            let opp2 = vertices[triangles[t2 as usize][(k2 as usize + 2) % 3] as usize];
            let p1 = vertices[triangles[t1 as usize][k1 as usize] as usize];
            let flat = n1.dot(n2) > 0.9998;
            let concave = n1.dot(opp2 - p1) > 1.0e-6;
            if flat || concave {
                edge_flags[t1 as usize] &= !(1 << k1);
                edge_flags[t2 as usize] &= !(1 << k2);
            }
        }
        let aabb = Aabb::of_points(vertices.iter().copied());
        let mut mesh = TriMesh { vertices, triangles, normals, edge_flags, aabb, nodes: Vec::new(), order: Vec::new() };
        mesh.build_bvh();
        Ok(mesh)
    }

    pub fn triangle(&self, i: usize) -> [Vec3; 3] {
        self.triangles[i].map(|v| self.vertices[v as usize])
    }

    fn tri_aabb(&self, i: usize) -> Aabb {
        Aabb::of_points(self.triangle(i))
    }

    fn build_bvh(&mut self) {
        let n = self.triangles.len();
        let centroids: Vec<Vec3> = (0..n).map(|i| {
            let [a, b, c] = self.triangle(i);
            (a + b + c) * (1.0 / 3.0)
        }).collect();
        let mut order: Vec<u32> = (0..n as u32).collect();
        let mut nodes = Vec::with_capacity(2 * n / BVH_LEAF + 1);
        self.build_node(&mut nodes, &mut order, 0, n, &centroids);
        self.nodes = nodes;
        self.order = order;
    }

    fn build_node(&self, nodes: &mut Vec<BvhNode>, order: &mut [u32], start: usize, end: usize, centroids: &[Vec3]) -> usize {
        let at = nodes.len();
        let aabb = order[start..end].iter().fold(Aabb::EMPTY, |b, &t| b.union(&self.tri_aabb(t as usize)));
        nodes.push(BvhNode { aabb, index: start as u32, count: (end - start) as u32 });
        if end - start <= BVH_LEAF {
            return at;
        }
        let cb = Aabb::of_points(order[start..end].iter().map(|&t| centroids[t as usize]));
        let ext = cb.max - cb.min;
        let axis = if ext.x >= ext.y && ext.x >= ext.z { 0 } else if ext.y >= ext.z { 1 } else { 2 };
        order[start..end].sort_by(|&a, &b| {
            crate::math::get(centroids[a as usize], axis).total_cmp(&crate::math::get(centroids[b as usize], axis)).then(a.cmp(&b))
        });
        let mid = (start + end) / 2;
        self.build_node(nodes, order, start, mid, centroids);
        let right = self.build_node(nodes, order, mid, end, centroids);
        nodes[at].index = right as u32;
        nodes[at].count = 0;
        at
    }

    /// Calls `f` for each triangle whose bounds overlap `b` (local), in a fixed order, until it
    /// returns `false`.
    pub fn each_in(&self, b: &Aabb, mut f: impl FnMut(usize) -> bool) {
        if self.nodes.is_empty() {
            return;
        }
        let mut stack = [0u32; 64];
        let mut top = 1;
        while top > 0 {
            top -= 1;
            let ni = stack[top];
            let node = self.nodes[ni as usize];
            if !node.aabb.overlaps(b) {
                continue;
            }
            if node.count > 0 {
                for &t in &self.order[node.index as usize..(node.index + node.count) as usize] {
                    if self.tri_aabb(t as usize).overlaps(b) && !f(t as usize) {
                        return;
                    }
                }
            } else if top + 2 <= stack.len() {
                stack[top] = node.index;
                stack[top + 1] = ni + 1;
                top += 2;
            }
        }
    }

    /// Calls `f(triangle, max_t)` for triangles whose bounds the ray (local) may hit before the
    /// current `max_t`; `f` returns the new `max_t` (lower it to find the closest hit).
    pub fn each_on_ray(&self, origin: Vec3, dir: Vec3, mut max_t: f32, mut f: impl FnMut(usize, f32) -> f32) {
        if self.nodes.is_empty() {
            return;
        }
        let inv = crate::math::inv_dir(dir);
        let mut stack = [0u32; 64];
        let mut top = 1;
        while top > 0 {
            top -= 1;
            let ni = stack[top];
            let node = self.nodes[ni as usize];
            if node.aabb.ray(origin, inv, max_t).is_none() {
                continue;
            }
            if node.count > 0 {
                for &t in &self.order[node.index as usize..(node.index + node.count) as usize] {
                    max_t = f(t as usize, max_t);
                }
            } else if top + 2 <= stack.len() {
                stack[top] = node.index;
                stack[top + 1] = ni + 1;
                top += 2;
            }
        }
    }
}

/// A grid of heights (static), centred on the local origin: sample `(x, z)` sits at
/// `((x - (columns-1)/2) · cell.0, heights[z·columns + x], (z - (rows-1)/2) · cell.1)`.
/// Each cell is two triangles split along the same diagonal.
#[derive(Clone, Debug)]
pub struct Heightfield {
    pub columns: usize,
    pub rows: usize,
    pub heights: Vec<f32>,
    pub cell: (f32, f32),
    pub mesh: TriMesh,
}

impl Heightfield {
    pub fn new(columns: usize, rows: usize, heights: Vec<f32>, cell: (f32, f32)) -> Result<Heightfield, PhysicsError> {
        let side = 2..=MAX_HEIGHTFIELD_SIDE;
        if !side.contains(&columns) || !side.contains(&rows) {
            return Err(PhysicsError::new(
                "invalid_heightfield",
                format!("a heightfield needs 2 to {MAX_HEIGHTFIELD_SIDE} samples per side (got {columns} x {rows})"),
            ));
        }
        if heights.len() != columns * rows {
            return Err(PhysicsError::new("invalid_heightfield", format!("{columns} x {rows} samples need {} heights (got {})", columns * rows, heights.len())));
        }
        if !heights.iter().all(|h| h.is_finite()) || !(cell.0 > 0.0 && cell.1 > 0.0 && cell.0.is_finite() && cell.1.is_finite()) {
            return Err(PhysicsError::new("invalid_heightfield", "heights must be finite and cell sizes positive"));
        }
        let x0 = -((columns - 1) as f32) * cell.0 * 0.5;
        let z0 = -((rows - 1) as f32) * cell.1 * 0.5;
        let mut vertices = Vec::with_capacity(columns * rows);
        for z in 0..rows {
            for x in 0..columns {
                vertices.push(v3(x0 + x as f32 * cell.0, heights[z * columns + x], z0 + z as f32 * cell.1));
            }
        }
        let mut triangles = Vec::with_capacity((columns - 1) * (rows - 1) * 2);
        for z in 0..rows - 1 {
            for x in 0..columns - 1 {
                let v00 = (z * columns + x) as u32;
                let v10 = v00 + 1;
                let v01 = v00 + columns as u32;
                let v11 = v01 + 1;
                triangles.push([v00, v01, v10]);
                triangles.push([v10, v01, v11]);
            }
        }
        let mesh = TriMesh::new(vertices, triangles)?;
        Ok(Heightfield { columns, rows, heights, cell, mesh })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_hull_has_six_faces_and_twelve_edges() {
        let Shape::Box { hull, .. } = Shape::cuboid(v3(1.0, 2.0, 3.0)) else { panic!() };
        assert_eq!((hull.points.len(), hull.face_count(), hull.edges.len()), (8, 6, 12));
        assert!((hull.volume - 48.0).abs() < 1e-3, "{}", hull.volume);
        assert!(hull.centroid.length() < 1e-5);
        // Inertia of the hull matches the box formula.
        let (_, i_box) = Shape::cuboid(v3(1.0, 2.0, 3.0)).mass_properties(48.0);
        let i_hull = hull.unit_inertia;
        for k in 0..3 {
            assert!((i_box.at(k, k) - i_hull.at(k, k)).abs() < 1e-2 * i_box.at(k, k), "{k}: {:?} vs {:?}", i_box, i_hull);
        }
        assert!((hull.inner_radius - 1.0).abs() < 1e-5);
        // Loops wind counter-clockwise seen from outside.
        for f in 0..hull.face_count() {
            let l = hull.face(f);
            let (a, b, c) = (hull.points[l[0] as usize], hull.points[l[1] as usize], hull.points[l[2] as usize]);
            assert!((b - a).cross(c - a).dot(hull.normals[f]) > 0.0);
        }
    }

    #[test]
    fn hulls_drop_interior_points_and_refuse_flat_input() {
        let mut pts = vec![v3(0.0, 0.0, 0.0), v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0), v3(0.0, 0.0, 1.0), v3(0.1, 0.1, 0.1)];
        let h = Hull::new(&pts).unwrap();
        assert_eq!((h.points.len(), h.face_count(), h.edges.len()), (4, 4, 6));
        assert!((h.volume - 1.0 / 6.0).abs() < 1e-5);
        pts.truncate(3);
        pts.push(v3(1.0, 1.0, 0.0));
        assert_eq!(Hull::new(&pts).unwrap_err().code, "invalid_hull");
        assert_eq!(Hull::new(&pts[..3]).unwrap_err().code, "invalid_hull");
    }

    #[test]
    fn mesh_edges_between_flat_neighbours_are_seams() {
        let v = vec![v3(0.0, 0.0, 0.0), v3(1.0, 0.0, 0.0), v3(1.0, 0.0, 1.0), v3(0.0, 0.0, 1.0)];
        let m = TriMesh::new(v, vec![[0, 2, 1], [0, 3, 2]]).unwrap();
        // Shared edge 0-2: edge 0 of triangle 0 and edge 2 of triangle 1.
        assert_eq!(m.edge_flags[0] & 1, 0);
        assert_eq!(m.edge_flags[1], 0b011);
        assert_eq!(m.edge_flags[0] & 0b110, 0b110);
        assert!(m.normals[0].y > 0.99);
        let mut seen = Vec::new();
        m.each_in(&Aabb::new(v3(0.9, -1.0, 0.0), v3(1.1, 1.0, 0.05)), |t| {
            seen.push(t);
            true
        });
        seen.sort_unstable();
        assert_eq!(seen, vec![0, 1]);
        m.each_in(&Aabb::new(v3(0.9, -1.0, -0.5), v3(1.1, 1.0, -0.1)), |t| {
            seen.push(t);
            true
        });
        assert_eq!(seen.len(), 2, "nothing past the mesh");
    }

    #[test]
    fn heightfields_cover_their_grid_and_bvh_finds_every_triangle() {
        let hf = Heightfield::new(9, 5, (0..45).map(|i| (i % 7) as f32 * 0.1).collect(), (1.0, 2.0)).unwrap();
        assert_eq!(hf.mesh.triangles.len(), 8 * 4 * 2);
        assert!((hf.mesh.aabb.min.x + 4.0).abs() < 1e-6 && (hf.mesh.aabb.max.z - 4.0).abs() < 1e-6);
        assert!(hf.mesh.normals.iter().all(|n| n.y > 0.0));
        let mut count = 0;
        hf.mesh.each_in(&hf.mesh.aabb, |_| {
            count += 1;
            true
        });
        assert_eq!(count, 64);
    }
}
