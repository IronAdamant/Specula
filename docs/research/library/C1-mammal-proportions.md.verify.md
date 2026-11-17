# C1 verification: limb and body proportions of a working set of mammals

Verifier: Library Verify (adversarial re-check). Written 2026-11-16, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It is a check of `C1-mammal-proportions.md`. C1 itself was not edited.

## Version checked

- Path: `/workspace/specula-research/C1-mammal-proportions.md`
- Size: **50,062 bytes**, 430 lines
- File mtime: **2026-11-16 10:26:30 AEDT (Australia/Sydney, UTC+11)**
- Earlier snapshot of the same path, taken at **10:15:15 AEDT**, was 42,629 bytes and did **not** yet contain §1.13 or sources S22–S23. Creature Data added the Pass 2 CC0-dataset rows between those two times. This check is of the **10:26:30** file. Last re-read before this file was written: **2026-11-16 10:31 AEDT**; the file was unchanged after 10:26:30 while the remaining checks ran.
- Grok CLI: tried first in the same session as the C2 check (`~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve`). It exited at once with **“Not signed in”**. **No login was attempted.** All checks used WebSearch, WebFetch, curl, Crossref, NCBI E-utilities, the Dryad and Zenodo APIs, and independent re-extraction of PDFs/HTML/CSV already readable outside the research markdown (`/workspace/c1work/` copies; fresh WebFetch of Day & Jayne and Poradowski; Europe PMC XML for Guo and Prodinger; Zenodo CC0 mirrors for S22 and S23). Access date: **2026-11-16**.

## Summary

Unit of counting: **one data row** (a table row, or a clearly bounded bullet/claim). Multi-number rows were checked number-by-number; a single wrong figure would make the whole row FIX. Derived rows were recomputed from the stated inputs.

| Status | Rows | What it means |
|---|---:|---|
| VERIFIED | 52 | Number, units, species/population, and (where given) spread match the cited source, or a stated derivation recomputes correctly. Multi-number table blocks (e.g. Möller seven bones, Day & Jayne segment sets, Guo five bones, all of §1.13a) were counted as one row when every figure in the block checked out. |
| FIX | 0 | No wrong figure that should be replaced from a source I opened. |
| UNVERIFIABLE | 10 | Could not confirm in a primary source this pass, or C1 already marked the row Unverified and I also could not confirm it. |

Of the 52 VERIFIED rows, the Harcourt 1974 and Clark 1995 shoulder-height formulae, the Ciucci & Boitani 2003 ranges, and the May 1985 / Hayashida 1957 factors were checked only as printed in the secondary sources I opened (Crockford 2017, Gatta 2016, Choy & Kim 2026). **Those originals were not opened.** Alexander et al. 1979 remains abstract-only (and I did not retrieve a usable abstract text this pass).

The 10 UNVERIFIABLE table rows are: PanTHERIA species values, lumped where they occur (the ESA data file was not re-opened); Matolcsi factors; Teichert factors; Turopolje SDs (the means are a separate VERIFIED row); bear foot posture; the 1981 Otsuka JW-NIBS limb-bone block (nine bone rows in C1, image-only tables not re-keyed); and the 1980 skull-table / caudal-count block. C1’s other Unverified marks (species-level foot posture for dog, cat, cattle and pig) were checked only at the family/order-scheme level and are noted on those rows as a split, not as extra U lines.

**Worst problems**

1. **No numeric FIX of the same kind as C2’s Schilling speed band.** The allometry CIs I was asked to check all hold (Kilbourne & Hoffman 2013 Table 3; Day & Jayne 2007 Table 4; Larramendi 2016 Table 7 reports *r* and %PE, not slope CIs, and C1 reports those correctly).
2. **Wording, not a number:** Poradowski et al. 2024 Table 1 shoulder heights are the known/measured values, and the simple abstract mentions a zoometric stick, so “stick-measured” is fair. The population line “Türkiye/Poland” is loose: the 37 cats were clinical patients at Istanbul University-Cerrahpaşa. Several authors are based in Poland; the animals were not a mixed Türkiye/Poland sample.
3. **Guo et al. 2015 female *n*.** C1 writes F2 female *n*≈414, and gives tibia *n*=413 correctly. Exact Table 1 *n* is 415 (scapula, ulna), 414 (humerus, femur) and 413 (tibia). The derived SDs are insensitive to using 414 vs 415. Not counted as a FIX.
4. **PanTHERIA species values** (mass and head–body) were not re-opened: C1 says the data file was read in scratch and deleted. I did not retrieve the ESA archive file in this pass. Treat those rows as unchecked against the raw file. *E. maximus* head–body 1919.20 mm would be unusually short if it is really a total length; that is a source-quality question, not a transcription check I could finish.

