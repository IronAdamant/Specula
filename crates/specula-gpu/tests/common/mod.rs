//! Test helpers: camera matrices, a unit cube, sRGB maths and a minimal PNG writer.
//! (Kept local on purpose: specula-core's math/PNG are implemented separately.)

#![allow(dead_code)] // each test binary uses a different subset

use specula_gpu::{Gpu, GpuOptions, Severity, Validation, ValidationMessage, Vertex};
use std::path::PathBuf;

pub type V3 = [f32; 3];

pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub fn normalize(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

pub const IDENTITY: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// Column-major `a * b`.
pub fn mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut o = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

/// Right-handed view matrix (camera looks down −Z).
pub fn look_at_rh(eye: V3, target: V3, up: V3) -> [f32; 16] {
    let f = normalize(sub(target, eye));
    let s = normalize(cross(f, up));
    let u = cross(s, f);
    [
        s[0], u[0], -f[0], 0.0, //
        s[1], u[1], -f[1], 0.0, //
        s[2], u[2], -f[2], 0.0, //
        -dot(s, eye), -dot(u, eye), dot(f, eye), 1.0,
    ]
}

/// Right-handed perspective, Vulkan clip (y down, depth 0..1).
pub fn perspective_vk(fov_y: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (fov_y / 2.0).tan();
    [
        f / aspect, 0.0, 0.0, 0.0, //
        0.0, -f, 0.0, 0.0, //
        0.0, 0.0, far / (near - far), -1.0, //
        0.0, 0.0, near * far / (near - far), 0.0,
    ]
}

pub fn translation(t: V3) -> [f32; 16] {
    let mut m = IDENTITY;
    m[12] = t[0];
    m[13] = t[1];
    m[14] = t[2];
    m
}

pub fn rotation_y(radians: f32) -> [f32; 16] {
    let (s, c) = radians.sin_cos();
    [c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0]
}

/// World point → pixel (x right, y down) for a `w`×`h` image.
pub fn project(view_proj: &[f32; 16], p: V3, w: u32, h: u32) -> (f32, f32) {
    let m = view_proj;
    let clip: Vec<f32> = (0..4).map(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r]).collect();
    let (x, y) = (clip[0] / clip[3], clip[1] / clip[3]);
    ((x * 0.5 + 0.5) * w as f32, (y * 0.5 + 0.5) * h as f32)
}

/// Unit cube centred on the origin: 24 vertices (flat face normals), 36 indices.
pub fn cube() -> (Vec<Vertex>, Vec<u32>) {
    let faces: [(V3, V3, V3); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ];
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (n, u, v) in faces {
        let base = vertices.len() as u32;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = |k: usize| 0.5 * (n[k] + su * u[k] + sv * v[k]);
            vertices.push(Vertex { position: [p(0), p(1), p(2)], normal: n });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (vertices, indices)
}

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Expected sRGB byte for an sRGB input channel lit with Lambert factor `n·l`.
pub fn expected_lit(srgb: f32, n_dot_l: f32) -> u8 {
    let lit = srgb_to_linear(srgb) * (0.25 + 0.75 * n_dot_l.max(0.0));
    (linear_to_srgb(lit) * 255.0).round() as u8
}

pub fn light_dir() -> V3 {
    normalize([0.4, 0.8, 0.45])
}

pub fn pixel(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

/// A GPU with validation on when the layer exists (falls back to off, saying why), or `None`
/// (with the reason printed) when Vulkan is unavailable — callers then pass trivially.
pub fn gpu_or_skip(test: &str, adapter: specula_gpu::AdapterPreference) -> Option<Gpu> {
    if !specula_gpu::is_available() {
        eprintln!("{test}: SKIPPED — Vulkan is not available on this machine");
        return None;
    }
    match specula_gpu::enumerate_adapters() {
        Ok(a) if !a.is_empty() => {}
        other => {
            eprintln!("{test}: SKIPPED — no Vulkan adapters ({other:?})");
            return None;
        }
    }
    let gpu = match Gpu::new(&GpuOptions { validation: Validation::On, adapter }) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{test}: validation unavailable ({e}); running without it");
            Gpu::new(&GpuOptions { validation: Validation::Off, adapter }).expect("Gpu::new")
        }
    };
    Some(gpu)
}

/// Shut the GPU down, print every captured message (including teardown) and return the errors.
pub fn validation_errors(test: &str, gpu: Gpu) -> Vec<ValidationMessage> {
    let messages = gpu.close();
    for m in &messages {
        eprintln!("{test}: [{:?}] {} {}", m.severity, m.id_name, m.message);
    }
    messages.into_iter().filter(|m| m.severity == Severity::Error).collect()
}

/// Directory for test images: `$SPECULA_TEST_OUT` if set, else cargo's per-target temp dir.
pub fn out_dir() -> PathBuf {
    let dir = std::env::var_os("SPECULA_TEST_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")));
    std::fs::create_dir_all(&dir).expect("create test output dir");
    dir
}

// --- Minimal PNG writer: RGBA8, filter 0, zlib with stored (uncompressed) deflate blocks. ---

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

pub fn encode_png(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut raw = Vec::with_capacity((width as usize * 4 + 1) * height as usize);
    for row in rgba.chunks_exact(width as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push((i + 1 == blocks.len()) as u8); // BFINAL, BTYPE=00 (stored)
        let len = block.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA, deflate, filter 0, no interlace
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &z);
    chunk(&mut png, b"IEND", &[]);
    png
}

pub fn write_png(name: &str, rgba: &[u8], width: u32, height: u32) -> PathBuf {
    let path = out_dir().join(name);
    std::fs::write(&path, encode_png(rgba, width, height)).expect("write png");
    path
}

#[test]
fn crc_and_adler_known_values() {
    assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
}
