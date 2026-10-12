//! Checksums used by zlib and PNG.

/// CRC-32 (IEEE 802.3, reflected, poly 0xEDB88320) as used by PNG chunks and gzip.
pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0, data)
}

/// Continue a CRC-32 over more data: `crc32_update(crc32(a), b) == crc32(a ++ b)`.
pub fn crc32_update(crc: u32, data: &[u8]) -> u32 {
    let _ = (crc, data);
    todo!("specula-core: crc32_update")
}

/// Adler-32 as used by the zlib wrapper.
pub fn adler32(data: &[u8]) -> u32 {
    let _ = data;
    todo!("specula-core: adler32")
}
