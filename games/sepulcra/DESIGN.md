# Sepulcra: game design doc (draft)

Latin for "tombs" (name confirmed by the owner, 2026-10-13). Status: **draft, 2026-10-13**. Chosen as Specula's first
game and test bed in [docs/GAME_SELECTION.md](../../docs/GAME_SELECTION.md). Lines marked **(proposal)** are Claude's
suggestions and still need the owner's yes. Lines marked **(open)** are unanswered questions.

## Pitch

You are a magician guarding a graveyard of carved stones on a shattered slab of land. Necromancers send undead to
raise the dead. They come across the top and up the cliffs, jump between lanes to slip past you, and fly around the
edges. You carve stones that fight, wall off paths to force new routes, and rotate the slab to watch every side.

## Pillars

1. **Think in three angles.** The top and the sides of the slab are all battlefields. The centre is easy to hold and
   the edges are hard, because a stone near an edge can't watch both faces.
2. **Plan, then adapt.** Walls reroute enemies, and enemies adapt by jumping lanes or flying. Good play is planning a
   layout the way Factorio players plan a factory.
3. **Carved stone, growing nature.** Towers are stones you carve. More runes mean more power, and later nature grows
   over them to help.
4. **Readable at a glance.** You always know which face is under threat, even when it's out of view.

## Platforms and scope

- **Windows + Steam Deck** for v1.0 (owner, 2026-10-13). **Mobile later.** Tower defence suits touch well, so the
  controls are built on input actions (tap to place, drag to orbit) and nothing assumes a mouse.
- Single-player. English first; other languages are **(open)**.
- **MVP win condition:** survive 10 waves.

## The slab

One shattered slab of land that the player rotates around (owner, 2026-10-13).

**Confirmed by the owner (2026-10-13):**
- **Layout.** The graveyard sits in the centre of the top face. The slab is cracked into fragments, and the
  fissures between them form the lanes. Enemies come from below and the rim: up the side faces (cliffs), across the
  top towards the graveyard, and through the air around the edges.
- **Coverage.** A stone has range across faces only near an edge, so placing it there is a trade-off between
  covering the top and covering the side. Stones in the centre cover every approach on top, but only at the last
  moment.
- **Size.** For the slice: a top face of about 40 × 40 m, with 2–3 climbable side faces about 15 m high.

## Core loop

- **Every 30 seconds:** spot a threat on a face, then carve, upgrade or wall to answer it. Rotate to check the faces
  you can't see.
- **Every wave (2–3 min):** enemies probe your layout. Lane-jumpers and flyers expose its weak spots, and you patch
  them between waves.
- **Every run (about 12 min for the slice):** 10 waves, ending with a boss wave.

**(proposal)** You can build at any time, and pausing is allowed (active pause, as in Factorio), because it's a
planning game first.

## Stones (towers): (proposal)

You upgrade a stone by carving more runes into it (tiers I–III, visible on the mesh).

| Stone | Role | Notes |
|---|---|---|
| Warding stone | Single-target bolts | The basic stone |
| Rune pillar | Slows an area | Strong at lane-jump points |
| Brazier cairn | Splash fire | Good against packed shamblers |
| Sky obelisk | Anti-air | The only answer to flyers in the slice |

**Nature (from wave 5)**: roots grow from carved stones and slow anything walking near them. This is the first taste
of "nature assists the stones".

## Walls and rerouting

You can place low stone walls on lane cells to force enemies onto a new route.

**Rule (owner, 2026-10-13):** enemies always take an open route if one exists, even a long one. Only when walls
leave **no** route do they attack walls to break through. **(proposal)** They attack the wall that sits on the
shortest route once broken, so a full block buys time but always costs a wall.

## Enemies (slice)

| Enemy | Movement | Twist |
|---|---|---|
| Shambler | Ground, slow, tough | Climbs cliffs |
| Skeleton runner | Ground, fast | **Lane-jumper:** jumps to a neighbouring lane at marked jump points, with a seeded chance |
| Wraith | Flies | Ignores walls and circles round the edges |
| Boss (wave 10) **(proposal)** | Bone colossus | Slow and huge; it spawns shamblers |

**(proposal)** Lane-jump points are visible in the world (cracked ledges). That way the chance to jump is something
you plan for, not a coin flip you lose to.

Demons come after the MVP.

## Economy: (proposal)

Slain undead drop **grave-dust**. You spend it on carving, upgrades and walls, and selling refunds 70%. Each wave
has a fixed income, plus whatever you kill.

## Lose condition

Each necromancer who reaches the graveyard raises the dead from one tombstone. If you lose all the tombstones (for
example 10), the run is lost.

## Camera and controls

- **Mouse:** orbit by dragging the right button, zoom with the wheel, click to place.
- **Gamepad (needed for Deck Verified):**
  - The bumpers snap the camera to the next face, and the stick orbits.
  - A cursor snaps to placement cells.
  - A radial menu builds stones.
- Design for the gamepad from day one; bolting it on later is a known pain point for tower defence games.
- **Threat indicators:** arrows at the screen edge for faces under attack that are out of view.

## Art direction

**(open)** This is written with the owner in the style bible: palette, shape language, proportions and material
presets. It must suit procedural modelling (stone, bevels, runes, roots) and integrated GPUs.

## How we test it (headless, deterministic)

- Waves, lane jumps and drops all use the engine's seeded RNG, so a seed plus the player's inputs replays exactly.
- **Replay suite:** scripted layouts must end the run with known outcomes and hashes, for example "centre-heavy
  layout survives wave 10" and "no anti-air loses by wave 6".
- **Balance swarm:** many headless engines, each played by an agent trying a strategy. A dominant or useless stone
  shows up in the data.

## What it needs from the engine (Phase B)

| Lane | Needed for the slice | Not needed yet |
|---|---|---|
| Foundation | All of M1 (saves, claims, journal, determinism, `perf_frame`) | — |
| Render | Materials, shadows, post, **instancing** (hundreds of undead), sky/fog | GI, terrain streaming |
| Sim | Module host, SDK, input actions + gamepad, **mouse picking** (P1 ray queries), **graph pathfinding** on lane cells with rerouting, 3D flyer paths, active pause, `time_scale` | Character controller, rigid bodies, navmesh |
| Anim | A1–A2: skinning plus a few clips per enemy | Blend trees, IK |
| Content | Procedural tier 1 (stones, walls, slab), SDF slab in tier 3 if ready; stage + contact sheets | Building generators |
| Interface | HUD (dust, wave, tombstones), radial build menu, text T1–T2, WAV audio | Dialogue, localisation |
| VFX | **Pulled forward:** bolts, fire, roots, wraith trails | — |
| Swarm | Task board, playtest swarms, balance runs | — |

## Slice: the Phase B gate

- One slab, the 4 stones, walls, 3 enemies plus the boss, and 10 waves in about 12 minutes.
- Playable with a mouse and with a gamepad.
- Within budget on the integrated GPU, with a headless replay suite passing.

## After the MVP (not now)

Demons, deeper nature progression, more slabs, story framing of the necromancers.
