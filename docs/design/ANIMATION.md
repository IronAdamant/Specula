# Design: skeletal animation

Status: researched 2026-10-13. A1–A2 built in M2; **A3, agent-authored motion, motion checks and animation sight
built 2026-10-18** (see [A3 as built](#a3-as-built-2026-10-18) at the end); **A4 (ragdolls and powered ragdolls)
built 2026-10-18** on physics P4 (see [A4 as built](#a4-as-built-2026-10-18)).
Foundation **F4** ([ROADMAP.md](../ROADMAP.md)).

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

## A3 as built (2026-10-18)

What exists, where, and the decisions behind it. Code: `crates/specula-anim` (`graph`, `play`, `ik`, `checks`,
`film`, new `gen` kinds) and the engine's `crates/specula/src/anim.rs` and `anim_tools.rs`.

### Graphs as data (`graph.rs`)

Graphs are strict JSON (like every anim format: unknown fields, names and impossible values are errors with a path
and a code), kept in an anim-set file under `"graphs"` or written live as world assets. A graph has:

- **parameters**: `float` (range, default), `bool`, `trigger`, `enum` (values), `vec3` (IK and look-at targets);
- **nodes** (blend trees): clips, `blend1d`, `blend2d` (gradient band interpolation), `{base, add}` additive,
  and `{external: true}` (see A4 below); children play in step by normalised phase;
- **states** in **layers** (the base layer, plus override or additive layers with a weight and a mask): each plays a
  node at a speed or with `match_speed` (the rate that keeps feet planted at a ground-speed parameter), with
  `root_motion`, per-state IK weights and an `on_enter` event;
- **transitions**, tried in order: from a state, a list or `*`; conditions as short strings (`"act == move and
  speed > 0.1"`, `"hit"`), an exit phase (`"end"`), a blend time, `keep_phase`;
- **IK** (`feet`, `two_bone`, `look_at`), **joint limits** and the bones the checks treat as **feet**.

### Plays as pure functions of time (`play.rs`)

The decision that shaped everything: the engine already evaluates clips as a pure function of render time, so a
capture at a time always shows the same pose. Graphs keep that. A **play** (an entity's `anim` in graph mode) is data:
parameter values, the graph clock at an anchor and, per layer, a short blend stack (state, phase, crossfade). Its pose
at any time is computed by running the state machines forward from the anchor with constant parameters: conditions
fire at once, exit times at the moment the phase reaches them. Periodic chains of any length (attack → idle →
attack) are detected (the last firing of every transition is kept) and skipped whole periods at a time, so the cost is
bounded however far the time is from the anchor (`MAX_STEPS` otherwise, reported as `truncated`). Zero-time
loops (`a → b when on`, `b → a when on`: transitions without an exit time or a trigger around a loop of states, whose
conditions can all hold together) are refused at parse with the transitions named (`InstantCycle`); past the check's
work budget the resolver still stops one: an instant that fires more transitions than the layer has states is undone
(settling stays idempotent, never a flip) and the layer reports `truncated`. Blend trees are bounded at parse time by
their real depth and clip count (named nodes reused under each other count every use; at most 64 clips).
**Writing parameters settles the play** at the write's time (the world does it in `World::update`): blends stay
continuous, fired triggers are consumed, and parameters not given keep their values. The anim set a write settles
against never comes from the mesh build: the library builds it on the spot from the recipe's rig (its parameters,
`let`s and bones; no geometry) and anims file, or a glTF's skeleton and clips (5-9 ms the first time for a Sepulcra
creature in release). The set is shared by every build key with the same rig and anims file (seeded and parameter
variants), and the world's anim assets as of the write are laid over it once per asset revision; the cache holds at
most 64 sets and 64 MiB of clips. A glTF file changed on disk since the library last polled it isn't used until the
next poll. Clip generation runs on whatever thread loads a set, the engine's included, so it has a deterministic
budget in frames × bones: 24,000 per generated clip and 160,000 per set (overlays included; Sepulcra's largest set,
the colossus, is about 11,000), checked before any work in anims files, overlays and `anim_set` alike, with a `Limit`
error at the clip's path.
Nothing waits, and nothing depends on whether or when a build lands: the library's view of the files changes only
when the engine polls them, and a changed anim set (hot reload, `model_reload`) reaches the world right there, as
journaled re-settles of the plays it touches (by `anim_reload`). Without an anim set (no rig, a rig or anims file that
fails to load) parameters still merge. There is no per-frame state, so captures, replays and the window agree.
`root_motion` integrates along the stacks of one resolve.

### Determinism: what is simulation, what is cosmetic

- **Deterministic** (exact `f64` time, IEEE basic operations, `dmath`): state resolution, events, and
  `play::root_motion(t0, t1)`; sampled at simulation ticks they can feed a simulation (a character controller's
  desired velocity). Plays are hashed and journaled like every component.
- **Cosmetic**: render-time poses with IK (feet planted on the ground the renderer finds, look-at). Nothing in a
  simulation reads them. Sepulcra's simulation stays headless; its scene sets parameters.

### IK (`ik.rs`)

Analytic two-bone IK (keeps the animated bend plane; optional end rotation), feet planted on a `Ground` (each foot
keeps its animated height above the real ground under it, the pelvis moves by the lowest foot's offset), and look-at
shared along a chain, clamped. The engine's ground is the tops of entities tagged `ground`, with soft edges so a
foot crossing a step eases over it; physics queries can replace that behind the same trait.

### Agent-authored motion (`gen.rs`, tools)

New generator kinds: `keys` (poses at times, Catmull–Rom; feet planted when legs are given), `pose`, `recoil` (snap
then damped spring, for additive hit layers), `breathe`, `chain` (spring-chain secondary motion baked over a source
clip, seamless loops: tails, shrouds, cloth strips), and `"body": "auto"` (a gait from a rig's bone names). Gaits
now lift and land feet in place (`clear`) with the toe up, found by the foot-sliding check (the shambler's walk went
from 16 to 0.08 cm/s). Agents write clips and graphs with `anim_set`: they become Group entities tagged `anim_asset`
under `/anims/<source path with / as :>/` (`/anims/recipes:shambler.json/graphs/creature`, so two sources sharing
a file stem never share a folder; writing over another source's asset is refused), so they are journaled,
claimable, undoable and saved, and the library lays them over the recipe's own set for every entity of that model. *Limit:* props hold 16 KiB, so long keyframed clips should be
generator specs.

### Motion checks as data (`checks.rs`)

Foot sliding (soles within 2.5 cm, weighted `2 − 2^(h/H)`, per foot), floating (for play-once subjects such as
deaths and leaps: how high the body rests at the end), ground penetration (skinned vertices), joint limits, pops at
transitions (excess per-frame joint jump, scaled to a 1.8 m figure), root drift, loop seams and, for flyers, a hover
band (`[motion.hover]`: the body's lowest point stays within it; feet checks don't apply) and, for leaps, the
root's apex against the height the generator was given (`leap_apex`), for clips, graph states held with parameters
and parameter changes. Thresholds and precise exemptions (`"rig/subject" = ["check", "check:bone"]`) come from the
game's `style.toml` `[motion]` table; a value over its limit is an error. A graph state takes the exemptions of the
clips it plays; a state off its feet (feet IK off) still has its feet checked when its clips travel (a climb, a leap),
and checks a subject skips are listed (`skipped`). Filmstrip labels say both: "CHECKS OK (EXEMPT: ...; SKIPPED:
...)". On Sepulcra the checks found real defects, now
fixed: the runner hovered 7.8 cm, the colossus's hands went 33 cm into the slab walking and 2 m in its death, the
shambler's death sank 73 cm and the wraith's shroud dipped 15 cm under the floor in its swoop. Self-intersection is
not checked yet.

### Animation sight

`anim_filmstrip` renders contact sheets (rows of clips, states or transitions on a neutral stage, or an entity's own
play in the world) with each row's checks attached, saved like captures with a packet. Sheets stay within 2048 px a
side (tiles shrink) and 128 tiles a call; a stage sheet frames each row on its own and labels a row framed wider
than the closest one with its scale; graph-state rows put the root's rise back (root motion is taken out of their
poses), so a leap leaves the ground. Entity sheets add a front view of the feet from above knee-high clutter, light
each view from the camera, warn about tiles too dark to judge
(mean luma under 0.1 in the subject's box) and report `feet_in_world`: per frame the ground under each foot, the
ankle's height over it after the IK against the animation's, and a `feet_on_ground` check (non-zero only when a
leg can't reach where the animation meant the foot to be). The `filmstrip` and
`motion_checks` examples do the same offline.

### Tools

`anim_get`, `anim_set`, `anim_state`, `anim_check`, `anim_filmstrip`; parameters are written with `world_set {anim:
{graph, params}}`.

### The seam for A4 (ragdolls)

- `{external: true}` nodes play a pose supplied from outside (`play::Surroundings::external`): a ragdoll's pose from
  physics joints becomes a graph state, and transitions blend to and from it like any other ("any → ragdoll when
  dead", "ragdoll → get_up when grounded").
- Powered ragdolls drive their joint motors towards the graph's own pose: `GraphPlayer::evaluate` gives the target
  local rotations at any time (pure, so physics can ask at its ticks).
- Graph `limits` (Euler ranges per bone) are the joint limits a ragdoll's constraints take.
- What was still needed (done, below): physics P4 joints, a model-space → local pose conversion for the physics pose (as
  `gen::deltas_from_globals` does), and the engine supplying the external pose per entity.

## A4 as built (2026-10-18)

Code: `crates/specula/src/physics/ragdoll.rs` (the plan, live ragdolls), `crates/specula/src/ragdoll_tools.rs`
(`ragdoll_set`, `ragdoll_info`), `specula_anim::checks::check_frames`, the graph's `ragdoll.radius`, and the renderer
reading the world's external poses (`World::external_pose`, `models.rs`). Joints are physics P4
([PHYSICS.md](PHYSICS.md#p4-as-built-2026-10-18)).

- **Data.** A model entity's `props.ragdoll` turns it on: `{mode: limp | powered | off, strength, mass, relocate,
  radii, hit: {bone, impulse, seq}}`. World data, so journaled, undoable and claimable for saved entities, and games
  write it for their transient presentation. `ragdoll_set` writes it and the graph's bool `ragdoll` parameter in one
  change (`fit: true` also measures the radii).
- **The plan** (pure, deterministic, unit-tested): every bone at least 5 cm long except the root gets a capsule along
  its segment; shorter bones ride with their parent as the pose had them. Each part is joined to its nearest ancestor
  part at its head: a **hinge** where the graph's `limits` give one axis (knees), else a **ball joint** twisting about the
  bone (twist from the limit on the axis the bone lies along, else ±30°) inside a cone (from the other limits, else
  50°). Frames come from the rest pose, so joint angles are rest-relative like the limits. Mass (40 kg × scale³ by
  default) goes three quarters by length, the trunk (the parts from the pelvis to the last branching part) weighing
  four times a limb of its length, and a quarter shared equally (no part many times lighter than its neighbour: big
  mass ratios make chains soft). Joint friction (0.6 N·m per carried kg) and damping let it come to rest instead of
  jittering. Parts of one ragdoll never collide with each other.
- **Radii.** From the graph's `ragdoll: {radius: {bone: m}}` (model metres), over by `props.ragdoll.radii`, else a
  fifth of the bone's length (a branching trunk bone a third of how far apart its branches start). `ragdoll_set {fit}`
  measures them from the built skinned mesh (the 95th percentile of how far each bone's vertices lie from it, the
  farthest for end parts) and writes them as data: the mesh build is asynchronous, data isn't. A fitted capsule spans the
  bone's whole segment. *Found by the motion check:* with guessed radii the shambler's hump sank 50 cm into the ground;
  fitted, 0.05 cm. Sepulcra's shambler graph carries its fitted radii, the feet thinner than measured (a capsule as wide
  as the toes' reach sinks the soles the walk plants; at 0.08 m the toes sank 3.6 cm into the slab in play, at 0.1 m
  1 cm).
- **Limp** (deaths, falls): starts from the play evaluated at the tick's time without IK and without its ragdoll state
  (which would play this very pose), each part moving on at the velocity the animation had (the pose a tick on); a
  `hit` knocks a bone once per `seq` (a bone without a body passes it to its parent's). That velocity comes from the
  graph's **base layer only**: other layers (reactions) stay in their entry states. *Found in play:* Sepulcra's killing
  blow sets off the flinch layer in the same tick (its attack turns the chest 16° in 0.05 s), which flung the shambler's
  hands up at 16 m/s and failed the pops check (24 cm a 30 Hz frame); a test reproduces it. A joint the starting pose
  already holds past its limits has the limit opened to there (`joint::open_to`, 3° inside) rather than being snapped
  back inside. The engine records the pose each
  tick (150 frames) and sets it as the entity's external pose; the graph's `{external: true}` nodes play it, so
  `any → ragdoll when ragdoll` blends into it.
- **Powered** (hit reactions): the pelvis and the legs follow the animation as kinematic bodies (planted where it
  plants them; dynamic legs fought the ground wherever the raw animation, before feet IK, dips into it), and each upper
  body joint's motor drives towards the animation's pose at the end of each physics tick (`GraphPlayer::evaluate`, pure,
  so deterministic) at 60 Hz with a torque cap of `strength` × 25 N·m per kg it carries. The renderer draws it as it is
  (it already follows the animation). Measured: within 10° of the walk at every joint; a 21 N·s blow to the shambler's
  reaching arm (strength 0.6) knocks it 9° further off, and it recovers within 1.5 s.
- **Off** (getting up): with `relocate` (default) the entity moves under the pelvis, the last pose stays the external
  pose for 2 s (so the graph's transition out blends from where the body lies), and the parts go.
- **Saved vs transient.** Saved entities' ragdolls are simulation (hashed; twin runs step the same). Transient ones are
  cosmetic (they land on static colliders but never touch the simulation or its hash). Like warm starts, ragdoll bodies
  aren't saved in world files: a loaded world's ragdoll starts again from the animation.
- **Motion checks on ragdolls.** `check_frames` runs ground penetration, where the body comes to rest (`floating` at
  the end), joint limits and pops where physics takes over (the first frames' change of per-frame motion; later
  changes are impacts, and per frame at the rules' `sample_hz`: a change of motion over a 30 Hz tick is four 60 Hz
  frames' worth) on the recorded poses; `ragdoll_info` reports them with the game's `style.toml` thresholds, measured
  by the skinned mesh when it is built. The shambler's limp death passes them all, in `joints_check.py` (penetration
  0.05 cm, floating 1.45 cm, pops 2.9 cm) and in play, dying on a Sepulcra path (penetration 1.0 cm, floating 0,
  pops 0.7 cm), against Sepulcra's 3, 2.5 and 6 cm.
- **Limitation: round capsules.** The shambler slumps into a crouched heap on its knees and hands rather than lying
  flat: its trunk capsules (radius 0.63 and 0.6 m in model metres, the 95th percentile of a wide, hunched coat) are
  round where the body is not, and prop it up. Shifting a capsule into the hump (fitting an offset across the bone)
  slimmed the chest to 0.52 m but left the heap rocking and sliding at 1 m/s after 2.5 s, so it was not kept. The fix
  is parts shaped like the mesh: convex hulls per bone (physics already has hull and compound bodies), fitted by
  `ragdoll_set {fit}`.
- **Sepulcra**: shamblers die as ragdolls (knocked back from where they faced, `props.ragdoll` and the `ragdoll`
  parameter set together, the play otherwise left as it was: a death clip blending in at the same tick would set the
  takeover's velocity) onto the slab, whose ground pieces got box colliders; the other creatures keep their death clips
  (the runner's bones scatter, the wraith fades, the colossus crumbles: not falls).