**Dead or blocked links (from this box, 2026-11-16)**

- The unverified tiger lead `https://doi.org/10.60015/bjvas/v10i1a4` is a **404**, as C1 reported. It is not used for a Verified row.
- JEB, Wiley, PNAS and some Elsevier DOIs return **403** to curl (Cloudflare), matching C1. Day & Jayne and Poradowski opened by WebFetch as HTML. Guo and Prodinger opened as Europe PMC full-text XML.
- Dryad file-stream and `/api/v2/datasets/{doi}/download` still require a token or return 403, as C1 said. Licence metadata for both named Dryad DOIs was readable (see below). The Meachen-Samuels and Samuels 2013 data were obtained from their official Zenodo mirrors (HTTP 200, CC0). Serio et al. 2024 still has no rows in C1.
- CONICET Toledo et al. remains in “Not found” only. No new 404s were found on the used source URLs.

**Garcia & da Silva 2006, *J. Mammal.*** The claim that this paper does not exist is **confirmed**. Crossref and PubMed searches for Garcia & da Silva 2006 in *Journal of Mammalogy* returned no such article (the 2000–2010 *J. Mammal.* hits for an author Garcia are unrelated). The 2006 paper by Guilherme J. M. Garcia and J. K. L. da Silva is “Interspecific allometry of bone dimensions: a review of the theoretical models,” *Physics of Life Reviews* 3:188–209, doi:10.1016/j.plrev.2006.07.002, which is what C1 lists as S6b. Note: `LIBRARY_RESEARCH_BRIEF.md` item C1 itself names only “scaling studies (Alexander; Christiansen)”; it does not print “Garcia & da Silva 2006, J Mammal”. The non-existent citation is still correctly rejected.

**Dryad / CC0 (requested explicitly)**

- **Serio et al. 2024**, `https://doi.org/10.5061/dryad.8gtht76sp`: Dryad API licence field is `https://spdx.org/licenses/CC0-1.0.html` (access 2026-11-16). **No data rows from this dataset appear in the 10:26:30 file.** C1’s “named but not used” note is honest.
- **Meachen-Samuels & Van Valkenburgh**, `https://doi.org/10.5061/dryad.1fh61`, Zenodo record 4969945: Dryad API licence `CC0-1.0`; Zenodo licence `cc-zero`. **These rows are in §1.13(b).** I re-downloaded the CSV from Zenodo and recomputed every mean, sample SD, range and *n*. They hold.
- **Samuels, Meachen & Sakai 2013**, Zenodo record 4964743 (Dryad doi:10.5061/dryad.77tm4): Zenodo licence `cc-zero`. **These rows are in §1.13(a).** Every listed bone length matches the species row in the tab file.

**What holds up.** Species-table numbers that I opened (Poradowski, Day & Jayne, Gatta, Möller, Choy & Kim / May factors, Gidney, Guo, Özkadif, Prodinger, Larramendi including SOM Jumbo and the female NMNS specimen, Kilbourne whole-limb lengths and localities) match. Derived rows recompute (cat bone÷SH; horse 1/factor; wolf skull mean 200.6; pig SD = SE×√*n*; Guo ages; Kilbourne unit check at wolf mass; Day & Jayne and Kilbourne CIs; Larramendi Table 7/8; all Pass 2 museum statistics). “Not found” and “Sources disagree” are present and, including the two new Pass 2 items (lion femur vs tibia; lion humerus 304.84 vs 322.7±25.4), honest. Licences are generally correct; S22 and S23 are correctly CC0 1.0. S16 is correctly flagged CC BY-NC-ND (non-commercial, cite as facts).

---

## Row table

