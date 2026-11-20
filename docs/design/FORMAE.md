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
- **Letters and symbols; organic shapes** (leaves, bones): part 5's libraries.
- **An involute gear**: the gear's teeth have straight flanks.
- **The Johnson solids and simple fractals**: the next stage's.

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
<!-- formae-table: end -->
