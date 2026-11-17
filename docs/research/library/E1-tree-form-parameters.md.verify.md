# E1 verification: tree form as parameters

Verifier: Library Verify (adversarial re-check). Written 2026-11-16, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It is a check of `E1-tree-form-parameters.md`. E1 itself was not edited.

## Version checked

- Path: `/workspace/specula-research/E1-tree-form-parameters.md`
- Size: 34,884 bytes, 312 lines
- File mtime: **2026-11-16 09:59:38 AEDT (Australia/Sydney, UTC+11)**
- Last re-read before this file was written: **2026-11-16 10:10 AEDT**. The file was bit-for-bit identical to a 10:01 snapshot. There was **no “Pass 2 changes” section** and the mtime had not moved, so **World Data’s second pass had not finished (had not changed this file) when this check was closed**.
- Grok CLI: tried first (`~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve`). It exited at once with **“Not signed in”** (same as World Data reported). **No login was attempted.** All checks used curl, WebFetch, WebSearch, and the scratch copies already in `/workspace/scratch-e1/` (outside any repo). Access date for live URLs: **2026-11-16**.

## Summary

Unit of counting: **one data row** (a table row, a CSV parameter-set row, or a clearly bounded multi-value claim such as the Khaya / Araucaria paragraph). Multi-number rows were checked number-by-number; a single wrong figure makes the whole row FIX. The licence summary in §1 is not in the row counts; it is in the source table.

| Status | Rows | What it means |
|---|---:|---|
| **VERIFIED** | **101** | The cited source opens (or an open copy does), and it states that number, units, and population/species. |
| **FIX** | **5** | A stated number or wording is wrong or misleading. Correction below. |
| **UNVERIFIABLE** | **4** | Could not confirm the claimed digit or the unopened primary; no number invented. |
| **Total data rows** | **110** | About 380 individual figures sit inside these rows, as World Data said. |

**Most serious problems**

1. **FIX — California black oak `1DownAngle` is written “30 ±30”.** Weber & Penn 1995, Appendix p.126, gives **30, −30**. A negative `nDownAngleV` is not the same as a ± spread: the paper’s own rule makes the angle change along the parent when V is negative. The aspen and tupelo rows correctly write `60 ±(−50)` and `60 ±(−40)`; the oak row dropped the minus. For a generator this is the error that would actually change the tree.
2. **FIX — Norway spruce “seed-source range ≈ 83–91°” is not in Budeanu et al. 2012.** Trial means are about 84° (Avrig, index 1.87) and 83° (Breţcu, index 1.84). The paper says the **average** of 33 seed sources in both sites is about 83–84°. 91° is one source (Moldoviţa) at Avrig only. No seed-source minimum of 83° is given. The 3-step index (1 = <90°, 2 = 90°, 3 = >90°) is described correctly.
3. **FIX — S13 does not call the textbook phyllotaxis list “folklore”.** Okabe 2015 says the list “is often presented” and that “the phyllotaxis fraction is not a determined trait of each species.” The list itself and the willow 5/13 vs 3/8 disagreement are right. This wording appears in the cross-species phyllotaxis table and in the oak and beech phyllotaxis rows.
4. **UNVERIFIABLE — *Quercus alba* Bechtold b1 third digit.** Table 3 Eq. 4 in the USDA scan is OCR-garbled as `5.9658 1 S212`. Reading b0 = 5.9658 is sound; b1 as 1.5212 is a reasonable guess but the third digit is not readable. Eq. 5 prints **1.5158**. World Data already flagged this ⚠; it should not be used as a confirmed coefficient.
5. **Dead or fragile full-text links** (numbers were still checked from another copy where noted): the Stellenbosch Pretzsch PDF returns **HTTP 500** to curl; the Notulae Botanicae PDF URL returns **403** to curl (WebFetch did open it); publisher DOIs for ACM, APS, Wiley and Oxford often return 403/500 to curl, as E1 already says. Alternative URLs that did resolve are in the source table.

**What is in good shape.** Cross-species rules that matter for a generator — Chave 2014 Eq. 6a/6b, Jucker 2017 Eq. 6, Loubota Panzou et al. 2021 Table 1 crown exponents, Eloy 2011 Δ range and wind-stress prediction, Minamino & Tateno 2014 field indices, ABOP Tables 2.1–2.3 and 137.5°, Runions pipe exponent and illustration settings, Pretzsch Tables 1, 2 and 4 (coefficients recompute to the printed cr25/h25), Bechtold Table 3 Eq. 4 for every species except the *Q. alba* digit, FEIS descriptive sizes, and the NPS saguaro pages — checked out. Kind labels (meas / param / desc) are right for the rows I checked. Weber & Penn, ABOP, Runions and Mäkelä are correctly marked as chosen parameters, not measurements.

**§5 Not found and §6 Sources disagree** are present and, where I could check, honest. The olive “crown” equation is correctly rejected as a **biomass** equation in S17 Table 4 (`y = 0.1202 x^2.2159`, Brunori et al. 2017). The saguaro disagreement with unopened Drezner/brochure figures is correctly left ⚠. The S6 5%-quantile height vs S7 stand-grown crown-width caveat is right. The pipe-exponent tension (ABOP = 2, S5 range 1.8–2.3, Runions “usually 2 to 3”) is fairly described. ⚠ items in E1 are also listed in §6.

## Row table

Columns: row id, claim (short), status, note/correction, URL checked. Access date 2026-11-16 throughout.

### 2.1 Pipe / Leonardo rule

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| P1 | Literature Δ in 1.8 < Δ < 2.3 for “a large number of species”; S5 p.1 cites Mandelbrot 1983, Aratsu 1998, Sone et al. 2009 | VERIFIED in S5; primaries UNVERIFIABLE | Exact interval is on p.1 of Eloy. The three primary studies were not opened here either (same ⚠ as E1). | https://www.irphe.fr/~eloy/assets/pdf/PRL2011.pdf (200); APS DOI 403 |
| P2 | Wind-stress model predicts 1.93 < Δ < 2.21 for 2 < D < 3, m = 10 | VERIFIED | After eq. 8a, for infinite branching. Kind “model” is right. | same author PDF |
| P3 | Weibull modulus of wood typically 5 < m < 20 | VERIFIED | “typically 5 < m < 20 for wood”. | same |
| P4 | l_k/l_{k+1} = N^{1/D}, d_k/d_{k+1} = N^{1/Δ}; N = daughters per node | VERIFIED | Eloy eq. 2. | same |
| P5 | Uniform-stress index 0.97 ± 0.02 SE, *Fagus crenata*, 34 points, 6 trees, Nikko | VERIFIED | “0.97±0.02 SE”, Fig. 7A. 34 points, always 2 daughters, 13 branches on 6 trees ~30 yr. | https://doi.org/10.1371/journal.pone.0093535 (200, CC BY 4.0) |
| P6 | Same index 0.88 ± 0.02 SE, *Abies homolepis* | VERIFIED | Fig. 7B. 79 points = 39 with 2, 37 with 3, and 3 “with more than 4” daughters, 13 branches, 7 trees. E1’s “≥4” is slightly looser than “more than 4”; not enough to reclass the row. | same |
| P7 | Elastic-similarity index 0.88 ± 0.03 SE, *F. crenata* | VERIFIED | | same |
| P8 | Daughters per point: 2 (*F. c.*); 2–5 (*A. h.*) | VERIFIED | Methods. | same |
| P9 | Lateral-daughter angles 10–50° (*F. c.*); 50–80° (*A. h.*) | VERIFIED | Discussion, as E1 cites. | same |
| P10 | Vertical/horizontal diameter 1.065 ± 0.007 (*F. c.*); 1.034 ± 0.003 (*A. h.*) | VERIFIED | Methods. | same |
| P11 | Mother diameter 8.3–56.0 mm (*F. c.*); 6.0–49.1 mm (*A. h.*) | VERIFIED | | same |
| P12 | Sample height 14.7 ± 1.3 SD, 14.1–16.9 m (*F. c.*); 13.8 ± 5.5, 9.4–22.6 m (*A. h.*) | VERIFIED | | same |
| P13 | Sample DBH 26.4 ± 1.5 cm; 31.6 ± 13.1 cm | VERIFIED | | same |
| P14 | w2/w1 = 1/√2 ≈ 0.707 for Δ = 2 | VERIFIED | ABOP p.57, wr = 0.707. Kind param is right. | https://algorithmicbotany.org/papers/abop/abop.pdf (200) |
| P15 | Runions pipe exponent n “usually between 2 and 3”; eq. 1; Macdonald 1983 pp.131–135 | VERIFIED | §2, eq. 1. Kind param is right. | https://algorithmicbotany.org/papers/colonization.egwnp2007.pdf (200) |
| P16 | Bentley abstract: “branching is area preserving”, 5 species; exponents not read | VERIFIED (abstract only); exponents UNVERIFIABLE | Crossref abstract confirms the qualitative claim and n = 5 species. No exponents in the abstract. | https://api.crossref.org/works/10.1111/ele.12127 (200); Wiley DOI 403 |

