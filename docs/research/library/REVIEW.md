# Review of the library research (2026-11-17)

Claude's review of the files in this folder on the day they were delivered. Seven readers (Sonnet 5.5), one
for each group of topics, read every researcher's file whole and every checker's summary, and were asked
for: coverage against the brief, what is still secondary or unverifiable, the licences, their own check of
a few numbers against the primary sources, and what the engine can use. Their reports are condensed here.

**In short.** Useful, and most of it fit to build from. The readers fetched primary sources for about
forty numbers or sets of parameters: every one they could reach matched, and none did not match (a number
of sources could not be opened by their tool, and are unchecked by them). Nothing looks
invented, and no wording copied from a book was found. The files say what was not opened. **Every number
is still checked against its source when it is typed into the engine.**

## What to build from first, and what is thin

| Group | Fit to build from | Thin, missing, or the engine's own to decide |
|---|---|---|
| A. The human body | An adult body by sex. A1-1 and A1-3 (segment lengths, and their ratios to stature with spread, correlation and a fitted line), A3-1 and A3-2 (girths and depths, and how they follow mass and stature: the "build"), A4-1 (segment masses, centres of mass, radii of gyration: sums to 100%), A2-1 (growth, birth to 19 years), A5-1 (joint ranges by sex and age), A6-1 and A6-2 (hands and feet) | Joint ranges are the weakest table: thumb, toes and several axes are missing, one source's values are administrative and not measured, and sources give 9.5 to 28 degrees for hip extension. Children and the elderly beyond stature and head. Muscle and fat as volumes. Where the joint centres are. Secondary and not to be used as primary: A1-4 (read off a redrawn figure), A4-5 (from a course's copy), A2's head-height ratios |
| B. Faces, heads, hair | B1 Tables 1 and 3 (head and face measures by sex with spreads), B2 Tables 1 and 2 (what differs by sex; growth from 3 to 25), B3 Table 1 (action units and the muscle behind each), B4 Table 1 (the viseme set), B5 (blinks, lids, pupils), B7 (skin tone swatches and colour by group), B8's table of which body and face models may be used | How far each action unit moves the face (three movements only). A mouth shape for each viseme. A taxonomy of hairstyles (asked for, absent). Ageing in men, and outside one country's women. Ears, chin, brow ridge |
| C1 to C5. Mammals, gaits, birds, wings, fish, snakes | C1 sections 1.4, 1.5 and 1.13 (big cats, horse, wolf, bears: segment shares and bone lengths), C1 section 2 (limb length from body mass), C2 sections 1a, 2 and 3 (a dog's footfalls, when gaits change, stride and duty factor), C4 (wing bones, wing loading, mass from span), C5 (a fish's body wave; four ways a snake moves) | Neck and tail lengths, for every species. Skull sizes. How a wing folds on a body with four legs, and wingbeat timing for a large flyer. Fin positions |
| C6 to C9. Reptiles, insects, coats, groups | Gaits and joint angles for lizards, crocodiles, tortoises, insects and spiders; C9's models of a flock, a herd of sheep and a raft of ducks with their parameters | Limb proportions for lizards, frogs and turtles. C8 gives one pair of reaction-diffusion parameters and no map of which pairs give stripes, spots or worms. No general weights for flocking exist in the source |
| D. Machines | D1: robot arms and legged robots (joint limits, link geometry), an excavator, the cylinder formula; D2: tolerances, panel gaps, rivet and bolt ratios, corrosion rates | Vehicles: steering, suspension, tracks, cranes. What makes a machine read as heavy, old or precise has no study behind it |
| E. Plants | E1: the tables of a small complete branching generator, the divergence angle, crown and height against trunk diameter, four published trees' parameters; E3: blade sizes for about thirty species; E4: biome extents, canopy heights, stand density | The equations of the best-known tree model are only in its paper. Branch angles by order for common species. Leaf shape and veins, and roots (E2). Costs of grass renderers were measured on strong GPUs, not an integrated one. One oak height in E1 is a 5% quantile, not an average |
| F. Terrain | F1: angles of repose, river profiles, the ratios of drainage networks and meanders, valley shapes, cinder cones, beaches; F2: constants and costs for thermal, hydraulic and droplet erosion | The steps of the erosion methods are outlined, not given. F3 is the thinnest file: no roughness for rendering and no rock colour by type |
| G. Water | G1: wave spectra with their constants, how depth changes them, breaking, sea states, whitecaps | G2 is about half of what was asked (no eddies, no path of a waterfall, no lakes). G3: colour by depth, scattering and water types are nearly absent |
| H. Motion | H1 sections 1, 2a, 3a and 4 (a walk: its timing, cadence and stride against speed, joint curves for four groups, the change to a run), H3 (breathing, sway, shifts of weight, fidgets, blinks), H6 (one physics walker in full, one rule for placing a step), H5 (which motion-capture sets may be used for what) | Running (peaks only), stairs, turning while walking, how much an emotion changes a posture. The walking set is from a treadmill |
| I. Prior art, garments, materials | I3: measured values for metals, dielectrics, stone, cloth; I1: what seventeen shipped procedural systems did and what went wrong | I2 is a survey: one way of fitting a garment is described well enough to build. Leather, and measured roughness |

## Rights, as the readers found them

- **Facts, never tables copied whole, from sources that are non-commercial or no-derivatives.** They are
  named in each file's licence section. The clearest case: B3's Table 3 gives every row of a table from a
  no-derivatives paper; the engine re-derives what it needs or cites it.
- **No game's list of controls is mirrored** (B8 lists what five shipped character creators offer, from
  fan wikis: it is there to learn from).
- **Not to be used at all**: the SMPL family of body models, FLAME before its 2023 open release, the Basel
  face models (all non-commercial); the code of MakeHuman and its Blender add-on (AGPL, GPL), though its
  assets are public domain; the learned garment models. Material rows fitted from the MERL set are for
  research only and stay out of what ships.
- **The walking set's data files carry no licence** (the article about them is open): its numbers are
  cited, and its curves are not shipped without its authors' word.
- **Motion capture** is never shipped by this engine in any case; H5 says which sets allow commercial use
  (four) and which may only be studied.

## What is to be researched again when its part comes

Water's colour and scattering (G3) and the rest of rivers and falls (G2); leaves, veins and roots (E2);
ground as render values (F3); the movement of the face by action unit (B3) and hairstyles (B6); the map of
pattern parameters (C8); running and stairs (H1, H2); vehicles (D1); children and old age (A2); necks and
tails (C1).
