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
