# Design: the library of effects (`specula-effectus`, `specula-elementa`)

Status: **designed; Elementa's pure crate built (stage 5, below), the rest not built** (lane `m6-effectus`, 2026-11-24). Nothing in this note exists yet
except where it says "I read" or "exists". Foundations F1 (an agent finds and changes an effect by
name and numbers, in a batch, bounded), F2 (behaviour is simulation and holds its rules; looks are
reproducible), F3 (the Intel iGPU is the reference; every cost known before spawning), F5 (every
entry reachable through the protocol and as files a human can edit) and F6 (the same entry in 2D
and 3D, no keyboard, no desktop assumed).

The owner, 2026-11-21: "Are there plans to set up SFX library, like lightning, fire, explosions,
fire spread, laser, freeze, wind, etc?", then "Add it in the plan. This will be part of M6. any
part of the system." On 2026-11-24: "sfx meaning like lightning, fire, and all that". About the
libraries in general: "to help you and other AI agents build stuff and reuse, not just completely
re-create. Think of it like a base template".

## The names, and why two

- **Effectus** (Latin: an effect; what is brought about, a performance) is the **library**: the
  table of named effects, each a generator from a few numbers to everything the effect needs
  (particles, a flash and its light, a decal, a sound, and where it has one a behaviour), the
  catalogue an agent searches, and what each costs before it is spawned. It is presentation data,
  pure and deterministic, and decides nothing in the simulation.
- **Elementa** (Latin: the elements; first principles) is the **behaviour**: fire that spreads,
  cold that freezes, water that wets and quenches, wind that pushes, a strike that conducts. It is
  simulation, with every determinism rule, its own saved state and its own hash.

Two crates because the line between them is the line F2 draws: what changes the world over time
is simulation, what is drawn and heard is not (DETERMINISM.md, "What is deterministic and what
isn't"). One name still offers both: `fire` with `spread: true` is the look from Effectus and the
spreading from Elementa (below, "Cosmetic and behaviour").

**A clash of words to keep clear.** Lex already has a kind called `effect`: a gameplay status in
the manner of Unreal's Gameplay Ability System (slow, poison: LEX.md, table of kinds, line 111;
I read it there). The library never calls itself "effects" where Lex is in play: its tools are
`effectus_*`, its file kind is `effectus`, and a frozen *creature* is a Lex or module status that
Elementa's events let the rules apply, not something Elementa decides (below).

## What exists, read for this design

- **Scintilla** (`crates/specula-scintilla`, docs/design/VFX.md; I read `lib.rs`, `effect.rs`,
  `component.rs`, `library.rs`, `check.rs`, `textures.rs`, the `Particle` and `Instance` types in
  `eval.rs`, and VFX.md whole). Effects as JSON: up to 8 emitters (rate and bursts, lifetime,
  spawn shape point, sphere, box, disc, cone or line, direction along a vector or radial,
  gravity, linear drag, curl noise, size, colour and alpha curves, flipbooks, additive to alpha
  blend, soft edges, lit, `stretch` along velocity, local or world space, collision with a plane
  or the ground grid), and one point light (colour, intensity, range up to 100 m, flicker, a
  `duration` that makes it a flash). **Particles are a closed-form function of time** (no state,
  nothing saved, captures repeat). Analytic summaries exist: `Effect::peak`, `Effect::reach`,
  `Emitter::fill_area`, `eval::work_per_particle`. `check::check` gives `peak_particles`,
  `overdraw`, `reach`, `visible`, `blowout`, `trail_track`, `flash_rate`, `flash_unchecked` and,
  with a palette, `palette`. `flash::flash` refuses at parse time any effect that flashes more
  than 3 times a second at the 3 m reference view. The component `vfx: {effect, params, start}`
  has a **fixed set of params** (rate, size, speed, alpha, intensity, tint, light, seed); a
  `decal` component has `life`, `fade_in`, `fade_out` and `start`. Eight built-in effects
  (`fire`, `embers`, `smoke`, `motes`, `trail`, `dust_puff`, `impact`, `sparks`), seven built-in
  particle textures (`soft`, `spark`, `smoke`, `flame`, `ring`, `rune`, `dust`) and five decal
  textures (`scorch`, `rune_circle`, `cracks`, `cracks_normal`, `soft`), all procedural.
- **Drawing** (`crates/specula-gpu/shaders/particle.vert`, read): every particle of every effect
  is **one instanced draw**; a particle is a camera-facing quad, or a quad **stretched along any
  world vector** `r.c.xyz` (its length on screen plus `size`), which is what makes beams possible
  without a new draw path (below). Decals are one or two instanced draws, **3D only** (VFX.md,
  "Seams for later": "decals on the 2D path" is not built).
