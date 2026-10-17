//! Queries against single placed shapes: raycasts (exact per shape), shape casts (conservative
//! advancement on GJK distance: any convex caster against any shape) and overlap tests.
//!
//! Rays starting inside a convex shape don't hit it (as in Box2D and most engines); triangles
//! are two-sided.

use crate::contact::{collide, convex, shape_bounds_in, Convex, Manifold};
use crate::gjk::{gjk, Core, Support, Triangle};
use crate::math::{get, inv_dir, set, v3, Aabb, Pose, Vec3};
use crate::shape::{Shape, TriMesh};

/// A ray hit: distance along the (unit) direction, the point, the surface normal facing the
/// ray, and the triangle index for meshes (else 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub distance: f32,
    pub point: Vec3,
    pub normal: Vec3,
    pub triangle: u32,
}

/// The first hit of the ray `origin + t·dir` (`dir` unit length, `t ≤ max`) on `shape` at
/// `pose`.
pub fn ray_shape(shape: &Shape, pose: &Pose, origin: Vec3, dir: Vec3, max: f32) -> Option<RayHit> {
    let o = pose.inv_point(origin);
    let d = pose.inv_vector(dir);
    let local = match shape {
        Shape::Sphere { radius } => ray_sphere(o, d, Vec3::ZERO, *radius, max),
        Shape::Capsule { radius, half_height } => ray_capsule(o, d, *radius, *half_height, max),
        Shape::Box { half, .. } => ray_box(o, d, *half, max),
        Shape::Hull(h) => ray_hull(h, o, d, max),
        Shape::Mesh(m) => ray_mesh(m, o, d, max),
        Shape::Heightfield(h) => ray_mesh(&h.mesh, o, d, max),
    }?;
    Some(RayHit { distance: local.0, point: origin + dir * local.0, normal: pose.vector(local.1), triangle: local.2 })
}

type Local = (f32, Vec3, u32);

fn ray_sphere(o: Vec3, d: Vec3, c: Vec3, r: f32, max: f32) -> Option<Local> {
    let m = o - c;
    let b = m.dot(d);
    let cc = m.dot(m) - r * r;
    if cc <= 0.0 {
        return None; // starts inside
    }
    if b > 0.0 {
        return None;
    }
    let disc = b * b - cc;
    if disc < 0.0 {
        return None;
    }
    let t = -b - disc.sqrt();
    if t < 0.0 || t > max {
        return None;
    }
    let p = o + d * t;
    Some((t, crate::math::normalize_or(p - c, -d), 0))
}

fn ray_capsule(o: Vec3, d: Vec3, r: f32, h: f32, max: f32) -> Option<Local> {
    // Inside the capsule: no hit.
    let (q, _) = crate::math::closest_on_segment(o, v3(0.0, -h, 0.0), v3(0.0, h, 0.0));
    if crate::math::len_sq(o - q) <= r * r {
        return None;
    }
    let mut best: Option<Local> = None;
    let mut consider = |hit: Option<Local>| {
        if let Some(x) = hit {
            if best.is_none_or(|b| x.0 < b.0) {
                best = Some(x);
            }
        }
    };
    // The side of the cylinder (x² + z² = r², |y| ≤ h).
    let a = d.x * d.x + d.z * d.z;
    if a > 1.0e-12 {
        let b = o.x * d.x + o.z * d.z;
        let c = o.x * o.x + o.z * o.z - r * r;
        let disc = b * b - a * c;
        if disc >= 0.0 {
            let t = (-b - disc.sqrt()) / a;
            if t >= 0.0 && t <= max {
                let p = o + d * t;
                if p.y.abs() <= h {
                    consider(Some((t, crate::math::normalize_or(v3(p.x, 0.0, p.z), -d), 0)));
                }
            }
        }
    }
    consider(ray_sphere(o, d, v3(0.0, h, 0.0), r, max));
    consider(ray_sphere(o, d, v3(0.0, -h, 0.0), r, max));
    best
}

