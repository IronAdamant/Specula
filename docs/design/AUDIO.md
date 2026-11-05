# Audio: output, streaming, music as data, and hearing for agents

Status: M5 (lane m5-audio, 2026-11-06). Code: `crates/specula-audio` (output, mixer, synthesis,
Vorbis, streaming, music, meter), `crates/specula/src/audio_service.rs` and `audio_tools.rs` (the
engine's service and the `audio_*` tools), `games/sepulcra-app/src/audio.rs` (the game's use).

Agents can't listen. Specula's answer is the same as for pictures: sound is **made from data**
(recipes and music pieces), **checked as numbers** the engine measures itself, and **reported**
with who played what. Nothing about audio feeds back into the simulation, so `world_hash`,
replays and headless runs never depend on it.

## Signal path

```text
game / agent ──play──▶ AudioService (engine) ──commands (SPSC ring)──▶ mixer (audio thread)
                          │  play log, streams, settings                 │ voices, streams
                          │                                              ▼
music piece / .ogg ──▶ worker thread: decode, loop, resample ──blocks──▶ buses ─▶ master ─▶ device
                          ▲                                              │ tap (every block)
                          └────────── meter (worker thread) ◀────────────┘
```

- **Audio thread** (WASAPI on Windows, a null thread elsewhere or when asked): mixes voices and
  streams into six buses (`master`, `sfx`, `ui`, `ambience`, `music`, `voice`), applies bus gains
  and duck rules, sums into the master and a look-ahead limiter. It never allocates, locks or
  frees: commands arrive on a bounded ring, finished sounds and streams go back on another.
- **Worker thread** (`specula-audio-worker`, one per `Audio`): fills streams' rings and measures
  the mixer's tap. Starvation, stream errors and a meter that fell behind are traced from here
  (`audio.stream_starved`, `audio.stream_error`, `audio.meter_behind`, subsystem `audio`).
- **Engine thread**: pushes commands (cheap); starting a stream also allocates its 16 ring blocks
  (about 128 KB, no waiting). Decodes, renders, resampler tables and measurements never run on
  it: the one sinc table streams share is built when audio starts.

## Streaming

`Audio::play_stream(source, params)` streams any `StreamSource` (`VorbisSource`, `MusicSource`,
`SoundSource`). The worker reads blocks of 1024 frames, applies loop points (`StreamParams`
`looping`, `loop_start`, `loop_end`; a looping stream whose params set none loops where the
source says: an Ogg file's `LOOPSTART` and `LOOPEND` / `LOOPLENGTH` comments, kept only when the
start is before the end and the end inside the file),
resamples to the mix rate with the mixer's 8-tap sinc, and pushes them into a ring of 16
preallocated blocks (about 0.34 s). A stream starts after 4 blocks are buffered. If the ring runs
dry the mixer plays **silence** for the missing frames and counts them (`starved_frames`,
`starve_events`); it never repeats stale samples. `Audio::crossfade(from, source, params, s)`
fades one in while the other fades out. At most 8 streams play at once; more are refused and
reported finished.

## Ogg Vorbis

`specula_audio::ogg` reads pages (CRC checked, lacing, packets across pages, damaged pages
skipped and counted); `specula_audio::vorbis` decodes Vorbis I by hand: codebooks (lookup types 1
and 2, a 10-bit fast Huffman table), floors 0 and 1, residues 0, 1 and 2, channel coupling, the
inverse MDCT through an N/4-point FFT, every long/short window combination and overlap-add.
Granule positions trim leading samples the first page doesn't count and the end past the last
page's granule, as libvorbis does (`vorbis_synthesis_blockin`), including its rule that a stream
whose first audio page is also its last is cut at the end. Seeking bisects over pages by granule,
resumes on that page and decodes forward to the exact frame.

- **Tests:** six fixtures made from signals we generate (`tests/data/vorbis/make_fixtures.py`,
  encoded by ffmpeg's libvorbis and its own Vorbis encoder: mono, stereo, 5.1, low and high
  quality, transients) decode within 1e-4 of ffmpeg's own decode; seeking anywhere matches a
  straight decode exactly; a mutation fuzzer (byte and bit flips, runs, truncations, splices,
  CRCs repaired so damage reaches the codec) never panics.
