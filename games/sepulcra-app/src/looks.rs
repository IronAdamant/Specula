//! What every thing in Sepulcra looks like: the STYLE.md palette and one function per kind of
//! thing that returns its model as a list of [`Part`]s.
//!
//! This is the seam for real assets: the procedural-modelling lane will build meshes from
//! `games/sepulcra/assets/recipes/`; swapping a greybox for a model means changing the function
//! for that kind (say [`enemy_model`]) to return one part with the model's mesh. Nothing else in
//! the presentation knows what a thing is made of.
//!
//! Models are in metres, with the origin where the thing stands (feet, base), +Y up and +Z
//! forward (the way an enemy walks). Gameplay pieces are a little oversized, per the style bible.

use specula::world::{LightSpec, Look, Profile, Shape};
use specula_core::math::Vec3;
use specula_gpu::{Material, Surface};

/// sRGB from `0xRRGGBB`.
pub const fn hex(h: u32) -> [f32; 3] {
    [((h >> 16) & 255) as f32 / 255.0, ((h >> 8) & 255) as f32 / 255.0, (h & 255) as f32 / 255.0]
}

/// The style bible's colours (STYLE.md). Three families never mix: player cyan, enemy green,
/// fire warm; everything else is a cool neutral.
pub mod palette {
    use super::hex;
    pub const RUNE_GLOW: [f32; 3] = hex(0x3fe0d0);
    pub const RUNE_DEEP: [f32; 3] = hex(0x1a8f9e);
    pub const RUNE_PALE: [f32; 3] = hex(0xb8fff6);
    pub const GRAVE_GLOW: [f32; 3] = hex(0x9cff4f);
    pub const GRAVE_DEEP: [f32; 3] = hex(0x3e7a1f);
    pub const FIRE_CORE: [f32; 3] = hex(0xffd27a);
    pub const FIRE_FLAME: [f32; 3] = hex(0xff7a1f);
    pub const SLAB_TOP: [f32; 3] = hex(0x5b6470);
    pub const CLIFF_A: [f32; 3] = hex(0x3d434d);
    pub const CLIFF_B: [f32; 3] = hex(0x4a515c);
    pub const FISSURE: [f32; 3] = hex(0x262a31);
    pub const CARVED: [f32; 3] = hex(0x8c96a3);
    pub const CHAMFER: [f32; 3] = hex(0xaab4c0);
    pub const MOSS: [f32; 3] = hex(0x4f6b3a);
    pub const BONE: [f32; 3] = hex(0xd8cfb8);
    pub const BONE_SHADOW: [f32; 3] = hex(0x9c937e);
    pub const GRAVE_STONE: [f32; 3] = hex(0xa7a39a);
    pub const LICHEN: [f32; 3] = hex(0x7d8a6a);
    pub const SKY: [f32; 3] = hex(0x0b1020);
    pub const MIST: [f32; 3] = hex(0x2a3448);
    pub const MOON: [f32; 3] = hex(0xc9d6ff);
    pub const IRON: [f32; 3] = hex(0x5c6168);
    /// The chalk scar on lane-jump ledges (a pale neutral).
    pub const CHALK: [f32; 3] = hex(0xd6d2c4);
    /// Invalid placement. The style bible has no UI red yet; a muted brick keeps clear of both
    /// the player's cyan and the enemy's green.
    pub const INVALID: [f32; 3] = hex(0xb03a2e);
}

use palette::*;

/// One primitive of a model.
#[derive(Clone, Debug)]
pub struct Part {
    pub name: &'static str,
    pub shape: Shape,
    pub pos: Vec3,
    pub rot: Vec3,
    pub scale: Vec3,
    pub color: [f32; 3],
    pub look: Look,
    pub light: Option<LightSpec>,
    pub profile: Option<Profile>,
}

