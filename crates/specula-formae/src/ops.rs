//! Operators: solids made from any plane figure of the table with one ring (no holes).
//!
//! - [`prism_of`]: the figure raised straight up by a height (its outline's `(u, v)` is
//!   `(x, −z)`, so seen from above it is as drawn; it stands on `y = 0`).
//! - [`pyramid_of`]: the figure raised to an apex over its origin. The apex must see every edge
//!   of the figure from inside it (each edge turning counter-clockwise about the origin), or the
//!   pyramid's faces would fold through each other: refused as `apex_not_over_the_figure`
//!   otherwise, at the detail asked for (a curved figure is checked on its samples).
//! - [`revolve_of`]: the figure turned about the Y axis, moved out from it by an offset: the
//!   figure's `u` is the distance from the axis less the offset and its `v` the height (lifted
//!   so that it stands on `y = 0`). It must stay clear of the axis (every point at a positive
//!   distance), so it is a ring torus of that section (genus 1), or, through an angle under a
//!   full turn, a bend of one, capped.
//!
//! What they make is a [`Solid`]: counted before building, built (well made or refused as data,
//! as a table form's solid), measured in closed form where the figure's closed forms allow (a
//! prism's volume is its area times its height, exactly; a pyramid's a third of that; a solid of
//! revolution's from its profile's integrals where the figure is lines and arcs), with its
//! error at a detail.
//!
//! A figure with holes is refused (`one_ring_only`): its caps would be faces with holes, which
//! this library's faces are not.

use crate::error::Error;
use crate::geom::{Geom, Seg};
use crate::maths::*;
use crate::mesh::Mesh;
use crate::solid::{Cone, Extrude, Make, Piece, Rev};
use crate::spec::{solid_build, solid_refusal, Spec};
use crate::table::{Kind, LEN_MAX, LEN_MIN};
use crate::{Count, Measures, DETAIL_MAX, DETAIL_MIN, MAX_POINTS};
use std::rc::Rc;

/// A solid an operator made from a plane figure.
#[derive(Clone)]
pub struct Solid {
    /// What made it: `prism_of`, `pyramid_of` or `revolve_of`.
    pub operator: &'static str,
    /// The plane figure's name.
    pub figure: &'static str,
    make: Rc<dyn Make>,
}

impl std::fmt::Debug for Solid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Solid").field("operator", &self.operator).field("figure", &self.figure).finish()
    }
}

impl Solid {
    fn check_detail(detail: u32) -> Result<(), Error> {
        if (DETAIL_MIN..=DETAIL_MAX).contains(&detail) {
            Ok(())
        } else {
            Err(Error::DetailOutOfRange { detail, min: DETAIL_MIN, max: DETAIL_MAX })
        }
    }
    /// The points and faces a build at `detail` has, exactly, before any work.
    pub fn count(&self, detail: u32) -> Result<Count, Error> {
        Solid::check_detail(detail)?;
        let (points, parts) = self.make.count(detail);
        Ok(Count { points, parts })
    }
    /// Builds it at `detail`: a well-made mesh, or a refusal as data (the ceiling first, then
    /// the same rules as a table form's solid: [`Spec::build`]).
    pub fn build(&self, detail: u32) -> Result<Mesh, Error> {
        Solid::check_detail(detail)?;
        let (points, _) = self.make.count(detail);
        if points > MAX_POINTS {
            return Err(Error::TooManyPoints { form: self.figure, points, ceiling: MAX_POINTS });
        }
        let (mesh, f) = solid_build(self.make.as_ref(), detail);
        match f {
            None => Ok(mesh),
            Some(f) => Err(solid_refusal(self.figure, self.make.as_ref(), detail, &mesh, f)),
        }
    }
    /// The closed-form measures (volume, area, centroid, bounds) where they are known.
    pub fn closed(&self) -> Measures {
        self.make.exact()
    }
    /// A bound on the build's distance from the true solid at `detail`, where one is known.
    pub fn error(&self, detail: u32) -> Result<Option<f64>, Error> {
        Solid::check_detail(detail)?;
        Ok(self.make.error(detail))
    }
    /// The genus: 0 for a ball, 1 for a ring.
    pub fn genus(&self) -> u32 {
        self.make.genus()
    }
}

