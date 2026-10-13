//! Structured errors: a machine-readable code, the JSON path (or byte offset) of the problem, and
//! a human message.

use std::fmt;

/// What went wrong, as a stable machine-readable code ([`ErrorCode::as_str`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// The text is not valid JSON (`offset` is set).
    JsonSyntax,
    /// Missing or unsupported `"format"` tag.
    Format,
    MissingField,
    /// A field has the wrong JSON type.
    WrongType,
    /// A field has the right type but an invalid value (range, enum spelling, name syntax).
    BadValue,
    /// A source name that doesn't exist (`"Key.Spaec"`, `"Pad.Z"`).
    UnknownSource,
    /// A source that can't drive this kind of action (e.g. a key on a 2D axis without a composite).
    IncompatibleSource,
    DuplicateAction,
    UnknownAction,
    /// A binding index out of range.
    UnknownBinding,
    /// A recording's bytes are malformed (`offset` is set).
    BadRecording,
    /// A recording or frame was made for a different set of actions.
    SchemaMismatch,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::JsonSyntax => "json_syntax",
            ErrorCode::Format => "format",
            ErrorCode::MissingField => "missing_field",
            ErrorCode::WrongType => "wrong_type",
            ErrorCode::BadValue => "bad_value",
            ErrorCode::UnknownSource => "unknown_source",
            ErrorCode::IncompatibleSource => "incompatible_source",
            ErrorCode::DuplicateAction => "duplicate_action",
            ErrorCode::UnknownAction => "unknown_action",
            ErrorCode::UnknownBinding => "unknown_binding",
            ErrorCode::BadRecording => "bad_recording",
            ErrorCode::SchemaMismatch => "schema_mismatch",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct InputError {
    pub code: ErrorCode,
    /// Where: a JSON path such as `actions[2].bindings[0].scale`, or empty.
    pub path: String,
    /// Byte offset into the input (JSON text or recording bytes) when known.
    pub offset: Option<usize>,
    pub message: String,
}

impl InputError {
    pub fn new(code: ErrorCode, path: impl Into<String>, message: impl Into<String>) -> InputError {
        InputError { code, path: path.into(), offset: None, message: message.into() }
    }

    pub(crate) fn at(mut self, offset: usize) -> InputError {
        self.offset = Some(offset);
        self
    }
}

impl fmt::Display for InputError {
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

impl std::error::Error for InputError {}
