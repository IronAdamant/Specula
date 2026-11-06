//! The bridge from a UI [`DrawList`] to the GPU overlay (`specula_gpu::FrameDesc::overlay`).
//!
//! ```text
//! let mut overlay = GpuOverlay::new();
//! // every frame, after ui.frame(...):
//! ui.set_textures(&[icons.0, portraits.0]);                // once: the game's image textures
//! // every frame, after ui.frame(...):
//! overlay.sync_atlas(&mut gpu, ui.atlas_mut())?;          // new pages + dirty glyph rects
//! let quads = overlay.build(&ui)?;                          // DrawCmd → OverlayQuad, in order
//! for s in overlay.skipped() { log(s.to_json(&ui)) }        // images with unknown textures
//! gpu.render(target, &FrameDesc { overlay: quads, ..frame })?;
//! ```
//!
//! Every [`DrawCmd`] maps onto overlay records in order, so order (and therefore blending) is kept:
//! - `Rect` → a quad with gradient fill, border + border gradient, radius and feather (or up to
//!   four bands of it around a middle that draws nothing: see [`GpuOverlay::build_list`]);
//! - `Glyphs` → alpha-mask quads sampling the atlas page textures (R8, uploaded here);
//! - `Image` → an RGBA quad of the game's texture (`DrawCmd::Image::texture` is a
//!   `specula_gpu::TextureId`'s number). Only textures registered with `Ui::set_textures`
//!   are drawn; others (and this overlay's own atlas pages) are skipped and listed in
//!   [`GpuOverlay::skipped`], so one bad icon can't fail the whole GPU frame;
//! - `Mesh` → coloured triangles (already anti-aliased by their alpha fringe);
//! - `Clip` → the clip rect of every following record.
//!
//! Allocation-free once warm: the quad list and the upload scratch buffer are reused.

use crate::draw::{DrawCmd, DrawList, Owner, Rect, RectCmd};
use crate::ui::{Ui, WidgetRef};
use specula_core::json::Json;
use specula_gpu::{GpuError, Limit, OverlayKind, OverlayQuad, OverlayTextures, TextureFormat, TextureId, MAX_OVERLAY_QUADS};
use specula_text::GlyphAtlas;

/// Why an image wasn't drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// The texture id isn't in `Ui::set_textures` (or no registry was set).
    UnknownTexture,
    /// The id is one of this overlay's glyph atlas pages, which screens may not show.
    AtlasPage,
}

/// An image command [`GpuOverlay::build`] skipped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkippedImage {
    pub texture: u32,
    pub widget: WidgetRef,
    pub reason: SkipReason,
}

impl SkippedImage {
    /// Machine-readable, naming the screen and widget.
    pub fn to_json(&self, ui: &Ui) -> Json {
        let screen = if self.widget.screen < ui.screen_count() { ui.screen_name(self.widget.screen).to_owned() } else { String::new() };
        let widget = ui.widget_id(self.widget).map(str::to_owned).or_else(|| ui.widget_path(self.widget).map(str::to_owned)).unwrap_or_default();
        Json::obj([
            ("code", Json::Str(match self.reason {
                SkipReason::UnknownTexture => "unknown_texture".into(),
                SkipReason::AtlasPage => "atlas_page_texture".into(),
            })),
            ("texture", Json::Num(f64::from(self.texture))),
            ("screen", Json::Str(screen)),
            ("widget", Json::Str(widget)),
        ])
    }
}

/// Owns the glyph atlas textures and the per-frame overlay list.
#[derive(Default)]
pub struct GpuOverlay {
    pages: Vec<TextureId>,
    page_size: u32,
    quads: Vec<OverlayQuad>,
    scratch: Vec<u8>,
    skipped: Vec<SkippedImage>,
}

impl GpuOverlay {
    pub fn new() -> GpuOverlay {
        GpuOverlay::default()
    }

    /// Atlas page textures uploaded so far (page index → texture).
    pub fn atlas_textures(&self) -> &[TextureId] {
        &self.pages
    }

