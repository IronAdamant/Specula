//! JPEG decode (baseline, extended-sequential and progressive Huffman, 8-bit) to RGBA8, plus a
//! small encoder (baseline or progressive) for tests and tools.
//!
//! # Decoder coverage
//!
//! - **Processes:** SOF0 (baseline), SOF1 (extended sequential, Huffman, 8-bit) and SOF2
//!   (progressive, Huffman, spectral selection + successive approximation).
//!   Rejected with [`JpegErrorCode::Unsupported`]: lossless (SOF3), hierarchical (SOF5–7),
//!   arithmetic coding (SOF9–15, DAC), 12-bit samples, DNL-defined heights.
//! - **Colour:** 1 component (greyscale), 3 components (YCbCr; RGB when an Adobe APP14 marker
//!   says transform 0 or the component ids are `'R' 'G' 'B'`), 4 components (Adobe CMYK, stored
//!   inverted as Photoshop writes it, and YCCK). Output is always RGBA8 with alpha 255.
//! - **Sampling:** any factors 1..=4 per component (4:4:4, 4:2:2, 4:2:0, 4:1:1, 4:4:0, ...).
//!   Chroma is upsampled by **replication** (nearest sample): simple and exact to describe.
//! - **Restart markers** (DRI / RST0–7) are checked in sequence.
//! - **IDCT:** the integer "islow" algorithm (Loeffler–Ligtenberg–Moschytz, as in IJG libjpeg's
//!   `jidctint.c`) computed in `i64`; colour conversion is fixed-point integer. Output bits are
//!   identical on every platform (foundation F2).
//! - **EXIF orientation is ignored**: pixels come out in stored order. Callers that care read
//!   the APP1 orientation tag themselves.
//!
//! # Robustness
//!
//! Every read is bounds-checked and every failure is a [`JpegError`] with a code and a byte
//! offset; malformed input never panics. Images over [`Limits::max_pixels`] are refused before
//! anything is allocated, coefficient storage is only allocated once a scan proves (by its
//! remaining length: every DC code is at least one bit) that the data can fill it, and the
//! progressive scan sequence is validated per coefficient (each coefficient gets one first scan
//! and at most 13 refinements), so decoding time is bounded by the image size.

use crate::png::Image;

mod encode;
pub use encode::{encode, encode_baseline, ColorModel, EncodeOptions, Subsampling};

#[cfg(test)]
mod tests;

/// Default pixel limit for [`decode`]: 64 Mpx (8192 × 8192).
pub const DEFAULT_MAX_PIXELS: u64 = 1 << 26;

/// Most scans one image may have (a libjpeg progressive image has 10; a valid sequence can't
/// exceed 4 components × 64 coefficients × 14 passes anyway).
pub const MAX_SCANS: usize = 4096;

/// Natural (row-major) index of each zig-zag position.
pub(crate) const ZIGZAG: [usize; 64] = zigzag();

const fn zigzag() -> [usize; 64] {
    let mut out = [0usize; 64];
    let mut i = 0;
    let mut s = 0usize;
    while s < 15 {
        if s % 2 == 0 {
            // Up and to the right: row decreasing.
            let mut r = if s < 8 { s } else { 7 };
            loop {
                let c = s - r;
                if c > 7 {
                    break;
                }
                out[i] = r * 8 + c;
                i += 1;
                if r == 0 {
                    break;
                }
                r -= 1;
            }
        } else {
            let mut c = if s < 8 { s } else { 7 };
            loop {
                let r = s - c;
                if r > 7 {
                    break;
                }
                out[i] = r * 8 + c;
                i += 1;
                if c == 0 {
                    break;
                }
                c -= 1;
            }
        }
        s += 1;
    }
    out
}

/// What went wrong, for callers to branch on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JpegErrorCode {
    /// The data doesn't start with an SOI marker.
    NotJpeg,
    /// The data ends before the image is complete.
    Truncated,
    /// A marker segment is malformed (bad length or field values, missing tables, wrong order).
    BadSegment,
    /// A Huffman table is invalid, or the entropy-coded data holds a code the table lacks.
    BadHuffman,
    /// Entropy-coded data decodes to impossible values (a coefficient past index 63, a bad
    /// progression, a missing restart marker).
    BadData,
    /// A JPEG feature this decoder doesn't implement.
    Unsupported,
    /// The image exceeds a size limit.
    Limit,
}

/// A decode (or encode) failure: a code, the byte offset where it was found, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JpegError {
    pub code: JpegErrorCode,
    /// Byte offset into the input.
    pub offset: usize,
    pub message: String,
}

impl JpegError {
    fn new(code: JpegErrorCode, offset: usize, message: impl Into<String>) -> JpegError {
        JpegError { code, offset, message: message.into() }
    }
}

impl std::fmt::Display for JpegError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JPEG {:?} at byte {}: {}", self.code, self.offset, self.message)
    }
}

impl std::error::Error for JpegError {}

/// Size limits checked before allocating.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Largest `width × height` accepted.
    pub max_pixels: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { max_pixels: DEFAULT_MAX_PIXELS }
    }
}

/// What the frame header says, read without decoding (see [`info`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Info {
    pub width: u32,
    pub height: u32,
    /// 1 (grey), 3 (YCbCr/RGB) or 4 (CMYK/YCCK) as declared; other counts are reported too.
    pub components: u8,
    pub progressive: bool,
}

