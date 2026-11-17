# G2: rivers, lakes and waterfalls — hydraulic geometry, flow, jumps, channel types, falls

Item G2 of `LIBRARY_RESEARCH_BRIEF.md` (§G Water). Compiled 2026-11-17 (Australia/Sydney). Private working file: nothing here was posted, published or committed.

## Fixes after verify (2026-11-17, Australia/Sydney, AEDT)

Applied from `G2-rivers-lakes-falls.md.verify.md`. Page numbers were confirmed in the HEC-RAS 5.0, Leopold & Maddock, and Peterka PDFs before editing. The official Montgomery & Buffington 1993 URL was fetched (HTTP 200).

1. **HEC-RAS 1.486:** eqs. (2-4)–(2-5) are on printed **p. 2-4**, not p. 2-3.
2. **Leopold & Maddock:** "b = 0.5, a typical value for many rivers" and the constructed m = 0 example at the 50% frequency are on **p. 17**, not p. 18. The downstream means on p. 26 are unchanged.
3. **Peterka:** "about 125 tests… six test flumes… discharges of 1 to 28 c.f.s." is on printed **p. 5**. The sequent-depth formula and the agreement statement stay on p. 7.
4. **Montgomery & Buffington 1993 official URL:** https://dnr.wa.gov/sites/default/files/2025-05/fp_tfw_sh10_93_002.pdf (same 1,766,212-byte file as the local copy).

## 0. Read this first

- **Source keys.** Every number has a key (R1, R2, …) with a full entry in §11. Each entry gives a URL and the access date, 2026-11-17.
- **Kind of number.** **meas**, **desc**, or **calc**, as in `G1-ocean-waves.md`.
- **Verification marks.** ✔ primary text; **[OCR]**; **[IMG]** page image; ⚠ doubtful (listed in §9).
- **Method.** WebSearch, WebFetch, curl, pdftotext and page images. Scratch copies are in `/tmp/g2/` (never in a repo). **A Grok live-search pass was run** on 16 Nov 2026 (Grok CLI, grok-4.6, reasoning effort xhigh, live web search; prompt `/tmp/grokrun/g2.txt`; the process exited the same morning; raw output `/tmp/grokrun/g2.out`). No second pass was run after the 17 Nov reset, to keep usage lean. Every Grok claim used here was re-opened by me in the primary source and is marked "via Grok ✔" only where that applies. Grok thought the official USBR EM 25 PDF was blocked; a copy was already on disk and I read it. Claims I could not open are in §9. Conflicts are in §10.
- **Licences.** USGS Professional Paper 252, USGS Water-Supply Paper 2339, USGS Bulletin 306 and USBR Engineering Monograph 25 are US Government works (public domain). The 1993 Montgomery & Buffington TFW report is a Washington State contract report (see R4). HEC-RAS 5.0 is a USACE manual (US Government work). Game talks are © the studio or the conference; facts only.

## 1. Sources and licences

| Key | Source | Gives | Licence |
|---|---|---|---|
| R1 | Arcement & Schneider (1989) USGS Water-Supply Paper 2339 | Manning formula in US customary units, eq. (1); pointers to Chow and Barnes for n | US Government work, public domain |
| R2 | Leopold & Maddock (1953) USGS Professional Paper 252 | At-a-station and downstream hydraulic-geometry exponents | US Government work, public domain. Scanned [OCR], key values checked [IMG]/text |
| R3 | Peterka, *Hydraulic Design of Stilling Basins and Energy Dissipators*, USBR Engineering Monograph 25 (revised printings through 1984/1990 in the copy read) | Sequent-depth formula; jump forms by incoming Froude number | US Government work, public domain. Scanned [IMG] |
| R4 | Montgomery & Buffington (1993) TFW-SH10-93-002 (also styled TFW-SI-110-93-002 on the title page), Washington State Timber/Fish/Wildlife | Alluvial channel types and preliminary Pacific-Northwest slope bins | Washington State contract report. Official PDF: https://dnr.wa.gov/sites/default/files/2025-05/fp_tfw_sh10_93_002.pdf. Copy also on disk |
| R5 | Gilbert (1907) USGS Bulletin 306, *Rate of recession of Niagara Falls* | Horseshoe and American Fall recession, 1842–1905 / 1827–1905 | US Government work, public domain |
| R6 | Brunner (2016) HEC-RAS Hydraulic Reference Manual, Version 5.0, December 2015 / 2016 | Manning conveyance, English units, eq. (2-5) | US Government work (USACE) |

