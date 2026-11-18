# J31: Electric propulsion (ion and Hall thrusters) and re-entry heating

Fixed after the second check (2026-11-18): NEXT Table 3 is on printed p. 8 (PDF p. 12), not p. 7, and Table 6 on printed p. 10 (PDF p. 14), not p. 9; the Soyuz heat-shield quote is not uncited (Wikipedia cites it to NTRS 20130013840); 0.034-0.044 is the ratio of doubly-to-singly-charged ion current, not a "double-ion fraction", and "not from a thrust stand" is marked as an inference; the NTRS licence is now read from the records (PUBLIC, GOV_PUBLIC_USE_PERMITTED), not assumed. Spreads added under Sources disagree: the Stardust drogue altitude, the MSL design heat flux, the Apollo heat-shield mass and the radiative heating exponent.

Researched 2026-11-18 (all URLs accessed 2026-11-18). Round-four gap 4. Extends round-three J24 (chemical engines, g loads, Shuttle tile temperatures). Values marked "computed by me" are arithmetic on the cited numbers.

## J31-1. NASA NEXT gridded ion engine: thrust, Isp and power across its throttle range

Source: Soulas G.C., Domonkos M.T., Patterson M.J. (2003), "Performance Evaluation of the NEXT Ion Engine", NASA/TM-2003-212551 (AIAA-2003-5278), NASA Glenn Research Center, Table 3 (engine LM2, printed p. 8, PDF p. 12), Table 6 (printed p. 10, PDF p. 14). https://ntrs.nasa.gov/citations/20040000790 (US Government work, NTRS "public" distribution).

What was measured: 40 cm xenon ion engine in vacuum facility VF6; thrust is calculated from beam current and voltage with a beam-divergence correction of about 0.98 and a ratio of doubly-to-singly-charged ion current of 0.034-0.044 (Table 3 note a). The source says only that thrust is "calculated"; "not from a thrust stand" is an inference (second check, 2026-11-18: a reasonable one).

| Beam current (A) | Beam voltage (V) | Input power (kW) | Thrust (mN) | Specific impulse (s) | Thrust efficiency |
|---|---|---|---|---|---|
| 1.203 | 671 | 1.12 | 49.9 | 2,210 | 0.485 |
| 1.203 | 1,790 | 2.43 | 81.4 | 3,610 | 0.592 |
| 2.004 | 1,010 | 2.43 | 102 | 2,970 | 0.609 |
| 2.005 | 1,790 | 3.98 | 135 | 3,950 | 0.657 |
| 2.710 | 1,790 | 5.32 | 183 | 4,050 | 0.684 |
| 3.106 | 1,790 | 6.06 | 210 | 4,090 | 0.693 |
| 3.524 | 1,790 | 6.86 | 238 | 4,120 | 0.700 (not a throttle-table point) |

Selection of rows from the 25 in Table 3. Requirement and results (Table 6): Isp at least 4,050 s at full power (three engines gave 4,090, 4,070, 4,060 s); thrust efficiency above 0.63 at full power (0.68-0.69) and above 0.42 at low power (0.49-0.50). Text: power range 1.1-6.9 kW; thrust about 50 mN at low power to 237-238 mN at full power; propellant throughput requirement 270 kg (405 kg for qualification).

Thrust per kilowatt (computed by me): about 34 to 45 mN/kW across the table.

## J31-2. NSTAR (Deep Space 1) and Hall thrusters

