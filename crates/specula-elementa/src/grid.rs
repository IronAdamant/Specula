//! How "near" is found: a sorted grid, no hashing.
//!
//! Space is cut into cubes of `cell_m` metres. Every node is entered in each cell its box
//! overlaps (at most [`MAX_NODE_CELLS`], checked when the node is given), as an [`Entry`] of
//! (cell, node index); the entries are one `Vec` sorted by cell then node, so a cell's nodes are
//! a binary search away and every walk is in a fixed order (within a cell, by node index, which
//! is id order). A query (a box) visits the cells it overlaps in x, y, z order; a node met in
//! several of them is counted once, in the first cell the two boxes share (the per-axis larger of
//! their lowest cells), so no set is needed.
//!
//! The grid is derived: never saved and never hashed. It is brought up to date by the next spread
//! step after nodes were added, removed or moved into other cells ([`Grid::update`]), **without
//! sorting it again**: the entries of the nodes that changed are made and sorted on their own,
//! and merged in one pass with the entries kept (whose node indices are renumbered in the same
//! pass). A node that moves within the cells it already covers changes nothing here. The cost is
//! [`update_ops`], said before it runs and counted in the step's operations.

use crate::sim::Node;
use specula_core::dmath;
use specula_core::math::Vec3;

/// Cells one node's box may overlap. A box 3 cells wide on each axis overlaps at most 4 × 4 × 4.
pub const MAX_NODE_CELLS: u32 = 64;
/// Coordinates are within ±1,000 km (with the smallest cell, 4 million cells: inside an `i32`).
pub const MAX_COORD: f32 = 1.0e6;
/// Index of a node that is no longer in the grid (removed, or its entries made again).
const GONE: u32 = u32::MAX;

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

/// The cells a node's box overlaps.
pub fn node_cells(n: &Node, cell: f32) -> ([i32; 3], [i32; 3]) {
    cell_box(n.center - n.half, n.center + n.half, cell)
}

/// How many cells lie in `lo..=hi`.
pub fn cells_in(lo: [i32; 3], hi: [i32; 3]) -> u64 {
    (0..3).map(|a| (i64::from(hi[a]) - i64::from(lo[a]) + 1).max(0) as u64).product()
}

/// `⌈log2 n⌉` (0 for 0 and 1): the comparisons a sort of `n` makes per item, at most, and the
/// steps of a binary search over `n`.
pub fn log2_ceil(n: u64) -> u64 {
    if n <= 1 {
        0
    } else {
        u64::from(64 - (n - 1).leading_zeros())
    }
}

/// The cost of sorting `n` things: each placed once, and at most `⌈log2 n⌉` comparisons each.
pub fn sort_ops(n: u64) -> u64 {
    n + n * log2_ceil(n)
}

/// What a grid update costs (said before it runs, counted after):
/// - `old_nodes + nodes`: the node ids before and now, walked once together to renumber;
/// - `sort_ops(changed)`: the changed ids put in order (each node once);
/// - `sort_ops(fresh)`: the changed nodes' new entries made and sorted;
/// - `old_entries + entries`: every entry kept read once, and every entry written once.
pub fn update_ops(old_nodes: u64, nodes: u64, changed: u64, old_entries: u64, entries: u64, fresh: u64) -> u64 {
    old_nodes + nodes + sort_ops(changed) + sort_ops(fresh) + old_entries + entries
}

/// What has changed since the grid was last brought up to date: the ids of nodes added, removed
/// or moved into other cells (in the order they came, maybe twice), or everything.
#[derive(Clone, Debug, Default)]
pub struct Changes {
    pub(crate) all: bool,
    pub(crate) ids: Vec<u64>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        !self.all && self.ids.is_empty()
    }

    /// One node changed. Past `max(cap, 1024)` ids waiting, the whole grid is made again instead
    /// (so this list never grows past that, however many changes come between two steps).
    pub(crate) fn push(&mut self, id: u64, cap: usize) {
        if self.all {
            return;
        }
        if self.ids.len() >= cap.max(1024) {
            self.everything();
            return;
        }
        self.ids.push(id);
    }

    pub(crate) fn everything(&mut self) {
        self.all = true;
        self.ids.clear();
    }

    fn clear(&mut self) {
        self.all = false;
        self.ids.clear();
    }
}

/// The sorted grid, and the buffers it is brought up to date in (kept, so an update allocates
/// nothing once the grid has held as many entries before).
#[derive(Clone, Debug, Default)]
pub struct Grid {
    entries: Vec<Entry>,
    /// The ids of the nodes at the last update, by the index the entries hold.
    ids: Vec<u64>,
    /// The number of cells each node covered at the last update, by index.
    counts: Vec<u8>,
    spare: Vec<Entry>,
    fresh: Vec<Entry>,
    map: Vec<u32>,
    new_counts: Vec<u8>,
}

/// What an update will do: counted by walking the ids only, never the entries.
struct Plan {
    changed: u64,
    fresh: u64,
    entries: u64,
}

