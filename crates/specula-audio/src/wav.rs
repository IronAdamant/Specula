//! WAV (RIFF/WAVE) decode and encode.
//!
//! Decodes PCM 8/16/24/32-bit and IEEE float 32/64-bit, plain or `WAVE_FORMAT_EXTENSIBLE`, any
//! channel count. Hostile input never panics or over-allocates: every size is checked against the
//! bytes actually present, and the output is capped at [`MAX_SAMPLES`]. A `data` chunk that
//! claims more bytes than the file has (streaming writers leave it at `0xFFFFFFFF`) is decoded
//! as far as whole frames go and reported with `truncated: true`.

use crate::error::{AudioError, ErrorCode, Result};

/// Most samples (all channels) a decode will produce: 2²⁸ (1 GiB of `f32`).
pub const MAX_SAMPLES: usize = 1 << 28;
/// Most channels accepted.
pub const MAX_CHANNELS: u16 = 32;

const SUBTYPE_TAIL: [u8; 14] = [0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71];

/// A decoded WAV.
#[derive(Clone, Debug, PartialEq)]
pub struct Wav {
    pub rate: u32,
    pub channels: u16,
    /// Interleaved samples in -1..1 (floats are passed through unclamped).
    pub samples: Vec<f32>,
    /// The source encoding.
    pub format: WavFormat,
    /// The `data` chunk was cut short; `samples` holds the whole frames that were present.
    pub truncated: bool,
}

/// Sample encodings for [`encode`] (and what [`decode`] found).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WavFormat {
    Pcm8,
    Pcm16,
    Pcm24,
    Pcm32,
    Float32,
    Float64,
}

impl WavFormat {
    fn bits(self) -> u16 {
        match self {
            WavFormat::Pcm8 => 8,
            WavFormat::Pcm16 => 16,
            WavFormat::Pcm24 => 24,
            WavFormat::Pcm32 | WavFormat::Float32 => 32,
            WavFormat::Float64 => 64,
        }
    }

    fn is_float(self) -> bool {
        matches!(self, WavFormat::Float32 | WavFormat::Float64)
    }
}

impl Wav {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1) as usize
    }

    /// Converts to a playable [`crate::Sound`] (downmixing 3+ channels to stereo).
    pub fn to_sound(&self, name: &str) -> Result<crate::Sound> {
        crate::Sound::from_interleaved_any(name, self.rate, self.channels, self.samples.clone())
    }
}

fn err(code: ErrorCode, offset: usize, msg: impl Into<String>) -> AudioError {
    AudioError::new(code, msg).with_offset(offset)
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

/// Decodes a WAV file.
pub fn decode(bytes: &[u8]) -> Result<Wav> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(err(ErrorCode::WavNotWave, 0, "not a RIFF/WAVE file"));
    }
    let mut fmt: Option<(WavFormat, u16, u32, usize)> = None;
    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32_at(bytes, pos + 4) as usize;
        let body = pos + 8;
        let available = bytes.len() - body;
        if id == b"fmt " {
            if size < 16 || size > available {
                return Err(err(ErrorCode::WavTruncated, pos, "fmt chunk is too short").with_numbers(size.min(available) as f64, 16.0));
            }
            fmt = Some(parse_fmt(&bytes[body..body + size], body)?);
        } else if id == b"data" {
            let Some((format, channels, rate, align)) = fmt else {
                return Err(err(ErrorCode::WavTruncated, pos, "data chunk before fmt chunk"));
            };
            let truncated = size > available;
            let len = size.min(available);
            let frames = len / align;
            let total = frames.checked_mul(channels as usize).filter(|&t| t <= MAX_SAMPLES);
            let Some(total) = total else {
                return Err(err(ErrorCode::Limit, pos, "WAV is too large").with_numbers((frames as f64) * channels as f64, MAX_SAMPLES as f64));
            };
            let data = &bytes[body..body + frames * align];
            let samples = convert(data, format, channels as usize, align, total);
            return Ok(Wav { rate, channels, samples, format, truncated: truncated || len % align != 0 });
        }
        // Chunks are padded to even sizes.
        let next = body.checked_add(size).and_then(|n| n.checked_add(size & 1));
        match next {
            Some(n) if n > pos => pos = n,
            _ => break,
        }
    }
    if fmt.is_none() {
        Err(err(ErrorCode::WavTruncated, pos, "no fmt chunk"))
    } else {
        Err(err(ErrorCode::WavTruncated, pos, "no data chunk"))
    }
}

