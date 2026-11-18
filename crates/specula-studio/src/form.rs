//! Form, measured: what a trained eye sees in a mesh's shape, as numbers (the form checks of
//! docs/design/FORM.md). Loose pieces, edges sharp as a razor, how coarsely a round surface
//! is faceted, where the shading jumps across an edge that is not a crease, and how much of
//! the shape's relief is big, medium and small.
//!
//! Everything here is read from the finest level of the built mesh alone: positions, normals
//! and triangles. It says what is there, never whether it is meant: a rule in `style.toml`
//! (`[form]`) turns a measure into a finding, and `[form.allow]` says what is deliberate.

use specula_core::Json;
use specula_model::MeshData;
use std::collections::BTreeMap;

type V3 = [f32; 3];

/// An edge whose two faces turn more than this is a crease (sharp on purpose or not).
pub const CREASE_DEG: f32 = 30.0;
/// A crease that turns more than this with nothing to soften it is a razor: no made thing has
/// an edge that sharp (a bevel splits a right angle into two of 45°, which pass).
pub const RAZOR_DEG: f32 = 75.0;
/// Across an edge that is not a crease, shading normals further apart than this are a seam.
pub const SEAM_DEG: f32 = 15.0;
/// A mesh of more triangles than this is not measured (the facts say so).
pub const MAX_TRIANGLES: usize = 600_000;
/// Points the relief is sampled at, at most (a mesh of large triangles is sampled within
/// each; one of many small ones once a triangle).
pub const MAX_SAMPLES: f64 = 1_500_000.0;
/// More pieces than this are counted and not compared with each other (a field of grass).
pub const MAX_PIECES: usize = 2_000;
/// Loose pieces listed (the largest; all are counted).
pub const MAX_LISTED: usize = 8;

/// The form checks `[form.allow]` may exempt.
pub const CHECKS: [&str; 6] = ["loose_pieces", "razor_edges", "faceting", "shading_seams", "smeared_creases", "small_relief"];

/// A piece of the mesh that touches no other piece and does not reach the ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loose {
    pub triangles: usize,
    /// The middle of its bounds.
    pub at: V3,
    /// The longest side of its bounds, metres.
    pub size_m: f32,
    /// How far its bounds are from the nearest other piece's, metres.
    pub gap_m: f32,
}

/// What was measured of a mesh's form ([`measure`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormFacts {
    pub triangles: usize,
    /// Why nothing below was measured (too many triangles), if so.
    pub skipped: Option<&'static str>,
    /// Pieces: sets of triangles joined through shared corners.
    pub pieces: usize,
    /// Pieces whose bounds touch no other piece's and do not reach the ground, y = 0 (`None`:
    /// too many pieces to compare).
    pub loose_count: Option<usize>,
    pub loose: Vec<Loose>,
    /// Length of the edges two triangles share, metres.
    pub edge_m: f32,
    /// Length of the edges only one triangle has (the rim of an open sheet), metres.
    pub open_m: f32,
    /// Length of the edges more than two triangles share, metres.
    pub tangled_m: f32,
    /// Length of the creases (edges turning more than [`CREASE_DEG`]), metres.
    pub crease_m: f32,
    /// Length of the razors (creases turning more than [`RAZOR_DEG`]), metres.
    pub razor_m: f32,
    /// How coarsely round surfaces are faceted: the turn, in degrees, that 95% of the length
    /// of the edges that are not creases stays under. Above a few degrees an outline reads as
    /// a polygon.
    pub facet_deg: f32,
    /// Length of the edges that are not creases across which the shading normals jump by more
    /// than [`SEAM_DEG`]: a line in the light where the surface has none.
    pub seam_m: f32,
    /// Length of the creases shaded as if they were round (one normal for both faces): the
    /// dark smear along a hard edge.
    pub smeared_m: f32,
    /// How the shape's relief is shared between big, medium and small form: how much the
    /// surface's facing varies within a quarter, a sixteenth and a sixty-fourth of its size.
    pub relief: [f32; 3],
}

impl FormFacts {
    /// The share of the creases' length that is razor-sharp (0 with no crease).
    pub fn razor_share(&self) -> f32 {
        if self.crease_m > 0.0 {
            self.razor_m / self.crease_m
        } else {
            0.0
        }
    }

