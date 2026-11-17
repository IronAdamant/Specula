# B6-hair.md — verification (Library Verify)

Checked 2026-11-17, 08:48–09:30 AEDT (Australia/Sydney). Every URL below was opened on 2026-11-17 unless the row says otherwise.
File checked: `B6-hair.md`, mtime 2026-11-16 10:59:32 AEDT, 29,545 bytes (recorded at start; re-stat before writing this note: unchanged, byte-identical to the start-of-run snapshot).

## Summary

- Rows checked: 67. **VERIFIED 58 · FIX 3 · UNVERIFIABLE 6.**
- Worst problems:
  1. **Luo 2023 is not a real disagreement.** Grok's per-cohort counts (NSPT 1,497 CW / 652 CCW; TZL 1,394 CW / 556 CCW) sum exactly to the GWAS Catalog totals (1,208 cases CCW / 2,891 controls CW = 4,099). So pooled CCW = **29.5%** (researcher's figure, VERIFIED), and per cohort it is 30.3% (NSPT) and 28.5% (TZL). All three numbers are consistent. The per-cohort split itself stays UNVERIFIABLE because I could not open the paper's Supplementary Table S1 (JID and the Edinburgh repository PDF both return 403 to curl).
  2. The Loussouarn 2016 `http://` URL returns HTTP 500 to curl today. The same path over `https://` loads in full (WebFetch), and every Table 1–5 value checked out against it.
  3. Kashiyama temporal-type and widow's-peak rows cite "Results / Abstract"; the values are also in Tables 2 and 3 with case counts (location fix only).
- Dead links: `http://www.jle.com/…306744/article.phtml?tab=texte` (500 to curl; use `https://`). O'Reilly GPU Pro 5 preview: 403 to curl. JID full text: 403. Slideshare: JS challenge to curl, loads through WebFetch.
- Licence check: one more disagreement found for Luo 2023 (see Sources disagree). All other licences as stated.

