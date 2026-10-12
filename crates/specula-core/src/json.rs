//! JSON value, parser and serializer.
//!
//! Objects keep insertion order (`Vec` of pairs) so output is stable and readable — worlds and
//! protocol traffic diff cleanly. Numbers are `f64`; integers up to 2^53 round-trip exactly.

use std::fmt::{self, Write as _};

/// Nesting limit for parsing; protects the server from hostile input.
const MAX_DEPTH: usize = 128;

/// Largest integer an `f64` represents exactly (2^53).
const MAX_SAFE_INT: f64 = 9_007_199_254_740_992.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct JsonError {
    /// Byte offset into the input where parsing failed.
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JSON error at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for JsonError {}

impl Json {
    pub fn parse(text: &str) -> Result<Json, JsonError> {
        let mut p = Parser { s: text.as_bytes(), i: 0 };
        p.ws();
        let v = p.value(0)?;
        p.ws();
        if p.i != p.s.len() {
            return Err(p.err("trailing characters after JSON value"));
        }
        Ok(v)
    }

    /// Build an object from `(key, value)` pairs, keeping their order.
    pub fn obj<K: Into<String>>(pairs: impl IntoIterator<Item = (K, Json)>) -> Json {
        Json::Obj(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    pub fn arr(items: impl IntoIterator<Item = Json>) -> Json {
        Json::Arr(items.into_iter().collect())
    }

    /// Object member lookup (first match). `None` for missing keys and non-objects.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Insert or replace an object member. Returns `false` (and does nothing) on non-objects.
    pub fn set(&mut self, key: impl Into<String>, value: Json) -> bool {
        let Json::Obj(pairs) = self else { return false };
        let key = key.into();
        match pairs.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => pairs.push((key, value)),
        }
        true
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }

    /// Integral numbers within ±2^53 only.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Num(n) if n.fract() == 0.0 && n.abs() <= MAX_SAFE_INT => Some(*n as i64),
            _ => None,
        }
    }

    /// Non-negative integral numbers up to 2^53 only.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Json::Num(n) if n.fract() == 0.0 && *n >= 0.0 && *n <= MAX_SAFE_INT => Some(*n as u64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&[(String, Json)]> {
        match self {
            Json::Obj(pairs) => Some(pairs),
            _ => None,
        }
    }

    /// Two-space indented output. `Display` gives the compact form.
    pub fn to_pretty(&self) -> String {
        let mut out = String::new();
        write_value(&mut out, self, Some(0));
        out
    }
}

impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        write_value(&mut out, self, None);
        f.write_str(&out)
    }
}

impl From<bool> for Json {
    fn from(v: bool) -> Self {
        Json::Bool(v)
    }
}

impl From<f64> for Json {
    fn from(v: f64) -> Self {
        Json::Num(v)
    }
}

impl From<f32> for Json {
    fn from(v: f32) -> Self {
        Json::Num(v as f64)
    }
}

impl From<i64> for Json {
    fn from(v: i64) -> Self {
        Json::Num(v as f64)
    }
}

impl From<i32> for Json {
    fn from(v: i32) -> Self {
        Json::Num(v as f64)
    }
}

impl From<u64> for Json {
    fn from(v: u64) -> Self {
        Json::Num(v as f64)
    }
}

impl From<u32> for Json {
    fn from(v: u32) -> Self {
        Json::Num(v as f64)
    }
}

impl From<usize> for Json {
    fn from(v: usize) -> Self {
        Json::Num(v as f64)
    }
}

impl From<&str> for Json {
    fn from(v: &str) -> Self {
        Json::Str(v.to_owned())
    }
}

impl From<String> for Json {
    fn from(v: String) -> Self {
        Json::Str(v)
    }
}

impl From<Vec<Json>> for Json {
    fn from(v: Vec<Json>) -> Self {
        Json::Arr(v)
    }
}

impl<T: Into<Json>> From<Option<T>> for Json {
    fn from(v: Option<T>) -> Self {
        v.map_or(Json::Null, Into::into)
    }
}

// ── serializer ───────────────────────────────────────────────────────────────

