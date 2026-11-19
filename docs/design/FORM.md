# Design: form within an agent's reach

The owner, 2026-11-16, on the five small items of [the research note](../research/FORM_AND_LIGHT.md):
"Add all. This engine SHOULD help you. Not leave you lacking, and dealing with people's complaints."

An agent describes shapes in numbers and cannot see curvature. So round and human form is carried by
three things: **handles** whose plain result is round, **views** in which a flaw makes a pattern, and
**measures** that say what a picture cannot. This page says where each of the five items stands.

| Item | State | Where |
|---|---|---|
| Roundness as one number; bone chains | built | [PROCEDURAL_MODELLING.md](PROCEDURAL_MODELLING.md), tier 3 |
| The engine's own figure from a skeleton | built, and not yet right to look at | [FIGURE.md](FIGURE.md), [CORPUS.md](CORPUS.md) |
| The form views | built (this page) | `sight_frame`, `sight_orbit` |
| The form checks | built (this page) | `style.toml` `[form]`; every asset's facts |
| Shape language in the presets | built (this page) | `[taste] shape_language`; every asset's taste measures |

## The form views

`sight_frame` and `sight_orbit` take `form`: `clay`, `normals`, `curvature` or `stripes` (`world`, the
frame as it is, unless given). A form view draws the same surfaces from the same camera and shows
**shape alone**: nothing of a surface's own colour, texture, metal or glow, and nothing of the world's
light, sky, fog, post effects, particles or decals. The ground behind is a dark neutral. The packet's
`form` says which view it is and what was left out, and the image's colours say nothing about materials.

Why they exist: twice an independent check of the figure could not say whether its trunk was flat or
round, because the world's light came from the front and the surface was one even grey. A picture
lit that way cannot show form, however long it is looked at.

| View | What it shows | What to look for |
|---|---|---|
| `clay` | every lit surface as the same matte clay, under a studio's light that comes with the camera whichever way it looks: a key from the upper left, a fill from the right, the glow of a pale ceiling and a dim floor, a soft shine. A surface whose normal points away from the viewer is stained red | the first look at any shape: it reads the same from every side and under any world's light; flat where it should be round shows as an even tone; **red is an inside-out face** (or the inside of an open shape) |
| `normals` | each surface's own normal as colour: world x, y, z as red, green, blue (128 is 0). Not turned toward the viewer, as the lit frame turns it | normals that point the wrong way; **hard edges and flat-shaded faces, as steps of colour**; a seam, where two neighbours differ by a step; shading that does not follow the shape |
| `curvature` | how fast a **smooth-shaded** surface's normal turns: mid grey where it is flat, lighter where it turns away (a ridge, a ball), darker where it turns in (a groove) | where a form that should swell is flat; a ledge or a soft crease as a light or dark band. **A hard edge does not show here**, and a flat-shaded shape reads flat however coarse it is: the normal does not turn within a face. Look for those in `normals`, and in the asset's facts (`crease_m`, `razor_share`, and `faceting` for what its recipe names round) |
| `stripes` | the level bands a mirror of this shape would show of a striped room | bands that run evenly on an even surface, and kink, pinch or break where the surface has a dent, a ridge or a seam too slight to see otherwise (how a car's body is checked) |

- **The curvature view's scale** (`form_scale`, metres, 0.001 to 1000): the radius of the ball that
  shows halfway from mid grey to white (191). The turn is eased and never cut off: a ball a tenth of
  that radius is nearly white, and a limb and a finger on it both show. Without `form_scale` the
  capture takes a twentieth of the view's half height where the camera looks. The packet says the
  scale it drew at (`form.scale_m`). Smaller shows fine detail; larger the big forms.
- **Stripes fade where they would be narrower than a few pixels** (a finger, a toe): drawn, they are
  a shimmer that says nothing. Mid grey in a stripes view means "too fine to band at this size".
- **They are exact.** The frame is drawn with no tone curve, grade, bloom or anti-aliasing, so a
  colour means what the table says: a wall facing +z reads (128, 128, 255) in `normals`, and a flat
  wall reads 128 in `curvature`, on every adapter (held by a test on each).
- **Meshes and the ground take them** (terrain's shader calls the same views). **What is not a lit
  surface is drawn as ever** or not at all: water and glass are drawn as water and glass, under a
  plain light; sprites, tiles and in-world text are drawn as they are; a shadow's disc, particles and
  decals are left out, and the packet says so (its `effects` say they were not drawn, its `lights`
  count none, and its `gi` says no baked light was drawn and why). The ID buffer is the world's, so
  `visible_px` and the boxes are the same as in the world's frame (a test).
- **Not with the views of baked light.** `gi: "irradiance"` or `"probes"` with a `form` is refused:
  the one shows light and the other none.
- **A form view is for one capture.** The window is never drawn as one, and the capture after it is
  the world's again, to the byte (a test).
- **Normals are the shading normals**: a normal map shows in `normals`, `curvature` and `stripes`,
  since it is what a light would follow.
- **Cost to the world's frame: none, by construction and by measurement.** The views are in
  fragment shaders of their own (`mesh_form.frag`, `mesh_tex_form.frag`, `terrain_form.frag`), and
  the world's four compiled shaders are byte for byte what they were before the views existed (a
  comparison with the commit before them). Measured on the reference iGPU at 1600 x 900, twelve
  rounds of each build alternating: the scene pass of `cubes_10k` 1.5211 ms against 1.5208 ms
  before the feature (ranges 1.5190 to 1.5249 and 1.5195 to 1.5226), `game` 4.2317 against 4.2332,
  `mixed_1000` 5.0224 against 5.0222. (They were first a branch on a value of the frame in every
  lit pipeline: 2 to 5% slower. Then a branch behind a specialisation constant in the same shader:
  the Intel driver still charged one scene 2.2%. Two reviewers measured those.)
- **A form view's pipelines are built on a worker, the first time one is asked for**, and the
  capture or the sheet that asked waits for them, a pass at a time: no frame ever compiles one.
  (`Variants::form`, `Gpu::request_form_pipelines`; a frame drawn as a form view before they exist
  draws no mesh and no ground. Compiled inside the first form capture's render they held the engine
  thread 440 ms and left a budget error for the rest of the run.) A set that failed to build is
  reported by the call (`gpu_failed`, step `form_pipelines`; the studio: `render_unavailable`).
- **Under an orthographic camera** every pixel is seen along the camera's own direction (told as
  the renderer tells it: the projection's bottom row). Taken from the camera's place, clay stained
  a ring of true outline red.

How they are made: the frame carries the view and the scale in two spare slots of its uniform
(`FrameData::sky_params.z` and `.w`); the views are in `shaders/form.glsl`, which `mesh_frag.glsl`
and `terrain_frag.glsl` include only under `#ifdef FORM` (the form entry points define it) and
return from before any light is gathered; a pipeline whose features have `FEAT_FORM` takes those
shaders; `sight::form_frame` turns a capture's frame into the plain one a view is drawn in. Clay's light takes the camera's own right and up from the frame's inverse projection.

**The review of the form views** (a model that did not write them, 2026-11-19) said not to merge
them: clay's light was built from the world's up and each pixel's own direction, so four balls seen
from above were lit from four sides; `gi: "probes"` with a form drew the probes' markers as clay
balls and every form capture in a world with baked light said its light "was not ready"; an
inside-out ball was the same picture as a right one, and the description promised hard edges in
curvature, which cannot show them; and the world's frame was slower. Each is mended and is a test;
shadows' discs are left out and the packet's lights count what was drawn.

**A second review, of those mends** (another model, 2026-11-19), said not to merge them: the first
form capture compiled its pipelines on the engine thread (440 ms, and a budget error that never
cleared); on the iGPU one scene of the world's was still 2.2% slower (twelve alternating rounds,
no overlap; isolated to the branch in `shade_lit`); and clay stained outlines under an
orthographic camera. All three are mended as said above; the orthographic case is a test that was
shown to fail on the fault. It found sound: the camera's axes in every projection and roll,
terrain (no stain along ridges), frames to the byte after form frames, the pipeline keys, the
packet.

