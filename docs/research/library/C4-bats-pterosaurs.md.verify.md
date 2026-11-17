# C4 verification: bats and pterosaurs

Verifier: Library Verify (adversarial re-check). Written 2026-11-17, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It is a check of `C4-bats-pterosaurs.md`. C4 itself was not edited.

## Version checked

- Path: `/workspace/specula-research/C4-bats-pterosaurs.md`
- Size: **63,274 bytes**, **393 lines**
- File mtime: **2026-11-17 08:55:48 AEDT (Australia/Sydney, UTC+11)** — matches the stamp given at the start of the check (08:55:48, 63,274 bytes). Snapshot copied to `/tmp/lv17c4/`. Re-stat before writing: **unchanged**.
- **Grok CLI live-search pass:** one pass, as required: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve --prompt-file /tmp/lv17c4/grok_prompt.md --output-format plain` (web search left on). Started **2026-11-17 09:18:43 AEDT**. Result in the Grok section below. A Grok claim was treated as support only when it named a URL that I then opened myself.
- Independent checks used curl (browser UA), WebFetch, Crossref, NCBI E-utilities, Europe PMC XML, official journal/repository PDFs, and PLOS HTML/PDF. Access date for live URLs: **2026-11-17**. Pirate mirrors were not used.
- KU ScholarWorks handle and bitstream URLs returned HTTP 403 to curl from the box; the official bitstream cited in C4 was opened with WebFetch and the text matches *Allometry in pterosaurs* (Paper 105).

## Summary

Unit of counting: **one data row** (a table row, or a clearly bounded prose claim such as the jump-technique paragraph, the Castillo range paragraph, the Castillo author-conclusion paragraph, the half-span computation, the giant-humerus paragraph, the Norberg qualitative paragraph, the Witton AR-cluster sentence, the Brower Table 4 example, the Gardiner scaling paragraph, Altenbach, and the engine rule of thumb). The source list is not in the row counts; it is in the source table. Multi-number rows were checked number-by-number; a single wrong figure makes the whole row FIX. Derived rows were recomputed from checked inputs.

| Status | Rows | What it means |
|---|---:|---|
| VERIFIED | 138 | Number, units, population, and (where given) spread match a source I opened, or a stated derivation recomputes correctly. |
| FIX | 9 | See the FIX list. None of these is a fabricated number; most come from an incomplete PMC rendering of Castillo-Figueroa Table 1, or from a mis-copied species name or family count. |
| UNVERIFIABLE | 26 | Full text not obtained this pass, or C4 already marked the row Unverified and I agree. |

**173** data rows in total (162 table rows + 11 bounded prose claims: jump technique; Castillo ranges; Castillo author conclusion; half-span percentages; giant humeri; Norberg qualitative findings; Witton AR/limb-ratio cluster; Brower Table 4 example; Gardiner scaling paragraph; Altenbach; engine rule of thumb).

### Worst problems

1. **Castillo-Figueroa 2020 Table 1 is complete in the official journal PDF (97 species), not 85.** C4 counted 85 rows on the PMC HTML and computed guild medians from 82 species after dropping three internally inconsistent rows. The version-of-record PDF at `https://zoolstud.sinica.edu.tw/Journals/59/59-60.pdf` (opened with the issue-page referer, 14 pages, © 2020 Academia Sinica, labelled Open Access, no CC licence printed) has **97 species rows**, matching the text and the stated guild totals (BAI 23, CCF 23, CUF 18, CGN 10, UAI 6, …). After the same three exclusions the official table leaves **94** species, not 82. Several guild *n* values and a few medians therefore need correction (see FIX list). The three “impossible” rows C4 flagged are real in the official table.
2. **Anthwal et al. 2023: “11 families” is wrong.** Table 1 and the text say the 16 species “span **nine** families”. Stage 15 outgrowth / stage 16 fusion in all 16 species is correct (Table 1: St15 / St16 in every row).
3. **Castillo range minima are attributed to the wrong species.** The paper’s *text* says LD3 50.31 mm and LD4 41.99 mm belong to *Eptesicus fuscus*. Official Table 1 has those two means on the *Eptesicus furinalis* row (n=1). *E. fuscus* is not in Table 1. C4 copied the text error.
4. **Wilkinson et al. 2006 and Palmer & Dyke 2010 could not be re-opened this pass** (PMC recaptcha; Royal Society HTTP 403). C4 marked those pteroid / propatagium rows Verified. I do not overturn C4’s reading, but I could not confirm the numbers myself, so they are UNVERIFIABLE here.
5. **Gardiner & Nudds 2011 Table 1 and Table 2 cell values were not recovered.** The JEB HTML (WebFetch of the official article) confirms the surrounding text (70–80°, 3.5–4.5 × body weight, N per species, 125–250 frames s⁻¹, F ∝ M^1.255±0.074 vs model 0.66, *Desmodus* outside the 95% CI for speed and force, “typically generates a force of 6.51 times their body weight”). The numeric table bodies themselves were not in the fetched HTML, and PMC table images were recaptcha-blocked, so the five species rows stay UNVERIFIABLE.

### Five close checks (requested by Creature Data)