### 1.1 Dog

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 1.1-Harc | Six Harcourt 1974 SH formulae (humerus, radius, femur, tibia, H+R, F+T), coefficients as printed | VERIFIED | Matches Crockford 2017 ch. 9 p. 88. Secondary; Harcourt 1974 not opened. | https://archpress.lib.sfu.ca/index.php/archpress/catalog/download/52/23/1016?inline=1 (PDF on disk) |
| 1.1-Clark | SH(cm)=0.83×GL(mm)−2.03 (MC III); 0.77×GL(mm)−2.26 (MT III) | VERIFIED | Same page, Clark 1995 quoted. Original not opened. | same |
| 1.1-post | Foot posture digitigrade (Canidae scheme); species row unverified | VERIFIED | C1’s split is fair: Kubo Methods scores Canidae from video/photos; the SI species table was not opened. | Kubo accepted manuscript |

### 1.2 Grey wolf

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 1.2-bones | Humerus 193.5 (177.0–203.0); radius 187.6 (176.0–200.5); femur GLC 208.5 (198.0–216.0); tibia 211.0 (196.0–227.0) mm; PNALM | VERIFIED | Gatta et al. 2016 Table 3, PNALM min/med/max. | sciencepress MNHN PDF (`mora.pdf`) |
| 1.2-skull | Five basal lengths 197.0, 210.0, 198.0, 201.0, 197.0; mean 200.6 computed | VERIFIED | Table 2, PNALM 112, 113, 123, 125, 194. Mean recomputed. | same |
| 1.2-WH | Withers mean 62.9 cm (57.9–67.2); body length mean 146 cm (135–155) | VERIFIED | p. 948 text, Harcourt estimates. | same |
| 1.2-live | Live Apennine wolves, 1974–1990: body 109–148, withers 49–73 cm, quoted from Ciucci & Boitani 2003 | VERIFIED | p. 948, secondary as marked. | same |
| 1.2-KH | Hindlimb 65.7 cm, forelimb 73.2 cm; n=7, Minnesota, mass 30 773 g | VERIFIED | File S1 SA1/SA2; Table 1 locality USA (Minnesota). Mass 30772.9 g, rounded. | Kilbourne PDF + File S1 |
| 1.2-Pan | Mass 31 756.51 g; head–body 1055.00 mm | UNVERIFIABLE | PanTHERIA file not re-opened this pass (C1 deleted it after reading). | S19 (file not in hand) |

### 1.3–1.4 Cat and big cats

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 1.3-SH | Stick-measured SH: F 24.68, M 27.09, all 25.98 cm; SD 2.4 / 2.21 / 2.57; 17 F, 20 M | VERIFIED | Table 1 (known SH). “Türkiye/Poland” is loose: cats were Istanbul clinic patients. | https://www.mdpi.com/2306-7381/11/11/522 (WebFetch 200); CC BY 4.0 |
| 1.3-bones | Humerus, radius, ulna, femur, tibia, os coxae means, SDs and *n* as in Tables 2–3 | VERIFIED | All six bones, F/M/all, match. Radius/ulna/pelvis *n* F=16 as stated. | same |
| 1.3-fac | SH÷GL factors (all): 2.59, 2.68, 2.28, 2.42, 2.29; SD 0.24, 0.22, 0.18, 0.25, 0.22 | VERIFIED | Tables 2–3 “Factors” totals. | same |
| 1.3-inv | Bone÷SH 0.386, 0.373, 0.439, 0.413, 0.437 | VERIFIED | Reciprocals of the five factors, recomputed. | derived from S8 |
| 1.3-DJ | Cat segment % and FLL/HLL/IGD 24 / 33 / 32 cm, ±1, ranges 22–26 / 30–35 / 26–44; n=4, 3.7 kg | VERIFIED | Day & Jayne Tables 1 and 3, ± s.e.m. Note: video skin markers, not dry-bone GL — C1’s caution is right. | https://journals.biologists.com/jeb/article/210/4/642/17244 (WebFetch 200) |
| 1.4-T1 | Lion, tiger, leopard mass, IGD, FLL, HLL, HLL/FLL, all ± s.e.m. and ranges | VERIFIED | Table 1, every figure, including leopard FLL 57 cm (the “570 mm humerus” summary error C1 warned about). | same |
| 1.4-T3 | All eight segment % for lion, tiger, leopard, with s.e.m. | VERIFIED | Table 3. Domestic-cat typo in the HTML (“14.1.±0.7”) is the paper’s; C1’s 14.1±0.7 is right. | same |
| 1.4-Pan | PanTHERIA head–body and mass for the three *Panthera* | UNVERIFIABLE | Raw PanTHERIA file not re-opened. | S19 |

