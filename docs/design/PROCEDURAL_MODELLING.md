# Design: procedural modelling and the taste loop

Status: designed 2026-10-13. Tier 1 built in M2 (2026-10-13); tiers 2–4 and procedural materials
built for M4 on 2026-10-22: see [Tiers 2-4 as built](#tiers-2-4-as-built-2026-10-22)
([ROADMAP.md](../ROADMAP.md)).

**Goal:** agents author original, on-style, efficient 3D content directly in the engine, with no external modelling
tool required. The owner can still import or hand-edit whatever they like.

This is where a small studio closes the art-team gap. It is also where an agent's taste matters most: generating a
mesh is the easy part, and judging and refining it is the hard part.

## Core idea: the recipe is the source, the mesh is a build product

An asset is a **text recipe**: a short list of operations with parameters, kept in the game repo next to the world
files. The engine builds the mesh from it deterministically, along with its LODs and collider.

```json
{ "name": "lamp_post", "style": "harbour_town",
  "ops": [
    { "op": "lathe",   "profile": [[0,0],[0.12,0],[0.10,0.2],[0.06,0.25],[0.05,3.2]], "segments": 12 },
    { "op": "bevel",   "edges": "sharp", "width": 0.01 },
    { "op": "attach",  "asset": "lantern_small", "at": [0,3.2,0] },
    { "op": "colour",  "palette": "iron_dark" }
  ] }
```

*Why recipes, not raw meshes:*
- **Small and readable.** An agent can read, critique and edit a 10-line recipe. It can't do that with 40,000
  vertices.
- **Editable by intent.** "Make it taller" is a one-number change, and siblings stay consistent.
- **Diffable and mergeable** in git, and journaled and undoable in the engine.
- **Optimisation comes free.** LODs, colliders and budgets are derived on every build, never forgotten.
- **Variation is cheap.** One recipe with a seed and ranges gives 20 distinct but coherent crates.

Imported meshes (glTF) remain first-class. A recipe can `import` one and keep modifying it.

## Mesh kernel

- An **indexed triangle mesh** for rendering, plus a **half-edge mesh** for operations that need adjacency (bevel,
  extrude-faces, subdivision).
- **Normals** use an angle threshold, so faces meeting at a sharp angle get hard edges and gentle ones are smoothed.
  This gives crisp hard-surface objects without manual smoothing groups.
- **UVs:** box and triplanar projection by default. Most stylised materials use triplanar mapping in the shader, so
  many assets need no unwrap at all. Proper unwrapping is a later tool.
- **Deterministic:** recipe + seed always gives the identical mesh (bit-exact), following the
  [determinism rules](DETERMINISM.md). Asset hashes are therefore stable, and caching is safe.

## Operation tiers

| Tier | Operations | Good for | Milestone |
|---|---|---|---|
| **1** | Primitives (box, cylinder, cone, sphere, torus, capsule, plane with subdivisions); transform; **extrude** (2D outline → solid, with optional taper); **lathe** (spin a profile); **sweep** (shape along a path/spline); **loft** (between profiles); **bevel/chamfer**; array and mirror; attach (compose recipes); vertex colour by palette | Architecture, props, furniture, columns, pipes, rails, barrels, vases, signs | **M2** |
| **2** | **Booleans**: robust mesh CSG for hard surfaces (cut windows, doors, sockets) | Buildings, machinery | M4 |
| **3** | **SDF modelling**: shapes as distance functions, smooth union/subtract, noise displacement, then meshed with surface nets or dual contouring and simplified | Rocks, cliffs, terrain features, organic props, stylised creatures | M4 |
| **4** | **Generators**: scatter with rules (Poisson-disk placement, slope and height masks); building/facade grammars (floors → bays → windows); terrain heightfields with erosion; road and path layout from splines | Whole districts, forests, landscapes | M4 |

Notes:
- **Bevel is in tier 1 on purpose.** Real objects never have perfectly sharp edges. A small chamfer catches light and
  makes a primitive read as a crafted object. It is the cheapest large quality win in stylised 3D.
- **Booleans are notoriously fragile** in floating point (slivers, holes, non-manifold results). The plan:
  - exact plane-based CSG with robust predicates for hard surfaces
  - SDF booleans for organic work, where small inexactness is invisible
  - every boolean result validated (watertight, no degenerate triangles) before acceptance

## Tiers 2-4 as built (2026-10-22)

Everything below is recipe data (`model_build` builds it on the model workers, never the engine
thread) plus three agent tools. Every field is validated with a path (`parts[0].shape.of[2].ops[0]`),
an error code and a reason; every size that drives work or memory is capped and charged before the
work runs; everything is deterministic (exact integer predicates, the crate's own trig and noise,
ordered containers, per-pixel-independent threads). `cargo test -p specula-model` covers it
(`tests/procgen_recipes.rs`, `tests/sepulcra_assets.rs` pins the Sepulcra assets' hashes);
`scripts/procgen_check.py` proves it live.

### Tier 2: booleans (`crate::csg`)

```json
"shape": { "op": "difference", "bevel": 0.012, "bevel_segments": 1, "of": [
  { "op": "extrude", "outline": { "arch": [0.52, 0.86] }, "height": 0.16, "ops": [ { "op": "rotate", "deg": [-90, 0, 0] } ] },
  { "op": "box", "size": [0.34, 0.22, 0.08], "ops": [ { "op": "move", "by": [0, 0.2, 0.1] } ] } ] }
```

- Shapes `union`, `difference` (the first minus the rest) and `intersection`; operands are any
  shapes (booleans nest) with their own `ops` and `when`. Operands must be closed solids.
- **Exact plane-based CSG.** Each tree is snapped once to an integer grid (±2^18 steps across its
  bounds: micrometres for a prop), every triangle becomes a support plane plus edge planes, and every
  vertex stays an intersection of three input planes, so nesting never loses precision. The BSP
  clipping (csg.js's scheme) classifies with a 256-bit orientation determinant behind a proven
  float filter: coplanar faces (boxes sharing a face, a cutter flush with a wall) are decided exactly.
  Disjoint operands are united by concatenation, and many disjoint cutters are cut with one tree of
  the target (a wall's windows in one pass).
- **Output:** exact welding (candidates confirmed against the other point's planes), T-junctions split
  exactly, pieces of one face re-merged and triangulated with their holes (hole bridging, ear
  clipping that respects collinear points), straight-edge points dropped, then sub-20 µm features
  collapsed or flipped away. The result is checked closed (every directed edge balanced); an open
  result is an `InvalidMesh` error with the count. Colours and bevel tags come from the operand each
  piece was cut from, so a cutter can colour its cut faces.
- **Bevels on boolean edges** (`bevel` metres, `bevel_segments` 1 = chamfer, up to 8 = rounds): the
  edges where faces from different operands meet are chained into runs, each chain swept into one
  closed tube (mitred at joints, squared at chain ends) and applied in the same exact plane space:
  cut from convex edges, added into concave ones. A tube that would fold on itself is skipped and
  counted. When the pass's booleans fail, each tube is tried on its own and only those that fail
  alone are left sharp (counted in `skipped`); `bevel_failed` says so only if even the rest fail.
  Concave fillets are costly where curved operands meet (a ring on a cross more than tripled its
  triangles), so bevel such parts before the union instead.
- **Limits:** 24,000 operand triangles, 200,000 live polygons, the build's work budget (below), 8
  levels of nesting, 64 operands, 2,048 bevel runs.
- **Degenerate input and output:** an operand that loses triangles on the grid while being only a
  few grid steps across is refused (a 1 cm cut on a 10 km slab used to vanish silently); any other
  triangles the grid drops are counted (`dropped_triangles`). A connected piece of the result whose
  mean thickness (2·|volume| / area) is under 40 µm is a sliver: it is dropped and counted
  (`slivers_dropped`), and a result that is all slivers (a solid minus itself nudged by a micron, or
  turned 0.001°) is refused as having no volume.

### Tier 3: SDF modelling (`crate::sdf`)

```json
"shape": { "op": "sdf", "resolution": 40, "tree": { "op": "union", "smooth": 0.12,
  "displace": { "amount": 0.03, "scale": 0.15 }, "of": [
    { "op": "ellipsoid", "radii": [0.42, 0.28, 0.34], "color": "carved_shadow" },
    { "op": "sphere", "radius": 0.2, "at": [-0.05, 0.3, -0.04], "color": "moss" } ] } }
```

- Primitives `sphere box cylinder cone capsule torus ellipsoid halfspace`, **centred on `at`** (mesh
  shapes rest on y = 0; sculpts are placed point by point); groups `union difference intersection`
  with `smooth` (Quilez's polynomial smooth minimum, colours blended the same way).
- Modifiers on any node: `at rotate scale stretch mirror repeat elongate twist bend round shell
  displace color`. Uncoloured nodes take the part's colour. `stretch` is a scale for each axis
  (`[1, 1, 0.6]`: a section as broad and as deep as it is measured); the surface is exact, and the
  field away from it is a lower bound of the distance. `rotate` turns a node as a mesh step's
  rotation turns a mesh (X, then Y, then Z). **Until 2026-11-19 it turned the field the opposite
  way to the node's bounds** (the rotation was used where its inverse was needed): unseen on a shape
  that looks alike turned either way, and a capsule lying along x and turned a quarter turn about z
  was meshed at a tenth of its volume. A reviewer of another change found it. The seven recipes that
  turn such nodes had their angles converted so that each looks as it did (`grave_rock`,
  `ruined_wall`, the two statues; the fidelity set's runestone, skeleton and tree at T3): measured
  before and after, the Sepulcra ones differ by a few triangles in thousands and a millimetre in
  size, because their grids now follow the right bounds; captured in the engine before and after,
  a model that did not take the pictures found each of the four props the same from four sides.
  **Three nodes of the fidelity set were put back as written**, on the review of the conversion:
  the runestone's grooves (its glows, mesh parts that were always turned the right way, were
  placed by the grooves' angles as written, so the old engine had drawn groove and glow crossed),
  the skeleton's upper arm (written to run from the shoulder to the elbow), and the skeleton's
  pelvis. The recipe could not say which way the pelvis was meant, so both were built and looked
  at beside Richer's plate of the trunk's skeleton from behind, at the owner's word ("I'd like you
  to experiment on both"): as written its two wings rise up and outward with the spine coming down
  between them, as the hip bones do; as converted they drooped into a dome. A model that did not
  take the pictures confirmed it. There the mend shows what the recipes meant.
- **Roundness as one number** (M6 part 4, 2026-11-18): `roundness`, 0 to 1, on a `box` and a
  `cylinder` here, and on a `rect` outline (the profile of an extrusion, a loft or a sweep). 0 is the
  hard shape, 1 its fully round form (the ellipsoid as wide, as tall and as deep; the ellipse), and
  every value between is a valid shape with flat sides and corners that tighten as the number falls
  (a superquadric, a superellipse: Barr, 1981). The shape keeps its sizes whatever its roundness, so
  it can be turned up or down without anything else moving. **What it costs:** a roundness
  of 1 is the cheap one (no power to take: six times a plain box's sample); any other value solves
  for the nearest point with a dozen to twenty powers a sample outside the shape, over twenty times
  a plain box's. So a cube of roundness 0.3 builds at a resolution of 64 on under half of the work
  limit and is refused at 96, where the same cube at roundness 1 takes a third of the limit (a test
  holds this); a thin board has an eighth of a cube's samples at the same resolution. **A very thin
  rounded shape costs more a sample than a thick one**: down to a twentieth as thick as it is
  long a sample costs what a cube's does, and from there to a two-thousandth the weight rises to
  about 1.7 times (a foil, a hair: more of their samples take more steps, and up to one in eight
  is left to the sure way). The weights are **set against the plain primitives' own rate, not
  against a time**: a build at the work limit is to take what a plain box cut from a plain box
  takes there, whatever the machine and its state (2026-11-19, release, one thread: that plain
  cut ran at 1.01 to 1.07 ns a unit of work, a 2 m box with a rounded cube, plate or needle cut
  out of it, whose every sample is outside the rounded shape, at 0.7 to 1.2, and with a foil or
  a hair of a nine-thousandth at 0.65 to 1.04 by the thin weight). It is not the modifier `round`
  (a swelling by a radius in metres, on any node) nor `corner` (an edge radius in metres, which cannot
  be given with it). A rounded cylinder has one radius. On an outline that is not a `rect` it is
  refused, as `segments` is where it would do nothing.
  - **Outside the shape its field is the distance itself**, to the nearest point of the rounded
    surface, to a few parts in a hundred thousand: so `round`, `shell` and a blend of any size
    are measured from that surface on every side. It took three reviews. The first field was in
    the shape's own units (`round` swelled a thin board by its metres times the ratio of each
    side to the thinnest, and it was infinite a few sizes away at a low roundness). The second
    was the distance to the plane that touches the shape where the gradient points, and the
    third to three such planes: right along the axes and wrong beside a thin shape's edge,
    where a swelling of a centimetre came out up to five (and at roundness 1 still 4.7 after
    the third, with a step in the field where the extra planes were switched on). Now the
    nearest point is solved for: the point `x` of the surface with `p - x` along the surface's
    normal there. First by Newton's steps on the coordinates and the one number that ties them
    together, in double precision, believed only when it has arrived; otherwise by the sure way,
    steps on that one number with each coordinate's own equation solved inside (both curves are
    convex, so those steps cannot pass the answer). **A fourth review** found the sure way alone
    too dear far from the shape (a cut by a rounded plate ran 3.8 s inside the 1.5 s budget) and,
    at a roundness of 0.05, able to stop early with a step in the field. **A fifth** found the
    first steps giving up far from a rounded needle (71 steps a sample, a cut that ran 3.6 s):
    they are now taken on each share **to the power less one**, in which every equation is
    nearly a straight line however low the roundness, each from the most its own equation
    allows. A test holds a plate, a needle, a cube and a board, at six roundnesses, to under 25
    steps a sample in a 2 m box with no more than one in twenty left to the sure way, the
    reviews' points to the distance, and scans out to 100 m for steps. A test holds the field
    against a distance found by search, on six shapes to 100:1 at six roundnesses, and swollen
    meshes against the unswollen surface at every roundness, for swellings to three times the
    thinness. **A sixth** (2026-11-19) held the accuracy against a nearest point of its own in
    double precision on eight shapes to 1:1000 at ten roundnesses, from a micrometre to ten
    kilometres out (the worst over the truth 1.3 parts in ten thousand, of a ten-billionth of a
    metre), and found two things. **A recipe could stop its own build with a panic**: a sample
    past 1.8e19 from the shape, whose square is past what the numbers hold, made a limit of the
    solve nothing; a rounded box nested in seven scales of a thousandth, which a recipe may
    write, did it (such a sample has the bound for its distance now, as it has for a plain box,
    and the solve cannot be stopped by its limits; a test builds that recipe). **And a foil or a
    hair cost up to 1.7 times a plain cut's rate under the one weight**, with up to 11% of
    their samples left to the sure way where the test's four shapes had 5% at most: thin shapes
    have a weight of their own now, the test holds a foil and a hair to under thirty steps a
    sample and one in eight left over, and the weights are said against the plain primitives'
    rate (the time the comment gave, 1.3 s at the limit, was measured on a cooler machine than
    the reviewer's or this one an afternoon later). **A seventh** (2026-11-19) **said merge**:
    the panic gone by every reproduction and by sweeps from a kilometre to the end of the
    numbers, with no step count over 200. It counted the solve's steps by thinness and found
    the thin weight's one line at an eighth in the wrong place: the steps are the same from a
    half to a twentieth as thick as long and rise only far thinner, so a plank a hair under an
    eighth was charged 1.7 times its neighbour. The weight is graded now, by the logarithm of
    the thinness from a twentieth to a two-thousandth. Its timings, taken while two other
    builds ran, could not settle whether shapes near a seventh are under-priced by a fifth:
    **to be measured again on a quiet machine.**
  - **Inside it is a bound, and the mesher is told a slack**: the greater of the hard shape's
    own depth and the distance behind up to three touching planes, each never less than the
    depth, and never more than it by more than the hard shape's corner is from the rounded
    surface. That slack is next to nothing for a nearly hard shape and 0.42 of the half diagonal
    at a roundness of 1, where less is skipped and pruned near the shape: for an ellipsoid,
    `ellipsoid` is the cheaper way. A `shell` or a negative `round` (a shrinking) of a rounded
    shape is therefore measured from a bound, not from the depth: right on the flat sides,
    thinner than asked by up to the slack beside the corners. **Not mended.**
  - **A known limit of the same kind, not mended:** a `column`'s field is in units of its
    section's narrower half size, so `round`, `shell` and a blend's radius act further along its
    broader side (by the ratio of the two: one and a half on the figure's trunk). The figure is
    tuned and measured on that field.
- **Round form in a few rows** (M6 part 4, 2026-11-17: "bone chains", and what a figure is made of).
  These are given by their ends, not centred on `at`:
  - `round_cone`: `from`, `to`, `radius`, `radius_to`. Every ball between two balls.
  - `chain`: `points`, rows of `[x, y, z, radius]`, and `smooth`. A round cone between every two
    neighbours: a limb, a tail, a branch or a horn. Always closed, always round.
  - `column`: `from`, `to`, `side`, `square`, and `sections`, rows of `[how far along, half breadth,
    half depth]` (up to 8). `square` (2 to 8, 2 unless given) says how square the sections are: 2 an
    ellipse, 3 a body's trunk, 8 nearly a box with rounded corners (a superellipse: as broad and as
    deep whatever its squareness, and further round the squarer it is). 2 and 4 cost a sample what an
    ellipse does, 3 about twice that, any other about four times (they take powers). A squarer
    section reaches toward the corners of the box round it, and the column's bounds go by that (they
    went by an ellipse's, and a square column turned an eighth of a turn was cut flat at its corners). The breadth (toward `side`) and the depth pass smoothly from one section to
    the next, and a rounded end closes it. A trunk with no steps, a limb with its swell, a neck.
    The sizes follow a curve through the sections that keeps going the way they go (a monotone
    cubic): it is level at a section only where the sizes turn there, and never outside the two
    sections it is between. **A column's field is trusted by its steepest section**: it says how
    far out of the section at its own height a point is, and where the sections grow fast (a flange
    on a thin stem) the nearest surface is at another height and much nearer, so the mesher divides
    the field by `sqrt(1 + s * s)` for the steepest growth `s` (`column_lip`). With one fixed factor
    such a column was meshed with samples on the wrong side of its surface. (At first each span eased in and out, which is level at every section:
    a limb that tapered through three sections showed a band at the middle one, and every limb of
    the first figure had rings; over a short span from a wide section to a narrow one it was a
    shelf, which is why the figure's shoulders are a slanted `round_cone`. Changed 2026-11-17; the
    shoulders have not been tried again as sections.)
  A chain's links are joined by a **near-union**: only the links within the smoothing of the nearest
  take part, where a plain smooth union blends its children one after another. It is what lets a
  tree of many parts cost what is near each point.
- **Meshing:** dual contouring on a grid of `resolution` cells along the longest side (≤ 160, ≤ 4 M
  samples), sparse: 8³-cell blocks whose centre is farther from the surface than the tree's
  conservative Lipschitz reach are filled by sign. The reach (Lipschitz factor and slack) folds in
  every node, bounded or not: an unbounded displaced cutter (a half-space with noise) counts as
  much as a bounded one. **A block that is sampled leaves out what cannot matter in it**: for the
  block, then for each half of it, then for each quarter, the tree is pruned of the children of its
  unions that are too far to touch the surface or its smoothing there (`sdf::prune`), exactly for
  the distance (tests compare the pruned and the whole tree bit for bit; a colour is the same through
  near-unions and may differ in the last place through a plain smooth union, which no mesh shows; a
  plain smooth union keeps more than a near-union, because its children blend in turn). A child with
  a repeat, a twist, a bend, a shell or a displacement anywhere under it is never dropped and never
  a reason to drop another: its field jumps, so what it says at a block's middle is no bound for the
  block. A column whose breadth and depth change in different proportions is pruned with an allowance:
  far out to one side its field follows that side's radius, and changes along the column faster
  than its factor says, by its wander (`column_wander`) for each of its own radii a point is out; a
  ball too large beside such a column says nothing, and the column stays. A smooth plain union with
  a child that is not steady keeps every child. One pruned tree is kept for each half block that was sampled, for the crossings' normals
  (it was one a quarter block: 188 MB for a body, and up to 742 MB for a legal build). Each sample costs the evaluation
  weight of what is left (`sdf::eval_cost`: a noise octave about as much as a primitive), and so
  does each crossing's normal. A figure's body of columns costs about three quarters of what its whole
  tree would at every sample (it was under two thirds before its columns' wander was allowed for and
  each limb was cut by its own room; a body of chains, a third). A triangle that comes out under the
  least area a triangle may have is closed by its shortest edge (left to the clean-up, which drops
  such a triangle and keeps its neighbours, it was a pinhole).
- **The weights were halved on 2026-11-17**, after three measurements in release on the reference
  laptop (`calibrate_sdf`): a sphere 23, 29 and 34 ns where its weight said 107; a box 25 to 40; a
  round cone 37 to 48; a twist 35 to 48; eight octaves of displacement 208 to 260; a smooth union of
  eight spheres 251 to 309. Each weight is still at least one and a half times the slowest of those
  (the rule they were first set by, on 2026-10-22, from a sphere of about 70 ns on a shared
  machine). What moved it: a figure of six parts sat at the build's limit and was refused at its
  levels of detail. A sculpt that built before builds now; one that was refused for work may not be. A column
  with its curve through the sections (the same day) measured 54 to 56 ns, and its weight went from
  78 to 86 by the same rule. One vertex per surface cell (a QEF of the
  crossings' planes regularised towards their mass point, clamped to the cell): flat faces stay flat
  and creases sharp. The outer sample layer counts as outside, so the mesh is always closed (it may
  have edges where two sheets touch: still balanced). Unbounded trees (a half-space in a union) need
  `bounds`. The triangle count is known before any is made.
- **LOD:** the coarse rebuild halves the resolution and drops displacement.
- **`figure`** is a shape of this tier: a part of a person from a measured body
  ([FIGURE.md](FIGURE.md)).

### Tier 4: generators (`generate` on a part)

A part takes exactly one of `shape`, `include` or `generate`; a generator may make several pieces.

- **`building`** (`crate::building`): `plan` (any outline), `floors`, `floor_height`, `wall`,
  `hollow`, `plinth`/`band`/`cornice` `[height, out]`, `windows` (bays: width, height, sill, spacing,
  margin, depth, arch, trim, glass, floors), `doors` (side, at, width, height, arch), `roof` (flat
  with parapet, gable, hip, shed on rectangular plans, pyramid on convex ones, none), `ruin` (broken
  tops, no roof, seeded), `bevel` on the openings, and `materials` per role (walls, roof, trim, door,
  glass; the part's material by default). Openings are cut with the tier-2 booleans in one pass.
- **`terrain`** (`crate::terrain`): `size`, `samples` (≤ 257 a side), `base`, noise `layers`
  (fbm, ridged, billow), `warp`, `island`, `terrace`, flat `pads`, hydraulic `erosion` (seeded
  droplets one after another) and `thermal` relaxation (double-buffered), up to four `splat` layers by
  slope, height and noise, `depth` (a closed slab whose sides are soil-coloured from a lip 3 cm under the rim), `mesh_step`. The heights use **the physics
  heightfield's convention** (centred; cell split v10-v01), so `Terrain::height_at` is exactly the
  collider's surface. `Model.extras.terrain` keeps the heights and splat weights (RGBA8 per sample)
  for the future terrain renderer; `physics_heightfield(max_side)` resamples for the engine's
  129-a-side colliders; `walkable_cells(max_slope)` marks cells a navmesh builder may walk.
- **`scatter`** (`crate::scatter`): Poisson-disc (Bridson) in an `area` outline, `min_distance`,
  `count`, rules (`slope`, `height`, noise `mask`, a `splat` layer's weight, `avoid` circles, `clear_of` an
  earlier scatter part's placements by a radius, 256 circles in all), items
  by `weight` (a `shape` with ops, or an `include` with params sampled from `[min, max]`, `variants`
  built once each), `scale`, `yaw`, `tilt`, `align` to the ground, `sink`; `on` an earlier terrain
  part of the same recipe (a terrain part whose ops do more than colour it is refused there: scatter
  and the exported heightfield use the generator's own heights, before the part's ops). Copies merge
  into one mesh per material: for many separate, instanced, culled entities use the
  `procgen_scatter` tool instead. At most 256 `avoid` circles (with `clear_of`'s). Rules run as each
  Poisson sample is accepted, so a scatter stops at its `count`; polygon areas (≤ 1,024 points) are
  tested on a grid of their edges (a candidate meets a handful, not all of them).

### Work budget and cancelling (`crate::work`)

Every build shares one work budget, `work::MAX_BUILD` = 1.5·10^9 **work units**, across booleans,
sculpts, generators, scatters and texture bakes, and the tier-1 stages every recipe has: shape ops
(per vertex: a displace about 750 plus 50 an octave, cavity shading 3,200, weld 650; per output
triangle: subdivisions and copies), assembling the mesh (2,500 a vertex), each LOD simplification
(3,000 an input triangle: 24 clustering passes) and the collider (600 a hull point), includes
too. A unit is an estimated nanosecond of
one core of the reference laptop: each tier counts its own operations and weighs them by what they
cost there (a CSG classification 200 and each exact predicate 100 more; an SDF sample its tree's
`eval_cost`, a 6-octave displaced sphere about 680; a terrain noise octave 50, a droplet step 150; a
scatter candidate 280 plus 10 per polygon edge tested; a texture pixel operation 20 on one core,
since low-end devices may have one or two, with per-op costs measured per op). Units are counted, never timed, so where a build runs out is the same on every
machine; the weights make that about 1-2 s of wall time on the reference laptop, even with other
work running (`cargo run --release -p specula-model --example work_calibration` prints each tier's
worst cases as wall ns per unit charged: 0.2-1.1 measured on 2026-10-22, noisy with other sessions
on the machine). Before the units, a capped boolean took 9-12 s, a capped sculpt 212 s and a comb
scatter 7 s.

The engine queues procedural recipes apart: one worker takes them first, the other light workers
take them when no light build waits, and the first light worker never does, so a light build never
waits behind a sculpt (tier-1 recipes stay on the light queue: they are bounded like everything
else). Each build has a cancel flag (`work::cancel_on`) that every budget check and the long tier-1
loops (displace, cavity shading, LOD passes, hull directions) read: a build no entity wants any more
stops within milliseconds, and one that finished anyway is kept and cached.

### Procedural materials: texture graphs (`crate::texgraph`)

```json
"textures": { "stone": "tex_crypt_stone" },
"parts": [ { "name": "walls", "material": "carved_stone", "texture": "stone", "uv_scale": 1.6, ... } ]
```

- A graph is text: named nodes, each an op over earlier ones (no cycles by construction): `noise`
  (value, perlin, fbm, ridged, billow, worley, worley_edge, tiling), `bricks`, `gradient`,
  `constant`, `blend` (mix, multiply, add, subtract, screen, overlay, max, min, difference; mask),
  `curve`, `levels`, `invert`, `warp`, `blur`, `normal`, `ao`, `curvature`, `gradient_map` (palette
  colours). Outputs: `base_color`, `height` or `normal` (+ `normal_strength`), `roughness`,
  `metallic`, `ao` (a node, or `{radius, strength}` from the height; multiplied into the base
  colour: the engine has no occlusion slot) and `emissive`.
- Inline in a recipe's `textures`, or a library file with `"texgraph": 1` (a recipe folder's other
  files are its library; such a file is refused as a recipe with a pointer to how to use it). A
  textured part's material key is `preset+lib:<file>` or `preset+<recipe>/<name>`, so an including
  recipe's own texture names never repaint an included one; included recipes' *inline* graphs
  aren't baked (an error says to make them library files).
- Baked on the model worker into RGBA8 sets (sRGB base, unorm normal in the engine's convention,
  glTF metallic-roughness), rows split across up to 4 threads (the bits don't depend on the count),
  mip chains made on the worker and drawn with the engine's textured pipeline. Textured parts get
  box-projected UVs (`uv_scale` metres per repeat; split where the projection changes). The texture
  dresses the part's own material only (a building's door and glass keep theirs).
- Sizes are powers of two 16..1024 (256 suits the iGPU: about 1 MB of GPU memory per set with
  mips); every node's cost (derived normals and emissive included) is charged to the build's work
  budget before it runs, all of a model's sets together take at most 32 MiB of GPU memory with mips
  (two 1024 sets with normal maps), and images are freed after their last use (160 MB live at
  most).

### LODs

Recipes with booleans, sculpts or generators take **the coarse rebuild as LOD 1** (no bevels, half
the SDF resolution, a coarser terrain grid) when it saves at least 10%: clustering would fold their
cuts and recesses. Later LODs cluster as before.

### Agent tools

| Tool | Does |
|---|---|
| `model_paint` | Paints a model's vertex colours as data: appends strokes `{at: [x, y, z], radius, color, amount}` to the recipe's `"edits"` (applied in order to the built model, in its own metres: vertices within the radius take the colour, fully at the centre, easing to nothing at the rim; alpha, the wind's painted weight, is untouched) through the project store. 1..64 strokes a call, 512 a recipe (numbers are kept to a tenth of a millimetre). A `dry_run` or `expect_version` of the wrong type is refused, never ignored (here, in `model_path` and in `terrain_edit`). Paint colours vertices: a stroke with no vertex within its radius changes nothing (the editor's Paint brush, which sends this tool, refuses to send such a stroke) | the project store's (journaled, claim-checked, `world_undo`) |
| `model_path` | Redraws the path of a sweep part: `points` as `[x, z]` (the path keeps its first point's height) or `[x, y, z]`, 2..256; the part by `part` (its name) or `sweep` (its place among the recipe's sweep parts); `closed` and `smooth` set when given; per-point `scales` that no longer match reset to their ends. One call that needs none of the recipe's text. The editor's Path tool sends it | the project store's |
| `model_build` | Builds any recipe: per-LOD triangles, the procedural steps each part took (`procgen.steps`: work, grid, placements...), texture sets (size, GPU bytes, hash), the terrain |
| `procgen_texture` | Bakes a texture graph (inline or a file) and returns stats plus an image strip: base, normal, roughness, metallic; `save` writes it to the captures folder |
| `procgen_terrain` | Builds a terrain (a recipe's or an inline spec) and reports it: height range, splat coverage, walkable share (at `max_slope`, default 45° as `nav_build`'s agents); `collider: {entity}` puts its heightfield collider on an entity through `world_set`; `preview` adds an image |
| `procgen_scatter` | Poisson-disc placements with rules on flat ground or a terrain recipe; `spawn` makes one model entity per placement in one `world_batch` (journaled, claim-checked, undoable; one recipe instances on the GPU) |

The three tools run on the calling agent's protocol thread (two at a time across all agents), each
bounded by the build work budget; their world edits go through the engine like any agent's. Every
argument object is key-checked, nested ones too (`area`, `mask`, `ground`, `spawn`, `items[i]`,
`items[i].model`, `collider`), and seeds must be whole numbers: a typo is an error, never a default.
A terrain part's `move` and `scale` ops (and the recipe's `finish`, and through an `include` the
including part's ops and the outer `finish`; the first terrain is the model's) are carried in
`Model.extras.terrain_transform`, so the collider and a scatter's ground follow them; a rotation,
mirror or reshaping op (displace, bend...) is refused with `terrain_rotated` / `terrain_reshaped`.

### Sepulcra dogfood

Recipes only, in `games/sepulcra/assets/recipes`: `headstone_carved` (arched with a sunk panel and
a carved cross, a ringed cross of bevelled boxes, a broken obelisk; bevelled booleans; half the
graveyard's tombstones in the game now), `crypt` (building generator, textured with `tex_crypt_stone`), `ruined_wall`
(booleans plus an SDF rubble heap, textured), `grave_rock` (SDF), `graveyard_ground` (terrain) and
`graveyard_plot` (terrain plus scattered headstones and rocks in one model). The slab floats over
mist, so no terrain was put under it.

### Measured (2026-10-22, this machine shared with other sessions, release build, best of 3-5)

`cargo run --release -p specula-model --example procgen_bench`:

| Case | Time | Output |
|---|---|---|
| Wall minus 12 windows | 5.5 ms | 312 triangles |
| ... chamfered (96 runs) | 55 ms | 1,752 triangles |
| Sphere (1,280) minus 3 cylinders | 587 ms; 1.25 s to the work budget since it | refused since the work budget (the heavy case: curved operands; use an SDF) |
| SDF rock, resolution 24 / 48 / 96 | 8 / 43 / 241 ms | 1,792 / 7,200 / 28,888 triangles |
| Terrain 129², 20k droplets, 10 thermal | 118 ms | |
| Scatter 100 × 100 m at 1.5 m | 23 ms | 2,825 placements |
| Crypt stone 256 px (4 threads / 1) | 80 / 319 ms | 1 MB GPU |
| Crypt stone 512 px | refused: 136.3 M estimated pixel operations against the 75 M work budget (measured before the budget: 245 / 1,228 ms on 4 threads / 1) | bake 256 px (its recipe's size, for the iGPU: 34.1 M, 73 ms on 4 threads, 2026-10-29), or lower octaves and the AO radius for 512 |
| Recipes: headstone 2 / crypt / ruined wall / rock / ground / plot | 90 / 190 / 350 / 21 / 164 / 674 ms | 466 / 1,110 / 3,658 / 720 / 7,264 / 12,662 triangles |

Live (`procgen_check.py`, Intel iGPU, dev engine): a 1600×900 capture of the scattered graveyard
(90 headstones and 14 rocks as instanced entities on the heightfield) takes 8.8 ms of GPU, 58 draw
calls for 22,610 triangles.

### Not done yet

- The bevel pass leaves more triangles than it needs where a merged face fails to re-triangulate
  (it falls back to the pieces); a chamfered wall is about 5× its sharp self.
- Curved, dense operands are slow in the BSP (a 1,280-triangle sphere is a 1,280-deep chain);
  organic work belongs in SDFs.
- Textures of an included recipe's inline graphs aren't baked (name a library graph instead).
- A recipe's own scatter `on` a terrain part whose ops do more than colour it is refused (the tools
  follow moves and scales, the recipe generator doesn't yet), and `mesh_step` > 1 draws a coarser
  surface than the heights a scatter or collider uses.
- Road and path layout along splines (tier 4 in the plan above) isn't built.
- A terrain renderer (LOD, splat materials) is a separate lane: it reads `Model.extras.terrain`.

## Materials

- **Palette-first and vertex colours by default.** Each game's style bible defines named palettes, and recipes refer
  to palette names, never raw colours. *Why:* it's consistent across a swarm, needs almost no texture memory (good
  for F3) and suits stylised art.
- **Procedural textures** where wanted: value/gradient noise, Worley (cells), bricks and tiles, wood rings, rust and
  dirt masks by curvature or height. They are either baked to small textures at build time or computed in a shader
  with a budget.
- **Material presets** in the style bible (e.g. "painted_wood", "wet_stone"), with fixed roughness and metalness
  ranges. Agents choose presets rather than invent parameters.

## Optimisation built in (F3)

Every recipe build produces:
- **LODs** by quadric error simplification ([Garland & Heckbert, 1997](https://www.cs.cmu.edu/~garland/Papers/quadrics.pdf)),
  with switch distances from screen-size thresholds.
- **A collider**: box, capsule or convex hull fit, or a simplified mesh for static level geometry. See
  [PHYSICS.md](PHYSICS.md).
- **Budget checks** against `style.toml` (triangles per LOD, palette, scale range, texture bytes). A violation fails
  the build with the measured numbers attached.
- **Instancing-friendly output.** Repeated recipes share one mesh on the GPU.

## The taste loop

Measurable rules catch budget and scale errors. Taste needs a loop. Every asset (or batch) an agent makes goes
through:

1. **Brief:** the task names the style bible entry, purpose, scale, and neighbours ("harbour town lamp post, beside
   these benches").
2. **Silhouette pass:** black-on-white renders at thumbnail size (64–128 px) from 3 angles. If the object isn't
   recognisable as a silhouette, no amount of detail will save it. This is the first and cheapest check.
3. **Value pass:** greyscale render under stage lighting. Do the large shapes separate? Is there a clear focal area?
4. **Colour pass:** palette render, then a **line-up** beside approved siblings and a 1.8 m human for scale.
5. **Critique against the bible:**
   - shape language (e.g. "chunky, rounded, slightly oversized tops")
   - proportions
   - detail density (fewer big shapes beat many small ones)
   - consistency with siblings
6. **Revise** the recipe and repeat. At most N rounds, then submit anyway with the critique attached.
7. **Review queue:** the owner sees a contact sheet of the batch, approves or rejects, and notes go back to the agent.

The **style bible** is a per-game document: palette, shape language, proportions, material presets, reference images
and banned patterns. The owner writes it with Claude at the start of a game. Every art agent's brief points to it.
This is how one person's taste scales across a swarm.

## What procedural modelling won't do well

**Realistic human faces and characters, and complex organic sculpts,** are poor fits for recipes. For those:
- import from Blender, sculpting tools, scans or asset stores (glTF)
- later, a **parametric character base**: one base mesh plus morph targets and proportion sliders, so agents can make
  varied characters

The art direction should play to the strengths. Stylised, readable and shape-driven art suits agent swarms and
low-end hardware alike.

## Tools (sketch)

| Tool | Does |
|---|---|
| `asset_build` | Build a recipe → mesh, LODs, collider, report (triangles, bounds, budget results) |
| `asset_preview` | Silhouette, value, colour and line-up renders on the stage world, as one contact sheet |
| `asset_check` | Run `style.toml` checks; return violations with measured values |
| `asset_submit` | Send to the review queue with captures and critique notes |
| `asset_variants` | Build N seeded variants of a recipe as a contact sheet |

## References

- Garland & Heckbert, "Surface Simplification Using Quadric Error Metrics" (SIGGRAPH 1997).
- Ju, Losasso, Schaefer & Warren, "Dual Contouring of Hermite Data" (SIGGRAPH 2002), and Gibson, "Constrained
  Elastic Surface Nets" (1998), for meshing distance fields.
- Inigo Quilez's articles on signed distance functions and smooth minimum (iquilezles.org).
- Bridson, "Fast Poisson Disk Sampling in Arbitrary Dimensions" (SIGGRAPH 2007 sketch).
- Müller, Wonka, Haegler, Ulmer & Van Gool, "Procedural Modeling of Buildings" (SIGGRAPH 2006): shape grammars.