fn parse_fmt(f: &[u8], offset: usize) -> Result<(WavFormat, u16, u32, usize)> {
    let mut tag = u16_at(f, 0);
    let channels = u16_at(f, 2);
    let rate = u32_at(f, 4);
    let align = u16_at(f, 12) as usize;
    let bits = u16_at(f, 14);
    if tag == 0xFFFE {
        if f.len() < 40 {
            return Err(err(ErrorCode::WavTruncated, offset, "extensible fmt chunk is too short").with_numbers(f.len() as f64, 40.0));
        }
        if f[26..40] != SUBTYPE_TAIL {
            return Err(err(ErrorCode::WavUnsupported, offset, "unknown extensible sub-format"));
        }
        tag = u16_at(f, 24);
    }
    let format = match (tag, bits) {
        (1, 8) => WavFormat::Pcm8,
        (1, 16) => WavFormat::Pcm16,
        (1, 24) => WavFormat::Pcm24,
        (1, 32) => WavFormat::Pcm32,
        (3, 32) => WavFormat::Float32,
        (3, 64) => WavFormat::Float64,
        _ => {
            return Err(err(ErrorCode::WavUnsupported, offset, "unsupported WAV encoding (want PCM 8/16/24/32 or float 32/64)").with_numbers(tag as f64, bits as f64))
        }
    };
    if channels == 0 || channels > MAX_CHANNELS {
        return Err(err(ErrorCode::WavBadFormat, offset, "bad channel count").with_numbers(channels as f64, MAX_CHANNELS as f64));
    }
    if !(1..=1_000_000).contains(&rate) {
        return Err(err(ErrorCode::WavBadFormat, offset, "bad sample rate").with_numbers(rate as f64, 1_000_000.0));
    }
    let want = channels as usize * (bits as usize / 8);
    if align != want {
        return Err(err(ErrorCode::WavBadFormat, offset, "block align doesn't match channels × sample size").with_numbers(align as f64, want as f64));
    }
    Ok((format, channels, rate, align))
}

fn convert(data: &[u8], format: WavFormat, channels: usize, align: usize, total: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(total);
    let bytes = align / channels;
    for s in data.chunks_exact(bytes) {
        let v = match format {
            WavFormat::Pcm8 => (s[0] as f32 - 128.0) / 128.0,
            WavFormat::Pcm16 => i16::from_le_bytes([s[0], s[1]]) as f32 / 32_768.0,
            WavFormat::Pcm24 => ((i32::from_le_bytes([0, s[0], s[1], s[2]]) >> 8) as f32) / 8_388_608.0,
            WavFormat::Pcm32 => (i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f64 / 2_147_483_648.0) as f32,
            WavFormat::Float32 => sanitize(f32::from_le_bytes([s[0], s[1], s[2], s[3]])),
            WavFormat::Float64 => sanitize(f64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]) as f32),
        };
        out.push(v);
    }
    out
}

