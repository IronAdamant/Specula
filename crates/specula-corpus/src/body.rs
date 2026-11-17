//! A body: a sex, a stature, a weight, and the traits that tell it from others like it.
//!
//! Every dimension of a body follows from those by the survey's rows ([`crate::ansur`]): the
//! average of the people of that stature and weight, moved by each trait as that trait moves
//! it in the people measured. Because every row is computed over the same people, whatever
//! adds up on a person adds up on a body: floor to ankle, shank, thigh, trunk and head give
//! the stature again, to a hundredth of a millimetre.
//!
//! ```no_run
//! use specula_corpus::{Body, Sex};
//! // A tall woman of average build, with long legs and narrow shoulders for her height.
//! let body = Body::with_build(Sex::Female, 1.78, 0.0)?.with_trait("legs", 1.5)?.with_trait("shoulders", -1.0)?;
//! let frame = body.frame();
//! assert!((frame.stature() - 1.78).abs() < 1e-4);
//! # Ok::<(), specula_corpus::BodyError>(())
//! ```
//!
//! **Where it stops.** The people measured were soldiers of 17 to 58 years. A body may be as
//! short and as tall as the shortest and the tallest of its sex among them, as light and as
//! heavy for its stature as the lowest and the highest body mass index among them allow, of
//! an age they had, and three standard deviations either way on a trait. Past that the rows
//! are straight lines drawn beyond anyone measured, and [`Body`] refuses. A giant or a child
//! is not a body of this table: a giant can be a frame scaled, and a child needs its own rows
//! (docs/research/library/A2-proportions-by-age.md).
//!
//! **Every body made is a body.** The rows are straight lines, and many traits pushed far at
//! once can carry them where hardly anyone is: the navel above the bottom of the ribs (two of
//! the 4,082 men measured have it there, by a millimetre or two). So a body
//! is checked as it is made ([`BodyError::Folded`], [`BodyError::NotPositive`] say which part
//! gave way). With the traits no further than [`ALWAYS_A_BODY`] from the average together
//! (the root of the sum of their squares: one at 3, or four at 2, or all thirteen at 1.1)
//! that cannot happen, at any stature, weight and age a body may have: a test proves it for
//! the whole range, not for samples of it. About three people in four of those measured are
//! that near the average; the rest are bodies too, and nearly all of them pass the check.

use crate::ansur::{self, at, Row, Sex, TRAIT_COUNT};
use crate::frame::{Frame, Joint, Side};
use crate::inertia::KNUCKLE;

/// How far one trait may be from the average, in standard deviations.
pub const TRAIT_LIMIT: f64 = 3.0;
/// With its traits together no further than this from the average (the root of the sum of
/// their squares, age apart), a body of any stature, weight and age that may be had is never
/// refused as [`BodyError::Folded`] or [`BodyError::NotPositive`].
pub const ALWAYS_A_BODY: f64 = 4.0;
/// The least one of the frame's heights must be over the one below it, millimetres.
pub const LEAST_GAP: f64 = 1.0;

/// The frame's heights from the floor up, as the rows name them: each must be over the one
/// before it.
const UPWARD: [&str; 9] = ["lateral_malleolus_height", "knee_height", "hip_height", "waist_height_omphalion", "tenth_rib_height", "chest_height", "cervicale_height", "tragion_height", "stature"];
/// The ages the people measured had, years.
pub const AGES: [f64; 2] = [17.0, 58.0];

/// How far along the foot the ankle is, from the back of the heel, as a share of the foot's
/// length.
///
/// The survey did not measure it. A survey of 580 students' feet in Urmia, Iran (Hajaghazadeh
/// et al. 2018, Health Scope 7(3):e14063, Tables 1 and 2) gives the heel to the lateral
/// malleolus as 0.26 of the foot's length on men and 0.18 on women, and the heel to the medial
/// one as 0.31 and 0.23. The ankle's axis runs between the two bumps, so the middle of them
/// is taken: 0.285 on its men, 0.206 on its women. Its difference between the sexes is far
/// wider than any other ratio of the foot shows (the ball of the foot is 0.74 of its length on
/// both sexes in the survey used here), so one value serves both: their mean. The engine's
/// choice, from one hand-measured survey.
pub const ANKLE_ALONG_FOOT: f64 = 0.245;

const STATURE: usize = at("stature");
const WEIGHT: usize = at("weight");
const AGE: usize = at("age");

