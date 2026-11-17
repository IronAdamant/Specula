# Reference, round three: the gaps, listed ahead of their parts

Status: **a request, written 2026-11-17**, at the owner's word the same night ("Might as well list me
the gaps now. So you'll have materials. Vehicle physics, air plane physics, space physics would be good
to have as well as bullet physics, and sword physics").

The first two rounds are in: 269 pictures in
[specula-imagines](https://github.com/IronAdamant/specula-imagines) and 57 files of numbers in
[library/](library/README.md). This is everything known to be missing, for the parts still to come, so
that it can be gathered before it is needed. Nothing here blocks the work in hand.

## The rules, as before

- **Numbers** come as library files in the house form: every value with its source, its page or table
  and a link; what the people, animals or machines measured were; "Not found" and "Sources disagree" at
  the end; "computed by me" for arithmetic on cited numbers. They go into this repository's library.
- **Pictures** come with an index row each (file, what it shows, view, subject, source page, author,
  licence with its link, date checked) and go into the pictures' repository.
- **Licences:** public domain and CC0 first, then CC BY and CC BY-SA with the author named. Anything
  else is a link and not a file. A licence is read from its source, never guessed.
- **People in pictures:** unclothed adults are kept apart. No unclothed figure of a child, drawn or
  photographed, and none whose subject cannot be shown to be an adult: leave those out, links included.
- **A fact is not owned**, a table's layout may be: numbers are cited and retyped, never a publisher's
  table copied whole from a source that does not allow it.

## 1. Physics the owner asked for

A game's designer changes these; the engine should start from what is real. For each: the governing
relations in a line or two with their source, then tables of real values.

### 1a. Ground vehicles

| Wanted | |
|---|---|
| Mass, wheelbase, track, height of the centre of mass, weight split front to back, for a small car, a family car, a sports car, a pickup, a lorry, a bus, a motorcycle, a bicycle, a tank or other tracked vehicle | a table by kind |
| Engines: power and torque against engine speed for a few petrol, diesel and electric motors; gear ratios and final drives | curves or tables |
| Tyres: grip against slip (longitudinal and sideways) on dry road, wet road, gravel, snow and ice; rolling resistance; how load changes grip | the curve's shape and its peak values |
| Suspension: spring rates, damping ratios, travel, anti-roll stiffness, by kind of vehicle | typical ranges (round two found none) |
| Brakes: deceleration reached, and stopping distances from set speeds | a table |
| Aerodynamics: drag coefficient, frontal area, lift or downforce, by kind | a table |
| What measured vehicles do: 0 to 100 km/h, top speed, lateral grip in g, braking from 100 km/h | a table to check a model against |
| Tracked and off-road: ground pressure, sinkage and traction on sand, mud and snow | tables |
| Two wheels: lean angle against speed and radius, trail and rake, what makes a bicycle stay up | relations and typical values |

### 1b. Aircraft

| Wanted | |
|---|---|
| Lift and drag coefficients against angle of attack, with the stall, for a few common aerofoils (a symmetric one, a cambered one, a thin one) | curves or tables; NASA and NACA reports are public domain |
| The same for a whole aircraft: lift curve, drag polar, the best glide ratio | for a trainer, an airliner, a fighter, a glider |
| Mass, wing area, span, aspect ratio, thrust or power, stall speed, cruise and top speed, ceiling, climb rate, turn rate, for about ten well-documented aircraft | a table to check a model against |
| Control surfaces: how much an elevator, an aileron and a rudder turn the aircraft; stability derivatives for one or two documented aircraft | tables |
| Propellers and jets: thrust against speed and altitude; a propeller's efficiency against advance ratio | curves |
| Helicopters and multirotors: disc loading, hover power, how thrust falls near the ground and with height | relations and values |
| The air: density, pressure, temperature and the speed of sound against altitude (round two has density) | the standard atmosphere's table |
| Wind and gusts: typical speeds, how wind grows with height above ground | tables |
| What a bird's and an insect's wing does differently (for creatures that fly) | lift and wingbeat figures by size |

### 1c. Space

| Wanted | |
|---|---|
| The planets, the larger moons and a few asteroids: mass, radius, surface gravity, rotation period, orbit's size, shape and tilt, atmosphere | NASA's fact sheets, as a table |
| Orbits: the relations for period, speed, escape speed and transfers (Hohmann, plane change), with worked values for Earth, the Moon and Mars | relations and a table |
| Rocket engines: thrust, specific impulse at sea level and in vacuum, mass, propellants, for about ten flown engines; ion engines too | a table |
| Launchers and craft: stage masses full and empty, delta-v by stage, for a few flown rockets and landers | a table |
| Re-entry: heating and deceleration against speed and angle, ballistic coefficient of capsules | relations and flown values |
| Attitude: reaction wheels, thrusters and their typical torques; moments of inertia of documented craft | a table |
| What a person can stand: g by direction and duration; radiation; decompression times | NASA's published limits |
| Low gravity on the body: how walking, jumping and falling change (the Apollo film studies, parabolic flights) | published figures |
| Tides, Lagrange points, sphere of influence | relations and values for the Earth and Moon |

### 1d. Bullets and other shot things

For game physics only: what leaves a barrel and how it flies, as published in manufacturers' and
standards bodies' tables.

| Wanted | |
|---|---|
| For about twenty common cartridges (pistol, rifle, shotgun) and a few historical ones (musket, arquebus): bullet mass, muzzle speed, muzzle energy, ballistic coefficient and the drag model it is given against (G1, G7) | a table |
| The standard drag curves themselves (G1, G7: drag coefficient against Mach number) | tables |
| Drop and drift: worked trajectories at 100, 300, 500 and 1,000 m for three cartridges, to check a model against; wind's effect; spin drift | tables |
| Recoil: momentum and energy against the weapon's mass | relation and values |
| Arrows, bolts, sling stones, thrown spears, javelins, cannon balls, catapult shot: mass, speed, range | a table |
| What stops what: penetration in ballistic gelatin, wood, steel plate and soil, by published test | tables, with the test's conditions |
| Shot and fragments: pattern spread against distance for shotguns; fragment speeds from published tests | tables |
| Sound: muzzle and supersonic crack levels, how sound falls with distance | figures |

### 1e. Swords and other hand weapons

| Wanted | |
|---|---|
| Measured museum pieces, about thirty across kinds (arming sword, longsword, rapier, sabre, katana, gladius, dagger, axe, mace, war hammer, spear, staff, halberd): length, mass, point of balance, blade width and thickness along its length | a table; museum catalogues and published measurements |
| How a blade is balanced: centre of mass, centres of percussion, pivot points, moment of inertia about the grip, for a few measured swords | published figures |
| How fast they move: tip and hand speeds, and swing durations, from motion studies of cuts and thrusts | tables |
| What a blow does: impact energy and force by weapon; what it takes to cut or pierce cloth, leather, mail and plate, by published test | tables, with the test's conditions |
| Shields and armour: mass by piece, coverage, thickness, hardness of period steel | tables |
| The body in a fight: reach, lunge distance, step lengths and times, recovery times, reaction times | figures; historical fencing treatises for the stances (many are public domain, with plates) |
| Plates of guards, cuts and thrusts from the treatises, as pictures | for the pictures' repository |

## 2. Still missing from the first two rounds

| Wanted | |
|---|---|
| Running: joint angles as curves through the cycle, not peaks (the open file exists at doi 10.6084/m9.figshare.4543435 and could not be fetched in round two) | the file's tables |
| Arm swing in walking and running: shoulder and elbow angles through the cycle, from a source that may be used (the library's two are weak) | curves |
| Matched front, side and back sets of slender, muscular and older adult bodies; a whole figure at three-quarter | pictures |
| A large open sheet of a walk cycle's key poses (contact, down, passing, up), and a run's | pictures |
| Bounce for everyday pairs (wood, steel, stone, rubber, glass on a hard floor) | a table |
| Children's and old people's proportions as **numbers** (segment lengths and girths by age), since the engine's body covers 17 to 58 years only | published growth and ageing surveys; no pictures of unclothed children |

## 3. For the parts still to come

| Part | Wanted |
|---|---|
| Faces and hair | Skulls and heads of women and of several ages and ancestries, as numbers and plates; how each facial muscle moves the surface; hairlines, partings, hair's length and thickness figures; teeth, eyes and ears with dimensions |
| Hands and feet in use | Grips by kind (power, pinch, key) with joint angles; a foot's roll through a step, with pressures |
| Clothing | How cloth hangs and folds by fabric (weight, stiffness); garment ease; period and modern silhouettes as flat patterns where open |
| Animals | For each body plan of part 5: limb segment lengths as shares of body length, joint ranges, mass by size, gait speeds; wing shapes and wingbeat figures; fish body shapes and fin places; fur and feather tracts as diagrams |
| Fantasy creatures | Reconstructions with skeletons: pterosaurs and large dinosaurs (for dragons and beasts), with mass estimates and the published arguments about how they moved |
| Machines and robots | Joints by kind with their ranges and speeds; industrial arms' and legged robots' published dimensions and masses; hydraulic and electric actuator figures; three-view drawings of open-licensed or public-domain machines |
| Trees and plants | By species: height, trunk taper, branching angles and counts, leaf shape and size, bark as photographs lit evenly; winter skeletons; growth with age |
| Terrain and rock | Slopes at which materials rest; river profiles; cliff and scree forms; rock joints and strata as photographs; soil and sand colours as measured values |
| Water | Wave height and period against wind; breaking forms; river surface speeds; foam and spray; water's colour against depth and what is in it, as measured values |
| Sky and weather | Cloud types with heights and sizes; rain and snow fall speeds and sizes; fog densities; sun and moon brightness and colour by height above the horizon |
| Light and materials | Measured reflectance of common materials (the open databases and their licences); skin, hair and eye colours as measured values; how wet changes a surface |
| Buildings | Storey heights, door and window sizes, stair rules, roof pitches, by period and region; wall thicknesses by material; open measured drawings |
| Sound | Footstep, impact and weather sounds under open licences, each with its licence; how loud things are at set distances |
| Fire, smoke and explosions | Flame heights and temperatures by fuel; smoke rise; blast pressure against distance and charge (published curves); how things break (fracture energies of wood, glass, stone, steel) |

## 4. Where each kind goes

Pictures: the pictures' repository, in a folder by subject, with its index. Numbers: this
repository's library, a file a topic (the letters so far: A body, B face, C animals, D machines, E
plants, F land, G water, H motion, I prior art and materials, J physics). Recorded motion, scans and
sounds: a repository of their own, with their licences beside them, and a list of links here.
