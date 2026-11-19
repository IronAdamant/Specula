//! `perf_light`'s noise rule (docs/design/LIGHT.md, "What was mended, second round"): whether a
//! feature's cost is told from the noise of the machine it was measured on, and whether that
//! machine was steady enough while it was measured to advise on.
//!
//! The rounds draw every variant once, as the world has it first, and every second round
//! backwards ([`variant_at`]), so that the frames of one round are near each other in time
//! and a machine whose times drift moves both orders alike. A cost is then read from **pairs**:
//! the view as it is less the view without the feature, frame by frame of the same round. Load
//! that comes and goes (another program compiling) moves both frames of a pair alike far more
//! often than it moves two medians of separate frames alike, which is what the rule before
//! this one compared, and called a costless feature a cost in up to one report in ten.
//!
//! Pure: tested, and simulated, without a GPU.

/// The middle of the samples (the mean of the middle two of an even count); 0 of none.
pub(crate) fn median(samples: &[f64]) -> f64 {
    let mut s = samples.to_vec();
    s.sort_by(f64::total_cmp);
    match s.len() {
        0 => 0.0,
        n if n % 2 == 1 => s[n / 2],
        n => (s[n / 2 - 1] + s[n / 2]) / 2.0,
    }
}

/// The middle of the samples' distances from their median: a spread one wild sample cannot
/// spoil.
pub(crate) fn spread(samples: &[f64]) -> f64 {
    let middle = median(samples);
    let off: Vec<f64> = samples.iter().map(|v| (v - middle).abs()).collect();
    median(&off)
}

/// The variant drawn `i`-th in round `round` of `len` variants: in order in even rounds,
/// backwards in odd ones.
pub(crate) fn variant_at(round: u32, i: usize, len: usize) -> usize {
    if round % 2 == 1 {
        len - 1 - i
    } else {
        i
    }
}

/// How many times the pairs' spread a cost must pass, by how many pairs there are (by
/// simulation: a costless feature is called a cost about once in a hundred on an even noise
/// at five pairs, less often with more; LIGHT.md has the table).
fn times(pairs: usize) -> f64 {
    match pairs {
        0..=5 => 5.0,
        6..=7 => 4.0,
        8..=11 => 3.2,
        _ => 2.5,
    }
}

/// How many of the pairs must say the same as the cost (more, or less, without the feature):
/// every pair of five or fewer, all but one to eleven, all but three from twelve.
fn agreeing(pairs: usize) -> usize {
    match pairs {
        0..=5 => pairs,
        6..=11 => pairs - 1,
        _ => pairs - 3,
    }
}

/// The least a cost may be and be told, as a share of the frame: frames alike to the last digit
/// have no spread.
pub(crate) const FLOOR_SHARE: f64 = 0.005;

/// A feature's cost against the view as it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Cost {
    /// The middle of the pairs' differences (the view as it is less the view without it).
    pub cost: f64,
    /// What a cost must pass to be told.
    pub noise: f64,
    pub told: bool,
    /// The pairs it was read from.
    pub pairs: usize,
}

/// The cost of what `without` leaves out, from the frames of the same rounds (`as_is[r]` and
/// `without[r]` were drawn in round `r`; a round one of them lacks is not counted). Nothing is
/// told from fewer than `least` pairs.
pub(crate) fn cost(as_is: &[f64], without: &[f64], least: usize) -> Cost {
    let pairs = as_is.len().min(without.len());
    let d: Vec<f64> = as_is[..pairs].iter().zip(&without[..pairs]).map(|(a, b)| a - b).collect();
    let cost = median(&d);
    let noise = (times(pairs) * spread(&d)).max(median(&as_is[..pairs]) * FLOOR_SHARE);
    let more = d.iter().filter(|x| **x > 0.0).count();
    let less = d.iter().filter(|x| **x < 0.0).count();
    let agree = if cost > 0.0 { more } else { less };
    Cost { cost, noise, told: pairs >= least && pairs > 0 && cost.abs() > noise && agree >= agreeing(pairs), pairs }
}

/// A frame further than this share over its variant's quick frames is late.
pub(crate) const LATE_BY: f64 = 0.05;
/// With this share of all the frames late or more, the machine was busy by turns.
pub(crate) const BUSY_SHARE: f64 = 0.2;

