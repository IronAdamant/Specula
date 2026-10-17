//! Specula navigation (*via*, "way"): deterministic, CPU-only, zero-dependency
//! (docs/design/NAVIGATION.md).
//!
//! - **Navmesh generation** ([`NavMesh`]): triangles are voxelised per tile into spans, the open
//!   space above walkable spans is linked where an agent can step ([`Config::max_step`]) and
//!   stand ([`Config::agent_height`]), eroded by [`Config::agent_radius`], and merged into
//!   axis-aligned rectangles with four corner heights (greedy meshing). Rectangles link through
//!   portals inside a tile and across tile edges, so a rebuilt tile relinks with its neighbours
//!   only.
//! - **Queries** ([`query`]): nearest polygon, A* over polygons with string pulling (the funnel
//!   algorithm), moving along the surface, seeded random points.
//! - **Crowds** ([`crowd`]): agents following corridors, steering, sampled velocity avoidance,
//!   budgeted repathing.
//!
//! **Determinism (F2):** only IEEE `+ - * /`, `sqrt` and `specula_core::dmath`; ordered
//! containers; ties broken by stable ids; links sorted canonically, so a navmesh built tile by tile
//! equals one built at once. [`NavMesh::hash`] covers every polygon and link.
//!
//! **Bounded work (F1/F3):** tiles, columns, spans, polygons, input triangles, path nodes and
//! agents are capped; every cap that bites is a structured [`NavError`] (code `budget`).

#![forbid(unsafe_code)]

pub mod build;
pub mod crowd;
pub mod mesh;
pub mod query;

pub use crowd::{AgentParams, Crowd, CrowdAgent, CrowdSettings};
pub use mesh::ResolvedLink;
pub use mesh::{Link, NavMesh, Poly, PolyRef, TileStats};
pub use query::{PathResult, PathStatus};
pub use crowd::AgentStatus;

use specula_core::math::Vec3;
use specula_core::Json;

/// A structured navigation error: a machine-readable `code`, a message, and details.
#[derive(Clone, Debug, PartialEq)]
pub struct NavError {
    pub code: &'static str,
    pub message: String,
    pub details: Vec<(&'static str, Json)>,
}

impl NavError {
    pub fn new(code: &'static str, message: impl Into<String>) -> NavError {
        NavError { code, message: message.into(), details: Vec::new() }
    }

    pub fn with(mut self, key: &'static str, value: impl Into<Json>) -> NavError {
        self.details.push((key, value.into()));
        self
    }

    pub fn to_json(&self) -> Json {
        let mut fields = vec![("error", Json::from(self.code)), ("message", Json::from(self.message.as_str()))];
        fields.extend(self.details.iter().map(|(k, v)| (*k, v.clone())));
        Json::obj(fields)
    }
}

impl std::fmt::Display for NavError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for NavError {}

/// Most tiles one navmesh has.
pub const MAX_TILES: usize = 4096;
/// Most columns (cells) one navmesh covers, and most a full build rasterises (tiles with their
/// borders).
pub const MAX_COLUMNS: u64 = 16 * 1024 * 1024;
/// Widest a tile's grid with its border may be (a rebuild's work is about its square).
pub const MAX_TILE_SIDE: u64 = 96;
/// Most solid spans in one column (more are dropped, reported as a budget error).
pub const MAX_SPANS_PER_COLUMN: usize = 64;
/// Most polygons in one tile.
pub const MAX_POLYS_PER_TILE: usize = 16384;
/// Most input triangles one tile build takes.
pub const MAX_TILE_TRIANGLES: usize = 400_000;
/// Height range in cell-height steps (heights are stored as `u16`).
pub const MAX_HEIGHT_STEPS: f32 = 60_000.0;

/// What a navmesh is built for: the grid and the agent it must fit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    /// Width of a column, metres.
    pub cell_size: f32,
    /// Height resolution, metres.
    pub cell_height: f32,
    pub agent_radius: f32,
    pub agent_height: f32,
    /// Highest step an agent climbs (the character controller's `step_height`).
    pub max_step: f32,
    /// Steepest walkable slope, degrees.
    pub max_slope_deg: f32,
    /// Columns per tile side.
    pub tile_cells: u32,
    /// The volume the navmesh covers (fixed once built, so the grid never moves).
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            cell_size: 0.15,
            cell_height: 0.05,
            agent_radius: 0.3,
            agent_height: 1.8,
            max_step: 0.3,
            max_slope_deg: 45.0,
            tile_cells: 48,
            bounds_min: Vec3::new(-16.0, -2.0, -16.0),
            bounds_max: Vec3::new(16.0, 8.0, 16.0),
        }
    }
}

