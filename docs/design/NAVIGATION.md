# Design: navigation and AI

Status: designed 2026-10-18 (M3 lane "Locus, navigation and AI"); implementation notes at the end. ROADMAP M3
"Navigation and AI". Rules it follows: [DETERMINISM.md](DETERMINISM.md) (F2), [PHYSICS.md](PHYSICS.md) (the P2
character controller it matches), [AGENTS.md](../../AGENTS.md). Placement by relations is its sibling:
[LOCUS.md](LOCUS.md).

**Goal:** non-player characters that find their way through any world agents build, avoid each other, and decide what
to do from behaviour written as data, all deterministic, cheap on the Intel iGPU machine's CPU, and explainable to
agents as data.

## The pieces

| Crate / file | Role |
|---|---|
| `crates/specula-via` (*via*, "way") | Pure navigation, no engine types: tiled navmesh generation from triangles, polygon queries, A* and the funnel, crowd steering and avoidance. Depends only on `specula-core`. |
| `crates/specula-mens` (*mens*, "mind") | Pure AI as data: behaviour trees and utility AI parsed and validated from JSON (diagnostics with paths and hints), and a tick runner over a host trait. Knows nothing about navigation or the world. |
| `crates/specula/src/nav.rs` | The engine bridge: navmeshes as world entities, geometry gathered from colliders and shapes, tiles rebuilt when the world changes, nav agents and minds stepped on the fixed tick, written back like physics. |
| `crates/specula/src/nav_tools.rs` | The `nav_*` and `ai_*` tools, and the `navmesh` and `paths` sight overlays. |

Why pure crates: the Lex lane (gameplay as data) binds AI and navigation without depending on the engine, and unit
tests run without a world. The engine never hands a crate a `World`; it hands triangles, points and a host.

## Navmesh generation

The standard pipeline (Recast: voxelise, filter, erode, build polygons), simplified where agents gain nothing from the
complexity.

1. **Input.** Triangles in world space, each flagged walkable when its normal is within `max_slope_deg` of +Y.
   Sources, chosen per navmesh:
   - **colliders**: static colliders (box, sphere, capsule, hull, mesh, heightfield), triangulated exactly
     (spheres and capsules through `dmath` with fixed segment counts);
   - **shapes**: drawn primitives without a collider (cube, quad, sphere, lathe as its bounds box) so greybox worlds
     work before anyone adds colliders.

   Left out: dynamic bodies, characters, nav agents (they move), triggers, transient entities, hidden entities, the
   `ui` layer, 2D shapes, and entities tagged `nav_ignore`. Models without a collider are left out and counted
   (their bounds arrive when an asynchronous build finishes, which would make the navmesh depend on wall-clock time,
   F2); give them a collider.
2. **Voxelise per tile.** The world inside the navmesh's `bounds` is a grid of square columns `cell_size` wide; a tile
   is `tile_cells × tile_cells` columns. Each tile rasterises the triangles that touch it, plus a border wide enough
   for erosion, into **spans** (solid intervals per column, heights in `cell_height` steps). Overlapping spans merge;
   a span's top is walkable if its top triangle was.
3. **Open space.** The gaps between solid spans are where an agent can stand. A gap is walkable if its floor is
   walkable and it is at least `agent_height` tall. Neighbouring gaps connect when their floors differ by at most
   `max_step` and the shared opening is `agent_height` tall: the same numbers as the P2 controller
   (`body.character.step_height`, `max_slope_deg`, the capsule's radius and height). `nav_build {like: entity}` copies
   them from a character, so the navmesh and the walker never disagree.
4. **Erode** by `agent_radius`: a two-pass chamfer distance (2 per straight step, 3 per diagonal, integers) from every
   gap with a missing neighbour; gaps closer than the radius go. Ledges higher than a step have no neighbour, so they
   are eroded too: no separate ledge filter is needed.
5. **Polygons: greedy rectangles.** Each tile's walkable gaps merge into axis-aligned rectangles (greedy meshing,
   rows first, in a fixed order) over gaps that are connected and lie within `cell_height × 2` of the rectangle's
   bilinear height surface; so a ramp is one rectangle and each stair tread is another. A polygon is a rectangle in X/Z
   with four corner heights.
   - *Why not Recast's contours and convex polygons:* contour tracing, simplification and triangulation are the
     hardest, most error-prone part of Recast and buy smaller polygon counts and boundaries at any angle. Rectangles
     are exact on the grid, trivially convex, cheap to link across tiles (their edges lie on cell lines), and the
     funnel smooths the staircase out of paths. Diagonal walls cost more polygons (a staircase of rectangles, at most
     one per cell row per tile). Contour polygons stay a possible upgrade behind the same query API.
