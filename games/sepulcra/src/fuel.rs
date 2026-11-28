//! What burns on the slab (the elements' fuel; DESIGN.md, "Elemental stones").
//!
//! The slab is stone: fragments, lanes, cliffs and the graveyard's slabs do not burn. What
//! grows on it does: the grass at the rock's edges (STYLE.md, "carved stone, growing nature")
//! and, in the proving ground, the dry grass its data lays along a lane. The grass the app draws
//! (`sepulcra-app/src/grass.rs`) is [`grass_shown`]: the rock's-edge rule below and every cell
//! of the content's fuel. The app's test holds the two equal, on the shipped content and on the
//! proving ground's, so the fire burns where grass is seen, and nowhere else.

use crate::map::{CellId, CellKind, Face, Map};

/// The most grass patches the app draws on the slab (its `grass::MAX_PATCHES`): the rock's-edge
/// rule thins to it, so it is part of what the word `"grass"` names in a `fuel` list; it limits
/// nothing else in the rules.
pub const MAX_GRASS: usize = 72;

fn hash(c: CellId) -> u32 {
    (c.0 as u32).wrapping_mul(2_654_435_761).rotate_left(13) ^ 0x5e9c
}

/// The top-face cells with grass, as the app draws it: the graveyard's cells, and fragment cells
/// at the rock's edge (a lane or the rim beside them), about two in three of those by hash, at
/// most [`MAX_GRASS`], in cell order.
pub fn grass_drawn(map: &Map) -> Vec<CellId> {
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
    if out.len() > MAX_GRASS {
        out.sort_by_key(|&id| (hash(id), id.0));
        out.truncate(MAX_GRASS);
        out.sort_by_key(|id| id.0);
    }
    out
}

/// The grass the app draws for content whose fuel is `fuel` (the app's `grass::pick`, held
/// equal by its test), at most [`MAX_GRASS`] patches, in cell order: **the fuel first** (the dry
/// grass laid on a lane is drawn as grass too, though no rock's edge would give it any), then
/// the rock's-edge grass that does not burn ([`grass_drawn`]: the graveyard's, and edge grass the
/// content's fuel leaves out) while there is room, the rest dropped by the same hash as the edge
/// grass's own thinning. Content may have more fuel than that (the rules hold no budget of the
/// app's drawing): then only fuel is drawn, the fuel past the budget dropped by the hash. On the
/// shipped maps (no fuel) it is [`grass_drawn`] exactly.
pub fn grass_shown(map: &Map, fuel: &[CellId]) -> Vec<CellId> {
    let mut out: Vec<CellId> = fuel.to_vec();
    out.sort();
    out.dedup();
    if out.len() > MAX_GRASS {
        out.sort_by_key(|&id| (hash(id), id.0));
        out.truncate(MAX_GRASS);
        out.sort();
    }
    let mut rest: Vec<CellId> = grass_drawn(map).into_iter().filter(|c| out.binary_search(c).is_err()).collect();
    rest.sort_by_key(|&id| (hash(id), id.0));
    rest.truncate(MAX_GRASS.saturating_sub(out.len()));
    out.extend(rest);
    out.sort();
    out
}

/// The grass that burns: the drawn grass less the graveyard's (hallowed ground: the dead that
/// reach it are already past every stone, and a fire there would burn nothing the rules count).
pub fn grass_cells(map: &Map) -> Vec<CellId> {
    grass_drawn(map).into_iter().filter(|&c| map.cell(c).kind != CellKind::Graveyard).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::SLAB_01_JSON;

    #[test]
    fn every_fuel_cell_is_shown_and_the_budget_holds() {
        let shipped = crate::content::Content::builtin();
        assert_eq!(grass_shown(&shipped.map, &shipped.fuel), grass_drawn(&shipped.map), "the shipped slab's grass is as it was");
        let proving = crate::content::Content::proving();
        let shown = grass_shown(&proving.map, &proving.fuel);
        assert!(!proving.fuel.is_empty());
        for f in &proving.fuel {
            assert!(shown.binary_search(f).is_ok(), "fuel {} is drawn", proving.map.cell_name(*f));
        }
        assert!(shown.len() <= MAX_GRASS, "{} patches", shown.len());
        assert!(shown.windows(2).all(|w| w[0] < w[1]), "in cell order");
        // A slab all fuel (as many as the budget allows) leaves no room for the rest.
        let many: Vec<CellId> = shipped.map.cells().iter().filter(|c| c.face == Face::Top).map(|c| c.id).take(MAX_GRASS).collect();
        assert_eq!(grass_shown(&shipped.map, &many), many);
    }

    /// The shipped slab's grass, counted again another way: by the cells' names (a neighbour off
    /// the face has no name), with the counts of each step written out.
    #[test]
    fn the_shipped_slab_grows_the_grass_counted_again_by_name() {
        let m = Map::parse(SLAB_01_JSON, "slab_01.json").unwrap();
        let fragment = |c: i32, r: i32| m.cell_ref(&format!("top:{c},{r}")).is_some_and(|id| m.cell(id).kind == CellKind::Fragment);
        let (mut yard, mut edge) = (Vec::new(), Vec::new());
        for c in m.cells().iter().filter(|c| c.face == Face::Top) {
            let (cc, rr) = (i32::from(c.col), i32::from(c.row));
            match c.kind {
                CellKind::Graveyard => yard.push(c.id),
                CellKind::Fragment if !(fragment(cc + 1, rr) && fragment(cc - 1, rr) && fragment(cc, rr + 1) && fragment(cc, rr - 1)) => edge.push(c.id),
                _ => {}
            }
        }
        let thinned: Vec<CellId> = edge.iter().copied().filter(|&id| !hash(id).is_multiple_of(3)).collect();
        assert_eq!((yard.len(), edge.len(), thinned.len()), (16, 177, 111), "the graveyard's 16, the rock's edge, two in three of it");
        let mut all = yard;
        all.extend(thinned);
        all.sort_by_key(|&id| (hash(id), id.0));
        all.truncate(MAX_GRASS);
        all.sort();
        assert_eq!(grass_drawn(&m), all);
    }

    #[test]
    fn grass_burns_on_fragments_only() {
        let m = Map::parse(SLAB_01_JSON, "slab_01.json").unwrap();
        let g = grass_cells(&m);
        assert!(!g.is_empty());
        assert!(g.iter().all(|&c| m.cell(c).kind == CellKind::Fragment), "stone lanes and the graveyard never burn");
        assert!(g.windows(2).all(|w| w[0] < w[1]), "in cell order");
    }
}
