//! What a figure is held to: its proportions against the people measured, and its balance.
//!
//! Both read a [`Frame`], so they serve a body made here, a figure an agent modelled, and a
//! drawn one alike. Neither judges: a hero eight heads tall is out of proportion on purpose.
//! They say how far, in the survey's own spread, and what is made of that is the caller's
//! (a project's style says what it allows).

use crate::ansur::{self, at, Sex};
use crate::frame::{Frame, Joint, Side};
use crate::inertia::{centre_of_mass, distance, P3};

/// One length of a figure against the people measured.
#[derive(Clone, Debug, PartialEq)]
pub struct Proportion {
    /// What was measured: `thigh`, `shoulders`.
    pub name: &'static str,
    /// The side, for a part the body has two of.
    pub side: Option<Side>,
    /// The figure's, metres.
    pub measured: f64,
    /// What people of the figure's stature have on average, metres.
    pub expected: f64,
    /// How far people of that stature are spread around it (one standard error), metres.
    pub spread: f64,
    /// How many of those spreads the figure is from the average: beyond 2 either way is
    /// rarer than one person in twenty, beyond 3 than one in three hundred.
    pub z: f64,
}

/// A figure's lengths against people of its own stature and of `sex`.
///
/// The stature is the frame's own ([`Frame::stature`]: the sum of its parts), and every length
/// is measured along the joints between its ends (a hand by way of its knuckle, a foot by way
/// of its ball, the trunk along the spine), so a pose does not change the answer: not a lean,
/// a fist or a foot on its toes. Each length is held to the survey's line on stature for it.
/// Compared: the head and neck (cervicale to the top of the head), the trunk along the spine,
/// the upper arm, forearm, hand, thigh and shank between joint centres, the ankle's height,
/// the foot's length, the breadth between the shoulders' centres (against the biacromial
/// breadth) and between the hips' centres.
///
/// Not compared, because the survey has no row for it: how many heads tall the figure is (the
/// survey measured no chin-to-top height), and anything of a child or of old age.
pub fn proportions(sex: Sex, frame: &Frame) -> Vec<Proportion> {
    let stature = frame.stature();
    let rows = ansur::rows(sex);
    let mut out = Vec::new();
    let mut hold = |name: &'static str, side: Option<Side>, index: usize, measured: f64| {
        let row = &rows[index];
        let (expected, spread) = (row.at_stature(stature * 1000.0) / 1000.0, row.see / 1000.0);
        out.push(Proportion { name, side, measured, expected, spread, z: (measured - expected) / spread });
    };
    let between = |a: Joint, b: Joint| distance(frame.at(a), frame.at(b));
    hold("head_and_neck", None, at("vertex_to_cervicale"), between(Joint::Neck, Joint::Vertex));
    hold("trunk", None, at("trunk"), frame.trunk());
    hold("shoulders", None, at("biacromial_breadth"), between(Joint::Shoulder(Side::Left), Joint::Shoulder(Side::Right)));
    hold("hips", None, at("hip_spacing"), between(Joint::Hip(Side::Left), Joint::Hip(Side::Right)));
    for side in Side::BOTH {
        hold("upper_arm", Some(side), at("upper_arm"), between(Joint::Shoulder(side), Joint::Elbow(side)));
        hold("forearm", Some(side), at("forearm"), between(Joint::Elbow(side), Joint::Wrist(side)));
        hold("hand", Some(side), at("hand"), frame.hand(side));
        hold("thigh", Some(side), at("thigh"), between(Joint::Hip(side), Joint::Knee(side)));
        hold("shank", Some(side), at("shank"), between(Joint::Knee(side), Joint::Ankle(side)));
        hold("ankle_height", Some(side), at("lateral_malleolus_height"), frame.ankle_height(side));
        hold("foot", Some(side), at("foot_length"), frame.foot(side));
    }
    out
}

/// Whether a posed figure stands.
#[derive(Clone, Debug, PartialEq)]
pub struct Balance {
    /// The centre of mass, metres.
    pub centre: P3,
    /// The ground's height: the lowest point of either foot.
    pub ground: f64,
    /// Which feet are on it (left, right).
    pub planted: [bool; 2],
    /// What the figure stands on, seen from above: the corners of the area its planted soles
    /// span, as `[x, z]`, in order around it (counter-clockwise when drawn with +X to the right
    /// and +Z up the page). Fewer than three when it stands on a line or a point.
    pub support: Vec<[f64; 2]>,
    /// How far the centre of mass is inside that area, metres, measured on the ground:
    /// positive inside (the least distance to its edge), negative outside (how far out). A
    /// figure that is not moving stands when it is positive, and the nearer zero the nearer
    /// it is to tipping.
    pub margin: f64,
}

