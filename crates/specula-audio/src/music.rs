//! Music as data: a pattern sequencer agents can compose, rendered as a stream.
//!
//! Instruments are ordinary sound recipes (see the crate docs) played as samplers: each is
//! rendered once at its `root` note and transposed by resampling, gated at the note's length
//! with a release fade. Patterns are bars of notes; tracks play a sequence of patterns on one
//! instrument. The whole piece loops from `loop_from_bar` to its end, and notes that ring past
//! the end carry over into the next lap, so the loop is seamless.
//!
//! ```text
//! { "music": 1, "name": "graveyard", "tempo": 66, "beats_per_bar": 4, "steps_per_beat": 4,
//!   "bars": 16, "loop_from_bar": 0, "loudness": -20,
//!   "instruments": {
//!     "bell": { "sound": { <a sound recipe> }, "root": "A4", "gain_db": -3, "release": 0.4,
//!               "variations": 3 } },
//!   "patterns": {
//!     "motif": { "bars": 2, "notes": [[0, "E5", 6], [8, "C5", 4, 0.7], [16, "A4", 12]] } },
//!   "tracks": [
//!     { "instrument": "bell", "gain_db": -2, "pan": -0.3,
//!       "sequence": ["motif", ".", ".", "motif"] } ] }
//! ```
//!
//! - **Notes**: `[step, note, length_steps, velocity?]`; steps count from the pattern start
//!   (`steps_per_beat` per beat); notes are names (`C4` = MIDI 60, `F#3`, `Bb2`) or MIDI numbers.
//! - **Sequences**: pattern names, or `"."` for one bar of rest. A track may be shorter than the
//!   piece (it then rests) but not longer.
//! - **Variations**: an instrument with `variations` > 1 renders that many takes of its recipe
//!   (its `vary` ranges) and picks one per note from a hash of where the note is: deterministic.
//! - **Loudness**: with `loudness`, one lap is measured (BS.1770, gated) and the piece scaled to
//!   meet it (instead of `gain_db`), never past -1 dBFS peak.
//! - **Strict and bounded**: unknown fields are errors with paths; at most 16 instruments,
//!   64 patterns, 1024 notes per pattern, 16 tracks, 256 bars, [`MAX_EVENTS`] notes a lap,
//!   transposition within ±36 semitones.
//! - **Costed before any work** ([`Cost`], computed by [`Music::from_json`]): the takes synthesised
//!   ([`MAX_TAKE_FLOATS`], only takes some note plays), the transposed takes kept
//!   ([`MAX_SAMPLE_FLOATS`]) and how many ([`MAX_KEYS`]), the note-frames a lap sounds
//!   ([`MAX_LAP_POLYPHONY`] per frame, [`MAX_NOTE_FRAMES`] in all) and the most notes at once
//!   ([`MAX_VOICES`]). A piece over budget is refused with a `limit` error naming the tracks or
//!   instruments; rendering, measuring and mixing never cost more than the model allows.

use crate::analysis::k_weighting;
use crate::error::{AudioError, ErrorCode, Result};
use crate::resample::resample;
use crate::stream::StreamSource;
use crate::synth::{self, Recipe, RenderOptions};
use crate::Bus;
use specula_core::{dmath, Json};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The rate pieces render at (the stream resamples to the device).
pub const RATE: u32 = 48_000;
/// The shortest fade a note ends with (50 ms), when its take ends before its gate does.
const DECLICK_FRAMES: u64 = RATE as u64 / 20;
pub const MAX_INSTRUMENTS: usize = 16;
pub const MAX_PATTERNS: usize = 64;
pub const MAX_NOTES: usize = 1024;
pub const MAX_TRACKS: usize = 16;
pub const MAX_BARS: usize = 256;
pub const MAX_VARIATIONS: u32 = 4;
/// Most floats of transposed takes one piece may keep (64 MB).
pub const MAX_SAMPLE_FLOATS: usize = 16 << 20;
/// Longest piece, in seconds (a lap).
pub const MAX_SECONDS: f64 = 600.0;
/// Most notes one lap may play (every track's sequence expanded).
pub const MAX_EVENTS: usize = 20_000;

pub const MUSIC_FIELDS: &[&str] = &["music", "name", "bus", "tempo", "beats_per_bar", "steps_per_beat", "bars", "loop_from_bar", "loudness", "gain_db", "instruments", "patterns", "tracks", "notes"];
const INSTRUMENT_FIELDS: &[&str] = &["sound", "root", "gain_db", "release", "variations"];
const PATTERN_FIELDS: &[&str] = &["bars", "notes"];
const TRACK_FIELDS: &[&str] = &["instrument", "gain_db", "pan", "sequence"];

#[derive(Clone, Debug, PartialEq)]
pub struct Instrument {
    pub name: String,
    pub recipe: Recipe,
    pub root: i32,
    pub gain_db: f64,
    pub release: f64,
    pub variations: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub step: u32,
    pub pitch: i32,
    pub length: u32,
    pub velocity: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub name: String,
    pub bars: u32,
    pub notes: Vec<Note>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub instrument: usize,
    pub gain_db: f64,
    pub pan: f64,
    /// Pattern index per entry, `None` for a bar of rest.
    pub sequence: Vec<Option<usize>>,
}

/// A parsed, validated piece.
#[derive(Clone, Debug, PartialEq)]
pub struct Music {
    pub name: String,
    pub bus: Bus,
    pub tempo: f64,
    pub beats_per_bar: u32,
    pub steps_per_beat: u32,
    pub bars: u32,
    pub loop_from_bar: u32,
    pub loudness: Option<f64>,
    pub gain_db: f64,
    pub instruments: Vec<Instrument>,
    pub patterns: Vec<Pattern>,
    pub tracks: Vec<Track>,
    /// What it costs to render and play, checked against the limits when it was parsed.
    pub cost: Cost,
}

fn err(code: ErrorCode, path: impl Into<String>, msg: impl Into<String>) -> AudioError {
    AudioError::at(code, path, msg)
}

fn obj<'a>(j: &'a Json, path: &str, allowed: &[&str]) -> Result<&'a [(String, Json)]> {
    let pairs = j.as_object().ok_or_else(|| err(ErrorCode::WrongType, path, "expected an object"))?;
    for (i, (k, _)) in pairs.iter().enumerate() {
        if !allowed.contains(&k.as_str()) {
            return Err(err(ErrorCode::UnknownField, format!("{path}.{k}").trim_start_matches('.'), format!("unknown field `{k}` (allowed: {})", allowed.join(", "))));
        }
        if pairs[..i].iter().any(|(p, _)| p == k) {
            return Err(err(ErrorCode::Duplicate, format!("{path}.{k}").trim_start_matches('.'), format!("duplicate field `{k}`")));
        }
    }
    Ok(pairs)
}

/// Input echoed in an error, cut to 64 characters.
fn echo(s: &str) -> String {
    s.chars().take(64).collect()
}

