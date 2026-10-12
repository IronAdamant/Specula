# Design: physics

Status: researched 2026-10-13, not yet implemented. Foundation **F4**; stages P1–P3 land in M3 and P4 in M4
([ROADMAP.md](../ROADMAP.md)).

**Goal:** the physics most games need, written from scratch (zero-dep):
- deterministic ([DETERMINISM.md](DETERMINISM.md))
- cheap enough for integrated GPUs and handhelds (the CPU budget matters on those too)
- explainable to agents through data

## What games actually need, in order

| Stage | Feature | Used for | Size |
|---|---|---|---|
| **P1** | Static colliders, **queries** (raycast, shape cast, overlap), **triggers** | Line of sight, shooting, picking up, "player entered the room" | Small–moderate |
| **P2** | **Kinematic character controller** (capsule) | Walking, slopes, stairs, jumping, not passing through walls | Moderate |
| **P3** | **Rigid bodies** (sphere, capsule, box, convex hull), contacts, friction, restitution, sleeping | Crates, debris, thrown objects, stacking | Large |
| **P4** | **Joints** (hinge, ball, fixed, distance, limits, motors), **ragdolls** | Doors, chains, bridges, death animations | Moderate |
| Later | Continuous collision for fast bullets, vehicles, cloth, 2D variant | Per game | Per feature |

Most games are well served by P1 and P2. P3 adds polish, and P4 adds spectacle. We build in that order so the game is
playable early.

## Pipeline per simulation tick

```
1. Apply forces (gravity, game forces) → integrate velocities
2. Broadphase: find pairs whose bounding boxes overlap
3. Narrowphase: exact contact points for each pair → contact manifolds
4. Islands: group touching bodies; put resting islands to sleep
5. Solve: Soft Step (soft constraints + sub-stepping + relax pass)
6. Integrate positions; update the broadphase
7. Emit events: contact begin/end, trigger enter/exit
```

### Broadphase: dynamic AABB tree

A bounding-volume hierarchy of fattened bounding boxes, incrementally updated, with tree rotations to stay balanced.
It is the approach used by Box2D, and it handles mixed static and moving worlds well. The same tree serves raycasts
and overlap queries (P1), so P1 builds the broadphase that P3 reuses.

### Narrowphase: per shape pair

- **Sphere / capsule** pairs are closed-form (closest points between points and segments). They're the fastest and
  most robust, which is why capsules are the default for characters.
- **Box–box**: separating axis test (15 axes) plus clipping to build a manifold of up to 4 points.
- **General convex**: GJK for distance and overlap, then EPA or SAT for penetration depth and normal.
- **Triangle meshes** (static level geometry only): the per-triangle convex tests, with fixes for internal edges so
  bodies don't snag on seams between triangles.
- **Manifolds persist** across ticks, with contact ids, so the solver can warm-start. This is key to stable stacks.

### Solver: Soft Step