- **The engine side** (`crates/specula/src/vfx.rs`, `vfx_tools.rs`, read in part): the library
  resolves a name to a built-in or a world effect under `/vfx` (written by `vfx_set`, under
  `Rule::Vfx` of the write policy); instances come **only from entities' `vfx` components**; the
  `[vfx]` budgets of `perf.toml` (`max_particles` 4,000 on an integrated GPU, `max_work`,
  `max_emitters`, `max_decals`, `fill_cap` 1.5 screens integrated, `lights` 4 of the frame's 16).
  Tools: `vfx_get`, `vfx_set`, `vfx_preview`, `vfx_check`, `vfx_info`. `View` has an `ortho`
  case, so particles already draw under a 2D game's orthographic camera.
- **2D** (`crates/specula/src/two_d.rs`, `crates/specula-gpu/src/sprite.rs`, read): 2D lives in
  the XY plane, y up, seen down −Z. Sprites draw inside the scene pass; **cutout sprites write
  depth, blended ones do not**; sprites are unlit unless `lit`. Particles draw after the scene,
  with their own depth test against the scene's depth.
- **Sound** (docs/design/AUDIO.md, `crates/specula/src/module_sounds.rs`, `logic/handler.rs`,
  read): sounds are JSON recipes (`games/sepulcra/assets/sounds/obelisk_zap.json` is a typical
  one: layers of oscillators and noise, envelopes, filters, a `loudness` in LUFS, `vary` ranges);
  a module asks with `Cx::sound(name, at, gain)`, which the module host queues as a cue
  (**presentation only**, outside the hash); renders run off the engine thread and are cached;
  a sound rendered more than 500 ms late is skipped. `audio_preview` measures LUFS and peaks.
- **Wind** (docs/design/FOLIAGE.md, read): `world_env {wind: {direction, strength 0..4,
  frequency, gust, ...}}`, deterministic in simulation time, used only by shaders to sway
  foliage. Nothing in the simulation reads it today.
- **Light** (docs/design/LIGHT.md, read in part): no quality preset; the engine measures.
  Bloom exists (an emissive above 1 blooms). I found **no volumetric pass, no screen-space
  distortion or heat haze, and no refraction outside water** (`grep` for refract, distort,
  volumetric over `crates/specula-gpu`: only `shaders/water.frag`). World fog exists
  (`FogSettings`: density, height falloff, sun scatter).
- **Simulation order** (`crates/specula/src/lib.rs`, `advance_clock_ticks`, read): each tick runs
  the game, the modules, the game after modules, live Lex, timelines, navigation, physics, then
  tells the modules what physics saw (heard next tick), then records effects' tracks
  (read-only), then hashes. **Saves** (docs/design/SAVES.md, read) have one section per system
  (`physics`, `logic`, `nav`, `lex_live`, `sounds`), each versioned.
- **Modules** (`crates/specula-sdk/src/module.rs`, `crates/specula/src/logic/handler.rs`, read):
  every SDK call is a request with an `op` on one channel (`ask`); the engine answers a list of
  ops and refuses an unknown one with "is the module's SDK newer than the engine?". **A new op
  is not a change of the ABI**: `sound` was added that way.
- **The pattern to follow** (docs/design/FORMAE.md, `crates/specula-formae/src/table.rs`, read):
  an entry is a row with a name, family, one line, parameters (name, words, unit, default, min,
  max, integer), aliases; `nearest` by edit distance; `formae_list` answers every name by family
  in under 3 KB; `formae_get` one entry under 3 KB with snippets to paste; counts exact before
  building; a test runs over the whole table so a new entry is tested by being there; a test
  holds the document's table to the crate's.
- **How Sepulcra made its effects** (`games/sepulcra-app/src/scene_vfx.rs`,
  `games/sepulcra/assets/recipes/brazier_fire.json`, read): each effect by hand for that game:
  transient entities with `vfx` and `decal` components, a list of one-shots the game deletes
  when their time is up (at most 64), sounds played separately by name, colours from STYLE.md
  passed as tints. The palette in `games/sepulcra/assets/style.json` already names its fire
  colours `fire_core`, `fire_flame`, `fire_ember`, which is the convention this design takes up.

## What an entry is

An entry is **a name, changed by numbers**: one row of a table in `specula-effectus`, as a form
is in `specula-formae`.

| Field | What it is |
|---|---|
| `name` | snake_case, unique over names and aliases (`explosion`) |
| `family` | what it is listed with (`fire`, `explosion`, `lightning`, ...) |
| `about` | one line |
| `args` | each with `name`, a few words, `unit` (`m`, `s`, `m/s`, `deg`, `count`, `ratio`, `hz`, `db`, `colour`, `bool`), `default`, `min`, `max`, `integer` |
| `roles` | the colours it is drawn in, by role (`core`, `flame`, `ember`, `smoke`), each with a default sRGB colour (below, "The look is the project's") |
| `aliases` | other words, perhaps with numbers set (`torch` is `fire` with `size: 0.15`; `bonfire` with `size: 1.6`; `boom` is `explosion`) |
| `kind` | `loop` (plays until removed: a torch), `oneshot` (ends by itself: a blast), `beam` (from here to a target: a laser) |
| `parts` | which parts it can make: particles, light, decal, sound, behaviour |
| `behaviour` | `none`, or which Elementa behaviour it can start and the args that start it (`spread`) |
| `compose(args) -> Composite` | the generator |

`Composite` is everything one play of the entry needs, all of it data the engine already takes:

- `effect`: a Scintilla `Effect` (particles and the light), validated by `Effect::parse` like any;
- `decal`: an optional `DecalSpec` with its `life` and fades (a scorch, frost, a splat);
- `sound`: an optional sound recipe (JSON, the format `specula-audio` reads), generated from the
  args (a larger explosion is lower and longer), or the name of the project's own recipe;
- `shake`: an optional number, metres of camera shake it would suggest, **data only**: the
  camera is the game's, so the engine never moves it (a game may read it);
- `behaviour`: an optional Elementa start (`ignite {radius, heat}`, `chill`, `wet`, `blast`,
  `strike`, `wind_zone`), only when the caller asked for one (below).

**Counted before it is built.** `cost(args)` answers, without composing the frames: the peak of
live particles (`Effect::peak` × rate), the CPU work units at that peak (`work_per_particle`),
the fill at the reference view (`fill_area`, as `vfx_check` does), lights (0 or 1), decals (0 to
2), **draw calls added: 0** for particles (they join the one instanced draw) and at most one
decal batch, the sound's length and its render cost, the flash rate and the flash energy of its
onset (for the governor, below), and for a behaviour its bounds (cells, work per tick). The
same numbers come from the composed effect, and a test holds them equal.

**Every entry is deterministic**: the same args give the same bytes on every machine; the crate
uses only `+ − × ÷`, `sqrt` and `dmath`, and carries the determinism `clippy.toml` (it is listed
under `SIMULATION` in `crates/specula/tests/determinism_lints.rs`, as `specula-formae` and
`specula-scintilla` are: the safe side, since it is pure).

**The eight built-ins become entries.** `fire`, `embers`, `smoke`, `motes`, `trail`, `dust_puff`,
`impact` and `sparks` are entries of the table whose composition **at default args is byte for
byte the effect `library.rs` holds now**, held by a test. So Sepulcra, every saved world and
`vfx_check.py`'s hashes see nothing change, and the old names gain args.

## The first table

Common to every entry (not repeated in the rows): `size` (a multiplier on its natural size,
ratio 1, 0.1..10; the rows give the natural size), `intensity` (ratio 1, 0..4: emissive and the
light), `seed` (count 0, 0..2^32−1), `sound` (bool, true), `volume` (db 0, −40..6), `light`
(bool, true), one colour per role, and `plane` (bool, false: the 2D variant, below). Loops take
`duration` (s, none = until removed, 0.1..3600).

Parts: **P** particles, **L** light, **D** decal, **S** sound, **B** behaviour it can start.
"Needs" names the Scintilla changes of the build plan (E1 to E8, below) an entry waits for; an
entry with no need can be built on Scintilla as it is.

| Family | Entry (aliases) | Kind | Its own args (unit default, range) | Parts | Behaviour | Needs |
|---|---|---|---|---|---|---|
| fire | `fire` (torch, campfire, bonfire, brazier) | loop | `radius` m 0.16 (0.03..5), `height` ratio 1 (0.3..3), `smoke` ratio 0 (0..1), `embers` ratio 0.3 (0..1), `flicker` ratio 0.22 (0..0.6) | P L S | ignite (`spread`) | none |
| fire | `burning` | loop | `extent` m [1,1,1] (0.1..20 each), `fuel` ratio 1 (0..1: how fierce) | P L S | (drawn by Elementa for a burning thing) | none |
| fire | `fireball` | loop | `radius` m 0.3 (0.05..3), `trail` s 0.6 (0..2) | P L S | ignite on `impact` | none |
| fire | `candle` | loop | `height` m 0.03 (0.01..0.2) | P L | none | none |
| fire | `embers` | loop | `rate` /s 5 (0.5..60), `rise` m/s 0.8 (0..4) | P | none | none |
| explosion | `explosion` (blast, boom, grenade) | oneshot | `radius` m 2 (0.2..30), `smoke` ratio 0.6 (0..1), `debris` count 24 (0..200), `fireball` ratio 1 (0..1), `shockwave` bool true, `scorch` bool true | P L D S | `blast` (push, heat, event) | E2 for the ground ring |
| explosion | `firework` | oneshot | `radius` m 6 (1..30), `stars` count 80 (10..300), `trail` s 0.8 (0..2) | P L S | none | none |
| explosion | `energy_burst` (magic_burst) | oneshot | `radius` m 1.5 (0.2..10) | P L S | none | E2 |
| smoke | `smoke` | loop | `rate` /s 4 (0.5..40), `darkness` ratio 0.5 (0..1), `rise` m/s 0.4 (0..3), `spread` m 1.5 (0.2..10) | P | none | E8 for wind drift |
| smoke | `smoke_puff` | oneshot | `radius` m 0.6 (0.1..5) | P | none | none |
| smoke | `steam` | loop | `rate` /s 6 (0.5..40) | P S | none | none |
| smoke | `smoke_column` | loop | `height` m 12 (3..60) | P | none | E8 |
| lightning | `lightning` (thunder, strike) | oneshot | `from_height` m 30 (5..300), `branches` count 3 (0..8), `strikes` count 2 (1..3), `jag` ratio 0.25 (0..0.6), `thunder` bool true | P L D S | `strike` (conducts, ignites) | E1, E4 |
| lightning | `arc` (zap, electric) | beam | `jag` ratio 0.3 (0..0.6), `reshape` hz 12 (1..30), `width` m 0.03 (0.005..0.3) | P L S | `strike` per contact, optional | E1, E4 |
| lightning | `spark_shower` | oneshot | `count` count 40 (5..200), `speed` m/s 3 (0.5..12) | P S | none | none |
| beam | `laser` | beam | `width` m 0.02 (0.002..0.5), `glow` ratio 1 (0..3), `impact` bool true, `hum` bool true | P L S | heat at the end, optional | E1, E4 |
| beam | `bolt` (blaster, tracer) | loop (a projectile) | `length` m 0.8 (0.1..5), `width` m 0.04 (0.005..0.3) | P L S | none | E1 |
| cold | `frost` | loop | `extent` m [1,0.2,1] (0.1..20 each), `glints` ratio 0.5 (0..1) | P D | `chill` | E5 |
| cold | `freeze_burst` (ice_blast) | oneshot | `radius` m 1.5 (0.2..10), `shards` count 30 (0..200), `mist` ratio 0.6 (0..1) | P L D S | `chill` | E5 |
| cold | `ice_shards` | oneshot | `count` count 20 (1..200) | P S | none | E5 |
| cold | `breath` (cold_breath) | oneshot | `length` m 0.6 (0.1..3) | P | none | none |
| wind | `wind_gust` | oneshot | `direction` deg 0 (0..360), `length` m 8 (1..50), `streaks` count 30 (0..200), `debris` bool true | P S | `push` (a gust) | none |
| wind | `whirlwind` (dust_devil, vortex) | loop | `radius` m 1.5 (0.3..10), `height` m 5 (1..30), `turn` hz 0.8 (0.1..3) | P S | `wind_zone` (swirl) | E3 |
| wind | `leaves` | loop | `rate` /s 3 (0.2..30), `area` m 10 (1..50) | P | none | E5, E8 |
| sparks | `sparks` (welding) | loop | `rate` /s 18 (1..200), `speed` m/s 3 (0.5..12) | P | none | none |
| sparks | `metal_hit` | oneshot | `count` count 20 (1..150) | P L S | none | none |
| weather | `rain` (drizzle, downpour) | loop | `rate` ratio 1 (0.1..4), `area` m 20 (5..60), `angle` deg 0 (−45..45), `splashes` bool true | P S | `wet` over its area | E8 |
| weather | `snow` (snowfall, blizzard) | loop | `rate` ratio 1 (0.1..4), `area` m 20 (5..60), `drift` m/s 0.5 (0..10) | P S | `chill` over its area, optional | E8 |
| water | `splash` | oneshot | `radius` m 0.5 (0.05..5), `ripple` bool true | P D S | `wet` | E2 |
| water | `drips` | loop | `rate` /s 2 (0.1..20), `height` m 2 (0.2..20) | P S | none | none |
| water | `mist` (spray, waterfall_mist) | loop | `extent` m [2,1,2] (0.2..30 each) | P | none | none |
| dust | `dust_puff` | oneshot | `radius` m 0.35 (0.05..5) | P | none | none |
| dust | `dust_cloud` | oneshot | `radius` m 3 (0.5..20) | P S | none | none |
| dust | `debris` (rubble) | oneshot | `count` count 20 (1..150), `chunk` m 0.08 (0.01..0.5) | P S | none | E5 |
| dust | `footstep` | oneshot | `ground` ratio 0 (0 dust .. 1 snow) | P S | none | none |
| magic | `motes` (aura, glimmer) | loop | `radius` m 0.55 (0.1..5), `rate` /s 7 (0.5..60) | P | none | none |
| magic | `portal` | loop | `radius` m 1 (0.2..6), `turn` hz 0.4 (0.05..2) | P L S | none | E2, E3 |
| magic | `charge_up` | oneshot | `radius` m 1 (0.2..5), `time` s 1 (0.2..5) | P L S | none | E3 |
| magic | `teleport` | oneshot | `height` m 2 (0.5..5) | P L S | none | E2 |
| impact | `impact` (hit) | oneshot | `radius` m 0.3 (0.05..3) | P L S | none | none |
| impact | `bullet_hit` | oneshot | `surface` ratio 0 (0 stone, 0.33 metal, 0.66 wood, 1 dirt) | P D S | none | E5 |
| impact | `blood_hit` | oneshot | `amount` ratio 0.5 (0..1), `splat` bool true | P D S | none | E5 |
| impact | `slash` (swipe) | oneshot | `arc` deg 120 (20..300), `radius` m 1 (0.2..4) | P S | none | E5 |
| heal | `heal` | oneshot | `height` m 1.8 (0.3..5) | P L S | none | none |
| heal | `buff` (power_up, level_up) | oneshot | `radius` m 0.8 (0.2..4) | P L S | none | E2 |
| heal | `shield` (bubble, barrier) | loop | `radius` m 1.2 (0.2..8) | P L S | none | E5 |
| heal | `poison` (toxic) | loop | `rate` /s 4 (0.5..30) | P | none | none |
| trail | `trail` (ghost_trail) | loop | `width` m 0.4 (0.05..3), `life` s 1 (0.2..4) | P | none | none |
| trail | `projectile_trail` | loop | `width` m 0.08 (0.01..1), `life` s 0.5 (0.1..3) | P | none | none |
| ambient | `fireflies` | loop | `area` m 6 (1..40), `count` count 20 (1..200) | P L | none | none |
| ambient | `pickup_sparkle` (sparkle, coin) | oneshot | `count` count 16 (1..100) | P S | none | none |

That is 51 entries in 16 families, 18 of them buildable on Scintilla as it is. Every entry is
cosmetic unless its caller asks for its behaviour; the behaviours are seven: `ignite`, `chill`,
`wet`, `blast`, `strike`, `push`, `wind_zone` (Elementa, below).

**Left out, and why:**

- **Fluids** (pouring water, lava flows, blood pools spreading): no fluid simulation in the
  engine, and none planned in M6. A splash, drips and mist are particles; a pool is a decal.
- **Volumetric smoke, clouds, god rays and fog banks that light passes through:** the renderer has
  no volumetric pass (I found none). Smoke is lit camera-facing particles; a "fog bank" made of
  many large soft particles costs fill the iGPU does not have, so it is not offered; world fog is
  `world_env`'s.
- **Heat haze, shock distortion, refraction:** no screen-space distortion pass exists. A
  shockwave is a ring (E2), not a ripple of the image behind it.
- **Mesh effects** (a dome of energy as a mesh, a tornado as a mesh): Effectus composes what
  Scintilla draws. A `shield` is a camera-facing bubble texture, which reads as a sphere from
  every side; a mesh dome is a model recipe's work.
- **Destruction and fracture:** physics and models (part 5's libraries). `debris` is cosmetic
  chunks that die on the ground.
- **Ribbons** (continuous strips for sword trails): the drawing path has none (VFX.md, seams).
  `slash` is a crescent texture, trails are particles, beams are stretched segments (E1).
- **Gore:** no dismemberment, no gibs. `blood_hit` is droplets and a splat whose colour is an arg
  (green for a beetle, ink for a squid); whether it ships at all is a question for the owner
  (below).
- **Screen effects** (a white-out flash of the whole screen, a camera shake): the frame's image
  and the camera belong to the game. An entry's `shake` is data a game may use; a full-screen
  flash is never offered (photosensitivity, and the game's UI owns the screen).
- **Nuclear-size explosions, forest-wide fire as one effect:** past the budgets of the reference
  device. A large fire is many burning things drawn by Elementa within the budget, nearest first.

## Cosmetic and behaviour: designed apart, offered under one name

**The line.** Anything an entry draws or plays is cosmetic: it is never hashed, never written
back, and the same world at the same time draws it the same (VFX.md, "Cosmetic, yet
reproducible"). Anything that changes over time and that the rules can see (a thing burning, a
thing burnt out, a frozen pond, a body pushed) is behaviour: Elementa's state, in the simulation.
**An entry never starts a behaviour unless asked**: `fire` draws flames; `fire` with
`spread: true` (or a module's `ignite`) also starts heat in Elementa. A designer who wants a
pretty brazier never meets the simulation; one who wants a burning forest asks for it by one arg.

**Elementa's model: one temperature, fuel, wetness, and two impulses.** Rather than a system per
element, Elementa keeps for each thing it tracks (a **node**):

| State | Unit | What moves it |
|---|---|---|
| `heat` | ratio, −1 (frozen solid) .. 0 (ambient) .. 1 (white hot) | burning, a blast, a strike, a laser (up); cold, wet, time back towards 0 |
| `fuel` | s of burning left | burning (down) |
| `wet` | ratio 0..1 | rain, a splash, water near (up); heat and time (down) |
| `phase` | `intact`, `burning`, `burnt`, `frozen` | crossing a material's thresholds |
| `since` | tick | when the phase began (for the look and for events) |

So **fire and freezing are one field** (heat above a material's `ignite` with fuel left burns;
below its `freeze` it freezes, for materials that can), fire melts ice and cold puts fire out
without a rule written twice. Two impulses act once rather than over time: a **blast** (heat in a
radius and a push to bodies) and a **strike** (lightning: heat at a point, carried through
conducting nodes). **Wind** is a vector field: the world's wind (`world_env.wind`, read from the
world, so world data, journaled and hashed) and wind zones (entities with an `element` component
of kind `wind`).

**What it reads from the world, and how a project says so.** A project's `elements.json` (a new
project file kind, validated on write, as `sounds/*.json` are) names its materials and which
things are made of them:

```json
{
  "elements": 1,
  "materials": {
    "wood":  {"ignite": 0.35, "burn_s": 40, "heat_out": 0.6, "conducts": false},
    "grass": {"ignite": 0.2,  "burn_s": 6,  "heat_out": 0.4},
    "water": {"freeze": -0.5, "quench": 1.0},
    "metal": {"conducts": true}
  },
  "tags": {"tree": "wood", "crate": "wood", "pond": "water", "fence_iron": "metal"},
  "cell_m": 0.5, "spread_hz": 10,
  "limits": {"nodes": 4096, "burning": 512, "work": 32768}
}
```

Built-in materials (wood, grass, cloth, oil, flesh, stone, metal, water, ice, snow) need no
listing; a project overrides any number. A thing joins Elementa by a tag the file maps, or by an
`element` component on the entity (`"element": "wood"`, or `{"material": "wood", "fuel_s": 90}`),
both world data. Its node's position and size are its entity's world bounds, refreshed from the
world's change feed (only entities that moved), never by walking the world each tick (rule 15).
A thing with no material is not there for Elementa: stone walls stop fire by not being fuel.

**Spreading** runs `spread_hz` times a second (10 by default, a whole number of ticks): each
burning node gives heat to nodes within its reach (its size plus `cell_m`), falling off with
distance, more downwind (the wind's component along the gap) and upward (+Y, the same in 2D and
3D); wetness takes heat first. Neighbours come from a grid of cells (`BTreeMap` keyed by cell,
nodes in id order). Ignition is a threshold, not a dice roll; where chance is wanted (an ember
jumping a gap), it comes from Elementa's own seeded stream, one draw per (node, step) in id
order. All arithmetic in `f32` with `dmath`; containers ordered; the crate carries the
determinism lints.

**Bounded, and said before it runs.** `elements.json` limits: nodes tracked (4,096 by default,
16,384 at most; past it the newest by id are left out and the count is reported), nodes burning
at once (512 by default; a node that would ignite past it waits in id order), work per step
(32,768 neighbour checks by default; a step that would pass it is cut at a node boundary in id
order and the rest carried to the next step: still deterministic, and reported as `deferred`).
Each limit is checked when the file is written, with the worst case it allows (nodes ×
neighbours), and `elementa_check` answers the cost of a scenario before it is run (below).

**State, save and hash.** Elementa's state (nodes, phases, fuel, heat, wet, since, its stream,
its step counter) is a save section `elementa` with its own version, its own subsystem hash in
the save header, and is part of the tick hash. A load restores it whole and the run goes on bit
for bit (a test saves at tick T, loads, and compares hashes at T + 600 with an unbroken run).

**Where it runs.** After physics in each tick (it reads where things ended up) and before the
tick hash; what it has to tell the rules is heard at the next tick, as physics' events are
(`tell_modules_of_physics` is the model).

**What it writes.** In the first stages, **nothing in the world**: its state is its own, and the
world changes only by what the rules do with its events. Two things act outside its state:
pushes to dynamic bodies (a blast, a gust, a wind zone), through the physics bridge as
`Cx::push` already does; and, in a later stage and only if the lead or owner wants it, an
opt-in `on_burnt: "delete" | "keep"` per material, which would be a named exemption of the
write policy (`Exempt::Elementa`, a row in the regression table: AGENTS rule 13). I would rather
leave destruction to the rules (question 1, below).

**How rules hear it.** Events to modules, at the next tick: `element.ignited {id, at}`,
`element.burnt {id}`, `element.extinguished {id, by}`, `element.frozen {id}`,
`element.thawed {id}`, `element.shocked {id, amount}`, `element.blast {id, distance, push}`
(at most 64 named per event kind a tick, the rest counted). A module asks a node's state with a
new request op `element` (`Cx::element(id)`), and starts behaviour with ops `ignite`, `chill`,
`wet`, `blast`, `strike` (`Cx::ignite(at, radius, heat)` and so on), each inside its tick, as
`Cx::push` is. **What a frozen or burning creature does is the rules' decision**: Elementa tells
them it is in a `frozen` cell; a Lex `effect` such as `slow` or a module's own status does the
rest.

## Elementa as built (stage 5, the pure crate)

Built in `crates/specula-elementa` (lane `m6-elementa`, 2026-11-24): std and `specula-core`
only, `[lints] workspace = true`, the determinism `clippy.toml`, listed under `SIMULATION`.
Nothing of the engine calls it yet (stage 6). What the note above left open, decided here:

**Changed from the note, and why.**

- **`cell_m` and `reach_m` are two numbers.** The note's "reach is its size plus `cell_m`" gave
  one number two jobs. `cell_m` (default 2 m, 0.25..64) is the neighbour grid's cell;
  `reach_m` (default 0.5 m, 0..`cell_m`) is the gap between two things' boxes that heat
  crosses. Holding the reach within one cell is what keeps a query inside its cells (below).
- **Nodes: 32,768 at most**, not 16,384 (the default stays 4,096): the forest this stage was
  asked to hold is 20,000 nodes. The worst case at 32,768 is said before a run (below).
- **Past the node limit**, `add` refuses the node with the limit's number (`limit`, `max`,
  `asked`) and `add_many` keeps the lowest ids of the batch and reports how many were left
  out. Both count into `refused`, which is saved and shown.
