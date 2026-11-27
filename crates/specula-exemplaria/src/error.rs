//! Structured errors: a machine-readable code, a path into the kind file, a reason, and the
//! nearest names when a name was not found (AGENTS rule 5).

use specula_core::Json;
use std::fmt;

/// What went wrong with a kind, a plan's numbers or an expansion.
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    /// `json`, `too_large`, `control_character`, `unknown_field`, `missing_field`, `wrong_type`,
    /// `out_of_range`, `bad_name`, `no_source`, `two_sources`, `unknown_plan`, `unknown_name`,
    /// `too_many`, `base_not_found`, `base_too_deep`, `base_cycle`, `base_other_plan`,
    /// `role_unresolved`, `unknown_kind`, `not_built`, `outside_plan` (a draw of the kind could
    /// leave what its plan can write: [`Error::reach`] says how far).
    pub code: &'static str,
    /// Where in the kind file (`numbers.leg_length.share`); empty for the whole file.
    pub path: String,
    pub message: String,
    /// The nearest names, when a name was not found.
    pub near: Vec<String>,
    /// For `outside_plan`: the number, the worst value a draw could reach, and the bounds it
    /// leaves (in the number's unit).
    pub reach: Option<Box<Reach>>,
    /// For `outside_plan` by a relation between numbers: which, and the worst values.
    pub relation: Option<Box<Broken>>,
}

/// A relation of a plan's numbers ([`crate::plan::RelationDef`]) a kind's seeds could break.
#[derive(Clone, Debug, PartialEq)]
pub struct Broken {
    pub relation: String,
    /// The body at which the reach found the relation least, as a seed draws it: the size (on its
    /// step), then each trait the relation's numbers follow (its residual, a fraction), then each
    /// of its numbers with a spread of its own (`<number>.own`, in spreads, at the end of its
    /// window there).
    pub at: Vec<(String, f64)>,
    /// Each of its numbers at that body (before rounding), in the plan's order of its terms.
    pub values: Vec<(String, f64)>,
    /// The relation's sum at that body (after the allowance for rounding), and the least it may be.
    pub sum: f64,
    /// The check's bound on the least over every seed (after the allowance for rounding and for
    /// floating point): under `margin`, which is why the kind is refused. Never above `sum`, and
    /// within a millionth of a millimetre of it for a body of metres, except where a measured
    /// range narrows a residual (the one place the check is a bound).
    pub bound: f64,
    pub margin: f64,
}

/// How far a kind's draws would go past what its plan allows.
#[derive(Clone, Debug, PartialEq)]
pub struct Reach {
    /// The drawn number (`leg_length`, or the size's name).
    pub number: String,
    /// The worst value a seed could give it.
    pub worst: f64,
    /// What the plan allows for it, low and high.
    pub bound: [f64; 2],
}

impl Error {
    pub fn new(code: &'static str, path: impl Into<String>, message: impl Into<String>) -> Error {
        Error { code, path: path.into(), message: message.into(), near: Vec::new(), reach: None, relation: None }
    }

    pub fn with_relation(mut self, broken: Broken) -> Error {
        self.relation = Some(Box::new(broken));
        self
    }

    pub fn with_near(mut self, near: Vec<String>) -> Error {
        self.near = near;
        self
    }

    pub fn with_reach(mut self, number: &str, worst: f64, bound: [f64; 2]) -> Error {
        self.reach = Some(Box::new(Reach { number: number.to_string(), worst, bound }));
        self
    }

    pub fn to_json(&self) -> Json {
        let mut o = vec![("code", Json::from(self.code)), ("path", Json::from(self.path.as_str())), ("message", Json::from(self.message.as_str()))];
        if !self.near.is_empty() {
            o.push(("nearest", Json::arr(self.near.iter().map(|n| Json::from(n.as_str())))));
        }
        if let Some(r) = &self.reach {
            o.push(("number", Json::from(r.number.as_str())));
            o.push(("worst", Json::from(r.worst)));
            o.push(("bound", Json::arr(r.bound.map(Json::from))));
        }
        if let Some(b) = &self.relation {
            o.push(("relation", Json::from(b.relation.as_str())));
            o.push(("at", Json::obj(b.at.iter().map(|(n, v)| (n.as_str(), Json::from(*v))))));
            o.push(("values", Json::obj(b.values.iter().map(|(n, v)| (n.as_str(), Json::from(*v))))));
            o.push(("worst", Json::from(b.sum)));
            o.push(("worst_bound", Json::from(b.bound)));
            o.push(("least", Json::from(b.margin)));
        }
        Json::obj(o)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{} [{}]", self.message, self.code)?;
        } else {
            write!(f, "{}: {} [{}]", self.path, self.message, self.code)?;
        }
        if !self.near.is_empty() {
            write!(f, " (nearest: {})", self.near.join(", "))?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}
