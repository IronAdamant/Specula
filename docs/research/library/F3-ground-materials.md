# F3: how natural ground materials look (rock forms, albedo, colour, roughness, snow line)

Item F3 of `LIBRARY_RESEARCH_BRIEF.md` (§F Terrain). Compiled 2026-11-16 (Australia/Sydney). This is a private working file: nothing here was posted, published or committed. No dataset was downloaded into any repo; scratch copies are in `/tmp/f3/` and `/tmp/f3g/`.

## Fixes after verify (2026-11-17, Australia/Sydney, AEDT)

Applied from `F3-ground-materials.md.verify.md`. Brock table numbers were confirmed on the journal page images; no WGMS numbers were changed.

1. **Brock 2006 Arolla z0:** cited to **Table 3** (not Table 4). Table 3 has the sample sizes and the microtopographic values. Table 4 is the wind-profile height-adjustment means (−50 / 0 / +50 mm), with no n.
2. **Brock literature ranges:** cited to **Table 1a/1b** (not Tables 1–2). Table 2 lists sampling dates. (The Cambridge Core PDF of the same article numbers Table 1a/1b as Tables 1–2, and Table 3 as Table 4; that typography difference is under 'Sources disagree'.)
3. **WGMS method:** two previously unstated steps are now written out: ELA values ≤ 0 m are dropped (6 observations), and latitude bands include their upper edge (right-closed: glacier 150 at 60.000°N is in 50–60°N). Band medians, IQRs and counts are unchanged.

Later the same day, optional tidy-ups from the leftover-row verify (F3 was otherwise clear, 48/0/0). Confirmed on the official pages before editing.

4. **G11 Arches stability:** the NPS page says this area has been "rather geologically stable for **at least 50,000 years**", not "about 50,000".
5. **G17 licence:** **CC BY-NC-ND 4.0** (non-commercial, no derivatives). The version was previously omitted.

## 0. Read this first

- **Source keys.** Every number carries a key (G1, G2, …). Each key has a full entry in §11 with its URL. Every source was accessed on **2026-11-16**.
- **Kind of number.**
  - **meas**: measured in the field or the lab.
  - **calc**: computed by me from a source's raw data. The method is stated in the row.
  - **desc**: a descriptive statement or range from the source.
- **Verification marks.**
  - ✔ = read by me in the primary text, data file or page.
  - [IMG] = read from a rendered page image of a scanned PDF.
  - ◐ = a secondary source, or the abstract only.
  - ⚠ = doubtful as printed.
- **Reflectance is not albedo.** Lab spectra (G2) are directional reflectance of a sample, averaged here over a waveband. Broadband albedo (G4, G5) is hemispheric, solar-weighted and measured in the field. Do not mix the two without saying so. I could not solar-weight the G2 spectra: the NREL ASTM G173 solar spectrum download failed with an SSL error.
- **Method.** WebSearch, WebFetch, curl, pdftotext and Python (pandas) on downloaded data files.
  - **A Grok live-search pass was also run.** Settings: Grok CLI, model grok-4.6, reasoning effort xhigh, live web search. It was launched at about 10:48 AEDT and the process exited before about 10:58 AEDT. Raw output is in `/tmp/grokrun/f3.out`.
  - Every Grok claim used here was re-opened by me in the primary source. Those rows are marked "via Grok ✔".
  - Grok claims I could not open are listed in §9, and conflicts are in §10.
  - For one source (Grenfell 1994) Grok recovered text from a pirate mirror. I did **not** use that; I read the same table from the authors' own scan.
- **Licences.** Facts and numbers only; no copied figures or wording. Licences that matter for reuse:
  - USGS splib07 data (G2) is **public domain**.
  - The ECOSTRESS library (G8) is **© Caltech, all rights reserved**, with mandatory citations.
  - The WGMS FoG (G1) is **open access, with correct citation required**.
  - G17 is **CC BY-NC-ND 4.0 (non-commercial, no derivatives)**.
  - The UC Davis soil-colour table (G6) states **no licence**.