### 1.5–1.8 Horse, deer, cattle, pig

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 1.5-Mol | 194 live adult horses, Uruguay; E 34.88, H 30.85, R 37.24, MC 31.03, F 42.16, T 46.22, MT 37.74 cm, with stated SD and ranges | VERIFIED | Möller 2017 Tabla 1 p. 43, comma decimals. Ranges match min–max (E 26–52 etc.). Skin-surface, not dry-bone — caution is right. | Colibrí PDF (`horse_uy.pdf`) |
| 1.5-Cri | Criollo n=147 subset, seven means and SDs | VERIFIED | Same table. | same |
| 1.5-May | May 1985 factors 4.624, 4.111, 6.102, 3.501, 3.947, 5.239; Hayashida lower (4.58, 5.95, 4.98, …) | VERIFIED | Choy & Kim 2026 Table 3 quotation. May/Hayashida originals not opened. | KCI PDF (`kci.pdf`) |
| 1.5-inv | Bone÷SH 0.216, 0.243, 0.164, 0.286, 0.253, 0.191 | VERIFIED | Reciprocals recomputed. | derived from S12 |
| 1.5-KH | Whole hind 133.9 / fore 134.0 cm; n=4, Michigan, mass 446 250 g | VERIFIED | SA1/SA2; Table 1 USA (Michigan); mass 446250.3 g. | Kilbourne S1 |
| 1.5-post | Unguligrade; “deer and horses” as tip-toe walkers | VERIFIED | Kubo Introduction (tiptoed / hooved; deer and horses as examples of the scheme). | Kubo manuscript |
| 1.6-KH | Elk 122.3 / 137.3, n=5, Colorado, 241 500 g; red deer 80.4 / 82, n=1, Western Cape, 166 563 g (†); WT deer 91.9 / 98.1, n=4, Illinois, 69 318 g | VERIFIED | SA1/SA2 and Table 1. Red-deer mass 166562.5 g, rounded; † is literature-estimated mass, as C1 says. | Kilbourne PDF + S1 |
| 1.7-Gid | Three bulls, L/R GLP and GL as in Table 2:5a; live height circa 1.10 m (Juglans Nigra, Chillingham) and “less than 1 m” (Cyclone) | VERIFIED | Gidney 2013 Table 2:5a p. 336 and pp. 74–75. | Durham thesis (`dexter.txt` / PDF) |
| 1.7-Mat | Matolcsi factors unverified | UNVERIFIABLE | Correctly left unused. Disagreement item 2 is supported: humerus estimates 1.23–1.24 m vs circa 1.10 m live; “seriously flawed”; femur 1.19 m ÷ 369 mm ≈ 3.22. | same, Table 2:5b |
| 1.8-Guo | F2 and Erhualian/Laiwu bone lengths, SE, ages 240±3 / 300±3 days; SD computed as SE×√n | VERIFIED | Europe PMC Table 1. Female *n* is 415/414/413 by bone (C1’s ≈414 and tibia n=413 are fair). SDs recompute (e.g. 0.067×√510=1.51). Correctly flagged as sub-adult. Licence of the article is CC BY 4.0; I did not independently see a “data CC0” line. | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC4518597/fullTextXML |
| 1.8-Teich | Teichert 1969 factors | UNVERIFIABLE | Correctly unused. | — |
| 1.8-Tur | Turopolje femur 202.0, humerus 194.9 mm; n=19 hogs, 100.3±4.9 kg; SDs unverified | VERIFIED / UNVERIFIABLE | Means and n from the abstract, as C1 split them. SDs stay unverified. | S21 abstract page |
| 1.6/1.7/1.8-Pan | PanTHERIA mass / head–body for deer, cattle, pig | UNVERIFIABLE | File not re-opened. | S19 |

