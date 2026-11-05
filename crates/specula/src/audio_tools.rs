//! `audio_*`: hearing for agents (docs/design/AUDIO.md). Agents can't listen, so the engine says
//! what played, how loud each bus is, whether streams starved, and what a sound or piece would
//! measure before anyone plays it. Every tool answers on the caller's connection thread (the
//! service and its meter are shared); none touches the world or the engine thread, except a
//! project file snapshot for `audio_preview`.

use crate::audio_service::{self, AudioService};
use crate::authority::{self, Authority, Site};
use crate::project::FileKind;
use crate::tools::{bool_arg, check_keys, int_arg, string_arg, Tools};
use specula_audio::music::{self, Music, MusicSource};
use specula_audio::{analysis, meter, synth, vorbis, wav, Bus, DuckRule, RenderOptions, StreamSource};
use specula_core::Json;
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// The longest window `audio_played` and `audio_levels` cover (the meter keeps ten minutes).
pub const MAX_WINDOW_S: f64 = 600.0;
/// Most plays one `audio_played` answer lists.
pub const MAX_PLAYS: u64 = 200;
/// Seconds of audio a preview renders or decodes at most (at rates up to 48 kHz).
pub const MAX_PREVIEW_S: f64 = 120.0;
/// Rates above this count as this much (a 384 kHz file previews an eighth as long).
const PREVIEW_RATE: u32 = 48_000;
/// Samples a preview keeps at most, whatever the file: 120 s of stereo at 48 kHz. Everything is
/// folded to stereo as it is decoded, so neither the channel count nor the rate a file claims
/// can make a preview bigger or slower (review round 2: 8 channels at 384 kHz took 2.2 GB).
pub const MAX_PREVIEW_SAMPLES: usize = MAX_PREVIEW_S as usize * PREVIEW_RATE as usize * 2;

/// Frames a preview keeps of a sound at `rate`: at most `max_s` of it, and never more than
/// [`MAX_PREVIEW_SAMPLES`] stereo samples.
fn preview_frames(max_s: f64, rate: u32) -> usize {
    ((max_s * rate.min(PREVIEW_RATE) as f64) as usize).min(MAX_PREVIEW_SAMPLES / 2)
}
/// Previews running at once (each renders or decodes on its caller's thread).
const MAX_PREVIEWS: usize = 2;
static PREVIEWS: AtomicUsize = AtomicUsize::new(0);
/// Preview files written this run (numbering).
static PREVIEW_SEQ: AtomicUsize = AtomicUsize::new(0);

fn error(code: &str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_string())
}

fn detailed(args: &Json) -> Result<bool, String> {
    Ok(crate::response::Format::arg(args, crate::response::Format::Concise)? == crate::response::Format::Detailed)
}

/// Answers an `audio_*` call.
pub fn call(tools: &Tools, session: &SessionInfo, name: &str, args: &Json) -> ToolOutput {
    let Some(audio) = tools.audio.clone() else { return error("audio_off", "this engine has no audio service") };
    let out = match name {
        "audio_played" => played(&audio, args),
        "audio_levels" => levels(&audio, args),
        "audio_settings" => settings(&audio, session, args),
        "audio_preview" => preview(tools, session, args),
        other => Err(format!("unknown audio tool '{other}'")),
    };
    out.unwrap_or_else(|m| ToolOutput::error(crate::tools::argument_error(&m).to_string()))
}

fn seconds(args: &Json, key: &str, default: f64) -> Result<f64, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(default),
        Some(v) => v.as_f64().filter(|s| (0.1..=MAX_WINDOW_S).contains(s)).ok_or(format!("'{key}' must be seconds, 0.1..{MAX_WINDOW_S}")),
    }
}

fn bus_arg(args: &Json, key: &str) -> Result<Option<Bus>, String> {
    match string_arg(args, key)? {
        None => Ok(None),
        Some(b) => AudioService::bus(b).map(Some).ok_or(format!("'{key}' must be a bus: master, sfx, ui, ambience, music or voice (not '{}')", crate::bounds::echo(b))),
    }
}

