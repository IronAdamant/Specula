# C5-fish-snakes.md — Library Verify

**Rebuilt 2026-11-17 ~10:02 AEDT.** The full verify file (34,913 bytes, written 09:59:41 AEDT) was lost when the box restore overwrote this folder. This rebuild is taken from the checker's final report. The row-by-row table and full source table were not recoverable; numbers below are as the checker reported them.

## Summary
- File checked: C5-fish-snakes.md at 09:13:56 AEDT, 39,945 bytes. Unchanged at the start and end of the check; restored copy is the same size.
- 87 table rows: **82 VERIFIED, 1 FIX, 4 UNVERIFIABLE**. No rows skipped.
- Almost every number checks out. The only data-row correction is Hu 2013's speed range. The "Science version not opened" caveat on the 33 ± 8° angle is outdated.
- All 24 labelled envelope points, and the related unlabelled arithmetic, recompute correctly. C5 labels three computed rows, not seven.

## FIX
- **L145 Speeds (Hu 2013, rectilinear).** C5 has 1–6 cm s⁻¹. The author PDF's Results give the gait range twice as **0.2–6 cm s⁻¹ (0.02–0.07 L s⁻¹)** for snakes of 50–180 cm. Methods has "(1–6 cm s⁻¹)", which is the only figure C5 quoted. Use 0.2–6 and keep 1–6 as the Methods figure. Source: https://hu.gatech.edu/Publications/Hu13-rectilinear.pdf

## Source note (no number changes)
- **Sources disagree 5 / S9:** drop "Science version was not opened; values may differ." A Goldman-lab copy of the journal paper plus SI (https://crablab.gatech.edu/pages/publications/pdf/MarviScience2014_ALL.pdf, personal non-commercial use) prints α = 33 ± 8° in the journal text (p. 226) and in SI Fig. S4d. The 33.8° value was not found in the Science paper, the SI or the arXiv version. L159's number stands. The journal and SI ANOVA for this angle differ slightly (F2,51 = 0.06, P = 0.81 vs 0.09, p = 0.77); C5 doesn't use those P values.

## Close checks
1. **Amplitude conventions confirmed.** Di Santo's 0.18 L is peak-to-peak (St = fA/U). Tytell's 0.07 L is half-amplitude (St = 2fA/U). Halve the amplitudes before combining them; don't halve St.
2. **Tytell & Lauder Table 1 "4A/f" is a typo for 4Af.** 0.856 is the mean of the per-beat 4Af values: 4 × 0.0693 × 3.11 = 0.862, whereas 4A/f ≈ 0.089.
3. **Hu 2009:** measured L = 36 ± 2.3 cm; speeds are normalised by L = 30 cm and τ = 2 s.
4. **33 ± 8° is in the Science paper and SI,** not only the arXiv preprint (see the source note).
5. **Jayne & Davis 1991 and Secor 1992:** the JEB pages and PDFs return 403, and there are no PubMed or Europe PMC records. The official Crossref abstracts match C5 in full, so these rows stay VERIFIED (abstract). Per-cycle tables were still not found.
6. **Licences confirmed as C5 states them,** including FishBase CC BY-NC 4.0 (facts only), and Di Santo and Hu under PNAS terms rather than CC.

## UNVERIFIABLE (already marked Unverified in C5; agreed)
- L66 Gillis 1998 original
- L76 Bainbridge 1958
- L77 Videler & Hess 1984
- L151 Newman/Petersen cycle and duty-factor figures

## Links
- No true 404s among C5's cited URLs.
- Blocked from this computer:
  - PMC HTML (recaptcha)
  - official PNAS, Royal Society, Science and JEB PDFs (Cloudflare 403)
  - figshare (403; DataCite 200)
  - the fishbase.org citation (403; fishbase.se 200)
  - Europe PMC fullTextXML (500, except Jayne 2020)
- Author, lab, Crossref, FishBase and DataCite copies work.

## Grok
- One live pass, 09:38:49–09:58:18 AEDT, exit 0.
- It flagged the 0.2–6 range, which was then confirmed in the Hu 2013 PDF.
- It reported the Goldman URL as 404. That PDF was opened independently and used for close check 4.
- No number was taken from Grok alone.
