# J23: Aircraft and the air: standard atmosphere, wing data, specs, glide, hover, wind

Researched 2026-11-17 (all URLs accessed 2026-11-17). Brief section 1b. Values marked "computed by me" are arithmetic. Bird flight numbers (wingbeat, wing loading) are in the main library's C3; drag of simple shapes is in J19.

## J23-1. International Standard Atmosphere (computed by me from the defining formula)

Definition: U.S. Standard Atmosphere 1976 (NOAA-S/T 76-1562, NASA-TM-X-74335), https://ntrs.nasa.gov/citations/19770009539; same model as ISO 2533 for the troposphere and lower stratosphere. Sea level 15 °C, 101,325 Pa, 1.225 kg/m³; lapse rate 6.5 K/km up to 11 km (geopotential), then constant 216.65 K to 20 km. Formulas used: T = 288.15 − 0.0065 h; p = 101,325 (T/288.15)^5.2559; above 11 km p = 22,632 exp(−g(h − 11,000)/(R T)); ρ = p/(R T) with R = 287.053 J/(kg K); speed of sound a = √(1.4 R T).

| Geopotential altitude (m) | T (°C) | p (Pa) | ρ (kg/m³) | ρ/ρ₀ | a (m/s) |
|---|---|---|---|---|---|
| 0 | 15.0 | 101,325 | 1.225 | 1.000 | 340.3 |
| 1,000 | 8.5 | 89,875 | 1.112 | 0.907 | 336.4 |
| 2,000 | 2.0 | 79,495 | 1.007 | 0.822 | 332.5 |
| 3,000 | −4.5 | 70,109 | 0.909 | 0.742 | 328.6 |
| 5,000 | −17.5 | 54,020 | 0.736 | 0.601 | 320.5 |
| 8,000 | −37.0 | 35,600 | 0.525 | 0.429 | 308.1 |
| 10,000 | −50.0 | 26,436 | 0.413 | 0.337 | 299.5 |
| 11,000 | −56.5 | 22,632 | 0.364 | 0.297 | 295.1 |
| 15,000 | −56.5 | 12,045 | 0.194 | 0.158 | 295.1 |
| 20,000 | −56.5 | 5,475 | 0.088 | 0.072 | 295.1 |

Use: lift, drag and rotor thrust all scale with ρ, so a wing at 10 km needs about 1.7 times the true airspeed of sea level for the same lift (√(1/0.337), computed by me).

## J23-2. A wing section's lift and stall (measured data, read directly)

NACA 0012 (symmetric, 12% thick), Ladson, "Effects of Independent Variation of Mach and Reynolds Numbers on the Low-Speed Aerodynamic Characteristics of the NACA 0012 Airfoil Section", NASA TM 4074 (1988), https://ntrs.nasa.gov/citations/19880019495; data file served by the NASA Turbulence Modeling Resource, https://tmbwg.github.io/turbmodels/NACA0012_validation/CLCD_Ladson_expdata.dat (Re 6 million, Mach 0.15, transition tripped with 80 grit). US Government work.

| α (deg) | cl | cd |
|---|---|---|
| −4.04 | −0.442 | 0.0087 |
| 0 (−0.05) | −0.013 | 0.0081 |
| 4.04 | 0.432 | 0.0082 |
| 8.30 | 0.887 | 0.0105 |
| 12.12 | 1.261 | 0.0133 |
| 15.26 | 1.513 | 0.0190 |
| 17.13 | 1.612 | 0.0256 |
| 18.02 (stalled) | 0.997 | 0.188 |
| 19.08 | 1.136 | 0.273 |

Selected rows. Lift slope from −4° to 8.3°: 0.108 per degree (computed by me), close to thin-aerofoil theory's 2π per radian (0.110 per degree). Stall is abrupt: cl drops by about 40% and cd rises sevenfold between 17° and 18°. McCroskey's best fit across many tunnels (AGARD CP-429, 1988, Fig. 3, as given in the TMR file 0012.mccroskeydata.cl.dat): dcl/dα = (0.1025 + 0.00485 log10(Re/10⁶)) / √(1 − M²) per degree.

Banked level turn (standard result, computed by me): load factor n = 1/cos φ and stall speed rises by √n. 30° bank n = 1.15 (stall +7.5%); 45° n = 1.41 (+18.9%); 60° n = 2.00 (+41.4%); 75° n = 3.86 (+96.6%).

