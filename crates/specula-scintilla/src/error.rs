//! Structured errors: what was wrong, where (a field path such as `emitters[0].lifetime`), and
//! what would be accepted, so an agent can fix the input from the data alone.

use specula_core::Json;
use std::fmt;

/// A rejected effect, component or decal field.
#[derive(Clone, Debug, PartialEq)]
pub struct VfxError {
    /// Machine-readable kind: `invalid_value`, `unknown_field`, `missing_field`, `too_many`,
    /// `unknown_texture`, `unknown_effect`, `bad_type`.
    pub code: &'static str,
    /// Where: a dotted path with indices (`emitters[1].color.keys[2]`).
    pub field: String,
    /// What would be accepted (a range, a list of names, a shape), when there is one.
    pub allowed: Option<String>,
    /// The value given (compact JSON), when there was one.
    pub value: Option<String>,
    pub message: String,
}

impl VfxError {
    pub fn new(code: &'static str, field: impl Into<String>, message: impl Into<String>) -> VfxError {
        VfxError { code, field: field.into(), allowed: None, value: None, message: message.into() }
    }

    pub fn allowed(mut self, allowed: impl Into<String>) -> VfxError {
        self.allowed = Some(allowed.into());
        self
    }

    pub fn value(mut self, v: &Json) -> VfxError {
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

impl fmt::Display for VfxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)?;
        if let Some(a) = &self.allowed {
            write!(f, " (allowed: {a})")?;
        }
        Ok(())
    }
}

impl std::error::Error for VfxError {}

/// A field path being parsed (`emitters[0].size.keys[1]`).
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

pub type Result<T> = std::result::Result<T, VfxError>;

/// Field readers that report structured errors.
pub(crate) mod read {
    use super::{Path, Result, VfxError};
    use specula_core::Json;

    /// Rejects keys of `v` (an object) not in `known`, naming the closest known one.
    pub fn keys(v: &Json, at: &Path, known: &[&str]) -> Result<()> {
        let Some(pairs) = v.as_object() else {
            return Err(VfxError::new("bad_type", at.as_str(), format!("'{at}' must be an object")).value(v));
        };
        for (k, _) in pairs {
            if !known.contains(&k.as_str()) {
                let hint = closest(k, known).map(|c| format!("; did you mean '{c}'?")).unwrap_or_default();
                return Err(VfxError::new("unknown_field", at.key(k).as_str(), format!("'{k}' is not a field of '{at}'{hint}")).allowed(known.join(", ")));
            }
        }
        Ok(())
    }

