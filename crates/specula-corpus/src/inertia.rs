//! What each part of the body weighs, and where its weight sits.
//!
//! The source is de Leva (1996), *Adjustments to Zatsiorsky-Seluyanov's segment inertia
//! parameters*, J Biomech 29(9):1223-1230, Table 4: each segment's mass as a share of the
//! body's, its centre of mass as a share of its length from its first endpoint, and its three
//! radii of gyration as shares of its length. The data under it are Zatsiorsky's gamma-ray
//! scans of 100 men and 15 women, students of physical education of mean ages 24 and 19;
//! de Leva moved their reference points to joint centres. **Means only: the paper gives no
//! spread**, and the women are few.
//!
//! Every number below was read on the image of the printed page (and agrees with
//! docs/research/library/A4-segment-masses-com.md, Table A4-1).
//!
//! The rows used are the ones whose endpoints a skeleton has: joint centres for the limbs,
//! and for the head and the trunk's three parts the paper's alternatives that run from the
//! cervicale (the bone felt at the base of the neck) instead of the top of the breastbone.

use crate::ansur::Sex;
use crate::frame::{Frame, Joint, Side};

/// A point's coordinates, metres.
pub type P3 = [f64; 3];

/// One segment's row of the paper's table: women first, then men.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub name: &'static str,
    /// The endpoints, in the paper's words: the share of length is measured from the first.
    pub from: &'static str,
    pub to: &'static str,
    /// The segment's length on the paper's subjects, millimetres.
    pub length: [f64; 2],
    /// Its mass, as a percentage of the body's.
    pub mass: [f64; 2],
    /// Its centre of mass, as a percentage of its length from the first endpoint.
    pub centre: [f64; 2],
    /// Its radii of gyration about its sagittal, transverse and longitudinal axes, as
    /// percentages of its length.
    pub gyration: [[f64; 2]; 3],
    /// Whether the body has one on each side.
    pub paired: bool,
}

/// The ten segments (sixteen with both sides) the body's mass is shared among.
pub const SEGMENTS: [Segment; 10] = [
    Segment { name: "head", from: "vertex", to: "cervicale", length: [243.7, 242.9], mass: [6.68, 6.94], centre: [48.41, 50.02], gyration: [[27.1, 30.3], [29.5, 31.5], [26.1, 26.1]], paired: false },
    Segment { name: "upper_trunk", from: "cervicale", to: "xiphoid", length: [228.0, 242.1], mass: [15.45, 15.96], centre: [50.50, 50.66], gyration: [[46.6, 50.5], [31.4, 32.0], [44.9, 46.5]], paired: false },
    Segment { name: "middle_trunk", from: "xiphoid", to: "navel", length: [205.3, 215.5], mass: [14.65, 16.33], centre: [45.12, 45.02], gyration: [[43.3, 48.2], [35.4, 38.3], [41.5, 46.8]], paired: false },
    Segment { name: "lower_trunk", from: "navel", to: "between the hip centres", length: [181.5, 145.7], mass: [12.47, 11.17], centre: [49.20, 61.15], gyration: [[43.3, 61.5], [40.2, 55.1], [44.4, 58.7]], paired: false },
    Segment { name: "upper_arm", from: "shoulder centre", to: "elbow centre", length: [275.1, 281.7], mass: [2.55, 2.71], centre: [57.54, 57.72], gyration: [[27.8, 28.5], [26.0, 26.9], [14.8, 15.8]], paired: true },
    Segment { name: "forearm", from: "elbow centre", to: "wrist centre", length: [264.3, 268.9], mass: [1.38, 1.62], centre: [45.59, 45.74], gyration: [[26.1, 27.6], [25.7, 26.5], [9.4, 12.1]], paired: true },
    Segment { name: "hand", from: "wrist centre", to: "tip of the middle finger", length: [170.1, 187.9], mass: [0.56, 0.61], centre: [34.27, 36.24], gyration: [[24.4, 28.8], [20.8, 23.5], [15.4, 18.4]], paired: true },
    Segment { name: "thigh", from: "hip centre", to: "knee centre", length: [368.5, 422.2], mass: [14.78, 14.16], centre: [36.12, 40.95], gyration: [[36.9, 32.9], [36.4, 32.9], [16.2, 14.9]], paired: true },
    Segment { name: "shank", from: "knee centre", to: "lateral malleolus", length: [432.3, 434.0], mass: [4.81, 4.33], centre: [44.16, 44.59], gyration: [[27.1, 25.5], [26.7, 24.9], [9.3, 10.3]], paired: true },
    Segment { name: "foot", from: "heel", to: "tip of the longest toe", length: [228.3, 258.1], mass: [1.29, 1.37], centre: [40.14, 44.15], gyration: [[29.9, 25.7], [27.9, 24.5], [13.9, 12.4]], paired: true },
];

