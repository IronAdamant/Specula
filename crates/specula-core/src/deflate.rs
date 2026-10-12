//! DEFLATE (RFC 1951) and the zlib wrapper (RFC 1950).
//!
//! Compression must actually compress (LZ77 + Huffman), not just emit stored blocks: frames sent
//! to agents as base64 PNGs would otherwise be several megabytes each.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Fastest: short match search.
    Fast,
    /// Balanced default.
    Default,
    /// Smallest output: longer match search, dynamic Huffman everywhere.
    Best,
}

/// Raw DEFLATE stream.
pub fn compress(data: &[u8], level: Level) -> Vec<u8> {
    let _ = (data, level);
    todo!("specula-core: deflate::compress")
}

/// Raw DEFLATE stream → bytes. Handles stored, fixed and dynamic Huffman blocks.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, InflateError> {
    let _ = data;
    todo!("specula-core: deflate::decompress")
}

/// zlib stream: 2-byte header, DEFLATE data, Adler-32 trailer.
pub fn zlib_compress(data: &[u8], level: Level) -> Vec<u8> {
    let _ = (data, level);
    todo!("specula-core: deflate::zlib_compress")
}

/// Validates header and Adler-32.
pub fn zlib_decompress(data: &[u8]) -> Result<Vec<u8>, InflateError> {
    let _ = data;
    todo!("specula-core: deflate::zlib_decompress")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InflateError {
    pub message: String,
}

impl std::fmt::Display for InflateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "inflate error: {}", self.message)
    }
}

impl std::error::Error for InflateError {}
