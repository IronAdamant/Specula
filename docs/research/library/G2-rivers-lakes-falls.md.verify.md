# G2-rivers-lakes-falls.md: verification

Checked by: Grok Bot executor (adversarial source check). Access date for every URL below: **2026-11-17** (Australia/Sydney, AEDT).
File checked: `G2-rivers-lakes-falls.md`. mtime at start was **2026-11-17 09:04:26 AEDT**, 17,433 bytes. Re-statted before writing and found **unchanged and byte-identical to the start snapshot**. Final mtime verified against: **2026-11-17 09:04:26.398 AEDT**.

## Summary

- **Scope:** Every numbered claim in the G2 tables, the researcher's calculations, and the close-check items (Manning, Leopold & Maddock exponents, Peterka jump forms, Montgomery & Buffington slope bins, Gilbert's Niagara rate). Key scanned pages were read as images (WSP 2339 printed p. 1; Peterka printed pp. 6–7 and 15–16). USGS/USACE/USBR public-domain status was checked against the government imprints. Official URLs were fetched even where the file used a local copy.
- **Counts (rows checked: 34):** VERIFIED 31 · FIX 3 · UNVERIFIABLE 0.
- **Worst problems (page numbers only; no wrong published numbers):**
  1. **HEC-RAS eqs. (2-4)–(2-5) with 1.486 are on printed p. 2-4, not p. 2-3.**
  2. **Leopold & Maddock "b = 0.5, a typical value for many rivers" and the constructed m = 0 example at the 50% frequency are on p. 17, not p. 18.** The at-a-station means on p. 9 and the downstream 0.5 / 0.4 / 0.1 summary on p. 26 are right.
  3. **Peterka's "about 125 tests… six test flumes… discharges of 1 to 28 c.f.s." is on printed p. 5, not p. 7.** The sequent-depth formula and "agreement is excellent over the entire range" are on p. 7, as stated. Jump-form bands and names on pp. 15–16 are right.
