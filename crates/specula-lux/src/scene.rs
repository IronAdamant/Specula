//! What a bake sees: triangles with their surface, the lights and the sky (linear light, the
//! renderer's units: a surface's diffuse light is `albedo × E`).

use crate::bvh::{Bvh, Tri};
use specula_core::math::Vec3;

/// One triangle of the scene, world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    pub p: [[f32; 3]; 3],
    /// The side the surface faces (from the mesh's normals): hits from behind it are back faces.
    pub normal: [f32; 3],
    /// Linear diffuse colour, 0..1.
    pub albedo: [f32; 3],
    /// Linear emitted radiance (renderer units: `albedo × emissive strength`).
    pub emissive: [f32; 3],
}

/// A point light as the renderer draws it (`shaders/mesh_frag.glsl`, `gather_light`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointLight {
    pub position: [f32; 3],
    /// Linear colour × intensity.
    pub color: [f32; 3],
    pub range: f32,
}

/// The sun (or moon): parallel light with ray-traced shadows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sun {
    /// Towards the sun (normalized by [`Scene::prepare`]).
    pub direction: [f32; 3],
    /// Linear colour × intensity.
    pub color: [f32; 3],
}

/// What rays that leave the scene see.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sky {
    /// Linear radiance (the world's ambient colour × intensity).
    pub radiance: [f32; 3],
    /// Times this below the horizon (a floating slab's mist, a closed world's floor), 0..1.
    pub below: f32,
}

impl Sky {
    pub fn radiance(&self, d: Vec3) -> [f32; 3] {
        // A soft horizon: full above, `below` under, blended over ±0.1 in y.
        let t = ((d.y + 0.1) / 0.2).clamp(0.0, 1.0);
        let k = self.below + (1.0 - self.below) * t;
        [self.radiance[0] * k, self.radiance[1] * k, self.radiance[2] * k]
    }
}

/// A bake's whole input. Order matters for determinism only as far as it is given: the engine
/// gathers entities by id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub triangles: Vec<Triangle>,
    pub lights: Vec<PointLight>,
    pub sun: Option<Sun>,
    pub sky: Option<Sky>,
}

/// Per-triangle shading data kept beside the tree.
#[derive(Clone, Copy, Debug)]
pub struct Surface {
    pub normal: Vec3,
    pub albedo: [f32; 3],
    pub emissive: [f32; 3],
}

/// A scene ready to trace.
pub struct Prepared {
    pub bvh: Bvh,
    pub surfaces: Vec<Surface>,
    pub lights: Vec<PointLight>,
    pub sun: Option<(Vec3, [f32; 3])>,
    pub sky: Sky,
}

fn v(a: [f32; 3]) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}

fn clean(c: [f32; 3], max: f32) -> [f32; 3] {
    c.map(|x| if x.is_finite() { x.clamp(0.0, max) } else { 0.0 })
}

impl Scene {
    /// Builds the tree. Degenerate and non-finite triangles are dropped; colours are clamped.
    pub fn prepare(&self) -> Prepared {
        self.prepare_with(&|| false).expect("never cancelled")
    }

    /// [`Scene::prepare`], stopping (`None`) once `cancelled` says so (the BVH build checks it).
    pub fn prepare_with(&self, cancelled: &dyn Fn() -> bool) -> Option<Prepared> {
        let mut tris = Vec::with_capacity(self.triangles.len());
        let mut surfaces = Vec::with_capacity(self.triangles.len());
        for t in &self.triangles {
            if t.p.iter().flatten().any(|x| !x.is_finite()) {
                continue;
            }
            let tri = Tri::new(v(t.p[0]), v(t.p[1]), v(t.p[2]));
            let g = tri.e1.cross(tri.e2);
            let len = g.length();
            if len < 1e-12 || !len.is_finite() {
                continue;
            }
            let mut n = g * (1.0 / len);
            // Face the way the mesh's normals say (winding conventions and mirrors don't matter).
            if n.dot(v(t.normal)) < 0.0 {
                n = -n;
            }
            tris.push(tri);
            surfaces.push(Surface { normal: n, albedo: clean(t.albedo, 1.0), emissive: clean(t.emissive, 1.0e4) });
        }
        let sun = self.sun.and_then(|s| {
            let d = v(s.direction);
            let len = d.length();
            (len > 1e-6 && len.is_finite()).then(|| (d * (1.0 / len), clean(s.color, 1.0e4)))
        });
        let lights = self
            .lights
            .iter()
            .filter(|l| l.position.iter().all(|x| x.is_finite()) && l.range.is_finite() && l.range > 0.0)
            .map(|l| PointLight { position: l.position, color: clean(l.color, 1.0e4), range: l.range.min(1.0e4) })
            .collect();
        let sky = self.sky.map_or(Sky { radiance: [0.0; 3], below: 1.0 }, |s| Sky { radiance: clean(s.radiance, 1.0e4), below: s.below.clamp(0.0, 1.0) });
        Some(Prepared { bvh: Bvh::build_with(tris, cancelled)?, surfaces, lights, sun, sky })
    }
}

