//! A probe field: the lattice, each probe's irradiance, offset and weight, each cell's visibility
//! mask, the GPU record they pack into, and the sampler (the CPU twin of `shaders/gi.glsl`).

use crate::half;
use crate::sh::Sh;
use specula_core::dmath;
use specula_core::math::Vec3;

/// The sample point moves this many spacings off the surface along its normal...
pub const NORMAL_BIAS: f32 = 0.25;
/// ...and this many towards the viewer.
pub const VIEW_BIAS: f32 = 0.05;
/// Weights never fall below this share of the trilinear weight (visible, valid probes only).
pub const WEIGHT_FLOOR: f32 = 1e-4;
/// The share of a point's (valid) trilinear weight its visible probes must carry for their
/// irradiance to count in full; below it the result dims in proportion. A wall right beside a point
/// can leave only a far probe visible at a sliver of weight, and normalizing it made a strip of a
/// closed room as bright as outside. A point beside a wall on the open side keeps at least this
/// share: the wall sits at least the normal bias (a quarter spacing) behind it.
pub const VISIBLE_SHARE: f32 = 0.25;
/// A mask that hides nothing (cells on a lattice's far faces, which no point samples).
pub const ALL_VISIBLE: u64 = u64::MAX;

/// The bit of a cell's mask that says whether its `corner` probe (x | y << 1 | z << 2) can be
/// seen from the centre of its `octant` (the cell's half in x | y << 1 | z << 2).
pub fn mask_bit(octant: u32, corner: u32) -> u64 {
    1u64 << (octant * 8 + corner)
}

/// A world-aligned grid of probes: probe `(i, j, k)` sits at `origin + (i, j, k) × spacing`
/// (before its offset).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lattice {
    pub origin: [f32; 3],
    pub spacing: f32,
    pub dims: [u32; 3],
}

impl Lattice {
    /// The lattice covering `[min, max]` at `spacing`, on the world grid of probes at half
    /// spacings (`(n + ½) × spacing`): floors and walls at whole metres never pass through a
    /// probe, and two volumes' probes coincide where they overlap. At least 2 probes an axis.
    pub fn covering(min: [f32; 3], max: [f32; 3], spacing: f32) -> Lattice {
        let mut origin = [0.0; 3];
        let mut dims = [1u32; 3];
        for a in 0..3 {
            let lo = dmath::floor(min[a] / spacing - 0.5);
            let hi = dmath::ceil(max[a] / spacing - 0.5);
            origin[a] = (lo + 0.5) * spacing;
            dims[a] = ((hi - lo).max(0.0) as u32 + 1).max(2);
        }
        Lattice { origin, spacing, dims }
    }

    pub fn count(&self) -> usize {
        self.dims[0] as usize * self.dims[1] as usize * self.dims[2] as usize
    }

    /// Index of probe `(i, j, k)`: x fastest, then z, then y (so a column of cells in x and z,
    /// a streaming chunk, is a few contiguous runs per layer).
    pub fn index(&self, i: u32, j: u32, k: u32) -> usize {
        (j as usize * self.dims[2] as usize + k as usize) * self.dims[0] as usize + i as usize
    }

    pub fn coords(&self, index: usize) -> [u32; 3] {
        let nx = self.dims[0] as usize;
        let nz = self.dims[2] as usize;
        [(index % nx) as u32, (index / (nx * nz)) as u32, ((index / nx) % nz) as u32]
    }

    /// Where probe `(i, j, k)` sits before its offset.
    pub fn at(&self, c: [u32; 3]) -> Vec3 {
        Vec3::new(
            self.origin[0] + c[0] as f32 * self.spacing,
            self.origin[1] + c[1] as f32 * self.spacing,
            self.origin[2] + c[2] as f32 * self.spacing,
        )
    }

    /// Where probe `index` sits before its offset.
    pub fn position(&self, index: usize) -> Vec3 {
        self.at(self.coords(index))
    }

    /// The box the probes span.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut hi = self.origin;
        for (a, h) in hi.iter_mut().enumerate() {
            *h += (self.dims[a] - 1) as f32 * self.spacing;
        }
        (self.origin, hi)
    }
}

