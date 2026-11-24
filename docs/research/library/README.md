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

**Second-checked on 2026-11-18** (see "The second check of rounds two to four" below): 296 rows, 281
verified, 13 to fix, 2 unverifiable. The fixes are applied and each file has its report beside it.
Still missing by their own account: suspension figures, bounce for everyday materials (running joint
angles as curves and engine figures came with round four: H10, J28).

**OpenStax is not free to copy.** J1, J3, J4 and J9 (and J8 through them) take their values from
OpenStax *University Physics*, which is **CC BY-NC-SA 4.0**: non-commercial and share-alike. The
values are facts and may be used; its text and its tables as compiled may not be copied into a
product. The files said CC BY 4.0 as delivered, and the second check corrected each. J2's figure for
table tennis came from a secondary page and is marked so.

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

**Second-checked on 2026-11-18**: 832 rows, 752 verified, 51 to fix, 29 unverifiable. The fixes are
applied and each file has its report beside it. Where a value was taken from a Wikipedia page, the
primary source is named where the page gives one, and not always: J26's dimensions of weapon types,
J27 and parts of J24 and J25 rest on Wikipedia alone, and say so.

## Round four: the gaps of the first three (2026-11-18)

Seven files and two folders of data, delivered with their second checker's findings already made
(235 rows: 216 verified, 15 to fix, 4 unverifiable), and placed here with those fixes applied. The
delivery's `H9` is `H10` here (`H9` was taken), and its list of sources that could only be linked is
[R4-links.md](R4-links.md).

| File | What it holds | Thin on |
|---|---|---|
| [B9](B9-face-teeth-eye-ear-dimensions.md) | Facial breadths by sex with percentiles, tooth crowns, eye biometry (8,532 people), ears | the eye's opening, crown heights, eyes of more than one people, spread of the ears (they are CT scans of hospital patients) |
| [H10](H10-joint-curves-walk-run-and-arm-swing.md), [H10-data/](H10-data/) | Hip, knee and ankle angle and moment curves at 101 points of a stride, walking and running at seven grades (CC0 data, four CSV files); how far the arms swing | 8 subjects; no shoulder, elbow or trunk curves; no arm swing in running |
| [I5](I5-fabric-drape-bending-weight-ease-and-patterns.md) | Bending and drape of 9 fabrics as measured, a cloth simulator's inputs, drape by weight, ease, open patterns | no spread of drape, no full set of fabric measures, ease from a secondary source |
| [J28](J28-engine-torque-curves-and-gears.md), [J28-data/](J28-data/) | Measured full-load torque of a petrol, a turbo petrol and a diesel engine and a hybrid's motor (three CSV files); an 8-speed gearbox's ratios | no battery car's curve, nothing at high revs, no manual gearbox |
| [J29](J29-bullet-drag-bc-and-penetration.md) | Drag of the two standard bullet shapes against speed, makers' ballistic coefficients, the armour ladder, what walls stop, gelatin | the drag tables are retyped from a secondary source; no pistol or military coefficients |
| [J30](J30-sword-balance-percussion-inertia-and-cutting-tests.md) | Where swords balance and where they strike best, their masses, moments of inertia (computed), cutting tests against mail | no tests on plate, no set of longswords, makers' figures are secondary |
| [J31](J31-ion-engines-and-reentry-heating.md) | An ion engine's throttle table, the constant of re-entry heating, what flown capsules met | no second ion engine, Hall thrusters from secondary sources |

Its pictures (ten of twelve) are in the pictures' repository.

## The second check of rounds two to four (2026-11-18)

An independent checker went through rounds two, three and four row by row against the sources:
1,363 rows, 1,249 verified, 79 to fix, 35 unverifiable. What was done with it here (2026-11-19):

- **Each file has the checker's summary beside it** (`<file>.md.verify.md`), word for word as it was
  relayed; the checker's own row-by-row tables stayed with the checker.
- **Every fix is applied** as the checker gave it: wrong values, authors, pages, figures and licences.
  The corrections were applied by assistants working from the checker's findings and are not yet
  re-opened against the sources here; what each file changed is in a note under its title.
