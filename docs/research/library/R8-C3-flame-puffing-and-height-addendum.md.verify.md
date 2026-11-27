# Second check: R8-C3-flame-puffing-and-height-addendum.md

Re-read against raw/round8/fds-validation-guide-6.11.2.flow.txt, hal-04338538.flow.txt and cetegen93-osti.json, and against round 3's J11 file to confirm the "missing" list.

| row | claim | source page | result |
|---|---|---|---|
| 1 | J11 has no puffing frequency and no Q* form | grep of J11 for puff, flicker, frequency, Q* | VERIFIED |
| 2 | L1 citation (journal, volume, pages, DOI) | OSTI record; search result DOI | VERIFIED |
| 3 | L1 abstract: helium at 1.10 m and 0.30 m, D^(-1/2), dilution, vortex mechanism | OSTI abstract | VERIFIED |
| 4 | 1.5/√D attributed to Cetegen-Ahmed, 1.5 Hz at 1 m | S2 PDF 17-18 (printed 16-17) | VERIFIED |
| 5 | S2 citation and HAL licence | S2 PDF 1; HAL API licence_s | VERIFIED |
| 6 | S1 edition, revision FDS-6.11.2, date Nov 14 2026, DOI | S1 PDF 1, 3 | VERIFIED |
| 7 | Zukoski f = 0.50 √(g/D), Eq. 6.7 | S1 PDF 403 (printed 347) | VERIFIED |
| 8 | My conversion to 1.566/√D | my arithmetic | VERIFIED |
| 9 | Sandia Test 17 1.65 Hz | S1 PDF 413 (printed 357) | VERIFIED |
| 10 | Helium 1.37 ± 0.1 Hz; Cetegen-Kasper 0.8 Ri^0.38 U/D = 1.34 Hz; Ri about 76; Ri < 100 | S2 PDF 17 | VERIFIED |
| 11 | Coarse 1.69 Hz against medium and fine 1.39 Hz | S2 PDF 17 | VERIFIED |
| 12 | Table C3-1a values | my arithmetic (python) | VERIFIED |
| 13 | L_f/D = 3.7 Q*^(2/5) − 1.02 and the Q* definition, Eq. 3.3 | S1 PDF 108 (printed 52) | VERIFIED |
| 14 | Eq. 3.39 and the Froude-number wording | S1 PDF 222-223 (printed 166-167) | VERIFIED |
| 15 | 50% luminous flame against 95-99% heat release | S1 PDF 397 (printed 341) | VERIFIED |
| 16 | "Reported variation is significant, especially at low Q*" | S1 PDF 397-398 | VERIFIED |
| 17 | Table 3.56 Heskestad row: 10^2 to 10^7 kW, D 1.1 m, Q* 10^-1 to 10^4 | S1 PDF 224 | VERIFIED (read from a scrambled text layer; numbers match the LaTeX source read before the box refresh) |
| 18 | 3.7 corresponds to 0.224-0.225; 0.235 corresponds to 3.87-3.88 | my arithmetic (python) | VERIFIED |
| 19 | NUREG-1805 download blocked | raw/round8/nureg1805-ch3*.FAILED-not-a-pdf.html | VERIFIED |

No FIX rows.
