# F1: the numbers that make land look right, landform by landform

Item F1 of `LIBRARY_RESEARCH_BRIEF.md` (§F Terrain). Compiled 2026-11-16 (Australia/Sydney). Private working file: nothing here was posted, published or committed.

## Fixes after verify (2026-11-17, Australia/Sydney, AEDT)

Applied from `F1-landform-numbers.md.verify.md`. Confirmed against the CETN II-32 and Wood 1980 page images before editing.

1. **CETN II-32 Dean A (L26):** the row is now labelled from the grain-size formula (Eq. 4, A = 0.21·D^0.48). For 0.2 mm sand that gives **0.0970**. The fall-velocity formula (Eq. 2, A = 0.067·w^0.44) with w = 2.4 cm/s gives **0.098**. The CETN p.2 prints "A = 0.97 m to the one-third power", a misprint (the later worked example on p.7 uses 0.097 from Eq. 4). The misprint and the two routes are under 'Sources disagree'.
2. **Wood 1980 Woodhouse coefficient:** already **0.038** (Eq. 3), cited to the p.143 page image. Reconfirmed on that page; no wording change.

Later the same day, after the leftover-row verify (F1 counts 54 verified / 5 fix / 1 unverifiable). Each item below was confirmed in the primary PDF or ESM spreadsheet before editing.

3. **L5 dune slopes:** the 29.5–33° avalanche (slipface) angles, the 14–16° aprons, and the windward slopes of about 6° at the base and about 15° higher up are from **Lindsay (1973)** on reversing dunes in Victoria Valley, Antarctica, cited in L5 at book p.104. They are not from Merk or Great Sand Dunes. Merk (1960) is discussed in the preceding paragraph on that page, on internal structures at Great Sand Dunes, without those numbers. Licence: US government public domain (USGS PP 1052).
4. **L11 coverage latitudes:** the ESM XLS latitudes run from **59.361°N to 54.409°S** (759 edifices, all with a latitude; basal width at least 2.0 km), not "about 60°N to 56°S".
5. **L17 Muav Limestone:** L17 book p.56 states that the Muav "forms cliffs at GRCA". The earlier "form not stated in the L17 text read" is dropped.
6. **L17 Hermit Formation:** L17 (same chapter, Hermit section) calls it a "reddish-brown, slope-forming unit". The Esplanade Sandstone is a separate Supai Group unit below the Hermit, not another name for that slope.
7. **L11 abstract quote:** the wording "large variation without clear separations" is now marked **UNVERIFIABLE**. Springer blocks bots, and the quote was not found in Crossref, OpenAlex or Semantic Scholar. The XLS calculations in §6.1 are unchanged.
8. **L8 Woodhouse note in §10:** the 0.038 coefficient is clear on the p.143 page image; only the young-cone slope range remains garbled.

## 0. Read this first

- **Source keys.** Every number has a key (L1, L2, …) with a full entry in §12. Each entry gives a URL and the access date, 2026-11-16. Keys T14, T17 and T18 are shared with `F2-erosion-algorithms.md` and are repeated in §12.
- **Kind of number.** Each row is tagged as one of:
  - **meas**: field or lab measurement, or a regression fitted to measurements.
  - **desc**: a range from a reference work or a review.
  - **calc**: a number I computed from a published dataset or formula. The method is stated in the row.
- **Verification marks.**
  - ✔: I read the number in the primary text, from the PDF via pdftotext or from the page.
  - **[OCR]**: the text came from a scanned PDF and its OCR layer. Garbled digits are flagged.
  - **[IMG]**: I read the number from the rendered page image because the PDF has no text layer.
  - ⚠: doubtful as printed. All ⚠ items are listed in §10.
- **Method.** WebSearch, WebFetch, curl and pdftotext. **A Grok live-search pass was also run** (Grok CLI, grok-4.6, reasoning effort xhigh, live web search; launched about 10:48 AEDT; the process exited before about 11:00 AEDT; raw output in `/tmp/grokrun/f1.out`). Every Grok claim used here was re-opened by me in the primary source and is marked "via Grok ✔". Grok claims I could not open are listed in §10, and conflicts are in §11. Scratch copies are in `/tmp/f1/` and `/tmp/f1g/`.
  - One dataset file was downloaded to scratch only, never to a repo: the Grosse et al. 2014 supplementary spreadsheet, used to compute §6.1.
- **Licences.**
  - Many sources here are **US Government works (public domain)**: USGS, USBR, NPS, and the USACE Coastal Engineering Manual.
  - Others carry Creative Commons licences. Some are **non-commercial** and are flagged NC: L7 MIT OCW (CC BY-NC-SA), L9 Bryndal (CC BY-NC-ND), L10 Coles thesis (CC BY-NC-ND), and T18 (CC BY-NC).
  - Everything else is publisher copyright. From those sources I use facts and numbers only, with no copied figures or wording.

## 1. Sources and licences

