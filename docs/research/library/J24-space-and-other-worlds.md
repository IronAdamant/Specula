# J24: Space: other worlds, orbits, rocket engines, launch and re-entry loads, suits and rovers

Fixed after the second check (2026-11-18):
- J24-4: NASA TN D-337 has no "Fig. 10" for the human g-tolerance values; its tolerance data are in figures 6 through 9, as plots. The "20 g under 10 s, 10 g for 1 minute, 6 g for 10 minutes" wording is Wikipedia's summary and is now marked Wikipedia-only, with the report's own context (select test pilots) added.
- J24-3: the plume-colour note has no source; moved to Unconfirmed.
- Spreads added: Raptor 1 and Raptor 3, the Merlin 1D figures' date tag, the F-1 thrust/weight variants, the shuttle EMU weight, Stapp's 25 g for 1.1 s and Bräck's citation; the LRV's third top speed under Sources disagree.
- Licences stated; a Sources list added at the bottom.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Brief section 1c. Values marked "computed by me" are arithmetic. NASA fact sheets are US Government works. Values taken from Wikipedia are secondary and under CC BY-SA 4.0 (Wikipedia's text licence).

## J24-1. Other worlds (NASA NSSDCA fact sheets, read directly)

Sources: D. R. Williams, NASA Goddard NSSDCA, Moon Fact Sheet https://nssdc.gsfc.nasa.gov/planetary/factsheet/moonfact.html; Mars Fact Sheet https://nssdc.gsfc.nasa.gov/planetary/factsheet/marsfact.html; Venus Fact Sheet https://nssdc.gsfc.nasa.gov/planetary/factsheet/venusfact.html (Earth column from the same comparison tables).

| Quantity | Moon | Mars | Venus | Earth |
|---|---|---|---|---|
| Surface gravity, mean (m/s²) | 1.62 | 3.73 | 8.87 | 9.82 (9.80 on the Moon sheet) |
| Escape velocity (km/s) | 2.38 | 5.03 | 10.36 | 11.19 |
| Volumetric mean radius (km) | 1,737.4 | 3,389.5 | 6,051.8 | 6,371.0 |
| Length of day (hours) | sidereal rotation 655.72 | 24.6597 | 2,802.0 | 24.0000 |
| Bond albedo | 0.11 | 0.250 | 0.77 | 0.294 |
| Solar irradiance (W/m²) | 1,361.0 | 586.2 | 2,601.3 | 1,361.0 |
| Surface pressure | 3 × 10⁻¹⁵ bar (night) | 6.36 mbar at mean radius (4.0 to 8.7 by season) | 92 bar | 1.013 bar |
| Surface air density (kg/m³) | none | about 0.016 ± 0.006 | about 65 | 1.225 (J23) |
| Temperature | 95 K to 390 K at the equator, day to night | average about 214 K; 184 K to 242 K at Viking 1 | average 737 K, almost no day-night range | |
| Surface wind | none | 2-7 m/s summer, 5-10 m/s fall, 17-30 m/s dust storm (Viking landers) | 0.3 to 1.0 m/s | |
| Atmosphere | trace He, Ne, H₂, Ar | CO₂ 95.1%, N₂ 2.59%, Ar 1.94% | CO₂ 96.5%, N₂ 3.5% | |
| Scale height (km) | | 11.0 | 15.9 | |

Moon orbit (same sheet): semimajor axis 384,400 km; sidereal period 27.3217 days; synodic 29.53 days; distance ranges about 357,000 to 407,000 km.

Computed by me from the sheets' GM and radius: a person who leaves the ground at 3 m/s rises 0.46 m with 0.61 s airtime on Earth, 1.21 m with 1.61 s on Mars, and 2.78 m with 3.70 s on the Moon. Circular orbit speed at the surface: Moon 1,679 m/s, Mars 3,555 m/s.

## J24-2. Earth orbits (computed by me)

v = √(GM/r), period = 2πr/v, GM = 3.986004418 × 10¹⁴ m³/s² (IERS/WGS 84 value), mean radius 6,371 km.

| Altitude | Speed (m/s) | Period (min) |
|---|---|---|
| 200 km | 7,788 | 88.4 |
| 400 km (ISS region) | 7,673 | 92.4 |
| 35,786 km (geostationary) | 3,075 | 1,435.7 (the true sidereal day is 1,436.1 min; the gap is from using mean rather than equatorial radius; the second check confirms that the equatorial radius gives 1,436.07 min) |

## J24-3. Rocket engines (Wikipedia infoboxes, each citing maker or NASA documents)

| Engine | Propellants (mixture ratio O/F) | Thrust sea level / vacuum | Isp sea level / vacuum (s) | Chamber pressure | Dry mass | Thrust-to-weight | Source page |
|---|---|---|---|---|---|---|---|
| Rocketdyne F-1 (Saturn V first stage) | LOX / RP-1, 2.27 | 1,522,000 / 1,746,000 lbf | 263 / 304 | 70 bar | 18,500 lb | 94.1 | https://en.wikipedia.org/wiki/Rocketdyne_F-1 |
| RS-25 (Space Shuttle main engine) | LOX / LH₂, 6.03 | 418,000 / 512,300 lbf | 366 / 452.3 | 2,994 psi | 7,004 lb | 73.1 | https://en.wikipedia.org/wiki/RS-25 |
| Merlin 1D (Falcon 9) | LOX / RP-1 | 845 / 981 kN | 282 / 311 | 1,410 psi | 1,030 lb | 184 | https://en.wikipedia.org/wiki/SpaceX_Merlin |
| Raptor 2 | LOX / CH₄, 3.6 | 230 tf sea level / 258 tf vacuum | not given in the infobox | 3,669.5 psi (booster engine, 2019) | 1,630 kg | 141.1 | https://en.wikipedia.org/wiki/SpaceX_Raptor |
| RL10 (upper stage) | LOX / LH₂, 5.88 | 110.1 kN vacuum | 465.5 vacuum | | 301 kg | | https://en.wikipedia.org/wiki/RL10 |

More from the same infoboxes (second check):
- Merlin 1D: the Isp values (282/311 s) carry an "update after 2020" tag in the infobox ("figures are from 2013").
- Raptor: the infobox also gives Raptor 3, 250 tf sea level / 275 tf vacuum, 1,525 kg, T/W 163.9; and Raptor 1, 185 tf, 2,080 kg, T/W 88.94. The infobox's T/W figures are sea-level thrust over engine-only mass.
- F-1: the infobox gives T/W 94.1; vacuum thrust over dry mass gives 94.4, and sea-level thrust over dry mass 82.3 (second checker's arithmetic).

The plume-colour note that stood here is under Unconfirmed.

## J24-4. Launch and re-entry loads

Saturn V, Apollo 11 ascent (Wikipedia "Saturn V", citing Schulman 2025 pp. 53-56 and the 1968 Saturn V Flight Manual p. 43): max q about T+66 s; crew reached 4 g at T+2 min 15 s; centre engine cut to bring it to about 3 g; outboard engines cut about T+2 min 40 s at 35.7 nmi altitude and 9,068.6 ft/s. Stack heights: Saturn V 363 ft, 6.22 to 6.54 million lb; Falcon 9 Full Thrust 69.8 m, 549,000 kg (infoboxes).

Human tolerance to sustained acceleration (Wikipedia "G-force", https://en.wikipedia.org/wiki/G-force, Wikipedia-only wording): untrained subjects tolerated about 20 g for under 10 s, 10 g for 1 minute and 6 g for 10 minutes, eyeballs in or out. The article cites NASA TN D-337 (Creer, Smedal and Wingrove 1960, https://ntrs.nasa.gov/api/citations/19980223621/downloads/19980223621.pdf), but the report has no "Fig. 10" for these values: it says its tolerance data "are presented in figures 6 through 9" (figure 6 eyeballs-out, figure 8 eyeballs-in, both drawn as plots), and the second checker could not read these numbers off the scanned plots. Treat them as Wikipedia's summary. Context from the report: its new boundaries come from preconditioned test pilots, with a "roughly sixfold increase" over Clarke's nonpilot data at 7 g eyeballs-out (about 0.6 min against 4 min and up), and the report says these boundaries apply "only to a fairly select group".

Survived peaks (same article's table, each with its own citation): John Stapp rocket sled 46.2 g (with "more than 25 g for 1.1 s"); Kenny Bräck IndyCar crash 214 g (2003; in the article's table and its prose, which cites news.com.au).

Space Shuttle thermal protection (Wikipedia "Space Shuttle thermal protection system"): re-entry heat up to 1,650 °C; reinforced carbon-carbon where temperature exceeded 1,260 °C (nose cap, wing leading edges); black HRSI tiles below 1,260 °C, 20,548 tiles on the underside; flexible blankets below 649 °C; aluminium structure limit 175 °C.

## J24-5. Suits and rovers

Extravehicular Mobility Unit (Wikipedia, citing Thomas and McMann, *US Spacesuits*, Praxis 2006): operating pressure 4.3 psi (29.6 kPa, computed by me); suit 109 to 122 lb; total ISS EVA suit 319 lb; the article also gives a total shuttle EVA suit weight of 254-275 lb.

Apollo Lunar Roving Vehicle (Wikipedia): 210 kg empty, 230 kg payload design (NASA "Apollo vehicles" citation); designed for 6 mph (intro; the record section says about 8 mph, see Sources disagree), reached 11.2 mph on Apollo 17.

## Sources disagree

- Earth's surface gravity: 9.80 m/s² on the Moon sheet's comparison against 9.82 on the Mars and Venus sheets; the sheets define "surface gravity" with and without rotation in different places. Standard gravity is 9.80665.
- LRV mass: 462 lb (210 kg) and payload 970 lb (440 kg) in one part of the article against 230 kg payload in another (both in Wikipedia "Lunar Roving Vehicle"); the higher payload counts two astronauts.
- LRV top speed: the intro of Wikipedia "Lunar Roving Vehicle" says it was designed for 6 mph, while its record section says "designed with a top speed of about 8 mph"; 11.2 mph was reached on Apollo 17.
- F-1 thrust-to-weight: the infobox gives 94.1, against 94.4 (vacuum thrust over dry mass) and 82.3 (sea-level thrust over dry mass) from the infobox's own figures.

## Not found

- NASA-STD-3001 Volume 2 acceleration limit tables (the standard was not opened this session; Wikipedia's summary of TN D-337 stands in).
- Raptor specific impulse from a SpaceX primary source.
- Measured plume colour or temperature by propellant from a primary paper.
- Titan, Europa and asteroid surface values (fact sheets exist at nssdc.gsfc.nasa.gov but were not read this session).
- Spacecraft attitude-thruster (RCS) thrust and pulse figures.
- The g-tolerance numbers read from TN D-337's own figures 6 to 9 (scanned plots; not read off by the second checker).

## Unconfirmed (second check, 2026-11-18)

Not to be used until a source that gives the value has been opened.

- Plume look by propellant (J24-3): RP-1 plumes are bright yellow-orange and sooty; hydrogen plumes are nearly transparent with visible shock diamonds; methane plumes are blue-violet with less soot. Checker's reason: no source, as the file itself said (visual note, no numeric source).

## Sources

All accessed 2026-11-17.

- Williams, NASA Goddard NSSDCA, Moon Fact Sheet, https://nssdc.gsfc.nasa.gov/planetary/factsheet/moonfact.html
- Williams, NASA Goddard NSSDCA, Mars Fact Sheet, https://nssdc.gsfc.nasa.gov/planetary/factsheet/marsfact.html
- Williams, NASA Goddard NSSDCA, Venus Fact Sheet, https://nssdc.gsfc.nasa.gov/planetary/factsheet/venusfact.html
- IERS/WGS 84 value of Earth's GM (3.986004418 × 10¹⁴ m³/s²)
- Wikipedia, "Rocketdyne F-1" (CC BY-SA 4.0), https://en.wikipedia.org/wiki/Rocketdyne_F-1
- Wikipedia, "RS-25" (CC BY-SA 4.0), https://en.wikipedia.org/wiki/RS-25
- Wikipedia, "SpaceX Merlin" (CC BY-SA 4.0), https://en.wikipedia.org/wiki/SpaceX_Merlin
- Wikipedia, "SpaceX Raptor" (CC BY-SA 4.0), https://en.wikipedia.org/wiki/SpaceX_Raptor
- Wikipedia, "RL10" (CC BY-SA 4.0), https://en.wikipedia.org/wiki/RL10
- Wikipedia, "Saturn V" (CC BY-SA 4.0), https://en.wikipedia.org/wiki/Saturn_V (citing Schulman 2025 pp. 53-56 and the 1968 Saturn V Flight Manual p. 43)
- Wikipedia, "Falcon 9 Full Thrust" infobox (CC BY-SA 4.0)
- Wikipedia, "G-force" (CC BY-SA 4.0), https://en.wikipedia.org/wiki/G-force (citing news.com.au for Bräck)
- Creer, Smedal and Wingrove, NASA TN D-337 (1960), https://ntrs.nasa.gov/api/citations/19980223621/downloads/19980223621.pdf
- Wikipedia, "Space Shuttle thermal protection system" (CC BY-SA 4.0), https://en.wikipedia.org/wiki/Space_Shuttle_thermal_protection_system
- Wikipedia, "Extravehicular Mobility Unit" (CC BY-SA 4.0), https://en.wikipedia.org/wiki/Extravehicular_Mobility_Unit (citing Thomas and McMann, *US Spacesuits*, Praxis 2006)
- Wikipedia, "Lunar Roving Vehicle" (CC BY-SA 4.0), https://en.wikipedia.org/wiki/Lunar_Roving_Vehicle (citing NASA "Apollo vehicles")
