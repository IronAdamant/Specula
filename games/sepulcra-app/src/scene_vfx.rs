//! Sepulcra's effects (docs/design/VFX.md): the engine's particles and decals on the game's
//! transient presentation, so they are never saved, journaled or hashed and can't change a run.
//!
//! - **Ambient:** fire and embers in every brazier's bowl (fire's own colours; the stone keeps
//!   its steady light, so the effect's flicker light stays off and the game's light budget holds),
//!   rune motes around carved stones from tier II (player cyan).
//! - **Moving:** a world-space trail behind every wraith (grave green).
//! - **One-shots:** a dust puff and a scorch mark where the dead fall, a cyan impact where a
//!   bolt hits, a scorch where fire splashes, a cyan binding rune under a wall as it is built and
//!   cracks where one breaks. Each is a transient entity removed when its effect is over.
//!
//! STYLE.md: fire colour only for fire, cyan for the player's magic, green for the dead,
//! neutrals for dust and soot.

use super::*;
use specula::world::{DecalSpec, Shape, VfxSpec};
use specula_scintilla::Params;

/// One-shot effects alive at once (the oldest goes first past this).
const MAX_ONESHOTS: usize = 64;
/// How high a brazier's bowl is above its cell (where the fire sits).
const BOWL: f32 = 1.72;

/// The game's effects: the entities it made and when the one-shots end.
#[derive(Clone, Default)]
pub struct Effects {
    group: u64,
    /// Ambient effects of each stone, with the tier they were made for.
    stones: BTreeMap<CellId, (u8, Vec<u64>)>,
    /// `(entity, presentation time it ends)`.
    oneshots: Vec<(u64, f32)>,
    next: u64,
}

fn spec(parent: u64, name: String, pos: Vec3) -> SpawnSpec {
    let mut s = SpawnSpec::new(Shape::Group);
    s.name = Some(name);
    s.parent = Some(EntityKey::Id(parent));
    s.position = pos;
    s
}

fn tinted(effect: &str, tint: [f32; 3]) -> VfxSpec {
    VfxSpec { params: Params { tint, ..Params::default() }, ..VfxSpec::new(effect) }
}

impl Effects {
    /// A fresh `/live/vfx` group (transient, like everything under `/live`).
    pub fn new(world: &mut GameWorld<'_>, agent: &str, live: u64) -> Effects {
        let group = ents::group(world, agent, Some(live), "vfx", Vec3::ZERO, Vec3::ZERO, &[], Json::Null);
        Effects { group, ..Effects::default() }
    }

    fn name(&mut self, what: &str) -> String {
        self.next += 1;
        format!("{what}_{}", self.next)
    }

