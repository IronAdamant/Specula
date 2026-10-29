//! Stylised water's look as data (docs/design/TERRAIN.md, "Stylised water"): a [`WaterStyle`]
//! on a [`crate::Material::Water`] draw (a quad: the surface), drawn in the frame's `water` pass
//! after the opaque scene and the decals, reading the scene's depth.

/// How the surface moves and what it looks like up close.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WaterPattern {
    /// Wave trains and ripples.
    #[default]
    Waves,
    /// Frozen: still, a faint crack network, frosted edges.
    Ice,
    /// Opaque, emissive: a slow crust over bright cracks.
    Lava,
}

impl WaterPattern {
    pub const ALL: [WaterPattern; 3] = [WaterPattern::Waves, WaterPattern::Ice, WaterPattern::Lava];

    pub fn name(self) -> &'static str {
        match self {
            WaterPattern::Waves => "waves",
            WaterPattern::Ice => "ice",
            WaterPattern::Lava => "lava",
        }
    }

    pub fn parse(s: &str) -> Option<WaterPattern> {
        WaterPattern::ALL.into_iter().find(|p| p.name() == s)
    }
}

/// A water surface's look. Colours are sRGB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterStyle {
    /// Colour where the water is deep.
    pub deep: [f32; 3],
    /// Colour where it is shallow (and the lava's glow).
    pub shallow: [f32; 3],
    /// Foam at the shore (the frost on ice, the cooled crust on lava).
    pub foam: [f32; 3],
    /// Most opacity (0..1) where the water is thick.
    pub opacity: f32,
    /// Metres of water to reach most of the deep colour and opacity.
    pub clarity: f32,
    /// Metres of depth over which shore foam shows (0 = none).
    pub foam_width: f32,
    /// Wavelength of the largest waves (metres).
    pub wave_scale: f32,
    /// Wave speed (m/s); 0 freezes them.
    pub wave_speed: f32,
    /// How much waves bend the normal (0..1).
    pub wave_strength: f32,
    pub roughness: f32,
    /// Strength of reflections (0..1, × Fresnel).
    pub reflect: f32,
    /// Light the body emits, as a multiple of `shallow` (lava).
    pub emissive: f32,
    pub pattern: WaterPattern,
}

impl Default for WaterStyle {
    fn default() -> Self {
        WaterStyle::preset("lake").expect("built in")
    }
}

/// The built-in presets' names.
pub const WATER_PRESETS: [&str; 5] = ["lake", "murk", "frozen", "lava", "swamp"];

fn hex(rgb: u32) -> [f32; 3] {
    [((rgb >> 16) & 255) as f32 / 255.0, ((rgb >> 8) & 255) as f32 / 255.0, (rgb & 255) as f32 / 255.0]
}

impl WaterStyle {
    /// A built-in preset (see [`WATER_PRESETS`] and the design doc's table).
    pub fn preset(name: &str) -> Option<WaterStyle> {
        let s = |deep, shallow, foam, opacity, clarity, foam_width, wave: (f32, f32, f32), roughness, reflect, emissive, pattern| WaterStyle {
            deep: hex(deep),
            shallow: hex(shallow),
            foam: hex(foam),
            opacity,
            clarity,
            foam_width,
            wave_scale: wave.0,
            wave_speed: wave.1,
            wave_strength: wave.2,
            roughness,
            reflect,
            emissive,
            pattern,
        };
        Some(match name {
            "lake" => s(0x13303f, 0x2f6a73, 0xd8e4e8, 0.92, 3.0, 0.35, (3.0, 0.6, 0.4), 0.08, 1.0, 0.0, WaterPattern::Waves),
            // Map 1: still, dark, neutral (< 25% saturation), the moon in it.
            "murk" => s(0x0b1119, 0x1d2633, 0x5d6878, 0.97, 1.2, 0.2, (6.0, 0.15, 0.25), 0.12, 0.65, 0.0, WaterPattern::Waves),
            // Map 2: a frozen lake, cool neutrals, frosted shore.
            "frozen" => s(0x3c5566, 0xa9c2cf, 0xeef4f8, 0.96, 6.0, 0.6, (8.0, 0.0, 0.3), 0.35, 0.6, 0.0, WaterPattern::Ice),
            // Map 3: lava, fire colours, glowing.
            "lava" => s(0x2a0d06, 0xff7a1f, 0x3a2a24, 1.0, 0.5, 0.8, (8.0, 0.1, 0.6), 0.75, 0.1, 2.5, WaterPattern::Lava),
            // Peat-brown, tea-coloured in the shallows, a weed-green scum at the shore; dull
            // (little reflection), unlike murk's cool grey mirror.
            "swamp" => s(0x1c1309, 0x4a3618, 0x6b7034, 0.99, 0.6, 0.45, (5.0, 0.1, 0.15), 0.4, 0.35, 0.0, WaterPattern::Waves),
            _ => return None,
        })
    }

