# The library research, as delivered

What [LIBRARY_RESEARCH_BRIEF.md](../LIBRARY_RESEARCH_BRIEF.md) asked for: the published measurements behind
ROADMAP M6's parts 4 and 5 (a human figure from proportion tables, then the libraries: people, animals,
machines, plants, terrain, water, motion). Delivered on 2026-11-17 by the owner, who had it researched by
Grok over two days: "at my request, I had Grok Bot provide links and citations as well as actually ensuring
the license allowed this usage. Took me 2 days to ensure we got everything."

**45 topics, two files each.** `A1-segment-lengths.md` is the researcher's file: tables as plain text, every
number with its source, page and link, the population or species, what could not be found and where sources
disagree, and the licence of each source. `A1-segment-lengths.md.verify.md` is a second checker's report on
it, row by row (VERIFIED, FIX with the corrected value, UNVERIFIABLE with the reason); the fixes are applied
in the researcher's file, which says so at its top. **The files are kept as they were delivered**, so that a
checker's line numbers still point at the lines they were written about.

| Group | Topics |
|---|---|
| A | The human body: segment lengths, proportions by age, girths and depths, segment masses, joint ranges, hands and feet, the skeleton |
| B | Faces, heads and hair: measurements, sex and age, expressions, visemes, eyes, hair, colour, what character creators offer |
| C | Animals: mammal proportions and gaits, birds, bats and pterosaurs, fish and snakes, reptiles and amphibians, insects and spiders, coat patterns, group behaviour |
| D | Machines: jointing, and what makes a machine read |
| E | Trees, leaves, bark and roots, grass and ground cover, vegetation by biome |
| F | Terrain: landform numbers, erosion algorithms, ground materials |
| G | Water: ocean waves, rivers and falls, appearance |
| H | Motion: human gait, other movement, idle motion, posture and emotion, motion-capture licences, rule-based locomotion |
| I | Prior art in procedural generation, garment fitting, material values |

## How it is used

- **It is the provenance of the engine's tables, not the tables.** A number is typed into the engine by hand,
  with its primary source named beside it, and **checked against that source as it is typed** (HANDOFF: what
  comes back from outside is checked before it is used). Nothing here is read by the engine or by a test, and
  nothing is downloaded into the engine.
- **A row marked secondary, computed by the researcher, or unverifiable is not used as if it were primary.**
  [REVIEW.md](REVIEW.md) says which tables to use first in each group, which are thin, and what is still
  the engine's own to decide.
- **Rights.** These files cite measurements and methods as facts, in their own words; each names its
  sources' licences. A source under a non-commercial or no-derivatives licence is cited for its facts and
  never has a table of its own copied whole into the engine. No commercial game's list of controls is
  mirrored. No third-party code, model or dataset named here is taken into the engine; several may not be
  (the review lists them).
- **People are not all alike, and the samples say whom they measured**: soldiers, workers, the students of one
  country, the women of another. Nothing here is universal, and the engine presents none of it so.

## Reference pictures (2026-11-17)

Beside the numbers, something to look at: 357 pictures gathered in three rounds for
[REFERENCE_IMAGES_BRIEF.md](../REFERENCE_IMAGES_BRIEF.md),
[REFERENCE_ROUND2_BRIEF.md](../REFERENCE_ROUND2_BRIEF.md) and
[REFERENCE_ROUND3_BRIEF.md](../REFERENCE_ROUND3_BRIEF.md), with their indexes, in
[reference-images/](reference-images/README.md).

## Round two: motion and physics (2026-11-17)

