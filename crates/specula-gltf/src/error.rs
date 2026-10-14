//! Structured import/export errors and warnings: a JSON path into the glTF document, a
//! machine-readable code, a byte offset when one applies, and measured/limit numbers.

use std::fmt;

use specula_core::Json;

/// What went wrong, for agents to branch on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GltfErrorCode {
    /// Reading a file failed (missing, unreadable).
    Io,
    /// The JSON chunk or file isn't valid JSON (`offset` = byte offset in the JSON text).
    Json,
    /// The GLB container is malformed (header, chunk lengths, chunk types).
    Container,
    /// A URI is malformed, escapes the asset folder, or names an unsupported scheme.
    Uri,
    /// A required property is missing.
    MissingField,
    /// A property has the wrong JSON type or shape.
    WrongType,
    /// A number or enum value is outside its allowed range.
    OutOfRange,
    /// An index refers to an object that doesn't exist.
    BadReference,
    /// An accessor, buffer view or chunk reaches past the end of its data.
    Bounds,
    /// Data that parses but breaks a rule: NaN, unsorted times, indices out of range, cycles.
    InvalidData,
    /// A feature this importer doesn't implement (POINTS/LINES, a required extension, ...).
    Unsupported,
    /// A size limit ([`crate::Limits`]) was exceeded.
    Limit,
    /// An image failed to decode (PNG or JPEG error in `message`).
    Image,
}

/// An import or export failure.
#[derive(Clone, Debug, PartialEq)]
pub struct GltfError {
    pub code: GltfErrorCode,
    /// JSON path into the document, e.g. `meshes[0].primitives[1].attributes.POSITION`
    /// (empty for container-level errors).
    pub path: String,
    /// Byte offset (into the file, a buffer or the JSON text, as the message says), if any.
    pub offset: Option<u64>,
    pub message: String,
    /// Measured value for limit and range errors.
    pub measured: Option<f64>,
    /// The bound that was broken.
    pub limit: Option<f64>,
}

impl GltfError {
    pub fn new(code: GltfErrorCode, path: impl Into<String>, message: impl Into<String>) -> GltfError {
        GltfError { code, path: path.into(), offset: None, message: message.into(), measured: None, limit: None }
    }

    pub fn at_offset(mut self, offset: u64) -> GltfError {
        self.offset = Some(offset);
        self
    }

    pub fn with_numbers(mut self, measured: f64, limit: f64) -> GltfError {
        self.measured = Some(measured);
        self.limit = Some(limit);
        self
    }

    /// The error as JSON, for tool responses.
    pub fn to_json(&self) -> Json {
        let mut pairs = vec![("code", Json::Str(format!("{:?}", self.code))), ("path", Json::Str(self.path.clone())), ("message", Json::Str(self.message.clone()))];
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

impl fmt::Display for GltfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.code)?;
        if !self.path.is_empty() {
            write!(f, " at {}", self.path)?;
        }
        if let Some(o) = self.offset {
            write!(f, " (byte {o})")?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for GltfError {}

pub(crate) fn gerr(code: GltfErrorCode, path: &str, message: impl Into<String>) -> GltfError {
    GltfError::new(code, path, message)
}

/// Most warnings kept; later ones are dropped behind one `warnings_truncated` (hostile files
/// can't grow the list without bound).
pub const MAX_WARNINGS: usize = 256;

/// Appends `warning` unless the list is full.
pub(crate) fn push_warning(list: &mut Vec<Warning>, warning: Warning) {
    match list.len() {
        n if n < MAX_WARNINGS => list.push(warning),
        MAX_WARNINGS => list.push(Warning { code: "warnings_truncated", path: String::new(), message: format!("more than {MAX_WARNINGS} warnings; the rest were dropped") }),
        _ => {}
    }
}

/// Something the importer ignored, approximated or repaired. Imports succeed with warnings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    /// Short machine-readable kind, e.g. `ignored_extension`, `degenerate_triangles`.
    pub code: &'static str,
    pub path: String,
    pub message: String,
}

impl Warning {
    pub fn to_json(&self) -> Json {
        Json::obj([("code", Json::Str(self.code.into())), ("path", Json::Str(self.path.clone())), ("message", Json::Str(self.message.clone()))])
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}: {}", self.code, self.message)
        } else {
            write!(f, "{} at {}: {}", self.code, self.path, self.message)
        }
    }
}