/// The middle of the hand's knuckles (the third metacarpale) as a share of the way from the
/// wrist's centre to the fingertip: the paper's two hand lengths, 78.0 of 170.1 mm and 86.2
/// of 187.9 mm.
pub const KNUCKLE: [f64; 2] = [78.0 / 170.1, 86.2 / 187.9];

/// One part of a posed body: what share of the body's mass it is, and where that mass sits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub name: &'static str,
    /// The side, for a part the body has two of.
    pub side: Option<Side>,
    /// Its share of the body's mass, 0 to 1.
    pub share: f64,
    /// Its centre of mass, metres.
    pub centre: P3,
    /// Its length in this pose, metres.
    pub length: f64,
    /// Its radii of gyration about its sagittal, transverse and longitudinal axes, metres.
    pub gyration: [f64; 3],
}

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn lerp(a: P3, b: P3, t: f64) -> P3 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

pub(crate) fn distance(a: P3, b: P3) -> f64 {
    let d = sub(a, b);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// The point a share `t` of the way along a line of points, by length.
fn along(path: &[P3], t: f64) -> P3 {
    let whole: f64 = path.windows(2).map(|w| distance(w[0], w[1])).sum();
    let mut left = t.clamp(0.0, 1.0) * whole;
    for w in path.windows(2) {
        let step = distance(w[0], w[1]);
        if left <= step && step > 0.0 {
            return lerp(w[0], w[1], left / step);
        }
        left -= step;
    }
    path[path.len() - 1]
}

/// The sixteen parts of a posed body.
///
/// A limb's part lies between its two joints; the hand's follows its knuckle and the foot's
/// its ball, so a fist and a foot on its toes carry their weight where they are. The trunk's
/// three parts share the spine's line from the neck down to the pelvis in the proportions the
/// paper's subjects had (228.0, 205.3 and 181.5 mm of 614.8 on the women; 242.1, 215.5 and
/// 145.7 of 603.3 on the men), each following the spine's bends. **Those cuts are the
/// paper's, not the frame's joints**: its shares of mass belong to them. The lowest falls at
/// 0.295 of the trunk above the hips on the paper's women, where the frame's waist (the
/// navel of the survey's women) is at 0.241, three centimetres lower. The foot's centre is
/// put on its sole's line: the paper says how far along, not how high.
pub fn parts(sex: Sex, frame: &Frame) -> Vec<Part> {
    let s = sex.index();
    let mut out = Vec::with_capacity(16);
    let mut part = |segment: &Segment, side: Option<Side>, centre: P3, length: f64| {
        out.push(Part {
            name: segment.name,
            side,
            share: segment.mass[s] / 100.0,
            centre,
            length,
            gyration: [segment.gyration[0][s] / 100.0 * length, segment.gyration[1][s] / 100.0 * length, segment.gyration[2][s] / 100.0 * length],
        });
    };
    let (head, upper, middle, lower) = (&SEGMENTS[0], &SEGMENTS[1], &SEGMENTS[2], &SEGMENTS[3]);
    let (vertex, neck) = (frame.at(Joint::Vertex), frame.at(Joint::Neck));
    part(head, None, lerp(vertex, neck, head.centre[s] / 100.0), distance(vertex, neck));
    // The spine from the neck down, and where the paper's two cuts fall on it.
    let spine = [Joint::Neck, Joint::Chest, Joint::Ribs, Joint::Waist, Joint::Pelvis].map(|j| frame.at(j));
    let whole = upper.length[s] + middle.length[s] + lower.length[s];
    let cuts = [0.0, upper.length[s] / whole, (upper.length[s] + middle.length[s]) / whole, 1.0];
    let spine_length: f64 = spine.windows(2).map(|w| distance(w[0], w[1])).sum();
    for (i, segment) in [upper, middle, lower].into_iter().enumerate() {
        // The centre is found along the spine, so that a bent back carries its weight where it is.
        let at = cuts[i] + (cuts[i + 1] - cuts[i]) * segment.centre[s] / 100.0;
        let length = (cuts[i + 1] - cuts[i]) * spine_length;
        part(segment, None, along(&spine, at), length);
    }
    for side in Side::BOTH {
        let limb: [(usize, &[Joint]); 6] = [
            (4, &[Joint::Shoulder(side), Joint::Elbow(side)]),
            (5, &[Joint::Elbow(side), Joint::Wrist(side)]),
            (6, &[Joint::Wrist(side), Joint::Knuckle(side), Joint::Fingertip(side)]),
            (7, &[Joint::Hip(side), Joint::Knee(side)]),
            (8, &[Joint::Knee(side), Joint::Ankle(side)]),
            (9, &[Joint::Heel(side), Joint::Ball(side), Joint::Toe(side)]),
        ];
        for (segment, joints) in limb {
            let (segment, path) = (&SEGMENTS[segment], joints.iter().map(|j| frame.at(*j)).collect::<Vec<_>>());
            let length = path.windows(2).map(|w| distance(w[0], w[1])).sum();
            part(segment, Some(side), along(&path, segment.centre[s] / 100.0), length);
        }
    }
    out
}

/// Where a posed body's whole mass can be taken to sit.
pub fn centre_of_mass(sex: Sex, frame: &Frame) -> P3 {
    let (mut sum, mut share) = ([0.0; 3], 0.0);
    for part in parts(sex, frame) {
        for (s, c) in sum.iter_mut().zip(part.centre) {
            *s += c * part.share;
        }
        share += part.share;
    }
    sum.map(|s| s / share)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The paper's shares account for the whole body, to its rounding.
    #[test]
    fn the_parts_are_the_whole_body() {
        for s in 0..2 {
            let whole: f64 = SEGMENTS.iter().map(|g| g.mass[s] * if g.paired { 2.0 } else { 1.0 }).sum();
            assert!((whole - 100.0).abs() < 0.02, "{whole}");
            // The trunk's three parts are the paper's whole trunk: 42.57% and 43.46%, and
            // 614.8 and 603.3 mm from the cervicale to between the hips.
            let trunk: f64 = SEGMENTS[1..4].iter().map(|g| g.mass[s]).sum();
            let length: f64 = SEGMENTS[1..4].iter().map(|g| g.length[s]).sum();
            assert!((trunk - [42.57, 43.46][s]).abs() < 0.011 && (length - [614.8, 603.3][s]).abs() < 0.11, "{trunk} {length}");
        }
        assert_eq!(SEGMENTS.iter().filter(|g| g.paired).count(), 6);
    }

    /// The table above was typed from the image of the paper's page. The library research
    /// typed the same rows by another hand, and a second checker held them to the paper
    /// (docs/research/library/A4-segment-masses-com.md, Table A4-1). These are its numbers,
    /// taken from its text: mass, centre, and the three radii, women then men in each pair.
    #[test]
    fn the_table_is_the_researchs_too() {
        let research: [(&str, [f64; 10]); 10] = [
            ("head", [6.68, 6.94, 48.41, 50.02, 27.1, 30.3, 29.5, 31.5, 26.1, 26.1]),
            ("upper_trunk", [15.45, 15.96, 50.50, 50.66, 46.6, 50.5, 31.4, 32.0, 44.9, 46.5]),
            ("middle_trunk", [14.65, 16.33, 45.12, 45.02, 43.3, 48.2, 35.4, 38.3, 41.5, 46.8]),
            ("lower_trunk", [12.47, 11.17, 49.20, 61.15, 43.3, 61.5, 40.2, 55.1, 44.4, 58.7]),
            ("upper_arm", [2.55, 2.71, 57.54, 57.72, 27.8, 28.5, 26.0, 26.9, 14.8, 15.8]),
            ("forearm", [1.38, 1.62, 45.59, 45.74, 26.1, 27.6, 25.7, 26.5, 9.4, 12.1]),
            ("hand", [0.56, 0.61, 34.27, 36.24, 24.4, 28.8, 20.8, 23.5, 15.4, 18.4]),
            ("thigh", [14.78, 14.16, 36.12, 40.95, 36.9, 32.9, 36.4, 32.9, 16.2, 14.9]),
            ("shank", [4.81, 4.33, 44.16, 44.59, 27.1, 25.5, 26.7, 24.9, 9.3, 10.3]),
            ("foot", [1.29, 1.37, 40.14, 44.15, 29.9, 25.7, 27.9, 24.5, 13.9, 12.4]),
        ];
        for (segment, (name, row)) in SEGMENTS.iter().zip(research) {
            let typed = [segment.mass[0], segment.mass[1], segment.centre[0], segment.centre[1], segment.gyration[0][0], segment.gyration[0][1], segment.gyration[1][0], segment.gyration[1][1], segment.gyration[2][0], segment.gyration[2][1]];
            assert_eq!((segment.name, typed), (name, row));
        }
    }

    /// The paper gives five of the segments twice, between different endpoints: the same
    /// centre of mass and the same radii, said as shares of two lengths. The rows used here
    /// and the paper's other rows (typed below from the page's image) must say the same
    /// millimetres. This holds each row's length against its shares, which no sum does.
    #[test]
    fn the_papers_two_ways_of_saying_a_segment_agree() {
        // (segment, the other row: length, centre, radii, and how far its first endpoint is
        // from this table's, along the segment; women then men.)
        type Pair = [f64; 2];
        let other: [(usize, Pair, Pair, [Pair; 3], Pair); 5] = [
            // Head: vertex to mid-gonion.
            (0, [200.2, 203.3], [58.94, 59.76], [[33.0, 36.2], [35.9, 37.6], [31.8, 31.2]], [0.0, 0.0]),
            // Upper trunk: suprasternale to xiphoid; the suprasternale is 228.0 - 142.5 and 242.1 - 170.7 mm under the cervicale.
            (1, [142.5, 170.7], [20.77, 29.99], [[74.6, 71.6], [50.2, 45.4], [71.8, 65.9]], [85.5, 71.4]),
            // Forearm: elbow centre to stylion.
            (5, [262.4, 266.9], [45.92, 46.08], [[26.3, 27.8], [25.9, 26.7], [9.5, 12.2]], [0.0, 0.0]),
            // Hand: wrist centre to the third metacarpale.
            (6, [78.0, 86.2], [74.74, 79.00], [[53.1, 62.8], [45.4, 51.3], [33.5, 40.1]], [0.0, 0.0]),
            // Shank: knee centre to ankle centre.
            (8, [438.6, 440.3], [43.52, 43.95], [[26.7, 25.1], [26.3, 24.6], [9.2, 10.2]], [0.0, 0.0]),
        ];
        for (index, length, centre, gyration, offset) in other {
            let segment = &SEGMENTS[index];
            for s in 0..2 {
                let (here, there) = (segment.centre[s] / 100.0 * segment.length[s], offset[s] + centre[s] / 100.0 * length[s]);
                assert!((here - there).abs() < 0.1, "{}: the centre is {here} mm along by this row and {there} by the paper's other", segment.name);
                for (axis, (ours, theirs)) in segment.gyration.iter().zip(gyration).enumerate() {
                    // A radius is printed to a tenth of a percent: a quarter of a millimetre on each row.
                    let (here, there) = (ours[s] / 100.0 * segment.length[s], theirs[s] / 100.0 * length[s]);
                    assert!((here - there).abs() < 0.5, "{}, axis {axis}: {here} mm against {there}", segment.name);
                }
            }
        }
        // The trunk's three parts carry their weight where the paper's whole trunk does: 49.64%
        // of the way from the cervicale on its women. On its men the parts give 51.88% where
        // it prints 51.38%: the paper's own rows differ by 3 mm there, and are typed as printed.
        for (s, printed, within) in [(0, 49.64, 0.02), (1, 51.38, 0.55)] {
            let (mut moment, mut mass, mut from) = (0.0, 0.0, 0.0);
            for part in &SEGMENTS[1..4] {
                moment += part.mass[s] * (from + part.centre[s] / 100.0 * part.length[s]);
                mass += part.mass[s];
                from += part.length[s];
            }
            let whole = moment / mass / from * 100.0;
            assert!((whole - printed).abs() < within, "{whole} against the paper's {printed}");
        }
    }

    /// The parts of a body standing straight are where a hand puts them: each limb's centre
    /// its share of the way from its first joint, the trunk's three from the neck down, and
    /// the radii the shares of each part's length.
    #[test]
    fn a_standing_bodys_parts_are_where_the_table_puts_them() {
        use crate::body::Body;
        for sex in crate::ansur::Sex::BOTH {
            let (s, frame) = (sex.index(), Body::typical(sex).frame());
            let found = parts(sex, &frame);
            assert_eq!(found.len(), 16);
            let part = |name: &str| found.iter().find(|p| p.name == name && p.side != Some(Side::Right)).unwrap();
            let y = |joint: Joint| frame.at(joint)[1];
            let left = Side::Left;
            // Down a hanging limb: from the upper joint's height, the share of the length.
            for (name, upper, lower, row) in [("upper_arm", Joint::Shoulder(left), Joint::Elbow(left), 4), ("forearm", Joint::Elbow(left), Joint::Wrist(left), 5), ("hand", Joint::Wrist(left), Joint::Fingertip(left), 6), ("thigh", Joint::Hip(left), Joint::Knee(left), 7), ("shank", Joint::Knee(left), Joint::Ankle(left), 8)] {
                let (length, table) = (y(upper) - y(lower), &SEGMENTS[row]);
                let expected = y(upper) - table.centre[s] / 100.0 * length;
                assert!((part(name).centre[1] - expected).abs() < 1e-9 && (part(name).length - length).abs() < 1e-9, "{name}");
                assert!(part(name).centre[1] < y(upper) && part(name).centre[1] > y(lower));
                for (found, share) in part(name).gyration.iter().zip(table.gyration) {
                    assert!((found - share[s] / 100.0 * length).abs() < 1e-9, "{name}: a radius of {found} against {}% of {length}", share[s]);
                }
                assert!(part(name).gyration[2] < part(name).gyration[0], "{name}: a limb turns most easily about its own length");
            }
            // The head from the top down; the trunk's parts from the neck down, in their order.
            let head = y(Joint::Vertex) - SEGMENTS[0].centre[s] / 100.0 * (y(Joint::Vertex) - y(Joint::Neck));
            assert!((part("head").centre[1] - head).abs() < 1e-9);
            let (trunk, whole) = (y(Joint::Neck) - y(Joint::Pelvis), SEGMENTS[1].length[s] + SEGMENTS[2].length[s] + SEGMENTS[3].length[s]);
            let mut from = 0.0;
            for table in &SEGMENTS[1..4] {
                let expected = y(Joint::Neck) - (from + table.centre[s] / 100.0 * table.length[s]) / whole * trunk;
                assert!((part(table.name).centre[1] - expected).abs() < 1e-9, "{}: {} against {expected}", table.name, part(table.name).centre[1]);
                assert!((part(table.name).length - table.length[s] / whole * trunk).abs() < 1e-9);
                from += table.length[s];
            }
            assert!(part("upper_trunk").centre[1] > part("middle_trunk").centre[1] && part("middle_trunk").centre[1] > part("lower_trunk").centre[1]);
            // The foot along its sole from the heel.
            let (heel, toe) = (frame.at(Joint::Heel(left))[2], frame.at(Joint::Toe(left))[2]);
            assert!((part("foot").centre[2] - (heel + SEGMENTS[9].centre[s] / 100.0 * (toe - heel))).abs() < 1e-9 && part("foot").centre[1].abs() < 1e-12);
            // The two sides are mirrors, and the shares are the whole body.
            for p in found.iter().filter(|p| p.side == Some(Side::Right)) {
                let other = part(p.name);
                assert_eq!((p.centre[0], p.centre[1], p.centre[2], p.share), (-other.centre[0], other.centre[1], other.centre[2], other.share));
            }
            assert!((found.iter().map(|p| p.share).sum::<f64>() - 1.0).abs() < 2e-4);
        }
    }

    /// A fist and a foot on its toes carry their weight where they are.
    #[test]
    fn a_bent_hand_and_a_bent_foot_are_weighed_along_their_bends() {
        use crate::body::Body;
        let sex = crate::ansur::Sex::Female;
        let mut frame = Body::typical(sex).frame();
        let side = Side::Left;
        let (knuckle, tip) = (frame.at(Joint::Knuckle(side)), frame.at(Joint::Fingertip(side)));
        // The fingers point straight ahead from the knuckle.
        frame.set(Joint::Fingertip(side), [knuckle[0], knuckle[1], knuckle[2] + (knuckle[1] - tip[1])]);
        let found = parts(sex, &frame);
        let hand = found.iter().find(|p| p.name == "hand" && p.side == Some(side)).unwrap();
        // The hand's centre is 34.27% of the way along, and the knuckle is 45.9%: so it is on
        // the palm, straight under the wrist, and as far down as before.
        let wrist = frame.at(Joint::Wrist(side));
        let length = (wrist[1] - knuckle[1]) + (knuckle[1] - tip[1]);
        assert!(hand.centre[2].abs() < 1e-12 && (hand.centre[1] - (wrist[1] - 0.3427 * length)).abs() < 1e-9 && (hand.length - length).abs() < 1e-12, "{hand:?}");
    }

    #[test]
    fn a_point_along_a_bent_line_is_found_by_length() {
        let path = [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [3.0, 1.0, 0.0]];
        assert_eq!(along(&path, 0.0), [0.0, 0.0, 0.0]);
        assert_eq!(along(&path, 0.25), [0.0, 1.0, 0.0]);
        assert_eq!(along(&path, 0.625), [1.5, 1.0, 0.0]);
        assert_eq!(along(&path, 1.0), [3.0, 1.0, 0.0]);
        assert_eq!(along(&path, 7.0), [3.0, 1.0, 0.0]);
        // A line with no length is its point.
        assert_eq!(along(&[[1.0, 2.0, 3.0], [1.0, 2.0, 3.0]], 0.5), [1.0, 2.0, 3.0]);
    }
}