fn ray_box(o: Vec3, d: Vec3, half: Vec3, max: f32) -> Option<Local> {
    let b = Aabb::new(-half, half);
    if b.contains_point(o) {
        return None;
    }
    let inv = inv_dir(d);
    let (t0, _) = b.ray(o, inv, max)?;
    // The entry face: the axis whose slab was entered last.
    let p = o + d * t0;
    let mut n = Vec3::ZERO;
    let mut best = f32::MIN;
    for i in 0..3 {
        let x = get(p, i).abs() - get(half, i);
        if x > best {
            best = x;
            n = Vec3::ZERO;
            set(&mut n, i, if get(p, i) >= 0.0 { 1.0 } else { -1.0 });
        }
    }
    Some((t0, n, 0))
}

fn ray_hull(h: &crate::shape::Hull, o: Vec3, d: Vec3, max: f32) -> Option<Local> {
    let mut t_in = 0.0f32;
    let mut t_out = max;
    let mut n_in: Option<Vec3> = None;
    let mut inside = true;
    for f in 0..h.face_count() {
        let n = h.normals[f];
        // Positive: the origin is on the inner side of this face's plane.
        let num = h.offsets[f] - n.dot(o);
        let den = n.dot(d);
        inside &= num >= 0.0;
        if den == 0.0 {
            if num < 0.0 {
                return None;
            }
            continue;
        }
        let t = num / den;
        if den < 0.0 {
            if t > t_in || n_in.is_none() && t >= t_in {
                t_in = t;
                n_in = Some(n);
            }
        } else if t < t_out {
            t_out = t;
        }
        if t_in > t_out {
            return None;
        }
    }
    if inside {
        return None;
    }
    n_in.map(|n| (t_in, n, 0))
}

/// Möller–Trumbore, two-sided; the normal faces the ray.
pub fn ray_triangle(o: Vec3, d: Vec3, t: &[Vec3; 3], max: f32) -> Option<(f32, Vec3)> {
    let e1 = t[1] - t[0];
    let e2 = t[2] - t[0];
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1.0e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - t[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let dist = e2.dot(q) * inv;
    if dist < 0.0 || dist > max {
        return None;
    }
    let n = crate::math::normalize_or(e1.cross(e2), Vec3::Y);
    Some((dist, if n.dot(d) > 0.0 { -n } else { n }))
}

fn ray_mesh(m: &TriMesh, o: Vec3, d: Vec3, max: f32) -> Option<Local> {
    let mut best: Option<Local> = None;
    m.each_on_ray(o, d, max, |t, cur| {
        if let Some((dist, n)) = ray_triangle(o, d, &m.triangle(t), cur) {
            // Equal distances: the lower triangle index wins (deterministic).
            if best.is_none_or(|b| dist < b.0 || dist == b.0 && (t as u32) < b.2) {
                best = Some((dist, n, t as u32));
                return dist;
            }
        }
        cur
    });
    best
}

/// A shape cast's hit: the fraction of the motion travelled, the contact point and the normal
/// of the surface hit (facing the caster).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CastHit {
    pub fraction: f32,
    pub point: Vec3,
    pub normal: Vec3,
    /// The caster already overlapped the target at the start (`fraction` 0).
    pub initially_overlapping: bool,
    pub triangle: u32,
}

const CAST_ITERATIONS: usize = 32;

/// Casts convex `caster` from `from` along `delta` against `target` at `target_pose`; the hit
/// is where the gap first falls to `skin` (a small positive distance keeps casts from ending
/// inside surfaces).
pub fn cast_shape(caster: &Shape, from: &Pose, delta: Vec3, target: &Shape, target_pose: &Pose, skin: f32) -> Option<CastHit> {
    let c = convex(caster, from)?;
    if let Some(mesh) = target.triangles() {
        let local = caster.local_aabb();
        let start = shape_bounds_in(&local, &target_pose.inv_mul(from));
        let swept = start.swept(target_pose.inv_vector(delta)).expand(skin);
        let mut best: Option<CastHit> = None;
        let mut tested = 0;
        mesh.each_in(&swept, |t| {
            tested += 1;
            if tested > 256 {
                return false;
            }
            let tri = Triangle::new(mesh.triangle(t).map(|v| target_pose.point(v)));
            if let Some(mut h) = cast_convex(&c, delta, &tri, 0.0, skin) {
                h.triangle = t as u32;
                if best.is_none_or(|b| h.fraction < b.fraction) {
                    best = Some(h);
                }
            }
            true
        });
        return best;
    }
    match convex(target, target_pose)? {
        Convex::Core(core, r) => cast_convex(&c, delta, &core, r, skin),
        Convex::Poly(p) => cast_convex(&c, delta, &p, 0.0, skin),
    }
}

