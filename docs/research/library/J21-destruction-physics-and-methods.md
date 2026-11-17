# J21: Physics of destruction, and how games model it

Researched 2026-11-17 (all URLs accessed 2026-11-17). Values marked "computed by me" are arithmetic, never measurements. Extends J15 (fracture toughness, stiffness, crack energy) and J14 (blast); those values are not repeated. This file covers how things break (impact, crushing, cutting, tools, wear, losing supports, fire) and, because the engine will do this natively, how published games and research model damage. Pictures are in images/6-destruction.

## J21-1. Strength of building materials (read directly)

| Material | Value | Source |
|---|---|---|
| Ordinary concrete, compressive | 40 MPa readily available; above 40 MPa for special elements; 80 MPa or more in lower columns of tall buildings; up to 130 MPa used commercially; 14 MPa or less for lightweight mixes | Wikipedia "Concrete", https://en.wikipedia.org/wiki/Concrete (secondary) |
| Ultra-high-performance concrete | over 150 MPa compressive (definition) | Wikipedia "Compressive strength", https://en.wikipedia.org/wiki/Compressive_strength |
| Concrete, tensile | 2-5 MPa (density 2.7 g/cm³ as listed) | Wikipedia "Ultimate tensile strength", table, https://en.wikipedia.org/wiki/Ultimate_tensile_strength |
| Concrete: air content | each 1% of entrained air may cut compressive strength by about 5% | Wikipedia "Concrete" |
| Clay brick, compressive (USA) | about 7 to 103 MPa (1,000-15,000 psi), by intended use | Wikipedia "Brick", https://en.wikipedia.org/wiki/Brick |
| Clay brick, compressive (England) | up to 100 MPa; common house brick 20-40 MPa | same |
| Masonry | very high ratio of compressive to tensile strength, so loads do not spread as in elastic bodies | Wikipedia "Masonry", https://en.wikipedia.org/wiki/Masonry |
| Structural steel ASTM A36 | yield 250 MPa, ultimate 400-550 MPa | Wikipedia "Ultimate tensile strength" table |
| Annealed glass, tensile | 41 MPa | same |
| Pine, along the grain, tensile | 40 MPa | same |

Rule of thumb from these rows (computed by me): concrete is about 10 times stronger in compression than in tension (40 against 2-5 MPa). That is why unreinforced concrete and masonry crack from bending and pulling, and why a game should store separate compression and tension limits per material, as the NVIDIA Blast stress solver does (J21-7).

## J21-2. How solids break into pieces (read directly)

**Average fragment size under fast loading (Grady energy balance).** Grady (1982) equated the local kinetic energy of an expanding body to the energy needed to make new crack surface. As re-derived by Miller et al. (1999) and printed in Drugan, *Dynamic Fragmentation of Brittle Materials: Analytical Mechanics-Based Models*, UWFDM-1149, University of Wisconsin Fusion Technology Institute, January 2001, Eq. (67), https://fti.neep.wisc.edu/fti.neep.wisc.edu/pdf/fdm1149.pdf:

l = 24^(1/3) · (K_Ic / (ρ c ε̇))^(2/3)

