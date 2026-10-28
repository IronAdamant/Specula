//! Effects as data: up to [`MAX_EMITTERS`] emitters and an optional light, parsed from JSON with
//! structured errors, written back out canonically, and summarised analytically (how far a
//! particle can travel, how many can live at once, when a one-shot is over).

use crate::curve::{Curve, Gradient};
use crate::error::{read, Path, Result, VfxError};
use crate::textures;
use specula_core::{dmath, Json};

pub const MAX_EMITTERS: usize = 8;
pub const MAX_BURSTS: usize = 8;
/// Particles per second per emitter.
pub const MAX_RATE: f32 = 2000.0;
/// Seconds a particle may live.
pub const MAX_LIFETIME: f32 = 10.0;
/// Particles per burst.
pub const MAX_BURST_COUNT: u64 = 1000;
pub const MAX_CYCLES: u64 = 1000;
pub const MAX_SPEED: f32 = 100.0;
/// Particle size in metres.
pub const MAX_SIZE: f32 = 50.0;
pub const MAX_ACCEL: f32 = 100.0;
pub const MAX_DRAG: f32 = 50.0;
/// Seconds of emission (and burst times).
pub const MAX_DURATION: f32 = 3600.0;
/// Flipbook columns or rows.
pub const MAX_GRID: u64 = 16;
pub const MAX_EMISSIVE: f32 = 100.0;
/// Spawn shape extents in metres.
pub const MAX_EXTENT: f32 = 100.0;
/// Characters of a description or emitter name.
pub const MAX_TEXT: usize = 200;
/// Characters of an asset texture path.
pub const MAX_PATH: usize = 256;
/// Most particles one effect may have alive at once, by its analytic peak (a hard safety limit,
/// far above any frame budget; budgets are `vfx_check`'s and the frame's).
pub const MAX_PEAK: f32 = 50_000.0;
/// The most flashes a second an effect light may make (WCAG 2.3.1).
pub const MAX_FLASH_HZ: f32 = 3.0;
/// The shortest interval a repeating burst may have (seconds): faster than any frame is a rate.
pub const MIN_INTERVAL: f32 = 1.0 / 120.0;

/// Where a particle is born, relative to the emitter (local axes; scaled by the entity).
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Point,
    /// In (or with `surface`, on) a sphere.
    Sphere { radius: f32, surface: bool },
    /// In an axis-aligned box `size` metres across, centred on the emitter.
    Box { size: [f32; 3] },
    /// On a disc in the local XZ plane.
    Disc { radius: f32 },
    /// On the base disc of a cone opening along the emitter's direction by `angle_deg`
    /// (velocities point within the cone).
    Cone { radius: f32, angle_deg: f32 },
    /// Along a segment on the local X axis, centred.
    Line { length: f32 },
}

impl Shape {
    /// The farthest a spawn point can be from the emitter (unscaled).
    pub fn extent(&self) -> f32 {
        match self {
            Shape::Point => 0.0,
            Shape::Sphere { radius, .. } | Shape::Disc { radius } | Shape::Cone { radius, .. } => *radius,
            Shape::Box { size } => 0.5 * (size[0] * size[0] + size[1] * size[1] + size[2] * size[2]).sqrt(),
            Shape::Line { length } => 0.5 * length,
        }
    }
}