### 2.2 Model constants (ABOP and Runions)

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| A1–A4 | Honda monopodial Table 2.1 p.57: 2.6a–d as in the CSV (r1/r2/a0/a2) | VERIFIED | All four rows match. wr = 0.707 stated on p.57. Printed page 57 confirmed. | ABOP PDF (200) |
| A5–A8 | Aono–Kunii sympodial Table 2.2 p.58: 2.7a–d | VERIFIED | All four rows match. Page 58 confirmed. | same |
| A9–A12 | Ternary Table 2.3 p.61, including tropism vectors, e, n, and vr = 1.732 | VERIFIED | All four rows match (E1 shortens 0.00 to 0). “Width increase rate” is ABOP’s own name. Page 61 confirmed. | same |
| R1 | Space-colonization illustrations: d_k = 2D and 20D; N = 375–12000; d_i = ∞, 17D (shrubs, Fig. 4) or 8D (trees, Fig. 5) | VERIFIED | Figs. 3–5. Correctly labelled as illustration settings, not recommended values. | Runions PDF (200) |

### 2.3 Phyllotaxis

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| Ph1 | Golden angle 360°·τ⁻² ≈ 137.5° | VERIFIED | ABOP p.99; Okabe Table 1 (0.382, 137.5°). | ABOP PDF; https://www.nature.com/articles/srep15358/tables/1 (200) |
| Ph2 | Same angle as a fraction of a turn: 0.382 | VERIFIED | Okabe Table 1 and text (137.5/360 = 0.382). | same |
| Ph3 | Main fraction series 1/2, 1/3, 2/5, 3/8, 5/13, 8/21, 13/34 → 180°, 120°, 144°, 135°, 138.5°… | VERIFIED | S13 gives the Fibonacci rule and 3/8 (135°), 5/13 (138°). 8/21 and 13/34 follow the same rule; the degree column is plain arithmetic, as E1 says (5/13 × 360 ≈ 138.46°, rounded 138.5°). | same |
| Ph4 | Textbook list: 1/2 elm, lime, linden; 1/3 beech, hazel; 2/5 oak, cherry, apple, holly, plum; 3/8 poplar, rose, pear; 5/13 almond; and “S13 calls it folklore” | FIX | The list is exact, and S13 does say the fraction “is not a determined trait of each species.” **Correction:** S13 does **not** use the word “folklore”. It says the list “is often presented as follows.” Do not attribute that word to Okabe. | https://www.nature.com/articles/srep15358 (200, CC BY 4.0) |
| Ph5 | Anomalous angles: 99.5° (*Cunninghamia*, *Cedrus deodara*); 151.1° (*Betula alba*, *Corylus*); 106.4° (*Abies balsamea*); 137.5° (*Picea abies* cone, *Populus deltoides*) | VERIFIED | Okabe Table 1, including 0.276→99.5, 0.420→151.1, 0.296→106.4. Species match (Corylus as *C. tubulosa*). | Table 1 URL (200) |
| Ph6 | Decussate 90°; lilac Fig. 3.20, pp.93–94 | VERIFIED | Production p1 rotates the apex by 90°, “results in a decussate branching pattern.” Fig. 3.20 is on p.93; p.94 is Fig. 3.21. | ABOP PDF |
| Ph7 | Weber & Penn nRotate 140° for most levels; 77° and 80° for some; −120° alternating for willow | VERIFIED | Appendix: aspen 3Rotate = 77; CA black oak 1Rotate = 80; willow 1Rotate/2Rotate = −120 (negative = alternating, as E1 says). | https://courses.cs.duke.edu/fall02/cps124/resources/p119-weber.pdf (200); ACM DOI 500/403 |

### 3. Cross-species allometry

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| C1 | Chave 2014: ln H = 0.893 − E + 0.760 ln D − 0.0340 (ln D)²; E = (0.178·TS − 0.938·CWD − 6.61·PS)·10⁻³; H m, D cm; 4004 trees ≥ 5 cm, 58 sites | VERIFIED (equation and sample); journal page UNVERIFIABLE | Eq. 6a/6b exact in the author PDF. Abstract: “58 sites… 4004 trees ≥ 5 cm.” The author PDF is an early-view copy without journal pagination, so p.3183 stays unconfirmed, as E1 already ⚠. “Tropical sites” is slightly loose (pantropical study, mixed vegetation types) but not a wrong coefficient. | https://jeromechave.github.io/pubs/chave-gcb14.pdf (200); Wiley DOI 403 |
| C2 | Jucker 2017 Eq. 6: D = 0.557·(H·CD)^0.809·exp(0.056²/2); D cm, H m, CD m; 108,753 trees; RMSE 9.7 cm | VERIFIED | Eq. 6 and “RMSE (9.7 cm)” confirmed. Units in the methods match. | https://research-information.bris.ac.uk/ws/files/232002669/gcb.13388.pdf (200); https://pmc.ncbi.nlm.nih.gov/articles/PMC6849852/ (200) |
| C3 | Loubota forest Cdia exponent b = 0.63 [0.57–0.69] | VERIFIED | Table 1, Cdia–D column. n = 87,737 trees, 205 forest + 40 savanna sites. | https://agritrop.cirad.fr/599164/1/Loubota%26al%282020%29PantropicalVariabilityInTreeCrownAllometry.pdf (200) |
| C4 | Savanna Cdia b = 0.75 [0.67–0.82] | VERIFIED | Table 1. | same |
| C5 | Crown-depth exponent forest / savanna 0.60 [0.47–0.72] / 0.69 [0.67–0.72] | VERIFIED | Table 1, Cdep–D. | same |
| C6 | Crown-volume exponent 1.88 [1.64–2.12] / 2.15 [2.10–2.21] | VERIFIED | Table 1, Cvol–D. | same |

### 4. Per-species tables — Pretzsch (S6) and Bechtold (S7)