fn finite(v: Vec3) -> bool {
    v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
}

impl Config {
    /// Checks every field; errors name the field and the allowed range.
    pub fn validate(&self) -> Result<(), NavError> {
        self.validate_fields()?;
        self.validate_work()
    }

    /// Each field in its range, the bounds finite and not too tall: what must hold before any
    /// grid numbers are derived from the config.
    pub fn validate_fields(&self) -> Result<(), NavError> {
        let range = |field: &'static str, v: f32, lo: f32, hi: f32| -> Result<(), NavError> {
            if v.is_finite() && v >= lo && v <= hi {
                Ok(())
            } else {
                Err(NavError::new("invalid_value", format!("'{field}' must be {lo}..{hi} (got {v})")).with("field", field))
            }
        };
        range("cell_size", self.cell_size, 0.02, 2.0)?;
        range("cell_height", self.cell_height, 0.01, 1.0)?;
        range("agent_radius", self.agent_radius, 0.0, 10.0)?;
        range("agent_height", self.agent_height, 0.1, 50.0)?;
        range("max_step", self.max_step, 0.0, 10.0)?;
        range("max_slope_deg", self.max_slope_deg, 0.0, 89.0)?;
        if !(8..=256).contains(&self.tile_cells) {
            return Err(NavError::new("invalid_value", format!("'tile_cells' must be 8..256, and the tile with its border (radius + 3 columns a side) at most {MAX_TILE_SIDE} columns")).with("field", "tile_cells"));
        }
        if !finite(self.bounds_min) || !finite(self.bounds_max) {
            return Err(NavError::new("invalid_value", "'bounds' must be finite").with("field", "bounds"));
        }
        let size = self.bounds_max - self.bounds_min;
        if size.x <= 0.0 || size.y <= 0.0 || size.z <= 0.0 {
            return Err(NavError::new("invalid_value", "'bounds' max must exceed min on every axis").with("field", "bounds"));
        }
        if size.y / self.cell_height > MAX_HEIGHT_STEPS {
            return Err(NavError::new(
                "budget",
                format!("the bounds are {:.1} m tall: at most {} cell heights ({:.0} m at this cell_height)", size.y, MAX_HEIGHT_STEPS, MAX_HEIGHT_STEPS * self.cell_height),
            )
            .with("field", "bounds"));
        }
        Ok(())
    }

    /// The grid's size and each tile's work bounded (after [`Config::validate_fields`]).
    pub fn validate_work(&self) -> Result<(), NavError> {
        let d = Derived::of(self);
        let columns = u64::from(d.cells_x) * u64::from(d.cells_z);
        if columns > MAX_COLUMNS {
            return Err(NavError::new(
                "budget",
                format!("{} x {} columns is more than {MAX_COLUMNS}: raise cell_size or shrink the bounds", d.cells_x, d.cells_z),
            )
            .with("field", "cell_size")
            .with("columns", columns));
        }
        // Each tile is built with an erosion border of radius + 3 columns on every side: keep it
        // no wider than the tile, and the whole build's columns bounded.
        if d.border > self.tile_cells {
            return Err(NavError::new(
                "budget",
                format!(
                    "agent_radius {} m is {} columns at this cell_size; tiles need at least radius + 3 = {} columns (tile_cells is {}): raise tile_cells or cell_size",
                    self.agent_radius, d.radius_cells, d.border, self.tile_cells
                ),
            )
            .with("field", "tile_cells"));
        }
        let tiles = d.tiles_x as usize * d.tiles_z as usize;
        let side = u64::from(self.tile_cells) + 2 * u64::from(d.border);
        if side > MAX_TILE_SIDE {
            return Err(NavError::new(
                "budget",
                format!("a tile with its erosion border is {side} columns across, more than {MAX_TILE_SIDE}: lower tile_cells (or agent_radius / cell_size)"),
            )
            .with("field", "tile_cells"));
        }
        if tiles as u64 * side * side > MAX_COLUMNS {
            return Err(NavError::new("budget", format!("building {tiles} tiles of {side} x {side} columns (with their borders) is too much work: raise cell_size or shrink the bounds"))
                .with("field", "cell_size"));
        }
        if tiles > MAX_TILES {
            return Err(NavError::new("budget", format!("{} x {} tiles is more than {MAX_TILES}: raise tile_cells", d.tiles_x, d.tiles_z))
                .with("field", "tile_cells")
                .with("tiles", tiles));
        }
        Ok(())
    }

    #[cfg(test)]
    fn huge_border() -> Config {
        Config { cell_size: 0.02, tile_cells: 8, agent_radius: 2.0, bounds_min: Vec3::new(0.0, 0.0, 0.0), bounds_max: Vec3::new(10.24, 4.0, 10.24), ..Config::default() }
    }

    pub fn to_json(&self) -> Json {
        let v = |p: Vec3| Json::arr([Json::from(p.x), Json::from(p.y), Json::from(p.z)]);
        Json::obj([
            ("cell_size", Json::from(self.cell_size)),
            ("cell_height", Json::from(self.cell_height)),
            ("agent_radius", Json::from(self.agent_radius)),
            ("agent_height", Json::from(self.agent_height)),
            ("max_step", Json::from(self.max_step)),
            ("max_slope_deg", Json::from(self.max_slope_deg)),
            ("tile_cells", Json::from(self.tile_cells)),
            ("bounds", Json::obj([("min", v(self.bounds_min)), ("max", v(self.bounds_max))])),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borders_wider_than_tiles_are_refused() {
        let e = Config::huge_border().validate().unwrap_err();
        assert_eq!((e.code, e.details[0].1.as_str()), ("budget", Some("tile_cells")), "{e:?}");
        assert!(Config { tile_cells: 128, ..Config::huge_border() }.validate().is_err(), "128 + 2 x 103 is wider than 96");
        // 0.3 m at 0.02 m cells: a border of 15 + 3 columns; 60 + 2 x 18 = 96 fits, 61 doesn't.
        assert!(Config { tile_cells: 60, agent_radius: 0.3, ..Config::huge_border() }.validate().is_ok());
        assert!(Config { tile_cells: 61, agent_radius: 0.3, ..Config::huge_border() }.validate().is_err());
        assert!(Config { tile_cells: 256, cell_size: 0.0396, agent_radius: 10.0, ..Config::default() }.validate().is_err());
        assert!(Config::default().validate().is_ok());
    }

    #[test]
    fn hostile_configs_are_errors_not_panics() {
        for c in [
            Config { cell_size: 0.0, ..Config::default() },
            Config { agent_radius: 1.0e30, ..Config::default() },
            Config { agent_radius: f32::NAN, ..Config::default() },
            Config { tile_cells: 0, ..Config::default() },
            Config { cell_size: 1.0e-30, agent_radius: 10.0, ..Config::default() },
            Config { cell_height: 0.0, ..Config::default() },
        ] {
            let _ = Derived::of(&c);
            assert!(c.validate().is_err(), "{c:?}");
        }
    }
}

/// Grid numbers derived from a [`Config`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Derived {
    pub cells_x: u32,
    pub cells_z: u32,
    pub tiles_x: u32,
    pub tiles_z: u32,
    /// Erosion radius in columns (rounded up).
    pub radius_cells: u32,
    /// Agent height in cell heights (rounded up).
    pub height_q: u16,
    /// Step in cell heights (rounded down).
    pub climb_q: u16,
    /// Extra columns rasterised around each tile, so erosion at its edge is exact.
    pub border: u32,
    pub max_q: u16,
    /// `cos(max_slope)`: a triangle is walkable when its normal's Y is at least this.
    pub walkable_y: f32,
}

impl Derived {
    pub fn of(c: &Config) -> Derived {
        use specula_core::dmath;
        let size = c.bounds_max - c.bounds_min;
        // Ratios within a thousandth of a whole number count as that number (0.3 / 0.15 is 2,
        // whatever the last bit of either says).
        const EPS: f32 = 1.0e-3;
        let up = |v: f32| dmath::ceil(v - EPS).max(0.0);
        let down = |v: f32| dmath::floor(v + EPS).max(0.0);
        let cells = |s: f32| (up(s / c.cell_size) as u64).clamp(1, u64::from(u32::MAX)) as u32;
        let (cells_x, cells_z) = (cells(size.x), cells(size.z));
        // Saturating throughout: derived numbers of an unchecked config never overflow (validate
        // refuses such configs; this only has to stay sane until it does).
        let tiles = |n: u32| n.div_ceil(c.tile_cells.max(1));
        let radius_cells = up(c.agent_radius / c.cell_size) as u32;
        Derived {
            cells_x,
            cells_z,
            tiles_x: tiles(cells_x),
            tiles_z: tiles(cells_z),
            radius_cells,
            height_q: (up(c.agent_height / c.cell_height) as u32).min(u32::from(u16::MAX)) as u16,
            climb_q: (down(c.max_step / c.cell_height) as u32).min(u32::from(u16::MAX)) as u16,
            border: radius_cells.saturating_add(3),
            max_q: (up(size.y / c.cell_height) as u32).min(u32::from(u16::MAX) - 1) as u16,
            walkable_y: dmath::cos(c.max_slope_deg.to_radians()),
        }
    }
}

/// An input triangle in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle(pub [Vec3; 3]);

impl Triangle {
    pub fn aabb(&self) -> (Vec3, Vec3) {
        let [a, b, c] = self.0;
        (a.min(b).min(c), a.max(b).max(c))
    }
}

/// A source as one tile reads it: all its triangles, or only those listed (from
/// [`Source::bucket`]).
#[derive(Clone, Copy, Debug)]
pub struct Input<'a> {
    pub source: &'a Source,
    pub only: Option<&'a [u32]>,
    /// The area its walkable surfaces get (0: plain ground).
    pub area: u8,
}

