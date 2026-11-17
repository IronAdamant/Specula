# E2 verification: leaves, bark and roots

Verifier: Library Verify (adversarial re-check). Written 2026-11-16, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It is a check of `E2-leaves-bark-roots.md`. E2 itself was not edited.

## Version checked

- Path: `/workspace/specula-research/E2-leaves-bark-roots.md`
- Size: 40,913 bytes, 393 lines
- File mtime: **2026-11-16 10:39:15 AEDT (Australia/Sydney, UTC+11)**
- Last re-read before this file was written: **2026-11-16 ~11:05 AEDT**. The research file was unchanged.
- Access date for live URLs: **2026-11-16**.

### Grok live-search pass

A Grok CLI research pass was run for this file (and for E4), after the CLI was signed in, per the grok-cli-research-pass skill:

`~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve`

Live web search was left on. The process was started about 10:47 AEDT and was waited out until it exited. **A Grok answer was treated as a lead only.** Every number below comes from a URL opened independently. Numbers were never taken from Grok alone.

**Conflicts or leads the Grok pass raised, and what was done with them:**

1. **FOFEM 5 code vs official later USDA table, index 36.** Grok reported `Fofem_Sbt[36] = 0.065` in xfblib.cpp but **0.063** in the FOFEM 6.x user-guide table. Independently confirmed: FOFEM 5.0 arrays in `xfblib.cpp` have index 36 = 0.065 (used for Douglas-fir, ponderosa pine, western larch). The official **FOFEM 6.7 User Guide** (Feb 2020, USDA, public domain), p.58, has the same index table except **index 36 = 0.063**. The FFE / RMRS-GTR-116 addendum (public domain) lists the Douglas-fir multiplier as 0.063. E2’s FOFEM **5** values are correctly taken from the code and correctly labelled. A public-domain government document with *almost* the same numbers exists, but it is a later FOFEM 6 table and disagrees at index 36. See Sources disagree. No FOFEM **5.0** user-guide table with this *v* list was found (same conclusion as Grok).
2. **Cushman et al. 2014** — Grok found a ForestGEO copy: `https://si-forestgeo01.si.edu/Public/pdfs/CushmanConditEtAl_2014_MEE.pdf`. Independently downloaded (HTTP 200, PDF). Running head: “© Published 2014. This article is a U.S. Government work and is in the public domain in the USA.” E2 cited it as [ABSTRACT] only. The claims E2 took from the abstract are all in this public-domain full text and are verified. Licence should be stated as US government work (public domain in the USA), not left as a closed Wiley paper.
3. **Mehedi et al. 2012 (MKC12)** — Grok verified the abstract figures and noted an Academia Table 1 caption saying “12 species” against the abstract’s 11. The table body was not opened here either. Abstract numbers were independently confirmed from the Springer/HEP abstract text. The 11 vs 12 caption was not independently confirmed and is **not treated as a FIX**. Full text remains closed; species-level table cells remain unopened.
4. **FOFEM 4 INT-GTR-344** — Grok opened it. Independently confirmed from `https://www.fs.usda.gov/rm/pubs_int/int_gtr344.pdf` and, on retry, `https://research.fs.usda.gov/download/treesearch/25257.pdf` (earlier curl of the treesearch URL returned a 1,313-byte stub; it now serves the real 3.9 MB PDF). Table 1 matches E2, including the “[OCR]” flag on “.I52” for western redcedar.

## Summary

Unit of counting: **one data row** (a species row in a table, a FOFEM/FVS coefficient row, a derived size-class or bark-thickness cell-set, or a clearly bounded multi-value claim such as the WHA84 worked example or the WAN25 biomass set). The licence table in §2 and E2’s own “Sources disagree” table are not in the row counts. Multi-number rows were checked number-by-number; one wrong figure makes the row FIX.

| Status | Rows | What it means |
|---|---:|---|
| **VERIFIED** | **128** | The cited source opens (or an open copy does), and it states that number, units, and population, at the level E2 claims (including [ABSTRACT] / [DERIVED] / [OCR] flags). |
| **FIX** | **4** | A licence, a derived pairing, or an access note is wrong or incomplete. Correction below. |
| **UNVERIFIABLE** | **11** | Full text not independently opened for IK14 C_sd values and VAS24 relative-bark ranges. |
| **Total data rows** | **143** | Tables in §§3–7 plus the bounded notes. |