S6 form: ln(cr) = a + α ln(d), 95% quantile (open-grown); ln(h) = a + α ln(d), 5% quantile (open-grown, short for their diameter). d in cm, cr and h in m. I recomputed every cr25/h25 as exp(a + α ln 25); all match the printed one-decimal values. S6 full-text URL (Stellenbosch) returned HTTP 500; values were checked in the local article extract, which carries the journal running heads (pp.466–479) and Tables 1, 2 and 4. S7 is the USDA TreeSearch PDF (200). LCW in **ft**, D in **inches**, D ≥ 5.0 in., FHM plots in 24 eastern states, 1991–1999, stand-grown trees: all confirmed. Reprint stamp “Not for further reproduction” is on p.269.

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| Q1 | *Q. robur* data range d 3.6–196.1 cm (mean 35.2); cr 0.2–13.8 m (mean 3.1); age 16–220 (mean 104); n = 4768 LTP + Cape Town 27 park / 44 street | VERIFIED | S6 Table 1. | Pretzsch extract (Stellenbosch URL 500); DOI landing 200 |
| Q2 | *Q. robur* crown a = −1.4370, α = 0.8157; cr25 = 3.3 m; SE 0.0353 / 0.0103; n = 4839 | VERIFIED | Table 2. 4768+27+44 = 4839. Recompute 3.28 → 3.3. | same |
| Q3 | *Q. robur* height a = −0.5239, α = 0.7163; h25 = 5.9 m; SE 0.2355 / 0.0551; n = 69 | VERIFIED | Table 4, 5% quantile. Recompute 5.94 → 5.9. Small n correctly flagged. | same |
| Q4 | *Q. alba* crown width b0 = 5.9658, b1 = 1.5212; RSQ 0.68, RMSE 4.8 ft, CV 23%; n = 1586 | UNVERIFIABLE (b1 third digit); rest VERIFIED | Table 3 Eq. 4: RSQ/RMSE/CV and b0 = 5.9658 are clear. b1 is printed “1 S212”. Eq. 5 gives **1.5158**. n = 1,586 in Table 1. Keep ⚠. | https://research.fs.usda.gov/treesearch/download/6206.pdf (200) |
| Q5 | *Q. rubra* b0 = 6.2141, b1 = 1.4026; RSQ 0.61, RMSE 5.7, CV 25% | VERIFIED | Table 3 Eq. 4, northern red oak. | same |
| Q6 | CA black oak Weber set: Shape 2; BaseSize 0.05; Ratio 0.018, RatioPower 1.3; 1DownAngle 30 ±30; 1Rotate 80; 1Branches 40; 1Length 0.8; 2DownAngle 45 ±10; 2Rotate 140; 2Branches 120; 2Length 0.2; Leaves 25 | FIX | Every listed number matches the Appendix **except** `1DownAngleV`. **Correction:** 1DownAngle, 1DownAngleV = **30, −30** (not 30 ±30). Negative V has special meaning in the paper. Source: Weber & Penn 1995, Appendix “Parameter List”, printed p.126. URL: https://courses.cs.duke.edu/fall02/cps124/resources/p119-weber.pdf . Access 2026-11-16. | Duke PDF (200) |
| Q7 / Be4 / Wi3 | Oak / beech / willow phyllotaxis rows that repeat the textbook list and “folklore” | FIX | Same wording issue as Ph4. Willow 5/13 vs 3/8 is correctly taken from S13. | Okabe article + Table 1 (200) |
| Be1 | *F. sylvatica* range d 2.9–191.4 (mean 23.1); cr 0.1–14.3 (mean 3.0); age 16–283; n = 14,963 | VERIFIED | S6 Table 1. | Pretzsch extract |
| Be2 | *F. sylvatica* crown a = 0.0111, α = 0.4710; cr25 = 4.6 m; SE 0.0135 / 0.0045 | VERIFIED | Table 2 (a is positive, unlike most species). Recompute 4.60. | same |
| Be3 | *F. sylvatica* height a = −0.2154, α = 0.8607; h25 = 12.9 m; SE 0.0764 / 0.0235; n = 640 | VERIFIED | Table 4. Recompute 12.87 → 12.9. | same |
| Be4b | *F. crenata* branch angle 10–50°; 34 points, 6 trees | VERIFIED | Same as P9/P5. | Minamino (200) |
| Be5 | *F. grandifolia* b0 = 11.5347, b1 = 1.1368; RSQ 0.38, RMSE 6.8, CV 31%; n = 777 | VERIFIED | S7 Table 3 Eq. 4 and Table 1 n. | Bechtold PDF (200) |
| M1 | *A. pseudoplatanus* crown a = −0.4446, α = 0.5339; cr25 = 3.6 m; SE 0.0447 / 0.0162; n = 942; d 4.7–56.7 cm | VERIFIED | S6 Tables 1–2. Recompute 3.58 → 3.6. | Pretzsch extract |
| M2 | *A. pseudoplatanus* height a = 0.9669, α = 0.5858; h25 = 17.3 m; SE 0.0376 / 0.0112; n = 3248 | VERIFIED | Table 4. Recompute 17.33 → 17.3. | same |
| M3 | *A. saccharum* b0 = 9.9644, b1 = 1.0545; RSQ 0.38, RMSE 5.4, CV 27%; n = 2259 | VERIFIED | S7 Table 3 Eq. 4; Table 1 n = 2,259. | Bechtold PDF |
| M4 | *A. saccharum* mature size 27–37 m, 76–91 cm DBH; extremes > 40 m, > 1.5 m; lives 300–400 yr | VERIFIED | FEIS Tirmenstein 1991, “90 to 120 feet (27-37 m)… 30 to 36 inches (76-91 cm)… more than 130 feet (40 m)… more than 5 feet (1.5 m)… 300 to 400 years.” | https://www.fs.usda.gov/database/feis/plants/tree/acesac/all.html (200) |
| Bi1 | *B. pendula* crown a = −0.9688, α = 0.6483; cr25 = 3.1 m; SE 0.0766 / 0.0316; n = 31; d 5.0–38.7 cm | VERIFIED | S6 Table 2 + Table 1. Small n correctly flagged. Recompute 3.06 → 3.1. | Pretzsch extract |
| Bi2 | *B. pendula* height a = 0.6952, α = 0.5490; h25 = 11.7 m; SE 0.1773 / 0.0768; n = 26 | VERIFIED | Table 4. Recompute 11.73 → 11.7. | same |
| Bi3 | *B. papyrifera* b0 = 5.2177, b1 = 1.3332; RSQ 0.34, RMSE 4.7, CV 29%; n = 889 | VERIFIED | S7 Table 3 (b0 OCR-spaced as “5.2 177”); Table 1 n = 889. | Bechtold PDF |
| Bi4 | *B. papyrifera* 21–24 m tall, 25–30 cm DBH (up to 75 cm); Alaska 6–18 m; narrow forest crown, wide from near the base in the open; often multi-stemmed | VERIFIED | FEIS Uchytil 1991, all clauses present. | https://www.fs.usda.gov/database/feis/plants/tree/betpap/all.html (200) |
| Bi5 | *Betula alba* divergence 151.1° | VERIFIED | Okabe Table 1. | Table 1 (200) |
| At1 | Quaking aspen Weber set (Shape 7, BaseSize 0.4, Scale 13 ±3, Levels 3, Ratio 0.015 / 1.2, 1DownAngle 60 ±(−50), etc.) | VERIFIED | Appendix matches, including the negative 1DownAngleV written correctly here. | Duke PDF |
| At2 | *P. tremuloides* b0 = 4.6305, b1 = 1.2798; RSQ 0.51; n = 1375 | VERIFIED | S7 Table 3 Eq. 4 (RMSE 3.8, CV 24% omitted but not contradicted); Table 1 n = 1,375. | Bechtold PDF |
| At3 | Black tupelo Weber set (Shape 4, BaseSize 0.2, Scale 23 ±5, Levels 4, Ratio 0.015 / 1.3, 1DownAngle 60 ±(−40), etc.) | VERIFIED | Appendix matches. 2DownAngle 30 and 3DownAngle 45 are the central values (V = 10 omitted, not wrong). | Duke PDF |
| Pi1 | *P. sylvestris* crown a = −1.0345, α = 0.6381; cr25 = 2.8 m; SE 0.0527 / 0.0172; n = 1609; d 5.3–76.2 cm; age 16–200 | VERIFIED | S6 Tables 1–2. Recompute 2.77 → 2.8. | Pretzsch extract |
| Pi2 | Height a = −0.3449, α = 0.9326; h25 = 14.3 m; SE 0.1807 / 0.0552; n = 331 | VERIFIED | Table 4. Recompute 14.25 → 14.3. | same |
| Pi3 | Eastern US plantations b0 = 6.3738, b1 = 0.8917; RSQ 0.20, RMSE 3.9, CV 29%; n = 87 | VERIFIED | S7 Table 3, Scotch pine; Table 1 n = 87. | Bechtold PDF |
| Pi4 | Branch insertion at emergence 45° + N(0, 5°), then increases with whorl age; some parameters set “through guesswork”, no fitting | VERIFIED | S12 §2.4.1, and later “some of them through guesswork, but no model fitting.” Kind **param** is right. | https://www.silvafennica.fi/pdf/5632 (200) |
| Pi5 | Azimuth shift 30° clockwise between yearly whorls; branches evenly spaced | VERIFIED | §2.4.1. | same |
| Pi6 | Crown “open and spreading”; more branches per whorl than red or white pine | VERIFIED | FEIS Sullivan 1993: both *P. resinosa* and *P. strobus* are named. | https://www.fs.usda.gov/database/feis/plants/tree/pinsyl/all.html (200) |
| Sp1 | *P. abies* crown a = −0.6713, α = 0.5083; cr25 = 2.6 m; SE 0.0202 / 0.0063; n = 10,775; d 2.2–98.9 cm | VERIFIED | S6. Recompute 2.62 → 2.6. | Pretzsch extract |
| Sp2 | Height a = −0.1753, α = 0.9113; h25 = 15.8 m; SE 0.0347 / 0.0108; n = 6713 | VERIFIED | Table 4. Recompute 15.77 → 15.8. | same |
| Sp3 | US crown width b0 = 4.5152, b1 = 1.0654; RSQ 0.58, RMSE 2.6, CV 21%; n = 100 | VERIFIED | S7 Table 3, Norway spruce; Table 1 n = 100. | Bechtold PDF |
| Sp4 | Dominant-branch insertion ≈ 83–84° (mean of 33 seed sources, both trials); trial means ≈ 84° (Avrig) and ≈ 83° (Breţcu); seed-source range ≈ 83–91°; 30-yr trials, Romania; whorl ~2.2 m; scored 1/2/3 then converted | FIX | Trial means, the 33-source **average** of about 83–84°, age 30, whorl at 2.20 m, and the index (1 = <90°, 2 = 90°, 3 = >90°) are all in Budeanu et al. 2012, Results and Methods. **Correction:** do not call 83–91° a seed-source range. 91° is Moldoviţa at Avrig only; 83° is a trial mean, not a documented source minimum. Variance is described as high, with no min–max table. Open copy used: WebFetch of https://www.notulaebotanicae.ro/index.php/nbha/article/download/8148/7042/30157 (curl 403). Access 2026-11-16. | WebFetch of that PDF URL |
| Sp5 | Crown narrowly conic when young, broadly columnar when old | VERIFIED | FEIS Sullivan 1994. | https://www.fs.usda.gov/database/feis/plants/tree/picabi/all.html (200) |
| Sp6 | Cone-scale divergence 137.5° | VERIFIED | Okabe Table 1, *Picea abies* (cone). | Table 1 (200) |
| Df1 | *P. menziesii* range d 10.8–64.4 cm (mean 32.7); cr 1.0–6.9 m; age 40–51; n = 1613 | VERIFIED | S6 Table 1. | Pretzsch extract |
| Df2 | Crown a = −1.2808, α = 0.7522; cr25 = 3.1 m; SE 0.1006 / 0.0291 | VERIFIED | Table 2. Recompute 3.13 → 3.1. | same |
| Df3 | Height a = 0.1377, α = 0.8005; h25 = 15.1 m; SE 0.0433 / 0.0133; n = 2677 | VERIFIED | Table 4. Recompute 15.10 → 15.1. | same |
| Bo1 | *P. mariana* b0 = 2.8233, b1 = 0.9797; RSQ 0.24, RMSE 3.1, CV 31%; n = 349 | VERIFIED | S7 Table 3 and Table 1. | Bechtold PDF |
| Bo2 | *P. mariana* 9–15 m, 15–25 cm DBH; 100–200-yr trees may be only 3–6 m; branches from ground to crown; dead branches kept | VERIFIED | FEIS: “Branches are usually retained after they die.” Other figures match. | https://www.fs.usda.gov/database/feis/plants/tree/picmar/all.html (200) |
| Bo3 | *L. laricina* b0 = 1.9502, b1 = 1.4938; RSQ 0.39; n = 163 | VERIFIED | S7 Table 3, tamarack; Table 1 n = 163. | Bechtold PDF |
| Bo4 | *L. decidua* crown a = −1.2919, α = 0.7131; cr25 = 2.7 m; n = 340 | VERIFIED | S6 Table 2. Recompute 2.73 → 2.7. | Pretzsch extract |
| Bo5 | *L. decidua* height a = 0.8585, α = 0.6378; h25 = 18.4 m; n = 833 | VERIFIED | Table 4. Recompute 18.38 → 18.4. | same |
| Bo6 | *P. glauca* crown a = −1.7910, α = 0.8276, cr25 = 2.4 m; height a = 0.9573, α = 0.5714, h25 = 16.4 m; n = 143 (Prince George) | VERIFIED | S6 Tables 2 and 4 (n = 143 both). Table 1 splits 87+48+10 by plot type. Recomputes 2.39 and 16.39. | same |
| Bo7 | *P. glauca* US b0 = 6.1834, b1 = 0.8567; n = 200 | VERIFIED | S7 Table 3, white spruce; Table 1 n = 200. | Bechtold PDF |
| Bo8 | *A. homolepis* branch angle 50–80°; 79 points, 7 trees | VERIFIED | Same as P6/P9. | Minamino (200) |
| Wi1 | *S. nigra* crown “broad and open with stout branches” | VERIFIED | FEIS. | https://www.fs.usda.gov/database/feis/plants/tree/salnig/all.html (200) |
| Wi2 | *S. nigra* DBH coefficient not significant at P = 0.05, no usable slope | VERIFIED | S7 Results: P = 0.05, five species including black willow. | Bechtold PDF |
| Wi3b | Weeping willow Weber set (Shape 3, BaseSize 0.05, Levels 4, Ratio 0.03 / 2, 0BaseSplits 2, 1DownAngle 20 ±10, 1Rotate −120 ±30 alternating, 1Branches 25, 1Curve 40, 1CurveBack 80, 2Length 1.5, 3Branches 300, AttractionUp −3, PruneRatio 1) | VERIFIED | Appendix matches, including negative rotate = alternating. | Duke PDF |
| Eu1 | *E. globulus* height 30–55 m | VERIFIED | FEIS Esser 1993, “98 to 180 feet (30-55 m)”. | https://www.fs.usda.gov/database/feis/plants/tree/eucglo/all.html (200) |
| Eu2 | *E. globulus* “open crowns bearing pendulous branches” | VERIFIED | Same page, exact phrase. | same |
| Ol1 | *Olea* H–D, crown–D, branch angle not found | VERIFIED as a gap | See §5 / S17: the alleged crown equation is a biomass equation. | https://pmc.ncbi.nlm.nih.gov/articles/PMC12753925/ (200) |
| Tr1 | *Khaya senegalensis* (Hanoi, n = 163, d 44.1–123.1 cm): crown a = −0.5889, α = 0.6461, cr25 = 4.4 m; height a = −0.4335, α = 0.7724, h25 = 7.8 m (n = 144) | VERIFIED | S6 Tables 1, 2, 4. Recomputes 4.44 and 7.79. | Pretzsch extract |
| Tr2 | *Araucaria cunninghamii* (Brisbane, n = 126): crown a = −0.6754, α = 0.3899, cr25 = 1.8 m; height a = 0.1012, α = 0.6586, h25 = 9.2 m | VERIFIED | Table 1 n = 46+80 = 126. Recomputes 1.79 and 9.22. | same |
| J1 | *Y. brevifolia* height 5–20 m (“20- to 70-foot”); trees > 10 m rare | VERIFIED | FEIS Gucker 2006. | https://www.fs.usda.gov/database/feis/plants/tree/yucbre/all.html (200) |
| J2 | Trunk diameter 0.3–0.9 m, expanded base | VERIFIED | “1 to 3 feet (0.3-0.9 m)… expanded base.” | same |
| J3 | Height before first branching 0.9–3 m | VERIFIED | “3 to 9 feet (0.9-3 m) tall before branching.” | same |
| J4 | Dichotomous or nearly so; branch after terminal bud dies (flowering or insect damage) | VERIFIED | | same |
| J5 | Branches 2–5 m or longer; fork every 0.6–0.9 m; inner erect, outer horizontal or drooping | VERIFIED | “7 to 20 feet (2-5 m) or longer and fork at 2- to 3-foot (0.6-0.9 m) intervals.” | same |
| Sg1 | First-arm height: some at about 3.25 m (prelim. citizen science); arms start at 8–20 ft (≈2.4–6.1 m) | VERIFIED | Arms page: “some saguaros… around 3.25 meters”; “Initial data.” Growth page: “Eight to Twenty Feet Tall At this height, the saguaro will start pushing out its arms.” Conversion 8 ft = 2.44 m, 20 ft = 6.10 m. Kind meas (prelim.) / desc is right. | https://www.nps.gov/sagu/learn/saguaro-arms-project.htm (200); https://www.nps.gov/sagu/learn/nature/saguaro-growth.htm (200) |
| Sg2 | Age at first arm usually 50–70 yr; up to 100 yr in some areas | VERIFIED | Arms page, exact figures. | arms page (200) |
| Sg3 | Height–age: 1–3 in at 5–10 yr; 2 ft at 20–40 yr; 6–8 ft at 35–70 yr; healthy adults about 40 ft at > 150 yr; tallest recorded 78 ft (no arms) | VERIFIED | Growth page, all five milestones, including “78 feet tall, and it had zero arms.” Updated 19 Dec 2024 as E1 says. | growth page (200) |
| Sg4 | First arm tends to face south or southeast; second arm often north | VERIFIED | Arms page, “Initial data.” Kind meas (prelim.) is right. Updated 2 May 2024. | arms page (200) |

