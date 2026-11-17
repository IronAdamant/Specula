# B7-skin-eye-hair-colour.md — verification (Library Verify)

Checked 2026-11-17, 08:50–09:35 AEDT (Australia/Sydney). Every URL below was opened on 2026-11-17.
File checked: `B7-skin-eye-hair-colour.md`, mtime 2026-11-16 11:01:21 AEDT, 18,750 bytes (recorded at start; re-stat before writing this note: unchanged, byte-identical to the start-of-run snapshot).

## Summary

- Rows checked: 46. **VERIFIED 42 · FIX 3 · UNVERIFIABLE 1.**
- Worst problems:
  1. **ITA category names are not in the cited source.** Alipour 2026 Table 3 gives only Fitzpatrick types I–VI with these bands: ITA > 55°; 41° < ITA ≤ 55°; 28° < ITA ≤ 41°; 10° < ITA ≤ 28°; −30° < ITA ≤ 10°; ITA < −30°. The names "very light / light / intermediate / tan / brown / dark" do not appear in that table. Either drop the names, or source them from Chardon 1991 or Del Bino 2006, which nobody has opened yet. The numeric bands are right.
  2. **ISSA paper misattributed.** "ISSA (Xiao et al.) 2025" should be **Lu Y, Xiao K, Pointer M, et al. 2025**, *Sci Data* 12:487. Yan Lu is the first author; Kaida Xiao is the corresponding author.
  3. **Wrong Loussouarn URL.** The Loussouarn 2016 URL in §4 (`…_306835/article.phtml`) redirects to an unrelated JLE article (an oncology network paper). The working full-text URL is the `…_306744/article.phtml?tab=texte` form (https). Table 3 tone counts are VERIFIED there.
- Arithmetic: I recomputed all of it with D65, the sRGB matrix and gamma. Every value matches: the MST Lab from hex (to ±0.01), MST ITA, Lu 2026 ITA, Itou mean/SD from Table S1, the Itou sRGB #2a2522, and all five lip sRGB values. One caveat: Charton and Vergnaud are D65/**10°** data, pushed through the 2°-based sRGB transform, so treat those swatches as approximate. The researcher already says "D65, 2°".
- Dead links: the Loussouarn `…306835` URL (wrong target). The MST-E bucket listing is 403 (so its licence is UNVERIFIABLE).

