//! Shape language (`specula_studio::shape`) held over families of shapes at every angle of a
//! quarter turn, plain, mirrored and shifted by part of a pixel, on the studio's 512 pixels
//! (docs/design/FORM.md, "Shape language"). The default run turns each shape every nine degrees;
//! `families_hold_their_claims_every_degree` (ignored) turns it every degree and holds the same
//! claims; `table` (ignored) writes every family's least and most to a file for tuning.
use specula_studio::image::Mask;
use specula_studio::shape::of_mask;
use std::f32::consts::{PI, TAU};

type F = Box<dyn Fn(f32, f32) -> bool + Sync + Send>;

fn rounded(x: f32, y: f32, hx: f32, hy: f32, r: f32) -> bool {
    let (dx, dy) = (x.abs() - (hx - r), y.abs() - (hy - r));
    dx <= r && dy <= r && (dx <= 0.0 || dy <= 0.0 || dx.hypot(dy) <= r)
}
fn within(p: &[[f32; 2]], x: f32, y: f32) -> bool {
    let n = p.len();
    let mut odd = false;
    for i in 0..n {
        let (a, b) = (p[i], p[(i + 1) % n]);
        if (a[1] > y) != (b[1] > y) && x < a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            odd = !odd;
        }
    }
    odd
}
fn capsule(x: f32, y: f32, a: [f32; 2], b: [f32; 2], r: f32) -> bool {
    let (abx, aby) = (b[0] - a[0], b[1] - a[1]);
    let t = (((x - a[0]) * abx + (y - a[1]) * aby) / (abx * abx + aby * aby)).clamp(0.0, 1.0);
    (x - a[0] - t * abx).hypot(y - a[1] - t * aby) < r
}
fn render(side: u32, f: &(dyn Fn(f32, f32) -> bool + Sync), deg: f32, mirror: bool, off: (f32, f32)) -> Mask {
    let (s, c) = (-deg).to_radians().sin_cos();
    let mut m = Mask::empty(side, side);
    for y in 0..side {
        for x in 0..side {
            let (px, py) = ((x as f32 + 0.5 + off.0) / side as f32 - 0.5, (y as f32 + 0.5 + off.1) / side as f32 - 0.5);
            let (mut lx, ly) = (px * c - py * s, px * s + py * c);
            if mirror {
                lx = -lx;
            }
            m.on[(y * side + x) as usize] = f(lx, ly);
        }
    }
    m
}
fn hash(i: i32) -> f32 {
    let mut h = (i as u32).wrapping_mul(2654435761) ^ 0x9e3779b9;
    h ^= h >> 15;
    h = h.wrapping_mul(2246822519);
    h ^= h >> 13;
    (h % 10000) as f32 / 10000.0 - 0.5
}
fn regpoly(n: usize, r: f32) -> Vec<[f32; 2]> {
    (0..n).map(|i| { let a = TAU * i as f32 / n as f32 - PI / 2.0; [r * a.cos(), r * a.sin()] }).collect()
}
fn star(n: usize, ro: f32, ri: f32) -> Vec<[f32; 2]> {
    (0..2 * n).map(|i| { let a = TAU * i as f32 / (2 * n) as f32 - PI / 2.0; let r = if i % 2 == 0 { ro } else { ri }; [r * a.cos(), r * a.sin()] }).collect()
}

fn families() -> Vec<(String, F)> {
    let mut v: Vec<(String, F)> = Vec::new();
    let mut p = |n: String, f: F| v.push((n, f));
    for (hx, hy) in [(0.3f32, 0.3f32), (0.4, 0.3), (0.4, 0.2), (0.4, 0.1), (0.4, 0.05)] {
        let short = 2.0 * hy;
        for fr in [0.0f32, 0.05, 0.1, 0.15, 0.2, 0.25, 0.33, 0.5] {
            let r = fr * short;
            p(format!("box {:.1}x{:.1} round {fr}", 2.0 * hx, short), Box::new(move |x, y| rounded(x, y, hx, hy, r.max(1e-6))));
        }
        for fr in [0.05f32, 0.1, 0.14, 0.2, 0.3] {
            let c = fr * short;
            p(format!("box {:.1}x{:.1} cut {fr}", 2.0 * hx, short), Box::new(move |x, y| x.abs() < hx && y.abs() < hy && x.abs() + y.abs() < hx + hy - c));
        }
    }
    for tooth in [6.0f32, 8.0, 10.0, 12.0, 16.0, 20.0, 24.0] {
        for len in [0.06f32, 0.25] {
            let per = 2.0 * tooth / 512.0;
            p(format!("comb teeth {tooth}px long {len}"), Box::new(move |x, y| x.abs() < 0.4 && ((0.0..0.12).contains(&y) || ((-len..0.0).contains(&y) && ((x + 0.4) / per).fract() < 0.5))));
        }
    }
    for long in [2.0f32, 3.0, 4.0, 5.0, 6.0, 8.0] {
        p(format!("oval {long}:1"), Box::new(move |x, y| (x / 0.45).hypot(y * long / 0.45) < 1.0));
    }
    for amp_px in [0.5f32, 1.0, 1.5, 2.0] {
        for freq in [90.0f32, 200.0] {
            let amp = amp_px / 512.0;
            p(format!("noisy box {amp_px}px f{freq}"), Box::new(move |x, y| { let a = (y.atan2(x) / TAU + 0.5) * freq; let i = a.floor(); let t = a - i; let n = hash(i as i32 + 77) * (1.0 - t) + hash(i as i32 + 78) * t; x.abs() < 0.30 + 2.0 * amp * n && y.abs() < 0.30 + 2.0 * amp * n }));
            p(format!("noisy disc {amp_px}px f{freq}"), Box::new(move |x, y| { let a = (y.atan2(x) / TAU + 0.5) * freq; let i = a.floor(); let t = a - i; let n = hash(i as i32) * (1.0 - t) + hash(i as i32 + 1) * t; x.hypot(y) < 0.40 + 2.0 * amp * n }));
            p(format!("noisy plank {amp_px}px f{freq}"), Box::new(move |x, y| { let a = (y.atan2(x) / TAU + 0.5) * freq; let i = a.floor(); let t = a - i; let n = hash(i as i32 + 7) * (1.0 - t) + hash(i as i32 + 8) * t; x.abs() < 0.40 + 2.0 * amp * n && y.abs() < 0.10 + 2.0 * amp * n }));
        }
    }
    let s5 = star(5, 0.36, 0.15);
    let s5b = s5.clone();
    let hex = regpoly(6, 0.42);
    p("disc".into(), Box::new(|x, y| x.hypot(y) < 0.44));
    p("hexagon".into(), Box::new(move |x, y| within(&hex, x, y)));
    p("triangle".into(), Box::new(|x, y| within(&[[0.0, -0.42], [0.36, 0.24], [-0.36, 0.24]], x, y)));
    p("star 5".into(), Box::new(move |x, y| within(&s5, x, y)));
    p("star 5 rounded tips".into(), Box::new(move |x, y| (0..5).any(|i| { let a = TAU * i as f32 / 5.0 - PI / 2.0; capsule(x, y, [0.0, 0.0], [0.32 * a.cos(), 0.32 * a.sin()], 0.07) }) || (within(&s5b, x, y) && x.hypot(y) < 0.2)));
    p("spike 15".into(), Box::new(|x, y| within(&[[0.0, -0.44], [0.1158, 0.44], [-0.1158, 0.44]], x, y)));
    p("crescent".into(), Box::new(|x, y| x.hypot(y) < 0.44 && (x - 0.18).hypot(y) > 0.36));
    p("mace".into(), Box::new(|x, y| x.hypot(y) < 0.26 || (0..8).any(|k| { let a = TAU * k as f32 / 8.0; let (c, s) = (a.cos(), a.sin()); let (u, v) = (x * c + y * s, -x * s + y * c); (0.2..0.45).contains(&u) && v.abs() < 0.05 * (0.45 - u) / 0.25 })));
    p("L".into(), Box::new(|x, y| within(&[[-0.32, -0.32], [-0.04, -0.32], [-0.04, 0.04], [0.32, 0.04], [0.32, 0.32], [-0.32, 0.32]], x, y)));
    p("cross".into(), Box::new(|x, y| (x.abs() < 0.44 && y.abs() < 0.12) || (x.abs() < 0.12 && y.abs() < 0.44)));
    p("house".into(), Box::new(|x, y| within(&[[-0.3, 0.4], [-0.3, -0.1], [0.0, -0.4], [0.3, -0.1], [0.3, 0.4]], x, y)));
    p("pentagon".into(), Box::new(|x, y| within(&regpoly(5, 0.45), x, y)));
    p("slot disc".into(), Box::new(|x, y| x.hypot(y) < 0.44 && !(x.abs() < 0.01 && y > 0.34)));
    p("notch disc".into(), Box::new(|x, y| x.hypot(y) < 0.44 && !(y > 0.24 && x.abs() < (y - 0.24) * 0.5)));
    p("cloud".into(), Box::new(|x, y| x.hypot(y) < 0.2 || (x - 0.22).hypot(y - 0.05) < 0.16 || (x + 0.22).hypot(y - 0.05) < 0.16 || (x - 0.1).hypot(y + 0.14) < 0.15 || (x + 0.1).hypot(y + 0.14) < 0.15));
    p("balloon".into(), Box::new(|x, y| x.hypot(y + 0.15) < 0.28 || (x.abs() < 0.004 && (0.1..0.45).contains(&y))));
    p("lollipop".into(), Box::new(|x, y| x.hypot(y + 0.18) < 0.25 || (x.abs() < 0.025 && (0.0..0.45).contains(&y))));
    p("snowman sticks".into(), Box::new(|x, y| x.hypot(y - 0.2) < 0.24 || x.hypot(y + 0.16) < 0.17 || (y.abs() < 0.008 && x.abs() < 0.45)));
    p("snowman thick arms".into(), Box::new(|x, y| x.hypot(y - 0.2) < 0.24 || x.hypot(y + 0.16) < 0.17 || capsule(x, y, [-0.40, -0.0], [0.40, 0.0], 0.03)));
    p("disc 4 whiskers".into(), Box::new(|x, y| x.hypot(y) < 0.3 || ((x.abs() < 0.004 || y.abs() < 0.004) && x.hypot(y) < 0.45)));
    p("table".into(), Box::new(|x, y| (y < -0.13 && y > -0.21 && x.abs() < 0.33) || ((x.abs() - 0.28).abs() < 0.024 && (-0.21..0.3).contains(&y))));
    p("box+spike".into(), Box::new(|x, y| (x.abs() < 0.3 && y.abs() < 0.2) || within(&[[-0.05, -0.2], [0.05, -0.2], [0.0, -0.44]], x, y)));
    p("box+thick limb".into(), Box::new(|x, y| (x.abs() < 0.2 && y.abs() < 0.2) || (x.abs() < 0.08 && (0.0..0.44).contains(&y))));
    p("character".into(), Box::new(|x, y| x.hypot(y + 0.32) < 0.09 || (x.abs() < 0.12 && (-0.22..0.08).contains(&y)) || capsule(x, y, [-0.14, -0.19], [-0.24, 0.06], 0.035) || capsule(x, y, [0.14, -0.19], [0.24, 0.06], 0.035) || capsule(x, y, [-0.06, 0.08], [-0.08, 0.40], 0.045) || capsule(x, y, [0.06, 0.08], [0.08, 0.40], 0.045)));
    p("char arms down".into(), Box::new(|x, y| x.hypot(y + 0.32) < 0.09 || (x.abs() < 0.12 && (-0.22..0.08).contains(&y)) || capsule(x, y, [-0.16, -0.18], [-0.17, 0.06], 0.035) || capsule(x, y, [0.16, -0.18], [0.17, 0.06], 0.035) || capsule(x, y, [-0.06, 0.08], [-0.06, 0.40], 0.045) || capsule(x, y, [0.06, 0.08], [0.06, 0.40], 0.045)));
    p("char T-pose".into(), Box::new(|x, y| x.hypot(y + 0.32) < 0.09 || (x.abs() < 0.12 && (-0.22..0.08).contains(&y)) || capsule(x, y, [-0.40, -0.18], [0.40, -0.18], 0.035) || capsule(x, y, [-0.06, 0.08], [-0.06, 0.40], 0.045) || capsule(x, y, [0.06, 0.08], [0.06, 0.40], 0.045)));
    p("char round body".into(), Box::new(|x, y| x.hypot(y + 0.32) < 0.09 || capsule(x, y, [0.0, -0.12], [0.0, 0.0], 0.12) || capsule(x, y, [-0.14, -0.15], [-0.24, 0.06], 0.035) || capsule(x, y, [0.14, -0.15], [0.24, 0.06], 0.035) || capsule(x, y, [-0.06, 0.08], [-0.08, 0.40], 0.045) || capsule(x, y, [0.06, 0.08], [0.08, 0.40], 0.045)));
    p("sheep".into(), Box::new(|x, y| (x / 0.30).hypot((y + 0.02) / 0.17) < 1.0 || (x + 0.32).hypot(y + 0.12) < 0.09 || [-0.2f32, -0.08, 0.08, 0.2].iter().any(|lx| capsule(x, y, [*lx, 0.05], [*lx, 0.32], 0.03))));
    p("bottle square shoulder".into(), Box::new(|x, y| rounded(x, y - 0.12, 0.15, 0.30, 0.04) || (x.abs() < 0.045 && (-0.44..-0.1).contains(&y))));
    p("bottle curved shoulder".into(), Box::new(|x, y| (rounded(x, y - 0.12, 0.15, 0.30, 0.04) && !(y < 0.0 && x.abs() > 0.045 + 0.105 * ((y + 0.18) / 0.18).clamp(0.0, 1.0).sqrt())) || (x.abs() < 0.045 && (-0.44..-0.1).contains(&y))));
    p("bottle long neck".into(), Box::new(|x, y| (rounded(x, y - 0.17, 0.13, 0.25, 0.04) && !(y < 0.05 && x.abs() > 0.035 + 0.095 * ((y + 0.08) / 0.13).clamp(0.0, 1.0).sqrt())) || (x.abs() < 0.035 && (-0.44..0.0).contains(&y))));
    p("flask".into(), Box::new(|x, y| x.hypot(y - 0.12) < 0.28 || (x.abs() < 0.07 && (-0.40..-0.1).contains(&y)) || (x.abs() < 0.10 && (-0.42..-0.36).contains(&y))));
    p("vase".into(), Box::new(|x, y| y.abs() < 0.42 && x.abs() < 0.08 + 0.18 * (1.0 - ((y - 0.1) / 0.32).powi(2)).max(0.0).sqrt().max(if y < -0.25 { 0.1 } else { 0.0 })));
    p("axe".into(), Box::new(|x, y| (x.abs() < 0.025 && y.abs() < 0.44) || (x > 0.0 && x.hypot(y + 0.22) < 0.26 && (x + 0.2).hypot(y + 0.22) > 0.30)));
    p("sword".into(), Box::new(|x, y| within(&[[-0.03, -0.26], [0.03, -0.26], [0.03, 0.36], [0.0, 0.44], [-0.03, 0.36]], x, y) || (y > -0.30 && y < -0.26 && x.abs() < 0.14) || (x.abs() < 0.018 && (-0.42..-0.30).contains(&y)) || x.hypot(y + 0.42) < 0.03));
    p("dagger".into(), Box::new(|x, y| within(&[[0.0, -0.44], [0.05, -0.05], [-0.05, -0.05]], x, y) || (x.abs() < 0.12 && (-0.06..0.0).contains(&y)) || (x.abs() < 0.025 && (0.0..0.3).contains(&y))));
    p("hammer".into(), Box::new(|x, y| (x.abs() < 0.03 && (-0.2..0.44).contains(&y)) || (x.abs() < 0.2 && (-0.36..-0.2).contains(&y))));
    p("key".into(), Box::new(|x, y| (0.06..0.14).contains(&(x + 0.3).hypot(y)) || (x > -0.17 && x < 0.40 && y.abs() < 0.025) || (x > 0.30 && x < 0.34 && y > 0.0 && y < 0.10) || (x > 0.22 && x < 0.26 && y > 0.0 && y < 0.08)));
    p("mushroom".into(), Box::new(|x, y| (x.hypot(y) < 0.40 && y < 0.0) || (x.abs() < 0.09 && (0.0..0.36).contains(&y))));
    p("slime".into(), Box::new(|x, y| y < 0.25 && (x / 0.42).hypot((y - 0.25) / 0.45) < 1.0));
    p("ghost".into(), Box::new(|x, y| (x.hypot(y + 0.1) < 0.3 && y < -0.1) || (x.abs() < 0.3 && y >= -0.1 && y < 0.30 + 0.06 * (x * 30.0).sin())));
    p("lobed disc 10".into(), Box::new(|x, y| { let a = y.atan2(x); x.hypot(y) < 0.36 + 0.06 * (10.0 * a).sin() }));
    p("tombstone".into(), Box::new(|x, y| (x.abs() < 0.26 && (-0.10..0.40).contains(&y)) || x.hypot(y + 0.10) < 0.26));
    p("heater shield".into(), Box::new(|x, y| y > -0.36 && x.abs() < 0.30 && (y < 0.0 || ((x.abs() + 0.15).hypot(y) < 0.45 && y < 0.42))));
    p("heart".into(), Box::new(|x, y| (x - 0.16).hypot(y + 0.12) < 0.2 || (x + 0.16).hypot(y + 0.12) < 0.2 || within(&[[-0.33, -0.02], [0.33, -0.02], [0.0, 0.4]], x, y)));
    p("barrel".into(), Box::new(|x, y| y.abs() < 0.38 && x.abs() < 0.22 + 0.06 * (1.0 - (y / 0.38).powi(2))));
    p("tree ball on trunk".into(), Box::new(|x, y| x.hypot(y + 0.12) < 0.30 || (x.abs() < 0.045 && (0.0..0.44).contains(&y))));
    p("pine".into(), Box::new(|x, y| within(&[[0.0,-0.44],[0.16,-0.18],[-0.16,-0.18]],x,y) || within(&[[0.0,-0.30],[0.22,0.02],[-0.22,0.02]],x,y) || within(&[[0.0,-0.12],[0.28,0.26],[-0.28,0.26]],x,y) || (x.abs()<0.04 && (0.2..0.42).contains(&y))));
    p("gear square teeth".into(), Box::new(|x, y| { let r = x.hypot(y); let a = y.atan2(x); r < 0.32 || (r < 0.42 && ((a / TAU * 10.0).rem_euclid(1.0) < 0.5)) }));
    p("apple stem .05 6px".into(), Box::new(|x, y| x.hypot(y - 0.04) < 0.38 || (x.abs() < 6.0 / 1024.0 && y < 0.0 && y > -0.39)));
    p("disc whisker 30px".into(), Box::new(|x, y| x.hypot(y) < 0.39 || (x < -0.38 && x > -0.39 - 30.0 / 512.0 && y.abs() < 1.0 / 1024.0)));
    p("fence".into(), Box::new(|x, y| (x.abs() < 0.44 && ((y + 0.1).abs() < 0.02 || (y - 0.15).abs() < 0.02)) || ((0..5).any(|k| (x - (-0.36 + 0.18 * k as f32)).abs() < 0.03) && y > -0.30 && y < 0.30) || (0..5).any(|k| within(&[[-0.36 + 0.18 * k as f32 - 0.03, -0.30], [-0.36 + 0.18 * k as f32 + 0.03, -0.30], [-0.36 + 0.18 * k as f32, -0.36]], x, y))));
    drop(p);
    v.extend(reviewed());
    v
}

