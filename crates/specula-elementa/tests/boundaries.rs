//! What a query or an act reaches is decided by its exact test alone, never by which cells a
//! rounding put a box in: the grid path and a look at every node agree exactly, for acts, the
//! spread query and embers, at cell boundaries and far from the origin; and so a load never
//! changes what an act reaches (the fourth reviewer's `actgrid`: on `b2d675d4` an Ignite reached
//! 0 nodes live and 1 after a load of the same state, and the hashes parted).

mod common;

use common::{mat, sim};
use specula_core::math::Vec3;
use specula_elementa::grid::{box_gap, cell_of, holds, node_cells};
use specula_elementa::sim::{heat_rate, lean, MAX_ACTS_PER_TICK};
use specula_elementa::{Act, ActKind, Elementa, NodeDesc, Phase, Scenario, Target};

/// A small xorshift, so the worlds are the same every run.
struct Gen(u64);

impl Gen {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// `v` moved by `d` units in the last place.
fn ulps(v: f32, d: i32) -> f32 {
    if v == 0.0 {
        return f32::from_bits(d.unsigned_abs()) * if d < 0 { -1.0 } else { 1.0 };
    }
    let b = v.to_bits() as i64 + i64::from(d) * if v < 0.0 { -1 } else { 1 };
    f32::from_bits(b as u32)
}

fn settle(e: &mut Elementa) {
    while !e.step().spread {}
    assert!(e.grid_is_current());
}

/// Steps until a spread step has run, and stops there.
fn spread_once(e: &mut Elementa) {
    while !e.step().spread {}
}

/// Cell boundaries (as cell numbers) for 64 places at least 200 m and 10 cells apart, starting
/// anywhere up to `far_m` from the origin, on one side or the other.
fn places(g: &mut Gen, cell: f32, far_m: f32) -> Vec<f32> {
    let sign = if g.below(2) == 0 { 1.0 } else { -1.0 };
    let start = g.below(far_m as u64 + 1) as f32;
    let apart = (10.0 * cell).max(200.0);
    (0..64).map(|j| sign * (((start + j as f32 * apart) / cell) as u64 as f32)).collect()
}

/// The reviewer's search, widened: spheres whose bottom is a cell boundary (at = k × cell + r),
/// and around each, nodes whose top corner is within four places of the sphere's bottom, and
/// the mirror image above; some worlds lie hundreds of km out.
fn act_world(g: &mut Gen, cell: f32, far_m: f32) -> (Elementa, Vec<(Vec3, f32)>) {
    let json = format!(r#"{{"elements": 1, "cell_m": {cell}, "reach_m": {cell}, "limits": {{"nodes": 8192, "burning": 8192}}}}"#);
    let mut e = sim(&json, 1);
    let wood = mat(&e, "wood");
    let mut d = Vec::new();
    let mut spheres = Vec::new();
    let mut id = 1u64;
    let ks = places(g, cell, far_m);
    for k in ks.into_iter().take(MAX_ACTS_PER_TICK as usize) {
        let r = match g.below(3) {
            0 => g.below(1000) as f32 * 0.001,
            1 => g.below(64_000) as f32 * 0.001,
            _ => cell * (g.below(4) as f32 + 1.0) * 0.25,
        };
        let at = Vec3::new(k * cell + r, 0.5, 0.5);
        spheres.push((at, r));
        for side in [-1.0f32, 1.0] {
            let h = g.below(100_000) as f32 * 0.00001 + 0.0001;
            let c0 = if side < 0.0 { (at.x - r) - h } else { (at.x + r) + h };
            for u in -4..=4 {
                let c = ulps(c0, u);
                d.push(NodeDesc::new(id, wood, Vec3::new(c, 0.5, 0.5), Vec3::new(h, 0.1, 0.1)));
                id += 1;
            }
        }
    }
    e.add_many(&d).unwrap();
    (e, spheres)
}

/// The nodes an act on this sphere reaches by the exact test, looked at one by one.
fn brute(e: &Elementa, at: Vec3, r: f32) -> Vec<u64> {
    e.nodes().iter().filter(|n| box_gap(at, Vec3::ZERO, n.center, n.half) <= r).map(|n| n.id).collect()
}

fn wet_ids(e: &Elementa) -> Vec<u64> {
    e.nodes().iter().filter(|n| n.wet > 0.0).map(|n| n.id).collect()
}

#[test]
fn an_act_on_a_node_touching_its_sphere_at_a_cell_boundary_reaches_it_live_and_after_a_load() {
    // The reviewer's case: cell 2 m, the sphere at x 2.99 with radius 0.99000007, a wood node at
    // x 1.9950299 with half 0.0049699997, touching it; 300 far nodes so the grid is walked.
    let (at, r, c, h) = (2.99f32, 0.990_000_07f32, 1.995_029_9f32, 0.004_969_999_7f32);
    let mk = || {
        let mut e = sim(r#"{"elements": 1, "cell_m": 2, "limits": {"nodes": 4096}}"#, 1);
        let wood = mat(&e, "wood");
        let mut d = vec![NodeDesc::new(1, wood, Vec3::new(c, 0.5, 0.5), Vec3::new(h, 0.1, 0.1))];
        for k in 0..300u64 {
            d.push(NodeDesc::new(10 + k, wood, Vec3::new(500.0 + k as f32 * 3.0, 0.5, 0.5), Vec3::new(0.1, 0.1, 0.1)));
        }
        e.add_many(&d).unwrap();
        for _ in 0..6 {
            e.step();
        }
        e
    };
    let act = Act { kind: ActKind::Ignite, target: Target::Sphere { at: Vec3::new(at, 0.5, 0.5), radius: r }, amount: 1.0 };
    let mut live = mk();
    let mut loaded = Elementa::load(&live.save()).unwrap();
    assert!(live.grid_is_current() && !loaded.grid_is_current());
    assert_eq!(brute(&live, Vec3::new(at, 0.5, 0.5), r), vec![1], "the node touches the sphere");
    let (a, b) = (live.act(act).unwrap(), loaded.act(act).unwrap());
    assert!(!a.scanned && b.scanned && a.work < 300, "live walks the grid, loaded every node: {a:?} {b:?}");
    assert_eq!((a.nodes, b.nodes), (1, 1), "on b2d675d4 live reached 0, loaded 1");
    for _ in 0..60 {
        live.step();
        loaded.step();
    }
    assert_eq!(live.hash(), loaded.hash());
    assert_eq!(live.node(1).unwrap().phase, Phase::Burning);
    // A stale grid (another node moved into other cells) scans; the prediction makes the grid
    // current first and walks it: both reach it, and the prediction equals the run.
    let mut e = mk();
    e.move_node(10, Vec3::new(700.0, 0.5, 0.5), Vec3::new(0.1, 0.1, 0.1)).unwrap();
    let p = e.check(&Scenario { acts: vec![act], wind: None, ambient: None, ticks: 60, max_ops: 1 << 30 }).unwrap();
    assert!(e.act(act).unwrap().scanned);
    for _ in 0..60 {
        e.step();
        e.take_events();
    }
    assert_eq!((p.ignited, p.hash), (1, e.hash()), "the prediction equals the run");
}

#[test]
fn in_random_worlds_an_act_reaches_what_a_look_at_every_node_reaches_live_and_after_a_load() {
    let mut g = Gen(12345);
    let (mut boundary, mut walked) = (0usize, 0usize);
    // Near the origin, and out to 980 km.
    for (cell, far, worlds) in [(2.0f32, 100.0f32, 40), (2.0, 980_000.0, 20), (0.25, 980_000.0, 20), (64.0, 500_000.0, 10)] {
        for _ in 0..worlds {
            let (mut live, spheres) = act_world(&mut g, cell, far);
            settle(&mut live);
            let mut loaded = Elementa::load(&live.save()).unwrap();
            let mut want: Vec<u64> = Vec::new();
            for &(at, r) in &spheres {
                let act = Act { kind: ActKind::Wet, target: Target::Sphere { at, radius: r }, amount: 0.5 };
                let reach = brute(&live, at, r);
                let (a, b) = (live.act(act).unwrap(), loaded.act(act).unwrap());
                assert!(b.scanned && !a.scanned);
                walked += usize::from((a.work as usize) < live.nodes().len());
                assert_eq!((a.nodes as usize, b.nodes as usize), (reach.len(), reach.len()), "cell {cell}: sphere at {at:?} radius {r}");
                // How many of these lie within a rounding of the sphere's bottom cell boundary.
                boundary += reach.len();
                want.extend(reach);
            }
            want.sort_unstable();
            want.dedup();
            assert_eq!(wet_ids(&live), want, "cell {cell}: the grid's act reached other nodes than the exact test");
            assert_eq!(live.save(), loaded.save(), "cell {cell}: a load changed what the acts reached");
        }
    }
    println!("{boundary} nodes reached at boundaries; {walked} acts walked the grid");
    assert!(boundary > 10_000 && walked > 3_000, "{boundary} nodes reached at boundaries; {walked} acts walked the grid");
}

#[test]
fn in_random_worlds_the_spread_query_heats_what_a_look_at_every_node_heats() {
    // Torches whose query box (their box grown by the reach) has its bottom at a cell boundary,
    // and sinks (which never cool, so their heat is what they were given) within four places of
    // it, below and the mirror image above; the torches far apart; the reach no round fraction
    // of the cell (a round one rounds exactly far more often and hides the fault). After the
    // torches' first spread step of heat, every sink's heat is exactly the model's for its gap,
    // or 0 past the reach, by `heat_rate` looked at one by one. On b2d675d4 a sink within the
    // reach whose top corner rounded into the cell below was never heated.
    let mut g = Gen(777);
    let mut heated = 0usize;
    for (cell, far, worlds) in [(2.0f32, 100.0f32, 20), (2.0, 980_000.0, 20), (0.25, 980_000.0, 10), (64.0, 500_000.0, 10)] {
        let json = format!(
            r#"{{"elements": 1, "materials": {{"torch": {{"ignite": 0.1, "flame": 0.9, "burn_s": 3600, "heat_out": 0.5, "cool": 0}},
                "sink": {{"cool": 0}}}}, "cell_m": {cell}, "reach_m": {}, "limits": {{"nodes": 16384, "burning": 8192, "work": 524288}}}}"#,
            cell * 0.4417
        );
        for _ in 0..worlds {
            let mut e = sim(&json, 1);
            let (torch, sink) = (mat(&e, "torch"), mat(&e, "sink"));
            let reach = e.config().reach_m;
            let mut d = Vec::new();
            let mut id = 1u64;
            for k in places(&mut g, cell, far) {
                let ha = g.below(1000) as f32 * 0.001 * cell;
                let a = k * cell + ha + reach + g.below(100) as f32 * 1.0e-6;
                d.push(NodeDesc::new(id, torch, Vec3::new(a, 0.5, 0.5), Vec3::new(ha, 0.1, 0.1)).heat(1.0));
                id += 1;
                for side in [-1.0f32, 1.0] {
                    let hb = g.below(100_000) as f32 * 0.00001 + 0.0001;
                    let c0 = if side < 0.0 { ((a - ha) - reach) - hb } else { ((a + ha) + reach) + hb };
                    for u in -4..=4 {
                        d.push(NodeDesc::new(id, sink, Vec3::new(ulps(c0, u), 0.5, 0.5), Vec3::new(hb, 0.1, 0.1)));
                        id += 1;
                    }
                }
            }
            e.add_many(&d).unwrap();
            spread_once(&mut e); // the torches catch
            spread_once(&mut e); // and give their first heat
            assert_eq!(e.last_report().truncated + e.last_report().deferred, 0, "with room");
            let dt = 1.0 / e.config().spread_hz as f32;
            let heat_out = e.config().materials[usize::from(torch)].heat_out;
            // Each sink's torch is the one added before it.
            let mut source = None;
            for n in e.nodes() {
                if n.material == torch {
                    assert_eq!(n.phase, Phase::Burning);
                    source = Some(n.clone());
                    continue;
                }
                let s = source.as_ref().unwrap();
                let gap = box_gap(s.center, s.half, n.center, n.half);
                let want = if gap <= reach { heat_rate(heat_out, gap, reach, lean(e.wind(), s.center, n.center)) * dt } else { 0.0 };
                heated += usize::from(want > 0.0);
                assert_eq!(n.heat, want, "cell {cell}: sink {} at gap {gap} (reach {reach}) from torch {}", n.id, s.id);
            }
        }
    }
    assert!(heated > 3_000, "{heated} sinks heated at boundaries");
}

