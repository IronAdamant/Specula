# Second check: R7-00-dryad-DS1-DS3.md

Independent re-check, 2026-11-24. Every table was recomputed from the raw files with new code, written separately from the code that built the tables. Metadata was re-read from the saved Dryad API JSON, live Zenodo and Crossref API calls, and the saved PMC text. No sign-in, no account, no captcha.

| Status | Item | What was re-checked | Result |
|---|---|---|---|
| VERIFIED | Access table (401 / 403 / openapi.yml / ORCID route) | saved API responses, openapi.yml and the login-page text in work/ | as stated |
| VERIFIED | DS1 licence, size | API JSON: `license` = CC0-1.0, storageSize 26,144,481; file list has 7 files; the per-file sizes add up to 26,144,481 (computed) | match |
| VERIFIED | DS1 counts (246 / 177 species, 1,883 skull values, 793 / 805 / 668 / 663 bone values) | each number found in the Dryad API methods text | match |
| VERIFIED | DS1 paper citation and CC BY 4.0 | Crossref | match |
| VERIFIED | DS2 licence, size | API JSON: CC0-1.0, storageSize 4,982,436; 4 files; the sizes add up (computed) | match |
| VERIFIED | DS2 paper: *Sci Data* 12: 884, CC BY-NC-ND 4.0 | live Crossref call | match |
| VERIFIED | DS2 mirror licence GPL-3.0 | LICENSE file of bcweeks/Trait-Imputation | match |
| VERIFIED | DS2/Q12 table, 7 species x 8 bones = 56 cells, plus n specimens | recomputed: exp(ln mm), sample SD, per-bone n, from `filtered_dataset` | **0 mismatches** |
| VERIFIED (added) | *C. corax* medians (new sentence added during this check, **computed**) | same data | added to give a robust centre beside the inflated SDs |
| VERIFIED | DS3 licence, size | API JSON: CC0-1.0, storageSize 77,531,646; 11 files | match |
| VERIFIED | Zenodo 4623882 AFL-3.0, dat.mat 229,926 B; Zenodo 5114877 CC BY 4.0 | live Zenodo API | match |
| VERIFIED | dat.mat holds 151 clips, 44 species; fields species / swimmingmode / ampenv / clipid | reloaded | match (no speed field, as stated) |
| VERIFIED | DS3/Q25 table, 44 species: n clips, head / mid / tail amplitude (132 cells), head:tail ratio, mode code | recomputed | **0 mismatches** |
| VERIFIED | Paper wavelengths eel 0.58, trout 1.00, mackerel 0.96, tuna 1.17 BL, n = 5 each, about 1 BL/s | PMC8670443 text: "(0.58 BL) than that of the trout, mackerel, or tuna (1.00, 0.96, and 1.17 BL, respectively ...)" | match |
| VERIFIED | Peak-to-peak convention, BL units | Kinematics.rmd (`amplitude = maxy - miny`) and the paper text; BL is inferred from the code's axis label, and the file says so | as stated |
| UNVERIFIABLE | Dataset S2 contents ("species-specific kinematic parameters") | only the paper's description; the file is behind reCAPTCHA | description only |
| UNVERIFIABLE | Whether a human browser download needs only the "confirm you are human" step | not tested end to end (captcha not solved, by rule) | stated as unconfirmed in the file |

No errors found. One addition: the raven medians.