- **A value the checker could not confirm is moved, not deleted**, under `Unconfirmed (second check,
  2026-11-18)` in its file, with the reason. **Nothing under that heading is typed into the engine**
  until a source that gives it has been opened. The largest: J26's table of sword types (the museum's
  search it was made with has been retired; the twenty single swords beside it were found exact), the
  eight blast formulas of J14 that come through one unopened paper, and one cell of J29's table of
  what walls stop, whose pairing was wrong as retyped.
- **Spreads the sources give were added** where a single number had been kept, and a source that
  contradicts itself is under "Sources disagree" (the Sun at 128 klx above the air against 100 klx on
  the ground; a clear sky's shade at 6,000 against 20,000 lx; the lunar rover's top speed).
- **Still open:** in H3, whether Duarte and Freitas give their value on p. 186 or p. 187; in J20, which
  of two spreads the checker gave belong to which coefficient (marked "needs a look"); access dates
  that some of round four's sources never had.

The rule for all of the library stands: a number is checked against its source as it is typed into the
engine.

## Round five: the figure's surface (2026-11-19)

What [REFERENCE_ROUND5_BRIEF.md](../REFERENCE_ROUND5_BRIEF.md) asked for, delivered the day it was asked:
eight files of numbers, and 217 pictures of the trunk, the shoulder and arm, hands and feet, which are in
the pictures' repository ([specula-imagines](https://github.com/IronAdamant/specula-imagines), its
`INDEX-round-5.md`; that index also has how well each line of the brief is covered, and what was found
and could only be linked).

| File | What it holds | Thin on |
|---|---|---|
| [A9](A9-hand-dimensions.md) | 86 dimensions of the hand for women and men (Greiner 1991: finger lengths by segment, breadths, depths and girths at each joint, the heights of the webs), finger segments as shares, and the join to ANSUR II | the hand's thickness, nails |
| [A10](A10-foot-dimensions.md) | 33 measures of the foot and lower leg for women and men (Parham 1992: lengths, breadths, heights of the ankle bones, girths), ratios, ANSUR II's foot | toe by toe lengths, the arch's height by type, a foot loaded against unloaded |
| [A11](A11-joint-ranges.md) | Ranges of the wrist, hand, ankle, toes and shoulder (NASA's standard, 5th to 95th percentile; a CDC study by sex and age), finger and thumb joints, how the shoulder blade turns as the arm rises at 30, 60, 90 and 120 degrees by sex, how the collar bone moves | the arm above 120 degrees, finger ranges by sex; three digits of NASA's figure are marked (?) for blur |
| [A12](A12-joint-centres.md) | Where a joint's centre lies from the landmarks on the skin (Dempster 1955, from eight cadavers: the collar bone's two joints, the shoulder, elbow, wrist, hip, knee and ankle), and the links between them as shares | no data for women; no finger joints; the ankle's axis as numbers (the source could not be read) |
| [A13](A13-trunk-sections.md) | Breadth, depth and girth of the chest, waist and hip by sex and build, from ANSUR II, with a shape number for each: the exponent of the superellipse whose outline has the measured girth | the chest's shape numbers are not an outline (the file says why); no level between; no measured outline |
| [A14](A14-muscle-thickness.md) | The calf muscle's thickness by age and sex; the thigh's front muscle as a share of the thigh's length, by age and sex | every other muscle asked for (chest, shoulder, back, buttock, belly): not found |
| [A15](A15-skinfolds.md) | Skinfolds at the back of the arm, under the shoulder blade, over the hip bone and on the thigh, by sex, age and build (NHANES) | chest, belly, flank and calf; one code of the data file is inferred |
| [A16](A16-muscle-attachments.md) | Nothing as numbers: where a muscle attaches, as a share along its bone, was not found. It names the plates to read them from (Spalteholz's, in the pictures' repository) | all of it |

- **Each file has its checker's rows beside it** (`<file>.md.verify.md`): the delivery's own checker
  re-opened the sources of 26 rows (20 verified, 4 fixed before delivery, 2 unverifiable), tried every
  link, and read the licence of every picture again.
