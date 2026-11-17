//! A body's frame: where its joints are, in metres.
//!
//! The engine's conventions: right-handed, +Y up, the figure faces +Z, so its left is +X. The
//! ground is the plane the soles are on. A frame is a set of named points and nothing else:
//! [`crate::body::Body::frame`] makes the one a measured body stands in, and any figure's
//! joints, in any pose, can be put in one to be weighed and checked.

use crate::inertia::{distance, P3};

/// A side of the body. The figure faces +Z, so its left is +X.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub const BOTH: [Side; 2] = [Side::Left, Side::Right];

    /// +1 on the left (+X), -1 on the right.
    pub fn sign(self) -> f64 {
        match self {
            Side::Left => 1.0,
            Side::Right => -1.0,
        }
    }

    fn index(self) -> usize {
        match self {
            Side::Left => 0,
            Side::Right => 1,
        }
    }
}

/// A named point of the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Joint {
    /// Midway between the two hip joints' centres.
    Pelvis,
    /// On the spine's line at the navel's height.
    Waist,
    /// At the height of the lowest point of the tenth rib: the bottom of the rib cage.
    Ribs,
    /// At the height of the nipples.
    Chest,
    /// At the height of the cervicale, the bone felt at the base of the back of the neck.
    Neck,
    /// At the height of the tragion (the notch in front of the ear's opening): the nearest
    /// landmark the survey measured to where the skull sits on the spine. An assumption.
    Head,
    /// The top of the head.
    Vertex,
    /// The shoulder joint's centre.
    Shoulder(Side),
    Elbow(Side),
    Wrist(Side),
    /// The knuckle of the middle finger.
    Knuckle(Side),
    /// The tip of the middle finger.
    Fingertip(Side),
    /// The hip joint's centre.
    Hip(Side),
    Knee(Side),
    /// The ankle, at the height of the lateral malleolus (the bump on its outer side).
    Ankle(Side),
    /// The back of the heel, on the sole.
    Heel(Side),
    /// The ball of the foot (the head of the first metatarsal), on the sole.
    Ball(Side),
    /// The tip of the longest toe, on the sole.
    Toe(Side),
}

const NAMES: [&str; Joint::COUNT] = [
    "pelvis", "waist", "ribs", "chest", "neck", "head", "vertex", "shoulder_l", "shoulder_r", "elbow_l", "elbow_r", "wrist_l", "wrist_r", "knuckle_l", "knuckle_r", "fingertip_l",
    "fingertip_r", "hip_l", "hip_r", "knee_l", "knee_r", "ankle_l", "ankle_r", "heel_l", "heel_r", "ball_l", "ball_r", "toe_l", "toe_r",
];

impl Joint {
    /// How many joints a frame has.
    pub const COUNT: usize = 29;

    pub(crate) fn index(self) -> usize {
        let paired = |kind: usize, side: Side| 7 + kind * 2 + side.index();
        match self {
            Joint::Pelvis => 0,
            Joint::Waist => 1,
            Joint::Ribs => 2,
            Joint::Chest => 3,
            Joint::Neck => 4,
            Joint::Head => 5,
            Joint::Vertex => 6,
            Joint::Shoulder(s) => paired(0, s),
            Joint::Elbow(s) => paired(1, s),
            Joint::Wrist(s) => paired(2, s),
            Joint::Knuckle(s) => paired(3, s),
            Joint::Fingertip(s) => paired(4, s),
            Joint::Hip(s) => paired(5, s),
            Joint::Knee(s) => paired(6, s),
            Joint::Ankle(s) => paired(7, s),
            Joint::Heel(s) => paired(8, s),
            Joint::Ball(s) => paired(9, s),
            Joint::Toe(s) => paired(10, s),
        }
    }

