//! PNG encode (RGBA8) and decode (common colour types → RGBA8).

use crate::deflate::Level;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGBA8, row-major, top row first. `len == width * height * 4`.
    pub rgba: Vec<u8>,
}

/// Encode RGBA8 pixels (top row first). Panics if `rgba.len() != width * height * 4`.
/// Uses adaptive per-row filtering (pick the filter with smallest sum of abs values) and zlib.
pub fn encode_rgba8(width: u32, height: u32, rgba: &[u8], level: Level) -> Vec<u8> {
    let _ = (width, height, rgba, level);
    todo!("specula-core: png::encode_rgba8")
}

/// Decode 8-bit non-interlaced PNGs of colour types 0 (grey), 2 (RGB), 3 (palette, incl. tRNS),
/// 4 (grey+alpha) and 6 (RGBA) to RGBA8. Validates chunk CRCs. Interlaced or 16-bit → error.
pub fn decode(bytes: &[u8]) -> Result<Image, PngError> {
    let _ = bytes;
    todo!("specula-core: png::decode")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PngError {
    pub message: String,
}

impl std::fmt::Display for PngError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PNG error: {}", self.message)
    }
}

impl std::error::Error for PngError {}
