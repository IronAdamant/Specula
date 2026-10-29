//! Dispersed spreads (Design Galleries, Marks et al. 1997): variants as different from each
//! other as the space allows. Candidates are a scrambled Halton set (Cranley-Patterson rotation,
//! seeded); points are chosen farthest-first from what is already chosen or shown.

use specula_core::rng::Rng;

const PRIMES: [u32; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
/// Candidates per point chosen.
const CANDIDATES_PER_POINT: usize = 48;

/// The radical inverse of `i` in base `b` (Halton's sequence).
pub fn radical_inverse(mut i: u64, b: u32) -> f64 {
    let b = u64::from(b);
    let (mut f, mut r) = (1.0f64, 0.0f64);
    while i > 0 {
        f /= b as f64;
        r += f * (i % b) as f64;
        i /= b;
    }
    r
}

/// `n` points of a `dims`-dimensional Halton set in the unit cube, rotated by a seeded shift.
pub fn halton(n: usize, dims: usize, rng: &mut Rng) -> Vec<Vec<f64>> {
    let shift: Vec<f64> = (0..dims).map(|_| f64::from(rng.next_u32()) / 4_294_967_296.0).collect();
    (1..=n as u64)
        .map(|i| (0..dims).map(|d| (radical_inverse(i, PRIMES[d % PRIMES.len()]) + shift[d]) % 1.0).collect())
        .collect()
}

pub fn dist2(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}

/// A region to spread in: the whole cube, or a box of half-width `radius` around `centre`.
#[derive(Clone, Debug)]
pub struct Region<'a> {
    pub centre: Option<&'a [f64]>,
    pub radius: f64,
}

/// `n` points chosen farthest-first. `fixed` points are part of the result first (the start
/// point, an incumbent); `avoid` are points already shown, which new points keep away from.
pub fn dispersed(n: usize, dims: usize, fixed: &[Vec<f64>], avoid: &[Vec<f64>], region: &Region<'_>, rng: &mut Rng) -> Vec<Vec<f64>> {
    dispersed_where(n, dims, fixed, avoid, region, rng, &|p| Some(p))
}

/// [`dispersed`] over candidates mapped by `keep` (a projection onto an allowed set; `None`
/// drops the candidate): chips keep a spread on their side of the incumbent.
#[allow(clippy::type_complexity)]
pub fn dispersed_where(n: usize, dims: usize, fixed: &[Vec<f64>], avoid: &[Vec<f64>], region: &Region<'_>, rng: &mut Rng, keep: &dyn Fn(Vec<f64>) -> Option<Vec<f64>>) -> Vec<Vec<f64>> {
    let mut out: Vec<Vec<f64>> = fixed.iter().take(n).cloned().collect();
    if dims == 0 {
        return out;
    }
    let mut cands = halton(n.max(1) * CANDIDATES_PER_POINT, dims, rng);
    if let Some(c) = region.centre {
        for p in &mut cands {
            for (d, v) in p.iter_mut().enumerate() {
                *v = (c.get(d).copied().unwrap_or(0.5) + (2.0 * *v - 1.0) * region.radius).clamp(0.0, 1.0);
            }
        }
    }
    let cands: Vec<Vec<f64>> = cands.into_iter().filter_map(keep).collect();
    // Nearest distance from each candidate to everything chosen or shown.
    let mut near: Vec<f64> = cands.iter().map(|c| out.iter().chain(avoid).map(|p| dist2(c, p)).fold(f64::INFINITY, f64::min)).collect();
    while out.len() < n {
        // Ties go to the earlier candidate (deterministic); a candidate on top of a chosen one
        // adds nothing.
        let mut best = None;
        for (i, d) in near.iter().enumerate() {
            if *d > 1e-12 && best.is_none_or(|(_, bd)| *d > bd) {
                best = Some((i, *d));
            }
        }
        let Some((i, _)) = best else { break };
        let p = cands[i].clone();
        for (j, c) in cands.iter().enumerate() {
            near[j] = near[j].min(dist2(c, &p));
        }
        near[i] = f64::NEG_INFINITY;
        out.push(p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spreads_are_dispersed_and_seeded() {
        let mut a = Rng::new(7, 1);
        let mut b = Rng::new(7, 1);
        let start = vec![vec![0.5; 3]];
        let p = dispersed(9, 3, &start, &[], &Region { centre: None, radius: 1.0 }, &mut a);
        assert_eq!(p, dispersed(9, 3, &start, &[], &Region { centre: None, radius: 1.0 }, &mut b), "same seed, same spread");
        assert_eq!(p[0], vec![0.5; 3]);
        let min = (0..9).flat_map(|i| (0..9).filter(move |j| *j != i).map(move |j| (i, j))).map(|(i, j)| dist2(&p[i], &p[j])).fold(f64::INFINITY, f64::min);
        // Nine random points in a cube are often within 0.1 of each other; a spread isn't.
        assert!(min.sqrt() > 0.3, "closest pair {}", min.sqrt());
        // Avoided points are kept away from.
        let q = dispersed(4, 3, &[], &p, &Region { centre: None, radius: 1.0 }, &mut a);
        for x in &q {
            assert!(p.iter().all(|y| dist2(x, y) > 0.01));
        }
        // A region keeps points near its centre.
        let c = [0.2, 0.8, 0.5];
        let r = dispersed(6, 3, &[], &[], &Region { centre: Some(&c), radius: 0.1 }, &mut a);
        assert!(r.iter().all(|x| x.iter().zip(c).all(|(v, cv)| (v - cv).abs() <= 0.1 + 1e-12)));
    }
}