Erin Catto compared solver designs in Solver2D (2024), including TGS_Soft, XPBD and others. He chose **soft step** for
Box2D v3: soft constraints plus sub-stepping, with a relax pass to remove the energy the stabilisation added
([Catto, "Solver2D", 2024](https://box2d.org/posts/2024/02/solver2d/)). His Box3D carries the same solver into 3D.
Reasons to use it:
- **Stiff and stable stacking** at low cost. Sub-stepping beats adding iterations.
- **Soft constraints have physical parameters** (frequency in Hz, damping ratio) instead of magic numbers. That makes
  tuning explainable to agents, e.g. "door hinge: 30 Hz, damping 1.0".
- **Published, simple, and proven deterministic** in Box2D.

Background: Catto's GDC 2011 talk on soft constraints
([PDF](https://box2d.org/files/ErinCatto_SoftConstraints_GDC2011.pdf)).

### Sleeping and islands

Bodies at rest for a short time go to sleep and cost nothing until touched. That's essential for big worlds and
low-end CPUs.

### Character controller (P2)

A **kinematic capsule**, not a rigid body. Game logic gives a desired velocity, and the controller does
**sweep-and-slide**:
1. Sweep the capsule along the motion.
2. On a hit, move to the contact and remove the motion into the surface.
3. Slide along the rest, up to N iterations.

It also handles:
- **step-up** (climb stairs below a height)
- **slope limit** (walkable vs. too steep)
- **ground snapping** (stick to the ground going downhill)
- **moving platforms** (inherit platform velocity)
- **pushing rigid bodies** with a capped force

*Why kinematic:* players expect exact, responsive control. Rigid-body characters feel floaty and tip over.

## Data layout and engine integration

- **Components** on entities:
  - `Collider { shape, offset, is_trigger, layer, mask, material }`
  - `Body { kind: Static | Kinematic | Dynamic, mass, damping, … }`
  - `CharacterController { … }`

  These are reserved in M1 (F4), so world files don't change shape later.
- **Physics state lives in compact arrays** (structure-of-arrays, stable indices) inside the physics system. Entity
  transforms are written back each tick. Ordering is by stable id, per the determinism rules.
- **Collision layers and masks** come from a named table in the game's config ("player", "enemy", "debris"), so
  agents use names, not bit numbers.
- **Auto-colliders:** procedural and imported assets get a generated collider (box fit, capsule fit or convex hull)
  unless one is given. See [PROCEDURAL_MODELLING.md](PROCEDURAL_MODELLING.md).
- **Threading:** single-threaded first. When we parallelise, use deterministic merges as in rule 7 of the determinism
  doc.

## Agent-facing tools

- **`physics_query`**: raycast, shape cast and overlap from an agent, returning hits as data.
- **Debug overlay in captures**: collider wireframes, contact points and normals, sleeping state, and the character
  controller's ground normal.
- **Structured explanations**, e.g. "Why did the crate fall through the floor?" returns:
  - tick
  - pair
  - penetration depth
  - relative speed vs. thickness (the tunnelling indicator)
  - which layer mask filtered the pair
- **Scenario tests as data**: an agent defines a scene, runs N ticks headless, and asserts (e.g. "tower of 10 boxes
  still standing after 10 s").

## Test suite

These are written as headless scenarios with hashes:
- shape-pair unit tests
- a 10-box tower standing for 600 ticks
- a 20-row pyramid going to sleep
- a hinge chain
- a "falling hinges" determinism scene, hashed in CI on msvc, gnu and ubuntu, as Box2D does
- a character walking a stair, slope and ledge course with expected end positions
- a fast-sphere vs. thin-wall tunnelling test, which sets when continuous collision is needed

## Performance budgets (F3)

Targets on the reference integrated GPU machine's CPU, to be confirmed in M3:
- 1,000 awake bodies under 2 ms per tick.
- 10,000 sleeping bodies costing nearly nothing.
- The character controller under 0.05 ms per character.

`perf_frame` reports physics time per stage.

## References

- Catto, ["Solver2D"](https://box2d.org/posts/2024/02/solver2d/) and
  ["Determinism"](https://box2d.org/posts/2024/08/determinism/), box2d.org, 2024.
- Catto, [Soft Constraints, GDC 2011](https://box2d.org/files/ErinCatto_SoftConstraints_GDC2011.pdf).
- Jolt Physics: a production 3D engine (Horizon Forbidden West) with a cross-platform determinism build option
  ([build docs](https://raw.githubusercontent.com/jrouwe/JoltPhysics/master/Build/README.md)). A good reference for
  3D-specific details: convex radius, character controllers and mesh shapes.
- Christer Ericson, *Real-Time Collision Detection* (2005): the standard reference for the shape tests.
- Glenn Fiedler, ["Fix Your Timestep!"](https://gafferongames.com/post/fix_your_timestep/).

We read these for the techniques and write our own code. No source is copied, which keeps both zero-dep and licensing
clean.
