//! Render a HUD line and a paragraph at several sizes to a PNG, through the full pipeline
//! (font → layout → atlas → quads → a tiny CPU "overlay renderer"), so a human or an agent can
//! judge legibility.
//!
//! ```text
//! cargo run -p specula-text --example render_text [--release] -- [out.png] [font.ttf]
//! ```
//!
//! Default output: `target/text-example/render_text.png`; default font: the system UI font.

use specula_core::deflate::Level;
use specula_core::png::encode_rgba8;
use specula_text::*;
use std::path::PathBuf;

const W: u32 = 960;
const H: u32 = 860;

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn hex(rgb: u32) -> [f32; 4] {
    [((rgb >> 16) & 255) as f32 / 255.0, ((rgb >> 8) & 255) as f32 / 255.0, (rgb & 255) as f32 / 255.0, 1.0]
}

/// A linear-light framebuffer, like an sRGB swapchain with blending in linear space.
struct Canvas {
    px: Vec<[f32; 3]>,
}

impl Canvas {
    fn fill(&mut self, x0: u32, y0: u32, x1: u32, y1: u32, c: [f32; 4]) {
        let lin = [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2])];
        for y in y0..y1.min(H) {
            for x in x0..x1.min(W) {
                self.px[(y * W + x) as usize] = lin;
            }
        }
    }

    /// Draw glyph quads exactly as the GPU overlay pass would: 1:1 texel fetch, coverage × alpha.
    fn draw(&mut self, atlas: &GlyphAtlas, quads: &[GlyphQuad]) {
        let ps = atlas.page_size() as f32;
        for q in quads {
            let page = atlas.page_pixels(q.page as usize).unwrap();
            let lin = [srgb_to_linear(q.color[0]), srgb_to_linear(q.color[1]), srgb_to_linear(q.color[2])];
            let (w, h) = ((q.rect[2] - q.rect[0]) as i32, (q.rect[3] - q.rect[1]) as i32);
            let (tx, ty) = ((q.uv[0] * ps).round() as i32, (q.uv[1] * ps).round() as i32);
            for dy in 0..h {
                for dx in 0..w {
                    let (x, y) = (q.rect[0] as i32 + dx, q.rect[1] as i32 + dy);
                    if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 {
                        continue;
                    }
                    let cov = page[((ty + dy) * ps as i32 + tx + dx) as usize] as f32 / 255.0 * q.color[3];
                    let p = &mut self.px[(y as u32 * W + x as u32) as usize];
                    for k in 0..3 {
                        p[k] += (lin[k] - p[k]) * cov;
                    }
                }
            }
        }
    }

    /// Draw an SDF bitmap scaled by `zoom` with a smoothstep edge (what an SDF shader does).
    fn draw_sdf(&mut self, b: &Bitmap, x0: f32, baseline: f32, zoom: f32, spread: f32, c: [f32; 4]) {
        let lin = [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2])];
        let (w, h) = (b.width as f32 * zoom, b.height as f32 * zoom);
        let (ox, oy) = (x0 + b.left as f32 * zoom, baseline + b.top as f32 * zoom);
        // Edge softness: one output pixel in distance units.
        let px_dist = 1.0 / zoom / (2.0 * spread);
        for y in oy.floor() as i32..(oy + h).ceil() as i32 {
            for x in ox.floor() as i32..(ox + w).ceil() as i32 {
                if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 {
                    continue;
                }
                // Bilinear sample at the pixel centre.
                let u = (x as f32 + 0.5 - ox) / zoom - 0.5;
                let v = (y as f32 + 0.5 - oy) / zoom - 0.5;
                let (iu, iv) = (u.floor(), v.floor());
                let (fu, fv) = (u - iu, v - iv);
                let s = |a: f32, b2: f32| b.get(a.max(0.0) as u32, b2.max(0.0) as u32) as f32 / 255.0;
                let s = |a: f32, c2: f32| if a < 0.0 || c2 < 0.0 { 0.0 } else { s(a, c2) };
                let d = s(iu, iv) * (1.0 - fu) * (1.0 - fv)
                    + s(iu + 1.0, iv) * fu * (1.0 - fv)
                    + s(iu, iv + 1.0) * (1.0 - fu) * fv
                    + s(iu + 1.0, iv + 1.0) * fu * fv;
                let t = ((d - 0.5 + px_dist * 0.5) / px_dist).clamp(0.0, 1.0);
                let cov = t * t * (3.0 - 2.0 * t);
                let p = &mut self.px[(y as u32 * W + x as u32) as usize];
                for k in 0..3 {
                    p[k] += (lin[k] - p[k]) * cov;
                }
            }
        }
    }

    fn rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity((W * H * 4) as usize);
        for p in &self.px {
            for c in p {
                out.push((linear_to_srgb(c.clamp(0.0, 1.0)) * 255.0 + 0.5) as u8);
            }
            out.push(255);
        }
        out
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args.first().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("target/text-example/render_text.png"));
    let font = match args.get(1) {
        Some(p) => load_font_file(p).unwrap_or_else(|e| panic!("{e}")),
        None => match find_system_ui_font() {
            Some((path, font)) => {
                println!("font: {}", path.display());
                font
            }
            None => {
                println!("no system UI font found; pass a .ttf path as the second argument");
                return;
            }
        },
    };
    println!("family: {}", font.family_name());

    let stone = hex(0x2b3038);
    let ivory = hex(0xece6d6);
    let cyan = hex(0x3fe0d0);
    let paper = hex(0xf2efe8);
    let ink = hex(0x1d2026);

    let mut canvas = Canvas { px: vec![[0.0; 3]; (W * H) as usize] };
    canvas.fill(0, 0, W, H, hex(0x14171c));
    let mut atlas = GlyphAtlas::new(AtlasConfig::default());
    let mut quads = Vec::new();
    let hud = "Wave 3/10   Dust 240   Tombstones 9";
    let para = "The slab cracked on the night the bells stopped. Raise tombstones along the \
                broken edge, carve runes into them, and hold until dawn. Wraiths drift over \
                anything; ghouls must climb. AVA, Toy, LT: kerning check.";

    // Left panel: ivory on carved stone (the Sepulcra HUD style), sizes 10-20 px.
    canvas.fill(16, 16, 470, 540, stone);
    let mut y = 28.0f32;
    for size in [10.0, 11.0, 12.0, 13.0, 14.0, 16.0, 18.0, 20.0] {
        let label = layout(&font, &format!("{size} px"), &TextStyle::new(10.0));
        quads.clear();
        label.append_quads(&font, &mut atlas, [24.0, y], cyan, &mut quads).unwrap();
        canvas.draw(&atlas, &quads);
        let l = layout(&font, hud, &TextStyle::new(size));
        quads.clear();
        l.append_quads(&font, &mut atlas, [72.0, y], ivory, &mut quads).unwrap();
        canvas.draw(&atlas, &quads);
        y += l.height.max(14.0) + 6.0;
    }
    // Paragraph, wrapped and justified three ways at 13 px.
    for (i, align) in [Align::Left, Align::Center, Align::Right].into_iter().enumerate() {
        let style = TextStyle::new(13.0).with_max_width(420.0).with_align(align);
        let l = layout(&font, if i == 0 { para } else { "Centred and right-aligned lines stay inside the box." }, &style);
        quads.clear();
        l.append_quads(&font, &mut atlas, [32.0, y + 4.0], ivory, &mut quads).unwrap();
        canvas.draw(&atlas, &quads);
        y += l.height + 8.0;
    }

    // Right panel: dark ink on paper, plus a 2x DPI line.
    canvas.fill(490, 16, W - 16, 540, paper);
    let mut y = 28.0f32;
    for size in [10.0, 12.0, 14.0, 16.0] {
        let l = layout(&font, hud, &TextStyle::new(size));
        quads.clear();
        l.append_quads(&font, &mut atlas, [500.0, y], ink, &mut quads).unwrap();
        canvas.draw(&atlas, &quads);
        y += l.height + 6.0;
    }
    let style = TextStyle::new(12.0).with_max_width(210.0).with_scale(2.0);
    let l = layout(&font, para, &style);
    quads.clear();
    l.append_quads(&font, &mut atlas, [500.0, y + 6.0], ink, &mut quads).unwrap();
    canvas.draw(&atlas, &quads);

    // Bottom: SDF title — one 32 px SDF per glyph, drawn at 32, 64 and 112 px.
    let title = "Sepulcra";
    let mut top = 552.0;
    for zoom in [1.0f32, 2.0, 3.5] {
        let l = layout(&font, title, &TextStyle::new(32.0 * zoom));
        let spread = 4.0;
        for g in &l.glyphs {
            let b = font.rasterize_glyph_sdf(g.glyph, 32.0, spread).unwrap();
            canvas.draw_sdf(&b, 24.0 + g.x, top + g.y, zoom, spread, ivory);
        }
        top += l.height * 0.85;
    }
    // Next to it, the coverage atlas page (top-left 256x256) so the packing is visible.
    let page = atlas.page_pixels(0).unwrap();
    let ps = atlas.page_size();
    for y in 0..256u32 {
        for x in 0..256u32 {
            let v = page[(y * ps + x) as usize] as f32 / 255.0;
            let (cx, cy) = (W - 16 - 256 + x, 560 + y);
            if cy < H {
                canvas.px[(cy * W + cx) as usize] = [v, v, v];
            }
        }
    }

    let rgba = canvas.rgba();
    let png = encode_rgba8(W, H, &rgba, Level::Default);
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(&out, png).unwrap();

    // A 3x nearest-neighbour zoom of the small HUD sizes, to judge individual pixels.
    let (cx, cy, cw, ch, z) = (16u32, 16u32, 400u32, 200u32, 3u32);
    let mut zoom = Vec::with_capacity((cw * ch * z * z * 4) as usize);
    for y in 0..ch * z {
        for x in 0..cw * z {
            let i = (((cy + y / z) * W + cx + x / z) * 4) as usize;
            zoom.extend_from_slice(&rgba[i..i + 4]);
        }
    }
    let zoom_path = out.with_file_name("render_text_zoom.png");
    std::fs::write(&zoom_path, encode_rgba8(cw * z, ch * z, &zoom, Level::Default)).unwrap();
    println!("wrote {}", zoom_path.display());
    let s = atlas.stats();
    println!("wrote {} ({}x{}); atlas: {} glyphs, {} page(s)", out.display(), W, H, s.glyphs, atlas.page_count());
}
