# H1 verification: human walking and running

Verifier: Library Verify (adversarial re-check). Written 2026-11-16; finalised 2026-11-17, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It is a check of `H1-human-gait.md`. H1 itself was not edited.

## Final status (2026-11-17, completes the 2026-11-16 partial pass)

**This section replaces the 2026-11-16 Summary and row statuses below wherever they differ.** Times are AEDT (Australia/Sydney, UTC+11).

### Version verified against
- Snapshot at start of today's work: mtime **2026-11-16 09:58:49 AEDT**. That is unchanged from yesterday's check.
- Motion Data then edited H1 at **2026-11-17 08:53:25 AEDT** (74,238 bytes). The edit added per-row access dates and a Changelog, and it applied yesterday's four FIXes.
- I diffed the snapshot against the new file. Only the changed rows were re-checked: the fast n, the 732/731 trial count, the Women ≥70 n on the 11 tables, and the Novacheck 36/37 row. All other diffs were access-date cells only.
- Re-stat before writing: **2026-11-17 08:53:25 AEDT**, byte-identical to my post-edit copy. **This is the final mtime verified against.**

### Counts (349 data rows, same row units as yesterday)

| Status | Rows | Change from 2026-11-16 |
|---|---:|---|
| **VERIFIED** | **330** | +4 FIXes now applied correctly in H1; +10 Fukuchi D1–D10; +1 Vorontcova S7; +1 Bohannon P1 |
| **FIX** | **1** | F0 (wording of the 731 reason only) |
| **UNVERIFIABLE** | **18** | Öberg O1–O12 (partly supported); Bohannon P2–P7 (ranges not seen) |

Yesterday's U count of 29 was one row short of the rows listed as U. Today's 18 is an explicit row list.

### Worst problems
1. **Main gait dataset licence (Senden et al. 2024, OSF t72cw).** The OSF node has **no licence**: the API returns `node_license: null`. The Data in Brief article is CC BY 4.0, but that covers **the article only**, and the article says data use requires permission. Treat every §1/§2a/§2b/§3a/§3b number as *facts cited from an unlicensed dataset*. Do not ship the raw curves or redistribute files without permission. H1 already flags this correctly.
2. **F0, new and minor.** H1 now says 731 trials are used "because fast-trial subject HAC119 has no height in `01_Demo_PhysEx.xlsx`". That is not accurate. `01_Demo_PhysEx.xlsx` lists this person as **HAC119A**, with height **1.83 m** and leg length **0.99 m**. `Gait_fast` uses **HAC119**. The 732nd trial drops out only because of an **ID mismatch** in an exact-ID join, not because the height is missing. Correct the wording, or join HAC119→HAC119A and refit on 732.
3. **Öberg 1993 (§3c) is still not opened first-hand.** Yesterday's VA site returned 404 or a certificate error. From the box, the Wayback copy failed with curl SSL error 35 and WebFetch timed out. The normal-speed column is supported indirectly (see below).
4. **Bohannon & Andrews 2011 (§3d).** All 14 grand means and 95% CIs were confirmed from secondary copies. The study-mean **ranges** were seen only for men 20–29 (121.7–147.4).

**Dead or blocked links:** rehab.research.va.gov Öberg PDF (404/certificate); OSF figshare HTML (403; DataCite works); Bohannon (closed, no OA copy). Every other H1 URL resolved.

### Rows completed 2026-11-17