/// One probe, in floats.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Probe {
    pub sh: Sh,
    /// The visibility of the cell whose lowest corner this probe is (see [`mask_bit`]): a wall
    /// through the cell hides the probes on its far side from the octants on its near side.
    pub mask: u64,
    /// Where the probe moved to get out of geometry (metres).
    pub offset: [f32; 3],
    /// 1 = used, 0 = left inside geometry.
    pub weight: f32,
    /// How far the probe sees before a surface along [`REACH_DIRS`] (the six axes, then the eight
    /// cube diagonals), in eighths of the spacing, 0..14 ([`REACH_NONE`]: nothing within 15/8). A
    /// point further than that along the direction nearest to it, from the probe, is behind a wall
    /// ([`Probe::reaches`]): the cells' masks are exact only at octant centres, and a thin wall
    /// through an octant (its centre in the wall) let light into closed rooms.
    pub reach: [u8; 14],
}

/// [`Probe::reach`] when no surface is within 15/8 spacings.
pub const REACH_NONE: u8 = 15;

const D: f32 = 0.577_350_26; // 1/sqrt(3)

/// The reach directions: +x, -x, +y, -y, +z, -z, then the diagonals with the sign bits
/// (x negative = 1, y = 2, z = 4) of their index minus 6.
pub const REACH_DIRS: [[f32; 3]; 14] = [
    [1.0, 0.0, 0.0],
    [-1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, -1.0, 0.0],
    [0.0, 0.0, 1.0],
    [0.0, 0.0, -1.0],
    [D, D, D],
    [-D, D, D],
    [D, -D, D],
    [-D, -D, D],
    [D, D, -D],
    [-D, D, -D],
    [D, -D, -D],
    [-D, -D, -D],
];

/// The reach directions nearest to `d` (not normalized): the nearest axis and the nearest
/// diagonal, each with how far `d` goes along it, the nearer first.
pub fn reach_dirs(d: Vec3) -> [(usize, f32); 2] {
    let (ax, ay, az) = (d.x.abs(), d.y.abs(), d.z.abs());
    let face = ax.max(ay).max(az);
    let diag = (ax + ay + az) * D;
    let k = if ax >= ay && ax >= az {
        usize::from(d.x < 0.0)
    } else if ay >= az {
        2 + usize::from(d.y < 0.0)
    } else {
        4 + usize::from(d.z < 0.0)
    };
    let kd = 6 + usize::from(d.x < 0.0) + 2 * usize::from(d.y < 0.0) + 4 * usize::from(d.z < 0.0);
    if face >= diag {
        [(k, face), (kd, diag)]
    } else {
        [(kd, diag), (k, face)]
    }
}

/// The reach direction nearest to `d` and how far `d` goes along it.
pub fn reach_dir(d: Vec3) -> (usize, f32) {
    reach_dirs(d)[0]
}

/// When the second-nearest reach direction is this close to the nearest (by how far the point
/// goes along each), both must reach: a point between an axis and a diagonal is judged by both.
pub const REACH_TIE: f32 = 0.95;

impl Default for Probe {
    fn default() -> Self {
        Probe { sh: Sh::default(), mask: ALL_VISIBLE, offset: [0.0; 3], weight: 0.0, reach: [REACH_NONE; 14] }
    }
}

/// The GPU's record of a probe: 12 words (`shaders/gi.glsl` reads the same): `[0..6)` the SH as
/// half pairs (c0 rgb, c1x rgb, c1y rgb, c1z rgb), `[6..8)` offset xyz and weight as halves,
/// `[8..10)` its cell's visibility mask (low word first), `[10..12)` zero. A sample reads the
/// first two vec4s of eight probes and the third of one (its cell's).
pub type Record = [u32; 12];

impl Probe {
    pub fn pack(&self) -> Record {
        let s = &self.sh;
        let h = [s.c0[0], s.c0[1], s.c0[2], s.c1[0][0], s.c1[0][1], s.c1[0][2], s.c1[1][0], s.c1[1][1], s.c1[1][2], s.c1[2][0], s.c1[2][1], s.c1[2][2]];
        [
            half::pack2(h[0], h[1]),
            half::pack2(h[2], h[3]),
            half::pack2(h[4], h[5]),
            half::pack2(h[6], h[7]),
            half::pack2(h[8], h[9]),
            half::pack2(h[10], h[11]),
            half::pack2(self.offset[0], self.offset[1]),
            half::pack2(self.offset[2], self.weight),
            self.mask as u32,
            (self.mask >> 32) as u32,
            self.reach_words()[0],
            self.reach_words()[1],
        ]
    }

