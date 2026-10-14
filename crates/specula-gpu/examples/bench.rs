//! GPU benchmark: renders fixed scenes offscreen for N frames and prints one JSON line per scene
//! with frame statistics (median / p95 GPU time, draw calls before and after instancing, …).
//!
//! `cargo run --release -p specula-gpu --example bench -- [--adapter discrete|integrated|<index>]
//! [--frames N] [--size WxH] [--validation]`
//!
//! Scenes (deterministic; a fixed seed places everything):
//! - `cubes_10k`: 10,000 lit cubes in a 100 × 100 grid, seen from above one edge (some culled).
//! - `mixed_1000`: 1,000 draws: lit and metal cubes and spheres, emissive cubes, glass spheres
//!   (blended, two passes each), blob shadows and a ground plane; 4 point lights, tone mapping.
//! - `lights_16`: a ground and back wall filling the frame plus 64 glossy spheres under 16 point
//!   lights, tone mapping: fragment-bound.
//! - `game`: the Sepulcra look at full quality: 2,000 instanced lit objects on a 40 m slab under
//!   a low moon, a 2048² shadow map fitted to the view, bloom (5 levels), height fog, the sky, FXAA and 200
//!   overlay quads (panels and alpha-masked glyphs). Budget: under ~8 ms GPU on the Intel iGPU
//!   at 1600×900.
//! - `game_low`: the same scene with the cheaper settings (a 1024² shadow map, 3 bloom levels,
//!   no FXAA): the fallback when `game` is over budget.
//! - `skinned_300`: 300 GPU-skinned creatures (20 bones, ~1.9k triangles each, every one in its
//!   own pose) on a ground under a low moon with a 2048² shadow map, fog and tone mapping: the
//!   M2 animation budget (300 creatures × 20 bones on the Intel iGPU).
//! - `textured_1000`: 1,000 textured cubes and spheres (base colour, normal and
//!   metallic-roughness maps with mip chains, trilinear + anisotropic sampling) under the same
//!   light and shadow.
//!
//! Each line has `passes`: median and p95 GPU ms per pass (shadow, scene, post_bloom, …), and the
//! render graph's passes, barriers per frame and compilations (`graph_*`; one compile per scene).
//!
//! Without Vulkan it prints `{"skipped": …}` and exits 0. Validation is off by default (it skews
//! CPU timings); `--validation` turns it on and reports the error count.

use specula_gpu::{
    AdapterPreference, DrawItem, Environment, FrameDesc, Gpu, GpuOptions, Material, MeshId, MeshStreams, OverlayKind, OverlayQuad,
    PointLight, Severity, SkinVertex, Surface, TextureFormat, TextureId, TextureSampler, Validation, Vertex,
};
use std::f32::consts::{PI, TAU};
use std::time::Instant;

const WARMUP_FRAMES: usize = 3;

// ---------------------------------------------------------------------------------------------
// Small maths and meshes (examples don't share code with the engine)
// ---------------------------------------------------------------------------------------------

type M4 = [f32; 16];

