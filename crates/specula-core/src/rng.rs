//! The engine's seeded random numbers (foundation F2).
//!
//! PCG32 (O'Neill, `pcg32_random_r`): 64-bit state, 32-bit output, and a selectable stream, so
//! each system draws from its own independent sequence. Same seed + same stream + same calls =
//! same numbers on every platform. Simulation code must use this, never a time- or OS-seeded RNG.
//!
//! ```text
//! use specula_core::rng::Rng;
//! let mut waves = Rng::named(1234, "waves");
//! let roll = waves.below(6);
//! assert!(roll < 6);
//! ```

use crate::hash::hash_str;

const MUL: u64 = 6_364_136_223_846_793_005;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rng {
    state: u64,
    inc: u64,
}

impl Rng {
    /// `pcg32_srandom_r(seed, stream)`.
    pub fn new(seed: u64, stream: u64) -> Rng {
        let mut rng = Rng { state: 0, inc: (stream << 1) | 1 };
        rng.step();
        rng.state = rng.state.wrapping_add(seed);
        rng.step();
        rng
    }

    /// A stream named after the system that owns it (`"waves"`, `"lane_jump"`), so adding a
    /// system never shifts another system's numbers.
    pub fn named(seed: u64, stream: &str) -> Rng {
        Rng::new(seed, hash_str(stream))
    }

    /// A child stream: independent of the parent, and the parent is not advanced.
    pub fn fork(&self, stream: &str) -> Rng {
        Rng::new(self.state ^ hash_str(stream), self.inc ^ hash_str(stream).rotate_left(17))
    }

    #[inline]
    fn step(&mut self) {
        self.state = self.state.wrapping_mul(MUL).wrapping_add(self.inc);
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.step();
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    pub fn next_u64(&mut self) -> u64 {
        (u64::from(self.next_u32()) << 32) | u64::from(self.next_u32())
    }

    /// Uniform in `0..n` without modulo bias (Lemire's method). `n` must be > 0.
    pub fn below(&mut self, n: u32) -> u32 {
        assert!(n > 0, "Rng::below(0)");
        let threshold = n.wrapping_neg() % n;
        loop {
            let m = u64::from(self.next_u32()) * u64::from(n);
            if (m as u32) >= threshold {
                return (m >> 32) as u32;
            }
        }
    }

    /// Uniform in `lo..=hi`.
    pub fn range_i32(&mut self, lo: i32, hi: i32) -> i32 {
        assert!(lo <= hi, "Rng::range_i32({lo}, {hi})");
        let span = (i64::from(hi) - i64::from(lo) + 1) as u64;
        if span > u64::from(u32::MAX) {
            return self.next_u32() as i32;
        }
        (i64::from(lo) + i64::from(self.below(span as u32))) as i32
    }

    /// Uniform in `[0, 1)`, with 24 random bits (every value exactly representable).
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    /// Uniform in `[lo, hi)`.
    pub fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    /// `true` with probability `p` (clamped to 0..1).
    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }

    /// A uniformly chosen element, or `None` when empty.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            return None;
        }
        items.get(self.below(items.len() as u32) as usize)
    }

    /// Fisher–Yates.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u32 + 1) as usize;
            items.swap(i, j);
        }
    }

    /// The raw state, for saves and replays.
    pub fn to_parts(self) -> (u64, u64) {
        (self.state, self.inc)
    }

    /// Restores [`Rng::to_parts`]. The increment is forced odd, as PCG requires.
    pub fn from_parts(state: u64, inc: u64) -> Rng {
        Rng { state, inc: inc | 1 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_pcg32_reference() {
        // pcg32-demo: pcg32_srandom_r(&rng, 42u, 54u), then pcg32_random_r six times.
        let mut r = Rng::new(42, 54);
        let got: Vec<u32> = (0..6).map(|_| r.next_u32()).collect();
        assert_eq!(got, [0xa15c02b7, 0x7b47f409, 0xba1d3330, 0x83d2f293, 0xbfa4784b, 0xcbed606e]);
    }

    #[test]
    fn named_streams_are_independent_and_stable() {
        let mut a = Rng::named(7, "waves");
        let mut b = Rng::named(7, "lane_jump");
        assert_ne!(a.next_u32(), b.next_u32());
        let first: Vec<u32> = (0..4).map(|_| Rng::named(7, "waves").next_u32()).collect();
        assert!(first.iter().all(|&v| v == first[0]));
    }

    #[test]
    fn fork_leaves_the_parent_alone() {
        let parent = Rng::new(1, 2);
        let mut child = parent.fork("x");
        let mut copy = parent;
        assert_ne!(child.next_u32(), copy.next_u32());
        assert_eq!(parent, Rng::new(1, 2));
    }

    #[test]
    fn bounded_values_stay_in_range_and_cover_it() {
        let mut r = Rng::new(3, 3);
        let mut seen = [0u32; 6];
        for _ in 0..6000 {
            seen[r.below(6) as usize] += 1;
        }
        assert!(seen.iter().all(|&n| (800..1200).contains(&n)), "{seen:?}");
        for _ in 0..1000 {
            let v = r.range_i32(-3, 3);
            assert!((-3..=3).contains(&v));
            let f = r.f32();
            assert!((0.0..1.0).contains(&f));
        }
        assert_eq!(r.range_i32(5, 5), 5);
        let full = r.range_i32(i32::MIN, i32::MAX);
        let _ = full;
    }

    #[test]
    fn shuffle_is_a_permutation_and_reproducible() {
        let mut a: Vec<u32> = (0..50).collect();
        let mut b = a.clone();
        Rng::new(9, 1).shuffle(&mut a);
        Rng::new(9, 1).shuffle(&mut b);
        assert_eq!(a, b);
        let mut sorted = a.clone();
        sorted.sort();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        assert_ne!(a, (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn parts_round_trip() {
        let mut r = Rng::named(11, "s");
        r.next_u32();
        let (s, i) = r.to_parts();
        let mut back = Rng::from_parts(s, i);
        assert_eq!(back.next_u64(), r.next_u64());
    }
}
