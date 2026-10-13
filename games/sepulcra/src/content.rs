//! Content as data: the map, stones, enemies and waves are JSON files under `data/`, so agents
//! tune the game without touching code. Everything is validated on load and converted to the
//! sim's fixed-point units ([`crate::fixed`]) once.
//!
//! The default game is embedded with `include_str!`; [`Content::load_dir`] reads a custom set
//! (same four file names) for balance experiments.

use std::path::Path;

use specula_core::hash::StableHasher;
use specula_core::Json;

use crate::error::{DataError, Obj};
use crate::fixed::{milli, mm, permille, ticks, TICK_HZ};
use crate::map::Map;

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
    /// Lane-jump chance per jump point reached, permille (0 = never jumps).
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
    /// Stable hash of the four source files: part of every state hash, so a replay can tell
    /// when it is run against different content.
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
        let wave_defs = parse_waves(waves, &map_v, &enemy_defs, &mut rules)?;
        let mut h = StableHasher::with_seed(0x5e9_c1a);
        for t in [map, stones, enemies, waves] {
            h.write_str(t);
        }
        Ok(Content { map: map_v, stones: stone_defs, enemies: enemy_defs, waves: wave_defs, rules, hash: h.finish() })
    }

    /// Reads `slab_01.json`, `stones.json`, `enemies.json` and `waves.json` from `dir`.
    pub fn load_dir(dir: &Path) -> Result<Content, DataError> {
        let read = |name: &str| {
            std::fs::read_to_string(dir.join(name)).map_err(|e| DataError::new(name, "", format!("cannot read {}: {e}", dir.join(name).display())))
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
    Json::parse(text).map_err(|e| DataError::new(file, format!("byte {}", e.offset), e.message))
}

fn glyph(o: &Obj, key: &str) -> Result<char, DataError> {
    let s = o.str(key)?;
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(c), None) if c.is_ascii_graphic() && !"#.jGSx-|B=".contains(c) => Ok(c),
        _ => Err(o.err(key, "expected one printable ASCII character not used by the map legend (# . j G S x - | B =)")),
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
            return Err(o.err("id", format!("duplicate stone id {id:?}")));
        }
        let g = glyph(&o, "glyph")?;
        if stones.iter().any(|s| s.glyph == g) {
            return Err(o.err("glyph", format!("glyph '{g}' is already used")));
        }
        let targets = match o.str("targets")? {
            "ground" => Targets::Ground,
            "air" => Targets::Air,
            "both" => Targets::Both,
            _ => return Err(o.err("targets", "expected \"ground\", \"air\" or \"both\"")),
        };
        let policy = match o.str("policy")? {
            "first" => Policy::First,
            "last" => Policy::Last,
            "strongest" => Policy::Strongest,
            "closest" => Policy::Closest,
            _ => return Err(o.err("policy", "expected \"first\", \"last\", \"strongest\" or \"closest\"")),
        };
        let attack = match o.str("attack")? {
            "bolt" => Attack::Bolt,
            "lob" => Attack::Lob,
            "pulse" => Attack::Pulse,
            _ => return Err(o.err("attack", "expected \"bolt\", \"lob\" or \"pulse\"")),
        };
        let mut tiers = Vec::new();
        let tiers_json = o.arr("tiers")?;
        if !(1..=5).contains(&tiers_json.len()) {
            return Err(o.err("tiers", "needs 1..=5 tiers"));
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
                return Err(to.err("projectile_mps", "bolts and lobs need a projectile speed of at least 0.03 m/s"));
            }
            if attack == Attack::Lob && tier.splash <= 0 {
                return Err(to.err("splash_m", "a lob needs a splash radius"));
            }
            if tier.damage == 0 && tier.slow == 0 {
                return Err(to.err("damage", "a tier must deal damage or slow"));
            }
            tiers.push(tier);
        }
        stones.push(StoneDef { id, name: o.str("name")?.to_owned(), glyph: g, targets, policy, attack, tiers });
    }
    if stones.is_empty() || stones.len() > 64 {
        return Err(root.err("stones", "needs 1..=64 stones"));
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
        return Err(root.err("enemies", "needs 1..=64 enemies"));
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
            return Err(o.err("id", format!("duplicate enemy id {id:?}")));
        }
        let movement = match o.str("movement")? {
            "walker" => Movement::Walker,
            "climber" => Movement::Climber,
            "flyer" => Movement::Flyer,
            _ => return Err(o.err("movement", "expected \"walker\", \"climber\" or \"flyer\"")),
        };
        let (jump_chance, jump_detour) = match o.get("jump") {
            None => (0, 0),
            Some(j) => {
                let jo = Obj::new(j, F, o.path("jump"))?;
                jo.only(&["chance", "max_detour_m"])?;
                if movement == Movement::Flyer {
                    return Err(o.err("jump", "flyers don't lane-jump"));
                }
                (permille(jo.num("chance", 0.0, 1.0)?), mm(jo.num_or("max_detour_m", 0.0, 0.0, 1000.0)?))
            }
        };
        let pair = |key: &str, min: i64, max: i64| -> Result<(i64, i64), DataError> {
            let a = o.arr(key)?;
            match (a.first().and_then(Json::as_i64), a.get(1).and_then(Json::as_i64), a.len()) {
                (Some(lo), Some(hi), 2) if min <= lo && lo <= hi && hi <= max => Ok((lo, hi)),
                _ => Err(o.err(key, format!("expected [min, max] integers with {min} <= min <= max <= {max}"))),
            }
        };
        let circle = if movement == Movement::Flyer {
            let (lo, hi) = pair("circle_points", 0, 1000)?;
            (lo as u16, hi as u16)
        } else {
            (0, 0)
        };
        let (dlo, dhi) = pair("dust", 0, 100_000)?;
        let summon = match o.get("summon") {
            None => None,
            Some(s) => {
                let so = Obj::new(s, F, o.path("summon"))?;
                so.only(&["enemy", "every_s", "count"])?;
                let target = so.str("enemy")?;
                let k = ids.iter().position(|&x| x == target).ok_or_else(|| so.err("enemy", format!("no enemy {target:?}")))?;
                if k == i {
                    return Err(so.err("enemy", "an enemy can't summon itself"));
                }
                Some(Summon { enemy: EnemyKind(k as u8), every: ticks(so.num("every_s", 0.1, 600.0)?), count: so.int("count", 1, 100)? as u16 })
            }
        };
        out.push(EnemyDef {
            id,
            name: o.str("name")?.to_owned(),
            glyph: glyph(&o, "glyph")?,
            hp: milli(o.num("hp", 0.001, 1_000_000.0)?),
            speed: mm(o.num("speed_mps", 0.01, 100.0)?),
            movement,
            climb_speed: permille(o.num_or("climb_speed", 1.0, 0.05, 2.0)?),
            jump_chance,
            jump_detour,
            circle,
            dust: (dlo as i32, dhi as i32),
            tombstones: o.int("tombstones", 0, 1000)? as i32,
            wall_damage: milli(o.num_or("wall_dps", 0.0, 0.0, 1_000_000.0)?) / TICK_HZ as i32,
            summon,
        });
    }
    // A summoned enemy that flies would need an air entry; keep summons on the ground.
    for (i, e) in out.iter().enumerate() {
        if let Some(s) = e.summon {
            if out[usize::from(s.enemy.0)].flies() || e.flies() {
                return Err(DataError::new(F, format!("enemies[{i}].summon"), "summoner and summoned must both walk or climb"));
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
        return Err(root.err("waves", "needs 1..=1000 waves"));
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
            let k = enemies.iter().position(|e| e.id == eid).ok_or_else(|| go.err("enemy", format!("no enemy {eid:?}")))?;
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
            income: o.int_or("income", 0, 0, 1_000_000)? as i32,
            hp_scale: permille(o.num_or("hp", 1.0, 0.01, 100.0)?),
            groups,
        });
    }
    Ok(waves)
}

