//! The editing mesh: welded positions, per-vertex colours and triangles, without normals.
//!
//! Every modelling operation works on a [`Shape`]; [`Shape::finalize`] turns one into a
//! render-ready [`MeshData`] (normals split at hard edges, colours quantised to sRGB8).
//!
//! **Conventions**
//! - Solids rest on y = 0 and are centred on the Y axis (box, cylinder, cone, sphere, torus,
//!   capsule, extrude, loft, lathe), so stacking is just a `move` in y.
//! - 2D outlines for extrude/loft are `[x, z]` points (any orientation; they are normalised).
//! - Triangles are counter-clockwise seen from outside; closed generators produce watertight
//!   shells (every edge shared by exactly two triangles).
//! - All maths is `f32` basic operations, `sqrt` and [`crate::dmath`] trig, so a shape is
//!   bit-identical on every platform (F2).

use std::collections::HashMap;

use specula_core::rng::Rng;

use crate::dmath;
use crate::math3::{self, Affine, V2, V3};
use crate::mesh::{MeshData, Part, MIN_TRIANGLE_AREA};
use crate::noise;
use crate::poly;

/// Triangle tag: an ordinary face.
pub const TAG_FACE: u8 = 0;
/// Triangle tag: a chamfer/bevel face (can be tinted with [`Shape::bevel_tint`]).
pub const TAG_BEVEL: u8 = 1;

/// sRGB colour with linear alpha, each 0..1.
pub type Rgba = [f32; 4];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shape {
    pub positions: Vec<V3>,
    pub colors: Vec<Rgba>,
    pub tris: Vec<[u32; 3]>,
    /// One [`TAG_FACE`]/[`TAG_BEVEL`] per triangle.
    pub tags: Vec<u8>,
    /// Colour blended into bevel faces at [`Shape::finalize`]: (colour, amount 0..1).
    pub bevel_tint: Option<(Rgba, f32)>,
}

/// A ring of a ring stack: several points, or one point (an apex).
type Ring = Vec<V3>;
/// A cap triangulation over a ring's points, if the end is capped.
type CapTris<'a> = Option<&'a [[u32; 3]]>;

/// A colour gradient for [`Shape::gradient`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gradient {
    /// 0 = X, 1 = Y, 2 = Z.
    pub axis: usize,
    pub from: Rgba,
    pub to: Rgba,
    /// Fractions of the current bounds along `axis`, or metres when `absolute`.
    pub range: V2,
    pub absolute: bool,
    /// Blend over the current colour (1 = replace).
    pub strength: f32,
    /// Vertices outside the range keep their colour (a local tint band).
    pub keep_outside: bool,
}

const WHITE: Rgba = [1.0, 1.0, 1.0, 1.0];

impl Shape {
    pub fn triangle_count(&self) -> usize {
        self.tris.len()
    }

    fn push_tri(&mut self, t: [u32; 3], tag: u8) {
        self.tris.push(t);
        self.tags.push(tag);
    }

    /// Bounds of the positions.
    pub fn bounds(&self) -> crate::mesh::Bounds {
        crate::mesh::Bounds::of(&self.positions)
    }

    // -----------------------------------------------------------------------------------------
    // Ring stacks: the shared core of extrude, loft, lathe, sweep and torus.

    /// Stitches rings into a surface. All multi-point rings must have the same length; a
    /// one-point ring makes a fan (an apex). Cap triangulations index the ring polygon
    /// (counter-clockwise in the profile plane): `caps.0` closes the first ring (reversed so it
    /// faces backwards), `caps.1` the last.
    /// `tags[k]` tags the band between ring k and k+1.
    fn ring_stack(rings: &[Ring], caps: (CapTris, CapTris), closed: bool, band_tags: &[u8]) -> Shape {
        let mut s = Shape::default();
        let mut starts = Vec::with_capacity(rings.len());
        for r in rings {
            starts.push(s.positions.len() as u32);
            s.positions.extend_from_slice(r);
        }
        s.colors = vec![WHITE; s.positions.len()];
        let bands = if closed { rings.len() } else { rings.len() - 1 };
        for k in 0..bands {
            let (i, j) = (k, (k + 1) % rings.len());
            let tag = band_tags.get(k).copied().unwrap_or(TAG_FACE);
            let (na, nb) = (rings[i].len(), rings[j].len());
            let (a, b) = (starts[i], starts[j]);
            if na == 1 && nb == 1 {
                continue;
            }
            if na == 1 {
                for m in 0..nb {
                    let m1 = (m + 1) % nb;
                    s.push_tri([a, b + m1 as u32, b + m as u32], tag);
                }
            } else if nb == 1 {
                for m in 0..na {
                    let m1 = (m + 1) % na;
                    s.push_tri([a + m as u32, a + m1 as u32, b], tag);
                }
            } else {
                debug_assert_eq!(na, nb);
                for m in 0..na {
                    let m1 = (m + 1) % na;
                    let (a0, a1, b0, b1) = (a + m as u32, a + m1 as u32, b + m as u32, b + m1 as u32);
                    let p = |v: u32| s.positions[v as usize];
                    // Split along the shorter diagonal (deterministic: ties take a0-b1).
                    let d0 = math3::length(math3::sub(p(a0), p(b1)));
                    let d1 = math3::length(math3::sub(p(a1), p(b0)));
                    if d0 <= d1 {
                        s.push_tri([a0, a1, b1], tag);
                        s.push_tri([a0, b1, b0], tag);
                    } else {
                        s.push_tri([a0, a1, b0], tag);
                        s.push_tri([a1, b1, b0], tag);
                    }
                }
            }
        }
        if let Some(ct) = caps.0.filter(|_| rings[0].len() > 1) {
            let o = starts[0];
            for t in ct {
                s.push_tri([o + t[0], o + t[2], o + t[1]], TAG_FACE);
            }
        }
        let last = rings.len() - 1;
        if let Some(ct) = caps.1.filter(|_| rings[last].len() > 1) {
            let o = starts[last];
            for t in ct {
                s.push_tri([o + t[0], o + t[1], o + t[2]], TAG_FACE);
            }
        }
        s
    }