| Key | Source | Gives | Licence |
|---|---|---|---|
| L1 | USBR Hydraulic Laboratory (1950) HYD-299, memo to E.W. Lane on angles of repose | dry and submerged angles of repose for sand and gravel by size and shape | US Government work, public domain. Scanned [OCR] |
| L2 | Beakawi Al-Hashemi & Baghabra Al-Amoudi (2018) "A review on the angle of repose of granular materials", Powder Technology 330:397–417 | flowability classes; table of "typical values" | © Elsevier. Crossref also lists a CC BY 4.0 entry, not confirmed. The copy read is a ResearchGate-derived PDF on a third-party site. Its "typical values" table is itself compiled from another source (its ref. 30) |
| L3 | Hack (1957) USGS Professional Paper 294-B | Hack's law; slope–area relation | public domain (USGS) |
| L4 | Leopold & Wolman (1957) USGS Professional Paper 282-B | meander wavelength against width | public domain (USGS). Scanned [OCR] |
| L5 | McKee (ed.) (1979) USGS Professional Paper 1052, "A study of global sand seas" | dune classification by number of slipfaces; slipface and foreset angles | public domain (USGS) |
| L6 | Gangodagamage, Belmont & Foufoula-Georgiou (2011) WRR 47, W07508 | Horton ratios, South Fork Eel River | © AGU/Wiley. Author-hosted copy |
| L7 | Whipple (2004) MIT OCW 12.163 lecture notes, "Erosional channel networks" | typical Horton ratios; Hack exponent range; concavity range | **CC BY-NC-SA (non-commercial)** |
| L8 | Wood (1980) "Morphometric analysis of cinder cone degradation", J. Volcanol. Geotherm. Res. 8:137–160 | cinder cone H/W ratios and slopes by age | © Elsevier. Course-hosted copy (WOU) [OCR] |
| L9 | Bryndal (2015) Quaestiones Geographicae 34(1):85–98 | Horton ratios from Polish Carpathian catchments | **CC BY-NC-ND 3.0** |
| L10 | Coles (2014) PhD thesis, University of Sheffield, "The cross-sectional characteristics of glacial valleys and their spatial variability" | valley b-values and form ratios for 21,412 valleys | **CC BY-NC-ND 2.5** |
| L11 | Grosse, Euillades, Euillades & van Wyk de Vries (2014) "A global database of composite volcano morphometry", Bull. Volcanol. 76:784 (published online 2013) | morphometry of 759 volcanoes, SRTM 90 m. **I computed the statistics in §6.1 from its supplementary XLS** | © Springer (free to read; ESM downloadable). Numbers only |
| L12 | Mark & Moore (1987) "Slopes of the Hawaiian Ridge", USGS Professional Paper 1350, ch. 3 | Hawaiian shield slopes, above and below sea level | public domain (USGS). No text layer, so [IMG] |
| L13 | USGS (Tilling), "Volcanoes of the United States", general-interest page | shield volcano slopes less than about 10° | public domain (USGS) |
| L14 | USACE (2002) Coastal Engineering Manual EM 1110-2-1100 Part III ch. 3 | equilibrium beach profile h = A y^(2/3); A against grain size; depth of closure | US Government work, public domain. The official USACE URL returns 403 to curl, so I read a redistributed copy (pdhonline) |
| L15 | Hoyle & King (1957) "The origin and stability of beaches", Coastal Engineering Proceedings 6, ch. 17 | slope of equilibrium by beach material | **CC BY** (authors keep copyright) [IMG] |
| L16 | Hapke & Reid (2007) USGS Open-File Report 2007-1133 | California coastal cliff retreat rates | public domain (USGS) |
| L17 | Connors, Tweet & Santucci, "Chapter 3. Stratigraphy of Grand Canyon National Park" (NPS) | thickness of each stratum, and whether it forms cliff or slope | public domain (NPS) |
| L19 | Leopold & Wolman (1960) "River meanders", GSA Bulletin 71(6):769–794 (via Grok) | meander radius / width; wavelength against width and radius | © 1960 GSA. Course copy (Univ. of Oregon blog) |
| L20 | Montgomery & Dietrich (1992) "Channel initiation and the problem of landscape scale", Science 255:826–830 (via Grok) | basin length against area over 11 orders of magnitude | © AAAS. Teaching copy (UVM) |
| L21 | Mueller (1972) "Re-evaluation of the relationship of master streams and drainage basins", GSA Bulletin 83(11):3471–3474 (via Grok) | Hack exponent for large basins | © GSA. Teaching copy (UVM) |
| L22 | Rigon et al. (1996) "On Hack's law", WRR 32(11):3367–3374 (via Grok) | Hack exponent from DEMs; why it exceeds 0.5 | © AGU. Teaching copy (UVM) |
| L23 | Rowland & Garbeil (2000) "Slopes of oceanic basalt volcanoes", AGU Geophys. Monogr. 116:223–247 (via Grok) | slope classes on 15 oceanic shields | © 2000 AGU. Author copy (SOEST) |
| L24 | Grosse et al. (2013) IAVCEI 2013 Scientific Assembly abstract 4A2_4H-O14, composite-volcano database (via Grok) | the authors' own ranges and medians for the L11 database | © IAVCEI (abstract) |
| L25 | Selby (1980) rock mass strength classification, Z. Geomorph. 24:31–51, Table 1 as reprinted in Arkansas Geological Survey MP-24 (Baker 2011) (via Grok) | rock mass strength rating classes | Table © Schweizerbart, "used with permission" in MP-24; MP-24 is a state publication |
| L26 | USACE CETN II-32 (rev. 3/95) beach-fill volume note (via Grok) | Dean A from grain size (Eq. 4) and fall velocity (Eq. 2); worked values | US Government work |
| L27 | Moore & Mark (1992) "Morphology of the island of Hawaii", GSA Today; USGS Publications Warehouse abstract (via Grok) | average slope of each Hawaiian volcano | USGS record page; abstract text public |
| L18 | NPS, "Geologic Formations – Grand Canyon National Park" (web page) | which layers form cliffs and which form slopes | public domain (NPS) |
| T14 | Harel, Mudd & Attal (2016) Geomorphology 268:184–196 (author accepted manuscript) | measured m/n and n | © Elsevier (AAM) |
| T17 | Duvall, Kirby & Burbank (2004) JGR 109, F03002 | concavity range 0.30–0.60 | © AGU. Author copy |
| T18 | Gailleton et al. (2021) JGR Earth Surface 126, e2020JF006060 | concavity in 5,033 basins | **CC BY-NC 4.0** |

## 2. Angles of repose by material