### §5 Not found and §6 disagreements

| Check | Status | Note |
|---|---|---|
| Olive crown equation rejected | VERIFIED as a correct rejection | S17 Table 4 is headed “allometric equations for **biomass** estimation.” Olive: y = 0.1202 x^2.2159, Brunori et al. 2017, R² 0.9960. Not a crown-diameter equation. |
| Coconut, baobab, acacia, rainforest emergents, per-order branch ratios, crown-ratio means, flora-level phyllotaxis | Present; not re-researched from scratch | I did not invent replacements. The gap list matches what I could and could not open. Bentley (S16) still has no readable exponent table from the abstract alone. |
| Saguaro onset vs unopened Drezner / brochure | Honest ⚠ | NPS figures above are verified. The conflicting ~4.5 m / ~95–100 yr figures were not opened here either. |
| Willow 5/13 vs 3/8 | VERIFIED disagreement | Okabe: “Some references present willow as 5/13 and others as 3/8 without citing sources.” |
| S6 height = 5% quantile, S7 crowns = stand-grown | VERIFIED caveat | Pretzsch Methods: 5% height quantile is explicitly for open-grown (lowest height at a given d). Bechtold used only stand-grown trees (open-grown excluded). |
| Q. alba OCR; Chave page; S5 primaries | Honest ⚠ | Confirmed as still unverifiable, not hidden errors. |