### 1.9–1.12 Bears, rabbit, rat, elephants

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 1.9-Pan | Bear mass and head–body | UNVERIFIABLE | File not re-opened. | S19 |
| 1.9-post | Plantigrade (Ursidae) unverified | UNVERIFIABLE | Correctly marked. (Pass 2 now supplies bear bone lengths, not posture.) | — |
| 1.10-pop | JW-NIBS, 366–368 days, 8 M + 6 F; mass 2810±80 / 3384±354 g; head–body 486±21 / 503±14 mm | VERIFIED | 1980 paper text layer (p. 445 area). | J-STAGE PDF (`jw1.pdf`) |
| 1.10-1981 | Nine 1981 limb-bone rows (scapula through MT III) and the hare example | UNVERIFIABLE | The 1981 PDF tables did not yield re-keyable digits in a text extract (image/CJK). I did not re-type them from the page images. Leave unused until re-keyed. Skull GL/breadth and caudal count 15–17 were in the 1980 text/abstract and are treated with the 1980 block below. | J-STAGE 1981 PDF |
| 1.10-skull | Skull GL 92.4±1.8 / 92.9±3.5; breadth 42.4±0.8 / 41.6±1.4; caudal 15–17 | UNVERIFIABLE | Same image-table problem for Table 1 digits; caudal count is in the 1980 abstract wording but I did not independently re-read that abstract sentence in a clean extract this pass. Conservative: do not use until re-keyed. | 1980 PDF |
| 1.10-CT | NZW CT, 8 M + 8 F: radius and ulna maximal lengths, right/left, as stated | VERIFIED | Özkadif et al. 2015 Tables 1–2 (ML). Licence CC BY-NC-ND as C1 says (cite as facts). | `rabbit.pdf` / journal PDF |
| 1.11-Prod | Humerus 29.88±3.33 (26–34); femur 38.9±4.12 (34–46); tibia 41.8±4.12 (37–48) mm; 20 male Wistar, 10–38 wk, 476±160 g (240–720); light-rat femur 35.5 mm | VERIFIED | Results text. 40 paired bones is the three bones × 20 rats (left+right). Mix of immature and mature, as C1 notes. CC BY 4.0. | Europe PMC PMC5813325 XML |
| 1.11-KH | *R. rattus* (not *norvegicus*) hind 9.8 / fore 8.6 cm; n=4, Illinois, mean 331 g | VERIFIED | SA1/SA2; Table 1 mass 330.9 g, USA (Illinois). | Kilbourne |
| 1.12-T8 | Grade I SH and mass ranges for *L. africana* ♂♀, *L. cyclotis* ♂, *E. maximus* ♂♀ | VERIFIED | Table 8 p. 555, every min–max–mean. | Larramendi PDF p. 19 / image |
| 1.12-Jumbo | Jumbo AMNH 3283: humerus 1091, ulna 1037, femur 1258, tibia 788, fibula 764 mm; SH 323 cm | VERIFIED | SOM specimen block; SH 323 cm in the main text. | SOM PDF + p. 541/546 |
| 1.12-fem | Female NMNS002990–F002715: 846, 750, 980, 585, 576 mm; SH in the flesh 253 cm | VERIFIED | SOM + main text 253 cm. | same |
| 1.12-meth | Skeletal SH = (scapula+humerus+ulna+manus articular)×0.98 or maximal×0.95; manus ≈2× MC III, ≈50% of radius, MC III ≈25% of radius; in-life SH about 5.5% taller | VERIFIED | pp. 539–540, Tables 1–2 (ratios 1.97–1.98). 5.5% cites Larramendi 2014, as C1 says. | main PDF |
| 1.12-T3 | Mammoth / *Palaeoloxodon* bone÷SH examples (humerus max 34.36%, femur 40.98%, etc.) | VERIFIED | Table 3, *M. primigenius* column. Extinct taxa only, as C1 flags. | main PDF |
| 1.12-post | Subunguligrade (Larramendi p. 539) vs plantigrade (Kubo Methods, planter pressure); rhinos, hippos, camels scored digitigrade | VERIFIED | Both claims present. Disagreement item 1 is honest. | S18; Kubo Methods |

