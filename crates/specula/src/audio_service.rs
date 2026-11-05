//! The engine's audio service: one output device for the engine and its hosted game, and the
//! record of what played that the `audio_*` tools read (docs/design/AUDIO.md).
//!
//! The engine owns the device (`--audio device|null|off`; headless engines default to off, so
//! headless runs stay silent). A hosted game gets the service in [`crate::game::Game::use_audio`]
//! and plays through it, naming itself; agents read what played, how loud each bus is and the
//! streams' health from any thread. Nothing here touches the world or the simulation: sound
//! follows the game, never the other way round, so replays and `world_hash` don't depend on it.
//!
//! Every method takes `&self` and is cheap (a command pushed to the audio thread, a short lock on
//! the play log): call them from the engine thread or a caller thread alike.

use specula_audio::meter::{self, TRACKS};
use specula_audio::{Audio, AudioConfig, AudioError, Backend, Bus, DuckRule, ErrorCode, Listener, PlayParams, Sound, StreamHandle, StreamParams, StreamSource, VoiceId};
use specula_core::Json;
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

/// Plays remembered for `audio_played` (oldest dropped first, and counted).
pub const MAX_RECORDS: usize = 4096;
/// Distinct sound names interned for the log (more are logged as `other`).
const MAX_NAMES: usize = 512;
/// Streams remembered (finished ones are dropped first).
const MAX_STREAM_RECORDS: usize = 32;

/// How the engine opens audio (`--audio`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The default output device (the null backend when there is none).
    Device,
    /// Mix in real time and discard the output: meters and logs without a device.
    Null,
    /// No audio at all (the headless default).
    Off,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "device" => Some(Mode::Device),
            "null" => Some(Mode::Null),
            "off" => Some(Mode::Off),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Device => "device",
            Mode::Null => "null",
            Mode::Off => "off",
        }
    }
}

/// One play, as `audio_played` reports it.
#[derive(Clone, Debug)]
pub struct PlayRecord {
    pub seq: u64,
    /// The audio clock when it started (seconds of audio mixed).
    pub at_s: f64,
    pub voice: u64,
    pub name: Arc<str>,
    pub bus: Bus,
    pub gain: f32,
    pub position: Option<[f32; 3]>,
    pub by: Arc<str>,
    pub stream: bool,
    pub looping: bool,
    pub duration_s: Option<f64>,
}

impl PlayRecord {
    fn to_json(&self, now: f64) -> Json {
        let r3 = |v: f64| Json::from((v * 1000.0).round() / 1000.0);
        let mut j = vec![
            ("ago_s", r3(now - self.at_s)),
            ("at_s", r3(self.at_s)),
            ("name", Json::from(&*self.name)),
            ("bus", Json::from(self.bus.name())),
            ("gain", Json::from((self.gain as f64 * 1000.0).round() / 1000.0)),
            ("by", Json::from(&*self.by)),
            ("voice", Json::from(self.voice)),
            ("kind", Json::from(if self.stream { "stream" } else { "sound" })),
        ];
        if let Some(p) = self.position {
            j.push(("position", Json::arr(p.iter().map(|&v| Json::from((v as f64 * 100.0).round() / 100.0)))));
        }
        if self.looping {
            j.push(("looping", Json::Bool(true)));
        }
        if let Some(d) = self.duration_s {
            j.push(("duration_s", r3(d)));
        }
        Json::obj(j)
    }
}

/// What [`AudioService::record`] logs about one play.
struct Played<'a> {
    name: &'a str,
    by: &'a str,
    bus: Bus,
    gain: f32,
    position: Option<[f32; 3]>,
    stream: bool,
    looping: bool,
    duration_s: Option<f64>,
}

impl<'a> Played<'a> {
    fn stream(name: &'a str, by: &'a str, p: &StreamParams, total_s: Option<f64>) -> Played<'a> {
        Played { name, by, bus: p.bus, gain: p.gain, position: None, stream: true, looping: p.looping, duration_s: if p.looping { None } else { total_s } }
    }
}

#[derive(Default)]
struct Log {
    records: VecDeque<PlayRecord>,
    seq: u64,
    dropped: u64,
    names: BTreeMap<String, Arc<str>>,
}