    /// The reaches as two words of nibbles (the record's last two).
    fn reach_words(&self) -> [u32; 2] {
        let mut w = [0u32; 2];
        for (k, r) in self.reach.iter().enumerate() {
            w[k / 8] |= u32::from(r & 15) << ((k % 8) * 4);
        }
        w
    }

    /// Whether the probe, at `pos`, sees point `q` by its reaches (spacing `s`): no further along
    /// the reach direction nearest to `q` than the probe sees that way (an eighth of a spacing's
    /// slack: reaches are rounded down).
    pub fn reaches(&self, pos: Vec3, q: Vec3, s: f32) -> bool {
        let [(k, along), (k2, along2)] = reach_dirs(q - pos);
        let ok = |k: usize, along: f32| {
            let r = self.reach[k];
            r >= REACH_NONE || along <= (f32::from(r) + 1.0) * s * 0.125
        };
        ok(k, along) && (along2 < along * REACH_TIE || ok(k2, along2))
    }

    pub fn unpack(r: &Record) -> Probe {
        let mut h = [0.0f32; 12];
        for w in 0..6 {
            let (a, b) = half::unpack2(r[w]);
            h[2 * w] = a;
            h[2 * w + 1] = b;
        }
        let (ox, oy) = half::unpack2(r[6]);
        let (oz, weight) = half::unpack2(r[7]);
        let mut reach = [0u8; 14];
        for (k, x) in reach.iter_mut().enumerate() {
            *x = ((r[10 + k / 8] >> ((k % 8) * 4)) & 15) as u8;
        }
        Probe {
            sh: Sh { c0: [h[0], h[1], h[2]], c1: [[h[3], h[4], h[5]], [h[6], h[7], h[8]], [h[9], h[10], h[11]]] },
            mask: u64::from(r[8]) | (u64::from(r[9]) << 32),
            offset: [ox, oy, oz],
            weight,
            reach,
        }
    }

    /// Every value rounded to what the GPU reads.
    pub fn quantized(&self) -> Probe {
        Probe::unpack(&self.pack())
    }
}

/// A lattice of probes.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub lattice: Lattice,
    pub probes: Vec<Probe>,
}

/// What [`Field::sample`] found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// Irradiance (renderer units), before the volume's intensity.
    pub irradiance: [f32; 3],
    /// How much of it applies: 1 inside the lattice, falling to 0 one spacing outside (the
    /// caller blends towards its flat ambient), 0 when every probe around is inside geometry.
    pub fade: f32,
}

/// One probe's share in a sample, as the shader computes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contribution {
    pub index: usize,
    pub trilinear: f32,
    pub backface: f32,
    /// Whether the cell's mask lets this probe light the point (always, without visibility).
    pub visible: bool,
    pub weight: f32,
}

impl Field {
    /// Irradiance at `p` on a surface facing unit `n`, seen from unit direction `v` towards the
    /// viewer (zero for none). `visibility`: use the cells' masks (quality high).
    pub fn sample(&self, p: Vec3, n: Vec3, v: Vec3, visibility: bool) -> Sample {
        self.sample_with(p, n, v, visibility, |_| {})
    }