**Most serious problems**

1. **FIX — VicFlora licence is understated.** E2 §2 says “© RBGV. Facts only (as in E1).” The official page `https://vicflora.rbg.vic.gov.au/pages/copyright` (opened) states: **text is published under CC BY 4.0**; **the majority of images are CC BY-NC-SA 4.0** (check each caption; some third-party images retain all rights). This matters for reuse of text versus images. The two taxon pages that failed curl at 10:39 AEDT (*Olea* `d89d5b58-…`, *A. pseudoplatanus* `f9f0b3aa-…`) returned **HTTP 200** on retry, and the leaf/bark wording matches E2.
2. **FIX — Cushman 2014 is not abstract-only, and it is public domain in the USA.** A ForestGEO PDF is a U.S. Government work. E2’s numbers (190 stems, BCI, more than half of biomass measured above 1.3 m, uncorrected −0.21 % yr−1 becoming +0.18 % yr−1) all appear in that PDF (summary p.573 and discussion). Keep the numbers; change the access flag and the licence line. The paper does not contain Bangladesh-style buttress height/length ranges (E2 does not claim that it does).
3. **FIX (minor) — MGP15 coppice bark-thickness range written “7.7 ± 1.5 to 8.6 ± 2.6 mm”.** Silva Fennica Table 3 (opened in HTML) for the 2nd rotation, five spacings, is: **8.5 ± 2.6, 7.7 ± 1.5, 8.6 ± 2.0, 7.9 ± 2.6, 8.6 ± 1.6 mm**. The means do run 7.7–8.6 mm, but **8.6 is not paired with ± 2.6** (± 2.6 belongs to 8.5 and 7.9). 1st-rotation **10.3 ± 2.5 to 6.6 ± 2.8 mm**, axial **12.1 mm at the base to 3.4 mm at 15.3 m**, and y = 10.987 *X*^(−0.584) (R² 0.914) all match. Licence **CC BY-SA 4.0** (Crossref), as usable.
4. **FIX — FOFEM 5 *v* list is correctly taken from all-rights-reserved code, but a public-domain USDA table now exists and disagrees at one index.** E2 already says the FOFEM 5 values come from xfblib.cpp (© 2002–2011 Collin D. Bevins, all rights reserved) and that BT at 30/60 cm is [DERIVED]. Those *v* values and the derived thicknesses recompute exactly, including the “other values” block (yellow birch 0.031, … Pacific madrone 0.060) and SEQGIG / SEQSEM both barkEq 39 → 0.081. The official FOFEM 6.7 User Guide table is public-domain and matches indices 1–35 and 37–39; **index 36 is 0.063, not 0.065**. Cite the 6.7 (or later 6.9.4) guide for a government document, and do not treat 0.065 as if it were in that official table. FOFEM 4 Table 1 remains a different functional form (intercept + slope) except Douglas-fir BTH = 0.065 · DBH, which does match FOFEM 5.

