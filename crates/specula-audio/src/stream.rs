//! Streaming playback: long sounds (music, ambience beds, Vorbis files) decoded a block at a time
//! on the audio worker thread into a bounded ring the mixer reads, never on the audio or game
//! thread.
//!
//! ```text
//! StreamSource ──(worker: read, loop, resample to the mix rate)──▶ full blocks ──▶ mixer voice
//!                                         ▲                                          │
//!                                         └──────────── empty blocks ◀────────────────┘
//! ```
//!
//! Blocks are allocated once, when the stream is made, and travel between the two threads in
//! two SPSC rings, so neither side allocates or frees while playing. If the ring runs dry the
//! mixer plays silence (never stale or garbage samples), counts the frames and the event in
//! [`StreamStatus`], and resumes when blocks arrive. A stream buffers [`PREFILL_BLOCKS`] before
//! it starts, so a cold start isn't counted as starvation.

use crate::error::{AudioError, ErrorCode, Result};
use crate::resample::{dot8, SincTable, FRAC_BITS, ONE, TAPS};
use crate::ring::{self, Consumer, Producer};
use crate::sound::Sound;
use crate::vorbis;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Frames per ring block (21 ms at 48 kHz).
pub const BLOCK_FRAMES: usize = 1024;
/// Blocks per stream ring (about 0.34 s at 48 kHz).
pub const RING_BLOCKS: usize = 16;
/// Blocks buffered before a stream starts playing.
pub const PREFILL_BLOCKS: usize = 4;
/// The most samples a second (rate × channels) a stream's source may decode: 8 channels at
/// 96 kHz. Denser files are refused rather than taking the worker's whole core.
pub const MAX_STREAM_SAMPLES_PER_S: u64 = 768_000;

/// Produces interleaved PCM for a stream, on the worker thread.
pub trait StreamSource: Send {
    fn name(&self) -> &str;
    fn rate(&self) -> u32;
    /// 1 or 2 (the worker makes mono into stereo).
    fn channels(&self) -> usize;
    /// Fills `out` (interleaved); returns frames written, 0 at the end.
    fn read(&mut self, out: &mut [f32]) -> Result<usize>;
    /// Moves to `frame`.
    fn seek(&mut self, frame: u64) -> Result<()>;
    fn total_frames(&self) -> Option<u64>;
    /// Loop points the source itself names (an Ogg file's `LOOPSTART`/`LOOPEND` comments), used
    /// when a looping stream's params set none.
    fn loop_points(&self) -> Option<(u64, u64)> {
        None
    }
}

/// An Ogg Vorbis file (any channel count, folded to stereo).
pub struct VorbisSource {
    name: String,
    dec: vorbis::Decoder,
    scratch: Vec<f32>,
    folded: Vec<f32>,
}

impl VorbisSource {
    pub fn new(name: impl Into<String>, bytes: impl Into<Arc<[u8]>>) -> Result<VorbisSource> {
        let dec = vorbis::Decoder::new(bytes)?;
        let per_second = u64::from(dec.info().rate) * dec.channels() as u64;
        if per_second > MAX_STREAM_SAMPLES_PER_S {
            return Err(AudioError::new(ErrorCode::Limit, "the file has more samples a second than a stream decodes (8 channels at 96 kHz at most)").with_numbers(per_second as f64, MAX_STREAM_SAMPLES_PER_S as f64));
        }
        Ok(VorbisSource { name: name.into(), dec, scratch: Vec::new(), folded: Vec::new() })
    }

    pub fn info(&self) -> &vorbis::Info {
        self.dec.info()
    }

    pub fn stats(&self) -> vorbis::DecodeStats {
        self.dec.stats
    }
}