    /// The closest name by edit distance (at most 3 edits away).
    fn closest<'a>(key: &str, known: &[&'a str]) -> Option<&'a str> {
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

    /// A finite number in `lo..=hi`.
    pub fn num(v: &Json, at: &Path, lo: f32, hi: f32) -> Result<f32> {
        match v.as_f64() {
            Some(x) if x.is_finite() && (lo as f64..=hi as f64).contains(&x) => Ok(x as f32),
            _ => Err(VfxError::new("invalid_value", at.as_str(), format!("'{at}' must be a number from {lo} to {hi}")).allowed(format!("{lo}..={hi}")).value(v)),
        }
    }

    /// An optional number field with a default.
    pub fn num_or(obj: &Json, key: &str, at: &Path, lo: f32, hi: f32, default: f32) -> Result<f32> {
        match obj.get(key) {
            None | Some(Json::Null) => Ok(default),
            Some(v) => num(v, &at.key(key), lo, hi),
        }
    }

    /// A whole number in `lo..=hi`.
    pub fn int(v: &Json, at: &Path, lo: u64, hi: u64) -> Result<u64> {
        match v.as_f64() {
            Some(x) if x.is_finite() && specula_core::dmath::trunc_f64(x) == x && x >= lo as f64 && x <= hi as f64 => Ok(x as u64),
            _ => Err(VfxError::new("invalid_value", at.as_str(), format!("'{at}' must be a whole number from {lo} to {hi}")).allowed(format!("{lo}..={hi}")).value(v)),
        }
    }

    pub fn int_or(obj: &Json, key: &str, at: &Path, lo: u64, hi: u64, default: u64) -> Result<u64> {
        match obj.get(key) {
            None | Some(Json::Null) => Ok(default),
            Some(v) => int(v, &at.key(key), lo, hi),
        }
    }

    pub fn boolean_or(obj: &Json, key: &str, at: &Path, default: bool) -> Result<bool> {
        match obj.get(key) {
            None | Some(Json::Null) => Ok(default),
            Some(v) => v.as_bool().ok_or_else(|| VfxError::new("bad_type", at.key(key).as_str(), format!("'{}' must be true or false", at.key(key))).value(v)),
        }
    }

    /// `[min, max]` (or one number for both), each in `lo..=hi`, min ≤ max.
    pub fn range(v: &Json, at: &Path, lo: f32, hi: f32) -> Result<[f32; 2]> {
        if v.as_f64().is_some() {
            let x = num(v, at, lo, hi)?;
            return Ok([x, x]);
        }
        let bad = || VfxError::new("invalid_value", at.as_str(), format!("'{at}' must be [min, max] (or one number) from {lo} to {hi}, min <= max")).allowed(format!("[{lo}..={hi}, {lo}..={hi}]")).value(v);
        match v.as_array() {
            Some([a, b]) => {
                let (a, b) = (num(a, &at.index(0), lo, hi)?, num(b, &at.index(1), lo, hi)?);
                if a > b {
                    return Err(bad());
                }
                Ok([a, b])
            }
            _ => Err(bad()),
        }
    }

    pub fn range_or(obj: &Json, key: &str, at: &Path, lo: f32, hi: f32, default: [f32; 2]) -> Result<[f32; 2]> {
        match obj.get(key) {
            None | Some(Json::Null) => Ok(default),
            Some(v) => range(v, &at.key(key), lo, hi),
        }
    }

    /// `[x, y, z]`, each in `-limit..=limit`.
    pub fn vec3(v: &Json, at: &Path, limit: f32) -> Result<[f32; 3]> {
        match v.as_array() {
            Some([x, y, z]) => Ok([num(x, &at.index(0), -limit, limit)?, num(y, &at.index(1), -limit, limit)?, num(z, &at.index(2), -limit, limit)?]),
            _ => Err(VfxError::new("invalid_value", at.as_str(), format!("'{at}' must be [x, y, z]")).allowed(format!("three numbers from {} to {limit}", -limit)).value(v)),
        }
    }

    pub fn vec3_or(obj: &Json, key: &str, at: &Path, limit: f32, default: [f32; 3]) -> Result<[f32; 3]> {
        match obj.get(key) {
            None | Some(Json::Null) => Ok(default),
            Some(v) => vec3(v, &at.key(key), limit),
        }
    }

    pub fn string<'a>(v: &'a Json, at: &Path) -> Result<&'a str> {
        v.as_str().ok_or_else(|| VfxError::new("bad_type", at.as_str(), format!("'{at}' must be a string")).value(v))
    }

    /// An sRGB colour: `"#rrggbb"` or `[r, g, b]` in 0..1.
    pub fn color(v: &Json, at: &Path) -> Result<[f32; 3]> {
        if let Some(s) = v.as_str() {
            if let Some(c) = hex(s) {
                return Ok(c);
            }
        } else if let Some([r, g, b]) = v.as_array() {
            return Ok([num(r, &at.index(0), 0.0, 1.0)?, num(g, &at.index(1), 0.0, 1.0)?, num(b, &at.index(2), 0.0, 1.0)?]);
        }
        Err(VfxError::new("invalid_value", at.as_str(), format!("'{at}' must be an sRGB colour: \"#rrggbb\" or [r, g, b] from 0 to 1")).allowed("\"#rrggbb\" | [r, g, b]").value(v))
    }

    pub fn hex(s: &str) -> Option<[f32; 3]> {
        let h = s.strip_prefix('#')?;
        if h.len() != 6 || !h.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(|b| b as f32 / 255.0);
        Some([byte(0)?, byte(2)?, byte(4)?])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_carry_field_allowed_and_value() {
        let e = read::num(&Json::from(12.0), &Path::root("lifetime"), 0.01, 10.0).unwrap_err();
        assert_eq!(e.code, "invalid_value");
        assert_eq!(e.field, "lifetime");
        assert_eq!(e.allowed.as_deref(), Some("0.01..=10"));
        assert_eq!(e.value.as_deref(), Some("12"));
        let j = e.to_json();
        assert_eq!(j.get("error").and_then(Json::as_str), Some("invalid_value"));
    }

    #[test]
    fn unknown_keys_suggest_the_closest() {
        let v = Json::parse(r#"{"lifetim": 1}"#).unwrap();
        let e = read::keys(&v, &Path::root("emitters").index(0), &["lifetime", "rate"]).unwrap_err();
        assert_eq!(e.field, "emitters[0].lifetim");
        assert!(e.message.contains("did you mean 'lifetime'"), "{e}");
    }

    #[test]
    fn ranges_and_colours() {
        let p = Path::root("x");
        assert_eq!(read::range(&Json::parse("[1, 2]").unwrap(), &p, 0.0, 5.0).unwrap(), [1.0, 2.0]);
        assert_eq!(read::range(&Json::from(3.0), &p, 0.0, 5.0).unwrap(), [3.0, 3.0]);
        assert!(read::range(&Json::parse("[2, 1]").unwrap(), &p, 0.0, 5.0).is_err());
        assert_eq!(read::color(&Json::from("#ff8000"), &p).unwrap(), [1.0, 128.0 / 255.0, 0.0]);
        assert!(read::color(&Json::from("#ff80"), &p).is_err());
        assert!(read::color(&Json::parse("[1, 2, 0]").unwrap(), &p).is_err());
    }
}