**A third review, of the second mends and the sheets** (another model, 2026-11-19), said not to
merge: a form pipeline that failed in a later build (the ground's, asked for after the form set
was built) was counted as nothing lost, so the set stayed "built" and the next form frame
compiled the missing pipeline itself; with `references` a sheet's form tile was framed without
the reference figure's room while its box was the colour tile's; and a form capture that waited
for its pipelines held every other capture behind it. Mended: a failed form pipeline loses the
set whether or not the set was new to that build, and a retry makes what is missing; a frame only
ever **looks up** a form pipeline, so no bookkeeping can make it compile one; a form tile is
framed as its colour tile is; a waiting form capture stands behind the others. Each is a test
that was shown to fail on the fault, and the sheets have a live check of their own
(`form_sheet_check`). It found sound: the shaders' bytes, the variant set on both adapters, the
orthographic rule, the waits' bounds.

**A fourth review, of those** (another model, 2026-11-19), **said merge**, with four small
findings, all mended: a sheet's silhouette tile was still framed without the references' room
(as its form tiles had been); one line of the live check could not fail (every tile of a view
is given the one box, so only its test of the pixels holds the framing: it says so now, and
holds the silhouette tile too); the pass ends at the second form view put back in it, which the
comment had said otherwise; and a failed form set gave no way back, so the capture that meets
one now asks for it again and says so (`recovery`). It traced the failure's bookkeeping in every
order, found no way for a frame to compile a form pipeline, and both seeded faults failing
their tests.

What the first looks through them taught (the figure, 2026-11-18; an independent check of each):
curvature cut off at white lost limbs and fingers alike, so the turn is eased; clay under an even
key showed form faintly, so its key rakes; stripes on a finger were a shimmer, so they fade. Of the
three, curvature finds the most and is the noisiest, clay is the one to trust first, and stripes
are for broad surfaces.

**How not to misread them** (from the check of the second look, which did not pass every claim made
for it):

- **Curvature shows the mesh's normals, triangle by triangle.** Blocks in steps on a flat trunk and
  rings on a head are how the mesh's own normals turn at its resolution: clay shows the same
  surface smooth, and nobody would see them on it. Read curvature for lines and bands that follow
  the form, not for its grain.
- **Easing costs contrast in the middle.** Rings at the knees that were bright under the first,
  cut-off curve are pale bands under the eased one, and a finger seen end-on is still nearly white.
  `form_scale` is the handle: larger for the broad forms, smaller for the fine ones; one scale does
  not show both at full strength.
- **Mid grey in stripes is not a band.** It means "too fine to band here", and its edge follows the
  stripes' width on screen, not the surface. Stripes say nothing of hands, feet or wrists.
- **Clay's raking key brings out small ripples** (a diamond pattern on an upper arm) that normals,
  read by eye, show as smooth: both are right, and the ripple is slight.

**On the studio's sheets** (2026-11-19): `studio_sheet` and `studio_compare` take the four views as
`passes` beside `colour`, `value` and `silhouette` (`"passes": ["colour", "clay", "curvature"]`):
a row a pass, the same camera as the colour tile (with `references` the room the reference figure
takes is kept, so a form tile's box is its colour tile's), the subject alone on the neutral ground
(no floor, no reference figure). Each form pass is a render of its own for every view; a standard
shot is shown at the shot's own size and level. Not measured: the taste checks read the colour and
silhouette passes as before.