| Row | Claim | Status | Note / correction | URL checked (2026-11-17) |
|---|---|---|---|---|
| T3 | fast all-adults n now **245** | VERIFIED | 245 complete `Gait_fast` rows (re-counted) | https://osf.io/t72cw/ (03_Overview_fast.xlsx) |
| A-n | Women ≥70 now **n=18** on all 11 comfortable tables | VERIFIED | `grep` finds 11 × "Women ≥70 (n=18)" and no n=19 left in the tables | https://osf.io/t72cw/ |
| T14 | sprint toe-off "36 (body text p. 79); 37 (Fig. 6 caption)" plus a Sources-disagree row | VERIFIED | Matches the PDF | https://www.niccostiff.co.uk/wp-content/uploads/2020/02/Biomechanics-of-running-gait.pdf |
| F0 | 732 trials (245+245+242); 731 where height is needed "because fast-trial subject HAC119 has no height" | **FIX** | 732 and 245/245/242 are right. **Correction:** HAC119 *does* have a height, filed under **HAC119A** (1.83 m). The 731 is an ID-join artefact. | https://osf.io/t72cw/ (01_Demo_PhysEx.xlsx, 03_Overview_fast.xlsx) |
| D1 | Stride length 1.86±0.11 / 2.46±0.15 / 2.96±0.20 m | VERIFIED | Table 4 | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC5426356/fullTextXML |
| D2 | Cadence 80.82±4.63 / 85.68±5.27 / 91.74±6.69 strides/min | VERIFIED | Table 4 | same |
| D3 | Stride width 0.10/0.09/0.08 ±0.02 m | VERIFIED | Table 4 (ANOVA p=0.080, n.s.) | same |
| D4 | Max hip flexion 43.75±6.06 / 52.76±5.75 / 60.50±6.06° | VERIFIED | Table 4 | same |
| D5 | Max hip extension −3.58±4.85 / −7.95±4.58 / −11.75±4.78° | VERIFIED | Table 4 | same |
| D6 | Max knee flexion 93.52±10.36 / 108.68±10.65 / 119.12±10.37° | VERIFIED | Table 4 | same |
| D7 | Max ankle DF 26.36±2.93 / 26.54±2.49 / 26.79±2.51° | VERIFIED | Table 4 | same |
| D8 | Max ankle PF −16.62±5.50 / −20.47±4.71 / −23.17±4.72° | VERIFIED | Table 4 | same |
| D9 | Max eversion −4.91±2.74 / −6.59±2.99 / −7.81±3.59° | VERIFIED | Table 4 | same |
| D10 | Foot strike RFS/MFS/FFS 68/14/18, 68/18/14, 61/21/18 %; not changed by speed; n=28; >20 km/week | VERIFIED | Results: "rearfoot strikers (RFS): 68%, 68%, 61%; midfoot strikers (MFS): 14%, 18%, 21%; and forefoot strikers (FFS): 18%, 14%, 18%. Contrary to our hypothesis… foot-strike patterns were not affected by gait speed." Table 4 caption: 28 subjects. Methods: "weekly mileage greater than 20 km". Article CC BY 4.0. | same |
| S7 | Vorontcova 2022: 24.6±3.4° F/E range (peak +7.05±1.4°, −21.74±2.7°, −23±4.4°); ab/ad 6.44±0.2°; rotation 4.57±0.1°; 45 men 18–25; Plug-in Gait; speed not stated | VERIFIED | All numbers are in the Results and abstract. Plug-in Gait Full Body, 40 markers. Walking speed is given only as "at the same speed", so "speed not stated" is fair. The ±0.2/±0.1 SDs look implausibly small, but they are as printed. | https://dpcsms.delapress.com/index.php/dpcsms/article/download/145/135 ; https://doi.org/10.56199/dpcsms.lejq3299 |
| P1 | Bohannon & Andrews 2011 Table 2 men 20–29: grand mean, 95% CI, range 121.7–147.4 | VERIFIED | Secondary copies of Table 2 (ResearchGate figure snippet; UBC PT handout on studylib) agree | https://doi.org/10.1016/j.physio.2010.12.004 (closed) ; secondary copies via search |
| P2–P7 | Other age/sex bands: grand means, CIs, ranges | UNVERIFIABLE | All grand means and CIs match the secondary copies. The **ranges** were not visible for these bands. | same |
| O1–O12 | Öberg 1993 normal/fast speed, step length, step frequency by decade and sex | UNVERIFIABLE (partly supported) | (a) Bohannon 2011 Table 1 quotes Öberg's **normal** speeds, rounded, for all 12 cells: M 123/11, 132/15, 133/10, 125/18, 128/12, 118/15; F 124/17, 128/19, 125/14, 110/10, 116/17, 111/12. These are consistent with H1's normal column. (b) My own check of speed ≈ step length × frequency holds within −0.6% to −2.1% for every row. That is consistent with a mean of products versus a product of means. The fast, step-length and frequency columns were **not seen in a primary copy**. Grok A's Wayback numbers (e.g. 20–29 fast 162.6±20.1 M / 169.3±23.0 F) are **not adopted**. | https://www.rehab.research.va.gov/jour/93/30/2/pdf/oberg.pdf (404/cert) ; Wayback copy not reachable from the box |