    /// Every joint, in the frame's order.
    pub fn all() -> [Joint; Joint::COUNT] {
        let mut all = [Joint::Pelvis; Joint::COUNT];
        let single = [Joint::Pelvis, Joint::Waist, Joint::Ribs, Joint::Chest, Joint::Neck, Joint::Head, Joint::Vertex];
        all[..7].copy_from_slice(&single);
        let kinds: [fn(Side) -> Joint; 11] =
            [Joint::Shoulder, Joint::Elbow, Joint::Wrist, Joint::Knuckle, Joint::Fingertip, Joint::Hip, Joint::Knee, Joint::Ankle, Joint::Heel, Joint::Ball, Joint::Toe];
        for (k, kind) in kinds.iter().enumerate() {
            for side in Side::BOTH {
                all[7 + k * 2 + side.index()] = kind(side);
            }
        }
        all
    }

    /// The joint's name as data has it: `pelvis`, `knee_l`, `toe_r`.
    pub fn name(self) -> &'static str {
        NAMES[self.index()]
    }

    pub fn parse(name: &str) -> Option<Joint> {
        Joint::all().into_iter().find(|j| j.name() == name)
    }
}

/// Where a figure's joints are, and how broad its soles are.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    points: [P3; Joint::COUNT],
    /// The heel's breadth, metres.
    pub heel_breadth: f64,
    /// The foot's breadth at the ball, metres.
    pub ball_breadth: f64,
}

impl Frame {
    /// A frame with every joint at the origin, to be filled in.
    pub fn empty() -> Frame {
        Frame { points: [[0.0; 3]; Joint::COUNT], heel_breadth: 0.0, ball_breadth: 0.0 }
    }

    pub fn at(&self, joint: Joint) -> P3 {
        self.points[joint.index()]
    }

    pub fn set(&mut self, joint: Joint, point: P3) {
        self.points[joint.index()] = point;
    }

    /// The spine's line from the pelvis up to the neck.
    pub fn spine(&self) -> [P3; 5] {
        [Joint::Pelvis, Joint::Waist, Joint::Ribs, Joint::Chest, Joint::Neck].map(|j| self.at(j))
    }

    /// The trunk's length along the spine, pelvis to neck.
    pub fn trunk(&self) -> f64 {
        self.spine().windows(2).map(|w| distance(w[0], w[1])).sum()
    }

    /// A hand's length, wrist to fingertip by way of the knuckle: bent fingers are as long.
    pub fn hand(&self, side: Side) -> f64 {
        distance(self.at(Joint::Wrist(side)), self.at(Joint::Knuckle(side))) + distance(self.at(Joint::Knuckle(side)), self.at(Joint::Fingertip(side)))
    }

    /// A foot's length, heel to toe by way of the ball: a foot risen on its toes is as long.
    pub fn foot(&self, side: Side) -> f64 {
        distance(self.at(Joint::Heel(side)), self.at(Joint::Ball(side))) + distance(self.at(Joint::Ball(side)), self.at(Joint::Toe(side)))
    }

    /// How far a side's ankle is above its sole: the line from the heel to the ball, which
    /// turns with the ankle when the foot rises on its toes.
    pub fn ankle_height(&self, side: Side) -> f64 {
        let (ankle, heel, toe) = (self.at(Joint::Ankle(side)), self.at(Joint::Heel(side)), self.at(Joint::Ball(side)));
        let (foot, to_ankle) = ([toe[0] - heel[0], toe[1] - heel[1], toe[2] - heel[2]], [ankle[0] - heel[0], ankle[1] - heel[1], ankle[2] - heel[2]]);
        let length2 = foot[0] * foot[0] + foot[1] * foot[1] + foot[2] * foot[2];
        if length2 <= 0.0 {
            return distance(ankle, heel);
        }
        let t = (to_ankle[0] * foot[0] + to_ankle[1] * foot[1] + to_ankle[2] * foot[2]) / length2;
        distance(ankle, [heel[0] + foot[0] * t, heel[1] + foot[1] * t, heel[2] + foot[2] * t])
    }

