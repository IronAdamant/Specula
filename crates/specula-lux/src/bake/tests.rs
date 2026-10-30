use super::*;
use crate::scene::{PointLight, Sky, Sun, Triangle};

/// An axis-aligned solid box's 12 triangles, normals outwards.
pub(crate) fn solid(out: &mut Vec<Triangle>, lo: [f32; 3], hi: [f32; 3], albedo: [f32; 3], emissive: [f32; 3]) {
    let c = |x: usize, y: usize, z: usize| [if x == 1 { hi[0] } else { lo[0] }, if y == 1 { hi[1] } else { lo[1] }, if z == 1 { hi[2] } else { lo[2] }];
    // (corner quad, outward normal)
    let faces: [([[usize; 3]; 4], [f32; 3]); 6] = [
        ([[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]], [1.0, 0.0, 0.0]),
        ([[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]], [-1.0, 0.0, 0.0]),
        ([[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]], [0.0, 1.0, 0.0]),
        ([[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]], [0.0, -1.0, 0.0]),
        ([[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]], [0.0, 0.0, 1.0]),
        ([[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]], [0.0, 0.0, -1.0]),
    ];
    for (q, n) in faces {
        let p: Vec<[f32; 3]> = q.iter().map(|k| c(k[0], k[1], k[2])).collect();
        out.push(Triangle { p: [p[0], p[1], p[2]], normal: n, albedo, emissive });
        out.push(Triangle { p: [p[0], p[2], p[3]], normal: n, albedo, emissive });
    }
}

/// A ground slab and a closed hut (walls 0.2 m thick, inside 3 x 2.4 x 3 m) under a uniform sky,
/// with a low sun.
pub(crate) fn hut(door: bool) -> Scene {
    let mut t = Vec::new();
    let a = [0.5; 3];
    solid(&mut t, [-12.0, -1.0, -12.0], [12.0, 0.0, 12.0], a, [0.0; 3]);
    // Walls of the hut from x, z in [0, 3.4], floor at 0, roof 2.4..2.6.
    solid(&mut t, [0.0, 0.0, 0.0], [0.2, 2.4, 3.4], a, [0.0; 3]);
    solid(&mut t, [3.2, 0.0, 0.0], [3.4, 2.4, 3.4], a, [0.0; 3]);
    solid(&mut t, [0.2, 0.0, 3.2], [3.2, 2.4, 3.4], a, [0.0; 3]);
    if door {
        // A 1.2 m door in the front wall.
        solid(&mut t, [0.2, 0.0, 0.0], [1.1, 2.4, 0.2], a, [0.0; 3]);
        solid(&mut t, [2.3, 0.0, 0.0], [3.2, 2.4, 0.2], a, [0.0; 3]);
        solid(&mut t, [1.1, 2.0, 0.0], [2.3, 2.4, 0.2], a, [0.0; 3]);
    } else {
        solid(&mut t, [0.2, 0.0, 0.0], [3.2, 2.4, 0.2], a, [0.0; 3]);
    }
    solid(&mut t, [0.0, 2.4, 0.0], [3.4, 2.6, 3.4], a, [0.0; 3]);
    Scene {
        triangles: t,
        lights: Vec::new(),
        sun: Some(Sun { direction: [0.3, 0.5, -0.8], color: [1.0; 3] }),
        sky: Some(Sky { radiance: [0.3, 0.3, 0.3], below: 1.0 }),
    }
}

fn req(min: [f32; 3], max: [f32; 3], spacing: f32, rays: u32, bounces: u32) -> Request {
    Request { min, max, spacing, rays, bounces, clamp: 0.0 }
}

fn bytes(f: &Field) -> Vec<u32> {
    f.pack().into_iter().flatten().collect()
}