**What is in good shape.** FNA leaf dimensions, shapes, margins, petioles and bark descriptions for *Q. alba*, *Q. rubra*, *Q. robur*, *F. grandifolia* (and the *F. sylvatica* crenate-margin contrast), *B. papyrifera*, *B. pendula*, *A. saccharum*, *A. rubrum*, *P. sylvestris* var. *sylvestris*, *P. abies* and *P. menziesii* (including the variety colour key) all match the live FNA pages. VicFlora *A. pseudoplatanus*, *E. globulus* (adult and juvenile) and *O. europaea* subsp. *europaea* match, including olive “entire and recurved” and blue-gum bark “smooth, apart from base… persistent slabs… yellowish or grey.” Derived Webb size classes use MLA99’s L × W × 2/3 and the class limits in the Penn State PDF, and they recompute. FEIS coast Douglas-fir needles “about 1 inch (2.5 cm), spirally arranged” and mature bark “thick (4 to 12 inches [10–30 cm]) and corky,” with resin blisters on young bark, match. FVS NE Table 4.2.1 BRATIO values and species-group numbers from Table 3.2.1 match, as do the Hilt 1985 / Martin 1981 / Solomon 1977 source notes and the [DERIVED] single-bark thicknesses at 30 and 60 cm DOB. KPS09 Table 2 and eqs. 3.1–3.3 match (Scots pine, European larch, common oak; n, means, min–max, SD, CV, mean DBH and ages). SHE13 Table 5 means and 95% CIs match. WHA84 Table 2 spruce–fir and Table 3 softwood/hardwood coefficients, the 14-in stump → 12.1 in DBH example with ±2 SD 10.1–14.1, the 2.1–48.5 in stump range, and the [DERIVED] flare ratios at DBH 12 in (≈1.09 softwoods/hardwoods, ≈1.15 spruce–fir) all recompute; E2 honestly left the other Table 2 species rows unused because of OCR. WAN25 (PMC12078330, CC BY 4.0): 1–7 buttresses, 69.57% with 3–5, 500 trees, length > 20 cm, DBH 28.5 ± 13.3 cm, 8.5 t/ha vs 44.04 t/ha, 16.18%, and the height/length/width definitions all match. MKC12 abstract figures match and are correctly flagged [ABSTRACT]. MIT20 tree counts, DBH medians, quadrat counts and bark-pattern scores recompute exactly from the CEH zip (OGL v3); furrow medians are close and the *Q. robur* n = 126 / median 18.8 mm reproduce when all measured quadrats are included (not only those scored “fissured”). *Fagus sylvatica* leaf dimensions are correctly left in §8 as not found.

**§8 Not found and §9 Sources disagree** are present and, where checked, honest. The measured-Polish vs FOFEM/FVS Scots pine disagreement is real. FOFEM 4 vs FOFEM 5 functional-form differences are correctly described. *Q. robur* “light grey, scaly” (FNA, North American material) vs MIT20 furrowed UK trees is fairly described.

## Row table

Columns: row id, claim (short), status, note/correction, URL checked. Access date 2026-11-16 throughout.

### §3 Leaves

| Id | Claim | Status | Note | URL |
|---|---|---|---|---|
| L-Qa–L-Ol | Eleven broadleaf blade size / shape / margin / venation / petiole rows (FNA + VicFlora), plus *F. sylvatica* “not found” / crenate-margin note | VERIFIED | All numbers and descriptors checked on the live flora pages. Size classes [DERIVED] recompute from L × W × 2/3 against MLA99. *Fagus* genus venation (“each secondary vein ends in a tooth”; mapped as craspedodromous) is taken from the genus discussion as labelled. | FNA taxon URLs in E2 §3a; VicFlora taxon URLs (retried, HTTP 200) |
| L-con | *P. sylvestris* 4–6(–8) cm × 2 mm, 2/fascicle; *P. abies* 1–2.5 cm, 4-angled; *P. menziesii* 15–30(–40) × 1–1.5 mm; FEIS “about 1 inch (2.5 cm)” | VERIFIED | FNA and FEIS agree; FEIS is a typical value. | FNA V2 URLs; https://www.fs.usda.gov/database/feis/plants/tree/psemenm/all.html |
| L-class | MLA99 / Webb class limits (leptophyll &lt;25 … megaphyll &gt;164,025 mm²) and vein-category paraphrases | VERIFIED | © 1999 Smithsonian Institution, all rights reserved, as stated. | https://personal.ems.psu.edu/~pwilf/1999_MLA.pdf |

### §4 Bark type

| Id | Claim | Status | Note | URL |
|---|---|---|---|---|
| Bk1–Bk10 | Descriptive bark (oaks, beech, birches, Scots pine orange-and-platy upper trunk, Norway spruce, DF 4–12 in corky, *E. globulus* smooth with basal slabs) | VERIFIED | | FNA / FEIS / VicFlora as cited |
| Bk-euc | VicFlora eucalypt bark vocabulary (ironbark, tessellated, stringy, box, etc.) | VERIFIED | Paraphrased; not copied. | https://vicflora.rbg.vic.gov.au/pages/eucalypts |
| Lic-VIC | “© RBGV. Facts only” | **FIX** | Text **CC BY 4.0**; images mostly **CC BY-NC-SA 4.0**. | https://vicflora.rbg.vic.gov.au/pages/copyright |

### §5a–5b FOFEM and FVS bark

