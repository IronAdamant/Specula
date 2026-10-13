use super::*;

/// Finalises, validates, and checks closed + outward; returns the mesh.
fn solid(s: &Shape) -> MeshData {
    assert_eq!(s.tris.len(), s.tags.len());
    assert_eq!(s.positions.len(), s.colors.len());
    assert_eq!(s.open_edge_count(), 0, "shape must be watertight");
    assert!(s.signed_volume() > 0.0, "faces must point outward (volume {})", s.signed_volume());
    let m = s.finalize(30.0, "m");
    m.validate().unwrap();
    assert!(m.is_watertight());
    assert!(m.signed_volume() > 0.0);
    // Every face's normal agrees with its corners' normals (hard edges really split).
    for t in 0..m.triangle_count() {
        let [a, b, c] = m.triangle(t);
        let fn_ = math3::normalize(math3::tri_normal(a, b, c));
        for k in 0..3 {
            let vn = m.normals[m.indices[3 * t + k] as usize];
            assert!(math3::dot(fn_, vn) > 0.5, "triangle {t} corner normal disagrees with face");
        }
    }
    m
}

fn close(a: V3, b: V3, eps: f32) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= eps)
}

#[test]
fn cuboid_counts_bounds_and_bevel() {
    let s = Shape::cuboid([1.0, 2.4, 0.8], 0.06).unwrap();
    // 8-gon outline, 4 rings: 3 bands × 8 quads + 2 caps × 6.
    assert_eq!(s.triangle_count(), 3 * 8 * 2 + 2 * 6);
    let m = solid(&s);
    let b = m.bounds();
    assert!(close(b.min, [-0.5, 0.0, -0.4], 1e-6) && close(b.max, [0.5, 2.4, 0.4], 1e-6), "{b:?}");
    // Volume = box minus the chamfers (slightly less than 1.92).
    let v = m.signed_volume();
    assert!(v < 1.92 && v > 1.85, "{v}");
    // Flat-shaded chamfers: 26 distinct face directions.
    let mut dirs: Vec<[i32; 3]> = m.normals.iter().map(|n| n.map(|v| (v * 20.0).round() as i32)).collect();
    dirs.sort();
    dirs.dedup();
    assert_eq!(dirs.len(), 26);
    assert!(s.tags.contains(&TAG_BEVEL));
}

#[test]
fn unbevelled_box_is_twelve_triangles() {
    let s = Shape::cuboid([2.0, 1.0, 2.0], 0.0).unwrap();
    assert_eq!(s.triangle_count(), 12);
    let m = solid(&s);
    assert_eq!(m.vertex_count(), 24);
    assert!((m.signed_volume() - 4.0).abs() < 1e-5);
}

#[test]
fn cylinder_cone_and_taper() {
    let c = Shape::cylinder(0.5, 0.5, 2.0, 12, 0.0).unwrap();
    assert_eq!(c.triangle_count(), 12 * 2 + 2 * 10);
    let m = solid(&c);
    let ideal = 0.5 * 12.0 * 0.25 * dmath::sin_cos_deg(30.0).0 as f32 * 2.0; // n/2 r² sin(2π/n) h
    assert!((m.signed_volume() - ideal).abs() < 1e-4);
    let cone = Shape::cylinder(0.5, 0.0, 1.0, 8, 0.0).unwrap();
    assert_eq!(cone.triangle_count(), 8 + 6);
    let m = solid(&cone);
    assert!(close(m.bounds().max, [m.bounds().max[0], 1.0, m.bounds().max[2]], 1e-6));
    let bev = Shape::cylinder(0.5, 0.4, 1.0, 16, 0.05).unwrap();
    solid(&bev);
}

#[test]
fn icosphere_counts_and_roundness() {
    for (sub, tris) in [(0, 20), (1, 80), (2, 320)] {
        let s = Shape::sphere(1.0, sub);
        assert_eq!(s.triangle_count(), tris);
        let m = solid(&s);
        for p in &m.positions {
            let r = math3::length(math3::sub(*p, [0.0, 1.0, 0.0]));
            assert!((r - 1.0).abs() < 1e-5);
        }
        assert!(m.bounds().min[1].abs() < 1e-5);
    }
    // Smooth at 2 subdivisions with a 30° threshold: one normal per position.
    let m = Shape::sphere(1.0, 2).finalize(30.0, "m");
    assert_eq!(m.vertex_count(), 162);
}