with K_Ic fracture toughness, ρ density, c elastic wave speed, ε̇ strain rate. Equivalent form s = (24 G_c / (ρ ε̇²))^(1/3) with G_c fracture energy, so size falls as ε̇^(−2/3) (abstract of "Analysis of the brittle fragmentation of an expanding ring", *Computational Materials Science* 2006, https://www.sciencedirect.com/science/article/abs/pii/S0927025605003629, search extract). Both sources warn that energy models overestimate: measured and simulated fragments are often much smaller, and Drugan finds the instantaneous-fragmentation assumption holds only at very high strain rates.

### Table J21-2a. Grady fragment size (computed by me; K from J15, c = √(E/ρ))

| Material | c (m/s) | ε̇ = 10 /s | 100 /s | 1,000 /s | 10,000 /s |
|---|---|---|---|---|---|
| Soda-lime glass (K 0.75 MPa·m^½, ρ 2,500, E 60 GPa) | 4,900 | 97 mm | 21 mm | 4.5 mm | 1.0 mm |
| Concrete (K 1.0, ρ 2,400, E 20 GPa) | 2,900 | 171 mm | 37 mm | 7.9 mm | 1.7 mm |

Treat these as upper bounds on typical piece size. Strain rate for a game impact can be estimated as impact speed / object size (my approximation, not from a source).

**Fragment size distributions.**
- Oddershede, Dimon and Bohr, "Self-organized criticality in fragmenting", *Phys. Rev. Lett.* 71, 3107 (1993), doi:10.1103/PhysRevLett.71.3107 (abstract read): 26 broken objects of gypsum, soap, paraffin and potato; the probability of a fragment falls as a power of its mass; the exponent depends on the object's shape (balls, cubes, half cubes, plates, bars), not the material; scaling holds for fragments smaller than the object's smallest dimension. The individual exponents were seen only in a search summary (about 1.6 for balls and cubes, about 1.1 for bars and plates) and are not used as cited values.
- Ching, Yiu and Lo, "Energy dependence of mass distributions in fragmentation", arXiv:cond-mat/9809172, https://arxiv.org/abs/cond-mat/9809172: molecular-dynamics model; the cumulative fragment-mass distribution has a power-law region whose exponent increases with the energy given to the object, "confirmed by experiments". Their review section also notes glass rods dropped from low heights give lognormal masses and from large heights a power-law region, and that dry-clay plates show two power-law regions separated by a mass that grows with plate thickness (Meibom and Balslev).
- Mott and Linfoot (1943) predicted distributions by randomly cutting shapes with lines; Grady (2008) noted ductile materials fit an exponential form and brittle materials a power law ("A Review of Mechanisms and Models for Dynamic Strength, Dynamic Failure, and Fragmentation", University of Alberta repository, lines 543-552, https://ualberta.scholaris.ca/server/api/core/bitstreams/64de9741-8620-47fc-83d8-a0985a572bb9/content, search extract).

Game use: more energy means more, smaller pieces and a steeper power law; metal tears into fewer, exponentially distributed pieces; glass and stone shatter in power-law showers; cap the smallest piece at the voxel size and turn the rest into particles and dust.

## J21-3. Impact, crushing, tools and cutting

- **Kinetic energy** delivered is ½ m v². Examples computed by me: a 1,500 kg car at 50 km/h carries about 145 kJ and at 100 km/h about 579 kJ (four times, because energy goes as speed squared); a 10 t truck at 50 km/h about 965 kJ; a 10 g bullet at 400 m/s about 800 J.
- **Sledgehammer heads** weigh about 1-9 kg; modern heavy-duty heads 10-20 lb (4.5-9 kg) (Wikipedia "Sledgehammer", https://en.wikipedia.org/wiki/Sledgehammer). No open measured swing speed was found, so Table J21-3a gives energy for chosen head speeds.

### Table J21-3a. Hammer head energy per blow (computed by me; head speed is a game parameter, not a measurement)

| Head mass | 5 m/s | 10 m/s | 15 m/s | 20 m/s |
|---|---|---|---|---|
| 1 kg | 12 J | 50 J | 112 J | 200 J |
| 3 kg | 38 J | 150 J | 338 J | 600 J |
| 5 kg | 62 J | 250 J | 562 J | 1,000 J |
| 9 kg | 112 J | 450 J | 1,012 J | 1,800 J |

A simple chip model: new crack area per blow ≈ (energy absorbed) / G_c, with G_c from J15-4 (about 10 J/m² for glass and stone, thousands for metals). Only a fraction of the blow goes into cracks; the rest becomes heat, sound and motion. That fraction was not found in an open source.

- **Cutting and chopping wood** follows the grain: Wood Handbook p. 5-27 (J15-2) shows cracks run most easily along the grain (RL and TL systems), with mode I toughness 220-550 kPa·m^½ against 1,650-2,400 for shear. Splitting along the grain therefore needs a few times less energy than cutting across it (direction from J15; no per-cut energy found).
- **Drilling and pickaxe** energy per volume removed: not found in an open source.

## J21-4. Gradual damage, wear and fire weakening (read directly)

**Steel in fire.** EN 1993-1-2 (Eurocode 3, structural fire design) Table 3.1, as retyped by FireSolver, "Reduction factors for steel in fire", https://firesolver.net/en/guides/steel-reduction-factors-fire.html, and the Promat structural steel manual, https://media.promat.com/pi10301/original/288272370/promat-paph-02-steelwork-en.pdf (both secondary; the standard itself is paid). Linear interpolation is allowed between rows.

| Steel temperature | Yield strength factor k_y | Stiffness factor k_E |
|---|---|---|
| 20-100 °C | 1.00 | 1.00 |
| 200 °C | 1.00 | 0.90 |
| 300 °C | 1.00 | 0.80 |
| 400 °C | 1.00 | 0.70 |
| 500 °C | 0.78 | 0.60 |
| 600 °C | 0.47 | 0.31 |
| 700 °C | 0.23 | 0.13 |
| 800 °C | 0.11 | 0.09 |
| 900 °C | 0.06 | 0.0675 |
| 1,000 °C | 0.04 | 0.045 |
| 1,200 °C | 0 | 0 |

Promat notes structural steel begins to lose its design margin of safety around 550 °C.

**Wood in fire.** USDA *Wood Handbook* FPL-GTR-282 (2021), chapter 18, p. 18-15, https://research.fs.usda.gov/treesearch/62200 (public domain): under the standard ASTM E119 fire, wood ignites in about 2 minutes; char then grows at about 0.8 mm/min for the next 8 minutes, after which the char layer insulates and the rate drops to about 0.6 mm/min; the average over the first hour is about 0.6 mm/min (1.5 in/h), the nominal rate generally assumed for solid wood. Moisture content is a major factor; density and species matter. Strength depends on the remaining uncharred cross-section. Same chapter: lignin breaks down with exothermic peaks between 225 and 450 °C, and cellulose depolymerises strongly between 300 and 350 °C.

Computed by me: a 100 mm square post burning on all four faces at 0.6 mm/min loses 36 mm of width (18 mm per face) in 30 minutes, leaving a 64 mm core, about 41% of the original area; after 60 minutes (36 mm per face) a 28 mm core, about 8% of the area.

**Concrete wear.** Spalling (surface layers flaking off, for example from corroding rebar, "oxide jacking") is shown in images/6-destruction; no open rate data found.

## J21-5. How buildings collapse (read directly)

**Progressive (disproportionate) collapse**: damage out of proportion to the cause (Wikipedia "Progressive collapse", https://en.wikipedia.org/wiki/Progressive_collapse).

**Ronan Point, London, 16 May 1968** (Wikipedia "Ronan Point", https://en.wikipedia.org/wiki/Ronan_Point, secondary): a 22-storey tower block, two months old. A gas explosion in a corner flat on the 18th floor blew out the load-bearing flank wall that supported the four flats above. Those floors fell, and the falling slabs stacked onto the floors below, collapsing the whole south-east corner's living-room bays; 4 people died and 17 were injured. Game lesson: a panel building with no alternate load path loses a whole column of rooms from one wall.

**World Trade Center** (NIST, "FAQs - NIST WTC Towers Investigation", Q31, https://www.nist.gov/world-trade-center-investigation/study-faqs/wtc-towers-investigation, citing NCSTAR 1 section 6.14.4 and NCSTAR 1-5A):
- First exterior panels hit the ground about 11 s (WTC 1) and about 9 s (WTC 2) after collapse began, timed from video and seismic records.
- The structure below "offered minimal resistance"; the falling 12 (WTC 1) to 28 (WTC 2) storeys' momentum far exceeded what the floors below could absorb, and the falling mass grew as each floor failed.
- Large parts of the cores (about 60 storeys of WTC 1, 40 of WTC 2) stood for 15-25 s after collapse began.
- Cause chain: fire weakened core columns, sagging floors pulled perimeter columns inward, the columns buckled and the top tilted (NCSTAR 1, https://nvlpubs.nist.gov/nistpubs/Legacy/NCSTAR/ncstar1.pdf).

**WTC 7** (NIST, "FAQs - NIST WTC 7 Investigation", https://www.nist.gov/world-trade-center-investigation/study-faqs/wtc-7-investigation, and NCSTAR 1A):
- A single initiating event: fire damage to floors and connections failed one column (Column 79).
- 12.9 s passed between the floor failures around Column 79 and the roofline starting to drop (measured and predicted, NCSTAR 1A Table 3-1).
- The north-face descent had three stages: slower than gravity for 1.75 s; free fall for about 2.25 s over about 8 storeys (32 m); then slowing as it met resistance. The upper 18 storeys took about 5.4 s, about 40% longer than free fall (quotes via search extract).

### Table J21-5a. Free-fall time from height (computed by me, t = √(2h/g), no air or resistance)

| Height | Storeys at 3.5 m | Free-fall time |
|---|---|---|
| 10 m | 3 | 1.4 s |
| 32 m | 9 | 2.6 s |
| 50 m | 14 | 3.2 s |
| 100 m | 29 | 4.5 s |
| 200 m | 57 | 6.4 s |
| 417 m (WTC 1 roof, assumed) | 119 | 9.2 s |

Real collapses take longer than free fall because the structure below resists (WTC 7 upper storeys: about 40% longer). A game collapse should run from roughly free fall for a building with no resistance to 1.5 times free fall, as a tunable.

## J21-6. Demolition (read directly)

Wikipedia "Building implosion", https://en.wikipedia.org/wiki/Building_implosion (several statements flagged "citation needed" there):
- "Implosion" is a misnomer: charges remove chosen supports so the building fails under its own weight, guided in one direction or straight down (Stacey Loizeaux quote, via NOVA).
- Explosives shatter reinforced-concrete supports; linear shaped charges cut steel; charges fire progressively, lower floors starting the collapse.
- Preparation: under a day for a chimney, up to six months for a complex building (stripping walls, wrapping columns in fabric and fencing to hold back fly-rock).
- 1959 example: an 8-storey Washington DC apartment block, 200 lb of dynamite in half-pound charges at split-second intervals, down in 15 seconds.

Debris field: no universal "radius = k × height" rule was found. One search summary mentions demolition debris "commonly confined to about 1.5 times the footprint radius" and another a minimum debris length of "building length plus height" for low-rise RC frames, but neither was read in its primary, so they are in Not found.

## J21-7. How games and research model damage (method summaries; links also in links.md)

| Source | Method | Use for this engine |
|---|---|---|
| Dennis Gustafsson, *Teardown* (voxel engine). Interview, Software Engineering Daily ep. 1772, Dec 2024, transcript https://softwareengineeringdaily.com/wp-content/uploads/2024/12/SED1772-Teardown.txt; 80.lv interview https://80.lv/articles/teardown-developer-breaks-down-multiplayer-and-voxel-destruction-tech; Voxagon blog "The unlikely story of Teardown Multiplayer" (13 Mar 2026), https://blog.voxagon.se/2026/03/13/teardown-multiplayer.html | Objects are voxel volumes on a regular grid, so cutting is discrete and avoids mesh-cutting precision and convexity problems. After voxels are removed the engine checks connectivity; disconnected parts become new physics objects. No load or stress model: a house hanging on one voxel stays up (a deliberate gameplay choice; he says real structural integrity over hundreds of millions of voxels is hard). Early access needed an improved connected-parts search for very large objects. Multiplayer: destruction logic rewritten in fixed-point integer maths and sent as ordered deterministic commands ("cut hole in this shape at voxel x,y,z"); transforms use state sync at about 1 Mbit/s per client. | Baseline voxel cut plus connectivity islands. Add a support/stress pass on top (Red Faction, 7 Days to Die, Blast) to fix Teardown's known gap. |
| Gustafsson, "Cracking destruction" (Smash Hit), 13 May 2014, https://blog.voxagon.se/2014/05/13/cracking-destruction.html | When a hit's impulse passes a threshold, carve a small volume at the impact point, slice it into pieces, check whether the remainder falls apart. Cap contact impulses on breakable objects inside the solver, break when the cap is hit, then re-solve so the pieces keep the motion. | Break inside the physics step, not after it, so breaking absorbs momentum naturally. |
| Volition, *Red Faction: Guerrilla* (Geo-Mod 2.0). GDC 2009 "Blowing Up the Outside World: Destruction Done the Next Gen Way", Eric Arnold and Jeff Hanna, https://gdcvault.com/play/1672/Blowing-Up-the-Outside-World; Digital Foundry interview, 9 Sep 2009, https://www.digitalfoundry.net/articles/digitalfoundry-red-faction-guerrilla-big-bang-theory-article | Pre-broken structures with materials and weight. The stress code scans from top to bottom, adding up the force from the mass above and comparing it with each material's strength; if exceeded, the piece breaks, which can drop the whole building if it was the last connection. Creaks and groans play as stress rises. Havok handled rigid bodies, vehicles and ray casts; the destruction engine was custom on top. Developers say level artists had to learn to build like structural engineers, and memory cost is high. | The model Aron named: per-piece mass, per-connection strength, top-down load accumulation, audio cues from stress ratio. |
| *7 Days to Die* structural integrity, official wiki, https://7daystodie.wiki.gg/wiki/Structural_Integrity | Each block has mass, a yes/no vertical support, and a horizontal support limit (mass that one side face can carry). Support propagates up from bedrock; vertically supported blocks are always stable; cantilevered runs fail when the attached mass exceeds the face limit (example: wood limit 40, mass 5, so 8 blocks hold and the 9th drops all of them; concrete limit 120). Recomputed on every place or remove. | Cheap integer support rule for block worlds; good first version before a full stress solve. |
| NVIDIA Blast SDK, "Stress Solver (NvBlastExtStress)", https://docs.omniverse.nvidia.com/kit/docs/blast-sdk/latest/docs/api/extensions/ext_stress.html, and "Introduction", https://nvidia-omniverse.github.io/PhysX/blast/docs/api/introduction.html | Support graph: chunks are nodes, bonds join them, each bond has health; damage lowers health and bonds at or below zero break; connected islands become new actors. The stress solver iteratively spreads forces across bonds with separate tension, compression and shear limits; gravity needs at least one zero-mass (fixed) node; collision impulses can be added to the nearest node so things break at their weak point, not just where hit; supports dynamic actors (spin fast enough and it tears). Update loop: add forces, update, if overstressed bonds then generate fracture commands, then notify new and destroyed actors. | Closest open reference implementation of a stress/support graph. |
| Unreal Engine Chaos Destruction, "Destruction Overview", https://dev.epicgames.com/documentation/en-us/unreal-engine/destruction-overview; GDC 2019 "Causing Chaos", https://www.youtube.com/watch?v=6T8LzaIq3Qs | Geometry collections pre-fractured (Voronoi and other patterns) into a cluster hierarchy; each level has a damage threshold (strain); a connection graph joins neighbouring pieces and breaks when strain exceeds the limit. | Hierarchical clusters: break big chunks first, small ones later. |
| DICE, Frostbite 2 (Battlefield). Robert Kihl, "Destruction Masking in Frostbite 2 using Volume Distance Fields", SIGGRAPH 2010 Advances in Real-Time Rendering, https://advances.realtimerendering.com/s2010/Kihl-Destruction%20in%20Frostbite(SIGGRAPH%202010%20Advanced%20RealTime%20Rendering%20Course).pdf | Rendering side: artist-placed spheres and signed volume distance fields mask damage on surfaces, replacing the per-part UV masks of Bad Company 2 and 1943. | How to show damage on surfaces without new geometry. |
| O'Brien and Hodgins, "Graphical Modeling and Animation of Brittle Fracture", SIGGRAPH 1999, pp. 137-146, doi:10.1145/311535.311550, https://graphics.berkeley.edu/papers/Obrien-GMA-1999-08/Obrien-GMA-1999-08.pdf | Finite-element stress analysis decides where cracks start and which way they run. | Physically based reference; too slow for whole buildings in real time. |
| Parker and O'Brien, "Real-time deformation and fracture in a game environment", SCA 2009, doi:10.1145/1599470.1599492 | Corotational tetrahedral finite elements made robust and controllable for a shipped game (search summary). | Real-time FEM fracture for hero objects. |
| Müller, Chentanez and Kim, "Real Time Dynamic Fracture with Volumetric Approximate Convex Decompositions", SIGGRAPH 2013, doi:10.1145/2461912.2461934, https://matthias-research.github.io/pages/publications/fractureSG2013.pdf | Meshes are split into convex pieces; a fracture pattern (for example Voronoi cells) is placed at the impact point at run time, giving impact-aligned, partial fracture without pre-baking (search summary). | Pattern-at-impact fracture, artist controllable and fast. |

## Sources disagree

- Brick strength: about 7-103 MPa (US range) against 20-40 MPa for a common English house brick (same Wikipedia page, different scopes).
- Structural stability in games: Teardown has connectivity only; Red Faction and Blast accumulate load and compare it with strength; 7 Days to Die uses a per-face mass limit. These are design choices, not factual disagreements.
- Energy-based fragment sizes (Grady) against measurements: measured fragments are often much smaller (Drugan 2001; Computational Materials Science 2006 abstract).

## Not found

- Debris spread distance against building height from an open primary source (search summaries only: about 1.5 times the footprint radius for implosions; building length plus height for low-rise RC frames).
- Energy per cut for axes, saws, pickaxes and drills on wood, stone and concrete.
- Measured sledgehammer head speeds (a seismic-survey report surfaced but the figures could not be checked).
- Concrete strength against temperature (EN 1992-1-2 is paid; no open retyping read).
- Oddershede et al. exponents from the paper body (paywalled); the Red Faction GDC 2009 slides themselves (GDC Vault, members only).
- Open measured data on concrete, brick or stone wear rates.