#[test]
fn an_ember_finds_in_its_cell_every_node_whose_box_holds_its_point() {
    // An ember looks in the one cell its point is in; a node that holds the point by the
    // corners the grid enters it by always has an entry there.
    let mut g = Gen(4242);
    let mut held = 0u64;
    for (cell, kmax) in [(2.0f32, 1_000u64), (0.25, 4_000_000), (64.0, 15_000)] {
        for _ in 0..200_000 {
            let k = g.below(kmax) as f32;
            let sign = if g.below(2) == 0 { 1.0 } else { -1.0 };
            let p = Vec3::new(sign * k * cell + g.below(3) as f32 * 1.0e-6, 0.5, 0.5);
            let h = g.below(100_000) as f32 * 0.00001 + 0.0001;
            let near = [p.x - h, p.x + h];
            for (c0, u) in near.into_iter().flat_map(|c| (-4..=4).map(move |u| (c, u))) {
                let half = Vec3::new(h, 0.1, 0.1);
                let n = specula_elementa::Node {
                    id: 1,
                    material: 0,
                    center: Vec3::new(ulps(c0, u), 0.5, 0.5),
                    half,
                    heat: 0.0,
                    fuel: 0.0,
                    wet: 0.0,
                    phase: Phase::Intact,
                    since: 0,
                    ready_at: u64::MAX,
                };
                if holds(n.center, n.half, p) {
                    held += 1;
                    let (lo, hi) = node_cells(&n, cell);
                    let c = [cell_of(p.x, cell), cell_of(p.y, cell), cell_of(p.z, cell)];
                    assert!((0..3).all(|a| lo[a] <= c[a] && c[a] <= hi[a]), "cell {cell}: point {p:?} in {c:?}, node {:?} in {lo:?}..{hi:?}", n.center);
                }
            }
        }
    }
    assert!(held > 100_000);
}
