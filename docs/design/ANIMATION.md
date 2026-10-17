# Design: skeletal animation

Status: researched 2026-10-13, not yet implemented. Foundation **F4**. Stages A1–A2 land in M2 and A3–A4 in M3
([ROADMAP.md](../ROADMAP.md)).

**Goal:** characters and creatures that move well, are cheap on low-end GPUs, deterministic where gameplay depends on
them, and checkable by agents.

## How skeletal animation works

- **Skeleton:** a model has a tree of **joints** (bones): hip → spine → chest → shoulder → elbow → hand, and so on.
- **Inverse bind matrix:** each joint has one. It moves a vertex from model space into that joint's space, as the
  mesh was in its bind pose (usually a T-pose).
- **Weights:** each vertex is bound to **up to 4 joints** with weights that sum to 1.
- **Clip:** a set of keyframed tracks, e.g. "walk" or "attack". Each track animates one joint's translation, rotation
  (a quaternion) or scale over time.

Every frame:
```
1. Sample clips at time t       → local pose per joint (T, R, S)
2. Blend poses (crossfade, layers, additive)
3. Post-process: IK, look-at
4. Local → model space          (walk the tree parents-first: global = parent_global × local)
5. Skin matrices                (skin[j] = global[j] × inverse_bind[j])
6. GPU: vertex shader           (position = Σ weight_i × skin[joint_i] × bind_position)
```

This matches the glTF 2.0 model. A `skin` lists its `joints` and an `inverseBindMatrices` accessor. Vertices carry
`JOINTS_0` and `WEIGHTS_0` attributes (4 influences per set), and the shader blends the joint matrices by weight
([Khronos glTF tutorial: Skins](https://github.khronos.org/glTF-Tutorials/gltfTutorial/gltfTutorial_020_Skins.html)).
glTF is our import format, so this is the natural design. The exact accessor rules are checked against the normative
spec when implementing.

## Stages

| Stage | Contents | Milestone |
|---|---|---|
| **A1** | glTF skins and clips import; clip sampling (STEP, LINEAR, CUBICSPLINE); GPU linear-blend skinning; play/loop one clip | M2 |
| **A2** | Crossfades and layered/additive blending; masks (upper body only); clip events (footstep, hit frame); filmstrip sight | M2 |
| **A3** | State machines and blend trees as text data; root motion; two-bone IK (feet on ground, hands on handles); look-at | M3 |
| **A4** | Ragdolls (physics P4 drives joints) and powered ragdolls blending to animation | M3 (moved from M4, owner 2026-10-17) |
| M3 | Agent-authored clips and procedural motion as data; animation sight checks (foot sliding, penetration, pops) | M3 (owner 2026-10-17) |
| Later | Dual-quaternion skinning (no "candy-wrapper" twist), retargeting between skeletons, morph targets (faces), animation compression | — |

## Key details

- **Rotation blending:** use normalised linear interpolation of quaternions (nlerp) for blending. Pick the shortest
  hemisphere: flip the sign if the dot product is below 0. Use slerp only where the angle is large. nlerp is cheaper,
  and order-independent when blending many poses.
- **Skinning on the GPU:** joint matrices go in a storage buffer, one region per character instance. The skinned vertex
  format adds `joints: u8x4` (or `u16x4` beyond 256 joints) and `weights: unorm8x4` or `f16x4`. A second pipeline is
  needed alongside the static one, and its layout is reserved in M1/M2 (F4).
- **Tree order:** joints are stored parents-first, so step 4 is a single linear pass with no recursion.
- **Animation as data:** state machines and blend trees live in text files next to the world, e.g.
  ```
  states: idle, walk, run, jump
  blend:  locomotion = 1D(speed: idle 0, walk 1.5, run 5)
  transition: any → jump when jumped, crossfade 0.1 s
  ```
  Agents edit these like any other world file, with diffs, the journal and undo. Game logic only sets parameters
  (`speed`, `grounded`, `jumped`).
- **Root motion:** optionally take character movement from the clip's root joint instead of game code. This avoids
  foot sliding for scripted moves. It feeds the character controller as a desired velocity (see the physics design).

## Determinism and performance

**Two kinds of animation:**
- **Display animation:** what you see. It runs per render frame with interpolation and needn't be deterministic.
- **Gameplay animation:** hitbox positions, root motion and event timing. It is sampled **at simulation ticks**, uses
  `specula-core`'s deterministic maths, and is journaled like everything else.

  A sword's hitbox at tick 1234 is therefore identical on every replay ([DETERMINISM.md](DETERMINISM.md)).

**Low-end costs (F3):**
- Skinning stays on the GPU.
- Sampling is cheap with keyframe reduction and quantised rotations (compression, later).
- Distant characters update at lower rates; for example, beyond 30 m they animate at 15 Hz.
- Joint-count and influence budgets live in `style.toml`, e.g. "≤ 64 joints for crowd NPCs".

## Agent-facing tools

- **Filmstrip sight:** `anim_preview` renders one image with N poses across a clip, side by side, with the skeleton
  overlaid.
  *Why:* a model judges motion far better from a strip of frames than from a single frame.
- **Measured checks**, returned as data:
  - **Foot sliding:** foot speed while the foot should be planted, in cm per tick.
  - **Joint limits:** e.g. elbow bending backwards.
  - **Self-intersection:** mesh penetrating itself.
  - **Ground penetration.**
  - **Loop seam:** the pose difference between the first and last frame.
- **Clip events listed with times**, so agents can sync footsteps and hits.
- **Agent-authored clips** (later): agents keyframe joints through a tool for simple procedural motions (doors,
  mechanisms, idle sways). These are checked with the filmstrip and the measured checks.

## Where assets come from

- **Import:** glTF from Blender, asset stores or motion-capture libraries. This is data, not a dependency, so it's
  fine for zero-dep.
- **Generated:** procedural motion for mechanisms and simple creatures (sine-driven tails, IK-driven legs on uneven
  ground). Rich humanoid motion still comes from imported or captured clips. Being realistic about that keeps
  expectations honest.

## References

- Khronos, glTF 2.0 specification (skins, animations) and the
  [glTF tutorial on skins](https://github.khronos.org/glTF-Tutorials/gltfTutorial/gltfTutorial_020_Skins.html).
- Jason Gregory, *Game Engine Architecture*, animation chapter: pipeline, blend trees and compression.
- Kavan et al., "Skinning with Dual Quaternions" (2007): for the later dual-quaternion upgrade.
