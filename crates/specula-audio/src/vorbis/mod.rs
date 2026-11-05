//! A hand-written Ogg Vorbis I decoder (no dependencies).
//!
//! - **Container:** [`crate::ogg`] (pages, CRC, lacing, packets across pages).
//! - **Headers:** identification, comment (vendor and `KEY=value` comments; `LOOPSTART`,
//!   `LOOPEND` / `LOOPLENGTH` give loop points), setup (codebooks with lookup types 1 and 2,
//!   floors 0 and 1, residues 0, 1 and 2, mappings with channel coupling, modes).
//! - **Audio:** floor and residue decode, inverse coupling, the floor × residue product, the
//!   inverse MDCT ([`mdct`]), the Vorbis power-sine window for every long/short combination,
//!   overlap-add, and granule positions: leading samples the first page doesn't count are
//!   dropped, the last page's granule trims the end.
//! - **Seeking:** bisection over pages by granule position, then decoding forward to the exact
//!   frame (positions are resynchronised from the next page's granule).
//!
//! **Hostile bytes** give structured errors ([`ErrorCode::OggBadPage`],
//! [`ErrorCode::VorbisBadHeader`], [`ErrorCode::Limit`] …) and never panic or allocate without a
//! cap: codebooks are limited to [`codebook::MAX_ENTRIES`] entries and the whole setup to
//! [`SETUP_BUDGET`] table values; a damaged audio packet is skipped and counted.

mod bits;
mod codebook;
mod floor;
mod mdct;
mod residue;

use crate::error::{AudioError, ErrorCode, Result};
use crate::ogg::{self, PacketReader, NO_GRANULE};
use bits::{ilog, Bits};
use codebook::Codebook;
use floor::{Floor, FloorOut, FloorScratch};
use residue::{Residue, ResidueScratch};
use specula_core::dmath;
use std::sync::Arc;

/// Most channels decoded (Vorbis allows 255; 7.1 is 8).
pub const MAX_CHANNELS: usize = 8;
/// Most codebook entries and VQ values one setup header may expand to, in total.
pub const SETUP_BUDGET: usize = 1 << 23;
/// Most frames decoded while the position is unknown (after a seek, before a granule).
const MAX_PENDING: usize = 1 << 20;
/// Kept comments, and the longest kept comment or vendor string (longer ones are cut).
const MAX_COMMENTS: usize = 64;
const MAX_COMMENT_LEN: usize = 512;

pub(crate) fn header_err(path: String, msg: impl Into<String>) -> AudioError {
    AudioError::at(ErrorCode::VorbisBadHeader, path, msg)
}

/// Charges setup allocations against [`SETUP_BUDGET`].
pub(crate) struct Limits {
    used: usize,
}

impl Limits {
    pub fn charge(&mut self, n: usize, path: impl Fn() -> String) -> Result<()> {
        self.used = self.used.saturating_add(n);
        if self.used > SETUP_BUDGET {
            return Err(AudioError::at(ErrorCode::Limit, path(), "the setup header's tables are larger than the decoder accepts").with_numbers(self.used as f64, SETUP_BUDGET as f64));
        }
        Ok(())
    }
}

/// What the headers say.
#[derive(Clone, Debug, PartialEq)]
pub struct Info {
    pub channels: u16,
    pub rate: u32,
    /// Short and long block sizes.
    pub blocksizes: [u32; 2],
    pub bitrate_nominal: i32,
    pub vendor: String,
    /// `(KEY, value)` comments, keys upper-cased (at most 64 kept, each cut at 512 bytes).
    pub comments: Vec<(String, String)>,
    /// Frames in the stream (the last page's granule position), when known.
    pub total_frames: Option<u64>,
    /// Loop points from `LOOPSTART` and `LOOPEND` / `LOOPLENGTH` comments (frames).
    pub loop_start: Option<u64>,
    pub loop_end: Option<u64>,
}

/// Decode counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DecodeStats {
    pub packets: u64,
    /// Audio packets that couldn't be decoded and were skipped.
    pub bad_packets: u64,
    /// Ogg pages skipped for a bad CRC or a sequence gap.
    pub pages_skipped: u64,
    pub seeks: u64,
    /// Non-finite output samples (hostile codebook values) replaced by silence.
    pub non_finite: u64,
    /// The reader gave up resyncing after damage (its CRC budget ran out).
    pub resync_exhausted: bool,
}

