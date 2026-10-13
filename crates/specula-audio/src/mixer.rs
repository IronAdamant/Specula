//! The mixer: voices → buses → master, run on the audio thread.
//!
//! The game thread talks to it only through a bounded SPSC ring of [`Command`]s (see
//! [`crate::ring`]); finished sounds travel back on a second ring so their memory is freed on the
//! game thread. [`Mixer::render`] never allocates, locks or blocks: all buffers are sized in
//! [`Mixer::new`], and a voice's sound is an `Arc` handed over in its `Play` command.
//!
//! Signal path per output block (≤ [`MAX_BLOCK`] frames):
//!
//! ```text
//! voice: sinc SRC (rate × pitch) → [low-pass by distance] → gain · fade · distance → pan ┐
//! bus (sfx, ui, ambience, music, master-direct): Σ voices · bus gain · duck gain        ├→ master
//! master: Σ buses · master gain → block-lookahead peak limiter (ceiling -0.2 dBFS)       ┘
//! ```

use crate::dsp::{self, OnePole};
use crate::resample::{SincTable, FRAC_BITS, ONE, TAPS};
use crate::ring::{self, Consumer, Producer};
use crate::sound::{Sound, SoundData};
use crate::error::{AudioError, ErrorCode, Result};
use specula_core::dmath;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// Frames mixed per internal block (5.3 ms at 48 kHz); `render` splits larger requests.
pub const MAX_BLOCK: usize = 256;
/// Extra voice slots where stolen voices fade out over a few ms instead of clicking.
const TAIL_SLOTS: usize = 8;
/// Fade used when a voice is stolen or stopped without a fade.
const DECLICK_S: f32 = 0.004;
/// Limiter ceiling (linear, ≈ -0.2 dBFS) and release time.
const LIMIT_CEILING: f32 = 0.977;
const LIMIT_RELEASE_S: f32 = 0.15;
/// Duck rules that can be active at once.
pub const MAX_DUCKS: usize = 4;

/// A mixer bus. Every voice plays on one; each has a volume, and buses can duck each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bus {
    /// Straight to the master (no bus volume besides the master's).
    Master = 0,
    Sfx = 1,
    Ui = 2,
    Ambience = 3,
    Music = 4,
}

pub const BUS_COUNT: usize = 5;

impl Bus {
    pub const ALL: [Bus; BUS_COUNT] = [Bus::Master, Bus::Sfx, Bus::Ui, Bus::Ambience, Bus::Music];

    pub fn name(self) -> &'static str {
        match self {
            Bus::Master => "master",
            Bus::Sfx => "sfx",
            Bus::Ui => "ui",
            Bus::Ambience => "ambience",
            Bus::Music => "music",
        }
    }

    pub fn from_name(name: &str) -> Option<Bus> {
        Bus::ALL.into_iter().find(|b| b.name() == name)
    }
}

/// Identifies a playing voice. Ids are never reused within one [`crate::Audio`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VoiceId(pub u64);

/// 3D placement of a voice, relative to the [`Listener`]. Metres, right-handed, +Y up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spatial {
    pub position: [f32; 3],
    /// Full volume inside this distance.
    pub min_distance: f32,
    /// Silent beyond this distance (the voice keeps its place but costs no mixing).
    pub max_distance: f32,
    /// Inverse-distance rolloff factor (1 = physical 1/r beyond `min_distance`).
    pub rolloff: f32,
    /// Darken far sounds with a low-pass (20 kHz at `min_distance` → ~1.5 kHz at `max_distance`).
    pub lowpass: bool,
}

impl Spatial {
    pub fn at(position: [f32; 3]) -> Spatial {
        Spatial { position, min_distance: 2.0, max_distance: 80.0, rolloff: 1.0, lowpass: true }
    }
}

/// How to play a sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayParams {
    pub bus: Bus,
    /// Linear gain (1 = as rendered).
    pub gain: f32,
    /// Stereo position -1 (left) .. 1 (right); ignored for spatial voices.
    pub pan: f32,
    /// Playback-rate ratio (2 = an octave up, shorter). Clamped to 1/16 .. 16.
    pub pitch: f32,
    pub looping: bool,
    /// Higher priorities steal from lower ones when all voices are busy (default 128).
    pub priority: u8,
    /// Fade in over this many seconds (0 = start at full gain).
    pub fade_in_s: f32,
    pub spatial: Option<Spatial>,
}

impl Default for PlayParams {
    fn default() -> Self {
        PlayParams { bus: Bus::Sfx, gain: 1.0, pan: 0.0, pitch: 1.0, looping: false, priority: 128, fade_in_s: 0.0, spatial: None }
    }
}

/// Where the ears are. `forward` and `up` need not be normalised but must not be parallel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    pub position: [f32; 3],
    pub forward: [f32; 3],
    pub up: [f32; 3],
}

impl Default for Listener {
    fn default() -> Self {
        Listener { position: [0.0; 3], forward: [0.0, 0.0, -1.0], up: [0.0, 1.0, 0.0] }
    }
}