    /// Lofts 2D outlines (all the same point count, or a single point) stacked at heights:
    /// `rings[k] = (outline in [a, b] counter-clockwise, y)`, placed at `(a, y, -b)`.
    fn loft_ab(rings: &[(Vec<V2>, f32)], caps: (bool, bool), band_tags: &[u8]) -> Shape {
        let rings3: Vec<Ring> = rings.iter().map(|(o, y)| o.iter().map(|q| [q[0], *y, -q[1]]).collect()).collect();
        let tri = |o: &Vec<V2>| if o.len() > 2 { poly::triangulate(o) } else { None };
        let first = if caps.0 { tri(&rings[0].0) } else { None };
        let last = if caps.1 { tri(&rings[rings.len() - 1].0) } else { None };
        let mut s = Shape::ring_stack(&rings3, (first.as_deref(), last.as_deref()), false, band_tags);
        s.orient_outward_if_closed();
        s
    }

    /// Flips every triangle if the shell encloses negative volume (a profile traversed top to
    /// bottom). Only for closed shells.
    fn orient_outward_if_closed(&mut self) {
        if self.signed_volume() < 0.0 {
            for t in &mut self.tris {
                t.swap(1, 2);
            }
        }
    }

    pub fn signed_volume(&self) -> f32 {
        let mut v = 0.0f64;
        for t in &self.tris {
            let (a, b, c) = (self.positions[t[0] as usize], self.positions[t[1] as usize], self.positions[t[2] as usize]);
            v += math3::dot(a, math3::cross(b, c)) as f64;
        }
        (v / 6.0) as f32
    }

    // -----------------------------------------------------------------------------------------
    // Generators

    /// Extrudes an `[x, z]` outline up +Y from 0 to `height`.
    ///
    /// - `taper`: top outline scale (1 = prism, 0 = pyramid apex).
    /// - `bevel`: chamfer width on the top and bottom rims (the caps are inset by it).
    /// - `corner_bevel`: chamfer on the outline's corners (vertical edges).
    /// - `segments`: side bands (≥ 1), for later deformation.
    /// - `cap_inset`: `(inset, depth)` adds a panel on the top cap: inset by `inset`, raised
    ///   (`depth` > 0) or recessed (`depth` < 0).
    pub fn extrude(outline_xz: &[V2], height: f32, taper: f32, bevel: f32, corner_bevel: f32, segments: u32, cap_inset: Option<(f32, f32)>) -> Shape {
        let ab: Vec<V2> = outline_xz.iter().map(|q| [q[0], -q[1]]).collect();
        let mut base = poly::canonical(&ab);
        if corner_bevel > 0.0 {
            base = poly::bevel_corners(&base, corner_bevel);
        }
        let scaled = |s: f32| -> Vec<V2> { base.iter().map(|q| [q[0] * s, q[1] * s]).collect() };
        let apex = taper <= 1e-4;
        let b = bevel.max(0.0).min(0.45 * height);
        let mut rings: Vec<(Vec<V2>, f32)> = Vec::new();
        let mut tags = Vec::new();
        if b > 0.0 {
            rings.push((poly::inset(&base, b), 0.0));
            tags.push(TAG_BEVEL);
        }
        let segs = segments.max(1);
        let (y0, y1) = (b, if apex { height } else { height - b });
        for k in 0..=segs {
            let t = k as f32 / segs as f32;
            let y = y0 + (y1 - y0) * t;
            let s = 1.0 + (taper - 1.0) * (y / height);
            if apex && k == segs {
                rings.push((vec![[0.0, 0.0]], height));
            } else {
                rings.push((scaled(s), y));
            }
            if k < segs {
                tags.push(TAG_FACE);
            }
        }
        if !apex {
            let top = scaled(taper);
            if b > 0.0 {
                tags.push(TAG_BEVEL);
                rings.push((poly::inset(&top, b), height));
            }
            if let Some((inset, depth)) = cap_inset {
                let last = rings.last().unwrap().0.clone();
                let inner = poly::inset(&last, inset);
                tags.push(TAG_FACE);
                rings.push((inner.clone(), height));
                tags.push(TAG_BEVEL);
                rings.push((inner, height + depth));
            }
        }
        Shape::loft_ab(&rings, (true, true), &tags)
    }

    /// Lofts one `[x, z]` outline through `(scale, y, dx, dz)` stations (scale 0 = apex point).
    /// Bands between stations whose scale changes by more than their height are tagged as bevels.
    pub fn loft(outline_xz: &[V2], stations: &[[f32; 4]], corner_bevel: f32) -> Shape {
        let ab: Vec<V2> = outline_xz.iter().map(|q| [q[0], -q[1]]).collect();
        let mut base = poly::canonical(&ab);
        if corner_bevel > 0.0 {
            base = poly::bevel_corners(&base, corner_bevel);
        }
        let radius = base.iter().map(|q| (q[0] * q[0] + q[1] * q[1]).sqrt()).fold(0.0f32, f32::max);
        let rings: Vec<(Vec<V2>, f32)> = stations
            .iter()
            .map(|&[s, y, dx, dz]| {
                if s <= 1e-4 {
                    (vec![[dx, -dz]], y)
                } else {
                    (base.iter().map(|q| [q[0] * s + dx, q[1] * s - dz]).collect(), y)
                }
            })
            .collect();
        let tags: Vec<u8> = stations
            .windows(2)
            .map(|w| if ((w[1][0] - w[0][0]) * radius).abs() > 0.7 * (w[1][1] - w[0][1]).abs() { TAG_BEVEL } else { TAG_FACE })
            .collect();
        Shape::loft_ab(&rings, (true, true), &tags)
    }