fn field<'a>(pairs: &'a [(String, Json)], key: &str) -> Option<&'a Json> {
    pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn at(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

fn number(pairs: &[(String, Json)], path: &str, key: &str, lo: f64, hi: f64, default: Option<f64>) -> Result<f64> {
    let p = at(path, key);
    match field(pairs, key) {
        None => default.ok_or_else(|| err(ErrorCode::MissingField, p, format!("missing required field `{key}`"))),
        Some(j) => {
            let v = j.as_f64().ok_or_else(|| err(ErrorCode::WrongType, &p, "expected a number"))?;
            if !(lo..=hi).contains(&v) {
                return Err(err(ErrorCode::OutOfRange, p, format!("must be {lo}..={hi}")).with_numbers(v, if v < lo { lo } else { hi }));
            }
            Ok(v)
        }
    }
}

fn integer(pairs: &[(String, Json)], path: &str, key: &str, lo: u32, hi: u32, default: Option<u32>) -> Result<u32> {
    let v = number(pairs, path, key, lo as f64, hi as f64, default.map(|d| d as f64))?;
    if v.fract() != 0.0 {
        return Err(err(ErrorCode::WrongType, at(path, key), "expected a whole number"));
    }
    Ok(v as u32)
}

/// A note name (`C4` = 60, `F#3`, `Bb2`) or a MIDI number, as MIDI.
pub fn parse_note(j: &Json, path: &str) -> Result<i32> {
    if let Some(n) = j.as_f64() {
        if n.fract() != 0.0 || !(0.0..=127.0).contains(&n) {
            return Err(err(ErrorCode::OutOfRange, path, "a MIDI note is a whole number 0..=127").with_numbers(n, 127.0));
        }
        return Ok(n as i32);
    }
    let s = j.as_str().ok_or_else(|| err(ErrorCode::WrongType, path, "expected a note name like \"A4\" or a MIDI number"))?;
    let b = s.as_bytes();
    let base = match b.first().map(|c| c.to_ascii_uppercase()) {
        Some(b'C') => 0,
        Some(b'D') => 2,
        Some(b'E') => 4,
        Some(b'F') => 5,
        Some(b'G') => 7,
        Some(b'A') => 9,
        Some(b'B') => 11,
        _ => return Err(err(ErrorCode::UnknownName, path, format!("`{}` is not a note name (C D E F G A B, then # or b, then an octave)", echo(s)))),
    };
    let (accidental, rest) = match b.get(1) {
        Some(b'#') => (1, &s[2..]),
        Some(b'b') => (-1, &s[2..]),
        _ => (0, &s[1..]),
    };
    let octave: i32 = rest.parse().map_err(|_| err(ErrorCode::UnknownName, path, format!("`{}` has no octave number", echo(s))))?;
    // MIDI covers octaves -1..=9; checked before any arithmetic (review: C2147483647 overflowed).
    if !(-1..=9).contains(&octave) {
        return Err(err(ErrorCode::OutOfRange, path, format!("`{}`: octaves are -1..=9", echo(s))).with_numbers(octave as f64, 9.0));
    }
    let midi = (octave + 1) * 12 + base + accidental;
    if !(0..=127).contains(&midi) {
        return Err(err(ErrorCode::OutOfRange, path, "note outside MIDI 0..=127").with_numbers(midi as f64, 127.0));
    }
    Ok(midi)
}

impl Music {
    pub fn parse(text: &str) -> Result<Music> {
        let j = Json::parse(text).map_err(|e| AudioError::new(ErrorCode::Json, e.to_string()).with_offset(e.offset))?;
        Music::from_json(&j)
    }

    pub fn from_json(j: &Json) -> Result<Music> {
        let top = obj(j, "", MUSIC_FIELDS)?;
        number(top, "", "music", 1.0, 1.0, None)?;
        let name = match field(top, "name") {
            Some(n) => n.as_str().filter(|s| !s.is_empty() && s.len() <= 64).ok_or_else(|| err(ErrorCode::WrongType, "name", "expected a name of 1..=64 characters"))?.to_string(),
            None => return Err(err(ErrorCode::MissingField, "name", "missing required field `name`")),
        };
        let bus = match field(top, "bus") {
            None => Bus::Music,
            Some(b) => {
                let s = b.as_str().ok_or_else(|| err(ErrorCode::WrongType, "bus", "expected a bus name"))?;
                Bus::from_name(s).ok_or_else(|| err(ErrorCode::UnknownName, "bus", "unknown bus (master, sfx, ui, ambience, music, voice)"))?
            }
        };
        if let Some(n) = field(top, "notes") {
            if n.as_str().is_none_or(|s| s.len() > 4096) {
                return Err(err(ErrorCode::WrongType, "notes", "expected a string of at most 4096 bytes"));
            }
        }
        let tempo = number(top, "", "tempo", 20.0, 300.0, None)?;
        let beats_per_bar = integer(top, "", "beats_per_bar", 1, 16, Some(4))?;
        let steps_per_beat = integer(top, "", "steps_per_beat", 1, 16, Some(4))?;
        let bars = integer(top, "", "bars", 1, MAX_BARS as u32, None)?;
        let loop_from_bar = integer(top, "", "loop_from_bar", 0, bars - 1, Some(0))?;
        let loudness = field(top, "loudness").map(|_| number(top, "", "loudness", -60.0, -6.0, None)).transpose()?;
        let gain_db = number(top, "", "gain_db", -60.0, 12.0, Some(0.0))?;
        let lap_s = bars as f64 * beats_per_bar as f64 * 60.0 / tempo;
        if lap_s > MAX_SECONDS {
            return Err(err(ErrorCode::Limit, "bars", "the piece is longer than allowed").with_numbers(lap_s, MAX_SECONDS));
        }

        let ins = field(top, "instruments").ok_or_else(|| err(ErrorCode::MissingField, "instruments", "missing required field `instruments`"))?;
        let ins = ins.as_object().ok_or_else(|| err(ErrorCode::WrongType, "instruments", "expected an object of instruments by name"))?;
        if ins.is_empty() || ins.len() > MAX_INSTRUMENTS {
            return Err(err(ErrorCode::Limit, "instruments", "1..=16 instruments").with_numbers(ins.len() as f64, MAX_INSTRUMENTS as f64));
        }
        let mut instruments = Vec::new();
        for (i, (iname, ij)) in ins.iter().enumerate() {
            let p = format!("instruments.{iname}");
            if ins[..i].iter().any(|(k, _)| k == iname) {
                return Err(err(ErrorCode::Duplicate, p, "duplicate instrument"));
            }
            let f = obj(ij, &p, INSTRUMENT_FIELDS)?;
            let sound = field(f, "sound").ok_or_else(|| err(ErrorCode::MissingField, at(&p, "sound"), "missing required field `sound` (a sound recipe)"))?;
            let recipe = Recipe::from_json(sound).map_err(|mut e| {
                e.path = if e.path.is_empty() { at(&p, "sound") } else { format!("{p}.sound.{}", e.path) };
                e
            })?;
            let root = match field(f, "root") {
                Some(r) => parse_note(r, &at(&p, "root"))?,
                None => 69,
            };
            instruments.push(Instrument {
                name: iname.clone(),
                recipe,
                root,
                gain_db: number(f, &p, "gain_db", -60.0, 12.0, Some(0.0))?,
                release: number(f, &p, "release", 0.0, 10.0, Some(0.1))?,
                variations: integer(f, &p, "variations", 1, MAX_VARIATIONS, Some(1))?,
            });
        }

        let pats = field(top, "patterns").ok_or_else(|| err(ErrorCode::MissingField, "patterns", "missing required field `patterns`"))?;
        let pats = pats.as_object().ok_or_else(|| err(ErrorCode::WrongType, "patterns", "expected an object of patterns by name"))?;
        if pats.len() > MAX_PATTERNS {
            return Err(err(ErrorCode::Limit, "patterns", "too many patterns").with_numbers(pats.len() as f64, MAX_PATTERNS as f64));
        }
        let steps_per_bar = beats_per_bar * steps_per_beat;
        let mut patterns = Vec::new();
        for (i, (pname, pj)) in pats.iter().enumerate() {
            let p = format!("patterns.{pname}");
            if pname == "." || pats[..i].iter().any(|(k, _)| k == pname) {
                return Err(err(ErrorCode::Duplicate, p, "duplicate (or reserved `.`) pattern name"));
            }
            let f = obj(pj, &p, PATTERN_FIELDS)?;
            let pbars = integer(f, &p, "bars", 1, MAX_BARS as u32, Some(1))?;
            let nj = field(f, "notes").ok_or_else(|| err(ErrorCode::MissingField, at(&p, "notes"), "missing required field `notes`"))?;
            let nj = nj.as_array().ok_or_else(|| err(ErrorCode::WrongType, at(&p, "notes"), "expected an array of [step, note, length, velocity?]"))?;
            if nj.len() > MAX_NOTES {
                return Err(err(ErrorCode::Limit, at(&p, "notes"), "too many notes").with_numbers(nj.len() as f64, MAX_NOTES as f64));
            }
            let total_steps = pbars * steps_per_bar;
            let mut notes = Vec::with_capacity(nj.len());
            for (k, n) in nj.iter().enumerate() {
                let np = format!("{p}.notes[{k}]");
                let a = n.as_array().filter(|a| (3..=4).contains(&a.len())).ok_or_else(|| err(ErrorCode::WrongType, &np, "expected [step, note, length_steps, velocity?]"))?;
                let whole = |j: &Json, what: &str, lo: f64, hi: f64| -> Result<u32> {
                    let v = j.as_f64().filter(|v| v.fract() == 0.0).ok_or_else(|| err(ErrorCode::WrongType, &np, format!("{what} must be a whole number")))?;
                    if !(lo..=hi).contains(&v) {
                        return Err(err(ErrorCode::OutOfRange, &np, format!("{what} must be {lo}..={hi}")).with_numbers(v, hi));
                    }
                    Ok(v as u32)
                };
                let step = whole(&a[0], "step", 0.0, total_steps as f64 - 1.0)?;
                let pitch = parse_note(&a[1], &np)?;
                let length = whole(&a[2], "length", 1.0, (MAX_BARS as u32 * steps_per_bar) as f64)?;
                let velocity = match a.get(3) {
                    Some(v) => v.as_f64().filter(|v| (0.0..=1.0).contains(v)).ok_or_else(|| err(ErrorCode::OutOfRange, &np, "velocity must be 0..=1"))?,
                    None => 1.0,
                };
                notes.push(Note { step, pitch, length, velocity });
            }
            patterns.push(Pattern { name: pname.clone(), bars: pbars, notes });
        }

        let tj = field(top, "tracks").ok_or_else(|| err(ErrorCode::MissingField, "tracks", "missing required field `tracks`"))?;
        let tj = tj.as_array().ok_or_else(|| err(ErrorCode::WrongType, "tracks", "expected an array of tracks"))?;
        if tj.is_empty() || tj.len() > MAX_TRACKS {
            return Err(err(ErrorCode::Limit, "tracks", "1..=16 tracks").with_numbers(tj.len() as f64, MAX_TRACKS as f64));
        }
        let mut tracks = Vec::new();
        for (t, j) in tj.iter().enumerate() {
            let p = format!("tracks[{t}]");
            let f = obj(j, &p, TRACK_FIELDS)?;
            let iname = field(f, "instrument").and_then(|v| v.as_str()).ok_or_else(|| err(ErrorCode::MissingField, at(&p, "instrument"), "missing `instrument` (a name)"))?;
            let instrument = instruments.iter().position(|i| i.name == iname).ok_or_else(|| err(ErrorCode::UnknownName, at(&p, "instrument"), format!("no instrument `{}`", echo(iname))))?;
            let sj = field(f, "sequence").and_then(|v| v.as_array()).ok_or_else(|| err(ErrorCode::MissingField, at(&p, "sequence"), "missing `sequence` (pattern names and \".\" rests)"))?;
            let mut sequence = Vec::new();
            let mut used = 0u32;
            for (k, s) in sj.iter().enumerate() {
                let sp = format!("{p}.sequence[{k}]");
                let s = s.as_str().ok_or_else(|| err(ErrorCode::WrongType, &sp, "expected a pattern name or \".\""))?;
                if s == "." {
                    sequence.push(None);
                    used += 1;
                } else {
                    let pi = patterns.iter().position(|x| x.name == s).ok_or_else(|| err(ErrorCode::UnknownName, &sp, format!("no pattern `{}`", echo(s))))?;
                    sequence.push(Some(pi));
                    used += patterns[pi].bars;
                }
                if used > bars {
                    return Err(err(ErrorCode::OutOfRange, sp, "the track runs past the piece's `bars`").with_numbers(used as f64, bars as f64));
                }
            }
            tracks.push(Track {
                instrument,
                gain_db: number(f, &p, "gain_db", -60.0, 12.0, Some(0.0))?,
                pan: number(f, &p, "pan", -1.0, 1.0, Some(0.0))?,
                sequence,
            });
        }
        let mut m = Music { name, bus, tempo, beats_per_bar, steps_per_beat, bars, loop_from_bar, loudness, gain_db, instruments, patterns, tracks, cost: Cost::default() };
        // The cost model, before any audio is made: a piece over budget is refused here, so no
        // caller (a preview, a hot reload, the game) ever starts work it can't afford.
        m.cost = m.plan()?.cost;
        Ok(m)
    }

    /// Frames per step at [`RATE`].
    pub fn step_frames(&self) -> f64 {
        RATE as f64 * 60.0 / self.tempo / self.steps_per_beat as f64
    }

    /// Frames in one lap of the piece.
    pub fn lap_frames(&self) -> u64 {
        (self.bars as f64 * (self.beats_per_bar * self.steps_per_beat) as f64 * self.step_frames()).round() as u64
    }

    /// Frame where the loop starts.
    pub fn loop_frame(&self) -> u64 {
        (self.loop_from_bar as f64 * (self.beats_per_bar * self.steps_per_beat) as f64 * self.step_frames()).round() as u64
    }
}

/// One note to play: where, which rendered sample, and its channel gains.
#[derive(Clone, Copy, Debug)]
struct Event {
    start: u64,
    /// The transposed take it plays.
    sample: usize,
    gain: [f32; 2],
    /// Frames it sounds (gate plus release, at most the take's length) and its gate: past the
    /// gate it fades out linearly to `len`.
    len: u64,
    gate: u64,
}

/// A rendered piece, ready to stream.
#[derive(Debug)]
pub struct Rendered {
    pub name: String,
    pub bus: Bus,
    /// Note samples (stereo, [`RATE`]).
    samples: Vec<Arc<[f32]>>,
    events: Vec<Event>,
    pub lap_frames: u64,
    pub loop_frame: u64,
    /// Longest note sample (frames): how far a note can ring past its start.
    max_len: u64,
    /// Where the first lap's last note has rung out (frames).
    ring_out: u64,
    /// The overall gain applied (loudness target and `gain_db`).
    pub gain: f32,
    /// Measured integrated loudness of one lap after `gain` (LUFS), when measured.
    pub lufs: Option<f64>,
}

fn hash3(a: u64, b: u64, c: u64) -> u64 {
    let mut x = a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ c.wrapping_mul(0x1656_67B1_9E37_79F9);
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^ (x >> 29)
}

/// What a piece costs, from its score alone, before any audio is made. Rendering, measuring
/// and mixing it never cost more than this (see [`MAX_TAKE_FLOATS`], [`MAX_LAP_POLYPHONY`],
/// [`MAX_NOTE_FRAMES`], [`MAX_VOICES`], [`MAX_KEYS`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cost {
    /// Floats of instrument takes synthesised (stereo, only the takes some note plays).
    pub take_floats: usize,
    /// Frames of notes sounding in one lap, summed over every note: the work of mixing a lap.
    pub note_frames: u64,
    /// The most notes sounding at once (the work of mixing one frame).
    pub max_voices: usize,
    /// Distinct transposed takes (instrument, take, pitch) to resample.
    pub keys: usize,
    /// Floats the transposed takes keep (stereo, each as long as its longest note sounds).
    pub sample_floats: usize,
    /// Estimated synthesis work for the takes some note plays, in [`synth::recipe::MAX_WORK`]'s
    /// cost units (about ns of one core): each take is one render of its instrument's recipe.
    pub synth_work: f64,
}

