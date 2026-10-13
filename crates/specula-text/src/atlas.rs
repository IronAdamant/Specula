//! Glyph atlas: R8 coverage pages packed with a skyline (bottom-left) packer, a cache keyed by
//! (font, glyph, size, sub-pixel bin), and dirty rectangles so the GPU uploads only what changed.
//!
//! **Eviction** is per page, least-recently-used: when every page is full and the page limit is
//! reached, the page least recently used *before the current frame* is cleared and reused. Call
//! [`GlyphAtlas::begin_frame`] once per frame; quads produced during the current frame are never
//! invalidated by eviction. If every page was used this frame, [`AtlasError::Full`] is returned.

use crate::error::AtlasError;
use crate::font::{Font, FontId, GlyphId};
use crate::raster::Rasterizer;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Horizontal sub-pixel positions per pixel (quarter-pixel positioning).
pub const SUBPIXEL_BINS: u8 = 4;

/// Atlas configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasConfig {
    /// Side of each square page in pixels.
    pub page_size: u32,
    /// Maximum number of pages (memory cap = `page_size² * max_pages` bytes).
    pub max_pages: usize,
    /// Empty pixels kept around every glyph (1–2 px; stops bilinear bleed).
    pub padding: u32,
    /// Reuse the least-recently-used page when full (otherwise report [`AtlasError::Full`]).
    pub evict: bool,
}

impl Default for AtlasConfig {
    /// 1024² pages, at most 4 (4 MiB), 1 px padding, eviction on.
    fn default() -> AtlasConfig {
        AtlasConfig { page_size: 1024, max_pages: 4, padding: 1, evict: true }
    }
}

/// Cache key. `size_q` is the pixel size in 1/64 px; `subpixel` is the horizontal bin (0..4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub font: FontId,
    pub glyph: GlyphId,
    pub size_q: u32,
    pub subpixel: u8,
}

impl GlyphKey {
    pub fn new(font: &Font, glyph: GlyphId, size_px: f32, subpixel: u8) -> GlyphKey {
        GlyphKey { font: font.id(), glyph, size_q: quantize_size(size_px), subpixel: subpixel % SUBPIXEL_BINS }
    }
}

/// Sizes are cached in 1/64 px steps (and clamped to 0..=1024 px).
pub fn quantize_size(size_px: f32) -> u32 {
    if size_px.is_finite() {
        (size_px.clamp(0.0, 1024.0) * 64.0).round() as u32
    } else {
        0
    }
}

/// `AtlasGlyph::page` of ink-less glyphs (space, or a glyph whose outline could not be read).
pub const NO_PAGE: u32 = u32::MAX;

/// Ink-less cache entries are dropped once there are more than this many (e.g. after a size
/// tween); they cost nothing to recreate.
const MAX_EMPTY_ENTRIES: usize = 4096;

const EMPTY_GLYPH: AtlasGlyph = AtlasGlyph { page: NO_PAGE, x: 0, y: 0, width: 0, height: 0, left: 0, top: 0 };

/// Where a glyph lives in the atlas and how to place it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasGlyph {
    /// Atlas page; [`NO_PAGE`] for glyphs with no ink.
    pub page: u32,
    /// Top-left texel of the glyph bitmap in the page (padding excluded).
    pub x: u32,
    pub y: u32,
    /// Bitmap size; 0 for glyphs with no ink (space).
    pub width: u32,
    pub height: u32,
    /// Bitmap offset from the integer pen position / baseline, y down.
    pub left: i32,
    pub top: i32,
}

/// A rectangle of a page that changed since the last [`GlyphAtlas::take_dirty`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyRect {
    pub page: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Cache counters (cumulative).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AtlasStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    /// Glyphs whose outline could not be read (skipped by `TextLayout::append_quads`).
    pub font_errors: u64,
    pub glyphs: usize,
}

