# Design: physics

Status: researched 2026-10-13; **P1–P3 implemented 2026-10-18** (crate `specula-physics`, engine bridge
`crates/specula/src/physics.rs`, tools `crates/specula/src/physics_tools.rs`); **P4 (joints, ragdolls) implemented
2026-10-18**, with compound colliders and continuous collision against moving bodies (see
[P4 as built](#p4-as-built-2026-10-18)). Foundation
**F4**; stages P1–P4 land in M3 (P4 moved from M4 with ragdolls, owner 2026-10-17) ([ROADMAP.md](../ROADMAP.md)).
What was built, and where it departs from the design below (with why), is in
[Implementation](#implementation-2026-10-18) at the end.

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

## Implementation (2026-10-18)

P1–P3 as designed above, with these changes and details. Each amendment says why.

**Crates and files.** `crates/specula-physics` (pure, `#![forbid(unsafe_code)]`, depends only on `specula-core`):
`shape` (shapes, hull builder, triangle-mesh BVH, heightfields, mass properties), `tree` (dynamic AABB tree), `gjk`
(support mappings, GJK), `contact` (narrowphase), `query` (raycasts, shape casts, overlaps), `solver` (Soft Step),
`space` (the world: bodies, pairs, step, sleep, tunnelling guard, events, hash), `character` (P2). The engine bridge is
`crates/specula/src/physics.rs`; the tools and overlays are `crates/specula/src/physics_tools.rs`. Live check:
`scripts/physics_check.py`; benchmark: `cargo run --release -p specula-physics --example physics_bench`.

**Components (world data).** `collider: {shape: box | sphere | capsule | hull (4–64 points; at most 16 on a moving
body) | mesh (≤ 4,096 vertices,
8,192 triangles; static only) | heightfield (2–129 per side; static only), offset, is_trigger, layer, mask (layer
names it collides with), friction, restitution}`; `body: {kind, mass, velocity, angular_velocity, linear_damping,
angular_damping, gravity_scale, sleeping, ccd, lock_position, lock_rotation, character}`. A collider alone is static.
Shapes are in local (scaled) units. Defaults are left out of the JSON and the hash, so worlds from before keep their
files and hashes. `world_env {physics: {gravity, substeps, sleep, plane_2d, contact_hertz, enabled}}`.
- *Amendment: layers need no config table.* Layer names map to mask bits on first use (64 names at most; past that a
  collider is refused with a structured error). Agents still use names only. A table can come with game config later.
- *Amendment: the character controller is part of `body` (`body.character`)*, not a third component: a character is a
  kinematic body walked by the controller, and keeping it in `body` kept the world model's reserved components as they
  were.
- *Not done: auto-colliders from models.* Model builds are asynchronous, so a collider that appears "when the build
  finishes" would make physics depend on wall-clock time (F2). The deterministic route is a tool that fits a collider
  from the built model and writes it into the entity as data (a journaled change at a tick boundary). Deferred.

**Broadphase.** The dynamic AABB tree as designed, in two trees (static, moving) as Box2D v3 does. Pairs are only
searched for bodies whose fat box changed. *Amendment:* moving bodies' fat boxes also include one step of motion at
their velocity, so pairs exist before fast bodies arrive. Tree traversal order never reaches the simulation: every
result is sorted by key.

**Narrowphase.** As designed: closed-form sphere/capsule pairs (two points for parallel capsules); GJK on cores for
spheres and capsules against polytopes; SAT with Gauss-map pruning plus clipping for polytope pairs, reduced to four
points; per-triangle tests for meshes and heightfields with internal-edge flags (flat or concave seams snap to the
triangle normal). Plus:
- *Amendment: no EPA.* When a sphere's or capsule's core is inside a polytope, a SAT over the polytope's faces (and
  segment × edge axes) finds the shallowest exit exactly; EPA isn't needed for any pair we have.
- *Amendment: a normal per contact point*, not one per manifold, so a body on several mesh triangles keeps one
  manifold (Bullet's choice rather than Box2D's).
- A box-box fast path (the 15-axis oriented-box test, then the same clipping), checked against the general test on
  random poses. Boxes are built directly (6 faces, 12 edges), so any positive size works; half extents are at least
  0.1 mm.
- *Amendment: a per-pair memory for polytope pairs* (temporal coherence): while apart, the pair re-checks last tick's
  separating direction first (one support scan each); while touching, if the relative pose moved less than 5 mm and
  about 1° since the last full test, it reuses that face or edge and only re-clips. Hull pairs cost edges × edges, so
  hulls on moving bodies are capped at 16 points (a structured error says so); static hulls take up to 64.
  `specula-model` fits hulls of up to 48 points; `collider::fit_limited` with its `MAX_MOVING_HULL_POINTS` (16, the
  same number, checked by an engine test) fits one for a moving body.
  The memory changes results (a reused feature can differ from a fresh test by a few millimetres and some degrees of
  normal at the edge of the 5 mm / 1° window), so it is **state**: it is in the physics hash beside the warm-start
  impulses. Changing a collider in place clears both on all of its pairs, and a remembered index the shape doesn't
  have means a fresh test, never "no contacts".
- Against meshes, the triangles near a body are visited nearest its own vertices first (so a box's corners are
  supported first), up to 256 a tick; more is a structured `budget` error. Shape casts against meshes sweep the BVH
  nearest-first, pruned by the best hit so far, with no cap.
- Feature ids per point carry warm-start impulses across ticks.

**Solver.** Soft Step as designed: substeps of integrate, warm start, solve (soft, push-out capped at 3 m/s), integrate
positions, relax; then restitution. Contacts 30 Hz, damping ratio 10 (static contacts twice as stiff), friction as a
disc per point. Jacobian rows are precomputed per step. Joints (P4) slot in beside contacts (see `solver.rs`'s docs).
- *Amendment: `substeps` counts per 1/60 s* and scales with the tick rate. The engine ticks at 30 Hz without a game;
  four substeps per 30 Hz tick made a 10-box tower sway. Scaling keeps the substep length (the solver's stiffness) the
  same at any tick rate.
- *Amendment: the speculative distance follows the closing speed.* Box2D uses a fixed 2 cm plus continuous collision
  for fast bodies. At 30 Hz a bouncing ball between those (moving 15 cm a tick, under the guard's threshold) sank
  ~10 cm into the floor; with the margin at 2 cm + closing speed × dt (capped at 42 cm) it never sinks.

**Islands and sleeping.** Union-find each step over touching awake dynamic pairs; an island at rest for 0.5 s sleeps
whole, and wakes whole when touched, when a support is removed, or when an agent moves or changes a member. Moving a
static or kinematic body by hand wakes what overlaps it where it was and where it is now; a small move (up to two of
its own sizes) wakes its whole swept box, a teleport doesn't wake the ground it jumps over. Only awake
bodies and their pairs are visited (an awake set and a contact slab with a pair-key index), so sleeping bodies cost
nothing.

**Continuous collision: a tunnelling guard.** As in Box2D v3: a dynamic body that moved more than half its thickness
(the radius of the largest sphere it surely contains) in a step casts that sphere from where it was to where it is
against static colliders, and stops at the first hit (`ccd` events). Dynamic-versus-dynamic tunnelling isn't guarded;
such bodies are reported as fast (`physics_check no_tunnelling`, `physics_explain`). A full time-of-impact solver for
bullets is still "Later".

**Character controller (P2).** As designed (sweep-and-slide, step up, slope limit, snap, platforms, capped pushing),
plus: the edge of a walkable surface counts as ground (a rounded collider resting on a corner gets a tilted normal; a
short ray just inside the edge checks the surface), and on the ground a rounded collider rides over edges lower than
the step height. The controller's memory (grounded, ground) is part of the physics hash.

**Engine integration.** Each fixed tick, after the hosted game's tick: entities whose components or world transforms
changed since the last tick are synced (through the world's change log; a full compare when the log doesn't reach
back, which keeps physics state); characters walk; the space steps; bodies it moved are written back (position,
rotation, body velocity and sleep state, converted into the parent's frame), journaled as kind `physics` like a game's
changes (after-only, coalesced, exempt from claims). Kinematic bodies that game code moves by setting their position
sweep there (pushing what they meet) unless the jump is over 2 m (a teleport). Transient entities take no part.

**Determinism.** Twin runs (two engine processes, same calls) end in the same world hash and physics hash
(`scripts/physics_check.py`); scenes built in any insertion order step to bit-identical states (unit test); the journal
replays physics changes exactly (`world_hash verify`). `world_hash` reports the physics state hash per tick next to the
world's. *Limit:* hidden state (warm-start impulses, sleep timers, the exact rotation behind `rotation_deg`) isn't saved
in world files, so a reloaded world continues deterministically from the load, but not bit-identically to the run that
saved it. Save games carry all of it, so a loaded save continues bit-identically ([SAVES.md](SAVES.md)); bodies
remember the scale their collider was built at for it (a world matrix's column lengths jitter in the last bit as a body
turns, and the space keeps the shape it was built with).

**2D.** `world_env {physics: {plane_2d: true}}` keeps dynamic bodies in the XY plane (Specula's 2D plane) turning about
Z only, by locking axes in the solver. Shapes stay 3D (a circle is a sphere, a 2D box a thin box), so 2D games get the
same solver, queries and checks. Tile-map colliders are a follow-up.

**Agent tools.** `physics_raycast`, `physics_cast`, `physics_overlap` (the design's `physics_query`, split by kind so
each schema stays small), `physics_contacts`, `physics_events` (cursor), `physics_info`, `physics_explain` (the
"why did it fall through" answer: contacts, neighbours and the layer mask that filtered them, the tunnelling
indicator), `physics_check` (scenario assertions as data), `physics_impulse`. Sight overlays `colliders` and
`contacts`. `perf_frame` has a `physics` section (stage times, counts, budget errors, automatic checks);
`perf.example.toml` budgets the `frame/sim/physics` zone.

**Bounded work.** Bodies (20,000) and pairs (65,536) are capped, as are triangles per mesh pair (256, nearest first),
events per step and every query result; a cap that bites is a structured `budget` error in `physics_info` and
`perf_frame`. The physics hash caches the part outside the awake set (static and sleeping bodies and their pairs), so a
sleeping world costs nearly nothing to hash too; moving bodies' world-hash share is written field by field (no JSON).

**Measured (2026-10-18, i9-13900HX, one thread, release, shared machine so ±30%).** `physics_bench` at 60 Hz, 4
substeps: 20-row pyramid settles (top box 3 cm low) and sleeps; 10,000 sleeping bodies 0.001 ms a tick and 0.009 ms to
hash (target: nearly nothing); 100 characters on a heightfield 0.010–0.015 ms each (target 0.05); 1,000 awake mixed
bodies piling up 3.9–4.2 ms a tick (target 2 ms: **not met yet**: narrowphase 1.5–1.7 ms, solver 1.75–1.8 ms; the
solver is compute-bound, as packing its points densely changed nothing). 1,000 falling 16-point hulls: narrowphase
about 15 ms a tick while they land (58 ms before the per-pair memory), far less once they rest. The world hash of 1,000
moving boxes: 0.4–0.5 ms a tick (9.3 ms when bodies were hashed as JSON). In the engine (`bridge_profile`, an ignored
test in `crates/specula/src/physics.rs`), 1,000 awake boxes: the space's step 2.4 ms (its stages 2.2 ms), write-back
1.05 ms (of which `World::update` 0.7 ms and the Euler conversion 0.09 ms), the physics hash 0.28 ms, and outside the
physics tick the change snapshot and journal 0.56 ms. The write-back skips bodies whose pose bits and body are
unchanged, reuses its buffers and computes the world rotation only when the position is unchanged; the rest is the
world's generic update and its full-entity snapshots. The next steps for the 2 ms target are
friction at the manifold centre instead of per point, SIMD rows (Box2D's wide solver) and a deterministic parallel
narrowphase.

**Deferred (at P1–P3).** Since done in P4 (below): joints and ragdolls, continuous collision against moving bodies,
compound colliders, a parallel narrowphase. Still open: auto-colliders from models (`ragdoll_set {fit}` shows the
deterministic route: measure the built mesh, write the result as data); tile-map colliders for 2D; the
1,000-awake-bodies budget; SIMD and parallel solving.

## P4 as built (2026-10-18)

**Joints** (`crates/specula-physics/src/joint.rs`): fixed, hinge, ball (an elliptical swing cone and a twist range),
distance (rod `[L, L]`, rope `[0, L]`, a spring towards a rest length) and prismatic. Each joint has a frame on each
body (anchor, axis = frame x, reference = frame y), in the body's frame or the world's (`a: None`). Limits, a friction
torque (hinge and ball), and motors: a speed (hinge, prismatic), a position (hinge angle, prismatic offset) or a
rotation (ball, fixed: frame B in frame A), each clamped by `max_force`; position and rotation drives are soft springs
(Hz, damping ratio). Solved in the Soft Step solver as Box2D v3 does: warm start, solve with bias, relax without it,
joints before contacts in every pass, softness `joint_hertz` (60 Hz, damping 5, capped at a quarter of the substep
rate). Point constraints are one 3×3 block, angular locks one 3×3 block, hinge axis alignment two rows, limits
one-sided rows with speculation, anchors and axes recomputed from each body's motion within the step. Jointed bodies
don't collide unless `collide_connected`; joints link islands (they sleep and wake together; a joint to a moving
kinematic body keeps its partner awake). A joint whose body is missing waits (`inactive: missing_body`); its state is
measured after each step it is solved in (`JointState`: gap, misalignment, angle or offset, swing, excess past its
limits, force and torque), and its description, warm-start impulses and drive targets are in the hash.
- *Amendment: joint stiffness is relative to the two bodies a joint links.* A soft spring is stiff against those
  bodies' inertia, not against what hangs below them, so a soft drive tracking an animation through a chain of light
  parts sags; a powered ragdoll's drives run at 60 Hz with a torque cap instead (see ANIMATION.md).
- *Measured (`tests/joints.rs`):* a 10-link ball chain holds within 2 cm while it swings, then sleeps; a door's motor
  stops it 0.05° past its 100° limit; a rope holds 3.0001 m; a rod holds 1 m within 1 cm; a slider stays on its rail
  within 5 mm; a weld keeps its offset; a falling-hinges scene is bit-identical in any insertion order. A weld landing
  hard stretches for a tick (7.7 cm at 7 m/s in `joints_check.py`), which `physics_check joint_error` with `since_tick`
  separates from how joints hold at rest.

**Engine joints** (`crates/specula/src/physics/joints.rs`, tools `crates/specula/src/joint_tools.rs`): a joint is an
entity (a group, by default under body b, tagged `joint`) whose `props.joint` holds it as data, with frames in each
body's local scaled coordinates (like collider offsets), so joints are journaled, undoable, claimable, saved and
hashed. The bridge rebuilds a joint when its entity changes and when a body's scale does; unchanged joints keep their
warm start. `physics_joint` makes, changes or removes one from world-space anchors and axes (it needs the right to
change both bodies); `physics_joints` lists joints with their state; `physics_check` takes `joint_error` and
`joint_limits`; `physics_explain` lists an entity's joints; the `joints` sight overlay draws anchors, axes and hinge
limit arcs, coloured by how each holds. Live check: `scripts/joints_check.py` (twin runs end in the same world and
physics hashes; undo removes a joint; the journal replays).

**Ragdolls** (`crates/specula/src/physics/ragdoll.rs`; design in [ANIMATION.md](ANIMATION.md#a4-as-built-2026-10-18)):
capsules and joints built from a skeleton, limits from the anim graph. Transient entities' ragdolls are **cosmetic
bodies** (`BodyDesc::cosmetic`): they collide with static and kinematic bodies and with each other, never with
simulated dynamic bodies or triggers; queries don't see them; they have their own caps; they are left out of the state
hash, so the simulation is bit-identical with or without them (tested). Parts of one ragdoll share a collision group
(`BodyDesc::group`).

**Compound colliders** (`Shape::Compound`; engine `collider {shape: compound, parts: [...]}`): 1–16 convex parts
(box, sphere, capsule, hull) with offsets and rotations, as one body: mass and inertia from the parts' volumes
(parallel axes); contacts from every part pair whose boxes meet, reduced to four points (the deepest, then each time
the farthest from those kept; feature ids carry the parts, so warm starts follow them); rays and casts per part. Hull
parts on moving bodies keep the 16-point cap. Characters refuse compounds (the controller sweeps one convex shape).
Under an uneven scale a rotated part keeps its own proportions (close, not exact).

**Continuous collision against moving bodies.** The tunnelling guard also casts a fast body's inner sphere against the
dynamic and kinematic bodies it pairs with that weren't fast themselves that step, where they ended up (Box2D v3's
bullets against non-bullets). Two fast bodies still don't stop each other (that would depend on which went first);
they stay listed as fast. A 300 m/s bullet stops at a free-floating 5 cm plate and knocks it on (tested).

**Write-back fast path.** Physics writes bodies back through `World::set_motion`: the position, rotation and body
component when only its motion changed, recorded by the snapshot recorder as a `Change::Pose`, which now carries the
body (replays put it back; physics changes were never undoable), instead of a copy of the entity. Journal replay, the
desync marks (a `body` field) and the world hash layout are unchanged. Measured in `bridge_profile` over the same
1,000 bodies in one run: `World::update` and its snapshot 3.6 ms, `set_motion` and its snapshot 1.9 ms (46% less).

**Parallel narrowphase, opt-in.** `Settings::threads` (default 1) splits a big narrowphase (768 pairs or more) over up
to that many scoped threads, each filling its own slice of results in pair order, merged in that order: the same states
on any thread count (tested). On free cores it takes about a quarter off a 1,000-body narrowphase (1.65 vs 2.24 ms,
best of six), but threads started each step can wait tens of milliseconds for a busy machine to schedule them (the
loaded test machine saw 50 ms steps), so it stays off until there is a persistent worker pool.

**The 1,000-awake-body budget: not met.** Best of six runs on the shared i9-13900HX under 76% CPU load from other
sessions: 5.0 ms a tick (narrowphase 1.65 with four threads, 2.4 on one; solver 2.1–2.3) against main's 5.2 ms;
uncontended runs earlier measured 3.9–4.2 ms. Tried and left out: friction at the manifold centre (two tangent rows
and a twist row per pair instead of two rows per point) changed nothing measurable (solver 2.13 vs 2.14 ms), so
per-point friction stays. The solver is compute-bound scalar code; the route to 2 ms is Box2D v3's: colour the
constraint graph, solve each colour wide (SIMD-style rows), on a persistent deterministic worker pool.
