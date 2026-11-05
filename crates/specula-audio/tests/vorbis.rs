//! The Ogg Vorbis decoder against ffmpeg's own decode of the fixtures in `data/vorbis/` (made
//! from signals we generate, see `make_fixtures.py`), plus seeking and a mutation fuzzer.

use specula_audio::ogg;
use specula_audio::vorbis::{self, Decoder};
use specula_audio::wav;
use specula_audio::ErrorCode;

const FIXTURES: [(&str, &[u8], &[u8]); 6] = [
    ("sweep_mono_22k", include_bytes!("data/vorbis/sweep_mono_22k.ogg"), include_bytes!("data/vorbis/sweep_mono_22k.ref.wav")),
    ("stereo_48k", include_bytes!("data/vorbis/stereo_48k.ogg"), include_bytes!("data/vorbis/stereo_48k.ref.wav")),
    ("noise_44k_q0", include_bytes!("data/vorbis/noise_44k_q0.ogg"), include_bytes!("data/vorbis/noise_44k_q0.ref.wav")),
    ("surround_6ch", include_bytes!("data/vorbis/surround_6ch.ogg"), include_bytes!("data/vorbis/surround_6ch.ref.wav")),
    ("stereo_44k_ffenc", include_bytes!("data/vorbis/stereo_44k_ffenc.ogg"), include_bytes!("data/vorbis/stereo_44k_ffenc.ref.wav")),
    ("loud_32k_q10", include_bytes!("data/vorbis/loud_32k_q10.ogg"), include_bytes!("data/vorbis/loud_32k_q10.ref.wav")),
];

/// The reference is 16-bit (quantisation error 1.5e-5); allow a little more for float order.
const TOLERANCE: f32 = 1.0e-4;

/// ffmpeg reorders channels to WAVE order; ours are in Vorbis order. `map[ours] = ffmpeg's`.
fn ffmpeg_channel(channels: usize, c: usize) -> usize {
    if channels == 6 {
        // Vorbis FL C FR RL RR LFE -> WAVE FL FR C LFE BL BR.
        [0, 2, 1, 4, 5, 3][c]
    } else {
        c
    }
}

#[test]
fn decodes_every_fixture_like_ffmpeg() {
    for (name, ogg_bytes, ref_bytes) in FIXTURES {
        let reference = wav::decode(ref_bytes).unwrap();
        let d = vorbis::decode(ogg_bytes.to_vec(), 1 << 22).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(d.info.channels, reference.channels, "{name}: channels");
        assert_eq!(d.info.rate, reference.rate, "{name}: rate");
        let ch = d.info.channels as usize;
        let ours = d.samples.len() / ch;
        // We return exactly the frames the last granule counts (ffmpeg's lengths differ by up
        // to a block: it ends on its own idea of the last packet).
        assert_eq!(d.info.total_frames, Some(ours as u64), "{name}: we return exactly the last granule's frames");
        let theirs = reference.samples.len() / ch;
        let overlap = ours.min(theirs);
        assert!(overlap * 10 >= theirs * 9, "{name}: only {overlap} of {theirs} frames compared");
        let (mut worst, mut at) = (0f32, 0);
        for f in 0..overlap {
            for c in 0..ch {
                let a = d.samples[f * ch + c].clamp(-1.0, 32767.0 / 32768.0); // ffmpeg clips to 16 bits
                let b = reference.samples[f * ch + ffmpeg_channel(ch, c)];
                if (a - b).abs() > worst {
                    worst = (a - b).abs();
                    at = f;
                }
            }
        }
        assert!(worst <= TOLERANCE, "{name}: worst difference {worst} at frame {at}");
        assert_eq!(d.stats.bad_packets, 0, "{name}");
        assert!(d.samples.iter().any(|s| s.abs() > 0.05), "{name}: not silent");
    }
}

#[test]
fn headers_are_read() {
    let d = Decoder::new(FIXTURES[1].1.to_vec()).unwrap();
    let info = d.info();
    assert_eq!((info.channels, info.rate), (2, 48_000));
    assert!(info.vendor.contains("Xiph") || info.vendor.contains("Lavf") || !info.vendor.is_empty(), "{}", info.vendor);
    assert!(info.blocksizes[0] < info.blocksizes[1]);
}