#[derive(Clone, Copy, Debug)]
struct Node {
    x: u32,
    y: u32,
    w: u32,
}

struct Page {
    pixels: Vec<u8>,
    skyline: Vec<Node>,
    /// Union of changed texels: `[x0, y0, x1, y1)`.
    dirty: Option<[u32; 4]>,
    last_used: u64,
}

impl Page {
    fn new(size: u32) -> Page {
        Page {
            pixels: vec![0; (size as usize) * (size as usize)],
            skyline: vec![Node { x: 0, y: 0, w: size }],
            dirty: Some([0, 0, size, size]),
            last_used: 0,
        }
    }

    fn clear(&mut self, size: u32) {
        self.pixels.fill(0);
        self.skyline.clear();
        self.skyline.push(Node { x: 0, y: 0, w: size });
        self.dirty = Some([0, 0, size, size]);
    }

    fn mark_dirty(&mut self, x: u32, y: u32, w: u32, h: u32) {
        let r = [x, y, x + w, y + h];
        self.dirty = Some(match self.dirty {
            None => r,
            Some(d) => [d[0].min(r[0]), d[1].min(r[1]), d[2].max(r[2]), d[3].max(r[3])],
        });
    }

    /// The lowest y at which a `w x h` rect fits with its left edge at node `i`.
    fn fit(&self, i: usize, w: u32, h: u32, size: u32) -> Option<u32> {
        let x = self.skyline[i].x;
        if x + w > size {
            return None;
        }
        let mut left = w;
        let mut j = i;
        let mut y = 0;
        loop {
            let n = self.skyline.get(j)?;
            y = y.max(n.y);
            if y + h > size {
                return None;
            }
            if n.w >= left {
                return Some(y);
            }
            left -= n.w;
            j += 1;
        }
    }

    /// Bottom-left skyline allocation of a `w x h` rect.
    fn alloc(&mut self, w: u32, h: u32, size: u32) -> Option<(u32, u32)> {
        let mut best: Option<(usize, u32, u32)> = None; // (node, y, node width)
        for i in 0..self.skyline.len() {
            if let Some(y) = self.fit(i, w, h, size) {
                let nw = self.skyline[i].w;
                let better = match best {
                    None => true,
                    Some((_, by, bw)) => y + h < by + h || (y == by && nw < bw),
                };
                if better {
                    best = Some((i, y, nw));
                }
            }
        }
        let (i, y, _) = best?;
        let x = self.skyline[i].x;
        self.skyline.insert(i, Node { x, y: y + h, w });
        // Trim the nodes now covered by the new one.
        let j = i + 1;
        while j < self.skyline.len() {
            let prev_end = self.skyline[j - 1].x + self.skyline[j - 1].w;
            let n = &mut self.skyline[j];
            if n.x >= prev_end {
                break;
            }
            let shrink = prev_end - n.x;
            if n.w <= shrink {
                self.skyline.remove(j);
            } else {
                n.x += shrink;
                n.w -= shrink;
                break;
            }
        }
        // Merge neighbours at the same height.
        let mut k = 0;
        while k + 1 < self.skyline.len() {
            if self.skyline[k].y == self.skyline[k + 1].y {
                self.skyline[k].w += self.skyline[k + 1].w;
                self.skyline.remove(k + 1);
            } else {
                k += 1;
            }
        }
        Some((x, y))
    }
}

/// A fast hasher for small fixed-size keys (multiply-rotate, FxHash-style). Not DoS-resistant,
/// which is fine for keys the engine itself produces.
#[derive(Default)]
pub struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(b as u64);
        }
    }
    fn write_u8(&mut self, v: u8) {
        self.write_u64(v as u64);
    }
    fn write_u16(&mut self, v: u16) {
        self.write_u64(v as u64);
    }
    fn write_u32(&mut self, v: u32) {
        self.write_u64(v as u64);
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
    fn write_usize(&mut self, v: usize) {
        self.write_u64(v as u64);
    }
}

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<KeyHasher>>;

