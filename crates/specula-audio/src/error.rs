//! Structured audio errors: a machine-readable code, a path or offset, and OS result codes.

use specula_core::Json;
use std::fmt;

/// What went wrong, for agents and the engine to branch on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    /// Recipe text isn't valid JSON (`offset` is the byte offset).
    Json,
    /// A required recipe field is missing.
    MissingField,
    /// A recipe field this object doesn't accept (typos are errors, not silently ignored).
    UnknownField,
    /// A recipe field has the wrong JSON type or shape.
    WrongType,
    /// A number is outside its allowed range (`measured` vs `limit`).
    OutOfRange,
    /// An unknown name (waveform, filter type, bus…).
    UnknownName,
    /// A field or layer name that appears twice.
    Duplicate,
    /// A safety limit was exceeded (render length, layers, repeats, file size).
    Limit,
    /// The bytes are not a RIFF/WAVE file.
    WavNotWave,
    /// A WAV `fmt ` chunk is inconsistent (zero channels, block align mismatch…).
    WavBadFormat,
    /// A WAV encoding this decoder doesn't support (`measured` = format tag, `limit` = bits).
    WavUnsupported,
    /// A required WAV chunk (`fmt `, `data`) is missing or cut short.
    WavTruncated,
    /// There is no audio output device (none installed, or none enabled).
    NoDevice,
    /// The device went away (unplugged, disabled, default changed) or the audio service stopped.
    DeviceLost,
    /// A COM / WASAPI call failed (`hresult` holds the result code).
    Os,
    /// The device's mix format can't be rendered (not float/int PCM).
    DeviceFormat,
    /// This backend is not available on this platform.
    Unsupported,
    /// The command queue to the audio thread is full; the command was dropped.
    QueueFull,
    /// The call isn't valid in this backend mode (e.g. `pull` on a real-time backend).
    WrongMode,
    /// The audio thread stopped or panicked.
    ThreadGone,
    /// A sound buffer is invalid (empty, wrong channel count, rate out of range).
    BadSound,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Json => "json",
            ErrorCode::MissingField => "missing_field",
            ErrorCode::UnknownField => "unknown_field",
            ErrorCode::WrongType => "wrong_type",
            ErrorCode::OutOfRange => "out_of_range",
            ErrorCode::UnknownName => "unknown_name",
            ErrorCode::Duplicate => "duplicate",
            ErrorCode::Limit => "limit",
            ErrorCode::WavNotWave => "wav_not_wave",
            ErrorCode::WavBadFormat => "wav_bad_format",
            ErrorCode::WavUnsupported => "wav_unsupported",
            ErrorCode::WavTruncated => "wav_truncated",
            ErrorCode::NoDevice => "no_device",
            ErrorCode::DeviceLost => "device_lost",
            ErrorCode::Os => "os",
            ErrorCode::DeviceFormat => "device_format",
            ErrorCode::Unsupported => "unsupported",
            ErrorCode::QueueFull => "queue_full",
            ErrorCode::WrongMode => "wrong_mode",
            ErrorCode::ThreadGone => "thread_gone",
            ErrorCode::BadSound => "bad_sound",
        }
    }
}

/// An audio error. `path` points into a recipe (`layers[2].filters[0].cutoff`) when relevant.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioError {
    pub code: ErrorCode,
    pub message: String,
    /// Path into a recipe, or the failing call for OS errors (`IAudioClient::Initialize`).
    pub path: String,
    /// Byte offset (JSON text, WAV file).
    pub offset: Option<usize>,
    /// The raw `HRESULT` of a failed COM call.
    pub hresult: Option<u32>,
    /// Measured value for range/limit errors.
    pub measured: Option<f64>,
    /// The bound that was broken.
    pub limit: Option<f64>,
}

impl AudioError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> AudioError {
        AudioError { code, message: message.into(), path: String::new(), offset: None, hresult: None, measured: None, limit: None }
    }

    pub fn at(code: ErrorCode, path: impl Into<String>, message: impl Into<String>) -> AudioError {
        AudioError { path: path.into(), ..AudioError::new(code, message) }
    }

    pub fn with_numbers(mut self, measured: f64, limit: f64) -> AudioError {
        self.measured = Some(measured);
        self.limit = Some(limit);
        self
    }

    pub fn with_offset(mut self, offset: usize) -> AudioError {
        self.offset = Some(offset);
        self
    }

    pub fn with_hresult(mut self, hr: u32) -> AudioError {
        self.hresult = Some(hr);
        self
    }

    /// The error as JSON, for tool responses.
    pub fn to_json(&self) -> Json {
        let mut pairs = vec![("code".to_string(), Json::from(self.code.as_str())), ("message".to_string(), Json::from(self.message.as_str()))];
        if !self.path.is_empty() {
            pairs.push(("path".into(), Json::from(self.path.as_str())));
        }
        if let Some(o) = self.offset {
            pairs.push(("offset".into(), Json::from(o)));
        }
        if let Some(hr) = self.hresult {
            pairs.push(("hresult".into(), Json::from(format!("0x{hr:08X}"))));
        }
        if let Some(m) = self.measured {
            pairs.push(("measured".into(), Json::from(m)));
        }
        if let Some(l) = self.limit {
            pairs.push(("limit".into(), Json::from(l)));
        }
        Json::Obj(pairs)
    }
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "audio error [{}]", self.code.as_str())?;
        if !self.path.is_empty() {
            write!(f, " at {}", self.path)?;
        }
        write!(f, ": {}", self.message)?;
        if let Some(o) = self.offset {
            write!(f, " (byte {o})")?;
        }
        if let Some(hr) = self.hresult {
            write!(f, " (HRESULT 0x{hr:08X})")?;
        }
        if let (Some(m), Some(l)) = (self.measured, self.limit) {
            write!(f, " (measured {m}, limit {l})")?;
        }
        Ok(())
    }
}

impl std::error::Error for AudioError {}

pub type Result<T> = std::result::Result<T, AudioError>;
