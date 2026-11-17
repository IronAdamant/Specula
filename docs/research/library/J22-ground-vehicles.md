# J22: Ground vehicles: mass, centre of gravity, inertia, grip, suspension, stopping, lean

Researched 2026-11-17 (all URLs accessed 2026-11-17). Brief section 1a. Values marked "computed by me" are arithmetic on the cited numbers. Wikipedia is used only as a pointer to its cited primary, named in each row. Car drag (Cd, CdA) is already in round-two J5 and J19; not repeated here.

## J22-1. Measured mass, footprint, centre-of-gravity height and inertia (read directly, selection)

Source: Heydinger, Bixel, Garrott, Pyne, Howe and Guenther, "Measured Vehicle Inertial Parameters: NHTSA's Data Through November 1998", SAE 1999-01-1336, doi:10.4271/1999-01-1336; copy read at https://eng.auburn.edu/~dmbevly/mech4420/vehicle_params.pdf (database listing, two table pages per vehicle: geometry, then CG and inertia). All 1998 model year, measured with S.E.A. Inc.'s Vehicle Inertia Measurement Facility, driver only, full fuel. Stated accuracy for the facility: CG height ± 0.5% (paper, IPMD version table).

| Vehicle (NHTSA no.) | Weight (N) | Mass (kg, computed by me) | Wheelbase (m) | Track F / R (m) | CG behind front axle (m) | CG height (m) | Pitch / Roll / Yaw inertia (kg m²) | Static stability factor (as listed) |
|---|---|---|---|---|---|---|---|---|
| Toyota Tercel 2-door (527) | 10,332 | 1,054 | 2.375 | 1.400 / 1.389 | 0.953 | 0.502 | 1,326 / 343 / 1,473 | 1.389 |
| Honda Civic 2-door (452) | 11,217 | 1,144 | 2.621 | 1.471 / 1.468 | 1.038 | 0.513 | 1,617 / 365 / 1,785 | 1.431 |
| Dodge Neon 4-door (496) | 12,199 | 1,244 | 2.642 | 1.458 / 1.455 | 0.954 | 0.507 | 1,748 / 441 / 1,945 | 1.435 |
| Chevrolet S10 pickup (457) | 14,144 | 1,442 | 2.750 | 1.379 / 1.387 | 1.169 | 0.604 | 2,321 / 504 / 2,477 | 1.145 |
| Dodge Caravan van (517) | 16,975 | 1,731 | 2.885 | 1.595 / 1.631 | 1.189 | 0.649 | 3,267 / 818 / 3,508 | 1.243 |
| Dodge Dakota pickup (521) | 17,832 | 1,818 | 3.326 | 1.524 / 1.560 | 1.353 | 0.618 | 4,061 / 699 / 4,271 | 1.247 |
| Ford Explorer 4x4 (485) | 19,792 | 2,018 | 2.827 | 1.481 / 1.486 | 1.295 | 0.697 | 3,561 / 740 / 3,682 | 1.063 |
| Ford Expedition 4x4 (451) | 25,871 | 2,638 | 3.025 | 1.664 / 1.664 | 1.459 | 0.777 | 5,398 / 1,210 / 5,639 | 1.071 |

Loading changes the numbers: the same Expedition loaded to gross vehicle weight rating (no. 538, 7 occupants plus 1,753 N ballast) moved its CG to 1.708 m behind the front axle and 0.838 m high, and its SSF fell to 0.993 (same table).

Class averages (sprung mass basis), from S. Sommer, Penn State ME 481 notes, "Inertial Properties of American Vehicles", https://www.me.psu.edu/sommer/me481/notes_07_04.pdf, citing the same SAE paper:

| Class | Sprung mass (kg) | Wheelbase (m) | Track (m) | Front axle to CG (m) | CG height (m) | Jxx roll / Jyy pitch / Jzz yaw (kg m²) |
|---|---|---|---|---|---|---|
| Passenger, small | 969.0 | 2.524 | 1.446 | 1.021 | 0.519 | 392.6 / 1,632.2 / 1,798.8 |
| Passenger, large | 1,403.0 | 2.679 | 1.468 | 1.277 | 0.585 | 632.3 / 2,749.7 / 2,893.3 |
| Pickup, small | 1,409.4 | 2.948 | 1.424 | 1.396 | 0.620 | 571.3 / 3,142.8 / 3,326.3 |
| Pickup, large | 1,885.8 | 3.425 | 1.619 | 1.581 | 0.684 | 940.5 / 5,344.0 / 5,642.3 |
| SUV, small | 1,718.5 | 2.683 | 1.496 | 1.350 | 0.688 | 803.3 / 3,367.0 / 3,522.2 |
| SUV, large | 2,251.1 | 3.032 | 1.579 | 1.628 | 0.767 | 1,157.3 / 5,960.8 / 6,111.0 |
| Van | 1,847.5 | 2.947 | 1.589 | 1.480 | 0.698 | 992.3 / 4,410.7 / 4,617.8 |

The notes give the rule of thumb Jyy ≈ Jzz ≈ 5 Jxx, with Jyy/Jxx 4.15 to 5.68 and Jzz/Jxx 4.38 to 6.00.

**Static stability factor** SSF = T / (2H), track over twice CG height. It is the steady sideways acceleration (in g) at which a rigid vehicle on rigid tyres would start to lift its inside wheels. Computed by me from the table: Neon 1.438, tip-over lean of the resultant 55.2°; Explorer 1.062, 46.7°. Real vehicles roll and their tyres slide first, so treat SSF as an upper bound.

## J22-2. Tyre grip: Magic Formula and surface friction

Pacejka Magic Formula, F = D sin(C arctan(B k − E(B k − arctan(B k)))), with F normalised by vertical load and k the slip ratio. Typical constant coefficients by surface, MathWorks Simscape documentation, "Tire-Road Interaction (Magic Formula)", section "Magic Formula Coefficients for Typical Road Conditions", https://www.mathworks.com/help/sdl/ref/tireroadinteractionmagicformula.html (cites Pacejka, *Tire and Vehicle Dynamics*, Elsevier 2005):

| Surface | B (stiffness) | C (shape) | D (peak, ≈ peak μ) | E (curvature) |
|---|---|---|---|---|
| Dry tarmac | 10 | 1.9 | 1 | 0.97 |
| Wet tarmac | 12 | 2.3 | 0.82 | 1 |
| Snow | 5 | 2 | 0.3 | 1 |
| Ice | 4 | 2 | 0.1 | 1 |

Other cited friction values:
- Accident-reconstruction baseline: kinetic μ = 0.7 with a 1.5 s perception-reaction time (Wikipedia "Braking distance", citing Taoka 1989, *ITE Journal* 59(3) 19-21, and the Virginia Code § 46.2-880 stopping tables).
- Wet or frozen asphalt: μ 0.25 or lower (same article, citing J. Y. Wong, *Theory of Ground Vehicles*, 2nd ed. 1993, p. 26).
- Rubber on dry asphalt: static μ often 0.5 to 0.8 (Wikipedia "Bicycle and motorcycle dynamics", braking section, citing its listed references).

## J22-3. Rolling resistance (Wikipedia table, retyped selection; the table credits HowStuffWorks "tire4" and per-row references)

https://en.wikipedia.org/wiki/Rolling_resistance, "Rolling resistance coefficient examples". Force = Crr × normal force.

| Case | Crr |
|---|---|
| Rail steel wheel on steel rail | 0.0010-0.0024 (passenger car about 0.0020) |
| Road bicycle tyres, 120 psi, 50 km/h, rollers | 0.0022-0.0050 |
| Lorry (semi) tyres | 0.0045-0.0080 |
| EU tyre label class A upper limit | 0.0065 |
| Car tyre measurements | 0.0062-0.0150 |
| Ordinary car tyres on concrete | 0.0100-0.0150 |
| Stage coach on dirt road; soft snow worst case | 0.0385-0.0730 |
| Ordinary car tyres on sand | 0.30 |

## J22-4. Suspension: ride frequency and damping (read directly)

OptimumG, "Tech Tip: Springs & Dampers, Part One", https://optimumg.com/wp-content/uploads/2020/01/SpringsDampers_Tech_Tip_1.pdf:
- Ride frequency (undamped natural frequency of the body in ride): 0.5-1.5 Hz passenger cars; 1.5-2.0 Hz saloon racecars and moderate-downforce formula cars; 3.0-5.0+ Hz high-downforce racecars.
- Front-to-rear frequency split commonly 10-20% (to limit pitching over bumps).