- **Speed:** about 300x real time for 48 kHz stereo at quality 5 (`examples/vorbis_bench.rs`).
- **Limits:** 8 channels, codebooks of at most 2^20 entries, 2^23 table values per setup header,
  packets of 4 MiB; resyncing after damage may check at most 4 MiB of pages (CRC work), then the
  stream ends with a `limit` error (a file full of `OggS` patterns can't cost seconds); positions
  from hostile granules saturate instead of overflowing. Uploads (`project_upload` to `audio/`,
  `sounds/` or `music/`) must decode whole to some audio, to at most 30 minutes of stereo at
  48 kHz counted in samples (frames × channels), within 20 s of checking, and at most 768,000
  samples a second (8 channels at 96 kHz), the most a stream decodes.

## Music as data

`specula_audio::music`: a pattern sequencer agents compose. Instruments are sound recipes played
as samplers: rendered once at their `root` note (only the takes some note plays), each take
transposed once per pitch (only as far as its longest note sounds) and gated with a linear
release as notes mix. Notes are `C4`-style names in octaves -1..=9 or MIDI numbers;
patterns are bars of notes `[step, note, length, velocity]`, tracks play sequences of patterns
(`"."` rests a bar). A piece loops from `loop_from_bar` and notes ringing past the end carry
into the next lap, so loops are seamless. With `loudness`, one lap is measured (BS.1770, gated)
and the piece scaled to meet it, never past -1 dBFS peak. Rendering is the costly part (about
1.2 s for Sepulcra's map 1 theme in a release build) and runs on a worker; mixing notes as they
stream is cheap. Pieces are project files (`music/*.json`, kind `music`), validated on write.

**One cost model, checked before any work** (`music::Cost`, computed by `Music::from_json`, so a
`project_write`, an `audio_preview` or a hot reload refuses a piece before it renders anything):

| Cost | Limit | Bounds |
|---|---|---|
| `take_floats`: instrument takes synthesised (only those some note plays) | 8M floats (about 87 s of stereo) | synthesis memory |
| `synth_work`: estimated CPU to synthesise those takes (each take is one recipe render) | 4 × a recipe's own limit (about 1.2 s of one core) | synthesis time |
| `sample_floats`: transposed takes kept, each as long as its longest note | 16M floats (64 MB) | resampling and memory |
| `keys`: distinct (instrument, take, pitch) | 512 | resample calls |
| `note_frames`: frames of notes sounding in one lap | 32 per lap frame, 200M in all | rendering's loudness pass and mixing a lap |
| `max_voices`: notes sounding at once | 64 | mixing one frame (live, on the worker) |

A piece over a limit is refused with a `limit` error naming the instruments or tracks; render,
measure and live mixing never do more than the model counted (`rendering_stays_within_the_cost`).
`audio_preview` reports a piece's cost. Sepulcra's map 1 sits far inside it.

## The engine's audio service

The engine opens audio once (`--audio device|null|off`; device with a window, **off headless**
so headless runs stay silent; `null` mixes in real time with no device, so a headless engine can
still be heard by the tools). A hosted game receives it in `Game::use_audio` and plays through it,
naming itself; `AudioService` records every play (name, bus, gain, position, who, when, on the
audio clock: seconds mixed) in a log of 4096, remembers streams, and records who set bus gains
and duck rules.

## Hearing tools

All four answer on the caller's connection thread; none touches the world or the engine thread
(`audio_preview` asks the engine only for a project file's snapshot).

| Tool | Access | What it answers |
|---|---|---|
| `audio_played` | read | what played in the last 0.1..600 s: name, bus, who, when (detailed: gain, voice, position, duration); counts by name |
| `audio_levels` | read | per bus and `master_out`: momentary, short-term and gated integrated LUFS over a window, sample and true peak, overs, audible and silent time, octave bands; streams and their starvation; totals |
| `audio_preview` | open | renders a recipe or piece, or decodes an `.ogg`/`.wav`, offline: LUFS, peaks, centroid, clipping, bands, a piece's cost; optional WAV and picture in `captures/audio/`. Capped in samples: at most 120 s of stereo at 48 kHz (rates above 48 kHz count as 48 kHz; channels fold to stereo as they decode), two at once |
| `audio_settings` | lead to change | bus gains and duck rules, and who set them |

The meter follows ITU-R BS.1770-4: K-weighting, 400 ms blocks with 75 % overlap kept as 100 ms
sub-blocks for ten minutes, absolute (-70 LUFS) and relative (-10 LU) gates; true peak by 4x
oversampling (a 48-tap Kaiser-windowed sinc). A full-scale 997 Hz sine in both channels reads
0.0 LUFS and 0.0 dBFS (tested). Silence is reported as `null`, never `-inf`.

## Sepulcra

`games/sepulcra/assets/music/`: `map1.json` (an A minor nocturne, 16 bars at 66 BPM, about 58 s a
lap, -29 LUFS, about 5 LU under the effects as heard: drone, choir, glass bells and a heartbeat pulse), `victory.json` and `defeat.json`
(four-bar cues). The game sets three duck rules at start (music under voice by 12 dB, under
effects by 4 dB, ambience under voice by 6 dB), plays the boss call (`sounds/boss_call.json`)
and the victory and defeat stings on the voice bus, and crossfades the theme into the cues. A new
run brings the theme back. Edited sounds and pieces hot-reload; a playing piece crossfades to its
new render.

## Seams

- **Timeline** (`m5-timeline`): a cue track calls `AudioService::play` / `play_stream` with the
  timeline's name as `by`, and reads `audio_played` to check it fired.
- **Accessibility** (subtitles and captions): the play log is the source of sound events (name,
  bus, position, time); captions can map names to text.
