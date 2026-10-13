//! The slab: a graph of cells in 3D.
//!
//! The map file (`data/slab_01.json`) draws each face as rows of characters, so agents can read
//! and edit a level as text. The top face is a grid seen from above: row 0 is the north rim
//! (−Z), column 0 the west rim (−X). Each cliff face hangs below one rim: its row 0 touches the
//! rim and rows go down; its column index runs along the rim in the same direction as the top
//! face's (north/south cliffs: column = top column, +X; east/west cliffs: column = top row, +Z).
//!
//! ```text
//! top:   #  fragment (stones go here)   .  fissure (a lane)    j  jump point (cracked ledge)
//!        G  graveyard (the goal)        S  gorge (ground spawn) x  void (broken away)
//! cliff: -  rock    |  climbable lane   #  ledge (stones go here)
//!        B  cliff base (climber spawn)  x  void
//! ```
//!
//! Cells are linked to their four neighbours on the same face, and a cliff's row-0 lane cells to
//! the top-face cell on the rim above them. Two `j` cells two apart in a straight line, with an
//! unwalkable cell between, form a lane-jump pair.

use specula_core::Json;

use crate::error::{DataError, Obj};
use crate::fixed::{mm, Pos};

/// Index into [`Map::cells`]. Ids are assigned face by face (top, north, east, south, west),
/// row-major, so they are stable for a given map file and order every tie-break.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellId(pub u16);

impl CellId {
    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Face {
    Top,
    North,
    East,
    South,
    West,
}

impl Face {
    pub const ALL: [Face; 5] = [Face::Top, Face::North, Face::East, Face::South, Face::West];

    pub fn name(self) -> &'static str {
        match self {
            Face::Top => "top",
            Face::North => "north",
            Face::East => "east",
            Face::South => "south",
            Face::West => "west",
        }
    }

    pub fn from_name(s: &str) -> Option<Face> {
        Face::ALL.into_iter().find(|f| f.name() == s)
    }