/// `"from"`: `"gorges"`, `"cliffs"`, `"ground"` (both), `"air"`, a spawn name, or a list of names.
fn spawn_points(go: &Obj, map: &Map, def: &EnemyDef) -> Result<Vec<SpawnPoint>, DataError> {
    let names: Vec<&str> = match go.req("from")? {
        Json::Str(s) => vec![s.as_str()],
        Json::Arr(a) => a.iter().map(|v| v.as_str().ok_or_else(|| go.err("from", "expected spawn names"))).collect::<Result<_, _>>()?,
        _ => return Err(go.err("from", "expected a spawn name, a list of names, \"gorges\", \"cliffs\", \"ground\" or \"air\"")),
    };
    if def.flies() {
        if names != ["air"] {
            return Err(go.err("from", format!("{} flies: use \"air\"", def.id)));
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
            "air" => return Err(go.err("from", format!("{} doesn't fly", def.id))),
            name => {
                let i = map.spawns.iter().position(|s| s.name == name).ok_or_else(|| go.err("from", format!("no spawn named {name:?}")))?;
                out.push(SpawnPoint::Ground(i as u16));
            }
        }
    }
    if !def.climbs() {
        if let Some(SpawnPoint::Ground(i)) = out.iter().find(|p| matches!(p, SpawnPoint::Ground(i) if map.spawns[usize::from(*i)].climbers_only)) {
            return Err(go.err("from", format!("{} can't climb, but {} is a cliff base", def.id, map.spawns[usize::from(*i)].name)));
        }
    }
    if out.is_empty() {
        return Err(go.err("from", "no spawn points"));
    }
    out.sort_by_key(|p| match p {
        SpawnPoint::Ground(i) | SpawnPoint::Air(i) => *i,
    });
    out.dedup();
    Ok(out)
}