/// Reads the frame header (dimensions, components, process) without decoding pixels, so callers
/// can check sizes before committing memory.
pub fn info(bytes: &[u8]) -> Result<Info, JpegError> {
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return Err(JpegError::new(JpegErrorCode::NotJpeg, 0, "missing SOI marker"));
    }
    let mut pos = 2;
    loop {
        let (marker, at) = next_marker(bytes, &mut pos)?;
        match marker {
            0xC0..=0xCF if marker != 0xC4 && marker != 0xC8 && marker != 0xCC => {
                let seg = segment(bytes, &mut pos, at)?;
                if seg.len() < 6 {
                    return Err(JpegError::new(JpegErrorCode::BadSegment, at, "frame header is too short"));
                }
                return Ok(Info {
                    height: u16::from_be_bytes([seg[1], seg[2]]) as u32,
                    width: u16::from_be_bytes([seg[3], seg[4]]) as u32,
                    components: seg[5],
                    progressive: matches!(marker, 0xC2 | 0xC6 | 0xCA | 0xCE),
                });
            }
            0xD8..=0xDA => return Err(JpegError::new(JpegErrorCode::BadSegment, at, format!("marker 0xFF{marker:02X} before the frame header"))),
            0x01 | 0xD0..=0xD7 => {}
            _ => {
                segment(bytes, &mut pos, at)?;
            }
        }
    }
}

/// Decodes a JPEG to RGBA8 with the default [`Limits`].
pub fn decode(bytes: &[u8]) -> Result<Image, JpegError> {
    decode_with_limits(bytes, &Limits::default())
}

/// Decodes a JPEG to RGBA8 (alpha 255), refusing images over `limits`.
pub fn decode_with_limits(bytes: &[u8], limits: &Limits) -> Result<Image, JpegError> {
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return Err(JpegError::new(JpegErrorCode::NotJpeg, 0, "missing SOI marker"));
    }
    let mut d = Decoder {
        data: bytes,
        pos: 2,
        limits: *limits,
        quant: [None; 4],
        dc: [None, None, None, None],
        ac: [None, None, None, None],
        frame: None,
        restart: 0,
        adobe: None,
        jfif: false,
        scans: 0,
    };
    d.run()?;
    d.output()
}

/// Finds the next marker at or after `pos` (skipping 0xFF fill bytes); returns it and its
/// offset, leaving `pos` after the marker code.
fn next_marker(d: &[u8], pos: &mut usize) -> Result<(u8, usize), JpegError> {
    let at = *pos;
    if at >= d.len() {
        return Err(JpegError::new(JpegErrorCode::Truncated, at, "data ends before the EOI marker"));
    }
    if d[at] != 0xFF {
        return Err(JpegError::new(JpegErrorCode::BadSegment, at, format!("expected a marker, found byte 0x{:02X}", d[at])));
    }
    let mut p = at;
    while p < d.len() && d[p] == 0xFF {
        p += 1;
    }
    if p >= d.len() {
        return Err(JpegError::new(JpegErrorCode::Truncated, at, "data ends inside a marker"));
    }
    if d[p] == 0x00 {
        return Err(JpegError::new(JpegErrorCode::BadSegment, at, "stuffed 0xFF00 outside entropy-coded data"));
    }
    *pos = p + 1;
    Ok((d[p], p - 1))
}

/// The payload of the length-prefixed segment at `pos` (after its marker); advances `pos`.
fn segment<'a>(d: &'a [u8], pos: &mut usize, marker_at: usize) -> Result<&'a [u8], JpegError> {
    let p = *pos;
    let Some(len) = d.get(p..p + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize) else {
        return Err(JpegError::new(JpegErrorCode::Truncated, p, "data ends inside a segment length"));
    };
    if len < 2 {
        return Err(JpegError::new(JpegErrorCode::BadSegment, marker_at, format!("segment length {len} is below 2")));
    }
    let Some(seg) = d.get(p + 2..p + len) else {
        return Err(JpegError::new(JpegErrorCode::Truncated, marker_at, format!("segment of {len} bytes runs past the end of the data")));
    };
    *pos = p + len;
    Ok(seg)
}

/// A decoding Huffman table: a 9-bit fast lookup plus canonical-code limits for longer codes.
#[derive(Clone)]
struct Huff {
    /// `(length << 8) | symbol` for codes of up to 9 bits, indexed by the next 9 bits; 0 = slow.
    fast: Box<[u16; 512]>,
    /// Largest code of each length (−1 = none), lengths 0..=16.
    maxcode: [i32; 18],
    /// `index of first symbol of length L − first code of length L`.
    delta: [i32; 17],
    vals: Vec<u8>,
}

const FAST_BITS: u32 = 9;

impl Huff {
    fn build(counts: &[u8; 16], vals: &[u8]) -> Result<Huff, String> {
        let total: usize = counts.iter().map(|&c| c as usize).sum();
        if total > 256 || total != vals.len() {
            return Err(format!("table declares {total} codes (at most 256)"));
        }
        let mut h = Huff { fast: Box::new([0u16; 512]), maxcode: [-1; 18], delta: [0; 17], vals: vals.to_vec() };
        let mut code: u32 = 0;
        let mut k = 0usize;
        for len in 1..=16u32 {
            let n = counts[len as usize - 1] as usize;
            h.delta[len as usize] = k as i32 - code as i32;
            for _ in 0..n {
                if code >= (1 << len) {
                    return Err(format!("code lengths overflow at length {len}"));
                }
                if len <= FAST_BITS {
                    let shift = FAST_BITS - len;
                    let base = (code << shift) as usize;
                    for e in &mut h.fast[base..base + (1 << shift)] {
                        *e = ((len as u16) << 8) | vals[k] as u16;
                    }
                }
                code += 1;
                k += 1;
            }
            if n > 0 {
                h.maxcode[len as usize] = code as i32 - 1;
            }
            if code > (1 << len) {
                return Err(format!("code lengths overflow at length {len}"));
            }
            code <<= 1;
        }
        Ok(h)
    }
}

/// The bit reader over entropy-coded data: un-stuffs 0xFF00 and stops at markers. Bits past
/// the data are zeros that may be peeked but never consumed (that's a truncation error).
struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    acc: u64,
    /// Bits in `acc` (from the top).
    n: u32,
    /// How many of the low `n` bits are padding past the data.
    fake: u32,
    at_marker: bool,
}