/// Why a body could not be made.
#[derive(Clone, Debug, PartialEq)]
pub enum BodyError {
    /// A number that is not one (NaN or infinite).
    NotANumber { what: &'static str },
    /// A stature (metres) outside what the rows cover for this sex.
    Stature { given: f64, least: f64, most: f64 },
    /// A weight (kilograms) outside what the rows cover at this stature.
    Weight { given: f64, least: f64, most: f64 },
    /// No trait has this name.
    UnknownTrait { name: String },
    /// A trait set further than the rows cover.
    Trait { name: String, given: f64, least: f64, most: f64 },
    /// Traits and an age that are each in range and together put one of the frame's heights
    /// (`above`, a dimension's name) under or level with the one that belongs below it: `gap`
    /// is how far it is over it, millimetres. `distance` is how far the traits are from the
    /// average together; at [`ALWAYS_A_BODY`] or less this cannot happen.
    Folded { above: &'static str, below: &'static str, gap: f64, distance: f64 },
    /// Traits and an age that are each in range and together make a dimension nothing or less.
    /// (A guard: no body asked for in any test or sweep of the reviews has reached it.)
    NotPositive { dimension: &'static str, value: f64, distance: f64 },
    /// An age (years) nobody measured had.
    Age { given: f64, least: f64, most: f64 },
}

impl BodyError {
    /// The error's kind as a word, for an answer that is data.
    pub fn code(&self) -> &'static str {
        match self {
            BodyError::NotANumber { .. } => "not_a_number",
            BodyError::Stature { .. } => "stature_out_of_range",
            BodyError::Weight { .. } => "weight_out_of_range",
            BodyError::UnknownTrait { .. } => "unknown_trait",
            BodyError::Trait { .. } => "trait_out_of_range",
            BodyError::Folded { .. } => "folded",
            BodyError::NotPositive { .. } => "not_positive",
            BodyError::Age { .. } => "age_out_of_range",
        }
    }
}

impl std::fmt::Display for BodyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BodyError::NotANumber { what } => write!(f, "the {what} is not a number"),
            BodyError::Stature { given, least, most } => write!(f, "a stature of {given:.3} m is outside the people measured ({least:.3} to {most:.3} m for this sex): build one inside and scale its frame"),
            BodyError::Weight { given, least, most } => write!(f, "a weight of {given:.1} kg is outside the people measured at this stature ({least:.1} to {most:.1} kg)"),
            BodyError::UnknownTrait { name } => {
                let known: Vec<&str> = ansur::traits().iter().map(|(n, _)| *n).collect();
                write!(f, "no trait is named '{name}': the traits are {}", known.join(", "))
            }
            BodyError::Trait { name, given, least, most } => write!(f, "the trait '{name}' at {given} is outside {least} to {most} (standard deviations)"),
            BodyError::Folded { above, below, gap, distance } => write!(
                f,
                "these traits together put {above} {gap:.1} mm over {below}, which belongs under it by {LEAST_GAP} mm at the least: hardly anyone is so. The traits are {distance:.2} standard deviations from the average together; at {ALWAYS_A_BODY} or less this cannot happen. Set fewer, or set them less far"
            ),
            BodyError::NotPositive { dimension, value, distance } => write!(
                f,
                "these traits together make {dimension} {value:.1}: nobody is so. The traits are {distance:.2} standard deviations from the average together; at {ALWAYS_A_BODY} or less this cannot happen. Set fewer, or set them less far"
            ),
            BodyError::Age { given, least, most } => write!(f, "an age of {given} is outside the people measured ({least} to {most} years)"),
        }
    }
}

impl std::error::Error for BodyError {}

/// A body. Lengths are in metres here; the table's are in millimetres.
#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    sex: Sex,
    /// Millimetres.
    stature: f64,
    /// Kilograms.
    weight: f64,
    traits: [f64; TRAIT_COUNT],
}

fn number(what: &'static str, x: f64) -> Result<f64, BodyError> {
    if x.is_finite() {
        Ok(x)
    } else {
        Err(BodyError::NotANumber { what })
    }
}

impl Body {
    /// The body at the mean stature and weight of its sex, with no trait set: every one of
    /// its dimensions is the survey's mean.
    pub fn typical(sex: Sex) -> Body {
        let rows = ansur::rows(sex);
        Body { sex, stature: rows[STATURE].mean, weight: rows[WEIGHT].mean, traits: [0.0; TRAIT_COUNT] }
    }

    /// A body of this stature (metres) and weight (kilograms).
    pub fn new(sex: Sex, stature: f64, weight: f64) -> Result<Body, BodyError> {
        let (stature, weight) = (number("stature", stature)?, number("weight", weight)?);
        let [least, most] = ansur::statures(sex).map(|mm| mm / 1000.0);
        if stature < least || stature > most {
            return Err(BodyError::Stature { given: stature, least, most });
        }
        let [least, most] = Body::weights(sex, stature);
        if weight < least || weight > most {
            return Err(BodyError::Weight { given: weight, least, most });
        }
        Ok(Body { sex, stature: stature * 1000.0, weight, traits: [0.0; TRAIT_COUNT] })
    }

    /// The least and the greatest weight (kilograms) a body of this stature (metres) may
    /// have: the body mass indices of the lightest and the heaviest person measured.
    pub fn weights(sex: Sex, stature: f64) -> [f64; 2] {
        ansur::mass_indices(sex).map(|index| index * stature * stature)
    }

