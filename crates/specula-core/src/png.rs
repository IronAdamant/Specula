//! PNG encode (RGBA8) and decode (common colour types → RGBA8).

use crate::checksum::{crc32, crc32_update};
use crate::deflate::{self, Level};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGBA8, row-major, top row first. `len == width * height * 4`.
    pub rgba: Vec<u8>,
}

/// The 8-byte PNG file signature.
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];

/// IDAT payloads are split into chunks of at most this many bytes.
const IDAT_CHUNK: usize = 1 << 20;

/// Decoder refuses images with more pixels than this (1 GiB of RGBA8) instead of allocating.
const MAX_PIXELS: u64 = 1 << 28;

/// Encode RGBA8 pixels (top row first). Panics if `rgba.len() != width * height * 4`.
/// Uses adaptive per-row filtering (pick the filter with smallest sum of abs values) and zlib.
pub fn encode_rgba8(width: u32, height: u32, rgba: &[u8], level: Level) -> Vec<u8> {
    encode_with_filter(width, height, rgba, level, None)
}

/// Encoder core; `force_filter` pins every row to one filter type (tests use it to exercise
/// each unfilter path).
fn encode_with_filter(width: u32, height: u32, rgba: &[u8], level: Level, force_filter: Option<u8>) -> Vec<u8> {
    let stride = width as usize * 4;
    assert_eq!(
        rgba.len(),
        stride * height as usize,
        "encode_rgba8: rgba has {} bytes, expected {width}×{height}×4",
        rgba.len()
    );
    let filtered = filter_rows(rgba, stride, height as usize, force_filter);
    let z = deflate::zlib_compress(&filtered, level);

    let mut out = Vec::with_capacity(z.len() + 64);
    out.extend_from_slice(&SIGNATURE);
    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&width.to_be_bytes());
    ihdr[4..8].copy_from_slice(&height.to_be_bytes());
    ihdr[8] = 8; // bit depth
    ihdr[9] = 6; // colour type: RGBA
    // compression 0, filter method 0, interlace 0
    write_chunk(&mut out, b"IHDR", &ihdr);
    for part in z.chunks(IDAT_CHUNK) {
        write_chunk(&mut out, b"IDAT", part);
    }
    write_chunk(&mut out, b"IEND", &[]);
    out
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32_update(crc32(kind), data).to_be_bytes());
}

/// Paeth predictor (PNG spec §9.4).
#[inline]
fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i16 + b as i16 - c as i16;
    let pa = (p - a as i16).abs();
    let pb = (p - b as i16).abs();
    let pc = (p - c as i16).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Bytes per complete pixel for the filters (8-bit RGBA).
const BPP: usize = 4;

/// Prefix each row with its filter type byte and the filtered bytes. Adaptive selection uses the
/// "minimum sum of absolute differences" heuristic (bytes read as signed).
fn filter_rows(rgba: &[u8], stride: usize, height: usize, force: Option<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity((stride + 1) * height);
    let zero_row = vec![0u8; stride];
    let mut candidates: [Vec<u8>; 5] = std::array::from_fn(|_| vec![0u8; stride]);
    for y in 0..height {
        let row = &rgba[y * stride..(y + 1) * stride];
        let up = if y == 0 { &zero_row[..] } else { &rgba[(y - 1) * stride..y * stride] };
        let types: &[u8] = match force {
            Some(t) => &[t][..],
            None => &[0, 1, 2, 3, 4][..],
        };
        let mut best = (u64::MAX, 0u8);
        for &t in types {
            let buf = &mut candidates[t as usize];
            apply_filter(t, row, up, buf);
            let cost: u64 = buf.iter().map(|&b| (b as i8).unsigned_abs() as u64).sum();
            if cost < best.0 {
                best = (cost, t);
            }
        }
        out.push(best.1);
        out.extend_from_slice(&candidates[best.1 as usize]);
    }
    out
}