- **Acts in stage 5: `ignite`, `chill`, `wet`**, on a node or a sphere (radius up to 64 m), at
  most 64 between two ticks. `blast`, `strike`, `push` and wind zones are stages 9 and 10; the
  materials already carry `conducts` for the strike. Water does not wet its neighbours yet
  (stage 8: rain wetting its area).
- **A node's box is its entity's world bounds** (centre and half size), not a sphere: gaps are
  box to box, so a long log and a wall are what they look like.

**The state and the step.** Each node keeps `heat` (−1..1), `fuel` (seconds), `wet` (0..1), a
phase (intact, burning, burnt, frozen) and `since` (the tick it began). The world gives a wind
(m/s; `world_env.wind.strength` 1 is 4 m/s, question 6) and an ambient heat (−1..1; below the
freezing line of a material it freezes). `step()` is called every tick; every
`tick_hz / spread_hz` ticks (6 at 60 Hz) a spread step of `dt = 1 / spread_hz` s runs:

1. **Heat given.** Each burning node, in id order from a cursor, gives each neighbour within
   the reach `heat_out × (1 − gap / reach) × lean × dt`, where
   `lean = max(W_MIN, 1 + K_WIND × (wind · u) + K_UP × u.y)` and `u` is the unit vector from
   the burning centre to the neighbour's. Gains are summed first and applied after, so no
   node's place in the order changes another's result. A burning node of a material with
   `embers` throws, with chance `embers × dt` (one draw of the run's stream per node and step,
   only in a wind of at least 1 m/s), an ember downwind past its box by
   `reach + U(0,1) × 3 m × min(wind / 4 m/s, 4)`; it gives 0.25 heat to what it lands in.
