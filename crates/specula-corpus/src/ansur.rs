//! The measured body: 1,986 women and 4,082 men on 114 dimensions (93 measured, their age, and
//! 20 links made of those), and how each is spread.
//!
//! The source is ANSUR II, the US Army's anthropometric survey of 2010 to 2012 (Gordon et al.
//! 2014, *2012 Anthropometric Survey of U.S. Army Personnel: Methods and Summary Statistics*,
//! NATICK/TR-15/007; a US Government work, in the public domain). The numbers in
//! [`crate::ansur_data`] are **computed from the survey's two public data files** by
//! `examen corpus tables` (tools/examen/src/corpus.rs), never typed; the tests here hold a
//! sample of them against the survey's own report.
//!
//! **Who was measured**: soldiers aged 17 to 58, fit for service. Nobody under 17, nobody old,
//! few who are very heavy or very slight. A body far outside them is an extrapolation
//! ([`crate::body::Body`] says where it stops).
//!
//! A row is one dimension of one sex: lengths in millimetres, `weight` in kilograms, `age` in
//! years.
//!
//! - The 93 dimensions the survey measured keep its names with the words apart
//!   (`acromion_radiale_length`), and its definitions.
//! - `age`, from the same files.
//! - 20 links made of them ([`link`] says how each): the survey's own, by subtraction, and
//!   the skeleton's, between joint centres. A joint's centre is not where a tape lands: how
//!   far each lies from its landmark is de Leva's (1996, J Biomech 29(9):1223-1230, Table 2),
//!   scaled to the person's stature.

use crate::ansur_data as data;

/// How many traits a body has besides its stature and its weight.
pub const TRAIT_COUNT: usize = 14;

/// One dimension of one sex.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Row {
    /// The mean of the people measured.
    pub mean: f64,
    /// Their standard deviation.
    pub sd: f64,
    /// Their 5th percentile.
    pub p5: f64,
    /// Their 95th percentile.
    pub p95: f64,
    /// On stature alone, `a + b * stature` is what a person of that stature has on average.
    pub a: f64,
    pub b: f64,
    /// How far people of one stature are spread around that (the standard error of estimate).
    pub see: f64,
    /// The correlation with stature.
    pub r: f64,
    /// On stature and weight: `c[0] + c[1] * stature + c[2] * weight`.
    pub c: [f64; 3],
    /// How far people of one stature and weight are spread around that.
    pub see2: f64,
    /// What one standard deviation of each trait adds ([`traits`] has their order).
    pub t: [f64; TRAIT_COUNT],
    /// The spread the traits leave unexplained: what tells two people apart who have the same
    /// stature, weight and traits.
    pub left: f64,
}

impl Row {
    /// What a person of this stature (millimetres) has on average.
    pub fn at_stature(&self, stature: f64) -> f64 {
        self.a + self.b * stature
    }

    /// What a person of this stature (millimetres) and weight (kilograms) has on average.
    pub fn at(&self, stature: f64, weight: f64) -> f64 {
        self.c[0] + self.c[1] * stature + self.c[2] * weight
    }
}

/// The two sexes the survey tabulates. A body is built from one sex's rows; the survey holds
/// nothing between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Sex {
    Female,
    Male,
}

impl Sex {
    pub const BOTH: [Sex; 2] = [Sex::Female, Sex::Male];

    pub(crate) fn index(self) -> usize {
        match self {
            Sex::Female => 0,
            Sex::Male => 1,
        }
    }

    pub fn name(self) -> &'static str {
        ["female", "male"][self.index()]
    }

    pub fn parse(name: &str) -> Option<Sex> {
        Sex::BOTH.into_iter().find(|s| s.name() == name)
    }
}

/// Every dimension's name, in the order of the rows.
pub fn names() -> &'static [&'static str] {
    &data::NAMES
}

pub(crate) fn index(name: &str) -> Option<usize> {
    data::NAMES.iter().position(|n| *n == name)
}

pub(crate) fn rows(sex: Sex) -> &'static [Row] {
    match sex {
        Sex::Female => &data::FEMALE,
        Sex::Male => &data::MALE,
    }
}