| Material | Angle (spread) | Kind | Source |
|---|---|---|---|
| Dry sand, "typical" | 34° ("no other specific values could be determined"; published data widely scattered) | desc/meas | L1 summary ✔ [OCR] |
| Sand, wet or submerged | lower than dry ("a reduction … when submerged"); gravel shows "practically no change" | meas | L1 ✔ [OCR] |
| Gravel, angular vs rounded | angular gravel is **5–10° steeper** than rounded | meas | L1 ✔ [OCR] |
| Gravel 19–38 mm, rounded / angular (stacked in air) | 32° / 39° | meas | L1 Table 15 (Hanson data) ✔ [OCR] |
| Gravel 9.5–19 mm, rounded / angular | 24° / 37° | meas | L1 Table 15 ✔ [OCR] |
| Gravel 4.7–9.5 mm, rounded / angular | 25° / 31° | meas | L1 Table 15 ✔ [OCR] |
| Granules 2.4–4.7 mm, rounded | 34° | meas | L1 Table 15 ✔ [OCR] |
| Coarse sand 1.2–2.4 mm, rounded | 35° | meas | L1 Table 15 ✔ [OCR] |
| Fine–medium sand 0.15–0.42 mm, rounded | 32° in air; 27° in the submerged column | meas | L1 Table 15 ✔ [OCR] |
| Sand, general | 30–35° | desc | L2 ✔ |
| Sand (dry) / (wet) / (water-filled) | 34° / 45° / 15–30° | desc (compiled table) | L2 "Typical values" ✔ |
| Gravel, crushed stone / natural with sand | 45° / 25–30° | desc | L2 ✔ |
| Earth | 30–45° | desc | L2 ✔ |
| Clay, dry lump | 25–40° | desc | L2 ✔ |
| Granite (broken) | 35–40° | desc | L2 ✔ |
| Chalk | 45° | desc | L2 ✔ |
| Ashes | 40° | desc | L2 ✔ |
| Snow | 38° | desc | L2 ✔ |
| Flow classes | very free-flowing below 30°; free-flowing 30–38°; fair 38–45°; cohesive 45–55°; non-flowing above 55° | desc | L2 Table 1 ✔ |
| Dynamic vs static repose | dynamic is "usually at least 3 to 10° less" than static | desc | L2 ✔ |
| Dune slipface (dry sand avalanche) | see §7: 29.5–33°, about 34° | meas | L5 ✔ |

## 3. River long profiles and concavity

| Quantity | Value (spread) | Kind | Source |
|---|---|---|---|
| Slope against drainage area (Hack, for constant bed-material size) | slope ∝ A^(−0.6); also ∝ the 0.6 power of (grain size / drainage area) | meas | L3 ✔ |
| Concavity θ, median of 5,033 basins | 0.425 (IQR 0.225–0.575) | meas | T18 ✔ |
| Concavity m/n, from ¹⁰Be data in 59 zones | median 0.51 ± 0.14 (values above 0.65 not tested) | meas | T14 ✔ |
| Concavity, expected range for uniform uplift, rock and climate | 0.30–0.60 | desc (theory plus earlier studies) | T17 ✔ |
| Concavity, "commonly observed" | 0.4–0.6; can vary downstream | desc | L7 ✔ |
| Reference concavity used for steepness (ksn) | 0.45 | convention | T18 ✔ |
| Measured concavities, Santa Ynez Mountains, CA (by group) | channels in weak rock only: 0.48 ± 0.04 (low-uplift region, 8 streams) and 0.6 ± 0.09 (high-uplift region, 6 streams). Channels crossing a rock-strength transition: 0.92 ± 0.07 (35 streams). Single streams range from 0.37 to 1.6 | meas | T17 Tables 1 and 4 ✔ |
| Discharge against area | Qw = kq·A^c, 0.7 ≤ c ≤ 1 | desc | L7 ✔ |

For profile shape: steady-state channels follow S = ks·A^(−θ). Combined with Hack's law (§4), this gives the concave-up long profile.

## 4. Hack's law and Horton ratios

| Quantity | Value (spread) | Kind | Source |
|---|---|---|---|
| Hack's law (Virginia and Maryland) | L = 1.4·A^0.6 (L in miles, A in square miles); coefficient 1–2.5 between areas, about 2.0 in sandstone | meas | L3 eq. 3 ✔ |
| Same in metric (my conversion) | L(km) ≈ 1.27·A(km²)^0.6 | calc | from L3 |
| Check against other data | 400 gauging-station values (Langbein 1947) fit the same equation | meas | L3 ✔ |
| Hack exponent h, range | 0.52–0.67 (typically 0.6) | desc | L7 ✔ |
| Hack written as area against length | A = ka·x^h with ka = 6.69, h = 1.667 | desc | L7 ✔ |
| Bifurcation ratio Rb | 3–5, mode 4 | desc | L7 ✔ |
| Area ratio Ra | 3–6, mode 4 | desc | L7 ✔ |
| Length ratio Rl | 1.5–3, mode 2 | desc | L7 ✔ |
| South Fork Eel River (CA) | Rb = 4.6, Ra = 4.9, Rl = 2.3 | meas | L6 Fig. 5 ✔ |
| Hack's law, large basins (65 basins, Thames to Amazon) | L = 4.499·A^0.466 (miles, mi²). Mean prediction error 21.77% vs 70.12% for Hack's equation; Hack is 117.9% off on average for the 8 basins over 1,000,000 mi² | meas | L21 ✔ via Grok |
| Basin length (along valley axis to divide) vs area, field composite | L = 1.78·A^0.49 (metres); approximated by L = (3A)^0.5; the composite with large rivers spans more than 11 orders of magnitude of area | meas | L20 eqs. 1–2 ✔ via Grok |
| Hack exponent from DEMs (WV/KY basins) | h = 0.60 by direct fit, matching 0.60 from Hurst exponent H = 0.92 and sinuosity exponent 1.15; 0.575 if self-similar with no elongation; 0.52 if channels were not fractal. Gray (1961): L ∝ A^0.568 | meas | L22 ✔ via Grok |
| Polish Carpathians, many small catchments | mean Rb = 3.8, Ra = 4.8, Rl = 2.3. Only 383 catchments (48%) obey the law of stream lengths | meas | L9 abstract ✔ |

