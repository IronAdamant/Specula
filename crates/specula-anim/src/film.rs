//! Contact-sheet helpers for animation sight (filmstrips): a 5×7 label font, tile blits and
//! framing, on plain RGBA8 sheets. Rendering the tiles is the caller's (the engine's
//! `anim_filmstrip`, the `filmstrip` example); this is the part both share.

use crate::math::{math3, V3};

/// Pixels per label character (5 wide plus a space).
pub const CHAR_W: u32 = 6;
/// Label height in pixels.
pub const CHAR_H: u32 = 7;

/// The 5×7 bitmap of a character (upper case; unknown characters are blank).
pub fn glyph(c: char) -> [u8; 7] {
    match c.to_ascii_uppercase() {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 2, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 17, 25, 21, 19, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [31, 2, 4, 2, 1, 17, 14],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 30, 1, 1, 17, 14],
        '6' => [6, 8, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 2, 12],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ',' => [0, 0, 0, 0, 12, 4, 8],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '>' => [8, 4, 2, 1, 2, 4, 8],
        '=' => [0, 0, 31, 0, 31, 0, 0],
        '!' => [4, 4, 4, 4, 4, 0, 4],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        _ => [0; 7],
    }
}

/// Draws `text` into an RGBA8 image `w` wide at (`x`, `y`) (clipped to the image).
pub fn draw_text(img: &mut [u8], w: u32, x: u32, y: u32, text: &str, color: [u8; 3]) {
    for (i, c) in text.chars().enumerate() {
        let g = glyph(c);
        for (row, bits) in g.iter().enumerate() {
            for col in 0..5 {
                if bits & (16 >> col) != 0 {
                    let (px, py) = (x + i as u32 * CHAR_W + col, y + row as u32);
                    let o = ((py as usize * w as usize) + px as usize) * 4;
                    if px < w && o + 3 < img.len() {
                        img[o..o + 3].copy_from_slice(&color);
                    }
                }
            }
        }
    }
}

/// Copies an RGBA8 tile (`tw` × `th`) into a sheet `sw` wide at (`x0`, `y0`) (clipped).
pub fn blit(sheet: &mut [u8], sw: u32, x0: u32, y0: u32, tile: &[u8], tw: u32, th: u32) {
    let sh = (sheet.len() / 4) as u32 / sw.max(1);
    for y in 0..th.min(sh.saturating_sub(y0)) {
        let w = tw.min(sw.saturating_sub(x0)) as usize * 4;
        let d = (((y0 + y) * sw + x0) * 4) as usize;
        let o = (y * tw * 4) as usize;
        sheet[d..d + w].copy_from_slice(&tile[o..o + w]);
    }
}

/// Fills a rectangle of an RGBA8 sheet.
pub fn fill(sheet: &mut [u8], sw: u32, x0: u32, y0: u32, w: u32, h: u32, rgba: [u8; 4]) {
    let sh = (sheet.len() / 4) as u32 / sw.max(1);
    for y in y0..(y0 + h).min(sh) {
        for x in x0..(x0 + w).min(sw) {
            let o = ((y * sw + x) * 4) as usize;
            sheet[o..o + 4].copy_from_slice(&rgba);
        }
    }
}

/// A camera eye for framing a sphere (`centre`, `radius`) seen from `az`/`el` degrees (azimuth 0
/// = from +Z, the creature's front; 90 = from +X, its left side) with vertical field of view
/// `fov_deg`: the eye position (look at `centre`, +Y up).
pub fn frame_eye(centre: V3, radius: f32, az: f32, el: f32, fov_deg: f32) -> V3 {
    let dist = radius.max(0.05) / (fov_deg.to_radians() * 0.5).sin() * 1.02;
    let (sa, ca) = (az.to_radians().sin(), az.to_radians().cos());
    let (se, ce) = (el.to_radians().sin(), el.to_radians().cos());
    math3::add(centre, [dist * ce * sa, dist * se, dist * ce * ca])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_tiles_land_where_asked() {
        let (w, h) = (40u32, 12u32);
        let mut img = vec![0u8; (w * h * 4) as usize];
        draw_text(&mut img, w, 1, 1, "AB", [255, 0, 0]);
        // 'A' top row is .XXX. → pixels 2..4 of row 1.
        let px = |img: &[u8], x: u32, y: u32| img[((y * w + x) * 4) as usize];
        assert_eq!((px(&img, 1, 1), px(&img, 2, 1), px(&img, 4, 1), px(&img, 5, 1)), (0, 255, 255, 0));
        // Clipped, never out of bounds.
        draw_text(&mut img, w, 38, 10, "WWW", [1, 1, 1]);
        let tile = vec![9u8; 4 * 4 * 4];
        blit(&mut img, w, 38, 10, &tile, 4, 4);
        assert_eq!(px(&img, 39, 11), 9);
        fill(&mut img, w, 0, 0, 2, 2, [7, 7, 7, 7]);
        assert_eq!(px(&img, 1, 1), 7);
        let eye = frame_eye([0.0, 1.0, 0.0], 1.0, 90.0, 0.0, 60.0);
        assert!(eye[0] > 1.9 && eye[2].abs() < 1e-5);
    }
}