impl Cost {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("take_floats", Json::from(self.take_floats)),
            ("note_frames", Json::from(self.note_frames)),
            ("max_voices", Json::from(self.max_voices)),
            ("keys", Json::from(self.keys)),
            ("sample_floats", Json::from(self.sample_floats)),
            ("synth_work", Json::from(self.synth_work)),
        ])
    }
}

/// Most floats of instrument takes a piece may synthesise (about 87 s of stereo at 48 kHz).
pub const MAX_TAKE_FLOATS: usize = 8 << 20;
/// Most note-frames a lap may sound, per frame of the lap (an average polyphony of 32).
pub const MAX_LAP_POLYPHONY: u64 = 32;
/// Most note-frames a lap may sound in all (about 70 minutes of single notes).
pub const MAX_NOTE_FRAMES: u64 = 200_000_000;
/// Most notes sounding at once.
pub const MAX_VOICES: usize = 64;
/// Most distinct transposed takes.
pub const MAX_KEYS: usize = 512;
/// Most synthesis work a piece's takes may cost in all: four of the costliest recipes allowed
/// (about 1.2 s of one core in a release build). Short takes pass [`MAX_TAKE_FLOATS`] easily, so
/// without this a piece of many short but dense instruments could take tens of seconds to make.
pub const MAX_SYNTH_WORK: f64 = 4.0 * synth::recipe::MAX_WORK;

