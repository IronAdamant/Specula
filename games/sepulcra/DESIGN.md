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
- **Props** (M5; the owner's decision of 2026-11-06): the 33 graveyard props the owner approved in M4 are map data
  (`props`: name, `what`, `at`, `yaw`, `blocks_build`, `cells`; `src/props.rs`). Big pieces block building on every
  cell they cover (`prop_blocks`, "a grave angel stands here"): the mausoleum, statues, obelisks, the tall lanterns,
  tombs, columns, the gate, the crypt door, the fallen pediment, the candle shrine, the bell, the stone bench and
  the iron fences: 40 fragments in all, each prop's cells checked against its mesh and the widest stone (`games/sepulcra-app/tests/prop_cells.rs`). Small dressing (mounds, tools, the wheelbarrow, coffins, urns, candles, the
  wreath, headstones and markers, bones, the tripod brazier) doesn't; it is hidden while a stone stands on it.
  Props stand only on fragments, so walls (lanes) never meet them and no path changes. The corner fence moved
  1.4 m from its approved place to `top:11,15`, so it no longer blocks the build spots beside the middle lane.

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
- **Wall damage per second:** shambler 12, runner 6, colossus 150. Flyers ignore walls. Every ground enemy must
  deal some wall damage (at least 0.1 HP/s), or a full block would stall the game.
- **Safety net:** an enemy with no route at all, even through walls, is removed after 5 s (a `Stranded` event; no
  dust, no tombstone), so it can't hold its wave open forever. Content validation rules this out. Every spawn must
  reach the graveyard. And every cell a summoner can get to (by walking, climbing or leaping, as it can) must let its
  summons reach the graveyard too, so a leaping boss can't strand non-leaping summons on a ledge-only island. The
  headless sweep fails if any run strands an enemy.

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

- **Lane jumps.** While a runner can still walk to the graveyard, it takes a ledge by a seeded 40 % chance, and only
  if the landing is at most 4 m farther from the graveyard. A ledge always counts as an open route, though: a runner
  walled into a lane walks to the ledge and leaps (no roll) instead of breaking a wall.
- **Who can leap:** any enemy whose data has a `jump` object, even with a chance of 0. A chance of 0 means it never
  takes a ledge as a shortcut, but still leaps when the ledge is its only way out. A chance that rounds to 0 per
  mille (below 0.0005) is refused, so it can't silently turn the shortcut off.
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

- **53 tests:**
  - map and content validation (errors carry a machine-readable code), including summon islands
  - the content hash, which ignores cosmetic edits
  - flow fields and rerouting
  - the wall rule
  - lane jumps, including a runner sealed in a lane, which must leap on every seed
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
- **The replay suite above** passes: per-tick hash equality, divergence across seeds, and three pinned full runs.
- **Recordings check themselves**; see the next section.
- **Speed:** a full 12-minute run simulates in about 15 ms on one core (release build). A 1000-seed sweep of the
  reference layout takes about 2 s on 32 threads.

### How agents play it headless

Everything runs without a window, through `games/sepulcra/examples/headless.rs`. Always use `--release`.

- **One run:**

  ```text
  cargo run -p sepulcra --example headless --release -- --seed 7 --layout centre
  ```

  This prints a table per wave (spawned, killed, leaked, stranded, tombstones lost, dust) and the final state hash.
  - `--ascii` adds the final board as text, and `--map` prints the slab.
  - `--waves N` stops before wave N + 1.
  - `--data DIR` runs a modified copy of `data/`.
- **Layouts** are build orders in JSON (`data/layouts/*.json`, format in `src/script.rs`): commands keyed to a wave
  or a tick, such as `{ "wave": 3, "upgrade": "top:12,8" }`. Name a built-in layout (`centre`, `no_air`,
  `full_block`, `none`) or pass a file.
- **Balance sweeps:** `--sweep N` plays N seeds on every core and prints:
  - win rate
  - tombstones left
  - losses by wave
  - mean leaks per wave
  - stranded enemies

  `--layout all` sweeps every built-in layout. The command exits non-zero if any run strands an enemy.
- **Recordings:** `--journal FILE` writes the run as a recording. It holds:
  - the seed, as a decimal string
  - the content hash, as 16 lowercase hex digits
  - every applied command with its tick
  - the end tick and state hash

  `--replay FILE` replays a recording and checks it. It fails, with a structured reason, if:
  - the content differs
  - any command is refused
  - a command is stamped at or after the end
  - the end tick or hash differ

  A recording without an end replays as "unverified", never as verified. Edits to notes, display names, glyphs,
  whitespace or key order don't change the content hash, so they keep old recordings valid.
- **From code:**
  - `sepulcra::run(content, seed, &script, max_waves, |state, events| ..)` gives a `RunReport`.
  - `State::step` and `State::step_into` drive a game one tick at a time.
  - `State::hash()` lets a run check its determinism tick by tick.

### Decided (owner, 2026-10-22)

- **Walls:** only on top-face lanes. The cliffs stay the danger lanes.
- **The colossus:** a leak is survivable but costly.
- **Edge reach:** keep 3 m, until a human playtest says otherwise.
- **Wraiths:** rune pillars don't slow them; the obelisk answers flyers.
- **Calling waves early:** the bonus is a real lever, but small.
- **Breaking walls:** keep 8 m per wall, until playtests say otherwise.
- **The v1.0 scope:**
  - three maps of about 10 waves each, using every face of the cube, sides and bottom included;
  - the maps are a graveyard, a frozen wasteland and a volcano;
  - English first.
  - **Staged:** map 1 is the prototype on all six faces, which proves the camera and the pathing. Maps 2 and 3
    reuse the same cube base with different paths.