fn apply_filter(t: u8, row: &[u8], up: &[u8], out: &mut [u8]) {
    let n = row.len();
    match t {
        0 => out.copy_from_slice(row),
        1 => {
            out[..BPP.min(n)].copy_from_slice(&row[..BPP.min(n)]);
            for i in BPP..n {
                out[i] = row[i].wrapping_sub(row[i - BPP]);
            }
        }
        2 => {
            for i in 0..n {
                out[i] = row[i].wrapping_sub(up[i]);
            }
        }
        3 => {
            for i in 0..n {
                let left = if i >= BPP { row[i - BPP] as u16 } else { 0 };
                out[i] = row[i].wrapping_sub(((left + up[i] as u16) / 2) as u8);
            }
        }
        _ => {
            for i in 0..n {
                let (a, c) = if i >= BPP { (row[i - BPP], up[i - BPP]) } else { (0, 0) };
                out[i] = row[i].wrapping_sub(paeth(a, up[i], c));
            }
        }
    }
}

/// Decode 8-bit non-interlaced PNGs of colour types 0 (grey), 2 (RGB), 3 (palette, incl. tRNS),
/// 4 (grey+alpha) and 6 (RGBA) to RGBA8. Validates chunk CRCs. Interlaced or 16-bit → error.
pub fn decode(bytes: &[u8]) -> Result<Image, PngError> {
    if bytes.len() < 8 || bytes[..8] != SIGNATURE {
        return Err(PngError::new("missing PNG signature"));
    }
    let mut pos = 8;
    let mut header: Option<Header> = None;
    let mut palette: Vec<[u8; 4]> = Vec::new();
    let mut trns: Option<Vec<u8>> = None;
    let mut idat = Vec::new();
    let mut seen_iend = false;
    while !seen_iend {
        if bytes.len() < pos + 12 {
            return Err(PngError::new(format!("truncated chunk header at byte {pos}")));
        }
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        if len > i32::MAX as usize {
            return Err(PngError::new(format!("chunk length {len} at byte {pos} exceeds 2^31-1")));
        }
        let kind: [u8; 4] = bytes[pos + 4..pos + 8].try_into().unwrap();
        let name = String::from_utf8_lossy(&kind).into_owned();
        let Some(data) = bytes.get(pos + 8..pos + 8 + len) else {
            return Err(PngError::new(format!("chunk {name} at byte {pos} is truncated")));
        };
        let Some(crc_bytes) = bytes.get(pos + 8 + len..pos + 12 + len) else {
            return Err(PngError::new(format!("chunk {name} at byte {pos} is missing its CRC")));
        };
        let stored = u32::from_be_bytes(crc_bytes.try_into().unwrap());
        let actual = crc32_update(crc32(&kind), data);
        if stored != actual {
            return Err(PngError::new(format!(
                "CRC mismatch in chunk {name} at byte {pos}: stored {stored:#010x}, computed {actual:#010x}"
            )));
        }
        if header.is_none() && &kind != b"IHDR" {
            return Err(PngError::new(format!("first chunk is {name}, expected IHDR")));
        }
        match &kind {
            b"IHDR" => {
                if header.is_some() {
                    return Err(PngError::new("duplicate IHDR chunk"));
                }
                header = Some(Header::parse(data)?);
            }
            b"PLTE" => {
                if len % 3 != 0 || len == 0 || len > 256 * 3 {
                    return Err(PngError::new(format!("PLTE length {len} is invalid")));
                }
                palette = data.chunks(3).map(|c| [c[0], c[1], c[2], 255]).collect();
            }
            b"tRNS" => trns = Some(data.to_vec()),
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => seen_iend = true,
            _ => {
                if kind[0] & 0x20 == 0 {
                    return Err(PngError::new(format!("unsupported critical chunk {name}")));
                }
                // Ancillary chunks (text, gamma, …) are ignored.
            }
        }
        pos += 12 + len;
    }
    let h = header.expect("IHDR checked above");
    if idat.is_empty() {
        return Err(PngError::new("no IDAT chunks"));
    }

    let channels = h.channels();
    let stride = h.width as usize * channels;
    let expected = (stride + 1) * h.height as usize;
    let raw = deflate::zlib_decompress_limited(&idat, expected, true)
        .map_err(|e| PngError::new(format!("image data: {}", e.message)))?;
    if raw.len() != expected {
        return Err(PngError::new(format!(
            "image data is {} bytes, expected {expected} for {}×{}",
            raw.len(),
            h.width,
            h.height
        )));
    }
    let pixels = unfilter(&raw, stride, h.height as usize, channels)?;

    let n = h.width as usize * h.height as usize;
    let mut rgba = Vec::with_capacity(n * 4);
    match h.color_type {
        0 => {
            let key = trns_key::<1>(&trns)?.map(|k| k[0]);
            for &g in &pixels {
                let a = if key == Some(g as u16) { 0 } else { 255 };
                rgba.extend_from_slice(&[g, g, g, a]);
            }
        }
        2 => {
            let key = trns_key::<3>(&trns)?;
            for px in pixels.chunks_exact(3) {
                let rgb = [px[0] as u16, px[1] as u16, px[2] as u16];
                let a = if key == Some(rgb) { 0 } else { 255 };
                rgba.extend_from_slice(&[px[0], px[1], px[2], a]);
            }
        }
        3 => {
            if palette.is_empty() {
                return Err(PngError::new("palette image without PLTE chunk"));
            }
            if let Some(t) = &trns {
                if t.len() > palette.len() {
                    return Err(PngError::new("tRNS has more entries than PLTE"));
                }
                for (entry, &a) in palette.iter_mut().zip(t) {
                    entry[3] = a;
                }
            }
            for &i in &pixels {
                let entry = palette.get(i as usize).ok_or_else(|| {
                    PngError::new(format!("palette index {i} out of range ({} entries)", palette.len()))
                })?;
                rgba.extend_from_slice(entry);
            }
        }
        4 => {
            for px in pixels.chunks_exact(2) {
                rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
        }
        _ => rgba = pixels,
    }
    Ok(Image { width: h.width, height: h.height, rgba })
}

/// Parsed IHDR (only the supported subset gets past [`Header::parse`]).
struct Header {
    width: u32,
    height: u32,
    color_type: u8,
}

impl Header {
    fn parse(d: &[u8]) -> Result<Header, PngError> {
        if d.len() != 13 {
            return Err(PngError::new(format!("IHDR length is {}, expected 13", d.len())));
        }
        let width = u32::from_be_bytes(d[0..4].try_into().unwrap());
        let height = u32::from_be_bytes(d[4..8].try_into().unwrap());
        let (depth, color_type, compression, filter, interlace) = (d[8], d[9], d[10], d[11], d[12]);
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err(PngError::new(format!("invalid image size {width}×{height}")));
        }
        if u64::from(width) * u64::from(height) > MAX_PIXELS {
            return Err(PngError::new(format!("image {width}×{height} exceeds the {MAX_PIXELS}-pixel limit")));
        }
        if !matches!(color_type, 0 | 2 | 3 | 4 | 6) {
            return Err(PngError::new(format!("invalid colour type {color_type}")));
        }
        if depth != 8 {
            return Err(PngError::new(format!(
                "unsupported bit depth {depth} (colour type {color_type}); only 8-bit images are supported"
            )));
        }
        if compression != 0 || filter != 0 {
            return Err(PngError::new(format!("unknown compression ({compression}) or filter ({filter}) method")));
        }
        match interlace {
            0 => {}
            1 => return Err(PngError::new("interlaced (Adam7) PNGs are not supported")),
            m => return Err(PngError::new(format!("invalid interlace method {m}"))),
        }
        Ok(Header { width, height, color_type })
    }

    fn channels(&self) -> usize {
        match self.color_type {
            0 | 3 => 1,
            4 => 2,
            2 => 3,
            _ => 4,
        }
    }
}