## J23-3. Aircraft specifications (Wikipedia infoboxes; each names its source in the infobox "ref" line)

| Aircraft | Empty / max take-off mass | Span, wing area | Power or thrust | Speeds | Climb, ceiling | Infobox source |
|---|---|---|---|---|---|---|
| Wright Flyer (1903) | 274 / 338 kg | 12.29 m, 47 m² | 12 hp (8.9 kW) | max 48 km/h | wing loading 6.4 kg/m² | https://en.wikipedia.org/wiki/Wright_Flyer |
| Cessna 172 | 1,691 / 2,450 lb (767 / 1,111 kg, computed by me) | 36 ft 1 in, 174 sq ft; aspect ratio 7.32; modified NACA 2412 | 160 hp Lycoming IO-360 | cruise 122 kt, stall 47 kt (power off, flaps down), Vne 163 kt | 721 ft/min, 13,500 ft | "Cessna, FAA type certificate"; https://en.wikipedia.org/wiki/Cessna_172 |
| Piper PA-28-150 Cherokee | 1,201 / 2,150 lb | 30 ft, 160 sq ft | 150 hp | max 123 kt, cruise 108 kt, stall 47 kt | 660 ft/min, 14,300 ft | "Piper Aircraft Owner's Handbook"; https://en.wikipedia.org/wiki/Piper_PA-28_Cherokee |
| Supermarine Spitfire Mk VB | 5,065 / 6,700 lb | 36 ft 10 in, 242.1 sq ft; NACA 2213 root, 2209.4 tip | 1,470 hp Merlin 45 | max 370 mph | 2,600 ft/min, 36,500 ft; 27.35 lb/sq ft | *Spitfire: The History*; *Jane's Fighting Aircraft of WWII*; https://en.wikipedia.org/wiki/Supermarine_Spitfire |
| Douglas DC-3 | 16,865 lb empty; 25,200 lb gross | 95 ft, 987 sq ft; aspect ratio 9.17 | 2 × 1,200 hp | max 223 kt at 8,500 ft, cruise 183 kt, stall 68 kt | 1,140 ft/min; 23,200 ft | *McDonnell Douglas Aircraft since 1920*; https://en.wikipedia.org/wiki/Douglas_DC-3 |
| C-130 Hercules | 75,800 / 155,000 lb | 132 ft 7 in, 1,745 sq ft | 4 × 4,590 shp | max 320 kt at 20,000 ft, cruise 292 kt | 1,830 ft/min; 33,000 ft empty | USAF fact sheet and others; https://en.wikipedia.org/wiki/Lockheed_C-130_Hercules |
| Boeing 737-800 | 41,413 / 79,016 kg | 35.79 m with winglets, 124.6 m² | 2 × 26,300 lbf CFM56-7B26 | Mach 0.82 max; long-range cruise 452 kt at 39,000 ft | ceiling 41,000 ft | https://en.wikipedia.org/wiki/Boeing_737_Next_Generation |
| F-16C Block 50/52 | 18,900 / 42,300 lb | 32 ft, 300 sq ft; NACA 64A204 | 17,155 lbf dry, 29,500 lbf with afterburner | Mach 2.05 | +9 g; thrust/weight 1.095 | USAF sheet; flight manual; https://en.wikipedia.org/wiki/General_Dynamics_F-16_Fighting_Falcon |
| Schleicher ASW 22 glider | 465 / 850 kg | 26.58 m, 16.7 m²; aspect ratio 38.3 | none | stall 65 km/h, Vne 280 km/h | +5.3 / −2.65 g; 33.4 kg/m² | *Jane's All the World's Aircraft 1988-89*; https://en.wikipedia.org/wiki/Schleicher_ASW_22 |

Computed by me from the Cessna 172 row: stall at 47 kt and 2,450 lb on 174 sq ft at sea level implies CLmax ≈ 1.88 with flaps down; wing loading 68.7 kg/m², matching the infobox's 68.6.

## J23-4. Lift-to-drag (glide) ratios (Wikipedia "Lift-to-drag ratio", each row with its cited source)