#[test]
fn the_same_scene_bakes_to_the_same_bytes_on_any_thread_count() {
    let mut scene = hut(true);
    scene.lights.push(PointLight { position: [1.7, 1.5, 1.7], color: [2.0, 1.0, 0.4], range: 4.0 });
    let r = req([-1.0, 0.0, -1.0], [4.5, 3.0, 4.5], 1.0, 32, 2);
    let c = Control::default();
    let a = bake(&scene, &r, 1, &c).unwrap();
    let b = bake(&scene, &r, 3, &c).unwrap();
    let d = bake(&scene, &r, 8, &c).unwrap();
    assert_eq!(bytes(&a.field), bytes(&b.field));
    assert_eq!(bytes(&a.field), bytes(&d.field));
    assert_eq!(a.field.lattice, b.field.lattice);
    assert_eq!(a.stats.valid, b.stats.valid);
    assert!(a.stats.valid > 0 && a.stats.probes == a.field.probes.len());
    assert_eq!(c.progress().phase, Phase::Done);
}

#[test]
fn open_ground_under_a_uniform_sky_gets_the_sky() {
    let mut t = Vec::new();
    solid(&mut t, [-20.0, -1.0, -20.0], [20.0, 0.0, 20.0], [0.0; 3], [0.0; 3]);
    let scene = Scene { triangles: t, sky: Some(Sky { radiance: [0.4; 3], below: 1.0 }), ..Scene::default() };
    let baked = bake(&scene, &req([-2.0, 0.0, -2.0], [2.0, 2.0, 2.0], 1.0, 128, 1), 2, &Control::default()).unwrap();
    // A black floor: an up-facing point sees only sky; a wall-facing one half sky, half black floor.
    let up = baked.field.sample(Vec3::new(0.3, 0.0, 0.2), Vec3::Y, Vec3::ZERO, true);
    assert!((up.irradiance[0] - 0.4).abs() < 0.04, "{up:?}");
    let side = baked.field.sample(Vec3::new(0.3, 0.5, 0.2), Vec3::X, Vec3::ZERO, true);
    assert!((side.irradiance[0] - 0.2).abs() < 0.04, "{side:?}");
}

#[test]
fn a_thin_wall_does_not_leak() {
    // A closed hut: inside, there is no light at all (its walls are 0.2 m, a fifth of a spacing).
    let scene = hut(false);
    let baked = bake(&scene, &req([-2.0, 0.0, -2.0], [5.5, 3.5, 5.5], 1.0, 128, 2), 4, &Control::default()).unwrap();
    let f = &baked.field;
    let outside = f.sample(Vec3::new(-1.0, 0.0, 1.7), Vec3::Y, Vec3::ZERO, true).irradiance[0];
    assert!(outside > 0.2, "outside: {outside}");
    // Inside: the floor, each wall's inner face, the ceiling.
    let points = [
        (Vec3::new(1.7, 0.0, 1.7), Vec3::Y),
        (Vec3::new(0.2, 1.2, 1.7), Vec3::X),
        (Vec3::new(3.2, 1.2, 1.7), Vec3::new(-1.0, 0.0, 0.0)),
        (Vec3::new(1.7, 1.2, 0.2), Vec3::Z),
        (Vec3::new(1.7, 1.2, 3.2), Vec3::new(0.0, 0.0, -1.0)),
        (Vec3::new(1.7, 2.4, 1.7), Vec3::new(0.0, -1.0, 0.0)),
        (Vec3::new(0.35, 0.1, 0.35), Vec3::Y),
    ];
    for (p, n) in points {
        let e = f.sample(p, n, Vec3::ZERO, true).irradiance[0];
        assert!(e < 0.08 * outside, "leak at {p:?} facing {n:?}: {e} (outside {outside})");
    }
    // The visibility weights are what stop it: without them (quality low) the light leaks in.
    let leaky = points.iter().map(|(p, n)| f.sample(*p, *n, Vec3::ZERO, false).irradiance[0]).fold(0.0f32, f32::max);
    assert!(leaky > 0.1 * outside, "without visibility the walls leak ({leaky})");
}

/// The closed hut moved by `dx` in x and z: what [`hut`] makes, shifted against the lattice.
fn shifted_hut(dx: f32) -> Scene {
    let mut scene = hut(false);
    for t in scene.triangles.iter_mut().skip(12) {
        for v in t.p.iter_mut() {
            v[0] += dx;
            v[2] += dx;
        }
    }
    scene
}

