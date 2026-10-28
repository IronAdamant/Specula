//! Save games: the scene for a loaded save, adopting what the save's world already holds.
//!
//! [`Scene::build`] replaces the slab and the player's builds with fresh entities: right for a
//! new run, wrong for a loaded save, where the world (saved bit for bit) already holds them and
//! new entities would move the world's id counter and hash away from the run that saved.
//! [`Scene::adopt`] finds those entities instead (`/units/stones/<cell>` and the like) and
//! rebuilds only the transient presentation (`/live`, `/cursor`), which is never saved or hashed.

use super::*;

impl Scene {
    /// The scene over a loaded world that holds what `state` built, or why it can't adopt it
    /// (then [`Scene::build`] is the fallback).
    pub fn adopt(world: &mut GameWorld<'_>, agent: &str, state: &State) -> Result<Scene, String> {
        let content = state.content();
        let map = &content.map;
        let geo = SlabGeo::new(map);
        let find = |world: &World, path: &str| world.find_path(path).map_err(|_| format!("the world has no {path}"));
        let units = find(world, "/units")?;
        let (stones_g, walls_g, roots_g, tombs_g) =
            (find(world, "/units/stones")?, find(world, "/units/walls")?, find(world, "/units/roots")?, find(world, "/units/tombstones")?);
        let _ = units;
        // What the player built, by cell (entities are named after their cell).
        let by_cell = |world: &World, group: u64| -> Vec<(CellId, u64)> {
            world.children_of(Some(group)).filter_map(|id| Some((map.cell_ref(&world.get(id)?.name)?, id))).collect()
        };
        let mut stones = BTreeMap::new();
        for (cell, id) in by_cell(world, stones_g) {
            let (Some(e), Some(s)) = (world.get(id), state.stone(cell)) else {
                return Err(format!("a stone entity stands on {} but the saved game has none there", map.cell_name(cell)));
            };
            let tier = e.props.get("tier").and_then(Json::as_u64).map_or(s.tier, |t| t.saturating_sub(1) as u8);
            stones.insert(cell, StoneView { id, kind: s.kind.0, tier, lit: e.light.is_some() });
        }
        let mut walls = BTreeMap::new();
        for (cell, id) in by_cell(world, walls_g) {
            let e = world.get(id).ok_or("a wall vanished")?;
            // sync_walls scales a wall's height by 0.4 + 0.15 × its damage bucket.
            let bucket = ((e.scale.y - 0.4) / 0.15).round().clamp(1.0, 4.0) as u8;
            walls.insert(cell, WallView { id, bucket });
        }
        let roots: BTreeMap<CellId, u64> = by_cell(world, roots_g).into_iter().collect();
        // Tombstones in order, with where each stands upright (as build_tombstones placed it).
        let n = content.rules.tombstones.max(0) as usize;
        let [cx, _, cz] = map.graveyard_centre.to_f32();
        let cols = (n as f32).sqrt().ceil().max(1.0) as usize;
        let rows = n.div_ceil(cols);
        let mut tombs = Vec::with_capacity(n);
        for i in 0..n {
            let (r, c) = (i / cols, i % cols);
            let in_row = if r + 1 == rows { n - r * cols } else { cols };
            let x = cx + (c as f32 - (in_row as f32 - 1.0) / 2.0) * 1.7 + 0.15 * (((i * 53) % 7) as f32 / 3.0 - 1.0);
            let z = cz + (r as f32 - (rows as f32 - 1.0) / 2.0) * 2.2;
            let pos = Vec3::new(x, slab::GRAVEYARD_HEIGHT, z);
            let rot = Vec3::new(0.0, ((i * 37) % 13) as f32 - 6.0, 0.0);
            let path = format!("/units/tombstones/tomb_{:02}", i + 1);
            let id = find(world, &path)?;
            let down = world.get(id).is_some_and(|e| e.tags.contains("raised"));
            tombs.push((id, pos, rot, down));
        }
        // The transient presentation, afresh (a loaded world has none).
        for name in ["live", "cursor"] {
            if let Some(old) = ents::top_level(world, name) {
                ents::delete(world, old);
            }
        }
        let sub = |world: &mut GameWorld<'_>, parent: u64, name: &str| ents::group(world, agent, Some(parent), name, Vec3::ZERO, Vec3::ZERO, &[], Json::Null);
        let live = ents::transient_group(world, agent, "live", Layer::World, &["game", "transient"]);
        let (enemies_g, bolts_g, fx_g) = (sub(world, live, "enemies"), sub(world, live, "bolts"), sub(world, live, "fx"));
        let ui = ents::transient_group(world, agent, "cursor", Layer::Ui, &["ui"]);
        let cursor_part = Part::cube("cell", Vec3::ZERO, Vec3::ONE, highlight::VALID.color).glow(highlight::VALID.emissive);
        let cursor = ents::part(world, agent, ui, &cursor_part);
        ents::show(world, agent, cursor, false);
        let ring_g = sub(world, ui, "range");
        let ring = (0..RING_DOTS)
            .map(|i| ents::part_named(world, agent, ring_g, &format!("dot_{i:02}"), &Part::cube("", Vec3::ZERO, Vec3::new(0.16, 0.16, 0.5), highlight::RANGE_DOT.color).glow(highlight::RANGE_DOT.emissive)))
            .collect();
        ents::show(world, agent, ring_g, false);
        Ok(Scene {
            geo,
            agent: agent.to_owned(),
            stones_g,
            walls_g,
            roots_g,
            tombs_g,
            enemies_g,
            bolts_g,
            fx_g,
            stones,
            walls,
            roots,
            enemies: BTreeMap::new(),
            corpses: Vec::new(),
            bolts: BTreeMap::new(),
            fx: Vec::new(),
            tombs,
            cursor,
            cursor_shown: CursorLook::Hidden,
            ring_g,
            ring,
            ring_shown: None,
            pace: 1.0,
            seen: Vec::new(),
            live: Vec::new(),
        })
    }
}