## 2. Manning equation and n

| Quantity | Value | Kind | Source |
|---|---|---|---|
| US customary form | V = (1.486 / n) R^{2/3} S_e^{1/2}. V in ft s⁻¹, R in ft, S_e energy-grade-line slope (ft/ft), n Manning coefficient | desc | R1 eq. (1), printed p. 1 [IMG] ✔. Same 1.486 in the conveyance K = (1.486 / n) A R^{2/3}, eq. (2) |
| Same formula in HEC-RAS | Q = K S_f^{1/2}, K = (1.486 / n) A R^{2/3} (English units) | desc | R6 eqs. (2-4)–(2-5), p. 2-4 ✔ |
| Origin of 1.486 | (1 m / 1 ft)^{1/3} = 3.28084^{1/3} = 1.4859, printed 1.486 | calc | from the foot–metre ratio. Not derived in R1's opening page |
| SI companion | V = (1 / n) R^{2/3} S^{1/2} with R in metres, same numerical n, is the usual metric form. R1 prints only the US customary equation | desc | SI form not printed on the R1 page I opened. Some handbooks round the customary factor to 1.49; R1 prints 1.486 (0.27% apart) |

R1 points to Chow (1959), Henderson (1966) and Streeter (1971) for tabulated n, and to Barnes (1967) for photographs and measured n of natural channels (R1 p. 1 ✔). I did not open Chow 1959 (copyrighted) or Barnes 1967 this pass: the USGS WSP 1849 URL returned a 404 page. Those min/normal/max tables are therefore not copied here. Verified n in R1 is selected by evaluating roughness factors (Cowan 1956 procedure, described in the same report); I have not extracted R1's worked examples.

## 3. Hydraulic geometry (Leopold & Maddock 1953)

Width, mean depth and mean velocity are written as power functions of discharge: w = a Q^b, d = c Q^f, v = k Q^m, with b + f + m = 1 because Q = w d v (R2 eqs. (1)–(3), discussed p. 8–9 ✔).

| Quantity | Value (spread) | Kind | Source |
|---|---|---|---|
| At-a-station means | b = 0.26, f = 0.40, m = 0.34 | meas | R2 p. 9 ✔. Average of 20 river cross sections in the Great Plains and the Southwest. "Biased toward semiarid conditions." Lines drawn by eye through current-meter plots; "no statistical-fitting procedure was used" |
| Downstream means, at equal frequency (mean annual discharge) | b = 0.5, f = 0.4, m = 0.1 | meas | R2 p. 26 summary ✔, and the graph discussion on p. 17 (b = 0.5 "a typical value for many rivers") and p. 26 (downstream width slope 0.5, depth 0.4 both ways, velocity m = 0.1 downstream and 0.34 at a station) |
| Check on the averages | 0.26 + 0.40 + 0.34 = 1; 0.5 + 0.4 + 0.1 = 1 | calc | from R2 |
| Downstream velocity | At equal frequency, mean velocity typically increases slowly downstream (m = 0.1), or in a constructed example can be unchanged (m = 0 at the 50% frequency if width takes all of the downstream increase). It is not a universal slowing | meas / desc | R2 p. 17, 26 ✔ |

The 1953 averages are means from a small, regionally biased US set, not universal constants. Later worldwide compilations (Park 1977 and others) were not opened in the original papers this pass; they are flagged in §9. Grok correctly noted that the scatter, not the 1953 means, is the later story.

## 4. Hydraulic jumps and standing waves

