//! Anchors: named points of a form, for placing and joining.

/// Where an anchor lies, which a test holds it to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum On {
    /// A vertex of the outline: a point every build has exactly.
    Vertex,
    /// On the boundary (between vertices, perhaps on a curved part: then within the build's
    /// chord error of the built outline).
    Boundary,
    /// Inside the figure.
    Inside,
    /// A point of reference that need not be on or in the figure (the origin of a crescent).
    Free,
}

/// A named point, with an outward direction where it has one. Plane anchors have `z = 0`.
#[derive(Clone, Debug, PartialEq)]
pub struct Anchor {
    /// Its name (snake_case; a numbered one ends in `_<index>`).
    pub name: String,
    /// Where it is, in metres.
    pub at: [f64; 3],
    /// The outward direction there, a unit vector, where there is one.
    pub dir: Option<[f64; 3]>,
    /// Where it lies.
    pub on: On,
}

/// The most numbered anchors of one kind a form gives (a polygon's vertices past this are not
/// named).
pub const MAX_NUMBERED: usize = 64;

/// A plane anchor.
pub(crate) fn at(name: &str, p: [f64; 2], on: On) -> Anchor {
    Anchor { name: name.to_string(), at: [p[0], p[1], 0.0], dir: None, on }
}

/// A plane anchor with an outward direction (normalised here).
pub(crate) fn out(name: &str, p: [f64; 2], d: [f64; 2], on: On) -> Anchor {
    let l = (d[0] * d[0] + d[1] * d[1]).sqrt();
    Anchor { name: name.to_string(), at: [p[0], p[1], 0.0], dir: Some([d[0] / l, d[1] / l, 0.0]), on }
}

/// Numbered vertex anchors `prefix_0`.. for the first [`MAX_NUMBERED`] points.
pub(crate) fn vertices(prefix: &str, pts: &[[f64; 2]]) -> Vec<Anchor> {
    pts.iter().take(MAX_NUMBERED).enumerate().map(|(i, p)| at(&format!("{prefix}_{i}"), *p, On::Vertex)).collect()
}
