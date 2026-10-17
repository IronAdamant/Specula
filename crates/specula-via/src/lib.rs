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
pub const MAX_TILE_SIDE: u64 = 160;
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
            return Err(NavError::new("invalid_value", "'tile_cells' must be 8..256").with("field", "tile_cells"));
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
        let side = u64::from(self.tile_cells + 2 * d.border);
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
        assert!(Config { tile_cells: 128, ..Config::huge_border() }.validate().is_err(), "128 + 2 x 103 is wider than 160");
        assert!(Config { tile_cells: 120, agent_radius: 0.3, ..Config::huge_border() }.validate().is_ok());
        assert!(Config { tile_cells: 256, cell_size: 0.0396, agent_radius: 10.0, ..Config::default() }.validate().is_err());
        assert!(Config::default().validate().is_ok());
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
        let tiles = |n: u32| n.div_ceil(c.tile_cells);
        let radius_cells = up(c.agent_radius / c.cell_size) as u32;
        Derived {
            cells_x,
            cells_z,
            tiles_x: tiles(cells_x),
            tiles_z: tiles(cells_z),
            radius_cells,
            height_q: (up(c.agent_height / c.cell_height) as u32).min(u32::from(u16::MAX)) as u16,
            climb_q: (down(c.max_step / c.cell_height) as u32).min(u32::from(u16::MAX)) as u16,
            border: radius_cells + 3,
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
