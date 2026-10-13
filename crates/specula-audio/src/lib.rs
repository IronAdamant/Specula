//! Specula audio: output, mixing, synthesis and analysis, with zero dependencies.
//!
//! Agents can't import outside sounds and can't hear, so sounds are **written as JSON recipes**,
//! **rendered deterministically** by [`synth`], **checked as numbers and pictures** with
//! [`analysis`], and **played** by a lock-free [`mixer`] on a WASAPI (Windows) or null backend.
//!
//! # Playing sounds
//!
//! ```text
//! let audio = Audio::start(AudioConfig::default())?;          // WASAPI, or null if no device
//! let bolt = synth::render_text(&fs::read_to_string("bolt_fire.json")?, &RenderOptions::default())?;
//! let bolt = bolt.to_sound()?;                                 // Sound: Arc'd samples, cheap clone
//! let id = audio.play(&bolt, PlayParams { spatial: Some(Spatial::at([4.0, 1.0, -6.0])), ..Default::default() })?;
//! audio.set_listener(Listener { position, forward, up })?;    // once per frame
//! audio.set_bus_gain(Bus::Music, 0.5, 1.0)?;                   // fade the music bus to half over 1 s
//! audio.stop(id, 0.2)?;                                        // fade out over 200 ms
//! let s = audio.stats();                                       // backend, latency, voices, mix time
//! audio.shutdown()?;                                           // or drop
//! ```
//!
//! [`Audio`] methods take `&self` and are cheap: each pushes one command into a bounded SPSC ring
//! that the audio thread drains at the start of every buffer. The audio thread never allocates,
//! locks or frees memory; finished sounds come back through a second ring and are dropped on the
//! caller's thread. `Backend::Manual` runs no thread at all: the caller pulls samples with
//! [`Audio::pull`] (tests, offline renders, headless playtests with exact timing).
//!
//! # Recipe format (version 1)
//!
//! ```text
//! { "sound": 1, "name": "bolt_hit", "bus": "sfx", "seed": 3,
//!   "duration": 0.35,                      // seconds (≤ 60)
//!   "channels": 1,                         // 1 (default; needed for 3D) or 2
//!   "loudness": -18,                       // optional: normalise to this LUFS-ish target
//!   "peak_ceiling_db": -1,                 // default -1 dBFS, applied after normalising
//!   "loop": { "crossfade": 0.4 },          // optional: render a seamless loop
//!   "vary": { "pitch": [-1, 1], "gain_db": [-1.5, 0] },   // per-variation ranges (variation ≥ 1)
//!   "layers": [
//!     { "name": "thump", "at": 0, "length": 0.08, "gain_db": 0, "pan": 0,
//!       "source": { "wave": "sine", "freq": { "points": [[0, 180], [0.08, 55]], "curve": "exp" } },
//!       "env": { "attack": 0.001, "decay": 0.08, "sustain": 0, "release": 0.02 },
//!       "filters": [ { "type": "lowpass", "cutoff": 900, "q": 0.7 } ] },
//!     { "name": "crack", "source": { "wave": "noise", "color": "white" },
//!       "env": { "attack": 0.0005, "decay": 0.05, "sustain": 0, "release": 0.01 },
//!       "filters": [ { "type": "bandpass", "cutoff": { "rand": [2500, 3500] }, "q": 2 } ],
//!       "repeat": { "count": 3, "interval": 0.012, "jitter": 0.004, "gain_db_step": -6 } } ],
//!   "fx": [ { "type": "reverb", "room": 0.4, "damp": 0.6, "mix": 0.15 } ] }
//! ```
//!
//! - **Numbers**: any number may be `{"rand": [lo, hi]}`, redrawn per variation and per repeat
//!   from a stream keyed by its path. A **named** layer is keyed by its name, so adding,
//!   removing or reordering layers leaves named layers' draws alone (unnamed layers are keyed by
//!   index; within a layer, array entries such as `filters[1]` are keyed by index). Editing a
//!   value never changes other fields' draws. Layer names must be unique.
//! - **Params over time** (`freq`, `cutoff`, `duty`, `fm.index`): a number or
//!   `{"points": [[t, v], ...], "curve": "linear" | "exp"}` with t in seconds from the layer start.
//! - **Sources**: `wave` = `sine` (optional `partials: [[ratio, gain, decay_s?], ...]` and
//!   `fm: {ratio, index}`), `square` (`duty`), `saw`, `triangle` (band-limited with PolyBLEP),
//!   `noise` (`color`: white | pink | brown; optional `freq` = sample-and-hold rate). Tonal
//!   waves take `unison: {voices, cents}` and `phase`.
//! - **Layer**: `at`, `length` (gate; default to the end), `gain_db`, `pan` (stereo recipes),
//!   `env` (`attack hold decay sustain release curve: exp|linear`, or `points: [[t, level]]`),
//!   `vibrato: {rate, depth (semitones), delay}`, `arp: {steps: [semitones], rate}`, up to 4
//!   `filters` (`lowpass highpass bandpass notch` = resonant SVF with `q`; `lp1 hp1` =
//!   one-pole), up to 8 `fx`, and `repeat: {count, interval, jitter, gain_db_step, pitch_jitter}`.
//! - **Fx** (per layer or master): `distort {drive, mix}`, `bitcrush {bits, rate}`,
//!   `delay {time, feedback, mix, damp}`, `reverb {room, damp, mix, width}` (Freeverb),
//!   `tremolo {rate, depth}`.
//! - **Strict**: unknown fields, names and out-of-range numbers are errors with a path
//!   (`layers[1].filters[0].cutoff`), an [`ErrorCode`] and measured/limit numbers.
//! - **Deterministic**: same recipe + rate + variation ⇒ bit-identical samples
//!   ([`synth::Rendered::hash`]) on every platform.

