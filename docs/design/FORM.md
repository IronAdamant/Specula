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
  speaks for the whole sculpt.

#### Its first review

**A model that did not write it said not to merge it** (2026-11-20, of the code as it stood
at `f173b170`). What each kind of shape records was found right: the recorded turn matched
the largest turn measured on the built mesh to about a degree, for the cylinder, the cone,
the sphere, the torus at both its radii, the capsule, the lathe and a sculpt at three
resolutions. What is done with the record was not. Each finding was shown by a recipe the
reviewer built; **none is mended yet, so the check is not to be given a limit until it is.**

1. **The radius is the recipe's, and the asset's size is the built model's.** A part's
   `scale`, an include's scale, the finish and a taper change how far a facet stands in and
   leave the record as written. With a limit of 20°: a six-sided cylinder of radius 1 m
   fails; a six-sided cylinder of radius 0.1 m scaled by ten, which is the same mesh,
   passes; and a twelve-sided cylinder of radius 100 m scaled by a thousandth (a column a
   tenth of a metre across, its facets 3.4 mm in and not seen) fails, with a message that
   its facets stand 3.4 m in. The limit written above ("the radius is not scaled") said
   this as a limit; a recipe that scales its parts is ordinary, so it is a fault.
2. **A sculpt speaks by its tightest radius alone**, and the tightest radius is the largest
   turn but not the facet that shows. A coarse ball of radius 1 m fails a limit of 10°; the
   same ball with a bead of 2 mm joined to it, or with a ball of 2 mm cut from it three
   metres away where it cuts nothing, passes with nothing said to show.
3. **The 64 records kept are those that stand in the most; the answer is the largest turn.**
   With 64 twelve-sided columns and one three-sided spike the spike is dropped, and the
   check answers 30° where the truth is 120°. The facts give the count kept as if it were
   the count there is.
4. **The advice for a capsule names the wrong number.** A capsule of 256 segments and one
   ring fails at 90° and is told to have at least twelve round it; the mend is `rings`,
   which the advice never names.
5. **A shape nobody sees fails the check.** A three-sided cylinder wholly inside a box
   fails at 120°, and so does a three-sided cutter nine metres from the box it is cut
   from. (A cutter that does cut leaves faceted walls, and those are rightly counted.)
6. **The verdict moves as the asset is turned.** The asset's size is the longest side of
   its box along the axes: a ten-metre bar beside a twelve-sided column passes straight and
   fails turned by 45°, where the box is 7.4 m.
7. **Rules that no test holds**, by reading the tests (the reviewer was refused the
   permission to seed faults, so none of these was shown by a failing run): that the answer
   is picked by turn; the cap of 64; every branch of a sculpt's tightest radius but a plain
   ball's; the greater of a capsule's two terms; a cone's radius.

Smaller: an included recipe's round shapes keep their own paths, so two records can carry
one path and the message can name the wrong part; the sphere's advice says one more
subdivision where two or three are needed; a cone's "turn between neighbouring faces" is
its base's.

Found sound beside the recording: a part switched off records nothing, nor does the coarse
far level; values that are not finite are dropped; the record is the same every time and
costs at most 64 steps a round shape; the limit is bounded and can be allowed by name; an
asset that names nothing round says so. **Not checked:** bends, twists, mirrors and
displacements on the record; scatters; the measured figure's own record; Sepulcra's recipes
again; the levels of detail between the finest and the far one.

**Not built:** pieces that pass through each other by
accident; a check for the figure's seams between its separately meshed parts (they share no edge, so
`seam_m` does not see them: the form views do).

## Shape language

How round, square or pointed an asset's outline is, as numbers (`specula-studio`, `shape`). Round
reads as friendly, square as sturdy, pointed as dangerous: a convention of character and prop
design, not a law. So it is **reported for every asset** (`studio_check`: `measures.thumbnails.shape`)
and **checked only when a style asks** for one.