    /// Upload atlas pages that are new since the last call, and the regions glyph rasterising
    /// changed in the others (`GlyphAtlas::take_dirty`). Call after `Ui::frame`, before drawing.
    /// `gpu` is a `specula_gpu::Gpu`, or whatever a host lends (a hosted game gets the engine's
    /// overlay textures, not the device).
    pub fn sync_atlas<G: OverlayTextures + ?Sized>(&mut self, gpu: &mut G, atlas: &mut GlyphAtlas) -> Result<(), GpuError> {
        let size = atlas.page_size();
        if self.page_size != 0 && self.page_size != size {
            // The atlas was rebuilt with another page size: start over.
            self.destroy_textures(gpu);
        }
        self.page_size = size;
        let fresh = self.pages.len();
        while self.pages.len() < atlas.page_count() {
            let page = self.pages.len();
            let pixels = atlas.page_pixels(page).unwrap_or(&[]);
            let id = gpu.upload_texture(size, size, TextureFormat::R8, pixels)?;
            self.pages.push(id);
        }
        for r in atlas.take_dirty() {
            let page = r.page as usize;
            if page >= fresh || r.width == 0 || r.height == 0 {
                continue; // a whole new page went up above
            }
            atlas.copy_rect(&r, &mut self.scratch);
            gpu.update_texture(self.pages[page], r.x, r.y, r.width, r.height, &self.scratch)?;
        }
        Ok(())
    }

    /// The overlay quads for `ui`'s last frame, in draw order. Images whose texture isn't in
    /// `ui`'s registry are skipped (see [`GpuOverlay::skipped`]). Fails with the GPU's structured
    /// limit error beyond [`MAX_OVERLAY_QUADS`].
    pub fn build(&mut self, ui: &Ui) -> Result<&[OverlayQuad], GpuError> {
        self.build_list(ui.draw_list(), ui.textures())
    }

    /// [`GpuOverlay::build`] for any draw list, with an explicit image texture registry.
    ///
    /// **Fewer fragments, the same pixels.** A rect whose middle draws nothing (a bevel or an
    /// outline: transparent fill) and a rect the next rect covers with opaque fill (a soft
    /// shadow under its panel) are drawn as bands around that middle (up to eight: edge strips
    /// and corner squares), each the same record with a whole-pixel clip ([`hole`]): the pixels
    /// inside would have blended nothing
    /// (alpha 0) or been painted over, so the image is unchanged. On the HUD these middles were
    /// half the overlay pass's GPU time (review R1 of m5-uitools, finding 7: shadows and bevels
    /// 0.12 ms each of 0.46 ms on the Intel iGPU at 1.5x). Past [`MAX_OVERLAY_QUADS`] with the
    /// bands, the list is built again without them.
    pub fn build_list(&mut self, list: &DrawList, images: Option<&[u32]>) -> Result<&[OverlayQuad], GpuError> {
        self.build_with(list, images, true);
        if self.quads.len() > MAX_OVERLAY_QUADS {
            self.build_with(list, images, false);
        }
        if self.quads.len() > MAX_OVERLAY_QUADS {
            return Err(GpuError::limit(Limit::OverlayQuads, self.quads.len() as u64, MAX_OVERLAY_QUADS as u64));
        }
        Ok(&self.quads)
    }

    /// [`GpuOverlay::build`] with every rect one whole record (no bands): the same pixels, for
    /// tests that compare the two on a GPU.
    pub fn build_whole(&mut self, ui: &Ui) -> Result<&[OverlayQuad], GpuError> {
        self.build_with(ui.draw_list(), ui.textures(), false);
        if self.quads.len() > MAX_OVERLAY_QUADS {
            return Err(GpuError::limit(Limit::OverlayQuads, self.quads.len() as u64, MAX_OVERLAY_QUADS as u64));
        }
        Ok(&self.quads)
    }

