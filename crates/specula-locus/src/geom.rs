//! Upright oriented boxes (turned about Y only) and their 2D (X/Z) tests: overlap, distance,
//! how far one sticks out of another.

use specula_core::dmath;
use specula_core::math::Vec3;

/// An upright box: centre, half sizes along its own axes, and its X and Z axes in the X/Z plane
/// (unit, perpendicular).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Box3 {
    pub center: Vec3,
    pub half: Vec3,
    /// Local +X in world X/Z.
    pub ax: (f32, f32),
    /// Local +Z in world X/Z.
    pub az: (f32, f32),
}

/// A yaw (degrees, about +Y) as the local X and Z axes in world X/Z.
pub fn yaw_axes(yaw_deg: f32) -> ((f32, f32), (f32, f32)) {
    let (s, c) = dmath::sin_cos(yaw_deg.to_radians());
    // R(yaw)·X = (cos, -sin) and R(yaw)·Z = (sin, cos) in (x, z).
    ((c, -s), (s, c))
}

/// The yaw (degrees) that turns local +Z toward the world direction `(dx, dz)`.
pub fn yaw_toward(dx: f32, dz: f32) -> f32 {
    dmath::atan2(dx, dz).to_degrees()
}

/// The smallest angle between two yaws, degrees (0..180).
pub fn yaw_difference(a: f32, b: f32) -> f32 {
    let mut d = (a - b) % 360.0;
    if d < 0.0 {
        d += 360.0;
    }
    if d > 180.0 {
        360.0 - d
    } else {
        d
    }
}

fn dot(a: (f32, f32), b: (f32, f32)) -> f32 {
    a.0 * b.0 + a.1 * b.1
}

impl Box3 {
    pub fn point(p: Vec3) -> Box3 {
        Box3 { center: p, half: Vec3::ZERO, ax: (1.0, 0.0), az: (0.0, 1.0) }
    }

    pub fn aligned(lo: Vec3, hi: Vec3) -> Box3 {
        Box3 { center: (lo + hi) * 0.5, half: (hi - lo) * 0.5, ax: (1.0, 0.0), az: (0.0, 1.0) }
    }

    /// A footprint: local bounds `lo..hi` at `position` turned by `yaw_deg`.
    pub fn footprint(lo: Vec3, hi: Vec3, position: Vec3, yaw_deg: f32) -> Box3 {
        let (ax, az) = yaw_axes(yaw_deg);
        let c = (lo + hi) * 0.5;
        let center = Vec3::new(position.x + ax.0 * c.x + az.0 * c.z, position.y + c.y, position.z + ax.1 * c.x + az.1 * c.z);
        Box3 { center, half: (hi - lo) * 0.5, ax, az }
    }

    pub fn top(&self) -> f32 {
        self.center.y + self.half.y
    }

    pub fn bottom(&self) -> f32 {
        self.center.y - self.half.y
    }

    /// The four X/Z corners, in order around the box.
    pub fn corners(&self) -> [(f32, f32); 4] {
        let (c, h) = ((self.center.x, self.center.z), (self.half.x, self.half.z));
        let at = |sx: f32, sz: f32| (c.0 + self.ax.0 * h.0 * sx + self.az.0 * h.1 * sz, c.1 + self.ax.1 * h.0 * sx + self.az.1 * h.1 * sz);
        [at(-1.0, -1.0), at(1.0, -1.0), at(1.0, 1.0), at(-1.0, 1.0)]
    }

    /// Smaller by `by` on every side (never below zero).
    pub fn shrunk(&self, by: f32) -> Box3 {
        let h = self.half;
        Box3 { half: Vec3::new((h.x - by).max(0.0), (h.y - by).max(0.0), (h.z - by).max(0.0)), ..*self }
    }

    /// The point's local coordinates (along ax, along az) relative to the centre.
    pub fn local(&self, p: (f32, f32)) -> (f32, f32) {
        let d = (p.0 - self.center.x, p.1 - self.center.z);
        (dot(d, self.ax), dot(d, self.az))
    }