| Id | Claim | Status | Note | URL |
|---|---|---|---|---|
| F5-v | FOFEM 5 *v* for PSEMEN/PINPON/LAROCC 0.065, PINLAM 0.072, SEQGIG/SEQSEM 0.081, PINTAE 0.052, PINSTR 0.045, QUERUB 0.042, QUEALB 0.040, TSUCAN 0.039, PICRUB 0.034, ACESACU 0.033, ABIBAL 0.031, PINSYL 0.030, PICABI 0.029, ACERUB 0.028, BETPAP 0.027, FAGGRA 0.025, plus the “other values” block | VERIFIED as FOFEM 5 code | Mapped from `FofemSpecies[].barkEq` and `Fofem_Sbt[]`. Header: “FOFEM 5.0 equations.” © 2002–2011 Bevins, all rights reserved. | https://www.frames.gov/documents/behaveplus/software/xfblib.cpp |
| F5-der | BT at 30 and 60 cm = *v* × DBH | VERIFIED [DERIVED] | Arithmetic exact. | — |
| F5-pd | Implication that only the ARR code holds these *v* numbers | **FIX** (source note) | Public-domain **FOFEM 6.7 User Guide** (and FFE GTR-116 addendum) has the same index scheme; **index 36 = 0.063**, not 0.065. Use that document for a government table, and keep 0.065 labelled as FOFEM 5 / Bevins. | https://research.fs.usda.gov/sites/default/files/2023-03/rmrs-fofem_6-7_user_guide.pdf ; https://www.fs.usda.gov/rm/pubs_series/rmrs/gtr/rmrs_gtr116_addendum.pdf |
| F4 | FOFEM 4 Table 1 equations (DF 0.065·DBH; PP −0.0376+0.0584·DBH; …) and “.I52” read as 0.152, uncertain | VERIFIED [OCR] | Same PDF via int_gtr344.pdf; treesearch/25257.pdf now serves the real file. Public domain. | https://www.fs.usda.gov/rm/pubs_int/int_gtr344.pdf |
| FVS-b | Table 4.2.1 BRATIO by group, sources, and [DERIVED] thickness at 30/60 cm DOB | VERIFIED | Groups from Table 3.2.1 (Scots pine is group 11). Public domain. | https://www.fs.usda.gov/sites/default/files/forest-management/fvs-ne-overview.pdf |

### §5c Measured bark

| Id | Claim | Status | Note | URL |
|---|---|---|---|---|
| KPS | Pine 2.05 cm (0.60–4.90; SD 1.14; CV 55.53%), n=40, mean DBH 20.59 (10.11–31.30), mean age 55; larch 2.01 (0.92–4.50; 0.82; 40.71%), n=24, DBH 26.35; oak 1.87 (0.90–3.40; 0.63; 33.59%), n=33, DBH 31.48, age 92; eqs. 0.0452d+0.4487, 0.0578d+0.4847, 0.1362d−0.7506 | VERIFIED | Table 2 and eqs. 3.1–3.3. | https://bibliotekanauki.pl/articles/9695.pdf |
| VAS | *F. sylvatica* 2.30–6.45%; *P. abies* 3.51–20.00%; *Q. robur* 5.71–26.92%; n = 2,389 / 5,403 / 4,394 | **UNVERIFIABLE** | MDPI HTML 403; a guessed PMC id did not contain the article body. Not independently opened this pass. | https://www.mdpi.com/1999-4907/15/12/2264 |
| MGP | *E. globulus* bark as in E2 Table 3 / §3.3 | **FIX** on the coppice “8.6 ± 2.6” pairing; other figures VERIFIED | See summary item 3. CC BY-SA 4.0. | https://silvafennica.fi/article/1141 |
| FEIS-bt | Mature DF bark 10–30 cm (4–12 in) | VERIFIED | | FEIS URL above |

### §6 Furrows and roughness