fn hash2(i: i32, seed: u32) -> f32 {
    let mut h = (i as u32).wrapping_mul(2654435761) ^ seed.wrapping_mul(0x9e3779b9);
    h ^= h >> 15;
    h = h.wrapping_mul(2246822519);
    h ^= h >> 13;
    (h % 10000) as f32 / 10000.0 - 0.5
}

/// The shapes of the fourth and fifth reviews' probes that were not among the families, as
/// they drew them (every review's probe shape is a permanent test: its sixth version's brief).
fn reviewed() -> Vec<(String, F)> {
    let mut v: Vec<(String, F)> = Vec::new();
    let mut p = |n: String, f: F| v.push((n, f));
    // The fifth review: figures, tapers, smooth outlines, holes, lobes, crescents, gears.
    for th in [6.0f32, 8.0, 10.0, 12.0, 16.0, 24.0] {
        let r = th / 2.0 / 512.0;
        p(format!("fig diag arms {th}px"), Box::new(move |x, y| x.hypot(y + 0.32) < 0.09 || rounded(x, y + 0.07, 0.12, 0.15, 0.01) || capsule(x, y, [-0.1, -0.2], [-0.34, 0.02], r) || capsule(x, y, [0.1, -0.2], [0.34, 0.02], r) || capsule(x, y, [-0.06, 0.07], [-0.14, 0.42], 0.04) || capsule(x, y, [0.06, 0.07], [0.14, 0.42], 0.04)));
    }
    p("fig tapered limbs".into(), Box::new(|x, y| x.hypot(y + 0.32) < 0.09 || rounded(x, y + 0.07, 0.12, 0.15, 0.01) || cone(x, y, [-0.1, -0.2], [-0.34, 0.02], 0.04, 0.008) || cone(x, y, [0.1, -0.2], [0.34, 0.02], 0.04, 0.008) || cone(x, y, [-0.06, 0.07], [-0.14, 0.42], 0.06, 0.015) || cone(x, y, [0.06, 0.07], [0.14, 0.42], 0.06, 0.015)));
    p("dog tapered legs".into(), Box::new(|x, y| ell(x, y, 0.0, -0.05, 0.28, 0.12) || (x + 0.32).hypot(y + 0.18) < 0.09 || [-0.2f32, 0.2].iter().any(|lx| cone(x, y, [*lx, 0.0], [*lx + 0.03, 0.36], 0.05, 0.015)) || cone(x, y, [0.26, -0.08], [0.42, -0.25], 0.03, 0.008)));
    p("fig box limbs".into(), Box::new(|x, y| rounded(x, y + 0.32, 0.08, 0.08, 0.005) || rounded(x, y + 0.07, 0.12, 0.15, 0.005) || ((x.abs() - 0.18).abs() < 0.03 && (-0.2..0.1).contains(&y)) || ((x.abs() - 0.06).abs() < 0.035 && (0.08..0.42).contains(&y))));
    p("pear".into(), Box::new(|x, y| x.hypot(y - 0.15) < 0.27 || x.hypot(y + 0.17) < 0.16 || (y > -0.17 && y < 0.15 && x.abs() < 0.16 + 0.11 * ((y + 0.17) / 0.32).powi(2))));
    p("bowling pin".into(), Box::new(|x, y| { let t = (y + 0.44) / 0.88; if !(0.0..=1.0).contains(&t) { return false; } let w = 0.06 + 0.10 * (-((t - 0.12) / 0.09).powi(2)).exp() + 0.17 * (-((t - 0.68) / 0.2).powi(2)).exp(); x.abs() < w && !(t > 0.97 && x.abs() > 0.08) }));
    p("teardrop smooth".into(), Box::new(|x, y| { let t = (y + 0.44) / 0.86; (0.0..=1.0).contains(&t) && x.abs() < 0.6 * (t * (1.0 - t)).sqrt() * t.powf(0.3) }));
    p("vase smooth".into(), Box::new(|x, y| { let t = (y + 0.42) / 0.84; (0.0..=1.0).contains(&t) && x.abs() < 0.08 + 0.16 * (PI * t).sin().powi(2) + 0.06 * (1.0 - t).powi(6) }));
    p("ring".into(), Box::new(|x, y| (0.25..0.43).contains(&x.hypot(y))));
    p("box with hole".into(), Box::new(|x, y| x.abs() < 0.32 && y.abs() < 0.32 && !(x.abs() < 0.15 && y.abs() < 0.15)));
    p("trefoil".into(), Box::new(|x, y| (0..3).any(|k| { let a = TAU * k as f32 / 3.0 - PI / 2.0; (x - 0.17 * a.cos()).hypot(y - 0.17 * a.sin()) < 0.25 })));
    p("6 lobes sharp cusps".into(), Box::new(|x, y| (0..6).any(|k| { let a = TAU * k as f32 / 6.0; (x - 0.3 * a.cos()).hypot(y - 0.3 * a.sin()) < 0.15 }) || x.hypot(y) < 0.25));
    for (name, off, r2) in [("crescent thin", 0.10f32, 0.40f32), ("crescent mid", 0.18, 0.36), ("crescent fat", 0.25, 0.30)] {
        p(name.into(), Box::new(move |x, y| x.hypot(y) < 0.44 && (x - off).hypot(y) > r2));
    }
    for teeth in [8usize, 12, 24] {
        p(format!("gear tri teeth {teeth}"), Box::new(move |x, y| { let r = x.hypot(y); let a = (y.atan2(x) / TAU * teeth as f32).rem_euclid(1.0); r < 0.33 + 0.1 * (1.0 - (2.0 * a - 1.0).abs()) }));
        p(format!("gear sq teeth {teeth}"), Box::new(move |x, y| { let r = x.hypot(y); let a = (y.atan2(x) / TAU * teeth as f32).rem_euclid(1.0); r < 0.34 || (r < 0.43 && a < 0.5) }));
    }
    p("L thin".into(), Box::new(|x, y| ((-0.3..-0.24).contains(&x) && y.abs() < 0.4) || ((-0.3..0.3).contains(&x) && (0.34..0.4).contains(&y))));
    p("T".into(), Box::new(|x, y| (x.abs() < 0.4 && (-0.4..-0.28).contains(&y)) || (x.abs() < 0.06 && (-0.4..0.4).contains(&y))));
    p("plus thin".into(), Box::new(|x, y| (x.abs() < 0.43 && y.abs() < 0.03) || (x.abs() < 0.03 && y.abs() < 0.43)));
    p("plus thick".into(), Box::new(|x, y| (x.abs() < 0.43 && y.abs() < 0.15) || (x.abs() < 0.15 && y.abs() < 0.43)));
    for (n, inner) in [(3usize, 0.18f32), (4, 0.18), (6, 0.25), (8, 0.3), (16, 0.33), (16, 0.40), (24, 0.40)] {
        let s = star(n, 0.45, inner);
        p(format!("star {n} inner {inner}"), Box::new(move |x, y| within(&s, x, y)));
    }
    for n in [3usize, 4, 5, 7, 9] {
        let s = regpoly(n, 0.44);
        p(format!("polygon {n}"), Box::new(move |x, y| within(&s, x, y)));
    }
    for (hx, hy) in [(0.4f32, 0.2f32), (0.25, 0.4)] {
        for seed in [3u32, 11] {
            for amp_px in [1.0f32, 2.0] {
                let amp = amp_px / 512.0;
                p(format!("noisy box {:.1}x{:.1} {amp_px}px s{seed}", 2.0 * hx, 2.0 * hy), Box::new(move |x, y| { let a = (y.atan2(x) / TAU + 0.5) * 140.0; let i = a.floor(); let t = a - i; let n = hash2(i as i32, seed) * (1.0 - t) + hash2(i as i32 + 1, seed) * t; x.abs() < hx + 2.0 * amp * n && y.abs() < hy + 2.0 * amp * n }));
            }
        }
    }
    p("box edge pixel noise".into(), Box::new(|x, y| { let a = (y.atan2(x) / TAU + 0.5) * 1400.0; let n = hash2(a as i32, 5); x.abs() < 0.3 + n / 512.0 && y.abs() < 0.25 + n / 512.0 }));
    for tooth in [10.0f32, 12.0, 14.0, 16.0] {
        let per = 2.0 * tooth / 512.0;
        p(format!("comb {tooth}px long .12"), Box::new(move |x, y| x.abs() < 0.4 && ((0.0..0.12).contains(&y) || ((-0.12..0.0).contains(&y) && ((x + 0.4) / per).fract() < 0.5))));
        p(format!("rake {tooth}px tines .3"), Box::new(move |x, y| x.abs() < 0.4 && ((0.0..0.05).contains(&y) || ((-0.30..0.0).contains(&y) && ((x + 0.4) / per).fract() < 0.5))));
    }
    // The fifth review's stalks of every width by length (an apple is a disc of 0.38).
    for th in [6.0f32, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0] {
        for len in [0.02f32, 0.03, 0.04, 0.06, 0.09, 0.1] {
            let hw = th / 1024.0;
            p(format!("stalk {th}px len {len}"), Box::new(move |x, y| x.hypot(y - 0.04) < 0.38 || (x.abs() < hw && y < 0.0 && y > -0.34 - len)));
        }
    }
    for th in [7.0f32, 8.0, 9.0] {
        let r = th / 2.0 / 512.0;
        p(format!("snowman diag capsule arms {th}px"), Box::new(move |x, y| x.hypot(y - 0.2) < 0.24 || x.hypot(y + 0.16) < 0.17 || capsule(x, y, [-0.12, -0.12], [-0.40, -0.30], r) || capsule(x, y, [0.12, -0.12], [0.40, -0.30], r)));
        p(format!("snowman horiz capsule arms {th}px"), Box::new(move |x, y| x.hypot(y - 0.2) < 0.24 || x.hypot(y + 0.16) < 0.17 || capsule(x, y, [-0.44, -0.16], [0.44, -0.16], r)));
    }
    // Lenses: two arcs meeting at tips of the angle named.
    for tip in [50.0f32, 70.0, 90.0, 110.0, 130.0] {
        let c = 0.42f32;
        let d = c / (tip / 2.0).to_radians().tan();
        let r = (d * d + c * c).sqrt();
        p(format!("lens tip {tip}"), Box::new(move |x, y| (x + d).hypot(y) < r && (x - d).hypot(y) < r));
    }
    for ratio in [3.0f32, 3.3, 4.0] {
        let ry = 0.40 / ratio;
        p(format!("ellipse alone {ratio}:1"), Box::new(move |x, y| (x / 0.4).hypot((y - 0.08) / ry) < 1.0));
        p(format!("cumulus base {ratio}:1"), Box::new(move |x, y| (x / 0.4).hypot((y - 0.08) / ry) < 1.0 || (x + 0.12).hypot(y + 0.02) < 0.16 || (x - 0.12).hypot(y + 0.06) < 0.19));
    }
    p("cloud flat bottom".into(), Box::new(|x, y| y < 0.15 && ((x + 0.2).hypot(y - 0.05) < 0.17 || (x - 0.2).hypot(y - 0.05) < 0.17 || x.hypot(y + 0.05) < 0.22)));
    p("sword square guard no pommel".into(), Box::new(|x, y| within(&[[-0.035, -0.10], [0.035, -0.10], [0.035, 0.34], [0.0, 0.44], [-0.035, 0.34]], x, y) || (x.abs() < 0.15 && (y + 0.12).abs() < 0.02) || (x.abs() < 0.02 && (-0.40..-0.10).contains(&y))));
    p("pointed arch window".into(), Box::new(|x, y| (x.abs() < 0.2 && (0.0..0.42).contains(&y)) || (y < 0.0 && (x + 0.2).hypot(y) < 0.4 && (x - 0.2).hypot(y) < 0.4)));
    for tip in [0.06f32, 0.045, 0.03, 0.02] {
        p(format!("lone club tip {tip}"), Box::new(move |x, y| cone(x, y, [0.0, 0.36], [0.0, -0.36], 0.06, tip)));
        p(format!("disc + club arm tip {tip}"), Box::new(move |x, y| x.hypot(y - 0.14) < 0.28 || cone(x, y, [0.0, 0.0], [0.0, -0.40], 0.06, tip)));
    }
    p("disc + square stub 60px".into(), Box::new(|x, y| x.hypot(y - 0.08) < 0.34 || (x.abs() < 30.0 / 512.0 && (-0.42..0.0).contains(&y))));
    // The fourth review: rims that wobble further, whiskers by width and count, tables' legs.
    for amp_px in [3.0f32, 4.0, 6.0] {
        for freq in [40.0f32, 90.0, 200.0] {
            let amp = amp_px / 512.0;
            p(format!("noisy disc {amp_px}px f{freq}"), Box::new(move |x, y| { let a = (y.atan2(x) / TAU + 0.5) * freq; let i = a.floor(); let t = a - i; let n = hash(i as i32) * (1.0 - t) + hash(i as i32 + 1) * t; x.hypot(y) < 0.40 + 2.0 * amp * n }));
            p(format!("noisy box {amp_px}px f{freq}"), Box::new(move |x, y| { let a = (y.atan2(x) / TAU + 0.5) * freq; let i = a.floor(); let t = a - i; let n = hash(i as i32 + 77) * (1.0 - t) + hash(i as i32 + 78) * t; x.abs() < 0.30 + 2.0 * amp * n && y.abs() < 0.30 + 2.0 * amp * n }));
        }
    }
    for wpx in [1.0f32, 2.0, 3.0, 4.0, 6.0, 8.0] {
        let hw = wpx / 512.0 / 2.0;
        for count in [1usize, 2, 4] {
            p(format!("disc + {count} whiskers {wpx}px"), Box::new(move |x, y| x.hypot(y) < 0.36 || ((0..count).any(|k| { let a = PI * k as f32 / count as f32 * if count == 1 { 1.0 } else { 2.0 }; let (u, v) = (x * a.cos() + y * a.sin(), -x * a.sin() + y * a.cos()); v.abs() < hw && u > 0.0 && u < 0.47 }))));
        }
    }
    for leg_px in [8.0f32, 12.0, 16.0, 20.0, 25.0] {
        let hw = leg_px / 1024.0;
        p(format!("table legs {leg_px}px"), Box::new(move |x, y| (y < -0.13 && y > -0.21 && x.abs() < 0.33) || ((x.abs() - 0.28).abs() < hw && (-0.21..0.3).contains(&y))));
    }
    drop(p);
    v
}

