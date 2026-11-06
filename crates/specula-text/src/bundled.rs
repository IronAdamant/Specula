//! The fonts Specula ships (SIL OFL 1.1, owner 2026-10-22), compiled into the binary so every
//! game and engine build has the same faces on every platform, with no system font lookup.
//!
//! - **Alegreya Sans** Regular and Bold: the UI face (clean humanist sans, legible at Deck sizes).
//! - **Alegreya**: the display serif for titles and carved inscriptions (Sepulcra's STYLE.md:
//!   "decorative serif only for titles"). A variable font, drawn at its default instance.
//!
//! All three cover Latin, Cyrillic and Greek (with their extended sets) and Vietnamese. Where each
//! file came from, its version, size and SHA-256 are in `fonts/MANIFEST.toml` ([`MANIFEST`]);
//! the licence texts sit beside the files ([`licence`]). CJK has no bundled face (a pan-CJK font
//! is 10–20 MB): [`crate::FontStack`] falls back to the system's CJK fonts
//! ([`crate::system::system_fallback_candidates`]).

use crate::font::Font;
use crate::error::FontError;

/// The licence manifest (TOML): one `[[font]]` table per bundled file.
pub const MANIFEST: &str = include_str!("../fonts/MANIFEST.toml");

/// A bundled face.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Bundled {
    /// Alegreya Sans Regular: interface text.
    UiRegular,
    /// Alegreya Sans Bold: emphasis in interface text (rich text `bold`).
    UiBold,
    /// Alegreya (serif): titles, inscriptions.
    Display,
}

impl Bundled {
    pub const ALL: [Bundled; 3] = [Bundled::UiRegular, Bundled::UiBold, Bundled::Display];

    /// The name agents use (`text` components, rich text, `text_*` tools).
    pub fn name(self) -> &'static str {
        match self {
            Bundled::UiRegular => "ui",
            Bundled::UiBold => "ui-bold",
            Bundled::Display => "display",
        }
    }

    pub fn from_name(name: &str) -> Option<Bundled> {
        Bundled::ALL.into_iter().find(|b| b.name() == name)
    }

    /// The file under `fonts/` (as the manifest names it).
    pub fn file(self) -> &'static str {
        match self {
            Bundled::UiRegular => "alegreya-sans/AlegreyaSans-Regular.ttf",
            Bundled::UiBold => "alegreya-sans/AlegreyaSans-Bold.ttf",
            Bundled::Display => "alegreya/Alegreya-VF.ttf",
        }
    }

    /// The font file's bytes.
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Bundled::UiRegular => include_bytes!("../fonts/alegreya-sans/AlegreyaSans-Regular.ttf"),
            Bundled::UiBold => include_bytes!("../fonts/alegreya-sans/AlegreyaSans-Bold.ttf"),
            Bundled::Display => include_bytes!("../fonts/alegreya/Alegreya-VF.ttf"),
        }
    }

    /// Parses the face (a fresh [`Font`] with its own id; parse once and keep it).
    pub fn load(self) -> Result<Font, FontError> {
        Font::from_bytes(self.bytes().to_vec())
    }
}

/// The licence text shipped with `font` (the SIL Open Font License 1.1 with the copyright line).
pub fn licence(font: Bundled) -> &'static str {
    match font {
        Bundled::UiRegular | Bundled::UiBold => include_str!("../fonts/alegreya-sans/OFL.txt"),
        Bundled::Display => include_str!("../fonts/alegreya/OFL.txt"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_face_parses_and_is_named_once() {
        for b in Bundled::ALL {
            let f = b.load().unwrap_or_else(|e| panic!("{b:?}: {e}"));
            assert!(f.num_glyphs() > 500, "{b:?}");
            assert_eq!(Bundled::from_name(b.name()), Some(b));
            assert!(MANIFEST.contains(&format!("file = \"{}\"", b.file())), "{b:?} missing from the manifest");
            assert!(licence(b).contains("SIL Open Font License, Version 1.1"));
        }
        assert_eq!(Bundled::from_name("comic"), None);
    }
}