    /// Every number finite and in range (what the shader assumes).
    pub fn clamped(mut self) -> WaterStyle {
        let c = |v: [f32; 3]| v.map(|x| if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.0 });
        let f = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.deep = c(self.deep);
        self.shallow = c(self.shallow);
        self.foam = c(self.foam);
        self.opacity = f(self.opacity, 0.0, 1.0, 1.0);
        self.clarity = f(self.clarity, 0.01, 1000.0, 1.0);
        self.foam_width = f(self.foam_width, 0.0, 100.0, 0.0);
        self.wave_scale = f(self.wave_scale, 0.05, 1000.0, 1.0);
        self.wave_speed = f(self.wave_speed, 0.0, 50.0, 0.0);
        self.wave_strength = f(self.wave_strength, 0.0, 1.0, 0.0);
        self.roughness = f(self.roughness, 0.02, 1.0, 0.1);
        self.reflect = f(self.reflect, 0.0, 1.0, 1.0);
        self.emissive = f(self.emissive, 0.0, 100.0, 0.0);
        self
    }
}

/// The draw record fields of a water draw (besides its model matrix): `color` = deep (linear) +
/// opacity, `color2` = shallow + clarity, `params` = emissive, foam width, roughness, reflect,
/// `ext` = wave scale bits, wave speed bits, foam (RGB8) | pattern << 24, wave strength (unorm16).
pub(crate) fn fields(s: &WaterStyle, linear: &mut impl FnMut([f32; 3]) -> [f32; 3]) -> ([f32; 4], [f32; 4], [f32; 4], [u32; 4]) {
    let s = s.clamped();
    let (d, sh) = (linear(s.deep), linear(s.shallow));
    let fm = linear(s.foam);
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    let pattern = match s.pattern {
        WaterPattern::Waves => 0,
        WaterPattern::Ice => 1,
        WaterPattern::Lava => 2,
    };
    (
        [d[0], d[1], d[2], s.opacity],
        [sh[0], sh[1], sh[2], s.clarity],
        [s.emissive, s.foam_width, s.roughness, s.reflect],
        [s.wave_scale.to_bits(), s.wave_speed.to_bits(), b(fm[0]) | b(fm[1]) << 8 | b(fm[2]) << 16 | pattern << 24, (s.wave_strength * 65535.0).round() as u32],
    )
}