fn played(audio: &AudioService, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "audio_played", &[&["seconds", "bus", "by", "name_prefix", "limit", crate::response::KEY]])?;
    let secs = seconds(args, "seconds", 10.0)?;
    let bus = bus_arg(args, "bus")?;
    let by = string_arg(args, "by")?;
    let prefix = string_arg(args, "name_prefix")?;
    let limit = int_arg(args, "limit", 1, MAX_PLAYS)?.unwrap_or(50) as usize;
    let mut j = audio.played_json(secs, bus, by, prefix, limit);
    if !detailed(args)? {
        // Concise: what, where and when; the voice, gain and position stay in the detailed form.
        if let Some(Json::Arr(plays)) = j.get("plays").cloned() {
            let keep = ["ago_s", "name", "bus", "by", "kind"];
            let slim = plays.into_iter().map(|p| match p {
                Json::Obj(pairs) => Json::Obj(pairs.into_iter().filter(|(k, _)| keep.contains(&k.as_str())).collect()),
                other => other,
            });
            j.set("plays", Json::Arr(slim.collect()));
        }
    }
    Ok(ToolOutput::text(j.to_string()))
}

fn levels(audio: &AudioService, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "audio_levels", &[&["window_s", "buses", "spectrum", crate::response::KEY]])?;
    let window = seconds(args, "window_s", 10.0)?;
    let names: Vec<String> = match args.get("buses") {
        None | Some(Json::Null) => Vec::new(),
        Some(Json::Arr(a)) if a.len() <= meter::TRACKS => a.iter().map(|v| v.as_str().map(str::to_owned).ok_or("'buses' must be bus names")).collect::<Result<_, _>>()?,
        Some(_) => return Err(format!("'buses' must be an array of at most {} bus names", meter::TRACKS)),
    };
    let tracks = audio_service::tracks(&names)?;
    let spectrum = bool_arg(args, "spectrum")?.unwrap_or(false);
    let mut j = audio.levels_json(window, &tracks, spectrum);
    if !detailed(args)? {
        if let Json::Obj(pairs) = &mut j {
            pairs.retain(|(k, _)| k != "device");
        }
    }
    Ok(ToolOutput::text(j.to_string()))
}