pub mod analysis;
mod backend;
pub mod dsp;
pub mod error;
pub mod mixer;
pub mod resample;
pub mod ring;
pub mod sound;
pub mod synth;
pub mod wav;

pub use error::{AudioError, ErrorCode, Result};
pub use mixer::{BackendState, Bus, DuckRule, Listener, PlayParams, Spatial, Stats, VoiceId};
pub use sound::Sound;
pub use synth::{Recipe, RenderOptions, Rendered};

use mixer::{Mixer, MixerConfig, MixerHandle, SharedStats};

/// Parses and renders a recipe into a playable [`Sound`] (the usual way to load a sound set;
/// render at [`Stats::output_rate`] to skip resampling, or at 48 kHz for reproducible hashes).
pub fn load_recipe(text: &str, options: &RenderOptions) -> Result<Sound> {
    synth::render_text(text, options)?.to_sound()
}
use std::sync::{Arc, Mutex, MutexGuard};

/// Which output to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// The platform device if there is one (WASAPI on Windows), otherwise the null backend. On
    /// Windows, a missing or lost device is retried in the background.
    Auto,
    /// The platform device; [`Audio::start`] fails with [`ErrorCode::NoDevice`] /
    /// [`ErrorCode::Unsupported`] if it can't be opened.
    Device,
    /// Mix in real time on a thread and discard the output (headless servers, CI).
    Null,
    /// No thread: the caller pulls samples with [`Audio::pull`].
    Manual,
}

/// Configuration for [`Audio::start`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioConfig {
    pub backend: Backend,
    /// Simultaneous voices before priority stealing (default 64).
    pub max_voices: usize,
    /// Target device buffer (ms); WASAPI may round up. Default 30.
    pub latency_ms: u32,
    /// Commands that can be queued between two audio buffers (default 1024).
    pub command_capacity: usize,
    /// Mix rate of the null and manual backends (default 48 kHz).
    pub null_rate: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        AudioConfig { backend: Backend::Auto, max_voices: 64, latency_ms: 30, command_capacity: 1024, null_rate: 48_000 }
    }
}

enum Runner {
    Thread(backend::ThreadBackend),
    Manual(Box<Mutex<Mixer>>),
}

/// The audio system: a backend thread (or a manual mixer) plus the command side.
pub struct Audio {
    handle: Mutex<MixerHandle>,
    stats: Arc<SharedStats>,
    runner: Option<Runner>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding the lock can't leave the handle inconsistent (each call is one
    // push), so poisoning is ignored.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Audio {
    /// Starts audio output.
    pub fn start(config: AudioConfig) -> Result<Audio> {
        let mc = |rate| MixerConfig { rate, max_voices: config.max_voices, command_capacity: config.command_capacity };
        let null_rate = config.null_rate.clamp(8_000, 192_000);
        let (handle, mixer) = Mixer::new(mc(null_rate));
        let stats = handle.stats.clone();
        let runner = match config.backend {
            Backend::Manual => {
                backend::set_state(&stats, BackendState::Null);
                stats.device_channels.store(2, std::sync::atomic::Ordering::Relaxed);
                Runner::Manual(Box::new(Mutex::new(mixer)))
            }
            Backend::Null => Runner::Thread(backend::null::start(mixer)?),
            Backend::Auto | Backend::Device => {
                #[cfg(windows)]
                {
                    let opts = backend::wasapi::Options { latency_ms: config.latency_ms, require_device: config.backend == Backend::Device };
                    Runner::Thread(backend::wasapi::start(mixer, opts)?)
                }
                #[cfg(not(windows))]
                {
                    if config.backend == Backend::Device {
                        return Err(AudioError::new(ErrorCode::Unsupported, "no audio device backend on this platform yet (null backend only)"));
                    }
                    Runner::Thread(backend::null::start(mixer)?)
                }
            }
        };
        Ok(Audio { handle: Mutex::new(handle), stats, runner: Some(runner) })
    }

    /// Plays `sound`; returns its voice id.
    pub fn play(&self, sound: &Sound, params: PlayParams) -> Result<VoiceId> {
        self.cmd(|h| h.play(sound, params))
    }