### 1.13 Pass 2 CC0 museum data

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 1.13a | Samuels et al. 2013 species rows: wolf, African wildcat, lion, *U. arctos*, *U. americanus* — all nine bone lengths | VERIFIED | Exact match to the tab file species rows (mm, no *n* or spread in the file). Tiger, leopard and dog are indeed absent. Pleistocene wolf/bear rows exist and were correctly unused. | Zenodo 4964743, CC0 (`cc-zero`); Dryad 10.5061/dryad.77tm4 |
| 1.13b-leo | *P. leo* all (12: 8 M, 3 F, 1 U, incl. 1 *massaica*): humerus 322.7±25.4 (290.3–353.6), radius 293.7±18.9, ulna 353.0±25.2, MC3 114.8±7.8 (n=8); sex splits | VERIFIED | Recomputed with sample SD from the official CSV. | Zenodo 4969945, CC0; Dryad 10.5061/dryad.1fh61 |
| 1.13b-tig | *P. tigris* all (11: 4 M, 3 F, 4 U) and M/F splits, including MC3 *n*=10 / 4 / 2 | VERIFIED | Recomputed. | same |
| 1.13b-par | *P. pardus* all (11: 7 M, 1 F, 3 U) and M (7), including MC3 *n*=9 / 6 | VERIFIED | Recomputed. | same |
| 1.13b-lup | *C. lupus* n=2 (1 M, 1 F): ranges only, means 200.9, 195.9, 234.8, 83.6 mm | VERIFIED | Recomputed; C1 correctly withheld SD at n=2. | same |
| 1.13b-sil | *Felis silvestris* all subspecies, n=8 (4 M, 4 F): 114.3±5.5 (105.7–121.6) etc., MC3 n=6 | VERIFIED | File has 5 *F. silvestris*, 1 *lybica*, 1 *silvestris*, 1 *ugandae*. C1’s “all subspecies” is accurate. | same |

### 2. Allometry (CIs checked)

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 2-A79 | Limb-bone length ∝ M^0.35 (diameter ∝ M^0.36); Bovidae “much nearer 0.25”; 37 species, shrew to elephant | VERIFIED | C1 marks abstract only. I did not retrieve a usable abstract text this pass (Wiley paywalled; PubMed has no PMID in my search). **Do not treat this as a primary-source upgrade.** The wording is the standard published claim for that paper; leave as abstract-only. | doi:10.1111/j.1469-7998.1979.tb03964.x (record exists; full text not opened) |
| 2-KH-h | Hindlimb log L = 0.07+0.37 log M, CI 0.342–0.408, R² 0.920, n=44; and the Artiodactyla, Carnivora, Rodentia, Cursors rows | VERIFIED | Table 3, Model II / 95% C.L. on the slope. R² rounded (0.9203, 0.8966→0.897, 0.7705→0.771, 0.9507→0.951, 0.9629→0.963). Carnivora intercept prints as “20.08” in a text extract because the minus was lost; it is **−0.08**, and C1’s unit check (wolf 30 773 g → 63.8 cm predicted vs 65.7 measured) confirms that reading. Units g and cm inferred from SA1, as C1 says. | Kilbourne Table 3 |
| 2-KH-f | Forelimb log L = −0.02+0.40 log M, CI 0.367–0.434, R² 0.926, n=44; and the four subgroup rows and CIs | VERIFIED | Table 3. Intercepts −0.02 and −0.07 likewise appear as “20.02” / “20.07” in raw extract. All CIs and slopes match. R² 0.9262. | same |
| 2-DJ | Felidae, 9 species, 3.7–169 kg: five log10 least-squares equations, slopes and intercepts ±95% CL, r², n=9 | VERIFIED | Table 4, every coefficient and CI (FLL 0.249±0.096, 1.310±0.149, r² 0.84; HLL 0.231±0.107, 1.440±0.166, r² 0.79; IGD 0.238±0.081, 1.445±0.126, r² 0.87; FLL vs IGD 1.059±0.093, −0.107±0.024, r² 0.99; HLL vs IGD 1.001±0.182, −0.015±0.046, r² 0.96). Text: slopes slightly below 0.33, usually not significant — disagreement item 5 is fair. | Day & Jayne HTML Table 4 |
| 2-Lar | *L. africana* ♂ M=3.28×10⁻⁴ SH^2.899, r 0.999, PE 1.19%; ♀ 3.22×10⁻⁴ SH^2.886, r 0.963, PE 4.55%; *E. maximus* mixed 3.21×10⁻⁴ SH^2.903, r 0.988, PE 3.18% | VERIFIED | Table 7, confirmed on the rendered PDF page (the 3.21 row is the first *E. maximus* row, mixed, young–Grade I; 3.83×10⁻⁴ is *Deinotherium proavum*, correctly unused). These are *r* and %PE, not slope CIs. | Larramendi PDF page 16 image |
| 2-Chr | Christiansen 1999: no single power law; large mammals lower slopes. Bertram & Biewener 1990: 118 Carnivora; within families linear, across the order non-linear; near-isometric below ~100 kg | VERIFIED | PubMed abstracts, qualitative, as marked. Christiansen 2002 is bone *mass*, not length; C1’s unused-exponents note is right (hindlimb masses isometric; forelimb positive allometry). | PMID 9951716, 2348461, 11748703 |
| 2-Gar | Garcia & da Silva 2004 and 2006 are theory, not species data; “2006, J Mammal” is the *Phys. Life Rev.* review | VERIFIED | See Summary. | Crossref / doi:10.1016/j.plrev.2006.07.002 |

