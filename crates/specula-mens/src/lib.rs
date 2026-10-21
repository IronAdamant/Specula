//! Specula AI as data (*mens*, "mind"): behaviour trees and utility AI written as JSON,
//! validated with diagnostics that say where and what to change, and run one fixed tick at a
//! time by a deterministic runner (docs/design/NAVIGATION.md, "AI as data").
//!
//! The runner knows no game. Conditions (`{"if": name, ...}`), actions (`{"do": name, ...}`)
//! and utility inputs (`{"input": name, ...}`) belong to a [`Host`], which declares them in a
//! [`Vocabulary`] so a definition is checked against what the host can actually do. The engine
//! is one host (navigation, sight, physics); Lex's `tree` / `utility` item kinds are another,
//! binding the same definitions to gameplay data without either crate depending on the other.
//!
//! **Determinism (F2):** no randomness, no clocks: time is the tick count the host passes in;
//! curves use `specula_core::dmath`; ties go to the earlier child or option.
//!
//! **Bounded (F1/F3):** definitions are capped (nodes, depth, options, bytes); one tick visits
//! each node at most once; traces and status reports are capped.

#![forbid(unsafe_code)]

pub mod def;
pub mod run;

pub use def::{Curve, Def, NodeKind};
pub use run::{Host, Mind, Phase, Status};

use specula_core::Json;

/// Most nodes in one definition.
pub const MAX_NODES: usize = 512;
/// Deepest nesting.
pub const MAX_DEPTH: usize = 32;
/// Most options of one utility node, and considerations of one option.
pub const MAX_OPTIONS: usize = 16;
pub const MAX_CONSIDERATIONS: usize = 8;
/// Most diagnostics one check reports.
pub const MAX_DIAGNOSTICS: usize = 32;

/// One problem with a definition: where (`path`, like `root.sequence[1].do`), what (`code`,
/// `message`) and how to fix it (`hint`).
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub path: String,
    pub code: &'static str,
    pub message: String,
    pub hint: Option<String>,
}

impl Diagnostic {
    pub fn to_json(&self) -> Json {
        let mut f = vec![("path", Json::from(self.path.as_str())), ("code", Json::from(self.code)), ("message", Json::from(self.message.as_str()))];
        if let Some(h) = &self.hint {
            f.push(("hint", Json::from(h.as_str())));
        }
        Json::obj(f)
    }
}

/// The type of a verb's argument.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ArgKind {
    Number { min: f64, max: f64 },
    Bool,
    Str,
    /// An entity (path, unique name or id), a point `[x, y, z]`, or a blackboard key `"$key"`.
    Target,
    /// A list of points, or an entity path whose children are the points.
    Points,
    /// Any JSON value.
    Any,
}

impl ArgKind {
    pub fn describe(self) -> String {
        match self {
            ArgKind::Number { min, max } => format!("a number {min}..{max}"),
            ArgKind::Bool => "true or false".into(),
            ArgKind::Str => "a string".into(),
            ArgKind::Target => "an entity (\"/path\", a unique name or an id), a point [x, y, z] or a blackboard key \"$key\"".into(),
            ArgKind::Points => "a list of points [[x, y, z], ...] or an entity path whose children are the points".into(),
            ArgKind::Any => "any value".into(),
        }
    }
}

/// A verb's argument.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arg {
    pub name: &'static str,
    pub kind: ArgKind,
    pub required: bool,
}

/// A condition, action or input the host provides.
#[derive(Clone, Debug, PartialEq)]
pub struct Verb {
    pub name: &'static str,
    pub doc: &'static str,
    pub args: Vec<Arg>,
}

/// What a host can do: the names a definition may use.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Vocabulary {
    pub conditions: Vec<Verb>,
    pub actions: Vec<Verb>,
    pub inputs: Vec<Verb>,
}

impl Vocabulary {
    pub fn to_json(&self) -> Json {
        let verbs = |list: &[Verb]| {
            Json::arr(list.iter().map(|v| {
                Json::obj([
                    ("name", Json::from(v.name)),
                    ("doc", Json::from(v.doc)),
                    (
                        "args",
                        Json::arr(v.args.iter().map(|a| {
                            Json::obj([("name", Json::from(a.name)), ("type", Json::from(a.kind.describe())), ("required", Json::from(a.required))])
                        })),
                    ),
                ])
            }))
        };
        Json::obj([("conditions", verbs(&self.conditions)), ("actions", verbs(&self.actions)), ("inputs", verbs(&self.inputs))])
    }
}

/// The valid name `key` was most likely meant to be (a prefix match or a near edit distance).
pub fn closest<'a>(key: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let distance = |a: &str, b: &str| -> usize {
        if a.starts_with(b) || b.starts_with(a) {
            return 1;
        }
        let b: Vec<char> = b.chars().collect();
        let mut row: Vec<usize> = (0..=b.len()).collect();
        for (i, ca) in a.chars().enumerate() {
            let mut prev = row[0];
            row[0] = i + 1;
            for (j, cb) in b.iter().enumerate() {
                let next = (row[j + 1] + 1).min(row[j] + 1).min(prev + usize::from(ca != *cb));
                prev = row[j + 1];
                row[j + 1] = next;
            }
        }
        row[b.len()]
    };
    let limit = (key.chars().count() / 3).max(2);
    candidates.map(|c| (distance(key, c), c)).filter(|(d, _)| *d <= limit).min().map(|(_, c)| c)
}
