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
pub mod meter;
pub mod mixer;
pub mod music;
pub mod ogg;
pub mod resample;
pub mod ring;
pub mod sound;
pub mod stream;
pub mod synth;
pub mod vorbis;
pub mod wav;

pub use error::{AudioError, ErrorCode, Result};
pub use mixer::{BackendState, Bus, DuckRule, Listener, PlayParams, Spatial, Stats, VoiceId};
pub use sound::Sound;
pub use stream::{StreamHandle, StreamParams, StreamSource, StreamStatus};
pub use synth::{Recipe, RenderOptions, Rendered};

use mixer::{Mixer, MixerConfig, MixerHandle, SharedStats, TapBlock};

/// The `audio` trace subsystem: stream starvation and errors (from the worker thread), a meter
/// that fell behind, and whatever the engine's audio service reports.
pub static TRACE: specula_core::trace::Subsystem = specula_core::trace::Subsystem::new("audio");
use ring::{Consumer, Producer};
use std::sync::atomic::{AtomicBool, Ordering};

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

/// The worker's state: stream writers to fill and the meter tap to drain.
struct WorkerState {
    streams: Vec<stream::StreamWriter>,
    /// Starve events and errors already traced, per stream (by index into `streams`).
    seen: Vec<(u64, bool)>,
    tap_dropped_seen: u64,
    incoming: Consumer<stream::StreamWriter>,
    tap_full: Consumer<Box<TapBlock>>,
    tap_free: Producer<Box<TapBlock>>,
    meter: Arc<Mutex<meter::Meter>>,
    stats: Arc<SharedStats>,
}

impl WorkerState {
    /// One round: take new streams, fill every stream's free blocks, measure the tap. Returns
    /// whether there was anything to do.
    fn step(&mut self) -> bool {
        let started = std::time::Instant::now();
        let worked = self.step_inner();
        if worked {
            self.stats.worker_ns.fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        }
        worked
    }

    fn step_inner(&mut self) -> bool {
        let mut worked = false;
        while let Some(w) = self.incoming.pop() {
            self.streams.push(w);
            self.seen.push((0, false));
            worked = true;
        }
        let rate = self.stats.output_rate.load(Ordering::Relaxed);
        for (w, seen) in self.streams.iter_mut().zip(self.seen.iter_mut()) {
            worked |= w.pump(rate);
            // Starvation is traced (never heard as garbage: the mixer plays silence).
            let events = w.shared.starve_events.load(Ordering::Relaxed);
            if events > seen.0 {
                seen.0 = events;
                specula_core::trace_event!(&TRACE, Warn, "audio.stream_starved", stream = w.name(), events = events, frames = w.shared.starved_frames.load(Ordering::Relaxed));
            }
            if !seen.1 {
                if let Some(e) = w.shared.error.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
                    seen.1 = true;
                    specula_core::trace_event!(&TRACE, Warn, "audio.stream_error", stream = w.name(), code = e.code.as_str(), error = e.to_string());
                }
            }
        }
        let mut i = 0;
        while i < self.streams.len() {
            if self.streams[i].finished() {
                self.streams.swap_remove(i);
                self.seen.swap_remove(i);
            } else {
                i += 1;
            }
        }
        let dropped = self.stats.tap_dropped.load(Ordering::Relaxed);
        if dropped > self.tap_dropped_seen {
            specula_core::trace_event!(&TRACE, Debug, "audio.meter_behind", dropped_blocks = dropped - self.tap_dropped_seen);
            self.tap_dropped_seen = dropped;
        }
        let mut m = lock(&self.meter);
        while let Some(tap) = self.tap_full.pop() {
            m.feed(&tap);
            // The ring holds every block, so handing it back can't fail.
            let _ = self.tap_free.push(tap);
            worked = true;
        }
        worked
    }
}

enum Worker {
    Thread { stop: Arc<AtomicBool>, join: Option<std::thread::JoinHandle<()>> },
    /// `Backend::Manual`: the caller runs [`Audio::service`].
    Manual(Box<Mutex<WorkerState>>),
}

/// Streams that can be waiting for the worker at once.
const INCOMING_STREAMS: usize = 16;