impl Part {
    pub fn new(name: &'static str, shape: Shape, pos: Vec3, scale: Vec3, color: [f32; 3]) -> Part {
        let look = Look { material: Material::Lit, opacity: 1.0, surface: Surface { roughness: 0.8, ..Surface::default() } };
        Part { name, shape, pos, rot: Vec3::ZERO, scale, color, look, light: None, profile: None }
    }

    pub fn cube(name: &'static str, pos: Vec3, scale: Vec3, color: [f32; 3]) -> Part {
        Part::new(name, Shape::Cube, pos, scale, color)
    }

    pub fn sphere(name: &'static str, pos: Vec3, scale: Vec3, color: [f32; 3]) -> Part {
        Part::new(name, Shape::Sphere, pos, scale, color)
    }

    pub fn rot(mut self, x: f32, y: f32, z: f32) -> Part {
        self.rot = Vec3::new(x, y, z);
        self
    }

    pub fn glow(mut self, emissive: f32) -> Part {
        self.look.surface.emissive = emissive;
        self
    }

    pub fn rough(mut self, roughness: f32) -> Part {
        self.look.surface.roughness = roughness;
        self
    }

    pub fn metal(mut self, metallic: f32) -> Part {
        self.look.surface.metallic = metallic;
        self.look.surface.specular = 0.5;
        self
    }

    pub fn glass(mut self, opacity: f32) -> Part {
        self.look.material = Material::Glass;
        self.look.opacity = opacity;
        self
    }

    pub fn light(mut self, intensity: f32, range: f32, color: [f32; 3]) -> Part {
        self.light = Some(LightSpec { intensity, range, color: Some(color), offset: Vec3::ZERO });
        self
    }
}

fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

fn s(k: f32) -> Vec3 {
    Vec3::new(k, k, k)
}