## 1. Sources and licences

| Key | Source | What it gives | Licence, in plain words | Read |
|---|---|---|---|---|
| G1 | WGMS (2026) Fluctuations of Glaciers Database, version 2026-02-10 | measured equilibrium-line altitude (ELA) per glacier and year | open access; correct citation required | ✔ data |
| G2 | Kokaly et al. (2017) USGS Spectral Library Version 7 data (splib07a) | lab reflectance spectra of snow, sand, rocks, soils, mud | public domain (USGS data release) | ✔ data |
| G3 | Kokaly et al. (2017) USGS Data Series 1035 (report for G2) | licence wording | "for the most part, is in the public domain"; it may contain third-party copyrighted items | ✔ |
| G4 | Bøggild, Brandt, Brown & Warren (2010) J. Glaciol. 56(195):101–113 | broadband albedo of glacier ice types, debris, tundra and melting snow | © International Glaciological Society. Author copy (UW) | ✔ |
| G5 | Grenfell, Warren & Mullen (1994) JGR 99(D9):18669–18684 | spectral and broadband albedo of Antarctic snow | © 1994 AGU. Author scan (UW) | ✔ [IMG] |
| G6 | UC Davis California Soil Resource Lab, "unique moist colour" lookup table (CSV) behind the 2018 SSURGO soil-colour maps | how often each moist Munsell colour occurs, plus sRGB | **no licence stated** | ✔ data |
| G7 | Beaudette, Ferguson & Nemecek (2019) "The Geography of Soil Color" (presentation) | context and authorship for G6 | no licence stated | ✔ |
| G8 | ECOSTRESS Spectral Library website (JPL) | licence wording; library contents | "Copyright © 2017, California Institute of Technology. ALL RIGHTS RESERVED"; credit line and two citations required | ✔ |
| G9 | Meerdink, Hook, Roberts & Abbott (2019) RSE 230:111196 | required citation for G8 | © Elsevier (article) | ◐ metadata |
| G10 | Baldridge, Hook, Grove & Rivera (2009) RSE 113:711–715 | required citation for G8; ASTER library contents | © Elsevier. Copy read | ✔ |
| G11 | NPS, Arches National Park, "Geologic Formations" | arch counts and size limits (sandstone) | US Government work (public domain) | ✔ |
| G12 | Huber & Eckhardt, "Devils Postpile Story", as the USGS/NPS web page dpgeol5 (Wayback capture 27 May 2010) | column diameter, length and number of sides | US Government work. The pamphlet is published by Sequoia Natural History Association | ✔ |
| G13 | Glazner (2025) "Columnar jointing in the Devils Postpile…", WMRC 75th Anniversary Symposium, UC eScholarship, DOI 10.83206/B1RP47 (via Grok) | column polygon counts from a 940-polygon map | no licence stated | ✔ |
| G14 | Goehring & Morris (2005) "Order and disorder in columnar joints", Europhys. Lett. (arXiv cond-mat/0501015) | columns are not tending to a perfect hexagonal lattice | arXiv preprint; journal © EDP/IOP | ✔ |
| G15 | Mitchell (2010) MS thesis, Univ. of Hawaiʻi (SOEST), sheeting joints (via Grok) | Matthes 1930 sheet thicknesses, as cited | thesis; no licence stated | ✔ (secondary for Matthes) |
| G16 | Collins & Stock (2016) Nature Geoscience 9, Supplementary Table S1 (via Grok) | size of a monitored exfoliation sheet in Yosemite | © Springer Nature (supplement) | ✔ |
| G17 | Lončar & Grcić (2022) "GIS-based analysis of doline density on Miljevci karst plateau (Croatia)", Acta Carsologica 51(1) | doline counts, densities and density classes | **CC BY-NC-ND 4.0** (non-commercial, no derivatives) | ✔ |
| G18 | Brock, Willis & Sharp (2006) J. Glaciol. 52(177):281–297 (via Grok) | aerodynamic roughness length z0 over snow, slush and ice | © IGS / Cambridge. Free to read | ✔ |
| L17, L18 | Grand Canyon strata (NPS); see F1 | which rock types form cliffs and which form slopes | public domain (NPS) | ✔ (in F1) |

