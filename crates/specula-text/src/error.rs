//! Structured errors. Every font error names the table, the byte offset in the font file and a
//! machine-readable reason, so an agent can debug a bad font from the data alone.

use std::fmt;

/// A four-byte OpenType table tag such as `glyf` or `cmap`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tag(pub [u8; 4]);

impl Tag {
    /// The pseudo-table used for errors in the file header / table directory.
    pub const HEADER: Tag = Tag(*b"sfnt");

    pub const fn new(bytes: &[u8; 4]) -> Tag {
        Tag(*bytes)
    }

    /// The tag as text; non-printable bytes become `?`.
    pub fn as_string(&self) -> String {
        self.0.iter().map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '?' }).collect()
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tag({:?})", self.as_string())
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_string())
    }
}

/// Why a font could not be parsed (or a glyph could not be read).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// A read of `needed` bytes ran past the end of the table or file.
    Truncated { needed: usize, available: usize },
    /// A required table is missing.
    MissingTable,
    /// The file is not a TrueType / OpenType font or collection.
    BadMagic { found: u32 },
    /// The font uses an outline format this crate does not rasterise (CFF / CFF2).
    UnsupportedOutline { format: &'static str },
    /// A table version or subtable format this crate does not understand.
    UnsupportedFormat { format: u32 },
    /// A field holds a value that is out of range or inconsistent with other fields.
    BadValue { field: &'static str, value: i64 },
    /// A collection index was out of range.
    FontIndex { index: u32, count: u32 },
    /// A glyph id was out of range (`>= num_glyphs`).
    GlyphId { glyph: u32, num_glyphs: u32 },
    /// Composite glyphs nest deeper than the limit (cycles are reported this way too).
    CompositeTooDeep { depth: u32 },
    /// A glyph expands to more outline points than the limit.
    TooManyPoints { points: usize },
    /// A table asks for more work than a sane font needs (`what` exceeds `limit`).
    LimitExceeded { what: &'static str, limit: usize },
}

impl Reason {
    /// A stable machine-readable code (`snake_case`).
    pub fn code(&self) -> &'static str {
        match self {
            Reason::Truncated { .. } => "truncated",
            Reason::MissingTable => "missing_table",
            Reason::BadMagic { .. } => "bad_magic",
            Reason::UnsupportedOutline { .. } => "unsupported_outline_format",
            Reason::UnsupportedFormat { .. } => "unsupported_format",
            Reason::BadValue { .. } => "bad_value",
            Reason::FontIndex { .. } => "font_index",
            Reason::GlyphId { .. } => "glyph_id",
            Reason::CompositeTooDeep { .. } => "composite_too_deep",
            Reason::TooManyPoints { .. } => "too_many_points",
            Reason::LimitExceeded { .. } => "limit_exceeded",
        }
    }
}

/// A font parsing error: which table, where in the file, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontError {
    pub table: Tag,
    /// Byte offset from the start of the font file (or collection) where the problem was found.
    pub offset: usize,
    pub reason: Reason,
}

impl FontError {
    pub(crate) fn new(table: Tag, offset: usize, reason: Reason) -> FontError {
        FontError { table, offset, reason }
    }
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "font error in table '{}' at byte {}: ", self.table, self.offset)?;
        match &self.reason {
            Reason::Truncated { needed, available } => {
                write!(f, "truncated (needed {needed} bytes, {available} available)")
            }
            Reason::MissingTable => write!(f, "required table is missing"),
            Reason::BadMagic { found } => write!(f, "not a TrueType/OpenType font (magic 0x{found:08x})"),
            Reason::UnsupportedOutline { format } => {
                write!(f, "unsupported outline format {format} (only TrueType 'glyf' outlines are supported)")
            }
            Reason::UnsupportedFormat { format } => write!(f, "unsupported table/subtable format {format}"),
            Reason::BadValue { field, value } => write!(f, "bad value {value} for {field}"),
            Reason::FontIndex { index, count } => write!(f, "font index {index} out of range (collection has {count})"),
            Reason::GlyphId { glyph, num_glyphs } => write!(f, "glyph id {glyph} out of range ({num_glyphs} glyphs)"),
            Reason::CompositeTooDeep { depth } => write!(f, "composite glyph nesting exceeds {depth} (cycle?)"),
            Reason::TooManyPoints { points } => write!(f, "glyph expands to more than {points} points"),
            Reason::LimitExceeded { what, limit } => write!(f, "{what} exceed the limit of {limit}"),
        }
    }
}

impl std::error::Error for FontError {}

/// Glyph atlas errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AtlasError {
    /// Every page is full and none can be evicted (all were used this frame) or eviction is off.
    Full { pages: usize, page_size: u32, glyph_w: u32, glyph_h: u32 },
    /// The glyph bitmap (plus padding) is larger than a whole page.
    GlyphTooLarge { glyph_w: u32, glyph_h: u32, page_size: u32 },
    /// The glyph's outline could not be read.
    Font(FontError),
}

impl AtlasError {
    /// A stable machine-readable code (`snake_case`).
    pub fn code(&self) -> &'static str {
        match self {
            AtlasError::Full { .. } => "atlas_full",
            AtlasError::GlyphTooLarge { .. } => "glyph_too_large",
            AtlasError::Font(e) => e.reason.code(),
        }
    }
}

impl fmt::Display for AtlasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AtlasError::Full { pages, page_size, glyph_w, glyph_h } => write!(
                f,
                "glyph atlas full: {pages} pages of {page_size}x{page_size} px, no room for a {glyph_w}x{glyph_h} glyph"
            ),
            AtlasError::GlyphTooLarge { glyph_w, glyph_h, page_size } => {
                write!(f, "glyph bitmap {glyph_w}x{glyph_h} px does not fit a {page_size}x{page_size} atlas page")
            }
            AtlasError::Font(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AtlasError {}

impl From<FontError> for AtlasError {
    fn from(e: FontError) -> AtlasError {
        AtlasError::Font(e)
    }
}