1. **Witton 2008 Table 4 “kg” column is really grams. CONFIRMED.** I opened the LMU PDF (`zitteliana_2008_b28_08.pdf`), rotated printed p. 151, and read Table 4. The header is **“Skeletal mass (kg)”**, but the numbers are 5.43 (*Preondactylus*, body mass 0.08 kg) through **18034.65** (*Quetzalcoatlus*, body mass 259.06 kg). The same paper’s text calls the *Quetzalcoatlus* skeletal mass **“18 kg”**. Witton & Habib 2010 Table 3 reprints 2553.54 / 18034.65 beside a grams-scale comparison. C4 already logs this as Sources disagree 4. All 20 taxon rows in C4 match the table (large *Pteranodon*: b = **5.96** m, M = 36.68 kg, S = 1.990 m², AR 17.85, Q 180.82, limb ratio 4.97).
2. **Pteranodon wingspan 5.43 m vs 5.96 m. CONFIRMED as C4 states.** Witton 2008 Table 4 large morph is **5.96 m / 1.990 m² / AR 17.85 / Q 180.82 / 36.68 kg**. Witton & Habib 2010 Table 1, citing Witton 2008 [24], prints **5.43 m / 1.60 m² / AR 18.42 / Q 224.79 / 36.68 kg**; Table 3 again uses 5.43 m (with the original skeletal mass 2553.54). The *Quetzalcoatlus* rows match between the two papers. Use the 2008 table.
3. **Castillo-Figueroa 2020: three impossible rows, 97 vs 85 species, unconfirmed licence. CONFIRMED, with a correction.** Official Table 1: *Artibeus lituratus* FL = LD3 = 68.38 mm and LD4 = 105.40 mm (LD3 < LD4); *Phylloderma stenops* LD3/FL = 1.26 (and its LD3/LD4/LD5 means are identical to *Lophostoma silvicolum*); *Platyrrhinus lineatus* LD3 = LD4 = 100.40 mm. *Molossops temminckii* and *Rhinophylla alethina* have LD4 = LD5 exactly; *Eptesicus furinalis* has the lowest LD3/FL (1.36). **The official PDF has 97 species, not 85** (see Worst problem 1). Licence: the PDF is labelled Open Access and **© 2020 Academia Sinica, Taiwan**; no CC BY (or other) statement. Crossref returns 404 for doi:10.6620/ZS.2020.59-60. C4’s “unconfirmed licence, facts only” is correct. Published date in the PDF is **20 November 2020** (the journal issue page also shows 15 October 2020).
4. **Vampire-bat jump force 9.5× vs 6.51×. CONFIRMED as C4 states, with the full Schutt text still unopened.** Schutt et al. 1997 abstract (NCBI E-utilities, PMID 9359889): “Peak vertical force can reach **9.5 times body weight** in approximately **30 ms**. Mean impulse is **0.0580±0.007 N s** (mean ± s.d., **N=12**), which accelerates the animal to a mean take-off velocity of **2.38±0.24 m s−1**.” Pectoral limbs generate thrust; hindlimbs orient; thumbs extend the force pulse. Gardiner & Nudds 2011 Introduction (JEB HTML opened): “Schutt and colleagues found that *D. rotundus* **typically generates a force of 6.51 times their body weight**,” later “over 6 times”. The abstract does not contain 6.51. I could not open the Schutt full text (JEB HTTP 403), so I cannot confirm whether 6.51 is a mean peak force in newtons or a body-weight multiple. C4 already logs this as Sources disagree 6. Do not treat 6.51× as a primary Schutt figure until the full paper is opened.
5. **Elgin & Hone 2020 licence CC BY-NC-SA, facts only. CONFIRMED.** Official PDF `https://palaeo-electronica.org/content/pdfs/741.pdf`: “This is an open access article distributed under the terms of Attribution-NonCommercial-ShareAlike 4.0 International (**CC BY-NC-SA 4.0**)”. Non-commercial: facts may be cited; do not copy the table into a commercial product. Table 1 bone lengths and half-spans (SMNK PAL 6990 FL = 1120 mm, WF = 654 mm; *Cycnorhamphus* GPIT 80 FL = 678 mm, WF = 415 mm) match C4; all six ratio rows recompute correctly; 654/1120 = 58% and 415/678 = 61%.

### Dead or blocked links (none of the cited URLs is a true 404 dead link)

| URL | HTTP from the box (2026-11-17) | Note |
|---|---|---|
| https://kuscholarworks.ku.edu/handle/1808/3677 and the cited bitstream | **403** to curl | Content opened via WebFetch of the same bitstream. Wayback handle snapshot exists but the box got HTTP 500 fetching it. |
| http://hdl.handle.net/1808/3677 | **500** | C4 already replaced this with the KU handle page. |
| https://europepmc.org/article/PMC/PMC10161559 and the two other europepmc.org rows | **403** | C4 already switched the table column to PMC equivalents. Europe PMC XML at ebi.ac.uk worked for several OA articles. |
| https://royalsocietypublishing.org/doi/10.1098/rstb.1987.0030 (and the 2006/2010 Proc B pages) | **403** | Norberg abstract recovered from Crossref. Wilkinson and Palmer full text not opened. |
| https://doi.org/10.1002/ar.23740 | **403** (Wiley) | Paywalled, as C4 says. Crossref title confirms “Assessment of the Hindlimb Membrane Musculature of Bats”. |
| https://doi.org/10.1130/0-8137-2376-0.1 | **403** (GeoscienceWorld) | Paywalled, as C4 says. |
| https://pubmed.ncbi.nlm.nih.gov/9359889/ | **203** (cookie wall in the HTML) | Abstract recovered with NCBI E-utilities. |
| Many pmc.ncbi.nlm.nih.gov HTML/PDF URLs | 200 then a recaptcha challenge page (~20,550 bytes) | Opened instead via Europe PMC XML, official journal PDFs, or PLOS. |
| https://doi.org/10.1038/371062a0 | 200 to a cookies-not-supported Nature page | Full text not opened; C4 already Unverified. |

No cited link returned a clean 404.

## FIX list (corrected value + source)

