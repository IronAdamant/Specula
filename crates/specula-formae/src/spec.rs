//! A form with its arguments resolved: what is counted, built and measured.

use crate::anchor::{self, Anchor, On};
use crate::detail::{DETAIL_MAX, DETAIL_MIN};
use crate::error::Error;
use crate::geom::Geom;
use crate::table::{find, Alias, Form, Found, Kind, Param, Set};
use crate::outline::{Fault, EDGE_FLOOR};
#[cfg(doc)]
use crate::outline::{Curve, Outline};
use crate::mesh::{Mesh, SolidFault};
use crate::solid::Make;
use crate::{Built, Count, Measures, MAX_POINTS};

/// A form's parameters, resolved: every parameter of the form has its value (defaults, then an
/// alias's presets, then what was given).
#[derive(Clone, Debug, PartialEq)]
pub struct Args {
    params: &'static [Param],
    vals: Vec<f64>,
}

impl Args {
    /// The value of the form's parameter `name`. Panics on a name the form does not have (a
    /// mistake in the form's own code, which the table's tests reach).
    pub fn f(&self, name: &str) -> f64 {
        match self.params.iter().position(|p| p.name == name) {
            Some(i) => self.vals[i],
            None => panic!("no parameter {name}"),
        }
    }
    /// Whether the form has a parameter of this name.
    pub fn has(&self, name: &str) -> bool {
        self.params.iter().any(|p| p.name == name)
    }
    /// The value of a whole-number parameter.
    pub fn n(&self, name: &str) -> usize {
        self.f(name) as usize
    }
    /// The parameters' values in the form's order.
    pub fn values(&self) -> &[f64] {
        &self.vals
    }
}

/// The parameters a word takes: its form's, less what its alias fixes or ties, plus the tied
/// parameters' new names.
pub fn params_of(found: &Found) -> Vec<Param> {
    let Some(alias) = found.alias else { return found.form.params.to_vec() };
    let mut out = Vec::new();
    for p in found.form.params {
        let mut hidden = false;
        for s in alias.set {
            match s {
                Set::Fixed(n, _) if *n == p.name => hidden = true,
                Set::Tie { name, about, targets } if targets.contains(&p.name) => {
                    hidden = true;
                    if targets[0] == p.name {
                        out.push(Param { name, about, ..*p });
                    }
                }
                _ => {}
            }
        }
        if !hidden {
            out.push(*p);
        }
    }
    out
}

/// A form with its arguments: made by [`Spec::new`], it counts, builds and measures.
#[derive(Clone, Debug)]
pub struct Spec {
    /// The form.
    pub form: &'static Form,
    /// The alias it was named by, if any.
    pub alias: Option<&'static Alias>,
    /// Its resolved arguments.
    pub args: Args,
}

impl Spec {
    /// Finds `name` (of `kind`, when the word names more than one kind) and resolves `given`
    /// against the parameters it takes: each must be one of them, given once, finite, within
    /// its range and whole where it is a count.
    pub fn new(name: &str, kind: Option<Kind>, given: &[(&str, f64)]) -> Result<Spec, Error> {
        let found = find(name, kind)?;
        Spec::of(found, given)
    }