/// How a particle's initial velocity points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    /// Around this (unit) direction, within the emitter's spread cone.
    Along([f32; 3]),
    /// Away from the emitter's centre (through the spawn point; a random direction at the centre).
    Radial,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Burst {
    /// Seconds after the emitter starts.
    pub time: f32,
    pub count: u32,
    /// Times it fires (0 = forever, with `interval` > 0).
    pub cycles: u32,
    /// Seconds between cycles.
    pub interval: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Noise {
    /// Displacement per second of age, metres (about: the field's magnitude averages near 1).
    pub strength: f32,
    /// Spatial frequency: lattice cells per metre.
    pub frequency: f32,
    /// How fast the field evolves (per second).
    pub speed: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flipbook {
    pub cols: u32,
    pub rows: u32,
    /// Frames used (≤ cols × rows), in reading order.
    pub frames: u32,
    /// Frames per second of age (ignored with `over_life`).
    pub fps: f32,
    /// One pass over the frames across the particle's life.
    pub over_life: bool,
    /// Each particle starts on a random frame.
    pub random_start: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollideMode {
    /// The particle dies when it reaches the surface.
    Kill,
    /// It is held on the surface and slides along it.
    Slide,
    /// It stops where it first touched the surface.
    Stick,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Surface {
    /// A horizontal plane this many metres above (below, if negative) the emitter's origin.
    Plane(f32),
    /// The world's ground (the engine's ground grid: entities tagged `ground`).
    Ground,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Collide {
    pub surface: Surface,
    pub mode: CollideMode,
    /// Kept this far above the surface (a particle's radius, say).
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lod {
    /// Metres from the camera where thinning starts.
    pub start: f32,
    /// Metres where it reaches `min`.
    pub end: f32,
    /// Fraction of the particles kept at `end` and beyond (0..1).
    pub min: f32,
}

impl Default for Lod {
    fn default() -> Self {
        Lod { start: 30.0, end: 90.0, min: 0.25 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    /// Particles move with the emitter (fire on a brazier).
    Local,
    /// Particles stay where they were born (trails behind something moving).
    World,
}

/// A texture: a built-in (procedural, see [`crate::textures`]) or a PNG under the assets folder.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TextureRef {
    Builtin(&'static str),
    Asset(String),
}

impl TextureRef {
    pub fn parse(v: &Json, at: &Path, decal: bool) -> Result<TextureRef> {
        let s = read::string(v, at)?;
        let names = if decal { textures::DECAL_NAMES } else { textures::PARTICLE_NAMES };
        if let Some(n) = names.iter().find(|n| **n == s) {
            return Ok(TextureRef::Builtin(n));
        }
        if s.ends_with(".png") && !s.is_empty() {
            let ok = s.len() <= MAX_PATH
                && !s.starts_with('/')
                && !s.contains('\\')
                && !s.contains(':')
                && s.split('/').all(|seg| !seg.is_empty() && seg != "." && seg != "..");
            if ok {
                return Ok(TextureRef::Asset(s.to_owned()));
            }
            return Err(VfxError::new("invalid_value", at.as_str(), format!("'{at}': a texture path is relative to the assets folder, with '/' separators and no '..' (at most {MAX_PATH} characters)")).value(v));
        }
        Err(VfxError::new("unknown_texture", at.as_str(), format!("'{at}': no built-in texture '{s}' (or give a .png path under the assets folder)")).allowed(names.join(", ")).value(v))
    }

    pub fn name(&self) -> &str {
        match self {
            TextureRef::Builtin(n) => n,
            TextureRef::Asset(p) => p,
        }
    }
}

/// One stream of particles.
#[derive(Clone, Debug, PartialEq)]
pub struct Emitter {
    pub name: String,
    /// Particles per second (0: bursts only).
    pub rate: f32,
    pub bursts: Vec<Burst>,
    /// Seconds after the effect starts before this emitter does.
    pub delay: f32,
    /// Seconds it emits at `rate` (`None`: forever).
    pub duration: Option<f32>,
    /// Seconds, min..max.
    pub lifetime: [f32; 2],
    pub shape: Shape,
    pub direction: Direction,
    pub spread_deg: f32,
    /// Metres per second, min..max.
    pub speed: [f32; 2],
    /// World acceleration, m/s².
    pub gravity: [f32; 3],
    /// Linear drag per second.
    pub drag: f32,
    pub noise: Option<Noise>,
    /// Metres over life.
    pub size: Curve,
    /// Per-particle size multiplier, min..max.
    pub size_random: [f32; 2],
    pub color: Gradient,
    pub alpha: Curve,
    /// HDR multiplier of the colour.
    pub emissive: f32,
    /// Initial rotation, degrees, min..max.
    pub rotation_deg: [f32; 2],
    /// Degrees per second, min..max.
    pub spin_deg: [f32; 2],
    pub texture: TextureRef,
    pub flipbook: Option<Flipbook>,
    /// 0 = alpha blending, 1 = additive (between: a mix).
    pub additive: f32,
    /// Metres of depth fade where particles meet geometry (0 = hard edges).
    pub soft: f32,
    /// Lit by ambient, sun and point lights (smoke, dust); unlit glows as its colour.
    pub lit: bool,
    /// Sparks: the quad is stretched along the velocity × this many seconds (0 = square).
    pub stretch: f32,
    pub space: Space,
    /// World space: fraction of the emitter's own velocity particles inherit at birth.
    pub inherit_velocity: f32,
    pub collide: Option<Collide>,
    pub lod: Lod,
    /// Higher keeps its particles first when the frame's budget is short.
    pub priority: f32,
    /// Emitters smaller than this on screen (pixels) are skipped.
    pub cull_px: f32,
}

/// An effect's light: a point light at the emitter, optionally flickering.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    /// sRGB.
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    /// Local offset from the emitter (metres).
    pub offset: [f32; 3],
    /// Brightness variation, 0..1 (0 = steady).
    pub flicker: f32,
    /// How fast it varies (noise cycles per second).
    pub flicker_hz: f32,
    /// Seconds over which it dims to nothing after the effect starts (a flash); `None`: steady.
    pub duration: Option<f32>,
}

impl Light {
    /// Brightness multiplier at `age` seconds after the effect started (`seed` varies the
    /// flicker between instances): flicker noise and the dimming of a flash.
    pub fn level(&self, age: f64, seed: u32) -> f32 {
        let flicker = if self.flicker > 0.0 { 1.0 - self.flicker * 0.5 * (1.0 + crate::noise::noise1(age * f64::from(self.flicker_hz), seed)) } else { 1.0 };
        let dim = match self.duration {
            Some(d) if d > 0.0 => (1.0 - (age / f64::from(d)) as f32).clamp(0.0, 1.0),
            _ => 1.0,
        };
        if age < 0.0 {
            0.0
        } else {
            flicker * dim
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    pub description: String,
    pub emitters: Vec<Emitter>,
    pub light: Option<Light>,
}

const EFFECT_KEYS: &[&str] = &["description", "emitters", "light"];
const EMITTER_KEYS: &[&str] = &[
    "name", "rate", "bursts", "delay", "duration", "lifetime", "shape", "direction", "spread_deg", "speed", "gravity", "drag",
    "noise", "size", "size_random", "color", "alpha", "emissive", "rotation_deg", "spin_deg", "texture", "flipbook", "blend",
    "soft", "lit", "stretch", "space", "inherit_velocity", "collide", "lod", "priority", "cull_px",
];
const SHAPES: &str = "point, sphere, box, disc, cone, line";

impl Effect {
    /// Parses and validates an effect.
    /// Parses an effect; refuses one that would flash faster than [`MAX_FLASH_HZ`] (lights and
    /// particles: [`crate::flash`]).
    pub fn parse(v: &Json) -> Result<Effect> {
        let effect = Effect::parse_unchecked(v)?;
        let f = crate::flash::flash(&effect, &crate::component::Params::default());
        if f.hz > MAX_FLASH_HZ {
            let field = f.emitter.map_or_else(|| "emitters".to_string(), |k| format!("emitters[{k}]"));
            return Err(VfxError::new(
                "photosensitive",
                field,
                format!(
                    "{}: more than {MAX_FLASH_HZ} flashes a second can trigger photosensitive seizures (WCAG 2.3.1); slow the bursts or streams, overlap or soften the particles, or make them smaller or dimmer",
                    f.describe()
                ),
            )
            .allowed(format!("at most {MAX_FLASH_HZ} flashes a second")));
        }
        Ok(effect)
    }

    /// [`Effect::parse`] without the flash refusal (for measuring).
    pub(crate) fn parse_unchecked(v: &Json) -> Result<Effect> {
        let at = Path::root("");
        read::keys(v, &Path::root("effect"), EFFECT_KEYS)?;
        let description = match v.get("description") {
            None | Some(Json::Null) => String::new(),
            Some(d) => {
                let s = read::string(d, &at.key("description"))?;
                if s.chars().count() > MAX_TEXT {
                    return Err(VfxError::new("invalid_value", "description", format!("'description' is at most {MAX_TEXT} characters")));
                }
                s.to_owned()
            }
        };
        let list = v.get("emitters").and_then(Json::as_array).ok_or_else(|| VfxError::new("missing_field", "emitters", "'emitters' is required: a list of 1 to 8 emitters"))?;
        if list.is_empty() || list.len() > MAX_EMITTERS {
            return Err(VfxError::new("too_many", "emitters", format!("an effect has 1 to {MAX_EMITTERS} emitters (got {})", list.len())).allowed(format!("1..={MAX_EMITTERS}")));
        }
        let mut emitters = Vec::with_capacity(list.len());
        for (i, e) in list.iter().enumerate() {
            let em = Emitter::parse(e, &at.key("emitters").index(i), i)?;
            if emitters.iter().any(|o: &Emitter| o.name == em.name) {
                return Err(VfxError::new("invalid_value", at.key("emitters").index(i).key("name").as_str(), format!("two emitters are named '{}'", em.name)));
            }
            emitters.push(em);
        }
        let light = match v.get("light") {
            None | Some(Json::Null) => None,
            Some(l) => Some(Light::parse(l, &at.key("light"))?),
        };
        let effect = Effect { description, emitters, light };
        let peak = effect.peak();
        if peak > MAX_PEAK {
            return Err(VfxError::new("too_many", "emitters", format!("up to {peak:.0} particles alive at once (rates × lifetimes, bursts × overlapping cycles); at most {MAX_PEAK:.0}"))
                .allowed(format!("peak <= {MAX_PEAK:.0}")));
        }
        Ok(effect)
    }

    pub fn to_json(&self) -> Json {
        let mut f = Vec::new();
        if !self.description.is_empty() {
            f.push(("description", Json::from(self.description.as_str())));
        }
        f.push(("emitters", Json::arr(self.emitters.iter().map(Emitter::to_json))));
        if let Some(l) = &self.light {
            f.push(("light", l.to_json()));
        }
        Json::obj(f)
    }

    /// The longest any particle lives.
    pub fn max_lifetime(&self) -> f32 {
        self.emitters.iter().map(|e| e.lifetime[1]).fold(0.0, f32::max)
    }

    /// The farthest any particle gets from where it was born, plus its half size (metres, at
    /// scale 1, `speed` and `size` multipliers 1).
    pub fn reach(&self) -> f32 {
        self.emitters.iter().map(Emitter::reach).fold(0.0, f32::max)
    }

    /// The most particles one instance can have alive at once (an upper bound).
    pub fn peak(&self) -> f32 {
        self.emitters.iter().map(Emitter::peak).sum()
    }

    /// Seconds after the start when every particle is gone (`None`: it never ends).
    pub fn finished_after(&self) -> Option<f32> {
        let mut end = 0.0f32;
        for e in &self.emitters {
            end = end.max(e.finished_after()?);
        }
        Some(end)
    }

    /// Any emitter keeps its particles where they were born.
    pub fn world_space(&self) -> bool {
        self.emitters.iter().any(|e| e.space == Space::World)
    }

    /// Every texture the effect samples.
    pub fn textures(&self) -> impl Iterator<Item = &TextureRef> {
        self.emitters.iter().map(|e| &e.texture)
    }
}

impl Emitter {
    fn parse(v: &Json, at: &Path, index: usize) -> Result<Emitter> {
        read::keys(v, at, EMITTER_KEYS)?;
        let name = match v.get("name") {
            None | Some(Json::Null) => format!("emitter_{index}"),
            Some(n) => {
                let s = read::string(n, &at.key("name"))?;
                if s.is_empty() || s.chars().count() > 64 {
                    return Err(VfxError::new("invalid_value", at.key("name").as_str(), "an emitter's name is 1 to 64 characters"));
                }
                s.to_owned()
            }
        };
        let rate = read::num_or(v, "rate", at, 0.0, MAX_RATE, 0.0)?;
        let mut bursts = Vec::new();
        if let Some(b) = v.get("bursts").filter(|b| !b.is_null()) {
            let bp = at.key("bursts");
            let list = b.as_array().ok_or_else(|| VfxError::new("bad_type", bp.as_str(), format!("'{bp}' must be a list of {{time, count, cycles?, interval?}}")))?;
            if list.len() > MAX_BURSTS {
                return Err(VfxError::new("too_many", bp.as_str(), format!("at most {MAX_BURSTS} bursts per emitter")).allowed(format!("0..={MAX_BURSTS}")));
            }
            for (i, x) in list.iter().enumerate() {
                let p = bp.index(i);
                read::keys(x, &p, &["time", "count", "cycles", "interval"])?;
                let count = x.get("count").ok_or_else(|| VfxError::new("missing_field", p.key("count").as_str(), "a burst needs 'count'"))?;
                let burst = Burst {
                    time: read::num_or(x, "time", &p, 0.0, MAX_DURATION, 0.0)?,
                    count: read::int(count, &p.key("count"), 1, MAX_BURST_COUNT)? as u32,
                    cycles: read::int_or(x, "cycles", &p, 0, MAX_CYCLES, 1)? as u32,
                    interval: read::num_or(x, "interval", &p, 0.0, MAX_DURATION, 0.0)?,
                };
                if burst.cycles != 1 && burst.interval < MIN_INTERVAL {
                    return Err(VfxError::new(
                        "invalid_value",
                        p.key("interval").as_str(),
                        format!("a burst that repeats ('cycles' other than 1, 0 = forever) needs an 'interval' of at least {MIN_INTERVAL:.4} s (1/120 s)"),
                    )
                    .allowed(format!("{MIN_INTERVAL:.4}..={MAX_DURATION}")));
                }
                bursts.push(burst);
            }
        }
        if rate == 0.0 && bursts.is_empty() {
            return Err(VfxError::new("missing_field", at.key("rate").as_str(), format!("'{at}' emits nothing: give it a 'rate' (particles per second) or 'bursts'")));
        }
        let duration = match v.get("duration") {
            None | Some(Json::Null) => None,
            Some(d) => Some(read::num(d, &at.key("duration"), 0.0, MAX_DURATION)?),
        };
        let lifetime = read::range_or(v, "lifetime", at, 0.01, MAX_LIFETIME, [1.0, 1.0])?;
        let shape = match v.get("shape") {
            None | Some(Json::Null) => Shape::Point,
            Some(s) => parse_shape(s, &at.key("shape"))?,
        };
        let direction = match v.get("direction") {
            None | Some(Json::Null) => Direction::Along([0.0, 1.0, 0.0]),
            Some(Json::Str(s)) if s == "radial" => Direction::Radial,
            Some(d) => {
                let p = at.key("direction");
                let [x, y, z] = read::vec3(d, &p, 1e3).map_err(|e| VfxError { message: format!("'{p}' must be [x, y, z] (any length above 0) or \"radial\""), ..e })?;
                let len = (x * x + y * y + z * z).sqrt();
                if len < 1e-6 {
                    return Err(VfxError::new("invalid_value", p.as_str(), format!("'{p}' must not be zero")).value(d));
                }
                Direction::Along([x / len, y / len, z / len])
            }
        };
        let flipbook = match v.get("flipbook") {
            None | Some(Json::Null) => None,
            Some(f) => Some(parse_flipbook(f, &at.key("flipbook"))?),
        };
        let additive = match v.get("blend") {
            None | Some(Json::Null) => 1.0,
            Some(Json::Str(s)) if s == "additive" => 1.0,
            Some(Json::Str(s)) if s == "alpha" => 0.0,
            Some(b) if b.as_f64().is_some() => read::num(b, &at.key("blend"), 0.0, 1.0)?,
            Some(b) => return Err(VfxError::new("invalid_value", at.key("blend").as_str(), format!("'{}' must be \"additive\", \"alpha\" or a mix from 0 (alpha) to 1 (additive)", at.key("blend"))).allowed("additive | alpha | 0..=1").value(b)),
        };
        let space = match v.get("space") {
            None | Some(Json::Null) => Space::Local,
            Some(Json::Str(s)) if s == "local" => Space::Local,
            Some(Json::Str(s)) if s == "world" => Space::World,
            Some(s) => return Err(VfxError::new("invalid_value", at.key("space").as_str(), format!("'{}' must be \"local\" or \"world\"", at.key("space"))).allowed("local | world").value(s)),
        };
        let collide = match v.get("collide") {
            None | Some(Json::Null) => None,
            Some(c) => Some(parse_collide(c, &at.key("collide"))?),
        };
        let lod = match v.get("lod") {
            None | Some(Json::Null) => Lod::default(),
            Some(l) => {
                let p = at.key("lod");
                read::keys(l, &p, &["start", "end", "min"])?;
                let d = Lod::default();
                let lod = Lod {
                    start: read::num_or(l, "start", &p, 0.0, 10_000.0, d.start)?,
                    end: read::num_or(l, "end", &p, 0.0, 10_000.0, d.end)?,
                    min: read::num_or(l, "min", &p, 0.0, 1.0, d.min)?,
                };
                if lod.end <= lod.start {
                    return Err(VfxError::new("invalid_value", p.key("end").as_str(), format!("'{p}.end' must be beyond '{p}.start' ({} m)", lod.start)));
                }
                lod
            }
        };
        let noise = match v.get("noise") {
            None | Some(Json::Null) => None,
            Some(n) => {
                let p = at.key("noise");
                read::keys(n, &p, &["strength", "frequency", "speed"])?;
                let noise = Noise {
                    strength: read::num_or(n, "strength", &p, 0.0, 50.0, 0.5)?,
                    frequency: read::num_or(n, "frequency", &p, 0.001, 20.0, 1.0)?,
                    speed: read::num_or(n, "speed", &p, 0.0, 20.0, 0.5)?,
                };
                (noise.strength > 0.0).then_some(noise)
            }
        };
        let texture = match v.get("texture") {
            None | Some(Json::Null) => TextureRef::Builtin("soft"),
            Some(t) => TextureRef::parse(t, &at.key("texture"), false)?,
        };
        Ok(Emitter {
            name,
            rate,
            bursts,
            delay: read::num_or(v, "delay", at, 0.0, MAX_DURATION, 0.0)?,
            duration,
            lifetime,
            shape,
            direction,
            spread_deg: read::num_or(v, "spread_deg", at, 0.0, 180.0, 0.0)?,
            speed: read::range_or(v, "speed", at, 0.0, MAX_SPEED, [0.0, 0.0])?,
            gravity: read::vec3_or(v, "gravity", at, MAX_ACCEL, [0.0; 3])?,
            drag: read::num_or(v, "drag", at, 0.0, MAX_DRAG, 0.0)?,
            noise,
            size: match v.get("size") {
                None | Some(Json::Null) => Curve::constant(0.2),
                Some(s) => Curve::parse(s, &at.key("size"), 0.0, MAX_SIZE)?,
            },
            size_random: read::range_or(v, "size_random", at, 0.0, 10.0, [1.0, 1.0])?,
            color: match v.get("color") {
                None | Some(Json::Null) => Gradient::solid([1.0; 3]),
                Some(c) => Gradient::parse(c, &at.key("color"))?,
            },
            alpha: match v.get("alpha") {
                None | Some(Json::Null) => Curve::parse(&Json::parse("[[0, 1], [1, 0]]").expect("literal"), &at.key("alpha"), 0.0, 1.0)?,
                Some(a) => Curve::parse(a, &at.key("alpha"), 0.0, 1.0)?,
            },
            emissive: read::num_or(v, "emissive", at, 0.0, MAX_EMISSIVE, 1.0)?,
            rotation_deg: read::range_or(v, "rotation_deg", at, -3600.0, 3600.0, [0.0, 0.0])?,
            spin_deg: read::range_or(v, "spin_deg", at, -3600.0, 3600.0, [0.0, 0.0])?,
            texture,
            flipbook,
            additive,
            soft: read::num_or(v, "soft", at, 0.0, 10.0, 0.0)?,
            lit: read::boolean_or(v, "lit", at, false)?,
            stretch: read::num_or(v, "stretch", at, 0.0, 1.0, 0.0)?,
            space,
            inherit_velocity: read::num_or(v, "inherit_velocity", at, 0.0, 2.0, 0.0)?,
            collide,
            lod,
            priority: read::num_or(v, "priority", at, 0.0, 100.0, 1.0)?,
            cull_px: read::num_or(v, "cull_px", at, 0.0, 100.0, 1.5)?,
        })
    }

    pub fn to_json(&self) -> Json {
        let range = |r: [f32; 2]| if r[0] == r[1] { Json::from(r[0]) } else { Json::arr([Json::from(r[0]), Json::from(r[1])]) };
        let v3 = |v: [f32; 3]| Json::arr(v.map(Json::from));
        let mut f = vec![("name", Json::from(self.name.as_str()))];
        if self.rate > 0.0 {
            f.push(("rate", Json::from(self.rate)));
        }
        if !self.bursts.is_empty() {
            f.push((
                "bursts",
                Json::arr(self.bursts.iter().map(|b| {
                    Json::obj([("time", Json::from(b.time)), ("count", Json::from(b.count)), ("cycles", Json::from(b.cycles)), ("interval", Json::from(b.interval))])
                })),
            ));
        }
        if self.delay > 0.0 {
            f.push(("delay", Json::from(self.delay)));
        }
        if let Some(d) = self.duration {
            f.push(("duration", Json::from(d)));
        }
        f.push(("lifetime", range(self.lifetime)));
        f.push(("shape", shape_json(&self.shape)));
        f.push(("direction", match self.direction {
            Direction::Radial => Json::from("radial"),
            Direction::Along(d) => v3(d),
        }));
        f.push(("spread_deg", Json::from(self.spread_deg)));
        f.push(("speed", range(self.speed)));
        if self.gravity != [0.0; 3] {
            f.push(("gravity", v3(self.gravity)));
        }
        if self.drag > 0.0 {
            f.push(("drag", Json::from(self.drag)));
        }
        if let Some(n) = &self.noise {
            f.push(("noise", Json::obj([("strength", Json::from(n.strength)), ("frequency", Json::from(n.frequency)), ("speed", Json::from(n.speed))])));
        }
        f.push(("size", self.size.to_json()));
        if self.size_random != [1.0, 1.0] {
            f.push(("size_random", range(self.size_random)));
        }
        f.push(("color", self.color.to_json()));
        f.push(("alpha", self.alpha.to_json()));
        f.push(("emissive", Json::from(self.emissive)));
        if self.rotation_deg != [0.0, 0.0] {
            f.push(("rotation_deg", range(self.rotation_deg)));
        }
        if self.spin_deg != [0.0, 0.0] {
            f.push(("spin_deg", range(self.spin_deg)));
        }
        f.push(("texture", Json::from(self.texture.name())));
        if let Some(fb) = &self.flipbook {
            let mut g = vec![("cols", Json::from(fb.cols)), ("rows", Json::from(fb.rows)), ("frames", Json::from(fb.frames))];
            if fb.over_life {
                g.push(("over_life", Json::from(true)));
            } else {
                g.push(("fps", Json::from(fb.fps)));
            }
            if fb.random_start {
                g.push(("random_start", Json::from(true)));
            }
            f.push(("flipbook", Json::obj(g)));
        }
        f.push((
            "blend",
            if self.additive == 1.0 {
                Json::from("additive")
            } else if self.additive == 0.0 {
                Json::from("alpha")
            } else {
                Json::from(self.additive)
            },
        ));
        if self.soft > 0.0 {
            f.push(("soft", Json::from(self.soft)));
        }
        if self.lit {
            f.push(("lit", Json::from(true)));
        }
        if self.stretch > 0.0 {
            f.push(("stretch", Json::from(self.stretch)));
        }
        if self.space == Space::World {
            f.push(("space", Json::from("world")));
        }
        if self.inherit_velocity > 0.0 {
            f.push(("inherit_velocity", Json::from(self.inherit_velocity)));
        }
        if let Some(c) = &self.collide {
            let mut g = match c.surface {
                Surface::Plane(h) => vec![("plane", Json::from(h))],
                Surface::Ground => vec![("ground", Json::from(true))],
            };
            g.push(("mode", Json::from(match c.mode {
                CollideMode::Kill => "kill",
                CollideMode::Slide => "slide",
                CollideMode::Stick => "stick",
            })));
            if c.radius > 0.0 {
                g.push(("radius", Json::from(c.radius)));
            }
            f.push(("collide", Json::obj(g)));
        }
        if self.lod != Lod::default() {
            f.push(("lod", Json::obj([("start", Json::from(self.lod.start)), ("end", Json::from(self.lod.end)), ("min", Json::from(self.lod.min))])));
        }
        if self.priority != 1.0 {
            f.push(("priority", Json::from(self.priority)));
        }
        if self.cull_px != 1.5 {
            f.push(("cull_px", Json::from(self.cull_px)));
        }
        Json::obj(f)
    }

    /// See [`Effect::reach`].
    pub fn reach(&self) -> f32 {
        let l = self.lifetime[1];
        let g = (self.gravity[0] * self.gravity[0] + self.gravity[1] * self.gravity[1] + self.gravity[2] * self.gravity[2]).sqrt();
        // |v(a)| <= |v0| + |g| a, so |p(a) - p0| <= |v0| a + |g| a² / 2 (drag only slows); the
        // curl field's magnitude stays below 2 (tested).
        let travel = self.speed[1] * l + 0.5 * g * l * l + self.noise.map_or(0.0, |n| 2.0 * n.strength * l);
        // A rotated quad reaches its half diagonal; the budget may grow sizes up to 1.5x.
        let half = 0.5 * std::f32::consts::SQRT_2 * crate::budget::MAX_SIZE_BOOST * self.size.max() * self.size_random[1];
        let streak = self.stretch * (self.speed[1] + g * l);
        self.shape.extent() + travel + half + streak
    }

    /// The mean over a particle's life of size² × alpha (m², at scale 1): what one particle
    /// covers on average (a spiking size curve counts by its square, not its mean's square).
    pub fn mean_area(&self) -> f32 {
        const N: usize = 64;
        let r = self.size_random;
        let r2 = (r[0] * r[0] + r[0] * r[1] + r[1] * r[1]) / 3.0;
        let sum: f32 = (0..N)
            .map(|i| {
                let u = (i as f32 + 0.5) / N as f32;
                let s = self.size.eval(u);
                s * s * self.alpha.eval(u)
            })
            .sum();
        sum / N as f32 * r2
    }

    /// The most size² × alpha one particle reaches (m², at scale 1, largest random size): what
    /// a burst covers when all its particles peak together.
    pub fn max_area(&self) -> f32 {
        const N: usize = 64;
        let r = self.size_random[1];
        (0..=N)
            .map(|i| {
                let u = i as f32 / N as f32;
                let s = self.size.eval(u);
                s * s * self.alpha.eval(u)
            })
            .fold(0.0f32, f32::max)
            * r
            * r
    }

    /// The most area (m² × alpha, at scale 1) this emitter's particles cover at once: a steady
    /// stream at its mean, bursts at their peak (`rate` multiplies rates and counts).
    pub fn fill_area(&self, rate: f32) -> f32 {
        let alive = match self.duration {
            Some(d) => self.rate * d.min(self.lifetime[1]),
            None => self.rate * self.lifetime[1],
        };
        let bursts: f32 = self
            .bursts
            .iter()
            .map(|b| {
                let overlap = if b.cycles == 1 { 1.0 } else { (self.lifetime[1] / b.interval.max(MIN_INTERVAL)).min(if b.cycles == 0 { f32::MAX } else { b.cycles as f32 }) + 1.0 };
                b.count as f32 * overlap.max(1.0)
            })
            .sum();
        rate * (alive * self.mean_area() + bursts * self.max_area())
    }

    /// See [`Effect::peak`].
    pub fn peak(&self) -> f32 {
        let bursts: f32 = self.bursts.iter().map(|b| {
            // Overlapping cycles: as many as fit in one lifetime.
            let overlap = if b.cycles == 1 { 1.0 } else { (self.lifetime[1] / b.interval.max(MIN_INTERVAL)).min(if b.cycles == 0 { f32::MAX } else { b.cycles as f32 }) + 1.0 };
            b.count as f32 * overlap.max(1.0)
        }).sum();
        let rated = match self.duration {
            Some(d) => self.rate * d.min(self.lifetime[1]),
            None => self.rate * self.lifetime[1],
        };
        rated + bursts
    }

    /// Seconds after the effect starts when this emitter's last particle is gone.
    pub fn finished_after(&self) -> Option<f32> {
        let rated = if self.rate > 0.0 { self.duration.map(|d| self.delay + d)? } else { self.delay };
        let mut last = rated;
        for b in &self.bursts {
            if b.cycles == 0 {
                return None;
            }
            last = last.max(self.delay + b.time + b.interval * (b.cycles - 1) as f32);
        }
        Some(last + self.lifetime[1])
    }

    /// Particles expected alive `age` seconds after the effect started (rate part: ramping up
    /// over one mean lifetime; bursts: those still within their longest lifetime).
    pub fn expected(&self, age: f32) -> f32 {
        let mean_life = 0.5 * (self.lifetime[0] + self.lifetime[1]);
        let a = age - self.delay;
        if a < 0.0 {
            return 0.0;
        }
        let mut n = 0.0;
        if self.rate > 0.0 {
            let end = self.duration.unwrap_or(f32::MAX);
            // Births in the window (a - mean_life, a] ∩ [0, end).
            let from = (a - mean_life).max(0.0);
            let to = a.min(end);
            n += self.rate * (to - from).max(0.0);
        }
        for b in &self.bursts {
            let t0 = b.time;
            if a < t0 {
                continue;
            }
            let fired = if b.interval > 0.0 { dmath::floor((a - t0) / b.interval) + 1.0 } else { 1.0 };
            let fired = if b.cycles == 0 { fired } else { fired.min(b.cycles as f32) };
            // Cycles fired within the last lifetime.
            let recent = if b.interval > 0.0 { (self.lifetime[1] / b.interval).min(fired) } else { fired };
            let last = t0 + (fired - 1.0) * b.interval;
            if a - last < self.lifetime[1] {
                n += b.count as f32 * recent.max(1.0);
            }
        }
        n
    }
}

fn parse_shape(v: &Json, at: &Path) -> Result<Shape> {
    let kind = match v {
        Json::Str(s) => s.as_str(),
        Json::Obj(_) => v.get("kind").and_then(Json::as_str).ok_or_else(|| VfxError::new("missing_field", at.key("kind").as_str(), format!("'{at}' needs a 'kind': {SHAPES}")).allowed(SHAPES))?,
        _ => return Err(VfxError::new("bad_type", at.as_str(), format!("'{at}' must be a shape name or {{kind, ...}}")).allowed(SHAPES).value(v)),
    };
    let obj = if v.as_object().is_some() { v.clone() } else { Json::obj::<&str>([]) };
    let keys: &[&str] = match kind {
        "point" => &["kind"],
        "sphere" => &["kind", "radius", "surface"],
        "box" => &["kind", "size"],
        "disc" => &["kind", "radius"],
        "cone" => &["kind", "radius", "angle_deg"],
        "line" => &["kind", "length"],
        other => return Err(VfxError::new("invalid_value", at.key("kind").as_str(), format!("no spawn shape '{other}'")).allowed(SHAPES)),
    };
    read::keys(&obj, at, keys)?;
    Ok(match kind {
        "point" => Shape::Point,
        "sphere" => Shape::Sphere { radius: read::num_or(&obj, "radius", at, 0.0, MAX_EXTENT, 0.5)?, surface: read::boolean_or(&obj, "surface", at, false)? },
        "box" => {
            let s = read::vec3_or(&obj, "size", at, MAX_EXTENT, [1.0; 3])?;
            if s.iter().any(|x| *x < 0.0) {
                return Err(VfxError::new("invalid_value", at.key("size").as_str(), "box sizes are 0 or more"));
            }
            Shape::Box { size: s }
        }
        "disc" => Shape::Disc { radius: read::num_or(&obj, "radius", at, 0.0, MAX_EXTENT, 0.5)? },
        "cone" => Shape::Cone { radius: read::num_or(&obj, "radius", at, 0.0, MAX_EXTENT, 0.0)?, angle_deg: read::num_or(&obj, "angle_deg", at, 0.0, 90.0, 20.0)? },
        _ => Shape::Line { length: read::num_or(&obj, "length", at, 0.0, MAX_EXTENT, 1.0)? },
    })
}

fn shape_json(s: &Shape) -> Json {
    match s {
        Shape::Point => Json::from("point"),
        Shape::Sphere { radius, surface } => {
            let mut f = vec![("kind", Json::from("sphere")), ("radius", Json::from(*radius))];
            if *surface {
                f.push(("surface", Json::from(true)));
            }
            Json::obj(f)
        }
        Shape::Box { size } => Json::obj([("kind", Json::from("box")), ("size", Json::arr(size.map(Json::from)))]),
        Shape::Disc { radius } => Json::obj([("kind", Json::from("disc")), ("radius", Json::from(*radius))]),
        Shape::Cone { radius, angle_deg } => Json::obj([("kind", Json::from("cone")), ("radius", Json::from(*radius)), ("angle_deg", Json::from(*angle_deg))]),
        Shape::Line { length } => Json::obj([("kind", Json::from("line")), ("length", Json::from(*length))]),
    }
}

fn parse_flipbook(v: &Json, at: &Path) -> Result<Flipbook> {
    read::keys(v, at, &["cols", "rows", "frames", "fps", "over_life", "random_start"])?;
    let cols = read::int_or(v, "cols", at, 1, MAX_GRID, 1)? as u32;
    let rows = read::int_or(v, "rows", at, 1, MAX_GRID, 1)? as u32;
    let frames = read::int_or(v, "frames", at, 1, u64::from(cols * rows), u64::from(cols * rows))? as u32;
    Ok(Flipbook {
        cols,
        rows,
        frames,
        fps: read::num_or(v, "fps", at, 0.0, 120.0, 12.0)?,
        over_life: read::boolean_or(v, "over_life", at, false)?,
        random_start: read::boolean_or(v, "random_start", at, false)?,
    })
}

fn parse_collide(v: &Json, at: &Path) -> Result<Collide> {
    read::keys(v, at, &["plane", "ground", "mode", "radius"])?;
    let ground = read::boolean_or(v, "ground", at, false)?;
    let surface = match (v.get("plane").filter(|p| !p.is_null()), ground) {
        (Some(p), false) => Surface::Plane(read::num(p, &at.key("plane"), -MAX_EXTENT, MAX_EXTENT)?),
        (None, true) => Surface::Ground,
        _ => return Err(VfxError::new("invalid_value", at.as_str(), format!("'{at}' takes exactly one of 'plane' (metres above the emitter) or 'ground': true (the world's ground)"))),
    };
    let mode = match v.get("mode") {
        None | Some(Json::Null) => CollideMode::Kill,
        Some(Json::Str(s)) if s == "kill" => CollideMode::Kill,
        Some(Json::Str(s)) if s == "slide" => CollideMode::Slide,
        Some(Json::Str(s)) if s == "stick" => CollideMode::Stick,
        Some(m) => return Err(VfxError::new("invalid_value", at.key("mode").as_str(), format!("'{}' must be kill, slide or stick", at.key("mode"))).allowed("kill | slide | stick").value(m)),
    };
    Ok(Collide { surface, mode, radius: read::num_or(v, "radius", at, 0.0, 10.0, 0.0)? })
}

impl Light {
    fn parse(v: &Json, at: &Path) -> Result<Light> {
        read::keys(v, at, &["color", "intensity", "range", "offset", "flicker", "flicker_hz", "duration"])?;
        let light = Light {
            duration: match v.get("duration") {
                None | Some(Json::Null) => None,
                Some(d) => Some(read::num(d, &at.key("duration"), 0.01, MAX_DURATION)?),
            },
            color: match v.get("color") {
                None | Some(Json::Null) => [1.0, 0.6, 0.3],
                Some(c) => read::color(c, &at.key("color"))?,
            },
            intensity: read::num_or(v, "intensity", at, 0.0, 1000.0, 1.0)?,
            range: read::num_or(v, "range", at, 0.01, 100.0, 4.0)?,
            offset: read::vec3_or(v, "offset", at, MAX_EXTENT, [0.0; 3])?,
            flicker: read::num_or(v, "flicker", at, 0.0, 1.0, 0.0)?,
            flicker_hz: read::num_or(v, "flicker_hz", at, 0.0, 30.0, 2.0)?,
        };
        // WCAG 2.3.1: never more than three flashes a second (variations under 10% don't count).
        if light.flicker >= 0.1 && light.flicker_hz > MAX_FLASH_HZ {
            return Err(VfxError::new(
                "photosensitive",
                at.key("flicker_hz").as_str(),
                format!(
                    "a light varying by {:.0}% {} times a second can trigger photosensitive seizures (WCAG 2.3.1): at most {MAX_FLASH_HZ} flashes a second, or a flicker under 0.1",
                    light.flicker * 100.0,
                    light.flicker_hz
                ),
            )
            .allowed(format!("flicker_hz <= {MAX_FLASH_HZ} (or flicker < 0.1)")));
        }
        Ok(light)
    }

    fn to_json(self) -> Json {
        let mut f = vec![
            ("color", Json::from(crate::curve::hex_of(self.color))),
            ("intensity", Json::from(self.intensity)),
            ("range", Json::from(self.range)),
            ("offset", Json::arr(self.offset.map(Json::from))),
            ("flicker", Json::from(self.flicker)),
            ("flicker_hz", Json::from(self.flicker_hz)),
        ];
        if let Some(d) = self.duration {
            f.push(("duration", Json::from(d)));
        }
        Json::obj(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fire() -> Json {
        Json::parse(
            r##"{"description": "test fire", "emitters": [{"name": "flames", "rate": 40, "lifetime": [0.5, 1.0],
                "shape": {"kind": "cone", "radius": 0.2, "angle_deg": 15}, "speed": [0.5, 1.0], "gravity": [0, 1, 0],
                "drag": 1.5, "noise": {"strength": 0.3}, "size": [[0, 0.2], [1, 0.05]],
                "color": [[0, "#ffd27a"], [1, "#c2410c"]], "texture": "flame", "flipbook": {"cols": 4, "rows": 4, "fps": 16},
                "soft": 0.2}], "light": {"color": "#ff7a1f", "intensity": 2, "flicker": 0.2}}"##,
        )
        .unwrap()
    }

    #[test]
    fn repeating_bursts_need_a_frame_or_more_between_cycles() {
        let e = Effect::parse(&Json::parse(r#"{"emitters": [{"bursts": [{"count": 1, "cycles": 0, "interval": 1e-7}], "lifetime": 10}]}"#).unwrap()).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("invalid_value", "emitters[0].bursts[0].interval"));
        let ok = Effect::parse(&Json::parse(r#"{"emitters": [{"bursts": [{"count": 1, "cycles": 0, "interval": 0.01}], "lifetime": 10}]}"#).unwrap()).unwrap();
        assert!(ok.peak() <= 1002.0, "{}", ok.peak());
    }

    #[test]
    fn parses_round_trips_and_summarises() {
        let e = Effect::parse(&fire()).unwrap();
        let em = &e.emitters[0];
        assert_eq!((em.rate, em.lifetime), (40.0, [0.5, 1.0]));
        assert_eq!(em.texture, TextureRef::Builtin("flame"));
        assert_eq!(em.flipbook.unwrap().frames, 16);
        let back = Effect::parse(&e.to_json()).unwrap();
        assert_eq!(back, e, "canonical JSON reads back the same effect");
        assert_eq!(e.peak(), 40.0);
        assert_eq!(e.finished_after(), None, "a rate emitter without a duration never ends");
        assert!(e.reach() > 1.0 && e.reach() < 3.0, "{}", e.reach());
        // Ramps up over a mean lifetime, then steady at rate × mean life.
        assert!((em.expected(0.375) - 15.0).abs() < 1e-3);
        assert!((em.expected(5.0) - 30.0).abs() < 1e-3);
    }

    #[test]
    fn one_shots_end() {
        let j = Json::parse(r#"{"emitters": [{"bursts": [{"time": 0.1, "count": 30}], "lifetime": [0.4, 0.8]}]}"#).unwrap();
        let e = Effect::parse(&j).unwrap();
        assert_eq!(e.finished_after(), Some(0.1 + 0.8));
        assert_eq!(e.peak(), 30.0);
        assert_eq!(e.emitters[0].expected(0.05), 0.0);
        assert_eq!(e.emitters[0].expected(0.5), 30.0);
        assert_eq!(e.emitters[0].expected(1.0), 0.0);
    }

    #[test]
    fn errors_name_the_field() {
        let bad = |text: &str| Effect::parse(&Json::parse(text).unwrap()).unwrap_err();
        assert_eq!(bad(r#"{"emitters": []}"#).code, "too_many");
        assert_eq!(bad(r#"{"emitters": [{"rate": 5000}]}"#).field, "emitters[0].rate");
        assert_eq!(bad(r#"{"emitters": [{}]}"#).field, "emitters[0].rate");
        let e = bad(r#"{"emitters": [{"rate": 1, "texture": "flam"}]}"#);
        assert_eq!((e.code, e.field.as_str()), ("unknown_texture", "emitters[0].texture"));
        assert!(e.allowed.unwrap().contains("flame"));
        assert_eq!(bad(r#"{"emitters": [{"rate": 1, "shape": {"kind": "torus"}}]}"#).field, "emitters[0].shape.kind");
        assert_eq!(bad(r#"{"emitters": [{"rate": 1, "bursts": [{"count": 3, "cycles": 0}]}]}"#).field, "emitters[0].bursts[0].interval");
        assert_eq!(bad(r#"{"emitters": [{"rate": 1, "collide": {"plane": 0, "ground": true}}]}"#).field, "emitters[0].collide");
        assert_eq!(bad(r#"{"emitters": [{"rate": 1, "lod": {"start": 50, "end": 10}}]}"#).field, "emitters[0].lod.end");
        assert_eq!(bad(r#"{"emitters": [{"rate": 1, "texture": "../up.png"}]}"#).field, "emitters[0].texture");
        assert_eq!(bad(r#"{"emitters": [{"rate": 1, "spred_deg": 4}]}"#).code, "unknown_field");
        assert_eq!(bad(r#"{"emitters": [{"rate": 1}, {"name": "emitter_0", "rate": 1}]}"#).field, "emitters[1].name");
    }

    #[test]
    fn asset_textures_are_relative_png_paths() {
        let p = Path::root("t");
        assert_eq!(TextureRef::parse(&Json::from("fx/spark.png"), &p, false).unwrap(), TextureRef::Asset("fx/spark.png".into()));
        for bad in ["/abs.png", "a/../b.png", "c:x.png", "a\\b.png", "a//b.png"] {
            assert!(TextureRef::parse(&Json::from(bad), &p, false).is_err(), "{bad}");
        }
    }
}
