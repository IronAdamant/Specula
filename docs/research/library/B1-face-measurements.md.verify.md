# B1 verification: head and face measurements

Verifier: Library Verify (adversarial re-check). Written 2026-11-16, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It is a check of `B1-face-measurements.md`. B1 itself was not edited.

## Version checked

- Path: `/workspace/specula-research/B1-face-measurements.md`
- Size: 27,050 bytes, 236 lines
- File mtime: **2026-11-16 09:54:07 AEDT (Australia/Sydney, UTC+11)**
- Last re-read before this file was written: **2026-11-16 10:19 AEDT**. The file was bit-for-bit identical to a snapshot taken at the start of this check. There was no later rewrite while the check was running.
- Grok CLI: tried first (`~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve`). It exited at once with **“Not signed in”**. **No login was attempted.** All checks used curl, WebFetch, WebSearch, and local copies of the source PDFs/DOCX under `/tmp/b1verify/` (outside any repo). Access date for live URLs: **2026-11-16**.

## Summary

Unit of counting: **one data row** (a table row, or the clearly bounded orbital-canon claim under Table 4). Multi-number rows were checked number-by-number, including population, sex, age and n on every face measurement; a single wrong figure makes the whole row FIX. Licence wording is in the source table and in “Most serious problems,” not in the row counts.

| Status | Rows | What it means |
|---|---:|---|
| **VERIFIED** | **67** | The cited source opens (or an open copy does), and it states that number, units, and population/sex/age/n. |
| **FIX** | **2** | A stated number is wrong. Correction below. |
| **UNVERIFIABLE** | **0** | — |
| **Total data rows** | **69** | Table 1: 13; Table 2: 15; Table 3: 29; Table 4: 9 + 1 canon claim; Table 5: 2. |

**Most serious problems**

1. **FIX — Table 2, Bizygomatic breadth, men n written 2543.** Zhuang & Bradtmiller 2005, Table VI, gives **n = 2542** for male bizygomatic breadth (mean 143.5, SD 6.9 unchanged). B1’s table preamble already states the exception correctly (“bizygomatic men 2542”); the table body does not. All other male n values in Table 2 are 2543, as the source has them.
2. **FIX — Table 2, Interpupillary distance, women n written 1454.** Table VI gives **n = 1452** for female interpupillary distance (mean 61.9, SD 3.5 unchanged). Again the preamble is right (“interpupillary women 1452”); the table body is not.
3. **Licence wording in “How to use these tables” groups NIOSH with ANSUR as “US government work.”** That is too loose. The ANSUR II report is a US Army technical report (“Approved for public release; distribution is unlimited”) and is correctly treated as US government work / public domain in the US. The NIOSH numbers come from a *Journal of Occupational and Environmental Hygiene* article that is **© Taylor & Francis**. B1’s Licence section already says to treat that article as copyrighted and to cite the numbers as facts — keep that, and do not describe the published article as US government work. The survey was federally funded and a copy is hosted on CDC Stacks; that does not make the journal article public domain.
4. **Fragile full-text links** (numbers were still checked from another copy where noted): the CDC Stacks PDF URL returns **HTTP 403** to curl from this box (WebFetch did retrieve the article text); the Taylor & Francis DOI returns **HTTP 500**, as B1 already notes; the figshare Additional file 1 DOI returns **403** to curl. Table S6 was verified from the official Springer ESM DOCX (`https://static-content.springer.com/esm/art%3A10.1186%2Fs13293-016-0076-8/MediaObjects/13293_2016_76_MOESM1_ESM.docx`, HTTP 200). The BMC hostname `biologysexdifferences.biomedcentral.com` now 404s; the article DOI resolves to Springer Link (HTTP 200).