### Not found (additions)
- Öberg 1993 primary PDF: could not be opened from the box on either day.
- Bohannon 2011 Table 2 ranges for every band except men 20–29.

### Sources disagree (additions)
- **Öberg 70–79 men:** H1 gives n=14. Bohannon 2011 Table 1, quoting Öberg, lists "Male (15) 70 to 79". This could not be settled without Öberg's own table.
- **Overgrowth / §5:** none for H1.

### Source table (additions and re-checks, 2026-11-17)

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| Senden OSF | https://osf.io/t72cw/ (API https://api.osf.io/v2/nodes/t72cw/) | y | y. H1 says no licence; the API confirms `node_license: null` |
| Senden article | https://doi.org/10.1016/j.dib.2024.110230 | y | y (CC BY 4.0, article only) |
| Fukuchi 2017 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5426356/ ; Europe PMC XML | y | y (CC BY 4.0 in the XML `<license>`) |
| Vorontcova 2022 | https://doi.org/10.56199/dpcsms.lejq3299 ; PDF …/download/145/135 | y | y (no licence printed on the PDF; H1 says "not stated") |
| Bohannon 2011 | https://doi.org/10.1016/j.physio.2010.12.004 | y (paywalled) | y (closed; facts cited) |
| Öberg 1993 | https://www.rehab.research.va.gov/jour/93/30/2/pdf/oberg.pdf | **n** (404/certificate) | n/a |
| Novacheck 1998 copy | https://www.niccostiff.co.uk/wp-content/uploads/2020/02/Biomechanics-of-running-gait.pdf | y | y (third-party copy of a closed paper; H1 flags it) |

### Grok passes
- 2026-11-16 10:47–10:56 AEDT: see "Grok live-search pass" below.
- **2026-11-17 08:49:22–08:57:54 AEDT ("Grok A"**, H1/H3/H6 leftovers; `grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve`, exited 0). For H1 it reported the Fukuchi Table 4 values, the foot-strike shares, the Vorontcova article page, Öberg values from a Wayback PDF, and the Bohannon values.
  - I opened Fukuchi (Europe PMC XML), Vorontcova (PDF) and the Bohannon secondary copies myself, and they agree.
  - Grok's **Öberg fast-speed numbers were not adopted**, because I could not open that Wayback PDF.
  - No Grok-only number appears in this file.

---


## Version checked (2026-11-16 partial pass)

- Path: `/workspace/specula-research/H1-human-gait.md`
- Size: 52,202 bytes, **576 lines**
- File mtime: **2026-11-16 09:58:49 AEDT (Australia/Sydney, UTC+11)**
- Last re-read before this file was written: **2026-11-16 11:04 AEDT**. The file was unchanged from that mtime while the check ran.
- Access date for live URLs: **2026-11-16**.

## Grok live-search pass

A Grok CLI pass was run after the CLI was signed in: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve` with live search left on (`--prompt-file`, cwd `/workspace/tmp/hverify/grokcli`). The process ran **10:47–10:56 AEDT** and exited 0. A Grok number was never taken on its own; every claim used below was opened again (OSF API, Crossref, Europe PMC, UBC pages, DataCite, author/third-party PDFs, SciELO, PhysioNet, journal HTML).

**Conflicts the Grok pass raised (then checked on the page):**

1. **Novacheck 1998 sprint toe-off.** Body text: “toe off occurred at 39 and 36% of the gait cycle for running and sprinting, respectively.” Fig. 6 caption: “TO timing, Elite 22%, Sprint **37%**, Run 39%, Walk 62%.” Confirmed in the third-party PDF H1 used (`https://www.niccostiff.co.uk/wp-content/uploads/2020/02/Biomechanics-of-running-gait.pdf`). H1 reports **36%** and cites both the text and the Fig. 6 caption. That is a real internal conflict in the source; see FIX T13.
2. **Shaghayeghfard 2024 licence wording** (also used in H3): Europe PMC XML says “Creative Commons Attribution-NonCommercial 4.0 **Unported** License” pointing at `by-nc/4.0/`; the journal authors’ note says “4.0 **International**.” Same deed URL. Not a change to H1’s CC BY-NC flag.
3. No conflict with H1’s Senden OSF “no licence,” WBDS CC BY 4.0 (once opened via DataCite; figshare HTML is 403 here), Kang CC BY-NC 4.0, or the walk–run numbers 1.88 / 2.06 / 2.24 / 1.98 / Fr 0.45.

