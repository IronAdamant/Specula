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

## Simulation: rules and numbers (proposal)

Built on 2026-10-13 as `games/sepulcra`: the whole rules engine as a deterministic, headless Rust library (no
rendering yet). Everything in this section is Claude's proposal until the owner says yes. Every number lives in
`data/*.json`, so agents can change it without touching code; the files are validated on load with errors that
name the file, the field and the reason.

### Time and determinism

- **30 ticks per second.** The tick number is the only clock the simulation knows.
- **No floats in simulation state.** Positions are millimetres, HP is milli-HP and ratios are permille, so runs
  match bit for bit on any CPU (F2, F6).
- **Three seeded random streams:** `waves` (spawn points, flyer arcs), `lane_jump` and `drops` (dust per kill). A
  seed plus the player's commands replays a run exactly, and `State::hash()` checks it on every tick.
- **Active pause** is just not stepping. Commands given while paused apply at the current tick.

### The slab (`data/slab_01.json`)

The map is drawn as text: the top face as seen from above, with the cliffs unfolded around it like a box net.
North is up. The west and east cliffs are turned so that their rim column touches the top face.

```text
           00000000001111111111
           01234567890123456789
N5         ----B----------B----
N4         ----|----------|----
N3         ----|----------|----
N2         ----|--#----#--|----
N1         ----|----------|----
N0         ----|----------|----
 0  ------ xx##.##########.###x ------
 1  ------ x###.##########.#### ------
 2  ------ ####.##########.#### ------
 3  ------ ####...########.#### ------
 4  ------ ######.######...#### ------
 5  ------ ######.######.###### ------
 6  ------ ######........###### --#---
 7  ------ ######.###.##.###### ------
 8  ------ ######.#GGGG#.###### ------
 9  ------ ######..GGGG#.###### ------
10  ------ ######.#GGGG..###### ---||B
11  B||||| ....j..#GGGG#.###### ---|--
12  ------ ######.##.###..j.... ||||--
13  ------ ####j.........###### ------
14  ------ ####.####.###..j#### ------
15  ------ ####.####.#####.#### ------
16  ---#-- ####.####.#####.#### ------
17  ------ ####............#### ------
18  ------ ####.####.#####.###x ------
19  ------ x###S####S#####S##xx ------
top:   # fragment  . fissure  j jump point  G graveyard  S gorge spawn  x void
cliff: - rock  | climb lane  # ledge  B cliff-base spawn
```

- **Top face:** 40 × 40 m in cells of 2 m.
- **The graveyard:** 8 × 8 m in the centre, inside a ring fissure.
- **The four gates** are off-centre, so enemies travel round the ring. They are the main wall choke points:
  north `top:10,7`, east `top:12,10`, south `top:9,12` and west `top:7,9`.
- **The centre build spots** are the fragments just inside the ring.
- **Cliffs:** north (two lanes), east (one lane with a traverse) and west (one lane). Each is 15 m high (6 rows of
  2.5 m), and only climbers use them. The ledges (`#`) on a cliff are buildable, but they cover only their own cliff.
- **Gorges:** the south rim has three, where every kind of ground enemy can emerge. A connector fissure (row 17)
  joins them, and a central lane runs straight to the south gate.
- **Lane-jump points** are cracked ledges across a single fragment: `top:4,13` ↔ `top:4,11` (west gorge lane to
  west lane) and `top:15,14` ↔ `top:15,12` (east gorge lane to east lane).
- **The flyer ring:** 32 waypoints on a square 3 m outside the rim, at 4 m altitude. Flyers enter at the middle of
  each side and dive to 1.5 m above the centre of the graveyard.
- **Broken corners** (`x`) give the slab its shattered silhouette.

### Range across faces (precise)

A stone hits an enemy when both of these are true:

1. The 3D distance from the stone's cell to the enemy is within the stone's range.
2. If the enemy is on another face, the stone stands within **3 m** (`edge_reach_m`) of the rim the two faces
   share.

