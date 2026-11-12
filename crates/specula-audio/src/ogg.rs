//! The Ogg container (RFC 3533): pages, their CRC, and packets rebuilt from lacing values.
//!
//! A page is a 27-byte header (`OggS`, version 0, flags, granule position, serial, sequence,
//! CRC, segment count), a lacing table and a body. A packet is a run of 255-byte segments ended
//! by one shorter than 255; packets may span pages (the `continued` flag). [`PacketReader`] walks
//! one logical stream (the first one that begins in the file) and hands back whole packets with
//! the granule position of the page that completes them.
//!
//! Hostile bytes give structured errors, never a panic or an uncapped allocation: a packet may
//! not grow past [`MAX_PACKET`], and pages with a bad CRC are skipped (and counted) once the
//! headers are read.

use crate::error::{AudioError, ErrorCode, Result};
use std::sync::Arc;

/// Page header flags.
pub const FLAG_CONTINUED: u8 = 1;
pub const FLAG_BOS: u8 = 2;
pub const FLAG_EOS: u8 = 4;
/// The fixed part of a page header.
pub const HEADER_LEN: usize = 27;
/// Largest packet the reader assembles (Vorbis audio packets are a few KB; setup headers rarely
/// pass 100 KB).
pub const MAX_PACKET: usize = 4 << 20;
/// The granule value meaning "no packet ends on this page".
pub const NO_GRANULE: u64 = u64::MAX;

const fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut r = (i as u32) << 24;
        let mut k = 0;
        while k < 8 {
            r = if r & 0x8000_0000 != 0 { (r << 1) ^ 0x04c1_1db7 } else { r << 1 };
            k += 1;
        }
        t[i] = r;
        i += 1;
    }
    t
}

static CRC: [u32; 256] = crc_table();

/// Ogg's CRC-32 (polynomial 0x04c11db7, not reflected, initial value 0, no final xor).
pub fn crc32(data: &[u8]) -> u32 {
    crc_update(0, data)
}

fn crc_update(mut crc: u32, data: &[u8]) -> u32 {
    for &b in data {
        crc = (crc << 8) ^ CRC[((crc >> 24) as u8 ^ b) as usize];
    }
    crc
}

/// One parsed page (borrowing the file's bytes).
#[derive(Clone, Copy, Debug)]
pub struct Page<'a> {
    /// Byte offset of the page in the file.
    pub offset: usize,
    pub flags: u8,
    /// Granule position ([`NO_GRANULE`] when no packet ends here).
    pub granule: u64,
    pub serial: u32,
    pub sequence: u32,
    pub lacing: &'a [u8],
    pub body: &'a [u8],
    /// Total page length in bytes (header + lacing + body).
    pub len: usize,
}

fn page_err(code: ErrorCode, offset: usize, msg: impl Into<String>) -> AudioError {
    AudioError::new(code, msg).with_offset(offset)
}

/// Parses the page at `at`, checking its CRC.
pub fn parse_page(bytes: &[u8], at: usize) -> Result<Page<'_>> {
    let b = bytes.get(at..).unwrap_or(&[]);
    if b.len() < HEADER_LEN {
        return Err(page_err(ErrorCode::OggTruncated, at, "the stream ends inside a page header"));
    }
    if &b[..4] != b"OggS" {
        return Err(page_err(ErrorCode::OggBadPage, at, "no OggS capture pattern here"));
    }
    if b[4] != 0 {
        return Err(page_err(ErrorCode::OggBadPage, at, "unknown Ogg page version").with_numbers(b[4] as f64, 0.0));
    }
    let flags = b[5];
    let granule = u64::from_le_bytes(b[6..14].try_into().expect("8 bytes"));
    let serial = u32::from_le_bytes(b[14..18].try_into().expect("4 bytes"));
    let sequence = u32::from_le_bytes(b[18..22].try_into().expect("4 bytes"));
    let crc = u32::from_le_bytes(b[22..26].try_into().expect("4 bytes"));
    let segments = b[26] as usize;
    let lacing_end = HEADER_LEN + segments;
    if b.len() < lacing_end {
        return Err(page_err(ErrorCode::OggTruncated, at, "the stream ends inside a page's lacing table"));
    }
    let lacing = &b[HEADER_LEN..lacing_end];
    let body_len: usize = lacing.iter().map(|&l| l as usize).sum();
    let len = lacing_end + body_len;
    if b.len() < len {
        return Err(page_err(ErrorCode::OggTruncated, at, "the stream ends inside a page body").with_numbers(b.len() as f64, len as f64));
    }
    // CRC over the page with the CRC field zeroed.
    let mut c = crc_update(0, &b[..22]);
    c = crc_update(c, &[0; 4]);
    c = crc_update(c, &b[26..len]);
    if c != crc {
        return Err(page_err(ErrorCode::OggBadPage, at, "page CRC mismatch").with_numbers(c as f64, crc as f64));
    }
    Ok(Page { offset: at, flags, granule, serial, sequence, lacing, body: &b[lacing_end..len], len })
}

