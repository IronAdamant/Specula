//! The baked file (`LUX1`): a header, the probes in the GPU's record format, and a hash of it
//! all. The engine stores files by that hash (content-addressed): the world keeps only the hash.

use crate::field::{Field, Lattice, Probe, Record};
use specula_core::hash::StableHasher;

pub const MAGIC: &[u8; 4] = b"LUX1";
pub const VERSION: u32 = 2;
/// Header bytes before the records.
pub const HEADER: usize = 4 + 4 + 12 + 4 + 12 + 4 + 8 + 8 + 24;

/// What a file says besides its probes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    /// Hash of the scene the bake saw (geometry, lights, sky).
    pub scene: u64,
    /// Hash of the request (box, spacing, rays, bounces).
    pub settings: u64,
}

/// Counts kept with the probes (deterministic: what the bake saw and made, so a bake landed from
/// the store records the same as a fresh one).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub valid: u32,
    pub relocated: u32,
    pub invalid: u32,
    pub triangles: u32,
    pub lights: u32,
}

/// A decoded file.
#[derive(Clone, Debug, PartialEq)]
pub struct Decoded {
    pub field: Field,
    pub records: Vec<Record>,
    pub keys: Keys,
    pub counts: Counts,
    pub hash: u64,
}

fn hash(bytes: &[u8]) -> u64 {
    let mut h = StableHasher::with_seed(0x4c55_5831);
    h.write_bytes(bytes);
    h.finish()
}

/// The file for `field` (already quantized: its records are written as they are).
pub fn encode(field: &Field, keys: Keys, counts: Counts) -> (Vec<u8>, u64) {
    let l = &field.lattice;
    let mut b = Vec::with_capacity(HEADER + field.probes.len() * 48 + 8);
    b.extend_from_slice(MAGIC);
    b.extend_from_slice(&VERSION.to_le_bytes());
    for v in l.origin {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&l.spacing.to_le_bytes());
    for d in l.dims {
        b.extend_from_slice(&d.to_le_bytes());
    }
    b.extend_from_slice(&(field.probes.len() as u32).to_le_bytes());
    b.extend_from_slice(&keys.scene.to_le_bytes());
    b.extend_from_slice(&keys.settings.to_le_bytes());
    for c in [counts.valid, counts.relocated, counts.invalid, counts.triangles, counts.lights, 0] {
        b.extend_from_slice(&c.to_le_bytes());
    }
    for p in &field.probes {
        for w in p.pack() {
            b.extend_from_slice(&w.to_le_bytes());
        }
    }
    let h = hash(&b);
    b.extend_from_slice(&h.to_le_bytes());
    (b, h)
}

/// Why a file was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileError(pub &'static str, pub String);

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.0, self.1)
    }
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from(u32_at(b, at)) | (u64::from(u32_at(b, at + 4)) << 32)
}