## 5. Meanders

| Quantity | Value (spread) | Kind | Source |
|---|---|---|---|
| Meander wavelength / bankfull width | 7–12 in the data analysed | meas | L4 abstract ✔ [OCR] |
| Ratio by size | about 7 for streams 1–10 ft wide, up to about 15 for rivers wider than 1,000 ft | meas | L4 p.58–59 ✔ [OCR] |
| Power law | λ = 6.5·w^1.1 (feet). The OCR reads "X=6. 5w1.'"; the exponent is described as "slightly larger than 1.0" | meas | L4 ✔ [OCR] ⚠ |
| Same in metres (my conversion) | λ ≈ 7.3·w^1.1 | calc | from L4 |
| Radius of curvature / width, Rc/w | median 2.7, mean 3.1 (50 rivers); two-thirds of cases 1.5–4.3; "most river curves" 2–3 | meas | L19 abstract and p.774 ✔ via Grok |
| Wavelength against width and radius (1960 fit, feet) | L = 10.9·w^1.01 and L = 4.7·rm^0.98; meander length "ranges from 7 to 10 times the channel width" | meas | L19 Table 1 / p.773 ✔ via Grok |
| European rule of thumb (quoted by L4) | λ ≈ 15·w | desc | L4 ✔ |
| Straight channels | riffle spacing (half a wavelength) follows the same width relation | meas | L4 ✔ |

## 6. Valley cross-sections (V against U)

| Quantity | Value (spread) | Kind | Source |
|---|---|---|---|
| Power-law flank fit y = a·x^b | b = 1 gives a V (fluvial); b = 2 gives a parabola (Svensson 1959) | desc | L10 §2 ✔ |
| b-value, glacial valleys inside last-glacial-maximum (LGM) ice limits: 8,005 segments, about 334,068 transects (Patagonia, Southern Alps NZ, Pyrenees) | mean 1.38; median 1.24; mode 1.13; min 0.003; max 3.56 | meas | L10 Table 6.2 ✔ |
| Form ratio (depth / top width), same set | mean 0.20; median 0.19; mode 0.20; range 0.0009–1.74 | meas | L10 Table 6.2 ✔ |
| Cross-sectional area, same set | mean 206,058 m²; median 171,460 m² | meas | L10 ✔ |
| Process finding | longer or more intense glaciation gives larger, wider parabolic sections, more than deeper ones | meas | L10 abstract ✔ |
| Literature total before L10 | 696 profiles measured by the transect method | desc | L10 ✔ |

## 6.1 Volcano slopes by type

**Composite-volcano database (L11).** Statistics computed by me from the supplementary XLS (759 edifices), grouped by the Smithsonian GVP type label.
- Labels in plural form or in brackets were kept as separate groups, as in the file.
- Coverage: SRTM 90 m, basal width at least 2 km, latitudes 59.361°N to 54.409°S (my reading of the ESM XLS).

| GVP type (n) | Mean flank slope °: mean ± sd (IQR) | Max avg slope °: median (IQR) | Height / basal width: median (IQR) | Basal width km: median |
|---|---|---|---|---|
| Stratovolcano (488) | 17.7 ± 4.7 (14.4–20.6) | 27.6 (22.0–32.1) | 0.132 (0.097–0.171) | 8.1 |
| "Stratovolcanoes" (80) | 16.4 ± 3.7 (13.8–18.3) | 22.4 (18.8–28.0) | 0.114 (0.082–0.141) | 10.2 |
| Complex volcano (48) | 15.3 ± 3.8 (13.0–17.9) | 21.0 (16.8–26.4) | 0.088 (0.073–0.132) | 9.5 |
| Compound volcano (11) | 17.0 ± 3.5 (14.9–19.1) | 22.4 (21.1–30.0) | 0.092 (0.067–0.124) | 13.1 |
| Shield volcano (76) | 8.9 ± 4.6 (5.8–10.6) | 12.0 (8.0–17.1) | 0.056 (0.040–0.072) | 15.8 |
| All 759 | 16.3 ± 5.2 (13.1–19.8) | 25.1 (18.8–31.2) | 0.120 (0.079–0.160) | 8.8 |

(calc; L11 ✔ for the raw values.) The authors' own conclusion is reported as most parameters showing "large variation without clear separations", with large overlap between GVP types (L11 abstract, **UNVERIFIABLE** this pass: Springer blocks bots, and the quote was not in Crossref, OpenAlex or Semantic Scholar).

**Other volcano sources.**