2. **Taken in.** Wetness takes heat first (`LATENT`: 0.5 heat dries wet 1 to 0); the rest
   raises heat, to at most 1. Ice's water is held: a frozen node warms without drying.
3. **What ends**, in id order: a burning node at wetness 0.5 or more goes out (`by: wet`),
   below its `ignite` goes out (`by: cold`; it keeps its fuel), with no fuel left burns out;
   an intact node at or below its `freeze` freezes; a frozen one above `freeze + 0.1` thaws.
4. **What catches**, in id order: an intact node with fuel, heat at or above its `ignite` and
   wetness at most 0.05, while fewer than `limits.burning` burn; the rest wait (counted).
5. **Time.** A burning node burns `dt` of fuel, moves `min(1, 2 × dt)` of the way to its
   `flame` heat and dries 0.1 a second; any other moves `min(1, cool × dt)` of the way to the
   ambient heat and dries `1/600 + 0.05 × max(heat, 0)` a second, never below its `quench`.
6. **Events**, sorted by node id and then kind (ignited, burnt, extinguished, frozen, thawed),
   queued for the engine to take; at most 64 of a kind named, the rest counted.

So the time for heat to cross one gap is a recurrence, not a guess: `steps_to_ignite(from, to)`
runs the same arithmetic in the same order and a run equals it exactly (the line test holds
every one of 40 gaps to it, both ways).

