//! Checksums used by zlib and PNG.

/// Reflected CRC-32 polynomial (IEEE 802.3).
const CRC32_POLY: u32 = 0xEDB8_8320;

/// Byte-at-a-time CRC-32 lookup table, built at compile time.
static CRC32_TABLE: [u32; 256] = build_crc32_table();

const fn build_crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { CRC32_POLY ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}

/// CRC-32 (IEEE 802.3, reflected, poly 0xEDB88320) as used by PNG chunks and gzip.
pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0, data)
}

/// Continue a CRC-32 over more data: `crc32_update(crc32(a), b) == crc32(a ++ b)`.
pub fn crc32_update(crc: u32, data: &[u8]) -> u32 {
    let mut c = !crc;
    for &b in data {
        c = CRC32_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
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
    fn adler32_update_is_incremental() {
        let data: Vec<u8> = (0..20_000u32).map(|i| (i * 7 + 3) as u8).collect();
        let (a, b) = data.split_at(12_345);
        assert_eq!(adler32_update(adler32(a), b), adler32(&data));
    }
}
