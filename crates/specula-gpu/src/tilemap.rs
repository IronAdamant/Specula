//! Tile maps on the CPU: cells ([`TileCell`]), chunked grids ([`TileGrid`]) and the cell texture
//! encoding the tile shader reads (see [`crate::TileChunk`]).
//!
//! A grid is split into [`CHUNK`] × [`CHUNK`] chunks, each shared (`Arc`) until written: a copy of
//! a grid (an undo snapshot, a journal entry) costs a pointer per chunk, an edit copies one chunk,
//! and a renderer that kept the grid it uploaded re-uploads exactly the chunks that are no longer
//! the same allocation ([`TileGrid::same_chunk`]). Chunks are also the culling unit: one
//! [`crate::TileChunk`] draw per non-empty chunk in view.
//!
//! Tiles are numbered from 1 (0 is empty), left to right then top to bottom in the tileset.

use std::sync::Arc;

/// Cells per chunk side.
pub const CHUNK: u32 = 32;
const CHUNK_CELLS: usize = (CHUNK * CHUNK) as usize;

/// [`TileCell::orient`] bits, applied to the tile's texture coordinates in this order: swap x and
/// y (a transpose), then mirror x, then mirror y. Together they give all 8 orientations.
pub const ORIENT_SWAP_XY: u8 = 4;
pub const ORIENT_FLIP_X: u8 = 1;
pub const ORIENT_FLIP_Y: u8 = 2;

/// Most frames of an animated tile.
pub const MAX_TILE_FRAMES: u8 = 32;

/// One cell: which tile, how it is turned, and how it animates. Animation frames are consecutive
/// tiles (`tile`, `tile + 1`, …), each shown for `frame_ticks` sixtieths of a second, looping, in
/// step with [`crate::FrameDesc::time_s`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileCell {
    /// 0 = empty.
    pub tile: u16,
    /// `ORIENT_*` bits.
    pub orient: u8,
    /// 1..=[`MAX_TILE_FRAMES`] (0 counts as 1).
    pub frames: u8,
    /// Sixtieths of a second per frame (0 = still).
    pub frame_ticks: u8,
}

/// The 8 orientations as 2×2 integer matrices acting on tile coordinates centred on the tile
/// (x right, y down): `source = M · display`. Index = orientation bits.
fn orient_matrix(o: u8) -> [i8; 4] {
    // [a, b, c, d] = [[a, b], [c, d]]: (x, y) -> (a x + b y, c x + d y).
    let mut m = [1i8, 0, 0, 1];
    if o & ORIENT_SWAP_XY != 0 {
        m = [0, 1, 1, 0];
    }
    if o & ORIENT_FLIP_X != 0 {
        m = [-m[0], -m[1], m[2], m[3]];
    }
    if o & ORIENT_FLIP_Y != 0 {
        m = [m[0], m[1], -m[2], -m[3]];
    }
    m
}

impl TileCell {
    pub const EMPTY: TileCell = TileCell { tile: 0, orient: 0, frames: 0, frame_ticks: 0 };

    /// Tile `tile`, upright, still.
    pub const fn new(tile: u16) -> TileCell {
        TileCell { tile, orient: 0, frames: 1, frame_ticks: 0 }
    }

    /// Animated: `frames` consecutive tiles from `tile`, `frame_ticks` sixtieths of a second each.
    pub const fn animated(tile: u16, frames: u8, frame_ticks: u8) -> TileCell {
        TileCell { tile, orient: 0, frames, frame_ticks }
    }

    pub fn is_empty(self) -> bool {
        self.tile == 0
    }

    /// The four bytes the tile shader reads: tile (little endian), orientation bits 0..2 and
    /// frames − 1 in bits 3..7, frame ticks.
    pub fn encode(self) -> [u8; 4] {
        let frames = self.frames.clamp(1, MAX_TILE_FRAMES) - 1;
        let [lo, hi] = self.tile.to_le_bytes();
        [lo, hi, (self.orient & 7) | (frames << 3), self.frame_ticks]
    }

