//! Particles and decals ([`crate::Effects`]): checked, packed into the frame's upload buffer and
//! recorded in their own passes (docs/design/VFX.md).
//!
//! # Particles
//!
//! One instanced draw of camera-facing quads for every particle of the frame, in the order given
//! (the caller sorts far to near): the `particles` pass after the scene (and its blended draws),
//! colour only. The fragment shader reads the scene's depth as a texture, discards what is behind
//! geometry and fades particles over their material's `soft` metres where they meet it.
//! Premultiplied blending with alpha × (1 − additive): additive and alpha particles share the
//! pipeline and the draw. No early depth test (no depth attachment): fill is the caller's budget.
//!
//! # Decals
//!
//! Between the opaque part of the scene and the rest (the scene pass splits into `scene`,
//! `decals`, `scene_blend` when a frame has decals): each decal's box is drawn by its back faces,
//! and the fragment shader projects the texture onto the opaque surface reconstructed from the
//! depth. Lit decals blend premultiplied; multiply decals use `dst × src`. Consecutive decals of
//! one mode merge into one instanced draw.
//!
//! Neither writes the ID buffer (sight's per-entity facts stay about geometry).

use crate::device::Device;
use crate::overlay::Textures;
use crate::render::{LinearCache, PUSH_STAGES};
use crate::shadow::invert;
use crate::sprite::View2d;
use crate::vk::*;
use crate::{DrawStats, ErrorReason, GpuError, IdKind, Limit, TextureId};
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;

/// At most this many particles per frame.
pub const MAX_PARTICLES: usize = 262_144;
/// At most this many particle materials per frame.
pub const MAX_PARTICLE_MATERIALS: usize = 4096;
/// At most this many decals per frame.
pub const MAX_DECALS: usize = 16_384;

/// One particle (see the module docs): a camera-facing quad.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Particle {
    pub position: [f32; 3],
    /// Side of the quad in metres.
    pub size: f32,
    /// **Linear** RGB (HDR: above 1 glows and blooms) and straight alpha. Exposure applies.
    pub color: [f32; 4],
    /// Radians, about the view axis (ignored when stretched).
    pub rotation: f32,
    /// A world vector the quad is stretched along (its on-screen part lengthens the quad):
    /// sparks along their velocity. Zero: square.
    pub stretch: [f32; 3],
    /// 0 = alpha blended (covers what is behind), 1 = additive (adds light); between: a mix.
    pub additive: f32,
    /// Index into [`Effects::materials`].
    pub material: u16,
    /// Flipbook frame (cells in reading order, wrapped).
    pub frame: u16,
}

/// What particles look like: shared by many.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleMaterial {
    /// sRGB colour texture with straight alpha (`None`: a flat quad). Mip-mapped textures
    /// ([`crate::Gpu::upload_material_texture`]) shimmer least.
    pub texture: Option<TextureId>,
    /// Flipbook columns and rows (1 × 1: the whole texture).
    pub grid: [u32; 2],
    /// Metres over which a particle fades where it meets geometry (0 = hard).
    pub soft: f32,
    /// Lit by ambient, sun and point lights at its centre (smoke); unlit glows its colour.
    pub lit: bool,
}

impl Default for ParticleMaterial {
    fn default() -> Self {
        ParticleMaterial { texture: None, grid: [1, 1], soft: 0.0, lit: false }
    }
}

/// How a decal is applied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DecalMode {
    /// Re-lit albedo (normal map, roughness, specular, emissive) blended over the surface.
    #[default]
    Lit,
    /// The surface's colour times the texture's: darkening (soot, scorch).
    Multiply,
}