/// CRC bytes one resync search may check (a page is at most 65,307 bytes, so this is about
/// 64 candidate pages): hostile bytes full of `OggS` patterns can't turn a scan into seconds of
/// CRC work (review: 1 MiB of them took 6.7 s).
pub const RESYNC_BUDGET: usize = 4 << 20;

/// The next offset at or after `from` where a valid page starts (resync after damage), within
/// [`RESYNC_BUDGET`] bytes of CRC work.
pub fn find_page(bytes: &[u8], from: usize) -> Option<usize> {
    let mut budget = RESYNC_BUDGET;
    find_page_within(bytes, from, &mut budget)
}

/// [`find_page`], charging each candidate's checked length to `budget` (`None` once it runs out).
pub fn find_page_within(bytes: &[u8], from: usize, budget: &mut usize) -> Option<usize> {
    let mut i = from;
    while i + 4 <= bytes.len() {
        let rest = &bytes[i..];
        let p = rest.windows(4).position(|w| w == b"OggS")?;
        let at = i + p;
        // Cheap checks before the CRC: version 0, and the whole page inside the file.
        let b = &bytes[at..];
        if b.len() >= HEADER_LEN && b[4] == 0 && b.len() >= HEADER_LEN + b[26] as usize {
            let segs = b[26] as usize;
            let len = HEADER_LEN + segs + b[HEADER_LEN..HEADER_LEN + segs].iter().map(|&l| l as usize).sum::<usize>();
            if len <= b.len() {
                if len > *budget {
                    *budget = 0;
                    return None;
                }
                *budget -= len;
                if parse_page(bytes, at).is_ok() {
                    return Some(at);
                }
            }
        }
        i = at + 1;
    }
    None
}

/// What [`PacketReader::next_packet`] knows about a packet it returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PacketInfo {
    /// The granule position of the page this packet completes on, when it is the last packet
    /// completed there (`None` otherwise, or when the page has no granule).
    pub granule: Option<u64>,
    /// The page that completes the packet has the end-of-stream flag and this is its last packet.
    pub eos: bool,
    /// Byte offset of the page this packet completes on.
    pub page_offset: usize,
}

/// Reads the packets of one logical stream from an in-memory file.
#[derive(Clone)]
pub struct PacketReader {
    bytes: Arc<[u8]>,
    serial: u32,
    /// The next page to read.
    next: usize,
    /// The page being consumed: its offset, flags, granule, lacing position and body cursor.
    cur: Option<CurPage>,
    partial: Vec<u8>,
    /// Drop the continued fragment at the start of the next page (after a seek or a lost page).
    skip_continued: bool,
    ended: bool,
    /// Pages skipped for a bad CRC or a gap in sequence numbers.
    pub pages_skipped: u64,
    /// The file ends mid-page (the packets before it still decode).
    pub truncated: bool,
    last_sequence: Option<u32>,
    /// Report bad pages as errors instead of skipping them (while reading headers).
    pub strict: bool,
    /// CRC work left for resyncing after damage ([`RESYNC_BUDGET`]); spent, the stream ends.
    resync_budget: usize,
    /// The resync budget ran out (the rest of the file was not searched).
    pub resync_exhausted: bool,
}

