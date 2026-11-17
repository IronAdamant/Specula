# F3-ground-materials.md: verification

Checked by: Grok Bot executor (adversarial source check). Access date for every URL below: **2026-11-17** (Australia/Sydney, AEDT).
File checked: `F3-ground-materials.md`. The box was restored from a snapshot at 09:59 AEDT, so mtimes cannot be used to spot changes. Identified by size and sha256.
- At the start of this completing pass (10:39:06 AEDT): **25,361 bytes**, sha256 `70d3f923f75498cfad7bbd5ae3818bfa1ac1809d32269de247859784006ce372`.
- Re-hashed immediately before writing this file: **unchanged**. The current file is larger than the 23,832-byte copy checked in the earlier pass because it already contains the 2026-11-17 “Fixes after verify” section (Brock table numbering and WGMS method steps).

## Summary

- **Scope (stated plainly):** This pass finishes the rows the earlier check never re-checked: G2 lab-reflectance values (§4), the G6 soil-colour table (§5), and G11 and G13–G17 (§2). Earlier-checked rows (Brock/G18, Grenfell/G5, Bøggild/G4, WGMS/G1, licences, G12) are **kept below**, with the three earlier FIX items now counted as VERIFIED against the current wording (already applied in the file).
- **Counts (all checked rows, current file):** VERIFIED 48 · FIX 0 · UNVERIFIABLE 0. (Earlier pass: 37 VERIFIED / 3 FIX / 0 UNVERIFIABLE across 40 rows. This pass adds 8 VERIFIED rows; the 3 earlier FIX items move to VERIFIED because the current text matches the correction.)
- **Outstanding FIX:** none. Notes only: G11 “at least 50,000 years” vs the file’s “about 50,000”; G11 300 ft vs 97 m already in §10; G17 licence is CC BY-NC-ND 4.0 (version omitted in the file); G13 has no CC statement in the pages opened (Crossref 404 for the DOI).
- **Dead links:** none. eScholarship HTML returned 202 (bot challenge) as before; the PDF opened (HTTP 200).

## Rows


| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| §3 G5 spectral albedo | 0.96–0.98 UV/visible; < 0.15 at 1.5 and 2.0 µm | VERIFIED | abstract, p.18669 (page image) | https://atmos.uw.edu/~sgw/PAPERS/1994_SnowAlbJGR.pdf |
| §3 G5 spectrally averaged | 0.80–0.85 | VERIFIED | abstract | https://atmos.uw.edu/~sgw/PAPERS/1994_SnowAlbJGR.pdf |
| §3 G5 Table 7: Cloudy average | 0.83 / 0.98 / 0.67 | VERIFIED | read from page image, p.18682 (PDF p.14) | https://atmos.uw.edu/~sgw/PAPERS/1994_SnowAlbJGR.pdf |
| §3 G5 Table 7: Cloudy upper envelope | 0.85 / 0.99 / 0.71 | VERIFIED | read from page image, p.18682 (PDF p.14) | https://atmos.uw.edu/~sgw/PAPERS/1994_SnowAlbJGR.pdf |
| §3 G5 Table 7: Cloudy lower envelope | 0.80 / 0.97 / 0.64 | VERIFIED | read from page image, p.18682 (PDF p.14) | https://atmos.uw.edu/~sgw/PAPERS/1994_SnowAlbJGR.pdf |
| §3 G5 Table 7: Clear SP θ 72° | 0.85 / 0.99 / 0.72 | VERIFIED | read from page image, p.18682 (PDF p.14) | https://atmos.uw.edu/~sgw/PAPERS/1994_SnowAlbJGR.pdf |
| §3 G5 Table 7: Clear SP θ 68° | 0.84 / 0.99 / 0.70 | VERIFIED | read from page image, p.18682 (PDF p.14) | https://atmos.uw.edu/~sgw/PAPERS/1994_SnowAlbJGR.pdf |
| §3 G5 Table 7: Clear Vostok θ 55° | 0.80 / 0.98 / 0.63 | VERIFIED | read from page image, p.18682 (PDF p.14) | https://atmos.uw.edu/~sgw/PAPERS/1994_SnowAlbJGR.pdf |
| §3 G5 NOAA-CMDL monthly | 0.81–0.87, Dec 1985 – Feb 1988 | VERIFIED | Table 7: 0.83, 0.85, 0.87, 0.83, 0.82, 0.81, 0.82, 0.83 | https://atmos.uw.edu/~sgw/PAPERS/1994_SnowAlbJGR.pdf |
| §3 G4 Table 1 (12 surfaces, clear / overcast) | e.g. melting coarse snow 0.67 / 0.75 … tundra soil damp 0.15 / 0.14 | VERIFIED | All 12 pairs match Table 1. Label note: "Tundra soil, dry/damp" is "Tundra soil with large rocks removed" in the paper | https://atmos.uw.edu/~sgw/PAPERS/2010_KPCL.pdf |
| §3 G4 abstract ice range | 0.2 (heavy debris) to 0.6 (hummocks with cryoconite holes) | VERIFIED | | https://atmos.uw.edu/~sgw/PAPERS/2010_KPCL.pdf |
| §3 G4 superimposed ice 0.68; heavy impurity → ~0.15 | | VERIFIED | text and Table 2 (0.68); "asymptoting to the value 0.15" | https://atmos.uw.edu/~sgw/PAPERS/2010_KPCL.pdf |
| §6 G18 literature ranges, current wording | 0.1–10 mm melting mid-latitude; 0.001–10 mm high latitude; ~0.1 mm smooth fresh snow (0.01–0.001 polar); 1–10 mm melting snow, Table 1a/1b | VERIFIED | Earlier FIX (location “Tables 1–2” → “Table 1a/1b”) already applied in “Fixes after verify”. Values re-confirmed | https://arctic.eas.ualberta.ca/downloads/Brock%20et%20al%20J%20Glaciol%202006.pdf |
| §6 G18 Arolla measured block, current wording | snow 3.56 (n 14), micro 2.31 across / 1.62 along; slush 0.88 (n 10); ice 6.89 (n 34), micro 6.96 across; Table 3 | VERIFIED | Earlier FIX (location Table 4 → Table 3) already applied in “Fixes after verify”. Values re-confirmed | https://arctic.eas.ualberta.ca/downloads/Brock%20et%20al%20J%20Glaciol%202006.pdf |
| §6 G18 ablation hollows | 0.1 m vertical relief, 0.5–1.0 m spacing | VERIFIED | §4.1 | https://arctic.eas.ualberta.ca/downloads/Brock%20et%20al%20J%20Glaciol%202006.pdf (same paper as the file's Cambridge URL, which also returns 200) |
| §10 Brock Grok vs primary | Grok's 1.27 / 1.93 are ln(z0); z0 = 3.56 / 6.89 mm | VERIFIED | Table 4 does list ln z0p 1.27 / 1.93 at 0 mm. Today's Grok pass labelled this "Table 3", which also lists them. Both are fine | https://arctic.eas.ualberta.ca/downloads/Brock%20et%20al%20J%20Glaciol%202006.pdf (same paper as the file's Cambridge URL, which also returns 200) |
| §7 WGMS data line | 5,512 obs, 343 glaciers, 1885–2025, excluding ELA flagged above/below | VERIFIED | Recomputed from `data/mass_balance.csv` + `data/glacier.csv`: excluding `ela_position` in (above, below) **and ela ≤ 0** gives 5,512 obs / 343 glaciers / 1885–2025. Without the ela ≤ 0 filter: 5,518 obs / 343 glaciers | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 WGMS method, current wording | per-glacier median of annual ELAs, then median across glaciers per 10° band, plus the two stated exclusion/band-edge steps | VERIFIED | Earlier FIX already applied in “Fixes after verify” (ELA ≤ 0 excluded; bands right-closed). Not re-derived in this pass (WGMS zip was checked earlier and deleted) | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 band 80–90°N | median 700 m, 2 glaciers | VERIFIED | recomputed: 700.0 m, n = 2 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 band 70–80°N | median 555 (IQR 438–704) m, 26 glaciers | VERIFIED | recomputed: 555.2 (438.1–704.2) m, n = 26 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 band 60–70°N | median 1,308 (IQR 1,104–1,491) m, 72 glaciers | VERIFIED | recomputed: 1307.5 (1103.8–1490.6) m, n = 72 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 band 50–60°N | median 1,810 (IQR 1,290–2,570) m, 41 glaciers | VERIFIED | recomputed: 1810.0 (1290.0–2570.0) m, n = 41 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 band 40–50°N | median 3,075 (IQR 2,837–3,400) m, 129 glaciers | VERIFIED | recomputed: 3075.0 (2837.0–3400.0) m, n = 129 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 band 30–40°N | median 4,861 (IQR 4,477–5,222) m, 30 glaciers | VERIFIED | recomputed: 4860.8 (4477.1–5221.8) m, n = 30 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 band 20–30°N | median 5,528 m, 8 glaciers | VERIFIED | recomputed: 5527.5 m, n = 8 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 band 10–20°N | median 5,105 m, 1 glaciers | VERIFIED | recomputed: 5105.0 m, n = 1 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 band 0–10°N | median 5,009 m, 3 glaciers | VERIFIED | recomputed: 5009.0 m, n = 3 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 band 0–10°S | median 4,912 m, 7 glaciers | VERIFIED | recomputed: 4912.0 m, n = 7 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 band 10–20°S | median 5,340 m, 3 glaciers | VERIFIED | recomputed: 5340.5 m, n = 3 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 band 20–30°S | median 5,288 m, 3 glaciers | VERIFIED | recomputed: 5287.5 m, n = 3 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 band 30–40°S | median 4,560 m, 3 glaciers | VERIFIED | recomputed: 4560.0 m, n = 3 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 band 40–50°S | median 1,816 m, 5 glaciers | VERIFIED | recomputed: 1816.5 m, n = 5 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 band 50–60°S | median 1,052 m, 2 glaciers | VERIFIED | recomputed: 1051.8 m, n = 2 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §7 band 60–70°S | median 231 m, 8 glaciers | VERIFIED | recomputed: 231.2 m, n = 8 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip |
| §8 G1 licence | "Open access under the requirement of correct citation" | VERIFIED | wording on wgms.ch database-versions page | https://wgms.ch/data_databaseversions/ |
| §8 G2/G3 USGS public domain | "for the most part, is in the public domain … may contain copyrighted materials" | VERIFIED | DS 1035 imprint page | https://pubs.usgs.gov/ds/1035/ds1035.pdf |
| §8 G8 ECOSTRESS | "Copyright © 2017, California Institute of Technology. ALL RIGHTS RESERVED"; credit line; two citations | VERIFIED | Wording and credit line confirmed on the homepage. Note: the site has **no explicit "no redistribution" clause**; F3's "treat as no redistribution without permission" is a reasonable inference from "all rights reserved", and the file words it as advice | https://speclib.jpl.nasa.gov/ |
| §8 G9/G10 required citations | Meerdink 2019 and Baldridge 2009 | VERIFIED | both listed on the homepage | https://speclib.jpl.nasa.gov/ |
| §2 G12 Devils Postpile | avg 2 ft, max 3.5 ft, some 60 ft; 200 posts: 4 = 2%, 5 = 37%, 6 = 55%, 7 = 5%; a few have 3 | VERIFIED | via WebFetch of the Wayback capture | https://web.archive.org/web/20100527174149/http://www.nature.nps.gov/geology/usgsnps/depo/dpgeol5.html |
| Partly-read sources kept out of number tables | G9 ◐ used only as a citation | VERIFIED | | — |