## 2. Rock forms by rock type

| Rock type and form | Numbers (spread) | Kind | Source |
|---|---|---|---|
| **Sandstone: arches** (Entrada / Navajo, Arches NP) | over 2,000 documented arches; spans from "sliver-thin cracks" to more than 300 ft (the page prints "97 m", see §10); an official arch needs an opening of at least 3 ft (1 m) in one direction; the arch count suggests at least 50,000 years of relative geological stability | desc | G11 ✔ |
| **Sandstone and limestone vs shale: cliffs vs slopes** | hard sandstone and limestone stand as cliffs; shale "melts" into slopes. Grand Canyon thicknesses by unit are in F1 §8 | desc | L18 ✔, L17 ✔ |
| **Basalt / andesite: columnar joints** (Devils Postpile; 200-post sample) | average diameter 2 ft (about 0.6 m), maximum 3.5 ft; some columns 60 ft long. Number of sides: 4 = 2%, 5 = 37%, 6 = 55%, 7 = 5%; a few have 3 | meas | G12 ✔ |
| Same, full map (940 polygons) | 6 sides 53%, 5 sides 27%, 7 sides 18%. Columns up to 20 m tall. Only this study and O'Reilly (1879, Giant's Causeway) found about 20% 7-gons | meas | G13 ✔ via Grok |
| Columnar joints: shape rule | the pattern is ordered and "mostly hexagonal" but does **not** tend towards a perfect hexagonal lattice | desc | G14 ✔ |
| **Granite: exfoliation (sheeting) joints**, Yosemite | sheets from "less than a meter to ~30 m" thick (Matthes 1930, as cited); sheeting seen at depths as great as 30 m | desc | G15 ✔ (secondary for Matthes) |
| One monitored granodiorite sheet, Yosemite Valley | 19 m long × 4 m wide × 0.1 m average thickness | meas | G16 Table S1 ✔ via Grok |
| **Limestone: karst dolines** (Miljevci plateau, Croatia) | 286 dolines mapped; two concentration zones of 30 and 34 dolines/km² (0–34/km² across the plateau) | meas | G17 ✔ |
| Doline density classes (Pahernik 2012, quoted by G17) | negligible 1–10 /km²; small 10–30; medium 30–60; large 60–100; very high 100–200; extremely high above 200 | desc | G17 ✔ |

## 3. Broadband albedo (measured, field)

**Glacier surfaces, NE Greenland (G4 Table 1 ✔ meas).** Broadband shortwave albedo, clear sky / overcast:

| Surface | Clear | Overcast |
|---|---|---|
| Melting coarse snow | 0.67 | 0.75 |
| Melting snow, top 3 cm (debris) removed | 0.68 | 0.76 |
| Holocene ice with cryoconite holes | 0.57 | 0.65 |
| Pleistocene ice with uniform debris | 0.52 | 0.58 |
| Debris ice, light | 0.40 | 0.44 |
| Debris ice, intermediate | 0.28 | 0.30 |
| Debris ice, dark | 0.21 | 0.22 |
| Cryoconite basin | 0.10 | 0.11 |
| Cryoconite material (damp) | 0.09 | 0.09 |
| Tundra soil, dry | 0.23 | 0.21 |
| Tundra (rocks and soil) | 0.18 | 0.16 |
| Tundra soil, damp | 0.15 | 0.14 |

- Abstract: ice albedo ranges from 0.2 (heavy uniform debris) to 0.6 (hummocks with cryoconite holes) (G4 ✔).
- Superimposed ice: 0.68. Ice with heavy impurity loading tends towards about 0.15 (G4 ✔).

