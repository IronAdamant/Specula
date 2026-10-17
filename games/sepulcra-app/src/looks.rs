//! What every thing in Sepulcra looks like: the STYLE.md palette, which procedural recipe
//! (`games/sepulcra/assets/recipes/`) draws each kind of thing, how its creatures animate
//! (clips of `assets/anims/`), and the few primitives (bolts, flashes, the slab's blocks) that
//! have no recipe yet.
//!
//! Models are in metres, with the origin where the thing stands (feet, base), +Y up and +Z
//! forward (the way an enemy walks). Gameplay pieces are a little oversized, per the style bible.

use specula::world::{LightSpec, Look, ModelSpec, Profile, Shape};
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
    pub const ROOTS: [f32; 3] = hex(0x5a4632);
    pub const BONE: [f32; 3] = hex(0xd8cfb8);
    pub const BONE_SHADOW: [f32; 3] = hex(0x9c937e);
    pub const GRAVE_STONE: [f32; 3] = hex(0xa7a39a);
    pub const LICHEN: [f32; 3] = hex(0x7d8a6a);
    pub const SKY: [f32; 3] = hex(0x0b1020);
    pub const MIST: [f32; 3] = hex(0x2a3448);
    /// Fog below the slab: darker than MIST so cliffs fade into the night, not into a haze.
    pub const DEEP_MIST: [f32; 3] = hex(0x161c2a);
    /// The sky below the horizon (the void under the slab).
    pub const VOID: [f32; 3] = hex(0x06080e);
    pub const MOON: [f32; 3] = hex(0xc9d6ff);
    /// The moon's light on the slab (the disc stays MOON).
    pub const MOONLIGHT: [f32; 3] = hex(0xd0d9f0);
    /// The night's fill light: MIST with half its blue taken out (shadowed cliffs stay readable, blue).
    pub const NIGHT_FILL: [f32; 3] = hex(0x2f3850);
    pub const IRON: [f32; 3] = hex(0x5c6168);
    /// The chalk scar on lane-jump ledges (a pale neutral).
    pub const CHALK: [f32; 3] = hex(0xd6d2c4);
    /// Invalid placement. The style bible has no UI red yet; a muted brick keeps clear of both
    /// the player's cyan and the enemy's green.
    pub const INVALID: [f32; 3] = hex(0xb03a2e);
}

use palette::*;

/// A glowing marker: its colour (sRGB) and emissive strength.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    pub color: [f32; 3],
    pub emissive: f32,
}

/// The player's highlights on the slab (STYLE.md: rune cyan for the cursor and selection). They
/// glow in the family's saturated colour: a pale colour (`RUNE_PALE`) times a glow lands in the
/// tone curve's shoulder on every channel and reads as white, worse under a brazier's fire, and
/// the grade's negative vibrance takes the rest of its little saturation (the "near-white slab"
/// under a selected stone). See `highlights_read_as_player_cyan` in the tests.
pub mod highlight {
    use super::palette::*;
    use super::Glow;
    /// The build cursor on a free cell.
    pub const VALID: Glow = Glow { color: RUNE_GLOW, emissive: 1.4 };
    pub const INVALID: Glow = Glow { color: super::palette::INVALID, emissive: 1.4 };
    /// The mark under a selected stone or wall (its panel is open): the cursor's cyan, brighter.
    pub const SELECTED: Glow = Glow { color: RUNE_GLOW, emissive: 2.0 };
    /// The dots of a range ring.
    pub const RANGE_DOT: Glow = Glow { color: RUNE_GLOW, emissive: 2.0 };
}

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

/// A projectile from a stone of `kind` (one part: bolts are small and numerous).
pub fn projectile_model(kind: &str) -> Part {
    match kind {
        "brazier" => Part::sphere("bolt", Vec3::ZERO, s(0.42), FIRE_CORE).glow(4.0),
        "sky_obelisk" => Part::cube("bolt", Vec3::ZERO, v(0.12, 0.12, 1.1), RUNE_PALE).glow(4.0),
        _ => Part::cube("bolt", Vec3::ZERO, v(0.16, 0.16, 0.7), RUNE_GLOW).glow(3.5),
    }
}