/// One note as planned.
struct Planned {
    ins: usize,
    take: u32,
    semis: i32,
    gate: u64,
    start: u64,
    gain: [f32; 2],
    /// Frames it can sound at most: its gate plus release, or its transposed take's length.
    len_bound: u64,
}

/// The whole piece's notes, the takes and transpositions they need, and what that costs.
struct Plan {
    notes: Vec<Planned>,
    /// (instrument, take, semitones) → frames the longest note on it sounds.
    needs: BTreeMap<(usize, u32, i32), u64>,
    takes: std::collections::BTreeSet<(usize, u32)>,
    cost: Cost,
}

impl Music {
    /// Lays out every note and checks the cost model; errors name the instruments or tracks
    /// that pass a limit. Cheap: no audio is made.
    fn plan(&self) -> Result<Plan> {
        let step = self.step_frames();
        let steps_per_bar = (self.beats_per_bar * self.steps_per_beat) as u64;
        let take_frames = |i: usize| (self.instruments[i].recipe.duration * RATE as f64).round().max(1.0) as u64;
        let mut notes: Vec<Planned> = Vec::new();
        let mut needs: BTreeMap<(usize, u32, i32), u64> = BTreeMap::new();
        let mut takes = std::collections::BTreeSet::new();
        let mut track_frames = vec![0u64; self.tracks.len()];
        for (ti, t) in self.tracks.iter().enumerate() {
            let ins = &self.instruments[t.instrument];
            let tg = dmath::exp2_f64((t.gain_db + ins.gain_db) / 6.020_599_913_279_624) as f32;
            let pan = t.pan as f32;
            let bal = [(1.0 - pan).min(1.0), (1.0 + pan).min(1.0)];
            let rel = (ins.release * RATE as f64) as u64;
            let mut bar = 0u64;
            for (si, entry) in t.sequence.iter().enumerate() {
                let Some(pi) = entry else {
                    bar += 1;
                    continue;
                };
                let pat = &self.patterns[*pi];
                for (ni, n) in pat.notes.iter().enumerate() {
                    let semis = n.pitch - ins.root;
                    if !(-36..=36).contains(&semis) {
                        return Err(err(ErrorCode::OutOfRange, format!("patterns.{}.notes[{ni}]", pat.name), format!("note is {semis} semitones from instrument `{}`'s root (at most 36)", ins.name)).with_numbers(semis as f64, 36.0));
                    }
                    if notes.len() >= MAX_EVENTS {
                        return Err(err(ErrorCode::Limit, format!("tracks[{ti}]"), "the piece plays more notes than allowed").with_numbers(notes.len() as f64 + 1.0, MAX_EVENTS as f64));
                    }
                    let take = if ins.variations > 1 { (hash3(ti as u64, (bar << 16) | si as u64, ni as u64) % ins.variations as u64) as u32 } else { 0 };
                    let gate = (n.length as f64 * step).round() as u64;
                    // A take transposed up by r plays 1/r as long.
                    let r = dmath::exp2_f64(semis as f64 / 12.0);
                    let len_bound = (gate + rel).min((take_frames(t.instrument) as f64 / r).ceil() as u64);
                    let need = needs.entry((t.instrument, take, semis)).or_default();
                    *need = (*need).max(len_bound);
                    takes.insert((t.instrument, take));
                    track_frames[ti] += len_bound;
                    let start = ((bar * steps_per_bar + n.step as u64) as f64 * step).round() as u64;
                    let g = tg * n.velocity as f32;
                    notes.push(Planned { ins: t.instrument, take, semis, gate, start, gain: [g * bal[0], g * bal[1]], len_bound });
                }
                bar += pat.bars as u64;
            }
        }
        let mut cost = Cost { keys: needs.len(), ..Cost::default() };
        // Takes: what synthesis makes.
        let mut per_instrument = vec![0usize; self.instruments.len()];
        for &(i, _) in &takes {
            per_instrument[i] += take_frames(i) as usize * 2;
        }
        cost.take_floats = per_instrument.iter().sum();
        if cost.take_floats > MAX_TAKE_FLOATS {
            let mut worst: Vec<(usize, usize)> = per_instrument.iter().copied().enumerate().filter(|&(_, f)| f > 0).collect();
            worst.sort_by_key(|w| std::cmp::Reverse(w.1));
            let names: Vec<String> = worst.iter().take(4).map(|&(i, f)| format!("`{}` ({:.0} s of takes)", echo(&self.instruments[i].name), f as f64 / 2.0 / RATE as f64)).collect();
            return Err(err(ErrorCode::Limit, "instruments", format!("the instruments' takes are longer than allowed: {} (shorter sounds, fewer variations, or fewer instruments)", names.join(", "))).with_numbers(cost.take_floats as f64, MAX_TAKE_FLOATS as f64));
        }
        // Synthesis: what making those takes costs in CPU (each take is one recipe render).
        let mut work = vec![0f64; self.instruments.len()];
        for &(i, _) in &takes {
            work[i] += self.instruments[i].recipe.work_estimate(RATE);
        }
        cost.synth_work = work.iter().sum();
        if cost.synth_work > MAX_SYNTH_WORK {
            let mut worst: Vec<(usize, f64)> = work.iter().copied().enumerate().filter(|&(_, w)| w > 0.0).collect();
            worst.sort_by(|a, b| b.1.total_cmp(&a.1));
            let names: Vec<String> = worst.iter().take(4).map(|&(i, w)| format!("`{}` ({:.2} s)", echo(&self.instruments[i].name), w / 1e9)).collect();
            return Err(err(ErrorCode::Limit, "instruments", format!("the instruments' takes cost too much to synthesise (estimated {:.1} s of CPU): {} (simpler recipes, fewer variations, or fewer instruments)", cost.synth_work / 1e9, names.join(", "))).with_numbers(cost.synth_work, MAX_SYNTH_WORK));
        }
        if cost.keys > MAX_KEYS {
            return Err(err(ErrorCode::Limit, "instruments", "too many distinct pitches per instrument take (each is resampled once)").with_numbers(cost.keys as f64, MAX_KEYS as f64));
        }
        // Transposed takes: what is kept to play from (and resampled once each).
        let mut kept = vec![0usize; self.instruments.len()];
        for (&(i, _, _), &frames) in &needs {
            kept[i] += frames as usize * 2;
        }
        cost.sample_floats = kept.iter().sum();
        if cost.sample_floats > MAX_SAMPLE_FLOATS {
            let (i, f) = kept.iter().copied().enumerate().max_by_key(|&(_, f)| f).unwrap_or((0, 0));
            return Err(err(ErrorCode::Limit, format!("instruments.{}", echo(&self.instruments[i].name)), format!("the transposed takes kept to play from are larger than allowed (instrument `{}` alone keeps {:.0} s): fewer distinct pitches or shorter notes", echo(&self.instruments[i].name), f as f64 / 2.0 / RATE as f64)).with_numbers(cost.sample_floats as f64, MAX_SAMPLE_FLOATS as f64));
        }
        // Note-frames: what mixing a lap costs.
        cost.note_frames = track_frames.iter().sum();
        let lap = self.lap_frames().max(1);
        let allowed = (lap * MAX_LAP_POLYPHONY).min(MAX_NOTE_FRAMES);
        if cost.note_frames > allowed {
            let (ti, f) = track_frames.iter().copied().enumerate().max_by_key(|&(_, f)| f).unwrap_or((0, 0));
            return Err(err(ErrorCode::Limit, format!("tracks[{ti}]"), format!("the notes sound for too long in all ({:.0} s of notes in a {:.0} s lap; track {ti} alone sounds {:.0} s): shorter notes, fewer notes, or shorter releases", cost.note_frames as f64 / RATE as f64, lap as f64 / RATE as f64, f as f64 / RATE as f64)).with_numbers(cost.note_frames as f64, allowed as f64));
        }
        // Voices: what mixing one frame costs.
        let mut edges: Vec<(u64, i32)> = Vec::with_capacity(notes.len() * 2);
        for n in &notes {
            edges.push((n.start, 1));
            edges.push((n.start + n.len_bound, -1));
        }
        edges.sort();
        let (mut now, mut peak, mut at) = (0i32, 0i32, 0u64);
        for (t, d) in edges {
            now += d;
            if now > peak {
                peak = now;
                at = t;
            }
        }
        cost.max_voices = peak.max(0) as usize;
        if cost.max_voices > MAX_VOICES {
            return Err(err(ErrorCode::Limit, "tracks", format!("{} notes sound at once at {:.2} s (fewer overlapping notes, or shorter releases)", cost.max_voices, at as f64 / RATE as f64)).with_numbers(cost.max_voices as f64, MAX_VOICES as f64));
        }
        Ok(Plan { notes, needs, takes, cost })
    }
}