- **Camera on the cube:** right-click orbits the cube, and the view never flips upside-down.
  - Paths stay readable across edges: path edges are easy to trace from face to face.
  - **(proposal)** Path edges glow and carry on over the cube's edges, with small arrows where a path crosses a
    face.
  - **(proposal)** A small cube indicator in a corner shows the face in view and marks the faces where enemies are.
- **Free and open:** Sepulcra is a free demo on Steam and stays open source (MIT/Apache), as the engine's showcase.
- **Controls:** a gamepad pause, a title screen, saved settings and rebinding (M5/M6).

The rusher balance question was answered by the owner on 2026-11-16: "Keep it risky." Anything still
undecided is in [HANDOFF.md](../../HANDOFF.md#open-owner-decisions).

## Elemental stones (proving ground; proposal)

The owner, 2026-11-24: "Test the elements by setting up elemental towers in sepulchra. Test fire,
lightning, frost and wind (non damage but through physics, and wind pushes mobs back, recharges
each shot)"; then "You can also have one exploding zombie, and upon impact, destroys the walls
that's placed other lane"; then "And test the fire spread. Also think up earth tower effect for
sepulchra. It will be a good time to really test out our effects for elements that can be
generated via code rather than human made images and svg". **Everything in this section is the
lane's proposal** (lane `m6-turres`, 2026-11-24), read by the lead as: five new stones, the wind's
doing no damage and working by pushing, the parenthesis about the wind only (the other reading is
at the end), and one new enemy.

**They live in a proving ground, not on the shipped maps.** The proving ground is map 1's slab with
content of its own (`data/proving/`: `stones.json`, `enemies.json`, `waves.json`, `layouts/`),
loaded by `Content::proving()`, `--ground proving` in the headless example, `--data
games/sepulcra/data/proving` and `play_new {"data": "games/sepulcra/data/proving"}` in the app (a
content folder without a `slab_01.json` plays on map 1's). The shipped content is untouched: its
content hash, every state hash and every save byte are what they were (the elements are hashed and
saved only where the content has them; a test and the pinned replays hold it), and its sweep prints
the same numbers. **Only map 1 exists as data today**: the frozen wasteland and the volcano of the
v1.0 scope are not built, so "the three maps' balance" is map 1's. To offer the stones on a real map
later: add their rows (and the `elements` rules and fuel) to that map's `stones.json`, give the HUD
build bar room for nine stones and their strings in every language, sweep the map's layouts with
them, and say which waves the exploding dead join; nothing in the rules changes.

### The stones

Costs follow the shipped stones (a basic stone 50, the specialists 60 to 80, upgrades about the
stone's cost again, then half as much more); ranges sit between the brazier's 6 m and the obelisk's
9 m. Every number is data (`data/proving/stones.json`) and is a first guess the sweeps below test.

| Stone | Element | Attack | Tier I | Tier II (+cost) | Tier III (+cost) |
|---|---|---|---|---|---|
| **Rogus**, the pyre stone | fire | lob that sets alight | 85: 8 dmg, 1.8 m splash / 1.8 s, 6 m; burns 4/s for 3 s | +80: 12, 2 m / 1.7 s, 6.5 m; 6/s for 3.5 s | +110: 18, 2.3 m / 1.6 s, 7 m; 9/s for 4 s |
| **Fulmen**, the storm menhir | lightning | instant strike that chains | 90: 14 dmg / 1.5 s, 7 m; 2 jumps of 2.5 m keeping 70 %; wet +50 % | +80: 22 / 1.4 s, 7.5 m; 3 jumps of 3 m | +120: 34 / 1.3 s, 8 m; 4 jumps of 3.5 m keeping 75 %; wet +60 % |
| **Gelu**, the rime stone | frost | bolt that chills and freezes | 75: 6 dmg / 1 s, 6.5 m; 25 % slow 1.5 s; chill 0.4 a hit, frozen 2 s, shatter ×2 | +70: 9, 7 m; 30 %; chill 0.5, 2.5 s, ×2.5 | +100: 14 / 0.9 s, 7.5 m; 35 %; chill 0.6, 3 s, ×3 |
| **Ventus**, the gale cairn | wind | gust: no damage, a shove | 70: shoves 3 m over 0.5 s, 5.5 m; recharges 6 s; waits for 2 in range | +70: 4 m, 6 m; 5 s | +90: 5 m over 0.6 s, 6.5 m; 4 s |
| **Saxum**, the heaving stone | earth | spikes along the lane | 90: 15 dmg on 2 lane cells, stagger 0.8 s / 3 s, 6 m | +80: 24 on 3 cells, 1 s / 2.8 s, 6.5 m; rubble 120 HP for 6 s | +110: 36 on 4 cells, 1.2 s / 2.6 s, 7 m; rubble 200 HP for 8 s |

What each does, in the rules' own terms (`src/sim_elements.rs`, `src/blaze.rs`):

- **Rogus (fire).** A lob like the brazier's, weaker on impact, that sets every ground enemy in its
  splash alight (a burn per tick for a time) and heats the fuel under the splash. A burning enemy,
  at every spread step (0.5 s), sets alight any other within **1.2 m** (packed dead) and heats the
  fuel within 2.9 m. Good against columns, and on grassed lanes; useless against the wet and the
  frozen (fire thaws the frozen, with steam, and the wet do not catch); on bare stone it burns only
  what it hits.
- **Fulmen (lightning).** No projectile: the strike hits the first enemy in range and jumps to the
  nearest enemy not yet struck within the jump's reach of the last, each jump keeping 70 to 75 % of
  the damage. The wet take half again (more at III). A staggered enemy (earth) grounds it: the chain
  ends on it. Good against strings of runners and the thawed; weak against one tough target. One
  menhir never strikes more often than every 0.367 s (`MIN_STRIKE_TICKS`, refused in the content
  otherwise): lightning never flashes more than three times a second.
- **Gelu (frost).** A bolt that slows (as the rune pillar does) and adds chill, divided by the
  target's **mass**; at full chill the target freezes: it stands, breaks no wall, cannot burst, and
  the next hit on it shatters the ice for double damage (2.5×, 3×). Chill fades over 4 s; a freeze
  thaws after its time and leaves the enemy **wet** for 4 s. Frost on a burning enemy puts it out;
  frost lays rime on the target's cell, which the grass fire does not cross while it lasts. Good
  against the exploding dead and the colossus's escort; poor against the colossus itself (mass 8:
  0.075 chill a hit at III) and alone against crowds.
- **Ventus (wind).** Deals nothing (the content refuses any damage, slow, burn or chill on a gust).
  When at least two enemies are in range it **gusts**: every ground enemy in range is shoved back
  along its path, 3 m (4, 5) divided by its mass, over half a second, then walks on; burning
  enemies it shoves are fanned (their fire renewed); the wind lingers 4 s over the area and leans
  the grass fire; and the cairn's charge is spent. It cannot gust again until it has **recharged**
  (its cooldown, 6 s, 5 s, 4 s), and the charge is a number the HUD can draw filling
  (`State::charge`). **The dead lean into the wind:** the n-th shove an enemy takes is divided by
  1 + 0.5 n (`shove_fade`). Without it the first proving sweep found runs that never ended (a
  gust every 5 s pushed a wave's last shamblers back 4 m, as far as they walk in 5 s: 922 of 1,000
  `fire_wind` runs unfinished at the time cap); a test now holds that every wind layout ends.
  Good for buying time, for keeping the dead in another stone's range, and for
  pushing a burning one back into the column; useless against flyers, nearly so against the
  colossus (3 m ÷ 8 is 0.37 m).
- **Saxum (earth): an upheaval.** Stone spikes erupt along the target's lane: its cell and the next
  cells of its route (2, 3, 4). Every ground enemy on them takes damage and is staggered (it stands
  0.8 to 1.2 s), and the fire on them is smothered (the burning dead put out, the fuel covered: sand
  on flames). From tier II the spikes leave **rubble** on the farthest cell of the line a wall can
  stand on: a wall worth nothing (no refund), with its own HP, that blocks and re-routes like the
  player's walls, that the dead break and the exploding dead blast, and that crumbles by itself
  after 6 or 8 s. The two other first thoughts were weighed and set aside: a *tremor* ring (a
  slowing ring is the rune pillar's job already) and a *boulder* loosed down a lane (a rolling body
  needs physics bodies in the rules, which the wind's design below defers; it is the natural
  tier-III variant once that seam is handed over).

### The exploding dead (Tumidus)

| Enemy | HP | Speed | Movement | Dust | Tombstones |
|---|---|---|---|---|---|
| Tumidus, the bloated dead | 40 | 0.9 m/s | Walker from the gorges; a **sapper**: it follows the breach field even while an open route exists, so it walks to the wall on its cheapest way through (a wall costs its length plus 8 m) | 4–6 | 2 |

- **What sets it off:** reaching a wall it would break (it bursts on it: no dust, counted as
  `detonated`, not as a kill); being killed (it bursts where it falls: dust as a kill); being struck
  by lightning (at once); being set alight (its 1.5 s fuse burns, then it bursts; frost or earth
  putting the fire out puts the fuse out). **Frozen, it cannot burst**, and if it dies frozen it
  dies whole.
- **The blast:** radius 3 m from where it stands. **Every wall and rubble whose cell centre is in
  the radius is destroyed**, with no refund: the wall it hit and any other within 3 m, which on map 1
  is the cell beside it in a lane two walls thick, or a wall across the neighbouring lane where two
  lanes run side by side (the reading "walls placed in the other lane" is a radius of about 4 to
  5 m; it is one number). Every ground enemy in the radius takes 25 damage and is shoved 1 m back;
  the fuel in it takes a full heat (the blast's heat: it lights the grass). Stones are not hurt
  (stones in Sepulcra have no hit points; giving them some is a design of its own and is left out).
- **A wall destroyed mid-wave re-routes everyone:** the flow fields are rebuilt once per burst (a
  Dijkstra over the slab's cells for each of six fields: well under a millisecond, once per burst
  and not per tick), from the walls that are left, in a fixed order, so the run stays deterministic
  and a save taken the tick before continues the same (tested). Every enemy takes its next step
  from the new fields when it reaches its next cell.

### How they meet (in and out)

In, each by one rule in the code above:

- **Frost then fire:** fire thaws the frozen (steam) instead of burning them; frost puts the burning
  out and lays rime that stops the grass fire. They undo each other, on purpose.
- **Frost then lightning:** the thawed are wet; the wet take half again from a strike. Gelu and
  Fulmen together are the "set up and cash in" pair.
- **Wind and fire:** a gust fans the fires of the dead it shoves, pushes the burning ones back into
  the column behind them (where they set others alight), and leans the grass fire: heat moving with
  the gust's wind is × 1.6, against it × 0.4, for 4 s.
- **Earth and fire:** the heave smothers fire on its cells, the dead's and the grass's.
- **Earth and lightning:** a staggered enemy ends a chain (stone grounds the strike).
- **Earth and the exploding dead:** rubble is a wall to them; they walk to it and blast it.
- **Lightning and the exploding dead:** struck, they burst at once, wherever they are (well before
  your wall, or in the middle of the column).
- **Frost and the exploding dead:** frozen, they cannot burst; shattered, they die whole.
- **Fire and the exploding dead:** alight, their fuse burns; fire spreading through a column with
  bloated dead in it sets off a chain of blasts.
- **Wind and the exploding dead:** a gust pushes them back from the wall they are walking to.

Out, deliberately: wind putting fires out (a gust only fans); wind pushing anyone off the slab or
off a lane (the shove follows the lane graph; see the physics question below); flyers and the
elements (wraiths are the obelisk's, as the owner decided, so every elemental stone targets the
ground); frost turning wet ground to ice that the dead slide on, and earth to hard ground that
cracks (both need bodies that slide: the physics hand-over); stones that burn or crack (stones have
no hit points); fire on the graveyard's grass (hallowed ground: those who reach it are past every
stone); the dead catching fire from rooted cells (the roots of nature are the player's own and
would burn the player's slow: left to the owner).

### What burns on the slab (fire's spread)

The slab is stone: fragments, lanes, cliffs and the graveyard's slabs do not burn. **Fuel** is what
grows on it: the grass the app draws at the rock's edges (`fuel::grass_drawn`, the same rule as
`sepulcra-app/src/grass.rs`, held equal on map 1 by the app's test, less the graveyard's), and, in
the proving ground, dry grass laid along two lane stretches (`top:9,14..top:9,16` on the middle
lane, `top:5,17..top:8,17` on the west connector), listed in `stones.json`'s `elements.fuel`.

`src/blaze.rs` is the fire, behind its seam (below). Each fuel cell has a heat, fuel left, a phase
(fuel, burning, spent) and a cover. Every spread step (0.5 s) each burning cell gives 0.35 of an
ignition to every fuel cell of its face within **2.9 m** of it (its four neighbours at 2 m and
four diagonals at 2.83 m), leaned by the wind; the gains are summed first and then taken in, so no
cell's place in the order changes another's. A cell at a full ignition catches, burns 8 s and is
spent (char, never again). **A break of bare stone wider than 2.9 m stops it**: one bare cell is a
gap of 4 m. Frost, earth and the wet cover a cell for 4 s: it takes no heat and goes out. A burning
cell sets alight the ground dead within 2.9 m of it (so the dead walking past burning grass catch:
3 damage a second for 2.5 s); a burning enemy heats the fuel within 2.9 m.

### Rules or physics: where each one's truth lives

Sepulcra's rules are a deterministic integer simulation that runs with no engine (`cargo run -p
sepulcra --example headless`: 1,000-run sweeps are how balance is tested), with its own hash and
saves. The engine's physics (`specula-physics`) and Elementa (`specula-elementa`) are deterministic
too, but they are the engine's, and they are floats (`dmath`-clean `f32`). The decision, from the
code:

| Effect | Where its truth lives now | Why | Later |
|---|---|---|---|
| Fire on the dead (burn, catching, fanning) | the rules (`EnemyEl`) | a status on an enemy the rules own, like the slow; integers; the sweep needs it | stays in the rules: an enemy is not an Elementa node (it moves every tick, and a node that moves costs a grid update; Elementa's own design says a burning creature is the rules' decision) |
| Fire on the slab (fuel cells) | the rules, behind the seam `Blaze` | the sweep must burn grass too; Elementa is not in the engine yet (stage 6) | Elementa (stage 12, fuel fields), behind the same seam: see "the hand-over" |
| Lightning's chain | the rules | an instant integer search among the rules' enemies | stays; Elementa's `strike` (stage 10) is for world things that conduct (a metal fence), not for the dead |
| Frost's chill, freeze, shatter, wet | the rules | statuses on enemies; integers | stays; rime on a cell is a cover in `Blaze` (Elementa's `chill` and `wet` acts when handed over) |
| Wind's shove | the rules, behind the seam `shove_back` | see below | a physics body, behind the same seam |
| Earth's spikes, stagger, rubble | the rules | rubble is a wall: walls are the rules' (the flow fields read them) | stays; the spikes' rising and sinking are a look |
| The exploding dead's blast on walls and the dead | the rules | walls and flow fields are the rules' | stays; its push and heat are Elementa's `blast` (stage 9) only for world things outside the rules |

**The wind, through physics.** The owner's words are "through physics". Today the dead are not
bodies: the rules move each one along the edges of the lane graph, in millimetres, and the app draws
them where the rules say. Three ways were weighed:

1. *A dynamic body steered back to the flow field.* The truth would leave the rules: the dead's
   positions would be the engine's, the headless sweep (which has no engine) could not run the
   wind, and the rules' integers and hash would depend on the engine's floats.
2. *A character controller that takes an impulse.* The same, with better behaviour (no tipping);
   the same problem for the sweep and the hash.
3. **A kinematic body moved by the rules, with the push as an input.** Chosen. The push is the
   rules' own deterministic displacement, `World::shove_back`: back along the edge the enemy walks,
   then onto the cell upstream of it on the flow field (on a shortest route through it, ties to the
   lowest id), never into a wall, never off the lanes, not in mid-leap, pinned at a spawn. Its
   amount is divided by the enemy's mass and spread over the gust's time, so it reads as a body
   pushed (fast, then walking on). The headless sweep runs it exactly; saves hold `shove` and its
   rate; replays repeat it bit for bit.

**The seam.** The rules' callers give a push (`World::shove(i, push, ticks)`) and read positions as
before; one function decides where that push puts the enemy. To hand it to physics without
changing a caller: `specula-physics` is a pure crate (std and `specula-core`), so the rules crate
may own a physics `Space` of its own, built from the slab's cells (lanes as floor, fragments and
walls as boxes, the rim as an edge one can fall from), with each pushed enemy a character body
(`move_character`) for the time of its shove, then snapped back to the nearest lane cell and its
flow field. That keeps the sweep (the space is in the rules, not the engine) and makes "off the
slab" possible (a body over the rim falls: a new outcome the rules would then have to name).
What it costs: floats in the rules' state (Sepulcra's "no floats" pillar would give way to the
engine's `dmath` rules: DETERMINISM.md accepts both), a physics section in the game's save and hash,
and a design question for the owner (below). The app's ragdolls (the shamblers' deaths) are
already engine physics and stay cosmetic.

**Fire's hand-over to Elementa** (`Blaze` is the seam: the rules call only `heat`, `cover`,
`add_zone`, `is_burning`, `burning`, `step`, and hear `Caught`, `Spent`, `Smothered`). What it
needs, measured against `C:/sl-elem/crates/specula-elementa` as built (stage 5): each fuel cell a
node of material `grass` (`NodeDesc::new(id, grass, centre, half)`), `heat` an `Act { Ignite }`,
`cover` an `Act { Wet }` (frost) or a covered material (earth), `step` the node's `Elementa::step`
and `take_events` mapped to the three events. **The rules crate can depend on `specula-elementa`
directly** (it is pure), so this needs no engine and keeps the sweep. What does not exist yet:
**local wind**: Elementa has one world wind (`set_wind`), and a gust is local and brief, so either
wind zones (stage 9) or the gust setting the whole slab's wind for 4 s (a different rule); the
**tick rate**: Sepulcra ticks at 30 Hz and Elementa's `spread_hz` must divide it (10 Hz does, so 3
ticks a step against the rules' 15); **reach**: Elementa's `reach_m` is at most its `cell_m`, so
`cell_m` 3 m, `reach_m` 0.9 m between grass boxes of 2 m; **ignition by threshold, not by summed
steps**, which changes the numbers (`elementa_check` predicts them); and the hash and save: an
`elementa` section inside the game's state (`save_state` / `restore_state` exist). A burning enemy
heating fuel would be an `Ignite` act on the nodes near it each step (64 acts a tick at most, which
a column of burning dead can pass: a bound to raise or to batch as a sphere act).

### What each stone shows and sounds like (Effectus entries, by moment)

By the effects library's table (`C:/sl-fx/docs/design/EFFECTUS.md`, "The first table" and "The table
as built"). **Built** means in the table today (stage 1a: `fire`, `smoke`, `explosion`, `sparks`,
`embers`, `motes`, `trail`, `dust_puff`, `impact`); **planned** means in the first table and not
built; **missing** means in neither.

| Stone / enemy | Idle | Charging | Firing | Hit | Lingering |
|---|---|---|---|---|---|
| Rogus | `fire` (`brazier`, radius 0.16, `light: false`; built) in its bowl | none | `fireball` (planned, radius 0.3, trail 0.6) in flight | `explosion` small (built, radius 0.6, debris 0, smoke 0.2, shockwave false, scorch true) | `burning` (planned, stage 7: Elementa's look) on each burning one; **on fuel cells: `burning` with `extent` 2 × 0.3 × 2 and `fuel`** (stage 7), char as a scorch decal |
| Fulmen | `motes` (built, tint cyan) or a faint `arc` between its horns (planned) | none | `lightning` (planned, E1/E4, `from_height` 2, `strikes` 1, `branches` 1) from the menhir to the first; `arc` (planned) along each jump, **from one enemy to the next: `to_entity`** | `impact` (built) | none |
| Gelu | `frost` (planned, E5) on its top | none | a cold `bolt` (planned) or `projectile_trail` (planned) | `ice_shards` (planned); on freeze `freeze_burst` (planned, radius 0.6) | `frost` with `extent` the enemy's box while frozen; thaw: `steam` (planned) one-shot |
| Ventus | `leaves` (planned, E5/E8) idling about it | **missing: a visible charge** (a ring that fills: the nearest is `charge_up`, planned E3, which is a one-shot that gathers, not a meter; the game can draw the meter from `State::charge` in its HUD) | `wind_gust` (planned, `direction` from the stone to its target, `length` its range, streaks 30, debris true) | `dust_puff` (built) at each shoved enemy's feet | `smoke` drift with wind (E8) over a fanned fire |
| Saxum | `motes` (built, dust tint) | none | **missing: geometry that rises and sinks** (spikes) | `dust_cloud` (planned), `debris` (planned, E5) | **missing: a crack in the ground as a code-made mark** (the `cracks` decal exists, built in Scintilla, but not as an Effectus entry); rubble is the game's wall model |
| Tumidus | none (its model) | burning fuse: `sparks` (built, rate 18) at its head | — | `explosion` (built, radius 3, debris 24, smoke 0.6, shockwave true, scorch true, `blast: false`: the rules own the blast) | scorch decal (built in the entry), the wall's rubble (`debris`, planned) |
| Fuel cell (grass) | the grass model | — | — | — | `burning` (stage 7), `embers` (built), `smoke` (built), char (scorch decal) |

Sounds: every entry with a sound generates its recipe from its arguments (decision 8); the game's
own recipes (`assets/sounds/*.json`, synthesised) stay for what it has. Missing as sounds: a
crackle of ice, a rumble of earth, a gust (the entries `freeze_burst`, `dust_cloud`, `wind_gust`
carry them when built).

### Made by code, or drawn

Searched (2026-11-24): `games/sepulcra/assets` holds 112 JSON files and one TOML file and **no image
at all**: every texture is a texture-graph recipe (`recipes/tex_grass_blades.json`,
`recipes/tex_crypt_stone.json`), every sound a synthesis recipe (`sounds/*.json`), the music a
recipe (`music/*.json`). Scintilla's particle textures (`soft`, `spark`, `smoke`, `flame`, `ring`,
`rune`, `dust`) and decals (`scorch`, `rune_circle`, `cracks`, `cracks_normal`, `soft`) are drawn
by functions (`crates/specula-scintilla/src/textures.rs`). **So everything Sepulcra's effects show
and play today is made by code.** The only drawn shapes the game shows are the letters of its
interface: the bundled Alegreya and Alegreya Sans faces (`crates/specula-text/fonts/*.ttf`,
outlines a type designer drew), which are not effects. The elemental stones' looks above keep to
that, with three things the library does not do yet:

- **Geometry that comes and goes** (Saxum's spikes, rubble, a boulder): a small procedural model
  from a recipe with a seed, shown by the effect and removed by it. The shape library has the
  shapes by name (prisms, pyramids, crystals; `formae_get`), and the modeller builds a recipe with
  a seed; what is missing is an Effectus entry kind that spawns a model for its life (rising from
  below the ground over 0.15 s, sinking after), with its cost (triangles, draws) counted before.
  EFFECTUS.md leaves "mesh effects" out ("Left out, and why"): this asks to bring a small one in.
- **A crack in the ground made by code**: the `cracks` decal exists in Scintilla; an Effectus
  entry (`ground_crack`, length and branching as args, generated per seed) does not.
- **A rumble**: a low synthesised sound (noise through a low-pass, a slow envelope), as the other
  sound recipes are: missing only as an entry.

### The proving ground's waves and layouts

`data/proving/waves.json`, six waves, 450 dust to start and 20 tombstones: (1) a packed column of
14 shamblers up the grassed middle lane, 0.9 s apart (fire's spread); (2) ten runners strung out
1.6 s apart, then ten shamblers packed (lightning against spacing); (3) shamblers, then six
exploding dead (walls destroyed, frost, lightning); (4) eighteen light runners in a rush (the
wind); (5) the colossus and its escort (frost's shatter; the wind barely moves it); (6) everything,
with wraiths for the obelisks. `data/proving/layouts/`: one build order for every case, the same
dust and the same places: the two stones under test at `top:10,15` (beside the grassed middle lane)
and `top:8,12` (beside the south gate), three warding stones, walls on the middle lane and the
south gate from wave 2 (for the exploding dead), obelisks from wave 4. `baseline` puts warding
stones where the elements go; `fire`, `lightning`, `frost`, `wind`, `earth` two of one; the ten
pairs one of each.

### What the sweeps say

`cargo run -p sepulcra --example headless --release -- --ground proving --layout all --sweep 1000`
(seeds 1–1000, commit `f39b9770`, 2026-11-24). Tombstones out of 20; kills are of every enemy
spawned (the colossus summons, so the count varies); "detonated" counts bursts on walls only
(bursts of killed exploders are kills). No run stranded an enemy and every run ended.

| Layout | Won | Tombstones left (mean) | Leaks (mean) | Kills (mean) | Detonated on walls (mean) | Game time (mean) |
|---|---|---|---|---|---|---|
| baseline (wardings) | 1000 | 19.65 | 0.3 | 128.5 | 0.0 | 6:39 |
| fire | 1000 | 18.60 | 1.4 | 127.6 | 0.0 | 6:40 |
| lightning | 1000 | 19.93 | 0.1 | 126.9 | 0.0 | 6:39 |
| frost | 973 | 6.51 | 10.7 | 124.6 | 0.3 | 6:38 |
| wind | 56 | 0.10 | 19.2 | 82.7 | 0.9 | 6:01 |
| earth | 1000 | 19.03 | 1.0 | 123.2 | 0.9 | 6:40 |
| fire + lightning | 1000 | 19.80 | 0.2 | 128.8 | 0.0 | 6:39 |
| fire + frost | 1000 | 9.60 | 6.4 | 130.6 | 0.0 | 6:40 |
| fire + wind | 959 | 4.37 | 11.6 | 127.0 | 0.0 | 6:44 |
| fire + earth | 1000 | 18.52 | 1.5 | 155.0 | 0.1 | 6:57 |
| lightning + frost | 1000 | 10.47 | 5.5 | 131.4 | 0.0 | 6:40 |
| lightning + wind | 983 | 5.21 | 10.8 | 128.0 | 0.0 | 6:45 |
| lightning + earth | 1000 | 19.50 | 0.5 | 154.1 | 0.0 | 6:53 |
| frost + wind | 712 | 9.18 | 9.4 | 120.1 | 0.7 | 6:34 |
| frost + earth | 1000 | 18.01 | 1.9 | 155.7 | 0.4 | 7:21 |
| wind + earth | 992 | 12.62 | 6.9 | 123.3 | 0.6 | 6:39 |

What it says, read honestly:

- **The proving ground is too easy to separate the strong stones**: the baseline of wardings
  leaks 0.3 in a run, so fire, lightning and earth all sit at the ceiling. A harder wave set (or
  fewer wardings in every layout) is the next tuning step before any number is called balanced.
- **Lightning is the strongest single element** (0.1 leaks, better than the wardings it
  replaces); **frost alone is weak** (10.7 leaks, most in wave 1's packed column and wave 5's
  colossus, which mass 8 makes nearly unfreezable), as its row says; frost's own payoff (thaw then
  strike, freeze then shatter) does not make up for it in these waves.
- **Wind alone loses** (5.6 % wins): two cairns that deal nothing in place of two wardings leave
  three wardings to do all the killing. Paired with a damage stone it holds (fire + wind 95.9 %,
  lightning + wind 98.3 %, wind + earth 99.2 %) but costs tombstones: in these waves it buys time
  the other stones do not use. That is the shape of the owner's "non damage" stone; its numbers
  (range, push, recharge) are what to tune.
- **Earth's pairs kill the most** (155 kills: the rubble re-routes and the colossus summons more
  while it is held), and earth + fire runs longest of the 100 % layouts (6:57).
- **The exploding dead rarely reach a wall** (at most 0.9 bursts on walls a run): the walls on the
  middle lane and the south gate stand inside the wardings' range. A layout that puts its walls
  out on the gorge lanes would show them more; the rules are tested directly
  (`an_exploder_bursts_on_a_wall_the_wall_goes_and_the_dead_reroute`).
- **The first sweep found a stall** (fixed: `shove_fade`, above), which is what the sweep is for.

Single runs of seed 1, the elements' counts (`--layout NAME --seed 1`): fire 155 ignitions and 31
fuel cells caught; lightning 309 strikes; frost 34 freezes and 32 shatters; wind 87 gusts; earth
81 heaves and 5 walls (rubble) broken; fire + wind 124 ignitions, 25 gusts; lightning + frost 215
strikes, 10 freezes.

The shipped map's sweep (`--layout all --sweep 1000`) prints, line for line, what it printed
before the elements (centre-heavy won 999, tombstones 7.42; no anti-air lost 1000 in wave 6; full
block lost 1000 in waves 3–6; no defence lost 1000 in wave 2), and the final hashes of seeds 1, 7,
42 and 999 of each built-in layout are unchanged.

### The other reading: all of them through physics, none through damage

If the parenthesis is about all four (no stone deals damage; everything works by physics):

- **Fire** would deal nothing itself: burning would *drive* the dead (a burning one flees: a push
  away from the flames and a speed-up into other stones' range), spread to the grass, and its harm
  would come only from the blasts it sets off (the exploding dead) and the fall from the rim.
- **Lightning** would be a shove that chains: each struck enemy knocked back a little (a stagger),
  the chain's value being time and the exploders it sets off.
- **Frost** would be the freeze and the slide: a frozen one stands, and on rimed ground the dead
  slide (a body with low friction), which needs the physics seam handed over.
- **Wind** as now.
- **Earth** would be rubble and spikes as obstacles only: re-routing and stagger, no damage.

What changes: the elemental stones' `damage`, `burn_dps` and `shatter` go to 0 (data), the
content's checks forbid damage on every elemental attack as they do on the gust today (one line
per attack in `check_tier`), and the game needs a way for physics to kill: **falling off the slab**
(the physics seam above, then a rule that a body below the rim dies: a new event) or the warding
stones doing all the killing. The sweeps would then show the elements as time bought, measured as
leaks and the waves' clear times, not as kills.

### What the elements ask of the engine

Where each piece stands on 2026-11-24. *Working line* is `m6-una` (this branch's base); *lane*
names the branch it is on and not merged; *planned* names the stage of EFFECTUS.md's build plan;
*nowhere* is in no plan. The rules need nothing from the engine (they are built, here); what the
towers need is their **look and sound**, and, for the hand-overs, two crates the rules can use
without the engine.

| For | Piece | Where |
|---|---|---|
| all | the rules' events (`Struck`, `Gusted`, `Heaved`, `Ignited`, `Frozen`, `Thawed`, `Shattered`, `Detonated`, `RubbleRaised`, `WallCrumbled`, `CellCaught`, `CellSpent`, `CellSmothered`) and state (`EnemyEl`, `State::blaze`, `State::rubble`, `State::charge`) that the app turns into looks | **built** (this lane) |
| all | an Effectus entry played by name and args from the app's Rust (a `vfx` component with `args`, and a one-shot cue at a place) | component `args`: lane `m6-effectus` (stage 1b); one-shot cues (`effectus_play`, the engine's cue instances, the `effects` save section, the flash governor): planned, stage 1c. The app can use components and delete them itself (as it does today) before 1c |
| Rogus | `fire` (`brazier` alias) idle; `explosion` small on landing | lane `m6-effectus` (1a, built) |
| Rogus | `fireball` in flight | planned (first table, stage 4) |
| Rogus, fuel | `burning` on each burning enemy and each burning fuel cell, nearest first within the budget | planned, stage 7 (Elementa's look); the entry itself needs nothing of Elementa: the app can place it from `CellCaught` / `Ignited` |
| Rogus, fuel | char on spent cells: a scorch decal | working line (`scorch` decal); as an entry's decal, lane `m6-effectus` |
| Fulmen | `lightning` (menhir to first), `arc` (enemy to enemy, `to_entity`) | planned, stage 2; engine side (E1 beams, E4 targets) on lane `m6-scintilla2` |
| Fulmen | the flash governor (several strikes and blasts in one second) | planned, stage 1c, with the fraction-of-frame from `flash.rs` (lane `m6-scintilla2`, E1..E4) |
| Gelu | `frost`, `freeze_burst`, `ice_shards`, `steam`, a cold `bolt` | planned, stage 4 (E5 textures: `shard`, decal `frost`) |
| Ventus | `wind_gust` with `direction` | planned, stage 4 (needs nothing new of Scintilla) |
| Ventus | a charge meter filling | **nowhere** as an effect; the HUD draws it from `State::charge` (game side, not built: the build bar has four stones) |
| Ventus | smoke and embers drifting with the gust | planned, E8 wind drift (lane `m6-scintilla2`), with a *local* wind: **nowhere** (E8 reads the world's one wind) |
| Saxum | spikes rising and sinking: a seeded model shown by an effect for its life | **nowhere** ("mesh effects" are left out of EFFECTUS.md); the modeller can build the spike (formae `pyramid`/`prism`, a recipe with a seed) today, and the app can animate a transient model itself |
| Saxum | a code-made crack (`ground_crack` with length and branching) | **nowhere** as an entry; the `cracks` decal exists (working line) |
| Saxum | `dust_cloud`, `debris`, a rumble sound | planned, stage 4 (E5 `chunk`); the rumble is a generated sound like the others (decision 8) |
| Tumidus | `explosion` (radius 3, debris, shockwave, scorch) | lane `m6-effectus` (1a, built); its ground ring is E2 (stage 3) |
| Tumidus | a destroyed wall's debris | cosmetic chunks (`debris`, planned stage 4), **not physics bodies**: the rules decide the wall is gone and nothing else may depend on where its pieces fall; physics chunks would be presentation that costs bodies and could block nothing. Today the app's `wall_broken` dust and cracks stand in |
| Tumidus | `blast` (push, heat, event) for world things | planned, stage 9; **not used by the rules**: the rules' blast is their own (walls, the dead, the fuel). Used only if the slab's props become Elementa nodes |
| fuel (hand-over) | the rules crate depending on `specula-elementa` | possible now (a pure crate, lane `m6-elementa`, stage 5 built); needs local wind (stage 9 wind zones) or a rule change, and the reach and threshold numbers re-tuned (above) |
| fuel (hand-over) | fuel fields: grass patches as fuel by cell | planned, stage 12 (for a game whose fuel is the world's); Sepulcra's own fuel list is the rules', so it needs only stage 5's nodes |
| wind (hand-over) | a physics `Space` inside the rules: the slab as boxes, a pushed enemy a character body (`move_character`), snapped back to its lane | possible now (`specula-physics` is pure, working line); not built: the owner's question 2 below |
| events back to modules | `element.ignited` and the rest at the next tick, `Cx::ignite` | planned, stage 6; **not needed by Sepulcra**: its rules are a Rust crate, not a module, and own their fire, so they hear their own events in the same tick |
| photosensitivity | lightning never over 3 flashes a second for any args | lane `m6-effectus` (decision 5), held per stone here by `MIN_STRIKE_TICKS` |

**The rules' question "how does a rule hear that a thing it owns caught fire":** in Sepulcra it
does not need to: the rules own the fire (`Blaze`) and the dead's statuses, and hear `Caught` in
the step that made it. If the fire is handed to Elementa, the rules crate owns the `Elementa` value
too (it is pure) and reads `take_events()` right after `step()` in the same tick: no next-tick delay
and no engine. A game that is a module, not a Rust crate, hears them at the next tick (stage 6).

**The smallest set that lets one tower be played end to end, looking right: Tumidus and its
blast** (rules: built; look: `explosion`, built on `m6-effectus`, plus the app's existing dust and
cracks for the wall), so the first to come alive is the exploding dead, as soon as stage 1b
merges (a component with args, deleted by the app as its one-shots are today). Then, in order:

1. **Rogus**: `fire` and a small `explosion` (1a), then `fireball` and `burning` (stages 4, 7):
   playable with 1b, right with 7.
2. **Ventus**: `wind_gust` (stage 4, nothing new in Scintilla) and the HUD's charge (game side).
3. **Fulmen**: needs beams and targets (stage 2, E1 and E4 on `m6-scintilla2`) and the governor
   (1c): the most engine work, and the most photosensitivity risk.
4. **Gelu**: needs E5 textures (`shard`, `frost`) and stage 4's cold family.
5. **Saxum**: needs what no stage plans (a transient model, a ground crack entry): last, unless
   the lead adds a stage for "geometry that comes and goes".

### Questions for the owner (with the answer the lane would choose)

1. **"Non damage but through physics": the wind only, or all four?** The lane would choose the
   wind only (built so); the other reading is above and is a change of data plus one rule.
2. **Must the wind's push be the engine's physics, or is the rules' own push "through physics"
   enough?** The lane would keep the rules' push (it runs in the 1,000-run sweeps, saves and
   replays, and reads as a body pushed), and hand it to a physics body inside the rules only if
   the owner wants the dead to be able to fall off the slab.
3. **The exploding dead's blast: the wall it hits and every wall within 3 m, or walls in the
   neighbouring lane too (about 5 m)?** The lane would keep 3 m (a lane two walls thick goes; the
   next lane's wall does not), and no refund.
4. **Does a killed exploder still burst?** The lane would say yes (where it falls: killing it near
   your own wall is a mistake to learn), and no burst when it dies frozen.
5. **Earth: upheaval (spikes and rubble), as built?** The lane would keep it; a boulder is the
   physics hand-over's tier-III variant.
6. **Do the elemental stones ever go on the shipped maps?** The lane would keep them in the proving
   ground until the owner has played it, then offer them on map 2 or 3 (a frozen wasteland suits
   frost and wind; a volcano suits fire and earth), keeping map 1's balance as it is ("keep it
   risky").
7. **Fire on the roots of nature** (the player's own slow): burn or not? The lane would say not.

## Camera and controls

- **Mouse:** orbit by dragging the right button, zoom with the wheel, click to place.
- **Gamepad (needed for Deck Verified):**
  - The bumpers snap the camera to the next face, and the stick orbits.
  - A cursor snaps to placement cells.
  - A radial menu builds stones.
- Design for the gamepad from day one; bolting it on later is a known pain point for tower defence games.
- **Threat indicators:** arrows at the screen edge for faces under attack that are out of view.

**Built (2026-10-16, the playable greybox `games/sepulcra-app`):** clicking a stone or wall selects it
(its panel upgrades, sells or removes); a build bar picks the stone; **P is an active pause** (building
still works) and Escape / the pad's Start open the pause menu; the HUD is `assets/ui/*.json` drawn
with `specula-ui` on the GPU overlay. In-game checks for the owner: [TESTPLAN.md](TESTPLAN.md).

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
  - How to drive it is in [How agents play it headless](#how-agents-play-it-headless).

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
