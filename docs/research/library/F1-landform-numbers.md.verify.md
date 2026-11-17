# F1-landform-numbers.md: verification

Checked by: Grok Bot executor (adversarial source check). Access date for every URL below: **2026-11-17** (Australia/Sydney, AEDT).
File checked: `F1-landform-numbers.md`. The box was restored from a snapshot at 09:59 AEDT, so mtimes cannot be used to spot changes. Identified by size and sha256.
- At the start of this completing pass (10:39:06 AEDT): **35,670 bytes**, sha256 `d5baf38608a1af81a45d821a33cbdf0adb608a1c9fd0292e0d6006676d4043d6`.
- Re-hashed immediately before writing this file: **unchanged** (same size and sha256). The current file is larger than the 34,437-byte copy checked in the earlier pass because it already contains the 2026-11-17 “Fixes after verify” section (CETN wording).

## Summary

- **Scope (stated plainly):** This pass finishes the rows the earlier check never re-checked: L1–L7, L9–L18, and the §6.1 Grosse calculation (re-derived from the ESM XLS). Earlier-checked rows (L8, L19–L27, and the §10/§11 items that use them) are **kept below unchanged**, except that the two CETN FIX items were already applied in the file and are now counted as VERIFIED against the current wording.
- **Counts (all checked rows, current file):** VERIFIED 54 · FIX 5 · UNVERIFIABLE 1. (Earlier pass: 17 VERIFIED / 2 FIX / 0 UNVERIFIABLE across 19 rows. This pass adds 41 rows: 35 VERIFIED, 5 FIX, 1 UNVERIFIABLE; the 2 earlier FIX items move to VERIFIED because the current text matches the correction.)
- **Outstanding FIX (this pass):**
  1. **L5, avalanche slopes 29.5–33° and aprons 14–16°, and the 6° / 15° windward slopes, are not Merk / Great Sand Dunes.** They are from Lindsay (1973) on reversing dunes in Victoria Valley, Antarctica, cited on the same L5 page (book p.104). Merk is discussed immediately above, but without those numbers.
  2. **L11 coverage line:** the ESM latitudes are **59.361°N to 54.409°S**, not “about 60°N to 56°S”.
  3. **L17 Muav Limestone:** the form **is** stated: “forms cliffs at GRCA” (L17 p.56). Drop “form not stated in the L17 text read”.
  4. **L17 Hermit row:** Hermit is the “reddish-brown, slope-forming unit”. The Esplanade Sandstone is a **separate** unit (Supai Group, below Hermit), not a name for the Hermit slope.
- **UNVERIFIABLE:** the L11 abstract quote “large variation without clear separations”. The Springer HTML/PDF are bot-blocked; Crossref, OpenAlex and Semantic Scholar return no abstract text. The XLS calculation itself is verified.
- **Dead links:** none among the L1–L18 sources opened for this pass. Bot-blocks / stubs (not dead): L11 Springer article/PDF (HTML stub); L2 Elsevier DOI landing (the third-party PDF opened, HTTP 200); official USACE CEM 403 (the pdhonline copy opened, as the file already says); L15 record page returned gzip HTML, but the PDF opened (HTTP 200).
- **Earlier CETN FIX (applied in the file):** Eq. 4 gives A = 0.0970 for 0.2 mm sand; Eq. 2 gives ≈ 0.098. Current §9 and §11 wording is correct.