    /// The figure's stature as its parts give it, whatever its pose: the ankle above the sole,
    /// the shank, the thigh (each the mean of the two sides), the trunk along the spine, and
    /// the neck to the top of the head. For a body standing straight it is the height of the
    /// top of the head above the ground.
    pub fn stature(&self) -> f64 {
        let leg = |side: Side| self.ankle_height(side) + distance(self.at(Joint::Ankle(side)), self.at(Joint::Knee(side))) + distance(self.at(Joint::Knee(side)), self.at(Joint::Hip(side)));
        (leg(Side::Left) + leg(Side::Right)) / 2.0 + self.trunk() + distance(self.at(Joint::Neck), self.at(Joint::Vertex))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_joint_has_its_own_place_and_name() {
        let all = Joint::all();
        for (i, joint) in all.iter().enumerate() {
            assert_eq!(joint.index(), i, "{joint:?}");
            assert_eq!(Joint::parse(joint.name()), Some(*joint));
        }
        assert_eq!((Joint::Knee(Side::Left).name(), Joint::Toe(Side::Right).name(), Joint::Pelvis.name()), ("knee_l", "toe_r", "pelvis"));
        assert_eq!(Joint::parse("tail"), None);
        assert_eq!((Side::Left.sign(), Side::Right.sign()), (1.0, -1.0));
    }

    #[test]
    fn a_frames_stature_is_the_sum_of_its_parts_in_any_pose() {
        let mut frame = Frame::empty();
        for side in Side::BOTH {
            let x = 0.1 * side.sign();
            frame.set(Joint::Heel(side), [x, 0.0, -0.05]);
            frame.set(Joint::Ball(side), [x, 0.0, 0.13]);
            frame.set(Joint::Toe(side), [x, 0.0, 0.2]);
            frame.set(Joint::Wrist(side), [x * 2.0, 0.8, 0.0]);
            frame.set(Joint::Knuckle(side), [x * 2.0, 0.72, 0.0]);
            frame.set(Joint::Fingertip(side), [x * 2.0, 0.62, 0.0]);
            frame.set(Joint::Ankle(side), [x, 0.07, 0.0]);
            frame.set(Joint::Knee(side), [x, 0.5, 0.0]);
            frame.set(Joint::Hip(side), [x, 0.9, 0.0]);
        }
        for (joint, y) in [(Joint::Pelvis, 0.9), (Joint::Waist, 1.0), (Joint::Ribs, 1.1), (Joint::Chest, 1.25), (Joint::Neck, 1.45), (Joint::Vertex, 1.7)] {
            frame.set(joint, [0.0, y, 0.0]);
        }
        assert!((frame.stature() - 1.7).abs() < 1e-12, "{}", frame.stature());
        assert!((frame.trunk() - 0.55).abs() < 1e-12 && (frame.ankle_height(Side::Left) - 0.07).abs() < 1e-12);
        // Lying on its back (every point's height and depth exchanged): the parts are as long.
        let mut lying = frame.clone();
        for joint in Joint::all() {
            let [x, y, z] = frame.at(joint);
            lying.set(joint, [x, -z, y]);
        }
        assert!((lying.stature() - 1.7).abs() < 1e-12, "{}", lying.stature());
        // A fist, and a foot risen on its toes with the heel and the ankle turned about the
        // ball: the hand, the foot and the ankle's height are what they were.
        let (side, mut bent) = (Side::Left, frame.clone());
        assert!((frame.hand(side) - 0.18).abs() < 1e-12 && (frame.foot(side) - 0.25).abs() < 1e-12);
        bent.set(Joint::Fingertip(side), [0.2, 0.72, 0.1]);
        let (ball, turn) = (frame.at(Joint::Ball(side)), |p: [f64; 3], ball: [f64; 3]| [p[0], ball[1] + (p[1] - ball[1]) * 0.8 - (p[2] - ball[2]) * 0.6, ball[2] + (p[1] - ball[1]) * 0.6 + (p[2] - ball[2]) * 0.8]);
        for joint in [Joint::Heel(side), Joint::Ankle(side)] {
            bent.set(joint, turn(frame.at(joint), ball));
        }
        assert!(bent.at(Joint::Heel(side))[1] > 0.1, "the heel is off the ground: {:?}", bent.at(Joint::Heel(side)));
        assert!((bent.hand(side) - 0.18).abs() < 1e-12 && (bent.foot(side) - 0.25).abs() < 1e-12 && (bent.ankle_height(side) - 0.07).abs() < 1e-12, "{} {} {}", bent.hand(side), bent.foot(side), bent.ankle_height(side));
    }
}