/// A stone the player carved: `kind` is its id in `stones.json`, `tier` 0..=2. Tiers read from
/// the default camera: each tier adds a glowing rune band and height, tier II a capstone,
/// tier III a floating ring of rune shards.
pub fn stone_model(kind: &str, tier: u8) -> Vec<Part> {
    let t = f32::from(tier.min(2));
    let hf = 1.0 + 0.12 * t;
    let mut m = vec![Part::cube("plinth", v(0.0, 0.06, 0.0), v(1.4, 0.12, 1.4), CHAMFER).rough(0.7)];
    // (body height, body width, where rune bands start, top of the stone)
    let (body_h, body_w) = match kind {
        "warding" => (2.0 * hf, 1.0),
        "rune_pillar" => (1.7 * hf, 1.1),
        "brazier" => (0.0, 1.3),
        "sky_obelisk" => (3.6 * hf, 0.8),
        _ => (2.0 * hf, 1.0),
    };
    let mut top: f32;
    match kind {
        "brazier" => {
            // A cairn: stacked stones, a little twisted, an iron bowl of fire on top.
            let mut y = 0.12;
            let blocks = [(1.3, 0.6, 8.0), (1.05, 0.5, -14.0), (0.85, 0.42, 21.0)];
            for (i, (w, h, r)) in blocks.iter().enumerate().take(2 + usize::from(tier >= 1)) {
                let name = ["cairn_1", "cairn_2", "cairn_3"][i];
                m.push(Part::cube(name, v(0.0, y + h / 2.0, 0.0), v(*w, *h, *w), CARVED).rot(0.0, *r, 0.0).rough(0.75));
                y += h;
            }
            top = y;
            m.push(Part::cube("rune_1", v(0.0, 0.42, 0.0), v(1.33, 0.08, 1.33), RUNE_GLOW).rot(0.0, 8.0, 0.0).glow(2.5));
            if tier >= 2 {
                m.push(Part::cube("rune_2", v(0.0, 0.88, 0.0), v(1.08, 0.08, 1.08), RUNE_GLOW).rot(0.0, -14.0, 0.0).glow(2.5));
            }
            m.push(Part::sphere("bowl", v(0.0, top + 0.12, 0.0), v(1.3, 0.5, 1.3), IRON).metal(0.9).rough(0.45));
            m.push(Part::sphere("flame", v(0.0, top + 0.38, 0.0), v(0.8, 0.62, 0.8), FIRE_FLAME).glow(3.0).light(9.0, 9.0, FIRE_FLAME));
            m.push(Part::sphere("flame_core", v(0.0, top + 0.5, 0.0), v(0.42, 0.5, 0.42), FIRE_CORE).glow(4.0));
            top += 0.8;
        }
        _ => {
            m.push(Part::cube("body", v(0.0, 0.12 + body_h / 2.0, 0.0), v(body_w, body_h, body_w), CARVED).rough(0.75));
            for k in 0..=tier.min(2) {
                let y = 0.6 + 0.5 * f32::from(k);
                let name = ["rune_1", "rune_2", "rune_3"][usize::from(k)];
                m.push(Part::cube(name, v(0.0, y, 0.0), v(body_w + 0.04, 0.09, body_w + 0.04), RUNE_GLOW).glow(2.5));
            }
            let crown = 0.12 + body_h;
            if tier >= 1 {
                m.push(Part::cube("capstone", v(0.0, crown + 0.07, 0.0), v(body_w + 0.18, 0.14, body_w + 0.18), CHAMFER).rough(0.7));
            }
            let crown = crown + if tier >= 1 { 0.14 } else { 0.0 };
            match kind {
                "rune_pillar" => {
                    m.push(Part::cube("cap", v(0.0, crown + 0.08, 0.0), v(1.3, 0.16, 1.3), CHAMFER).rough(0.7));
                    m.push(Part::sphere("halo", v(0.0, crown + 0.3, 0.0), v(1.9, 0.07, 1.9), RUNE_GLOW).glow(1.6));
                    top = crown + 0.35;
                }
                "sky_obelisk" => {
                    // A needle: a slim diamond tip that glows.
                    m.push(Part::cube("tip", v(0.0, crown + 0.1, 0.0), v(0.5, 0.5, 0.8), RUNE_GLOW).rot(0.0, 0.0, 45.0).glow(2.0));
                    top = crown + 0.45;
                }
                _ => {
                    // Warding stone: a pointed gable.
                    m.push(Part::cube("point", v(0.0, crown, 0.0), v(0.7, 0.7, body_w), CARVED).rot(0.0, 0.0, 45.0).rough(0.75));
                    top = crown + 0.5;
                }
            }
        }
    }
    if tier >= 2 {
        for (i, a) in [0.0f32, 90.0, 180.0, 270.0].iter().enumerate() {
            let (x, z) = (a.to_radians().sin() * 0.95, a.to_radians().cos() * 0.95);
            let name = ["shard_1", "shard_2", "shard_3", "shard_4"][i];
            m.push(Part::cube(name, v(x, top + 0.45, z), s(0.2), RUNE_PALE).rot(45.0, *a, 45.0).glow(2.5));
        }
    }
    m
}

/// How tall a stone's model is (for the range ring and labels).
pub fn stone_height(kind: &str, tier: u8) -> f32 {
    let hf = 1.0 + 0.12 * f32::from(tier.min(2));
    match kind {
        "rune_pillar" => 1.7 * hf + 0.5,
        "sky_obelisk" => 3.6 * hf + 0.6,
        "brazier" => 2.4,
        _ => 2.0 * hf + 0.6,
    }
}