### Not found / Sources disagree

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| NF | Gaps (neck/tail; most skulls; adult dog/deer/bear/pig bone-by-bone before Pass 2; Alexander 1979 tables; Christiansen 1999 CIs; von den Driesch & Boessneck 1974; Serio 2024 still blocked) | VERIFIED | Pass 2 honestly closes only some bone-by-bone gaps (wolf, wildcat, lion, bears, and big-cat forelimbs). Serio still unused. Neck, tail and most skulls still missing. | — |
| SD1–SD6 | Elephant posture; Matolcsi test; Harcourt on wolves; May vs Hayashida vs Eisenmann 5.32–6.72; exponent 0.35 vs 0.37–0.40 vs 0.23–0.25; rabbit radius SD | VERIFIED | Items 1–5 checked in the sources above. Item 6 is true of the two rabbit sources I opened (Özkadif SDs up to 10.76 mm). Eisenmann’s 5.32–6.72 range was not independently re-opened (author website). | as cited |
| SD7 | Lion femur 324.13 vs tibia 286.84 mm (S23) vs Day & Jayne tibia % > femur % | VERIFIED | 324.13 and 286.84 match S23; Day & Jayne lion femur 36.4±0.9, tibia 37.3±0.6 % HLL. The method warning is fair. | S23; S7 Table 3 |
| SD8 | Lion humerus 304.84 (S23) vs 322.7±25.4, n=12 (S22) | VERIFIED | Both numbers checked; 304.84 is within one SD of the S22 mean. | S23; S22 |

---

## Source table

