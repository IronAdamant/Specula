//! A human figure's surface from a measured body: the shape `figure`.
//!
//! [`specula_corpus::Body`] says how long a body's bones are, where its joints stand and how
//! far round each part of it measures. This module puts a surface on that: lines of points
//! with radii ([`crate::sdf`]'s round cones, joined smoothly), meshed like any sculpt. One
//! part of a recipe is one part of a person, each meshed on a grid as fine as it needs (a
//! finger is a third of a trunk's cell):
//!
//! ```text
//! "let": { },
//! "parts": [
//!   { "name": "body",   "material": "skin", "shape": { "op": "figure", "part": "body",   "sex": "female", "stature": 1.70 } },
//!   { "name": "head",   "material": "skin", "shape": { "op": "figure", "part": "head",   "sex": "female", "stature": 1.70 } },
//!   { "name": "hand_l", "material": "skin", "shape": { "op": "figure", "part": "hand_l", "sex": "female", "stature": 1.70 } },
//!   ... hand_r, foot_l, foot_r ]
//! ```
//!
//! The parts overlap where they meet (a hand begins inside the forearm's end), each closed on
//! its own. `"part": "whole"` is everything on one grid: coarse, for a figure seen from afar.
//!
//! **Where each size comes from.** A girth the survey measured is a radius (the girth over
//! two pi) at the height it was measured at. The trunk's sections are as broad and as deep as
//! the survey's breadths and depths at the hips, the waist and the chest. A finger's bones
//! are in the proportions Buryanov and Kotiuk measured on radiographs of 66 adults (2010, Int
//! J Morphol 28(3):755-758; cited as facts, their article is not free for commercial reuse),
//! scaled to the body's own hand. Between the measured places the surface is this module's
//! own: where a limb is thickest along its length, how a knee narrows, how thick a palm is
//! for its girth, how long the small toes are. Those are judged by looking, in the engine,
//! and each look is recorded with what was changed for it (docs/design/FIGURE.md).
//!
//! The figure stands in the pose it is rigged in: arms held out from the sides and legs a
//! little apart ([`specula_corpus::Frame::spread`]), the thumbs forward.

use specula_corpus::{Body, Frame, Joint, Side};

use crate::math3::{self, V3};
use crate::sdf::{Combine, Kind, Modifiers, Prim, SdfNode};

/// The choices no measurement makes about the whole figure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shaping {
    /// Degrees each arm is held out from hanging, and each leg from standing straight.
    pub arms: f64,
    pub legs: f64,
    /// How far the parts are melted into each other where they meet, metres.
    pub smooth: f32,
}

impl Default for Shaping {
    fn default() -> Self {
        Shaping { arms: 35.0, legs: 7.0, smooth: 0.025 }
    }
}

/// Which part of the figure a tree is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// Everything, for one coarse grid.
    Whole,
    /// The trunk, the neck, the arms to the wrists and the legs to the ankles.
    Body,
    Head,
    Hand(Side),
    Foot(Side),
}