/// A family's shares over the masks of a sweep: the least, the most and the mean of each, how
/// many masks each kind led in, and how many masks had no shape.
#[derive(Clone, Copy, Debug)]
struct Sweep {
    lo: [f32; 3],
    hi: [f32; 3],
    mean: [f32; 3],
    lead: [usize; 3],
    none: usize,
}

impl Sweep {
    fn swing(&self, k: usize) -> f32 {
        self.hi[k] - self.lo[k]
    }
    fn leads(&self, k: usize) -> bool {
        self.lead[k] == self.lead.iter().sum::<usize>() && self.none == 0
    }
}

const R: usize = 0;
const S: usize = 1;
const P: usize = 2;

/// Every `step` degrees over a quarter turn, plain, mirrored, and shifted by (0.37, 0.71) of a
/// pixel each way.
fn sweep(f: &(dyn Fn(f32, f32) -> bool + Sync), step: u32) -> Sweep {
    let (mut lo, mut hi, mut sum, mut lead, mut none, mut n) = ([1.0f32; 3], [0.0f32; 3], [0.0f32; 3], [0usize; 3], 0usize, 0usize);
    for deg in (0..90).step_by(step as usize) {
        for mirror in [false, true] {
            for off in [(0.0, 0.0), (0.37, 0.71)] {
                n += 1;
                let Some(a) = of_mask(&render(512, f, deg as f32, mirror, off)) else {
                    none += 1;
                    continue;
                };
                let sh = [a.round, a.square, a.pointed];
                assert!((sh.iter().sum::<f32>() - 1.0).abs() < 1e-3 && sh.iter().all(|v| (0.0..=1.0).contains(v)), "shares {sh:?}");
                for k in 0..3 {
                    lo[k] = lo[k].min(sh[k]);
                    hi[k] = hi[k].max(sh[k]);
                    sum[k] += sh[k];
                }
                lead[a.leading() as usize] += 1;
            }
        }
    }
    let m = (n - none).max(1) as f32;
    Sweep { lo, hi, mean: [sum[0] / m, sum[1] / m, sum[2] / m], lead, none }
}