/// Renders the instruments and lays out every note (up to seconds of CPU for long pieces, all
/// within the piece's [`Cost`]: run it on a worker, never on the engine thread).
pub fn render(m: &Music) -> Result<Rendered> {
    // The plan and its cost were checked when the piece was parsed (`Music::from_json`); the
    // work below never exceeds that cost.
    let plan = m.plan()?;
    // Only the takes some note plays are synthesised, stereo at RATE.
    let mut bases: BTreeMap<(usize, u32), Vec<f32>> = BTreeMap::new();
    for &(i, take) in &plan.takes {
        let ins = &m.instruments[i];
        let r = synth::render(&ins.recipe, &RenderOptions { rate: RATE, variation: if ins.variations > 1 { take + 1 } else { 0 } })?;
        let stereo = if r.channels == 1 { r.samples.iter().flat_map(|&s| [s, s]).collect() } else { r.samples };
        bases.insert((i, take), stereo);
    }
    // Each take transposed once per pitch, and only as far as its longest note sounds.
    let mut transposed: BTreeMap<(usize, u32, i32), Vec<f32>> = BTreeMap::new();
    for (&(ins, take, semis), &frames) in &plan.needs {
        let base = &bases[&(ins, take)];
        let base_frames = base.len() / 2;
        let r = dmath::exp2_f64(semis as f64 / 12.0);
        // Input frames that make `frames` output frames, plus the resampler's kernel.
        let input = ((frames as f64 * r).ceil() as usize + 16).min(base_frames);
        let mut t = if semis == 0 {
            base[..input * 2].to_vec()
        } else {
            // Playing faster by r raises the pitch: resample from RATE·r to RATE.
            resample(&base[..input * 2], 2, (RATE as f64 * r).round() as u32, RATE)
        };
        t.truncate((frames as usize).min(t.len() / 2) * 2);
        transposed.insert((ins, take, semis), t);
    }
    drop(bases);
    // Notes play their transposed take, gated and faded as they mix: no copy per length.
    let mut index: BTreeMap<(usize, u32, i32), usize> = BTreeMap::new();
    let mut samples: Vec<Arc<[f32]>> = Vec::with_capacity(transposed.len());
    for (key, t) in transposed {
        index.insert(key, samples.len());
        samples.push(t.into());
    }
    let mut events = Vec::with_capacity(plan.notes.len());
    for p in plan.notes {
        let sample = index[&(p.ins, p.take, p.semis)];
        // Never longer than the plan counted (`len_bound`), so mixing costs what was checked.
        let len = p.len_bound.min((samples[sample].len() / 2) as u64);
        events.push(Event { start: p.start, sample, gain: p.gain, len, gate: p.gate });
    }
    events.sort_by_key(|e| (e.start, e.sample));
    let max_len = events.iter().map(|e| e.len).max().unwrap_or(0);
    let ring_out = events.iter().map(|e| e.start + e.len).max().unwrap_or(0);
    let mut r = Rendered {
        name: m.name.clone(),
        bus: m.bus,
        samples,
        events,
        lap_frames: m.lap_frames().max(1),
        loop_frame: m.loop_frame(),
        max_len,
        ring_out,
        gain: dmath::exp2_f64(m.gain_db / 6.020_599_913_279_624) as f32,
        lufs: None,
    };
    if let Some(target) = m.loudness {
        let (lufs, peak) = r.measure();
        if lufs.is_finite() && peak > 0.0 {
            // The target replaces `gain_db`; never past -1 dBFS sample peak.
            let want = dmath::exp2_f64((target - lufs) / 6.020_599_913_279_624);
            let g = want.min(0.891 / peak as f64);
            r.gain = g as f32;
            r.lufs = Some(lufs + 20.0 * dmath::log2_f64(g) * std::f64::consts::LOG10_2);
        }
    }
    Ok(r)
}

impl Rendered {
    /// Mixes frames `[from, from + n)` of the endless stream (the first lap, then laps from the
    /// loop point) into `out` (stereo).
    pub fn mix(&self, from: u64, out: &mut [f32]) {
        self.mix_unscaled(from, out);
        for s in out.iter_mut() {
            *s *= self.gain;
        }
    }

