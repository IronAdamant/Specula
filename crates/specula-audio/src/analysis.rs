//! Eyes on sound: numbers and pictures, because agents can't hear.
//!
//! - [`analyze`] → [`Metrics`]: peak, RMS, integrated loudness (LUFS-ish), duration, spectral
//!   centroid, time of the peak, DC offset, clipped samples, and how long the sound is "active".
//! - [`sound_panel`] → an image: title, waveform (min/max + RMS envelope) over a log-frequency
//!   spectrogram, and a metrics line. [`contact_sheet`] tiles panels; [`encode_png`] writes PNG.
//!
//! **Loudness** follows ITU-R BS.1770-4: K-weighting (the standard's two biquads, derived for
//! any rate as in libebur128), mean square summed over channels with weight 1, 400 ms blocks
//! with 75 % overlap, an absolute gate at -70 LUFS and a relative gate at -10 LU. Sounds shorter
//! than one block are measured as a single block. It's "LUFS-ish" because mono files are not
//! given the +3 dB a dual-mono playback would add, and very short one-shots are not what the
//! standard was designed for; it is still the right number to level a sound set with.

use crate::dsp::{self, Biquad, PI};
use specula_core::deflate::Level;
use specula_core::dmath;
use specula_core::Json;

/// Measurements of a sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub duration_s: f64,
    pub peak_dbfs: f64,
    pub rms_dbfs: f64,
    /// Integrated loudness (BS.1770 K-weighted, gated), LUFS.
    pub lufs: f64,
    /// Energy-weighted mean frequency of the spectrum (Hz): "brightness".
    pub centroid_hz: f64,
    pub peak_time_s: f64,
    pub dc_offset: f64,
    /// Samples at or beyond full scale.
    pub clipped: usize,
    /// Time from the first to the last 10 ms window louder than -40 dBFS (RMS).
    pub active_s: f64,
}

impl Metrics {
    pub fn to_json(&self) -> Json {
        let r = |v: f64, d: i32| {
            if v.is_finite() {
                let p = 10f64.powi(d);
                Json::from((v * p).round() / p)
            } else {
                Json::Null
            }
        };
        Json::obj([
            ("duration_s", r(self.duration_s, 3)),
            ("peak_dbfs", r(self.peak_dbfs, 2)),
            ("rms_dbfs", r(self.rms_dbfs, 2)),
            ("lufs", r(self.lufs, 2)),
            ("centroid_hz", r(self.centroid_hz, 0)),
            ("peak_time_s", r(self.peak_time_s, 3)),
            ("dc_offset", r(self.dc_offset, 5)),
            ("clipped", Json::from(self.clipped)),
            ("active_s", r(self.active_s, 3)),
        ])
    }
}

fn db(power_or_amp: f64, power: bool) -> f64 {
    if power_or_amp <= 0.0 {
        return f64::NEG_INFINITY;
    }
    let l = dmath::log2_f64(power_or_amp) * std::f64::consts::LOG10_2;
    if power {
        10.0 * l
    } else {
        20.0 * l
    }
}

