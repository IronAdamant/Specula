# Design: procedural modelling and the taste loop

Status: designed 2026-10-13, not yet implemented. Tier 1 lands in M2 and tiers 2–4 in M4
([ROADMAP.md](../ROADMAP.md)).

**Goal:** agents author original, on-style, efficient 3D content directly in the engine, with no external modelling
tool required. The owner can still import or hand-edit whatever they like.

This is where a small studio closes the art-team gap. It is also where an agent's taste matters most: generating a
mesh is the easy part, and judging and refining it is the hard part.

## Core idea: the recipe is the source, the mesh is a build product

An asset is a **text recipe**: a short list of operations with parameters, kept in the game repo next to the world
files. The engine builds the mesh from it deterministically, along with its LODs and collider.

```json
{ "name": "lamp_post", "style": "harbour_town",
  "ops": [
    { "op": "lathe",   "profile": [[0,0],[0.12,0],[0.10,0.2],[0.06,0.25],[0.05,3.2]], "segments": 12 },
    { "op": "bevel",   "edges": "sharp", "width": 0.01 },
    { "op": "attach",  "asset": "lantern_small", "at": [0,3.2,0] },
    { "op": "colour",  "palette": "iron_dark" }
  ] }
```

*Why recipes, not raw meshes:*
- **Small and readable.** An agent can read, critique and edit a 10-line recipe. It can't do that with 40,000
  vertices.
- **Editable by intent.** "Make it taller" is a one-number change, and siblings stay consistent.
- **Diffable and mergeable** in git, and journaled and undoable in the engine.
- **Optimisation comes free.** LODs, colliders and budgets are derived on every build, never forgotten.
- **Variation is cheap.** One recipe with a seed and ranges gives 20 distinct but coherent crates.

Imported meshes (glTF) remain first-class. A recipe can `import` one and keep modifying it.

## Mesh kernel

- An **indexed triangle mesh** for rendering, plus a **half-edge mesh** for operations that need adjacency (bevel,
  extrude-faces, subdivision).
- **Normals** use an angle threshold, so faces meeting at a sharp angle get hard edges and gentle ones are smoothed.
  This gives crisp hard-surface objects without manual smoothing groups.
- **UVs:** box and triplanar projection by default. Most stylised materials use triplanar mapping in the shader, so
  many assets need no unwrap at all. Proper unwrapping is a later tool.
- **Deterministic:** recipe + seed always gives the identical mesh (bit-exact), following the
  [determinism rules](DETERMINISM.md). Asset hashes are therefore stable, and caching is safe.

## Operation tiers

| Tier | Operations | Good for | Milestone |
|---|---|---|---|
| **1** | Primitives (box, cylinder, cone, sphere, torus, capsule, plane with subdivisions); transform; **extrude** (2D outline → solid, with optional taper); **lathe** (spin a profile); **sweep** (shape along a path/spline); **loft** (between profiles); **bevel/chamfer**; array and mirror; attach (compose recipes); vertex colour by palette | Architecture, props, furniture, columns, pipes, rails, barrels, vases, signs | **M2** |
| **2** | **Booleans**: robust mesh CSG for hard surfaces (cut windows, doors, sockets) | Buildings, machinery | M4 |
| **3** | **SDF modelling**: shapes as distance functions, smooth union/subtract, noise displacement, then meshed with surface nets or dual contouring and simplified | Rocks, cliffs, terrain features, organic props, stylised creatures | M4 |
| **4** | **Generators**: scatter with rules (Poisson-disk placement, slope and height masks); building/facade grammars (floors → bays → windows); terrain heightfields with erosion; road and path layout from splines | Whole districts, forests, landscapes | M4 |

Notes:
- **Bevel is in tier 1 on purpose.** Real objects never have perfectly sharp edges. A small chamfer catches light and
  makes a primitive read as a crafted object. It is the cheapest large quality win in stylised 3D.
- **Booleans are notoriously fragile** in floating point (slivers, holes, non-manifold results). The plan:
  - exact plane-based CSG with robust predicates for hard surfaces
  - SDF booleans for organic work, where small inexactness is invisible
  - every boolean result validated (watertight, no degenerate triangles) before acceptance