/// While `source` is louder than `threshold_db`, turn `target` down by `depth_db`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DuckRule {
    pub source: Bus,
    pub target: Bus,
    /// Gain reduction in dB (positive, e.g. 9).
    pub depth_db: f32,
    /// Source level (dBFS, peak follower) above which ducking starts.
    pub threshold_db: f32,
    pub attack_s: f32,
    pub release_s: f32,
}

pub(crate) enum Command {
    Play { id: u64, sound: Arc<SoundData>, params: PlayParams },
    Stop { id: u64, fade_s: f32 },
    StopAll { bus: Option<Bus>, fade_s: f32 },
    SetGain { id: u64, gain: f32, fade_s: f32 },
    SetPitch { id: u64, pitch: f32 },
    SetPan { id: u64, pan: f32 },
    SetPosition { id: u64, position: [f32; 3] },
    SetBusGain { bus: Bus, gain: f32, fade_s: f32 },
    SetDuck { slot: usize, rule: Option<DuckRule> },
    SetListener(Listener),
}

/// Counters the audio thread publishes; read with [`Stats`].
#[derive(Default)]
pub(crate) struct SharedStats {
    pub active_voices: AtomicU32,
    pub peak_voices: AtomicU32,
    pub stolen: AtomicU64,
    pub rejected: AtomicU64,
    pub frames: AtomicU64,
    pub blocks: AtomicU64,
    pub mix_ns_last: AtomicU64,
    pub mix_ns_max: AtomicU64,
    pub mix_ns_total: AtomicU64,
    pub master_peak_bits: AtomicU32,
    pub limiter_gain_bits: AtomicU32,
    pub output_rate: AtomicU32,
    // Backend fields (written by the backend thread).
    pub device_channels: AtomicU32,
    pub buffer_frames: AtomicU32,
    pub latency_us: AtomicU32,
    pub backend_state: AtomicU32,
    pub underruns: AtomicU64,
    pub reconnects: AtomicU64,
    pub last_hresult: AtomicU32,
    pub queue_full: AtomicU64,
}

#[derive(Clone, Copy, Debug)]
struct Ramp {
    value: f32,
    target: f32,
    /// Change per frame.
    step: f32,
}

impl Ramp {
    fn new(v: f32) -> Ramp {
        Ramp { value: v, target: v, step: 0.0 }
    }

    fn set(&mut self, target: f32, seconds: f32, rate: f32) {
        self.target = target;
        let frames = (seconds * rate).max(0.0);
        if frames < 1.0 {
            self.value = target;
            self.step = 0.0;
        } else {
            self.step = (target - self.value) / frames;
        }
    }

    /// Advances `frames` frames; returns the new value.
    fn advance(&mut self, frames: usize) -> f32 {
        if self.step != 0.0 {
            self.value += self.step * frames as f32;
            if (self.step > 0.0 && self.value >= self.target) || (self.step < 0.0 && self.value <= self.target) {
                self.value = self.target;
                self.step = 0.0;
            }
        }
        self.value
    }

    fn done(&self) -> bool {
        self.step == 0.0
    }
}

struct Voice {
    id: u64,
    sound: Arc<SoundData>,
    /// Read position in source frames, 32.32 fixed point.
    pos: u64,
    /// Source frames per output frame at pitch 1.
    base_ratio: f64,
    pitch: f32,
    step: u64,
    kernel: usize,
    params: PlayParams,
    gain: Ramp,
    fade: Ramp,
    stopping: bool,
    /// A stolen voice fading out in a tail slot (doesn't count against the cap).
    tail: bool,
    finished: bool,
    /// Has wrapped around its loop at least once (reads before the loop start come from its end).
    wrapped: bool,
    /// Channel gains applied at the end of the last block (ramped from, to avoid zipper noise).
    cur: [f32; 2],
    started: bool,
    lp: [OnePole; 2],
    lp_on: bool,
    age: u64,
}

impl Voice {
    fn update_step(&mut self) {
        let ratio = self.base_ratio * self.pitch.clamp(1.0 / 16.0, 16.0) as f64;
        self.step = ((ratio * ONE as f64) as u64).max(1);
        self.kernel = SincTable::kernel_for_step(self.step);
    }

    fn loudness(&self) -> f32 {
        self.cur[0].max(self.cur[1])
    }
}

struct BusState {
    gain: Ramp,
    duck: f32,
    /// Peak follower of the bus output (linear).
    level: f32,
    buf: Vec<f32>,
}

/// The game-thread side: sends commands, receives finished voices.
pub struct MixerHandle {
    commands: Producer<Command>,
    returns: Consumer<(u64, Arc<SoundData>)>,
    next_id: u64,
    finished: Vec<VoiceId>,
    pub(crate) stats: Arc<SharedStats>,
}

/// Most finished-voice ids kept for [`MixerHandle::drain_finished`] before the oldest are dropped.
const MAX_FINISHED: usize = 4096;

impl MixerHandle {
    fn collect(&mut self) {
        while let Some((id, sound)) = self.returns.pop() {
            drop(sound);
            if self.finished.len() >= MAX_FINISHED {
                self.finished.remove(0);
            }
            self.finished.push(VoiceId(id));
        }
    }

