//! Image measures on plain RGBA8 renders: subject masks (from black-on-white silhouette renders),
//! fitting, intersection over union, morphological opening, rings around a silhouette and
//! lightness statistics. Sizes are bounded by the caller (the engine's tiles).

use crate::colour::lightness8;

/// A binary image: `true` where the subject is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mask {
    pub w: u32,
    pub h: u32,
    pub on: Vec<bool>,
}

impl Mask {
    pub fn empty(w: u32, h: u32) -> Mask {
        Mask { w, h, on: vec![false; (w * h) as usize] }
    }

    /// The subject of a black-on-white silhouette render (lightness under one half, that is
    /// relative luminance under 1/8: no cube root per pixel).
    pub fn from_silhouette(rgba: &[u8], w: u32, h: u32) -> Mask {
        let n = (w * h) as usize;
        let lin = crate::colour::linear8();
        let on = (0..n)
            .map(|i| rgba.get(i * 4..i * 4 + 3).is_some_and(|p| 0.2126 * lin[p[0] as usize] + 0.7152 * lin[p[1] as usize] + 0.0722 * lin[p[2] as usize] < 0.125))
            .collect();
        Mask { w, h, on }
    }

    fn at(&self, x: i64, y: i64) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h && self.on[(y as u32 * self.w + x as u32) as usize]
    }

    pub fn area(&self) -> usize {
        self.on.iter().filter(|b| **b).count()
    }

    /// `[x, y, w, h]` of the set pixels.
    pub fn bbox(&self) -> Option<[u32; 4]> {
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
        for y in 0..self.h {
            for x in 0..self.w {
                if self.on[(y * self.w + x) as usize] {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        (x0 != u32::MAX).then(|| [x0, y0, x1 - x0 + 1, y1 - y0 + 1])
    }

    /// The silhouette cropped to its box and scaled (keeping its aspect) to fit `side` × `side`,
    /// standing on the bottom edge and centred across: shapes compare regardless of size and
    /// framing. Area sampling (a pixel is set when at least half of what it covers is).
    pub fn fitted(&self, side: u32) -> Mask {
        let mut out = Mask::empty(side, side);
        let Some([bx, by, bw, bh]) = self.bbox() else { return out };
        let scale = side as f32 / bw.max(bh) as f32;
        let (ow, oh) = ((bw as f32 * scale).round().max(1.0) as u32, (bh as f32 * scale).round().max(1.0) as u32);
        let (ox, oy) = ((side - ow.min(side)) / 2, side - oh.min(side));
        for y in 0..oh.min(side) {
            for x in 0..ow.min(side) {
                // The source rectangle this output pixel covers.
                let (sx0, sx1) = (bx as f32 + x as f32 / scale, bx as f32 + (x + 1) as f32 / scale);
                let (sy0, sy1) = (by as f32 + y as f32 / scale, by as f32 + (y + 1) as f32 / scale);
                let (mut hit, mut all) = (0u32, 0u32);
                let (ix0, ix1) = (sx0.floor() as i64, (sx1.ceil() as i64).max(sx0.floor() as i64 + 1));
                let (iy0, iy1) = (sy0.floor() as i64, (sy1.ceil() as i64).max(sy0.floor() as i64 + 1));
                for sy in iy0..iy1 {
                    for sx in ix0..ix1 {
                        all += 1;
                        hit += self.at(sx, sy) as u32;
                    }
                }
                out.on[((oy + y) * side + ox + x) as usize] = 2 * hit >= all;
            }
        }
        out
    }

    /// Intersection over union with a mask of the same size (1 for two empty masks).
    pub fn iou(&self, other: &Mask) -> f32 {
        if (self.w, self.h) != (other.w, other.h) {
            return 0.0;
        }
        let (mut i, mut u) = (0u32, 0u32);
        for (a, b) in self.on.iter().zip(&other.on) {
            i += (*a && *b) as u32;
            u += (*a || *b) as u32;
        }
        if u == 0 {
            1.0
        } else {
            i as f32 / u as f32
        }
    }

    /// A square (Chebyshev radius `r`) dilation or erosion, as a row pass then a column pass:
    /// O(pixels × r) instead of O(pixels × r²). Outside the image counts as empty.
    fn morph(&self, r: i64, dilate: bool) -> Mask {
        if self.on.iter().all(|b| !*b) {
            return Mask::empty(self.w, self.h);
        }
        let (w, h) = (self.w as i64, self.h as i64);
        let pass = |src: &[bool], horizontal: bool| -> Vec<bool> {
            let mut out = vec![false; src.len()];
            for y in 0..h {
                for x in 0..w {
                    let mut v = !dilate;
                    for d in -r..=r {
                        let (xx, yy) = if horizontal { (x + d, y) } else { (x, y + d) };
                        let s = xx >= 0 && yy >= 0 && xx < w && yy < h && src[(yy * w + xx) as usize];
                        if dilate && s {
                            v = true;
                            break;
                        }
                        if !dilate && !s {
                            v = false;
                            break;
                        }
                    }
                    out[(y * w + x) as usize] = v;
                }
            }
            out
        };
        let rows = pass(&self.on, true);
        Mask { w: self.w, h: self.h, on: pass(&rows, false) }
    }

    pub fn dilated(&self, r: u32) -> Mask {
        self.morph(r as i64, true)
    }

    pub fn eroded(&self, r: u32) -> Mask {
        self.morph(r as i64, false)
    }

    /// The share of the silhouette that survives an opening of radius 1 (erode, then dilate):
    /// parts thinner than about 3 pixels vanish. 1 for a solid shape, low for wire-thin ones.
    pub fn solidity(&self) -> f32 {
        let a = self.area();
        if a == 0 {
            return 0.0;
        }
        let opened = self.eroded(1).dilated(1);
        let kept = self.on.iter().zip(&opened.on).filter(|(s, o)| **s && **o).count();
        kept as f32 / a as f32
    }

    /// The pixels within `r` of the silhouette but outside it.
    pub fn ring(&self, r: u32) -> Mask {
        let mut d = self.dilated(r);
        for (o, s) in d.on.iter_mut().zip(&self.on) {
            *o &= !*s;
        }
        d
    }

    /// Nearest-neighbour scaled to `w` × `h`.
    pub fn resized(&self, w: u32, h: u32) -> Mask {
        let mut out = Mask::empty(w, h);
        for y in 0..h {
            for x in 0..w {
                let sx = (x as u64 * self.w as u64 / w.max(1) as u64) as u32;
                let sy = (y as u64 * self.h as u64 / h.max(1) as u64) as u32;
                out.on[(y * w + x) as usize] = self.on[(sy * self.w + sx) as usize];
            }
        }
        out
    }
}

/// Oklab lightness statistics of the pixels of `rgba` (`w` wide) under `mask`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lightness {
    pub count: usize,
    pub mean: f32,
    pub p10: f32,
    pub p50: f32,
    pub p90: f32,
}

impl Lightness {
    /// `p90 - p10`: how far the form turns in the light.
    pub fn spread(&self) -> f32 {
        self.p90 - self.p10
    }
}

/// The Oklab lightness of every pixel of `rgba` (computed once, shared by the measures).
pub fn lightness_map(rgba: &[u8]) -> Vec<f32> {
    rgba.chunks_exact(4).map(|p| lightness8(p[0], p[1], p[2])).collect()
}

pub fn lightness(rgba: &[u8], mask: &Mask) -> Lightness {
    lightness_of(&lightness_map(rgba), mask)
}

/// [`lightness`] from a [`lightness_map`].
pub fn lightness_of(lmap: &[f32], mask: &Mask) -> Lightness {
    // A histogram of 256 bins keeps it O(pixels) and deterministic.
    let mut hist = [0u32; 256];
    let (mut sum, mut n) = (0.0f64, 0usize);
    for (i, on) in mask.on.iter().enumerate() {
        if !*on {
            continue;
        }
        let Some(&l) = lmap.get(i) else { continue };
        sum += l as f64;
        n += 1;
        hist[((l * 255.0).round() as usize).min(255)] += 1;
    }
    if n == 0 {
        return Lightness::default();
    }
    let pct = |q: f32| {
        let want = (q * n as f32).ceil().max(1.0) as u32;
        let mut acc = 0u32;
        for (b, c) in hist.iter().enumerate() {
            acc += c;
            if acc >= want {
                return b as f32 / 255.0;
            }
        }
        1.0
    };
    Lightness { count: n, mean: (sum / n as f64) as f32, p10: pct(0.1), p50: pct(0.5), p90: pct(0.9) }
}

/// How well a silhouette's outline separates from what is around it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edge {
    /// Outline pixels measured.
    pub count: usize,
    /// Median, over the outline, of |lightness just inside − lightness just outside|.
    pub median: f32,
    /// The weakest quarter of the outline (25th percentile).
    pub p25: f32,
}