    /// How far a point lies outside the box in X/Z (0 inside).
    pub fn outside(&self, p: (f32, f32)) -> f32 {
        let (u, v) = self.local(p);
        let (eu, ev) = ((u.abs() - self.half.x).max(0.0), (v.abs() - self.half.z).max(0.0));
        (eu * eu + ev * ev).sqrt()
    }

    /// How far `other`'s footprint sticks out of this one in X/Z (the worst corner).
    pub fn overhang(&self, other: &Box3) -> f32 {
        other.corners().iter().map(|c| self.outside(*c)).fold(0.0, f32::max)
    }

    /// Whether the X/Z footprints overlap (separating axis test on the four axes).
    pub fn overlaps_xz(&self, o: &Box3) -> bool {
        for axis in [self.ax, self.az, o.ax, o.az] {
            let proj = |b: &Box3| {
                let c = dot((b.center.x, b.center.z), axis);
                let r = b.half.x * dot(b.ax, axis).abs() + b.half.z * dot(b.az, axis).abs();
                (c - r, c + r)
            };
            let (a0, a1) = proj(self);
            let (b0, b1) = proj(o);
            if a1 <= b0 || b1 <= a0 {
                return false;
            }
        }
        true
    }

    /// Whether the boxes overlap in 3D.
    pub fn overlaps(&self, o: &Box3) -> bool {
        (self.center.y - o.center.y).abs() < self.half.y + o.half.y && self.overlaps_xz(o)
    }

    /// Distance between the X/Z footprints (0 when they overlap).
    pub fn distance_xz(&self, o: &Box3) -> f32 {
        if self.overlaps_xz(o) {
            return 0.0;
        }
        let (ca, cb) = (self.corners(), o.corners());
        let mut best = f32::INFINITY;
        for (p, edges) in ca.iter().map(|p| (p, &cb)).chain(cb.iter().map(|p| (p, &ca))) {
            for i in 0..4 {
                best = best.min(segment_distance(*p, edges[i], edges[(i + 1) % 4]));
            }
        }
        best
    }
}

/// Distance from `p` to segment `a`-`b` in 2D.
pub fn segment_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dz * dz;
    let t = if len2 > 0.0 { (((p.0 - a.0) * dx + (p.1 - a.1) * dz) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let (qx, qz) = (a.0 + dx * t - p.0, a.1 + dz * t - p.1);
    (qx * qx + qz * qz).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes() {
        let a = Box3::aligned(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 1.0, 2.0));
        let b = Box3::aligned(Vec3::new(3.0, 0.0, 0.0), Vec3::new(4.0, 1.0, 1.0));
        assert!(!a.overlaps(&b));
        assert!((a.distance_xz(&b) - 1.0).abs() < 1.0e-6);
        let turned = Box3::footprint(Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 1.0, 0.5), Vec3::new(2.5, 0.0, 1.0), 45.0);
        // Turned 45 degrees, its corner reaches 0.707 toward a.
        assert!(turned.overlaps(&a));
        assert_eq!(a.overhang(&Box3::aligned(Vec3::new(0.5, 0.0, 0.5), Vec3::new(1.5, 1.0, 1.5))), 0.0);
        assert!((a.overhang(&Box3::aligned(Vec3::new(1.5, 0.0, 0.5), Vec3::new(2.5, 1.0, 1.5))) - 0.5).abs() < 1.0e-6);
        assert!((yaw_toward(1.0, 0.0) - 90.0).abs() < 1.0e-4);
        assert!((yaw_difference(350.0, 10.0) - 20.0).abs() < 1.0e-4);
        let (ax, az) = yaw_axes(90.0);
        assert!((ax.0).abs() < 1.0e-6 && (ax.1 + 1.0).abs() < 1.0e-6 && (az.0 - 1.0).abs() < 1.0e-6);
    }
}