/// An undead: `kind` is its id in `enemies.json`. Distinct silhouettes (STYLE.md shape
/// language): the shambler wide and low, the runner thin and leaning, the wraith a tattered
/// floating cone, the colossus a giant ribcage and spine.
pub fn enemy_model(kind: &str) -> Vec<Part> {
    let eyes = |name, pos, scale| Part::cube(name, pos, scale, GRAVE_GLOW).glow(2.2);
    match kind {
        "shambler" => vec![
            Part::cube("legs", v(0.0, 0.32, 0.0), v(0.72, 0.64, 0.42), BONE_SHADOW),
            Part::cube("torso", v(0.0, 0.98, 0.08), v(1.0, 0.82, 0.62), BONE).rot(20.0, 0.0, 0.0),
            Part::cube("arm_l", v(-0.56, 1.02, 0.42), v(0.2, 0.2, 0.8), BONE_SHADOW).rot(-12.0, 0.0, 0.0),
            Part::cube("arm_r", v(0.56, 0.98, 0.42), v(0.2, 0.2, 0.8), BONE_SHADOW).rot(-4.0, 0.0, 0.0),
            Part::sphere("head", v(0.0, 1.46, 0.36), s(0.46), BONE),
            eyes("eyes", v(0.0, 1.5, 0.58), v(0.28, 0.07, 0.05)),
        ],
        "runner" => vec![
            Part::cube("leg_l", v(-0.13, 0.4, 0.05), v(0.11, 0.8, 0.11), BONE_SHADOW).rot(24.0, 0.0, 0.0),
            Part::cube("leg_r", v(0.13, 0.4, -0.05), v(0.11, 0.8, 0.11), BONE_SHADOW).rot(-22.0, 0.0, 0.0),
            Part::cube("spine", v(0.0, 1.12, 0.18), v(0.16, 0.8, 0.16), BONE).rot(28.0, 0.0, 0.0),
            Part::cube("ribs", v(0.0, 1.18, 0.26), v(0.48, 0.4, 0.3), BONE).rot(28.0, 0.0, 0.0),
            Part::sphere("skull", v(0.0, 1.6, 0.48), s(0.34), BONE),
            eyes("eyes", v(0.0, 1.63, 0.64), v(0.2, 0.05, 0.04)),
        ],
        "wraith" => {
            // A tattered hood: the hem narrows and flares in layers, with no feet.
            let hood = Profile {
                points: vec![[0.0, -1.1], [0.32, -0.8], [0.18, -0.62], [0.46, -0.25], [0.32, -0.08], [0.58, 0.3], [0.56, 0.68], [0.4, 0.95], [0.0, 1.1]],
                segments: 9,
            };
            let mut shroud = Part::new("shroud", Shape::Lathe, Vec3::ZERO, Vec3::ONE, GRAVE_DEEP).glass(0.3).glow(0.25);
            shroud.profile = Some(hood);
            vec![
                shroud,
                Part::sphere("core", v(0.0, 0.3, 0.0), v(0.3, 0.55, 0.3), GRAVE_GLOW).glow(0.9),
                eyes("eyes", v(0.0, 0.72, 0.44), v(0.3, 0.07, 0.05)),
            ]
        }
        "colossus" => {
            let mut m = vec![
                Part::cube("leg_l", v(-0.8, 0.95, 0.0), v(0.55, 1.9, 0.55), BONE_SHADOW).rot(10.0, 0.0, 0.0),
                Part::cube("leg_r", v(0.8, 0.95, 0.0), v(0.55, 1.9, 0.55), BONE_SHADOW).rot(-10.0, 0.0, 0.0),
                Part::cube("pelvis", v(0.0, 2.0, 0.0), v(2.1, 0.6, 1.0), BONE),
                Part::cube("spine", v(0.0, 4.3, 0.35), v(0.6, 4.8, 0.6), BONE_SHADOW).rot(12.0, 0.0, 0.0),
                Part::sphere("skull", v(0.0, 7.1, 1.25), v(1.4, 1.15, 1.5), BONE),
                Part::cube("jaw", v(0.0, 6.5, 1.55), v(0.95, 0.3, 0.7), BONE_SHADOW),
                eyes("eyes", v(0.0, 7.25, 1.98), v(0.8, 0.16, 0.1)).glow(3.0),
                Part::cube("arm_l", v(-1.9, 4.6, 0.9), v(0.4, 3.4, 0.4), BONE).rot(-24.0, 0.0, 8.0),
                Part::cube("arm_r", v(1.9, 4.6, 0.9), v(0.4, 3.4, 0.4), BONE).rot(-30.0, 0.0, -8.0),
            ];
            let ribs = [("rib_1", "rib_1l", "rib_1r"), ("rib_2", "rib_2l", "rib_2r"), ("rib_3", "rib_3l", "rib_3r"), ("rib_4", "rib_4l", "rib_4r")];
            for (i, (front, l, r)) in ribs.iter().enumerate() {
                let y = 3.3 + 0.75 * i as f32;
                let z = 0.75 + 0.15 * i as f32;
                let w = 2.6 - 0.2 * i as f32;
                m.push(Part::cube(front, v(0.0, y, z), v(w, 0.2, 0.2), BONE));
                m.push(Part::cube(l, v(-w / 2.0, y, z - 0.6), v(0.2, 0.2, 1.3), BONE));
                m.push(Part::cube(r, v(w / 2.0, y, z - 0.6), v(0.2, 0.2, 1.3), BONE));
            }
            m
        }
        _ => vec![
            Part::cube("body", v(0.0, 0.85, 0.0), v(0.8, 1.7, 0.5), BONE),
            eyes("eyes", v(0.0, 1.45, 0.27), v(0.3, 0.07, 0.04)),
        ],
    }
}