**Elementa's numbers.** None is measured from real materials; each is the engine's own, chosen
so that a fire reads at game speed and an agent can predict it. The shape of the wind's effect
follows what fire science reports (a head fire runs many times faster than a fire backing into
the wind; Rothermel's 1972 spread model is the usual reference), but no number is taken from it.

| Constant | Value | Why this value |
|---|---|---|
| `K_WIND` | 0.25 per m/s | a 4 m/s wind (strength 1, a breeze) doubles the heat given downwind |
| `W_MIN` | 0.1 | a fire backing into a strong wind still creeps, at a tenth |
| `K_UP` | 0.5 | heat rises: a thing above gets 1.5 times, below 0.5 times (+Y up, 2D and 3D alike) |
| `LATENT` | 0.5 | a soaked thing takes 0.5 heat to dry, more than catching dry wood takes (0.35): wet visibly resists |
| `WET_BLOCK` | 0.05 | damp is not wet: a nearly dry thing catches |
| `DOUSE` | 0.5 | half soaked puts a fire out |
| `DRY_BASE`, `DRY_HOT` | 1/600 s⁻¹, 0.05 s⁻¹ per heat | wet dries in ten minutes alone, in seconds beside a fire |
| `BURN_DRY` | 0.1 s⁻¹ | a fire dries itself; a douse under 0.5 is beaten in a few seconds |
| `FLAME_RATE` | 2 s⁻¹ | a burning thing settles at its flame heat in about a second |
| `THAW_MARGIN` | 0.1 | a thing at its freezing line does not freeze and thaw every step |
| ember heat, range, wind | 0.25, 3 m, 1 m/s | one ember warms; two close together light dry wood; calm air carries none |

The built-in materials (a project overrides any field and adds its own; a new burning material
with no `flame`, `burn_s` or `heat_out` gets midway to 1, 10 s and 0.5):

| Material | ignite | burn_s | heat_out | flame | cool | embers | conducts | freeze | quench | starts at |
|---|---|---|---|---|---|---|---|---|---|---|
| cloth | 0.25 | 12 | 0.5 | 0.7 | 0.2 | 0.03 | | | | 0 |
| flesh | 0.6 | 8 | 0.2 | 0.7 | 0.3 | | | | | 0 |
| grass | 0.2 | 6 | 0.4 | 0.6 | 0.2 | 0.02 | | | | 0 |
| ice | | | | | 0.02 | | | −0.2 | 1 | −0.6 (frozen) |
| metal | | | | | 0.3 | | yes | | | 0 |
| oil | 0.15 | 20 | 1.0 | 0.95 | 0.1 | | | | | 0 |
| snow | | | | | 0.05 | | | −0.1 | 0.5 | −0.4 (frozen) |
| stone | | | | | 0.05 | | | | | 0 |
| water | | | | | 0.1 | | yes | −0.5 | 1 | 0 |
| wood | 0.35 | 40 | 0.6 | 0.8 | 0.1 | 0.05 | | | | 0 |

Ice in a world at ambient 0 thaws over a few minutes, as it would; a world that keeps it sets
its ambient heat below −0.2. Water never warms from a fire (its quench of 1 takes the heat as
drying), so a pond is never lit and never boils.

**`elements.json`**, as parsed: `{"elements": 1, "materials": {...}, "tags": {...}, "cell_m",
"reach_m", "spread_hz", "limits": {"nodes", "burning", "work"}}`. At most 64 KB (refused
before parsing), 64 materials, 1,024 tags, names of 1 to 32 bytes of `a-z0-9_`; every key
known (`unknown_key` lists the known ones), every number finite and in its range
(`out_of_range` names the path, the value and the range), a key given twice refused, a tag
naming no material refused, `"ignite": null` takes a threshold away, and a burning material's
`flame` must be above its `ignite` (or it would put itself out). The engine's tick rate must be
a whole multiple of `spread_hz` (1..60).

**Near, without hashing.** Space is cut into cubes of `cell_m`. Each node is entered in every
cell its box overlaps, at most 64 (refused at `add` as `too_large`, with the count, otherwise:
a tree 1 × 10 × 1 m covers at most 2 × 6 × 2 cells of 2 m); the entries are one `Vec` of
(cell, node) sorted by cell. A cell's nodes are a binary search away; a query walks its cells
in x, y, z order and counts a node met in several cells once, in the first cell the two boxes
share, so no set is needed. Since the reach is at most a cell, a burning node's query covers at
most `(a+2)(b+2)(c+2)` cells where `abc ≤ 64`, so at most 594 lookups. The grid is derived:
rebuilt (one sort) at the first step or act after a node is added, removed or moved, never
saved or hashed.

**What it costs for 20,000 nodes** (counted by the test `a_forest_of_20000_steps_within_the_
stated_work_bound`, never timed: a 141 × 142 m forest at 1 m spacing, wood 0.7 × 3 × 0.7 m and
grass 0.6 × 0.4 × 0.6 m, `reach_m` 1, a 4 m/s wind): 64,416 grid entries (3.2 a node; about
1 MB, and 1.1 MB of nodes); every spread step 20,000 node updates plus at most the work limit
(32,768); a burning node's heating costs about 97 lookups and checks in this forest, so the
default work limit heats about 340 burning nodes a step and the rest wait for the next (in
1,168 of 1,200 steps the budget or the burning limit of 512 was reached; the fire still spread
to over 3,000 things in two minutes). **Work** is cell lookups plus neighbour checks;
`StepReport` says each, and `work ≤ limits.work` every step is what the test asserts.

**Bounded, and said before a run.** `Config::worst_case()`: ops per spread step at most
`limits.nodes + limits.work` (52,768 at the default 4,096 and 32,768; 65,536 at 32,768 nodes);
grid entries at most 64 a node; memory at most about 60 bytes a node plus 16 bytes an entry (at
32,768 nodes, the worst 35 MB; the 20,000-node forest above about 2 MB). A step that would pass the
work limit stops at it, in the middle of a node's query if it must, and the next step starts
from the first burning node it did not reach (a cursor, saved): deterministic, and reported as
`deferred` and `truncated`. Between spread steps a tick does nothing (0 ops). An act's work is
at most the node count (a sphere with more cells than there are nodes looks at each node once).

**The prediction.** `Elementa::check(&Scenario {acts, wind, ambient, ticks, max_ops})` runs the
scenario on a copy (the state is untouched), at most 36,000 ticks and `max_ops` ops, and
answers `Prediction`: how many caught, burnt out, were put out, froze and thawed; up to 64 burnt
ids; the peak burning; the first and last ignition and when the fire went out; the ops spent,
the worst step, the steps cut, the most that waited; `bound_ops` (the steps times the worst
step, said before it runs); `cut` when `max_ops` stopped it; and the end state's hash. A test
holds that a run of the same scenario gives the same counts and the same hash.

**The save section `elementa`, version 1.** Self-contained: the config it ran under (every
material, tag, the grid, the limits), the tick rate, tick, the stream's two words, the cursor,
`refused`, wind, ambient, every node (id, material, centre, half, heat, fuel, wet, phase,
since) in id order, and the events still waiting. Floats are their bits. On load: every count
against its cap and the bytes left before anything is allocated (`r.count`), the config by the
same `validate` a file passes, ids strictly rising, materials in the table, every number in its
range, a phase its material allows (only what can burn burns, only what can freeze is frozen),
no `since` or event after the save's tick, at most the burning limit burning, at most 64 named
events a kind, nothing left over. A load saves to the same bytes and continues to the same hash
(tested 600 ticks past a save taken mid-fire). The fuzzing test cuts the section at every
length, flips every bit (and runs 60 ticks after every seventh flip that loads), and makes
3,000 random edits with extreme values (stepping every edit that loads); nothing panics or
hangs, and a count of `u32::MAX` is refused before allocation.

**The hash** (`Elementa::hash`) covers the config's hash, tick rate, tick, stream, cursor,
`refused`, wind, ambient, every node's every field, and the waiting events. Tested: the same
seed and inputs give the same hash at each of 10,000 ticks, twice, and with the nodes added in
a batch, one at a time in reverse, and in interleaved batches; another seed differs.

**For the engine's lane (stage 6).** What it calls, all in `specula_elementa`:

- *Setting up.* `Config::parse(&text)` for `elements.json` (or `Config::default()`), then
  `Elementa::new(config, tick_hz, seed)`. `config.tag(tag)` and `config.material(name)` map an
  entity to a material index. `config.worst_case()` and `elementa.info()` (JSON, a few hundred
  bytes) for `elementa_info`.
- *The change feed.* `add(NodeDesc::new(entity_id, material, centre, half))` (with
  `.fuel(s)`, `.heat(h)`, `.wet(w)` from an `element` component), `add_many(&[...])` at a load
  or a project start, `move_node(id, centre, half)` for an entity that moved, `remove(id)` for
  one deleted. Errors are `ElementaError` (`code`, `message`, `details`; `to_json()`).
- *The world.* `set_wind(Vec3)` in m/s (strength × 4) and `set_ambient(h)` whenever
  `world_env` changes.
- *Module ops.* `act(Act { kind: ActKind::Ignite | Chill | Wet, target: Target::Node(id) |
  Target::Sphere { at, radius }, amount })` inside the tick that asks, as `Cx::push` is; the
  answer's `ActReport` says how many it reached. `node(id)` answers `Cx::element(id)`.
- *The tick.* `step()` once per tick, after physics and before the tick hash; it answers a
  `StepReport` (for traces and `perf_frame`). Then `take_events()`: `Events { list, unnamed }`,
  each `Event { tick, kind, id, at, by }`, `kind.name()` is the module event's name
  (`element.ignited`, ...) and `Event::to_json()` its data; tell the modules at the next tick.
- *The hash.* `hash()` into the tick hash and the save header's subsystem hashes.
- *The save.* `save_state(&mut ByteWriter)` and `Elementa::restore_state(&mut ByteReader)`
  (or `save()` and `load(&bytes)` for a whole section), section version `SECTION_VERSION` (1).
  A load replaces the whole state, its config included; the engine compares the saved config's
  hash with the project's file and says so if they differ.
- *The prediction.* `check(&Scenario)` on a clone, on a worker (`Engine::offload`); it never
  reads the world. `check_bound(ticks, acts)` is the ceiling to say before it runs.
- *What the engine must decide:* whether a moved entity's node is moved every tick (each move
  rebuilds the grid at the next step: one sort of all entries; batch the moves), and the order
  the change feed adds nodes in (any order gives the same hash, below the node limit; past it
  `add_many` keeps the lowest ids, `add` the first given).

**Not done in stage 5, or unsure.** Insertion order changes nothing below the node limit; past
it, single `add`s keep what came first, so an engine that adds in id order should use
`add_many`. A burning node's heat to a node above it uses only the centres' direction, so a
wide flat thing below a tall one leans less than it might. Burnt nodes stay tracked (counting
against the node limit) until the engine removes them. Nothing here was run inside the engine.

## The look is the project's

As a recipe takes `style.json`'s palette and materials by name, an entry takes its colours by
**role**:

1. a colour given in the call's args (`"core": "#9cff4f"`) wins;
2. else the project palette's colour named `<family>_<role>` (`fire_core`, `fire_flame`,
   `fire_ember`: Sepulcra's palette has exactly these, so its fire is its own without a word), or
   a role map in `style.json`'s new optional `effects` section (`"effects": {"magic": {"core":
   "rune_glow"}}`) for palettes that name colours otherwise;
3. else the entry's own default.

`vfx_check`'s `palette` check runs on every composed effect against the project palette when
there is one, so an entry drawn in colours outside the style is a warning with the colour named.
Sizes are metres, the project's unit. A project that wants a different *shape* of fire saves
its own entry (below); it never edits the library.

## Budgets and safety

- **Before spawning.** `cost(args)` (above) is answered by `effectus_get` and by every spawn's
  answer. The table's test holds every entry, at its default args and at every corner of its args
  box, inside `[vfx] max_per_effect` (1,500 particles) and the `overdraw` fill cap at the
  reference view; most entries sit far under (an `explosion` at default is about 140 particles at
  its peak: fireball 40, smoke 24, debris 24, sparks 40, ring 1, glow 1; to be measured).
- **In the frame.** Effectus adds nothing to the frame's budgeting: what it makes is Scintilla
  effects, cut by the existing `[vfx]` budgets, LOD and fill cap, nearest and highest priority
  first. Lights stay within `[vfx] lights` (4 of 16), nearest first. Particles add no draw call.
  Elementa's look (every burning thing drawn as `burning`) is instances like any other and is
  thinned the same way: a forest fire on the iGPU shows its nearest fires whole.
- **Photosensitivity, per entry.** `flash::flash` already refuses at parse time an effect that
  flashes more than 3 times a second at 3 m. Every entry is composed and checked at its defaults
  and at every corner of its args box by a test, so no args an agent can give make an entry
  that flashes. Where real things flash faster (lightning's return strokes, an arc's flicker),
  the entry is built to the rule: `strikes` is at most 3 and the strokes are at least 0.34 s
  apart; an `arc` reshapes its path up to 30 times a second but keeps its brightness steady and
  its area under the significant change, which the per-entry check measures.
- **Photosensitivity, in combination** (many flashes at once). The per-effect check cannot see
  several instances (VFX.md says so); the flash watch sees the final image but only warns.
  Effectus adds a **flash governor** for one-shots: each entry's onset has a known flash energy
  (the fraction of the significant change its first transition makes at the reference view: a
  small addition to `flash.rs`, which today answers a rate and a cause, not this fraction, from
  the same timelines); one-shots started in any one second add their
  energies, and a cue that would take the sum past the equivalent of three flashes is played
  with its flash parts (the light and the additive brightening) dimmed to what is left, its
  particles of other kinds, decal and sound unchanged, and reported (`flash_governed` in
  `vfx_info` and the frame's problems). Deterministic: cues arrive on ticks, in order. The flash
  watch stays the last word on the image.
- **The low end.** The reference is the Intel iGPU: `max_particles` 4,000 and `fill_cap` 1.5
  screens there. Weather is the hard case: `rain` follows the camera in a box of `area` metres and
  its default rate is set so that it fits in a third of the integrated budget at 720p (to be
  measured, and lowered if it does not).

## 2D and 3D

The same entry plays in a 2D game and a 3D one; `plane: true` asks for the 2D variant (and a
project whose game file says it is 2D gets it by default; question 7, below). What I read and
what follows from it:

- **Axes.** Both use +Y up (two_d.rs: 2D lives in the XY plane, y up, seen down −Z), so gravity,
  rising smoke and falling sparks are the same numbers.
- **What differs.** A 3D burst throws particles in every direction; seen down −Z, the part of
  their speed along Z is lost and the burst looks smaller and slower. The 2D variant keeps every
  spawn point and velocity in the local XY plane (E7, a small change to the evaluator). Spawn
  shapes in the XZ plane (`disc`) are turned into the XY plane where the entry means a ground
  disc seen from the side (a fire's base is a line in 2D, which is right).
- **Depth.** In 2D everything sits near z = 0. Particles are drawn after the scene with their own
  depth test; cutout sprites write depth and blended ones do not (sprite.rs). So an effect at
  z slightly behind a cutout layer is hidden by it, and an effect is always drawn over blended
  sprites whatever their layer. The 2D variant sets `soft` to 0 (a soft edge measured against a
  coplanar sprite fades the particles: the probe at the end saw it).
- **Decals.** The decal pass is 3D only. In the 2D variant an entry's decal becomes a flat
  particle in the XY plane (E2) with the decal's life, up to Scintilla's 10 s; a mark that
  must stay longer is a sprite the game places.
- **Light.** An entry's light lights only sprites that are `lit` (sprites are unlit by default:
  sprite.rs). The 2D variant keeps the light and says so in its answer.

**Probe (2026-11-24, this lane):** see "What the probe showed", at the end.

## How an agent uses it

**Find.** `effectus_list` with nothing asked: every name by family, a line a family, and how to
ask for more, **in under 3 KB** (the table above is about 2 KB of names). With `family`, a
`word` or `rows: true`: rows with each arg, unit, default and range, and the snippet to paste,
at most 8 KB a page. `effectus_get {name, args?, plane?}`: one entry under 3 KB: what it is, its
args with units and ranges, its parts, its cost at those args, and three snippets to paste (the
component for `world_spawn`, the module call, the file to save as the project's own); `full:
true` adds the composed Scintilla effect and sound recipe. **An unknown name answers with the
nearest** (edit distance over names and aliases, as `formae::table::nearest`), and a word that
is an alias answers as its entry with the numbers it sets. Both tools read nothing of the world,
run on the caller's protocol thread, and have answer ceilings (64 KB).

**Play.** Three ways, by need:

- *On a thing, until removed* (a torch on a wall): the `vfx` component with an entry and args,
  `{"vfx": {"effect": "fire", "args": {"radius": 0.1, "smoke": 0.3}}}`, through `world_spawn`,
  `world_set`, `world_batch` (journaled, claims, undo: as now). `args` is the component's new
  optional field; `params` stay as the overrides every effect understands.
- *Once, at a place* (an explosion where a shell lands): a **cue**, not an entity. A module
  calls `cx.effect("explosion", at, args)`; an agent calls `effectus_play {name, at, args}` (for
  looking, in a live world). A cue is presentation, as `Cx::sound` is: queued with the tick that
  asked, never hashed or journaled, its sound and decal played with it, ended by the engine when
  the entry's end time passes (no list of one-shots for a game to keep, as Sepulcra keeps
  today). A cue's sound is rendered off the engine thread like a module's; since a render more
  than 500 ms late is skipped (`module_sounds.rs`, `STALE`), an entry's sound at the args a
  project names (in its files, components and modules' last cues) is rendered ahead, when the
  project loads and when a file changes. The live cues are kept in a small save section (`effects`, at most 256), so a capture
  after a load shows what it showed before.
- *From here to there* (a laser at a target, an arc between two pylons): the component or the
  cue with `to: [x, y, z]` or `to_entity`; the beam follows a moving target.

**Cost.** `effectus_get {name, args}` answers the cost before anything is spawned; every spawn
and cue answer carries it again, and `vfx_check` / `vfx_preview` take `{effect, args}` as they
take a built-in's name now.

**Change and keep.** A project's own effects are **files**, `effects/<name>.json` (a new
project file kind, validated on write; found by name after the world's `/vfx` library and before
the table): `{"effectus": 1, "from": "explosion", "args": {"radius": 4}, "roles": {"core":
"#9cff4f"}, "sound": "my_boom"}` keeps an entry with its numbers (the library's later
improvements reach it); `{"effectus": 1, "effect": {...}}` keeps a whole Scintilla effect for a
look no entry gives. A file is what a cold builder writes cheapest (one write, no tool call),
and a human edits it as text (F5). `vfx_set` and `/vfx` stay as they are.

**Two worked examples, as an agent would type them.**

1. A cold builder of a 2D tower defence wants a hit, a death and a burning tower.

   ```text
   specula call effectus_list '{}'
     -> fire: fire torch candle embers burning fireball | explosion: explosion firework ...
   specula call effectus_get '{"name": "explosoin"}'
     -> {"error": "unknown_effect", "nearest": ["explosion", "energy_burst"]}
   specula call effectus_get '{"name": "explosion", "args": {"radius": 1}, "plane": true}'
     -> {"name": "explosion", "cost": {"particles": 96, "lights": 1, "decals": 0, "draw_calls": 0,
         "sound_s": 1.1, "flash_hz": 1.0}, "module": "cx.effect(\"explosion\", at, ...)", ...}
   ```

   Then in the module: `cx.effect("impact", hit_at, Args::new())?;` when a shot lands;
   `cx.effect("explosion", at, Args::new().num("radius", 1.0))?;` when a creep dies; and for the
   tower that catches fire, `cx.spawn(Spawn::new("group").position(p).effect("fire",
   Args::new().num("radius", 0.4)))?` with `cx.delete(id)` when it is put out.

2. A builder of a 3D survival game wants grass fire that spreads with the wind and that rain
   puts out. It writes `elements.json` with `{"tags": {"grass_patch": "grass", "tree": "wood"}}`,
   then in its module on a lightning event: `cx.effect("lightning", strike_at,
   Args::new().flag("strike", true))?;` and reads the rules' side from events:

   ```rust
   for e in cx.take_events() {
       match e.name.as_str() {
           "element.burnt" => { cx.delete(Id(e.data.get("id").as_u64()))?; }
           "element.ignited" => { cx.sound("alarm", None, 1.0)?; }
           _ => {}
       }
   }
   ```

   Before shipping it asks `specula call elementa_check '{"ignite": [[10, 0, 4]], "ticks": 1800,
   "wind": [3, 0, 0]}'` and is told, without running the game, how many things would burn, how
   far, and the worst step's work against the limits.