/// Reads a file, checking its magic, version, sizes, limits and hash.
pub fn decode(b: &[u8]) -> Result<Decoded, FileError> {
    if b.len() < HEADER + 8 || &b[..4] != MAGIC {
        return Err(FileError("not_lux", "not a baked GI file (no LUX1 header)".into()));
    }
    let version = u32_at(b, 4);
    if version != VERSION {
        return Err(FileError("version", format!("file version {version}, this engine reads {VERSION}: rebake")));
    }
    let f = |at: usize| f32::from_bits(u32_at(b, at));
    let origin = [f(8), f(12), f(16)];
    let spacing = f(20);
    let dims = [u32_at(b, 24), u32_at(b, 28), u32_at(b, 32)];
    let count = u32_at(b, 36) as usize;
    let product = dims.iter().try_fold(1usize, |a, &d| a.checked_mul(d as usize));
    // The bake's own limits: at least two probes per axis (the sampler interpolates between
    // two), its spacing range, a finite origin.
    let (lo, hi) = (crate::bake::MIN_SPACING, crate::bake::MAX_SPACING);
    if product != Some(count) || count > crate::bake::MAX_PROBES || dims.iter().any(|&d| d < 2) || !(lo..=hi).contains(&spacing) || origin.iter().any(|v| !v.is_finite()) {
        return Err(FileError("corrupt", format!("bad lattice {dims:?} x {spacing} m for {count} probes")));
    }
    if b.len() != HEADER + count * 48 + 8 {
        return Err(FileError("corrupt", format!("{} bytes, expected {}", b.len(), HEADER + count * 48 + 8)));
    }
    let end = HEADER + count * 48;
    let stored = u64_at(b, end);
    if hash(&b[..end]) != stored {
        return Err(FileError("corrupt", "the hash doesn't match the contents".into()));
    }
    let keys = Keys { scene: u64_at(b, 40), settings: u64_at(b, 48) };
    let counts = Counts { valid: u32_at(b, 56), relocated: u32_at(b, 60), invalid: u32_at(b, 64), triangles: u32_at(b, 68), lights: u32_at(b, 72) };
    let mut records = Vec::with_capacity(count);
    for i in 0..count {
        let mut r = [0u32; 12];
        for (w, slot) in r.iter_mut().enumerate() {
            *slot = u32_at(b, HEADER + i * 48 + w * 4);
        }
        records.push(r);
    }
    let lattice = Lattice { origin, spacing, dims };
    let probes: Vec<Probe> = records.iter().map(Probe::unpack).collect();
    // Finite SH and offsets, weights in [0, 1] (what a bake writes; the shader trusts them).
    for (i, p) in probes.iter().enumerate() {
        let finite = p.sh.c0.iter().chain(p.sh.c1.iter().flatten()).chain(p.offset.iter()).all(|v| v.is_finite());
        if !finite || !(0.0..=1.0).contains(&p.weight) {
            return Err(FileError("corrupt", format!("probe {i} has non-finite values or a weight outside 0..1")));
        }
    }
    let field = Field { lattice, probes };
    Ok(Decoded { field, records, keys, counts, hash: stored })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sh::Sh;

    fn field() -> Field {
        let lattice = Lattice { origin: [-1.5, 0.5, 2.5], spacing: 1.0, dims: [3, 2, 2] };
        let probes = (0..lattice.count())
            .map(|i| Probe { sh: Sh { c0: [i as f32 * 0.1; 3], c1: [[0.01; 3]; 3] }, mask: i as u64 * 0x1_0001, offset: [0.0; 3], weight: 1.0, reach: [(i % 15) as u8, 2, 3, 4, 5, 15, 0, 1, 2, 3, 4, 5, 6, 7] }.quantized())
            .collect();
        Field { lattice, probes }
    }

    #[test]
    fn files_round_trip_and_hash_their_contents() {
        let f = field();
        let keys = Keys { scene: 7, settings: 9 };
        let counts = Counts { valid: 12, relocated: 1, invalid: 0, triangles: 40, lights: 2 };
        let (bytes, h) = encode(&f, keys, counts);
        let d = decode(&bytes).unwrap();
        assert_eq!(d.field, f);
        assert_eq!((d.keys, d.counts, d.hash), (keys, counts, h));
        assert_eq!(encode(&f, keys, counts).1, h, "deterministic");
        assert_ne!(encode(&f, Keys { scene: 8, ..keys }, counts).1, h);
    }

    #[test]
    fn damaged_files_are_refused() {
        let (mut bytes, _) = encode(&field(), Keys::default(), Counts::default());
        assert_eq!(decode(&bytes[..10]).unwrap_err().0, "not_lux");
        let at = HEADER + 5;
        bytes[at] ^= 1;
        assert_eq!(decode(&bytes).unwrap_err().0, "corrupt");
        bytes[at] ^= 1;
        bytes[4] = 9;
        assert_eq!(decode(&bytes).unwrap_err().0, "version");
        bytes[4] = VERSION as u8;
        bytes[24] = 200; // dims no longer match the count
        assert_eq!(decode(&bytes).unwrap_err().0, "corrupt");
    }

    /// A file encoded from `field` with a fixed-up hash: what a damaged or hostile store holds.
    fn forged(field: &Field) -> Vec<u8> {
        encode(field, Keys::default(), Counts::default()).0
    }

    #[test]
    fn files_the_sampler_cant_use_are_refused() {
        // One probe along an axis (the sampler interpolates between two), spacings outside the
        // bake's range, non-finite SH, weights outside 0..1: each refused, none panics later.
        let flat = Field { lattice: Lattice { origin: [0.5; 3], spacing: 1.0, dims: [4, 1, 4] }, probes: vec![Probe { weight: 1.0, ..Probe::default() }; 16] };
        assert_eq!(decode(&forged(&flat)).unwrap_err().0, "corrupt");
        for spacing in [0.1, 9.0, f32::NAN] {
            let mut f = field();
            f.lattice.spacing = spacing;
            assert_eq!(decode(&forged(&f)).unwrap_err().0, "corrupt", "spacing {spacing}");
        }
        let mut nan = field();
        nan.probes[0].sh.c0 = [f32::NAN; 3];
        assert_eq!(decode(&forged(&nan)).unwrap_err().0, "corrupt");
        let mut inf = field();
        inf.probes[1].sh.c1[2][1] = f32::INFINITY;
        assert_eq!(decode(&forged(&inf)).unwrap_err().0, "corrupt");
        let mut heavy = field();
        heavy.probes[2].weight = 2.0;
        assert_eq!(decode(&forged(&heavy)).unwrap_err().0, "corrupt");
        let mut nanw = field();
        nanw.probes[3].weight = f32::NAN;
        assert_eq!(decode(&forged(&nanw)).unwrap_err().0, "corrupt");
    }

    #[test]
    fn random_damage_never_panics() {
        let (good, _) = encode(&field(), Keys::default(), Counts::default());
        let mut seed = 0x1234_5678u64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..5000 {
            let mut b = good.clone();
            for _ in 0..(rnd() % 4 + 1) {
                let at = (rnd() as usize) % b.len();
                b[at] = rnd() as u8;
            }
            if rnd() % 5 == 0 {
                let cut = (rnd() as usize) % b.len();
                b.truncate(cut);
            }
            if let Ok(d) = decode(&b) {
                let _ = d.field.sample(specula_core::math::Vec3::new(0.0, 1.0, 3.0), specula_core::math::Vec3::Y, specula_core::math::Vec3::ZERO, true);
            }
        }
    }
}
