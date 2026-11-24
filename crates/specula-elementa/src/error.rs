//! Structured errors: a code an agent can branch on, a sentence, and the numbers (AGENTS rule 5).

use specula_core::bytes::DecodeError;
use specula_core::json::Json;

/// Why Elementa refused something: data, a call, a limit or a save.
///
/// Codes: `bad_json`, `too_large`, `unknown_key`, `wrong_type`, `out_of_range`, `bad_name`,
/// `unknown_material`, `duplicate`, `unsupported_version`, `bad_config`, `limit`, `unknown_node`,
/// `bad_snapshot`, `snapshot_invalid`.
#[derive(Clone, Debug, PartialEq)]
pub struct ElementaError {
    pub code: &'static str,
    pub message: String,
    pub details: Vec<(&'static str, Json)>,
}

impl ElementaError {
    pub fn new(code: &'static str, message: impl Into<String>) -> ElementaError {
        ElementaError { code, message: message.into(), details: Vec::new() }
    }

    pub fn with(mut self, key: &'static str, value: impl Into<Json>) -> ElementaError {
        self.details.push((key, value.into()));
        self
    }

    /// The value of a detail, for tests and callers that branch on one.
    pub fn detail(&self, key: &str) -> Option<&Json> {
        self.details.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    pub fn to_json(&self) -> Json {
        let mut fields = vec![("error", Json::from(self.code)), ("message", Json::from(self.message.as_str()))];
        fields.extend(self.details.iter().map(|(k, v)| (*k, v.clone())));
        Json::obj(fields)
    }

    /// A value outside its range, with the path to it and the range.
    pub fn range(path: &str, value: f64, min: f64, max: f64) -> ElementaError {
        ElementaError::new("out_of_range", format!("{path} is {value}; it must be from {min} to {max}"))
            .with("path", path)
            .with("value", value)
            .with("min", min)
            .with("max", max)
    }

    /// A limit that would be passed: which, its number, and what was asked.
    pub fn limit(which: &'static str, max: u64, asked: u64, message: impl Into<String>) -> ElementaError {
        ElementaError::new("limit", message).with("limit", which).with("max", max).with("asked", asked)
    }
}

impl std::fmt::Display for ElementaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ElementaError {}

impl From<DecodeError> for ElementaError {
    fn from(e: DecodeError) -> ElementaError {
        ElementaError::new("bad_snapshot", format!("the elementa section is damaged: {e}"))
            .with("decode", e.code)
            .with("offset", e.offset)
            .with("field", e.field.as_str())
    }
}