Peterka (R3), stilling-basin design on a horizontal apron. Incoming Froude number F₁ = V₁ / √(g D₁) (R3 eq. (1), printed p. 6 ✔). Sequent (conjugate) depths satisfy the momentum balance; the ratio D₂/D₁ is a function of F₁ only (R3 eq. (3), p. 7 ✔). I read the typeset page: the standard form is D₂/D₁ = −1/2 + √(1/4 + 2 F₁²), equivalently ½(√(1+8 F₁²) − 1). Experimental points agreed with the formula "over the entire range" (R3 p. 7 ✔). Those points came from about 125 tests, six flumes, discharges 1–28 cfs (R3 printed p. 5 ✔).

Jump *forms* as catalogued by Peterka, Figure 9 and the facing text (printed pp. 15–16 [IMG] ✔). The limits "are not definite values but overlap somewhat depending on local factors."

| Incoming F₁ | Peterka's name (Fig. 9) | What he describes | Kind | Source |
|---|---|---|---|---|
| 1 | (critical) | Jump cannot form | desc | R3 p. 15 ✔ |
| 1.0–1.7 | slight ruffle | Only a slight difference in conjugate depths | meas / desc | R3 p. 15 ✔ |
| 1.7–2.5 | A, "Pre-jump — very low energy loss" | Small surface rollers appear as F₁ approaches 1.7 and intensify up to about 2.5; water surface fairly smooth; energy loss less than 20% (Fig. 8) | meas / desc | R3 p. 16 [IMG] ✔ |
| 2.5–4.5 | B, "Transition — rough water surface" | Pulsating action; the entering jet oscillates from bottom to surface; each oscillation can throw a large irregular wave downstream | meas / desc | R3 p. 16 [IMG] ✔ |
| 4.5–9.0 | C, "range of good jumps — least affected by tail-water variations" | Well-stabilized jump; surface roller and the point where the jet leaves the floor are in nearly the same vertical plane; energy absorption 45–70% | meas / desc | R3 p. 16 [IMG] ✔ |
| 9.0 upward | D, "effective but rough" | High incoming velocity, large sequent-depth ratio; slugs of water roll down the front face; downstream waves; energy dissipation high, may reach 85% | meas / desc | R3 p. 16 [IMG] ✔ |

Later textbooks often rename these undular / weak / oscillating / steady / strong jumps, sometimes with slightly different band edges. Peterka's own names and bands are used here. Standing waves and antidunes in alluvial channels (Fr near 1) were not opened in a primary bedform paper this pass (§9).

## 5. Mountain-channel types and slope

Montgomery & Buffington (R4), 1993 TFW report, distinguish cascade, step-pool, plane-bed, pool-riffle, regime and braided alluvial reaches (and bedrock / colluvial). Preliminary field observations in Pacific-Northwest mountain basins (cited as "Montgomery, in prep.") give these slope bins (R4, around printed p. 56 ✔):

| Type | Gradient S (m/m) | Kind | Source |
|---|---|---|---|
| Pool-riffle | < 0.02 | meas (preliminary, PNW) | R4 p. 56 ✔ |
| Plane-bed | 0.01–0.03 | meas (preliminary, PNW) | R4 p. 56 ✔ |
| Step-pool | 0.03–0.08 | meas (preliminary, PNW) | R4 p. 56 ✔ |
| Cascade | 0.08–0.30 | meas (preliminary, PNW) | R4 p. 56 ✔ |
| Colluvial (this region) | > 0.20 | meas (preliminary, PNW) | R4 p. 56 ✔ |
| Reconnaissance from DEM | source S > 0.30; transport 0.03 < S < 0.30; response S < 0.03 (OCR of the last bound is "S ~0.03") | desc | R4 p. 56 ✔ |

"These gradient divisions are not absolute, and morphologic transitions between channel types may be gradual. Drainage area and both local and regional conditions may also influence channel type." Pool-riffle and plane-bed bins overlap (0.01–0.02). The 1997 GSA Bulletin paper by the same authors is the usual journal version of this classification; I did not open it this pass, so later refinements of the bins are not used.

Step spacing in pool-riffle is given in R4 as typified by five to seven channel widths, against a shorter pool spacing in step-pool channels (R4, around p. 31, citing Grant et al. 1990 ✔ for the contrast). I have not extracted a new field table of step height or spacing from Chin or Abrahams.

## 6. Waterfalls