#[derive(Clone, Debug)]
struct Mode {
    blockflag: bool,
    mapping: usize,
}

#[derive(Clone, Debug)]
struct Mapping {
    /// `(floor, residue)` per submap.
    submaps: Vec<(usize, usize)>,
    mux: Vec<usize>,
    coupling: Vec<(usize, usize)>,
}

struct Setup {
    books: Vec<Codebook>,
    floors: Vec<Floor>,
    residues: Vec<Residue>,
    mappings: Vec<Mapping>,
    modes: Vec<Mode>,
}

/// A streaming decoder over an in-memory Ogg Vorbis file.
pub struct Decoder {
    info: Info,
    setup: Setup,
    reader: PacketReader,
    /// The reader positioned at the first audio packet (restarts and seeks begin here).
    audio_start: PacketReader,
    audio_start_page: usize,
    imdct: [mdct::Imdct; 2],
    /// Rising window slopes for the short and long block (length blocksize / 2).
    slopes: [Vec<f32>; 2],
    overlap: Vec<Vec<f32>>,
    prev_n: usize,
    /// Decoded frames not yet returned, per channel, from `out_pos`.
    out: Vec<Vec<f32>>,
    out_pos: usize,
    /// Absolute frame of `out[.][out_pos]` (valid when `known`).
    pos: u64,
    known: bool,
    /// Decoding from the first audio packet (not resumed after a seek).
    fresh: bool,
    /// Frames before this one are dropped once the position is known (seek target).
    target: u64,
    ended: bool,
    packet: Vec<u8>,
    floors: Vec<Vec<f32>>,
    residues: Vec<Vec<f32>>,
    floor_unused: Vec<bool>,
    no_residue: Vec<bool>,
    y: Vec<f32>,
    fscratch: FloorScratch,
    rscratch: ResidueScratch,
    pub stats: DecodeStats,
}

fn bad_packet(msg: &str) -> AudioError {
    AudioError::new(ErrorCode::VorbisBadPacket, msg)
}

fn check_header_type(data: &[u8], kind: u8, name: &str) -> Result<()> {
    if data.len() < 7 || data[0] != kind || &data[1..7] != b"vorbis" {
        return Err(header_err(name.into(), format!("expected the Vorbis {name} header (packet type {kind})")));
    }
    Ok(())
}