impl<'a> Bits<'a> {
    fn new(d: &'a [u8], pos: usize) -> Bits<'a> {
        Bits { d, pos, acc: 0, n: 0, fake: 0, at_marker: false }
    }

    #[inline]
    fn fill(&mut self) {
        while self.n <= 56 {
            let mut byte = 0u8;
            if !self.at_marker {
                match self.d.get(self.pos) {
                    Some(&0xFF) => {
                        if self.d.get(self.pos + 1) == Some(&0x00) {
                            self.pos += 2;
                            byte = 0xFF;
                        } else {
                            self.at_marker = true;
                        }
                    }
                    Some(&b) => {
                        self.pos += 1;
                        byte = b;
                    }
                    None => self.at_marker = true,
                }
            }
            if self.at_marker {
                self.fake += 8;
            }
            self.acc |= (byte as u64) << (56 - self.n);
            self.n += 8;
        }
    }

    #[inline]
    fn peek(&mut self, k: u32) -> u32 {
        if self.n < k {
            self.fill();
        }
        (self.acc >> (64 - k)) as u32
    }

    #[inline]
    fn consume(&mut self, k: u32) -> Result<(), JpegError> {
        if k > self.n - self.fake {
            return Err(JpegError::new(JpegErrorCode::Truncated, self.pos, "entropy-coded data ends inside a block"));
        }
        self.acc <<= k;
        self.n -= k;
        Ok(())
    }

    #[inline]
    fn bits(&mut self, k: u32) -> Result<u32, JpegError> {
        if k == 0 {
            return Ok(0);
        }
        let v = self.peek(k);
        self.consume(k)?;
        Ok(v)
    }

    #[inline]
    fn bit(&mut self) -> Result<bool, JpegError> {
        Ok(self.bits(1)? != 0)
    }

    /// Reads `s` magnitude bits and sign-extends them (JPEG's EXTEND).
    #[inline]
    fn extend(&mut self, s: u32) -> Result<i32, JpegError> {
        if s == 0 {
            return Ok(0);
        }
        if s > 16 {
            return Err(JpegError::new(JpegErrorCode::BadData, self.pos, format!("magnitude category {s} is over 16")));
        }
        let v = self.bits(s)? as i32;
        Ok(if v < (1 << (s - 1)) { v - (1 << s) + 1 } else { v })
    }

    #[inline]
    fn huff(&mut self, h: &Huff) -> Result<u8, JpegError> {
        let p = self.peek(16);
        let f = h.fast[(p >> (16 - FAST_BITS)) as usize];
        if f != 0 {
            self.consume((f >> 8) as u32)?;
            return Ok(f as u8);
        }
        for len in FAST_BITS + 1..=16 {
            let code = (p >> (16 - len)) as i32;
            if code <= h.maxcode[len as usize] {
                self.consume(len)?;
                let i = code + h.delta[len as usize];
                return h.vals.get(i as usize).copied().ok_or_else(|| JpegError::new(JpegErrorCode::BadHuffman, self.pos, "Huffman code index out of range"));
            }
        }
        Err(JpegError::new(JpegErrorCode::BadHuffman, self.pos, "entropy-coded data holds a code the Huffman table doesn't define"))
    }

    /// Drops buffered bits and returns the offset of the next marker (skipping any leftover bytes).
    fn finish(&mut self) -> usize {
        self.acc = 0;
        self.n = 0;
        self.fake = 0;
        self.at_marker = false;
        let mut p = self.pos;
        while p < self.d.len() {
            if self.d[p] == 0xFF && p + 1 < self.d.len() && self.d[p + 1] != 0x00 && self.d[p + 1] != 0xFF {
                break;
            }
            p += 1;
        }
        self.pos = p;
        p
    }
}

struct Comp {
    id: u8,
    h: usize,
    v: usize,
    tq: usize,
    /// Allocated blocks per row / column (the MCU grid).
    bw: usize,
    bh: usize,
    /// Blocks that hold image data (non-interleaved scans cover these).
    cbw: usize,
    cbh: usize,
    /// `bw × bh` blocks of 64 coefficients in natural order; empty until the first scan.
    coeffs: Vec<i16>,
    /// The quantisation table, latched when the component's first scan starts.
    quant: Option<[u16; 64]>,
    /// Progression state per coefficient: −1 never scanned, else the last scan's `Al`.
    coef_bits: [i8; 64],
    dc_pred: i32,
}

struct Frame {
    width: usize,
    height: usize,
    progressive: bool,
    comps: Vec<Comp>,
    hmax: usize,
    vmax: usize,
    mcux: usize,
    mcuy: usize,
}

struct Decoder<'a> {
    data: &'a [u8],
    pos: usize,
    limits: Limits,
    quant: [Option<[u16; 64]>; 4],
    dc: [Option<Huff>; 4],
    ac: [Option<Huff>; 4],
    frame: Option<Frame>,
    restart: u16,
    adobe: Option<u8>,
    jfif: bool,
    scans: usize,
}

/// One scan's parameters.
struct Scan {
    comps: Vec<(usize, usize, usize)>, // (frame component, DC table, AC table)
    ss: usize,
    se: usize,
    ah: u32,
    al: u32,
}

fn bad(at: usize, msg: impl Into<String>) -> JpegError {
    JpegError::new(JpegErrorCode::BadSegment, at, msg)
}