/// The two K-weighting biquads for `rate`.
fn k_weighting(rate: u32) -> (Biquad, Biquad) {
    let fs = rate as f64;
    let (f0, g, q) = (1_681.974_450_955_533, 3.999_843_853_973_347, 0.707_175_236_955_419_6);
    let k = dmath::tan_f64(PI * f0 / fs);
    let vh = dsp::db_to_gain(g);
    let vb = dmath::exp2_f64(dmath::log2_f64(vh) * 0.499_666_774_154_541_6);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad::new(
        [(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0],
        [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    );
    let (f0, q) = (38.135_470_876_024_44, 0.500_327_037_323_877_3);
    let k = dmath::tan_f64(PI * f0 / fs);
    let d = 1.0 + k / q + k * k;
    let hp = Biquad::new([1.0, -2.0, 1.0], [2.0 * (k * k - 1.0) / d, (1.0 - k / q + k * k) / d]);
    (shelf, hp)
}

/// Integrated loudness in LUFS (`-inf` for silence). See the module docs.
pub fn loudness_lufs(samples: &[f32], channels: usize, rate: u32) -> f64 {
    let channels = channels.max(1);
    let frames = samples.len() / channels;
    if frames == 0 {
        return f64::NEG_INFINITY;
    }
    // K-weighted squared signal, summed over channels, per frame.
    let mut power = vec![0f64; frames];
    for c in 0..channels {
        let (mut s1, mut s2) = k_weighting(rate);
        for (f, p) in power.iter_mut().enumerate() {
            let y = s2.process(s1.process(samples[f * channels + c] as f64));
            *p += y * y;
        }
    }
    let block = (rate as usize * 4 / 10).max(1);
    let step = (block / 4).max(1);
    let mut blocks = Vec::new();
    if frames <= block {
        blocks.push(power.iter().sum::<f64>() / frames as f64);
    } else {
        // Prefix sums make each block O(1).
        let mut prefix = Vec::with_capacity(frames + 1);
        prefix.push(0.0);
        let mut acc = 0.0;
        for p in &power {
            acc += p;
            prefix.push(acc);
        }
        let mut s = 0;
        while s + block <= frames {
            blocks.push((prefix[s + block] - prefix[s]) / block as f64);
            s += step;
        }
    }
    let lufs = |p: f64| -0.691 + db(p, true);
    let abs: Vec<f64> = blocks.into_iter().filter(|&p| lufs(p) > -70.0).collect();
    if abs.is_empty() {
        return f64::NEG_INFINITY;
    }
    let mean = abs.iter().sum::<f64>() / abs.len() as f64;
    let rel = lufs(mean) - 10.0;
    let gated: Vec<f64> = abs.into_iter().filter(|&p| lufs(p) > rel).collect();
    lufs(gated.iter().sum::<f64>() / gated.len() as f64)
}

/// In-place radix-2 complex FFT (`re.len()` must be a power of two).
pub fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    debug_assert!(n.is_power_of_two() && im.len() == n);
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let (ws, wc) = dmath::sin_cos_f64(-2.0 * PI / len as f64);
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wc - ci * ws;
                ci = cr * ws + ci * wc;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

/// Mono downmix.
fn mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    samples.chunks_exact(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect()
}

/// Short-time spectrum: magnitude in dBFS per bin (a full-scale sine reads ≈ 0 dB), frames
/// centred every `hop` samples.
pub struct Stft {
    pub size: usize,
    pub hop: usize,
    pub rate: u32,
    /// `frames × (size / 2 + 1)` dB values.
    pub db: Vec<f32>,
    pub frames: usize,
}

pub fn stft(samples: &[f32], channels: usize, rate: u32, size: usize, hop: usize) -> Stft {
    let x = mono(samples, channels);
    let size = size.next_power_of_two().max(16);
    let hop = hop.max(1);
    let window: Vec<f64> = (0..size).map(|i| 0.5 - 0.5 * dmath::cos_f64(2.0 * PI * i as f64 / size as f64)).collect();
    let wsum: f64 = window.iter().sum();
    let frames = x.len().div_ceil(hop).max(1);
    let bins = size / 2 + 1;
    let mut out = Vec::with_capacity(frames * bins);
    let (mut re, mut im) = (vec![0f64; size], vec![0f64; size]);
    for fr in 0..frames {
        let centre = (fr * hop) as i64;
        for i in 0..size {
            let idx = centre - (size / 2) as i64 + i as i64;
            re[i] = if idx >= 0 && (idx as usize) < x.len() { x[idx as usize] as f64 * window[i] } else { 0.0 };
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        for k in 0..bins {
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt() / (wsum / 2.0);
            out.push(db(mag, false).max(-140.0) as f32);
        }
    }
    Stft { size, hop, rate, db: out, frames }
}

/// Measures a sound (interleaved samples).
pub fn analyze(samples: &[f32], channels: usize, rate: u32) -> Metrics {
    let channels = channels.max(1);
    let frames = samples.len() / channels;
    let rate_f = rate.max(1) as f64;
    let (mut peak, mut peak_i, mut sq, mut sum, mut clipped) = (0f32, 0usize, 0f64, 0f64, 0usize);
    for (i, &s) in samples.iter().enumerate() {
        let a = s.abs();
        if a > peak {
            peak = a;
            peak_i = i / channels;
        }
        if a >= 1.0 {
            clipped += 1;
        }
        sq += (s as f64) * (s as f64);
        sum += s as f64;
    }
    let n = samples.len().max(1) as f64;
    // Active span: 10 ms RMS windows above -40 dBFS.
    let win = (rate as usize / 100).max(1);
    let (mut first, mut last) = (None, 0);
    for (w, chunk) in samples.chunks(win * channels).enumerate() {
        let p = chunk.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / chunk.len().max(1) as f64;
        if db(p, true) > -40.0 {
            first.get_or_insert(w);
            last = w;
        }
    }
    let active_s = first.map_or(0.0, |f| ((last - f + 1) * win) as f64 / rate_f);
    // Spectral centroid over a 2048-point STFT (energy weighted across all frames).
    let st = stft(samples, channels, rate, 2048, 1024);
    let bins = st.size / 2 + 1;
    let (mut num, mut den) = (0f64, 0f64);
    for fr in 0..st.frames {
        for k in 1..bins {
            let d = st.db[fr * bins + k] as f64;
            if d <= -120.0 {
                continue;
            }
            let p = dsp::db_to_gain(d) * dsp::db_to_gain(d);
            num += p * (k as f64 * rate_f / st.size as f64);
            den += p;
        }
    }
    Metrics {
        duration_s: frames as f64 / rate_f,
        peak_dbfs: db(peak as f64, false),
        rms_dbfs: db(sq / n, true),
        lufs: loudness_lufs(samples, channels, rate),
        centroid_hz: if den > 0.0 { num / den } else { 0.0 },
        peak_time_s: peak_i as f64 / rate_f,
        dc_offset: sum / n,
        clipped,
        active_s,
    }
}

/// An RGBA8 image.
#[derive(Clone, Debug, PartialEq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32, bg: [u8; 3]) -> Canvas {
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        for _ in 0..width as usize * height as usize {
            rgba.extend_from_slice(&[bg[0], bg[1], bg[2], 255]);
        }
        Canvas { width, height, rgba }
    }

    pub fn set(&mut self, x: i64, y: i64, c: [u8; 3]) {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.rgba[i..i + 3].copy_from_slice(&c);
    }

    pub fn rect(&mut self, x: i64, y: i64, w: i64, h: i64, c: [u8; 3]) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, c);
            }
        }
    }

    /// Draws ASCII text with the built-in 5×7 font (lowercase shown as capitals), `scale` ≥ 1.
    pub fn text(&mut self, x: i64, y: i64, s: &str, c: [u8; 3], scale: i64) {
        let mut cx = x;
        for ch in s.chars() {
            let g = glyph(ch.to_ascii_uppercase());
            for (row, bits) in g.iter().enumerate() {
                for col in 0..5 {
                    if bits & (0x10 >> col) != 0 {
                        self.rect(cx + col * scale, y + row as i64 * scale, scale, scale, c);
                    }
                }
            }
            cx += 6 * scale;
        }
    }

    pub fn blit(&mut self, src: &Canvas, x: i64, y: i64) {
        for sy in 0..src.height as i64 {
            for sx in 0..src.width as i64 {
                let i = (sy as usize * src.width as usize + sx as usize) * 4;
                self.set(x + sx, y + sy, [src.rgba[i], src.rgba[i + 1], src.rgba[i + 2]]);
            }
        }
    }
}