impl Decoder {
    /// Reads the three headers; audio decodes on demand.
    pub fn new(bytes: impl Into<Arc<[u8]>>) -> Result<Decoder> {
        let bytes: Arc<[u8]> = bytes.into();
        let mut reader = PacketReader::new(bytes.clone())?;
        reader.strict = true;
        let mut packet = Vec::new();
        let missing = |what: &str| header_err(what.into(), format!("the stream ends before the {what} header"));
        // Identification.
        reader.next_packet(&mut packet)?.ok_or_else(|| missing("identification"))?;
        check_header_type(&packet, 1, "identification")?;
        if packet.len() < 30 {
            return Err(header_err("identification".into(), "identification header is too short"));
        }
        let le32 = |i: usize| u32::from_le_bytes(packet[i..i + 4].try_into().expect("4 bytes"));
        let version = le32(7);
        let channels = packet[11] as usize;
        let rate = le32(12);
        let bitrate_nominal = le32(20) as i32;
        let bs = packet[28];
        let (bs0, bs1) = (1u32 << (bs & 15), 1u32 << (bs >> 4));
        if version != 0 {
            return Err(header_err("identification.version".into(), "unknown Vorbis version").with_numbers(version as f64, 0.0));
        }
        if channels == 0 || rate == 0 {
            return Err(header_err("identification".into(), "zero channels or sample rate"));
        }
        if channels > MAX_CHANNELS {
            return Err(AudioError::at(ErrorCode::VorbisUnsupported, "identification.channels", "more channels than the decoder mixes").with_numbers(channels as f64, MAX_CHANNELS as f64));
        }
        if !(64..=8192).contains(&bs0) || !(64..=8192).contains(&bs1) || bs0 > bs1 || packet[29] & 1 == 0 {
            return Err(header_err("identification.blocksize".into(), "block sizes must be 64..=8192 with short <= long, and the framing bit set").with_numbers(bs0 as f64, bs1 as f64));
        }
        if !(crate::sound::MIN_RATE..=crate::sound::MAX_RATE).contains(&rate) {
            return Err(AudioError::at(ErrorCode::VorbisUnsupported, "identification.rate", "sample rate out of range").with_numbers(rate as f64, crate::sound::MAX_RATE as f64));
        }
        // Comments.
        reader.next_packet(&mut packet)?.ok_or_else(|| missing("comment"))?;
        check_header_type(&packet, 3, "comment")?;
        let (vendor, comments) = parse_comments(&packet[7..])?;
        // Setup.
        reader.next_packet(&mut packet)?.ok_or_else(|| missing("setup"))?;
        check_header_type(&packet, 5, "setup")?;
        let setup = parse_setup(&packet[7..], channels)?;

        let lookup = |key: &str| comments.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.trim().parse::<u64>().ok());
        let total_frames = last_granule(&bytes, reader.serial());
        let (loop_start, loop_end) = match loop_points(lookup("LOOPSTART"), lookup("LOOPEND"), lookup("LOOPLENGTH"), total_frames) {
            Some((s, e)) => (Some(s), Some(e)),
            None => (None, None),
        };
        let info = Info { channels: channels as u16, rate, blocksizes: [bs0, bs1], bitrate_nominal, vendor, comments, total_frames, loop_start, loop_end };
        reader.strict = false;
        let start_page = reader_next_page(&reader);
        let slope = |n: usize| -> Vec<f32> {
            let half = n / 2;
            (0..half)
                .map(|i| {
                    let s = dmath::sin_f64((i as f64 + 0.5) / half as f64 * std::f64::consts::FRAC_PI_2);
                    dmath::sin_f64(std::f64::consts::FRAC_PI_2 * s * s) as f32
                })
                .collect()
        };
        let n1 = bs1 as usize;
        Ok(Decoder {
            info,
            setup,
            audio_start: reader.clone(),
            audio_start_page: start_page,
            reader,
            imdct: [mdct::Imdct::new(bs0 as usize), mdct::Imdct::new(n1)],
            slopes: [slope(bs0 as usize), slope(n1)],
            overlap: vec![Vec::with_capacity(n1 / 2); channels],
            prev_n: 0,
            out: vec![Vec::new(); channels],
            out_pos: 0,
            pos: 0,
            known: false,
            fresh: true,
            target: 0,
            ended: false,
            packet,
            floors: vec![vec![0.0; n1 / 2]; channels],
            residues: vec![vec![0.0; n1 / 2]; channels],
            floor_unused: vec![false; channels],
            no_residue: vec![false; channels],
            y: vec![0.0; n1],
            fscratch: FloorScratch::default(),
            rscratch: ResidueScratch::default(),
            stats: DecodeStats::default(),
        })
    }

    pub fn info(&self) -> &Info {
        &self.info
    }

    pub fn channels(&self) -> usize {
        self.info.channels as usize
    }

    /// The frame the next [`Decoder::read`] returns.
    pub fn position(&self) -> u64 {
        if self.known {
            self.pos
        } else {
            self.target
        }
    }

    /// Decodes up to `out.len() / channels` frames, interleaved; returns the frames written
    /// (0 at the end of the stream).
    pub fn read(&mut self, out: &mut [f32]) -> Result<usize> {
        let ch = self.channels();
        let want = out.len() / ch;
        let mut done = 0;
        while done < want {
            if self.out_pos >= self.out[0].len() || !self.known {
                if !self.fill()? {
                    break;
                }
                continue;
            }
            let avail = self.out[0].len() - self.out_pos;
            let n = avail.min(want - done);
            for c in 0..ch {
                let src = &self.out[c][self.out_pos..self.out_pos + n];
                for (i, &s) in src.iter().enumerate() {
                    out[(done + i) * ch + c] = s;
                }
            }
            self.out_pos += n;
            // Positions come from the stream's granules: hostile ones may sit near u64::MAX.
            self.pos = self.pos.saturating_add(n as u64);
            done += n;
        }
        Ok(done)
    }

    /// Moves to frame `frame` (the next read starts exactly there).
    pub fn seek(&mut self, frame: u64) -> Result<()> {
        if let Some(total) = self.info.total_frames {
            if frame > total {
                return Err(AudioError::new(ErrorCode::SeekOutOfRange, "seek past the end of the stream").with_numbers(frame as f64, total as f64));
            }
        }
        self.stats.seeks += 1;
        // Start decoding a few blocks early: the first packet only primes the overlap, and the
        // page granule is the end of the page's last packet.
        let preroll = 4 * self.info.blocksizes[1] as u64;
        let want = frame.saturating_sub(preroll);
        let mut best: Option<usize> = None;
        // The whole bisection's page searches share one CRC budget (hostile files full of OggS
        // patterns can't make a seek slow).
        let mut budget = 2 * ogg::RESYNC_BUDGET;
        if want > 0 {
            let bytes = self.reader.bytes().clone();
            let (mut lo, mut hi) = (self.audio_start_page, bytes.len());
            let mut guard = 0;
            while lo < hi && guard < 128 {
                guard += 1;
                let mid = lo + (hi - lo) / 2;
                let mut at = mid;
                let found = loop {
                    match self.reader.page_at_or_after(at, &mut budget) {
                        Some((off, g, len)) if off < hi => {
                            if g == NO_GRANULE {
                                at = off + len;
                                continue;
                            }
                            break Some((off, g, len));
                        }
                        _ => break None,
                    }
                };
                match found {
                    Some((off, g, len)) if g <= want => {
                        // Resume on this page itself: the packets that begin on it end with its
                        // granule, which fixes the position as soon as the page is read.
                        best = Some(off);
                        lo = off + len;
                    }
                    _ => hi = mid,
                }
            }
        }
        self.reader = self.audio_start.clone();
        if let Some(page) = best {
            self.reader.seek_to_page(page);
        }
        self.reset(frame);
        self.fresh = best.is_none();
        Ok(())
    }

    fn reset(&mut self, target: u64) {
        self.prev_n = 0;
        for o in &mut self.overlap {
            o.clear();
        }
        for o in &mut self.out {
            o.clear();
        }
        self.out_pos = 0;
        self.known = false;
        self.target = target;
        self.ended = false;
    }

    /// Decodes packets until frames are ready at a known position; `Ok(false)` at the end.
    fn fill(&mut self) -> Result<bool> {
        if self.out_pos >= self.out[0].len() && self.known {
            for o in &mut self.out {
                o.clear();
            }
            self.out_pos = 0;
        }
        loop {
            if self.ended {
                if !self.known {
                    // The stream ended before a granule: assume the target is where we are.
                    self.known = true;
                    self.pos = self.target;
                    self.skip_to_target();
                    return Ok(self.out_pos < self.out[0].len());
                }
                return Ok(false);
            }
            let mut packet = std::mem::take(&mut self.packet);
            let info = self.reader.next_packet(&mut packet);
            self.stats.pages_skipped = self.reader.pages_skipped;
            let info = match info {
                Ok(Some(i)) => i,
                Ok(None) => {
                    self.packet = packet;
                    self.ended = true;
                    if self.reader.resync_exhausted && !self.stats.resync_exhausted {
                        // Too much damage to search through: say so rather than end quietly.
                        self.stats.resync_exhausted = true;
                        return Err(AudioError::new(ErrorCode::Limit, "the stream is too damaged to resync (gave up after the resync budget)").with_numbers(ogg::RESYNC_BUDGET as f64, ogg::RESYNC_BUDGET as f64));
                    }
                    continue;
                }
                Err(e) => {
                    self.packet = packet;
                    self.ended = true;
                    if e.code == ErrorCode::Limit {
                        return Err(e);
                    }
                    continue;
                }
            };
            self.stats.packets += 1;
            if self.audio_packet(&packet).is_err() {
                self.stats.bad_packets += 1;
            }
            self.packet = packet;
            if !self.known {
                if info.eos && !self.fresh {
                    // A seek that resolves only on the last page can't tell how much of it the
                    // end trim removes: decode that stretch from the start instead (rare).
                    self.reader = self.audio_start.clone();
                    let target = self.target;
                    self.reset(target);
                    self.fresh = true;
                    continue;
                }
                if let Some(g) = info.granule {
                    let pending = (self.out[0].len() - self.out_pos) as u64;
                    if info.eos && self.fresh {
                        // The first audio page is also the last: the spec cuts the end, not
                        // the beginning (as libvorbis does).
                        self.pos = 0;
                    } else if g < pending {
                        // Leading samples the stream doesn't count (encoder priming).
                        self.out_pos += (pending - g) as usize;
                        self.pos = 0;
                    } else {
                        self.pos = g - pending;
                    }
                    self.known = true;
                    if info.eos {
                        self.trim_end(g);
                    }
                    self.skip_to_target();
                } else if self.out[0].len() - self.out_pos > MAX_PENDING {
                    // No granule for far too long: trust the target.
                    self.known = true;
                    self.pos = self.target;
                }
            } else if info.eos {
                if let Some(g) = info.granule {
                    self.trim_end(g);
                }
            }
            if self.known {
                // A seek target can lie several packets past the page that fixed the position.
                self.skip_to_target();
            }
            if self.known && self.out_pos < self.out[0].len() {
                return Ok(true);
            }
        }
    }

    /// Drops buffered frames past absolute frame `end`.
    fn trim_end(&mut self, end: u64) {
        let buffered = (self.out[0].len() - self.out_pos) as u64;
        if self.pos.saturating_add(buffered) > end {
            let keep = end.saturating_sub(self.pos).min(buffered) as usize;
            for o in &mut self.out {
                o.truncate(self.out_pos + keep);
            }
        }
    }

    /// Drops buffered frames before the seek target.
    fn skip_to_target(&mut self) {
        if self.pos < self.target {
            let buffered = (self.out[0].len() - self.out_pos) as u64;
            let skip = (self.target - self.pos).min(buffered);
            self.out_pos += skip as usize;
            self.pos += skip;
        }
        if self.out_pos >= self.out[0].len() {
            for o in &mut self.out {
                o.clear();
            }
            self.out_pos = 0;
        }
    }

    /// Decodes one audio packet and appends its finished frames to `out`.
    fn audio_packet(&mut self, data: &[u8]) -> Result<()> {
        let mut b = Bits::new(data);
        if b.bit() {
            return Err(bad_packet("not an audio packet"));
        }
        let setup = &self.setup;
        let mode_n = b.read(ilog(setup.modes.len() as u32 - 1)) as usize;
        let Some(mode) = setup.modes.get(mode_n) else { return Err(bad_packet("mode number out of range")) };
        let blockflag = mode.blockflag;
        let n = self.info.blocksizes[blockflag as usize] as usize;
        let (prev_flag, next_flag) = if blockflag { (b.bit(), b.bit()) } else { (false, false) };
        if b.eop() {
            return Err(bad_packet("packet ends in its header"));
        }
        let mapping = &setup.mappings[mode.mapping];
        let half = n / 2;
        let ch = self.info.channels as usize;
        // Floors.
        for c in 0..ch {
            let floor = &setup.floors[mapping.submaps[mapping.mux[c]].0];
            let r = floor.decode(&mut b, &setup.books, half, &mut self.floors[c][..half], &mut self.fscratch);
            self.floor_unused[c] = matches!(r, FloorOut::Unused);
            self.no_residue[c] = self.floor_unused[c];
        }
        // Nonzero propagation through coupling.
        for &(m, a) in &mapping.coupling {
            if !self.no_residue[m] || !self.no_residue[a] {
                self.no_residue[m] = false;
                self.no_residue[a] = false;
            }
        }
        // Residues, per submap.
        for r in &mut self.residues {
            r[..half].fill(0.0);
        }
        for (s, &(_, res)) in mapping.submaps.iter().enumerate() {
            let mut vecs: Vec<&mut [f32]> = Vec::with_capacity(ch);
            let mut skip = [false; MAX_CHANNELS];
            let mut k = 0;
            for (c, v) in self.residues.iter_mut().enumerate() {
                if mapping.mux[c] == s {
                    skip[k] = self.no_residue[c];
                    vecs.push(&mut v[..half]);
                    k += 1;
                }
            }
            setup.residues[res].decode(&mut b, &setup.books, &mut vecs, &skip[..k], half, &mut self.rscratch);
        }
        // Inverse coupling, last step first.
        for &(m, a) in mapping.coupling.iter().rev() {
            let (mv, av) = two_mut(&mut self.residues, m, a);
            for (mm, aa) in mv[..half].iter_mut().zip(av[..half].iter_mut()) {
                let (x, y) = (*mm, *aa);
                let (nm, na) = if x > 0.0 {
                    if y > 0.0 {
                        (x, x - y)
                    } else {
                        (x + y, x)
                    }
                } else if y > 0.0 {
                    (x, x + y)
                } else {
                    (x - y, x)
                };
                *mm = nm;
                *aa = na;
            }
        }
        // Floor × residue, inverse MDCT, window, overlap-add.
        let short = self.info.blocksizes[0] as usize;
        let (ls, le) = if blockflag && !prev_flag { (n / 4 - short / 4, n / 4 + short / 4) } else { (0, half) };
        let (rs, re) = if blockflag && !next_flag { (3 * n / 4 - short / 4, 3 * n / 4 + short / 4) } else { (half, n) };
        let lslope = if le - ls == half { &self.slopes[blockflag as usize] } else { &self.slopes[0] };
        let rslope = if re - rs == half { &self.slopes[blockflag as usize] } else { &self.slopes[0] };
        let prev_n = self.prev_n;
        let produce = prev_n / 4 + n / 4;
        let d = prev_n as isize / 4 - n as isize / 4;
        for c in 0..ch {
            let res = &mut self.residues[c][..half];
            if self.floor_unused[c] {
                res.fill(0.0);
            } else {
                for (r, f) in res.iter_mut().zip(&self.floors[c][..half]) {
                    *r *= *f;
                }
            }
            let y = &mut self.y[..n];
            self.imdct[blockflag as usize].inverse(res, y);
            y[..ls].fill(0.0);
            for (i, v) in y[ls..le].iter_mut().enumerate() {
                *v *= lslope[i];
            }
            let rl = re - rs;
            for (i, v) in y[rs..re].iter_mut().enumerate() {
                *v *= rslope[rl - 1 - i];
            }
            y[re..].fill(0.0);
            if prev_n > 0 {
                let ov = &self.overlap[c];
                let out = &mut self.out[c];
                out.reserve(produce);
                for i in 0..produce {
                    let mut v = ov.get(i).copied().unwrap_or(0.0);
                    let j = i as isize - d;
                    if j >= 0 && (j as usize) < n {
                        v += y[j as usize];
                    }
                    if !v.is_finite() {
                        v = 0.0;
                        self.stats.non_finite += 1;
                    }
                    out.push(v);
                }
            }
            let ov = &mut self.overlap[c];
            ov.clear();
            ov.extend_from_slice(&y[half..n]);
        }
        self.prev_n = n;
        Ok(())
    }
}