/// A foot counts as on the ground when its lowest point is within this of the ground, metres.
pub const PLANTED_WITHIN: f64 = 0.01;

/// Weighs a posed figure ([`crate::inertia::parts`]) and says where its centre of mass falls
/// against what its feet cover.
///
/// The ground is taken at the figure's lowest foot point, level. A foot's sole is the quad
/// around its heel and its ball, as broad as the frame says at each, and reaching to the toe
/// when the toe is down too. **A still figure only**: one that runs or leaps is off balance
/// on purpose between steps, and whether that motion holds is the motion checks' to say.
pub fn balance(sex: Sex, frame: &Frame) -> Balance {
    let centre = centre_of_mass(sex, frame);
    let points = |side: Side| [Joint::Heel(side), Joint::Ball(side), Joint::Toe(side)].map(|j| frame.at(j));
    let ground = Side::BOTH.into_iter().flat_map(points).map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let (mut corners, mut planted) = (Vec::new(), [false; 2]);
    for (i, side) in Side::BOTH.into_iter().enumerate() {
        let [heel, ball, toe] = points(side);
        let down = |p: P3| p[1] <= ground + PLANTED_WITHIN;
        planted[i] = down(heel) || down(ball) || down(toe);
        // Across the foot, on the ground: a quarter turn of the line from heel to toe.
        let (dx, dz) = (toe[0] - heel[0], toe[2] - heel[2]);
        let length = (dx * dx + dz * dz).sqrt();
        let across = if length > 0.0 { [dz / length, -dx / length] } else { [1.0, 0.0] };
        for (point, breadth, is_down) in [(heel, frame.heel_breadth, down(heel)), (ball, frame.ball_breadth, down(ball)), (toe, 0.0, down(toe))] {
            if is_down {
                for half in [-breadth / 2.0, breadth / 2.0] {
                    corners.push([point[0] + across[0] * half, point[2] + across[1] * half]);
                }
            }
        }
    }
    let support = hull(corners);
    let margin = inside(&support, [centre[0], centre[2]]);
    Balance { centre, ground, planted, support, margin }
}

fn cross(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// The convex hull of some points (Andrew's monotone chain), without points on its edges.
fn hull(mut points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let mut out: Vec<[f64; 2]> = Vec::with_capacity(points.len() + 1);
    for pass in 0..2 {
        let start = out.len();
        let walk: Box<dyn Iterator<Item = &[f64; 2]>> = if pass == 0 { Box::new(points.iter()) } else { Box::new(points.iter().rev()) };
        for p in walk {
            while out.len() >= start + 2 && cross(out[out.len() - 2], out[out.len() - 1], *p) <= 0.0 {
                out.pop();
            }
            out.push(*p);
        }
        out.pop();
    }
    out
}

fn to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (ab, ap) = ([b[0] - a[0], b[1] - a[1]], [p[0] - a[0], p[1] - a[1]]);
    let length2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if length2 > 0.0 { ((ap[0] * ab[0] + ap[1] * ab[1]) / length2).clamp(0.0, 1.0) } else { 0.0 };
    let (dx, dz) = (p[0] - a[0] - ab[0] * t, p[1] - a[1] - ab[1] * t);
    (dx * dx + dz * dz).sqrt()
}