- **A13 was held against the engine the day it came.** Its 24 means and spreads of the whole sample are
  the engine's own to a rounding (the body's table in `specula-corpus` is computed from the same public
  files), and its shape numbers come out of a solver written here from its mean breadths, depths and
  girths: 2.83 and 2.73 at the waist against its 2.83 and 2.71, 3.59 and 3.39 at the hip against its
  3.64 and 3.40. The rows by build and everything in the other seven files are **not** re-opened here.
- **What it says of the figure:** a waist is squarer than an ellipse (a shape number near 2.7 to 2.8,
  where an ellipse is 2) and a hip squarer still (3.3 to 3.7). The figure's trunk already has one
  squareness for all its sections, 3, found by fitting the tape before this file came
  ([FIGURE.md](../../design/FIGURE.md)): the survey's numbers lie either side of it, which confirms it,
  and says the next step is a squareness for each section. **The chest's numbers are not to be used
  as an outline**: the tape there bridges hollows that the breadth and the depth are measured across
  (for one man in seven the chest's girth is as long as the rectangle round its breadth and depth, or
  longer), which is what the figure's own measuring had found: no section as broad and as deep as
  the survey's chest can be as far round as its chest girth.
- **Almost everything pictured is one man** (Richer's model, and the men of three atlases). A woman's
  trunk as a surface, builds, and the levels between the shoulder and the hip in cross-section were
  not found under an open licence.

The rule stands for these as for the rest: a number is checked against its source as it is typed into
the engine.

## Round six: distance fields, for the forms as sculpt primitives (2026-11-20)

One file, asked for the same morning (the owner offered research for the shape library's
sculpt primitives; Claude said none was needed to build them and named six sources worth
citing rightly) and delivered within the hour.

| File | What it has | What it could not confirm |
|---|---|---|
| [I6](I6-distance-fields-citations.md) | Citations, working links, licences and what each source claims, for: Quilez's 2D and 3D distance functions (a 2D field that is exact stays exact when extruded or revolved; unions, differences and intersections give bounds); Bærentzen and Aanæs 2005 (the sign of a mesh's distance by the angle-weighted pseudonormal, for a closed, orientable mesh); Jacobson, Kavan and Sorkine-Hornung 2013 and Barill et al. 2018 (inside and outside by winding numbers, for meshes with defects); Hart 1996 (a field may fall short of the distance and must never exceed it); the smooth minimum (Quilez: in its normalised form the blend's `k` is the most a shape is thickened, and the polynomial family never overstates the distance; Barbier et al. 2025, CC BY, the best peer-reviewed use of it) | the licence of Quilez's article text and figures (only his code snippets are stated MIT); MIT headers inside his Shadertoy shaders; whether the free copy of Hart's paper matches the journal's text; whether Dekkers et al. 2004 states a blend's error bound; no peer-reviewed paper was found that states the smooth minimum's deviation bound as a result |

- **It was brought up to date three times the same day** (six more passes, three of them with a
  second check, all in the one file): the gaps of the first pass followed up, and modern free sources found for what
  sat behind paywalls.
  - *A proof that a distance field never rises faster than one in one*: Bálint, Valasek and Gergó
    2019 (Acta Cybernetica, free), from the triangle inequality, not by quoting Hart. The same
    authors 2023: a union, an intersection and a difference of such fields stay bounds, and their
    error against the true distance can be arbitrarily large.
  - *A proof that a blended union stays within that rule*: Dekkers, van Overveld and Golsteijn's
    1996 technical report (free; the 2004 journal text was not reached), Appendix A, for a blend
    term whose slope lies between −1 and 0.
  - *The blends' formulas in peer-reviewed papers*: the quadratic kernel (Barbier et al. 2025) and
    a cubic one (Zanni 2023/2025, who sets his tracer's bound to 1.45 "to accommodate for field
    compression associated with our compact operators": a blend is not to be taken as keeping the
    rule without a test).
  - *Settled*: Quilez's article text has no licence at all (his code is MIT, read on a shader of his
    own); the article numbers of the two winding-number papers; the name the second paper's author
    was published under.
  - *How far a smooth minimum falls below the minimum* (a sixth pass, with an addendum): for the
    exponential one, three peer-reviewed papers state it as a numbered result (Molnar and Ames
    2023, Theorem 4; Rabiee and Hoagg 2025, Proposition 1; Igel 2024, equations 5 and 6): at most
    `k ln n` for `n` shapes, `k ln 2` for two, reached where the fields are equal. For the
    quadratic one, a quarter of `k`: stated in Li et al. 2026 (a journal article; read in its
    preprint only, which was revised before acceptance). For the cubic one, a sixth of `k`: still
    in no peer-reviewed paper found, and one line of arithmetic from its formula.
  - *A seventh pass, for what was left*: a working official copy of Igel's paper; two free notes
    that say the quadratic one's quarter of `k` in words (neither peer-reviewed); and still no
    paper, note or thesis that states the cubic one's sixth.
  - *Still not settled*: Hart's journal wording against the free manuscript copies; the wording of
    Li et al.'s published article.