fn two_mut(v: &mut [Vec<f32>], a: usize, b: usize) -> (&mut Vec<f32>, &mut Vec<f32>) {
    debug_assert!(a != b);
    if a < b {
        let (x, y) = v.split_at_mut(b);
        (&mut x[a], &mut y[0])
    } else {
        let (x, y) = v.split_at_mut(a);
        (&mut y[0], &mut x[b])
    }
}

/// The offset of the page the reader reads next (the first audio page after the headers).
fn reader_next_page(r: &PacketReader) -> usize {
    // A fresh clone's next packet starts at its current page; finding it by reading is the
    // simplest exact way.
    let mut probe = r.clone();
    let mut buf = Vec::new();
    match probe.next_packet(&mut buf) {
        Ok(Some(info)) => info.page_offset,
        _ => r.bytes().len(),
    }
}

/// The granule position of the last page of stream `serial` (the stream's length in frames).
fn last_granule(bytes: &[u8], serial: u32) -> Option<u64> {
    // Pages are at most 65307 bytes: the last one starts in the final 64 KiB (or a little more).
    let mut from = bytes.len().saturating_sub(70_000);
    let mut last = None;
    let mut budget = ogg::RESYNC_BUDGET;
    while let Some(at) = ogg::find_page_within(bytes, from, &mut budget) {
        let p = ogg::parse_page(bytes, at).ok()?;
        if p.serial == serial && p.granule != NO_GRANULE {
            last = Some(p.granule);
        }
        from = at + p.len;
    }
    last
}