impl Decoder<'_> {
    fn run(&mut self) -> Result<(), JpegError> {
        loop {
            if self.pos >= self.data.len() && self.scans > 0 {
                // A missing EOI after complete scans is tolerated (many writers truncate it).
                return Ok(());
            }
            let (marker, at) = next_marker(self.data, &mut self.pos)?;
            match marker {
                0xD9 => {
                    if self.scans == 0 {
                        return Err(JpegError::new(JpegErrorCode::Truncated, at, "EOI before any scan"));
                    }
                    return Ok(());
                }
                0xD8 => return Err(bad(at, "second SOI marker")),
                0xC0..=0xC2 => {
                    let seg = segment(self.data, &mut self.pos, at)?;
                    self.sof(seg, at, marker == 0xC2)?;
                }
                0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                    let what = match marker {
                        0xC3 => "lossless JPEG (SOF3)",
                        0xC5..=0xC7 => "hierarchical JPEG (SOF5-7)",
                        _ => "arithmetic-coded JPEG (SOF9-15)",
                    };
                    return Err(JpegError::new(JpegErrorCode::Unsupported, at, format!("{what} is not supported")));
                }
                0xCC => return Err(JpegError::new(JpegErrorCode::Unsupported, at, "arithmetic coding (DAC) is not supported")),
                0xC4 => {
                    let seg = segment(self.data, &mut self.pos, at)?;
                    self.dht(seg, at)?;
                }
                0xDB => {
                    let seg = segment(self.data, &mut self.pos, at)?;
                    self.dqt(seg, at)?;
                }
                0xDD => {
                    let seg = segment(self.data, &mut self.pos, at)?;
                    if seg.len() != 2 {
                        return Err(bad(at, format!("DRI payload is {} bytes, expected 2", seg.len())));
                    }
                    self.restart = u16::from_be_bytes([seg[0], seg[1]]);
                }
                0xDA => {
                    let seg = segment(self.data, &mut self.pos, at)?;
                    self.sos(seg, at)?;
                }
                0xDC => return Err(JpegError::new(JpegErrorCode::Unsupported, at, "DNL (height defined after the first scan) is not supported")),
                0xD0..=0xD7 => return Err(JpegError::new(JpegErrorCode::BadData, at, "restart marker outside a scan")),
                0x01 => {}
                0xE0 => {
                    let seg = segment(self.data, &mut self.pos, at)?;
                    if seg.starts_with(b"JFIF\0") {
                        self.jfif = true;
                    }
                }
                0xEE => {
                    let seg = segment(self.data, &mut self.pos, at)?;
                    if seg.len() >= 12 && seg.starts_with(b"Adobe") {
                        self.adobe = Some(seg[11]);
                    }
                }
                _ => {
                    segment(self.data, &mut self.pos, at)?;
                }
            }
        }
    }

    fn sof(&mut self, s: &[u8], at: usize, progressive: bool) -> Result<(), JpegError> {
        if self.frame.is_some() {
            return Err(bad(at, "second frame header (SOF)"));
        }
        if s.len() < 6 {
            return Err(bad(at, "frame header is too short"));
        }
        if s[0] != 8 {
            return Err(JpegError::new(JpegErrorCode::Unsupported, at, format!("{}-bit samples are not supported (8-bit only)", s[0])));
        }
        let height = u16::from_be_bytes([s[1], s[2]]) as usize;
        let width = u16::from_be_bytes([s[3], s[4]]) as usize;
        let n = s[5] as usize;
        if height == 0 {
            return Err(JpegError::new(JpegErrorCode::Unsupported, at, "height 0 (defined by a DNL marker) is not supported"));
        }
        if width == 0 {
            return Err(bad(at, "image width is 0"));
        }
        if !matches!(n, 1 | 3 | 4) {
            return Err(JpegError::new(JpegErrorCode::Unsupported, at, format!("{n} colour components (1, 3 or 4 are supported)")));
        }
        if s.len() != 6 + 3 * n {
            return Err(bad(at, format!("frame header is {} bytes for {n} components, expected {}", s.len(), 6 + 3 * n)));
        }
        let pixels = width as u64 * height as u64;
        if pixels > self.limits.max_pixels {
            return Err(JpegError::new(JpegErrorCode::Limit, at, format!("{width}x{height} = {pixels} pixels exceeds the limit of {}", self.limits.max_pixels)));
        }
        let mut comps = Vec::with_capacity(n);
        for i in 0..n {
            let c = &s[6 + 3 * i..9 + 3 * i];
            let (h, v, tq) = ((c[1] >> 4) as usize, (c[1] & 15) as usize, c[2] as usize);
            if !(1..=4).contains(&h) || !(1..=4).contains(&v) {
                return Err(bad(at, format!("component {i} has sampling factors {h}x{v} (1..=4 allowed)")));
            }
            if tq > 3 {
                return Err(bad(at, format!("component {i} uses quantisation table {tq} (0..=3 allowed)")));
            }
            if comps.iter().any(|o: &Comp| o.id == c[0]) {
                return Err(bad(at, format!("duplicate component id {}", c[0])));
            }
            // A single-component image's MCU is one block whatever its factors say.
            let (h, v) = if n == 1 { (1, 1) } else { (h, v) };
            comps.push(Comp { id: c[0], h, v, tq, bw: 0, bh: 0, cbw: 0, cbh: 0, coeffs: Vec::new(), quant: None, coef_bits: [-1; 64], dc_pred: 0 });
        }
        let hmax = comps.iter().map(|c| c.h).max().unwrap_or(1);
        let vmax = comps.iter().map(|c| c.v).max().unwrap_or(1);
        let mcux = width.div_ceil(8 * hmax);
        let mcuy = height.div_ceil(8 * vmax);
        for c in &mut comps {
            c.bw = mcux * c.h;
            c.bh = mcuy * c.v;
            c.cbw = (width * c.h).div_ceil(hmax).div_ceil(8);
            c.cbh = (height * c.v).div_ceil(vmax).div_ceil(8);
        }
        self.frame = Some(Frame { width, height, progressive, comps, hmax, vmax, mcux, mcuy });
        Ok(())
    }

    fn dht(&mut self, mut s: &[u8], at: usize) -> Result<(), JpegError> {
        while !s.is_empty() {
            if s.len() < 17 {
                return Err(bad(at, "Huffman table segment is truncated"));
            }
            let (class, id) = (s[0] >> 4, (s[0] & 15) as usize);
            if class > 1 || id > 3 {
                return Err(bad(at, format!("Huffman table class {class} id {id} (class 0..=1, id 0..=3)")));
            }
            let counts: [u8; 16] = s[1..17].try_into().expect("16 bytes");
            let total: usize = counts.iter().map(|&c| c as usize).sum();
            let Some(vals) = s.get(17..17 + total) else {
                return Err(bad(at, format!("Huffman table declares {total} symbols past the segment end")));
            };
            let table = Huff::build(&counts, vals).map_err(|m| JpegError::new(JpegErrorCode::BadHuffman, at, m))?;
            if class == 0 {
                self.dc[id] = Some(table);
            } else {
                self.ac[id] = Some(table);
            }
            s = &s[17 + total..];
        }
        Ok(())
    }

    fn dqt(&mut self, mut s: &[u8], at: usize) -> Result<(), JpegError> {
        while !s.is_empty() {
            let (pq, id) = (s[0] >> 4, (s[0] & 15) as usize);
            if pq > 1 || id > 3 {
                return Err(bad(at, format!("quantisation table precision {pq} id {id} (precision 0..=1, id 0..=3)")));
            }
            let size = if pq == 0 { 64 } else { 128 };
            let Some(body) = s.get(1..1 + size) else {
                return Err(bad(at, "quantisation table segment is truncated"));
            };
            let mut q = [0u16; 64];
            for (k, &z) in ZIGZAG.iter().enumerate() {
                q[z] = if pq == 0 { body[k] as u16 } else { u16::from_be_bytes([body[2 * k], body[2 * k + 1]]) };
            }
            self.quant[id] = Some(q);
            s = &s[1 + size..];
        }
        Ok(())
    }

    fn sos(&mut self, s: &[u8], at: usize) -> Result<(), JpegError> {
        let Some(frame) = self.frame.as_mut() else {
            return Err(bad(at, "scan (SOS) before the frame header (SOF)"));
        };
        self.scans += 1;
        if self.scans > MAX_SCANS {
            return Err(JpegError::new(JpegErrorCode::Limit, at, format!("more than {MAX_SCANS} scans")));
        }
        if s.is_empty() {
            return Err(bad(at, "empty scan header"));
        }
        let ns = s[0] as usize;
        if !(1..=4).contains(&ns) || s.len() != 4 + 2 * ns {
            return Err(bad(at, format!("scan header with {ns} components is {} bytes", s.len())));
        }
        let mut comps = Vec::with_capacity(ns);
        for i in 0..ns {
            let (cid, tables) = (s[1 + 2 * i], s[2 + 2 * i]);
            let Some(ci) = frame.comps.iter().position(|c| c.id == cid) else {
                return Err(bad(at, format!("scan names component id {cid}, which the frame lacks")));
            };
            if comps.iter().any(|&(c, _, _)| c == ci) {
                return Err(bad(at, format!("scan names component id {cid} twice")));
            }
            let (td, ta) = ((tables >> 4) as usize, (tables & 15) as usize);
            if td > 3 || ta > 3 {
                return Err(bad(at, format!("scan uses Huffman tables {td}/{ta} (0..=3 allowed)")));
            }
            comps.push((ci, td, ta));
        }
        let tail = &s[1 + 2 * ns..];
        let (ss, se, ah, al) = (tail[0] as usize, tail[1] as usize, (tail[2] >> 4) as u32, (tail[2] & 15) as u32);
        let scan = Scan { comps, ss, se, ah, al };
        // Check the scan against the process and the progression so far.
        if !frame.progressive {
            if ss != 0 || se != 63 || ah != 0 || al != 0 {
                return Err(bad(at, format!("sequential scan with Ss={ss} Se={se} Ah={ah} Al={al} (expected 0, 63, 0, 0)")));
            }
            for &(ci, _, _) in &scan.comps {
                if frame.comps[ci].coef_bits[0] >= 0 {
                    return Err(JpegError::new(JpegErrorCode::BadData, at, format!("component {} is scanned twice in a sequential image", frame.comps[ci].id)));
                }
            }
        } else {
            if ss == 0 && se != 0 {
                return Err(bad(at, format!("progressive DC scan with Se={se} (must be 0)")));
            }
            if ss > 0 && (se < ss || se > 63 || ns != 1) {
                return Err(bad(at, format!("progressive AC scan Ss={ss} Se={se} with {ns} components (needs Ss<=Se<=63, one component)")));
            }
            if al > 13 || ah > 13 {
                return Err(bad(at, format!("successive approximation Ah={ah} Al={al} (0..=13)")));
            }
            for &(ci, _, _) in &scan.comps {
                let c = &frame.comps[ci];
                if ss > 0 && c.coef_bits[0] < 0 {
                    return Err(JpegError::new(JpegErrorCode::BadData, at, format!("AC scan of component {} before its DC scan", c.id)));
                }
                for k in ss..=se {
                    let ok = if ah == 0 { c.coef_bits[k] < 0 } else { c.coef_bits[k] == ah as i8 && al + 1 == ah };
                    if !ok {
                        return Err(JpegError::new(
                            JpegErrorCode::BadData,
                            at,
                            format!("invalid progression for component {} coefficient {k}: Ah={ah} Al={al} after Al={}", c.id, c.coef_bits[k]),
                        ));
                    }
                }
            }
        }
        if ns > 1 {
            let blocks: usize = scan.comps.iter().map(|&(ci, _, _)| frame.comps[ci].h * frame.comps[ci].v).sum();
            if blocks > 10 {
                return Err(bad(at, format!("interleaved MCU of {blocks} blocks (at most 10)")));
            }
        }
        let needs_dc = ss == 0 && ah == 0;
        let needs_ac = ss > 0 || !frame.progressive;
        for &(_, td, ta) in &scan.comps {
            if needs_dc && self.dc[td].is_none() {
                return Err(bad(at, format!("scan uses undefined DC Huffman table {td}")));
            }
            if needs_ac && self.ac[ta].is_none() {
                return Err(bad(at, format!("scan uses undefined AC Huffman table {ta}")));
            }
        }
        // Blocks this scan covers; allocation only happens in DC scans, which spend at least one
        // bit per block, so a short file can't make us allocate a huge image.
        let scan_blocks: usize = if ns > 1 {
            frame.mcux * frame.mcuy * scan.comps.iter().map(|&(ci, _, _)| frame.comps[ci].h * frame.comps[ci].v).sum::<usize>()
        } else {
            let c = &frame.comps[scan.comps[0].0];
            c.cbw * c.cbh
        };
        let remaining = self.data.len() - self.pos;
        if ss == 0 && scan_blocks as u64 > 8 * remaining as u64 {
            return Err(JpegError::new(
                JpegErrorCode::Truncated,
                self.pos,
                format!("scan covers {scan_blocks} blocks but only {remaining} bytes remain (at least one bit per block)"),
            ));
        }
        for &(ci, _, _) in &scan.comps {
            let c = &mut frame.comps[ci];
            if c.quant.is_none() {
                c.quant = Some(self.quant[c.tq].ok_or_else(|| bad(at, format!("component {} uses undefined quantisation table {}", c.id, c.tq)))?);
            }
            if c.coeffs.is_empty() {
                c.coeffs = vec![0i16; c.bw * c.bh * 64];
            }
            for k in ss..=se {
                c.coef_bits[k] = al as i8;
            }
            c.dc_pred = 0;
        }
        let end = decode_scan(self.data, self.pos, frame, &scan, &self.dc, &self.ac, self.restart)?;
        self.pos = end;
        Ok(())
    }

    fn output(self) -> Result<Image, JpegError> {
        let Some(frame) = self.frame else {
            return Err(JpegError::new(JpegErrorCode::Truncated, self.pos, "no frame header"));
        };
        let (w, h) = (frame.width, frame.height);
        // Reconstruct every component plane.
        let mut planes: Vec<(Vec<u8>, usize)> = Vec::with_capacity(frame.comps.len());
        let mut block = [0i16; 64];
        for c in &frame.comps {
            let stride = c.bw * 8;
            let mut plane = vec![128u8; stride * c.bh * 8];
            if let (Some(q), false) = (c.quant, c.coeffs.is_empty()) {
                // Only blocks holding image data need the IDCT.
                for by in 0..c.cbh {
                    for bx in 0..c.cbw {
                        let i = (by * c.bw + bx) * 64;
                        block.copy_from_slice(&c.coeffs[i..i + 64]);
                        idct_block(&block, &q, &mut plane[by * 8 * stride + bx * 8..], stride);
                    }
                }
            }
            planes.push((plane, stride));
        }
        let ncomp = frame.comps.len();
        let transform = match ncomp {
            1 => 0,
            3 => match self.adobe {
                Some(t) => t,
                None if self.jfif => 1,
                None => {
                    let ids: Vec<u8> = frame.comps.iter().map(|c| c.id).collect();
                    u8::from(ids != b"RGB")
                }
            },
            _ => self.adobe.unwrap_or(0),
        };
        let mut rgba = vec![0u8; w * h * 4];
        // Per component: source column of each output column, and row mapping.
        let xmaps: Vec<Vec<usize>> = frame.comps.iter().map(|c| (0..w).map(|x| x * c.h / frame.hmax).collect()).collect();
        let mut row: [&[u8]; 4] = [&[], &[], &[], &[]];
        for y in 0..h {
            for (k, c) in frame.comps.iter().enumerate() {
                let sy = y * c.v / frame.vmax;
                let (p, stride) = &planes[k];
                row[k] = &p[sy * stride..(sy + 1) * stride];
            }
            let out = &mut rgba[y * w * 4..(y + 1) * w * 4];
            match (ncomp, transform) {
                (1, _) => {
                    for (x, px) in out.chunks_exact_mut(4).enumerate() {
                        let g = row[0][x];
                        px.copy_from_slice(&[g, g, g, 255]);
                    }
                }
                (3, 0) => {
                    for (x, px) in out.chunks_exact_mut(4).enumerate() {
                        px.copy_from_slice(&[row[0][xmaps[0][x]], row[1][xmaps[1][x]], row[2][xmaps[2][x]], 255]);
                    }
                }
                (3, _) => {
                    for (x, px) in out.chunks_exact_mut(4).enumerate() {
                        let [r, g, b] = ycc_to_rgb(row[0][xmaps[0][x]], row[1][xmaps[1][x]], row[2][xmaps[2][x]]);
                        px.copy_from_slice(&[r, g, b, 255]);
                    }
                }
                (_, 2) => {
                    for (x, px) in out.chunks_exact_mut(4).enumerate() {
                        let [r, g, b] = ycc_to_rgb(row[0][xmaps[0][x]], row[1][xmaps[1][x]], row[2][xmaps[2][x]]);
                        let k = row[3][xmaps[3][x]];
                        px.copy_from_slice(&[mul8(255 - r, k), mul8(255 - g, k), mul8(255 - b, k), 255]);
                    }
                }
                _ => {
                    for (x, px) in out.chunks_exact_mut(4).enumerate() {
                        let k = row[3][xmaps[3][x]];
                        px.copy_from_slice(&[mul8(row[0][xmaps[0][x]], k), mul8(row[1][xmaps[1][x]], k), mul8(row[2][xmaps[2][x]], k), 255]);
                    }
                }
            }
        }
        Ok(Image { width: w as u32, height: h as u32, rgba })
    }
}

