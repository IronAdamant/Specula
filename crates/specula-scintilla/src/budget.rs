//! The frame's particle budget, shared out deterministically (docs/design/VFX.md, "Budgets, LOD
//! and culling"): distance LOD first, then `max_particles` and the CPU work budget by priority
//! and distance, then the fill-rate cap, biggest screen contributors thinned first. Scratch is the caller's and reused.

use specula_core::dmath;

/// The most a thinned emitter's particles grow to keep its weight (the size boost's cap).
pub const MAX_SIZE_BOOST: f32 = 1.5;

/// One visible emitter, as the budget sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub priority: f32,
    /// Metres from the camera.
    pub distance: f32,
    /// Ties: entity id and emitter index, so the order never depends on anything else.
    pub key: u64,
    /// Particles expected alive now (analytic).
    pub expected: f32,
    /// Estimated screen pixels one of its particles covers.
    pub px_per_particle: f32,
    /// CPU work one of its particles costs ([`crate::eval::work_per_particle`]).
    pub work: f32,
    /// Fraction its distance LOD keeps.
    pub lod_keep: f32,
}

/// What one emitter keeps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Share {
    pub keep: f32,
    pub size_boost: f32,
}

/// Totals of one allocation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BudgetStats {
    /// Expected particles before any thinning.
    pub expected: f32,
    /// After LOD, the particle budget and the fill cap.
    pub kept: f32,
    /// Estimated pixels covered after thinning, and before the fill cap.
    pub fill_px: f32,
    pub fill_px_before_cap: f32,
    /// Expected CPU work after thinning.
    pub work: f32,
    /// The particle budget, the work budget or the fill cap cut something.
    pub over_particles: bool,
    pub over_work: bool,
    pub over_fill: bool,
}

/// Distance LOD: 1 up to `start`, falling linearly to `min` at `end`.
pub fn lod_keep(distance: f32, lod: &crate::effect::Lod) -> f32 {
    if distance <= lod.start {
        return 1.0;
    }
    let f = ((distance - lod.start) / (lod.end - lod.start)).clamp(0.0, 1.0);
    1.0 + (lod.min - 1.0) * f
}