    /// A body of this stature (metres) whose weight is `build` standard errors from the usual
    /// one for its stature: 0 is the average of the people that tall, 1 heavier than five in
    /// six of them, -1 lighter than five in six. Refused, as a weight, when it is lighter or
    /// heavier than anyone measured was for the stature.
    pub fn with_build(sex: Sex, stature: f64, build: f64) -> Result<Body, BodyError> {
        let (stature, build) = (number("stature", stature)?, number("build", build)?);
        // The stature is refused first: a build says nothing of a stature nobody had.
        Body::new(sex, stature, Body::weights(sex, stature)[0])?;
        let row = &ansur::rows(sex)[WEIGHT];
        let weight = row.at_stature(stature * 1000.0) + build * row.see;
        if !weight.is_finite() {
            let [least, most] = Body::weights(sex, stature);
            return Err(BodyError::Weight { given: weight, least, most });
        }
        Body::new(sex, stature, weight)
    }

    /// The same body with one trait set, in standard deviations ([`ansur::traits`] names
    /// them). `age` is a trait too, and easier set by [`Body::with_age`].
    ///
    /// The body is checked after each trait. Far from the average (never within
    /// [`ALWAYS_A_BODY`]) that makes the order matter: a body that would be whole with all its
    /// traits can be refused on the way to them. [`Body::with_traits`] sets several and checks
    /// once.
    pub fn with_trait(self, name: &str, value: f64) -> Result<Body, BodyError> {
        self.with_traits(&[(name, value)])
    }

    /// The same body with several traits set, checked once they all are.
    pub fn with_traits(mut self, traits: &[(&str, f64)]) -> Result<Body, BodyError> {
        for (name, value) in traits {
            let k = ansur::trait_index(name).ok_or_else(|| BodyError::UnknownTrait { name: (*name).to_owned() })?;
            let value = number("trait", *value)?;
            if !(-TRAIT_LIMIT..=TRAIT_LIMIT).contains(&value) && *name != "age" {
                return Err(BodyError::Trait { name: (*name).to_owned(), given: value, least: -TRAIT_LIMIT, most: TRAIT_LIMIT });
            }
            self.traits[k] = value;
        }
        let age = self.age();
        if traits.iter().any(|(name, _)| *name == "age") && (age < AGES[0] || age > AGES[1]) {
            return Err(BodyError::Age { given: age, least: AGES[0], most: AGES[1] });
        }
        self.checked()
    }

