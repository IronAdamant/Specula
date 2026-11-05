//! Structured errors: what was wrong, where (a field path such as `tracks[2].keys[1].t`), and
//! what would be accepted, so an agent can fix a timeline from the data alone.

use specula_core::Json;
use std::fmt;

/// A rejected timeline field.
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineError {
    /// Machine-readable kind: `invalid_value`, `unknown_field`, `missing_field`, `too_many`,
    /// `bad_type`, `out_of_order`, `duplicate`, `too_large`, `json`.
    pub code: &'static str,
    /// Where: a dotted path with indices (`tracks[0].move[3].at`).
    pub field: String,
    /// What would be accepted (a range, a list of names, a shape), when there is one.
    pub allowed: Option<String>,
    /// The value given (compact JSON, cut to 120 bytes), when there was one.
    pub value: Option<String>,
    pub message: String,
}

impl TimelineError {
    pub fn new(code: &'static str, field: impl Into<String>, message: impl Into<String>) -> TimelineError {
        TimelineError { code, field: field.into(), allowed: None, value: None, message: message.into() }
    }

    pub fn allowed(mut self, allowed: impl Into<String>) -> TimelineError {
        self.allowed = Some(allowed.into());
        self
    }

    pub fn value(mut self, v: &Json) -> TimelineError {
        let mut s = v.to_string();
        if s.len() > 120 {
            let mut cut = 117;
            while !s.is_char_boundary(cut) {
                cut -= 1;
            }
            s.truncate(cut);
            s.push_str("...");
        }
        self.value = Some(s);
        self
    }

    /// `{error, field, message, allowed?, value?}`.
    pub fn to_json(&self) -> Json {
        let mut f = vec![("error", Json::from(self.code)), ("field", Json::from(self.field.as_str())), ("message", Json::from(self.message.as_str()))];
        if let Some(a) = &self.allowed {
            f.push(("allowed", Json::from(a.as_str())));
        }
        if let Some(v) = &self.value {
            f.push(("value", Json::from(v.as_str())));
        }
        Json::obj(f)
    }
}

impl fmt::Display for TimelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)?;
        if let Some(a) = &self.allowed {
            write!(f, " (allowed: {a})")?;
        }
        Ok(())
    }
}

impl std::error::Error for TimelineError {}

/// A field path being parsed (`tracks[0].move[1]`).
#[derive(Clone, Debug, Default)]
pub struct Path(String);

impl Path {
    pub fn root(name: &str) -> Path {
        Path(name.to_owned())
    }

    pub fn key(&self, k: &str) -> Path {
        if self.0.is_empty() {
            Path(k.to_owned())
        } else {
            Path(format!("{}.{k}", self.0))
        }
    }