    fn build_with(&mut self, list: &DrawList, images: Option<&[u32]>, bands: bool) {
        self.quads.clear();
        self.skipped.clear();
        let mut clip: Option<[f32; 4]> = None;
        for (k, cmd) in list.cmds.iter().enumerate() {
            match *cmd {
                DrawCmd::Clip(r) => clip = r.map(|r| r.to_array()),
                DrawCmd::Rect(r) => {
                    let quad = OverlayQuad {
                        rect: r.rect.to_array(),
                        color: r.fill[0],
                        color_bottom: Some(r.fill[1]),
                        border: r.border,
                        border_color: r.border_color,
                        feather: r.feather,
                        radius: r.radius,
                        clip,
                        ..OverlayQuad::default()
                    };
                    let next = match list.cmds.get(k + 1) {
                        Some(DrawCmd::Rect(n)) => Some(n),
                        _ => None,
                    };
                    match hole(&r, next).filter(|_| bands) {
                        Some(h) => push_bands(&mut self.quads, quad, h),
                        None => self.quads.push(quad),
                    }
                }
                DrawCmd::Image { rect, uv, texture, color, owner } => {
                    let reason = if self.pages.iter().any(|p| p.0 == texture) {
                        Some(SkipReason::AtlasPage)
                    } else if !images.is_some_and(|ids| ids.binary_search(&texture).is_ok()) {
                        Some(SkipReason::UnknownTexture)
                    } else {
                        None
                    };
                    match reason {
                        Some(reason) => {
                            let Owner { screen, node } = owner;
                            self.skipped.push(SkippedImage { texture, widget: WidgetRef { screen: screen as usize, node }, reason });
                        }
                        None => self.quads.push(OverlayQuad {
                            clip,
                            ..OverlayQuad::textured(rect.to_array(), uv, TextureId(texture), OverlayKind::Rgba, color)
                        }),
                    }
                }
                DrawCmd::Glyphs { start, end } => {
                    for g in &list.glyphs[start as usize..end as usize] {
                        let Some(&tex) = self.pages.get(g.page as usize) else { continue };
                        // GlyphQuad rects are [x0, y0, x1, y1]; overlay rects are x, y, w, h.
                        let r = Rect::new(g.rect[0], g.rect[1], g.rect[2] - g.rect[0], g.rect[3] - g.rect[1]);
                        self.quads.push(OverlayQuad { clip, ..OverlayQuad::textured(r.to_array(), g.uv, tex, OverlayKind::AlphaMask, g.color) });
                    }
                }
                DrawCmd::Mesh { start, end } => {
                    for t in list.vertices[start as usize..end as usize].chunks_exact(3) {
                        self.quads.push(OverlayQuad {
                            clip,
                            ..OverlayQuad::triangle([t[0].pos, t[1].pos, t[2].pos], [t[0].color, t[1].color, t[2].color])
                        });
                    }
                }
            }
        }
    }

    /// Image commands the last build skipped, with the widget that drew them.
    pub fn skipped(&self) -> &[SkippedImage] {
        &self.skipped
    }

    fn destroy_textures<G: OverlayTextures + ?Sized>(&mut self, gpu: &mut G) {
        for t in self.pages.drain(..) {
            gpu.destroy_texture(t);
        }
    }

    /// Free the atlas textures.
    pub fn destroy<G: OverlayTextures + ?Sized>(mut self, gpu: &mut G) {
        self.destroy_textures(gpu);
    }
}

/// Far beyond any target (a band's open side).
const FAR: f32 = 8_000_000.0;

/// Whether every pixel a rect fills at full coverage is exactly opaque: both fill stops.
fn opaque(r: &RectCmd) -> bool {
    r.fill.iter().all(|c| c[3] >= 1.0)
}

/// The middle of a rect that draws nothing worth drawing: a plus shape, the union of
/// `inner` narrowed by `corner` top and bottom and `inner` narrowed by `corner` left and right
/// (whole pixels), where `inner` is the rect inset by `border + feather / 2 + 1` and `corner`
/// covers its rounded corners. Pixel centres in the plus have the overlay shader's edge and
/// border coverage exactly 1 (along a straight edge the distance to the edge is the signed
/// distance; the corners' arcs stay inside the corner squares), so the fill's alpha decides.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Hole {
    inner: [f32; 4],
    corner: f32,
}

/// Where `next` (drawn right after `r`, with the same clip) covers `r` with opaque fill, or
/// where `r` itself has a transparent fill (only its border and edge draw): that middle, `None`
/// when it is under a pixel wide.
fn hole(r: &RectCmd, next: Option<&RectCmd>) -> Option<Hole> {
    let middle = |c: &RectCmd| {
        let d = c.border.max(0.0) + 0.5 * c.feather.max(1.0) + 1.0;
        let inner = [(c.rect.x + d).ceil(), (c.rect.y + d).ceil(), (c.rect.right() - d).floor(), (c.rect.bottom() - d).floor()];
        let radius = c.radius.max(0.0).min(0.5 * c.rect.w.min(c.rect.h).max(0.0));
        let corner = radius.ceil();
        let ok = inner.iter().all(|v| v.is_finite()) && corner.is_finite() && inner[2] - inner[0] >= 2.0 * corner + 1.0 && inner[3] - inner[1] >= 2.0 * corner + 1.0;
        ok.then_some(Hole { inner, corner })
    };
    if let Some(n) = next.filter(|n| opaque(n)) {
        if let Some(h) = middle(n) {
            return Some(h);
        }
    }
    let empty = r.fill.iter().all(|c| c[3] <= 0.0);
    if empty {
        return middle(r);
    }
    None
}