impl Part {
    pub const NAMES: [&'static str; 7] = ["whole", "body", "head", "hand_l", "hand_r", "foot_l", "foot_r"];

    pub fn parse(name: &str) -> Option<Part> {
        Some(match name {
            "whole" => Part::Whole,
            "body" => Part::Body,
            "head" => Part::Head,
            "hand_l" => Part::Hand(Side::Left),
            "hand_r" => Part::Hand(Side::Right),
            "foot_l" => Part::Foot(Side::Left),
            "foot_r" => Part::Foot(Side::Right),
            _ => return None,
        })
    }
}

/// The lengths of a finger's bones on Buryanov and Kotiuk's adults, millimetres: the soft tip
/// past the last bone, then the distal, middle and proximal phalanges, then the metacarpal.
/// Thumb (no middle phalanx), index, middle, ring, little.
const FINGERS: [[f32; 5]; 5] = [[5.67, 21.67, 0.0, 31.57, 46.22], [3.84, 15.82, 22.38, 39.78, 68.12], [3.95, 17.40, 26.33, 44.63, 64.60], [3.95, 17.30, 25.65, 41.37, 58.00], [3.73, 15.96, 18.11, 32.74, 53.69]];

/// +1 on the left (+X), -1 on the right.
fn sign(side: Side) -> f32 {
    side.sign() as f32
}

fn point(p: [f64; 3]) -> V3 {
    [p[0] as f32, p[1] as f32, p[2] as f32]
}

fn lerp(a: V3, b: V3, t: f32) -> V3 {
    math3::lerp(a, b, t)
}

fn node(kind: Kind) -> SdfNode {
    SdfNode { kind, m: Modifiers::default() }
}

fn cone(a: V3, b: V3, ra: f32, rb: f32) -> SdfNode {
    node(Kind::Prim(Prim::RoundCone { a, b, ra, rb }))
}

fn union(kids: Vec<SdfNode>, smooth: f32) -> SdfNode {
    node(Kind::Group { op: Combine::Near, kids, smooth })
}

/// A chain: a round cone between every two neighbours of `points` (each a place and a radius).
fn chain(points: &[(V3, f32)], smooth: f32) -> SdfNode {
    union(points.windows(2).map(|w| cone(w[0].0, w[1].0, w[0].1, w[1].1)).collect(), smooth)
}

/// The half thickness of a section that is `breadth` across and `girth` round, taken as two
/// flat sides with a half circle at each end (which is what a row of round bones side by side
/// makes): the girth is twice the flat part and one circle of the thickness.
fn half_thickness(breadth: f32, girth: f32) -> f32 {
    ((girth - 2.0 * breadth) / (std::f32::consts::PI - 2.0) / 2.0).clamp(breadth * 0.08, breadth / 2.0)
}

/// A body's sizes and its posed frame, read once.
struct Sizes<'a> {
    body: &'a Body,
    frame: Frame,
    /// How far the posed figure stands lower than the body's straight one, metres.
    drop: f32,
    smooth: f32,
}

