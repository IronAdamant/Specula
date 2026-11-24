# Design: the libraries of sample assets (`exemplaria`)

Status: **designed 2026-11-24; stage S0 (the frame) built 2026-11-24** (branch `m6-exemplar`, not
yet reviewed). The design note and the staged plan for ROADMAP M6 part 5, "A library of sample
assets, for any game", written before any of its code; what S0 settled for the other stages is in
"For the plans' lanes" (section B).

The owner (2026-11-16): "Build up the assets of animals, fantasy creatures like dragons, undead, and both
tech and living assets, and tech like robots. Set up animation. Keep them as sample, along side actually
realistic human assets. All of them by code and can be shown as in both 2D and 3D. The main logic for this
one is to build the assets that you can use in any game. the primary reason is to help you and other AI
agents build stuff and reuse, not just completely re-create. Think of it like a base template". "Those
assets must be separate from [Sepulcra] but be it's own thing." "We get model library, terrain library,
assets library, water library, grass blades. etc. ALL written in Rust ... Like having unique trees, not
copy-pasted characters as humans have done." "Renderer can come later, ONCE you have data for this."

## The name

**Exemplaria**: the plural of *exemplar*, the pattern a copyist works from. That is what an entry is: a
pattern a project copies and changes, never the thing itself. It sits beside the shape library
(*formae*, shapes) and the body's numbers (*corpus*). The crate is `specula-exemplaria`; the folder of
entries is `exemplaria/` at the repository's root. (One near name exists: the demo game
`games/exemplum`, "an example". The two are told apart by their folders; *genera*, "kinds", was the
other candidate and reads as biology's genus, which a body plan is not.)

## What exists, as read for this note

Everything below rests on code and documents read on 2026-11-24 in the merged line (`m6-una` at
`59b79140`). Where a claim is about the code, the file is named.