/// `quad` as bands around `hole`: four edge strips and four corner squares, each clipped to the
/// quad's clip as the GPU snaps it (whole pixels), so together they draw exactly the pixels the
/// whole quad would outside the hole.
fn push_bands(out: &mut Vec<OverlayQuad>, quad: OverlayQuad, hole: Hole) {
    // The clip as the GPU rounds it (each edge), so band edges agree with it exactly.
    let [cx0, cy0, cx1, cy1] = match quad.clip {
        Some(c) if c.iter().all(|v| v.is_finite()) => [c[0].round(), c[1].round(), (c[0] + c[2]).round(), (c[1] + c[3]).round()],
        Some(_) => {
            out.push(quad);
            return;
        }
        None => [-FAR, -FAR, FAR, FAR],
    };
    let ([x0, y0, x1, y1], k) = (hole.inner, hole.corner);
    let bands = [
        // The edges: above, below, left and right of `inner`.
        [-FAR, -FAR, FAR, y0],
        [-FAR, y1, FAR, FAR],
        [-FAR, y0, x0, y1],
        [x1, y0, FAR, y1],
        // The corners of `inner` the plus leaves out.
        [x0, y0, x0 + k, y0 + k],
        [x1 - k, y0, x1, y0 + k],
        [x0, y1 - k, x0 + k, y1],
        [x1 - k, y1 - k, x1, y1],
    ];
    for [bx0, by0, bx1, by1] in bands {
        let (bx0, by0, bx1, by1) = (bx0.max(cx0), by0.max(cy0), bx1.min(cx1), by1.min(cy1));
        if bx1 > bx0 && by1 > by0 {
            out.push(OverlayQuad { clip: Some([bx0, by0, bx1 - bx0, by1 - by0]), ..quad });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::{RectCmd, Vertex};
    use specula_text::GlyphQuad;

    #[test]
    fn draw_commands_map_to_overlay_records_in_order() {
        let mut d = DrawList { size: [100, 100], scale: 1.0, ..DrawList::default() };
        d.rect(RectCmd {
            rect: Rect::new(1.0, 2.0, 30.0, 40.0),
            radius: 4.0,
            fill: [[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]],
            border: 2.0,
            border_color: [[1.0; 4], [0.0, 0.0, 0.0, 1.0]],
            feather: 6.0,
        });
        d.clip(Some(Rect::new(0.0, 0.0, 50.0, 50.0)));
        let s = d.glyphs.len() as u32;
        d.glyphs.push(GlyphQuad { rect: [10.0, 10.0, 18.0, 22.0], uv: [0.0, 0.0, 0.1, 0.1], page: 0, color: [1.0; 4] });
        d.glyph_run(s);
        let m = d.begin_mesh();
        let v = |x: f32| Vertex { pos: [x, 0.0], color: [0.5; 4] };
        d.vertices.extend([v(0.0), v(1.0), v(2.0)]);
        d.end_mesh(m);
        d.clip(None);
        let owner = Owner { screen: 0, node: 3 };
        d.cmds.push(DrawCmd::Image { rect: Rect::new(5.0, 5.0, 4.0, 4.0), uv: [0.0, 0.0, 1.0, 1.0], texture: 7, color: [1.0; 4], owner });
        d.cmds.push(DrawCmd::Image { rect: Rect::new(5.0, 5.0, 4.0, 4.0), uv: [0.0; 4], texture: 4242, color: [1.0; 4], owner });
        d.cmds.push(DrawCmd::Image { rect: Rect::new(5.0, 5.0, 4.0, 4.0), uv: [0.0; 4], texture: 99, color: [1.0; 4], owner });
        let mut o = GpuOverlay::new();
        o.pages.push(TextureId(99));
        let q = o.build_list(&d, Some(&[7, 99])).unwrap();
        assert_eq!(q.len(), 4, "the unknown texture and the atlas page are skipped");
        assert_eq!((q[0].rect, q[0].color_bottom, q[0].border, q[0].feather, q[0].clip), ([1.0, 2.0, 30.0, 40.0], Some([0.0, 0.0, 1.0, 1.0]), 2.0, 6.0, None));
        assert_eq!((q[1].rect, q[1].texture, q[1].kind, q[1].clip), ([10.0, 10.0, 8.0, 12.0], Some(TextureId(99)), OverlayKind::AlphaMask, Some([0.0, 0.0, 50.0, 50.0])));
        assert!(q[2].triangle.is_some() && q[2].clip.is_some());
        assert_eq!((q[3].texture, q[3].clip), (Some(TextureId(7)), None));
        let skipped: Vec<(u32, SkipReason)> = o.skipped().iter().map(|s| (s.texture, s.reason)).collect();
        assert_eq!(skipped, vec![(4242, SkipReason::UnknownTexture), (99, SkipReason::AtlasPage)]);
        // No registry: every image is skipped.
        o.build_list(&d, None).unwrap();
        assert_eq!(o.skipped().len(), 3);
    }

    /// The overlay shaders' maths for untextured quads on the CPU (overlay.vert's clip and
    /// overlay.frag's coverage, premultiplied blending in f64): what a pixel ends up as.
    fn shade(quads: &[OverlayQuad], w: usize, h: usize) -> Vec<[f64; 4]> {
        let mut img = vec![[0.1, 0.2, 0.3, 1.0]; w * h];
        for q in quads {
            let clip = match q.clip {
                Some(c) => [c[0].round(), c[1].round(), (c[0] + c[2]).round(), (c[1] + c[3]).round()],
                None => [-1e9, -1e9, 1e9, 1e9],
            };
            let [rx, ry, rw, rh] = q.rect;
            for y in 0..h {
                for x in 0..w {
                    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                    if fx < clip[0] || fy < clip[1] || fx >= clip[2] || fy >= clip[3] {
                        continue;
                    }
                    let (lx, ly) = (fx - rx, fy - ry);
                    let (hw, hh) = (0.5 * rw, 0.5 * rh);
                    let r = q.radius.clamp(0.0, hw.min(hh));
                    let (px, py) = ((lx - hw).abs() - (hw - r), (ly - hh).abs() - (hh - r));
                    let sd = (px.max(0.0).powi(2) + py.max(0.0).powi(2)).sqrt() + px.max(py).min(0.0) - r;
                    let outer = (0.5 - sd / q.feather.max(1.0)).clamp(0.0, 1.0);
                    let t = (ly / rh.max(1e-6)).clamp(0.0, 1.0);
                    let bottom = q.color_bottom.unwrap_or(q.color);
                    let fill: Vec<f32> = (0..4).map(|i| q.color[i] + (bottom[i] - q.color[i]) * t).collect();
                    let (rgb, a) = if q.border > 0.0 {
                        let inner = (0.5 - (sd + q.border)).clamp(0.0, 1.0).min(outer);
                        let b: Vec<f32> = (0..4).map(|i| q.border_color[0][i] + (q.border_color[1][i] - q.border_color[0][i]) * t).collect();
                        let af = fill[3].clamp(0.0, 1.0) * inner;
                        let ab = b[3].clamp(0.0, 1.0) * (outer - inner);
                        ([fill[0] * af + b[0] * ab, fill[1] * af + b[1] * ab, fill[2] * af + b[2] * ab], af + ab)
                    } else {
                        let a = fill[3].clamp(0.0, 1.0) * outer;
                        ([fill[0] * a, fill[1] * a, fill[2] * a], a)
                    };
                    let d = &mut img[y * w + x];
                    let k = 1.0 - f64::from(a);
                    *d = [f64::from(rgb[0]) + d[0] * k, f64::from(rgb[1]) + d[1] * k, f64::from(rgb[2]) + d[2] * k, f64::from(a) + d[3] * k];
                }
            }
        }
        img
    }

    /// Finding 7 of review R1 (m5-uitools): bevels, outlines and shadows under opaque panels
    /// are drawn as bands around their middles, and the image is the same as with whole quads
    /// (to float rounding: an opaque fill's alpha interpolates to 1 within an ulp).
    #[test]
    fn bands_draw_the_same_pixels_as_whole_quads() {
        let mut d = DrawList { size: [160, 120], scale: 1.5, ..DrawList::default() };
        let opaque = [[0.21, 0.24, 0.27, 1.0], [0.13, 0.15, 0.18, 1.0]];
        let clear = [[0.0; 4]; 2];
        let rect = |x: f32, y: f32, w: f32, h: f32, radius: f32, fill: [[f32; 4]; 2], border: f32, feather: f32| RectCmd {
            rect: Rect::new(x, y, w, h),
            radius,
            fill,
            border,
            border_color: [[0.31, 0.35, 0.4, 1.0], [0.08, 0.09, 0.11, 0.8]],
            feather,
        };
        // A panel as draw_box makes it: soft shadow, opaque fill with a border, bevel ring.
        d.rect(RectCmd { fill: [[0.0, 0.0, 0.0, 0.63]; 2], border: 0.0, ..rect(10.3, 14.7, 90.2, 50.5, 9.0, clear, 0.0, 13.5) });
        d.rect(rect(10.3, 10.2, 90.2, 50.5, 9.0, opaque, 1.5, 1.0));
        d.rect(RectCmd { border_color: [[1.0, 1.0, 1.0, 0.1], [0.0, 0.0, 0.0, 0.22]], ..rect(11.8, 11.7, 87.2, 47.5, 7.5, clear, 1.5, 1.0) });
        // Inside a fractional clip (a scrolled list): a button's shadow, fill and bevel.
        d.clip(Some(Rect::new(20.4, 70.6, 61.3, 30.5)));
        d.rect(RectCmd { fill: [[0.0, 0.0, 0.0, 0.56]; 2], border: 0.0, ..rect(24.0, 75.0, 50.0, 30.0, 5.0, clear, 0.0, 6.0) });
        d.rect(rect(24.0, 72.0, 50.0, 30.0, 5.0, opaque, 1.0, 1.0));
        d.rect(RectCmd { border_color: [[1.0, 1.0, 1.0, 0.12], [0.0, 0.0, 0.0, 0.22]], ..rect(25.0, 73.0, 48.0, 28.0, 4.0, clear, 1.0, 1.0) });
        d.clip(None);
        // A translucent panel's shadow stays whole (it shows through); a tiny outline too.
        d.rect(RectCmd { fill: [[0.0, 0.0, 0.0, 0.7]; 2], border: 0.0, ..rect(108.0, 20.0, 40.0, 60.0, 6.0, clear, 0.0, 20.0) });
        d.rect(rect(108.0, 16.0, 40.0, 60.0, 6.0, [[0.2, 0.22, 0.26, 0.96]; 2], 1.0, 1.0));
        d.rect(rect(120.0, 90.0, 5.0, 4.0, 1.0, clear, 1.0, 1.0));
        let mut o = GpuOverlay::new();
        let split = o.build_list(&d, None).unwrap().to_vec();
        o.build_with(&d, None, false);
        let whole = o.quads.clone();
        assert_eq!(whole.len(), 9);
        // Big shadow, big bevel, the clipped shadow and bevel: 4 bands each (the clipped ones
        // fewer where the clip cuts a side off); the rest whole.
        assert!(split.len() > whole.len() + 8, "{} quads", split.len());
        let (a, b) = (shade(&whole, 160, 120), shade(&split, 160, 120));
        let worst = a.iter().zip(&b).flat_map(|(p, q)| (0..4).map(move |i| (p[i] - q[i]).abs())).fold(0.0, f64::max);
        assert!(worst < 1e-6, "the bands change a pixel by {worst}");
        // Something was saved: the bands' clipped area is well under the whole quads'.
        let area = |qs: &[OverlayQuad]| -> f32 {
            qs.iter()
                .map(|q| {
                    let g = 0.5 * q.feather.max(1.0);
                    let (mut x0, mut y0, mut x1, mut y1) = (q.rect[0] - g, q.rect[1] - g, q.rect[0] + q.rect[2] + g, q.rect[1] + q.rect[3] + g);
                    if let Some(c) = q.clip {
                        (x0, y0, x1, y1) = (x0.max(c[0]), y0.max(c[1]), x1.min(c[0] + c[2]), y1.min(c[1] + c[3]));
                    }
                    (x1 - x0).max(0.0) * (y1 - y0).max(0.0)
                })
                .sum()
        };
        assert!(area(&split) < 0.9 * area(&whole), "{} vs {}", area(&split), area(&whole));
    }

    /// Bands never push a list past the GPU's limit: it is built whole instead.
    #[test]
    fn bands_give_way_at_the_quad_limit() {
        let mut d = DrawList { size: [64, 64], scale: 1.0, ..DrawList::default() };
        let ring = RectCmd { rect: Rect::new(2.0, 2.0, 40.0, 40.0), radius: 2.0, fill: [[0.0; 4]; 2], border: 1.0, border_color: [[1.0; 4]; 2], feather: 1.0 };
        for _ in 0..MAX_OVERLAY_QUADS {
            d.rect(ring);
        }
        let n = GpuOverlay::new().build_list(&d, None).unwrap().len();
        assert_eq!(n, MAX_OVERLAY_QUADS);
    }
}
