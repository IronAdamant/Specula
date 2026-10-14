//! A small JPEG encoder: baseline (standard or optimised Huffman tables) or progressive
//! (spectral selection + successive approximation, optimised tables), 4:4:4 / 4:2:2 / 4:2:0,
//! optional restart markers. It exists so tests can round-trip images without third-party
//! files, and so tools and agents can write JPEGs; it favours clarity over speed.
//!
//! Deterministic: integer colour conversion, a fixed-point separable DCT and integer
//! quantisation, so the same pixels give the same bytes everywhere.

use super::{JpegError, JpegErrorCode, ZIGZAG};

/// Chroma subsampling of the first component's neighbours (the first component is full size).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subsampling {
    S444,
    /// Half horizontal chroma resolution.
    S422,
    /// Half horizontal and vertical chroma resolution.
    S420,
}

/// How the RGBA input is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorModel {
    /// JFIF YCbCr (alpha dropped).
    YCbCr,
    /// One luma component (alpha dropped).
    Gray,
    /// R, G, B stored directly (Adobe APP14 transform 0).
    AdobeRgb,
    /// R, G, B, A stored as four "inverted CMYK" components (Adobe transform 0, as Photoshop
    /// writes CMYK): a decoder returns `(R·A/255, G·A/255, B·A/255)`.
    AdobeCmyk,
    /// YCbCr of the inverted RGB plus A as K (Adobe transform 2, YCCK); decodes like
    /// [`ColorModel::AdobeCmyk`].
    AdobeYcck,
}

/// Encoder settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncodeOptions {
    /// 1..=100 (IJG scaling of the Annex K tables; 50 = the tables as printed).
    pub quality: u8,
    pub subsampling: Subsampling,
    pub color: ColorModel,
    /// Progressive (SOF2) with the IJG "simple progression" scan script.
    pub progressive: bool,
    /// Optimal Huffman tables for a baseline image (progressive images always use them).
    pub optimize: bool,
    /// MCUs between restart markers (0 = none).
    pub restart_interval: u16,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        EncodeOptions { quality: 90, subsampling: Subsampling::S420, color: ColorModel::YCbCr, progressive: false, optimize: false, restart_interval: 0 }
    }
}

/// Baseline 4:2:0 YCbCr JPEG of RGBA8 pixels (alpha dropped) with the standard tables.
///
/// Panics if `rgba.len() != width * height * 4` or a dimension is outside 1..=65535 (like
/// `png::encode_rgba8`; use [`encode`] for a `Result`).
pub fn encode_baseline(width: u32, height: u32, rgba: &[u8], quality: u8) -> Vec<u8> {
    encode(width, height, rgba, &EncodeOptions { quality, ..EncodeOptions::default() }).expect("valid image for encode_baseline")
}

/// Encodes RGBA8 pixels (top row first) as a JPEG.
pub fn encode(width: u32, height: u32, rgba: &[u8], opts: &EncodeOptions) -> Result<Vec<u8>, JpegError> {
    encode_with(width, height, rgba, opts, true)
}