fn cast_convex<B: Support>(caster: &Convex, delta: Vec3, target: &B, target_radius: f32, skin: f32) -> Option<CastHit> {
    let (core_r, at) = match caster {
        Convex::Core(core, r) => (*r, CasterAt::Core(*core)),
        Convex::Poly(p) => (0.0, CasterAt::Poly(*p)),
    };
    let radius = core_r + target_radius;
    let mut t = 0.0f32;
    let target_gap = skin.max(1.0e-4);
    for i in 0..CAST_ITERATIONS {
        let moved = delta * t;
        let c = match &at {
            CasterAt::Core(core) => gjk(&core.translated(moved), target),
            CasterAt::Poly(p) => gjk(&Shifted { inner: p, by: moved }, target),
        };
        let gap = c.distance - radius;
        if c.overlap || gap <= target_gap {
            // Within the skin but not closing in: the gap only grows from here (both convex).
            if !c.overlap && gap >= 0.0 && delta.dot(c.b - c.a) <= 0.0 {
                return None;
            }
            if i == 0 && (c.overlap || gap < 0.0) {
                // Already touching at the start.
                let n = crate::math::try_normalize(c.a - c.b).unwrap_or(-crate::math::normalize_or(delta, Vec3::Y));
                return Some(CastHit { fraction: 0.0, point: c.b + n * target_radius, normal: n, initially_overlapping: true, triangle: 0 });
            }
            let n = crate::math::try_normalize(c.a - c.b).unwrap_or(-crate::math::normalize_or(delta, Vec3::Y));
            return Some(CastHit { fraction: t, point: c.b + n * target_radius, normal: n, initially_overlapping: false, triangle: 0 });
        }
        let n = (c.b - c.a) * (1.0 / c.distance.max(1.0e-12));
        let approach = delta.dot(n);
        if approach <= 1.0e-9 {
            return None;
        }
        t += (gap - target_gap * 0.5) / approach;
        if t > 1.0 {
            return None;
        }
    }
    None
}

enum CasterAt<'a> {
    Core(Core),
    Poly(crate::gjk::PosedHull<'a>),
}

struct Shifted<'a, 'b> {
    inner: &'b crate::gjk::PosedHull<'a>,
    by: Vec3,
}

impl Support for Shifted<'_, '_> {
    fn support(&self, d: Vec3) -> Vec3 {
        self.inner.support(d) + self.by
    }

    fn center(&self) -> Vec3 {
        self.inner.center() + self.by
    }
}