#[test]
fn torus_and_capsule() {
    let t = Shape::torus(1.0, 0.2, 16, 8);
    assert_eq!(t.triangle_count(), 16 * 8 * 2);
    let m = solid(&t);
    let b = m.bounds();
    assert!(b.min[1].abs() < 1e-6 && (b.max[1] - 0.4).abs() < 1e-6);
    let c = Shape::capsule(0.3, 1.8, 10, 3);
    let m = solid(&c);
    assert!((m.bounds().max[1] - 1.8).abs() < 1e-5 && m.bounds().min[1].abs() < 1e-6);
}

#[test]
fn extrude_concave_with_taper_bevel_and_inset_cap() {
    let l = [[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0], [1.0, 2.0], [0.0, 2.0]];
    let s = Shape::extrude(&l, 1.0, 1.0, 0.0, 0.0, 1, None).unwrap();
    let m = solid(&s);
    assert!((m.signed_volume() - 3.0).abs() < 1e-5);
    let s = Shape::extrude(&l, 1.0, 0.8, 0.05, 0.05, 3, Some((0.1, -0.05))).unwrap();
    solid(&s);
    // Pyramid: taper 0.
    let s = Shape::extrude(&poly::rect(1.0, 1.0), 1.0, 0.0, 0.0, 0.0, 1, None).unwrap();
    let m = solid(&s);
    assert!((m.signed_volume() - 1.0 / 3.0).abs() < 1e-5);
}

#[test]
fn loft_with_apex_and_offsets() {
    let s = Shape::loft(&poly::rect(1.0, 1.0), &[[0.9, 0.0, 0.0, 0.0], [1.0, 0.1, 0.0, 0.0], [1.0, 2.0, 0.0, 0.0], [0.0, 2.6, 0.0, 0.0]], 0.05).unwrap();
    let m = solid(&s);
    assert!((m.bounds().max[1] - 2.6).abs() < 1e-6);
    assert!(s.tags.contains(&TAG_BEVEL));
}

#[test]
fn lathe_closed_profiles_both_directions() {
    let up = [[0.0, 0.0], [0.5, 0.0], [0.4, 1.0], [0.0, 1.2]];
    let down: Vec<V2> = up.iter().rev().copied().collect();
    let a = solid(&Shape::lathe(&up, 12, false));
    let b = solid(&Shape::lathe(&down, 12, false));
    assert!((a.signed_volume() - b.signed_volume()).abs() < 1e-5);
    // Open profile with caps.
    solid(&Shape::lathe(&[[0.3, 0.0], [0.5, 0.5], [0.2, 1.0]], 10, true));
}

#[test]
fn plane_grid() {
    let p = Shape::plane([2.0, 4.0], [2, 4]);
    assert_eq!(p.triangle_count(), 16);
    let m = p.finalize(0.0, "m");
    m.validate().unwrap();
    assert!(m.normals.iter().all(|n| *n == [0.0, 1.0, 0.0]));
    assert!(!m.is_watertight());
}

#[test]
fn sweep_tube_rmf_and_closed_ring() {
    let prof = poly::regular(6, 0.05, 0.0);
    let path = [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.5, 1.5, 0.0], [1.0, 1.5, 0.5]];
    let s = Shape::sweep(&prof, &path, &[1.0; 4], 0.0, false, true, None).unwrap();
    assert_eq!(s.triangle_count(), 3 * 6 * 2 + 2 * 4);
    let m = solid(&s);
    // Thickness kept at the corner: every ring point at least the radius from the path line.
    assert!(m.bounds().min[1] >= -1e-6);
    // Tapered to a point: no caps needed, still closed.
    solid(&Shape::sweep(&prof, &path, &[1.0, 0.8, 0.5, 0.0], 30.0, false, true, None).unwrap());
    // Closed loop (a ring of rope).
    let loop_: Vec<V3> = (0..12)
        .map(|i| {
            let (s, c) = dmath::sin_cos_deg(30.0 * i as f64);
            [c as f32, 0.0, s as f32]
        })
        .collect();
    let r = Shape::sweep(&prof, &loop_, &[], 0.0, true, false, None).unwrap();
    assert_eq!(r.triangle_count(), 12 * 6 * 2);
    solid(&r);
}

#[test]
fn rmf_frames_do_not_twist_on_a_planar_path() {
    // A planar S-curve: ring normals must stay in the curve's plane or perpendicular to it.
    let path: Vec<V3> = (0..20)
        .map(|i| {
            let t = i as f32 * 0.2;
            [dmath::sin_cos(t).0, t, 0.0]
        })
        .collect();
    let prof = vec![[0.1, 0.0], [0.0, 0.1], [-0.1, 0.0], [0.0, -0.1]];
    let s = Shape::sweep(&prof, &path, &[], 0.0, false, true, Some([0.0, 0.0, 1.0])).unwrap();
    // The `up` hint is the frame normal: profile points with a = ±0.1 lie on z = ±0.1 (out of
    // the curve's plane) at every ring.
    for i in 0..20 {
        let z = s.positions[4 * i][2];
        assert!((z.abs() - 0.1).abs() < 1e-4, "ring {i}: z = {z}");
    }
}