/// Every family swept, on as many threads as the machine offers (up to twelve).
fn swept(step: u32, only: &(dyn Fn(&str) -> bool + Sync)) -> Vec<(String, Sweep)> {
    let fam: Vec<(String, F)> = families().into_iter().filter(|(n, _)| only(n)).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let out = std::sync::Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(12);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some((name, f)) = fam.get(i) else { break };
                let r = sweep(&**f, step);
                out.lock().unwrap().push((i, name.clone(), r));
            });
        }
    });
    let mut out = out.into_inner().unwrap();
    out.sort_by_key(|r| r.0);
    out.into_iter().map(|(_, n, r)| (n, r)).collect()
}

/// What the measure guarantees of each family, at every angle swept. Each bound is what the
/// fifth version measured at every degree (FORM.md has the table), less a margin of a few
/// hundredths; the swing limits are the measured swing and about a twentieth more.
fn hold(all: &[(String, Sweep)]) {
    let get = |name: &str| all.iter().find(|(n, _)| n == name).map(|(_, s)| *s).unwrap_or_else(|| panic!("no family {name}"));
    let mut failed: Vec<String> = Vec::new();
    let mut check = |name: &str, ok: bool, what: &str| {
        if !ok {
            failed.push(format!("{name}: {what}: {:?}", get(name)));
        }
    };
    let k = |kind: &str| ["round", "square", "pointed"].iter().position(|n| *n == kind).unwrap();
    // Boxes of five proportions, their corners rounded or cut by a share of the shorter side
    // (its fourth review: only the 0.8 by 0.6 box was tested, and a plank swung by a half).
    // A thinner part has fewer pixels across its corner, and swings further in the bands.
    for (shape, steady) in [("0.6x0.6", 0.25f32), ("0.8x0.6", 0.25), ("0.8x0.4", 0.3), ("0.8x0.2", 0.45), ("0.8x0.1", 0.72)] {
        let thin = steady > 0.3;
        let mut last = -1.0f32;
        for fr in ["0", "0.05", "0.1", "0.15", "0.2", "0.25", "0.33", "0.5"] {
            let name = format!("box {shape} round {fr}");
            let s = get(&name);
            let f: f32 = fr.parse().unwrap();
            check(&name, s.hi[P] < 0.02, "nothing of a rounded box is a point");
            check(&name, s.swing(R) <= steady, "round moves no more than the limit for its proportion as it turns");
            if f <= 0.05 || (f <= 0.1 && steady < 0.7) {
                check(&name, s.leads(S) && s.lo[S] >= 0.95, "square to a tenth of the shorter side");
            }
            if f >= 0.33 {
                check(&name, s.leads(R) && s.lo[R] >= if thin { 0.75 } else { 0.95 }, "round from a third of the shorter side");
            }
            let middle = (s.lo[R] + s.hi[R]) / 2.0;
            check(&name, middle >= last - 0.02, "the more rounded no less round");
            last = middle;
        }
        let mut last = 2.0f32;
        for fr in ["0.05", "0.1", "0.14", "0.2", "0.3"] {
            let name = format!("box {shape} cut {fr}");
            let s = get(&name);
            let f: f32 = fr.parse().unwrap();
            check(&name, s.hi[P] < 0.02, "nothing of a cut box is a point");
            check(&name, s.swing(S) <= steady, "square moves no more than the limit for its proportion as it turns");
            if f <= 0.05 || (f <= 0.14 && !thin) || (f <= 0.1 && steady < 0.5) {
                check(&name, s.leads(S) && s.lo[S] >= 0.75, "a corner cut off flat is a corner until the cut is a seventh of the side");
            }
            if f >= 0.3 {
                check(&name, s.leads(R) && s.lo[R] >= 0.95, "cut by a third, eight-sided and round");
            }
            let middle = (s.lo[S] + s.hi[S]) / 2.0;
            check(&name, middle <= last + 0.05, "a larger cut no squarer");
            last = middle;
        }
    }
    // A box, a disc and a plank whose edges wobble by half a pixel to two (its fourth review:
    // a box turned 36 degrees read 0% square).
    for amp in ["0.5", "1", "1.5", "2"] {
        for f in ["90", "200"] {
            let name = format!("noisy box {amp}px f{f}");
            check(&name, get(&name).lo[S] >= 0.9, "a box with a wobbling edge is square");
            let name = format!("noisy disc {amp}px f{f}");
            check(&name, get(&name).lo[R] >= 0.95, "a disc with a wobbling edge is round");
            let name = format!("noisy plank {amp}px f{f}");
            check(&name, get(&name).leads(S) && get(&name).lo[S] >= 0.55 && get(&name).hi[P] < 0.2, "a plank with a wobbling edge leads square");
        }
    }
    // Combs: teeth of twenty pixels and more are square, of twelve they lead square, and under
    // twelve (a part under six pixels thick) they are read as points: a limit, held here so
    // that a change to it is seen. (Its fourth review: teeth of 6 to 16 pixels read as points.)
    for len in ["0.06", "0.25"] {
        for t in ["20", "24"] {
            let name = format!("comb teeth {t}px long {len}");
            check(&name, get(&name).leads(S) && get(&name).lo[S] >= 0.75, "teeth of twenty pixels or more are square");
        }
        let name = format!("comb teeth 12px long {len}");
        check(&name, get(&name).leads(S), "teeth of twelve pixels lead square");
        for t in ["6", "8"] {
            let name = format!("comb teeth {t}px long {len}");
            check(&name, get(&name).leads(P), "teeth under twelve pixels read as points (a limit)");
        }
    }
    // Ovals: round to three to one, pointed from five, never much square.
    for (long, lead) in [("2", Some("round")), ("3", Some("round")), ("4", None), ("5", Some("pointed")), ("6", Some("pointed")), ("8", Some("pointed"))] {
        let name = format!("oval {long}:1");
        check(&name, get(&name).hi[S] < 0.3, "an oval is never more than a quarter square");
        if let Some(kind) = lead {
            check(&name, get(&name).leads(k(kind)), "leads as it should");
        }
    }
    // Shapes that lead one kind at every angle, by at least so much (0: steady only).
    let leads: [(&str, &str, f32); 48] = [
        ("disc", "round", 0.97),
        ("hexagon", "round", 0.55),
        ("triangle", "pointed", 0.95),
        ("star 5", "pointed", 0.95),
        ("star 5 rounded tips", "round", 0.9),
        ("spike 15", "pointed", 0.55),
        ("crescent", "pointed", 0.65),
        ("mace", "pointed", 0.9),
        ("L", "square", 0.95),
        ("cross", "square", 0.95),
        ("house", "square", 0.7),
        ("pentagon", "square", 0.8),
        ("slot disc", "round", 0.9),
        ("notch disc", "round", 0.55),
        ("cloud", "round", 0.95),
        ("balloon", "round", 0.95),
        ("lollipop", "round", 0.9),
        ("snowman sticks", "round", 0.9),
        ("snowman thick arms", "round", 0.95),
        ("disc 4 whiskers", "round", 0.85),
        // (Its fourth review: a stalk shorter than the rim's window took a quarter of a disc.)
        ("apple stem .05 6px", "round", 0.95),
        ("disc whisker 30px", "round", 0.95),
        ("table", "square", 0.95),
        ("box+spike", "square", 0.75),
        ("box+thick limb", "square", 0.95),
        // (Its fourth review: a character with capsule limbs was 22% to 67% round and led
        // pointed in 102 masks of 360; a bottle with a curved shoulder led round in 282 and
        // square in 76.)
        ("character", "round", 0.6),
        ("char T-pose", "round", 0.95),
        ("char round body", "round", 0.95),
        ("sheep", "round", 0.95),
        ("bottle square shoulder", "square", 0.85),
        ("bottle curved shoulder", "square", 0.58),
        ("bottle long neck", "square", 0.45),
        ("flask", "round", 0.75),
        ("vase", "round", 0.75),
        ("hammer", "square", 0.95),
        ("barrel", "square", 0.75),
        ("tree ball on trunk", "round", 0.85),
        ("pine", "pointed", 0.9),
        ("gear square teeth", "square", 0.85),
        ("heart", "round", 0.65),
        // (Its fourth review: wholly pointed.)
        ("lobed disc 10", "round", 0.95),
        ("heater shield", "square", 0.52),
        ("fence", "square", 0.42),
        ("mushroom", "square", 0.42),
        ("slime", "square", 0.4),
        // (These three lead their kind in most masks and not in all.)
        ("sword", "pointed", 0.0),
        ("key", "round", 0.0),
        ("ghost", "pointed", 0.0),
    ];
    for (name, kind, least) in leads {
        let s = get(name);
        if least > 0.0 {
            check(name, s.leads(k(kind)) && s.lo[k(kind)] >= least, &format!("leads {kind}, at least {least}"));
        }
        check(name, (0..3).all(|j| s.swing(j) <= 0.25), "no share moves by more than a quarter as it turns");
    }
    // Between two kinds, and steady there (which leads says nothing: they are near half and half).
    for name in ["char arms down", "tombstone", "dagger", "axe"] {
        check(name, (0..3).all(|j| get(name).swing(j) <= 0.25), "no share moves by more than a quarter as it turns");
    }
    assert!(failed.is_empty(), "{} claims failed:\n{}", failed.len(), failed.join("\n"));
}

#[test]
fn families_hold_their_claims_every_nine_degrees() {
    hold(&swept(9, &|_| true));
}

#[test]
#[ignore = "every degree of a quarter turn: minutes, in release (cargo test --release -p specula-studio --test shape_families -- --ignored)"]
fn families_hold_their_claims_every_degree() {
    hold(&swept(1, &|_| true));
}

/// **What the check's advice says to do leads to a pass** (its fourth review: the round advice
/// was half what the measure asked for; its third: the square advice led to round). Each advice
/// text of `taste.rs`, done to boxes of three proportions, passes the check at the default share
/// (0.5) in every mask.
#[test]
fn the_checks_advice_leads_to_a_pass() {
    for (hx, hy) in [(0.4f32, 0.3f32), (0.4, 0.2), (0.4, 0.1)] {
        let width = 2.0 * hy;
        // Round: corners rounded by a third of the part's width or more.
        let round = move |x: f32, y: f32| rounded(x, y, hx, hy, width / 3.0);
        // Square: rounded corners under a tenth of the part's width; corners cut off flat under
        // a twentieth of it.
        let square = move |x: f32, y: f32| rounded(x, y, hx, hy, 0.099 * width);
        let cut = move |x: f32, y: f32| x.abs() < hx && y.abs() < hy && x.abs() + y.abs() < hx + hy - 0.049 * width;
        for (what, f, kind) in [("round", &round as &(dyn Fn(f32, f32) -> bool + Sync), R), ("square (rounded)", &square, S), ("square (cut)", &cut, S)] {
            let s = sweep(f, 9);
            assert!(s.lo[kind] >= 0.5, "a {:.1} by {:.1} box made {what} by the advice: {s:?}", 2.0 * hx, width);
        }
    }
    // Pointed: tips under 65 degrees, the foot thick.
    let spike = |x: f32, y: f32| within(&[[0.0, -0.44], [0.24, 0.44], [-0.24, 0.44]], x, y);
    let s = sweep(&spike, 9);
    assert!(s.lo[P] >= 0.5, "a spike of 30 degrees: {s:?}");
}

/// (For whoever tunes the measure: every family's shares written to
/// `specula-shape-families.txt` in the temporary folder, or `OUT`, every `STEP` degrees, 3
/// unless given; `ONLY` keeps the families whose names hold one of its `|`-parted words.)
#[test]
#[ignore = "writes a table for tuning"]
fn table() {
    let step: u32 = std::env::var("STEP").ok().and_then(|s| s.parse().ok()).unwrap_or(3);
    let only = std::env::var("ONLY").ok();
    let all = swept(step, &|n| only.as_ref().is_none_or(|o| o.split('|').any(|o| n.contains(o))));
    let mut out = String::new();
    for (name, s) in all {
        out += &format!(
            "{name:30} lead R/S/P {:3?} none {:3} mean r{:.2} s{:.2} p{:.2} | r {:.2}-{:.2} s {:.2}-{:.2} p {:.2}-{:.2}\n",
            s.lead, s.none, s.mean[0], s.mean[1], s.mean[2], s.lo[0], s.hi[0], s.lo[1], s.hi[1], s.lo[2], s.hi[2]
        );
    }
    let path = std::env::var("OUT").map_or_else(|_| std::env::temp_dir().join("specula-shape-families.txt"), std::path::PathBuf::from);
    std::fs::write(&path, out).unwrap();
}

