//! The table: every form and every alias, and finding them by the words people use.

use crate::anchor::Anchor;
use crate::error::{Error, Refusal};
use crate::geom::Geom;
use crate::spec::Args;
use crate::Measures;
use std::sync::OnceLock;

/// What a form builds to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A plane figure: an outline of closed rings (the first the outer boundary,
    /// counter-clockwise; any further rings holes, clockwise).
    Plane,
    /// A curve: a run of points, open or closed, that bounds no area of its own (an arc, a
    /// spiral; a figure eight is a closed curve).
    Curve,
    /// A solid: a closed polygon mesh (stage 1b).
    Solid,
}

impl Kind {
    /// The kind's name in the table: `plane`, `curve` or `solid`.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Plane => "plane",
            Kind::Curve => "curve",
            Kind::Solid => "solid",
        }
    }
}

/// A form's family: what it is grouped with in the catalogue.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Family {
    /// Triangles.
    Triangle,
    /// Four-sided figures.
    Quadrilateral,
    /// Regular polygons.
    Polygon,
    /// Stars and star polygons.
    Star,
    /// Crosses, arrows, chevrons and shields.
    Symbol,
    /// The letter sections of structural steel (L, T, U, H, Z).
    Section,
    /// Figures with a hole: frames and rings.
    Frame,
    /// Gears.
    Gear,
    /// Strips with a pattern on one edge, and curves given a width.
    Band,
    /// Figures made of circles: circle, sectors, segments, rings, lunes, lenses, stadiums.
    Circle,
    /// Ovals, ellipses, eggs, drops and hearts.
    Oval,
    /// Arches, as the shape of the opening.
    Arch,
    /// Tracery: foils, Reuleaux polygons, keyholes, flowers.
    Tracery,
    /// Curves traced by a rolling circle, and polar curves: cycloids, limaçons, the
    /// superformula.
    Roulette,
    /// Straight curves: a segment, a zigzag.
    Line,
    /// Arcs of circles and ellipses, and the graphs of a parabola, a catenary and a sine.
    Arc,
    /// Spirals and the involute.
    Spiral,
    /// Closed curves that cross or touch themselves (a figure eight, a rose).
    Loop,
}

impl Family {
    /// The family's name in the table (snake_case).
    pub fn name(self) -> &'static str {
        match self {
            Family::Triangle => "triangle",
            Family::Quadrilateral => "quadrilateral",
            Family::Polygon => "polygon",
            Family::Star => "star",
            Family::Symbol => "symbol",
            Family::Section => "section",
            Family::Frame => "frame",
            Family::Gear => "gear",
            Family::Band => "band",
            Family::Circle => "circle",
            Family::Oval => "oval",
            Family::Arch => "arch",
            Family::Tracery => "tracery",
            Family::Roulette => "roulette",
            Family::Line => "line",
            Family::Arc => "arc",
            Family::Spiral => "spiral",
            Family::Loop => "loop",
        }
    }
}

/// What a parameter's number is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// Metres.
    Metres,
    /// Degrees.
    Degrees,
    /// A count (always a whole number).
    Count,
    /// A ratio (no unit).
    Ratio,
}

impl Unit {
    /// The unit's name: `m`, `deg`, `count` or `ratio`.
    pub fn name(self) -> &'static str {
        match self {
            Unit::Metres => "m",
            Unit::Degrees => "deg",
            Unit::Count => "count",
            Unit::Ratio => "ratio",
        }
    }
}

/// One parameter of a form.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Param {
    /// Its name (snake_case ASCII).
    pub name: &'static str,
    /// What it is, in a few words.
    pub about: &'static str,
    /// What its number is in.
    pub unit: Unit,
    /// Its value when not given.
    pub default: f64,
    /// Its least value.
    pub min: f64,
    /// Its greatest value.
    pub max: f64,
    /// Whether it must be a whole number.
    pub integer: bool,
}

/// The least length a parameter takes: a tenth of a millimetre.
pub const LEN_MIN: f64 = 1e-4;
/// The greatest length a parameter takes: ten kilometres.
pub const LEN_MAX: f64 = 1e4;

/// A length in metres, from [`LEN_MIN`] to [`LEN_MAX`].
pub(crate) const fn len(name: &'static str, about: &'static str, default: f64) -> Param {
    Param { name, about, unit: Unit::Metres, default, min: LEN_MIN, max: LEN_MAX, integer: false }
}
/// A whole count.
pub(crate) const fn count(name: &'static str, about: &'static str, default: f64, min: f64, max: f64) -> Param {
    Param { name, about, unit: Unit::Count, default, min, max, integer: true }
}
/// An angle in degrees.
pub(crate) const fn deg(name: &'static str, about: &'static str, default: f64, min: f64, max: f64) -> Param {
    Param { name, about, unit: Unit::Degrees, default, min, max, integer: false }
}
/// A ratio.
pub(crate) const fn ratio(name: &'static str, about: &'static str, default: f64, min: f64, max: f64) -> Param {
    Param { name, about, unit: Unit::Ratio, default, min, max, integer: false }
}

/// What an alias does to one of its form's parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Set {
    /// The parameter is fixed at this value (`hexagon`: `sides` is 6).
    Fixed(&'static str, f64),
    /// One new parameter gives all these parameters the same value (`cube`: `size` is each of
    /// `x`, `y`, `z`). Its range and default are the first target's.
    Tie {
        /// The new parameter's name.
        name: &'static str,
        /// What it is.
        about: &'static str,
        /// The form's parameters it sets.
        targets: &'static [&'static str],
    },
}

