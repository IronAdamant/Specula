//! Loading fonts from disk, and finding a system UI font for development.
//!
//! The shipping font is an open owner decision; games should bundle their own font file. The
//! system lookup exists so tools, tests and greybox HUDs have *something* to draw with.

use crate::error::FontError;
use crate::font::Font;
use std::fmt;
use std::path::{Path, PathBuf};

/// Error loading a font file.
#[derive(Debug)]
pub enum LoadError {
    Io { path: PathBuf, kind: std::io::ErrorKind, message: String },
    Font { path: PathBuf, error: FontError },
}

impl LoadError {
    pub fn code(&self) -> &'static str {
        match self {
            LoadError::Io { .. } => "io",
            LoadError::Font { error, .. } => error.reason.code(),
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io { path, message, .. } => write!(f, "cannot read font {}: {message}", path.display()),
            LoadError::Font { path, error } => write!(f, "{}: {error}", path.display()),
        }
    }
}

impl std::error::Error for LoadError {}

/// Read and parse a font file.
pub fn load_font_file(path: impl AsRef<Path>) -> Result<Font, LoadError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|e| LoadError::Io {
        path: path.to_path_buf(),
        kind: e.kind(),
        message: e.to_string(),
    })?;
    Font::from_bytes(bytes).map_err(|error| LoadError::Font { path: path.to_path_buf(), error })
}

/// Candidate UI font files for this platform, best first.
pub fn system_ui_font_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if cfg!(windows) {
        let root = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let fonts = root.join("Fonts");
        for name in ["segoeui.ttf", "arial.ttf", "tahoma.ttf"] {
            out.push(fonts.join(name));
        }
    } else if cfg!(target_os = "android") {
        for p in ["/system/fonts/Roboto-Regular.ttf", "/system/fonts/NotoSans-Regular.ttf", "/system/fonts/DroidSans.ttf"] {
            out.push(PathBuf::from(p));
        }
    } else if cfg!(target_os = "macos") {
        // Most macOS UI fonts are CFF / .ttc; Arial is plain TrueType.
        out.push(PathBuf::from("/System/Library/Fonts/Supplemental/Arial.ttf"));
        out.push(PathBuf::from("/Library/Fonts/Arial.ttf"));
    } else {
        for p in [
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        ] {
            out.push(PathBuf::from(p));
        }
    }
    out
}

/// The first system UI font that exists and parses, with its path; `None` if there is none
/// (headless CI images often have no fonts — callers and tests must handle that).
pub fn find_system_ui_font() -> Option<(PathBuf, Font)> {
    system_ui_font_candidates()
        .into_iter()
        .filter(|p| p.is_file())
        .find_map(|p| load_font_file(&p).ok().map(|f| (p, f)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_a_structured_error() {
        let e = load_font_file("definitely/not/here.ttf").unwrap_err();
        assert_eq!(e.code(), "io");
        assert!(matches!(e, LoadError::Io { kind: std::io::ErrorKind::NotFound, .. }));
    }

    #[test]
    fn candidates_exist_for_this_platform() {
        assert!(!system_ui_font_candidates().is_empty());
    }
}