impl<'a> Input<'a> {
    pub fn all(source: &'a Source) -> Input<'a> {
        Input { source, only: None, area: 0 }
    }

    pub fn triangle_count(&self) -> usize {
        match self.only {
            Some(list) if !self.source.aligned => list.len(),
            _ => self.source.triangle_count(),
        }
    }

    /// The triangles this tile reads.
    pub fn triangles(&self) -> impl Iterator<Item = &'a Triangle> + 'a {
        let (src, only) = (self.source, self.only);
        let listed = only.map(move |l| l.iter().filter_map(move |&i| src.triangles.get(i as usize)));
        let all = only.is_none().then(|| src.triangles.iter());
        listed.into_iter().flatten().chain(all.into_iter().flatten())
    }
}

/// Areas a navmesh tells apart: 0 is plain ground, 1.. are named in its settings.
pub const MAX_AREAS: usize = 32;

/// A volume whose open space takes an area (water, mud, a road): the columns whose centre and
/// floor lie inside the box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub lo: Vec3,
    pub hi: Vec3,
    pub area: u8,
}

/// How a path search weighs areas: a cost multiplier per area (distance across a polygon times
/// its area's cost) and the areas it may not enter at all.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Filter {
    pub cost: [f32; MAX_AREAS],
    /// Bit `a`: area `a` is excluded.
    pub exclude: u32,
}