| Row | C4 claim | Correction | Source opened (2026-11-17) |
|---|---|---|---|
| L28 Plagiopatagium (Anthwal) | Population “16 species, 11 families” | **16 species, 9 families.** Outgrowth St15 and fusion St16 in all 16 rows is correct. | Europe PMC XML https://www.ebi.ac.uk/europepmc/webservices/rest/PMC10161559/fullTextXML (Table 1; text: “These sixteen species span nine families”) |
| L64 Castillo ranges (prose) | LD3 50.31 mm and LD4 41.99 mm (*Eptesicus fuscus*) | Those two means are on the ***Eptesicus furinalis*** row (n=1) in official Table 1. *E. fuscus* is not in the table. FL 28.64 mm (*Rhogeessa io*)–88.52 mm (*P. hastatus*), LD3 to 176.2 / 176.205 mm (*N. leporinus*), LD4 to 132.67 mm, LD5 33.81 mm (*C. planirostris*)–120.22 mm (*C. auritus*) are correct. The paper text itself has this *fuscus* error, and also spells “Rhogessa io”. | Official PDF https://zoolstud.sinica.edu.tw/Journals/59/59-60.pdf (Table 1, pp. 4–6; Results p. 3) |
| L85 Guild BAI | n=**22**; LD3/FL 1.75 [1.36–2.18]; LD3/LD5 1.39 [1.26–1.62] | Official table has **23** BAI (none of the three exclusions is BAI). Medians from all 23: LD3/FL **1.74** [1.36–2.18], LD4/FL 1.36 [1.06–1.58], LD5/FL 1.25 [0.99–1.41], LD3/LD5 **1.40** [1.26–1.62]. | same official Table 1 |
| L87 Guild CCF | n=**19**; LD3/FL **2.17** [2.03–2.34]; LD3/LD5 **1.45** [1.42–1.54] | Official CCF = 23, minus *A. lituratus* and *P. lineatus* = **21**. Medians: LD3/FL **2.19** [2.03–2.34], LD4/FL 1.57 [1.48–1.65], LD5/FL 1.49 [1.40–1.54], LD3/LD5 **1.46** [1.42–1.54]. | same |
| L88 Guild CUF | n=**15** | Official CUF = 18, minus *Phylloderma stenops* = **17**. Medians agree with C4 to the stated rounding (LD3/FL 2.14 [2.07–2.45], and the other three ratios as printed). | same |
| L89 Guild CGN | n=**9**; LD3/FL **2.05**; LD5/FL **1.33**; LD3/LD5 **1.53** | Official CGN = **10** (no exclusions). Medians: LD3/FL **2.04** [1.84–2.19], LD4/FL 1.46 [1.34–1.51], LD5/FL **1.36** [1.24–1.43], LD3/LD5 **1.52** [1.39–1.66]. | same |
| L90 All species (Castillo) | **82** species; LD5/FL 1.41 [0.99–1.72]; LD3/LD5 1.44 [**1.19**–2.00] | Official table minus the same three rows = **94** species. Medians: LD3/FL 2.03 [1.36–2.45], LD4/FL 1.50 [1.06–1.72], LD5/FL **1.43** [0.99–1.72], LD3/LD5 1.44 [**1.17**–2.00]. | same |
| L92 Author conclusion (prose) | “aerial foragers in uncluttered or background space (BAI, UAI) have a higher LD3/FL than gleaners” | Too strong for **BAI**. Official medians: UAI LD3/FL 1.98, BAI **1.74**, CCF **2.19**, CUF 2.14. BAI is below the frugivore gleaners on LD3/FL. What the official table clearly supports is a **short digit V** in UAI (LD5/FL ≈ 1.03) and a high LD3/LD5, plus UAI (not BAI) having a reasonably high LD3/FL. | same official Table 1, vs Results/Discussion wording |
| L308 *Argentavis* take-off wording | “can take off running down a 10° slope with a 5 m s−1 headwind” | The paper has two neighbouring 5 m s−1 figures. A **headwind ≈ 5 m s−1** helps a gentle down-slope take-off. Separately, simulated take-off runs on a **10° slope** use a **running speed of 5 m s−1**, and “takeoff down a short 10° sloping surface should be possible, **even with little wind**.” Do not merge those into one 10° + 5 m s−1-headwind condition. Standing take-off and continuous flapping are correctly ruled out. Table 1 (70 kg, 687 N, 8.11 m², 7 m, AR 6.04, b/l 5.56, Q 84.6 N m−2), cruising speed 67 kph, gliding angle close to 3°, and min power 600 W ≈ 3.5× available 170 W are correct. Condor row (L309) is correct. | https://pmc.ncbi.nlm.nih.gov/articles/PMC1906724/ (Table 1 and text) |

UAI (L84, n=6) and BTP (L86, n=2) were recomputed from the official table and **stand** (not FIX). *Chiroderma salvini* and *Enchisthenes hartii* have identical official means (51.11, 115.94, 80.07, 75.24) with different n and SDs — another possible Table 1 paste error, not used in C4’s example species.

## Row table

Status key as in the Summary. Grouped verified blocks are one line per block; FIX and UNVERIFIABLE rows are listed individually. URL is the copy I opened.

