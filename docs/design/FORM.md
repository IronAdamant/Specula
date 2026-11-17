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
| The form checks | not built | |
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
| `clay` | every lit surface as the same matte clay, under a studio's light that comes with the camera: a key from the upper left, a fill from the right, the glow of a pale ceiling and a dim floor, a soft shine | the first look at any shape: it reads the same from every side and under any world's light; flat where it should be round shows as an even tone |
| `normals` | where each surface faces, as colour: world x, y, z as red, green, blue (128 is 0) | faces that point the wrong way; a seam, where two neighbours differ in colour by a step; shading that does not follow the shape |
| `curvature` | how fast a surface turns: mid grey where it is flat, lighter where it turns away (a ridge, a ball), darker where it turns in (a groove) | ledges and creases as thin light or dark lines; faceting as a pattern of grey tiles with pale borders; where a form that should swell is flat |
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
  plain light; sprites, tiles and in-world text are drawn as they are; particles and decals are left
  out, and the packet's effects say they were not drawn. The ID buffer is the world's, so
  `visible_px` and the boxes are the same as in the world's frame (a test).
- **A form view is for one capture.** The window is never drawn as one, and the capture after it is
  the world's again, to the byte (a test).
- **Normals are the shading normals**: a normal map shows in `normals`, `curvature` and `stripes`,
  since it is what a light would follow.
- **Cost:** one branch on a value of the frame in the lit fragment shader. The world's frame pays
  that and no more.

How they are made: the frame carries the view and the scale in two spare slots of its uniform
(`FrameData::sky_params.z` and `.w`); the views are in `shaders/form.glsl`, and `mesh_frag.glsl`'s
`shade_lit` and `terrain.frag` return `form_view` before any light is gathered; `sight::form_frame`
turns a capture's frame into the plain one a view is drawn in.

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