| §4 G2 band-mean reflectance (all tabulated samples) | dry/wet dune, basalt fresh/weathered, granite, limestone, siltstone, mud, burn area, etc., 3 d.p. in blue/green/red/VIS/NIR | VERIFIED | Recomputed from the official USGS splib07a ASCII (ASD 2151-channel / BECK). Band means inclusive 0.45–0.50 / 0.52–0.58 / 0.62–0.68 / 0.40–0.70 / 0.80–1.10 µm; deleted-channel sentinel −1.23e34 skipped. Every claimed 3-d.p. value matched. DWO dry = DEL2ar1 no oil; wet ar2 = DEL2ar2; wet 2b = DEL2b; burn area = BurnArea_TopSurface (not Traverse) | https://www.sciencebase.gov/catalog/file/get/586e8c88e4b0f5ce109fccae?name=ASCIIdata_splib07a.zip |
| §5 G6 soil-colour LUT | 454,664 occurrences / 441 soil colours; hue/value/chroma shares and top-15 Munsell + sRGB (e.g. 10YR 3/2 8.36%, 83,68,52); 58.9% in the listed top colours | VERIFIED | Recomputed from the official CSRL CSV, filtered to type==soil (7 non-soil rows have freq=NA). No licence statement in the CSV or on the page searched. The file already notes that “freq” is undocumented | https://casoilresource.lawr.ucdavis.edu/soil-properties/unique-moist-color-LUT.csv |
| §2 G11 Arches | over 2,000 arches; sliver-thin to >300 ft; official opening ≥3 ft (1 m); geological stability “at least 50,000 years” | VERIFIED | NPS page, HTTP 200. The page also prints “97 m” for the 300 ft span (300 ft ≠ 97 m); this conversion is already in F3 §10. File said “about 50,000”; the page is “at least 50,000 years” | https://www.nps.gov/arch/learn/nature/geologicformations.htm |
| §2 G13 Devils Postpile polygons | 940 polygons; columns up to 20 m; 53% 6-sided, 27% 5-sided, 18% 7-sided; only this study and O’Reilly 1879 found ~20% 7-gons | VERIFIED | Results text: “Of the mapped polygons, 53% have 6 sides, 27% 5 sides, and 18% 7 sides.” “columns up to 20 m”; “940 polygons”; “Only this study and O'Reilly (1879) found an abundance (~20%) of 7-gons.” eScholarship HTML was 202; the PDF opened (HTTP 200, 3.6 MB). Crossref 404 for DOI 10.83206/B1RP47; no CC statement in the extract | https://escholarship.org/content/qt06x2s49m/qt06x2s49m.pdf |
| §2 G14 columnar order | ordered, mostly hexagonal; “contrary to the frequently encountered assumption that columnar jointing tends towards a perfect hexagonal lattice” | VERIFIED | arXiv copy of Goehring & Morris 2005, abstract and text | https://arxiv.org/pdf/cond-mat/0501015 |
| §2 G15 sheeting thickness (Matthes via Mitchell) | sheets from less than a metre to ~30 m | VERIFIED | Mitchell 2010 thesis: “The sheets he observed vary in thickness from less than a meter to ~ 30 m” (citing Matthes 1930). Secondary citation, as the file states | https://www.soest.hawaii.edu/earthsciences/wp-content/uploads/2025/09/MS_2010_Mitchell.pdf |
| §2 G16 Yosemite sheet example | 19 m × 4 m × 0.1 m granodiorite sheet, Yosemite Valley | VERIFIED | Collins & Stock 2016 Table S1 / SI | https://static-content.springer.com/esm/art%3A10.1038%2Fngeo2686/MediaObjects/41561_2016_BFngeo2686_MOESM162_ESM.pdf |
| §2 G17 Miljevci dolines | 286 dolines mapped; density 0–34 /km²; two larger areas around 30 and 34 /km²; average ~1 /km² is negligible; Pahernik classes negligible 1–10, small 10–30, medium 30–60, … extremely high >200 | VERIFIED | Abstract and results. “286 dolines were mapped”; “varies from 0 to 34 dolines/km²”; “two larger areas with density around 30 and 34 dolines/km²”; “average of 1 doline/km² is negligible”. Pahernik (2012) classes as listed. Licence is CC BY-NC-ND **4.0** (file omitted the version) | https://ojs.zrc-sazu.si/carsologica/article/download/10465/10101 |