/// PNG bytes for a canvas.
pub fn encode_png(c: &Canvas) -> Vec<u8> {
    specula_core::png::encode_rgba8(c.width, c.height, &c.rgba, Level::Default)
}

const BG: [u8; 3] = [0x0b, 0x10, 0x20];
const PANEL: [u8; 3] = [0x12, 0x18, 0x2a];
const GRID: [u8; 3] = [0x2a, 0x34, 0x48];
const WAVE: [u8; 3] = [0x3f, 0xe0, 0xd0];
const WAVE_RMS: [u8; 3] = [0xb8, 0xff, 0xf6];
const TEXT: [u8; 3] = [0xec, 0xe6, 0xd6];
const DIM: [u8; 3] = [0x8c, 0x96, 0xa3];
const WARN: [u8; 3] = [0xff, 0x7a, 0x1f];

/// Inferno-like ramp, t in 0..1.
fn heat(t: f32) -> [u8; 3] {
    const STOPS: [[f32; 3]; 6] = [[0.0, 0.0, 4.0], [40.0, 11.0, 84.0], [101.0, 21.0, 110.0], [188.0, 55.0, 84.0], [249.0, 142.0, 9.0], [252.0, 255.0, 164.0]];
    let t = t.clamp(0.0, 1.0) * (STOPS.len() - 1) as f32;
    let i = (t as usize).min(STOPS.len() - 2);
    let f = t - i as f32;
    let (a, b) = (STOPS[i], STOPS[i + 1]);
    [(a[0] + (b[0] - a[0]) * f) as u8, (a[1] + (b[1] - a[1]) * f) as u8, (a[2] + (b[2] - a[2]) * f) as u8]
}

