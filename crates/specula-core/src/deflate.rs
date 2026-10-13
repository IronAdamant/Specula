//! DEFLATE (RFC 1951) and the zlib wrapper (RFC 1950).
//!
//! Compression must actually compress (LZ77 + Huffman), not just emit stored blocks: frames sent
//! to agents as base64 PNGs would otherwise be several megabytes each.
//!
//! The compressor runs LZ77 over a 32 KiB window with hash chains, then writes each block as
//! whichever of dynamic Huffman, fixed Huffman or stored is smallest. Dynamic codes are
//! length-limited (15 bits for literal/length and distance codes, 7 for code-length codes).
//! The decompressor handles all three block types and rejects malformed input with an
//! [`InflateError`] instead of panicking.
//!
//! **Parallel and deterministic.** Inputs longer than one band ([`BAND_SIZE`] bytes) are cut into
//! fixed-size bands that are compressed on separate threads and stitched pigz-style: every band
//! but the last ends with an empty stored block (a "sync flush") so it finishes on a byte
//! boundary, only the last band sets BFINAL, and each band may still refer back into the previous
//! 32 KiB. The band layout depends only on the input length, never on the thread count, so the
//! output bytes are identical whether one thread or sixty-four run.

use crate::checksum::{adler32, adler32_combine};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Capture speed: no match search, only runs of a repeated byte (distance 1, like zlib's
    /// `Z_RLE`) plus Huffman coding. About 2x as fast as [`Level::Fast`] and 4x as fast as
    /// [`Level::Default`]; on filtered, dithered image data the output is about the same size
    /// (often smaller), because LZ77 finds little there. Poor on text and other data whose
    /// redundancy is in repeated strings.
    Fastest,
    /// Short match search.
    Fast,
    /// Balanced default.
    Default,
    /// Smallest output: longer match search, dynamic Huffman everywhere.
    Best,
}

/// Inputs are compressed in bands of this many bytes, one band per task. Fixed (not derived
/// from the core count) so the output bytes never depend on the machine.
pub const BAND_SIZE: usize = 128 * 1024;

/// Most threads the compressors use by default (the `*_threads` functions accept any count).
pub const MAX_DEFAULT_THREADS: usize = 8;

/// Threads the parallel compressors use by default: all cores but one (left for the render and
/// server threads, so a capture never starves the frame on a low-end laptop), at most
/// [`MAX_DEFAULT_THREADS`], at least 1. The thread count never changes the output bytes.
pub fn available_threads() -> usize {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    cores.saturating_sub(1).clamp(1, MAX_DEFAULT_THREADS)
}

/// Raw DEFLATE stream, using [`available_threads`] threads for large inputs.
pub fn compress(data: &[u8], level: Level) -> Vec<u8> {
    compress_threads(data, level, available_threads())
}

/// [`compress`] with at most `threads` threads (0 or 1 = the calling thread only). The output
/// is byte-identical for every thread count.
pub fn compress_threads(data: &[u8], level: Level, threads: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 2 + 64);
    deflate_into(&mut out, data, level, threads, false);
    out
}

/// Bytes to compress, produced band by band. Lets the PNG encoder filter rows inside the
/// compression workers instead of materialising the whole filtered image first.
pub(crate) trait Source: Sync {
    /// Length of the whole stream.
    fn total_len(&self) -> usize;
    /// Bytes `range` of the stream, borrowed or computed into `scratch`.
    fn bytes<'s>(&'s self, range: std::ops::Range<usize>, scratch: &'s mut Vec<u8>) -> &'s [u8];
}

impl Source for [u8] {
    fn total_len(&self) -> usize {
        self.len()
    }

    fn bytes<'s>(&'s self, range: std::ops::Range<usize>, _: &'s mut Vec<u8>) -> &'s [u8] {
        &self[range]
    }
}

/// Per-worker buffers, reused from band to band so workers do not fault in fresh pages.
#[derive(Default)]
struct Scratch {
    source: Vec<u8>,
    buffers: Buffers,
}

/// A compressed band and the Adler-32 of its input bytes.
type Band = (Vec<u8>, u32);

/// Compress `src` into `out` as raw DEFLATE, in bands on up to `threads` threads. Returns the
/// Adler-32 of the stream when `want_adler` is set (computed band by band on the workers), else 0.
fn deflate_into<S: Source + ?Sized>(out: &mut Vec<u8>, src: &S, level: Level, threads: usize, want_adler: bool) -> u32 {
    let params = level.params();
    let total = src.total_len();
    let bands = total.div_ceil(BAND_SIZE).max(1);
    // History each band sees before its first byte: the LZ77 window, or one byte for runs.
    let history = if params.rle { 1 } else { WINDOW };

    // Band `i` covers `[i * BAND_SIZE, (i + 1) * BAND_SIZE)`. Its compressor also sees up to
    // `history` bytes before that, so matches may reach back across the boundary; positions are
    // relative, so the bytes it emits are the same as if it saw the whole prefix.
    let encode_band = |i: usize, scratch: &mut Scratch| -> Band {
        let start = i * BAND_SIZE;
        let end = (start + BAND_SIZE).min(total);
        let lead = start.min(history);
        let data = src.bytes(start - lead..end, &mut scratch.source);
        let mut w = BitWriter::with_capacity((end - start) / 2 + 64);
        let buffers = std::mem::take(&mut scratch.buffers);
        scratch.buffers = Compressor::new(data, params, buffers).run(&mut w, lead, end == total);
        let adler = if want_adler { adler32(&data[lead..]) } else { 0 };
        (w.finish(), adler)
    };
    let threads = threads.clamp(1, bands);
    let parts: Vec<Band> = if threads == 1 {
        let mut scratch = Scratch::default();
        (0..bands).map(|i| encode_band(i, &mut scratch)).collect()
    } else {
        // Workers pull band indices from a shared counter and file results by index, so
        // scheduling never affects the output.
        let next = AtomicUsize::new(0);
        let slots: Mutex<Vec<Option<Band>>> = Mutex::new((0..bands).map(|_| None).collect());
        let work = || {
            let mut scratch = Scratch::default();
            loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= bands {
                    return;
                }
                let part = encode_band(i, &mut scratch);
                slots.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(part);
            }
        };
        std::thread::scope(|s| fan_out(s, 0, threads, &work));
        let slots = slots.into_inner().unwrap_or_else(|e| e.into_inner());
        slots.into_iter().map(|s| s.expect("every band is compressed exactly once")).collect()
    };

    out.reserve(parts.iter().map(|(p, _)| p.len()).sum());
    let mut adler = 1;
    for (i, (part, band_adler)) in parts.iter().enumerate() {
        out.extend_from_slice(part);
        let len = (total - i * BAND_SIZE).min(BAND_SIZE);
        adler = adler32_combine(adler, *band_adler, len);
    }
    if want_adler {
        adler
    } else {
        0
    }
}

/// Run `work` on up to `threads` threads: thread `k` (the caller is 0) first spawns threads
/// `2k + 1` and `2k + 2`, so start-up is a tree of depth log2(threads) rather than a serial loop
/// on the caller (spawning costs tens of microseconds per thread on Windows). If the OS refuses
/// a thread, that subtree is skipped: `work` pulls from a shared queue, so the threads that did
/// start (at least the caller) finish everything.
fn fan_out<'scope, 'env, F: Fn() + Sync>(
    s: &'scope std::thread::Scope<'scope, 'env>,
    k: usize,
    threads: usize,
    work: &'env F,
) {
    for child in [2 * k + 1, 2 * k + 2] {
        if child < threads {
            let spawned = std::thread::Builder::new().spawn_scoped(s, move || fan_out(s, child, threads, work));
            if spawned.is_err() {
                break;
            }
        }
    }
    work();
}