fn settings(audio: &AudioService, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "audio_settings", &[&["buses", "ducks", "fade_s"]])?;
    let fade = match args.get("fade_s") {
        None | Some(Json::Null) => 0.1,
        Some(v) => v.as_f64().filter(|s| (0.0..=10.0).contains(s)).ok_or("'fade_s' must be seconds, 0..10")? as f32,
    };
    // Parse everything first: a call either applies whole or changes nothing.
    let mut gains = Vec::new();
    match args.get("buses") {
        None | Some(Json::Null) => {}
        Some(Json::Obj(pairs)) if pairs.len() <= Bus::ALL.len() => {
            for (k, v) in pairs {
                let bus = AudioService::bus(k).ok_or(format!("'buses' has '{}', not a bus (master, sfx, ui, ambience, music, voice)", crate::bounds::echo(k)))?;
                if gains.iter().any(|(b, _)| *b == bus) {
                    return Err(format!("'buses' names '{k}' twice"));
                }
                let g = v.as_f64().filter(|g| (0.0..=4.0).contains(g)).ok_or(format!("'buses.{k}' must be a linear gain, 0..4 (1 = unchanged)"))?;
                gains.push((bus, g as f32));
            }
        }
        Some(_) => return Err("'buses' must be an object of bus name to gain (at most 6)".into()),
    }
    let mut ducks = Vec::new();
    match args.get("ducks") {
        None | Some(Json::Null) => {}
        Some(Json::Arr(items)) if items.len() <= specula_audio::mixer::MAX_DUCKS => {
            for (i, d) in items.iter().enumerate() {
                let f = |k: &str| d.get(k);
                check_keys(d, &format!("'ducks[{i}]'"), &[&["slot", "clear", "source", "target", "depth_db", "threshold_db", "attack_s", "release_s"]])?;
                let slot = f("slot").and_then(Json::as_u64).filter(|s| (*s as usize) < specula_audio::mixer::MAX_DUCKS).ok_or(format!("'ducks[{i}].slot' must be 0..3"))? as usize;
                if ducks.iter().any(|(s, _)| *s == slot) {
                    return Err(format!("'ducks' sets slot {slot} twice"));
                }
                if f("clear").and_then(Json::as_bool) == Some(true) {
                    ducks.push((slot, None));
                    continue;
                }
                let bus = |k: &str| f(k).and_then(Json::as_str).and_then(AudioService::bus).ok_or(format!("'ducks[{i}].{k}' must be a bus"));
                let num = |k: &str, lo: f64, hi: f64, def: Option<f64>| -> Result<f32, String> {
                    match f(k) {
                        None if def.is_some() => Ok(def.unwrap_or(0.0) as f32),
                        v => v.and_then(Json::as_f64).filter(|x| (lo..=hi).contains(x)).map(|x| x as f32).ok_or(format!("'ducks[{i}].{k}' must be {lo}..{hi}")),
                    }
                };
                let rule = DuckRule {
                    source: bus("source")?,
                    target: bus("target")?,
                    depth_db: num("depth_db", 0.0, 60.0, None)?,
                    threshold_db: num("threshold_db", -90.0, 0.0, Some(-40.0))?,
                    attack_s: num("attack_s", 0.001, 5.0, Some(0.05))?,
                    release_s: num("release_s", 0.001, 10.0, Some(0.4))?,
                };
                if rule.target == Bus::Master || rule.source == rule.target {
                    return Err(format!("'ducks[{i}]': a rule ducks one bus under another (not the master, not itself)"));
                }
                ducks.push((slot, Some(rule)));
            }
        }
        Some(_) => return Err("'ducks' must be an array of at most 4 rules".into()),
    }
    if !gains.is_empty() || !ducks.is_empty() {
        // Bus volumes and ducking are global: they change what every agent and the human hear.
        if let Err(e) = authority::require(authority::of_session(&session.id), Authority::Lead, Site::AudioSettings, "changing audio_settings (bus volumes and duck rules are global; anyone may read them)") {
            return Ok(crate::world_error(&e));
        }
        let by = crate::trace_sink::name_of(&session.id).map_or_else(|| session.client_name.clone(), |(a, _)| a);
        // Whole or not at all: the service validates every rule and checks the queue first.
        if let Err(e) = audio.apply_settings(&by, &gains, &ducks, fade) {
            return Ok(ToolOutput::error(e.to_json().to_string()));
        }
        specula_core::trace_event!(&specula_audio::TRACE, Info, "audio.settings", by = by.as_str());
    }
    Ok(ToolOutput::text(audio.settings_json().to_string()))
}

