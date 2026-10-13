//! Structured errors: a machine-readable code, the JSON path of the problem, and a message.
//!
//! Agents author screens and themes as JSON, so every error names *where* (`root.children[2].width`)
//! and *what* (`bad_value`), and unknown names come with the closest valid spelling.

use specula_core::json::Json;
use std::fmt;

/// What went wrong, as a stable machine-readable code ([`ErrorCode::as_str`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// The text is not valid JSON (`offset` is set).
    JsonSyntax,
    /// Missing or unsupported `"format"` tag.
    Format,
    MissingField,
    /// A field that the format does not have (typos are caught, not ignored).
    UnknownField,
    WrongType,
    /// Right type, invalid value (range, enum spelling, unit syntax).
    BadValue,
    /// Two widgets with the same `id` in one screen.
    DuplicateId,
    /// A colour, box style, button style or text role the theme doesn't define.
    UnknownName,
    /// A `{binding}` template that doesn't parse.
    BadTemplate,
    /// A `visible` / `enabled` condition that doesn't parse.
    BadCondition,
    /// Nesting or node count beyond the limits.
    TooLarge,
    /// A screen name not loaded in this [`crate::Ui`].
    UnknownScreen,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::JsonSyntax => "json_syntax",
            ErrorCode::Format => "format",
            ErrorCode::MissingField => "missing_field",
            ErrorCode::UnknownField => "unknown_field",
            ErrorCode::WrongType => "wrong_type",
            ErrorCode::BadValue => "bad_value",
            ErrorCode::DuplicateId => "duplicate_id",
            ErrorCode::UnknownName => "unknown_name",
            ErrorCode::BadTemplate => "bad_template",
            ErrorCode::BadCondition => "bad_condition",
            ErrorCode::TooLarge => "too_large",
            ErrorCode::UnknownScreen => "unknown_screen",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiError {
    pub code: ErrorCode,
    /// Where: a JSON path such as `root.children[2].width`, or empty.
    pub path: String,
    /// Byte offset into the JSON text (syntax errors only).
    pub offset: Option<usize>,
    pub message: String,
}

impl UiError {
    pub fn new(code: ErrorCode, path: impl Into<String>, message: impl Into<String>) -> UiError {
        UiError { code, path: path.into(), offset: None, message: message.into() }
    }

    /// Machine-readable form, for MCP tools and agent logs.
    pub fn to_json(&self) -> Json {
        let mut pairs = vec![
            ("code", Json::Str(self.code.as_str().into())),
            ("path", Json::Str(self.path.clone())),
            ("message", Json::Str(self.message.clone())),
        ];
        if let Some(o) = self.offset {
            pairs.push(("offset", Json::Num(o as f64)));
        }
        Json::obj(pairs)
    }
}

impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code.as_str())?;
        if !self.path.is_empty() {
            write!(f, " at {}", self.path)?;
        }
        if let Some(offset) = self.offset {
            write!(f, " (byte {offset})")?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for UiError {}

/// Parse JSON text into a [`Json`] value with a structured error.
pub(crate) fn parse_json(text: &str) -> Result<Json, UiError> {
    Json::parse(text).map_err(|e| UiError {
        code: ErrorCode::JsonSyntax,
        path: String::new(),
        offset: Some(e.offset),
        message: e.message,
    })
}

/// Levenshtein distance, for "did you mean" hints.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.chars().enumerate() {
        cur[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur[j + 1] = sub.min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// `" (did you mean \"x\"?)"` when a candidate is close, else empty.
pub(crate) fn suggest<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> String {
    let best = candidates
        .into_iter()
        .map(|c| (edit_distance(name, c), c))
        .filter(|&(d, c)| d <= 2.max(c.len() / 3))
        .min_by_key(|&(d, _)| d);
    match best {
        Some((_, c)) => format!(" (did you mean \"{c}\"?)"),
        None => String::new(),
    }
}

/// A strict view of one JSON object: every field read is recorded, and [`Obj::finish`] fails on
/// any field that was never read (so typos are errors, not silently ignored).
pub(crate) struct Obj<'a> {
    pairs: &'a [(String, Json)],
    pub path: String,
    seen: Vec<bool>,
}

impl<'a> Obj<'a> {
    pub fn new(json: &'a Json, path: &str) -> Result<Obj<'a>, UiError> {
        match json.as_object() {
            Some(pairs) => Ok(Obj { pairs, path: path.to_owned(), seen: vec![false; pairs.len()] }),
            None => Err(UiError::new(ErrorCode::WrongType, path, format!("expected an object, found {}", kind(json)))),
        }
    }

    pub fn field_path(&self, key: &str) -> String {
        if self.path.is_empty() {
            key.to_owned()
        } else {
            format!("{}.{key}", self.path)
        }
    }

    pub fn get(&mut self, key: &str) -> Option<&'a Json> {
        let i = self.pairs.iter().position(|(k, _)| k == key)?;
        self.seen[i] = true;
        Some(&self.pairs[i].1)
    }

    pub fn has(&self, key: &str) -> bool {
        self.pairs.iter().any(|(k, _)| k == key)
    }

    pub fn req(&mut self, key: &str) -> Result<&'a Json, UiError> {
        let path = self.field_path(key);
        self.get(key).ok_or_else(|| UiError::new(ErrorCode::MissingField, path, format!("\"{key}\" is required")))
    }

    pub fn str(&mut self, key: &str) -> Result<Option<&'a str>, UiError> {
        let path = self.field_path(key);
        match self.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(j) => j.as_str().map(Some).ok_or_else(|| wrong(&path, "a string", j)),
        }
    }

    pub fn req_str(&mut self, key: &str) -> Result<&'a str, UiError> {
        let path = self.field_path(key);
        let j = self.req(key)?;
        j.as_str().ok_or_else(|| wrong(&path, "a string", j))
    }

    pub fn num(&mut self, key: &str) -> Result<Option<f32>, UiError> {
        let path = self.field_path(key);
        match self.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(j) => match j.as_f64() {
                Some(v) if v.is_finite() && v.abs() < 1e7 => Ok(Some(v as f32)),
                Some(v) => Err(UiError::new(ErrorCode::BadValue, path, format!("{v} is out of range"))),
                None => Err(wrong(&path, "a number", j)),
            },
        }
    }

    pub fn bool(&mut self, key: &str) -> Result<Option<bool>, UiError> {
        let path = self.field_path(key);
        match self.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(j) => j.as_bool().map(Some).ok_or_else(|| wrong(&path, "true or false", j)),
        }
    }

    /// Fail on any field never read. `allowed` lists every field name the format has, for hints.
    pub fn finish(self, allowed: &[&str]) -> Result<(), UiError> {
        for (i, (k, _)) in self.pairs.iter().enumerate() {
            if !self.seen[i] {
                let hint = suggest(k, allowed.iter().copied());
                let known = if hint.is_empty() { format!("; known fields: {}", allowed.join(", ")) } else { hint };
                return Err(UiError::new(
                    ErrorCode::UnknownField,
                    self.field_path(k),
                    format!("unknown field \"{k}\"{known}"),
                ));
            }
        }
        Ok(())
    }
}

pub(crate) fn kind(j: &Json) -> &'static str {
    match j {
        Json::Null => "null",
        Json::Bool(_) => "a boolean",
        Json::Num(_) => "a number",
        Json::Str(_) => "a string",
        Json::Arr(_) => "an array",
        Json::Obj(_) => "an object",
    }
}

pub(crate) fn wrong(path: &str, expected: &str, found: &Json) -> UiError {
    UiError::new(ErrorCode::WrongType, path, format!("expected {expected}, found {}", kind(found)))
}

pub(crate) fn bad(path: &str, message: impl Into<String>) -> UiError {
    UiError::new(ErrorCode::BadValue, path, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestions_find_close_names() {
        assert_eq!(suggest("widht", ["width", "height"]), " (did you mean \"width\"?)");
        assert_eq!(suggest("zzzzzz", ["width", "height"]), "");
    }

    #[test]
    fn unknown_fields_are_errors_with_paths() {
        let j = Json::parse(r#"{"widht": 3, "height": 2}"#).unwrap();
        let mut o = Obj::new(&j, "root").unwrap();
        assert_eq!(o.num("height").unwrap(), Some(2.0));
        let e = o.finish(&["width", "height"]).unwrap_err();
        assert_eq!(e.code, ErrorCode::UnknownField);
        assert_eq!(e.path, "root.widht");
        assert!(e.message.contains("did you mean \"width\""), "{e}");
    }
}
