//! Material texture helpers that run on the CPU (pure functions, safe on any thread): mip chains
//! and tangents.

use crate::overlay::check_pixels;
use crate::{GpuError, TextureFormat};
use std::sync::OnceLock;

/// Levels in a full mip chain of a `width × height` image (down to 1 × 1).
pub fn mip_count(width: u32, height: u32) -> u32 {
    32 - width.max(height).max(1).leading_zeros()
}

/// 8-bit sRGB → linear, exactly as the GPU decodes it.
fn srgb_lut() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| std::array::from_fn(|i| crate::render::srgb_to_linear(i as f32 / 255.0)))
}

/// Linear (quantised to 1/16384) → 8-bit sRGB, rounded.
const LINEAR_STEPS: usize = 16384;

fn linear_lut() -> &'static [u8] {
    static LUT: OnceLock<Vec<u8>> = OnceLock::new();
    LUT.get_or_init(|| {
        (0..=LINEAR_STEPS)
            .map(|i| {
                let l = i as f64 / LINEAR_STEPS as f64;
                let s = if l <= 0.003_130_8 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
                (s * 255.0).round().clamp(0.0, 255.0) as u8
            })
            .collect()
    })
}

/// The mip chain of an image: `levels[0]` is `pixels`, each next level half the size of the one
/// before (rounded down, at least 1 pixel), down to 1 × 1. Each texel is the mean of the 2 × 2
/// texels above it (edge texels repeat for odd sizes); sRGB colour is averaged in linear light,
/// alpha and other formats as stored. Deterministic: the same bytes everywhere.
pub fn generate_mips(width: u32, height: u32, format: TextureFormat, pixels: &[u8]) -> Result<Vec<Vec<u8>>, GpuError> {
    check_pixels("generate_mips", width, height, format, pixels)?;
    generate_mips_owned(width, height, format, pixels.to_vec())
}

/// [`generate_mips`] taking the image itself as level 0 (no copy: the chain adds a third).
pub fn generate_mips_owned(width: u32, height: u32, format: TextureFormat, pixels: Vec<u8>) -> Result<Vec<Vec<u8>>, GpuError> {
    check_pixels("generate_mips", width, height, format, &pixels)?;
    let bpp = format.bytes_per_pixel();
    let count = mip_count(width, height) as usize;
    let mut levels = Vec::with_capacity(count);
    levels.push(pixels);
    let (mut w, mut h) = (width as usize, height as usize);
    let (to_lin, to_srgb) = (srgb_lut(), linear_lut());
    for _ in 1..count {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let src = levels.last().expect("level 0 pushed");
        let mut dst = vec![0u8; nw * nh * bpp];
        for y in 0..nh {
            let (y0, y1) = ((2 * y).min(h - 1), (2 * y + 1).min(h - 1));
            for x in 0..nw {
                let (x0, x1) = ((2 * x).min(w - 1), (2 * x + 1).min(w - 1));
                let taps = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)].map(|(tx, ty)| (ty * w + tx) * bpp);
                for c in 0..bpp {
                    let out = &mut dst[(y * nw + x) * bpp + c];
                    if format == TextureFormat::Rgba8Srgb && c < 3 {
                        let sum: f32 = taps.iter().map(|&t| to_lin[src[t + c] as usize]).sum();
                        let i = ((sum * 0.25) * LINEAR_STEPS as f32 + 0.5) as usize;
                        *out = to_srgb[i.min(LINEAR_STEPS)];
                    } else {
                        let sum: u32 = taps.iter().map(|&t| src[t + c] as u32).sum();
                        *out = ((sum + 2) / 4) as u8;
                    }
                }
            }
        }
        levels.push(dst);
        (w, h) = (nw, nh);
    }
    Ok(levels)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Per-vertex tangents for normal mapping, in glTF's convention: xyz along +U on the surface