## Source table

| Key | URL (as listed, or the open copy actually read) | Resolves? | Licence stated correctly? |
|---|---|---|---|
| S1 Weber & Penn 1995 | DOI https://doi.org/10.1145/218380.218427 — curl 500. Full text: https://courses.cs.duke.edu/fall02/cps124/resources/p119-weber.pdf | DOI n / PDF **y** (200) | **y** — © ACM; parameters are chosen, not measured. |
| S2 ABOP 1990 | https://algorithmicbotany.org/papers/abop/abop.pdf | **y** (200; E1’s 206/200 is a curl quirk, I got 200) | **y** — copyrighted Springer book; authors post a free PDF. |
| S3 Runions et al. 2007 | https://algorithmicbotany.org/papers/colonization.egwnp2007.pdf | **y** (200) | **y** — © Eurographics / authors; free author PDF. |
| S4 Minamino & Tateno 2014 | https://doi.org/10.1371/journal.pone.0093535 | **y** (200) | **y** — CC BY 4.0 in the article XML. |
| S5 Eloy 2011 | DOI https://doi.org/10.1103/PhysRevLett.107.258101 — 403. Read: https://www.irphe.fr/~eloy/assets/pdf/PRL2011.pdf and https://arxiv.org/abs/1105.2591 | DOI n / author PDF **y** / arXiv **y** | **y** — © APS; author PDF and arXiv. |
| S6 Pretzsch et al. 2015 | DOI https://doi.org/10.1016/j.ufug.2015.04.006 — landing 200. Full text http://academic.sun.ac.za/forestry/people/Bdt/2015%20Pretzsch%20et%20al%20Urban%20tree%20crowns%20UFUG%2014(3)466-479.pdf — **HTTP 500** now | DOI landing y / listed PDF **n (500)** | **y** — CC BY-NC-ND 4.0 is in the article (“no commercial use… no derivatives” of the article; coefficients are facts). **Needs a replacement working PDF URL.** Content was checked from the local article extract with journal pagination 466–479. |
| S7 Bechtold 2003 | https://research.fs.usda.gov/treesearch/6206 (200); PDF https://research.fs.usda.gov/treesearch/download/6206.pdf (200). DOI https://doi.org/10.1093/sjaf/27.4.269 — 403 | TreeSearch **y** / DOI n | **y** — USFS author; reprint stamped “Not for further reproduction.” Cite numbers, don’t redistribute the PDF. |
| S8 Chave et al. 2014 | DOI 403. Author PDF https://jeromechave.github.io/pubs/chave-gcb14.pdf | DOI n / PDF **y** (200) | **y** — © Wiley; author-hosted PDF. |
| S9 Loubota Panzou et al. 2021 | DOI 403. https://agritrop.cirad.fr/599164/1/Loubota%26al%282020%29PantropicalVariabilityInTreeCrownAllometry.pdf | DOI n / PDF **y** (200) | **mostly y** — article says “Creative Commons Attribution License.” I did not see the words “4.0” on p.1. CC BY is correct; the version number is not explicit in the copy I opened. |
| S10 Jucker et al. 2017 | DOI 403. https://research-information.bris.ac.uk/ws/files/232002669/gcb.13388.pdf (200); https://pmc.ncbi.nlm.nih.gov/articles/PMC6849852/ (200) | DOI n / author and PMC **y** | **mostly y** — article: “Creative Commons Attribution License.” Bristol cover sheet: “CC BY.” “4.0” not seen in those lines. Same caution as S9. |
| S11 Budeanu et al. 2012 | DOI https://doi.org/10.15835/nbha4028148 — 500. PDF https://www.notulaebotanicae.ro/index.php/nbha/article/download/8148/7042/30157 — curl 403, **WebFetch succeeded** | curl n / WebFetch **y** | **y** — E1 says the licence was not stated in the text read. I also found no licence line in the open copy. Treat as copyrighted, numbers as facts. |
| S12 Mäkelä et al. 1997 | https://www.silvafennica.fi/pdf/5632 | **y** (200) | **y** — open journal archive; I also did not find a CC statement on the first page. “Licence not confirmed” is fair. |
| S13 Okabe 2015 | https://www.nature.com/articles/srep15358 (200); Table 1 https://www.nature.com/articles/srep15358/tables/1 (200). (The period after the Table 1 URL in E1 is end-of-sentence punctuation, not part of the URL.) | **y** | **y** — “Creative Commons Attribution 4.0 International License” is stated on the article. |
| S14 FEIS | All eight listed species pages | **y** (200 each) | **y** — US government work, public domain for the text. E1 correctly notes some photos are © and none are used. |
| S15 NPS Saguaro | https://www.nps.gov/sagu/learn/saguaro-arms-project.htm (200, updated 2 May 2024); https://www.nps.gov/sagu/learn/nature/saguaro-growth.htm (200, updated 19 Dec 2024) | **y** | **y** — US government work, public domain. |
| S16 Bentley et al. 2013 | DOI 403. Abstract via https://api.crossref.org/works/10.1111/ele.12127 (200) | DOI n / Crossref **y** | **y** — © Wiley; only the abstract was used, as E1 says. |
| S17 Orchard review (PMC12753925) | https://pmc.ncbi.nlm.nih.gov/articles/PMC12753925/ | **y** (200) | **y** — open on PMC; the review itself is CC BY. Secondary citation, as E1 says. |

## Method notes

- Priority was given to species tables and the cross-species rules (Chave, crown exponents, pipe rule, phyllotaxis), then to ⚠ rows.
- A paywalled publisher DOI that still has an open author or government copy was treated as VERIFIED when that copy stated the number. The copy used is named in the row and source tables.
- No datasets were downloaded into a repo. No numbers were invented. A dead link without a readable open copy would have been UNVERIFIABLE, not a guess.
- If World Data’s pass 2 later rewrites E1, this file applies only to the 09:59:38 AEDT version named above and should be run again on the new text.

## Pass 2 re-check (2026-11-16, Australia/Sydney)

This section only. Earlier sections were not edited and still describe the 09:59:38 AEDT / 34,884-byte / 312-line file.

### Version checked (pass 2)

- Path: `/workspace/specula-research/E1-tree-form-parameters.md`
- Size: **63,251 bytes, 469 lines** (unchanged while this check was run; E1 was not edited)
- File mtime checked: **2026-11-16 10:17:56.65 AEDT (Australia/Sydney, UTC+11)**
- Scope: (1) whether Library Verify fixes V1–V5 were applied correctly; (2) every new row and new sources S18–S34, including FIA group labels by Table 1/Table 2 row order and the VicFlora licence. Pre-existing numbers that were not in this scope were not re-checked.
- Grok CLI: tried first (`~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve`; also a short `whoami`). It failed immediately with **“Error: No such device or address (os error 6)”**; `XAI_API_KEY` is unset. **No login was attempted.** Checks used curl, WebFetch, pdftotext, Crossref, Europe PMC, and a complete local copy of the Lau 2019 post-print already in `/workspace/scratch-e1/p2/` (the live WUR download this pass started with `%PDF-` but had no xref trailer). Access date: **2026-11-16**.

### Pass 2 summary

Unit of counting: one clearly bounded claim (a V1–V5 application check, a new table row, a derived-arithmetic row, the 27-row FIA block split into labels / values / conversion model, or the VicFlora licence line). Multi-number rows were checked number-by-number.

| Status | Rows | What it means |
|---|---:|---|
| **VERIFIED** | **42** | The cited source opens, and it states that number, units, population/species, or (for V1–V5) the requested correction is in the current E1 text. |
| **FIX** | **1** | A stated licence line is incomplete. Correction below. |
| **UNVERIFIABLE** | **0** | — |
| **Total pass-2 claims** | **43** | FIA contributes 3 claims covering all 27 groups (every label and every tabulated figure was compared to S24). |

**V1–V5 application.** All five requested fixes are in the current file.