**Dry polar snow, Antarctica (G5 [IMG] ✔ meas).**
- Spectral albedo is 0.96–0.98 across the UV and visible. It falls below 0.15 in the 1.5 and 2.0 µm bands.
- Spectrally averaged albedo is 0.80–0.85.
- Table 7, broadband 0.3–5.0 µm / visible 0.3–0.7 / near-IR 0.7–5.0:

  | Condition | 0.3–5.0 µm | 0.3–0.7 µm | 0.7–5.0 µm |
  |---|---|---|---|
  | Cloudy sky, average | 0.83 | 0.98 | 0.67 |
  | Cloudy sky, upper envelope | 0.85 | 0.99 | 0.71 |
  | Cloudy sky, lower envelope | 0.80 | 0.97 | 0.64 |
  | Clear sky, South Pole, sun zenith 72° | 0.85 | 0.99 | 0.72 |
  | Clear sky, South Pole, sun zenith 68° | 0.84 | 0.99 | 0.70 |
  | Clear sky, Vostok, sun zenith 55° | 0.80 | 0.98 | 0.63 |

- NOAA-CMDL monthly broadband means at the South Pole, Dec 1985 – Feb 1988, range 0.81–0.87 (same table).

## 4. Spectral reflectance and colour (lab, USGS splib07a, G2 ✔ calc)

Each value is my mean of the splib07a ASD/BECK spectrum over the waveband: blue 0.45–0.50 µm, green 0.52–0.58, red 0.62–0.68, visible 0.40–0.70, near-IR 0.80–1.10. These are **reflectance factors, not albedo** (§0).

| Sample (splib07a name) | Blue | Green | Red | VIS | NIR |
|---|---|---|---|---|---|
| Melting snow mSnw01a | 0.833 | 0.832 | 0.822 | 0.829 | 0.646 |
| Melting snow mSnw03 | 0.721 | 0.723 | 0.716 | 0.720 | 0.533 |
| Melting snow mSnw04 / 05 / 08 | | | | 0.664 / 0.624 / 0.605 | 0.485 / 0.452 / 0.378 |
| Slush mSnw09 / 12 / 14 | | | | 0.579 / 0.562 / 0.499 | 0.297 / 0.252 / 0.212 |
| Slush mSnw15 / 16 | | | | 0.249 / 0.203 | 0.116 / 0.080 |
| Sand, Grand Isle (GrndIsle1) | 0.196 | 0.243 | 0.271 | 0.230 | 0.316 |
| Sand DWO, dry | 0.170 | 0.218 | 0.254 | 0.209 | 0.305 |
| Sand DWO, wet (ar2) | 0.095 | 0.115 | 0.129 | 0.111 | 0.147 |
| Sand DWO, wet (2b) | 0.082 | 0.121 | 0.141 | 0.112 | 0.310 |
| Basalt, fresh | 0.080 | 0.092 | 0.107 | 0.092 | 0.107 |
| Basalt, weathered | 0.111 | 0.129 | 0.145 | 0.126 | 0.157 |
| Pyroxene basalt | 0.133 | 0.157 | 0.173 | 0.152 | 0.177 |
| Limestone CU02-11A | 0.165 | 0.191 | 0.213 | 0.186 | 0.258 |
| Hydrated volcanic tuff | 0.289 | 0.346 | 0.387 | 0.332 | 0.419 |
| Stonewall Playa dry mud | 0.262 | 0.364 | 0.478 | 0.359 | 0.538 |
| Jarosite-bearing rhyolite | | | | 0.289 | 0.257 |
| Desert varnish GDS141 / ANP90-14 | | | | 0.064 / 0.062 | 0.177 / 0.106 |
| Burn area (ash) | 0.026 | 0.028 | 0.031 | 0.028 | 0.046 |

Patterns in the table:
- Snow is near-white in the visible and darker in the near-IR. Slush drops steeply in both.
- Wetting this sand roughly halves its visible reflectance (0.209 to about 0.11).
- Most rocks and soils are redder than they are blue (red > green > blue).
- Fresh basalt is about 0.09 in the visible and brightens a little with weathering.