## Row table

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| §1 MST hex ×10 | #f6ede4 … #292420 | VERIFIED | All 10 match the official bundle exactly | https://skintone.google/assets/tono-color-block-arranger.73c427e1.js |
| §1 MST Lab ×10 | 94.211,1.503,5.422 … 14.61,1.482,3.525 | VERIFIED | All 10 match the bundle. Recomputed from hex (D65 sRGB): 94.211/1.504/5.430 … 14.610/1.483/3.528, which agrees to ≤ 0.01 | same |
| §1 MST ITA ×10 | 83.0, 80.2, 71.8, 64.7, 50.3, 10.9, −20.1, −55.4, −78.4, −84.3 | VERIFIED | Recomputed from the site's Lab | same |
| §1 MST licence | CC BY 4.0 | VERIFIED | Get-started page: "License: Creative Commons Attribution 4.0 International license" | https://skintone.google/assets/tono-get-started.62a4df4e.js |
| §1 Lab is derived from the hex, likely D65 | inference | VERIFIED | My recomputation reproduces the site's Lab for all 10 tones | — |
| §1 MST-E CC BY 4.0 | licence | UNVERIFIABLE | arXiv 2305.09073 resolves; the bucket listing is 403 and the download page was not found | https://arxiv.org/abs/2305.09073 ; https://storage.googleapis.com/mste/ (403) |
| §2 ITA formula | arctan((L*−50)/b*)·180/π | VERIFIED | Alipour eq. 5 | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC13459198/fullTextXML |
| §2 ITA bands (numbers) | > 55; 41–55; 28–41; 10–28; −30–10; < −30 | VERIFIED | Alipour Table 3, ref 76 = Chardon, Cretois, Hourseau 1991, *Int J Cosmet Sci* 13(4):191–208 | same |
| §2 ITA category names | very light … dark | FIX | Not in Alipour Table 3, which gives only FST I–VI. Drop the names or cite a source that has them (Chardon 1991 / Del Bino 2006, unopened) | same |
| §2 "cut-offs came from lighter skin" | statement | VERIFIED | "The thresholds were established using measurements from individuals with lighter skin types" | same |
| §2 Alipour licence | CC BY | VERIFIED | CC BY 4.0 | same |
| §2 ISSA dataset description | 15,256 spectra / 2,113 subjects / 11 datasets; UK 172, China 201, Iraq 149, Thai 710, Pakistani 117, Spain 27 | VERIFIED | Abstract; Table 1 | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC11930939/fullTextXML |
| §2 ISSA figshare DOI | 10.6084/m9.figshare.28228571.v4 | VERIFIED | Data citation (ref 37) | same |
| §2 ISSA authorship | "Xiao et al." | FIX | First author **Lu Y** (Lu, Xiao, Pointer, He, Zhou, Nasseraldin, …), *Sci Data* 12:487, CC BY 4.0 | same |
| §2 Xiao 2017 n and instruments | 960; CA 187 (102/85), CN 202 (65/137), KU 145 (74/72), TH 426 (283/143); CM-2600d 3 mm, SP62 4 mm | VERIFIED | Table 1; Methods | https://eprints.whiterose.ac.uk/id/eprint/100718/7/Xiao_et_al-2017-Skin_Research_and_Technology.pdf |
| §2 Xiao 2017 Table 2 (20 rows) | all L*, a*, b* (SD) | VERIFIED | Every cell matches Table 2 | same |
| §2 Xiao 2017 Table 3 | 6.2 ± 3.5; 4.3 ± 2.3; 5.5 ± 3.0; 5.0 ± 2.6 | VERIFIED | Table 3 "All" column; the caption's "three ethnic groups" error is confirmed | same |
| §2 Xiao 2017 licence | CC BY | VERIFIED | "Creative Commons Attribution License" on p. 1 | same |
| §2 Lu 2026 Table 1 | subjects F/M and spectra, 8 groups | VERIFIED | Table 1; spectra sum = 14,532 | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC13097458/fullTextXML |
| §2 Lu 2026 Table 2 (8 rows × 5) | L*, a*, b*, C*, ΔE (SD) | VERIFIED | Every cell matches Table 2 | same |
| §2 Lu 2026 overlap | 89.4% (81.5–91.9); 60.5% (54.5–63.6); N = 14,532; ages 18–75 | VERIFIED | Abstract; Results; Fig. 1 caption. JND used = ΔE*ab 2 | same |
| §2 Lu 2026 licence | CC BY 4.0 | VERIFIED | XML licence | same |
| §2 ITA from Lu means | 37.3, 30.4, 38.5, 8.6, −35.8, 24.8, 18.6, 29.7 | VERIFIED | Recomputed | — |
| §3 Liu 2010 hue/sat ×5 cohorts | 34.86 ± 7.62 / 0.40 ± 0.13 … 27.99 ± 6.38 / 0.54 ± 0.24; n 2429/1535/1987/2261/1282; RS1 age 74.02 ± 8.23 | VERIFIED | Table 1 | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC2865509/fullTextXML |
| §3 Liu categorical shares | Blue 68.01/69.51/71.26/53.51 etc. | VERIFIED | Table 1 (cohort assignment of the 4 columns not labelled in XML; ambiguity confirmed) | same |
| §3 Liu five-grade split | 4.12/3.52/2.62 … 0.95/1.63/1.71 | VERIFIED | Table 1 | same |
| §3 Liu H–S r = −0.77 | — | VERIFIED | Results | same |
| §3 Liu TwinsUK spread explanation | low-res portraits, varying daylight | VERIFIED | "taken under some variation in daylight and exposure conditions" | same |
| §3 Liu licence | CC BY | VERIFIED | XML | same |
| §3 Edwards 2016 | exists; CC BY; abstract only | VERIFIED | Europe PMC: *PCMR* 29:141–162, licence field "cc by" | https://doi.org/10.1111/pcmr.12435 (Europe PMC record PMID 26547379) |
| §4 UK Biobank shares (18 values) | Red 3.8/5.2/4.6 … Other 2.0/0.9/1.4; n 343,884 | VERIFIED | Morgan 2018 Table 1 | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC6288091/fullTextXML |
| §4 Itou tress CIELAB | n = 42, L* 15.19 ± 1.22 (12.3–18.0); a* 1.98 ± 0.76 (0.5–4.0); b* 2.72 ± 0.95 (0.9–5.5); p = 0.0035 | VERIFIED | Recomputed from Suppl. Table S1 (42 rows with colour; MT-1 and TI blank); p in Fig. 1 legend | Europe PMC supplementary zip for PMC9693441 → ijms-2027964-supplementary.pdf |
| §4 Itou licence | CC BY | VERIFIED | CC BY 4.0 | PMC9693441 XML |
| §4 Itou sRGB | #2a2522 | VERIFIED | Recomputed | — |
| §4 Loussouarn tones | 231, 489, 456, 245, 178, 166, 86, 55, 14, 2; n 1,922; 74% tones 1–4 | VERIFIED | Table 3 and text | https `…306744/article.phtml?tab=texte` |
| §4 Loussouarn URL | `…_306835/article.phtml` | FIX (link) | Redirects to an unrelated article. Use `https://www.jle.com/fr/revues/ejd/e-docs/diversity_in_human_hair_growth_diameter_colour_and_shape._an_in_vivo_study_on_young_adults_from_24_different_ethnic_groups_observed_in_the_five_continents_306744/article.phtml?tab=texte` | both |
| §5 Charton group means ×4 | 33.14/16.16/36.75; 45.96/26.56/31.93; 42.48/25.06/35.52; 42.36/24.99/35.46 | VERIFIED | Results | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC12877972/fullTextXML |
| §5 Charton n | 514; AA 103; CA 207 (100 FR + 107 US); HI 100; CN 104; age 19–68, 42.5 ± 12.8 | VERIFIED | Methods | same |
| §5 Charton ranges | L* 23–56, h 20–52; 53 tones; Hispanic 32 | VERIFIED | Results; abstract | same |
| §5 Charton SDs (12 values) | L* 4.99/4.03/3.31/3.16; C* 6.06/3.47/3.06/3.04; h 3.28/4.79/4.18/4.42 | VERIFIED | Results text | same |
| §5 Charton D65 / 10° / lower lip | conditions | VERIFIED | Methods | same |
| §5 Charton tone coverage and Table 1 overlaps | 23/25/27/32; 10, 4, 15, 15, 21, 17 | VERIFIED | Results; Table 1 | same |
| §5 Charton licence | CC BY-NC-ND 4.0 | VERIFIED | XML `ali:license_ref` = by-nc-nd/4.0; Grok also found the same in Crossref | same |
| §5 Vergnaud 2024 | n 410; L* 41.9 ± 6.6 (24.6–54.4); C* 23.6 ± 6.0 (4.9–35.1); h 34.0 ± 4.7 (20.8–48.2) | VERIFIED | Results | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC10823443/fullTextXML |
| §5 Vergnaud licence | CC BY-NC-ND | VERIFIED | by-nc-nd/4.0 | same |
| §5 Lip sRGB ×5 | #66463f, #975e56, #8b574d, #8b574d, #88564e; AA a* 12.9 b* 9.7; CA a* 22.5 b* 14.0 | VERIFIED | Recomputed. These are 10°-observer data put through the 2° sRGB path, so they are approximate | — |
| Count | | | **Rows 46: VERIFIED 42 · FIX 3 · UNVERIFIABLE 1** | |