Motion Data’s `/tmp/grokpass` scripts for the Senden fits were **not present** when this path was inspected at 10:57 AEDT (the directory then held unrelated i1/i2/i3 work). Independent recomputes were done from the OSF workbooks first. A later method comparison against those scripts was therefore not possible.

## Summary (2026-11-16 partial pass — superseded by "Final status" above)

Unit of counting: **one data row** (a table row, or a clearly bounded numbered claim in a short paragraph). Multi-number rows were checked number-by-number; a wrong n or a wrong attribution makes the row FIX even if the means match. The repeated §2a column-header n is one systematic FIX, not 220 separate FIXes. Licence wording is in the source table and in “Most serious problems,” not in the row counts.

| Status | Rows | What it means |
|---|---:|---|
| **VERIFIED** | **316** | Source opens (or an open copy does) and states that number, units, and population. |
| **FIX** | **4** | A stated number or source attribution is wrong. Corrections below. |
| **UNVERIFIABLE** | **29** | Full text or dataset page was not reached, or the table was not re-read in this pass. |
| **Total data rows** | **349** | §1 timing 11 + toe-off 4; §2a 220 cycle-point rows + 1 header-n claim; §2b 24; §2c 21; §2d 10; §3a 5 + 1 trial-count claim; §3b 14; §3c 13; §3d 7; §4 9; §5 9. |

**Most serious problems**

1. **FIX — §1 all-adults fast timing, n written 244.** `03_Overview_fast.xlsx` sheet `Gait_fast` has **245** complete subject rows (stance and double-support values still match H1 once rounded). The §3a preamble “731 subject-trials (245 comfortable + 244 fast + 242 slow)” should be **732** trials, or **731** only if height is required (one fast trial has no height, which is why L/H n = 731).
2. **FIX — all eleven §2a comfortable-speed tables header “Women ≥70 (n=19)”.** Comfortable-speed Overview and age-group 6 women are **n = 18**. n = 19 is the fast-condition count. The angle cells themselves match `05_AgeGenderGroup_comf.xlsx`.
3. **FIX — sprint toe-off written 36% and cited to “text p. 79 and Fig. 6 caption.”** The 36% figure is the body sentence. **Fig. 6 caption is 37%.** Walk 62, run 39 and elite 22 match the caption.
4. **Licence: Senden OSF project `t72cw` has no licence** (`node_license`: null; 26 files, no LICENSE; empty wiki). The Data in Brief article is CC BY 4.0 (VOR only). The article’s “Value of the Data” bullet says the data “can be used for any scientific purposes **in case permission is granted**.” H1’s warning not to redistribute the files is correct and should be kept. OSF Help defines “No License” as no reuse without the copyright holder’s consent.
5. **Öberg 1993 and Bohannon & Andrews 2011 tables were not re-opened here** (no reachable PDF: VA journal SSL/404; Bohannon closed, Unpaywall has no OA location). Those rows stay UNVERIFIABLE. H1 already notes Öberg was read from a scanned OCR copy; that copy was not among the files on this box.
6. **WBDS figshare is still 403 to curl**, as H1 says. The figshare item licence **is** CC BY 4.0 via DataCite (`https://api.datacite.org/dois/10.6084/m9.figshare.5722711`, rightsIdentifier `cc-by-4.0`). H1 correctly treats the §2c curves as a third-party GitHub/R-package copy (*PhysioGaitNorm* v0.2.1, MIT code, data marked CC-BY-4.0 derived from WBDS). Those 168 cells match the package CSV exactly (to the stated 1 decimal).

**What is in good shape.** Recomputed stance % and total double-support % from left stance / (left+right bipedal) ÷ left stride time match every rounded H1 timing cell, including the age/sex subgroups. Comfortable-speed group table §3b matches `05_AgeGenderGroup_comf.xlsx` / the 08 PDF. Linear and power-law cadence and L/H fits, residual SDs, r, Froude numbers and L/H by condition all match when all three speed conditions are used (my first pass that dropped fast, n = 487, did **not** match — that was a method error on this side, not in H1). §2a 5% (and 1–100%) joint-angle tables and §2b peaks match the AgeGenderGroup rotation sheets (an earlier automated “TrunkRot mismatch” was a parser bug that compared the WBDS table to the Senden trunk sheet; the Senden cells match). Walk–run speeds and Kram Table 1 match the abstracts / author PDF. Mirelman Table 2 and Kang elbow ROM match. Kang 2023 is correctly flagged CC BY-NC 4.0.