/// Lowest frequency drawn in spectrograms.
pub const SPEC_MIN_HZ: f64 = 40.0;
/// Spectrogram colour range (dBFS).
pub const SPEC_FLOOR_DB: f32 = -96.0;

/// A panel: `title`, waveform, log-frequency spectrogram (40 Hz – 20 kHz) and metrics.
pub fn sound_panel(title: &str, samples: &[f32], channels: usize, rate: u32, width: u32, metrics: &Metrics) -> Canvas {
    let (wave_h, spec_h) = (72i64, 112i64);
    let height = 14 + wave_h + 2 + spec_h + 24;
    let mut c = Canvas::new(width, height as u32, PANEL);
    let w = width as i64;
    c.text(4, 3, title, TEXT, 1);
    let dur = format!("{:.2}S", metrics.duration_s);
    c.text(w - 4 - 6 * dur.len() as i64, 3, &dur, DIM, 1);

    // Waveform.
    let y0 = 14;
    c.rect(0, y0, w, wave_h, BG);
    let mid = y0 + wave_h / 2;
    let x = mono(samples, channels);
    let frames = x.len().max(1);
    // Time grid: every 0.1 s, brighter every second.
    let secs = frames as f64 / rate as f64;
    let step = if secs > 4.0 { 1.0 } else { 0.1 };
    let mut t = step;
    while t < secs {
        let gx = (t / secs * w as f64) as i64;
        let major = (t / 1.0 - dmath::round_f64(t)).abs() < 1e-6 && t >= 1.0;
        c.rect(gx, y0, 1, wave_h, if major { DIM } else { GRID });
        t += step;
    }
    c.rect(0, mid, w, 1, GRID);
    for px in 0..w {
        let a = px as usize * frames / w as usize;
        let b = ((px as usize + 1) * frames / w as usize).max(a + 1).min(frames);
        if a >= x.len() {
            break;
        }
        let (mut lo, mut hi, mut sq) = (0f32, 0f32, 0f64);
        for &s in &x[a..b] {
            lo = lo.min(s);
            hi = hi.max(s);
            sq += (s * s) as f64;
        }
        let rms = (sq / (b - a) as f64).sqrt() as f32;
        let half = (wave_h / 2 - 1) as f32;
        let ya = mid - (hi.min(1.0) * half) as i64;
        let yb = mid - (lo.max(-1.0) * half) as i64;
        c.rect(px, ya, 1, (yb - ya).max(1), WAVE);
        let r = (rms.min(1.0) * half) as i64;
        c.rect(px, mid - r, 1, 2 * r + 1, WAVE_RMS);
        if hi >= 1.0 || lo <= -1.0 {
            c.rect(px, y0, 1, 2, WARN);
        }
    }

    // Spectrogram.
    let sy0 = y0 + wave_h + 2;
    let st = stft(samples, channels, rate, 1024, (frames / w as usize).clamp(16, 4096));
    let bins = st.size / 2 + 1;
    let fmax = (rate as f64 / 2.0).min(20_000.0);
    let ratio = fmax / SPEC_MIN_HZ;
    let bin_hz = rate as f64 / st.size as f64;
    for py in 0..spec_h {
        // Row → frequency band (top = high).
        let f_hi = SPEC_MIN_HZ * dmath::pow((ratio) as f32, (1.0 - py as f64 / spec_h as f64) as f32) as f64;
        let f_lo = SPEC_MIN_HZ * dmath::pow((ratio) as f32, (1.0 - (py + 1) as f64 / spec_h as f64) as f32) as f64;
        let k_lo = ((f_lo / bin_hz) as usize).min(bins - 1);
        let k_hi = ((f_hi / bin_hz).ceil() as usize).clamp(k_lo, bins - 1);
        for px in 0..w {
            let fr = (px as usize * st.frames / w as usize).min(st.frames - 1);
            let row = &st.db[fr * bins..(fr + 1) * bins];
            let v = row[k_lo..=k_hi].iter().fold(f32::NEG_INFINITY, |m, &d| m.max(d));
            c.set(px, sy0 + py, heat((v - SPEC_FLOOR_DB) / -SPEC_FLOOR_DB));
        }
    }
    // Frequency guides at 100 Hz, 1 kHz and 10 kHz.
    for (f, label) in [(100.0, "100"), (1000.0, "1K"), (10_000.0, "10K")] {
        if f < fmax {
            let py = ((1.0 - (dmath::log2_f64(f / SPEC_MIN_HZ) / dmath::log2_f64(ratio))) * spec_h as f64) as i64;
            for px in (0..w).step_by(4) {
                c.set(px, sy0 + py, GRID);
            }
            c.text(2, sy0 + py - 8, label, DIM, 1);
        }
    }

    // Metrics.
    let my = sy0 + spec_h + 4;
    let line1 = format!(
        "PEAK {:.1} RMS {:.1} LUFS {:.1}",
        metrics.peak_dbfs.max(-99.0),
        metrics.rms_dbfs.max(-99.0),
        metrics.lufs.max(-99.0),
    );
    let line2 = format!("CENTROID {:.0}HZ  PEAK AT {:.2}S  ACTIVE {:.2}S", metrics.centroid_hz, metrics.peak_time_s, metrics.active_s);
    c.text(4, my, &line1, TEXT, 1);
    c.text(4, my + 10, &line2, DIM, 1);
    if metrics.clipped > 0 {
        c.text(w - 4 - 6 * 4, my, "CLIP", WARN, 1);
    }
    c
}