| Row | Claim (short) | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| L26, L27, L29 | Anthwal membranes: propatagium neck–digit I; dactylopatagium II–V (loss of interdigital apoptosis); uropatagium hindlimbs and tail | VERIFIED | Wording matches the Background. | Europe PMC XML PMC10161559 |
| L28 | Plagiopatagium: flank outgrowth then fusion; 16 species, **11 families** | **FIX** | Attachment/sequence wording is correct (St15 outgrowth / St16 fusion in all 16 Table 1 rows). Population is **9 families**, not 11. | same, Table 1 and text |
| L30 | Argoitia Table 1, 40 landmarks; 25 = foot; 39 = calcar tip; 40 = end of uropatagium; 6 molossids, Argentina | VERIFIED | Landmark 25 “Union between the plagiopatagium membrane and foot”; 39 “End of the calcar”; 40 “End of the uropatagium membrane”; dactylopatagium junctions at distal phalanges III–V present. CC BY 4.0. | Europe PMC XML PMC12690267 |
| L31, L33 | Morgan Fig. 2: *M. grisescens* to tarsus, *M. lucifugus* “and others” to toe bases; keeled calcar *M. sodalis* & *M. leibii*; unkeeled *M. lucifugus* & *M. austroriparius* | VERIFIED | Caption matches exactly. Official TTU PDF. Copyright: Museum of Texas Tech University; some authors have CDC addresses; no open licence. | https://www.depts.ttu.edu/nsrl/publications/downloads/OP360.pdf (Fig. 2 caption) |
| L32 | Calcar description, Stanchak & Santana 2018 | UNVERIFIABLE | Wiley 403; C4 already Unverified. Crossref title confirms the 2018 paper. | https://doi.org/10.1002/ar.23740 (blocked) |
| L34 | Norberg & Rayner 1987 Fig. 2 / tip indices, N=257 | UNVERIFIABLE | RS 403; C4 already Unverified. Abstract (Crossref) defines the tip-shape index and hand-wing vs arm-wing idea but I did not see Fig. 2 or N=257. | https://royalsocietypublishing.org/doi/10.1098/rstb.1987.0030 (403); Crossref abstract |
| P1 (L36) | Jump from wrists, feet often already off; wing muscles; *Desmodus* role split | VERIFIED | Gardiner Summary + Methods/Results; Schutt abstract. | JEB Gardiner HTML; Schutt PMID 9359889 via eutils |
| L42–L44 | Elgin et al. 2011: brachiopatagium to lower leg or ankle where complete, proposed universal; no conclusive torso/hip; knee only Vienna *Pterodactylus* NHMW 1975/1756; *Eosipterus* mid-tibia, possibly ankle; ankle attachment still allows high AR | VERIFIED | Abstract and body agree. Specimen list including SMNK PAL 3830, 3855, 3900, 6409 is in the later passage (an earlier “e.g.” typo in the paper says 6404). Printed pages cover the mid-tibia remark. No licence statement in the PDF (Crossref also has none). | https://app.pan.pl/archive/published/app56/app20090145.pdf |
| L45 | Witton 2008 default ankle-attached broad-chord wings; pelvis in no specimen; knee only ambiguous Solnhofen *Pterodactylus*; 20 morphs / 19 genera | VERIFIED | §2.2.1 on printed pp. 148–149 (C4’s 147–148 is a page early for the section header, which is on 148; the conclusion is on 148–149). | LMU PDF https://epub.ub.uni-muenchen.de/12010/1/zitteliana_2008_b28_08.pdf |
| L46–L49, L50–L51, L245 | Wilkinson 2006 membranes, pteroid angles, CL,max 2.4, L:D 18.1, chords; Palmer & Dyke 2010 medial pteroid, material values, M = 0.4526·b^2.4232 | UNVERIFIABLE | PMC recaptcha and Royal Society 403. C4 marked these Verified; I could not re-open the papers. | PMC1560000 / PMC2842769 (blocked this pass) |
| L52–L54 | Unwin & Bakhurina 1994; Chatterjee & Templin 2004; Bennett 2001 | UNVERIFIABLE | Paywalled or no legitimate open copy, as C4 says. | Nature / GSA / no open URL |
| P2 (ranges) | Castillo FL / LD3 / LD4 / LD5 ranges and species | **FIX** | Minima 50.31 / 41.99 are *E. furinalis*, not *E. fuscus*. Other endpoints correct. | Official ZS PDF Table 1 and Results |
| L70–L78 | Nine example species, means ± SD, n, computed ratios | VERIFIED | Official Table 1 matches every mean, SD and n quoted, including 176.205 and 90.115. Ratios recompute to C4’s two-decimal rounding. | Official ZS PDF |
| L84, L86 | Guild UAI and BTP medians | VERIFIED | Recomputed from the official 97-row table; n=6 and n=2 and the printed medians stand. | Official ZS PDF Table 1 |
| L85, L87–L90 | Guild BAI, CCF, CUF, CGN, and “All 82 species” | **FIX** | Official table is 97 species; after the same three exclusions, n and some medians change (see FIX list). | Official ZS PDF Table 1 (recomputed) |
| L92 | Author conclusion: BAI and UAI have higher LD3/FL than gleaners | **FIX** | Overstated for BAI (see FIX list). | Official ZS PDF Table 1 |
| L102–L113 | Brower & Veinus 1981 Table 1 RMA, a, b±SE, r, N, isometry rejected? | VERIFIED | All 12 rows match Table 1 (cm, g, RMA). SEs and r are slightly rounded in C4 (e.g. 0.01522→0.0152, 0.9885→0.989). t-values marked with a prime are the 0.05 rejections, agreeing with C4’s yes (+)/(−)/no. 16 taxa; log length on log mass. | KU bitstream (WebFetch of the URL C4 cites) |
| L121–L124 | Implied bone/humerus ratios at 10 g, 100 g, 1 kg, 15 kg | VERIFIED | Recomputed from the Table 1 coefficients; all within 0.01 of C4. Do not extrapolate above ~15 kg: largest animal in the set is *Pteranodon* 14.94 kg. | same, plus arithmetic |
| L132–L147 | Cerqueira et al. 2026 Tables 1–3 slopes, 95% CIs, R², allometry calls; N=28 / 18 / 81 of 127; SMA vs OLS | VERIFIED | Every listed b, CI, R² and +/=/− call matches Tables 1–3. N=28 Eupterodactyloidea, 18 Euctenochasmatia, 81 *R. muensteri*, 127 specimens, MB-Ruler: yes. The paper’s own name for the method is “standardized major **analysis** (SMA)”; one methods sentence still says OLS. CC BY 4.0 in the XML. Log base not stated. | Europe PMC XML PMC13645348 |
| L155–L161, P4 | Elgin & Hone 2020 Table 1 lengths and computed ratios; half-spans 1120 and 678 mm, 58% and 61% | VERIFIED | See close check 5. Humerus of MB.R.5591.1 is 117*. | https://palaeo-electronica.org/content/2020/2979-solnhofen-pterodactyloids-table and PDF 741 |
| L161 ratios | Hone & McDavid 2025 NHMUK PV OR 37002; ratios use Hu=78, left side | VERIFIED | Table: Hu 70, 78 (length, omitting plaster); ulna 103; WP1 133 as preserved; WP2 168/176. Ratios 103/78=1.32, 133/78≥1.71, 168/78=2.15, and the quoted WP3/WP4 left-side values give 1.78 and 1.74. CC BY 4.0. | Europe PMC XML PMC11700493 |
| P5 | Giant humeri: *Q. northropi* 544 mm; Brower 52 cm and *Pteranodon* 29 cm; both diaphysis widths 80 mm after crush correction (*Hatzegopteryx* was 90 mm) | VERIFIED | 544 mm and the 90 vs 80 mm correction are in Witton & Habib 2010; 52 cm and 29 cm (Lawson 1975; Bramwell & Whitfield 1974) are in Brower pp. 26–27. | PLOS PDF / KU text |
| L175–L179 | Brower Table 2 bat relations (Greenewalt 1962) and Witton eq. 15; SI conversion | VERIFIED | a, b, SE, r, N match Table 2. SI conversion b(m)≈1.09·M(kg)^0.324 and Q(N m−2)≈82·M^0.402 recomputes. Witton eq. 15 is 0.5088·b^3.0294, 102 bats, b=0.16–1.3 m. | KU text; Witton PDF printed p. 150–151 |
| L180–L183 | Norberg & Rayner 1987 Table 4 coefficients and mass range | UNVERIFIABLE | Full text not opened; C4 already Unverified. | RS 403 |
| P6 | Norberg qualitative findings (from the abstract) | VERIFIED | Crossref abstract: high speed with high wing loading; high-flying hawkers small pointed wings; gleaners short rounded tips, often short broad wings; piscivores very long wings with long rounded tips; carnivores large wing areas; speed correlates with mass, wing loading and tip proportions, not directly with aspect ratio. | Crossref abstract for 10.1098/rstb.1987.0030 |
| L193–L212, P7 | Witton 2008 Table 4, all 20 taxa; AR 6.6–22.9, basal range 5.16, pterodactyloids 14.95, cluster AR 7.3–10.4 and limb ratio 2.7–3.7; skeletal mass unit | VERIFIED | Close checks 1–2. Limb-ratio definition in the text is “humerus + ulna carpals + wing metacarpal + flight phalanges / femur + tibia” (a “+” is missing after ulna in the PDF). | Witton PDF, rotated Table 4 on p. 151, and p. 150 text |
| P8 | Brower Table 4 *Pteranodon ingens* spec. 27: 14,940 g, 25,300 cm², 695 cm, AR 19.09, Q 0.5904 g cm−2 (=57.9 N m−2) | VERIFIED | Table 4 line “27 14,940 25,300 695.0 19.09 0.5904”. 695²/25300=19.09; 14940/25300=0.5905. 0.5904×98.1=57.9. | KU text, Table 4 |
| L222–L228 | Witton & Habib 2010 Table 1 planforms | VERIFIED | Every cell matches Table 1, including the 5.43 m Witton 2008 *Pteranodon* row and both *Diomedea exulans* rows. CC BY (XML: CC BY 4.0). | PLOS PDF https://journals.plos.org/plosone/article/file?id=10.1371/journal.pone.0013982&type=printable |
| L234–L244 | Brower Table 2 pterosaur / hang-glider eqs; Witton eqs 8–14 | VERIFIED | All coefficients, r, N=16 / 99, and eqs 8–14 (including the three “light” fits as quoted, 96 birds, b=0.089–3.5 m) match. AR vs mass r=0.07891, C4’s 0.079. | KU text; Witton PDF |
| L255–L257, L259 | Schutt 1997 abstract: 9.5× in ~30 ms; impulse 0.0580±0.007 N s; 2.38±0.24 m s−1; N=12; role split | VERIFIED | Close check 4. | PMID 9359889 via eutils |
| L258 | Gardiner secondary “typically 6.51 × body weight” | VERIFIED | Exact sentence in the Introduction. See close check 4 and Sources disagree 6. | https://journals.biologists.com/jeb/article/214/13/2182/10319/… |
| L260 | Take-off angle 70–80°, no species difference; 5 insectivorous species | VERIFIED | Results: “no detectable difference… typically jumping at an angle between 70 and 80 deg from horizontal.” | same JEB HTML |
| L266–L270 | Gardiner Table 1 species means | UNVERIFIABLE | N per species, 125–250 frames s−1, and the 3.5–4.5 × BW *text* are in the HTML. The Table 1 cells (e.g. 4.68, 3.37) were not in the fetched page and PMC images were blocked. C4’s Sources disagree 7 (text 3.5–4.5 vs table 3.37–4.68) could not be re-checked on the table side. | JEB HTML (tables missing); PMC3110502 recaptcha |
| P9 (scaling) | Forearm ∝ M^0.342±0.028 (r² 0.981) etc.; predicted force exponent 0.666; *Desmodus* outside 95% CI for speed and force, inside for forearm | UNVERIFIABLE (partly supported) | Text confirms F ∝ M^1.255±0.074 vs model **0.66** (C4 writes 0.666), and that *Desmodus* is outside the 95% CI for speed and force but within it for forearm length. Table 2 exponents and r² for forearm, height and speed were not seen. | same |
| P10 | Altenbach 1979 film data | UNVERIFIABLE | C4 already Unverified. | no open URL used |
| L280–L284 | Habib 2008: humerus vs femur strength, 500 g, Cubo & Casinos masses, Tables 1–2 exponents; three pterosaur specimens ~5 m / ~3 m | VERIFIED | Fig. 1 caption and text: femur strength exceeds humerus above ~500 g. Closest bird *Phaethon rubricauda*, p<0.001. Cubo masses 0.351/0.160 g at 100 g and 36.264/32.143 g at 10 kg. Table 1 and Table 2 match. No per-species ratio printed. © 2008 BSPG; no open licence. | https://epub.ub.uni-muenchen.de/12011/1/zitteliana_2008_b28_09.pdf |
| L285–L289 | Witton & Habib 2010 RFF Table 2; launch narrative; 200–250 kg “could easily launch” (unpublished data); bipedal 70 kg / 85 kg as reported; Flight program 13.3 / 8.80, 24.9 / 16.3, 48.3 m s−1, 1 m s−1 climb, 2.88 km, CL 1.8/2.2, flap:glide 0.2, span stop 80% | VERIFIED | Table 2 RFF values match (2.66, 1.90, 2.69, 2.82/2.62/2.25, femur 0.30×). Results and methods match the flight-program figures. The “could easily launch” sentence is explicitly tied to unpublished data, as C4 says. | PLOS PDF |
| L290 | Press “launch in under 1 s”; 120 / 90 km h−1 | UNVERIFIABLE | C4 already Unverified; not in the papers I opened. | no paper URL |
| L298–L301 | Sato et al. 2009 Table 1, 41 kg (CI 26–75, 100,000 replicates)=5.1 m; ~12 kg albatross; 93 kg / 276 kg | VERIFIED | Table 1 coefficients and CIs match (HF 6.1, 5.9–6.4, −0.30, −0.33 to −0.27, etc.). n=26 birds of 5 species (flapping) and 22 of 4 (wing size). CC BY 4.0. | PLOS PDF / Europe PMC XML PMC2670537 |
| L302–L306 | Largest reliable size 10–11 m, 200–250 kg; 13 m / 544 kg overestimates; density 2.235 g cm−3; Henderson 544 kg, torso 1.8 m said 2.77× too long (~0.65 m) → ~240 kg; slimmer 198 vs 474 kg; 13 m almost twice a 10 m animal; Witton mass range 0.035 kg (b=0.352 m) to 259 kg (b=9.64 m) | VERIFIED | All present in S16 / S8 as reported. Henderson’s own tables were not opened (C4 already flags that). | PLOS PDF; Witton PDF §3.1 |
| L307 | Brower *Q. northropi* 74.54 kg from 11.43 m; Q 0.8888 g cm−2; S 8.386 m²; scaled *Pteranodon* 86.13 kg / 8.136 m² (52/29) or 66.45 kg / 6.844 m² (11.43/6.95) | VERIFIED | pp. 26–27. | KU text |
| L308–L309 | *Argentavis* Table 1 and condor; no standing take-off / continuous flap | **FIX** (wording of the 10° / 5 m s−1 take-off only) | Table 1 and the no-standing / no-continuous-flap claims are correct, including condor 9.5 kg, 1.32 m², 2.74 m, AR 5.7, Q 70.6, 17 m s−1. Correct the 10°/5 m s−1 combination (FIX list). © 2007 NAS; no CC licence. | PMC1906724 HTML (Table 1 and text) |
| L310 | Bustards up to 22 kg as a bird-flight limit, as cited in S16 | VERIFIED (as cited) | S16 cites Henderson 2010 this way. Henderson’s paper was not opened. | PLOS PDF |
| L311–L312 | Pennycuick ~12 kg; Naish et al. 2021 *Anhanguera* humerus | UNVERIFIABLE | C4 already Unverified. The 12 kg figure I verified is Sato’s albatross figure. | no URL opened |
| P11 | Engine rule of thumb: 0.551·b^2.516 gives ~18 / 74 / 181 / 286 kg at 4 / 7 / 10 / 12 m; light ~1/3; Q≈220; Sato 41 kg (26–75) | VERIFIED | Recomputed 18.03, 73.69, 180.78, 286.00 kg. Light 0.19·b^2.48 is about 0.32–0.33 of the heavy masses. Q 223.66 is the *Quetzalcoatlus* Table 4 value. | Witton eq. 8 and Table 4; Sato Results; arithmetic |

