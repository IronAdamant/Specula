//! How slow this machine is right now, against its own best: the gate's `speed` step scales the
//! budgets of the tests it runs by it.
//!
//! A fixed budget cannot tell a loaded machine from dearer code: one test read 220 ms quiet and
//! 467, 565 and 1,254 ms with other builds beside the gate. So the gate times a reference
//! workload (single-threaded, deterministic, about what the speed tests do: integer and float
//! arithmetic over a few megabytes) [`RUNS`] times at its start and again before each package's
//! speed run. "Now" is the median of a set; the best is the least single sample this checkout
//! has ever taken, this run's included (under a load that comes and goes, the fastest run is
//! near what the machine does unloaded). The best is kept in `<target>/gate/speed-reference.txt`
//! (not tracked, and machine-specific by where it lives) with the day it was last lowered and
//! the day the file was first written. The ratio, floored at 1, is the factor the gate hands the
//! tests in `SPECULA_SPEED_TESTS` (`specula_core::speed_budget` multiplies by it). Past [`CAP`]
//! the step fails without running anything: a budget scaled eightfold tells nothing, and a gate
//! must not pass by luck. A best from this run, or from a file under [`YOUNG_DAYS`] old, is said
//! to be young: it may not have seen the machine quiet, so the factor may be too low.

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Past this factor the machine is too loaded to judge speed.
pub const CAP: f64 = 8.0;
/// A best kept for fewer days than this is said to be young.
pub const YOUNG_DAYS: i64 = 3;

/// 4 MiB of `u64`s.
const WORDS: usize = 512 * 1024;
/// Passes over the buffer: the integer checksum and the float loop each run this many.
const PASSES: usize = 8;
/// How many times one measurement times the workload.
pub const RUNS: usize = 15;

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