**How a game's rules trigger one.** A module: `Cx::effect` (a cue), `Spawn::effect` (a
component), and the Elementa ops. Lex: a live run's `present` rules already spawn world
entities with any component (LEX.md, "Presentation"), so a `vfx` with an entry and args works
the day the component takes args; one-shots from Lex come with a `cues` map in `present`
(`{"died": {"effect": "explosion", "args": {...}}}`), in a later stage. A hosted game in Rust
calls the engine's API with the same three things. A timeline's vfx track takes the component.

## How an entry is verified

**Measured, for every entry, by tests that run over the whole table** (so an entry is tested by
being there, as in `specula-formae`):

- it composes at its defaults, at every corner of its args box and at 32 seeded points inside
  it, in both variants (3D and `plane`), and each composed effect passes `Effect::parse` and a
  round trip;
- `cost(args)` equals what the composed effect says (`peak`, work, fill, lights, decals);
- `peak_particles` within `max_per_effect` and `overdraw` within `fill_cap` at every corner;
  `flash_rate` at most 3 at every corner (an error fails the test);
- a one-shot ends: evaluated just after its declared end time it has no live particle, its light
  is out, its sound has ended;
- the evaluated live count at 16 times across its life never exceeds the predicted peak;
- its flash energy (for the governor) is computed and within (0, 1] for entries that flash;
- its sound recipe parses in `specula-audio`, renders within 10 s, and meets its declared
  loudness within 1 LU with a true peak at or under −1 dBFS (rendered in the test, off any
  engine);
- the bytes of the whole table's composition at defaults are held by a golden hash, so any
  drift is a decision;
- in 2D, no composed particle moves along Z;
- the eight old built-ins compose byte for byte as `library.rs` holds them;
- the document's table is written from the crate's and a test holds them equal.

**Looked at, for every entry, before it is called done:** `vfx_preview` filmstrips at named
moments (onset, peak, tail; for loops, three moments of the steady state), in 3D on the neutral
stage and in 2D under an orthographic camera, at default args and at two corners; a contact sheet
per family under `gallery-full/`. The lane looks first and says what it saw in each frame and
what it is unsure of; a model that did not take them checks them (AGENTS: `sight-skeptic`;
rules.md 9). A frame with the iGPU's numbers (`perf_frame` `gpu_passes.particles`) for ten
instances of each family's heaviest entry.

**Elementa, by tests:** two runs with the same seed hash the same every tick; a save at T and a
load continue bit for bit; a line of wood nodes burns at the speed its numbers predict; a wet
node does not catch; stone stops it; downwind spreads faster than upwind by the ratio the model
says; fire melts ice and cold puts fire out; the limits hold on a forest of 20,000 nodes (the
burning count never passes its limit, a step's work never passes its limit, the left-out count
is reported); events reach a module at the next tick; `elementa_check`'s prediction equals a
run's outcome. Then a live check in Rust in `tools/examen/src/native.rs` (rule 16): a project
with a module that lights a field and reads the events, played by `specula run`, its saves and
its hashes.

## Where it lives

| What | Where | New or changed |
|---|---|---|
| The table, args, roles, aliases, nearest, catalogue, compose, cost | `crates/specula-effectus` | new crate (std + `specula-core`, `specula-scintilla`, `specula-elementa`) |
| Behaviour: nodes, materials, spread, cold, wet, blast, strike, wind, its save codec and hash | `crates/specula-elementa` | new crate (std + `specula-core`); in `SIMULATION` with the determinism `clippy.toml` |
| Beams, flat particles, orbit and inward motion, the plane variant, wind drift, new textures | `crates/specula-scintilla`, `crates/specula-gpu/shaders/particle.*` | changed (E1 to E8 below) |
| `vfx` component `args`, `to`; name resolution through files and the table; one-shot cues; the flash governor; the `effects` save section | `crates/specula/src/vfx.rs`, `vfx_tools.rs`, `savegame.rs` | changed |
| `effectus_list`, `effectus_get`, `effectus_play`; `elementa_info`, `elementa_check` | new `crates/specula/src/effectus_tools.rs`, `elementa_tools.rs`; each with access in `authority::access`, a family and a catalogue line (`toolscope.rs`) | new |
| Elementa in the tick, its section, events to modules, module ops | `crates/specula/src/lib.rs` (one call in `advance_clock_ticks`), `logic/handler.rs`, a new `crates/specula/src/elementa.rs` | changed / new |
| File kinds `effects/*.json`, `elements.json` | `crates/specula/src/project.rs`, `project_kinds.rs` | changed |
| SDK | `crates/specula-sdk/src/module.rs`: `Cx::effect`, `Cx::element`, `Cx::ignite`, `chill`, `wet`, `blast`, `strike`; `Spawn::effect`; an `Args` builder | **new ops on the existing request channel: no change to the ABI** (`sound` was added the same way) |
| A new project's pages | `notes/how.md` gains two lines: `effectus_list` and `elements.json` | changed |

## What it does not do

- No fluid simulation, no volumetric rendering, no distortion or refraction (none exists in the
  renderer: above).
- No full-screen flash and no camera shake by the engine.
- Elementa does not model chemistry, pressure, oxygen or real temperatures: its heat is a ratio
  and its fuel is seconds of burning, chosen so that an agent can predict it, not to match a
  thermometer.
- Elementa does not destroy, damage or change anything in the world by itself (until and unless
  question 1 says otherwise): it tells the rules.
- The engine never flashes more than three times a second for any args, and no setting turns
  that off.
- In 2D: no decal pass (a flat particle stands in, up to 10 s), effects draw over blended sprites
  whatever their layer, and a light shows only on lit sprites.
- Particles in a wind take the wind as it is now for all their life (a drift in closed form);
  when the wind changes fast, a cloud of smoke moves as one (a stateful simulation would not).

## The changes to Scintilla, sized

Each keeps particles a closed-form function of time and keeps the one instanced draw.

| | Change | Why | Size (lines, estimate) |
|---|---|---|---|
| E1 | **Beam emitters**: a path from the emitter to a target, cut into 4..64 segments displaced by seeded midpoint displacement (`jag`), with up to 8 branches, reshaped `reshape` times a second; each segment drawn as **one particle at its middle stretched along it** (the vertex shader already builds a quad along `r.c.xyz` of length `size` + its length: particle.vert lines 31 to 42), so no shader change. The flash check counts a beam's area as length × width | lightning, arcs, lasers, tracers | 700 with tests |
| E2 | **Flat particles**: a `facing` of `camera` (now), `up` (a quad in the emitter's XZ plane: a shockwave on the ground, a ripple), `plane` (the emitter's XY: 2D marks) | rings, ripples, 2D decals | 250 (one flag in the record, a branch in `particle.vert`, the SPIR-V compiled by hand and committed; a window check after) |
| E3 | **Orbit and inward motion**: an angular speed about the emitter's axis (closed form: the ballistic position turned by ω·age), and `direction: "inward"` (towards the centre: charge-ups) | whirlwind, portal, charge_up | 250 |
| E4 | **A target on the component and the cue**: `to` (a point) or `to_entity`, hashed only when present so existing worlds hash as before | beams | 150 |
| E5 | **New built-in textures**: `beam`, `bubble`, `shard`, `drop`, `streak`, `leaf`, `chunk`, `crescent`; decals `frost`, `splat`, `soot` | cold, impact, shield, debris | 400 |
| E6 | **Instances without an entity** (cues) in `Vfx`, with an end, a follow entity, their decal and sound, at most 256, saved in a section | one-shots | 600 (engine side) |
| E7 | **The plane variant**: spawn points and velocities kept in the local XY plane | 2D | 80 |
| E8 | **Wind drift**: the current wind × age × a per-emitter `wind` factor added to position (closed form; smooth when the wind changes smoothly) | smoke, rain, snow, leaves | 120 |

## The build plan

Stages are small enough to review one at a time. Each ends with its tests passing, its gate
(`cargo run -p specula-examen -- gate`), its pictures looked at and handed to a checker, and a
note in this document of what was measured.