/// The recipe of a stone kind (its id in `stones.json`), carved to `tier` 0..=2.
pub fn stone_spec(kind: &str, tier: u8) -> ModelSpec {
    let recipe = match kind {
        "rune_pillar" => "recipes/rune_pillar.json",
        "brazier" => "recipes/brazier_cairn.json",
        "sky_obelisk" => "recipes/sky_obelisk.json",
        _ => "recipes/warding_stone.json",
    };
    ModelSpec::new(recipe).with_param("tier", f64::from(tier.min(2)) + 1.0)
}

/// A stone's own light, if it has one: the brazier's fire (warm, from tier I) and the rune glow
/// of the others at tier III (STYLE.md: small cyan lights at tier III only, for the light
/// budget). Offsets are from the stone's base.
pub fn stone_light(kind: &str, tier: u8) -> Option<LightSpec> {
    match kind {
        "brazier" => Some(LightSpec { intensity: 2.2, range: 4.5, color: Some(FIRE_FLAME), offset: v(0.0, 1.9, 0.0) }),
        _ if tier >= 2 => {
            let h = if kind == "sky_obelisk" { 3.6 } else { 2.9 };
            Some(LightSpec { intensity: 1.4, range: 3.5, color: Some(RUNE_GLOW), offset: v(0.0, h, 0.0) })
        }
        _ => None,
    }
}

/// A one-cell wall; `seed` varies the stones.
pub fn wall_spec(seed: u32) -> ModelSpec {
    ModelSpec::new("recipes/wall_segment.json").with_seed(u64::from(seed % 1_000_000))
}

/// A tombstone of the graveyard; `index` varies its stones and lean.
pub fn tombstone_spec(index: usize) -> ModelSpec {
    let tilt = ((index * 37) % 13) as f64 / 6.0 - 1.0;
    ModelSpec::new("recipes/tombstone.json").with_seed(606 + index as u64 * 7).with_param("tilt", tilt)
}

/// A 2 m fence segment (along X).
pub fn fence_spec(index: usize) -> ModelSpec {
    ModelSpec::new("recipes/graveyard_fence.json").with_seed(707 + index as u64)
}

/// A chunk of the slab's underside (about 10 × 10 m, `height` tall, its flat top at `height`).
pub fn underside_spec(seed: u64, height: f64) -> ModelSpec {
    ModelSpec::new("recipes/slab_cliff.json").with_seed(seed).with_param("height", height)
}

/// What a creature does, as far as its animation is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Act {
    /// Standing (spawning, waiting).
    Idle,
    /// Walking, running or flying along its path.
    Move,
    /// Breaking a wall.
    Attack,
    /// A runner leaping a cracked ledge, a wraith's dive, the colossus raising shamblers.
    Special,
    Death,
}

/// A clip of the creature's anim set, how fast it plays (clip seconds per game second at
/// normal speed) and whether it loops.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clip {
    pub name: &'static str,
    pub rate: f32,
    pub looping: bool,
    /// Clip length in seconds (as in `assets/anims/*.json`; a test checks).
    pub duration: f32,
}

const fn clip(name: &'static str, rate: f32, looping: bool, duration: f32) -> Clip {
    Clip { name, rate, looping, duration }
}

/// How an enemy kind (its id in `enemies.json`) is drawn.
#[derive(Clone, Copy, Debug)]
pub struct Creature {
    pub recipe: &'static str,
    /// Uniform scale over the recipe's real size (STYLE.md: a little oversized to read at range).
    pub scale: f32,
    pub idle: Clip,
    pub walk: Clip,
    pub attack: Clip,
    pub special: Clip,
    pub death: Clip,
}