/// A projected decal (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decal {
    /// The unit box (−0.5..0.5 on each axis) to the world, column-major. The texture spans the
    /// box's X (u) and Z (v, down the image) and projects along its Y.
    pub model: [f32; 16],
    /// sRGB tint and opacity.
    pub color: [f32; 4],
    pub albedo: Option<TextureId>,
    /// Texture rect u0, v0, u1, v1.
    pub uv: [f32; 4],
    /// Tangent-space normal map (`Rgba8Unorm`, +Y up; lit decals).
    pub normal: Option<TextureId>,
    pub normal_strength: f32,
    pub roughness: f32,
    pub specular: f32,
    /// Light emitted, × the tinted albedo (lit decals).
    pub emissive: f32,
    pub mode: DecalMode,
    /// Surfaces whose normal is less than this cosine from the box's +Y get nothing; the decal
    /// fades in over the next 0.2.
    pub angle_fade: f32,
}

impl Default for Decal {
    fn default() -> Self {
        Decal {
            model: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            color: [1.0; 4],
            albedo: None,
            uv: [0.0, 0.0, 1.0, 1.0],
            normal: None,
            normal_strength: 1.0,
            roughness: 0.8,
            specular: 0.0,
            emissive: 0.0,
            mode: DecalMode::Lit,
            angle_fade: 0.3,
        }
    }
}

/// A frame's effects ([`crate::FrameDesc::effects`]). Empty by default: the frame renders exactly
/// what it rendered before effects existed.
#[derive(Clone, Copy, Debug, Default)]
pub struct Effects<'a> {
    /// Drawn in this order (sort them far to near).
    pub particles: &'a [Particle],
    pub materials: &'a [ParticleMaterial],
    /// Drawn in this order (sort them by their own order).
    pub decals: &'a [Decal],
}

impl Effects<'_> {
    pub fn is_empty(&self) -> bool {
        self.particles.is_empty() && self.decals.is_empty()
    }
}

/// One particle as the shaders read it; must match `ParticleRec` in `shaders/particle.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParticleRecord {
    pub a: [f32; 4],
    pub b: [u32; 4],
    pub c: [f32; 4],
}

/// One material as the shaders read it; must match `MaterialRec`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MaterialRecord {
    pub info: [u32; 4],
    pub params: [f32; 4],
}

/// One decal as the shaders read it; must match `DecalRec` in `shaders/decal.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DecalRecord {
    pub model: [f32; 16],
    pub inv: [f32; 16],
    pub up: [f32; 4],
    pub color: [f32; 4],
    pub uv: [f32; 4],
    pub params: [f32; 4],
    pub info: [u32; 4],
}

/// Push constants of the particle pass; must match `Push` in `shaders/particle.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParticlePush {
    pub frame: u64,
    pub particles: u64,
    pub materials: u64,
    pub depth: u32,
    pub mode: u32,
    pub cam_pos: [f32; 4],
    pub cam_right: [f32; 4],
    pub cam_up: [f32; 4],
    pub cam_forward: [f32; 4],
}

/// Push constants of the decal pass; must match `Push` in `shaders/decal.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DecalPush {
    pub frame: u64,
    pub decals: u64,
    pub depth: u32,
    pub mode: u32,
    pub pad: [u32; 2],
    pub cam_pos: [f32; 4],
}

const _: () = assert!(size_of::<ParticleRecord>() == 48);
const _: () = assert!(size_of::<MaterialRecord>() == 32);
const _: () = assert!(size_of::<DecalRecord>() == 208);
const _: () = assert!(size_of::<ParticlePush>() <= 128 && size_of::<DecalPush>() <= 128);

/// Material flag: lit (`MAT_LIT` in the shader).
const MAT_LIT: u32 = 1;

/// An `f32` as IEEE half-float bits (round to nearest even; NaN stays NaN, overflow → infinity).
pub fn f16_bits(v: f32) -> u16 {
    let x = v.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let exp = ((x >> 23) & 0xff) as i32;
    let mant = x & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | if mant != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        // Subnormal: shift the mantissa (with its implicit 1) into place, rounding to even.
        let m = mant | 0x80_0000;
        let shift = (14 - e) as u32;
        let half = 1u32 << (shift - 1);
        let rest = m & ((1 << shift) - 1);
        let mut r = m >> shift;
        if rest > half || (rest == half && r & 1 == 1) {
            r += 1;
        }
        return sign | r as u16;
    }
    let mut r = ((e as u32) << 10) | (mant >> 13);
    let rest = mant & 0x1fff;
    if rest > 0x1000 || (rest == 0x1000 && r & 1 == 1) {
        r += 1; // may carry into the exponent: still correct (up to infinity)
    }
    sign | r as u16
}

