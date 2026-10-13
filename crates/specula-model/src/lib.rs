//! Specula procedural modelling, tier 1: deterministic **recipes** → meshes, LODs and colliders.
//!
//! Agents can't import outside art, so original 3D content is written as small JSON recipes
//! (see `docs/design/PROCEDURAL_MODELLING.md`). The recipe is the source; the mesh is a build
//! product. Same recipe + same parameters + same seed ⇒ **bit-identical** buffers on every
//! platform (foundation F2): all maths is `f32`/`f64` basic operations, `sqrt` and this crate's
//! deterministic trig, and no hash-map iteration order reaches the output.
//!
//! # Entry point
//!
//! ```text
//! let style = Style::parse(&fs::read_to_string("games/sepulcra/assets/style.json")?)?;
//! let model = specula_model::build(&recipe_text, &BuildParams {
//!     values: &[("tier", 3.0)],
//!     ..BuildParams::new(&style)
//! })?;
//! // model.lods[0..]: MeshData (positions, normals, sRGB8 colours, indices, parts by material)
//! // model.collider, model.materials (resolved presets), model.hash, model.report()
//! ```
//!
//! # Recipe format (version 1)
//!
//! ```text
//! { "recipe": 1, "name": "warding_stone", "category": "stone", "seed": 11,
//!   "params": { "tier": { "default": 1, "min": 1, "max": 3, "integer": true } },
//!   "let":    { "h": "2.4 + 0.2 * (tier - 1)" },          // evaluated in order
//!   "budget": { "max_triangles": "tier >= 3 ? 3000 : 1500" },
//!   "lods": [0.5, 0.25],                                  // LOD1.. as ratios of LOD0
//!   "collider": "convex_hull",                            // none | aabb | capsule | convex_hull
//!   "smooth_angle": 30,                                   // hard edge above this (degrees)
//!   "parts": [
//!     { "name": "body", "material": "carved_stone",
//!       "shape": { "op": "loft", "outline": { "rect": [1, 1], "bevel": 0.06 },
//!                  "stations": [[0.94, 0], [1, 0.06], [0.96, "h - 0.5"], [0, "h"]] },
//!       "ops": [ { "op": "bevel_tint", "color": "carved_highlight", "amount": 0.6 },
//!                { "op": "noise_tint", "amount": 0.08, "scale": 0.4 } ] },
//!     { "name": "runes", "material": "rune_glow", "when": "tier >= 2", "lod_detail": 0,
//!       "shape": { "op": "box", "size": [0.06, 0.5, 0.02] },
//!       "ops": [ { "op": "move", "by": [0, 1.2, 0.5] } ] },
//!     { "name": "flame", "include": "brazier_fire", "params": { "size": 0.8 } } ],
//!   "finish": [ { "op": "ground_shade", "amount": 0.3, "height": 0.4 } ] }
//! ```
//!
//! - **Numbers** anywhere may be expression strings over parameters and lets
//!   (`"h * 0.5"`, `"tier >= 2 ? 1 : 0"`, `min max clamp abs floor ceil round lerp sqrt sin cos`).
//! - **`when`** on a part or op switches it on or off (non-zero = on).
//! - **Colours** are `#rrggbb[aa]`, a palette name or a material preset name from the style.
//!   A part starts in its material's colour (or `color`).
//! - **Shapes** ([`recipe::shape_names`]): `box cylinder cone sphere torus capsule plane extrude
//!   loft lathe sweep`. Solids rest on y = 0, centred on the Y axis.
//! - **Ops** ([`recipe::op_names`]): `move rotate scale mirror array radial subdivide displace
//!   taper twist lean bend weld color gradient bands noise_tint ao ground_shade bevel_tint`.
//! - **Outlines** (extrude/loft/sweep profile): `[[x, z], ...]` or `{ "rect" | "sides"+"radius"
//!   | "arch" | "star"+"radius"+"inner" | "points", "rotate", "bevel", "resample", "jitter",
//!   "scale", "offset" }`.
//! - **Strict:** unknown fields, unknown ops, unknown names, bad types and out-of-range numbers
//!   are errors with a path (`parts[2].ops[0].amount`), an [`ErrorCode`] and a reason; budgets
//!   report measured vs limit.
//!
//! # Rigs (optional)
//!
//! ```text
//! "rig": { "blend": 0.08,                                 // joint blend band, metres (0 = rigid)
//!   "bones": [ { "name": "root", "head": [0, 0, 0] },     // bone 0: the only root
//!              { "name": "hips", "parent": "root", "head": [0, 0.9, 0], "tail": [0, 1.2, 0.1] },
//!              { "name": "thigh_l", "parent": "hips", "head": [0.2, 0.9, 0], "mirror": true },
//!              { "name": "shin_l", "parent": "thigh_l", "head": [0.27, 0.46, 0.12], "mirror": true } ] },
//! "parts": [ { "name": "legs", "bones": ["thigh_l", "shin_l", "thigh_r", "shin_r"], "blend": 0.1, ... } ]
//! ```
//!
//! - Bones are listed parents-first; `head` is the pivot in recipe space (numbers or
//!   expressions), `tail` defaults to the only child's head (else a point). `"mirror": true` on
//!   an `_l` bone (left = +X; the model faces +Z) adds an `_r` twin at −X right after it.
//! - The rig follows the recipe's rigid `finish` ops (move, rotate, scale, mirror without keep).
//! - A part's vertices bind to the nearest segment among its `bones` (none = the root), blending
//!   with an adjacent bone within `blend` metres. See [`rig`].
//! - [`Model::rig`] holds the bones; every LOD's [`MeshData`] carries `joints`/`weights`.
//!
//! # Mesh data
//!
//! [`MeshData`]: positions (m), unit normals (split at hard edges), **sRGB8 colours** (`[u8; 4]`,
//! alpha linear), CCW-outward triangles, and `parts` = one contiguous index range per material
//! preset (one draw each).

pub mod collider;
pub mod dmath;
pub mod error;
pub mod expr;
pub mod lod;
pub mod math3;
pub mod mesh;
pub mod noise;
pub mod poly;
pub mod recipe;
pub mod rig;
pub mod shape;
pub mod style;

pub use collider::{Collider, ColliderKind};
pub use error::{ErrorCode, RecipeError};
pub use mesh::{Bounds, MeshData, MeshError, MeshErrorCode, Part};
pub use recipe::{build, build_recipe, BuildParams, Model, Recipe};
pub use rig::{Bone, Rig, RigError};
pub use shape::Shape;
pub use style::{MaterialKind, MaterialPreset, Style};