    /// Stops a voice, fading out over `fade_s` seconds (a few ms minimum, to avoid clicks).
    pub fn stop(&self, id: VoiceId, fade_s: f32) -> Result<()> {
        self.cmd(|h| h.stop(id, fade_s))
    }

    /// Stops every voice on `bus` (or everywhere with `None`).
    pub fn stop_all(&self, bus: Option<Bus>, fade_s: f32) -> Result<()> {
        self.cmd(|h| h.stop_all(bus, fade_s))
    }

    pub fn set_gain(&self, id: VoiceId, gain: f32, fade_s: f32) -> Result<()> {
        self.cmd(|h| h.set_gain(id, gain, fade_s))
    }

    pub fn set_pitch(&self, id: VoiceId, pitch: f32) -> Result<()> {
        self.cmd(|h| h.set_pitch(id, pitch))
    }

    pub fn set_pan(&self, id: VoiceId, pan: f32) -> Result<()> {
        self.cmd(|h| h.set_pan(id, pan))
    }

    /// Moves a spatial voice.
    pub fn set_position(&self, id: VoiceId, position: [f32; 3]) -> Result<()> {
        self.cmd(|h| h.set_position(id, position))
    }

    pub fn set_listener(&self, listener: Listener) -> Result<()> {
        self.cmd(|h| h.set_listener(listener))
    }

    /// Sets a bus volume (linear), ramped over `fade_s`.
    pub fn set_bus_gain(&self, bus: Bus, gain: f32, fade_s: f32) -> Result<()> {
        self.cmd(|h| h.set_bus_gain(bus, gain, fade_s))
    }

    /// Installs or clears duck rule `slot` (0..4).
    pub fn set_duck(&self, slot: usize, rule: Option<DuckRule>) -> Result<()> {
        self.cmd(|h| h.set_duck(slot, rule))
    }

    /// Voices that ended since the last call.
    pub fn drain_finished(&self) -> Vec<VoiceId> {
        lock(&self.handle).drain_finished()
    }

    pub fn stats(&self) -> Stats {
        Stats::read(&self.stats)
    }

    /// Manual backend only: mixes the next `out.len() / 2` frames (interleaved stereo).
    pub fn pull(&self, out: &mut [f32]) -> Result<()> {
        match &self.runner {
            Some(Runner::Manual(m)) => {
                lock(m).render(out);
                Ok(())
            }
            _ => Err(AudioError::new(ErrorCode::WrongMode, "pull() needs Backend::Manual")),
        }
    }

    /// Runs a command on the handle, failing fast if the audio thread is gone.
    fn cmd<T>(&self, f: impl FnOnce(&mut MixerHandle) -> Result<T>) -> Result<T> {
        self.check()?;
        f(&mut lock(&self.handle))
    }

    fn check(&self) -> Result<()> {
        if self.stats.thread_panicked.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(AudioError::new(ErrorCode::ThreadGone, "the audio thread panicked; start a new Audio to recover"));
        }
        if let Some(Runner::Thread(t)) = &self.runner {
            if t.join.as_ref().is_some_and(|j| j.is_finished()) {
                return Err(AudioError::new(ErrorCode::ThreadGone, "the audio thread has stopped"));
            }
        }
        Ok(())
    }

    /// Stops the backend and joins its thread.
    pub fn shutdown(mut self) -> Result<()> {
        self.stop_runner()
    }

    fn stop_runner(&mut self) -> Result<()> {
        match self.runner.take() {
            Some(Runner::Thread(mut t)) => {
                if t.shutdown() {
                    Ok(())
                } else {
                    Err(AudioError::new(ErrorCode::ThreadGone, "the audio thread panicked"))
                }
            }
            Some(Runner::Manual(_)) => {
                backend::set_state(&self.stats, BackendState::Stopped);
                Ok(())
            }
            None => Ok(()),
        }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        let _ = self.stop_runner();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_can_be_shared_across_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Audio>();
        assert_send_sync::<Sound>();
    }

    #[test]
    fn load_recipe_plays_through_manual_backend() {
        let text = r#"{"sound": 1, "name": "beep", "duration": 0.05, "layers": [{"source": {"wave": "sine", "freq": 880}}]}"#;
        let sound = load_recipe(text, &RenderOptions::default()).unwrap();
        assert_eq!((sound.rate(), sound.channels(), sound.frames()), (48_000, 1, 2400));
        let audio = Audio::start(AudioConfig { backend: Backend::Manual, ..Default::default() }).unwrap();
        audio.play(&sound, PlayParams::default()).unwrap();
        let mut out = vec![0.0; 4800];
        audio.pull(&mut out).unwrap();
        assert!(out.iter().any(|s| s.abs() > 0.3));
        assert_eq!(load_recipe("{}", &RenderOptions::default()).unwrap_err().code, ErrorCode::MissingField);
    }
}
