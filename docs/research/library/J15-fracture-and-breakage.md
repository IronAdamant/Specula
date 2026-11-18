# J15: Fracture toughness, stiffness and breaking energy (wood, glass, stone, steel)

Fixed after the second check (2026-11-18):
- J15-2: the Wood Handbook's text on toughness, moisture and the speed of sound is on p. 5-17, not p. 5-27; Tables 5-9 and 5-10 are on p. 5-28. The values all matched.
- J15-3: OpenStax is CC BY-NC-SA 4.0 (not CC BY 4.0), with a notice against ingestion into AI offerings: link only, held for a licence decision before any use. Its label is "Hardwood", without "along grain".
- Sources disagree: mode II is about 4 to 7 times mode I by species (3 to 11 across the full ranges), not 4 to 6.
- Added: the checker's reading of the Mode II orientations; the source of G = K²/E; that Wikipedia's granite strength has no reference of its own. A "Sources" list added at the end.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Values marked "computed by me" are arithmetic, never measurements. Round-one I3 (material values) and round-two J3 (densities) hold density and strength; this file adds how hard it is to grow a crack.

## J15-1. Fracture toughness K_Ic (read directly, secondary)

Wikipedia "Fracture toughness", table "Typical values", https://en.wikipedia.org/wiki/Fracture_toughness (row references noted where the page gives them). Units MPa·m^½.

| Material | K_Ic | Row reference on the page |
|---|---|---|
| Aluminium | 14-28 | none given |
| Aluminium alloy 7075 | 20-35 | Kaufman 2015 |
| Inconel 718 | 73-87 | ASM Handbook 1996 |
| Maraging steel (200 grade) | 175 | none given |
| Steel alloy 4340 | 50 | none given |
| Titanium alloy | 84-107 | AZO Materials (Ti6Al4V) |
| Aluminium oxide | 3-5 | none given |
| Silicon carbide | 3-5 | none given |
| Soda-lime glass | 0.7-0.8 | none given |
| Concrete | 0.2-1.4 | none given |
| PMMA (acrylic) | 0.7-1.6 | none given |
| Polystyrene | 0.7-1.1 | none given |
| Mullite-fibre composite | 1.8-3.3 | Boccaccini |
| Silica aerogel | 0.0008-0.0048 | Phalippou et al. 1989, doi:10.1051/jphyscol:1989431 |

## J15-2. Wood (read directly, public domain)

USDA Forest Products Laboratory, *Wood Handbook: Wood as an Engineering Material*, FPL-GTR-282 (2021), chapter 5: text on p. 5-17 (PDF page 133), Tables 5-9 and 5-10 on p. 5-28 (PDF page 144), https://research.fs.usda.gov/treesearch/62200 (PDF https://research.fs.usda.gov/download/treesearch/62200.pdf; US government work).

- Text p. 5-17: mode I (opening) toughness 220-550 kPa·m^½; mode II (shear) 1,650-2,400 kPa·m^½. Cracks prefer to run along the grain (RL and TL systems) because wood is weak across it. Toughness is insensitive to moisture or peaks between 6% and 15% moisture.
- Speed of sound (p. 5-17): 3,800 m/s along the grain for a wood with E = 12.4 GPa and density 480 kg/m³; about 890 m/s across the grain (transverse E about 690 MPa). Across-grain modulus can be as little as 1/20 of the long-grain value.

### Table J15-2a. Selected wood fracture toughness, 10-12% moisture (retyped selection from Table 5-10, p. 5-28, kPa·m^½)

| Species | Mode I, TL | Mode I, RL | Mode II |
|---|---|---|---|
| Douglas-fir | 320 | 360 | 2,230 |
| Western hemlock | 375 | | 2,240 |
| Western white pine | 250 | 260 | |
| Scots pine | 440 | 500 | 2,050 |
| Southern pine | 375 | | 2,070 |
| Ponderosa pine | 290 | | |
| Red spruce | 420 | | 2,190 and 1,665 (two orientations) |
| Northern red oak | 410 | | |
| Sugar maple | 480 | | |
| Yellow-poplar | 517 | | |

Blank cells are blank in the source. The column for each Mode II value (TL or RL) is hard to read in the PDF text layer, so treat the Mode II orientation as uncertain. The second checker's reading, by column position in the PDF text layer (offered, not certain): Douglas-fir 2,230, western hemlock 2,240, Scots pine 2,050, southern pine 2,070 and red spruce 2,190 sit under Mode II TL, and red spruce 1,665 under Mode II RL; Douglas-fir's RL Mode I is 360 (330 lbf).

Table 5-9 (p. 5-28) gives impact "toughness" as energy to break a standard small specimen, in joules, for example: coast Douglas-fir, green, 24 J radial and 41 J tangential; western red cedar at 9% moisture, 10 J and 15 J. These are per-specimen energies (the specimen is defined in the Handbook), not material constants.

