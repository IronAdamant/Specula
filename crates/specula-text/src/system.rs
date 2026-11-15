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

/// The four families of CJK faces a locale orders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CjkScript {
    Simplified,
    Japanese,
    Korean,
    Traditional,
}

impl CjkScript {
    /// The order `locale` (a BCP 47 tag such as `ja`, `zh-Hans`, `zh-TW`, `ko`, `en`) looks for
    /// them in. It matters because Chinese, Japanese and Korean share code points (Han
    /// unification) but not glyph shapes: Japanese text should find a Japanese face first.
    /// Other locales get Simplified Chinese, Japanese, Korean, Traditional Chinese.
    pub fn order(locale: &str) -> [CjkScript; 4] {
        use CjkScript::{Japanese, Korean, Simplified, Traditional};
        let lang = locale.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
        let region = locale.to_ascii_lowercase();
        let traditional = region.contains("hant") || region.ends_with("-tw") || region.ends_with("-hk") || region.ends_with("-mo") || region.ends_with("_tw");
        match lang.as_str() {
            "ja" => [Japanese, Simplified, Korean, Traditional],
            "ko" => [Korean, Simplified, Japanese, Traditional],
            "zh" if traditional => [Traditional, Simplified, Japanese, Korean],
            "zh" => [Simplified, Traditional, Japanese, Korean],
            _ => [Simplified, Japanese, Korean, Traditional],
        }
    }

    /// This platform's font files for the family, best first: file names in the system's font
    /// folder on Windows, whole paths elsewhere. Every family has at least one on every
    /// platform. (macOS keeps Simplified and Traditional Chinese in one collection file.)
    pub fn files(self) -> &'static [&'static str] {
        use CjkScript::{Japanese, Korean, Simplified, Traditional};
        if cfg!(windows) {
            match self {
                Simplified => &["msyh.ttc", "simsun.ttc"],
                Japanese => &["YuGothM.ttc", "meiryo.ttc", "msgothic.ttc"],
                Korean => &["malgun.ttf", "gulim.ttc"],
                Traditional => &["msjh.ttc", "mingliu.ttc"],
            }
        } else if cfg!(target_os = "macos") {
            match self {
                Simplified => &["/System/Library/Fonts/PingFang.ttc", "/System/Library/Fonts/STHeiti Light.ttc"],
                Japanese => &["/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", "/System/Library/Fonts/Hiragino Sans GB.ttc"],
                Korean => &["/System/Library/Fonts/AppleSDGothicNeo.ttc"],
                Traditional => &["/System/Library/Fonts/PingFang.ttc"],
            }
        } else {
            match self {
                Simplified => &[
                    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
                    "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
                    "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
                    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
                ],
                Japanese => &["/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf", "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf"],
                Korean => &["/usr/share/fonts/truetype/nanum/NanumGothic.ttf"],
                // (Debian's and Ubuntu's fonts-arphic-bsmi00lp and fonts-arphic-uming. The list
                // was empty until CI first ran these tests on Linux, 2026-11-15: Traditional
                // Chinese then looked for the Simplified faces first.)
                Traditional => &["/usr/share/fonts/truetype/arphic-bsmi00lp/bsmi00lp.ttf", "/usr/share/fonts/truetype/arphic/uming.ttc"],
            }
        }
    }
}

/// This platform's symbol face: what ends [`system_fallback_candidates`].
fn symbol_files() -> &'static [&'static str] {
    if cfg!(windows) {
        &["seguisym.ttf"]
    } else if cfg!(target_os = "macos") {
        &["/System/Library/Fonts/Apple Symbols.ttf"]
    } else {
        &["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"]
    }
}

/// Where a font file named by [`CjkScript::files`] is.
fn font_path(name: &str) -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("Fonts").join(name)
    } else {
        PathBuf::from(name)
    }
}

