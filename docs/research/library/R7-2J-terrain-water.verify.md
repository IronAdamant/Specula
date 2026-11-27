# R7-2J Terrain and water: adversarial second check

Checked 2026-11-24. Method: re-ran the splib07a extraction script on the original ASCII files (both the spectrum and the matching wavelength file, length-checked); re-read the J2 Cambridge Core text capture and the J3 PDF text layer for each quoted phrase; checked citations on Crossref.

| Q | Row checked | Result | Evidence / action |
|---|---|---|---|
| Q44 | J1 citation: 11 authors, Data Series 1035, 2017, doi:10.3133/ds1035; public domain | VERIFIED | Crossref author list and year |
| Q44 | 15 splib07a rows, values at 450/550/650 nm and VIS (computed) | VERIFIED | re-run: identical to 3 decimals; record numbers read from each file's header line |
| Q44 | error bars for DWO sands and basalts | VERIFIED | values from the splib07a "errorbars_for_..." files at the same wavelengths |
| Q44 | Grand Isle sand described as "beach sand ... Louisiana" | FIX applied | the file name says only "Sand GrndIsle1 no oil"; the library's sample description was not opened, so "beach" and "Louisiana" were removed |
| Q44 | GDS136 ice has no visible data | VERIFIED | all values below 0.7 µm are the library's no-data flag |
| Q44 | consistency with the earlier F3 file (band means) | VERIFIED | F3's band means (blue 0.45-0.50, etc.) differ slightly from these point values, as expected; e.g. DWO dry sand VIS 0.209 in both |
| Q44 | wet/dry ratio 0.53 (computed) | VERIFIED | 0.111 / 0.209 = 0.531 |
| Q44 | J2 citation 46(155):675-688; doi | VERIFIED | Crossref |
| Q44 | J2 instrument, 0.3-2.8 µm, ±2%, < ±0.02 within 3 h | VERIFIED | J2 Methods text |
| Q44 | J2 snow about 0.80 late May; 0.40-0.60 July-August; fresh snowfall about 0.80; < 0.40 patchy/old dirty snow | VERIFIED | J2 Results items 5, 7, 9 |
| Q44 | J2 ice "generally < 0.30"; change of about 0.10 | VERIFIED | J2 Discussion; Results item 8 |
| Q44 | J2 literature range < 0.10 to > 0.90 | VERIFIED | J2 Introduction |
| Q44 | search-summary means 0.52 / 0.18 / 0.56 | correctly excluded | not present in the readable text (tables are images) |
| Q45 | Musgrave 1989 and Cordonnier 2016 citations | VERIFIED; FIX applied | Crossref: ACM SIGGRAPH Computer Graphics 23(3):41-50; CGF 35(2):165-175, doi:10.1111/cgf.12820 added |
| Q45 | Mei et al. 2007 citation | UNVERIFIABLE | HAL bot check; left labelled as unchecked |
| Q46 | Solonenko and Mobley 2015 citation | VERIFIED; FIX applied | Crossref gives first page 5392 only; I had written an end page from memory, removed |
| Q48 | J3 citation: Simons and Richardson 1966, PP 422-J, doi:10.3133/pp422J | VERIFIED | Crossref |
| Q48 | glossary definitions of lower and upper flow regime | VERIFIED | J3 glossary text |
| Q48 | F > 1 antidunes, F < 1 dunes | VERIFIED | J3 p. J9 |
| Q48 | C/√g 7.0-13.2, f 0.04-0.16; f 0.02-0.07, C/√g 10.7-20 | VERIFIED | J3 abstract/summary text (OCR shows "C/Vg" for C/√g) |
| Q48 | L = 2πV²/g; break at about 0.14 | VERIFIED | J3 p. J9 (Kennedy 1961 conclusions 1 and 4) |
| Q48 | ripples 0.4-2 ft long, 0.02-0.2 ft high; not coarser than 0.6 mm | VERIFIED | J3 "Ripples" and Summary |
| Q48 | dunes 2.0 to > 100 ft long, 0.2 to > 10 ft high | VERIFIED | J3 Summary |
| Q48 | bed materials 0.19 to 0.93 mm | VERIFIED; wording FIX | J3: "The median fall diameter of the bed material ranged from 0.19 mm to 0.93 mm" |
| Q48 | metric conversions (computed) | VERIFIED | 0.4 ft = 0.122 m, 2 ft = 0.610 m, 0.02 ft = 6.1 mm, 0.2 ft = 61 mm, 100 ft = 30.5 m, 10 ft = 3.05 m |

Summary: 25 rows checked; 22 VERIFIED (or correctly excluded), 1 UNVERIFIABLE (Mei 2007 citation); 5 FIXes applied.