**What is in good shape.** Every Table 1 mean, SD and P5/P50/P95 matches the ANSUR II PDF at the cited pages (cm; working database n = 4082 men / 1986 women, ages 17–58). Every Table 2 mean, SD and P5/P50/P95 matches NIOSH Table VI (mm; percentiles are the 5th / 50th / 95th of the 1, 2, 5, 10, 20, 25, 30, 40, 50, 60, 70, 75, 80, 90, 95, 98, 99 columns; weighted to the US population aged 18–66). Every Table 3 n, mean, SD and Cohen’s d matches official Additional file 1 Table S6 (adult 22–25 group; 222 men, mean age 23.55; 360 women, mean age 23.46, before per-measure exclusions; self-identified European ancestry at Pittsburgh, Seattle, Houston and Iowa City). Every Table 4 mean/SD matches Karaca Saygili et al. 2016 Table 2 p.514 (200 adults 20–35, 100 F / 100 M, Nevşehir, BMI 20–25). Table 5 ratios recompute from Table 3 means to the two quoted decimals. No used source is wrongly marked as free: Kesterke is correctly CC BY 4.0 (with the standard BMC CC0 waiver for data made available in the article); Karaca is correctly © Via Medica, facts cited. Farkas is correctly left unused.

**§ Not found and § Sources disagree** are present and, where I could check, honest. The ~5 mm eye-fissure disagreement (3DFN 28.6 / 27.7 mm vs Turkish caliper 33.9 / 33.4 mm) is real and correctly flagged. ANSUR↔NIOSH conversions from cm to mm are exact. Ear position/inclination is correctly recorded as not found.

## Row table

Columns: row id, claim (short), status, note/correction, URL checked. Access date 2026-11-16 throughout.

### Table 1 — ANSUR II (Gordon et al. 2014), cm