impl Sizes<'_> {
    fn at(&self, joint: Joint) -> V3 {
        point(self.frame.at(joint))
    }

    /// A dimension in metres.
    fn m(&self, name: &str) -> f32 {
        (self.body.dimension(name).unwrap_or(0.0) / 1000.0) as f32
    }

    /// A girth as the radius of a circle that far round.
    fn round(&self, name: &str) -> f32 {
        self.m(name) / (2.0 * std::f32::consts::PI)
    }

    /// A height the survey measured, on the posed figure.
    fn height(&self, name: &str) -> f32 {
        self.m(name) - self.drop
    }

    /// One slice of the trunk between two stations, each a height, a half breadth, a half
    /// depth and how far forward its middle is: a round cone as broad as measured, stretched
    /// to the depth measured.
    fn slab(&self, lower: [f32; 4], upper: [f32; 4]) -> SdfNode {
        let depth = (lower[2] / lower[1] + upper[2] / upper[1]) / 2.0;
        let forward = (lower[3] + upper[3]) / 2.0;
        let mut slab = cone([0.0, lower[0], 0.0], [0.0, upper[0], 0.0], lower[1], upper[1]);
        slab.m.at = [0.0, 0.0, forward];
        slab.m.stretch = [1.0, 1.0, depth];
        slab
    }

    /// The trunk from the crotch to the base of the neck, the neck, and the bulk of each
    /// shoulder out to its joint.
    fn trunk(&self) -> Vec<SdfNode> {
        let k = self.smooth;
        let neck = self.at(Joint::Neck);
        let shoulder = self.at(Joint::Shoulder(Side::Left));
        let neck_base = self.round("neck_circumference_base");
        // Each station: height, half breadth, half depth, forward of the frame's plane.
        let hips = [self.height("buttock_height"), self.m("hip_breadth") / 2.0, self.m("buttock_depth") / 2.0, -0.012];
        let crotch = [self.height("crotch_height") + hips[1] * 0.45, hips[1] * 0.86, hips[2] * 0.86, -0.008];
        let waist = [self.at(Joint::Waist)[1], self.m("waist_breadth") / 2.0, self.m("waist_depth") / 2.0, 0.004];
        let chest = [self.at(Joint::Chest)[1], self.m("chest_breadth") / 2.0, self.m("chest_depth") / 2.0, 0.012];
        let top = [shoulder[1] - 0.02, shoulder[0] * 0.80, chest[2] * 0.62, -0.004];
        let collar = [neck[1] - neck_base * 0.2, neck_base * 1.25, neck_base, 0.0];
        let mut parts = vec![union(vec![self.slab(crotch, hips), self.slab(hips, waist), self.slab(waist, chest), self.slab(chest, top), self.slab(top, collar)], k)];
        let upper_r = self.round("biceps_circumference_flexed");
        for side in Side::BOTH {
            let joint = self.at(Joint::Shoulder(side));
            parts.push(chain(&[([joint[0] * 0.55, top[0], 0.0], top[2] * 0.95), (joint, upper_r * 1.12)], k));
        }
        parts.push(chain(&[(neck, neck_base), (self.at(Joint::Head), self.round("neck_circumference"))], k));
        parts
    }

    /// An arm from the shoulder to the wrist, and `past` of the way on into the hand.
    fn arm(&self, side: Side, past: f32) -> SdfNode {
        let (shoulder, elbow, wrist, knuckle) = (self.at(Joint::Shoulder(side)), self.at(Joint::Elbow(side)), self.at(Joint::Wrist(side)), self.at(Joint::Knuckle(side)));
        let (upper_r, fore_r, wrist_r) = (self.round("biceps_circumference_flexed"), self.round("forearm_circumference_flexed"), self.round("wrist_circumference"));
        chain(
            &[
                (shoulder, upper_r * 1.05),
                (lerp(shoulder, elbow, 0.42), upper_r * 0.98),
                (lerp(shoulder, elbow, 0.88), fore_r * 0.92),
                (elbow, fore_r * 0.96),
                (lerp(elbow, wrist, 0.22), fore_r),
                (lerp(elbow, wrist, 0.80), wrist_r * 1.12),
                (wrist, wrist_r),
                (lerp(wrist, knuckle, past), wrist_r),
            ],
            self.smooth * 0.4,
        )
    }

    /// A leg from the hip to the ankle.
    fn leg(&self, side: Side) -> SdfNode {
        let (hip, knee, ankle) = (self.at(Joint::Hip(side)), self.at(Joint::Knee(side)), self.at(Joint::Ankle(side)));
        let (thigh_r, lower_r, calf_r, ankle_r) = (self.round("thigh_circumference"), self.round("lower_thigh_circumference"), self.round("calf_circumference"), self.round("ankle_circumference"));
        // The thigh's girth is measured at its top, where the trunk's hips already are: the
        // chain begins a little down it.
        let back = |p: V3, by: f32| [p[0], p[1], p[2] - by];
        chain(
            &[
                (lerp(hip, knee, 0.10), thigh_r * 0.97),
                (lerp(hip, knee, 0.45), (thigh_r + lower_r) / 2.0 * 1.02),
                (lerp(hip, knee, 0.84), lower_r),
                (knee, lower_r * 0.93),
                (back(lerp(knee, ankle, 0.14), 0.004), calf_r * 0.94),
                (back(lerp(knee, ankle, 0.32), 0.012), calf_r),
                (back(lerp(knee, ankle, 0.62), 0.006), (calf_r + ankle_r) / 2.0 * 0.92),
                (lerp(knee, ankle, 0.90), ankle_r),
                (ankle, ankle_r * 1.05),
            ],
            self.smooth * 0.4,
        )
    }

    /// A hand: four metacarpals side by side for the palm, three bones to each finger, a
    /// thumb from the heel of the hand. The thumb is forward (+Z); the palm faces the thigh.
    fn hand(&self, side: Side) -> SdfNode {
        let (wrist, knuckle, tip) = (self.at(Joint::Wrist(side)), self.at(Joint::Knuckle(side)), self.at(Joint::Fingertip(side)));
        let along = math3::normalize(math3::sub(tip, wrist));
        // Across the hand, toward the thumb: forward, squared to the hand's length.
        let forward = [0.0, 0.0, 1.0];
        let across = math3::normalize(math3::sub(forward, math3::scale(along, math3::dot(forward, along))));
        let breadth = self.m("hand_breadth");
        let half = half_thickness(breadth, self.m("hand_circumference"));
        // The paper's bones at this hand's size: its middle finger is what the frame's is.
        let size = math3::length(math3::sub(tip, knuckle)) / (FINGERS[2][0] + FINGERS[2][1] + FINGERS[2][2] + FINGERS[2][3]);
        let place = |from: V3, far: f32, side_by: f32| math3::add(math3::add(from, math3::scale(along, far)), math3::scale(across, side_by));
        let wrist_r = self.round("wrist_circumference");
        let mut parts = vec![cone(lerp(wrist, knuckle, -0.25), lerp(wrist, knuckle, 0.12), wrist_r, wrist_r * 0.98)];
        let finger_r = (breadth / 4.0) * 0.42;
        // Index (ray 1) to little (ray 4): the index on the thumb's side.
        for (ray, bones) in FINGERS.iter().copied().enumerate().skip(1) {
            let offset = (2.5 - ray as f32) * (breadth - 2.0 * finger_r) / 3.0;
            // Where this finger's knuckle is: across the palm, and as much nearer or further
            // than the middle one as its metacarpal is shorter or longer.
            let reach = math3::dot(math3::sub(knuckle, wrist), along) + (bones[4] - FINGERS[2][4]) * size;
            let root = place(wrist, reach, offset);
            let heel = place(wrist, reach * 0.12, offset * 0.62);
            parts.push(cone(heel, root, half * 0.92, half * 0.86));
            // The finger: a little toward the palm with each bone, as a hand at rest. (The palm
            // faces the thigh: along x across points there on the left hand, and away on the right.)
            let curl = [0.10, 0.24, 0.40f32];
            let palmward = math3::scale(math3::cross(along, across), sign(side));
            let mut joint = root;
            let mut points = vec![(root, finger_r * 1.04)];
            for (i, length) in [bones[3], bones[2], bones[1] + bones[0]].into_iter().enumerate() {
                let lean = curl[i];
                // The fingers fan a little apart, the index toward the thumb.
                let fan = (2.5 - ray as f32) * 0.07;
                let step = math3::normalize(math3::add(math3::add(math3::scale(along, (1.0 - lean * lean).max(0.0).sqrt()), math3::scale(palmward, lean)), math3::scale(across, fan)));
                // The last bone ends a tip's radius short, so that the finger is as long as measured.
                let radius = finger_r * [0.96, 0.88, 0.78][i];
                let reach = length * size - if i == 2 { radius } else { 0.0 };
                joint = math3::add(joint, math3::scale(step, reach));
                points.push((joint, radius));
            }
            parts.push(chain(&points, 0.002));
        }
        // The thumb: its metacarpal from the heel of the hand, out and forward.
        let bones = FINGERS[0];
        let base = place(wrist, 0.012, breadth * 0.30);
        let out = math3::normalize(math3::add(math3::scale(along, 0.72), math3::scale(across, 0.69)));
        let first = math3::add(base, math3::scale(out, bones[4] * size));
        let further = math3::normalize(math3::add(math3::scale(along, 0.86), math3::scale(across, 0.50)));
        let second = math3::add(first, math3::scale(further, bones[3] * size));
        let thumb_r = finger_r * 1.22;
        let end = math3::add(second, math3::scale(further, (bones[1] + bones[0]) * size - thumb_r * 0.86));
        parts.push(chain(&[(base, half * 0.95), (first, thumb_r * 1.05), (second, thumb_r * 0.95), (end, thumb_r * 0.86)], 0.004));
        union(parts, 0.006)
    }

    /// A foot: a heel, an instep from the ankle, the body of the foot lower toward the ball,
    /// and five toes.
    fn foot(&self, side: Side) -> SdfNode {
        let (ankle, heel, ball, toe) = (self.at(Joint::Ankle(side)), self.at(Joint::Heel(side)), self.at(Joint::Ball(side)), self.at(Joint::Toe(side)));
        let ankle_r = self.round("ankle_circumference");
        let (heel_r, ball_half) = (self.m("heel_breadth") / 2.0, self.m("foot_breadth_horizontal") / 2.0);
        let x = ankle[0];
        // The body of the foot, as broad as measured at the heel and at the ball, and flatter
        // than it is broad: stretched down about the ground it stands on.
        let flat = 0.62;
        let mut sole = cone([x, heel_r, heel[2] + heel_r], [x, ball_half * 0.92, ball[2] - ball_half * 0.25], heel_r, ball_half * 0.92);
        sole.m.stretch = [1.0, flat, 1.0];
        let mut parts = vec![sole];
        // The heel under the ankle, and the instep from the ankle forward and down.
        parts.push(chain(&[(lerp(self.at(Joint::Knee(side)), ankle, 0.86), ankle_r), (ankle, ankle_r * 1.05), ([x, heel_r * 0.95, heel[2] + heel_r * 1.02], heel_r * 0.95)], 0.01));
        parts.push(chain(&[(ankle, ankle_r * 0.98), ([x, ball_half * 0.92 * flat * 1.25, lerp(ankle, ball, 0.55)[2]], ball_half * 0.60)], 0.012));
        // Five toes from the ball's line, the great toe on the inside and as long as the foot
        // is measured; the others each shorter (no survey here measures them).
        let inward = -sign(side);
        let shares = [1.0, 0.94, 0.86, 0.76, 0.64f32];
        let radii = [0.0125, 0.0092, 0.0086, 0.0080, 0.0074f32];
        let toes = toe[2] - ball[2];
        let room = 2.0 * ball_half - 2.0 * radii[0];
        for i in 0..5 {
            let across = x + inward * (room / 2.0) - inward * room * (i as f32 / 4.0) * 0.96;
            // The ball's line slants back toward the little toe.
            let from = ball[2] - toes * 0.20 * i as f32 / 4.0 - 0.004;
            let r = radii[i] * (ball_half / 0.047);
            let reach = from + (toes + 0.004) * shares[i] - r;
            parts.push(chain(&[([across, r * 1.25, from], r * 1.2), ([across, r * 1.02, (from + reach) / 2.0], r), ([across, r, reach], r * 0.95)], 0.002));
        }
        union(parts, 0.008)
    }

    /// A head: the skull as broad and as long as measured, a face and a jaw under it, and
    /// the neck it sits on. (A face's features are their own work.)
    fn head(&self) -> SdfNode {
        let (neck, head, vertex) = (self.at(Joint::Neck), self.at(Joint::Head), self.at(Joint::Vertex));
        let (breadth, length) = (self.m("head_breadth"), self.m("head_length"));
        // The skull's height: from the top of the head to the ear's opening, and as much again
        // less a fifth below it.
        let top = vertex[1] - head[1];
        let mut skull = node(Kind::Prim(Prim::Sphere { radius: breadth / 2.0 }));
        skull.m.at = [0.0, vertex[1] - top * 0.92, 0.012];
        skull.m.stretch = [1.0, top * 0.92 / (breadth / 2.0), length / breadth];
        // The face: from the brow down to the chin, narrower than the skull and forward of it.
        let face = self.m("menton_sellion_length");
        let cheeks = self.m("bizygomatic_breadth") / 2.0;
        let chin_y = head[1] + top * 0.10 - face * 0.92;
        let mut jaw = chain(&[([0.0, head[1] + top * 0.05, length * 0.20], cheeks * 0.86), ([0.0, chin_y + cheeks * 0.42, length * 0.27], cheeks * 0.50)], 0.0);
        jaw.m.stretch = [1.0, 1.0, 0.86];
        let neck_r = self.round("neck_circumference");
        let stem = chain(&[(lerp(neck, head, -0.25), self.round("neck_circumference_base")), (lerp(neck, head, 0.5), neck_r), ([0.0, head[1] + top * 0.1, 0.0], neck_r * 0.98)], 0.01);
        union(vec![skull, jaw, stem], 0.02)
    }
}