/// Contrast along the outline of `mask` in `rgba`: for each outline pixel, the mean lightness of
/// the subject within `r` pixels (one pixel in from the edge, past anti-aliasing) against the
/// mean of the background within `r` (one pixel out). A subject lit from behind (a bright rim
/// over a dark body) or one lighter on top and darker below still reads by its outline, which a
/// mean over the whole subject would average away.
pub fn edge_contrast(rgba: &[u8], mask: &Mask, r: u32) -> Edge {
    edge_contrast_of(&lightness_map(rgba), mask, r)
}

/// [`edge_contrast`] from a [`lightness_map`].
pub fn edge_contrast_of(lmap: &[f32], mask: &Mask, r: u32) -> Edge {
    if mask.area() == 0 {
        return Edge::default();
    }
    let inner = mask.eroded(1);
    let outer = mask.dilated(1);
    let (w, h) = (mask.w as i64, mask.h as i64);
    let l = |i: usize| lmap.get(i).copied().unwrap_or(0.0);
    let r = r.max(1) as i64;
    let mut diffs: Vec<f32> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            if !mask.on[i] || inner.on[i] {
                continue;
            }
            let (mut si, mut ni, mut so, mut no) = (0.0f32, 0u32, 0.0f32, 0u32);
            for dy in -r..=r {
                for dx in -r..=r {
                    let (xx, yy) = (x + dx, y + dy);
                    if xx < 0 || yy < 0 || xx >= w || yy >= h {
                        continue;
                    }
                    let j = (yy * w + xx) as usize;
                    if inner.on[j] {
                        si += l(j);
                        ni += 1;
                    } else if !outer.on[j] {
                        so += l(j);
                        no += 1;
                    }
                }
            }
            if ni > 0 && no > 0 {
                diffs.push((si / ni as f32 - so / no as f32).abs());
            }
        }
    }
    if diffs.is_empty() {
        return Edge::default();
    }
    diffs.sort_by(f32::total_cmp);
    let at = |q: f32| diffs[((diffs.len() - 1) as f32 * q).round() as usize];
    Edge { count: diffs.len(), median: at(0.5), p25: at(0.25) }
}

