# Design: navigation and AI

Status: designed and built 2026-10-18 (M3 lane "Locus, navigation and AI"); what was built, and where it departs from
this design (with why), is in [Implementation](#implementation-2026-10-18) at the end. ROADMAP M3
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

Dynamic obstacles carved without a rebuild, hierarchical pathfinding for huge worlds, formations. Each fits the
structure and waits for a game that needs it. (Off-mesh links and area costs were added in M3: see "Links, areas,
saves and model colliders" below.)

## Implementation (2026-10-18)

Built as designed, with these changes and details. Each amendment says why.

**Files.** `crates/specula-via` (`build`: one tile; `mesh`: tiles, links, hash; `query`: nearest, A*, shortcut,
funnel, surface moves, random points; `crowd`; `examples/via_bench.rs`), `crates/specula-mens` (`def`: parse and
check; `run`: the runner), `crates/specula/src/nav.rs` (the bridge), `crates/specula/src/nav_tools.rs` (tools,
overlays, the tick hook). Live checks: `scripts/nav_check.py` (a courtyard: Locus, navmesh, edits, a guard that patrols
and chases, a crowd, captures, twin-run hashes) and `scripts/nav_sepulcra.py` (Sepulcra's slab).

**Generation.**
- *Amendment: convex solids fill their columns.* Rasterising only surfaces (as Recast does) leaves the inside of a
  closed box as open space; a wall taller than an agent then has walkable "rooms" inside it. Boxes, hulls, spheres and
  capsules are solids: each column they cover is solid from their lowest to their highest point. Meshes and
  heightfields stay surfaces.
- *Amendment: an axis-aligned box fast path.* Unrotated cubes and box colliders (most greybox) fill their columns
  directly instead of clipping 12 triangles per column: a tile's rasterisation went from 99 ms to 2.5 ms for the
  benchmark's 100 tiles. The result is the same as the triangles' except in columns a face only touches along a
  column line.
- *Amendment: tiny islands are dropped.* Erosion leaves single walkable cells where narrow wall tops meet (a cell two
  steps from every edge survives a two-cell radius). Islands under 8 cells that don't reach the tile's border are
  removed (Recast's minimum region area does the same).
- Faces lying exactly on a column line count for the columns on both sides (conservative: a wall on a line erodes a
  column more).
- Margins are the agent radius to within a column: an obstacle's edge inside a column makes the whole column its
  border, so the walkable area can come up to one `cell_size` closer than the radius (0.15-0.3 m for a 0.3 m radius
  at 0.15 m cells; tested at a doorway's jambs). Use a smaller `cell_size` for tighter margins.
- Tile builds reuse their buffers; links are kept sorted, so a navmesh built tile by tile equals one built at once
  (tested).

**Queries.**
- *Amendment: a visibility shortcut after A*.* A* over large rectangles can bend a path at a polygon corner it never
  needed (its node positions are portal entry points). After the search, a navmesh raycast tries the straight walk to
  the end, then from each funnel corner to the farthest later one it can reach; the polygons it crosses replace the
  detour. Used by `nav_path` and by crowd repaths.
- A*'s nodes live in a small open-addressing map (lookups only, never iterated) instead of an ordered map.
- The funnel works out portals as it reaches them, so steering (3 corners ahead) never builds a long corridor's
  portal list.
- Random points pick a tile by area, then a polygon by a per-tile running total (bounded, no full scan per sample).

**Crowds.**
- *Amendment: repaths are budgeted by work, not only by count:* at most 32 searches and 2,048 expanded polygons per
  step (at least one search always runs), round robin by id.
- *Amendment: no periodic repath every 2 s; every 8 s.* Corridors are checked every step (tiles rebuilt under them, the
  agent leaving them, the target moving 0.5 m), which is what periodic repaths were for.
- `remaining` is exact over the next three corners and a straight line past them (exact on the last stretch, where
  arrival slows the agent).
- Neighbours come from agents sorted by grid cell (rows contiguous); avoidance reads a snapshot, so agents never see
  each other's half-updated state; overlap push-apart uses the same grid. A step's containers are reused between
  steps (only the funnel's two small lists per moving agent are allocated).

**Engine.**
- Sources: as designed, plus *amendment: whatever is below something that moves on its own* (a nav agent, a dynamic
  body, a character) never feeds a navmesh: a character's visible body, a child cube without a collider, used to
  carve a hole where it stood. When an entity starts or stops moving on its own, its subtree is looked at again.
- Agents and minds are re-read only when their `props` change (versions change at every write-back).
- Navmesh changes rebuild everything (a new grid) and re-link agents; edits mark the tiles their old and new bounds
  touch (plus the erosion border); they rebuild within `tiles_per_tick` default tiles of work per tick (see the
  review round below). `sources.under` matches
  by entity id up the parent chain; when the path comes to name another entity (or none), every tile rebuilds.
- Read tools (`nav_path`, `nav_point`, `nav_info`, `ai_status`, a `world_place` dry run) never apply world changes:
  they see navigation as of the last tick or write, so calling them can't change what the simulation does. Write
  tools (`nav_build`, `nav_agent`, `ai_set`, `world_place`) bring it up to date first, as inputs at a tick boundary.
- Configs whose erosion border (radius + 3 columns) is wider than a tile, whose tile grid with its border is wider
  than 96 columns, or whose tiles with borders add up to more than 16 M columns, are refused (`budget`, naming the
  field to change); settings saved with wider tiles are narrowed when read (nav_build refuses them).
- A mind that can't run (its definition missing, invalid or over the cap) leaves its agent standing, like a removed
  one.
- A mind that is replaced (its definition changed or became invalid) or removed takes its goal, facing and patrol
  index with it. Dry runs and status reads see the last tick (said in the tool descriptions).
- A mind's abort clears only the goal its own action node set (a reactive selector starts the new child before
  halting the old one); `Host::action` receives the node index for this.
- Mind problems are kept apart from agent problems; a mind whose definition path doesn't exist yet loads when an
  entity with `props.mind_def` appears there.
- `wander` and `nav_point {random, radius}` sample the polygons of the tiles around the point, not the whole
  navmesh; `world_place`'s `reachable_from` floods the navmesh once per call instead of searching per candidate.
- Write-back: crowd-moved agents get position (and, at the top level, yaw from their velocity or a `face` action);
  physics characters get `body.velocity` (x, z; the vertical part kept) and physics walks them. Journal kind `nav`.
- `world_hash` reports `nav` (navmeshes, crowds, minds, agents' runtime goals); the desync trace carries a `nav`
  subsystem hash; `perf_frame` has a `nav` section.
- *Limit:* hidden state (corridors, minds' running nodes, patrol indices) isn't saved in world files: a reloaded world
  rebuilds its navmeshes and restarts its minds from the root (deterministically, but not bit-identically to the run
  that saved it).

**Review round (coordinator, 2026-10-18).**
- Hostile settings (cell size 0, radius 1e30, NaN, zero heights) are `invalid_value` / `budget` errors: field ranges
  are checked before anything is derived from them, with saturating maths, in the tool and when world data is read.
- *Amendment: building is sliced* (superseded in review round 2: `nav_build` no longer builds, and loads hold the clock). Work is counted in grid columns plus 16 per source triangle (big sources, over
  256 triangles, are bucketed by tile once, so a tile reads only its own triangles). `nav_build` does about 50 ms of
  work (80,000 units) and says `complete`, `tiles_left`; the ticks finish the rest within `tiles_per_tick` default
  tiles of work (3,364 units, about 2 ms each; default 2), and calling `nav_build` again builds the next slice (for
  a paused world). A navmesh that has never been complete answers `navmesh_building` with its progress (paths,
  points, `world_place on_navmesh`), and its agents wait. New settings for a navmesh that serves are built beside it
  and swapped in when complete (agents relink then). Navmeshes a world load (or the first look at a world) brings
  are built in full before the first tick, so a loaded world behaves like the run that saved it.
- Navmesh settings under `/nav` sit nowhere: region claims don't cover them (path claims on `/nav` do), like Lex
  rule data.
- Minds share a per-tick work budget of 4,096 (node visits, senses and inputs 1, line-of-sight raycasts 16), round
  robin by id from the one after the last that ran (at least one runs); the tick report says `mind_work` and
  `minds_skipped`. A mind's identical `sees` within a tick casts once. Shared definitions are parsed once per
  version. Each mind's state hash is kept as it runs, so the per-tick nav hash costs a word per mind.
- Senses sync the physics space before minds run, so `sees` respects walls with physics disabled.
- `nav_info` lists problems as one short line each (a code and the first diagnostic; `ai_status` has the rest) in
  8 KB, says `problems_not_listed`, and cuts the agent page at 32 KB (`next_cursor` continues it).
- Agents turn about Y keeping any tilt, below a parent in its frame (exact for parents turned about Y). Editing the
  Euler Y alone mirrored characters whose physics write-back reads (180, 180 - yaw, 180).
- `world_undo` of `nav_agent` / `ai_set` after navigation moved the entity stays `undo_conflict`, with a hint:
  `force: true` undoes it and puts the entity back where it stood.
- A clump looks at at most 128 agents per grid row for neighbours and push-apart, and picks the nearest by
  selection, not a sort (1,024 agents at one point: bounded, still deterministic).
- Warm minds allocate nothing per tick (an allocation-counting test): running leaves compared in place, parallel and
  utility buffers kept, the blackboard overwritten in place and hashed without formatting; the nav tick's lists are
  reused.
- Measured (release, this machine, reviewer probes): 1,000 minds of 150 `sees` each: nav tick 2.6-3.7 ms mean (was
  453 ms with 290; 13 minds run per tick); a 129 x 129 heightfield: `nav_build` call 96 ms before this round's call
  budget was lowered, ticks 4.9 ms while rebuilding (was 5.1 s in one call); `world_place` with `avoid` 4,000
  tagged boxes: 34 ms for one instance, 140 ms for 8 (was minutes); 64 instances against 4,000: refused with
  `budget` after about 4 M units (1.5 M since review round 2).

**Review round 2 (coordinator, 2026-10-18).**
- *`/nav` is reserved* like `/lex`: only the nav tools (`Swarm::commit_nav`) write `/nav` or `props.navmesh`
  anywhere; world_* writes there are refused with `reserved`. Only bare groups (no shape drawn, no collider, no
  body) holding settings are exempt from region claims, and one whose transform changes is checked like any entity.
  `nav_build remove: true` deletes a navmesh.
- *Amendment: nav_build builds nothing.* It writes the settings (the same settings again are a no-op: no journal
  entry) and the fixed ticks build within `tiles_per_tick`; a paused world advances with `time_control step`. So
  the navmesh at every tick depends only on the world and the ticks, never on how many calls were made (one call
  and three identical ones now give the same nav hash).
- *Loads hold the clock.* The engine reads a loaded world's navigation as it loads; its navmeshes (built in full
  before the first tick in the run that saved it) are now built in slices of 16,000 work units a frame, about
  10 ms, with the simulation clock held: frames and agent calls are served meanwhile and the held time never
  becomes ticks. `nav_info` says `loading`. `Nav::restore_state` re-reads the world and waits for those navmeshes
  (`Nav::loading`, `Nav::load_slice`); `Nav::take_restore_outcome` has the result. A 205 m terrain after
  `world_load`: calls answered in about 40 ms throughout, where one tick used to take 2.4 s.
- Bucketing a big source is charged (4 units a triangle) and fills a list per tile directly (no list per
  triangle): nudging a 129 x 129 heightfield costs a 5.7 ms tick (was 35 ms), its tiles follow on later ticks.
- `BoxSet`'s huge-reach path tests boxes against a mask, not a scan of the big ones: 16,000 tagged boxes reach the
  budget error in under a second (was 9.8 s); the per-call budget is 1.5 M units (about 350 ms there).
- `model_collider` answers with a summary (shape, sizes or point / vertex / triangle counts, bounds, offset): a
  mesh collider in a few hundred bytes (was 84-330 KB).
- Mind work units now track cost: senses resolve their targets without building keys (a cached `sees` is about
  0.8 us, its 2 units), about 0.4 us a unit, so 4,096 units is about 1.7 ms a tick (1,000 minds of 200 `sees`:
  1.7 ms mean). Minds loading a shared definition already parsed at that version no longer copy its JSON (300 new
  minds: first tick 9 ms, was about 190; 1,000 was 840 ms).
- `Mind::restore` checks every node's cursor and choice against its children, parallel results and utility scores
  against the node: a damaged save is refused, never a panic later.
- A malformed or unknown `props.nav_area` (an area the navmesh doesn't name) is a problem in `nav_info`
  (`invalid_nav_area`, `unknown_area` with the navmeshes), not silent plain ground.

**Save games.** A save's `nav` section (`nav::encode_section` of `Nav::save_state`, versioned, at most 64 MB) is
decoded and shape-checked on the load's worker (`nav::parse_section`: version, JSON, kinds and counts). At the swap
`Nav::restore_state` re-reads the loaded world and waits: the engine holds the clock while the navmeshes build in
slices, then puts the crowds (and each navmesh's build counters, which agents compare corridors against), minds and
goals back and checks the nav hash (`nav_info last_restore`). `save_read` lists `nav` as pending until then.
*Amendment (review round 4): saving builds nothing.* Settling dirty tiles before a save made `save_write` an input
(the saving run diverged from one that didn't save) and cost 650 ms with 1,224 dirty tiles. Now the build state is
data: the dirty and unbuilt tile sets and the polygons of stale tiles (built, then their geometry changed: the world
can't rebuild them as they are) go in the save (`NavMesh::tile_state`, `restore_tile`: checked, bounded by the
section cap). A load builds only the up-to-date tiles (sliced, clock held), puts the stale ones back, keeps the
queues and the build counters, so the ticks' rebuilds continue exactly. Saves while a navmesh is first being built
work; only a switch to new settings (two navmeshes at once) or a load being prepared is refused (`nav_building`,
`retry_after_ms`; the game's autosave is retried up to 20 times). The save syncs navigation first, which is what the
next tick's sync does anyway. The review's p23 is a test (`saving_is_never_an_input_and_a_load_continues_mid_build_exactly`):
a run that saves and one that doesn't hash the same on every tick, and the save, taken mid first build with an edit
just made, loads and continues tick for tick. Live (p23 with a load): the same world and nav hashes and walker
position 90 ticks on; the save takes about 10 ms. `scripts/save_check.py`: Sepulcra with a slab navmesh, three walkers and a patrolling
guard, saved at tick 651 and loaded in another engine: `desync_compare` matches on all 900 later ticks, `nav`
included. A unit test cuts and flips the section: structured errors, never a panic.

**Review round 3.** A load finishes only the tiles it brought (never built yet); edits made meanwhile are left
to the ticks, so a busy swarm can't hold the clock. Load slices run back to back (headless wakes at once; a
window spends 30 ms a frame). `time_control step` waits up to 2 s for a load and otherwise answers `loading`
(steps are never queued behind it); a playtest's ticks wait up to 30 s. Playtest checkpoints hash `nav` too.
`nav_build` is an edit-class call now that it builds nothing (a no-op answers in about 10 ms, was 104).

**Links, areas, saves and model colliders (M3 additions, 2026-10-18).**
- *Off-mesh links* are navmesh data: `props.navmesh.links` (nav_build `links`), at most 256, each `{from, to,
  kind: jump | drop | ladder, bidirectional (not for drops by default), cost, area, radius}`. Each end is found on the
  navmesh (the nearest polygon within `radius`) and found again when the tiles around it rebuild; the navmesh hash
  covers them. A* treats a link as an edge from its take-off polygon to its landing one (cost: its length times
  its cost and its area's); the funnel ends a stretch at a take-off point, so a path is surface stretches joined by
  hops (`nav_path` lists them as `offmesh: [{corner, kind}]`); shortcuts never cross a link. A crowd agent at a
  take-off point crosses on its own clock (a jump in an arc, drops at 1.5x and ladders at half speed), outside
  avoidance and push-apart, and lands on the far polygon; a physics character is carried (its position written, its
  velocity zeroed) while it crosses. Changing only the links resets them in place: no tile rebuilds, agents search
  again. The navmesh overlay draws each link as an arc (red when an end found no navmesh).
- *Areas:* `props.navmesh.areas` names up to 31 areas `{name, cost (0.1..1000, times the distance), flags}`; id 0
  is plain ground. An entity's `props.nav_area` gives its walkable surfaces an area (`"water"`), or, as
  `{area, volume: true}`, marks the open space inside its unit box (scaled, turned and placed; it may be hidden and
  adds no geometry). Spans carry the area of the surface forming their top (equal tops: the larger id, so order
  never matters), marks apply after, rectangles never mix areas. Searches take a filter (a cost per area and an
  exclusion mask): `nav_agent` and `nav_path` take `area_costs` (by name; `ground` for plain ground),
  `avoid_areas` and `avoid_flags`. The A* heuristic is scaled by the cheapest cost, so it never overestimates; an
  excluded area is a wall for paths and for moving (an agent standing in one may still leave it); a shortcut
  crosses only areas costing no more than ground or already on the corridor. The overlay colours named areas.
- *Saves:* `Nav::save_state()` (crowds with the hash of the navmesh their polygon refs belong to, minds' whole
  running state and blackboards via `Mind::save`, agents' runtime goals, facing, patrol indices, the mind round
  robin, and the nav state hash; floats as bits) and `Nav::restore_state(world, generation, save)` (re-reads the
  world, builds every navmesh in full, puts the state back and checks the nav hash, else `state_mismatch` naming
  what differs and navigation starts fresh). A world loaded with its save continues tick for tick like the run that
  saved it (a test runs both side by side for 165 ticks). The save-game lane wires these into the save file.
- *Model colliders:* `model_collider {entity, kind: auto | recipe | box | capsule | hull | mesh, moving?}` fits a
  collider to a model entity's built mesh with `specula-model`'s fitting and writes it as the entity's collider
  (journaled, claims, undo), so models collide and carve navmeshes. Moving bodies (dynamic, kinematic or
  `moving: true`) get hulls of at most 16 points (physics' cap); `mesh` uses the most detailed LOD within physics'
  mesh limits and is static only; a sideways capsule becomes a hull (physics capsules stand along +Y); notes say
  when the fit changed what was asked.
- Cost (via_bench, release, A/B against the commit before these additions, three alternating runs each on a
  machine 45-80% busy with other lanes' builds, so only the comparison means anything): build 113-135 ms vs 76-146,
  one tile 1.5-2.2 ms vs 1.1-1.4, 40 m paths 0.51-0.92 ms vs 0.60-0.67, 100 agents 0.96-1.8 ms vs 1.1-2.3 per step:
  within the noise. Without links or areas the new paths are the old ones (default filter; no links to look up).

**AI.**
- *Amendment: sequences are reactive by default too:* each tick a running sequence first re-checks the conditions
  before its running child and halts it if one fails (`"reactive": false` resumes without checking). Without it
  `[if sees, do chase]` kept chasing after sight was lost.
- Labels are `label` (node `name`s would collide with verb arguments). A blackboard holds at most 64 keys.
- `ai_set` refuses invalid definitions with every diagnostic (path, code, message, hint: the nearest valid name).

**The Lex seam, as built.** `specula_mens::{Def, Mind, Host, Vocabulary}` is the whole contract: a Lex item of kind
`tree` or `utility` holds a definition, parses it with Lex's own vocabulary, and calls `Mind::tick(&mut host, tick)`.
Lex's `navigate` behaviour can set an agent's goal through `props.nav_agent.goal` (or, in engine code, the nav agent
link) and read `Nav::path_remaining(entity)` as `path_remaining`. Neither crate depends on Lex.

**Measured (2026-10-18; i9-13900HX, one thread, release, shared machine so roughly +-30%).** `via_bench`: a 64 m x
64 m set of 64 rooms with 1.6 m doorways and 150 crates (439 boxes, 10 x 10 tiles, 1,454 polygons) builds in
28-33 ms (target 50; 108-151 ms as triangles); one tile rebuilds in 0.30-0.34 ms (target 2); paths of 40 m or more
across it take 0.16-0.18 ms on average (p95 0.31-0.34 ms, about 450 polygons expanded; target 0.2); 100 agents
step in 0.33-0.43 ms (max 0.7-1.0 ms; the first second's repath burst 1.3-1.8 ms; target 0.5). In the engine (dev
profile, engine crates at opt-level 2): the courtyard's navmesh (16 tiles) builds in 6-9 ms, Sepulcra's slab (49
tiles, 219 polygons) in 22-33 ms, a 100 m x 100 m field of 400 boxes (196 tiles, 2,548 polygons) in 102 ms; a nav
tick with a guard, a thief and 8 walkers averages 0.02-0.06 ms.