fn pack_half2(a: f32, b: f32) -> u32 {
    f16_bits(a) as u32 | (f16_bits(b) as u32) << 16
}

fn finite(v: f32, or: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        or
    }
}

/// Fail early: too many particles, materials or decals; a particle naming a material that isn't
/// there; a texture that doesn't exist.
pub fn check_effects(fx: &crate::Effects, textures: &Textures) -> Result<(), GpuError> {
    if fx.particles.len() > MAX_PARTICLES {
        return Err(GpuError::limit(Limit::Particles, fx.particles.len() as u64, MAX_PARTICLES as u64));
    }
    if fx.materials.len() > MAX_PARTICLE_MATERIALS {
        return Err(GpuError::limit(Limit::ParticleMaterials, fx.materials.len() as u64, MAX_PARTICLE_MATERIALS as u64));
    }
    if fx.decals.len() > MAX_DECALS {
        return Err(GpuError::limit(Limit::Decals, fx.decals.len() as u64, MAX_DECALS as u64));
    }
    let n = fx.materials.len() as u32;
    if let Some((i, p)) = fx.particles.iter().enumerate().find(|(_, p)| p.material as u32 >= n) {
        return Err(GpuError::rejected(
            ErrorReason::MaterialOutside { particle: i as u32, material: p.material as u32, materials: n },
            format!("particle {i} names material {} but the frame has {n}", p.material),
        ));
    }
    let texture = |t: Option<TextureId>, what: &str| match t {
        Some(id) if textures.slot(id).is_none() => Err(GpuError::unknown(IdKind::Texture, id.0, what)),
        _ => Ok(()),
    };
    for m in fx.materials {
        texture(m.texture, " in effects.materials")?;
    }
    for d in fx.decals {
        texture(d.albedo, " in effects.decals (albedo)")?;
        texture(d.normal, " in effects.decals (normal)")?;
    }
    Ok(())
}

/// A frame's effects as records, and the decal draws (consecutive decals of one mode).
#[derive(Default)]
pub struct EffectsPlan {
    pub particles: Vec<ParticleRecord>,
    pub materials: Vec<MaterialRecord>,
    pub decals: Vec<DecalRecord>,
    /// `(multiply, first, count)` per decal draw.
    pub decal_batches: Vec<(bool, u32, u32)>,
}

impl EffectsPlan {
    /// Record units (of `unit` bytes) the records take in the upload buffer, each kind starting on
    /// a unit: `(particles, materials, decals)`.
    pub fn units(&self, unit: usize) -> (usize, usize, usize) {
        let u = |bytes: usize| bytes.div_ceil(unit);
        (u(self.particles.len() * size_of::<ParticleRecord>()), u(self.materials.len() * size_of::<MaterialRecord>()), u(self.decals.len() * size_of::<DecalRecord>()))
    }
}