    fn send(&mut self, cmd: Command) -> Result<()> {
        self.collect();
        self.commands.push(cmd).map_err(|_| {
            self.stats.queue_full.fetch_add(1, Ordering::Relaxed);
            AudioError::new(ErrorCode::QueueFull, "audio command queue is full; command dropped")
        })
    }

    /// Starts a voice. The id is valid even if the mixer later rejects the voice (cap reached
    /// with higher-priority voices); it then simply reports as finished.
    pub fn play(&mut self, sound: &Sound, params: PlayParams) -> Result<VoiceId> {
        let id = self.next_id;
        self.send(Command::Play { id, sound: sound.data.clone(), params })?;
        self.next_id += 1;
        Ok(VoiceId(id))
    }

    pub fn stop(&mut self, id: VoiceId, fade_s: f32) -> Result<()> {
        self.send(Command::Stop { id: id.0, fade_s })
    }

    /// Stops every voice (on one bus, or all).
    pub fn stop_all(&mut self, bus: Option<Bus>, fade_s: f32) -> Result<()> {
        self.send(Command::StopAll { bus, fade_s })
    }

    pub fn set_gain(&mut self, id: VoiceId, gain: f32, fade_s: f32) -> Result<()> {
        self.send(Command::SetGain { id: id.0, gain: sane(gain, 0.0, 16.0), fade_s })
    }

    pub fn set_pitch(&mut self, id: VoiceId, pitch: f32) -> Result<()> {
        self.send(Command::SetPitch { id: id.0, pitch: sane(pitch, 1.0 / 16.0, 16.0) })
    }

    pub fn set_pan(&mut self, id: VoiceId, pan: f32) -> Result<()> {
        self.send(Command::SetPan { id: id.0, pan: sane(pan, -1.0, 1.0) })
    }

    pub fn set_position(&mut self, id: VoiceId, position: [f32; 3]) -> Result<()> {
        self.send(Command::SetPosition { id: id.0, position })
    }

    pub fn set_bus_gain(&mut self, bus: Bus, gain: f32, fade_s: f32) -> Result<()> {
        self.send(Command::SetBusGain { bus, gain: sane(gain, 0.0, 16.0), fade_s })
    }

    /// Installs (or clears, with `None`) duck rule `slot` (0..[`MAX_DUCKS`]).
    pub fn set_duck(&mut self, slot: usize, rule: Option<DuckRule>) -> Result<()> {
        if slot >= MAX_DUCKS {
            return Err(AudioError::new(ErrorCode::OutOfRange, "duck slot out of range").with_numbers(slot as f64, MAX_DUCKS as f64 - 1.0));
        }
        self.send(Command::SetDuck { slot, rule })
    }

    pub fn set_listener(&mut self, listener: Listener) -> Result<()> {
        self.send(Command::SetListener(listener))
    }

    /// Voices that ended (played out, stopped, stolen or rejected) since the last call.
    pub fn drain_finished(&mut self) -> Vec<VoiceId> {
        self.collect();
        std::mem::take(&mut self.finished)
    }
}

fn sane(v: f32, lo: f32, hi: f32) -> f32 {
    if v.is_nan() {
        lo
    } else {
        v.clamp(lo, hi)
    }
}

/// The audio-thread side.
pub struct Mixer {
    rate: u32,
    table: SincTable,
    voices: Vec<Voice>,
    max_voices: usize,
    buses: [BusState; BUS_COUNT],
    master_tmp: Vec<f32>,
    ducks: [Option<DuckRule>; MAX_DUCKS],
    listener: Listener,
    right: [f32; 3],
    limiter: f32,
    commands: Consumer<Command>,
    returns: Producer<(u64, Arc<SoundData>)>,
    stats: Arc<SharedStats>,
    age: u64,
}

/// Configuration for [`Mixer::new`].
#[derive(Clone, Copy, Debug)]
pub struct MixerConfig {
    pub rate: u32,
    pub max_voices: usize,
    pub command_capacity: usize,
}

impl Default for MixerConfig {
    fn default() -> Self {
        MixerConfig { rate: 48_000, max_voices: 64, command_capacity: 1024 }
    }
}

