//! How slow this machine is right now, against its own best: the gate's `speed` step scales the
//! budgets of the tests it runs by it.
//!
//! A fixed budget cannot tell a loaded machine from dearer code: one test read 220 ms quiet and
//! 467, 565 and 1,254 ms with other builds beside the gate. So the step first runs a reference
//! workload (single-threaded, deterministic, about what the speed tests do: integer and float
//! arithmetic over a few megabytes), keeps the least time it ever took in this checkout
//! (`<target>/gate/speed-reference.txt`: not tracked, and machine-specific by where it lives),
//! and before each package's run measures it again. The ratio, floored at 1, is the factor the
//! gate hands the tests in `SPECULA_SPEED_TESTS` (`specula_core::speed_budget` multiplies by
//! it). Past [`CAP`] the step fails without running anything: a budget scaled eightfold tells
//! nothing, and a gate must not pass by luck.

use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Past this factor the machine is too loaded to judge speed.
pub const CAP: f64 = 8.0;

/// 4 MiB of `u64`s.
const WORDS: usize = 512 * 1024;
/// Passes over the buffer: the integer checksum and the float loop each run this many.
const PASSES: usize = 8;
/// How many times [`reference_ms`] times the workload (it returns the median).
const RUNS: usize = 7;

/// The reference workload: a buffer filled by a fixed generator, an integer checksum and a
/// floating-point loop over it, a few passes each. Its result is the same on every run (the
/// time is what varies); it is passed through `black_box` so it can't be optimised away.
pub fn workload() -> u64 {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut buf = vec![0_u64; WORDS];
    for w in &mut buf {
        // splitmix64
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        *w = z ^ (z >> 31);
    }
    let buf = std::hint::black_box(buf);
    let mut sum = 0_u64;
    for pass in 0..PASSES as u64 {
        // A chain each step of which needs the one before: memory and multiplies, not SIMD.
        let mut h = pass;
        for &w in &buf {
            h = (h ^ w).wrapping_mul(0x0000_0100_0000_01B3).rotate_left(29);
        }
        sum = sum.wrapping_add(h);
    }
    let mut acc = 0.0_f64;
    for pass in 0..PASSES {
        let k = 1.0 + pass as f64 * 1e-3;
        for &w in &buf {
            let x = (w >> 11) as f64 * (1.0 / (1_u64 << 53) as f64);
            acc = acc * 0.999_999 + x * k;
        }
    }
    std::hint::black_box(sum ^ acc.to_bits())
}

/// The workload timed [`RUNS`] times: the median, in milliseconds.
pub fn reference_ms() -> f64 {
    let mut times: Vec<f64> = (0..RUNS)
        .map(|_| {
            let t = Instant::now();
            std::hint::black_box(workload());
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    times.sort_by(f64::total_cmp);
    times[RUNS / 2]
}

/// How many times slower `now` is than `best`: at least 1, rounded up to two decimals. A best
/// that isn't a positive number gives 1 (nothing to compare with).
pub fn factor(now: f64, best: f64) -> f64 {
    if !(best > 0.0 && best.is_finite() && now.is_finite()) {
        return 1.0;
    }
    let raw = (now / best).max(1.0);
    // (The small step down keeps a ratio of exactly 3.4, which reads 3.4000000000000004 in
    // floats, at 3.40.)
    ((raw * 100.0 - 1e-6).ceil() / 100.0).max(1.0)
}

/// Whether a factor is past what a gate can judge speed through.
pub fn too_loaded(factor: f64) -> bool {
    factor > CAP
}

/// The best kept at `path`: the first number in the file.
pub fn stored(path: &Path) -> Option<f64> {
    let text = std::fs::read_to_string(path).ok()?;
    let ms: f64 = text.split_whitespace().next()?.parse().ok()?;
    (ms > 0.0 && ms.is_finite()).then_some(ms)
}

/// The machine's best after a measurement of `now_ms`: the stored best, lowered to `now_ms` if
/// it beat it (and written), or `now_ms` itself when nothing usable was stored (and written).
/// Never raised. A write that fails is said and the measurement still counts for this run.
pub fn remember(path: &Path, now_ms: f64) -> f64 {
    match stored(path) {
        Some(best) if best <= now_ms => best,
        _ => {
            let text = format!("{now_ms:.3} {}\n", today());
            let wrote = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, text));
            if let Err(e) = wrote {
                println!("  NOTE: the machine's best reference could not be kept in {}: {e}", path.display());
            }
            now_ms
        }
    }
}

/// Today's date (UTC) as `YYYY-MM-DD`.
fn today() -> String {
    let days = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86_400);
    let (y, m, d) = civil(days as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 as (year, month, day) in the proleptic Gregorian calendar.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_factor_is_floored_at_one_rounded_up_and_capped() {
        // (now, best, factor, too loaded)
        let table = [
            (30.0, 30.0, 1.0, false),
            (20.0, 30.0, 1.0, false),
            (102.0, 30.0, 3.4, false),
            (105.0, 31.0, 3.39, false),
            (30.3, 30.0, 1.01, false),
            (30.01, 30.0, 1.01, false),
            (240.0, 30.0, 8.0, false),
            (240.3, 30.0, 8.01, true),
            (900.0, 30.0, 30.0, true),
            (50.0, 0.0, 1.0, false),
            (50.0, f64::NAN, 1.0, false),
            (f64::INFINITY, 30.0, 1.0, false),
        ];
        for (now, best, want, loaded) in table {
            let f = factor(now, best);
            assert!((f - want).abs() < 1e-9, "factor({now}, {best}) = {f}, not {want}");
            assert_eq!(too_loaded(f), loaded, "factor {f}");
        }
    }

    #[test]
    fn the_best_is_lowered_and_never_raised() {
        let dir = std::env::temp_dir().join(format!("examen-mensura-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("gate").join("speed-reference.txt");
        // Missing: today's measurement becomes the best, and the folder and file are made.
        assert_eq!(remember(&path, 40.0), 40.0);
        assert_eq!(stored(&path), Some(40.0));
        assert_eq!(factor(40.0, remember(&path, 40.0)), 1.0);
        // Slower: the best stays.
        assert_eq!(remember(&path, 55.0), 40.0);
        assert_eq!(stored(&path), Some(40.0));
        // Faster: the best is lowered.
        assert_eq!(remember(&path, 31.5), 31.5);
        assert_eq!(stored(&path), Some(31.5));
        let text = std::fs::read_to_string(&path).unwrap();
        let date = text.split_whitespace().nth(1).unwrap_or("");
        assert!(date.len() == 10 && date.as_bytes()[4] == b'-' && date.as_bytes()[7] == b'-', "{text:?}");
        // Unreadable: the measurement counts as the best (a factor of 1) and is written.
        for junk in ["", "fast\n", "-3 2026-11-28\n", "NaN\n"] {
            std::fs::write(&path, junk).unwrap();
            assert_eq!(stored(&path), None, "{junk:?}");
            assert_eq!(remember(&path, 70.0), 70.0);
            assert_eq!(stored(&path), Some(70.0), "{junk:?} was not written over");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_workload_gives_the_same_result_every_run() {
        let first = workload();
        assert_eq!(workload(), first);
        assert_eq!(workload(), first);
        assert_ne!(first, 0);
    }

    #[test]
    fn days_become_dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(11_016), (2000, 2, 29));
        assert_eq!(civil(20_785), (2026, 11, 28));
    }
}
