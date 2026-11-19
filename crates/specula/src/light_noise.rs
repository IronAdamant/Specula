//! `perf_light`'s noise rule (docs/design/LIGHT.md, "What was mended, second round"): whether a
//! feature's cost is told from the noise of the machine it was measured on, and whether that
//! machine was steady enough while it was measured to advise on.
//!
//! The rounds draw every variant once, and every second round backwards ([`variant_at`]), so
//! that the frames of one round are near each other in time and a machine whose times drift
//! moves both orders alike; the variants between the view as it is and the last one go in an
//! order that changes from one pair of rounds to the next, so that a load that keeps step with
//! the rounds does not land on the same variants every time. A cost is then read from **pairs**:
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

/// Variants a round can have and still be drawn in a shuffled order (a report has eleven at
/// most: the view as it is, nine features, all of them out).
const MAX_SHUFFLED: usize = 16;
/// The seed of the rounds' order: a constant, so that the same report draws in the same order
/// every time, on every machine (no clock, no platform randomness).
const ORDER_SEED: u64 = 0x6C69_6768_745F_6F72;

/// The places `first..end` of `len` variants in order, shuffled by a fixed sequence of the
/// pair of rounds `pair` (Fisher and Yates, by splitmix64); the other places in order.
fn shuffled(pair: u32, len: usize, first: usize, end: usize) -> [usize; MAX_SHUFFLED] {
    let mut order = [0; MAX_SHUFFLED];
    for (k, o) in order.iter_mut().enumerate() {
        *o = k;
    }
    if len <= MAX_SHUFFLED && end <= len && first + 2 <= end {
        let mut s = ORDER_SEED ^ u64::from(pair).wrapping_mul(0xD6E8_FEB8_6659_FD93);
        for k in (1..end - first).rev() {
            s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = s;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            order.swap(first + k, first + (z % (k as u64 + 1)) as usize);
        }
    }
    order
}

/// The order of `len` variants in the forward round of the pair of rounds `pair` (rounds
/// `2 * pair` and `2 * pair + 1`), by a fixed sequence of the pair's number. The first pair
/// draws the view as it is first (its first render says which variants there are) and the
/// last variant last, the ones between shuffled; every later pair shuffles every place, the
/// view as it is too. (Drawn in one order every pair, a load that keeps step with the rounds
/// fell on the same places every round, and the pairs carried it as a cost: the third review.
/// With the view as it is always at an end, it still did: its place is half of every pair.)
fn forward_order(pair: u32, len: usize) -> [usize; MAX_SHUFFLED] {
    if pair == 0 {
        shuffled(pair, len, 1, len.saturating_sub(1))
    } else {
        shuffled(pair, len, 0, len)
    }
}