fn mul(a: &M4, b: &M4) -> M4 {
    let mut o = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
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

fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

fn look_at_rh(eye: [f32; 3], target: [f32; 3]) -> M4 {
    let f = normalize(sub(target, eye));
    let s = normalize(cross(f, [0.0, 1.0, 0.0]));
    let u = cross(s, f);
    [s[0], u[0], -f[0], 0.0, s[1], u[1], -f[1], 0.0, s[2], u[2], -f[2], 0.0, -dot(s, eye), -dot(u, eye), dot(f, eye), 1.0]
}

/// Reversed-Z, infinite far, Vulkan clip (the engine's projection).
fn perspective_reverse_z(fov_y_deg: f32, aspect: f32) -> M4 {
    let f = 1.0 / (fov_y_deg.to_radians() / 2.0).tan();
    [f / aspect, 0.0, 0.0, 0.0, 0.0, -f, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.05, 0.0]
}

/// Translation · rotation about +Y · scale.
fn trs(t: [f32; 3], yaw: f32, s: [f32; 3]) -> M4 {
    let (sn, cs) = yaw.sin_cos();
    [cs * s[0], 0.0, -sn * s[0], 0.0, 0.0, s[1], 0.0, 0.0, sn * s[2], 0.0, cs * s[2], 0.0, t[0], t[1], t[2], 1.0]
}

fn cube() -> (Vec<Vertex>, Vec<u32>) {
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ];
    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
    for (n, u, v) in faces {
        let base = vertices.len() as u32;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = |k: usize| 0.5 * (n[k] + su * u[k] + sv * v[k]);
            vertices.push(Vertex::new([p(0), p(1), p(2)], n));
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (vertices, indices)
}

/// UV sphere of radius 0.5.
fn sphere(segments: u32, rings: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    for r in 0..=rings {
        let phi = PI * r as f32 / rings as f32;
        for s in 0..=segments {
            let theta = TAU * s as f32 / segments as f32;
            let n = [phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin()];
            vertices.push(Vertex::new(n.map(|c| c * 0.5), n));
        }
    }
    let row = segments + 1;
    let mut indices = Vec::new();
    for r in 0..rings {
        for s in 0..segments {
            let a = r * row + s;
            let b = a + row;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    (vertices, indices)
}

/// Unit quad on XZ, facing +Y.
fn quad() -> (Vec<Vertex>, Vec<u32>) {
    let n = [0.0, 1.0, 0.0];
    let vertices = [[-0.5, 0.0, 0.5], [0.5, 0.0, 0.5], [0.5, 0.0, -0.5], [-0.5, 0.0, -0.5]].map(|position| Vertex::new(position, n));
    (vertices.to_vec(), vec![0, 1, 2, 0, 2, 3])
}

/// Bones of the bench creature, one above the other.
const CREATURE_BONES: usize = 20;
const SEGMENT: f32 = 0.12;

/// A skinned creature: a tapering tube up a chain of [`CREATURE_BONES`] bones (bone i's joint at
/// y = i × SEGMENT), 16 sides, 3 rings per bone, each ring blending the bones around it.
fn creature() -> (Vec<Vertex>, Vec<u32>, Vec<SkinVertex>) {
    let (sides, rings) = (16usize, CREATURE_BONES * 3 + 1);
    let mut vertices = Vec::new();
    let mut skin = Vec::new();
    for r in 0..rings {
        let y = r as f32 / 3.0 * SEGMENT;
        let t = r as f32 / (rings - 1) as f32;
        let radius = 0.16 * (1.0 - 0.7 * t) + 0.04 * (t * 12.0).sin().abs();
        // Between joint b and b + 1: blend linearly by height.
        let f = y / SEGMENT;
        let b = (f.floor() as usize).min(CREATURE_BONES - 1);
        let w1 = ((f - b as f32) * 255.0).round().clamp(0.0, 255.0) as u8;
        let (joints, weights) = if b + 1 < CREATURE_BONES { ([b as u8, b as u8 + 1, 0, 0], [255 - w1, w1, 0, 0]) } else { ([b as u8, 0, 0, 0], [255, 0, 0, 0]) };
        for s in 0..=sides {
            let a = TAU * s as f32 / sides as f32;
            let n = [a.cos(), 0.0, a.sin()];
            vertices.push(Vertex::with_color([n[0] * radius, y, n[2] * radius], n, [220, 210, 190, 255]));
            skin.push(SkinVertex { joints, weights });
        }
    }
    let row = (sides + 1) as u32;
    let mut indices = Vec::new();
    for r in 0..rings as u32 - 1 {
        for s in 0..sides as u32 {
            let a = r * row + s;
            let b = a + row;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    (vertices, indices, skin)
}

/// The skinning matrices of a creature swaying with `phase`: bone i bends by a travelling wave
/// about X and Z; matrix = pose · rest⁻¹ (rest = translation up the chain).
fn creature_pose(phase: f32, out: &mut Vec<[f32; 16]>) {
    let mut world: M4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    for i in 0..CREATURE_BONES {
        let (ax, az) = ((phase + i as f32 * 0.45).sin() * 0.16, (phase * 0.7 + i as f32 * 0.3).cos() * 0.1);
        let (sx, cx) = ax.sin_cos();
        let (sz, cz) = az.sin_cos();
        let rx: M4 = [1.0, 0.0, 0.0, 0.0, 0.0, cx, sx, 0.0, 0.0, -sx, cx, 0.0, 0.0, 0.0, 0.0, 1.0];
        let rz: M4 = [cz, sz, 0.0, 0.0, -sz, cz, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let up = if i == 0 { 0.0 } else { SEGMENT };
        let t: M4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, up, 0.0, 1.0];
        world = mul(&mul(&world, &t), &mul(&rx, &rz));
        let rest_inv: M4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -(i as f32) * SEGMENT, 0.0, 1.0];
        out.push(mul(&world, &rest_inv));
    }
}

/// The cube with per-face UVs (0..1 on every face).
fn cube_uv() -> (Vec<Vertex>, Vec<u32>, Vec<[f32; 2]>) {
    let (vertices, indices) = cube();
    let uvs = (0..vertices.len()).map(|i| [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]][i % 4]).collect();
    (vertices, indices, uvs)
}

/// The UV sphere with its longitude/latitude UVs (2 × around, 1 × down).
fn sphere_uv(segments: u32, rings: u32) -> (Vec<Vertex>, Vec<u32>, Vec<[f32; 2]>) {
    let (vertices, indices) = sphere(segments, rings);
    let uvs = (0..=rings).flat_map(|r| (0..=segments).map(move |s| [2.0 * s as f32 / segments as f32, r as f32 / rings as f32])).collect();
    (vertices, indices, uvs)
}

/// 256² stone tiles: base colour (sRGB), a matching normal map and metallic-roughness (Unorm).
fn tile_textures() -> [Vec<u8>; 3] {
    const N: usize = 256;
    let mut rng = Rng(9);
    let noise: Vec<f32> = (0..N * N).map(|_| rng.next()).collect();
    let height = |x: usize, y: usize| {
        let (tx, ty) = (x % 64, y % 64);
        let edge = (tx.min(63 - tx).min(ty.min(63 - ty)) as f32 / 4.0).min(1.0);
        edge * (0.8 + 0.2 * noise[(y % N) * N + (x % N)])
    };
    let (mut base, mut normal, mut mr) = (Vec::with_capacity(N * N * 4), Vec::with_capacity(N * N * 4), Vec::with_capacity(N * N * 4));
    for y in 0..N {
        for x in 0..N {
            let h = height(x, y);
            let tone = (110.0 + 90.0 * h) as u8;
            base.extend_from_slice(&[tone, (tone as f32 * 0.95) as u8, (tone as f32 * 0.88) as u8, 255]);
            let dx = height(x + 1, y) - height(x + N - 1, y);
            let dy = height(x, y + 1) - height(x, y + N - 1);
            // +Y is the image top (glTF): the slope along +Y is −dy.
            let n = normalize([-dx * 2.0, dy * 2.0, 1.0]);
            normal.extend_from_slice(&[((n[0] * 0.5 + 0.5) * 255.0) as u8, ((n[1] * 0.5 + 0.5) * 255.0) as u8, ((n[2] * 0.5 + 0.5) * 255.0) as u8, 255]);
            mr.extend_from_slice(&[255, (255.0 * (0.5 + 0.5 * h)) as u8, if h < 0.5 { 200 } else { 0 }, 255]);
        }
    }
    [base, normal, mr]
}

fn moonlit() -> Environment {
    let mut environment = Environment {
        sun_direction: [-0.6, 0.5, 0.35],
        sun_color: [0.79, 0.84, 1.0],
        sun_intensity: 0.9,
        ambient_color: [0.165, 0.204, 0.282],
        ambient_intensity: 1.0,
        exposure: 1.1,
        tonemap: true,
        ..Default::default()
    };
    environment.shadow.enabled = true;
    environment.fog.enabled = true;
    environment.fog.height = -4.0;
    environment.fog.density = 0.04;
    environment.fog.falloff = 0.3;
    environment
}

fn skinned_300(m: &Meshes) -> Scene {
    let mut rng = Rng(6);
    let mut draws = vec![DrawItem::new(m.quad, trs([0.0; 3], 0.0, [40.0, 1.0, 40.0]), hex(0x5b6470))];
    let mut bones = Vec::with_capacity(300 * CREATURE_BONES);
    for i in 0..300 {
        let p = [(i % 20) as f32 * 1.6 - 15.2 + rng.range(-0.3, 0.3), 0.0, (i / 20) as f32 * 1.6 - 11.2 + rng.range(-0.3, 0.3)];
        let mut d = DrawItem::new(m.creature, trs(p, rng.range(0.0, TAU), [1.0; 3]), hex(0xd8cfb8));
        d.surface = Surface { roughness: 0.6, specular: 0.4, ..Default::default() };
        d.skin = Some(bones.len() as u32);
        // The chain reaches at most 2.4 m from its root: a posed bound for culling and shadows.
        d.bounds = Some([p[0], 1.2, p[2], 2.6]);
        creature_pose(rng.range(0.0, TAU), &mut bones);
        draws.push(d);
    }
    Scene {
        name: "skinned_300",
        draws,
        overlay: Vec::new(),
        lights: Vec::new(),
        environment: moonlit(),
        clear: [0.04, 0.06, 0.13, 1.0],
        eye: [0.0, 20.0, 26.0],
        target: [0.0, 0.0, 0.0],
        bones,
        fov_y_deg: 50.0,
    }
}

fn textured_1000(m: &Meshes) -> Scene {
    let mut rng = Rng(7);
    let [base, normal, mr] = m.textures;
    let textures = specula_gpu::MaterialTextures { base_color: Some(base), normal: Some(normal), metallic_roughness: Some(mr), ..Default::default() };
    let look = Surface { roughness: 1.0, metallic: 1.0, specular: 1.0, textures, ..Default::default() };
    let mut ground = DrawItem::new(m.cube_uv, trs([0.0, -0.5, 0.0], 0.0, [40.0, 1.0, 40.0]), [1.0; 4]);
    ground.surface = look;
    let mut draws = vec![ground];
    for i in 0..1000 {
        let p = [rng.range(-19.0, 19.0), 0.0, rng.range(-19.0, 19.0)];
        let s = rng.range(0.5, 1.4);
        let mesh = if i % 2 == 0 { m.cube_uv } else { m.sphere_uv };
        let mut d = DrawItem::new(mesh, trs([p[0], s / 2.0, p[2]], rng.range(0.0, TAU), [s; 3]), rng.colour());
        d.surface = look;
        draws.push(d);
    }
    Scene {
        name: "textured_1000",
        draws,
        overlay: Vec::new(),
        lights: Vec::new(),
        environment: moonlit(),
        clear: [0.04, 0.06, 0.13, 1.0],
        eye: [0.0, 14.0, 28.0],
        target: [0.0, 0.0, 0.0],
        bones: Vec::new(),
        fov_y_deg: 50.0,
    }
}

/// Deterministic values in [0, 1) (SplitMix64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }

    fn colour(&mut self) -> [f32; 4] {
        [self.range(0.2, 0.95), self.range(0.2, 0.95), self.range(0.2, 0.95), 1.0]
    }
}

// ---------------------------------------------------------------------------------------------
// Scenes
// ---------------------------------------------------------------------------------------------

struct Meshes {
    cube: MeshId,
    sphere: MeshId,
    quad: MeshId,
    /// A 20-bone skinned creature (`creature()`).
    creature: MeshId,
    /// The cube and sphere with UVs (textured draws).
    cube_uv: MeshId,
    sphere_uv: MeshId,
    /// Base colour, normal and metallic-roughness textures for `textured_1000`.
    textures: [TextureId; 3],
}

struct Scene {
    name: &'static str,
    draws: Vec<DrawItem>,
    overlay: Vec<OverlayQuad>,
    lights: Vec<PointLight>,
    environment: Environment,
    clear: [f32; 4],
    eye: [f32; 3],
    target: [f32; 3],
    fov_y_deg: f32,
    /// Skinning matrices of the skinned draws.
    bones: Vec<[f32; 16]>,
}

fn with(mut d: DrawItem, material: Material, surface: Surface) -> DrawItem {
    d.material = material;
    d.surface = surface;
    d
}

fn cubes_10k(m: &Meshes) -> Scene {
    let mut rng = Rng(1);
    let mut draws = Vec::with_capacity(10_000);
    for i in 0..10_000 {
        let (x, z) = ((i % 100) as f32 - 49.5, (i / 100) as f32 - 49.5);
        let h = rng.range(0.3, 1.2);
        draws.push(DrawItem::new(m.cube, trs([x, h / 2.0, z], rng.range(0.0, TAU), [0.6, h, 0.6]), rng.colour()));
    }
    Scene {
        name: "cubes_10k",
        draws,
        overlay: Vec::new(),
        lights: Vec::new(),
        environment: Environment::default(),
        clear: [0.55, 0.65, 0.8, 1.0],
        eye: [0.0, 22.0, 62.0],
        target: [0.0, 0.0, -5.0],
        bones: Vec::new(),
        fov_y_deg: 55.0,
    }
}

fn mixed_1000(m: &Meshes) -> Scene {
    let mut rng = Rng(2);
    let mut draws = Vec::with_capacity(1000);
    let spot = |rng: &mut Rng| [rng.range(-20.0, 20.0), 0.0, rng.range(-20.0, 20.0)];
    draws.push(DrawItem::new(m.quad, trs([0.0; 3], 0.0, [50.0, 1.0, 50.0]), [0.35, 0.4, 0.3, 1.0]));
    for _ in 0..299 {
        let p = spot(&mut rng);
        let s = rng.range(0.4, 1.4);
        draws.push(DrawItem::new(m.cube, trs([p[0], s / 2.0, p[2]], rng.range(0.0, TAU), [s; 3]), rng.colour()));
    }
    for _ in 0..200 {
        let p = spot(&mut rng);
        let s = rng.range(0.4, 1.2);
        let look = Surface { metallic: rng.range(0.0, 1.0), roughness: rng.range(0.15, 0.8), specular: 1.0, ..Default::default() };
        draws.push(with(DrawItem::new(m.sphere, trs([p[0], s / 2.0, p[2]], 0.0, [s; 3]), rng.colour()), Material::Lit, look));
    }
    for _ in 0..80 {
        let p = spot(&mut rng);
        let look = Surface { emissive: rng.range(0.5, 3.0), ..Default::default() };
        draws.push(with(DrawItem::new(m.cube, trs([p[0], 0.25, p[2]], 0.0, [0.5; 3]), rng.colour()), Material::Lit, look));
    }
    for _ in 0..150 {
        let p = spot(&mut rng);
        let s = rng.range(0.6, 1.6);
        let mut c = rng.colour();
        c[3] = rng.range(0.1, 0.4);
        draws.push(with(DrawItem::new(m.sphere, trs([p[0], s / 2.0 + 0.01, p[2]], 0.0, [s; 3]), c), Material::Glass, Surface::default()));
        if draws.len() % 3 == 0 {
            // Some glass gets a contact shadow.
            let shadow = DrawItem::new(m.quad, trs([p[0], 0.002, p[2]], 0.0, [s * 1.3, 1.0, s * 1.3]), [0.0, 0.0, 0.0, 0.5]);
            draws.push(with(shadow, Material::BlobShadow, Surface::default()));
        }
    }
    while draws.len() < 1000 {
        let p = spot(&mut rng);
        let shadow = DrawItem::new(m.quad, trs([p[0], 0.002, p[2]], 0.0, [1.0, 1.0, 1.0]), [0.0, 0.0, 0.0, 0.5]);
        draws.push(with(shadow, Material::BlobShadow, Surface::default()));
    }
    draws.truncate(1000);
    let lights = (0..4)
        .map(|i| {
            let a = i as f32 * TAU / 4.0;
            PointLight { position: [10.0 * a.cos(), 3.0, 10.0 * a.sin()], color: [1.0, 0.8, 0.6], intensity: 20.0, range: 18.0 }
        })
        .collect();
    let environment = Environment { tonemap: true, exposure: 1.2, ..Default::default() };
    Scene {
        name: "mixed_1000",
        draws,
        overlay: Vec::new(),
        lights,
        environment,
        clear: [0.1, 0.12, 0.16, 1.0],
        eye: [0.0, 14.0, 30.0],
        target: [0.0, 0.0, 0.0],
        bones: Vec::new(),
        fov_y_deg: 55.0,
    }
}

/// Hex sRGB colour.
fn hex(rgb: u32) -> [f32; 4] {
    [((rgb >> 16) & 255) as f32 / 255.0, ((rgb >> 8) & 255) as f32 / 255.0, (rgb & 255) as f32 / 255.0, 1.0]
}

fn game(m: &Meshes, atlas: TextureId, low: bool) -> Scene {
    let mut rng = Rng(4);
    let mut draws = Vec::with_capacity(2001);
    // The slab: a 40 × 40 m top and 15 m cliffs.
    draws.push(DrawItem::new(m.cube, trs([0.0, -7.5, 0.0], 0.0, [40.0, 15.0, 40.0]), hex(0x5b6470)));
    let stone = Surface { roughness: 0.8, specular: 0.3, ..Default::default() };
    for i in 0..2000 {
        let p = [rng.range(-19.0, 19.0), 0.0, rng.range(-19.0, 19.0)];
        let d = match i % 10 {
            // Glowing runes and grave lights: emissive (they bloom).
            0 => {
                let c = if i % 20 == 0 { hex(0x3fe0d0) } else { hex(0x9cff4f) };
                with(DrawItem::new(m.sphere, trs([p[0], 0.4, p[2]], 0.0, [0.3; 3]), c), Material::Lit, Surface { emissive: 2.5, ..stone })
            }
            // Stones and tombstones.
            1..=5 => {
                let h = rng.range(0.6, 2.4);
                let s = [rng.range(0.4, 1.0), h, rng.range(0.15, 1.0)];
                with(DrawItem::new(m.cube, trs([p[0], h / 2.0, p[2]], rng.range(0.0, TAU), s), hex(0x8c96a3)), Material::Lit, stone)
            }
            // Undead: bone-coloured spheres.
            _ => {
                let s = rng.range(0.5, 1.0);
                with(DrawItem::new(m.sphere, trs([p[0], s / 2.0, p[2]], 0.0, [s, s * 1.7, s]), hex(0xd8cfb8)), Material::Lit, stone)
            }
        };
        draws.push(d);
    }
    // HUD: two panels, 196 glyphs (alpha-masked from the atlas) and two bars.
    let mut overlay = vec![
        OverlayQuad { radius: 10.0, ..OverlayQuad::solid([24.0, 24.0, 520.0, 120.0], [0.17, 0.19, 0.22, 0.85]) },
        OverlayQuad { radius: 10.0, ..OverlayQuad::solid([1056.0, 756.0, 520.0, 120.0], [0.17, 0.19, 0.22, 0.85]) },
    ];
    for k in 0..196 {
        let (col, row) = (k % 49, k / 49);
        let (gx, gy) = ((k * 7) % 16, (k * 3) % 16);
        let uv = [gx as f32 / 16.0, gy as f32 / 16.0, (gx + 1) as f32 / 16.0, (gy + 1) as f32 / 16.0];
        let (x, y) = if row < 2 { (40.0 + col as f32 * 10.0, 40.0 + row as f32 * 22.0) } else { (1072.0 + col as f32 * 10.0, 772.0 + (row - 2) as f32 * 22.0) };
        overlay.push(OverlayQuad::textured([x, y, 10.0, 18.0], uv, atlas, OverlayKind::AlphaMask, hex(0xece6d6)));
    }
    overlay.push(OverlayQuad { radius: 4.0, ..OverlayQuad::solid([40.0, 100.0, 300.0, 12.0], hex(0x3fe0d0)) });
    overlay.push(OverlayQuad { radius: 4.0, ..OverlayQuad::solid([1072.0, 832.0, 220.0, 12.0], hex(0x9cff4f)) });
    debug_assert_eq!(overlay.len(), 200);

    let mut environment = Environment {
        sun_direction: [-0.6, 0.5, 0.35],
        sun_color: [0.79, 0.84, 1.0],
        sun_intensity: 0.9,
        ambient_color: [0.165, 0.204, 0.282],
        ambient_intensity: 1.0,
        exposure: 1.1,
        tonemap: true,
        ..Default::default()
    };
    environment.shadow.enabled = true;
    environment.bloom.enabled = true;
    environment.fog.enabled = true;
    environment.fog.height = -4.0;
    environment.fog.density = 0.04;
    environment.fog.falloff = 0.3;
    environment.sky.enabled = true;
    environment.fxaa = !low;
    if low {
        environment.shadow.cascades = 1;
        environment.shadow.resolution = 1024;
        environment.bloom.levels = 3;
    }
    Scene {
        name: if low { "game_low" } else { "game" },
        draws,
        overlay,
        lights: Vec::new(),
        environment,
        clear: [0.04, 0.06, 0.13, 1.0],
        eye: [0.0, 32.0, 42.0],
        target: [0.0, 0.0, 2.0],
        bones: Vec::new(),
        fov_y_deg: 50.0,
    }
}

/// A 256 × 256 R8 "glyph atlas": 16 × 16 cells of blocky shapes.
fn atlas() -> Vec<u8> {
    let mut rng = Rng(5);
    let mut px = vec![0u8; 256 * 256];
    for cell in 0..256 {
        let (cx, cy) = ((cell % 16) * 16, (cell / 16) * 16);
        for _ in 0..3 {
            let (x0, y0) = (cx + 2 + (rng.next() * 6.0) as usize, cy + 2 + (rng.next() * 6.0) as usize);
            let (w, h) = (2 + (rng.next() * 6.0) as usize, 2 + (rng.next() * 6.0) as usize);
            for y in y0..(y0 + h).min(cy + 15) {
                for x in x0..(x0 + w).min(cx + 15) {
                    px[y * 256 + x] = 255;
                }
            }
        }
    }
    px
}

fn lights_16(m: &Meshes) -> Scene {
    let mut rng = Rng(3);
    let mut draws = Vec::new();
    let ground = Surface { specular: 0.6, roughness: 0.35, ..Default::default() };
    draws.push(with(DrawItem::new(m.quad, trs([0.0; 3], 0.0, [80.0, 1.0, 80.0]), [0.5, 0.5, 0.52, 1.0]), Material::Lit, ground));
    // The XZ quad stood up facing +Z: 80 m wide, 40 m tall, its bottom edge on the ground.
    let wall = [80.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -40.0, 0.0, 0.0, 0.0, 20.0, -12.0, 1.0];
    draws.push(with(DrawItem::new(m.quad, wall, [0.45, 0.42, 0.4, 1.0]), Material::Lit, ground));
    for i in 0..64 {
        let (x, z) = ((i % 8) as f32 * 3.0 - 10.5, (i / 8) as f32 * 2.5 - 9.0);
        let look = Surface { metallic: rng.range(0.0, 1.0), roughness: rng.range(0.1, 0.6), specular: 1.0, ..Default::default() };
        draws.push(with(DrawItem::new(m.sphere, trs([x, 0.7, z], 0.0, [1.4; 3]), rng.colour()), Material::Lit, look));
    }
    let lights = (0..16)
        .map(|i| {
            let a = i as f32 * TAU / 16.0;
            let c = [0.5 + 0.5 * a.cos(), 0.5 + 0.5 * (a + 2.1).cos(), 0.5 + 0.5 * (a + 4.2).cos()];
            PointLight { position: [12.0 * a.cos(), 2.0, 10.0 * a.sin() - 2.0], color: c, intensity: 6.0, range: 14.0 }
        })
        .collect();
    let environment =
        Environment { tonemap: true, exposure: 1.0, sun_intensity: 0.1, ambient_intensity: 0.05, ..Default::default() };
    Scene {
        name: "lights_16",
        draws,
        overlay: Vec::new(),
        lights,
        environment,
        clear: [0.02, 0.02, 0.03, 1.0],
        eye: [0.0, 6.0, 16.0],
        target: [0.0, 0.5, -2.0],
        bones: Vec::new(),
        fov_y_deg: 60.0,
    }
}

// ---------------------------------------------------------------------------------------------
// Running
// ---------------------------------------------------------------------------------------------

struct Args {
    adapter: AdapterPreference,
    frames: usize,
    size: (u32, u32),
    validation: bool,
    /// Write each scene's last frame here as `<scene>.png`.
    png: Option<std::path::PathBuf>,
    /// Only these scenes (comma-separated names).
    scenes: Option<Vec<String>>,
    /// Features to switch off in every scene: shadow, bloom, fog, sky, fxaa, overlay, skinning, textures, anisotropy.
    without: Vec<String>,
    /// Shadow overrides for scenes with shadows: cascades, resolution, softness.
    cascades: Option<u32>,
    shadow_res: Option<u32>,
    softness: Option<f32>,
    bloom_levels: Option<u32>,
    /// Effects to add: `ssao`, `grading`.
    with: Vec<String>,
    ssao_res: Option<f32>,
    ssao_samples: Option<u32>,
}

const USAGE: &str =
    "usage: bench [--adapter discrete|integrated|<index>] [--frames N] [--size WxH] [--validation] [--png DIR] [--scenes a,b] [--without shadow,bloom,fog,sky,fxaa,overlay,skinning,textures,anisotropy]\n\
     [--cascades N] [--shadow-res N] [--softness F] [--bloom-levels N] [--with ssao,grading] [--ssao-res F] [--ssao-samples N]";

fn parse_args() -> Result<Args, String> {
    let mut args =
        Args { adapter: AdapterPreference::HighPerformance, frames: 120, size: (1600, 900), validation: false, png: None,
        scenes: None,
        without: Vec::new(),
        cascades: None,
        shadow_res: None,
        softness: None,
        bloom_levels: None,
        with: Vec::new(),
        ssao_res: None,
        ssao_samples: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{flag} needs a value\n{USAGE}"));
        match flag.as_str() {
            "--adapter" => {
                args.adapter = match value()?.as_str() {
                    "discrete" => AdapterPreference::HighPerformance,
                    "integrated" => AdapterPreference::LowPower,
                    n => AdapterPreference::Index(n.parse().map_err(|_| format!("bad adapter '{n}'\n{USAGE}"))?),
                }
            }
            "--frames" => {
                args.frames = value()?.parse().ok().filter(|&n| n > 0).ok_or_else(|| format!("bad frame count\n{USAGE}"))?
            }
            "--size" => {
                let v = value()?;
                let parsed = v.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)));
                args.size = parsed.filter(|&(w, h)| w > 0 && h > 0).ok_or_else(|| format!("bad size '{v}'\n{USAGE}"))?;
            }
            "--validation" => args.validation = true,
            "--png" => args.png = Some(value()?.into()),
            "--scenes" => args.scenes = Some(value()?.split(',').map(str::to_string).collect()),
            "--without" => args.without = value()?.split(',').map(str::to_string).collect(),
            "--cascades" => args.cascades = Some(value()?.parse().map_err(|_| format!("bad cascades\n{USAGE}"))?),
            "--shadow-res" => args.shadow_res = Some(value()?.parse().map_err(|_| format!("bad resolution\n{USAGE}"))?),
            "--softness" => args.softness = Some(value()?.parse().map_err(|_| format!("bad softness\n{USAGE}"))?),
            "--bloom-levels" => args.bloom_levels = Some(value()?.parse().map_err(|_| format!("bad levels\n{USAGE}"))?),
            "--with" => args.with = value()?.split(',').map(str::to_string).collect(),
            "--ssao-res" => args.ssao_res = Some(value()?.parse().map_err(|_| format!("bad SSAO resolution\n{USAGE}"))?),
            "--ssao-samples" => args.ssao_samples = Some(value()?.parse().map_err(|_| format!("bad SSAO samples\n{USAGE}"))?),
            "--help" | "-h" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown argument '{other}'\n{USAGE}")),
        }
    }
    Ok(args)
}

