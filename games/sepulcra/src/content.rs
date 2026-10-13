//! Content as data: the map, stones, enemies and waves are JSON files under `data/`, so agents
//! tune the game without touching code. Everything is validated on load and converted to the
//! sim's fixed-point units ([`crate::fixed`]) once.
//!
//! The default game is embedded with `include_str!`; [`Content::load_dir`] reads a custom set
//! (same four file names) for balance experiments.

use std::path::Path;

use specula_core::hash::StableHasher;
use specula_core::Json;

use crate::error::{DataError, ErrorCode, Obj};
use crate::fixed::{milli, mm, permille, ticks, TICK_HZ};
use crate::map::{CellId, Map};

pub const SLAB_01_JSON: &str = include_str!("../data/slab_01.json");
pub const STONES_JSON: &str = include_str!("../data/stones.json");
pub const ENEMIES_JSON: &str = include_str!("../data/enemies.json");
pub const WAVES_JSON: &str = include_str!("../data/waves.json");

/// Index into [`Content::stones`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StoneKind(pub u8);

/// Index into [`Content::enemies`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnemyKind(pub u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Targets {
    Ground,
    Air,
    Both,
}

impl Targets {
    pub fn hits(self, flies: bool) -> bool {
        match self {
            Targets::Ground => !flies,
            Targets::Air => flies,
            Targets::Both => true,
        }
    }
}

