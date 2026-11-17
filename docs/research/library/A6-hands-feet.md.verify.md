# Verification of A6-hands-feet.md

Checked 2026-11-16 (AEDT, UTC+11) by Library Verify's checker. The researcher's file was not edited.

**File checked:** mtime 2026-11-16 10:30:00 +1100, 120 lines, 11736 bytes, recorded before checking. Re-measured when this report was written: 120 lines, 11736 bytes (unchanged = yes).

## Summary

**Rows: 34 total: 33 VERIFIED, 1 FIX, 0 UNVERIFIABLE.**

- **All 31 table rows VERIFIED.** A6-1: every mean, SD, P5, P95 matches the report. Pages: the file cites the second page of each two-page entry (mean/SD page), e.g. hand length p. 132 (entry pp. 131–132) – correct for mean/SD; P5/P95 sit on the facing page. A6-2: all 11 ratios (mean, SD, r) and all six OLS fits/SEE recomputed exactly from the CSVs.
- **Buryanov & Kotiuk Table I, II, III: all values re-read from the publisher PDF** (Wayback copy of https://www.scielo.cl/pdf/ijmorphol/v28n3/art15.pdf, snapshot 2026-10-17, p. 757). The ResearchGate text-layer worry ("4 6.22") is resolved: 46.22 ± 3.94 is printed. Population (66 adults, 19–78 y, right hands, ≤ 0.5 % L/R difference) ✓.
- **Licence FIX:** the file says Int J Morphol is CC BY ("Creative Commons Attribution"). The SciELO footer text says "Creative Commons Attribution License", but the licence link on the article page is **CC BY-NC 4.0** (http://creativecommons.org/licenses/by-nc/4.0/deed.es, seen on https://scielo.conicyt.cl/scielo.php?script=sci_arttext&pid=S0717-95022010000300015), and OpenAlex also records cc-by-nc. **Flag as NON-COMMERCIAL.**
- Blocked links: doi.org/10.4067/… (500), scielo.cl PDF, about page and ResearchGate (403 bot challenge). The about page did open through WebFetch (it carries no licence statement). The article HTML opens through WebFetch and the scielo.conicyt.cl mirror.
- Kolesova is not in A6 (it is in A7; see A7.verify.md).
- Structure check: rows have URLs; access date only in header. Not found / Sources disagree present.

### Grok live-search pass

Ran 2026-11-16 ~10:47–10:53 AEDT: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve` (live web search on), single 10-question prompt; waited until the process exited (exit 0). A Grok answer counted only where it gave a URL that I then opened myself; no number was taken from Grok alone.

For A6: Grok returned Buryanov Table I from the SciELO PDF with all three queried values (46.22, 68.12, 44.63) matching; I confirmed every cell myself from the Wayback PDF. **Conflict raised by Grok: the Int J Morphol licence link points to CC BY-NC 4.0**, contradicting the file's "CC BY"; I confirmed it on the SciELO mirror page and in OpenAlex.

## Row table

L = line number in the researcher's file. Status: VERIFIED = re-read or recomputed from the source and matches; FIX = correction needed (corrected value given); UNVERIFIABLE = could not be re-read (reason given).

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| L11 | Hand length (wrist crease to tip of middle finger) ; 18.11 ± 1.01 ; 16.5–19.9 ; 19.33 ± 0.99 | VERIFIED | Mean, SD, P5, P95 match report (page = mean/SD page of the entry). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L12 | Palm length ; 10.87 ± 0.59 ; 10.0–11.9 ; 11.65 ± 0.62 | VERIFIED | Mean, SD, P5, P95 match report (page = mean/SD page of the entry). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L13 | Hand breadth ; 7.82 ± 0.38 ; 7.2–8.5 ; 8.83 ± 0.44 | VERIFIED | Mean, SD, P5, P95 match report (page = mean/SD page of the entry). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L14 | Hand circumference ; 18.66 ± 0.88 ; 17.3–20.2 ; 21.23 ± 1.02 | VERIFIED | Mean, SD, P5, P95 match report (page = mean/SD page of the entry). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L15 | Wrist circumference ; 15.48 ± 0.78 ; 14.2–16.8 ; 17.59 ± 0.90 | VERIFIED | Mean, SD, P5, P95 match report (page = mean/SD page of the entry). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L16 | Foot length ; 24.63 ± 1.24 ; 22.6–26.8 ; 27.12 ± 1.31 | VERIFIED | Mean, SD, P5, P95 match report (page = mean/SD page of the entry). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L17 | Foot breadth, horizontal ; 9.27 ± 0.48 ; 8.5–10.1 ; 10.19 ± 0.52 | VERIFIED | Mean, SD, P5, P95 match report (page = mean/SD page of the entry). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L18 | Ball-of-foot length (heel to first metatarsal head) ; 18.21 ± 0.96 ; 16.7–19.8 ; 20.09 ± 1.05 | VERIFIED | Mean, SD, P5, P95 match report (page = mean/SD page of the entry). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L19 | Ball-of-foot circumference ; 22.81 ± 1.18 ; 20.9–24.8 ; 25.20 ± 1.29 | VERIFIED | Mean, SD, P5, P95 match report (page = mean/SD page of the entry). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L20 | Heel breadth ; 6.72 ± 0.52 ; 6.0–7.6 ; 7.25 ± 0.55 | VERIFIED | Mean, SD, P5, P95 match report (page = mean/SD page of the entry). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |
| L28 | Hand length / stature ; 0.1112 (0.0047) ; 0.65 ; 0.1101 (0.0042) | VERIFIED | Recomputed from CSVs; mean, SD, r match. |  |
| L29 | Palm length / stature ; 0.0668 (0.0030) ; 0.59 ; 0.0663 (0.0029) | VERIFIED | Recomputed from CSVs; mean, SD, r match. |  |
| L30 | Hand breadth / stature ; 0.0480 (0.0022) ; 0.47 ; 0.0503 (0.0023) | VERIFIED | Recomputed from CSVs; mean, SD, r match. |  |
| L31 | Palm length / hand length ; 0.6006 (0.0188) ; 0.84 ; 0.6028 (0.0203) | VERIFIED | Recomputed from CSVs; mean, SD, r match. |  |
| L32 | Hand breadth / hand length ; 0.4322 (0.0210) ; 0.57 ; 0.4572 (0.0211) | VERIFIED | Recomputed from CSVs; mean, SD, r match. |  |
| L33 | Foot length / stature ; 0.1513 (0.0053) ; 0.72 ; 0.1544 (0.0052) | VERIFIED | Recomputed from CSVs; mean, SD, r match. |  |
| L34 | Foot breadth / stature ; 0.0570 (0.0029) ; 0.41 ; 0.0580 (0.0028) | VERIFIED | Recomputed from CSVs; mean, SD, r match. |  |
| L35 | Foot breadth / foot length ; 0.3768 (0.0182) ; 0.56 ; 0.3759 (0.0171) | VERIFIED | Recomputed from CSVs; mean, SD, r match. |  |
| L36 | Ball-of-foot length / foot length ; 0.7392 (0.0134) ; 0.94 ; 0.7410 (0.0147) | VERIFIED | Recomputed from CSVs; mean, SD, r match. |  |
| L37 | Heel breadth / foot length ; 0.2728 (0.0182) ; 0.51 ; 0.2675 (0.0179) | VERIFIED | Recomputed from CSVs; mean, SD, r match. |  |
| L38 | Hand length / foot length ; 0.7355 (0.0249) ; 0.80 ; 0.7130 (0.0234) | VERIFIED | Recomputed from CSVs; mean, SD, r match. |  |
| L53 | I (thumb) ; 5.67 ± 0.61 ; 21.67 ± 1.60 ; n/a | VERIFIED | Matches Buryanov & Kotiuk Table I/II (publisher PDF via Wayback, p. 757). | https://doi.org/10.4067/S0717-95022010000300015 |
| L54 | II (index) ; 3.84 ± 0.59 ; 15.82 ± 2.26 ; 22.38 ± 2.51 | VERIFIED | Matches Buryanov & Kotiuk Table I/II (publisher PDF via Wayback, p. 757). |  |
| L55 | III (middle) ; 3.95 ± 0.61 ; 17.40 ± 1.85 ; 26.33 ± 3.00 | VERIFIED | Matches Buryanov & Kotiuk Table I/II (publisher PDF via Wayback, p. 757). |  |
| L56 | IV (ring) ; 3.95 ± 0.60 ; 17.30 ± 2.22 ; 25.65 ± 3.29 | VERIFIED | Matches Buryanov & Kotiuk Table I/II (publisher PDF via Wayback, p. 757). |  |
| L57 | V (little) ; 3.73 ± 0.62 ; 15.96 ± 2.45 ; 18.11 ± 2.54 | VERIFIED | Matches Buryanov & Kotiuk Table I/II (publisher PDF via Wayback, p. 757). |  |
| L63 | I ; 1 : n/a : 1.5 : 2.1 | VERIFIED | Matches Buryanov & Kotiuk Table I/II (publisher PDF via Wayback, p. 757). | https://doi.org/10.4067/S0717-95022010000300015 |
| L64 | II ; 1 : 1.4 : 2.5 : 4.3 | VERIFIED | Matches Buryanov & Kotiuk Table I/II (publisher PDF via Wayback, p. 757). | https://doi.org/10.4067/S0717-95022010000300015 |
| L65 | III ; 1 : 1.5 : 2.6 : 3.7 | VERIFIED | Matches Buryanov & Kotiuk Table I/II (publisher PDF via Wayback, p. 757). | https://doi.org/10.4067/S0717-95022010000300015 |
| L66 | IV ; 1 : 1.5 : 2.4 : 3.4 | VERIFIED | Matches Buryanov & Kotiuk Table I/II (publisher PDF via Wayback, p. 757). | https://doi.org/10.4067/S0717-95022010000300015 |
| L67 | V ; 1 : 1.1 : 2.1 : 3.4 | VERIFIED | Matches Buryanov & Kotiuk Table I/II (publisher PDF via Wayback, p. 757). | https://doi.org/10.4067/S0717-95022010000300015 |
| L41–43 (prose) | OLS fits stature↔foot/hand length (F/M) and inverse fits | VERIFIED | Recomputed: 3.728/710.2/44.4; 4.126/881.4/49.0; 3.757/737.4/47.7; 4.639/859.5/50.7 – match. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L69 (prose) | Table III percentages (tip+pd, pm, web height) | VERIFIED | 49.36, 27.80, 26.32, 28.25, 33.92; 30.97, 33.11, 33.61, 31.99; 15.52, 15.33, 18.49, 24.72 – match after rounding. | https://doi.org/10.4067/S0717-95022010000300015 |
| L97 (licence) | Int J Morphol content under CC BY | FIX | Licence link is CC BY-NC 4.0 → non-commercial; do not assume commercial reuse of text/figures. Numbers as facts with attribution still fine. | https://scielo.conicyt.cl/scielo.php?script=sci_arttext&pid=S0717-95022010000300015 |

## Source table

| Key | URL | Resolves (y/n) | Licence statement correct (y/n) | Note |
|---|---|---|---|---|
| Gordon 2014 (ANSUR II report) | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf | y (206) | y | "Approved for public release; distribution is unlimited" confirmed on the report. |
| ANSUR II public CSVs | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv ; …MALE_Public.csv | y (206) | y | Downloaded to /workspace/tmp, recomputed, then deleted. CSVs = unweighted working sample, F 1986 / M 4082, ages 17–58. |
| OpenLab host page | https://www.openlab.psu.edu/ansur2/ | y (200) | n/a |  |
| Buryanov & Kotiuk 2010 DOI | https://doi.org/10.4067/S0717-95022010000300015 | n (500 from SciELO) | n (CC BY-NC 4.0, not CC BY) | Article HTML opens via WebFetch and scielo.conicyt.cl mirror. |
| Buryanov PDF (SciELO) | https://www.scielo.cl/pdf/ijmorphol/v28n3/art15.pdf | n live (403 Cloudflare); y via Wayback http://web.archive.org/web/20261017211938/https://www.scielo.cl/pdf/ijmorphol/v28n3/art15.pdf | n | Values re-read from this copy. |
| ResearchGate copy | https://www.researchgate.net/publication/295677869_Proportions_of_Hand_Segments | n (403) | n/a | No longer needed – use the publisher PDF. |
| Int J Morphol about page | https://www.scielo.cl/revistas/ijmorphol/iaboutj.htm | n curl (403); y WebFetch | n | Page carries no licence text; the licence is on the journal/article footer (CC BY-NC 4.0 link). |