OptimumG, Part Two, https://optimumg.com/wp-content/uploads/2020/01/SpringsDampers_Tech_Tip_2.pdf: spring rate Ks = 4π² f² m_sm MR², worked example 200 kg sprung corner mass, 2 Hz, motion ratio 1.25 gives 49.3 N/mm.

OptimumG, Part Three, https://optimumg.com/wp-content/uploads/2020/01/SpringsDampers_Tech_Tip_3.pdf: ride damping ratio about 0.25 in passenger vehicles for comfort; 0.65-0.70 as a racecar baseline.

C. Haney / OptimumG column, "Of springs and dampers", *Racecar Engineering* 2020-11, https://optimumg.com/wp-content/uploads/2021/10/racecar-2020_11.pdf: passenger cars 1.1 Hz (very soft) to 1.8 Hz (very stiff); non-aero racecars 1.5-3.0 Hz; aero racecars 3.0-7.0 Hz.

## J22-5. Stopping distances (UK Highway Code, Rule 126)

The Highway Code, Rule 126 and its "Typical stopping distances" diagram, https://www.gov.uk/guidance/the-highway-code/general-rules-techniques-and-advice-for-all-drivers-and-riders-103-to-158 and https://assets.publishing.service.gov.uk/media/65f97642aa9b760011fbda4e/the-highway-code-stopping-distances.pdf (the PDF is an image; totals cross-checked against Wikipedia "Braking distance", which quotes Rule 126). Crown copyright, Open Government Licence.

| Speed | Thinking (m) | Braking (m) | Total (m) | Implied reaction time (s, computed by me) | Implied deceleration (computed by me) |
|---|---|---|---|---|---|
| 20 mph | 6 | 6 | 12 | 0.67 | 6.7 m/s² (0.68 g) |
| 30 mph | 9 | 14 | 23 | 0.67 | 6.4 m/s² (0.66 g) |
| 40 mph | 12 | 24 | 36 | 0.67 | 6.7 m/s² (0.68 g) |
| 50 mph | 15 | 38 | 53 | 0.67 | 6.6 m/s² (0.67 g) |
| 60 mph | 18 | 55 | 73 | 0.67 | 6.5 m/s² (0.67 g) |
| 70 mph | 21 | 75 | 96 | 0.67 | 6.5 m/s² (0.67 g) |

The thinking and braking split was checked against the AA's retyped chart (https://www.theaa.com/breakdown-cover/advice/stopping-distances), which also states the Code assumes a thinking time of just under 0.7 s on a dry road. Rule 126 also says leave at least a two-second gap, doubled on wet roads and up to ten times on icy roads.

Bicycle braking (Wikipedia "Bicycle and motorcycle dynamics", citing Wilson, *Bicycling Science*, 3rd ed. 2004): rear brake alone gives about 0.25 g at best on an upright bike; front braking is limited by pitch-over, with combined CG about 60 cm behind and 120 cm above the front contact patch.

## J22-6. Two-wheelers: lean, trail, self-stability, wobble

Lean in a steady turn: tan θ = v² / (g r) (Wikipedia "Bicycle and motorcycle dynamics", citing Fajans, "Steering in bicycles and motorcycles", *Am. J. Phys.* 68 (2000) 654). The article's example: 10 m/s on a 10 m radius needs 45.6°. Computed by me: 13.9 m/s (50 km/h) on 50 m, 21.5°; 27.8 m/s (100 km/h) on 100 m, 38.2°; 5 m/s on 5 m, 27.0°. Wide motorcycle tyres need a little more lean than this (same article, citing Foale, *Motorcycle Handling and Chassis Design*, 2006).

Trail and head angle (same article, citing Zinn and others): touring bicycles 72-73° head angle with 43-60 mm trail; racing 73-74° with 28-45 mm; track 75° with 23.5-37 mm.

Self-stability of the benchmark bicycle (same article, citing Meijaard, Papadopoulos, Ruina and Schwab 2007, *Proc. R. Soc. A* 463:1955, and Kooijman et al. 2011, *Science* 332:339, doi:10.1126/science.1201959): oscillations begin about 1 m/s; weave speed about 5.3 m/s; capsize speed about 8 m/s; self-stable range 5.3-8.0 m/s for that design.