    /// The body, when it is one: every dimension more than nothing, and every height of the
    /// frame over the one below it.
    fn checked(self) -> Result<Body, BodyError> {
        let rows = ansur::rows(self.sex);
        let distance = self.traits_distance();
        for (index, row) in rows.iter().enumerate() {
            let value = self.of(row);
            if value.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
                return Err(BodyError::NotPositive { dimension: ansur::names()[index], value, distance });
            }
        }
        for pair in UPWARD.windows(2) {
            let (below, above) = (pair[0], pair[1]);
            let gap = self.dimension(above).unwrap_or(0.0) - self.dimension(below).unwrap_or(0.0);
            if gap.partial_cmp(&LEAST_GAP) != Some(std::cmp::Ordering::Greater) {
                return Err(BodyError::Folded { above, below, gap, distance });
            }
        }
        Ok(self)
    }

    /// How far the body's traits are from the average together, in standard deviations: the
    /// root of the sum of their squares, age apart.
    pub fn traits_distance(&self) -> f64 {
        let age = ansur::trait_index("age");
        self.traits.iter().enumerate().filter(|(k, _)| Some(*k) != age).map(|(_, z)| z * z).sum::<f64>().sqrt()
    }

    /// The same body at an age, in years. Among the people measured, age at the same stature
    /// and weight goes with a wider waist, a deeper chest and slimmer thighs and calves; this
    /// sets the `age` trait to what the age asks for.
    pub fn with_age(mut self, years: f64) -> Result<Body, BodyError> {
        let years = number("age", years)?;
        if years < AGES[0] || years > AGES[1] {
            return Err(BodyError::Age { given: years, least: AGES[0], most: AGES[1] });
        }
        let row = &ansur::rows(self.sex)[AGE];
        let k = ansur::trait_index("age").unwrap_or(0);
        self.traits[k] = (years - row.at(self.stature, self.weight)) / row.t[k];
        self.checked()
    }

    pub fn sex(&self) -> Sex {
        self.sex
    }

    /// Metres.
    pub fn stature(&self) -> f64 {
        self.stature / 1000.0
    }

    /// Kilograms.
    pub fn weight(&self) -> f64 {
        self.weight
    }

    /// The weight as standard errors from the usual one for the stature.
    pub fn build(&self) -> f64 {
        let row = &ansur::rows(self.sex)[WEIGHT];
        (self.weight - row.at_stature(self.stature)) / row.see
    }

    /// Years. With no age set, the average age of the people of this stature and weight.
    pub fn age(&self) -> f64 {
        self.value(AGE)
    }

    /// A trait's value, in standard deviations.
    pub fn trait_value(&self, name: &str) -> Option<f64> {
        ansur::trait_index(name).map(|k| self.traits[k])
    }

    fn of(&self, row: &Row) -> f64 {
        row.at(self.stature, self.weight) + row.t.iter().zip(self.traits).map(|(t, z)| t * z).sum::<f64>()
    }

    /// A dimension by its place in the rows, in the table's unit.
    pub(crate) fn value(&self, index: usize) -> f64 {
        self.of(&ansur::rows(self.sex)[index])
    }

    /// One of the table's dimensions on this body, in the table's unit: millimetres, and
    /// kilograms for `weight` and years for `age`.
    pub fn dimension(&self, name: &str) -> Option<f64> {
        ansur::index(name).map(|i| self.value(i))
    }

    /// How far people who share this body's stature, weight and traits are still spread on a
    /// dimension (one standard deviation, in the table's unit): what the body does not say.
    pub fn spread(&self, name: &str) -> Option<f64> {
        ansur::row(self.sex, name).map(|row| row.left)
    }

    /// The frame this body stands in: upright, the feet straight ahead and as far apart as
    /// the hips' centres, the arms hanging straight down from the shoulders' centres.
    ///
    /// The heights are the survey's, each joint's centre at de Leva's distance from its
    /// landmark. Where no survey says, the frame is plain about it:
    ///
    /// - everything is in the plane through the hips' centres except the feet: a spine's
    ///   curves and a neck's forward set are for a figure's maker to add;
    /// - each shoulder's centre is put straight under its acromion, half the biacromial
    ///   breadth from the middle;
    /// - a leg goes straight down from its hip's centre;
    /// - the ankle's place along the foot is [`ANKLE_ALONG_FOOT`].
    pub fn frame(&self) -> Frame {
        const HIP_HEIGHT: usize = at("hip_height");
        const HIP_SPACING: usize = at("hip_spacing");
        const KNEE_HEIGHT: usize = at("knee_height");
        const ANKLE_HEIGHT: usize = at("lateral_malleolus_height");
        const FOOT: usize = at("foot_length");
        const BALL: usize = at("ball_of_foot_length");
        const HEEL_BREADTH: usize = at("heel_breadth");
        const BALL_BREADTH: usize = at("foot_breadth_horizontal");
        const WAIST: usize = at("waist_height_omphalion");
        const RIBS: usize = at("tenth_rib_height");
        const CHEST: usize = at("chest_height");
        const NECK: usize = at("cervicale_height");
        const HEAD: usize = at("tragion_height");
        const SHOULDER_HEIGHT: usize = at("shoulder_height");
        const SHOULDERS: usize = at("biacromial_breadth");
        const UPPER_ARM: usize = at("upper_arm");
        const FOREARM: usize = at("forearm");
        const HAND: usize = at("hand");
        let m = |index: usize| self.value(index) / 1000.0;
        let mut frame = Frame::empty();
        frame.heel_breadth = m(HEEL_BREADTH);
        frame.ball_breadth = m(BALL_BREADTH);
        // (The heights of UPWARD, which `checked` holds in their order.)
        for (joint, index) in [(Joint::Pelvis, HIP_HEIGHT), (Joint::Waist, WAIST), (Joint::Ribs, RIBS), (Joint::Chest, CHEST), (Joint::Neck, NECK), (Joint::Head, HEAD), (Joint::Vertex, STATURE)] {
            frame.set(joint, [0.0, m(index), 0.0]);
        }
        let heel = -ANKLE_ALONG_FOOT * m(FOOT);
        for side in Side::BOTH {
            let x = side.sign() * m(HIP_SPACING) / 2.0;
            frame.set(Joint::Hip(side), [x, m(HIP_HEIGHT), 0.0]);
            frame.set(Joint::Knee(side), [x, m(KNEE_HEIGHT), 0.0]);
            frame.set(Joint::Ankle(side), [x, m(ANKLE_HEIGHT), 0.0]);
            frame.set(Joint::Heel(side), [x, 0.0, heel]);
            frame.set(Joint::Ball(side), [x, 0.0, heel + m(BALL)]);
            frame.set(Joint::Toe(side), [x, 0.0, heel + m(FOOT)]);
            let x = side.sign() * m(SHOULDERS) / 2.0;
            let shoulder = m(SHOULDER_HEIGHT);
            let (elbow, wrist) = (shoulder - m(UPPER_ARM), shoulder - m(UPPER_ARM) - m(FOREARM));
            frame.set(Joint::Shoulder(side), [x, shoulder, 0.0]);
            frame.set(Joint::Elbow(side), [x, elbow, 0.0]);
            frame.set(Joint::Wrist(side), [x, wrist, 0.0]);
            frame.set(Joint::Knuckle(side), [x, wrist - KNUCKLE[self.sex.index()] * m(HAND), 0.0]);
            frame.set(Joint::Fingertip(side), [x, wrist - m(HAND), 0.0]);
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_typical_body_is_the_surveys_mean_on_every_dimension() {
        for sex in Sex::BOTH {
            let body = Body::typical(sex);
            for name in ansur::names() {
                let row = ansur::row(sex, name).unwrap();
                // Six digits of each of three numbers, two of them multiplied by about a thousand.
                assert!((body.dimension(name).unwrap() - row.mean).abs() < 0.02, "{name}: {} against {}", body.dimension(name).unwrap(), row.mean);
            }
            assert!(body.build().abs() < 0.01 && (body.age() - ansur::row(sex, "age").unwrap().mean).abs() < 0.01);
        }
        assert_eq!(Body::typical(Sex::Female).dimension("tail_length"), None);
    }

    /// A body adds up: each part of its frame is as long as the row for that part says, so the
    /// rows of the parts, which were fitted one by one, give the stature again between them;
    /// and a trait that lengthens the legs shortens the rest by as much.
    #[test]
    fn a_body_adds_up_whatever_its_traits() {
        use crate::inertia::distance;
        for sex in Sex::BOTH {
            for (stature, build) in [(1.55, -1.0), (1.70, 0.0), (1.82, 2.0)] {
                let plain = Body::with_build(sex, stature, build).unwrap();
                let mut body = plain.clone();
                for (k, (name, _)) in ansur::traits().iter().enumerate().skip(1) {
                    body = body.with_trait(name, [1.2, -1.4, 0.7, -0.4, 1.1, -0.9][k % 6]).unwrap();
                }
                assert!(body.traits_distance() > 3.0 && body.traits_distance() < ALWAYS_A_BODY, "{}", body.traits_distance());
                for body in [plain, body] {
                    let frame = body.frame();
                    let mm = |name: &str| body.dimension(name).unwrap() / 1000.0;
                    let (left, between) = (Side::Left, |a: Joint, b: Joint| distance(frame.at(a), frame.at(b)));
                    // Each from its own row: none of these rows is another's sum by construction here.
                    for (what, length, row) in [
                        ("shank", between(Joint::Knee(left), Joint::Ankle(left)), "shank"),
                        ("thigh", between(Joint::Hip(left), Joint::Knee(left)), "thigh"),
                        ("trunk", frame.trunk(), "trunk"),
                        ("head and neck", between(Joint::Neck, Joint::Vertex), "vertex_to_cervicale"),
                        ("neck", between(Joint::Neck, Joint::Head), "neck_link"),
                        ("upper arm", between(Joint::Shoulder(left), Joint::Elbow(left)), "upper_arm"),
                        ("forearm", between(Joint::Elbow(left), Joint::Wrist(left)), "forearm"),
                        ("arm", between(Joint::Shoulder(left), Joint::Wrist(left)), "arm"),
                        ("hips", between(Joint::Hip(left), Joint::Hip(Side::Right)), "hip_spacing"),
                    ] {
                        assert!((length - mm(row)).abs() < 5e-5, "{} {stature} {build}: the {what} is {length} in the frame and {} in its row", sex.name(), mm(row));
                    }
                    let parts = mm("lateral_malleolus_height") + mm("shank") + mm("thigh") + mm("trunk") + mm("vertex_to_cervicale");
                    assert!((parts - stature).abs() < 5e-5 && (frame.stature() - stature).abs() < 5e-5, "{parts} {}", frame.stature());
                }
            }
        }
        let (short, long) = (Body::typical(Sex::Male).with_trait("legs", -2.0).unwrap(), Body::typical(Sex::Male).with_trait("legs", 2.0).unwrap());
        let d = |b: &Body, name: &str| b.dimension(name).unwrap();
        assert!(d(&long, "hip_height") - d(&short, "hip_height") > 90.0, "four standard deviations of leg are about a hand's length");
        let rest = |b: &Body| d(b, "trunk") + d(b, "vertex_to_cervicale");
        assert!(((d(&long, "hip_height") - d(&short, "hip_height")) + (rest(&long) - rest(&short))).abs() < 0.02, "the trunk and the head give back what the legs take");
        assert_eq!(short.stature(), long.stature());
    }

    /// Each joint of the frame is the dimension its documentation says, written here a second
    /// time: a joint read from the wrong row moves no sum and would pass every other test.
    #[test]
    fn each_joint_of_the_frame_is_read_from_its_own_row() {
        for sex in Sex::BOTH {
            let body = Body::with_build(sex, 1.72, 0.5).unwrap().with_trait("legs", 0.8).unwrap().with_trait("feet", -1.0).unwrap();
            let (frame, m) = (body.frame(), |name: &str| body.dimension(name).unwrap() / 1000.0);
            for (joint, row) in [
                (Joint::Pelvis, "hip_height"),
                (Joint::Waist, "waist_height_omphalion"),
                (Joint::Ribs, "tenth_rib_height"),
                (Joint::Chest, "chest_height"),
                (Joint::Neck, "cervicale_height"),
                (Joint::Head, "tragion_height"),
                (Joint::Vertex, "stature"),
                (Joint::Shoulder(Side::Right), "shoulder_height"),
                (Joint::Hip(Side::Right), "hip_height"),
                (Joint::Knee(Side::Right), "knee_height"),
                (Joint::Ankle(Side::Right), "lateral_malleolus_height"),
            ] {
                assert!((frame.at(joint)[1] - m(row)).abs() < 1e-12, "{}: {} against {row} at {}", joint.name(), frame.at(joint)[1], m(row));
            }
            // The foot: the heel a quarter of its length (0.245) behind the ankle, the ball and
            // the toe ahead of the heel by their own rows; the soles as broad as theirs.
            for side in Side::BOTH {
                let (heel, ball, toe) = (frame.at(Joint::Heel(side)), frame.at(Joint::Ball(side)), frame.at(Joint::Toe(side)));
                assert!((heel[2] + 0.245 * m("foot_length")).abs() < 1e-12 && (ball[2] - heel[2] - m("ball_of_foot_length")).abs() < 1e-12 && (toe[2] - heel[2] - m("foot_length")).abs() < 1e-12);
                assert!(heel[1] == 0.0 && ball[1] == 0.0 && toe[1] == 0.0 && heel[0] == frame.at(Joint::Hip(side))[0]);
                assert!((frame.at(Joint::Shoulder(side))[0] - side.sign() * m("biacromial_breadth") / 2.0).abs() < 1e-12 && (frame.at(Joint::Hip(side))[0] - side.sign() * m("hip_spacing") / 2.0).abs() < 1e-12);
                let (wrist, knuckle, tip) = (frame.at(Joint::Wrist(side))[1], frame.at(Joint::Knuckle(side))[1], frame.at(Joint::Fingertip(side))[1]);
                assert!((wrist - tip - m("hand")).abs() < 1e-12 && ((wrist - knuckle) / (wrist - tip) - [78.0 / 170.1, 86.2 / 187.9][sex.index()]).abs() < 1e-12);
            }
            assert!((frame.heel_breadth - m("heel_breadth")).abs() < 1e-12 && (frame.ball_breadth - m("foot_breadth_horizontal")).abs() < 1e-12);
        }
    }

    /// What the traits say of the people measured, in plain words.
    #[test]
    fn traits_move_what_goes_with_them() {
        let body = Body::typical(Sex::Female);
        let d = |b: &Body, name: &str| b.dimension(name).unwrap();
        let wide = body.clone().with_trait("shoulders", 2.0).unwrap();
        assert!(d(&wide, "biacromial_breadth") - d(&body, "biacromial_breadth") > 25.0);
        assert!(d(&wide, "bideltoid_breadth") > d(&body, "bideltoid_breadth"), "the flesh over them follows the bones");
        assert!((d(&wide, "hip_height") - d(&body, "hip_height")).abs() < 1.0, "the legs do not: they were taken out first");
        // Heavier at the same stature: every girth grows, no bone lengthens much.
        let heavy = Body::with_build(Sex::Female, body.stature(), 2.0).unwrap();
        for girth in ["waist_circumference", "chest_circumference", "thigh_circumference", "calf_circumference", "neck_circumference", "buttock_circumference"] {
            assert!(d(&heavy, girth) > d(&body, girth) + 15.0, "{girth}");
        }
        assert!((d(&heavy, "radiale_stylion_length") - d(&body, "radiale_stylion_length")).abs() < 5.0);
        // Older at the same stature and weight: more waist, by the people measured.
        let (young, old) = (body.clone().with_age(20.0).unwrap(), body.clone().with_age(50.0).unwrap());
        assert!((young.age() - 20.0).abs() < 1e-9 && (old.age() - 50.0).abs() < 1e-9);
        assert!(d(&old, "waist_circumference") > d(&young, "waist_circumference") + 20.0);
        assert_eq!(d(&old, "stature"), d(&young, "stature"));
        assert_eq!(body.trait_value("legs"), Some(0.0));
        assert!(body.spread("ear_length").unwrap() > 3.0 && body.spread("hip_height").unwrap() < 1e-6);
    }

    #[test]
    fn a_body_outside_the_people_measured_is_refused_with_the_range() {
        assert_eq!(Body::new(Sex::Male, 2.4, 100.0).unwrap_err().code(), "stature_out_of_range");
        assert_eq!(Body::new(Sex::Female, 1.0, 30.0).unwrap_err().code(), "stature_out_of_range");
        let thin = Body::new(Sex::Female, 1.65, 30.0).unwrap_err();
        assert!(matches!(thin, BodyError::Weight { least, most, .. } if least > 40.0 && least < 50.0 && most > 100.0), "{thin}");
        assert_eq!(Body::with_build(Sex::Male, 1.8, 6.0).unwrap_err().code(), "weight_out_of_range");
        assert_eq!(Body::with_build(Sex::Male, 1.8, f64::INFINITY).unwrap_err().code(), "not_a_number");
        // The lightest and the heaviest a stature may be are bodies.
        let [least, most] = Body::weights(Sex::Female, 1.65);
        assert!(Body::new(Sex::Female, 1.65, least).is_ok() && Body::new(Sex::Female, 1.65, most).is_ok() && Body::new(Sex::Female, 1.65, most + 0.1).is_err());
        assert_eq!(Body::new(Sex::Male, f64::NAN, 80.0).unwrap_err().code(), "not_a_number");
        let body = Body::typical(Sex::Male);
        let unknown = body.clone().with_trait("tail", 1.0).unwrap_err();
        assert!(unknown.to_string().contains("legs") && unknown.code() == "unknown_trait", "{unknown}");
        assert_eq!(body.clone().with_trait("legs", 3.5).unwrap_err().code(), "trait_out_of_range");
        assert_eq!(Body::with_build(Sex::Male, 3.0, f64::MAX).unwrap_err().code(), "stature_out_of_range", "the stature is refused before the build is read");
        assert_eq!(Body::with_build(Sex::Male, 1.8, f64::MAX).unwrap_err().code(), "weight_out_of_range");
        assert_eq!(body.clone().with_age(70.0).unwrap_err().code(), "age_out_of_range");
        assert_eq!(body.clone().with_trait("age", 9.0).unwrap_err().code(), "age_out_of_range");
        assert!(body.clone().with_trait("legs", -3.0).is_ok() && body.with_age(58.0).is_ok());
    }

    /// The least a sum of rows less another can be on any body whose traits are no further
    /// than `together` from the average.
    ///
    /// Every dimension is a plane over stature and weight, plus a straight line in each trait.
    /// So its least over the traits is found, not sampled: all of them together at `together` pull
    /// it down by that times the length of its row of traits, and the age pulls furthest at
    /// one end of its range. Over stature and weight it is a straight line in the weight
    /// (least at the lightest or the heaviest the stature allows), and the stature is walked
    /// in steps of two or three millimetres (a walk a hundred times finer lowers no bound by
    /// a thousandth of a millimetre: the second review measured it).
    fn least(sex: Sex, plus: &[&str], minus: &[&str], together: f64) -> f64 {
        let rows = |names: &[&str]| names.iter().map(|n| *ansur::row(sex, n).unwrap()).collect::<Vec<_>>();
        let (plus, minus) = (rows(plus), rows(minus));
        let net = |of: &dyn Fn(&Row) -> f64| plus.iter().map(of).sum::<f64>() - minus.iter().map(of).sum::<f64>();
        let age = ansur::row(sex, "age").unwrap();
        let pull = (1..TRAIT_COUNT).map(|k| net(&|r| r.t[k]) * net(&|r| r.t[k])).sum::<f64>().sqrt();
        let [shortest, tallest] = ansur::statures(sex);
        let mut least = f64::INFINITY;
        for step in 0..=200 {
            let stature = shortest + (tallest - shortest) * f64::from(step) / 200.0;
            for weight in Body::weights(sex, stature / 1000.0) {
                for years in AGES {
                    let z = (years - age.at(stature, weight)) / age.t[0];
                    least = least.min(net(&|r| r.at(stature, weight)) + net(&|r| r.t[0]) * z - together * pull);
                }
            }
        }
        least
    }

    /// With its traits within [`ALWAYS_A_BODY`] together, whatever body may be asked for is a
    /// body: every dimension is positive and every height of the frame is over the one below
    /// it, so the check that refuses a folded body never fires. Proved for the whole range
    /// ([`least`]), not for samples of it. And a little further out it is no longer so, which
    /// is why the check is there.
    #[test]
    fn near_the_average_every_body_that_can_be_asked_for_is_a_body() {
        for sex in Sex::BOTH {
            for name in ansur::names() {
                let lowest = least(sex, &[name], &[], ALWAYS_A_BODY);
                assert!(lowest > 0.0, "{}: {name} can be {lowest}", sex.name());
            }
            for pair in UPWARD.windows(2) {
                let gap = least(sex, &[pair[1]], &[pair[0]], ALWAYS_A_BODY);
                assert!(gap > LEAST_GAP + 1.0, "{}: {} is as little as {gap} mm over {}", sex.name(), pair[1], pair[0]);
            }
            for limb in ["upper_arm", "forearm", "hand", "thigh", "shank", "foot_length", "hip_spacing", "biacromial_breadth", "heel_breadth"] {
                let shortest = least(sex, &[limb], &[], ALWAYS_A_BODY);
                assert!(shortest > 30.0, "{}: {limb} can be {shortest} mm", sex.name());
            }
            // The shoulders' centres stay under the base of the neck, and the fingertips over the knees.
            assert!(least(sex, &["cervicale_height"], &["shoulder_height"], ALWAYS_A_BODY) > 20.0);
            assert!(least(sex, &["shoulder_height"], &["upper_arm", "forearm", "hand", "knee_height"], ALWAYS_A_BODY) > 0.0);
        }
        // The guarantee is not far from as wide as it can be: a unit further out, a man's navel
        // can be over the bottom of his ribs.
        assert!(least(Sex::Male, &["tenth_rib_height"], &["waist_height_omphalion"], ALWAYS_A_BODY + 1.0) < 0.0);
    }

    /// The worst body the proof speaks of is built, for each gap of the frame: at the edge of
    /// the guarantee it is made and has the gap the proof says; pushed further the same way it
    /// folds, and is refused by name.
    #[test]
    fn the_worst_body_is_made_at_the_edge_and_refused_beyond_it() {
        let mut folded = 0;
        for sex in Sex::BOTH {
            for pair in UPWARD.windows(2) {
                let (below, above) = (ansur::row(sex, pair[0]).unwrap(), ansur::row(sex, pair[1]).unwrap());
                let pull: Vec<f64> = (1..TRAIT_COUNT).map(|k| above.t[k] - below.t[k]).collect();
                let length = pull.iter().map(|p| p * p).sum::<f64>().sqrt();
                let proved = least(sex, &[pair[1]], &[pair[0]], ALWAYS_A_BODY);
                let mut worst = f64::INFINITY;
                let [shortest, tallest] = ansur::statures(sex);
                for step in 0..=20 {
                    let stature = (shortest + (tallest - shortest) * f64::from(step) / 20.0) / 1000.0;
                    for weight in Body::weights(sex, stature) {
                        for years in AGES {
                            // Every trait against the gap, all of them together at `far`.
                            let pushed = |far: f64| {
                                let mut body = Body::new(sex, stature, weight)?.with_age(years)?;
                                for (k, (name, _)) in ansur::traits().iter().enumerate().skip(1) {
                                    body = body.with_trait(name, (-pull[k - 1] / length * far).clamp(-TRAIT_LIMIT, TRAIT_LIMIT))?;
                                }
                                Ok::<Body, BodyError>(body)
                            };
                            let body = pushed(ALWAYS_A_BODY).unwrap_or_else(|e| panic!("{} {stature} {weight} {years}: {e}", sex.name()));
                            assert!(body.traits_distance() <= ALWAYS_A_BODY + 1e-9 && (body.age() - years).abs() < 1e-9);
                            let gap = body.dimension(pair[1]).unwrap() - body.dimension(pair[0]).unwrap();
                            assert!(gap >= proved - 0.01, "{}: {} over {} is {gap} mm on {body:?}, under the {proved} proved", sex.name(), pair[1], pair[0]);
                            worst = worst.min(gap);
                            match pushed(3.0 * ALWAYS_A_BODY) {
                                Err(BodyError::Folded { above, below, gap, distance }) => {
                                    assert!(gap <= LEAST_GAP && distance > ALWAYS_A_BODY && UPWARD.contains(&above) && UPWARD.contains(&below), "{above} {below} {gap} {distance}");
                                    folded += 1;
                                }
                                Err(other) => panic!("{other}"),
                                Ok(body) => {
                                    let frame = body.frame();
                                    let heights = [Joint::Ankle(Side::Left), Joint::Knee(Side::Left), Joint::Pelvis, Joint::Waist, Joint::Ribs, Joint::Chest, Joint::Neck, Joint::Head, Joint::Vertex].map(|j| frame.at(j)[1]);
                                    assert!(heights.windows(2).all(|w| w[1] > w[0] + LEAST_GAP / 1000.0 - 1e-12), "a body that was made has its joints in order: {heights:?}");
                                }
                            }
                        }
                    }
                }
                // The walk above is coarser than the proof's, so it finds the proof's worst or a
                // little over it; unless one trait alone would have to go past its own limit
                // (the thigh is nearly all legs), where the proof allows more than can be asked.
                let one_past_its_limit = pull.iter().any(|p| (p / length * ALWAYS_A_BODY).abs() > TRAIT_LIMIT);
                assert!(one_past_its_limit || worst < proved + 12.0,"{}: {} over {}: the worst built is {worst} mm, the proof says {proved}", sex.name(), pair[1], pair[0]);
            }
        }
        assert!(folded > 10, "some bodies pushed three times as far fold, and are refused: {folded}");
        // An age can be what folds a body, so setting one is checked too; and several traits
        // set together are checked once, where one by one the order would decide.
        let far = |up: &[&str]| ansur::traits().iter().skip(1).map(|(name, _)| (*name, if up.contains(name) { 3.0 } else { -3.0 })).collect::<Vec<_>>();
        let [_, heaviest] = Body::weights(Sex::Female, 1.409);
        let small = Body::new(Sex::Female, 1.409, heaviest).unwrap();
        let whole = small.clone().with_traits(&far(&["legs", "shoulders", "chest", "waist", "neck"])).unwrap();
        assert_eq!(whole.with_age(58.0).unwrap_err().code(), "folded");
        let together = far(&["legs", "chest", "waist"]);
        assert!(small.clone().with_traits(&together).is_ok());
        let mut one_by_one = Ok(small);
        for (name, value) in &together {
            one_by_one = one_by_one.and_then(|body| body.with_trait(name, *value));
        }
        assert_eq!(one_by_one.unwrap_err().code(), "folded", "refused on the way, at a body the rest of the traits would have mended");
    }
}
