# R7-34 Primary-source checks for part 3 (Wikipedia rows) and part 4 (unverified rows)

Round 7. **Access date for every source: 2026-11-24.** Only the rows I could settle this round are here; the rest are listed in the README coverage map as not attempted.

## U1. Tarantula *Eupalaestrus weijenberghi* tarsus lengths (C7 row 1.18)

Source: Silva-Pereyra V, Fábrica C, Biancardi C, Pérez-Miles F (2019). "Kinematics of male *Eupalaestrus weijenberghi* (Araneae, Theraphosidae) locomotion on different substrates and inclines." *PeerJ* 7: e7748. doi:10.7717/peerj.7748. PMC6766366. **Free to read, CC BY 4.0** (Crossref licence field and the article's own statement). Read: the publisher's JATS XML through Europe PMC (fullTextXML), Table 1, "Measures and inertial parameter of leg segments (mean ± standard deviation, n = 12)". The PeerJ PDF answered 403 to my script, so the PDF itself was not opened.

**Finding (settles U1): the four "tarsus" numbers are not lengths.** In the XML the Tarsus rows have empty Diameter and Length cells, and the values sit in the **third data column, "Mass (10^-2 g)"**. The columns are: Segment, Leg, Diameter (mm), Length (mm), Mass (10^-2 g), Center of mass (% from proximal end), Radius of gyration (mm), Moment of inertia.

| Q/U | Leg | Cell as published (column "Mass (10^-2 g)") | Reading |
|---|---|---|---|
| U1 | I | 10 ± 0.3 | tarsus mass, 10^-2 g; the "10" next to "0.8" for legs II and III suggests a lost decimal point ("1.0"), **not confirmable without the PDF** |
| U1 | II | 0.8 ± 0.3 | tarsus mass, 10^-2 g |
| U1 | III | 0.8 ± 0.4 | tarsus mass, 10^-2 g |
| U1 | IV | 10 ± 0.2 | as leg I |

**Tarsus length: not given in Table 1 at all.** The verified rows from the same table, for reference (length, mm, mean ± SD, n = 12 males): femur I 14.12 ± 0.70, II 12.91 ± 0.61, III 11.48 ± 0.63, IV 14.13 ± 0.71; patella-tibia I 16.87 ± 0.73, II 15.11 ± 0.56, III 13.59 ± 0.49, IV 18.52 ± 0.59; metatarsus I 10.11 ± 0.58, II 9.95 ± 0.54, III 11.18 ± 0.32, IV 16.96 ± 0.60.

Action for the library: delete the four tarsus "lengths" from C7 row 1.18; mark tarsus length as Not found for this species.

## W64. Lockheed C-130 Hercules (J23)

Primary: US Air Force fact sheet "C-130 Hercules", af.mil/About-Us/Fact-Sheets/Display/Article/1555054/c-130-hercules/ ("Current as of June 2018"). **Free to read; a US government work (public domain in the US).**

| W | Item | Wikipedia figure (as delivered) | USAF fact sheet figure | Agree? |
|---|---|---|---|---|
| W64 | Span | 132 ft 7 in | "Wingspan: 132 feet, 7 inches (39.7 meters)" | yes |
| W64 | Max take-off weight | 155,000 lb | C-130E/H 155,000 lb (69,750 kg); C-130J 164,000 lb (74,393 kg) | yes, for the E/H |
| W64 | Engines | 4 x 4,590 shp | C-130H: four Allison T56-A-15, "4,591 prop shaft horsepower"; C-130E 4,200; C-130J 4,700 hp | 1 shp apart (H model) |
| W64 | Speed | max 320 kt; cruise 292 kt | C-130H 366 mph / 318 ktas (Mach 0.52) at 20,000 ft; C-130E 300 ktas; C-130J 362 ktas at 22,000 ft (labelled "Speed", not split into max and cruise) | different figure types |
| W64 | Ceiling | 33,000 ft | C-130H 23,000 ft **with 42,000 lb payload**; C-130J 28,000 ft with 42,000 lb | different condition (loaded) |
| W64 | Length / height | (not in row) | 97 ft 9 in (29.3 m) / 38 ft 10 in (11.9 m) | |
| W64 | Empty weight 75,800 lb, wing area 1,745 sq ft, climb 1,830 ft/min | as delivered | **not in the fact sheet** | Not found in this primary |

## W66. General Dynamics F-16C (J23)

Primary: US Air Force fact sheet "F-16 Fighting Falcon", af.mil/About-Us/Fact-Sheets/Display/Article/104505/f-16-fighting-falcon/ ("Current as of September 2021"). **Free to read; US government work (public domain in the US).**

| W | Item | Wikipedia figure (as delivered) | USAF fact sheet figure | Agree? |
|---|---|---|---|---|
| W66 | Span | 32 ft 8 in | "Wingspan: 32 feet, 8 inches (9.8 meters)" | yes |
| W66 | Empty weight | 18,900 lb | "Weight: 19,700 pounds without fuel (8,936 kilograms)" | **no** (different definitions possible) |
| W66 | Max take-off / gross | 42,300 lb max, gross 26,500 lb | "Maximum takeoff weight: 37,500 pounds (16,875 kilograms)" | **no** |
| W66 | Thrust | 17,155 lbf dry, 29,500 lbf with afterburner | "Thrust: F-16C/D, 27,000 pounds" (engine not split dry/wet) | **no** |
| W66 | Speed | Mach 2.05 | "1,500 mph (Mach 2 at altitude)" | rounding |
| W66 | Ceiling | 50,000 ft | "above 50,000 feet (15 kilometers)" | yes |
| W66 | Load factor | +9 g | "can withstand up to nine G's" (with full internal fuel) | yes |
| W66 | Length / height | (not in row) | 49 ft 5 in (14.8 m) / 16 ft (4.8 m) | |
| W66 | Wing area 300 sq ft, NACA 64A204, thrust/weight | as delivered | **not in the fact sheet** | Not found in this primary |

## Not found (this file)
- U1: tarsus lengths (the table has none); the PeerJ PDF (to confirm whether "10" is "1.0").
- W64: empty weight, wing area, climb rate; W66: wing area, aerofoil, thrust/weight in the fact sheets.

## Sources disagree (this file)
- U1: the earlier round read mass cells as lengths.
- W66: the fact sheet's 19,700 lb (without fuel), 37,500 lb (max take-off) and 27,000 lb thrust against the page's 18,900, 42,300 and 29,500. The fact sheet is the public-domain primary; prefer it, but note it covers all F-16C/D blocks and both engines, while the page's row is Block 50/52.
- W64: the page's "cruise 292 kt" and "ceiling 33,000 ft" are not the fact sheet's figures (318 ktas "speed" at 20,000 ft; 23,000 ft ceiling with 42,000 lb payload, both C-130H).
