//! Real grass on the slab (owner's look decision, STYLE.md: "real grass blades should replace flat
//! green"; moss may exceed the saturation rule on map 1).
//!
//! Grass is the `grass_patch` recipe: alpha-cutout tufts (`tex_grass_blades`, crossed cards)
//! scattered by Poisson-disc rules into one mesh per 2 m cell, swaying in the world's wind
//! (`world_env {wind}`, set by [`crate::game::night`]). A few seed variants of one recipe, so the
//! GPU instances the patches: one draw per variant, not per cell.
//!
//! **Where:** the graveyard's cells, and the fragment cells at the edges of the rock (STYLE.md:
//! moss and roots at fragment edges), never on a lane (the dead's paths stay readable), at most
//! [`MAX_PATCHES`]; and on every cell roots take (the flat moss patches of earlier builds), from
//! wave 5. Picked by a hash of the cell, so every run's slab is the same. Content with the
//! elements also has grass on every cell its rules burn (`Content::fuel`: in the proving ground,
//! dry grass laid on two lane stretches; the same patch stands in for its look), so the fire
//! burns only where grass is seen (`sepulcra::fuel::grass_shown`, held equal below).
//!
//! **Budget (iGPU):** a patch is 12 tufts of three upright quads (72 triangles; 4 mm boxes were
//! 432, with both broad faces shaded); the far side of the slab thins out by a dithered fade from
//! [`FADE_START`] to [`FADE_END`] metres, past the slab's far rim at the game's zoom. Grass casts
//! the moon's shadow only within [`SHADOW_END`] metres of the camera (zoomed in), where its
//! shadow is more than a few texels. The renderer draws its cutout batches after the opaque
//! ones, so whatever stands in front of it rejects its fragments by depth.

use crate::ents;
use crate::slab::SlabGeo;
use sepulcra::map::{CellKind, Face, Map};
use sepulcra::CellId;
use specula::game::GameWorld;
use specula::world::ModelSpec;
use specula_core::math::Vec3;
use specula_core::Json;

/// Most static grass patches on the slab (roots add theirs).
pub const MAX_PATCHES: usize = 72;
/// Tufts per patch.
pub const TUFTS: u32 = 12;
/// Seed variants (each one GPU mesh, instanced across its patches).
pub const VARIANTS: u64 = 4;
/// The foliage fade (metres from the camera; `world_env {wind: {fade_start, fade_end}}`).
pub const FADE_START: f32 = 70.0;
pub const FADE_END: f32 = 110.0;
/// Grass casts the moon's shadow only this near the camera (m of view depth; the game's camera
/// sits 56 m out, so only when zoomed in).
pub const SHADOW_END: f32 = 30.0;
/// A breeze over the slab: strength, frequency (Hz), gust.
pub const WIND: (f32, f32, f32) = (0.8, 0.45, 0.6);

/// A grass patch of variant `v` (seeds 1101, 1102, ...).
pub fn patch_spec(v: u64) -> ModelSpec {
    ModelSpec::new("recipes/grass_patch.json").with_seed(1101 + v % VARIANTS).with_param("tufts", f64::from(TUFTS))
}

fn hash(c: CellId) -> u32 {
    (c.0 as u32).wrapping_mul(2_654_435_761).rotate_left(13) ^ 0x5e9c
}

/// The top-face cells that get grass: graveyard cells, and fragment cells at the rock's edge (a
/// lane or the rim beside them), about two in three of those by hash, at most [`MAX_PATCHES`];
/// then every cell of `fuel` (the content's burning cells; none on the shipped maps).
pub fn cells(map: &Map, fuel: &[CellId]) -> Vec<CellId> {
    let mut out = edge_cells(map);
    out.extend_from_slice(fuel);
    out.sort();
    out.dedup();
    out
}

fn edge_cells(map: &Map) -> Vec<CellId> {
    let Some(top) = map.face(Face::Top).copied() else { return Vec::new() };
    let kind = |c: i32, r: i32| -> Option<CellKind> {
        if c < 0 || r < 0 || c >= i32::from(top.cols) || r >= i32::from(top.rows) {
            return None;
        }
        map.at(Face::Top, c as u8, r as u8).map(|id| map.cell(id).kind)
    };
    let mut out = Vec::new();
    for c in map.cells() {
        if c.face != Face::Top {
            continue;
        }
        let pick = match c.kind {
            CellKind::Graveyard => true,
            CellKind::Fragment => {
                let (cc, rr) = (i32::from(c.col), i32::from(c.row));
                let edge = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dc, dr)| kind(cc + dc, rr + dr) != Some(CellKind::Fragment));
                edge && !hash(c.id).is_multiple_of(3)
            }
            _ => false,
        };
        if pick {
            out.push(c.id);
        }
    }
    // Even thinning when there are more than the budget (deterministic: by hash).
    if out.len() > MAX_PATCHES {
        out.sort_by_key(|&id| (hash(id), id.0));
        out.truncate(MAX_PATCHES);
        out.sort_by_key(|id| id.0);
    }
    out
}