| Stage | Adds | Tests | Pictures | Size (lines) |
|---|---|---|---|---|
| **1a** | `specula-effectus`: the table's types (entry, args with units and ranges, roles, aliases, kinds), `nearest`, the catalogue text, `compose` and `cost`; entries `fire`, `smoke`, `explosion` (with its scorch decal, its light and a generated boom), `sparks`, and the eight old built-ins as entries byte for byte | the whole-table tests above (corners, cost equals effect, flash, peak, end, sound loudness, golden hash, the old built-ins, the document's table) | none (pure crate) | 1,800 |
| **1b** | The engine takes it: the component's `args`, names resolved through `effects/*.json` then `/vfx` then the table; `effectus_list`, `effectus_get`; `vfx_preview` and `vfx_check` take `{effect, args}`; the `effects/` file kind; `notes/how.md` | tool tests (bounds, ceilings, access, family), a world with every stage-1 entry hashes the same with effects on and off | filmstrips of the four entries at onset, peak and tail | 1,200 |
| **1c** | One-shot cues: `Cx::effect`, `effectus_play`, cue instances with decal and sound, the `effects` save section, sight packets list cues, the flash governor | a module's cue plays and ends by itself; save and load show the same frame; the governor dims the fourth explosion in a second and reports it; cues never change `world_hash` | a burst of explosions, before and after the governor | 1,200 |
| **2** | E1 beams, E4 targets, the `beam` texture: `lightning`, `arc`, `laser`, `bolt` | flash at every corner (the hard case), a beam follows a moving target, a beam's cost | each at three moments; an arc held still and moving | 1,400 |
| **3** | E2 flat particles, E3 orbit and inward, E7 the plane variant, E8 wind drift; the 2D variant of every entry so far; the ground ring of `explosion` | no Z motion in 2D; orbit closed form against stepping; a window check (`window_check.py`) after the shader change | every entry so far in 2D; a shockwave from above and the side | 1,000 |
| **4** | The rest of the cosmetic table (cold, wind, weather, water, dust, magic, impact, heal, trails, ambient), E5 textures, one family file each | the whole-table tests pick them up | a contact sheet per family, 3D and 2D | 2,500 |
| **5** | `specula-elementa`: nodes, materials, heat, fuel, wet, phases, spreading with wind, limits, save codec, hash; `elements.json` kind | the Elementa tests above, pure | none (pure crate) | 2,200 |
| **6** | Elementa in the engine: the tick, the save section, the change feed, events to modules, module ops and SDK calls, `elementa_info`, `elementa_check` (on a worker: `Engine::offload`), `fire` with `spread` | save and load; events at the next tick; prediction equals the run; limits under a 20,000-node forest; engine-thread guard | none needed | 1,400 |
| **7** | Elementa's look: burning things drawn as `burning` (start time from `since`, so frames repeat), charred and frozen as decals over the thing's box, thinned nearest first | frames repeat; the look's cost within budget for 512 burning nodes | a field burning in 3D and a tile-world burning in 2D, at three times | 800 |
| **8** | Cold and wet in full: `chill`, `wet`, rain wetting its area, fire drying and melting, water freezing | fire meets ice; rain puts a fire out | a pond freezing; rain on a fire | 900 |
| **9** | `blast` (pushes, heat, events), wind zones pushing bodies and leaning fire, `push` | pushes go through the physics bridge; a body in a zone drifts at the predicted speed | a blast among crates | 1,000 |
| **10** | `strike` (lightning conducts through metal and wet; ignites at the point) | a chain of metal nodes all shocked within the bound | a strike on a fence | 600 |
| **11** | Lex: `present` cues; Elementa's events as Lex events | a Lex run plays a cue and hears `element.ignited` | none | 600 |
| **12** | Fuel fields: a tile map's tiles and grass patches as fuel by tile id or density, so a field burns cell by cell | bounds; 2D and 3D alike | a meadow burning | 1,200 |
| **13** | The measurement: a comparison job that needs effects (`evals/`), with the library and without, written before it is measured | the harness's | none | job file only |

About 16,000 lines in all, of which 4,200 are stage 1.

**What can run at the same time without touching the same files.**

- **Elementa's pure crate (stage 5) beside everything of stages 1a to 4**: it is a new crate that
  Effectus only names. It needs nothing from them; Effectus's `behaviour` field can name its
  types once stage 5 has merged (until then a behaviour is a name and args, validated later).
- **Stages 2 and 3 in one lane, after 1a**: both change `effect.rs`, `eval.rs` and `flash.rs` of
  Scintilla, so not two lanes. They may run beside 1b and 1c, which change the engine's
  `vfx.rs`, `vfx_tools.rs` and `logic/handler.rs`, not Scintilla.
- **Stage 4 split by family** across lanes once 2 and 3 have merged: each family is its own file
  (`crates/specula-effectus/src/forms/<family>.rs`, as `specula-formae/src/forms/` does), and
  only the family list in the table is shared (one line each: merge by hand).
- **Stage 6 after 1c** (both change `logic/handler.rs` and the SDK); stages 7 to 10 one at a
  time after 6, since each changes `crates/specula-elementa` and `crates/specula/src/elementa.rs`;
  stage 11 after 6, beside 7 to 10 (it changes `lex_live`).

## Questions only the owner can answer, with the answer I would choose

1. **Does a behaviour change the world by itself, or only tell the rules?** I would choose: it
   only tells the rules (events, and state the rules ask for), in M6. Why: no new write path
   (AGENTS rule 13), every outcome is the game's, and a builder sees in its own code why a crate
   vanished. An opt-in `on_burnt: "delete"` can come later as a named exemption if the
   measurement shows builders writing the same three lines every time.
2. **Do the eight built-ins become library entries of the same names?** Yes, byte for byte at
   their defaults (a test holds it): nothing old changes and every old name gains args.
3. **Blood:** ship `blood_hit` with its colour an arg and no gore at all? I would ship it: games
   need hits that read, the colour makes it any creature's, and a game rated for children sets
   the colour or leaves it out. No dismemberment, no gibs, ever, without the owner's word.
4. **A project's own effects as files** (`effects/<name>.json`) beside the world's `/vfx`
   library? Files, as sounds and recipes are: cheaper for a cold builder than a tool call (the
   comparisons' finding that requests cost), and editable by a human. `/vfx` stays.
5. **Lightning and the three-flash rule.** Real lightning flickers faster than three times a
   second; the engine never will, for any args, and offers no switch to allow it. I would keep
   it so (WCAG 2.3.1 is the engine's floor already: VFX.md).
6. **Which units does the world's wind have?** `world_env.wind.strength` runs 0..4 and today only
   sways foliage. I would define strength 1 as 4 m/s (a gentle breeze, Beaufort 3), so 4 is
   16 m/s (a near gale), and say it in FOLIAGE.md; the foliage's look does not change.
7. **2D by default when the game is 2D.** I would have a project's game file say
   `"view": "2d"` once, and every entry take its 2D variant there unless told; until such a word
   exists, `plane: true` in the call.
8. **Sounds made by the library.** I would have every entry with a sound generate its recipe from
   its args, and let a project name its own recipe instead (`"sound": "my_boom"`). Why: an effect
   without its sound is half an effect, and the recipes are data a project can copy and change.
9. **The flash governor dims, it does not drop.** When one-shots together would pass three
   flashes a second, the extra ones play without their brightening parts and are reported. I
   would choose dimming over dropping: the game's event still shows and is heard.
10. **The names**: Effectus for the library, Elementa for the behaviour, tools `effectus_*` and
    `elementa_*`. The lead can decide this one.

Questions 2, 4, 6, 7, 8, 9 and 10 the lead can decide; 1, 3 and 5 touch what the engine is
(destruction by itself, violence, safety) and are the owner's.

## What the probe showed

A headless engine built from this branch's base (release, RTX 4090 Laptop GPU; not run on the
integrated GPU), port 7820, `--settings off`. One `world_batch`: a flat cutout sprite 4 × 3 m at
z = 0 (blue), a flat blended sprite 2.5 × 3 m at z = 0 (red), the built-in `fire` at z = 0 over
the cutout, at z = +0.5 in front of it, at z = −0.5 behind it at its edge, at z = 0 over the
blended sprite, and `sparks` in the open; one `sight_frame` with `view_2d` (centre [2, 0],
7 m high, 960 × 540). Seen in the frame (the lane's own reading; not yet checked by another
model; the frame is kept outside the repository, `C:/lv/lanes/fx/probe/2d_probe.png`):

- the fire coplanar with the cutout sprite is **faint and washed out**: its soft edge (0.12 m)
  fades it against the sprite's depth, as predicted. So the 2D variant sets `soft` to 0, or
  puts the effect a little in front (z + a few centimetres);
- the fire 0.5 m in front of the cutout draws whole;
- the fire behind the cutout is **cut at the sprite's edge**: hidden over the sprite, drawn past
  it. Cutout sprites do hide effects behind them by depth;
- the fire over the blended sprite at the same z draws whole over it (no depth from blended
  sprites);
- the sparks fall and streak under the orthographic camera as in 3D.

So particles already work in a 2D game; what the 2D variant needs is E7 (no motion along Z),
`soft` 0 and a small z offset, and E2 for marks; nothing in the drawing path.