/// [`encode`]; `interleaved: false` writes a sequential image as one scan per component
/// (legal, rare in the wild, and a separate decoder path worth testing).
pub(super) fn encode_with(width: u32, height: u32, rgba: &[u8], opts: &EncodeOptions, interleaved: bool) -> Result<Vec<u8>, JpegError> {
    let fail = |m: String| JpegError::new(JpegErrorCode::Limit, 0, m);
    if !(1..=65535).contains(&width) || !(1..=65535).contains(&height) {
        return Err(fail(format!("{width}x{height}: JPEG dimensions are 1..=65535")));
    }
    let (w, h) = (width as usize, height as usize);
    if rgba.len() != w * h * 4 {
        return Err(fail(format!("{} bytes of RGBA for {w}x{h} (expected {})", rgba.len(), w * h * 4)));
    }
    let quality = opts.quality.clamp(1, 100);
    // Full-resolution component planes.
    let px = |i: usize| (rgba[4 * i] as i32, rgba[4 * i + 1] as i32, rgba[4 * i + 2] as i32, rgba[4 * i + 3]);
    let n = w * h;
    let ycc = |r: i32, g: i32, b: i32| -> [u8; 3] {
        let y = (19_595 * r + 38_470 * g + 7_471 * b + 32_768) >> 16;
        let cb = ((-11_059 * r - 21_709 * g + 32_768 * b + 32_767) >> 16) + 128;
        let cr = ((32_768 * r - 27_439 * g - 5_329 * b + 32_767) >> 16) + 128;
        [y.clamp(0, 255) as u8, cb.clamp(0, 255) as u8, cr.clamp(0, 255) as u8]
    };
    let (planes, ids, adobe): (Vec<Vec<u8>>, Vec<u8>, Option<u8>) = match opts.color {
        ColorModel::Gray => (vec![(0..n).map(|i| ycc(px(i).0, px(i).1, px(i).2)[0]).collect()], vec![1], None),
        ColorModel::YCbCr | ColorModel::AdobeYcck => {
            let inv = opts.color == ColorModel::AdobeYcck;
            let mut p = vec![Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n)];
            for i in 0..n {
                let (r, g, b, _) = px(i);
                let c = if inv { ycc(255 - r, 255 - g, 255 - b) } else { ycc(r, g, b) };
                for k in 0..3 {
                    p[k].push(c[k]);
                }
            }
            if inv {
                p.push((0..n).map(|i| px(i).3).collect());
                (p, vec![1, 2, 3, 4], Some(2))
            } else {
                (p, vec![1, 2, 3], None)
            }
        }
        ColorModel::AdobeRgb => ((0..3).map(|k| (0..n).map(|i| rgba[4 * i + k]).collect()).collect(), b"RGB".to_vec(), Some(0)),
        ColorModel::AdobeCmyk => ((0..4).map(|k| (0..n).map(|i| rgba[4 * i + k]).collect()).collect(), vec![1, 2, 3, 4], Some(0)),
    };
    let nc = planes.len();
    let (h0, v0) = match (nc, opts.subsampling) {
        (1, _) | (_, Subsampling::S444) => (1, 1),
        (_, Subsampling::S422) => (2, 1),
        (_, Subsampling::S420) => (2, 2),
    };
    let (hmax, vmax) = (h0, v0);
    let (mcux, mcuy) = (w.div_ceil(8 * hmax), h.div_ceil(8 * vmax));
    let chroma_tables = matches!(opts.color, ColorModel::YCbCr | ColorModel::AdobeYcck);
    let quant = [scale_quant(&LUMA_Q, quality), scale_quant(&CHROMA_Q, quality)];
    let mut comps = Vec::with_capacity(nc);
    for (k, plane) in planes.iter().enumerate() {
        let (ch, cv) = if k == 0 { (h0, v0) } else { (1, 1) };
        let tq = usize::from(chroma_tables && (k == 1 || k == 2));
        let (bw, bh) = (mcux * ch, mcuy * cv);
        let (fx, fy) = (hmax / ch, vmax / cv);
        // Downsample (box average) into a plane padded to whole blocks by edge replication.
        let (pw, ph) = (bw * 8, bh * 8);
        let mut sub = vec![0u8; pw * ph];
        for y in 0..ph {
            for x in 0..pw {
                let mut sum = 0u32;
                for j in 0..fy {
                    for i in 0..fx {
                        let sx = (x * fx + i).min(w - 1);
                        let sy = (y * fy + j).min(h - 1);
                        sum += plane[sy * w + sx] as u32;
                    }
                }
                let cnt = (fx * fy) as u32;
                sub[y * pw + x] = ((sum + cnt / 2) / cnt) as u8;
            }
        }
        let mut blocks = Vec::with_capacity(bw * bh);
        let mut samples = [0i64; 64];
        for by in 0..bh {
            for bx in 0..bw {
                for r in 0..8 {
                    for c in 0..8 {
                        samples[r * 8 + c] = sub[(by * 8 + r) * pw + bx * 8 + c] as i64 - 128;
                    }
                }
                blocks.push(fdct_quantize(&samples, &quant[tq]));
            }
        }
        let cbw = (w * ch).div_ceil(hmax).div_ceil(8);
        let cbh = (h * cv).div_ceil(vmax).div_ceil(8);
        comps.push(EComp { id: ids[k], h: ch, v: cv, tq, bw, cbw, cbh, blocks });
    }

    let mut out = vec![0xFF, 0xD8];
    match adobe {
        None => out.extend_from_slice(&[0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1, 1, 0, 0, 1, 0, 1, 0, 0]),
        Some(t) => out.extend_from_slice(&[0xFF, 0xEE, 0, 14, b'A', b'd', b'o', b'b', b'e', 0, 100, 0, 0, 0, 0, t]),
    }
    let ntables = if chroma_tables { 2 } else { 1 };
    out.extend_from_slice(&[0xFF, 0xDB]);
    out.extend_from_slice(&((2 + 65 * ntables) as u16).to_be_bytes());
    for (t, q) in quant.iter().enumerate().take(ntables) {
        out.push(t as u8);
        out.extend(ZIGZAG.iter().map(|&z| q[z] as u8));
    }
    out.extend_from_slice(&[0xFF, if opts.progressive { 0xC2 } else { 0xC0 }]);
    out.extend_from_slice(&((8 + 3 * nc) as u16).to_be_bytes());
    out.push(8);
    out.extend_from_slice(&(height as u16).to_be_bytes());
    out.extend_from_slice(&(width as u16).to_be_bytes());
    out.push(nc as u8);
    for c in &comps {
        out.extend_from_slice(&[c.id, ((c.h << 4) | c.v) as u8, c.tq as u8]);
    }
    if opts.restart_interval > 0 {
        out.extend_from_slice(&[0xFF, 0xDD, 0, 4]);
        out.extend_from_slice(&opts.restart_interval.to_be_bytes());
    }
    let grid = Grid { mcux, mcuy, restart: opts.restart_interval as usize };
    let all: Vec<usize> = (0..nc).collect();
    let scans: Vec<ScanDef> = if opts.progressive {
        let mut s = vec![ScanDef { comps: all.clone(), ss: 0, se: 0, ah: 0, al: 1 }];
        for c in 0..nc {
            s.push(ScanDef { comps: vec![c], ss: 1, se: 5, ah: 0, al: 2 });
            s.push(ScanDef { comps: vec![c], ss: 6, se: 63, ah: 0, al: 2 });
            s.push(ScanDef { comps: vec![c], ss: 1, se: 63, ah: 2, al: 1 });
        }
        s.push(ScanDef { comps: all.clone(), ss: 0, se: 0, ah: 1, al: 0 });
        for c in 0..nc {
            s.push(ScanDef { comps: vec![c], ss: 1, se: 63, ah: 1, al: 0 });
        }
        s
    } else if !interleaved {
        (0..nc).map(|c| ScanDef { comps: vec![c], ss: 0, se: 63, ah: 0, al: 0 }).collect()
    } else {
        vec![ScanDef { comps: all, ss: 0, se: 63, ah: 0, al: 0 }]
    };
    for scan in &scans {
        // Table slot of each component: 0 for the first component, 1 for the others.
        let slot = |c: usize| usize::from(c > 0);
        let tables: [EncTable; 4] = if opts.progressive || opts.optimize {
            let mut e = Emitter::new(true, Default::default(), opts.progressive);
            run_scan(&mut e, &comps, scan, &grid);
            let mut t: [EncTable; 4] = Default::default();
            let mut dht = Vec::new();
            for (i, f) in e.freq.iter().enumerate() {
                if f.iter().any(|&x| x > 0) {
                    let (counts, vals) = optimal_table(f);
                    t[i] = EncTable::from_spec(&counts, &vals);
                    dht.push(((i / 2) as u8, (i % 2) as u8, counts, vals));
                }
            }
            write_dht(&mut out, &dht);
            t
        } else {
            let specs = [(STD_DC_LUMA_BITS, &STD_DC_VALS[..]), (STD_DC_CHROMA_BITS, &STD_DC_VALS[..]), (STD_AC_LUMA_BITS, &STD_AC_LUMA_VALS[..]), (STD_AC_CHROMA_BITS, &STD_AC_CHROMA_VALS[..])];
            let used = if nc > 1 { 2 } else { 1 };
            let mut dht = Vec::new();
            for (i, (counts, vals)) in specs.iter().enumerate() {
                if i % 2 < used {
                    dht.push(((i / 2) as u8, (i % 2) as u8, *counts, vals.to_vec()));
                }
            }
            write_dht(&mut out, &dht);
            [EncTable::from_spec(&specs[0].0, specs[0].1), EncTable::from_spec(&specs[1].0, specs[1].1), EncTable::from_spec(&specs[2].0, specs[2].1), EncTable::from_spec(&specs[3].0, specs[3].1)]
        };
        out.extend_from_slice(&[0xFF, 0xDA]);
        out.extend_from_slice(&((6 + 2 * scan.comps.len()) as u16).to_be_bytes());
        out.push(scan.comps.len() as u8);
        for &c in &scan.comps {
            let s = slot(c) as u8;
            out.extend_from_slice(&[comps[c].id, (s << 4) | s]);
        }
        out.extend_from_slice(&[scan.ss as u8, scan.se as u8, ((scan.ah << 4) | scan.al) as u8]);
        let mut e = Emitter::new(false, tables, opts.progressive);
        run_scan(&mut e, &comps, scan, &grid);
        out.extend_from_slice(&e.w.out);
    }
    out.extend_from_slice(&[0xFF, 0xD9]);
    Ok(out)
}

