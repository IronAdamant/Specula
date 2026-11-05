//! Vorbis decode speed: real-time factor (seconds of audio decoded per second of CPU) on the
//! test fixtures. `cargo run --release -p specula-audio --example vorbis_bench`
use std::time::Instant;

const FILES: [(&str, &[u8]); 3] = [
    ("stereo_48k q5", include_bytes!("../tests/data/vorbis/stereo_48k.ogg")),
    ("loud_32k q10", include_bytes!("../tests/data/vorbis/loud_32k_q10.ogg")),
    ("surround 5.1 q3", include_bytes!("../tests/data/vorbis/surround_6ch.ogg")),
];

fn main() {
    for (name, bytes) in FILES {
        let mut audio_s = 0.0;
        let start = Instant::now();
        let mut runs = 0;
        while start.elapsed().as_secs_f64() < 1.5 {
            let d = specula_audio::vorbis::decode(bytes.to_vec(), 1 << 24).expect("decodes");
            audio_s += d.samples.len() as f64 / d.info.channels as f64 / d.info.rate as f64;
            runs += 1;
        }
        let cpu = start.elapsed().as_secs_f64();
        println!("{name}: {runs} decodes, {audio_s:.1} s of audio in {cpu:.2} s: {:.0}x real time", audio_s / cpu);
    }
}