    pub fn to_json(&self) -> Json {
        let r3 = |x: f32| Json::from((f64::from(x) * 1000.0).round() / 1000.0);
        let mut j = Json::obj([("triangles", Json::from(self.triangles))]);
        if let Some(why) = self.skipped {
            j.set("skipped", Json::from(why));
            return j;
        }
        j.set("pieces", Json::from(self.pieces));
        match self.loose_count {
            Some(n) => {
                j.set("loose_pieces", Json::from(n));
                if n > 0 {
                    let list = self.loose.iter().map(|l| {
                        Json::obj([
                            ("triangles", Json::from(l.triangles)),
                            ("at", Json::arr(l.at.map(r3))),
                            ("size_m", r3(l.size_m)),
                            ("gap_m", r3(l.gap_m)),
                        ])
                    });
                    j.set("loose", Json::arr(list));
                }
            }
            None => {
                j.set("loose_pieces", Json::from("not compared: too many pieces"));
            }
        }
        j.set("edge_m", r3(self.edge_m));
        j.set("open_edge_m", r3(self.open_m));
        j.set("tangled_edge_m", r3(self.tangled_m));
        j.set("crease_m", r3(self.crease_m));
        j.set("razor_m", r3(self.razor_m));
        j.set("razor_share", r3(self.razor_share()));
        j.set("facet_deg", r3(self.facet_deg));
        j.set("seam_m", r3(self.seam_m));
        j.set("smeared_crease_m", r3(self.smeared_m));
        j.set("relief", Json::obj([("big", r3(self.relief[0])), ("medium", r3(self.relief[1])), ("small", r3(self.relief[2]))]));
        j
    }
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn length(a: V3) -> f32 {
    dot(a, a).sqrt()
}

/// The angle between two directions, degrees (0 when either has no length).
fn angle_deg(a: V3, b: V3) -> f32 {
    let l = length(a) * length(b);
    if l <= 0.0 {
        return 0.0;
    }
    specula_core::dmath::acos((dot(a, b) / l).clamp(-1.0, 1.0)).to_degrees()
}

/// One triangle's use of an edge: its face (twice its area along its normal) and the shading
/// normals it has at the edge's two ends (in the edge's own order).
#[derive(Clone, Copy)]
struct Side {
    /// Which triangle (its place in the list of those kept).
    tri: u32,
    face: V3,
    shade: [V3; 2],
}

fn find(parent: &mut [u32], mut x: u32) -> u32 {
    while parent[x as usize] != x {
        parent[x as usize] = parent[parent[x as usize] as usize];
        x = parent[x as usize];
    }
    x
}

/// Measures a mesh's form. Work and memory go by its triangles, of which there are at most
/// [`MAX_TRIANGLES`] (more: `skipped`).
pub fn measure(m: &MeshData) -> FormFacts {
    let triangles = m.indices.len() / 3;
    let mut f = FormFacts { triangles, ..FormFacts::default() };
    if triangles > MAX_TRIANGLES {
        f.skipped = Some("more triangles than the form measures take");
        return f;
    }
    // Corners at one place are one corner, whatever their normals (a hard edge splits them).
    let mut place: BTreeMap<[u32; 3], u32> = BTreeMap::new();
    let mut points: Vec<V3> = Vec::new();
    let weld: Vec<u32> = m
        .positions
        .iter()
        .map(|p| {
            // (-0.0 and 0.0 are one place.)
            let key = p.map(|x| (x + 0.0).to_bits());
            *place.entry(key).or_insert_with(|| {
                points.push(*p);
                (points.len() - 1) as u32
            })
        })
        .collect();
    // Triangles with three places and an area. A sliver (an area under a millionth of a
    // millionth of the square of the shape's size: three corners nearly in a line, or nearly
    // one place) is left out with them: its facing is noise, and the edges it would make
    // razors of are no edges anyone sees.
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &points {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let size = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-6);
    let least = 2.0e-12 * size * size;
    let mut tris: Vec<([u32; 3], [u32; 3], V3)> = Vec::with_capacity(triangles);
    for t in m.indices.as_chunks::<3>().0 {
        let (Some(a), Some(b), Some(c)) = (weld.get(t[0] as usize), weld.get(t[1] as usize), weld.get(t[2] as usize)) else { continue };
        if a == b || b == c || a == c {
            continue;
        }
        let face = cross(sub(points[*b as usize], points[*a as usize]), sub(points[*c as usize], points[*a as usize]));
        // (A cross product is twice the area.)
        let twice = length(face);
        if twice.is_nan() || twice <= least {
            continue;
        }
        tris.push(([*a, *b, *c], *t, face));
    }
    if tris.is_empty() {
        return f;
    }

    // ---- pieces, and which are loose
    let mut parent: Vec<u32> = (0..points.len() as u32).collect();
    for (w, _, _) in &tris {
        let (a, b, c) = (find(&mut parent, w[0]), find(&mut parent, w[1]), find(&mut parent, w[2]));
        parent[b as usize] = a;
        parent[c as usize] = a;
    }
    // (root -> triangles, bounds), in the order of the roots' numbers.
    let mut pieces: BTreeMap<u32, (usize, V3, V3)> = BTreeMap::new();
    for (w, _, _) in &tris {
        let root = find(&mut parent, w[0]);
        let e = pieces.entry(root).or_insert((0, [f32::MAX; 3], [f32::MIN; 3]));
        e.0 += 1;
        for v in w {
            let p = points[*v as usize];
            for k in 0..3 {
                e.1[k] = e.1[k].min(p[k]);
                e.2[k] = e.2[k].max(p[k]);
            }
        }
    }
    f.pieces = pieces.len();
    if pieces.len() <= MAX_PIECES {
        // Two pieces touch when their bounds come within a hundredth of the shape's size (and
        // at least a millimetre) of each other: bounds, so a ring round a post it never
        // touches counts as touching it, and a loose piece is loose beyond doubt. A piece that
        // reaches the ground (y = 0, where an asset stands) rests on it and is not loose: a
        // coffin's lid lying beside it, rubble round a fallen stone.
        let near = (0.01 * size).max(0.001);
        let list: Vec<(usize, V3, V3)> = pieces.values().copied().collect();
        let mut loose = Vec::new();
        for (i, (count, a_lo, a_hi)) in list.iter().enumerate() {
            let mut gap = f32::MAX;
            for (j, (_, b_lo, b_hi)) in list.iter().enumerate() {
                if i == j {
                    continue;
                }
                let d = [0, 1, 2].map(|k| (b_lo[k] - a_hi[k]).max(a_lo[k] - b_hi[k]).max(0.0));
                gap = gap.min(length(d));
            }
            if list.len() > 1 && gap > near && a_lo[1] > near {
                let mid = [0, 1, 2].map(|k| 0.5 * (a_lo[k] + a_hi[k]));
                loose.push(Loose { triangles: *count, at: mid, size_m: (a_hi[0] - a_lo[0]).max(a_hi[1] - a_lo[1]).max(a_hi[2] - a_lo[2]), gap_m: gap });
            }
        }
        f.loose_count = Some(loose.len());
        // The largest first; ties by where they are.
        loose.sort_by(|a, b| b.triangles.cmp(&a.triangles).then(a.at[0].total_cmp(&b.at[0])).then(a.at[1].total_cmp(&b.at[1])).then(a.at[2].total_cmp(&b.at[2])));
        loose.truncate(MAX_LISTED);
        f.loose = loose;
    }

    // ---- edges
    let normal = |i: u32| m.normals.get(i as usize).copied().unwrap_or([0.0; 3]);
    let mut edges: BTreeMap<(u32, u32), Vec<Side>> = BTreeMap::new();
    for (i, (w, raw, face)) in tris.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (k, (k + 1) % 3);
            let (key, shade) = if w[a] < w[b] { ((w[a], w[b]), [normal(raw[a]), normal(raw[b])]) } else { ((w[b], w[a]), [normal(raw[b]), normal(raw[a])]) };
            edges.entry(key).or_default().push(Side { tri: i as u32, face: *face, shade });
        }
    }
    // (The turns across edges that are not creases, with each edge's length, for the 95th
    // percentile by length.)
    let mut turns: Vec<(f32, f32)> = Vec::new();
    for ((a, b), sides) in &edges {
        let len = length(sub(points[*a as usize], points[*b as usize]));
        match sides.as_slice() {
            [_] => f.open_m += len,
            [s, t] => {
                f.edge_m += len;
                let turn = angle_deg(s.face, t.face);
                // How far apart the two triangles are shaded at the edge's ends.
                let jump = angle_deg(s.shade[0], t.shade[0]).max(angle_deg(s.shade[1], t.shade[1]));
                if turn > CREASE_DEG {
                    f.crease_m += len;
                    if turn > RAZOR_DEG {
                        f.razor_m += len;
                    }
                    if jump < 1.0 {
                        f.smeared_m += len;
                    }
                } else {
                    turns.push((turn, len));
                    if jump > SEAM_DEG {
                        f.seam_m += len;
                    }
                }
            }
            _ => f.tangled_m += len,
        }
    }
    turns.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let total: f64 = turns.iter().map(|t| f64::from(t.1)).sum();
    let mut run = 0.0f64;
    for (turn, len) in &turns {
        run += f64::from(*len);
        if run >= 0.95 * total {
            f.facet_deg = *turn;
            break;
        }
    }

    // ---- relief, at three sizes
    // How much the surface's facing varies within a box of each size: 0 where every face in
    // the box faces one way. A triangle is in every box a point of it is in (it is sampled
    // more finely than the smallest box, within a budget). What is in a box is taken sheet by
    // sheet (the triangles in it that are joined through edges), so the two sides of a thin
    // plate, which lie in one box and are joined only round its rim, are not taken for one
    // surface that turns right round.
    let joins: Vec<(u32, u32)> = edges.values().filter_map(|sides| if let [s, t] = sides.as_slice() { Some((s.tri, t.tri)) } else { None }).collect();
    // Points along each side of each triangle: enough to be finer than the smallest box, and
    // fewer of them all round when that would be more than the budget.
    let fine = size / 128.0;
    let want: Vec<u32> = tris
        .iter()
        .map(|(w, _, _)| {
            let p = [points[w[0] as usize], points[w[1] as usize], points[w[2] as usize]];
            let longest = length(sub(p[1], p[0])).max(length(sub(p[2], p[1]))).max(length(sub(p[0], p[2])));
            ((longest / fine).ceil() as u32).clamp(1, 64)
        })
        .collect();
    let asked: f64 = want.iter().map(|n| f64::from(n * (n + 1) / 2)).sum();
    let thin = if asked > MAX_SAMPLES { (MAX_SAMPLES / asked).sqrt() } else { 1.0 };
    let roughness = |cells: f32| -> f64 {
        let cell = size / cells;
        // (triangle, box) -> the share of the triangle that is in the box, by its points.
        let mut within: BTreeMap<(u32, [i32; 3]), f32> = BTreeMap::new();
        for (i, (w, _, _)) in tris.iter().enumerate() {
            let p = [points[w[0] as usize], points[w[1] as usize], points[w[2] as usize]];
            let n = ((f64::from(want[i]) * thin).floor() as u32).max(1);
            let each = 1.0 / (n * (n + 1) / 2) as f32;
            for a in 0..n {
                for b in 0..n - a {
                    let (u, v) = ((a as f32 + 1.0 / 3.0) / n as f32, (b as f32 + 1.0 / 3.0) / n as f32);
                    let q = [0, 1, 2].map(|k| p[0][k] + (p[1][k] - p[0][k]) * u + (p[2][k] - p[0][k]) * v);
                    let at = [0, 1, 2].map(|k| ((q[k] - lo[k]) / cell).floor() as i32);
                    *within.entry((i as u32, at)).or_insert(0.0) += each;
                }
            }
        }
        // Each (triangle, box) is a node; two are one sheet when their triangles share an edge
        // and both are in the box.
        let nodes: Vec<(u32, [i32; 3])> = within.keys().copied().collect();
        let first_of = |tri: u32| nodes.partition_point(|n| n.0 < tri);
        let mut sheet: Vec<u32> = (0..nodes.len() as u32).collect();
        for (a, b) in &joins {
            let (mut i, mut j) = (first_of(*a), first_of(*b));
            // (Both lists are in the boxes' order: walked together.)
            while i < nodes.len() && j < nodes.len() && nodes[i].0 == *a && nodes[j].0 == *b {
                match nodes[i].1.cmp(&nodes[j].1) {
                    std::cmp::Ordering::Less => i += 1,
                    std::cmp::Ordering::Greater => j += 1,
                    std::cmp::Ordering::Equal => {
                        let (ra, rb) = (find(&mut sheet, i as u32), find(&mut sheet, j as u32));
                        sheet[rb as usize] = ra;
                        i += 1;
                        j += 1;
                    }
                }
            }
        }
        // (Each sheet's faces summed: a face's cross product is its normal times twice its area.)
        let mut sums: Vec<(V3, f32)> = vec![([0.0; 3], 0.0); nodes.len()];
        for (i, node) in nodes.iter().enumerate() {
            let (face, share) = (tris[node.0 as usize].2, within[node]);
            let e = &mut sums[find(&mut sheet, i as u32) as usize];
            for k in 0..3 {
                e.0[k] += face[k] * share;
            }
            e.1 += length(face) * share;
        }
        let (together, apart): (f64, f64) = sums.iter().fold((0.0, 0.0), |(t, a), (sum, area)| (t + f64::from(length(*sum)), a + f64::from(*area)));
        if apart > 0.0 {
            (1.0 - together / apart).clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    let (quarter, sixteenth, sixty_fourth) = (roughness(4.0), roughness(16.0), roughness(64.0));
    let bands = [(quarter - sixteenth).max(0.0), (sixteenth - sixty_fourth).max(0.0), sixty_fourth];
    let all: f64 = bands.iter().sum();
    if all > 1e-9 {
        f.relief = bands.map(|b| (b / all) as f32);
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mesh from quads (each four corners, counter-clockwise from outside), flat shaded: each
    /// quad has its own four vertices with its own normal.
    fn flat(quads: &[[V3; 4]]) -> MeshData {
        let mut m = MeshData::default();
        for q in quads {
            let n = cross(sub(q[1], q[0]), sub(q[2], q[0]));
            let l = length(n).max(1e-12);
            let n = n.map(|x| x / l);
            let first = m.positions.len() as u32;
            for p in q {
                m.positions.push(*p);
                m.normals.push(n);
            }
            m.indices.extend_from_slice(&[first, first + 1, first + 2, first, first + 2, first + 3]);
        }
        m
    }

    /// The six faces of a box from `lo` to `hi`.
    fn box_quads(lo: V3, hi: V3) -> Vec<[V3; 4]> {
        let p = |x: bool, y: bool, z: bool| [if x { hi[0] } else { lo[0] }, if y { hi[1] } else { lo[1] }, if z { hi[2] } else { lo[2] }];
        vec![
            [p(true, false, false), p(true, true, false), p(true, true, true), p(true, false, true)],
            [p(false, false, true), p(false, true, true), p(false, true, false), p(false, false, false)],
            [p(false, true, false), p(false, true, true), p(true, true, true), p(true, true, false)],
            [p(false, false, true), p(false, false, false), p(true, false, false), p(true, false, true)],
            [p(true, false, true), p(true, true, true), p(false, true, true), p(false, false, true)],
            [p(false, false, false), p(false, true, false), p(true, true, false), p(true, false, false)],
        ]
    }

    /// A ball of `rings` bands, smooth shaded (shared vertices, normals from the middle).
    fn ball(radius: f32, rings: u32, at: V3) -> MeshData {
        let mut m = MeshData::default();
        let around = rings * 2;
        for i in 0..=rings {
            let lat = std::f32::consts::PI * i as f32 / rings as f32;
            for j in 0..around {
                let lon = std::f32::consts::TAU * j as f32 / around as f32;
                // (The poles are one point each, exactly.)
                let n = if i == 0 {
                    [0.0, 1.0, 0.0]
                } else if i == rings {
                    [0.0, -1.0, 0.0]
                } else {
                    [specula_core::dmath::sin(lat) * specula_core::dmath::cos(lon), specula_core::dmath::cos(lat), specula_core::dmath::sin(lat) * specula_core::dmath::sin(lon)]
                };
                m.positions.push([at[0] + radius * n[0], at[1] + radius * n[1], at[2] + radius * n[2]]);
                m.normals.push(n);
            }
        }
        for i in 0..rings {
            for j in 0..around {
                let (a, b) = (i * around + j, i * around + (j + 1) % around);
                let (c, d) = (a + around, b + around);
                m.indices.extend_from_slice(&[a, b, c, b, d, c]);
            }
        }
        m
    }

    fn joined(a: &MeshData, b: &MeshData) -> MeshData {
        let mut m = a.clone();
        let first = m.positions.len() as u32;
        m.positions.extend_from_slice(&b.positions);
        m.normals.extend_from_slice(&b.normals);
        m.indices.extend(b.indices.iter().map(|i| i + first));
        m
    }

    /// A hard box is all razors: twelve edges, each a right angle with no bevel, none of them
    /// smeared (its faces are flat shaded) and nothing loose, open or tangled.
    #[test]
    fn a_hard_box_is_all_razors() {
        let f = measure(&flat(&box_quads([0.0; 3], [2.0, 1.0, 0.5])));
        assert_eq!((f.triangles, f.pieces, f.loose_count), (12, 1, Some(0)));
        assert!((f.crease_m - 4.0 * (2.0 + 1.0 + 0.5)).abs() < 1e-4 && (f.razor_m - f.crease_m).abs() < 1e-4, "{f:?}");
        assert_eq!(f.razor_share(), 1.0);
        assert!(f.open_m == 0.0 && f.tangled_m == 0.0 && f.seam_m == 0.0 && f.smeared_m == 0.0, "{f:?}");
        // Its faces' own diagonals turn nothing.
        assert_eq!(f.facet_deg, 0.0);
    }

    /// The same box shaded as if it were round (one normal a corner) is all smeared creases;
    /// and a flat sheet whose two halves are shaded 20° apart has a seam down its middle.
    #[test]
    fn shading_that_disagrees_with_the_shape_is_found() {
        let mut m = MeshData::default();
        let (lo, hi) = ([-1.0f32; 3], [1.0f32; 3]);
        for i in 0..8 {
            let p = [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
            m.positions.push(p);
            m.normals.push(p.map(|x| x / 3f32.sqrt()));
        }
        for q in [[1u32, 3, 7, 5], [4, 6, 2, 0], [2, 6, 7, 3], [4, 0, 1, 5], [5, 7, 6, 4], [0, 2, 3, 1]] {
            m.indices.extend_from_slice(&[q[0], q[1], q[2], q[0], q[2], q[3]]);
        }
        let f = measure(&m);
        assert!((f.crease_m - 24.0).abs() < 1e-4 && (f.smeared_m - 24.0).abs() < 1e-4, "{f:?}");

        // Two quads in one plane, each with its own normals, leaning 10° either way.
        let lean = |deg: f32| [specula_core::dmath::sin(deg.to_radians()), specula_core::dmath::cos(deg.to_radians()), 0.0];
        let mut s = MeshData::default();
        for (x0, n) in [(-1.0f32, lean(-10.0)), (0.0, lean(10.0))] {
            let first = s.positions.len() as u32;
            for p in [[x0, 0.0, 1.0], [x0 + 1.0, 0.0, 1.0], [x0 + 1.0, 0.0, 0.0], [x0, 0.0, 0.0]] {
                s.positions.push(p);
                s.normals.push(n);
            }
            s.indices.extend_from_slice(&[first, first + 1, first + 2, first, first + 2, first + 3]);
        }
        let f = measure(&s);
        assert!((f.seam_m - 1.0).abs() < 1e-5 && f.crease_m == 0.0, "{f:?}");
        assert!((f.open_m - 6.0).abs() < 1e-5, "an open sheet has a rim: {f:?}");
    }

    /// A ball has no crease; how coarsely it is faceted is the turn between its bands, which
    /// halves when the bands double; and all of its relief is big.
    #[test]
    fn a_ball_is_faceted_by_its_bands() {
        let coarse = measure(&ball(1.0, 8, [0.0; 3]));
        let fine = measure(&ball(1.0, 32, [0.0; 3]));
        assert!(coarse.crease_m == 0.0 && fine.crease_m == 0.0 && coarse.seam_m == 0.0, "{coarse:?}");
        assert!((coarse.facet_deg - 22.5).abs() < 1.5 && (fine.facet_deg - 5.625).abs() < 0.6, "{} and {}", coarse.facet_deg, fine.facet_deg);
        assert!(fine.relief[0] > 0.8 && fine.relief[2] < 0.05, "{:?}", fine.relief);
        assert!((fine.relief.iter().sum::<f32>() - 1.0).abs() < 1e-5);
    }

    /// Relief goes by size. A ball's is all big; bumps a thirtieth of its size across put most
    /// of it in the small and the rest in the medium; and a sheet ribbed a hundred times
    /// across has nearly all of its relief in the small. Slivers at a ball's pole (corners a ten-millionth apart) make
    /// no razors.
    #[test]
    fn relief_goes_by_size() {
        let plain = ball(1.0, 48, [0.0; 3]);
        let mut bumpy = plain.clone();
        for (i, p) in bumpy.positions.iter_mut().enumerate() {
            // (A fixed pattern, every other corner out by 2%.)
            let out = if (i * 7 + i / 96) % 2 == 0 { 1.02 } else { 0.98 };
            *p = p.map(|x| x * out);
        }
        let (a, b) = (measure(&plain), measure(&bumpy));
        assert!(a.relief[0] > 0.9 && b.relief[2] > 0.5 && b.relief[0] < 0.1, "{:?} bumpy, {:?} plain", b.relief, a.relief);
        // (Bumps that steep are creases: the plain ball has none.)
        assert!(a.crease_m == 0.0 && b.crease_m > 10.0, "{} and {}", a.crease_m, b.crease_m);

        // A sheet one metre square, ribbed: a hundred ridges of 45° across it.
        let quads: Vec<[V3; 4]> = (0..200)
            .map(|i| {
                let (x0, x1) = (i as f32 * 0.005, (i + 1) as f32 * 0.005);
                let (y0, y1) = if i % 2 == 0 { (0.0, 0.005) } else { (0.005, 0.0) };
                [[x0, y0, 1.0], [x1, y1, 1.0], [x1, y1, 0.0], [x0, y0, 0.0]]
            })
            .collect();
        let ribbed = measure(&flat(&quads));
        assert!(ribbed.relief[2] > 0.9, "{:?}", ribbed.relief);
        // (Every ridge is a right angle with no bevel: all razors.)
        assert!((ribbed.razor_share() - 1.0).abs() < 1e-6 && (ribbed.crease_m - 199.0).abs() < 1e-2, "{ribbed:?}");

        let mut pinched = ball(1.0, 8, [0.0; 3]);
        let around = 16;
        for j in 0..around {
            // The south pole's ring, opened to a ten-millionth of a metre across.
            let lon = std::f32::consts::TAU * j as f32 / around as f32;
            let at = pinched.positions.len() - around + j;
            pinched.positions[at] = [1.0e-7 * specula_core::dmath::cos(lon), -1.0, 1.0e-7 * specula_core::dmath::sin(lon)];
        }
        let f = measure(&pinched);
        assert!(f.razor_m == 0.0 && f.crease_m == 0.0 && f.smeared_m == 0.0, "{f:?}");
    }

    /// A piece that touches nothing and hangs in the air is loose, with where it is and how
    /// far off; pieces that touch, or rest on the ground, are not; and a mesh too large is not
    /// measured, and says so.
    #[test]
    fn a_piece_that_touches_nothing_is_loose() {
        let body = flat(&box_quads([0.0; 3], [1.0; 3]));
        let touching = joined(&body, &flat(&box_quads([1.0, 0.2, 0.2], [1.5, 0.6, 0.6])));
        let f = measure(&touching);
        assert_eq!((f.pieces, f.loose_count), (2, Some(0)));
        // (A lid lying on the ground beside it touches nothing and is not loose.)
        let with_lid = joined(&touching, &flat(&box_quads([2.0, 0.0, 0.0], [3.0, 0.1, 1.0])));
        let f = measure(&with_lid);
        assert_eq!((f.pieces, f.loose_count), (3, Some(0)));
        let adrift = joined(&with_lid, &ball(0.05, 6, [0.5, 2.0, 0.5]));
        let f = measure(&adrift);
        assert_eq!((f.pieces, f.loose_count), (4, Some(1)));
        let l = f.loose[0];
        assert!((l.gap_m - 0.95).abs() < 0.01 && (l.at[1] - 2.0).abs() < 1e-4 && (l.size_m - 0.1).abs() < 0.01 && l.triangles == 120, "{l:?}");
        let j = f.to_json();
        assert_eq!(j.get("loose_pieces").and_then(Json::as_u64), Some(1));
        assert!(j.get("loose").and_then(Json::as_array).is_some_and(|a| a.len() == 1));

        let big = MeshData { indices: vec![0; (MAX_TRIANGLES + 1) * 3], ..MeshData::default() };
        let f = measure(&big);
        assert!(f.skipped.is_some() && f.to_json().get("skipped").is_some());
        assert_eq!(measure(&MeshData::default()), FormFacts::default());
    }
}