Modes (same article, citing Cossalter, *Motorcycle Dynamics*): weave 0-4 Hz, whole bike; wobble (shimmy, tank-slapper) 4-10 Hz, front end; rear wobble up to about 6.5 Hz. A motorcycle model with frame flex shows wobble going unstable at 43.7 m/s (figure caption).

## J22-7. Heavy vehicles and tracks (Wikipedia infoboxes, each citing its own sources)

| Vehicle | Mass | Length / width / height | Other | Source |
|---|---|---|---|---|
| VW Golf Mk8 hatchback | from 1,255 kg | 4,284 / 1,789 / 1,456 mm, wheelbase 2,636 mm | | https://en.wikipedia.org/wiki/Volkswagen_Golf_Mk8 infobox |
| Porsche 911 (992) | 1,380-1,710 kg | 4,519-4,535 / 1,852-1,900 / 1,289-1,303 mm, wheelbase 2,450 mm | | https://en.wikipedia.org/wiki/Porsche_911_(992) infobox |
| Tesla Model 3 | from 3,552 lb (1,611 kg, computed by me) | width 72.8 in, wheelbase 113.2 in | single-speed 9:1 reduction | https://en.wikipedia.org/wiki/Tesla_Model_3 infobox |
| New Flyer Xcelsior bus, 40 ft | 26,000 lb (11,790 kg, computed by me) | 41 ft over bumpers, 102 in wide, 10-11 ft high, wheelbase 283.75 in | 60 ft articulated: 39,000 lb | https://en.wikipedia.org/wiki/New_Flyer_Xcelsior infobox |
| M1A1 Abrams | 63 short tons (57.2 t, computed by me), citing Hunnicutt 2015 p. 306 | hull 26.02 ft, 12 ft wide, 8 ft high | 1,500 hp gas turbine; torsion bars with rotary dampers; road 45 mph governed, off-road 40 km/h; ground clearance 0.48 m | https://en.wikipedia.org/wiki/M1_Abrams infobox |
| Leopard 2A6 | 62.3 t | 10.97 m gun forward, 3.75 m wide, 3 m high | 17.8 kW/t; torsion bars; 70 km/h | https://en.wikipedia.org/wiki/Leopard_2 infobox |

Ground pressure (Wikipedia "Ground pressure" table; rows not individually cited, so treat as indicative): human on snowshoes 3.5 kPa; average human in flat shoes 17 kPa standing; M1 Abrams 103 kPa; adult horse 170 kPa; passenger car 205 kPa; adult elephant 240 kPa; road racing bicycle 620 kPa; lorry 793 kPa; stiletto heel 3,250 kPa. The article notes a walking human exerts more than double the standing pressure and that a pneumatic tyre's ground pressure roughly equals its inflation pressure.

## Sources disagree

- Passenger-car ride frequency: 0.5-1.5 Hz (OptimumG Tech Tip 1) against 1.1-1.8 Hz (OptimumG *Racecar Engineering* column). Aero racecars 3.0-5.0+ Hz against 3.0-7.0 Hz.
- Perception-reaction time: Highway Code thinking distances imply 0.67 s (computed by me), the reconstruction baseline uses 1.5 s, and the Wikipedia article reports 1 s as a population average.
- NHTSA SSF as listed (e.g. Neon 1.435) differs slightly from T/(2H) with the front track (1.438, computed by me); the paper's value presumably uses the mean track.

## Not found

- Open, measured engine torque and power curves for named production engines (only peak figures appear in infoboxes and maker brochures, which are not open data).
- Gear ratio sets for named cars from a primary maker source.
- Measured tank and tracked-vehicle suspension frequencies and track tension; snowmobile and ski ground pressure from a primary test.
- Measured Magic Formula coefficient sets for a named tyre (TNO/MF-Tyre files are proprietary).
- EU lorry size and weight limits (Directive 96/53/EC): the EUR-Lex consolidated text did not return readable text to the fetcher this session.
- Motorcycle CG height and inertia from a measured database (the NHTSA listing covers light four-wheel vehicles only).