## Row table

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| T1 Frontal linear | W 36.1 (121), M 45.9 (56) | VERIFIED | Table 1 | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC8367035/fullTextXML |
| T1 Frontal triangular | 7.2 (24) / 0.82 (1) | VERIFIED | Table 1 | same |
| T1 Frontal round | 38.5 (129) / 10.7 (13) | VERIFIED | Table 1 | same |
| T1 Frontal M-shaped | 18.2 (61) / 42.6 (52) | VERIFIED | Table 1 | same |
| T1 Temporal inverted triangle / inverted round / straight / convex | 20.3/65.6, 27.8/17.2, 24.8/10.6, 27.2/6.6 | FIX (location only) | Values right. They are in **Table 2** with case counts W 68/93/83/91, M 80/21/13/8, as well as the abstract | same |
| T1 Widow's peak | 29.6 / 32.8 | VERIFIED | Also Table 3: W 99 present / 236 absent (= 335 women, the same 335-vs-334 mismatch already flagged), M 40/82 | same |
| T1 Mid-frontal line | 6.2 (2–8.5) / 6.65 (2.7–9.5) cm | VERIFIED | Results; legend calls it panel B, text calls it Fig. 2A (already flagged) | same |
| T1 Intertemporal line | 14.9 (9–23.5) / 15.8 (12–23) | VERIFIED | Results | same |
| T1 Head circumference | 55 (42–66) / 57 (42–64) | VERIFIED | Results | same |
| T1 Nusbaum temporal (secondary) | 61/26/3/9 | VERIFIED | Discussion, ref 11 | same |
| T1 Mid-frontal Caucasians (secondary) | 5.54 (3.5–8.0); 5.89 (4.5–9) | VERIFIED | Discussion, refs 11 and 19 | same |
| T1 Licence | CC BY-NC-ND 4.0 | VERIFIED | XML licence block | same |
| T2 Luo CCW share | 1,208/4,099 = 29.5% | VERIFIED | Open Targets GraphQL: nSamples 4099, nCases 1208, nControls 2891. GWAS Catalog v2: "1,208 Han Chinese ancestry cases, 2,891 … controls" | https://api.platform.opentargets.org/api/v4/graphql (study GCST90671959); https://www.ebi.ac.uk/gwas/rest/api/v2/studies?pubmed_id=37565938 |
| T2 Luo cohort split | 2,149 + 1,950 | UNVERIFIABLE | Consistent with Grok's S1 counts (1,497+652 = 2,149; 1,394+556 = 1,950), but S1 not opened by me | https://www.jidonline.org/article/S0022-202X(23)01995-4/fulltext (403) |
| T2 Luo: no sex difference | statement | UNVERIFIABLE | Main text not opened (403) | same |
| T2 Whorl set at 10–16 weeks | Luo intro citing Smith & Gong 1973 | UNVERIFIABLE | Main text not opened (403) | same |
| T2 Klar 2003 CCW | 8.4% | VERIFIED | Abstract: "counterclockwise whorl rotation infrequently in 8.4% of individuals" | https://www.ncbi.nlm.nih.gov/pmc/articles/PMC1462764/ ; PubMed 14504234 |
| T2 Klar 2003 non-right-handers random | statement | VERIFIED | Abstract | same |
| T2 Ziering 2003 | 5 patterns; 534 patients; White men distinct; women/African Americans diffuse | VERIFIED | Abstract | https://pubmed.ncbi.nlm.nih.gov/12859380/ |
| T2 Kim 2025 angles | R 55.03° (49.05–60.95), L 32.83° (26.72–38.88), skew 22.2°, 11/125 = 8.8% | VERIFIED | Results; CC BY-NC-ND 4.0 confirmed | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC12502931/fullTextXML |
| T3 8 types, I–VIII | count | VERIFIED | Both abstracts ("eight well-defined categories"; "eight main groups") | https://pubmed.ncbi.nlm.nih.gov/18078200/ ; https://pubmed.ncbi.nlm.nih.gov/17919196/ |
| T3 Descriptors CD, i, w (+t) | — | VERIFIED | Both abstracts | same |
| T3 Samples 1,442/18 and 2,449/22 | — | VERIFIED | Abstracts | same |
| T3 Sample prep (6 cm, washed, on glass) | — | UNVERIFIABLE | Academia.edu copy not opened (researcher also marked it search-index only) | — |
| T3 Type distribution n = 980 | 86/349/288/154/46/23/26/8 | VERIFIED | Loussouarn 2016 Table 4 | https://www.jle.com/fr/revues/ejd/e-docs/diversity_in_human_hair_growth_diameter_colour_and_shape._an_in_vivo_study_on_young_adults_from_24_different_ethnic_groups_observed_in_the_five_continents_306744/article.phtml?tab=texte |
| T3 Brazilian curliest vs others | 194 ± 66 vs 231 ± 72; 339 ± 54 vs 382 ± 50 | VERIFIED | Results text before Table 4 | same |
| T4 Population | 2,249; 1,065 M / 1,184 W; 18–35; diameter 823 | VERIFIED | Methods; Table 1 totals | same |
| T4a Cluster 1/2/3 means | 178/12/325/71; 182/12/413/84; 214/10/386/73 | VERIFIED | Table 5 (means only, despite the caption) | same |
| T4b African-American | 176 ± 54; 312 ± 50; 71 ± 9 (49–91) | VERIFIED | Table 2 | same |
| T4b Western-African | 165 ± 34; 289 ± 41; 78 ± 8 (69–92) | VERIFIED | Table 2 | same |
| T4b South-African | 153 ± 30; 272 ± 37; 68 ± 9 (53–94) | VERIFIED | Table 2 | same |
| T4b Chinese | 172 ± 51; 418 ± 43; 89 ± 7 (68–102) | VERIFIED | Table 2 | same |
| T4b Japanese | 167 ± 55; 427 ± 38; 86 ± 10 (68–107) | VERIFIED | Table 2 | same |
| T4b Korean | 164 ± 52; 426 ± 39; 88 ± 6 (74–104) | VERIFIED | Table 2 | same |
| T4b Thai | 183 ± 61; 406 ± 39; 87 ± 8 (64–102) | VERIFIED | Table 2 | same |
| T4b Indian | 202 ± 57; 409 ± 43; 79 ± 7 (61–101) | VERIFIED | Table 2 | same |
| T4b French | 233 ± 74; 368 ± 46; 69 ± 8 (54–86) | VERIFIED | Table 2 | same |
| T4b Danish | 214 ± 65; 397 ± 49; 74 ± 8 (55–96) | VERIFIED | Table 2 | same |
| T4b Scottish | 211 ± 69; 397 ± 39; 72 ± 11 (55–95) | VERIFIED | Table 2 | same |
| T4b Caucasian-American | 206 ± 59; 385 ± 43; no diameter | VERIFIED | Table 2. Blank diameter cells match Table 1 "-" entries (CA-Am, CA-Au, Kanak, Latino, Lebanese, Mexican, Peruvian, Russian), so column alignment is right | same |
| T4 Other facts | vertex ≈ 2× temple; men −19/cm² vertex only; Asian ≈ 5 cm/yr longer than African; tone > 6: 72 ± 9 vs 81 ± 10 µm | VERIFIED | Results / Discussion | same |
| T4 Link | `http://…?tab=texte` | FIX (link) | http form gave HTTP 500 to curl today; use the `https://` form above, which loaded in full | same |
| T5 Grunwald beard counts | T-zone 55.0 (14.5–80.5); R 28.75 (13.38–40.0); L 33.00 (12.00–41.13) | VERIFIED | Table 1 | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC11830429/fullTextXML |
| T5 Grunwald vellus | 235.83 / 247.41 / 228.66 | VERIFIED | Table 1 | same |
| T5 Grunwald men age | 31.66 ± 2.72 | VERIFIED | Methods | same |
| T5 Image area 8.01 × 6.40 mm; ≈ 107/cm² | arithmetic | VERIFIED | 8.01 × 6.40 = 51.26 mm²; 55 / 0.5126 cm² = 107.3 | same |
| T5 Rajput brow direction | qualitative | VERIFIED | "pointing upward… tilt outward and downward… right angles… crisscross" | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC8719974/fullTextXML |
| T5 Rajput graft counts | 70–120; 150–200 | VERIFIED | Abstract; text. Text also gives 70–90 for slimmer brows | same |
| T6 Frostbite long hair | 10,000 strands, 240,000 pts, 4.7 / 4.5 ms, PS4 900p no MSAA | VERIFIED | Slide 34 | https://advances.realtimerendering.com/s2019/hair_presentation_final.pdf |
| T6 Frostbite short hair | 15,000 / 75,000 / 0.4 / 1.9 ms | VERIFIED | Slide 34 | same |
| T6 Frostbite optimisations | ~2× dedup; 5× with 1/10 strands | VERIFIED | Slide 32; slide 34 notes (continued on PDF p. 35) | same |
| T6 TressFX normal distance | 7,014 → ~21,042 splines × 16 verts; 4.8070 ms total (0.9439/0.7963/1.9690/1.0978); 44 draw calls; HD 7970 | VERIFIED | Slides 13 and 55 | https://www.slideshare.net/slideshow/hair-intombraider-final/20208628 |
| T6 TressFX close-up | 7.5559 ms; shading 4.5677; 36 draw calls | VERIFIED | Slide 56 ("1.752.3" garble confirmed) | same |
| T6 TressFX OIT memory | 12 bytes × 8; ~200 MB at 1080p | VERIFIED | Per-pixel linked list slide | same |
| T6 GPU Pro 5 "< 1 ms", ~19,000 strands | secondary | UNVERIFIABLE | O'Reilly preview 403 to curl. Grok names the chapter author as Dongsoo Han; not confirmed | https://www.oreilly.com/library/view/gpu-pro-5/9781482208641/chapter-99.html (403) |
| T6 Lengyel 2001 | 16 layers 128²; 512×64 fin; ½ layers 20–31 fps; all 10–15 fps; 1.6 MB, DXT5 ÷3; K7 700 MHz, GeForce DDR 32 MB, DX7 | VERIFIED | Table 1 and §4 text | https://hhoppe.com/fur.pdf |
| T6 UE 4.27 groom: no ms figures | absence | VERIFIED | Page loads (200); features only | https://dev.epicgames.com/documentation/en-us/unreal-engine/hair-rendering?application_version=4.27 |
| T6 TressFX MIT, v4.1 | licence | VERIFIED | Repo page: "MIT license"; "New in TressFX 4.1" | https://github.com/GPUOpen-Effects/TressFX |
| T7 Kashiyama lateral line | 4.2 (2–7) / 4.4 (2–8); P = 0.33 | VERIFIED | Fig. 2 legend / Results | PMC8367035 XML |
| T7 Nusbaum 2009 | 81%; 5.5 cm; 98%; 3.74/3.97; 3.78/3.51; 87%; n = 360 | VERIFIED | Abstract | https://pubmed.ncbi.nlm.nih.gov/19397668/ |
| T7 Ceballos 2013 | 94.17%; 1.01 / 2.13 cm; age 29.7; n = 103 | VERIFIED | Abstract | https://pubmed.ncbi.nlm.nih.gov/23582301/ |
| T7 Koo 2000 | 70.37%; 6.53; 5.9/5.95; 8.49 cm; types M, C, O, U, MO, CO; n = 108 / 1,731 | VERIFIED | Abstract | https://pubmed.ncbi.nlm.nih.gov/10742469/ |
| T7 Klar 2009 "random" | qualitative | UNVERIFIABLE | Not opened this run | https://doi.org/10.1016/j.semcdb.2008.11.003 |
| T7 Bhokare 2024 | 100 × 100k strands in 2 ms (8× MSAA, LOD); 47 ms without LOD; 1.7 MB (13–21 KB); 300k strands 1.8 / 3.3 ms vs LHS 18 ms; 340 MB vs 39 KB; "GTX 4090" | VERIFIED | Fig. 1 caption; Results; Fig. 7 caption | https://www.cemyuksel.com/research/hairmesh_rendering/Real-Time_Hair_Rendering_with_Hair_Meshes-SIGGRAPH24.pdf |
| T7 HairWorks | "roughly 3-5ms" example budget; "A GTX 970 or better" | VERIFIED | Article body | https://developer.nvidia.com/content/hairworks-authoring-considerations |
| T7 NVIDIA LSS | ~2× faster, ~5× less VRAM vs DOTS; ~1 vertex/segment vs ≥2 for quads | VERIFIED | Intro and Fig. 5 caption | https://developer.nvidia.com/blog/render-path-traced-hair-in-real-time-with-nvidia-geforce-rtx-50-series-gpus |
| Sources disagree: Luo | framed as a Grok-vs-researcher conflict | FIX | Replace with: pooled 29.5% (VERIFIED); NSPT 30.3%, TZL 28.5% (S1 counts per Grok, arithmetically consistent with the catalogue, not opened). No conflict | — |