/// Whether `a` at `pa` and `b` at `pb` overlap.
pub fn overlap(a: &Shape, pa: &Pose, b: &Shape, pb: &Pose) -> bool {
    let mut m = Manifold::default();
    collide(a, pa, b, pb, 0.0, &mut m);
    m.as_slice().iter().any(|p| p.separation <= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula_core::math::Quat;

    #[test]
    fn rays_hit_every_shape_kind() {
        let o = v3(-5.0, 0.0, 0.0);
        let d = Vec3::X;
        let h = ray_shape(&Shape::sphere(1.0), &Pose::IDENTITY, o, d, 100.0).unwrap();
        assert!((h.distance - 4.0).abs() < 1e-5 && (h.normal + Vec3::X).length() < 1e-5);
        let h = ray_shape(&Shape::cuboid(v3(1.0, 1.0, 1.0)), &Pose::at(v3(1.0, 0.0, 0.0)), o, d, 100.0).unwrap();
        assert!((h.distance - 5.0).abs() < 1e-5 && (h.normal + Vec3::X).length() < 1e-5);
        let lying = Pose::new(Vec3::ZERO, Quat::from_euler_xyz_deg(0.0, 0.0, 90.0));
        let h = ray_shape(&Shape::capsule(0.5, 1.0), &lying, o, d, 100.0).unwrap();
        assert!((h.distance - 3.5).abs() < 1e-4, "{h:?}");
        let h = ray_shape(&Shape::capsule(0.5, 1.0), &Pose::IDENTITY, v3(0.0, 5.0, 0.0), -Vec3::Y, 100.0).unwrap();
        assert!((h.distance - 3.5).abs() < 1e-4 && h.normal.y > 0.999, "{h:?}");
        let hull = Shape::hull(&[v3(-1.0, -1.0, -1.0), v3(1.0, -1.0, -1.0), v3(0.0, -1.0, 1.0), v3(0.0, 1.0, 0.0)]).unwrap();
        let h = ray_shape(&hull, &Pose::IDENTITY, v3(0.0, -5.0, 0.0), Vec3::Y, 100.0).unwrap();
        assert!((h.distance - 4.0).abs() < 1e-5 && h.normal.y < -0.999);
        let floor = Shape::heightfield(3, 3, vec![0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0], (1.0, 1.0)).unwrap();
        let h = ray_shape(&floor, &Pose::IDENTITY, v3(0.0, 5.0, 0.0), -Vec3::Y, 100.0).unwrap();
        assert!((h.distance - 4.0).abs() < 1e-5, "{h:?}");
        // Starting inside: no hit; too short: no hit.
        assert!(ray_shape(&Shape::sphere(1.0), &Pose::IDENTITY, Vec3::ZERO, d, 100.0).is_none());
        assert!(ray_shape(&Shape::sphere(1.0), &Pose::IDENTITY, o, d, 3.0).is_none());
    }

    #[test]
    fn casts_stop_at_the_skin() {
        let ground = Shape::cuboid(v3(5.0, 0.5, 5.0));
        let gp = Pose::at(v3(0.0, -0.5, 0.0));
        let h = cast_shape(&Shape::sphere(0.5), &Pose::at(v3(0.0, 3.0, 0.0)), v3(0.0, -10.0, 0.0), &ground, &gp, 0.01).unwrap();
        // Touches when the centre reaches y = 0.5 (+ skin): fraction about 0.25.
        assert!((h.fraction - 0.25).abs() < 0.002 && h.normal.y > 0.999, "{h:?}");
        let cube = Shape::cuboid(v3(0.5, 0.5, 0.5));
        let h = cast_shape(&cube, &Pose::at(v3(-3.0, 0.5, 0.0)), v3(6.0, 0.0, 0.0), &Shape::sphere(0.5), &Pose::at(v3(1.0, 0.5, 0.0)), 0.01).unwrap();
        assert!((h.fraction - 3.0 / 6.0).abs() < 0.003 && h.normal.x < -0.999, "{h:?}");
        // Moving away: no hit. Starting inside: fraction 0.
        assert!(cast_shape(&Shape::sphere(0.5), &Pose::at(v3(0.0, 3.0, 0.0)), v3(0.0, 1.0, 0.0), &ground, &gp, 0.01).is_none());
        let h = cast_shape(&Shape::sphere(0.5), &Pose::at(v3(0.0, 0.2, 0.0)), v3(1.0, 0.0, 0.0), &ground, &gp, 0.01).unwrap();
        assert!(h.initially_overlapping && h.fraction == 0.0);
        // Against a mesh.
        let floor = Shape::mesh(vec![v3(-5.0, 0.0, -5.0), v3(5.0, 0.0, -5.0), v3(0.0, 0.0, 5.0)], vec![[0, 2, 1]]).unwrap();
        let h = cast_shape(&Shape::capsule(0.3, 0.6), &Pose::at(v3(0.0, 2.0, 0.0)), v3(0.0, -4.0, 0.0), &floor, &Pose::IDENTITY, 0.01).unwrap();
        assert!((h.fraction - (2.0 - 0.9) / 4.0).abs() < 0.003, "{h:?}");
    }

    #[test]
    fn overlaps() {
        let a = Shape::cuboid(v3(1.0, 1.0, 1.0));
        assert!(overlap(&a, &Pose::IDENTITY, &Shape::sphere(0.5), &Pose::at(v3(1.4, 0.0, 0.0))));
        assert!(!overlap(&a, &Pose::IDENTITY, &Shape::sphere(0.5), &Pose::at(v3(1.6, 0.0, 0.0))));
    }
}
