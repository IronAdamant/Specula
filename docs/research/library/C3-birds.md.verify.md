# C3 verification: birds (wing bones, shape, wingbeat, folding, feathers, gait)

Verifier: Library Verify (adversarial re-check). Written 2026-11-16, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It is a check of `C3-birds.md`. C3 itself was not edited.

## Version checked

- Path: `/workspace/specula-research/C3-birds.md`
- Size: **42,583 bytes**, **380 lines**
- File mtime: **2026-11-16 10:40:20 AEDT (Australia/Sydney, UTC+11)** — this matches the last-seen stamp given at the start of the check. Re-read at **2026-11-16 10:59 AEDT** before this file was written; the research file was unchanged while the check ran.
- **Grok CLI live-search pass:** run as required, after sign-in, with `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve --prompt-file /workspace/c3verify/grok_prompt.md --output-format plain` (web search left on). Started **10:47:48 AEDT**, exited 0 at **10:57:08 AEDT** (elapsed 559.8 s). Live search was used (publisher pages, PMC, Dryad, USFWS, Wikipedia). A Grok claim was treated as support only when it named a URL that I then opened myself. Independent checks used curl, WebFetch, Crossref, Europe PMC, BTO BirdFacts, the official PLOS corrected Table S1, Protocol S1, and PDFs/HTML read outside any repo (`/workspace/c3verify/`). Access date for live URLs: **2026-11-16**.
- **Grok conflicts** are listed at the end of the Summary. The only conflict that survived my own opening of the sources is the Weeks et al. 2025 paper licence.

## Summary

Unit of counting: **one data row** (a table row, or a clearly bounded bullet/claim). Multi-number rows were checked number-by-number; a single wrong figure would make the whole row FIX. Derived (computed) rows were recomputed from checked inputs. The source list is not in the row counts; it is in the source table. C3 itself claimed 127 Verified and 6 Unverified; I count **table rows plus the published Fig. 1 means, the ANOVA claim, the 1990-equation comparison, the two §3.1 wingbeat bullets, and the §2.4 claims**. That is a slightly broader unit than C3’s headline, so the totals need not match 127+6.

| Status | Rows | What it means |
|---|---:|---|
| VERIFIED | 131 | Number, units, species/population, and (where given) spread match a source I opened, or a stated derivation recomputes correctly. Rows C3 marked Verified (abstract) that I confirmed in an abstract I opened are included, and are labelled abstract-level in the row table. |
| FIX | 1 | Source-list licence only: Weeks et al. 2025 [B23] is **CC BY-NC-ND 4.0**, not CC BY 4.0. No numerical table cell was wrong. |
| UNVERIFIABLE | 13 | Full text or a primary source not obtained this pass, or C3 already marked the row Unverified and I agree: Nudds, Dyke & Rayner 2004 electronic appendix (BI clade list); four Wikipedia feather-count rows (wording matches Wikipedia; still secondary); five Clark 1975 gait rows (Digital Commons / SORA PDF returned 403); Wang & Clarke 2015 specimen count 498, AR–PC1 r² values, and PC1 variance percentages (105 species and the qualitative result are in the abstract I opened). |

**Worst problems**