/// The glyph atlas. See the module docs.
pub struct GlyphAtlas {
    config: AtlasConfig,
    pages: Vec<Page>,
    cache: FastMap<GlyphKey, AtlasGlyph>,
    raster: Rasterizer,
    frame: u64,
    stats: AtlasStats,
    empty_entries: usize,
}

impl GlyphAtlas {
    pub fn new(config: AtlasConfig) -> GlyphAtlas {
        let config = AtlasConfig {
            page_size: config.page_size.clamp(16, 8192),
            max_pages: config.max_pages.max(1),
            padding: config.padding.min(8),
            evict: config.evict,
        };
        GlyphAtlas {
            config,
            pages: Vec::new(),
            cache: FastMap::default(),
            raster: Rasterizer::new(),
            frame: 1,
            stats: AtlasStats::default(),
            empty_entries: 0,
        }
    }

    pub fn config(&self) -> AtlasConfig {
        self.config
    }

    pub fn page_size(&self) -> u32 {
        self.config.page_size
    }

    /// Pages allocated so far. A new page appears fully dirty (upload it whole once).
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// R8 texels of a page, `page_size²` bytes, row-major.
    pub fn page_pixels(&self, page: usize) -> Option<&[u8]> {
        self.pages.get(page).map(|p| p.pixels.as_slice())
    }

    pub fn stats(&self) -> AtlasStats {
        AtlasStats { glyphs: self.cache.len(), ..self.stats }
    }

    /// Start a new frame (for LRU eviction).
    pub fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// Drop every glyph and page.
    pub fn clear(&mut self) {
        self.pages.clear();
        self.cache.clear();
        self.empty_entries = 0;
    }

    /// Changed regions since the last call, one union rectangle per changed page.
    pub fn take_dirty(&mut self) -> Vec<DirtyRect> {
        let mut out = Vec::new();
        for (i, p) in self.pages.iter_mut().enumerate() {
            if let Some(d) = p.dirty.take() {
                out.push(DirtyRect { page: i as u32, x: d[0], y: d[1], width: d[2] - d[0], height: d[3] - d[1] });
            }
        }
        out
    }

    /// True if any page has changes not yet taken.
    pub fn is_dirty(&self) -> bool {
        self.pages.iter().any(|p| p.dirty.is_some())
    }

    /// Copy a rectangle of a page into a tightly packed buffer (for a texture upload).
    pub fn copy_rect(&self, r: &DirtyRect, out: &mut Vec<u8>) {
        out.clear();
        let Some(page) = self.pages.get(r.page as usize) else { return };
        let size = self.config.page_size;
        let x1 = r.x.saturating_add(r.width).min(size);
        let y1 = r.y.saturating_add(r.height).min(size);
        for y in r.y.min(size)..y1 {
            let row = (y * size) as usize;
            out.extend_from_slice(&page.pixels[row + r.x.min(size) as usize..row + x1 as usize]);
        }
    }

