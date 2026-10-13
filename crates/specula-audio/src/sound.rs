//! Sample buffers the mixer plays.

use crate::error::{AudioError, ErrorCode, Result};
use std::sync::Arc;

/// Lowest and highest sample rate a [`Sound`] may have.
pub const MIN_RATE: u32 = 4_000;
pub const MAX_RATE: u32 = 384_000;

#[derive(Debug)]
pub(crate) struct SoundData {
    pub name: String,
    pub rate: u32,
    /// 1 (mono) or 2 (stereo, interleaved L R).
    pub channels: u16,
    pub frames: usize,
    pub samples: Box<[f32]>,
    /// Loop region in frames for looping playback (`start..end`, end exclusive).
    pub loop_start: usize,
    pub loop_end: usize,
}

/// An immutable, shareable sample buffer (mono or interleaved stereo `f32`). Cheap to clone; the
/// mixer holds a reference while a voice plays it, and the memory is always freed on the game
/// thread, never on the audio thread.
#[derive(Clone, Debug)]
pub struct Sound {
    pub(crate) data: Arc<SoundData>,
}

impl Sound {
    /// Wraps interleaved samples. Mono and stereo only (downmix others first, see
    /// [`Sound::from_interleaved_any`]). Non-finite samples are rejected.
    pub fn new(name: impl Into<String>, rate: u32, channels: u16, samples: Vec<f32>) -> Result<Sound> {
        if !(1..=2).contains(&channels) {
            return Err(AudioError::new(ErrorCode::BadSound, "a sound must be mono or stereo").with_numbers(channels as f64, 2.0));
        }
        if !(MIN_RATE..=MAX_RATE).contains(&rate) {
            return Err(AudioError::new(ErrorCode::BadSound, "sample rate out of range").with_numbers(rate as f64, MAX_RATE as f64));
        }
        if samples.is_empty() || samples.len() % channels as usize != 0 {
            return Err(AudioError::new(ErrorCode::BadSound, "sample count must be a non-zero multiple of the channel count"));
        }
        if let Some(i) = samples.iter().position(|s| !s.is_finite()) {
            return Err(AudioError::new(ErrorCode::BadSound, format!("sample {i} is not finite")));
        }
        let frames = samples.len() / channels as usize;
        Ok(Sound {
            data: Arc::new(SoundData { name: name.into(), rate, channels, frames, samples: samples.into_boxed_slice(), loop_start: 0, loop_end: frames }),
        })
    }

    /// Like [`Sound::new`] but accepts any channel count: 3+ channels are downmixed to stereo
    /// (front left/right kept; centre added to both at -3 dB; the rest folded in at -6 dB).
    pub fn from_interleaved_any(name: impl Into<String>, rate: u32, channels: u16, samples: Vec<f32>) -> Result<Sound> {
        if channels <= 2 {
            return Sound::new(name, rate, channels, samples);
        }
        let ch = channels as usize;
        if samples.is_empty() || samples.len() % ch != 0 {
            return Err(AudioError::new(ErrorCode::BadSound, "sample count must be a non-zero multiple of the channel count"));
        }
        let mut out = Vec::with_capacity(samples.len() / ch * 2);
        for frame in samples.chunks_exact(ch) {
            let (mut l, mut r) = (frame[0], frame[1]);
            // WAVE order: FL FR FC LFE BL BR …; LFE (index 3) is dropped.
            if ch > 2 {
                l += frame[2] * std::f32::consts::FRAC_1_SQRT_2;
                r += frame[2] * std::f32::consts::FRAC_1_SQRT_2;
            }
            for (i, &s) in frame.iter().enumerate().skip(4) {
                if i % 2 == 0 {
                    l += s * 0.5;
                } else {
                    r += s * 0.5;
                }
            }
            out.push(l);
            out.push(r);
        }
        Sound::new(name, rate, 2, out)
    }

    /// Sets the loop region (frames, end exclusive) used when the sound is played looping.
    pub fn with_loop(self, start: usize, end: usize) -> Result<Sound> {
        let d = &self.data;
        if start >= end || end > d.frames {
            return Err(AudioError::new(ErrorCode::BadSound, "loop region must satisfy start < end <= frames").with_numbers(end as f64, d.frames as f64));
        }
        Ok(Sound {
            data: Arc::new(SoundData {
                name: d.name.clone(),
                rate: d.rate,
                channels: d.channels,
                frames: d.frames,
                samples: d.samples.clone(),
                loop_start: start,
                loop_end: end,
            }),
        })
    }

    pub fn name(&self) -> &str {
        &self.data.name
    }

    pub fn rate(&self) -> u32 {
        self.data.rate
    }

    pub fn channels(&self) -> u16 {
        self.data.channels
    }

    pub fn frames(&self) -> usize {
        self.data.frames
    }

    pub fn duration_s(&self) -> f64 {
        self.data.frames as f64 / self.data.rate as f64
    }

    /// Interleaved samples.
    pub fn samples(&self) -> &[f32] {
        &self.data.samples
    }
}