/// (What a silhouette costs to read, as a ratio to a disc's in the same process: the worst
/// cases its reviews found beside ordinary shapes. Written to `specula-shape-cost.txt` in the
/// temporary folder; it asserts nothing of the times, which belong to the machine.)
#[test]
#[ignore = "times the measure; run in release"]
fn cost() {
    let from = |f: &dyn Fn(u32, u32) -> bool| {
        let mut m = Mask::empty(512, 512);
        for y in 0..512 {
            for x in 0..512 {
                m.on[(y * 512 + x) as usize] = f(x, y);
            }
        }
        m
    };
    let cases: Vec<(&str, Mask)> = vec![
        ("disc", from(&|x, y| (x as f32 - 255.5).hypot(y as f32 - 255.5) < 225.0)),
        ("box", from(&|x, y| (60..452).contains(&x) && (100..412).contains(&y))),
        ("character", render(512, &*families().into_iter().find(|f| f.0 == "character").unwrap().1, 20.0, false, (0.0, 0.0))),
        ("comb teeth 8px", render(512, &*families().into_iter().find(|f| f.0 == "comb teeth 8px long 0.25").unwrap().1, 20.0, false, (0.0, 0.0))),
        ("noisy box 2px", render(512, &*families().into_iter().find(|f| f.0 == "noisy box 2px f200").unwrap().1, 20.0, false, (0.0, 0.0))),
        ("comb of 1px teeth", from(&|x, y| (y > 200 && y < 260 && x > 20 && x < 490) || ((260..460).contains(&y) && x > 20 && x < 490 && x % 2 == 0))),
        ("comb of 1px teeth, 2px gaps", from(&|x, y| (y > 200 && y < 260 && x > 20 && x < 490) || ((260..460).contains(&y) && x > 20 && x < 490 && x % 3 == 0))),
        ("big box and 1px comb", from(&|x, y| (y > 5 && y < 400 && x > 5 && x < 507) || ((400..500).contains(&y) && x > 5 && x < 507 && x % 2 == 0))),
        ("whole picture", from(&|_, _| true)),
    ];
    let time = |m: &Mask| {
        let started = std::time::Instant::now();
        for _ in 0..5 {
            std::hint::black_box(of_mask(m));
        }
        started.elapsed().as_secs_f64() / 5.0
    };
    let disc = time(&cases[0].1);
    let mut out = String::new();
    for (name, m) in &cases {
        let t = time(m);
        out += &format!("{name:30} {:6.2} ms  {:5.2} times a disc\n", t * 1000.0, t / disc);
    }
    std::fs::write(std::env::temp_dir().join("specula-shape-cost.txt"), out).unwrap();
}

/// **A short stalk is not its rim's corner, at any degree** (its fourth review: a stalk shorter
/// than the rim's window took a quarter of an apple as a point; and a stalk of six pixels, too
/// thin to leave a joint in the smoothed outline, did so at three degrees of ninety until a step
/// took its class only from parts at least half as thick as its own). Every degree, plain,
/// mirrored and shifted, for the apple and the disc with a whisker.
#[test]
fn a_short_stalk_is_not_its_rims_corner_at_any_degree() {
    for (name, s) in swept(1, &|n| n == "apple stem .05 6px" || n == "disc whisker 30px") {
        assert!(s.leads(R) && s.lo[R] >= 0.95 && s.hi[P] < 0.05, "{name}: {s:?}");
    }
}