/// The creature for an enemy kind. Walk rates are tuned so the feet roughly keep up with the
/// simulated speed (shambler 0.8 m/s, runner 2.2, colossus 0.4) at this scale, within what still
/// reads as the authored gait.
pub fn creature(kind: &str) -> Creature {
    match kind {
        "runner" => Creature {
            recipe: "recipes/skeleton_runner.json",
            scale: 1.2,
            idle: clip("idle", 1.0, true, 1.6),
            walk: clip("sprint", 0.62, true, 0.56),
            attack: clip("idle", 1.6, true, 1.6),
            special: clip("leap", 1.0, false, 0.95),
            death: clip("death", 1.0, false, 1.6),
        },
        "wraith" => Creature {
            recipe: "recipes/wraith.json",
            scale: 1.2,
            idle: clip("float", 1.0, true, 2.4),
            walk: clip("float", 1.0, true, 2.4),
            attack: clip("swoop", 1.0, true, 1.4),
            special: clip("swoop", 0.8, false, 1.4),
            death: clip("death", 1.0, false, 1.8),
        },
        "colossus" => Creature {
            recipe: "recipes/bone_colossus.json",
            scale: 1.0,
            idle: clip("walk", 0.2, true, 2.8),
            walk: clip("walk", 0.55, true, 2.8),
            attack: clip("summon", 0.9, true, 3.0),
            special: clip("summon", 1.0, false, 3.0),
            death: clip("death", 1.0, false, 3.4),
        },
        _ => Creature {
            recipe: "recipes/shambler.json",
            scale: 1.2,
            idle: clip("idle", 1.0, true, 3.2),
            walk: clip("walk", 1.15, true, 1.7),
            attack: clip("attack", 1.0, true, 1.3),
            special: clip("attack", 1.0, false, 1.3),
            death: clip("death", 1.0, false, 2.4),
        },
    }
}

impl Creature {
    pub fn clip(&self, act: Act) -> Clip {
        match act {
            Act::Idle => self.idle,
            Act::Move => self.walk,
            Act::Attack => self.attack,
            Act::Special => self.special,
            Act::Death => self.death,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula_core::Json;

    const ANIMS: [(&str, &str); 4] = [
        ("recipes/shambler.json", include_str!("../../sepulcra/assets/anims/shambler.json")),
        ("recipes/skeleton_runner.json", include_str!("../../sepulcra/assets/anims/skeleton_runner.json")),
        ("recipes/wraith.json", include_str!("../../sepulcra/assets/anims/wraith.json")),
        ("recipes/bone_colossus.json", include_str!("../../sepulcra/assets/anims/bone_colossus.json")),
    ];

    #[test]
    fn every_clip_exists_in_its_anim_set_with_its_length() {
        for kind in ["shambler", "runner", "wraith", "colossus"] {
            let c = creature(kind);
            let text = ANIMS.iter().find(|(r, _)| *r == c.recipe).expect("anim set").1;
            let set = Json::parse(text).unwrap();
            let clips = set.get("clips").and_then(Json::as_array).unwrap();
            for act in [Act::Idle, Act::Move, Act::Attack, Act::Special, Act::Death] {
                let want = c.clip(act);
                let found = clips.iter().find(|j| j.get("name").and_then(Json::as_str) == Some(want.name));
                let found = found.unwrap_or_else(|| panic!("{kind}: no clip '{}'", want.name));
                let d = found.get("duration").and_then(Json::as_f64).unwrap() as f32;
                assert!((d - want.duration).abs() < 1e-3, "{kind}/{}: {d} s, the table says {}", want.name, want.duration);
            }
        }
    }

    #[test]
    fn stones_have_recipes_for_every_tier() {
        for kind in ["warding", "rune_pillar", "brazier", "sky_obelisk"] {
            for tier in 0..3 {
                let s = stone_spec(kind, tier);
                assert!(s.check().is_ok(), "{kind} {tier}");
                assert_eq!(s.params, vec![("tier".to_owned(), f64::from(tier) + 1.0)]);
            }
        }
        assert!(stone_light("brazier", 0).is_some() && stone_light("warding", 1).is_none() && stone_light("warding", 2).is_some());
        assert!(wall_spec(3).check().is_ok() && tombstone_spec(9).check().is_ok() && fence_spec(1).check().is_ok());
        assert!(underside_spec(1, 15.0).check().is_ok());
    }

    /// sRGB to linear, as the engine converts the colours it is given.
    fn lin(c: [f32; 3]) -> [f32; 3] {
        c.map(|v| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) })
    }