/// Lightness `rgba` greyscale (the value pass), in place.
pub fn to_value(rgba: &mut [u8]) {
    for p in rgba.chunks_exact_mut(4) {
        let l = (lightness8(p[0], p[1], p[2]) * 255.0).round().clamp(0.0, 255.0) as u8;
        p[0] = l;
        p[1] = l;
        p[2] = l;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: u32, h: u32, r: [u32; 4]) -> Mask {
        let mut m = Mask::empty(w, h);
        for y in r[1]..r[1] + r[3] {
            for x in r[0]..r[0] + r[2] {
                m.on[(y * w + x) as usize] = true;
            }
        }
        m
    }

    #[test]
    fn fitting_makes_size_and_position_irrelevant() {
        let small = rect(100, 100, [10, 20, 10, 30]);
        let big = rect(100, 100, [40, 10, 20, 60]);
        let (a, b) = (small.fitted(64), big.fitted(64));
        assert!(a.iou(&b) > 0.97, "{}", a.iou(&b));
        assert_eq!(a.bbox().map(|b| b[1] + b[3]), Some(64), "stands on the bottom");
        let wide = rect(100, 100, [10, 40, 60, 10]);
        assert!(wide.fitted(64).iou(&a) < 0.3);
        assert_eq!(Mask::empty(8, 8).fitted(16).area(), 0);
    }

    #[test]
    fn separable_morphology_matches_the_square_window() {
        // A plus sign: dilating by 1 fills its 3x3 neighbourhood box corners too (square window).
        let mut m = Mask::empty(9, 9);
        for (x, y) in [(4, 3), (3, 4), (4, 4), (5, 4), (4, 5)] {
            m.on[y * 9 + x] = true;
        }
        let d = m.dilated(1);
        assert_eq!(d.area(), 21, "a 5x5 box minus its 4 corners");
        assert!(d.on[3 * 9 + 3] && !d.on[2 * 9 + 2]);
        assert_eq!(rect(9, 9, [2, 2, 5, 5]).eroded(1), rect(9, 9, [3, 3, 3, 3]));
        assert_eq!(rect(9, 9, [0, 0, 3, 3]).eroded(1).area(), 1, "outside the image counts as empty");
        assert_eq!(Mask::empty(4, 4).dilated(2).area(), 0);
    }

    #[test]
    fn solidity_drops_for_thin_shapes() {
        let block = rect(64, 64, [10, 10, 40, 40]);
        assert!(block.solidity() > 0.99);
        let mut comb = Mask::empty(64, 64);
        for x in (4..60).step_by(6) {
            for y in 10..50 {
                comb.on[y * 64 + x] = true;
            }
        }
        assert!(comb.solidity() < 0.05, "{}", comb.solidity());
    }

    #[test]
    fn rings_and_lightness() {
        let m = rect(20, 20, [5, 5, 10, 10]);
        let ring = m.ring(2);
        assert_eq!(ring.area(), 14 * 14 - 100);
        let mut img = vec![255u8; 20 * 20 * 4];
        for (i, on) in m.on.iter().enumerate() {
            if *on {
                img[i * 4..i * 4 + 3].copy_from_slice(&[0x76, 0x76, 0x76]);
            }
        }
        let l = lightness(&img, &m);
        assert_eq!(l.count, 100);
        assert!((l.mean - 0.566).abs() < 0.01 && l.spread() < 0.01, "{l:?}");
        assert!((lightness(&img, &ring).mean - 1.0).abs() < 1e-3);
        let mut sil_img = img.clone();
        for (i, on) in m.on.iter().enumerate() {
            if *on {
                sil_img[i * 4..i * 4 + 3].copy_from_slice(&[10, 10, 10]);
            }
        }
        assert_eq!(Mask::from_silhouette(&sil_img, 20, 20), m);
        // A grey square on white: the outline separates by 1 - 0.566.
        let e = edge_contrast(&img, &m, 2);
        assert!(e.count > 0 && (e.median - 0.434).abs() < 0.01 && e.p25 > 0.4, "{e:?}");
        // Half the square as light as the background: the median is still the dark half's.
        let mut half = img.clone();
        for y in 5..15 {
            for x in 5..10 {
                half[(y * 20 + x) * 4..(y * 20 + x) * 4 + 3].copy_from_slice(&[255, 255, 255]);
            }
        }
        assert!(edge_contrast(&half, &m, 2).p25 < 0.05);
        assert_eq!(edge_contrast(&img, &Mask::empty(20, 20), 2), Edge::default());
        let mut v = vec![255, 0, 0, 255];
        to_value(&mut v);
        assert!(v[0] == v[1] && v[1] == v[2] && v[0] > 0 && v[0] < 255);
        assert_eq!(m.resized(40, 40).area(), 400);
    }
}