    /// Look up a glyph, rasterising and packing it on a miss.
    /// `subpixel` is the horizontal bin `0..SUBPIXEL_BINS` (offset `subpixel / 4` px).
    pub fn glyph(&mut self, font: &Font, glyph: GlyphId, size_px: f32, subpixel: u8) -> Result<AtlasGlyph, AtlasError> {
        let key = GlyphKey::new(font, glyph, size_px, subpixel);
        if let Some(g) = self.cache.get(&key) {
            self.stats.hits += 1;
            // Ink-less glyphs (NO_PAGE) touch no page, so they never pin one against eviction.
            if let Some(p) = self.pages.get_mut(g.page as usize) {
                p.last_used = self.frame;
            }
            return Ok(*g);
        }
        self.stats.misses += 1;
        let size = key.size_q as f32 / 64.0;
        let offset = key.subpixel as f32 / SUBPIXEL_BINS as f32;
        let bmp = match font
            .glyph_outline(glyph)
            .and_then(|outline| self.raster.rasterize(&outline, font.scale_for_px(size), offset))
        {
            Ok(b) => b,
            Err(e) => {
                // Reported once; afterwards the glyph is a cached ink-less entry.
                self.stats.font_errors += 1;
                self.insert_empty(key);
                return Err(AtlasError::Font(e));
            }
        };
        if bmp.is_empty() {
            self.insert_empty(key);
            return Ok(EMPTY_GLYPH);
        }
        let pad = self.config.padding;
        let (w, h) = (bmp.width + 2 * pad, bmp.height + 2 * pad);
        let ps = self.config.page_size;
        if w > ps || h > ps {
            return Err(AtlasError::GlyphTooLarge { glyph_w: bmp.width, glyph_h: bmp.height, page_size: ps });
        }
        let (page, x, y) = self.allocate(w, h)?;
        let p = &mut self.pages[page];
        let (gx, gy) = (x + pad, y + pad);
        for row in 0..bmp.height {
            let src = &bmp.pixels[(row * bmp.width) as usize..((row + 1) * bmp.width) as usize];
            let dst = ((gy + row) * ps + gx) as usize;
            p.pixels[dst..dst + bmp.width as usize].copy_from_slice(src);
        }
        p.mark_dirty(gx, gy, bmp.width, bmp.height);
        p.last_used = self.frame;
        let g = AtlasGlyph {
            page: page as u32,
            x: gx,
            y: gy,
            width: bmp.width,
            height: bmp.height,
            left: bmp.left,
            top: bmp.top,
        };
        self.cache.insert(key, g);
        Ok(g)
    }

    fn insert_empty(&mut self, key: GlyphKey) {
        if self.empty_entries >= MAX_EMPTY_ENTRIES {
            self.cache.retain(|_, g| g.page != NO_PAGE);
            self.empty_entries = 0;
        }
        self.cache.insert(key, EMPTY_GLYPH);
        self.empty_entries += 1;
    }

    fn allocate(&mut self, w: u32, h: u32) -> Result<(usize, u32, u32), AtlasError> {
        let ps = self.config.page_size;
        // Most recent pages first: they are the emptiest.
        for i in (0..self.pages.len()).rev() {
            if let Some((x, y)) = self.pages[i].alloc(w, h, ps) {
                return Ok((i, x, y));
            }
        }
        if self.pages.len() < self.config.max_pages {
            self.pages.push(Page::new(ps));
            let i = self.pages.len() - 1;
            if let Some((x, y)) = self.pages[i].alloc(w, h, ps) {
                return Ok((i, x, y));
            }
        } else if self.config.evict {
            let frame = self.frame;
            let victim = (0..self.pages.len())
                .filter(|&i| self.pages[i].last_used < frame)
                .min_by_key(|&i| self.pages[i].last_used);
            if let Some(i) = victim {
                self.pages[i].clear(ps);
                // Drop the page's glyphs, and the ink-less entries too (cheap to recreate).
                self.cache.retain(|_, g| g.page as usize != i && g.page != NO_PAGE);
                self.empty_entries = 0;
                self.stats.evictions += 1;
                if let Some((x, y)) = self.pages[i].alloc(w, h, ps) {
                    return Ok((i, x, y));
                }
            }
        }
        Err(AtlasError::Full {
            pages: self.pages.len(),
            page_size: ps,
            glyph_w: w.saturating_sub(2 * self.config.padding),
            glyph_h: h.saturating_sub(2 * self.config.padding),
        })
    }

