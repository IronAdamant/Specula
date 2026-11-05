//! The level's props: the graveyard's statues, gates, candles and bones, as map data.
//!
//! Each prop in the map file's `props` list names its recipe, where it stands on the top face
//! (`at`, metres), its turn and the cells it covers. **`blocks_build`** is the owner's rule as
//! data (2026-11-06): big pieces (statues, the mausoleum, gates, columns, fences) block building,
//! so a stone can't be carved on any cell they cover; small dressing (candles, bones, wreaths,
//! small markers) doesn't, and is hidden while a stone stands on one of its cells (the
//! presentation's job: the sim only answers which props a cell holds). Walls go on lanes and
//! props only stand on fragments, so a prop never meets a wall and never blocks a path.
//!
//! ```text
//! "props": [
//!   { "name": "statue_angel", "what": "a grave angel", "at": [-3.6, -15.0], "yaw": 15,
//!     "blocks_build": true, "cells": ["top:7,2", "top:8,2"] },
//!   { "name": "bone_pile", "what": "a pile of bones", "at": [-16.0, 10.0], "yaw": 30,
//!     "blocks_build": false, "cells": ["top:1,14", "top:2,14", "top:1,15", "top:2,15"] }
//! ]
//! ```
//!
//! `recipe` defaults to the name (`recipes/<recipe>.json` in the game's assets). `what` is how the
//! game names the prop to a player ("a grave angel stands here"). Only the blocking cells are
//! part of the content hash: moving or renaming dressing doesn't invalidate recordings.

use specula_core::Json;

use crate::error::{DataError, ErrorCode, Obj};
use crate::fixed::{mm, Pos};
use crate::map::{CellId, CellKind, Face, Map};

/// Most props a map may hold.
pub const MAX_PROPS: usize = 256;
/// Most cells one prop may cover.
pub const MAX_CELLS: usize = 12;
/// Longest prop or recipe name, and the longest `what`.
pub const MAX_NAME: usize = 32;
pub const MAX_WHAT: usize = 64;

/// One prop of the level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prop {
    pub name: String,
    /// The recipe's name (`recipes/<recipe>.json`).
    pub recipe: String,
    /// How the game names it to a player: "a grave angel".
    pub what: String,
    /// Where it stands on the top face, in mm (`y` is 0: the presentation stands it on the
    /// surface of its cell).
    pub at: Pos,
    /// Its turn about +Y, in thousandths of a degree (presentation only).
    pub yaw_mdeg: i32,
    /// Big pieces block building on every cell they cover; small dressing doesn't.
    pub blocks_build: bool,
    /// The cells it covers, sorted, the cell under `at` among them.
    pub cells: Vec<CellId>,
}

impl Prop {
    /// Its turn in degrees, for presentation.
    pub fn yaw_deg(&self) -> f32 {
        self.yaw_mdeg as f32 / 1000.0
    }
}