impl StreamSource for VorbisSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn rate(&self) -> u32 {
        self.dec.info().rate
    }

    fn channels(&self) -> usize {
        self.dec.channels().min(2)
    }

    fn read(&mut self, out: &mut [f32]) -> Result<usize> {
        let ch = self.dec.channels();
        if ch <= 2 {
            return self.dec.read(out);
        }
        let frames = out.len() / 2;
        self.scratch.resize(frames * ch, 0.0);
        let n = self.dec.read(&mut self.scratch)?;
        vorbis::downmix_stereo(&self.scratch[..n * ch], ch, &mut self.folded);
        out[..n * 2].copy_from_slice(&self.folded[..n * 2]);
        Ok(n)
    }

    fn seek(&mut self, frame: u64) -> Result<()> {
        self.dec.seek(frame)
    }

    fn total_frames(&self) -> Option<u64> {
        self.dec.info().total_frames
    }

    fn loop_points(&self) -> Option<(u64, u64)> {
        let i = self.dec.info();
        i.loop_start.zip(i.loop_end)
    }
}

/// An in-memory sound played as a stream (cues, tests).
pub struct SoundSource {
    sound: Sound,
    pos: usize,
}

impl SoundSource {
    pub fn new(sound: Sound) -> SoundSource {
        SoundSource { sound, pos: 0 }
    }
}

impl StreamSource for SoundSource {
    fn name(&self) -> &str {
        self.sound.name()
    }

    fn rate(&self) -> u32 {
        self.sound.rate()
    }

    fn channels(&self) -> usize {
        self.sound.channels() as usize
    }

    fn read(&mut self, out: &mut [f32]) -> Result<usize> {
        let ch = self.channels();
        let frames = (out.len() / ch).min(self.sound.frames() - self.pos.min(self.sound.frames()));
        out[..frames * ch].copy_from_slice(&self.sound.samples()[self.pos * ch..(self.pos + frames) * ch]);
        self.pos += frames;
        Ok(frames)
    }

    fn seek(&mut self, frame: u64) -> Result<()> {
        if frame > self.sound.frames() as u64 {
            return Err(AudioError::new(ErrorCode::SeekOutOfRange, "seek past the end of the sound").with_numbers(frame as f64, self.sound.frames() as f64));
        }
        self.pos = frame as usize;
        Ok(())
    }

    fn total_frames(&self) -> Option<u64> {
        Some(self.sound.frames() as u64)
    }
}

/// How a stream plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreamParams {
    pub bus: crate::Bus,
    pub gain: f32,
    /// Stereo balance -1..1.
    pub pan: f32,
    pub fade_in_s: f32,
    /// Loop forever between `loop_start` and `loop_end` (the source's end when `None`).
    pub looping: bool,
    pub loop_start: u64,
    pub loop_end: Option<u64>,
    /// Source frame to start at.
    pub start_at: u64,
}

impl Default for StreamParams {
    fn default() -> Self {
        StreamParams { bus: crate::Bus::Music, gain: 1.0, pan: 0.0, fade_in_s: 0.0, looping: false, loop_start: 0, loop_end: None, start_at: 0 }
    }
}

/// Counters shared by a stream's worker side, its mixer voice, and whoever asks.
#[derive(Default)]
pub(crate) struct StreamShared {
    pub produced: AtomicU64,
    pub played: AtomicU64,
    pub starved_frames: AtomicU64,
    pub starve_events: AtomicU64,
    pub loops: AtomicU64,
    /// Source frame the worker reads next.
    pub source_pos: AtomicU64,
    /// The worker has delivered the last block.
    pub source_done: AtomicBool,
    /// The voice is done (played out, stopped or rejected); the worker drops its side.
    pub finished: AtomicBool,
    /// The voice has started playing (prefill reached).
    pub started: AtomicBool,
    pub buffered_blocks: AtomicU32,
    pub error: Mutex<Option<AudioError>>,
}

/// A snapshot of a stream, for agents and traces.
#[derive(Clone, Debug, PartialEq)]
pub struct StreamStatus {
    pub name: String,
    pub produced_frames: u64,
    pub played_frames: u64,
    /// Frames played as silence because the ring ran dry (should stay 0).
    pub starved_frames: u64,
    pub starve_events: u64,
    pub loops: u64,
    pub source_position: u64,
    pub buffered_blocks: u32,
    pub started: bool,
    pub source_done: bool,
    pub finished: bool,
    pub error: Option<AudioError>,
}

/// The caller's handle on a stream (cheap to clone; read its status any time).
#[derive(Clone)]
pub struct StreamHandle {
    pub(crate) name: Arc<str>,
    pub(crate) shared: Arc<StreamShared>,
}