## Materials

- **Palette-first and vertex colours by default.** Each game's style bible defines named palettes, and recipes refer
  to palette names, never raw colours. *Why:* it's consistent across a swarm, needs almost no texture memory (good
  for F3) and suits stylised art.
- **Procedural textures** where wanted: value/gradient noise, Worley (cells), bricks and tiles, wood rings, rust and
  dirt masks by curvature or height. They are either baked to small textures at build time or computed in a shader
  with a budget.
- **Material presets** in the style bible (e.g. "painted_wood", "wet_stone"), with fixed roughness and metalness
  ranges. Agents choose presets rather than invent parameters.

## Optimisation built in (F3)

Every recipe build produces:
- **LODs** by quadric error simplification ([Garland & Heckbert, 1997](https://www.cs.cmu.edu/~garland/Papers/quadrics.pdf)),
  with switch distances from screen-size thresholds.
- **A collider**: box, capsule or convex hull fit, or a simplified mesh for static level geometry. See
  [PHYSICS.md](PHYSICS.md).
- **Budget checks** against `style.toml` (triangles per LOD, palette, scale range, texture bytes). A violation fails
  the build with the measured numbers attached.
- **Instancing-friendly output.** Repeated recipes share one mesh on the GPU.

## The taste loop

Measurable rules catch budget and scale errors. Taste needs a loop. Every asset (or batch) an agent makes goes
through:

1. **Brief:** the task names the style bible entry, purpose, scale, and neighbours ("harbour town lamp post, beside
   these benches").
2. **Silhouette pass:** black-on-white renders at thumbnail size (64–128 px) from 3 angles. If the object isn't
   recognisable as a silhouette, no amount of detail will save it. This is the first and cheapest check.
3. **Value pass:** greyscale render under stage lighting. Do the large shapes separate? Is there a clear focal area?
4. **Colour pass:** palette render, then a **line-up** beside approved siblings and a 1.8 m human for scale.
5. **Critique against the bible:**
   - shape language (e.g. "chunky, rounded, slightly oversized tops")
   - proportions
   - detail density (fewer big shapes beat many small ones)
   - consistency with siblings
6. **Revise** the recipe and repeat. At most N rounds, then submit anyway with the critique attached.
7. **Review queue:** the owner sees a contact sheet of the batch, approves or rejects, and notes go back to the agent.

The **style bible** is a per-game document: palette, shape language, proportions, material presets, reference images
and banned patterns. The owner writes it with Claude at the start of a game. Every art agent's brief points to it.
This is how one person's taste scales across a swarm.

## What procedural modelling won't do well

**Realistic human faces and characters, and complex organic sculpts,** are poor fits for recipes. For those:
- import from Blender, sculpting tools, scans or asset stores (glTF)
- later, a **parametric character base**: one base mesh plus morph targets and proportion sliders, so agents can make
  varied characters

The art direction should play to the strengths. Stylised, readable and shape-driven art suits agent swarms and
low-end hardware alike.

## Tools (sketch)

| Tool | Does |
|---|---|
| `asset_build` | Build a recipe → mesh, LODs, collider, report (triangles, bounds, budget results) |
| `asset_preview` | Silhouette, value, colour and line-up renders on the stage world, as one contact sheet |
| `asset_check` | Run `style.toml` checks; return violations with measured values |
| `asset_submit` | Send to the review queue with captures and critique notes |
| `asset_variants` | Build N seeded variants of a recipe as a contact sheet |

## References

- Garland & Heckbert, "Surface Simplification Using Quadric Error Metrics" (SIGGRAPH 1997).
- Ju, Losasso, Schaefer & Warren, "Dual Contouring of Hermite Data" (SIGGRAPH 2002), and Gibson, "Constrained
  Elastic Surface Nets" (1998), for meshing distance fields.
- Inigo Quilez's articles on signed distance functions and smooth minimum (iquilezles.org).
- Bridson, "Fast Poisson Disk Sampling in Arbitrary Dimensions" (SIGGRAPH 2007 sketch).
- Müller, Wonka, Haegler, Ulmer & Van Gool, "Procedural Modeling of Buildings" (SIGGRAPH 2006): shape grammars.
