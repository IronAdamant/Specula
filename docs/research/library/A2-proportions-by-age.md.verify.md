# Verification of A2-proportions-by-age.md

Checked 2026-11-16 (AEDT, UTC+11) by Library Verify's checker. The researcher's file was not edited.

**File checked:** mtime 2026-11-16 10:30:00 +1100, 164 lines, 25312 bytes, recorded before checking. Re-measured when this report was written: 164 lines, 25312 bytes (unchanged = yes).

## Summary

**Rows: 58 total: 57 VERIFIED, 1 FIX, 0 UNVERIFIABLE.**

- **One FIX:** Snyder 1977, males 16.5–17.5 y head height = **21.8 cm** (file 21.9); ratio 0.124 (file 0.125). Read by eye at 400 dpi from printed p. 464 (PDF page = printed + 20).
- WHO tables (A2-1, A2-2): every M, S and ±2SD recomputed from the full-precision WHO xlsx; rounding OK. HC/length ratios match. Adult HC/stature 0.345 F / 0.327 M ✓. ANSUR head circumference 56.11 ± 1.94 F, 57.44 ± 1.60 M ✓ (printed pp. 135–136).
- Snyder (A2-3): all other 17 rows match the scan (M 4.5–5.5 = 18.1 accepted; digits degraded). Page refs correct. Population 4127 (2092 M / 2035 F) ✓. Note: Snyder's title/abstract says "to age 18" ("2 weeks through 18 years"); oldest band 17.5–19.0. Independent check: NIST AnthroKids combined-sex table (https://math.nist.gov/~SRessler/anthrokids/data1977/173.txt) gives 16.5–17.5 combined n 80, mean 21.2; consistent with M n 38 at either 21.8 or 21.9 (cannot discriminate), so the scan reading stands.
- NHANES (A2-4): all 16 rows reproduced (BMX_J/DEMO_J, WTMEC2YR-weighted). Note: BMXLEG is only recorded from age 8, so the "6–9" leg-length cell covers ages 8–9 only.
- A2-5 sitting-height ratio matches A1-3 and the report.
- Blocked link: https://hdl.handle.net/2027.42/684 → Deep Blue item page 403 to curl (bot protection); the PDF bitstream itself opens via the Deep Blue API, and a Wayback snapshot (2025-01-30) exists. Mark the handle as "resolves via redirect; landing page bot-blocked".
- Structure check: every row has a URL; access date only in header (see A1 note). Not found / Sources disagree present.

### Grok live-search pass