| Id | Claim | Status | Note | URL |
|---|---|---|---|---|
| MIT-n | Tree counts, DBH medians, quadrat counts and pattern scores for nine species (and the two leftover species with n=2) | VERIFIED [DERIVED counts] | Recomputed from the CEH zip. OGL v3. *Q. robur* 34 trees, DBH median 124 cm, 122/136 fissured, etc., all match. | https://doi.org/10.5285/f539567f-a8cd-482e-89b8-64a951b52d93 |
| MIT-f | Furrow depth / width / ridge median (IQR; range) | VERIFIED with a method note | Stated method (mean of up to 8 readings per quadrat, then median/IQR of those means). *Q. robur* n = 126 and median 18.8 mm reproduce when **all quadrats with measurements** are included (not only the 122 scored fissured). A few IQR bounds differ by ~0.1–0.4 mm after 1-d.p. rounding; treated as rounding, not a FIX. *F. sylvatica* one fissured quadrat at 17.5 mm matches. | same zip, `data/Trees.csv` + `data/Bark_plot.csv` |
| SHE | Young unthinned / thinned / old furrow depth, area, roughness and DBH, with 95% CIs | VERIFIED | Oregon State author PDF (OUP 403 to curl). | https://www.cof.orst.edu/cof/fs/kpuettmann/WJAF%2028%202013.pdf |
| IK | C_sd values at breast height on the thickest tree (larch 2.56 at 58 cm … beech 1.07 at 59 cm; sycamore breaks up at ~37 cm) | **UNVERIFIABLE** | Crossref confirms the article and a **CC BY 4.0** licence (so E2’s licence line is right). Springer PDF returned a stub. Values not independently read in the paper body or an erratum. | https://doi.org/10.1007/s00468-014-0995-0 ; Crossref work record |

### §7 Root flare and buttresses

| Id | Claim | Status | Note | URL |
|---|---|---|---|---|
| WHA | Spruce–fir a=0.96596, b=0.79667, n=337, r²=0.863, mean stump 11.8 in; all softwoods / hardwoods Table 3; 14-in stump → 12.1 in (10.1–14.1); range 2.1–48.5 in; [DERIVED] stump/DBH ≈ 1.09 (1.15 spruce–fir) at 12 in DBH | VERIFIED [OCR] where flagged | Other Table 2 species rows correctly left unused. Public domain. | https://research.fs.usda.gov/download/treesearch/21612.pdf |
| MKC | 45% of 11 species; *A. chaplasha* 87%; *A. scholaris* 0%; mean height 0.71–2.13 m; mean length 0.37–1.37 m; height/length grow with DBH and tree height, number does not; emergents larger (p&lt;0.01); slope no effect | VERIFIED at the [ABSTRACT] level E2 already claims | Full text not opened (Unpaywall closed). A Table 1 caption saying “12 species” was not independently confirmed and is not used. | https://link.springer.com/article/10.1007/s11676-012-0285-x |
| WAN | 1–7 buttresses; 69.57% have 3–5; 500 trees, length &gt;20 cm, DBH 28.5 ± 13.3 cm; 8.5 vs 44.04 t/ha; 16.18%; dimension definitions | VERIFIED | CC BY 4.0. PMC full text. | https://www.ncbi.nlm.nih.gov/pmc/articles/PMC12078330/ |
| CUS | 190 stems, BCI; more than half of biomass measured above 1.3 m; −0.21 % yr−1 uncorrected → +0.18 % yr−1 | VERIFIED, with a licence/access **FIX** | Public-domain ForestGEO PDF, not merely the Wiley abstract. | https://si-forestgeo01.si.edu/Public/pdfs/CushmanConditEtAl_2014_MEE.pdf |

## Source table