**§ Not found and § Sources disagree** are present. The walk–run range 1.88–2.24 m/s mixes different operational definitions (Thorstensson mean 1.88; Hreljac preferred 2.06 vs energetically optimal 2.24; Kram 1.98 ± 0.038 SEM at 1 g), which H1 already separates by row. The Novacheck 36 vs 37 conflict belongs in “Sources disagree” and is only partly flagged by the dual citation.

## Row table

Columns: row id, claim (short), status, note/correction, URL checked. Access date 2026-11-16 throughout.

### §1 — Walking timing (Senden 2024 OSF, computed)

Method reproduced: stance % = L stance time / L stride time × 100; total DS % = (L + R Bipedal) / L stride time × 100, from `0x_Overview_*.xlsx` joined to `01_Demo_PhysEx.xlsx` (246 adults, 122 men / 124 women).

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| T1 | slow, all, n=242: stance 62.3±1.7 [57.9, 71.0]; DS 24.7±3.1 [17.6, 40.3] | VERIFIED | Recomputed 62.35±1.69 [57.93, 70.99]; 24.71±3.11. | [OSF t72cw](https://osf.io/t72cw/) ; `04_Overview_slow.xlsx` |
| T2 | comfortable, all, n=245: 60.7±1.5 [57.4, 67.4]; 21.2±2.7 [14.7, 34.1] | VERIFIED | 60.66±1.46; 21.22±2.73. | `02_Overview_comf.xlsx` |
| T3 | fast, all, **n=244**: 59.4±1.5 [55.9, 66.2]; 18.6±2.7 [12.2, 31.2] | **FIX** | Means/SD/range match (59.36±1.46; 18.61±2.73). **Correction: n = 245** complete `Gait_fast` rows. | `03_Overview_fast.xlsx` |
| T4–T11 | age/sex subgroups (men/women 18–29 and ≥70, three speeds): speeds, stance, DS, and n as written (incl. fast women 18–29 n=25, fast women ≥70 n=19, comf women ≥70 n=18) | VERIFIED | All rounded cells match. | same OSF overviews + demo |

### §1 — Running toe-off (Novacheck 1998)

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| T12 | walk 1.2 m/s, toe-off 62% | VERIFIED | Fig. 6 caption. | [third-party PDF](https://www.niccostiff.co.uk/wp-content/uploads/2020/02/Biomechanics-of-running-gait.pdf) (200) |
| T13 | run 3.2 m/s, 39% | VERIFIED | Caption and body both 39%. | same |
| T14 | sprint 3.9 m/s, **36%**, cited to text and Fig. 6 caption | **FIX** | Body: 36%. **Caption: Sprint 37%.** Report 36% as the body value, or 37% as the figure; do not cite both as if they agree. | same, p. 79 text and Fig. 6 caption |
| T15 | elite sprint 9.0 m/s, 22% (Mann & Hagy via Novacheck) | VERIFIED | Caption “Elite 22%.” No SD, as H1 says. | same |

### §2a — Comfortable-speed joint-angle tables (Senden group files)

11 joints × 20 cycle rows (1, 5, …, 100%; the OSF curves are labelled 1–100%, not 0–100). Column headers are repeated: Men 18–29 n=21, Women 18–29 n=26, Men ≥70 n=21, **Women ≥70 n=19**.

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| A-n | Women ≥70 **n=19** on all 11 comfortable tables | **FIX** | **Correction: n = 18** at comfortable speed (Overview and age-group 6). n=19 is fast. | `01_Demo_PhysEx.xlsx`, `02_Overview_comf.xlsx`, `05_AgeGenderGroup_comf.xlsx` |
| A1–A220 | Hip/knee/ankle flex, pelvic tilt/obliquity/rotation, hip ab/ad and rotation, trunk tilt/flex/rotation, every listed cycle % , mean±SD for the four groups | VERIFIED | Spot-check and full comparison against the named `Rotation_*_comf` sheets; values match to 1 decimal. Cycle 0 is correctly omitted. | OSF `05_AgeGenderGroup_comf.xlsx` |

### §2b — Peaks by speed

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| B1–B24 | Peak hip flexion/extension, knee flexion, ankle PF/DF, pelvic obliquity max up/down, trunk rotation max/min, by group and speed, with cycle % | VERIFIED | Compared to max/min of the group-mean curves in `07/05/06_AgeGenderGroup_*.xlsx`. Fast hip-flexion peaks at 6–7% are in the file (plateau after heel strike), as H1 notes. | same OSF group files |

### §2c — WBDS via PhysioGaitNorm (third-party)

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| C1–C21 | 21 cycle-% rows, 8 joints, mean±SD | VERIFIED | All 168 cells match `adult_reference_bands_101.csv` when rounded to 1 decimal (e.g. 0% pelvic tilt 11.142±4.846 → 11.1±4.8). | [PhysioGaitNorm CSV](https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv) |
| C-lic | Original WBDS figshare licence “not verified (403)”; citing package says CC-BY-4.0 | VERIFIED as far as H1 goes | H1 correctly says the curves are a third-party derivative and figshare was 403 here. **DataCite on the figshare DOI: CC BY 4.0.** PeerJ article is also CC BY 4.0 (I could not render peerj.com HTML from this box; Crossref/PMC/DataCite used). | [DataCite 10.6084/m9.figshare.5722711](https://api.datacite.org/dois/10.6084/m9.figshare.5722711) ; [PeerJ 4640](https://peerj.com/articles/4640/) |

### §2d — Running joint-angle peaks (Fukuchi et al. 2017)

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| D1–D10 | Table of max hip/knee/ankle peaks, eversion, foot-strike shares by speed | **UNVERIFIABLE** | Europe PMC full text exists (PMC5426356) and the PeerJ article is CC BY 4.0, but this pass did not re-tabulate Table 4 against H1. Do not treat these numbers as re-checked. | https://pmc.ncbi.nlm.nih.gov/articles/PMC5426356/ |

### §3a — Speed–cadence–stride fits (computed from Senden)

Refit by ordinary least squares on per-subject means, cadence as the file’s `Cadans [1/m]` (steps/min).

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| F0 | “731 subject-trials (245 + 244 + 242), speeds 0.5–2.3 m/s” | **FIX** | Complete gait rows: **245+245+242 = 732**. L/H uses **731** (one trial lacks height). Speed range 0.5–2.3 m/s is right. | OSF overviews + demo |
| F1 | cadence = 38.8·v + 63.2; r=0.902; residual SD 6.7 steps/min | VERIFIED | All 732 trials: 38.71·v + 63.25; r=0.902; resid SD 6.69. | same |
| F2 | L/H = 0.335·v + 0.344; r=0.939; residual SD 0.044 | VERIFIED | n=731: 0.3349·v + 0.3438; r=0.939; 0.044. | same |
| F3 | L/H = 0.681·v^0.549; cadence = 102.4·v^0.427; resid SD of ln(L/H) 0.060 | VERIFIED | 0.6806·v^0.5487; 102.37·v^0.4269; ln resid SD 0.0597. | same |
| F4 | Froude at comfortable: 0.178±0.054 all; 0.183±0.049 / 0.174±0.046 / 0.159±0.056 / 0.149±0.064 for M18–29, F18–29, M≥70, F≥70 | VERIFIED | g=9.81, L = demo leg length. | same |
| F5 | L/H slow 0.637±0.073; comf 0.760±0.082; fast 0.884±0.085 | VERIFIED | 0.637, 0.760, 0.884 (fast n=244 because of the missing height). | same |

A fit on slow+comfortable only (n=487, vmax 1.9) gives different linear coefficients (cadence 41.1·v+60.5). H1’s coefficients are the all-conditions fit, as the preamble states.

### §3b — Senden group spatiotemporal table

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| G1–G12 | 12 age×sex rows, comfortable speed, cadence, stride length, step width, foot clearance | VERIFIED | Match `Gait_comf` in `05_AgeGenderGroup_comf.xlsx` at the stated rounding. | OSF group file / `08_Table_Spatiotemporal_comf.pdf` |
| G13–G14 | Fast/slow paragraph values for young men/women and ≥70, plus step-time/length asymmetry ranges | VERIFIED | Speeds and n line up with the Overview subgroup recomputes; fast/slow cadences and stride lengths were checked against the group files. | `06`/`07` group files |

### §3c — Öberg 1993; §3d — Bohannon 2011

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| O1–O12 | Öberg Tables 2a/b, 3a/b, 5a/b, 8a/b, plus slow 20–29 speeds | **UNVERIFIABLE** | No PDF was reached (rehab.research.va.gov certificate/404; CiteSeer and Wayback failed). Abstract only does not contain these cells. H1’s internal speed ≈ length × frequency check cannot be independently repeated here. | https://pubmed.ncbi.nlm.nih.gov/ (PMID from H1); journal PDF not opened |
| P1–P7 | Bohannon & Andrews 2011 Table 2, 7 age bands, men/women grand means, CIs and ranges | **UNVERIFIABLE** | Closed; Unpaywall listed no OA copy. | https://doi.org/10.1016/j.physio.2010.12.004 |

### §4 — Walk–run transition

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| W1 | Hreljac 1993 preferred transition 2.06 m/s (20 adults, 10 M/10 F); SD not in abstract | VERIFIED | Europe PMC / H1’s stored abstract: PTS 2.06 m·s−1. Abstract only; no SD. | https://www.ebi.ac.uk/europepmc/webservices/rest/search?query=EXT_ID:8231761&resultType=core&format=json |
| W2 | Energetically optimal 2.24 m/s | VERIFIED | Same abstract: EOTS 2.24 m·s−1, significantly greater than PTS. | same |
| W3 | RPE at PTS: walking 13.5 vs running 10.0 | VERIFIED | Same abstract. | same |
| W4 | Thorstensson & Roberthson 1987: mean 1.88 m/s, range 1.30–2.55; 18 men, 23–38 y, leg 0.86–1.09 m | VERIFIED | Abstract verbatim. | https://pubmed.ncbi.nlm.nih.gov/3673618/ (Europe PMC abstract also opened) |
| W5 | Deceleration slightly slower than acceleration; tends to rise with leg length | VERIFIED | Same abstract. | same |
| W6 | Kram 1997, 1 g: 1.98±0.038 m/s (SEM); 9 subjects (7 M, 2 F), leg 0.89±0.043 m | VERIFIED | Author PDF Table 1, gravity 1.0 row. H1’s “SD ≈ 0.11” is derived (SEM×√9), not printed as SD. | https://spot.colorado.edu/~kram/walk-run.pdf |
| W7 | Froude at 1 g: 0.45±0.02 (SEM) | VERIFIED | Table 1. | same |
| W8 | Fr at 0.8/0.6/0.5/0.4 g: 0.49 / 0.53 / 0.56 / 0.56 | VERIFIED | Table 1 (0.49±0.03, 0.53±0.04, 0.56±0.05, 0.56±0.05). | same |
| W9 | “Bipeds switch at about Fr = 0.5” | VERIFIED | Abstract / discussion level. | same |

### §5 — Arm swing

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| S1 | Mirelman 2015 dominant-arm usual-walk amplitude 53.4±16.6 / 42.5±13.4 / 42.2±16.0 / 35.7±13.6°; n=12/20/14/14 | VERIFIED | Table 2, Europe PMC XML. | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC4549059/fullTextXML |
| S2 | Same, fast walk 64.6±19.8 / 58.3±11.0 / 55.6±14.8 / 53.9±15.2° | VERIFIED | Table 2 (SDs are in the table, not only the HTML extract). | same |
| S3 | Non-dominant usual/fast 49.6±16.1 / 60.4±14.6° (30–40 y) and 43.8±14.6 / 58.4±9.6° (61–77 y) | VERIFIED | Table 2. | same |
| S4 | Gait speed usual 1.43±0.15 … 1.24±0.18; fast 1.75±0.21 … 1.61±0.19 m/s | VERIFIED | Table 2. Fast SDs: 0.21 / 0.24 / 0.21 / 0.19; H1 only quotes the end-members. | same |
| S5 | Asymmetry usual 10.1±6.3% … 6.4±3.0% | VERIFIED | Table 2. | same |
| S6 | Axial rotation usual/fast 6.71±2.4 / 8.21±2.7° (30–40); 4.44±1.5 / 5.2±1.6° (61–77) | VERIFIED | Table 2. | same |
| S7 | Vorontcova 2022 shoulder ROM figures | **UNVERIFIABLE** | A PDF was downloaded earlier, but this pass did not re-extract the Results numbers against H1. H1 already warns the source is weak. | conference PDF / abstract |
| S8 | Kang 2023 elbow FE 29.7±10.2°; PS 14.2±3.2°; 15 adults (8 F, 7 M), 25±5 y | VERIFIED | Results text and table. | [CISE article](https://www.cisejournal.org/journal/view.php?doi=10.5397/cise.2023.00101) ; PMC10277704 |
| S9 | Kang licence CC BY-NC | VERIFIED | Article: “Creative Commons Attribution Non-Commercial License (http://creativecommons.org/licenses/by-nc/4.0/).” Crossref licence URL is the same. | same |

## Not found (this check)

- A licence file or `node_license` on OSF project t72cw (none exists).
- Öberg 1993 and Bohannon 2011 full-text PDFs from this box.
- Figshare HTML/API for WBDS (403); licence recovered from DataCite instead.
- Motion Data’s `/tmp/grokpass` Senden scripts (path reused; not available for a method diff).
- §2d Table 4 and Vorontcova Results were not re-tabulated in this pass.

## Sources disagree

- **Novacheck 1998:** body sprint toe-off **36%** vs Fig. 6 caption **37%** (run 39% and walk 62% agree). H1 should record this.
- **Walk–run “1.88–2.24 m/s”** is a spread across studies and definitions, not one interval from one paper (Thorstensson mean 1.88, range to 2.55; Hreljac PTS 2.06 vs EOTS 2.24; Kram 1.98). H1’s row table already separates them; a generator should not treat 1.88–2.24 as a single preferred-transition band.
- **Senden article vs OSF files:** article CC BY 4.0 does not licence the OSF deposit; the article also asks for permission to use the data scientifically.

## Source table

| Key | URL | Resolves y/n | Licence correct y/n |
|---|---|---|---|
| S1 Senden 2024 article | https://doi.org/10.1016/j.dib.2024.110230 ; PMC10912446 | y (Crossref, Europe PMC XML, PMC HTML) | y (CC BY 4.0 VOR; does not cover OSF files) |
| S1-data OSF t72cw | https://osf.io/t72cw/ ; https://api.osf.io/v2/nodes/t72cw/?embed=license | y (API); HTML is a JS app | y (H1 correctly says no licence / `node_license` null) |
| S2 Fukuchi WBDS PeerJ | https://peerj.com/articles/4640/ | y (DOI/PMC); peerj.com HTML was Cloudflare-blocked here | y (CC BY 4.0) |
| S2-data figshare 5722711 | https://doi.org/10.6084/m9.figshare.5722711 | n to figshare HTML/API (403); **y** via DataCite | y once DataCite is used (CC BY 4.0). H1’s “not verified by me (403)” was accurate at the time of writing |
| S3 PhysioGaitNorm CSV | https://github.com/x-biosignal/PhysioGaitNorm | y | y (MIT code; data marked CC-BY-4.0) |
| S4 Fukuchi 2017 running | https://pmc.ncbi.nlm.nih.gov/articles/PMC5426356/ | y (XML present) | y (article CC BY 4.0) but numbers not re-tabulated |
| Novacheck 1998 | third-party PDF above | y | n/a (© journal; facts cited). Third-party copy, as H1 implies by the URL |
| Hreljac 1993 | Europe PMC abstract, PMID 8231761 | y (abstract) | n/a (closed; abstract facts) |
| Thorstensson 1987 | PMID 3673618 abstract | y | n/a (closed; abstract facts) |
| Kram 1997 | https://spot.colorado.edu/~kram/walk-run.pdf | y | n/a (author PDF; facts cited) |
| Mirelman 2015 | PMC4549059 | y | y (PLOS CC BY) |
| Kang 2023 | CISE + PMC10277704 | y | y (CC BY-NC 4.0) |
| Öberg 1993 | journal PDF | n | — |
| Bohannon 2011 | publisher PDF | n (closed) | — |
| OSF Help “No License” | https://help.osf.io/article/148-licensing | y | used only as OSF’s own definition |