/// Loop points from `LOOPSTART` and `LOOPEND` (or `LOOPLENGTH`) comments: kept only when the
/// start comes before the end and (when the length is known) the end is inside the stream.
/// Hostile values never overflow (review: `LOOPSTART=u64::MAX` with `LOOPLENGTH=1` panicked).
fn loop_points(start: Option<u64>, end: Option<u64>, length: Option<u64>, total: Option<u64>) -> Option<(u64, u64)> {
    let start = start?;
    let end = end.or_else(|| start.checked_add(length?))?;
    (start < end && total.is_none_or(|t| end <= t)).then_some((start, end))
}

fn parse_comments(data: &[u8]) -> Result<(String, Vec<(String, String)>)> {
    let path = |s: &str| format!("comment.{s}");
    let mut at = 0usize;
    let mut take = |n: usize, what: &str| -> Result<&[u8]> {
        let end = at.checked_add(n).filter(|&e| e <= data.len()).ok_or_else(|| header_err(path(what), "comment header ends early"))?;
        let s = &data[at..end];
        at = end;
        Ok(s)
    };
    let le = |b: &[u8]| u32::from_le_bytes(b.try_into().expect("4 bytes")) as usize;
    let cut = |b: &[u8]| String::from_utf8_lossy(&b[..b.len().min(MAX_COMMENT_LEN)]).into_owned();
    let vlen = le(take(4, "vendor_length")?);
    let vendor = cut(take(vlen, "vendor")?);
    let count = le(take(4, "count")?);
    let mut comments = Vec::new();
    for _ in 0..count {
        let n = le(take(4, "comment_length")?);
        let c = take(n, "comment")?;
        if comments.len() < MAX_COMMENTS {
            let s = cut(c);
            if let Some((k, v)) = s.split_once('=') {
                comments.push((k.to_ascii_uppercase(), v.to_owned()));
            }
        }
    }
    // The framing bit is often missing in the wild; don't insist on it.
    Ok((vendor, comments))
}