/// Pack the frame's effects into `out` (reused: no allocation once warm).
pub fn plan_effects(fx: &crate::Effects, textures: &Textures, cache: &mut LinearCache, out: &mut EffectsPlan) {
    out.particles.clear();
    out.materials.clear();
    out.decals.clear();
    out.decal_batches.clear();
    let slot = |t: Option<TextureId>| t.and_then(|id| textures.slot(id)).map_or(0, |s| s + 1);
    for m in fx.materials {
        out.materials.push(MaterialRecord {
            info: [slot(m.texture), m.grid[0].clamp(1, 64), m.grid[1].clamp(1, 64), if m.lit { MAT_LIT } else { 0 }],
            params: [finite(m.soft, 0.0).clamp(0.0, 100.0), 0.0, 0.0, 0.0],
        });
    }
    for p in fx.particles {
        let c = p.color;
        let pos = p.position.map(|v| finite(v, 0.0));
        out.particles.push(ParticleRecord {
            a: [pos[0], pos[1], pos[2], finite(p.size, 0.0).max(0.0)],
            b: [
                pack_half2(finite(c[0], 0.0).clamp(0.0, 60000.0), finite(c[1], 0.0).clamp(0.0, 60000.0)),
                pack_half2(finite(c[2], 0.0).clamp(0.0, 60000.0), finite(c[3], 0.0).clamp(0.0, 1.0)),
                p.material as u32 | (p.frame as u32) << 16,
                finite(p.rotation, 0.0).to_bits(),
            ],
            c: [finite(p.stretch[0], 0.0), finite(p.stretch[1], 0.0), finite(p.stretch[2], 0.0), finite(p.additive, 1.0).clamp(0.0, 1.0)],
        });
    }
    const IDENTITY: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    for d in fx.decals {
        // A degenerate box (a zero axis) can't be inverted: it draws nothing.
        let Some(inv) = invert(&d.model) else { continue };
        let m = &d.model;
        let up = {
            let y = [m[4], m[5], m[6]];
            let l = (y[0] * y[0] + y[1] * y[1] + y[2] * y[2]).sqrt();
            if l > 1e-12 && l.is_finite() {
                [y[0] / l, y[1] / l, y[2] / l]
            } else {
                [0.0, 1.0, 0.0]
            }
        };
        let tint = cache.get([d.color[0], d.color[1], d.color[2]]);
        let multiply = d.mode == DecalMode::Multiply;
        let n = out.decals.len() as u32;
        out.decals.push(DecalRecord {
            model: if m.iter().all(|v| v.is_finite()) { *m } else { IDENTITY },
            inv,
            up: [up[0], up[1], up[2], finite(d.angle_fade, 0.3).clamp(-1.0, 1.0)],
            color: [tint[0], tint[1], tint[2], finite(d.color[3], 1.0).clamp(0.0, 1.0)],
            uv: d.uv.map(|v| finite(v, 0.0)),
            params: [
                finite(d.roughness, 0.8).clamp(0.0, 1.0),
                finite(d.specular, 0.0).clamp(0.0, 1.0),
                finite(d.emissive, 0.0).clamp(0.0, 1000.0),
                finite(d.normal_strength, 1.0).clamp(0.0, 8.0),
            ],
            info: [slot(d.albedo), if multiply { 0 } else { slot(d.normal) }, 0, 0],
        });
        match out.decal_batches.last_mut() {
            Some(b) if b.0 == multiply => b.2 += 1,
            _ => out.decal_batches.push((multiply, n, 1)),
        }
    }
}

/// What recording the effect passes needs.
pub struct EffectsDraw<'a> {
    pub layout: VkPipelineLayout,
    pub particle: VkPipeline,
    pub decal_lit: VkPipeline,
    pub decal_multiply: VkPipeline,
    pub plan: &'a EffectsPlan,
    pub frame: u64,
    /// Device addresses of the particle, material and decal records.
    pub particles: u64,
    pub materials: u64,
    pub decals: u64,
    /// Bindless slot of the scene's depth.
    pub depth: u32,
    pub reverse_z: bool,
    pub view: View2d,
}