    fn mix_unscaled(&self, from: u64, out: &mut [f32]) {
        out.fill(0.0);
        let n = (out.len() / 2) as u64;
        let mut pos = from;
        let end = from + n;
        while pos < end {
            // Where `pos` falls: lap 0 is 0..lap; later laps cover loop..lap.
            let (q, lap0) = self.song_pos(pos);
            let run = (self.lap_frames - q).min(end - pos);
            let o = &mut out[((pos - from) * 2) as usize..((pos - from + run) * 2) as usize];
            self.mix_window(q, o);
            if !lap0 {
                // Tails of the previous lap ring on: events of that lap sit `loop_len` earlier in
                // song time, so frame q of this lap is frame q + loop_len of theirs.
                let loop_len = self.lap_frames - self.loop_frame;
                self.mix_window(q + loop_len, o);
            }
            pos += run;
        }
    }

    /// Song position of stream frame `pos` and whether it is in the first lap.
    fn song_pos(&self, pos: u64) -> (u64, bool) {
        if pos < self.lap_frames {
            return (pos, true);
        }
        let loop_len = (self.lap_frames - self.loop_frame).max(1);
        (self.loop_frame + (pos - self.lap_frames) % loop_len, false)
    }

    /// Adds every event sounding at song frames `[q, q + out.len() / 2)` into `out`.
    fn mix_window(&self, q: u64, out: &mut [f32]) {
        let len = (out.len() / 2) as u64;
        let lo = q.saturating_sub(self.max_len);
        let first = self.events.partition_point(|e| e.start < lo);
        for e in &self.events[first..] {
            if e.start >= q + len {
                break;
            }
            let s = &self.samples[e.sample];
            let frames = e.len;
            if e.start + frames <= q {
                continue;
            }
            let a = q.max(e.start);
            let b = (q + len).min(e.start + frames);
            // Past the gate the note fades linearly to silence at `len` (its release); a note cut
            // short by the end of its take still fades over its last 50 ms (no click).
            let fade_from = e.gate.min(frames.saturating_sub(DECLICK_FRAMES));
            let span = (frames - fade_from).max(1) as f32;
            for f in a..b {
                let i = f - e.start;
                let g = if i < fade_from { 1.0 } else { 1.0 - (i - fade_from) as f32 / span };
                let si = (i * 2) as usize;
                let oi = ((f - q) * 2) as usize;
                out[oi] += s[si] * e.gain[0] * g;
                out[oi + 1] += s[si + 1] * e.gain[1] * g;
            }
        }
    }

    /// Integrated loudness (BS.1770, gated) and sample peak of one lap (with the laps' tails as
    /// they sound when looping), before `gain`.
    fn measure(&self) -> (f64, f32) {
        let mut k = [k_weighting(RATE), k_weighting(RATE)];
        let block = (RATE as usize * 4) / 10;
        let hop = block / 4;
        let mut energies = Vec::new();
        let mut window: Vec<f64> = Vec::new();
        let mut peak = 0f32;
        let mut buf = vec![0f32; hop * 2];
        // Measure the second lap when looping from the start (tails included), else the first.
        let start = if self.loop_frame == 0 { self.lap_frames } else { 0 };
        let mut pos = start;
        while pos < start + self.lap_frames {
            let n = (hop as u64).min(start + self.lap_frames - pos) as usize;
            let b = &mut buf[..n * 2];
            self.mix_unscaled(pos, b);
            let mut acc = 0f64;
            for f in 0..n {
                let (l, r) = (b[2 * f], b[2 * f + 1]);
                peak = peak.max(l.abs()).max(r.abs());
                let yl = k[0].1.process(k[0].0.process(l as f64));
                let yr = k[1].1.process(k[1].0.process(r as f64));
                acc += yl * yl + yr * yr;
            }
            window.push(acc / n as f64);
            if window.len() >= 4 {
                let w = &window[window.len() - 4..];
                energies.push(w.iter().sum::<f64>() / 4.0);
            }
            pos += n as u64;
        }
        let l = |e: f64| if e > 0.0 { -0.691 + 10.0 * dmath::log2_f64(e) * std::f64::consts::LOG10_2 } else { f64::NEG_INFINITY };
        let abs: Vec<f64> = energies.into_iter().filter(|&e| l(e) > -70.0).collect();
        if abs.is_empty() {
            return (f64::NEG_INFINITY, peak);
        }
        let rel = l(abs.iter().sum::<f64>() / abs.len() as f64) - 10.0;
        let gated: Vec<f64> = abs.into_iter().filter(|&e| l(e) > rel).collect();
        (l(gated.iter().sum::<f64>() / gated.len().max(1) as f64), peak)
    }

    pub fn notes(&self) -> usize {
        self.events.len()
    }
}

/// A rendered piece as an endless (or one-lap) stream.
pub struct MusicSource {
    piece: Arc<Rendered>,
    pos: u64,
    looping: bool,
}

impl MusicSource {
    /// `looping`: play the first lap, then loop from the loop point forever; otherwise one lap
    /// plus the tails of its last notes.
    pub fn new(piece: Arc<Rendered>, looping: bool) -> MusicSource {
        MusicSource { piece, pos: 0, looping }
    }

    fn end(&self) -> Option<u64> {
        // Played once, a piece ends when its last note has rung out (rests after it, or a take
        // shorter than its last bar, would only hold the stream in silence).
        (!self.looping).then_some(self.piece.ring_out.max(1))
    }
}

impl StreamSource for MusicSource {
    fn name(&self) -> &str {
        &self.piece.name
    }

    fn rate(&self) -> u32 {
        RATE
    }

    fn channels(&self) -> usize {
        2
    }

    fn read(&mut self, out: &mut [f32]) -> Result<usize> {
        let mut n = (out.len() / 2) as u64;
        if let Some(end) = self.end() {
            n = n.min(end.saturating_sub(self.pos));
            if n == 0 {
                return Ok(0);
            }
            // One lap: no wrapping, just the first lap's frames and tails.
            let o = &mut out[..(n * 2) as usize];
            o.fill(0.0);
            self.piece.mix_window(self.pos, o);
            for s in o.iter_mut() {
                *s *= self.piece.gain;
            }
        } else {
            self.piece.mix(self.pos, &mut out[..(n * 2) as usize]);
        }
        self.pos += n;
        Ok(n as usize)
    }

    fn seek(&mut self, frame: u64) -> Result<()> {
        if let Some(end) = self.end() {
            if frame > end {
                return Err(AudioError::new(ErrorCode::SeekOutOfRange, "seek past the end of the piece").with_numbers(frame as f64, end as f64));
            }
        }
        self.pos = frame;
        Ok(())
    }

    fn total_frames(&self) -> Option<u64> {
        self.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BELL: &str = r#"{ "sound": 1, "name": "bell", "duration": 1.5,
        "layers": [ { "source": { "wave": "sine", "freq": 440, "partials": [[1, 1, 1.0], [2.76, 0.3, 0.3]] },
                      "env": { "attack": 0.002, "decay": 1.4, "sustain": 0 } } ] }"#;