fn parse_setup(data: &[u8], channels: usize) -> Result<Setup> {
    let mut b = Bits::new(data);
    let mut limits = Limits { used: 0 };
    let n = b.read(8) as usize + 1;
    let mut books = Vec::with_capacity(n);
    for i in 0..n {
        books.push(Codebook::read(&mut b, i, &mut limits)?);
    }
    let times = b.read(6) + 1;
    for i in 0..times {
        if b.read(16) != 0 {
            return Err(header_err(format!("setup.times[{i}]"), "time domain transforms must be 0"));
        }
    }
    let n = b.read(6) as usize + 1;
    let mut floors = Vec::with_capacity(n);
    for i in 0..n {
        floors.push(Floor::read(&mut b, i, &books)?);
    }
    let n = b.read(6) as usize + 1;
    let mut residues = Vec::with_capacity(n);
    for i in 0..n {
        residues.push(Residue::read(&mut b, i, &books)?);
    }
    let n = b.read(6) as usize + 1;
    let mut mappings = Vec::with_capacity(n);
    for i in 0..n {
        let path = || format!("setup.mappings[{i}]");
        if b.read(16) != 0 {
            return Err(header_err(path(), "unknown mapping type"));
        }
        let submaps = if b.bit() { b.read(4) as usize + 1 } else { 1 };
        let mut coupling = Vec::new();
        if b.bit() {
            let steps = b.read(8) as usize + 1;
            let bits = ilog(channels as u32 - 1);
            for _ in 0..steps {
                let m = b.read(bits) as usize;
                let a = b.read(bits) as usize;
                if m == a || m >= channels || a >= channels {
                    return Err(header_err(path(), "bad channel coupling"));
                }
                coupling.push((m, a));
            }
        }
        if b.read(2) != 0 {
            return Err(header_err(path(), "reserved mapping bits must be 0"));
        }
        let mut mux = vec![0usize; channels];
        if submaps > 1 {
            for m in &mut mux {
                *m = b.read(4) as usize;
                if *m >= submaps {
                    return Err(header_err(path(), "channel mux names a submap that doesn't exist"));
                }
            }
        }
        let mut subs = Vec::with_capacity(submaps);
        for _ in 0..submaps {
            b.read(8);
            let f = b.read(8) as usize;
            let r = b.read(8) as usize;
            if f >= floors.len() || r >= residues.len() {
                return Err(header_err(path(), "submap names a floor or residue that doesn't exist"));
            }
            subs.push((f, r));
        }
        mappings.push(Mapping { submaps: subs, mux, coupling });
    }
    let n = b.read(6) as usize + 1;
    let mut modes = Vec::with_capacity(n);
    for i in 0..n {
        let blockflag = b.bit();
        let window = b.read(16);
        let transform = b.read(16);
        let mapping = b.read(8) as usize;
        if window != 0 || transform != 0 || mapping >= mappings.len() {
            return Err(header_err(format!("setup.modes[{i}]"), "bad mode (window and transform must be 0, mapping must exist)"));
        }
        modes.push(Mode { blockflag, mapping });
    }
    if !b.bit() || b.eop() {
        return Err(header_err("setup.framing".into(), "setup header ends early or misses its framing bit"));
    }
    Ok(Setup { books, floors, residues, mappings, modes })
}