- **The engine does not wait for those**: every fact the sculpt work leans on is stated and
  proved from first principles in the repository's own words, the cubic join's sixth of `k`
  among them, in [docs/design/DISTANCE_FIELDS.md](../../design/DISTANCE_FIELDS.md), and held in
  numbers by a test. This file is its credits and further reading.
- **Its last three passes each have a second check inside them** (the fifth: six of eight sources
  confirmed, two corrected on their licences, one whose free copy could not be re-opened; the
  sixth: all confirmed, one licence corrected; the seventh: confirmed); the earlier passes have
  none. Nothing in it has
  been re-opened against its sources here yet. It is a list of where to read,
  not of numbers: each formula is derived and written here, and held by a test against a nearest
  point found by search, whatever its source says.
- **Kept as delivered but for a few words**: the phrases naming the time zone it was compiled in
  were taken out of nine heading lines.
- **What it says for the design** (FORMAE's sculpt stage, when it is built): raise and turn the
  library's plane figures from an exact distance in the plane, which stays exact; a boolean or a
  blend of fields is a bound, so the mesher is told a slack and bounds are widened by the blend's
  radius; a mesh's own sign is safe by pseudonormals only where the mesh is closed and well made
  (the library's are checked so), and by winding numbers otherwise.
- **Rights.** Formulas are mathematics and are written here in the engine's own code; no code is
  copied from the articles (their snippets are MIT, their text and pictures are not licensed).

## Round seven: what the sample library still lacked, and primary sources (2026-11-24)

Delivered by the owner the same day [LIBRARY_RESEARCH_BRIEF_ROUND7.md](../LIBRARY_RESEARCH_BRIEF_ROUND7.md)
was written, researched by Grok in several passes, with the owner fetching by hand what refused a
script: "This is the best I can do, right now, there are some gaps as I couldn't find alternative
documents, and some remained blocked, so these are what we'll make do with." The brief asked 159
numbered things: three data sets (DS1 to DS3), 49 requests for the sample library's kinds (Q1 to
Q49: four-legged animals, sprawling reptiles, birds, bats, snakes and fish, arthropods, machines,
people, trees, terrain and water), the primary source of 82 numbers the earlier rounds had only
from Wikipedia (W1 to W82), and 25 rows an earlier checker had marked unverifiable (U1 to U25).

**What came back, by the delivery's own count** ([its own overview](R7-README-as-delivered.md)):
44 answered or confirmed (11 of them corrected: the primary source was found and the earlier value
was wrong or mislabelled), 66 partly answered, 48 searched and not found, one not needed. No Q row
is counted as fully answered, because each file lists what it did not find.

| File | What it answers |
|---|---|
| [R7-00](R7-00-dryad-DS1-DS3.md) | The three data sets: access and licences (all three CC0); DS2's bird bones; DS3's swimming amplitude envelopes |
| [R7-01](R7-01-DS1-serio-values.md) | DS1 (Serio et al. 2024: limb bones and skulls of living and fossil mammals) read from the files, with what is odd in them |
| [R7-02](R7-02-quadruped-gap-fill.md) | What DS1 lacks: dog, elephant, rabbit and rat bones; body mass |
| [R7-2A](R7-2A-quadruped-body-bulk-Q1-Q8.md), [Q8](R7-2A-Q8-footfalls.md) | Four-legged animals: girths, neck and tail, vertebral counts, heads, mass, stance angles; footfall timing |
| [R7-2B](R7-2B-sprawling.md) | Sprawling animals: lizards, tortoises, crocodylians |
| [R7-2C](R7-2C-birds.md) | Birds: leg and neck bones, wingbeat |
| [R7-2D](R7-2D-bats-winged.md) | Bats and large flyers: digits, take-off, the limits of size |
| [R7-2E](R7-2E-chains.md) | Snakes, eels and fish: vertebrae, proportions, swimming |
| [R7-2F](R7-2F-arthropods.md) | Spiders and insects: leg segments and gaits |
| [R7-2G](R7-2G-machines.md) | Machines: steering, tracks, suspension, robots' joint tables |
| [R7-2H](R7-2H-people-biped.md) | People: the spine's curves, shoulder girdle, muscle volumes, girths, proportions by age, turning and stairs |
| [R7-2I](R7-2I-trees-plants.md) | Trees and plants |
| [R7-2J](R7-2J-terrain-water.md) | Terrain and water: reflectance, erosion, water colour, waves |
| [R7-3](R7-3-primary-sources.md), [3b](R7-3b-primary-sources.md), [34](R7-34-primary-checks-W-and-U-rows.md) | The W rows: primary sources for the numbers that had only Wikipedia |
| [R7-4](R7-4-unverified-rows.md), [4b](R7-4b-unverified-rows.md) | The U rows |
| [R7-5](R7-5-alt-sources.md) | Numbers from free copies and substitutes found for the blocked sources |
| [R7-5b](R7-5b-browser-downloads.md) | Numbers from six files fetched in a browser: three erosion papers, a pterosaur mass paper, the stick insect's legs, a bat supplement ([its note](R7-5b-README-as-delivered.md)) |
| [R7-6](R7-6-free-equivalents.md) | A third pass: a free equivalent for each source that stayed blocked |
| [blocked, all](R7-blocked-downloads-ALL.md), [alternatives](R7-blocked-downloads-ALTERNATIVES.md) | The 78 sources that refused a script, and what became of each: 20 exact free copies, 7 substitutes, 5 abstracts only, 46 still blocked when that list was made (some were fetched after it: R7-5b and R7-6) |

Each research file has a second checker's file beside it (`.md.verify.md`, or `.verify.md` for
2F to 2J), by the same researcher. **They are uneven:** the checks of R7-01, R7-02, 2A, 2F to 2J,
the W and U files and R7-6 give a verdict row by row; those of 2A-Q8, 2B, 2C, 2D, 2E, R7-5 and
R7-5b do not.