## J15-3. Stiffness (read directly)

OpenStax *University Physics Volume 1*, section 12.3, Table 12.1 "Approximate Elastic Moduli for Selected Materials", https://openstax.org/books/university-physics-volume-1/pages/12-3-stress-strain-and-elastic-modulus (CC BY-NC-SA 4.0, OpenStax, Ling, Sanny, Moebs et al., 2016: non-commercial and share-alike, and the page also says its content may not be ingested into large language models or generative AI offerings without OpenStax's prior written permission. Link only: these values are not to be copied into a product, and this table is held for a licence decision before any use). Young's modulus, GPa (retyped from ×10¹⁰ Pa):

| Material | Young's modulus |
|---|---|
| Steel | 200 |
| Iron | 210 |
| Aluminium | 70 |
| Crown glass | 60 |
| Marble | 60 |
| Granite | 45 |
| Polystyrene | 30 (as printed; usual handbook values are near 3 GPa, so check before use) |
| Concrete | 20 |
| Brick | 15 |
| Hardwood | 15 (OpenStax labels it "Hardwood" only; "along grain" was this file's gloss) |

Granite: density 2.65-2.75 g/cm³ and compressive strength usually above 200 MPa (Wikipedia "Granite", https://en.wikipedia.org/wiki/Granite, secondary; its EduMine reference supports the density only, and the compressive strength has no reference of its own).

## J15-4. Energy to grow a crack (computed by me)

Energy release rate G = K² / E (plane stress, linear elastic; Wikipedia "Fracture mechanics", https://en.wikipedia.org/wiki/Fracture_mechanics, gives G = K_I²/E for plane stress). Uses J15-1 and J15-3 values; a game can use G as "joules per square metre of new crack face" to budget how much a break costs.

| Material | K used (MPa·m^½) | E used (GPa) | G (J/m²) |
|---|---|---|---|
| Crown or soda-lime glass | 0.75 | 60 | about 9 |
| 4340 steel | 50 | 200 | about 12,500 |
| Aluminium | 21 | 70 | about 6,300 |
| Concrete | 0.2 to 1.4 | 20 | about 2 to 100 |
| Polystyrene | 0.9 | 30 (OpenStax as printed) | about 27 (about 270 if E = 3 GPa) |
| Douglas-fir, TL mode I | 0.32 | 0.69 to 12.4 (Handbook's across and along values) | about 8 to 150 (wood is orthotropic, so this is only a bracket) |

Rule of thumb from the table: glass and stone break at around 10 J/m² of crack, metals need thousands, which is why metal bends before it breaks.

## Sources disagree

- Polystyrene Young's modulus: OpenStax Table 12.1 prints 3.0 × 10¹⁰ Pa (30 GPa); most polymer references put it near 3 GPa. Not resolved this pass.
- Wood toughness depends on orientation: mode II is about 4 to 7 times mode I (4.7 for Scots pine to 7.0 for Douglas-fir, species paired in Table 5-10), and 3 to 11 times across the full ranges (1,650 / 550 = 3.0 to 2,400 / 220 = 10.9; like-for-like endpoints 1,650 / 220 = 7.5 and 2,400 / 550 = 4.4). Computed by the second checker; this file first said "4 to 6 times".

## Not found

- Fracture toughness of granite, marble and sandstone from an open primary source.
- Shatter pattern statistics (number and size of pieces against impact energy) for glass or stone.
- Charpy energies for common steels in an open table.

## Sources

- Wikipedia, "Fracture toughness", table "Typical values" (row references as given in J15-1: Kaufman 2015; ASM Handbook 1996; AZO Materials; Boccaccini; Phalippou et al. 1989, doi:10.1051/jphyscol:1989431; none opened). https://en.wikipedia.org/wiki/Fracture_toughness . CC BY-SA 4.0. Accessed 2026-11-17.
- USDA Forest Products Laboratory, *Wood Handbook: Wood as an Engineering Material*, FPL-GTR-282 (2021), chapter 5, pp. 5-17 and 5-28. https://research.fs.usda.gov/treesearch/62200 ; PDF https://research.fs.usda.gov/download/treesearch/62200.pdf . US government work (public domain). Accessed 2026-11-17.
- OpenStax, *University Physics Volume 1* (Ling, Sanny, Moebs et al., 2016), section 12.3, Table 12.1. https://openstax.org/books/university-physics-volume-1/pages/12-3-stress-strain-and-elastic-modulus . CC BY-NC-SA 4.0 (non-commercial; link only). Accessed 2026-11-17.
- Wikipedia, "Granite" (citing EduMine for the density). https://en.wikipedia.org/wiki/Granite . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Fracture mechanics" (G = K_I²/E for plane stress; added after the second check). https://en.wikipedia.org/wiki/Fracture_mechanics . CC BY-SA 4.0.