    /// As [`Spec::new`], from a word already found.
    pub fn of(found: Found, given: &[(&str, f64)]) -> Result<Spec, Error> {
        let takes = params_of(&found);
        let word = found.name();
        let form = found.form;
        let mut vals: Vec<f64> = form.params.iter().map(|p| p.default).collect();
        let set = |vals: &mut Vec<f64>, name: &str, v: f64| {
            if let Some(i) = form.params.iter().position(|p| p.name == name) {
                vals[i] = v;
            }
        };
        if let Some(a) = found.alias {
            for s in a.set {
                if let Set::Fixed(n, v) = s {
                    set(&mut vals, n, *v);
                }
            }
        }
        let mut seen: Vec<&str> = Vec::new();
        let mut chosen: Vec<(&'static str, f64)> = takes.iter().map(|p| (p.name, p.default)).collect();
        for (n, v) in given {
            if let Some(a) = found.alias {
                for s in a.set {
                    if let Set::Fixed(fixed, value) = s {
                        if fixed == n && !takes.iter().any(|p| p.name == *n) {
                            return Err(Error::FixedByAlias { alias: a.name, param: fixed, value: *value });
                        }
                    }
                }
            }
            let Some(p) = takes.iter().find(|p| p.name == *n) else {
                return Err(Error::UnknownParam { form: word, param: n.to_string(), takes: takes.iter().map(|p| p.name).collect() });
            };
            if seen.contains(n) {
                return Err(Error::DuplicateParam { form: word, param: p.name });
            }
            seen.push(n);
            if !(v.is_finite() && *v >= p.min && *v <= p.max) {
                return Err(Error::OutOfRange { form: word, param: p.name, value: *v, min: p.min, max: p.max });
            }
            if p.integer && crate::maths::floor(*v) != *v {
                return Err(Error::NotInteger { form: word, param: p.name, value: *v });
            }
            if let Some(c) = chosen.iter_mut().find(|c| c.0 == p.name) {
                c.1 = *v;
            }
        }
        for (n, v) in chosen {
            let tie = found.alias.and_then(|a| {
                a.set.iter().find_map(|s| match s {
                    Set::Tie { name, targets, .. } if *name == n => Some(*targets),
                    _ => None,
                })
            });
            match tie {
                Some(targets) => targets.iter().for_each(|t| set(&mut vals, t, v)),
                None => set(&mut vals, n, v),
            }
        }
        Ok(Spec { form, alias: found.alias, args: Args { params: form.params, vals } })
    }

    /// The word it was named by.
    pub fn name(&self) -> &'static str {
        self.alias.map(|a| a.name).unwrap_or(self.form.name)
    }

    fn geom(&self) -> Result<Geom, Error> {
        (self.form.imp.geom)(&self.args).map_err(|r| Error::Constraint { form: self.form.name, rule: r.rule, message: r.message, values: r.values })
    }

    fn check_detail(detail: u32) -> Result<(), Error> {
        if (DETAIL_MIN..=DETAIL_MAX).contains(&detail) {
            Ok(())
        } else {
            Err(Error::DetailOutOfRange { detail, min: DETAIL_MIN, max: DETAIL_MAX })
        }
    }

    /// The exact geometry (for the operators).
    pub(crate) fn geom_of(&self) -> Result<Geom, Error> {
        self.geom()
    }

    /// Whether the arguments make the figure (its cross-parameter rules hold).
    pub fn check(&self) -> Result<(), Error> {
        self.geom().map(|_| ())
    }

    /// The points and parts (rings for a plane figure, 1 for a curve) a build at `detail`
    /// will have, exactly, before any work. (It is what the build has when it is made: a
    /// build can still be refused as not well made at that detail, which only sampling shows:
    /// see [`Spec::build`].)
    pub fn count(&self, detail: u32) -> Result<Count, Error> {
        Spec::check_detail(detail)?;
        let (points, parts) = self.geom()?.count(detail);
        Ok(Count { points, parts })
    }