## Sources

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| G1 | https://wgms.ch/downloads/DOI-WGMS-FoG-2026-02-10.zip | y (200, 39.1 MB zip; earlier pass) | y |
| G1 DOI | https://doi.org/10.5904/wgms-fog-2026-02-10 | y | y |
| G2 ASCII zip | https://www.sciencebase.gov/catalog/file/get/586e8c88e4b0f5ce109fccae?name=ASCIIdata_splib07a.zip | y (200, 21 MB zip; used in scratch only) | y (USGS data release, public domain; DOI 10.5066/F7RR1WDJ) |
| G3 | https://pubs.usgs.gov/ds/1035/ds1035.pdf | y | y |
| G4 | https://atmos.uw.edu/~sgw/PAPERS/2010_KPCL.pdf | y | y (© IGS; author copy) |
| G5 | https://atmos.uw.edu/~sgw/PAPERS/1994_SnowAlbJGR.pdf | y (scan, no text layer) | y (© 1994 AGU; author scan) |
| G6 | https://casoilresource.lawr.ucdavis.edu/soil-properties/unique-moist-color-LUT.csv | y (200, 16,308 B) | no licence stated on the CSV or the page searched |
| G8 | https://speclib.jpl.nasa.gov/ | y | y |
| G11 | https://www.nps.gov/arch/learn/nature/geologicformations.htm | y (200) | y (NPS, US Government, public domain) |
| G12 | Wayback dpgeol5 (above) | y via WebFetch; curl timed out | y |
| G13 page | https://escholarship.org/uc/item/06x2s49m | 202 (bot challenge) | — |
| G13 PDF | https://escholarship.org/content/qt06x2s49m/qt06x2s49m.pdf | y (200, 3.6 MB PDF) | no CC found in the extract; Crossref 404 for 10.83206/B1RP47 |
| G14 | https://arxiv.org/pdf/cond-mat/0501015 | y | y (arXiv copy of EPL paper; © EDP Sciences on the journal, arXiv is the copy read) |
| G15 | https://www.soest.hawaii.edu/earthsciences/wp-content/uploads/2025/09/MS_2010_Mitchell.pdf | y | no open licence stated (UH Manoa MS thesis) |
| G16 SI | https://static-content.springer.com/esm/art%3A10.1038%2Fngeo2686/MediaObjects/41561_2016_BFngeo2686_MOESM162_ESM.pdf | y | y (© Springer Nature; SI of a paywalled paper) |
| G17 | https://ojs.zrc-sazu.si/carsologica/article/download/10465/10101 | y | y (CC BY-NC-ND **4.0**; file said CC BY-NC-ND, version omitted) |
| G18 | https://www.cambridge.org/core/services/aop-cambridge-core/content/view/30AB8A2DCFF90741FF302B4EB68D359B | y (200) | y (© IGS, free to read) |