**In a before and after** (2026-11-19): `sight_compare`'s `mark` takes `form` (and `form_scale`),
and its `diff` is drawn in the mark's view, never one of its own. So a mark taken in clay answers
one question, "did the shape change?": a cube that only turned red leaves the clay diff with no
changed pixel and the world's diff with thousands; a cube that changed size changes both. The
mark's answer, the diff's packet and the list of marks say which view a mark is in. (Not
reviewed.)

**In the line-up** (2026-11-19): `studio_lineup` takes `form`. The siblings stand as shape alone
on the neutral ground with no floor, **with the 1.8 m figure and the 1 m cube beside them**: a
line-up is for scale, so its references stay (a sheet's form tile leaves them out). No `rig`
goes with it, and the packet says the view and no rig. The distinctness matrix is measured as
before, from silhouettes. `form_sheet_check` holds it live. (Not reviewed.)

**Not yet:** a filmstrip does not take `form`. A view of the surface's thickness, and the
silhouette alone, are not built (the studio's silhouette frames are the nearest).

## The form checks

What a trained eye sees in a mesh's shape, as numbers (`specula-studio`, `form.rs`). They are read
from the finest level of the built mesh alone (positions, normals, triangles: no picture, no GPU), so
they run wherever the studio's other rules do, CI included, and they are in **every asset's facts**
(`form`, in `studio_check`, `style_check` and the sheets' answers) whether or not a style limits them.

| Measure | What it is | A limit in `[form]` |
|---|---|---|
| `loose_pieces` | pieces of the mesh (triangles joined through shared corners) whose bounds touch no other piece's and do not reach the ground, each with where it is, how large and how far from anything | `max_loose_pieces` (0 unless given: checked in any style) |
| `razor_share` | of the creases (edges whose faces turn more than 30°), the share by length that turn more than 75° with nothing to soften them. A bevel splits a right angle into two of 45°, which pass | `max_razor_share` |
| `seam_m` | length of the edges that are not creases across which the two sides are shaded more than 15° apart: a line in the light where the surface has none | `max_seam_share` (of the shape's size) |
| `smeared_crease_m` | length of the creases shaded as if they were round (one normal for both faces): the dark smear along a hard edge | `max_smeared_share` (of the creases) |
| `relief` | how the shape's relief is shared between `big`, `medium` and `small` form: how much the surface's facing varies within a box a quarter, a sixteenth and a sixty-fourth of its size, sheet by sheet | `max_small_relief` |

Also in the facts, with no check of their own: `pieces`, `pieces_clear_of_ground`, `open_edge_m` (the
rim of an open sheet: grass cards, cloth), `tangled_edge_m` (edges more than two triangles share),
`crease_m`, `razor_m`, `edge_m`. A mesh in which a vertex in use has no normal (or one of no
length) has `shading: "not measured"` in place of the two measures of shading, and **a style that
limits them is failed by it, saying so**: a share of nothing is never a pass. `unread_triangles`
counts the triangles that could not be read (an index past the vertices, a corner whose place is
not a finite number): they are no part of what is measured, and a mesh that has any fails the
loose check, because it was not all looked at.

- **Only loose pieces are checked unless the style says more.** A piece that hangs in the air and
  touches nothing is a fault in any style; how sharp and how busy a shape may be is the
  style's to say (Sepulcra's assets have a median razor share of 0.18: its look, not a fault). `[form.allow]` exempts a subject from a check, with the reason in a comment, like
  `[taste.allow]`.
- **Loose means beyond doubt.** Two pieces touch when their bounds come within a hundredth of the
  shape's size of each other (and never more than 5 cm, however large the shape), so a ring round a
  post it never touches counts as touching; a piece whose bounds reach the ground (y = 0, give or
  take that much) rests on it (a coffin's lid lying beside it, rubble). What is left hangs in the
  air, or lies wholly under the ground. A shape of one piece has no other to be apart from:
  whether the whole of it may hang clear of the ground is the `float` check's to say, which knows
  the band its category allows.
- **The shape is its triangles.** Its size is taken from the corners of the triangles that are
  measured: a vertex no triangle uses (an importer keeps every vertex of a file) and a corner at
  infinity, or past a million kilometres, are no part of it.
- **Slivers are left out.** A triangle half a million times longer than it is broad (an area under
  a millionth of the square of its longest side) has a facing that
  is noise, and the edges it would make razors of are edges nobody sees. By its own sides, not the
  shape's size: one far triangle made slivers of every other.
- **Relief is by boxes, sheet by sheet.** A triangle is in every box a point of it is in; what is
  in a box is taken sheet by sheet (triangles joined through edges in it), so the two sides of a
  thin plate are not taken for one surface that turns right round. A ball's relief is all big; bumps
  a thirtieth of its size across put most of it in the small; a ribbed sheet nearly all. **It is a
  coarse measure:** the boxes lie along the axes and are sized by the bounds, so the same shape
  turned moves a share by up to a quarter (a ball on a block turned 37° about a slanted axis: 0.88
  big upright, 0.73 turned; a reviewer found 0.26 for two boxes turned 15° and 0.25 for a plank
  turned 45°). Set `max_small_relief` with that in mind.
- **Bounds:** a mesh of more than 600,000 triangles is not measured (the facts say `skipped`), and
  only pieces clear of the ground are compared with the others: over 50 million comparisons the
  pieces are counted and the facts say `not compared`. In both cases **the loose check fails
  saying so**: it never passes on what it did not look at. The relief has a budget of 200,000
  points for each of its three sizes, and every triangle keeps one, so a mesh of 600,000 triangles
  takes 600,000. A 600,000-triangle ball takes 1.4 to 1.6 s (release, this laptop); the studio
  hands the measure its call's deadline, and a measure given up says `skipped` (the call answers
  `timed_out`).

**What the first run found** (2026-11-18, on assets that had passed every earlier check):

- Sepulcra: three tier 3 towers wear a halo that floats (deliberate: exempted with the reason); and
  **one of the fallen pediment's rubble blocks hangs about 5 cm over the ground**, in a prop the
  owner approved at the window in M4. It is written into `style.toml` as a fault and left as
  approved until the owner says to mend it.
- The fidelity benchmark's skeleton warrior: at T0 six of its eight blocks touch nothing (a
  blockout of bones), and at T3 one hand bone is 4 cm clear of the forearm (a fault of the recipe).
  Both written down in that set's `style.toml`.

**The review of the form checks** (a model that did not write them, 2026-11-19) said not to merge:
two major findings and five minor, all mended, each case now a test
(`the_review_of_the_form_checks`).
- *Major:* the shape's size was taken from every vertex, used or not: one stray vertex a kilometre
  off made a floating block "near" the rest, and one at infinity dropped every triangle, so the loose
  check passed on nothing. And `facet_deg` was taken over every edge that is not a crease: a coarse
  dome (18°) on a flat slab read 0.
- *Minor:* more than 2,000 pieces passed the loose check unlooked at; the measure could not be
  stopped and cost a small asset 175 ms (the relief's sampling: the studio's tests took four times
  as long; they take what they did before now); a piece wholly under the ground was never loose; a
  mesh with no normals had every crease called smeared; lengths summed in `f32` came out 0.6% off
  over 600,000 edges.
- It found sound: the measures under a turn and a move, a ball's facets by its bands, equal facts
  bit for bit over two runs, pathological meshes, the three halo exemptions (each hides the halo and
  nothing else), and `pediment_fallen`'s fault (a 0.19 m block, its bottom 5.5 cm off the ground).

**A second review, of those mends** (another model, 2026-11-19), said not to merge: `facet_deg`
left out every edge that turns more than 30° as a crease, so a smooth cylinder of ten sides read 0
and passed a limit that one of 64 sides failed, and one folded diagonal in a flat sheet set it;
one far corner of a real triangle still voided the loose check (the sliver's floor and "near"
both went by the shape's size); a shape of one floating piece was never loose; a mesh too large
to measure passed the loose check; the relief's turn was understated (0.15, where 0.26 was
found); and the sample budget was called a cap. Each is mended as said above and is a test
(`the_second_review_of_the_form_checks`). It found sound: the relief's sampling (inside each
triangle, shares summing to 1, three times faster, bit for bit over two runs), the comparison's
boundary, stopping, and the first review's other findings closed.

**A third review** (another model, 2026-11-19) said not to merge those. Faceting, mended to
count the edges shaded as one surface, still read 0 on the engine's own coarse cylinders, because
the modeller shades an edge that turns more than a recipe's `smooth_angle` as two faces: a
ten-sided cylinder was hard-shaded and "had no facets", a smooth-shaded box "was faceted by 90°",
and finer tiers of an asset read more faceted than coarse ones. A normal of no length counted as
shading; and one piece hovering on purpose, counted as loose since the second review, failed
whatever float band its category allowed. Mended: **faceting is taken out** (below); normals of
no length are no shading; one piece alone is the `float` check's again.

**A fourth review, of those** (another model, 2026-11-19), said not to merge them. With faceting
gone it found nothing of it left that parses or is reported, and the earlier findings closed. It
found: **one stray vertex with a normal of no length, used by no triangle, switched the shading
measures off**, and the two checks that limit them then passed with "0% of its creases" on a
mesh whose every crease was smeared (shading is now judged on the vertices in use, and
unmeasured shading fails those checks, saying so); a triangle written from another of its
corners was sampled elsewhere, and a shape's small relief moved by a tenth (two sides as short
as each other are now told apart by where they are, not by their turn in the triangle); twenty
million vertices that no triangle used were welded before the first look at whether to stop,
three seconds of them (only vertices in use are looked at now, and the stop is asked every
65,536 triangles); a mesh whose indices are all out of range, or a floating box whose places are
not numbers, passed the loose check on nothing (`unread_triangles`, above); and two lines of this
page said a sliver is a thousand to one where the code's is half a million to one. All mended,
each a case of a test (`the_fourth_review_of_the_form_checks`, and the check's own).

**A fifth review, of those** (another model, 2026-11-19), **said merge**, with small findings,
mended: a normal whose length is not a finite number still counted as shading, and its angles,
which are no numbers, read as neither seam nor smear (it is no normal now); **a mesh whose
faces are all written twice** (back to back, as some tools write a double-sided sheet) has
every edge shared by four triangles, no crease between two faces, and passed "0% of its creases
are razors" (when more of the edge length is shared by more than two triangles than by two, the
three checks of the creases now fail saying they could not be measured); a mesh of triangles
with no area passed the loose check with "0 of its 0 pieces" (it fails on nothing to measure);
and the line of this page that still named the old faceting fact. It found the earlier findings
closed by their own reproductions, the facts exactly the same under every reordering of
triangles, corners and vertices (the relief to a billionth), 600,000 separate triangles
measured in 1.8 s with a stop honoured in a twentieth of a second, and unread triangles
failing the loose check in the default style.