    /// Builds the form at `detail`. **What it returns is well made, or it is refused as
    /// data**: an [`Outline`] that [`Outline::validate`] accepts, or a [`Curve`] that
    /// [`Curve::validate`] accepts. In order:
    ///
    /// - a build whose count is over [`MAX_POINTS`] is refused with both numbers
    ///   ([`Error::TooManyPoints`]), before any sampling;
    /// - the build is sampled, and checked (near linear in its points);
    /// - an edge at most [`EDGE_FLOOR`] of the figure's size is refused as
    ///   [`Error::Constraint`] with the rule **`edge_under_floor`**: the figure's parts are
    ///   farther apart in size than the floor allows (a hole of a tenth of a millimetre in a
    ///   disc of a kilometre). Its values are the edge's `ring`, `point` and length (`edge`),
    ///   the figure's `size`, the `floor` and the `detail`; and, where one of the coarser
    ///   details `detail / 2`, `detail / 4`, ... (down to [`DETAIL_MIN`]) builds well made,
    ///   the finest of them as `coarser_detail_that_holds` (short edges that are a curve's
    ///   samples grow at a coarser detail; a straight edge does not, and then none is named);
    ///   `searched_to` is the last detail tried. Two edges that do not cross but come no farther
    ///   apart than the floor (a corner that sharp, or two parts that near) are refused by the
    ///   same rule, with the two edges (`ring_a`, `edge_a`, `ring_b`, `edge_b`) and the `gap` in
    ///   place of the one edge: the crossing test cannot tell them from crossing ones;
    /// - two edges that cross are refused with the rule **`crosses_at_this_detail`**: the
    ///   true figure is simple, and its chords stray from it by the build's chord error,
    ///   which at this detail is more than half the gap between two of its parts (a spiral's
    ///   turns, far out). Its values are the two edges (`ring_a`, `edge_a`, `ring_b`,
    ///   `edge_b`), the `detail` and the `chord_error`; and, where one of `detail × 2`,
    ///   `detail × 4`, ... (up to [`DETAIL_MAX`], under the ceiling) builds well made, the
    ///   coarsest of them as `finer_detail_that_holds`; `searched_to` is the last detail
    ///   tried;
    /// - any other fault (a ring wound the wrong way, a hole outside) would be a mistake in the
    ///   form's own geometry, refused with the rule `not_well_made`; the crate's tests hold that
    ///   no form makes one.
    ///
    /// Refusing, and not building something else, keeps the count exact and the detail what
    /// was asked for. The search for a detail that holds builds at most nine more times, each
    /// under the ceiling, and only on a refusal.
    pub fn build(&self, detail: u32) -> Result<Built, Error> {
        Spec::check_detail(detail)?;
        let g = self.geom()?;
        let (points, _) = g.count(detail);
        if points > MAX_POINTS {
            return Err(Error::TooManyPoints { form: self.form.name, points, ceiling: MAX_POINTS });
        }
        if let Geom::Solid(m) = &g {
            let (mesh, f) = solid_build(m.as_ref(), detail);
            return match f {
                None => Ok(Built::Mesh(mesh)),
                Some(f) => Err(self.solid_not_well_made(&g, detail, &mesh, f)),
            };
        }
        let b = g.build(detail);
        match fault_of(&b) {
            None => Ok(b),
            Some(f) => Err(self.not_well_made(&g, detail, &b, f)),
        }
    }

    /// The genus of the solid these arguments make (0 for a ball, 1 for a torus); `None` for a
    /// plane figure or a curve, or arguments that do not make the form.
    pub fn genus(&self) -> Option<u32> {
        match self.geom() {
            Ok(Geom::Solid(m)) => Some(m.genus()),
            _ => None,
        }
    }

    /// The refusal of a solid's build at `detail` that has the fault `f`: the same rules as a
    /// plane figure's (`edge_under_floor`, `crosses_at_this_detail`, `not_well_made`), with a
    /// solid's numbers: a short edge's `face` and `edge` (or, in the section the solid is made
    /// from, its `point`), two crossing faces' `face_a` and `face_b` (or the section's
    /// `ring_a`, `edge_a`, `ring_b`, `edge_b`, or a section's two edges nearer than the floor
    /// with their `gap`). And one rule of a solid's own, **`normal_behind_face`**: a normal the
    /// build gives at a face's corner lies on the face's inner side (its `face`, its `corner`,
    /// the `cos` of the angle to the face's own normal, and the coarsest of `detail × 2`,
    /// `× 4`, ... that holds as `finer_detail_that_holds`). The table's forms are built so that
    /// it cannot happen (each form's construction says why); a tube swept along a curve is
    /// refused by it where a detail is too coarse for its bends.
    fn solid_not_well_made(&self, g: &Geom, detail: u32, b: &Mesh, f: SolidFault) -> Error {
        let Geom::Solid(m) = g else { unreachable!("a solid's geometry") };
        solid_refusal(self.form.name, m.as_ref(), detail, b, f)
    }
}