fn cone(x: f32, y: f32, a: [f32; 2], b: [f32; 2], ra: f32, rb: f32) -> bool {
    let (abx, aby) = (b[0] - a[0], b[1] - a[1]);
    let t = (((x - a[0]) * abx + (y - a[1]) * aby) / (abx * abx + aby * aby)).clamp(0.0, 1.0);
    (x - a[0] - t * abx).hypot(y - a[1] - t * aby) < ra + (rb - ra) * t
}
fn ell(x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> bool {
    ((x - cx) / rx).hypot((y - cy) / ry) < 1.0
}

/// **The held-out corpus**: common game assets as silhouettes, each with the lead a person
/// would give it, written down before the measure was run on them (the commit that added
/// them came before any reading of them). `R`, `S`, `P`: round, square, pointed; two letters
/// where a person could say either (a chest, a barrel, a kite shield). From the fifth
/// review's probe (39), the fourth review's (24 not among the families) and the sixth
/// version's own (40), across weapons, tools, containers, plants, animals, characters,
/// vehicles, buildings, pickups and effects. It is not tuned against: what it reads is a
/// rate (`the_corpus_reads_as_a_person_would`).
fn corpus() -> Vec<(&'static str, &'static str, F)> {
    let mut v: Vec<(&'static str, &'static str, F)> = Vec::new();
    let mut p = |n: &'static str, lead: &'static str, f: F| v.push((n, lead, f));
    // The fifth review's assets.
    p("sword (round guard, pommel)", "P", Box::new(|x, y| within(&[[-0.035, -0.10], [0.035, -0.10], [0.035, 0.34], [0.0, 0.44], [-0.035, 0.34]], x, y) || rounded(x, y + 0.12, 0.15, 0.02, 0.01) || (x.abs() < 0.02 && (-0.36..-0.10).contains(&y)) || x.hypot(y + 0.39) < 0.04));
    p("round shield", "R", Box::new(|x, y| x.hypot(y) < 0.43));
    p("kite shield", "SP", Box::new(|x, y| within(&[[-0.3, -0.38], [0.3, -0.38], [0.3, 0.0], [0.0, 0.44], [-0.3, 0.0]], x, y)));
    p("potion", "R", Box::new(|x, y| x.hypot(y - 0.12) < 0.3 || (x.abs() < 0.07 && (-0.36..-0.1).contains(&y)) || rounded(x, y + 0.38, 0.09, 0.04, 0.02)));
    p("tree of three balls", "R", Box::new(|x, y| x.hypot(y + 0.14) < 0.22 || (x - 0.17).hypot(y + 0.02) < 0.16 || (x + 0.17).hypot(y + 0.02) < 0.16 || (x.abs() < 0.05 && (0.0..0.44).contains(&y))));
    p("rock (low polygon)", "RS", Box::new(|x, y| within(&[[-0.40, 0.2], [-0.33, -0.12], [-0.12, -0.30], [0.15, -0.27], [0.38, -0.08], [0.42, 0.2], [0.0, 0.28]], x, y)));
    p("house, 45 degree roof", "S", Box::new(|x, y| within(&[[-0.3, 0.4], [-0.3, 0.0], [-0.36, 0.0], [0.0, -0.42], [0.36, 0.0], [0.3, 0.0], [0.3, 0.4]], x, y)));
    p("chest, round lid", "SR", Box::new(|x, y| (x.abs() < 0.36 && (-0.05..0.28).contains(&y)) || (x.abs() < 0.36 && (y > -0.26 && y < -0.05) && ((x / 0.36).hypot((y + 0.05) / 0.21) < 1.0))));
    p("barrel", "RS", Box::new(|x, y| y.abs() < 0.4 && x.abs() < 0.25 + 0.06 * (1.0 - (y / 0.4).powi(2))));
    p("key", "RS", Box::new(|x, y| (0.05..0.13).contains(&(x + 0.3).hypot(y)) || (x > -0.18 && x < 0.42 && y.abs() < 0.03) || (x > 0.33 && x < 0.39 && y > 0.0 && y < 0.12) || (x > 0.22 && x < 0.28 && y > 0.0 && y < 0.09)));
    p("skull", "R", Box::new(|x, y| x.hypot(y + 0.08) < 0.3 || rounded(x, y - 0.24, 0.17, 0.12, 0.05)));
    p("heart", "R", Box::new(|x, y| (x - 0.15).hypot(y + 0.14) < 0.2 || (x + 0.15).hypot(y + 0.14) < 0.2 || within(&[[-0.33, -0.06], [0.33, -0.06], [0.0, 0.40]], x, y)));
    p("star", "P", Box::new(|x, y| within(&star(5, 0.45, 0.19), x, y)));
    p("gem", "PS", Box::new(|x, y| within(&[[-0.2, -0.3], [0.2, -0.3], [0.38, -0.12], [0.0, 0.40], [-0.38, -0.12]], x, y)));
    p("mushroom, oval cap", "R", Box::new(|x, y| ((x / 0.42).hypot((y + 0.02) / 0.32) < 1.0 && y < -0.02) || rounded(x, y - 0.18, 0.1, 0.2, 0.05)));
    p("cumulus cloud", "R", Box::new(|x, y| (x / 0.4).hypot((y - 0.08) / 0.12) < 1.0 || (x + 0.12).hypot(y + 0.02) < 0.16 || (x - 0.12).hypot(y + 0.06) < 0.19));
    p("fish", "R", Box::new(|x, y| ell(x, y, -0.06, 0.0, 0.3, 0.16) || within(&[[0.18, 0.0], [0.44, -0.18], [0.44, 0.18]], x, y)));
    p("bird", "PR", Box::new(|x, y| ell(x, y, 0.0, 0.05, 0.25, 0.13) || (x + 0.24).hypot(y + 0.06) < 0.09 || within(&[[-0.40, -0.06], [-0.30, -0.08], [-0.30, -0.03]], x, y) || within(&[[-0.05, 0.0], [0.15, 0.0], [0.25, -0.38]], x, y) || within(&[[0.2, 0.05], [0.44, -0.04], [0.44, 0.12]], x, y)));
    p("cat sitting", "R", Box::new(|x, y| ell(x, y, 0.0, 0.14, 0.2, 0.26) || x.hypot(y + 0.22) < 0.14 || within(&[[-0.13, -0.28], [-0.04, -0.34], [-0.12, -0.42]], x, y) || within(&[[0.13, -0.28], [0.04, -0.34], [0.12, -0.42]], x, y) || capsule(x, y, [0.15, 0.36], [0.38, 0.1], 0.03)));
    p("robot", "S", Box::new(|x, y| (x.abs() < 0.1 && (-0.42..-0.24).contains(&y)) || (x.abs() < 0.18 && (-0.22..0.1).contains(&y)) || (((x.abs() - 0.24).abs() < 0.04) && (-0.2..0.08).contains(&y)) || (((x.abs() - 0.09).abs() < 0.05) && (0.1..0.42).contains(&y))));
    p("car", "SR", Box::new(|x, y| rounded(x, y - 0.04, 0.44, 0.09, 0.04) || within(&[[-0.24, -0.05], [-0.14, -0.2], [0.14, -0.2], [0.26, -0.05]], x, y) || (x + 0.26).hypot(y - 0.14) < 0.08 || (x - 0.26).hypot(y - 0.14) < 0.08));
    p("rocket", "P", Box::new(|x, y| rounded(x, y - 0.06, 0.1, 0.26, 0.03) || within(&[[-0.1, -0.2], [0.1, -0.2], [0.0, -0.44]], x, y) || within(&[[-0.1, 0.12], [-0.22, 0.38], [-0.1, 0.30]], x, y) || within(&[[0.1, 0.12], [0.22, 0.38], [0.1, 0.30]], x, y)));
    p("hammer", "S", Box::new(|x, y| (x.abs() < 0.035 && (-0.2..0.44).contains(&y)) || rounded(x, y + 0.29, 0.22, 0.09, 0.01)));
    p("crown", "P", Box::new(|x, y| (x.abs() < 0.36 && (0.05..0.25).contains(&y)) || (0..5).any(|k| { let cx = -0.30 + 0.15 * k as f32; within(&[[cx - 0.07, 0.06], [cx + 0.07, 0.06], [cx, -0.28]], x, y) })));
    p("bone", "R", Box::new(|x, y| capsule(x, y, [-0.3, 0.0], [0.3, 0.0], 0.05) || (x + 0.33).hypot(y + 0.06) < 0.08 || (x + 0.33).hypot(y - 0.06) < 0.08 || (x - 0.33).hypot(y + 0.06) < 0.08 || (x - 0.33).hypot(y - 0.06) < 0.08));
    p("leaf with a stem", "PR", Box::new(|x, y| ((x - 0.3).hypot(y) < 0.48 && (x + 0.3).hypot(y) < 0.48 && y.abs() < 0.40) || (x.abs() < 0.008 && (0.30..0.44).contains(&y))));
    p("flame", "P", Box::new(|x, y| (x.hypot(y - 0.16) < 0.22) || within(&[[-0.21, 0.10], [-0.08, -0.15], [-0.12, -0.32], [0.02, -0.16], [0.06, -0.44], [0.16, -0.12], [0.21, 0.10]], x, y)));
    p("drop", "RP", Box::new(|x, y| x.hypot(y - 0.15) < 0.25 || within(&[[0.0, -0.44], [0.212, 0.018], [-0.212, 0.018]], x, y)));
    p("bomb", "R", Box::new(|x, y| x.hypot(y - 0.08) < 0.33 || capsule(x, y, [0.05, -0.24], [0.14, -0.40], 0.015)));
    p("coin", "R", Box::new(|x, y| x.hypot(y) < 0.4));
    p("ghost, scalloped hem", "R", Box::new(|x, y| (x.hypot(y + 0.1) < 0.3 && y < -0.1) || (x.abs() < 0.3 && y >= -0.1 && y < 0.25 + 0.08 * ((x + 0.3) / 0.6 * 4.0 * PI).cos().abs())));
    p("double axe", "PS", Box::new(|x, y| (x.abs() < 0.025 && y.abs() < 0.44) || (x.abs() < 0.32 && { let t = ((x.abs() - 0.02) / 0.30).clamp(0.0, 1.0); (y + 0.18).abs() < 0.04 + 0.14 * t * t })));
    p("arrow", "P", Box::new(|x, y| (x.abs() < 0.012 && (-0.30..0.36).contains(&y)) || within(&[[0.0, -0.44], [0.06, -0.28], [-0.06, -0.28]], x, y) || within(&[[-0.012, 0.26], [-0.06, 0.40], [-0.012, 0.36]], x, y) || within(&[[0.012, 0.26], [0.06, 0.40], [0.012, 0.36]], x, y)));
    p("lightning bolt", "P", Box::new(|x, y| within(&[[0.10, -0.44], [-0.18, 0.04], [0.0, 0.04], [-0.10, 0.44], [0.20, -0.06], [0.02, -0.06], [0.16, -0.44]], x, y)));
    p("bell", "R", Box::new(|x, y| (y > -0.3 && y < 0.3 && x.abs() < 0.12 + 0.24 * ((y + 0.3) / 0.6).powi(2)) || x.hypot(y + 0.3) < 0.12 || x.hypot(y - 0.34) < 0.06));
    p("carrot", "P", Box::new(|x, y| within(&[[-0.1, -0.2], [0.1, -0.2], [0.0, 0.44]], x, y) || x.hypot(y + 0.2) < 0.1 || within(&[[-0.02, -0.25], [-0.12, -0.44], [0.0, -0.3], [0.12, -0.44], [0.02, -0.25]], x, y)));
    p("crate", "S", Box::new(|x, y| x.abs() < 0.34 && y.abs() < 0.34));
    p("tombstone, round top", "RS", Box::new(|x, y| (x.abs() < 0.22 && (-0.18..0.40).contains(&y)) || x.hypot(y + 0.18) < 0.22));
    p("pine tree", "P", Box::new(|x, y| within(&[[0.0, -0.44], [0.3, 0.26], [-0.3, 0.26]], x, y) || (x.abs() < 0.05 && (0.2..0.42).contains(&y))));
    // The fourth review's game shapes that are not among the families.
    p("crate, bevelled", "S", Box::new(|x, y| x.abs() < 0.3 && y.abs() < 0.3 && x.abs() + y.abs() < 0.6 - 0.03));
    p("crate, rounded a twentieth", "S", Box::new(|x, y| rounded(x, y, 0.3, 0.3, 0.03)));
    p("crate, rounded a twelfth", "S", Box::new(|x, y| rounded(x, y, 0.3, 0.3, 0.05)));
    p("tree, cone on a trunk", "P", Box::new(|x, y| within(&[[0.0, -0.44], [0.24, 0.28], [-0.24, 0.28]], x, y) || (x.abs() < 0.04 && (0.2..0.44).contains(&y))));
    p("gear, wavy teeth", "R", Box::new(|x, y| { let r = x.hypot(y); let a = y.atan2(x); r < 0.32 + 0.10 * (0.5 + 0.5 * (a * 10.0).cos()) }));
    p("leaf (lens)", "PR", Box::new(|x, y| (x - 0.25).hypot(y) < 0.40 && (x + 0.25).hypot(y) < 0.40));
    p("leaf and stem", "PR", Box::new(|x, y| ((x - 0.2).hypot(y + 0.06) < 0.34 && (x + 0.2).hypot(y + 0.06) < 0.34) || (x.abs() < 0.012 && (0.2..0.44).contains(&y))));
    p("crescent moon, thick", "P", Box::new(|x, y| x.hypot(y) < 0.42 && (x - 0.2).hypot(y) > 0.34));
    p("cloud, flat base", "R", Box::new(|x, y| y < 0.12 && (x.hypot(y) < 0.22 || (x - 0.22).hypot(y - 0.03) < 0.15 || (x + 0.22).hypot(y - 0.03) < 0.15 || (x.abs() < 0.3 && y > -0.02))));
    p("speech bubble, oval", "R", Box::new(|x, y| (x / 0.42).hypot(y / 0.28) < 1.0 || within(&[[-0.15, 0.20], [-0.02, 0.22], [-0.25, 0.42]], x, y)));
    p("speech bubble, box", "RS", Box::new(|x, y| rounded(x, y + 0.05, 0.42, 0.25, 0.08) || within(&[[-0.15, 0.15], [-0.02, 0.15], [-0.25, 0.40]], x, y)));
    p("block arrow", "P", Box::new(|x, y| within(&[[0.44, 0.0], [0.14, -0.2], [0.14, -0.06], [-0.42, -0.06], [-0.42, 0.06], [0.14, 0.06], [0.14, 0.2]], x, y)));
    p("bone, flat shaft", "R", Box::new(|x, y| (x.abs() < 0.30 && y.abs() < 0.05) || (x.abs() - 0.32).hypot(y.abs() - 0.07) < 0.08));
    p("arch gate", "RS", Box::new(|x, y| ((x.abs() < 0.36 && (-0.05..0.30).contains(&y)) || x.hypot(y + 0.05) < 0.36) && !((x.abs() < 0.22 && (-0.05..0.41).contains(&y)) || x.hypot(y + 0.05) < 0.22)));
    p("cross pattee", "SP", Box::new(|x, y| { let (u, w) = (x.abs(), y.abs()); let (a, b) = if u > w { (u, w) } else { (w, u) }; a < 0.42 && b < 0.04 + 0.12 * (a / 0.42) }));
    p("grave cross", "S", Box::new(|x, y| (x.abs() < 0.05 && (-0.42..0.44).contains(&y)) || (y > -0.26 && y < -0.16 && x.abs() < 0.24)));
    p("ring", "R", Box::new(|x, y| (0.25..0.42).contains(&x.hypot(y))));
    p("horseshoe", "RS", Box::new(|x, y| ((0.22..0.40).contains(&x.hypot(y + 0.04)) && y < 0.0) || (x.abs() > 0.22 && x.abs() < 0.40 && (-0.04..0.26).contains(&y))));
    p("mushroom, rounded stem", "R", Box::new(|x, y| (x.hypot(y) < 0.40 && y < 0.0) || rounded(x, y - 0.18, 0.09, 0.2, 0.06)));
    p("candle and flame", "S", Box::new(|x, y| (x.abs() < 0.08 && (-0.1..0.42).contains(&y)) || x.hypot(y + 0.2) < 0.06 || within(&[[0.0, -0.40], [0.058, -0.21], [-0.058, -0.21]], x, y)));
    p("disc, sixteen hairs", "R", Box::new(|x, y| x.hypot(y) < 0.3 || ((0..8).any(|k| { let a = PI * k as f32 / 8.0; (x * a.sin() - y * a.cos()).abs() < 0.001 }) && x.hypot(y) < 0.45)));
    p("sun of twelve rays", "P", Box::new(|x, y| x.hypot(y) < 0.22 || ((0..12).any(|k| { let a = TAU * k as f32 / 12.0; let (u, w) = (x * a.cos() + y * a.sin(), -x * a.sin() + y * a.cos()); u > 0.2 && u < 0.44 && w.abs() < 0.03 * (0.44 - u) / 0.24 }))));
    p("hedgehog", "P", Box::new(|x, y| (y < 0.1 && (x / 0.36).hypot((y - 0.1) / 0.24) < 1.0) || (y < 0.1 && (0..20).any(|k| { let a = PI + PI * (k as f32 + 0.5) / 20.0; let (u, w) = (x * a.cos() + (y - 0.1) * a.sin(), -x * a.sin() + (y - 0.1) * a.cos()); u > 0.2 && u < 0.40 && w.abs() < 0.012 * (0.40 - u) / 0.2 }))));
    p("grass tuft", "P", Box::new(|x, y| (y > 0.25 && y < 0.32 && x.abs() < 0.3) || (0..7).any(|k| { let bx = -0.27 + 0.09 * k as f32; let top = -0.40 + 0.05 * ((k as f32 - 3.0).abs()); within(&[[bx - 0.025, 0.30], [bx + 0.025, 0.30], [bx + 0.02 * (k as f32 - 3.0) / 3.0, top]], x, y) })));
    // The sixth version's own. Weapons and tools.
    p("spear", "P", Box::new(|x, y| (x.abs() < 0.012 && (-0.2..0.45).contains(&y)) || within(&[[0.0, -0.45], [0.05, -0.3], [0.03, -0.2], [-0.03, -0.2], [-0.05, -0.3]], x, y)));
    p("shuriken", "P", Box::new(|x, y| within(&star(4, 0.44, 0.12), x, y)));
    p("trident", "P", Box::new(|x, y| (x.abs() < 0.015 && (-0.18..0.45).contains(&y)) || (x.abs() < 0.155 && (-0.21..-0.16).contains(&y)) || [-0.13f32, 0.0, 0.13].iter().any(|cx| within(&[[cx - 0.025, -0.18], [cx + 0.025, -0.18], [*cx, -0.44]], x, y))));
    p("club", "R", Box::new(|x, y| cone(x, y, [0.0, 0.4], [0.0, -0.25], 0.03, 0.1) || x.hypot(y + 0.25) < 0.1));
    p("pickaxe", "P", Box::new(|x, y| (x.abs() < 0.02 && (-0.24..0.44).contains(&y)) || within(&[[-0.42, -0.1], [-0.2, -0.28], [0.0, -0.33], [0.2, -0.28], [0.42, -0.1], [0.18, -0.21], [0.0, -0.24], [-0.18, -0.21]], x, y)));
    p("saw", "PS", Box::new(|x, y| (x > -0.4 && x < 0.3 && (-0.1..0.05).contains(&y)) || (y >= 0.05 && y < 0.1 && x > -0.4 && x < 0.3 && { let t = ((x + 0.4) / 0.06).fract(); y - 0.05 < 0.05 * (1.0 - (2.0 * t - 1.0).abs()) }) || rounded(x - 0.37, y + 0.02, 0.07, 0.1, 0.03)));
    p("frying pan", "R", Box::new(|x, y| (x - 0.12).hypot(y) < 0.28 || capsule(x, y, [-0.44, 0.0], [-0.1, 0.0], 0.03)));
    // Containers.
    p("bucket", "S", Box::new(|x, y| within(&[[-0.3, -0.1], [0.3, -0.1], [0.22, 0.42], [-0.22, 0.42]], x, y) || ((0.27..0.31).contains(&x.hypot(y + 0.1)) && y < -0.1)));
    p("jar", "R", Box::new(|x, y| rounded(x, y - 0.06, 0.25, 0.32, 0.15) || rounded(x, y + 0.31, 0.17, 0.06, 0.03)));
    p("sack", "R", Box::new(|x, y| x.hypot(y - 0.12) < 0.3 || within(&[[-0.1, -0.12], [0.1, -0.12], [0.03, -0.27], [-0.03, -0.27]], x, y) || ell(x, y, 0.0, -0.31, 0.1, 0.06)));
    p("cauldron", "R", Box::new(|x, y| (x.hypot(y + 0.02) < 0.34 && y > -0.2) || rounded(x, y + 0.22, 0.38, 0.04, 0.03) || capsule(x, y, [-0.2, 0.24], [-0.22, 0.36], 0.035) || capsule(x, y, [0.2, 0.24], [0.22, 0.36], 0.035)));
    // Plants.
    p("cactus", "R", Box::new(|x, y| capsule(x, y, [0.0, -0.34], [0.0, 0.36], 0.09) || capsule(x, y, [-0.26, -0.2], [-0.26, 0.04], 0.06) || capsule(x, y, [-0.26, 0.04], [0.0, 0.04], 0.06) || capsule(x, y, [0.26, -0.28], [0.26, -0.06], 0.06) || capsule(x, y, [0.26, -0.06], [0.0, -0.06], 0.06)));
    p("flower of six petals", "R", Box::new(|x, y| x.hypot(y) < 0.12 || (0..6).any(|k| { let a = TAU * k as f32 / 6.0; (x - 0.25 * a.cos()).hypot(y - 0.25 * a.sin()) < 0.13 })));
    p("bush", "R", Box::new(|x, y| y < 0.25 && (x.hypot(y + 0.08) < 0.24 || (x - 0.22).hypot(y - 0.06) < 0.19 || (x + 0.22).hypot(y - 0.06) < 0.19 || (x - 0.1).hypot(y + 0.2) < 0.15)));
    // Animals.
    p("turtle", "R", Box::new(|x, y| (ell(x, y, 0.0, 0.05, 0.32, 0.26) && y < 0.08) || (x.abs() < 0.34 && (0.04..0.11).contains(&y)) || (x - 0.38).hypot(y - 0.02) < 0.08 || [-0.2f32, 0.2].iter().any(|lx| capsule(x, y, [*lx, 0.1], [*lx, 0.2], 0.05))));
    p("whale", "R", Box::new(|x, y| ell(x, y, -0.06, 0.04, 0.36, 0.18) || within(&[[0.26, 0.0], [0.44, -0.16], [0.40, 0.02], [0.44, 0.18]], x, y)));
    p("snail", "R", Box::new(|x, y| (x - 0.05).hypot(y + 0.06) < 0.25 || capsule(x, y, [-0.32, 0.2], [0.32, 0.2], 0.07) || capsule(x, y, [-0.3, 0.18], [-0.36, 0.0], 0.02)));
    p("shark", "P", Box::new(|x, y| ell(x, y, -0.04, 0.04, 0.36, 0.11) || within(&[[-0.1, -0.04], [0.06, -0.04], [-0.06, -0.3]], x, y) || within(&[[0.28, 0.04], [0.45, -0.18], [0.38, 0.05], [0.44, 0.2]], x, y)));
    p("bat", "P", Box::new(|x, y| ell(x, y, 0.0, 0.0, 0.07, 0.12) || within(&[[0.0, -0.06], [0.44, -0.2], [0.36, 0.0], [0.27, -0.03], [0.2, 0.08], [0.12, 0.02], [0.0, 0.08], [-0.12, 0.02], [-0.2, 0.08], [-0.27, -0.03], [-0.36, 0.0], [-0.44, -0.2]], x, y)));
    // Characters.
    p("gingerbread man", "R", Box::new(|x, y| x.hypot(y + 0.28) < 0.12 || capsule(x, y, [0.0, -0.12], [0.0, 0.1], 0.13) || capsule(x, y, [-0.1, -0.08], [-0.34, -0.04], 0.06) || capsule(x, y, [0.1, -0.08], [0.34, -0.04], 0.06) || capsule(x, y, [-0.07, 0.1], [-0.16, 0.38], 0.065) || capsule(x, y, [0.07, 0.1], [0.16, 0.38], 0.065)));
    p("knight, box helm", "S", Box::new(|x, y| rounded(x, y + 0.31, 0.09, 0.1, 0.01) || (x.abs() < 0.17 && (-0.21..0.1).contains(&y)) || ((x.abs() - 0.23).abs() < 0.05 && (-0.2..0.06).contains(&y)) || ((x.abs() - 0.08).abs() < 0.06 && (0.1..0.42).contains(&y))));
    p("wizard", "P", Box::new(|x, y| within(&[[0.0, -0.45], [0.14, -0.2], [-0.14, -0.2]], x, y) || (x.abs() < 0.2 && (-0.22..-0.18).contains(&y)) || x.hypot(y + 0.12) < 0.08 || within(&[[0.0, -0.08], [0.26, 0.42], [-0.26, 0.42]], x, y)));
    // Vehicles.
    p("tank", "S", Box::new(|x, y| within(&[[-0.42, 0.04], [0.42, 0.04], [0.36, 0.22], [-0.36, 0.22]], x, y) || (x.abs() < 0.18 && (-0.1..0.05).contains(&y)) || (x > 0.1 && x < 0.45 && (-0.07..-0.03).contains(&y))));
    p("truck", "S", Box::new(|x, y| (x > -0.4 && x < 0.16 && (-0.22..0.12).contains(&y)) || (x >= 0.16 && x < 0.42 && (-0.1..0.12).contains(&y)) || (x + 0.28).hypot(y - 0.14) < 0.08 || (x - 0.28).hypot(y - 0.14) < 0.08));
    p("aeroplane from above", "P", Box::new(|x, y| capsule(x, y, [0.0, -0.4], [0.0, 0.38], 0.05) || within(&[[0.0, -0.1], [0.44, 0.1], [0.44, 0.15], [0.0, 0.06], [-0.44, 0.15], [-0.44, 0.1]], x, y) || within(&[[0.0, 0.26], [0.16, 0.38], [0.16, 0.42], [-0.16, 0.42], [-0.16, 0.38]], x, y)));
    p("sailing boat", "PS", Box::new(|x, y| within(&[[-0.42, 0.18], [0.42, 0.18], [0.3, 0.36], [-0.3, 0.36]], x, y) || (x.abs() < 0.015 && (-0.42..0.18).contains(&y)) || within(&[[0.03, -0.4], [0.3, 0.12], [0.03, 0.12]], x, y)));
    // Buildings.
    p("castle tower", "S", Box::new(|x, y| (x.abs() < 0.22 && (-0.3..0.44).contains(&y)) || (y > -0.42 && y <= -0.3 && x.abs() < 0.26 && ((x + 0.26) / 0.104).fract() < 0.6)));
    p("church with a spire", "SP", Box::new(|x, y| (x.abs() < 0.28 && (0.0..0.38).contains(&y)) || (x.abs() < 0.1 && (-0.2..0.0).contains(&y)) || within(&[[0.0, -0.45], [0.1, -0.2], [-0.1, -0.2]], x, y)));
    p("igloo", "R", Box::new(|x, y| (ell(x, y, 0.0, 0.2, 0.42, 0.42) && y < 0.2) || rounded(x - 0.32, y - 0.14, 0.12, 0.06, 0.06)));
    p("skyscraper", "S", Box::new(|x, y| (x.abs() < 0.12 && (-0.36..0.44).contains(&y)) || (x.abs() < 0.06 && (-0.44..-0.36).contains(&y))));
    p("tent", "P", Box::new(|x, y| within(&[[0.0, -0.38], [0.38, 0.26], [-0.38, 0.26]], x, y)));
    // Pickups and effects.
    p("diamond", "P", Box::new(|x, y| x.abs() / 0.3 + y.abs() / 0.44 < 1.0));
    p("battery", "S", Box::new(|x, y| rounded(x, y - 0.04, 0.16, 0.36, 0.03) || (x.abs() < 0.07 && (-0.38..-0.3).contains(&y))));
    p("anvil", "SP", Box::new(|x, y| within(&[[-0.44, -0.2], [-0.2, -0.24], [0.32, -0.24], [0.32, -0.08], [0.12, -0.04], [0.12, 0.14], [0.26, 0.14], [0.26, 0.24], [-0.22, 0.24], [-0.22, 0.14], [-0.1, 0.14], [-0.1, -0.04], [-0.2, -0.1]], x, y)));
    p("explosion", "P", Box::new(|x, y| { let a = y.atan2(x); let k = ((a / TAU + 0.5) * 10.0).floor() as i32; let t = ((a / TAU + 0.5) * 10.0).fract(); let r0 = 0.3 + 0.12 * (0.5 + hash(k)); x.hypot(y) < 0.18 + (r0 - 0.18) * (1.0 - (2.0 * t - 1.0).abs()) }));
    p("sparkle", "P", Box::new(|x, y| { let (u, w) = (x.abs() / 0.44, y.abs() / 0.44); u.sqrt() + w.sqrt() < 1.0 }));
    p("smoke puff", "R", Box::new(|x, y| x.hypot(y) < 0.2 || (x - 0.2).hypot(y - 0.08) < 0.16 || (x + 0.2).hypot(y - 0.1) < 0.15 || (x - 0.08).hypot(y + 0.2) < 0.15 || (x + 0.14).hypot(y + 0.14) < 0.13));
    p("heart pickup, plump", "R", Box::new(|x, y| (x - 0.17).hypot(y + 0.1) < 0.22 || (x + 0.17).hypot(y + 0.1) < 0.22 || (y > -0.1 && y < 0.38 && x.abs() < 0.36 * (1.0 - ((y + 0.1) / 0.48).powf(1.6)).max(0.0))));
    p("health cross", "S", Box::new(|x, y| (x.abs() < 0.4 && y.abs() < 0.13) || (x.abs() < 0.13 && y.abs() < 0.4)));
    p("shield, round bottom", "RS", Box::new(|x, y| (x.abs() < 0.3 && (-0.36..0.04).contains(&y)) || (ell(x, y, 0.0, 0.04, 0.3, 0.38) && y >= 0.04)));
    drop(p);
    v
}

/// The corpus swept every `step` degrees: each shape's name, the leads a person would give,
/// and its sweep.
fn corpus_swept(step: u32) -> Vec<(&'static str, &'static str, Sweep)> {
    let only = std::env::var("ONLY").ok();
    let all: Vec<_> = corpus().into_iter().filter(|(n, _, _)| only.as_ref().is_none_or(|o| o.split('|').any(|o| n.contains(o)))).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let out = std::sync::Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(12);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some((name, lead, f)) = all.get(i) else { break };
                let r = sweep(&**f, step);
                out.lock().unwrap().push((i, *name, *lead, r));
            });
        }
    });
    let mut out = out.into_inner().unwrap();
    out.sort_by_key(|r| r.0);
    out.into_iter().map(|(_, n, l, r)| (n, l, r)).collect()
}