**Faceting cannot be read from a mesh, after three tries.** How coarsely a round surface is
faceted cannot be read from a mesh alone: a box and a four-sided cylinder are the same mesh, a
hard-shaded prism is a coarse cylinder or a prism by what its maker meant, and the modeller's
own shading follows the coarseness it would be measuring. Each definition that satisfied one
reviewer's cases failed another's: over every edge that is not a crease (a dome on a slab read
smooth), over curved edges that are not creases (a ten-sided cylinder read smooth), over edges
shaded as one surface (the modeller's coarse cylinders read smooth, and a smooth-shaded box
faceted). It was taken out of the mesh's measures, and built where it belongs (below).

### Faceting, from what the recipe names round

**A recipe says what is meant to be round, by the name it gives a shape** (2026-11-19). A
`cylinder` is round and an extruded outline of ten sides is a prism, whatever their triangles;
so the modeller records, for every shape a recipe names round, how coarsely it was built
(`specula_model::Round`, in the built model's extras, the 64 that sag the most):

| The recipe's shape | The turn between neighbouring faces | At the radius |
|---|---|---|
| `cylinder`, `cone` | 360° over `sides` (12 unless given: 30°) | its larger radius |
| `sphere` | 41.8° halved for each of its `subdivisions` (one unless given: 20.9°) | its radius |
| `torus` | twice: 360° over `segments` at the ring's outer radius, and over `sides` at the tube's | each its own |
| `capsule` | the greater of 360° over `segments` and 90° over `rings` | its radius |
| `lathe` | 360° over `segments` (16 unless given: 22.5°) | its profile's widest |
| `sdf` (a sculpt) | what a cell of its grid turns the surface by round its tightest ball, limb, tube or section: twice the arc sine of the cell over twice that radius | that radius |

Not counted: boxes, extruded and swept outlines (their corners are their maker's), a rounded
box's or cylinder's own roundness, and the coarse far level of a model, which halves every count
on purpose. The radius is the recipe's own, before the part is scaled.

- **In every asset's facts** (`faceting`, beside `form`): `turn_deg`, the coarsest turn among the
  round shapes **whose facets are seen at the asset's size**; that shape (`worst`: where in the
  recipe, what it is, its turn, its radius, and `sag_m`, how far the middle of a facet stands in
  from the true curve); the eight that sag the most; and how many round shapes the recipe has.
  A facet is seen when it stands in by a 512th of the asset's size or more (a pixel of the
  studio's silhouettes): the six sides of a candle on a table of candles are not counted, and
  the six sides of the table's leg are.
- **The check `faceting`**, when a style gives `[form] max_facet_deg` (degrees, 0 to 180; none
  unless given): fails an asset whose `turn_deg` is over it, naming the shape and where it is,
  with how many sides would hold the limit in its fix, and that a shape meant to have flat sides
  is an extruded outline. `[form.allow]` exempts as for the other form checks. An asset with no
  round shape, or none whose facets show, passes saying so; an imported mesh has no recipe and
  nothing to say. It is checked whether or not the mesh's own form was measured.
- **What it says of Sepulcra** (2026-11-19, its 56 recipes through `studio_check`): 41 name
  something round, and in 36 of them a round shape whose facets show turns more than 20° a face;
  the median is 90°. Rings of three sides (`torus`, `sides: 3`, on six props: 120°), cones of four
  sides, capsules of one ring, and two statues whose sculpts' cells are larger than their thinnest limb
  (`statue_angel`: 116° at a radius of 5 cm). That is its look, low and faceted, approved at the
  window in M4: Sepulcra's `style.toml` gives no limit and nothing fails. A style that wants
  round things round sets one (20° is a cylinder of 18 sides).
- **Limits.** The radius is not scaled by the part's own transform, so a shape scaled up in
  place is judged at its written size. An arch outline's `segments` and a rounded outline's are
  not counted. A sculpt's figure is its tightest radius against its cells, so one thin limb
  speaks for the whole sculpt. **Not reviewed yet.**

**Not built:** pieces that pass through each other by
accident; a check for the figure's seams between its separately meshed parts (they share no edge, so
`seam_m` does not see them: the form views do).