impl std::fmt::Debug for StreamHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamHandle").field("name", &self.name).finish_non_exhaustive()
    }
}

impl StreamHandle {
    pub fn status(&self) -> StreamStatus {
        let s = &self.shared;
        StreamStatus {
            name: self.name.to_string(),
            produced_frames: s.produced.load(Ordering::Relaxed),
            played_frames: s.played.load(Ordering::Relaxed),
            starved_frames: s.starved_frames.load(Ordering::Relaxed),
            starve_events: s.starve_events.load(Ordering::Relaxed),
            loops: s.loops.load(Ordering::Relaxed),
            source_position: s.source_pos.load(Ordering::Relaxed),
            buffered_blocks: s.buffered_blocks.load(Ordering::Relaxed),
            started: s.started.load(Ordering::Relaxed),
            source_done: s.source_done.load(Ordering::Relaxed),
            finished: s.finished.load(Ordering::Relaxed),
            error: s.error.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

pub(crate) struct Block {
    pub samples: Box<[f32]>,
    pub frames: usize,
    /// The source ended after these frames.
    pub last: bool,
}

/// The mixer's end of a stream.
pub(crate) struct StreamReader {
    full: Consumer<Box<Block>>,
    empty: Producer<Box<Block>>,
    current: Option<Box<Block>>,
    offset: usize,
    pub shared: Arc<StreamShared>,
    /// The output rate the worker resamples to (the mixer checks it matches).
    pub started: bool,
    pub ended: bool,
}

impl StreamReader {
    /// Copies up to `out.len() / 2` stereo frames; returns `(frames, starved)`: frames copied
    /// before the ring ran dry, and whether it did (the rest of `out` is then silence). Never
    /// allocates or frees.
    pub fn read(&mut self, out: &mut [f32]) -> (usize, bool) {
        let want = out.len() / 2;
        if !self.started {
            if self.full.len() >= PREFILL_BLOCKS.min(RING_BLOCKS - 1) || self.shared.source_done.load(Ordering::Acquire) {
                self.started = true;
                self.shared.started.store(true, Ordering::Relaxed);
            } else {
                out.fill(0.0);
                return (0, false);
            }
        }
        let mut done = 0;
        while done < want {
            if self.current.is_none() {
                match self.full.pop() {
                    Some(b) => {
                        self.current = Some(b);
                        self.offset = 0;
                    }
                    None => break,
                }
            }
            let b = self.current.as_ref().expect("a block");
            let n = (b.frames - self.offset).min(want - done);
            out[done * 2..(done + n) * 2].copy_from_slice(&b.samples[self.offset * 2..(self.offset + n) * 2]);
            self.offset += n;
            done += n;
            if self.offset >= b.frames {
                let b = self.current.take().expect("a block");
                let last = b.last;
                // Capacity holds every block, so this never fails; if it did, the block would be
                // dropped later with the reader, on the game thread.
                if let Err(b) = self.empty.push(b) {
                    self.current = Some(b);
                    self.offset = self.current.as_ref().map_or(0, |b| b.frames);
                }
                if last {
                    self.ended = true;
                    break;
                }
            }
        }
        out[done * 2..].fill(0.0);
        self.shared.played.fetch_add(done as u64, Ordering::Relaxed);
        self.shared.buffered_blocks.store(self.full.len() as u32, Ordering::Relaxed);
        let starved = done < want && !self.ended;
        if starved {
            self.shared.starved_frames.fetch_add((want - done) as u64, Ordering::Relaxed);
        }
        (done, starved)
    }
}

/// The worker's end of a stream.
pub(crate) struct StreamWriter {
    source: LoopSource,
    full: Producer<Box<Block>>,
    empty: Consumer<Box<Block>>,
    resampler: Resampler,
    pub shared: Arc<StreamShared>,
    done: bool,
}

/// Makes the two ends of a stream with every block allocated up front.
pub(crate) fn channel(source: Box<dyn StreamSource>, params: &StreamParams, out_rate: u32) -> Result<(StreamWriter, StreamReader, StreamHandle)> {
    let ch = source.channels();
    if !(1..=2).contains(&ch) {
        return Err(AudioError::new(ErrorCode::BadSound, "a stream source must give mono or stereo").with_numbers(ch as f64, 2.0));
    }
    if !(crate::sound::MIN_RATE..=crate::sound::MAX_RATE).contains(&source.rate()) {
        return Err(AudioError::new(ErrorCode::BadSound, "stream sample rate out of range").with_numbers(source.rate() as f64, crate::sound::MAX_RATE as f64));
    }
    let name: Arc<str> = source.name().into();
    // A looping stream without loop points of its own loops where the source says (Ogg comments).
    let mut p = *params;
    if p.looping && p.loop_start == 0 && p.loop_end.is_none() {
        if let Some((s, e)) = source.loop_points() {
            p.loop_start = s;
            p.loop_end = Some(e);
        }
    }
    let params = &p;
    let mut looped = LoopSource::new(source, params)?;
    if params.start_at > 0 {
        looped.seek(params.start_at)?;
    }
    let (full_tx, full_rx) = ring::channel(RING_BLOCKS);
    let (mut empty_tx, empty_rx) = ring::channel(RING_BLOCKS);
    for _ in 0..RING_BLOCKS {
        let b = Box::new(Block { samples: vec![0.0; BLOCK_FRAMES * 2].into_boxed_slice(), frames: 0, last: false });
        if empty_tx.push(b).is_err() {
            break;
        }
    }
    let shared = Arc::new(StreamShared::default());
    shared.source_pos.store(params.start_at, Ordering::Relaxed);
    let resampler = Resampler::new(looped.rate(), out_rate, looped.channels());
    let writer = StreamWriter { source: looped, full: full_tx, empty: empty_rx, resampler, shared: shared.clone(), done: false };
    let reader = StreamReader { full: full_rx, empty: empty_tx, current: None, offset: 0, shared: shared.clone(), started: false, ended: false };
    Ok((writer, reader, StreamHandle { name, shared }))
}

impl StreamWriter {
    /// Fills every free block; returns whether any work was done. `out_rate` is the mixer's
    /// current rate (a device can reconnect at another one).
    pub fn pump(&mut self, out_rate: u32) -> bool {
        if self.done || self.shared.finished.load(Ordering::Acquire) {
            return false;
        }
        self.resampler.set_rate(self.source.rate(), out_rate);
        let mut worked = false;
        while let Some(mut b) = self.empty.pop() {
            worked = true;
            let (frames, end) = match self.resampler.fill(&mut self.source, &mut b.samples) {
                Ok(r) => r,
                Err(e) => {
                    *self.shared.error.lock().unwrap_or_else(|p| p.into_inner()) = Some(e);
                    (0, true)
                }
            };
            b.frames = frames;
            b.last = end;
            self.shared.produced.fetch_add(frames as u64, Ordering::Relaxed);
            self.shared.loops.store(self.source.loops, Ordering::Relaxed);
            self.shared.source_pos.store(self.source.pos, Ordering::Relaxed);
            if self.full.push(b).is_err() {
                // Can't happen (both rings hold every block); stop rather than lose track.
                self.done = true;
                return worked;
            }
            if end {
                self.done = true;
                self.shared.source_done.store(true, Ordering::Release);
                return worked;
            }
        }
        worked
    }

    pub fn name(&self) -> &str {
        self.source.inner.name()
    }

    /// The mixer has let go of the stream.
    pub fn finished(&self) -> bool {
        self.shared.finished.load(Ordering::Acquire)
    }
}

/// Loops a source between two frames.
struct LoopSource {
    inner: Box<dyn StreamSource>,
    looping: bool,
    start: u64,
    end: Option<u64>,
    pos: u64,
    loops: u64,
}

impl LoopSource {
    fn new(inner: Box<dyn StreamSource>, p: &StreamParams) -> Result<LoopSource> {
        let total = inner.total_frames();
        let end = match (p.loop_end, total) {
            (Some(e), Some(t)) => Some(e.min(t)),
            (e, t) => e.or(t),
        };
        if p.looping {
            if let Some(e) = end {
                if p.loop_start >= e {
                    return Err(AudioError::new(ErrorCode::OutOfRange, "loop start must be before the loop end").with_numbers(p.loop_start as f64, e as f64));
                }
            }
        }
        if let Some(t) = total {
            if p.start_at > t {
                return Err(AudioError::new(ErrorCode::SeekOutOfRange, "start past the end of the stream").with_numbers(p.start_at as f64, t as f64));
            }
        }
        Ok(LoopSource { inner, looping: p.looping, start: p.loop_start, end, pos: 0, loops: 0 })
    }

    fn rate(&self) -> u32 {
        self.inner.rate()
    }

    fn channels(&self) -> usize {
        self.inner.channels()
    }

    fn seek(&mut self, frame: u64) -> Result<()> {
        self.inner.seek(frame)?;
        self.pos = frame;
        Ok(())
    }

    /// Reads, jumping back to the loop start at the loop end. 0 only at the true end.
    fn read(&mut self, out: &mut [f32]) -> Result<usize> {
        let ch = self.inner.channels();
        let want = out.len() / ch;
        let mut done = 0;
        let mut empty_laps = 0;
        while done < want {
            let mut limit = want - done;
            if self.looping {
                if let Some(e) = self.end {
                    limit = limit.min(e.saturating_sub(self.pos) as usize);
                }
            }
            let n = if limit == 0 { 0 } else { self.inner.read(&mut out[done * ch..(done + limit) * ch])? };
            done += n;
            self.pos += n as u64;
            if n == 0 {
                if !self.looping {
                    break;
                }
                // A loop that yields nothing twice in a row would spin: stop instead.
                empty_laps += 1;
                if empty_laps > 2 {
                    break;
                }
                self.inner.seek(self.start)?;
                self.pos = self.start;
                self.loops += 1;
            } else {
                empty_laps = 0;
            }
        }
        Ok(done)
    }
}

/// A streaming 8-tap sinc resampler to stereo (copies straight through at equal rates).
struct Resampler {
    table: Option<Arc<SincTable>>,
    from: u32,
    to: u32,
    step: u64,
    kernel: usize,
    ch: usize,
    /// Stereo input frames, starting `TAPS / 2 - 1` frames before the read position.
    buf: Vec<f32>,
    /// Read position in `buf` frames, 32.32.
    pos: u64,
    input_done: bool,
    /// Frames of zeros appended after the input ended (to flush the kernel).
    tail: usize,
    read_buf: Vec<f32>,
}

const HISTORY: usize = TAPS / 2 - 1;

impl Resampler {
    fn new(from: u32, to: u32, ch: usize) -> Resampler {
        let mut r = Resampler { table: None, from: 0, to: 0, step: ONE, kernel: 0, ch, buf: vec![0.0; HISTORY * 2], pos: (HISTORY as u64) << FRAC_BITS, input_done: false, tail: 0, read_buf: Vec::new() };
        r.set_rate(from, to);
        r
    }

    fn set_rate(&mut self, from: u32, to: u32) {
        if from == self.from && to == self.to {
            return;
        }
        self.from = from;
        self.to = to.max(1);
        if from != self.to && self.table.is_none() {
            // One table for every stream, built once (at audio start-up), never per stream.
            self.table = Some(crate::resample::shared_table());
        }
        self.step = ((from as f64 / self.to as f64) * ONE as f64) as u64;
        self.kernel = SincTable::kernel_for_step(self.step);
    }

    /// Fills `out` (stereo) from `src`; returns `(frames, ended)`.
    fn fill(&mut self, src: &mut LoopSource, out: &mut [f32]) -> Result<(usize, bool)> {
        let want = out.len() / 2;
        if self.from == self.to && self.buf.len() == HISTORY * 2 && self.pos == (HISTORY as u64) << FRAC_BITS {
            // Same rate and nothing buffered: copy straight through.
            let n = self.read_stereo(src, want)?;
            out[..n * 2].copy_from_slice(&self.read_buf[..n * 2]);
            return Ok((n, n < want));
        }
        let mut done = 0;
        while done < want {
            let idx = (self.pos >> FRAC_BITS) as usize;
            let frames = self.buf.len() / 2;
            if idx + TAPS - HISTORY > frames {
                if self.input_done {
                    if self.tail >= TAPS {
                        return Ok((done, true));
                    }
                    self.buf.extend(std::iter::repeat_n(0.0, TAPS * 2));
                    self.tail += TAPS;
                    continue;
                }
                // Drop consumed input (keep the history), then read more.
                let drop = idx.saturating_sub(HISTORY);
                if drop > 0 {
                    self.buf.drain(..drop * 2);
                    self.pos -= (drop as u64) << FRAC_BITS;
                }
                let n = self.read_stereo(src, BLOCK_FRAMES)?;
                if n == 0 {
                    self.input_done = true;
                }
                let got = self.read_buf[..n * 2].to_vec();
                self.buf.extend_from_slice(&got);
                continue;
            }
            let frac = self.pos as u32;
            let table = self.table.as_ref().expect("a table when rates differ");
            let (cd, t) = table.phase(self.kernel, frac);
            let start = idx - HISTORY;
            let mut l = [0f32; TAPS];
            let mut r = [0f32; TAPS];
            for k in 0..TAPS {
                l[k] = self.buf[(start + k) * 2];
                r[k] = self.buf[(start + k) * 2 + 1];
            }
            out[done * 2] = dot8(cd, t, &l);
            out[done * 2 + 1] = dot8(cd, t, &r);
            done += 1;
            self.pos += self.step;
        }
        Ok((done, false))
    }

    /// Reads up to `frames` from `src` as stereo into `read_buf`.
    fn read_stereo(&mut self, src: &mut LoopSource, frames: usize) -> Result<usize> {
        self.read_buf.resize(frames * 2, 0.0);
        let n = if self.ch == 2 {
            src.read(&mut self.read_buf[..frames * 2])?
        } else {
            let n = src.read(&mut self.read_buf[..frames])?;
            for i in (0..n).rev() {
                let s = self.read_buf[i];
                self.read_buf[2 * i] = s;
                self.read_buf[2 * i + 1] = s;
            }
            n
        };
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, frames: usize, ch: u16) -> Sound {
        let mut s = Vec::new();
        for i in 0..frames {
            let v = (i as f32 * 0.01).sin() * 0.5;
            for _ in 0..ch {
                s.push(v);
            }
        }
        Sound::new("tone", rate, ch, s).unwrap()
    }

    fn drain(w: &mut StreamWriter, r: &mut StreamReader, rate: u32, frames: usize) -> Vec<f32> {
        let mut out = Vec::new();
        let mut buf = vec![0f32; 256 * 2];
        while out.len() < frames * 2 {
            w.pump(rate);
            let (n, starved) = r.read(&mut buf);
            assert!(!starved, "a pumped stream never starves");
            out.extend_from_slice(&buf[..n * 2]);
            if r.ended {
                break;
            }
        }
        out
    }

    #[test]
    fn same_rate_is_bit_exact_and_ends() {
        let s = tone(48_000, 5000, 2);
        let (mut w, mut r, h) = channel(Box::new(SoundSource::new(s.clone())), &StreamParams::default(), 48_000).unwrap();
        let out = drain(&mut w, &mut r, 48_000, 100_000);
        assert_eq!(out, s.samples());
        assert!(r.ended);
        let st = h.status();
        assert_eq!((st.played_frames, st.produced_frames, st.starved_frames), (5000, 5000, 0));
    }

    #[test]
    fn loops_wrap_at_the_loop_points() {
        let s = tone(48_000, 3000, 1);
        let p = StreamParams { looping: true, loop_start: 1000, loop_end: Some(2000), ..StreamParams::default() };
        let (mut w, mut r, h) = channel(Box::new(SoundSource::new(s.clone())), &p, 48_000).unwrap();
        let out = drain(&mut w, &mut r, 48_000, 5000);
        // 0..2000, then 1000..2000 again and again.
        let src = s.samples();
        for f in 0..5000 {
            let want = if f < 2000 { src[f] } else { src[1000 + (f - 2000) % 1000] };
            assert_eq!(out[2 * f], want, "frame {f}");
        }
        assert!(h.status().loops >= 3);
    }

    #[test]
    fn resampling_keeps_pitch_and_length() {
        let s = tone(44_100, 44_100, 2);
        let (mut w, mut r, _) = channel(Box::new(SoundSource::new(s)), &StreamParams::default(), 48_000).unwrap();
        let out = drain(&mut w, &mut r, 48_000, 1 << 20);
        let frames = out.len() / 2;
        assert!((frames as i64 - 48_000).abs() <= 8, "{frames}");
        // The tone's period scales by 48/44.1: zero crossings of sin(0.01 i) every 314.16 input
        // frames become every 341.95 output frames.
        let crossings = out.as_chunks::<2>().0.iter().collect::<Vec<_>>().windows(2).filter(|w| w[0][0] <= 0.0 && w[1][0] > 0.0).count();
        let expect = 48_000.0 / (2.0 * std::f64::consts::PI / 0.01 * 48_000.0 / 44_100.0);
        assert!((crossings as f64 - expect).abs() <= 1.5, "{crossings} vs {expect}");
    }

    #[test]
    fn an_unpumped_stream_starves_into_silence() {
        let s = tone(48_000, 20_000, 2);
        let (mut w, mut r, h) = channel(Box::new(SoundSource::new(s)), &StreamParams::default(), 48_000).unwrap();
        w.pump(48_000);
        let mut buf = vec![1f32; 512 * 2];
        let mut starved_any = false;
        for _ in 0..80 {
            let (_, starved) = r.read(&mut buf);
            starved_any |= starved;
        }
        assert!(starved_any);
        // Once dry, the output is silence, not stale samples.
        let (n, starved) = r.read(&mut buf);
        assert_eq!(n, 0);
        assert!(starved);
        assert!(buf.iter().all(|&s| s == 0.0));
        assert!(h.status().starved_frames > 0);
    }

    /// A source's own loop points (an Ogg's LOOPSTART/LOOPEND) apply when the params set none
    /// (review: they were only reported, never used); params that set some win.
    #[test]
    fn source_loop_points_apply_unless_params_set_them() {
        struct Named(SoundSource);
        impl StreamSource for Named {
            fn name(&self) -> &str {
                self.0.name()
            }
            fn rate(&self) -> u32 {
                self.0.rate()
            }
            fn channels(&self) -> usize {
                self.0.channels()
            }
            fn read(&mut self, out: &mut [f32]) -> Result<usize> {
                self.0.read(out)
            }
            fn seek(&mut self, frame: u64) -> Result<()> {
                self.0.seek(frame)
            }
            fn total_frames(&self) -> Option<u64> {
                self.0.total_frames()
            }
            fn loop_points(&self) -> Option<(u64, u64)> {
                Some((1000, 2000))
            }
        }
        let s = tone(48_000, 3000, 1);
        let src = s.samples();
        let looping = StreamParams { looping: true, ..StreamParams::default() };
        let (mut w, mut r, _) = channel(Box::new(Named(SoundSource::new(s.clone()))), &looping, 48_000).unwrap();
        let out = drain(&mut w, &mut r, 48_000, 4000);
        assert_eq!(out[2 * 2500], src[1500], "wrapped at the source's 2000 to 1000");
        let own = StreamParams { looping: true, loop_start: 500, loop_end: Some(2500), ..StreamParams::default() };
        let (mut w, mut r, _) = channel(Box::new(Named(SoundSource::new(s.clone()))), &own, 48_000).unwrap();
        let out = drain(&mut w, &mut r, 48_000, 4000);
        assert_eq!(out[2 * 2600], src[600], "the params' 2500 to 500");
    }

    #[test]
    fn bad_loop_points_are_refused() {
        let s = tone(48_000, 100, 1);
        let p = StreamParams { looping: true, loop_start: 50, loop_end: Some(50), ..StreamParams::default() };
        assert!(channel(Box::new(SoundSource::new(s.clone())), &p, 48_000).is_err());
        let p = StreamParams { start_at: 101, ..StreamParams::default() };
        assert!(channel(Box::new(SoundSource::new(s)), &p, 48_000).is_err());
    }
}