/// A whole decoded file.
#[derive(Clone, Debug, PartialEq)]
pub struct Decoded {
    pub info: Info,
    /// Interleaved samples in Vorbis channel order.
    pub samples: Vec<f32>,
    pub stats: DecodeStats,
}

/// Decodes a whole file, refusing to produce more than `max_frames` frames.
pub fn decode(bytes: impl Into<Arc<[u8]>>, max_frames: usize) -> Result<Decoded> {
    let mut d = Decoder::new(bytes)?;
    let ch = d.channels();
    if let Some(t) = d.info.total_frames {
        if t as usize > max_frames {
            return Err(AudioError::new(ErrorCode::Limit, "the stream is longer than allowed").with_numbers(t as f64, max_frames as f64));
        }
    }
    let mut samples = Vec::with_capacity(d.info.total_frames.map_or(0, |t| t as usize * ch));
    let mut buf = vec![0f32; 4096 * ch];
    loop {
        let n = d.read(&mut buf)?;
        if n == 0 {
            break;
        }
        if samples.len() / ch + n > max_frames {
            return Err(AudioError::new(ErrorCode::Limit, "the stream is longer than allowed").with_numbers((samples.len() / ch + n) as f64, max_frames as f64));
        }
        samples.extend_from_slice(&buf[..n * ch]);
    }
    Ok(Decoded { info: d.info.clone(), samples, stats: d.stats })
}