    /// Spins an `[r, y]` profile around +Y with `segments` sides. r = 0 points close the shape;
    /// open ends get flat caps when `caps` is set.
    pub fn lathe(profile: &[V2], segments: u32, caps: bool) -> Shape {
        let circle = poly::regular(segments.max(3), 1.0, 0.0);
        let rings: Vec<(Vec<V2>, f32)> = profile
            .iter()
            .map(|&[r, y]| if r <= 1e-5 { (vec![[0.0, 0.0]], y) } else { (circle.iter().map(|q| [q[0] * r, q[1] * r]).collect(), y) })
            .collect();
        let rings3: Vec<Ring> = rings.iter().map(|(o, y)| o.iter().map(|q| [q[0], *y, -q[1]]).collect()).collect();
        let tri = if caps { poly::triangulate(&circle) } else { None };
        let mut s = Shape::ring_stack(&rings3, (tri.as_deref(), tri.as_deref()), false, &[]);
        let closed = caps || (profile.first().is_some_and(|p| p[0] <= 1e-5) && profile.last().is_some_and(|p| p[0] <= 1e-5));
        if closed {
            s.orient_outward_if_closed();
        }
        s
    }

    /// A `size` box resting on y = 0 with every edge chamfered by `bevel`.
    pub fn cuboid(size: V3, bevel: f32) -> Shape {
        let b = bevel.max(0.0).min(0.45 * size[0].min(size[1]).min(size[2]));
        Shape::extrude(&poly::rect(size[0], size[2]), size[1], 1.0, b, b, 1, None)
    }

    /// A prism with `sides` (≥ 3), radii at the bottom and top, rims chamfered by `bevel`.
    pub fn cylinder(radius: f32, radius_top: f32, height: f32, sides: u32, bevel: f32) -> Shape {
        let ngon = poly::regular(sides.max(3), radius, 90.0 + 180.0 / sides.max(3) as f64);
        let xz: Vec<V2> = ngon.iter().map(|q| [q[0], -q[1]]).collect();
        Shape::extrude(&xz, height, radius_top / radius, bevel, 0.0, 1, None)
    }