On the top face, that means only the two cell rows nearest a rim (1 m and 3 m from it) can reach down a cliff. A
cliff ledge reaches the top only from its rim row. Two cliffs never see each other. Flyers are in open sky, so range
alone decides.

### Pathing and walls

- **Flow fields.** Ground enemies follow Dijkstra flow fields toward the graveyard, one each for walkers and
  climbers. The fields are recomputed whenever a wall changes, and ties go to the lowest cell id.
- **The owner's rule.** An enemy that has an open route always takes it, however long.
- **Breaking walls.** Only enemies with no open route break walls. They follow a breach field in which each wall
  costs its length plus **8 m**, and break the first wall on it. So they attack the wall on the shortest route once
  broken, and prefer breaking one wall to two when the routes are similar.
- **Where walls go:** top-face lanes only, not gorges, cliffs or the graveyard. A wall can't be placed on a cell an
  enemy stands on or is stepping into.
- **Wall cost:** 15 dust for 300 HP. Removing a wall refunds 70 % of its cost, scaled by the HP it has left.
- **Wall damage per second:** shambler 12, runner 6, colossus 150. Flyers ignore walls.

### Stones

| Stone | Attack | Hits | Tier I | Tier II (+cost) | Tier III (+cost) |
|---|---|---|---|---|---|
| Warding stone | Homing bolt | Ground | 50 dust: 10 dmg / 0.8 s, 7 m | +60: 18 / 0.7 s, 7.5 m | +90: 30 / 0.6 s, 8 m |
| Rune pillar | Pulse on all in range | Ground | 60: 35 % slow for 1.5 s, 2 dmg / 1 s, 5 m | +60: 45 %, 4 dmg, 5.5 m | +90: 55 %, 6 dmg, 6 m |
| Brazier cairn | Lobbed splash | Ground | 80: 16 dmg, 1.8 m splash / 1.6 s, 6 m | +80: 28, 2 m / 1.5 s, 6.5 m | +110: 45, 2.3 m / 1.4 s, 7 m |
| Sky obelisk | Homing bolt | Air only | 70: 22 / 0.9 s, 9 m | +70: 40 / 0.8 s, 9.5 m | +100: 65 / 0.7 s, 10 m |

- **Targeting.** Each stone has a targeting policy in its data: first (least distance left to the graveyard), last,
  strongest or closest. Every stone uses "first" for now. Ties go to the lowest enemy id.
- **Projectiles** are simulation objects (10–34 m/s), so the visuals can draw them in flight. A bolt follows its
  target and fizzles if the target dies first. A lob lands where the target stood, so fast runners can step out of
  the fire.
- **Nature, from wave 5:** stones of tier II and up root the lane cells within 2.9 m of them. A rooted cell slows
  ground enemies by 15 %, and the slow stacks with a rune pillar's.

### Enemies

| Enemy | HP | Speed | Movement | Dust | Tombstones |
|---|---|---|---|---|---|
| Shambler | 55 | 0.8 m/s, 60 % of that on cliffs | Climber | 3–5 | 1 |
| Skeleton runner | 26 | 2.2 m/s | Walker, gorges only. 40 % chance to leap at a jump point (see below) | 2–4 | 1 |
| Wraith | 40 | 2.4 m/s | Flyer: appears on the ring, circles 6–14 waypoints in a seeded direction, then dives | 5–7 | 1 |
| Bone colossus | 1250 | 0.4 m/s | Walker from the middle gorge; summons 2 shamblers every 7 s | 150 | 5 |

- **Lane jumps.** A runner leaps only if the landing is at most 4 m farther from the graveyard, or if the leap is its
  only open route.
- **Enemy HP** scales with the wave, from × 1.0 to × 2.4 in wave 10.

### Economy and waves

- **Dust:** start with 150. Each wave pays its income when it starts: 0, 40, 50, 60, 70, 80, 90, 100, 110 and 120
  dust for waves 1–10. Kills pay as in the enemy table.