| What | Where (read) | What it gives this design | What it lacks for it |
|---|---|---|---|
| Recipes | `crates/specula-model/src/recipe.rs`, `lib.rs` | `params` with `default`, `min`, `max`, `integer`; `let` expressions; a `seed` (a part's and an op's streams are `stream_seed(seed, name)` through `StableHasher`); `include` of another recipe by name with `params`; a `rig` of named bones; LOD ratios; budgets; strict parsing with paths. An entity's model takes its own seed (`ModelSpec::with_seed`, `crates/specula/src/models.rs`) | No way to **draw** a parameter from its range by the seed outside a scatter (`expr.rs` has no random function); a recipe names style **presets**, not roles |
| Seeded variants | `recipe/procgen.rs`, scatter `items` | An included item's `params: {name: [min, max]}` drawn uniformly by `Rng::new(vseed, 77)` for each of up to `variants` | Uniform only, independent only, inside a scatter only |
| Sculpt (tier 3) | `crates/specula-model/src/sdf.rs`, `recipe/procgen.rs` (`SDF_NODES`) | `chain` (points with radii), `column` (sections along a line, with `square`), `round_cone`, smooth unions, a mesher with a work budget | A body is written by hand, bone by bone; the sculpt's box mend is still under review (HANDOFF) |
| The human figure | `crates/specula-model/src/figure.rs`, `docs/design/FIGURE.md` | A surface from a measured body ([CORPUS.md](CORPUS.md)), six parts on their own grids, an 18-bone rig bound by flesh, a test that measures the mesh against the body | It does not look right yet (below, "People") |
| Motion generators | `crates/specula-anim/src/gen.rs` | `gait` (up to 8 legs, a phase each, one `duty` for all, two-bone IK legs with a foot that pitches like a person's, pelvis bounce and sway, arm swing, `footstep` events), `idle`, `hover`, `swing`, `leap`, `collapse`, `recoil`, `breathe`, `chain` (spring tails), `keys`, `pose`, `wobble`; `"body": "auto"` from bone names; a work budget | No flap, no travelling body wave, no three-segment leg, no duty per leg, no gait by footfall pattern (`auto` gives legs phases 0 and 0.5 in turn), no spine flexion in a gallop |
| Motion checks | `crates/specula-anim/src/checks.rs` | foot sliding, floating, ground penetration, joint limits, pops, root drift, loop seam, hover band, leap apex, thresholds from `style.toml` | Nothing asks whether a walk is a walk: no check of the order and timing of footfalls (FIGURE.md says the checks passed a lunge) |
| IK | `crates/specula-anim/src/ik.rs` | `two_bone`, `plant_feet`, `look_at` | No chain of three or more |
| Shape library | `crates/specula-formae`, `docs/design/FORMAE.md`, `crates/specula/src/formae_tools.rs` | **The pattern followed here**: a table of entries with parameters (unit, default, range), aliases, a first answer under 3 KB, rows on request, the nearest name for a wrong one, exact counts before a build; `draw` makes any solid flat in a named view with hidden faces removed | Organic shapes are "part 5's libraries" (FORMAE.md, "Not here") |
| Studio and galleries | `docs/design/ASSET_STUDIO.md`, `GALLERIES.md` | `studio_sheet` (subjects by angles by passes: colour, value, silhouette, clay, normals, curvature, stripes), `studio_lineup` (true scale beside a person and a cube), silhouette distinctness, `style.toml` budgets and palette rules | No picture with a transparent background: nothing in `crates/specula` or `crates/specula-gpu` writes one (searched); no sprite sheet is made from a model |
| Animation sight | `docs/design/ANIMATION.md` | `anim_filmstrip`: clips by frames with their checks | The same: frames on a stage, not sprites |
| Terrain, water, grass | `TERRAIN.md`, `FOLIAGE.md`, `recipe/procgen.rs` (`TERRAIN`, `SCATTER`) | A terrain generator (noise layers, warp, island, terraces, droplet and thermal erosion, splat by slope and height, edits), scatter with masks, cutout cards with wind, water as a flat surface with presets | No tree generator; no landform beyond noise and erosion (rivers, dunes, cones); water's presets are looks, not sea states |
| Research | `docs/research/library/` (README, REVIEW, and the files named in each plan below) | Most of what a plan's numbers are | What is missing is section C below |

**A probe** (scratch only, `C:/lv/lanes/tpl/probe`, not in the repository): a four-legged rig of 18
bones (each leg thigh, shin and a near-upright metapodial named `foot_*`), 5,516 triangles, walked
by today's `gait` with the measured lateral-sequence walk of a dog (C2 §1a, Maes et al. 2008: LF 0.16,
RH 0.50, RF 0.67 after LH) at a stride and duty from Alexander and Jayes's equations (C2 §3:
λ/h = 2.4 Fr^0.34 and DF = 0.51 Fr^−0.18 at Fr 0.25, h = 0.62 m: 0.93 m, 0.75 s, duty 0.65). The motion
checks with their default limits said: foot sliding 23.0 cm/s (limit 6), ground penetration 6.7 cm
(limit 3). A trot (C2's phases) slid 73.3 cm/s; with the foot's pitch held at 0, 45.2 cm/s; a stride of
0.4 m, 9.9 cm/s; penetration stayed 6.7 cm at every stride. With no foot bone (the hock as the end of a
two-bone leg) sliding was 0 and the body floated and sank 7.5 cm. `"body": "auto"` gave the same
numbers as the measured phases (its phases differ, its failures do not). **What this shows**: the gait
generator is a person's, and a digitigrade leg is not a person's leg; a four-legged walk needs its own
leg model (stage Q below). **What it does not show**: why penetration is the same at every stride; the
cause was not traced (a reading: the foot's rest orientation is held as a person's flat foot would be).

## A. The design

### 1. What an entry is

Three layers, so that "a wolf" is never one model:

- **A plan** is Rust, in `specula-exemplaria`: a skeleton's topology and bone names, the table of
  numbers it takes (each with a unit, a default and a range, as a form's parameters are), how a body's
  surface is made on that skeleton, its levels of detail, and its motions. There are few plans and they
  are code.
- **A kind** is data: a plan with its numbers set (a grey wolf, a horse, a red oak, a tracked hauler),
  each number with its source, and with how far each may vary. There are many kinds and they are text.
- **A specimen** is one seed of a kind: every varied number drawn, the result written out as an ordinary
  recipe and anims file, and built by the modeller as any recipe is. Two seeds of the oak are two oaks;
  two seeds of the villager are two people.

A kind is a file, `<name>.kind.json`. Its shape (the fields are the plan's; this is a quadruped's):

```json
{ "kind": 1, "name": "grey_wolf", "plan": "quadruped", "about": "a grey wolf, adult",
  "aliases": ["wolf"],
  "foot": "digitigrade",
  "size":    { "withers_height": { "mean": 0.629, "range": [0.579, 0.672], "unit": "m",
               "source": "Gatta et al. 2016, J Mammal Evol, p. 948 (research C1 1.2)" } },
  "numbers": { "humerus": { "of": "withers_height", "share": 0.307, "source": "..." },
               "tail":    { "of": "body_length", "share": 0.40, "own": "no measured tail length (research C1, Not found)" } },
  "vary":    { "build": 0.06, "legs": 0.03 },
  "roles":   { "coat": "#776a5a", "underside": "#b8ab98", "eye": "#c8a040", "claw": "#2a2420" },
  "motion":  { "walk": { "pattern": "lateral_walk", "froude": 0.25 }, "trot": { "pattern": "trot", "froude": 1.0 },
               "gallop": { "pattern": "rotary_gallop", "froude": 3.0 }, "idle": {}, "attack": { "preset": "bite" },
               "hit": {}, "death": { "preset": "topple" } },
  "levels":  { "near": 6000, "mid": 2000, "crowd": 800, "far": 150 } }
```

- **Every number has a `source`** (the primary source, as the corpus's rows do, with the research file
  named as where it was found) **or an `own` with the reason** the engine set it. A test refuses a kind
  with a number that has neither. A number is checked against its source as it is typed (the research
  README's rule).
- **A species overrides a plan** by naming only what differs: a plan has a default for every number, a
  kind sets some, and a kind may name another kind as its `base` (a `dire_wolf` is the `grey_wolf` with
  `withers_height` and `build` set). One level of `base` only, so what a kind is can be read from two
  files at most.
- **A project changes a copy**, never the library: the copy is the same file in the project's
  `assets/models/`, and a change is a number in it.

**Why a kind is not a recipe with `params`.** A recipe's parameters are given from outside; they are not
drawn, they carry no source, and they do not move together. A wolf's legs, trunk and skull grow with its
size by exponents (C1 §2: Kilbourne and Hoffman 2013), and its build moves many girths at once (as the
corpus's traits do for people). That is a plan's work, in Rust, once; a kind only says the numbers.

**Why a specimen is a recipe and not a mesh.** Everything after it is the engine as it is: the modeller
builds it (LODs, collider, style, budgets, form facts), the anim library generates its clips, the studio
pictures it, the checks measure it. The plan **writes text** (the recipe and the anims file) and touches
no part of the modeller but one new shape (stage S1). `exemplar_get {name, seed}` hands back that text,
so an agent can see exactly what it would build and take the recipe itself if it wants to sculpt by hand
(and then it leaves the generator: its specimens no longer vary; the answer says so).

### 2. How a seed varies a kind, and the same on every machine

- **The seed comes from where seeds already come from**: an entity's model seed (`ModelSpec::with_seed`),
  a scatter item's variant seed, or a recipe's `seed`. A kind with no seed given builds its mean. A
  scatter item names a kind as `{"kind": "box_creature", "variants": 32}` (exactly one of `shape`,
  `include` and `kind`; no `params`, since the seed draws the numbers): each variant is the specimen of
  its variant seed, so 32 variants are 32 different bodies, at most `MAX_KIND_VARIANTS` (32) an item
  where other items take 8, each built once as a template inside the build's work budget as any
  variant is. The kind is a `<name>.kind.json` in the recipe's folder (the modeller knows no kinds: the
  engine hands it `specula_model::Specimens`, `exemplar_tools::FolderKinds`).
- **Each number has its own stream**, `stream_seed(seed, "<kind>/<number>")` (the modeller's own
  `StableHasher` scheme), so adding a number to a kind never moves the draws of the others.
- **What is drawn, in order:** size first (one draw, normal, from the size row's spread, or from its range
  read as plus and minus two spreads where only a range is printed: the engine's assumption, said in the
  kind's answer); every length then follows size by the plan's exponents; then each **trait** of the plan
  (build, legs, neck, tail, head for a quadruped; the corpus's fourteen for a person) as a residual, normal,
  at the spread the source gives, or the kind's `vary` where it gives none (marked `own`). Draws are
  clipped at 2.5 spreads and at the kind's measured range, whichever is narrower: a specimen is always a
  body that was measured or lies between measured ones.
- **People are the corpus's**: a person's specimen is a `Body` with stature, build, age and traits drawn
  as CORPUS.md says a crowd is made (each trait about zero), and refused traits redrawn by a counted
  rule (at most 8 tries, then the mean), so a seed always gives a body.
- **Deterministic**: draws through `specula_core::rng::Rng` and `dmath` (`ln_f64`, `cos_f64`, square
  roots: Box–Muller in `f64`), each drawn number rounded to 0.1 mm (or 0.01 degree) before it is written,
  so the recipe's text is the same bytes everywhere. A test holds a table of kinds and seeds to recipe
  hashes, and CI's Linux job runs it, so the two systems are compared on every push.
- **The text says what was drawn**: the written recipe's `description` carries the kind, the seed and
  each drawn number, so a specimen is reproducible from its own file.

### 3. The plans, their skeletons and where their numbers come from

"Research X" is a file of `docs/research/library/`. "Not delivered" means the number is in no delivered
file and must be asked for (section C); it is never written from memory.

| Plan | For | Skeleton | Numbers it takes | Where they come from |
|---|---|---|---|---|
| **biped** | people; the undead; humanoid robots; humanoid aliens | the figure's 18 bones (`figure::BONES`) plus, before people ship: clavicle and scapula each side, spine in three, toes and fingers as chains | the corpus's body (stature, build, age, fourteen traits) | CORPUS.md (ANSUR II, de Leva 1996); A1 to A16; motion H1, H3, H10. Spine curves, the shoulder girdle's place and muscle volumes: not delivered (C) |
| biped: **undead** | skeletons; the wasted dead | the biped's | the biped's, plus how much flesh is left (a share of each girth, 0 to 1) and what is missing (parts by name) | the bones' lengths are the person's; skulls and landmarks A7; **the wasting is the engine's own** (no measurement of the dead is wanted or possible): `own`, said so |
| biped: **humanoid robot** | androids, mechs on two legs | the biped's names, rigid parts | link lengths as the person's or as a named robot's; joint ranges | D1 (H1 robot layout and torques, iCub's degrees of freedom), D3 (Atlas and others, from Wikipedia: secondary) |
| biped: **alien** | the derived ("humanoid aliens") | the biped's | the person's numbers with traits past the measured range on purpose | the corpus, marked "outside measured people" in the specimen's facts (CORPUS.md: `proportions` judges nothing) |
| **quadruped** | dogs, wolves, cats, big cats, horses, deer, cattle, pigs, bears, elephants, rabbits, rats | pelvis, spine (3), chest, neck chain (3), head, jaw, tail chain (4 to 8); each foreleg scapula, humerus, radius, metacarpal, digits; each hind leg femur, tibia, metatarsal, digits; foot posture `plantigrade`, `digitigrade`, `unguligrade` | size (withers height), bone shares, foot posture, build; gait table | C1 (horse: Möller 2017 bone lengths with spread, May's factors; wolf: Gatta 2016; felids and canids: Samuels 2013, Meachen-Samuels; bears; elephant mass from shoulder height), C1 §2 allometry; C2 footfalls, duty against speed, transition Froude numbers, stride against Froude, spine flexion. **Not delivered**: neck and tail lengths, skulls, girths and trunk sections of any animal, metapodials of most (C) |
| quadruped: **sprawling** | lizards, crocodiles, tortoises, salamanders | the quadruped's with the limbs out to the side | limb angles, duty, body bend | C6 (thin: lizard proportions and croc high-walk mostly unverified) |
| **winged quadruped** | dragons (four legs and two wings), and griffins | the quadruped's plus a wing pair from the chest: humerus, forearm, metacarpals, digits II to V, a membrane between them | the quadruped's; wing digit shares, span and area from mass | C4 (bat digits against forearm, Castillo-Figueroa 2020; pterosaur wing-finger shares; wing loading and aspect ratio against mass; the largest flyers' mass against span), C10. **Six limbs are no animal's**: where the wing's root sits is the engine's own (`own`) |
| winged: **wyvern, bat** | two legs and wings that are the forelimbs | the bat's or the pterosaur's | as above | C4 (bats are measured; pterosaurs mostly as shares) |
| **bird** | birds of every flight style; flightless | pelvis, synsacrum, a long neck chain (8 to 14), head, beak; wing humerus, ulna, carpometacarpus, primaries as one chain; legs femur, tibiotarsus, tarsometatarsus, toes; tail | wing shares by flight style, span and area, wingbeat from mass | C3 (Wang et al. 2011 wing bones by style; Alerstam 2007 span and area; Pennycuick wingbeat; stroke phases; folding, pigeon only; walking against hopping). **Not delivered**: leg bones, neck vertebrae, downstroke fraction (C) |
| **chain** | snakes, eels, fish; and as a part of other plans: tails, necks, tentacles | a chain of 8 to 32 bones with a section along it; fish add fins as their own small chains | length, girth along the length, the body wave (wavelength, amplitude envelope, frequency), fin places | C5 (Di Santo 2021 across 44 species; the eel; snakes' four gaits, Jayne 2020, Hu 2009). **Not delivered**: girth along a snake, fin places as shares of length, per-species wave numbers (Di Santo's data set, CC0, not downloaded) |
| **arthropod** | insects (six legs), spiders (eight), and crabs as a variant | body in two or three parts, legs coxa, femur, tibia, tarsus (four segments) | segment shares, gait (tripod, tetrapod, wave; the spider's) | C7 (stick insect and ant models; tripod and wave gaits; a tarantula table with garbled cells); H6 (Walknet rules). **Thin**: no measured segment table with spread (C) |
| **machine** | vehicles on wheels or tracks, walkers, arms, flyers, things that hover | rigid parts on joints: revolute, prismatic (pistons), wheel axles, track loops; no skin | wheel base and track, wheel radius, ride height, link lengths, joint ranges, cylinder lengths against angle | D1 (joint kinds, arms, legged robots, the excavator's geometry and its cylinder formula), D2 (panel gaps, fasteners: what reads as made), D3, J22 (vehicles' masses, dimensions, tyres, suspension), J5, J23 (aircraft), J28 (engine curves, for sound and speed). **Not delivered**: Ackermann geometry, track pitch and sprockets, robots' joint ranges (C) |
| **tree** | broadleaf and conifer trees, palms, shrubs | a branching structure, not a rig (wind sways it: FOLIAGE.md) | height and crown against trunk diameter, branching per order (angles, length and radius ratios), phyllotaxis, crown shape, leaves, bark, root flare | E1 (the pipe model; ABOP's tables; Weber and Penn's parameter set for a black oak; Pretzsch and Bechtold allometry by species; crown ratio by group), E2 (leaves, bark thickness and furrows, root flare), E4 (where they grow). **Not delivered**: branch angles by order for any common species (E1, Not found) |
| **plant** | grasses, flowers, ferns, reeds, moss | blades and stems as cards or thin strips | blade length, width, curve; clump density; flowering height | E3 (about thirty species' blades, densities); FOLIAGE.md's cards and wind |
| **landform** | terrain pieces: hills, ridges, valleys, rivers' beds, dunes, cones, cliffs, coasts | a terrain recipe (the generator that exists) plus shaped features | slopes, profiles, ratios | F1 (angles of repose; river concavity; Hack and Horton; meanders; V and U valleys; volcano slopes; dunes; cliffs; coasts), F2 (erosion constants), F3 (thin: no colour or roughness by rock) |
| **water** | seas, lakes, rivers, falls | a water surface and its shore; a river's path | sea state (wave height and period from wind), river flow from slope and depth | G1 (Pierson–Moskowitz, JONSWAP, Beaufort, breaking, whitecaps), G2 (Manning, hydraulic geometry, falls: half of what was asked), G3 (nearly empty) |

### 4. The surface on a skeleton

The figure builds a person from columns of sections between joint centres (`figure.rs`: `column`,
`squared`, `chain`, eggs for buttocks and breasts). Every animal plan needs the same thing with its own
bones. **One shape is added to the modeller, `body`** (stage S1): a list of bones, each with sections
along it (as `column` takes: place along the bone, half breadth, half depth, squareness), a smoothing
radius, and the parts to mesh finely (heads, feet, hands) with their own resolution. It is written once
and is what the figure is made of, generalised: the figure's own shape stays as it is, so nothing built
changes; a person's plan may move to `body` when the figure's mends (below) are made. A plan's job is
then to turn a kind's numbers into bones and sections.

### 5. Motion belongs to the plan

A clip is written in the plan's terms, not in angles for one body: a gait is a **footfall pattern** (a
phase and a duty for each foot) at a **Froude number** (speed squared over gravity times hip height), so
its stride and its period come from the specimen's own leg length by Alexander's dynamic similarity
(C2 §3). One spec, written once per plan, fits every kind and every specimen of it. The plan writes the
anims file next to the recipe, so a specimen arrives walking.

What each plan needs, against what `gen.rs` has:

| Plan | Walk, run | Idle | Attack | Hit | Death | Missing in `specula-anim` |
|---|---|---|---|---|---|---|
| biped | `gait` (exists) | `idle`, `breathe` | `swing` | `recoil` | `collapse`, ragdoll | a walk from measured curves (H1 §2, H10: hip, knee, ankle, pelvis and trunk through the stride) instead of guessed numbers; arm swing from H10-2 |
| quadruped | `gait` **with a footfall table**: lateral walk, trot, pace, canter, transverse and rotary gallops, bound (C2 §1) | `idle` | `swing` on the head and jaw (a bite), `leap` (a pounce) | `recoil` | `collapse` | **a three-segment leg** (the probe above), **a duty per leg** (fore and hind differ: C2 §3), **gait presets by name** from C2's tables, **spine flexion in a gallop** (C2 §4), phases by pattern for `body: auto` |
| winged | the quadruped's on the ground | `idle` with the wings folded | as quadruped | `recoil` | `collapse` | **`flap`**: a wingbeat from mass (Pennycuick, C3 §3), downstroke and upstroke phases (C3 §4), the fold (C3 §5); a launch (C4 §4) |
| bird | `gait` and a hop (C3 §7) | `idle` (head bobs) | a peck (`swing`) | `recoil` | `collapse` | `flap`, `glide`, a hop |
| chain | **`undulate`**: a wave travelling down the chain, its amplitude growing toward the tail by the style (anguilliform to thunniform, C5 §1), and on the ground the four snake gaits (C5 §3) | a slow `undulate` | a strike (`swing` on the front of the chain) | `recoil` | `collapse` | `undulate` with ground contact (a snake moves where its body pushes) |
| arthropod | `gait` with six or eight legs (`MAX_LIMBS` is 8: enough) by tripod, tetrapod or wave pattern (C7 §2; the spider's, §3) | `idle` | `swing` | `recoil` | `collapse` (legs curl) | four-segment legs (the same leg model as the quadruped's, one more segment), gait presets |
| machine | **`mechanism`**: wheels turn by the distance travelled, tracks scroll, suspension follows the ground, pistons follow their joints (D1's cylinder formula), a walker's legs by `gait` | `wobble` (idling engine) | `swing` (an arm) | `recoil` | `collapse` (`scatter`) | `mechanism` (rigid, no skin: the cheapest) |
| tree, plant | wind (exists, in the shader) | | | | a fall (a tree felled: `keys`) | nothing for the library's first stages |

**New checks** (in `checks.rs`, beside the nine): `footfalls`, which measures each foot's contact
intervals in the produced clip and holds their order, phase and duty to the pattern the clip was made
from (so a lunge is caught where foot sliding passes it); `stride`, the stride against leg length within
the band the plan's Froude number gives. Both read the clip, so they hold every kind.

### 6. 2D from the same source

What exists: `formae::draw` (a solid drawn flat in a named view, hidden faces removed, on the CPU),
`studio_sheet` (a model by angles and passes, on the GPU), `anim_filmstrip` (clips by frames). None writes
a sprite: no capture has a transparent background, and nothing lays frames out as an atlas with their
data.

**What is added (stage D): `sprite_bake`**, one engine tool: a model (any recipe, so any specimen) at the
angles a game asks for (`side`, `three_quarter`, `top`, `4` or `8` directions, or a list of azimuths and an
elevation), for the clips it names, at a frame rate and a pixel height, under the project's light or a
flat one, written as one PNG atlas **with alpha** and a JSON of frames (each frame's box, pivot, the
ground line, which feet are down, the clip's events). Options a 2D game asks for: an orthographic camera,
a pixel-art reduction (nearest-neighbour to the target height, the palette of the project's style, an
outline). The silhouette pass gives the alpha (the studio already renders it). It runs on the render
workers like the studio's sheets, off the engine thread, bounded in frames and pixels.

A vector 2D game takes the same specimen through `formae::draw` frame by frame (polygons with depth and
shade): stage D2, after the atlas, if a game asks for it. A 2D skeleton (the bones projected, the parts as
sprites) is cheaper at run time than baked frames; it is noted and not designed here.

### 7. The look is the project's

A kind names **roles** (`coat`, `underside`, `eye`, `claw`, `bark`, `leaf`, `hull`, `panel`, `glass`),
never a style's presets, with a hint colour each. A specimen's recipe names the roles; the build resolves
a role to the project style's preset of the same name, else through a `roles` table in the project's
`style.json` (`"roles": {"coat": "fur_brown"}`), else it is refused naming the role and the style's
nearest presets by colour to the hint. So the same wolf is built in each game's own materials and
palette, and the palette checks of `style.toml` hold it as they hold anything. No fixed art pack ships.
(A change to `crates/specula-model/src/style.rs`: a role table read beside the presets; about 200 lines.)
A kind uses at most four roles by default: each is a draw call (ASSET_STUDIO.md: Sepulcra allows six
materials a model).

### 8. Its own place

- **`exemplaria/` at the repository's root**, by plan: `exemplaria/quadruped/grey_wolf.kind.json`. Not
  under `games/`, and nothing of Sepulcra's: Sepulcra is a user, its mobs of maps 2 and 3 made from it in
  its own folder and style. **The folder is the source and the library is built into the engine**
  (`include_str!` of each file, `specula-exemplaria/src/library.rs`): what the engine answers and what the
  table of hashes holds are the same bytes, and nothing beside the executable is read, so nothing there
  can be swapped (decided by the lead on 2026-11-24; reading the folder from beside the executable, as
  this note first said, is not built). A test holds the folder and the table together: every file under
  `exemplaria/`, at any depth, is a kind with its line in the table and the same bytes, and the folder
  holds nothing else.
- **A project takes a copy**: `exemplar_take {name, as?}` writes the kind into
  **`assets/models/NAME.kind.json`**, where a new project keeps its models (`specula new`,
  `notes/models.md`), through `project_write` (rule 13: the project tools' rule; one tool call, one
  commit). So `model_build_all` (whose folder is `models` unless told) builds it with the project's own
  models, and it is listed with them. Its base, if any, is **written into the copy** (one file says
  everything; one call makes one commit), so a project's copy never has a base, and deleting the
  library kind's base in a project is moot. Never a reference to the library's file: a project's assets do
  not change when the engine does.
- **What every entry carries**: its licence (the engine's, MIT OR Apache-2.0, in a field); what it is in
  one line; its numbers with units, ranges and sources. **A project's copy also records its taking, in
  the copy itself** (no second file, which would be a second commit): `"from":
  "exemplaria/<plan>/<name>@<hash of the kind file>"`, so a later answer can say "the library's wolf has
  changed since you took it", and `"taken": {"library": <the library's version, 16 hex digits>, "base":
  <the base written into it>, "checked": <what was checked at the take>, "own": <how many of its numbers
  are the engine's own>}`. At the take the kind is written at its mean and at seeds 1, 2 and 3, each
  recipe parsed and its clips generated on its rig, and `checked` says so. The checks of form and motion
  at the library's seeds (B, stages Q on) are added to `taken` by the stages that make them; a
  `<name>.record.json` beside the kind file is not made (a test may not write into the checkout, and the
  folder holds kinds only). A contact sheet per entry (thumbnail tracked under
  `docs/gallery/exemplaria/`, the full size in `gallery-full/`, as the gallery's rule has it) comes with
  the first real kinds.

### 9. The catalogue: one path, not three

An agent meets three lists: the shape library (`formae_list`), this library, and what its project holds
(the item "One call that lists what the project holds", built in another lane). Three tools that each
know nothing of the others would cost an agent a request each before it knows where to look. So:

- **`exemplar_list`**, as `formae_list` is: with nothing asked, every kind by plan, a line a plan, and how
  to ask for more, **in under 3 KB**; rows on request (by plan, word or role) with the snippet to paste
  (the spawn item with a seed, the scatter item with `variants`), each number's default, range and unit, at
  most 8 KB a page. **`exemplar_get {name, seed?, full?}`**: one kind, short by default (under 3 KB); with a
  seed, the specimen's drawn numbers and the recipe's counts before any build; `full` adds the written
  recipe and its record. **`exemplar_take`** as above. Read-only but `take`; on the caller's thread; every
  argument bounded and every answer under a ceiling (rule 15).
- **One index of names** (a module of the engine crate, `catalog_names`), which each of the three fills
  with (name, kind of thing, one line, which list): a name not found in any of the three tools answers with
  the nearest names **across all three**, each with the call that shows it. A word that names a shape and a
  kind (`egg`) is answered with both.
- **The first lines of each list name the other two**, with their counts ("also: 159 shapes,
  `formae_list`; 23 recipes in this project, `project_catalog`"), and the orientation an agent is given
  unasked (the ROADMAP item after the catalogue) names all three. A kind a project has taken shows in the
  project's list as taken, changed or not.
- `formae_list {figures: true}` already shows the human figure; it gains a line pointing at
  `exemplar_list`, and the figure appears in this library as the biped plan's people.

### 10. What "realistic" means here, and what it does not

**Means**: the proportions are measured (each number sourced), the silhouette reads as the animal at a
thumbnail (the studio's silhouette pass, seen by a model that did not make it), and the gait has the
footfall order, duty and stride of the animal at that speed (the new checks). **Does not mean**: fur,
feathers, scales or hair (a renderer's work, the Fidelity track: until then animals are smooth or
stylised, and the look is the project's style); skin shading; faces (their own lane); muscles that slide
under skin.

**People, and what the figure needs before people can be an entry.** Read in `figure.rs` and FIGURE.md,
in terms of the model, not of the picture:

1. **The trunk is one straight column on a flat frame.** `trunk()` builds a `squared` column from
   `[0, hips, 0]` to `[0, top, 0]` with four sections (hips, waist, chest, top) and one squareness (3); the
   corpus's frame is flat (CORPUS.md, "The frame is flat"). So no curve of the lower back or the upper back
   can exist, the belly and back are ruled lines, and the buttocks are eggs set on the column (a corner
   where they start). Needed: the spine's curves in the frame (numbers not delivered: C), a squareness for
   each section (A13 gives waist 2.7 to 2.8, hips 3.3 to 3.7; FIGURE.md already names this as next), and
   sections at more than four heights.
2. **Limbs are straight columns between joint centres**, their sections set by the survey's girths at the
   measured places and between them by eye (each such number commented as the engine's own). The muscle
   masses that make a thigh, calf, shoulder and upper arm read are not in the model (no deltoid, no biceps,
   no calf belly behind the shin's line but a set-back), and the research for them is thin (A14: two
   muscles; A16: none). Needed: muscle volumes as sections or eggs along each bone, from measured numbers.
3. **Six parts meshed on six grids and cut by boxes** (`room`, `without`). Each cut is a seam (FIGURE.md:
   cuff lines at wrists and ankles, a rim under the skull) that no smoothing can remove, because the two
   sides are different surfaces. Needed: one surface (a mesher that refines where a part needs it, or cuts
   placed where both grids share their samples). This is a change to the sculpt mesher, not to the figure.
4. **The skeleton has no shoulder girdle and no spine joints**: 18 bones, `chest` to `upperarm` directly,
   so a shoulder cannot rise or come forward, and binding at the shoulder creases. Needed: clavicle and
   scapula (A11 has how the shoulder blade turns at 30 to 120 degrees; A12 the joints' places), a spine of
   at least three bones, toes and fingers as chains.
5. **The walk is guessed** (FIGURE.md: "the walk is a lunge from guessed numbers"), and the checks passed
   it. Needed: a walk from H1 and H10's measured curves, and the `footfalls` and `stride` checks.
6. **A figure is 45,000 to 53,000 triangles**; Sepulcra's `style.toml` gives an enemy 300 to 800. The
   `whole` part (one coarse grid) exists for distance and has not been looked at (FIGURE.md). Needed: a
   ladder of levels made from the same body and each measured against it, within a crowd budget.

Until 1 to 6 are done, people are not an entry; the undead as **skeletons** (rigid bones, no skin: the
machine's problem, with the corpus's lengths and A7's landmarks) can be, and early.

### 11. How an entry is verified

- **Measured against its numbers**: the built rig's bone lengths against the kind's (within 1%), its
  heights (withers, hip) against the specimen's drawn numbers, its mass from its volume and a density
  (research J3) against the kind's mass where one is sourced; for people the figure's own measuring test.
  For trees, height and crown against the allometry the kind took.
- **Form**: the form checks of FORM.md (loose pieces, razor edges, faceting, seams, smeared detail),
  every entry at its library seeds, as facts in its record.
- **Looked at**: clay and curvature, colour and silhouette, front, side, three-quarter and top
  (`studio_sheet`), and a line-up at true scale beside the person and the cube (`studio_lineup`); its 2D
  sheet; **by a model that did not take the pictures** (`sight-skeptic`). Siblings (two kinds of one plan)
  held apart by the studio's silhouette distinctness.
- **Motion**: the nine checks and the two new ones, on every clip of every entry at three seeds (the
  smallest, the mean and the largest specimen), with `style.toml`'s default `[motion]` limits.
- **Budgets on the reference device** (Intel iGPU, F3): each level within its triangle ceiling, bones
  within 64 for a crowd level (ANIMATION.md), roles within four (draw calls); built and drawn with
  `perf_frame` in a scene of twenty specimens; the build's work within the modeller's budget.
- **Varied**: at 32 seeds every specimen builds, passes the checks, and no two hash alike; the spread of
  each drawn number over 1,000 seeds is within a tenth of what the kind says.
- **The same on every machine**: the table of recipe hashes, on Windows and on CI's Linux.

### 12. Faces and hair: what that lane needs from this design

Only this: the biped plan names a `head` bone with `jaw` and `eye_l`, `eye_r` beneath it; the head's
part is one region with named anchors (vertex, ear openings, eye centres, the hairline's front from B6)
in the head bone's frame; a person's specimen hands its drawn traits to the face (so a face varies with
its body); roles `skin`, `iris`, `lip`, `hair`, `brow`. Expressions are face bones, because the renderer
has no morph targets (`crates/specula-gltf/src/lib.rs`: its import says they are not supported); B3's action units map to them. Everything else is
that lane's.

## B. The staged build plan

### Order, and why

1. **The shared code first (S0, S1)**: the kind format, seeded draws, expansion to recipe and anims, roles,
   the catalogue tools and the name index; then the `body` shape. Every other stage uses them, and two
   lanes writing them separately would make two formats.
2. **Machines and trees next, at the same time.** Machines are the cheapest to make right (rigid parts,
   no skin to bend, numbers in D1, D3 and J22, motion that is a turning wheel) and they prove the whole
   path: kind, specimen, rig, clips, checks, sheet, catalogue. Trees need no rig and answer "unique trees"
   at once; E1 holds a full parameter set and allometry. Skeleton undead come with machines (the same
   rigid problem, the corpus's lengths).
3. **Quadrupeds**: the best-researched animals (C1, C2) and the leg and gait machinery that dragons,
   insects and birds' legs reuse.
4. **The 2D baker** whenever a lane is free after S0: it takes any recipe, so it needs no plan.
5. **Chains, birds, winged quadrupeds, arthropods** on the quadruped's legs and the new generators.
6. **Plants, landforms, water** beside them: they touch other files.
7. **People last**, after the figure's six mends (part 4's lane), then the people's kinds and the walk
   from measured curves.

### Stages

Lines are Rust including tests; data (kind files) is counted apart. "Pictures" are captures in the
engine, looked at by the builder and then by a model that did not take them.

| Stage | Adds | Tests | Pictures | Lines |
|---|---|---|---|---|
| **S0 the frame** | crate `specula-exemplaria`: the `Plan` trait, the kind format (parse with paths, sources required, `base`), seeded draws (streams, clipping, rounding), expansion to recipe and anims text; roles in `style.rs`; the engine reads `*.kind.json` as recipes (`models.rs`); tools `exemplar_list`, `exemplar_get`, `exemplar_take` (access, family, catalogue line); the name index across shapes, kinds and the project; `gen.rs` split so each generator kind is a file of its own (so lanes adding generators do not meet in one file); one test plan (`box_creature`) | kind parsing refusals; draws' spread over 1,000 seeds; hash table of seeds; a source-less number refused; nearest names across the three lists; answer ceilings; `take` through the write policy (its row in `every_write_path_holds_the_policy`) | none (a test plan only) | 5,000 |
| **S1 `body`** | the `body` shape in `specula-model` (bones with sections, fine parts, smoothing) | the figure rebuilt as a `body` measures as the figure does (its test's table within the same tolerances); a column's bound held as the figure's are | a test body in clay and curvature | 2,000 |
| **M machines** | plans `wheeled`, `tracked`, `walker`, `arm`, `hover`; the `mechanism` generator; kinds: a cart, a car, a tracked hauler, an excavator, a two-legged and a four-legged robot, an arm, a drone; **skeleton undead** (biped bones as rigid parts) | wheels turn by distance (no sliding: the foot-sliding check on wheel contacts), pistons hold their length rule, joint ranges from D1 | each kind, four views, colour and clay; clips as filmstrips | 5,000 |
| **T trees** | `tree` generator in `specula-model` (`generate: tree`): pipe-model radii, branching by order, phyllotaxis, tropism, crown envelope, leaves as cutout cards (FOLIAGE.md), bark by a texture graph; kinds for the species E1 has (oaks, maples, birch, beech, pines, spruce, firs, willow, a palm, desert and savanna shapes as E1 allows) | height and crown against each kind's allometry; radii by the pipe rule; 32 seeds unlike; budgets by level | each species at three seeds, two views, in a line-up at true scale | 5,000 |
| **D the 2D baker** | `sprite_bake`: angles, clips, frames, alpha, atlas, frame data, orthographic, pixel-art option | alpha from the silhouette pass matches the ID buffer; atlas bounds; the same bytes twice; bounded work off the engine thread | sheets of a machine and a tree, at eight directions | 3,000 |
| **Q quadrupeds** | plan `quadruped` (three foot postures, sprawling variant); three-segment legs in the gait (and four for S-A); duty per leg; gait presets from C2; spine flexion; `footfalls` and `stride` checks; kinds: wolf, dog, cat, lion, tiger, leopard, horse, deer, cattle, pig, bear, elephant, rabbit, rat (as their numbers allow; each missing number `own` or waiting) | the probe above as a test (sliding under 6 cm/s, no penetration); footfall order and duty within the table; Froude scaling across the smallest and largest specimen | each kind at three seeds, four views; walk, trot and gallop filmstrips | 7,000 |
| **C chains** | plan `chain`; the `undulate` generator (swimming styles; four snake gaits with ground contact); tails and necks for other plans; kinds: snakes, an eel, fish of three styles | wave speed and amplitude envelope against the style; a snake's contact points do not slide where its gait says they hold | swimming and slithering filmstrips; four views | 3,000 |
| **B birds** | plan `bird`; `flap`, `glide`, a hop; kinds by flight style | wingbeat against Pennycuick's frequency for the mass; stroke phases; fold | flight filmstrips; wings spread and folded | 4,000 |
| **W winged** | plans `winged_quadruped`, `wyvern`, `bat`; launch | span and area against the mass's wing loading (C4 §3, §5) | a dragon at three seeds; flight and walk | 3,000 |
| **A arthropods** | plan `arthropod` (six, eight legs); tripod, tetrapod, wave and spider gaits | footfalls by pattern; leg shares | insects and spiders, four views and gaits | 3,000 |
| **P plants** | blades, clumps, flowers, ferns, reeds, moss as kinds (E3), on the cards and wind that exist | blade sizes against E3; budgets per tuft (FOLIAGE.md's six triangles) | patches in a line-up | 2,500 |
| **L landforms** | landform kinds over the terrain generator, and what it lacks: a river carved along a path with Hack's and the meander's numbers, dunes at their slip angle, cones at their slopes, cliffs with strata (F1) | slopes against angles of repose; a river's long profile concave as F1's | each landform from above and at eye height | 4,000 |
| **H water** | water kinds: sea states by Beaufort number (G1), a river's surface on its path with flow from Manning (G2), a fall; on the water component as it is | wave height and period against the sea state | each at two times | 3,000 |
| **F people** (after part 4's figure mends) | plan `biped`: people from the corpus as kinds (a villager, a soldier, an elder within the survey's ages), the undead fleshed, humanoid robots, aliens; a walk from H1 and H10; arm swing | the figure's measuring test on every specimen; footfalls and stride; the corpus's balance on idle | crowds of twelve seeds; walks | 4,000 (plus the figure's own mends, part 4: about 6,000) |
| **H2 human motion** | run (H7, H9, H10), turning and stairs when researched | joint curves within H10's spread | filmstrips | 2,500 |

### What runs at once, and on which files

- **S0 must exist first** (one lane). **S1 after S0** or beside it (it is in `specula-model` only, and
  S0 does not touch the modeller but `style.rs`).
- **After S0 and S1, four lanes at once without a shared file**: M (its plan files, `gen/mechanism.rs`),
  T (`specula-model/src/tree.rs`, the generator's registration line), D (`crates/specula/src/sprite_bake.rs`
  and the studio's render path), and the kinds' data. The only shared lines are registrations (a plan in
  the plan list, a generator in the generator list, a tool in the tool list): one line each, merged by the
  lead.
- **Q owns `gait`** (three-segment legs, duty per leg). A, W and B's walking wait for Q; C and B's flight
  (`undulate`, `flap`) are generators of their own files and can run beside Q.
- **P, L and H** touch the terrain and foliage code and nothing of the animals': beside anything.
- **F waits** for part 4's figure lane.

### For the plans' lanes (what S0 built, and how a stage adds to it)

S0 is built in `crates/specula-exemplaria`, the folder `exemplaria/`, a hook in the modeller
(`specula_model::Specimens`, for a scatter item by kind) and these files of the engine:
`exemplar_tools.rs` (the three tools, a kind read as a recipe, `FolderKinds`),
`catalog_names.rs` (the name index), and single lines in `models.rs`, `project.rs`,
`project_kinds.rs`, `procgen_tools.rs`, `formae_tools.rs`, `tools.rs`, `authority.rs`,
`toolscope.rs`, `toolkits.rs` and `write_policy/tests.rs`. What a later stage needs to know,
beyond the code's own notes:

**Adding a plan** (a stage's plan, `quadruped`, `wheeled`...):

1. A file `crates/specula-exemplaria/src/plans/<plan>.rs` with a unit struct and
   `pub static PLAN: MyPlan`, implementing `plan::Plan`: `name`, `about`, and its tables as
   `'static` arrays (`size`, `numbers`, `traits`, `roles`, `options`, `motions`), then `write`,
   which turns a `Specimen` into bones, parts and clips through `write::Writing` (`bone`,
   `part`, `block`, `clip`, `gait`, `fact`; `material(role)` resolves a role).
   `plans/box_creature.rs` is the pattern, at about 150 lines.
2. **Two single lines** in `crates/specula-exemplaria/src/plans/mod.rs`: `pub mod <plan>;` at
   the end of the modules, and `&<plan>::PLAN,` at the end of `PLANS`.
3. Nothing else registers it: `resolve` checks a kind against the plan's tables, and the test
   `the_plans_tables_are_sound` holds every plan's tables (names valid and unique, a number's
   traits known, at most four roles, every motion's generator in `specula_anim::gen::KINDS`,
   every pattern in `gaits::PATTERNS`).

A plan's defaults are the engine's own numbers: a kind that leaves a number out takes the plan's
share, and `exemplar_get` counts those as "at the plan's default share". A plan writes text and
touches no part of the modeller: a new shape it needs (S1's `body`) is the modeller's lane.

**Adding a kind**:

1. Its file at `exemplaria/<plan>/<name>.kind.json` (the format: `kind.rs`'s notes; every
   number with `source` or `own`, never both). The folder holds kind files only.
2. **One line** at the end of `ENTRIES` in `crates/specula-exemplaria/src/library.rs`: its
   path and `include_str!` of it. The test
   `every_file_of_the_folder_is_built_in_and_nothing_built_in_is_missing` fails until both are
   there, and fails on any other file under the folder.
3. Rows in the table of `tests/library.rs` for the seeds worth holding (at least the mean and
   two seeds): the test prints the rows as they are, to paste once their specimens have been
   looked at. A change that moves a specimen changes those rows in the same commit, saying why.
   (A kind file's own hash is in its specimens' text, through `from`: any edit to the file moves
   every row, on purpose.)
4. A base is another kind of the same plan, by name, one level: in the library it is looked up
   in `ENTRIES`; in a project, beside the kind's file (`<base>.kind.json`, in the same folder).
   A project's copy (`exemplar_take`) has its base written into it, so it is one file.

**Adding a generator** (`mechanism`, `undulate`, `flap`) after the split: each kind is a file of
its own under `crates/specula-anim/src/gen/`. A new one is `gen/<kind>.rs` with
`use super::*;` and `pub(super) fn <kind>(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32],
duration: f32) -> Result<Out, AnimError>` (the shared `Body`, `Frame`, `Out`, `solve_all` and the
pose helpers stay in `gen/mod.rs`, visible to its files), then, in `gen/mod.rs`: `mod <kind>;`
after the others, its name at the end of the `use {...}` line, its arm in the dispatch
(`"<kind>" => <kind>(&mut o, skel, &body, &times, duration)?,`) and its name at the end of
`KINDS`. A plan's `MotionDef.generator` names it; the plans' test reads `KINDS` itself.

**What S0 decided that the note did not say:**

- **The library is built into the engine** (A.8): the folder is the source, held to the
  built-in table by a test; nothing is read from beside the executable.
- **A kind is read as a recipe** where its path ends in `.kind.json`: the model library checks
  it at a spawn by writing its mean in the recipe's style (every role resolved), and its build
  worker writes the recipe and clips at the spec's seed; a project's `project_write` checks it
  the same way. A kind's clips are written by the plan: no `anims/<name>.json` is read for it.
  The anim set an entity's graph writes settle against (`Library::write_set`) is the specimen's
  at the entity's seed, rig and clips, not its mean's.
- **Roles** resolve through `specula_model::Style::role`: a preset of the role's name, else the
  style's `"roles"` table, else refused with the three presets nearest in colour to the kind's
  hint. A kind's first part written in an unresolved role is the one named.
- **The copy goes to `assets/models/`** (A.8). Found on the way, and mended: the project store
  mirrored a project's recipes only from folders named `recipes`, while `specula new` puts a
  project's models in `assets/models/`, its guide tells an agent to write them there and
  `model_build_all` builds that folder; so `project_write` (and every tool that writes through
  it) refused `assets/models/x.json`, and a project's models were not in `project_list`. The
  store now mirrors `.json` files whose folder is `models` as recipes too (`project::kind_for`);
  `.glb` and `.gltf` there are still models, and `models/exports/` holds no recipes (as with
  `recipes/`, the file's own folder must be named so: `models/props/x.json` is not mirrored).
- **A take into a new project is refused until its style names the kind's roles** (a new
  project's `style.json` has four presets and no `roles`; A.7 says a role is never guessed).
  Seen in the cold job: the refusal named one role at a time. The refusal now carries `fix`: the
  one `project_patch` of the style (its path and version) that names every unresolved role to
  the preset nearest its hint colour; the agent makes it (or chooses other presets) and takes
  again. Two calls, not one a role.
- **A scatter item by kind** (A.2): `{"kind": NAME, "variants": N}`, at most 32 variants, each
  its own specimen; the kind is read from the recipe's folder (`<NAME>.kind.json`, its base
  beside it) and counts as an include for hot reload. The variant's seed is folded into a
  recipe's range (below 2^53: found by its test, a full 64-bit seed was refused by the written
  recipe); a build writes each variant once a pass over its parts (twice with its coarse pass),
  the same text each time.
- **The name index** is `crates/specula/src/catalog_names.rs`: a `Source` per list (`Shapes`,
  `Kinds`); the project's catalogue (lane `m6-formae`) plugs in as a third `Source` in
  `sources()` when it merges. `formae_get`'s refusal names the nearest kinds, and `exemplar_*`
  refusals the nearest names of both lists with the call that shows each. `exemplar_list`'s
  first answer names the shape library's call and count on its second line. `formae_list`'s
  first answer does **not** name the kinds: one more line puts it over its 3 KB and cuts the
  shape names it shows today (its test `index_holds_any_table` says so); the orientation an
  agent is given unasked (lane `m6-formae`) names every catalogue.
- **The tools are in the kit `make_asset`** (`toolkits.rs`): `exemplar_list`, `exemplar_get`
  and `exemplar_take` in its `more`, and its steps name `exemplar_list` beside `formae_list`.
- **A specimen's seed** is the model library's, 0 to 1,000,000; `exemplar_get` takes the same.
- **Not in S0**: the `footfalls` and `stride` checks (stage Q), and the checks of form and motion
  a copy's `taken` will carry (the stages that make them).

### The size

The roadmap says 40,000 to 60,000 lines. The stages above come to about **56,000**, with the figure's
mends (about 6,000, part 4's) and faces and hair (their own lane: about 8,000 to 12,000, not designed
here) outside it. **I agree with the range for this design, near its top**, read against the code: the
shape library's crate is about 12,800 lines of source (15,200 with its tests) for a table of 159 forms with one kind of build
each; the figure alone is 1,016 lines on top of the 5,972-line sculpt; one generator kind in `gen.rs` is
150 to 400 lines and the probe shows the gait needs real work for every plan that is not a person. What
would push it over: a second leg model if insects' do not fit the quadruped's, and water if its
renderer must change to show a sea state (it is a flat surface with stylised waves today). Kind files
are data and are not counted; there will be about 80 to 120 of them.

## C. Research still missing

To hand to a researcher as the earlier rounds were: **every number with its source (paper, table or page),
the population or species, mean and spread, and the source's licence**; datasets named, never
downloaded into the repository; nothing from a book in copyright.

**Quadrupeds** (C1, C2 hold limbs and gaits; nothing of the body's bulk)
1. Neck length and tail length against trunk length or withers height, with vertebra counts, for dog,
   wolf, cat, lion, tiger, leopard, horse, deer, cattle, pig, bear, elephant, rabbit, rat.
2. **Girths and depths of the body**: chest girth, chest depth, body breadth at the shoulder and the hip,
   against withers height or mass, for the same species (veterinary and livestock body measurements; none
   delivered for any animal).
3. Skull length and breadth, muzzle length; ear length; horn and antler sizes (deer, cattle).
4. Metacarpal and metatarsal lengths and the digits' lengths for dog, wolf, cat, pig, deer, elephant.
5. Standing joint angles (shoulder, elbow, carpus, hip, stifle, hock) in quiet stance, per species.
6. Joint ranges of the limbs and the neck for dog, cat and horse.
7. Horse footfall phases with LH = 0 and duty per gait (Hildebrand's papers); cat footfalls and duty;
   bound and half-bound phases.
8. Body mass with spread per species (PanTHERIA gives one number).
9. The CC0 data sets the delivery could not reach, **which need the owner to sign in to Dryad**: Serio et
   al. 2024 (limb bones and skull of 246 species), and the two below.

**Sprawling** (C6): lizard limb proportions against snout–vent length with spread; crocodile high-walk
angles and duty (primary); turtle limb and shell lengths.

**Birds** (C3): leg bones (femur, tibiotarsus, tarsometatarsus) and toes by species; neck vertebra count
and neck length; body girth; downstroke fraction across species; wing fold angles beyond the pigeon;
Skelevision (Weeks et al. 2025, CC0, Dryad).

**Bats and dragons** (C4, C10): bat metacarpal and phalanx lengths in millimetres; where the wing membrane
attaches by species; launch speeds and times; a physical scaling of mass for a flyer larger than any known
(a paper, if one exists); membrane thickness.

**Chains** (C5): girth along the body of snakes and eels (head, mid, vent, tail as shares of length);
head and tail length against snout–vent length; vertebra counts; fin origins as shares of length for named
fish; per-species wavelength and amplitude envelope (Di Santo et al. 2021, Dataset S2, CC0, Dryad);
lateral undulation speed in body lengths per second.

**Arthropods** (C7): segment lengths (coxa to tarsus, every leg) with spread for an ant, a cockroach, a
beetle and a spider (the tarantula table's garbled cells read again); duty against speed for a spider.

**Machines** (D1, D3, J22): Ackermann geometry and track-to-wheelbase ratios; track pitch, sprocket and
idler sizes of a named excavator or tracked vehicle; suspension travel by vehicle class; joint ranges of
a published humanoid and a quadruped robot (primary); three-view dimensions of public-domain machines.

**People** (A, H): the spine's standing curves (lumbar lordosis, thoracic kyphosis, cervical) in degrees
with spread, by sex and age; the clavicle's length and angle and the scapula's rest place against the
corpus's landmarks; muscle volumes or cross-sections along the arm, thigh, calf and trunk (A14 has two,
A16 none); a woman's trunk sections between the hip and the shoulder; children and old age (A2 thin);
turning and stairs (H2).

**Trees** (E1, E2): branch angles by order for oak, maple, birch, beech, Douglas fir, spruce, pine, willow
(measured, e.g. by laser scans); branch length and radius ratios per order from field measurement (Bentley
et al. 2013; Malhi et al. 2018); whorl spacing of conifers; olive, coconut, baobab and acacia forms.

**Terrain and water** (F3, G2, G3): colour and roughness of rock, soil, sand, snow by type (measured);
the steps of the erosion methods as published; water colour by depth and by Jerlov type; a waterfall's
path; eddies and standing waves' places; lakes' shores.

**Not research**: grass and tree costs on the integrated GPU (measured here, not looked up); the undead's
wasting and a dragon's six limbs (no measurement exists: the engine's own, said so).

## D. Questions only the owner can answer

Each with the answer this note would choose, so the lead can decide most without the owner.

1. **The name, Exemplaria.** Chosen: yes. Lead can decide.
2. **Fantasy creatures have no measurements.** May the engine set their numbers itself, from the nearest
   real plan (a dragon's legs from big cats, its wings from bats and pterosaurs scaled by C4's loading),
   each such number marked `own` with its reasoning? Chosen: yes; there is nothing else to measure, and
   the marking keeps it honest. Lead can decide.
3. **A dragon with four legs and two wings, or a wyvern?** Chosen: both, as two plans; the four-legged one
   first, because the owner said "dragons". Lead can decide.
4. **Undead early as skeletons, fleshed undead with people.** Chosen: yes (section A.10). Lead can decide.
5. **A copy, never a reference.** The roadmap's words already say a project copies; chosen as written.
6. **Research round seven** (section C), and **signing in to Dryad** for three CC0 data sets the delivery
   could not reach. Only the owner can ask for the first and do the second. Chosen: ask for C's quadruped
   girths and necks first: without them every animal's body is the engine's own guess.
7. **Numbers from Wikipedia under CC BY-SA** (C10, D3 and some of J22): facts are used with the source
   named, as the research README's rule is; chosen: use them as facts and prefer the primary source as
   each is typed. The owner may prefer none from such pages.
8. **Pixel-art output in the 2D baker.** Chosen: yes, as an option under the project's palette.
9. **Which kinds first for Sepulcra's maps 2 and 3** (the frozen and the volcanic): the lead can choose
   them from the plans built first (a skeleton, a wolf, a machine) and say so to the owner.
10. **Faces and hair are a separate instance's** (the owner's words). The note gives that lane only what it
    needs (A.12); when it starts is the owner's.