struct EComp {
    id: u8,
    h: usize,
    v: usize,
    tq: usize,
    bw: usize,
    cbw: usize,
    cbh: usize,
    blocks: Vec<[i16; 64]>,
}

struct Grid {
    mcux: usize,
    mcuy: usize,
    restart: usize,
}

struct ScanDef {
    comps: Vec<usize>,
    ss: usize,
    se: usize,
    ah: u32,
    al: u32,
}

/// Annex K.1 luminance quantisation table (natural order).
const LUMA_Q: [u16; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56, 14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113, 92, 49, 64, 78,
    87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];

/// Annex K.1 chrominance quantisation table (natural order).
const CHROMA_Q: [u16; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99, 47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
];

fn scale_quant(base: &[u16; 64], quality: u8) -> [u16; 64] {
    let q = quality as u32;
    let scale = if q < 50 { 5000 / q } else { 200 - 2 * q };
    base.map(|b| ((b as u32 * scale + 50) / 100).clamp(1, 255) as u16)
}

/// `round(8192 · C(u) · cos((2x + 1)uπ/16))`, C(0) = 1/√2, as `COS[u][x]`.
const COS: [[i64; 8]; 8] = cos_table();

const fn cos_table() -> [[i64; 8]; 8] {
    // cos(kπ/16) · 8192 for k = 0..=8.
    const C: [i64; 9] = [8192, 8035, 7568, 6811, 5793, 4551, 3135, 1598, 0];
    let mut t = [[0i64; 8]; 8];
    let mut u = 0;
    while u < 8 {
        let mut x = 0;
        while x < 8 {
            t[u][x] = if u == 0 {
                5793
            } else {
                let mut m = ((2 * x + 1) * u) % 32;
                if m > 16 {
                    m = 32 - m;
                }
                if m <= 8 {
                    C[m]
                } else {
                    -C[16 - m]
                }
            };
            x += 1;
        }
        u += 1;
    }
    t
}