    pub fn is_cliff(self) -> bool {
        self != Face::Top
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CellKind {
    /// Top-face rock between fissures: stones are carved here.
    Fragment,
    /// A lane on the top face. Walls go here.
    Fissure,
    /// A lane cell with a cracked ledge to a neighbouring lane (see [`Map::jump_partners`]).
    Jump,
    Graveyard,
    /// A top-face rim cell where ground enemies emerge.
    Gorge,
    Void,
    /// Cliff rock: nothing happens here.
    Rock,
    /// A climbable cliff lane (climbers only).
    Climb,
    /// A buildable cliff ledge.
    Ledge,
    /// The foot of a cliff lane, where climbers spawn.
    Base,
}

impl CellKind {
    fn from_top(c: char) -> Option<CellKind> {
        Some(match c {
            '#' => CellKind::Fragment,
            '.' => CellKind::Fissure,
            'j' => CellKind::Jump,
            'G' => CellKind::Graveyard,
            'S' => CellKind::Gorge,
            'x' => CellKind::Void,
            _ => return None,
        })
    }

    fn from_cliff(c: char) -> Option<CellKind> {
        Some(match c {
            '-' => CellKind::Rock,
            '|' => CellKind::Climb,
            '#' => CellKind::Ledge,
            'B' => CellKind::Base,
            'x' => CellKind::Void,
            _ => return None,
        })
    }

    /// The character the map file uses for this kind.
    pub fn glyph(self) -> char {
        match self {
            CellKind::Fragment | CellKind::Ledge => '#',
            CellKind::Fissure => '.',
            CellKind::Jump => 'j',
            CellKind::Graveyard => 'G',
            CellKind::Gorge => 'S',
            CellKind::Void => 'x',
            CellKind::Rock => '-',
            CellKind::Climb => '|',
            CellKind::Base => 'B',
        }
    }

    /// Ground enemies can stand here (`climber` adds cliff lanes).
    pub fn walkable(self, climber: bool) -> bool {
        match self {
            CellKind::Fissure | CellKind::Jump | CellKind::Graveyard | CellKind::Gorge => true,
            CellKind::Climb | CellKind::Base => climber,
            _ => false,
        }
    }

    pub fn buildable(self) -> bool {
        matches!(self, CellKind::Fragment | CellKind::Ledge)
    }

    /// Low walls go on top-face lanes only **(proposal)**: walls on cliffs would hide the
    /// reroute from a player looking at the top.
    pub fn wallable(self) -> bool {
        matches!(self, CellKind::Fissure | CellKind::Jump)
    }

    pub fn is_climb(self) -> bool {
        matches!(self, CellKind::Climb | CellKind::Base)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub id: CellId,
    pub face: Face,
    pub col: u8,
    pub row: u8,
    /// Centre of the cell's surface, in mm.
    pub pos: Pos,
    pub kind: CellKind,
}

impl Cell {
    /// Metres, for presentation.
    pub fn world_pos_f32(&self) -> [f32; 3] {
        self.pos.to_f32()
    }
}

/// A walkable edge. `cost` is its length in mm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Link {
    pub to: CellId,
    pub cost: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceGrid {
    pub face: Face,
    pub cols: u8,
    pub rows: u8,
    first: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spawn {
    pub name: String,
    pub cell: CellId,
    /// Cliff bases admit climbers only; gorges admit every ground enemy.
    pub climbers_only: bool,
}

/// The flyers' route: a square ring of waypoints outside the rim, plus the dive point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Air {
    pub altitude: i32,
    pub ring: Vec<Pos>,
    /// Ring indices where flyers appear.
    pub entries: Vec<u16>,
    /// Where a flyer ends its dive: above the graveyard's centre.
    pub dive_target: Pos,
    /// Half the ring's side, in mm.
    pub ring_half: i32,
}

#[derive(Clone, Debug)]
pub struct Map {
    pub name: String,
    pub cell_mm: i32,
    pub cliff_row_mm: i32,
    /// Half the top face's extent along X and Z.
    pub half_x: i32,
    pub half_z: i32,
    faces: Vec<FaceGrid>,
    cells: Vec<Cell>,
    links: Vec<Vec<Link>>,
    jumps: Vec<Vec<CellId>>,
    pub spawns: Vec<Spawn>,
    pub graveyard: Vec<CellId>,
    pub graveyard_centre: Pos,
    pub air: Air,
}

const MAP_KEYS: [&str; 8] = ["name", "notes", "cell_m", "cliff_row_m", "top", "cliffs", "spawns", "air"];

impl Map {
    /// Parses and validates a map file. `file` names it in errors.
    pub fn parse(text: &str, file: &str) -> Result<Map, DataError> {
        let json = Json::parse(text).map_err(|e| DataError::new(file, format!("byte {}", e.offset), e.message))?;
        let root = Obj::new(&json, file, "")?;
        root.only(&MAP_KEYS)?;
        let name = root.str("name")?.to_owned();
        let cell_mm = mm(root.num("cell_m", 0.5, 20.0)?);
        let cliff_row_mm = mm(root.num("cliff_row_m", 0.5, 20.0)?);

        let top = grid(&root, "top", 2, 64)?;
        let (top_rows, top_cols) = (top.len(), top[0].len());
        let half_x = cell_mm * top_cols as i32 / 2;
        let half_z = cell_mm * top_rows as i32 / 2;
        let col_x = |c: usize| -half_x + c as i32 * cell_mm + cell_mm / 2;
        let row_z = |r: usize| -half_z + r as i32 * cell_mm + cell_mm / 2;

        let mut faces = Vec::new();
        let mut cells = Vec::new();
        let mut add_face = |face: Face, g: &[Vec<char>], path: &str| -> Result<(), DataError> {
            let first = cells.len();
            for (r, line) in g.iter().enumerate() {
                for (c, &ch) in line.iter().enumerate() {
                    let kind = if face == Face::Top { CellKind::from_top(ch) } else { CellKind::from_cliff(ch) };
                    let kind = kind.ok_or_else(|| {
                        let legend = if face == Face::Top { "# . j G S x" } else { "- | # B x" };
                        DataError::new(file, format!("{path}[{r}][{c}]"), format!("unknown cell '{ch}' (legend: {legend})"))
                    })?;
                    let y = -(r as i32 * cliff_row_mm + cliff_row_mm / 2);
                    let pos = match face {
                        Face::Top => Pos::new(col_x(c), 0, row_z(r)),
                        Face::North => Pos::new(col_x(c), y, -half_z),
                        Face::South => Pos::new(col_x(c), y, half_z),
                        Face::East => Pos::new(half_x, y, row_z(c)),
                        Face::West => Pos::new(-half_x, y, row_z(c)),
                    };
                    if cells.len() >= usize::from(u16::MAX) {
                        return Err(DataError::new(file, path, "too many cells"));
                    }
                    let id = CellId(cells.len() as u16);
                    cells.push(Cell { id, face, col: c as u8, row: r as u8, pos, kind });
                }
            }
            faces.push(FaceGrid { face, cols: g[0].len() as u8, rows: g.len() as u8, first: first as u16 });
            Ok(())
        };
        add_face(Face::Top, &top, "top")?;
        if let Some(cliffs_json) = root.get("cliffs") {
            let cliffs = Obj::new(cliffs_json, file, "cliffs")?;
            cliffs.only(&["north", "east", "south", "west"])?;
            for face in [Face::North, Face::East, Face::South, Face::West] {
                if cliffs.get(face.name()).is_none() {
                    continue;
                }
                let g = grid(&cliffs, face.name(), 1, 32)?;
                let along = if matches!(face, Face::North | Face::South) { top_cols } else { top_rows };
                if g[0].len() != along {
                    return Err(cliffs.err(face.name(), format!("must be {along} columns wide (the length of its rim)")));
                }
                add_face(face, &g, &cliffs.path(face.name()))?;
            }
        }

        let mut map = Map {
            name,
            cell_mm,
            cliff_row_mm,
            half_x,
            half_z,
            links: vec![Vec::new(); cells.len()],
            jumps: vec![Vec::new(); cells.len()],
            faces,
            cells,
            spawns: Vec::new(),
            graveyard: Vec::new(),
            graveyard_centre: Pos::default(),
            air: Air { altitude: 0, ring: Vec::new(), entries: Vec::new(), dive_target: Pos::default(), ring_half: 0 },
        };
        map.link(file)?;
        map.find_jumps(file)?;
        map.read_spawns(&root)?;
        map.graveyard = map.cells.iter().filter(|c| c.kind == CellKind::Graveyard).map(|c| c.id).collect();
        if map.graveyard.is_empty() {
            return Err(root.err("top", "no graveyard cells ('G')"));
        }
        let n = map.graveyard.len() as i64;
        let sum = map.graveyard.iter().fold([0i64; 3], |s, &g| {
            let p = map.cell(g).pos;
            [s[0] + i64::from(p.x), s[1] + i64::from(p.y), s[2] + i64::from(p.z)]
        });
        map.graveyard_centre = Pos::new((sum[0] / n) as i32, (sum[1] / n) as i32, (sum[2] / n) as i32);
        map.read_air(&root)?;
        map.check_reachable(&root)?;
        Ok(map)
    }

    fn link(&mut self, file: &str) -> Result<(), DataError> {
        let mut pairs = Vec::new();
        for g in &self.faces {
            for r in 0..g.rows {
                for c in 0..g.cols {
                    let a = self.id_in(g, c, r);
                    if !self.cell(a).kind.walkable(true) {
                        continue;
                    }
                    for (dc, dr) in [(1u8, 0u8), (0, 1)] {
                        if c + dc < g.cols && r + dr < g.rows {
                            let b = self.id_in(g, c + dc, r + dr);
                            if self.cell(b).kind.walkable(true) {
                                pairs.push((a, b));
                            }
                        }
                    }
                }
            }
        }
        // Rim links: a cliff lane's top cell meets the top-face cell on the rim above it.
        let top = self.faces[0];
        for g in self.faces[1..].iter().copied() {
            for c in 0..g.cols {
                let a = self.id_in(&g, c, 0);
                if !self.cell(a).kind.walkable(true) {
                    continue;
                }
                let (tc, tr) = match g.face {
                    Face::North => (c, 0),
                    Face::South => (c, top.rows - 1),
                    Face::East => (top.cols - 1, c),
                    _ => (0, c),
                };
                let b = self.id_in(&top, tc, tr);
                if !self.cell(b).kind.walkable(false) {
                    return Err(DataError::new(
                        file,
                        format!("cliffs.{}[0][{c}]", g.face.name()),
                        format!("cliff lane meets the rim at {}, which is not a walkable top cell", self.cell_name(b)),
                    ));
                }
                pairs.push((a, b));
            }
        }
        for (a, b) in pairs {
            let cost = self.cell(a).pos.dist(self.cell(b).pos);
            self.links[a.index()].push(Link { to: b, cost });
            self.links[b.index()].push(Link { to: a, cost });
        }
        for l in &mut self.links {
            l.sort_by_key(|l| l.to);
        }
        Ok(())
    }

    fn find_jumps(&mut self, file: &str) -> Result<(), DataError> {
        for g in self.faces.clone() {
            for r in 0..g.rows {
                for c in 0..g.cols {
                    let a = self.id_in(&g, c, r);
                    if self.cell(a).kind != CellKind::Jump {
                        continue;
                    }
                    for (dc, dr) in [(0i32, -2i32), (2, 0), (0, 2), (-2, 0)] {
                        let (bc, br) = (i32::from(c) + dc, i32::from(r) + dr);
                        if bc < 0 || br < 0 || bc >= i32::from(g.cols) || br >= i32::from(g.rows) {
                            continue;
                        }
                        let b = self.id_in(&g, bc as u8, br as u8);
                        let mid = self.id_in(&g, (i32::from(c) + dc / 2) as u8, (i32::from(r) + dr / 2) as u8);
                        if self.cell(b).kind == CellKind::Jump && !self.cell(mid).kind.walkable(true) {
                            self.jumps[a.index()].push(b);
                        }
                    }
                    if self.jumps[a.index()].is_empty() {
                        return Err(DataError::new(
                            file,
                            format!("{}[{r}][{c}]", g.face.name()),
                            "jump point 'j' needs a partner 'j' two cells away across one unwalkable cell",
                        ));
                    }
                    self.jumps[a.index()].sort();
                }
            }
        }
        Ok(())
    }

    fn read_spawns(&mut self, root: &Obj) -> Result<(), DataError> {
        let spawns = root.obj("spawns")?;
        for (name, v) in spawns.json.as_object().unwrap_or(&[]) {
            let r = v.as_str().ok_or_else(|| spawns.err(name, "expected a cell like \"top:4,19\""))?;
            let cell = self.cell_ref(r).ok_or_else(|| spawns.err(name, format!("no cell {r:?}")))?;
            let kind = self.cell(cell).kind;
            if !matches!(kind, CellKind::Gorge | CellKind::Base) {
                return Err(spawns.err(name, format!("{r} is '{}', not a spawn ('S' or 'B')", kind.glyph())));
            }
            if self.spawns.iter().any(|s| s.cell == cell) {
                return Err(spawns.err(name, format!("{r} already has a name")));
            }
            self.spawns.push(Spawn { name: name.clone(), cell, climbers_only: kind == CellKind::Base });
        }
        for c in &self.cells {
            if matches!(c.kind, CellKind::Gorge | CellKind::Base) && !self.spawns.iter().any(|s| s.cell == c.id) {
                return Err(spawns.err("", format!("spawn cell {} has no name", self.cell_name(c.id))));
            }
        }
        Ok(())
    }

    fn read_air(&mut self, root: &Obj) -> Result<(), DataError> {
        let air = root.obj("air")?;
        air.only(&["altitude_m", "margin_m", "points", "entries", "dive_height_m"])?;
        let altitude = mm(air.num("altitude_m", 0.5, 100.0)?);
        let half = self.half_x.max(self.half_z) + mm(air.num("margin_m", 0.0, 100.0)?);
        let n = air.int("points", 4, 1024)?;
        if n % 4 != 0 {
            return Err(air.err("points", "must be a multiple of 4 (a point on every corner)"));
        }
        let side = 2 * i64::from(half);
        let ring = (0..n)
            .map(|k| {
                let s = k * 4 * side / n;
                let (edge, off) = ((s / side) as i32, (s % side) as i32);
                match edge {
                    0 => Pos::new(-half + off, altitude, -half),
                    1 => Pos::new(half, altitude, -half + off),
                    2 => Pos::new(half - off, altitude, half),
                    _ => Pos::new(-half, altitude, half - off),
                }
            })
            .collect();
        let mut entries = Vec::new();
        for (i, e) in air.arr("entries")?.iter().enumerate() {
            match e.as_i64() {
                Some(v) if (0..n).contains(&v) => entries.push(v as u16),
                _ => return Err(air.err(&format!("entries[{i}]"), format!("expected a ring index in 0..{n}"))),
            }
        }
        if entries.is_empty() {
            return Err(air.err("entries", "needs at least one entry point"));
        }
        let dive = mm(air.num_or("dive_height_m", 1.5, 0.0, 100.0)?);
        self.air = Air { altitude, ring, entries, dive_target: self.graveyard_centre.up(dive), ring_half: half };
        Ok(())
    }

    /// Every spawn must reach the graveyard when no walls stand (walls may cut it later; that's
    /// the breach rule's job).
    fn check_reachable(&self, root: &Obj) -> Result<(), DataError> {
        for climber in [false, true] {
            let mut seen = vec![false; self.cells.len()];
            let mut stack: Vec<CellId> = self.graveyard.clone();
            for g in &stack {
                seen[g.index()] = true;
            }
            while let Some(c) = stack.pop() {
                for l in self.links(c) {
                    if !seen[l.to.index()] && self.cell(l.to).kind.walkable(climber) {
                        seen[l.to.index()] = true;
                        stack.push(l.to);
                    }
                }
            }
            for s in &self.spawns {
                if s.climbers_only == climber && !seen[s.cell.index()] {
                    return Err(root.err(&format!("spawns.{}", s.name), "no path to the graveyard"));
                }
            }
        }
        Ok(())
    }

    fn id_in(&self, g: &FaceGrid, col: u8, row: u8) -> CellId {
        CellId(g.first + u16::from(row) * u16::from(g.cols) + u16::from(col))
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    pub fn cell(&self, id: CellId) -> &Cell {
        &self.cells[id.index()]
    }

    pub fn faces(&self) -> &[FaceGrid] {
        &self.faces
    }

    pub fn face(&self, face: Face) -> Option<&FaceGrid> {
        self.faces.iter().find(|g| g.face == face)
    }

    /// The cell at `col, row` on `face`.
    pub fn at(&self, face: Face, col: u8, row: u8) -> Option<CellId> {
        let g = self.face(face)?;
        (col < g.cols && row < g.rows).then(|| self.id_in(g, col, row))
    }

    /// Walkable neighbours (for climbers; walkers filter out cliff cells), sorted by id.
    pub fn links(&self, id: CellId) -> &[Link] {
        &self.links[id.index()]
    }

    /// The other end of each cracked ledge at a jump point.
    pub fn jump_partners(&self, id: CellId) -> &[CellId] {
        &self.jumps[id.index()]
    }

    /// Every lane-jump pair once, lowest id first.
    pub fn jump_pairs(&self) -> Vec<(CellId, CellId)> {
        let mut v = Vec::new();
        for (i, ps) in self.jumps.iter().enumerate() {
            for &p in ps {
                if (i as u16) < p.0 {
                    v.push((CellId(i as u16), p));
                }
            }
        }
        v
    }

    pub fn spawn(&self, name: &str) -> Option<&Spawn> {
        self.spawns.iter().find(|s| s.name == name)
    }

    /// Parses `"face:col,row"`, e.g. `"top:10,5"` or `"north:4,2"`.
    pub fn cell_ref(&self, s: &str) -> Option<CellId> {
        let (face, rest) = s.split_once(':')?;
        let (c, r) = rest.split_once(',')?;
        self.at(Face::from_name(face.trim())?, c.trim().parse().ok()?, r.trim().parse().ok()?)
    }

    /// The `"face:col,row"` name of a cell (the inverse of [`Map::cell_ref`]).
    pub fn cell_name(&self, id: CellId) -> String {
        let c = self.cell(id);
        format!("{}:{},{}", c.face.name(), c.col, c.row)
    }

    /// How far a point on `face` is from the rim it shares with `other`, or `None` when the two
    /// faces share no rim (two cliffs, or the same face). This is what "near an edge" means for
    /// a stone's range across faces.
    pub fn rim_distance(&self, pos: Pos, face: Face, other: Face) -> Option<i32> {
        match (face, other) {
            (Face::Top, Face::North) => Some(pos.z + self.half_z),
            (Face::Top, Face::South) => Some(self.half_z - pos.z),
            (Face::Top, Face::East) => Some(self.half_x - pos.x),
            (Face::Top, Face::West) => Some(pos.x + self.half_x),
            (f, Face::Top) if f.is_cliff() => Some(-pos.y),
            _ => None,
        }
    }

    /// The map as text (see [`Map::ascii_with`]).
    pub fn ascii(&self) -> String {
        self.ascii_with(|_| None)
    }

    /// The slab unfolded like a box net, with `overlay` replacing any cell's character. The north
    /// cliff hangs above the top face, the west and east cliffs beside it (their rim column
    /// touching the top face), the south cliff below. Row numbers on the left are top-face rows,
    /// which are also the column index of the west and east cliffs.
    pub fn ascii_with(&self, overlay: impl Fn(CellId) -> Option<char>) -> String {
        let top = self.faces[0];
        let rows_of = |f: Face| self.face(f).map_or(0, |g| usize::from(g.rows));
        let (wr, er) = (rows_of(Face::West), rows_of(Face::East));
        let glyph = |id: CellId| overlay(id).unwrap_or(self.cell(id).kind.glyph());
        let mut out = String::new();
        let pad = " ".repeat(4 + wr + usize::from(wr > 0));
        out.push_str(&format!("{} ({} m cells, cliffs {} m per row)\n", self.name, self.cell_mm / 1000, self.cliff_row_mm as f32 / 1000.0));
        for digit in [10, 1] {
            out.push_str(&pad);
            out.extend((0..top.cols).map(|c| char::from(b'0' + (c / digit % 10))));
            out.push('\n');
        }
        let cliff_line = |out: &mut String, face: Face, r: u8| {
            out.push_str(&format!("{}{r:<2}", face.name()[..1].to_uppercase()));
            out.push_str(&" ".repeat(pad.len() - 3));
            out.extend((0..top.cols).map(|c| glyph(self.at(face, c, r).expect("cliff cell"))));
            out.push('\n');
        };
        for r in (0..rows_of(Face::North) as u8).rev() {
            cliff_line(&mut out, Face::North, r);
        }
        for j in 0..top.rows {
            out.push_str(&format!("{j:>2}  "));
            for r in (0..wr as u8).rev() {
                out.push(glyph(self.at(Face::West, j, r).expect("west cell")));
            }
            if wr > 0 {
                out.push(' ');
            }
            out.extend((0..top.cols).map(|c| glyph(self.id_in(&top, c, j))));
            if er > 0 {
                out.push(' ');
                out.extend((0..er as u8).map(|r| glyph(self.at(Face::East, j, r).expect("east cell"))));
            }
            out.push('\n');
        }
        for r in 0..rows_of(Face::South) as u8 {
            cliff_line(&mut out, Face::South, r);
        }
        out.push_str("top:   # fragment  . fissure  j jump point  G graveyard  S gorge spawn  x void\n");
        out.push_str("cliff: - rock  | climb lane  # ledge  B cliff-base spawn  (N/S rows and W/E columns count from the rim)\n");
        out
    }
}

/// Reads an array of equal-length strings as a character grid.
fn grid(obj: &Obj, key: &str, min: usize, max: usize) -> Result<Vec<Vec<char>>, DataError> {
    let arr = obj.arr(key)?;
    let rows: Vec<Vec<char>> = arr
        .iter()
        .enumerate()
        .map(|(i, v)| v.as_str().map(|s| s.chars().collect()).ok_or_else(|| obj.err(&format!("{key}[{i}]"), "expected a string")))
        .collect::<Result<_, _>>()?;
    if !(min..=max).contains(&rows.len()) {
        return Err(obj.err(key, format!("needs {min}..={max} rows, got {}", rows.len())));
    }
    let w = rows[0].len();
    if !(1..=64).contains(&w) {
        return Err(obj.err(&format!("{key}[0]"), "rows must be 1..=64 cells wide"));
    }
    if let Some(i) = rows.iter().position(|r| r.len() != w) {
        return Err(obj.err(&format!("{key}[{i}]"), format!("is {} cells wide, row 0 is {w}", rows[i].len())));
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::SLAB_01_JSON;

    fn slab() -> Map {
        Map::parse(SLAB_01_JSON, "slab_01.json").expect("built-in map")
    }

    fn bad(from: &str, to: &str) -> DataError {
        assert!(SLAB_01_JSON.contains(from), "test fixture: {from:?} not in the map");
        Map::parse(&SLAB_01_JSON.replacen(from, to, 1), "slab_01.json").expect_err("should be rejected")
    }

    #[test]
    fn builtin_slab_shape() {
        let m = slab();
        let r = |s: &str| m.cell_ref(s).unwrap_or_else(|| panic!("{s}"));
        assert_eq!(m.faces().len(), 4, "top + north, east, west cliffs");
        assert_eq!(m.cells().len(), 400 + 3 * 120);
        assert_eq!((m.half_x, m.half_z), (20_000, 20_000));
        assert_eq!(m.graveyard.len(), 16);
        assert_eq!(m.graveyard_centre, Pos::new(0, 0, 0));
        assert_eq!(m.spawns.len(), 7);
        assert_eq!(m.spawns.iter().filter(|s| s.climbers_only).count(), 4);
        assert_eq!(m.jump_pairs(), vec![(r("top:4,11"), r("top:4,13")), (r("top:15,12"), r("top:15,14"))]);
        assert_eq!(m.cell(r("top:0,0")).pos, Pos::new(-19_000, 0, -19_000));
        assert_eq!(m.cell(r("north:4,0")).pos, Pos::new(-11_000, -1_250, -20_000));
        assert_eq!(m.cell(r("east:10,5")).pos, Pos::new(20_000, -13_750, 1_000));
        assert_eq!(m.cell(r("west:11,5")).pos, Pos::new(-20_000, -13_750, 3_000));
        // Rim links join each cliff lane to the top face: sqrt(1.25² + 1²) m.
        for (cliff, top) in [("north:4,0", "top:4,0"), ("north:15,0", "top:15,0"), ("east:12,0", "top:19,12"), ("west:11,0", "top:0,11")] {
            assert!(m.links(r(cliff)).iter().any(|l| l.to == r(top) && l.cost == 1600), "{cliff} -> {top}");
        }
        assert_eq!(m.air.ring.len(), 32);
        assert_eq!(m.air.ring[0], Pos::new(-23_000, 4_000, -23_000));
        assert_eq!(m.air.ring[4], Pos::new(0, 4_000, -23_000));
        assert_eq!(m.air.ring[8], Pos::new(23_000, 4_000, -23_000));
        assert_eq!(m.air.ring[24], Pos::new(-23_000, 4_000, 23_000));
        assert_eq!(m.air.dive_target, Pos::new(0, 1_500, 0));
    }

    #[test]
    fn cell_refs_round_trip() {
        let m = slab();
        for c in m.cells() {
            assert_eq!(m.cell_ref(&m.cell_name(c.id)), Some(c.id));
        }
        for bad in ["top:20,0", "top:0", "sky:1,1", "north:4,6", "south:0,0", ""] {
            assert_eq!(m.cell_ref(bad), None, "{bad}");
        }
    }

    #[test]
    fn rim_distance_defines_near_an_edge() {
        let m = slab();
        let d = |s: &str, other: Face| {
            let c = *m.cell(m.cell_ref(s).unwrap());
            m.rim_distance(c.pos, c.face, other)
        };
        assert_eq!(d("top:5,1", Face::North), Some(3_000));
        assert_eq!(d("top:5,2", Face::North), Some(5_000));
        assert_eq!(d("top:18,9", Face::East), Some(3_000));
        assert_eq!(d("top:1,9", Face::West), Some(3_000));
        assert_eq!(d("top:9,19", Face::South), Some(1_000));
        assert_eq!(d("north:7,2", Face::Top), Some(6_250));
        assert_eq!(d("north:4,0", Face::Top), Some(1_250));
        assert_eq!(d("north:4,0", Face::East), None, "two cliffs share no rim");
        assert_eq!(d("top:5,1", Face::Top), None);
    }

    #[test]
    fn walkability_by_class() {
        let m = slab();
        let k = |s: &str| m.cell(m.cell_ref(s).unwrap()).kind;
        assert_eq!(k("top:9,19"), CellKind::Gorge);
        assert!(k("north:4,3").walkable(true) && !k("north:4,3").walkable(false));
        assert!(k("top:9,14").wallable() && !k("top:9,19").wallable() && !k("top:9,9").wallable());
        assert!(k("top:12,9").buildable() && k("north:7,2").buildable() && !k("top:9,14").buildable());
    }

    #[test]
    fn ascii_dump_shows_every_face() {
        let a = slab().ascii();
        for line in [
            "N5         ----B----------B----",
            " 0  ------ xx##.##########.###x ------",
            "11  B||||| ....j..#GGGG#.###### ---|--",
            "12  ------ ######.##.###..j.... ||||--",
            "19  ------ x###S####S#####S##xx ------",
        ] {
            assert!(a.lines().any(|l| l == line), "missing {line:?} in\n{a}");
        }
    }

    #[test]
    fn validation_errors_name_file_field_and_reason() {
        let e = bad("xx##.##########.###x", "xq##.##########.###x");
        assert_eq!((e.file.as_str(), e.field.as_str()), ("slab_01.json", "top[0][1]"));
        assert!(e.reason.contains("unknown cell 'q'"), "{e}");

        let e = bad("xx##.##########.###x", "xx##.##########.###");
        assert_eq!(e.field, "top[1]", "{e}");

        let e = bad("....j..#GGGG#.######", ".......#GGGG#.######");
        assert_eq!(e.field, "top[13][4]", "{e}");
        assert!(e.reason.contains("partner"), "{e}");

        let e = bad("\"gorge_mid\": \"top:9,19\",", "");
        assert!(e.reason.contains("top:9,19 has no name"), "{e}");

        let e = bad("\"gorge_mid\": \"top:9,19\"", "\"gorge_mid\": \"top:9,18\"");
        assert_eq!(e.field, "spawns.gorge_mid");
        assert!(e.reason.contains("not a spawn"), "{e}");

        let e = bad("####.####.#####.###x", "#########.#####.###x");
        assert_eq!(e.field, "spawns.gorge_west");
        assert!(e.reason.contains("no path"), "{e}");

        let e = bad("xx##.##########.###x", "xx######.######.###x");
        assert_eq!(e.field, "cliffs.north[0][4]", "{e}");

        let e = bad("\"cell_m\"", "\"cel_m\"");
        assert!(e.reason.contains("unknown field"), "{e}");

        let e = bad("\"points\": 32", "\"points\": 30");
        assert_eq!(e.field, "air.points");

        let e = bad("{", "[");
        assert!(e.field.starts_with("byte "), "{e}");
    }
}