## Source table

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| MST (Google) | https://skintone.google ; bundle JS files above | y | y (CC BY 4.0) |
| MST-E | https://arxiv.org/abs/2305.09073 | y (licence page not reached) | unverified |
| Alipour 2026 | PMC13459198 | y | y (CC BY 4.0) |
| Xiao 2017 (Wiley DOI) | https://doi.org/10.1111/srt.12295 | not re-opened (open PDF used) | y |
| Xiao 2017 open PDF | White Rose eprints 100718 | y | y (CC BY) |
| ISSA 2025 | PMC11930939 | y | y (CC BY 4.0). Author attribution wrong (FIX) |
| Liu 2010 | PMC2865509 / doi 10.1371/journal.pgen.1000934 | y | y |
| Edwards 2016 | doi 10.1111/pcmr.12435 | y (Europe PMC record) | y (cc by per Europe PMC) |
| Morgan 2018 | PMC6288091 | y | y |
| Itou 2022 | PMC9693441; MDPI page 403 to curl; supplement via Europe PMC | y | y |
| Loussouarn 2016 | `…306835` URL | **n (wrong article)** | y (© JLE) |
| Charton 2025 | PMC12877972 | y | y (CC BY-NC-ND 4.0) |
| Vergnaud 2024 | PMC10823443 | y | y (CC BY-NC-ND 4.0) |
| Lu 2026 | PMC13097458 | y | y (CC BY 4.0) |

## Sources disagree (new, from this check)

- **Morgan 2018 sample size:** the Methods text says "This left 343,234 participants", while the Table 1 total is 343,884. B7 uses the table, which matches its shares.
- **Xiao 2017, Caucasian mean a\*:** the text says 9.1, Table 2 says 9.0 (3.6). B7 uses the table.

## Grok section

- B7 was covered by the single B6–B8 Grok pass (prompt `/tmp/lv17/grok/B.txt`). It ran from 08:49:03 to 08:56:14 AEDT and exited with code 0.
- What Grok reported, and how it compared:
  - **MST:** Grok could not load skintone.google, so it gave Wikidata's hex list, which matches the official bundle.
  - **Charton:** licence CC BY-NC-ND 4.0 (from Crossref), and the four group means. Both match what I read.
  - **Lu 2026:** 14,532 spectra and the CA row. Matches.
  - **Itou:** n = 42, but Grok could not get the supplement. I opened Table S1 myself.
  - **Morgan:** Table 1 matches.
- No number in this note was taken from Grok alone.