- **Corrections the delivery makes to numbers the earlier rounds hold** (as delivered; being
  held against the packed sources here, see below). The benchmark bicycle is self-stable between
  4.29 and 6.02 m/s, not 5.3 and 8 (a different example bicycle). The rolling resistance of
  0.0385 to 0.0730 is a stage coach at a walk on muddy broken stone (Baker 1918), not a dirt
  road or snow. The M1A1 Abrams, the Leopard 2A6, Atlas of 2016, the ASW 22, three Universal
  Robots arms, the C-130 and the F-16C differ from their Wikipedia pages on the makers' or the
  air force's own sheets. *Tyrannosaurus*: Hutchinson et al. 2011 give the largest specimen about
  9,500 kg and a peak growth of 1,790 kg a year, against Erickson's 5,650 kg and less than half
  that growth. The 79 m Secchi depth is Gieskes et al. 1987, not Berman et al. 1985. PET's
  refractive index is 1.569 measured, not 1.575. A table of "tarsus lengths" of a tarantula held
  masses. A milk snake's "0.22 body lengths a second" was per undulation, so about 0.11.
- **Faults it found inside the data it read**, which whoever types a number in must know: DS1
  files cave-bear papers under the brown bear, mixes Pleistocene wolf tibiae with modern ones,
  and has two leopard rows 120 mm apart; DS2's column `tarsus` is the tibiotarsus; a gull's mass
  of 0.08 kg in a bird table is an error; the vertebral counts one source gives for horse, pig
  and brown bear differ from the usual ones; a sea-snake file's lengths are in millimetres where
  its own note says centimetres; one bat table prints a digit's length equal to the forearm's.
