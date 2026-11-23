//! How "near" is found: a sorted grid, no hashing.
//!
//! Space is cut into cubes of `cell_m` metres. Every node is entered in each cell its box
//! overlaps (at most [`MAX_NODE_CELLS`], checked when the node is given), as an [`Entry`] of
//! (cell, node index); the entries are one `Vec` sorted by cell then node, so a cell's nodes are
//! a binary search away and every walk is in a fixed order. A query (a box) visits the cells it
//! overlaps in x, y, z order; a node met in several of them is counted once, in the first cell the
//! two boxes share (the per-axis larger of their lowest cells), so no set is needed.
//!
//! The grid is derived: it is rebuilt from the nodes when one is added, removed or moved (the
//! next step or act that needs it), in the memory it already has, never saved and never hashed.
//! A rebuild is counted in the step's or the act's operations ([`rebuild_ops`]).

use crate::sim::Node;
use specula_core::dmath;
use specula_core::math::Vec3;

/// Cells one node's box may overlap. A box 3 cells wide on each axis overlaps at most 4 × 4 × 4.
pub const MAX_NODE_CELLS: u32 = 64;
/// Coordinates are within ±1,000 km (with the smallest cell, 4 million cells: inside an `i32`).
pub const MAX_COORD: f32 = 1.0e6;

/// One node in one cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Entry {
    pub cell: [i32; 3],
    pub node: u32,
}

/// The cell a coordinate is in.
pub fn cell_of(v: f32, cell: f32) -> i32 {
    dmath::floor(v / cell) as i32
}

/// The lowest and highest cells a box (by its corners) overlaps.
pub fn cell_box(lo: Vec3, hi: Vec3, cell: f32) -> ([i32; 3], [i32; 3]) {
    (
        [cell_of(lo.x, cell), cell_of(lo.y, cell), cell_of(lo.z, cell)],
        [cell_of(hi.x, cell), cell_of(hi.y, cell), cell_of(hi.z, cell)],
    )
}

/// How many cells lie in `lo..=hi`.
pub fn cells_in(lo: [i32; 3], hi: [i32; 3]) -> u64 {
    (0..3).map(|a| (i64::from(hi[a]) - i64::from(lo[a]) + 1).max(0) as u64).product()
}

/// The sorted grid.
#[derive(Clone, Debug, Default)]
pub struct Grid {
    entries: Vec<Entry>,
}

impl Grid {
    /// Enters every node in each cell its box overlaps, then sorts, reusing this grid's memory.
    /// Answers its cost in [`rebuild_ops`] units: one per entry written and the sort's
    /// comparisons at most (`entries × ⌈log2 entries⌉`).
    pub fn rebuild(&mut self, nodes: &[Node], cell: f32) -> u64 {
        self.entries.clear();
        for (i, n) in nodes.iter().enumerate() {
            let (lo, hi) = cell_box(n.center - n.half, n.center + n.half, cell);
            for x in lo[0]..=hi[0] {
                for y in lo[1]..=hi[1] {
                    for z in lo[2]..=hi[2] {
                        self.entries.push(Entry { cell: [x, y, z], node: i as u32 });
                    }
                }
            }
        }
        self.entries.sort_unstable();
        rebuild_ops(self.entries.len() as u64)
    }

    /// The nodes entered in one cell, in index (so id) order.
    pub fn cell(&self, c: [i32; 3]) -> &[Entry] {
        let (start, end) = self.range(c);
        &self.entries[start..end]
    }

    /// Where one cell's entries lie in [`Grid::entries`].
    pub fn range(&self, c: [i32; 3]) -> (usize, usize) {
        let start = self.entries.partition_point(|e| e.cell < c);
        (start, start + self.entries[start..].partition_point(|e| e.cell == c))
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The cost of building a grid of `entries` entries: each written once, and at most
/// `⌈log2 entries⌉` comparisons each for the sort.
pub fn rebuild_ops(entries: u64) -> u64 {
    let log = if entries <= 1 { 0 } else { u64::from(64 - (entries - 1).leading_zeros()) };
    entries + entries * log
}

/// The entries a grid of these nodes holds (one per cell each box overlaps), counted without
/// building it.
pub fn entries_for(nodes: &[Node], cell: f32) -> u64 {
    nodes.iter().map(|n| {
        let (lo, hi) = cell_box(n.center - n.half, n.center + n.half, cell);
        cells_in(lo, hi)
    }).sum()
}

/// The gap between two boxes (0 when they touch or overlap), metres.
pub fn box_gap(a_center: Vec3, a_half: Vec3, b_center: Vec3, b_half: Vec3) -> f32 {
    let d = |ca: f32, ha: f32, cb: f32, hb: f32| ((ca - cb).abs() - (ha + hb)).max(0.0);
    let x = d(a_center.x, a_half.x, b_center.x, b_half.x);
    let y = d(a_center.y, a_half.y, b_center.y, b_half.y);
    let z = d(a_center.z, a_half.z, b_center.z, b_half.z);
    (x * x + y * y + z * z).sqrt()
}