fn paths_of<'a>(names: impl Iterator<Item = &'a &'static str>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for name in names {
        let p = font_path(name);
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// The system's CJK font files for `locale`, best first ([`CjkScript::order`]), and nothing
/// else: what "this locale's face" is looked for in. Files may be missing or use CFF outlines
/// (Noto CJK on Linux): [`load_cjk_face`] skips what doesn't load. The bundled faces cover
/// Latin, Cyrillic and Greek themselves ([`crate::Bundled`]).
pub fn system_cjk_candidates(locale: &str) -> Vec<PathBuf> {
    paths_of(CjkScript::order(locale).iter().flat_map(|s| s.files()))
}

/// Fallback font files for `locale`, best first: its CJK faces ([`system_cjk_candidates`]),
/// then a symbol face. **The last one is not a face for the locale**: a machine with no CJK
/// font still has it, so what wants the locale's face asks [`load_cjk_face`] (taking the first
/// of these that loads gave such a machine its symbol face as its Japanese one, with every
/// character missing and nothing said).
pub fn system_fallback_candidates(locale: &str) -> Vec<PathBuf> {
    paths_of(CjkScript::order(locale).iter().flat_map(|s| s.files()).chain(symbol_files()))
}

/// The fallback faces for `locale` that exist and parse (at most `max`), with their paths: its
/// CJK faces, then the symbol face. Reads font files (CJK files are 10–20 MB): call it off the
/// engine thread, once.
pub fn load_fallbacks(locale: &str, max: usize) -> Vec<(PathBuf, Font)> {
    system_fallback_candidates(locale).into_iter().filter(|p| p.is_file()).filter_map(|p| load_font_file(&p).ok().map(|f| (p, f))).take(max).collect()
}

/// The first of `locale`'s CJK faces that exists and parses, with its path; `None` on a
/// machine that has none (never the symbol face). Reads a font file of 10–20 MB: off the engine
/// thread, once.
pub fn load_cjk_face(locale: &str) -> Option<(PathBuf, Font)> {
    first_face_with(system_cjk_candidates(locale), "")
}

/// What an interface draws in every language beside the language's own script: the digits,
/// Latin letters and punctuation of ASCII (every printable character but the space).
pub const INTERFACE_ASCII: &str = "!\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";

/// [`load_cjk_face`] for a face that draws `also` as well: the first of `locale`'s CJK faces
/// that exists, parses and has a glyph for every character of `also`. An interface's face is
/// one face for all it shows, and its Japanese still has digits and Latin words in it
/// ([`INTERFACE_ASCII`]); some CJK faces are fallback faces with no Latin at all (Droid Sans
/// Fallback, the first one found on some Linux machines: an outside review's run, 2026-11-15,
/// where every digit of the HUD was a missing glyph). Each face tried is a file of 10–20 MB
/// read: off the engine thread.
pub fn load_cjk_face_with(locale: &str, also: &str) -> Option<(PathBuf, Font)> {
    first_face_with(system_cjk_candidates(locale), also)
}

