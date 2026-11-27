//! The forms, by family. Each file holds its forms' entries and their functions side by side.

/// A plane figure's entry.
macro_rules! plane {
    ($name:expr, $fam:ident, $about:expr, $origin:expr, $params:expr, curved: $c:expr, kin: $kin:expr, $geom:expr, $closed:expr, $anchors:expr) => {
        crate::table::Form {
            name: $name,
            kind: crate::table::Kind::Plane,
            family: crate::table::Family::$fam,
            about: $about,
            origin: $origin,
            params: $params,
            curved: $c,
            kin: $kin,
            band: None,
            imp: crate::table::Imp { geom: $geom, closed: $closed, anchors: $anchors },
        }
    };
}

/// A curve's entry.
macro_rules! curve {
    ($name:expr, $fam:ident, $about:expr, $origin:expr, $params:expr, curved: $c:expr, band: $band:expr, kin: $kin:expr, $geom:expr, $closed:expr, $anchors:expr) => {
        crate::table::Form {
            name: $name,
            kind: crate::table::Kind::Curve,
            family: crate::table::Family::$fam,
            about: $about,
            origin: $origin,
            params: $params,
            curved: $c,
            kin: $kin,
            band: $band,
            imp: crate::table::Imp { geom: $geom, closed: $closed, anchors: $anchors },
        }
    };
}

/// A solid's entry.
macro_rules! solid {
    ($name:expr, $fam:ident, $about:expr, $origin:expr, $params:expr, curved: $c:expr, kin: $kin:expr, $geom:expr, $closed:expr, $anchors:expr) => {
        crate::table::Form {
            name: $name,
            kind: crate::table::Kind::Solid,
            family: crate::table::Family::$fam,
            about: $about,
            origin: $origin,
            params: $params,
            curved: $c,
            kin: $kin,
            band: None,
            imp: crate::table::Imp { geom: $geom, closed: $closed, anchors: $anchors },
        }
    };
}

/// An alias of a plane figure, a curve or a solid.
macro_rules! alias {
    ($name:expr, $kind:ident, $of:expr, $about:expr, $set:expr) => {
        crate::table::Alias { name: $name, kind: crate::table::Kind::$kind, of: $of, about: $about, set: $set }
    };
}

pub(crate) mod curved;
pub(crate) mod curves;
pub(crate) mod flat;
pub(crate) mod ornamenta;
pub(crate) mod polygons;
pub(crate) mod round;
pub(crate) mod supellex;

use crate::anchor::Anchor;
use crate::spec::Args;
use crate::Measures;

/// No typed formulas.
pub(crate) fn none(_: &Args) -> Measures {
    Measures::default()
}

/// No anchors beyond the ones every form has.
pub(crate) fn no_anchors(_: &Args) -> Vec<Anchor> {
    Vec::new()
}

/// Typed measures: an area and a perimeter, either unknown.
pub(crate) fn ap(area: Option<f64>, perimeter: Option<f64>) -> Measures {
    Measures { size: area, boundary: perimeter, ..Measures::default() }
}

/// Typed measures of a curve: its length.
pub(crate) fn length(l: f64) -> Measures {
    Measures { boundary: Some(l), ..Measures::default() }
}