6. **Links.** Rectangles touching along a cell line link through a **portal** (the shared segment). Inside a tile they
   are found from the gap neighbours; across tiles from each tile's edge columns, kept per tile, so a rebuilt tile
   relinks with its four neighbours only.

**Tiles and edits.** Tiles are keyed `(tx, tz)` in an ordered map. When an entity that feeds a navmesh changes (the
world's change log, as physics uses it), the tiles its old and new bounds touch, grown by the erosion border, are
marked dirty. Dirty tiles rebuild on the fixed tick, at most `max_tiles_per_tick` (default 8) per tick in tile order,
so an edit costs only the tiles it touches and a huge edit spreads over ticks without breaking determinism (the budget
counts tiles, not milliseconds). `nav_build` builds every tile at once.

**Navmeshes are entities.** `nav_build` writes (journaled, claim-checked, undoable) a group entity `/nav/<name>`
whose `props.navmesh` holds the settings, including the `bounds` it computed from the sources if none were given (so
new geometry outside never moves the grid). The navmesh itself is derived data, rebuilt deterministically from the
world when a world loads. Several navmeshes (small and large agents) are several entities.

## Queries

- **Nearest polygon:** the polygons of the tiles around a point, the closest point on each (clamped in X/Z, height from
  its surface), nearest first, ties by polygon id.
- **Paths:** A* over polygons. A node's position is where the path entered it (the portal's closest point to the
  previous position); costs are 3D distances; the heuristic is 0.999 × the straight distance (Recast's). The open
  list orders by the cost's bits, then the polygon id, so ties never depend on memory layout. At most `max_nodes`
  (default 4,096) polygons are expanded; past that the path goes to the polygon nearest the goal and says
  `partial: true`, as does a goal off the navmesh or unreachable.
- **String pulling:** the simple stupid funnel algorithm over the portals (in X/Z), corner heights from the polygons.
- **Moving along the surface:** a point moves toward a target through portals; where there is no neighbour it slides
  along the edge. Agents that aren't physics characters move this way, so they never leave the navmesh.
- **Random points** (wandering): a polygon chosen by area with a seeded `specula_core::rng::Rng`, a point inside it.

Everything is `+ - * /`, `sqrt` and `dmath`; containers are ordered; every result is sorted by stable keys.

## Crowds

Nav agents are entities with `props.nav_agent` (`speed`, `radius`, `goal`, the navmesh). Each fixed tick, before
physics:

1. **Goals and corridors.** An agent with a goal keeps a corridor (its path's polygons). It repaths when the goal
   moves more than a metre, when it leaves its corridor, when a tile under the corridor rebuilt, and at least every 2 s.
   Repaths are budgeted (default 32 per tick, oldest request first), so a hundred agents changing goal at once spread
   over a few ticks.
2. **Steering.** Toward the next funnel corner, slowing within the arrival distance.
3. **Avoidance:** sampled velocities, like Recast's `dtObstacleAvoidance`: the preferred velocity plus 16 directions
   × 2 speeds (a fixed table) plus standing still, each scored by how far it is from the preferred and current
   velocities and by the time to the first collision with up to 6 nearest neighbours (reciprocal: each agent takes
   half). Lowest score wins, ties by sample index. It is cheaper and simpler than ORCA's linear program, and bounded:
   `agents × neighbours × samples`. Overlapping agents are pushed apart.
4. **Moving.** An agent with a physics character body (`body.character`) only gets its walking velocity
   (`body.velocity` x and z): the P2 controller walks it, with steps, slopes and pushing, so physics stays the one
   source of collisions (F4). Other agents move along the navmesh surface directly.

Writes go through the world like physics' (after-only, coalesced per tick, journal kind `nav`, exempt from claims).
Hidden state (corridors, velocities, minds' running nodes, timers) is hashed each tick; `world_hash` reports the nav
hash next to the world's and physics'.

## AI as data

Behaviour is JSON data, validated with diagnostics that say where and what to change, run by a deterministic tick
runner. Two models, freely nested:

- **Behaviour trees:** `sequence`, `selector` (fallback), `parallel` (success/failure thresholds), decorators
  (`invert`, `succeed`, `fail`, `repeat` n or forever, `retry`, `cooldown` ticks, `timeout` ticks), `wait` ticks,
  `condition` and `action` leaves, and `set` (write the blackboard). Status per tick: success, failure, running;
  running nodes resume next tick (no re-evaluation from the root of `sequence`s already past), and a `selector`
  re-checks higher-priority children each tick (reactive) unless `"reactive": false`.
- **Utility AI:** a `utility` node scores its options and runs the best one. An option's score is the product of its
  considerations (each an input mapped through a curve: linear, quadratic, power, logistic, step, inverse; clamped to
  0..1), times its weight, plus a `momentum` bonus while it is the running option (hysteresis, so agents don't
  dither). Ties go to the earlier option. `ai_status` shows every option's score and each consideration's value.

**Inputs, conditions and actions belong to the host.** The runner knows no game: a condition is `{"if": name, ...}`
and an action `{"do": name, ...}`, checked against the host's declared vocabulary (names, argument names and types),
so a typo is a diagnostic with the nearest valid name. Blackboard comparisons (`{"if": "bb", "key", "op", "value"}`)
are built in. The engine's host offers:

| Kind | Names |
|---|---|
| Conditions | `sees` (range, field of view, line of sight by a physics raycast), `near` (distance), `has_goal`, `arrived`, `on_navmesh`, `bb` |
| Actions | `move_to` (entity, point or blackboard key), `patrol` (points or an entity's children, in order, looping), `chase` (a moving entity, repathing, giving up past a distance), `flee`, `wander` (seeded random points), `stop`, `face` |
| Inputs (utility) | `distance` (to an entity), `bb` (a blackboard number), `health`-like props (`prop`), `time_in_option` |

A mind is `props.mind = {def: <definition object> | "/path/of/an/entity/with/props.mind_def", blackboard: {...}}`.
`ai_set` validates and writes it (journaled, claim-checked, undoable), refusing an invalid definition with its
diagnostics; a mind written by other means that doesn't validate is reported by `ai_status` and doesn't run.

**The Lex seam.** `specula-mens` is the contract: Lex's `tree` / `utility` item kinds hold a mens definition and call
`Mind::tick` with a host of their own (Lex's vocabulary), and Lex's planned `navigate` behaviour maps onto the engine's
nav agent: set a goal, read `path_remaining` (the corridor's length, which the engine exposes per agent). Neither
crate depends on Lex, and Lex needs no engine change to bind them.

## Agent tools

- `nav_build`: create or update a navmesh (settings, `like` a character, sources) and build it; returns tile and
  polygon counts, time, problems (sources skipped and why).
- `nav_path`: a path between points or entities: corners, length, polygons visited, `partial` and why.
- `nav_point`: the nearest point on a navmesh, whether a point is on it, or seeded random reachable points.
- `nav_agent`: make an entity a nav agent or change its goal and speed (validated, journaled, undoable).
- `nav_info`: navmeshes, tiles (dirty, rebuilt), agents (paged: goal, corner, speed, remaining, stuck), errors, hash.
- `ai_check`: validate a definition (diagnostics, no world change). `ai_set`: attach a mind. `ai_status`: per agent
  the running path through its tree, utility scores, the blackboard, and recent decisions (paged, byte-capped).
- Sight overlays `navmesh` (polygons and portals, coloured per tile, dirty tiles marked) and `paths` (agents'
  corridors, corners, goals, velocities).

## Budgets (F3)

Targets on the reference machine (one core):
- building a 64 m × 64 m greybox room set at 0.15 m cells: under 50 ms; rebuilding one tile: under 2 ms;
- a path across that map: under 0.2 ms;
- 100 agents stepping (steering, avoidance, moving): under 0.5 ms a tick.

Hard caps, each a structured `budget` error when it bites: 4,096 tiles and 16 M columns per navmesh, 400,000 input
triangles, 64 spans per column, 16,384 polygons per tile, 4,096 expanded nodes per path, 256 corridor polygons,
1,024 agents and 64 minds' trace lines; responses are paged and byte-capped.

## Not in this design

Off-mesh links (jumps, ladders), area costs (mud, water), dynamic obstacles carved without a rebuild, hierarchical
pathfinding for huge worlds, formations. Each fits the structure (links are portals with a kind; costs are per polygon)
and waits for a game that needs it.
