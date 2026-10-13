//! Structured animation errors: a path into the source JSON, a machine-readable code and a reason.

use std::fmt;

use specula_core::Json;

/// What went wrong, for agents to branch on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimErrorCode {
    /// The text isn't valid JSON (`offset` holds the byte offset).
    Json,
    /// A required field is missing.
    MissingField,
    /// A field this object doesn't accept (typos are errors, not silently ignored).
    UnknownField,
    /// A field has the wrong JSON type or shape.
    WrongType,
    /// A number is outside its allowed range (or not finite).
    OutOfRange,
    /// A bone name the skeleton doesn't have.
    UnknownBone,
    /// A name used twice where names must be unique (tracks, clips, masks, keys).
    Duplicate,
    /// Key times that don't strictly increase.
    NotIncreasing,
    /// An unknown `kind`, `channel`, `interp`, `loop` or mask name.
    UnknownName,
    /// A safety limit (keys, tracks, events, duration) was exceeded.
    Limit,
    /// The skeleton is unusable (bad parents, non-finite rest pose, unsupported rest frames).
    InvalidRig,
}

/// An animation error. `path` points into the source, e.g. `clips[2].tracks[0].times[3]`.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimError {
    pub path: String,
    pub code: AnimErrorCode,
    pub message: String,
    /// Byte offset for [`AnimErrorCode::Json`].
    pub offset: Option<usize>,
    /// Measured value for range and limit errors.
    pub measured: Option<f64>,
    /// The bound that was broken.
    pub limit: Option<f64>,
}

impl AnimError {
    pub fn new(path: impl Into<String>, code: AnimErrorCode, message: impl Into<String>) -> AnimError {
        AnimError { path: path.into(), code, message: message.into(), offset: None, measured: None, limit: None }
    }

    pub fn with_numbers(mut self, measured: f64, limit: f64) -> AnimError {
        self.measured = Some(measured);
        self.limit = Some(limit);
        self
    }

    /// Prefixes the path (`clips[2]` + `.tracks[0]`).
    pub fn under(mut self, prefix: &str) -> AnimError {
        self.path = match (prefix.is_empty(), self.path.is_empty()) {
            (true, _) => self.path,
            (false, true) => prefix.to_string(),
            (false, false) if self.path.starts_with('[') => format!("{prefix}{}", self.path),
            _ => format!("{prefix}.{}", self.path),
        };
        self
    }

    /// The error as JSON, for tool responses.
    pub fn to_json(&self) -> Json {
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

impl fmt::Display for AnimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{:?}: {}", self.code, self.message)
        } else {
            write!(f, "{:?} at {}: {}", self.code, self.path, self.message)
        }
    }
}

impl std::error::Error for AnimError {}

pub(crate) fn err(path: &str, code: AnimErrorCode, msg: impl Into<String>) -> AnimError {
    AnimError::new(path, code, msg)
}

// ---------------------------------------------------------------------------------------------
// Strict JSON object reading, shared by clips, generator specs and anim sets.

/// Reads one JSON object field by field and rejects anything not read (`finish`).
pub(crate) struct Obj<'a> {
    pub path: String,
    fields: &'a [(String, Json)],
    used: Vec<bool>,
}

impl<'a> Obj<'a> {
    pub fn new(v: &'a Json, path: &str, what: &str) -> Result<Obj<'a>, AnimError> {
        let fields = v.as_object().ok_or_else(|| err(path, AnimErrorCode::WrongType, format!("expected {what} (an object)")))?;
        // Duplicate keys, found by sorting (O(n log n) for hostile objects with many keys).
        let mut keys: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
        keys.sort_unstable();
        if let Some(w) = keys.windows(2).find(|w| w[0] == w[1]) {
            return Err(err(&self_path(path, w[0]), AnimErrorCode::Duplicate, format!("duplicate key '{}'", w[0])));
        }
        Ok(Obj { path: path.to_string(), fields, used: vec![false; fields.len()] })
    }

    pub fn at(&self, k: &str) -> String {
        self_path(&self.path, k)
    }

    pub fn get(&mut self, k: &str) -> Option<&'a Json> {
        let i = self.fields.iter().position(|(n, _)| n == k)?;
        self.used[i] = true;
        Some(&self.fields[i].1)
    }