## Source table

| Key | URL | Resolves (2026-11-17) | Licence correct |
|---|---|---|---|
| S1 Anthwal 2023 | https://pmc.ncbi.nlm.nih.gov/articles/PMC10161559/ (XML via ebi.ac.uk) | y (XML 200; PMC HTML recaptcha) | y — CC BY 4.0 (XML and Crossref) |
| S2 Argoitia 2025 | https://pmc.ncbi.nlm.nih.gov/articles/PMC12690267/ (XML) | y (XML 200) | y — CC BY 4.0 |
| S3 Morgan 2019 | C4 cites PMC6537616; official PDF https://www.depts.ttu.edu/nsrl/publications/downloads/OP360.pdf | y (TTU PDF 200, 28 pp.). PMC HTML recaptcha; efetch XML has the body. | y — no open licence; © Museum of Texas Tech University; some authors are CDC staff; facts only |
| S4 Norberg & Rayner 1987 | https://royalsocietypublishing.org/doi/10.1098/rstb.1987.0030 | **n** from the box (403). Abstract via Crossref. | y — © Royal Society; abstract used; tables Unverified |
| S5 Gardiner & Nudds 2011 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3110502/ ; official HTML https://journals.biologists.com/jeb/article/214/13/2182/10319/… | y for the official HTML (WebFetch). PMC recaptcha; curl to JEB 403. | y — © Company of Biologists; no open licence (Crossref has none) |
| S6 Schutt et al. 1997 | https://pubmed.ncbi.nlm.nih.gov/9359889/ | PubMed HTML 203 cookie wall; **abstract via eutils 200**. JEB PDF 403. | y — © Company of Biologists; abstract only, as C4 says |
| S7 Elgin, Hone & Frey 2011 | https://app.pan.pl/archive/published/app56/app20090145.pdf | y (200, 13 pp.) | y — no licence in the PDF; Crossref none. C4 correctly says current APP policy is CC BY, **not confirmed for this 2011 article** |
| S8 Witton 2008 | https://epub.ub.uni-muenchen.de/12010/ and `/12010/1/zitteliana_2008_b28_08.pdf` | y (200, 19 pp.) | y — © 2008 Bayerische Staatssammlung (the PDF itself misspells “Staassammlung”); free to read; no open licence |
| S9 Wilkinson et al. 2006 | https://pmc.ncbi.nlm.nih.gov/articles/PMC1560000/ | **n** this pass (recaptcha / 403) | not re-checked |
| S10 Palmer & Dyke 2010 | https://pmc.ncbi.nlm.nih.gov/articles/PMC2842769/ | **n** this pass (recaptcha / 403) | not re-checked |
| S11 Castillo-Figueroa 2020 | C4 cites PMC8181164; official PDF https://zoolstud.sinica.edu.tw/Journals/59/59-60.pdf | y for the official PDF (200, 3.0 MB, 14 pp., referer required). PMC HTML recaptcha; Crossref 404 on the DOI. | y — C4 is right that the licence is unconfirmed. PDF: Open Access, **© 2020 Academia Sinica, Taiwan**, no CC statement |
| S12 Brower & Veinus 1981 | https://kuscholarworks.ku.edu/handle/1808/3677 ; bitstream C4 cites | curl **403**; **WebFetch of the bitstream succeeded** and is the Paper 105 text. hdl.handle.net 500. | y — free to read; no licence statement in the text |
| S13 Cerqueira et al. 2026 | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ (XML) | y (XML 200; HTML also large/real) | y — CC BY 4.0 |
| S14 Elgin & Hone 2020 | Table page and https://palaeo-electronica.org/content/pdfs/741.pdf | y (200) | y — **CC BY-NC-SA 4.0** (non-commercial, facts only), as C4 says |
| S15 Hone & McDavid 2025 | https://pmc.ncbi.nlm.nih.gov/articles/PMC11700493/ (XML) | y (XML 200) | y — CC BY 4.0 |
| S16 Witton & Habib 2010 | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ ; PLOS PDF | y (PLOS HTML+PDF 200). PMC HTML recaptcha; XML 200. | y — CC BY / CC BY 4.0 |
| S17 Habib 2008 | https://epub.ub.uni-muenchen.de/12011/ and `/12011/1/zitteliana_2008_b28_09.pdf` | y (200, 11 pp.) | y — © 2008 BSPG; free to read; no open licence |
| S18 Sato et al. 2009 | https://pmc.ncbi.nlm.nih.gov/articles/PMC2670537/ ; PLOS PDF | y (PLOS 200; XML 200) | y — CC BY 4.0 |
| S19 Chatterjee et al. 2007 | https://pmc.ncbi.nlm.nih.gov/articles/PMC1906724/ | y (HTML 200, real article, 153,600 bytes) | y — © 2007 National Academy of Sciences; no CC licence (PNAS OA option / free to read) |