## Source table

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| Kashiyama 2021 | https://www.ncbi.nlm.nih.gov/pmc/articles/PMC8367035/ (via Europe PMC XML) | y | y (CC BY-NC-ND 4.0) |
| Kim 2025 | https://www.ncbi.nlm.nih.gov/pmc/articles/PMC12502931/ | y | y (CC BY-NC-ND 4.0) |
| Luo 2023 | https://doi.org/10.1016/j.jid.2023.04.008 ; JID full text | n (403 to curl) | y. Crossref VOR licence is CC BY 4.0, but see Sources disagree |
| Open Targets GCST90671959 | https://platform.opentargets.org/study/GCST90671959 (API checked) | y | n/a |
| Klar 2003 | https://www.ncbi.nlm.nih.gov/pmc/articles/PMC1462764/ | y | y (abstract only) |
| Ziering 2003 | https://pubmed.ncbi.nlm.nih.gov/12859380/ | y | y |
| De La Mettrie 2007 / Loussouarn 2007 | PubMed 18078200 / 17919196 | y | y |
| Loussouarn 2016 | https `…306744/article.phtml?tab=texte` | y (https); http form n (500) | y (© JLE) |
| Grunwald 2025 | PMC11830429 | y | y (CC BY 4.0) |
| Rajput 2021 | PMC8719974 | y | y (CC BY-NC-ND 4.0) |
| Tafuri 2019 | advances.realtimerendering.com/s2019/hair_presentation_final.pdf | y | y |
| Hodes & Engel 2013 | slideshare 20208628 | y (WebFetch; JS challenge to curl) | y |
| GPU Pro 5 | oreilly.com … chapter-99 | n (403) | — |
| TressFX | github.com/GPUOpen-Effects/TressFX | y | y (MIT) |
| Lengyel 2001 | hhoppe.com/fur.pdf | y | y |
| Epic UE 4.27 | dev.epicgames.com … hair-rendering | y | y |
| Nusbaum 2009 / Ceballos 2013 / Koo 2000 | PubMed 19397668 / 23582301 / 10742469 | y | y |
| Klar 2009 | doi 10.1016/j.semcdb.2008.11.003 | not opened | — |
| Bhokare 2024 | cemyuksel.com PDF | y | y |
| NVIDIA HairWorks | developer.nvidia.com/content/hairworks-authoring-considerations | y | y |
| NVIDIA LSS blog | developer.nvidia.com/blog/render-path-traced-hair… | y | y |
| 4Gamer CEDEC 2013 | https://www.4gamer.net/games/125/G012544/20130822084/ | y (200; content not re-read) | — |