/// Tiles panels into a grid with `columns` columns and a header line.
pub fn contact_sheet(header: &str, panels: &[Canvas], columns: usize) -> Canvas {
    let columns = columns.max(1);
    let pw = panels.iter().map(|p| p.width).max().unwrap_or(1) as i64;
    let ph = panels.iter().map(|p| p.height).max().unwrap_or(1) as i64;
    let rows = panels.len().div_ceil(columns) as i64;
    let gap = 6;
    let top = 24;
    let width = columns as i64 * (pw + gap) + gap;
    let height = top + rows * (ph + gap) + gap;
    let mut c = Canvas::new(width as u32, height as u32, BG);
    c.text(gap, 6, header, TEXT, 2);
    for (i, p) in panels.iter().enumerate() {
        let (col, row) = ((i % columns) as i64, (i / columns) as i64);
        c.blit(p, gap + col * (pw + gap), top + row * (ph + gap));
    }
    c
}

fn glyph(c: char) -> [u8; 7] {
    match c {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 2, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 17, 25, 21, 19, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 17, 10, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [31, 2, 4, 2, 1, 17, 14],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 30, 1, 1, 17, 14],
        '6' => [6, 8, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 2, 12],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        '%' => [24, 25, 2, 4, 8, 19, 3],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        '=' => [0, 0, 31, 0, 31, 0, 0],
        ',' => [0, 0, 0, 0, 12, 4, 8],
        '#' => [10, 10, 31, 10, 31, 10, 10],
        '*' => [0, 4, 21, 14, 21, 4, 0],
        '?' => [14, 17, 1, 2, 4, 0, 4],
        _ => [0; 7],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, rate: u32, secs: f64, amp: f64) -> Vec<f32> {
        (0..(rate as f64 * secs) as usize).map(|i| (amp * dmath::sin_f64(2.0 * PI * freq * i as f64 / rate as f64)) as f32).collect()
    }

    #[test]
    fn fft_finds_a_bin() {
        let n = 256;
        let mut re: Vec<f64> = (0..n).map(|i| dmath::cos_f64(2.0 * PI * 10.0 * i as f64 / n as f64)).collect();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im);
        let mags: Vec<f64> = re.iter().zip(&im).map(|(r, i)| (r * r + i * i).sqrt()).collect();
        assert!((mags[10] - 128.0).abs() < 1e-9 && (mags[246] - 128.0).abs() < 1e-9);
        assert!(mags.iter().enumerate().all(|(k, &m)| k == 10 || k == 246 || m < 1e-9));
    }

    /// BS.1770 reference point: a 0 dBFS 997 Hz sine on one channel reads -3.01 LUFS.
    #[test]
    fn loudness_reference_tone() {
        for rate in [44_100, 48_000] {
            let s = sine(997.0, rate, 5.0, 1.0);
            let l = loudness_lufs(&s, 1, rate);
            assert!((l + 3.01).abs() < 0.05, "{rate}: {l}");
        }
        // -20 dB → -23.01; stereo (both channels) → +3 dB.
        let s = sine(997.0, 48_000, 3.0, 0.1);
        assert!((loudness_lufs(&s, 1, 48_000) + 23.01).abs() < 0.05);
        let st: Vec<f32> = s.iter().flat_map(|&x| [x, x]).collect();
        assert!((loudness_lufs(&st, 2, 48_000) + 20.0).abs() < 0.05);
        assert_eq!(loudness_lufs(&vec![0.0; 48_000], 1, 48_000), f64::NEG_INFINITY);
        // Gating drops the silence around a tone; the 6 blocks straddling its edges carry 3 blocks'
        // worth of its power, so the result is 10·log10(20/23) = -0.61 dB below the tone.
        let mut gap = vec![0.0f32; 96_000];
        gap.extend(sine(997.0, 48_000, 2.0, 0.1));
        gap.extend(vec![0.0f32; 96_000]);
        assert!((loudness_lufs(&gap, 1, 48_000) + 23.62).abs() < 0.05, "{}", loudness_lufs(&gap, 1, 48_000));
    }

    #[test]
    fn metrics_of_a_tone() {
        let s = sine(1000.0, 48_000, 1.0, 0.5);
        let m = analyze(&s, 1, 48_000);
        assert!((m.peak_dbfs + 6.02).abs() < 0.01);
        assert!((m.rms_dbfs + 9.03).abs() < 0.01);
        assert!((m.centroid_hz - 1000.0).abs() < 30.0, "{}", m.centroid_hz);
        assert!((m.duration_s - 1.0).abs() < 1e-9 && m.dc_offset.abs() < 1e-4 && m.clipped == 0);
        assert!((m.active_s - 1.0).abs() < 0.02);
    }

    #[test]
    fn panel_and_sheet_render() {
        let s = sine(440.0, 48_000, 0.5, 0.5);
        let m = analyze(&s, 1, 48_000);
        let p = sound_panel("tone_440", &s, 1, 48_000, 320, &m);
        let sheet = contact_sheet("TEST", &[p.clone(), p], 2);
        assert_eq!(sheet.width, 2 * (320 + 6) + 6);
        let png = encode_png(&sheet);
        let back = specula_core::png::decode(&png).unwrap();
        assert_eq!((back.width, back.height), (sheet.width, sheet.height));
    }
}
