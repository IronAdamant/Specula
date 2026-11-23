# Design: the shape library (`specula-formae`)

The shape library is a table of named shapes that an agent starts from by name and changes by
numbers: plane figures (a heart, a gear, a Reuleaux triangle), curves (a spiral, a catenary) and
solids (the Platonic, Archimedean and Catalan solids, spheres three ways, tori, solids of
revolution, sweeps). The owner asked for it on 2026-11-19: "all known shapes ... in 2D, 2.5D and
3D", "scalable", and "a base template" to reuse rather than make again. It sits beside the
measured human figure: `formae_list` shows both.

This document says what the library is, its conventions, how detail and error work, its limits,
what is not in it and why, how the engine and the modeller take it, and the table of every form.
The crate's own top comment (`crates/specula-formae/src/lib.rs`) is the reference for the code.

## What it is

- **A table.** Every form is one entry: a name (snake_case, unique over every name and alias), a
  kind (`plane`, `curve` or `solid`), a family, one line of what it is, its parameters (each with
  a unit, a default and a range), whether it is curved, its kin in the other dimensions (a
  circle's are the disc, the cylinder, the cone and the sphere), and the functions that build it.
  An alias is another word for a form, perhaps with numbers set (`hexagon` is `regular_polygon`
  with six sides; `cube` is `box` with one size). Words people use resolve: `ball`, `donut`,
  `pill`, `coin`, `pie`. Where a word is both a plane figure and a solid (`egg`, `disc`), the
  kind is asked.
- **Counted before it is built.** `count(detail)` is exact: the points and faces a build will
  have. A build over the ceiling (262,144 points) is refused with both numbers before any work.
- **Measured twice.** A form's measures (area or volume, perimeter or surface area, centroid,
  bounds) come in closed form where one is known, and as built from the shape itself. The tests
  hold one against the other.
- **Built well or refused.** A build is well made at every detail it accepts (outlines simple
  and wound right; solids watertight, wound outward, of the right genus, not crossing
  themselves), or it is refused as data: a code, the numbers, and a detail that holds where
  one does.
- **Deterministic.** The same arguments give the same bytes on every machine.

## Conventions

- Metres. Plane figures in x right, y up; solids right-handed, +Y up, standing on y = 0 with
  their middle on the Y axis, as the modeller's own solids do.
- A plane figure is rings of points: the first the outer boundary, counter-clockwise; further
  rings holes, clockwise. A curve is points in order, open or closed.
- A solid is a closed mesh of planar faces wound counter-clockwise from outside, each face in a
  named surface that is flat or smooth, with exact normals where a smooth surface is analytic.