    /// Brings every stone's ambient effects in line with `stones` (new, carved further, gone).
    pub fn sync_stones(&mut self, world: &mut GameWorld<'_>, agent: &str, state: &State, geo: &SlabGeo, stones: &BTreeMap<CellId, StoneView>) {
        let gone: Vec<CellId> = self.stones.keys().copied().filter(|c| !stones.contains_key(c)).collect();
        for c in gone {
            if let Some((_, ids)) = self.stones.remove(&c) {
                ids.into_iter().for_each(|id| ents::delete(world, id));
            }
        }
        let content = state.content();
        for (cell, v) in stones {
            if self.stones.get(cell).is_some_and(|(t, _)| *t == v.tier) {
                continue;
            }
            if let Some((_, ids)) = self.stones.remove(cell) {
                ids.into_iter().for_each(|id| ents::delete(world, id));
            }
            let kind = content.stone(sepulcra::StoneKind(v.kind)).id.as_str();
            let (anchor, _) = geo.anchor(&content.map, *cell);
            let mut made = Vec::new();
            let mut add = |world: &mut GameWorld<'_>, name: String, pos: Vec3, vfx: VfxSpec| {
                let mut s = spec(self.group, name, pos);
                s.vfx = Some(vfx);
                let id = ents::spawn(world, agent, s);
                if id != 0 {
                    made.push(id);
                }
            };
            match kind {
                "brazier" => {
                    // The stone carries the fire's steady light: the effect's own stays off.
                    let fire = VfxSpec { params: Params { light: false, size: 1.0 + 0.15 * f32::from(v.tier), ..Params::default() }, ..VfxSpec::new("fire") };
                    let n = self.next + 1;
                    add(world, format!("fire_{n}"), anchor + Vec3::new(0.0, BOWL, 0.0), fire);
                    add(world, format!("embers_{n}"), anchor + Vec3::new(0.0, BOWL + 0.1, 0.0), VfxSpec::new("embers"));
                    self.next += 1;
                }
                _ if v.tier >= 1 => {
                    let n = self.next + 1;
                    let motes = VfxSpec { params: Params { tint: RUNE_GLOW, rate: 0.5 + 0.25 * f32::from(v.tier), ..Params::default() }, ..VfxSpec::new("motes") };
                    add(world, format!("motes_{n}"), anchor + Vec3::new(0.0, 1.3, 0.0), motes);
                    self.next += 1;
                }
                _ => {}
            }
            self.stones.insert(*cell, (v.tier, made));
        }
    }

    /// A green trail behind a wraith (a child: it follows; world space: the wisps stay behind).
    pub fn trail(&mut self, world: &mut GameWorld<'_>, agent: &str, wraith: u64) {
        let mut s = spec(wraith, "trail".into(), Vec3::new(0.0, 1.1, 0.0));
        s.vfx = Some(tinted("trail", GRAVE_GLOW));
        ents::spawn(world, agent, s);
    }

    /// A one-shot at `pos` ending `life` seconds of presentation time from `now`.
    #[allow(clippy::too_many_arguments)]
    fn oneshot(&mut self, world: &mut GameWorld<'_>, agent: &str, now: f32, life: f32, what: &str, pos: Vec3, vfx: Option<VfxSpec>, decal: Option<DecalSpec>, scale: f32) {
        if self.oneshots.len() >= MAX_ONESHOTS {
            let (id, _) = self.oneshots.remove(0);
            ents::delete(world, id);
        }
        let name = self.name(what);
        let mut s = spec(self.group, name, pos);
        s.vfx = vfx;
        s.decal = decal;
        s.scale = Vec3::new(scale, 1.0, scale);
        let id = ents::spawn(world, agent, s);
        if id != 0 {
            self.oneshots.push((id, now + life));
        }
    }

    /// Where something died: dust and a soot mark that fades.
    pub fn died(&mut self, world: &mut GameWorld<'_>, agent: &str, now: f32, pos: Vec3, big: f32) {
        let ground = Vec3::new(pos.x, pos.y.max(0.0), pos.z);
        let dust = VfxSpec { params: Params { size: big, rate: big.min(2.0), ..Params::default() }, ..VfxSpec::new("dust_puff") };
        self.oneshot(world, agent, now, 1.6, "dust", ground, Some(dust), None, 1.0);
        let scorch = DecalSpec { size: [1.6, 0.5, 1.6], opacity: 0.7, life: Some(8.0), fade_out: 3.0, ..DecalSpec::parse(&Json::from("scorch")).expect("a built-in decal") };
        self.oneshot(world, agent, now, 8.5, "soot", ground, None, Some(scorch), big);
    }

    /// A bolt's impact: a burst of cyan sparks, a ring and a flash of light.
    pub fn hit(&mut self, world: &mut GameWorld<'_>, agent: &str, now: f32, pos: Vec3) {
        let impact = VfxSpec { params: Params { tint: RUNE_GLOW, size: 0.55, rate: 0.6, intensity: 0.8, ..Params::default() }, ..VfxSpec::new("impact") };
        self.oneshot(world, agent, now, 0.7, "impact", pos, Some(impact), None, 1.0);
    }

    /// Fire splashed here: the ground is scorched.
    pub fn splash(&mut self, world: &mut GameWorld<'_>, agent: &str, now: f32, pos: Vec3, radius: f32) {
        let scorch = DecalSpec { size: [2.0, 0.6, 2.0], opacity: 0.6, life: Some(6.0), fade_out: 2.5, ..DecalSpec::parse(&Json::from("scorch")).expect("a built-in decal") };
        self.oneshot(world, agent, now, 6.5, "scorch", Vec3::new(pos.x, pos.y, pos.z), None, Some(scorch), radius.max(0.5));
    }

    /// A wall is raised: a binding rune glows under it and fades.
    pub fn wall_built(&mut self, world: &mut GameWorld<'_>, agent: &str, now: f32, pos: Vec3) {
        let rune = DecalSpec {
            size: [2.2, 0.6, 2.2],
            color: RUNE_GLOW,
            emissive: 1.4,
            life: Some(2.5),
            fade_in: 0.15,
            fade_out: 1.5,
            ..DecalSpec::parse(&Json::from("rune_circle")).expect("a built-in decal")
        };
        self.oneshot(world, agent, now, 3.0, "binding", pos, None, Some(rune), 1.0);
    }

    /// A wall broke: dust, and cracks in the stone where it stood.
    pub fn wall_broken(&mut self, world: &mut GameWorld<'_>, agent: &str, now: f32, pos: Vec3) {
        self.oneshot(world, agent, now, 1.6, "rubble", pos, Some(VfxSpec { params: Params { size: 1.4, rate: 1.5, ..Params::default() }, ..VfxSpec::new("dust_puff") }), None, 1.0);
        let cracks = DecalSpec {
            size: [2.0, 0.6, 2.0],
            opacity: 0.8,
            life: Some(10.0),
            fade_out: 3.0,
            normal: specula_scintilla::TextureRef::parse(&Json::from("cracks_normal"), &specula_scintilla::error::Path::root("normal"), true).ok(),
            ..DecalSpec::parse(&Json::from("cracks")).expect("a built-in decal")
        };
        self.oneshot(world, agent, now, 10.5, "cracks", pos, None, Some(cracks), 1.0);
    }

    /// Removes the one-shots that are over.
    pub fn expire(&mut self, world: &mut GameWorld<'_>, now: f32) {
        self.oneshots.retain(|&(id, until)| {
            if now >= until {
                ents::delete(world, id);
                return false;
            }
            true
        });
    }

    /// Effect entities alive, for play_state and tests.
    pub fn count(&self) -> usize {
        self.stones.values().map(|(_, v)| v.len()).sum::<usize>() + self.oneshots.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::looks::hex;
    use specula_scintilla::check::{check, Limits};
    use specula_scintilla::library::builtin;

    /// STYLE.md: each effect stays in its colour family (fire only for fire, cyan magic, green
    /// dead, neutral dust), as played here (with the game's tints), and passes the budget checks.
    #[test]
    fn effects_keep_their_colour_family_and_budgets() {
        let fire = [FIRE_CORE, FIRE_FLAME, hex(0xc2410c)];
        let cyan = [RUNE_GLOW, RUNE_DEEP, RUNE_PALE];
        let green = [GRAVE_GLOW, GRAVE_DEEP, hex(0xc8e66a)];
        let neutral = [BONE, BONE_SHADOW, SLAB_TOP, hex(0x7d7564)];
        let tint = |t: [f32; 3]| Params { tint: t, ..Params::default() };
        for (effect, params, family) in [
            ("fire", Params::default(), &fire[..]),
            ("embers", Params::default(), &fire[..]),
            ("motes", tint(RUNE_GLOW), &cyan[..]),
            ("impact", tint(RUNE_GLOW), &cyan[..]),
            ("trail", tint(GRAVE_GLOW), &green[..]),
            ("dust_puff", Params::default(), &neutral[..]),
        ] {
            let e = builtin(effect).unwrap();
            let report = check(e, &params, &Limits { palette_distance: 0.3, ..Limits::default() }, Some(family));
            let bad: Vec<_> = report.iter().filter(|c| !c.ok && (c.severity == "error" || c.check == "palette")).collect();
            assert!(bad.is_empty(), "{effect}: {bad:?}");
        }
    }

    /// Every effect as the game plays it (its params, which skip world_spawn's checks) stays
    /// within 3 flashes a second, so the engine's evaluation-time guard never hides one.
    #[test]
    fn the_games_effects_never_flash() {
        let p = Params::default();
        let cases = [
            ("fire", Params { light: false, size: 1.45, ..p }),
            ("embers", p),
            ("motes", Params { tint: RUNE_GLOW, rate: 1.25, ..p }),
            ("trail", Params { tint: GRAVE_GLOW, ..p }),
            ("dust_puff", Params { size: 4.0, rate: 2.0, ..p }),
            ("dust_puff", Params { size: 1.4, rate: 1.5, ..p }),
            ("impact", Params { tint: RUNE_GLOW, size: 0.55, rate: 0.6, intensity: 0.8, ..p }),
        ];
        for (effect, params) in cases {
            let f = specula_scintilla::flash::flash(builtin(effect).unwrap(), &params);
            assert!(f.hz <= 3.0, "{effect} {params:?}: {f:?}");
        }
    }
}