impl Prepared {
    /// Direct light reaching `p` on a surface facing `n` (renderer units, before albedo): the
    /// sun and every point light in range, each with a shadow ray.
    pub fn direct(&self, p: Vec3, n: Vec3) -> [f32; 3] {
        let mut e = [0.0f32; 3];
        let o = p + n * 1e-3;
        if let Some((d, c)) = self.sun {
            let ndl = n.dot(d);
            if ndl > 0.0 && !self.bvh.occluded(o, d, 1e-4, 1.0e5) {
                for k in 0..3 {
                    e[k] += c[k] * ndl;
                }
            }
        }
        for l in &self.lights {
            let to = v(l.position) - p;
            let d2 = to.dot(to);
            if d2 >= l.range * l.range {
                continue;
            }
            let d = d2.sqrt();
            let ld = to * (1.0 / d.max(1e-5));
            let nl = n.dot(ld);
            if nl <= 0.0 {
                continue;
            }
            // The shader's window: (1 − (d/range)⁴)², over d² + 0.0025.
            let x = d / l.range;
            let x2 = x * x;
            let w = (1.0 - x2 * x2).clamp(0.0, 1.0);
            let k = w * w / (d2 + 0.0025) * nl;
            if k <= 0.0 || self.bvh.occluded(o, ld, 1e-4, (d - 2e-3).max(0.0)) {
                continue;
            }
            for (out, c) in e.iter_mut().zip(l.color) {
                *out += c * k;
            }
        }
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Triangle {
        Triangle { p: [a, b, c], normal: [0.0, 1.0, 0.0], albedo: [0.5; 3], emissive: [0.0; 3] }
    }

    #[test]
    fn bad_triangles_are_dropped_and_colours_clamped() {
        let mut s = Scene::default();
        s.triangles.push(tri([0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]));
        s.triangles.push(tri([0.0; 3], [0.0; 3], [0.0; 3]));
        s.triangles.push(tri([f32::NAN, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]));
        s.triangles[0].albedo = [2.0, -1.0, f32::NAN];
        let p = s.prepare();
        assert_eq!(p.bvh.len(), 1);
        assert_eq!(p.surfaces[0].albedo, [1.0, 0.0, 0.0]);
        // The geometric normal follows the given one (y up), whatever the winding.
        assert!(p.surfaces[0].normal.y > 0.99);
    }

    #[test]
    fn the_sun_is_shadowed_and_point_lights_fall_off_like_the_shader() {
        let mut s = Scene { sun: Some(Sun { direction: [0.0, 2.0, 0.0], color: [1.0; 3] }), ..Scene::default() };
        // A roof over x < 0.
        s.triangles.push(tri([-10.0, 3.0, -10.0], [0.0, 3.0, -10.0], [0.0, 3.0, 10.0]));
        s.triangles.push(tri([-10.0, 3.0, -10.0], [0.0, 3.0, 10.0], [-10.0, 3.0, 10.0]));
        s.lights.push(PointLight { position: [5.0, 1.0, 0.0], color: [2.0; 3], range: 4.0 });
        let p = s.prepare();
        let up = Vec3::Y;
        assert_eq!(p.direct(Vec3::new(-5.0, 0.0, 0.0), up), [0.0; 3], "under the roof");
        assert!((p.direct(Vec3::new(20.0, 0.0, 0.0), up)[0] - 1.0).abs() < 1e-6, "in the open");
        let near = p.direct(Vec3::new(5.0, 0.0, 0.0), up)[0] - 1.0;
        let x: f32 = 0.25;
        let expect = 2.0 * (1.0 - x * x * x * x) * (1.0 - x * x * x * x) / (1.0 + 0.0025);
        assert!((near - expect).abs() < 1e-4, "{near} vs {expect}");
    }
}