## 5. Soil colour (Munsell)

**Moist soil colours in the 2018 SSURGO colour maps** (G6 CSV ✔ calc; context G7 ✔). 454,664 occurrences across 441 distinct colours. The "freq" column is undocumented; I treat it as occurrences.

| Hue | Share | | Value | Share | | Chroma | Share |
|---|---|---|---|---|---|---|---|
| 10YR | 62.2% | | 4 | 34.0% | | 2 | 29.6% |
| 7.5YR | 14.4% | | 3 | 26.6% | | 3 | 24.9% |
| 2.5Y | 9.2% | | 5 | 20.7% | | 4 | 22.6% |
| 5YR | 7.4% | | 2 | 8.4% | | 1 | 12.0% |
| 2.5YR | 2.7% | | 6 | 6.5% | | 6 | 8.5% |
| 5Y | 2.4% | | | | | | |
| N (neutral) | 0.9% | | | | | | |

| Munsell (moist) | Share | sRGB from G6 |
|---|---|---|
| 10YR 3/2 | 8.36% | 83, 68, 52 |
| 10YR 4/3 | 8.05% | 115, 92, 64 |
| 10YR 3/3 | 5.38% | 88, 67, 43 |
| 10YR 4/2 | 4.79% | 110, 93, 75 |
| 10YR 4/4 | 4.40% | 120, 91, 54 |
| 10YR 2/2 | 3.78% | 58, 45, 31 |
| 10YR 2/1 | 3.77% | 52, 46, 39 |
| 10YR 5/4 | 3.77% | 147, 117, 79 |
| 10YR 5/3 | 3.63% | 142, 118, 90 |
| 7.5YR 4/4 | 3.19% | 124, 89, 58 |
| 10YR 3/1 | 2.41% | — |
| 10YR 5/2 | 1.91% | — |
| 2.5Y 4/2 | 1.90% | — |
| 10YR 5/6 | 1.79% | — |
| 7.5YR 3/2 | 1.73% | — |

- The top 15 colours make up 58.9% of all occurrences.
- These are US soils only, and moist colours only.

## 6. Roughness

**Aerodynamic roughness length z0 over glacier surfaces (G18 ✔ meas, via Grok).**
- Literature ranges quoted by G18 (its Table 1a/1b):
  - melting mid-latitude glacier surfaces 0.1–10 mm;
  - high-latitude glaciers and ice sheets 0.001–10 mm;
  - smooth fresh snow about 0.1 mm (down to 0.01–0.001 mm on polar snow);
  - melting snow 1–10 mm.
- Haut Glacier d'Arolla, measured (Table 3; mean z0 in mm, with sample size):
  - snow with ablation hollows: wind profile 3.56 (n = 14); microtopographic 2.31 across the wind and 1.62 along it;
  - slush: wind profile 0.88 (n = 10);
  - ice: wind profile 6.89 (n = 34); microtopographic 6.96 across the wind.
- The snow surface had ablation hollows with 0.1 m vertical relief and 0.5–1.0 m spacing.

Roughness of sand, soil, desert pavement and lava, and optical (Hapke) roughness: see §9.

## 7. Snow line by latitude (glacier ELA as a proxy)

**Measured ELA from the WGMS Fluctuations of Glaciers database (G1 ✔ calc).**
- Data: 5,512 annual observations from 343 glaciers, 1885–2025, excluding ELA flagged as above or below the glacier, and ELA values ≤ 0 m (6 observations).
- Method: for each glacier I took the median of its annual ELAs, then the median across glaciers in each 10° latitude band. Bands include their upper edge (right-closed: 50–60°N means > 50° and ≤ 60°), so glacier 150 at 60.000°N is in 50–60°N.

