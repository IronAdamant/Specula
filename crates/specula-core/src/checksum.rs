//! Checksums used by zlib and PNG.

/// Reflected CRC-32 polynomial (IEEE 802.3).
const CRC32_POLY: u32 = 0xEDB8_8320;

/// Slicing-by-8 CRC-32 tables, built at compile time: `CRC32_TABLES[0]` is the classic
/// byte-at-a-time table, `CRC32_TABLES[k][n]` is the CRC of byte `n` followed by `k` zero bytes.
static CRC32_TABLES: [[u32; 256]; 8] = build_crc32_tables();

const fn build_crc32_tables() -> [[u32; 256]; 8] {
    let mut t = [[0u32; 256]; 8];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { CRC32_POLY ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        t[0][n] = c;
        n += 1;
    }
    let mut k = 1;
    while k < 8 {
        let mut n = 0;
        while n < 256 {
            let prev = t[k - 1][n];
            t[k][n] = (prev >> 8) ^ t[0][(prev & 0xFF) as usize];
            n += 1;
        }
        k += 1;
    }
    t
}

/// CRC-32 (IEEE 802.3, reflected, poly 0xEDB88320) as used by PNG chunks and gzip.
pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0, data)
}

/// Continue a CRC-32 over more data: `crc32_update(crc32(a), b) == crc32(a ++ b)`.
pub fn crc32_update(crc: u32, data: &[u8]) -> u32 {
    let t = &CRC32_TABLES;
    let mut c = !crc;
    let mut words = data.chunks_exact(8);
    for w in &mut words {
        let v = u64::from_le_bytes(w.try_into().unwrap()) ^ u64::from(c);
        let byte = |i: u32| (v >> (8 * i) & 0xFF) as usize;
        c = t[7][byte(0)]
            ^ t[6][byte(1)]
            ^ t[5][byte(2)]
            ^ t[4][byte(3)]
            ^ t[3][byte(4)]
            ^ t[2][byte(5)]
            ^ t[1][byte(6)]
            ^ t[0][byte(7)];
    }
    for &b in words.remainder() {
        c = t[0][((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

/// Adler-32 modulus (largest prime below 2^16).
const ADLER_MOD: u32 = 65_521;

/// Largest block length for which the Adler-32 sums cannot overflow a `u32` before reduction.
const ADLER_NMAX: usize = 5552;

/// Adler-32 as used by the zlib wrapper.
pub fn adler32(data: &[u8]) -> u32 {
    adler32_update(1, data)
}

/// Continue an Adler-32 over more data: `adler32_update(adler32(a), b) == adler32(a ++ b)`.
pub fn adler32_update(adler: u32, data: &[u8]) -> u32 {
    let mut a = adler & 0xFFFF;
    let mut b = adler >> 16;
    for chunk in data.chunks(ADLER_NMAX) {
        for &byte in chunk {
            a += byte as u32;
            b += a;
        }
        a %= ADLER_MOD;
        b %= ADLER_MOD;
    }
    (b << 16) | a
}

/// Adler-32 of `a ++ b` from `adler32(a)`, `adler32(b)` and `b.len()`, without the data. Lets
/// parallel compressors checksum their bands independently.
pub fn adler32_combine(adler_a: u32, adler_b: u32, len_b: usize) -> u32 {
    let m = u64::from(ADLER_MOD);
    let rem = len_b as u64 % m;
    let (a1, b1) = (u64::from(adler_a & 0xFFFF), u64::from(adler_a >> 16));
    let (a2, b2) = (u64::from(adler_b & 0xFFFF), u64::from(adler_b >> 16));
    // Appending b adds sum(b) = a2 - 1 to A, and to B adds b2 - len_b (b's own running sums,
    // less its initial 1 per byte) plus len_b * a1 (a's final A carried through every byte).
    let a = (a1 + a2 + m - 1) % m;
    let b = (b1 + b2 + rem * a1 % m + m - rem) % m;
    ((b << 16) | a) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_known_vectors() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b"The quick brown fox jumps over the lazy dog"), 0x414F_A339);
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }

    #[test]
    fn crc32_update_is_incremental() {
        let data = b"hello, incremental world";
        for split in 0..=data.len() {
            let (a, b) = data.split_at(split);
            assert_eq!(crc32_update(crc32(a), b), crc32(data));
        }
    }

    #[test]
    fn adler32_known_vectors() {
        assert_eq!(adler32(b""), 1);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        // A long run of 0xFF exercises the deferred modular reduction.
        let big = vec![0xFFu8; 100_000];
        let (mut a, mut b) = (1u64, 0u64);
        for &x in &big {
            a = (a + x as u64) % 65_521;
            b = (b + a) % 65_521;
        }
        assert_eq!(adler32(&big), ((b << 16) | a) as u32);
    }

    #[test]
    fn crc32_slicing_matches_bytewise() {
        let data: Vec<u8> = (0..1000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
        for len in [0, 1, 7, 8, 9, 15, 16, 17, 63, 64, 65, 999, 1000] {
            let mut c = !0u32;
            for &b in &data[..len] {
                c = CRC32_TABLES[0][((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
            }
            assert_eq!(crc32(&data[..len]), !c, "len {len}");
        }
    }

    #[test]
    fn adler32_combine_matches_concatenation() {
        let data: Vec<u8> = (0..300_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 11) as u8).collect();
        for split in [0, 1, 5551, 5552, 65_521, 65_522, 100_000, 299_999, 300_000] {
            let (a, b) = data.split_at(split);
            assert_eq!(adler32_combine(adler32(a), adler32(b), b.len()), adler32(&data), "split {split}");
        }
        let ff = vec![0xFFu8; 200_000];
        let (a, b) = ff.split_at(70_000);
        assert_eq!(adler32_combine(adler32(a), adler32(b), b.len()), adler32(&ff));
    }

    #[test]
    fn adler32_update_is_incremental() {
        let data: Vec<u8> = (0..20_000u32).map(|i| (i * 7 + 3) as u8).collect();
        let (a, b) = data.split_at(12_345);
        assert_eq!(adler32_update(adler32(a), b), adler32(&data));
    }
}