| Quantity | Value (spread) | Kind | Source |
|---|---|---|---|
| Horseshoe Fall, Niagara, natural recession 1842–1905 (63 years) | 5 feet per annum, with an uncertainty of 1 foot (1.52 ± 0.30 m yr⁻¹, my conversion). Gilbert's working range before rounding was 4.2–5.3 ft yr⁻¹, nearer 5.3 if the 1842 map is given full weight; he selects 5 ft to avoid a false sense of precision | meas | R5 summary, p. 25 ✔; discussion pp. 24–25 ✔ |
| American Fall, 1827–1905 (78 years) | less than 3 inches per annum (< 0.076 m yr⁻¹) | meas | R5 p. 25 ✔ |
| Diversion already flagged in 1907 | "The present and prospective diversions of water for economic uses interfere with the course of nature and may be expected to modify the rate of recession." The 5 ft yr⁻¹ figure is offered as the *natural* rate for geological use | desc | R5 p. 25 ✔ |

A free-overfall brink-depth ratio (Rouse 1936, often quoted as y_e / y_c ≈ 0.715) was not opened in the 1936 paper. Waterfall-form taxonomies (plunge, horsetail, cascade, and so on) were not found as a Leopold or Lamb classification; they are popular-guide names, not used here. Angel Falls / Tugela height disputes were not re-opened in a primary survey.

## 7. Lakes and shores

Fetch-limited lake-wave hindcasting (USACE Shore Protection Manual / Coastal Engineering Manual) and a measured seiche period were not opened in those manuals this pass (an earlier CEM download in this project returned 404 or 403). The Merian seiche period T = 2L / √(g h) is the standard rectangular-basin formula; I have not attached a number from a primary seiche paper (Forel on Lake Geneva was not opened). Listed in §9.

## 8. Real-time river and waterfall talks

A short survey only. Published per-frame GPU costs were not found in the pages I or Grok could open.

| Talk / system | What was opened | Published ms cost | Source / note |
|---|---|---|---|
| Vlachos, Valve, "Water Flow in Portal 2" (SIGGRAPH / similar 2010 course) | Grok opened a Valve publication page; I did not re-open the slides this pass | Not taken from a page I opened | Listed in §9 |
| Uncharted 4 water / "Rendering Rapids" | Primary slides not opened | NOT FOUND | §9 |
| Horizon Zero Dawn water | Grok: baked height maps, flow maps, screen-space tessellation; millisecond costs not on the opened slides | NOT FOUND | §9 |
| Ghost of Tsushima dedicated water talk | Not found | NOT FOUND | §9 |

Sea of Thieves water (SIGGRAPH 2018) belongs with G3 (how water is drawn) and is taken up there.

## 9. Not found / not checked against the primary source

1. **Chow (1959) Table 5-6** min/normal/max n, and **Barnes (1967) WSP 1849** measured n. R1 names both. Chow's book was not opened (copyright). The official USGS PDF for WSP 1849 returned 404. An ODOT drainage-manual reprint of a Chow-like table was reported by Grok; I did not open it, and I have not copied those rows.
2. **Park (1977), Rhodes (1977)** worldwide exponent ranges. Not opened in the originals. Not used.
3. **Leopold (1953)** "Downstream change of velocity in rivers" as a separate paper: not opened; the downstream-velocity statements used here are from R2 itself.
4. **Kennedy / Carling antidune and standing-wave Fr bands.** Not opened.
5. **Montgomery & Buffington (1997)** GSA Bulletin tables. The 1993 report (R4) is used. Later slope-bin tweaks are unchecked.
6. **Chin (1989, 1999) and Abrahams et al. (1995)** step spacing and H/(L S) relations. Not opened in the originals.
7. **Schmidt (1990)** Grand Canyon eddy sizes. Not opened.
8. **Rouse (1936)** brink depth. Not opened. Grok marked 0.715 as unverified in the typeset page; I agree and do not use it.
9. **USACE SPM / CEM** fetch-limited lake waves and wind setup. Not opened this pass.
10. **Merian / Forel** seiche periods for named lakes. Not opened in a primary paper.
11. **Game-talk millisecond costs**, including Portal 2, Uncharted 4, Horizon Zero Dawn and Ghost of Tsushima water passes. NOT FOUND in opened pages.
12. **Grok error / correction:** Grok reported that the official EM 25 PDF was blocked and left the sequent-depth formula and Fr bands unverified in Peterka. The monograph was already in `/tmp/g2/em25.pdf`; I read printed pp. 6–7 and 15–16 on the page images. Those rows are ✔ from R3, not from Grok.