/// The refusal of a solid's build (see [`Spec::build`] and the solid rules there), for a form
/// of the table or a solid an operator made.
pub(crate) fn solid_refusal(form: &'static str, m: &dyn Make, detail: u32, b: &Mesh, f: SolidFault) -> Error {
    {
        let size = b.size();
        let d = detail as f64;
        let constraint = |rule: &'static str, message: &str, values: Vec<(&'static str, f64)>| Error::Constraint { form, rule, message: message.to_string(), values };
        let search = |finer: bool| holds_at(|x| m.count(x).0, |x| class_of_solid(&solid_build(m, x).1), detail, finer);
        match f {
            SolidFault::ShortEdge { length, .. } | SolidFault::SectionShortEdge { length, .. } => {
                let mut v = match f {
                    SolidFault::ShortEdge { face, edge, .. } => vec![("face", face as f64), ("corner", edge as f64)],
                    SolidFault::SectionShortEdge { point, .. } => vec![("point", point as f64)],
                    _ => Vec::new(),
                };
                v.extend([("edge", length), ("size", size), ("floor", EDGE_FLOOR * size), ("detail", d)]);
                let (holds, last) = search(false);
                if let Some(h) = holds {
                    v.push(("coarser_detail_that_holds", h as f64));
                }
                v.push(("searched_to", last as f64));
                constraint("edge_under_floor", "an edge of the build is no longer than a billionth of the solid's size: its parts are too far apart in size (a coarser detail is named where one holds)", v)
            }
            SolidFault::Crossing { .. } | SolidFault::SectionCrossing { .. } => {
                let mut v = match f {
                    SolidFault::Crossing { a, b } => vec![("face_a", a as f64), ("face_b", b as f64)],
                    SolidFault::SectionCrossing { a, b } => vec![("ring_a", a.0 as f64), ("edge_a", a.1 as f64), ("ring_b", b.0 as f64), ("edge_b", b.1 as f64)],
                    _ => Vec::new(),
                };
                v.extend([("detail", d), ("chord_error", b.chord_error)]);
                let (holds, last) = search(true);
                if let Some(h) = holds {
                    v.push(("finer_detail_that_holds", h as f64));
                }
                v.push(("searched_to", last as f64));
                constraint("crosses_at_this_detail", "the build's faces (or the section it is made from) cross at this detail: where none holds, the solid's own surface comes too near itself", v)
            }
            SolidFault::SectionGap { a, b, gap } => {
                let mut v = vec![("ring_a", a.0 as f64), ("edge_a", a.1 as f64), ("ring_b", b.0 as f64), ("edge_b", b.1 as f64), ("gap", gap), ("size", size), ("floor", EDGE_FLOOR * size), ("detail", d)];
                let (holds, last) = search(false);
                if let Some(h) = holds {
                    v.push(("coarser_detail_that_holds", h as f64));
                }
                v.push(("searched_to", last as f64));
                constraint("edge_under_floor", "two edges of the section the solid is made from come no farther apart than a billionth of the solid's size: its parts are too far apart in size (a coarser detail is named where one holds)", v)
            }
            SolidFault::NormalBehind { face, corner, cos } => {
                // A corner's normal on its face's inner side: the faces turn too far across the
                // surface's normal at this detail (a finer one turns them less).
                let mut v = vec![("face", face as f64), ("corner", corner as f64), ("cos", cos), ("detail", d)];
                let (holds, last) = search(true);
                if let Some(h) = holds {
                    v.push(("finer_detail_that_holds", h as f64));
                }
                v.push(("searched_to", last as f64));
                constraint("normal_behind_face", "a normal the build gives at a face's corner lies on the face's inner side (a right angle or more from the face's own): the faces are too coarse for how fast the surface turns there (a finer detail is named where one holds under the ceiling)", v)
            }
            SolidFault::InsetTooDeep { inset, param, holds, searched } => match holds {
                Some((s, p)) => {
                    let named = match param {
                        "width" => "width_that_holds",
                        "run" => "run_that_holds",
                        _ => "inset_that_holds",
                    };
                    constraint("inset_too_deep", &format!("the figure's outline, inset this far, collapses an edge or stops being simple (its parts would meet): a raised form is not built past that; the deepest inset that holds at this detail is named, with the {param} that gives it (the same form built with it was built, and holds)"), vec![("inset", inset), ("deepest_inset_that_holds", s), (named, p), ("detail", d)])
                }
                None if !searched => constraint("inset_too_deep", &format!("the figure's outline, inset this far, collapses an edge or stops being simple (its parts would meet); the deepest {param} that holds was not searched for (its search builds the form again up to {} times, and the caller's budget had no room for it): ask for a shallower {param}, or for this form alone where its budget allows the search", crate::raise::SEARCH_BUILDS), vec![("inset", inset), ("detail", d)]),
                None => constraint("inset_too_deep", &format!("the figure's outline, inset this far, collapses an edge or stops being simple (its parts would meet), and so does every inset that may be asked for: no {param} from the least length up holds at this detail (a corner too sharp, or parts too near, for any inset); raise it as a slab, or at another detail"), vec![("inset", inset), ("least_length", crate::table::LEN_MIN), ("detail", d)]),
            },
            SolidFault::ApexOutside { edge } => constraint("apex_not_over_the_figure", "the apex must see every edge of the figure from inside it (each edge turning counter-clockwise about the origin under the apex), or the faces fold through each other: move the figure so that its origin is inside it and sees all of it", vec![("edge", edge as f64), ("detail", d)]),
            SolidFault::ZeroArea { face, area } => {
                // A face thinner than the floor (its area over its longest edge): a needle's, whose
                // parts are as far apart in size as a short edge's.
                let f = &b.faces[face];
                let n = f.verts.len();
                let longest = (0..n).map(|i| crate::maths::dist3(b.points[f.verts[i] as usize], b.points[f.verts[(i + 1) % n] as usize])).fold(0.0, f64::max);
                let mut v = vec![("face", face as f64), ("width", area / longest), ("size", size), ("floor", EDGE_FLOOR * size), ("detail", d)];
                let (holds, last) = search(false);
                if let Some(h) = holds {
                    v.push(("coarser_detail_that_holds", h as f64));
                }
                v.push(("searched_to", last as f64));
                constraint("edge_under_floor", "a face of the build is thinner than a billionth of the solid's size: its parts are too far apart in size (a coarser detail is named where one holds)", v)
            }
            other => constraint("not_well_made", &format!("the build is not well made ({other:?}): a mistake in the form's geometry"), vec![("detail", d)]),
        }
    }
}