/// A preview's samples and what it was.
struct Rendered {
    name: String,
    kind: &'static str,
    rate: u32,
    channels: usize,
    samples: Vec<f32>,
    /// The whole thing is longer than the preview.
    cut: bool,
    extra: Vec<(&'static str, Json)>,
}

struct Slot;

impl Drop for Slot {
    fn drop(&mut self) {
        PREVIEWS.fetch_sub(1, Ordering::AcqRel);
    }
}

fn preview(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "audio_preview", &[&["path", "recipe", "music", "variation", "seconds", "wav", "png"]])?;
    let sources = ["path", "recipe", "music"].into_iter().filter(|k| args.get(k).is_some_and(|v| !v.is_null())).count();
    if sources != 1 {
        return Err("give exactly one of 'path' (a project file: sounds/*.json, music/*.json, .ogg, .wav), 'recipe' (a sound recipe object) or 'music' (a music object)".into());
    }
    let variation = int_arg(args, "variation", 0, 64)?.unwrap_or(0) as u32;
    let max_s = match args.get("seconds") {
        None | Some(Json::Null) => MAX_PREVIEW_S,
        Some(v) => v.as_f64().filter(|s| (0.1..=MAX_PREVIEW_S).contains(s)).ok_or(format!("'seconds' must be 0.1..{MAX_PREVIEW_S}"))?,
    };
    if PREVIEWS.fetch_add(1, Ordering::AcqRel) >= MAX_PREVIEWS {
        PREVIEWS.fetch_sub(1, Ordering::AcqRel);
        return Ok(ToolOutput::error(
            Json::obj([("error", Json::from("busy")), ("message", Json::from(format!("{MAX_PREVIEWS} previews are rendering already; try again shortly"))), ("retry", Json::Bool(true))]).to_string(),
        ));
    }
    let _slot = Slot;
    let started = std::time::Instant::now();
    let r = if let Some(recipe) = args.get("recipe").filter(|v| !v.is_null()) {
        render_sound(&synth::Recipe::from_json(recipe).map_err(|e| format!("'recipe': {e}"))?, variation, "inline")?
    } else if let Some(m) = args.get("music").filter(|v| !v.is_null()) {
        render_music(&Music::from_json(m).map_err(|e| format!("'music': {e}"))?, max_s)?
    } else {
        let path = crate::project_tools::path_arg(args)?;
        if path.chars().count() > 512 {
            return Err("'path' is at most 512 characters".into());
        }
        let snap = match crate::project_tools::snapshot(tools, session, &path, None, false) {
            Ok(s) => s,
            Err(out) => return Ok(out),
        };
        let Some(file) = snap.current else { return Ok(error("not_found", format!("there is no project file {path}"))) };
        match file.kind {
            FileKind::Sound => render_sound(&synth::Recipe::parse(&file.text).map_err(|e| format!("'path' {path}: {e}"))?, variation, "sound")?,
            FileKind::Music => render_music(&Music::parse(&file.text).map_err(|e| format!("'path' {path}: {e}"))?, max_s)?,
            FileKind::Ogg | FileKind::Wav => {
                let Some((id, _)) = crate::blobs::parse_pointer(&file.text) else { return Ok(error("not_a_pointer", format!("{path}'s entry doesn't name its content"))) };
                let Some(store) = &tools.content else { return Ok(error("no_content_store", "this engine has no content store")) };
                let bytes = match store.get(id) {
                    Ok(b) => b,
                    Err(e) => return Ok(ToolOutput::error(e.to_json().to_string())),
                };
                if file.kind == FileKind::Ogg {
                    decode_ogg(&path, bytes, max_s)?
                } else {
                    decode_wav(&path, &bytes, max_s)?
                }
            }
            other => return Err(format!("'path' {path} is a {} file: audio_preview takes sounds/*.json recipes, music/*.json pieces, .ogg and .wav files", other.name())),
        }
    };
    let mut r = r;
    let frames = r.samples.len() / r.channels.max(1);
    let max_frames = preview_frames(max_s, r.rate);
    if frames > max_frames {
        r.samples.truncate(max_frames * r.channels);
        r.cut = true;
    }
    debug_assert!(r.samples.len() <= MAX_PREVIEW_SAMPLES && r.channels <= 2);
    let ch = r.channels;
    let m = analysis::analyze(&r.samples, ch, r.rate);
    let mut metrics = m.to_json();
    let tp = meter::true_peak(&r.samples, ch);
    metrics.set("true_peak_dbtp", if tp > 0.0 { Json::from((20.0 * (tp as f64).log10() * 10.0).round() / 10.0) } else { Json::Null });
    let bands: Vec<(String, Json)> = meter::bands(&r.samples, ch, r.rate)
        .into_iter()
        .map(|(hz, db)| (format!("{hz}"), if db.is_finite() { Json::from((db * 10.0).round() / 10.0) } else { Json::Null }))
        .collect();
    let mut out = vec![
        ("name", Json::from(r.name.as_str())),
        ("kind", Json::from(r.kind)),
        ("rate", Json::from(r.rate)),
        ("channels", Json::from(ch)),
        ("seconds", Json::from(((r.samples.len() / ch.max(1)) as f64 / r.rate as f64 * 1000.0).round() / 1000.0)),
        ("metrics", metrics),
        ("spectrum_db", Json::Obj(bands)),
    ];
    if r.cut {
        let kept = r.samples.len() / ch.max(1);
        out.push(("cut_at_s", Json::from((kept as f64 / r.rate as f64 * 1000.0).round() / 1000.0)));
    }
    out.append(&mut r.extra);
    let want_wav = bool_arg(args, "wav")?.unwrap_or(false);
    let want_png = bool_arg(args, "png")?.unwrap_or(false);
    if want_wav || want_png {
        let n = PREVIEW_SEQ.fetch_add(1, Ordering::Relaxed);
        let stem: String = r.name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).take(48).collect();
        let dir = tools.procgen.captures.join("audio");
        let mut files = Vec::new();
        let write = |file: &str, bytes: &[u8]| -> Result<String, ToolOutput> {
            let p = dir.join(file);
            crate::disk::book().write(crate::disk::Category::Captures, "audio", &p, bytes, crate::disk::Need::Agent).map_err(|e| ToolOutput::error(e.to_json().to_string()))?;
            Ok(p.to_string_lossy().into_owned())
        };
        if want_wav {
            let bytes = wav::encode(&r.samples, ch as u16, r.rate, wav::WavFormat::Pcm16).map_err(|e| e.to_string())?;
            match write(&format!("{stem}-{n:04}.wav"), &bytes) {
                Ok(p) => files.push(("wav", Json::from(p))),
                Err(o) => return Ok(o),
            }
        }
        if want_png {
            let panel = analysis::sound_panel(&r.name, &r.samples, ch, r.rate, 900, &m);
            match write(&format!("{stem}-{n:04}.png"), &analysis::encode_png(&panel)) {
                Ok(p) => files.push(("png", Json::from(p))),
                Err(o) => return Ok(o),
            }
        }
        out.push(("files", Json::obj(files)));
    }
    out.push(("took_ms", Json::from(started.elapsed().as_millis() as u64)));
    Ok(ToolOutput::text(Json::obj(out).to_string()))
}