/// Seeking anywhere and reading on gives exactly what a straight decode gives there.
#[test]
fn seeking_is_sample_exact() {
    for (name, bytes, _) in FIXTURES {
        let full = vorbis::decode(bytes.to_vec(), 1 << 22).unwrap();
        let ch = full.info.channels as usize;
        let total = full.samples.len() / ch;
        let mut d = Decoder::new(bytes.to_vec()).unwrap();
        for target in [0, 1, 777, total / 3, total / 2 + 13, total.saturating_sub(300), total - 1, total] {
            d.seek(target as u64).unwrap();
            assert_eq!(d.position(), target as u64);
            let mut buf = vec![0f32; 256 * ch];
            let n = d.read(&mut buf).unwrap();
            assert_eq!(n, 256.min(total - target), "{name}: frames after seeking to {target}");
            let want = &full.samples[target * ch..(target + n) * ch];
            for (i, (a, b)) in buf[..n * ch].iter().zip(want).enumerate() {
                assert!((a - b).abs() < 1e-5, "{name}: seek {target}, sample {i}: {a} vs {b}");
            }
            assert_eq!(d.position(), (target + n) as u64);
        }
        assert_eq!(d.seek(total as u64 + 1).unwrap_err().code, ErrorCode::SeekOutOfRange);
    }
}

/// Reading in odd-sized pieces gives the same samples as one big read.
#[test]
fn chunked_reads_match() {
    let (_, bytes, _) = FIXTURES[1];
    let full = vorbis::decode(bytes.to_vec(), 1 << 22).unwrap();
    let mut d = Decoder::new(bytes.to_vec()).unwrap();
    let mut got = Vec::new();
    let mut sizes = [1usize, 7, 333, 4096, 2].into_iter().cycle();
    loop {
        let mut buf = vec![0f32; sizes.next().unwrap() * 2];
        let n = d.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        got.extend_from_slice(&buf[..n * 2]);
    }
    assert_eq!(got, full.samples);
}