/// Colour-key transparency for grey (`N` = 1) or RGB (`N` = 3) images: 16-bit big-endian samples.
fn trns_key<const N: usize>(trns: &Option<Vec<u8>>) -> Result<Option<[u16; N]>, PngError> {
    let Some(t) = trns else { return Ok(None) };
    let len = 2 * N;
    if t.len() != len {
        return Err(PngError::new(format!("tRNS length {} is invalid, expected {len}", t.len())));
    }
    Ok(Some(std::array::from_fn(|i| u16::from_be_bytes([t[2 * i], t[2 * i + 1]]))))
}

/// Undo per-row filters. `raw` holds `height` rows of `1 + stride` bytes.
fn unfilter(raw: &[u8], stride: usize, height: usize, bpp: usize) -> Result<Vec<u8>, PngError> {
    let mut out = vec![0u8; stride * height];
    for y in 0..height {
        let line = &raw[y * (stride + 1)..(y + 1) * (stride + 1)];
        let (prev, cur) = out.split_at_mut(y * stride);
        let up: &[u8] = if y == 0 { &[] } else { &prev[(y - 1) * stride..] };
        let cur = &mut cur[..stride];
        let src = &line[1..];
        let up_at = |i: usize| if y == 0 { 0 } else { up[i] };
        match line[0] {
            0 => cur.copy_from_slice(src),
            1 => {
                for i in 0..stride {
                    let left = if i >= bpp { cur[i - bpp] } else { 0 };
                    cur[i] = src[i].wrapping_add(left);
                }
            }
            2 => {
                for i in 0..stride {
                    cur[i] = src[i].wrapping_add(up_at(i));
                }
            }
            3 => {
                for i in 0..stride {
                    let left = if i >= bpp { cur[i - bpp] as u16 } else { 0 };
                    cur[i] = src[i].wrapping_add(((left + up_at(i) as u16) / 2) as u8);
                }
            }
            4 => {
                for i in 0..stride {
                    let (a, c) = if i >= bpp { (cur[i - bpp], up_at(i - bpp)) } else { (0, 0) };
                    cur[i] = src[i].wrapping_add(paeth(a, up_at(i), c));
                }
            }
            f => return Err(PngError::new(format!("invalid filter type {f} on row {y}"))),
        }
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PngError {
    pub message: String,
}

impl PngError {
    fn new(message: impl Into<String>) -> Self {
        PngError { message: message.into() }
    }
}

impl std::fmt::Display for PngError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PNG error: {}", self.message)
    }
}