| Quantity | Value | Kind | Source |
|---|---|---|---|
| Shield volcanoes, general | slopes "less than about 10 degrees" | desc | L13 ✔ |
| Mauna Loa, subaerial | mostly 3–6°; steepest average 8° at 3,300 m; summit slopes rarely above 11° | meas | L12 [IMG] ✔ |
| Mauna Kea, subaerial | lower slopes 3–6°, steepening to 6–9° on the east side; steepest average 13° at 3,000 m | meas | L12 [IMG] ✔ |
| Kīlauea | gentle, generally less than 3°; Puna area less than 3° | meas | L12 [IMG] ✔ |
| Kīlauea and Mauna Loa, above and below sea level | average subaerial slope below 2,000 m elevation is 4°; 500 m below sea level the slope rises to 13°. Major submarine landslides head where regional slopes are 14–17°, flanked by slopes up to 19° | meas | L12 abstract [IMG] ✔ |
| Average slope of each Hawaii volcano (30 m DEM), youngest to oldest | Kīlauea 3.3, Mauna Loa 5.4, Hualālai 6.6, Mauna Kea 7.0, Kohala 11.3 (degrees implied; the abstract prints no unit). Steeper with age | meas | L27 abstract ✔ via Grok |
| Oceanic basaltic shields, 15 volcanoes (Hawaii, W Galápagos, Grand Comoro, Réunion) | ponded calderas, saddles and coastal plains under 5°; rift-zone axes and Galápagos summit platforms 10–12°; constructional slopes up to 40°; erosion, collapse and fault scarps locally near 90° | meas | L23 abstract ✔ via Grok |
| Composite volcanoes, authors' own summary of the L11 database | average slope 3–30°, median 17°; H/WB 0.01–0.28, median 0.12; basal width 2–36 km, median 8.8 km; height 100–2,500 m, median 1,020 m (range excludes outliers) | meas | L24 ✔ via Grok (agrees with my §6.1 calc: all-759 H/W median 0.120, basal width median 8.8 km) |
| Fresh cinder cones: height / basal width | Hco = 0.179·Wco (N = 9, r = 0.98; Sunset/Merriam age, under 0.1 Myr; matches Mauna Kea) | meas | L8 eq. 1 ✔ [OCR] |
| Older cinder cones | Hco = 0.125·Wco (Tappan age 0.2–0.7 Myr; N = 24); Woodhouse age 0.8–3.0 Myr: Hco = 0.038·Wco (N = 5, r = 0.90), read from the page image. Ratio boundaries: above 0.15 = Merriam/Sunset, below 0.06 = Woodhouse | meas | L8 eqs. 2–3, p.143 [IMG] ✔ |
| Fresh cones: crater width | Wcr = 0.40·Wco | meas | L8 ✔ [OCR] |
| Cinder cone flank slopes by age | Merriam 30.8 ± 3.9° (N = 7); Tappan 23.1 ± 2.0° (N = 15); older 14.1 ± 4.2° (N = 5) | meas | L8 ✔ [OCR] |
| Young cones | 28–32° (one field) and "30° to 3?°" (last digit garbled) ⚠ | meas | L8 [OCR] |

## 7. Dunes

| Quantity | Value (spread) | Kind | Source |
|---|---|---|---|
| Classification by number of slipfaces | **none, no mound:** sheet, stringer, climbing, falling. **None, mound:** dome. **One:** barchan, barchan ridge, transverse ridge, retention ridge. **Two:** linear (seif or longitudinal), reversing. **Multiple:** star, blowout, parabolic (may have only one) | desc | L5 Table 9 ✔ |
| Winds by type | prevailing single wind: barchan, barchanoid ridge, transverse, dome, some parabolic. Bidirectional at low angle: linear. Nearly opposite: reversing. Shifting around the compass: star | desc | L5 ✔ |
| Angle of repose of dune sand | "about 34°" | meas | L5 ✔ |
| Avalanche (slipface) slopes, reversing dunes in Victoria Valley, Antarctica (Lindsay 1973, cited in L5, book p.104) | 29.5–33°, "normal for most dunes elsewhere"; aprons below at 14–16° | meas | L5 ✔ |
| Foreset dips in trenches (record of slipfaces) | commonly 26–34°; 28–34°; 30–34°. Brazil retention-ridge dunes reach 34–39°, probably from damp sand | meas | L5 ✔ |
| Gentler (non-slipface) windward slopes, same Lindsay 1973 Victoria Valley description | about 6° at the base, about 15° higher up, changing at the lower limit of surface moisture | meas | L5 ✔ |

## 8. Cliffs and strata

**Grand Canyon as a worked example of hard and soft layers** (L17 ✔ thickness; L18 ✔ cliff or slope).

| Unit (top to bottom) | Thickness | Forms | Source |
|---|---|---|---|
| Kaibab Formation | 90–120 m (300–400 ft); forms the canyon rim | cliff and rim | L17 ✔ |
| Toroweap Formation (Brady Canyon Mbr / Woods Ranch Mbr) | up to 93 m / up to about 60 m | cliff (carbonate) / slope | L17 ✔ |
| Coconino Sandstone | thickens rapidly in the east to 210 m (700 ft), thins to almost nothing in the west | "bold cliffs" | L17 ✔; L18 ✔ |
| Hermit Formation | 49 m (east) to 244 m (west) | reddish-brown, slope-forming unit | L17 ✔ |
| Redwall Limestone (Mooney Falls Member) | 76 m (east) to 104 m (west); whole formation about 121–213 m (my sum of the 4 member ranges: Whitmore Wash 15–36, Thunder Springs 30–43, Mooney Falls 76–104, Horseshoe Mesa 0–30 m) | massive cliff | L17 ✔ (sum calc) |
| Muav Limestone | 45–245 m (150–800 ft) | forms cliffs at GRCA | L17 ✔ |
| Bright Angel Shale | 107–150 m (350–500 ft) | slope (Tonto Platform) | L17 ✔; L18 ✔ |
| Tapeats Sandstone | 0–90 m (300 ft), including a 12–15 m transition zone | cliff | L17 ✔ |
| Whole Paleozoic stack | 2,400–5,000 ft (about 730–1,520 m) | — | L18 ✔ |

Rule stated by NPS (L18 ✔): harder, erosion-resistant rocks (sandstone, limestone) erode into bold cliffs, while softer shale "melts" into slopes. The canyon is deep because downcutting is fast and widening is held in check by the dry climate.