/// Whether a sweep leads, in every mask, one of the kinds a person would give (`R`, `S`, `P`).
fn reads_as(s: &Sweep, lead: &str) -> bool {
    s.none == 0 && (0..3).all(|k| s.lead[k] == 0 || lead.contains(["R", "S", "P"][k]))
}
/// Whether a sweep leads one of those kinds in most masks.
fn mostly_reads_as(s: &Sweep, lead: &str) -> bool {
    let total: usize = s.lead.iter().sum();
    let right: usize = (0..3).filter(|k| lead.contains(["R", "S", "P"][*k])).map(|k| s.lead[k]).sum();
    2 * right > total
}

/// (For whoever tunes the measure: the corpus's readings written to `specula-shape-corpus.txt`
/// in the temporary folder, or `OUT`, every `STEP` degrees, 3 unless given.)
#[test]
#[ignore = "writes the corpus's table"]
fn corpus_table() {
    let step: u32 = std::env::var("STEP").ok().and_then(|s| s.parse().ok()).unwrap_or(3);
    let all = corpus_swept(step);
    let mut out = String::new();
    let (mut right, mut mostly) = (0, 0);
    for (name, lead, s) in &all {
        let (r, m) = (reads_as(s, lead), mostly_reads_as(s, lead));
        right += usize::from(r);
        mostly += usize::from(m);
        out += &format!(
            "{name:30} {lead:2} {} lead R/S/P {:3?} none {:3} mean r{:.2} s{:.2} p{:.2} | r {:.2}-{:.2} s {:.2}-{:.2} p {:.2}-{:.2}\n",
            if r { "ok  " } else if m { "most" } else { "NO  " },
            s.lead, s.none, s.mean[0], s.mean[1], s.mean[2], s.lo[0], s.hi[0], s.lo[1], s.hi[1], s.lo[2], s.hi[2]
        );
    }
    out += &format!("{right} of {} lead as a person would in every mask; {mostly} in most\n", all.len());
    let path = std::env::var("OUT").map_or_else(|_| std::env::temp_dir().join("specula-shape-corpus.txt"), std::path::PathBuf::from);
    std::fs::write(&path, out).unwrap();
}