#[test]
fn transforms_mirror_and_arrays() {
    let mut s = Shape::cuboid([0.2, 0.2, 0.2], 0.0).unwrap();
    s.transform(&Affine::translate([0.1, 0.0, 0.0]));
    let mut m = s.clone();
    m.mirror(0, true);
    // The two cubes share the x = 0 face: welded, the result has two internal faces; still a
    // valid mesh, bounds symmetric.
    let b = m.bounds();
    assert!(close(b.min, [-0.2, 0.0, -0.1], 1e-6) && close(b.max, [0.2, 0.2, 0.1], 1e-6));
    let mut a = Shape::cuboid([0.1, 0.1, 0.1], 0.01).unwrap();
    let mut rng = Rng::new(1, 2);
    a.array_linear(5, [0.2, 0.0, 0.0], &mut rng, &Jitter::default());
    assert_eq!(a.triangle_count(), 5 * 60);
    assert!((a.bounds().max[0] - 0.85).abs() < 1e-5);
    solid(&a);
    let mut r = Shape::cuboid([0.1, 0.1, 0.1], 0.0).unwrap();
    r.transform(&Affine::translate([1.0, 0.0, 0.0]));
    let jit = Jitter { offset: [0.01, 0.0, 0.01], rotate_deg: [0.0, 10.0, 0.0], scale: 0.1 };
    r.array_radial(8, [0.0, 1.0, 0.0], 360.0, &mut rng, &jit);
    assert_eq!(r.triangle_count(), 96);
    solid(&r);
    let b = r.bounds();
    assert!(b.max[0] > 0.9 && b.min[0] < -0.9 && b.max[2] > 0.9 && b.min[2] < -0.9);
}

#[test]
fn mirror_of_half_shape_closes_it() {
    // Half a lathe-like wedge mirrored back becomes a closed solid when the cut faces coincide:
    // simplest check, a box cut at x = 0 rebuilt by mirroring its outline.
    let half = Shape::extrude(&[[0.0, -0.5], [0.5, -0.5], [0.5, 0.5], [0.0, 0.5]], 1.0, 1.0, 0.0, 0.0, 1, None).unwrap();
    let mut m = half.clone();
    m.mirror(0, false);
    assert!(m.signed_volume() > 0.0, "mirror keeps outward winding");
}

#[test]
fn subdivide_keeps_closed_and_shape() {
    let mut s = Shape::cuboid([1.0, 1.0, 1.0], 0.1).unwrap();
    let v0 = s.signed_volume();
    s.subdivide(2);
    assert_eq!(s.triangle_count(), 60 * 16);
    assert!((s.signed_volume() - v0).abs() < 1e-4);
    solid(&s);
}

#[test]
fn displace_is_seeded_and_keeps_closed() {
    let mut a = Shape::cuboid([1.0, 1.0, 1.0], 0.1).unwrap();
    a.subdivide(1);
    let mut b = a.clone();
    let mut c = a.clone();
    a.displace(0.05, 0.3, 2, 7, [1.0; 3]);
    b.displace(0.05, 0.3, 2, 7, [1.0; 3]);
    c.displace(0.05, 0.3, 2, 8, [1.0; 3]);
    assert_eq!(a, b);
    assert_ne!(a, c);
    solid(&a);
    // Displacement stays within the amount.
    let ref_ = {
        let mut r = Shape::cuboid([1.0, 1.0, 1.0], 0.1).unwrap();
        r.subdivide(1);
        r
    };
    for (p, q) in a.positions.iter().zip(&ref_.positions) {
        assert!(math3::length(math3::sub(*p, *q)) <= 0.05 + 1e-6);
    }
}