- **Calling a wave early** pays 0.5 dust for every second left on the timer.
- **Selling** refunds 70 % of everything spent on the stone.
- **Tombstones:** 10. Win by clearing all 10 waves with at least one tombstone left.

There are 30 s of build time before wave 1. Each wave starts when the previous one's timer runs out, or earlier if
the player calls it. A run lasts about 12 minutes; the reference layout takes 12:07.

| Wave | Timer | HP | Enemies |
|---|---|---|---|
| 1 | 55 s | × 1.0 | 6 shamblers |
| 2 | 60 s | × 1.0 | 8 shamblers, 6 runners |
| 3 | 65 s | × 1.1 | 10 shamblers (cliffs), 8 runners |
| 4 | 65 s | × 1.2 | 8 shamblers, 4 wraiths, 10 runners |
| 5 | 70 s | × 1.35 | 14 shamblers, 14 runners |
| 6 | 70 s | × 1.5 | 8 wraiths, 16 runners, 10 shamblers |
| 7 | 75 s | × 1.7 | 16 shamblers, 18 runners, 4 wraiths |
| 8 | 75 s | × 1.95 | 20 shamblers, 10 wraiths, 10 runners |
| 9 | 80 s | × 2.2 | 26 runners, 16 shamblers, 10 wraiths |
| 10 | until cleared | × 2.4 | the bone colossus, 12 shamblers, 8 runners, 6 wraiths |

### Balance so far

Tuned with `cargo run -p sepulcra --example headless --release -- --layout all --sweep 1000`. The layouts are build
orders in `data/layouts/`.

| Layout | Over seeds 1–1000 |
|---|---|
| Centre-heavy (wardings, a pillar and braziers inside the ring; obelisks from wave 3) | Won 999. Tombstones left: mean 7.4, min 1. The colossus gets through in about 30 % of runs |
| The same without anti-air | Lost all 1000, all in wave 6, mostly to wraiths |
| Full block (walls on all four gates, two stones) | Lost all 1000 in waves 3–6. Walls only buy time |
| No defence | Lost all 1000 in wave 2 |

How it got there:

1. With a flat HP curve, the centre-heavy layout leaked nothing before the boss.
2. The colossus was all or nothing: at 1500 HP it always died, and at 2200 it always got through. The boss walks one
   fixed path, so the seed barely matters.
3. Raising the HP curve to × 2.4 and adding runners to waves 4–9 brought the first leaks in mid-game.
4. Setting the colossus to 1250 HP (3000 effective) means it gets through in about a third of runs.

### Testing (built)

- **43 tests:**
  - map and content validation
  - flow fields and rerouting
  - the wall rule
  - lane jumps
  - flyers against walls and obelisks
  - climbing
  - range across faces
  - targeting
  - economy
  - early calls
  - summons
  - slows and splash
  - roots
  - win and lose
- **The replay suite above** passes: per-tick hash equality, divergence across seeds, journal replay with a JSON round
  trip, and three pinned full runs.
- **Speed:** a full 12-minute run simulates in about 17 ms on one core (release build). A 1000-seed sweep of the
  reference layout takes about 2 s on 32 threads.

### Questions for the owner

1. **Walls:** only on top-face lanes (as now), or on cliff lanes too?
2. **The colossus:** is a boss leak meant to be survivable, as it is now (5 tombstones, about 30 % of good runs), or
   should it be a must-kill?
3. **Edge reach:** 3 m (two cell rows) for range across faces. Too generous, or too strict?
4. **Wraiths:** should rune pillars slow flyers? For now the obelisk is the only answer to them, as written above.
5. **Calling waves early:** 0.5 dust for every second left is small. Should it be a real economy lever?
6. **Breaking walls:** is 8 m per wall a good weight? It makes enemies prefer breaking one wall to two.

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
- **Built (2026-10-13):**
  - The replay suite is `games/sepulcra/tests/replay.rs`.
  - The first balance tool is `examples/headless.rs` (`--sweep N`).
  - Details are in [Simulation: rules and numbers](#simulation-rules-and-numbers-proposal).

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