impl Default for Filter {
    fn default() -> Filter {
        Filter { cost: [1.0; MAX_AREAS], exclude: 0 }
    }
}

impl Filter {
    /// The cost multiplier of an area (`None`: excluded).
    pub fn cost_of(&self, area: u8) -> Option<f32> {
        let a = usize::from(area).min(MAX_AREAS - 1);
        (self.exclude & (1 << a) == 0).then(|| self.cost[a])
    }

    /// The smallest multiplier (scales the A* heuristic so it never overestimates).
    pub fn min_cost(&self) -> f32 {
        self.cost.iter().copied().fold(f32::INFINITY, f32::min).clamp(0.01, 1.0)
    }

    pub fn is_default(&self) -> bool {
        *self == Filter::default()
    }

    pub fn hash_into(&self, h: &mut specula_core::hash::StableHasher) {
        for c in self.cost {
            h.write_u32(c.to_bits());
        }
        h.write_u32(self.exclude);
    }
}

/// How an off-mesh link is crossed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LinkKind {
    /// An arc through the air.
    Jump,
    /// Falling down (one way unless said otherwise).
    Drop,
    /// Climbing, at half speed.
    Ladder,
}

impl LinkKind {
    pub fn name(self) -> &'static str {
        match self {
            LinkKind::Jump => "jump",
            LinkKind::Drop => "drop",
            LinkKind::Ladder => "ladder",
        }
    }

    pub fn parse(s: &str) -> Option<LinkKind> {
        match s {
            "jump" => Some(LinkKind::Jump),
            "drop" => Some(LinkKind::Drop),
            "ladder" => Some(LinkKind::Ladder),
            _ => None,
        }
    }

    /// Speed while crossing, as a share of the agent's.
    pub fn speed_share(self) -> f32 {
        match self {
            LinkKind::Jump => 1.0,
            LinkKind::Drop => 1.5,
            LinkKind::Ladder => 0.5,
        }
    }
}