| Latitude band | Median ELA (m) | Glaciers |
|---|---|---|
| 80–90°N | 700 | 2 |
| 70–80°N | 555 (IQR 438–704) | 26 |
| 60–70°N | 1,308 (IQR 1,104–1,491) | 72 |
| 50–60°N | 1,810 (IQR 1,290–2,570) | 41 |
| 40–50°N | 3,075 (IQR 2,837–3,400) | 129 |
| 30–40°N | 4,861 (IQR 4,477–5,222) | 30 |
| 20–30°N | 5,528 | 8 |
| 10–20°N | 5,105 | 1 |
| 0–10°N | 5,009 | 3 |
| 0–10°S | 4,912 | 7 |
| 10–20°S | 5,340 | 3 |
| 20–30°S | 5,288 | 3 |
| 30–40°S | 4,560 | 3 |
| 40–50°S | 1,816 | 5 |
| 50–60°S | 1,052 | 2 |
| 60–70°S | 231 | 8 |

How to read the table:
- The highest snow lines are in the dry subtropics (20–30°), not at the equator.
- Snow lines fall steeply between 30° and 50° in both hemispheres.
- Polar glaciers have ELAs of a few hundred metres.
- **Strong sampling bias:** most glaciers are in the northern mid-latitudes; Central Europe alone has 79 of the 129 glaciers at 40–50°N. Bands with fewer than 10 glaciers are indicative only. ELA also depends on continentality, which latitude alone does not capture.

## 8. Licences of the spectral and colour data

- **USGS splib07 (G2, G3):** public domain. The data release is DOI 10.5066/F7RR1WDJ. The report says some third-party items may be copyrighted.
- **ECOSTRESS / ASTER spectral library (G8):**
  - The site footer reads "Copyright © 2017, California Institute of Technology. ALL RIGHTS RESERVED".
  - Required credit line: "Reproduced from the ECOSTRESS Spectral Library through the courtesy of the Jet Propulsion Laboratory, California Institute of Technology…".
  - Users must cite Meerdink et al. 2019 (G9) and Baldridge et al. 2009 (G10).
  - Contents: over 3,400 spectra; download categories include All Rock (647 files), All Soil (120) and All Water (9).
  - Treat the library as **no redistribution without permission**. The Grok pass confirmed the same wording from a 2019 archived copy of the site.
- **WGMS FoG (G1):** "Open access under the requirement of correct citation".
- **UC Davis colour table (G6):** no licence stated. Ask before redistributing.

## 9. Not found / not checked against the primary source

1. **ECOSTRESS spectra were not downloaded.** The library is delivered through a cart/checkout order form, which I may not submit (executor rule), and direct file paths return 403/404. No ECOSTRESS reflectance values are given.
2. **No solar-weighted broadband albedo from the lab spectra:** the NREL ASTM G173 download failed (SSL). The §4 values are band means only.
3. **Fresh mid-latitude snow, firn, clean glacier ice:** no primary field albedo opened beyond G4 and G5. Grok also reported no labelled firn albedo.
4. **Granite and limestone field broadband albedo:** not found, by me or by Grok.
5. **Munsell dry vs moist value difference:** not checked. The NRCS Soil Survey Manual ch. 3 download failed (TLS error, then timed out).
6. **Roughness of sand, soil, desert pavement and lava (z0), and optical (Hapke) roughness:** not checked. See item 7.
7. **Grok-pass claims not checked against the primary source** (not used):
   - Albedo:
     - Tsvetsinskaya et al. 2002 MODIS white-sky albedo: dunes 0.426 ± 0.043; "Lithosols and rock: other" 0.302; volcanics and intrusives 0.174 (Wiley, blocked).
     - Shapotou dry dune sand 0.266 (ResearchGate).
     - Perovich 2002 and Grenfell & Perovich 2004 sea-ice albedo.
     - Bøggild's "clean drained ice 0.7" (not located in my text extract).
   - Wet vs dry soil:
     - Post et al. 2000: wet albedo 32–58% below dry, mean 45%; albedo = 0.069 × Munsell value − 0.114; dry Munsell value 10–36% lower when wet, mean 27%. SSSAJ is closed and there is no open copy.
     - Idso 1974/1975 (wet 0.14, dry 0.305; Arizona repository blocked).
     - Matthias 2000.
   - Roughness and snow line:
     - Gromke 2011 fresh-snow z0 0.24 ± 0.05 mm.
     - Marticorena 2006 desert z0 table (e.g. Lucerne playa 1.4×10⁻⁴ m; Golden Canyon fan 4.72×10⁻³ m; Gobabeb interdune 4×10⁻⁵ m). Wiley supplement blocked.
     - Xue 2002 Gobi z0 0.3–1.9 mm.
     - Greene et al. 2002 Table A1 modern tropical ELAs (e.g. Mt Kenya 4,796 m; Cordillera Blanca 4,950 m; Juneau 1,050 m). Wiley blocked.
     - Ohmura & Boettcher 2018 Alpine ELAs.
   - Rock forms:
     - Glazner's mean column width "60 ± 16 cm" (not located in the text I extracted).
     - Mihevc & Mihevc 2021 Slovenian dolines (average 9 m deep, 42 m across; up to 500/km²), known only from a later paper.