/// zlib-wrap any [`Source`] (the PNG encoder streams filtered rows through this).
pub(crate) fn zlib_compress_source<S: Source + ?Sized>(src: &S, level: Level, threads: usize) -> Vec<u8> {
    // CMF 0x78 = deflate with a 32 KiB window; FLG carries FLEVEL and makes the header a multiple of 31.
    let flg = match level {
        Level::Fastest | Level::Fast => 0x01,
        Level::Default => 0x9C,
        Level::Best => 0xDA,
    };
    let mut out = Vec::with_capacity(src.total_len() / 2 + 64);
    out.extend_from_slice(&[0x78, flg]);
    let adler = deflate_into(&mut out, src, level, threads, true);
    out.extend_from_slice(&adler.to_be_bytes());
    out
}

/// Output cap of [`decompress`] and [`zlib_decompress`]: a few kilobytes of hostile input can
/// otherwise expand to gigabytes ("zip bomb"). Use the `_with_limit` variants to choose another.
pub const DEFAULT_MAX_OUTPUT: usize = 256 * 1024 * 1024;

/// Raw DEFLATE stream → bytes. Handles stored, fixed and dynamic Huffman blocks.
/// Bytes after the final block are ignored. Output over [`DEFAULT_MAX_OUTPUT`] is an error.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, InflateError> {
    decompress_with_limit(data, DEFAULT_MAX_OUTPUT)
}

/// [`decompress`] with an explicit output cap in bytes (exceeding it is an error).
pub fn decompress_with_limit(data: &[u8], max_out: usize) -> Result<Vec<u8>, InflateError> {
    inflate(data, max_out).map(|(out, _)| out)
}

/// zlib stream: 2-byte header, DEFLATE data, Adler-32 trailer. Uses [`available_threads`]
/// threads for large inputs.
pub fn zlib_compress(data: &[u8], level: Level) -> Vec<u8> {
    zlib_compress_threads(data, level, available_threads())
}

/// [`zlib_compress`] with at most `threads` threads (0 or 1 = the calling thread only). The
/// output is byte-identical for every thread count.
pub fn zlib_compress_threads(data: &[u8], level: Level, threads: usize) -> Vec<u8> {
    zlib_compress_source(data, level, threads)
}

/// Validates header and Adler-32. Output over [`DEFAULT_MAX_OUTPUT`] is an error.
pub fn zlib_decompress(data: &[u8]) -> Result<Vec<u8>, InflateError> {
    zlib_decompress_with_limit(data, DEFAULT_MAX_OUTPUT)
}

/// [`zlib_decompress`] with an explicit output cap in bytes (exceeding it is an error).
pub fn zlib_decompress_with_limit(data: &[u8], max_out: usize) -> Result<Vec<u8>, InflateError> {
    zlib_decompress_limited(data, max_out, false)
}

/// zlib decode with an output cap (exceeding it is an error) and optional tolerance for bytes
/// after the Adler-32 trailer. Used by the PNG decoder, which knows the exact output size.
pub(crate) fn zlib_decompress_limited(
    data: &[u8],
    max_out: usize,
    allow_trailing: bool,
) -> Result<Vec<u8>, InflateError> {
    if data.len() < 2 {
        return Err(InflateError::new("zlib stream shorter than its 2-byte header"));
    }
    let (cmf, flg) = (data[0], data[1]);
    if cmf & 0x0F != 8 {
        return Err(InflateError::new(format!("zlib compression method {} is not deflate (8)", cmf & 0x0F)));
    }
    if cmf >> 4 > 7 {
        return Err(InflateError::new(format!("zlib window size exponent {} exceeds 7", cmf >> 4)));
    }
    if (u16::from(cmf) << 8 | u16::from(flg)) % 31 != 0 {
        return Err(InflateError::new("zlib header check bits are wrong"));
    }
    if flg & 0x20 != 0 {
        return Err(InflateError::new("zlib preset dictionaries are not supported"));
    }
    let (out, used) = inflate(&data[2..], max_out)?;
    let rest = &data[2 + used..];
    if rest.len() < 4 {
        return Err(InflateError::new("zlib stream truncated before Adler-32 trailer"));
    }
    let expected = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]);
    let actual = adler32(&out);
    if expected != actual {
        return Err(InflateError::new(format!(
            "Adler-32 mismatch: stream says {expected:#010x}, data is {actual:#010x}"
        )));
    }
    if rest.len() > 4 && !allow_trailing {
        return Err(InflateError::new(format!("{} unexpected bytes after zlib trailer", rest.len() - 4)));
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InflateError {
    pub message: String,
}

impl InflateError {
    fn new(message: impl Into<String>) -> Self {
        InflateError { message: message.into() }
    }
}

impl std::fmt::Display for InflateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "inflate error: {}", self.message)
    }
}

impl std::error::Error for InflateError {}

// ---------------------------------------------------------------------------------------------
// Shared tables
// ---------------------------------------------------------------------------------------------

/// Window size: the largest back-reference distance.
const WINDOW: usize = 32 * 1024;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
/// End-of-block symbol in the literal/length alphabet.
const END_OF_BLOCK: usize = 256;
/// Literal/length symbols that may appear in a stream (286 and 287 are reserved).
const NUM_LITLEN: usize = 286;
/// Distance symbols that may appear in a stream (30 and 31 are reserved).
const NUM_DIST: usize = 30;
const NUM_CODELEN: usize = 19;
const MAX_CODE_BITS: usize = 15;
const MAX_CODELEN_BITS: usize = 7;

/// Base match length for length symbols 257..=285.
const LENGTH_BASE: [u16; 29] =
    [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
/// Extra bits for length symbols 257..=285.
const LENGTH_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
/// Base distance for distance symbols 0..=29.
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097,
    6145, 8193, 12289, 16385, 24577,
];
/// Extra bits for distance symbols 0..=29.
const DIST_EXTRA: [u8; 30] =
    [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
/// Order in which code-length code lengths are transmitted.
const CODELEN_ORDER: [usize; NUM_CODELEN] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// Match length (3..=258, indexed by `len - 3`) → length symbol index (0..29, i.e. symbol − 257).
static LENGTH_SYMBOL: [u8; 256] = build_length_symbol();

const fn build_length_symbol() -> [u8; 256] {
    let mut table = [0u8; 256];
    let mut sym = 0;
    while sym < 29 {
        let base = LENGTH_BASE[sym] as usize - MIN_MATCH;
        let span = if sym == 28 { 1 } else { 1 << LENGTH_EXTRA[sym] };
        let mut k = 0;
        while k < span && base + k < 256 {
            table[base + k] = sym as u8;
            k += 1;
        }
        sym += 1;
    }
    // Length 258 has its own symbol (285) even though 227 + 31 would also cover it.
    table[255] = 28;
    table
}

/// Distance (1..=32768) → distance symbol (0..=29).
fn dist_symbol(dist: usize) -> usize {
    let d = dist - 1;
    if d < 4 {
        d
    } else {
        let nb = (usize::BITS - 1 - d.leading_zeros()) as usize;
        2 * nb + ((d >> (nb - 1)) & 1)
    }
}

/// Fixed Huffman code lengths for literal/length symbols 0..=287 (RFC 1951 §3.2.6).
fn fixed_litlen_lengths() -> [u8; 288] {
    let mut l = [0u8; 288];
    l[..144].fill(8);
    l[144..256].fill(9);
    l[256..280].fill(7);
    l[280..].fill(8);
    l
}

/// Canonical Huffman codes (already bit-reversed for LSB-first output) for the given lengths.
fn canonical_codes(lengths: &[u8]) -> Vec<u16> {
    let mut count = [0u16; MAX_CODE_BITS + 1];
    for &l in lengths {
        count[l as usize] += 1;
    }
    count[0] = 0;
    let mut next = [0u16; MAX_CODE_BITS + 2];
    let mut code = 0u16;
    for bits in 1..=MAX_CODE_BITS {
        code = (code + count[bits - 1]) << 1;
        next[bits] = code;
    }
    lengths
        .iter()
        .map(|&l| {
            if l == 0 {
                return 0;
            }
            let c = next[l as usize];
            next[l as usize] += 1;
            reverse_bits(c, l)
        })
        .collect()
}

fn reverse_bits(code: u16, len: u8) -> u16 {
    code.reverse_bits() >> (16 - len as u32)
}

// ---------------------------------------------------------------------------------------------
// Inflate
// ---------------------------------------------------------------------------------------------

/// LSB-first bit reader. Reading past the end yields zero bits for peeking, but consuming them
/// is an error, so truncated streams are always reported.
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u64,
    count: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader { data, pos: 0, buf: 0, count: 0 }
    }

    fn refill(&mut self) {
        while self.count <= 56 && self.pos < self.data.len() {
            self.buf |= u64::from(self.data[self.pos]) << self.count;
            self.pos += 1;
            self.count += 8;
        }
    }

    /// Next `n` (≤ 16) bits without consuming them.
    fn peek(&mut self, n: u32) -> u32 {
        if self.count < n {
            self.refill();
        }
        (self.buf & ((1u64 << n) - 1)) as u32
    }

    fn consume(&mut self, n: u32) -> Result<(), InflateError> {
        if n > self.count {
            return Err(InflateError::new("unexpected end of deflate data"));
        }
        self.buf >>= n;
        self.count -= n;
        Ok(())
    }

    fn bits(&mut self, n: u32) -> Result<u32, InflateError> {
        let v = self.peek(n);
        self.consume(n)?;
        Ok(v)
    }

    /// Discard bits up to the next byte boundary and rewind so `pos` is the next unread byte.
    fn align_to_byte(&mut self) {
        let drop = self.count % 8;
        self.buf >>= drop;
        self.count -= drop;
        self.pos -= (self.count / 8) as usize;
        self.buf = 0;
        self.count = 0;
    }

    /// Bytes consumed so far (a partially used final byte counts as consumed).
    fn bytes_used(&self) -> usize {
        self.pos - (self.count / 8) as usize
    }
}