#[test]
fn closed_huts_anywhere_on_the_lattice_stay_dark() {
    // The review's sweep: the closed hut (0.2 m walls) moved 0..0.95 m in 5 cm steps against
    // probe lattices 1 m and 0.5 m apart. No point inside (floor, ceiling, every inner wall, from
    // 5 cm off the corners) may read above 8% of the open ground. Before the watertight edges, the
    // inside-a-solid segment test, the probes' reaches and the visible-share dimming, octant
    // centres in or on a wall and segments slipping through box edges lit it at full brightness.
    for spacing in [1.0f32, 0.5] {
        for step in 0..20 {
            let dx = step as f32 * 0.05;
            let baked = bake(&shifted_hut(dx), &req([-2.0, 0.0, -2.0], [5.5, 3.5, 5.5], spacing, 128, 2), 4, &Control::default()).unwrap();
            let f = &baked.field;
            let outside = f.sample(Vec3::new(-1.0, 0.0, 1.7), Vec3::Y, Vec3::ZERO, true).irradiance[0];
            assert!(outside > 0.2);
            let mut worst = (0.0f32, Vec3::ZERO, Vec3::ZERO);
            let mut look = |p: Vec3, n: Vec3| {
                let p = p + Vec3::new(dx, 0.0, dx);
                let e = f.sample(p, n, Vec3::ZERO, true).irradiance[0];
                if e > worst.0 {
                    worst = (e, p, n);
                }
            };
            for i in 0..15 {
                let u = 0.25 + 2.9 * i as f32 / 14.0;
                for k in 0..15 {
                    let w = 0.25 + 2.9 * k as f32 / 14.0;
                    look(Vec3::new(u, 0.0, w), Vec3::Y);
                    look(Vec3::new(u, 2.4, w), -Vec3::Y);
                }
                for h in 0..8 {
                    let y = 0.1 + 2.2 * h as f32 / 7.0;
                    look(Vec3::new(0.2, y, u), Vec3::X);
                    look(Vec3::new(3.2, y, u), -Vec3::X);
                    look(Vec3::new(u, y, 0.2), Vec3::Z);
                    look(Vec3::new(u, y, 3.2), -Vec3::Z);
                }
            }
            assert!(worst.0 < 0.08 * outside, "spacing {spacing}, hut moved {dx}: {} ({:.0}% of outside) at {:?} facing {:?}", worst.0, 100.0 * worst.0 / outside, worst.1, worst.2);
        }
    }
}

#[test]
fn light_comes_in_through_a_door() {
    let open = bake(&hut(true), &req([-2.0, 0.0, -2.0], [5.5, 3.5, 5.5], 1.0, 128, 2), 4, &Control::default()).unwrap();
    let shut = bake(&hut(false), &req([-2.0, 0.0, -2.0], [5.5, 3.5, 5.5], 1.0, 128, 2), 4, &Control::default()).unwrap();
    let p = Vec3::new(1.7, 0.0, 1.0);
    let a = open.field.sample(p, Vec3::Y, Vec3::ZERO, true).irradiance[0];
    let b = shut.field.sample(p, Vec3::Y, Vec3::ZERO, true).irradiance[0];
    assert!(a > 3.0 * b + 0.005, "through the door {a}, shut {b}");
}

#[test]
fn emissive_surfaces_and_point_lights_bounce() {
    let mut scene = hut(false);
    // A glowing block in the hut, and a separate run with a lamp instead.
    let mut lit = scene.clone();
    solid(&mut scene.triangles, [1.5, 0.0, 1.5], [1.9, 0.4, 1.9], [0.8, 0.4, 0.1], [4.0, 2.0, 0.5]);
    lit.lights.push(PointLight { position: [1.7, 1.2, 1.7], color: [3.0; 3], range: 5.0 });
    let r = req([-1.0, 0.0, -1.0], [4.5, 3.0, 4.5], 1.0, 128, 1);
    let glow = bake(&scene, &r, 4, &Control::default()).unwrap();
    let lamp = bake(&lit, &r, 4, &Control::default()).unwrap();
    let dark = bake(&hut(false), &r, 4, &Control::default()).unwrap();
    let p = (Vec3::new(1.7, 2.4, 1.7), Vec3::new(0.0, -1.0, 0.0));
    let e = |b: &Baked| b.field.sample(p.0, p.1, Vec3::ZERO, true).irradiance;
    assert!(e(&glow)[0] > e(&glow)[2] && e(&glow)[0] > 0.05, "the ceiling picks up the warm glow: {:?}", e(&glow));
    assert!(e(&lamp)[0] > 0.05, "the lamp's light bounces off the floor and walls: {:?}", e(&lamp));
    assert!(e(&dark)[0] < 0.01, "{:?}", e(&dark));
}

