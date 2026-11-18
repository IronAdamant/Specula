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
| Shape language in the presets | not built | |

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
| `curvature` | how fast a **smooth-shaded** surface's normal turns: mid grey where it is flat, lighter where it turns away (a ridge, a ball), darker where it turns in (a groove) | where a form that should swell is flat; a ledge or a soft crease as a light or dark band. **A hard edge does not show here**, and a flat-shaded shape reads flat however coarse it is: the normal does not turn within a face. Look for those in `normals`, and in the asset's form facts (`crease_m`, `razor_share`, `facet_deg`) |
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
- **Cost to the world's frame: none.** The views are compiled only into pipelines made for a form
  view (a specialisation bit, `FEAT_FORM`, as shadows and fog have), built the first time one is
  asked for. (They were first a branch on a value of the frame in every lit pipeline: a reviewer
  measured the world's scene pass 2 to 5% slower for it on the reference iGPU.)

How they are made: the frame carries the view and the scale in two spare slots of its uniform
(`FrameData::sky_params.z` and `.w`); the views are in `shaders/form.glsl`, and `mesh_frag.glsl`'s
`shade_lit` and `terrain.frag` return `form_view` before any light is gathered, in pipelines
specialised with `FEAT_FORM`; `sight::form_frame` turns a capture's frame into the plain one a view
is drawn in. Clay's light takes the camera's own right and up from the frame's inverse projection.

**The review of the form views** (a model that did not write them, 2026-11-19) said not to merge
them: clay's light was built from the world's up and each pixel's own direction, so four balls seen
from above were lit from four sides; `gi: "probes"` with a form drew the probes' markers as clay
balls and every form capture in a world with baked light said its light "was not ready"; an
inside-out ball was the same picture as a right one, and the description promised hard edges in
curvature, which cannot show them; and the world's frame was slower. Each is mended and is a test;
shadows' discs are left out and the packet's lights count what was drawn.

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

**Not yet:** the asset studio's sheets (`studio_sheet`, `studio_lineup`, `studio_compare`), a
filmstrip, and `sight_compare` do not take `form`. A view of the surface's thickness, and the
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
| `facet_deg` | how coarsely round surfaces are faceted: the turn that 95% of the length of the **curved** edges stays under (not creases, and turning more than 1°: flat ground and the diagonals of flat quads are left out, or a dome on a slab reads as smooth). 0 with no curved edge | `max_facet_deg` |
| `seam_m` | length of the edges that are not creases across which the two sides are shaded more than 15° apart: a line in the light where the surface has none | `max_seam_share` (of the shape's size) |
| `smeared_crease_m` | length of the creases shaded as if they were round (one normal for both faces): the dark smear along a hard edge | `max_smeared_share` (of the creases) |
| `relief` | how the shape's relief is shared between `big`, `medium` and `small` form: how much the surface's facing varies within a box a quarter, a sixteenth and a sixty-fourth of its size, sheet by sheet | `max_small_relief` |

Also in the facts, with no check of their own: `pieces`, `pieces_clear_of_ground`, `open_edge_m` (the
rim of an open sheet: grass cards, cloth), `tangled_edge_m` (edges more than two triangles share),
`crease_m`, `razor_m`, `edge_m`. A mesh without a normal for every vertex has `shading: "not
measured"` in place of the two measures of shading.

- **Only loose pieces are checked unless the style says more.** A piece that hangs in the air and
  touches nothing is a fault in any style; how sharp, how faceted and how busy a shape may be is the
  style's to say (Sepulcra's assets have a median razor share of 0.18 and facets up to 28°: its look,
  not a fault). `[form.allow]` exempts a subject from a check, with the reason in a comment, like
  `[taste.allow]`.
- **Loose means beyond doubt.** Two pieces touch when their bounds come within a hundredth of the
  shape's size of each other, so a ring round a post it never touches counts as touching; a piece
  whose bounds reach the ground (y = 0, give or take that much) rests on it (a coffin's lid lying
  beside it, rubble). What is left hangs in the air, or lies wholly under the ground.
- **The shape is its triangles.** Its size is taken from the corners of the triangles that are
  measured: a vertex no triangle uses (an importer keeps every vertex of a file) and a corner at
  infinity are no part of it.
- **Slivers are left out.** A triangle with next to no area has a facing that is noise, and the
  edges it would make razors of are edges nobody sees.
- **Relief is by boxes, sheet by sheet.** A triangle is in every box a point of it is in; what is
  in a box is taken sheet by sheet (triangles joined through edges in it), so the two sides of a
  thin plate are not taken for one surface that turns right round. A ball's relief is all big; bumps
  a thirtieth of its size across put most of it in the small; a ribbed sheet nearly all. **It is a
  coarse measure:** the boxes lie along the axes and are sized by the bounds, so the same shape
  turned 37° about a slanted axis moves a share by up to 0.15 (a ball on a block: 0.88 big upright,
  0.73 turned). Set `max_small_relief` with that in mind.
- **Bounds:** a mesh of more than 600,000 triangles is not measured (the facts say `skipped`). Only
  pieces clear of the ground are compared with the others, and when that would be more than 50
  million comparisons the pieces are counted, the facts say `not compared`, and **the loose check
  fails saying so** (it never passes on what it did not look at). The relief is sampled at 200,000
  points at most for each of its three sizes. A 600,000-triangle ball takes 1.4 s (release, this
  laptop); the studio hands the measure its call's deadline, and a measure given up says `skipped`.

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
- **The mends are not reviewed yet.**

**Not built:** faceting at the size seen (the measure is the mesh's own turn, not what the gameplay
shot shows of it: a facet under a pixel is no fault); pieces that pass through each other by
accident; a check for the figure's seams between its separately meshed parts (they share no edge, so
`seam_m` does not see them: the form views do); shape language.