impl Log {
    fn intern(&mut self, name: &str) -> Arc<str> {
        if let Some(n) = self.names.get(name) {
            return Arc::clone(n);
        }
        if self.names.len() >= MAX_NAMES {
            return Arc::from("other");
        }
        let n: Arc<str> = Arc::from(name);
        self.names.insert(name.to_owned(), Arc::clone(&n));
        n
    }
}

struct StreamRecord {
    voice: VoiceId,
    handle: StreamHandle,
    by: Arc<str>,
    bus: Bus,
    started_s: f64,
}

/// Bus volumes and duck rules as last set, and by whom.
struct Settings {
    gains: [(f32, Arc<str>); specula_audio::mixer::BUS_COUNT],
    ducks: [Option<(DuckRule, Arc<str>)>; specula_audio::mixer::MAX_DUCKS],
}

/// The engine's audio.
pub struct AudioService {
    audio: Option<Audio>,
    mode: Mode,
    off_reason: Option<String>,
    log: Mutex<Log>,
    streams: Mutex<Vec<StreamRecord>>,
    settings: Mutex<Settings>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl AudioService {
    /// Opens audio in `mode` (a device that can't be opened falls back to the null backend;
    /// a backend that can't start at all leaves the service off, with the reason).
    pub fn start(mode: Mode) -> Arc<AudioService> {
        let backend = match mode {
            Mode::Device => Some(Backend::Auto),
            Mode::Null => Some(Backend::Null),
            Mode::Off => None,
        };
        let (audio, off_reason) = match backend {
            None => (None, Some("audio is off (--audio off; the headless default)".to_owned())),
            Some(b) => match Audio::start(AudioConfig { backend: b, ..AudioConfig::default() }) {
                Ok(a) => (Some(a), None),
                Err(e) => {
                    specula_core::trace_event!(&specula_audio::TRACE, Warn, "audio.start_failed", mode = mode.name(), error = e.to_string());
                    (None, Some(format!("audio failed to start: {e}")))
                }
            },
        };
        Arc::new(AudioService::with(audio, mode, off_reason))
    }

    /// A service over an existing [`Audio`] (tests use the manual backend).
    pub fn with(audio: Option<Audio>, mode: Mode, off_reason: Option<String>) -> AudioService {
        let none: Arc<str> = Arc::from("");
        AudioService {
            audio,
            mode,
            off_reason,
            log: Mutex::new(Log::default()),
            streams: Mutex::new(Vec::new()),
            settings: Mutex::new(Settings { gains: std::array::from_fn(|_| (1.0, Arc::clone(&none))), ducks: [None, None, None, None] }),
        }
    }

    /// A service with no audio (`reason` says why).
    pub fn off(reason: &str) -> Arc<AudioService> {
        Arc::new(AudioService::with(None, Mode::Off, Some(reason.to_owned())))
    }

    pub fn enabled(&self) -> bool {
        self.audio.is_some()
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The underlying audio (for the manual backend's `pull` and `service` in tests).
    pub fn audio(&self) -> Option<&Audio> {
        self.audio.as_ref()
    }

    /// Seconds of audio mixed: the clock plays and meters are stamped with.
    pub fn clock_s(&self) -> f64 {
        self.audio.as_ref().map_or(0.0, Audio::clock_s)
    }

    fn record(&self, voice: VoiceId, p: Played<'_>) {
        let at_s = self.clock_s();
        let mut log = lock(&self.log);
        log.seq += 1;
        let seq = log.seq;
        let name = log.intern(p.name);
        let by = log.intern(p.by);
        if log.records.len() >= MAX_RECORDS {
            log.records.pop_front();
            log.dropped += 1;
        }
        log.records.push_back(PlayRecord { seq, at_s, voice: voice.0, name, bus: p.bus, gain: p.gain, position: p.position, by, stream: p.stream, looping: p.looping, duration_s: p.duration_s });
    }

    /// Plays `sound`, recorded as played `by` (a game's or an agent's name).
    pub fn play(&self, by: &str, sound: &Sound, params: PlayParams) -> Option<VoiceId> {
        let a = self.audio.as_ref()?;
        let id = a.play(sound, params).ok()?;
        let duration = (!params.looping).then(|| sound.duration_s() / params.pitch.clamp(1.0 / 16.0, 16.0) as f64);
        self.record(id, Played { name: sound.name(), by, bus: params.bus, gain: params.gain, position: params.spatial.map(|s| s.position), stream: false, looping: params.looping, duration_s: duration });
        Some(id)
    }

    /// Streams `source` (decoded on the audio worker), recorded as played `by`.
    pub fn play_stream(&self, by: &str, source: Box<dyn StreamSource>, params: StreamParams) -> Result<(VoiceId, StreamHandle), AudioError> {
        let a = self.audio.as_ref().ok_or_else(|| self.off_error())?;
        let name = source.name().to_owned();
        let total = source.total_frames().map(|f| f as f64 / source.rate().max(1) as f64);
        let (id, h) = a.play_stream(source, params)?;
        self.remember_stream(id, &h, by, params.bus);
        self.record(id, Played::stream(&name, by, &params, total));
        Ok((id, h))
    }

    /// Fades `from` out while `source` fades in over `seconds` (see [`Audio::crossfade`]).
    pub fn crossfade(&self, by: &str, from: Option<VoiceId>, source: Box<dyn StreamSource>, params: StreamParams, seconds: f32) -> Result<(VoiceId, StreamHandle), AudioError> {
        let a = self.audio.as_ref().ok_or_else(|| self.off_error())?;
        let name = source.name().to_owned();
        let total = source.total_frames().map(|f| f as f64 / source.rate().max(1) as f64);
        let (id, h) = a.crossfade(from, source, params, seconds)?;
        self.remember_stream(id, &h, by, params.bus);
        self.record(id, Played::stream(&name, by, &params, total));
        Ok((id, h))
    }

    /// The structured error of a call made while audio is off.
    fn off_error(&self) -> AudioError {
        AudioError::new(ErrorCode::NoDevice, self.off_reason.clone().unwrap_or_else(|| "audio is off".into()))
    }

    fn remember_stream(&self, voice: VoiceId, handle: &StreamHandle, by: &str, bus: Bus) {
        let started_s = self.clock_s();
        let mut s = lock(&self.streams);
        if s.len() >= MAX_STREAM_RECORDS {
            // Forget finished streams first, then the oldest.
            match s.iter().position(|r| r.handle.status().finished) {
                Some(i) => {
                    s.remove(i);
                }
                None => {
                    s.remove(0);
                }
            }
        }
        s.push(StreamRecord { voice, handle: handle.clone(), by: Arc::from(by), bus, started_s });
    }

    pub fn stop(&self, id: VoiceId, fade_s: f32) {
        if let Some(a) = &self.audio {
            let _ = a.stop(id, fade_s);
        }
    }

    pub fn stop_all(&self, bus: Option<Bus>, fade_s: f32) {
        if let Some(a) = &self.audio {
            let _ = a.stop_all(bus, fade_s);
        }
    }

    pub fn set_gain(&self, id: VoiceId, gain: f32, fade_s: f32) {
        if let Some(a) = &self.audio {
            let _ = a.set_gain(id, gain, fade_s);
        }
    }

    pub fn set_position(&self, id: VoiceId, position: [f32; 3]) {
        if let Some(a) = &self.audio {
            let _ = a.set_position(id, position);
        }
    }

    pub fn set_listener(&self, listener: Listener) {
        if let Some(a) = &self.audio {
            let _ = a.set_listener(listener);
        }
    }

    /// Sets a bus volume (recorded with who set it).
    pub fn set_bus_gain(&self, by: &str, bus: Bus, gain: f32, fade_s: f32) -> Result<(), AudioError> {
        self.apply_settings(by, &[(bus, gain)], &[], fade_s)
    }

    /// Installs or clears duck rule `slot` (recorded with who set it).
    pub fn set_duck(&self, by: &str, slot: usize, rule: Option<DuckRule>) -> Result<(), AudioError> {
        self.apply_settings(by, &[], &[(slot, rule)], 0.0)
    }

    /// Bus gains and duck rules in one change that applies whole or not at all (rules validated
    /// and room for every command checked first), recorded with who made it only once applied.
    pub fn apply_settings(&self, by: &str, gains: &[(Bus, f32)], ducks: &[(usize, Option<DuckRule>)], fade_s: f32) -> Result<(), AudioError> {
        let gains: Vec<(Bus, f32)> = gains.iter().map(|&(b, g)| (b, if g.is_finite() { g.clamp(0.0, 4.0) } else { 1.0 })).collect();
        match &self.audio {
            Some(a) => a.apply_settings(&gains, ducks, fade_s)?,
            None => {
                for &(slot, rule) in ducks {
                    specula_audio::mixer::validate_duck(slot, rule.as_ref())?;
                }
            }
        }
        let mut s = lock(&self.settings);
        for &(bus, g) in &gains {
            s.gains[bus as usize] = (g, Arc::from(by));
        }
        for &(slot, rule) in ducks {
            s.ducks[slot] = rule.map(|r| (r, Arc::from(by)));
        }
        Ok(())
    }

    /// The bus a gain and duck rules refer to by name.
    pub fn bus(name: &str) -> Option<Bus> {
        Bus::from_name(name)
    }

    /// `audio_played`: plays in the last `seconds`, newest first, filtered, at most `limit`.
    pub fn played_json(&self, seconds: f64, bus: Option<Bus>, by: Option<&str>, prefix: Option<&str>, limit: usize) -> Json {
        let now = self.clock_s();
        let log = lock(&self.log);
        let since = now - seconds;
        let mut matched = 0usize;
        let mut shown = Vec::new();
        for r in log.records.iter().rev() {
            if r.at_s < since {
                break;
            }
            if bus.is_some_and(|b| b != r.bus) || by.is_some_and(|b| &*r.by != b) || prefix.is_some_and(|p| !r.name.starts_with(p)) {
                continue;
            }
            matched += 1;
            if shown.len() < limit {
                shown.push(r.to_json(now));
            }
        }
        // Per-name counts of everything matched, so a long window still says what played.
        let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
        for r in log.records.iter().rev().take_while(|r| r.at_s >= since) {
            if bus.is_some_and(|b| b != r.bus) || by.is_some_and(|b| &*r.by != b) || prefix.is_some_and(|p| !r.name.starts_with(p)) {
                continue;
            }
            *counts.entry(&r.name).or_default() += 1;
        }
        let mut out = vec![
            ("clock_s", Json::from((now * 1000.0).round() / 1000.0)),
            ("window_s", Json::from(seconds)),
            ("matched", Json::from(matched)),
            ("shown", Json::from(shown.len())),
            ("plays", Json::Arr(shown)),
            ("by_name", Json::obj(counts.into_iter().take(64).map(|(k, v)| (k.to_owned(), Json::from(v))))),
        ];
        if matched > limit {
            out.push(("truncated", Json::Bool(true)));
        }
        if log.dropped > 0 {
            out.push(("log_dropped", Json::from(log.dropped)));
        }
        if self.audio.is_none() {
            out.push(("off", Json::from(self.off_reason.as_deref().unwrap_or("off"))));
        }
        Json::obj(out)
    }

    /// The meter's report on `track` (a bus index, or [`TRACKS`]` - 1` for the master output).
    pub fn report(&self, track: usize, window_s: f64, spectrum: bool) -> Option<meter::Report> {
        Some(self.audio.as_ref()?.meter_report(track, window_s, spectrum))
    }

    /// `audio_levels`: per-bus loudness and peaks over `window_s`, the streams, and the device.
    pub fn levels_json(&self, window_s: f64, tracks: &[usize], spectrum: bool) -> Json {
        let Some(a) = &self.audio else {
            return Json::obj([("enabled", Json::Bool(false)), ("off", Json::from(self.off_reason.as_deref().unwrap_or("off")))]);
        };
        let stats = a.stats();
        let mut buses = Vec::new();
        for &t in tracks {
            let r = a.meter_report(t, window_s, spectrum);
            buses.push((meter::Meter::track_name(t).to_owned(), report_json(&r)));
        }
        let streams: Vec<Json> = lock(&self.streams).iter().rev().map(|s| self.stream_json(s)).collect();
        Json::obj([
            ("enabled", Json::Bool(true)),
            ("mode", Json::from(self.mode.name())),
            ("clock_s", Json::from((self.clock_s() * 1000.0).round() / 1000.0)),
            ("window_s", Json::from(window_s)),
            ("buses", Json::Obj(buses)),
            ("streams", Json::Arr(streams)),
            (
                "device",
                Json::obj([
                    ("backend", Json::from(stats.backend.name())),
                    ("rate", Json::from(stats.output_rate)),
                    ("latency_ms", Json::from((stats.latency_ms as f64 * 10.0).round() / 10.0)),
                    ("voices", Json::from(stats.active_voices)),
                    ("streams", Json::from(stats.active_streams)),
                    ("underruns", Json::from(stats.underruns)),
                    ("mix_us_mean", Json::from((stats.mix_us_mean as f64 * 10.0).round() / 10.0)),
                    // Share of one core the mixer and the worker used since audio started.
                    ("mix_cpu_percent", Json::from(cpu_percent(stats.mix_ms_total, self.clock_s()))),
                    ("worker_cpu_percent", Json::from(cpu_percent(stats.worker_ms_total, self.clock_s()))),
                ]),
            ),
            (
                "totals",
                Json::obj([
                    ("clipped_samples", Json::from(stats.clipped_samples)),
                    ("limited_blocks", Json::from(stats.limited_blocks)),
                    ("stream_starved_frames", Json::from(stats.stream_starved_frames)),
                    ("stream_starve_events", Json::from(stats.stream_starve_events)),
                    ("non_finite_samples", Json::from(stats.non_finite_samples)),
                    ("voices_stolen", Json::from(stats.voices_stolen)),
                    ("meter_dropped_blocks", Json::from(stats.tap_dropped)),
                ]),
            ),
        ])
    }

    fn stream_json(&self, s: &StreamRecord) -> Json {
        let st = s.handle.status();
        let rate = self.audio.as_ref().map_or(48_000, |a| a.stats().output_rate).max(1) as f64;
        let mut j = vec![
            ("voice", Json::from(s.voice.0)),
            ("name", Json::from(st.name.as_str())),
            ("by", Json::from(&*s.by)),
            ("bus", Json::from(s.bus.name())),
            ("started_s", Json::from((s.started_s * 1000.0).round() / 1000.0)),
            ("played_s", Json::from((st.played_frames as f64 / rate * 1000.0).round() / 1000.0)),
            ("starved_frames", Json::from(st.starved_frames)),
            ("starve_events", Json::from(st.starve_events)),
            ("loops", Json::from(st.loops)),
            ("buffered_blocks", Json::from(st.buffered_blocks)),
            ("state", Json::from(if st.finished { "finished" } else if st.started { "playing" } else { "buffering" })),
        ];
        if let Some(e) = &st.error {
            j.push(("error", e.to_json()));
        }
        Json::obj(j)
    }

    /// `audio_settings`: bus volumes and duck rules, and who set them.
    pub fn settings_json(&self) -> Json {
        let s = lock(&self.settings);
        let buses = Bus::ALL.iter().map(|b| {
            let (g, by) = &s.gains[*b as usize];
            let mut j = vec![("gain", Json::from((*g as f64 * 1000.0).round() / 1000.0))];
            if !by.is_empty() {
                j.push(("set_by", Json::from(&**by)));
            }
            (b.name().to_owned(), Json::obj(j))
        });
        let ducks = s.ducks.iter().enumerate().filter_map(|(i, d)| {
            let (r, by) = d.as_ref()?;
            Some(Json::obj([
                ("slot", Json::from(i)),
                ("source", Json::from(r.source.name())),
                ("target", Json::from(r.target.name())),
                ("depth_db", Json::from(r.depth_db)),
                ("threshold_db", Json::from(r.threshold_db)),
                ("attack_s", Json::from(r.attack_s)),
                ("release_s", Json::from(r.release_s)),
                ("set_by", Json::from(&**by)),
            ]))
        });
        Json::obj([("enabled", Json::Bool(self.enabled())), ("buses", Json::Obj(buses.collect())), ("ducks", Json::Arr(ducks.collect()))])
    }

    /// A short summary for `engine_info`.
    pub fn summary_json(&self) -> Json {
        match &self.audio {
            None => Json::obj([("mode", Json::from(self.mode.name())), ("off", Json::from(self.off_reason.as_deref().unwrap_or("off")))]),
            Some(a) => {
                let s = a.stats();
                Json::obj([
                    ("mode", Json::from(self.mode.name())),
                    ("backend", Json::from(s.backend.name())),
                    ("voices", Json::from(s.active_voices)),
                    ("streams", Json::from(s.active_streams)),
                    ("plays", Json::from(lock(&self.log).seq)),
                ])
            }
        }
    }
}

/// `ms` of CPU over `clock_s` seconds, as a percent of one core (2 decimals).
fn cpu_percent(ms: f64, clock_s: f64) -> f64 {
    if clock_s <= 0.0 {
        return 0.0;
    }
    (ms / (clock_s * 1000.0) * 100.0 * 100.0).round() / 100.0
}

/// Numbers rounded for agents; silence is `null` (JSON has no -inf).
pub fn report_json(r: &meter::Report) -> Json {
    let f = |v: f64| if v.is_finite() { Json::from((v * 10.0).round() / 10.0) } else { Json::Null };
    let mut j = vec![
        ("momentary_lufs", f(r.momentary_lufs)),
        ("short_term_lufs", f(r.short_term_lufs)),
        ("integrated_lufs", f(r.integrated_lufs)),
        ("peak_dbfs", f(r.peak_dbfs)),
        ("true_peak_dbtp", f(r.true_peak_dbtp)),
        ("overs", Json::from(r.overs)),
        ("audible_s", Json::from((r.audible_s * 10.0).round() / 10.0)),
        ("silent_for_s", r.silent_for_s.map_or(Json::Null, |s| Json::from((s * 10.0).round() / 10.0))),
    ];
    if r.silent_for_s.is_none() {
        j.push(("never_audible", Json::Bool(true)));
    }
    if let Some(bands) = &r.spectrum {
        j.push(("spectrum_db", Json::Obj(bands.iter().map(|(hz, db)| (format!("{hz}"), f(*db))).collect())));
    }
    Json::obj(j)
}

/// Meter tracks by name: the buses and `master_out` (every one when `names` is empty).
pub fn tracks(names: &[String]) -> Result<Vec<usize>, String> {
    if names.is_empty() {
        return Ok((0..TRACKS).collect());
    }
    let mut out = Vec::new();
    for n in names {
        let t = (0..TRACKS).find(|&t| meter::Meter::track_name(t) == n).ok_or_else(|| {
            let all: Vec<&str> = (0..TRACKS).map(meter::Meter::track_name).collect();
            format!("unknown bus '{}' (one of: {})", n.chars().take(32).collect::<String>(), all.join(", "))
        })?;
        if out.contains(&t) {
            return Err(format!("bus '{n}' is listed twice"));
        }
        out.push(t);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula_audio::stream::SoundSource;

    fn manual() -> AudioService {
        let a = Audio::start(AudioConfig { backend: Backend::Manual, ..AudioConfig::default() }).unwrap();
        AudioService::with(Some(a), Mode::Null, None)
    }

    fn run(s: &AudioService, secs: f64) {
        let a = s.audio().unwrap();
        let mut buf = vec![0f32; 480 * 2];
        for _ in 0..(secs * 100.0) as usize {
            a.pull(&mut buf).unwrap();
            a.service();
        }
    }

    fn tone(name: &str, amp: f32, secs: f32) -> Sound {
        let n = (48_000.0 * secs) as usize;
        Sound::new(name, 48_000, 1, (0..n).map(|i| amp * (i as f32 * 0.05).sin()).collect()).unwrap()
    }

    #[test]
    fn plays_are_logged_with_who_and_filtered() {
        let s = manual();
        s.play("game", &tone("zap", 0.3, 0.2), PlayParams { bus: Bus::Sfx, spatial: Some(specula_audio::Spatial::at([1.0, 2.0, 3.0])), ..PlayParams::default() }).unwrap();
        run(&s, 0.5);
        s.play("agent-a", &tone("click", 0.3, 0.1), PlayParams { bus: Bus::Ui, ..PlayParams::default() }).unwrap();
        s.play_stream("game", Box::new(SoundSource::new(tone("theme", 0.2, 5.0))), StreamParams::default()).unwrap();
        run(&s, 1.0);
        let all = s.played_json(10.0, None, None, None, 50);
        assert_eq!(all.get("matched").and_then(Json::as_u64), Some(3));
        let plays = all.get("plays").and_then(Json::as_array).unwrap();
        // Newest first; the stream is recorded as a stream.
        assert_eq!(plays[0].get("kind").and_then(Json::as_str), Some("stream"));
        let zap = plays.iter().find(|p| p.get("name").and_then(Json::as_str) == Some("zap")).unwrap();
        assert_eq!(zap.get("by").and_then(Json::as_str), Some("game"));
        assert!(zap.get("position").is_some());
        assert!(zap.get("ago_s").and_then(Json::as_f64).unwrap() >= 1.0);
        let ui = s.played_json(10.0, Some(Bus::Ui), None, None, 50);
        assert_eq!(ui.get("matched").and_then(Json::as_u64), Some(1));
        let by = s.played_json(10.0, None, Some("game"), Some("th"), 50);
        assert_eq!(by.get("matched").and_then(Json::as_u64), Some(1));
        // The window excludes older plays; limit truncates but counts all.
        assert_eq!(s.played_json(1.2, None, None, None, 50).get("matched").and_then(Json::as_u64), Some(2));
        let one = s.played_json(10.0, None, None, None, 1);
        assert_eq!(one.get("truncated"), Some(&Json::Bool(true)));
        assert_eq!(one.get("by_name").and_then(|b| b.get("zap")).and_then(Json::as_u64), Some(1));
    }

    #[test]
    fn levels_settings_and_off() {
        let s = manual();
        s.play_stream("game", Box::new(SoundSource::new(tone("theme", 0.3, 5.0))), StreamParams::default()).unwrap();
        run(&s, 2.0);
        let j = s.levels_json(2.0, &[Bus::Music as usize, Bus::Sfx as usize, TRACKS - 1], true);
        let music = j.get("buses").and_then(|b| b.get("music")).unwrap();
        assert!(music.get("integrated_lufs").and_then(Json::as_f64).is_some());
        assert!(music.get("spectrum_db").is_some());
        assert_eq!(j.get("buses").and_then(|b| b.get("sfx")).and_then(|b| b.get("never_audible")), Some(&Json::Bool(true)));
        assert_eq!(j.get("buses").and_then(|b| b.get("sfx")).and_then(|b| b.get("integrated_lufs")), Some(&Json::Null));
        let streams = j.get("streams").and_then(Json::as_array).unwrap();
        assert_eq!(streams[0].get("state").and_then(Json::as_str), Some("playing"));
        assert_eq!(streams[0].get("starved_frames").and_then(Json::as_u64), Some(0));
        s.set_bus_gain("lead-x", Bus::Music, 0.5, 0.0).unwrap();
        s.set_duck("game", 1, Some(DuckRule { source: Bus::Voice, target: Bus::Music, depth_db: 10.0, threshold_db: -40.0, attack_s: 0.05, release_s: 0.5 })).unwrap();
        assert!(s.set_duck("game", 9, None).is_err());
        let set = s.settings_json();
        assert_eq!(set.get("buses").and_then(|b| b.get("music")).and_then(|m| m.get("set_by")).and_then(Json::as_str), Some("lead-x"));
        assert_eq!(set.get("ducks").and_then(Json::as_array).map(|d| d.len()), Some(1));
        assert_eq!(tracks(&["music".into(), "master_out".into()]).unwrap(), vec![Bus::Music as usize, TRACKS - 1]);
        assert!(tracks(&["music".into(), "music".into()]).is_err());
        assert!(tracks(&["nope".into()]).is_err());
        let off = AudioService::off("headless");
        assert!(!off.enabled());
        assert!(off.play("game", &tone("x", 0.1, 0.1), PlayParams::default()).is_none());
        assert_eq!(off.levels_json(1.0, &[0], false).get("enabled"), Some(&Json::Bool(false)));
        assert!(off.played_json(1.0, None, None, None, 5).get("off").is_some());
    }

    #[test]
    fn mode_names_round_trip() {
        for m in [Mode::Device, Mode::Null, Mode::Off] {
            assert_eq!(Mode::parse(m.name()), Some(m));
        }
        assert_eq!(Mode::parse("loud"), None);
    }
}