/// Folds Vorbis channel order down to stereo (L R): mono is copied to both sides; 3+ channels
/// use the Vorbis layouts (centre at -3 dB to both sides, surrounds at -6 dB, LFE dropped).
pub fn downmix_stereo(samples: &[f32], channels: usize, out: &mut Vec<f32>) {
    out.clear();
    match channels {
        1 => {
            for &s in samples {
                out.push(s);
                out.push(s);
            }
        }
        2 => out.extend_from_slice(samples),
        _ => {
            const C: f32 = std::f32::consts::FRAC_1_SQRT_2;
            for f in samples.chunks_exact(channels) {
                // Vorbis order: 3 = L C R; 4 = FL FR RL RR; 5 = FL C FR RL RR; 6 = + LFE;
                // 7 = FL C FR SL SR RC LFE; 8 = FL C FR SL SR RL RR LFE.
                let (l, r) = match channels {
                    3 => (f[0] + C * f[1], f[2] + C * f[1]),
                    4 => (f[0] + 0.5 * f[2], f[1] + 0.5 * f[3]),
                    5 | 6 => (f[0] + C * f[1] + 0.5 * f[3], f[2] + C * f[1] + 0.5 * f[4]),
                    7 => (f[0] + C * f[1] + 0.5 * f[3] + 0.35 * f[5], f[2] + C * f[1] + 0.5 * f[4] + 0.35 * f[5]),
                    _ => (f[0] + C * f[1] + 0.5 * (f[3] + f[5]), f[2] + C * f[1] + 0.5 * (f[4] + f[6])),
                };
                out.push(l);
                out.push(r);
            }
        }
    }
}