/// One dimension's row, by name.
pub fn row(sex: Sex, name: &str) -> Option<&'static Row> {
    index(name).map(|i| &rows(sex)[i])
}

/// A dimension's place in the rows, found while compiling: a name that is not in the table
/// does not build.
pub(crate) const fn at(name: &str) -> usize {
    let (want, mut i) = (name.as_bytes(), 0);
    while i < data::NAMES.len() {
        let have = data::NAMES[i].as_bytes();
        if have.len() == want.len() {
            let mut k = 0;
            while k < have.len() && have[k] == want[k] {
                k += 1;
            }
            if k == have.len() {
                return i;
            }
        }
        i += 1;
    }
    panic!("not a dimension of the table")
}

/// How many people a sex's rows are computed from.
pub fn people(sex: Sex) -> u32 {
    data::PEOPLE[sex.index()]
}

/// The least and the greatest stature among the people measured, millimetres.
pub fn statures(sex: Sex) -> [f64; 2] {
    data::STATURES[sex.index()]
}

/// The least and the greatest body mass index among the people measured: kilograms over the
/// square of the stature in metres.
pub fn mass_indices(sex: Sex) -> [f64; 2] {
    data::MASS_INDICES[sex.index()]
}

/// What a link is made of, when `name` is one.
pub fn link(name: &str) -> Option<&'static str> {
    data::LINKS.iter().find(|(n, _)| *n == name).map(|(_, how)| *how)
}

/// The traits in their order, each with the dimension it is read on.
///
/// A trait is what is left of its dimension once stature, weight and the traits before it are
/// accounted for. So in the people measured the traits are uncorrelated with each other and
/// with stature and weight (not more than that: age, for one, is not spread as a bell, most
/// soldiers being young); one unit of a trait is one standard deviation of what was left; and a
/// trait moves every dimension that goes with it in those people, not only its own (long legs
/// come with a longer thigh and a longer shank, and with a shorter trunk at the same stature).
pub fn traits() -> &'static [(&'static str, &'static str)] {
    &data::TRAITS
}

/// A trait's place in [`Row::t`].
pub fn trait_index(name: &str) -> Option<usize> {
    data::TRAITS.iter().position(|(n, _)| *n == name)
}