- NSTAR: "The 2.3-kW ion engine, which provided primary propulsion and two-axis attitude control, thrusted for more than 16,000 hr and consumed more than 70 kg of xenon" on Deep Space 1. NASA Glenn, "Deep Space 1 Ion Engine Completed a 3-Year Journey" (2005), https://ntrs.nasa.gov/citations/20050201632 (US Government work). The NSTAR throttle table (thrust and Isp by power level) was **not found** in an opened source.
- Hall thrusters (**secondary**, Wikipedia "Hall-effect thruster", text CC BY-SA 4.0, https://en.wikipedia.org/wiki/Hall-effect_thruster): exhaust speeds 10-80 km/s (1,000-8,000 s), most 15-30 km/s; "Devices operating at 1.35 kW produce about 83 mN"; SPT-50 20 mN, SPT-60 30 mN, SPT-70 40 mN, SPT-100 83 mN (citing Morozov 2003, *Plasma Physics Reports*, doi:10.1134/1.1561119, and others); BPT-4000 at 4.5 kW the highest-power Hall thruster flown (citing Aerojet Rocketdyne 2015). Gridded ion engines in general 3,000-10,000 s (Wikipedia "Gridded ion thruster", same licence).

## J31-3. Stagnation-point convective heating (Sutton-Graves)

Source: Sutton K., Graves R.A. Jr. (1971), "A General Stagnation-Point Convective-Heating Equation for Arbitrary Gas Mixtures", NASA TR R-376, Langley Research Center, November 1971. https://ntrs.nasa.gov/citations/19720003329 (US Government work). Page image of printed p. 16 checked by me (the equations are images in the PDF).

- Form: results are presented as q̇w·√(R/ps) against (hs − hw), with a single heat-transfer coefficient K per gas; so q̇w = K·√(ps/R)·(hs − hw), where q̇w is wall heat flux, ps stagnation pressure, R nose radius, hs total enthalpy, hw wall enthalpy (printed p. 16, eqs. 41-42).
- Units of K: kg/(s·m^3/2·atm^1/2) (figure captions), so with ps in atm, R in m and h in J/kg the flux is in W/m² (unit check by me).
- K from Table II (correlation of the computer results; Table II is on printed p. 38, second check 2026-11-18), with average and maximum correlation error:

| Gas (mass fractions) | K | Average error | Max error |
|---|---|---|---|
| Air (0.2320 O2, 0.7680 N2) | 0.1113 | 3.3% | 9.8% |
| N2 | 0.1112 | 3.6% | 10.0% |
| CO2 | 0.1210 | 3.4% | 8.4% |
| 0.1345 CO2, 0.8655 N2 | 0.1113 | 3.0% | 9.4% |
| 0.3500 H2, 0.6500 He | 0.0547 | 3.3% | 13.8% |

- Range of the study (abstract): enthalpies 2.3 to 116.2 MJ/kg, pressures 0.001 to 100 atm, wall temperatures 300 and 1,111 K.
- Allen-Eggers (blunt body) principle, **secondary**: "maximum aeroshell bluntness (maximum drag) yields minimum TPS mass", and convective heating rises roughly with velocity cubed, radiative far faster, so radiation dominates at high speed with large nose radius (Wikipedia "Atmospheric entry", CC BY-SA 4.0). Allen and Eggers' 1958 NACA report itself was not opened.

## J31-4. Flown and designed entry conditions

| Vehicle | Value | Source |
|---|---|---|
| Stardust sample return capsule (2006) | entry 46,440 km/h (12.9 km/s, computed by me) in the fact table; "about 12.8 kilometers per second" at 125 km entry interface in the timeline | NASA/JPL, *Stardust Sample Return* press kit, January 2006, PDF pp. 8 and 14, https://www.jpl.nasa.gov/news/press_kits/stardust-return.pdf |
| Stardust | peak heating at entry + 52 s, 61 km altitude, heat-shield exterior about 2,700 °C; peak deceleration 38 g ten seconds later; drogue at about Mach 1.4, 32 km | same press kit, PDF p. 14 |
| Stardust capsule | diameter 0.8 m (fact sheet) or 81 cm (description); mass about 43 kg | same press kit, PDF pp. 8, 28 and 16 |
| Apollo 10 command module (1969) | entered at 11.11 km/s, the record before Stardust | same press kit, PDF p. 14 |
| Stardust heat shield (PICA, about 0.25 g/cc) | mission needed an ablator good for about 1,000 W/cm² | Venkatapathy E. (NASA Ames), "Ablators - From Apollo to Future Missions to Moon, Mars and Beyond", https://ntrs.nasa.gov/citations/20190032222, PDF p. 11 |
| Mars Pathfinder (1997) | peak stagnation heat flux estimated 100 W/cm²; SLA-561V tested to 188 W/cm² | same, PDF p. 11 |
| Mars Science Laboratory (2012) | tiled PICA designed for about 250 W/cm²; flight conditions about 100 W/cm² | same, PDF pp. 12-13 |
| Pioneer Venus probes (1978) | entry 11.54 km/s, flight path angle −25.4° to −65.7° | same, PDF pp. 8-9 |
| Galileo probe (Jupiter, 1995) | 47.4 km/s relative (60 km/s inertial); predicted peak stagnation heat flux about 40 kW/cm² at 7 atm; heat load 200 kJ/cm² | same, PDF pp. 9-10 (47.4 km/s on p. 9) |
| Apollo | lunar return above 11 km/s | same, abstract |
| Apollo CM heat shield | 2 in (5.1 cm) thick on the aft face, 0.5 in on the crew compartment and forward | Wikipedia "Apollo command and service module", CC BY-SA 4.0 (secondary) |
| Soyuz descent module | "21mm to 28mm thick ablator (glass-phenolic composite)" plus 8 mm silica insulation | Wikipedia "Soyuz (spacecraft)", CC BY-SA 4.0 (secondary), which cites it to NASA, *ISS Soyuz Vehicle Descent Module Evaluation of Thermal Protection System (TPS) Penetration Characteristics*, https://ntrs.nasa.gov/citations/20130013840 (corrected by the second check, 2026-11-18; that report not opened, so still secondary) |
| Orion, Crew Dragon | Avcoat and PICA-3 heat shields respectively; no heat flux numbers found | Wikipedia articles (secondary) |

Note: the Venkatapathy paper is a NASA Ames author's conference paper on NTRS. Its licence is not stated on the PDF, but the NTRS record lists it as distribution PUBLIC, copyright GOV_PUBLIC_USE_PERMITTED, and the same holds for the other three NTRS items in this note (second check, 2026-11-18), so the licence is not just assumed from NASA authorship.

## Sources disagree

- Stardust entry speed: the same press kit gives 46,440 km/h (12.9 km/s) and "about 12.8 km/s".
- Stardust capsule diameter: 0.8 m and 81 cm in the same press kit.
- Stardust drogue altitude: 32 km (PDF p. 14) against "approximately 30 kilometers (100,000 feet)" (PDF p. 29) in the same press kit, both at about Mach 1.4 (second check, 2026-11-18).
- MSL peak heat flux grew during design, from about 140 W/cm² to over 250 W/cm² at PDR (Venkatapathy, PDF p. 12; second check, 2026-11-18).
- Apollo CM heat-shield mass: about 3,000 lb in the text against 1,869 lb in the spec list, on the same Wikipedia page, "Apollo command and service module" (second check, 2026-11-18).
- Radiative against convective heating: Wikipedia "Atmospheric entry" gives radiative heating as roughly v⁸ against convective v³; section J31-3 says only "far faster" (second check, 2026-11-18).

## Not found

- NSTAR throttle table (thrust, Isp, power by level) from a NASA document.
- Hall thruster performance from a primary NASA source (only Wikipedia secondary used).
- Capsule ballistic coefficients (mass ÷ Cd·A) from a primary source; computing them needs a sourced Cd, not found this round.
- Peak heat flux and g for Apollo, Orion, Crew Dragon and Soyuz from primary sources.
- Allen and Eggers (1958) NACA Report 1381 text.

## Sources

All accessed 2026-11-18 (as the header says).

- Soulas, Domonkos, Patterson (2003), NASA/TM-2003-212551, https://ntrs.nasa.gov/citations/20040000790
- NASA Glenn (2005), DS1 ion engine, https://ntrs.nasa.gov/citations/20050201632
- Sutton and Graves (1971), NASA TR R-376, https://ntrs.nasa.gov/citations/19720003329
- NASA/JPL (2006), Stardust Sample Return press kit, https://www.jpl.nasa.gov/news/press_kits/stardust-return.pdf
- Venkatapathy, "Ablators - From Apollo to Future Missions...", https://ntrs.nasa.gov/citations/20190032222
- Wikipedia (text CC BY-SA 4.0): "Hall-effect thruster" (https://en.wikipedia.org/wiki/Hall-effect_thruster), "Gridded ion thruster", "Atmospheric entry", "Apollo command and service module", "Soyuz (spacecraft)" (https://en.wikipedia.org/wiki/Soyuz_(spacecraft)), "Orion (spacecraft)", "SpaceX Dragon 2"
- Cited by Wikipedia for the Soyuz quote, not opened: NASA, ISS Soyuz Vehicle Descent Module Evaluation of Thermal Protection System (TPS) Penetration Characteristics, https://ntrs.nasa.gov/citations/20130013840
