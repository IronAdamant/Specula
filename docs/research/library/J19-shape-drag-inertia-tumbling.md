# J19: Shape: drag by shape, moment of inertia, terminal speed, tumbling, buoyancy

Researched 2026-11-17 (all URLs accessed 2026-11-17). Values marked "computed by me" are arithmetic. Extends round-two J4 (drag and air, with OpenStax and NASA drag values) and J8 (projectiles); those values are not repeated.

## J19-1. Drag coefficient by shape (read directly, secondary)

Wikipedia "Drag coefficient", table "General", https://en.wikipedia.org/wiki/Drag_coefficient, citing aerodyn.org "Shapes" (archived 2007) and, for the 3D plate, NASA. Reference area is the frontal area unless noted. Cd changes with Reynolds number, so these are typical values only.

| Shape | Cd |
|---|---|
| Flat plate parallel to flow, laminar (Re < 10⁶) | 0.001 |
| Flat plate parallel to flow, turbulent (Re > 10⁶) | 0.005 |
| Smooth sphere, Re = 10⁶ (past the drag crisis) | 0.1 |
| Rough sphere, Re = 10⁶ | 0.47 |
| Bullet (not ogive), subsonic | 0.295 |
| Triangular trapeze, 45° | 0.81 |
| Trapeze with triangular base, 45° | 0.9-1.7 |
| Skier | 1.0-1.1 |
| Wires and cables | 1.0-1.3 |
| Adult human, upright | 1.0-1.3 |
| Ski jumper | 1.1-1.3 |
| Flat plate square to flow (3D) | 1.28 |
| Empire State Building | 1.3-1.5 |
| Eiffel Tower | 1.8-2.0 |
| Long flat plate square to flow (2D) | 1.98-2.05 |

Same page: Cd = 1 would mean all the oncoming air is brought to rest over the whole front face; at very low Reynolds number drag is proportional to speed, not speed squared (Stokes regime).

## J19-2. Moment of inertia of simple shapes (read directly)

Wikipedia "List of moments of inertia", https://en.wikipedia.org/wiki/List_of_moments_of_inertia (secondary; these are standard results, also in OpenStax *University Physics Volume 1*, section 10.5, Figure 10.20, https://openstax.org/books/university-physics-volume-1/pages/10-5-calculating-moments-of-inertia, CC BY 4.0, OpenStax, Ling, Sanny, Moebs et al.).

| Shape (mass m) | Axis | I |
|---|---|---|
| Thin rod, length L | through centre, perpendicular | m L² / 12 |
| Thin rod, length L | through one end, perpendicular | m L² / 3 |
| Thin hoop, radius r | through centre, perpendicular to the hoop | m r² |
| Solid disk or cylinder, radius r | symmetry axis | m r² / 2 |
| Solid cylinder, radius r, height h | through centre, across | m (3r² + h²) / 12 |
| Hollow thin sphere, radius r | any diameter | 2 m r² / 3 |
| Solid sphere, radius r | any diameter | 2 m r² / 5 |
| Solid box, sides w, h, d | through centre, parallel to d | m (w² + h²) / 12 |

Parallel-axis theorem: I about any axis = I about the parallel axis through the centre of mass + m·(distance)² (OpenStax 10.5).

## J19-3. Tumbling and stability (read directly)