/// Records the water pass into `color` (no depth attachment): the plan's water batches with the
/// water pipeline, reading the scene's depth at bindless slot `depth_slot`. `end` runs inside the
/// rendering instance before it ends (the overlay, when it joins this pass).
pub(crate) fn record_water(
    ctx: &crate::render::FrameContext,
    cmd: crate::vk::VkCommandBuffer,
    color: &crate::vk::VkRenderingAttachmentInfo,
    extent: crate::vk::VkExtent2D,
    depth_slot: u32,
    stats: &mut crate::DrawStats,
    end: &mut dyn FnMut(crate::vk::VkCommandBuffer),
) {
    use crate::vk::*;
    let dev = ctx.dev;
    let area = VkRect2D { offset: VkOffset2D { x: 0, y: 0 }, extent };
    let rendering = VkRenderingInfo {
        s_type: ST_RENDERING_INFO,
        p_next: std::ptr::null(),
        flags: 0,
        render_area: area,
        layer_count: 1,
        view_mask: 0,
        color_attachment_count: 1,
        p_color_attachments: color,
        p_depth_attachment: std::ptr::null(),
        p_stencil_attachment: std::ptr::null(),
    };
    let viewport = VkViewport { x: 0.0, y: 0.0, width: extent.width as f32, height: extent.height as f32, min_depth: 0.0, max_depth: 1.0 };
    let pipeline = ctx.pipelines.water;
    // SAFETY: `cmd` is recording outside any rendering instance; `color` is an attachment of an
    // image the graph transitioned to COLOR_ATTACHMENT_OPTIMAL for this pass and the depth image
    // to SHADER_READ_ONLY (sampled); the pipeline renders into that single colour format with the
    // shared layout; every mesh is live (`check_frame`) and the upload holds one record per plan
    // entry, so every instance index is in bounds.
    unsafe {
        (dev.fns.cmd_begin_rendering)(cmd, &rendering);
        (dev.fns.cmd_set_viewport)(cmd, 0, 1, &viewport);
        (dev.fns.cmd_set_scissor)(cmd, 0, 1, &area);
        if pipeline != VK_NULL_HANDLE {
            ctx.descriptors.bind(dev, cmd, ctx.pipelines.layout);
            (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, pipeline);
            stats.pipeline_binds += 1;
            let pc = crate::render::PushConstants {
                frame: ctx.upload.frame_address(),
                draws: ctx.upload.records_address(),
                // Bit 0: reversed Z.
                material: ctx.frame.reverse_z as u32,
                pass: depth_slot,
                bones: 0,
            };
            (dev.fns.cmd_push_constants)(
                cmd,
                ctx.pipelines.layout,
                crate::render::PUSH_STAGES,
                0,
                std::mem::size_of::<crate::render::PushConstants>() as u32,
                &pc as *const crate::render::PushConstants as *const std::ffi::c_void,
            );
            for batch in ctx.scratch.plan.batches.iter().filter(|b| b.material == crate::Material::Water) {
                let Some(mesh) = ctx.meshes.get(&batch.mesh.0) else { continue };
                mesh.bind(dev, cmd);
                (dev.fns.cmd_draw_indexed)(cmd, mesh.index_count, batch.count, 0, 0, batch.first);
                stats.draw_calls += 1;
                stats.water_draws += batch.count;
            }
        }
    }
    end(cmd);
    // SAFETY: ends the rendering instance begun above.
    unsafe { (dev.fns.cmd_end_rendering)(cmd) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_exist_and_are_in_range() {
        for name in WATER_PRESETS {
            let p = WaterStyle::preset(name).unwrap();
            assert_eq!(p, p.clamped(), "{name}");
        }
        assert!(WaterStyle::preset("ocean").is_none());
        assert_eq!(WaterStyle::preset("lava").unwrap().pattern, WaterPattern::Lava);
        assert_eq!(WaterPattern::parse("ice"), Some(WaterPattern::Ice));
    }

    #[test]
    fn clamping_removes_nan_and_extremes() {
        let s = WaterStyle { clarity: f32::NAN, opacity: 7.0, deep: [2.0, -1.0, f32::INFINITY], ..WaterStyle::default() }.clamped();
        assert_eq!(s.clarity, 1.0);
        assert_eq!(s.opacity, 1.0);
        assert_eq!(s.deep, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn fields_pack_the_pattern_and_foam() {
        let s = WaterStyle::preset("frozen").unwrap();
        let (_, c2, p, e) = fields(&s, &mut |c| c);
        assert_eq!(c2[3], s.clarity);
        assert_eq!(p[1], s.foam_width);
        assert_eq!(e[2] >> 24, 1);
        assert_eq!(f32::from_bits(e[1]), 0.0);
        assert_eq!(e[2] & 255, (s.foam[0] * 255.0).round() as u32);
    }
}