#[derive(Clone, Copy, Debug)]
struct CurPage {
    offset: usize,
    flags: u8,
    granule: u64,
    seg: usize,
    segs: usize,
    lacing_at: usize,
    body_at: usize,
    /// Index of the last segment that ends a packet (lacing < 255), if any.
    last_end: Option<usize>,
}

impl PacketReader {
    /// A reader over the first logical stream that begins in `bytes`.
    pub fn new(bytes: Arc<[u8]>) -> Result<PacketReader> {
        if bytes.len() < 4 || &bytes[..4] != b"OggS" {
            return Err(page_err(ErrorCode::OggNotOgg, 0, "not an Ogg stream (no OggS at the start)"));
        }
        let first = parse_page(&bytes, 0)?;
        if first.flags & FLAG_BOS == 0 {
            return Err(page_err(ErrorCode::OggBadPage, 0, "the first page doesn't begin a stream (no BOS flag)"));
        }
        let serial = first.serial;
        Ok(PacketReader {
            bytes,
            serial,
            next: 0,
            cur: None,
            partial: Vec::new(),
            skip_continued: false,
            ended: false,
            pages_skipped: 0,
            truncated: false,
            last_sequence: None,
            strict: true,
            resync_budget: RESYNC_BUDGET,
            resync_exhausted: false,
        })
    }

    pub fn bytes(&self) -> &Arc<[u8]> {
        &self.bytes
    }

    pub fn serial(&self) -> u32 {
        self.serial
    }

    /// Continues reading at the page at `offset` (a page of this stream); the fragment of a
    /// packet continued from an earlier page is dropped.
    pub fn seek_to_page(&mut self, offset: usize) {
        self.next = offset;
        self.cur = None;
        self.partial.clear();
        self.skip_continued = true;
        self.ended = false;
        self.last_sequence = None;
    }

    /// Loads the next page of this stream into `cur`; `Ok(false)` at the end.
    fn load_page(&mut self) -> Result<bool> {
        loop {
            if self.next >= self.bytes.len() {
                return Ok(false);
            }
            let bytes = self.bytes.clone();
            let page = match parse_page(&bytes, self.next) {
                Ok(p) => p,
                Err(e) if e.code == ErrorCode::OggTruncated => {
                    self.truncated = true;
                    return Ok(false);
                }
                Err(e) => {
                    if self.strict {
                        return Err(e);
                    }
                    // Damaged page: resync at the next valid page and drop any partial packet.
                    self.pages_skipped += 1;
                    self.partial.clear();
                    self.skip_continued = true;
                    // The gap this leaves in sequence numbers is the same loss: don't count it twice.
                    self.last_sequence = None;
                    match find_page_within(&bytes, self.next + 1, &mut self.resync_budget) {
                        Some(at) => {
                            self.next = at;
                            continue;
                        }
                        None => {
                            self.resync_exhausted = self.resync_budget == 0;
                            return Ok(false);
                        }
                    }
                }
            };
            self.next = page.offset + page.len;
            if page.serial != self.serial {
                continue;
            }
            if let Some(last) = self.last_sequence {
                if page.sequence != last.wrapping_add(1) {
                    // A lost page: whatever was being assembled is incomplete.
                    if self.strict {
                        return Err(page_err(ErrorCode::OggBadPage, page.offset, "page sequence gap").with_numbers(page.sequence as f64, last.wrapping_add(1) as f64));
                    }
                    self.pages_skipped += 1;
                    self.partial.clear();
                    self.skip_continued = true;
                }
            }
            self.last_sequence = Some(page.sequence);
            let continued = page.flags & FLAG_CONTINUED != 0;
            if !continued && !self.partial.is_empty() {
                // The previous packet never finished: drop it.
                if self.strict {
                    return Err(page_err(ErrorCode::OggBadPage, page.offset, "a packet was left unfinished before this page"));
                }
                self.partial.clear();
            }
            let last_end = page.lacing.iter().rposition(|&l| l < 255);
            let lacing_at = page.offset + HEADER_LEN;
            let mut cur = CurPage {
                offset: page.offset,
                flags: page.flags,
                granule: page.granule,
                seg: 0,
                segs: page.lacing.len(),
                lacing_at,
                body_at: lacing_at + page.lacing.len(),
                last_end,
            };
            if continued && self.skip_continued {
                // Skip the tail of a packet whose start we never saw.
                while cur.seg < cur.segs {
                    let l = bytes[cur.lacing_at + cur.seg];
                    cur.body_at += l as usize;
                    cur.seg += 1;
                    if l < 255 {
                        break;
                    }
                }
                self.partial.clear();
            }
            self.skip_continued = false;
            self.cur = Some(cur);
            return Ok(true);
        }
    }