## Grok live-search pass

One pass, as required. Command: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve --prompt-file /tmp/lv17c4/grok_prompt.md --output-format plain`.

- Started: **2026-11-17 09:18:43 AEDT (Australia/Sydney, UTC+11)**
- Exited: **2026-11-17 09:42:41 AEDT**, exit code **0**, runtime 23 min 58 s (the run auto-compacted three times)
- Prompt: `/tmp/lv17c4/grok_prompt.md`
- Raw log: `/tmp/lv17c4/grok.out` (cleaned report: `/tmp/lv17c4/grok.report.txt`)

I did not take any number from Grok alone. The counts and FIX list rest on sources I opened. Where Grok agreed with a source I had already opened, that is noted as confirmation, not as independent evidence.

### Where Grok agreed with sources I opened

- Witton 2008 Table 4 (all taxa; large *Pteranodon* **5.96 m / 1.990 m² / AR 17.85 / Q 180.82 / 36.68 kg**); skeletal-mass column labelled kg but values are grams (text “18 kg” vs 18034.65); eqs 8–15; default ankle-attached model; mass range 0.035 kg (*Anurognathus*, b=0.352 m) to 259 kg.
- Witton & Habib 2010 Table 1 *Pteranodon* citing Witton 2008 as **5.43 m / 1.60 m² / AR 18.42 / Q 224.79 / 36.68 kg**; Table 3 skeletal masses 2553.54 / 18034.65 g; RFF Table 2; 544 mm humerus; 80 mm vs 90 mm diaphysis; flight-program figures; 10–11 m / 200–250 kg; density 2.235 g cm−3; Henderson 544→~240 kg; CC BY.
- Official Castillo-Figueroa findings I had from the journal PDF: 97 species, the three inconsistent rows, *E. furinalis* (not *fuscus*) owning the LD3/LD4 minima, no CC licence on the publisher page, example-species cells.
- Schutt 1997 abstract: 9.5× in ~30 ms; 0.0580±0.007 N s; 2.38±0.24 m s−1; N=12; 6.51 not in the abstract.
- Gardiner prose: 6.51 × body weight as a secondary citation; 70–80°; text 3.5–4.5 × BW; *Desmodus* 95% CI; F exponent 1.255±0.074 vs model 0.66.
- Elgin & Hone 2020 Table 1, half-spans, **CC BY-NC-SA 4.0**.
- Elgin, Hone & Frey 2011 ankle/lower-leg attachment; no CC BY on the 2011 PDF; SMNK PAL 6404 and 6409 both appear in the paper.
- Anthwal: **nine families**, not 11; St15/St16 in all 16 species; CC BY 4.0.
- Argoitia landmarks 25 / 39 / 40; Morgan Fig. 2 caption; Brower Tables 1, 2 and 4 and the *Q. northropi* estimates; Cerqueira Tables 1–3 and the SMA vs one OLS sentence; Habib Tables 1–2, 500 g, Cubo masses; Sato Table 1 and 41 kg (26–75); *Argentavis* Table 1 and the no-standing / no-continuous-flap claims; Norberg qualitative abstract.

### Grok claims I then checked in a source I opened

- **Cerqueira discussion vs Table 1 (added to Sources disagree).** One discussion sentence says Eupterodactyloidea have “negative allometry in the femur, and isometry in the skull, humerus, and tibia/tarsus.” A later discussion sentence, and Table 1, have **isometry in the femur** and **negative allometry in the tibia/tarsus**. Confirmed in Europe PMC XML PMC13645348. C4 copied the tables, not the swapped sentence — not a C4 FIX.
- **Chatterjee take-off wording.** The opened PMC HTML has both “Argentavis could take off by running downhill with a light headwind of 5 m/s” and, separately, 10°-slope simulations at a **running speed** of 5 m/s that “should be possible, even with little wind.” C4’s merged “10° slope with a 5 m s−1 headwind” remains a mild wording FIX. Grok also pointed at a PNAS commentary (doi:10.1073/pnas.0705515104) that it says uses the merged wording; I did not open the commentary and I do not treat it as the primary.
- **Argoitia type counts.** The XML says 13 type I, 6 type II and 28 type III (semilandmarks), which sum to 47, while the points are numbered L/SL 1–40. C4’s “40 landmarks” matches the numbering through landmark 40 (end of the uropatagium). I did not recount every row; this is a possible paper-internal inconsistency, not a C4 FIX.

### Grok claims I could not confirm, so they do not change any status

- Grok reported that the **PMC HTML table also has 97 species**, not 85. I could not re-open PMC8181164 (recaptcha). The official journal PDF has 97, matching the text. C4’s “PMC has 85 rows” is therefore **unconfirmed this pass**, not independently contradicted by me.
- Grok reported Wilkinson 2006 and Palmer & Dyke 2010 numbers, and a printed typo *M* = 0.4526·*S*^2.4232 for a span fit, from PMC pages I could not open (recaptcha / 403). Those C4 rows stay **UNVERIFIABLE**.
- Grok reported Gardiner Table 2 from `https://journals.biologists.com/jeb/article-pdf/214/13/2182/1436232/2182.pdf`. Curl from the box returned **HTTP 403**. Table 1/2 cells stay **UNVERIFIABLE**.
- Grok treated Brower’s abstract range “about 4 g to an estimated 75 kg” as a conflict with “15 kg.” That is not a C4 error: C4’s 14.94 kg is *Pteranodon* in Table 4, the largest specimen in the regression set. 75 kg is Brower’s *Q. northropi* estimate (C4 already records 74.54 kg from pp. 26–27).
- Grok noted Witton citing “Brower & Veinus 1983.” The Witton PDF cites both **Brower & Veinus 1981** and a separate **Brower 1983**. Not a C4 year typo.