Population, sex, age and n (working database, 4082 men / 1986 women, ages 17–58, stratified to the 2011 Army) are on printed p.38 / PDF p.48 and Table 11. The braids/cornrows footnote for head breadth, head length, head circumference and tragion–top of head is on printed p.44 / PDF p.54; the report found no significant change in means (0.05 level). Units are centimetres. “Approved for public release; distribution is unlimited.” Page citations use PDF page = printed page + 10, which matches the file.

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| A1 | Head length F 18.98 (0.74) 17.80/19.00/20.20; M 19.95 (0.70) 18.80/20.00/21.10 | VERIFIED | PDF pp.147–148 (printed 137–138). | [ANSUR PDF p.148](https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf#page=148) (200) |
| A2 | Head breadth* F 14.78 (0.52) 14.00/14.70/15.60; M 15.43 (0.55) 14.60/15.40/16.30 | VERIFIED | PDF pp.143–144. | same PDF p.144 |
| A3 | Head circumference* F 56.11 (1.94) 53.20/56.00/59.70; M 57.44 (1.60) 54.80/57.40/60.10 | VERIFIED | PDF pp.145–146. | same PDF p.146 |
| A4 | Tragion–top of head F 12.65 (0.65) 11.60/12.70/13.70; M 13.11 (0.62) 12.10/13.10/14.10 | VERIFIED | PDF pp.217–218. | same PDF p.218 |
| A5 | Menton–sellion length F 11.31 (0.61) 10.40/11.30/12.30; M 12.26 (0.67) 11.20/12.30/13.40 | VERIFIED | PDF pp.175–176. | same PDF p.176 |
| A6 | Bizygomatic breadth F 13.38 (0.56) 12.50/13.40/14.30; M 14.26 (0.62) 13.30/14.30/15.30 | VERIFIED | PDF pp.83–84. | same PDF p.84 |
| A7 | Interpupillary breadth F 6.17 (0.36) 5.55/6.20/6.75; M 6.40 (0.34) 5.85/6.40/7.00 | VERIFIED | PDF pp.159–160. | same PDF p.160 |
| A8 | Ear length F 5.94 (0.39) 5.30/6.00/6.60; M 6.42 (0.45) 5.70/6.40/7.20 | VERIFIED | PDF pp.115–116. | same PDF p.116 |
| A9 | Ear breadth F 3.32 (0.28) 2.90/3.30/3.80; M 3.61 (0.30) 3.10/3.60/4.10 | VERIFIED | PDF pp.113–114. | same PDF p.114 |
| A10 | Ear protrusion F 2.04 (0.27) 1.60/2.00/2.50; M 2.32 (0.30) 1.90/2.30/2.80 | VERIFIED | PDF pp.117–118. | same PDF p.118 |
| A11 | Bitragion chin arc F 31.10 (1.42) 28.80/31.10/33.50; M 33.17 (1.40) 30.90/33.20/35.50 | VERIFIED | PDF pp.79–80 (printed 69–70). | same PDF p.80 |
| A12 | Bitragion submandibular arc F 28.80 (1.53) 26.40/28.70/31.30; M 31.57 (1.66) 28.90/31.50/34.50 | VERIFIED | PDF pp.81–82. | same PDF p.82 |
| A13 | Neck circumference F 32.98 (1.92) 30.20/32.80/36.30; M 39.76 (2.58) 36.00/39.50/44.30 | VERIFIED | PDF pp.177–178. | same PDF p.178 |

### Table 2 — NIOSH / Zhuang & Bradtmiller 2005, mm

Population: 41 sites in 8 states; stratified by age (18–29, 30–44, 45–66), sex and race/ethnicity; n = 2543 men, 1454 women; Table VI footnote: “Respirator sample weighted to represent U.S. population aged 18–66 (weight in kg, all other values in mm).” Traditional calipers and tape. Percentile columns in Table VI are 1, 2, 5, 10, 20, 25, 30, 40, 50, 60, 70, 75, 80, 90, 95, 98, 99; B1 correctly takes the 5th, 50th and 95th. The article is in *J Occup Environ Hyg* 2005;2(11):567–576; Table VI is on printed pp.572–573. Two row-level n exceptions are the only numerical errors.

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| N1 | Head length M 2543, 197.3 (7.4) 185/197/210; F 1454, 187.5 (7.2) 175/187/199 | VERIFIED | | [CDC Stacks PDF](https://stacks.cdc.gov/view/cdc/190020/cdc_190020_DS1.pdf) (curl 403; WebFetch retrieved the article) |
| N2 | Head breadth M 153.0 (6.0) 144/153/163; F 146.8 (5.6) 137/146/156 | VERIFIED | n 2543/1454. | same |
| N3 | Head circumference M 575.7 (17.1) 547/575/604; F 554.9 (17.8) 527/555/585 | VERIFIED | | same |
| N4 | Menton–sellion M 122.7 (7.0) 111/123/135; F 113.4 (6.1) 104/113/124 | VERIFIED | | same |
| N5 | Subnasale–sellion M 52.0 (4.1) 45/52/59; F 48.2 (3.8) 42/48/55 | VERIFIED | | same |
| N6 | Bizygomatic breadth M **n 2543**, 143.5 (6.9) 132/143/155; F 1454, 135.1 (6.5) 124/135/146 | **FIX** | Mean, SD and percentiles match. **Correction: male n = 2542.** Preamble already has this exception. | same, Table VI |
| N7 | Bigonial breadth M 120.4 (10.4) 105/120/140; F 110.1 (8.9) 98/110/125 | VERIFIED | | same |
| N8 | Minimum frontal breadth M 105.5 (5.7) 95/105/115; F 102.9 (5.4) 94/103/111 | VERIFIED | | same |
| N9 | Maximum frontal breadth M 112.3 (5.5) 104/112/122; F 108.6 (5.3) 100/108/117 | VERIFIED | | same |
| N10 | Interpupillary distance M 2543, 64.5 (3.6) 59/65/71; F **n 1454**, 61.9 (3.5) 56/62/68 | **FIX** | Mean, SD and percentiles match. **Correction: female n = 1452.** Preamble already has this exception. | same, Table VI |
| N11 | Nose breadth M 36.6 (4.1) 31/36/45; F 33.2 (3.9) 28/33/41 | VERIFIED | | same |
| N12 | Nose protrusion M 21.1 (2.7) 17/21/26; F 19.8 (2.7) 16/20/25 | VERIFIED | | same |
| N13 | Nasal root breadth M 16.6 (2.3) 13/16/20; F 16.3 (2.0) 13/16/20 | VERIFIED | | same |
| N14 | Lip length (ch–ch) M 51.1 (4.2) 44/51/58; F 48.0 (4.0) 42/48/55 | VERIFIED | The note that NIOSH “lip length” is mouth-corner distance is right. | same |
| N15 | Bitragion chin arc M 331.2 (15.5) 306/330/355; F 303.9 (14.9) 280/305/328 | VERIFIED | | same |

### Table 3 — Kesterke et al. 2016, Table S6, mm

Population (article Table 1): adult group aged 22–25, 222 men (mean age 23.55, SD 1.08) and 360 women (mean age 23.46, SD 1.09) before per-measure exclusions; 3DFN recruitment limited to self-identified European ancestry at Pittsburgh, Seattle, Houston and Iowa City (B1’s “Iowa sites” is slightly loose — the article says Iowa City — but not a numerical error). First five measurements: spreading calipers (eu–eu, ft–ft, zy–zy, go–go, g–op); the rest from 3D stereophotogrammetry. “a” = left side only (bilateral measures; no meaningful L/R difference). d = covariate-adjusted Cohen’s d (ANCOVA for height and age); positive values in this table correspond to larger male means. Landmarks listed under the table match article Table 2.

Every n / mean / SD / d in the 29 rows was compared with official Additional file 1 Table S6 (Springer ESM DOCX) and matches exactly. Figshare DOI 10.6084/m9.figshare.4458020 is the same supplement but returns 403 to curl.

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| K1–K5 | Caliper block: max cranial width 211, 152.01 (5.39) / 349, 145.72 (5.11), d=1.20; min frontal width 213, 112.54 (8.54) / 349, 105.91 (8.49), d=0.80; max facial width 213, 136.66 (6.14) / 330, 129.15 (5.31), d=1.34; mandibular width 209, 103.88 (5.54) / 343, 96.55 (5.88), d=1.21; max cranial length 212, 198.05 (6.19) / 349, 187.00 (6.22), d=1.84 | VERIFIED | These are the five spreading-caliper measures. | [DOI](https://doi.org/10.1186/s13293-016-0076-8) → Springer (200); ESM DOCX (200); [PMC4841054](https://pmc.ncbi.nlm.nih.gov/articles/PMC4841054/) (HEAD 200) |
| K6–K29 | Remaining 24 3D measures, including cranial base width, three facial depths (a), morphological / upper / lower facial height, intercanthal 221, 33.01 (3.01) / 359, 31.25 (2.83), d=0.61; outercanthal; palpebral fissure length a 216, 28.56 (1.80) / 357, 27.70 (1.95), d=0.46; nasal, labial and lip / philtrum / vermilion heights, through cutaneous lower lip height 218, 12.20 (2.55) / 358, 10.70 (2.23), d=0.64 | VERIFIED | All 24 rows match Table S6, including the two near-zero vermilion d values (0.05, 0.05). | same ESM Table S6 |

### Table 4 — Karaca Saygili et al. 2016, mm

Population: 200 healthy students and staff (100 females, 100 males) from Nevşehir University, ages 20–35, BMI 20–25; “direct measurements were made on the faces with a sliding calliper.” Table 2, p.514. PDF: “Copyright © 2016 Via Medica.”

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| T4-1 | tr–n F 66.93 (8.13); M 68.80 (9.61) | VERIFIED | Table 2, p.514. | [DOI](https://doi.org/10.5603/FM.a2016.0028) (200); [Balikesir PDF](https://acikerisim.balikesir.edu.tr/server/api/core/bitstreams/9c15ac15-2e25-4fa5-a92a-2170b5fcc82a/content) (200) |
| T4-2 | n–sn F 50.36 (3.74); M 53.14 (4.41) | VERIFIED | | same |
| T4-3 | sn–gn F 63.44 (5.88); M 70.54 (5.58) | VERIFIED | | same |
| T4-4 | ex–en left F 33.39 (1.84); M 33.91 (2.30) | VERIFIED | | same |
| T4-5 | en–en F 31.86 (2.36); M 33.17 (2.79) | VERIFIED | | same |
| T4-6 | zy–zy F 127.20 (6.54); M 129.06 (7.08) | VERIFIED | | same |
| T4-7 | al–al F 32.32 (2.71); M 35.15 (2.92) | VERIFIED | | same |
| T4-8 | ch–ch F 48.88 (3.92); M 51.55 (4.06) | VERIFIED | | same |
| T4-9 | sa–sba F 58.81 (4.29); M 61.49 (4.80) | VERIFIED | | same |
| T4-10 | Neoclassical canons held in only a minority; e.g. orbital canon 25% of women and 23% of men (abstract, p.512) | VERIFIED | Abstract and results: “the left eye-fissure length was equal to the intercanthal width in 25% of females and in 23% of males.” “Minority” is a fair summary (highest listed shares are 33% F / 23% M). | same PDF |

### Table 5 — ratios computed from Table 3 means

B1 correctly labels these as the author’s arithmetic, not a quoted figure. Recomputed from Table 3 means, rounded half-up to two decimals.

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| R1 | Male: 1.16, 1.09, 4.79, 0.65, 1.41, 1.27 | VERIFIED | 33.01/28.56=1.1558…; 35.99/33.01=1.0903…; 136.66/28.56=4.7850…; 88.92/136.66=0.6507…; 50.62/35.99=1.4065…; 72.29/56.74=1.2741…. | Derived from K1–K29 (Table 3), not an external source |
| R2 | Female: 1.13, 1.04, 4.66, 0.66, 1.48, 1.22 | VERIFIED | 31.25/27.70=1.1282…; 32.35/31.25=1.0352…; 129.15/27.70=4.6625…; 85.42/129.15=0.6614…; 48.00/32.35=1.4838…; 66.45/54.49=1.2195…. | same |

The reading (intercanthal a little wider than one eye; face about 4.7–4.8 eye-widths, not 5; lower third ~1.2–1.3× nose height) follows from those ratios.

## Source table

| Source | Licence as stated in B1 | Licence check | URL status (2026-11-16) | Notes |
|---|---|---|---|---|
| Gordon et al. 2014, NATICK/TR-15/007 (ANSUR II) | US government work — public domain in the US; “Approved for public release; distribution is unlimited” | **Confirmed** on the report documentation page of the OPEN Design Lab PDF. | PDF 200; [openlab dataset page](https://www.openlab.psu.edu/datasets/ansur2/) 200; [NTIS ADA611869](https://ntrl.ntis.gov/NTRL/dashboard/searchResults/titleDetail/ADA611869.xhtml) 200 | All Table 1 figures checked against this PDF. Raw microdata not downloaded, as B1 says. |
| Zhuang & Bradtmiller 2005, *J Occup Environ Hyg* | © Taylor & Francis; facts cited. Federally funded; copy on CDC Stacks. | **Confirmed.** ISSN 1545-9624; journal typography is T&F. Treat the article as copyrighted. **Do not group it with ANSUR as “US government work”** in the how-to-use blurb (see problem 3). | CDC Stacks PDF: curl 403, WebFetch succeeded; T&F DOI 10.1080/15459620500324727: 500 (already noted in B1); PubMed 16223715: 203 | Table VI means, SDs, n and percentiles checked from the retrieved article. |
| Kesterke et al. 2016, *Biol Sex Differ* 7:23 | CC BY 4.0; “the article states CC0 applies to data made available in the article”; commercial use with attribution | **Confirmed.** PMC and the article footer give CC BY 4.0 (`https://creativecommons.org/licenses/by/4.0/`, 200). The standard BMC/Springer Open sentence applies the CC0 public-domain dedication waiver to “the data made available in this article.” That waiver covers the article tables and Additional file 1, which is what B1 used. It is **not** a licence for FaceBase individual-level 3DFN data (controlled access; B1 did not use those data). | DOI → [Springer article](https://link.springer.com/article/10.1186/s13293-016-0076-8) 200; PMC4841054 HEAD 200 (WebFetch 403); figshare 4458020 curl 403; official ESM DOCX 200 | Table S6 verified from the ESM DOCX, not from a scrape. |
| Weinberg et al. 2016, 3DFN Part 1; FaceBase | Context only; no numbers taken | Fair. FaceBase summary pages are open; individual-level data require application. B1 did not pull numbers from either. | [PMC4841760](https://pmc.ncbi.nlm.nih.gov/articles/PMC4841760/) (cited); [FaceBase](https://www.facebase.org/resources/human/facial_norms/) 200 | — |
| Karaca Saygili et al. 2016, *Folia Morphol* | “Copyright © 2016 Via Medica”; facts cited | **Confirmed** on the PDF first page. Not an open licence; not marked as free. | Via Medica DOI 200 (resolves to journals.viamedica.pl); Balikesir institutional PDF 200 | Table 2 and abstract checked against this PDF. |
| Farkas et al. 2005 | Existence confirmed; tables not accessed; no numbers used | Honest. PubMed 16077306 returns 203. | PubMed 203 | Correctly in Not found. |

## Notes that are not FIXes

- B1’s “Iowa sites” should preferably read “Iowa City.”
- CDC Stacks curl 403 from this box does not mean the URL is dead; WebFetch opened the same URL, and B1 reports a 200 from the researcher’s link check.
- No number in this check came from the grok CLI, and no number was invented.
