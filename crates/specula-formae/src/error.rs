//! Refusals as data: each carries a code and the numbers that decided it.

use crate::table::Kind;
use std::fmt;

/// Why a name, an argument or a build was refused. [`Error::code`] is the machine-readable
/// code; every variant carries the values that decided it.
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// No form or alias has this name (for the asked kind, if one was asked).
    UnknownForm {
        /// The name asked for.
        name: String,
        /// The kind asked for, if any.
        kind: Option<Kind>,
        /// The nearest names there are, closest first (at most five).
        nearest: Vec<&'static str>,
    },
    /// The word names forms of more than one kind: say which.
    Ambiguous {
        /// The word.
        name: String,
        /// The kinds it names.
        kinds: Vec<Kind>,
    },
    /// The form takes no parameter of this name.
    UnknownParam {
        /// The form (or alias) asked for.
        form: &'static str,
        /// The parameter given.
        param: String,
        /// The parameters it does take.
        takes: Vec<&'static str>,
    },
    /// The parameter was given twice.
    DuplicateParam {
        /// The form (or alias).
        form: &'static str,
        /// The parameter.
        param: &'static str,
    },
    /// The alias fixes this parameter, so it cannot be given.
    FixedByAlias {
        /// The alias.
        alias: &'static str,
        /// The parameter.
        param: &'static str,
        /// The value the alias gives it.
        value: f64,
    },
    /// A parameter is outside its range, or not a finite number.
    OutOfRange {
        /// The form (or alias).
        form: &'static str,
        /// The parameter.
        param: &'static str,
        /// The value given.
        value: f64,
        /// Its least value.
        min: f64,
        /// Its greatest value.
        max: f64,
    },
    /// A whole-number parameter was given a fraction.
    NotInteger {
        /// The form (or alias).
        form: &'static str,
        /// The parameter.
        param: &'static str,
        /// The value given.
        value: f64,
    },
    /// The parameters are each in range but together do not make the figure (a triangle whose
    /// sides break the triangle inequality, an inner radius past the outer one...).
    Constraint {
        /// The form.
        form: &'static str,
        /// A short snake_case name of the rule broken.
        rule: &'static str,
        /// What the rule says, in a sentence.
        message: String,
        /// The values the rule read.
        values: Vec<(&'static str, f64)>,
    },
    /// The detail is outside its range.
    DetailOutOfRange {
        /// The detail given.
        detail: u32,
        /// Its least value.
        min: u32,
        /// Its greatest value.
        max: u32,
    },
    /// The build would have more points than the ceiling: refused before any work.
    TooManyPoints {
        /// The form.
        form: &'static str,
        /// The points the build would have.
        points: u64,
        /// The ceiling.
        ceiling: u64,
    },
}

impl Error {
    /// The machine-readable code.
    pub fn code(&self) -> &'static str {
        match self {
            Error::UnknownForm { .. } => "unknown_form",
            Error::Ambiguous { .. } => "ambiguous_name",
            Error::UnknownParam { .. } => "unknown_param",
            Error::DuplicateParam { .. } => "duplicate_param",
            Error::FixedByAlias { .. } => "fixed_by_alias",
            Error::OutOfRange { .. } => "out_of_range",
            Error::NotInteger { .. } => "not_integer",
            Error::Constraint { .. } => "constraint",
            Error::DetailOutOfRange { .. } => "detail_out_of_range",
            Error::TooManyPoints { .. } => "too_many_points",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnknownForm { name, kind, nearest } => {
                let k = kind.map(|k| format!(" {}", k.name())).unwrap_or_default();
                write!(f, "no{k} form is called \"{name}\"; the nearest are {nearest:?}")
            }
            Error::Ambiguous { name, kinds } => {
                let ks: Vec<&str> = kinds.iter().map(|k| k.name()).collect();
                write!(f, "\"{name}\" names a form of each kind {ks:?}: say which kind")
            }
            Error::UnknownParam { form, param, takes } => write!(f, "{form} takes no \"{param}\"; it takes {takes:?}"),
            Error::DuplicateParam { form, param } => write!(f, "{form}: \"{param}\" is given twice"),
            Error::FixedByAlias { alias, param, value } => write!(f, "{alias} fixes \"{param}\" at {value}: it cannot be given"),
            Error::OutOfRange { form, param, value, min, max } => write!(f, "{form}: \"{param}\" is {value}; it must be a number from {min} to {max}"),
            Error::NotInteger { form, param, value } => write!(f, "{form}: \"{param}\" is {value}; it must be a whole number"),
            Error::Constraint { form, rule, message, values } => write!(f, "{form} ({rule}): {message}; {values:?}"),
            Error::DetailOutOfRange { detail, min, max } => write!(f, "detail {detail} is out of range: from {min} to {max}"),
            Error::TooManyPoints { form, points, ceiling } => write!(f, "{form} would have {points} points; the ceiling is {ceiling}"),
        }
    }
}

impl std::error::Error for Error {}

/// A form's own refusal of its arguments, before the library adds the form's name.
#[derive(Clone, Debug)]
pub(crate) struct Refusal {
    pub rule: &'static str,
    pub message: String,
    pub values: Vec<(&'static str, f64)>,
}

/// A refusal: `rule`, what it says, and the values it read.
pub(crate) fn refuse(rule: &'static str, message: &str, values: &[(&'static str, f64)]) -> Refusal {
    Refusal { rule, message: message.to_string(), values: values.to_vec() }
}