/// A connection the surface doesn't have (a gap to jump, a ledge to drop from, a ladder): from
/// one point on the navmesh to another, as data (navmesh settings).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OffMeshLink {
    pub from: Vec3,
    pub to: Vec3,
    pub kind: LinkKind,
    pub bidirectional: bool,
    /// Path cost multiplier of crossing it (times its length).
    pub cost: f32,
    /// Its area (filters may exclude it).
    pub area: u8,
    /// How far (X/Z) from its end points the navmesh may be found.
    pub radius: f32,
}

/// Most off-mesh links one navmesh holds.
pub const MAX_OFFMESH_LINKS: usize = 256;

/// Work a tile build costs, in grid columns: its grid with the border, plus 16 per triangle.
pub fn tile_work(cfg: &Config, inputs: &[Input]) -> u64 {
    let d = Derived::of(cfg);
    let side = u64::from(cfg.tile_cells) + 2 * u64::from(d.border);
    side * side + 16 * inputs.iter().map(|i| i.triangle_count() as u64).sum::<u64>()
}

/// One piece of input geometry: the triangles of a convex solid (a box, hull, sphere or capsule:
/// every column it covers is solid from its lowest to its highest point) or of a surface (a mesh
/// or heightfield: only the triangles themselves are solid).
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub triangles: Vec<Triangle>,
    pub solid: bool,
    /// An axis-aligned solid box (no triangles): filled column by column, without clipping
    /// (most greybox floors and walls; much faster than their triangles).
    pub aligned: bool,
    /// Bounds of the triangles (the box itself when `aligned`).
    pub lo: Vec3,
    pub hi: Vec3,
}

impl Source {
    pub fn new(triangles: Vec<Triangle>, solid: bool) -> Source {
        let (mut lo, mut hi) = (Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY), Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY));
        for t in &triangles {
            let (a, b) = t.aabb();
            lo = lo.min(a);
            hi = hi.max(b);
        }
        Source { triangles, solid, aligned: false, lo, hi }
    }

    /// An axis-aligned solid box.
    pub fn aligned_box(lo: Vec3, hi: Vec3) -> Source {
        Source { triangles: Vec::new(), solid: true, aligned: true, lo: lo.min(hi), hi: lo.max(hi) }
    }

    /// Triangles it counts as against a tile's budget.
    pub fn triangle_count(&self) -> usize {
        if self.aligned {
            12
        } else {
            self.triangles.len()
        }
    }

    /// Which of its triangles each tile of `mesh` reads (for big sources: a tile then looks at
    /// its own triangles only, not all of them).
    pub fn bucket(&self, mesh: &mesh::NavMesh) -> std::collections::BTreeMap<(u32, u32), Vec<u32>> {
        let mut out: std::collections::BTreeMap<(u32, u32), Vec<u32>> = std::collections::BTreeMap::new();
        for (i, t) in self.triangles.iter().enumerate() {
            let (lo, hi) = t.aabb();
            for tile in mesh.tiles_touching(lo, hi) {
                out.entry(tile).or_default().push(i as u32);
            }
        }
        out
    }

    /// Whether the source's bounds meet the box `lo..hi`.
    pub fn touches(&self, lo: Vec3, hi: Vec3) -> bool {
        (self.aligned || !self.triangles.is_empty())
            && self.lo.x <= hi.x
            && self.hi.x >= lo.x
            && self.lo.y <= hi.y
            && self.hi.y >= lo.y
            && self.lo.z <= hi.z
            && self.hi.z >= lo.z
    }
}
