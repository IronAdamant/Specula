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
//! Every [`DrawCmd`] maps onto one overlay record, so order (and therefore blending) is kept:
//! - `Rect` → a quad with gradient fill, border + border gradient, radius and feather;
//! - `Glyphs` → alpha-mask quads sampling the atlas page textures (R8, uploaded here);
//! - `Image` → an RGBA quad of the game's texture (`DrawCmd::Image::texture` is a
//!   `specula_gpu::TextureId`'s number). Only textures registered with `Ui::set_textures`
//!   are drawn; others (and this overlay's own atlas pages) are skipped and listed in
//!   [`GpuOverlay::skipped`], so one bad icon can't fail the whole GPU frame;
//! - `Mesh` → coloured triangles (already anti-aliased by their alpha fringe);
//! - `Clip` → the clip rect of every following record.
//!
//! Allocation-free once warm: the quad list and the upload scratch buffer are reused.

use crate::draw::{DrawCmd, DrawList, Owner, Rect};
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
    pub fn build_list(&mut self, list: &DrawList, images: Option<&[u32]>) -> Result<&[OverlayQuad], GpuError> {
        self.quads.clear();
        self.skipped.clear();
        let mut clip: Option<[f32; 4]> = None;
        for cmd in &list.cmds {
            match *cmd {
                DrawCmd::Clip(r) => clip = r.map(|r| r.to_array()),
                DrawCmd::Rect(r) => self.quads.push(OverlayQuad {
                    rect: r.rect.to_array(),
                    color: r.fill[0],
                    color_bottom: Some(r.fill[1]),
                    border: r.border,
                    border_color: r.border_color,
                    feather: r.feather,
                    radius: r.radius,
                    clip,
                    ..OverlayQuad::default()
                }),
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
        if self.quads.len() > MAX_OVERLAY_QUADS {
            return Err(GpuError::limit(Limit::OverlayQuads, self.quads.len() as u64, MAX_OVERLAY_QUADS as u64));
        }
        Ok(&self.quads)
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
}