1. **V1 applied.** CA black oak is now `1DownAngle 30, 1DownAngleV −30` with the note that a negative V is not a ± spread. Quaking aspen is `1DownAngle 60, 1DownAngleV −50`; black tupelo is `1DownAngle 60, 1DownAngleV −40`.
2. **V2 applied.** The phrase “seed-source range ≈ 83–91°” is gone. The ≈ 91° value is labelled as one seed source (Moldoviţa) at one site, not a trial mean. Trial means remain ≈ 84° (Avrig) and ≈ 83° (Breţcu).
3. **V3 applied.** §2.3, the oak and beech phyllotaxis rows, and §5 now use Okabe’s own wording (“is often presented”; “the phyllotaxis fraction is not a determined trait of each species”). The word “folklore” occurs only in the Pass 2 changelog describing the fix. The Nature page (S13) was re-read and matches that wording; it does not say “folklore”.
4. **V4 applied.** The *Q. alba* b1 = 1.5212 ⚠ is kept (§6 item 6; the row still notes the OCR-garbled “1 S212” and Eq. 5 1.5158).
5. **V5 applied.** S6 now cites https://repositorio.uchile.cl/handle/2250/135852 (HTTP 200) plus DOI 10.1016/j.ufug.2015.04.006 (landing 200). The handle is the right paper: Pretzsch, Biber, Uhl et al., “Crown size and growing space requirement of common tree species in urban centres, parks, and forests,” *Urban For. Urban Green.* 14 (2015) 466–479. E1 correctly says the PDF bitstream returns HTML to curl (confirmed: `Crown-size-and-growing-space.pdf?sequence=1` is an HTML page). The repository item licence is **CC BY-NC-ND 3.0 Chile**; E1’s “CC BY-NC-ND 4.0 (stated in the article)” was a pass-1 reading of the article itself and was not re-opened here (bitstream blocked). Numbers were not changed, and were not re-checked in this pass.

**Most important pass-2 finding**

