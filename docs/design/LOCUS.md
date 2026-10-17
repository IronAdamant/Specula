# Design: Locus, placement by relations

Status: designed and built 2026-10-18 (M3 lane "Locus, navigation and AI"); implementation notes at
[the end](#implementation-2026-10-18). ROADMAP M3
"Placement by relations: Locus"; research in [AGENT_GAMEMAKING.md](../research/AGENT_GAMEMAKING.md) §1 ("Agents are
bad at raw coordinates and good at relations") and §3. Siblings: [NAVIGATION.md](NAVIGATION.md) (the navmesh it
places on), [PHYSICS.md](PHYSICS.md) (the queries it checks collisions with).

**Goal:** an agent says *where things go in relation to other things* ("on the table, against the north wall, 2 m
apart, facing the door, clear of the path") and the engine solves the positions deterministically, checks them with
physics, and reports every relation it couldn't satisfy as data with the nearest valid alternative. The agent decides;
the engine computes (the same principle as the ID buffer).

## Why

Agents make systematic coordinate errors: wrong heights, off-by-one stacking, ignored axes. Letting a model plan
relations while a deterministic executor computes coordinates beat larger models working in raw 3D, and the largest
error bucket in rearrangement benchmarks is placements that satisfy the relation but collide (about 26 %). So Locus
checks collisions as part of every placement, not afterwards.

## The tool

`world_place {spawn | entity, count, relations, dry_run, on_unsatisfied, footprint, seed}`:
- `spawn`: world_spawn's arguments (names get `_1`, `_2`... when `count` > 1), or `entity`: an existing entity to move
  (with `expect_version`).
- `relations`: 1 to 16, each `{rel, ...}`, hard by default; `"soft": true` makes one a preference.
- `dry_run`: solve and report, change nothing.
- `on_unsatisfied`: `"refuse"` (default: nothing is written, the report says why and what would work) or
  `"alternative"` (write the nearest valid alternative).

The result per instance: position, rotation, the footprint used, what it rests on, and per relation `satisfied` with
the measured value (distance off, angle off). Unsatisfied ones carry `why` and the alternative's numbers; whole-call
hints say what to relax (for example "6 at 2 m spacing don't fit along 9 m; 4 fit, or spacing 1.5 m fits 6").

A write goes through `world_batch`'s path: one journal entry (`batch`), claim-checked, undoable with `world_undo`.

## Relations

| Relation | Means | Geometry it generates |
|---|---|---|
| `on {target?}` | rests on the target's top surface (or whatever is below, `"ground"`) | the target's top face (X/Z); height by downward rays |
| `against {target, side?, gap?}` | its back touches a vertical face of the target | a line along that face, `gap` + half its depth out; faces the other way |
| `along {target \| from, to, offset?}` | on a line: the target's long axis at its base, or `from`→`to`, shifted `offset` sideways | a segment; with `count`, spread evenly along it |
| `between {a, b, t?}` | on the segment between two entities or points (`t` 0..1 fixes where) | a segment or a point |
| `near {target, max, min?}` | within `max` (and beyond `min`) of the target's bounds | a ring around the target |
| `inside {region \| target}` | its footprint inside a box region or another entity's bounds | the region |
| `facing {target \| point \| direction}` | its front (local +Z, or `front`) turns toward it | the yaw |
| `spacing {min, max?, among?}` | at least `min` from the other instances of this call (and from entities tagged `among`) | a constraint |
| `avoid {target \| tag, distance}` | at least `distance` from the target's (or every tagged entity's) bounds | a constraint |
| `on_navmesh {mesh?, reachable_from?}` | on a navmesh, and reachable by path from a point or entity | a constraint (and the polygons as a region) |

Distances are between bounds in X/Z (an oriented box per entity), so "1 m from the wall" means a metre of floor in
between, not between centres.

## The solver

Deterministic, bounded and explainable rather than clever. Per instance:

1. **Resolve targets** into oriented boxes (world corners), points and segments, once.
2. **Generate candidates** from the most constraining generator: a fixed point (`between` with `t`), then segments
   (`between`, `along`, `against`), then areas (`on`, `inside`, `near`, a navmesh's polygons), then the entity's
   current position (a move) or the origin. Segments sample at a quarter of the footprint (at most 256 samples per
   segment), areas on a grid (at most 64 × 64), then a finer pass around the best candidate. A ring of candidates up
   to 2 m outside the generator's domain is always included, so a near miss can be reported as the nearest valid
   alternative.
3. **Height and rotation.** `on` (or, without any vertical relation, the ground: `settle: true`) casts rays down at
   the footprint's corners and centre and rests on the highest hit, so nothing sinks into a slope. Rotation comes from
   `facing`, else `against` (back to the face), else `along` (aligned with the line), else stays as given.
4. **Score.** Each relation measures a violation in metres or degrees (0 when satisfied within 1 mm / 0.5°). Hard
   violations weigh 1,000, soft ones 1, plus small preferences (the middle of `between`, an even share of `along`,
   the closest point of `near`). **Collisions** are checked for every candidate: physics overlap (the footprint box,
   shrunk 5 mm, against non-trigger colliders) and the oriented boxes of drawn entities without colliders; a collision
   is a hard violation whatever the relations say. The lowest score wins; ties go to the earlier candidate (fixed
   generation order; `seed` only offsets the grid phase).
5. **Several instances** are placed one after another; each placed one blocks the next (collision and `spacing`).

**Unsatisfied relations** come back as data: which relation, by how much, why (blocked by an entity, outside the
region, off the navmesh), and the nearest valid alternative (the best candidate with no collision, the generator
domain extended by the ring). With `on_unsatisfied: "alternative"` that alternative is written instead.

## Footprints

The object's local bounding box times its scale: a primitive's unit box, a lathe's profile, a collider if it has one,
or `footprint: {size}`. A model's bounds are known only once its asynchronous build finishes, which depends on
wall-clock time, so a model is placed by its collider or an explicit `footprint`; its built bounds are used only when
the build is already finished and the answer says so (`footprint_from: "model_bounds"`).

## Determinism and bounds (F1, F2, F3)

Candidate order is fixed; arithmetic is IEEE `+ - * /`, `sqrt` and `dmath` (`atan2` for yaw); the physics queries it
calls are sorted by distance then id. A call evaluates at most 16 relations × 64 instances × about 5,000 candidates,
and every candidate's checks are bounded (collision candidates come from the physics broadphase and a list of nearby
boxes gathered once). The response is capped and lists instances in order.

## Not in this design

Relations between instances of one call beyond spacing (stacking a pile, "in a circle around"), optimisation of a
whole scene at once (Holodeck's constraint optimiser), and style rules (`style.toml`) as placement preferences. The
format leaves room for each.

## Implementation (2026-10-18)

Built as designed in `crates/specula-locus` (`geom`: upright boxes; `solve`: the solver and the `Probe` trait) and
`crates/specula/src/locus.rs` (the tool, target resolution, the probe over physics, drawn boxes and the navmesh, the
write). Changes and details:

- *Amendment: settling looks for ground from half a metre above the generator's base*, then from high up if nothing is
  there. Rays from high above landed an object between two points on a crate that stood between them. `on` still rays
  from just above its target's top.
- *Amendment: `inside` with y bounds doesn't switch settling off;* it is a constraint, not a height.
- `along` turns the object's local X along the line (its front, +Z or `front`, faces across it); `facing` overrides;
  `against` turns its back to the face.
- `spacing` and `near` measure the gap between footprints, not between centres.
- Collisions: physics overlap (the footprint box shrunk 5 mm, raised 1 cm) against non-trigger colliders, and the
  upright boxes of drawn entities without colliders around the relations' area (in X/Z; at most 8,192, in a 2 m
  grid; boxes covering more than 256 cells are always checked). `on_navmesh {reachable_from}` floods the navmesh once
  per call. A dry run applies nothing and sees physics as of the last tick. The moved entity's subtree and `ignore` are left out.
- A candidate first gets a lower bound from what doesn't depend on height (the X/Z relations and the preference);
  if that can't beat the best so far, its ground rays, navmesh queries and collision checks are skipped.
- Errors: `unsatisfied` (with the nearest valid alternatives and hints), `no_free_spot` (some instance has no
  collision-free spot near the asked one), `no_placement`, `footprint_unknown`; argument errors name the field. The
  answer lists every relation for up to 8 instances (then only failures) and at most 48 KB of placements in detail.
- Writes go through the swarm commit path as one `batch` journal entry (claims, conflict help, `world_undo`). Spawns
  with `count` > 1 are named `name_1`, `name_2`...

Measured in `scripts/nav_check.py` (dev build): a table against a wall inside a room, a crate on it, four pillars along
a wall, a lamp between two of them, a bench near the table facing it and clear of the pillars, and refusals with hints
and alternatives, each placement within a few milliseconds.