fn valid_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= MAX_NAME && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Reads the map file's `props` (absent: none), against the map's cells.
pub(crate) fn parse(root: &Obj<'_>, map: &Map) -> Result<Vec<Prop>, DataError> {
    let Some(list) = root.get("props") else { return Ok(Vec::new()) };
    let list = list.as_array().ok_or_else(|| root.err("props", ErrorCode::BadType, "expected an array of props"))?;
    if list.len() > MAX_PROPS {
        return Err(root.err("props", ErrorCode::OutOfRange, format!("at most {MAX_PROPS} props, got {}", list.len())));
    }
    let mut props: Vec<Prop> = Vec::with_capacity(list.len());
    for (i, v) in list.iter().enumerate() {
        let o = Obj::new(v, root.file, root.path(&format!("props[{i}]")))?;
        o.only(&["name", "recipe", "what", "at", "yaw", "blocks_build", "cells"])?;
        let name = o.str("name")?;
        if !valid_name(name) {
            return Err(o.err("name", ErrorCode::Invalid, format!("a prop name is 1..={MAX_NAME} of a-z, 0-9 and _")));
        }
        if props.iter().any(|p| p.name == name) {
            return Err(o.err("name", ErrorCode::Duplicate, format!("another prop is already named {name:?}")));
        }
        let recipe = match o.get("recipe") {
            None => name,
            Some(_) => o.str("recipe")?,
        };
        if !valid_name(recipe) {
            return Err(o.err("recipe", ErrorCode::Invalid, format!("a recipe name is 1..={MAX_NAME} of a-z, 0-9 and _")));
        }
        let what = o.str("what")?;
        if what.trim().is_empty() || what.chars().count() > MAX_WHAT || what.chars().any(char::is_control) {
            return Err(o.err("what", ErrorCode::Invalid, format!("how a player hears the prop named, 1..={MAX_WHAT} characters (\"a grave angel\")")));
        }
        let at = o.arr("at")?;
        let coord = |k: usize, half: i32| -> Result<i32, DataError> {
            let v = at.get(k).and_then(Json::as_f64).ok_or_else(|| o.err("at", ErrorCode::BadType, "expected [x, z] in metres"))?;
            let lim = f64::from(half) / 1000.0;
            if !(-lim..lim).contains(&v) {
                return Err(o.err("at", ErrorCode::OutOfRange, format!("{v} is off the top face (-{lim}..{lim} m)")));
            }
            Ok(mm(v))
        };
        if at.len() != 2 {
            return Err(o.err("at", ErrorCode::BadType, "expected [x, z] in metres"));
        }
        let (x, z) = (coord(0, map.half_x)?, coord(1, map.half_z)?);
        let yaw_mdeg = mm(o.num_or("yaw", 0.0, -360.0, 360.0)?);
        let blocks_build = match o.req("blocks_build").map_err(|_| {
            o.err("blocks_build", ErrorCode::Missing, "say whether it blocks building: true for big pieces (statues, gates, columns, fences), false for small dressing (candles, bones, wreaths, small markers)")
        })? {
            Json::Bool(b) => *b,
            _ => return Err(o.err("blocks_build", ErrorCode::BadType, "expected true or false")),
        };
        let refs = o.arr("cells")?;
        if refs.is_empty() || refs.len() > MAX_CELLS {
            return Err(o.err("cells", ErrorCode::OutOfRange, format!("a prop covers 1..={MAX_CELLS} cells, got {}", refs.len())));
        }
        let mut cells = Vec::with_capacity(refs.len());
        for (k, r) in refs.iter().enumerate() {
            let field = format!("cells[{k}]");
            let s = r.as_str().ok_or_else(|| o.err(&field, ErrorCode::BadType, "expected a cell like \"top:8,2\""))?;
            let id = map.cell_ref(s).ok_or_else(|| o.err(&field, ErrorCode::UnknownRef, format!("no cell {s:?}")))?;
            let c = map.cell(id);
            if c.face != Face::Top || c.kind != CellKind::Fragment {
                return Err(o.err(&field, ErrorCode::Invalid, format!("{s} is '{}': props stand on top-face fragments ('#'), never on a lane, the graveyard or a cliff", c.kind.glyph())));
            }
            if cells.contains(&id) {
                return Err(o.err(&field, ErrorCode::Duplicate, format!("{s} is listed twice")));
            }
            cells.push(id);
        }
        cells.sort();
        let col = (x + map.half_x).div_euclid(map.cell_mm);
        let row = (z + map.half_z).div_euclid(map.cell_mm);
        let anchor = map.at(Face::Top, col as u8, row as u8);
        if !anchor.is_some_and(|a| cells.contains(&a)) {
            let named = anchor.map_or_else(|| format!("top:{col},{row}"), |a| map.cell_name(a));
            return Err(o.err("cells", ErrorCode::Invalid, format!("must include {named}, the cell under 'at'")));
        }
        props.push(Prop { name: name.to_owned(), recipe: recipe.to_owned(), what: what.trim().to_owned(), at: Pos::new(x, 0, z), yaw_mdeg, blocks_build, cells });
    }
    Ok(props)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Probatio found the cap's boundary untested (`<=` vs `<`).
    #[test]
    fn names_are_lowercase_words_up_to_the_cap() {
        let at_cap = "a".repeat(MAX_NAME);
        assert!(valid_name(&at_cap) && valid_name("statue_angel") && valid_name("t3"));
        for bad in [String::new(), "a".repeat(MAX_NAME + 1), "Statue".into(), "statue angel".into(), "statue-angel".into(), "\u{e9}t\u{e9}".into()] {
            assert!(!valid_name(&bad), "{bad:?}");
        }
    }
}