1. **FIX — S33 VicFlora licence is incomplete.** §1 says only “© RBGV; facts only.” The VicFlora copyright page (https://vicflora.rbg.vic.gov.au/pages/copyright, 200) states that **text is published under CC BY 4.0** and **most images under CC BY-NC-SA 4.0**, with some third-party images retaining all rights. Facts-only use of the leaf-arrangement words is still fine; the licence line should name those terms.

**What is in good shape.** Every new numeric row that was in scope checked out against the cited table, supplement, or abstract, including all 27 FIA group labels (Table 1 has no names; Table 2 names groups 1–27 in the same order, and every n, mean DBH, UNCR mean [IQR] and CCR mean [IQR] matches Table 1). Derived Lau ratios and Ceiba/Dipteryx H and crown diameters recompute to the printed one-decimal figures. New ⚠ flags (S24 row-order labels, S20 figure-only exponents, baobab zone means) are honest. §5 updates for the new sources are consistent with what was opened.

**Notes that are not FIXes.** (a) Lau defines θ_node = 1/(2α_node + β_node) per node (Table 2); the Table 3 pseudo-medians 0.50 / 0.59 are not equal to that formula applied to the median α and β, which is expected. (b) The Organ Pipe height–age shorthand lists “10 yr; 30 yr” without the brochure’s “1 inch” and “1 foot” heights; the ages and the later figures are right. (c) Tredennick’s paper itself spells *Combretum geitynophylum* (and *Deterium*); E1’s *Detarium* is the accepted genus spelling. (d) S28 was read at npshistory.com, a third-party archive of an NPS Organ Pipe brochure; the document header is NPS / Organ Pipe Cactus NM and cites Steenbergh & Lowe 1983.

### Pass 2 row table

Columns: row id, claim (short), status, note/correction, URL checked. Access date 2026-11-16 throughout.

| Id | Claim | Status | Note / correction | URL |
|---|---|---|---|---|
| P2-V1a | CA black oak: 1DownAngle 30, 1DownAngleV −30 (V1) | VERIFIED | Applied as requested; “negative V: angle varies along the parent, not ±.” (Weber Appendix p.126 was the pass-1 source; notation only re-checked in E1.) | E1 §4.1 |
| P2-V1b | Quaking aspen: 1DownAngle 60, 1DownAngleV −50 (V1) | VERIFIED | Applied. | E1 §4.1 |
| P2-V1c | Black tupelo: 1DownAngle 60, 1DownAngleV −40 (V1) | VERIFIED | Applied. | E1 §4.1 |
| P2-V2 | Spruce ≈ 91° is one source (Moldoviţa) at one site (V2) | VERIFIED | “Seed-source range ≈ 83–91°” removed; trial means ≈ 84° / ≈ 83° retained. | E1 §4.2 |
| P2-V3 | Okabe wording in §2.3, oak, beech, and §5 (V3) | VERIFIED | “is often presented” and “the phyllotaxis fraction is not a determined trait of each species” appear in all four places. “folklore” only in the changelog. Confirmed on the Nature page. | https://www.nature.com/articles/srep15358 |
| P2-V4 | *Q. alba* b1 1.5212 ⚠ kept (V4) | VERIFIED | Still in the oak row and §6 item 6, with the “1 S212” / Eq. 5 1.5158 note. | E1 §4.1, §6 |
| P2-V5 | Pretzsch S6 replaced by uchile handle + DOI (V5) | VERIFIED | Handle 200, right paper and DOI 10.1016/j.ufug.2015.04.006, journal 14 (2015) 466–479. PDF bitstream still HTML, as E1 says. Repo lists CC BY-NC-ND 3.0 Chile (see source table). | https://repositorio.uchile.cl/handle/2250/135852 |
| P2-01 | Lau α_branch manual 0.63 (CI 0.62–0.65); TreeQSM 0.45 (0.43–0.48); n = 484 branches > 10 cm, 9 trees, DBH 61.3–97 cm, H 18.8–29.9 m | VERIFIED | Table 3 (pseudo) medians and CIs; Methods: seven *Eperua grandiflora*, one *Ormosia coutinhoi*, one *Eperua falcata*; DBH 61.3–97.0 cm, height 18.8–29.9 m. | Post-print https://edepot.wur.nl/472345 (complete copy used; see method note) |
| P2-02 | Lau β_branch manual / TreeQSM 0.07 / 0.42 (CI −0.06–0.2 / 0.31–0.54) | VERIFIED | Table 3, including the short “0.2” in the manual CI. | same |
| P2-03 | Lau θ_branch manual / TreeQSM 0.50 / 0.59 (CI 0.4–0.56 / 0.53–0.65); WBE ideal α = ½, β = ⅓, θ = 0.69 | VERIFIED | Table 3, including “0.4”. WBE class column is 0.50 / 0.33 / 0.69. θ definition in Table 2 is 1/(2α+β) *per node*; the listed medians are not that formula applied to median α, β (see note above). | same |
| P2-04 | Tree-level α, β, θ (TreeQSM / manual): 0.46 / 0.64; 0.41 / 0.05; 0.56 / 0.51 | VERIFIED | Results §3.4, text after Table 3: “average (pseudo) median exponents … were 0.46 and 0.64 for αtree, 0.41 and 0.05 for β tree, and 0.56 and 0.51 for the θtree, respectively.” | same |
| P2-05 | Derived 2^−α: manual 0.65; TreeQSM 0.73; WBE 0.71 | VERIFIED | Arithmetic from the Table 3 medians: 2^−0.63 ≈ 0.646, 2^−0.45 ≈ 0.732, 2^−0.5 ≈ 0.707, rounded to two decimals as stated. | derived from S18 |
| P2-06 | Derived 2^−β: manual 0.95; TreeQSM 0.75; WBE 0.79 | VERIFIED | 2^−0.07 ≈ 0.953, 2^−0.42 ≈ 0.747, 2^−1/3 ≈ 0.794. The “wide (β CI crosses 0 for manual)” caution is right. | derived from S18 |
| P2-07 | TreeQSM diameter error: 10–20 cm +40% (n = 160); 20–30 cm −2% (n = 67); 30–40 cm −15% (n = 26) | VERIFIED | Table 1 relative diameter errors and branch counts. (The ≥ 70 cm absolute error −1.54 ± 0.80 cm is in the same table but is not a separate E1 row.) | same |
| P2-08 | Sperry 2012 defaults: length 0.794, diameter 0.707 (n = 2; c elastic similarity, b area-preserving) | VERIFIED | Table 2: “c, branch length ratio 0·794 (elastic similarity for n = 2”; “b, branch diameter ratio 0·707 (area-preserving n = 2”. Text: c = n^−1/3, b = n^−1/2. | https://sperry.biology.utah.edu/publications/Sperry%20FuncEcol%202012.pdf |
| P2-09 | Tredennick Table 1: length ∝ D^b is MST 2/3, STRESS 1/2, GEOM 1 | VERIFIED | Table 1 image (PLoS): Length column 2/3, 1/2, 1. | https://doi.org/10.1371/journal.pone.0058241 and table image t001 |
| P2-10 | Savanna length-scaling qualitative: exponents “greater than predicted by MST”, rising with rainfall, closest to GEOM; 25 trees, 286 branches, 3 species, 3 West African sites; numbers ⚠ figure only | VERIFIED | Results: “Fitted length scaling exponents are greater than predicted by MST and increase with mean annual precipitation (Figure 2)”; Discussion: “most consistent with the predictions of GEOM (Figure 2).” Sample: “25 individual trees composed of 286 branches … *Deterium microcarpum* … *Combretum geitynophylum* … *Combretum glutinosum*” at three West African (Mali) sites. No exponent digits in the text; the figure-only ⚠ is honest. E1’s *Detarium* is the accepted spelling. | same |
| P2-11 | Sone 2009 abstract: Leonardo rule held at nodes with one shed branch, not without; 295 nodes, 13 untreated *Acer rufinerve* trees (decussate) | VERIFIED | Europe PMC abstract (DOI 10.1007/s10265-008-0177-5): “Analysis of 295 branching nodes from 13 untreated trees revealed that the da Vinci's rule held in branching nodes having one shed branch but not in the nodes without branch shedding”; decussate phyllotaxis stated. | https://www.ebi.ac.uk/europepmc/webservices/rest/search?query=DOI:10.1007/s10265-008-0177-5&resultType=core&format=json |
| P2-12 | FNA / VicFlora leaf arrangement for the listed taxa (*Acer* opposite, rarely subopposite; Fagaceae alternate, spirally arranged; *Betula* nearly 2-ranked; Salicaceae alternate, spirally arranged, opposite or subopposite in *S. purpurea*, *Salix* branching sympodial; *Picea* branches whorled, leaves borne singly; *P. sylvestris* 2 per fascicle, branches spreading to ascending; *Olea* opposite; *E. globulus* juvenile opposite “for many pairs”, adult alternate) | VERIFIED | Each phrase is on the cited FNA or VicFlora page (FNA via WebFetch; VicFlora pages 200). *Salix* page: “branching sympodial.” *E. globulus*: “Juvenile leaves sessile, opposite for many pairs … adult leaves petiolate, alternate.” | FNA URLs in S25; VicFlora taxon URLs in S33 |
| P2-13 | Coconut 5 visible leaf spirals (as in *Copernicia*); oil palm 8; *Phoenix canariensis* 13 | VERIFIED | Davis 1971: coconut and *Copernicia* “have five spirals”; African oil palm “bears eight spirals”; *P. canariensis* “thirteen spirals.” The five-spiral sentence begins at the foot of p.237 and continues with the figures on pp.238–239. | https://www.fq.math.ca/Scanned/9-3/davis1.pdf |
| P2-14 | S33 VicFlora licence: “© RBGV; facts only” | **FIX** | Incomplete. Copyright page: text **CC BY 4.0**; most images **CC BY-NC-SA 4.0**; some third-party images retain rights. Correct the §1 line; facts-only use of the arrangement words remains appropriate. | https://vicflora.rbg.vic.gov.au/pages/copyright |
| P2-15 | Feldpausch 2011 Table A2: pantropical ln H = 1.2229 + 0.5320 ln D (D cm); continent and region (β0, β1) as listed; n ≈ 40,000 (36,386 primary + 3,569 secondary); Table 2 SE 0.0151 / 0.0070; 22 countries, D ≥ 10 cm | VERIFIED | Table A2 matches every listed coefficient. Methods: “old-growth (n = 36 386) and some secondary (n = 3569)” and “22 countries”; stems “greater than 1 dm.” 36,386 + 3,569 = 39,955. Table 2 (D in dm) pantropical intercept SE 0.0151, slope 0.5320 SE 0.0070. E1 correctly uses the cm form from A2. | https://bg.copernicus.org/articles/8/1081/2011/bg-8-1081-2011.pdf |
| P2-16 | Scots pine TLS: 1st-order median 81°, IQR 62–99°, μ 82°, σ 29°; 2nd-order 69°, 52–90°, μ 72°, σ 31°; 10 trees, Loobos, H 12.5–23.8 (mean 18.6) m, DBH 22.3–32.9 (mean 28.9) cm; RMSE 9.28° / 11.59° | VERIFIED | Bekkers et al. 2025 Tables 1, 4 and 5 (Europe PMC XML). 1st-order Scots pine is from the parent axis, as E1’s §6 item 12 says. | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC11826235/fullTextXML |
| P2-17 | Douglas fir, Bechtold 2004 Eq. 3a: b0 = 5.7753, b1 = 1.0639, b2 = −0.0109; RSQ 0.51, RMSE 4.4 ft, CV 27%; n = 4088, D 5.0–68.7 in. | VERIFIED | Tables 1 and 3. LCW ft = b0 + b1·D + b2·D², D in., stand-grown trees ≥ 5.0 in. | https://research.fs.usda.gov/download/treesearch/7732.pdf |
| P2-18 | Eight western US species, Bechtold 2004 Table 3 Eq. 3a (all b0, b1, b2, RSQ, RMSE, CV, n as listed) | VERIFIED | Every coefficient and fit statistic matches Table 3; every n matches Table 1: ponderosa 1413; lodgepole 2761; subalpine fir 1262; western larch 183; quaking aspen 1383; bigleaf maple 106; red alder 409; Oregon white oak 126. | same |
| P2-19 | *Ceiba pentandra* height gMM: a = 53.6, b = 0.79, k = 23.8; 90% intervals 48.1–60.7, 0.72–0.88, 20.2–28.1 | VERIFIED | Martínez Cano et al. 2019 supplement Table S2, row ceibpe. Eq. 3 in the article is the gMM y = a D^b / (k + D^b). | https://bg.copernicus.org/articles/16/847/2019/bg-16-847-2019-supplement.pdf |
| P2-20 | *C. pentandra* crown area a = 0.288, b = 1.51 (0.143–0.576; 1.36–1.65) | VERIFIED | Same Table S2 row. | same |
| P2-21 | *C. pentandra* derived H and crown diameter at D = 50/100/200 cm: 25.7 / 33.0 / 39.4 m; 11.6 / 19.6 / 33.1 m | VERIFIED | Recomputed from the point estimates: H = 25.739, 32.964, 39.353 m; circular-crown diameter 2√(A/π) = 11.611, 19.595, 33.070 m. | derived from S22 |
| P2-22 | *Dipteryx oleifera* height gMM: a = 64.5, b = 0.80, k = 23.0 (57.4–74.4; 0.73–0.87; 19.5–26.8) | VERIFIED | Table S2, row diptpa. | same supplement |
| P2-23 | *D. oleifera* crown area a = 0.530, b = 1.47 (0.222–1.178; 1.30–1.67) | VERIFIED | Same row. | same |
| P2-24 | *D. oleifera* derived H and crown diameter at D = 50/100/200: 32.2 / 40.9 / 48.4 m; 14.6 / 24.2 / 40.4 m | VERIFIED | H = 32.155, 40.881, 48.430 m; diameter 14.566, 24.243, 40.351 m. | derived from S22 |
| P2-25 | S22 context: 162 species; 9884 trees for height, 2425 for crown area; Barro Colorado Nature Monument, Panama | VERIFIED | Abstract / §2 of the article. | https://bg.copernicus.org/articles/16/847/2019/ |
| P2-26 | Six Guyanese species, 1st-order branch angle from horizontal: medians 42, 43, 39, 45, 47, 41°; IQR printed high–low as listed; μ/σ and n = 10, 5, 4, 9, 5, 4; H 23.8–44.2 (mean 33.3) m, DBH 22–126 (mean 60.9) cm; RMSE 22.19°, R² 0.36 | VERIFIED | Table 5 (order: greenheart, kabukalli, mora, morabukea, wallaba, wamara) and Table 1. Table 5 caption: tropical 1st-order is “relative to the horizontal plane.” | Bekkers XML, as P2-16 |
| P2-27 | Same six species, 2nd-order medians 60, 55, 51, 55, 48, 57°; IQR and μ/σ as listed; RMSE 18.1° | VERIFIED | Table 5 and Table 4 (2nd-order tropical RMSE 18.1, R² 0.41). | same |
| P2-28 | *Acacia tortilis*, Arava: height 2.41 m ± 0.17 SE, n = 9; C1+C2 5.55 m ± 0.32 SE (≈ 2.8 m mean diameter) | VERIFIED | Winters et al. 2025 Results. C1 and C2 are defined as N–S and E–W canopy diameters; 5.55/2 ≈ 2.78 m. | https://www.scirp.org/pdf/oje2025152_41381718.pdf |
| P2-29 | *A. raddiana*: height 3.09 ± 0.27 m; C1+C2 5.08 ± 0.47 m (SE; n = 9 each) | VERIFIED | Same Results paragraph. | same |
| P2-30 | *A. tortilis* many stems from the base so DBH is one stem; *A. raddiana* one stem at ground level | VERIFIED | Discussion: “A. tortilis has many stems branching from the bottom, thus DBH only represents one of many branches, while A. raddiana has one stem at ground level.” | same |
| P2-31 | Baobab Assogbadjo 2006 abstract: Sudanian and Sudano-Guinean trees short; Guinean trees tall; height and number of branches correlated with genetic structure; 137 trees, 6 populations; zone means ⚠ | VERIFIED | Europe PMC abstract (DOI 10.1093/aob/mcl043): 137 individuals, six populations; “Sudanian and Sudan-Guinean zones … were short … Guinean zone, which were tall”; significant correlation of height, number of branches and capsule thickness with genetic diversity. No zone means in the abstract; the ⚠ is honest. | Europe PMC search DOI:10.1093/aob/mcl043 |
| P2-32 | Saguaro, Organ Pipe brochure: 10 yr; 30 yr; 3 ft at 50 yr; 6½ ft at 70 yr; first arm at 15–16 ft (≈ 4.6–4.9 m) at 95–100 yr; 43 ft at 200 yr | VERIFIED | Brochure timeline: 1 inch / 10 yr; 1 foot / 30 yr; 3 ft / 50 yr; first flowers 6 1/2 ft / 70 yr; first arm 15–16 ft / 95–100 yr; full height 43 ft / 200 yr. 15–16 ft ≈ 4.57–4.88 m. The 10- and 30-year heights are omitted from E1’s shorthand but the listed figures are right. §6 correctly says these figures are in S28, not in the Drezner abstract. | https://www.npshistory.com/publications/orpi/brochures/saguaro.pdf |
| P2-33 | Drezner 2003 site growth factors: Saguaro NP East 1.0; SNP West 0.743; Organ Pipe 0.617 | VERIFIED | Abstract (Crossref and Europe PMC PMID 21659186). No arm height in the abstract, as E1 says. | https://api.crossref.org/works/10.3732/ajb.90.6.911 |
| P2-34 | FIA §4.7 group labels 1–27 assigned by Table 1 row order to Table 2 names | VERIFIED | Table 1 has no group names (27 numeric rows). Table 2 lists groups 1–27 in the same layout: Loblolly and shortleaf pines … Eastern noncommercial hardwoods. Every E1 label matches Table 2 in that order. The ⚠ that labels were assigned by row order is honest, but the assignment is correct. | https://research.fs.usda.gov/download/treesearch/61347.pdf |
| P2-35 | All 27 FIA rows: n, mean DBH (in.), UNCR mean [IQR], CCR mean [IQR] | VERIFIED | Every figure matches Westfall et al. 2019 Table 1. Also checked: DBH ≥ 1.0 in., 24 northern US states; “minima of 0.01–0.10 and maxima of 0.85–0.99 for every group” describes the across-group range of those extrema. | same |
| P2-36 | Conversion: UNCR = 1/(1 + exp(β0 + β1·CCR + β2·ln DBH)); beech β0 0.6889, β1 −4.6325, β2 0.3158; cottonwood/aspen 1.6683, −4.4202, 0.0402 | VERIFIED | Model (2)/(4): UNCR = 1 / (1 + exp(β0 + β1 CCR + β2 Ldbh)); Ldbh is “natural logarithm of dbh.” Table 2 group 17 Beech and group 21 Cottonwood and aspen match the stated betas. | same |

### Pass 2 source table (S18–S34, plus the new Pretzsch link)

| Key | URL checked | Resolves? | Licence correct? |
|---|---|---|---|
| S6 Pretzsch (new) | https://repositorio.uchile.cl/handle/2250/135852 (200); DOI https://doi.org/10.1016/j.ufug.2015.04.006 (landing 200); bitstream `Crown-size-and-growing-space.pdf?sequence=1` | **y** (handle and DOI landing); PDF bitstream **n** (HTML, as E1 says) | **mostly y** — article CC BY-NC-ND 4.0 was a pass-1 reading. The Chile record describes the item as **Atribución-NoComercial-SinDerivadas 3.0 Chile** (CC BY-NC-ND 3.0 Chile). Same family of terms; version differs. Right paper confirmed. |
| S18 Lau et al. 2019 | DOI landing only. Post-print https://edepot.wur.nl/472345 (HTTP 200). The body fetched this pass was a truncated PDF (no xref); a complete copy already at `/workspace/scratch-e1/p2/lau2019.pdf` (5,846,997 bytes, valid trailer) was used for text. | **y** (200; use a complete copy) | **y** — post-print: “non-commercial no derivatives Creative Commons (CC-BY-NC-ND).” |
| S19 Bekkers et al. 2025 | PMC HTML was a short stub (20,557 bytes). Full text: Europe PMC XML https://www.ebi.ac.uk/europepmc/webservices/rest/PMC11826235/fullTextXML (200) | **y** (XML) | **y** — “Creative Commons Attribution License (https://creativecommons.org/licenses/by/4.0/).” |
| S20 Tredennick et al. 2013 | https://doi.org/10.1371/journal.pone.0058241 (200). Table 1 is an image; large PNG opened. | **y** | **y** — “Creative Commons Attribution License.” E1’s “CC BY” is right (PLoS; version string not on the licence sentence I saw). |
| S21 Sperry et al. 2012 | https://sperry.biology.utah.edu/publications/Sperry%20FuncEcol%202012.pdf (200) | **y** | **y** — © 2012 Authors / British Ecological Society; author PDF; numbers as facts. |
| S22 Martínez Cano et al. 2019 | https://bg.copernicus.org/articles/16/847/2019/ (200); supplement PDF (200) | **y** | **y** — “Creative Commons Attribution 4.0 License.” |
| S23 Winters et al. 2025 | https://www.scirp.org/pdf/oje2025152_41381718.pdf (200) | **y** | **y** — “Creative Commons Attribution International License (CC BY 4.0).” |
| S24 Westfall et al. 2019 | https://research.fs.usda.gov/treesearch/61347 (200); PDF https://research.fs.usda.gov/download/treesearch/61347.pdf (200) | **y** | **y** — “This work is written by (a) US Government employee(s) and is in the public domain in the US.” |
| S25 FNA | Listed taxon pages, opened via WebFetch (curl often 403/500, as E1 says) | **y** (WebFetch) | **y** — © Flora of North America Association; facts / short quoted terms only. |
| S26 Bechtold 2004 | https://research.fs.usda.gov/download/treesearch/7732.pdf (200) | **y** | **y** — USFS author (US government work); journal reprint. Cite numbers, do not redistribute the PDF, as E1 says. |
| S27 Feldpausch et al. 2011 | https://bg.copernicus.org/articles/8/1081/2011/bg-8-1081-2011.pdf (200) | **y** | **y** — “CC Attribution 3.0 License.” |
| S28 NPS Organ Pipe brochure | https://www.npshistory.com/publications/orpi/brochures/saguaro.pdf (200) | **y** | **y** — the PDF is an NPS Organ Pipe Cactus NM brochure (US government work / public domain). Host is a third-party historical archive, which E1 names. |
| S29 Drezner 2003 abstract | https://api.crossref.org/works/10.3732/ajb.90.6.911 (200); Europe PMC PMID 21659186 (200) | **y** (abstract) | **y** — copyrighted (BSA/Wiley); abstract only, as E1 says. |
| S30 Davis 1971 | https://www.fq.math.ca/Scanned/9-3/davis1.pdf (200) | **y** | **y** — © Fibonacci Association; facts only. |
| S31 Jenkins et al. 2003 | https://research.fs.usda.gov/download/treesearch/6996.pdf (200). Opened; not used for form, as E1 says. Numbers not re-checked. | **y** | **y** — US government work. |
| S32 Assogbadjo et al. 2006 abstract | Europe PMC API search DOI:10.1093/aob/mcl043 (200). Full text not opened. | **y** (abstract) | **y** — copyrighted (OUP); abstract only, as E1 says. |
| S33 VicFlora | Copyright https://vicflora.rbg.vic.gov.au/pages/copyright (200); *Olea* and *E. globulus* taxon pages (200) | **y** | **n** — E1 says “© RBGV; facts only.” The copyright page states **text CC BY 4.0** and **images mostly CC BY-NC-SA 4.0**. See P2-14. |
| S34 Sone et al. 2009 abstract | Europe PMC API search DOI:10.1007/s10265-008-0177-5 (200) | **y** (abstract) | **y** — copyrighted (Springer / BSJ); abstract only, as E1 says. |
