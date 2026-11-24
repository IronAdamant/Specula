# Second check: R7-2A-quadruped-body-bulk-Q1-Q8.md

Independent adversarial re-check, 2026-11-24. Method: every data row of the file was re-matched number by number against my cached full text of its cited source (PDF text or PMC/Europe PMC XML), by script (`verify.py`), not by memory. Q2 vertebra counts were re-read from the S11 spreadsheet (Supplementary Data 1) with a separate script. Every **computed** ratio was re-done by hand from the cited means. Citations (authors, year, volume, pages, licence) were re-checked against Crossref.

Status key: **VERIFIED** = every number in the row is in the cited source at the cited place; **FIX** = an error was found and corrected in the file (shown); **UNVERIFIABLE** = the full text could not be opened.

## Fixes applied to the file

| Row | Was | Now | Why |
|---|---|---|---|
| Q1 / Q5 wolf female body mass | 30.4 ± 7.0 kg | 30.4 ± 7 kg | S3 Table 1 prints "7"; the extra decimal implied precision the source does not give |
| Q1 wolf index | 55.0 | 55 | as printed in S3 Table 1 |
| Q3 horse cannon (metacarpus) width SD | 0.20 | 0.2 | as printed in S5 Table 1 |
| Q5 cat body mass | given in kg | 5431.3 ± 1174.4 / 4781.3 ± 839.3 g as printed in S4 Table 1, with the kg conversion marked **computed** | the source gives grams |
| Q1 S5 citation | wrong pages in the search summary | pages 25-32 (Crossref) | citation fixed |
| S11 citation | wrong authors in the search summary | Li Y, Brinkworth A, Green E, Oyston J, Wills M, Ruta M (2023), Nat Ecol Evol 7(3): 367-381, CC BY 4.0 (Crossref) | citation fixed |
| S10 authors | incomplete | full author list from Crossref | citation fixed |

## Row-by-row result