Twelve more files, delivered the same evening in answer to
[REFERENCE_ROUND2_BRIEF.md](../REFERENCE_ROUND2_BRIEF.md), in the same form (every value with its
source, its page or table and a link; "Not found" and "Sources disagree" at each file's end; "computed
by me" marks arithmetic on cited numbers, never a measurement). They are here as delivered.

| File | What it holds |
|---|---|
| [H7](H7-running-joint-angles-and-motion-additions.md) | Running: peak joint angles, torques and stride by speed (Fukuchi 2017, CC BY 4.0); and a map of what H1, C2, B3 and H5 already hold of the brief's motion rows (walking curves with pelvis and trunk, stride against speed, four-legged footfalls, the face's muscles) |
| [H8](H8-recorded-motion-datasets.md) | Recorded human motion that may be used: links only, each licence in its own words, the format, and which of walk, run, jump, sit, climb, carry each has (delivered as `motion-datasets.md`) |
| [J1](J1-friction.md) | Friction, static and sliding, for 13 pairs of materials; braking from it (computed) |
| [J2](J2-restitution.md) | Bounce: what the rules of tennis, basketball and table tennis require of a dropped ball, with the coefficient computed; balls' masses and sizes |
| [J3](J3-densities.md) | Densities of 18 solids, 7 liquids, 9 gases |
| [J4](J4-drag-and-air.md) | Drag coefficients with the area each is taken on; a falling person; air density by altitude (computed from NASA's model) |
| [J5](J5-vehicles.md) | Tyres' rolling resistance; a car's drag. Suspension figures were not found |
| [J6](J6-human-capabilities.md) | World records (100 m, long jump, high jump), grip strength by age and sex, running speed and ground force |
| [J7](J7-animal-capabilities.md) | Animals' top speed against body mass (a published model; one constant still missing) |
| [J8](J8-projectiles.md) | Gravity, drag and terminal speed of balls (computed) |
| [J9](J9-water.md) | Water's density by temperature, the viscosity of some twenty fluids, the speed of sound in 17 media |
| [J10](J10-other-worlds.md) | Gravity, pressure, air density and temperature on Earth, the Moon, Mars, Venus and Titan |

**Not yet checked.** The 45 topics above each came with a second checker's report and were then
reviewed here ([REVIEW.md](REVIEW.md)); these twelve came with neither. Until they are, the rule that
holds for all of the library holds harder for them: a number is checked against its source as it is
typed into the engine. Still missing by their own account: running joint angles as curves (the open
file could not be fetched by the researcher), suspension and engine figures, bounce for everyday
materials.

## Round three: effects, light, breaking, and the physics the owner named (2026-11-17)

Twenty-three more files, delivered the same night in answer to
[REFERENCE_ROUND3_BRIEF.md](../REFERENCE_ROUND3_BRIEF.md), in the same form, here as delivered (the
delivery's `H8-running-numbers.md` is `H9` here: `H8` was taken), with its list of sources that could
only be linked ([R3-links.md](R3-links.md)).

| File | What it holds |
|---|---|
| [A8](A8-grip-strength-across-life.md) | Grip strength across life |
| [C10](C10-giant-extinct-animals.md) | Giant and extinct animals: sizes and masses, for creatures made from them |
| [D3](D3-robots-and-machines.md) | Robots and machines: published dimensions, masses and joints |
| [G4](G4-sky-and-weather.md) | Sky and weather: clouds, rain, snow, fog |
| [H9](H9-running-numbers.md) | Running, in numbers |
| [I4](I4-buildings-and-architecture.md) | Buildings: storey heights, doors, stairs, roofs |
| [J11](J11-fireballs-and-flames.md) to [J13](J13-lightning-and-thunder.md) | Fireballs and flames; the colour of light by temperature; lightning and thunder |
| [J14](J14-blast-and-explosions.md), [J15](J15-fracture-and-breakage.md), [J21](J21-destruction-physics-and-methods.md) | Blast against distance and charge; fracture and breakage; how destruction is modelled |
| [J16](J16-light-levels-and-sky.md) to [J18](J18-fog-smoke-scattering-falloff.md) | Light levels from a moonless night to noon; the eye's adaptation; fog, smoke and falloff |
| [J19](J19-shape-drag-inertia-tumbling.md), [J20](J20-restitution-extended.md) | Drag, inertia and tumbling by shape; more on bounce |
| [J22](J22-ground-vehicles.md) | Ground vehicles: masses and dimensions, tyres, suspension, brakes, two wheels |
| [J23](J23-aircraft-and-air.md) | Aircraft and the air: the standard atmosphere, an aerofoil's lift and drag, nine aircraft, helicopters, wind |
| [J24](J24-space-and-other-worlds.md) | Space: planets and moons, orbits, engines, re-entry |
| [J25](J25-bullets-arrows-and-thrown-things.md) | Bullets, arrows and thrown things |
| [J26](J26-swords-and-hand-weapons.md) | Swords and hand weapons |
| [J27](J27-sound-loudness.md) | How loud things are |

Its own coverage map says what is still thin: engine curves and gear ratios, cambered and thin
aerofoils, control figures and propeller efficiency, tyre load, tracked traction.

**Rounds two and three are not yet checked** (the thirty-five files above, H7 to J27). They came
without a second checker's report. The owner, 2026-11-17: "A second checker pass.. for round 2 and 3
will need to be dealt with at a later date. I'll need you to log a reminder before we go onto MacOS",
and later the same evening that the pass is being run after all and its files will follow, with a
fourth round on the gaps. When they come, each report goes beside its file, its fixes are applied, and
the files are reviewed as the first 45 were. Until then a number from them is checked against its
source as it is typed into the engine.