1. **Licence of Weeks et al. 2025 [B23] is wrong in the source list.** C3 says “Paper CC BY 4.0 (Nature open access)”. The PDF I opened, Europe PMC (`cc by-nc-nd`), and Crossref all give **CC BY-NC-ND 4.0**. That forbids commercial use and derivatives of the article. The RMSE figures in Table 1 themselves check out, and the Dryad dataset licence **CC0** is correct (JSON-LD on https://datadryad.org/dataset/doi:10.5061/dryad.v41ns1s4c). This is the only FIX.
2. **Clark 1975 [B20] could not be opened here** (Digital Commons `viewcontent.cgi` 403; SORA redirects to a HTML landing page). C3 marks five §7 rows Verified from that paper. I have not confirmed 47 species, the pinyon jay / Australian magpie / currawong notes, “at least 19” higher taxa, or the age/gait examples. Those rows are UNVERIFIABLE this pass, not a demonstration that C3 is wrong.
3. **Wang & Clarke 2015 [B7] full text was blocked** (PMC and Royal Society 403 / reCAPTCHA to this box). The abstract I opened supports 105 species and only a weak relationship with flight-style categories, wing loading and aerodynamic variables, with most shape variation in primary-covert extent. I could not confirm **498 specimens**, **90 landmarks**, the **r²** values (0.32/0.34 Hackett; 0.50/0.53 Jarvis), or **PC1 30.8% dorsal / 40.06% ventral**. Those figures stay UNVERIFIABLE. Table S4 was not opened (C3 already said so).
4. **Wang et al. 2011 Table S1 masses remain unusable**, as C3 said. The 2013 correction exists and fixes the bone columns (see Wang 2011 findings below). I found **no later erratum for the mass column**. In the corrected file, *Alcedo atthis* is still **719 g** (and is coded PT). BTO BirdFacts, which I opened, gives adult common kingfishers **39.1 ± 3.5 g** (34.5–45.5 g, N=554). *Alca torda* is **67.75 g** in the same file; BTO adults are **612.6 ± 52.8 g** (525–705 g, N=266). C3 did not use the mass column, and already flagged the kingfisher example. Not a C3 numerical error.
5. **C3’s statement that computed fprim/ta means “match the published back-transformed means … exactly” is numerically true of the arithmetic means, but the published figures are not geometric (log-back-transformed) means of the raw ratios.** Arithmetic means from the corrected file are CF 0.913062, FS 0.788882, FG 1.547108, PT 1.178161, against published 0.9131, 0.7889, 1.5471, 1.1782. Geometric means are lower (CF 0.888, FS 0.775, FG 1.463, PT 1.164). The paper’s “back transformed” label does not describe the quantity as usually defined. C3 reports both sets of numbers correctly. Not counted as a FIX.

**What holds up.** The corrected Wang 2011 Table S1 bone lengths and all 25 example-species rows match exactly; ta = hu+ul+mn in all 183 living-species rows; the four flight-style ratio blocks recompute exactly with sample SDs; n = 90/20/8/65 matches. Alerstam Protocol S1 biometry for all 29 example species matches, as do AR and Q (g = 9.81). The §2.2 style-group means recompute once *Carduelis cannabina* is joined to Wang’s *Acanthis cannabina* (PT) and the mislabelled Egyptian vulture is dropped, as C3 stated. Pennycuick 1996 working equation and 1990 eq. 10 check out; f_ref recomputed from the 1996 formula matches Table 2 to the printed precision; the 1990 formula on the same 16 birds is 2.35–9.78% below 1996 (C3: 2–10%), with swan 3.13 vs 3.37 Hz, eider 5.99 vs 6.60, wigeon 6.56 vs 7.27. Crandell & Tobalske 2015, Stowers et al. 2017, Tobalske 2007, Alerstam Table 1, Weeks Table 1, and the USFWS glossary/position guide all check out in full text or official pages I opened. C3 has a full source list, a “Not found” section, and a “Sources disagree” section. Unverified Wikipedia and BI-appendix rows are not presented as fact elsewhere (they appear only as Unverified rows or in the disagreement list).

**Dead or blocked links (from this box, 2026-11-16)**

- `journals.biologists.com` PDF and many DOI URLs: **403** to curl (Cloudflare). The same articles often opened by WebFetch as HTML (B12, B15, and the 1990 abstract at `/jeb/article/150/1/171/5695/...`). The 1996 HTML at `/jeb/article/199/7/1613/7250/...` resolved to the **wrong article** (a damselfly paper). Tethys holds the 1996 abstract and the 2001 PDF.
- Royal Society and Europe PMC PDF/XML endpoints: **403/500** for B7, B8, B10.
- USFWS Feather Atlas: **403** to curl; **WebFetch 200**.
- Clark 1975 Digital Commons PDF (`article=7592` and `article=1914`): **403**. SORA legacy PDF URLs redirect to a HTML portal.
- Weeks/Dryad dataset files were not downloaded (C3 reported HTTP 401 / bearer token). The Dryad **landing page** returned **200** and declares **CC0**.
- No 404s were found on the listed source DOIs themselves; the papers exist.

---

## Wang 2011 findings

- **2013 correction exists.** Wang, McGowan & Dyke, *PLoS ONE* 8(10), 24 Oct 2013, https://doi.org/10.1371/annotation/6c8c755f-58fc-49e3-b499-8d082318eef6: “There were errors in Table S1. The correct version of the file is available here.” I downloaded the official file: https://journals.plos.org/plosone/article/file?type=supplementary&id=10.1371/annotation/6c8c755f-58fc-49e3-b499-8d082318eef6.s001.cn (OLE `.doc`, 249,856 bytes). Licence of the paper and correction: **CC BY 4.0**.
- **Original vs corrected file.** The 2011 Table S1 (pone.0028672.s001.doc) has the manus column independently sorted, so ta does not equal hu+ul+mn (e.g. *Columba palumbus* ta printed 262.27 vs 54.90+59.02+26.21=140.13). In the **corrected** file, ta = hu+ul+mn in **all 183** living-species rows (largest residual 0.015 mm, i.e. rounding). C3 is right that the 2013 file is the one to use, and that the original manus column was scrambled.
- **Bone lengths in §1.2.** All 25 example species match the corrected file (style, hu, ul, mn, fprim, ta, 2-decimal fractions, brachial index, fprim/ta).
- **Group ratios in §1.1.** Recomputed from the 183 rows with sample SD: CF n=90, FS 20, FG 8, PT 65, every mean, SD and range matches C3 to the printed 3 decimals.
- **Published Fig. 1a means and ANOVA.** Fig. 1 caption: back-transformed means with 95% CI CF/A 0.9131 (0.8685–0.9557), FS/B 0.7889 (0.7190–0.8588), FG/C 1.5471 (1.0835–2.0107), PT/D 1.1782 (1.1319–1.2244). ANOVA F(3,182)=37.789, P<0.001. CF vs FS “do not … pass the HMCT (P = 0.062)”. All match. Mapping A–D = CF–PT is consistent with the values (FG highest, FS lowest).
- **Species count.** Methods: “184 species from 55 families (18 orders)”. Fig. 2 caption: “183 species (S1)”. Corrected file: **183** living-species rows. C3 already records this in Sources disagree #7.
- **Masses.** No later mass erratum found (Europe PMC, PLOS correction page, Crossref). Corrected *Alcedo atthis* M = 719 g vs BTO adults 39.1±3.5 g (https://www.bto.org/learn/about-birds/birdfacts/kingfisher). Corrected *Alca torda* M = 67.75 g vs BTO adults 612.6±52.8 g (https://www.bto.org/learn/about-birds/birdfacts/razorbill). *Neophron percnopterus* is coded **PT** (mass 2120 g is a plausible vulture mass; the style label is the error). C3 did not use masses.
- **Style labels.** Methods give FG examples as “swifts, falcons and **gulls**”, but Table S1 codes *Larus* (and herons, owls, crows, pigeons) as **CF**, exactly as C3 reports from the table. Paper-internal inconsistency; C3 followed the table.
- **Wood pigeon manus.** *Columba palumbus* manus 26.21 mm vs *C. livia* 47.00 and *C. oenas* 48.40, despite the wood pigeon being the larger bird. C3 already flags this in Sources disagree #7. Confirmed in the corrected file.
- **Methods wording.** Specimens are “museum specimens … from the Natural History collections of the National Museum, Ireland”, measured “using digital calipers (rounded to the nearest mm)”. C3’s “National Museum of Ireland” is the usual name. Bone lengths in Table S1 are given to two decimal places, which sits awkwardly with “nearest mm”; fprim values are integers. Not a C3 error.

---

## Pennycuick findings

- **1996 working equation (JEB 199:1613).** Confirmed from the Tethys abstract (https://tethys.pnnl.gov/publications/wingbeat-frequency-birds-steady-cruising-flight-new-data-improved-predictions) and from eq. 2 in the 2001 paper I opened (Tethys PDF):  
  **f = m^(3/8) · g^(1/2) · b^(−23/24) · S^(−1/3) · ρ^(−3/8)**.  
  Full form: f ∝ (mg)^(1/2) · b^(−17/24) · S^(−1/3) · I^(−1/8) · ρ^(−3/8), with **I ∝ m·b²**. “The equation is also numerically correct as it stands, without requiring a multiplication factor.” Scaling: geometrically similar species **f ∝ m^(−1/6)**; mass change only **f ∝ m^(1/2)**. 47 species (32 from 1990 + 15 new) in the abstract. All match C3.
- **1990 equation (JEB 150:171).** Publisher abstract opened at https://journals.biologists.com/jeb/article/150/1/171/5695/Predicting-Wingbeat-Frequency-and-Wavelength-of : 32 species, 18 families, 20 g to nearly 5 kg. A GitHub-hosted copy of the PDF (https://github.com/jgoppert/control-systems/raw/master/papers/pennycuick-1990.pdf) that I opened has: “The constant of proportionality is estimated to be **1.08**”, eq. 10, and Table 5 “chosen” exponents mass **1/3**, span **−1**, area **−1/4**, with g^(1/2) and ρ^(−1/3). C3’s formula is correct. (This PDF is a third-party copy, used only to read the equation; it is not a source to redistribute.)
- **Recompute of every §3.1 f_ref and of the 1990 comparison.** Using g = 9.81 and ρ = 1.23 from Table 3, the 1996 formula reproduces all 16 Table 2 f_ref values to the printed precision (largest difference 0.023 Hz, on the chaffinch 10.8, which is given to 3 significant figures). AR and Q from Table 1 m, b, S match C3 after ordinary rounding. Observed/f_ref for the 14 non-bounding species is 0.809–1.047, which is the paper’s “0.81 to 1.05”. The 1990 formula on the same 16 birds is **2.35–9.78%** below 1996 (C3: 2–10%). Examples I recomputed: mute swan **3.13 vs 3.37 Hz**; eider **5.99 vs 6.60**; wigeon **6.56 vs 7.27**. These are C3’s computed values, not numbers printed as a 1990-vs-1996 pair in the 2001 paper. Grok flagged 3.13 as the herring-gull *observed* frequency (which is also 3.13 Hz). That is a coincidence; the swan 1990 prediction really is 3.13.
- **Bounding correction.** Eq. 8 is **g′ = g/q**. Eq. 10 substitutes g/q into the f_ref formula. C3 cites “eq. 8” for the g′ definition and says g′ is then used in the f_ref equation, which is fair; the substituted formula is eq. 10. Not a FIX.
- **Table 3.** g = 9.81 m s−2, uncertainty 0.01 **as a proportion** (caption: “uncertainties expressed as a proportion of the value”); ρ_SL = 1.23 kg m−3, fixed. C3 is right. Falsterbo, southwest Sweden, confirmed.

---

## Licence findings

| ID | C3 statement | This check |
|---|---|---|
| B1, B2, B3, B12, B13, B14, B15, B21 | © The Company of Biologists (free to read where stated) | Fair. JEB papers are not open-licensed. |
| B4, B5, B6 | CC BY | **Correct** (Crossref / PLOS). |
| B7, B8, B10, B16 | © Royal Society (B16: Author(s), all rights reserved) | Fair for these papers. Stowers PDF: “© 2017 The Author(s) Published by the Royal Society. All rights reserved.” |
| B9 | © Journal of Avian Biology (paywalled) | Fair. |
| B11 | © Nature Publishing Group (paywalled) | Fair. |
| B17 | US government work, normally public domain; no explicit licence on the page | Fair. I opened the pages; no licence statement on the glossary or position guide. |
| B18 | CC BY-SA 4.0; secondary | Correct. |
| B19 | © Oxford University Press | Fair. |
| B20 | Free access (SORA); © Wilson Ornithological Society | Plausible; the PDF itself was not opened here. |
| B22 | CC BY 4.0 | **Correct** (Crossref). |
| **B23** | Paper **CC BY 4.0**; dataset CC0, not obtained | **Paper licence is wrong: CC BY-NC-ND 4.0.** Dataset **CC0** is correct. Dataset files were not obtained. |

---

## Row table

Columns: row, claim, status, note or correction, URL checked.

### 1.1–1.2 Wang 2011 / 2013 Table S1

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 1.1-CF | CF n=90; hu/ta 0.358±0.038 (0.270–0.433); ul/ta 0.359±0.036 (0.297–0.428); mn/ta 0.282±0.052 (0.180–0.388); BI 1.008±0.151 (0.759–1.382); fprim/ta 0.913±0.213 (0.506–1.484) | VERIFIED | Recomputed from corrected Table S1, sample SD. | https://doi.org/10.1371/annotation/6c8c755f-58fc-49e3-b499-8d082318eef6.s001.cn |
| 1.1-FS | FS n=20; 0.344±0.026 (0.292–0.389); 0.400±0.021 (0.374–0.444); 0.257±0.044 (0.167–0.313); 0.860±0.045 (0.741–0.938); 0.789±0.149 (0.515–1.067) | VERIFIED | Same. | same |
| 1.1-FG | FG n=8; 0.280±0.038 (0.204–0.312); 0.369±0.031 (0.313–0.411); 0.351±0.054 (0.320–0.483); 0.762±0.112 (0.640–0.884); 1.547±0.555 (0.985–2.523) | VERIFIED | Same. FG species: *Apus apus, Delichon urbica, Falco columbarius, F. peregrinus, F. subbuteo, F. tinnunculus, Hirundo rustica, Riparia riparia*. | same |
| 1.1-PT | PT n=65; 0.316±0.022 (0.271–0.376); 0.386±0.022 (0.340–0.427); 0.298±0.038 (0.199–0.354); 0.819±0.055 (0.681–0.914); 1.178±0.187 (0.828–1.643) | VERIFIED | Same. | same |
| 1.1-pub | Published fprim/ta means with 95% CI: 0.9131 (0.8685–0.9557), 0.7889 (0.7190–0.8588), 1.5471 (1.0835–2.0107), 1.1782 (1.1319–1.2244) | VERIFIED | Fig. 1a legend. These equal the *arithmetic* means to 4 decimals, not the geometric means of the raw ratios. | https://doi.org/10.1371/journal.pone.0028672 (PDF) |
| 1.1-ANOVA | F(3,182)=37.789, P<0.001; CF vs FS HMCT P=0.062 | VERIFIED | Results and Fig. 1. | same |
| 1.1-notes | Gulls, herons, owls, crows, pigeons coded CF; *Neophron percnopterus* coded PT; *Alcedo atthis* 719 g unused | VERIFIED | All three are in the corrected file. Methods text lists gulls as FG examples; the table (which C3 follows) codes them CF. 719 g is implausible (BTO 39.1±3.5 g, N=554). | corrected S1; https://www.bto.org/learn/about-birds/birdfacts/kingfisher |
| 1.2-spp (25 rows) | *Passer domesticus* through *Cygnus olor*: lengths, style, ta, 2-decimal fractions, BI, fprim/ta | VERIFIED | Every cell matches the corrected file, including *Columba palumbus* manus 26.21 (doubtful, already flagged by C3). | corrected S1 |

### 1.3 Scaling and Weeks error

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 1.3-ta | Total arm ∝ M^0.37–0.39 (positive allometry); n not in abstract | VERIFIED (abstract) | Full text not opened. Abstract matches. | https://research.manchester.ac.uk/en/publications/wing-bone-length-allometry-in-birds/ ; https://doi.org/10.1111/j.2007.0908-8857.03913.x |
| 1.3-span | Wingspan ∝ M^0.35 (CAIC) and ∝ M^0.39 (species as points) | VERIFIED (abstract) | Same abstract. | same |
| 1.3-bones | Similar exponents for each bone; isometric with total arm | VERIFIED (abstract) | Same abstract. | same |
| 1.3-BI-prim | BI>1 primitive for modern birds; varies between clades at every taxonomic level | VERIFIED (abstract) | Nudds, Dyke & Rayner 2004 abstract. | https://europepmc.org/articles/PMC1810057 ; https://doi.org/10.1098/rsbl.2004.0167 |
| 1.3-BI-list | Groups with BI<1 (Piciformes, higher landbirds, swifts/hummingbirds, Megapodidae, etc.) | UNVERIFIABLE | C3 already Unverified. Electronic appendix not opened (Cloudflare). List is not in the abstract. | — |
| 1.3-Weeks | RMSE humerus 0.47, ulna 0.96, carpometacarpus 1.11 mm; UMMZ, 100 specimens; passerines | VERIFIED | Table 1. Dataset is passerines (2,057 spp., 14,419 skeletons). Licence of the *paper* is wrong in the source list (see B23). | https://doi.org/10.1038/s41597-025-05234-y ; https://www.nature.com/articles/s41597-025-05234-y/tables/1 |

### 2. Wingspan, area, AR, Q (Alerstam 2007)

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 2.1-methods | 33,610 measurements; b = tip-to-tip in the hand; S = both wings plus body strip; weighted means; no spread | VERIFIED | Protocol S1 opening text. 33610 is written without a comma there. | https://doi.org/10.1371/journal.pbio.0050197.sd001 |
| 2.1-spp (29 rows) | *Parus major* through *Pelecanus onocrotalus*: m, b, S from S1; AR=b²/S and Q=m·g/S (g=9.81) | VERIFIED | All 29 triples match S1. AR (1 decimal) and Q (integer) recompute with ordinary rounding. | same |
| 2.2-CF | n=42; AR 8.6±1.4 (5.7–12.2); Q 75±44 (27–166); mass 0.054–10.6 kg | VERIFIED | Recomputed from S1 ∩ Wang styles. | S1 + corrected Wang S1 |
| 2.2-FS | n=16; 7.2±1.2 (5.4–8.9); 47±24 (21–104); 0.277–8.50 | VERIFIED | Same. | same |
| 2.2-FG | n=7; 8.1±0.7 (7.5–9.5); 27±17 (12–62); 0.015–0.789 | VERIFIED | Same. | same |
| 2.2-PT | n=19; 5.5±0.5 (4.4–6.2); 21±7 (12–34); 0.009–0.162; vulture excluded | VERIFIED | Recomputation matches once linnet is joined as *Carduelis cannabina* (Alerstam) = *Acanthis cannabina* (Wang, PT) and *Neophron percnopterus* is dropped. | same |
| 2.3-mass-all | Ue=15.9 (15.2–16.7) × mass^0.13 (0.11–0.15); n=138 | VERIFIED | Table 1. RMA; CIs from 100,000 bootstrap replicates. | https://doi.org/10.1371/journal.pbio.0050197 (PDF) |
| 2.3-mass-10 | 16.0 (15.0–17.0), exponent 0.13 (0.10–0.16); n=56 (≥10 tracks) | VERIFIED | Table 1. | same |
| 2.3-mass-contr | exponent 0.12 (0.07–0.16); n=17 contrasts; a not given | VERIFIED | Table 1. | same |
| 2.3-Q-all | Ue=4.3 (4.0–4.6) × Q^0.31 (0.27–0.35); n=129 | VERIFIED | Table 1. | same |
| 2.3-Q-contr | exponent 0.32 (0.24–0.40); n=17 | VERIFIED | Table 1. | same |
| 2.3-AR | AR rises significantly with mass and with Q (p<0.01, n=129; p<0.05 by contrasts, n=17); slope not in text | VERIFIED | Results paragraph immediately after Table 1. Predicted exponents 1/6 and 1/2 are in the Table 1 footnote. | same |
| 2.4-main | 105 species; wing shape mostly phylogeny and ecology; only a weak relationship with flight style, wing loading and aerodynamic variables; most variation in primary-covert extent | VERIFIED (abstract) | Abstract of Wang & Clarke 2015. | https://doi.org/10.1098/rspb.2015.1935 ; Europe PMC abstract (PMC4614784) |
| 2.4-498 | 498 spread-wing specimens; 90 landmarks (60 outline + 30 coverts) | UNVERIFIABLE | 105 species is in the abstract. 498 and the 90-landmark split were not in any full text I could open (PMC/Royal Society blocked). | same, full text not obtained |
| 2.4-r2 | r² AR vs PC1 0.32/0.34 (Hackett) and 0.50/0.53 (Jarvis) | UNVERIFIABLE | Not in the abstract. Full text not obtained. | same |
| 2.4-pc1 | PC1 30.8% dorsal, 40.06% ventral | UNVERIFIABLE | Not in the abstract. Full text not obtained. | same |

### 3–3.1 Wingbeat equations and Pennycuick 2001 table

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 3-1990 | f=1.08·m^(1/3)·g^(1/2)·b^(−1)·S^(−1/4)·ρ^(−1/3); 32 spp., 18 families, 20 g to nearly 5 kg | VERIFIED | Abstract (sample) + eq. 10 and Table 5 exponents in the 1990 PDF I opened. | https://journals.biologists.com/jeb/article/150/1/171/5695 ; GitHub-hosted PDF (read only) https://github.com/jgoppert/control-systems/raw/master/papers/pennycuick-1990.pdf |
| 3-1996-full | f∝(mg)^(1/2)·b^(−17/24)·S^(−1/3)·I^(−1/8)·ρ^(−3/8); 47 species | VERIFIED | 1996 abstract (Tethys). | https://tethys.pnnl.gov/publications/wingbeat-frequency-birds-steady-cruising-flight-new-data-improved-predictions ; https://doi.org/10.1242/jeb.199.7.1613 |
| 3-1996-work | Working equation as above; I∝m·b²; no multiplying constant | VERIFIED | Same abstract; restated as 2001 eq. 2. | same, and https://tethys.pnnl.gov/sites/default/files/publications/Pennycuick_2001.pdf |
| 3-scale-sim | f∝m^(−1/6) between geometrically similar species | VERIFIED | 1996 abstract. | Tethys 1996 |
| 3-scale-mass | f∝m^(1/2) when only mass changes | VERIFIED | 1996 abstract. | same |
| 3-const | g=9.81; ρ=1.23; g uncertainty 0.01 as a proportion | VERIFIED | 2001 Table 3 and caption. | Pennycuick 2001 PDF |
| 3-ratio | Observed f is 0.81–1.05×f_ref in 14 of 16; starling and chaffinch higher | VERIFIED | Abstract and Results. Recompute 0.809–1.047 for the 14. | same |
| 3-bound | g′=g/q used in f_ref predicts the bounders; C3 cites eq. 8 | VERIFIED | Eq. 8 is g′=g/q; the substituted f_ref formula is eq. 10. | same |
| 3-St | St=f·A/U; cruising birds at the lower end of 0.2<St<0.4; 60 species | VERIFIED (abstract) | Nudds, Taylor & Thomas 2004 abstract; Taylor et al. 2003 abstract supports the 0.2–0.4 interval. Full texts not opened. | https://doi.org/10.1098/rspb.2004.2838 ; https://doi.org/10.1038/nature02000 |
| 3-theta | θ≈67·b^(−0.24); units (degrees, b in m) inferred | VERIFIED (abstract) | Abstract has the formula. Units are not stated there, as C3 already notes. Degrees are the only sensible unit (67 rad is impossible). | https://doi.org/10.1098/rspb.2004.2838 |
| 3-St-pred | f≈St·U/(b·sin(33.5·b^(−0.24))); St≈0.21 continuous / 0.25 intermittent; 90% of variance; n=60 | VERIFIED (abstract) | Abstract says “direct and intermittent fliers” rather than “continuous flappers”; the paraphrase is standard. | same |
| 3.1-spp (16 rows) | Pennycuick 2001 Tables 1–2: m, b, S, obs f, f_ref, airspeed, Vmp, with SDs / uncertainties | VERIFIED | Every printed figure matches. AR and Q recomputed. f_ref recomputed from the 1996 equation. | 2001 Tethys PDF, Tables 1–2 |
| 3.1-1990c | 1990 equation 2–10% below f_ref; swan 3.13 vs 3.37; eider 5.99 vs 6.60 | VERIFIED | Independent recompute (also wigeon 6.56 vs 7.27, as in Sources disagree). These are C3’s computed values, not a 2001 table of 1990 predictions. | computed from Table 1 and 1990 eq. 10 |
| 3.1-magpie | Frequency “did not change appreciably” with speed; magpie 4–14 and pigeon 6–20 m s−1; pigeons flex more in glides as speed rises; magpie glide span unchanged | VERIFIED (abstract) | Tobalske & Dial 1996 abstract. Pigeon gait change at about 10 m s−1 is also in this abstract (transitional vortex-ring gait at 10 m s−1). | https://doi.org/10.1242/jeb.199.2.263 ; PubMed 9317775 |
| 3.1-finch | Zebra finch 13.2 g, N=4, 0–14 m s−1; flap-bounds at every speed; downstroke % unchanged; max body lift 0.0206 N (15.9% of weight) at 10 m s−1 | VERIFIED (abstract) | Tobalske, Peacock & Dial 1999 abstract. | https://doi.org/10.1242/jeb.202.13.1725 |

### 4–5 Stroke, intermittent flight, folding

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 4-phase | Upstroke→downstroke ≈40–50%; first quarter of downstroke ≈50–63%; second ≈63–75%; last half ≈75–100%; 4 birds each, 3rd–5th wingbeat (PIV) | VERIFIED | Crandell & Tobalske 2015 Methods (PIV samples on the 3rd–5th wingbeat). Kinematic traces themselves use the third wingbeat. | https://journals.biologists.com/jeb/article/218/16/2518/14222 (WebFetch full HTML) |
| 4-hand | Dove hand-wing 80.2±3.2% of wing length; finch 64.8±5.2% | VERIFIED | Results. | same |
| 4-tip | Dove 8.4±0.6 (up), 9.9±1.3 (down); finch 7.5±3.3 (up), 11.0±1.9 (down) m s−1 | VERIFIED | Results. | same |
| 4-ang | Dove 15.3±0.8 mid-up, 19.4±2.1 mid-down; finch 45.4±13.8 at the transition, deg ms−1; abstract 45.5 vs Results 45.4 | VERIFIED | Both figures are in the paper, as C3 says. | same |
| 4-aoa | Dove min −81.3±4° in first third of upstroke; finch min −45.9±7.3° upstroke, peak 51.5±7.4° downstroke | VERIFIED | Results. | same |
| 4-clap | Leading edges meet ~3 ms before trailing edges, at ~5% of the cycle; trailing edges separate 4–9 ms later | VERIFIED | Results, dove clap and peel. | same |
| 4-gait | Two-gait scheme replaced by continuous variation; pigeon gait change at ~10 m s−1 | VERIFIED | Tobalske 2007 full HTML; 1996 abstract for the 10 m s−1 transition. | https://journals.biologists.com/jeb/article/210/18/3135/17027 ; B13 abstract |
| 4-inter | Both flap-glide and flap-bound: under ~300 g or pointed high-AR wings; over 300 g (e.g. pigeons) glide but do not bound; under ~30 g with rounded wings almost only bound | VERIFIED | Tobalske 2007 review text, almost word for word. | JEB 210:3135 WebFetch |
| 4-synth | Qualitative downstroke / tip-reversal / flexed / feathered / swept-wing account; galliform tip-reversal at take-off | VERIFIED | Matches B12 (pointed vs rounded kinematics) and B15 discussion (galliforms, citing Tobalske & Dial 2000). Span-ratio sentence not independently hunted down in B12; no contradictory number found. | B12, B13, B15 |
| 5-mech | Six-bar, not planar four-bar; two spherical joints at elbow, pin at radius–radiale, slider (ulnare along ulna), two spherical to carpometacarpus; 8 DoF; 3 pigeons, mocap + micro-CT in 7 poses | VERIFIED | Stowers, Matloff & Lentink 2017, author PDF. “These joints together comprise 8 DOF.” | https://lentinklab.com/wp-content/uploads/2022/06/1510366770_2017-avian-morphing-stowers.pdf ; https://doi.org/10.1098/rsif.2017.0224 |
| 5-45 | At full flex, radius/ulna + carpometacarpus plane deviates ~45° from the anatomical plane | VERIFIED | Results, “about 45°”. | same |
| 5-err | Bones leave best-fit plane by 5.0±0.7 mm | VERIFIED | “5.0 ± 0.7 mm for all three pigeons.” | same |
| 5-d2 | Digit 2 moves independently by up to about 30° | VERIFIED | Fig. 2 caption. | same |
| 5-glide | Gliding flex/extension follows a narrow path within the full wrist range | VERIFIED | Results. | same |
| 5-speed | Pigeons flexed more as glide speed rose; magpie glide span did not change | VERIFIED (abstract) | Tobalske & Dial 1996 abstract. | B13 abstract |

### 6 Feathers

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 6-p-usfws | “Most bird species have 9–10”; “Most groups of birds have 10” primaries | VERIFIED | Glossary and position guide, respectively. | https://www.fws.gov/lab/featheratlas/glossary.html ; https://www.fws.gov/lab/featheratlas/id-position.html (WebFetch) |
| 6-p-wiki | Typically 11 (6+5), remicle often tiny/absent; non-passerines 9–11; grebes, storks, flamingos 12; ostriches 16; most passerines 10, some 9 | UNVERIFIABLE | C3 already Unverified. Wikipedia wikitext (MediaWiki API, 2026-11-16) matches this wording. Still a secondary source. | https://en.wikipedia.org/wiki/Flight_feather |
| 6-s-usfws | Secondaries 9–25 depending on species | VERIFIED | Glossary. | USFWS glossary |
| 6-s-wiki | 6 (hummingbirds) to ~40 (some albatrosses); >40 non-passerine families lack the 5th secondary (diastataxy) | UNVERIFIABLE | Wikipedia matches. Secondary, as C3 says. | Wikipedia Flight feather |
| 6-tert | Tertials “usually 3–4” | VERIFIED | Glossary. | USFWS glossary |
| 6-r-usfws | Rectrices “most 10–12”; most common number 12 | VERIFIED | Glossary: most species 10–12. Position guide: “The most common number of tail feathers is 12.” | both USFWS pages |
| 6-r-wiki | Always paired; most have 6 pairs; absent in grebes and some ratites; many grouse >12; varies in ruffed grouse, hazel grouse, common snipe; domestic pigeons vary widely | UNVERIFIABLE | Wikipedia matches. | Wikipedia |
| 6-ratite | Cassowary remiges reduced to 5–6 bare quills; most ratites have no rectrices (ostrich does); penguins have no differentiated flight feathers | UNVERIFIABLE | Wikipedia “Vestigiality in flightless birds” matches. | Wikipedia |
| 6-fprim | Primary length ~0.79×ta in soarers and 1.55×ta in flap-gliders | VERIFIED | These are the published back-transformed (here: arithmetic) means 0.7889 and 1.5471, rounded. | Wang 2011 Fig. 1 |

### 7 Walking vs hopping

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 7-def | Hopping = legs together; walking/running = alternate | UNVERIFIABLE | C3 cites Clark 1975. The PDF was 403 from this box. Not a showing that the claim is false. | https://digitalcommons.usf.edu/wilson_bulletin/vol87/iss3/9 (landing HTML only) |
| 7-who | Arboreal species typically hop; more terrestrial usually walk/run; 47 species (US, England, Kenya) plus literature | UNVERIFIABLE | Same: full text not opened. | same |
| 7-walk | Pinyon jays walk; Australian magpie walks, pied currawong hops; Furnariinae walk/run; Formicariidae hop except terrestrial forms; common grackles usually walk, sometimes hop | UNVERIFIABLE | Same. | same |
| 7-mixed | At least 19 passerine higher taxa contain both; adds 11 to Kunkel’s (1962) 8 | UNVERIFIABLE | Same. | same |
| 7-same | Gait can change with age (*Mimus*, *Gymnorhinus*), substrate, speed, display; house sparrows hop at all ages | UNVERIFIABLE | Same. | same |
| 7-evo | Hopping and arboreality derived and correlated; >1000 species; 47 species micro-CT; pelvis (hip and knee extensor origins); habitat more than gait shapes the tarsometatarsus | VERIFIED (abstract) | Provini & Höfling 2020 abstract. Full text not opened. | https://doi.org/10.1093/sysbio/syaa015 |
| 7-magpie | Walk, run and out-of-phase hop; similar joint flexion; prefer hopping to running above walking speeds | VERIFIED (abstract) | Verstappen et al. 2000 abstract. | https://doi.org/10.1242/jeb.203.14.2159 |
| 7-ground | Quail, oystercatcher, lapwing, pigeon, avocet: mixed gaits more often than pure walking, perhaps as often as grounded running; >1 million frames | VERIFIED (abstract) | Andrada et al. 2015 abstract (Europe PMC / Sci. Rep.). | https://doi.org/10.1038/srep13636 ; PMC4558583 |

C3’s Unverified rows are not presented as fact in other sections. Wikipedia and USFWS counts are compared in Sources disagree, with both sources named. Wang masses are not used in any table.

---

## Source table

| Key | URL | Resolves y/n | Licence correct y/n |
|---|---|---|---|
| B1 | https://doi.org/10.1242/jeb.150.1.171 ; Bristol record; abstract at /jeb/article/150/1/171/5695 | y (abstract / WebFetch; curl 403 on some JEB URLs; a GitHub-hosted PDF of the paper was also opened) | y (© CoB, not open-licensed) |
| B2 | https://doi.org/10.1242/jeb.199.7.1613 ; https://tethys.pnnl.gov/publications/wingbeat-frequency-birds-steady-cruising-flight-new-data-improved-predictions | y (Tethys abstract 200; JEB curl 403; one JEB HTML URL pointed at the wrong article) | y |
| B3 | https://doi.org/10.1242/jeb.204.19.3283 ; https://tethys.pnnl.gov/sites/default/files/publications/Pennycuick_2001.pdf | y (Tethys PDF 200) | y |
| B4 | https://doi.org/10.1371/journal.pone.0028672 ; PMC3233598 | y (PDF 200) | y (CC BY 4.0) |
| B5 | https://doi.org/10.1371/annotation/6c8c755f-58fc-49e3-b499-8d082318eef6 and .s001.cn | y (HTML + corrected Table S1 200) | y (CC BY 4.0) |
| B6 | https://doi.org/10.1371/journal.pbio.0050197 ; Protocol S1 .sd001 | y (PDF and S1 200) | y (CC BY 4.0) |
| B7 | https://doi.org/10.1098/rspb.2015.1935 ; PMC4614784 | y (DOI exists; PMC/RS full text 403 or reCAPTCHA from this box; abstract obtained) | y (© Royal Society, as C3 says) |
| B8 | https://doi.org/10.1098/rsbl.2004.0167 ; Europe PMC PMC1810057 | y (abstract; full scan / appendix not opened) | y |
| B9 | https://doi.org/10.1111/j.2007.0908-8857.03913.x ; Manchester repository page | y (abstract / Manchester page; Wiley full text not opened; C3 reported DOI HTTP 500) | y |
| B10 | https://doi.org/10.1098/rspb.2004.2838 ; PMC1691825 | y (abstract; full text not opened) | y |
| B11 | https://doi.org/10.1038/nature02000 | y (abstract via Europe PMC) | y |
| B12 | https://doi.org/10.1242/jeb.000273 ; JEB HTML 210/18/3135 | y (WebFetch full HTML) | y |
| B13 | https://doi.org/10.1242/jeb.199.2.263 ; PubMed 9317775 | y (abstract; curl 403) | y |
| B14 | https://doi.org/10.1242/jeb.202.13.1725 | y (abstract) | y |
| B15 | https://doi.org/10.1242/jeb.116228 ; JEB HTML 218/16/2518 | y (WebFetch full HTML) | y |
| B16 | https://doi.org/10.1098/rsif.2017.0224 ; author PDF on lentinklab.com | y (author PDF 200) | y (all rights reserved) |
| B17 | https://www.fws.gov/lab/featheratlas/glossary.html ; …/id-position.html | y (WebFetch 200; curl 403) | y (cautious public-domain wording is fair) |
| B18 | https://en.wikipedia.org/wiki/Flight_feather | y (MediaWiki API 200) | y (CC BY-SA 4.0; secondary) |
| B19 | https://doi.org/10.1093/sysbio/syaa015 | y (abstract) | y |
| B20 | https://digitalcommons.usf.edu/wilson_bulletin/vol87/iss3/9 | landing page y; **PDF 403** | licence not checked on the PDF |
| B21 | https://doi.org/10.1242/jeb.203.14.2159 | y (abstract) | y |
| B22 | https://doi.org/10.1038/srep13636 ; PMC4558583 | y (abstract / Europe PMC; PMC PDF blocked to curl) | y (CC BY 4.0) |
| B23 | https://doi.org/10.1038/s41597-025-05234-y ; Dryad 10.5061/dryad.v41ns1s4c | y (paper PDF 200; Dryad landing 200; dataset files not downloaded) | **n for the paper (CC BY-NC-ND 4.0, not CC BY 4.0)**; dataset CC0 y |

---

## Grok live-search pass

Command as above; exit 0 at 10:57:08 AEDT; report saved at `/workspace/c3verify/grok_c3_report.md` (scratch, not a source). I opened the URLs it relied on before using a number.

Conflicts it raised, and how they were treated:

1. Wang 2013 left a wrong mass column; no later mass erratum; *Alcedo* 719 g vs BTO 39.1 g. **Agreed** — already flagged by C3, masses unused.
2. Methods 184 vs Fig. 2 / file 183. **Agreed** — already in C3 Sources disagree #7.
3. Methods list gulls as FG; Table S1 codes them CF; *Neophron* is PT. **Agreed** — C3 followed the table and flagged the vulture.
4. Swan 3.13 vs 3.37 and eider 5.99 vs 6.60 “not in the 2001 paper”; 3.13 Hz is herring-gull observed f. **Rejected.** Those figures are C3’s stated recomputation of the 1990 equation on the 2001 Table 1 inputs. I recomputed them independently; they are correct. The match to herring-gull observed frequency is a coincidence.
5. Crandell abstract 45.5 vs Results 45.4; kinematics are the third wingbeat, not “3rd–5th.” **Partly agreed, not a FIX.** C3 already records the 45.5/45.4 typo. The phase-timing row correctly cites Methods (PIV on the 3rd–5th wingbeat). The kinematic numbers are from the third wingbeat; C3’s “as above” slightly stretches that population line.
6. Weeks paper is CC BY-NC-ND 4.0, not CC BY 4.0. **Agreed — this is the FIX.** I opened the PDF, Crossref, and Europe PMC.
7. Wang & Clarke 498 specimens and AR–PC1 r² not in the PMC HTML it opened. **Agreed that I could not verify those figures** (full text blocked here). 105 species and the qualitative result are in the abstract.
8. Nudds 2004 BI clade list / appendix not opened. **Agreed** — already Unverified in C3.
9. Nudds et al. 2004 θ units not in the abstract. **Agreed** — already noted by C3.

Grok did not surface any further numerical error in the C3 tables that I then confirmed.