/// The one ring of a plane figure's geometry, or why there is not one.
fn one_ring(figure: &Spec, operator: &'static str) -> Result<Vec<Seg>, Error> {
    if figure.form.kind != Kind::Plane {
        return Err(Error::Constraint { form: figure.form.name, rule: "plane_figure_only", message: format!("{operator} takes a plane figure, and {} is a {}", figure.name(), figure.form.kind.name()), values: Vec::new() });
    }
    match figure.geom_of()? {
        Geom::Rings(mut rings) if rings.len() == 1 => Ok(rings.remove(0)),
        Geom::Rings(rings) => Err(Error::Constraint {
            form: figure.form.name,
            rule: "one_ring_only",
            message: format!("{operator} takes a figure of one ring; this one has holes (its caps would be faces with holes)"),
            values: vec![("rings", rings.len() as f64)],
        }),
        _ => unreachable!("a plane figure's geometry is rings"),
    }
}

fn length(operator: &'static str, name: &'static str, v: f64) -> Result<f64, Error> {
    if v.is_finite() && (LEN_MIN..=LEN_MAX).contains(&v) {
        Ok(v)
    } else {
        Err(Error::OutOfRange { form: operator, param: name, value: v, min: LEN_MIN, max: LEN_MAX })
    }
}

/// The figure raised straight up by `height` metres: a prism of its outline.
pub fn prism_of(figure: &Spec, height: f64) -> Result<Solid, Error> {
    let segs = one_ring(figure, "prism_of")?;
    let height = length("prism_of", "height", height)?;
    let plane = figure.closed()?;
    Ok(Solid { operator: "prism_of", figure: figure.form.name, make: Rc::new(Extrude { segs, height, plane }) })
}

/// The figure raised to an apex `height` metres over its origin: a pyramid (a cone over a
/// curved figure). Its lateral area is not given in closed form.
pub fn pyramid_of(figure: &Spec, height: f64) -> Result<Solid, Error> {
    let segs = one_ring(figure, "pyramid_of")?;
    let height = length("pyramid_of", "height", height)?;
    let plane = figure.closed()?;
    Ok(Solid { operator: "pyramid_of", figure: figure.form.name, make: Rc::new(Cone { segs, height, plane, lateral: None }) })
}

/// The figure turned about the Y axis through `angle` degrees (360 for a full turn), its `u`
/// moved out by `offset` metres (it must stay clear of the axis), lifted to stand on `y = 0`.
pub fn revolve_of(figure: &Spec, offset: f64, angle: f64) -> Result<Solid, Error> {
    let segs = one_ring(figure, "revolve_of")?;
    if !(angle.is_finite() && (1.0..=360.0).contains(&angle)) {
        return Err(Error::OutOfRange { form: "revolve_of", param: "angle", value: angle, min: 1.0, max: 360.0 });
    }
    if !(offset.is_finite() && offset.abs() <= LEN_MAX) {
        return Err(Error::OutOfRange { form: "revolve_of", param: "offset", value: offset, min: -LEN_MAX, max: LEN_MAX });
    }
    let Some((lo, hi)) = figure.closed()?.bounds else {
        return Err(Error::Constraint { form: figure.form.name, rule: "bounds_unknown", message: "revolve_of needs the figure's exact bounds to keep it clear of the axis, and this figure has none in closed form".into(), values: Vec::new() });
    };
    // The nearest a point comes to the axis (its true outline's, not a sample's: a sample lies
    // on the outline, so no farther in).
    if lo[0] + offset <= 0.0 {
        return Err(Error::Constraint { form: figure.form.name, rule: "touches_the_axis", message: "the figure, moved out by the offset, must lie wholly off the axis".into(), values: vec![("least_u", lo[0]), ("offset", offset), ("greatest_u", hi[0])] });
    }
    let shift = [offset, -lo[1]];
    let moved = move |p: Pt| [p[0] + shift[0], p[1] + shift[1]];
    let pieces = segs
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let seg = match s {
                Seg::Line { a, b } => Seg::Line { a: moved(a), b: moved(b) },
                Seg::Arc { c, r, a0, sweep } => Seg::Arc { c: moved(c), r, a0, sweep },
                Seg::Fn { f, t0, t1, turns } => Seg::Fn { f: Rc::new(move |t| moved(f(t))), t0, t1, turns },
            };
            Piece { seg, name: format!("side_{i}").into(), normal: None }
        })
        .collect();
    let sweep = if angle >= 360.0 { TAU } else { rad(angle) };
    Ok(Solid { operator: "revolve_of", figure: figure.form.name, make: Rc::new(Rev { pieces, closed: true, sweep, scale: [1.0, 1.0], cut: "cut" }) })
}