    pub fn index(&self, i: usize) -> Path {
        Path(format!("{}[{i}]", self.0))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub type Result<T> = std::result::Result<T, TimelineError>;

/// Field readers that report structured errors.
pub(crate) mod read {
    use super::{Path, Result, TimelineError};
    use specula_core::Json;

    /// Rejects keys of `v` (an object) not in `known`, naming the closest known one.
    pub fn keys(v: &Json, at: &Path, known: &[&str]) -> Result<()> {
        let Some(pairs) = v.as_object() else {
            return Err(TimelineError::new("bad_type", at.as_str(), format!("'{at}' must be an object")).value(v));
        };
        for (i, (k, _)) in pairs.iter().enumerate() {
            // A key given twice: the reader would see only one of them (refused, not guessed).
            if known.contains(&k.as_str()) && pairs[..i].iter().any(|(p, _)| p == k) {
                return Err(TimelineError::new("duplicate", at.key(k).as_str(), format!("'{k}' is given twice in '{at}'")));
            }
            if !known.contains(&k.as_str()) {
                let shown = Json::from(k.as_str());
                let hint = closest(k, known).map(|c| format!("; did you mean '{c}'?")).unwrap_or_default();
                let name = if k.len() > 64 { "(a long key)".to_owned() } else { k.clone() };
                return Err(TimelineError::new("unknown_field", at.key(&name).as_str(), format!("'{name}' is not a field of '{at}'{hint}")).allowed(known.join(", ")).value(&shown));
            }
        }
        Ok(())
    }

    /// The closest name by edit distance (at most 3 edits away; long keys aren't compared).
    fn closest<'a>(key: &str, known: &[&'a str]) -> Option<&'a str> {
        if key.len() > 64 {
            return None;
        }
        known.iter().map(|k| (distance(key, k), *k)).filter(|(d, _)| *d <= 3).min_by_key(|(d, _)| *d).map(|(_, k)| k)
    }

    fn distance(a: &str, b: &str) -> usize {
        let b: Vec<char> = b.chars().collect();
        let mut row: Vec<usize> = (0..=b.len()).collect();
        for (i, ca) in a.chars().enumerate() {
            let mut prev = row[0];
            row[0] = i + 1;
            for (j, cb) in b.iter().enumerate() {
                let cur = row[j + 1];
                row[j + 1] = (prev + usize::from(ca != *cb)).min(row[j] + 1).min(cur + 1);
                prev = cur;
            }
        }
        row[b.len()]
    }

    /// A required field.
    pub fn need<'a>(obj: &'a Json, key: &str, at: &Path, what: &str) -> Result<&'a Json> {
        match obj.get(key) {
            None | Some(Json::Null) => Err(TimelineError::new("missing_field", at.key(key).as_str(), format!("'{}' is required: {what}", at.key(key)))),
            Some(v) => Ok(v),
        }
    }

    /// A finite number in `lo..=hi`.
    pub fn num(v: &Json, at: &Path, lo: f64, hi: f64) -> Result<f64> {
        match v.as_f64() {
            Some(x) if x.is_finite() && (lo..=hi).contains(&x) => Ok(x),
            _ => Err(TimelineError::new("invalid_value", at.as_str(), format!("'{at}' must be a number from {lo} to {hi}")).allowed(format!("{lo}..={hi}")).value(v)),
        }
    }

    /// An optional number field with a default.
    pub fn num_or(obj: &Json, key: &str, at: &Path, lo: f64, hi: f64, default: f64) -> Result<f64> {
        match obj.get(key) {
            None | Some(Json::Null) => Ok(default),
            Some(v) => num(v, &at.key(key), lo, hi),
        }
    }

    pub fn boolean_or(obj: &Json, key: &str, at: &Path, default: bool) -> Result<bool> {
        match obj.get(key) {
            None | Some(Json::Null) => Ok(default),
            Some(v) => v.as_bool().ok_or_else(|| TimelineError::new("bad_type", at.key(key).as_str(), format!("'{}' must be true or false", at.key(key))).value(v)),
        }
    }

    /// `[x, y, z]`, each finite and within ±`limit`.
    pub fn vec3(v: &Json, at: &Path, limit: f64) -> Result<[f64; 3]> {
        let bad = || TimelineError::new("invalid_value", at.as_str(), format!("'{at}' must be [x, y, z], each from -{limit} to {limit}")).allowed("[x, y, z]").value(v);
        match v.as_array() {
            Some([x, y, z]) => {
                let n = |j: &Json| j.as_f64().filter(|f| f.is_finite() && f.abs() <= limit);
                match (n(x), n(y), n(z)) {
                    (Some(x), Some(y), Some(z)) => Ok([x, y, z]),
                    _ => Err(bad()),
                }
            }
            _ => Err(bad()),
        }
    }

    /// A string of 1..=`max` bytes.
    pub fn string<'a>(v: &'a Json, at: &Path, max: usize) -> Result<&'a str> {
        match v.as_str() {
            Some(s) if !s.is_empty() && s.len() <= max => Ok(s),
            Some(s) if s.len() > max => Err(TimelineError::new("too_large", at.as_str(), format!("'{at}' is at most {max} bytes (got {})", s.len()))),
            _ => Err(TimelineError::new("bad_type", at.as_str(), format!("'{at}' must be a non-empty string of at most {max} bytes")).value(v)),
        }
    }

    /// An array of at most `max` items (at least `min`).
    pub fn array<'a>(v: &'a Json, at: &Path, min: usize, max: usize) -> Result<&'a [Json]> {
        match v.as_array() {
            Some(a) if a.len() > max => Err(TimelineError::new("too_many", at.as_str(), format!("'{at}' holds at most {max} items (got {})", a.len())).allowed(format!("{min}..={max} items"))),
            Some(a) if a.len() < min => Err(TimelineError::new("invalid_value", at.as_str(), format!("'{at}' needs at least {min} item(s)")).allowed(format!("{min}..={max} items"))),
            Some(a) => Ok(a),
            None => Err(TimelineError::new("bad_type", at.as_str(), format!("'{at}' must be an array of {min}..={max} items")).value(v)),
        }
    }

    /// `"#rrggbb"` as sRGB 0..1.
    pub fn colour(v: &Json, at: &Path) -> Result<[f64; 3]> {
        let bad = || TimelineError::new("invalid_value", at.as_str(), format!("'{at}' must be a colour \"#rrggbb\" (sRGB)")).allowed("\"#rrggbb\"").value(v);
        let s = v.as_str().ok_or_else(bad)?;
        let hex = s.strip_prefix('#').filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit())).ok_or_else(bad)?;
        let c = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map(|b| f64::from(b) / 255.0).map_err(|_| bad());
        Ok([c(0)?, c(2)?, c(4)?])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Probatio survivors: the readers' bounds, exactly at and past each edge.
    #[test]
    fn readers_hold_their_edges() {
        let at = Path::root("x");
        // Values echo at most 120 bytes (a longer one is cut to 117 and "...").
        let cut = TimelineError::new("c", "x", "m").value(&Json::from("a".repeat(120))).value.unwrap();
        assert!(cut.len() == 120 && cut.ends_with("..."), "{cut}");
        let fits = TimelineError::new("c", "x", "m").value(&Json::from("a".repeat(118))).value.unwrap();
        assert!(fits.len() == 120 && !fits.ends_with("..."), "{fits}");
        // Coordinates: finite and within the limit, both.
        assert!(read::vec3(&Json::parse("[1, 2, 3]").unwrap(), &at, 3.0).is_ok());
        assert!(read::vec3(&Json::parse("[1, 2, 3.5]").unwrap(), &at, 3.0).is_err());
        assert!(read::vec3(&Json::parse("[1, 2, 1e300]").unwrap(), &at, 3.0).is_err());
        // Strings: up to max bytes.
        assert!(read::string(&Json::from("abcd"), &at, 4).is_ok());
        assert_eq!(read::string(&Json::from("abcde"), &at, 4).unwrap_err().code, "too_large");
        // Arrays: up to max items, at least min.
        assert!(read::array(&Json::parse("[1, 2]").unwrap(), &at, 1, 2).is_ok());
        assert_eq!(read::array(&Json::parse("[1, 2, 3]").unwrap(), &at, 1, 2).unwrap_err().code, "too_many");
        assert_eq!(read::array(&Json::parse("[]").unwrap(), &at, 1, 2).unwrap_err().code, "invalid_value");
        // A long unknown key isn't compared or echoed whole.
        let long = format!("{{\"{}\": 1}}", "k".repeat(65));
        let e = read::keys(&Json::parse(&long).unwrap(), &at, &["kind"]).unwrap_err();
        assert!(e.field.ends_with("(a long key)"), "{}", e.field);
        let ok64 = format!("{{\"{}\": 1}}", "k".repeat(64));
        let e = read::keys(&Json::parse(&ok64).unwrap(), &at, &["kind"]).unwrap_err();
        assert!(!e.field.contains("(a long key)"));
    }
}
