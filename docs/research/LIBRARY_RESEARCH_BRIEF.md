# Research brief: the data behind the libraries

Status: **a request, written 2026-11-16. Nothing here has been researched yet.** The owner offered a deep
research pass by another assistant (Grok) for M6's libraries ([ROADMAP.md](../ROADMAP.md), M6, parts 4 and
5): people, faces and hair, animals, fantasy creatures and machines, trees and grass, terrain, water, and
the motion that goes with them. This is the list of what to bring back.

## Why data first

The owner, 2026-11-16: "Do your best as you can in realistic ... I need this done right, not rushed", and
"Renderer can come later, ONCE you have data for this." An agent describes shapes in numbers and cannot see
a curve ([FORM_AND_LIGHT.md](FORM_AND_LIGHT.md)), so what makes a body, a gait or a tree believable has to
be in the numbers it starts from. Those numbers exist in published measurements. They are looked up, not
written from an agent's memory.

## How to bring it back

1. **Every number with its source**: the paper, table or survey, its year, and the page or table number.
   A number with no source is not used.
2. **Units, and what was measured**: which population, species, age and sex; mean and spread where the
   source gives them, not the mean alone.
3. **The licence of each source**, said plainly: public domain or a government work; an open licence
   (which); or a published paper whose figures may be cited as facts. Say where a dataset may not be
   redistributed or may not be used commercially. **Nothing is downloaded into the engine**: it is
   zero-dependency, and the libraries are written here from the numbers.
4. **Tables as plain text** (CSV or Markdown), one file a topic.
5. **What could not be found, and what two sources disagree on.** That is as useful as the rest.
6. **No pictures or text copied from books in copyright.** Measurements and methods are facts; a book's
   drawings and wording are not ours to take.

Everything that comes back is checked against its source before it is used, as every outside report on
this project is. **The "likely sources" below were named from memory, as places to start**: confirm that
each exists and says what is claimed for it, and prefer a better source where there is one.

## The items

### A. The human body

| # | What | In what form | Likely sources |
|---|---|---|---|
| A1 | Segment lengths against stature: head, neck, trunk, upper arm, forearm, hand, thigh, shank, foot; shoulder and hip widths | a table by sex, with spread | Drillis and Contini (1966); Winter, *Biomechanics and Motor Control*; the US Army's ANSUR II survey (2012) |
| A2 | How proportions change with age, infant to elderly: head height against stature, leg length against stature, sitting height | a table by age and sex | WHO and CDC growth references; ANSUR; paediatric anthropometry |
| A3 | Girths and depths against stature and build: chest, waist, hip, thigh, calf, upper arm, neck; how they move with body mass and with sex | means, spreads and the main correlations | ANSUR II; NHANES |
| A4 | Segment masses and centres of mass (for balance checks and ragdolls) | fractions of body mass and of segment length | Dempster (1955); de Leva (1996) |
| A5 | Joint ranges of motion, every major joint, both directions | degrees, by joint and axis | AAOS norms; Norkin and White; Soucie et al. (2011) |
| A6 | Hands and feet: finger bone lengths against hand length, foot length and width against stature | tables | hand anthropometry surveys; ANSUR |
| A7 | The skeleton as it shows: rib cage, pelvis (and how it differs by sex), skull landmarks, which bones lie under the skin | measurements and ratios | osteology references; forensic anthropology standards |

### B. Faces, heads and hair