/// `a·b/255`, rounded.
#[inline]
fn mul8(a: u8, b: u8) -> u8 {
    let t = a as u32 * b as u32 + 128;
    ((t + (t >> 8)) >> 8) as u8
}

/// JFIF YCbCr → RGB in 16-bit fixed point (as IJG libjpeg's `jdcolor.c`).
#[inline]
fn ycc_to_rgb(y: u8, cb: u8, cr: u8) -> [u8; 3] {
    let (y, cb, cr) = (y as i32, cb as i32 - 128, cr as i32 - 128);
    let r = y + ((91_881 * cr + 32_768) >> 16);
    let g = y + ((-22_554 * cb - 46_802 * cr + 32_768) >> 16);
    let b = y + ((116_130 * cb + 32_768) >> 16);
    [r.clamp(0, 255) as u8, g.clamp(0, 255) as u8, b.clamp(0, 255) as u8]
}

/// Decodes one scan's entropy-coded data starting at `pos`; returns the offset of the marker
/// that follows it.
fn decode_scan(data: &[u8], pos: usize, frame: &mut Frame, scan: &Scan, dc: &[Option<Huff>; 4], ac: &[Option<Huff>; 4], restart: u16) -> Result<usize, JpegError> {
    let mut bits = Bits::new(data, pos);
    let ns = scan.comps.len();
    let total = if ns > 1 {
        frame.mcux * frame.mcuy
    } else {
        let c = &frame.comps[scan.comps[0].0];
        c.cbw * c.cbh
    };
    let mut eobrun: u32 = 0;
    let mut next_rst = 0u8;
    let progressive = frame.progressive;
    for m in 0..total {
        if restart > 0 && m > 0 && m % restart as usize == 0 {
            let p = bits.finish();
            if p + 1 >= data.len() || data[p + 1] != 0xD0 + next_rst {
                return Err(JpegError::new(JpegErrorCode::BadData, p, format!("expected restart marker RST{next_rst} after {m} MCUs")));
            }
            next_rst = (next_rst + 1) & 7;
            bits = Bits::new(data, p + 2);
            eobrun = 0;
            for &(ci, _, _) in &scan.comps {
                frame.comps[ci].dc_pred = 0;
            }
        }
        if ns > 1 {
            let (mx, my) = (m % frame.mcux, m / frame.mcux);
            for &(ci, td, ta) in &scan.comps {
                let c = &mut frame.comps[ci];
                for by in 0..c.v {
                    for bx in 0..c.h {
                        let b = ((my * c.v + by) * c.bw + mx * c.h + bx) * 64;
                        let blk: &mut [i16; 64] = (&mut c.coeffs[b..b + 64]).try_into().expect("64 coefficients");
                        decode_block(&mut bits, blk, &mut c.dc_pred, &mut eobrun, scan, progressive, dc[td].as_ref(), ac[ta].as_ref())?;
                    }
                }
            }
        } else {
            let (ci, td, ta) = scan.comps[0];
            let c = &mut frame.comps[ci];
            let (bx, by) = (m % c.cbw, m / c.cbw);
            let b = (by * c.bw + bx) * 64;
            let blk: &mut [i16; 64] = (&mut c.coeffs[b..b + 64]).try_into().expect("64 coefficients");
            decode_block(&mut bits, blk, &mut c.dc_pred, &mut eobrun, scan, progressive, dc[td].as_ref(), ac[ta].as_ref())?;
        }
    }
    // Skip restart markers some writers leave after the last MCU.
    let mut p = bits.finish();
    while p + 1 < data.len() && (0xD0..=0xD7).contains(&data[p + 1]) {
        p = Bits::new(data, p + 2).finish();
    }
    Ok(p)
}