/// Another word for a form, perhaps with preset numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alias {
    /// The word (snake_case ASCII). A word names at most one thing of each kind.
    pub name: &'static str,
    /// The kind of form it names (the same word may name a plane figure and a solid).
    pub kind: Kind,
    /// The form it names.
    pub of: &'static str,
    /// What it is, in a few words.
    pub about: &'static str,
    /// What it presets.
    pub set: &'static [Set],
}

/// A form's own functions. Crate-internal: a new form gives these, and the library derives the
/// count, the build, the measures and the error from what `geom` returns.
#[derive(Clone, Copy)]
pub(crate) struct Imp {
    /// The exact geometry (lines, arcs and parametric pieces), or why the arguments do not
    /// make the figure. Cheap: no sampling.
    pub geom: fn(&Args) -> Result<Geom, Refusal>,
    /// Closed-form measures typed for this form (the textbook formula), where there is one.
    pub closed: fn(&Args) -> Measures,
    /// Named points for placing and joining.
    pub anchors: fn(&Args) -> Vec<Anchor>,
}

/// One form of the table.
#[derive(Clone, Copy)]
pub struct Form {
    /// Its name (snake_case ASCII, unique over every form).
    pub name: &'static str,
    /// What it builds to.
    pub kind: Kind,
    /// Its family.
    pub family: Family,
    /// One line of what it is.
    pub about: &'static str,
    /// Where its origin is.
    pub origin: &'static str,
    /// Its parameters.
    pub params: &'static [Param],
    /// Whether it has curved parts (so that `detail` changes it).
    pub curved: bool,
    /// Its relatives of the other kinds, by name (a circle's are the cylinder, the cone and the
    /// sphere).
    pub kin: &'static [&'static str],
    /// For a curve, the plane form that is the curve given a width, if there is one.
    pub band: Option<&'static str>,
    pub(crate) imp: Imp,
}

impl std::fmt::Debug for Form {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Form").field("name", &self.name).field("kind", &self.kind).finish()
    }
}

impl Form {
    /// The parameter of this name.
    pub fn param(&self, name: &str) -> Option<&'static Param> {
        self.params.iter().find(|p| p.name == name)
    }
}

/// The families' own lists, in catalogue order. A new family adds its list here; a new form in
/// a family is one entry in that family's file.
fn lists() -> [&'static [Form]; 3] {
    [crate::forms::polygons::FORMS, crate::forms::round::FORMS, crate::forms::curves::FORMS]
}
fn alias_lists() -> [&'static [Alias]; 3] {
    [crate::forms::polygons::ALIASES, crate::forms::round::ALIASES, crate::forms::curves::ALIASES]
}

/// Every form, in catalogue order.
pub fn forms() -> &'static [Form] {
    static ALL: OnceLock<Vec<Form>> = OnceLock::new();
    ALL.get_or_init(|| lists().iter().flat_map(|l| l.iter().copied()).collect())
}

/// Every alias.
pub fn aliases() -> &'static [Alias] {
    static ALL: OnceLock<Vec<Alias>> = OnceLock::new();
    ALL.get_or_init(|| alias_lists().iter().flat_map(|l| l.iter().copied()).collect())
}

/// The form of this exact name (no aliases).
pub fn form(name: &str) -> Option<&'static Form> {
    forms().iter().find(|f| f.name == name)
}

/// What a word found: a form, and the alias it was reached by, if any.
#[derive(Clone, Copy, Debug)]
pub struct Found {
    /// The form.
    pub form: &'static Form,
    /// The alias the word was, if it was one.
    pub alias: Option<&'static Alias>,
}

impl Found {
    /// The word it was found by: the alias's name, or the form's.
    pub fn name(&self) -> &'static str {
        self.alias.map(|a| a.name).unwrap_or(self.form.name)
    }
}

/// Finds a form by a name or an alias. A word that names forms of two kinds (`pill`: the
/// stadium and the capsule) needs `kind`; without it the answer is [`Error::Ambiguous`].
pub fn find(name: &str, kind: Option<Kind>) -> Result<Found, Error> {
    let mut hits: Vec<Found> = Vec::new();
    if let Some(f) = form(name) {
        hits.push(Found { form: f, alias: None });
    }
    for a in aliases().iter().filter(|a| a.name == name) {
        if let Some(f) = form(a.of) {
            hits.push(Found { form: f, alias: Some(a) });
        }
    }
    if let Some(k) = kind {
        hits.retain(|h| h.form.kind == k);
    }
    match hits.len() {
        0 => Err(Error::UnknownForm { name: name.to_string(), kind, nearest: nearest(name, kind) }),
        1 => Ok(hits[0]),
        _ => {
            let mut kinds: Vec<Kind> = hits.iter().map(|h| h.form.kind).collect();
            kinds.sort();
            kinds.dedup();
            Err(Error::Ambiguous { name: name.to_string(), kinds })
        }
    }
}

/// The words nearest to `name` (by edit distance), closest first, at most five.
pub fn nearest(name: &str, kind: Option<Kind>) -> Vec<&'static str> {
    let words = forms().iter().filter(|f| kind.is_none_or(|k| f.kind == k)).map(|f| f.name).chain(
        aliases().iter().filter(|a| kind.is_none_or(|k| a.kind == k)).map(|a| a.name),
    );
    let name: Vec<u8> = name.bytes().take(64).collect();
    let mut scored: Vec<(usize, &'static str)> = words
        .map(|w| (edit_distance(&name, w.as_bytes()), w))
        .filter(|(d, w)| *d <= (w.len() / 2).max(2) || w.as_bytes().windows(name.len().max(1)).any(|x| x == name.as_slice()))
        .collect();
    scored.sort();
    scored.dedup_by(|a, b| a.1 == b.1);
    scored.into_iter().take(5).map(|(_, w)| w).collect()
}

fn edit_distance(a: &[u8], b: &[u8]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(ca != cb)).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}