impl Mixer {
    pub fn new(config: MixerConfig) -> (MixerHandle, Mixer) {
        let max_voices = config.max_voices.clamp(1, 1024);
        let (cmd_tx, cmd_rx) = ring::channel(config.command_capacity.max(16));
        // Every Arc that can be on the audio side at once (one per queued command, one per
        // voice) fits, so returning one never fails while the game thread keeps calling in.
        let (ret_tx, ret_rx) = ring::channel(cmd_tx.capacity() + max_voices + TAIL_SLOTS + 16);
        let stats = Arc::new(SharedStats::default());
        stats.output_rate.store(config.rate, Ordering::Relaxed);
        stats.limiter_gain_bits.store(1f32.to_bits(), Ordering::Relaxed);
        let bus = || BusState { gain: Ramp::new(1.0), duck: 1.0, level: 0.0, buf: vec![0.0; MAX_BLOCK * 2] };
        let mut mixer = Mixer {
            rate: config.rate.max(1),
            table: SincTable::new(),
            voices: Vec::with_capacity(max_voices + TAIL_SLOTS),
            max_voices,
            buses: [bus(), bus(), bus(), bus(), bus()],
            master_tmp: vec![0.0; MAX_BLOCK * 2],
            ducks: [None; MAX_DUCKS],
            listener: Listener::default(),
            right: [1.0, 0.0, 0.0],
            limiter: 1.0,
            commands: cmd_rx,
            returns: ret_tx,
            stats: stats.clone(),
            age: 0,
        };
        mixer.apply_listener(Listener::default());
        (MixerHandle { commands: cmd_tx, returns: ret_rx, next_id: 1, finished: Vec::new(), stats }, mixer)
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Changes the output rate (a device reconnected at another rate). Voices keep their place.
    pub fn set_rate(&mut self, rate: u32) {
        let rate = rate.max(1);
        if rate == self.rate {
            return;
        }
        self.rate = rate;
        self.stats.output_rate.store(rate, Ordering::Relaxed);
        for v in &mut self.voices {
            v.base_ratio = v.sound.rate as f64 / rate as f64;
            v.update_step();
        }
    }

    pub(crate) fn stats(&self) -> &Arc<SharedStats> {
        &self.stats
    }

    fn apply_listener(&mut self, l: Listener) {
        let f = normalize(l.forward).unwrap_or([0.0, 0.0, -1.0]);
        let u = normalize(l.up).unwrap_or([0.0, 1.0, 0.0]);
        // right = forward × up (right-handed, +Y up, -Z forward → +X right).
        let r = [f[1] * u[2] - f[2] * u[1], f[2] * u[0] - f[0] * u[2], f[0] * u[1] - f[1] * u[0]];
        self.right = normalize(r).unwrap_or([1.0, 0.0, 0.0]);
        self.listener = l;
    }

    fn find(&mut self, id: u64) -> Option<&mut Voice> {
        self.voices.iter_mut().find(|v| v.id == id && !v.tail)
    }

    fn handle(&mut self, cmd: Command) {
        let rate = self.rate as f32;
        match cmd {
            Command::Play { id, sound, params } => self.start_voice(id, sound, params),
            Command::Stop { id, fade_s } => {
                if let Some(v) = self.find(id) {
                    v.stopping = true;
                    v.fade.set(0.0, fade_s.max(DECLICK_S), rate);
                }
            }
            Command::StopAll { bus, fade_s } => {
                for v in self.voices.iter_mut().filter(|v| bus.is_none_or(|b| v.params.bus == b)) {
                    v.stopping = true;
                    v.fade.set(0.0, fade_s.max(DECLICK_S), rate);
                }
            }
            Command::SetGain { id, gain, fade_s } => {
                if let Some(v) = self.find(id) {
                    v.gain.set(gain, fade_s, rate);
                }
            }
            Command::SetPitch { id, pitch } => {
                if let Some(v) = self.find(id) {
                    v.pitch = pitch;
                    v.update_step();
                }
            }
            Command::SetPan { id, pan } => {
                if let Some(v) = self.find(id) {
                    v.params.pan = pan;
                }
            }
            Command::SetPosition { id, position } => {
                if let Some(v) = self.find(id) {
                    if let Some(s) = &mut v.params.spatial {
                        s.position = position;
                    }
                }
            }
            Command::SetBusGain { bus, gain, fade_s } => self.buses[bus as usize].gain.set(gain, fade_s, rate),
            Command::SetDuck { slot, rule } => self.ducks[slot] = rule,
            Command::SetListener(l) => self.apply_listener(l),
        }
    }

    fn start_voice(&mut self, id: u64, sound: Arc<SoundData>, mut params: PlayParams) {
        params.gain = sane(params.gain, 0.0, 16.0);
        params.pan = sane(params.pan, -1.0, 1.0);
        params.pitch = sane(params.pitch, 1.0 / 16.0, 16.0);
        let live = self.voices.iter().filter(|v| !v.tail).count();
        if live >= self.max_voices {
            // Steal the least important live voice: lowest priority, then quietest, then oldest.
            let victim = self
                .voices
                .iter()
                .enumerate()
                .filter(|(_, v)| !v.tail)
                .min_by(|(_, a), (_, b)| {
                    a.params.priority.cmp(&b.params.priority).then(a.loudness().total_cmp(&b.loudness())).then(a.age.cmp(&b.age))
                })
                .map(|(i, v)| (i, v.params.priority));
            match victim {
                Some((i, prio)) if prio <= params.priority => {
                    self.stats.stolen.fetch_add(1, Ordering::Relaxed);
                    let tails = self.voices.iter().filter(|v| v.tail).count();
                    let v = &mut self.voices[i];
                    if tails < TAIL_SLOTS {
                        v.tail = true;
                        v.stopping = true;
                        v.fade.set(0.0, DECLICK_S, self.rate as f32);
                    } else {
                        v.finished = true;
                        v.tail = true;
                    }
                }
                _ => {
                    self.stats.rejected.fetch_add(1, Ordering::Relaxed);
                    // Hand the sound straight back (capacity guarantees room; see `new`).
                    let _ = self.returns.push((id, sound));
                    return;
                }
            }
        }
        if self.voices.len() == self.voices.capacity() {
            // All tail slots busy with voices not yet removed: finish the oldest tail now.
            if let Some(v) = self.voices.iter_mut().filter(|v| v.tail).min_by_key(|v| v.age) {
                v.finished = true;
            }
            self.reap();
            if self.voices.len() == self.voices.capacity() {
                self.stats.rejected.fetch_add(1, Ordering::Relaxed);
                let _ = self.returns.push((id, sound));
                return;
            }
        }
        self.age += 1;
        let mut fade = Ramp::new(if params.fade_in_s > 0.0 { 0.0 } else { 1.0 });
        fade.set(1.0, params.fade_in_s, self.rate as f32);
        let base_ratio = sound.rate as f64 / self.rate as f64;
        let mut v = Voice {
            id,
            sound,
            pos: 0,
            base_ratio,
            pitch: params.pitch,
            step: ONE,
            kernel: 0,
            gain: Ramp::new(params.gain),
            fade,
            params,
            stopping: false,
            tail: false,
            finished: false,
            wrapped: false,
            cur: [0.0; 2],
            started: false,
            lp: [OnePole::new(false); 2],
            lp_on: false,
            age: self.age,
        };
        v.update_step();
        self.voices.push(v);
        let live = self.voices.iter().filter(|v| !v.tail).count() as u32;
        self.stats.peak_voices.fetch_max(live, Ordering::Relaxed);
    }

    /// Removes finished voices, returning their sounds to the game thread.
    fn reap(&mut self) {
        let mut i = 0;
        while i < self.voices.len() {
            if self.voices[i].finished {
                let v = self.voices.swap_remove(i);
                if let Err((id, sound)) = self.returns.push((v.id, v.sound)) {
                    // Can't happen while the capacity invariant holds; keep it as a silent zombie
                    // rather than freeing memory on the audio thread.
                    let mut zombie = v_zombie(id, sound);
                    zombie.finished = true;
                    self.voices.push(zombie);
                    break;
                }
            } else {
                i += 1;
            }
        }
    }

    /// Fills `out` (interleaved stereo, `out.len() / 2` frames) with the mix.
    pub fn render(&mut self, out: &mut [f32]) {
        let start = std::time::Instant::now();
        while let Some(cmd) = self.commands.pop() {
            self.handle(cmd);
        }
        for chunk in out.chunks_mut(MAX_BLOCK * 2) {
            self.render_block(chunk);
        }
        let ns = start.elapsed().as_nanos() as u64;
        let s = &self.stats;
        s.mix_ns_last.store(ns, Ordering::Relaxed);
        s.mix_ns_max.fetch_max(ns, Ordering::Relaxed);
        s.mix_ns_total.fetch_add(ns, Ordering::Relaxed);
        s.frames.fetch_add((out.len() / 2) as u64, Ordering::Relaxed);
        s.blocks.fetch_add(1, Ordering::Relaxed);
        s.active_voices.store(self.voices.iter().filter(|v| !v.tail).count() as u32, Ordering::Relaxed);
    }

    fn render_block(&mut self, out: &mut [f32]) {
        let n = out.len() / 2;
        let rate = self.rate as f32;
        for b in &mut self.buses {
            b.buf[..n * 2].fill(0.0);
        }
        let listener = self.listener;
        let right = self.right;
        for v in &mut self.voices {
            if v.finished {
                continue;
            }
            let bus = &mut self.buses[v.params.bus as usize].buf[..n * 2];
            render_voice(v, &self.table, bus, n, rate, &listener, right);
        }
        self.reap();

        // Bus levels (peak followers) and ducking.
        let block_s = n as f32 / rate;
        for b in &mut self.buses {
            let peak = b.buf[..n * 2].iter().fold(0.0f32, |m, s| m.max(s.abs())) * b.gain.value;
            // Instant attack, ~300 ms release.
            b.level = if peak > b.level { peak } else { b.level * exp_decay(block_s, 0.3) };
        }
        let mut duck_target = [1.0f32; BUS_COUNT];
        let mut duck_speed = [(0.05f32, 0.3f32); BUS_COUNT];
        for rule in self.ducks.iter().flatten() {
            let level_db = dsp::gain_to_db(self.buses[rule.source as usize].level as f64) as f32;
            if level_db > rule.threshold_db {
                let g = dsp::db_to_gain(-(rule.depth_db.abs() as f64)) as f32;
                let t = &mut duck_target[rule.target as usize];
                *t = t.min(g);
            }
            duck_speed[rule.target as usize] = (rule.attack_s.max(0.001), rule.release_s.max(0.001));
        }

        let master = &mut self.master_tmp[..n * 2];
        master.fill(0.0);
        for (i, b) in self.buses.iter_mut().enumerate() {
            let (attack, release) = duck_speed[i];
            let target = duck_target[i];
            let tc = if target < b.duck { attack } else { release };
            let k = 1.0 - exp_decay(block_s, tc);
            let duck0 = b.duck;
            b.duck += (target - b.duck) * k;
            let g0 = if i == 0 { 1.0 } else { b.gain.value * duck0 };
            if i != 0 {
                b.gain.advance(n);
            }
            let g1 = if i == 0 { 1.0 } else { b.gain.value * b.duck };
            if g0 == 0.0 && g1 == 0.0 {
                continue;
            }
            let dg = (g1 - g0) / n as f32;
            for f in 0..n {
                let g = g0 + dg * (f + 1) as f32;
                master[2 * f] += b.buf[2 * f] * g;
                master[2 * f + 1] += b.buf[2 * f + 1] * g;
            }
        }
        // The master bus's own gain applies to everything.
        let m0 = self.buses[0].gain.value;
        let m1 = self.buses[0].gain.advance(n);
        let peak_pre = master.iter().fold(0.0f32, |m, s| m.max(s.abs())) * m0.max(m1);
        let lim_target = if peak_pre > LIMIT_CEILING { LIMIT_CEILING / peak_pre } else { 1.0 };
        let l0 = self.limiter;
        // Release towards 1, but never above what keeps this block under the ceiling.
        let released = l0 + (1.0 - l0) * (1.0 - exp_decay(block_s, LIMIT_RELEASE_S));
        let l1 = lim_target.min(released);
        self.limiter = l1;
        // Attack is instant (the whole block is known: a one-block look-ahead), release is
        // ramped; a final clamp is the last line of defence.
        let (c0, c1) = (m0 * l0.min(l1), m1 * l1);
        let dc = (c1 - c0) / n as f32;
        let mut peak = 0.0f32;
        for f in 0..n {
            let g = c0 + dc * (f + 1) as f32;
            let l = (master[2 * f] * g).clamp(-1.0, 1.0);
            let r = (master[2 * f + 1] * g).clamp(-1.0, 1.0);
            peak = peak.max(l.abs()).max(r.abs());
            out[2 * f] = l;
            out[2 * f + 1] = r;
        }
        self.stats.master_peak_bits.store(peak.to_bits(), Ordering::Relaxed);
        self.stats.limiter_gain_bits.store(l1.to_bits(), Ordering::Relaxed);
    }
}

fn v_zombie(id: u64, sound: Arc<SoundData>) -> Voice {
    Voice {
        id,
        sound,
        pos: 0,
        base_ratio: 1.0,
        pitch: 1.0,
        step: ONE,
        kernel: 0,
        params: PlayParams::default(),
        gain: Ramp::new(0.0),
        fade: Ramp::new(0.0),
        stopping: true,
        tail: true,
        finished: true,
        wrapped: false,
        cur: [0.0; 2],
        started: true,
        lp: [OnePole::new(false); 2],
        lp_on: false,
        age: 0,
    }
}

/// `e^(-dt/tc)`.
fn exp_decay(dt: f32, tc: f32) -> f32 {
    dmath::exp(-dt / tc.max(1e-4))
}

fn normalize(v: [f32; 3]) -> Option<[f32; 3]> {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (l > 1e-6 && l.is_finite()).then(|| [v[0] / l, v[1] / l, v[2] / l])
}

/// Distance gain and pan for a spatial voice.
fn spatialize(s: &Spatial, listener: &Listener, right: [f32; 3]) -> (f32, f32, f32) {
    let d = [s.position[0] - listener.position[0], s.position[1] - listener.position[1], s.position[2] - listener.position[2]];
    let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if !dist.is_finite() {
        return (0.0, 0.0, 0.0);
    }
    let min_d = s.min_distance.max(0.01);
    let max_d = s.max_distance.max(min_d + 0.01);
    if dist >= max_d {
        return (0.0, 0.0, dist);
    }
    let clamped = dist.max(min_d);
    let mut att = min_d / (min_d + s.rolloff.max(0.0) * (clamped - min_d));
    // Fade the last 20% of the range to silence, so the cut-off at max_distance is inaudible.
    let edge = 0.8 * max_d;
    if dist > edge {
        att *= 1.0 - (dist - edge) / (max_d - edge);
    }
    let pan = if dist > 1e-4 {
        let side = (d[0] * right[0] + d[1] * right[1] + d[2] * right[2]) / dist;
        // Inside min_distance the source surrounds you: narrow the pan towards the centre.
        side * (dist / min_d).min(1.0)
    } else {
        0.0
    };
    (att, pan, dist)
}

/// Mixes one voice into its bus buffer (interleaved stereo, `n` frames).
fn render_voice(v: &mut Voice, table: &SincTable, bus: &mut [f32], n: usize, rate: f32, listener: &Listener, right: [f32; 3]) {
    // Gains at the end of this block.
    v.gain.advance(n);
    v.fade.advance(n);
    if v.stopping && v.fade.done() && v.fade.value <= 0.0 {
        v.finished = true;
        return;
    }
    let base = v.gain.value * v.fade.value;
    let (target, lp_coef) = match v.params.spatial {
        Some(s) => {
            let (att, pan, dist) = spatialize(&s, listener, right);
            let (l, r) = dsp::pan_gains(pan);
            let coef = if s.lowpass {
                let range = (s.max_distance - s.min_distance).max(0.01);
                let x = ((dist - s.min_distance) / range).clamp(0.0, 1.0);
                // 20 kHz → 1.5 kHz, log-spaced (x^0.7 keeps the near field brighter).
                let fc = 20_000.0 * dmath::exp(dmath::pow(x, 0.7) * -2.590_267);
                Some(OnePole::coefficient(fc as f64, rate as f64))
            } else {
                None
            };
            ([base * att * l, base * att * r], coef)
        }
        None => {
            let p = v.params.pan;
            let g = if v.sound.channels == 1 {
                let (l, r) = dsp::pan_gains(p);
                [l, r]
            } else {
                // Stereo balance: centre leaves both channels at unity.
                [(1.0 - p).min(1.0), (1.0 + p).min(1.0)]
            };
            ([base * g[0], base * g[1]], None)
        }
    };
    if !v.started {
        v.started = true;
        // Start at the target (sharp attacks stay sharp); a fade-in is already in `fade`.
        v.cur = if v.fade.value < 1.0 && v.params.fade_in_s > 0.0 { [0.0; 2] } else { target };
        if let Some(c) = lp_coef {
            // Settle the filter at its starting cutoff.
            for f in &mut v.lp {
                f.set_coefficient(c);
            }
        }
    }
    match lp_coef {
        Some(c) => {
            v.lp_on = true;
            for f in &mut v.lp {
                f.set_coefficient(c);
            }
        }
        None => v.lp_on = false,
    }
    let g0 = v.cur;
    v.cur = target;
    let silent = g0 == [0.0; 2] && target == [0.0; 2];
    let d = [(target[0] - g0[0]) / n as f32, (target[1] - g0[1]) / n as f32];

    let data = &*v.sound;
    let ch = data.channels as usize;
    let spatial_downmix = ch == 2 && v.params.spatial.is_some();
    let looping = v.params.looping && data.loop_end > data.loop_start;
    let limit = if looping { data.loop_end } else { data.frames };
    let loop_len = (data.loop_end - data.loop_start) as u64;
    let samples = &data.samples[..];

    for f in 0..n {
        let idx = (v.pos >> FRAC_BITS) as usize;
        if !looping && idx >= data.frames {
            v.finished = true;
            break;
        }
        if !silent {
            let frac = v.pos as u32;
            let (mut l, mut r);
            if v.step == ONE && frac == 0 {
                // Same rate, integer position: no interpolation needed.
                if ch == 1 {
                    l = samples[idx];
                    r = l;
                } else {
                    l = samples[2 * idx];
                    r = samples[2 * idx + 1];
                }
            } else {
                let c = table.coeffs(v.kernel, frac);
                if idx >= 3 && idx + TAPS - 3 < limit {
                    let start = idx - 3;
                    if ch == 1 {
                        let s = &samples[start..start + TAPS];
                        let mut acc = 0.0;
                        for k in 0..TAPS {
                            acc += c[k] * s[k];
                        }
                        l = acc;
                        r = acc;
                    } else {
                        let s = &samples[2 * start..2 * (start + TAPS)];
                        let (mut al, mut ar) = (0.0, 0.0);
                        for k in 0..TAPS {
                            al += c[k] * s[2 * k];
                            ar += c[k] * s[2 * k + 1];
                        }
                        l = al;
                        r = ar;
                    }
                } else {
                    // Near the edges: fetch each tap with bounds / loop wrapping.
                    let (mut al, mut ar) = (0.0, 0.0);
                    for (k, ck) in c.iter().enumerate() {
                        let mut i = idx as i64 - 3 + k as i64;
                        if looping {
                            if i >= data.loop_end as i64 {
                                i = data.loop_start as i64 + (i - data.loop_start as i64) % loop_len as i64;
                            } else if i < data.loop_start as i64 && v.wrapped {
                                i += loop_len as i64;
                            }
                        }
                        if i < 0 || i as usize >= data.frames {
                            continue;
                        }
                        let i = i as usize;
                        if ch == 1 {
                            al += ck * samples[i];
                        } else {
                            al += ck * samples[2 * i];
                            ar += ck * samples[2 * i + 1];
                        }
                    }
                    l = al;
                    r = if ch == 1 { al } else { ar };
                }
            }
            if spatial_downmix {
                l = 0.5 * (l + r);
                r = l;
            }
            if v.lp_on {
                l = v.lp[0].process(l);
                r = if ch == 1 || spatial_downmix { l } else { v.lp[1].process(r) };
            }
            let gl = g0[0] + d[0] * (f + 1) as f32;
            let gr = g0[1] + d[1] * (f + 1) as f32;
            bus[2 * f] += l * gl;
            bus[2 * f + 1] += r * gr;
        }
        v.pos += v.step;
        if looping && (v.pos >> FRAC_BITS) >= limit as u64 {
            v.pos -= loop_len << FRAC_BITS;
            v.wrapped = true;
        }
    }
    if v.tail && v.fade.done() {
        v.finished = true;
    }
}

impl Stats {
    pub(crate) fn read(s: &SharedStats) -> Stats {
        let blocks = s.blocks.load(Ordering::Relaxed);
        let rate = s.output_rate.load(Ordering::Relaxed);
        Stats {
            backend: BackendState::from_u32(s.backend_state.load(Ordering::Relaxed)),
            output_rate: rate,
            device_channels: s.device_channels.load(Ordering::Relaxed),
            buffer_frames: s.buffer_frames.load(Ordering::Relaxed),
            latency_ms: s.latency_us.load(Ordering::Relaxed) as f32 / 1000.0,
            active_voices: s.active_voices.load(Ordering::Relaxed),
            peak_voices: s.peak_voices.load(Ordering::Relaxed),
            voices_stolen: s.stolen.load(Ordering::Relaxed),
            voices_rejected: s.rejected.load(Ordering::Relaxed),
            frames_mixed: s.frames.load(Ordering::Relaxed),
            mix_calls: blocks,
            mix_us_last: s.mix_ns_last.load(Ordering::Relaxed) as f32 / 1000.0,
            mix_us_max: s.mix_ns_max.load(Ordering::Relaxed) as f32 / 1000.0,
            mix_us_mean: if blocks == 0 { 0.0 } else { (s.mix_ns_total.load(Ordering::Relaxed) / blocks) as f32 / 1000.0 },
            master_peak: f32::from_bits(s.master_peak_bits.load(Ordering::Relaxed)),
            limiter_gain: f32::from_bits(s.limiter_gain_bits.load(Ordering::Relaxed)),
            underruns: s.underruns.load(Ordering::Relaxed),
            reconnects: s.reconnects.load(Ordering::Relaxed),
            last_hresult: s.last_hresult.load(Ordering::Relaxed),
            commands_dropped: s.queue_full.load(Ordering::Relaxed),
        }
    }
}

/// What the output backend is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendState {
    Starting,
    /// Rendering to a real device.
    Device,
    /// The null backend (no device, headless, or not Windows).
    Null,
    /// The device was lost; mixing continues in real time while it retries.
    Reconnecting,
    Stopped,
}