/// How far a point is inside a convex area (positive) or outside it (negative). An area with
/// no inside (a line, a point, nothing) has every point outside or on it.
fn inside(area: &[[f64; 2]], p: [f64; 2]) -> f64 {
    match area.len() {
        0 => f64::NEG_INFINITY,
        1 => -to_segment(p, area[0], area[0]),
        2 => -to_segment(p, area[0], area[1]),
        n => {
            let edges = (0..n).map(|i| (area[i], area[(i + 1) % n]));
            let within = edges.clone().all(|(a, b)| cross(a, b, p) >= 0.0);
            let nearest = edges.map(|(a, b)| to_segment(p, a, b)).fold(f64::INFINITY, f64::min);
            if within {
                nearest
            } else {
                -nearest
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::Body;

    /// Turns every joint above the ankles about the line through the ankles (the X axis at
    /// their height): a figure leaning as one piece, feet flat.
    fn lean(frame: &Frame, degrees: f64) -> Frame {
        let (sin, cos) = specula_core::dmath::sin_cos_f64(degrees * std::f64::consts::PI / 180.0);
        let pivot = frame.at(Joint::Ankle(Side::Left))[1];
        let mut out = frame.clone();
        for joint in Joint::all() {
            if matches!(joint, Joint::Heel(_) | Joint::Ball(_) | Joint::Toe(_) | Joint::Ankle(_)) {
                continue;
            }
            let [x, y, z] = frame.at(joint);
            let (y, z) = (y - pivot, z);
            out.set(joint, [x, pivot + y * cos - z * sin, y * sin + z * cos]);
        }
        out
    }

    #[test]
    fn a_measured_body_is_in_proportion_and_a_stretched_one_is_not() {
        for sex in Sex::BOTH {
            let frame = Body::typical(sex).frame();
            for p in proportions(sex, &frame) {
                assert!(p.z.abs() < 0.05, "{} {:?}: {p:?}", p.name, p.side);
                assert!(p.spread > 0.0 && p.measured > 0.0);
            }
            assert_eq!(proportions(sex, &frame).len(), 18);
            // Long legs for the height, by two standard deviations of the trait: the check
            // says so of the thigh and the shank, and that the trunk is short.
            let long = Body::typical(sex).with_trait("legs", 2.0).unwrap().frame();
            let found = proportions(sex, &long);
            let z = |name: &str| found.iter().find(|p| p.name == name).unwrap().z;
            assert!(z("thigh") > 1.0 && z("shank") > 1.0 && z("trunk") < -1.0, "{} {} {}", z("thigh"), z("shank"), z("trunk"));
            assert!(z("foot").abs() < 0.6);
        }
        // A figure with the forearms of a much taller person.
        let mut frame = Body::typical(Sex::Male).frame();
        for side in Side::BOTH {
            for joint in [Joint::Wrist(side), Joint::Knuckle(side), Joint::Fingertip(side)] {
                let [x, y, z] = frame.at(joint);
                frame.set(joint, [x, y - 0.06, z]);
            }
        }
        let found = proportions(Sex::Male, &frame);
        let forearm = found.iter().find(|p| p.name == "forearm" && p.side == Some(Side::Right)).unwrap();
        assert!(forearm.z > 5.0 && (forearm.measured - forearm.expected - 0.06).abs() < 0.002, "{forearm:?}");
        assert!(found.iter().filter(|p| p.z.abs() > 1.0).count() == 2, "only the two forearms: {found:?}");
        // The pose does not matter: leaning changes no length, and neither does a fist, a foot
        // risen on its toes, or a bent back.
        let mut posed = lean(&frame, 20.0);
        for side in Side::BOTH {
            let (knuckle, tip) = (posed.at(Joint::Knuckle(side)), posed.at(Joint::Fingertip(side)));
            let fingers = distance(knuckle, tip);
            posed.set(Joint::Fingertip(side), [knuckle[0], knuckle[1], knuckle[2] + fingers]);
            // The heel and the ankle turn about the ball (a 3-4-5 turn), and everything over
            // the ankle rides up with it.
            let (ball, ankle) = (posed.at(Joint::Ball(side)), posed.at(Joint::Ankle(side)));
            let turn = |p: P3| [p[0], ball[1] + (p[1] - ball[1]) * 0.8 - (p[2] - ball[2]) * 0.6, ball[2] + (p[1] - ball[1]) * 0.6 + (p[2] - ball[2]) * 0.8];
            let risen = turn(ankle);
            posed.set(Joint::Heel(side), turn(posed.at(Joint::Heel(side))));
            posed.set(Joint::Ankle(side), risen);
            let knee = posed.at(Joint::Knee(side));
            let shank = distance(ankle, knee);
            posed.set(Joint::Knee(side), [risen[0], risen[1] + shank, risen[2]]);
            let hip = posed.at(Joint::Hip(side));
            posed.set(Joint::Hip(side), [risen[0], risen[1] + shank + distance(knee, hip), risen[2]]);
        }
        // The back bends at the ribs: what is over them leans a further 3-4-5 turn forward.
        let ribs = posed.at(Joint::Ribs);
        for joint in [Joint::Chest, Joint::Neck, Joint::Head, Joint::Vertex] {
            let p = posed.at(joint);
            posed.set(joint, [p[0], ribs[1] + (p[1] - ribs[1]) * 0.8 - (p[2] - ribs[2]) * 0.6, ribs[2] + (p[1] - ribs[1]) * 0.6 + (p[2] - ribs[2]) * 0.8]);
        }
        assert!(posed.at(Joint::Heel(Side::Left))[1] > 0.05, "the heels are off the ground");
        let again = proportions(Sex::Male, &posed);
        for (a, b) in found.iter().zip(&again) {
            assert!((a.z - b.z).abs() < 1e-6, "{}: {} when standing, {} when posed", a.name, a.z, b.z);
        }
    }

    /// The centre of mass of a body standing straight is a little over half its height up, and
    /// over its feet; leaning from the ankles carries it past the toes at an angle a person
    /// cannot hold either.
    #[test]
    fn a_standing_body_balances_and_a_leaning_one_tips() {
        for sex in Sex::BOTH {
            let body = Body::typical(sex);
            let frame = body.frame();
            let found = balance(sex, &frame);
            assert_eq!(found.planted, [true, true]);
            assert!(found.ground.abs() < 1e-12 && found.support.len() >= 4, "{found:?}");
            // Where the table's shares and the survey's joints put it: 55.9% of the stature up
            // on the typical woman and 55.8% on the typical man, and a millimetre ahead of the
            // ankles (the feet reach forward). Held this closely so that a share, a centre or
            // a joint that changes is seen here.
            let up = found.centre[1] / body.stature();
            assert!((up - [0.559, 0.558][sex.index()]).abs() < 0.0006, "{}: the centre of mass is {up} of the stature up", sex.name());
            assert!(found.centre[0].abs() < 1e-9, "a symmetric body's is in the middle");
            assert!(found.centre[2] > 0.0005 && found.centre[2] < 0.002, "{}", found.centre[2]);
            assert!(found.margin > 0.02, "{}: {found:?}", sex.name());
            // Forward: the toes are about three quarters of a foot ahead of the ankle.
            let mut tipped_at = None;
            for degrees in 1..30 {
                if balance(sex, &lean(&frame, f64::from(degrees))).margin < 0.0 {
                    tipped_at = Some(degrees);
                    break;
                }
            }
            let forward = tipped_at.unwrap();
            assert!((8..=16).contains(&forward), "{}: tips forward at {forward} degrees", sex.name());
            // Backward: the heel is a quarter of a foot behind it, so it tips sooner.
            let backward = (1..30).find(|d| balance(sex, &lean(&frame, -f64::from(*d))).margin < 0.0).unwrap();
            assert!(backward < forward && backward >= 2, "{}: tips backward at {backward} degrees", sex.name());
        }
    }

    #[test]
    fn standing_on_one_foot_narrows_what_holds_the_body() {
        let frame = Body::typical(Sex::Female).frame();
        let both = balance(Sex::Female, &frame);
        // Lift the right foot a hand's breadth, as it is.
        let mut one = frame.clone();
        for joint in [Joint::Knee(Side::Right), Joint::Ankle(Side::Right), Joint::Heel(Side::Right), Joint::Ball(Side::Right), Joint::Toe(Side::Right)] {
            let [x, y, z] = one.at(joint);
            one.set(joint, [x, y + 0.1, z]);
        }
        let lifted = balance(Sex::Female, &one);
        assert_eq!(lifted.planted, [true, false]);
        assert!(lifted.margin < 0.0, "the centre of mass is between the feet, not over the left one: {lifted:?}");
        assert!(both.margin > 0.0);
        // Shift the whole body over the left foot, and it stands.
        let shift = one.at(Joint::Ankle(Side::Left))[0] - lifted.centre[0];
        let mut over = one.clone();
        for joint in Joint::all() {
            if !matches!(joint, Joint::Heel(Side::Left) | Joint::Ball(Side::Left) | Joint::Toe(Side::Left) | Joint::Ankle(Side::Left)) {
                let [x, y, z] = one.at(joint);
                over.set(joint, [x + shift, y, z]);
            }
        }
        assert!(balance(Sex::Female, &over).margin > 0.0);
        // On the balls of the feet: the heels hold nothing, and the centre of mass, over the
        // ankles, is behind what does.
        let mut tiptoe = frame.clone();
        for side in Side::BOTH {
            let [x, y, z] = tiptoe.at(Joint::Heel(side));
            tiptoe.set(Joint::Heel(side), [x, y + 0.06, z]);
        }
        let raised = balance(Sex::Female, &tiptoe);
        assert!(raised.support.len() >= 3 && raised.margin < 0.0, "{raised:?}");
    }

    #[test]
    fn the_area_under_a_figure_is_found_and_measured() {
        let square = hull(vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0], [1.0, 1.0], [1.0, 0.0], [2.0, 2.0]]);
        assert_eq!(square, vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]);
        assert_eq!(inside(&square, [1.0, 1.0]), 1.0);
        assert_eq!(inside(&square, [0.5, 1.0]), 0.5);
        assert_eq!(inside(&square, [3.0, 1.0]), -1.0);
        assert_eq!(inside(&square, [2.0, 1.0]), 0.0);
        assert!((inside(&square, [3.0, 3.0]) + 2.0f64.sqrt()).abs() < 1e-12);
        assert_eq!(inside(&hull(vec![[0.0, 0.0], [2.0, 0.0]]), [1.0, 1.0]), -1.0);
        assert_eq!(inside(&hull(vec![[0.0, 0.0], [0.0, 0.0]]), [3.0, 4.0]), -5.0);
        assert_eq!(inside(&hull(Vec::new()), [0.0, 0.0]), f64::NEG_INFINITY);
        // Three points in a line are a line.
        assert_eq!(hull(vec![[0.0, 0.0], [1.0, 1.0], [2.0, 2.0]]).len(), 2);
    }
}
