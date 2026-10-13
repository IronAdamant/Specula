//! Sample-rate conversion: an 8-tap Kaiser-windowed sinc interpolator with a polyphase table.
//!
//! # Why windowed sinc, not linear
//!
//! Every voice is resampled (sound rate → device rate, times its pitch), so this runs 64× per
//! output frame. Linear interpolation is cheapest, but its images are only ~-25..-30 dB down for
//! content above a few kHz and it audibly dulls the top octave: exactly the range of Sepulcra's
//! zaps, shrieks and crickets. An 8-tap windowed sinc keeps the passband flat to ~16 kHz at 48 kHz
//! and pushes images/aliases to ~-60 dB, for 8 multiply-adds per sample (plus 8 to interpolate
//! between table phases) — measured at well under the F3 budget (see `examples/mix_bench.rs`).
//!
//! When a voice is pitched up (step > 1) the source must also be band-limited below the output
//! Nyquist, or it aliases. [`SincTable`] holds kernels for several cutoffs and picks the one that
//! fits the step, so a sound played an octave up is filtered at half band.

use specula_core::dmath;

/// Taps per output sample (4 either side of the read position).
pub const TAPS: usize = 8;
/// Phases per input sample; coefficients are linearly interpolated between neighbours.
pub const PHASES: usize = 128;
/// Kaiser β: ~60 dB side lobes with this length.
const KAISER_BETA: f64 = 6.0;
/// Kernel cutoffs as a fraction of the input Nyquist frequency, widest first.
const CUTOFFS: [f64; 8] = [0.92, 0.80, 0.70, 0.60, 0.50, 0.42, 0.35, 0.28];