impl BackendState {
    pub(crate) fn as_u32(self) -> u32 {
        match self {
            BackendState::Starting => 0,
            BackendState::Device => 1,
            BackendState::Null => 2,
            BackendState::Reconnecting => 3,
            BackendState::Stopped => 4,
        }
    }

    fn from_u32(v: u32) -> BackendState {
        match v {
            1 => BackendState::Device,
            2 => BackendState::Null,
            3 => BackendState::Reconnecting,
            4 => BackendState::Stopped,
            _ => BackendState::Starting,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            BackendState::Starting => "starting",
            BackendState::Device => "device",
            BackendState::Null => "null",
            BackendState::Reconnecting => "reconnecting",
            BackendState::Stopped => "stopped",
        }
    }
}

/// A snapshot of the audio system, cheap to read every frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stats {
    pub backend: BackendState,
    pub output_rate: u32,
    pub device_channels: u32,
    pub buffer_frames: u32,
    /// Device buffer + stream latency.
    pub latency_ms: f32,
    pub active_voices: u32,
    pub peak_voices: u32,
    pub voices_stolen: u64,
    pub voices_rejected: u64,
    pub frames_mixed: u64,
    pub mix_calls: u64,
    pub mix_us_last: f32,
    pub mix_us_max: f32,
    pub mix_us_mean: f32,
    /// Peak of the last block (linear, after the limiter).
    pub master_peak: f32,
    /// Current limiter gain (1 = not limiting).
    pub limiter_gain: f32,
    pub underruns: u64,
    pub reconnects: u64,
    /// Last failing `HRESULT` the device backend saw (0 = none).
    pub last_hresult: u32,
    pub commands_dropped: u64,
}