**Talus angle under cliffs:** see §2 for broken rock (35–45°). No field talus-slope survey was opened by me (§10). The Grok pass reported Chandler 1973 (typical well-drained talus 35°) and Francou & Manté 1990 (mode 34–36°, 35 Alpine talus slopes), but I could not open either (§10).

**Rock mass strength (Selby 1980, L25 ✔ via Grok).** Total rating classes: very strong 100–91; strong 90–71; moderate 70–51; weak 50–26; very weak under 26. The rating is the sum of intact strength (Schmidt hammer), weathering, joint spacing, joint orientation, joint width, joint continuity and groundwater. MP-24's own *modified* model uses different class edges (moderate 70–55, weak 54–20, very weak under 20); see §11. The class-to-equilibrium-slope numbers were **not** found (§10).

## 9. Coasts

| Quantity | Value (spread) | Kind | Source |
|---|---|---|---|
| Equilibrium beach profile | h = A·y^(2/3) (Bruun 1954; Dean 1977). n = 2/3 gave the best overall fit to about 500 US East and Gulf coast profiles, though individual profiles spread widely | meas/desc | L14 eq. III-3-14 ✔ |
| A against median grain size D (A in m^(1/3)) | 0.10 mm: 0.063 · 0.20 mm: 0.100 · 0.30 mm: 0.125 · 0.40 mm: 0.145 · 0.50 mm: 0.161 · 0.60 mm: 0.173 · 0.70 mm: 0.185 · 0.80 mm: 0.194 · 0.90 mm: 0.202 · 1.00 mm: 0.210. The table's note says the extra digits do not imply that accuracy | meas (fitted) | L14 Table III-3-3 ✔ |
| Depth of closure | hc = 2.28·He − 68.5·He²/(g·Te²) (Hallermeier); simplified hc ≈ 1.57·He (Birkemeier, Duck NC). He is the wave height exceeded 12 h per year | meas | L14 eqs. III-3-9 and III-3-12 ✔ |
| Beach slope of equilibrium (guide values) | fine sand 1 in 50 (≈1.1°); coarse sand 1 in 20 (≈2.9°); small pebbles 1 in 10 (≈5.7°); large pebbles 1 in 4 (≈14°). Degrees are my conversion | desc | L15 p.288 [IMG] ✔ |
| CETN grain-size formula, Eq. 4 | A = 0.21·D^0.48 (D in mm, A in m^(1/3)). For 0.2 mm sand, A = 0.0970. The fall-velocity route (Eq. 2, A = 0.067·w^0.44) with w = 2.4 cm/s (Eq. 3) gives A ≈ 0.098. The CETN p.2 prints "A = 0.97 m to the one-third power", a misprint. Worked example (p.7) uses A = 0.108 for 0.25 mm from Eq. 4 | meas (fitted) | L26 pp.1–2, 7 [IMG] ✔ |
| Erosion scarp | 1:1 slope suggested for design | desc | L14 ✔ |
| California sea-cliff retreat | average −0.3 ± 0.2 m/yr (17,653 transects, 353 km, about 70 years); average total 17.7 m; maximum retreat about 100 m (Fig. 40 caption) | meas | L16 ✔ |

## 10. Not found / not checked against the primary source