/// Recomputes every page CRC so mutated bodies reach the Vorbis parser instead of being
/// rejected by the container.
fn fix_crcs(b: &mut [u8]) {
    let mut at = 0;
    while at + 27 <= b.len() && &b[at..at + 4] == b"OggS" {
        let segs = b[at + 26] as usize;
        if at + 27 + segs > b.len() {
            return;
        }
        let body: usize = b[at + 27..at + 27 + segs].iter().map(|&l| l as usize).sum();
        let len = 27 + segs + body;
        if at + len > b.len() {
            return;
        }
        b[at + 22..at + 26].fill(0);
        let c = ogg::crc32(&b[at..at + len]);
        b[at + 22..at + 26].copy_from_slice(&c.to_le_bytes());
        at += len;
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn exercise(bytes: Vec<u8>) {
    // Hostile input may fail, but must fail with a structured error, never panic.
    if let Ok(mut d) = Decoder::new(bytes) {
        let ch = d.channels();
        let mut buf = vec![0f32; 1024 * ch];
        let mut frames = 0usize;
        while let Ok(n) = d.read(&mut buf) {
            if n == 0 || frames > 1 << 21 {
                break;
            }
            assert!(buf[..n * ch].iter().all(|s| s.is_finite()), "hostile streams decode to finite samples");
            frames += n;
        }
        let _ = d.seek(frames as u64 / 2);
        let _ = d.read(&mut buf);
    }
}

/// Seeds kept as tests: byte flips, bit flips, runs of 0x00 / 0xff, truncations and splices,
/// on every fixture, with CRCs repaired so the damage reaches the codec.
#[test]
fn hostile_bytes_never_panic() {
    let mut rng = Rng(0x005e_ed0f_0661);
    for (_, bytes, _) in FIXTURES {
        for round in 0..300 {
            let mut b = bytes.to_vec();
            match round % 6 {
                0 => {
                    for _ in 0..1 + rng.below(8) {
                        let i = rng.below(b.len());
                        b[i] = rng.next() as u8;
                    }
                }
                1 => {
                    let i = rng.below(b.len());
                    b[i] ^= 1 << rng.below(8);
                }
                2 => {
                    let i = rng.below(b.len());
                    let n = rng.below(64).min(b.len() - i);
                    let v = if rng.next() & 1 == 0 { 0 } else { 0xff };
                    b[i..i + n].fill(v);
                }
                3 => {
                    let n = rng.below(b.len());
                    b.truncate(n);
                }
                4 => {
                    // Damage only the setup header region (the first few KB), where the tables are.
                    let i = 30 + rng.below(b.len().min(4000) - 30);
                    b[i] = rng.next() as u8;
                }
                _ => {
                    let (i, j) = (rng.below(b.len()), rng.below(b.len()));
                    let piece = b[j..(j + rng.below(200)).min(b.len())].to_vec();
                    b.splice(i..i, piece);
                }
            }
            if round % 3 != 2 {
                fix_crcs(&mut b);
            }
            exercise(b);
        }
    }
    // Degenerate inputs.
    for b in [vec![], b"OggS".to_vec(), vec![0; 100], vec![0xff; 100]] {
        exercise(b);
    }
    // Comments: loop points and lengths at every extreme (review: LOOPSTART=u64::MAX with
    // LOOPLENGTH=1 overflowed).
    let values = ["0", "1", "18446744073709551615", "18446744073709551614", "9223372036854775808", "-1", "1e9", "", "99999999999999999999"];
    let (_, bytes, _) = FIXTURES[1];
    for round in 0..120 {
        let pick = |rng: &mut Rng| values[rng.below(values.len())];
        let comments = [format!("LOOPSTART={}", pick(&mut rng)), format!("LOOPEND={}", pick(&mut rng)), format!("LOOPLENGTH={}", pick(&mut rng))];
        let keep = 1 + round % 3;
        let refs: Vec<&str> = comments.iter().take(keep).map(String::as_str).collect();
        exercise(with_comments(bytes, &refs));
    }
}

/// Review probe: valid headers then a megabyte of OggS patterns took 14.7 s and decoded to 0
/// frames. The resync budget gives up fast, as a Limit error.
#[test]
fn ogg_spam_after_the_headers_is_refused_quickly() {
    let (_, bytes, _) = FIXTURES[1];
    let mut r = ogg::PacketReader::new(bytes.to_vec().into()).unwrap();
    let mut p = Vec::new();
    for _ in 0..3 {
        r.next_packet(&mut p).unwrap();
    }
    let audio_at = r.next_packet(&mut p).unwrap().unwrap().page_offset;
    let mut b = bytes[..audio_at].to_vec();
    for _ in 0..(1 << 15) {
        b.extend_from_slice(b"OggS");
        b.push(0);
        b.extend_from_slice(&[0xff; 27]);
    }
    let start = std::time::Instant::now();
    let e = vorbis::decode(b.clone(), 1 << 22).unwrap_err();
    assert_eq!(e.code, ErrorCode::Limit, "{e}");
    assert!(start.elapsed() < std::time::Duration::from_secs(5), "{:?}", start.elapsed());
    // Seeking into it is bounded too.
    let mut d = Decoder::new(b).unwrap();
    let start = std::time::Instant::now();
    let _ = d.seek(1_000_000);
    assert!(start.elapsed() < std::time::Duration::from_secs(5), "{:?}", start.elapsed());
}

/// Granules near u64::MAX (hostile pages) never overflow the decoder's position arithmetic.
#[test]
fn hostile_granules_never_overflow() {
    for g in [u64::MAX, u64::MAX - 1, u64::MAX / 2, 1] {
        let (_, bytes, _) = FIXTURES[1];
        let mut b = bytes.to_vec();
        let mut at = 0;
        while let Ok(p) = ogg::parse_page(&b, at) {
            let len = p.len;
            if at > 0 {
                b[at + 6..at + 14].copy_from_slice(&g.to_le_bytes());
            }
            at += len;
        }
        fix_crcs(&mut b);
        if let Ok(mut d) = Decoder::new(b) {
            let mut buf = vec![0f32; 4096 * d.channels()];
            while let Ok(n) = d.read(&mut buf) {
                if n == 0 {
                    break;
                }
            }
            let _ = d.seek(12_345);
            let _ = d.read(&mut buf);
        }
    }
}

/// One Ogg page holding whole `packets` (each under 64 KiB), with `flags` and `granule`.
fn page(flags: u8, granule: u64, serial: u32, seq: u32, packets: &[&[u8]]) -> Vec<u8> {
    let mut lacing = Vec::new();
    let mut body = Vec::new();
    for p in packets {
        let mut n = p.len();
        loop {
            let l = n.min(255);
            lacing.push(l as u8);
            n -= l;
            if l < 255 {
                break;
            }
        }
        body.extend_from_slice(p);
    }
    assert!(lacing.len() <= 255, "test packets fit one page");
    let mut out = b"OggS".to_vec();
    out.push(0);
    out.push(flags);
    out.extend_from_slice(&granule.to_le_bytes());
    out.extend_from_slice(&serial.to_le_bytes());
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.push(lacing.len() as u8);
    out.extend_from_slice(&lacing);
    out.extend_from_slice(&body);
    let c = ogg::crc32(&out);
    out[22..26].copy_from_slice(&c.to_le_bytes());
    out
}

/// A fixture with its comment header replaced by one holding `comments`.
fn with_comments(bytes: &[u8], comments: &[&str]) -> Vec<u8> {
    let mut r = ogg::PacketReader::new(bytes.to_vec().into()).unwrap();
    let mut id = Vec::new();
    let mut setup = Vec::new();
    let mut scratch = Vec::new();
    r.next_packet(&mut id).unwrap();
    r.next_packet(&mut scratch).unwrap();
    r.next_packet(&mut setup).unwrap();
    let audio_at = r.next_packet(&mut scratch).unwrap().unwrap().page_offset;
    let serial = r.serial();
    let mut c = b"\x03vorbis".to_vec();
    let vendor = b"test";
    c.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    c.extend_from_slice(vendor);
    c.extend_from_slice(&(comments.len() as u32).to_le_bytes());
    for k in comments {
        c.extend_from_slice(&(k.len() as u32).to_le_bytes());
        c.extend_from_slice(k.as_bytes());
    }
    c.push(1);
    let mut out = page(ogg::FLAG_BOS, 0, serial, 0, &[&id]);
    out.extend(page(0, 0, serial, 1, &[&c, &setup]));
    out.extend_from_slice(&bytes[audio_at..]);
    out
}

/// Loop comments are kept only when they make sense, and hostile ones never overflow (review
/// probe: LOOPSTART=u64::MAX with LOOPLENGTH=1 panicked).
#[test]
fn loop_comments_are_validated() {
    let (_, bytes, _) = FIXTURES[1];
    let total = Decoder::new(bytes.to_vec()).unwrap().info().total_frames.unwrap();
    type Case<'a> = (&'a [&'a str], Option<(u64, u64)>);
    let cases: [Case; 6] = [
        (&["LOOPSTART=1000", "LOOPEND=5000"], Some((1000, 5000))),
        (&["LOOPSTART=1000", "LOOPLENGTH=500"], Some((1000, 1500))),
        (&["LOOPSTART=18446744073709551615", "LOOPLENGTH=1"], None),
        (&["LOOPSTART=5000", "LOOPEND=1000"], None),
        (&["LOOPSTART=0", "LOOPEND=99999999999"], None),
        (&["LOOPSTART=-3", "LOOPLENGTH=x"], None),
    ];
    for (comments, want) in cases {
        let d = Decoder::new(with_comments(bytes, comments)).unwrap_or_else(|e| panic!("{comments:?}: {e}"));
        let got = d.info().loop_start.zip(d.info().loop_end);
        assert_eq!(got, want, "{comments:?}");
        assert_eq!(d.info().total_frames, Some(total));
    }
    // The rebuilt stream still decodes like the original.
    let a = vorbis::decode(with_comments(bytes, &["TITLE=x"]), 1 << 22).unwrap();
    assert_eq!(a.samples, vorbis::decode(bytes.to_vec(), 1 << 22).unwrap().samples);
}

#[test]
fn errors_are_structured() {
    assert_eq!(Decoder::new(b"RIFFxxxxWAVE".to_vec()).err().unwrap().code, ErrorCode::OggNotOgg);
    let (_, bytes, _) = FIXTURES[0];
    // Just the first page (identification header): the comment header is missing.
    let first = ogg::parse_page(bytes, 0).unwrap().len;
    let e = Decoder::new(bytes[..first].to_vec()).err().unwrap();
    assert_eq!(e.code, ErrorCode::VorbisBadHeader, "{e}");
    // A decode cap is a Limit error, not an allocation.
    assert_eq!(vorbis::decode(bytes.to_vec(), 100).unwrap_err().code, ErrorCode::Limit);
}
