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
- **Mended (2026-11-20): a strongly stretched node was cut off by the box it is meshed in.**
  The fault as found is below. **The box a sculpt is meshed in now holds what its field can
  reach** (`reach` in `sdf.rs`): each node is asked for the level of its field its caller
  needs, from the root's 0 down, and answers with a box and a ball holding every point where
  its field is at most that level. Its distance modifiers are undone first (noise by its
  amount, a shell by half, a swelling, and the scale times the least stretch, which is how the
  stretch's short reading enters); a stretch turns the ball into an ellipsoid (a ball of its
  least radius and a box of the rest); a smooth union asks its children for the level and its
  greatest dip (`min(n − 1, 4)` quarters of its width, DISTANCE_FIELDS 5.5), or, tighter where
  only one part is near, for the level, and meets each child's dipped reach with another's at
  the level and the join's width (two parts) or nine quarters of it (more: proved in the
  code's comment); a meeting or a cut asks for the level itself. An ellipsoid's and a column's
  fields read short far from them and are answered exactly for what they read (an ellipsoid:
  the greatest of a quadratic's roots; a column: its halves times `1 + level / cap`). A twist
  or a bend keeps a point's distance from its axis and turns it by at most its rate times the
  coordinate it goes by. **The mesher takes the old box and moves a side out only where this
  reaches more than a quarter of a cell past it** (the grid samples half a cell past its box,
  so nothing within a quarter is lost): every mesh that was right is the same mesh.
  - **How it is held** (`sdf.rs` tests): each case below reaches its whole surface, closed
    (two needles joined, a needle rounded, shelled, and a group's swelling over one, two groups
    of four needles, a flat box twisted, a box bent; each failed before); the recipe of two
    needles spans x ±1.25 within a cell; **a truth of its own**: 240 trees drawn from a fixed
    sequence (every primitive; unions, near-unions, meetings and cuts, sharp and smooth,
    nested; stretch, scale, turn, mirror, swelling, shell, noise, twist, bend, elongation,
    repetition), the field sampled outside the box on its faces pushed out five ways and at
    random, nowhere at or below nothing (the old box: 18 of them escaped, the worst by 19 m, a
    twist); the pruning, whole tree against pruned, bit for bit, over 120 drawn trees and the
    investigation's stretched cases, a block's tree pruned again as the mesher does; and **no
    box moves** for any sculpt in the repository's recipes, nor for figures at both sexes'
    limits of stature, three builds with traits at ±3 and five shapings (default melting with
    the limbs at their limits, none, twice the default), each part at its own resolution.
  - **What it costs**: the mended cases are meshed in a larger box at the same resolution, so
    with coarser cells and less work (two needles at resolution 120: 21.4 M work units to
    17.8 M; two groups of four: 145 M to 61 M; a rounded needle 6.7 M to 3.9 M); nothing in the
    repository costs anything more.
  - **Settled on the way**: the old box did not hold a twisted or bent node either (a flat box
    twisted swings its corners past the box about it; the old box was a cube of its largest
    coordinate); now it does. A rotated meeting of several boxes joined smoothly does not
    escape the old box (a meeting's field is never below any operand's, and the box was widened
    by a cube of the join's width, not a ball): run, 2 to 6 copies, widths 0.1 to 0.3.
  - **Still a limit**: the reach is a bound, sometimes more generous than the solid. A figure
    melted four to eight times more than the default (`smooth` 0.1 to 0.2) with its arms out
    gets a whole-figure box up to a third deeper than before (its limbs' columns read short
    across their broad side, and a join that wide is answered for where it could reach, not
    where it does), so its cells are a few percent coarser; no solid was found outside either
    box. A sliver thinner than the sampling of the truth test could pass it unseen. The union's
    tighter answer asks every child three times: past 16,384 asks in a tree it asks once.

- **The fault as found** (2026-11-20, by holding the modeller against
  [DISTANCE_FIELDS.md](DISTANCE_FIELDS.md): each case meshed inside the automatic box and inside
  explicit `bounds` wide enough for the whole surface). A stretched node's field reads short of
  the distance by the ratio of its least stretch to its greatest, and the box is widened as if
  it did not:
  - **`round`, `shell` or `displace` on an unevenly stretched node, or on a group with such a
    child**, moves the surface by the amount times that ratio's inverse along the long axis,
    and the box grows by the amount alone. A sphere of radius 0.1 stretched ten to one and
    rounded by 0.05 loses 0.43 m of its length; shelled by 0.04, 0.15 m.
  - **Two parts stretched by more than about four to one and joined smoothly** reach further
    than the join's width past their tips. Two ten-to-one needles joined at `smooth` 0.1 lose
    0.12 m (the model is 2.24 m long where the surface is 2.50 m); twenty to one, 0.34 m.
    Nested smooth groups of such parts lose more (two groups of four: 0.51 m).
  - **The mesh was still closed** (the box's border caps it flat), so no check caught it.
  - Not affected: no recipe in this repository stretches a sculpt node this way (each was
    sampled outside its box, with the figure's parts: nothing escapes). Thin edges made by an
    `intersection` or a `difference` joined smoothly lose nothing (a meeting's field is at least
    that of the child that sets its box), and neither do nested smooth groups of unstretched
    parts (each level's box is widened again). The mesher's pruning is not affected (whole tree
    against pruned at 72,000 points a case: no difference).
  - Not settled then, settled since (above): `twist` and `bend`; an `ellipsoid` and a
    `column` reading short far away.
- **A part thinner than a cell meshes with edges that four triangles share** (looked into on
  2026-11-20, after the bounds investigation saw "two crescents joined smoothly mesh open").
  It is not the join and not the box: **one crescent** (`{"op": "difference", "of": [{"op":
  "sphere", "radius": 0.5}, {"op": "sphere", "radius": 0.5, "at": [0.05, 0, 0]}]}`) has 436
  such edges at the default resolution of 48 and 312 to 688 at every resolution from 24 to
  120; a sphere with `shell` 0.01 has 168 to 12,312. **The cause, in the mesher**: dual
  contouring puts one vertex in each cell the surface crosses and a quad across each grid edge
  whose ends differ in sign. Where a sheet is thinner than a cell, a face of the grid can have
  its four corners inside, outside, inside, outside in turn; all four of its edges cross, and
  their four quads share the one mesh edge between the two cells either side of that face. The
  mesh is closed (two of the quads run one way, two the other), so the modeller's own checks
  passed it, and it is not a surface there: `open_edge_count` counts each such edge twice, a
  level of detail keeps no volume for that part, and the studio's form facts read it as
  `tangled_edge_m`. **Most sculpts in the repository had a few** (one to eighteen a part:
  `grave_rock`, `statue_angel`, `statue_mourner`, `runestone_t3`, `skeleton_warrior_t3`,
  `tree_t3`), and so did 65 of 126 parts of the measured figure tried (hands, feet, body).
- **Mended (2026-11-20, at the owner's word: "the fix needs to be now"): every edge of a
  sculpt is shared by two triangles.** In `sdf.rs`'s `mesh`:
  - **Each sheet through a cell has a vertex of its own.** A cell's crossings are grouped
    (`sheets`): on each face of the cell two crossings are one stretch of the surface's trace
    on that face; four crossings (corners inside, outside, inside, outside in turn) are two
    stretches, each cutting off one of the two corners the face's middle does not join, and
    **which two is asked of the field at the face's middle**, of the tree that holds the face,
    so the two cells beside it pair alike. The stretches chain the crossings into loops round
    the cell: a loop is a sheet, and each gets its own QEF and vertex. A cell with fewer than
    six crossings has one sheet and no face of four (a loop crosses at least three edges, and a
    face of four makes at least seven), and is not grouped at all.
  - **A thread thinner than a cell through a face** (one sheet of each cell beside a face of
    four takes both of its stretches: two cells' vertices would be joined twice, the thread's
    section a polygon of two sides) gets a vertex on the face for each stretch, between the
    two cells' vertices in the quads of the stretch's crossings; such a polygon is fanned from
    that vertex.
  - **A sliver is closed only where that leaves a surface.** The triangles under the least
    area were closed by moving one end of their shortest edge onto the other; that is done
    now only when the two ends have no neighbour in common but the far corners of the two
    triangles on the edge (the link condition), else the next edge is tried; a sliver no edge
    can close is left to the weld, which drops it, and the hole is said (none was met). This
    one is not about sheets: **a plain cone's tip at resolution 24 was not a surface** (two
    cells' vertices held a hundred-millionth of a metre apart on the line their cells share,
    the quad split along them, and the closing put four triangles on one edge).
  - **Vertices at one place are kept apart where joining them leaves no surface.** Two cells'
    vertices can be held at the same point by their cells' bounds (a corner or an edge both
    share), and the weld joins them; where they are not an edge of the mesh, or are one that
    fails the link condition, each is moved a sixteenth of the way to its own crossings'
    middle. Also not about sheets: a drawn tree at resolution 11 was not a surface for it.
  - **Why two triangles an edge**: an edge of the mesh is either a stretch on a face, joining
    the vertex of its sheet in each cell beside the face, and the stretch's two crossings are
    in that sheet in both cells (the pairing is the face's), so it is in the quads of those two
    crossings and no others (another stretch on the face gives another pair, or is a thread,
    which has vertices of its own); or it is a quad's diagonal or a fan's chord, between
    vertices of cells that meet only along that quad's grid edge, or from a vertex on a face
    that one other quad holds, whose cells differ. Round every vertex the triangles make one
    fan: a sheet is a disc in its cell.
  - **Where nothing is thinner than a cell, nothing changed by a byte**: every sculpt mesh of
    the repository without such a cell (30 of 42 a build makes, detail and coarse), 61 of 126
    figure parts and 74 of 174 drawn meshes hash as before. What changed is the twelve meshes
    that had shared edges (the six sculpts' seven detailed parts, and five coarse meshes the
    builds make for their levels of detail), the figure parts that had them, and **four drawn
    meshes with no such edge**: each had a cell where two sheets met at one vertex (two
    corners of the cell opposite each other, no face of four), which is now two vertices.
  - **The six**: `grave_rock` 716 triangles (10 new or moved), `statue_angel` 976 to 984
    (38), `statue_mourner` 1,124 (23), `runestone_t3`'s moss on top 1,088 (11), the
    skeleton's skull 1,276 (24) and ribcage 2,976 (178), `tree_t3`'s trunk 2,908 (44); bounds
    the same to the bit; volumes within 0.2% but the ribcage (+2.2%: its thin ribs were
    pinched to lines before). Of the coarse meshes, the runestone's slab 3,192 (24), the
    skull 340 (14), the ribcage 752 to 840 (264; +35% volume, of 0.4 litres), the trunk 796
    to 824 (140; +2.3%) and the crown 1,524 (12). Each is now closed
    as a surface, so a level of detail keeps its volume; the studio's `tangled_edge_m` fell to
    0 for five of the six models (the skeleton keeps 0.098 m, from where parts that are not
    sculpts meet); the colliders are unchanged. Pictures before and after in the engine are in
    the gallery's originals (`part4/mesher/`).
  - **What it says**: `SdfStats::thin_cells` (cells that hold more than one sheet or a face of
    four) and `thin_at` (the box round them), `threads`, and in the build's report
    `thin_cells` and `thin_at`; the model's `notes` say where, that the mesh is a closed
    surface there but drawn by the cells' vertices alone (no finer than a cell), and to raise
    `resolution` or thicken it. `tangled_edges` (edges shared by other than two triangles) is
    0 by construction and is said, in the report and the notes, if a fault ever makes one.
  - **What it costs**: [`SHEETS_WORK`] (40 units) for each cell with six crossings or more, an
    evaluation for each face of four (asked from each cell beside it), a unit a triangle for
    each sliver's edge tested, an eighth of a unit a vertex for finding vertices at one place
    and a unit a triangle when there are any; all charged. Over
    the repository's 42 sculpt meshes, 542.13 M to 542.24 M work units (+0.02%); a crescent at
    48, 21.90 M to 22.06 M; an ellipsoid's shell of 4 mm at resolution 160 (40,138 thin
    cells, 3,572 threads), 199.96 M to 205.72 M (+2.9%), 363,124 to 377,412 triangles; the
    dearest found, a crescent at 150, 235.97 M to 246.21 M (+4.3%: its slivers, each tested by
    a scan of 200,000 triangles). The samples taken are the same.
  - **Still a limit**: a sheet thinner than a cell is a surface now, and a coarse one: its
    two sides are a cell's vertices apart at most and may be closer than its true thickness,
    or cross where the field says they do not; a thread's section is four vertices. Raise
    `resolution` where it matters.
- **Roundness as one number** (M6 part 4, 2026-11-18): `roundness`, 0 to 1, on a `box` and a
  `cylinder` here, and on a `rect` outline (the profile of an extrusion, a loft or a sweep). 0 is the
  hard shape, 1 its fully round form (the ellipsoid as wide, as tall and as deep; the ellipse), and
  every value between is a valid shape with flat sides and corners that tighten as the number falls
  (a superquadric, a superellipse: Barr, 1981). The shape keeps its sizes whatever its roundness, so
  it can be turned up or down without anything else moving. **What it costs:** a roundness
  of 1 is the cheap one (no power to take: six times a plain box's sample); any other value solves
  for the nearest point, with about ten to forty powers a sample outside the shape on average (177 the most seen) (a cube of
  roundness 0.3 cut from a 2 m box: 16 a sample, inside and out). Its weight, one for every
  thinness, pays for 17 powers a sample; **what the samples take past that is charged as they
  take it**, 84 units a power over the build, so the work limit bounds what any recipe of
  rounded shapes takes, wherever they are sampled: no build takes more than one power for
  every 84 units (since the eighth review, below). So a cube of roundness 0.3 builds at a
  resolution of 64 on under half of the work limit (0.47), still builds at 84 (0.96) and is
  refused at 88 (1.04) and at 96 (1.28), where the same cube at roundness 1 takes a third of the
  limit (a test holds 64 and 96); a thin board has an eighth of a cube's samples at the same
  resolution (a board a tenth as thick, 0.07 of the limit at 64 and 0.18 at 96). The count is
  the measure, not a time: the calibrated case (that cube cut from the box) takes 9.5 powers
  for every thousand units, and on a quiet machine (2026-11-19, before the eighth review's
  mends, at 13 powers a thousand units) it ran at 0.97 of the plain cut's rate. It is not the modifier `round`
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
    allows. A test holds a plate, a needle, a cube and a board (and since the eighth review
    a foil and a hair), at six roundnesses, to under 20 powers a sample in a 2 m box and 30
    just off the surface, with no more than one in 200 left to the sure way, the
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
    had a weight of their own from then until the eighth review, the test held a foil and a
    hair to under thirty steps a sample and one in eight left over, and the weights were
    said against the plain primitives' rate (the time the comment gave, 1.3 s at the limit,
    was measured on a cooler machine than the reviewer's or this one an afternoon later). **A seventh** (2026-11-19) **said merge**:
    the panic gone by every reproduction and by sweeps from a kilometre to the end of the
    numbers, with no step count over 200. It counted the solve's steps by thinness and found
    the thin weight's one line at an eighth in the wrong place: the steps are the same from a
    half to a twentieth as thick as long and rise only far thinner, so a plank a hair under an
    eighth was charged 1.7 times its neighbour. The weight was graded then, by the logarithm of
    the thinness from a twentieth to a two-thousandth. Its timings, taken while two other
    builds ran, could not settle whether shapes near a seventh were under-priced by a fifth
    (it saw up to 1.75 times the plain cut's rate). **Measured again on a quiet machine the
    same evening, with its own probe**: 1.04 to 1.09 times for boxes a quarter to a ninth as
    thick as long, 0.75 to 0.87 for the foil and the hair. The rest was noise; the weights
    were moved by what was left (the thick one up a ninth, the thinnest down a tenth), and
    every case measured now runs at or under the plain cut's rate. **An eighth** (2026-11-20,
    of those levelled weights, by counting the solve's steps and powers in real builds)
    **said not to merge them** (what was mended after it follows the list):
    1. **A large, thin rounded cutter set past the corner of what it cuts costs four times
       its charge by count** (and timed four to seven times, on a busy machine): a 2 m box
       with a rounded sheet of 100 m by 0.1 mm subtracted from it, a metre past its corner,
       where the sheet cuts nothing. A difference keeps every child, every sample near the
       box is one to three metres from a 50 m shape (too near to take the far estimate), the
       first steps fail there, and 98% of the solves fall back to the sure way at 119 steps
       each. A build at the work limit then runs several times as long as the limit is
       meant to allow: **the limit does not bound the time for such a recipe.** A sheet of
       10 m, a ten-thousandth as thick, timed about three times. The test samples a 2 m box
       round shapes of under a metre and does not reach that region.
    2. *Medium:* **at roundnesses the levelling did not try, shapes inside the tested range
       run 1.2 to 1.6 times the plain cut's rate**: a foil a nine-thousandth as thick at a
       roundness of 0.3 (1.58, where the same foil at 0.5 runs 0.83), a hair at 0.05 (1.35),
       a board a twentieth as thick at 0.99 (1.24). The weights were levelled at one
       roundness a shape, and the cost moves by about two to one across roundness.
    3. *Medium:* **two sentences above are false**: "up to one in eight left to the sure
       way" (measured 14% and 15% in the tested range, and half to nearly all in finding
       1), and "a dozen to twenty powers a sample outside the shape" (the calibrated cube
       takes over 22).
    4. *Minor:* the weights before the levelling pass both tests, so nothing holds the
       levelling; and leaving a box's third size out of its thinness passes too, so a plank
       thin along its depth could be charged as thick.
    Found sound: the grading has no step (swept from 1 to 1 to a hundred-millionth as thick),
    thinner than its cap is not under-charged away from finding 1, discs and needles are
    cheap at every thinness, the earlier reviews' cases hold, Sepulcra's approved props
    build as they did, and nothing panicked in 1,500 builds and two million samples. A lone
    cube of roundness 0.3 at a resolution of 88 built before the levelling and is refused
    now (as meant: the old weight under-charged it; no recipe here uses `roundness`). Not
    checked: rounded shapes under a twist, a bend or a repeat; whether the field is right
    where the solve falls back most.
    **What was mended after it** (2026-11-20, by a builder that wrote none of it):
    - *The cause of 1.* The solve's `mu` is the distance times how far the shape reaches
      along the normal at the nearest point. It was started along the normal at the first
      touching plane's foot, brought onto the surface: beside the rim of a 100 m sheet a
      ten-thousandth as thick that is the normal of the face, along which the sheet reaches
      a twentieth of a millimetre, where along the true normal (out of its edge) it reaches
      over 60 m. `mu` began three hundred thousand to five million times too small, the first
      steps went the wrong way and stopped at their floor (traced sample by sample), and the
      sure way ran. Now the start is the point of the surface that reaches furthest along a
      guess of the normal taken from where the sample lies: past the hard shape, the way out
      of it from its nearest point; inside it, the way to the sample from the foot's point of
      the surface. There every unknown of the first steps is known with no power taken, and
      it is the answer itself for any sample on that point's normal, at any thinness. Both
      halves were needed (a sweep of 20,160 places, from a tenth of a millimetre to ten sizes
      out, thin to a hundred-millionth along each axis, fourteen roundnesses: 3,044 places
      averaged over 40 powers a sample before, none after; the right `mu` with each share
      started at the most its equation allows was nearly as bad as before). The review's
      samples arrive in 13 to 19 powers (were 27 to 127), over 3.9 million samples all but
      0.04% arrive, and none takes more than 177 powers.
    - *The bound.* The limit is charged as the build goes, so it can charge what the solve
      does: every power is counted, the weight pays for 17 a sample, and what the samples
      take past that over the build is charged at 84 units a power at the next charge (one
      sample's spare powers pay for another's). No sample can take more than 510 powers
      (counted from the code: the gradients and the start, ten first steps, the sure way's
      twelve steps of at most twelve powers a share), so between two charges at most a
      batch's samples run unpaid, and the colours are charged every 64 vertices. Whatever a
      recipe is, its build takes no more than one power for each 84 units it is charged:
      five quarters of what the calibrated case takes. The same recipe is charged the same
      on every machine.
    - *2 and 4.* With the start mended a thin shape's samples take what a cube's do (17.7 to
      18.5 powers a sample at the worst roundness for each thinness, from 1:1 to a
      hundred-millionth, for shapes a metre across cut from the box's middle), so the weight
      is one again, for every thinness and every axis; the levelling is the charge, and a
      test holds it by count over the review's cases, planks thin along each axis in turn,
      sheets to 10 km and to a hundred-millionth, a disc past a corner, stacked scales, a
      stretch, a twist, a bend, a repeat, a displacement, a smooth blend of three, and three
      thinnesses at eight roundnesses each: no case takes over five quarters of the
      calibrated case's powers for its work, nor two and a half times its powers a sample.
      Over 1,188 builds (a box and a disc, 1 m to 50 m, 1:1 to a hundred-millionth, eleven
      roundnesses, over the middle, beside an edge and past a corner) the dearest takes 1.14
      times the calibrated case's powers for its work (a 2 m sheet a millionth as thick cut
      through the middle, 37 powers a sample, charged for what it takes past the weight).
    - *3.* The sentences about what it costs are rewritten above with these counts.
    - *The field there.* A test holds it against a distance found by search, past the
      corner of sheets from 2 m to 100 m and a thousandth to a millionth as thick.
    - *What changed for recipes that built before:* nothing in Sepulcra (its asset tests
      pass; no recipe here uses `roundness`). A lone cube of roundness 0.3 is charged as
      before (0.47 of the limit at 64, refused at 88); a thin shape costs less than under the
      graded weight; and a sample that takes more than its weight pays for what it takes, so
      a recipe whose samples are dear can now be refused where it built.
    - *Left as it is:* the charge is in powers, and a power's time against a unit of plain
      work was last timed on a busy machine (about 48 units); whether the calibrated case
      still runs at the plain cut's rate now that it takes fewer powers a sample (16 where it
      took 22) is for a quiet machine: `cargo run --release -p specula-model --example
      rounded_rates` times the calibrated case and the dearest by count against a plain cut
      in the same run, with each one's powers a thousand units (by count the dearest takes
      1.14 times the calibrated case's powers for its work, so it should run at no more than
      about 1.14 times the calibrated case's rate, if time goes with the powers). Asking
      which primitive made each triangle (`primitive_of_triangles`) is charged at the
      weights only, not for powers past them.
    **A ninth** (2026-11-20, of that mend, against a distance bracketed from both sides by
    the reviewer's own method) **said not to merge it.** (Both major findings are mended
    below; that mend has yet to be reviewed. **Until a reviewer says merge, do not give
    `roundness` to a shape that is thin along one axis.**)
    1. **The mend brought a fault of its own: just past a thin face, beside its rim, the
       field falls far short of the distance, and steps.** A plank of half sizes
       1 by 0.5 by 0.00001 at a roundness of 0.3, sampled near its rim: a hair inside the
       hard face the field is 0.48 mm, which is right; a hair outside it the field is
       0.0086 mm, 56 times short, and it stays short for about ten thicknesses out. The
       code before the mend gives 0.48 mm at every one of those points. The new start
       takes the way out of the hard shape as its guessed normal; there that is the thin
       axis, which is the eighth review's failure turned round. A field that falls short
       is still a bound, but a swelling, a shell or a blend measured from it comes out
       wrong there, and a step in the field is what the fourth review refused. The new
       test samples only past the corners of sheets thin along one axis.
    2. **The work limit does not bound a build of rounded shapes under nested unions.**
       The mesher's pruning evaluates every subtree again at each level of nested plain
       unions and is charged once; those evaluations also earn the allowance of powers. A
       cube of roundness 0.3 under 10, 40 and 120 nested unions takes 1.7, 3.5 and 6.2
       times the calibrated case's powers for a unit of work (by a busy machine's clock,
       22 times at 120). The pruning did this for every primitive before; it makes the
       sentence above ("no recipe can take more than...") false.
    3. *Medium:* asking which primitive made a triangle runs up to 5.9 times the calibrated
       powers a unit (capped by its own limit: about 0.7 s at worst). *Medium:* false
       sentences above: "every power is counted" (at a roundness of 0.5 the sure way's
       inner steps take no power and are not counted); "a thin shape costs less than under
       the graded weight" (a millionth-thick sheet at a roundness of two thirds went from
       0.72 to 0.99 of the limit at a resolution of 64, so resolutions of about 65 to 71
       are refused where they built); "0.04% to the sure way" and "177 at most" hold for
       the author's samples (the reviewer's sent 3.1%, with 187 at most).
    4. *Minor:* nothing fails when the tally is not reset at the start of a mesh, nor when
       what was paid is not kept; the colours' charge is held by nothing.
    Found sound: away from finding 1 the field is within a thousandth of the bracketed
    distance and never over it; no other step in 13 million samples; 510 is a true ceiling
    on one sample's powers; the charge depends on nothing but the recipe and its tree;
    Sepulcra's approved props build as they did.
    **What was mended after the ninth** (2026-11-20, by a builder that wrote none of it):
    - *1, at its cause.* There is one nearest point: for each `mu` every share has one
      answer, and the sum of the shares to the power falls as `mu` grows. A start cannot make
      an answer wrong; a solve that stops before it has arrived, and calls where it stopped
      the answer, can. That is what the sure way did: twelve Newton steps on `mu`, which on a
      thin shape's sum (falling as a power of `mu` over many decades) gain less than a
      doubling each, where the review's sample needed about thirty. Now:
      - the first steps are believed only when they have arrived: every share on its
        equation and the sum on one to a thousand-millionth, and the next step on `mu` under
        a ten-millionth of it (the step is what says how near `mu` is near the surface, where
        the sum is nearly level). Past the hard shape they are taken from two starts in turn:
        the way out of the hard shape, then the touching plane at the sample;
      - **the sure way cannot fail to arrive.** `mu` lies between two bounds known before a
        step: the distance's lower bound times the least half size (the shape holds the ball
        of that radius), and a bound on the distance (past the hard shape its distance, plus
        the most the hard surface is from the rounded one) times the half sizes' length. Its
        steps are on the logarithms of `mu` and of the sum, a straight line where the sum
        falls as a power; each is kept inside the bounds, which close in at every step; one
        that would leave them, or that has not halved them in two steps, halves them instead
        (from below, a step on `mu` itself first, which cannot pass the answer on that convex
        curve). Each share is solved in logarithms from above. It stops when a step is under
        a ten-trillionth of `ln mu`;
      - a lower bound worked out in single precision is lowered by what rounding can add
        before the steps lean on it (found by the new tests: where it is the distance itself,
        at a ball or a face's middle, its last places put a floor over the answer, and 5.5% of
        a ball's samples, on a build's grid at 96, took the sure way for it).

      So **whichever way a sample is answered, it is the one nearest point** to within those
      tolerances, and nothing can switch between two answers: no step. Where the nearest point
      lies about a thin shape, and what finds it: over a face away from its rim, on the face
      below the sample, its normal the face's (the way out arrives); just past a face beside
      its rim, on the rim, its normal nearly in the shape's plane (the way out does not
      arrive, the touching plane at the sample does: the old start, right there); beside the
      rim at mid-thickness, past an edge or a corner, on the rim or the corner's rounding, its
      normal the way out (the way out); inside the hard shape and outside the rounded one,
      beside a rim or a corner (the touching plane's foot, as before); inside the rounded
      shape nothing is solved for (the field is a bound there). Whatever falls through goes
      to the sure way: of 200,000 samples just past a thin face, a millionth to a thousand
      thicknesses out, 2.75% did with one start and 0.01% with two (a sheet a millionth as
      thick at 0.4); of 200,000 anywhere in a 2 m box, 0.02% for that sheet and none for a
      cube, a ball, a plate or a foil.
      Also found by the new step scan, and mended with the arrival rule: a sample a
      hundredth of a millimetre past a rod's end at a roundness of 0.2 came out at the hard
      shape's distance, 4% short, beside samples that were right.
    - *The test of 1.* The field outside is held against a bracket that shares nothing with
      the solve: at a nearest point found by halving alone (no start, no step of Newton's),
      the distance is at most the distance to that point brought onto the surface, and at
      least `n . p - h(n)` for the normal `n` there, `h` the support (the ninth review's
      bound); where the two are more than a ten-millionth apart, the review's own search over
      directions is taken too. Over a box thin along each axis in turn and a cylinder thin
      along its height or across it, from 1:1 to a hundred-millionth, at eleven roundnesses
      (0.02 to 1), over each face, beside each rim and at its corner, past each kind of edge
      and a corner, a hair inside and a hair outside each hard face, from a millionth of the
      thickness to a hundred sizes out, and the eighth and ninth reviews' samples by name:
      33,206 samples, the field within the bracket at every one: never outside it by more
      than 0.22 of what rounding the coordinates to single precision can move it, and, where
      that rounding is under a thousandth of the distance, never by more than 6.1e-5 of the
      distance (these two figures are for this set). Along lines through every one of those
      regions, finest at the hard surface (2,970 lines of 240 samples), the field changes no
      faster than the distance moved.
      A share runs with the crate's tests (every 29th sample, every 7th line); all of them
      with `--ignored`.
    - *2, at its cause.* Pruning now takes every node's distance at the ball's middle in one
      pass, each node once, and decides from those: a ball's pruning costs one evaluation of
      the tree, which is what it was charged. The distances are taken by the same steps in
      the same order, so every pruned tree and every mesh is the same to the bit, and no
      charge moved (only what was done uncharged is no longer done; the walk is no longer
      quadratic in the depth). **The bound that is true now**: every evaluation a build makes
      is charged its weight; a rounded sample's weight pays for 17 powers (3 at a roundness of
      1); every power past that, over the build, is charged 84 units; so no build takes more
      than one power for each 84 units it is charged, whatever its recipe. A test counts it:
      the weights of every primitive evaluated are under the work charged, and under 10, 40
      and 120 nested unions and under 60 unions each with a speck the powers and the rounded
      samples a unit of work are what the calibrated case's are, within a twentieth.
    - *3.* Asking which primitive made each triangle is billed as a build is, for the powers
      past the weights, inside its own limit; a test counts the same questions asked again.
    - *4.* Every exponential and logarithm the solve takes is counted as a power, and every
      square root as a quarter of one (a price stated, not timed: one instruction, against
      a few dozen for a power), rounded up to whole powers a sample. Nothing else repeats. A
      sample takes at most 4,328 powers, counted from the code (it was 510; the sure way's
      cap is 64 steps of up to 65 each). On the ninth review's own random set (2.83 million
      samples outside shapes 1:1 to a hundred-millionth thin, a millimetre to ten kilometres,
      at roundnesses 0.02 to 1) 1.0% took the sure way (3.1% before) and the most any took
      was 1,271.
    - *5.* What it costs now, by count, each figure for the samples named. The calibrated case
      (a cube of roundness 0.3 cut from a 2 m box, resolution 48) takes 18.0 powers a rounded
      sample and 10.9 a thousand units charged (16.3 and 9.5 before: the arrival rule's extra
      step and the square roots); the bound, 11.9 a thousand, is 1.10 times that. Under 10, 40,
      100 and 120 nested unions the same cube takes 0.94 to 0.56 of it a unit, and a sheet a
      millionth as thick at 0.67, cut through the middle, 1.03 (29.7 powers a sample). The
      far samples of the test above take 11.6 to 20.4 powers each (10.0 to 19.0 with the old
      arrival rule). Just past a thin face, a millionth to a thousand thicknesses out, a
      sheet's samples take 16 to 20 on average.
    - *6.* A test fails when the tally is not started again at a build (a build after a
      thousand evaluations on its thread is charged as one alone), when what was paid is not
      kept (a bill that over-runs, falls back and over-runs again charges each power once),
      and when the colours are not charged as they go (no more than a tenth of a coloured
      build's powers run between two charges).
    - *What changed for recipes that built before:* no recipe in the repository uses
      `roundness`, and for every other primitive nothing is charged differently: no recipe's
      charge moved, and Sepulcra's asset tests pass with their meshes. Builds of rounded
      shapes that are not in the repository (the ninth review's table, as a share of the work
      limit at resolutions 48, 64, 80, 96 and 128): a lone cube of roundness 0.3 as before
      (0.469 at 64, refused at 96); the cube cut from the box 5% to 8% dearer (1.350 at 96,
      was 1.273); a cube of roundness 0.67 cut from it 10% to 11% dearer, and **refused at
      80, where it built** (1.059, was 0.957); a 2 m sheet a millionth as thick through the
      middle at 0.67 0.856 at 64 (0.988 under the eighth's mend, 0.718 before it), and at
      0.4 **refused at 64, where it built** (1.026, was 0.952 and 0.718); boards, plates and
      discs within 2%.
    - *Left as it is:* times. The rates against a plain cut are for a quiet machine: `cargo run
      --release -p specula-model --example rounded_rates 5` times the calibrated case, the
      dearest by count, both reviews' cases (the plank past its face, the cube under 40 and
      120 unions) against a plain cut in the same run, with each one's powers a thousand
      units.
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

### What a recipe is built of (`crate::made`, `Model.extras.made`)

Every build records, for each shape the recipe names, its own outlines along its own axes and
**its own surface in the finished model**; what no record speaks for is `made_rest_m2` (the
studio reads it as shape language: [FORM.md](FORM.md), "From what the recipe is built of").
**The surface is measured, never reckoned** (since the record's second review, 2026-11-20):
every triangle of the finest level carries what it was made as (`Shape::owners`, a slot of
the build's record; empty, and free, where nothing asked, as at the far level), and a record's
surface is summed from its own triangles of the finished parts, once, at the end
(`made::gather`). How each piece of the modeller keeps the tie:

- **Operations**: a move, a turn, a scale by a vector, a taper, a twist, a lean, a bend and a
  displacement move points and leave each triangle's owner as it is; a copy (`array`,
  `radial`, `mirror`) copies it; a subdivision gives a triangle's four pieces its owner; a weld
  drops it with the triangles it drops. So a part's, an operand's and a finish's operations
  change each shape by its own factor.
- **Booleans**: every face of the result is a piece of one input triangle's face and keeps
  that triangle's owner. A cutter has none: the wall it cuts is the rest. A bevel's strips are
  its tubes': the rest. **Not kept exactly**: inside one boolean, coplanar pieces of one
  operand with one colour and tag are joined into one face before they are cut into
  triangles, and the face is its first piece's; only an operand that holds several shapes (a
  nested boolean with ops of its own, a sculpt) can blur so.
- **Sculpts**: `sdf::primitive_of_triangles` gives each triangle of the mesh to the primitive
  whose field is the tree's at its middle (in a union the nearest, in an intersection the
  furthest, in a difference the first child or the cutter that made the wall; in a smooth
  blend the nearest takes the blend). Every triangle is asked up to 16,384 (`MAX_OWNER_ASKED`)
  and every so many past it (asked every other, no primitive's share moved by more than 0.13%
  of the whole on a measured figure). The asking spends a limit of its own for the whole build
  (`MAX_OWNER_BUILD_WORK`, about an eighth of a second at the most), **never the build's**: a
  triangle not asked within it is the rest, and the sculpt's stat says how many
  (`owners_unanswered`). Each primitive that adds is a slot of its own, so a mirror's and a
  repeat's copies, an elongation and what an intersection leaves are counted as meshed, and
  the part's operations after it as they changed each.
- **Includes**: records carry the including part's path as its errors do
  (`parts[1].include(gem).parts[0].shape`). **Scatters**: each item is recorded by the surface
  of what was placed of it (each copy carries its variant's owners), under its path in the
  items; a variant never placed has no record, and an item's variants are one record (one
  record a path, everywhere).
- **Bounds**: 512 shapes and 8,192 slots recorded while building; then the 64 shapes with the
  most surface, each with at most 96 outlines (its largest primitives', the rest's weight as
  an outline of no points, read as not read), and at most 256 outlines of 128 points in all
  (a quarter of a megabyte). The finest level is measured before its parts are welded at
  10 µm.

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