- Each form says where its origin is (a circle's centre, a triangle's centroid, an arch's base).
- Angles in degrees.

## Detail and error

`detail` is the number of segments in a full turn (8 to 4,096; default 64). An arc of a quarter
turn has a quarter of them, at least two. The error at a detail is how far the build stands from
the true shape: for an arc, its sag `r (1 − cos(π/n))`, exact; for most solids of revolution a
bound; for parametric pieces (a spiral, a superellipse's own curve) an estimate, and said to be
one. `detail_for(radius, error)` answers the detail that keeps every facet within an error, so
an agent can ask for "no facet over a millimetre".

## Two and a half dimensions

In the three senses the word has in games, all built:

- **Raised**: any closed plane figure made a solid (`slab`, `bevelled`, `pillow`, `stepped`,
  `relief`), holes and all.
- **Drawn**: a solid drawn flat in a named view (top, front, side, isometric, the 2:1 game
  isometric, dimetric, cavalier, cabinet) with its hidden faces removed, each face with its
  depth and a shade from 0 to 1.
- **Layered**: a solid cut by a plane, or into evenly spaced layers (what sprite stacking draws).

## How the engine and the modeller take it

- **In a recipe** (`crates/specula-model`, PROCEDURAL_MODELLING.md, "Shapes by name"): an
  outline by name, `{"form": "heart", "args": {...}, "detail": 64}`, wherever an outline goes;
  a solid by name, `{"op": "form", "form": "icosahedron", "args": {...}}`; a plane figure
  raised, `{"op": "form", "form": "annulus", "raise": {"as": "slab", "thickness": 0.1}}`.
- **In the 2D world**: a `shape2d` item by a form's name, beside `rect`, `circle`, `polygon` and
  the rest. A plane figure or a curve:
  `{"form": "heart", "args": {"width": 1}, "detail": 64, "at": [x, y], "scale": 1, "rotate": 0,
  "fill": "#c03040"}` (a figure's holes are holes; a curve is stroked, never filled). A solid,
  drawn in a view or cut into layers, as polygons filled with the item's colour darkened by each
  face's shade (`0.25 + 0.75 × shade`):
  `{"solid": "icosahedron", "view": "isometric", "fill": "#8c96a3"}`, or
  `{"solid": "torus", "layers": {"count": 8, "axis": "y", "step": [0, 0.05]}, "fill": "#8c96a3"}`
  (each layer drawn `step` further than the one under it, lighter as it rises). **It is an item
  kind, and `formae_get` also hands back the item to paste**: the 2D world's shapes are saved
  and hashed by their description, so an item by name saves as its name and numbers (a few
  dozen bytes) and is built again the same on every machine, where a pasted list of polygons
  would save every point. What it makes is counted against the shape's own point budget (16,384
  input points) before it is built, and against the world's vertex budget as every shape is. A
  solid in the 2D world has at most 4,096 points and 4,096 faces (hidden faces are found pair by
  pair where the solid is not convex), and at most 64 layers.
- **Two read-only tools**, in a family of their own (`formae`):
  - `formae_list`: the catalogue. **With nothing asked, every name by kind and family, a line a
    family, and how to ask for more, in under 3 KB** (about 600 tokens: stage 3b, a cheap first
    read for a cold agent; it was 39 KB). With a kind, a family or a word, or `rows: true`: rows
    that paste (the recipe shape, a plane figure's outline in an extrusion and its `shape2d`
    item), each parameter with its default, at most 200 rows and 8 KB a page. `figures: true`
    adds the measured human figure: the `figure` shape and the `figure` rig, with their fields
    read from the modeller's own field table.
  - `formae_get {name, kind?, args?, detail?, view?, full?}`: one form, short by default (under
    3 KB, nothing built): the snippets to paste first (the recipe shape or outline, the raised
    forms, the `shape2d` item, a solid drawn and layered), then each parameter's default, unit
    and range, and its count. `full: true` adds its measures in closed form and as built, the
    error at that detail, its anchors, its kin and a drawing. An unknown name answers with the
    nearest.
  - **Where a cold agent meets it** (stage 3b): `model_build`'s description, the modelling
    family's line in the tool catalog, a new project's `notes/how.md` (so the brief at an
    agent's first call), and PROCEDURAL_MODELLING.md's list of shapes each name `formae_list`.
    A recipe that names a library word as a shape (`{"op": "heart"}`) or as an outline's kind
    (`{"heart": ...}`) is answered with how to write it by name, and a near miss
    (`{"op": "icosahedrn"}`) with the library's nearest names.
  - **Where their work runs**: on the calling agent's own protocol thread, never the engine
    thread. They read nothing of the world and the library is pure, so the engine neither waits
    for them nor holds anything for them. Each is bounded before it works: `formae_get` builds at
    most 16,384 points (a larger count is answered without the measures as built, with what to
    ask instead), draws at most 4,096 faces, and its answer has a ceiling of 64 KB.

## Limits

- Lengths from 0.1 mm to 10 km; counts and ratios within each form's own range.
- At most 262,144 points a build; at most 41 extra builds when a raised form's refusal looks for
  an inset that holds (the modeller gives it only what its budget has room for).
- The modeller holds `f32`: a form whose parts fall under what `f32` keeps at its size is refused
  there with its numbers, and a shape made of an outline by name is built closed or refused.

## Not here, and why

- **The four Kepler–Poinsot solids** as self-crossing surfaces, and **non-orientable or
  self-crossing surfaces** (a Klein bottle): every solid here is one closed, outward surface.
  The small stellated dodecahedron and the stella octangula are here as their outer surfaces.
- **A cross of three boxes as one surface**: a union, which the modeller's booleans make.
- **A hollow sphere**: two surfaces (the dome and the bowl, shells with an opening, are here).
- **Letters and symbols; organic shapes** (bones, bodies): part 5's libraries. A leaf (and a
  petal) is here, as two arcs a side.
- **An involute gear**: the gear's teeth have straight flanks.
- **The Johnson solids** (cupolas, rotundas, augmented and diminished solids): rarely what a game
  asks for; a later stage.
- **Fractals whose parts touch**: the Sierpinski triangle (its holes meet the outline at points:
  not simple rings) and tetrahedron (its pieces meet at points: not one closed surface). The
  Menger sponge is one closed surface, but its count and its genus grow twentyfold a level:
  later. The Koch snowflake and the Sierpinski carpet are here, to a bounded depth.
- **Pipe tees and crosses**: two tubes' union, which the modeller's booleans make; the saddle
  where they meet is not a construction this library proves sound.
- **A screw's thread**: sound by construction (each section star-shaped about the axis), but its
  distance has no closed form and its build's field is past the field's ceiling at any useful
  detail: later.
- **What a recipe already says in one line from the table**: a chamfered or bevelled prism of
  any figure, a pillow, a plaque (the raise of a figure: `bevelled`, `pillow`, `slab`); an arch's
  ring and a barrel vault (`annular_sector` raised and turned upright); a ramp (`wedge`). The
  rounded cylinder is here although the raise of a circle bevelled round is the same solid,
  because turned it has exact measures and a field of a few operations, where the raise's is its
  build's.

## Stage 4: further families (2026-11-24)

What an agent building props, buildings, machines, plants and creatures reaches for and the first
table lacked: 11 plane figures and 13 solids, in seven new families and four old ones, each
entered once with its parameters, measures, anchors, count and field by the rules above.

- **Plane figures** (lines and arcs only, so everything about them is exact): three moulding
  profiles in a profile's frame (back on `x = 0`, bed on `y = 0`, face toward `+x`, so an outline
  by name sweeps along a wall): `cavetto`, `cyma_recta`, `cyma_reversa` (each S is two equal arcs
  of at most a quarter turn, point-symmetric about its middle, so its area is a polygon's; an S
  flatter than fifty to one is refused, as its arcs' centres would be too far off to keep the
  measures' digits); `arch_portal` (the wall round an arched opening); `stair_profile`; `grate`
  and `truss` (figures with holes, raised by a slab); `banner`; `leaf` (its width a share of its
  length, which bounds how shallow its arcs are); `koch_snowflake` (depth 0 to 7) and
  `sierpinski_carpet` (depth 1 to 5). New words for old shapes: `ovolo` and `quarter_round` (the
  sector at 90 degrees), `bead` and `astragal` (the semicircle).
- **Solids**: turned (a profile of lines and arcs from the axis to the axis: exact measures, a
  bounded error, an exact field): `rounded_cylinder`, the hollow vessels `cup`, `bell` and
  `goblet` (the cavity is part of the one profile: in from the floor's middle, round the rim, down
  the outside), and the props `bottle`, `vase`, `baluster`, `mushroom`. Flat: `chamfered_box`
  (every edge cut at 45 degrees and a triangle at each corner: 24 points, 26 faces), `stairs`,
  `crystal`, `gem` (a crown of triangles between the girdle and a table turned half a side; the
  table's corners must lie inside the girdle's sides). And `chain_link`: a round wire along a
  stadium, two half tori and two cylinders with every quad planar, whose field is the stadium's
  plane field lifted (`wire`: the distance to the curve less the wire's radius, exact on both
  sides while the wire is thinner than the bend, which the form requires).

## Adding a form

One entry in its family's list (`crates/specula-formae/src/forms/*.rs`) with its parameters and
its functions beside it: its exact pieces (lines, arcs, parametric pieces) or its construction,
its textbook formulas where there are any, its anchors. Counting, building, the exact measures
of lines and arcs and the error are derived from the pieces; the crate's tests run over the whole
table, so a new entry is tested by being there. Then write this document's table again:
`cargo test -p specula-formae --test doc_table -- --ignored` (a test holds the table to the
crate's, so it cannot go stale).

## What was measured

- The library's own build costs 2,800 to 6,400 ns a point for every curved solid near 32,000
  points (the Möbius band 99,000, which searches its twisted strips for faces that cross), on
  the reference laptop while other builds ran on it (PROCEDURAL_MODELLING.md, "Shapes by name").
- Its default tests run in about a minute and a half of wall time on a busy machine, every word
  covered; the full runs are marked and run in release at a stage's end.

## Distance fields

`specula_formae::field` gives every form a signed distance field (negative inside), so that a
form can become a node of a sculpt tree (PROCEDURAL_MODELLING.md, tier 3; the node itself is a
later stage). A field says, beside its values:

- **its sides**: exact or a bound, outside and inside (a bound has the right sign and is never
  more than the distance, DISTANCE_FIELDS.md section 1);
- **its truth**: the form itself, or the form sampled at a detail (its parametric pieces, or its
  whole build), with how far that is from the form and whether that is a bound or the library's
  estimate (then the field is within twice it of the form's: DISTANCE_FIELDS.md 4.4);
- **a box** that holds its solid;
- **what a sample costs**, by count: the pieces looked at, the square roots, the trigonometric
  calls and the powers (a test holds every sample to it).

How each kind of form gets its field, and what it rests on:

| Forms | Field | Sides | Rests on |
|---|---|---|---|
| Plane figures | the distance to their lines and arcs exactly, a parametric piece's samples; the sign by a ray's crossings | exact | 2.1; the sign rule below |
| A figure raised as a slab (`raise::slab`, `ops::prism_of`, the table's extrusions) | the plane field raised | exact | 4.1 |
| A relief | the plate's slab and the figure's, the lesser | exact outside, a bound inside | 3.2, 4.1 |
| Solids of revolution (circle ring), and `ops::revolve_of` | the profile's field at the radius, the axis left out of its surface | exact | 4.2 |
| The same through part of a turn | its meeting with the turn's wedge | exact inside up to a half turn, else a bound; a bound outside | 3.3 |
| On an ellipse (ellipsoids, the elliptic cylinder) | the turned field scaled unevenly | a bound | 2.5 |
| Spheres built any way, the rounded box, the bicylinder | in closed form | exact; the bicylinder exact inside only | 2.1, below; 3.3 |
| A wire along a plane figure's boundary (the chain link) | the plane field's size at `(x, −z)`, lifted: `√(d² + (y − y₀)²) − r` | exact while `r` is under the curve's reach | 4.3; `field::Plan::wire` |
| Any other solid (polyhedra, pyramids, bevelled, pillowed and stepped figures, superellipsoids, sweeps) | its build at the detail: the nearest face, edge or corner where it is convex, else the nearest triangle with the winding number's sign | exact for the build | 2.1 |
| Curves | refused: a curve has no inside | | |

**The sign of a plane figure.** The parity of the crossings of a ray from the point along +x.
For simple rings, the holes inside the outer ring, the parity is odd exactly inside the figure.
A line is counted half-open in height, so a ray through a vertex where the ring passes through
counts it once and where the ring only touches the ray counts it twice or not at all; whether a
line is crossed is one orientation test with a fixed tie-break (a point on the line is read as
moved a little along +x, then a little less along +y), so a point on an edge is read as just to
its right, the same way every time. An arc counts as its chord and its segment: the arc with
its chord turned back is a simple closed curve, so the ring's winding number at a point is the
chord polygon's plus one for each arc's segment that holds the point, and the parity of a sum
is the sum of the parities. Whether a point is in a segment is "inside the circle, on the arc's
side of the chord", and the side is the same orientation test, with the same tie-break, as the
chord's crossing: so the two can never disagree at a point on a chord, which lies inside the
figure. An arc's distance and in-circle test are written from its ends (`|p − c|² − r²` as
`|w|² + 2r w·u` with `w` from the end and `u` the unit direction from the centre to it), so a
shallow arc whose centre is far away keeps the digits of its own size.

**The axis of a turned figure.** DISTANCE_FIELDS.md 4.2 takes the figure with its mirror image
in the axis. For a point on the figure's side of the axis, a mirrored piece is never nearer
than the piece it mirrors (`|r + r′| ≥ |r − r′|` for `r, r′ ≥ 0`), so the mirror changes only
which pieces are surface: the stretch of axis the figure touches is inside the mirrored figure
and is left out of the distance, and nothing else is.

**A convex solid swollen by a ball is its distance less the radius, on both sides** (the
rounded box: a box swollen by its rounding). *Proof.* Let `K` be convex and `K_r` the points
within `r` of it. Outside `K_r` this is DISTANCE_FIELDS.md 4.3. Inside `K_r`: let `x` be in it
and `s = s_K(x)`, so `s ≤ r`. A point `y` within `r − s` of `x` is within `r` of `K`
(`d(y, K) ≤ d(x, K)⁺ + |x − y|` where `x` is outside `K`; inside `K`, the ball of radius `−s`
about `x` is in `K`, so `d(y, K) ≤ (|x − y| + s)⁺ ≤ r`): the depth is at least `r − s`. And it
is no more: let `n` be the outward normal of a supporting plane of `K` at the point of `∂K`
nearest `x` (outside `K`, along `x` minus that point; inside, the plane of the nearest face
reached at depth `−s`); then `K` lies behind that plane, `K_r` lies behind it moved out by `r`,
and `x` is `r − s` from it. ∎ (Held by `tests/fields.rs` against the built rounded box.)

**What is not here.** No field of a curve. No field of a bevelled, pillowed or stepped figure,
a pyramid, a cone over a curved outline, a superellipsoid or a sweep (helix, torus knot, Möbius
band) of its own: theirs is their build's, exact for the build and within its chord error of
the form (an estimate for the sweeps and for a parametric piece of a figure), at a cost by its
triangles, and refused past 4,096 (`field_too_large`, naming a coarser detail that fits).

<!-- fields-table: begin -->
Every form at its defaults and detail 64 (made from the crate; held by `crates/specula-formae/tests/fields.rs`, written again by its ignored `write_the_fields_table`). Cost: pieces, roots and trigonometric calls a sample takes at most.

| Form | Kind | Field | Outside | Inside | Of | Cost |
|---|---|---|---|---|---|---|
| `triangle_sss` | plane | plane | exact | exact | the form | 3 / 1 / 0 |
| `triangle_sas` | plane | plane | exact | exact | the form | 3 / 1 / 0 |
| `triangle` | plane | plane | exact | exact | the form | 3 / 1 / 0 |
| `rectangle` | plane | plane | exact | exact | the form | 4 / 1 / 0 |
| `parallelogram` | plane | plane | exact | exact | the form | 4 / 1 / 0 |
| `rhombus` | plane | plane | exact | exact | the form | 4 / 1 / 0 |
| `kite` | plane | plane | exact | exact | the form | 4 / 1 / 0 |
| `dart` | plane | plane | exact | exact | the form | 4 / 1 / 0 |
| `trapezium` | plane | plane | exact | exact | the form | 4 / 1 / 0 |
| `regular_polygon` | plane | plane | exact | exact | the form | 6 / 1 / 0 |
| `star` | plane | plane | exact | exact | the form | 10 / 1 / 0 |
| `star_polygon` | plane | plane | exact | exact | the form | 10 / 1 / 0 |
| `cross` | plane | plane | exact | exact | the form | 12 / 1 / 0 |
| `saltire` | plane | plane | exact | exact | the form | 12 / 1 / 0 |
| `arrow` | plane | plane | exact | exact | the form | 7 / 1 / 0 |
| `double_arrow` | plane | plane | exact | exact | the form | 10 / 1 / 0 |
| `chevron` | plane | plane | exact | exact | the form | 6 / 1 / 0 |
| `shield` | plane | plane | exact | exact | the form | 5 / 3 / 0 |
| `l_section` | plane | plane | exact | exact | the form | 6 / 1 / 0 |
| `t_section` | plane | plane | exact | exact | the form | 8 / 1 / 0 |
| `u_section` | plane | plane | exact | exact | the form | 8 / 1 / 0 |
| `h_section` | plane | plane | exact | exact | the form | 12 / 1 / 0 |
| `z_section` | plane | plane | exact | exact | the form | 8 / 1 / 0 |
| `frame` | plane | plane | exact | exact | the form | 8 / 1 / 0 |
| `polygon_ring` | plane | plane | exact | exact | the form | 12 / 1 / 0 |
| `gear` | plane | plane | exact | exact | the form | 48 / 1 / 0 |
| `sawtooth_band` | plane | plane | exact | exact | the form | 18 / 1 / 0 |
| `battlement_band` | plane | plane | exact | exact | the form | 20 / 1 / 0 |
| `circle` | plane | plane | exact | exact | the form | 1 / 2 / 0 |
| `semicircle` | plane | plane | exact | exact | the form | 2 / 2 / 0 |
| `sector` | plane | plane | exact | exact | the form | 3 / 2 / 0 |
| `circular_segment` | plane | plane | exact | exact | the form | 2 / 2 / 0 |
| `annulus` | plane | plane | exact | exact | the form | 2 / 3 / 0 |
| `annular_sector` | plane | plane | exact | exact | the form | 4 / 3 / 0 |
| `lune` | plane | plane | exact | exact | the form | 2 / 3 / 0 |
| `lens` | plane | plane | exact | exact | the form | 2 / 3 / 0 |
| `stadium` | plane | plane | exact | exact | the form | 4 / 3 / 0 |
| `rounded_rectangle` | plane | plane | exact | exact | the form | 8 / 5 / 0 |
| `ellipse` | plane | plane | exact | exact | samples (error estimated) | 64 / 1 / 0 |
| `superellipse` | plane | plane | exact | exact | samples (error estimated) | 64 / 1 / 0 |
| `moss_egg` | plane | plane | exact | exact | the form | 4 / 5 / 0 |
| `oval` | plane | plane | exact | exact | the form | 4 / 5 / 0 |
| `ovoid` | plane | plane | exact | exact | samples (error estimated) | 64 / 1 / 0 |
| `teardrop` | plane | plane | exact | exact | the form | 3 / 2 / 0 |
| `heart` | plane | plane | exact | exact | samples (error estimated) | 64 / 1 / 0 |
| `round_arch` | plane | plane | exact | exact | the form | 4 / 2 / 0 |
| `pointed_arch` | plane | plane | exact | exact | the form | 5 / 3 / 0 |
| `horseshoe_arch` | plane | plane | exact | exact | the form | 4 / 2 / 0 |
| `ogee_arch` | plane | plane | exact | exact | the form | 7 / 5 / 0 |
| `keyhole` | plane | plane | exact | exact | the form | 4 / 2 / 0 |
| `reuleaux_polygon` | plane | plane | exact | exact | the form | 3 / 4 / 0 |
| `foil` | plane | plane | exact | exact | the form | 4 / 5 / 0 |
| `flower` | plane | plane | exact | exact | samples (error estimated) | 66 / 1 / 0 |
| `limacon` | plane | plane | exact | exact | samples (error estimated) | 64 / 1 / 0 |
| `epicycloid` | plane | plane | exact | exact | samples (error estimated) | 66 / 1 / 0 |
| `hypocycloid` | plane | plane | exact | exact | samples (error estimated) | 64 / 1 / 0 |
| `superformula` | plane | plane | exact | exact | samples (error estimated) | 66 / 1 / 0 |
| `cavetto` | plane | plane | exact | exact | the form | 5 / 2 / 0 |
| `cyma_recta` | plane | plane | exact | exact | the form | 6 / 3 / 0 |
| `cyma_reversa` | plane | plane | exact | exact | the form | 7 / 3 / 0 |
| `arch_portal` | plane | plane | exact | exact | the form | 8 / 2 / 0 |
| `stair_profile` | plane | plane | exact | exact | the form | 12 / 1 / 0 |
| `grate` | plane | plane | exact | exact | the form | 68 / 1 / 0 |
| `truss` | plane | plane | exact | exact | the form | 31 / 1 / 0 |
| `banner` | plane | plane | exact | exact | the form | 5 / 1 / 0 |
| `leaf` | plane | plane | exact | exact | the form | 4 / 5 / 0 |
| `koch_snowflake` | plane | plane | exact | exact | the form | 192 / 1 / 0 |
| `sierpinski_carpet` | plane | plane | exact | exact | the form | 40 / 1 / 0 |
| `line_segment` | curve | refused | - | - | refused: `a_curve_has_no_inside` | - |
| `zigzag` | curve | refused | - | - | refused: `a_curve_has_no_inside` | - |
| `arc` | curve | refused | - | - | refused: `a_curve_has_no_inside` | - |
| `elliptical_arc` | curve | refused | - | - | refused: `a_curve_has_no_inside` | - |
| `parabola` | curve | refused | - | - | refused: `a_curve_has_no_inside` | - |
| `catenary` | curve | refused | - | - | refused: `a_curve_has_no_inside` | - |
| `sine_wave` | curve | refused | - | - | refused: `a_curve_has_no_inside` | - |
| `archimedean_spiral` | curve | refused | - | - | refused: `a_curve_has_no_inside` | - |
| `logarithmic_spiral` | curve | refused | - | - | refused: `a_curve_has_no_inside` | - |
| `involute` | curve | refused | - | - | refused: `a_curve_has_no_inside` | - |
| `lemniscate` | curve | refused | - | - | refused: `a_curve_has_no_inside` | - |
| `rose_curve` | curve | refused | - | - | refused: `a_curve_has_no_inside` | - |
| `zigzag_band` | plane | plane | exact | exact | the form | 18 / 1 / 0 |
| `elliptical_arc_band` | plane | plane | exact | exact | samples (error estimated) | 34 / 1 / 0 |
| `parabola_band` | plane | plane | exact | exact | samples (error estimated) | 48 / 1 / 0 |
| `catenary_band` | plane | plane | exact | exact | samples (error estimated) | 38 / 1 / 0 |
| `sine_band` | plane | plane | exact | exact | samples (error estimated) | 258 / 1 / 0 |
| `archimedean_spiral_band` | plane | plane | exact | exact | samples (error estimated) | 386 / 1 / 0 |
| `logarithmic_spiral_band` | plane | plane | exact | exact | samples (error estimated) | 386 / 1 / 0 |
| `involute_band` | plane | plane | exact | exact | samples (error estimated) | 248 / 1 / 0 |
| `box` | solid | convex_build | exact | exact | the form | 30 / 1 / 0 |
| `wedge` | solid | convex_build | exact | exact | the form | 23 / 1 / 0 |
| `tetrahedron` | solid | convex_build | exact | exact | the form | 16 / 1 / 0 |
| `octahedron` | solid | convex_build | exact | exact | the form | 32 / 1 / 0 |
| `dodecahedron` | solid | convex_build | exact | exact | the form | 72 / 1 / 0 |
| `icosahedron` | solid | convex_build | exact | exact | the form | 80 / 1 / 0 |
| `tetrahedron_in_sphere` | solid | convex_build | exact | exact | the form | 16 / 1 / 0 |
| `cube_in_sphere` | solid | convex_build | exact | exact | the form | 30 / 1 / 0 |
| `octahedron_in_sphere` | solid | convex_build | exact | exact | the form | 32 / 1 / 0 |
| `dodecahedron_in_sphere` | solid | convex_build | exact | exact | the form | 72 / 1 / 0 |
| `icosahedron_in_sphere` | solid | convex_build | exact | exact | the form | 80 / 1 / 0 |
| `truncated_tetrahedron` | solid | convex_build | exact | exact | the form | 44 / 1 / 0 |
| `cuboctahedron` | solid | convex_build | exact | exact | the form | 62 / 1 / 0 |
| `truncated_cube` | solid | convex_build | exact | exact | the form | 86 / 1 / 0 |
| `truncated_octahedron` | solid | convex_build | exact | exact | the form | 86 / 1 / 0 |
| `rhombicuboctahedron` | solid | convex_build | exact | exact | the form | 122 / 1 / 0 |
| `truncated_cuboctahedron` | solid | convex_build | exact | exact | the form | 170 / 1 / 0 |
| `snub_cube` | solid | convex_build | exact | exact | the form | 158 / 1 / 0 |
| `icosidodecahedron` | solid | convex_build | exact | exact | the form | 152 / 1 / 0 |
| `truncated_dodecahedron` | solid | convex_build | exact | exact | the form | 212 / 1 / 0 |
| `truncated_icosahedron` | solid | convex_build | exact | exact | the form | 212 / 1 / 0 |
| `rhombicosidodecahedron` | solid | convex_build | exact | exact | the form | 302 / 1 / 0 |
| `truncated_icosidodecahedron` | solid | convex_build | exact | exact | the form | 422 / 1 / 0 |
| `snub_dodecahedron` | solid | convex_build | exact | exact | the form | 392 / 1 / 0 |
| `triakis_tetrahedron` | solid | convex_build | exact | exact | the form | 48 / 1 / 0 |
| `rhombic_dodecahedron` | solid | convex_build | exact | exact | the form | 60 / 1 / 0 |
| `triakis_octahedron` | solid | convex_build | exact | exact | the form | 96 / 1 / 0 |
| `tetrakis_hexahedron` | solid | convex_build | exact | exact | the form | 96 / 1 / 0 |
| `deltoidal_icositetrahedron` | solid | convex_build | exact | exact | the form | 120 / 1 / 0 |
| `disdyakis_dodecahedron` | solid | convex_build | exact | exact | the form | 192 / 1 / 0 |
| `pentagonal_icositetrahedron` | solid | convex_build | exact | exact | the form | 144 / 1 / 0 |
| `rhombic_triacontahedron` | solid | convex_build | exact | exact | the form | 150 / 1 / 0 |
| `triakis_icosahedron` | solid | convex_build | exact | exact | the form | 240 / 1 / 0 |
| `pentakis_dodecahedron` | solid | convex_build | exact | exact | the form | 240 / 1 / 0 |
| `deltoidal_hexecontahedron` | solid | convex_build | exact | exact | the form | 300 / 1 / 0 |
| `disdyakis_triacontahedron` | solid | convex_build | exact | exact | the form | 480 / 1 / 0 |
| `pentagonal_hexecontahedron` | solid | convex_build | exact | exact | the form | 360 / 1 / 0 |
| `prism` | solid | slab | exact | exact | the form | 6 / 2 / 0 |
| `antiprism` | solid | convex_build | exact | exact | the form | 62 / 1 / 0 |
| `pyramid` | solid | convex_build | exact | exact | the form | 31 / 1 / 0 |
| `pyramid_frustum` | solid | convex_build | exact | exact | the form | 30 / 1 / 0 |
| `bipyramid` | solid | convex_build | exact | exact | the form | 48 / 1 / 0 |
| `trapezohedron` | solid | convex_build | exact | exact | the form | 60 / 1 / 0 |
| `star_prism` | solid | slab | exact | exact | the form | 10 / 2 / 0 |
| `stella_octangula` | solid | mesh_build | exact | exact | the form | 24 / 73 / 24 |
| `small_stellated_dodecahedron` | solid | mesh_build | exact | exact | the form | 60 / 181 / 60 |
| `sphere` | solid | turned | exact | exact | the form | 2 / 3 / 0 |
| `icosphere` | solid | ball | exact | exact | the form | 0 / 1 / 0 |
| `cube_sphere` | solid | ball | exact | exact | the form | 0 / 1 / 0 |
| `ellipsoid` | solid | turned_stretched | bound | bound | samples (error a bound) | 33 / 2 / 0 |
| `hemisphere` | solid | turned | exact | exact | the form | 3 / 3 / 0 |
| `spherical_cap` | solid | turned | exact | exact | the form | 3 / 3 / 0 |
| `spherical_segment` | solid | turned | exact | exact | the form | 4 / 3 / 0 |
| `spherical_wedge` | solid | turned_part | bound | exact | the form | 2 / 3 / 0 |
| `cylinder` | solid | turned | exact | exact | the form | 4 / 2 / 0 |
| `elliptic_cylinder` | solid | turned_stretched | bound | bound | the form | 4 / 2 / 0 |
| `cylinder_sector` | solid | turned_part | bound | exact | the form | 4 / 2 / 0 |
| `tube` | solid | turned | exact | exact | the form | 4 / 2 / 0 |
| `cone` | solid | turned | exact | exact | the form | 3 / 2 / 0 |
| `frustum` | solid | turned | exact | exact | the form | 4 / 2 / 0 |
| `bicone` | solid | turned | exact | exact | the form | 3 / 2 / 0 |
| `capsule` | solid | turned | exact | exact | the form | 4 / 4 / 0 |
| `torus` | solid | turned | exact | exact | the form | 1 / 3 / 0 |
| `torus_sector` | solid | turned_part | bound | exact | the form | 1 / 3 / 0 |
| `elliptic_torus` | solid | turned | exact | exact | samples (error a bound) | 64 / 2 / 0 |
| `biconvex_lens` | solid | turned | exact | exact | the form | 3 / 4 / 0 |
| `spindle` | solid | turned | exact | exact | the form | 2 / 3 / 0 |
| `barrel` | solid | turned | exact | exact | the form | 4 / 3 / 0 |
| `hourglass` | solid | turned | exact | exact | the form | 4 / 3 / 0 |
| `egg_of_revolution` | solid | turned | exact | exact | the form | 4 / 5 / 0 |
| `teardrop_of_revolution` | solid | turned | exact | exact | the form | 3 / 3 / 0 |
| `paraboloid` | solid | turned | exact | exact | samples (error a bound) | 14 / 2 / 0 |
| `hyperboloid` | solid | turned | exact | exact | samples (error a bound) | 15 / 2 / 0 |
| `superellipsoid` | solid | convex_build | exact | exact | samples (error a bound) | 10112 / 1 / 0 |
| `rounded_box` | solid | rounded_box | exact | exact | the form | 0 / 1 / 0 |
| `dome` | solid | turned | exact | exact | the form | 4 / 4 / 0 |
| `bowl` | solid | turned | exact | exact | the form | 4 / 4 / 0 |
| `helix` | solid | refused | - | - | refused: `field_too_large` | - |
| `torus_knot` | solid | refused | - | - | refused: `field_too_large` | - |
| `mobius_band` | solid | mesh_build | exact | exact | samples (error estimated) | 1024 / 3073 / 1024 |
| `steinmetz_solid` | solid | meeting | bound | exact | the form | 0 / 2 / 0 |
| `round_arrow` | solid | turned | exact | exact | the form | 5 / 2 / 0 |
| `rounded_cylinder` | solid | turned | exact | exact | the form | 6 / 4 / 0 |
| `chamfered_box` | solid | convex_build | exact | exact | the form | 122 / 1 / 0 |
| `stairs` | solid | mesh_build | exact | exact | the form | 44 / 133 / 44 |
| `cup` | solid | turned | exact | exact | the form | 6 / 2 / 0 |
| `bell` | solid | turned | exact | exact | the form | 6 / 4 / 0 |
| `goblet` | solid | turned | exact | exact | the form | 8 / 4 / 0 |
| `bottle` | solid | turned | exact | exact | the form | 6 / 3 / 0 |
| `vase` | solid | turned | exact | exact | the form | 5 / 3 / 0 |
| `baluster` | solid | turned | exact | exact | the form | 9 / 3 / 0 |
| `mushroom` | solid | turned | exact | exact | the form | 5 / 3 / 0 |
| `crystal` | solid | convex_build | exact | exact | the form | 78 / 1 / 0 |
| `gem` | solid | convex_build | exact | exact | the form | 105 / 1 / 0 |
| `chain_link` | solid | wire | exact | exact | the form | 4 / 4 / 0 |
<!-- fields-table: end -->

## The table of every form

Made from the crate; held by `crates/specula-formae/tests/doc_table.rs`.

<!-- formae-table: begin -->
#### plane forms, triangle

| Form | What it is | Parameters |
|---|---|---|
| `triangle_sss` | a triangle by its three sides | `a` m 1 (0.0001..10000), `b` m 1 (0.0001..10000), `c` m 1 (0.0001..10000) |
| `triangle_sas` | a triangle by two sides and the angle between them | `a` m 1 (0.0001..10000), `b` m 1 (0.0001..10000), `angle` deg 60 (1..179) |
| `triangle` | a triangle by its base, its height and where the apex is | `base` m 1 (0.0001..10000), `height` m 1 (0.0001..10000), `apex` ratio 0 (-2..2) |

#### plane forms, quadrilateral

| Form | What it is | Parameters |
|---|---|---|
| `rectangle` | a rectangle | `width` m 1 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `parallelogram` | a parallelogram by its base, height and bottom-left angle | `base` m 1 (0.0001..10000), `height` m 1 (0.0001..10000), `angle` deg 60 (10..170) |
| `rhombus` | a rhombus by its two diagonals | `width` m 1 (0.0001..10000), `height` m 0.6 (0.0001..10000) |
| `kite` | a convex kite: two pairs of equal sides next to each other | `width` m 0.8 (0.0001..10000), `top` m 0.4 (0.0001..10000), `bottom` m 0.8 (0.0001..10000) |
| `dart` | a concave kite (an arrowhead): a tip, two wings and a reflex notch | `width` m 0.8 (0.0001..10000), `length` m 1 (0.0001..10000), `notch` m 0.4 (0.0001..10000) |
| `trapezium` | a quadrilateral with one pair of parallel sides (US: trapezoid) | `bottom` m 1 (0.0001..10000), `top` m 0.6 (0.0001..10000), `height` m 0.6 (0.0001..10000), `lean` ratio 0 (-4..4) |

#### plane forms, polygon

| Form | What it is | Parameters |
|---|---|---|
| `regular_polygon` | a regular polygon, a vertex on +y | `sides` count 6 (3..256), `radius` m 0.5 (0.0001..10000) |

#### plane forms, star

| Form | What it is | Parameters |
|---|---|---|
| `star` | a star of n points with its tip and notch radii | `points` count 5 (3..100), `outer` m 0.5 (0.0001..10000), `inner` m 0.2 (0.0001..10000) |
| `star_polygon` | the outline of the regular star polygon {n/k} | `points` count 5 (5..100), `step` count 2 (2..49), `radius` m 0.5 (0.0001..10000) |

#### plane forms, symbol

| Form | What it is | Parameters |
|---|---|---|
| `cross` | a plus-shaped cross | `width` m 1 (0.0001..10000), `height` m 1 (0.0001..10000), `bar` m 0.3 (0.0001..10000) |
| `saltire` | an X-shaped cross (the plus turned 45 degrees) | `span` m 1 (0.0001..10000), `bar` m 0.25 (0.0001..10000) |
| `arrow` | an arrow pointing along +x: a shaft and a triangular head | `length` m 1 (0.0001..10000), `shaft` m 0.2 (0.0001..10000), `head_length` m 0.35 (0.0001..10000), `head_width` m 0.5 (0.0001..10000) |
| `double_arrow` | an arrow with a head at each end | `length` m 1 (0.0001..10000), `shaft` m 0.2 (0.0001..10000), `head_length` m 0.35 (0.0001..10000), `head_width` m 0.5 (0.0001..10000) |
| `chevron` | a chevron: a V band pointing up, of a thickness measured vertically | `width` m 1 (0.0001..10000), `height` m 0.6 (0.0001..10000), `thickness` m 0.2 (0.0001..10000) |
| `shield` | a heater shield: a straight top and sides, and two arcs meeting at the point | `width` m 1 (0.0001..10000), `height` m 1.2 (0.0001..10000), `curve` ratio 1 (0.5..4) |

#### plane forms, section

| Form | What it is | Parameters |
|---|---|---|
| `l_section` | an L (angle) section | `width` m 0.6 (0.0001..10000), `height` m 1 (0.0001..10000), `thickness` m 0.15 (0.0001..10000) |
| `t_section` | a T section: a flange on top of a web | `width` m 1 (0.0001..10000), `height` m 1 (0.0001..10000), `flange` m 0.2 (0.0001..10000), `web` m 0.2 (0.0001..10000) |
| `u_section` | a U (channel) section, open upward | `width` m 1 (0.0001..10000), `height` m 0.8 (0.0001..10000), `thickness` m 0.15 (0.0001..10000) |
| `h_section` | an H (I-beam) section: two flanges joined by a web | `width` m 0.8 (0.0001..10000), `height` m 1 (0.0001..10000), `flange` m 0.15 (0.0001..10000), `web` m 0.15 (0.0001..10000) |
| `z_section` | a Z section: a web with flanges to opposite sides | `width` m 1 (0.0001..10000), `height` m 1 (0.0001..10000), `thickness` m 0.15 (0.0001..10000) |

#### plane forms, frame

| Form | What it is | Parameters |
|---|---|---|
| `frame` | a rectangle with a rectangular hole of an even border | `width` m 1 (0.0001..10000), `height` m 0.8 (0.0001..10000), `border` m 0.1 (0.0001..10000) |
| `polygon_ring` | a regular polygon with a similar hole | `sides` count 6 (3..256), `radius` m 0.5 (0.0001..10000), `inner` m 0.35 (0.0001..10000) |

#### plane forms, gear

| Form | What it is | Parameters |
|---|---|---|
| `gear` | a spur gear with straight flanks (not an involute profile) | `teeth` count 12 (3..200), `root` m 0.4 (0.0001..10000), `tip` m 0.5 (0.0001..10000), `tooth` ratio 0.5 (0.1..0.9), `land` ratio 0.5 (0.05..1) |

#### plane forms, band

| Form | What it is | Parameters |
|---|---|---|
| `sawtooth_band` | a strip with sawteeth on its top edge (a ramp and a drop) | `length` m 2 (0.0001..10000), `base` m 0.1 (0.0001..10000), `height` m 0.2 (0.0001..10000), `teeth` count 8 (1..1000) |
| `battlement_band` | a wall with merlons on its top edge, one at each end | `length` m 2 (0.0001..10000), `base` m 0.3 (0.0001..10000), `height` m 0.2 (0.0001..10000), `merlons` count 5 (2..200), `merlon` ratio 0.5 (0.1..0.9) |

#### plane forms, circle

| Form | What it is | Parameters |
|---|---|---|
| `circle` | a circle | `radius` m 0.5 (0.0001..10000) |
| `semicircle` | a half disc, flat side down | `radius` m 0.5 (0.0001..10000) |
| `sector` | a circular sector (a pie slice), from +x counter-clockwise | `radius` m 0.5 (0.0001..10000), `angle` deg 60 (1..359) |
| `circular_segment` | the part of a disc cut off by a chord | `radius` m 0.5 (0.0001..10000), `angle` deg 60 (1..359) |
| `annulus` | a disc with a round hole (a ring) | `radius` m 0.5 (0.0001..10000), `inner` m 0.3 (0.0001..10000) |
| `annular_sector` | a sector of a ring (a thick arc) | `radius` m 0.5 (0.0001..10000), `inner` m 0.3 (0.0001..10000), `angle` deg 90 (1..359) |
| `lune` | a crescent: a disc less an overlapping disc | `radius` m 0.5 (0.0001..10000), `cut` m 0.45 (0.0001..10000), `offset` m 0.25 (0.0001..10000) |
| `lens` | the common part of two equal discs | `radius` m 0.5 (0.0001..10000), `separation` ratio 1 (0.05..1.95) |
| `stadium` | a rectangle with half discs on its ends (the plane capsule) | `length` m 1 (0.0001..10000), `radius` m 0.25 (0.0001..10000) |
| `rounded_rectangle` | a rectangle with corners of a true radius | `width` m 1 (0.0001..10000), `height` m 0.6 (0.0001..10000), `radius` m 0.1 (0.0001..10000) |

#### plane forms, oval

| Form | What it is | Parameters |
|---|---|---|
| `ellipse` | an ellipse | `rx` m 0.6 (0.0001..10000), `ry` m 0.35 (0.0001..10000) |
| `superellipse` | a superellipse (Lamé curve) | `rx` m 0.5 (0.0001..10000), `ry` m 0.5 (0.0001..10000), `exponent` ratio 4 (0.25..20) |
| `moss_egg` | Moss's egg: four arcs of circles (a half circle, two of twice its radius, a small cap) | `radius` m 0.5 (0.0001..10000) |
| `oval` | the four-centre oval of the drafting construction (arcs of two radii) | `width` m 1 (0.0001..10000), `height` m 0.6 (0.0001..10000) |
| `ovoid` | an egg of two half ellipses on one widest line | `width` m 0.8 (0.0001..10000), `top` m 0.6 (0.0001..10000), `bottom` m 0.4 (0.0001..10000) |
| `teardrop` | a disc drawn out to a point by two tangent lines | `radius` m 0.3 (0.0001..10000), `length` m 0.8 (0.0001..10000) |
| `heart` | the classic heart curve (x = 16 sin³t, y = 13 cos t − 5 cos 2t − 2 cos 3t − cos 4t) | `width` m 1 (0.0001..10000) |

#### plane forms, arch

| Form | What it is | Parameters |
|---|---|---|
| `round_arch` | a round arch's opening: legs and a half circle | `width` m 1 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `pointed_arch` | a pointed (gothic) arch's opening: two arcs meeting at the apex | `width` m 1 (0.0001..10000), `height` m 1 (0.0001..10000), `radius` ratio 1 (0.5..3) |
| `horseshoe_arch` | a horseshoe arch's opening: an arc past the half circle, wider than its legs | `width` m 1 (0.0001..10000), `height` m 1 (0.0001..10000), `bulge` deg 30 (0..60) |
| `ogee_arch` | an ogee arch's opening: convex then concave arcs meeting at a point | `width` m 1 (0.0001..10000), `height` m 1 (0.0001..10000), `radius` ratio 0.4 (0.25..2) |

#### plane forms, tracery

| Form | What it is | Parameters |
|---|---|---|
| `keyhole` | a keyhole: a round head over a straight slot | `radius` m 0.25 (0.0001..10000), `slot` m 0.2 (0.0001..10000), `length` m 0.6 (0.0001..10000) |
| `reuleaux_polygon` | a Reuleaux polygon: a figure of constant width from arcs about opposite vertices | `sides` count 3 (3..15), `width` m 1 (0.0001..10000) |
| `foil` | an n-foil: overlapping round lobes about a filled middle | `lobes` count 4 (3..12), `radius` m 0.3 (0.0001..10000), `lobe` ratio 0.9 (0.2..1) |
| `flower` | a flower of rounded petals with sharp valleys (r = R (c + (1 − c)/cos(nθ/2)/)) | `petals` count 6 (3..24), `radius` m 0.5 (0.0001..10000), `inner` ratio 0.4 (0.05..0.95) |

#### plane forms, roulette

| Form | What it is | Parameters |
|---|---|---|
| `limacon` | the limaçon of Pascal, r = b (1 + k cos θ); past k = 1 its outer loop alone | `radius` m 0.3 (0.0001..10000), `ratio` ratio 0.6 (0..4) |
| `epicycloid` | the curve a point of a circle traces rolling round the outside of another | `cusps` count 3 (1..24), `radius` m 0.1 (0.0001..10000) |
| `hypocycloid` | the curve a point of a circle traces rolling round the inside of another | `cusps` count 4 (3..24), `radius` m 0.125 (0.0001..10000) |
| `superformula` | Gielis's superformula, a polar curve (closed and simple wherever it is accepted) | `m` count 6 (0..40), `n1` ratio 1 (0.5..20), `n2` ratio 7 (0.5..20), `n3` ratio 8 (0.5..20), `a` ratio 1 (0.25..4), `b` ratio 1 (0.25..4), `scale` m 0.5 (0.0001..10000) |

#### plane forms, moulding

| Form | What it is | Parameters |
|---|---|---|
| `cavetto` | a cove moulding: a block less a quarter disc at its outer upper corner (back on x = 0, bed on y = 0) | `width` m 0.1 (0.0001..10000), `height` m 0.1 (0.0001..10000), `radius` m 0.08 (0.0001..10000) |
| `cyma_recta` | an ogee moulding: an S of two equal arcs, convex below and concave above, level at its ends (a crown moulding's) | `width` m 0.1 (0.0001..10000), `height` m 0.1 (0.0001..10000), `fillet` m 0.012 (0.0001..10000) |
| `cyma_reversa` | a reverse ogee moulding: an S of two equal arcs, concave below and convex above, upright at its ends | `width` m 0.08 (0.0001..10000), `height` m 0.12 (0.0001..10000), `fillet` m 0.01 (0.0001..10000) |

#### plane forms, arch

| Form | What it is | Parameters |
|---|---|---|
| `arch_portal` | a wall with a round-arched opening from its floor (a doorway or a gate's wall) | `width` m 2 (0.0001..10000), `height` m 3 (0.0001..10000), `opening` m 1 (0.0001..10000), `spring` m 1.6 (0.0001..10000) |

#### plane forms, stair

| Form | What it is | Parameters |
|---|---|---|
| `stair_profile` | the side of a flight of steps rising along +x, down to the floor | `steps` count 5 (1..100), `rise` m 0.18 (0.0001..10000), `run` m 0.28 (0.0001..10000) |

#### plane forms, frame

| Form | What it is | Parameters |
|---|---|---|
| `grate` | a panel with a grid of rectangular holes (a grille, a vent, a window's bars) | `width` m 1 (0.0001..10000), `height` m 1 (0.0001..10000), `columns` count 4 (1..8), `rows` count 4 (1..8), `bar` m 0.05 (0.0001..10000) |
| `truss` | a Warren truss panel: chords, end posts and diagonals of one width, with triangular holes | `length` m 4 (0.0001..10000), `height` m 0.6 (0.0001..10000), `bays` count 4 (1..100), `bar` m 0.05 (0.0001..10000) |

#### plane forms, symbol

| Form | What it is | Parameters |
|---|---|---|
| `banner` | a swallowtail banner: a rectangle with a V cut into its fly end | `length` m 1 (0.0001..10000), `height` m 0.6 (0.0001..10000), `notch` m 0.25 (0.0001..10000) |

#### plane forms, leaf

| Form | What it is | Parameters |
|---|---|---|
| `leaf` | a leaf (or a petal) of two arcs a side, pointed at its base and its tip, widest where asked | `length` m 0.1 (0.0001..10000), `width` ratio 0.4 (0.02..1), `widest` ratio 0.4 (0.1..0.9) |

#### plane forms, fractal

| Form | What it is | Parameters |
|---|---|---|
| `koch_snowflake` | the Koch snowflake to a depth: each side of a triangle replaced by four, again and again | `side` m 1 (0.0001..10000), `depth` count 3 (0..5) |
| `sierpinski_carpet` | the Sierpinski carpet to a depth: a square with its middle ninth taken out, and each of the eight left the same | `size` m 1 (0.0001..10000), `depth` count 2 (1..3) |

#### curve forms, line

| Form | What it is | Parameters |
|---|---|---|
| `line_segment` | a straight segment along x | `length` m 1 (0.0001..10000) |
| `zigzag` | a zigzag polyline along x | `length` m 1 (0.0001..10000), `height` m 0.2 (0.0001..10000), `teeth` count 4 (1..1000) |

#### curve forms, arc

| Form | What it is | Parameters |
|---|---|---|
| `arc` | a circular arc from +x counter-clockwise | `radius` m 0.5 (0.0001..10000), `angle` deg 90 (1..359) |
| `elliptical_arc` | an arc of an ellipse, by polar angle from +x | `rx` m 0.6 (0.0001..10000), `ry` m 0.35 (0.0001..10000), `angle` deg 90 (1..359) |
| `parabola` | an arc of the parabola y = c x², its vertex at the origin | `width` m 1 (0.0001..10000), `height` m 0.5 (0.0001..10000) |
| `catenary` | a hanging chain, y = a (cosh(x / a) − 1) | `width` m 1 (0.0001..10000), `a` m 0.5 (0.0001..10000) |
| `sine_wave` | a sine wave along x | `length` m 2 (0.0001..10000), `amplitude` m 0.2 (0.0001..10000), `cycles` ratio 2 (0.25..100), `phase` deg 0 (-360..360) |

#### curve forms, spiral

| Form | What it is | Parameters |
|---|---|---|
| `archimedean_spiral` | the spiral r = start + pitch θ / 2π | `start` m 0.05 (0..10000), `pitch` m 0.1 (0.0001..10000), `turns` ratio 3 (0.25..50) |
| `logarithmic_spiral` | the spiral whose radius grows by a ratio each turn | `start` m 0.05 (0.0001..10000), `growth` ratio 1.5 (1.01..100), `turns` ratio 3 (0.25..20) |
| `involute` | the involute of a circle: the path of a string's end unwound from it | `radius` m 0.1 (0.0001..10000), `turns` ratio 2 (0.1..20) |

#### curve forms, loop

| Form | What it is | Parameters |
|---|---|---|
| `lemniscate` | the lemniscate of Bernoulli, a figure eight crossing itself at the origin | `radius` m 0.5 (0.0001..10000) |
| `rose_curve` | the rose r = R cos(kθ), every petal through the pole | `k` count 3 (1..24), `radius` m 0.5 (0.0001..10000) |

#### plane forms, band

| Form | What it is | Parameters |
|---|---|---|
| `zigzag_band` | a zigzag given a vertical thickness | `length` m 1 (0.0001..10000), `height` m 0.2 (0.0001..10000), `teeth` count 4 (1..1000), `thickness` m 0.05 (0.0001..10000) |
| `elliptical_arc_band` | an elliptical arc given a width: between two nested ellipses, by polar angle | `rx` m 0.6 (0.0001..10000), `ry` m 0.35 (0.0001..10000), `angle` deg 90 (1..359), `width` m 0.05 (0.0001..10000) |
| `parabola_band` | a parabola given a vertical thickness | `width` m 1 (0.0001..10000), `height` m 0.5 (0.0001..10000), `thickness` m 0.05 (0.0001..10000) |
| `catenary_band` | a catenary given a vertical thickness | `width` m 1 (0.0001..10000), `a` m 0.5 (0.0001..10000), `thickness` m 0.05 (0.0001..10000) |
| `sine_band` | a sine wave given a vertical thickness | `length` m 2 (0.0001..10000), `amplitude` m 0.2 (0.0001..10000), `cycles` ratio 2 (0.25..100), `phase` deg 0 (-360..360), `thickness` m 0.05 (0.0001..10000) |
| `archimedean_spiral_band` | an Archimedean spiral given a width along the radius | `start` m 0.05 (0..10000), `pitch` m 0.1 (0.0001..10000), `turns` ratio 3 (0.25..50), `width` m 0.04 (0.0001..10000) |
| `logarithmic_spiral_band` | a logarithmic spiral given a width along the radius, as a share of it | `start` m 0.05 (0.0001..10000), `growth` ratio 1.5 (1.01..100), `turns` ratio 3 (0.25..20), `width` ratio 0.2 (0.001..1.9) |
| `involute_band` | an involute given a true width, from where its curvature allows | `radius` m 0.1 (0.0001..10000), `turns` ratio 2 (0.1..20), `width` m 0.05 (0.0001..10000) |

#### solid forms, block

| Form | What it is | Parameters |
|---|---|---|
| `box` | a rectangular box (a cuboid) | `width` m 1 (0.0001..10000), `height` m 1 (0.0001..10000), `depth` m 1 (0.0001..10000) |
| `wedge` | a ramp: a box cut along a diagonal, rising to the high end at +x | `width` m 1 (0.0001..10000), `height` m 1 (0.0001..10000), `depth` m 1 (0.0001..10000) |

#### solid forms, platonic

| Form | What it is | Parameters |
|---|---|---|
| `tetrahedron` | the regular tetrahedron, by its edge | `edge` m 1 (0.0001..10000) |
| `octahedron` | the regular octahedron, by its edge | `edge` m 1 (0.0001..10000) |
| `dodecahedron` | the regular dodecahedron, by its edge | `edge` m 1 (0.0001..10000) |
| `icosahedron` | the regular icosahedron, by its edge | `edge` m 1 (0.0001..10000) |
| `tetrahedron_in_sphere` | the regular tetrahedron, by the radius of the sphere through its points | `radius` m 1 (0.0001..10000) |
| `cube_in_sphere` | the cube, by the radius of the sphere through its corners | `radius` m 1 (0.0001..10000) |
| `octahedron_in_sphere` | the regular octahedron, by the radius of the sphere through its points | `radius` m 1 (0.0001..10000) |
| `dodecahedron_in_sphere` | the regular dodecahedron, by the radius of the sphere through its points | `radius` m 1 (0.0001..10000) |
| `icosahedron_in_sphere` | the regular icosahedron, by the radius of the sphere through its points | `radius` m 1 (0.0001..10000) |

#### solid forms, archimedean

| Form | What it is | Parameters |
|---|---|---|
| `truncated_tetrahedron` | the truncated tetrahedron: 4 triangles, 4 hexagons | `edge` m 1 (0.0001..10000) |
| `cuboctahedron` | the cuboctahedron: 8 triangles, 6 squares | `edge` m 1 (0.0001..10000) |
| `truncated_cube` | the truncated cube: 8 triangles, 6 octagons | `edge` m 1 (0.0001..10000) |
| `truncated_octahedron` | the truncated octahedron: 6 squares, 8 hexagons | `edge` m 1 (0.0001..10000) |
| `rhombicuboctahedron` | the (small) rhombicuboctahedron: 8 triangles, 18 squares | `edge` m 1 (0.0001..10000) |
| `truncated_cuboctahedron` | the truncated cuboctahedron (great rhombicuboctahedron): 12 squares, 8 hexagons, 6 octagons | `edge` m 1 (0.0001..10000) |
| `snub_cube` | the snub cube: 32 triangles, 6 squares (this hand of the two) | `edge` m 1 (0.0001..10000) |
| `icosidodecahedron` | the icosidodecahedron: 20 triangles, 12 pentagons | `edge` m 1 (0.0001..10000) |
| `truncated_dodecahedron` | the truncated dodecahedron: 20 triangles, 12 decagons | `edge` m 1 (0.0001..10000) |
| `truncated_icosahedron` | the truncated icosahedron (a football's pattern): 12 pentagons, 20 hexagons | `edge` m 1 (0.0001..10000) |
| `rhombicosidodecahedron` | the (small) rhombicosidodecahedron: 20 triangles, 30 squares, 12 pentagons | `edge` m 1 (0.0001..10000) |
| `truncated_icosidodecahedron` | the truncated icosidodecahedron (great rhombicosidodecahedron): 30 squares, 20 hexagons, 12 decagons | `edge` m 1 (0.0001..10000) |
| `snub_dodecahedron` | the snub dodecahedron: 80 triangles, 12 pentagons (this hand of the two) | `edge` m 1 (0.0001..10000) |

#### solid forms, catalan

| Form | What it is | Parameters |
|---|---|---|
| `triakis_tetrahedron` | the triakis tetrahedron: 12 isosceles triangles (the truncated tetrahedron's dual) | `radius` m 1 (0.0001..10000) |
| `rhombic_dodecahedron` | the rhombic dodecahedron: 12 rhombi (the cuboctahedron's dual) | `radius` m 1 (0.0001..10000) |
| `triakis_octahedron` | the triakis octahedron: 24 isosceles triangles (the truncated cube's dual) | `radius` m 1 (0.0001..10000) |
| `tetrakis_hexahedron` | the tetrakis hexahedron: 24 isosceles triangles (the truncated octahedron's dual) | `radius` m 1 (0.0001..10000) |
| `deltoidal_icositetrahedron` | the deltoidal icositetrahedron: 24 kites (the rhombicuboctahedron's dual) | `radius` m 1 (0.0001..10000) |
| `disdyakis_dodecahedron` | the disdyakis dodecahedron: 48 scalene triangles (the truncated cuboctahedron's dual) | `radius` m 1 (0.0001..10000) |
| `pentagonal_icositetrahedron` | the pentagonal icositetrahedron: 24 irregular pentagons (the snub cube's dual) | `radius` m 1 (0.0001..10000) |
| `rhombic_triacontahedron` | the rhombic triacontahedron: 30 golden rhombi (the icosidodecahedron's dual) | `radius` m 1 (0.0001..10000) |
| `triakis_icosahedron` | the triakis icosahedron: 60 isosceles triangles (the truncated dodecahedron's dual) | `radius` m 1 (0.0001..10000) |
| `pentakis_dodecahedron` | the pentakis dodecahedron: 60 isosceles triangles (the truncated icosahedron's dual) | `radius` m 1 (0.0001..10000) |
| `deltoidal_hexecontahedron` | the deltoidal hexecontahedron: 60 kites (the rhombicosidodecahedron's dual) | `radius` m 1 (0.0001..10000) |
| `disdyakis_triacontahedron` | the disdyakis triacontahedron: 120 scalene triangles (the truncated icosidodecahedron's dual) | `radius` m 1 (0.0001..10000) |
| `pentagonal_hexecontahedron` | the pentagonal hexecontahedron: 60 irregular pentagons (the snub dodecahedron's dual) | `radius` m 1 (0.0001..10000) |

#### solid forms, prismatoid

| Form | What it is | Parameters |
|---|---|---|
| `prism` | a right prism on a regular polygon (a vertex on −z) | `sides` count 6 (3..256), `radius` m 0.5 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `antiprism` | a regular antiprism: two regular polygons, the top turned half a side, joined by triangles | `sides` count 6 (3..256), `radius` m 0.5 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `pyramid` | a right pyramid on a regular polygon | `sides` count 6 (3..256), `radius` m 0.5 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `pyramid_frustum` | a right pyramid's frustum: a regular polygon below, a smaller one above | `sides` count 4 (3..256), `radius` m 0.5 (0.0001..10000), `top` m 0.25 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `bipyramid` | two right pyramids base to base (a dipyramid), standing on a tip | `sides` count 6 (3..256), `radius` m 0.5 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `trapezohedron` | a trapezohedron: 2n kites about two tips (the antiprism's dual; d10 at five), standing on a tip | `sides` count 6 (3..256), `radius` m 0.5 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `star_prism` | a prism on a star of n points | `points` count 5 (3..100), `outer` m 0.5 (0.0001..10000), `inner` m 0.2 (0.0001..10000), `height` m 0.2 (0.0001..10000) |

#### solid forms, stellated

| Form | What it is | Parameters |
|---|---|---|
| `stella_octangula` | the stella octangula's outer surface: a regular tetrahedron raised on each face of an octahedron (two tetrahedra through each other) | `edge` m 1 (0.0001..10000) |
| `small_stellated_dodecahedron` | the small stellated dodecahedron's outer surface: a pentagonal pyramid of golden triangles on each face of a dodecahedron of this edge | `edge` m 1 (0.0001..10000) |

#### solid forms, sphere

| Form | What it is | Parameters |
|---|---|---|
| `sphere` | a sphere of latitudes and longitudes (rings of points at equal angles from pole to pole) | `radius` m 0.5 (0.0001..10000) |
| `icosphere` | a geodesic sphere: an icosahedron's faces cut into triangles and pushed out to the sphere (points spread nearly evenly) | `radius` m 0.5 (0.0001..10000) |
| `cube_sphere` | a cube's faces cut at equal angles and pushed out to the sphere (an equiangular cube-sphere) | `radius` m 0.5 (0.0001..10000) |
| `ellipsoid` | an ellipsoid of three semi-axes (an elliptic profile turned on an ellipse: latitudes and longitudes, cut at the equator and the axes) | `rx` m 0.6 (0.0001..10000), `ry` m 0.4 (0.0001..10000), `rz` m 0.5 (0.0001..10000) |
| `hemisphere` | half a ball, flat side down | `radius` m 0.5 (0.0001..10000) |
| `spherical_cap` | the part of a ball cut off by a plane, flat side down | `radius` m 0.5 (0.0001..10000), `height` m 0.25 (0.0001..10000) |
| `spherical_segment` | a zone: the part of a ball between two parallel planes | `radius` m 0.5 (0.0001..10000), `from` ratio -0.5 (-0.999..0.999), `to` ratio 0.5 (-0.999..0.999) |
| `spherical_wedge` | a wedge of a ball between two half planes through its axis (Y) | `radius` m 0.5 (0.0001..10000), `angle` deg 90 (1..359) |

#### solid forms, cylinder

| Form | What it is | Parameters |
|---|---|---|
| `cylinder` | a right circular cylinder | `radius` m 0.5 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `elliptic_cylinder` | a right cylinder on an ellipse | `rx` m 0.6 (0.0001..10000), `rz` m 0.4 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `cylinder_sector` | a sector of a cylinder between two half planes through its axis (Y) | `radius` m 0.5 (0.0001..10000), `height` m 1 (0.0001..10000), `angle` deg 90 (1..359) |
| `tube` | a hollow cylinder (a pipe; a washer when short) | `radius` m 0.5 (0.0001..10000), `inner` m 0.4 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `cone` | a right circular cone | `radius` m 0.5 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `frustum` | a truncated cone: a cone with its tip cut off | `radius` m 0.5 (0.0001..10000), `top` m 0.25 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `bicone` | two cones base to base, standing on a tip | `radius` m 0.5 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `capsule` | a cylinder with half balls on its ends (the solid pill) | `radius` m 0.25 (0.0001..10000), `length` m 0.5 (0.0001..10000) |

#### solid forms, torus

| Form | What it is | Parameters |
|---|---|---|
| `torus` | a ring torus: a circle turned about an axis in its plane | `radius` m 0.5 (0.0001..10000), `tube` m 0.15 (0.0001..10000) |
| `torus_sector` | a bend of a torus between two half planes through its axis (a pipe elbow), capped | `radius` m 0.5 (0.0001..10000), `tube` m 0.15 (0.0001..10000), `angle` deg 90 (1..359) |
| `elliptic_torus` | a torus whose tube is an ellipse | `radius` m 0.5 (0.0001..10000), `width` m 0.15 (0.0001..10000), `height` m 0.25 (0.0001..10000) |

#### solid forms, revolution

| Form | What it is | Parameters |
|---|---|---|
| `biconvex_lens` | a lens of two equal spherical caps, rim to rim | `radius` m 0.5 (0.0001..10000), `thickness` m 0.2 (0.0001..10000) |
| `spindle` | a spindle (a lemon): a circle's arc turned about its chord | `radius` m 0.3 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `barrel` | a barrel: flat ends, its side an arc bulging out | `radius` m 0.4 (0.0001..10000), `bulge` m 0.5 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `hourglass` | an hourglass: flat ends, its side an arc drawn in to a waist | `radius` m 0.5 (0.0001..10000), `waist` m 0.2 (0.0001..10000), `height` m 1 (0.0001..10000) |
| `egg_of_revolution` | Moss's egg turned about its axis (arcs of circles) | `radius` m 0.5 (0.0001..10000) |
| `teardrop_of_revolution` | the plane teardrop turned about its axis: a ball drawn out to a point | `radius` m 0.3 (0.0001..10000), `length` m 0.8 (0.0001..10000) |
| `paraboloid` | a paraboloid, its bowl filled and capped flat | `radius` m 0.5 (0.0001..10000), `height` m 0.5 (0.0001..10000) |
| `hyperboloid` | a hyperboloid of one sheet, capped flat at both ends (a cooling tower) | `radius` m 0.5 (0.0001..10000), `waist` m 0.3 (0.0001..10000), `height` m 1 (0.0001..10000) |

#### solid forms, rounded

| Form | What it is | Parameters |
|---|---|---|
| `superellipsoid` | a superellipsoid, /x/a/^n + /y/b/^n + /z/c/^n = 1 (a rounded cube by its exponent): a superellipse profile turned on a superellipse | `rx` m 0.5 (0.0001..10000), `ry` m 0.5 (0.0001..10000), `rz` m 0.5 (0.0001..10000), `exponent` ratio 4 (1.5..32) |
| `rounded_box` | a box with edges and corners of a true radius (quarter cylinders and eighth balls) | `width` m 1 (0.0001..10000), `height` m 1 (0.0001..10000), `depth` m 1 (0.0001..10000), `radius` m 0.1 (0.0001..10000) |

#### solid forms, shell

| Form | What it is | Parameters |
|---|---|---|
| `dome` | a hemispherical shell, rim down | `radius` m 0.5 (0.0001..10000), `thickness` m 0.05 (0.0001..10000) |
| `bowl` | a hemispherical shell, rim up | `radius` m 0.5 (0.0001..10000), `thickness` m 0.05 (0.0001..10000) |

#### solid forms, sweep

| Form | What it is | Parameters |
|---|---|---|
| `helix` | a round wire wound in a helix about Y (a spring), its ends cut square | `radius` m 0.3 (0.0001..10000), `wire` m 0.03 (0.0001..10000), `pitch` m 0.15 (0.0001..10000), `coils` ratio 5 (0.25..50) |
| `torus_knot` | a round wire along the (p, q) torus knot: p times about the axis, q times through the hole | `radius` m 0.5 (0.0001..10000), `tube` m 0.2 (0.0001..10000), `wire` m 0.05 (0.0001..10000), `p` count 2 (2..16), `q` count 3 (1..16) |
| `mobius_band` | a Möbius band with a thickness: a rectangle swept round a circle turning half a turn | `radius` m 0.5 (0.0001..10000), `width` m 0.2 (0.0001..10000), `thickness` m 0.02 (0.0001..10000) |

#### solid forms, compound

| Form | What it is | Parameters |
|---|---|---|
| `steinmetz_solid` | the bicylinder: where two cylinders of one radius, along x and along z, cross | `radius` m 0.5 (0.0001..10000) |
| `round_arrow` | an arrow as one solid of revolution along +Y: a shaft and a conical head | `shaft` m 0.05 (0.0001..10000), `length` m 0.8 (0.0001..10000), `head` m 0.12 (0.0001..10000), `head_length` m 0.25 (0.0001..10000) |

#### solid forms, cylinder

| Form | What it is | Parameters |
|---|---|---|
| `rounded_cylinder` | a cylinder with its two rims rounded to a true radius (a puck, a coin, a button, a cushion) | `radius` m 0.5 (0.0001..10000), `height` m 0.2 (0.0001..10000), `fillet` m 0.05 (0.0001..10000) |

#### solid forms, block

| Form | What it is | Parameters |
|---|---|---|
| `chamfered_box` | a box with every edge cut at 45 degrees and a triangle at each corner (the bevelled cube of modelling tools) | `width` m 1 (0.0001..10000), `height` m 1 (0.0001..10000), `depth` m 1 (0.0001..10000), `chamfer` m 0.1 (0.0001..10000) |

#### solid forms, stair

| Form | What it is | Parameters |
|---|---|---|
| `stairs` | a straight flight of steps rising along +x, solid down to the floor | `steps` count 5 (1..100), `rise` m 0.18 (0.0001..10000), `run` m 0.28 (0.0001..10000), `width` m 1 (0.0001..10000) |

#### solid forms, vessel

| Form | What it is | Parameters |
|---|---|---|
| `cup` | a hollow vessel: a floor and a wall of one thickness, straight or tapered (a cup, a bucket, a pot) | `radius` m 0.4 (0.0001..10000), `top` m 0.45 (0.0001..10000), `height` m 0.5 (0.0001..10000), `wall` m 0.03 (0.0001..10000), `floor` m 0.04 (0.0001..10000) |
| `bell` | a hollow bell: a flat crown and a flare bowed in, its wall one thickness (two concentric arcs) | `radius` m 0.5 (0.0001..10000), `crown` m 0.2 (0.0001..10000), `height` m 0.6 (0.0001..10000), `wall` m 0.03 (0.0001..10000), `curve` ratio 1.5 (0.55..4) |
| `goblet` | a goblet: a hollow half-ball cup on a stem and a round foot | `radius` m 0.04 (0.0001..10000), `wall` m 0.003 (0.0001..10000), `stem` m 0.005 (0.0001..10000), `length` m 0.08 (0.0001..10000), `foot` m 0.035 (0.0001..10000), `base` m 0.004 (0.0001..10000) |

#### solid forms, lathe

| Form | What it is | Parameters |
|---|---|---|
| `bottle` | a bottle: a body, a shoulder (an arc upright at the body) and a neck | `radius` m 0.038 (0.0001..10000), `height` m 0.3 (0.0001..10000), `neck` m 0.0145 (0.0001..10000), `neck_length` m 0.09 (0.0001..10000), `shoulder` m 0.04 (0.0001..10000) |
| `vase` | a vase: a ball belly on a foot, a neck flaring to the mouth (solid: no cavity) | `radius` m 0.12 (0.0001..10000), `foot` m 0.06 (0.0001..10000), `neck` m 0.05 (0.0001..10000), `mouth` m 0.07 (0.0001..10000), `neck_length` m 0.08 (0.0001..10000) |
| `baluster` | a baluster: a ball on a plinth, a neck and a cap (a railing's post, a table's leg) | `radius` m 0.06 (0.0001..10000), `base` m 0.05 (0.0001..10000), `thickness` m 0.03 (0.0001..10000), `neck` m 0.025 (0.0001..10000), `neck_length` m 0.25 (0.0001..10000) |
| `mushroom` | a mushroom: a stem under a spherical cap | `stem` m 0.02 (0.0001..10000), `stem_height` m 0.06 (0.0001..10000), `cap` m 0.06 (0.0001..10000), `cap_height` m 0.04 (0.0001..10000) |

#### solid forms, crystal

| Form | What it is | Parameters |
|---|---|---|
| `crystal` | a crystal: a prism on a regular polygon with a pyramid on its top and, if asked, under its foot | `sides` count 6 (3..64), `radius` m 0.1 (0.0001..10000), `height` m 0.3 (0.0001..10000), `tip` m 0.1 (0.0001..10000), `foot` m 0.05 (0..10000) |
| `gem` | a cut stone: a flat table, a crown of triangles, a girdle and a pointed pavilion | `sides` count 8 (3..64), `radius` m 0.05 (0.0001..10000), `table` ratio 0.55 (0.1..0.95), `crown` m 0.016 (0.0001..10000), `pavilion` m 0.043 (0.0001..10000) |

#### solid forms, torus

| Form | What it is | Parameters |
|---|---|---|
| `chain_link` | a chain link: a round wire along a stadium (two half tori and two cylinders), lying flat | `radius` m 0.03 (0.0001..10000), `length` m 0.04 (0.0001..10000), `wire` m 0.008 (0.0001..10000) |

#### Aliases

| Word | Kind | Is | With |
|---|---|---|---|
| `equilateral_triangle` | plane | `triangle_sss` | `side` sets a, b, c |
| `isosceles_triangle` | plane | `triangle` | `apex` = 0 |
| `right_triangle` | plane | `triangle` | `apex` = -0.5 |
| `square` | plane | `rectangle` | `size` sets width, height |
| `oblong` | plane | `rectangle` | - |
| `diamond` | plane | `rhombus` | - |
| `lozenge` | plane | `rhombus` | - |
| `trapezoid` | plane | `trapezium` | - |
| `isosceles_trapezium` | plane | `trapezium` | `lean` = 0 |
| `isosceles_trapezoid` | plane | `trapezium` | `lean` = 0 |
| `right_trapezium` | plane | `trapezium` | `lean` = -1 |
| `right_trapezoid` | plane | `trapezium` | `lean` = -1 |
| `pentagon` | plane | `regular_polygon` | `sides` = 5 |
| `hexagon` | plane | `regular_polygon` | `sides` = 6 |
| `heptagon` | plane | `regular_polygon` | `sides` = 7 |
| `octagon` | plane | `regular_polygon` | `sides` = 8 |
| `nonagon` | plane | `regular_polygon` | `sides` = 9 |
| `decagon` | plane | `regular_polygon` | `sides` = 10 |
| `hendecagon` | plane | `regular_polygon` | `sides` = 11 |
| `dodecagon` | plane | `regular_polygon` | `sides` = 12 |
| `pentagram` | plane | `star_polygon` | `points` = 5, `step` = 2 |
| `hexagram` | plane | `star_polygon` | `points` = 6, `step` = 2 |
| `octagram` | plane | `star_polygon` | `points` = 8, `step` = 3 |
| `plus` | plane | `cross` | - |
| `channel` | plane | `u_section` | - |
| `i_beam` | plane | `h_section` | - |
| `angle_section` | plane | `l_section` | - |
| `disc` | plane | `circle` | - |
| `disk` | plane | `circle` | - |
| `half_circle` | plane | `semicircle` | - |
| `circular_sector` | plane | `sector` | - |
| `pie` | plane | `sector` | - |
| `quadrant` | plane | `sector` | `angle` = 90 |
| `ring` | plane | `annulus` | - |
| `crescent` | plane | `lune` | - |
| `moon` | plane | `lune` | - |
| `vesica_piscis` | plane | `lens` | `separation` = 1 |
| `pill` | plane | `stadium` | - |
| `obround` | plane | `stadium` | - |
| `rounded_rect` | plane | `rounded_rectangle` | - |
| `squircle` | plane | `superellipse` | `radius` sets rx, ry, `exponent` = 4 |
| `egg` | plane | `moss_egg` | - |
| `drop` | plane | `teardrop` | - |
| `raindrop` | plane | `teardrop` | - |
| `arch` | plane | `round_arch` | - |
| `gothic_arch` | plane | `pointed_arch` | - |
| `equilateral_arch` | plane | `pointed_arch` | `radius` = 1 |
| `reuleaux_triangle` | plane | `reuleaux_polygon` | `sides` = 3 |
| `trefoil` | plane | `foil` | `lobes` = 3 |
| `quatrefoil` | plane | `foil` | `lobes` = 4 |
| `cinquefoil` | plane | `foil` | `lobes` = 5 |
| `rose` | plane | `flower` | - |
| `cardioid` | plane | `limacon` | `ratio` = 1 |
| `nephroid` | plane | `epicycloid` | `cusps` = 2 |
| `deltoid` | plane | `hypocycloid` | `cusps` = 3 |
| `astroid` | plane | `hypocycloid` | `cusps` = 4 |
| `gielis` | plane | `superformula` | - |
| `cove` | plane | `cavetto` | - |
| `cove_moulding` | plane | `cavetto` | - |
| `ogee_moulding` | plane | `cyma_recta` | - |
| `crown_moulding` | plane | `cyma_recta` | - |
| `reverse_ogee` | plane | `cyma_reversa` | - |
| `ovolo` | plane | `sector` | `angle` = 90 |
| `quarter_round` | plane | `sector` | `angle` = 90 |
| `bead` | plane | `semicircle` | - |
| `astragal` | plane | `semicircle` | - |
| `doorway` | plane | `arch_portal` | - |
| `archway` | plane | `arch_portal` | - |
| `stair_side` | plane | `stair_profile` | - |
| `grille` | plane | `grate` | - |
| `grid_panel` | plane | `grate` | - |
| `warren_truss` | plane | `truss` | - |
| `girder` | plane | `truss` | - |
| `swallowtail` | plane | `banner` | - |
| `pennon` | plane | `banner` | - |
| `petal` | plane | `leaf` | - |
| `snowflake` | plane | `koch_snowflake` | - |
| `menger_carpet` | plane | `sierpinski_carpet` | - |
| `segment` | curve | `line_segment` | - |
| `polyline_zigzag` | curve | `zigzag` | - |
| `circular_arc` | curve | `arc` | - |
| `chain_curve` | curve | `catenary` | - |
| `sine` | curve | `sine_wave` | - |
| `wave` | curve | `sine_wave` | - |
| `spiral` | curve | `archimedean_spiral` | - |
| `equiangular_spiral` | curve | `logarithmic_spiral` | - |
| `figure_eight` | curve | `lemniscate` | - |
| `rhodonea` | curve | `rose_curve` | - |
| `cube` | solid | `box` | `size` sets width, height, depth |
| `hexahedron` | solid | `box` | `edge` sets width, height, depth |
| `d6` | solid | `box` | `size` sets width, height, depth |
| `cuboid` | solid | `box` | - |
| `rectangular_prism` | solid | `box` | - |
| `brick` | solid | `box` | - |
| `plank` | solid | `box` | - |
| `ramp` | solid | `wedge` | - |
| `d4` | solid | `tetrahedron` | - |
| `d8` | solid | `octahedron` | - |
| `d12` | solid | `dodecahedron` | - |
| `d20` | solid | `icosahedron` | - |
| `d10` | solid | `trapezohedron` | `sides` = 5 |
| `great_rhombicuboctahedron` | solid | `truncated_cuboctahedron` | - |
| `small_rhombicuboctahedron` | solid | `rhombicuboctahedron` | - |
| `great_rhombicosidodecahedron` | solid | `truncated_icosidodecahedron` | - |
| `small_rhombicosidodecahedron` | solid | `rhombicosidodecahedron` | - |
| `buckyball` | solid | `truncated_icosahedron` | - |
| `football` | solid | `truncated_icosahedron` | - |
| `soccer_ball` | solid | `truncated_icosahedron` | - |
| `triangular_prism` | solid | `prism` | `sides` = 3 |
| `pentagonal_prism` | solid | `prism` | `sides` = 5 |
| `hexagonal_prism` | solid | `prism` | `sides` = 6 |
| `octagonal_prism` | solid | `prism` | `sides` = 8 |
| `square_antiprism` | solid | `antiprism` | `sides` = 4 |
| `square_pyramid` | solid | `pyramid` | `sides` = 4 |
| `triangular_pyramid` | solid | `pyramid` | `sides` = 3 |
| `pentagonal_pyramid` | solid | `pyramid` | `sides` = 5 |
| `hexagonal_pyramid` | solid | `pyramid` | `sides` = 6 |
| `truncated_pyramid` | solid | `pyramid_frustum` | - |
| `dipyramid` | solid | `bipyramid` | - |
| `triangular_bipyramid` | solid | `bipyramid` | `sides` = 3 |
| `pentagonal_bipyramid` | solid | `bipyramid` | `sides` = 5 |
| `deltohedron` | solid | `trapezohedron` | - |
| `stellated_octahedron` | solid | `stella_octangula` | - |
| `ball` | solid | `sphere` | - |
| `globe` | solid | `sphere` | - |
| `uv_sphere` | solid | `sphere` | - |
| `geodesic_sphere` | solid | `icosphere` | - |
| `geosphere` | solid | `icosphere` | - |
| `spherified_cube` | solid | `cube_sphere` | - |
| `spheroid` | solid | `ellipsoid` | `radius` sets rx, rz |
| `oblate_spheroid` | solid | `ellipsoid` | `radius` sets rx, rz |
| `prolate_spheroid` | solid | `ellipsoid` | `radius` sets rx, rz |
| `half_sphere` | solid | `hemisphere` | - |
| `zone` | solid | `spherical_segment` | - |
| `spherical_lune` | solid | `spherical_wedge` | - |
| `disc` | solid | `cylinder` | - |
| `disk` | solid | `cylinder` | - |
| `coin` | solid | `cylinder` | - |
| `rod` | solid | `cylinder` | - |
| `half_cylinder` | solid | `cylinder_sector` | `angle` = 180 |
| `pipe` | solid | `tube` | - |
| `hollow_cylinder` | solid | `tube` | - |
| `washer` | solid | `tube` | - |
| `truncated_cone` | solid | `frustum` | - |
| `conical_frustum` | solid | `frustum` | - |
| `double_cone` | solid | `bicone` | - |
| `pill` | solid | `capsule` | - |
| `spherocylinder` | solid | `capsule` | - |
| `donut` | solid | `torus` | - |
| `doughnut` | solid | `torus` | - |
| `elbow` | solid | `torus_sector` | - |
| `pipe_elbow` | solid | `torus_sector` | - |
| `lemon` | solid | `spindle` | - |
| `egg` | solid | `egg_of_revolution` | - |
| `drop` | solid | `teardrop_of_revolution` | - |
| `raindrop` | solid | `teardrop_of_revolution` | - |
| `cooling_tower` | solid | `hyperboloid` | - |
| `rounded_cube` | solid | `superellipsoid` | `radius` sets rx, ry, rz |
| `rounded_cuboid` | solid | `rounded_box` | - |
| `spring` | solid | `helix` | - |
| `coil` | solid | `helix` | - |
| `trefoil_knot` | solid | `torus_knot` | `p` = 2, `q` = 3 |
| `cinquefoil_knot` | solid | `torus_knot` | `p` = 2, `q` = 5 |
| `moebius_band` | solid | `mobius_band` | - |
| `mobius_strip` | solid | `mobius_band` | - |
| `bicylinder` | solid | `steinmetz_solid` | - |
| `arrow_solid` | solid | `round_arrow` | - |
| `puck` | solid | `rounded_cylinder` | - |
| `tablet` | solid | `rounded_cylinder` | - |
| `bevelled_box` | solid | `chamfered_box` | - |
| `beveled_box` | solid | `chamfered_box` | - |
| `staircase` | solid | `stairs` | - |
| `steps` | solid | `stairs` | - |
| `bucket` | solid | `cup` | - |
| `pail` | solid | `cup` | - |
| `tumbler` | solid | `cup` | - |
| `flowerpot` | solid | `cup` | - |
| `chalice` | solid | `goblet` | - |
| `wine_glass` | solid | `goblet` | - |
| `urn` | solid | `vase` | - |
| `toadstool` | solid | `mushroom` | - |
| `gemstone` | solid | `gem` | - |
| `jewel` | solid | `gem` | - |
| `brilliant` | solid | `gem` | - |
| `link` | solid | `chain_link` | - |
<!-- formae-table: end -->
