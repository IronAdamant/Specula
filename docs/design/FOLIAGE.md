# Foliage: alpha-cutout cards, wind and grass (M5)

M4 left two gaps for foliage: texture graphs had no opacity output, and meshes had no wind term. M5's render
follow-ups lane (2026-11-06) closed both and put real grass on Sepulcra's slab (the owner's look decision: "real
grass blades should replace flat green").

## Cutout

- **Texture graphs** (`specula-model/src/texgraph.rs`) have two more outputs: `opacity` (a grey node, 1 = solid) goes
  into the base colour's alpha, and `alpha_cutoff` (0.05..0.95, default 0.5) makes the set alpha-tested. A cutoff
  without an opacity is refused (`outputs.alpha_cutoff`, `missing_field`). The cutoff is part of the set's hash.
- **Materials** were already alpha-masked on the GPU (glTF `MASK`): `MaterialTextures::alpha_cutoff` cuts fragments
  in the scene (`mesh_frag.glsl`), the sun's shadow (`shadow_mask.frag`) and sight's ID buffer (`ids.glsl`
  `cut_out`). A recipe part with a cutout texture set now takes its cutoff from the set.
- **Mips keep coverage** (`specula_gpu::preserve_alpha_coverage`): each smaller level's alpha is scaled so the share
  of texels at or above the cutoff matches level 0's (bisected, the nearer side chosen). Averaged alpha falls under
  the cutoff and cards thin out and vanish with distance; this is the usual fix (Castano's).
- **Alpha-to-coverage** needs MSAA, which the renderer doesn't have (FXAA is its anti-aliasing): alpha testing with
  coverage-kept mips is the path, and the distance fade (below) is an ordered dither.

## Wind

- **The world's wind**: `world_env {wind: {direction, strength 0..4, frequency 0..4 Hz, gust 0..1, fade_start,
  fade_end, shadow_end}}`, default a breeze (strength 1). It reaches the shaders as two `vec4`s of the frame data (`wind`,
  `foliage`). A world without swaying meshes renders exactly as before, and the world hash ignores the default.
- **A mesh's wind term**: a recipe's top-level `"wind": {amount, height?, painted?}` (`specula_model::WindDef`);
  `amount` is metres of sway at full weight in a strength-1 wind; the weight rises as the square of the object-space
  height to 1 at `height` (default: the model's height), or comes from the vertex colour's alpha (`painted`). It
  rides in the draw record's `ext.w` (flags bits 2-3, amount << 16, height << 24), so no record grew.
- **Deterministic in simulation time**: `frame.glsl`'s `wind_offset` is a bend along the wind plus two sine trains
  rolling across the field along it, under a slow gust envelope, from the frame's time (the engine passes world
  time). The same time gives the same vertices; `foliage_check.py` compares pixels byte for byte.
- **Mirrored everywhere a vertex is placed**: `mesh_vert.glsl`, `shadow_vert.glsl` and `ids_vert.glsl` apply the
  same offset, for `Lit` draws only (other materials' records use those bits for themselves: terrain levels, water
  strength; the ID passes know a `Lit` batch by the `FACES_FOLIAGE` flag), never skinned ones. The sway is in the
  draw's own scale (the model matrix's Y column) and never more than 0.8 of the weighted height; a height-weighted
  tip drops at most a third of its height as it leans (painted weights don't drop). Culling spheres grow by the
  sway in the strongest wind (`Wind::reach`, 1.25 x amount x 4, times the model's scale), and entities' boxes
  sideways by the same in object units (`Built::drawn_bounds`), so sight's projected box holds the exact box from
  the ID buffer.
- **Distance fade**: textured swaying draws (cards) thin out by a 4 x 4 ordered dither between `fade_start` and
  `fade_end` metres from the camera, in the scene and the ID buffer alike (`foliage_faded`), so a faded blade counts
  no pixels. Only the textured pipelines carry the fade's `discard` (they cut alpha-masked texels anyway): a discard
  in the plain opaque pipeline cost every Lit draw its early depth writes, and review R1 measured Sepulcra's scene
  pass about 1 ms slower for it. Untextured swaying draws don't fade.
- **Edge-on cards**: a textured swaying alpha-cut draw scales its texture's alpha by how squarely the card faces the
  camera (`foliage_edge`: 0 at |n.v| <= 0.08, 1 from 0.3), so a card seen edge-on doesn't draw its blades as dashed
  lines; the ID buffer's `cut_out` cuts the same fragments. The shadow pass does the same with the sun's direction
  (`shadow_mask.frag`; masked casters read normals): a card edge-on to the sun would cast its blades as dashed
  lines on the ground. Alpha-cut
  casters (grass) fade in the sun's shadow too (`shadow_mask.frag`, dithered in the shadow map's pixels); opaque
  swaying casters keep their shadow.
- **Shadow distance** (`shadow_end`, metres of view depth; 0 = any distance, the default): swaying draws further
  than that from the camera cast no sun shadow at all (`shadow::plan_shadows` leaves them out of every cascade).
  Far grass shadows are a few texels each, and every caster costs the shadow pass its cutout fragments.

## Draw order and cost

- **Cutout batches draw last among the opaque ones** (`batch::plan_into`: the group key starts with
  `shadow::masked`). A discarding shader can't reject what lies behind it early, but whatever stands in front of a
  card is already in the depth buffer and rejects the card's fragments before they are shaded.
- **Cards are quads**, not thin boxes: an upright `plane` is 2 triangles, lit from either side (the Lit path turns
  a normal that faces away from the camera), where a 4 mm box was 12 triangles with both broad faces shaded (mesh
  pipelines don't cull back faces). Review R1 measured the boxes at +3.8 ms GPU p95 in Sepulcra's busy wave on the
  Intel iGPU.
- **Lit like the ground**: swaying alpha-cut draws shade with their normal bent up towards the sky
  (`mesh_frag.glsl`, `n + 2 * up`, normalised), the usual foliage trick: upright cards lit edge-on by Sepulcra's
  high moon read as dark specks from the game's camera, darker than the stone they grow on.

## Grass on Sepulcra's slab

- `tex_grass_blades` (fourteen thin blades of uneven height and spacing, bent by noise, tapering to their tips),
  `grass_tuft` (three crossed upright quads, 6 triangles) and `grass_patch` (12 tufts scattered by Poisson-disc rules
  into one mesh for a 2 m cell, 72 triangles), category `foliage`.
- `games/sepulcra-app/src/grass.rs` puts a patch on the graveyard's cells and on fragment cells at the rock's edges
  (never on a lane: the dead's paths stay readable), at most 72, picked by a hash of the cell, in four seed
  variants (the GPU instances them); and one on every cell roots take (the flat moss patches of earlier builds).
- `night()` sets a breeze (strength 0.8, 0.45 Hz), the fade (70 to 110 m: beyond the slab's far rim at the game's
  zoom) and `shadow_end` 30 m (the game's camera sits 56 m out: grass shadows only when zoomed in).

## Budgets (style.toml)

`foliage` is a category of Sepulcra's `style.toml` with its own budget: 1500 triangles and 256 KB of textures a
model. Cutout cards cost overdraw more than triangles on the iGPU, so the recipes are tighter: `grass_tuft` 6
triangles, `grass_patch` 240 (40 tufts, its most). `sepulcra_assets.rs` (`grass_patches_cost_six_triangles_a_tuft`)
holds every density to 6 triangles a tuft; `style_check` holds both to the category.

## Proof

`scripts/foliage_check.py`: the cutout seen in the image, the ID buffer (visible pixels well under the projected box)
and the shadow (light gaps along the shadow); wind that differs between two times and nowhere else, byte-identical
at the same time, still at strength 0, and moved in the ID buffer too; the fade emptying the ID buffer; the budgets;
no validation messages and no `sight.packet_inconsistent`. `scripts/grass_check.py` measures the grass's own GPU
cost in the game's window (with and without the `/slab/grass` group, ABAB) and fails above 1 ms of GPU mean (1.5 ms of p95); `m3_exit.py --grass-ab` measures it in the busy and boss waves (the clock paused, every grass patch shown and hidden).