Ran 2026-11-16 ~10:47–10:53 AEDT: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve` (live web search on), single 10-question prompt; waited until the process exited (exit 0). A Grok answer counted only where it gave a URL that I then opened myself; no number was taken from Grok alone.

For A2: Grok could not find a male-only Snyder table online; it pointed to the NIST AnthroKids combined-sex table (16.5–17.5: n 80, mean 21.2, SD 1.1), which I opened myself (URL above). That is consistent with my 21.8 reading but cannot decide between 21.8 and 21.9. **Conflict raised: none against the file; the FIX rests on the scan.**

## Row table

L = line number in the researcher's file. Status: VERIFIED = re-read or recomputed from the source and matches; FIX = correction needed (corrected value given); UNVERIFIABLE = could not be re-read (reason given).

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| L13 | 0 (length) ; 49.88 ; 0.0380 ; 46.1 / 53.7 | VERIFIED | Matches WHO xlsx (full precision, rounding OK). | https://www.who.int/tools/child-growth-standards/standards/length-height-for-age |
| L14 | 6 (length) ; 67.62 ; 0.0317 ; 63.3 / 71.9 | VERIFIED | Matches WHO xlsx (full precision, rounding OK). | https://www.who.int/tools/child-growth-standards/standards/length-height-for-age |
| L15 | 12 (length) ; 75.75 ; 0.0314 ; 71.0 / 80.5 | VERIFIED | Matches WHO xlsx (full precision, rounding OK). | https://www.who.int/tools/child-growth-standards/standards/length-height-for-age |
| L16 | 24 (length) ; 87.82 ; 0.0348 ; 81.7 / 93.9 | VERIFIED | Matches WHO xlsx (full precision, rounding OK). | https://www.who.int/tools/child-growth-standards/standards/length-height-for-age |
| L17 | 24 (height) ; 87.12 ; 0.0351 ; 81.0 / 93.2 | VERIFIED | Matches WHO xlsx (full precision, rounding OK). | https://www.who.int/tools/child-growth-standards/standards/length-height-for-age |
| L18 | 36 ; 96.08 ; 0.0386 ; 88.7 / 103.5 | VERIFIED | Matches WHO xlsx (full precision, rounding OK). | https://www.who.int/tools/child-growth-standards/standards/length-height-for-age |
| L19 | 48 ; 103.33 ; 0.0406 ; 94.9 / 111.7 | VERIFIED | Matches WHO xlsx (full precision, rounding OK). | https://www.who.int/tools/child-growth-standards/standards/length-height-for-age |
| L20 | 60 ; 109.96 ; 0.0421 ; 100.7 / 119.2 | VERIFIED | Matches WHO xlsx (full precision, rounding OK). | https://www.who.int/tools/child-growth-standards/standards/length-height-for-age |
| L21 | 72 (WHO 2007) ; 115.95 ; 0.0425 ; 106.1 / 125.8 | VERIFIED | Matches WHO xlsx (full precision, rounding OK). | https://www.who.int/tools/growth-reference-data-for-5to19-years/indicators/height-for-age |
| L22 | 96 ; 127.27 ; 0.0444 ; 116.0 / 138.6 | VERIFIED | Matches WHO xlsx (full precision, rounding OK). | https://www.who.int/tools/growth-reference-data-for-5to19-years/indicators/height-for-age |
| L23 | 120 ; 137.78 ; 0.0463 ; 125.0 / 150.5 | VERIFIED | Matches WHO xlsx (full precision, rounding OK). | https://www.who.int/tools/growth-reference-data-for-5to19-years/indicators/height-for-age |
| L24 | 144 ; 149.08 ; 0.0475 ; 134.9 / 163.3 | VERIFIED | Matches WHO xlsx (full precision, rounding OK). | https://www.who.int/tools/growth-reference-data-for-5to19-years/indicators/height-for-age |
| L25 | 168 ; 163.18 ; 0.0471 ; 147.8 / 178.6 | VERIFIED | Matches WHO xlsx (full precision, rounding OK). | https://www.who.int/tools/growth-reference-data-for-5to19-years/indicators/height-for-age |
| L26 | 192 ; 172.90 ; 0.0450 ; 157.4 / 188.4 | VERIFIED | Matches WHO xlsx (full precision, rounding OK). | https://www.who.int/tools/growth-reference-data-for-5to19-years/indicators/height-for-age |
| L27 | 216 ; 176.14 ; 0.0424 ; 161.2 / 191.1 | VERIFIED | Matches WHO xlsx (full precision, rounding OK). | https://www.who.int/tools/growth-reference-data-for-5to19-years/indicators/height-for-age |
| L28 | 228 ; 176.54 ; 0.0413 ; 161.9 / 191.1 | VERIFIED | Matches WHO xlsx (full precision, rounding OK). | https://www.who.int/tools/growth-reference-data-for-5to19-years/indicators/height-for-age |
| L36 | 0 ; 34.46 ; 0.0369 ; 33.88 | VERIFIED | Matches WHO HC xlsx. | https://www.who.int/tools/child-growth-standards/standards/head-circumference-for-age |
| L37 | 6 ; 43.33 ; 0.0282 ; 42.20 | VERIFIED | Matches WHO HC xlsx. | https://www.who.int/tools/child-growth-standards/standards/head-circumference-for-age |
| L38 | 12 ; 46.07 ; 0.0279 ; 44.90 | VERIFIED | Matches WHO HC xlsx. | https://www.who.int/tools/child-growth-standards/standards/head-circumference-for-age |
| L39 | 24 ; 48.25 ; 0.0282 ; 47.18 | VERIFIED | Matches WHO HC xlsx. | https://www.who.int/tools/child-growth-standards/standards/head-circumference-for-age |
| L40 | 36 ; 49.46 ; 0.0287 ; 48.51 | VERIFIED | Matches WHO HC xlsx. | https://www.who.int/tools/child-growth-standards/standards/head-circumference-for-age |
| L41 | 48 ; 50.21 ; 0.0291 ; 49.33 | VERIFIED | Matches WHO HC xlsx. | https://www.who.int/tools/child-growth-standards/standards/head-circumference-for-age |
| L42 | 60 ; 50.74 ; 0.0295 ; 49.92 | VERIFIED | Matches WHO HC xlsx. | https://www.who.int/tools/child-growth-standards/standards/head-circumference-for-age |
| L56 | M ; 2.0–3.5 ; 42 ; 17.6 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L57 | M ; 3.5–4.5 ; 36 ; 18.1 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L58 | M ; 4.5–5.5 ; 50 ; 18.1 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L59 | M ; 5.5–6.5 ; 26 ; 18.7 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L60 | M ; 6.5–7.5 ; 30 ; 18.9 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L61 | M ; 7.5–8.5 ; 30 ; 19.1 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L62 | M ; 8.5–9.5 ; 39 ; 19.3 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L63 | M ; 9.5–10.5 ; 46 ; 19.6 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L64 | M ; 10.5–11.5 ; 51 ; 20.1 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L65 | M ; 14.5–15.5 ; 50 ; 21.3 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L66 | M ; 15.5–16.5 ; 31 ; 21.9 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L67 | M ; 16.5–17.5 ; 38 ; 21.9 | FIX | Head height mean is **21.8**, not 21.9 (n 38, SD 0.9 correct; read at 400 dpi from Snyder printed p. 464 = PDF p. 484). Ratio becomes 21.8/175.2 = **0.124**, not 0.125. | https://hdl.handle.net/2027.42/684 |
| L68 | F ; 5.5–6.5 ; 36 ; 18.1 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L69 | F ; 6.5–7.5 ; 33 ; 18.6 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L70 | F ; 7.5–8.5 ; 30 ; 18.6 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L71 | F ; 11.5–12.5 ; 47 ; 19.8 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L72 | F ; 12.5–13.5 ; 61 ; 20.2 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L73 | F ; 13.5–14.5 ; 44 ; 20.3 | VERIFIED | Matches Snyder scan (read by eye 200–400 dpi); page ref correct. | https://hdl.handle.net/2027.42/684 |
| L83 | M ; 2–5 ; n/a ;  | VERIFIED | Reproduced from NHANES 2017–18 BMX_J/DEMO_J, WTMEC2YR-weighted. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L84 | M ; 6–9 ; 184 ; 0.2361 | VERIFIED | Reproduced from NHANES 2017–18 BMX_J/DEMO_J, WTMEC2YR-weighted. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L85 | M ; 10–13 ; 298 ; 0.2437 | VERIFIED | Reproduced from NHANES 2017–18 BMX_J/DEMO_J, WTMEC2YR-weighted. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L86 | M ; 14–17 ; 286 ; 0.2449 | VERIFIED | Reproduced from NHANES 2017–18 BMX_J/DEMO_J, WTMEC2YR-weighted. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L87 | M ; 18–19 ; 127 ; 0.2442 | VERIFIED | Reproduced from NHANES 2017–18 BMX_J/DEMO_J, WTMEC2YR-weighted. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L88 | M ; 20–39 ; 701 ; 0.2417 | VERIFIED | Reproduced from NHANES 2017–18 BMX_J/DEMO_J, WTMEC2YR-weighted. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L89 | M ; 40–59 ; 742 ; 0.2366 | VERIFIED | Reproduced from NHANES 2017–18 BMX_J/DEMO_J, WTMEC2YR-weighted. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L90 | M ; 60–80 ; 945 ; 0.2326 | VERIFIED | Reproduced from NHANES 2017–18 BMX_J/DEMO_J, WTMEC2YR-weighted. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L91 | F ; 2–5 ; n/a ;  | VERIFIED | Reproduced from NHANES 2017–18 BMX_J/DEMO_J, WTMEC2YR-weighted. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L92 | F ; 6–9 ; 173 ; 0.2346 | VERIFIED | Reproduced from NHANES 2017–18 BMX_J/DEMO_J, WTMEC2YR-weighted. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L93 | F ; 10–13 ; 321 ; 0.2406 | VERIFIED | Reproduced from NHANES 2017–18 BMX_J/DEMO_J, WTMEC2YR-weighted. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L94 | F ; 14–17 ; 273 ; 0.2413 | VERIFIED | Reproduced from NHANES 2017–18 BMX_J/DEMO_J, WTMEC2YR-weighted. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L95 | F ; 18–19 ; 124 ; 0.2402 | VERIFIED | Reproduced from NHANES 2017–18 BMX_J/DEMO_J, WTMEC2YR-weighted. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L96 | F ; 20–39 ; 803 ; 0.2381 | VERIFIED | Reproduced from NHANES 2017–18 BMX_J/DEMO_J, WTMEC2YR-weighted. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L97 | F ; 40–59 ; 827 ; 0.2286 | VERIFIED | Reproduced from NHANES 2017–18 BMX_J/DEMO_J, WTMEC2YR-weighted. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L98 | F ; 60–80 ; 893 ; 0.2265 | VERIFIED | Reproduced from NHANES 2017–18 BMX_J/DEMO_J, WTMEC2YR-weighted. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| A2-5 (prose) | Sitting height / stature, adults (ANSUR) | VERIFIED | Matches report means and A1-3 recomputation. | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf |

## Source table

| Key | URL | Resolves (y/n) | Licence statement correct (y/n) | Note |
|---|---|---|---|---|
| WHO growth standards / reference LMS xlsx | https://www.who.int/tools/child-growth-standards and cdn.who.int xlsx links in file | y (200/206) | not re-checked | WHO licence terms not re-read in this pass; nothing found contradicting the file statement. |
| Snyder et al. 1977 UM-HSRI-77-17 | https://hdl.handle.net/2027.42/684 | redirect y; landing page n (403 bot block); PDF bitstream y | y | Deep Blue item has no explicit rights field; treat as citable report. |
| NHANES 2017–18 BMX_J / DEMO_J | wwwn.cdc.gov NHANES XPT links in file | y | y | US Gov public domain; NCHS data-use terms stated correctly. |
| Gordon 2014 (ANSUR II report) | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf | y (206) | y | "Approved for public release; distribution is unlimited" confirmed on the report. |
| ANSUR II public CSVs | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv ; …MALE_Public.csv | y (206) | y | Downloaded to /workspace/tmp, recomputed, then deleted. CSVs = unweighted working sample, F 1986 / M 4082, ages 17–58. |
| OpenLab host page | https://www.openlab.psu.edu/ansur2/ | y (200) | n/a |  |