impl Stats {
    pub fn to_json(&self) -> specula_core::Json {
        use specula_core::Json;
        Json::obj([
            ("backend", Json::from(self.backend.name())),
            ("output_rate", Json::from(self.output_rate)),
            ("device_channels", Json::from(self.device_channels)),
            ("buffer_frames", Json::from(self.buffer_frames)),
            ("latency_ms", Json::from(self.latency_ms)),
            ("active_voices", Json::from(self.active_voices)),
            ("peak_voices", Json::from(self.peak_voices)),
            ("voices_stolen", Json::from(self.voices_stolen)),
            ("voices_rejected", Json::from(self.voices_rejected)),
            ("frames_mixed", Json::from(self.frames_mixed)),
            ("mix_calls", Json::from(self.mix_calls)),
            ("mix_us_last", Json::from(self.mix_us_last)),
            ("mix_us_max", Json::from(self.mix_us_max)),
            ("mix_us_mean", Json::from(self.mix_us_mean)),
            ("master_peak", Json::from(self.master_peak)),
            ("limiter_gain", Json::from(self.limiter_gain)),
            ("underruns", Json::from(self.underruns)),
            ("reconnects", Json::from(self.reconnects)),
            ("last_hresult", Json::from(format!("0x{:08X}", self.last_hresult))),
            ("commands_dropped", Json::from(self.commands_dropped)),
        ])
    }
}

#[cfg(test)]
mod tests;