8. **Rock forms with no numbers found:** sea stacks and shore platforms (see F1 §10), tafoni and honeycomb weathering, hoodoo dimensions, granite tor and inselberg size statistics, karst tower height.
9. **G6 "freq" column** is undocumented. It may count map units or area rather than pedons.

## 10. Sources disagree

| Topic | Values | Comment |
|---|---|---|
| Brock z0 (Grok vs primary) | Grok reported "mean profile z0p 1.27 mm" on snow and 1.93 mm on ice. G18 Table 3 lists those as **ln(z0)**; the mean z0 values are **3.56 mm** (snow) and **6.89 mm** (ice) | Primary wins. Grok read the ln column as z0 |
| Brock table numbering | Journal typography (JoG 52(177), p.8): Arolla values with n are Table 3; literature ranges are Table 1a/1b; Table 2 is sampling dates; Table 4 is height-adjustment means only. The Cambridge Core PDF of the same article numbers these as Tables 4, 1–2, 3 and 5 respectively | Same numbers; different table numbers in the two typographies. This file uses the journal typography |
| Devils Postpile sides | 200-post sample: 6 sides 55%, 5 sides 37%, 7 sides 5% (G12) vs 940-polygon map: 53%, 27%, 18% (G13) | Different samples and methods; per Grok, Glazner argues field counts over-count pentagons |
| Arches span unit | "greater than 300 feet (97 m)" (G11) | 300 ft = 91 m. The page's conversion does not match its own figure |
| Snow albedo: field vs lab | Antarctic dry snow 0.80–0.85 broadband, about 0.98 visible (G5) vs melting snow 0.67–0.76 broadband (G4) vs lab melting-snow visible reflectance 0.50–0.83 (G2) | Different quantities (albedo vs reflectance) and snow states |
| Desert sand albedo (Grok-reported) | Field dry dune sand 0.266 (Shapotou) vs MODIS dunes 0.426 (Tsvetsinskaya) | Both unchecked (§9). Different deserts and measures |
| Wet-soil darkening (Grok-reported vs G2) | Post 2000: wet 32–58% darker (mean 45%, unchecked) vs G2 DWO sand: visible about 47% lower when wet (my calc) | Roughly consistent |

## 11. References (all accessed 2026-11-16)

Link check, 2026-11-16, curl -L with a browser user-agent: every URL below returned 200 (eScholarship landing page 202), except the G12 Wayback capture (timed out to curl; read via WebFetch).

- **G1** WGMS (2026) Fluctuations of Glaciers Database. World Glacier Monitoring Service, Zurich. DOI 10.5904/wgms-fog-2026-02-10.
  - https://doi.org/10.5904/wgms-fog-2026-02-10
  - Zip: https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip
- **G2** Kokaly R.F. et al. (2017) USGS Spectral Library Version 7 Data. USGS data release, DOI 10.5066/F7RR1WDJ.
  - https://doi.org/10.5066/F7RR1WDJ
  - ASCII zip: https://www.sciencebase.gov/catalog/file/get/586e8c88e4b0f5ce109fccae?name=ASCIIdata_splib07a.zip