impl Grid {
    /// Walks the old ids, the nodes and the sorted changed ids together: calls `f(Some(old), new)`
    /// for each node whose entries stay (renumbered), and `f(None, new)` for each whose entries
    /// are made again (changed or added). Removed nodes are not met.
    fn walk(&self, nodes: &[Node], changes: &Changes, mut f: impl FnMut(Option<usize>, usize)) {
        let (mut o, mut c) = (0usize, 0usize);
        for (i, n) in nodes.iter().enumerate() {
            while o < self.ids.len() && self.ids[o] < n.id {
                o += 1;
            }
            while c < changes.ids.len() && changes.ids[c] < n.id {
                c += 1;
            }
            let changed = changes.all || (c < changes.ids.len() && changes.ids[c] == n.id);
            let was = o < self.ids.len() && self.ids[o] == n.id;
            f((was && !changed).then_some(o), i);
        }
    }

    fn plan(&self, nodes: &[Node], changes: &Changes, cell: f32) -> Plan {
        let mut kept = 0u64;
        let mut fresh = 0u64;
        self.walk(nodes, changes, |old, i| match old {
            Some(o) => kept += u64::from(self.counts[o]),
            None => {
                let (lo, hi) = node_cells(&nodes[i], cell);
                fresh += cells_in(lo, hi);
            }
        });
        Plan { changed: changes.ids.len() as u64, fresh, entries: kept + fresh }
    }

    /// What [`Grid::update`] would cost now (0 when nothing changed). A query: it sorts a copy of
    /// the changed ids, so it is not for every tick.
    pub fn pending_ops(&self, nodes: &[Node], changes: &Changes, cell: f32) -> u64 {
        if changes.is_empty() {
            return 0;
        }
        let mut sorted = changes.clone();
        sorted.ids.sort_unstable();
        let p = self.plan(nodes, &sorted, cell);
        update_ops(self.ids.len() as u64, nodes.len() as u64, p.changed, self.entries.len() as u64, p.entries, p.fresh)
    }

    /// Brings the grid up to date with `nodes` after `changes`, and clears them. Answers its cost
    /// ([`update_ops`]), which is what [`Grid::pending_ops`] said.
    pub fn update(&mut self, nodes: &[Node], changes: &mut Changes, cell: f32) -> u64 {
        if changes.is_empty() {
            return 0;
        }
        let raw = changes.ids.len() as u64;
        changes.ids.sort_unstable();
        let (old_nodes, old_entries) = (self.ids.len() as u64, self.entries.len() as u64);
        // Old index → new index, or GONE; and the changed nodes' entries, made afresh.
        let mut map = std::mem::take(&mut self.map);
        let mut fresh = std::mem::take(&mut self.fresh);
        let mut new_counts = std::mem::take(&mut self.new_counts);
        map.clear();
        map.resize(self.ids.len(), GONE);
        fresh.clear();
        new_counts.clear();
        new_counts.resize(nodes.len(), 0);
        self.walk(nodes, changes, |old, i| match old {
            Some(o) => {
                map[o] = i as u32;
                new_counts[i] = self.counts[o];
            }
            None => {
                let (lo, hi) = node_cells(&nodes[i], cell);
                new_counts[i] = cells_in(lo, hi) as u8;
                for x in lo[0]..=hi[0] {
                    for y in lo[1]..=hi[1] {
                        for z in lo[2]..=hi[2] {
                            fresh.push(Entry { cell: [x, y, z], node: i as u32 });
                        }
                    }
                }
            }
        });
        fresh.sort_unstable();
        // One pass: the kept entries (renumbered; the map keeps their order, as both index
        // orders are id order) merged with the fresh ones.
        let mut out = std::mem::take(&mut self.spare);
        out.clear();
        let mut f = 0usize;
        for e in &self.entries {
            let to = map[e.node as usize];
            if to == GONE {
                continue;
            }
            let kept = Entry { cell: e.cell, node: to };
            while f < fresh.len() && fresh[f] < kept {
                out.push(fresh[f]);
                f += 1;
            }
            out.push(kept);
        }
        out.extend_from_slice(&fresh[f..]);
        self.spare = std::mem::replace(&mut self.entries, out);
        self.ids.clear();
        self.ids.extend(nodes.iter().map(|n| n.id));
        std::mem::swap(&mut self.counts, &mut new_counts);
        self.new_counts = new_counts;
        let ops = update_ops(old_nodes, nodes.len() as u64, raw, old_entries, self.entries.len() as u64, fresh.len() as u64);
        self.map = map;
        self.fresh = fresh;
        changes.clear();
        ops
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

    /// The capacities of its buffers (a test holds that a warm grid allocates nothing more). The
    /// entries and the spare buffer trade places at each update, so they are given as a pair,
    /// smaller first; so are the two counts.
    pub fn capacities(&self) -> [usize; 7] {
        let (e, s) = (self.entries.capacity(), self.spare.capacity());
        let (c, n) = (self.counts.capacity(), self.new_counts.capacity());
        [
            e.min(s),
            e.max(s),
            self.ids.capacity(),
            c.min(n),
            c.max(n),
            self.fresh.capacity(),
            self.map.capacity(),
        ]
    }
}

/// The cost of building a grid of `entries` entries from nothing: each written once, and at most
/// `⌈log2 entries⌉` comparisons each for the sort (what a full rebuild costs; an update after a
/// few changes costs far less, [`update_ops`]).
pub fn rebuild_ops(entries: u64) -> u64 {
    sort_ops(entries)
}

/// The entries a grid of these nodes holds (one per cell each box overlaps), counted without
/// building it.
pub fn entries_for(nodes: &[Node], cell: f32) -> u64 {
    nodes.iter().map(|n| {
        let (lo, hi) = node_cells(n, cell);
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