## 10. Sources disagree

| Topic | Values | Comment |
|---|---|---|
| Manning factor | 1.486 (R1, R6) vs 1.49 in many handbooks | 0.27%. Use the printed 1.486 with US customary units |
| Hydraulic-geometry exponents | R2 means (at-a-station 0.26/0.40/0.34; downstream 0.5/0.4/0.1) vs later worldwide scatter (not opened here, but reported as very wide) | The 1953 numbers are means of 20 semiarid-biased US sections, fitted by eye. They are a starting order of magnitude, not a universal law |
| Downstream velocity | R2: mean velocity at equal frequency increases slowly, or can be unchanged, downstream. Popular texts often say rivers slow downstream | R2 is the measurement. Local decreases exist; the equal-frequency comparison is the point |
| Channel-type slope bins | Pool-riffle < 0.02 and plane-bed 0.01–0.03 already overlap in R4. The authors say the divisions are not absolute | Later field papers (not opened) are widely said to find the same types well outside these "typical" bands |
| Jump-form names | Peterka: pre-jump / transition / good / effective-but-rough at 1.7, 2.5, 4.5, 9. Textbooks: undular / weak / oscillating / steady / strong, sometimes with 2.8–4.8 etc. | Use Peterka's bands when citing R3. He already says the edges overlap |
| Niagara rate | 5 ± 1 ft yr⁻¹ (1.52 ± 0.30 m yr⁻¹) for Horseshoe 1842–1905 (R5) vs later "~1 m yr⁻¹ historical" and "~0.1 m yr⁻¹ present" summaries | R5 already warns that diversion will change the rate. Later figures were not opened here |

## 11. References (all accessed 2026-11-17)

Link check, 2026-11-17, curl -L: 200 for every URL below unless a note says otherwise.


- **R1** Arcement G.J. Jr., Schneider V.R. (1989) Guide for Selecting Manning's Roughness Coefficients for Natural Channels and Flood Plains. USGS Water-Supply Paper 2339. https://pubs.usgs.gov/wsp/2339/report.pdf (DOI 10.3133/wsp2339)
- **R2** Leopold L.B., Maddock T. Jr. (1953) The hydraulic geometry of stream channels and some physiographic implications. USGS Professional Paper 252. https://pubs.usgs.gov/pp/0252/report.pdf (DOI 10.3133/pp252)
- **R3** Peterka A.J., Hydraulic Design of Stilling Basins and Energy Dissipators. USBR Engineering Monograph 25 (copy read includes later reprint dates in the front matter, through 1990). Official PDF commonly at https://www.usbr.gov/tsc/techreferences/hydraulics_lab/pubs/EM/EM25.pdf (Grok reported a fetch block; the copy read is `/tmp/g2/em25.pdf`)
- **R4** Montgomery D.R., Buffington J.M. (24 June 1993) Channel classification, prediction of channel response, and assessment of channel condition. Report TFW-SH10-93-002 / TFW-SI-110-93-002, Washington State Timber/Fish/Wildlife Agreement. Official PDF: https://dnr.wa.gov/sites/default/files/2025-05/fp_tfw_sh10_93_002.pdf (same 1,766,212-byte file as `/tmp/g2/mb93.pdf`)
- **R5** Gilbert G.K. (1907) Rate of recession of Niagara Falls, accompanied by a report on the survey of the crest. USGS Bulletin 306. https://pubs.usgs.gov/bul/0306/report.pdf (DOI 10.3133/b306)
- **R6** Brunner G.W. (2016) HEC-RAS River Analysis System, Hydraulic Reference Manual, Version 5.0 (December 2015). USACE Hydrologic Engineering Center. Copy read: `/tmp/g2/hecras.pdf`