    /// [`Field::sample`], reporting each valid probe's share to `each`.
    pub fn sample_with(&self, p: Vec3, n: Vec3, v: Vec3, visibility: bool, mut each: impl FnMut(Contribution)) -> Sample {
        let l = &self.lattice;
        let s = l.spacing;
        let q = p + n * (NORMAL_BIAS * s) + v * (VIEW_BIAS * s);
        let o = Vec3::new(l.origin[0], l.origin[1], l.origin[2]);
        let local = (q - o) * (1.0 / s);
        let lc = [local.x, local.y, local.z];
        let mut out = 0.0f32;
        let mut base = [0u32; 3];
        let mut f = [0.0f32; 3];
        for a in 0..3 {
            let m = (l.dims[a] - 1) as f32;
            out = out.max(-lc[a]).max(lc[a] - m);
            let c = lc[a].clamp(0.0, m);
            let b = dmath::floor(c).min((m - 1.0).max(0.0));
            base[a] = b as u32;
            f[a] = c - b;
        }
        let fade = (1.0 - out).clamp(0.0, 1.0);
        if fade <= 0.0 {
            return Sample { irradiance: [0.0; 3], fade: 0.0 };
        }
        let octant = u32::from(f[0] >= 0.5) | u32::from(f[1] >= 0.5) << 1 | u32::from(f[2] >= 0.5) << 2;
        let mask = if visibility { self.probes[l.index(base[0], base[1], base[2])].mask } else { ALL_VISIBLE };
        let mut acc = Sh::default();
        let mut wsum = 0.0f32;
        let mut any_valid = false;
        let (mut valid_tw, mut visible_tw) = (0.0f32, 0.0f32);
        for corner in 0..8u32 {
            let off = [corner & 1, (corner >> 1) & 1, corner >> 2];
            let c = [base[0] + off[0], base[1] + off[1], base[2] + off[2]];
            let tw = (0..3).fold(1.0f32, |w, a| w * if off[a] == 1 { f[a] } else { 1.0 - f[a] });
            let index = l.index(c[0], c[1], c[2]);
            let pr = &self.probes[index];
            if pr.weight <= 0.0 {
                continue;
            }
            any_valid = true;
            valid_tw += tw;
            let pos = l.at(c) + Vec3::new(pr.offset[0], pr.offset[1], pr.offset[2]);
            let visible = mask & mask_bit(octant, corner) != 0 && (!visibility || pr.reaches(pos, q, s));
            if !visible {
                each(Contribution { index, trilinear: tw, backface: 0.0, visible, weight: 0.0 });
                continue;
            }
            let to = pos - q;
            let d = to.length();
            let dir = if d > 1e-6 { to * (1.0 / d) } else { n };
            let b = (dir.dot(n) + 1.0) * 0.5;
            let backface = b * b + 0.2;
            // The floor keeps a visible probe in play at a small trilinear weight; at none (a point
            // on the cell's face) the far probes don't count: they were the only visible ones when
            // the near ones were behind a wall, and normalizing made a sliver of the room as
            // bright as outside.
            if tw <= 0.0 {
                each(Contribution { index, trilinear: tw, backface, visible, weight: 0.0 });
                continue;
            }
            visible_tw += tw;
            let w = (tw * backface).max(tw * WEIGHT_FLOOR) * pr.weight;
            each(Contribution { index, trilinear: tw, backface, visible, weight: w });
            wsum += w;
            for k in 0..3 {
                acc.c0[k] += w * pr.sh.c0[k];
                for a in 0..3 {
                    acc.c1[a][k] += w * pr.sh.c1[a][k];
                }
            }
        }
        if wsum <= 0.0 {
            // Every probe around inside geometry: nothing is known (the caller's flat ambient).
            // Every valid one hidden: an enclosed pocket no probe sees into: dark.
            return Sample { irradiance: [0.0; 3], fade: if any_valid { fade } else { 0.0 } };
        }
        let dim = if valid_tw > 0.0 { (visible_tw / (valid_tw * VISIBLE_SHARE)).min(1.0) } else { 1.0 };
        let inv = dim / wsum;
        let mut e = [0.0; 3];
        for (k, out) in e.iter_mut().enumerate() {
            *out = ((acc.c0[k] + acc.c1[0][k] * n.x + acc.c1[1][k] * n.y + acc.c1[2][k] * n.z) * inv).max(0.0);
        }
        Sample { irradiance: e, fade }
    }

    pub fn pack(&self) -> Vec<Record> {
        self.probes.iter().map(Probe::pack).collect()
    }

    pub fn unpack(lattice: Lattice, records: &[Record]) -> Field {
        Field { lattice, probes: records.iter().map(Probe::unpack).collect() }
    }