/// NaN/inf samples (corrupt float files) become silence rather than poisoning the mix.
fn sanitize(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// Encodes interleaved `samples` as a WAV file. PCM output is clamped to -1..1, scaled by 2^(bits-1)
/// (the same scale the decoder divides by, so a round trip only adds rounding error) and rounded
/// (no dither, so it is deterministic).
pub fn encode(samples: &[f32], channels: u16, rate: u32, format: WavFormat) -> Vec<u8> {
    let channels = channels.max(1);
    let bits = format.bits();
    let bytes = bits as usize / 8;
    let frames = samples.len() / channels as usize;
    let data_len = frames * channels as usize * bytes;
    let float = format.is_float();
    let mut out = Vec::with_capacity(44 + data_len + 1);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data_len + (data_len & 1)) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&(if float { 3u16 } else { 1u16 }).to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * channels as u32 * bytes as u32).to_le_bytes());
    out.extend_from_slice(&(channels * bytes as u16).to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for &s in &samples[..frames * channels as usize] {
        let c = s.clamp(-1.0, 1.0);
        match format {
            WavFormat::Pcm8 => out.push(((c * 128.0).round() + 128.0).min(255.0) as u8),
            WavFormat::Pcm16 => out.extend_from_slice(&((c * 32_768.0).round().min(32_767.0) as i16).to_le_bytes()),
            WavFormat::Pcm24 => out.extend_from_slice(&((c * 8_388_608.0).round().min(8_388_607.0) as i32).to_le_bytes()[..3]),
            WavFormat::Pcm32 => out.extend_from_slice(&((c as f64 * 2_147_483_648.0).round().min(2_147_483_647.0) as i32).to_le_bytes()),
            WavFormat::Float32 => out.extend_from_slice(&s.to_le_bytes()),
            WavFormat::Float64 => out.extend_from_slice(&(s as f64).to_le_bytes()),
        }
    }
    if data_len & 1 == 1 {
        out.push(0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula_core::rng::Rng;

    fn ramp(n: usize) -> Vec<f32> {
        (0..n).map(|i| (i as f32 / n as f32) * 2.0 - 1.0).collect()
    }

    #[test]
    fn round_trips_every_format() {
        let s = ramp(1000);
        for (fmt, tol) in [
            (WavFormat::Pcm8, 1.0 / 127.0),
            (WavFormat::Pcm16, 1.0 / 32_000.0),
            (WavFormat::Pcm24, 1e-6),
            (WavFormat::Pcm32, 1e-7),
            (WavFormat::Float32, 0.0),
            (WavFormat::Float64, 0.0),
        ] {
            let bytes = encode(&s, 2, 44_100, fmt);
            let w = decode(&bytes).unwrap();
            assert_eq!((w.rate, w.channels, w.format, w.truncated), (44_100, 2, fmt, false));
            assert_eq!(w.samples.len(), 1000);
            for (a, b) in s.iter().zip(&w.samples) {
                assert!((a - b).abs() <= tol + 1e-7, "{fmt:?}: {a} vs {b}");
            }
        }
        // Float32 is bit exact, including values beyond ±1.
        let wild = vec![1.5f32, -2.25, 0.1, f32::MIN_POSITIVE];
        assert_eq!(decode(&encode(&wild, 1, 48_000, WavFormat::Float32)).unwrap().samples, wild);
    }

    #[test]
    fn odd_sized_chunks_are_padded_and_skipped() {
        // An odd-sized unknown chunk before fmt, and an 8-bit mono odd-length data chunk.
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF\0\0\0\0WAVE");
        b.extend_from_slice(b"LIST\x03\0\0\0abc\0");
        let body = encode(&[0.0, 0.5, -0.5], 1, 8000, WavFormat::Pcm8);
        b.extend_from_slice(&body[12..]);
        let w = decode(&b).unwrap();
        assert_eq!(w.samples.len(), 3);
        assert!((w.samples[1] - 0.5).abs() < 0.01);
    }

    #[test]
    fn extensible_float_is_decoded() {
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF\0\0\0\0WAVEfmt ");
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&0xFFFEu16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&48_000u32.to_le_bytes());
        b.extend_from_slice(&192_000u32.to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&32u16.to_le_bytes());
        b.extend_from_slice(&22u16.to_le_bytes());
        b.extend_from_slice(&32u16.to_le_bytes());
        b.extend_from_slice(&4u32.to_le_bytes());
        b.extend_from_slice(&3u16.to_le_bytes());
        b.extend_from_slice(&SUBTYPE_TAIL);
        b.extend_from_slice(b"data");
        b.extend_from_slice(&8u32.to_le_bytes());
        b.extend_from_slice(&0.25f32.to_le_bytes());
        b.extend_from_slice(&f32::NAN.to_le_bytes());
        let w = decode(&b).unwrap();
        assert_eq!(w.format, WavFormat::Float32);
        assert_eq!(w.samples, vec![0.25, 0.0], "NaN sanitised");
    }

    #[test]
    fn structured_errors() {
        assert_eq!(decode(b"").unwrap_err().code, ErrorCode::WavNotWave);
        assert_eq!(decode(b"RIFF\0\0\0\0WAVX").unwrap_err().code, ErrorCode::WavNotWave);
        assert_eq!(decode(b"RIFF\0\0\0\0WAVE").unwrap_err().code, ErrorCode::WavTruncated);
        let good = encode(&ramp(10), 1, 8000, WavFormat::Pcm16);
        // Data cut short: whole frames kept, flagged.
        let w = decode(&good[..good.len() - 3]).unwrap();
        assert!(w.truncated);
        assert_eq!(w.samples.len(), 8);
        // ADPCM (tag 2) is unsupported, with the tag and bits as numbers.
        let mut adpcm = good.clone();
        adpcm[20] = 2;
        let e = decode(&adpcm).unwrap_err();
        assert_eq!((e.code, e.measured, e.limit), (ErrorCode::WavUnsupported, Some(2.0), Some(16.0)));
        // Block align lies.
        let mut lie = good.clone();
        lie[32] = 7;
        assert_eq!(decode(&lie).unwrap_err().code, ErrorCode::WavBadFormat);
        // Zero channels.
        let mut zero = good.clone();
        zero[22] = 0;
        assert_eq!(decode(&zero).unwrap_err().code, ErrorCode::WavBadFormat);
        // fmt without data.
        assert_eq!(decode(&good[..36]).unwrap_err().code, ErrorCode::WavTruncated);
    }

    /// Mutation fuzz: random byte flips, truncations, inserted garbage and size-field lies on
    /// valid files of every format must never panic, and every success must be self-consistent.
    #[test]
    fn fuzz_never_panics() {
        let mut rng = Rng::named(0xA0D10, "wav_fuzz");
        let seeds: Vec<Vec<u8>> = [WavFormat::Pcm8, WavFormat::Pcm16, WavFormat::Pcm24, WavFormat::Pcm32, WavFormat::Float32, WavFormat::Float64]
            .iter()
            .enumerate()
            .map(|(i, &f)| encode(&ramp(37 + i), 1 + (i % 3) as u16, 22_050, f))
            .collect();
        for iter in 0..20_000 {
            let mut b = seeds[iter % seeds.len()].clone();
            let mutations = 1 + rng.below(6);
            for _ in 0..mutations {
                match rng.below(5) {
                    0 => {
                        let i = rng.below(b.len() as u32) as usize;
                        b[i] ^= 1 << rng.below(8);
                    }
                    1 => {
                        let i = rng.below(b.len() as u32) as usize;
                        b[i] = rng.next_u32() as u8;
                    }
                    2 => {
                        let n = rng.below(b.len() as u32 + 1) as usize;
                        b.truncate(n);
                    }
                    3 => {
                        // Lie in a 32-bit size field (RIFF, fmt or data).
                        let at = [4usize, 16, 40][rng.below(3) as usize];
                        if at + 4 <= b.len() {
                            b[at..at + 4].copy_from_slice(&rng.next_u32().to_le_bytes());
                        }
                    }
                    _ => {
                        let i = rng.below(b.len() as u32 + 1) as usize;
                        let junk: Vec<u8> = (0..rng.below(16)).map(|_| rng.next_u32() as u8).collect();
                        b.splice(i..i, junk);
                    }
                }
                if b.is_empty() {
                    break;
                }
            }
            if let Ok(w) = decode(&b) {
                assert!(w.channels >= 1 && w.samples.len() % w.channels as usize == 0);
                assert!(w.samples.iter().all(|s| s.is_finite()));
            }
        }
    }
}