/// Which enemy in range a stone shoots. Ties go to the lowest enemy id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    /// Least distance left to the graveyard (the classic default).
    First,
    /// Most distance left.
    Last,
    /// Most hit points left (ties: first).
    Strongest,
    /// Nearest to the stone.
    Closest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attack {
    /// A homing projectile: it follows its target and always hits unless the target dies first.
    Bolt,
    /// An unguided projectile thrown at where the target stood; it splashes on landing, so fast
    /// enemies can step out of it.
    Lob,
    /// An instant pulse on every eligible enemy in range (no projectile).
    Pulse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tier {
    /// Dust to carve this tier (tier I: the stone; II and III: the upgrade).
    pub cost: i32,
    pub range: i32,
    pub cooldown: u32,
    /// Milli-HP per hit.
    pub damage: i32,
    /// Projectile speed in mm per tick.
    pub projectile_speed: i32,
    pub splash: i32,
    /// Speed reduction in permille.
    pub slow: i32,
    pub slow_ticks: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoneDef {
    pub id: String,
    pub name: String,
    pub glyph: char,
    pub targets: Targets,
    pub policy: Policy,
    pub attack: Attack,
    pub tiers: Vec<Tier>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summon {
    pub enemy: EnemyKind,
    pub every: u32,
    pub count: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Movement {
    /// Walks the top face; can't climb cliffs.
    Walker,
    /// Walks and climbs cliff lanes.
    Climber,
    /// Flies the ring round the rim, then dives at the graveyard; ignores walls.
    Flyer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnemyDef {
    pub id: String,
    pub name: String,
    pub glyph: char,
    /// Milli-HP.
    pub hp: i32,
    /// mm per second.
    pub speed: i32,
    pub movement: Movement,
    /// Speed on cliff lanes, permille of `speed`.
    pub climb_speed: i32,
    /// Can leap cracked ledges (the `jump` object is present). A leaper always treats a ledge
    /// as an open route, whatever its chance.
    pub leaps: bool,
    /// Chance, permille, to take a ledge as a shortcut while a walking route exists.
    pub jump_chance: i32,
    /// A jump is only taken when it lands no more than this much farther from the graveyard.
    pub jump_detour: i32,
    /// Ring hops a flyer circles before diving: uniform in `min..=max`.
    pub circle: (u16, u16),
    /// Grave-dust per kill: uniform in `min..=max` (the `drops` stream).
    pub dust: (i32, i32),
    pub tombstones: i32,
    /// Milli-HP of wall damage per tick.
    pub wall_damage: i32,
    pub summon: Option<Summon>,
}

impl EnemyDef {
    pub fn flies(&self) -> bool {
        self.movement == Movement::Flyer
    }

    pub fn climbs(&self) -> bool {
        self.movement == Movement::Climber
    }
}

/// Where one enemy of a group appears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnPoint {
    /// Index into [`Map::spawns`].
    Ground(u16),
    /// Index into the air ring's entries.
    Air(u16),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub enemy: EnemyKind,
    pub count: u16,
    /// Each enemy picks one of these (the `waves` stream).
    pub from: Vec<SpawnPoint>,
    /// Ticks after the wave starts.
    pub start: u32,
    pub every: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaveDef {
    /// Ticks until the next wave starts on its own.
    pub duration: u32,
    /// Dust granted when the wave starts.
    pub income: i32,
    /// Enemy HP scale, permille.
    pub hp_scale: i32,
    pub groups: Vec<Group>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WallDef {
    pub cost: i32,
    /// Milli-HP.
    pub hp: i32,
    /// Extra route cost per wall when enemies pick which wall to break.
    pub breach_penalty: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nature {
    /// Roots appear when this wave starts.
    pub from_wave: u16,
    /// Stones at this tier index or above grow roots (1 = tier II).
    pub min_tier: u8,
    pub radius: i32,
    /// Permille slow on rooted lane cells.
    pub slow: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    pub start_dust: i32,
    pub tombstones: i32,
    pub build_ticks: u32,
    /// Milli-dust per second left on the wave timer when the next wave is called early.
    pub early_call_milli_per_s: i32,
    /// A stone reaches another face only within this distance of their shared rim.
    pub edge_reach: i32,
    pub refund_percent: i32,
    pub wall: WallDef,
    pub nature: Nature,
}

#[derive(Clone, Debug)]
pub struct Content {
    pub map: Map,
    pub stones: Vec<StoneDef>,
    pub enemies: Vec<EnemyDef>,
    pub waves: Vec<WaveDef>,
    pub rules: Rules,
    /// Stable hash of the parsed content (see [`content_hash`]): part of every state hash, so a
    /// replay can tell when it is run against different content.
    pub hash: u64,
}

impl Content {
    /// The default game (embedded data). Panics only if the shipped data is invalid, which the
    /// test suite rules out.
    pub fn builtin() -> Content {
        Content::from_texts(SLAB_01_JSON, STONES_JSON, ENEMIES_JSON, WAVES_JSON).expect("built-in content is valid")
    }

    pub fn from_texts(map: &str, stones: &str, enemies: &str, waves: &str) -> Result<Content, DataError> {
        let map_v = Map::parse(map, "slab_01.json")?;
        let (stone_defs, mut rules) = parse_stones(stones)?;
        let enemy_defs = parse_enemies(enemies)?;
        check_summons(&map_v, &enemy_defs)?;
        let wave_defs = parse_waves(waves, &map_v, &enemy_defs, &mut rules)?;
        let mut c = Content { map: map_v, stones: stone_defs, enemies: enemy_defs, waves: wave_defs, rules, hash: 0 };
        c.hash = content_hash(&c);
        Ok(c)
    }

    /// Reads `slab_01.json`, `stones.json`, `enemies.json` and `waves.json` from `dir`.
    pub fn load_dir(dir: &Path) -> Result<Content, DataError> {
        let read = |name: &str| {
            std::fs::read_to_string(dir.join(name)).map_err(|e| DataError::new(name, "", ErrorCode::Io, format!("cannot read {}: {e}", dir.join(name).display())))
        };
        Content::from_texts(&read("slab_01.json")?, &read("stones.json")?, &read("enemies.json")?, &read("waves.json")?)
    }

    pub fn stone_kind(&self, id: &str) -> Option<StoneKind> {
        self.stones.iter().position(|s| s.id == id).map(|i| StoneKind(i as u8))
    }

    pub fn enemy_kind(&self, id: &str) -> Option<EnemyKind> {
        self.enemies.iter().position(|e| e.id == id).map(|i| EnemyKind(i as u8))
    }

    pub fn stone(&self, k: StoneKind) -> &StoneDef {
        &self.stones[usize::from(k.0)]
    }

    pub fn enemy(&self, k: EnemyKind) -> &EnemyDef {
        &self.enemies[usize::from(k.0)]
    }

    /// Ticks from the start of the run to the last wave's start, if nobody calls a wave early.
    pub fn scheduled_ticks(&self) -> u32 {
        self.rules.build_ticks + self.waves[..self.waves.len() - 1].iter().map(|w| w.duration).sum::<u32>()
    }
}

fn parse_root<'a>(json: &'a Json, file: &'a str) -> Result<Obj<'a>, DataError> {
    Obj::new(json, file, "")
}

fn parse_json(text: &str, file: &str) -> Result<Json, DataError> {
    Json::parse(text).map_err(|e| DataError::new(file, format!("byte {}", e.offset), ErrorCode::Syntax, e.message))
}

fn glyph(o: &Obj, key: &str) -> Result<char, DataError> {
    let s = o.str(key)?;
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(c), None) if c.is_ascii_graphic() && !"#.jGSx-|B=".contains(c) => Ok(c),
        _ => Err(o.err(key, ErrorCode::Invalid, "expected one printable ASCII character not used by the map legend (# . j G S x - | B =)")),
    }
}

fn parse_stones(text: &str) -> Result<(Vec<StoneDef>, Rules), DataError> {
    const F: &str = "stones.json";
    let json = parse_json(text, F)?;
    let root = parse_root(&json, F)?;
    root.only(&["notes", "edge_reach_m", "sell_refund", "wall", "nature", "stones"])?;
    let wall = root.obj("wall")?;
    wall.only(&["cost", "hp", "breach_penalty_m"])?;
    let nature = root.obj("nature")?;
    nature.only(&["from_wave", "min_tier", "radius_m", "slow"])?;
    let rules = Rules {
        start_dust: 0,
        tombstones: 0,
        build_ticks: 0,
        early_call_milli_per_s: 0,
        edge_reach: mm(root.num("edge_reach_m", 0.0, 100.0)?),
        refund_percent: (root.num("sell_refund", 0.0, 1.0)? * 100.0).round() as i32,
        wall: WallDef {
            cost: wall.int("cost", 0, 100_000)? as i32,
            hp: milli(wall.num("hp", 1.0, 1_000_000.0)?),
            breach_penalty: mm(wall.num_or("breach_penalty_m", 0.0, 0.0, 1000.0)?),
        },
        nature: Nature {
            from_wave: nature.int("from_wave", 1, 1000)? as u16,
            min_tier: (nature.int("min_tier", 1, 10)? - 1) as u8,
            radius: mm(nature.num("radius_m", 0.0, 100.0)?),
            slow: permille(nature.num("slow", 0.0, 0.9)?),
        },
    };
    let mut stones: Vec<StoneDef> = Vec::new();
    for (i, s) in root.arr("stones")?.iter().enumerate() {
        let o = Obj::new(s, F, format!("stones[{i}]"))?;
        o.only(&["id", "name", "glyph", "targets", "policy", "attack", "tiers", "notes"])?;
        let id = o.str("id")?.to_owned();
        if stones.iter().any(|s| s.id == id) {
            return Err(o.err("id", ErrorCode::Duplicate, format!("duplicate stone id {id:?}")));
        }
        let g = glyph(&o, "glyph")?;
        if stones.iter().any(|s| s.glyph == g) {
            return Err(o.err("glyph", ErrorCode::Duplicate, format!("glyph '{g}' is already used")));
        }
        let targets = match o.str("targets")? {
            "ground" => Targets::Ground,
            "air" => Targets::Air,
            "both" => Targets::Both,
            _ => return Err(o.err("targets", ErrorCode::BadChoice, "expected \"ground\", \"air\" or \"both\"")),
        };
        let policy = match o.str("policy")? {
            "first" => Policy::First,
            "last" => Policy::Last,
            "strongest" => Policy::Strongest,
            "closest" => Policy::Closest,
            _ => return Err(o.err("policy", ErrorCode::BadChoice, "expected \"first\", \"last\", \"strongest\" or \"closest\"")),
        };
        let attack = match o.str("attack")? {
            "bolt" => Attack::Bolt,
            "lob" => Attack::Lob,
            "pulse" => Attack::Pulse,
            _ => return Err(o.err("attack", ErrorCode::BadChoice, "expected \"bolt\", \"lob\" or \"pulse\"")),
        };
        let mut tiers = Vec::new();
        let tiers_json = o.arr("tiers")?;
        if !(1..=5).contains(&tiers_json.len()) {
            return Err(o.err("tiers", ErrorCode::OutOfRange, "needs 1..=5 tiers"));
        }
        for (t, tj) in tiers_json.iter().enumerate() {
            let to = Obj::new(tj, F, o.path(&format!("tiers[{t}]")))?;
            to.only(&["cost", "range_m", "cooldown_s", "damage", "projectile_mps", "splash_m", "slow", "slow_s"])?;
            let tier = Tier {
                cost: to.int("cost", 0, 100_000)? as i32,
                range: mm(to.num("range_m", 0.5, 100.0)?),
                cooldown: ticks(to.num("cooldown_s", 1.0 / f64::from(TICK_HZ), 600.0)?),
                damage: milli(to.num_or("damage", 0.0, 0.0, 100_000.0)?),
                projectile_speed: mm(to.num_or("projectile_mps", 0.0, 0.0, 1000.0)?) / TICK_HZ as i32,
                splash: mm(to.num_or("splash_m", 0.0, 0.0, 100.0)?),
                slow: permille(to.num_or("slow", 0.0, 0.0, 0.9)?),
                slow_ticks: ticks(to.num_or("slow_s", 0.0, 0.0, 600.0)?),
            };
            if attack != Attack::Pulse && tier.projectile_speed <= 0 {
                return Err(to.err("projectile_mps", ErrorCode::Invalid, "bolts and lobs need a projectile speed of at least 0.03 m/s"));
            }
            if attack == Attack::Lob && tier.splash <= 0 {
                return Err(to.err("splash_m", ErrorCode::Invalid, "a lob needs a splash radius"));
            }
            if tier.damage == 0 && tier.slow == 0 {
                return Err(to.err("damage", ErrorCode::Invalid, "a tier must deal damage or slow"));
            }
            tiers.push(tier);
        }
        stones.push(StoneDef { id, name: o.str("name")?.to_owned(), glyph: g, targets, policy, attack, tiers });
    }
    if stones.is_empty() || stones.len() > 64 {
        return Err(root.err("stones", ErrorCode::OutOfRange, "needs 1..=64 stones"));
    }
    Ok((stones, rules))
}

fn parse_enemies(text: &str) -> Result<Vec<EnemyDef>, DataError> {
    const F: &str = "enemies.json";
    let json = parse_json(text, F)?;
    let root = parse_root(&json, F)?;
    root.only(&["notes", "enemies"])?;
    let list = root.arr("enemies")?;
    if list.is_empty() || list.len() > 64 {
        return Err(root.err("enemies", ErrorCode::OutOfRange, "needs 1..=64 enemies"));
    }
    let ids: Vec<&str> = list.iter().map(|e| e.get("id").and_then(Json::as_str).unwrap_or("")).collect();
    let mut out: Vec<EnemyDef> = Vec::new();
    for (i, e) in list.iter().enumerate() {
        let o = Obj::new(e, F, format!("enemies[{i}]"))?;
        o.only(&[
            "id", "name", "glyph", "hp", "speed_mps", "movement", "climb_speed", "jump", "circle_points", "dust", "tombstones", "wall_dps",
            "summon", "notes",
        ])?;
        let id = o.str("id")?.to_owned();
        if out.iter().any(|d| d.id == id) {
            return Err(o.err("id", ErrorCode::Duplicate, format!("duplicate enemy id {id:?}")));
        }
        let movement = match o.str("movement")? {
            "walker" => Movement::Walker,
            "climber" => Movement::Climber,
            "flyer" => Movement::Flyer,
            _ => return Err(o.err("movement", ErrorCode::BadChoice, "expected \"walker\", \"climber\" or \"flyer\"")),
        };
        let (leaps, jump_chance, jump_detour) = match o.get("jump") {
            None => (false, 0, 0),
            Some(j) => {
                let jo = Obj::new(j, F, o.path("jump"))?;
                jo.only(&["chance", "max_detour_m"])?;
                if movement == Movement::Flyer {
                    return Err(o.err("jump", ErrorCode::Invalid, "flyers don't lane-jump"));
                }
                let chance = jo.num("chance", 0.0, 1.0)?;
                if chance > 0.0 && permille(chance) == 0 {
                    return Err(jo.err("chance", ErrorCode::OutOfRange, format!("{chance} rounds to 0 per mille: use 0 or at least 0.001")));
                }
                (true, permille(chance), mm(jo.num_or("max_detour_m", 0.0, 0.0, 1000.0)?))
            }
        };
        let pair = |key: &str, min: i64, max: i64| -> Result<(i64, i64), DataError> {
            let a = o.arr(key)?;
            let want = format!("expected [min, max] integers with {min} <= min <= max <= {max}");
            match (a.first().and_then(Json::as_i64), a.get(1).and_then(Json::as_i64), a.len()) {
                (Some(lo), Some(hi), 2) if min <= lo && lo <= hi && hi <= max => Ok((lo, hi)),
                (Some(_), Some(_), 2) => Err(o.err(key, ErrorCode::OutOfRange, want)),
                _ => Err(o.err(key, ErrorCode::BadType, want)),
            }
        };
        let circle = if movement == Movement::Flyer {
            let (lo, hi) = pair("circle_points", 0, 1000)?;
            (lo as u16, hi as u16)
        } else {
            (0, 0)
        };
        let (dlo, dhi) = pair("dust", 0, 10_000)?;
        // Ground enemies must be able to break a wall: at least one milli-HP per tick, or a full
        // block would stall the game. Flyers never touch walls.
        let wall_damage = if movement == Movement::Flyer {
            if o.get("wall_dps").is_some() {
                return Err(o.err("wall_dps", ErrorCode::Invalid, "flyers ignore walls; remove wall_dps"));
            }
            0
        } else {
            milli(o.num("wall_dps", 0.1, 30_000.0)?) / TICK_HZ as i32
        };
        let summon = match o.get("summon") {
            None => None,
            Some(s) => {
                let so = Obj::new(s, F, o.path("summon"))?;
                so.only(&["enemy", "every_s", "count"])?;
                let target = so.str("enemy")?;
                let k = ids.iter().position(|&x| x == target).ok_or_else(|| so.err("enemy", ErrorCode::UnknownRef, format!("no enemy {target:?}")))?;
                if k == i {
                    return Err(so.err("enemy", ErrorCode::Invalid, "an enemy can't summon itself"));
                }
                Some(Summon { enemy: EnemyKind(k as u8), every: ticks(so.num("every_s", 0.1, 600.0)?), count: so.int("count", 1, 100)? as u16 })
            }
        };
        out.push(EnemyDef {
            id,
            name: o.str("name")?.to_owned(),
            glyph: glyph(&o, "glyph")?,
            hp: milli(o.num("hp", 0.001, 100_000.0)?),
            speed: mm(o.num("speed_mps", 0.01, 100.0)?),
            movement,
            climb_speed: permille(o.num_or("climb_speed", 1.0, 0.05, 2.0)?),
            leaps,
            jump_chance,
            jump_detour,
            circle,
            dust: (dlo as i32, dhi as i32),
            tombstones: o.int("tombstones", 0, 1000)? as i32,
            wall_damage,
            summon,
        });
    }
    // Summons appear on the summoner's cell, so they must be able to stand and route there: a
    // flyer would need an air entry, and a walker summoned on a cliff could never move.
    for (i, e) in out.iter().enumerate() {
        if let Some(s) = e.summon {
            let child = &out[usize::from(s.enemy.0)];
            let reason = if child.flies() || e.flies() {
                Some("summoner and summoned must both walk or climb")
            } else if e.climbs() && !child.climbs() {
                Some("a climber can only summon climbers (a walker summoned on a cliff can't move)")
            } else {
                None
            };
            if let Some(reason) = reason {
                return Err(DataError::new(F, format!("enemies[{i}].summon.enemy"), ErrorCode::Invalid, reason));
            }
        }
    }
    Ok(out)
}

fn parse_waves(text: &str, map: &Map, enemies: &[EnemyDef], rules: &mut Rules) -> Result<Vec<WaveDef>, DataError> {
    const F: &str = "waves.json";
    let json = parse_json(text, F)?;
    let root = parse_root(&json, F)?;
    root.only(&["notes", "start_dust", "tombstones", "build_time_s", "early_call_dust_per_s", "waves"])?;
    rules.start_dust = root.int("start_dust", 0, 1_000_000)? as i32;
    rules.tombstones = root.int("tombstones", 1, 1000)? as i32;
    rules.build_ticks = ticks(root.num("build_time_s", 0.0, 3600.0)?);
    rules.early_call_milli_per_s = milli(root.num_or("early_call_dust_per_s", 0.0, 0.0, 1000.0)?);
    let list = root.arr("waves")?;
    if list.is_empty() || list.len() > 1000 {
        return Err(root.err("waves", ErrorCode::OutOfRange, "needs 1..=1000 waves"));
    }
    let mut waves = Vec::new();
    for (wi, w) in list.iter().enumerate() {
        let o = Obj::new(w, F, format!("waves[{wi}]"))?;
        o.only(&["duration_s", "income", "hp", "groups", "notes"])?;
        let mut groups = Vec::new();
        for (gi, g) in o.arr("groups")?.iter().enumerate() {
            let go = Obj::new(g, F, o.path(&format!("groups[{gi}]")))?;
            go.only(&["enemy", "count", "from", "start_s", "every_s"])?;
            let eid = go.str("enemy")?;
            let k = enemies.iter().position(|e| e.id == eid).ok_or_else(|| go.err("enemy", ErrorCode::UnknownRef, format!("no enemy {eid:?}")))?;
            let def = &enemies[k];
            let from = spawn_points(&go, map, def)?;
            groups.push(Group {
                enemy: EnemyKind(k as u8),
                count: go.int("count", 1, 10_000)? as u16,
                from,
                start: ticks(go.num_or("start_s", 0.0, 0.0, 3600.0)?),
                every: ticks(go.num_or("every_s", 1.0, 0.0, 3600.0)?),
            });
        }
        waves.push(WaveDef {
            duration: ticks(o.num("duration_s", 1.0, 3600.0)?),
            income: o.int_or("income", 0, 0, 100_000)? as i32,
            hp_scale: permille(o.num_or("hp", 1.0, 0.01, 20.0)?),
            groups,
        });
    }
    Ok(waves)
}

/// Every cell a summoner can stand on must let its summons reach the graveyard (by walking,
/// climbing or leaping, as they can; walls ignored, since breaking them is always an option).
/// Otherwise a summon could be born somewhere it can never leave: a leaping summoner on a
/// ledge-only island summoning walkers, say. Ordinary spawns are covered by the map's own check.
fn check_summons(map: &Map, enemies: &[EnemyDef]) -> Result<(), DataError> {
    for (i, e) in enemies.iter().enumerate() {
        let Some(s) = e.summon else { continue };
        let child = &enemies[usize::from(s.enemy.0)];
        let starts: Vec<CellId> = map.spawns.iter().filter(|sp| e.climbs() || !sp.climbers_only).map(|sp| sp.cell).collect();
        let stands = map.component(&starts, e.climbs(), e.leaps);
        let ok = map.component(&map.graveyard, child.climbs(), child.leaps);
        if let Some(c) = map.cells().iter().find(|c| stands[c.id.index()] && !ok[c.id.index()]) {
            return Err(DataError::new(
                "enemies.json",
                format!("enemies[{i}].summon.enemy"),
                ErrorCode::Unreachable,
                format!("{} can stand on {}, but a {} summoned there can't reach the graveyard", e.id, map.cell_name(c.id), child.id),
            ));
        }
    }
    Ok(())
}

/// The content hash: everything that changes the simulation or what a recording's commands
/// mean, in a fixed order. Display names, glyphs, `notes`, whitespace and key order are left
/// out, so editing them doesn't invalidate recordings or pinned hashes.
pub fn content_hash(c: &Content) -> u64 {
    let mut h = StableHasher::with_seed(0x5e9_c1a);
    let i = |h: &mut StableHasher, v: i64| h.write_i64(v);
    let pos = |h: &mut StableHasher, p: crate::fixed::Pos| {
        for v in [p.x, p.y, p.z] {
            h.write_i64(i64::from(v));
        }
    };
    let m = &c.map;
    for v in [m.cell_mm, m.cliff_row_mm, m.half_x, m.half_z] {
        i(&mut h, i64::from(v));
    }
    h.write_u64(m.faces().len() as u64);
    for g in m.faces() {
        for v in [g.face as u8, g.cols, g.rows] {
            h.write_u8(v);
        }
    }
    h.write_u64(m.cells().len() as u64);
    for cell in m.cells() {
        h.write_u8(cell.kind as u8);
        pos(&mut h, cell.pos);
    }
    h.write_u64(m.spawns.len() as u64);
    for s in &m.spawns {
        h.write_u32(u32::from(s.cell.0));
        h.write_bool(s.climbers_only);
    }
    i(&mut h, i64::from(m.air.altitude));
    i(&mut h, i64::from(m.air.ring_half));
    pos(&mut h, m.air.dive_target);
    h.write_u64(m.air.ring.len() as u64);
    for &p in &m.air.ring {
        pos(&mut h, p);
    }
    h.write_u64(m.air.entries.len() as u64);
    for &e in &m.air.entries {
        h.write_u32(u32::from(e));
    }
    h.write_u64(c.stones.len() as u64);
    for s in &c.stones {
        h.write_str(&s.id);
        for v in [s.targets as u8, s.policy as u8, s.attack as u8, s.tiers.len() as u8] {
            h.write_u8(v);
        }
        for t in &s.tiers {
            for v in [t.cost, t.range, t.damage, t.projectile_speed, t.splash, t.slow] {
                i(&mut h, i64::from(v));
            }
            h.write_u32(t.cooldown);
            h.write_u32(t.slow_ticks);
        }
    }
    h.write_u64(c.enemies.len() as u64);
    for e in &c.enemies {
        h.write_str(&e.id);
        h.write_u8(e.movement as u8);
        h.write_bool(e.leaps);
        for v in [e.hp, e.speed, e.climb_speed, e.jump_chance, e.jump_detour, e.dust.0, e.dust.1, e.tombstones, e.wall_damage] {
            i(&mut h, i64::from(v));
        }
        h.write_u32(u32::from(e.circle.0));
        h.write_u32(u32::from(e.circle.1));
        h.write_bool(e.summon.is_some());
        if let Some(s) = e.summon {
            h.write_u8(s.enemy.0);
            h.write_u32(s.every);
            h.write_u32(u32::from(s.count));
        }
    }
    h.write_u64(c.waves.len() as u64);
    for w in &c.waves {
        h.write_u32(w.duration);
        i(&mut h, i64::from(w.income));
        i(&mut h, i64::from(w.hp_scale));
        h.write_u64(w.groups.len() as u64);
        for g in &w.groups {
            h.write_u8(g.enemy.0);
            h.write_u32(u32::from(g.count));
            h.write_u32(g.start);
            h.write_u32(g.every);
            h.write_u64(g.from.len() as u64);
            for p in &g.from {
                match *p {
                    SpawnPoint::Ground(k) => {
                        h.write_u8(0);
                        h.write_u32(u32::from(k));
                    }
                    SpawnPoint::Air(k) => {
                        h.write_u8(1);
                        h.write_u32(u32::from(k));
                    }
                }
            }
        }
    }
    let r = &c.rules;
    for v in [
        r.start_dust,
        r.tombstones,
        r.early_call_milli_per_s,
        r.edge_reach,
        r.refund_percent,
        r.wall.cost,
        r.wall.hp,
        r.wall.breach_penalty,
        r.nature.radius,
        r.nature.slow,
    ] {
        i(&mut h, i64::from(v));
    }
    h.write_u32(r.build_ticks);
    h.write_u32(u32::from(r.nature.from_wave));
    h.write_u8(r.nature.min_tier);
    h.finish()
}

/// `"from"`: `"gorges"`, `"cliffs"`, `"ground"` (both), `"air"`, a spawn name, or a list of names.
fn spawn_points(go: &Obj, map: &Map, def: &EnemyDef) -> Result<Vec<SpawnPoint>, DataError> {
    let names: Vec<&str> = match go.req("from")? {
        Json::Str(s) => vec![s.as_str()],
        Json::Arr(a) => a.iter().map(|v| v.as_str().ok_or_else(|| go.err("from", ErrorCode::BadType, "expected spawn names"))).collect::<Result<_, _>>()?,
        _ => return Err(go.err("from", ErrorCode::BadType, "expected a spawn name, a list of names, \"gorges\", \"cliffs\", \"ground\" or \"air\"")),
    };
    if def.flies() {
        if names != ["air"] {
            return Err(go.err("from", ErrorCode::Invalid, format!("{} flies: use \"air\"", def.id)));
        }
        return Ok((0..map.air.entries.len() as u16).map(SpawnPoint::Air).collect());
    }
    let mut out = Vec::new();
    for n in names {
        let pick = |f: &dyn Fn(bool) -> bool| map.spawns.iter().enumerate().filter(|(_, s)| f(s.climbers_only)).map(|(i, _)| SpawnPoint::Ground(i as u16)).collect::<Vec<_>>();
        match n {
            "gorges" => out.extend(pick(&|c| !c)),
            "cliffs" => out.extend(pick(&|c| c)),
            "ground" => out.extend(pick(&|_| true)),
            "air" => return Err(go.err("from", ErrorCode::Invalid, format!("{} doesn't fly", def.id))),
            name => {
                let i = map.spawns.iter().position(|s| s.name == name).ok_or_else(|| go.err("from", ErrorCode::UnknownRef, format!("no spawn named {name:?}")))?;
                out.push(SpawnPoint::Ground(i as u16));
            }
        }
    }
    if !def.climbs() {
        if let Some(SpawnPoint::Ground(i)) = out.iter().find(|p| matches!(p, SpawnPoint::Ground(i) if map.spawns[usize::from(*i)].climbers_only)) {
            return Err(go.err("from", ErrorCode::Invalid, format!("{} can't climb, but {} is a cliff base", def.id, map.spawns[usize::from(*i)].name)));
        }
    }
    if out.is_empty() {
        return Err(go.err("from", ErrorCode::Invalid, "no spawn points"));
    }
    out.sort_by_key(|p| match p {
        SpawnPoint::Ground(i) | SpawnPoint::Air(i) => *i,
    });
    out.dedup();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(file: &str, from: &str, to: &str) -> Result<Content, DataError> {
        let swap = |name: &str, text: &str| {
            // Git may have checked the data out with CRLF; the fixtures are written with LF.
            let text = text.replace("\r\n", "\n");
            if name == file {
                assert!(text.contains(from), "test fixture: {from:?} not in {file}");
                text.replacen(from, to, 1)
            } else {
                text.to_owned()
            }
        };
        Content::from_texts(
            &swap("slab_01.json", SLAB_01_JSON),
            &swap("stones.json", STONES_JSON),
            &swap("enemies.json", ENEMIES_JSON),
            &swap("waves.json", WAVES_JSON),
        )
    }

    #[test]
    fn builtin_content_converts_units_once() {
        let c = Content::builtin();
        let warding = c.stone(c.stone_kind("warding").unwrap());
        assert_eq!((warding.tiers.len(), warding.glyph, warding.policy, warding.attack), (3, 'W', Policy::First, Attack::Bolt));
        let t = warding.tiers[0];
        assert_eq!((t.cost, t.range, t.cooldown, t.damage, t.projectile_speed), (50, 7_000, 24, 10_000, 666));
        let pillar = c.stone(c.stone_kind("rune_pillar").unwrap());
        assert_eq!((pillar.attack, pillar.tiers[0].slow, pillar.tiers[0].slow_ticks), (Attack::Pulse, 350, 45));
        assert_eq!(c.stone(c.stone_kind("sky_obelisk").unwrap()).targets, Targets::Air);
        let sh = c.enemy(c.enemy_kind("shambler").unwrap());
        assert_eq!((sh.hp, sh.speed, sh.movement, sh.climb_speed, sh.wall_damage), (55_000, 800, Movement::Climber, 600, 400));
        let runner = c.enemy(c.enemy_kind("runner").unwrap());
        assert_eq!((runner.jump_chance, runner.jump_detour, runner.movement), (400, 4_000, Movement::Walker));
        assert_eq!(c.enemy(c.enemy_kind("wraith").unwrap()).circle, (6, 14));
        let boss = c.enemy(c.enemy_kind("colossus").unwrap());
        assert_eq!(boss.summon, Some(Summon { enemy: c.enemy_kind("shambler").unwrap(), every: 210, count: 2 }));
        let r = c.rules;
        assert_eq!((r.start_dust, r.tombstones, r.build_ticks, r.early_call_milli_per_s), (150, 10, 900, 500));
        assert_eq!((r.edge_reach, r.refund_percent, r.wall.cost, r.wall.hp, r.wall.breach_penalty), (3_000, 70, 15, 300_000, 8_000));
        assert_eq!((r.nature.from_wave, r.nature.min_tier, r.nature.slow), (5, 1, 150));
        assert_eq!(c.waves.len(), 10);
        assert_eq!(c.waves[9].hp_scale, 2_400);
        // About 12 minutes: build time and nine timed waves, then the boss wave until it's done.
        let minutes = f64::from(c.scheduled_ticks()) / f64::from(TICK_HZ) / 60.0;
        assert!((10.0..11.5).contains(&minutes), "{minutes}");
    }

    #[test]
    fn content_errors_are_structured() {
        use ErrorCode::*;
        let cases: [(&str, &str, &str, &str, ErrorCode); 22] = [
            ("stones.json", "\"range_m\": 7.0", "\"rnage_m\": 7.0", "stones[0].tiers[0].rnage_m", UnknownField),
            ("stones.json", "\"glyph\": \"P\"", "\"glyph\": \"W\"", "stones[1].glyph", Duplicate),
            ("stones.json", "\"glyph\": \"P\"", "\"glyph\": \"#\"", "stones[1].glyph", Invalid),
            ("stones.json", "\"splash_m\": 1.8", "\"splash_m\": 0", "stones[2].tiers[0].splash_m", Invalid),
            ("stones.json", "\"policy\": \"first\"", "\"policy\": \"fastest\"", "stones[0].policy", BadChoice),
            ("stones.json", "\"range_m\": 7.0", "\"range_m\": \"far\"", "stones[0].tiers[0].range_m", BadType),
            ("stones.json", "\"range_m\": 7.0, ", "", "stones[0].tiers[0].range_m", Missing),
            ("enemies.json", "\"hp\": 55", "\"hp\": -1", "enemies[0].hp", OutOfRange),
            // Bounds that keep milli-HP in i32 even at the largest wave scale (100 000 HP x 20).
            ("enemies.json", "\"hp\": 55", "\"hp\": 100001", "enemies[0].hp", OutOfRange),
            ("enemies.json", "\"dust\": [3, 5]", "\"dust\": [3, 10001]", "enemies[0].dust", OutOfRange),
            ("enemies.json", "\"dust\": [3, 5]", "\"dust\": [3]", "enemies[0].dust", BadType),
            ("enemies.json", "\"enemy\": \"shambler\", \"every_s\"", "\"enemy\": \"ghoul\", \"every_s\"", "enemies[3].summon.enemy", UnknownRef),
            // Ground enemies must be able to break walls, or a full block stalls the game.
            ("enemies.json", ", \"wall_dps\": 12", "", "enemies[0].wall_dps", Missing),
            ("enemies.json", "\"wall_dps\": 12", "\"wall_dps\": 0", "enemies[0].wall_dps", OutOfRange),
            ("enemies.json", "\"tombstones\": 1
", "\"tombstones\": 1, \"wall_dps\": 5
", "enemies[2].wall_dps", Invalid),
            // A walker summoned onto a cliff could never move.
            (
                "enemies.json",
                "\"movement\": \"walker\",
      \"dust\": [150, 150], \"tombstones\": 5, \"wall_dps\": 150,
      \"summon\": { \"enemy\": \"shambler\"",
                "\"movement\": \"climber\",
      \"dust\": [150, 150], \"tombstones\": 5, \"wall_dps\": 150,
      \"summon\": { \"enemy\": \"runner\"",
                "enemies[3].summon.enemy",
                Invalid,
            ),
            ("waves.json", "\"enemy\": \"runner\", \"count\": 6, \"from\": \"gorges\"", "\"enemy\": \"runner\", \"count\": 6, \"from\": \"cliffs\"", "waves[1].groups[1].from", Invalid),
            ("waves.json", "\"enemy\": \"wraith\", \"count\": 4, \"from\": \"air\"", "\"enemy\": \"wraith\", \"count\": 4, \"from\": \"gorges\"", "waves[3].groups[1].from", Invalid),
            ("waves.json", "\"from\": \"gorge_mid\"", "\"from\": \"gorge_nowhere\"", "waves[9].groups[0].from", UnknownRef),
            ("waves.json", "\"enemy\": \"colossus\"", "\"enemy\": \"dragon\"", "waves[9].groups[0].enemy", UnknownRef),
            ("waves.json", "\"hp\": 2.4", "\"hp\": 21", "waves[9].hp", OutOfRange),
            ("waves.json", "\"tombstones\": 10", "\"tombstones\": 10,", "", Syntax),
        ];
        for (file, from, to, field, code) in cases {
            let e = with(file, from, to).err().unwrap_or_else(|| panic!("{from:?} -> {to:?} was accepted"));
            assert_eq!(e.file, file);
            assert_eq!(e.code, code, "{from:?} -> {to:?}: {e}");
            if !field.is_empty() {
                assert_eq!(e.field, field, "{e}");
            }
        }
        // The largest legal enemy at the largest legal wave scale still loads.
        let big = with("enemies.json", "\"hp\": 1250", "\"hp\": 100000").unwrap();
        assert_eq!(big.enemies[3].hp, 100_000_000);
    }

    #[test]
    fn leaping_is_a_capability_and_tiny_chances_are_refused() {
        let c = Content::builtin();
        let runner = c.enemy(c.enemy_kind("runner").unwrap());
        assert!(runner.leaps && runner.jump_chance == 400);
        assert!(!c.enemy(c.enemy_kind("shambler").unwrap()).leaps);
        let zero = with("enemies.json", "\"chance\": 0.4", "\"chance\": 0").unwrap();
        let r = zero.enemy(zero.enemy_kind("runner").unwrap());
        assert!(r.leaps && r.jump_chance == 0, "chance 0 still leaps when a ledge is the only way");
        let e = with("enemies.json", "\"chance\": 0.4", "\"chance\": 0.0004").unwrap_err();
        assert_eq!((e.code, e.field.as_str()), (ErrorCode::OutOfRange, "enemies[1].jump.chance"), "{e}");
    }

    /// Review finding: a leaping summoner can reach a ledge-only island; a summon that can't
    /// leap would be born there with no way out. Validation must catch it.
    #[test]
    fn summons_must_be_able_to_leave_every_cell_their_summoner_reaches() {
        let lf = |t: &str| t.replace("\r\n", "\n");
        // Cut the west lane (row 11) off from the ring, so only the ledge from top:4,13 reaches it,
        // and drop the west cliff's spawn so the map itself stays valid.
        let map = lf(SLAB_01_JSON)
            .replacen("\"....j..#GGGG#.######\"", "\"....j#.#GGGG#.######\"", 1)
            .replacen("\"-----------B--------\"", "\"-----------|--------\"", 1)
            .replacen(",\n    \"cliff_west\": \"west:11,5\"", "", 1);
        assert!(!map.contains("cliff_west") && map.contains("....j#.#"));
        let leaping_boss = lf(ENEMIES_JSON).replacen("\"movement\": \"walker\",\n      \"dust\": [150", "\"movement\": \"walker\", \"jump\": { \"chance\": 0 },\n      \"dust\": [150", 1);
        assert_ne!(leaping_boss, lf(ENEMIES_JSON));

        Content::from_texts(&map, STONES_JSON, ENEMIES_JSON, WAVES_JSON).expect("a walking boss never reaches the island");
        let e = Content::from_texts(&map, STONES_JSON, &leaping_boss, WAVES_JSON).unwrap_err();
        assert_eq!((e.file.as_str(), e.field.as_str(), e.code), ("enemies.json", "enemies[3].summon.enemy", ErrorCode::Unreachable), "{e}");
        assert!(e.reason.contains("top:"), "names the cell: {e}");
        // Summons that can leap too are fine.
        let leaping_summons = leaping_boss.replacen("\"enemy\": \"shambler\", \"every_s\"", "\"enemy\": \"runner\", \"every_s\"", 1);
        Content::from_texts(&map, STONES_JSON, &leaping_summons, WAVES_JSON).expect("runners leap back out");
    }

    #[test]
    fn content_hash_ignores_line_endings_but_not_content() {
        let a = Content::builtin();
        let crlf = |t: &str| t.replace("\r\n", "\n").replace('\n', "\r\n");
        let b = Content::from_texts(&crlf(SLAB_01_JSON), &crlf(STONES_JSON), &crlf(ENEMIES_JSON), &crlf(WAVES_JSON)).unwrap();
        assert_eq!(a.hash, b.hash);
        let c = with("waves.json", "\"start_dust\": 150", "\"start_dust\": 151").unwrap();
        assert_ne!(a.hash, c.hash);
    }
}