    fn piece(extra: &str) -> String {
        format!(
            r#"{{ "music": 1, "name": "test", "tempo": 120, "bars": 2, {extra}
                "instruments": {{ "bell": {{ "sound": {BELL}, "root": "A4", "release": 0.05 }} }},
                "patterns": {{ "a": {{ "bars": 1, "notes": [[0, "A4", 4], [4, "C5", 4, 0.5], [8, 69, 4], [14, "E5", 8]] }} }},
                "tracks": [ {{ "instrument": "bell", "pan": -0.5, "sequence": ["a", "a"] }} ] }}"#
        )
    }

    #[test]
    fn parses_renders_and_is_deterministic() {
        let m = Music::parse(&piece("")).unwrap();
        assert_eq!((m.bars, m.beats_per_bar, m.steps_per_beat), (2, 4, 4));
        assert_eq!(m.lap_frames(), 2 * 2 * RATE as u64); // two bars of 2 s at 120 BPM
        let a = render(&m).unwrap();
        let b = render(&m).unwrap();
        assert_eq!(a.notes(), 8);
        let (mut x, mut y) = (vec![0f32; 4096], vec![0f32; 4096]);
        for at in [0u64, 50_000, 190_000] {
            a.mix(at, &mut x);
            b.mix(at, &mut y);
            assert_eq!(x, y);
        }
        // Panned left: the left channel is louder.
        a.mix(0, &mut x);
        let (l, r) = x.chunks_exact(2).fold((0f32, 0f32), |(l, r), f| (l + f[0].abs(), r + f[1].abs()));
        assert!(l > 1.5 * r);
    }

    #[test]
    fn notes_ring_over_the_loop_point() {
        let m = Music::parse(&piece("")).unwrap();
        let r = render(&m).unwrap();
        let lap = r.lap_frames;
        // The last note (E5 at step 14 of bar 2, 8 steps long) rings past the end: lap 2 starts
        // with its tail added; the first lap doesn't have it.
        let mut first = vec![0f32; 2000];
        let mut second = vec![0f32; 2000];
        r.mix(0, &mut first);
        r.mix(lap, &mut second);
        let diff: f32 = first.iter().zip(&second).map(|(a, b)| (a - b).abs()).sum();
        assert!(diff > 1.0, "the tail is heard at the start of the second lap");
        // And the seam is continuous: the last frames of lap 1 flow into lap 2.
        let mut seam = vec![0f32; 400];
        r.mix(lap - 100, &mut seam);
        let jump = (seam[200] - seam[198]).abs();
        assert!(jump < 0.2, "no click at the seam: {jump}");
    }

    #[test]
    fn loudness_targets_are_met() {
        let m = Music::parse(&piece(r#""loudness": -24,"#)).unwrap();
        let r = render(&m).unwrap();
        let mut out = vec![0f32; (r.lap_frames * 2) as usize];
        r.mix(r.lap_frames, &mut out);
        let lufs = crate::analysis::loudness_lufs(&out, 2, RATE);
        assert!((lufs - -24.0).abs() < 0.7, "{lufs}");
        assert!((r.lufs.unwrap() - -24.0).abs() < 0.01);
    }

    #[test]
    fn errors_say_where() {
        let cases = [
            (piece(r#""tempo2": 1,"#), ErrorCode::UnknownField, "tempo2"),
            (piece("").replace(r#"["a", "a"]"#, r#"["a", "b"]"#), ErrorCode::UnknownName, "tracks[0].sequence[1]"),
            (piece("").replace(r#"["a", "a"]"#, r#"["a", "a", "."]"#), ErrorCode::OutOfRange, "tracks[0].sequence[2]"),
            (piece("").replace(r#""C5""#, r#""H5""#), ErrorCode::UnknownName, "patterns.a.notes[1]"),
            (piece("").replace(r#"[14, "E5", 8]"#, r#"[16, "E5", 8]"#), ErrorCode::OutOfRange, "patterns.a.notes[3]"),
            (piece("").replace(r#""duration": 1.5"#, r#""duration": "x""#), ErrorCode::WrongType, "instruments.bell.sound.duration"),
        ];
        for (text, code, path) in cases {
            let e = Music::parse(&text).unwrap_err();
            assert_eq!((e.code, e.path.as_str()), (code, path), "{e}");
        }
        // Far transpositions are refused when the piece is parsed (the plan checks every note).
        let far = Music::parse(&piece("").replace(r#""E5""#, "127")).unwrap_err();
        assert_eq!((far.code, far.path.as_str()), (ErrorCode::OutOfRange, "patterns.a.notes[3]"));
        assert_eq!(parse_note(&Json::from("Bb2"), "x").unwrap(), 46);
        assert_eq!(parse_note(&Json::from("F#3"), "x").unwrap(), 54);
        assert_eq!(parse_note(&Json::from("C-1"), "x").unwrap(), 0);
        assert_eq!(parse_note(&Json::from("G9"), "x").unwrap(), 127);
        for hostile in ["C2147483647", "C999999999", "G#9", "C-2", "A-2147483648"] {
            assert_eq!(parse_note(&Json::from(hostile), "x").unwrap_err().code, ErrorCode::OutOfRange, "{hostile}");
        }
    }

    /// Review probe: a long instrument played at many pitches and lengths resampled its whole
    /// take per note key (113 s for one pattern). Only what the notes sound is transposed now.
    #[test]
    fn long_instruments_at_many_pitches_render_quickly() {
        let long = r#"{ "sound": 1, "name": "long", "duration": 60,
            "layers": [ { "source": { "wave": "sine", "freq": 440 } } ] }"#;
        let mut notes = Vec::new();
        for (i, semis) in (-36..=36).enumerate() {
            notes.push(format!("[{}, {}, {}]", i, 69 + semis, 1 + i % 4));
        }
        let text = format!(
            r#"{{ "music": 1, "name": "probe", "tempo": 300, "steps_per_beat": 4, "bars": 5,
                "instruments": {{ "i": {{ "sound": {long}, "root": "A4", "release": 0 }} }},
                "patterns": {{ "p": {{ "bars": 5, "notes": [{}] }} }},
                "tracks": [ {{ "instrument": "i", "sequence": ["p"] }} ] }}"#,
            notes.join(",")
        );
        let m = Music::parse(&text).unwrap();
        let start = std::time::Instant::now();
        let r = render(&m).unwrap();
        assert_eq!(r.notes(), 73);
        // The 60 s take renders once (synthesis); the 73 transpositions cost a few notes' worth.
        assert!(r.max_len <= 4 * 2400, "the longest note is 4 steps of 2400 frames: {}", r.max_len);
        assert!(start.elapsed() < std::time::Duration::from_secs(30), "{:?}", start.elapsed());
    }

    const LONG: &str = r#"{ "sound": 1, "name": "long", "duration": 60, "layers": [ { "source": { "wave": "sine", "freq": 440 } } ] }"#;

    fn instruments(n: usize, variations: u32) -> String {
        (0..n).map(|i| format!(r#""i{i}": {{ "sound": {LONG}, "root": "A4", "release": 0, "variations": {variations} }}"#)).collect::<Vec<_>>().join(",")
    }

    /// Review round 2 probe "bases": 16 instruments × 4 variations of a 60 s take synthesised
    /// up front (1.4 GB, 10.6 s for one note). Only takes some note plays are counted and made;
    /// a piece whose notes use them all is refused before any synthesis, naming instruments.
    #[test]
    fn takes_are_costed_and_only_used_takes_are_made() {
        let one_note = format!(
            r#"{{ "music": 1, "name": "bases", "tempo": 120, "bars": 1, "instruments": {{ {} }},
                "patterns": {{ "p": {{ "bars": 1, "notes": [[0, "A4", 4]] }} }},
                "tracks": [ {{ "instrument": "i0", "sequence": ["p"] }} ] }}"#,
            instruments(16, 4)
        );
        let m = Music::parse(&one_note).unwrap();
        assert_eq!(m.cost.take_floats, 60 * RATE as usize * 2, "one take of one instrument: {:?}", m.cost);
        let r = render(&m).unwrap();
        assert_eq!(r.notes(), 1);
        // Every instrument, every variation, played: 64 takes of 60 s.
        let mut notes = Vec::new();
        for i in 0..16 {
            notes.push(format!(r#"{{ "instrument": "i{i}", "sequence": ["p"] }}"#));
        }
        let many = format!(
            r#"{{ "music": 1, "name": "bases", "tempo": 120, "bars": 1, "instruments": {{ {} }},
                "patterns": {{ "p": {{ "bars": 1, "notes": [{}] }} }},
                "tracks": [ {} ] }}"#,
            instruments(16, 4),
            (0..16).map(|s| format!("[{s}, \"A4\", 1]")).collect::<Vec<_>>().join(","),
            notes.join(",")
        );
        let start = std::time::Instant::now();
        let e = Music::parse(&many).unwrap_err();
        assert_eq!((e.code, e.path.as_str()), (ErrorCode::Limit, "instruments"), "{e}");
        assert!(e.message.contains("`i0`"), "names the instruments: {e}");
        assert!(start.elapsed() < specula_core::speed_budget(std::time::Duration::from_secs(1)), "refused before any synthesis");
    }

    /// Review round 3 probe "synth": 16 instruments × 4 variations of a short but dense recipe
    /// (1.3 s, 5 layers × 32 partials, about 0.33 s to render each) stayed under the take-float
    /// limit and took 21 s to make. Synthesis work is costed when the piece is parsed and refused
    /// over budget, naming the instruments.
    #[test]
    fn synthesis_work_is_costed_before_rendering() {
        let parts: Vec<String> = (0..32).map(|k| format!("[{}, 1, 1.0]", k + 1)).collect();
        let layer = format!(r#"{{ "source": {{ "wave": "sine", "freq": 110, "partials": [{}] }} }}"#, parts.join(","));
        let dense = format!(r#"{{ "sound": 1, "name": "c", "duration": 1.3, "vary": {{ "pitch": [-1, 1] }}, "layers": [{}] }}"#, vec![layer; 5].join(","));
        let instruments: Vec<String> = (0..16).map(|i| format!(r#""i{i}": {{ "sound": {dense}, "root": "A4", "variations": 4, "release": 0 }}"#)).collect();
        let tracks: Vec<String> = (0..16).map(|i| format!(r#"{{ "instrument": "i{i}", "sequence": ["p"] }}"#)).collect();
        let piece = |notes: usize| {
            format!(
                r#"{{ "music": 1, "name": "synth", "tempo": 120, "steps_per_beat": 16, "bars": 1, "instruments": {{ {} }},
                    "patterns": {{ "p": {{ "bars": 1, "notes": [{}] }} }}, "tracks": [ {} ] }}"#,
                instruments.join(","),
                (0..notes).map(|k| format!("[{k}, \"A4\", 1]")).collect::<Vec<_>>().join(","),
                tracks.join(",")
            )
        };
        // One note per track: 16 takes of the dense recipe pass under the take-float limit but
        // cost far more than the synthesis budget.
        let start = std::time::Instant::now();
        let e = Music::parse(&piece(64)).unwrap_err();
        assert_eq!((e.code, e.path.as_str()), (ErrorCode::Limit, "instruments"), "{e}");
        assert!(e.message.contains("synthesise"), "says it is the synthesis work: {e}");
        assert!(e.message.contains("`i"), "names the instruments: {e}");
        assert!(start.elapsed() < specula_core::speed_budget(std::time::Duration::from_secs(1)), "refused before any synthesis");
        // A piece within the budget reports its synthesis work and renders.
        let small = format!(
            r#"{{ "music": 1, "name": "ok", "tempo": 120, "bars": 1, "instruments": {{ "a": {{ "sound": {dense}, "root": "A4" }} }},
                "patterns": {{ "p": {{ "bars": 1, "notes": [[0, "A4", 1]] }} }}, "tracks": [ {{ "instrument": "a", "sequence": ["p"] }} ] }}"#
        );
        let m = Music::parse(&small).unwrap();
        assert!(m.cost.synth_work > 0.0 && m.cost.synth_work <= MAX_SYNTH_WORK, "{:?}", m.cost);
    }

    /// Review round 2 probe "dense": 16 tracks × 1024 notes of 16,384 steps on a 60 s take took
    /// 256 s to render and 0.52 s of CPU per second to play. The note-frames a lap sounds are
    /// costed when the piece is parsed and refused over budget, naming the track.
    #[test]
    fn polyphony_is_costed_before_rendering() {
        let notes: Vec<String> = (0..1024).map(|i| format!("[{}, 69, 16384]", i * 3)).collect();
        let tracks: Vec<String> = (0..16).map(|_| r#"{ "instrument": "i0", "sequence": ["p"] }"#.to_string()).collect();
        let dense = format!(
            r#"{{ "music": 1, "name": "dense", "tempo": 20, "steps_per_beat": 16, "bars": 50, "loudness": -20,
                "instruments": {{ {} }},
                "patterns": {{ "p": {{ "bars": 50, "notes": [{}] }} }},
                "tracks": [ {} ] }}"#,
            instruments(1, 1),
            notes.join(","),
            tracks.join(",")
        );
        let start = std::time::Instant::now();
        let e = Music::parse(&dense).unwrap_err();
        assert_eq!(e.code, ErrorCode::Limit, "{e}");
        assert!(e.path.starts_with("tracks"), "{e}");
        assert!(start.elapsed() < specula_core::speed_budget(std::time::Duration::from_secs(2)), "{:?}", start.elapsed());
        // Many short notes at once: refused for voices.
        let chord: Vec<String> = (0..100).map(|_| "[0, 69, 4]".to_string()).collect();
        let wall = format!(
            r#"{{ "music": 1, "name": "wall", "tempo": 60, "bars": 8, "instruments": {{ {} }},
                "patterns": {{ "p": {{ "bars": 1, "notes": [{}] }} }},
                "tracks": [ {{ "instrument": "i0", "sequence": ["p"] }} ] }}"#,
            instruments(1, 1),
            chord.join(",")
        );
        let e = Music::parse(&wall).unwrap_err();
        assert_eq!(e.code, ErrorCode::Limit, "{e}");
        assert!(e.message.contains("notes sound at once"), "{e}");
    }

    /// What a rendered piece does is never more than its cost said: note lengths sum to at most
    /// the counted note-frames and no more notes overlap than counted.
    #[test]
    fn rendering_stays_within_the_cost() {
        let m = Music::parse(&piece("")).unwrap();
        let r = render(&m).unwrap();
        let sum: u64 = r.events.iter().map(|e| e.len).sum();
        assert!(sum <= m.cost.note_frames, "{sum} > {:?}", m.cost);
        let mut edges: Vec<(u64, i32)> = r.events.iter().flat_map(|e| [(e.start, 1), (e.start + e.len, -1)]).collect();
        edges.sort();
        let (mut now, mut peak) = (0, 0);
        for (_, d) in edges {
            now += d;
            peak = peak.max(now);
        }
        assert!(peak as usize <= m.cost.max_voices, "{peak} > {:?}", m.cost);
        assert_eq!(r.samples.len(), m.cost.keys);
        let kept: usize = r.samples.iter().map(|s| s.len()).sum();
        assert!(kept <= m.cost.sample_floats, "{kept} > {:?}", m.cost);
    }

    #[test]
    fn one_lap_sources_end_after_their_tails() {
        let m = Music::parse(&piece("")).unwrap();
        let r = Arc::new(render(&m).unwrap());
        let mut s = MusicSource::new(r.clone(), false);
        let mut buf = vec![0f32; 8192];
        let mut total = 0;
        loop {
            let n = s.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            total += n as u64;
        }
        assert!(total > r.lap_frames && total <= r.lap_frames + RATE as u64 * 2);
        let mut l = MusicSource::new(r, true);
        assert_eq!(l.total_frames(), None);
        assert_eq!(l.read(&mut buf).unwrap(), 4096);
    }
}