## Sources disagree (new, from this check)

- **Luo 2023 licence:** Crossref's version-of-record licence is CC BY 4.0 (`http://creativecommons.org/licenses/by/4.0/`), and Europe PMC lists "cc by". The University of Edinburgh repository labels its copy of the published PDF "CC BY-NC-ND". Use the publisher's CC BY 4.0, and cite numbers as facts either way.

## Grok section

- One live-search pass for B6–B8 (grok-4.6, `--reasoning-effort xhigh`, `--always-approve`, prompt `/tmp/lv17/grok/B.txt`). It ran from 08:49:03 to 08:56:14 AEDT and exited with code 0. No other runs.
- What it said for B6, and what I did with it:
  - **Luo:** S1 counts NSPT 1,497 C / 652 CC; TZL 1,394 C / 556 CC; per-cohort CCW 30.34% / 28.51%; pooled 29.47%. I checked the totals against the GWAS Catalog and Open Targets: they match exactly. I could not open S1 myself, so the per-cohort split stays UNVERIFIABLE. Grok cited the Edinburgh repository's CC BY-NC-ND licence; I found Crossref says CC BY 4.0, and recorded the conflict.
  - **Loussouarn 2016:** Table 2 and Table 5 figures matched my own read.
  - **Frostbite, HairWorks, LSS:** these matched my own read of the primary sources.
  - **TressFX:** Grok said it found no Hodes & Engel total. I read 4.8070 ms myself on slide 55, so Grok's "NOT FOUND" was wrong.
- No number in this note was taken from Grok alone.