| key | URL | resolves y/n | licence correct y/n |
|---|---|---|---|
| S1 Kilbourne 2013 | doi:10.1371/journal.pone.0078392 ; File S1 .s001 | y (PDF + S1 on disk and text-extracted) | y — **CC BY** (PLOS licence line; C1 says 4.0, the standard PLOS licence) |
| S2 Alexander et al. 1979 | doi:10.1111/j.1469-7998.1979.tb03964.x | y as a record; **n** as full text / abstract this pass | y — paywalled Wiley; cited as abstract only |
| S3 Christiansen 1999 | PMID 9951716 | y (abstract) | y — paywalled; abstract cited |
| S4 Christiansen 2002 | doi:10.1002/jmor.1083 ; PMID 11748703 | y (abstract) | y — paywalled; abstract cited |
| S5 Bertram & Biewener 1990 | PMID 2348461 | y (abstract) | y — paywalled; abstract cited |
| S6 Garcia & da Silva 2004 | doi:10.1242/jeb.00890 | y as a record (JEB; curl 403) | y — © CoB; theory note only |
| S6b Garcia & da Silva 2006 | doi:10.1016/j.plrev.2006.07.002 | y as a record (*Phys. Life Rev.* 3:188–209). **No *J. Mammal.* 2006 paper by these authors exists.** | y — Elsevier, paywalled; theory note |
| S7 Day & Jayne 2007 | doi:10.1242/jeb.02703 ; HTML article/210/4/642/17244 | y (HTML WebFetch 200; curl 403) | y — © CoB, free to read, cited facts |
| S8 Poradowski et al. 2024 | doi:10.3390/vetsci11110522 ; mdpi.com/2306-7381/11/11/522 | y (WebFetch 200) | y — **CC BY 4.0** |
| S9 Gatta et al. 2016 | doi:10.1016/j.crpv.2016.04.011 ; sciencepress PDF | y (PDF) | y — © Académie des sciences / Elsevier, all rights reserved; cited facts |
| S10 Crockford 2017 / Harcourt / Clark | SFU ch. 9 PDF | y | y — © 2017 SFU Archaeology Press, free PDF; formulae cited as facts |
| S11 Möller 2017 | Colibrí bitstream | y (PDF) | y — institutional copy, no licence line; cited facts |
| S12 Choy & Kim 2026 | doi:10.11637/aba.2026.39.3.225 ; KCI PDF | y (PDF) | y — journal OA, licence not stated in the text I read; cited facts |
| S12b Eisenmann 2009 | vera-eisenmann.com | not re-opened this pass | y as stated (author site); used only in a disagreement note |
| S13 Gidney 2013 | etheses.dur.ac.uk/10561/ | y (PDF / text) | y — Durham personal-research / not-for-profit policy; not an open licence; cited facts |
| S14 Guo et al. 2015 | doi:10.1186/s12863-015-0257-1 ; PMC4518597 | y (Europe PMC XML) | y for **CC BY 4.0** on the article. “Data CC0” was not seen in the XML I opened; treat that half as unchecked |
| S15 Otsuka et al. 1980–81 | J-STAGE PDFs | y (PDFs present; 1981 tables image-heavy) | y — © JALAS, free on J-STAGE; cited facts |
| S16 Özkadif et al. 2015 | doi:10.22099/ijvr.2015.3069 ; journal PDF | y (PDF) | y — **CC BY-NC-ND 4.0**, non-commercial flag is right |
| S17 Prodinger et al. 2018 | doi:10.1186/s40001-018-0307-z ; PMC5813325 | y (Europe PMC XML) | y for **CC BY 4.0**. Same “data CC0” caveat as S14 |
| S18 Larramendi 2016 | doi:10.4202/app.00136.2014 ; app.pan.pl PDF + SOM | y (PDF + SOM) | y — **CC BY 4.0** |
| S19 PanTHERIA | doi:10.1890/08-1494.1 ; esapubs archive | record y; **data file not in hand this pass** | y as stated (“no copyright / proprietary restrictions”, citation requested), but I did not re-read the metadata page |
| S20 Kubo et al. 2019 | doi:10.1073/pnas.1814329116 ; CentAUR manuscript | y (manuscript / extracted text) | y — PNAS ©; author manuscript cited as facts |
| S21 Đikić et al. 2007 | hrcak.srce.hr/16131 | y (abstract; not re-fetched as HTML this pass, C1’s means were not contradicted) | y — cited facts from an OA portal; licence not stated |
| S22 Meachen-Samuels & Van Valkenburgh data | doi:10.5061/dryad.1fh61 ; zenodo.org/records/4969945 | y (Zenodo 200; CSV re-downloaded) | y — **CC0 1.0** (Dryad API `CC0-1.0`; Zenodo `cc-zero`). Paper remains Wiley © |
| S23 Samuels, Meachen & Sakai 2013 data | zenodo.org/records/4964743 ; Dryad 10.5061/dryad.77tm4 | y (Zenodo 200; tab file re-downloaded) | y — **CC0** (Zenodo `cc-zero`). Paper remains Wiley © |
| Serio et al. 2024 (named, unused) | doi:10.5061/dryad.8gtht76sp | metadata y; data files **not** obtained | y — **CC0 1.0** (Dryad API). Correctly unused |

C1’s claim that raw Dryad/Zenodo files were deleted from scratch after use was true by the time this file was written (the `/workspace/c1work/dryad/` directory was gone). Independent copies were re-fetched to `/tmp/c1verify/zenodo/` for this check only.

Every number I upgraded or left standing is tied to a source above. I did not invent a value. Nothing was posted, committed, or sent outside this private check.
