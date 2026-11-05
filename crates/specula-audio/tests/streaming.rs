//! Streams through the whole system (manual backend: exact timing, no device): Vorbis music
//! looping on the music bus, ducking under effects, crossfades, and the meter's numbers.

use specula_audio::meter::TRACKS;
use specula_audio::stream::{SoundSource, VorbisSource};
use specula_audio::{Audio, AudioConfig, Backend, Bus, DuckRule, PlayParams, Sound, StreamParams};

const MUSIC: &[u8] = include_bytes!("data/vorbis/stereo_48k.ogg");

fn manual() -> Audio {
    Audio::start(AudioConfig { backend: Backend::Manual, ..AudioConfig::default() }).unwrap()
}

/// Pulls `secs` of audio in device-sized pieces, servicing the worker in between.
fn run(a: &Audio, secs: f64) -> Vec<f32> {
    let mut all = Vec::new();
    let mut buf = vec![0f32; 480 * 2];
    for _ in 0..(secs * 100.0) as usize {
        a.pull(&mut buf).unwrap();
        a.service();
        all.extend_from_slice(&buf);
    }
    all
}

fn beep(amp: f32, secs: f32) -> Sound {
    let n = (48_000.0 * secs) as usize;
    Sound::new("beep", 48_000, 1, (0..n).map(|i| amp * (i as f32 * 0.07).sin()).collect()).unwrap()
}

#[test]
fn vorbis_music_loops_without_starving_and_is_metered() {
    let a = manual();
    let src = VorbisSource::new("music", MUSIC.to_vec()).unwrap();
    let (_, h) = a.play_stream(Box::new(src), StreamParams { looping: true, gain: 0.5, ..StreamParams::default() }).unwrap();
    let out = run(&a, 3.0);
    let st = h.status();
    assert_eq!(st.starved_frames, 0, "{st:?}");
    assert!(st.loops >= 4, "a 0.6 s file loops at least 4 times in 3 s: {st:?}");
    assert!(st.played_frames >= 140_000, "{st:?}");
    assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    let music = a.meter_report(Bus::Music as usize, 3.0, true);
    assert!(music.integrated_lufs.is_finite() && music.integrated_lufs < -6.0, "{music:?}");
    assert!(music.spectrum.as_ref().unwrap().iter().any(|b| b.1 > -40.0));
    assert_eq!(a.meter_report(Bus::Sfx as usize, 3.0, false).silent_for_s, None, "nothing on sfx");
    let master = a.meter_report(TRACKS - 1, 3.0, false);
    assert_eq!(master.overs, 0);
    assert!((master.integrated_lufs - music.integrated_lufs).abs() < 1.0, "the master is the music: {master:?}");
    let s = a.stats();
    assert_eq!((s.stream_starved_frames, s.clipped_samples, s.non_finite_samples), (0, 0, 0));
}

#[test]
fn music_ducks_under_effects() {
    let a = manual();
    let tone = beep(0.3, 10.0);
    let (_, _) = a.play_stream(Box::new(SoundSource::new(tone.clone())), StreamParams { looping: true, ..StreamParams::default() }).unwrap();
    a.set_duck(0, Some(DuckRule { source: Bus::Sfx, target: Bus::Music, depth_db: 10.0, threshold_db: -40.0, attack_s: 0.02, release_s: 0.3 })).unwrap();
    run(&a, 1.0);
    let before = a.meter_report(Bus::Music as usize, 0.4, false).momentary_lufs;
    a.play(&beep(0.5, 2.0), PlayParams { bus: Bus::Sfx, ..PlayParams::default() }).unwrap();
    run(&a, 1.0);
    let ducked = a.meter_report(Bus::Music as usize, 0.4, false).momentary_lufs;
    assert!(before - ducked > 8.0, "music ducks ~10 dB under sfx: {before} -> {ducked}");
    // The 2 s effect ends, the bus level follower falls under the threshold (~1.2 s) and the
    // duck releases.
    run(&a, 3.5);
    let after = a.meter_report(Bus::Music as usize, 0.4, false).momentary_lufs;
    assert!((after - before).abs() < 0.5, "and comes back: {before} -> {after}");
}

#[test]
fn crossfade_hands_over_and_finishes_the_old_stream() {
    let a = manual();
    let (old, old_h) = a.play_stream(Box::new(SoundSource::new(beep(0.3, 10.0))), StreamParams::default()).unwrap();
    run(&a, 0.5);
    let (new, new_h) = a.crossfade(Some(old), Box::new(VorbisSource::new("next", MUSIC.to_vec()).unwrap()), StreamParams { looping: true, ..StreamParams::default() }, 0.5).unwrap();
    run(&a, 1.0);
    let finished = a.drain_finished();
    assert!(finished.contains(&old) && !finished.contains(&new), "{finished:?}");
    assert!(old_h.status().finished);
    assert!(new_h.status().started && new_h.status().starved_frames == 0);
    assert_eq!(a.stats().active_streams, 1);
}

#[test]
fn a_stopped_stream_is_released_on_both_sides() {
    let a = manual();
    let (id, h) = a.play_stream(Box::new(VorbisSource::new("m", MUSIC.to_vec()).unwrap()), StreamParams { looping: true, ..StreamParams::default() }).unwrap();
    run(&a, 0.3);
    a.stop(id, 0.05).unwrap();
    run(&a, 0.3);
    assert!(a.drain_finished().contains(&id));
    assert!(h.status().finished);
    assert_eq!(a.stats().active_streams, 0);
    // Streams past the cap are refused by the mixer and reported finished, not leaked.
    let mut ids = Vec::new();
    for _ in 0..10 {
        ids.push(a.play_stream(Box::new(SoundSource::new(beep(0.1, 5.0))), StreamParams::default()).unwrap().0);
        run(&a, 0.01);
    }
    run(&a, 0.1);
    assert_eq!(a.stats().active_streams, 8);
    assert_eq!(a.stats().streams_rejected, 2);
    let done = a.drain_finished();
    assert_eq!(ids.iter().filter(|i| done.contains(i)).count(), 2);
}
