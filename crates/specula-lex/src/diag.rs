//! Diagnostics: what the validator says, as data an agent can act on.
//!
//! Every diagnostic names the item and the field path, says what was expected and what was
//! found, and suggests the fix (the closest valid name, the literal to write, the valid range).
//! Codes are stable strings (see [`codes`]).

use std::fmt;

use specula_core::Json;

/// Stable diagnostic codes.
pub mod codes {
    pub const SYNTAX: &str = "syntax";
    pub const UNKNOWN_FIELD: &str = "unknown_field";
    pub const MISSING_FIELD: &str = "missing_field";
    pub const WRONG_TYPE: &str = "wrong_type";
    pub const UNKNOWN_REF: &str = "unknown_ref";
    pub const WRONG_KIND: &str = "wrong_kind";
    pub const UNIT_MISMATCH: &str = "unit_mismatch";
    pub const OUT_OF_RANGE: &str = "out_of_range";
    pub const DUPLICATE: &str = "duplicate";
    pub const CYCLE: &str = "cycle";
    pub const NOT_AVAILABLE: &str = "not_available";
    pub const INVALID: &str = "invalid";
    pub const INEXACT: &str = "inexact";
    pub const UNUSED: &str = "unused";
    pub const NATIVE_UNBOUND: &str = "native_unbound";
    pub const UNSUPPORTED_VERSION: &str = "unsupported_version";
    pub const TOO_MANY: &str = "too_many";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    /// The item id (`""` for the document header).
    pub item: String,
    /// The field path inside the item, e.g. `behaviours[1].range`.
    pub path: String,
    pub message: String,
    pub expected: Option<String>,
    pub got: Option<String>,
    pub suggestion: Option<String>,
    /// For errors inside an expression string: the 1-based character column.
    pub column: Option<usize>,
}

impl Diagnostic {
    pub fn error(code: &'static str, item: impl Into<String>, path: impl Into<String>, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            severity: Severity::Error,
            code,
            item: item.into(),
            path: path.into(),
            message: message.into(),
            expected: None,
            got: None,
            suggestion: None,
            column: None,
        }
    }

    pub fn warning(code: &'static str, item: impl Into<String>, path: impl Into<String>, message: impl Into<String>) -> Diagnostic {
        Diagnostic { severity: Severity::Warning, ..Diagnostic::error(code, item, path, message) }
    }

    pub fn expected(mut self, s: impl Into<String>) -> Self {
        self.expected = Some(s.into());
        self
    }

    pub fn got(mut self, s: impl Into<String>) -> Self {
        self.got = Some(s.into());
        self
    }

    pub fn suggest(mut self, s: impl Into<String>) -> Self {
        self.suggestion = Some(s.into());
        self
    }

    pub fn suggest_opt(mut self, s: Option<String>) -> Self {
        if s.is_some() {
            self.suggestion = s;
        }
        self
    }

    pub fn column(mut self, c: usize) -> Self {
        self.column = Some(c);
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    pub fn to_json(&self) -> Json {
        let mut pairs: Vec<(&str, Json)> = vec![
            ("severity", Json::from(self.severity.name())),
            ("code", Json::from(self.code)),
            ("item", Json::from(self.item.as_str())),
            ("path", Json::from(self.path.as_str())),
            ("message", Json::from(self.message.as_str())),
        ];
        if let Some(s) = &self.expected {
            pairs.push(("expected", Json::from(s.as_str())));
        }
        if let Some(s) = &self.got {
            pairs.push(("got", Json::from(s.as_str())));
        }
        if let Some(s) = &self.suggestion {
            pairs.push(("suggestion", Json::from(s.as_str())));
        }
        if let Some(c) = self.column {
            pairs.push(("column", Json::from(c)));
        }
        Json::obj(pairs)
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let at = match (self.item.is_empty(), self.path.is_empty()) {
            (true, true) => "document".to_owned(),
            (true, false) => self.path.clone(),
            (false, true) => self.item.clone(),
            (false, false) => format!("{}.{}", self.item, self.path),
        };
        write!(f, "{} [{}] {}: {}", self.severity.name(), self.code, at, self.message)?;
        if let Some(c) = self.column {
            write!(f, " (column {c})")?;
        }
        if let Some(s) = &self.suggestion {
            write!(f, "; {s}")?;
        }
        Ok(())
    }
}

/// The candidate `key` was most likely meant to be: a prefix match, or the nearest by edit
/// distance when near enough (at most a third of the longer name).
pub fn closest<'a>(key: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let mut best: Option<(usize, &'a str)> = None;
    for c in candidates {
        let d = if !key.is_empty() && (c.starts_with(key) || key.starts_with(c)) { 1 } else { edit_distance(key, c) };
        let limit = key.chars().count().max(c.chars().count()).div_ceil(3).max(1);
        if d <= limit && best.is_none_or(|(b, bc)| (d, c) < (b, bc)) {
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c)
}

/// "did you mean 'x'?" for the closest candidate, else a list of the valid names (at most 12).
pub fn did_you_mean<'a>(key: &str, candidates: impl IntoIterator<Item = &'a str> + Clone) -> Option<String> {
    if let Some(c) = closest(key, candidates.clone()) {
        return Some(format!("did you mean '{c}'?"));
    }
    let all: Vec<&str> = candidates.into_iter().collect();
    if all.is_empty() {
        return None;
    }
    let shown: Vec<&str> = all.iter().copied().take(12).collect();
    let more = if all.len() > shown.len() { format!(" and {} more", all.len() - shown.len()) } else { String::new() };
    Some(format!("valid: {}{more}", shown.join(", ")))
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    // Optimal string alignment: insertions, deletions, substitutions and adjacent swaps cost 1.
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = v;
        }
    }
    d[a.len()][b.len()]
}

/// A short, bounded rendering of a JSON value for `got` fields.
pub fn show(v: &Json) -> String {
    let s = v.to_string();
    if s.chars().count() > 80 {
        let cut: String = s.chars().take(77).collect();
        format!("{cut}...")
    } else {
        s
    }
}

/// The JSON type's name, for "expected a number, got a string".
pub fn type_name(v: &Json) -> &'static str {
    match v {
        Json::Null => "null",
        Json::Bool(_) => "a boolean",
        Json::Num(_) => "a number",
        Json::Str(_) => "a string",
        Json::Arr(_) => "an array",
        Json::Obj(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_names() {
        let names = ["range", "cooldown", "targets", "policy"];
        assert_eq!(closest("rnage", names), Some("range"));
        assert_eq!(closest("cool", names), Some("cooldown"));
        assert_eq!(closest("zzzzzz", names), None);
        assert_eq!(did_you_mean("polcy", names).as_deref(), Some("did you mean 'policy'?"));
        assert_eq!(did_you_mean("xyzzy", names).as_deref(), Some("valid: range, cooldown, targets, policy"));
    }

    #[test]
    fn diagnostics_render_as_data_and_text() {
        let d = Diagnostic::error(codes::UNIT_MISMATCH, "warding_1", "behaviours[1].range", "a distance is expected")
            .expected("distance")
            .got("7")
            .suggest("write \"7m\"");
        let j = d.to_json();
        assert_eq!(j.get("code").and_then(Json::as_str), Some("unit_mismatch"));
        assert_eq!(j.get("suggestion").and_then(Json::as_str), Some("write \"7m\""));
        assert_eq!(d.to_string(), "error [unit_mismatch] warding_1.behaviours[1].range: a distance is expected; write \"7m\"");
    }
}