## Shape language

How round, square or pointed an asset's outline is, as numbers (`specula-studio`, `shape`). Round
reads as friendly, square as sturdy, pointed as dangerous: a convention of character and prop
design, not a law. So it is **reported for every asset** (`studio_check`: `measures.thumbnails.shape`)
and **checked only when a style asks** for one.

- **The measure** (its fourth version, 2026-11-19: **each part at its own scale, weighed by
  its thickness**). On three silhouettes of the asset (the taste checks' angles: front, side,
  three-quarter), each **512 pixels** a side, the outline of the largest piece:
  1. **How thick the piece is under each point of the outline**: the radius of the largest
     disc inside the silhouette that touches the outline there (from an exact distance
     transform and the discs no neighbour's disc holds). In a corner or along a taper that
     falls to nothing at the tip, so the outline is followed from the point, both ways, to the
     body it is a corner of; and not across a **joint**, where the thickness leaps as no
     taper's does (a string meets its balloon, a stick its snowman, a spike's foot its ball).
     That is the point's **part**, and its thickness is the scale the point is judged at.
  2. **The turning**: the outline's long straight runs are straightened to within a pixel and
     a half, its curves smoothed, and the net turn is taken within a window **six tenths of
     the part's thickness** long. A box's window is three tenths of its shorter side; a
     plank's, three tenths of its width; a hair's, nothing to speak of.
  3. **A corner or a curve.** A corner is turning in one place (20° to 60° within a window a
     third as long; and for certain where a straightened side meets another straight run at
     30° to 40° or more: the two ends of a corner cut off flat). A curve is turning spread
     out, and is round however far it turns in all.
  4. **A corner's angle** is its window's net turn less the window's lopsidedness: what the
     two sides of the window turn counts only as far as both do. A box's corner is 90°; two
     kinks in one window (a cut corner) are one corner of their sum; the mouth of a slot in a
     disc is a right angle, the rim's own turn being on one side only. (The window has soft
     ends, so a growing cut goes from one corner to two by degrees.)
  5. **Its class**: gentle under 45° (part of a curve, fading out by 80°: an octagon's
     corners), a point from 96° (wholly by 116°), square between. **A corner with no straight
     side near it is never square**: the end of an oval or the horn of a crescent is part of a
     curve up to 70° and a point from 110°.
  6. **Its weight.** Only outward turning counts (a notch, a slot, the inside corner of an L,
     the cusp between two bumps of a cloud count for nothing). A step's turn is the turn
     within its own window, and the window is as long as the part is thick: **a part's
     turning counts by its thickness.** The end of a hair turns right round and counts for a
     hair. A point counts double.

  The shares (`round`, `square`, `pointed`) sum to 1; `leading` names the largest; `convexity`
  is the outline's area over its hull's; `corners` counts its corners of 60° or more (an L has
  five). The three silhouettes' means are the asset's.