impl Spec {

    /// The refusal of a build at `detail` that has the fault `f`.
    fn not_well_made(&self, g: &Geom, detail: u32, b: &Built, f: Fault) -> Error {
        let size = match b {
            Built::Outline(o) => o.size(),
            Built::Mesh(m) => m.size(),
            Built::Curve(c) => c.size(),
        };
        let d = detail as f64;
        let constraint = |rule: &'static str, message: &str, values: Vec<(&'static str, f64)>| Error::Constraint { form: self.form.name, rule, message: message.to_string(), values };
        match f {
            Fault::ShortEdge { ring, point, length } => {
                let (holds, last) = plane_holds_at(g, detail, false);
                let mut v = vec![("ring", ring as f64), ("point", point as f64), ("edge", length), ("size", size), ("floor", EDGE_FLOOR * size), ("detail", d)];
                if let Some(h) = holds {
                    v.push(("coarser_detail_that_holds", h as f64));
                }
                v.push(("searched_to", last as f64));
                constraint("edge_under_floor", "an edge of the build is no longer than a billionth of the figure's size: its parts are too far apart in size (a coarser detail is named where one holds)", v)
            }
            Fault::Gap { a, b: e, gap } => {
                let (holds, last) = plane_holds_at(g, detail, false);
                let mut v = vec![("ring_a", a.0 as f64), ("edge_a", a.1 as f64), ("ring_b", e.0 as f64), ("edge_b", e.1 as f64), ("gap", gap), ("size", size), ("floor", EDGE_FLOOR * size), ("detail", d)];
                if let Some(h) = holds {
                    v.push(("coarser_detail_that_holds", h as f64));
                }
                v.push(("searched_to", last as f64));
                constraint("edge_under_floor", "two edges of the build come no farther apart than a billionth of the figure's size (a corner that sharp, or two parts that near): its parts are too far apart in size for its build to be told well made (a coarser detail is named where one holds)", v)
            }
            Fault::Crossing { a, b: e } => {
                let (holds, last) = plane_holds_at(g, detail, true);
                let mut v = vec![("ring_a", a.0 as f64), ("edge_a", a.1 as f64), ("ring_b", e.0 as f64), ("edge_b", e.1 as f64), ("detail", d), ("chord_error", b.chord_error())];
                if let Some(h) = holds {
                    v.push(("finer_detail_that_holds", h as f64));
                }
                v.push(("searched_to", last as f64));
                constraint("crosses_at_this_detail", "the build's chords cross at this detail, though the figure does not: they stray from it farther than half the gap between two of its parts (a finer detail is named where one holds under the ceiling)", v)
            }
            other => constraint("not_well_made", &format!("the build is not well made ({other:?}): a mistake in the form's geometry"), vec![("detail", d)]),
        }
    }

    /// The closed-form measures. For a plane figure or a curve: exact from the geometry where
    /// every piece is a line or an arc; otherwise the form's typed formulas where it has them;
    /// `None` where neither. For a solid the other way about: the form's typed formulas first,
    /// each written so that nothing cancels at the ends of its parameters' ranges (a thin
    /// shell's volume from its thickness, not as a difference of cubes), and the geometry's
    /// where the form types none. (A thin shell's profile of two arcs cannot be integrated
    /// without that difference: integrated along it, a dome 1e-4 thick of radius 1e4 keeps 8
    /// digits.) A test holds the typed formulas against the geometry's wherever both exist.
    pub fn closed(&self) -> Result<Measures, Error> {
        let g = self.geom()?;
        let path = g.exact();
        let typed = (self.form.imp.closed)(&self.args);
        if let Geom::Solid(_) = g {
            return Ok(Measures {
                size: typed.size.or(path.size),
                boundary: typed.boundary.or(path.boundary),
                centroid: typed.centroid.or(path.centroid),
                bounds: typed.bounds.or(path.bounds),
                series: if typed.boundary.is_some() || path.boundary.is_none() { typed.series } else { None },
            });
        }
        Ok(Measures {
            size: path.size.or(typed.size),
            boundary: path.boundary.or(typed.boundary),
            centroid: path.centroid.or(typed.centroid),
            bounds: path.bounds.or(typed.bounds),
            series: if path.boundary.is_some() { None } else { typed.series },
        })
    }

    /// The exact measures of the geometry alone (no typed formula), for comparing with the
    /// typed ones (for tests).
    #[cfg(test)]
    pub(crate) fn geometry(&self) -> Measures {
        self.geom().map(|g| g.exact()).unwrap_or_default()
    }

    /// The form's typed textbook formulas alone (no geometry), for comparing with the exact
    /// measures of its geometry.
    pub fn typed(&self) -> Measures {
        (self.form.imp.closed)(&self.args)
    }

    /// The greatest distance between the build at `detail` and the true figure, exactly, when
    /// every curved part is an arc of a circle (0 when nothing is curved); `None` otherwise,
    /// when the build's own `chord_error` is the measure.
    pub fn error(&self, detail: u32) -> Result<Option<f64>, Error> {
        Spec::check_detail(detail)?;
        Ok(self.geom()?.sag(detail))
    }

    /// How coarsely a plane figure's or a curve's curved pieces are cut at `detail`: the turn
    /// between neighbouring sample chords, degrees, and a radius, of the piece whose chords stand
    /// furthest off it (exact for arcs; for parametric pieces measured on the samples, the
    /// radius an estimate from their chord error). `None` for straight figures and for solids
    /// (a solid's mesh says, by `Mesh::rounds`).
    pub fn facets(&self, detail: u32) -> Result<Option<(f64, f64)>, Error> {
        Spec::check_detail(detail)?;
        Ok(self.geom()?.facets(detail).map(|(t, r)| (t * 180.0 / crate::maths::PI, r)))
    }

    /// The anchors: `origin` for every form, `start` and `end` for an open curve, and the
    /// form's own.
    pub fn anchors(&self) -> Result<Vec<Anchor>, Error> {
        let g = self.geom()?;
        let mut out = vec![anchor::at("origin", [0.0, 0.0], On::Free)];
        if let Geom::Path { segs, closed: false } = &g {
            if let (Some(a), Some(b)) = (segs.first(), segs.last()) {
                out.push(anchor::at("start", a.start(), On::Vertex));
                out.push(anchor::at("end", b.end(), On::Vertex));
            }
        }
        out.extend((self.form.imp.anchors)(&self.args));
        Ok(out)
    }

    /// The largest gap between consecutive pieces of the form's exact geometry (for tests).
    #[cfg(test)]
    pub(crate) fn gap(&self) -> f64 {
        self.geom().map(|g| g.gap()).unwrap_or(0.0)
    }

    /// The area a build at `detail` must have, for a figure of lines and arcs (for tests).
    #[cfg(test)]
    pub(crate) fn built_area(&self, detail: u32) -> Option<f64> {
        self.geom().ok()?.built_area(detail)
    }

    /// Whether the exact geometry has curved parts (for tests).
    #[cfg(test)]
    pub(crate) fn geom_curved(&self) -> bool {
        self.geom().map(|g| g.curved()).unwrap_or(false)
    }

    /// A solid's build at `detail` and the fault its checks find (for tests).
    #[cfg(test)]
    pub(crate) fn solid_fault(&self, detail: u32) -> Option<SolidFault> {
        match self.geom() {
            Ok(Geom::Solid(m)) => solid_build(m.as_ref(), detail).1,
            _ => None,
        }
    }

    /// The build at `detail` as sampled, unchecked (for tests: what [`Spec::build`] refuses).
    #[cfg(test)]
    pub(crate) fn raw(&self, detail: u32) -> Option<Built> {
        self.geom().ok().map(|g| g.build(detail))
    }
}