fn render_sound(recipe: &synth::Recipe, variation: u32, kind: &'static str) -> Result<Rendered, String> {
    let r = synth::render(recipe, &RenderOptions { variation, ..RenderOptions::default() }).map_err(|e| e.to_string())?;
    let mut extra = vec![("bus", Json::from(r.bus.name()))];
    if r.looping {
        extra.push(("loop", Json::Bool(true)));
    }
    if let Some(t) = recipe.loudness {
        extra.push(("loudness_target", Json::from(t)));
    }
    Ok(Rendered { name: r.name.clone(), kind, rate: r.rate, channels: r.channels as usize, samples: r.samples, cut: false, extra })
}

fn render_music(m: &Music, max_s: f64) -> Result<Rendered, String> {
    let piece = Arc::new(music::render(m).map_err(|e| e.to_string())?);
    let mut src = MusicSource::new(Arc::clone(&piece), false);
    let total = src.total_frames().unwrap_or(0).min((max_s * music::RATE as f64) as u64) as usize;
    let mut samples = vec![0f32; total * 2];
    let n = src.read(&mut samples).map_err(|e| e.to_string())?;
    samples.truncate(n * 2);
    let lap_s = piece.lap_frames as f64 / music::RATE as f64;
    let mut extra = vec![
        ("bus", Json::from(piece.bus.name())),
        ("lap_s", Json::from((lap_s * 1000.0).round() / 1000.0)),
        ("loop_from_s", Json::from((piece.loop_frame as f64 / music::RATE as f64 * 1000.0).round() / 1000.0)),
        ("notes", Json::from(piece.notes())),
        ("tempo", Json::from(m.tempo)),
        ("bars", Json::from(m.bars)),
        ("cost", m.cost.to_json()),
    ];
    if let (Some(t), Some(l)) = (m.loudness, piece.lufs) {
        extra.push(("loudness_target", Json::from(t)));
        extra.push(("loop_lufs", Json::from((l * 10.0).round() / 10.0)));
    }
    Ok(Rendered { name: piece.name.clone(), kind: "music", rate: music::RATE, channels: 2, samples, cut: lap_s > max_s, extra })
}

