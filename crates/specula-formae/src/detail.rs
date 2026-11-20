//! Detail: how finely curved parts are cut, and the error that leaves.
//!
//! `detail` is one integer for every form: the number of segments in a full turn. **The rule**
//! for a curved piece worth `t` turns is `max(2, ceil(detail · t))` segments, so no sample
//! segment of an arc spans more than `2π / detail`, and no curved piece is ever its bare chord
//! (a sliver of a circle keeps a point on its arc). What a turn is, for each kind of piece:
//! an arc's sweep over 2π; for a parametric piece, a stated share (a full ellipse is one turn,
//! a spiral one turn per turn of its angle, a sine wave one turn per cycle, a parabola or a
//! catenary its tangent's turning over 2π). A form without curved parts ignores it.

use crate::maths::{asin, ceil, versin, PI};

/// The least detail.
pub const DETAIL_MIN: u32 = 8;
/// The detail used when none is given.
pub const DETAIL_DEFAULT: u32 = 64;
/// The greatest detail.
pub const DETAIL_MAX: u32 = 4096;

/// The number of sample segments of a piece worth `turns` full turns, at `detail`: the rule
/// above. (A billionth is taken off first so that a quarter turn at 64 is 16 and not 17 when
/// the product rounds up by an ulp.)
pub fn segments(turns: f64, detail: u32) -> u64 {
    let x = ceil(turns * detail as f64 - 1e-9);
    if x.is_finite() && x > 2.0 {
        x as u64
    } else {
        2
    }
}

/// The sag of a circle of radius `r` cut into `n` equal chords: `r (1 − cos(π / n))`, the
/// greatest distance between a chord and its arc.
pub fn sag(r: f64, n: u32) -> f64 {
    r * versin(PI / n.max(1) as f64)
}

/// The least detail at which a circle of radius `r` has no chord farther than `error` metres
/// from its arc, within [`DETAIL_MIN`]..=[`DETAIL_MAX`] (so the answer can be the greatest
/// detail and still not meet a very small error: check with [`sag`]).
pub fn detail_for(r: f64, error: f64) -> u32 {
    if !(r > 0.0 && error > 0.0) || error >= r {
        return DETAIL_MIN;
    }
    // r (1 − cos(π/n)) = 2 r sin²(π/2n) <= e  <=>  π/n <= 2 asin(√(e / 2r)) (no cancellation
    // where e is far under r, as `acos(1 − e/r)` had).
    let n = ceil(PI / (2.0 * asin((0.5 * error / r).sqrt())));
    let n = if n.is_finite() { n.min(DETAIL_MAX as f64) as u32 } else { DETAIL_MAX };
    let mut n = n.clamp(DETAIL_MIN, DETAIL_MAX);
    // Rounding in acos can leave the answer one short: step up until it holds.
    while n < DETAIL_MAX && sag(r, n) > error {
        n += 1;
    }
    n
}