/// What is wrong with a build, if anything.
fn fault_of(b: &Built) -> Option<Fault> {
    match b {
        Built::Outline(o) => o.validate().err(),
        Built::Curve(c) => c.validate().err(),
        Built::Mesh(_) => None,
    }
}

/// A solid's build at `detail`, checked: the fault its construction found, else the mesh's own
/// ([`Mesh::validate`]), else, where the construction asks for it, two faces that cross.
pub(crate) fn solid_build(m: &dyn Make, detail: u32) -> (Mesh, Option<SolidFault>) {
    let (mesh, f) = m.build(detail);
    let f = f.or_else(|| mesh.validate(m.genus()).err()).or_else(|| m.check(&mesh, detail));
    (mesh, f)
}

/// The kind of a fault, for the search for a detail that holds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Bad {
    Short,
    Cross,
    Other,
}

fn class_of(f: &Option<Fault>) -> Option<Bad> {
    f.as_ref().map(|f| match f {
        Fault::ShortEdge { .. } | Fault::Gap { .. } => Bad::Short,
        Fault::Crossing { .. } => Bad::Cross,
        _ => Bad::Other,
    })
}

fn class_of_solid(f: &Option<SolidFault>) -> Option<Bad> {
    f.as_ref().map(|f| match f {
        SolidFault::ShortEdge { .. } | SolidFault::SectionShortEdge { .. } | SolidFault::SectionGap { .. } | SolidFault::ZeroArea { .. } => Bad::Short,
        // (A crossing and a normal behind its face are both mended by a finer detail.)
        SolidFault::Crossing { .. } | SolidFault::SectionCrossing { .. } | SolidFault::NormalBehind { .. } => Bad::Cross,
        _ => Bad::Other,
    })
}