fn begin(dev: &Device, cmd: VkCommandBuffer, color: &VkRenderingAttachmentInfo, extent: VkExtent2D) {
    let area = VkRect2D { offset: VkOffset2D { x: 0, y: 0 }, extent };
    let rendering = VkRenderingInfo {
        s_type: ST_RENDERING_INFO,
        p_next: ptr::null(),
        flags: 0,
        render_area: area,
        layer_count: 1,
        view_mask: 0,
        color_attachment_count: 1,
        p_color_attachments: color,
        p_depth_attachment: ptr::null(),
        p_stencil_attachment: ptr::null(),
    };
    let viewport = VkViewport { x: 0.0, y: 0.0, width: extent.width as f32, height: extent.height as f32, min_depth: 0.0, max_depth: 1.0 };
    // SAFETY: `cmd` is recording outside any rendering instance; `color` is an attachment of an
    // image the graph transitioned to COLOR_ATTACHMENT_OPTIMAL for this pass; the structs live
    // through the calls.
    unsafe {
        (dev.fns.cmd_begin_rendering)(cmd, &rendering);
        (dev.fns.cmd_set_viewport)(cmd, 0, 1, &viewport);
        (dev.fns.cmd_set_scissor)(cmd, 0, 1, &area);
    }
}

fn push<T>(dev: &Device, cmd: VkCommandBuffer, layout: VkPipelineLayout, value: &T) {
    // SAFETY: `cmd` is recording; the layout's push range covers both stages and 128 bytes, and
    // every push struct here is at most that (asserted above).
    unsafe { (dev.fns.cmd_push_constants)(cmd, layout, PUSH_STAGES, 0, size_of::<T>() as u32, value as *const T as *const c_void) };
}

/// Records the particle pass into `color` (`end` runs inside the rendering instance before it
/// ends: the overlay, when it joins this pass).
pub fn record_particles(dev: &Device, cmd: VkCommandBuffer, d: &EffectsDraw, color: &VkRenderingAttachmentInfo, extent: VkExtent2D, stats: &mut DrawStats, end: &mut dyn FnMut(VkCommandBuffer)) {
    begin(dev, cmd, color, extent);
    let count = d.plan.particles.len() as u32;
    if count > 0 && d.particle != VK_NULL_HANDLE {
        let v = &d.view;
        let w = |a: [f32; 3]| [a[0], a[1], a[2], 0.0];
        let pc = ParticlePush {
            frame: d.frame,
            particles: d.particles,
            materials: d.materials,
            depth: d.depth,
            mode: d.reverse_z as u32 | (v.ortho as u32) << 1,
            cam_pos: w(v.position),
            cam_right: w(v.right),
            cam_up: w(v.up),
            cam_forward: w(v.forward),
        };
        // SAFETY: recording inside the rendering instance begun above; the pipeline matches its
        // single colour attachment and the layout; the records hold `count` particles and every
        // material index is in range (`check_effects`).
        unsafe { (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, d.particle) };
        push(dev, cmd, d.layout, &pc);
        // SAFETY: as above.
        unsafe { (dev.fns.cmd_draw)(cmd, 6, count, 0, 0) };
        stats.particles = count;
        stats.particle_draw_calls = 1;
        stats.pipeline_binds += 1;
    }
    end(cmd);
    // SAFETY: ends the rendering instance begun above.
    unsafe { (dev.fns.cmd_end_rendering)(cmd) };
}