- **What stayed without a source** (the delivery's lists; the sample library marks these as its
  own estimates, `own`, never as measured): joint ranges of the limbs and neck of four-legged
  animals (Q7); body girth, breadth and depth of any bird (Q14); where a bat's wing membrane
  meets the hind limb, by species (Q18); girth along a land snake or an eel (Q22: six sea snakes
  at two stations came with R7-6, and their flattened tails are not a land snake's); a scaling
  rule for a flyer above 250 kg; branch length and radius ratios by order from field measurement
  (Q41), and measured branch angles for most species (Scots pine is the exception); the forms of
  olive, coconut, baobab and acacia (Q43); suspension travel by class of vehicle beyond one
  pickup (Q31); track pitch and sprocket sizes; turning gait and stair descent; water's colour
  by depth and by water type (Q46); a waterfall's path (Q47); chest girth of most wild animals;
  neck length of any animal but the horse; metapodial and toe bones; the footfall phases of a
  bound; a cat's gaits from an opened primary. The published steps of the three erosion methods
  (Q45), first listed as not found, came with R7-5b.
- **Edited after delivery, by a few words only:** a person's first name was replaced by "the
  owner" in two files, and the phrases naming the time zone it was compiled in were taken out
  of about two dozen lines; one heading of a checker's file ("Still open") was reworded, since
  a guard reads that phrase as a list of decisions for the owner. Nothing else was changed, so the checkers' line references hold.
- **The sources themselves are not in the repository** (about 600 MB of papers, data files and
  saved pages; several are all-rights-reserved): they are cited by address in each file.
- **Checked here** (2026-11-24), by two models that had no part in the research, against the
  source files packed with the delivery: [the animal half](R7-CHECK-animals.md) and
  [the rest](R7-CHECK-rest.md). **The delivery is careful.** About 470 numbers recomputed from
  its raw data files and about 260 read in its papers reproduced on the animal side; about 485
  were confirmed on the other. **No number this round corrects was found in the engine or the
  sample library** (searched by name and by value), so nothing already typed in has to change.
  - *Errors a builder would type in if not told:* a sea snake labelled *Aipysurus eydouxii* is
    the paper's *A. mosaicus*; a monitor-lizard table has 27 specimens, not 30, and its copied
    row repeats the first *V. tristis* row; a column headed "central cervical vertebra" is that
    vertebra's length in millimetres, not a count; a foot-posture file has 102 rows; the dogs'
    motion capture has 30 to 98 markers a file, not 98, and each standing trial is one or two
    frames; a pterosaur model's span is 11.2 m, not 13; a robot's thigh is 0.337 m by its own
    model file, not 0.332, and its "arm span 0.45 m" is its width.
  - *Corrections that are really disagreements:* the M1A1's maker sheet contradicts itself (67.6
    tons against a power-to-weight line that gives 63); the Leopard's figures are the A6M
    variant's; PET's 1.569 is one sample, a second value and not a correction; the Universal
    Robots masses are the current sheets'; the two *Tyrannosaurus* masses come from different
    methods.
  - *Left out by the delivery and in its own pack:* the caps that go with the wave-growth
    formulas (without them a long fetch gives waves taller than a fully developed sea); Scots
    pine's 9.4 branches a whorl; fish depth and width along the body for 44 species.
  - *On the earlier rounds:* F2 gives a 30 degree talus angle to Musgrave, whose paper prints
    none (it is Cordonnier's); the sample library's note of a bird's neck as "8 to 14" is wrong
    if it means vertebrae (the counts run from 10 to 26); measured walk phases in C2 are to be
    preferred to the idealised ones here.
  - *The second checker's files* are sound on transcription and weak on what a number means
    (frames, units, variants, a source that contradicts itself); seven of them give no verdict
    row by row.
  - **Each check ends with a table for builders**: which quantities can be typed in as measured
    and from which table, which are thin, and which are the engine's own estimate and what to
    anchor it to. That table, not the delivery's summary, is where a lane starts.
  - *Not checkable from the pack* (read online by the researcher and not saved, or packed as a
    refusal page): listed in each check. Those are not typed in as checked.