    pub fn req(&mut self, k: &str) -> Result<&'a Json, AnimError> {
        let p = self.at(k);
        self.get(k).ok_or_else(|| err(&p, AnimErrorCode::MissingField, format!("missing \"{k}\"")))
    }

    pub fn str(&mut self, k: &str) -> Result<Option<&'a str>, AnimError> {
        let p = self.at(k);
        match self.get(k) {
            None => Ok(None),
            Some(v) => v.as_str().map(Some).ok_or_else(|| err(&p, AnimErrorCode::WrongType, "expected a string")),
        }
    }

    pub fn req_str(&mut self, k: &str) -> Result<&'a str, AnimError> {
        let p = self.at(k);
        self.str(k)?.ok_or_else(|| err(&p, AnimErrorCode::MissingField, format!("missing \"{k}\"")))
    }

    /// A number in `lo..=hi`, or `default` when absent.
    pub fn num(&mut self, k: &str, default: f32, lo: f32, hi: f32) -> Result<f32, AnimError> {
        match self.get(k) {
            None => Ok(default),
            Some(v) => num(v, &self.at(k), lo, hi),
        }
    }

    pub fn req_num(&mut self, k: &str, lo: f32, hi: f32) -> Result<f32, AnimError> {
        let v = self.req(k)?;
        num(v, &self.at(k), lo, hi)
    }

    /// A list of exactly `N` numbers, each in `lo..=hi`.
    pub fn vec<const N: usize>(&mut self, k: &str, default: [f32; N], lo: f32, hi: f32) -> Result<[f32; N], AnimError> {
        match self.get(k) {
            None => Ok(default),
            Some(v) => vecn(v, &self.at(k), lo, hi),
        }
    }

    pub fn arr(&mut self, k: &str) -> Result<Option<&'a [Json]>, AnimError> {
        let p = self.at(k);
        match self.get(k) {
            None => Ok(None),
            Some(v) => v.as_array().map(Some).ok_or_else(|| err(&p, AnimErrorCode::WrongType, "expected a list")),
        }
    }

    /// Errors on the first field nobody read, listing what the object accepts.
    pub fn finish(&self, accepts: &str) -> Result<(), AnimError> {
        for (i, (k, _)) in self.fields.iter().enumerate() {
            if !self.used[i] && k != "note" && k != "description" {
                return Err(err(&self.at(k), AnimErrorCode::UnknownField, format!("accepted here: {accepts}, note, description")));
            }
        }
        Ok(())
    }
}

fn self_path(path: &str, k: &str) -> String {
    if path.is_empty() {
        k.to_string()
    } else {
        format!("{path}.{k}")
    }
}

pub(crate) fn num(v: &Json, path: &str, lo: f32, hi: f32) -> Result<f32, AnimError> {
    let x = v.as_f64().ok_or_else(|| err(path, AnimErrorCode::WrongType, "expected a number"))?;
    let f = x as f32;
    if !x.is_finite() || !(lo..=hi).contains(&f) {
        return Err(err(path, AnimErrorCode::OutOfRange, format!("{x} is outside {lo}..{hi}")).with_numbers(x, if f > hi { hi as f64 } else { lo as f64 }));
    }
    Ok(f)
}

pub(crate) fn vecn<const N: usize>(v: &Json, path: &str, lo: f32, hi: f32) -> Result<[f32; N], AnimError> {
    let items = v.as_array().filter(|a| a.len() == N).ok_or_else(|| err(path, AnimErrorCode::WrongType, format!("expected a list of {N} numbers")))?;
    let mut out = [0.0; N];
    for (i, x) in items.iter().enumerate() {
        out[i] = num(x, &format!("{path}[{i}]"), lo, hi)?;
    }
    Ok(out)
}

/// An identifier: letters, digits and `_`, not starting with a digit.
pub(crate) fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|f| f.is_ascii_alphabetic() || f == '_') && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