No additional FIX or UNVERIFIABLE row was added on Grok’s word alone.

## Not found (additions this pass)

- Schutt et al. 1997 full text (force-time traces, the origin of 6.51).
- Wilkinson et al. 2006 and Palmer & Dyke 2010 full text (box blocked).
- Gardiner & Nudds 2011 Table 1 and Table 2 *cell* values (official HTML omitted the table bodies; PMC table images blocked).
- Norberg & Rayner 1987 Table 1 / Table 4 (still paywalled / 403).
- Castillo-Figueroa supplement Table S1 (per-bone mm; still not a mm table, as C4 says). I did not download it.

## Sources disagree (additions this pass)

- **Castillo-Figueroa text vs official Table 1:** the text assigns the LD3/LD4 minima to *Eptesicus fuscus*; the table’s owner of those means is *Eptesicus furinalis*.
- **Castillo-Figueroa “97 species” vs PMC “85”:** the official PDF agrees with the text (97). The 85-row PMC HTML is an incomplete rendering, not a second published dataset.
- **Witton 2008 Table 4 unit label vs text:** already in C4 item 4; I confirm it from the page image and from the “18 kg” skeletal-mass sentence.
- **Witton 2008 vs Witton & Habib 2010 *Pteranodon* planform:** already in C4 item 3; I confirm both tables.
- **Chatterjee et al. 2007 5 m s−1:** running speed on a 10° slope, and (separately) a light headwind of ≈5 m s−1. C4 had merged them.
- Everything else in C4 §7 that I could open checks out as a real disagreement, including Cerqueira “SMA” vs one OLS sentence (the paper’s expansion is “standardized major analysis”).

