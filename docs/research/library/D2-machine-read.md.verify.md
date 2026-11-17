# Verification: D2-machine-read.md

Checked: 17 November 2026, Australia/Sydney (AEDT, UTC+11).
Source file: `/workspace/specula-research/D2-machine-read.md`
Output path (this file only): `/workspace/specula-research/D2-machine-read.md.verify.md`

Private adversarial source-check. No number was taken from Grok, a search snippet, or a pirate mirror. Every figure below was opened in a publisher page, official PDF or official preview, author PDF, government document, Europe PMC / Crossref / MEDLINE, or the official Getty Publications PDF. Key table and figure pages were also rendered and read as images. D2 itself was not edited.

## Summary

- **67 data rows** checked (2.1–2.13 = 13; 3.1–3.11 = 11; ISO 2768-1 Table 1's 8 size ranges + rows 4.1–4.3 = 11; 5.1–5.22 = 22; 6.1–6.10 = 10). ISO 2768-1 Tables 2 and 3, given in prose rather than numbered rows, were also checked and match.
- **47 VERIFIED.** Almost every D2 “Verified” number that I could open matches the source, with those units and that population. Two D2 “Unverified” rows are upgraded here: row 2.12 (Alexander & Shansky 1976), after the official Springer abstract was opened; and row 5.8 (FAA Fig. 4-5), after the official full AC 43.13-1B Change 1, p. 4-20, was opened and the figure was read as an image.
- **5 FIXES** (no invented numbers; corrected value and source below):
  1. **Rows 5.18–5.19 — wrong page.** Holm & Mattsson 1982, as quoted by Scott 2002, is on **printed p. 53**, not p. 70. The numbers themselves are correct. World Data read a third-party flipbook at `…/page70.html`; that is flipbook page 70, whose footer is book page 53. Official Getty PDF: `https://media.getty.edu/Text/a33da677-13c2-5f35-8954-7c4e1eef454b.pdf` (PDF page 70 = printed p. 53). Official printed p. 70 is a different passage (marine wrecks / Pourbaix), with no Holm & Mattsson content.
  2. **Row 5.21 — elapsed-time arithmetic.** 34 × 31.6 min = 1,074.4 min ≈ 17.9 h is a correct product, but Fig. 2 lists “average time interval *between* frames”, so the span of a 34-frame sequence is **(34 − 1) × 31.6 = 1,042.8 min ≈ 17.4 h**. Source: Gu et al. 2006, Fig. 2 caption, author PDF `https://cave.cs.columbia.edu/Statics/publications/pdfs/Gu_TOG06.pdf`.
  3. **Row 3.8 — Hecht 1996 vs the NTRS abstract.** The claims “3 experiments”, “better than chance but well below what the information allows”, and “3 diameters” are in the **NASA NTRS 1994 abstract** (`https://ntrs.nasa.gov/citations/20020006041`; `onlyAbstract: true`), not in the 1996 *Perception & Psychophysics* abstract. The official journal abstract (Europe PMC / doi 10.3758/BF03206833) reports **five** experiments and says observers were “much more sensitive to average velocity than to the gravitational acceleration pattern.” Do not treat the NTRS abstract as the journal article’s abstract.
  4. **Row 3.4 — analysed n.** Ujitoko et al. 2023 recruited 128 (16 M + 16 F in each of the 20s–50s; mean age 40.43, SD 10.93), but “Consequently, the data from **78** participants were analyzed,” after excluding people whose PCs were not at 60 fps and 4 pixels/mm. The heaviness results rest on n=78, not n=128. Official PDF: `https://www.frontiersin.org/articles/10.3389/fpsyg.2023.1042188/pdf`.
- **15 UNVERIFIABLE**, all already marked Unverified in D2, and still unopened in a primary text in this pass: rows 4.2 (ISO 286-1 Table 1), 4.3 (ISO 1302), 5.6 (VW ID.3, unofficial host), 5.17 (ISO 9224), 5.22 (AAMA 2605), and the ten §6 Grok leads (6.1–6.10). I did not take those Grok numbers as verified.
- **Rows skipped:** none of the 67.
- **Dead links:** none of the 27 citation URLs in §11 is a known-dead identifier. Scripted fetches returned **403 or 500 bot blocks** on some publisher landings (SAGE dois 10.1177/0301006616684369 and 10.1068/p210069; JOV/ARVO 10.1167/13.4.12; Springer 10.3758/BF03199388 on one pass). Those works still resolve in Crossref, and official abstracts or author PDFs were opened as noted below. PubMed abstract pages returned **HTTP 203** with content; MEDLINE/Europe PMC abstracts were used instead. The FHWA ROSA P PDF (`…/dot_49753_DS1.pdf`) returns **403** with a browser user-agent and **200** with default curl (2,934,767-byte PDF), as D2 already warned. Crossref returns **404** for the unused wrong DOI 10.21595/jve.2024.23830 (already corrected in D2 §8).
- **Worst problems:** (1) Scott 2002 p. 70 is the wrong page for Holm & Mattsson — the official free Getty PDF was opened and the passage is on p. 53; (2) Hecht’s NTRS abstract and the 1996 journal abstract disagree on the number of experiments and on the wording of the result; (3) Ujitoko’s heaviness tests used 78 analysed participants, not the 128 recruited; (4) the copper-patina “17.9 h” restatement counts one interval too many.

One-line summary: D2 is solid on the opened measured and standards rows, including ISO 2768-1 Tables 1–3, ISO 9223 Table 2, Pang Table 1’s asymmetric bonnet tolerances, Duraiswamy n=47, FAA/FHWA fastener rules, and the Todd & Norman CC BY-NC-ND 4.0 licence; fix the Scott page to p. 53, report Ujitoko’s analysed n as 78, prefer (n−1)×interval for Gu’s copper sequence (~17.4 h), and stop citing the NTRS abstract as if it were the 1996 Hecht paper.

## File snapshot

At the start of this check (11:28:23 AEDT) D2 was copied to `/workspace/lv-scratch/d2/` (not `/tmp`). Snapshot: **40,578 bytes**, sha256 `40f2c32da1ba1081d93ca313739a263dce3182b7b680f5986f71581592915742`, 246 lines (67 data rows). World Data’s reported mtime 17 November 2026, **11:27:13 AEDT**, size 40,578 bytes, and 50 Verified / 17 Unverified all match this file.

Immediately before this verify file was finished (11:47:12 AEDT), `sha256sum` and `cmp` of the workspace file against that snapshot were identical (`40f2c32da1ba1081d93ca313739a263dce3182b7b680f5986f71581592915742`, 40,578 bytes). **No content change; no row re-check was required for a moved file.** D2 was not edited.

## Close checks

### 1. Licences

**Todd & Norman 2026**, *J. Vision* 26(2):1, doi 10.1167/jov.26.2.1, PMC12875344. Opened: Europe PMC fullTextXML and Crossref. The article permissions block is “Creative Commons Attribution-NonCommercial-NoDerivatives 4.0 International License” (`creativecommons.org/licenses/by-nc-nd/4.0/`). Crossref licence URL is the same, start 2 Feb 2026. D2’s **CC BY-NC-ND 4.0 (non-commercial, no derivatives); facts only** is **correct**. Grok’s “CC BY” was already corrected in D2 §8.

**Pang, Huang & Chen 2024**, *Vibroengineering Procedia* 54, doi **10.21595/vp.2024.23830**, PDF `https://www.extrica.com/article/23830/pdf`. Authors, venue and DOI confirmed (not “Wang et al.”, not 10.21595/jve.2024.23830 — Crossref 404 for that DOI). Printed on p. 1: “This is an open access article distributed under the Creative Commons Attribution License, which permits unrestricted use, distribution, and reproduction in any medium, provided the original work is properly cited.” D2’s **CC BY** is **correct**.

**ISO 2768-1:1989 and ISO 9223:2012.** Official iTeh preview PDFs opened and © ISO; facts only, as D2 said.

**FAA AC 43.13-1B Change 1** and **FHWA-IF-12-052** are US Government works (public domain). D2 correctly notes that AASHTO rules quoted in the FHWA handbook are © AASHTO (facts only).

**Peters 2015** PLOS ONE HTML carries `creativecommons.org/licenses/by/4.0/`. **Buckingham & Goodale 2013** (published 23 January 2013) prints an unversioned “Creative Commons Attribution License” and does not give a 4.0 URL; CC BY 4.0 was released in November 2013, so D2’s “CC BY 4.0” for S8 is slightly strong — record it as CC BY (unversioned on the article). **Ujitoko 2023** PDF: “Creative Commons Attribution License (CC BY).” Peters and Ujitoko match D2.

**Deng & Kahn 2009** accepted manuscript (Wharton): © AMA / now listed by Crossref under SAGE; facts only, as D2 said. **Zhou 2017**, **Reitsma & Pollard 2003**, **Gu 2006**, **Hamrick 2016**, **Hamilton 2007**, **Walker 2017**, **Runeson 1981**, **Alexander 1976**, **Watson 1992**, **Hecht 1996**, **Buckingham 2009**, and **Scott 2002** (© 2002 The J. Paul Getty Trust): copyrighted; facts only, as D2 said. **Duraiswamy 2018**: Design Society proceedings, free download, no open licence printed — facts only, as D2 said.

**Gu 2006 STAF dataset.** Opened `https://www.cs.columbia.edu/CAVE/databases/staf/STAF_about.html`. The page describes 26 samples and 16 × 80 = 1,280 measurements, and points to a request email (`staf@lists.cs.columbia.edu`). **No licence or terms of use are stated.** D2’s “not found / not downloaded” is **correct**.

### 2. Abstract-only rows (full texts or official abstracts)

| Row | Source | What I opened | Result |
|---|---|---|---|
| 2.8 | Buckingham, Cant, Goodale 2009 | Official abstract (MEDLINE/PubMed 19793879; Crossref) and author PDF at UTSC (`https://www.utsc.utoronto.ca/~jcant/docs/3-Buckingham_Cant_Goodale_JournalNeurophysiology_2009.pdf`) | Directional claim matches: for initial lifts, “the metal block was gripped and lifted with more force than the polystyrene one.” No n in the abstract; D2 correctly used the abstract only and did not import an n. VERIFIED. |
| 2.11 | Walker, Scallon, Francis 2017 | Official abstract (Europe PMC / Crossref) and Sunway repository PDF of the article | “When hidden objects varying independently in size and mass are lifted, objects that feel heavier are judged to be darker and to make lower pitch sounds.” Bidirectionality is in the abstract. No numbers in the abstract, as D2 said. VERIFIED [abstract-only / author PDF qualitative]. |
| 2.12 | Alexander & Shansky 1976 | Official Springer article page abstract (doi 10.3758/BF03199388). Full text not opened (PDF 500 / paywall). | “The apparent heaviness of colors is an increasing function of chroma or saturation and a decreasing function of value or lightness. Hue has little influence.” Matches D2’s qualitative claim. Still no numbers. **Upgraded** from Unverified [search-index-only] to **VERIFIED [abstract-only]**. |
| 2.13 | Cholewiak, Fleming, Singh 2013 | Official abstract (Europe PMC PMC3736874; JOV landing 403 to this fetch). Full text XML 500; JOV PDFs 403. | “Estimates of overall object stability are strongly biased toward the minimum critical angle (i.e., the critical angle in the least stable direction).” Asymmetric skewed conical frustums; 3 experiments. VERIFIED [abstract-only]. |
| 3.1 | Runeson & Frykholm 1981 | Official MEDLINE abstract (PMID 6457088) | “Videotaped events… visible only as 21 bright patches… average slope of .87… pooled standard deviation of 3.8 kg. … live action. Average slopes of 1.00 in the visual mode and 1.20 in the haptic mode… standard deviations of 3.1 kg and 2.0 kg.” No n in the abstract. D2 was right not to use Grok’s n=26. VERIFIED [abstract-only]. |
| 3.2 | Hamilton et al. 2007 | Official abstract (Europe PMC PMC2637436) and author PDFs (Wolpert lab; antoniahamilton.com). Europe PMC fullTextXML 500. | “Observers rely most on the duration of the lifting movement… make less use of the durations of the grasp phase… or the place phase… duration of the grasp component is the best predictor of true box weight.” VERIFIED [abstract-only]; author PDF does not contradict the abstract-only wording D2 used. |
| 3.7 | Watson et al. 1992 | Official Europe PMC / Crossref abstract (SAGE HTML 403) | “Formal analysis supports the claim… Informal evidence from judgments of realism in films…” No data in the abstract. VERIFIED [abstract-only]. |
| 3.8 | Hecht, Kaiser, Banks 1996 | NTRS 20020006041 abstract **and** the 1996 journal abstract | **FIX** — see Summary item 3 and the row table. |

### 3. Second-hand rows

**Todd & Warren 1982 via Todd & Norman 2026, Fig. 1 caption** (Europe PMC XML). Exact wording: “In both cases, the blue object is 50% heavier than the red object and e = 0.5. When the objects approached each other with the same speed (left), the observers were 95% accurate. However, when a moving object collided with a stationary one (right), the observers were only 23% accurate.” Mass ratio 1.5 = 50% heavier. Row 3.10 **VERIFIED [secondary-citation]**.

Row 3.11: “A four-parameter logistic regression analysis was performed to compute the best fitting sigmoidal function… this model produces a marginally better fit to the data than the noisy Newton model (r = 0.95) with a smaller number of free parameters, and the average error of the predicted responses was only 1.4%.” The noisy Newton model is given r = 0.93 earlier in the same Results section, so r = 0.95 and 1.4% belong to the post-collision-velocity sigmoid. D2’s reading is fair. VERIFIED.

**Holm & Mattsson via Scott 2002.** Official Getty PDF printed **p. 53** (not p. 70) confirms, word for word, 36 copper alloys, sheet and rod, rural / marine / urban, sixteen years; greenish patina on most alloys after six to seven years at urban and marine sites; no distinct green at the rural site after sixteen years (black or brown only); average penetration 0.3–0.5 / 0.5–0.9 / 0.9–1.3 µm per year; rates apply to the first few years; once a film has formed the rate drops substantially, “usually after the first ten years.” Values **VERIFIED**; page citation is a **FIX** (see Summary).

### 4. Arithmetic (rows 2.5, 4.1, 5.12, 5.21)

- **2.5.** Peters 2015 fits “WR = .613VR + .114” after natural-log transforms, i.e. log weight ratio = 0.613 · log volume ratio + 0.114, so the exponent on volume is 0.613 (there is also a small intercept: WR = e^{0.114} · VR^{0.613}). “Weight scales about as volume^0.61” and “doubling visible volume should not double expected mass” are a fair restatement of that exponent. VERIFIED.
- **4.1.** ISO 2768-1 Table 1, over 6 to 30 mm: f ±0.1 mm, v ±1 mm. 1 / 0.1 = 10. VERIFIED.
- **5.12.** Min bolt edge 1-1/2 in for a 7/8 in (0.875 in) bolt: 1.5 / 0.875 ≈ 1.714 ≈ 1.7d. Rivet edge is 2d. “About 1.5–2 diameters from the edge” is a fair combined restatement of those two rules. Pitch between 3d and the sealing maximum follows 5.7 and 5.9–5.10. VERIFIED as labelled [own-arithmetic].
- **5.21.** **FIX** — see Summary. Use **(34 − 1) × 31.6 min = 1,042.8 min ≈ 17.4 h**, not 17.9 h.

### 5. Page-image reads: Pang Table 1 and Gu Fig. 2

**Pang 2024 Table 1, journal p. 41** (PDF p. 2), rendered and read. “There is no unified reference value for component flush in the industry” is verbatim. Fender to engine-compartment cover (bonnet): A 5±1.5, B 4±1.5, C **3.5<sup>1</sup>, 0**, D **3<sup>1</sup>, 0** mm — i.e. 3.5 (+1/0) and 3 (+1/0), as D2 read. Fender to front door outer panel: A 5±1.2, B 4.5±1, C 4±0.8, D 3.5±0.75. Fender to headlamp: A 3±1.5, B 2±1.5, C 2±0.8, D 1±0.5 (D2 abbreviated the middle classes with an ellipsis; the endpoints are right). A further bumper row (not claimed in D2) uses the same +/0 superscript style. Rows 5.4–5.5 **VERIFIED [IMG]**.

**Gu 2006 Fig. 2**, p. 3 of the Columbia author PDF, text layer and rendered page: 26 samples; Rusting Steel 1: 22 frames, 7.3 min; Rusting Steel 2: 35, 10.8 min; Cast Iron Rusting: 30, 13.9 min; Copper with Patina: 34, 31.6 min. Caption: “average time interval between frames (in minutes m).” Body text: “we acquire images from 1280 light and view directions at each time step”; “the minimum time between two consecutive measurements is about 2 minutes.” Rows 5.20 and the “~2 min is the minimum” correction in §8 **VERIFIED [IMG]**. DOI 10.1145/1179352.1141952 confirmed (Crossref and the paper).

### 6. ISO 2768-1 Tables 1–3

Opened the official iTeh preview (`ISO-2768-1-1989.pdf`) and read **printed p. 2** as an image (PDF p. 6). Every D2 value matches Table 1 (linear ±mm, including the blank cells for v on 0.5–3 and f on 2000–4000), Table 2 (broken edges: 0.5–3 f/m 0.2, c/v 0.4; over 3–6 f/m 0.5, c/v 1; over 6 f/m 1, c/v 2), and Table 3 (angles by the shorter side, including 1°30′, 0°30′, 0°20′, 0°15′, 0°10′, 0°5′). The standard applies to metal-removal or sheet-metal parts and says the class should follow “customary workshop accuracy.” All eight Table 1 rows and the Table 2–3 prose **VERIFIED [IMG]**.

### 7. ISO 9223 Table 2, Duraiswamy, FAA, FHWA, and the named papers

**ISO 9223:2012 Table 2** (official iTeh preview, §7, printed p. 4): carbon-steel µm/a and g/(m²·a), zinc µm/a, copper µm/a, and aluminium in g/(m²·a) only (C1 “negligible” … CX rcorr > 10) all match D2. ISO writes strict lower bounds (e.g. C2 is 1,3 < rcorr ≤ 25 µm/a), so the boundary 1.3 µm/a is C1, not C2; D2’s en-dash ranges are a shorthand, not a wrong table. “The first-year corrosion rates cannot be simply extrapolated… ISO 9224” is verbatim in §7. Rows 5.13–5.16 **VERIFIED**.

**Duraiswamy et al. 2018**, DESIGN 2018, doi 10.21278/idc.2018.0432, official PDF, pp. 241–243: n = 47 (23 F, 24 M), company employees, CAD images, Mann–Whitney. Table 2 p. 241: Location 1 3.5±1 then 4.6±1.6 mm; Location 2 3.5±1 then 4.6±1 mm. Gaps shown 2.5, 3.0, 3.5, 4.6, 6.2 mm; “gap differences below 2 mm do not appear to significantly influence the appeal rating”; 2.5 vs 4.6 mm (2.1 mm) p = 0.057; “gap differences bigger than 3mm appear to be related to significant differences”; “appeal appears to diminish at gap values of 4.6 and larger.” 3.5 vs 4.6 mm noticed by 12.77% (first) and 31.91% (repeat); Hawthorne effect named in §4.2. Rows 5.1–5.3 **VERIFIED**.

**FAA AC 43.13-1B Change 1, par. 4-57c(1)** (NTSB excerpt and official full AC): single-row “edge distance should not be less than 2 times the diameter of the rivet and spacing should not be less than 3 times the diameter.” Row 5.7 **VERIFIED**. Row 5.8: official Fig. 4-5 (p. 4-20 of the full AC, PDF p. 161 of `AC_43.13-1B_w-chg1.pdf`) **opened and read**. Double row (A): edge 2D, inter-row 4D, pitch along the row 4D; “strength through this section is 75% of sheet without holes.” Triple or multiple rows (B): edge 2D, inter-row 3D, longitudinal labels 3D and 6D (staggered pattern); “83% of sheet without holes.” D = rivet diameter, “preferably about 3× thickness of thicker sheet.” **Upgraded to VERIFIED [IMG].** Official URL: `https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_43.13-1B_w-chg1.pdf`.

**FHWA-IF-12-052 Vol. 14**, official ROSA P PDF (plain curl). s_min = 3d = 3(0.875) = 2.63 in (pp. 22/44 region). s ≤ (4.0 + 4.0t) ≤ 7.0 in; worked example “6.5 in. = 4.0 + 4.0(0.625) = 6.5 in. < 7.0 in. ok.” Edge distance for 7/8 in bolts: minimum 1-1/2 in (Table 6.13.2.6.6-1); maximum 8.0t, not over 5.0 in; example 8.0(0.625) = 5.0 in (p. 45). Rows 5.9–5.11 **VERIFIED**.

**Deng & Kahn 2009** accepted MS: Study 1 N=139, heaviness index from three nine-point scales (−4…+4), α = .87; Figure 1 means top −0.72, bottom 0.81, left −0.30, right 0.65, top-left −1.12, bottom-right 0.93. Study 1a N=117; 77% right-eyed; right-eyed Mleft = −.58, Mright = .55; left-eyed Mleft = .29, Mright = .59, t(115) = 1.2, p > .1. Rows 2.9–2.10 **VERIFIED**.

**Zhou et al. 2017** author PDF: confirmatory n = 40 (80% M, median age 29, US MTurk), 7-point scales, within-subjects 4 conditions; exploratory n = 61. “Weight. … speed … significant negative effect (F(1, 163) = 19.97, p < .0001), with moving faster resulting in the object being perceived as lighter.” “Surprisingly, pausing did not affect weight.” Row 3.3 **VERIFIED**.

**Ujitoko et al. 2023** official PDF: 128 people were recruited (16 M + 16 F in each of the 20s–50s; mean age 40.43, SD 10.93 — these demographics are given for the 128). Accelerations 0, 2.45, 4.9, 9.8, 19.6, 39.2 m/s²; drop 0.54 m, camera 6.53 m, 50 mm on screen. Zero fall judged lighter; “no significant differences among conditions with the falling speeds greater than 0.” Methods then say only people at 60 fps and 4 pixels/mm were kept, “Consequently, the data from 78 participants were analyzed.” The result numbers therefore rest on **n=78**. Row 3.4 is a **FIX** for the analysed n. CC BY.

**Reitsma & Pollard 2003** CMU author PDF, Table 2: thresholds for sensitivity 0.25 or less; gravity interval −12.7 m/s² < geff < −9.0 m/s²; horizontal velocity change over 0.1 s [−0.13, +0.10] m/s; vertical [−0.32, +0.22] m/s. Study 1: 5 women and 7 men, 18–42 y (n=12; one additional excluded). Study 2: 9 women and 2 men, 19–29 y (n=11; three additional excluded). Initial velocities “approximately 2m/s in the vertical direction and 1.5m/s in the horizontal direction.” Mean ratings (SEM): low g 2.7(0.2) vs high g 4.7(0.2), F(1,196)=41.9; acceleration 2.6(0.2) vs deceleration 4.1(0.2), F(1,430)=38.2; lower rating = more often judged wrong. Rows 3.5–3.6 **VERIFIED**.

**Peters 2015** official PLOS PDF: 195 liftable man-made objects; pooled R = −.5721 (p << .001); per dataset R1 = −.4673, R2 = −.6290, R3 = −.7917; natural liftable R4 = −.0048 (p = .981); unliftable R5 = .0396 (p = .8416); WR = .613VR + .114, DR = −.387VR + .114; n = 10 per group. Rows 2.1–2.4 **VERIFIED**.

**Buckingham & Goodale 2013** official PLOS PDF: n = 28 (21 F, 7 M; mean age 22.5, SD 4.7); 5 cm and 10 cm cubes, all 700 g; SWI normalised heaviness small 0.81, large −0.81; size F(1,27)=1371.7, partial η²=.98; MWI polystyrene 0.1, metal −0.1; material F(1,27)=19.6, partial η²=.42; no size × material interaction F(1,27)=1.22, p=.28; aluminium look 2.7 g/cm³ vs EPS 0.1 g/cm³. Rows 2.6–2.7 **VERIFIED**.

**Hamrick et al. 2016** Princeton author PDF, §2.1 and §2.6: 101 recruited, 1 excluded as a repeater plus 19 failed the attention check → 81 analysed; accuracy 81.9% (95% CI 79.9–83.7%); individual accuracy 45–100%; 9 of 40 stimuli not above chance; mass ratio 1:10 or 10:1, compared in the text to steel vs oak. Row 3.9 **VERIFIED**.

## Row table

| Row | Claim (short) | Status | Note | URL |
|---|---|---|---|---|
| 2.1 | 195 objects, R=−0.5721; per-set −0.4673, −0.6290, −0.7917 | VERIFIED | Peters 2015 Results, official PLOS PDF | https://doi.org/10.1371/journal.pone.0119794 |
| 2.2 | Natural R=−0.0048 (p=.981); unliftable R=0.0396 (p=.8416) | VERIFIED | Same | same |
| 2.3 | log WR=0.613 log VR+0.114; log DR=−0.387 log VR+0.114 | VERIFIED | Written WR=.613VR+.114 after ln transforms, Fig. 5 | same |
| 2.4 | Expected-weight matches environment fit; n=10/group | VERIFIED | Methods + Figs 3–5 | same |
| 2.5 | Weight ~ volume^0.61; doubling volume does not double mass | VERIFIED | Fair restatement of the 0.613 exponent (plus a small intercept) | same |
| 2.6 | SWI: +0.81 / −0.81; F(1,27)=1371.7, η²=.98; n=28, 5/10 cm, 700 g | VERIFIED | Buckingham & Goodale 2013 Methods/Results | https://doi.org/10.1371/journal.pone.0054709 |
| 2.7 | MWI: +0.1 / −0.1; F(1,27)=19.6, η²=.42; p=.28; 2.7 vs 0.1 g/cm³ | VERIFIED | Same; −0.1 and −0.81 appear as “20.1” / “20.81” in the text layer | same |
| 2.8 | Metal look → more initial grip/lift force than polystyrene | VERIFIED | Official abstract + UTSC author PDF; no n in abstract | https://pubmed.ncbi.nlm.nih.gov/19793879/ |
| 2.9 | Bottom −0.72 vs 0.81; left −0.30 vs 0.65; TL −1.12 vs BR 0.93; α=.87; N=139 | VERIFIED | Deng & Kahn 2009 accepted MS, Fig. 1 / Study 1 | https://marketing.wharton.upenn.edu/wp-content/uploads/2016/10/the_location_effect_in_packaging_design.pdf |
| 2.10 | Right-eyed −0.58 vs 0.55; left-eyed 0.29 vs 0.59 (n.s.); 77%; N=117 | VERIFIED | Study 1a, MS pp. 13–14 | same |
| 2.11 | Heavier feels darker and lower-pitched (both ways) | VERIFIED | Official abstract (and Sunway PDF); no numbers in abstract | https://doi.org/10.1177/0301006616684369 |
| 2.12 | Heaviness falls with lightness, rises with chroma; hue little | VERIFIED | Official Springer abstract opened; still no numbers. Upgraded from search-index-only | https://doi.org/10.3758/BF03199388 |
| 2.13 | Stability pulled toward minimum critical angle | VERIFIED | Official JOV/Europe PMC abstract; full text blocked | https://doi.org/10.1167/13.4.12 |
| 3.1 | Slope 0.87, SD 3.8 kg (21 patches); live 1.00 / 3.1 and 1.20 / 2.0 | VERIFIED | MEDLINE abstract; no n | https://pubmed.ncbi.nlm.nih.gov/6457088/ |
| 3.2 | Lift duration is the main seen-weight cue; grasp predicts true weight | VERIFIED | Europe PMC abstract + author PDF | https://www.ebi.ac.uk/europepmc/webservices/rest/search?query=PMCID:PMC2637436&resultType=core&format=json |
| 3.3 | Faster arm motion → lighter; F(1,163)=19.97; n=40; pauses n.s. | VERIFIED | Zhou 2017 author PDF §4; exploratory n=61 | https://people.eecs.berkeley.edu/~anca/papers/HRI17_timing.pdf |
| 3.4 | Zero fall looks lighter; 0.25–4×g n.s.; n=128, 0.54 m, 6.53 m, 50 mm | FIX | Recruited 128 with those demographics; **78 analysed** (60 fps / 4 px/mm only). Speeds and scene geometry match. | https://www.frontiersin.org/journals/psychology/articles/10.3389/fpsyg.2023.1042188/full |
| 3.5 | g in [−12.7, −9.0]; horiz [−0.13, +0.10], vert [−0.32, +0.22] m/s; n=11/12 | VERIFIED | Reitsma & Pollard 2003 Table 2; ISO-style strict inequalities in eq. (3) | https://www.cs.cmu.edu/~psar/research/metrics/metricsS2003.pdf |
| 3.6 | Low g 2.7(0.2) vs high 4.7(0.2), F(1,196)=41.9; accel 2.6 vs decel 4.1, F=38.2 | VERIFIED | Same, §3 | same |
| 3.7 | Gravity as a formal size/distance cue; no data in abstract | VERIFIED | Official abstract | https://doi.org/10.1068/p210069 |
| 3.8 | Use free-fall better than chance; 3 experiments, 3 diameters | FIX | NTRS 1994 abstract has 3 experiments and the “better than chance…” wording. The 1996 journal abstract has **five** experiments and stresses velocity over gravitational acceleration. | https://ntrs.nasa.gov/citations/20020006041 and doi 10.3758/BF03206833 |
| 3.9 | 81.9% (79.9–83.7); 45–100%; 9/40 n.s.; 81 of 101; 1:10, steel vs oak | VERIFIED | Hamrick 2016 author PDF §§2.1, 2.6 | https://cocosci.princeton.edu/papers/Hamrick_Cognition.pdf |
| 3.10 | 95% vs 23% at ratio 1.5, e=0.5 (Todd & Warren 1982) | VERIFIED | Todd & Norman 2026 Fig. 1 caption; secondary | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC12875344/fullTextXML |
| 3.11 | Sigmoid r=0.95, mean error 1.4%, vs noisy Newton | VERIFIED | Same Results; noisy Newton was r=0.93 earlier, so 0.95/1.4% are the sigmoid | same |
| T1 0.5–3 | f 0.05, m 0.1, c 0.2, v — | VERIFIED | ISO 2768-1 Table 1, official preview p. 2 [IMG] | https://cdn.standards.iteh.ai/samples/7748/62ad250fa883403897dfadc6f29c583b/ISO-2768-1-1989.pdf |
| T1 3–6 | 0.05, 0.1, 0.3, 0.5 | VERIFIED | same | same |
| T1 6–30 | 0.1, 0.2, 0.5, 1 | VERIFIED | same | same |
| T1 30–120 | 0.15, 0.3, 0.8, 1.5 | VERIFIED | same | same |
| T1 120–400 | 0.2, 0.5, 1.2, 2.5 | VERIFIED | same | same |
| T1 400–1000 | 0.3, 0.8, 2, 4 | VERIFIED | same | same |
| T1 1000–2000 | 0.5, 1.2, 3, 6 | VERIFIED | same | same |
| T1 2000–4000 | —, 2, 4, 8 | VERIFIED | same | same |
| 4.1 | Fine vs very coarse on 6–30 mm: ±0.1 vs ±1, factor 10 | VERIFIED | 1/0.1=10 | same |
| 4.2 | ISO 286-1 IT6–IT12, 30–50 mm | UNVERIFIABLE | Official preview not opened past the lead-in, as D2 said | — |
| 4.3 | ISO 1302 N6–N12 Ra | UNVERIFIABLE | Not opened, as D2 said | — |
| 5.1 | Tailgate gaps 3.5±1 then 4.6±1.6 / 4.6±1 mm | VERIFIED | Duraiswamy Table 2, p. 241 | https://www.designsociety.org/download-publication/40444/development_of_a_methodology_for_robust_evaluation_of_perceived_quality_of_vehicle_body_panel_gaps |
| 5.2 | Appeal falls at 4.6 mm; <2 mm n.s.; 2.1 mm p=0.057; >3 mm sig.; n=47 | VERIFIED | §§4.1, pp. 242–243; “bigger than 3mm” | same |
| 5.3 | 3.5 vs 4.6 mm noticed 12.77% then 31.91% | VERIFIED | §4.2, p. 243 | same |
| 5.4 | Class gaps, incl. C/D bonnet +1/0 | VERIFIED | Pang Table 1 p. 41 [IMG]; headlamp B/C 2±1.5 and 2±0.8 omitted under an ellipsis | https://www.extrica.com/article/23830/pdf |
| 5.5 | No unified flush reference | VERIFIED | Verbatim, p. 41 | same |
| 5.6 | VW ID.3 3.5±0.5 mm | UNVERIFIABLE | Unofficial host, as D2 said | — |
| 5.7 | Rivets: 2D edge, 3D pitch | VERIFIED | AC 43.13-1B par. 4-57c(1) | https://data.ntsb.gov/Docket/Document/docBLOB?FileExtension=.PDF&FileName=Excerpt+from+Advisory+Circular+%28AC%29+43.13-1B+Change+1+dated+September+27%2C+2001.-Master.PDF&ID=40266557 |
| 5.8 | Multi-row minimums in Fig. 4-5 | VERIFIED | Upgraded. Official Fig. 4-5 p. 4-20: double row edge 2D, inter-row 4D, pitch 4D (75% net); triple/multiple edge 2D, inter-row 3D, 3D/6D longitudinal (83% net) | https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_43.13-1B_w-chg1.pdf |
| 5.9 | Bolts s_min=3d; 7/8 in → 2.63 in | VERIFIED | FHWA Vol. 14 | https://rosap.ntl.bts.gov/view/dot/49753/dot_49753_DS1.pdf |
| 5.10 | s ≤ 4.0+4.0t ≤ 7.0 in; t=0.625 → 6.5 in | VERIFIED | Same; “6.5 in. = 4.0 + 4.0(0.625)” is in the PDF | same |
| 5.11 | 7/8 in edge min 1-1/2 in; max 8.0t ≤ 5.0 in | VERIFIED | p. 45, Table 6.13.2.6.6-1 example | same |
| 5.12 | Pitch 3d to sealing max; edge ~1.5–2d | VERIFIED | 1.5/0.875≈1.7d and 2d rivets; fair restatement | S3, S4 |
| 5.13 | Steel µm/a C1≤1.3 … CX 200–700 | VERIFIED | ISO 9223 Table 2; en-dash is shorthand for exclusive lower bounds | https://cdn.standards.iteh.ai/samples/53499/e1f1aefb0a5446ac8308e3ddfce1db8b/ISO-9223-2012.pdf |
| 5.14 | Steel g/(m²·a) C1≤10 … CX 1500–5500 | VERIFIED | same | same |
| 5.15 | Zn, Cu µm/a; Al g only, C1 negligible … CX>10 | VERIFIED | same | same |
| 5.16 | Cannot simply extrapolate; ISO 9224 for long-term | VERIFIED | §7 verbatim | same |
| 5.17 | ISO 9224 C3 steel 50/72/116/167/206/240 µm | UNVERIFIABLE | Paid standard, not opened, as D2 said | — |
| 5.18 | Green patina 6–7 y urban/marine; none at 16 y rural; n=36 alloys | FIX | Values match official Getty **p. 53**, not p. 70 | https://media.getty.edu/Text/a33da677-13c2-5f35-8954-7c4e1eef454b.pdf |
| 5.19 | Penetration 0.3–0.5 / 0.5–0.9 / 0.9–1.3 µm/a; slows after ~10 y | FIX | Same page (p. 53) | same |
| 5.20 | 26 samples, 1,280 dirs; rust/patina frames and intervals | VERIFIED | Gu Fig. 2; 2 min is the stated system minimum | https://cave.cs.columbia.edu/Statics/publications/pdfs/Gu_TOG06.pdf |
| 5.21 | Copper patina ≈17.9 h (34 × 31.6 min) | FIX | Use **(34−1)×31.6 = 1,042.8 min ≈ 17.4 h** | same |
| 5.22 | AAMA 2605: 10 y Florida ΔE≤5, gloss ≥50% | UNVERIFIABLE | Paid spec, not opened, as D2 said | — |
| 6.1 | Saccone 2019 meta-analysis d values | UNVERIFIABLE | Not opened, as D2 said | — |
| 6.2 | Ellis & Lederman 1993 | UNVERIFIABLE | Not opened | — |
| 6.3 | Flanagan & Beltzner 2000 | UNVERIFIABLE | Not opened | — |
| 6.4 | Twardy & Bingham; McConnell et al. | UNVERIFIABLE | Not opened | — |
| 6.5 | O'Sullivan 2003; Reitsma & O'Sullivan 2009 | UNVERIFIABLE | Not opened | — |
| 6.6 | Vicovaro & Burigana 2016 restitutions | UNVERIFIABLE | Not opened | — |
| 6.7 | Dorsey & Hanrahan 1996 ~30 y | UNVERIFIABLE | Not opened | — |
| 6.8 | Bellini et al. 2016 | UNVERIFIABLE | Not opened | — |
| 6.9 | Horizon Zero Dawn GDC / blog quotes | UNVERIFIABLE | Videos not watched; GDC Vault blurb URL resolves (200) | https://www.gdcvault.com/play/1025040/Animation-Bootcamp-Bringing-Life-to |
| 6.10 | Scott Robertson *How to Render* | UNVERIFIABLE | Copyrighted book, not opened, as D2 said | — |

## Source table

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| D2-S1 | https://cdn.standards.iteh.ai/samples/7748/62ad250fa883403897dfadc6f29c583b/ISO-2768-1-1989.pdf | Yes (200, PDF, official preview) | Yes — © ISO, facts only |
| D2-S2 | https://cdn.standards.iteh.ai/samples/53499/e1f1aefb0a5446ac8308e3ddfce1db8b/ISO-9223-2012.pdf | Yes (200, PDF, official preview) | Yes — © ISO, facts only |
| D2-S3 | NTSB docket excerpt of AC 43.13-1B Chg 1 | Yes (200, PDF). Full official AC also 200 at faa.gov | Yes — US Government work |
| D2-S4 | https://rosap.ntl.bts.gov/view/dot/49753/dot_49753_DS1.pdf | Yes with default curl (200, 2,934,767 B); 403 with a browser UA | Yes — US Gov work; AASHTO quotes facts only |
| D2-S5 | Design Society download (Duraiswamy 2018) | Yes (200, PDF) | Yes — no open licence printed, facts only |
| D2-S6 | https://www.extrica.com/article/23830/pdf | Yes (200, PDF) | Yes — CC BY printed on p. 1 |
| D2-S7 | https://doi.org/10.1371/journal.pone.0119794 | Yes (200, PLOS HTML + printable PDF) | Yes — CC BY 4.0 |
| D2-S8 | https://doi.org/10.1371/journal.pone.0054709 | Yes (200, PLOS HTML + PDF) | Yes — CC BY (Attribution License; PLOS family) |
| D2-S9 | https://pubmed.ncbi.nlm.nih.gov/19793879/ | PubMed 203 with content; MEDLINE/Crossref 200; author PDF 200 | Yes — © APS, facts only |
| D2-S10 | Wharton accepted MS PDF | Yes (200, PDF) | Yes — © AMA, facts only |
| D2-S11 | https://doi.org/10.1177/0301006616684369 | SAGE landing 403 to this fetch; Crossref + Europe PMC + Sunway PDF ok | Yes — © SAGE, facts only |
| D2-S12 | https://doi.org/10.3758/BF03199388 | Springer PDF 500; official HTML abstract 200; Crossref 200 | Yes — © Springer, facts only |
| D2-S13 | https://doi.org/10.1167/13.4.12 | JOV 403/500 to this fetch; Crossref + Europe PMC abstract 200; PMC3736874 | Yes — ARVO; abstract opened, full text blocked |
| D2-S14 | https://pubmed.ncbi.nlm.nih.gov/6457088/ | PubMed 203; MEDLINE 200 | Yes — © APA, facts only |
| D2-S15 | Europe PMC search PMC2637436 | Search API 200; fullTextXML 500; author PDFs 200 | Yes — © Springer, facts only |
| D2-S16 | https://people.eecs.berkeley.edu/~anca/papers/HRI17_timing.pdf | Yes (200, PDF) | Yes — ACM-licensed, facts only |
| D2-S17 | https://www.frontiersin.org/journals/psychology/articles/10.3389/fpsyg.2023.1042188/full | Yes (200, HTML + PDF) | Yes — CC BY |
| D2-S18 | https://www.cs.cmu.edu/~psar/research/metrics/metricsS2003.pdf | Yes (200, PDF) | Yes — © ACM, facts only |
| D2-S19 | https://ntrs.nasa.gov/citations/20020006041 | Yes (200; abstract-only NASA record) | NTRS abstract is NASA-hosted; the 1996 journal article is © Springer. See row 3.8 FIX |
| D2-S20 | https://doi.org/10.1068/p210069 | SAGE 403; Crossref + Europe PMC abstract 200 | Yes — © SAGE, facts only |
| D2-S21 | https://cocosci.princeton.edu/papers/Hamrick_Cognition.pdf | Yes (200, PDF) | Yes — © Elsevier, facts only |
| D2-S22 | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC12875344/fullTextXML | Yes (200, XML) | Yes — **CC BY-NC-ND 4.0**, not CC BY |
| D2-S23 | Columbia author PDF + STAF about page | Both 200 | Yes — © ACM, facts only; STAF terms **not found** |
| D2-S24 | Getty item 2420W0; flipbook page70.html | Both 200. Official free PDF also 200 at https://media.getty.edu/Text/a33da677-13c2-5f35-8954-7c4e1eef454b.pdf | Yes — © J. Paul Getty Trust, facts only. Cite **p. 53**, not the flipbook’s “page 70” |
| — | GDC Vault 1025040 (row 6.9 blurb) | Yes (200, HTML) | Practitioner blurb only |

All §11 entries have a URL and the stated access date 2026-11-17. The source list at the bottom is complete for D2-S1–S24.

## Grok section

## Grok section

One live Grok CLI pass was run as required: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve --prompt-file /workspace/lv-scratch/d2/grok/prompt.txt`. Started **11:29:22 AEDT**, exited **0 at 11:46:42 AEDT** on 17 November 2026 (about 17 minutes; under the 25-minute cap). Live web search was on (no `--disable-web-search`). Prompt, stdout and stderr are in `/workspace/lv-scratch/d2/grok/`. stderr only recorded two “Auto-compacting conversation” notices. No number in this verify file was taken from Grok alone; each used Grok lead was opened in its source first.

Grok independently matched the three source problems already found in this pass (Scott printed p. 53 not p. 70; Hecht NTRS 3 experiments vs the 1996 journal’s five; Gu span (n−1)×interval ≈ 17.4 h). It also pointed at Ujitoko’s analysed n=78, which I then confirmed in the official Frontiers PDF and recorded as a FIX.

Other Grok notes that I opened and treat as clarifications, not extra number fixes: Buckingham & Goodale 2013 prints unversioned CC BY (not “4.0” on the article); ISO 9223 class edges are exclusive at the lower bound; Todd & Norman’s r=0.95 / 1.4% is the sigmoid fit to the Sanborn et al. 2013 reanalysis (the paper then says similarly good fits were obtained for Todd & Warren 1982); Walker 2017’s new data are felt heaviness → darker/lower pitch, with bidirectionality as the theoretical frame; Watson’s abstract mentions free fall, pendulum and wave motion, not bouncing; Gu has a second TOG DOI `10.1145/1141911.1141952` besides the claimed SIGGRAPH-papers DOI, which is the one D2 used and which Crossref resolves.

Grok also offered ISO 286-1:2010 Table 1 values from an iTeh catalogue HTML page, not from the official preview PDF (which D2 correctly said stops before Table 1). I did not open that catalogue page in this pass, so row 4.2 stays **UNVERIFIABLE**. ISO 1302 N-grades were not found on an official ISO PDF (Grok NOT FOUND); row 4.3 stays **UNVERIFIABLE**.