/// RGBA8 → PNG with stored (uncompressed) deflate blocks: big, but simple and dependency-free.
fn encode_png(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    fn crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    }
    let mut raw = Vec::with_capacity((width as usize * 4 + 1) * height as usize);
    for row in rgba.chunks_exact(width as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push((i + 1 == blocks.len()) as u8);
        z.extend_from_slice(&(block.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        z.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &z);
    chunk(&mut png, b"IEND", &[]);
    png
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// (median, p95) of `v` (sorted in place); NaN when empty.
fn median_p95(v: &mut [f64]) -> (f64, f64) {
    if v.is_empty() {
        return (f64::NAN, f64::NAN);
    }
    v.sort_by(f64::total_cmp);
    let at = |q: f64| v[(((v.len() as f64) * q).ceil() as usize).clamp(1, v.len()) - 1];
    (at(0.5), at(0.95))
}

fn num(x: f64) -> String {
    if x.is_finite() {
        format!("{x:.4}")
    } else {
        "null".into()
    }
}

fn run_scene(gpu: &mut Gpu, args: &Args, scene: &Scene) -> Result<String, specula_gpu::GpuError> {
    let (w, h) = args.size;
    let target = gpu.create_target(w, h)?;
    let view_proj = mul(&perspective_reverse_z(scene.fov_y_deg, w as f32 / h as f32), &look_at_rh(scene.eye, scene.target));
    let (mut gpu_ms, mut record_ms, mut wall_ms) = (Vec::new(), Vec::new(), Vec::new());
    // Per pass name, in first-seen order.
    let mut passes: Vec<(&'static str, Vec<f64>)> = Vec::new();
    let mut last = None;
    let off = |f: &str| args.without.iter().any(|w| w == f);
    let mut environment = scene.environment;
    environment.shadow.enabled &= !off("shadow");
    environment.bloom.enabled &= !off("bloom");
    environment.fog.enabled &= !off("fog");
    environment.sky.enabled &= !off("sky");
    environment.fxaa &= !off("fxaa");
    let s = &mut environment.shadow;
    s.cascades = args.cascades.unwrap_or(s.cascades);
    s.resolution = args.shadow_res.unwrap_or(s.resolution);
    s.softness = args.softness.unwrap_or(s.softness);
    environment.bloom.levels = args.bloom_levels.unwrap_or(environment.bloom.levels);
    let with = |f: &str| args.with.iter().any(|w| w == f);
    if with("ssao") {
        let s = &mut environment.ssao;
        s.enabled = true;
        s.resolution = args.ssao_res.unwrap_or(s.resolution);
        s.samples = args.ssao_samples.unwrap_or(s.samples);
    }
    if with("grading") {
        // The Sepulcra grade (games/sepulcra-app night()).
        let g = &mut environment.grading;
        (g.enabled, g.vibrance, g.lift, g.gain, g.vignette, g.vignette_radius) =
            (true, -1.0, [-0.01, -0.004, 0.012], [1.03, 1.0, 0.97], 0.25, 0.55);
    }
    let overlay: &[OverlayQuad] = if off("overlay") { &[] } else { &scene.overlay };
    // Without skinning every creature draws its bind pose; without textures every material is flat.
    let stripped: Vec<DrawItem>;
    let draws: &[DrawItem] = if off("skinning") || off("textures") {
        stripped = scene
            .draws
            .iter()
            .map(|d| {
                let mut d = *d;
                if off("skinning") {
                    d.skin = None;
                }
                if off("textures") {
                    d.surface.textures = specula_gpu::MaterialTextures::NONE;
                }
                d
            })
            .collect();
        &stripped
    } else {
        &scene.draws
    };
    for i in 0..WARMUP_FRAMES + args.frames {
        let frame = FrameDesc {
            view_proj,
            clear_color: scene.clear,
            draws,
            reverse_z: true,
            environment,
            lights: &scene.lights,
            time_s: 1.0,
            overlay,
            bones: &scene.bones,
        };
        let start = Instant::now();
        gpu.render(target, &frame)?;
        let wall = start.elapsed().as_secs_f64() * 1e3;
        if i < WARMUP_FRAMES {
            continue;
        }
        let s = gpu.last_frame_stats();
        if let Some(g) = &s.gpu {
            gpu_ms.push(g.total_ms);
            for p in &g.passes {
                match passes.iter_mut().find(|(n, _)| *n == p.name) {
                    Some((_, v)) => v.push(p.gpu_ms),
                    None => passes.push((p.name, vec![p.gpu_ms])),
                }
            }
        }
        record_ms.push(s.cpu.record_ms);
        wall_ms.push(wall);
        last = Some(s);
    }
    if let Some(dir) = &args.png {
        let rgba = gpu.read_rgba8(target)?;
        let path = dir.join(format!("{}.png", scene.name));
        if let Err(e) = std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&path, encode_png(&rgba, w, h))) {
            eprintln!("bench: writing {}: {e}", path.display());
        }
    }
    gpu.destroy_target(target);
    let s = last.expect("at least one measured frame");
    let glass = scene.draws.iter().filter(|d| d.material == Material::Glass).count();
    let (gpu_med, gpu_p95) = median_p95(&mut gpu_ms);
    let (rec_med, _) = median_p95(&mut record_ms);
    let (wall_med, wall_p95) = median_p95(&mut wall_ms);
    let d = &s.draws;
    let pass_json: Vec<String> = passes
        .iter_mut()
        .map(|(name, v)| {
            let (m, p) = median_p95(v);
            format!("{}:{{\"median\":{},\"p95\":{}}}", json_str(name), num(m), num(p))
        })
        .collect();
    Ok(format!(
        concat!(
            "{{\"scene\":{},\"adapter\":{},\"kind\":\"{:?}\",\"width\":{},\"height\":{},\"frames\":{},\"validation\":{},",
            "\"items\":{},\"lights\":{},\"culled\":{},\"draw_calls\":{},\"draw_calls_before_instancing\":{},",
            "\"draw_calls_unbatched_visible\":{},\"instances\":{},\"triangles\":{},\"pipeline_binds\":{},",
            "\"gpu_ms_median\":{},\"gpu_ms_p95\":{},\"cpu_record_ms_median\":{},\"wall_ms_median\":{},\"wall_ms_p95\":{},",
            "\"draw_buffer_bytes\":{},\"gpu_memory_bytes\":{},\"pipelines_created\":{},\"pipelines_created_after_warm\":{},",
            "\"pipeline_create_ms\":{},\"shadow_draw_calls\":{},\"shadow_instances\":{},\"overlay_quads\":{},",
            "\"graph_passes\":{},\"graph_barriers\":{},\"graph_compiles\":{},\"passes\":{{{}}}}}"
        ),
        json_str(scene.name),
        json_str(&gpu.adapter().name),
        gpu.adapter().kind,
        w,
        h,
        args.frames,
        gpu.validation_enabled(),
        scene.draws.len(),
        scene.lights.len(),
        d.culled,
        d.draw_calls,
        scene.draws.len() + glass, // the M0 renderer: one call per item, two per glass item
        d.unbatched_draw_calls,
        d.instances,
        d.triangles,
        d.pipeline_binds,
        num(gpu_med),
        num(gpu_p95),
        num(rec_med),
        num(wall_med),
        num(wall_p95),
        s.draw_buffer_bytes,
        s.memory.total_bytes,
        s.pipelines.created,
        s.pipelines.created_after_warm,
        num(s.pipelines.create_ms),
        d.shadow_draw_calls,
        d.shadow_instances,
        d.overlay_quads,
        s.graph.passes,
        s.graph.barriers,
        s.graph.compiles,
        pass_json.join(","),
    ))
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if !specula_gpu::is_available() {
        println!("{{\"skipped\":\"Vulkan is not available\"}}");
        return;
    }
    let validation = if args.validation { Validation::On } else { Validation::Off };
    let mut gpu = match Gpu::new(&GpuOptions { validation, adapter: args.adapter, ..Default::default() }) {
        Ok(g) => g,
        Err(e) => {
            println!("{{\"skipped\":{}}}", json_str(&e.to_string()));
            return;
        }
    };
    let result = (|| {
        let upload = |gpu: &mut Gpu, (v, i): (Vec<Vertex>, Vec<u32>)| gpu.upload_mesh(&v, &i);
        let with_uvs = |gpu: &mut Gpu, (v, i, uvs): (Vec<Vertex>, Vec<u32>, Vec<[f32; 2]>)| {
            gpu.upload_mesh_with(&v, &i, &MeshStreams { uvs: Some(&uvs), ..Default::default() })
        };
        let (cv, ci, cs) = creature();
        // The skinned and textured scenes' pipelines at loading time, as a game would.
        gpu.prepare_variants(true, true)?;
        let [base, normal, mr] = tile_textures();
        let sampler = TextureSampler { anisotropy: !args.without.iter().any(|w| w == "anisotropy"), ..Default::default() };
        let meshes = Meshes {
            cube: upload(&mut gpu, cube())?,
            sphere: upload(&mut gpu, sphere(32, 16))?,
            quad: upload(&mut gpu, quad())?,
            creature: gpu.upload_mesh_with(&cv, &ci, &MeshStreams { skin: Some(&cs), ..Default::default() })?,
            cube_uv: with_uvs(&mut gpu, cube_uv())?,
            sphere_uv: with_uvs(&mut gpu, sphere_uv(32, 16))?,
            textures: [
                gpu.upload_material_texture(256, 256, TextureFormat::Rgba8Srgb, &base, sampler)?,
                gpu.upload_material_texture(256, 256, TextureFormat::Rgba8Unorm, &normal, sampler)?,
                gpu.upload_material_texture(256, 256, TextureFormat::Rgba8Unorm, &mr, sampler)?,
            ],
        };
        gpu.mark_warm();
        let atlas = gpu.upload_texture(256, 256, TextureFormat::R8, &atlas())?;
        let scenes = [
            cubes_10k(&meshes),
            mixed_1000(&meshes),
            lights_16(&meshes),
            game(&meshes, atlas, false),
            game(&meshes, atlas, true),
            skinned_300(&meshes),
            textured_1000(&meshes),
        ];
        for scene in scenes.iter().filter(|s| args.scenes.as_ref().is_none_or(|only| only.iter().any(|n| n == s.name))) {
            println!("{}", run_scene(&mut gpu, &args, scene)?);
        }
        Ok::<(), specula_gpu::GpuError>(())
    })();
    let errors = gpu.close().into_iter().filter(|m| m.severity == Severity::Error).count();
    match result {
        Err(e) => {
            eprintln!("bench: {e}");
            std::process::exit(1);
        }
        Ok(()) if errors > 0 => {
            eprintln!("bench: {errors} validation error(s)");
            std::process::exit(1);
        }
        Ok(()) => {}
    }
}