/// `indent` is `Some(level)` for pretty output, `None` for compact.
fn write_value(out: &mut String, v: &Json, indent: Option<usize>) {
    match v {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Num(n) => write_number(out, *n),
        Json::Str(s) => write_string(out, s),
        Json::Arr(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, indent.map(|l| l + 1));
                write_value(out, item, indent.map(|l| l + 1));
            }
            newline(out, indent);
            out.push(']');
        }
        Json::Obj(pairs) => {
            if pairs.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (k, item)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, indent.map(|l| l + 1));
                write_string(out, k);
                out.push(':');
                if indent.is_some() {
                    out.push(' ');
                }
                write_value(out, item, indent.map(|l| l + 1));
            }
            newline(out, indent);
            out.push('}');
        }
    }
}

fn newline(out: &mut String, indent: Option<usize>) {
    if let Some(level) = indent {
        out.push('\n');
        for _ in 0..level {
            out.push_str("  ");
        }
    }
}

fn write_number(out: &mut String, n: f64) {
    if !n.is_finite() {
        // JSON has no NaN/Infinity.
        out.push_str("null");
    } else if n.fract() == 0.0 && n.abs() < 1e15 {
        let _ = write!(out, "{}", n as i64);
    } else {
        // Rust prints the shortest representation that round-trips.
        let _ = write!(out, "{n}");
    }
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

// ── parser ───────────────────────────────────────────────────────────────────

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn err(&self, message: &str) -> JsonError {
        JsonError { offset: self.i, message: message.to_owned() }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn ws(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.peek() {
            self.i += 1;
        }
    }

    fn expect_lit(&mut self, lit: &str, v: Json) -> Result<Json, JsonError> {
        if self.s[self.i..].starts_with(lit.as_bytes()) {
            self.i += lit.len();
            Ok(v)
        } else {
            Err(self.err("invalid literal"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, JsonError> {
        if depth > MAX_DEPTH {
            return Err(self.err("nesting too deep"));
        }
        match self.peek() {
            None => Err(self.err("unexpected end of input")),
            Some(b'n') => self.expect_lit("null", Json::Null),
            Some(b't') => self.expect_lit("true", Json::Bool(true)),
            Some(b'f') => self.expect_lit("false", Json::Bool(false)),
            Some(b'"') => self.string().map(Json::Str),
            Some(b'[') => self.array(depth),
            Some(b'{') => self.object(depth),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.err("unexpected character")),
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.i += 1; // [
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            self.ws();
            items.push(self.value(depth + 1)?);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(self.err("expected ',' or ']' in array")),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.i += 1; // {
        let mut pairs = Vec::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Json::Obj(pairs));
        }
        loop {
            self.ws();
            if self.peek() != Some(b'"') {
                return Err(self.err("expected string key in object"));
            }
            let key = self.string()?;
            self.ws();
            if self.peek() != Some(b':') {
                return Err(self.err("expected ':' after object key"));
            }
            self.i += 1;
            self.ws();
            let v = self.value(depth + 1)?;
            pairs.push((key, v));
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Json::Obj(pairs));
                }
                _ => return Err(self.err("expected ',' or '}' in object")),
            }
        }
    }

    fn number(&mut self) -> Result<Json, JsonError> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        match self.peek() {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(self.err("invalid number")),
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.err("expected digit after '.'"));
            }
            self.digits();
        }
        if let Some(b'e' | b'E') = self.peek() {
            self.i += 1;
            if let Some(b'+' | b'-') = self.peek() {
                self.i += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.err("expected digit in exponent"));
            }
            self.digits();
        }
        // The grammar above only admits ASCII, so this slice is valid UTF-8.
        let text = std::str::from_utf8(&self.s[start..self.i]).expect("ascii number");
        text.parse::<f64>()
            .map(Json::Num)
            .map_err(|_| JsonError { offset: start, message: "invalid number".into() })
    }

    fn digits(&mut self) {
        while let Some(b'0'..=b'9') = self.peek() {
            self.i += 1;
        }
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.i += 1; // opening quote
        let mut out = String::new();
        loop {
            let run_start = self.i;
            while let Some(b) = self.peek() {
                if b == b'"' || b == b'\\' || b < 0x20 {
                    break;
                }
                self.i += 1;
            }
            // Input came from &str and we only stop on ASCII bytes, so the run is valid UTF-8.
            out.push_str(std::str::from_utf8(&self.s[run_start..self.i]).expect("utf-8 run"));
            match self.peek() {
                None => return Err(self.err("unterminated string")),
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    let esc = self.peek().ok_or_else(|| self.err("unterminated escape"))?;
                    self.i += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{08}'),
                        b'f' => out.push('\u{0c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.unicode_escape()?),
                        _ => return Err(self.err("invalid escape")),
                    }
                }
                Some(_) => return Err(self.err("control character in string")),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let hex = self.s.get(self.i..self.i + 4).ok_or_else(|| self.err("short \\u escape"))?;
        let text = std::str::from_utf8(hex).map_err(|_| self.err("invalid \\u escape"))?;
        let v = u32::from_str_radix(text, 16).map_err(|_| self.err("invalid \\u escape"))?;
        self.i += 4;
        Ok(v)
    }

    fn unicode_escape(&mut self) -> Result<char, JsonError> {
        let hi = self.hex4()?;
        let code = if (0xD800..0xDC00).contains(&hi) {
            // High surrogate: must be followed by \uDC00..\uDFFF.
            if !self.s[self.i..].starts_with(b"\\u") {
                return Err(self.err("unpaired surrogate"));
            }
            self.i += 2;
            let lo = self.hex4()?;
            if !(0xDC00..0xE000).contains(&lo) {
                return Err(self.err("invalid low surrogate"));
            }
            0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
        } else if (0xDC00..0xE000).contains(&hi) {
            return Err(self.err("unpaired surrogate"));
        } else {
            hi
        };
        char::from_u32(code).ok_or_else(|| self.err("invalid code point"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_values_in_order() {
        let v = Json::parse(r#" {"b": [1, 2.5, -3e2, true, null], "a": {"x": "y"}} "#).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["b", "a"]);
        let b = v.get("b").unwrap().as_array().unwrap();
        assert_eq!(b[0].as_u64(), Some(1));
        assert_eq!(b[1].as_f64(), Some(2.5));
        assert_eq!(b[2].as_i64(), Some(-300));
        assert_eq!(b[3].as_bool(), Some(true));
        assert!(b[4].is_null());
        assert_eq!(v.get("a").unwrap().get("x").unwrap().as_str(), Some("y"));
    }

    #[test]
    fn compact_round_trip() {
        let text = r#"{"id":7,"name":"cube \"A\"","pos":[0,1.5,-2],"tags":[],"meta":{}}"#;
        assert_eq!(Json::parse(text).unwrap().to_string(), text);
    }

    #[test]
    fn escapes_and_surrogates() {
        let v = Json::parse(r#""a\n\té😀\/""#).unwrap();
        assert_eq!(v.as_str(), Some("a\n\té😀/"));
        assert_eq!(Json::from("\u{01}x\"").to_string(), r#""\u0001x\"""#);
        assert!(Json::parse(r#""\ud83d""#).is_err());
    }

    #[test]
    fn numbers_format_and_validate() {
        assert_eq!(Json::Num(3.0).to_string(), "3");
        assert_eq!(Json::Num(0.1).to_string(), "0.1");
        assert_eq!(Json::Num(f64::NAN).to_string(), "null");
        assert_eq!(Json::Num(-1.5).as_u64(), None);
        for bad in ["01", "1.", "-", "1e", ".5", "+1"] {
            assert!(Json::parse(bad).is_err(), "{bad} should fail");
        }
    }

    #[test]
    fn errors_report_offsets() {
        let e = Json::parse(r#"{"a" 1}"#).unwrap_err();
        assert_eq!(e.offset, 5);
        assert!(Json::parse("[1,2] x").is_err());
        assert!(Json::parse(&"[".repeat(MAX_DEPTH + 2)).is_err());
    }

    #[test]
    fn builders_set_and_pretty() {
        let mut v = Json::obj([("a", Json::from(1)), ("b", Json::arr([Json::from("x")]))]);
        assert!(v.set("a", Json::from(2)));
        assert!(v.set("c", Json::Null));
        assert_eq!(v.to_string(), r#"{"a":2,"b":["x"],"c":null}"#);
        assert_eq!(v.to_pretty(), "{\n  \"a\": 2,\n  \"b\": [\n    \"x\"\n  ],\n  \"c\": null\n}");
        assert!(!Json::Null.clone().set("k", Json::Null));
    }
}