- **Known shapes, turned through a quarter turn** (a test of 42 shapes; each share steady to a
  tenth, every ten degrees). **Round:** a disc, an egg, an oval of 3 to 1, an octagon, capsules
  to a sixth as thick as long, a box rounded by a quarter of its shorter side or more, a cloud
  of five bumps, a disc with a slot or a notch cut in it, and **a balloon on a string, a
  cherry with its stem, a lollipop, a bomb with its fuse, a snowman with stick arms and a disc
  with four whiskers**. **Square:** a box, an L, a cross, **bars from a fifth to a fourteenth
  as thick as long, a table on thin legs, a comb**, a box rounded by a tenth of its side or
  less, a pentagon and a house. **Pointed:** a triangle, stars of five and of twelve points,
  spikes of 8, 15 and 25 degrees (65% to 73%), a thin diamond, a crescent, **a mace** and an
  oval eight times as long as wide. A hexagon, whose corners turn 60°, is between round and
  square and stays there (61% round).
- **Three families go from one class to the next by degrees** (three tests, every three
  degrees, the share moving by a fifth at most as the shape is turned):
  - **a corner's rounding**: square to a tenth of the box's shorter side, round from a
    quarter, a share of each between, the more rounded never the less round, and at no
    rounding anything of it a point;
  - **a corner cut off flat**: square while the cut is under a seventh of the side (98% to
    79% square for a 0.8 by 0.6 box cut by 0.05 to 0.09 of its size: the third review's failing
    case), eight-sided and round from a quarter, never squarer for a larger cut;
  - **an oval**: round to three times as long as wide, pointed from eight, **and at no length
    more than a fifth square**.
- **What it cannot say steadily**, measured:
  - In the bands above (a rounding of a tenth to a fifth of the side, a cut of a seventh to a
    fifth) the share moves by up to a fifth with the angle.
  - **An oval four to six times as long as wide is on the line between round and pointed**,
    and its lead goes by the angle it is turned to (its end is a dozen pixels round at 512).
  - **A body bristling with more than about a dozen hair-thin parts** (a disc with sixteen
    whiskers of one pixel) is read by its hairs or not at all: the inside corners at each
    hair's foot take the rim's turning out of every window that reaches them. Two to eight
    hairs change nothing. (Two tries at stopping windows at such feet made a slotted disc
    pointed and are not in it.)
  - A corner on a curve reads a few degrees over its own angle; a part under four pixels
    thick is not read; holes are not traced. It reads outlines, not surfaces: a ball on a
    square plinth seen from the front is both.
- **It needs pixels.** A piece under 110 pixels across is not read at all (a thumbnail never
  is), so the studio renders the three silhouettes again at 512 for it. A shape in one of the
  bands above swings further the smaller it is drawn (a box rounded by a sixth of its side:
  74% to 81% round at 512 pixels, 35% to 78% at 256, and anything from square to round at 150);
  out of the bands a square, a disc, a triangle and a star read the same from 512 down to 140. A silhouette of 512
  takes 10 to 16 ms to read (release, measured on this laptop: the distance transform 4 ms of
  it), on the caller's own thread, and is read anew at each call.
- **The rule.** `style.toml`: `[taste] shape_language = "round"` (or `"square"`, `"pointed"`),
  `shape_min_share` (0.5 unless given) and `shape_by` (`"outline"` unless given, or `"recipe"`:
  below, "From what the recipe is built of"): the check `shape_language` fails an asset whose
  share of that kind is under it, with both readings in its message and what to change in its
  fix; `[taste.allow]` exempts as for the other taste checks.
- **In the presets.** A base style may carry the ask (`Preset::shape`), and the style configurator
  writes it with the rest. **No shipped preset asks for one now**: `storybook` asked for round
  until the measure's third review (a ball on a string would have failed it), and a moonlit
  graveyard, a muted or a noir game and the neutral stage are not one shape.
- **What it says of Sepulcra** (2026-11-19, its 56 recipes through `studio_check`, by the
  fourth version): **29 lead pointed, 14 round and 13 square** (`grave_rock` wholly round,
  the wraith 0.80 pointed, the angel 0.63 pointed, the crypt 0.59 square, a tombstone 0.41
  pointed with the rest shared, the fallen pediment a third of each). (By the third version: 30 pointed, 20 square
  and 6 round; by the measure before its second review: 31 square, 18 pointed and 7 round.) A
  graveyard of crosses, spires and iron: no check is asked of it.
