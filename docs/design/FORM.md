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
orthographic rule, the waits' bounds. **These mends are not reviewed yet.**

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

**Not yet:** `studio_lineup`, a filmstrip and `sight_compare` do not take `form`. A view of the surface's thickness, and the
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
| `facet_deg` | how coarsely round surfaces are faceted: the turn that 95% of the length of the **curved** edges stays under. A curved edge is one **shaded as one surface** (its two faces share their normals along it, so it is meant to read as round) whose faces turn more than 1°, however far: a smooth-shaded cylinder of six sides is faceted by 60°, of 64 sides by 5.6°. Flat ground and the diagonals of flat quads are left out; an edge shaded as two faces is a hard edge, not a facet. 0 with fewer than three curved edges; `curved_edge_m` says how much of the shape it speaks for | `max_facet_deg` |
| `seam_m` | length of the edges that are not creases across which the two sides are shaded more than 15° apart: a line in the light where the surface has none | `max_seam_share` (of the shape's size) |
| `smeared_crease_m` | length of the creases shaded as if they were round (one normal for both faces): the dark smear along a hard edge | `max_smeared_share` (of the creases) |
| `relief` | how the shape's relief is shared between `big`, `medium` and `small` form: how much the surface's facing varies within a box a quarter, a sixteenth and a sixty-fourth of its size, sheet by sheet | `max_small_relief` |

Also in the facts, with no check of their own: `pieces`, `pieces_clear_of_ground`, `open_edge_m` (the
rim of an open sheet: grass cards, cloth), `tangled_edge_m` (edges more than two triangles share),
`crease_m`, `razor_m`, `edge_m`. A mesh without a normal for every vertex has `shading: "not
measured"` in place of the two measures of shading.

- **Only loose pieces are checked unless the style says more.** A piece that hangs in the air and
  touches nothing is a fault in any style; how sharp, how faceted and how busy a shape may be is the
  style's to say (Sepulcra's assets have a median razor share of 0.18, and their smooth-shaded
  surfaces are faceted by 0 to 45°, 23° at the median: its look, not a fault). `[form.allow]` exempts a subject from a check, with the reason in a comment, like
  `[taste.allow]`.
- **Loose means beyond doubt.** Two pieces touch when their bounds come within a hundredth of the
  shape's size of each other (and never more than 5 cm, however large the shape), so a ring round a
  post it never touches counts as touching; a piece whose bounds reach the ground (y = 0, give or
  take that much) rests on it (a coffin's lid lying beside it, rubble). What is left hangs in the
  air, or lies wholly under the ground. A shape of one piece that hangs clear of the ground is
  loose too.
- **The shape is its triangles.** Its size is taken from the corners of the triangles that are
  measured: a vertex no triangle uses (an importer keeps every vertex of a file) and a corner at
  infinity, or past a million kilometres, are no part of it.
- **Slivers are left out.** A triangle a thousand times longer than it is broad has a facing that
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
- **These mends are not reviewed yet.**

**Not built:** faceting at the size seen (the measure is the mesh's own turn, not what the gameplay
shot shows of it: a facet under a pixel is no fault); pieces that pass through each other by
accident; a check for the figure's seams between its separately meshed parts (they share no edge, so
`seam_m` does not see them: the form views do).

## Shape language

How round, square or pointed an asset's outline is, as numbers (`specula-studio`, `shape`). Round
reads as friendly, square as sturdy, pointed as dangerous: a convention of character and prop
design, not a law. So it is **reported for every asset** (`studio_check`: `measures.thumbnails.shape`)
and **checked only when a style asks** for one.

- **The measure.** On each thumbnail silhouette (the three the taste checks take: front, side,
  three-quarter): the outline of the largest piece is traced and straightened to a polygon that
  stays within a fortieth of the shape's size of it; each corner is told by how far the outline
  turns there. Under 60° it is part of a curve; 60° to 105° is a square corner; sharper is a
  point (the corner's own angle is then 75° or less). Corners within a tenth of the size of each
  other are one corner, so a small bevel or a tight rounding does not make a square corner round;
  two sharp corners stay two unless they are a pixel or two apart, so the cut end of a bar is two
  square corners and the tip of a spike one point. The shares (`round`, `square`, `pointed`) are
  of the outline's whole turning and sum to 1; `leading` names the largest; `convexity` is the
  outline's area over its hull's; `corners` counts the corners of 60° or more. The three
  silhouettes' means are the asset's.
- **Known shapes** (a test, at a thumbnail's size and four times it, upright and turned 17° and
  45°): a disc and an ellipse are round, a box and an L square, a triangle and a star pointed; a
  box rounded by a twentieth of its side is still square and one rounded by a third is round; a
  thin bar is square at its ends and a spike is pointed.
- **The rule.** `style.toml`: `[taste] shape_language = "round"` (or `"square"`, `"pointed"`) and
  `shape_min_share` (0.5 unless given): the check `shape_language` fails an asset whose share of
  that kind is under it, with the three shares in its message and what to do in its fix;
  `[taste.allow]` exempts as for the other taste checks.
- **In the presets.** A base style may carry the ask (`Preset::shape`), and the style configurator
  writes it with the rest: `storybook` asks for round (half of an outline's turning). The other
  four ask for nothing: a moonlit graveyard, a muted or a noir game and the neutral stage are not
  one shape.
- **What it says of Sepulcra** (2026-11-19, its 56 recipes through `studio_check`): 41 lead
  square, 13 pointed and 2 round (`grave_rock` 0.83 round, `mausoleum_domed` 0.70); the coffin is
  0.77 square, the wraith 0.58 pointed, the iron fences and the brazier's fire pointed. A graveyard
  of cut stone and iron: no check is asked of it.
- **Limits.** It reads outlines, not surfaces: a ball with a square plinth seen from the front is
  both. Holes are not traced. At 64 pixels a part under about three pixels across is read as a
  point whatever its end. It is not reviewed yet.