| Source | Opened? | Licence as checked | Notes |
|---|---|---|---|
| FNA taxon pages | Yes (WebFetch; curl often 500) | © Flora of North America Association. No open licence found. Facts only. | As E2 says. |
| VicFlora text | Yes (taxon pages HTTP 200 on retry; copyright page opened) | **Text CC BY 4.0; images mostly CC BY-NC-SA 4.0** | E2 understated this. |
| MLA99 Penn State PDF | Yes | © 1999 Smithsonian Institution, all rights reserved | |
| FEIS *P. menziesii* | Yes | US government work | |
| xfblib.cpp (FOFEM 5 arrays) | Yes | © 2002–2011 Collin D. Bevins, **all rights reserved** | Values verified as FOFEM 5. |
| FOFEM 6.7 User Guide | Yes | US government work, public domain | Index 36 = 0.063. |
| FFE / RMRS-GTR-116 addendum | Yes | US government work, public domain | Douglas-fir Vsp = 0.063. |
| FOFEM 4 INT-GTR-344 | Yes | US government work, public domain | Table 1. |
| FVS NE overview | Yes | US government work, public domain | Tables 3.2.1, 4.2.1, 3.5.1. |
| KPS09 bibliotekanauki PDF | Yes | As on the PDF; facts cited | |
| VAS24 MDPI | No (403) | Claimed OA (MDPI); not checked | UNVERIFIABLE this pass. |
| MGP15 Silva Fennica | Yes (HTML) | **CC BY-SA 4.0** (Crossref) | |
| MIT20 CEH / EIDC zip | Yes | **Open Government Licence v3** | Recomputed. |
| SHE13 Oregon State PDF | Yes | © SAF / OUP; author copy used | |
| IK14 *Trees* | Crossref only | **CC BY 4.0** (Crossref) | Body not opened; values UNVERIFIABLE. |
| WHA84 NE-RN-322 | Yes | US government work, public domain | |
| MKC12 Springer/HEP | Abstract only | Closed; © Northeast Forestry University and Springer 2012 (as reported) | [ABSTRACT]. |
| WAN25 PMC12078330 | Yes | **CC BY 4.0** | |
| Cushman 2014 ForestGEO PDF | Yes | **U.S. Government work, public domain in the USA** | E2 had this as [ABSTRACT] / Wiley only. |

## Not found

- *Fagus sylvatica* leaf dimensions: still not found in an open primary flora, as E2 §8 says.
- IK14 body and the 7 Oct 2014 erratum: not opened.
- VAS24 full text: not opened this pass (MDPI 403).
- MKC12 full text / Table 1 body: not opened. Abstract only.
- He et al. 2013, Warren et al. 1988, De Ridder et al. 2010 body, *Ceiba* buttresses, urban TFD/DBH: not opened, and E2 correctly left them out of the tables.
- A FOFEM **5.0** user-guide table of *v* coefficients: not found. The official public-domain table is in the FOFEM **6.x** user guide and differs at index 36.
- Silvics Manual: TLS error, as E2 reports.
- Flora iberica *Fagus* PDF: not retried beyond E2’s 500/502 report.

## Sources disagree

- **FOFEM 5 (Bevins code) vs FOFEM 6.7 / FFE addendum at index 36:** 0.065 vs 0.063 for the Douglas-fir / ponderosa / western-larch coefficient. E2’s FOFEM 5 column is right for FOFEM 5; do not silently substitute the 6.7 value, and do not cite 6.7 as if it were FOFEM 5.
- **FOFEM 4 vs FOFEM 5** functional form for ponderosa, western larch, lodgepole and Engelmann spruce: E2 §9 is right (and the worked 13–16% / 7–14% / 0.31 vs 0.56 in examples are consistent with the two equation sets).
- **KPS09 measured Scots pine** (double bark 2.05 cm at mean DBH 20.59 cm) vs FOFEM 5 *v* = 0.030 and FVS NE BRATIO 0.964: the disagreement E2 reports is real.
- **VAS24 vs FVS NE** constant bark ratio for Norway spruce: cannot be re-checked until VAS24 is opened; the logical point (a constant ratio vs a diameter- and height-varying percentage) stands.
- **FNA vs MIT20 for *Q. robur* bark:** descriptors differ (scaly vs furrowed); E2’s note is fair.
- **FNA vs FEIS Douglas-fir needle length:** consistent (range vs typical 2.5 cm), as E2 says.
- MKC12 abstract “11 species” vs a reported Table 1 caption “12 species”: not independently confirmed; not used as a FIX.

## Licence findings (short)

- VicFlora text is **CC BY 4.0**, images mostly **CC BY-NC-SA 4.0** — correct this in E2 §2.
- Cushman 2014 ForestGEO PDF is **U.S. Government work, public domain in the USA**.
- FOFEM 5 *v* list in xfblib.cpp is **all rights reserved** (Bevins). A later **public-domain** USDA table exists (FOFEM 6.7 User Guide) and disagrees at index 36.
- FOFEM 4, FVS NE, WHA84, FEIS: US government public domain, as stated.
- IK14 is genuinely **CC BY 4.0** (Crossref), even though the body was not opened.
- MGP15 **CC BY-SA 4.0**; WAN25 **CC BY 4.0**; MIT20 **OGL v3**; MLA99 and FNA are all-rights-reserved / no open licence; facts only.