    pub fn decode(b: [u8; 4]) -> TileCell {
        TileCell { tile: u16::from_le_bytes([b[0], b[1]]), orient: b[2] & 7, frames: (b[2] >> 3) + 1, frame_ticks: b[3] }
    }

    /// Turned `quarter_turns` × 90° clockwise (as seen on a map whose rows go down the screen).
    pub fn rotated_cw(self, quarter_turns: u32) -> TileCell {
        let mut m = orient_matrix(self.orient & 7);
        for _ in 0..quarter_turns % 4 {
            // The content at display d moves to R·d (R: (x, y) -> (−y, x), clockwise with y down),
            // so source = M · R⁻¹ · d' with R⁻¹: (x, y) -> (y, −x).
            m = [-m[1], m[0], -m[3], m[2]];
        }
        let orient = (0u8..8).find(|&o| orient_matrix(o) == m).expect("the 8 orientations form a group");
        TileCell { orient, ..self }
    }

    /// Mirrored left-right as displayed.
    pub fn mirrored_x(self) -> TileCell {
        let m = orient_matrix(self.orient & 7);
        // source = M · F · d with F: (x, y) -> (−x, y).
        let m = [-m[0], m[1], -m[2], m[3]];
        TileCell { orient: (0u8..8).find(|&o| orient_matrix(o) == m).expect("group"), ..self }
    }

    /// The tile shown at `time_s` (what the shader computes).
    pub fn tile_at(self, time_s: f64) -> u16 {
        let frames = self.frames.clamp(1, MAX_TILE_FRAMES) as u64;
        if self.tile == 0 || frames == 1 || self.frame_ticks == 0 || time_s.is_nan() || time_s <= 0.0 {
            return self.tile;
        }
        let step = (time_s * 60.0 / self.frame_ticks as f64) as u64;
        self.tile.wrapping_add((step % frames) as u16)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Block {
    cells: Vec<TileCell>,
    /// Non-empty cells.
    filled: u32,
}

/// A `width` × `height` grid of cells (column `x`, row `y`, row 0 at the top), stored in shared
/// chunks.
#[derive(Clone, Debug)]
pub struct TileGrid {
    width: u32,
    height: u32,
    chunks: Vec<Arc<Block>>,
}

impl PartialEq for TileGrid {
    fn eq(&self, other: &TileGrid) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.chunks.iter().zip(&other.chunks).all(|(a, b)| Arc::ptr_eq(a, b) || a == b)
    }
}

impl Eq for TileGrid {}

impl TileGrid {
    /// An empty grid. Every chunk shares one empty block until written.
    pub fn new(width: u32, height: u32) -> TileGrid {
        let empty = Arc::new(Block { cells: vec![TileCell::EMPTY; CHUNK_CELLS], filled: 0 });
        let n = (width.div_ceil(CHUNK) * height.div_ceil(CHUNK)) as usize;
        TileGrid { width, height, chunks: vec![empty; n] }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Chunks across and down.
    pub fn chunks_x(&self) -> u32 {
        self.width.div_ceil(CHUNK)
    }

    pub fn chunks_y(&self) -> u32 {
        self.height.div_ceil(CHUNK)
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    fn locate(&self, x: u32, y: u32) -> Option<(usize, usize)> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let chunk = ((y / CHUNK) * self.chunks_x() + x / CHUNK) as usize;
        let cell = ((y % CHUNK) * CHUNK + x % CHUNK) as usize;
        Some((chunk, cell))
    }

    /// The cell at (`x`, `y`); empty outside the grid.
    pub fn get(&self, x: u32, y: u32) -> TileCell {
        self.locate(x, y).map_or(TileCell::EMPTY, |(c, i)| self.chunks[c].cells[i])
    }

    /// Writes a cell. Returns false (and changes nothing) outside the grid or when the cell already
    /// holds `cell`. Copies the chunk first if it is shared.
    pub fn set(&mut self, x: u32, y: u32, cell: TileCell) -> bool {
        let Some((c, i)) = self.locate(x, y) else { return false };
        // Normalized so that equal-looking cells compare (and hash) equal.
        let cell = if cell.tile == 0 { TileCell::EMPTY } else { TileCell { frames: cell.frames.clamp(1, MAX_TILE_FRAMES), orient: cell.orient & 7, ..cell } };
        if self.chunks[c].cells[i] == cell {
            return false;
        }
        let block = Arc::make_mut(&mut self.chunks[c]);
        let was = block.cells[i];
        block.cells[i] = cell;
        match (was.is_empty(), cell.is_empty()) {
            (true, false) => block.filled += 1,
            (false, true) => block.filled -= 1,
            _ => {}
        }
        true
    }

    /// Cells of chunk `i`: x0, y0, width, height (clipped to the grid).
    pub fn chunk_region(&self, i: usize) -> [u32; 4] {
        let cx = self.chunks_x().max(1);
        let (x0, y0) = ((i as u32 % cx) * CHUNK, (i as u32 / cx) * CHUNK);
        [x0, y0, CHUNK.min(self.width - x0), CHUNK.min(self.height - y0)]
    }

    /// Non-empty cells in chunk `i`.
    pub fn chunk_filled(&self, i: usize) -> u32 {
        self.chunks.get(i).map_or(0, |b| b.filled)
    }

    /// Non-empty cells in the grid.
    pub fn filled(&self) -> u64 {
        self.chunks.iter().map(|b| b.filled as u64).sum()
    }

    /// Chunk `i` is the very same allocation in both grids (so its cells are unchanged since
    /// `other` was copied from this grid, or this one from it). Grids of different sizes share
    /// nothing.
    pub fn same_chunk(&self, other: &TileGrid, i: usize) -> bool {
        self.width == other.width
            && self.height == other.height
            && matches!((self.chunks.get(i), other.chunks.get(i)), (Some(a), Some(b)) if Arc::ptr_eq(a, b))
    }

    /// The cell texture bytes of `region` (x0, y0, width, height; clipped to the grid), row by
    /// row, appended to `out` (cleared first).
    pub fn encode_region(&self, region: [u32; 4], out: &mut Vec<u8>) {
        out.clear();
        let [x0, y0, w, h] = region;
        let (x1, y1) = ((x0 + w).min(self.width), (y0 + h).min(self.height));
        out.reserve((x1.saturating_sub(x0) * y1.saturating_sub(y0) * 4) as usize);
        for y in y0..y1 {
            for x in x0..x1 {
                out.extend_from_slice(&self.get(x, y).encode());
            }
        }
    }

    /// The whole cell texture (`width` × `height` RGBA8, top row first).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode_region([0, 0, self.width, self.height], &mut out);
        out
    }