- **Its first review** (a model that did not write it, 2026-11-19) **said not to merge it.** The
  measure then counted the corners of a straightened polygon with hard lines between the classes:
  a box rounded by an eighth of its size was round upright and square turned six degrees; a
  capsule was pointed (its round end merged into one corner of 180°); a bar was square or pointed
  by the angle; a hexagon, whose corners sit on the line, flipped; and a failed check never
  reached the critique, because no pass of the critique owned it (nor the form checks: they have
  a pass of their own now, and a test holds every check to a pass). The measure was written
  again as above, and its test turns every shape through a quarter turn. One finding is left as
  it was: the style configurator cannot add a key to an existing `style.toml` that writes its
  tables as dotted keys at the root (`taste.thumb_px = 64`); it refuses with a structured error
  and leaves the file untouched, as it does for every key of a preset. **That rewrite's review is the next.**
- **Its second review** (another model, 2026-11-19) **said not to merge the rewrite**, with three
  major findings, each reproduced here with the reviewer's own probe before it was mended. **A
  box rounded by a sixteenth of its size read 21% round at one angle and 89% at another, a degree
  apart**: the shape's size was the longer side of the box round it, which grows by two fifths as
  a square is turned onto its corner, and every length of the measure is a share of the size (the
  same corner was a curve at one angle and a corner at another); the size is the greatest width
  now, the bands between the classes are wider (45° to 80°, where 50° to 70° was a cliff), a
  curve is no longer made a polygon before it is smoothed, and it is measured on 512 pixels,
  where what is left of the swing is half what it is on 256. **A spike read under half pointed,
  the sharper the less**: its tip turns under 180° and its base's two corners 90° each, so by
  turning alone no thin spike can lead pointed, and the check's own advice ("spikes") failed the
  check; a point counts double now. **A disc with one slot cut in it failed "round"**: the
  slot's four right angles outweighed the disc's whole turn; only outward turning counts now.
  Minor: the module's own text gave other numbers than its constants; a thumbnail of 64 pixels
  read a square as round at one angle in five where this page said it still told a disc from a
  box (under 110 pixels nothing is read now, and the taste measures no longer fall back on the
  thumbnails); two ranges on this page were narrower than what a finer sweep found. It found
  sound: every edge case (no panic, no share out of range), the cost (nothing quadratic), the
  wiring into the check, the critique and the studio's renders.