/// The hip centres' spacing as a share of the bicristal breadth (the link `hip_spacing`).
pub fn hip_share(sex: Sex) -> f64 {
    data::HIP_SHARE[sex.index()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_has_the_shape_the_code_expects() {
        assert_eq!(data::TRAITS.len(), TRAIT_COUNT);
        assert_eq!((people(Sex::Female), people(Sex::Male)), (1986, 4082));
        assert_eq!(names().len(), 114);
        let mut sorted: Vec<&str> = names().to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names().len(), "a name twice");
        for (name, _) in &data::LINKS {
            assert!(index(name).is_some(), "{name}");
        }
        for (name, driver) in traits() {
            assert!(index(driver).is_some(), "{name} is read on {driver}, which is not a dimension");
        }
        assert_eq!((Sex::parse("female"), Sex::parse("male"), Sex::parse("")), (Some(Sex::Female), Some(Sex::Male), None));
    }

    /// The survey's own report prints each dimension's mean, standard deviation and 5th and
    /// 95th percentiles, in centimetres to two places. These are read from its data pages
    /// (docs/research/library/A1-segment-lengths.md, Tables A1-1 and A1-2, which give the
    /// page of each): what is computed here from the data files must round to them.
    #[test]
    fn the_computed_rows_are_the_reports() {
        // (dimension, women: mean, sd, p5, p95; men: the same), centimetres or kilograms.
        let report: [(&str, [f64; 4], [f64; 4]); 22] = [
            ("stature", [162.85, 6.42, 152.50, 174.00], [175.62, 6.86, 164.80, 187.00]),
            ("sitting_height", [85.66, 3.31, 80.30, 91.20], [91.83, 3.57, 86.00, 97.70]),
            ("cervicale_height", [139.57, 5.95, 130.00, 149.80], [151.73, 6.33, 141.60, 162.30]),
            ("suprasternale_height", [132.97, 5.72, 123.60, 142.80], [143.86, 6.12, 134.10, 154.00]),
            ("acromial_height", [133.51, 5.81, 124.20, 143.40], [144.07, 6.33, 133.90, 154.60]),
            ("tenth_rib_height", [105.63, 5.00, 97.60, 114.00], [112.07, 5.36, 103.40, 121.10]),
            ("iliocristale_height", [99.57, 4.97, 91.80, 108.00], [106.16, 5.20, 97.70, 114.80]),
            ("trochanterion_height", [84.54, 4.47, 77.40, 91.90], [90.09, 4.92, 82.40, 98.50]),
            ("tibial_height", [43.78, 2.50, 39.90, 47.90], [46.82, 2.66, 42.60, 51.30]),
            ("lateral_malleolus_height", [6.27, 0.51, 5.40, 7.10], [7.29, 0.57, 6.40, 8.30]),
            ("acromion_radiale_length", [31.12, 1.72, 28.30, 34.00], [33.52, 1.75, 30.70, 36.50]),
            ("radiale_stylion_length", [24.13, 1.52, 21.80, 26.80], [26.79, 1.54, 24.40, 29.50]),
            ("hand_length", [18.11, 1.01, 16.50, 19.90], [19.33, 0.99, 17.70, 21.00]),
            ("foot_length", [24.63, 1.24, 22.60, 26.80], [27.12, 1.31, 25.00, 29.30]),
            ("biacromial_breadth", [36.53, 1.83, 33.50, 39.60], [41.57, 1.92, 38.40, 44.70]),
            ("hip_breadth", [35.38, 2.67, 31.10, 40.00], [34.57, 2.42, 30.80, 38.70]),
            ("bicristal_breadth", [27.33, 2.23, 23.70, 31.00], [27.54, 1.75, 24.70, 30.50]),
            ("span", [166.03, 8.30, 152.90, 180.30], [181.42, 8.46, 168.00, 195.70]),
            ("weight", [67.76, 10.98, 51.30, 87.10], [85.52, 14.22, 64.40, 110.70]),
            // The report's links, made here by the same subtractions.
            ("thigh_link", [37.94, 2.40, 34.00, 41.80], [40.93, 2.84, 36.40, 45.80]),
            ("calf_link", [40.32, 2.59, 36.10, 44.60], [41.88, 2.46, 37.90, 46.00]),
            ("tragion_height", [150.20, 6.31, 140.20, 160.90], [162.51, 6.76, 151.70, 173.70]),
        ];
        for (name, women, men) in report {
            for (sex, printed) in [(Sex::Female, women), (Sex::Male, men)] {
                let row = row(sex, name).unwrap();
                // The table is in millimetres; the report rounds to a tenth of one. Weight is
                // in kilograms in both.
                let unit = if name == "weight" { 1.0 } else { 10.0 };
                let computed = [row.mean, row.sd, row.p5, row.p95];
                for (what, (c, p)) in ["mean", "sd", "p5", "p95"].iter().zip(computed.iter().zip(printed)) {
                    // Half of the report's last digit for a mean and a spread. A percentile is
                    // found between two people here and may be smoothed there: a third of a
                    // millimetre covers every one of these (the widest is a quarter).
                    let within = if what.starts_with('p') { 0.03 } else { 0.0051 };
                    assert!((c / unit - p).abs() <= within, "{name}, {}: the {what} is {} here and {p} in the report", sex.name(), c / unit);
                }
            }
        }
    }

    /// The fitted lines were computed independently for the library research (Table A1-3,
    /// from the same public files): the slope and intercept on stature, the spread around the
    /// line, and r.
    #[test]
    fn the_fitted_lines_are_the_researchs() {
        // (dimension, women: a, b, see, r; men: the same).
        let lines: [(&str, [f64; 4], [f64; 4]); 8] = [
            ("acromion_radiale_length", [-34.0, 0.2120, 10.5, 0.79], [-20.9, 0.2028, 10.6, 0.80]),
            ("radiale_stylion_length", [-24.5, 0.1632, 11.1, 0.69], [-14.4, 0.1608, 10.8, 0.71]),
            ("hand_length", [16.2, 0.1012, 7.7, 0.65], [21.7, 0.0977, 7.4, 0.67]),
            ("thigh_link", [-42.5, 0.2591, 17.4, 0.69], [-76.8, 0.2768, 21.2, 0.67]),
            ("calf_link", [-90.4, 0.3031, 17.1, 0.75], [-98.1, 0.2943, 14.0, 0.82]),
            ("vertex_to_cervicale", [90.4, 0.0874, 10.5, 0.47], [78.1, 0.0916, 11.2, 0.49]),
            ("subischial_leg", [-215.8, 0.6065, 21.3, 0.88], [-205.8, 0.5943, 22.4, 0.88]),
            ("span", [-63.6, 1.0586, 47.6, 0.82], [29.4, 1.0162, 48.1, 0.82]),
        ];
        for (name, women, men) in lines {
            for (sex, [a, b, see, r]) in [(Sex::Female, women), (Sex::Male, men)] {
                let row = row(sex, name).unwrap();
                assert!((row.a - a).abs() <= 0.051 && (row.b - b).abs() <= 0.000051 && (row.see - see).abs() <= 0.051 && (row.r - r).abs() <= 0.0051, "{name}, {}: {row:?}", sex.name());
            }
        }
    }

    /// What holds of every person holds of the rows: a link that is a sum or a difference of
    /// dimensions has the sum or the difference of their means, their lines and their traits.
    /// This is what lets a body made from the rows add up from the floor to the top of the head.
    #[test]
    fn sums_stay_sums_in_every_column() {
        let identities: [(&str, &[&str], &[&str]); 6] = [
            ("stature", &["lateral_malleolus_height", "shank", "thigh", "trunk", "vertex_to_cervicale"], &[]),
            ("arm", &["upper_arm", "forearm"], &[]),
            ("thigh", &["hip_height"], &["knee_height"]),
            ("neck_link", &["tragion_height"], &["cervicale_height"]),
            ("subischial_leg", &["stature"], &["sitting_height"]),
            ("trunk", &["cervicale_height"], &["hip_height"]),
        ];
        for sex in Sex::BOTH {
            for (whole, plus, minus) in identities {
                let whole_row = row(sex, whole).unwrap();
                let sum = |of: &dyn Fn(&Row) -> f64| plus.iter().map(|n| of(row(sex, n).unwrap())).sum::<f64>() - minus.iter().map(|n| of(row(sex, n).unwrap())).sum::<f64>();
                // Six digits are printed, so a sum of five is off by a few units of the last.
                let near = |a: f64, b: f64, what: &str| assert!((a - b).abs() <= 0.02, "{whole}, {}: {what} is {a}, its parts give {b}", sex.name());
                near(whole_row.mean, sum(&|r| r.mean), "the mean");
                near(whole_row.c[0], sum(&|r| r.c[0]), "c0");
                assert!((whole_row.c[1] - sum(&|r| r.c[1])).abs() <= 1e-5 && (whole_row.c[2] - sum(&|r| r.c[2])).abs() <= 1e-4, "{whole}");
                for k in 0..TRAIT_COUNT {
                    near(whole_row.t[k], sum(&|r| r.t[k]), traits()[k].0);
                }
            }
        }
    }

    /// A trait is all of what was left of its own dimension, and leaves nothing of it to the
    /// traits after it; stature and weight have nothing left at all.
    #[test]
    fn each_trait_uses_up_its_own_dimension() {
        for sex in Sex::BOTH {
            for (k, (name, driver)) in traits().iter().enumerate() {
                let row = row(sex, driver).unwrap();
                assert!(row.t[k] > 0.0 && row.left.abs() < 1e-6 && row.t[k + 1..].iter().all(|t| t.abs() < 1e-6), "{name}: {row:?}");
            }
            for fixed in ["stature", "weight"] {
                let row = row(sex, fixed).unwrap();
                assert!(row.t.iter().all(|t| t.abs() < 1e-6) && row.see2 < 1e-6 && row.left < 1e-6);
            }
        }
    }
}