    /// The sub-field of probe columns `x` and `z` (index ranges, inclusive ends, at least two
    /// probes each), every layer.
    pub fn chunk(&self, x: (u32, u32), z: (u32, u32)) -> Field {
        let l = &self.lattice;
        let x1 = x.1.min(l.dims[0] - 1).max(x.0 + 1);
        let z1 = z.1.min(l.dims[2] - 1).max(z.0 + 1);
        let dims = [x1 - x.0 + 1, l.dims[1], z1 - z.0 + 1];
        let origin = [l.origin[0] + x.0 as f32 * l.spacing, l.origin[1], l.origin[2] + z.0 as f32 * l.spacing];
        let mut probes = Vec::with_capacity(dims[0] as usize * dims[1] as usize * dims[2] as usize);
        for j in 0..l.dims[1] {
            for k in z.0..=z1 {
                for i in x.0..=x1 {
                    probes.push(self.probes[l.index(i, j, k)]);
                }
            }
        }
        Field { lattice: Lattice { origin, spacing: l.spacing, dims }, probes }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uniform(dims: [u32; 3], c: f32) -> Field {
        let lattice = Lattice { origin: [0.5, 0.5, 0.5], spacing: 1.0, dims };
        let p = Probe { sh: Sh { c0: [c; 3], c1: [[0.0; 3]; 3] }, mask: ALL_VISIBLE, offset: [0.0; 3], weight: 1.0, reach: [REACH_NONE; 14] };
        Field { lattice, probes: vec![p; lattice.count()] }
    }

    #[test]
    fn lattices_sit_on_the_world_grid_at_half_spacings() {
        let l = Lattice::covering([-3.2, 0.0, 1.0], [4.0, 2.0, 1.1], 1.0);
        assert_eq!(l.origin, [-3.5, -0.5, 0.5]);
        assert_eq!(l.dims, [9, 4, 2]);
        let (lo, hi) = l.bounds();
        assert!(lo[0] <= -3.2 && hi[0] >= 4.0 && lo[1] <= 0.0 && hi[1] >= 2.0);
        for i in 0..l.count() {
            let c = l.coords(i);
            assert_eq!(l.index(c[0], c[1], c[2]), i);
        }
        // Another volume's probes coincide with these where they overlap.
        let m = Lattice::covering([0.0, 0.0, 0.0], [2.0, 1.0, 1.0], 1.0);
        assert_eq!(dmath::fract(m.origin[0] - l.origin[0]), 0.0);
    }

    #[test]
    fn records_round_trip_through_halves() {
        let p = Probe {
            sh: Sh { c0: [0.1, 0.2, 0.3], c1: [[0.01, -0.02, 0.03], [0.04, 0.05, -0.06], [0.07, 0.08, 0.09]] },
            mask: 0x0123_4567_89ab_cdef,
            offset: [0.1, -0.2, 0.3],
            weight: 1.0,
            reach: [1, 2, 3, 4, 5, 15, 0, 7, 8, 9, 10, 11, 12, 14],
        };
        let q = p.quantized();
        assert_eq!(Probe::unpack(&q.pack()), q, "packing a quantized probe is exact");
        assert!((q.sh.c0[0] - 0.1).abs() < 1e-4 && q.mask == p.mask && q.weight == 1.0 && q.reach == p.reach);
        assert_eq!(p.pack()[8], 0x89ab_cdef);
        assert_eq!(p.pack()[10..], [0x7_0f54321, 0x00ec_ba98]);
    }

    #[test]
    fn reaches_hide_points_behind_the_first_surface_along_an_axis() {
        // A probe at the origin that sees 0.5 m (4 eighths at 1 m spacing) towards -x and 1 m
        // along the (-1, +1, -1) diagonal, and nothing within 15/8 m the other ways.
        let mut reach = [REACH_NONE; 14];
        reach[1] = 4;
        reach[6 + 1 + 4] = 8;
        let p = Probe { weight: 1.0, reach, ..Probe::default() };
        let at = |x: f32, y: f32, z: f32| p.reaches(Vec3::ZERO, Vec3::new(x, y, z), 1.0);
        assert!(at(-0.5, 0.0, 0.0) && at(-0.6, 0.1, 0.0), "up to the wall (and an eighth's slack)");
        assert!(!at(-0.7, 0.0, 0.0) && !at(-0.9, 0.3, 0.0), "behind it, nearest -x");
        assert!(at(-0.6, 0.9, 0.0), "nearest +y: that way is open");
        assert!(at(5.0, 0.0, 0.0), "+x is open");
        assert!(at(-0.6, 0.6, -0.6) && !at(-0.8, 0.8, -0.8), "the diagonal: 1.04 m along it is in reach, 1.39 m isn't");
        assert_eq!(reach_dir(Vec3::new(-1.0, 0.65, -1.0)).0, 6 + 1 + 4);
        assert_eq!(reach_dir(Vec3::new(0.0, -2.0, 0.1)).0, 3);
        for (k, d) in REACH_DIRS.iter().enumerate() {
            assert_eq!(reach_dir(Vec3::new(d[0], d[1], d[2])).0, k, "each direction is its own nearest");
        }
    }

    #[test]
    fn a_uniform_field_samples_its_value_and_fades_outside() {
        let f = uniform([4, 3, 4], 0.4);
        let s = f.sample(Vec3::new(2.0, 1.0, 2.0), Vec3::Y, Vec3::ZERO, true);
        assert!((s.irradiance[1] - 0.4).abs() < 1e-5 && s.fade == 1.0, "{s:?}");
        // Half a spacing beyond the last probe (x = 3.5, after the normal bias): half faded.
        let s = f.sample(Vec3::new(3.75, 1.0, 2.0), Vec3::X, Vec3::ZERO, true);
        assert!((s.fade - 0.5).abs() < 1e-4, "{s:?}");
        assert_eq!(f.sample(Vec3::new(50.0, 1.0, 2.0), Vec3::Y, Vec3::ZERO, true).fade, 0.0);
    }

    #[test]
    fn invalid_probes_never_count_and_all_invalid_falls_back() {
        let mut f = uniform([2, 2, 2], 1.0);
        f.probes[0].sh.c0 = [100.0; 3];
        f.probes[0].weight = 0.0;
        let s = f.sample(Vec3::new(0.9, 0.9, 0.9), Vec3::Y, Vec3::ZERO, true);
        assert!((s.irradiance[0] - 1.0).abs() < 1e-5, "{s:?}");
        for p in &mut f.probes {
            p.weight = 0.0;
        }
        assert_eq!(f.sample(Vec3::new(0.9, 0.9, 0.9), Vec3::Y, Vec3::ZERO, true).fade, 0.0);
    }

    #[test]
    fn a_cell_mask_hides_probes_behind_a_wall() {
        // A wall at x = 1.0 through the cell: from the low-x octants, the high-x probes are hidden.
        let mut f = uniform([2, 2, 2], 0.0);
        let l = f.lattice;
        let mut mask = ALL_VISIBLE;
        for octant in 0..8u32 {
            for corner in 0..8u32 {
                if (octant & 1) != (corner & 1) {
                    mask &= !mask_bit(octant, corner);
                }
            }
        }
        f.probes[0].mask = mask;
        for j in 0..2 {
            for k in 0..2 {
                f.probes[l.index(1, j, k)].sh.c0 = [1.0; 3];
            }
        }
        // A point at x = 0.9 facing -x samples at 0.65 (a low-x octant): the bright side is hidden.
        let s = f.sample(Vec3::new(0.9, 0.9, 0.9), Vec3::new(-1.0, 0.0, 0.0), Vec3::ZERO, true);
        assert_eq!(s.irradiance[0], 0.0, "{s:?}");
        // Without visibility (quality low) the light leaks in.
        let s = f.sample(Vec3::new(0.9, 0.9, 0.9), Vec3::new(-1.0, 0.0, 0.0), Vec3::ZERO, false);
        assert!(s.irradiance[0] > 0.05, "{s:?}");
        // A point on the far side samples the bright probes only.
        let s = f.sample(Vec3::new(1.1, 0.9, 0.9), Vec3::X, Vec3::ZERO, true);
        assert!((s.irradiance[0] - 1.0).abs() < 1e-5, "{s:?}");
        // All valid probes hidden: dark, not the flat ambient.
        f.probes[0].mask = 0;
        let s = f.sample(Vec3::new(0.6, 0.9, 0.9), Vec3::Y, Vec3::ZERO, true);
        assert_eq!((s.irradiance[0], s.fade), (0.0, 1.0));
    }

    #[test]
    fn chunks_share_their_border_probes() {
        let mut f = uniform([6, 2, 5], 0.0);
        for (i, p) in f.probes.iter_mut().enumerate() {
            p.sh.c0 = [i as f32; 3];
        }
        let a = f.chunk((0, 3), (0, 4));
        let b = f.chunk((3, 5), (0, 4));
        assert_eq!(a.lattice.dims, [4, 2, 5]);
        assert_eq!(b.lattice.origin[0], f.lattice.origin[0] + 3.0);
        // Sampling at the seam gives the same from both.
        let p = Vec3::new(f.lattice.origin[0] + 3.0 - 0.25, 1.0, 2.0);
        let (sa, sb, sf) = (a.sample(p, Vec3::X, Vec3::ZERO, true), b.sample(p, Vec3::X, Vec3::ZERO, true), f.sample(p, Vec3::X, Vec3::ZERO, true));
        assert!((sa.irradiance[0] - sf.irradiance[0]).abs() < 1e-3 && (sb.irradiance[0] - sf.irradiance[0]).abs() < 1e-3, "{sa:?} {sb:?} {sf:?}");
    }
}
