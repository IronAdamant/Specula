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
    pub reach: Option<Reach>,
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
        Error { code, path: path.into(), message: message.into(), near: Vec::new(), reach: None }
    }

    pub fn with_near(mut self, near: Vec<String>) -> Error {
        self.near = near;
        self
    }

    pub fn with_reach(mut self, number: &str, worst: f64, bound: [f64; 2]) -> Error {
        self.reach = Some(Reach { number: number.to_string(), worst, bound });
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