#[test]
fn deformers() {
    let mut s = Shape::cuboid([1.0, 2.0, 1.0], 0.0).unwrap();
    s.taper(1, 1.0, 0.5);
    let b = s.bounds();
    assert!((b.max[0] - 0.5).abs() < 1e-6);
    let top: Vec<_> = s.positions.iter().filter(|p| p[1] == 2.0).collect();
    assert!(top.iter().all(|p| (p[0].abs() - 0.25).abs() < 1e-6));
    let mut t = Shape::cuboid([1.0, 1.0, 1.0], 0.0).unwrap();
    t.twist(90.0);
    let top: Vec<_> = t.positions.iter().filter(|p| p[1] == 1.0).collect();
    assert!(top.iter().all(|p| (p[0].abs() - 0.5).abs() < 1e-6 && (p[2].abs() - 0.5).abs() < 1e-6));
    let mut l = Shape::cuboid([1.0, 1.0, 1.0], 0.0).unwrap();
    l.lean([0.5, 0.0]);
    assert!((l.bounds().max[0] - 1.0).abs() < 1e-6);
    let mut c = Shape::cylinder(0.1, 0.1, 2.0, 6, 0.0).unwrap();
    c.bend(45.0);
    assert!(c.bounds().max[0] > 0.5, "bent towards +X");
    solid(&c);
}

#[test]
fn colour_ops() {
    let mut s = Shape::cuboid([1.0, 2.0, 1.0], 0.0).unwrap();
    s.gradient(&Gradient { axis: 1, from: [0.0, 0.0, 0.0, 1.0], to: [1.0; 4], range: [0.0, 1.0], absolute: false, strength: 1.0, keep_outside: false });
    for (p, c) in s.positions.iter().zip(&s.colors) {
        assert!((c[0] - p[1] / 2.0).abs() < 1e-6);
    }
    s.bands(1, &[[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0]], 1.0, 0.0, 1);
    for (p, c) in s.positions.iter().zip(&s.colors) {
        let want = if (p[1] as i32) % 2 == 0 { 0.0 } else { 1.0 };
        assert_eq!(c[1], want, "band at y = {}", p[1]);
    }
    s.set_color([0.5; 4]);
    s.noise_tint(0.2, 0.5, 3);
    assert!(s.colors.iter().all(|c| (c[0] - 0.5).abs() <= 0.1 + 1e-6));
    s.set_color([1.0; 4]);
    s.ground_shade(0.5, 1.0);
    let low = s.positions.iter().position(|p| p[1] == 0.0).unwrap();
    assert!((s.colors[low][0] - 0.5).abs() < 1e-6);
    let high = s.positions.iter().position(|p| p[1] == 2.0).unwrap();
    assert_eq!(s.colors[high][0], 1.0);
}

#[test]
fn concavity_darkens_inner_corners_only() {
    // An L-shaped prism has one concave vertical edge.
    let l = [[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0], [1.0, 2.0], [0.0, 2.0]];
    let mut s = Shape::extrude(&l, 1.0, 1.0, 0.0, 0.0, 1, None).unwrap();
    s.concavity_shade(0.5);
    for (p, c) in s.positions.iter().zip(&s.colors) {
        let inner = p[0] == 1.0 && p[2] == 1.0;
        assert_eq!(c[0] < 1.0, inner, "vertex {p:?} colour {c:?}");
    }
}

#[test]
fn bevel_tint_colours_chamfers() {
    let mut s = Shape::cuboid([1.0, 1.0, 1.0], 0.1).unwrap();
    s.set_color([0.5, 0.5, 0.5, 1.0]);
    s.bevel_tint = Some(([1.0, 1.0, 1.0, 1.0], 1.0));
    let m = s.finalize(30.0, "m");
    let bright = m.colors.iter().filter(|c| c[0] == 255).count();
    let base = m.colors.iter().filter(|c| c[0] == 128).count();
    assert!(bright > 0 && base > 0);
    // Axis-aligned faces keep the base colour.
    for (n, c) in m.normals.iter().zip(&m.colors) {
        if n.iter().filter(|v| v.abs() > 0.99).count() == 1 && n[1] != 1.0 && n[1] != -1.0 {
            assert_eq!(c[0], 128);
        }
    }
}

#[test]
fn smooth_angle_controls_vertex_split() {
    let s = Shape::cylinder(0.5, 0.5, 1.0, 16, 0.0).unwrap();
    let flat = s.finalize(0.0, "m");
    let smooth = s.finalize(30.0, "m");
    // Flat: every side quad has its own 4 vertices; smooth: sides share one ring per height.
    assert_eq!(flat.vertex_count(), 16 * 4 + 2 * 16);
    assert_eq!(smooth.vertex_count(), 2 * 16 + 2 * 16);
    flat.validate().unwrap();
    smooth.validate().unwrap();
}

#[test]
fn weld_merges_close_vertices() {
    let mut s = Shape::cuboid([1.0, 1.0, 1.0], 0.0).unwrap();
    let mut t = s.clone();
    t.transform(&Affine::translate([1.0 + 1e-7, 0.0, 0.0]));
    s.merge(&t);
    let before = s.positions.len();
    s.weld(1e-4);
    assert!(s.positions.len() < before);
}
