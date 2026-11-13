# Export: assets carried to other engines

Status: built 2026-11-14 (lane m5-export). Code: `crates/specula/src/model_export.rs` (the
tool), `crates/specula-gltf/src/export.rs` (the writer). Live check: `scripts/export_check.py`.

## Why

The owner, 2026-11-13: assets are saved as data that can be reused, copied and modified, and
must be "easily exportable and transportable to any other game engines": terrain, 3D models,
2D, art. "I want to make sure it's easy that way."

Two different things make an asset portable, and Specula does both:

1. **The asset itself is plain data in the project.** Nothing an agent or a person made lives
   only inside a running engine or a binary editor format.
2. **A file other engines read.** For 3D that is glTF 2.0: `model_export` writes it.

## What an asset is, and how it travels

| Asset | Its source in the project | To carry it to another Specula project | To carry it to another engine |
|---|---|---|---|
| A 3D model, prop or building | A recipe: JSON text (`assets/recipes/*.json`) | Copy the recipe (and the style it names) | `model_export` writes a `.glb` |
| A creature with a rig and clips | The recipe and its animation set (`assets/anims/*.json`) | Copy both files | `model_export`: the rig as a skin, each clip as an animation |
| A terrain | A terrain recipe (the same JSON, with its sculpt and paint strokes) | Copy the recipe | `model_export`: the terrain as a mesh, its painted layers as vertex colours |
| Foliage, scatter | Recipes (and the texture recipe its cards are cut from) | Copy the recipes | `model_export`: the mesh and its baked cut-out texture; the wind is this engine's shader |
| A model made elsewhere | A `.glb` in `assets/models/` | Copy the file | It is glTF already: copy the file |
| A texture or image | A PNG (`assets/textures/`, `images/`...), or a texture recipe that bakes one | Copy the file | A PNG is a PNG already; a baked set travels inside the `.glb` of the model whose parts are drawn with it (not a terrain's layer textures: below) |
| 2D sprites, tilemaps, vector shapes | PNG images, and world text for where they go | Copy the files, save the world | PNGs as they are; the layout is this engine's world file (text) |
| Sounds, music | A sound recipe (JSON), or WAV / Ogg files | Copy the files | WAV and Ogg as they are; a sound recipe is this engine's synthesis |
| Interface, timelines, gameplay rules, strings | JSON and TOML text | Copy the files | This engine's own formats: text, documented, not another engine's |
| A world (a level) | A world file: text (`world_save`) | Copy the world's folder | Not exported as a scene yet (below) |

Everything in the left two columns is text or a standard file format, kept under the project's
folder, with claims, versions, the journal and undo ([PROJECT_DATA.md](PROJECT_DATA.md)). An
agent a year from now, or another vendor's agent, reads and changes it with a text editor: no
utility of ours is needed to open it. To make a variant, copy the recipe, change its numbers,
build.

## `model_export`

```
model_export {recipe, params?, seed?, lod?, animations?, path?, expect_version?, dry_run?}
```

- **Builds** the recipe with `params` and `seed` exactly as `model_build` does (the model
  library's own build: the same model an entity draws).
- **Writes one LOD** (`lod`, 0 by default) as a glTF binary:
  - positions, normals, vertex colours (`COLOR_0`, linear) and UVs when the mesh has them;
  - one primitive per material, with the preset's metallic, roughness, emissive and glass
    opacity as glTF PBR factors;
  - **baked texture sets as PNG images inside the file**, on the materials that use them: base
    colour (a cut-out's coverage in its alpha, `alphaMode: MASK` with the set's cutoff),
    normal, metallic-roughness and (when the material emits) emissive, pixel for pixel as baked. A textured part's
    vertices are written white: this engine draws such a part from the texture alone, and
    glTF multiplies `COLOR_0` into the base colour;
  - the rig as one skin (a node per bone with its rest pose, inverse bind matrices,
    `JOINTS_0` / `WEIGHTS_0`), and each clip as an animation (`animations: false` leaves the
    clips out);
  - Specula's own data in `extras.specula` (material presets, bone tails, clip loop modes,
    events, curves), so importing the file back loses nothing a recipe build had.
- **Stores it as a binary project file**, `assets/models/exports/<recipe's name>.glb` unless
  `path` says where (a `.glb` under a `models` folder). It goes the way `project_upload` goes:
  checked by kind (it must import as `model_build` imports a `.glb`), claim-checked
  (`/project/<path>`), versioned (`expect_version`), journaled as the caller's, written
  atomically, and undone by `world_undo`. At most 32 MiB, like every binary project file.
- **Answers** with what the file holds (triangles, vertices, materials, bones, clips, texture
  sets and images, bytes), what was carried and **what was not** (`not_carried`), and the
  file's version and content id. The clips listed are the animations in the file: a clip
  with no tracks (a neutral base for additive layers) moves nothing, glTF has no animation
  for it, and the answer says so.
- `dry_run` builds and exports, reports, and writes nothing.
- An export identical to the file already there writes nothing and answers `unchanged` (its
  answer never points at `world_undo`); under another agent's claim on the path it is refused
  as a write would be, like every project write that changes nothing.

glTF's conventions are Specula's (metres, +Y up, right-handed, counter-clockwise triangles), so
nothing is converted and nothing is flipped.

### What is not carried, and why

The answer's `not_carried` list says these for the model at hand; none is silent.

| Not in the file | Why | What to do |
|---|---|---|
| The other LODs | A `.glb` holds one mesh here | Export each LOD you need (`lod`, `path`) |
| The wind (foliage sway, and foliage's edge fade) | It is this engine's shader: foliage looks plainer in another engine | The term and any painted per-vertex weights are kept in the file (`extras.specula.wind`, the `_WIND` attribute): other programs ignore them, this engine reads them back |
| A clip with no tracks | A glTF animation needs at least one channel | Nothing: it moves nothing |
| A terrain's heights and splat layers as data | Other engines' terrain formats differ from each other | The mesh carries shape and colour; the recipe is the source |
| A terrain's layer textures | This engine's terrain renderer blends them over the ground; no part of the mesh is drawn with one | The mesh carries each layer's colour. The texture recipes are in the project; export a model that uses one to get its images |
| An emissive map on a material that does not emit | Nothing would show it | Nothing: the engine draws no emission there either |
| The collider | Engines build their own from the mesh | The recipe's collider is in `model_build`'s report |
| A model that is a `.glb` already | Nothing to convert | Copy the file (`project_read` gives its bytes) |

### Reading the file elsewhere

A vertex colour's alpha is the wind's weight in this engine, never opacity. glTF reads
`COLOR_0`'s alpha as opacity, so the file's `COLOR_0` alpha is always 1 and the weights travel
apart (`_WIND`). Without that, foliage would arrive in another engine with holes where the
wind is weak.

Vertex colours are how untextured recipe models are coloured, so the importing side must use them:

- **Blender:** imports them as a colour attribute; the material's Base Color needs a Color
  Attribute node (the glTF importer adds it when the file has `COLOR_0`).
- **Godot 4:** enable "Vertex Color: Use as Albedo" on the imported material.
- **Unity** (glTFast) and **Unreal** (the glTF importer): the default glTF materials multiply
  the base colour by the vertex colour.

These are the importers' own behaviours, stated from their documentation, not tested here:
this machine has none of those programs. What is tested is the file: it is read back by this
engine's own importer (every recipe of Sepulcra's, every LOD, bit for bit: the mesh, the rig,
the clips' poses, the texture sets' pixels) and by an independent reader in
`scripts/export_check.py`, and the original and the re-imported model are captured side by
side and compared.

## How it runs

1. On the calling agent's protocol thread, the arguments are read and the target path is
   checked to be a model file of the project (before anything is built).
2. The engine thread is asked for the build (`Command::ModelExport`): the model library builds
   or finds it, as for `model_build`, and lends the built model to the caller's thread.
3. The caller's thread writes the `.glb` (`specula_gltf::export_glb`), stages it in the content
   store and commits it through the project store.

What the engine thread does for this tool is what it does for `model_build` and for any
project write, and no more: it asks the model library for the build (which looks at the
recipe's folder for files that changed, as every build request does; the build itself runs
on the library's workers), answers with the build's report, and applies one commit. The
export, the PNG encoding, the file's check and its bytes on disk are never the engine
thread's (AGENTS rules 14 and 15).

## Known limits

- **A whole set per image.** A baked set's images are written at their baked size, as PNG; a
  model with many large sets can pass the 32 MiB a binary project file holds, and the answer
  says so (`too_large`).
- **One model a file, not a scene.** A whole world as one glTF scene (many nodes sharing
  meshes) needs the exporter to hold several meshes. Until then a level is carried by its
  world file between Specula projects, and its models one by one to other engines.
- **glTF only.** FBX and USD are not written: glTF is the open format every current engine
  imports, and writing a second format would double the surface for no asset that glTF cannot
  carry today.
- **Other engines' importers are not run here** (above).