/// Rounds `a / b` to nearest (ties away from zero), `b > 0`.
fn div_round(a: i64, b: i64) -> i64 {
    if a >= 0 {
        (a + b / 2) / b
    } else {
        -((-a + b / 2) / b)
    }
}

/// Forward DCT of level-shifted samples (row-major) and quantisation, natural order.
fn fdct_quantize(s: &[i64; 64], q: &[u16; 64]) -> [i16; 64] {
    let mut rows = [0i64; 64]; // rows[y * 8 + u]
    for y in 0..8 {
        for u in 0..8 {
            rows[y * 8 + u] = (0..8).map(|x| s[y * 8 + x] * COS[u][x]).sum();
        }
    }
    let mut out = [0i16; 64];
    for v in 0..8 {
        for u in 0..8 {
            let sum: i64 = (0..8).map(|y| rows[y * 8 + u] * COS[v][y]).sum();
            // F = sum / (4 · 8192²); quantised = round(F / Q).
            out[v * 8 + u] = div_round(sum, (q[v * 8 + u] as i64) << 28).clamp(-32767, 32767) as i16;
        }
    }
    out
}

// Annex K.3 standard Huffman tables.
const STD_DC_LUMA_BITS: [u8; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
const STD_DC_CHROMA_BITS: [u8; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
const STD_DC_VALS: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
pub(super) const STD_AC_LUMA_BITS: [u8; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 0x7d];
pub(super) const STD_AC_LUMA_VALS: [u8; 162] = [
    0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xa1, 0x08, 0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52, 0xd1, 0xf0, 0x24, 0x33, 0x62,
    0x72, 0x82, 0x09, 0x0a, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57,
    0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99,
    0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8,
    0xd9, 0xda, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa,
];
pub(super) const STD_AC_CHROMA_BITS: [u8; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 0x77];
pub(super) const STD_AC_CHROMA_VALS: [u8; 162] = [
    0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71, 0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xa1, 0xb1, 0xc1, 0x09, 0x23, 0x33, 0x52, 0xf0, 0x15, 0x62, 0x72,
    0xd1, 0x0a, 0x16, 0x24, 0x34, 0xe1, 0x25, 0xf1, 0x17, 0x18, 0x19, 0x1a, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56,
    0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97,
    0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6,
    0xd7, 0xd8, 0xd9, 0xda, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa,
];

/// An encoding table: canonical code and length per symbol (length 0 = absent).
#[derive(Clone, Copy)]
struct EncTable {
    code: [u16; 256],
    size: [u8; 256],
}

impl Default for EncTable {
    fn default() -> Self {
        EncTable { code: [0; 256], size: [0; 256] }
    }
}

impl EncTable {
    fn from_spec(counts: &[u8; 16], vals: &[u8]) -> EncTable {
        let mut t = EncTable::default();
        let mut code = 0u32;
        let mut k = 0;
        for len in 1..=16u8 {
            for _ in 0..counts[len as usize - 1] {
                t.code[vals[k] as usize] = code as u16;
                t.size[vals[k] as usize] = len;
                code += 1;
                k += 1;
            }
            code <<= 1;
        }
        t
    }
}

/// An optimal length-limited Huffman table for `freq` (IJG `jpeg_gen_optimal_table`, the
/// procedure of Annex K.2): `(counts per length 1..=16, symbols)`.
fn optimal_table(freq_in: &[u32; 256]) -> ([u8; 16], Vec<u8>) {
    let mut freq = [0i64; 257];
    for (f, &x) in freq.iter_mut().zip(freq_in) {
        *f = x as i64;
    }
    freq[256] = 1; // reserves the all-ones code
    let mut codesize = [0usize; 257];
    let mut others = [-1i32; 257];
    loop {
        let mut c1: i32 = -1;
        let mut v = i64::MAX;
        for (i, &f) in freq.iter().enumerate() {
            if f != 0 && f <= v {
                v = f;
                c1 = i as i32;
            }
        }
        let mut c2: i32 = -1;
        v = i64::MAX;
        for (i, &f) in freq.iter().enumerate() {
            if f != 0 && f <= v && i as i32 != c1 {
                v = f;
                c2 = i as i32;
            }
        }
        if c2 < 0 {
            break;
        }
        let (mut a, mut b) = (c1 as usize, c2 as usize);
        freq[a] += freq[b];
        freq[b] = 0;
        codesize[a] += 1;
        while others[a] >= 0 {
            a = others[a] as usize;
            codesize[a] += 1;
        }
        others[a] = c2;
        codesize[b] += 1;
        while others[b] >= 0 {
            b = others[b] as usize;
            codesize[b] += 1;
        }
    }
    let mut bits = [0usize; 260];
    for &c in &codesize {
        if c > 0 {
            bits[c] += 1;
        }
    }
    for i in (17..bits.len()).rev() {
        while bits[i] > 0 {
            let mut j = i - 2;
            while bits[j] == 0 {
                j -= 1;
            }
            bits[i] -= 2;
            bits[i - 1] += 1;
            bits[j + 1] += 2;
            bits[j] -= 1;
        }
    }
    let mut i = 16;
    while bits[i] == 0 {
        i -= 1;
    }
    bits[i] -= 1; // drop the reserved code point
    let mut counts = [0u8; 16];
    for l in 1..=16 {
        counts[l - 1] = bits[l] as u8;
    }
    let mut vals = Vec::new();
    for len in 1..codesize.len() {
        for (s, &c) in codesize.iter().enumerate().take(256) {
            if c == len {
                vals.push(s as u8);
            }
        }
    }
    (counts, vals)
}

fn write_dht(out: &mut Vec<u8>, tables: &[(u8, u8, [u8; 16], Vec<u8>)]) {
    if tables.is_empty() {
        return;
    }
    let len: usize = 2 + tables.iter().map(|t| 17 + t.3.len()).sum::<usize>();
    out.extend_from_slice(&[0xFF, 0xC4]);
    out.extend_from_slice(&(len as u16).to_be_bytes());
    for (class, id, counts, vals) in tables {
        out.push((class << 4) | id);
        out.extend_from_slice(counts);
        out.extend_from_slice(vals);
    }
}

/// MSB-first bit writer with 0xFF00 byte stuffing.
#[derive(Default)]
struct BitWriter {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl BitWriter {
    fn put(&mut self, v: u32, n: u32) {
        if n == 0 {
            return;
        }
        self.acc = (self.acc << n) | (v as u64 & ((1u64 << n) - 1));
        self.n += n;
        while self.n >= 8 {
            let b = (self.acc >> (self.n - 8)) as u8;
            self.out.push(b);
            if b == 0xFF {
                self.out.push(0);
            }
            self.n -= 8;
        }
        self.acc &= (1u64 << self.n) - 1;
    }

    /// Pads the last byte with 1 bits.
    fn flush(&mut self) {
        if self.n > 0 {
            self.put((1 << (8 - self.n)) - 1, 8 - self.n);
        }
    }
}

/// Emits (or, when counting, tallies) the symbols and bits of one scan.
struct Emitter {
    counting: bool,
    progressive: bool,
    /// Symbol counts per table slot: DC0, DC1, AC0, AC1 (slot = class·2 + id).
    freq: [[u32; 256]; 4],
    tables: [EncTable; 4],
    w: BitWriter,
    eobrun: u32,
    /// Correction bits buffered behind a pending EOB run (refinement scans).
    be: Vec<u8>,
    ac_slot: usize,
}

impl Emitter {
    fn new(counting: bool, tables: [EncTable; 4], progressive: bool) -> Emitter {
        Emitter { counting, progressive, freq: [[0; 256]; 4], tables, w: BitWriter::default(), eobrun: 0, be: Vec::new(), ac_slot: 2 }
    }

    fn sym(&mut self, slot: usize, s: u8) {
        if self.counting {
            self.freq[slot][s as usize] += 1;
        } else {
            let t = &self.tables[slot];
            debug_assert!(t.size[s as usize] > 0, "symbol {s:#x} missing from table {slot}");
            self.w.put(t.code[s as usize] as u32, t.size[s as usize] as u32);
        }
    }

    fn bits(&mut self, v: u32, n: u32) {
        if !self.counting {
            self.w.put(v, n);
        }
    }

    fn emit_eobrun(&mut self) {
        if self.eobrun > 0 {
            let nb = 31 - self.eobrun.leading_zeros();
            self.sym(self.ac_slot, (nb << 4) as u8);
            self.bits(self.eobrun, nb);
            self.eobrun = 0;
            let be = std::mem::take(&mut self.be);
            for b in be {
                self.bits(b as u32, 1);
            }
        }
    }

    /// Category (bit count) and the bits of a signed value.
    fn magnitude(v: i32) -> (u32, u32) {
        let mag = v.unsigned_abs();
        let n = 32 - mag.leading_zeros();
        let bits = if v < 0 { (v - 1) as u32 } else { v as u32 };
        (n, bits & ((1u32 << n) - 1))
    }

    fn block(&mut self, blk: &[i16; 64], pred: &mut i32, scan: &ScanDef, dc: usize, ac: usize) {
        self.ac_slot = ac;
        let (ss, se, ah, al) = (scan.ss, scan.se, scan.ah, scan.al);
        if !self.progressive {
            let diff = blk[0] as i32 - *pred;
            *pred = blk[0] as i32;
            let (n, b) = Self::magnitude(diff);
            self.sym(dc, n as u8);
            self.bits(b, n);
            let mut r = 0u32;
            for k in 1..64 {
                let c = blk[ZIGZAG[k]] as i32;
                if c == 0 {
                    r += 1;
                    continue;
                }
                while r > 15 {
                    self.sym(ac, 0xF0);
                    r -= 16;
                }
                let (n, b) = Self::magnitude(c);
                self.sym(ac, ((r << 4) | n) as u8);
                self.bits(b, n);
                r = 0;
            }
            if r > 0 {
                self.sym(ac, 0x00);
            }
            return;
        }
        if ss == 0 {
            if ah == 0 {
                let v = blk[0] as i32 >> al;
                let (n, b) = Self::magnitude(v - *pred);
                *pred = v;
                self.sym(dc, n as u8);
                self.bits(b, n);
            } else {
                self.bits((blk[0] as i32 >> al) as u32 & 1, 1);
            }
            return;
        }
        if ah == 0 {
            let mut r = 0u32;
            for k in ss..=se {
                let c = blk[ZIGZAG[k]] as i32;
                let t = c.abs() >> al;
                if t == 0 {
                    r += 1;
                    continue;
                }
                self.emit_eobrun();
                while r > 15 {
                    self.sym(ac, 0xF0);
                    r -= 16;
                }
                let (n, b) = Self::magnitude(if c < 0 { -t } else { t });
                self.sym(ac, ((r << 4) | n) as u8);
                self.bits(b, n);
                r = 0;
            }
            if r > 0 {
                self.eobrun += 1;
                if self.eobrun == 0x7FFF {
                    self.emit_eobrun();
                }
            }
            return;
        }
        // AC refinement (IJG `encode_mcu_AC_refine`).
        let mut abs = [0i32; 64];
        let mut eob = 0;
        for k in ss..=se {
            abs[k] = (blk[ZIGZAG[k]] as i32).abs() >> al;
            if abs[k] == 1 {
                eob = k;
            }
        }
        let mut r = 0u32;
        let mut br: Vec<u8> = Vec::new();
        for k in ss..=se {
            let t = abs[k];
            if t == 0 {
                r += 1;
                continue;
            }
            while r > 15 && k <= eob {
                self.emit_eobrun();
                self.sym(ac, 0xF0);
                r -= 16;
                for b in br.drain(..) {
                    self.bits(b as u32, 1);
                }
            }
            if t > 1 {
                br.push((t & 1) as u8);
                continue;
            }
            self.emit_eobrun();
            self.sym(ac, ((r << 4) | 1) as u8);
            self.bits(u32::from(blk[ZIGZAG[k]] >= 0), 1);
            for b in br.drain(..) {
                self.bits(b as u32, 1);
            }
            r = 0;
        }
        if r > 0 || !br.is_empty() {
            self.eobrun += 1;
            self.be.extend_from_slice(&br);
            if self.eobrun == 0x7FFF || self.be.len() > 937 {
                self.emit_eobrun();
            }
        }
    }
}

fn run_scan(e: &mut Emitter, comps: &[EComp], scan: &ScanDef, g: &Grid) {
    let slot = |c: usize| usize::from(c > 0);
    let ns = scan.comps.len();
    let total = if ns > 1 { g.mcux * g.mcuy } else { comps[scan.comps[0]].cbw * comps[scan.comps[0]].cbh };
    let mut preds = [0i32; 4];
    let mut rst = 0u8;
    e.ac_slot = 2 + slot(scan.comps[0]);
    for m in 0..total {
        if g.restart > 0 && m > 0 && m % g.restart == 0 {
            e.emit_eobrun();
            if !e.counting {
                e.w.flush();
                e.w.out.extend_from_slice(&[0xFF, 0xD0 + rst]);
            }
            rst = (rst + 1) & 7;
            preds = [0; 4];
        }
        if ns > 1 {
            let (mx, my) = (m % g.mcux, m / g.mcux);
            for (k, &ci) in scan.comps.iter().enumerate() {
                let c = &comps[ci];
                for by in 0..c.v {
                    for bx in 0..c.h {
                        let b = &c.blocks[(my * c.v + by) * c.bw + mx * c.h + bx];
                        e.block(b, &mut preds[k], scan, slot(ci), 2 + slot(ci));
                    }
                }
            }
        } else {
            let ci = scan.comps[0];
            let c = &comps[ci];
            let b = &c.blocks[(m / c.cbw) * c.bw + m % c.cbw];
            e.block(b, &mut preds[0], scan, slot(ci), 2 + slot(ci));
        }
    }
    e.emit_eobrun();
    if !e.counting {
        e.w.flush();
    }
}