    /// The next whole packet, copied into `out` (cleared first); `Ok(None)` at the end.
    pub fn next_packet(&mut self, out: &mut Vec<u8>) -> Result<Option<PacketInfo>> {
        out.clear();
        if self.ended {
            return Ok(None);
        }
        loop {
            let cur = match self.cur {
                Some(c) if c.seg < c.segs => c,
                Some(c) => {
                    self.cur = None;
                    if c.flags & FLAG_EOS != 0 {
                        self.ended = true;
                        return Ok(None);
                    }
                    continue;
                }
                None => {
                    if !self.load_page()? {
                        self.ended = true;
                        return Ok(None);
                    }
                    continue;
                }
            };
            let mut c = cur;
            let bytes = self.bytes.clone();
            while c.seg < c.segs {
                let l = bytes[c.lacing_at + c.seg] as usize;
                if self.partial.len() + l > MAX_PACKET {
                    self.cur = Some(c);
                    return Err(page_err(ErrorCode::Limit, c.offset, "an Ogg packet is larger than the decoder accepts").with_numbers((self.partial.len() + l) as f64, MAX_PACKET as f64));
                }
                self.partial.extend_from_slice(&bytes[c.body_at..c.body_at + l]);
                c.body_at += l;
                let seg = c.seg;
                c.seg += 1;
                if l < 255 {
                    let last = c.last_end == Some(seg);
                    let info = PacketInfo {
                        granule: (last && c.granule != NO_GRANULE).then_some(c.granule),
                        eos: last && c.flags & FLAG_EOS != 0,
                        page_offset: c.offset,
                    };
                    self.cur = Some(c);
                    std::mem::swap(out, &mut self.partial);
                    self.partial.clear();
                    if info.eos {
                        self.ended = true;
                    }
                    return Ok(Some(info));
                }
            }
            // The packet continues on the next page.
            self.cur = Some(c);
        }
    }