- **The measure** (its fifth version, 2026-11-20: the fourth's **each part at its own scale,
  weighed by its thickness**, with the parts and the sides found from the shape). On three
  silhouettes of the asset (the taste checks' angles: front, side, three-quarter), each **512
  pixels** a side, the outline of the largest piece:
  1. **The parts.** The outline is smoothed by the shape's scale and its **joints** found:
     where it turns inward by 30° or more in one place (the foot of a stalk, an armpit, the
     gap between two teeth, a bottle's neck). Under each point of the outline the piece is as
     thick as the largest disc inside it that touches it there (an exact distance transform,
     and the discs no neighbour's disc holds). A point's part is as thick as **the most
     thickness about it once that has been level for six times itself** along the outline:
     from a box's corner the thickness rises to the box's and stays, so the corner is the
     box's; from the end of a limb it is level at once, so the end is the limb's, whether or
     not the outline turns inward where the limb meets the body; along a taper it keeps
     rising, to the taper's foot. It is not followed across a joint.
  2. **The turning**: each part smoothed, and its direction taken along a chord, at its own
     thickness (a comb's tooth by the tooth, its back by the back), long straight runs
     straightened to within a pixel and a half first, and the net turn taken within a window
     **six tenths of the part's thickness** long. A box's window is three tenths of its
     shorter side; a plank's, three tenths of its width.
  3. **A corner or a curve.** A corner is turning in one place (15° to 45° within a window a
     third as long: each end of a corner cut off flat is wholly a corner). A curve is turning
     spread out, and is round however far it turns in all.
  4. **A corner's angle** is its window's net turn less the window's lopsidedness: what the
     two sides of the window turn counts only as far as both do. A box's corner is 90°; two
     kinks in one window (a cut corner) are one corner of their sum; the mouth of a slot in a
     disc is a right angle, the rim's own turn being on one side only.
  5. **Its class**: gentle under 45° (part of a curve, fading out by 80°: an octagon's
     corners), a point from 96° (wholly by 116°), square between. **A corner between two
     curves is never square**: the end of an oval or the horn of a crescent is part of a
     curve up to 70° and a point from 110°. "Between two curves" is two pieces of positive
     evidence, both read from the smoothed outline: no straight run near the corner (a
     window that turns under 4° to 8°), and the two sides of its window bending (by 0.4° to
     1° a step, out to twice the window). A corner with neither is read by its angle, so a
     side that was not found never makes a right angle a point.
  6. **Its weight.** Only outward turning counts (a notch, a slot, the inside corner of an L,
     the cusp between two bumps of a cloud count for nothing). A step's turn is the turn
     within its own window, and the window is as long as the part is thick: **a part's
     turning counts by its thickness.** A step takes its class from the corners near it of
     its own part, not across a joint: a stalk's tip is not its rim's corner. A point
     counts double.

  The shares (`round`, `square`, `pointed`) sum to 1; `leading` names the largest; `convexity`
  is the outline's area over its hull's; `corners` counts its corners of 60° or more (an L has
  five). The three silhouettes' means are the asset's.
- **Held at every degree.** `crates/specula-studio/tests/shape_families.rs` turns some 150
  shapes through a quarter turn, every degree, plain, mirrored and shifted by part of a pixel
  (360 masks each), and holds what is said here (every nine degrees in the default run; every
  degree in its ignored test, and `table` writes the numbers). The 42 known shapes of the
  module's own test are turned every ten degrees. What they read, by the least and most at
  every degree:
  - **Round:** a disc, an egg, an oval of 2 or 3 to 1 (76% at the least), an octagon,
    capsules, a box rounded by a third of its shorter side or more (99% to 100%), a cloud, a
    disc with a slot (100%) or a notch (60%), a balloon on a string, a lollipop, a snowman with
    stick arms or thick ones, a disc with four whiskers (90%), **an apple with a short stalk
    and a disc with a whisker of 30 pixels (100%)**, **a figure of a ball head, a box body
    and capsule limbs (69% to 85%), in a T-pose (100%), with a round body (100%), a sheep
    (98%)**, a flask and a vase (80%), a tree of a ball on a trunk, a heart (69%), **a disc
    with ten smooth lobes (98%)**, a star with rounded tips. A hexagon is between round and
    square and stays there (61% round).
  - **Square:** a box, an L, a cross, bars to a fourteenth as thick as long, a table on thin
    legs, a box rounded by a tenth of its shorter side or less or cut off by a seventh of it
    or less (95% to 100% on boxes of 0.6 by 0.6 to 0.8 by 0.2), a pentagon (86%), a house
    (75%), **a box whose edges wobble by half a pixel to two (95% to 100%)**, a plank whose
    edges wobble (59% at the least), **combs whose teeth are 20 pixels or more (79% at the
    least) and 12 pixels (leading)**, a box with a thick limb, a hammer, a barrel (82%), a gear
    with square teeth, **bottles (with a square shoulder 91%, a curved one 64%, a long neck
    50%)**, a heater shield (57%), a fence (46%), a mushroom (47%) and a dome on a flat base
    (44%).
  - **Pointed:** a triangle, a five-pointed star, a spike of 15° (63%), a crescent (72%), a
    mace (97%), a pine of three tiers, ovals of five to one and more, a sword (in 348 masks of
    360) and a ghost with a wavy hem (in 358).
  - **Between, and steady there** (no share moves by more than a quarter; which leads says
    nothing): a tombstone (50% round, 50% square), a dagger (50% square, 50% pointed), an axe
    (42% to 50% round, 35% to 52% square), a figure with its arms at its sides (48% round,
    52% square), a key (51% round).
- **Corners rounded and cut, on five proportions** (every degree):

  | box | rounded by a tenth of its shorter side | a sixth | a fifth | a quarter | a third | swing in the band |
  |---|---|---|---|---|---|---|
  | 0.6 by 0.6, 0.8 by 0.6 | square 99% to 100% | round 17% to 29% | 57% to 69% | 84% to 91% | 99% to 100% | 0.11 at the most |
  | 0.8 by 0.4 | square 99% to 100% | round 12% to 33% | 48% to 71% | 76% to 93% | 98% to 100% | 0.23 |
  | 0.8 by 0.2 | square 98% to 100% | round 1% to 19% | 22% to 53% | 58% to 92% | 85% to 100% | 0.34 |
  | 0.8 by 0.1 | round 0% to 28% | 10% to 60% | 16% to 82% | 46% to 93% | 79% to 100% | **0.66** |

  Cut off flat by a twentieth to a seventh of the shorter side, every box but the thinnest
  is 95% to 100% square (0.8 by 0.1: 78% at a twentieth, 46% at a tenth); by a fifth, round
  (55% to 100%); by a third, eight-sided and round. A more rounded box is never the less
  round, a larger cut never the squarer, and no rounded or cut box has anything of a point.
- **Ovals:** round to three times as long as wide; **four to one is on the line** (round
  41% to 78%, leading round in 322 masks of 360, pointed in 38); pointed from five; **at no
  length and no angle more than a quarter square** (25%, an oval of four to one turned 18°).
- **What it cannot say steadily**, measured:
  - In the bands above (a rounding of a sixth to a quarter of the side, a cut of a fifth) the
    share moves with the angle by up to a quarter on a box of 0.8 by 0.4 or squarer, by up to
    a third on a plank a quarter as thick as long, and **by up to two thirds on one an eighth
    as thick** (0.8 by 0.1, about 25 pixels thick at 512: its corner is a few pixels round,
    and the pixels decide). The thinner the part, the sooner a rounding or a cut takes it
    round.
  - **A part under about six pixels thick** (a comb's tooth under twelve pixels across, a
    fortieth of the silhouette) has its two corners smoothed into one end, and is read as a
    point (combs of teeth 6 to 10 pixels wide: 60% to 92% pointed). Teeth of twelve lead
    square, of twenty and more are square.
  - **Shapes near half and half stay there**, and their lead says nothing: a half disc's two
    right angles are as real as its arc (a tombstone, and a mushroom or a dome on a flat base
    are about as square as round), a dagger's blade is as pointed as its guard and grip are
    square, and an axe is a little of each.
  - **A body bristling with more than about a dozen hair-thin parts** (a disc with sixteen
    whiskers of one pixel) is read by its hairs or not at all. Two to eight hairs change
    nothing.
  - A corner on a curve reads a few degrees over its own angle; a part under four pixels
    thick is not read; holes are not traced. It reads outlines, not surfaces: a ball on a
    square plinth seen from the front is both.
- **It needs pixels.** A piece under 110 pixels across is not read at all (a thumbnail never
  is), so the studio renders the three silhouettes again at 512 for it. A shape in one of the
  bands above swings further the smaller it is drawn; out of them a square, a disc, a triangle
  and a star read the same from 512 down to 140 (by the measure before; for the fifth version
  a test holds a disc and a box on 256 pixels to within a tenth of 512). A silhouette of 512
  takes 17 to 40 ms to read for ordinary shapes (on a busy machine, in release), and twelve
  times a disc at the worst found (a comb of one-pixel teeth: below, "Its fifth version"), on
  the caller's own thread, and is read anew at each call.
- **The rule.** `style.toml`: `[taste] shape_language = "round"` (or `"square"`, `"pointed"`),
  `shape_min_share` (0.5 unless given) and `shape_by` (`"outline"` unless given, or `"recipe"`:
  below, "From what the recipe is built of"): the check `shape_language` fails an asset whose
  share of that kind is under it, with both readings in its message and what to change in its
  fix; `[taste.allow]` exempts as for the other taste checks.
- **In the presets.** A base style may carry the ask (`Preset::shape`), and the style configurator
  writes it with the rest. **No shipped preset asks for one now**: `storybook` asked for round
  until the measure's third review (a ball on a string would have failed it), and a moonlit
  graveyard, a muted or a noir game and the neutral stage are not one shape.
- **What it says of Sepulcra** (2026-11-19, its 56 recipes through `studio_check`; **not counted again for the fifth version**, which was measured on drawn shapes only; by the
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
  fourth version**: five major findings, each from a run at 512 pixels, mended in its fifth version (below).
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
- **Its fifth version: the fourth review's findings, mended** (2026-11-20, by an agent that
  did not write the fourth; each finding reproduced first with the review's own probe; every
  number here at every degree of a quarter turn, plain, mirrored and shifted). **Not reviewed
  yet.**
  1. **A box whose edges wobble: mended.** The cause was as the review read it: whether a
     corner "had a straight side" came from the runs the straightening kept within a pixel and
     a half, and a wobbling slanted side was cut into runs too short to count. A side is now a
     window of the smoothed outline that turns by under 4° to 8°, and a corner is read as one
     between curves only on two pieces of positive evidence (no straight run near it, **and**
     the sides of its window bending); with neither it is read by its angle. A box whose edges
     wobble by half a pixel to two is 95% to 100% square in every mask (it was 0% in some); a
     plank so, 59% at the least and leading square. Held by `tests/shape_families.rs` (eight
     noisy boxes, discs and planks); seeded with the old rule (a corner with no side found is
     soft) and sides found as strictly as the straightening found them, 15 of its claims fail
     and two of the module's tests.
  2. **Limbs and necks: mended, by a different cause than the review read.** The thickness
     profile along the outline was not the trouble so much as **what a part is**: a leg that
     continues a body's side leaves no joint at all in the outline, inward or by a leap, and so
     takes the body's thickness however the joint is found. A point's part is now as thick as
     the most thickness about it once that has been level for six times itself (a limb's end:
     at once; a box's corner: at the box's; a taper: at its foot), and joints are where the
     smoothed outline turns inward by 30° in one place (a property of the shape: it moves a
     pixel when the shape does). The review's suggestion, the branches of the spine, was not
     needed. The figure of a ball, a box and capsule limbs is 69% to 85% round and leads round
     in all 360 masks (it was 22% to 67%, pointed in 102); in a T-pose 100%; arms at its sides
     48% round, 52% square in every mask (between, and steady); the bottles lead square in
     every mask (a curved shoulder led round in 282, square in 76); an axe moves by under a
     quarter (it moved by 0.30 in a degree). Held by the families test; seeded with a part's
     thickness taken as the most along it (no plateau), 8 of its claims fail (the combs first).
  3. **The bands on a plank: mended where the cause was the code, and a limit where it is the
     pixels.** Most of a plank's swing came from a second source of corners: where two runs
     the straightening kept met at 30° to 40°, a corner for certain; the chords a small
     rounded corner is cut into meet so, or not, by the angle (a plank of 0.8 by 0.2 rounded
     by a sixth of its width was 6% to 61% round). It is gone, and a single turn of 45° is now
     wholly a kink (it was 0.68 of one), which is what the rule of certain corners was there
     for (a corner cut off flat). What is left grows as the part thins: up to a quarter on a
     box of 0.8 by 0.4 or squarer, a third to two fifths on a plank of 0.8 by 0.2, two thirds
     on one of 0.8 by 0.1 (above). Held by the families test, per proportion. **A convention
     moved:** a box is square to a sixth of its shorter side and round from about a quarter
     (it was round from a fifth); a corner cut by a seventh of the side is 95% square (it was
     79% to 98%).
  4. **The round advice: mended**, and every advice text is now tried: "a radius of a third of
     that part's width or more" for round; "rounded corners under a tenth of the part's width
     and corners cut off flat under a twentieth of it" for square; a spike of 30° for pointed.
     `the_checks_advice_leads_to_a_pass` does what each says to boxes of 0.8 by 0.6, 0.4 and
     0.2 (and the spike) and holds the check passing at the default share in every mask; the
     old "a quarter of that part's thickness" fails it.
  5. **Thin parts on a thick body: mended to a limit.** The cause was as the review read it:
     the smoothing and the chord were sized by the whole shape. Each part is now smoothed and
     its direction taken at its own thickness (in two looks: the first finds the parts, the
     second reads them). A comb's teeth of 12 pixels lead square (they led pointed), of 16
     with long teeth 96% square, of 20 and more square (79% at the least). Under 12 pixels (a
     part under six pixels thick) the smoothing's floor of two pixels still runs a tooth's two
     corners into one end, and it is read as a point: **a limit**, said above and held by the
     test as it is, so that a change to it is seen.
  Minor:
  6. **A short stalk: mended.** A step takes its class from the corners of its own part, not
     across a joint, and a stalk's tip is of the stalk's own thickness: an apple's stalk of 6
     pixels and a disc's whisker of 30 leave it 100% round in every mask of every degree (24%
     pointed before; a test). A rule that a step ignore the corners of parts under half its
     thickness was tried, and once the other mends were in, nothing at any degree needed it: it
     is not in the code.
  7. **Common assets**: a sword now leads pointed (in 348 masks of 360), a disc with ten smooth
     lobes is round (98%; it was wholly pointed), a heater shield leads square (57%). Not
     changed, and said above as what the measure is: a dagger is half square and half pointed,
     a mushroom and a dome on a flat base lead square (their brims are right angles), a ghost
     with a wavy hem is about half pointed, a tombstone sits at half and half.
  8. **Floors that held only at the sampled angles: mended.** The families test holds its
     claims at every degree, mirrored and shifted, in its ignored run (four minutes in release
     on a busy machine), and every nine degrees in the default one (half a minute in debug).
  9. **The median of five: not mended.** It is kept (now in the parts' first look as well),
     and still no test fails without it: seeded out, the module's tests and the families every
     nine degrees all pass. It is there for a disc that touches one step alone; whether it
     matters at every degree was not measured.
  10. **The cost: said.** On a busy machine, in one process: a disc 31 ms, a box 25 ms, a figure
     17 ms, a comb of 8-pixel teeth 32 ms, a box with wobbling edges 26 ms, the whole picture
     40 ms; **the worst found, a comb of one-pixel teeth, 354 ms: twelve times a disc** (a
     comb of one-pixel teeth with two-pixel gaps 7 times, a box with such a comb 5 times; the
     `cost` test writes them). The fifth version reads each outline twice and costs about
     twice the fourth for ordinary shapes.
- **Its fifth review** (another model, 2026-11-20, of `cf54bb37`) **said not to merge the fifth
  version**: four major findings, each from a sweep of a quarter turn at 512 pixels, plain,
  mirrored and shifted; two are worse than the fourth version, which it ran beside. None is
  mended yet. **What "Its fifth version" above gets wrong:**
  1. **The cost is not twelve times a disc: a coiled shape takes seconds.** The search for
     where a part's thickness levels off runs to the part's end for every step where it never
     does: a coiled wire three pixels wide takes 9.8 s, 303 times a disc (33 ms in the fourth
     version), and a comb of one-pixel teeth as tall as the picture 1.8 s. `studio_check`
     reads the measure twice on each of three silhouettes: a coiled rope or a spring costs
     about a minute.
  2. **"A corner between two curves is never square" is false for gentle curves.** The
     evidence asked of a curve is a bend of 0.4° to 1° a step, which an arc of a radius over
     about 140 pixels does not have: a lens with tips of 90° leads square in all 360 masks
     (38% to 59% square), a leaf is 35% square and a smooth teardrop 29%. The fourth version
     read all of them 0% to 4% square.
  3. **The short stalk is mended only at the widths that were tried.** An apple whose stalk
     is 9 to 11 pixels wide reads up to 30% pointed at some angles and round at others (the
     fourth version read it 22% pointed, steadily): a steady error became a flicker. The
     advice's "a stalk under half as thick as the body changes nothing" is false there.
  4. **The round advice fails the check again**, on a thinner part than the test tries: a
     plank of 0.8 by 0.05 with corners rounded by a third of its width is 39% to 91% round and
     leads square in one mask of ten.
  Smaller: a figure whose arms are capsules eight pixels wide still flips between round and
  pointed; a cloud (an oval with two bumps) leads pointed in every mask, a dog with tapered
  legs and a bell are two thirds pointed, a sunburst of 24 shallow points is wholly square;
  **five rules are held by no test** (the least length of a level stretch, the smoothing of
  each part at its own thickness, the median of five, a point's weight between 1.5 and 2,
  how straight a side must be), so that the mend of the comb's teeth is shown by none;
  **an oval drawn on 160 to 200 pixels reads up to 62% square** (the measure reads from 110
  pixels; everything holds at 384 and 512); the median is in the first look only, not "as
  well". It found sound: boxes with wobbling edges in other proportions and seeds; the
  square, cut and pointed advice; snowmen, pears, rings, lobes, crescents, stars, gears and
  combs of 12 to 16 pixels; about 28 of 38 common assets read as a person would; every edge
  case; no panic and the same answer twice. **Five reviews, five "do not merge".**
- **From what the recipe is built of: the second measure, built** (2026-11-19;
  `specula-model` `made`, `specula-studio` `built`). The modeller records, for every shape a
  recipe names, **that shape's own outlines along its own axes and the surface it has in the
  model**: a box's three rectangles, a ball's circle, a cylinder's section and its side (a
  rectangle; a cone's, a triangle), an extruded outline and the box it is raised to, a lathe's
  section and its profile mirrored, a sweep's profile and its path laid out straight (a horn
  that narrows to nothing comes to a point), each primitive of a sculpt. What a boolean or a
  sculpt takes away is not recorded, and the wall a cut makes is its cutter's surface, which
  no record speaks for; a part's operations change the surface (three copies count three
  times; since the second round of mends it is measured on each shape's own triangles after
  them) and not the outlines. The studio reads each outline with the measure above (drawn on 384 pixels), weighs
  a shape's outlines as the recipe weighed them and **the shapes by their surface**, and
  answers `built_of` in `studio_check` (`measures.thumbnails.built_of`): the three shares,
  the share of the surface that was read, and the eight shapes with the most surface, each
  with its path in the recipe, what it was made as, its share and what it reads as.
  - **What it says that a silhouette cannot**: which part is what, so a failed check names
    what to change; and it is not moved by the angle the asset is seen from.
  - **What it cannot say**: anything of an imported mesh, a terrain or a building
    (`read_share` says how much was read; a scatter's items are recorded by what was
    placed of each); what a part's operations did
    to its form (a taper does not make a box's outline a wedge's); how the parts are put
    together. **A straight-sided cylinder is a third round and two thirds square** by it, as
    it is in silhouette from the side: a can is not a ball. (Since the mends below this holds
    at any proportions: a coin of a hundred to one and a pole of fifty to one too.)
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
    not weighed by surface; the recipe answering unasked).
  - **Its first review** (another model, 2026-11-20, of `5781cc53`) **said not to merge it**:
    seven major findings, each shown by a probe, and none mended yet. **What is written above
    is the author's account, and these are what it gets wrong:**
    1. **A boolean operand's own operations are not counted in its surface.** A ball of 5 cm
       that its own `ops` scale by 20, unioned with a 1 m box, has 0.5% of the surface by the
       record and 68% as a part of its own; a cone arrayed 12 times inside a union counts
       once. The same in a sculpt: a mirrored or repeated primitive counts once, and
       `elongate` is ignored (a box with a row of nine balls reads 13% round, where the balls
       are about half of the surface).
    2. **A shape recorded inside an included recipe keeps that recipe's own path**, so paths
       collide (8 of Sepulcra's 54 recipes carry duplicates; `warding_stone`'s
       `parts[0].shape` is both its plinth and its included column's box) and the check's fix
       can name the wrong part.
    3. **An include's unrecorded surface is credited to its recorded shapes.** A 0.2 m box
       beside a 10 m terrain, included, is recorded at 100 m² (it has 0.24), and the reading
       says all of the surface was read.
    4. **A scatter records its items**, where this page says it records nothing, each
       template once whatever was placed (boxes at weight 1 and balls at weight 20: boxes
       66% by the record, balls about 86% in the model).
    5. **Every operand of an intersection is recorded whole.** A box intersected with a
       large sphere (the box itself, or a die with rounded corners) reads 95% round.
    6. **An outline too thin to read is dropped without a word, so "a cylinder is a third
       round and two thirds square" is false for a long or flat one.** A disc at 20 to 1
       reads as said; a coin at 100 to 1 and a pole at 50 to 1 read 100% round. Legs, rods and
       posts are past the line.
    7. **A measured human figure reads 98% square** (its columns' side outlines have flat
       ends, so every limb is a rectangle), where the same body sculpted of a ball, a capsule
       and cones reads 99% round.
    Smaller: a sculpt group's stretch is applied on the wrong axes; the cost has no ceiling
    (64 shapes of 96 views: 38 s, and 12.8 MB in the cached model; Sepulcra's assets take 5
    to 153 ms); no test holds the records of an extrude, a loft, a sweep, a torus, a capsule,
    a plane, the finish's scaling, an include, a scatter or an intersection; a loft's lean, a
    closed sweep's path and a sculpted cylinder's corner are not read; where the style does
    not ask for the recipe and the silhouettes cannot be read, the recipe answers, which this
    page did not say; a bucket and a mushroom cap read half pointed (the silhouette measure's
    reading of a trapezoid). It found sound: the check's every branch, the parsing, the
    determinism, the pinned props (no mesh changed), Sepulcra's 42, 11, 1 and 2, the
    cutters, and that a box, a crate and a table read square and a ball, a torus and a
    sculpted character round. The mends are the next work, by an agent that did not write
    it, and reviewed again.
  - **What was mended** (2026-11-20, by a model that did not write it; each finding seen first
    with the reviewer's own probe, each mend held by a test that fails without it:
    `specula-model`'s `tests/made_record.rs` and `specula-studio`'s `built` tests). The rule
    everywhere now: **a record's surface is that shape's own surface in the finished model,
    and what no record speaks for is `made_rest_m2`.** Not reviewed yet.
    1. **Operands after their own ops, and what is left of each.** A boolean operand's record
       is scaled by what its own `ops` did to it (the ball scaled by 20 has 12.5 m², as it
       does as a part; the twelve cones count twelve times, less what the slab hides). And
       the boolean now says which operand each triangle of its result came from (every face
       of the result is a piece of one operand's face, so this is exact): each operand is
       recorded with what is left of its surface, in a union, a difference and an
       intersection alike. A cutter has no record, and **the wall a cut makes is the
       cutter's surface: no record's** (the rest; this page said it was the cut thing's). A
       bevel's strips are its tubes': the rest too. (The reviewer's own probe, a ball of 3 m
       and a 1 m box, is not "exactly the box": a ball stands on the ground as a box does, so
       its bottom is the box's floor, and the record says so: the box 82%, the bowl 18%.)
    2. **Sculpts weigh each primitive by the surface it makes in the mesh**: every triangle of
       the mesh goes to the primitive whose field is the tree's at its middle (in a union the
       nearest, in an intersection the furthest, in a difference the first child or the cutter
       whose wall it is), at most 4,096 triangles asked, within a work limit charged to the
       build's (a measured figure: about 600 of its 8,000 triangles, in about half a
       millisecond). So a mirror's and a repeat's copies count, an elongation counts and
       widens the outlines it is along, and an intersection's primitive counts by what is left
       of it (a box with a row of nine balls: 58% round; a box inside a ball: the box). In a
       smooth blend the nearest primitive takes the blend: a stated rule, not a measure.
    3. **Includes** record under the including part's path, as their errors are written
       (`parts[1].include(kid).parts[0].shape`), and **one record a path** everywhere.
    4. **Part ops and a finish scale a record by what they did** to the part's pieces (area
       before against after), never to a total that holds surface no record speaks for: the
       0.2 m box beside an included terrain is 0.24 m², and the terrain is the rest (read
       share 0.11).
    5. **A scatter records each item by what was placed of it** (the probe: balls 88%, boxes
       12%), under its path in the items; a variant never placed has no record, and an item's
       variants are one record.
    6. **A thin outline is read at six to one at the most**: one straight along its middle (a
       rod's rectangle, a capsule's stadium, a spike's triangle) has length taken out of its
       middle and its ends kept; one curved along it (an oval, a profile) is pressed evenly,
       so an oval of a thousand to one is read as one of six (pointed). Rectangles, stadia,
       boxes rounded by four tenths, frustums, spikes and ovals from six to a thousand to one
       hold their lead at 384 and at 512 pixels (a test). And **what is not read counts as not
       read**: an outline the measure cannot read takes its share of its shape's surface out
       of `read_share`. Coin, disc, can and pole are all a third round and two thirds square.
    7. **A column's outline from the side closes in the caps its field has** (half an ellipse
       past each end, as high as the end's section is narrow), where it had flat ends. With
       the weights of (2) (the boxes the figure cuts its hands and feet with, recorded whole
       by an intersection before, are what is left of them: nearly nothing), **a measured
       figure reads 96% to 98% round** (it read 98% square; the whole male figure 96%, the female body 98%: corrected after the second review, which measured them; since the second round, 96% and 97%). A person reads round by this measure:
       it is made of limbs, a trunk and a head, each a column, a ball or a round cone.
    8. A group's stretch and elongation are put on each outline along its own axes.
    9. **The cost has a ceiling.** The record keeps at most 256 outlines of 128 points in all
       (a quarter of a megabyte; a shape whose outlines do not all fit keeps its heaviest that
       do, and the weight of the rest as an outline of no points, read as not read); the
       studio reads at most 32 distinct outlines for an asset, identical ones once, the
       largest shapes first, and what is past them is not read. Measured in release while
       other builds ran on the laptop (one outline 8 ms there): **the largest record a recipe
       can make** (64 sculpts of 32 primitives each, every outline different: 112 KB) reads
       in 0.27 s, and its read share says 1%; Sepulcra's assets read in 9 to 243 ms each;
       asking a measured figure's triangles which primitive made them takes about half a
       millisecond.
    10. Tests now hold the records of an extrude that tapers, a loft that leans, open and
        closed sweeps, a torus, a capsule (end to end), a plane, a frustum, a sculpted
        cylinder's corner, the finish's scaling, an include, a scatter, an intersection, the
        record's bounds, a group's stretch, and the check's three names out of four. The two
        guards the reviewer found redundant are gone (the far level's tally is its own and
        nobody reads it; nothing recovers from a failed shape on the same tally).
    11. **Read now**: a loft's lean (each side by its stations' shifts), a closed sweep's
        loop as smoothed (its plan, beside its profile: a ring of round tube is round, and a
        square frame of it would be half each), a sculpted cylinder's `corner` (its side's corners rounded by it; a frustum's
        are not). **Said here now**: when the style does not ask for the recipe and the
        silhouettes cannot be read, the recipe's reading answers, with "its silhouettes could
        not be read" in the message.
    **Still limits**: a node's turn (`rotate`), twist, bend, rounding and shell do not change
    its outlines; a smooth blend's surface goes to its nearest primitive; a generated
    building, a terrain and an imported mesh record nothing; a part's taper does not make a
    box a wedge. **For the next review of the silhouette measure**, not worked round here: a
    frustum (a bucket) reads half pointed and a mushroom's cap half pointed, which is that
    measure's reading of a trapezoid and of a dome on a rim, and not what a person would say.
    By the record after the mends, and read by the silhouette measure's fifth version,
    **Sepulcra is as it was: 42 lead square, 11 round, 1 pointed, 2 unread** (the crypt and
    the graveyard's ground, generated); its includes now carry their own paths, and
    `graveyard_plot` is read at 11% of its surface (its ground is the rest), where its terrain
    was credited to its headstones.
  - **Its second review** (another model, 2026-11-20, of `ead581da`) **said not to merge the
    mends**: two major findings, each shown by a probe, and neither mended yet. **What "What
    was mended" above gets wrong:**
    1. **"A record's surface is that shape's own surface in the finished model" is false
       under any operation that is not the same in every direction.** An operation scales
       every record under it by one ratio (the whole's surface after over before), and a
       scale by a vector, a taper, a bend, a twist or a subdivision changes each shape by
       another factor. A box of 0.6 by 3 by 0.6 unioned with a ball and flattened to a
       twentieth is 72% square by the record and 59% round in the model; a box beside a
       terrain, flattened, is recorded at nine times its surface. Sepulcra's `rune_column`
       scales its includes so.
    2. **A long outline with a point in its middle is pressed flat at both ends.** Whether an
       outline is "curved along its middle" is taken from whether a point lies there: a
       stadium with one more point on its straight side (the same polygon) reads 89% to 100%
       square where it read 100% round; a capped column of three sections (a spear's shaft,
       a limb) reads up to 98% square; a sword's blade, readable at its own length as 61%
       pointed, reads 63% square after the press.
    Smaller: a recipe of 52 sculpts that built before is refused for work now (the
    attribution is charged about a hundred times what it costs); a group's stretch still
    lands on the wrong axes for a round cone and for a column not along Y; a sculpt's shares
    from sampled triangles can be far off (30 balls with a displacement of 0.2 mm: up to 89%
    apart from the same mesh without it); a sculpt keeps its first 96 outlines in the tree's
    order and not its largest; a long outline on a diagonal is not shortened and its reading
    flips between 384 and 512 pixels; raising the reading's cap from 32 to 48 fails no test,
    and the test that the records add up cannot fail on a wrong record (the rest is defined
    as what is left); and three sentences above are false of the code as mended: a cut's
    wall is the rest's and not the box's, a scatter is recorded and not unread, and a whole
    male figure reads 96% round. It found sound: no mesh changed; a boolean's faces are
    given to the right operand exactly; operand ops, nested booleans, includes of every kind
    and scatters on cases of its own; the cost's ceiling; the determinism; Sepulcra's counts;
    and 15 of 23 ordinary recipes read as a person would.
  - **What was mended, second round** (2026-11-20, by a model that wrote neither the measure
    nor the first mends; each finding seen first with the second reviewer's probe, each mend
    held by a test that fails without it, and each such fault seeded back once to see its
    test fail). Not reviewed yet.
    1. **A record's surface is measured, not reckoned.** Every triangle of the finest level
       carries what it was made as (`Shape::owners`, a slot of the build's record), through
       every operation: a move, a turn, a scale by a vector, a taper, a twist, a bend leave
       it; a copy copies it; a subdivision gives a triangle's four pieces its owner; a weld
       drops it with the triangles it drops; every face a boolean makes keeps its input
       triangle's. A record's surface is summed from its own triangles in the finished model,
       once, at the end. No ratio is applied anywhere. The flattened union of the review is
       now exact (the box 1.080 m², the ball 1.578, as built alone), and so is the box beside
       an included terrain (0.096). A test builds 28 recipes drawn from a fixed sequence
       (shapes with their own ops; part ops that scale by vectors, taper, twist, mirror, copy,
       subdivide and bend; unions; includes; a finish) and holds every record against the
       same shape built alone, measured on that model's mesh: within a part in ten thousand,
       and in a thousand for a union's operand (whose truth is a union of it alone, since a
       boolean cuts faces into triangles of its own). **What cannot keep the tie**: inside one
       boolean, coplanar pieces of one operand, one colour and one tag are joined into one
       face before they are cut into triangles, and the face is its first piece's (only an
       operand that holds several shapes, a nested boolean with ops of its own or a sculpt,
       can blur so); a bevel's strips and a cut's walls belong to no shape (the rest). The
       far level and the LODs carry nothing and cost nothing for it.
    2. **A long outline keeps both its ends.** It is turned to the axis it is longest along
       (its area's moments, laid along a side of a quarter of its length or more within five
       degrees of that axis), and its length is taken out of its straight stretches, found
       from its geometry (its two sides straight and level within a fiftieth of its width,
       however many points lie on them), from all of them by what each can give. What is
       still too long (an oval, a taper, a horn, a round cone) is pressed evenly between its
       two ends, each as long as it is wide, which are kept as they are. Six to one stays the
       line: past it the readings of ordinary outlines at their own length begin to differ
       between 384 and 512 pixels (a horn at eight, a cut rectangle, a bottle and a club at
       twelve). The stadium with one more point reads round, columns of any number of
       sections round, the blade 61% pointed as at its own length; rods, stadia, columns and
       spikes at 10 to 45 degrees read as lying flat (they were not read past about forty to
       one, and flipped between sizes nearer). **What it cannot do**: a taper's middle is
       steeper after the press; a crescent of eight to one and more, and a superellipse of
       power 6, read differently at the two sizes (the latter at its own length too); the
       measure reads a round cone's round share unsteadily, at its own length as after.
    3. **The record never refuses a build.** Asking which primitive made each triangle spends
       a limit of its own for the build (120 million units, about an eighth of a second on
       the reference laptop at the most: a figure's tree takes about a nanosecond a unit),
       never the build's, and never passes it; a triangle not asked within it is the rest,
       and the sculpt's stat says how many (`owners_unanswered`). The review's 52 sculpts
       build, with 11% of their triangles unasked.
    4. **A round cone's and a column's outlines are laid along the axis they run on the most**
       (a column's breadth toward the axis its `side` points along), so a group's stretch
       lands on its length (tests: a round cone along Y stretched along Y, a column along X
       stretched along X).
    5. **Every triangle of a sculpt is asked, up to 16,384**, and every so many past that, each
       answer standing for the triangles after it (the mesher writes them block by block).
       The thirty displaced balls were sampling: the displacement's cost had cut the sample
       to about a hundred of 11,664 triangles, each standing for a hundred and twenty; with
       every triangle asked their shares are those of the plain balls. Asked every other
       triangle (a mesh of up to twice the count), a measured figure's primitives' shares of
       the surface move by at most 0.13% of the whole (8% of a primitive's own, for one of 1%
       or more): a test holds 0.2%.
    6. **A sculpt keeps its largest primitives' outlines** (by their own triangles' surface),
       up to 96; the rest's weight is an outline of no points, not read. Forty small balls
       before a large box keep the box's three rectangles.
    7. Long outlines on a diagonal: see 2.
    8. **Every cap is held by a test that fails when it moves either way**: 64 shapes kept,
       96 outlines a shape, 256 a record, 128 points an outline, 32 outlines read, 512 shapes
       and 8,192 slots while building, 16,384 triangles asked, the asking's limit. The
       twelve cones are twelve (within a part in a thousand of the alone truth).
    9. The sentences above that the second review found false are corrected where they
       stand (the cut's wall, scatters, the figure's share).
    **By the record now, Sepulcra is as it was: 42 lead square, 11 round, 1 pointed, 2
    unread**, and no recipe's reading moved by more than 0.01 (`rune_column`, whose includes
    are scaled by `["k", "k", 1]`, reads 46% round and 54% square before and after: every
    shape it records is a box, and surface moved between boxes moves the shares little). Of 23 ordinary
    recipes read as a person would, the swords now read square by the extrude's and the
    cone's convention (an extruded blade is its outline and two slabs, a cone its circle and
    a triangle), and a spear of a column and a cone reads round (a column is capped); the
    barrel, mug, bucket and rounded console disagree as before.