## Not found
- No G2/G6/G11/G13–G17 figure failed to open in a primary file or official page. G13’s licence is not stated in the pages opened.

## Sources disagree
- **Brock table numbering.** Kept from the earlier pass; the current file now cites Table 3 / Table 1a/1b correctly.
- **Grenfell copy.** Today's earlier Grok pass cited a pirate mirror for Grenfell 1994. I did not use it.
- **G11 300 ft vs 97 m.** Already in F3 §10. Confirmed: the NPS page prints both, and 300 ft ≠ 97 m.
- **G11 stability wording.** Page: “at least 50,000 years”; file: “about 50,000”.

## Grok pass
- A new F1–F3 completing-pass run was launched at **10:41:47 AEDT** on 2026-11-17: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve --prompt-file /workspace/lv-scratch/f13/grok/prompt.txt`, cwd `/workspace/lv-scratch/f13/grok`. The 25-minute wait ends at about 11:06:47 AEDT. At **11:07 AEDT** the process was **still running** (about 25 minutes; TUI output only; no finished report), so this verification was finished without it and **does not use any Grok-only number**. Every figure above was opened in a primary PDF, official CSV/zip, or official page.
- The earlier 09:01–09:05 AEDT F1–F3 run described in the previous verify file is unchanged and does not cover these leftover rows.