    /// An icosphere resting on y = 0: 20·4ⁿ triangles.
    pub fn sphere(radius: f32, subdivisions: u32) -> Shape {
        let t = (1.0 + 5.0f32.sqrt()) * 0.5;
        let mut pos: Vec<V3> = [
            [-1.0, t, 0.0],
            [1.0, t, 0.0],
            [-1.0, -t, 0.0],
            [1.0, -t, 0.0],
            [0.0, -1.0, t],
            [0.0, 1.0, t],
            [0.0, -1.0, -t],
            [0.0, 1.0, -t],
            [t, 0.0, -1.0],
            [t, 0.0, 1.0],
            [-t, 0.0, -1.0],
            [-t, 0.0, 1.0],
        ]
        .iter()
        .map(|&p| {
            // Turn about Z so vertices sit exactly at the poles (rests on one vertex at y = 0).
            let n = (1.0 + t * t).sqrt();
            let (c, s) = (t / n, 1.0 / n);
            let q = [p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2]];
            math3::normalize(q)
        })
        .collect();
    // Exact poles.
    pos[1] = [0.0, 1.0, 0.0];
    pos[2] = [0.0, -1.0, 0.0];
        let mut tris: Vec<[u32; 3]> = vec![
            [0, 11, 5],
            [0, 5, 1],
            [0, 1, 7],
            [0, 7, 10],
            [0, 10, 11],
            [1, 5, 9],
            [5, 11, 4],
            [11, 10, 2],
            [10, 7, 6],
            [7, 1, 8],
            [3, 9, 4],
            [3, 4, 2],
            [3, 2, 6],
            [3, 6, 8],
            [3, 8, 9],
            [4, 9, 5],
            [2, 4, 11],
            [6, 2, 10],
            [8, 6, 7],
            [9, 8, 1],
        ];
        for _ in 0..subdivisions.min(5) {
            let mut mid: HashMap<(u32, u32), u32> = HashMap::new();
            let mut next = Vec::with_capacity(tris.len() * 4);
            for t in &tris {
                let mut m = [0u32; 3];
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    let key = (a.min(b), a.max(b));
                    m[k] = *mid.entry(key).or_insert_with(|| {
                        pos.push(math3::normalize(math3::scale(math3::add(pos[a as usize], pos[b as usize]), 0.5)));
                        pos.len() as u32 - 1
                    });
                }
                next.push([t[0], m[0], m[2]]);
                next.push([t[1], m[1], m[0]]);
                next.push([t[2], m[2], m[1]]);
                next.push([m[0], m[1], m[2]]);
            }
            tris = next;
        }
        let positions: Vec<V3> = pos.iter().map(|p| [p[0] * radius, p[1] * radius + radius, p[2] * radius]).collect();
        let n = tris.len();
        let mut s = Shape { colors: vec![WHITE; positions.len()], positions, tris, tags: vec![TAG_FACE; n], bevel_tint: None };
        s.orient_outward_if_closed();
        s
    }

    /// A torus around +Y resting on y = 0: ring `radius`, `tube` radius.
    pub fn torus(radius: f32, tube: f32, segments: u32, sides: u32) -> Shape {
        let circle = poly::regular(sides.max(3), tube, 0.0);
        let segs = segments.max(3);
        let rings: Vec<Ring> = (0..segs)
            .map(|i| {
                let (s, c) = dmath::sin_cos_deg(360.0 * i as f64 / segs as f64);
                let (s, c) = (s as f32, c as f32);
                let radial = [c, 0.0, -s];
                let up = [0.0, 1.0, 0.0];
                let centre = math3::scale(radial, radius);
                circle.iter().map(|q| math3::add(math3::add(centre, math3::scale(radial, q[0])), math3::scale(up, q[1] + tube))).collect()
            })
            .collect();
        let mut s = Shape::ring_stack(&rings, (None, None), true, &[]);
        s.orient_outward_if_closed();
        s
    }

    /// A capsule along +Y from 0 to `height` (≥ 2·radius).
    pub fn capsule(radius: f32, height: f32, segments: u32, rings: u32) -> Shape {
        let h = height.max(2.0 * radius);
        let n = rings.max(1);
        let mut profile = vec![[0.0, 0.0]];
        for k in 1..=n {
            let (s, c) = dmath::sin_cos_deg(90.0 * k as f64 / n as f64);
            profile.push([radius * s as f32, radius - radius * c as f32]);
        }
        for k in 0..=n {
            let (s, c) = dmath::sin_cos_deg(90.0 * k as f64 / n as f64);
            if k == n {
                break;
            }
            profile.push([radius * c as f32, h - radius + radius * s as f32]);
        }
        profile.push([0.0, h]);
        Shape::lathe(&profile, segments, false)
    }

    /// A flat grid at y = 0 facing +Y: `size` = [x, z], `div` cells along each.
    pub fn plane(size: V2, div: [u32; 2]) -> Shape {
        let (nx, nz) = (div[0].max(1), div[1].max(1));
        let mut s = Shape::default();
        for j in 0..=nz {
            for i in 0..=nx {
                let x = -0.5 * size[0] + size[0] * i as f32 / nx as f32;
                let z = -0.5 * size[1] + size[1] * j as f32 / nz as f32;
                s.positions.push([x, 0.0, z]);
            }
        }
        let w = nx + 1;
        for j in 0..nz {
            for i in 0..nx {
                let (p00, p10, p01, p11) = (j * w + i, j * w + i + 1, (j + 1) * w + i, (j + 1) * w + i + 1);
                s.push_tri([p00, p01, p10], TAG_FACE);
                s.push_tri([p10, p01, p11], TAG_FACE);
            }
        }
        s.colors = vec![WHITE; s.positions.len()];
        s
    }

    /// Sweeps a 2D profile along a 3D polyline with rotation-minimising frames (double
    /// reflection, Wang et al. 2008). `scales[i]` sizes the profile at path point i (0 = a point);
    /// `twist_deg` turns it gradually along the path. Corners are mitred so tubes keep their
    /// thickness. Closed paths make rings (frames from the loop's plane, so they meet seamlessly).
    pub fn sweep(profile_ab: &[V2], path: &[V3], scales: &[f32], twist_deg: f32, closed: bool, caps: bool, up: Option<V3>) -> Shape {
        let prof = poly::canonical(profile_ab);
        let n = path.len();
        let seg = |i: usize, j: usize| math3::normalize(math3::sub(path[j], path[i]));
        let mut tangents = Vec::with_capacity(n);
        for i in 0..n {
            let t = if closed {
                math3::normalize(math3::add(seg((i + n - 1) % n, i), seg(i, (i + 1) % n)))
            } else if i == 0 {
                seg(0, 1)
            } else if i == n - 1 {
                seg(n - 2, n - 1)
            } else {
                let t = math3::normalize(math3::add(seg(i - 1, i), seg(i, i + 1)));
                if math3::length(t) < 0.5 {
                    seg(i, i + 1)
                } else {
                    t
                }
            };
            tangents.push(t);
        }
        let mut normals: Vec<V3> = Vec::with_capacity(n);
        if closed {
            // Newell normal of the loop.
            let mut nn = [0.0f32; 3];
            for i in 0..n {
                let (a, b) = (path[i], path[(i + 1) % n]);
                nn[0] += (a[1] - b[1]) * (a[2] + b[2]);
                nn[1] += (a[2] - b[2]) * (a[0] + b[0]);
                nn[2] += (a[0] - b[0]) * (a[1] + b[1]);
            }
            let plane = math3::normalize(nn);
            for t in &tangents {
                let r = math3::normalize(math3::sub(plane, math3::scale(*t, math3::dot(plane, *t))));
                normals.push(if math3::length(r) < 0.5 { math3::perpendicular(*t) } else { r });
            }
        } else {
            let hint = up.unwrap_or([0.0, 1.0, 0.0]);
            let t0 = tangents[0];
            let mut r0 = math3::normalize(math3::sub(hint, math3::scale(t0, math3::dot(hint, t0))));
            if math3::length(r0) < 0.5 {
                r0 = math3::perpendicular(t0);
            }
            normals.push(r0);
            for i in 0..n - 1 {
                let v1 = math3::sub(path[i + 1], path[i]);
                let c1 = math3::dot(v1, v1);
                let (ri, ti) = (normals[i], tangents[i]);
                if c1 < 1e-20 {
                    normals.push(ri);
                    continue;
                }
                let rl = math3::sub(ri, math3::scale(v1, 2.0 / c1 * math3::dot(v1, ri)));
                let tl = math3::sub(ti, math3::scale(v1, 2.0 / c1 * math3::dot(v1, ti)));
                let v2 = math3::sub(tangents[i + 1], tl);
                let c2 = math3::dot(v2, v2);
                let r = if c2 < 1e-20 { rl } else { math3::sub(rl, math3::scale(v2, 2.0 / c2 * math3::dot(v2, rl))) };
                normals.push(math3::normalize(r));
            }
        }
        let rings: Vec<Ring> = (0..n)
            .map(|i| {
                let s = scales.get(i).copied().unwrap_or(1.0);
                if s <= 1e-5 {
                    return vec![path[i]];
                }
                let t = tangents[i];
                let nrm = normals[i];
                let b = math3::cross(t, nrm);
                // Mitre: stretch along the bend direction by 1/cos(half angle).
                let interior = closed || (i > 0 && i < n - 1);
                let (bend, stretch) = if interior {
                    let din = seg((i + n - 1) % n, i);
                    let dout = seg(i, (i + 1) % n);
                    let cosh = math3::dot(din, t).clamp(0.5, 1.0);
                    let k = math3::normalize(math3::sub(dout, din));
                    let k = math3::normalize(math3::sub(k, math3::scale(t, math3::dot(k, t))));
                    (k, 1.0 / cosh - 1.0)
                } else {
                    ([0.0; 3], 0.0)
                };
                let tw = if n > 1 { twist_deg as f64 * i as f64 / (n - 1) as f64 } else { 0.0 };
                let (ts, tc) = dmath::sin_cos_deg(tw);
                let (ts, tc) = (ts as f32, tc as f32);
                prof.iter()
                    .map(|q| {
                        let (a, bb) = (q[0] * tc - q[1] * ts, q[0] * ts + q[1] * tc);
                        let o = math3::add(math3::scale(nrm, a * s), math3::scale(b, bb * s));
                        let o = math3::add(o, math3::scale(bend, math3::dot(bend, o) * stretch));
                        math3::add(path[i], o)
                    })
                    .collect()
            })
            .collect();
        let tri = if caps && !closed { poly::triangulate(&prof) } else { None };
        let mut s = Shape::ring_stack(&rings, (tri.as_deref(), tri.as_deref()), closed, &[]);
        let ends_closed = closed || (caps || scales.first().is_some_and(|&v| v <= 1e-5)) && (caps || scales.last().is_some_and(|&v| v <= 1e-5));
        if ends_closed {
            s.orient_outward_if_closed();
        }
        s
    }

    // -----------------------------------------------------------------------------------------
    // Operations

    /// Applies an affine transform (mirrors flip the winding to stay outward).
    pub fn transform(&mut self, t: &Affine) {
        for p in &mut self.positions {
            *p = t.apply_point(*p);
        }
        if t.determinant() < 0.0 {
            for tri in &mut self.tris {
                tri.swap(1, 2);
            }
        }
    }

    /// Appends `other`. The bevel tint of `self` wins (else `other`'s).
    pub fn merge(&mut self, other: &Shape) {
        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.colors.extend_from_slice(&other.colors);
        self.tris.extend(other.tris.iter().map(|t| [t[0] + base, t[1] + base, t[2] + base]));
        self.tags.extend_from_slice(&other.tags);
        if self.bevel_tint.is_none() {
            self.bevel_tint = other.bevel_tint;
        }
    }

    /// Mirrors across the plane `axis` = 0 and keeps both halves (`keep_original`), welding
    /// the vertices that lie on the plane.
    pub fn mirror(&mut self, axis: usize, keep_original: bool) {
        let mut s = [1.0; 3];
        s[axis] = -1.0;
        let mut copy = self.clone();
        copy.transform(&Affine::scale(s));
        if keep_original {
            self.merge(&copy);
            self.weld(1e-5);
        } else {
            *self = copy;
        }
    }

    /// Copies the shape `count` times; copy k gets `place(k)` applied after its jitter.
    fn array_with(&mut self, count: u32, rng: &mut Rng, jitter: &Jitter, place: impl Fn(u32) -> Affine) {
        let src = self.clone();
        let mut out = Shape { bevel_tint: src.bevel_tint, ..Shape::default() };
        for k in 0..count.max(1) {
            let mut c = src.clone();
            c.transform(&place(k).then_after(&jitter.sample(rng)));
            out.merge(&c);
        }
        *self = out;
    }

    /// `count` copies, each `offset` further along.
    pub fn array_linear(&mut self, count: u32, offset: V3, rng: &mut Rng, jitter: &Jitter) {
        self.array_with(count, rng, jitter, |k| Affine::translate(math3::scale(offset, k as f32)));
    }

    /// `count` copies around `axis` (through the origin), spread over `angle_deg`
    /// (360 = evenly around the full circle).
    pub fn array_radial(&mut self, count: u32, axis: V3, angle_deg: f64, rng: &mut Rng, jitter: &Jitter) {
        let full = (angle_deg.abs() - 360.0).abs() < 1e-9;
        let step = if full || count <= 1 { angle_deg / count.max(1) as f64 } else { angle_deg / (count - 1) as f64 };
        self.array_with(count, rng, jitter, |k| Affine::rotate_about_deg(axis, step * k as f64));
    }

    /// Merges vertices closer than `eps` (grid-snapped; first vertex wins) and drops triangles
    /// that collapse.
    pub fn weld(&mut self, eps: f32) {
        let inv = 1.0 / eps;
        let mut ids: HashMap<[i64; 3], u32> = HashMap::new();
        let mut remap = Vec::with_capacity(self.positions.len());
        let mut pos = Vec::new();
        let mut col = Vec::new();
        for (i, p) in self.positions.iter().enumerate() {
            let key = [
                dmath::floor_f64((p[0] * inv) as f64 + 0.5) as i64,
                dmath::floor_f64((p[1] * inv) as f64 + 0.5) as i64,
                dmath::floor_f64((p[2] * inv) as f64 + 0.5) as i64,
            ];
            let id = *ids.entry(key).or_insert_with(|| {
                pos.push(*p);
                col.push(self.colors[i]);
                pos.len() as u32 - 1
            });
            remap.push(id);
        }
        self.positions = pos;
        self.colors = col;
        self.remap_tris(&remap);
    }

    /// Re-indexes triangles and drops ones that repeat a vertex or have no area.
    fn remap_tris(&mut self, remap: &[u32]) {
        let mut tris = Vec::with_capacity(self.tris.len());
        let mut tags = Vec::with_capacity(self.tris.len());
        for (t, &tag) in self.tris.iter().zip(&self.tags) {
            let m = [remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize]];
            if m[0] == m[1] || m[1] == m[2] || m[0] == m[2] {
                continue;
            }
            let (a, b, c) = (self.positions[m[0] as usize], self.positions[m[1] as usize], self.positions[m[2] as usize]);
            if 0.5 * math3::length(math3::tri_normal(a, b, c)) < MIN_TRIANGLE_AREA {
                continue;
            }
            tris.push(m);
            tags.push(tag);
        }
        self.tris = tris;
        self.tags = tags;
    }

    /// Splits every triangle into four, `levels` times (midpoints are shared, so closed shells
    /// stay closed).
    pub fn subdivide(&mut self, levels: u32) {
        for _ in 0..levels {
            let mut mid: HashMap<(u32, u32), u32> = HashMap::new();
            let mut tris = Vec::with_capacity(self.tris.len() * 4);
            let mut tags = Vec::with_capacity(self.tris.len() * 4);
            for (t, &tag) in self.tris.clone().iter().zip(&self.tags.clone()) {
                let mut m = [0u32; 3];
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    let key = (a.min(b), a.max(b));
                    m[k] = *mid.entry(key).or_insert_with(|| {
                        let (pa, pb) = (self.positions[a as usize], self.positions[b as usize]);
                        self.positions.push(math3::lerp(pa, pb, 0.5));
                        let (ca, cb) = (self.colors[a as usize], self.colors[b as usize]);
                        self.colors.push([0.5 * (ca[0] + cb[0]), 0.5 * (ca[1] + cb[1]), 0.5 * (ca[2] + cb[2]), 0.5 * (ca[3] + cb[3])]);
                        self.positions.len() as u32 - 1
                    });
                }
                tris.extend_from_slice(&[[t[0], m[0], m[2]], [t[1], m[1], m[0]], [t[2], m[2], m[1]], [m[0], m[1], m[2]]]);
                tags.extend_from_slice(&[tag; 4]);
            }
            self.tris = tris;
            self.tags = tags;
        }
    }

    /// Area-weighted vertex normals, shared by every vertex at the same position (so seams
    /// between coincident vertices move together).
    pub fn smooth_normals(&self) -> Vec<V3> {
        let key = |p: V3| [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
        let mut group: HashMap<[u32; 3], usize> = HashMap::new();
        let gid: Vec<usize> = self
            .positions
            .iter()
            .map(|p| {
                let next = group.len();
                *group.entry(key(*p)).or_insert(next)
            })
            .collect();
        let mut acc = vec![[0.0f32; 3]; group.len()];
        for t in &self.tris {
            let (a, b, c) = (self.positions[t[0] as usize], self.positions[t[1] as usize], self.positions[t[2] as usize]);
            let n = math3::tri_normal(a, b, c);
            for &v in t {
                let g = gid[v as usize];
                acc[g] = math3::add(acc[g], n);
            }
        }
        gid.iter().map(|&g| math3::normalize(acc[g])).collect()
    }

    /// Moves vertices along their normals by `amount` × fractal noise in [-1, 1) sampled at
    /// `position / scale`. Coincident vertices move together, so closed shells stay closed.
    pub fn displace(&mut self, amount: f32, scale: f32, octaves: u32, seed: u64, axis_weight: V3) {
        let normals = self.smooth_normals();
        let inv = 1.0 / scale.max(1e-4);
        for (p, n) in self.positions.iter_mut().zip(&normals) {
            let d = noise::fbm3(seed, math3::scale(*p, inv), octaves) * amount;
            *p = math3::add(*p, math3::mul(math3::scale(*n, d), axis_weight));
        }
    }

    /// Scales the X/Z (or other two) coordinates linearly along `axis` between the bounds:
    /// `from` at the minimum, `to` at the maximum. Scales about the axis through the origin.
    pub fn taper(&mut self, axis: usize, from: f32, to: f32) {
        let b = self.bounds();
        let (lo, hi) = (b.min[axis], b.max[axis]);
        let span = (hi - lo).max(1e-6);
        for p in &mut self.positions {
            let t = (p[axis] - lo) / span;
            let s = from + (to - from) * t;
            for (k, v) in p.iter_mut().enumerate() {
                if k != axis {
                    *v *= s;
                }
            }
        }
    }

    /// Rotates about +Y by `deg` per metre of height.
    pub fn twist(&mut self, deg_per_m: f64) {
        for p in &mut self.positions {
            let r = Affine::rotate_axis_deg(1, deg_per_m * p[1] as f64);
            *p = r.apply_point(*p);
        }
    }

    /// Shears: x += lean[0]·y, z += lean[1]·y.
    pub fn lean(&mut self, lean: V2) {
        for p in &mut self.positions {
            p[0] += lean[0] * p[1];
            p[2] += lean[1] * p[1];
        }
    }

    /// Bends around the Z axis: a vertical shape curves towards +X by `deg` per metre of height
    /// (circular arc), like a hunched back or a curved horn.
    pub fn bend(&mut self, deg_per_m: f64) {
        if deg_per_m == 0.0 {
            return;
        }
        let k = (deg_per_m * dmath::PI / 180.0) as f32; // curvature (1/m)
        let r = 1.0 / k;
        for p in &mut self.positions {
            let (s, c) = dmath::sin_cos(k * p[1]);
            let x = p[0];
            // Point at height y on the bent centreline, offset by x along the rotated normal.
            p[0] = r - (r - x) * c;
            p[1] = (r - x) * s;
        }
    }

    // -----------------------------------------------------------------------------------------
    // Colour

    pub fn set_color(&mut self, c: Rgba) {
        self.colors.iter_mut().for_each(|v| *v = c);
    }

    /// Linear colour gradient along an axis (see [`Gradient`]).
    pub fn gradient(&mut self, g: &Gradient) {
        let Gradient { axis, from, to, range, absolute, strength, keep_outside } = *g;
        let b = self.bounds();
        let (lo, hi) = if absolute {
            (range[0], range[1])
        } else {
            let span = b.max[axis] - b.min[axis];
            (b.min[axis] + span * range[0], b.min[axis] + span * range[1])
        };
        let span = hi - lo;
        let (rlo, rhi) = (lo.min(hi), lo.max(hi));
        for (p, c) in self.positions.iter().zip(self.colors.iter_mut()) {
            if keep_outside && (p[axis] < rlo || p[axis] > rhi) {
                continue;
            }
            let t = if span.abs() < 1e-9 { 1.0 } else { ((p[axis] - lo) / span).clamp(0.0, 1.0) };
            *c = mix(*c, mix(from, to, t), strength);
        }
    }

    /// Horizontal (or other-axis) bands cycling through `colors`, each `width` thick, with the
    /// boundaries wobbled by noise of `wobble` metres.
    pub fn bands(&mut self, axis: usize, colors: &[Rgba], width: f32, wobble: f32, seed: u64) {
        if colors.is_empty() {
            return;
        }
        for (p, c) in self.positions.iter().zip(self.colors.iter_mut()) {
            let w = noise::fbm3(seed, math3::scale(*p, 0.7), 2) * wobble;
            let i = dmath::floor((p[axis] + w) / width.max(1e-4)) as i64;
            *c = colors[i.rem_euclid(colors.len() as i64) as usize];
        }
    }

    /// Multiplies brightness by 1 ± `amount` (fractal noise at `scale` metres).
    pub fn noise_tint(&mut self, amount: f32, scale: f32, seed: u64) {
        let inv = 1.0 / scale.max(1e-4);
        for (p, c) in self.positions.iter().zip(self.colors.iter_mut()) {
            let f = 1.0 + amount * noise::fbm3(seed, math3::scale(*p, inv), 2);
            *c = [(c[0] * f).clamp(0.0, 1.0), (c[1] * f).clamp(0.0, 1.0), (c[2] * f).clamp(0.0, 1.0), c[3]];
        }
    }

    /// Cheap ambient occlusion: darkens concave vertices (neighbours above the tangent plane)
    /// by up to `amount`.
    pub fn concavity_shade(&mut self, amount: f32) {
        // Per edge (welded by position): if the far corner of one face lies above the other
        // face's plane, the edge is a valley; its strength grows with the fold angle (90° = 1).
        let key = |p: V3| [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
        let mut group: HashMap<[u32; 3], u32> = HashMap::new();
        let gid: Vec<u32> = self
            .positions
            .iter()
            .map(|p| {
                let next = group.len() as u32;
                *group.entry(key(*p)).or_insert(next)
            })
            .collect();
        let mut edges: HashMap<(u32, u32), Vec<(usize, usize)>> = HashMap::new(); // → (face, far corner)
        for (f, t) in self.tris.iter().enumerate() {
            for k in 0..3 {
                let (a, b) = (gid[t[k] as usize], gid[t[(k + 1) % 3] as usize]);
                edges.entry((a.min(b), a.max(b))).or_default().push((f, t[(k + 2) % 3] as usize));
            }
        }
        let normal = |f: usize| {
            let t = self.tris[f];
            math3::normalize(math3::tri_normal(self.positions[t[0] as usize], self.positions[t[1] as usize], self.positions[t[2] as usize]))
        };
        let mut strength = vec![0.0f32; group.len()];
        // Order doesn't matter: each edge only raises its endpoints' maximum.
        for (&(a, b), faces) in &edges {
            if faces.len() != 2 {
                continue;
            }
            let ((f1, _), (f2, far2)) = (faces[0], faces[1]);
            let n1 = normal(f1);
            let n2 = normal(f2);
            let p1 = self.positions[self.tris[f1][0] as usize];
            let lift = math3::dot(n1, math3::sub(self.positions[far2], p1));
            if lift <= 1e-6 {
                continue;
            }
            let s = (1.0 - math3::dot(n1, n2)).clamp(0.0, 1.0);
            for g in [a, b] {
                strength[g as usize] = strength[g as usize].max(s);
            }
        }
        for (i, c) in self.colors.iter_mut().enumerate() {
            let f = 1.0 - amount * strength[gid[i] as usize];
            *c = [c[0] * f, c[1] * f, c[2] * f, c[3]];
        }
    }

    /// Darkens towards y = 0: by `amount` at the ground, fading out by `height` metres
    /// (contact shading that grounds an object).
    pub fn ground_shade(&mut self, amount: f32, height: f32) {
        for (p, c) in self.positions.iter().zip(self.colors.iter_mut()) {
            let t = (p[1] / height.max(1e-4)).clamp(0.0, 1.0);
            let f = 1.0 - amount * (1.0 - t) * (1.0 - t);
            *c = [c[0] * f, c[1] * f, c[2] * f, c[3]];
        }
    }

    // -----------------------------------------------------------------------------------------
    // Output

    /// Builds the render mesh: welds equal positions, drops degenerate triangles, computes
    /// normals (faces meeting at more than `smooth_angle_deg` get a hard edge; 0 = flat), blends
    /// the bevel tint and quantises colours to sRGB8.
    pub fn finalize(&self, smooth_angle_deg: f32, material: &str) -> MeshData {
        let mut s = self.clone();
        s.weld_exact();
        let face_n: Vec<V3> = s
            .tris
            .iter()
            .map(|t| math3::tri_normal(s.positions[t[0] as usize], s.positions[t[1] as usize], s.positions[t[2] as usize]))
            .collect();
        let face_u: Vec<V3> = face_n.iter().map(|n| math3::normalize(*n)).collect();
        // Vertex → incident faces (CSR, in triangle order).
        let nv = s.positions.len();
        let mut start = vec![0u32; nv + 1];
        for t in &s.tris {
            for &v in t {
                start[v as usize + 1] += 1;
            }
        }
        for i in 0..nv {
            start[i + 1] += start[i];
        }
        let mut fill = start.clone();
        let mut inc = vec![0u32; start[nv] as usize];
        for (f, t) in s.tris.iter().enumerate() {
            for &v in t {
                inc[fill[v as usize] as usize] = f as u32;
                fill[v as usize] += 1;
            }
        }
        let cos_t = dmath::cos((smooth_angle_deg.max(1.0) as f64 * dmath::PI / 180.0) as f32);
        let mut out = MeshData::default();
        let mut ids: HashMap<(u32, [u32; 3], [u8; 4]), u32> = HashMap::new();
        for (f, t) in s.tris.iter().enumerate() {
            let mut tri = [0u32; 3];
            for k in 0..3 {
                let v = t[k] as usize;
                let mut n = [0.0f32; 3];
                for &g in &inc[start[v] as usize..start[v + 1] as usize] {
                    if math3::dot(face_u[g as usize], face_u[f]) >= cos_t {
                        n = math3::add(n, face_n[g as usize]);
                    }
                }
                let mut n = math3::normalize(n);
                if math3::length(n) < 0.5 {
                    n = face_u[f];
                }
                let mut c = s.colors[v];
                if let (TAG_BEVEL, Some((tint, amt))) = (s.tags[f], s.bevel_tint) {
                    c = mix(c, tint, amt);
                }
                let c8 = quantize(c);
                let key = (v as u32, [n[0].to_bits(), n[1].to_bits(), n[2].to_bits()], c8);
                tri[k] = *ids.entry(key).or_insert_with(|| {
                    out.positions.push(s.positions[v]);
                    out.normals.push(n);
                    out.colors.push(c8);
                    out.positions.len() as u32 - 1
                });
            }
            out.indices.extend_from_slice(&tri);
        }
        if !out.indices.is_empty() {
            out.parts.push(Part { material: material.to_string(), first_index: 0, index_count: out.indices.len() as u32 });
        }
        out
    }

    /// Merges vertices with bit-identical positions and drops collapsed triangles.
    pub fn weld_exact(&mut self) {
        let mut ids: HashMap<[u32; 3], u32> = HashMap::new();
        let mut pos = Vec::new();
        let mut col = Vec::new();
        let mut remap = Vec::with_capacity(self.positions.len());
        for (i, p) in self.positions.iter().enumerate() {
            let key = [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
            let id = *ids.entry(key).or_insert_with(|| {
                pos.push(*p);
                col.push(self.colors[i]);
                pos.len() as u32 - 1
            });
            remap.push(id);
        }
        self.positions = pos;
        self.colors = col;
        self.remap_tris(&remap);
    }

    /// Open edges (each directed edge must be matched by exactly one reverse edge), after
    /// welding equal positions. 0 = watertight.
    pub fn open_edge_count(&self) -> usize {
        let mut s = self.clone();
        s.weld_exact();
        let mut edges: HashMap<(u32, u32), i32> = HashMap::new();
        for t in &s.tris {
            for k in 0..3 {
                *edges.entry((t[k], t[(k + 1) % 3])).or_insert(0) += 1;
            }
        }
        edges.iter().filter(|(&(a, b), &n)| n != 1 || edges.get(&(b, a)).copied().unwrap_or(0) != 1).count()
    }
}

/// Per-copy random transform for arrays: offsets up to ±`offset`, rotations up to ±`rotate_deg`
/// (X, Y, Z), uniform scale within ±`scale`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Jitter {
    pub offset: V3,
    pub rotate_deg: V3,
    pub scale: f32,
}

impl Jitter {
    fn sample(&self, rng: &mut Rng) -> Affine {
        if *self == Jitter::default() {
            return Affine::IDENTITY;
        }
        let mut r = || rng.f32() * 2.0 - 1.0;
        let o = [self.offset[0] * r(), self.offset[1] * r(), self.offset[2] * r()];
        let d = [(self.rotate_deg[0] * r()) as f64, (self.rotate_deg[1] * r()) as f64, (self.rotate_deg[2] * r()) as f64];
        let s = 1.0 + self.scale * r();
        Affine::translate(o).then_after(&Affine::rotate_euler_deg(d).then_after(&Affine::scale([s, s, s])))
    }
}

pub fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t]
}

/// 0..1 → 0..255, rounding half up (no `f32::round`: it misbehaves on some libms).
pub fn quantize(c: Rgba) -> [u8; 4] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
}

#[cfg(test)]
mod tests;