/// Fixed-point position: 32 fractional bits.
pub const FRAC_BITS: u32 = 32;
pub const ONE: u64 = 1 << FRAC_BITS;

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let q = x * x / 4.0;
    for k in 1..40 {
        term *= q / (k as f64 * k as f64);
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

/// Polyphase kernels for each cutoff: `[cutoff][phase 0..=PHASES][tap]`.
pub struct SincTable {
    coeffs: Box<[f32]>,
}

impl Default for SincTable {
    fn default() -> Self {
        SincTable::new()
    }
}

impl SincTable {
    pub fn new() -> SincTable {
        let half = (TAPS / 2) as f64;
        let i0_beta = bessel_i0(KAISER_BETA);
        let mut coeffs = Vec::with_capacity(CUTOFFS.len() * (PHASES + 1) * TAPS);
        for &fc in &CUTOFFS {
            for p in 0..=PHASES {
                let frac = p as f64 / PHASES as f64;
                let mut taps = [0f64; TAPS];
                let mut sum = 0.0;
                for (k, tap) in taps.iter_mut().enumerate() {
                    // Tap k reads input sample (n - 3 + k); its distance from the read point n + frac.
                    let d = (k as f64 - (half - 1.0)) - frac;
                    let x = d * fc;
                    let sinc = if x.abs() < 1e-12 { 1.0 } else { dmath::sin_f64(std::f64::consts::PI * x) / (std::f64::consts::PI * x) };
                    let r = d / half;
                    let w = if r.abs() >= 1.0 { 0.0 } else { bessel_i0(KAISER_BETA * (1.0 - r * r).sqrt()) / i0_beta };
                    *tap = sinc * w;
                    sum += *tap;
                }
                // Unity DC gain at every phase (no amplitude ripple as the phase moves).
                coeffs.extend(taps.iter().map(|t| (t / sum) as f32));
            }
        }
        SincTable { coeffs: coeffs.into_boxed_slice() }
    }

    /// The kernel index to use for a read step (input samples per output sample, 32.32).
    #[inline]
    pub fn kernel_for_step(step: u64) -> usize {
        // Needed cutoff (fraction of input Nyquist) = 0.92 / step; pick the widest kernel at or
        // below it.
        let ratio = step as f64 / ONE as f64;
        if ratio <= 1.0 {
            return 0;
        }
        let want = CUTOFFS[0] / ratio;
        CUTOFFS.iter().position(|&c| c <= want).unwrap_or(CUTOFFS.len() - 1)
    }

    /// Coefficients for fractional position `frac` (0..2³², 32 fractional bits) with `kernel`.
    #[inline]
    pub fn coeffs(&self, kernel: usize, frac: u32) -> [f32; TAPS] {
        let scaled = frac as u64 * PHASES as u64;
        let p = (scaled >> 32) as usize;
        let t = (scaled & 0xffff_ffff) as f32 * (1.0 / 4_294_967_296.0);
        let base = (kernel * (PHASES + 1) + p) * TAPS;
        let a = &self.coeffs[base..base + TAPS];
        let b = &self.coeffs[base + TAPS..base + 2 * TAPS];
        let mut out = [0f32; TAPS];
        for k in 0..TAPS {
            out[k] = a[k] + t * (b[k] - a[k]);
        }
        out
    }
}

/// Offline conversion of interleaved `samples` from `from` Hz to `to` Hz (whole buffer, no
/// looping). Used for imported WAVs and tests; the mixer resamples voices on the fly.
pub fn resample(samples: &[f32], channels: usize, from: u32, to: u32) -> Vec<f32> {
    let channels = channels.max(1);
    let frames = samples.len() / channels;
    if from == to || frames == 0 {
        return samples[..frames * channels].to_vec();
    }
    let table = SincTable::new();
    let step = ((from as u128) << FRAC_BITS) / to as u128;
    let step = step as u64;
    let kernel = SincTable::kernel_for_step(step);
    let out_frames = ((frames as u128 * to as u128) / from as u128) as usize;
    let mut out = Vec::with_capacity(out_frames * channels);
    let mut pos: u64 = 0;
    for _ in 0..out_frames {
        let idx = (pos >> FRAC_BITS) as i64;
        let c = table.coeffs(kernel, pos as u32);
        for ch in 0..channels {
            let mut acc = 0.0;
            for (k, ck) in c.iter().enumerate() {
                let i = idx - 3 + k as i64;
                if i >= 0 && (i as usize) < frames {
                    acc += ck * samples[i as usize * channels + ch];
                }
            }
            out.push(acc);
        }
        pos += step;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, rate: f64, n: usize, phase0: f64) -> Vec<f32> {
        (0..n).map(|i| dmath::sin_f64(std::f64::consts::TAU * freq * (i as f64 / rate) + phase0) as f32).collect()
    }

    #[test]
    fn phase_zero_of_widest_kernel_peaks_at_the_read_point() {
        let t = SincTable::new();
        let c = t.coeffs(0, 0);
        let sum: f32 = c.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
        assert!(c[3] > 0.9, "{c:?}");
        // Half-way between samples the kernel is symmetric about taps 3 and 4.
        let h = t.coeffs(0, 1 << 31);
        for k in 0..TAPS {
            assert!((h[k] - h[TAPS - 1 - k]).abs() < 1e-5, "{h:?}");
        }
    }

    #[test]
    fn kernel_choice_follows_step() {
        assert_eq!(SincTable::kernel_for_step(ONE), 0);
        assert_eq!(SincTable::kernel_for_step(ONE / 2), 0);
        assert!(SincTable::kernel_for_step(ONE * 2) >= 4);
        assert_eq!(SincTable::kernel_for_step(ONE * 8), CUTOFFS.len() - 1);
    }

    /// 44.1 kHz → 48 kHz of a 1 kHz sine matches the ideal 48 kHz sine to better than -60 dB.
    #[test]
    fn upsample_accuracy() {
        let input = sine(1000.0, 44_100.0, 44_100, 0.0);
        let out = resample(&input, 1, 44_100, 48_000);
        assert_eq!(out.len(), 48_000);
        let ideal = sine(1000.0, 48_000.0, 48_000, 0.0);
        let (mut err, mut sig) = (0.0f64, 0.0f64);
        // Skip the edges (the kernel reads zeros outside the buffer).
        for i in 100..47_900 {
            let e = (out[i] - ideal[i]) as f64;
            err += e * e;
            sig += (ideal[i] as f64).powi(2);
        }
        let snr = 10.0 * (sig / err).log10();
        assert!(snr > 60.0, "SNR {snr:.1} dB");
    }

    #[test]
    fn high_tone_accuracy_and_downsample() {
        // 8 kHz at 48k → 44.1k: still clean (passband).
        let input = sine(8000.0, 48_000.0, 48_000, 0.3);
        let out = resample(&input, 1, 48_000, 44_100);
        let ideal = sine(8000.0, 44_100.0, out.len(), 0.3);
        let (mut err, mut sig) = (0.0f64, 0.0f64);
        for i in 100..out.len() - 100 {
            err += ((out[i] - ideal[i]) as f64).powi(2);
            sig += (ideal[i] as f64).powi(2);
        }
        let snr = 10.0 * (sig / err).log10();
        assert!(snr > 50.0, "SNR {snr:.1} dB");
    }

    #[test]
    fn stereo_channels_stay_separate() {
        let mut st = Vec::new();
        for i in 0..1000 {
            st.push(1.0);
            st.push(if i % 2 == 0 { 0.5 } else { -0.5 } * 0.0);
        }
        let out = resample(&st, 2, 22_050, 48_000);
        for f in out.chunks_exact(2).skip(10).take(out.len() / 2 - 20) {
            assert!((f[0] - 1.0).abs() < 1e-3 && f[1].abs() < 1e-6);
        }
    }
}