    /// The ACES fit the engine tone maps with (`shaders/mesh.frag`, the post tonemap pass).
    fn aces(x: f32) -> f32 {
        ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
    }

    /// Linear to sRGB, as the target encodes it.
    fn srgb(c: f32) -> f32 {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    }

    fn saturation(c: [f32; 3]) -> f32 {
        let (max, min) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
        (max - min) / max.max(1e-6)
    }

    /// The player's highlights stay player cyan where they are lit the most: on the slab under a
    /// brazier's fire, in the night's ambient and moonlight. The skeptic's "near-white slab under
    /// a selected brazier" was the selection mark itself (`RUNE_PALE` at glow 1.4, identical
    /// under a warding stone and with the fire's light off): a pale colour times a glow is past
    /// the tone curve's shoulder on every channel. The fire only adds a little red on top.
    ///
    /// This is a **pre-grade** model: lighting (ambient, moon on a flat top, the fire with the
    /// engine's falloff), emission, exposure, ACES and the sRGB encoding. It leaves out what only
    /// lowers saturation further: the grade (`game::night`: saturation 0.8, vibrance -1), bloom
    /// spreading the glow, fog, SSAO and the moon's specular. So its numbers are upper bounds:
    /// it puts the new selection mark at about saturation 0.49 (the bound below is 0.45) where a live capture measured
    /// about (153, 240, 232), saturation 0.36, still clearly cyan; and the old mark at about 0.05
    /// (near-white), which the grade only whitens. That `show_cursor` really applies these glows
    /// is `scene::tests::cursor_and_range_dots_wear_the_highlight_glows`.
    #[test]
    fn highlights_read_as_player_cyan() {
        let mut world = specula::world::World::default();
        crate::game::night(&mut world);
        let env = &world.env().light;
        let ambient = lin(env.ambient_color).map(|c| c * env.ambient_intensity);
        // The moon on a flat top face.
        let moon = lin(env.sun_color).map(|c| c * env.sun_intensity * env.sun_direction[1].max(0.0));
        // A brazier's fire straight above the mark's top face (6 cm over the stone's base), with
        // the engine's falloff (`gather_light` in shaders/mesh_frag.glsl).
        for tier in 0..3 {
            let fire = stone_light("brazier", tier).expect("a brazier has its fire");
            let d = fire.offset.y - 0.06;
            assert!(d > 1.0, "the fire burns in the bowl, well above the base: {d} m");
            let window = (1.0 - (d / fire.range).powi(4)).clamp(0.0, 1.0);
            let k = fire.intensity * window * window / (d * d + 0.0025);
            let fire_light = lin(fire.color.expect("fire colour")).map(|c| c * k);
            let light: [f32; 3] = std::array::from_fn(|i| ambient[i] + moon[i] + fire_light[i]);
            // Tone mapped, then sRGB encoded: the values written to the image (before the grade).
            let shown = |g: Glow| {
                let a = lin(g.color);
                std::array::from_fn::<f32, 3, _>(|i| srgb(aces(env.exposure * a[i] * (g.emissive + light[i]))))
            };
            for (what, g) in [("the build cursor", highlight::VALID), ("the selection mark", highlight::SELECTED), ("a range dot", highlight::RANGE_DOT)] {
                let c = shown(g);
                assert!(saturation(c) >= 0.45 && c[1] > c[0] && c[2] > c[0], "tier {tier}: {what} shows {c:?}: not player cyan");
            }
            // Brighter than the hover, so a selection reads as one.
            assert!(shown(highlight::SELECTED)[1] > shown(highlight::VALID)[1]);
            // What it was: near-white.
            let old = shown(Glow { color: RUNE_PALE, emissive: 1.4 });
            assert!(saturation(old) < 0.1 && old.iter().all(|&c| c > 0.85), "tier {tier}: the old mark showed {old:?}");
        }
    }
}