impl std::error::Error for PngError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1280×720 test frame: flat background, a few shapes and a horizontal gradient band.
    fn synthetic_frame(w: u32, h: u32) -> Vec<u8> {
        let mut px = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = (x as f32, y as f32);
                let mut c = [30u8, 34, 42, 255]; // background
                if y >= h * 3 / 4 {
                    // gradient floor
                    let t = x * 255 / (w - 1);
                    c = [t as u8, (255 - t) as u8, 128, 255];
                }
                if (200..520).contains(&x) && (150..400).contains(&y) {
                    c = [220, 60, 50, 255]; // red rectangle
                }
                let (dx, dy) = (fx - 900.0, fy - 300.0);
                if dx * dx + dy * dy < 140.0 * 140.0 {
                    c = [60, 160, 230, 255]; // blue disc
                }
                if y > 420 && y < 520 && x > 600 && x - 600 < (y - 420) * 2 {
                    c = [240, 200, 40, 200]; // translucent triangle
                }
                let i = ((y * w + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&c);
            }
        }
        px
    }

    fn png_from_parts(ihdr: [u8; 13], extra: &[(&[u8; 4], &[u8])], zlib: &[u8]) -> Vec<u8> {
        let mut out = SIGNATURE.to_vec();
        write_chunk(&mut out, b"IHDR", &ihdr);
        for (kind, data) in extra {
            write_chunk(&mut out, kind, data);
        }
        write_chunk(&mut out, b"IDAT", zlib);
        write_chunk(&mut out, b"IEND", &[]);
        out
    }

    fn ihdr(w: u32, h: u32, depth: u8, color: u8, interlace: u8) -> [u8; 13] {
        let mut d = [0u8; 13];
        d[0..4].copy_from_slice(&w.to_be_bytes());
        d[4..8].copy_from_slice(&h.to_be_bytes());
        d[8] = depth;
        d[9] = color;
        d[12] = interlace;
        d
    }

    #[test]
    fn round_trip_with_every_filter() {
        let (w, h) = (37u32, 23u32);
        let mut seed = 0x1234_5678u32;
        let px: Vec<u8> = (0..w * h * 4)
            .map(|i| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                if i % 7 == 0 { seed as u8 } else { (i / 4 % w * 3 + i / (4 * w) * 5) as u8 }
            })
            .collect();
        for force in [None, Some(0), Some(1), Some(2), Some(3), Some(4)] {
            let png = encode_with_filter(w, h, &px, Level::Default, force);
            let img = decode(&png).unwrap();
            assert_eq!((img.width, img.height), (w, h));
            assert_eq!(img.rgba, px, "filter {force:?}");
        }
        // A 1×1 image is valid too.
        let one = decode(&encode_rgba8(1, 1, &[1, 2, 3, 4], Level::Fast)).unwrap();
        assert_eq!(one.rgba, [1, 2, 3, 4]);
    }

    #[test]
    fn synthetic_frame_compresses_well_and_fast() {
        let (w, h) = (1280, 720);
        let px = synthetic_frame(w, h);
        let start = std::time::Instant::now();
        let png = encode_rgba8(w, h, &px, Level::Default);
        let elapsed = start.elapsed();
        let ratio = png.len() as f64 / px.len() as f64;
        println!("1280x720 frame: {} bytes ({:.3}% of raw) in {elapsed:?}", png.len(), ratio * 100.0);
        assert!(ratio < 0.10, "PNG is {:.1}% of raw", ratio * 100.0);
        assert_eq!(decode(&png).unwrap().rgba, px);
    }

    #[test]
    fn decodes_hand_built_grey_and_grey_alpha() {
        // Grey 3×2, filter None. python: zlib.compress(bytes([0,0,128,255, 0,10,20,30]))
        let z = [120, 156, 99, 96, 104, 248, 207, 192, 37, 34, 7, 0, 8, 103, 1, 188];
        let img = decode(&png_from_parts(ihdr(3, 2, 8, 0, 0), &[], &z)).unwrap();
        let grey: Vec<u8> = img.rgba.chunks(4).map(|p| p[0]).collect();
        assert_eq!(grey, [0, 128, 255, 10, 20, 30]);
        assert!(img.rgba.chunks(4).all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 255));
        // Same data with tRNS grey key 128 → that pixel transparent.
        let img = decode(&png_from_parts(ihdr(3, 2, 8, 0, 0), &[(b"tRNS", &[0, 128])], &z)).unwrap();
        let alpha: Vec<u8> = img.rgba.chunks(4).map(|p| p[3]).collect();
        assert_eq!(alpha, [255, 0, 255, 255, 255, 255]);

        // Grey+alpha 2×1. python: zlib.compress(bytes([0, 50,100, 200,0]))
        let z = [120, 156, 99, 48, 74, 57, 193, 0, 0, 3, 137, 1, 95];
        let img = decode(&png_from_parts(ihdr(2, 1, 8, 4, 0), &[], &z)).unwrap();
        assert_eq!(img.rgba, [50, 50, 50, 100, 200, 200, 200, 0]);
    }

    #[test]
    fn decodes_hand_built_rgb_with_sub_and_paeth() {
        // RGB 2×2. Row 0 Sub-filtered, row 1 Paeth-filtered, pixels
        // [(10,20,30),(15,25,35)] / [(12,22,32),(200,100,50)].
        // python: zlib.compress(bytes([1, 10,20,30, 5,5,5,  4, 2,2,2, 185,75,15]))
        let z = [120, 156, 99, 228, 18, 145, 99, 101, 101, 101, 97, 98, 98, 218, 233, 205, 15, 0, 6, 105, 1, 106];
        let img = decode(&png_from_parts(ihdr(2, 2, 8, 2, 0), &[], &z)).unwrap();
        assert_eq!(img.rgba, [10, 20, 30, 255, 15, 25, 35, 255, 12, 22, 32, 255, 200, 100, 50, 255]);
    }

    #[test]
    fn decodes_hand_built_palette_with_trns() {
        // Palette 4×1, indices 0,1,2,1. python: zlib.compress(bytes([0, 0,1,2,1]))
        let z = [120, 156, 99, 96, 96, 100, 98, 4, 0, 0, 13, 0, 5];
        let plte: &[u8] = &[255, 0, 0, 0, 255, 0, 0, 0, 255];
        let trns: &[u8] = &[128];
        let img = decode(&png_from_parts(ihdr(4, 1, 8, 3, 0), &[(b"PLTE", plte), (b"tRNS", trns)], &z)).unwrap();
        assert_eq!(img.rgba, [255, 0, 0, 128, 0, 255, 0, 255, 0, 0, 255, 255, 0, 255, 0, 255]);
        // Index beyond the palette.
        let short: &[u8] = &[1, 2, 3];
        let err = decode(&png_from_parts(ihdr(4, 1, 8, 3, 0), &[(b"PLTE", short)], &z)).unwrap_err();
        assert!(err.message.contains("palette index"), "{err}");
    }

    #[test]
    fn rejects_bad_files_with_clear_errors() {
        let png = encode_rgba8(4, 4, &[7u8; 64], Level::Default);
        // CRC corruption in IDAT payload.
        let mut bad = png.clone();
        let idat = png.windows(4).position(|w| w == b"IDAT").unwrap();
        bad[idat + 6] ^= 0x40;
        assert!(decode(&bad).unwrap_err().message.contains("CRC mismatch in chunk IDAT"));
        // Signature, truncation.
        assert!(decode(b"GIF89a").is_err());
        for cut in [8, 20, 33, png.len() - 1] {
            assert!(decode(&png[..cut]).is_err(), "cut {cut}");
        }
        let z = deflate::zlib_compress(&[0, 0, 0], Level::Default);
        let e = decode(&png_from_parts(ihdr(1, 1, 16, 0, 0), &[], &z)).unwrap_err();
        assert!(e.message.contains("bit depth 16"), "{e}");
        let e = decode(&png_from_parts(ihdr(1, 1, 8, 0, 1), &[], &z)).unwrap_err();
        assert!(e.message.contains("interlaced"), "{e}");
        let e = decode(&png_from_parts(ihdr(1, 1, 8, 5, 0), &[], &z)).unwrap_err();
        assert!(e.message.contains("colour type"), "{e}");
        // Wrong amount of image data, invalid filter byte.
        let e = decode(&png_from_parts(ihdr(1, 2, 8, 0, 0), &[], &z)).unwrap_err();
        assert!(e.message.contains("expected"), "{e}");
        let z = deflate::zlib_compress(&[9, 0], Level::Default);
        let e = decode(&png_from_parts(ihdr(1, 1, 8, 0, 0), &[], &z)).unwrap_err();
        assert!(e.message.contains("filter type 9"), "{e}");
        // Unknown critical chunk.
        let z = deflate::zlib_compress(&[0, 0], Level::Default);
        let e = decode(&png_from_parts(ihdr(1, 1, 8, 0, 0), &[(b"ABCD", &[])], &z)).unwrap_err();
        assert!(e.message.contains("critical chunk ABCD"), "{e}");
        // Ancillary chunks are skipped.
        assert!(decode(&png_from_parts(ihdr(1, 1, 8, 0, 0), &[(b"tEXt", b"k\0v")], &z)).is_ok());
    }
}