/// No corpus shape reaches the picture's edge at any angle (a shape that did would be cut).
#[test]
fn the_corpus_fits_its_pictures() {
    let mut bad = Vec::new();
    for (name, _, f) in corpus() {
        for deg in [0.0f32, 30.0, 45.0, 60.0] {
            let m = render(512, &*f, deg, false, (0.37, 0.71));
            let edge = (0..512usize).any(|i| m.on[i] || m.on[511 * 512 + i] || m.on[i * 512] || m.on[i * 512 + 511]);
            if edge {
                bad.push(format!("{name} at {deg}"));
            }
        }
    }
    assert!(bad.is_empty(), "reach the edge: {bad:?}");
}

/// The masks whose cost its reviews found worst, and the worst made since: coils of wire,
/// combs of one-pixel teeth, spirals, fine branches, the whole picture.
fn costly() -> Vec<(&'static str, Mask)> {
    let from = |f: &dyn Fn(i32, i32) -> bool| {
        let mut m = Mask::empty(512, 512);
        for y in 0..512 {
            for x in 0..512 {
                m.on[(y * 512 + x) as usize] = f(x, y);
            }
        }
        m
    };
    let coil = |wire: f32, pitch: f32| {
        move |x: i32, y: i32| {
            let (u, v) = (x as f32 - 256.0, y as f32 - 256.0);
            let (r, a) = (u.hypot(v), v.atan2(u) + PI);
            (u - 127.0).hypot(v) < 12.0 || (0..40).any(|k| { let rr = 115.0 + pitch * (a + TAU * k as f32) / TAU; (r - rr).abs() < wire / 2.0 && rr < 250.0 })
        }
    };
    vec![
        ("disc", from(&|x, y| (x as f32 - 255.5).hypot(y as f32 - 255.5) < 225.0)),
        ("coil of 3px wire", from(&coil(3.0, 6.0))),
        ("coil of 2.5px wire", from(&coil(2.5, 5.0))),
        ("coil of 4px wire", from(&coil(4.0, 8.0))),
        ("coil of 6px wire", from(&coil(6.0, 12.0))),
        ("comb of 1px teeth, full height", from(&|x, y| (y > 2 && y < 14 && x > 2 && x < 509) || ((14..509).contains(&y) && x > 2 && x < 509 && x % 2 == 0))),
        ("double comb of 1px teeth", from(&|x, y| (y > 250 && y < 262 && x > 2 && x < 509) || (((3..250).contains(&y) || (262..509).contains(&y)) && x > 2 && x < 509 && x % 2 == 0))),
        ("comb of 1px teeth, a staircase", from(&|x, y| (y > 2 && y < 14 && x > 2 && x < 509) || ((14..509).contains(&y) && x > 2 && x < 509 && x % 2 == 0 && y < 14 + x))),
        ("comb of 3px teeth, 1px gaps", from(&|x, y| (y > 2 && y < 14 && x > 2 && x < 509) || ((14..509).contains(&y) && x > 2 && x < 509 && x % 4 != 0))),
        ("comb of 5px teeth, tapering", from(&|x, y| (y > 2 && y < 14 && x > 2 && x < 509) || ((14..509).contains(&y) && x > 2 && x < 509 && (x % 12) < 6 - (y - 14) / 100))),
        ("big box and 1px comb", from(&|x, y| (y > 5 && y < 400 && x > 5 && x < 507) || ((400..500).contains(&y) && x > 5 && x < 507 && x % 2 == 0))),
        ("square spiral", from(&|x, y| { let (cx, cy) = (x - 256, y - 256); let ring = cx.abs().max(cy.abs()); let band = ring / 10; band % 2 == 0 && !(cy > 0 && cx.abs() < 5 && band % 4 == 0) && ring < 250 })),
        ("tapering spiral horn", from(&|x, y| { let (u, v) = (x as f32 - 256.0, y as f32 - 256.0); let r = u.hypot(v); let a = v.atan2(u); (0..6).any(|k| { let th = a + PI + TAU * k as f32; let rr = 20.0 + 6.0 * th; let w = 1.0 + 0.6 * th; (r - rr).abs() < w && rr < 250.0 }) })),
        ("spiral of 9px", from(&|x, y| { let (u, v) = (x as f32 - 256.0, y as f32 - 256.0); let r = u.hypot(v); let a = v.atan2(u); (0..14).any(|k| { let rr = 30.0 + 18.0 / TAU * (a + PI + TAU * k as f32); (r - rr).abs() < 4.5 && rr < 250.0 }) })),
        ("thirty deep lobes", from(&|x, y| { let (u, v) = (x as f32 - 256.0, y as f32 - 256.0); u.hypot(v) < 180.0 + 60.0 * (30.0 * v.atan2(u)).sin() })),
        ("fine branches", from(&|x, y| { let (u, v) = (x as f32 - 256.0, y as f32); (u.abs() < 6.0 && v > 300.0) || (v < 300.0 && v > 40.0 && ((u / 5.0).round() * 5.0 - u).abs() < 1.2 && u.abs() < (300.0 - v) * 0.8) || (v > 290.0 && v < 305.0 && u.abs() < 220.0) })),
        ("a sun of 200 hairs", from(&|x, y| { let (u, v) = (x as f32 - 256.0, y as f32 - 256.0); let r = u.hypot(v); r < 120.0 || (r < 250.0 && ((v.atan2(u) / TAU + 0.5) * 200.0).fract() < 0.5 * 2.0 / (r * TAU / 200.0)) })),
        ("a box with a ragged rim", from(&|x, y| { let n = ((x * 7919 + y * 104729) % 97) as f32 / 97.0; (40..472).contains(&x) && (40..472).contains(&y) && !((x < 60 || x > 452 || y < 60 || y > 452) && n < 0.5) })),
        ("a checkerboard over a box", from(&|x, y| ((100..412).contains(&x) && (100..412).contains(&y)) || ((x + y) % 2 == 0 && (x / 16 + y / 16) % 2 == 0))),
        ("whole picture", from(&|_, _| true)),
    ]
}

/// The pixels of a mask on its piece's edge (on, with a side neighbour off or off the picture):
/// about the length of its outlines.
fn edge_pixels(m: &Mask) -> u64 {
    let (w, h) = (m.w as i32, m.h as i32);
    let on = |x: i32, y: i32| x >= 0 && y >= 0 && x < w && y < h && m.on[(y * w + x) as usize];
    (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|(x, y)| on(*x, *y) && !(on(x - 1, *y) && on(x + 1, *y) && on(*x, y - 1) && on(*x, y + 1))).count() as u64
}

/// (The work and time of each costly mask, as ratios: written to `specula-shape-cost.txt` in
/// the temporary folder.)
#[test]
#[ignore = "times the measure; run in release"]
fn cost_table() {
    let mut out = String::new();
    let cases = costly();
    let time = |m: &Mask| {
        let started = std::time::Instant::now();
        for _ in 0..3 {
            std::hint::black_box(of_mask(m));
        }
        started.elapsed().as_secs_f64() / 3.0
    };
    let disc = time(&cases[0].1);
    for (name, m) in &cases {
        let (_, work) = specula_studio::shape::of_mask_counted(m);
        let t = time(m);
        let (px, edge) = ((m.w * m.h) as u64, edge_pixels(m));
        out += &format!("{name:34} {:8.1} ms {:6.2} times a disc   work {work:10}  {:5.1} a pixel  {:6.1} an edge pixel\n", t * 1000.0, t / disc, work as f64 / px as f64, work as f64 / edge.max(1) as f64);
    }
    std::fs::write(std::env::temp_dir().join("specula-shape-cost.txt"), out).unwrap();
}

/// The shapes the size test draws at every size, across the families: polygons, rounded and
/// cut boxes, a plank, ovals, a lens, combs, figures, bottles, stalks and common assets.
const SIZED: [&str; 24] = [
    "disc", "box 0.8x0.6 round 0", "triangle", "star 5", "house", "L", "cross", "box 0.8x0.6 round 0.1", "box 0.8x0.6 round 0.33", "box 0.8x0.2 round 0.33", "box 0.8x0.4 cut 0.05", "oval 3:1", "oval 6:1", "lens tip 90", "crescent", "comb teeth 24px long 0.25", "character", "char T-pose", "bottle square shoulder", "stalk 12px len 0.06", "cloud", "heart", "mace", "pear",
];

/// A sweep of a family at another size of picture (the shape drawn as large in it).
fn sweep_at(f: &(dyn Fn(f32, f32) -> bool + Sync), step: u32, side: u32) -> Sweep {
    let (mut lo, mut hi, mut sum, mut lead, mut none, mut n) = ([1.0f32; 3], [0.0f32; 3], [0.0f32; 3], [0usize; 3], 0usize, 0usize);
    for deg in (0..90).step_by(step as usize) {
        for mirror in [false, true] {
            for off in [(0.0, 0.0), (0.37, 0.71)] {
                n += 1;
                let Some(a) = of_mask(&render(side, f, deg as f32, mirror, off)) else {
                    none += 1;
                    continue;
                };
                let sh = [a.round, a.square, a.pointed];
                for k in 0..3 {
                    lo[k] = lo[k].min(sh[k]);
                    hi[k] = hi[k].max(sh[k]);
                    sum[k] += sh[k];
                }
                lead[a.leading() as usize] += 1;
            }
        }
    }
    let m = (n - none).max(1) as f32;
    Sweep { lo, hi, mean: [sum[0] / m, sum[1] / m, sum[2] / m], lead, none }
}

/// Each sized shape at each size of picture, every `step` degrees: its sweep.
fn sized(step: u32, sides: &[u32]) -> Vec<(String, u32, Sweep)> {
    let fam: Vec<(String, F)> = families().into_iter().filter(|(n, _)| SIZED.contains(&n.as_str())).collect();
    assert_eq!(fam.len(), SIZED.len(), "every sized shape is a family");
    let jobs: Vec<(usize, u32)> = (0..fam.len()).flat_map(|i| sides.iter().map(move |s| (i, *s))).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let out = std::sync::Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(12);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let k = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some((i, side)) = jobs.get(k) else { break };
                let r = sweep_at(&*fam[*i].1, step, *side);
                out.lock().unwrap().push((k, fam[*i].0.clone(), *side, r));
            });
        }
    });
    let mut out = out.into_inner().unwrap();
    out.sort_by_key(|r| r.0);
    out.into_iter().map(|(_, n, s, r)| (n, s, r)).collect()
}

/// (For whoever tunes the measure: the sized shapes at pictures of 128 to 512 pixels, written
/// to `specula-shape-sizes.txt` in the temporary folder, or `OUT`, every `STEP` degrees.)
#[test]
#[ignore = "writes the sizes' table"]
fn size_table() {
    let step: u32 = std::env::var("STEP").ok().and_then(|s| s.parse().ok()).unwrap_or(9);
    let mut out = String::new();
    for (name, side, s) in sized(step, &[128, 160, 200, 256, 320, 384, 448, 512]) {
        out += &format!("{name:28} {side:3} lead R/S/P {:3?} none {:3} mean r{:.2} s{:.2} p{:.2} | r {:.2}-{:.2} s {:.2}-{:.2} p {:.2}-{:.2}\n", s.lead, s.none, s.mean[0], s.mean[1], s.mean[2], s.lo[0], s.hi[0], s.lo[1], s.hi[1], s.lo[2], s.hi[2]);
    }
    let path = std::env::var("OUT").map_or_else(|_| std::env::temp_dir().join("specula-shape-sizes.txt"), std::path::PathBuf::from);
    std::fs::write(&path, out).unwrap();
}