| Status | Q | Row (species / measure) | Check note |
|---|---|---|---|
| VERIFIED | Q1 | Dog, Labrador Retriever, males, adult (>2 y), Brazil / 27 | S1: numbers found: ['83.02', '7.06'] |
| VERIFIED | Q1 | same, females / 47 | S1: numbers found: ['79.58', '5.06'] |
| VERIFIED | Q1 | Labrador, M / F / 27 / 47 | S1: numbers found: ['54.94', '3.54', '51.26', '3.03'] |
| VERIFIED | Q1 | Labrador, M / F / 27 / 47 | S1: numbers found: ['65.22', '2.38', '61.70', '2.75'] |
| VERIFIED | Q1 | Labrador, M / F / 27 / 47 | S1: numbers found: ['119.9', '4.88', '111.20', '5.42'] |
| VERIFIED | Q1 | Labrador, M / F / 27 / 47 | S1: numbers found: ['35.77', '4.46', '32.49', '4.30'] |
| VERIFIED | Q1 | Labrador, M / F / 27 / 47 | S1: numbers found: ['26.06', '1.68', '22.96', '2.50'] |
| VERIFIED | Q1 | Labrador, M / F / 27 / 47 | S1: numbers found: ['24.28', '6.17', '21.92', '1.56'] |
| VERIFIED | Q1 | Labrador, M / F / 27 / 47 | S1: numbers found: ['19.78', '2.07', '19.45', '2.38'] |
| VERIFIED | Q1 | Labrador, M / F / 27 / 47 | S1: numbers found: ['50.14', '3.45', '45.89', '3.36'] |
| VERIFIED (computed) | Q1 | Labrador, M / F | ratio re-done from the cited means; same to 2 d.p. |
| VERIFIED (computed) | Q1 | Labrador, M / F | ratio re-done from the cited means; same to 2 d.p. |
| VERIFIED | Q1 | Labrador, 6 M + 6 F, UK; German shepherd 5 M + 7 F / 12 + 12 | S2: numbers found: ['29.3', '4.5', '36.2', '4.1'] |
| VERIFIED | Q1 | Labrador / German shepherd / 12 / 12 | S2: numbers found: ['77.8', '5.0', '86.1', '7.6'] |
| VERIFIED | Q1 | Grey wolf, Bulgaria, adult (>2 y), males / females, dead hunted animals / 60 / 29 | S3: numbers found: ['68.4', '7.8', '46', '81', '64.2', '5.8', '54', '79'] |
| VERIFIED | Q1 | same / 60 / 29 | S3: numbers found: ['125.2', '12.5', '100', '155', '115.1', '11.8', '87', '136'] |
| VERIFIED | Q1 | same / 60 / 29 | S3: numbers found: ['37.8', '7.3', '15', '57', '30.4', '7', '15', '45'] |
| VERIFIED | Q1 | same / 60 / 29 | S3: numbers found: ['100', '55', '7.5', '56.3', '7.2'] |
| VERIFIED (Not found) | Q1 | Wolf chest girth | the cited source has no such value; correctly listed as Not found |
| VERIFIED | Q1 | Domestic cat, mixed-breed, obese at start, Brazil; M / F / 8 / 8 | S4: numbers found: ['8.6', '8.7', '41.3', '3.1', '39.6', '2.4'] |
| VERIFIED | Q1 | same, after 20% weight loss (T2, BCS 6.0-6.7) / 8 / 8 | S4: numbers found: ['35.6', '3.8', '34.0', '3.2'] |
| VERIFIED | Q1 | same, T0 / 8 / 8 | S4: numbers found: ['50.5', '2.1', '47.5', '4.0'] |
| VERIFIED | Q1 | same, T0 / 8 / 8 | S4: numbers found: ['46.5', '4.9', '42.3', '3.3'] |
| VERIFIED | Q1 | same, T0 / T2 / 8 / 8 | S4: numbers found: ['5431.3', '1174.4', '4366.3', '1036.4', '4781.3', '839.3', '3788.8', '769.8'] |
| VERIFIED | Q1 | Horse, Brazilian Sport Horse (warmblood-type), both sexes, in eventing training / 17 | S5: numbers found: ['1.62', '0.034', '1.62'] |
| VERIFIED | Q1 | same / 17 | S5: numbers found: ['1.84', '0.041'] |
| VERIFIED | Q1 | same / 17 | S5: numbers found: ['1.60', '0.062'] |
| VERIFIED | Q1 | same / 17 | S5: numbers found: ['0.42', '0.018'] |
| VERIFIED | Q1 | same / 17 | S5: numbers found: ['0.54', '0.018'] |
| VERIFIED | Q1 | same / 17 | S5: numbers found: ['0.69', '0.018'] |
| VERIFIED | Q1 | same / 17 | S5: numbers found: ['0.86', '0.019'] |
| VERIFIED (computed) | Q1 | same | ratio re-done from the cited means; same to 2 d.p. |
| VERIFIED | Q1 | Cattle, Holstein cows, Italy, lactating / 277 | S6: numbers found: ['142', '0.3', '163', '0.45', '208', '0.61'] |
| VERIFIED (computed) | Q1 | same | ratio re-done from the cited means; same to 2 d.p. |
| VERIFIED | Q1 | Cattle, Holstein cows, Moldova, third lactation; Dutch / German selection / 31 / 51 | S7: numbers found: ['143.4', '0.68', '2.64', '146.0', '0.78', '3.84'] |
| VERIFIED | Q1 | same / 31 / 51 | S7: numbers found: ['76.1', '0.63', '77.4', '0.58'] |
| VERIFIED | Q1 | same / 31 / 51 | S7: numbers found: ['46.5', '0.82', '48.3', '0.71'] |
| VERIFIED | Q1 | same / 31 / 51 | S7: numbers found: ['57.2', '0.67', '58.5', '0.69'] |
| VERIFIED | Q1 | same / 31 / 51 | S7: numbers found: ['171.0', '1.02', '170.4', '1.17'] |
| VERIFIED | Q1 | same / 31 / 51 | S7: numbers found: ['206.3', '1.45', '209.3', '1.34'] |
| VERIFIED (computed) | Q1 | same | ratio re-done from the cited means; same to 2 d.p. |
| UNVERIFIABLE | Q1 | Cattle, Holstein, Italy, lactating / 94 | S8 full text not opened (PMC robot check, not bypassed); number came from a search extract only. Treat as provisional. |
| VERIFIED | Q1 | Pig, Swedish commercial sows, parity 1-10 / 153 | S9: numbers found: ['86.6', '7.53', '74', '133'] |
| VERIFIED | Q1 | same / 153 | S9: numbers found: ['150.3', '15.89', '107', '184'] |
| VERIFIED | Q1 | same / 153 | S9: numbers found: ['191.1', '19.22', '129', '238'] |
| VERIFIED | Q1 | same, parity ≥5 / 62 | S9: numbers found: ['209', '94', '166'] |
| VERIFIED (computed) | Q1 | Pig sows | ratio re-done from the cited means; same to 2 d.p. |
| VERIFIED | Q1 | Asian elephant, semi-captive timber elephants, Myanmar, 9 months to 71 years / 404 (2 | S10: −3636 and −2562 are in the source; the first-pass mismatch was a spacing difference only |
| VERIFIED | Q1 | same | S10: numbers found: ['0.944', '0.873'] |
| VERIFIED | Q2 | Labrador, M / F / 27 / 47 | S1: numbers found: ['31.89', '2.56', '29.31', '3.88'] |
| VERIFIED (computed) | Q2 | Labrador, M / F | ratio re-done from the cited means; same to 2 d.p. |
| VERIFIED (computed) | Q2 | Labrador, M / F | ratio re-done from the cited means; same to 2 d.p. |
| VERIFIED | Q2 | Labrador, M / F / 27 / 47 | S1: numbers found: ['18.43', '1.90', '17.41', '1.54'] |
| VERIFIED | Q2 | Grey wolf, M / F / 60 / 29 | S3: numbers found: ['40.9', '3.2', '31', '49', '38.5', '4.4', '30', '46'] |
| VERIFIED | Q2 | Grey wolf, M / F / 60 / 29 | S3: numbers found: ['100', '32.9', '3.9', '33.8', '5.2'] |
| VERIFIED | Q2 | Horse, Brazilian Sport Horse / 17 | S5: numbers found: ['0.68', '0.042'] |
| VERIFIED (computed) | Q2 | same | ratio re-done from the cited means; same to 2 d.p. |
| VERIFIED | Q2 | Horse, same / 17 | S5: numbers found: ['0.63', '0.021'] |
| VERIFIED | Q2 | *Canis lupus* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Felis catus* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Felis silvestris* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Panthera leo* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Panthera tigris* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Panthera pardus* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Equus caballus* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Equus ferus* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Cervus elaphus* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Odocoileus virginianus* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Bos taurus* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Sus scrofa* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Ursus arctos* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Ursus americanus* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Elephas maximus* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Loxodonta africana* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Oryctolagus cuniculus* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q2 | *Rattus norvegicus* / 7 | C/T/L/presacral re-read from S11 Supplementary Data 1 (xlsx); exact match, one row per species (the first-pass script failed only on the underscore in the species names) |
| VERIFIED | Q3 | Labrador / German shepherd / 12 / 12 | S2: numbers found: ['30.52', '2.40', '26.04', '3.01'] |
| VERIFIED | Q3 | same / 12 / 12 | S2: numbers found: ['36.95', '2.37', '34.40', '1.72'] |
| VERIFIED | Q3 | same / 12 / 12 | S2: numbers found: ['33.6', '2.8', '30.7', '2.3'] |
| VERIFIED | Q3 | same | S2: numbers found: ['5'] |
| VERIFIED | Q3 | Labrador, M / F / 27 / 47 | S1: numbers found: ['14.96', '0.82', '13.99', '0.73'] |
| VERIFIED | Q3 | Horse / 17 | S5: numbers found: ['0.2', '0.01'] |
| VERIFIED | Q4 | Labrador, M / F / 27 / 47 | S1: numbers found: ['24.15', '1.40', '22.40', '0.95'] |
| VERIFIED | Q4 | Labrador, M / F / 27 / 47 | S1: numbers found: ['13.12', '0.92', '11.90', '0.74'] |
| VERIFIED | Q4 | Labrador, M / F / 27 / 47 | S1: numbers found: ['9.40', '0.74', '8.91', '0.58'] |
| VERIFIED (computed) | Q4 | Labrador, M / F | ratio re-done from the cited means; same to 2 d.p. |
| VERIFIED | Q4 | Labrador, M / F / 27 / 47 | S1: numbers found: ['29.19', '1.55', '25.86', '1.37'] |
| VERIFIED | Q4 | Labrador, M / F / 27 / 47 | S1: numbers found: ['13.71', '1.18', '12.87', '1.36'] |
| VERIFIED | Q4 | Labrador, M / F / 27 / 47 | S1: numbers found: ['10.54', '2.06', '10.36', '1.13'] |
| VERIFIED | Q4 | Horse / 17 | S5: numbers found: ['0.63', '0.021', '0.22', '0.012'] |
| VERIFIED | Q5 | Grey wolf, Bulgaria, adult M / F / 60 / 29 | S3: numbers found: ['37.8', '7.3', '30.4', '7'] |
| VERIFIED | Q5 | Labrador, UK, 6 M + 6 F / 12 | S2: numbers found: ['29.3', '4.5'] |
| VERIFIED | Q5 | German shepherd, UK, 5 M + 7 F / 12 | S2: numbers found: ['36.2', '4.1'] |
| VERIFIED (computed) | Q5 | Domestic cat (obese at start) M / F / 8 / 8 | ratio re-done from the cited means; same to 2 d.p. |
| UNVERIFIABLE | Q5 | Holstein cows, Italy / 94 | S8 full text not opened (PMC robot check, not bypassed); number came from a search extract only. Treat as provisional. |
| VERIFIED | Q6 | Labrador Retriever / 12 | S2: numbers found: ['73.2', '8.5'] |
| VERIFIED | Q6 | German shepherd / 12 | S2: numbers found: ['59.8', '9.0'] |
| VERIFIED | Q6 | Labrador / 12 | S2: numbers found: ['29.6', '12.2'] |
| VERIFIED | Q6 | German shepherd / 12 | S2: numbers found: ['−9.6', '10.7'] |
| VERIFIED | Q6 | Labrador / 12 | S2: numbers found: ['31.5', '15.3', '34', '10'] |
| VERIFIED | Q6 | German shepherd / 12 | S2: numbers found: ['44.2', '8.4', '44'] |
| VERIFIED | Q6 | Labrador / 12 | S2: numbers found: ['5', '26.3', '9.1'] |
| VERIFIED | Q6 | German shepherd / 12 | S2: numbers found: ['57.5', '10.8'] |
| VERIFIED | Q6 | Labrador / GSD / 12 / 12 | S2: numbers found: ['66.2', '9.3', '78.8', '7.9'] |
| VERIFIED | Q6 | Labrador / GSD / 12 / 12 | S2: numbers found: ['4'] |
| VERIFIED | Q6 | Horse, Brazilian Sport Horse / 17 | S5: numbers found: ['70.82', '3.557', '70'] |
| VERIFIED | Q6 | same / 17 | S5: numbers found: ['99.53', '5.58', '100'] |
| VERIFIED | Q6 | same / 17 | S5: numbers found: ['145.41', '5.432', '145'] |
| VERIFIED | Q6 | same / 17 | S5: numbers found: ['151.24', '6.17', '151'] |
| VERIFIED | Q6 | same / 17 | S5: numbers found: ['27.06', '3.363', '26'] |
| VERIFIED | Q6 | same / 17 | S5: numbers found: ['77.47', '5.535', '80'] |
| VERIFIED | Q6 | same / 17 | S5: numbers found: ['109.18', '4.613', '108'] |
| VERIFIED | Q6 | same / 17 | S5: numbers found: ['147.18', '3.245', '148'] |

## Totals

- VERIFIED: 99 rows
- VERIFIED (computed): 11 rows
- VERIFIED (Not found): 1 rows
- UNVERIFIABLE: 2 rows

Q7 (joint ranges) and Q8 (footfall timing) have no data rows: Q7 was not found this round and Q8 was not attempted, as the file says.

## Left open by this check
- S8 (Magro et al. 2026, *Animals* 16(6): 938): open the full text when PMC allows it, to confirm the n = 94 and the heart-girth figures.
- Q2 horse, pig and bear vertebra rows match the S11 spreadsheet exactly, so they are not transcription errors. The file's "Sources disagree" list flags them as suspect against usual veterinary counts; those counts were not checked against a primary source this round.