/// Records the decal pass into `color`.
pub fn record_decals(dev: &Device, cmd: VkCommandBuffer, d: &EffectsDraw, color: &VkRenderingAttachmentInfo, extent: VkExtent2D, stats: &mut DrawStats, end: &mut dyn FnMut(VkCommandBuffer)) {
    begin(dev, cmd, color, extent);
    let pc = DecalPush { frame: d.frame, decals: d.decals, depth: d.depth, mode: d.reverse_z as u32, pad: [0; 2], cam_pos: [d.view.position[0], d.view.position[1], d.view.position[2], 0.0] };
    let mut bound = VK_NULL_HANDLE;
    for &(multiply, first, count) in &d.plan.decal_batches {
        let pipeline = if multiply { d.decal_multiply } else { d.decal_lit };
        if pipeline == VK_NULL_HANDLE {
            continue;
        }
        if pipeline != bound {
            // SAFETY: recording inside the rendering instance begun above; the pipeline matches it.
            unsafe { (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, pipeline) };
            push(dev, cmd, d.layout, &pc);
            bound = pipeline;
            stats.pipeline_binds += 1;
        }
        // SAFETY: the decal records `first .. first + count` are in the upload buffer.
        unsafe { (dev.fns.cmd_draw)(cmd, 36, count, 0, first) };
        stats.decal_draw_calls += 1;
        stats.decals += count;
    }
    end(cmd);
    // SAFETY: ends the rendering instance begun above.
    unsafe { (dev.fns.cmd_end_rendering)(cmd) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    #[test]
    fn records_match_shader_layout() {
        assert_eq!(offset_of!(ParticleRecord, b), 16);
        assert_eq!(offset_of!(ParticleRecord, c), 32);
        assert_eq!(offset_of!(DecalRecord, inv), 64);
        assert_eq!(offset_of!(DecalRecord, up), 128);
        assert_eq!(offset_of!(DecalRecord, info), 192);
        assert_eq!(offset_of!(ParticlePush, depth), 24);
        assert_eq!(offset_of!(ParticlePush, cam_pos), 32);
        assert_eq!(size_of::<ParticlePush>(), 96);
        assert_eq!(offset_of!(DecalPush, cam_pos), 32);
    }

    #[test]
    fn half_floats_round_like_the_hardware() {
        assert_eq!(f16_bits(0.0), 0);
        assert_eq!(f16_bits(1.0), 0x3c00);
        assert_eq!(f16_bits(-2.0), 0xc000);
        assert_eq!(f16_bits(65504.0), 0x7bff);
        assert_eq!(f16_bits(1e9), 0x7c00);
        assert_eq!(f16_bits(f32::NAN) & 0x7c00, 0x7c00);
        assert_eq!(f16_bits(0.5 + 1.0 / 4096.0), 0x3800, "ties to even");
        assert_eq!(f16_bits(6.0e-8), 0x0001, "smallest subnormal");
        assert_eq!(f16_bits(1e-9), 0);
    }

    #[test]
    fn plans_pack_and_batch_by_mode() {
        let fx = crate::Effects {
            particles: &[Particle { position: [1.0, 2.0, 3.0], size: 0.5, color: [2.0, 1.0, 0.5, 0.25], material: 0, frame: 3, ..Default::default() }],
            materials: &[ParticleMaterial::default()],
            decals: &[
                Decal::default(),
                Decal { mode: DecalMode::Multiply, ..Decal::default() },
                Decal { mode: DecalMode::Multiply, ..Decal::default() },
                Decal { model: [0.0; 16], ..Decal::default() },
            ],
        };
        let mut plan = EffectsPlan::default();
        plan_effects(&fx, &Textures::default(), &mut LinearCache::default(), &mut plan);
        assert_eq!(plan.particles[0].a, [1.0, 2.0, 3.0, 0.5]);
        assert_eq!(plan.particles[0].b[2], 3 << 16);
        assert_eq!(plan.particles[0].b[0], 0x3c00_4000, "r = 2, g = 1");
        assert_eq!(plan.decals.len(), 3, "the degenerate box is left out");
        assert_eq!(plan.decal_batches, [(false, 0, 1), (true, 1, 2)]);
        assert_eq!(plan.units(128), (1, 1, 5));
    }

    #[test]
    fn bad_effects_are_structured_errors() {
        let fx = crate::Effects { particles: &[Particle { material: 2, ..Default::default() }], materials: &[ParticleMaterial::default()], decals: &[] };
        let e = check_effects(&fx, &Textures::default()).unwrap_err();
        assert_eq!(e.reason, Some(ErrorReason::MaterialOutside { particle: 0, material: 2, materials: 1 }));
        let m = [ParticleMaterial { texture: Some(TextureId(9)), ..Default::default() }];
        let fx = crate::Effects { particles: &[], materials: &m, decals: &[] };
        assert_eq!(check_effects(&fx, &Textures::default()).unwrap_err().reason, Some(ErrorReason::UnknownId { kind: IdKind::Texture, id: 9 }));
    }
}