/// The distance tree of one part of a body's surface.
pub fn tree(body: &Body, shaping: &Shaping, part: Part) -> SdfNode {
    let straight = body.frame();
    let frame = straight.spread(shaping.arms, shaping.legs);
    let drop = (straight.at(Joint::Pelvis)[1] - frame.at(Joint::Pelvis)[1]) as f32;
    let sizes = Sizes { body, frame, drop, smooth: shaping.smooth };
    let limbs = |past: f32| {
        let mut parts = sizes.trunk();
        for side in Side::BOTH {
            parts.push(sizes.arm(side, past));
            parts.push(sizes.leg(side));
        }
        parts
    };
    match part {
        Part::Body => union(limbs(0.10), shaping.smooth),
        Part::Head => sizes.head(),
        Part::Hand(side) => sizes.hand(side),
        Part::Foot(side) => sizes.foot(side),
        // One coarse grid cannot hold a finger or a toe: a mitten and a shoe.
        Part::Whole => {
            let mut parts = limbs(0.0);
            parts.push(sizes.head());
            for side in Side::BOTH {
                let (wrist, knuckle, tip) = (sizes.at(Joint::Wrist(side)), sizes.at(Joint::Knuckle(side)), sizes.at(Joint::Fingertip(side)));
                let palm = sizes.m("hand_breadth") * 0.34;
                parts.push(chain(&[(wrist, sizes.round("wrist_circumference")), (knuckle, palm), (lerp(knuckle, tip, 0.8), palm * 0.7)], 0.01));
                let (ankle, heel, ball, toe) = (sizes.at(Joint::Ankle(side)), sizes.at(Joint::Heel(side)), sizes.at(Joint::Ball(side)), sizes.at(Joint::Toe(side)));
                let (heel_r, ball_r) = (sizes.m("heel_breadth") / 2.0, sizes.m("foot_breadth_horizontal") / 2.0 * 0.62);
                parts.push(chain(&[(ankle, sizes.round("ankle_circumference")), ([heel[0], heel_r, heel[2] + heel_r], heel_r), ([ball[0], ball_r, ball[2]], ball_r), ([toe[0], ball_r * 0.7, toe[2] - ball_r * 0.7], ball_r * 0.7)], 0.01));
            }
            union(parts, shaping.smooth)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdf::{mesh, MeshOptions, MAX_WORK};
    use specula_corpus::Sex;

    /// Directed edges not matched by as many reverse edges: 0 for a closed surface. (Where
    /// two sheets of it come within a cell of each other, between two fingers or at the top
    /// of the thighs, the mesher joins them along an edge four faces share: closed still, and
    /// counted apart, below, as what a finer grid or a wider pose would part.)
    fn unbalanced(shape: &crate::shape::Shape) -> usize {
        let mut welded = shape.clone();
        welded.weld_exact();
        let mut edges = std::collections::BTreeMap::new();
        for t in &welded.tris {
            for k in 0..3 {
                *edges.entry((t[k], t[(k + 1) % 3])).or_insert(0) += 1;
            }
        }
        edges.iter().filter(|(&(a, b), &n)| edges.get(&(b, a)).copied().unwrap_or(0) != n).count()
    }

    fn built(body: &Body, part: Part, resolution: u32) -> crate::shape::Shape {
        let opts = MeshOptions { resolution, bounds: None, seed: 1, max_triangles: 200_000, max_work: MAX_WORK, color: [1.0; 4] };
        mesh(&tree(body, &Shaping::default(), part), &opts).unwrap_or_else(|e| panic!("{part:?}: {}", e.message)).0
    }

    /// Each part is a closed surface where the body's frame says it is; the body is as tall
    /// as its shoulders and stands to its ankles; together they hold about the volume the
    /// body's weight says (a body is about as dense as water).
    #[test]
    fn a_figures_parts_are_closed_and_where_the_body_is() {
        for sex in Sex::BOTH {
            let body = Body::typical(sex);
            let stature = body.stature() as f32;
            let mut litres = 0.0;
            for (part, resolution) in [(Part::Body, 80), (Part::Head, 40), (Part::Hand(Side::Left), 80), (Part::Hand(Side::Right), 80), (Part::Foot(Side::Left), 48), (Part::Foot(Side::Right), 48)] {
                let shape = built(&body, part, resolution);
                assert_eq!(unbalanced(&shape), 0, "{} {part:?} at {resolution} is not closed", sex.name());
                // The head and the feet are clean surfaces outright; the body and the hands
                // may join two sheets along a few edges (thighs, fingers).
                let joined = shape.open_edge_count();
                assert!(joined <= if matches!(part, Part::Head | Part::Foot(_)) { 0 } else { 12 }, "{} {part:?} at {resolution}: {joined} edges where two sheets join", sex.name());
                let bounds = shape.bounds();
                match part {
                    Part::Head => assert!((bounds.max[1] - stature).abs() < 0.02 && bounds.max[1] - bounds.min[1] < 0.42, "{bounds:?}"),
                    Part::Foot(side) => {
                        assert!(bounds.min[1] > -0.012 && bounds.min[1] < 0.006 && bounds.max[1] < 0.2, "{} {part:?} {bounds:?}", sex.name());
                        let length = bounds.max[2] - bounds.min[2];
                        let measured = (body.dimension("foot_length").unwrap() / 1000.0) as f32;
                        assert!((length - measured).abs() < 0.015, "{} foot: {length} long, measured {measured}", sex.name());
                        assert!((bounds.max[0] + bounds.min[0]) * sign(side) > 0.0);
                    }
                    Part::Hand(side) => assert!((bounds.max[0] + bounds.min[0]) * sign(side) > 0.6 && bounds.max[1] < 1.1, "{bounds:?}"),
                    _ => assert!(bounds.min[1] > 0.02 && bounds.max[1] > stature * 0.8 && bounds.max[1] < stature * 0.97, "{bounds:?}"),
                }
                litres += shape.signed_volume() * 1000.0;
            }
            let weight = body.weight() as f32;
            assert!(litres > weight * 0.8 && litres < weight * 1.25, "{}: {litres} litres for {weight} kg", sex.name());
        }
    }

    /// A heavier body of the same stature is a bigger figure; the same body is the same figure.
    #[test]
    fn the_figure_follows_the_body() {
        let slight = built(&Body::with_build(Sex::Female, 1.65, -1.0).unwrap(), Part::Body, 64).signed_volume();
        let heavy = built(&Body::with_build(Sex::Female, 1.65, 2.0).unwrap(), Part::Body, 64).signed_volume();
        assert!(heavy > slight * 1.25, "{heavy} against {slight}");
        let (a, b) = (built(&Body::typical(Sex::Male), Part::Whole, 40), built(&Body::typical(Sex::Male), Part::Whole, 40));
        assert_eq!(a.positions, b.positions);
        assert_eq!(Part::NAMES.map(|n| Part::parse(n).is_some()), [true; 7]);
        assert_eq!(Part::parse("tail"), None);
    }

    /// A palm's thickness is found from its breadth and its girth: a section 80 mm across and
    /// 190 mm round is about 26 mm thick.
    #[test]
    fn a_sections_thickness_follows_its_breadth_and_girth() {
        let half = half_thickness(0.08, 0.19);
        assert!(half > 0.011 && half < 0.016, "{half}");
        // A circle's is its radius.
        let r = half_thickness(0.1, std::f32::consts::PI * 0.1);
        assert!((r - 0.05).abs() < 1e-4, "{r}");
    }
}
