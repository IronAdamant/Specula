//! Structured errors: what was wrong, where (a field path such as `strings."hud.wave".one`), and
//! what would be accepted, so an agent can fix a table from the data alone.

use specula_core::Json;
use std::fmt;

/// A rejected table, locale, rule or message.
#[derive(Clone, Debug, PartialEq)]
pub struct LinguaError {
    /// Machine-readable kind: `json`, `format`, `invalid_value`, `unknown_field`,
    /// `missing_field`, `bad_type`, `too_large`, `too_many`, `duplicate`, `bad_key`,
    /// `bad_locale`, `bad_placeholder`, `bad_rule`, `limit`.
    pub code: &'static str,
    /// Where: `locale`, `strings."hud.wave"`, `plural.one`.
    pub field: String,
    /// What would be accepted, when there is one.
    pub allowed: Option<String>,
    /// Byte offset into the text (JSON syntax, a placeholder or a rule).
    pub offset: Option<usize>,
    pub message: String,
}

impl LinguaError {
    pub fn new(code: &'static str, field: impl Into<String>, message: impl Into<String>) -> LinguaError {
        LinguaError { code, field: field.into(), allowed: None, offset: None, message: message.into() }
    }

    pub fn allowed(mut self, allowed: impl Into<String>) -> LinguaError {
        self.allowed = Some(allowed.into());
        self
    }

    pub fn at(mut self, offset: usize) -> LinguaError {
        self.offset = Some(offset);
        self
    }

    /// `{error, field, message, allowed?, offset?}`.
    pub fn to_json(&self) -> Json {
        let mut f = vec![("error", Json::from(self.code)), ("field", Json::from(self.field.as_str())), ("message", Json::from(self.message.as_str()))];
        if let Some(a) = &self.allowed {
            f.push(("allowed", Json::from(a.as_str())));
        }
        if let Some(o) = self.offset {
            f.push(("offset", Json::from(o)));
        }
        Json::obj(f)
    }
}

impl fmt::Display for LinguaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.field.is_empty() {
            write!(f, "{}", self.message)?;
        } else {
            write!(f, "{}: {}", self.field, self.message)?;
        }
        if let Some(a) = &self.allowed {
            write!(f, " (allowed: {a})")?;
        }
        Ok(())
    }
}

impl std::error::Error for LinguaError {}

/// `s` cut to at most `max` bytes on a character boundary, with `...` when cut (echoes of input
/// in errors stay bounded).
pub fn echo(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut cut = max.saturating_sub(3);
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}...", &s[..cut])
}

/// A JSON object key as it appears in a field path: bare when it is a plain identifier, quoted
/// otherwise (`strings."hud.wave"`).
pub fn key_path(parent: &str, key: &str) -> String {
    let plain = !key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    let k = if plain { key.to_owned() } else { format!("\"{}\"", echo(key, 96)) };
    if parent.is_empty() {
        k
    } else {
        format!("{parent}.{k}")
    }
}