#[test]
fn probes_inside_geometry_are_moved_out_or_dropped() {
    let mut t = Vec::new();
    // A 2.8 m solid block: probes 0.9 m deep can't get out (relocation moves 0.6 m at most).
    solid(&mut t, [-1.4, -1.4, -1.4], [1.4, 1.4, 1.4], [0.5; 3], [0.0; 3]);
    let scene = Scene { triangles: t, sky: Some(Sky { radiance: [0.2; 3], below: 1.0 }), ..Scene::default() };
    let baked = bake(&scene, &req([-2.0, -2.0, -2.0], [2.0, 2.0, 2.0], 1.0, 32, 0), 2, &Control::default()).unwrap();
    let l = baked.field.lattice;
    assert_eq!(l.origin, [-2.5; 3]);
    let centre = l.index(3, 3, 3);
    assert_eq!(l.position(centre), Vec3::new(0.5, 0.5, 0.5));
    assert_eq!(baked.field.probes[centre].weight, 0.0, "a metre deep: left inside");
    assert!(baked.stats.invalid > 0 && baked.stats.valid > 0 && baked.stats.relocated > 0, "{:?}", baked.stats);
    // Corner probes (outside) stay where they are.
    assert_eq!(baked.field.probes[0].offset, [0.0; 3]);
    assert_eq!(baked.field.probes[0].weight, 1.0);
}

#[test]
fn requests_are_checked_before_anything_runs() {
    let e = plan(&req([0.0; 3], [100.0, 20.0, 100.0], 0.5, 64, 2)).unwrap_err();
    assert_eq!((e.code, e.field), ("over_limit", Some("spacing")));
    assert!(e.message.contains("probes"));
    assert_eq!(plan(&req([0.0; 3], [1.0; 3], 0.1, 64, 2)).unwrap_err().field, Some("spacing"));
    assert_eq!(plan(&req([0.0; 3], [1.0; 3], 1.0, 4, 2)).unwrap_err().field, Some("rays"));
    assert_eq!(plan(&req([0.0; 3], [1.0; 3], 1.0, 64, 9)).unwrap_err().field, Some("bounces"));
    assert_eq!(plan(&req([1.0; 3], [0.0; 3], 1.0, 64, 1)).unwrap_err().code, "bad_bounds");
    assert_eq!(plan(&req([f32::NAN; 3], [0.0; 3], 1.0, 64, 1)).unwrap_err().code, "bad_bounds");
    // The hit cache: 40,000 probes x 1024 rays won't fit.
    let e = plan(&req([0.0; 3], [99.0, 3.0, 99.0], 1.0, 1024, 0)).unwrap_err();
    assert_eq!(e.field, Some("rays"));
    let p = plan(&req([0.0; 3], [10.0, 3.0, 10.0], 1.0, 64, 2)).unwrap();
    assert_eq!(p.lattice.count(), 12 * 5 * 12, "probes at -0.5..10.5 and -0.5..3.5");
}

#[test]
fn a_cancelled_bake_stops() {
    let c = Control::default();
    c.cancel();
    let e = bake(&hut(true), &req([-1.0, 0.0, -1.0], [4.5, 3.0, 4.5], 1.0, 32, 1), 2, &c).unwrap_err();
    assert_eq!(e.code, "cancelled");
}

#[test]
fn normals_survive_the_hit_cache() {
    for d in crate::sh::fibonacci(200) {
        let r = oct_decode(oct_encode(d));
        assert!(r.dot(d) > 0.9999, "{d:?} -> {r:?}");
    }
}