- **Intermediate axis (tennis racket) theorem**: for a rigid body with three different principal moments of inertia, spin about the largest and smallest axes is stable; spin about the middle axis is unstable and flips (Wikipedia "Tennis racket theorem", https://en.wikipedia.org/wiki/Tennis_racket_theorem, secondary). Game use: a thrown book, phone, or plank spun about its middle axis should flip periodically.
- **Falling disks**: Field, Klaus, Moore and Nori, "Chaotic dynamics of falling disks", *Nature* 388, 252-254 (1997), doi:10.1038/40817; author copy at https://nori-physics.org/images/pub/nori/pdf/Nature_388_252_Falling_Disks.pdf and University of Michigan Deep Blue, https://hdl.handle.net/2027.42/62793 (an author preprint text was read). They dropped steel and lead disks, d = 5.1-18.0 mm, t = 0.076-1.63 mm, in water and water-glycerol, plus some paper disks in air. Disks falling in a fluid show four regimes, set by two numbers: dimensionless moment of inertia I* = I_disk / (ρ_f d⁵) = π ρ_disk t / (64 ρ_f d) (t = thickness, d = diameter, ρ_f = fluid density) and Reynolds number R = U d / ν. The regimes are steady falling (low R; any starting tilt settles to flat), periodic oscillating (higher R, low I*), chaotic, and continuous tumbling (higher I*). The boundaries between regimes are well defined but are given only as a plot (their Fig. 2), so no numeric boundary is quoted here. Thickness-to-diameter ratio is expected to play no role for thin disks.

### Table J19-3a. I* for everyday flat things falling in air (computed by me, ρ_air = 1.225 kg/m³)

| Object | ρ (kg/m³) | t | d | I* |
|---|---|---|---|---|
| Paper disk | 800 | 0.1 mm | 10 cm | 0.032 |
| Playing card (as disk) | 900 | 0.3 mm | 8 cm | 0.14 |
| Coin (as disk) in air | 7,000 | 2 mm | 2.5 cm | 22 |
| Coin in water (ρ_f = 1,000) | 7,000 | 2 mm | 2.5 cm | 0.028 |

Reading of the paper (not a number from it): a higher I* moves a disk toward tumbling, a lower one toward fluttering or a steady fall, so a coin dropped in water flutters while a card in air tends to tumble at high enough R.

## J19-4. Terminal speed (formula read directly, values computed by me)

Terminal speed v_t = √(2 m g / (ρ A Cd)) (Wikipedia "Terminal velocity", https://en.wikipedia.org/wiki/Terminal_velocity). Reference point: a skydiver belly-down falls at about 55 m/s, reaching 50% of that in about 3 s and 90% in about 8 s (same page, citing Huang 1998, *The Physics Factbook*, https://hypertextbook.com/facts/1998/JianHuang.shtml).

Values computed by me with ρ_air = 1.225 kg/m³ (ISA sea level, Wikipedia "Density of air"), g = 9.80665 m/s², and Cd from J19-1:

| Object | Mass | Cd used | Terminal speed | Reynolds number (ν = 1.5 × 10⁻⁵ m²/s) |
|---|---|---|---|---|
| Stone sphere, 10 cm, 2,700 kg/m³ | 1.41 kg | 0.47 | 78 m/s | about 5 × 10⁵ |
| Steel ball, 5 cm, 7,850 kg/m³ | 0.51 kg | 0.47 | 94 m/s | about 3 × 10⁵ |
| Wooden ball, 10 cm, 500 kg/m³ | 0.26 kg | 0.47 | 34 m/s | about 2 × 10⁵ |
| Hailstone, 2 cm, 917 kg/m³ | 3.8 g | 0.47 | 20 m/s | about 3 × 10⁴ |
| Card plate 20 × 20 cm, 1 mm, 700 kg/m³, falling flat | 28 g | 1.28 | 3.0 m/s | |

Caveat: 0.47 is the rough-sphere value at Re = 10⁶; a smooth sphere past the drag crisis can drop toward 0.1 (J19-1), which would roughly double the terminal speed. The card would not stay flat (J19-3).

## J19-5. Buoyancy (read directly)

Archimedes' principle: the upward force equals the weight of the fluid that would occupy the submerged volume (Wikipedia "Buoyancy", https://en.wikipedia.org/wiki/Buoyancy). A floating object sinks until it displaces its own weight, so the submerged fraction = ρ_object / ρ_fluid (computed by me from the principle). Using round-two J3 densities: ice in fresh water about 0.92 submerged; pine (about 500 kg/m³) about 0.5; a human (close to water's density) floats only with air in the lungs.

## Sources disagree

- Sphere Cd: 0.47 (rough, Re = 10⁶) against 0.1 (smooth, Re = 10⁶) in the same table; round-two J4 has other sphere values. The drag crisis makes this a regime question, not a real disagreement.

## Not found

- Drag coefficient of a cube (face-on and edge-on) from an open, citable source. Not used anywhere here.
- Numeric regime boundaries for falling disks (only in Field et al. Fig. 2).
- Drag of irregular rocks and debris.