#[inline]
fn clamp16(v: i32) -> i16 {
    v.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn decode_block(bits: &mut Bits, blk: &mut [i16; 64], pred: &mut i32, eobrun: &mut u32, scan: &Scan, progressive: bool, dc: Option<&Huff>, ac: Option<&Huff>) -> Result<(), JpegError> {
    let missing = |bits: &Bits| JpegError::new(JpegErrorCode::BadSegment, bits.pos, "missing Huffman table");
    let past = |bits: &Bits, k: usize| JpegError::new(JpegErrorCode::BadData, bits.pos, format!("coefficient index {k} past the end of the band"));
    if !progressive {
        let (dc, ac) = (dc.ok_or_else(|| missing(bits))?, ac.ok_or_else(|| missing(bits))?);
        let t = bits.huff(dc)? as u32;
        *pred = pred.wrapping_add(bits.extend(t)?).clamp(-(1 << 16), 1 << 16);
        blk[0] = clamp16(*pred);
        let mut k = 1;
        while k < 64 {
            let rs = bits.huff(ac)?;
            let (r, s) = ((rs >> 4) as usize, (rs & 15) as u32);
            if s == 0 {
                if r == 15 {
                    k += 16;
                    continue;
                }
                break;
            }
            k += r;
            if k > 63 {
                return Err(past(bits, k));
            }
            blk[ZIGZAG[k]] = clamp16(bits.extend(s)?);
            k += 1;
        }
        return Ok(());
    }
    let (ss, se, ah, al) = (scan.ss, scan.se, scan.ah, scan.al);
    if ss == 0 {
        if ah == 0 {
            let dc = dc.ok_or_else(|| missing(bits))?;
            let t = bits.huff(dc)? as u32;
            *pred = pred.wrapping_add(bits.extend(t)?).clamp(-(1 << 16), 1 << 16);
            blk[0] = clamp16(*pred << al);
        } else if bits.bit()? {
            blk[0] |= 1 << al;
        }
        return Ok(());
    }
    let ac = ac.ok_or_else(|| missing(bits))?;
    if ah == 0 {
        // AC first pass.
        if *eobrun > 0 {
            *eobrun -= 1;
            return Ok(());
        }
        let mut k = ss;
        while k <= se {
            let rs = bits.huff(ac)?;
            let (r, s) = ((rs >> 4) as u32, (rs & 15) as u32);
            if s == 0 {
                if r < 15 {
                    *eobrun = (1 << r) + bits.bits(r)? - 1;
                    break;
                }
                k += 16;
                continue;
            }
            k += r as usize;
            if k > se {
                return Err(past(bits, k));
            }
            blk[ZIGZAG[k]] = clamp16(bits.extend(s)? * (1 << al));
            k += 1;
        }
        return Ok(());
    }
    // AC refinement.
    let p1: i16 = 1 << al;
    let m1: i16 = -p1;
    let mut k = ss;
    let refine = |bits: &mut Bits, c: &mut i16| -> Result<(), JpegError> {
        if bits.bit()? && (*c & p1) == 0 {
            *c = if *c >= 0 { c.saturating_add(p1) } else { c.saturating_add(m1) };
        }
        Ok(())
    };
    if *eobrun == 0 {
        while k <= se {
            let rs = bits.huff(ac)?;
            let (mut r, s) = ((rs >> 4) as i32, (rs & 15) as u32);
            let mut val: i16 = 0;
            if s != 0 {
                if s != 1 {
                    return Err(JpegError::new(JpegErrorCode::BadData, bits.pos, format!("refinement magnitude category {s} (must be 1)")));
                }
                val = if bits.bit()? { p1 } else { m1 };
            } else if r != 15 {
                *eobrun = (1u32 << r) + bits.bits(r as u32)?;
                break;
            }
            loop {
                let c = &mut blk[ZIGZAG[k]];
                if *c != 0 {
                    refine(bits, c)?;
                } else {
                    r -= 1;
                    if r < 0 {
                        break;
                    }
                }
                k += 1;
                if k > se {
                    break;
                }
            }
            if val != 0 {
                if k > se {
                    return Err(past(bits, k));
                }
                blk[ZIGZAG[k]] = val;
            }
            k += 1;
        }
    }
    if *eobrun > 0 {
        while k <= se {
            let c = &mut blk[ZIGZAG[k]];
            if *c != 0 {
                refine(bits, c)?;
            }
            k += 1;
        }
        *eobrun -= 1;
    }
    Ok(())
}

// Integer IDCT constants: round(x · 2^13).
const CONST_BITS: u32 = 13;
const PASS1_BITS: u32 = 2;
const FIX_0_298631336: i64 = 2446;
const FIX_0_390180644: i64 = 3196;
const FIX_0_541196100: i64 = 4433;
const FIX_0_765366865: i64 = 6270;
const FIX_0_899976223: i64 = 7373;
const FIX_1_175875602: i64 = 9633;
const FIX_1_501321110: i64 = 12299;
const FIX_1_847759065: i64 = 15137;
const FIX_1_961570560: i64 = 16069;
const FIX_2_053119869: i64 = 16819;
const FIX_2_562915447: i64 = 20995;
const FIX_3_072711026: i64 = 25172;

#[inline]
fn descale(x: i64, n: u32) -> i64 {
    (x + (1 << (n - 1))) >> n
}

/// One 1-D 8-point IDCT on `v` (even part + odd part), returning outputs before descaling.
#[inline]
fn idct_1d(v: [i64; 8]) -> [i64; 8] {
    // Even part.
    let z1 = (v[2] + v[6]) * FIX_0_541196100;
    let tmp2 = z1 - v[6] * FIX_1_847759065;
    let tmp3 = z1 + v[2] * FIX_0_765366865;
    let tmp0 = (v[0] + v[4]) << CONST_BITS;
    let tmp1 = (v[0] - v[4]) << CONST_BITS;
    let (tmp10, tmp13, tmp11, tmp12) = (tmp0 + tmp3, tmp0 - tmp3, tmp1 + tmp2, tmp1 - tmp2);
    // Odd part.
    let (t0, t1, t2, t3) = (v[7], v[5], v[3], v[1]);
    let (z1, z2, z3, z4) = (t0 + t3, t1 + t2, t0 + t2, t1 + t3);
    let z5 = (z3 + z4) * FIX_1_175875602;
    let (t0, t1, t2, t3) = (t0 * FIX_0_298631336, t1 * FIX_2_053119869, t2 * FIX_3_072711026, t3 * FIX_1_501321110);
    let (z1, z2) = (-z1 * FIX_0_899976223, -z2 * FIX_2_562915447);
    let z3 = -z3 * FIX_1_961570560 + z5;
    let z4 = -z4 * FIX_0_390180644 + z5;
    let (t0, t1, t2, t3) = (t0 + z1 + z3, t1 + z2 + z4, t2 + z2 + z3, t3 + z1 + z4);
    [tmp10 + t3, tmp11 + t2, tmp12 + t1, tmp13 + t0, tmp13 - t0, tmp12 - t1, tmp11 - t2, tmp10 - t3]
}

/// Dequantises and inverse-transforms one block (natural order) into 8 rows of `out`.
fn idct_block(coef: &[i16; 64], q: &[u16; 64], out: &mut [u8], stride: usize) {
    let mut ws = [0i64; 64];
    // Pass 1: columns.
    for col in 0..8 {
        let mut v = [0i64; 8];
        let mut ac = false;
        for (row, x) in v.iter_mut().enumerate() {
            let i = row * 8 + col;
            *x = coef[i] as i64 * q[i] as i64;
            ac |= row > 0 && *x != 0;
        }
        if !ac {
            let dc = v[0] << PASS1_BITS;
            for row in 0..8 {
                ws[row * 8 + col] = dc;
            }
            continue;
        }
        let r = idct_1d(v);
        for row in 0..8 {
            ws[row * 8 + col] = descale(r[row], CONST_BITS - PASS1_BITS);
        }
    }
    // Pass 2: rows.
    for row in 0..8 {
        let v: [i64; 8] = ws[row * 8..row * 8 + 8].try_into().expect("8 values");
        let line = &mut out[row * stride..row * stride + 8];
        if v[1..].iter().all(|&x| x == 0) {
            let p = (descale(v[0], PASS1_BITS + 3) + 128).clamp(0, 255) as u8;
            line.fill(p);
            continue;
        }
        let r = idct_1d(v);
        for (o, x) in line.iter_mut().zip(r) {
            *o = (descale(x, CONST_BITS + PASS1_BITS + 3) + 128).clamp(0, 255) as u8;
        }
    }
}