/// The first of `candidates` that is a file, parses and has a glyph for every character of
/// `also`, with its path.
pub fn first_face_with(candidates: impl IntoIterator<Item = PathBuf>, also: &str) -> Option<(PathBuf, Font)> {
    candidates.into_iter().filter(|p| p.is_file()).filter_map(|p| load_font_file(&p).ok().map(|f| (p, f))).find(|(_, f)| also.chars().all(|c| f.glyph_index(c).is_some()))
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

    /// What the order is, said of families, so it holds on every platform (CI's Linux job was
    /// the first to run this off Windows, 2026-11-15: "the first files differ" was true of
    /// Windows' font names only).
    #[test]
    fn fallbacks_follow_the_locale() {
        use CjkScript::{Japanese, Korean, Simplified, Traditional};
        for (locale, first) in [("ja-JP", Japanese), ("ja", Japanese), ("ko", Korean), ("zh-Hans", Simplified), ("zh", Simplified), ("zh-TW", Traditional), ("zh_TW", Traditional), ("zh-Hant", Traditional), ("zh-HK", Traditional), ("en", Simplified), ("", Simplified)] {
            let order = CjkScript::order(locale);
            assert_eq!(order[0], first, "{locale}");
            for script in [Simplified, Japanese, Korean, Traditional] {
                assert_eq!(order.iter().filter(|s| **s == script).count(), 1, "{locale}: every family once");
            }
            // The files are the families' in that order, each once; the first is the first
            // family's first file.
            let cjk = system_cjk_candidates(locale);
            assert_eq!(cjk[0], font_path(first.files()[0]), "{locale}");
            let mut sorted = cjk.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), cjk.len(), "{locale}: no duplicates");
            // The whole chain is those and then the symbol face, which is never among the
            // locale's own.
            let symbol = font_path(symbol_files()[0]);
            let all = system_fallback_candidates(locale);
            assert_eq!(all[..cjk.len()], cjk[..], "{locale}");
            assert_eq!(&all[cjk.len()..], std::slice::from_ref(&symbol), "{locale}");
            assert!(!cjk.contains(&symbol), "{locale}");
        }
        // Every family has a file on every platform (Linux had none for Traditional Chinese).
        for script in [Simplified, Japanese, Korean, Traditional] {
            assert!(!script.files().is_empty(), "{script:?}");
        }
        // Where a platform keeps the two Chinese families in files of their own, the two
        // locales start with different files.
        if Simplified.files()[0] != Traditional.files()[0] {
            assert_ne!(system_cjk_candidates("zh-Hans")[0], system_cjk_candidates("zh-TW")[0]);
        }
        assert_ne!(system_cjk_candidates("ja")[0], system_cjk_candidates("zh-Hans")[0], "Japanese text looks for a Japanese face first");
    }

    /// A face is taken for what it has to draw: the first in order when nothing more is asked,
    /// and the first that has the rest when more is. (Files made here, so the answer is the
    /// same on every machine: a font with three letters, and the bundled interface face.)
    #[test]
    fn a_face_that_lacks_what_else_it_draws_is_passed_over() {
        let dir = std::env::temp_dir().join(format!("specula-text-faces-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (tiny, full, none, broken) = (dir.join("tiny.ttf"), dir.join("full.ttf"), dir.join("none.ttf"), dir.join("broken.ttf"));
        std::fs::write(&tiny, crate::font::tests::tiny_font()).unwrap();
        std::fs::write(&full, crate::Bundled::UiRegular.bytes()).unwrap();
        std::fs::write(&broken, b"not a font").unwrap();
        let first = |files: &[&PathBuf], also: &str| first_face_with(files.iter().map(|p| (*p).clone()), also).map(|(p, _)| p);
        // In order; a file that is not there or is no font is passed over, as ever.
        assert_eq!(first(&[&none, &broken, &tiny, &full], ""), Some(tiny.clone()));
        assert_eq!(first(&[&tiny, &full], "A"), Some(tiny.clone()), "the tiny font has an A");
        // It has no digit: an interface's face is the next one.
        assert_eq!(first(&[&tiny, &full], INTERFACE_ASCII), Some(full.clone()));
        assert_eq!(first(&[&tiny], INTERFACE_ASCII), None);
        assert_eq!(first(&[&tiny, &full], "A\u{e000}"), None, "a character neither has");
        assert_eq!(first(&[], ""), None);
        assert_eq!(INTERFACE_ASCII.chars().map(u32::from).collect::<Vec<_>>(), (0x21..=0x7e).collect::<Vec<u32>>(), "every printable ASCII character but the space, once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The locale's face is one of its CJK files or nothing: never the symbol face that ends
    /// the chain (which a machine with no CJK font has, as CI's Linux image does).
    #[test]
    fn a_locales_face_is_never_the_symbol_face() {
        let symbol = font_path(symbol_files()[0]);
        for locale in ["ja", "zh-TW", "ko", "en"] {
            match load_cjk_face(locale) {
                Some((path, _)) => assert!(system_cjk_candidates(locale).contains(&path) && path != symbol, "{locale}: {}", path.display()),
                // (A machine with no CJK font: nothing is the answer.)
                None => assert!(!system_cjk_candidates(locale).iter().any(|p| p.is_file() && load_font_file(p).is_ok()), "{locale}"),
            }
        }
    }
}