fn decode_ogg(path: &str, bytes: Vec<u8>, max_s: f64) -> Result<Rendered, String> {
    let mut d = vorbis::Decoder::new(bytes).map_err(|e| format!("'path' {path}: {e}"))?;
    let info = d.info().clone();
    let ch = d.channels();
    let out_ch = ch.min(2);
    let max_frames = preview_frames(max_s, info.rate);
    let mut samples = Vec::with_capacity(info.total_frames.map_or(0, |t| (t as usize).min(max_frames)) * out_ch);
    // Decoded a chunk at a time and folded to stereo at once: only stereo is ever held.
    let mut buf = vec![0f32; 4096 * ch];
    let mut folded = Vec::new();
    let mut ended = false;
    while samples.len() / out_ch < max_frames {
        let n = d.read(&mut buf).map_err(|e| format!("'path' {path}: {e}"))?;
        if n == 0 {
            ended = true;
            break;
        }
        let take = n.min(max_frames - samples.len() / out_ch);
        if ch > 2 {
            vorbis::downmix_stereo(&buf[..take * ch], ch, &mut folded);
            samples.extend_from_slice(&folded);
        } else {
            samples.extend_from_slice(&buf[..take * ch]);
        }
    }
    let mut extra = vec![
        ("vendor", Json::from(crate::bounds::echo(&info.vendor))),
        ("source_channels", Json::from(ch)),
        ("bitrate_nominal", Json::from(info.bitrate_nominal as i64)),
        ("bad_packets", Json::from(d.stats.bad_packets)),
        ("pages_skipped", Json::from(d.stats.pages_skipped)),
    ];
    if let Some(t) = info.total_frames {
        extra.push(("total_s", Json::from((t as f64 / info.rate as f64 * 1000.0).round() / 1000.0)));
    }
    if let Some(s) = info.loop_start {
        extra.push(("loop_start_frame", Json::from(s)));
    }
    if let Some(e) = info.loop_end {
        extra.push(("loop_end_frame", Json::from(e)));
    }
    let cut = !ended && info.total_frames.is_none_or(|t| t as usize > max_frames);
    let name = path.rsplit('/').next().unwrap_or(path).to_owned();
    Ok(Rendered { name, kind: "ogg", rate: info.rate, channels: out_ch, samples, cut, extra })
}

fn decode_wav(path: &str, bytes: &[u8], max_s: f64) -> Result<Rendered, String> {
    // Only the frames a preview keeps are decoded (the rate the file claims is read first).
    let rate = wav::decode_frames(bytes, 0).map_err(|e| format!("'path' {path}: {e}"))?.rate;
    let max_frames = preview_frames(max_s, rate);
    let w = wav::decode_frames(bytes, max_frames + 1).map_err(|e| format!("'path' {path}: {e}"))?;
    let ch = w.channels as usize;
    let cut = w.samples.len() / ch.max(1) > max_frames;
    let mut samples = w.samples;
    samples.truncate(max_frames * ch);
    let mut stereo = Vec::new();
    let (samples, out_ch) = if ch > 2 {
        vorbis::downmix_stereo(&samples, ch, &mut stereo);
        (stereo, 2)
    } else {
        (samples, ch)
    };
    let name = path.rsplit('/').next().unwrap_or(path).to_owned();
    Ok(Rendered { name, kind: "wav", rate: w.rate, channels: out_ch, samples, cut, extra: vec![("source_channels", Json::from(ch))] })
}