- **G3** Kokaly R.F. et al. (2017) USGS Spectral Library Version 7. USGS Data Series 1035, DOI 10.3133/ds1035. https://pubs.usgs.gov/ds/1035/ds1035.pdf
- **G4** Bøggild C.E., Brandt R.E., Brown K.J., Warren S.G. (2010) J. Glaciol. 56(195):101–113, DOI 10.3189/002214310791190776. https://atmos.uw.edu/~sgw/PAPERS/2010_KPCL.pdf
- **G5** Grenfell T.C., Warren S.G., Mullen P.C. (1994) JGR 99(D9):18669–18684, DOI 10.1029/94JD01484. https://atmos.uw.edu/~sgw/PAPERS/1994_SnowAlbJGR.pdf
- **G6** UC Davis CSRL, unique moist colour LUT. https://casoilresource.lawr.ucdavis.edu/soil-properties/unique-moist-color-LUT.csv
- **G7** Beaudette D.E., Ferguson C., Nemecek J. (2019) The Geography of Soil Color. https://ncss-tech.github.io/AQP/presentations/beaudette-soil-color-2019-www.pdf
- **G8** ECOSTRESS Spectral Library (JPL/Caltech). https://speclib.jpl.nasa.gov/
- **G9** Meerdink S.K., Hook S.J., Roberts D.A., Abbott E.A. (2019) RSE 230:111196. https://doi.org/10.1016/j.rse.2019.05.015
- **G10** Baldridge A.M., Hook S.J., Grove C.I., Rivera G. (2009) RSE 113:711–715. https://doi.org/10.1016/j.rse.2008.11.007
- **G11** NPS, Arches NP, Geologic Formations. https://www.nps.gov/arch/learn/nature/geologicformations.htm
- **G12** Huber N.K., Eckhardt W.W., Devils Postpile Story (USGS/NPS web page dpgeol5), Wayback capture 2010-05-27. https://web.archive.org/web/20100527174149/http://www.nature.nps.gov/geology/usgsnps/depo/dpgeol5.html (timed out to curl; read via WebFetch)
- **G13** Glazner A.F. (2025) Columnar jointing in the Devils Postpile… UC eScholarship, DOI 10.83206/B1RP47.
  - https://escholarship.org/uc/item/06x2s49m
  - PDF: https://escholarship.org/content/qt06x2s49m/qt06x2s49m.pdf
- **G14** Goehring L., Morris S.W. (2005) Order and disorder in columnar joints. Europhys. Lett., DOI 10.1209/epl/i2004-10408-x. https://arxiv.org/pdf/cond-mat/0501015
- **G15** Mitchell K.J. (2010) MS thesis, University of Hawaiʻi at Mānoa. https://www.soest.hawaii.edu/earthsciences/wp-content/uploads/2025/09/MS_2010_Mitchell.pdf
- **G16** Collins B.D., Stock G.M. (2016) Rockfall triggering by cyclic thermal stressing of exfoliation fractures. Nat. Geosci. 9, Supplementary Information. https://static-content.springer.com/esm/art%3A10.1038%2Fngeo2686/MediaObjects/41561_2016_BFngeo2686_MOESM162_ESM.pdf
- **G17** Lončar N., Grcić I. (2022) Acta Carsologica 51(1), DOI 10.3986/ac.v51i1.10465. **CC BY-NC-ND 4.0** (non-commercial, no derivatives). https://ojs.zrc-sazu.si/carsologica/article/download/10465/10101
- **G18** Brock B.W., Willis I.C., Sharp M.J. (2006) J. Glaciol. 52(177):281–297, DOI 10.3189/172756506781828746. https://www.cambridge.org/core/services/aop-cambridge-core/content/view/30AB8A2DCFF90741FF302B4EB68D359B
- **L17, L18**: see F1 §12.
