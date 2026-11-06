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

/// Fallback font files for `locale` (a BCP 47 tag such as `ja`, `zh-Hans`, `zh-TW`, `ko`, `en`),
/// best first: the system's CJK faces in the order that locale prefers, then a symbol face. The
/// order matters because Chinese, Japanese and Korean share code points (Han unification) but
/// not glyph shapes: Japanese text should find a Japanese face first. Other locales get every
/// CJK face (Simplified Chinese, Japanese, Korean, Traditional Chinese). The bundled faces cover
/// Latin, Cyrillic and Greek themselves ([`crate::Bundled`]). Files may be missing or use CFF
/// outlines (Noto CJK on Linux): [`load_fallbacks`] skips what doesn't load.
pub fn system_fallback_candidates(locale: &str) -> Vec<PathBuf> {
    let lang = locale.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    let region = locale.to_ascii_lowercase();
    let traditional = region.contains("hant") || region.ends_with("-tw") || region.ends_with("-hk") || region.ends_with("-mo") || region.ends_with("_tw");
    type Names = &'static [&'static str];
    let (sc, ja, ko, tc, sym): (Names, Names, Names, Names, Names) = if cfg!(windows) {
        (&["msyh.ttc", "simsun.ttc"], &["YuGothM.ttc", "meiryo.ttc", "msgothic.ttc"], &["malgun.ttf", "gulim.ttc"], &["msjh.ttc", "mingliu.ttc"], &["seguisym.ttf"])
    } else if cfg!(target_os = "macos") {
        (
            &["/System/Library/Fonts/PingFang.ttc", "/System/Library/Fonts/STHeiti Light.ttc"],
            &["/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", "/System/Library/Fonts/Hiragino Sans GB.ttc"],
            &["/System/Library/Fonts/AppleSDGothicNeo.ttc"],
            &["/System/Library/Fonts/PingFang.ttc"],
            &["/System/Library/Fonts/Apple Symbols.ttf"],
        )
    } else {
        (
            &[
                "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
                "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
                "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            ],
            &["/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf", "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf"],
            &["/usr/share/fonts/truetype/nanum/NanumGothic.ttf"],
            &[],
            &["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"],
        )
    };
    let order: Vec<&[&str]> = match lang.as_str() {
        "ja" => vec![ja, sc, ko, tc],
        "ko" => vec![ko, sc, ja, tc],
        "zh" if traditional => vec![tc, sc, ja, ko],
        "zh" => vec![sc, tc, ja, ko],
        _ => vec![sc, ja, ko, tc],
    };
    let root = if cfg!(windows) {
        std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("Fonts")
    } else {
        PathBuf::new()
    };
    let mut out: Vec<PathBuf> = Vec::new();
    for name in order.into_iter().flatten().chain(sym.iter()) {
        let p = root.join(name);
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// The fallback faces for `locale` that exist and parse (at most `max`), with their paths. Reads
/// font files (CJK files are 10–20 MB): call it off the engine thread, once.
pub fn load_fallbacks(locale: &str, max: usize) -> Vec<(PathBuf, Font)> {
    system_fallback_candidates(locale).into_iter().filter(|p| p.is_file()).filter_map(|p| load_font_file(&p).ok().map(|f| (p, f))).take(max).collect()
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

    #[test]
    fn fallbacks_follow_the_locale() {
        let ja = system_fallback_candidates("ja-JP");
        let zh = system_fallback_candidates("zh-Hans");
        let tw = system_fallback_candidates("zh-TW");
        assert!(!ja.is_empty() && !zh.is_empty() && !tw.is_empty());
        assert_ne!(ja[0], zh[0], "Japanese text looks for a Japanese face first");
        assert_ne!(zh[0], tw[0], "Simplified and Traditional Chinese differ");
        let mut sorted = ja.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ja.len(), "no duplicates");
    }
}