- **Close checks:**
  - **Manning:** V = (1.486/n) R^{2/3} S_e^{1/2} and K = (1.486/n) A R^{2/3} are eqs. (1)–(2) on WSP 2339 printed p. 1 (page image). HEC-RAS 5.0 prints the same 1.486 in eq. (2-5). (1 m / 1 ft)^{1/3} = (1/0.3048)^{1/3} = 1.4859, printed 1.486. US Government work.
  - **Leopold & Maddock 1953:** at-a-station b = 0.26, f = 0.40, m = 0.34, average of 20 Great Plains / Southwest sections, "biased toward semiarid conditions," lines drawn by eye, "no statistical-fitting procedure" (p. 9). Downstream at mean annual discharge: b = 0.5, f = 0.4, m = 0.1 (p. 26). Both triples sum to 1. Public domain.
  - **Peterka EM 25:** F₁ = V₁/√(g D₁) eq. (1), p. 6; D₂/D₁ = −1/2 + √(1/4 + 2 F₁²) = ½(√(1+8 F₁²) − 1) eq. (3), p. 7. Jump forms in Fig. 9 and facing text, pp. 15–16, match the file's bands and names (slight ruffle 1.0–1.7; A pre-jump 1.7–2.5, energy loss < 20%; B transition 2.5–4.5; C good jumps 4.5–9.0, 45–70%; D effective but rough, 9 upward, may reach 85%). "Limits… are not definite values but overlap somewhat." Official https://www.usbr.gov/tsc/techreferences/hydraulics_lab/pubs/EM/EM25.pdf failed SSL on this pass; the copy in `/tmp/g2/em25.pdf` was read (front matter includes printings through May 1984 and October 1990). USBR / Department of the Interior work.
  - **Montgomery & Buffington 1993:** the PNW slope bins are on printed p. 56, as stated (Grok missed them). Pool-riffle < 0.02; plane-bed 0.01–0.03; step-pool 0.03–0.08; cascade 0.08–0.30; colluvial (this region) > 0.20. "These gradient divisions are not absolute…" Reconnaissance: source S > 0.30; transport 0.03 < S < 0.30; response **S ~ 0.03** (the file's OCR note is right). Step spacing "five to seven channel widths" typifying pool-riffle vs shorter in step-pool is around p. 31, citing Grant et al. 1990. Official PDF now at https://dnr.wa.gov/sites/default/files/2025-05/fp_tfw_sh10_93_002.pdf (200; same 1,766,212-byte file). Washington State contract report, not a US federal public-domain work. The 1997 GSA Bulletin (not used in the file) uses slightly different typical ranges (Grok reported < 0.015 / 0.015–0.03 / 0.03–0.065 / > 0.065); I did not re-open that 1997 PDF for a digit check.
  - **Gilbert 1907:** Horseshoe, 1842–1905 (63 years): 5 ft per annum, uncertainty 1 ft (p. 25 summary). Working range 4.2–5.3 ft yr⁻¹, nearer 5.3 if the 1842 map is given full weight; he selects 5 ft to avoid a false sense of precision (pp. 24–25). American Fall, 1827–1905 (78 years): less than 3 inches per annum. Diversion warning quoted correctly. Conversion 5 ± 1 ft yr⁻¹ = 1.52 ± 0.30 m yr⁻¹ and < 3 inches = < 0.076 m yr⁻¹ recomputed. USGS Bulletin; public domain.
- **Dead links:** the official USBR EM 25 URL failed SSL (unexpected EOF). All USGS report PDFs returned 200. HEC-RAS 5.0 official PDF returned 200.
- **Rows skipped:** none of the G2 number tables. Game-talk rows in §8 were already marked NOT FOUND / not opened by the researcher; I did not invent costs for them and did not re-open those slides.

## Rows

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| §2 US customary Manning | V = (1.486/n) R^{2/3} S_e^{1/2}; V in ft s⁻¹, R in ft | VERIFIED | R1 eq. (1), printed p. 1 page image | https://pubs.usgs.gov/wsp/2339/report.pdf |
| §2 conveyance | K = (1.486/n) A R^{2/3}, eq. (2) | VERIFIED | same page | same |
| §2 HEC-RAS | Q = K S_f^{1/2}, K = (1.486/n) A R^{2/3}, eqs. (2-4)–(2-5), p. 2-3 | **FIX** | Formula and 1.486 confirmed as eqs. (2-4)–(2-5). **Printed page is 2-4**, not 2-3. | https://www.hec.usace.army.mil/software/hec-ras/documentation/HEC-RAS%205.0%20Reference%20Manual.pdf |
| §2 origin of 1.486 | (1 m / 1 ft)^{1/3} = 3.28084^{1/3} = 1.4859, printed 1.486 | VERIFIED | recomputed (1/0.3048)^{1/3} = 1.48592 | calculation; not derived on R1 p. 1, as the file says |
| §2 SI companion | V = (1/n) R^{2/3} S^{1/2} not printed on the opened R1 page; some handbooks use 1.49 | VERIFIED | R1 p. 1 prints only 1.486 | R1 PDF |
| §2 pointers | Chow 1959, Henderson 1966, Streeter 1971; Barnes 1967 photographs | VERIFIED | R1 p. 1 | R1 PDF |
| §3 power functions | w = a Q^b, d = c Q^f, v = k Q^m; b+f+m = 1 because Q = w d v | VERIFIED | eqs. (1)–(3), discussed pp. 8–9 | https://pubs.usgs.gov/pp/0252/report.pdf |
| §3 at-a-station means | b = 0.26, f = 0.40, m = 0.34; 20 sections, Great Plains and Southwest, "biased toward semiarid"; drawn by eye; no statistical fitting | VERIFIED | p. 9 | same |
| §3 downstream means | b = 0.5, f = 0.4, m = 0.1 at equal frequency (mean annual discharge) | VERIFIED | p. 26 summary: downstream width slope 0.5, depth 0.4 both ways, velocity m = 0.1 downstream and 0.34 at a station | same |
| §3 check | 0.26+0.40+0.34 = 1; 0.5+0.4+0.1 = 1 | VERIFIED | | from R2 |
| §3 downstream velocity / p. 18 | b = 0.5 "a typical value for many rivers"; velocity can be unchanged (m = 0) at the 50% frequency; typically increases slowly downstream (m = 0.1) | **FIX** | Content correct. **"Typical value" and the m = 0 at 50% frequency example are on p. 17**, not p. 18 (discussion of Fig. 10). The m = 0.1 downstream / 0.34 at-a-station statement is on p. 26. | same |
| §4 F₁ | V₁ / √(g D₁), eq. (1), p. 6 | VERIFIED | page image, printed p. 6 | local `/tmp/g2/em25.pdf` (official USBR URL failed SSL) |
| §4 sequent depths | D₂/D₁ = −1/2 + √(1/4 + 2 F₁²) = ½(√(1+8 F₁²) − 1), eq. (3), p. 7; ~125 tests, six flumes, 1–28 cfs, agreed "over the entire range" | **FIX** | Formula and "agreement is excellent over the entire range" are on **p. 7**. **The 125 tests / six flumes / 1–28 c.f.s. sentence is on printed p. 5** ("Observation of the hydraulic jump throughout its entire range required tests in all six test flumes. As indicated in Table 1, this involved about 125 tests for discharges of 1 to 28 c.f.s."). | same |
| §4 F = 1 | jump cannot form | VERIFIED | p. 15 | same |
| §4 1.0–1.7 | slight ruffle; only a slight difference in conjugate depths | VERIFIED | p. 15 | same |
| §4 1.7–2.5 | A, "Pre-jump — very low energy loss"; rollers from ~1.7 to ~2.5; energy loss < 20% (Fig. 8) | VERIFIED | p. 16 page image and Fig. 9A | same |
| §4 2.5–4.5 | B, "Transition — rough water surface"; pulsating jet, irregular wave | VERIFIED | p. 16, Fig. 9B | same |
| §4 4.5–9.0 | C, "range of good jumps — least affected by tail-water variations"; 45–70% | VERIFIED | p. 16, Fig. 9C | same |
| §4 9.0 upward | D, "effective but rough"; slugs of water; may reach 85% | VERIFIED | p. 16, Fig. 9D | same |
| §4 overlap caveat | limits "are not definite values but overlap somewhat depending on local factors" | VERIFIED | p. 16 | same |
| §5 pool-riffle | S < 0.02 | VERIFIED | R4 p. 56; preliminary PNW (Montgomery, in prep.) | https://dnr.wa.gov/sites/default/files/2025-05/fp_tfw_sh10_93_002.pdf (also `/tmp/g2/mb93.pdf`) |
| §5 plane-bed | 0.01–0.03 | VERIFIED | p. 56 | same |
| §5 step-pool | 0.03–0.08 | VERIFIED | p. 56 | same |
| §5 cascade | 0.08–0.30 | VERIFIED | p. 56 | same |
| §5 colluvial (this region) | > 0.20 | VERIFIED | p. 56 | same |
| §5 reconnaissance | source S > 0.30; transport 0.03 < S < 0.30; response S ~ 0.03 | VERIFIED | p. 57 continues the sentence; the last bound is printed "S ~0.03" | same |
| §5 not absolute / overlap | quoted correctly; pool-riffle and plane-bed overlap 0.01–0.02 | VERIFIED | p. 56 | same |
| §5 step spacing | pool-riffle typified by five to seven channel widths; shorter in step-pool, citing Grant et al. 1990 | VERIFIED | around printed p. 31 | same |
| §6 Horseshoe 1842–1905 | 5 ft per annum, uncertainty 1 ft (1.52 ± 0.30 m yr⁻¹); working range 4.2–5.3, nearer 5.3 if 1842 map is given full weight; selects 5 ft to avoid a false sense of precision | VERIFIED | summary p. 25; discussion pp. 24–25. Conversion recomputed (5 × 0.3048 = 1.524). | https://pubs.usgs.gov/bul/0306/report.pdf |
| §6 American Fall 1827–1905 | less than 3 inches per annum (< 0.076 m yr⁻¹) | VERIFIED | p. 25; 3 × 0.0254 = 0.0762 | same |
| §6 diversion | "The present and prospective diversions of water… may be expected to modify the rate of recession." 5 ft yr⁻¹ offered as the *natural* rate | VERIFIED | p. 25 | same |
| R1/R2/R5/R6 public domain | US Government works | VERIFIED | USGS / USACE imprints; HEC-RAS 5.0 is a USACE manual | the official PDFs |
| R3 public domain | USBR monograph | VERIFIED | Department of the Interior / Bureau of Reclamation imprint; printings through 1984/1990 in the copy read | local EM 25 PDF |
| R4 licence | Washington State contract report | VERIFIED | not a US federal public-domain work; official DNR PDF now available | DNR URL above |

## Sources

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| R1 | https://pubs.usgs.gov/wsp/2339/report.pdf | y (200, PDF 4,813,334 B) | y (US Government work) |
| R1 DOI | https://doi.org/10.3133/wsp2339 | y (200, pubs.usgs.gov/publication/wsp2339) | y |
| R2 | https://pubs.usgs.gov/pp/0252/report.pdf | y (200, PDF 2,244,212 B) | y |
| R2 DOI | https://doi.org/10.3133/pp252 | y | y |
| R3 official | https://www.usbr.gov/tsc/techreferences/hydraulics_lab/pubs/EM/EM25.pdf | n (SSL unexpected EOF, twice) | y as a USBR government work, from the local copy's imprint |
| R4 official | https://dnr.wa.gov/sites/default/files/2025-05/fp_tfw_sh10_93_002.pdf | y (200, 1,766,212 B; same file as `/tmp/g2/mb93.pdf`) | y (Washington State contract report; file had no public URL — this one can be added) |
| R5 | https://pubs.usgs.gov/bul/0306/report.pdf | y (200, PDF 5,618,261 B) | y |
| R5 DOI | https://doi.org/10.3133/b306 | y | y |
| R6 official | https://www.hec.usace.army.mil/software/hec-ras/documentation/HEC-RAS%205.0%20Reference%20Manual.pdf | y (200, 8,671,674 B; same file as `/tmp/g2/hecras.pdf`) | y (USACE) |

## Grok

Same single G1–G3 pass as in the G1 verify file (09:21:41–09:27:48 AEDT, 2026-11-17). I opened or recomputed every Grok claim I used.

**Agreed:** USGS Manning 1.486 on p. 1; Leopold at-a-station 0.26/0.40/0.34 (n = 20) and downstream 0.5/0.4/0.1; public domain for the USGS reports; Gilbert 5 ± 1 ft yr⁻¹ and < 3 inches/yr; sequent-depth formula.

**Grok errors I did not adopt:** it failed to open EM 25 and used a 1955 Bradley & Peterka Hyd-399 PDF instead (Fig. 9 page numbers and some nicknames therefore unverified in its output). It reported the 1993 slope bins as **NOT FOUND**; they are on p. 56 of the 1993 report. It did not see 1.486 in the HEC-RAS 5.0 OCR; the constant is in eq. (2-5) on p. 2-4.