| # | What | In what form | Likely sources |
|---|---|---|---|
| B1 | Face and head measurements: head length, breadth and height; face thirds and fifths; eye spacing, eye width, nose width and length, mouth width, ear size and place; jaw and chin | means and spreads by sex, and by population where the source has it | Farkas, *Anthropometry of the Head and Face*; the NIOSH head and face survey (2003); ANSUR head measures |
| B2 | What differs between male and female faces, and how a face changes with age | the measures that move, and by how much | the same; craniofacial growth studies |
| B3 | Facial expression as movement: the action units of the Facial Action Coding System, the muscle behind each, which landmarks each moves and roughly how far; which units make the basic expressions; how long an expression takes to appear, hold and fade | a table of units; a table of expressions | Ekman and Friesen's published descriptions; expression timing studies |
| B4 | Mouth shapes for speech (visemes): the set, and which sounds map to each | a table | lip-sync literature; published viseme sets |
| B5 | Eyes that look alive: blink rate and length, how eyes lead the head in a gaze shift, pupil range, where eyelids sit | numbers with spreads | eye movement literature |
| B6 | Hair: hairline shapes, growth directions and whorls, how hairstyles are classified, beard and brow patterns; how real-time hair is drawn (cards, shells, strands) and what each costs | a taxonomy; a survey of methods with their published costs | dermatology and forensic sources; SIGGRAPH and GDC talks |
| B7 | Skin, eye, lip and hair colour as measured ranges | colour values with their source | the Monk Skin Tone scale; skin reflectance studies; iris and hair pigment studies |
| B8 | What character creators expose: the list of controls in shipped games of the Skyrim kind, and how open parametric bodies and faces are parameterised (and each one's licence) | a feature survey; a licence table | game documentation; MakeHuman, Anny, MHR, FLAME, ICT-FaceKit |

### C. Animals

| # | What | In what form | Likely sources |
|---|---|---|---|
| C1 | Limb and body proportions for a working set of mammals: dog and wolf, cat and big cat, horse, deer, cattle, bear, pig, rabbit, rat, elephant. Bone lengths against trunk length or shoulder height; neck and tail length; skull size; how the foot meets the ground (flat, toes, hoof) | a table a species | veterinary anatomy; zoological osteometry; scaling studies (Alexander; Christiansen) |
| C2 | Gaits: which foot lands when, and for how long, in walk, amble, trot, pace, canter, the two gallops and the bound; the speeds at which animals change gait; stride length against speed; how the spine bends in a gallop | footfall tables with duty factor and phase; the transition numbers | Hildebrand's gait diagrams; Alexander and Jayes (1983); Muybridge's plates (public domain) |
| C3 | Birds: wing bone proportions, wingspan and wing shape by way of flying, wingbeat rate against mass, the phases of a wingbeat, how a wing folds, feather counts and rows; walking and hopping | tables; a description of the stroke | Pennycuick; ornithology references |
| C4 | Bats and pterosaurs, for wings on four-limbed bodies (a dragon's): membrane attachment, finger proportions, how a quadruped launches; wing loading against mass, so a large flyer looks able to fly | tables; the scaling rule | bat and pterosaur flight literature |
| C5 | Fish and snakes: how the body wave travels (wavelength, how amplitude grows along the body) for the main swimming styles; fin placement; the four ways a snake moves | numbers a style | fish swimming kinematics; snake locomotion studies |
| C6 | Reptiles and amphibians: sprawling gaits (lizard, crocodile), their proportions, the turtle | tables | herpetology and locomotion studies |
| C7 | Insects and spiders: leg segments and their proportions, the tripod gait and the spider's, wingbeat | tables | entomology references |
| C8 | Coats: how stripes, spots and rosettes form and which numbers give which pattern; scale and feather layout | pattern models with parameters | reaction-diffusion literature (Turing; Murray) |
| C9 | How animals behave in groups and around others: flocking and herding rules with their published numbers, flight distances, daily rhythm | rules with parameters | Reynolds (1987); ethology references |

### D. Machines

| # | What | In what form | Likely sources |
|---|---|---|---|
| D1 | How built machines are jointed: joint kinds, the joint layouts of published humanoid and four-legged robots, how a piston's length follows a joint's angle, track and wheel geometry | tables and the geometry | robotics texts; manufacturers' published specifications |
| D2 | What makes a machine read as heavy, light, old or precise: panel lines, fasteners, wear patterns, proportion habits of industrial design | a list of conventions with examples named | industrial design and concept art practice (practitioners' guides: say so) |

### E. Trees, grass and plants

| # | What | In what form | Likely sources |
|---|---|---|---|
| E1 | Tree form as parameters, by species: branching angles, how branch length and thickness fall with each level, leaf arrangement angle, crown shape, height against trunk thickness; a working set of species across climates | a parameter table a species | Weber and Penn (1995), with its species tables; Prusinkiewicz and Lindenmayer, *The Algorithmic Beauty of Plants*; Runions et al. (2007); forestry allometry |
| E2 | Leaves, bark and roots: leaf shapes, edges, vein patterns and sizes by species; bark types; root flare | a taxonomy with measurements | botany references |
| E3 | Grass and ground cover: blade length, width and curve for common grasses; clump density; flowers, ferns, reeds, moss; how real-time grass is drawn and what it costs | tables; a survey of methods | botany; the Ghost of Tsushima grass talk (GDC 2021); Jahrmann and Wimmer (2017) |
| E4 | Where plants grow: which plants at which slope, height, wetness and light; how dense by biome; how a forest thins as it ages; the layers of a forest | rules with numbers | ecology references; the Whittaker biome diagram; forestry stand tables |

### F. Terrain

| # | What | In what form | Likely sources |
|---|---|---|---|
| F1 | The numbers that make land look right: angles of repose by material; how a river's slope falls along its length; meander wavelength against channel width; how streams join (Horton, Hack); valley shapes cut by water and by ice; dune forms and their slip angle; volcano slopes by type; cliffs and strata; coasts | a table a landform | geomorphology texts |
| F2 | Erosion as algorithms with their parameters: thermal, hydraulic, stream-power; what each costs and what it looks like | a survey with parameters | Musgrave et al. (1989); Mei et al. (2007); Cordonnier et al. (2016); Galin et al., *A Review of Digital Terrain Modeling* (2019) |
| F3 | Rock, soil, sand, snow and ice as they look: forms by rock type; measured colour and roughness ranges for natural ground; snow line by latitude | tables | geology references; published measured-albedo charts |

### G. Water

| # | What | In what form | Likely sources |
|---|---|---|---|
| G1 | Waves on open water: the wave spectra in use and their parameters; wave height against wind speed; how wavelength, speed and depth relate; when a wave breaks; foam | formulas and tables | Pierson and Moskowitz; JONSWAP; Tessendorf (2001); the Beaufort scale |
| G2 | Rivers, lakes and falls: flow speed against slope and depth; where rapids, eddies and standing waves form; how a fall arcs; shore behaviour | formulas and typical values | hydraulics references (Manning); game water talks |
| G3 | How water looks: colour by depth and by water type; clarity; reflection against angle; how real-time water is drawn and what it costs | measured values; a survey of methods | ocean optics (Jerlov water types); Sea of Thieves (SIGGRAPH 2018) and similar talks |

### H. Motion

| # | What | In what form | Likely sources |
|---|---|---|---|
| H1 | Human walking and running: the timing of a stride; hip, knee, ankle and pelvis angles through it; stride length and cadence against speed; when a walk becomes a run; arm swing; how these vary with age, sex and load | curves as tables; numbers with spreads | Winter; Perry, *Gait Analysis*; published gait databases |
| H2 | Other human movement: turning, stairs, jumping, reaching, sitting and standing, carrying, falling | timings and joint ranges | biomechanics literature |
| H3 | What makes a still figure look alive: breathing rate and depth, weight shifts, sway, small head and hand movements | numbers with spreads | posture and motor control literature |
| H4 | Posture and emotion: which changes in posture and movement read as which feeling | findings with their strength | affective movement studies |
| H5 | Motion capture that may be used as reference: which datasets exist, and for each whether it may be used commercially and whether results derived from it may be shipped | a licence table | the datasets' own licence pages |
| H6 | Walking, running and flying made by rule instead of by capture: the published methods for two, four and many legs, and what each needs | a survey with what each method takes as input | SIMBICON (2007); Coros et al. (2010, 2011); Wampler and Popovic (2009); Hecker et al., the Spore animation paper (2008); the Overgrowth talk (GDC 2014) |

### I. Prior art, to learn from and not to copy

| # | What | In what form | Likely sources |
|---|---|---|---|
| I1 | How shipped games generate creatures, plants and worlds from rules, and what their makers said went wrong | a survey, each with its source | Spore; No Man's Sky; Horizon Zero Dawn's placement talk (GDC 2017); Far Cry 5's terrain talks |
| I2 | How one garment or armour set is fitted to many bodies | a survey of methods | game character pipeline talks |
| I3 | Measured values for everyday materials (metals, wood, stone, cloth, leather): colour and roughness ranges | tables with sources | published physically based material charts |

## What is wanted first

The body and the face (A1 to A7, B1 to B5), the mammals and their gaits (C1, C2), human motion (H1, H3,
H6), and trees and grass (E1, E3): these decide the shape of the tools. The rest can follow in any order.