    /// Every non-empty cell as (x, y, cell), row by row.
    pub fn cells(&self) -> impl Iterator<Item = (u32, u32, TileCell)> + '_ {
        (0..self.height).flat_map(move |y| (0..self.width).map(move |x| (x, y, self.get(x, y)))).filter(|c| !c.2.is_empty())
    }

    /// Approximate heap bytes, each chunk divided among the grids sharing it.
    pub fn heap_bytes(&self) -> usize {
        let block = std::mem::size_of::<Block>() + CHUNK_CELLS * std::mem::size_of::<TileCell>() + 32;
        self.chunks.len() * std::mem::size_of::<Arc<Block>>()
            + self.chunks.iter().map(|c| block / Arc::strong_count(c).max(1)).sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_round_trip_through_their_bytes() {
        for cell in [
            TileCell::EMPTY,
            TileCell::new(1),
            TileCell::new(65_535),
            TileCell { tile: 300, orient: 7, frames: 32, frame_ticks: 255 },
            TileCell::animated(9, 4, 10),
        ] {
            let back = TileCell::decode(cell.encode());
            assert_eq!(back.tile, cell.tile);
            assert_eq!(back.orient, cell.orient);
            assert_eq!(back.frames, cell.frames.max(1));
            assert_eq!(back.frame_ticks, cell.frame_ticks);
        }
        assert_eq!(TileCell { tile: 258, orient: 5, frames: 3, frame_ticks: 6 }.encode(), [2, 1, 5 | (2 << 3), 6]);
        assert_eq!(TileCell { frames: 200, ..TileCell::new(1) }.encode()[2] >> 3, 31, "frames clamp to 32");
    }

    #[test]
    fn four_quarter_turns_and_two_mirrors_are_identity() {
        for o in 0..8u8 {
            let c = TileCell { orient: o, ..TileCell::new(5) };
            assert_eq!(c.rotated_cw(4).orient, o);
            assert_eq!(c.rotated_cw(1).rotated_cw(3).orient, o);
            assert_eq!(c.mirrored_x().mirrored_x().orient, o);
        }
        // Distinct orientations per quarter turn of an upright tile.
        let turns: Vec<u8> = (0..4).map(|q| TileCell::new(1).rotated_cw(q).orient).collect();
        assert_eq!(turns, [0, ORIENT_SWAP_XY | ORIENT_FLIP_Y, ORIENT_FLIP_X | ORIENT_FLIP_Y, ORIENT_SWAP_XY | ORIENT_FLIP_X]);
        assert_eq!(TileCell::new(1).mirrored_x().orient, ORIENT_FLIP_X);
    }

    #[test]
    fn animated_tiles_step_through_their_frames() {
        let c = TileCell::animated(10, 3, 30); // half a second per frame
        let at: Vec<u16> = [0.0, 0.49, 0.5, 1.0, 1.5, 1.99, -1.0, f64::NAN].map(|t| c.tile_at(t)).to_vec();
        assert_eq!(at, [10, 10, 11, 12, 10, 10, 10, 10]);
        assert_eq!(TileCell::new(4).tile_at(100.0), 4);
    }

    #[test]
    fn grids_share_chunks_until_written() {
        let mut g = TileGrid::new(70, 40);
        assert_eq!((g.chunks_x(), g.chunks_y(), g.chunk_count()), (3, 2, 6));
        assert_eq!(g.chunk_region(2), [64, 0, 6, 32]);
        assert_eq!(g.chunk_region(5), [64, 32, 6, 8]);
        let copy = g.clone();
        assert!(g.set(65, 33, TileCell::new(3)));
        assert!(!g.set(65, 33, TileCell::new(3)), "unchanged");
        assert!(!g.set(70, 0, TileCell::new(3)), "outside");
        assert_eq!(g.get(65, 33), TileCell::new(3));
        assert_eq!(copy.get(65, 33), TileCell::EMPTY, "the copy is untouched");
        let changed: Vec<usize> = (0..g.chunk_count()).filter(|&i| !g.same_chunk(&copy, i)).collect();
        assert_eq!(changed, [5], "only the written chunk differs");
        assert_eq!((g.chunk_filled(5), g.filled()), (1, 1));
        assert!(g.set(65, 33, TileCell::EMPTY));
        assert_eq!(g.filled(), 0);
        assert_eq!(g, copy, "equal by content");
        // Normalized: a cell with frames 0 equals one with frames 1.
        let mut a = TileGrid::new(4, 4);
        a.set(1, 1, TileCell { frames: 0, ..TileCell::new(2) });
        assert_eq!(a.get(1, 1), TileCell::new(2));
    }

    #[test]
    fn encoding_is_rows_top_first() {
        let mut g = TileGrid::new(3, 2);
        g.set(2, 0, TileCell::new(7));
        g.set(0, 1, TileCell { orient: ORIENT_FLIP_X, ..TileCell::new(1) });
        let bytes = g.encode();
        assert_eq!(bytes.len(), 3 * 2 * 4);
        assert_eq!(&bytes[8..12], &[7, 0, 0, 0]);
        assert_eq!(&bytes[12..16], &[1, 0, ORIENT_FLIP_X, 0]);
        let mut region = Vec::new();
        g.encode_region([1, 0, 5, 5], &mut region);
        assert_eq!(region.len(), 2 * 2 * 4, "clipped to the grid");
        let cells: Vec<(u32, u32, u16)> = g.cells().map(|(x, y, c)| (x, y, c.tile)).collect();
        assert_eq!(cells, [(2, 0, 7), (0, 1, 1)]);
    }
}