    /// The first page of this stream at or after `from`, as `(offset, granule, len)`, charging
    /// the search to `budget` (see [`find_page_within`]).
    pub fn page_at_or_after(&self, from: usize, budget: &mut usize) -> Option<(usize, u64, usize)> {
        let mut at = find_page_within(&self.bytes, from, budget)?;
        loop {
            let p = parse_page(&self.bytes, at).ok()?;
            if p.serial == self.serial {
                return Some((p.offset, p.granule, p.len));
            }
            at = find_page_within(&self.bytes, at + p.len, budget)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one page with the given packets' lacing (no packet spanning).
    pub(crate) fn page(flags: u8, granule: u64, serial: u32, seq: u32, segments: &[&[u8]]) -> Vec<u8> {
        let mut lacing = Vec::new();
        let mut body = Vec::new();
        for s in segments {
            let mut n = s.len();
            loop {
                let l = n.min(255);
                lacing.push(l as u8);
                n -= l;
                if l < 255 {
                    break;
                }
            }
            body.extend_from_slice(s);
        }
        let mut p = Vec::new();
        p.extend_from_slice(b"OggS");
        p.push(0);
        p.push(flags);
        p.extend_from_slice(&granule.to_le_bytes());
        p.extend_from_slice(&serial.to_le_bytes());
        p.extend_from_slice(&seq.to_le_bytes());
        p.extend_from_slice(&[0; 4]);
        p.push(lacing.len() as u8);
        p.extend_from_slice(&lacing);
        p.extend_from_slice(&body);
        let crc = crc32(&p);
        p[22..26].copy_from_slice(&crc.to_le_bytes());
        p
    }

    #[test]
    fn crc_matches_the_reference_value() {
        // The CRC of "OggS" with this polynomial (computed independently).
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0x89a1_897f);
    }

    #[test]
    fn packets_come_back_whole_across_pages() {
        let big = vec![7u8; 600];
        let mut f = page(FLAG_BOS, 0, 9, 0, &[b"hdr"]);
        // A 600-byte packet split over two pages: 255+255 on the first, 90 on the second.
        let mut p2 = page(0, NO_GRANULE, 9, 1, &[]);
        // Hand-build lacing for a spanning packet.
        p2.truncate(26);
        p2.push(2);
        p2.extend_from_slice(&[255, 255]);
        p2.extend_from_slice(&big[..510]);
        p2[22..26].copy_from_slice(&[0; 4]);
        let c = crc32(&p2);
        p2[22..26].copy_from_slice(&c.to_le_bytes());
        f.extend_from_slice(&p2);
        f.extend_from_slice(&page(FLAG_CONTINUED | FLAG_EOS, 1234, 9, 2, &[&big[510..], b"x"]));
        let mut r = PacketReader::new(f.into()).unwrap();
        let mut out = Vec::new();
        let a = r.next_packet(&mut out).unwrap().unwrap();
        assert_eq!(out, b"hdr");
        assert_eq!(a.granule, Some(0));
        let b = r.next_packet(&mut out).unwrap().unwrap();
        assert_eq!(out, big);
        assert_eq!(b.granule, None, "not the last packet on its page");
        let c = r.next_packet(&mut out).unwrap().unwrap();
        assert_eq!(out, b"x");
        assert_eq!((c.granule, c.eos), (Some(1234), true));
        assert!(r.next_packet(&mut out).unwrap().is_none());
    }

    #[test]
    fn damage_is_an_error_or_skipped_never_a_panic() {
        let mut f = page(FLAG_BOS, 0, 1, 0, &[b"abc"]);
        f.extend_from_slice(&page(0, 5, 1, 1, &[b"def"]));
        f.extend_from_slice(&page(FLAG_EOS, 9, 1, 2, &[b"ghi"]));
        // Corrupt the middle page's body.
        let mid = 31 + 28;
        f[mid] ^= 0xff;
        let mut r = PacketReader::new(f.clone().into()).unwrap();
        let mut out = Vec::new();
        r.next_packet(&mut out).unwrap();
        assert_eq!(r.next_packet(&mut out).unwrap_err().code, ErrorCode::OggBadPage);
        let mut r = PacketReader::new(f.into()).unwrap();
        r.strict = false;
        r.next_packet(&mut out).unwrap();
        r.next_packet(&mut out).unwrap();
        assert_eq!(out, b"ghi");
        assert_eq!(r.pages_skipped, 1);
        // A resync through OggS spam stops at its budget instead of checking every candidate.
        let mut spam = Vec::new();
        for _ in 0..(1 << 15) {
            spam.extend_from_slice(b"OggS");
            spam.push(0);
            spam.extend_from_slice(&[0xff; 27]);
        }
        let start = std::time::Instant::now();
        let mut budget = RESYNC_BUDGET;
        assert_eq!(find_page_within(&spam, 0, &mut budget), None);
        assert!(start.elapsed() < specula_core::speed_budget(std::time::Duration::from_secs(2)), "{:?}", start.elapsed());
        // Every truncation of a stream.
        let g = page(FLAG_BOS, 0, 1, 0, &[&[1u8; 300]]);
        for n in 0..g.len() {
            if let Ok(mut r) = PacketReader::new(g[..n].to_vec().into()) {
                let _ = r.next_packet(&mut out);
            }
        }
        assert_eq!(PacketReader::new(b"RIFF....".to_vec().into()).err().unwrap().code, ErrorCode::OggNotOgg);
    }
}
