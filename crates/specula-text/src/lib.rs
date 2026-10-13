//! Specula text: TrueType fonts → glyph atlas → laid-out quads, all on the CPU, zero dependencies.
//!
//! The pipeline (design: `docs/design/UI.md`, stages T1–T3):
//!
//! 1. [`Font`] parses a TrueType / OpenType font with `glyf` outlines from bytes: metrics,
//!    `cmap` (formats 4 and 12), `kern` format 0 and GPOS pair kerning, family name. Every read is
//!    bounds-checked and malformed fonts produce a structured [`FontError`] (table, byte offset,
//!    reason), never a panic. CFF outlines are reported as an unsupported outline format.
//! 2. [`Rasterizer`] turns outlines into anti-aliased coverage by exact signed-area accumulation
//!    (non-zero winding, quarter-pixel horizontal positioning); [`rasterize_sdf`] makes signed
//!    distance fields for scalable text.
//! 3. [`GlyphAtlas`] packs R8 glyph bitmaps into pages (skyline packing, padding, LRU page
//!    eviction) and reports [`DirtyRect`]s so the GPU uploads only what changed.
//! 4. [`layout`] turns UTF-8 into wrapped, aligned lines of positioned glyphs;
//!    [`TextLayout::quads`] turns those into [`GlyphQuad`]s (`rect`, `uv`, `page`, `color`) that
//!    an overlay renderer draws directly.
//!
//! Typical per-frame use:
//!
//! ```text
//! let font = Font::from_bytes(std::fs::read("ui.ttf")?)?;
//! let mut atlas = GlyphAtlas::new(AtlasConfig::default());
//! // every frame:
//! atlas.begin_frame();
//! let text = layout(&font, "Wave 3/10", &TextStyle::new(14.0).with_scale(ui_scale));
//! text.append_quads(&font, &mut atlas, [x, y], [0.93, 0.90, 0.84, 1.0], &mut quads)?;
//! for r in atlas.take_dirty() { /* upload r from atlas.page_pixels(r.page) */ }
//! ```
//!
//! Conventions: pixels are physical, origin top-left, y down; colours are sRGB, straight alpha.

pub mod atlas;
pub mod error;
pub mod font;
pub mod layout;
pub mod outline;
pub mod raster;
mod reader;
pub mod system;

#[cfg(test)]
mod fuzz_tests;

pub use atlas::{AtlasConfig, AtlasGlyph, AtlasStats, DirtyRect, GlyphAtlas, GlyphKey, SUBPIXEL_BINS};
pub use error::{AtlasError, FontError, Reason, Tag};
pub use font::{font_count, Font, FontId, GlyphId, LineMetrics};
pub use layout::{layout, layout_into, measure, Align, GlyphQuad, Line, PositionedGlyph, TextLayout, TextMetrics, TextStyle};
pub use outline::{Outline, Point, Segment};
pub use raster::{rasterize_sdf, Bitmap, Rasterizer};
pub use system::{find_system_ui_font, load_font_file, system_ui_font_candidates, LoadError};
