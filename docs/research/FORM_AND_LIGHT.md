# Research: round and human forms within an agent's reach, and light that mimics ray tracing

Status: a research note, written 2026-11-16 at the owner's request, before M6. **Nothing here is built, and no
prototype was made**: every cost on the reference iGPU is unknown until one is. What of it is M6's and what
comes after is for the scope talk; the owner has said that matching an AAA studio's tools is the work after M6
([HANDOFF.md](../../HANDOFF.md)).

The owner's words:

> "You can generate graphical assets but to get really close to a good one, like round stuff, more human
> stuff.. I think this engine will help you with it. What we need you to do is do your research and see if you
> can add as much tastes as possible for AI agents to handle before we work on M6."
>
> "This engine needs to scale from very basic shapes to actually realistic figures and some level of
> raytracing (Don't use Nvidia's one. I think how it was done is too inefficient. You can come up with
> efficient ones that mimics it and there are new research papers by now that show that it's possible to
> match those."

## The short answer

- **Agents are weak at round and human form for a reason that tools can remove.** An agent describes a shape
  in numbers and words and cannot see curvature. A box costs three numbers; a believable shoulder costs
  hundreds unless the tool offers a handle whose plain result is already round. Taste has to be carried by the
  handles and by measures, not by the agent's eye.
- **Most of the handles are small additions to what the engine has.** The modelling crate already has
  distance-function shapes with smooth blends, capsules and ellipsoids, a subdivide step, bevels, and rigs
  that bind each vertex to its nearest bone. A figure built from a skeleton with a radius at each joint is
  mostly a composition of those.
- **An agent's judgement of 3D form cannot be trusted yet, by measurement.** This year's studies say so
  (below). So form is checked as numbers, shown in views where flaws make patterns, and left to a person
  for the last word, as the galleries already do.
- **Light that follows changes, soft shadows and reflections of things off the screen can all be had without
  ray-tracing hardware.** The route is distance fields, probes and screen-space traces. Unreal's default
  path, AMD's open one and several papers do it that way. Specula is unusually well placed: its assets are
  recipes, and many are distance functions already, so a distance field for every model can be made when the
  model is built.
- **Some of ray tracing cannot be mimicked cheaply**, and this note says which parts.

## 1. Where agents stand at 3D form (2026)

- **3DCodeBench** ([arXiv 2606.01057](https://arxiv.org/abs/2606.01057), May 2026) had twelve vision-language
  models write procedural modelling code from text and pictures. Its abstract reports that failures are
  mostly calls to an interface that does not exist as written; that models which do run still leave
  "disconnected or floating" parts; that more thinking and several rounds of refinement help; and that
  refinement needs an environment that runs the code and gives faithful feedback. It ranks results by people's
  preferences because automatic scores miss perceived shape quality. (Read: the abstract.)
- **3D-DefectBench** ([arXiv 2607.10826](https://arxiv.org/abs/2607.10826), July 2026) measured models as
  judges of fine defects in 3D assets. The best of twelve stayed behind trained people, and the answer moved
  with the model, the rendering, the camera set-up and the wording of the question. (Read: a summary.)
- **In this repository:** the asset studio measures silhouette, value contrast, palette, scale and flat
  lighting ([design/ASSET_STUDIO.md](../design/ASSET_STUDIO.md)). Nothing yet measures roundness, the
  proportions of a figure, or whether a surface turns smoothly.

What follows from it:

1. **Give handles whose plain result is well formed.** One number that takes a box to a sphere beats sixty
   vertices.
2. **Measure form.** Each thing a trained eye sees is turned into a number or a picture with an obvious
   pattern.
3. **Keep a person's eye last.** A measure says "this is faceted"; whether it is beautiful is still chosen
   in a gallery.

## 2. Round things

What exists ([design/PROCEDURAL_MODELLING.md](../design/PROCEDURAL_MODELLING.md)): primitives, lathe, sweep
and loft; bevels; a `subdivide` step; distance-function modelling (sphere, box with corner radius, cylinder,
cone, capsule, torus, ellipsoid) with smooth union, difference and intersection, meshed by surface nets or
dual contouring; noise displacement; rigs.

Handles to add, the cheapest and most useful first:

| Handle | What the agent writes | Why it suits an agent | From |
|---|---|---|---|
| **Roundness as one number** | `round: 0..1` on a box, a cylinder, an outline | 0 is the hard shape, 1 its fully rounded form, and everything between is valid | superellipses and superquadrics (Barr, 1981) |
| **Bone chains** | a line of points, each with a radius | a limb, a tail, a branch or a horn in a few numbers; always closed, always smooth | round cones under a smooth union (Quilez); the engine has the parts |
| **A body from a skeleton** | joints with radii | a whole creature in about twenty rows; the rig and each vertex's bone come from the same rows | B-Mesh ([Ji, Liu and Wang, 2010](https://diglib.eg.org/handle/10.2312/CGF.v29i7pp2169-2177)); Blender's Skin modifier |
| **Creases on subdivision** | a cage of few points, and which edges stay sharp | hard and soft in one surface without placing every vertex | Catmull and Clark (1978); semi-sharp creases (DeRose, Kass and Truong, 1998) |
| **Sculpt steps as data** | inflate, pinch, flatten, crease along a curve | a list of small edits, each undoable and readable | the sculptures of *Dreams* are lists of simple distance-function edits ([Evans, SIGGRAPH 2015](https://advances.realtimerendering.com/s2015/AlexEvans_SIGGRAPH-2015-sml.pdf)) |
| **Curves that stay smooth** | a profile as a few control points | a lathe or sweep whose surface has no kinks where points meet | splines with continuous curvature |

Measures to add to the taste checks (each a number with a threshold in `style.toml`, like the ones there):

- **Loose parts:** pieces that touch nothing, and pieces that pass through each other by accident. Exact
  and cheap, and the commonest fault in the study above.
- **Razor edges:** the share of edges sharper than a set angle with no bevel. A real object has none.
- **Faceting at the size it is seen:** the largest turn between neighbouring pieces of the silhouette, in the
  gameplay shot. Above a few degrees a curve reads as a polygon.
- **Smooth turning:** where the surface normal jumps across an edge that was not marked as a crease.
- **Big, medium and small form:** how much of the shape's relief sits at each of three sizes. The taste loop
  already says "fewer big shapes beat many small ones"; this makes it a number.
- **Shape language:** how round, square or pointed a silhouette is (its convexity and how sharp its corners
  are), against what the style asks. Round reads as friendly, square as sturdy, pointed as dangerous: a
  convention of character design, not a law.

Views to add to sight, because a flaw that is invisible in a lit picture is a clear pattern in these:
**clay** (one grey material, soft light), **normals**, **curvature** (red where it bulges, blue where it
dips), and **stripes** (reflected bands, as car designers use: a kink in a stripe is a kink in the surface).

## 3. Human figures

The roadmap already says realistic humans are a poor fit for recipes and names a "parametric character base"
for later. What has changed is that open ones exist:

| Body model | Licence | What it gives | Note |
|---|---|---|---|
| **Anny** (NAVER LABS Europe, Nov 2025; [arXiv 2511.03589](https://arxiv.org/abs/2511.03589)) | Apache 2.0 | one body from infant to elder, with readable parameters, built on the MakeHuman community's measurements and calibrated with WHO statistics | check the licence of the data files when used |
| **MHR** (Meta, Nov 2025; [repository](https://github.com/facebookresearch/MHR), [arXiv 2511.15586](https://arxiv.org/abs/2511.15586)) | Apache 2.0, by its repository | a skeleton, a skinned mesh at seven levels of detail, pose correctives and face shapes | the same check |
| SMPL and its family | research licence | | not usable here |

Three routes, which do not exclude each other. **The owner chose the first as the road** (2026-11-16, on
body packs: "Only if needed. Zero-dependency remains. But if you can build it up, without relying on
external downloads, add it to handoff"): the engine's own body is built up here, and a pack is for a project
that needs more than it reaches.

1. **The engine's own stylised figure** (no outside data): a skeleton laid out from proportion tables (how
   many heads tall, limb ratios by archetype: child, adult, heroic, squat), given radii and built as a body
   from a skeleton (above). It is rigged by construction and fits the engine's style today. This is the one
   that helps every game soonest. **Built up by steps toward the lifelike, with no outside data:** shape
   parameters (height, build, age, the proportions that differ between bodies) as ranges written in this
   repository from general anatomy; then muscle and fat as shaped volumes along the bones; then hands, feet
   and a face as their own small skeletons. *How far that goes, said plainly:* a believable stylised to
   semi-realistic body is within reach, because proportions and volumes can be written down and measured.
   A photoreal likeness is not: the published body models get their shapes from thousands of scanned
   people, and nothing written by hand or by an agent replaces that data. That last step, if a game needs
   it, is what a pack is for. The owner, told so: "right now, I'm not concerned about photoreal likeness.
   Anyone that wants it will be waiting after M6 and likely for a long while."
2. **A body pack**: an open parametric model installed into a project as data, as a voice pack is. The engine
   reads its mesh, its shape parameters and its rig. It lives on the project's side and never in this
   repository, so zero dependencies hold. This is the route to realism.
3. **Import** a finished character as glTF. This exists.

What an agent needs measured on a figure:

- **Proportions** against ranges for the archetype (head count, arm span to height, where the elbow and the
  knee fall).
- **Balance** of a pose: the centre of mass over the feet.
- **Bends:** how much volume an elbow, knee or shoulder loses when it bends, and whether a twist pinches
  (the "candy wrapper"). These belong with the motion checks.

Skin, eyes and hair are a renderer's work. The cheap, proven skin is **pre-integrated skin shading** (Penner,
SIGGRAPH 2011): one lookup by how the surface faces the light and how curved it is, with no extra passes,
which suits the iGPU. A separable screen-space blur is the richer preset. Both sit under the Fidelity track's
"Subsurface and translucency". A realistic face stays the hardest thing here, and stays the benchmark's last
rung.

## 4. From basic shapes to realistic figures

The rule in the Fidelity track stands: every rung has an integrated-GPU budget, and realism is reached through
presets that scale down, "not through Nanite/Lumen-style brute force".

- **A recipe can be built at any detail.** Because the source is a program, a coarser or finer model is
  another build of the same recipe, not a simplification of a dense mesh. A `detail` level on a recipe
  (and on a figure: joints only, then muscles, then fingers and a face) is the natural handle.
- **Small detail belongs in maps.** Texture graphs exist; normal, detail and displacement maps are on the
  track.
- **Cluster levels of detail** (what Nanite does) are now public knowledge: meshoptimizer ships a small
  open implementation ([write-up](https://zeux.io/2025/09/30/billions-of-triangles-in-minutes/), MIT), and
  the steps are: cut the mesh into clusters, group them, simplify each group, choose by error at run time. It
  could be written here without a dependency. It pays only for scenes of many millions of triangles, which
  recipes do not make; it becomes worth it if dense imported sculpts or scans arrive. After M6.

## 5. Light that mimics ray tracing, without the hardware

**What "Nvidia's one" is taken to mean here:** rays traced by dedicated hardware through a triangle
hierarchy, then path tracing cleaned up by denoisers and upscalers. Two facts bound the choice. The reference
machine is an Intel iGPU (F3), and whether its generation offers Vulkan's ray queries is not confirmed (the
newest Intel integrated chips do; check with `vulkaninfo`). And the vendors' kits are third-party code, which
rule 1 excludes anyway. So no part of the engine's light may depend on that hardware.

What ray tracing buys, and what stands in for each part:

| What it gives | The stand-in | Who does it this way |
|---|---|---|
| **Indirect light that follows changes** | probes refreshed while the game runs, their rays marched through distance fields | SDF-DDGI ([Hu et al., arXiv 2007.14394](https://arxiv.org/abs/2007.14394)): dynamic diffuse light on distance fields, no precomputation, several bounces. Unreal's Lumen by default: mesh distance fields, one merged field for the far scene, and a cache of surface lighting ([Wright, Narkowicz and Kelly, SIGGRAPH 2022](https://advances.realtimerendering.com/s2022/SIGGRAPH2022-Advances-Lumen-Wright%20et%20al.pdf)). AMD's Brixelizer GI: sparse distance fields built at run time in compute shaders, screen probes and a radiance cache ([GPUOpen](https://gpuopen.com/fidelityfx-brixelizer/)). |
| the same, without noise | **radiance cascades**: many short rays near a surface, few long ones far away, merged; nothing accumulated over frames | Sannikov (2023), in *Path of Exile 2*; noiseless in 2D ([arXiv 2505.02041](https://arxiv.org/abs/2505.02041)). In 3D: Split Radiance Cascades ([Freeman, Sannikov and Pitt-Francis, arXiv 2607.20384](https://arxiv.org/abs/2607.20384), July 2026), with a sparse table of world-space probes. **Only its abstract was read: how its rays are traced and what it costs are not checked.** |
| **Close-range bounce and occlusion** | screen space with a visibility bitmask, which lets light pass behind thin things | [Therrien, Levesque and Gilet, 2023](https://arxiv.org/abs/2301.11376); the track's planned GTAO is its predecessor |
| **Reflections of things off the screen** | screen-space reflections first; where they miss, a march through the distance field that reads the probes or the surface cache; then reflection probes | Lumen's software path |
| **Soft shadows that sharpen at contact** | a cone marched through the distance field; contact shadows in screen space | distance-field shadows (Quilez; Unreal) |
| **Glowing surfaces that light the room** | falls out of either method above | |

**Why Specula is well placed:**

- **Assets are recipes.** A mesh-based engine must turn triangles into distance fields. Here a model built
  from distance functions has its field exactly, and every other recipe can be sampled into one when the
  model is built and cached with it.
- **The baked system is most of the way there.** `specula-lux` has a ray caster, probe volumes with
  visibility and relocation, and streaming by cell ([design/GI.md](../design/GI.md)). Moving from baked to
  live is refreshing some probes each frame on the GPU against a distance field, behind the sampling the
  shaders already do.
- **Figures built from skeletons give their own stand-ins.** A skinned character has no fixed distance field,
  but a capsule a bone is one, and the body-from-a-skeleton figure has those by construction.

**A ladder, each rung with its iGPU budget and each usable alone:**

| Rung | What | State |
|---|---|---|
| L0 | baked probes | built (M4) |
| L1 | screen space: occlusion with the bitmask, close-range bounce, reflections, contact shadows | GTAO and reflections are on the Fidelity track |
| L2 | a distance field for every model and one for the scene; soft shadows and occlusion from them | new |
| L3 | probes refreshed at run time through the field: time of day, opening doors, big moving things | new |
| L4 | reflections off the screen through the field | new |
| L5 | radiance cascades in 3D, if this year's work holds on modest hardware | to be evaluated: read the paper in full, then prototype |

Where the hardware exists, it could later serve as a faster tracer behind the same interface. It is never the
only path. **The owner agreed (2026-11-16): vendor-neutral only**, and gave the engine's part: "People will
have different requirements and AI agents will need to straight up tell them on what's possible and what's
not. This engine will only act as a guide in this case." So the engine reports which rungs this machine can
run and at what cost, from what it measured there (as it already reports its start, its frame and its
stalls), and the agent tells the person plainly. It follows the owner's rule of 2026-11-14 that an agent
sets the variables for the machine it is on.

**What cannot be mimicked cheaply, said plainly:**

- mirror-sharp reflections of fine detail that is off the screen;
- caustics and exact refraction (the lava lamp's ray-marched material is a special case, not a general one);
- anything thinner than the field's cells: light leaks through thin walls unless they are made thick enough
  or handled apart;
- fast-moving small lights in large numbers.

**Scoring it without an eye.** An agent cannot judge whether a cheap light looks right. The ray caster in
`specula-lux` can render a slow, exact picture of a scene. Each rung is then scored as a difference from that
picture, as a number, on a fixed set of scenes, and the Fidelity track's contact sheets carry the pictures
for a person.

## 6. What to add, in order

**Small, and for every game. All five are M6's** (the owner, 2026-11-16: "Add all. This engine SHOULD help
you. Not leave you lacking, and dealing with people's complaints"):

1. Roundness as one number on the primitives, and bone chains.
2. The form checks: loose parts, razor edges, faceting at the size seen, smooth turning, big to small form.
3. The form views in sight: clay, normals, curvature, stripes.
4. The engine's own stylised figure from a skeleton, with proportion checks.
5. Shape language in the style presets.

**Toward a true 1.0.0 (after M6):**

6. Creases on subdivision; sculpt steps.
7. The light ladder, L1 to L4, each rung scored against the engine's own exact picture; skin shading.
8. Body packs from open parametric models; hair.
9. Cluster levels of detail if dense imports arrive; radiance cascades in 3D if it holds.

## Not verified

- No prototype: no cost in milliseconds on the iGPU is known for any rung of light or any new handle.
- Split Radiance Cascades and 3DCodeBench were read as abstracts; 3D-DefectBench and the two body models'
  licences from summaries and their own pages. Read the licence of each data file before anything ships.
- Which Intel integrated generations offer ray queries, and whether the reference machine does.
- The shape-language associations are design convention; the sources found are practitioners' guides.

## Three things asked of the owner, and the answers (2026-11-16)

1. **"Don't use Nvidia's one"** was read as no dependence on ray-tracing hardware or on any vendor's kit,
   with Vulkan's vendor-neutral ray queries allowed later as an optional faster path. "You read right.
   Vendor-neutral." And the engine is a guide: it says what is possible on a machine, and the agent tells
   the person.
2. **Body packs:** "Only if needed. Zero-dependency remains. But if you can build it up, without relying on
   external downloads, add it to handoff." The engine's own body, built up here, is the road (section 3).
3. **Which of the five small items go into M6:** "Add all."

## Sources

- 3DCodeBench: [arXiv 2606.01057](https://arxiv.org/abs/2606.01057). 3D-DefectBench:
  [arXiv 2607.10826](https://arxiv.org/abs/2607.10826).
- B-Mesh: [Ji, Liu and Wang, Computer Graphics Forum 29(7), 2010](https://diglib.eg.org/handle/10.2312/CGF.v29i7pp2169-2177).
- Dreams: [Evans, "Learning from Failure", SIGGRAPH 2015](https://advances.realtimerendering.com/s2015/AlexEvans_SIGGRAPH-2015-sml.pdf).
- Anny: [arXiv 2511.03589](https://arxiv.org/abs/2511.03589),
  [NAVER LABS Europe](https://europe.naverlabs.com/blog/anny-a-free-to-use-3d-human-parametric-model-for-all-ages/).
  MHR: [arXiv 2511.15586](https://arxiv.org/abs/2511.15586), [repository](https://github.com/facebookresearch/MHR).
- Skin: Penner, "Pre-Integrated Skin Shading", SIGGRAPH 2011 course; Jimenez et al.,
  [Separable Subsurface Scattering](https://graphics.unizar.es/papers/Separable-Subsurface-Scattering.pdf).
- Cluster levels of detail: [meshoptimizer, "Billions of triangles in minutes"](https://zeux.io/2025/09/30/billions-of-triangles-in-minutes/).
- Lumen: [Wright, Narkowicz and Kelly, SIGGRAPH 2022](https://advances.realtimerendering.com/s2022/SIGGRAPH2022-Advances-Lumen-Wright%20et%20al.pdf).
- Distance-field dynamic diffuse light: [Hu et al., arXiv 2007.14394](https://arxiv.org/abs/2007.14394).
- Brixelizer GI: [GPUOpen](https://gpuopen.com/fidelityfx-brixelizer/).
- Radiance cascades: [Holographic Radiance Cascades, arXiv 2505.02041](https://arxiv.org/abs/2505.02041);
  [Split Radiance Cascades, arXiv 2607.20384](https://arxiv.org/abs/2607.20384).
- Screen-space light with a visibility bitmask: [Therrien, Levesque and Gilet, arXiv 2301.11376](https://arxiv.org/abs/2301.11376).
- Distance functions and smooth blends: Quilez, iquilezles.org.