/// The audio system: a backend thread (or a manual mixer), the worker thread that decodes
/// streams and meters the mix, and the command side.
pub struct Audio {
    handle: Mutex<MixerHandle>,
    stats: Arc<SharedStats>,
    runner: Option<Runner>,
    worker: Option<Worker>,
    incoming: Mutex<Producer<stream::StreamWriter>>,
    meter: Arc<Mutex<meter::Meter>>,
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
        // Streams resample with one shared table: built now, at start-up, never when a stream
        // starts (a game starts streams from its frame).
        let _ = resample::shared_table();
        let (handle, mut mixer) = Mixer::new(mc(null_rate));
        let stats = handle.stats.clone();
        // The meter tap: every block is allocated here, once.
        let (tap_full_tx, tap_full_rx) = ring::channel(mixer::TAP_BLOCKS);
        let (mut tap_free_tx, tap_free_rx) = ring::channel(mixer::TAP_BLOCKS);
        for _ in 0..mixer::TAP_BLOCKS {
            let _ = tap_free_tx.push(TapBlock::boxed());
        }
        mixer.set_tap(tap_full_tx, tap_free_rx);
        let meter = Arc::new(Mutex::new(meter::Meter::new(null_rate)));
        let (incoming_tx, incoming_rx) = ring::channel(INCOMING_STREAMS);
        let state = WorkerState { streams: Vec::new(), seen: Vec::new(), tap_dropped_seen: 0, incoming: incoming_rx, tap_full: tap_full_rx, tap_free: tap_free_tx, meter: meter.clone(), stats: stats.clone() };
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
        let worker = match &runner {
            Runner::Manual(_) => Worker::Manual(Box::new(Mutex::new(state))),
            Runner::Thread(_) => {
                let stop = Arc::new(AtomicBool::new(false));
                let flag = stop.clone();
                let mut state = state;
                let join = std::thread::Builder::new()
                    .name("specula-audio-worker".into())
                    .spawn(move || {
                        while !flag.load(Ordering::Acquire) {
                            if !state.step() {
                                std::thread::sleep(std::time::Duration::from_millis(3));
                            }
                        }
                    })
                    .map_err(|e| AudioError::new(ErrorCode::ThreadGone, format!("couldn't start the audio worker: {e}")))?;
                Worker::Thread { stop, join: Some(join) }
            }
        };
        Ok(Audio { handle: Mutex::new(handle), stats, runner: Some(runner), worker: Some(worker), incoming: Mutex::new(incoming_tx), meter })
    }

    /// Streams `source` (decoded on the worker thread into a bounded ring); returns the voice
    /// and a handle to watch it (starvation, loops, position). Stop it like any voice.
    pub fn play_stream(&self, source: Box<dyn StreamSource>, params: StreamParams) -> Result<(VoiceId, StreamHandle)> {
        self.start_stream(source, params, None)
    }

    /// Crossfades: starts `source` fading in over `seconds` and fades `from` out over the same
    /// time (equal-gain ramps; pass `None` to just fade in). Both happen or neither: with no room
    /// for both commands nothing starts and the old stream plays on (review round 2: the new one
    /// started, the stop was refused, and the caller lost track of the old one).
    pub fn crossfade(&self, from: Option<VoiceId>, source: Box<dyn StreamSource>, mut params: StreamParams, seconds: f32) -> Result<(VoiceId, StreamHandle)> {
        params.fade_in_s = seconds.max(0.0);
        self.start_stream(source, params, from.map(|v| (v, seconds)))
    }

    /// Starts a stream and, in the same breath, fades `stop` out: room for every command is
    /// checked under the command lock first, so the call applies whole or not at all.
    fn start_stream(&self, source: Box<dyn StreamSource>, params: StreamParams, stop: Option<(VoiceId, f32)>) -> Result<(VoiceId, StreamHandle)> {
        self.check()?;
        let rate = self.stats.output_rate.load(Ordering::Relaxed);
        let (writer, reader, handle) = stream::channel(source, &params, rate)?;
        let mut h = lock(&self.handle);
        let need = 1 + stop.is_some() as usize;
        if h.room() < need {
            return Err(AudioError::new(ErrorCode::QueueFull, "the audio command queue has no room to start this stream; nothing was started or stopped").with_numbers(need as f64, h.room() as f64));
        }
        if lock(&self.incoming).push(writer).is_err() {
            return Err(AudioError::new(ErrorCode::QueueFull, "too many streams are waiting to start; nothing was started or stopped").with_numbers(INCOMING_STREAMS as f64, INCOMING_STREAMS as f64));
        }
        // Fill the first blocks right away in manual mode (tests, offline renders).
        if let Some(Worker::Manual(w)) = &self.worker {
            lock(w).step();
        }
        // Room was checked under this lock (only the mixer takes commands out meanwhile), so
        // neither push can be refused.
        let id = h.play_stream(Box::new(reader), params)?;
        if let Some((old, fade)) = stop {
            h.stop(old, fade)?;
        }
        Ok((id, handle))
    }

    /// Manual backend: runs the worker once (fills streams, meters what was pulled). Threaded
    /// backends do this on their own worker thread.
    pub fn service(&self) {
        if let Some(Worker::Manual(w)) = &self.worker {
            let mut w = lock(w);
            while w.step() {}
        }
    }

    /// A loudness and peak report on one meter track (a bus, or `meter::TRACKS - 1` for the
    /// master output) over the last `window_s` seconds.
    pub fn meter_report(&self, track: usize, window_s: f64, spectrum: bool) -> meter::Report {
        lock(&self.meter).report(track, window_s, spectrum)
    }

    /// Seconds of audio mixed so far: the clock the meter and play logs use.
    pub fn clock_s(&self) -> f64 {
        let s = &self.stats;
        let rate = s.output_rate.load(Ordering::Relaxed).max(1);
        s.frames.load(Ordering::Relaxed) as f64 / rate as f64
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

    /// Commands that can be queued now (the queue is bounded; see [`Audio::apply_settings`]).
    pub fn command_room(&self) -> usize {
        lock(&self.handle).room()
    }

    /// Bus gains and duck rules in one call that applies whole or not at all: every rule is
    /// validated and the command queue checked for room before anything is sent.
    pub fn apply_settings(&self, gains: &[(Bus, f32)], ducks: &[(usize, Option<DuckRule>)], fade_s: f32) -> Result<()> {
        self.check()?;
        for &(slot, rule) in ducks {
            mixer::validate_duck(slot, rule.as_ref())?;
        }
        let mut h = lock(&self.handle);
        let need = gains.len() + ducks.len();
        if h.room() < need {
            return Err(AudioError::new(ErrorCode::QueueFull, "the audio command queue has no room for these settings; nothing was changed").with_numbers(need as f64, h.room() as f64));
        }
        for &(bus, gain) in gains {
            h.set_bus_gain(bus, gain, fade_s)?;
        }
        for &(slot, rule) in ducks {
            h.set_duck(slot, rule)?;
        }
        Ok(())
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
        if let Some(Worker::Thread { stop, join }) = &mut self.worker {
            stop.store(true, Ordering::Release);
            if let Some(j) = join.take() {
                let _ = j.join();
            }
        }
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

    /// Review: settings applied part-way when the queue filled. Now a full queue changes nothing,
    /// and a bad rule refuses the whole call before anything is sent.
    #[test]
    fn settings_apply_whole_or_not_at_all() {
        let audio = Audio::start(AudioConfig { backend: Backend::Manual, command_capacity: 16, ..Default::default() }).unwrap();
        let rule = DuckRule { source: Bus::Voice, target: Bus::Music, depth_db: 6.0, threshold_db: -40.0, attack_s: 0.05, release_s: 0.4 };
        let bad = DuckRule { target: Bus::Master, ..rule };
        assert_eq!(audio.apply_settings(&[(Bus::Music, 0.5)], &[(0, Some(rule)), (1, Some(bad))], 0.0).unwrap_err().code, ErrorCode::OutOfRange);
        assert_eq!(audio.apply_settings(&[], &[(9, None)], 0.0).unwrap_err().code, ErrorCode::OutOfRange);
        // Fill the queue (the manual mixer drains it only when pulled).
        while audio.set_bus_gain(Bus::Sfx, 1.0, 0.0).is_ok() {}
        let e = audio.apply_settings(&[(Bus::Music, 0.25)], &[(0, Some(rule))], 0.0).unwrap_err();
        assert_eq!(e.code, ErrorCode::QueueFull);
        let mut out = vec![0f32; 512];
        audio.pull(&mut out).unwrap();
        audio.apply_settings(&[(Bus::Music, 0.25)], &[(0, Some(rule))], 0.0).unwrap();
        // Exactly enough room is enough.
        audio.pull(&mut out).unwrap();
        while audio.command_room() > 2 {
            audio.set_bus_gain(Bus::Sfx, 1.0, 0.0).unwrap();
        }
        audio.apply_settings(&[(Bus::Music, 0.5)], &[(1, Some(rule))], 0.0).unwrap();
        assert_eq!(audio.command_room(), 0);
    }

    /// Review round 2 probe "orphan": with room for only one command a crossfade started the new
    /// stream, failed to stop the old one, and returned an error. Now it does both or neither.
    #[test]
    fn a_crossfade_starts_and_stops_together_or_not_at_all() {
        let audio = Audio::start(AudioConfig { backend: Backend::Manual, command_capacity: 16, ..Default::default() }).unwrap();
        let tone = || Box::new(stream::SoundSource::new(Sound::new("t", 48_000, 1, vec![0.1; 480_000]).unwrap()));
        let looping = StreamParams { looping: true, ..StreamParams::default() };
        let (old, _) = audio.play_stream(tone(), looping).unwrap();
        let mut out = vec![0f32; 4800];
        audio.pull(&mut out).unwrap();
        while audio.command_room() > 1 {
            audio.set_bus_gain(Bus::Sfx, 1.0, 0.0).unwrap();
        }
        let e = audio.crossfade(Some(old), tone(), looping, 0.05).unwrap_err();
        assert_eq!(e.code, ErrorCode::QueueFull);
        for _ in 0..20 {
            audio.pull(&mut out).unwrap();
            audio.service();
        }
        assert_eq!(audio.stats().active_streams, 1, "the old stream plays on, nothing new started");
        assert!(!audio.drain_finished().contains(&old));
        // With room, it hands over: one stream left once the fade is done.
        let (new, _) = audio.crossfade(Some(old), tone(), looping, 0.05).unwrap();
        for _ in 0..20 {
            audio.pull(&mut out).unwrap();
            audio.service();
        }
        assert_eq!(audio.stats().active_streams, 1);
        let done = audio.drain_finished();
        assert!(done.contains(&old) && !done.contains(&new));
    }

    /// Review round 2 probe "sinc": resample() built its table on every call (1.6 ms each, once
    /// per pitch of every piece). It shares one now.
    #[test]
    fn resampling_shares_one_table() {
        let x = vec![0.5f32; 64];
        let start = std::time::Instant::now();
        for _ in 0..200 {
            let _ = resample::resample(&x, 1, 44_100, 48_000);
        }
        assert!(start.elapsed() < specula_core::speed_budget(std::time::Duration::from_secs(1)), "{:?}", start.elapsed());
    }

    /// The threaded worker fills streams on its own (a worker that never ran survived the
    /// manual-backend tests).
    #[test]
    fn the_worker_thread_fills_streams() {
        let audio = Audio::start(AudioConfig { backend: Backend::Null, ..Default::default() }).unwrap();
        let tone = Sound::new("tone", 48_000, 1, (0..96_000).map(|i| (i as f32 * 0.05).sin() * 0.3).collect()).unwrap();
        let (_, h) = audio.play_stream(Box::new(stream::SoundSource::new(tone)), StreamParams::default()).unwrap();
        let start = std::time::Instant::now();
        while h.status().played_frames < 9_600 && start.elapsed() < std::time::Duration::from_secs(5) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let st = h.status();
        assert!(st.produced_frames >= 9_600 && st.played_frames >= 9_600, "{st:?}");
        assert!(audio.stats().worker_ms_total > 0.0);
    }

    /// Starvation is traced (audio.stream_starved), once per new starvation.
    #[test]
    fn starvation_is_traced() {
        let audio = Audio::start(AudioConfig { backend: Backend::Manual, ..Default::default() }).unwrap();
        let tone = Sound::new("starving_tone", 48_000, 1, vec![0.1; 480_000]).unwrap();
        let (_, h) = audio.play_stream(Box::new(stream::SoundSource::new(tone)), StreamParams::default()).unwrap();
        // Pull without servicing: the ring runs dry.
        let mut out = vec![0f32; 4800];
        for _ in 0..20 {
            audio.pull(&mut out).unwrap();
        }
        assert!(h.status().starve_events >= 1);
        audio.service();
        let traced = specula_core::trace::read(|ring| {
            ring.iter().filter(|e| e.code == "audio.stream_starved" && format!("{:?}", e.fields).contains("starving_tone")).count()
        });
        assert_eq!(traced, 1, "one trace for the one starvation");
        audio.service();
        let again = specula_core::trace::read(|ring| ring.iter().filter(|e| e.code == "audio.stream_starved" && format!("{:?}", e.fields).contains("starving_tone")).count());
        assert_eq!(again, 1, "not traced again without a new starvation");
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