/// Table-driven canonical Huffman decoder: index with the next `bits` input bits.
/// Each entry is `symbol << 4 | code_length`; length 0 marks bit patterns with no code.
struct Decoder {
    table: Vec<u16>,
    bits: u32,
}

impl Decoder {
    fn new(lengths: &[u8], what: &str) -> Result<Decoder, InflateError> {
        let mut count = [0i32; MAX_CODE_BITS + 1];
        for &l in lengths {
            count[l as usize] += 1;
        }
        count[0] = 0;
        let mut left = 1i32;
        for c in &count[1..] {
            left = (left << 1) - c;
            if left < 0 {
                return Err(InflateError::new(format!("over-subscribed {what} Huffman code")));
            }
        }
        let bits = lengths.iter().copied().max().unwrap_or(0).max(1) as u32;
        let mut table = vec![0u16; 1 << bits];
        let codes = canonical_codes(lengths);
        for (sym, (&len, &code)) in lengths.iter().zip(&codes).enumerate() {
            if len == 0 {
                continue;
            }
            let entry = (sym as u16) << 4 | len as u16;
            let mut i = code as usize;
            while i < table.len() {
                table[i] = entry;
                i += 1 << len;
            }
        }
        Ok(Decoder { table, bits })
    }

    fn decode(&self, r: &mut BitReader) -> Result<usize, InflateError> {
        let entry = self.table[r.peek(self.bits) as usize];
        let len = (entry & 15) as u32;
        if len == 0 {
            return Err(InflateError::new("invalid Huffman code in deflate data"));
        }
        r.consume(len)?;
        Ok((entry >> 4) as usize)
    }
}

fn too_large(max_out: usize) -> InflateError {
    InflateError::new(format!("decompressed data exceeds the {max_out}-byte limit"))
}

/// Inflate a raw DEFLATE stream. Returns the output and the number of input bytes used.
/// Output beyond `max_out` bytes is an error.
fn inflate(data: &[u8], max_out: usize) -> Result<(Vec<u8>, usize), InflateError> {
    let mut r = BitReader::new(data);
    let mut out = Vec::with_capacity(data.len().saturating_mul(4).min(max_out).min(1 << 26));
    loop {
        let last = r.bits(1)? == 1;
        match r.bits(2)? {
            0 => {
                r.align_to_byte();
                let p = r.pos;
                if data.len() < p + 4 {
                    return Err(InflateError::new("stored block header truncated"));
                }
                let len = u16::from_le_bytes([data[p], data[p + 1]]) as usize;
                let nlen = u16::from_le_bytes([data[p + 2], data[p + 3]]) as usize;
                if len != !nlen & 0xFFFF {
                    return Err(InflateError::new("stored block length does not match its complement"));
                }
                let body = data
                    .get(p + 4..p + 4 + len)
                    .ok_or_else(|| InflateError::new("stored block truncated"))?;
                if out.len() + len > max_out {
                    return Err(too_large(max_out));
                }
                out.extend_from_slice(body);
                r.pos = p + 4 + len;
            }
            1 => {
                let lit = Decoder::new(&fixed_litlen_lengths(), "fixed literal/length")?;
                let dist = Decoder::new(&[5u8; 32], "fixed distance")?;
                inflate_block(&mut r, &mut out, &lit, &dist, max_out)?;
            }
            2 => {
                let (lit, dist) = read_dynamic_header(&mut r)?;
                inflate_block(&mut r, &mut out, &lit, &dist, max_out)?;
            }
            _ => return Err(InflateError::new("invalid block type 3")),
        }
        if last {
            return Ok((out, r.bytes_used()));
        }
    }
}

fn read_dynamic_header(r: &mut BitReader) -> Result<(Decoder, Decoder), InflateError> {
    let hlit = r.bits(5)? as usize + 257;
    let hdist = r.bits(5)? as usize + 1;
    let hclen = r.bits(4)? as usize + 4;
    if hlit > NUM_LITLEN {
        return Err(InflateError::new(format!("too many literal/length codes ({hlit})")));
    }
    if hdist > NUM_DIST {
        return Err(InflateError::new(format!("too many distance codes ({hdist})")));
    }
    let mut cl_lengths = [0u8; NUM_CODELEN];
    for &sym in &CODELEN_ORDER[..hclen] {
        cl_lengths[sym] = r.bits(3)? as u8;
    }
    let cl = Decoder::new(&cl_lengths, "code-length")?;
    let mut lengths = [0u8; NUM_LITLEN + NUM_DIST];
    let total = hlit + hdist;
    let mut i = 0;
    while i < total {
        let sym = cl.decode(r)?;
        let (value, repeat) = match sym {
            0..=15 => (sym as u8, 1),
            16 => {
                if i == 0 {
                    return Err(InflateError::new("code-length repeat with no previous length"));
                }
                (lengths[i - 1], 3 + r.bits(2)? as usize)
            }
            17 => (0, 3 + r.bits(3)? as usize),
            _ => (0, 11 + r.bits(7)? as usize),
        };
        if i + repeat > total {
            return Err(InflateError::new("code-length repeat runs past the end of the code lengths"));
        }
        lengths[i..i + repeat].fill(value);
        i += repeat;
    }
    if lengths[END_OF_BLOCK] == 0 {
        return Err(InflateError::new("dynamic block has no end-of-block code"));
    }
    let lit = Decoder::new(&lengths[..hlit], "literal/length")?;
    let dist = Decoder::new(&lengths[hlit..total], "distance")?;
    Ok((lit, dist))
}

