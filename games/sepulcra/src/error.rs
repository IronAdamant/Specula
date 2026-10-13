//! Structured content errors: agents edit the JSON files, so every error names the file, the
//! field path and the reason in machine-readable fields.

use std::fmt;

use specula_core::Json;

/// What kind of problem a [`DataError`] is, for agents to branch on (the `reason` is prose).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// The file isn't valid JSON (`field` is `byte N`).
    Syntax,
    /// The file couldn't be read.
    Io,
    Missing,
    /// Wrong JSON type (a string where a number belongs, ...).
    BadType,
    /// A number or count outside its allowed range.
    OutOfRange,
    /// A key the format doesn't know (usually a typo).
    UnknownField,
    /// A string that isn't one of the allowed choices (`"policy": "fastest"`).
    BadChoice,
    /// A name that refers to nothing (an enemy id, a spawn, a cell, a stone).
    UnknownRef,
    Duplicate,
    /// A map grid with an unknown character or the wrong shape.
    BadGrid,
    /// A spawn that can't reach the graveyard.
    Unreachable,
    /// A replay recorded against different content.
    ContentMismatch,
    /// Well-formed but against a game rule (a lob without splash, a walker from a cliff...).
    Invalid,
}

impl ErrorCode {
    /// A stable snake_case name.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Syntax => "syntax",
            ErrorCode::Io => "io",
            ErrorCode::Missing => "missing",
            ErrorCode::BadType => "bad_type",
            ErrorCode::OutOfRange => "out_of_range",
            ErrorCode::UnknownField => "unknown_field",
            ErrorCode::BadChoice => "bad_choice",
            ErrorCode::UnknownRef => "unknown_ref",
            ErrorCode::Duplicate => "duplicate",
            ErrorCode::BadGrid => "bad_grid",
            ErrorCode::Unreachable => "unreachable",
            ErrorCode::ContentMismatch => "content_mismatch",
            ErrorCode::Invalid => "invalid",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataError {
    /// The content file, e.g. `stones.json`.
    pub file: String,
    /// Path to the offending value, e.g. `stones[2].tiers[0].range_m` or `top[3][7]`.
    pub field: String,
    pub code: ErrorCode,
    /// For humans; match on `code`.
    pub reason: String,
}

impl DataError {
    pub fn new(file: &str, field: impl Into<String>, code: ErrorCode, reason: impl Into<String>) -> DataError {
        DataError { file: file.to_owned(), field: field.into(), code, reason: reason.into() }
    }
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}: {} ({})", self.file, self.field, self.reason, self.code.as_str())
    }
}

impl std::error::Error for DataError {}

/// A JSON object being read, with its file and path for error messages.
#[derive(Clone)]
pub(crate) struct Obj<'a> {
    pub json: &'a Json,
    pub file: &'a str,
    path: String,
}

/// `a.b` / `a[3]`.
pub(crate) fn join(path: &str, key: &str) -> String {
    if path.is_empty() || key.is_empty() || key.starts_with('[') {
        format!("{path}{key}")
    } else {
        format!("{path}.{key}")
    }
}

impl<'a> Obj<'a> {
    pub fn new(json: &'a Json, file: &'a str, path: impl Into<String>) -> Result<Obj<'a>, DataError> {
        let path = path.into();
        if json.as_object().is_none() {
            let shown = if path.is_empty() { "(root)".to_owned() } else { path };
            return Err(DataError::new(file, shown, ErrorCode::BadType, "expected a JSON object"));
        }
        Ok(Obj { json, file, path })
    }

    /// The object member `key` as a nested reader.
    pub fn obj(&self, key: &str) -> Result<Obj<'a>, DataError> {
        Obj::new(self.req(key)?, self.file, self.path(key))
    }

    pub fn err(&self, key: &str, code: ErrorCode, reason: impl Into<String>) -> DataError {
        DataError::new(self.file, join(&self.path, key), code, reason)
    }

    pub fn path(&self, key: &str) -> String {
        join(&self.path, key)
    }

    /// Rejects keys outside `allowed`, so a typo (`"rnage_m"`) fails loudly instead of silently
    /// falling back to a default.
    pub fn only(&self, allowed: &[&str]) -> Result<(), DataError> {
        for (k, _) in self.json.as_object().unwrap_or(&[]) {
            if !allowed.contains(&k.as_str()) {
                return Err(self.err(k, ErrorCode::UnknownField, format!("unknown field (expected one of: {})", allowed.join(", "))));
            }
        }
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&'a Json> {
        self.json.get(key)
    }

    pub fn req(&self, key: &str) -> Result<&'a Json, DataError> {
        self.get(key).ok_or_else(|| self.err(key, ErrorCode::Missing, "missing"))
    }

    pub fn num(&self, key: &str, min: f64, max: f64) -> Result<f64, DataError> {
        let v = self.req(key)?.as_f64().ok_or_else(|| self.err(key, ErrorCode::BadType, "expected a number"))?;
        if !(min..=max).contains(&v) {
            return Err(self.err(key, ErrorCode::OutOfRange, format!("{v} is outside {min}..={max}")));
        }
        Ok(v)
    }

    pub fn num_or(&self, key: &str, default: f64, min: f64, max: f64) -> Result<f64, DataError> {
        if self.get(key).is_none() {
            return Ok(default);
        }
        self.num(key, min, max)
    }

    pub fn int(&self, key: &str, min: i64, max: i64) -> Result<i64, DataError> {
        let v = self.req(key)?.as_i64().ok_or_else(|| self.err(key, ErrorCode::BadType, "expected an integer"))?;
        if !(min..=max).contains(&v) {
            return Err(self.err(key, ErrorCode::OutOfRange, format!("{v} is outside {min}..={max}")));
        }
        Ok(v)
    }

    pub fn int_or(&self, key: &str, default: i64, min: i64, max: i64) -> Result<i64, DataError> {
        if self.get(key).is_none() {
            return Ok(default);
        }
        self.int(key, min, max)
    }

    pub fn str(&self, key: &str) -> Result<&'a str, DataError> {
        self.req(key)?.as_str().ok_or_else(|| self.err(key, ErrorCode::BadType, "expected a string"))
    }

    pub fn arr(&self, key: &str) -> Result<&'a [Json], DataError> {
        self.req(key)?.as_array().ok_or_else(|| self.err(key, ErrorCode::BadType, "expected an array"))
    }
}