- **Its third review** (another model, 2026-11-19) **said not to merge those mends.** The second
  review's findings were gone by their own probes at 512 pixels (a box rounded by a sixteenth
  now moves by an eighth, a spike is 63% to 72% pointed, a slotted disc is round), and twenty
  kinds of shape held their lead at every degree of turn. And the mends had brought two faults
  of their own. **A corner cut off flat by about a fifteenth of the size or more read as
  round** (a chamfer is two turns of 45°, each gentle by the rule: a 0.8 by 0.6 box cut by 0.07
  of its size was 100% round at every angle), and the check's own advice for a square style
  led there ("bevels under a tenth"). **One thin part on a round body read as pointed** (its
  end turns right round, and a point counted double: a balloon on a string 52% pointed, a
  snowman with stick arms 71%, a disc with four whiskers 84%). Smaller: the greatest width tried one hull point in four
  and came out up to a fifth short on a sliver (every pair is tried now); a reading kept inside
  the measures went stale in a copy (it is read anew at each call); the time to read a
  silhouette was understated (4 to 13 ms each on a busy machine, and up to a second for a
  spiral or a comb of one-pixel teeth, on the caller's own thread).
- **Its fourth version** (2026-11-19; the owner, shown the three choices: "2 - 3 and I don't
  care if it takes longer": a fourth silhouette measure that weighs parts by their size, **and**
  the measure from what a recipe is built of). The three reviews shared one finding under
  their different ones: the measure weighed **how the outline turns**, with one window for the
  whole shape, and a person weighs **how much of the thing** is round, flat-sided or sharp. The
  fourth version keeps the turning and gives it what it lacked: every point of the outline
  belongs to a part with a thickness, is judged at that thickness and counts by it. It was
  tuned against one table of fifty-six shapes printed at every angle (the `table` test, which writes it to a file and is
  ignored), and then run through the third review's own probes before it
  was shown to anyone: sixty kinds of shape at every degree of a quarter turn, mirrored and
  shifted by part of a pixel, hold their lead in every mask at 512 pixels (boxes, bars to a
  fourteenth, an L, a cross, letters, crescents, stars, a gear, polygons, triangles of 30 to
  70 degrees, houses), but for a thick C with square-cut ends, which is a tenth pointed-led
  (its cut corners are of 77 degrees, on the line). (Three of that probe's shapes are cut by
  the picture's edge as they turn, and say nothing.) **What changed for the three reviews' cases**, by their probes:
  - the third review's cut boxes (0.05, 0.07, 0.09 of the size) pass a square style at 96% to
    99% square, and lead square in all 120 turned masks each (they read 0% square);
  - its balloon, cherry, lollipops, bomb and snowman pass a round style at 92% to 100% round,
    and lead round in all 72 masks each (they led pointed); two to sixteen whiskers of one
    pixel leave a disc wholly round;
  - bars to a fourteenth as thick as long, a table's legs and a comb's teeth are square (they
    read pointed, a documented limit of the third version);
  - **a convention moved**: a corner is judged at the scale of its part, so a box rounded by
    a tenth of its shorter side is square (it was round from a sixteenth of the whole size),
    and round only from about a quarter. A crate with softened edges is a crate.
  The check stays as it was made safe: for a style that asks by name, and no preset asks.
- **Its fourth review** (another model, 2026-11-19, of `6e9c490b`) **said not to merge the
  fourth version**: five major findings, each from a run at 512 pixels, and none mended yet.
  **What is written above this point is the author's account, and these are what it gets
  wrong:**
  1. **A plain box whose edges wobble by half a pixel reads 0% square at some angles** (at 36
     degrees: round 34%, pointed 66%; at 0, 9, 18, 45 and 72 it is 100% square). The
     straightening cuts its slanted sides into runs too short to be "sides", so its right
     angles take the rule for corners between curves, half curve and half point. What is a
     side must come from how the smoothed outline turns, and a missing side must never make
     a right angle a point.
  2. **The joint rule flickers on limbs and necks**, and where it misses, the limb is judged
     at the body's scale. A character (a ball head, a box body, capsule limbs) is 22% to 67%
     round as it turns and leads pointed in 102 masks of 360; a bottle with a curved shoulder
     leads round in 282, square in 76; an axe is 25% to 65% square. "Sixty kinds of shape
     hold their lead" does not cover a humanoid, a bottle or a tool. A part should be taken
     from the branches of the spine, not from the thickness along the outline.
  3. **The bands' stated swing of a fifth is false for a plank**: a bar of 0.8 by 0.2 with
     its corners rounded by 0.03 to 0.04 moves by a half as it turns, and its lead flips.
     Only the 0.8 by 0.6 box was tested.
  4. **The advice for a round style leads into a failure**: "a quarter of that part's
     thickness" (the thickness being the largest disc's radius) is an eighth of its width,
     where the measure asks for about a quarter of the width. A box rounded so fails "round"
     in every mask. (The third review's first finding, again.)
  5. **Thin parts on a thick body read as points**: a comb whose teeth are 6 to 16 pixels
     wide and a quarter of the picture long is 50% to 91% pointed. "A comb's teeth are
     square" holds for the test's short teeth of 20 pixels and not for these.
  Smaller: a short stalk (a tenth of its body's radius long) takes up to a quarter of a disc
  as "pointed", so "a thin stalk changes nothing" holds only for long ones; a sword and a
  dagger lead square, a mushroom and a dome on a flat base square, a ghost with a wavy hem
  pointed, and a tombstone and a heater shield sit at exactly half and half; two floors the
  tests assert hold only at the angles they sample; one step of the measure (the median of
  five) is held by no test; the worst case costs eight times a disc (a comb of one-pixel
  teeth: 137 ms), which this page did not say. It found sound: the distance transform
  (against a search on 3,000 masks), every edge case, the index arithmetic, the determinism,
  that nine single breaks of the code each fail a test, and about thirty game shapes that
  read as a person would. **Four reviews, four "do not merge".** The mends are the next
  work, by an agent that did not write the fourth version, and reviewed again.
- **From what the recipe is built of: the second measure, built** (2026-11-19;
  `specula-model` `made`, `specula-studio` `built`). The modeller records, for every shape a
  recipe names, **that shape's own outlines along its own axes and the surface it has in the
  model**: a box's three rectangles, a ball's circle, a cylinder's section and its side (a
  rectangle; a cone's, a triangle), an extruded outline and the box it is raised to, a lathe's
  section and its profile mirrored, a sweep's profile and its path laid out straight (a horn
  that narrows to nothing comes to a point), each primitive of a sculpt. What a boolean or a
  sculpt takes away is not recorded; a part's operations change the surface (three copies
  count three times; a box with a hole cut through it has the hole's wall too) and not the
  outlines. The studio reads each outline with the measure above (drawn on 384 pixels), weighs
  a shape's outlines as the recipe weighed them and **the shapes by their surface**, and
  answers `built_of` in `studio_check` (`measures.thumbnails.built_of`): the three shares,
  the share of the surface that was read, and the eight shapes with the most surface, each
  with its path in the recipe, what it was made as, its share and what it reads as.
  - **What it says that a silhouette cannot**: which part is what, so a failed check names
    what to change; and it is not moved by the angle the asset is seen from.
  - **What it cannot say**: anything of an imported mesh or a generated part (terrain, a
    scatter, a building: `read_share` says how much was read); what a part's operations did
    to its form (a taper does not make a box's outline a wedge's); how the parts are put
    together. **A straight-sided cylinder is a third round and two thirds square** by it, as
    it is in silhouette from the side: a can is not a ball.
  - **The two readings differ, and both are true.** Sepulcra's 56 recipes: by silhouette 29
    lead pointed, 14 round and 13 square; **by the recipe 42 lead square, 11 round and 1
    pointed** (two are generated and unread), and the two agree on 20. The props are built
    of boxes and few-sided cylinders, which is what the recipe says; they are topped with
    crosses, finials and gables, which make points in an outline and little surface, which
    is what the silhouette says. A chest tomb is 69% square by what it is built of and 86%
    pointed in outline; a domed mausoleum 62% square and 85% round.
  - **So the silhouettes answer the check, unless the style says otherwise**: shape
    language is a convention about what the eye sees. `[taste] shape_by = "recipe"` makes the
    recipe answer where one speaks for half of the surface or more. Either way the other
    reading is said beside the answer, and **the fix names the recipe's largest shapes that
    are not of the kind asked for**: `parts[0].shape (a box, 60% of the surface, square)`.
  - Tests: the record (surface after copies and scaling, a boolean's cutter left out, a
    sculpt's primitives, the same recipe saying the same); the outlines read as what they
    are (a box square, a ball and a capsule round, a can a third and two thirds, a cone
    pointed); the weighing and the naming; the check's two readings. Four seeded faults
    were each caught by one of them (a cutter recorded; surfaces left as generated; shapes
    not weighed by surface; the recipe answering unasked). **Not reviewed.**