    /// Check the packing invariants (for tests): every cached glyph lies inside its page, and no
    /// two glyph rectangles (with padding) on the same page overlap.
    pub fn validate(&self) -> Result<(), String> {
        let ps = self.config.page_size;
        let pad = self.config.padding;
        let mut rects: Vec<(u32, [u32; 4])> = self
            .cache
            .values()
            .filter(|g| g.width > 0)
            .map(|g| (g.page, [g.x - pad, g.y - pad, g.x + g.width + pad, g.y + g.height + pad]))
            .collect();
        rects.sort_by_key(|r| (r.0, r.1));
        for (i, a) in rects.iter().enumerate() {
            if a.0 as usize >= self.pages.len() || a.1[2] > ps || a.1[3] > ps {
                return Err(format!("glyph rect {a:?} outside page"));
            }
            for b in &rects[i + 1..] {
                if b.0 == a.0 && a.1[0] < b.1[2] && b.1[0] < a.1[2] && a.1[1] < b.1[3] && b.1[1] < a.1[3] {
                    return Err(format!("overlap {a:?} {b:?}"));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::tests::tiny_font;

    #[test]
    fn skyline_packs_without_overlap() {
        let mut p = Page::new(64);
        let mut rects = Vec::new();
        let sizes = [(10, 12), (5, 5), (20, 3), (7, 9), (13, 13), (1, 1), (30, 8), (8, 30), (16, 16), (3, 17)];
        let mut k = 0;
        while let Some((x, y)) = p.alloc(sizes[k % sizes.len()].0, sizes[k % sizes.len()].1, 64) {
            let (w, h) = sizes[k % sizes.len()];
            assert!(x + w <= 64 && y + h <= 64);
            for &(ax, ay, aw, ah) in &rects {
                assert!(x >= ax + aw || ax >= x + w || y >= ay + ah || ay >= y + h, "overlap");
            }
            rects.push((x, y, w, h));
            k += 1;
        }
        let used: u32 = rects.iter().map(|r| r.2 * r.3).sum();
        assert!(used > 64 * 64 / 2, "poor packing: {used} of {}", 64 * 64);
        // The skyline stays sorted and contiguous.
        let mut x = 0;
        for n in &p.skyline {
            assert_eq!(n.x, x);
            x += n.w;
        }
        assert_eq!(x, 64);
    }

    #[test]
    fn caches_and_reports_dirty() {
        let font = Font::from_bytes(tiny_font()).unwrap();
        let mut atlas = GlyphAtlas::new(AtlasConfig { page_size: 64, ..AtlasConfig::default() });
        let a = atlas.glyph(&font, 1, 20.0, 0).unwrap();
        assert_eq!((a.width, a.height), (10, 10));
        assert_eq!(atlas.page_count(), 1);
        let d = atlas.take_dirty();
        assert_eq!(d, vec![DirtyRect { page: 0, x: 0, y: 0, width: 64, height: 64 }]);
        assert!(atlas.take_dirty().is_empty());
        // Hit: no new dirt.
        assert_eq!(atlas.glyph(&font, 1, 20.0, 0).unwrap(), a);
        assert!(!atlas.is_dirty());
        // New glyph: dirty rect is exactly the glyph.
        let b = atlas.glyph(&font, 2, 20.0, 2).unwrap();
        let d = atlas.take_dirty();
        assert_eq!(d, vec![DirtyRect { page: 0, x: b.x, y: b.y, width: b.width, height: b.height }]);
        let mut buf = Vec::new();
        atlas.copy_rect(&d[0], &mut buf);
        assert_eq!(buf.len(), (b.width * b.height) as usize);
        // Empty glyph: cached, no ink.
        assert_eq!(atlas.glyph(&font, 0, 20.0, 0).unwrap().width, 0);
        let s = atlas.stats();
        assert_eq!((s.hits, s.misses, s.glyphs), (1, 3, 3));
        atlas.validate().unwrap();
    }

    #[test]
    fn font_errors_and_empty_entries_are_bounded() {
        let font = Font::from_bytes(tiny_font()).unwrap();
        let mut atlas = GlyphAtlas::new(AtlasConfig::default());
        // An unreadable glyph is reported once, then cached as ink-less.
        assert_eq!(atlas.glyph(&font, 999, 16.0, 0).unwrap_err().code(), "glyph_id");
        assert_eq!(atlas.glyph(&font, 999, 16.0, 0).unwrap(), EMPTY_GLYPH);
        assert_eq!(atlas.stats().font_errors, 1);
        // A size tween over ink-less glyphs does not grow the cache without bound.
        for i in 0..3 * MAX_EMPTY_ENTRIES {
            atlas.glyph(&font, 0, 8.0 + i as f32 / 64.0, 0).unwrap();
        }
        assert!(atlas.stats().glyphs <= MAX_EMPTY_ENTRIES + 1, "{}", atlas.stats().glyphs);
        // copy_rect with absurd public fields neither panics nor over-reads.
        atlas.glyph(&font, 1, 16.0, 0).unwrap();
        let mut buf = Vec::new();
        atlas.copy_rect(&DirtyRect { page: 0, x: u32::MAX - 1, y: 5, width: u32::MAX, height: u32::MAX }, &mut buf);
        assert!(buf.is_empty());
        atlas.copy_rect(&DirtyRect { page: 0, x: 0, y: u32::MAX, width: 4, height: u32::MAX }, &mut buf);
        assert!(buf.is_empty());
        atlas.copy_rect(&DirtyRect { page: 7, x: 0, y: 0, width: 4, height: 4 }, &mut buf);
        assert!(buf.is_empty());
    }

    #[test]
    fn full_and_eviction() {
        let font = Font::from_bytes(tiny_font()).unwrap();
        let cfg = AtlasConfig { page_size: 32, max_pages: 1, padding: 1, evict: false };
        let mut atlas = GlyphAtlas::new(cfg);
        // 20 px glyphs are 10x10 (+2 padding): 4 fit on a 32 px page.
        let mut n = 0;
        let err = loop {
            match atlas.glyph(&font, 1, 20.0 + n as f32 / 64.0, 0) {
                Ok(_) => n += 1,
                Err(e) => break e,
            }
        };
        assert_eq!(n, 4);
        assert_eq!(err.code(), "atlas_full");
        atlas.validate().unwrap();

        let mut atlas = GlyphAtlas::new(AtlasConfig { evict: true, ..cfg });
        for i in 0..4 {
            atlas.glyph(&font, 1, 20.0 + i as f32 / 64.0, 0).unwrap();
        }
        // Same frame: nothing evictable.
        assert_eq!(atlas.glyph(&font, 2, 20.0, 0).unwrap_err().code(), "atlas_full");
        atlas.begin_frame();
        atlas.glyph(&font, 2, 20.0, 0).unwrap();
        assert_eq!(atlas.stats().evictions, 1);
        atlas.validate().unwrap();
        // Regression: a cache hit on an ink-less glyph must not pin a page. Fill the only page
        // in one frame, cache a space; next frame a space plus a new glyph must still evict.
        let mut atlas = GlyphAtlas::new(AtlasConfig { page_size: 64, max_pages: 1, padding: 1, evict: true });
        let mut n = 0;
        while atlas.glyph(&font, 1, 20.0 + n as f32 / 64.0, 0).is_ok() {
            n += 1;
        }
        assert_eq!(atlas.glyph(&font, 0, 20.0, 0).unwrap().page, NO_PAGE);
        atlas.begin_frame();
        assert_eq!(atlas.glyph(&font, 0, 20.0, 0).unwrap().page, NO_PAGE);
        atlas.glyph(&font, 2, 21.0, 0).unwrap();
        assert_eq!(atlas.stats().evictions, 1);
        atlas.validate().unwrap();

        // Too large for a page at all.
        let e = atlas.glyph(&font, 1, 200.0, 0).unwrap_err();
        assert_eq!(e.code(), "glyph_too_large");
    }
}