/// The variant drawn `i`-th in round `round` of `len` variants. Rounds go in pairs: the even
/// one forwards, its odd one the same order backwards (so a slow drift moves both orders
/// alike), in an order that changes from pair to pair ([`forward_order`]).
pub(crate) fn variant_at(round: u32, i: usize, len: usize) -> usize {
    let at = if round % 2 == 1 { len - 1 - i } else { i };
    if len > MAX_SHUFFLED {
        return at;
    }
    forward_order(round / 2, len)[at]
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
        /// Spread evenly by this share of the frame (a machine that is merely uneven).
        Uneven(f64),
        /// Slower by `more` over the first half of every `period` rounds, at a phase of its
        /// own, and 0.2% even: a load that keeps step with the rounds, under the line for a
        /// late frame (the third review).
        Locked { more: f64, period: f64 },
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

    /// What each of `total` renders in order is late by, as a share of the frame, with `round`
    /// renders a round.
    fn noise(r: &mut Rng, k: Noise, total: usize, round: usize) -> Vec<f64> {
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
            Noise::Uneven(share) => (0..total).map(|_| share * r.normal()).collect(),
            Noise::Locked { more, period } => {
                let period = period * round as f64;
                let phase = r.unif() * period;
                (0..total).map(|t| 0.002 * r.normal() + if ((t as f64 + phase) / period).fract() < 0.5 { more } else { 0.0 }).collect()
            }
        }
    }

    /// An order of the rounds: which variant is drawn `i`-th in a round of so many.
    type Order = fn(u32, usize, usize) -> usize;

    /// The second round's order, for the tables: forwards, and every second round backwards.
    fn serpentine(round: u32, i: usize, len: usize) -> usize {
        if round % 2 == 1 {
            len - 1 - i
        } else {
            i
        }
    }

    /// An order tried beside the one chosen, as the lead first decided it: only the variants
    /// between the view as it is and the last shuffled, in every pair, mirrored within a pair.
    fn shuffled_middle(round: u32, i: usize, len: usize) -> usize {
        let at = if round % 2 == 1 { len - 1 - i } else { i };
        shuffled(round / 2, len, 1, len.saturating_sub(1))[at]
    }

    /// The frames of each variant (`costs[0]` is the view as it is, 0) for `n` counted rounds
    /// after one that is not, in the order `order`; a 5 ms frame.
    fn measure(r: &mut Rng, k: Noise, costs: &[f64], n: usize, order: Order) -> Vec<Vec<f64>> {
        let len = costs.len();
        let late = noise(r, k, len * (n + 1), len);
        let mut out = vec![Vec::new(); len];
        let mut t = 0;
        for round in 0..=n as u32 {
            for i in 0..len {
                let v = order(round, i, len);
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
        let s = rates_in(rule, k, n, trials, seed, variant_at);
        (s.wrong, s.told, s.busy)
    }

    /// What [`rates`] says, in an order of the rounds, and of whole reports: the share of the
    /// steady ones (not found busy) in which the rule now ([`cost`]) tells some costless
    /// feature a cost more than nothing, the one way a report could advise giving up a feature
    /// that costs nothing (`None`: no report was steady).
    struct Rates {
        wrong: f64,
        told: [f64; 4],
        busy: f64,
        report_wrong: Option<f64>,
    }

    fn rates_in(rule: fn(&[f64], &[f64]) -> bool, k: Noise, n: usize, trials: usize, seed: u64, order: Order) -> Rates {
        const LEN: usize = 10;
        let mut r = Rng(0x2545_F491_4F6C_DD1D ^ seed);
        let (mut wrong, mut busy_reports, mut told) = (0usize, 0usize, [0usize; 4]);
        let mut wrong_reports = 0usize;
        for _ in 0..trials {
            let m = measure(&mut r, k, &[0.0; LEN], n, order);
            wrong += (1..LEN - 1).filter(|f| rule(&m[0], &m[*f])).count();
            let all: Vec<&[f64]> = m.iter().map(Vec::as_slice).collect();
            let is_busy = busy(late(&all));
            busy_reports += is_busy as usize;
            if !is_busy && (1..LEN - 1).any(|f| cost(&m[0], &m[f], 1).told && cost(&m[0], &m[f], 1).cost > 0.0) {
                wrong_reports += 1;
            }
            for (i, c) in [0.01, 0.02, 0.05, 0.10].into_iter().enumerate() {
                let mut costs = [0.0; LEN];
                costs[LEN - 2] = c;
                let m = measure(&mut r, k, &costs, n, order);
                told[i] += rule(&m[0], &m[LEN - 2]) as usize;
            }
        }
        let share = |x: usize, of: usize| x as f64 / of as f64;
        let steady = trials - busy_reports;
        Rates {
            wrong: share(wrong, trials * (LEN - 2)),
            told: told.map(|t| share(t, trials)),
            busy: share(busy_reports, trials),
            report_wrong: (steady > 0).then(|| share(wrong_reports, steady)),
        }
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
        // A load that keeps step with the rounds, under the line for a late frame (the third
        // review): in one order every pair, a third of costless features were called a cost on
        // a machine called steady; in an order that changes from pair to pair, the view as it
        // is with it, a few in a hundred.
        let locked = Noise::Locked { more: 0.04, period: 1.0 };
        let was = rates_in(now, locked, 9, 400, 4, serpentine);
        let is = rates_in(now, locked, 9, 400, 4, variant_at);
        assert!(was.wrong > 0.25 && was.busy == 0.0, "the case under test: in one order {} of costless features told, busy {}", was.wrong, was.busy);
        assert!(is.wrong < 0.08, "a load in step with the rounds: {} of costless features told", is.wrong);
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
        // Late frames: two of ten frames a fifth over the quick ones is a busy machine; one is not.
        let steady = [5.0, 5.01, 4.99, 5.0, 5.02, 5.0, 4.98, 5.0, 5.01, 5.0];
        let mut two = steady;
        (two[3], two[7]) = (6.0, 6.0);
        assert!(!busy(late(&[&steady])) && busy(late(&[&two])));
        let mut one = steady;
        one[3] = 6.0;
        assert!(!busy(late(&[&one])));
        // The line for a late frame is a twentieth over the mean of the quickest third: a
        // frame 6% over it is late, one 4% over is not (the third review: only the ignored
        // table held the line).
        let even = [5.0; 10];
        let (mut six, mut four) = (even, even);
        (six[2], six[8]) = (5.0 * 1.06, 5.0 * 1.06);
        (four[2], four[8]) = (5.0 * 1.04, 5.0 * 1.04);
        assert_eq!((late(&[&six]), late(&[&four])), ((2, 10), (0, 10)));
        assert!(busy(late(&[&six])) && !busy(late(&[&four])));
    }

    /// The order of a report's rounds: every round draws each variant once; an odd round is
    /// the even one before it backwards; the first round draws the view as it is first (its
    /// first render says what the variants are) and the last variant last; the order changes
    /// from pair to pair, the view as it is with it, and is the same every time.
    #[test]
    fn the_rounds_order_changes_from_pair_to_pair_and_mirrors_within_one() {
        for len in 1..=11 {
            let mut orders = std::collections::BTreeSet::new();
            let mut as_is_at = std::collections::BTreeSet::new();
            for round in 0..40u32 {
                let order: Vec<usize> = (0..len).map(|i| variant_at(round, i, len)).collect();
                let mut sorted = order.clone();
                sorted.sort_unstable();
                assert_eq!(sorted, (0..len).collect::<Vec<_>>(), "round {round} of {len} draws each variant once: {order:?}");
                assert_eq!(order, (0..len).map(|i| variant_at(round, i, len)).collect::<Vec<_>>(), "the same every time");
                if round == 0 {
                    assert_eq!((order[0], order[len - 1]), (0, len - 1), "{order:?}");
                }
                if round % 2 == 0 {
                    let back: Vec<usize> = (0..len).map(|i| variant_at(round + 1, i, len)).rev().collect();
                    assert_eq!(back, order, "round {} is round {round} backwards", round + 1);
                    as_is_at.insert(order.iter().position(|v| *v == 0));
                    orders.insert(order);
                }
            }
            assert!(len < 2 || (orders.len() > 1 && as_is_at.len() > 1), "{len} variants: the order never changed, or the view as it is never moved");
        }
        // Pinned: a report's order is a function of nothing but the round and the count.
        assert_eq!((0..10).map(|i| variant_at(0, i, 10)).collect::<Vec<_>>(), PINNED_PAIR_0);
        assert_eq!((0..10).map(|i| variant_at(2, i, 10)).collect::<Vec<_>>(), PINNED_PAIR_1);
    }

    const PINNED_PAIR_0: [usize; 10] = [0, 8, 4, 6, 2, 3, 5, 7, 1, 9];
    const PINNED_PAIR_1: [usize; 10] = [7, 3, 9, 0, 2, 4, 6, 8, 1, 5];

    /// LIGHT.md's table of the rule, computed: run with `--ignored` (about a minute in a debug
    /// build) and paste what it says differs.
    #[test]
    #[ignore = "a simulation of 3,000 reports a cell: run by hand when the rule or its table changes"]
    fn the_noise_table_in_light_md() {
        use std::fmt::Write;
        let mut table = String::new();
        // (The rule before as it was measured, in the second round's order; the rule now in
        // the order the job draws.)
        for (name, rule, order) in [("before", before as fn(&[f64], &[f64]) -> bool, serpentine as Order), ("now", now, variant_at)] {
            let _ = writeln!(table, "{name}:");
            for (what, k) in NOISES {
                let mut row = format!("| {what} |");
                for (i, n) in [5, 7, 9, 16].into_iter().enumerate() {
                    let Rates { wrong, told, busy: busy_share, .. } = rates_in(rule, k, n, 3000, 10 + i as u64, order);
                    let pc = |x: f64| format!("{:.1}", x * 100.0);
                    let _ = write!(row, " {} ({}) {}/{}/{}/{} |", pc(wrong), pc(busy_share), pc(told[0]), pc(told[1]), pc(told[2]), pc(told[3]));
                }
                let _ = writeln!(table, "{row}");
            }
        }
        assert_eq!(table, TABLE, "the table changed: LIGHT.md says the old one\n{table}");
    }

    /// Loads LIGHT.md's tables do not have, from the third review: a load that keeps step
    /// with the rounds (more by so much over half of each period), and a machine that is
    /// merely uneven.
    const MORE_NOISES: [(&str, Noise); 8] = [
        ("+4% over half of each round", Noise::Locked { more: 0.04, period: 1.0 }),
        ("+2% over half of each round", Noise::Locked { more: 0.02, period: 1.0 }),
        ("+4%, a period of 0.95 rounds", Noise::Locked { more: 0.04, period: 0.95 }),
        ("+4%, a period of 1.05 rounds", Noise::Locked { more: 0.04, period: 1.05 }),
        ("+4%, a period of 1.2 rounds", Noise::Locked { more: 0.04, period: 1.2 }),
        ("+4%, a period of 2 rounds", Noise::Locked { more: 0.04, period: 2.0 }),
        ("even, 2%", Noise::Uneven(0.02)),
        ("even, 3%", Noise::Uneven(0.03)),
    ];

    /// The order of the rounds before and after the third round, on every kind of noise of
    /// LIGHT.md's table and the third review's, at 5, 9 and 16 frames: `--ignored`, a few
    /// minutes in a debug build. Each cell as the table's, then `r` and the share of steady
    /// reports that tell some costless feature a cost more than nothing, %.
    #[test]
    #[ignore = "a simulation of 3,000 reports a cell: run by hand when the order or the rule changes"]
    fn the_orders_table_in_light_md() {
        use std::fmt::Write;
        let mut table = String::new();
        for (what, k) in NOISES.into_iter().chain(MORE_NOISES) {
            for (name, order) in [("serpentine", serpentine as Order), ("middle shuffled", shuffled_middle), ("all shuffled", variant_at)] {
                let mut row = format!("| {what} | {name} |");
                // (The seeds of the main table's 5, 9 and 16 frames: its cells come again here.)
                for (n, seed) in [(5, 10), (9, 12), (16, 13)] {
                    let s = rates_in(now, k, n, 3000, seed, order);
                    let pc = |x: f64| format!("{:.1}", x * 100.0);
                    let _ = write!(row, " {} ({}) {}/{}/{}/{} r {} |", pc(s.wrong), pc(s.busy), pc(s.told[0]), pc(s.told[1]), pc(s.told[2]), pc(s.told[3]), s.report_wrong.map_or("-".into(), pc));
                }
                let _ = writeln!(table, "{row}");
            }
        }
        assert_eq!(table, ORDERS, "the orders' table changed: LIGHT.md says the old one\n{table}");
    }

    const ORDERS: &str = r"| even, 1% | serpentine | 0.9 (0.0) 5.1/20.5/70.1/98.4 r 3.1 | 0.4 (0.0) 5.3/27.4/94.4/100.0 r 1.3 | 0.0 (0.0) 3.8/39.6/99.9/100.0 r 0.2 |
| even, 1% | middle shuffled | 0.8 (0.0) 4.6/19.3/71.2/98.6 r 2.9 | 0.4 (0.0) 5.6/27.9/94.9/100.0 r 1.5 | 0.0 (0.0) 4.1/38.9/99.9/100.0 r 0.1 |
| even, 1% | all shuffled | 0.9 (0.0) 4.8/18.8/69.6/98.3 r 3.4 | 0.4 (0.0) 5.2/27.6/94.9/100.0 r 1.5 | 0.1 (0.0) 3.7/38.4/99.9/100.0 r 0.3 |
| skewed | serpentine | 0.6 (0.0) 3.6/12.2/52.1/89.9 r 1.9 | 0.2 (0.0) 3.2/18.9/81.9/99.5 r 0.5 | 0.0 (0.0) 2.0/24.2/97.3/100.0 r 0.1 |
| skewed | middle shuffled | 0.6 (0.0) 3.5/12.0/52.5/89.4 r 2.1 | 0.1 (0.0) 3.1/18.5/82.1/99.7 r 0.7 | 0.0 (0.0) 1.9/25.0/97.4/100.0 r 0.0 |
| skewed | all shuffled | 0.7 (0.0) 3.0/12.2/53.6/91.0 r 2.6 | 0.2 (0.0) 2.9/18.2/81.2/99.7 r 0.7 | 0.0 (0.0) 1.9/25.3/97.0/100.0 r 0.0 |
| a third late by a fifth | serpentine | 0.5 (98.9) 5.9/12.6/18.2/20.1 r 0.0 | 0.1 (99.8) 8.1/16.0/18.7/24.7 r 0.0 | 0.0 (100.0) 9.1/17.1/20.3/28.7 r - |
| a third late by a fifth | middle shuffled | 0.5 (98.9) 5.8/11.8/16.8/19.6 r 0.0 | 0.2 (99.8) 7.3/16.7/18.9/22.2 r 0.0 | 0.0 (100.0) 9.3/17.1/21.8/29.4 r - |
| a third late by a fifth | all shuffled | 0.5 (98.9) 6.0/11.6/16.9/18.9 r 0.0 | 0.2 (99.8) 7.7/17.2/20.0/22.8 r 0.0 | 0.0 (100.0) 10.2/18.1/23.2/28.8 r - |
| half late by a fifth | serpentine | 1.0 (100.0) 2.3/3.8/5.8/8.8 r - | 0.3 (100.0) 1.3/2.2/4.6/7.6 r - | 0.0 (100.0) 0.5/0.8/2.7/6.4 r - |
| half late by a fifth | middle shuffled | 1.0 (100.0) 1.9/3.8/6.7/9.1 r - | 0.3 (100.0) 1.0/2.7/3.9/8.7 r - | 0.0 (100.0) 0.4/0.8/2.7/6.4 r - |
| half late by a fifth | all shuffled | 0.7 (100.0) 2.1/3.2/5.3/8.6 r - | 0.3 (100.0) 1.3/2.2/4.5/8.6 r - | 0.0 (100.0) 0.3/0.9/2.6/7.2 r - |
| bursts, a third of the time, ~8 renders | serpentine | 0.5 (73.5) 16.2/27.7/35.5/37.4 r 0.5 | 0.1 (81.6) 25.7/39.2/41.9/44.6 r 0.2 | 0.0 (88.1) 39.8/52.9/53.5/55.3 r 0.0 |
| bursts, a third of the time, ~8 renders | middle shuffled | 0.4 (73.5) 23.6/38.1/44.9/46.1 r 0.5 | 0.0 (81.6) 38.2/53.7/56.9/60.0 r 0.2 | 0.0 (88.1) 70.4/81.1/81.3/81.7 r 0.0 |
| bursts, a third of the time, ~8 renders | all shuffled | 0.1 (73.6) 39.5/59.2/61.6/63.1 r 0.0 | 0.0 (81.6) 61.4/77.8/78.2/79.9 r 0.0 | 0.0 (88.1) 90.1/94.7/95.2/94.7 r 0.0 |
| bursts, half the time, ~8 renders | serpentine | 1.0 (95.6) 8.3/13.6/20.7/23.5 r 1.5 | 0.3 (98.9) 9.0/17.8/20.8/24.6 r 0.0 | 0.0 (99.9) 10.2/18.2/21.4/27.7 r 0.0 |
| bursts, half the time, ~8 renders | middle shuffled | 0.8 (95.8) 11.8/20.8/29.6/32.4 r 0.0 | 0.1 (98.9) 18.2/31.3/35.4/39.0 r 0.0 | 0.0 (99.9) 40.7/53.9/56.5/62.1 r 0.0 |
| bursts, half the time, ~8 renders | all shuffled | 0.3 (95.8) 26.7/42.3/50.1/49.5 r 0.0 | 0.0 (98.9) 43.8/62.5/62.6/65.6 r 0.0 | 0.0 (99.9) 75.4/85.4/85.0/86.7 r 0.0 |
| bursts, half the time, ~40 renders | serpentine | 0.1 (56.7) 37.4/57.7/65.6/65.3 r 0.1 | 0.0 (75.1) 66.2/80.7/81.5/81.0 r 0.0 | 0.0 (87.9) 90.8/94.4/94.8/95.0 r 0.0 |
| bursts, half the time, ~40 renders | middle shuffled | 0.0 (56.7) 44.1/66.6/73.2/72.3 r 0.1 | 0.0 (75.1) 77.1/88.2/88.9/88.3 r 0.0 | 0.0 (87.9) 96.9/98.6/99.0/98.8 r 0.0 |
| bursts, half the time, ~40 renders | all shuffled | 0.0 (56.8) 56.6/81.7/85.5/85.1 r 0.1 | 0.0 (75.1) 86.2/95.9/96.4/96.2 r 0.0 | 0.0 (87.9) 99.4/99.8/99.9/99.8 r 0.0 |
| a slow drift of a tenth | serpentine | 0.0 (70.3) 0.7/21.1/76.8/99.9 r 0.1 | 0.0 (100.0) 3.5/33.4/100.0/100.0 r - | 0.0 (100.0) 30.7/99.0/100.0/100.0 r - |
| a slow drift of a tenth | middle shuffled | 0.0 (56.8) 0.3/5.7/56.7/100.0 r 0.2 | 0.0 (100.0) 9.0/64.6/100.0/100.0 r - | 0.0 (100.0) 67.8/100.0/100.0/100.0 r - |
| a slow drift of a tenth | all shuffled | 0.1 (75.4) 14.7/53.3/99.3/100.0 r 0.8 | 0.0 (100.0) 41.7/94.6/100.0/100.0 r - | 0.0 (100.0) 83.7/100.0/100.0/100.0 r - |
| +4% over half of each round | serpentine | 34.0 (0.0) 40.1/52.8/82.8/99.9 r 40.7 | 35.5 (0.0) 48.0/58.5/82.0/100.0 r 38.2 | 30.1 (0.0) 41.4/38.8/60.0/90.8 r 38.8 |
| +4% over half of each round | middle shuffled | 21.3 (0.0) 15.2/24.2/70.4/79.3 r 38.6 | 35.1 (0.0) 40.2/40.9/49.3/80.7 r 37.6 | 46.1 (0.0) 40.4/39.9/59.3/100.0 r 38.7 |
| +4% over half of each round | all shuffled | 3.2 (0.0) 11.0/18.2/40.7/39.3 r 11.0 | 4.2 (0.0) 15.2/21.1/39.2/42.1 r 16.9 | 2.5 (0.0) 28.9/30.1/47.2/97.8 r 9.4 |
| +2% over half of each round | serpentine | 31.2 (0.0) 37.1/54.7/94.9/100.0 r 40.7 | 32.0 (0.0) 45.9/62.0/99.1/100.0 r 38.2 | 30.1 (0.0) 42.0/59.9/91.5/100.0 r 38.8 |
| +2% over half of each round | middle shuffled | 14.9 (0.0) 12.0/25.7/76.9/94.9 r 36.0 | 29.8 (0.0) 32.7/39.9/81.9/100.0 r 37.0 | 39.2 (0.0) 39.3/40.6/100.0/100.0 r 38.7 |
| +2% over half of each round | all shuffled | 2.5 (0.0) 11.0/26.2/41.1/89.5 r 9.4 | 3.8 (0.0) 15.2/35.8/51.0/100.0 r 15.7 | 1.1 (0.0) 28.9/39.2/97.6/100.0 r 4.4 |
| +4%, a period of 0.95 rounds | serpentine | 21.2 (0.0) 20.0/33.2/77.3/79.4 r 31.4 | 14.4 (0.0) 34.0/54.2/100.0/100.0 r 24.4 | 0.0 (0.0) 71.3/86.7/100.0/100.0 r 0.0 |
| +4%, a period of 0.95 rounds | middle shuffled | 14.7 (0.0) 16.0/26.7/87.9/100.0 r 29.9 | 11.0 (0.0) 18.8/20.1/29.1/45.8 r 22.4 | 0.0 (0.0) 0.0/0.0/22.3/99.8 r 0.0 |
| +4%, a period of 0.95 rounds | all shuffled | 4.3 (0.0) 5.4/11.1/41.5/42.2 r 13.9 | 3.5 (0.0) 7.4/19.1/48.5/48.1 r 11.0 | 0.9 (0.0) 10.8/15.8/46.2/99.8 r 3.7 |
| +4%, a period of 1.05 rounds | serpentine | 21.2 (0.0) 11.6/19.9/57.7/63.0 r 29.3 | 16.1 (0.0) 6.9/17.8/57.2/58.0 r 23.5 | 0.0 (0.0) 15.6/28.4/57.2/99.9 r 0.0 |
| +4%, a period of 1.05 rounds | middle shuffled | 17.2 (0.0) 5.1/12.2/53.9/62.7 r 30.0 | 12.2 (0.0) 13.3/18.3/47.5/67.9 r 21.9 | 0.0 (0.0) 0.0/0.0/10.4/99.6 r 0.0 |
| +4%, a period of 1.05 rounds | all shuffled | 1.8 (0.0) 10.6/19.4/70.4/81.5 r 6.7 | 1.5 (0.0) 10.2/22.4/60.7/63.6 r 5.6 | 0.0 (0.0) 12.6/21.8/60.8/100.0 r 0.0 |
| +4%, a period of 1.2 rounds | serpentine | 0.0 (0.0) 0.0/0.0/0.0/0.0 r 0.0 | 0.0 (0.0) 0.0/0.0/0.0/1.0 r 0.0 | 0.0 (0.0) 0.0/0.0/0.0/96.5 r 0.0 |
| +4%, a period of 1.2 rounds | middle shuffled | 0.9 (0.0) 0.0/0.0/42.9/49.3 r 2.2 | 0.8 (0.0) 0.0/0.0/0.0/1.4 r 3.2 | 0.7 (0.0) 3.4/8.5/16.6/96.5 r 2.7 |
| +4%, a period of 1.2 rounds | all shuffled | 3.2 (0.0) 16.8/23.6/59.5/66.7 r 9.3 | 2.9 (0.0) 13.3/17.2/59.9/68.7 r 7.0 | 0.1 (0.0) 37.0/49.8/67.8/100.0 r 0.6 |
| +4%, a period of 2 rounds | serpentine | 25.3 (0.0) 82.6/85.7/84.7/100.0 r 39.6 | 27.6 (0.0) 84.7/89.6/93.4/100.0 r 38.1 | 20.2 (0.0) 81.8/78.1/89.4/100.0 r 34.2 |
| +4%, a period of 2 rounds | middle shuffled | 13.9 (0.0) 39.5/46.2/83.1/100.0 r 30.1 | 19.0 (0.0) 42.0/49.8/89.3/100.0 r 33.7 | 16.6 (0.0) 31.4/34.3/67.5/100.0 r 30.2 |
| +4%, a period of 2 rounds | all shuffled | 2.9 (0.0) 50.8/68.1/88.8/100.0 r 6.9 | 0.0 (0.0) 50.8/58.9/71.4/69.9 r 0.0 | 0.0 (0.0) 65.6/70.7/82.7/100.0 r 0.0 |
| even, 2% | serpentine | 0.9 (0.3) 2.1/5.0/28.7/71.1 r 3.3 | 0.4 (0.2) 1.4/4.8/41.6/94.2 r 1.3 | 0.0 (0.0) 0.3/3.7/63.3/100.0 r 0.2 |
| even, 2% | middle shuffled | 0.9 (0.2) 1.5/4.9/27.7/72.1 r 3.2 | 0.4 (0.0) 1.0/4.8/43.5/95.2 r 1.5 | 0.0 (0.0) 0.5/4.4/62.7/100.0 r 0.1 |
| even, 2% | all shuffled | 0.9 (0.1) 1.9/5.4/27.7/70.0 r 3.6 | 0.4 (0.1) 1.1/5.2/45.0/95.5 r 1.5 | 0.1 (0.0) 0.4/4.5/63.3/100.0 r 0.3 |
| even, 3% | serpentine | 0.9 (58.0) 1.6/2.7/13.7/44.8 r 4.2 | 0.4 (91.6) 0.8/1.9/16.4/68.1 r 1.6 | 0.0 (99.0) 0.2/1.0/23.4/91.9 r 0.0 |
| even, 3% | middle shuffled | 0.9 (58.9) 1.1/2.3/12.4/45.2 r 4.4 | 0.4 (91.7) 0.7/2.4/17.3/69.3 r 2.4 | 0.0 (98.7) 0.1/1.4/23.2/91.0 r 0.0 |
| even, 3% | all shuffled | 0.9 (57.7) 1.2/2.8/12.6/43.8 r 4.8 | 0.4 (91.7) 0.5/2.2/18.1/69.4 r 2.4 | 0.1 (98.6) 0.2/1.6/23.0/90.0 r 2.4 |
";

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
| even, 1% | 0.9 (0.0) 4.8/18.8/69.6/98.3 | 0.8 (0.0) 5.8/22.8/83.7/99.9 | 0.4 (0.0) 5.2/27.6/94.9/100.0 | 0.1 (0.0) 3.7/38.4/99.9/100.0 |
| skewed | 0.7 (0.0) 3.0/12.2/53.6/91.0 | 0.5 (0.0) 4.2/15.5/67.1/97.3 | 0.2 (0.0) 2.9/18.2/81.2/99.7 | 0.0 (0.0) 1.9/25.3/97.0/100.0 |
| a third late by a fifth | 0.5 (98.9) 6.0/11.6/16.9/18.9 | 0.4 (99.7) 9.6/21.6/27.0/30.9 | 0.2 (99.8) 7.7/17.2/20.0/22.8 | 0.0 (100.0) 10.2/18.1/23.2/28.8 |
| half late by a fifth | 0.7 (100.0) 2.1/3.2/5.3/8.6 | 0.7 (100.0) 2.1/5.3/7.5/13.3 | 0.3 (100.0) 1.3/2.2/4.5/8.6 | 0.0 (100.0) 0.3/0.9/2.6/7.2 |
| bursts, a third of the time, ~8 renders | 0.1 (73.6) 39.5/59.2/61.6/63.1 | 0.1 (80.1) 49.8/74.8/80.8/81.1 | 0.0 (81.6) 61.4/77.8/78.2/79.9 | 0.0 (88.1) 90.1/94.7/95.2/94.7 |
| bursts, half the time, ~8 renders | 0.3 (95.8) 26.7/42.3/50.1/49.5 | 0.1 (98.0) 33.6/57.5/64.3/67.2 | 0.0 (98.9) 43.8/62.5/62.6/65.6 | 0.0 (99.9) 75.4/85.4/85.0/86.7 |
| bursts, half the time, ~40 renders | 0.0 (56.8) 56.6/81.7/85.5/85.1 | 0.0 (68.9) 71.3/96.2/97.0/96.5 | 0.0 (75.1) 86.2/95.9/96.4/96.2 | 0.0 (87.9) 99.4/99.8/99.9/99.8 |
| a slow drift of a tenth | 0.1 (75.4) 14.7/53.3/99.3/100.0 | 0.1 (100.0) 17.9/65.2/100.0/100.0 | 0.0 (100.0) 41.7/94.6/100.0/100.0 | 0.0 (100.0) 83.7/100.0/100.0/100.0 |
";
}