https://en.wikipedia.org/wiki/Lift-to-drag_ratio, "Examples":
- House sparrow 4:1; herring gull 10:1; common tern 12:1; albatross 20:1; Wright Flyer 8.3:1 (list as cited in the article).
- Boeing 747 in cruise 17.7:1 (Filippone, *Advanced topics in aerodynamics*, archived aerodyn.org table).
- Airbus A380 cruising 20:1 (Cumpsty, *Jet Propulsion*, CUP 2003, p. 4).
- Concorde 4:1 at take-off and landing, 12:1 at Mach 0.95, 7.5:1 at Mach 2 (Orlebar, *The Concorde Story*, Osprey 1997, p. 116).
- Helicopter at 100 kt 4.5:1 (Leishman, *Principles of Helicopter Aerodynamics*, CUP 2006, p. 230).
- Cessna 172 gliding 10.9:1; Lockheed U-2 cruising 25.6:1 (CIA "U2 Developments" transcript).
- Article's computed jetliner table: Airbus A320-200 16.3; Boeing 777-200 19.3; Boeing 747-400 15.5.

## J23-5. Rotors and hover (momentum theory, computed by me)

Ideal hover: induced velocity v_i = √(T / (2ρA)), ideal power P = T v_i, T = weight. Real rotors need about P_ideal / FM with figure of merit FM typically 0.7 to 0.8 (Leishman, *Principles of Helicopter Aerodynamics*, ch. 2; 0.75 used here). Masses and rotor diameters from Wikipedia infoboxes (UH-60: *Encyclopedia of Modern Warplanes*, US Army, Lockheed Martin brochure; R22: Robinson R22 Pilot's Operating Handbook; Bell 206: *Jane's All the World's Aircraft 1976-77*).

| Helicopter | Max take-off mass (kg) | Rotor diameter (m) | Disc loading (kg/m²) | v_i at sea level (m/s) | P_ideal (kW) | P at FM 0.75 (kW) | Installed (kW) |
|---|---|---|---|---|---|---|---|
| Robinson R22 | 621 | 7.67 | 13.4 | 7.3 | 45 | 60 | 92 (124 hp) |
| Bell 206 | 1,451 | 10.16 | 17.9 | 8.5 | 120 | 161 | 298 |
| UH-60 Black Hawk | 9,979 | 16.36 | 47.5 | 13.8 | 1,349 | 1,799 | 2,837 (2 × 1,902 shp) |

Check: the R22 infobox disc loading 2.61 lb/sq ft is 12.7 kg/m², near the 13.4 computed here (rounding of diameter or a different weight basis). Downwash speed under the rotor in the far wake is about 2 v_i in ideal theory (UH-60: about 28 m/s, computed by me): this is why dust and spray clouds under a heavy helicopter are violent.

## J23-6. Wind near the ground

Power law u = u_r (z / z_r)^α, with α ≈ 1/7 (0.143) for neutral stability (Wikipedia "Wind profile power law", whose references include Touma 1977, *J. Air Pollution Control Assoc.* 27:863-866, and Peterson and Hennessey 1978, *J. Appl. Meteorol.* 17:390-394). Computed by me: a 10 m/s wind at 10 m becomes 11.0 m/s at 20 m, 12.6 m/s at 50 m, 13.9 m/s at 100 m, and 7.2 m/s at 1 m. Beaufort scale and wave heights are in the main library's G1.

## Sources disagree

- Standard-atmosphere density at 10 km: 0.413 kg/m³ computed here using geopotential height; the 1976 table lists 0.4135 at 10 km geometric (geometric height is about 16 m higher than geopotential at 10 km). Use geopotential for the formula.
- Lift slope: measured 0.108 per degree (Ladson, computed by me) against McCroskey's fit 0.1075 per degree at Re 6 million and Mach 0.15 (computed by me from his formula); thin-aerofoil 0.110 per degree.
- R22 disc loading: infobox 2.61 lb/sq ft (12.7 kg/m²) against 13.4 kg/m² computed from the infobox's own mass and diameter.

## Not found

- Consumer drone figures (DJI Mavic 3 mass, top speed, wind resistance): the maker's spec page renders by script and returned no text; not used.
- NACA Report 824 (Abbott, von Doenhoff and Stivers 1945) itself: the NTRS download did not finish in this session; the Ladson data stand in for it.
- Measured helicopter rotor RPM and blade flap angles from an open primary source.
- Open measured gust spectra for low-altitude game flight (Dryden and von Kármán gust models are in MIL-HDBK-1797 and MIL-F-8785C, not opened this session).
- Paper-plane and thrown-glider glide ratios from a primary measurement.
