//! Stable 64-bit hashing: the same bytes give the same hash on every platform, build and process.
//!
//! `std`'s `DefaultHasher` is randomly keyed per process, so it can't name content or check that
//! two simulations match. [`StableHasher`] is used for state hashes (`world_hash`, F2), content
//! keys and deterministic hash maps ([`StableBuildHasher`]). It is fast, not cryptographic.

use std::hash::{BuildHasherDefault, Hasher};

const K: u64 = 0x9e37_79b9_7f4a_7c15;

/// A deterministic, unkeyed 64-bit hasher.
///
/// Every `write_*` call is framed by its width, so `(1u32, 2u32)` and `(1u64 << 32 | 2)` hash
/// differently. Floats hash by their exact bits: `0.0` and `-0.0` differ, as they must for a
/// bit-for-bit determinism check.
#[derive(Clone, Copy, Debug)]
pub struct StableHasher {
    state: u64,
    len: u64,
}

impl Default for StableHasher {
    fn default() -> Self {
        StableHasher::new()
    }
}

impl StableHasher {
    pub const fn new() -> StableHasher {
        StableHasher { state: 0x243f_6a88_85a3_08d3, len: 0 }
    }

    /// Starts from `seed`, so separate uses of the hasher don't collide by construction.
    pub const fn with_seed(seed: u64) -> StableHasher {
        StableHasher { state: 0x243f_6a88_85a3_08d3 ^ seed.wrapping_mul(K), len: 0 }
    }

    #[inline]
    fn mix(&mut self, word: u64) {
        self.state = (self.state.rotate_left(23) ^ word).wrapping_mul(0x5851_f42d_4c95_7f2d).wrapping_add(K);
        self.len = self.len.wrapping_add(1);
    }

    pub fn write_u64(&mut self, v: u64) {
        self.mix(v);
    }

    pub fn write_u32(&mut self, v: u32) {
        self.mix(u64::from(v) | 0x4_0000_0000_0000);
    }

    pub fn write_u8(&mut self, v: u8) {
        self.mix(u64::from(v) | 0x1_0000_0000_0000);
    }

    pub fn write_i64(&mut self, v: i64) {
        self.mix(v as u64);
    }

    pub fn write_bool(&mut self, v: bool) {
        self.write_u8(v as u8);
    }

    pub fn write_f32(&mut self, v: f32) {
        self.write_u32(v.to_bits());
    }

    pub fn write_f64(&mut self, v: f64) {
        self.write_u64(v.to_bits());
    }

    /// Length-prefixed, so `("ab", "c")` and `("a", "bc")` differ.
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.mix(bytes.len() as u64 ^ 0xb5_0000_0000_0000);
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            self.mix(u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut last = [0u8; 8];
            last[..rest.len()].copy_from_slice(rest);
            self.mix(u64::from_le_bytes(last));
        }
    }

    pub fn write_str(&mut self, s: &str) {
        self.write_bytes(s.as_bytes());
    }

    /// The hash so far (the hasher can keep going afterwards).
    pub fn finish(&self) -> u64 {
        avalanche(self.state ^ self.len.wrapping_mul(K))
    }
}

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        StableHasher::finish(self)
    }

    fn write(&mut self, bytes: &[u8]) {
        self.write_bytes(bytes);
    }

    fn write_u8(&mut self, v: u8) {
        StableHasher::write_u8(self, v);
    }

    fn write_u32(&mut self, v: u32) {
        StableHasher::write_u32(self, v);
    }

    fn write_u64(&mut self, v: u64) {
        StableHasher::write_u64(self, v);
    }

    fn write_usize(&mut self, v: usize) {
        // Same value on 32- and 64-bit targets.
        StableHasher::write_u64(self, v as u64);
    }
}

/// For `HashMap<K, V, StableBuildHasher>`: unkeyed, so lookups are reproducible. Simulation code
/// must still not depend on a map's *iteration* order (it depends on insertion history); use
/// `BTreeMap` or sort first.
pub type StableBuildHasher = BuildHasherDefault<StableHasher>;

/// splitmix64's finaliser: every input bit affects every output bit.
#[inline]
pub const fn avalanche(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// One-shot hash of a byte string.
pub fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h = StableHasher::new();
    h.write_bytes(bytes);
    h.finish()
}

/// One-shot hash of a string (e.g. a name → an RNG stream id).
pub fn hash_str(s: &str) -> u64 {
    hash_bytes(s.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_values_never_change() {
        // These pin the algorithm: saved hashes and replays depend on them. Never update them
        // without a format version bump.
        assert_eq!(hash_str(""), 0x2a64_eb20_7deb_7ca6);
        assert_eq!(hash_str("specula"), 0xd03c_bb99_20e9_a96c);
        let mut h = StableHasher::new();
        h.write_u32(1);
        h.write_f32(-0.0);
        assert_eq!(h.finish(), 0xb172_2a7c_a3d5_0945);
    }

    #[test]
    fn framing_separates_lookalikes() {
        let mut a = StableHasher::new();
        a.write_str("ab");
        a.write_str("c");
        let mut b = StableHasher::new();
        b.write_str("a");
        b.write_str("bc");
        assert_ne!(a.finish(), b.finish());

        let mut a = StableHasher::new();
        a.write_u32(1);
        a.write_u32(2);
        let mut b = StableHasher::new();
        b.write_u64((1 << 32) | 2);
        assert_ne!(a.finish(), b.finish());

        let mut a = StableHasher::new();
        a.write_f32(0.0);
        let mut b = StableHasher::new();
        b.write_f32(-0.0);
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn seeds_and_lengths_matter() {
        assert_ne!(StableHasher::with_seed(1).finish(), StableHasher::with_seed(2).finish());
        assert_ne!(hash_bytes(&[0]), hash_bytes(&[0, 0]));
        assert_ne!(hash_bytes(&[0; 8]), hash_bytes(&[0; 9]));
    }

    #[test]
    fn spreads_bits() {
        // Neighbouring inputs differ in about half their output bits.
        let mut total = 0;
        for i in 0..1000u32 {
            let a = hash_bytes(&i.to_le_bytes());
            let b = hash_bytes(&(i + 1).to_le_bytes());
            total += (a ^ b).count_ones();
        }
        let mean = total as f64 / 1000.0;
        assert!((28.0..36.0).contains(&mean), "mean differing bits {mean}");
    }

    #[test]
    fn works_as_a_map_hasher() {
        let mut m: std::collections::HashMap<&str, u32, StableBuildHasher> = Default::default();
        m.insert("a", 1);
        assert_eq!(m.get("a"), Some(&1));
    }
}