## Rows

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| §7 Fresh cinder cones (L8 eq. 1) | Hco = 0.179·Wco, N = 9, r = 0.98 | VERIFIED | p.143 Eq. 1; Table 2 0.179, r 0.985 | https://people.wou.edu/~taylors/g407/wood_1980.pdf |
| §7 Older cinder cones (L8 eqs. 2–3) | Tappan 0.125·Wco N = 24; Woodhouse 0.038·Wco N = 5 r = 0.90; boundaries > 0.15 / < 0.06 | VERIFIED | Page image p.143 shows "0.038Wco (N = 5; r = 0.90)". Table 2 0.038. Text "less than 0.06", "greater than 0.15" | same |
| §7 Crater width (L8) | Wcr = 0.40·Wco | VERIFIED | printed as "Wcr = 0.4 Wco" (page not recorded) | same |
| §7 Flank slopes by age (L8) | 30.8 ± 3.9 (N 7); 23.1 ± 2.0 (N 15); 14.1 ± 4.2 (N 5) | VERIFIED | OCR matches | same |
| §7 Young cones (L8) | 28–32°; "30° to 3?°" garbled | VERIFIED | OCR is garbled as "3%"; the ⚠ flag is right | same |
| §5 Rc/w (L19) | median 2.7, mean 3.1 (50 rivers); two-thirds 1.5–4.3; most curves 2–3 | VERIFIED | abstract p.769; p.774 | https://cpb-us-e1.wpmucdn.com/blogs.uoregon.edu/dist/5/10896/files/2015/04/Leopold_Wolman_1960-1wypmnk.pdf |
| §5 Wavelength fit (L19) | L = 10.9·w^1.01, L = 4.7·rm^0.98 (ft); "7 to 10 times the channel width" | VERIFIED | Table 1 p.773. Note: the "7 to 10 times" sentence is on p.772, not 773 (PDF p.1 is a GSA cover sheet) | same |
| §4 Basin length vs area (L20) | L = 1.78·A^0.49 (m); L = (3A)^0.5; > 11 orders of magnitude | VERIFIED | Eqs. 1–2 | https://pdodds.w3.uvm.edu/files/papers/others/1992/montgomery1992a.pdf |
| §4 Hack, large basins (L21) | L = 4.499·A^0.466; 65 basins; 21.77% vs 70.12%; 117.9% for 8 rivers > 1,000,000 mi² | VERIFIED | | https://pdodds.w3.uvm.edu/teaching/courses/2009-08UVM-300/docs/others/1972/mueller1972a.pdf |
| §4 Hack exponent, DEMs (L22) | h = 0.60; H = 0.92; sinuosity 1.15; 0.575; 0.52; Gray L ∝ A^0.568 | VERIFIED | p.3369; Gray on p.3367 | https://pdodds.w3.uvm.edu/files/papers/others/1996/rigon1996a.pdf |
| §6 Oceanic shields (L23) | < 5°; 10–12°; up to 40°; near 90°; 15 volcanoes | VERIFIED | abstract | https://www.soest.hawaii.edu/GG/FACULTY/ROWLAND/pdfs/Rowland_Garbeil_2000.pdf |
| §6 Composite volcanoes (L24) | slope 3–30, median 17°; H/WB 0.01–0.28, 0.12; WB 2–36, 8.8 km; H 100–2,500, 1,020 m | VERIFIED | abstract (~750 volcanoes) | https://kazan-g.sakura.ne.jp/iavcei2013/iavcei_hp/PDF/4A2_4H-O14.pdf |
| §6 Hawaii volcano average slopes (L27) | 3.3 / 5.4 / 6.6 / 7.0 / 11.3, no unit printed | VERIFIED | USGS record abstract; correct that no unit is printed | https://www.usgs.gov/publications/morphology-island-hawaii |
| §9 Selby classes (L25) | 100–91 / 90–71 / 70–51 / 50–26 / < 26; MP-24 modified 70–55, 54–20, < 20 | VERIFIED | MP-24 Table 1 (p.3) and Table 2; "Used with permission of the publisher, E. Schweizerbart" | https://www.geology.arkansas.gov/docs/pdf/publication/miscellaneous_pubs/MP-24.pdf |
| §9 Dean A from CETN (L26), current wording | A = 0.21·D^0.48; 0.2 mm → 0.0970; Eq. 2 with w = 2.4 cm/s → ≈ 0.098; printed "0.97" is a misprint; example A = 0.108 for 0.25 mm | VERIFIED | Earlier FIX, already applied in “Fixes after verify” and in the current §9/§11 rows. Arithmetic re-checked: 0.21 × 0.2^0.48 = 0.0970; 0.067 × 2.4^0.44 = 0.098 | https://erdc-library.erdc.dren.mil/server/api/core/bitstreams/81b728f8-6f0d-4ef8-e053-411ac80adeb3/content |
| §11 Dean A for 0.2 mm, current wording | 0.100 (CEM) vs 0.0970 (CETN grain-size formula, Eq. 4); Eq. 2 gives 0.098 | VERIFIED | Earlier FIX, already applied | same |
| §11 Selby class edges | Table 1 vs MP-24 Table 2 | VERIFIED | | L25 URL |
| §11 Meander wavelength coefficient (L19 part) | 10.9·w^1.01 ft; 7–10× width | VERIFIED | L4 half now also re-checked (this pass) | L19 URL |
| §10 partly-read sources kept out of tables | Grosse & Kervyn 2018, Hesp & Hastings 1998, Li 2001, Williams 1986 only in §10 | VERIFIED | grep of the whole file | — |
| §2 L1 typical dry sand | 34°; "no other specific values could be determined"; data widely scattered | VERIFIED | HYD-299 p.2 memo, 25 Oct 1950. Page images. US Gov public domain | https://www.usbr.gov/tsc/techreferences/hydraulics_lab/pubs/HYD/HYD-299.pdf |
| §2 L1 wet/submerged | sand reduced when submerged; gravel "practically no change" | VERIFIED | same memo | same |
| §2 L1 angular vs rounded | angular gravel 5–10° steeper | VERIFIED | same memo | same |
| §2 L1 Table 15 (Hanson) | 19–38 mm 32°/39°; 9.5–19 24°/37°; 4.7–9.5 25°/31°; 2.4–4.7 rounded 34°; 1.2–2.4 rounded 35°; 0.15–0.42 rounded 32° air, 27° submerged | VERIFIED | Page image of Table 15. The 32/39 etc. are the unstarred “Stacked in air” values (starred memo-of-7-18-49 alternates also exist: 35 and 37 for 19–38 mm). “27° in the submerged column” is “Stacked in air and submerged”; the “Stacked under H2O” cell for that size is 29° | same |
| §2 L2 sand, general | 30–35° | VERIFIED | Body: “for sand, it ranges from 30° to 35° [28,29], as reported in Table 2 [30]”. Table 2 itself has no generic 30–35 row (it lists dry 34, wet 45, water-filled 15–30) | https://www.willemwitteveen.com/wp-content/uploads/2020/06/Pdf-File-1.pdf |
| §2 L2 typical values | dry/wet/water-filled sand 34/45/15–30; crushed gravel 45; natural w/ sand 25–30; Earth 30–45; clay dry lump 25–40; granite 35–40; chalk 45; ashes 40; snow 38 | VERIFIED | Table 2, “Typical values of angle of repose [30]”. The table label is “Granite”, not “Granite (broken)” | same |
| §2 L2 flow classes | <30 / 30–38 / 38–45 / 45–55 / >55 | VERIFIED | Table 1, Carr classification: very free-flowing <30°; free-flowing 30–38°; fair to passable 38–45°; cohesive 45–55°; very cohesive (non-flowing) >55°. File shortens the middle labels | same |
| §2 L2 dynamic vs static | dynamic “usually at least 3 to 10° less” than static | VERIFIED | §2.1.3 | same |
| §3 L3 slope–area | slope ∝ A^(−0.6); also ∝ the 0.6 power of (grain size / drainage area) | VERIFIED | “slope is directly proportional to the 0.6 power of the ratio of grain size to drainage area”; conversely, for constant grain size, to the 0.6 power of drainage area | https://pubs.usgs.gov/pp/0294b/report.pdf |
| §4 L3 Hack eq. 3 | L = 1.4·A^0.6 (miles, mi²); coefficient averages ~1.4 but ranges 1–2.5; about 2.0 in sandstone | VERIFIED | Eq. (3); “ranges between 1 and 2.5”; “the average in sandstone is around 2.0”. OCR of the exponent is garbled (“1.4A\"L”) but the surrounding algebra is 0.6 | same |
| §4 L3 metric conversion | L(km) ≈ 1.27·A(km²)^0.6 | VERIFIED | Re-derived: 1.4 × (1/1.609344) × (1.609344²)^0.6 ≈ 1.2729, claimed 1.27 | — |
| §4 L3 Langbein check | 400 gauging-station values fit the same equation | VERIFIED | “plotting 400 similar measurements made by Langbein at gaging stations in the northeastern United States (Langbein, 1947, p. 145)” | L3 URL |
| §5 L4 wavelength / width | 7–12 in the data analysed | VERIFIED | abstract: “7 to 12 times the channel width”. Page images | https://pubs.usgs.gov/pp/0282b/report.pdf |
| §5 L4 ratio by size | about 7 for 1–10 ft streams, up to about 15 for rivers > 1,000 ft | VERIFIED | p.58 image | same |
| §5 L4 power law and metric | λ = 6.5·w^1.1 (ft); ≈ 7.3·w^1.1 (m) | VERIFIED | p.59: “exponent slightly larger than 1.0, specifically λ=6.5w^{1.1}”. The file’s OCR flag is fair. Metric re-derived: 6.5 × (3.28084)^0.1 ≈ 7.320, claimed 7.3 | same |
| §5 L4 European rule; riffles | λ ≈ 15·w; riffle spacing is half a wavelength and follows the same width relation | VERIFIED | p.58 rule-of-thumb; abstract: riffle spacing = half wavelength | same |
| §7 L5 Table 9 | none/no mound: sheet, stringer, climbing, falling; mound: dome; one: barchan, barchan ridge, transverse ridge, retention ridge; two: linear (seif or longitudinal), reversing; multiple: star, blowout, parabolic (may have only one) | VERIFIED | Table 9, book p.89 (PDF p.98). Caption is “according to form and to number of slipfaces” | https://pubs.usgs.gov/pp/1052/report.pdf |
| §7 L5 winds by type | prevailing single: barchan, barchanoid ridge, transverse, dome, some parabolic; bidirectional low angle: linear; nearly opposite: reversing; shifting around the compass: star | VERIFIED | Text immediately above Table 9, book pp.88–89 | same |
| §7 L5 angle of repose of dune sand | “about 34°” | VERIFIED | “wind at the angle of repose (about 34°)” | same |
| §7 L5 29.5–33° and 14–16° aprons, attributed to Merk / Great Sand Dunes | 29.5–33°, “normal for most dunes elsewhere”; aprons 14–16° | **FIX** | Those numbers are **Lindsay (1973)**, Victoria Valley, Antarctica, on the same page (book p.104). Merk’s Great Sand Dunes discussion is the preceding paragraph and does not give 29.5–33 or 14–16. **Correction:** attribute 29.5–33° and 14–16° aprons to Lindsay 1973 (Antarctica), cited in L5 | same |
| §7 L5 gentler slopes “same Great Sand Dunes description” | about 6° at the base, about 15° higher up, changing at the lower limit of surface moisture | **FIX** | Same page, same Lindsay 1973 Antarctica paragraph: windward slopes “about 6° in their basal parts” and “approximately 15° in the upper parts”, change coinciding with the lower limit of surface moisture. **Correction:** attribute to Lindsay 1973, not Great Sand Dunes | same |
| §7 L5 foreset dips | commonly 26–34°; 28–34°; 30–34°; Brazil retention ridges 34–39°, probably from damp sand | VERIFIED | 26–34, 28–34, 30–34 and “not uncommonly 30°–34°” appear in the trenches discussion. Brazil retention ridges: “dip downwind at very high angles (34°–39°) probably owing to high humidity at the time of deposition (Bigarella, 1972, p. 21)”. “Damp sand” is a fair paraphrase of high humidity | same |
| §4 L6 South Fork Eel River | Rb = 4.6, Ra = 4.9, Rl = 2.3 | VERIFIED | Fig. 5 caption (RLs = 2.3) and Region C of the table | https://efi.eng.uci.edu/papers/efg_111.pdf |
| §3/§4 L7 Horton, Hack, θ, Qw | Rb 3–5 mode 4; Ra 3–6 mode 4; Rl 1.5–3 mode 2; Qw = kq A^c, 0.7 ≤ c ≤ 1; A = ka x^h, ka = 6.69, h = 1.667; θ commonly 0.4–0.6; Hack l ∝ A^{0.52–0.67}, typically 0.6 | VERIFIED | MIT OCW lecture notes. Licence CC BY-NC-SA 4.0 (file said CC BY-NC-SA, version omitted but correct family) | https://ocw.mit.edu/courses/12-163-surface-processes-and-landscape-evolution-fall-2004/3cc219cc9c9c9e50cd72aabdd376503c_6_erosional_channel_nets.pdf |
| §4 L9 Polish Carpathians | mean Rb = 3.8, Ra = 4.8, Rl = 2.3; only 383 catchments (48%) obey the law of stream lengths | VERIFIED | Abstract and results: 383 (48%), mean RL = 2.3; RB 3.8; RA 4.8. Licence CC BY-NC-ND 3.0 | https://pressto.amu.edu.pl/index.php/qg/article/download/quageo-2015-0008/29456/72158 |
| §6 L10 b = 1 vs 2; 696 profiles | b = 1 is V (fluvial); b = 2 a parabola (Svensson 1959); literature 696 transect profiles | VERIFIED | §2 and “in the literature 696 profiles have been measured using the transect method” | https://etheses.whiterose.ac.uk/id/eprint/5452/1/Coles_2014.pdf |
| §6 L10 Table 6.2 | 8,005 segments, 334,068 transects; b mean 1.38, median 1.24, mode 1.13, min 0.003, max 3.56; form ratio mean 0.20, median 0.19, mode 0.20, range 0.0009–1.74; area mean 206,058 m², median 171,460 m²; 21,412 valleys in the parent set | VERIFIED | Table 6.2 and surrounding text (p.167 area). Process finding in the abstract left as the file states | same |
| §6.1 L11 Grosse calculation | All tabulated means ± sample SD, inclusive IQR (half-up), medians, including All-759 H/W 0.120 and basal width 8.8 km; n by type 488/80/48/11/76; basal width min 2.0 km | VERIFIED | Recomputed from the official ESM XLS (759 edifices). 22 “ND” values in Max. Avg. Slope were dropped (15/1/4 in some groups); the file does not mention the ND cells. Rounding of 8.790 → 8.8 km is correct | https://static-content.springer.com/esm/art%3A10.1007%2Fs00445-013-0784-4/MediaObjects/445_2013_784_MOESM1_ESM.xls |
| §6.1 L11 authors’ conclusion | “large variation without clear separations” (abstract) | **UNVERIFIABLE** | Springer article/PDF bot-blocked (HTML stub). Crossref licence is TDM-only (bronze OA, no CC). OpenAlex/Semantic Scholar return no abstract text | https://link.springer.com/article/10.1007/s00445-013-0784-4 |
| §6.1 L11 coverage | “latitudes about 60°N to 56°S” | **FIX** | XLS latitudes are **min −54.409, max 59.361**. **Correction:** “about 59.4°N to 54.4°S” (basal width ≥ 2 km is correct) | ESM XLS above |
| §6 L12 Hawaiian shields | Mauna Loa 3–6°, steepest average 8° at 3,300 m, summit rarely > 11°; Mauna Kea lower 3–6°, east 6–9°, steepest average 13° at 3,000 m; Kīlauea generally < 3°, Puna < 3°; average subaerial below 2,000 m is 4°; 500 m below sea level 13°; landslides head 14–17°, flanked up to 19° | VERIFIED | No text layer; read from page-1 images | https://pubs.usgs.gov/pp/1987/1350/pdf/chapters/pp1350_ch3.pdf |
| §6 L13 shield general | “slopes less than about 10 degrees” | VERIFIED | USGS page, HTTP 200 | https://pubs.usgs.gov/gip/volcus/page11.html |
| §9 L14 equilibrium profile | h = A·y^(2/3); n = 2/3 best overall fit to about 500 US East and Gulf profiles | VERIFIED | Eq. III-3-14 (“h'Ay3” in OCR) and “approximately 500 profiles from the east coast and Gulf shorelines” | https://www.pdhonline.com/courses/c773/Part-III-Chap_3entire.pdf |
| §9 L14 Table III-3-3 A values | 0.10 mm 0.063; 0.20 0.100; 0.30 0.125; 0.40 0.145; 0.50 0.161; 0.60 0.173; 0.70 0.185; 0.80 0.194; 0.90 0.202; 1.00 0.210; extra digits do not imply that accuracy | VERIFIED | Table III-3-3, every listed A matches the 0.00 column. Note (1) confirms the accuracy caveat | same |
| §9 L14 depth of closure and scarp | hc = 2.28·He − 68.5·He²/(g·Te²) (Hallermeier, III-3-9); hc ≈ 1.57·He (Birkemeier, Duck NC, III-3-12); He exceeded 12 h per year; scarp 1:1 | VERIFIED | Eqs. III-3-9 and III-3-12; “exceeded only 12 hr per year”; “adopting a 1:1 slope for this erosion scarp” | same |
| §9 L15 beach slope of equilibrium | fine sand 1 in 50 (≈1.1°); coarse 1 in 20 (≈2.9°); small pebbles 1 in 10 (≈5.7°); large pebbles 1 in 4 (≈14°) | VERIFIED | Page-image p.288. Degree conversions re-derived (1/50=1.15°≈1.1, etc.). Record page CC BY 4.0, authors retain copyright | https://icce-ojs-tamu.tdl.org/icce/article/download/1882/1155 |
| §9 L16 California cliff retreat | −0.3 ± 0.2 m/yr; 17,653 transects; 353 km; ~70 years; average total 17.7 m; maximum about 100 m (Fig. 40) | VERIFIED | Abstract/intro and state totals 17,653 / 353 / −0.3 / −17.7. Fig. 40 caption: “maximum cliff retreat (-100 m)” | https://pubs.usgs.gov/of/2007/1133/of2007-1133.pdf |
| §8 L17 thicknesses (Kaibab, Toroweap, Coconino, Hermit, Redwall members, Muav, Bright Angel, Tapeats) | 90–120 m (300–400 ft) Kaibab, rim; Brady Canyon up to 93 m / Woods Ranch about 60 m; Coconino to 210 m (700 ft), thins to almost nothing in the west; Hermit 49 m east to 244 m west; Redwall members 15–36, 30–43, 76–104, 0–30 m, sum 121–213 m; Muav 45–245 m (150–800 ft); Bright Angel 107–150 m (350–500 ft); Tapeats very thin or absent to 90 m (300 ft), of which 12–15 m is a transition zone | VERIFIED | All match L17. Redwall sum re-derived: 15+30+76+0=121; 36+43+104+30=213. Kaibab “forms the canyon rim”. Fossil Mountain “75 to 205 m (250 to 300 ft)” is already flagged in §11 as internally inconsistent and unused | https://irma.nps.gov/DataStore/DownloadFile/637945 |
| §8 L17 Muav form | “form not stated in the L17 text read” | **FIX** | L17 p.56: the Muav Limestone “forms cliffs at GRCA (Middleton and Elliott 2003)”. **Correction:** form = cliff | same |
| §8 L17 Hermit form | “slope (the Esplanade)” | **FIX** | L17: Hermit “is known as a reddish-brown, slope-forming unit”. The Esplanade Sandstone is a separate Supai Group unit below the Hermit, not a name for that slope. **Correction:** “slope-forming unit” (L17); keep Esplanade with L18 if at all | same |
| §8 L18 Paleozoic stack and rule | 2,400–5,000 ft; harder rocks form bold cliffs, shale slopes; downcutting fast, widening held by dry climate; Coconino/Redwall cliffs; Bright Angel/Tonto and Hermit/Esplanade slopes | VERIFIED | NPS Grand Canyon geologic-formations page, HTTP 200 | https://www.nps.gov/grca/learn/nature/geologicformations.htm |

## Sources

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| L1 | https://www.usbr.gov/tsc/techreferences/hydraulics_lab/pubs/HYD/HYD-299.pdf | y (200, PDF) | y (US Government, public domain) |
| L2 DOI | https://doi.org/10.1016/j.powtec.2018.02.003 | Elsevier linkinghub stub to curl | — |
| L2 copy read | https://www.willemwitteveen.com/wp-content/uploads/2020/06/Pdf-File-1.pdf | y (200, PDF) | Crossref lists both Elsevier TDM and a **CC BY 4.0** VoR entry. The file said “Crossref also lists a CC BY 4.0 entry, not confirmed”; the CC BY 4.0 Crossref entry is now confirmed. The copy actually read is still a third-party PDF |
| L3 | https://pubs.usgs.gov/pp/0294b/report.pdf | y | y (USGS, public domain) |
| L4 | https://pubs.usgs.gov/pp/0282b/report.pdf | y | y (USGS, public domain) |
| L5 | https://pubs.usgs.gov/pp/1052/report.pdf | y | y (USGS, public domain) |
| L6 | https://efi.eng.uci.edu/papers/efg_111.pdf | y | y (© AGU/Wiley; author-hosted copy) |
| L7 | https://ocw.mit.edu/courses/12-163-surface-processes-and-landscape-evolution-fall-2004/3cc219cc9c9c9e50cd72aabdd376503c_6_erosional_channel_nets.pdf | y | y (CC BY-NC-SA 4.0; file omitted the version) |
| L8 | https://people.wou.edu/~taylors/g407/wood_1980.pdf | y | y (© Elsevier; course copy) |
| L9 | https://pressto.amu.edu.pl/index.php/qg/article/download/quageo-2015-0008/29456/72158 | y | y (CC BY-NC-ND 3.0) |
| L10 thesis | https://etheses.whiterose.ac.uk/id/eprint/5452/1/Coles_2014.pdf | y | y (thesis; no open licence stated in the pages checked) |
| L11 article | https://link.springer.com/article/10.1007/s00445-013-0784-4 | n to this pass (bot HTML stub) | bronze OA, no CC in Crossref (TDM only); file’s “© Springer, free to read / no CC” is consistent |
| L11 ESM | https://static-content.springer.com/esm/art%3A10.1007%2Fs00445-013-0784-4/MediaObjects/445_2013_784_MOESM1_ESM.xls | y (200, real OLE/XLS, 2.6 MB) | same paper |
| L12 | https://pubs.usgs.gov/pp/1987/1350/pdf/chapters/pp1350_ch3.pdf | y | y (USGS, public domain) |
| L13 | https://pubs.usgs.gov/gip/volcus/page11.html | y | y (USGS, public domain) |
| L14 official | publications.usace.army.mil CEM PDF | n to curl (403), as the file says | US Government work, public domain |
| L14 copy read | https://www.pdhonline.com/courses/c773/Part-III-Chap_3entire.pdf | y (200) | y |
| L15 PDF | https://icce-ojs-tamu.tdl.org/icce/article/download/1882/1155 | y | y (CC BY 4.0; authors retain copyright) |
| L16 | https://pubs.usgs.gov/of/2007/1133/of2007-1133.pdf | y | y (USGS, public domain) |
| L17 | https://irma.nps.gov/DataStore/DownloadFile/637945 | y | y (NPS, public domain) |
| L18 | https://www.nps.gov/grca/learn/nature/geologicformations.htm | y | y (NPS, public domain) |
| L19–L27, L25, L26 | as in the earlier source table | y (earlier pass) | y (earlier pass) |

## Not found
- L11 abstract text (bot-blocked; no abstract in Crossref/OpenAlex/Semantic Scholar). The “large variation without clear separations” quote is therefore unverifiable in this pass.
- 22 ND cells in the Grosse “Max. Avg. Slope” column are not mentioned in F1. They were dropped for the IQR/SD (necessary, and the claimed figures still match).

## Sources disagree
- All earlier items (L19 page range, Wood OCR vs page image, CETN internal misprint, etc.) are kept.
- **L5 Merk vs Lindsay.** F1 attributes 29.5–33°, 14–16° aprons, and 6°/15° windward slopes to Merk / Great Sand Dunes. L5’s page gives those figures for Lindsay 1973 (Antarctica). See FIX above.
- **L17 Fossil Mountain Member** “75 to 205 m (250 to 300 ft)” remains internally inconsistent (205 m ≈ 670 ft). Already in F1 §11; the row is unused. Confirmed on this pass.
- **L2 Table 2 vs body on sand.** The body cites 30–35° “as reported in Table 2”; Table 2 has no generic 30–35 row. The file already lists 30–35 separately from the table values. Fine.
- **L11 latitude wording** vs the XLS (see FIX).

## Grok pass
- A new F1–F3 completing-pass run was launched at **10:41:47 AEDT** on 2026-11-17: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve --prompt-file /workspace/lv-scratch/f13/grok/prompt.txt`, cwd `/workspace/lv-scratch/f13/grok`. The 25-minute wait ends at about 11:06:47 AEDT. At **11:07 AEDT** the process was **still running** (about 25 minutes; TUI output only; no finished report), so this verification was finished without it and **does not use any Grok-only number**. Every figure above was opened in a primary PDF or official page (or, for L11 abstract, recorded as unobtainable).
- The earlier 09:01–09:05 AEDT F1–F3 run described in the previous verify file is unchanged and does not cover these leftover rows.