/// A projectile from a stone of `kind` (one part: bolts are small and numerous).
pub fn projectile_model(kind: &str) -> Part {
    match kind {
        "brazier" => Part::sphere("bolt", Vec3::ZERO, s(0.48), FIRE_FLAME).glow(3.5),
        "sky_obelisk" => Part::cube("bolt", Vec3::ZERO, v(0.14, 0.14, 0.9), RUNE_PALE).glow(3.0),
        _ => Part::sphere("bolt", Vec3::ZERO, s(0.3), RUNE_GLOW).glow(3.0),
    }
}

/// A low dry-stone wall across a lane (along local X), `hp` 0..1 of its health: damaged walls
/// lose height. `seed` makes each wall's blocks a little different.
pub fn wall_model(seed: u32, hp: f32) -> Vec<Part> {
    let jitter = |k: u32| ((seed.wrapping_mul(2_654_435_761).wrapping_add(k * 40_503) >> 8) % 1000) as f32 / 1000.0 - 0.5;
    let h = 0.9 * (0.45 + 0.55 * hp.clamp(0.0, 1.0));
    let mut m = Vec::new();
    let names = ["block_1", "block_2", "block_3"];
    for (i, name) in names.iter().enumerate() {
        let x = -0.62 + 0.62 * i as f32;
        let bh = h * (0.9 + 0.2 * jitter(i as u32));
        let shade = 0.94 + 0.12 * jitter(10 + i as u32);
        let c = [CARVED[0] * shade, CARVED[1] * shade, CARVED[2] * shade];
        m.push(Part::cube(name, v(x + 0.06 * jitter(20 + i as u32), bh / 2.0, 0.04 * jitter(30 + i as u32)), v(0.66, bh, 0.6), c)
            .rot(0.0, 6.0 * jitter(40 + i as u32), 3.0 * jitter(50 + i as u32))
            .rough(0.85));
    }
    m.push(Part::cube("rune_line", v(0.0, h + 0.02, 0.0), v(1.7, 0.04, 0.08), RUNE_GLOW).glow(1.2 * hp.max(0.2)));
    m
}

/// A tombstone (the graveyard's lives). `index` varies the lichen.
pub fn tombstone_model(index: usize) -> Vec<Part> {
    // Lichen is a neutral here: green belongs to the dead (STYLE.md), so living stone stays grey.
    let c = if index % 3 == 1 { hex(0x9a978c) } else { GRAVE_STONE };
    vec![
        Part::cube("slab", v(0.0, 0.4, 0.0), v(0.5, 0.8, 0.15), c).rough(0.9),
        Part::sphere("crown", v(0.0, 0.8, 0.0), v(0.5, 0.26, 0.15), c).rough(0.9),
    ]
}