/// Spawns the slab's grass under `slab` (a group `grass`); returns the group's id.
pub fn build(world: &mut GameWorld<'_>, agent: &str, map: &Map, fuel: &[CellId], geo: &SlabGeo, slab: u64) -> u64 {
    let g = ents::group(world, agent, Some(slab), "grass", Vec3::ZERO, Vec3::ZERO, &["grass", "foliage"], Json::Null);
    for id in cells(map, fuel) {
        let (anchor, _) = geo.anchor(map, id);
        let h = hash(id);
        let yaw = (h % 360) as f32;
        let spec = ents::model_spec(g, &map.cell_name(id), patch_spec(u64::from(h >> 8)), anchor, Vec3::new(0.0, yaw, 0.0), 1.0, &[], Json::Null);
        ents::spawn(world, agent, spec);
    }
    g
}

/// A rooted cell's grass, as a child of its flat moss patch (scaled `patch_scale`, so the child
/// undoes that scale and sits on the patch's top face).
pub fn on_root(world: &mut GameWorld<'_>, agent: &str, patch: u64, patch_scale: Vec3, cell: CellId) -> u64 {
    let inv = Vec3::new(1.0 / patch_scale.x, 1.0 / patch_scale.y, 1.0 / patch_scale.z);
    let mut spec = ents::model_spec(patch, "grass", patch_spec(u64::from(hash(cell))), Vec3::new(0.0, 0.5, 0.0), Vec3::ZERO, 1.0, &[], Json::Null);
    spec.scale = inv;
    ents::spawn(world, agent, spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grass_grows_on_graveyard_and_rock_edges_never_on_lanes() {
        let content = sepulcra::content::Content::builtin();
        let map = &content.map;
        let cells = cells(map, &content.fuel);
        assert!(!cells.is_empty() && cells.len() <= MAX_PATCHES, "{} patches", cells.len());
        for id in &cells {
            let k = map.cell(*id).kind;
            assert!(matches!(k, CellKind::Graveyard | CellKind::Fragment), "grass on a {k:?} cell");
        }
        assert!(cells.iter().any(|id| map.cell(*id).kind == CellKind::Graveyard), "the graveyard has grass");
        assert_eq!(cells, super::cells(map, &content.fuel), "deterministic");
        // A few variants: the GPU instances them.
        let specs: std::collections::BTreeSet<String> = cells.iter().map(|id| format!("{:?}", patch_spec(u64::from(hash(*id) >> 8)))).collect();
        assert!(specs.len() as u64 <= VARIANTS);
    }

    /// The grass drawn here is the rules' `fuel::grass_shown`, cell for cell, and every cell the
    /// rules burn is drawn, on the shipped content and on the proving ground's (whose dry grass
    /// lies on lane cells), so fire burns where grass is seen (DESIGN.md, "What burns on the slab").
    #[test]
    fn the_rules_burn_the_grass_that_is_drawn() {
        let shipped = sepulcra::content::Content::builtin();
        assert!(shipped.fuel.is_empty());
        assert_eq!(cells(&shipped.map, &shipped.fuel), sepulcra::fuel::grass_drawn(&shipped.map), "the shipped slab's grass is as it was");
        assert_eq!(MAX_PATCHES, sepulcra::fuel::MAX_GRASS);
        let proving = sepulcra::content::Content::proving();
        let drawn = cells(&proving.map, &proving.fuel);
        assert_eq!(drawn, sepulcra::fuel::grass_shown(&proving.map, &proving.fuel));
        for f in &proving.fuel {
            assert!(drawn.contains(f), "fuel {} is drawn", proving.map.cell_name(*f));
        }
        let lanes: Vec<String> = proving.fuel.iter().filter(|&&f| proving.map.cell(f).kind != CellKind::Fragment).map(|&f| proving.map.cell_name(f)).collect();
        assert_eq!(lanes.len(), 7, "the dry grass on the two lane stretches is drawn: {lanes:?}");
    }
}