/// (orthogonal to the normal, unit length), w the bitangent's sign, where bitangent =
/// cross(normal, tangent) × w points along **−V**. glTF UVs have v growing down the image while
/// a normal map's +Y (green) means "towards the image's top row", so the bitangent follows −V
/// (what MikkTSpace gives when fed glTF's flipped V, as glTF viewers do). From the triangles' UV
/// gradients, area-weighted (the MikkTSpace idea without its seam splitting: vertices shared
/// across a UV mirror get an average). Vertices without a usable UV gradient get any unit vector
/// orthogonal to their normal. Lengths must agree (`positions`, `normals`, `uvs`); indices out
/// of range are skipped.
pub fn generate_tangents(positions: &[[f32; 3]], normals: &[[f32; 3]], uvs: &[[f32; 2]], indices: &[u32]) -> Vec<[f32; 4]> {
    let n = positions.len().min(normals.len()).min(uvs.len());
    let mut tan = vec![[0.0f32; 3]; positions.len()];
    let mut bit = vec![[0.0f32; 3]; positions.len()];
    for tri in indices.chunks_exact(3) {
        let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        if a >= n || b >= n || c >= n {
            continue;
        }
        let (e1, e2) = (sub(positions[b], positions[a]), sub(positions[c], positions[a]));
        let (du1, dv1) = (uvs[b][0] - uvs[a][0], uvs[b][1] - uvs[a][1]);
        let (du2, dv2) = (uvs[c][0] - uvs[a][0], uvs[c][1] - uvs[a][1]);
        let det = du1 * dv2 - du2 * dv1;
        if det.abs() < 1e-20 || !det.is_finite() {
            continue;
        }
        // Weighting by the UV-space determinant's sign only keeps mirrored triangles from
        // cancelling; the vectors' lengths already weight by area.
        let r = det.signum();
        let t = [(e1[0] * dv2 - e2[0] * dv1) * r, (e1[1] * dv2 - e2[1] * dv1) * r, (e1[2] * dv2 - e2[2] * dv1) * r];
        let bt = [(e2[0] * du1 - e1[0] * du2) * r, (e2[1] * du1 - e1[1] * du2) * r, (e2[2] * du1 - e1[2] * du2) * r];
        for v in [a, b, c] {
            for k in 0..3 {
                tan[v][k] += t[k];
                bit[v][k] += bt[k];
            }
        }
    }
    (0..positions.len())
        .map(|i| {
            let nrm = if i < normals.len() { normals[i] } else { [0.0, 1.0, 0.0] };
            let t = tan[i];
            // Gram–Schmidt against the normal.
            let o = sub(t, nrm.map(|v| v * dot(nrm, t)));
            let len = dot(o, o).sqrt();
            let t = if len > 1e-12 && len.is_finite() {
                o.map(|v| v / len)
            } else {
                // Any direction in the surface.
                let axis = if nrm[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
                let o = sub(axis, nrm.map(|v| v * dot(nrm, axis)));
                let l = dot(o, o).sqrt().max(1e-20);
                o.map(|v| v / l)
            };
            // `bit` follows +V; the bitangent must follow −V.
            let w = if dot(cross(nrm, t), bit[i]) > 0.0 { -1.0 } else { 1.0 };
            [t[0], t[1], t[2], w]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_counts() {
        assert_eq!(mip_count(1, 1), 1);
        assert_eq!(mip_count(2, 1), 2);
        assert_eq!(mip_count(256, 256), 9);
        assert_eq!(mip_count(300, 7), 9);
        assert_eq!(mip_count(1024, 4096), 13);
    }

    #[test]
    fn mips_halve_down_to_one_texel_and_average_in_linear_light() {
        // 4 × 2 sRGB: black and white columns; alpha 0 and 255.
        let mut px = Vec::new();
        for _y in 0..2 {
            for x in 0..4 {
                let v = if x % 2 == 0 { 0 } else { 255 };
                px.extend_from_slice(&[v, v, v, v]);
            }
        }
        let levels = generate_mips(4, 2, TextureFormat::Rgba8Srgb, &px).unwrap();
        assert_eq!(levels.iter().map(Vec::len).collect::<Vec<_>>(), vec![32, 8, 4]);
        // Half black, half white in linear light is sRGB 188 (not 128); alpha averages as stored.
        assert_eq!(&levels[1][..4], &[188, 188, 188, 128]);
        assert_eq!(&levels[2][..], &[188, 188, 188, 128]);
        // Unorm data averages as stored.
        let unorm = generate_mips(4, 2, TextureFormat::Rgba8Unorm, &px).unwrap();
        assert_eq!(&unorm[1][..4], &[128, 128, 128, 128]);
        // Odd sizes: 3 × 1 → 1 × 1 (edge texels repeat).
        let r8 = generate_mips(3, 1, TextureFormat::R8, &[0, 100, 200]).unwrap();
        assert_eq!(r8, vec![vec![0, 100, 200], vec![50]]);
        // Bad input is the usual structured error.
        assert!(generate_mips(2, 2, TextureFormat::R8, &[0; 3]).unwrap_err().reason.is_some());
        // Deterministic and exact for flat images.
        let flat = vec![77u8; 64 * 64 * 4];
        let levels = generate_mips(64, 64, TextureFormat::Rgba8Srgb, &flat).unwrap();
        assert!(levels.iter().all(|l| l.iter().all(|&v| v == 77)));
    }

    #[test]
    fn tangents_follow_u_with_handedness() {
        // A quad on XZ facing +Y; U along +X, V along +Z.
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]];
        let normals = [[0.0, 1.0, 0.0]; 4];
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let indices = [0, 2, 1, 0, 3, 2];
        let t = generate_tangents(&positions, &normals, &uvs, &indices);
        for v in &t {
            assert!((v[0] - 1.0).abs() < 1e-6 && v[1].abs() < 1e-6 && v[2].abs() < 1e-6, "{v:?}");
            // V runs along +Z, so the bitangent (−V) must point along −Z: cross(+Y, +X) = −Z
            // already does, so the sign is +1.
            assert_eq!(v[3], 1.0);
        }
        // Mirrored V (along −Z): the bitangent must point along +Z, so the sign flips.
        let flipped: Vec<[f32; 2]> = uvs.iter().map(|uv| [uv[0], 1.0 - uv[1]]).collect();
        assert!(generate_tangents(&positions, &normals, &flipped, &indices).iter().all(|v| v[3] == -1.0));
        // No UV gradient: still a unit vector orthogonal to the normal.
        let t = generate_tangents(&positions, &normals, &[[0.5, 0.5]; 4], &indices);
        for v in &t {
            let n = [0.0, 1.0, 0.0];
            assert!(dot([v[0], v[1], v[2]], n).abs() < 1e-6 && (dot([v[0], v[1], v[2]], [v[0], v[1], v[2]]) - 1.0).abs() < 1e-5);
        }
    }
}