/// Shares out `max_particles`, `max_work` (CPU work units) and `fill_cap_px` among `cands` into
/// `out` (one per candidate, in their order). `order` and `values` are scratch.
#[allow(clippy::too_many_arguments)]
pub fn allocate(cands: &[Candidate], max_particles: f32, max_work: f32, fill_cap_px: f32, order: &mut Vec<u32>, values: &mut Vec<f32>, out: &mut Vec<Share>) -> BudgetStats {
    out.clear();
    order.clear();
    let mut stats = BudgetStats::default();
    for c in cands {
        stats.expected += c.expected;
        out.push(Share { keep: c.lod_keep.clamp(0.0, 1.0), size_boost: 1.0 });
    }
    // Most important first: priority, then nearness, then key.
    order.extend(0..cands.len() as u32);
    order.sort_unstable_by(|&a, &b| {
        let (x, y) = (&cands[a as usize], &cands[b as usize]);
        y.priority.total_cmp(&x.priority).then(x.distance.total_cmp(&y.distance)).then(x.key.cmp(&y.key))
    });
    let (mut left, mut work_left) = (max_particles.max(0.0), max_work.max(0.0));
    for &i in order.iter() {
        let (c, s) = (&cands[i as usize], &mut out[i as usize]);
        let want = c.expected * s.keep;
        if want <= left {
            left -= want;
        } else {
            s.keep *= if want > 0.0 { left / want } else { 0.0 };
            left = 0.0;
            stats.over_particles = true;
        }
        // The CPU work budget (noise and `stick` cost more per particle).
        let want = c.expected * s.keep * c.work.max(0.0);
        if want <= work_left {
            work_left -= want;
        } else {
            s.keep *= if want > 0.0 { work_left / want } else { 0.0 };
            work_left = 0.0;
            stats.over_work = true;
        }
    }
    // Survivors of LOD and the particle budget grow a little, so a thinned effect keeps some of
    // its weight (the fill cap below counts their bigger size).
    for s in out.iter_mut() {
        if s.keep > 0.0 && s.keep < 1.0 {
            s.size_boost = dmath::pow(s.keep, -0.25).min(MAX_SIZE_BOOST);
        }
    }
    // The fill cap: water-filling over the contributions, biggest cut first.
    values.clear();
    values.extend(out.iter().zip(cands).map(|(s, c)| c.expected * s.keep * c.px_per_particle.max(0.0) * s.size_boost * s.size_boost));
    let total: f32 = values.iter().sum();
    stats.fill_px_before_cap = total;
    if total > fill_cap_px && fill_cap_px >= 0.0 {
        stats.over_fill = true;
        order.clear();
        order.extend(0..cands.len() as u32);
        order.sort_unstable_by(|&a, &b| values[b as usize].total_cmp(&values[a as usize]).then(cands[a as usize].key.cmp(&cands[b as usize].key)));
        // The cap c with Σ min(v_i, c) = fill_cap_px: the k biggest are capped when
        // c = (cap − Σ the rest) / k lies at or above the next value.
        let n = order.len();
        let (mut prefix, mut cap) = (0.0f32, 0.0f32);
        for k in 1..=n {
            prefix += values[order[k - 1] as usize];
            let c = (fill_cap_px - (total - prefix)) / k as f32;
            let next = if k < n { values[order[k] as usize] } else { 0.0 };
            if c >= next {
                cap = c.max(0.0);
                break;
            }
        }
        for (i, s) in out.iter_mut().enumerate() {
            if values[i] > cap && values[i] > 0.0 {
                s.keep *= cap / values[i];
                values[i] = cap;
            }
        }
    }
    for (i, s) in out.iter().enumerate() {
        stats.kept += cands[i].expected * s.keep;
        stats.work += cands[i].expected * s.keep * cands[i].work.max(0.0);
        stats.fill_px += values[i];
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::Lod;

    fn cand(priority: f32, distance: f32, key: u64, expected: f32, px: f32) -> Candidate {
        Candidate { priority, distance, key, expected, px_per_particle: px, work: 1.0, lod_keep: 1.0 }
    }

    fn run(cands: &[Candidate], max: f32, fill: f32) -> (Vec<Share>, BudgetStats) {
        let (mut order, mut values, mut out) = (Vec::new(), Vec::new(), Vec::new());
        let s = allocate(cands, max, f32::MAX, fill, &mut order, &mut values, &mut out);
        (out, s)
    }

    #[test]
    fn expensive_particles_are_thinned_to_the_work_budget() {
        // Two emitters of 100 particles: one plain (11 units each), one with noise and stick
        // (179 each). 15,000 units can't pay for both: the farther, dearer one thins.
        let mut a = cand(1.0, 5.0, 1, 100.0, 1.0);
        a.work = 11.0;
        let mut b = cand(1.0, 6.0, 2, 100.0, 1.0);
        b.work = 179.0;
        let (mut order, mut values, mut out) = (Vec::new(), Vec::new(), Vec::new());
        let s = allocate(&[a, b], 1.0e6, 15_000.0, 1.0e9, &mut order, &mut values, &mut out);
        assert!(s.over_work && !s.over_particles);
        assert_eq!(out[0].keep, 1.0);
        assert!((out[1].keep - (15_000.0 - 1_100.0) / 17_900.0).abs() < 1e-4, "{:?}", out[1]);
        assert!(s.work <= 15_000.0 + 1e-2);
    }

    #[test]
    fn lod_falls_from_start_to_end() {
        let lod = Lod { start: 10.0, end: 30.0, min: 0.2 };
        assert_eq!(lod_keep(5.0, &lod), 1.0);
        assert!((lod_keep(20.0, &lod) - 0.6).abs() < 1e-6);
        assert!((lod_keep(99.0, &lod) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn the_budget_goes_to_the_important_and_near_first() {
        let cands = [cand(1.0, 50.0, 1, 600.0, 1.0), cand(1.0, 5.0, 2, 600.0, 1.0), cand(5.0, 90.0, 3, 300.0, 1.0)];
        let (out, s) = run(&cands, 1000.0, f32::MAX);
        assert!(s.over_particles);
        assert_eq!(out[2].keep, 1.0, "highest priority, however far");
        assert_eq!(out[1].keep, 1.0, "then the nearest");
        assert!((out[0].keep - 100.0 / 600.0).abs() < 1e-5);
        assert!((s.kept - 1000.0).abs() < 1e-2);
        assert!(out[0].size_boost > 1.0 && out[0].size_boost <= 1.5);
        assert_eq!(run(&cands, 1000.0, f32::MAX).0, out, "the same input, the same shares");
    }

    #[test]
    fn the_fill_cap_thins_the_biggest_contributors() {
        // Fill: 100 × 50 = 5000 px and 100 × 1 = 100 px; cap 2100: the big one goes to 2000.
        let cands = [cand(1.0, 1.0, 1, 100.0, 50.0), cand(1.0, 2.0, 2, 100.0, 1.0)];
        let (out, s) = run(&cands, 1e9, 2100.0);
        assert!(s.over_fill && !s.over_particles);
        assert_eq!(out[1].keep, 1.0, "the small one is untouched");
        assert!((out[0].keep - 0.4).abs() < 1e-4, "{:?}", out[0]);
        assert!((s.fill_px_before_cap - 5100.0).abs() < 1e-2);
        assert!((s.fill_px - 2100.0).abs() < 1e-2);
        // Two equal big ones share the cut.
        let cands = [cand(1.0, 1.0, 1, 100.0, 10.0), cand(1.0, 2.0, 2, 100.0, 10.0)];
        let (out, _) = run(&cands, 1e9, 1000.0);
        assert!((out[0].keep - 0.5).abs() < 1e-5 && (out[1].keep - 0.5).abs() < 1e-5);
    }

    #[test]
    fn nothing_over_budget_changes_nothing() {
        let (out, s) = run(&[cand(1.0, 1.0, 1, 10.0, 4.0)], 100.0, 1000.0);
        assert_eq!(out[0], Share { keep: 1.0, size_boost: 1.0 });
        assert!(!s.over_fill && !s.over_particles);
    }
}