/// The workload timed [`RUNS`] times, in milliseconds, in the order taken.
pub fn samples() -> Vec<f64> {
    (0..RUNS)
        .map(|_| {
            let t = Instant::now();
            std::hint::black_box(workload());
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect()
}

/// The middle of a set (the upper middle of an even one); infinity for an empty one.
pub fn median(set: &[f64]) -> f64 {
    let mut s = set.to_vec();
    s.sort_by(f64::total_cmp);
    s.get(s.len() / 2).copied().unwrap_or(f64::INFINITY)
}

/// The least of a set; infinity for an empty one.
pub fn least(set: &[f64]) -> f64 {
    set.iter().copied().filter(|t| t.is_finite()).fold(f64::INFINITY, f64::min)
}

/// The best over a stored best (if any) and the least sample of this run.
pub fn best_over(stored: Option<f64>, run_least: f64) -> f64 {
    stored.map_or(run_least, |s| s.min(run_least))
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

/// What the factor's line adds when the best is young: from this run, or kept for fewer than
/// [`YOUNG_DAYS`] days (`age_days`: since the file was first written; `None` when unknown).
pub fn youth(this_run: bool, age_days: Option<i64>) -> String {
    match (this_run, age_days) {
        (true, _) => "; the best is from this run: the factor may be too low".to_owned(),
        (false, Some(n)) if n < YOUNG_DAYS => format!("; the best is {n} day{} old: the factor may be too low", if n == 1 { "" } else { "s" }),
        (false, None) => "; the best's age is not known: the factor may be too low".to_owned(),
        _ => String::new(),
    }
}

/// What the file keeps: the best, the day it was last lowered, the day the file was first
/// written. An older file (one number, or a number and one date) reads too.
#[derive(Clone, Debug, PartialEq)]
pub struct Kept {
    pub best: f64,
    pub lowered: String,
    pub first: String,
}

impl Kept {
    pub fn parse(text: &str) -> Option<Kept> {
        let mut words = text.split_whitespace();
        let best: f64 = words.next()?.parse().ok()?;
        if !(best > 0.0 && best.is_finite()) {
            return None;
        }
        let lowered = words.next().filter(|d| days_of(d).is_some()).unwrap_or("").to_owned();
        let first = words.next().filter(|d| days_of(d).is_some()).map_or_else(|| lowered.clone(), str::to_owned);
        Some(Kept { best, lowered, first })
    }

    pub fn text(&self) -> String {
        format!("{:.3} {} {}\n", self.best, self.lowered, self.first)
    }
}

/// The machine's reference in one gate run: the kept best, lowered by every set of samples taken.
pub struct Reference {
    path: PathBuf,
    kept: Option<Kept>,
    /// Whether this run lowered the best (or made it).
    pub this_run: bool,
}

impl Reference {
    pub fn open(path: PathBuf) -> Reference {
        let kept = std::fs::read_to_string(&path).ok().as_deref().and_then(Kept::parse);
        Reference { path, kept, this_run: false }
    }

    /// The best so far (infinity before any sample).
    pub fn best(&self) -> f64 {
        self.kept.as_ref().map_or(f64::INFINITY, |k| k.best)
    }

    /// The day the file was first written (empty when not known).
    pub fn since(&self) -> &str {
        self.kept.as_ref().map_or("", |k| k.first.as_str())
    }

    /// Takes a set of samples, lowers the best by its least, and returns the set.
    pub fn take(&mut self) -> Vec<f64> {
        let set = samples();
        self.offer(least(&set), &today());
        set
    }

    /// Lowers the best to `least` if it beats it (and writes the file); `today` is `YYYY-MM-DD`.
    pub fn offer(&mut self, least: f64, today: &str) {
        if !(least > 0.0 && least.is_finite()) || least >= self.best() {
            return;
        }
        let first = self.kept.as_ref().map(|k| k.first.clone()).filter(|f| !f.is_empty()).unwrap_or_else(|| today.to_owned());
        let kept = Kept { best: best_over(self.kept.as_ref().map(|k| k.best), least), lowered: today.to_owned(), first };
        let wrote = self.path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&self.path, kept.text()));
        if let Err(e) = wrote {
            println!("  NOTE: the machine's best reference could not be kept in {}: {e}", self.path.display());
        }
        self.kept = Some(kept);
        self.this_run = true;
    }

    /// The factor's line's ending when the best is young (empty when it is not).
    pub fn youth_on(&self, today: &str) -> String {
        let age = days_of(self.since()).zip(days_of(today)).map(|(a, b)| b - a);
        youth(self.this_run, age)
    }
}

/// Where the gate keeps the machine's best.
pub fn kept_at(target: &Path) -> PathBuf {
    target.join("gate").join("speed-reference.txt")
}

/// Today's date (UTC) as `YYYY-MM-DD`.
pub fn today() -> String {
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

/// `YYYY-MM-DD` as days since 1970-01-01; `None` for anything else.
fn days_of(date: &str) -> Option<i64> {
    let b = date.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let (y, m, d): (i64, i64, i64) = (date[..4].parse().ok()?, date[5..7].parse().ok()?, date[8..].parse().ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
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
            (50.0, f64::INFINITY, 1.0, false),
            (f64::INFINITY, 30.0, 1.0, false),
        ];
        for (now, best, want, loaded) in table {
            let f = factor(now, best);
            assert!((f - want).abs() < 1e-9, "factor({now}, {best}) = {f}, not {want}");
            assert_eq!(too_loaded(f), loaded, "factor {f}");
        }
    }

    #[test]
    fn least_median_and_best_over() {
        let table: [(&[f64], f64, f64); 5] = [
            (&[60.0, 45.0, 70.0], 45.0, 60.0),
            (&[3.0, 1.0, 2.0, 4.0], 1.0, 3.0),
            (&[7.0], 7.0, 7.0),
            (&[], f64::INFINITY, f64::INFINITY),
            (&[f64::NAN, 5.0, 9.0], 5.0, 9.0),
        ];
        for (set, lo, mid) in table {
            assert_eq!(least(set), lo, "{set:?}");
            let m = median(set);
            assert!(m == mid || (m.is_nan() && mid.is_nan()), "{set:?}: {m}");
        }
        for (stored, run, want) in [(None, 45.0, 45.0), (Some(27.0), 45.0, 27.0), (Some(50.0), 45.0, 45.0)] {
            assert_eq!(best_over(stored, run), want);
        }
        // No file, a loaded machine: the median over the least, not 1.
        let set = [60.0, 45.0, 62.0, 58.0, 61.0];
        assert!((factor(median(&set), best_over(None, least(&set))) - 1.34).abs() < 1e-9);
    }

    #[test]
    fn the_youth_is_said() {
        let table = [
            (true, Some(10), "; the best is from this run: the factor may be too low"),
            (true, None, "; the best is from this run: the factor may be too low"),
            (false, Some(0), "; the best is 0 days old: the factor may be too low"),
            (false, Some(1), "; the best is 1 day old: the factor may be too low"),
            (false, Some(2), "; the best is 2 days old: the factor may be too low"),
            (false, Some(3), ""),
            (false, Some(40), ""),
            (false, None, "; the best's age is not known: the factor may be too low"),
        ];
        for (this_run, age, want) in table {
            assert_eq!(youth(this_run, age), want, "{this_run} {age:?}");
        }
    }

    #[test]
    fn the_file_keeps_the_best_and_two_dates() {
        let k = Kept::parse("27.125 2026-11-28 2026-11-20\n").unwrap();
        assert_eq!(k, Kept { best: 27.125, lowered: "2026-11-28".into(), first: "2026-11-20".into() });
        assert_eq!(Kept::parse(&k.text()), Some(k));
        // Older files: a number and one date (both dates are that one), or a number alone.
        assert_eq!(Kept::parse("77.498 2026-11-28\n").unwrap().first, "2026-11-28");
        let bare = Kept::parse("31.5").unwrap();
        assert_eq!((bare.best, bare.first.as_str()), (31.5, ""));
        for junk in ["", "fast\n", "-3 2026-11-28\n", "NaN\n"] {
            assert_eq!(Kept::parse(junk), None, "{junk:?}");
        }
    }

    #[test]
    fn the_best_is_lowered_and_never_raised() {
        let dir = std::env::temp_dir().join(format!("examen-mensura-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = kept_at(&dir);
        // Missing: the first sample becomes the best, the folder and file are made, and it is
        // this run's.
        let mut r = Reference::open(path.clone());
        assert_eq!(r.best(), f64::INFINITY);
        r.offer(40.0, "2026-11-20");
        assert_eq!((r.best(), r.this_run), (40.0, true));
        assert_eq!(Kept::parse(&std::fs::read_to_string(&path).unwrap()).unwrap(), Kept { best: 40.0, lowered: "2026-11-20".into(), first: "2026-11-20".into() });
        assert_eq!(r.youth_on("2026-11-28"), youth(true, None));
        // A later run: slower never raises it, and an old best is not young.
        let mut r = Reference::open(path.clone());
        r.offer(55.0, "2026-11-28");
        assert_eq!((r.best(), r.this_run), (40.0, false));
        assert_eq!(r.youth_on("2026-11-28"), "");
        assert_eq!(r.youth_on("2026-11-21"), youth(false, Some(1)));
        // Faster lowers it: the day lowered moves, the day first written stays.
        r.offer(31.5, "2026-11-28");
        assert_eq!(Kept::parse(&std::fs::read_to_string(&path).unwrap()).unwrap(), Kept { best: 31.5, lowered: "2026-11-28".into(), first: "2026-11-20".into() });
        // Unreadable: as if missing, and written over.
        std::fs::write(&path, "fast\n").unwrap();
        let mut r = Reference::open(path.clone());
        r.offer(70.0, "2026-11-28");
        assert_eq!(Kept::parse(&std::fs::read_to_string(&path).unwrap()).map(|k| k.best), Some(70.0));
        // An old one-number file reads, and a lowering gives it today as its first day.
        std::fs::write(&path, "33\n").unwrap();
        let mut r = Reference::open(path.clone());
        assert_eq!(r.best(), 33.0);
        r.offer(32.0, "2026-11-28");
        assert_eq!(Kept::parse(&std::fs::read_to_string(&path).unwrap()).unwrap().first, "2026-11-28");
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
    fn days_become_dates_and_back() {
        for (days, date) in [(0, (1970, 1, 1)), (11_016, (2000, 2, 29)), (20_785, (2026, 11, 28))] {
            assert_eq!(civil(days), date);
            assert_eq!(days_of(&format!("{:04}-{:02}-{:02}", date.0, date.1, date.2)), Some(days));
        }
        assert_eq!(days_of("2026-13-01"), None);
        assert_eq!(days_of("26-11-28"), None);
    }
}