1. **Meander radius / width:** now filled from Leopold & Wolman 1960 (L19, via Grok, ✔). Williams 1986 (geometric mean Rc/W 2.43, range 1.02–6.97; λ = 7.5·W^1.12 in metres, per Grok) could **not** be opened: the Wayback copy failed for me. Not used.
2. **Field talus slopes below cliffs:** not opened by me. Grok-reported, unchecked: Chandler 1973 (abstract: typical 35°; φ'cv about 39–40°); Francou & Manté 1990 (IRD copy; it showed a maintenance page to me). Not used.
3. **Selby equilibrium slope per class:** not found, by me or by Grok. Only the rating classes (L25) are given.
4. **L2's "Typical values" table** is itself compiled from another source (its ref. 30), so it is secondary. The copy read is a third-party PDF, not the publisher's.
5. **OCR items:**
   - L1 table digits are from OCR and were spot-checked against the summary text.
   - L4: the "6.5 w^1.1" exponent is garbled in OCR ⚠.
   - L8: the Woodhouse coefficient (0.038) is clear on the p.143 page image; only one young-cone slope range remains garbled ⚠.
6. **Volcano statistics in §6.1** are my calculation from the L11 spreadsheet, not numbers printed in the paper. The type labels are GVP labels and overlap (the authors' point).
   - Oceanic giant shields appear only through L12 (Hawaii).
   - Slopes of named pyroclastic or scoria cone fields other than Wood's were not added.
7. **Coast forms not covered:** spit and barrier dimensions, tombolos, rocky shore platforms, sea stacks.
8. **Dune dimensions** (barchan height-to-width scaling, star dune heights): McKee has data scattered across its chapters, but none was extracted.
9. **The USACE CEM** was read from a redistributed copy (pdhonline), because the official USACE URL returned 403 to curl. It is the full Chapter III-3, dated 30 Apr 2002.
10. **Glacial valley b-values** come from one thesis (L10, three regions). Grok reported Graf 1970 Table 2 (b 1.67 / 1.81 / 1.84 and FR 0.304 / 0.445 / 0.393 by valley order). The USC Scholar Commons copy shows a Cloudflare challenge to curl, so **not checked**. Not used.
11. **Other Grok-pass claims not checked against the primary source** (not used):
   - Grosse & Kervyn 2018 shields: median slope 6.4°, median H/WB 0.05; about 12° and 0.10 as the shield/composite cut. These came from search snippets.
   - Hesp & Hastings 1998: barchan height ≈ width/10; side slopes about 11° (abstract only).
   - Li et al. 2001: mean b = 1.87, depth/width 0.14–0.32 (quoted by a later paper).
   - Hey 1976: 2πW / 4πW (secondary, via Williams and NRCS NEH 654).
   - Rinaldo 1998 Hack range 0.53–0.60.
   - Wood 1980a abstract: 910 cones, median basal width 0.8 km.

12. **L11 abstract wording** "large variation without clear separations": **UNVERIFIABLE** this pass. The Springer article and PDF are bot-blocked; Crossref, OpenAlex and Semantic Scholar returned no abstract text. The §6.1 statistics computed from the ESM XLS are unaffected.

## 11. Sources disagree

| Topic | Values | Comment |
|---|---|---|
| Wet sand repose | 45° (L2 "Sand (wet)") vs lower when submerged (L1) | Damp (cohesive) sand is not the same as submerged sand. L2 also lists water-filled sand at 15–30° |
| Crushed vs natural gravel | 45° crushed / 25–30° natural with sand (L2) vs 31–39° angular and 24–34° rounded by size (L1) | Measured lab values (L1) sit between the two compiled values |
| Concavity centre | 0.425 (T18) vs 0.51 (T14) vs 0.45 convention | Different methods and data |
| Meander ratio | 7–12 (L4 abstract) vs 7–15 (L4 text, by river size) vs 15 (European rule of thumb) | The ratio rises with river size because the exponent is 1.1 |
| Glacial U-shape | theory parabola b = 2 vs measured median b = 1.24 inside LGM limits (L10) | Real glacial valleys are less parabolic than the textbook shape |
| Large-pebble beach slope | 1 in 4 on the page image (L15) vs "1:5" in a web-search summary | 1 in 4 read directly from the page |
| Woodhouse cinder-cone coefficient (Grok vs primary) | Grok reported Hco = 0.058·Wco. The page image (L8 p.143) reads 0.038, consistent with the paper's own "less than 0.06" boundary | Primary wins: **0.038** [IMG] |
| Hack exponent by basin size | 0.6 (Hack, L3); 0.568 (Gray, via L22); 0.60 (DEM, L22); 0.52–0.67 (L7) vs 0.466 for large basins (L21) vs 0.49 / 0.5 for basin-axis length (L20) | Not the same L: channel length vs valley-axis length. Large basins widen faster than they lengthen |
| Meander wavelength coefficient | λ = 6.5·w^1.1 ft (L4, 1957, OCR) vs L = 10.9·w^1.01 ft (L19, 1960) | Different data and exponents. Same order: 7–10× width (L19) vs 7–12× (L4) |
| Dean A for 0.2 mm sand | 0.100 (CEM table, L14) vs 0.0970 (CETN grain-size formula, Eq. 4, L26). The fall-velocity formula (Eq. 2) gives 0.098 | Eq. 4 is 3% from 0.100; Eq. 2 is 2% from 0.100. Same underlying Dean/Moore fit |
| CETN printed A for 0.2 mm sand | CETN II-32 p.2 prints "A = 0.97 m to the one-third power"; Eq. 4 gives 0.0970, and the p.7 worked example uses 0.097 from Eq. 4 | Misprint in the source |
| Selby class edges | original: moderate 70–51, weak 50–26, very weak < 26 (L25 Table 1) vs MP-24 modified: 70–55, 54–20, < 20 (MP-24 Table 2) | Use Table 1 for Selby's own classes |
| Toroweap Fossil Mountain Mbr thickness | "75 to 205 m (250 to 300 ft)" in L17 | Internally inconsistent: 205 m ≈ 670 ft. Row not used |
| L5 dune slope figures | Earlier draft attributed 29.5–33°, 14–16° aprons and 6°/15° windward slopes to Merk / Great Sand Dunes | L5 book p.104 gives those figures for Lindsay (1973), Victoria Valley, Antarctica. Merk (1960) is the preceding paragraph and does not print them |
| Hermit slope vs Esplanade | L18 says softer layers melt into slopes like "the Esplanade (Hermit Shale)" | L17 treats the Esplanade Sandstone as a separate Supai Group unit below the Hermit. The Hermit itself is the "reddish-brown, slope-forming unit" (L17). This file follows L17 |

## 12. References (all accessed 2026-11-16)

Link check, 2026-11-16, curl -L: 200 for every URL below unless noted.

- **L1** USBR (1950) HYD-299, Angle of repose of granular materials (memo to E.W. Lane, 25 Oct 1950). https://www.usbr.gov/tsc/techreferences/hydraulics_lab/pubs/HYD/HYD-299.pdf. Downloaded successfully at about 10:30 AEDT; a later recheck got a connection reset (curl 56).
- **L2** Beakawi Al-Hashemi H.M., Baghabra Al-Amoudi O.S. (2018) Powder Technology 330:397–417 (volume and pages from the running head of the copy). https://doi.org/10.1016/j.powtec.2018.02.003. Copy read: https://www.willemwitteveen.com/wp-content/uploads/2020/06/Pdf-File-1.pdf.
- **L3** Hack J.T. (1957) USGS PP 294-B. https://pubs.usgs.gov/pp/0294b/report.pdf (DOI 10.3133/pp294b)
- **L4** Leopold L.B., Wolman M.G. (1957) USGS PP 282-B. https://pubs.usgs.gov/pp/0282b/report.pdf (DOI 10.3133/pp282b)
- **L5** McKee E.D. (ed.) (1979) USGS PP 1052. https://pubs.usgs.gov/pp/1052/report.pdf (DOI 10.3133/pp1052)
- **L6** Gangodagamage C., Belmont P., Foufoula-Georgiou E. (2011) WRR 47, W07508, DOI 10.1029/2010WR009252. https://efi.eng.uci.edu/papers/efg_111.pdf
- **L7** Whipple K. (2004) MIT OCW 12.163, Erosional channel networks. https://ocw.mit.edu/courses/12-163-surface-processes-and-landscape-evolution-fall-2004/3cc219cc9c9c9e50cd72aabdd376503c_6_erosional_channel_nets.pdf
- **L8** Wood C.A. (1980) JVGR 8:137–160, DOI 10.1016/0377-0273(80)90101-8. https://people.wou.edu/~taylors/g407/wood_1980.pdf
- **L9** Bryndal T. (2015) Quaestiones Geographicae 34(1):85–98, DOI 10.1515/quageo-2015-0008. https://pressto.amu.edu.pl/index.php/qg/article/download/quageo-2015-0008/29456/72158
- **L10** Coles R.J. (2014) PhD thesis, University of Sheffield. https://etheses.whiterose.ac.uk/id/eprint/5452/1/Coles_2014.pdf (record: https://etheses.whiterose.ac.uk/id/eprint/5452/)
- **L11** Grosse P., Euillades P.A., Euillades L.D., van Wyk de Vries B. (2014) Bull. Volcanol. 76:784, DOI 10.1007/s00445-013-0784-4.
  - Article: https://link.springer.com/article/10.1007/s00445-013-0784-4
  - ESM: https://static-content.springer.com/esm/art%3A10.1007%2Fs00445-013-0784-4/MediaObjects/445_2013_784_MOESM1_ESM.xls
- **L12** Mark R.K., Moore J.G. (1987) Slopes of the Hawaiian Ridge. USGS PP 1350 ch. 3. https://pubs.usgs.gov/pp/1987/1350/pdf/chapters/pp1350_ch3.pdf
- **L13** USGS, Volcanoes of the United States (shield volcanoes page). https://pubs.usgs.gov/gip/volcus/page11.html
- **L14** USACE (2002) Coastal Engineering Manual, EM 1110-2-1100 Part III ch. 3 (30 Apr 2002). Copy read: https://www.pdhonline.com/courses/c773/Part-III-Chap_3entire.pdf. The official publications.usace.army.mil PDF returns 403 to curl.
- **L15** Hoyle J.W., King G.T. (1957) Coastal Engineering Proceedings 6, ch. 17, DOI 10.9753/icce.v6.17.
  - Record: https://icce-ojs-tamu.tdl.org/icce/article/view/1882
  - PDF: https://icce-ojs-tamu.tdl.org/icce/article/download/1882/1155
- **L16** Hapke C.J., Reid D. (2007) USGS OFR 2007-1133. https://pubs.usgs.gov/of/2007/1133/of2007-1133.pdf
- **L17** Connors T.B., Tweet J.S., Santucci V.L., Chapter 3. Stratigraphy of Grand Canyon National Park (NPS). https://irma.nps.gov/DataStore/DownloadFile/637945
- **L18** NPS, Geologic Formations – Grand Canyon National Park. https://www.nps.gov/grca/learn/nature/geologicformations.htm
- **L19** Leopold L.B., Wolman M.G. (1960) River meanders. GSA Bull. 71(6):769–794, DOI 10.1130/0016-7606(1960)71[769:RM]2.0.CO;2. https://cpb-us-e1.wpmucdn.com/blogs.uoregon.edu/dist/5/10896/files/2015/04/Leopold_Wolman_1960-1wypmnk.pdf
- **L20** Montgomery D.R., Dietrich W.E. (1992) Science 255:826–830, DOI 10.1126/science.255.5046.826. https://pdodds.w3.uvm.edu/files/papers/others/1992/montgomery1992a.pdf
- **L21** Mueller J.E. (1972) GSA Bull. 83(11):3471–3474. https://pdodds.w3.uvm.edu/teaching/courses/2009-08UVM-300/docs/others/1972/mueller1972a.pdf
- **L22** Rigon R. et al. (1996) On Hack's law. WRR 32(11):3367–3374, DOI 10.1029/96WR02397. https://pdodds.w3.uvm.edu/files/papers/others/1996/rigon1996a.pdf
- **L23** Rowland S.K., Garbeil H. (2000) Slopes of oceanic basalt volcanoes. AGU Geophys. Monogr. 116:223–247. https://www.soest.hawaii.edu/GG/FACULTY/ROWLAND/pdfs/Rowland_Garbeil_2000.pdf
- **L24** Grosse P. et al. (2013) IAVCEI Scientific Assembly abstract 4A2_4H-O14. https://kazan-g.sakura.ne.jp/iavcei2013/iavcei_hp/PDF/4A2_4H-O14.pdf
- **L25** Selby M.J. (1980) Z. Geomorph. 24(1):31–51, Table 1, as reprinted in Baker C. (2011) Arkansas Geological Survey MP-24. https://www.geology.arkansas.gov/docs/pdf/publication/miscellaneous_pubs/MP-24.pdf
- **L26** USACE CETN II-32 (rev. 3/95). https://erdc-library.erdc.dren.mil/server/api/core/bitstreams/81b728f8-6f0d-4ef8-e053-411ac80adeb3/content
- **L27** Moore J.G., Mark R.K. (1992) Morphology of the island of Hawaii. GSA Today. Abstract: https://www.usgs.gov/publications/morphology-island-hawaii (403 to curl; read via WebFetch)
- **T14** Harel M.-A., Mudd S.M., Attal M. (2016) Geomorphology 268:184–196. AAM: https://www.pure.ed.ac.uk/ws/files/25670599/25669129._Harel_et_al_2016_author_accepted_manuscript_002_.pdf
- **T17** Duvall A., Kirby E., Burbank D. (2004) JGR 109, F03002. https://burbank.faculty.geol.ucsb.edu/Site/Publications_files/Duvall%20Ynez%20channels%20JGR.pdf
- **T18** Gailleton B. et al. (2021) JGR Earth Surface 126, e2020JF006060. https://eprints.gla.ac.uk/251538/1/251538.pdf
