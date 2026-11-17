# Verification of A5-joint-rom.md

Checked 2026-11-16 (AEDT, UTC+11) by Library Verify's checker. The researcher's file was not edited.

**File checked:** mtime 2026-11-16 10:30:00 +1100, 160 lines, 21627 bytes, recorded before checking. Re-measured when this report was written: 160 lines, 21627 bytes (unchanged = yes).

## Summary

**Rows: 55 total: 52 VERIFIED, 3 FIX, 0 UNVERIFIABLE.**

- **3 FIXes, all in Dempster Table A5-4:** forearm supination 113 (not 115); knee flexion standing voluntary 113 (not 115); ankle (marked "OCR unreadable") = dorsiflexion 35°, plantar flexion 38°. All three re-read from the scan and independently confirmed by NASA RP-1024 Vol. I Table 1 (Barter, Emanuel & Truett 1957, means from Dempster), https://archive.org/stream/nasa_techdoc_19790003563/19790003563_djvu.txt. That table also gives SDs and 5th/95th percentiles, so **A5-4's missing spread can be filled from it**.
- A5-1 (CDC/Soucie): all 88 cells match the CDC archive page; 95 % CIs and collection dates (Nov 2003 – Jul 2006) confirmed. Population (674 subjects, 53.6 % female, ages 2–69, 9 PTs, passive ROM) confirmed via the Europe PMC REST API abstract. CDC also publishes the public-use dataset (https://stacks.cdc.gov/view/cdc/153156, found via Grok, opened by me), which would allow SDs to be computed.
- A5-2 (Roaas): Table 1 p. 206 values and population ✓. Minor note on observed ranges (L48).
- A5-3 (VA, 38 CFR §4.71/4.71a, Plates I–II): all ✓.
- Blocked links: Soucie DOI (Wiley) 403 and europepmc.org 403 to curl; content confirmed via https://www.ebi.ac.uk/europepmc/webservices/rest/search?query=EXT_ID:21070485%20AND%20SRC:MED&resultType=core&format=json. Dempster handle 2027.42/4540 → Deep Blue 403; bitstream opens; Internet Archive mirror https://archive.org/details/DTIC_AD0087892.
- Structure check: rows have URLs; access date only in header. Not found / Sources disagree present.

### Grok live-search pass

Ran 2026-11-16 ~10:47–10:53 AEDT: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve` (live web search on), single 10-question prompt; waited until the process exited (exit 0). A Grok answer counted only where it gave a URL that I then opened myself; no number was taken from Grok alone.

For A5: Grok independently found NASA RP-1024 Table 1 (Barter et al. 1957) giving **supination 113 (SD 22)** and **knee flexion standing voluntary 113 (SD 13)**; I opened the archive.org text and confirmed both, and also read ankle flexion 35 / extension 38 there (Grok had said the ankle values were "not found"). Grok confirmed Soucie n = 674, ages 2–69, and that the live cdc.gov joint-ROM page is 404 with the archive copy working. **Conflict raised: the file's 115/115 vs 113/113 – resolved in favour of 113.**

## Row table

L = line number in the researcher's file. Status: VERIFIED = re-read or recomputed from the source and matches; FIX = correction needed (corrected value given); UNVERIFIABLE = could not be re-read (reason given).

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| L15 | Hip extension ; 26.2 (23.9–28.5) ; 28.3 (27.2–29.4) ; 20.5 (18.6–22.4) | VERIFIED | Matches CDC archive tables (mean and 95 % CI). | https://archive.cdc.gov/www_cdc_gov/ncbddd/jointrom/index.html |
| L16 | Hip flexion ; 140.8 (139.2–142.4) ; 131.1 (129.4–132.8) ; 134.9 (133.0–136.8) | VERIFIED | Matches CDC archive tables (mean and 95 % CI). | https://archive.cdc.gov/www_cdc_gov/ncbddd/jointrom/index.html |
| L17 | Knee flexion ; 152.6 (151.2–154.0) ; 147.8 (146.6–149.0) ; 142.3 (140.8–143.8) | VERIFIED | Matches CDC archive tables (mean and 95 % CI). | https://archive.cdc.gov/www_cdc_gov/ncbddd/jointrom/index.html |
| L18 | Knee extension ; 5.4 (3.9–6.9) ; 1.6 (0.9–2.3) ; 2.4 (1.5–3.3) | VERIFIED | Matches CDC archive tables (mean and 95 % CI). | https://archive.cdc.gov/www_cdc_gov/ncbddd/jointrom/index.html |
| L19 | Ankle dorsiflexion ; 24.8 (22.5–27.1) ; 22.8 (21.3–24.3) ; 17.3 (15.6–19.0) | VERIFIED | Matches CDC archive tables (mean and 95 % CI). | https://archive.cdc.gov/www_cdc_gov/ncbddd/jointrom/index.html |
| L20 | Ankle plantar flexion ; 67.1 (64.8–69.4) ; 55.8 (54.4–57.2) ; 57.3 (54.8–59.8) | VERIFIED | Matches CDC archive tables (mean and 95 % CI). | https://archive.cdc.gov/www_cdc_gov/ncbddd/jointrom/index.html |
| L21 | Shoulder flexion ; 178.6 (176.9–180.3) ; 177.8 (176.7–178.9) ; 171.8 (169.8–173.8) | VERIFIED | Matches CDC archive tables (mean and 95 % CI). | https://archive.cdc.gov/www_cdc_gov/ncbddd/jointrom/index.html |
| L22 | Elbow flexion ; 152.9 (151.5–154.3) ; 151.4 (150.8–152.0) ; 149.7 (148.5–150.9) | VERIFIED | Matches CDC archive tables (mean and 95 % CI). | https://archive.cdc.gov/www_cdc_gov/ncbddd/jointrom/index.html |
| L23 | Elbow extension ; 6.8 (5.2–8.4) ; 2.2 (0.9–3.5) ; 6.4 (4.7–8.1) | VERIFIED | Matches CDC archive tables (mean and 95 % CI). | https://archive.cdc.gov/www_cdc_gov/ncbddd/jointrom/index.html |
| L24 | Forearm pronation ("elbow pronation") ; 84.6 (82.8–86.4) ; 79.6 (78.8–80.4) ; 81.2 (79.6–82.8) | VERIFIED | Matches CDC archive tables (mean and 95 % CI). | https://archive.cdc.gov/www_cdc_gov/ncbddd/jointrom/index.html |
| L25 | Forearm supination ("elbow supination") ; 93.7 (91.4–96.0) ; 86.4 (85.3–87.5) ; 90.0 (88.0–92.0) | VERIFIED | Matches CDC archive tables (mean and 95 % CI). | https://archive.cdc.gov/www_cdc_gov/ncbddd/jointrom/index.html |
| L35 | Hip ; Extension ; 9.4 (5.3) ; 9.5 (5.2) | VERIFIED | Matches Roaas & Andersson Table 1, p. 206. | https://actaorthop.org/actao/article/download/28654/33535/82106 |
| L36 | Hip ; Flexion ; 120.3 (8.3) ; 120.4 (8.3) | VERIFIED | Matches Roaas & Andersson Table 1, p. 206. | https://actaorthop.org/actao/article/download/28654/33535/82106 |
| L37 | Hip ; Abduction ; 38.8 (7.0) ; 38.4 (7.3) | VERIFIED | Matches Roaas & Andersson Table 1, p. 206. | https://actaorthop.org/actao/article/download/28654/33535/82106 |
| L38 | Hip ; Adduction ; 30.5 (7.3) ; 30.5 (7.3) | VERIFIED | Matches Roaas & Andersson Table 1, p. 206. | https://actaorthop.org/actao/article/download/28654/33535/82106 |
| L39 | Hip ; Internal rotation ; 32.6 (8.2) ; 32.5 (8.2) | VERIFIED | Matches Roaas & Andersson Table 1, p. 206. | https://actaorthop.org/actao/article/download/28654/33535/82106 |
| L40 | Hip ; External rotation ; 33.6 (6.8) ; 33.7 (6.7) | VERIFIED | Matches Roaas & Andersson Table 1, p. 206. | https://actaorthop.org/actao/article/download/28654/33535/82106 |
| L41 | Knee ; Extension (hyperextension shown as −) ; −1.6 (2.8) ; −1.7 (3.0) | VERIFIED | Matches Roaas & Andersson Table 1, p. 206. | https://actaorthop.org/actao/article/download/28654/33535/82106 |
| L42 | Knee ; Flexion ; 143.8 (6.4) ; 143.7 (6.6) | VERIFIED | Matches Roaas & Andersson Table 1, p. 206. | https://actaorthop.org/actao/article/download/28654/33535/82106 |
| L43 | Ankle ; Dorsiflexion ; 15.3 (5.8) ; 15.3 (5.8) | VERIFIED | Matches Roaas & Andersson Table 1, p. 206. | https://actaorthop.org/actao/article/download/28654/33535/82106 |
| L44 | Ankle ; Plantar flexion ; 39.7 (7.5) ; 39.6 (7.7) | VERIFIED | Matches Roaas & Andersson Table 1, p. 206. | https://actaorthop.org/actao/article/download/28654/33535/82106 |
| L45 | Foot ; Valgus (eversion) ; 27.6 (4.6) ; 27.9 (5.0) | VERIFIED | Matches Roaas & Andersson Table 1, p. 206. | https://actaorthop.org/actao/article/download/28654/33535/82106 |
| L46 | Foot ; Varus (inversion) ; 27.7 (6.9) ; 27.8 (6.9) | VERIFIED | Matches Roaas & Andersson Table 1, p. 206. | https://actaorthop.org/actao/article/download/28654/33535/82106 |
| L58 | Cervical spine ; Flexion / extension ; 0–45 / 0–45 ; §4.71a Note (2) | VERIFIED | Matches eCFR §4.71a / Plates I–II. | https://www.ecfr.gov/current/title-38/chapter-I/part-4/subpart-B/subject-group-ECFR3ec9b5a5e29e4b8/section-4.71a |
| L59 | Cervical spine ; Lateral flexion, each side ; 0–45 ; §4.71a Note (2) | VERIFIED | Matches eCFR §4.71a / Plates I–II. | https://www.ecfr.gov/current/title-38/chapter-I/part-4/subpart-B/subject-group-ECFR3ec9b5a5e29e4b8/section-4.71a |
| L60 | Cervical spine ; Rotation, each side ; 0–80 (combined normal 340) ; §4.71a Note (2) | VERIFIED | Matches eCFR §4.71a / Plates I–II. | https://www.ecfr.gov/current/title-38/chapter-I/part-4/subpart-B/subject-group-ECFR3ec9b5a5e29e4b8/section-4.71a |
| L61 | Thoracolumbar spine ; Flexion / extension ; 0–90 / 0–30 ; §4.71a Note (2) | VERIFIED | Matches eCFR §4.71a / Plates I–II. | https://www.ecfr.gov/current/title-38/chapter-I/part-4/subpart-B/subject-group-ECFR3ec9b5a5e29e4b8/section-4.71a |
| L62 | Thoracolumbar spine ; Lateral flexion, each side; rotation, each side ; 0–30; 0–30 (combined 240) ; §4.71a Not… | VERIFIED | Matches eCFR §4.71a / Plates I–II. | https://www.ecfr.gov/current/title-38/chapter-I/part-4/subpart-B/subject-group-ECFR3ec9b5a5e29e4b8/section-4.71a |
| L63 | Shoulder ; Forward elevation (flexion); abduction ; 0–180; 0–180 ; Plate I | VERIFIED | Matches eCFR §4.71a / Plates I–II. | https://img.federalregister.gov/EC04NO91.001/EC04NO91.001_large.png |
| L64 | Shoulder ; External / internal rotation (arm abducted 90°) ; 0–90 / 0–90 ; Plate I | VERIFIED | Matches eCFR §4.71a / Plates I–II. | https://img.federalregister.gov/EC04NO91.001/EC04NO91.001_large.png |
| L65 | Elbow ; Flexion ; 0–145 ; Plate I | VERIFIED | Matches eCFR §4.71a / Plates I–II. | https://img.federalregister.gov/EC04NO91.001/EC04NO91.001_large.png |
| L66 | Forearm ; Pronation / supination ; 0–80 / 0–85 ; Plate I | VERIFIED | Matches eCFR §4.71a / Plates I–II. | https://img.federalregister.gov/EC04NO91.001/EC04NO91.001_large.png |
| L67 | Wrist ; Dorsiflexion (extension) / palmar flexion ; 0–70 / 0–80 ; Plate I | VERIFIED | Matches eCFR §4.71a / Plates I–II. | https://img.federalregister.gov/EC04NO91.001/EC04NO91.001_large.png |
| L68 | Wrist ; Ulnar / radial deviation ; 0–45 / 0–20 ; Plate I | VERIFIED | Matches eCFR §4.71a / Plates I–II. | https://img.federalregister.gov/EC04NO91.001/EC04NO91.001_large.png |
| L69 | Fingers II–V ; MCP / PIP / DIP flexion ; 0–90 / 0–100 / 0–70 or 80 ; §4.71a digits note (1) | VERIFIED | Matches eCFR §4.71a / Plates I–II. | https://www.ecfr.gov/current/title-38/chapter-I/part-4/subpart-B/subject-group-ECFR3ec9b5a5e29e4b8/section-4.71a |
| L70 | Hip ; Flexion / abduction ; 0–125 / 0–45 ; Plate II | VERIFIED | Matches eCFR §4.71a / Plates I–II. | https://img.federalregister.gov/EC04NO91.002/EC04NO91.002_large.png |
| L71 | Knee ; Flexion (from 0 extension) ; 0–140 ; Plate II | VERIFIED | Matches eCFR §4.71a / Plates I–II. | https://img.federalregister.gov/EC04NO91.002/EC04NO91.002_large.png |
| L72 | Ankle ; Dorsiflexion / plantar flexion ; 0–20 / 0–45 ; Plate II | VERIFIED | Matches eCFR §4.71a / Plates I–II. | https://img.federalregister.gov/EC04NO91.002/EC04NO91.002_large.png |
| L80 | Wrist ; Flexion / extension ; 90 / 99 | VERIFIED | Re-read from Dempster Table 6 scan; matches (and matches NASA RP-1024 Table 1). | https://hdl.handle.net/2027.42/4540 |
| L81 | Wrist ; Abduction (radial) / adduction (ulnar) ; 27 / 47 | VERIFIED | Re-read from Dempster Table 6 scan; matches (and matches NASA RP-1024 Table 1). | https://hdl.handle.net/2027.42/4540 |
| L82 | Forearm ; Supination / pronation ; 115 / 77 | FIX | Forearm supination = **113**, not 115 (Dempster Table 6, printed pp. 110–112, PDF pp. 130–132; printed sum 190 = 113 + 77). Independently confirmed by Barter, Emanuel & Truett 1957 (means taken from Dempster) as reproduced in NASA RP-1024 *Anthropometric Source Book* Vol. I, Table 1: supination 113 (SD 22). Pronation 77 ✓. | https://hdl.handle.net/2027.42/4540 |
| L83 | Elbow ; Flexion ; 142 | VERIFIED | Re-read from Dempster Table 6 scan; matches (and matches NASA RP-1024 Table 1). | https://hdl.handle.net/2027.42/4540 |
| L84 | Shoulder ; Flexion / extension ; 188 / 61 | VERIFIED | Re-read from Dempster Table 6 scan; matches (and matches NASA RP-1024 Table 1). | https://hdl.handle.net/2027.42/4540 |
| L85 | Shoulder ; Abduction / adduction in the transverse plane (horizontal) ; 134 / 48 | VERIFIED | Re-read from Dempster Table 6 scan; matches (and matches NASA RP-1024 Table 1). | https://hdl.handle.net/2027.42/4540 |
| L86 | Shoulder ; Medial / lateral rotation ; 97 / 34 | VERIFIED | Re-read from Dempster Table 6 scan; matches (and matches NASA RP-1024 Table 1). | https://hdl.handle.net/2027.42/4540 |
| L87 | Hip ; Flexion ; 113 | VERIFIED | Re-read from Dempster Table 6 scan; matches (and matches NASA RP-1024 Table 1). | https://hdl.handle.net/2027.42/4540 |
| L88 | Hip ; Abduction / adduction (transverse plane) ; 53 / 31 | VERIFIED | Re-read from Dempster Table 6 scan; matches (and matches NASA RP-1024 Table 1). | https://hdl.handle.net/2027.42/4540 |
| L89 | Hip ; Medial / lateral rotation, prone ; 39 / 34 | VERIFIED | Re-read from Dempster Table 6 scan; matches (and matches NASA RP-1024 Table 1). | https://hdl.handle.net/2027.42/4540 |
| L90 | Hip ; Medial / lateral rotation, sitting ; 31 / 30 | VERIFIED | Re-read from Dempster Table 6 scan; matches (and matches NASA RP-1024 Table 1). | https://hdl.handle.net/2027.42/4540 |
| L91 | Knee ; Flexion: prone voluntary / prone forced / standing voluntary / kneeling forced ; 125 / 144 / 115 / 159 | FIX | Knee flexion, standing voluntary = **113**, not 115 (glyph comparison on scan; OCR misread). Same NASA/Barter Table 1: 113 (SD 13). Prone voluntary 125, prone forced 144, kneeling forced 159 ✓. | https://hdl.handle.net/2027.42/4540 |
| L92 | Knee ; Medial / lateral rotation (kneeling) ; 35 / 43 | VERIFIED | Re-read from Dempster Table 6 scan; matches (and matches NASA RP-1024 Table 1). | https://hdl.handle.net/2027.42/4540 |
| L93 | Foot ; Inversion / eversion ; 24 / 23 | VERIFIED | Re-read from Dempster Table 6 scan; matches (and matches NASA RP-1024 Table 1). | https://hdl.handle.net/2027.42/4540 |
| L94 | Ankle ; Dorsi / plantar flexion ; **OCR unreadable** (Table 5 median-build group: "flexion" 39.0 ± 7.8 and "ex… | FIX | Ankle no longer unreadable: **dorsiflexion ("flexion") 35°, plantar flexion ("extension") 38°** (sum 73) from Dempster Table 6 scan; confirmed by NASA RP-1024 Vol. I Table 1 (Barter et al.): ankle flexion 35 (SD 7), ankle extension 38 (SD 12). Table 5 median-build values 39.0 ± 7.8 / 41.6 ± 10.6 ✓ (p. 108). | https://hdl.handle.net/2027.42/4540 |
| L48 (prose) | Roaas observed ranges (text, p. 206) | VERIFIED | Values match; text continues on p. 207; file omits ankle dorsiflexion 5–40 and knee extension 0 to −10 (minor). | https://actaorthop.org/actao/article/download/28654/33535/82106 |
| Spread (A5-4) | Dempster group means without spread | VERIFIED | Spread absent in file; SDs and 5th/95th percentiles available in NASA RP-1024 Vol. I Table 1 (Barter et al. 1957). | https://archive.org/stream/nasa_techdoc_19790003563/19790003563_djvu.txt |

## Source table

| Key | URL | Resolves (y/n) | Licence statement correct (y/n) | Note |
|---|---|---|---|---|
| CDC Joint ROM (archive) | https://archive.cdc.gov/www_cdc_gov/ncbddd/jointrom/index.html | y | y | Live cdc.gov page is 404 (per Grok; archive copy is the one cited). CDC data-use wording stated correctly. |
| Soucie 2011 DOI | https://doi.org/10.1111/j.1365-2516.2010.02399.x | n (403 Wiley bot block) | y | Abstract via EBI REST API. © Wiley (Crossref: Wiley TDM licence) – stated correctly. |
| Europe PMC page | https://europepmc.org/article/MED/21070485 | n (403) | n/a | Use the EBI REST URL instead. |
| Roaas & Andersson 1982 | https://actaorthop.org/actao/article/download/28654/33535/82106 | y | y |  |
| eCFR 38 CFR 4.71/4.71a + Plates | ecfr.gov / img.federalregister.gov links in file | y | y | US Gov public domain. |
| Dempster 1955 | https://hdl.handle.net/2027.42/4540 | redirect y; landing n (403); bitstream y | y | Add mirror https://archive.org/details/DTIC_AD0087892. |
| NASA RP-1024 Vol. I (cross-check, new) | https://archive.org/stream/nasa_techdoc_19790003563/19790003563_djvu.txt | y | n/a | US Gov (NASA) work. |
