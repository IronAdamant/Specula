//! A form with its arguments resolved: what is counted, built and measured.

use crate::anchor::{self, Anchor, On};
use crate::detail::{DETAIL_MAX, DETAIL_MIN};
use crate::error::Error;
use crate::geom::Geom;
use crate::table::{find, Alias, Form, Found, Kind, Param, Set};
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

    /// Whether the arguments make the figure (its cross-parameter rules hold).
    pub fn check(&self) -> Result<(), Error> {
        self.geom().map(|_| ())
    }

    /// The points and parts (rings for a plane figure, 1 for a curve) a build at `detail`
    /// will have, exactly, before any work.
    pub fn count(&self, detail: u32) -> Result<Count, Error> {
        Spec::check_detail(detail)?;
        let (points, parts) = self.geom()?.count(detail);
        Ok(Count { points, parts })
    }

    /// Builds the form at `detail`. A build whose count is over [`MAX_POINTS`] is refused
    /// with both numbers, before any sampling.
    pub fn build(&self, detail: u32) -> Result<Built, Error> {
        Spec::check_detail(detail)?;
        let g = self.geom()?;
        let (points, _) = g.count(detail);
        if points > MAX_POINTS {
            return Err(Error::TooManyPoints { form: self.form.name, points, ceiling: MAX_POINTS });
        }
        Ok(g.build(detail))
    }

    /// The closed-form measures: exact from the geometry where every piece is a line or an
    /// arc; otherwise the form's typed formulas where it has them; `None` where neither.
    pub fn closed(&self) -> Result<Measures, Error> {
        let g = self.geom()?;
        let path = g.exact();
        let typed = (self.form.imp.closed)(&self.args);
        Ok(Measures {
            size: path.size.or(typed.size),
            boundary: path.boundary.or(typed.boundary),
            centroid: path.centroid.or(typed.centroid),
            bounds: path.bounds.or(typed.bounds),
            series: if path.boundary.is_some() { None } else { typed.series },
        })
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
}