#[test]
fn the_firefly_clamp_caps_what_one_ray_brings() {
    // A tiny, very bright emitter in the open: unclamped, the probes next to it vary wildly with
    // how many of their rays happen to hit it; clamped, none brings more than the cap.
    let mut t = Vec::new();
    solid(&mut t, [-6.0, -1.0, -6.0], [6.0, 0.0, 6.0], [0.5; 3], [0.0; 3]);
    solid(&mut t, [-0.05, 0.5, -0.05], [0.05, 0.6, 0.05], [1.0; 3], [500.0; 3]);
    let scene = Scene { triangles: t, sky: Some(Sky { radiance: [0.1; 3], below: 1.0 }), ..Scene::default() };
    let mut r = req([-2.0, 0.0, -2.0], [2.0, 2.0, 2.0], 1.0, 64, 0);
    let wild = bake(&scene, &r, 2, &Control::default()).unwrap();
    r.clamp = 1.0;
    let calm = bake(&scene, &r, 2, &Control::default()).unwrap();
    let peak = |b: &Baked| b.field.probes.iter().map(|p| p.sh.luma()).fold(0.0f32, f32::max);
    assert!(peak(&wild) > 2.0, "unclamped: {}", peak(&wild));
    assert!(peak(&calm) <= 1.0 + 1e-3, "every ray at most 1, so the mean too: {}", peak(&calm));
    r.clamp = -1.0;
    assert_eq!(bake(&scene, &r, 1, &Control::default()).unwrap_err().field, Some("clamp"));
}

/// A closed, noise-displaced blob (an SDF mesh's look: crevices a few centimetres deep, the
/// T3 runestone's footing and the tree's root flares) sunk 0.3 m into the ground.
fn noisy_blob(t: &mut Vec<Triangle>, centre: [f32; 3], r: f32) {
    use specula_core::dmath::{cos, sin};
    const SEG: usize = 48;
    const RINGS: usize = 24;
    let pt = |i: usize, j: usize| -> Vec3 {
        let th = std::f32::consts::PI * j as f32 / RINGS as f32;
        let ph = 2.0 * std::f32::consts::PI * (i % SEG) as f32 / SEG as f32;
        let dir = Vec3::new(sin(th) * cos(ph), cos(th), sin(th) * sin(ph));
        // Three octaves of ridged noise: sharp creases between bumps.
        let n = 0.10 * sin(7.0 * ph + 3.0 * th).abs() + 0.06 * sin(13.0 * th + 5.0 * ph).abs() + 0.035 * sin(23.0 * ph - 17.0 * th).abs();
        let rr = r * (1.0 + n - 0.1);
        Vec3::new(centre[0], centre[1], centre[2]) + dir * rr
    };
    for j in 0..RINGS {
        for i in 0..SEG {
            let (a, b, c, d) = (pt(i, j), pt(i + 1, j), pt(i + 1, j + 1), pt(i, j + 1));
            for tri in [[a, c, b], [a, d, c]] {
                let nrm = (tri[1] - tri[0]).cross(tri[2] - tri[0]);
                if nrm.length() < 1e-9 {
                    continue;
                }
                // Outwards (away from the centre).
                let mid = (tri[0] + tri[1] + tri[2]) * (1.0 / 3.0) - Vec3::new(centre[0], centre[1], centre[2]);
                let nrm = if nrm.dot(mid) < 0.0 { -nrm.normalize() } else { nrm.normalize() };
                t.push(Triangle { p: tri.map(|v| [v.x, v.y, v.z]), normal: [nrm.x, nrm.y, nrm.z], albedo: [0.5; 3], emissive: [0.0; 3] });
            }
        }
    }
}