/// From a detail at which the build is not well made, the nearest detail by halves
/// (`finer` false: `from / 2`, `from / 4`, ... down to [`DETAIL_MIN`]) or by doubles (`finer`:
/// up to [`DETAIL_MAX`], while the count is under the ceiling) at which it is; and the last
/// detail tried (`from` when none was). Going coarser it goes on only past short edges, and
/// going finer only past crossings: the other fault only grows that way.
fn holds_at(count: impl Fn(u32) -> u64, bad: impl Fn(u32) -> Option<Bad>, from: u32, finer: bool) -> (Option<u32>, u32) {
    let mut d = from;
    loop {
        let next = if finer {
            if d >= DETAIL_MAX {
                return (None, d);
            }
            d.saturating_mul(2).min(DETAIL_MAX)
        } else {
            if d <= DETAIL_MIN {
                return (None, d);
            }
            (d / 2).max(DETAIL_MIN)
        };
        if count(next) > MAX_POINTS {
            return (None, d);
        }
        d = next;
        match bad(d) {
            None => return (Some(d), d),
            Some(Bad::Short) if !finer => {}
            Some(Bad::Cross) if finer => {}
            Some(_) => return (None, d),
        }
    }
}

/// The plane search: [`holds_at`] over a plane figure's or a curve's geometry.
fn plane_holds_at(g: &Geom, from: u32, finer: bool) -> (Option<u32>, u32) {
    holds_at(|x| g.count(x).0, |x| class_of(&fault_of(&g.build(x))), from, finer)
}