/// The tools' schemas.
pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let obj = |props: Vec<(&str, Json)>| Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::Arr(Vec::new()))]);
    let format = || crate::response::Format::schema();
    vec![
        ToolDef {
            name: "audio_played".into(),
            description: "What played in the last N seconds of the engine's audio (the audio clock: seconds mixed), newest first: each sound or stream's name, bus (master, sfx, ui, ambience, music, voice), who started it (the hosted game's name, or an agent's), and with response_format detailed its gain, voice id, 3D position, looping and duration. by_name counts every match in the window even past limit. Filter by bus, by or name_prefix. Off engines (headless default, --audio off) answer off with the reason. Agents can't hear: check what played here, and how loud in audio_levels.".into(),
            input_schema: obj(vec![
                ("seconds", t("number", "The window, 0.1..600 s back from now (default 10).")),
                ("bus", t("string", "Only this bus: master, sfx, ui, ambience, music or voice.")),
                ("by", t("string", "Only plays started by this name (e.g. the game's name, \"sepulcra\").")),
                ("name_prefix", t("string", "Only sounds whose name starts with this.")),
                ("limit", t("integer", "Plays listed, 1..200 (default 50); counts cover all.")),
                (crate::response::KEY, format()),
            ]),
        },
        ToolDef {
            name: "audio_levels".into(),
            description: "Measured loudness of what the engine is playing, per bus as it enters the master and of the master output (master_out), from the mixer itself (ITU-R BS.1770): momentary (last 400 ms), short-term (last 3 s) and gated integrated LUFS over window_s, sample peak (dBFS) and 4x true peak (dBTP), overs (samples at or past full scale; master_out's should be 0, the limiter catches them), audible_s and silent_for_s (null with never_audible: nothing above -70 LUFS or -60 dBFS yet), and with spectrum octave bands 31.5 Hz..16 kHz in dB (a full-scale sine reads 0). Also every stream (music) with its state, loops and starvation (starved_frames should stay 0; starvation is silence, never garbage, and is traced as audio.stream_starved), and totals (clipped samples, limiter blocks). Silence reads null, not -inf.".into(),
            input_schema: obj(vec![
                ("window_s", t("number", "Integration window for integrated LUFS, peaks, overs and audible_s, 0.1..600 s (default 10).")),
                ("buses", Json::obj([("type", Json::from("array")), ("items", Json::obj([("type", Json::from("string"))])), ("description", Json::from("Which: master, sfx, ui, ambience, music, voice, master_out (default all)."))])),
                ("spectrum", t("boolean", "Add octave-band levels of the last 4096 frames per bus.")),
                (crate::response::KEY, format()),
            ]),
        },
        ToolDef {
            name: "audio_preview".into(),
            description: "Renders or decodes a sound offline, without playing it, and measures it: integrated LUFS, peak and true peak, RMS, spectral centroid, clipped samples, active time, DC offset, octave bands; with wav and png, writes a 16-bit WAV and a waveform-over-spectrogram picture to the captures folder (audio/). Give one of: path (a project file: sounds/*.json recipe, music/*.json piece, an .ogg or .wav uploaded with project_upload), recipe (a sound recipe object, as project_write takes), or music (a music piece object). Music previews one lap plus its last notes' tails and reports lap_s, notes, its cost (pieces over the cost limits are refused before rendering) and the loop's measured loudness against its target; .ogg reports the vendor, length, loop points (LOOPSTART/LOOPEND comments) and damaged packets. At most 120 s of stereo at 48 kHz is kept (rates above 48 kHz count as 48 kHz, so they preview shorter; channels fold to stereo; cut_at_s says when cut); two previews run at once (busy, retry).".into(),
            input_schema: obj(vec![
                ("path", t("string", "A project file: assets/sounds/x.json, assets/music/x.json, or an .ogg / .wav under audio/, sounds/ or music/.")),
                ("recipe", t("object", "An inline sound recipe (the sound format; validated as project_write would).")),
                ("music", t("object", "An inline music piece (the music format; validated as project_write would).")),
                ("variation", t("integer", "A sound recipe's variation (0 = canonical; 1.. apply its vary ranges), 0..64.")),
                ("seconds", t("number", "Render or decode at most this much, 0.1..120 (default 120).")),
                ("wav", t("boolean", "Also write the render as a 16-bit WAV to captures/audio/.")),
                ("png", t("boolean", "Also write a waveform and spectrogram picture to captures/audio/.")),
            ]),
        },
        ToolDef {
            name: "audio_settings".into(),
            description: "The engine's bus volumes (linear gains per bus, 1 = unchanged) and duck rules (up to 4: while source is louder than threshold_db, target drops by depth_db, with attack and release), and who set each. Anyone may read; changing them is lead only (they change what every agent and the human hear). A call applies whole or not at all. The hosted game sets its own on start (Sepulcra: music ducks under voice and sfx).".into(),
            input_schema: obj(vec![
                ("buses", t("object", "Bus name to linear gain 0..4, e.g. {\"music\": 0.5}.")),
                ("ducks", Json::obj([("type", Json::from("array")), ("items", Json::obj([("type", Json::from("object"))])), ("description", Json::from("Rules: {slot 0..3, source, target, depth_db 0..60, threshold_db -90..0 (default -40), attack_s (0.05), release_s (0.4)} or {slot, clear: true}."))])),
                ("fade_s", t("number", "Ramp bus volume changes over this many seconds, 0..10 (default 0.1).")),
            ]),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONO: &[u8] = include_bytes!("../../specula-audio/tests/data/vorbis/sweep_mono_22k.ogg");

    /// The mono fixture with its identification header claiming `channels` at `rate` (it still
    /// decodes: one submap, no coupling), page CRC repaired: the shape of review round 2's
    /// "hi8" probe (8 channels at 384 kHz) without a 14 MB file.
    pub(crate) fn claiming(channels: u8, rate: u32) -> Vec<u8> {
        let mut b = MONO.to_vec();
        let (body, page_len) = {
            let p = specula_audio::ogg::parse_page(&b, 0).unwrap();
            (p.offset + 27 + p.lacing.len(), p.len)
        };
        b[body + 11] = channels;
        b[body + 12..body + 16].copy_from_slice(&rate.to_le_bytes());
        b[22..26].fill(0);
        let c = specula_audio::ogg::crc32(&b[..page_len]);
        b[22..26].copy_from_slice(&c.to_le_bytes());
        b
    }

    /// Previews keep at most MAX_PREVIEW_SAMPLES, folded to stereo as they decode, and a rate
    /// above 48 kHz counts as 48 kHz (review: 8 channels at 384 kHz took 2.2 GB and 17 s).
    #[test]
    fn previews_are_capped_in_samples_not_seconds() {
        assert_eq!(preview_frames(120.0, 384_000), 120 * 48_000);
        assert_eq!(preview_frames(120.0, 22_050), 120 * 22_050);
        assert!(preview_frames(1e9, 48_000) * 2 <= MAX_PREVIEW_SAMPLES);
        let hi8 = claiming(8, 384_000);
        let r = decode_ogg("assets/music/hi8.ogg", hi8, 0.001).unwrap();
        assert_eq!(r.channels, 2, "folded to stereo while decoding");
        assert_eq!(r.samples.len(), 48 * 2, "0.001 s at 48 kHz, not at 384 kHz");
        assert!(r.cut);
        // A whole short file isn't cut.
        let whole = decode_ogg("assets/music/m.ogg", MONO.to_vec(), 120.0).unwrap();
        assert!(!whole.cut && whole.channels == 1 && whole.samples.len() == 22_050);
        // WAV previews decode only the frames they keep.
        let mut wav = specula_audio::wav::encode(&vec![0.25f32; 8 * 96_000], 8, 96_000, specula_audio::wav::WavFormat::Pcm16).unwrap();
        let w = decode_wav("assets/audio/w.wav", &wav, 0.5).unwrap();
        assert_eq!((w.channels, w.samples.len()), (2, 24_000 * 2));
        assert!(w.cut);
        wav.truncate(10);
        assert!(decode_wav("assets/audio/w.wav", &wav, 0.5).is_err());
    }

    /// Changing audio_settings needs the lead; reading doesn't (review round 2: no test).
    #[test]
    fn a_non_lead_is_refused_settings_changes() {
        let audio = AudioService::off("test");
        let session = SessionInfo { id: "audio-settings-agent".into(), client_name: "c".into(), client_version: "1".into(), protocol_version: "x".into() };
        let read = settings(&audio, &session, &Json::Null).unwrap();
        assert!(!read.is_error);
        let change = Json::parse(r#"{"buses": {"music": 0.5}}"#).unwrap();
        let refused = settings(&audio, &session, &change).unwrap();
        assert!(refused.is_error);
        let text = match &refused.content[0] {
            specula_protocol::Content::Text(t) => t.clone(),
            _ => String::new(),
        };
        assert!(text.contains("lead_only") && text.contains("audio_settings"), "{text}");
        assert_eq!(audio.settings_json().get("buses").and_then(|b| b.get("music")).and_then(|m| m.get("gain")).and_then(Json::as_f64), Some(1.0), "nothing changed");
        // The same call from a session the operator made lead goes through.
        crate::authority::publish("audio-settings-lead", Authority::Lead);
        let lead = SessionInfo { id: "audio-settings-lead".into(), ..session };
        let ok = settings(&audio, &lead, &change).unwrap();
        assert!(!ok.is_error, "{:?}", ok.content);
        crate::authority::unpublish("audio-settings-lead");
    }
}