#[test]
fn points_beside_a_noisy_closed_mesh_are_never_black() {
    // The T3 fidelity lane's find: closed, noise-displaced SDF meshes baked at 0.25 to 0.5 m left
    // cells where every probe around a point read as hidden (black in the irradiance view, at
    // footings and root flares). A point in the open air beside the surface must always get
    // light: an octant whose centre sits inside the mesh takes the corners its open side sees
    // (`cell_mask`), and the closed-hut sweep above checks that walls still hold.
    let mut t = Vec::new();
    solid(&mut t, [-6.0, -1.0, -6.0], [6.0, 0.0, 6.0], [0.5; 3], [0.0; 3]);
    noisy_blob(&mut t, [0.0, 0.5, 0.0], 0.8);
    let scene = Scene { triangles: t.clone(), lights: Vec::new(), sun: None, sky: Some(Sky { radiance: [0.3, 0.3, 0.3], below: 1.0 }) };
    let v3 = |a: [f32; 3]| Vec3::new(a[0], a[1], a[2]);
    let bvh = crate::bvh::Bvh::build(t.iter().map(|tr| crate::bvh::Tri::new(v3(tr.p[0]), v3(tr.p[1]), v3(tr.p[2]))).collect());
    for spacing in [0.25f32, 0.5] {
        let baked = bake(&scene, &req([-1.6, 0.0, -1.6], [1.6, 1.8, 1.6], spacing, 64, 1), 4, &Control::default()).unwrap();
        let f = &baked.field;
        let (mut zero, mut tested, mut worst) = (0usize, 0usize, (f32::MAX, Vec3::ZERO));
        // Every blob triangle's centre, 3 cm out along its normal, above the ground and not
        // inside another part of the blob (open air the camera can see).
        for tr in t.iter().skip(12) {
            let c = (v3(tr.p[0]) + v3(tr.p[1]) + v3(tr.p[2])) * (1.0 / 3.0);
            let n = v3(tr.normal);
            let p = c + n * 0.03;
            let area = (v3(tr.p[1]) - v3(tr.p[0])).cross(v3(tr.p[2]) - v3(tr.p[0])).length() * 0.5;
            if p.y < 0.05 || area < 1e-3 || bvh.occluded(p, n, 1e-4, 0.5) {
                continue; // under the ground, a sliver at the sphere's pole (its normal says little), or facing into a crease closed within half a metre
            }
            tested += 1;
            let e = f.sample(p, n, n, true);
            let lum = e.irradiance[0] * e.fade;
            if lum <= 1e-4 {
                zero += 1;
            }
            if lum < worst.0 {
                worst = (lum, p);
            }
        }
        // The ground around the blob's foot (the root-flare contacts).
        for k in 0..64 {
            let a = 2.0 * std::f32::consts::PI * k as f32 / 64.0;
            for r in [0.75f32, 0.85, 0.95] {
                let p = Vec3::new(r * specula_core::dmath::cos(a), 0.0, r * specula_core::dmath::sin(a));
                if bvh.occluded(p + Vec3::Y * 0.01, Vec3::Y, 1e-4, 0.3) {
                    continue; // under the blob's overhang
                }
                tested += 1;
                let e = f.sample(p, Vec3::Y, Vec3::Y, true);
                if e.irradiance[0] * e.fade <= 1e-4 {
                    zero += 1;
                }
            }
        }
        assert!(tested > 500, "spacing {spacing}: {tested} points");
        assert_eq!(zero, 0, "spacing {spacing}: {zero} of {tested} open-air points read black (darkest {worst:?})");
    }
}

#[test]
fn a_light_inside_a_solid_is_reported() {
    // The T3 lantern: its flame's light sat inside the lantern's own cone and lit nothing. The
    // bake says which lights are inside a solid, so gi_bake and gi_check can warn.
    let mut t = Vec::new();
    solid(&mut t, [-4.0, -1.0, -4.0], [4.0, 0.0, 4.0], [0.5; 3], [0.0; 3]);
    solid(&mut t, [-0.3, 0.0, -0.3], [0.3, 0.8, 0.3], [0.5; 3], [0.0; 3]);
    let lights = vec![
        PointLight { position: [0.0, 0.4, 0.0], color: [4.0, 2.0, 1.0], range: 4.0 },
        PointLight { position: [1.5, 0.6, 0.0], color: [4.0, 2.0, 1.0], range: 4.0 },
    ];
    let scene = Scene { triangles: t, lights, sun: None, sky: Some(Sky { radiance: [0.1; 3], below: 1.0 }) };
    let baked = bake(&scene, &req([-2.0, 0.0, -2.0], [2.0, 2.0, 2.0], 1.0, 16, 1), 2, &Control::default()).unwrap();
    assert_eq!(baked.lights_inside, vec![0], "the light in the box, not the one beside it");
}