fn inflate_block(
    r: &mut BitReader,
    out: &mut Vec<u8>,
    lit: &Decoder,
    dist: &Decoder,
    max_out: usize,
) -> Result<(), InflateError> {
    loop {
        let sym = lit.decode(r)?;
        if sym < 256 {
            if out.len() >= max_out {
                return Err(too_large(max_out));
            }
            out.push(sym as u8);
            continue;
        }
        if sym == END_OF_BLOCK {
            return Ok(());
        }
        let li = sym - 257;
        if li >= 29 {
            return Err(InflateError::new(format!("invalid length symbol {sym}")));
        }
        let len = LENGTH_BASE[li] as usize + r.bits(LENGTH_EXTRA[li] as u32)? as usize;
        let ds = dist.decode(r)?;
        if ds >= NUM_DIST {
            return Err(InflateError::new(format!("invalid distance symbol {ds}")));
        }
        let d = DIST_BASE[ds] as usize + r.bits(DIST_EXTRA[ds] as u32)? as usize;
        if d > out.len() {
            return Err(InflateError::new(format!("distance {d} reaches before the start of the output")));
        }
        if out.len() + len > max_out {
            return Err(too_large(max_out));
        }
        let start = out.len() - d;
        if d >= len {
            out.extend_from_within(start..start + len);
        } else {
            // Overlapping copy: the run repeats with period `d`.
            for k in 0..len {
                let b = out[start + k];
                out.push(b);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Deflate
// ---------------------------------------------------------------------------------------------

/// LZ77 search parameters for a [`Level`].
#[derive(Clone, Copy)]
struct Params {
    /// Maximum hash-chain links followed per search.
    max_chain: usize,
    /// Stop searching once a match this long is found.
    nice_len: usize,
    /// Lazy matching: try the next position unless the current match is at least this long (0 = greedy).
    max_lazy: usize,
    /// Quarter the chain length when the pending lazy match is already this long.
    good_len: usize,
    /// Skip the hash chains: only look for runs of the previous byte (distance 1).
    rle: bool,
}

impl Level {
    fn params(self) -> Params {
        let lz = |max_chain, nice_len, max_lazy, good_len| {
            Params { max_chain, nice_len, max_lazy, good_len, rle: false }
        };
        match self {
            Level::Fastest => Params { max_chain: 0, nice_len: MAX_MATCH, max_lazy: 0, good_len: 0, rle: true },
            Level::Fast => lz(8, 32, 0, 4),
            Level::Default => lz(48, 128, 16, 8),
            Level::Best => lz(1024, MAX_MATCH, MAX_MATCH, 32),
        }
    }
}

const HASH_BITS: u32 = 15;
/// Empty hash-chain slot. Chains hold `u32` positions within a band's slice (at most
/// `BAND_SIZE + WINDOW` bytes), which halves the tables a band resets and keeps them in cache.
const NIL: u32 = u32::MAX;
/// Tokens buffered before a block is emitted.
const BLOCK_TOKENS: usize = 1 << 15;
/// Length-3 matches further than this usually cost more bits than three literals.
const TOO_FAR: usize = 4096;

/// One LZ77 output symbol: a literal (`dist == 0`, `value` = byte) or a match (`value` = length).
#[derive(Clone, Copy)]
struct Token {
    value: u16,
    dist: u16,
}

/// LSB-first bit writer.
struct BitWriter {
    out: Vec<u8>,
    buf: u64,
    count: u32,
}

impl BitWriter {
    fn with_capacity(cap: usize) -> Self {
        BitWriter { out: Vec::with_capacity(cap), buf: 0, count: 0 }
    }

    /// Append the low `n` (≤ 16) bits of `value`.
    #[inline]
    fn put(&mut self, value: u32, n: u32) {
        self.buf |= u64::from(value) << self.count;
        self.count += n;
        if self.count >= 32 {
            self.out.extend_from_slice(&(self.buf as u32).to_le_bytes());
            self.buf >>= 32;
            self.count -= 32;
        }
    }

    /// Pad with zero bits to a byte boundary and flush.
    fn align(&mut self) {
        while self.count > 0 {
            self.out.push(self.buf as u8);
            self.buf >>= 8;
            self.count = self.count.saturating_sub(8);
        }
        self.buf = 0;
    }

    fn finish(mut self) -> Vec<u8> {
        self.align();
        self.out
    }
}

/// A compressor's allocations, handed from band to band.
#[derive(Default)]
struct Buffers {
    head: Vec<u32>,
    prev: Vec<u32>,
    tokens: Vec<Token>,
}

struct Compressor<'a> {
    data: &'a [u8],
    p: Params,
    /// Most recent position for each 3-byte hash.
    head: Vec<u32>,
    /// Previous position with the same hash, indexed by `pos % WINDOW`.
    prev: Vec<u32>,
    tokens: Vec<Token>,
    /// Symbol frequencies of the buffered tokens, counted as they are pushed.
    lit_freq: [u32; NUM_LITLEN],
    dist_freq: [u32; NUM_DIST],
    /// Input offset where the buffered tokens start (for stored-block fallback).
    block_start: usize,
}

impl<'a> Compressor<'a> {
    /// A compressor over `data`, reusing (and resetting) `bufs`.
    fn new(data: &'a [u8], p: Params, bufs: Buffers) -> Self {
        let Buffers { mut head, mut prev, mut tokens } = bufs;
        assert!(data.len() < NIL as usize, "deflate band of {} bytes overflows u32 positions", data.len());
        // Run-length mode never consults the hash chains.
        if !p.rle {
            head.clear();
            head.resize(1 << HASH_BITS, NIL);
            prev.clear();
            prev.resize(WINDOW, NIL);
        }
        tokens.clear();
        tokens.reserve(BLOCK_TOKENS + 2);
        Compressor {
            data,
            p,
            head,
            prev,
            tokens,
            lit_freq: [0; NUM_LITLEN],
            dist_freq: [0; NUM_DIST],
            block_start: 0,
        }
    }

    #[inline]
    fn hash(&self, pos: usize) -> usize {
        let d = self.data;
        let v = u32::from(d[pos]) | u32::from(d[pos + 1]) << 8 | u32::from(d[pos + 2]) << 16;
        (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
    }

    #[inline]
    fn insert(&mut self, pos: usize) {
        if pos + MIN_MATCH <= self.data.len() {
            let h = self.hash(pos);
            self.prev[pos % WINDOW] = self.head[h];
            self.head[h] = pos as u32;
        }
    }

    /// Longest match for `pos` (which must already be inserted). Returns `(length, distance)`,
    /// length 0 when nothing worthwhile was found.
    fn longest_match(&self, pos: usize, prev_len: usize) -> (usize, usize) {
        let data = self.data;
        let max_len = MAX_MATCH.min(data.len() - pos);
        if max_len < MIN_MATCH {
            return (0, 0);
        }
        let mut chain = self.p.max_chain;
        if prev_len >= self.p.good_len {
            chain = (chain / 4).max(1);
        }
        let nice = self.p.nice_len.min(max_len);
        let mut best_len = prev_len.max(MIN_MATCH - 1);
        if best_len >= max_len {
            return (0, 0);
        }
        let mut best_dist = 0;
        let mut cand = self.prev[pos % WINDOW] as usize;
        while cand != NIL as usize && cand < pos && chain > 0 {
            let c = cand;
            let dist = pos - c;
            if dist > WINDOW {
                break;
            }
            // Cheap rejection: the byte that would extend the best match must agree.
            if data[c + best_len] == data[pos + best_len] && data[c] == data[pos] {
                let len = match_length(data, c, pos, max_len);
                if len > best_len {
                    best_len = len;
                    best_dist = dist;
                    if len >= nice {
                        break;
                    }
                }
            }
            let next = self.prev[c % WINDOW] as usize;
            if next == NIL as usize || next >= cand {
                break;
            }
            cand = next;
            chain -= 1;
        }
        if best_dist == 0 || (best_len == MIN_MATCH && best_dist > TOO_FAR) {
            (0, 0)
        } else {
            (best_len, best_dist)
        }
    }

    #[inline]
    fn push_literal(&mut self, w: &mut BitWriter, pos: usize) {
        let byte = self.data[pos];
        self.tokens.push(Token { value: u16::from(byte), dist: 0 });
        self.lit_freq[usize::from(byte)] += 1;
        self.maybe_flush(w, pos + 1);
    }

    fn push_match(&mut self, w: &mut BitWriter, pos: usize, len: usize, dist: usize) {
        self.tokens.push(Token { value: len as u16, dist: dist as u16 });
        self.lit_freq[257 + LENGTH_SYMBOL[len - MIN_MATCH] as usize] += 1;
        self.dist_freq[dist_symbol(dist)] += 1;
        // Insert every covered position (the first is already in the hash chains).
        if !self.p.rle {
            for q in pos + 1..pos + len {
                self.insert(q);
            }
        }
        self.maybe_flush(w, pos + len);
    }

    #[inline]
    fn maybe_flush(&mut self, w: &mut BitWriter, end: usize) {
        if self.tokens.len() >= BLOCK_TOKENS {
            self.flush_block(w, end, false);
        }
    }

    /// Compress `data[start..]` (the bytes before `start` are history that matches may refer
    /// to) and end with a final block when `last`, else with a sync flush: an empty stored
    /// block that byte-aligns the output so another stream can be appended.
    fn run(mut self, w: &mut BitWriter, start: usize, last: bool) -> Buffers {
        let n = self.data.len();
        self.block_start = start;
        if self.p.rle {
            self.run_rle(w, start);
        } else {
            for q in start.saturating_sub(WINDOW)..start {
                self.insert(q);
            }
            self.run_lz77(w, start);
        }
        self.flush_block(w, n, last);
        if !last {
            write_stored(w, &[], false);
        }
        Buffers { head: self.head, prev: self.prev, tokens: self.tokens }
    }

    /// Run-length mode: a match is only ever a repeat of the previous byte. Literal stretches
    /// are found first and buffered in bulk, which keeps the hot loops branch-light.
    fn run_rle(&mut self, w: &mut BitWriter, start: usize) {
        let data = self.data;
        let n = data.len();
        // A run (distance-1 match of at least MIN_MATCH) can start at `p`.
        let run_at = |p: usize| p > 0 && p + MIN_MATCH <= n && {
            let b = data[p - 1];
            (data[p] == b) & (data[p + 1] == b) & (data[p + 2] == b)
        };
        let mut pos = start;
        while pos < n {
            // Literals up to the next run, without overfilling the block.
            let limit = n.min(pos + (BLOCK_TOKENS - self.tokens.len()));
            let mut end = pos;
            while end < limit && !run_at(end) {
                end += 1;
            }
            let lits = &data[pos..end];
            self.tokens.extend(lits.iter().map(|&b| Token { value: u16::from(b), dist: 0 }));
            for &b in lits {
                self.lit_freq[usize::from(b)] += 1;
            }
            pos = end;
            self.maybe_flush(w, pos);
            if pos < n && run_at(pos) {
                let len = match_length(data, pos - 1, pos, MAX_MATCH.min(n - pos));
                self.push_match(w, pos, len, 1);
                pos += len;
            }
        }
    }

    /// LZ77 with hash chains and optional lazy matching. Leaves the last block buffered.
    fn run_lz77(&mut self, w: &mut BitWriter, start: usize) {
        let n = self.data.len();
        let lazy = self.p.max_lazy > 0;
        // A match `(start, length, distance)` held back to see whether `start + 1` does better.
        let mut pending: Option<(usize, usize, usize)> = None;
        let mut pos = start;
        while pos < n {
            self.insert(pos);
            let prev_len = pending.map_or(0, |(_, l, _)| l);
            let (len, dist) = self.longest_match(pos, prev_len);
            if let Some((pstart, plen, pdist)) = pending {
                if len > plen {
                    // The later match wins: the held-back position becomes a literal.
                    self.push_literal(w, pstart);
                    if len >= self.p.max_lazy {
                        self.push_match(w, pos, len, dist);
                        pending = None;
                        pos += len;
                    } else {
                        pending = Some((pos, len, dist));
                        pos += 1;
                    }
                } else {
                    self.push_match(w, pstart, plen, pdist);
                    pending = None;
                    pos = pstart + plen;
                }
            } else if len >= MIN_MATCH {
                if lazy && len < self.p.max_lazy {
                    pending = Some((pos, len, dist));
                    pos += 1;
                } else {
                    self.push_match(w, pos, len, dist);
                    pos += len;
                }
            } else {
                self.push_literal(w, pos);
                pos += 1;
            }
        }
        // A pending match always ends at or before `n`, so the loop resolves it; this is a fallback.
        if let Some((pstart, plen, pdist)) = pending {
            self.push_match(w, pstart, plen, pdist);
        }
    }

    /// Emit the buffered tokens (covering input `block_start..end`) as the cheapest block type.
    fn flush_block(&mut self, w: &mut BitWriter, end: usize, last: bool) {
        let mut lit_freq = std::mem::replace(&mut self.lit_freq, [0; NUM_LITLEN]);
        let dist_freq = std::mem::replace(&mut self.dist_freq, [0; NUM_DIST]);
        lit_freq[END_OF_BLOCK] = 1;

        // Extra bits are the same for fixed and dynamic codes.
        let extra_bits: u64 = (0..29).map(|i| u64::from(lit_freq[257 + i]) * u64::from(LENGTH_EXTRA[i])).sum::<u64>()
            + (0..NUM_DIST).map(|i| u64::from(dist_freq[i]) * u64::from(DIST_EXTRA[i])).sum::<u64>();

        let fixed_lit = fixed_litlen_lengths();
        let fixed_bits = 3 + extra_bits + weighted_bits(&lit_freq, &fixed_lit) + weighted_bits(&dist_freq, &[5; 30]);

        let dyn_lit = huffman_lengths(&lit_freq, MAX_CODE_BITS);
        let dyn_dist = huffman_lengths(&dist_freq, MAX_CODE_BITS);
        let header = DynamicHeader::new(&dyn_lit, &dyn_dist);
        let dynamic_bits = 3
            + header.bits()
            + extra_bits
            + weighted_bits(&lit_freq, &dyn_lit)
            + weighted_bits(&dist_freq, &dyn_dist);

        let raw = &self.data[self.block_start..end];
        let chunks = raw.len().div_ceil(0xFFFF).max(1) as u64;
        // Worst-case alignment padding included.
        let stored_bits = chunks * (3 + 7 + 32) + 8 * raw.len() as u64;

        if stored_bits < fixed_bits.min(dynamic_bits) {
            write_stored(w, raw, last);
        } else if fixed_bits <= dynamic_bits {
            w.put(u32::from(last) | 1 << 1, 3);
            let dist_lens = [5u8; 30];
            // Canonical codes depend on every length, so use all 288 fixed lengths.
            self.write_tokens(w, &fixed_lit, &dist_lens);
        } else {
            w.put(u32::from(last) | 2 << 1, 3);
            header.write(w);
            self.write_tokens(w, &dyn_lit, &dyn_dist);
        }
        self.tokens.clear();
        self.block_start = end;
    }

    fn write_tokens(&self, w: &mut BitWriter, lit_lens: &[u8], dist_lens: &[u8]) {
        // `(code, length)` per symbol, so each symbol costs one table load.
        let table = |lens: &[u8]| -> Vec<(u32, u32)> {
            canonical_codes(lens).iter().zip(lens).map(|(&c, &l)| (u32::from(c), u32::from(l))).collect()
        };
        let lit = table(lit_lens);
        let dist = table(dist_lens);
        for t in &self.tokens {
            if t.dist == 0 {
                let (code, len) = lit[t.value as usize];
                w.put(code, len);
            } else {
                let len = t.value as usize;
                let li = LENGTH_SYMBOL[len - MIN_MATCH] as usize;
                let (code, bits) = lit[257 + li];
                w.put(code, bits);
                w.put((len - LENGTH_BASE[li] as usize) as u32, u32::from(LENGTH_EXTRA[li]));
                let d = t.dist as usize;
                let ds = dist_symbol(d);
                let (code, bits) = dist[ds];
                w.put(code, bits);
                w.put((d - DIST_BASE[ds] as usize) as u32, u32::from(DIST_EXTRA[ds]));
            }
        }
        let (code, bits) = lit[END_OF_BLOCK];
        w.put(code, bits);
    }
}

/// Length of the common prefix of `data[a..]` and `data[b..]`, capped at `max` (`b + max <= len`).
#[inline]
fn match_length(data: &[u8], a: usize, b: usize, max: usize) -> usize {
    let mut l = 0;
    while l + 8 <= max {
        let x = u64::from_le_bytes(data[a + l..a + l + 8].try_into().unwrap());
        let y = u64::from_le_bytes(data[b + l..b + l + 8].try_into().unwrap());
        let diff = x ^ y;
        if diff != 0 {
            return l + (diff.trailing_zeros() / 8) as usize;
        }
        l += 8;
    }
    while l < max && data[a + l] == data[b + l] {
        l += 1;
    }
    l
}

fn write_stored(w: &mut BitWriter, raw: &[u8], last: bool) {
    let mut chunks: Vec<&[u8]> = raw.chunks(0xFFFF).collect();
    if chunks.is_empty() {
        chunks.push(&[]);
    }
    let n = chunks.len();
    for (i, chunk) in chunks.into_iter().enumerate() {
        w.put(u32::from(last && i + 1 == n), 3);
        w.align();
        let len = chunk.len() as u16;
        w.out.extend_from_slice(&len.to_le_bytes());
        w.out.extend_from_slice(&(!len).to_le_bytes());
        w.out.extend_from_slice(chunk);
    }
}

fn weighted_bits(freq: &[u32], lengths: &[u8]) -> u64 {
    freq.iter().zip(lengths).map(|(&f, &l)| u64::from(f) * u64::from(l)).sum()
}

/// Length-limited Huffman code lengths for `freq`. Always yields a complete code with at least
/// two symbols (unused symbols are given codes if needed) so every decoder accepts it.
fn huffman_lengths(freq: &[u32], limit: usize) -> Vec<u8> {
    let mut freq = freq.to_vec();
    let mut used = freq.iter().filter(|&&f| f > 0).count();
    for f in freq.iter_mut() {
        if used >= 2 {
            break;
        }
        if *f == 0 {
            *f = 1;
            used += 1;
        }
    }
    let mut syms: Vec<usize> = (0..freq.len()).filter(|&s| freq[s] > 0).collect();

    // Plain Huffman tree: leaves are 0..m, internal nodes m.. (each parent id > its children's ids).
    let m = syms.len();
    let mut parent = vec![0usize; 2 * m - 1];
    let mut heap: std::collections::BinaryHeap<std::cmp::Reverse<(u64, usize)>> =
        syms.iter().enumerate().map(|(i, &s)| std::cmp::Reverse((u64::from(freq[s]), i))).collect();
    let mut next = m;
    while heap.len() > 1 {
        let std::cmp::Reverse((fa, a)) = heap.pop().unwrap();
        let std::cmp::Reverse((fb, b)) = heap.pop().unwrap();
        parent[a] = next;
        parent[b] = next;
        heap.push(std::cmp::Reverse((fa + fb, next)));
        next += 1;
    }
    let root = next - 1;
    let mut depth = vec![0usize; 2 * m - 1];
    for node in (0..root).rev() {
        depth[node] = depth[parent[node]] + 1;
    }

    // Count codes per length, folding anything longer than `limit` back into range while
    // keeping the Kraft sum exactly 1.
    let max_depth = depth[..m].iter().copied().max().unwrap_or(0);
    let mut count = vec![0usize; max_depth.max(limit) + 1];
    for &d in &depth[..m] {
        count[d] += 1;
    }
    if max_depth > limit {
        let overflow: usize = count[limit + 1..].iter().sum();
        count[limit] += overflow;
        count.truncate(limit + 1);
        let mut total: usize = (1..=limit).map(|i| count[i] << (limit - i)).sum();
        while total > 1 << limit {
            count[limit] -= 1;
            for i in (1..limit).rev() {
                if count[i] > 0 {
                    count[i] -= 1;
                    count[i + 1] += 2;
                    break;
                }
            }
            total -= 1;
        }
    }

    // Longest codes go to the rarest symbols.
    syms.sort_by_key(|&s| (freq[s], s));
    let mut lengths = vec![0u8; freq.len()];
    let mut it = syms.into_iter();
    for len in (1..count.len()).rev() {
        for _ in 0..count[len] {
            if let Some(s) = it.next() {
                lengths[s] = len as u8;
            }
        }
    }
    lengths
}

/// Run-length-encoded code lengths and the code-length code for a dynamic block header.
struct DynamicHeader {
    hlit: usize,
    hdist: usize,
    hclen: usize,
    /// `(symbol 0..=18, extra bits value)`.
    rle: Vec<(u8, u8)>,
    cl_lengths: Vec<u8>,
}

impl DynamicHeader {
    fn new(lit: &[u8], dist: &[u8]) -> Self {
        let hlit = lit.iter().rposition(|&l| l != 0).map_or(0, |p| p + 1).max(257);
        let hdist = dist.iter().rposition(|&l| l != 0).map_or(0, |p| p + 1).max(1);
        let all: Vec<u8> = lit[..hlit].iter().chain(&dist[..hdist]).copied().collect();
        let rle = rle_code_lengths(&all);
        let mut freq = [0u32; NUM_CODELEN];
        for &(s, _) in &rle {
            freq[s as usize] += 1;
        }
        let cl_lengths = huffman_lengths(&freq, MAX_CODELEN_BITS);
        let hclen = CODELEN_ORDER.iter().rposition(|&s| cl_lengths[s] != 0).map_or(0, |p| p + 1).max(4);
        DynamicHeader { hlit, hdist, hclen, rle, cl_lengths }
    }

    fn bits(&self) -> u64 {
        let body: u64 = self
            .rle
            .iter()
            .map(|&(s, _)| u64::from(self.cl_lengths[s as usize]) + rle_extra_bits(s) as u64)
            .sum();
        14 + 3 * self.hclen as u64 + body
    }

    fn write(&self, w: &mut BitWriter) {
        w.put((self.hlit - 257) as u32, 5);
        w.put((self.hdist - 1) as u32, 5);
        w.put((self.hclen - 4) as u32, 4);
        for &s in &CODELEN_ORDER[..self.hclen] {
            w.put(u32::from(self.cl_lengths[s]), 3);
        }
        let codes = canonical_codes(&self.cl_lengths);
        for &(s, extra) in &self.rle {
            w.put(u32::from(codes[s as usize]), u32::from(self.cl_lengths[s as usize]));
            w.put(u32::from(extra), rle_extra_bits(s));
        }
    }
}

fn rle_extra_bits(sym: u8) -> u32 {
    match sym {
        16 => 2,
        17 => 3,
        18 => 7,
        _ => 0,
    }
}

/// Encode code lengths with the repeat symbols 16 (previous ×3–6), 17 (zero ×3–10), 18 (zero ×11–138).
fn rle_code_lengths(lengths: &[u8]) -> Vec<(u8, u8)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lengths.len() {
        let l = lengths[i];
        let mut run = lengths[i..].iter().take_while(|&&x| x == l).count();
        i += run;
        if l == 0 {
            while run >= 11 {
                let r = run.min(138);
                out.push((18, (r - 11) as u8));
                run -= r;
            }
            if run >= 3 {
                out.push((17, (run - 3) as u8));
                run = 0;
            }
        } else {
            out.push((l, 0));
            run -= 1;
            while run >= 3 {
                let r = run.min(6);
                out.push((16, (r - 3) as u8));
                run -= r;
            }
        }
        for _ in 0..run {
            out.push((l, 0));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiny deterministic PRNG (xorshift32) for test data.
    struct Rng(u32);

    impl Rng {
        fn next(&mut self) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            self.0
        }

        fn bytes(&mut self, n: usize) -> Vec<u8> {
            (0..n).map(|_| self.next() as u8).collect()
        }
    }

    const LEVELS: [Level; 4] = [Level::Fastest, Level::Fast, Level::Default, Level::Best];

    fn round_trip(data: &[u8]) {
        for level in LEVELS {
            let c = compress(data, level);
            assert_eq!(decompress(&c).unwrap(), data, "raw {level:?}, {} bytes", data.len());
            let z = zlib_compress(data, level);
            assert_eq!(zlib_decompress(&z).unwrap(), data, "zlib {level:?}, {} bytes", data.len());
        }
    }

    fn mixed_data() -> Vec<u8> {
        let mut rng = Rng(0xC0FFEE);
        let mut data = Vec::new();
        let words: [&[u8]; 6] = [b"specula ", b"engine ", b"agent ", b"world ", b"frame ", b"\x00\x00\x00\xff"];
        while data.len() < 200_000 {
            match rng.next() % 4 {
                0 => {
                    let n = (rng.next() % 64) as usize;
                    data.extend_from_slice(&rng.bytes(n));
                }
                1 => data.extend(std::iter::repeat_n((rng.next() % 256) as u8, (rng.next() % 600) as usize)),
                _ => data.extend_from_slice(words[(rng.next() % 6) as usize]),
            }
        }
        data
    }

    #[test]
    fn round_trips_assorted_inputs() {
        round_trip(b"");
        round_trip(b"a");
        round_trip(b"ab");
        round_trip(b"abc");
        round_trip(&Rng(1).bytes(100_000));
        round_trip(&vec![7u8; 300_000]);
        round_trip(&b"abcabcabd".repeat(5000));
        round_trip(&mixed_data());
        // Matches spanning the full 32 KiB window.
        let block = Rng(9).bytes(WINDOW);
        let mut far = block.clone();
        far.extend_from_slice(&Rng(10).bytes(10));
        far.extend_from_slice(&block);
        round_trip(&far);
    }

    /// Inputs of several bands: band-sized edges, runs and repeats straddling band boundaries,
    /// and noise, so both run-length and LZ77 matches reach back across a boundary.
    fn multi_band_inputs() -> Vec<Vec<u8>> {
        let mut rng = Rng(0xBA4D);
        let mut striped = Vec::new();
        while striped.len() < 3 * BAND_SIZE + 4321 {
            // A run of zeros ending just past each boundary, then a repeated 1 KiB pattern.
            let to_boundary = BAND_SIZE - striped.len() % BAND_SIZE;
            striped.extend(rng.bytes(to_boundary.saturating_sub(700)));
            striped.extend(std::iter::repeat_n(0u8, 900));
            let pattern = rng.bytes(1024);
            for _ in 0..3 {
                striped.extend_from_slice(&pattern);
            }
        }
        let mut repeated = mixed_data();
        while repeated.len() < 2 * BAND_SIZE + 1 {
            repeated.extend_from_within(..100_000);
        }
        vec![
            mixed_data().iter().cycle().take(BAND_SIZE - 1).copied().collect(),
            mixed_data().iter().cycle().take(BAND_SIZE).copied().collect(),
            mixed_data().iter().cycle().take(BAND_SIZE + 1).copied().collect(),
            striped,
            repeated,
            Rng(3).bytes(2 * BAND_SIZE + 5),
            vec![9u8; 5 * BAND_SIZE + 3],
        ]
    }

    #[test]
    fn output_is_identical_for_every_thread_count() {
        for data in multi_band_inputs() {
            for level in LEVELS {
                let one = zlib_compress_threads(&data, level, 1);
                assert_eq!(zlib_decompress(&one).unwrap(), data, "{level:?}, {} bytes", data.len());
                for threads in [0, 2, 3, 8, available_threads()] {
                    let many = zlib_compress_threads(&data, level, threads);
                    assert!(one == many, "{level:?}, {} bytes: {threads} threads differ from 1", data.len());
                }
                let raw = compress_threads(&data, level, 1);
                assert_eq!(raw, compress_threads(&data, level, 5), "raw {level:?}");
                assert_eq!(decompress(&raw).unwrap(), data, "raw {level:?}");
                assert_eq!(&one[2..one.len() - 4], &raw[..], "zlib body is the raw stream");
            }
        }
    }

    #[test]
    fn bands_are_stitched_with_sync_flushes() {
        let data = mixed_data().iter().cycle().take(3 * BAND_SIZE).copied().collect::<Vec<u8>>();
        for level in LEVELS {
            let c = compress_threads(&data, level, 1);
            // Sync flushes (empty stored blocks: LEN 0, NLEN 0xFFFF) end bands 0 and 1.
            let marks = c.windows(4).filter(|w| w == &[0, 0, 0xFF, 0xFF]).count();
            assert!(marks >= 2, "{level:?}: {marks} sync markers");
            // Only the last band sets BFINAL: inflating stops at the end of the stream.
            let (out, used) = inflate(&c, usize::MAX).unwrap();
            assert_eq!(out, data);
            assert_eq!(used, c.len(), "{level:?}");
        }
        // One band: no sync flush, same bytes whatever the thread count.
        let small = &data[..BAND_SIZE];
        assert_eq!(compress_threads(small, Level::Default, 1), compress_threads(small, Level::Default, 16));
    }

    #[test]
    fn fastest_compresses_runs_and_round_trips_text() {
        let runs = vec![42u8; 1 << 20];
        assert!(compress(&runs, Level::Fastest).len() < 3000);
        let text = b"the quick brown fox jumps over the lazy dog. ".repeat(2000);
        let c = compress(&text, Level::Fastest);
        assert_eq!(decompress(&c).unwrap(), text);
        // No string matching: Huffman only, still well under the raw size.
        assert!(c.len() < text.len() * 3 / 4, "{}", c.len());
    }

    #[test]
    fn compresses_repetitive_data_and_bounds_random_data() {
        let rep = vec![42u8; 1 << 20];
        assert!(compress(&rep, Level::Fast).len() < 3000);
        let text = b"the quick brown fox jumps over the lazy dog. ".repeat(2000);
        let best = compress(&text, Level::Best).len();
        assert!(best < text.len() / 50, "{best}");
        // Incompressible data falls back to stored blocks: tiny overhead only.
        let random = Rng(77).bytes(200_000);
        for level in LEVELS {
            let c = compress(&random, level);
            assert!(c.len() <= random.len() + random.len() / 1000 + 64, "{level:?}: {}", c.len());
        }
        let mixed = mixed_data();
        let fast = compress(&mixed, Level::Fast).len();
        let best = compress(&mixed, Level::Best).len();
        assert!(best <= fast, "best {best} > fast {fast}");
    }

    #[test]
    fn fixed_blocks_cover_all_literals() {
        // Short inputs pick fixed Huffman; bytes ≥ 144 use the 9-bit fixed codes.
        for start in [0u8, 100, 140, 200, 250] {
            let data: Vec<u8> = (0..24).map(|i| start.wrapping_add(i)).collect();
            let c = compress(&data, Level::Default);
            assert_eq!(c[0] >> 1 & 3, 1, "expected a fixed block");
            assert_eq!(decompress(&c).unwrap(), data);
        }
    }

    #[test]
    fn empty_input_matches_zlib() {
        // python: zlib.compress(b"", 6)
        assert_eq!(zlib_compress(b"", Level::Default), [120, 156, 3, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn decodes_python_zlib_stored() {
        // python: zlib.compress(b"Stored block, level 0!", 0)
        let z = [
            120, 1, 1, 22, 0, 233, 255, 83, 116, 111, 114, 101, 100, 32, 98, 108, 111, 99, 107, 44, 32, 108, 101,
            118, 101, 108, 32, 48, 33, 92, 107, 7, 114,
        ];
        assert_eq!(zlib_decompress(&z).unwrap(), b"Stored block, level 0!");
    }

    #[test]
    fn decodes_python_zlib_fixed() {
        // python: zlib.compress(b"hello hello hello hello, fixed huffman", 9)
        let z = [
            120, 218, 203, 72, 205, 201, 201, 87, 200, 64, 39, 117, 20, 210, 50, 43, 82, 83, 20, 50, 74, 211, 210,
            114, 19, 243, 0, 18, 32, 14, 18,
        ];
        assert_eq!(z[2] >> 1 & 3, 1, "reference stream should be a fixed-Huffman block");
        assert_eq!(zlib_decompress(&z).unwrap(), b"hello hello hello hello, fixed huffman");
    }

    #[test]
    fn decodes_python_zlib_dynamic() {
        // python: zlib.compress(bytes(b"etaoinshrdlu"[(i*i*7 + i//3) % 12] for i in range(40)), 9)
        let z = [
            120, 218, 13, 202, 65, 10, 0, 48, 8, 3, 176, 183, 10, 22, 44, 136, 130, 213, 255, 111, 57, 7, 65, 214,
            152, 75, 145, 188, 25, 135, 54, 243, 108, 26, 88, 102, 153, 181, 240, 207, 3, 88, 136, 16, 196,
        ];
        assert_eq!(z[2] >> 1 & 3, 2, "reference stream should be a dynamic-Huffman block");
        let expected: Vec<u8> = (0..40usize).map(|i| b"etaoinshrdlu"[(i * i * 7 + i / 3) % 12]).collect();
        assert_eq!(zlib_decompress(&z).unwrap(), expected);
        // Raw entry point on the same DEFLATE payload (the Adler trailer is ignored as trailing data).
        assert_eq!(decompress(&z[2..]).unwrap(), expected);
    }

    #[test]
    fn rejects_malformed_streams() {
        let plain = &mixed_data()[..5000];
        let good = zlib_compress(plain, Level::Default);
        assert_eq!(zlib_decompress(&good).unwrap(), plain);
        // Header problems.
        assert!(zlib_decompress(&[]).is_err());
        assert!(zlib_decompress(&[0x78]).is_err());
        assert!(zlib_decompress(&[0x79, 0x9C, 3, 0]).is_err());
        assert!(zlib_decompress(&[0x78, 0x9D, 3, 0, 0, 0, 0, 1]).is_err());
        assert!(zlib_decompress(&[0x78, 0xBB, 3, 0, 0, 0, 0, 1]).is_err()); // FDICT
        // Adler mismatch and trailing junk.
        let mut bad = good.clone();
        *bad.last_mut().unwrap() ^= 1;
        let e = zlib_decompress(&bad).unwrap_err();
        assert!(e.message.contains("Adler"), "{e}");
        let mut junk = good.clone();
        junk.push(0);
        assert!(zlib_decompress(&junk).is_err());
        // Truncation at every length must error, never panic.
        for cut in 0..good.len() {
            assert!(zlib_decompress(&good[..cut]).is_err(), "cut {cut}");
        }
        // Reserved block type, bad stored length, distance too far.
        assert!(decompress(&[0b111]).is_err());
        assert!(decompress(&[1, 5, 0, 0, 0]).is_err());
        // Fixed block: match (len 3, dist 1) with empty output → distance too far.
        assert!(decompress(&[0x03, 0x02, 0x00]).is_err());
    }

    #[test]
    fn random_garbage_never_panics() {
        let mut rng = Rng(12345);
        let seed = compress(&mixed_data()[..20_000], Level::Default);
        for i in 0..3000 {
            let n = (rng.next() % 200) as usize;
            let mut buf = if i % 2 == 0 { rng.bytes(n) } else { seed.clone() };
            if i % 2 == 1 {
                for _ in 0..1 + rng.next() % 4 {
                    let at = rng.next() as usize % buf.len();
                    buf[at] ^= 1 << (rng.next() % 8);
                }
            }
            let _ = decompress(&buf);
        }
    }

    #[test]
    fn output_limit_is_enforced() {
        let z = zlib_compress(&[1u8; 1000], Level::Default);
        assert!(zlib_decompress_limited(&z, 999, false).is_err());
        assert_eq!(zlib_decompress_limited(&z, 1000, false).unwrap().len(), 1000);
    }

    #[test]
    fn public_limits_stop_expansion_bombs() {
        // 4 MiB of zeros compresses to a few kilobytes: a miniature decompression bomb.
        let bomb = vec![0u8; 4 << 20];
        let z = zlib_compress(&bomb, Level::Best);
        assert!(z.len() < 16 * 1024, "bomb is {} bytes", z.len());
        let e = zlib_decompress_with_limit(&z, 1 << 20).unwrap_err();
        assert!(e.message.contains("1048576-byte limit"), "{e}");
        let raw = &z[2..z.len() - 4];
        assert!(decompress_with_limit(raw, 1 << 20).is_err());
        // Under the default cap it decodes.
        assert_eq!(zlib_decompress(&z).unwrap().len(), bomb.len());
        assert_eq!(decompress(raw).unwrap().len(), bomb.len());
        const { assert!(DEFAULT_MAX_OUTPUT >= 64 << 20) };
    }

    #[test]
    fn huffman_lengths_respect_limits() {
        // Fibonacci frequencies force very deep trees without a limit.
        let mut freq = vec![0u32; 40];
        let (mut a, mut b) = (1u32, 1u32);
        for f in freq.iter_mut() {
            *f = a;
            (a, b) = (b, a.saturating_add(b));
        }
        for limit in [7, 15] {
            let lens = huffman_lengths(&freq, limit);
            assert!(lens.iter().all(|&l| l >= 1 && l as usize <= limit));
            let kraft: f64 = lens.iter().map(|&l| 0.5f64.powi(l as i32)).sum();
            assert!((kraft - 1.0).abs() < 1e-12, "limit {limit}: kraft {kraft}");
        }
        // Degenerate inputs still give a complete two-symbol code.
        assert_eq!(huffman_lengths(&[0, 0, 0], 15), [1, 1, 0]);
        assert_eq!(huffman_lengths(&[0, 5, 0], 15), [1, 1, 0]);
    }

    #[test]
    fn symbol_tables_are_consistent() {
        for len in MIN_MATCH..=MAX_MATCH {
            let s = LENGTH_SYMBOL[len - MIN_MATCH] as usize;
            let span = if s == 28 { 0 } else { (1 << LENGTH_EXTRA[s]) - 1 };
            assert!(len >= LENGTH_BASE[s] as usize && len <= LENGTH_BASE[s] as usize + span, "len {len}");
        }
        for d in 1..=WINDOW {
            let s = dist_symbol(d);
            let base = DIST_BASE[s] as usize;
            assert!(d >= base && d < base + (1 << DIST_EXTRA[s]), "dist {d}");
        }
    }
}
