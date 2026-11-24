//! What burns on the slab (the elements' fuel; DESIGN.md, "Elemental stones").
//!
//! The slab is stone: fragments, lanes, cliffs and the graveyard's slabs do not burn. What
//! grows on it does: the grass at the rock's edges (STYLE.md, "carved stone, growing nature")
//! and, in the proving ground, the dry grass its data lays along a lane. The grass the app draws
//! (`sepulcra-app/src/grass.rs`) is picked by the rule below; the app's test holds the two
//! equal on map 1, so the fire burns where grass is seen, and nowhere else.

use crate::map::{CellId, CellKind, Face, Map};

/// The most grass patches on the slab (the app's `grass::MAX_PATCHES`).
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
    fn grass_burns_on_fragments_only() {
        let m = Map::parse(SLAB_01_JSON, "slab_01.json").unwrap();
        let g = grass_cells(&m);
        assert!(!g.is_empty());
        assert!(g.iter().all(|&c| m.cell(c).kind == CellKind::Fragment), "stone lanes and the graveyard never burn");
        assert!(g.windows(2).all(|w| w[0] < w[1]), "in cell order");
    }
}
