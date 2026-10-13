//! Structured recipe errors: a path into the recipe, a machine-readable code and a reason.

use std::fmt;

/// What went wrong, for agents to branch on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    /// The text isn't valid JSON (`path` is empty; the byte offset is in `offset`).
    Json,
    /// A required field is missing.
    MissingField,
    /// A field this object doesn't accept (typos are errors, not silently ignored).
    UnknownField,
    /// A field has the wrong JSON type or shape.
    WrongType,
    /// A number is outside its allowed range, or an expression evaluated to NaN/infinity.
    OutOfRange,
    /// An `op` name that doesn't exist.
    UnknownOp,
    /// A material preset the style doesn't define.
    UnknownMaterial,
    /// A colour that is neither `#rrggbb[aa]` nor a palette/material name.
    UnknownColor,
    /// An expression uses a name that isn't a parameter or an earlier `let`, or the caller
    /// passed a parameter the recipe doesn't declare.
    UnknownName,
    /// An `include` names an asset that isn't in the library.
    UnknownAsset,
    /// Includes form a cycle or nest too deeply.
    IncludeCycle,
    /// An expression doesn't parse (`offset` is the byte offset in the expression text).
    Expression,
    /// Geometry input is unusable (self-intersecting outline, too few points…).
    Geometry,
    /// The built mesh failed validation (NaN, degenerate triangles…).
    InvalidMesh,
    /// The build exceeded a safety limit (triangles, counts, nesting).
    Limit,
    /// The finished mesh breaks the recipe's triangle budget (`measured` vs `limit`).
    OverBudget,
    /// The finished mesh is below the recipe's minimum triangle count.
    UnderBudget,
    /// The style file is malformed.
    Style,
    /// A name used twice where names must be unique (rig bones, a part's bone list).
    Duplicate,
    /// The rig is unusable (bad parent order, a mirrored bone not named `_l`, non-finite pivots).
    InvalidRig,
}

/// A recipe or build error. `path` points into the recipe, e.g. `parts[2].ops[0].amount`.
#[derive(Clone, Debug, PartialEq)]
pub struct RecipeError {
    pub path: String,
    pub code: ErrorCode,
    pub message: String,
    /// Byte offset for [`ErrorCode::Json`] / [`ErrorCode::Expression`].
    pub offset: Option<usize>,
    /// Measured value for budget/limit/range errors.
    pub measured: Option<f64>,
    /// The bound that was broken.
    pub limit: Option<f64>,
}

impl RecipeError {
    pub fn new(path: impl Into<String>, code: ErrorCode, message: impl Into<String>) -> RecipeError {
        RecipeError { path: path.into(), code, message: message.into(), offset: None, measured: None, limit: None }
    }

    pub fn with_numbers(mut self, measured: f64, limit: f64) -> RecipeError {
        self.measured = Some(measured);
        self.limit = Some(limit);
        self
    }

    pub fn with_offset(mut self, offset: usize) -> RecipeError {
        self.offset = Some(offset);
        self
    }

    /// The error as JSON, for tool responses.
    pub fn to_json(&self) -> specula_core::Json {
        use specula_core::Json;
        let mut pairs = vec![
            ("code", Json::Str(format!("{:?}", self.code))),
            ("path", Json::Str(self.path.clone())),
            ("message", Json::Str(self.message.clone())),
        ];
        if let Some(o) = self.offset {
            pairs.push(("offset", Json::Num(o as f64)));
        }
        if let Some(m) = self.measured {
            pairs.push(("measured", Json::Num(m)));
        }
        if let Some(l) = self.limit {
            pairs.push(("limit", Json::Num(l)));
        }
        Json::obj(pairs)
    }
}

impl fmt::Display for RecipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{:?}: {}", self.code, self.message)
        } else {
            write!(f, "{:?} at {}: {}", self.code, self.path, self.message)
        }
    }
}

impl std::error::Error for RecipeError {}

/// Rejects objects that repeat a key anywhere in `j` (JSON allows it; a recipe that says two
/// different things must not silently keep one). The JSON parser bounds nesting depth.
pub(crate) fn check_duplicate_keys(j: &specula_core::Json, path: &str) -> Result<(), RecipeError> {
    use specula_core::Json;
    match j {
        Json::Obj(pairs) => {
            let mut seen = std::collections::BTreeSet::new();
            for (k, v) in pairs {
                let p = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
                if !seen.insert(k.as_str()) {
                    return Err(RecipeError::new(p, ErrorCode::WrongType, format!("duplicate key '{k}'")));
                }
                check_duplicate_keys(v, &p)?;
            }
            Ok(())
        }
        Json::Arr(items) => items.iter().enumerate().try_for_each(|(i, v)| check_duplicate_keys(v, &format!("{path}[{i}]"))),
        _ => Ok(()),
    }
}