/// How many frames of all the variants are late (more than [`LATE_BY`] over the mean of their
/// own variant's quickest third, at least two), of how many: what the frames' own scatter says
/// of a machine whose load came and went while it was measured.
pub(crate) fn late(variants: &[&[f64]]) -> (usize, usize) {
    let (mut late, mut of) = (0, 0);
    for v in variants {
        if v.len() < 2 {
            continue;
        }
        let mut s = v.to_vec();
        s.sort_by(f64::total_cmp);
        let m = (s.len() / 3).max(2);
        let quick = s[..m].iter().sum::<f64>() / m as f64;
        of += v.len();
        late += v.iter().filter(|x| **x > quick * (1.0 + LATE_BY)).count();
    }
    (late, of)
}

/// Whether `late` of `of` frames is a machine too busy by turns to advise on.
pub(crate) fn busy((late, of): (usize, usize)) -> bool {
    of > 0 && late as f64 >= of as f64 * BUSY_SHARE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule before this one (the first review's), kept to show what it did: two medians
    /// apart by more than the noisier variant's scatter.
    fn before(as_is: &[f64], without: &[f64]) -> bool {
        let scatter = |s: &[f64]| {
            let k = match s.len() {
                0..=6 => 6.0,
                7..=8 => 4.0,
                _ => 3.0,
            };
            k * spread(s)
        };
        let total = median(as_is);
        (total - median(without)).abs() > scatter(as_is).max(scatter(without)).max(total * 0.005)
    }

    /// Fixed sequences, the same every run.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn unif(&mut self) -> f64 {
            ((self.next() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
        }

        fn normal(&mut self) -> f64 {
            let (u, v) = (self.unif(), self.unif());
            (-2.0 * u.ln()).sqrt() * (2.0 * std::f64::consts::PI * v).cos()
        }
    }

    /// How a machine's frame times move, as a share of the frame.
    #[derive(Clone, Copy)]
    enum Noise {
        /// Spread evenly: 1% of the frame.
        Even,
        /// Mostly steady, now and then slow (a long tail: 0.3% even, and 1.5% on average more).
        Skewed,
        /// This share of frames late by about a fifth, each frame on its own.
        Late(f64),
        /// Late by a tenth to three tenths, in bursts of about `len` renders, this share of the
        /// time: another program compiling.
        Bursts { share: f64, len: f64 },
        /// Slower or quicker by a tenth from the first render to the last.
        Drift,
    }

    const NOISES: [(&str, Noise); 8] = [
        ("even, 1%", Noise::Even),
        ("skewed", Noise::Skewed),
        ("a third late by a fifth", Noise::Late(1.0 / 3.0)),
        ("half late by a fifth", Noise::Late(0.5)),
        ("bursts, a third of the time, ~8 renders", Noise::Bursts { share: 1.0 / 3.0, len: 8.0 }),
        ("bursts, half the time, ~8 renders", Noise::Bursts { share: 0.5, len: 8.0 }),
        ("bursts, half the time, ~40 renders", Noise::Bursts { share: 0.5, len: 40.0 }),
        ("a slow drift of a tenth", Noise::Drift),
    ];

    /// What each of `total` renders in order is late by, as a share of the frame.
    fn noise(r: &mut Rng, k: Noise, total: usize) -> Vec<f64> {
        match k {
            Noise::Even => (0..total).map(|_| 0.01 * r.normal()).collect(),
            Noise::Skewed => (0..total).map(|_| 0.003 * r.normal() - 0.015 * r.unif().ln()).collect(),
            Noise::Late(p) => (0..total).map(|_| 0.002 * r.normal() + if r.unif() < p { 0.2 * (0.5 + r.unif()) } else { 0.0 }).collect(),
            Noise::Bursts { share, len } => {
                let leave = 1.0 / len;
                let enter = share * leave / (1.0 - share);
                let mut on = r.unif() < share;
                let mut level = 0.1 + 0.2 * r.unif();
                (0..total)
                    .map(|_| {
                        if on {
                            on = r.unif() >= leave;
                        } else if r.unif() < enter {
                            on = true;
                            level = 0.1 + 0.2 * r.unif();
                        }
                        0.002 * r.normal() + if on { level } else { 0.0 }
                    })
                    .collect()
            }
            Noise::Drift => {
                let sign = if r.unif() < 0.5 { -1.0 } else { 1.0 };
                (0..total).map(|i| 0.003 * r.normal() + sign * 0.1 * i as f64 / total as f64).collect()
            }
        }
    }

    /// The frames of each variant (`costs[0]` is the view as it is, 0) for `n` counted rounds
    /// after one that is not, in the job's own order ([`variant_at`]); a 5 ms frame.
    fn measure(r: &mut Rng, k: Noise, costs: &[f64], n: usize) -> Vec<Vec<f64>> {
        let len = costs.len();
        let late = noise(r, k, len * (n + 1));
        let mut out = vec![Vec::new(); len];
        let mut t = 0;
        for round in 0..=n as u32 {
            for i in 0..len {
                let v = variant_at(round, i, len);
                let frame = 5.0 * (1.0 - costs[v] + late[t]);
                t += 1;
                if round > 0 {
                    out[v].push(frame);
                }
            }
        }
        out
    }

    /// Ten variants, as a report of eight features and all of them out has: out of `trials`
    /// reports, the share of the eight costless features called a cost, the share told of one
    /// that costs `costs` of the frame (drawn last of the eight in a round, the furthest from
    /// the view as it is), and the share of reports that found the machine busy.
    fn rates(rule: fn(&[f64], &[f64]) -> bool, k: Noise, n: usize, trials: usize, seed: u64) -> (f64, [f64; 4], f64) {
        const LEN: usize = 10;
        let mut r = Rng(0x2545_F491_4F6C_DD1D ^ seed);
        let (mut wrong, mut busy_reports, mut told) = (0usize, 0usize, [0usize; 4]);
        for _ in 0..trials {
            let m = measure(&mut r, k, &[0.0; LEN], n);
            wrong += (1..LEN - 1).filter(|f| rule(&m[0], &m[*f])).count();
            let all: Vec<&[f64]> = m.iter().map(Vec::as_slice).collect();
            busy_reports += busy(late(&all)) as usize;
            for (i, c) in [0.01, 0.02, 0.05, 0.10].into_iter().enumerate() {
                let mut costs = [0.0; LEN];
                costs[LEN - 2] = c;
                let m = measure(&mut r, k, &costs, n);
                told[i] += rule(&m[0], &m[LEN - 2]) as usize;
            }
        }
        let share = |x: usize, of: usize| x as f64 / of as f64;
        (share(wrong, trials * (LEN - 2)), told.map(|t| share(t, trials)), share(busy_reports, trials))
    }

    fn now(as_is: &[f64], without: &[f64]) -> bool {
        cost(as_is, without, 1).told
    }

    /// The rule holds where the rule before it did not: on fixed sequences of a machine busy in
    /// bursts (half the time, a few renders at a time), the rule before called a costless
    /// feature a cost in about one case in fifteen and this one in under one in a hundred; and
    /// on an even noise it still tells a cost of a twentieth of the frame nearly always.
    #[test]
    fn the_noise_rule_holds_when_the_machine_is_busy_by_turns() {
        let bursts = Noise::Bursts { share: 0.5, len: 8.0 };
        let (was, _, _) = rates(before, bursts, 9, 400, 1);
        let (is, _, busy_share) = rates(now, bursts, 9, 400, 1);
        assert!(was > 0.04, "the case under test: the rule before called {was} of costless features a cost");
        assert!(is < 0.01, "a costless feature called a cost {is} of the time under bursts");
        assert!(busy_share > 0.9, "and the machine is found busy: {busy_share}");
        let (is, told, busy_share) = rates(now, Noise::Even, 9, 400, 2);
        assert!(is < 0.01 && told[2] > 0.9 && busy_share == 0.0, "even noise: false {is}, a 5% cost told {}, busy {busy_share}", told[2]);
        // A drift: the order that turns back every second round leaves no cost where there is
        // none (drawn always forwards, the pairs carried the drift: a third of costless
        // features were called a cost).
        let (is, told, _) = rates(now, Noise::Drift, 9, 400, 3);
        assert!(is < 0.01 && told[2] > 0.9, "drift: false {is}, a 5% cost told {}", told[2]);
    }

    /// Small cases by hand: pairs that agree are told; one wild pair moves nothing; a pair that
    /// says the other way stops a cost of five frames; too few pairs tell nothing.
    #[test]
    fn a_cost_is_read_from_pairs_of_the_same_round() {
        let c = cost(&[10.0, 10.2, 9.9, 10.1, 10.0], &[6.0, 6.1, 5.9, 6.0, 6.2], 5);
        assert!(c.told && (c.cost - 4.0).abs() < 1e-9 && c.pairs == 5, "{c:?}");
        // Late together: the pairs still say 1 ms.
        let c = cost(&[10.0, 13.0, 10.0, 13.0, 10.0], &[9.0, 12.0, 9.0, 12.0, 9.0], 5);
        assert!(c.told && (c.cost - 1.0).abs() < 1e-9, "{c:?}");
        // The separate medians are 3 ms apart; the pairs are not.
        let c = cost(&[13.0, 13.0, 13.0, 10.0, 10.0], &[13.0, 10.0, 10.0, 10.0, 10.0], 5);
        assert!(!c.told, "{c:?}");
        // One pair of five the other way: not told.
        let c = cost(&[10.0, 10.0, 10.0, 10.0, 10.0], &[9.0, 9.0, 9.0, 9.0, 11.0], 5);
        assert!(!c.told, "{c:?}");
        // Fewer pairs than the least: never told; a round one variant lacks is not counted.
        assert!(!cost(&[10.0, 10.0], &[5.0, 5.0], 5).told);
        assert_eq!(cost(&[10.0, 10.0, 10.0], &[5.0, 5.0], 1).pairs, 2);
        // The order: forwards, then backwards.
        assert_eq!((0..4).map(|i| variant_at(0, i, 4)).collect::<Vec<_>>(), [0, 1, 2, 3]);
        assert_eq!((0..4).map(|i| variant_at(1, i, 4)).collect::<Vec<_>>(), [3, 2, 1, 0]);
        // Late frames: two of ten frames a fifth over the quick ones is a busy machine; one is not.
        let steady = [5.0, 5.01, 4.99, 5.0, 5.02, 5.0, 4.98, 5.0, 5.01, 5.0];
        let mut two = steady;
        (two[3], two[7]) = (6.0, 6.0);
        assert!(!busy(late(&[&steady])) && busy(late(&[&two])));
        let mut one = steady;
        one[3] = 6.0;
        assert!(!busy(late(&[&one])));
    }

    /// LIGHT.md's table of the rule, computed: run with `--ignored` (about a minute in a debug
    /// build) and paste what it says differs.
    #[test]
    #[ignore = "a simulation of 3,000 reports a cell: run by hand when the rule or its table changes"]
    fn the_noise_table_in_light_md() {
        use std::fmt::Write;
        let mut table = String::new();
        for (name, rule) in [("before", before as fn(&[f64], &[f64]) -> bool), ("now", now)] {
            let _ = writeln!(table, "{name}:");
            for (what, k) in NOISES {
                let mut row = format!("| {what} |");
                for (i, n) in [5, 7, 9, 16].into_iter().enumerate() {
                    let (wrong, told, busy_share) = rates(rule, k, n, 3000, 10 + i as u64);
                    let pc = |x: f64| format!("{:.1}", x * 100.0);
                    let _ = write!(row, " {} ({}) {}/{}/{}/{} |", pc(wrong), pc(busy_share), pc(told[0]), pc(told[1]), pc(told[2]), pc(told[3]));
                }
                let _ = writeln!(table, "{row}");
            }
        }
        assert_eq!(table, TABLE, "the table changed: LIGHT.md says the old one\n{table}");
    }

    const TABLE: &str = r"before:
| even, 1% | 0.6 (0.0) 2.8/10.6/64.6/99.1 | 0.7 (0.0) 4.6/21.8/94.1/100.0 | 0.6 (0.0) 7.0/38.6/99.4/100.0 | 0.0 (0.0) 2.6/37.7/100.0/100.0 |
| skewed | 0.5 (0.0) 2.8/9.0/53.0/93.1 | 0.5 (0.0) 2.5/18.3/81.5/99.5 | 0.4 (0.0) 6.2/32.4/95.2/100.0 | 0.0 (0.0) 1.1/25.8/98.6/100.0 |
| a third late by a fifth | 3.9 (98.9) 17.1/41.7/66.4/66.0 | 3.0 (99.7) 28.9/56.8/70.4/71.9 | 2.7 (99.8) 40.1/70.7/75.9/77.0 | 0.2 (100.0) 39.7/74.5/76.6/76.7 |
| half late by a fifth | 8.6 (100.0) 11.6/21.8/33.1/34.4 | 7.7 (100.0) 13.1/25.2/33.6/33.7 | 9.8 (100.0) 15.9/30.7/34.3/34.6 | 1.9 (100.0) 5.3/17.8/18.1/21.1 |
| bursts, a third of the time, ~8 renders | 7.9 (73.5) 28.4/53.0/70.3/72.0 | 7.7 (80.1) 41.2/65.9/74.9/77.8 | 6.0 (81.6) 48.3/71.6/76.6/76.1 | 0.9 (88.1) 42.0/66.7/70.6/69.9 |
| bursts, half the time, ~8 renders | 12.0 (95.6) 22.6/35.0/48.8/50.4 | 12.1 (98.0) 26.2/40.5/48.4/50.6 | 11.0 (98.9) 26.5/41.6/45.6/46.8 | 3.3 (99.9) 12.2/23.9/27.3/29.6 |
| bursts, half the time, ~40 renders | 6.7 (56.7) 42.0/72.8/89.3/89.9 | 6.9 (68.8) 53.1/80.5/89.5/89.7 | 5.9 (75.1) 56.4/78.7/83.3/83.4 | 0.8 (87.9) 35.4/52.1/57.8/60.2 |
| a slow drift of a tenth | 0.0 (70.3) 0.0/0.0/0.0/0.0 | 0.0 (100.0) 0.0/0.0/0.0/55.4 | 0.0 (100.0) 0.0/0.0/1.8/100.0 | 0.0 (100.0) 0.0/0.0/0.0/100.0 |
now:
| even, 1% | 0.9 (0.0) 5.1/20.5/70.1/98.4 | 0.8 (0.0) 4.9/22.1/83.8/99.8 | 0.4 (0.0) 5.3/27.4/94.4/100.0 | 0.0 (0.0) 3.8/39.6/99.9/100.0 |
| skewed | 0.6 (0.0) 3.6/12.2/52.1/89.9 | 0.5 (0.0) 3.7/15.5/68.0/97.5 | 0.2 (0.0) 3.2/18.9/81.9/99.5 | 0.0 (0.0) 2.0/24.2/97.3/100.0 |
| a third late by a fifth | 0.5 (98.9) 5.9/12.6/18.2/20.1 | 0.4 (99.7) 10.4/20.1/27.5/31.6 | 0.1 (99.8) 8.1/16.0/18.7/24.7 | 0.0 (100.0) 9.1/17.1/20.3/28.7 |
| half late by a fifth | 1.0 (100.0) 2.3/3.8/5.8/8.8 | 0.7 (100.0) 2.1/4.2/8.1/11.8 | 0.3 (100.0) 1.3/2.2/4.6/7.6 | 0.0 (100.0) 0.5/0.8/2.7/6.4 |
| bursts, a third of the time, ~8 renders | 0.5 (73.5) 16.2/27.7/35.5/37.4 | 0.3 (80.1) 25.2/44.4/50.1/54.5 | 0.1 (81.6) 25.7/39.2/41.9/44.6 | 0.0 (88.1) 39.8/52.9/53.5/55.3 |
| bursts, half the time, ~8 renders | 1.0 (95.6) 8.3/13.6/20.7/23.5 | 0.8 (98.0) 11.4/23.1/29.1/33.5 | 0.3 (98.9) 9.0/17.8/20.8/24.6 | 0.0 (99.9) 10.2/18.2/21.4/27.7 |
| bursts, half the time, ~40 renders | 0.1 (56.7) 37.4/57.7/65.6/65.3 | 0.0 (68.8) 56.0/83.7/88.0/88.3 | 0.0 (75.1) 66.2/80.7/81.5/81.0 | 0.0 (87.9) 90.8/94.4/94.8/95.0 |
| a slow drift of a tenth | 0.0 (70.3) 0.7/21.1/76.8/99.9 | 0.0 (100.0) 3.4/19.0/93.1/100.0 | 0.0 (100.0) 3.5/33.4/100.0/100.0 | 0.0 (100.0) 30.7/99.0/100.0/100.0 |
";
}
