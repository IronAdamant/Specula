# I3-material-values.md — Library Verify

Verifier: Library Verify (adversarial re-check). Written 2026-11-17, Australia/Sydney (AEDT, UTC+11).
I3 itself was not edited. URLs below were accessed on **2026-11-17**.

## Version checked
- Snapshot at start: **42,477 bytes**, sha256 `e89186b3ad78cd4368da125b9f019139fc8b24a912a77df9b515d2f6f3780772`.
- Re-stat before writing this file: same size and same sha256 as the start-of-check copy in `/workspace/lv-scratch/i13/`. No re-check of changed rows was needed.
- Box mtimes all read ~09:59 AEDT from the restore and were not used.

## Summary

| Status | Rows | Meaning |
|---|---:|---|
| **VERIFIED** | **122** | All metal, dielectric, stone, wood, cloth, MERL/Ngan plastic, and everyday-object ID rows except the four IOR→F0 rounding errors |
| **FIX** | **5** | PI2, PI3, PI4, PI5 (IOR→F0 arithmetic), plus the ECOSTRESS rock-type spread sentence (marble min and a 1-decimal basalt min) |
| **UNVERIFIABLE** | **0** | Every numbered ID was opened in a primary file or page this pass |
| **Total** | **127** | 126 ID rows (M1–M22, MN1–MN4+MN6–MN11, D1–D4, D7, D10, S1–S34, W1–W3, C1–C12, PL1–PL22, PI1–PI7, E1–E10) plus the S17–S34 spread sentence |

**Worst problems**
1. **Four computed plastic F0 values are off by 1–2 units in the 4th decimal.** The API IORs are exact as printed; F0 = ((n−1)/(n+1))² should be PC 0.0512, PET 0.0499, PLA 0.0348, PP 0.0390. I3 has 0.0513 / 0.0501 / 0.0350 / 0.0391. Acrylic, PUR and PVC are correctly rounded. The IOR-derived range “0.035–0.053” is still a fair 3-decimal rounding of 0.0348–0.0533.
2. **The ECOSTRESS marble spread is taken from the second-lowest sample, not the lowest.** Pink marble Ward96 (S22) has VIS mean 35.43 %, so the five-sample range is **35.4–72.8 %**, not 36.0–72.8 % (36.02 % is dolomitic marble S24). The same slip is in Sources disagree (“36–73 %”). White marble Ward95 itself (69.60 / 74.42 / 75.34; mean 72.82) is correct. A smaller 1-decimal issue: basalt min 4.15 rounds to **4.1**, not 4.2.
3. Nothing else in the numbered tables failed. The four close checks all stand as I3 printed them: plastic F0 0.02–0.15 vs IOR 0.035–0.053 is a real, already-documented disagreement; white marble 0.83 vs ~0.73 are different quantities; MERL research-only vs Zenodo CC BY-SA 4.0 is a real licence conflict; ECOSTRESS is “ALL RIGHTS RESERVED”.

**Dead links:** no true 404s among I3’s cited URLs.
- ACM Jensen 2001 DOI (cited only as the PB marble provenance): 403 paywall. The marble RGB was checked against the physicallybased.info API, not the paper.
- `https://doi.org/10.5281/zenodo.8101680` and `https://zenodo.org/records/8101681` return 403 to curl from this box; WebFetch of the record page opened it (CC-BY-SA-4.0, published 2023-06-30).
- SpectralDB measurement pages returned a Cloudflare challenge to curl, so the “licence not stated” claim could not be re-read on a fully rendered home page. The 00117 URL itself is as I3 cites.
- ECOSTRESS directory listings can 403; every individual S17–S34 spectrum file returned 200.
- De Deken thesis: some requests 403; a Mozilla GET of the official PDF returned 200 (75 pages, 133 MB).

## Close checks
1. **Plastic F0 0.02–0.15 vs IOR 0.035–0.053.** I3 already has both ranges and explains them. They are not a contradiction in the file.
   - IOR→F0 from the live API v2 (header `license: CC0 1.0 Universal`, `updated: 202611031901`, 122 materials): Acrylic 1.4905 → 0.0388; PC 1.5848 → 0.0512; PET 1.575 → 0.0499; PLA 1.4586 → 0.0348; PP 1.492 → 0.0390; PUR 1.6 → 0.0533; PVC 1.542 → 0.0455. Span 0.0348–0.0533, which I3 summarises as 0.035–0.053.
   - Ngan 2005 supplemental Cook-Torrance p0 for the 19 PL rows excluding the two 0.999 outliers and acrylic-yellow 0.399: min teflon 0.0203, max phenolic-yellow 0.153, median 0.0654 (I3’s 0.065). That is the 0.020–0.153 span. (MERL also has polypropylene p0 = 0.0183; I3 did not include that material in PL1–PL22.)
   - Hoffman Table 1 “Plastic / Glass (Low)” 0.03 and “Plastic High” 0.05, and Lagarde’s 0.02–0.05 dielectric rule, are a third, shading-guide range (D2, D4), not a MERL fit.
   - **Outcome:** I3 is right to keep both measured-fit and IOR-derived ranges. Only the four 4th-decimal F0 figures named above need a fix.

2. **White marble albedo: 0.83 or 0.73?** Both numbers are real and I3 already says they measure different things.
   - physicallybased.info “Marble”: linear sRGB **0.830, 0.791, 0.753**, IOR 1.5, citing Jensen et al. 2001. sRGB8 235,230,225 recomputes.
   - ECOSTRESS Ward95 (White Marble, West Rutland VT, hemispherical reflectance, percent): 450 / 550 / 650 nm = **69.6025 / 74.4161 / 75.3413** → 69.60 / 74.42 / 75.34; VIS mean over 0.40–0.70 µm, n = 301, **72.8227** → 72.82 %. That is ~0.73 as a 0–1 albedo, not 0.83.
   - The five ECOSTRESS marble VIS means are 72.82, 35.43, 45.94, 36.02, 68.04 % — range **35.4–72.8 %** (see FIX below), not 36–73 %.
   - **Outcome:** 0.83 is the PB/Jensen Rd; ~0.73 is Ward95 VIS mean. Keep both; correct only the five-sample min.

3. **MERL: research-only terms vs CC BY-SA on Zenodo.** The conflict is real; I3’s “treat as research-only for shipping until MERL confirms” is the right caution.
   - Live `Copyright_Notice.txt` (https://people.csail.mit.edu/wojciech/BRDFDatabase/Copyright_Notice.txt, 200): “Copyright 2006 Mitsubishi Electric Research Laboratories All Rights Reserved. Permission to use, copy and modify … without fee for educational, research and non-profit purposes, is hereby granted”; commercial products need MERL permission (license@merl.com). **[NC]**
   - CSAIL page: “MERL provides this data for research and academic use only.”
   - Zenodo record 8101681 (WebFetch, 2026-11-17): licence **CC-BY-SA-4.0**, published 2023-06-30, authors Matusik/Pfister/Brand/McMillan, “Created by MERL, 2003, 2007, 2023”. I3’s DOI 10.5281/zenodo.8101680 is the concept DOI; 8101681 is the record.

4. **ECOSTRESS: is it really ‘all rights reserved’?** Yes.
   - Home page https://speclib.jpl.nasa.gov/ (200): “Copyright © 2017, California Institute of Technology. ALL RIGHTS RESERVED,” plus the courtesy credit I3 quotes. I3’s **[NO-REDISTRIBUTION]** (cite facts; do not copy files into a repo) matches that notice. All 18 spectrum files opened (200).

## Fixes (corrected value and source)

| Row | I3 prints | Correction | Source |
|---|---|---|---|
| PI2 Polycarbonate | F0 0.0513 | **0.0512** | API IOR 1.5848; ((1.5848−1)/(1.5848+1))² = 0.051187. https://api.physicallybased.info/v2/materials |
| PI3 PET | F0 0.0501 | **0.0499** | API IOR 1.575; ((1.575−1)/(1.575+1))² = 0.049863. same URL |
| PI4 PLA | F0 0.0350 | **0.0348** | API IOR 1.4586; ((1.4586−1)/(1.4586+1))² = 0.034793. same URL |
| PI5 Polypropylene | F0 0.0391 | **0.0390** | API IOR 1.492; ((1.492−1)/(1.492+1))² = 0.038979. same URL |
| S17–S34 spread, and Sources disagree “White marble albedo” | marble 36.0–72.8 % (n = 5); “36–73 %”; basalt 4.2–16.7 % | marble **35.4–72.8 %** (S22 pink marble VIS mean 35.43); basalt **4.1–16.7 %** (S32 VIS mean 4.15) | Recomputed from the 18 official spectrum files under https://speclib.jpl.nasa.gov/ecospeclibdata/ |

PI1 0.0388, PI6 0.0533 and PI7 0.0455 are correctly rounded and stay.

## Row table

| Row | Claim (I3) | Status | Note / correction | URL checked (2026-11-17) |
|---|---|---|---|---|
| M1 | Iron 0.560, 0.570, 0.580; sRGB8 197,199,200 | VERIFIED | UE 4.27 “Measured BaseColors for metals”. sRGB8 recomputes. | https://dev.epicgames.com/documentation/en-us/unreal-engine/physically-based-materials?application_version=4.27 |
| M2–M9 | Lagarde 2011 metal RGB (Ag, Al, Au, Cu, Cr, Ni, Ti, Co/Pt) and sRGB8 | VERIFIED | All nine linear triples match the blog table exactly (Gold 1, 0.765557, 0.336057 etc.). sRGB8 samples (Au 255,227,157; Ag 252,250,245; Al 245,246,246; Cu 250,209,194) recompute. | https://seblagarde.wordpress.com/2011/08/17/feeding-a-physical-based-lighting-mode/ |
| M10 | UE 4.27 repeats M2–M9 to 3 dp, e.g. Gold 1.000, 0.766, 0.336; cites Lagarde and Karis | VERIFIED | UE table: Ag 0.972,0.960,0.915; Al 0.913,0.921,0.925; Au 1.000,0.766,0.336; Cu 0.955,0.637,0.538; Cr 0.550,0.556,0.554; Ni 0.660,0.609,0.526; Ti 0.542,0.497,0.449; Co 0.662,0.655,0.634; Pt 0.672,0.637,0.585. References 1–2 are Lagarde and Karis. | same as M1 |
| M11 | Hoffman Table 1 (RTR 3rd ed.): Fe/Cu/Au/Al/Ag linear and source sRGB | VERIFIED | Notes p. 16 Table 1 matches every pair I3 prints (Fe 0.56,0.57,0.58 / 0.77,0.78,0.78; Au 1.00,0.71,0.29 / 1.00,0.86,0.57, etc.). Caption credits Real-Time Rendering, 3rd ed. | https://blog.selfshadow.com/publications/s2013-shading-course/hoffman/s2013_pbs_physics_math_notes.pdf |
| M12–M20 | PB API metal colours (Al, Au, Cu, Ag, Fe, SS, brass, Cr/Ni/Ti, Zn/Pb/Pt/Co) and sRGB8 | VERIFIED | Live API matches every triple. sRGB8 for Al, Cu, Ag, Fe, SS, brass and clamped-R gold (255,228,150) all recompute. Licence CC0 1.0 in the JSON header. | https://api.physicallybased.info/v2/materials |
| M21 | Cast iron 0.027, 0.026, 0.026; metalness 1; roughness 0.3 (authored); sRGB8 46,45,45 | VERIFIED | API matches; roughness 0.3 and metalness 1 confirmed. | API; GitHub tree as cited |
| M22 | n,k → R at ~550 nm (Ag 0.984, Al 0.922, Au 0.807, Cr 0.681, Cu 0.619, Ni 0.644, Ti 0.404) | VERIFIED | YAML in refractiveindex.info-database: Cu Johnson 0.5486 µm n=1.02 k=2.577 R=0.61945; Ag Babar 0.5391 n=0.05168 k=3.483 R=0.98438; Au Babar 0.5391 n=0.329 k=2.249 R=0.80716; Al Cheng 0.55104 n=0.59319 k=5.28629 R=0.92216; Ni Johnson 0.549 n=1.92 k=3.61 R=0.64376; Cr Sytchkova 0.55085 n=3.59762 k=4.89768 R=0.68110; Ti Palm 0.54996 n=1.863979 k=2.074363 R=0.40378. Formula R=((n−1)²+k²)/((n+1)²+k²). Repo LICENSE is CC0 1.0. | https://github.com/polyanskiy/refractiveindex.info-database |
| MN1–MN4, MN6–MN11 | Ngan Cook-Torrance F0 / m / error for 10 MERL metals | VERIFIED | Every value matches the supplemental (incl. aluminium p. 17 F0 0.59 m 0.00776 err 0.36; copper F0 0.999). There is no MN5. Fit-error range 0.3–0.57 for the polished metals is fair. Licence conflict flagged correctly on the MN block. | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf |
| D1 | Water F0 0.02 (Hoffman); 0.020373188 (Lagarde) | VERIFIED | Both sources exact. | Hoffman notes; Lagarde blog |
| D2 | Plastic/glass 0.03–0.05; glass 0.04 | VERIFIED | Hoffman Table 1: Plastic/Glass (Low) 0.03, Plastic High 0.05; Lagarde sample “Glass 0.04”. | same |
| D3 | Ruby 0.077, diamond 0.172 (Lagarde); Hoffman ruby 0.08, diamond 0.17 | VERIFIED | Lagarde: Ruby 0.077271957, Diamond 0.171968833. Hoffman: Glass (High)/Ruby 0.08, Diamond 0.17. | same |
| D4 | Non-gem dielectrics 0.02–0.05; never below 0.02; 0.04 if unknown | VERIFIED | Lagarde: “No value under 0.02”; “setting a value of 0.04 (around plastic)”; “Except gemstones … 0.02-0.05”. Hoffman: “values lower than 2%”; “narrow range of F0 values—between 2% and 5%”. | same |
| D7 | Ice / milk / quartz / alcohol 0.0179 / 0.0222 / 0.0456 / 0.0200 | VERIFIED | Lagarde: 0.017908907 / 0.022181983 / 0.045593921 / 0.01995505. | Lagarde blog |
| D10 | Frostbite gemstones ~8% ruby to 17% diamond; cap at 16% | VERIFIED | v3.2 §3.2: “from 8% for ruby to 17% for diamond. We chose to limit the function to 16% as an approximation.” Also f0 = 0.16 reflectance². | https://seblagarde.files.wordpress.com/2015/07/course_notes_moving_frostbite_to_pbr_v32.pdf |
| S1–S5 | UE vs Lagarde charcoal/asphalt/concrete/soil/grass/sand/ice/snow; sRGB8 | VERIFIED | UE: 0.02 / 0.02 / 0.08 / 0.13 / 0.21 / 0.36 / 0.51 / 0.56 / 0.81, and “photographed using a polarizing filter (polarization removes the specular of nonmetals when aligned)”. Lagarde: 0.04 / 0.04 / 0.12 / 0.17 / 0.25 / 0.40 / 0.55 / 0.5–0.7 / 0.80–0.90, “includes some specular”. sRGB8 39/56, 80/97, 189/196 recompute. | UE 4.27; Lagarde |
| S6 | Diffuse sRGB 50–240 | VERIFIED | Lagarde: “a range for diffuse color in sRGB of 50-240”. | Lagarde |
| S7–S9, W1 | USGS splib05a 450/550/650 nm | VERIFIED | Official ASCII: Concrete WTC01-37A 0.452361 / 0.519802 / 0.55489; Asphalt GDS376 Blck_Road old 0.068915 / 0.085581 / 0.102643; Limestone CU02-11A 0.157698 / 0.190913 / 0.213514; Plywood GDS365 Fresh Pine 0.202069 / 0.321758 / 0.560472. Public-domain USGS OFR 03-395. ~sRGB8 at 550 nm (191, 83, 121, 154) recompute. | https://pubs.usgs.gov/of/2003/ofr-03-395/ASCII/… |
| S10–S16 | PB asphalt, concrete, brick, terracotta/clay, marble, sand, rust | VERIFIED | Live API matches (sand B is 0.23074, I3’s 0.231 is ordinary 3-dp rounding). IOR 1.6 → F0 0.053 and IOR 1.5 → 0.04 recompute. Marble sRGB8 235,230,225 recomputes. Asphalt IOR 1.6 and roughness 0.5 match. Concrete 0.51×3 is authored from Lagarde/UE, as I3 says. | API |
| S17–S34 (values) | 18 ECOSTRESS rocks, R% at 450/550/650 and VIS mean n=301 | VERIFIED | Every band and every mean matches the official file to the printed 2 decimals (Ward95 confirmed independently). n=301 in every file. Names/origins in the file headers match (e.g. S21 White Marble, West Rutland VT, Ward95). | individual ecospeclibdata URLs in I3 |
| S17–S34 spread | granite 16.2–27.9; marble 36.0–72.8 (n=5); sandstone 16.5–40.7; slate 9.5–19.1; basalt 4.2–16.7 | **FIX** | Granite, sandstone and slate stand (16.23–27.91, 16.47–40.66, 9.54–19.07). Marble min is S22 35.43, not S24 36.02 → **35.4–72.8**. Basalt min S32 4.15 → **4.1–16.7**. Same marble slip in Sources disagree. | same spectrum files |
| W2, W3 | MERL woods F0/m and fit errors 0.0035–0.017 | VERIFIED | All seven F0/m pairs match Ngan. Errors excluding pickled-oak-260: 0.00349–0.0168, which I3 rounds to 0.0035–0.017. Generator span F0 ≈ 0.02–0.07, m ≈ 0.09–0.26 is the non-outlier set (ipswich-pine 0.0192 is just under 0.02; walnut-dark 0.123 is called out separately as 0.123). | Ngan supplemental |
| C1–C4, C9 | Cotton + polyester interlock Lambert and Torrance-Sparrow | VERIFIED | De Deken 2010 Tables 5.1 and 5.3 (PDF pp. 31 and 34) match every ρd, error, ks and m I3 prints. IOR “fixed 1.5” is in the Table 5.3 commentary. | https://escholarship.org/content/qt9zh862bt/qt9zh862bt.pdf |
| C5–C8, C10 | Linen, wool, silk, nylon, velvet (Lambert ρd + TS m); Oren-Nayar velvet stretch 0.628 / 0.289 | VERIFIED | Table 5.1 / 5.3 / 5.2 exact (thesis typo “Heavey” / “Tropcial” not copied). C8 uses swimwear 0.818, not matte swimwear 0.695, as labelled. | same |
| C11 | Satin Lambert ρd saturates at 1.000, errors 0.38–0.44 | VERIFIED | Silk charmeuse 1.000 / 0.379; polyester satin 1.000 / 0.420; polyester satin charmeuse 1.000 / 0.437. | same |
| C12 | MERL fabrics F0/m | VERIFIED | All five pairs match Ngan (nylon 0.0423 / 0.0662 etc.). | Ngan supplemental |
| PL1–PL22 | 22 MERL plastics: F0, m, error, pages, some diffuse RGB | VERIFIED | Every printed number and every supplemental page number matches (acrylic-blue is p. 3, teflon p. 181, yellow-plastic p. 201, etc.). 19-row median 0.0654 → I3’s 0.065. | Ngan supplemental |
| PI1, PI6, PI7 | Acrylic / PUR / PVC IOR and F0 | VERIFIED | IOR exact; F0 0.0388 / 0.0533 / 0.0455 correctly rounded. | API |
| PI2–PI5 | PC / PET / PLA / PP F0 | **FIX** | IORs exact. Correct F0: **0.0512 / 0.0499 / 0.0348 / 0.0390**. | API |
| E1–E10 | Everyday-object colours, authored roughness, IOR; Palmqvist vs SpectralDB | VERIFIED | Live API matches every colour, roughness and IOR. References: E1–E3, E5–E7 Palmqvist first; E4 Palmqvist + Jakubiec; E8–E10 Jakubiec first (as I3 labels SpectralDB 00090 / 00074 / 00079). | API |
| Leather / Not found | No leather albedo in RGL, MERL, or the API | VERIFIED (gap) | RGL page has the CC0 1.0 link and no leather entry; API has no leather; Ngan 100 names have no leather. UTIA site not re-opened. | https://rgl.epfl.ch/materials ; API ; Ngan |
| SD Gold / Iron / Cr / Ti F0 | The four metal disagreements | VERIFIED | The printed triples match Lagarde/UE, PB and Hoffman as cited; Cr 0.681 and Ti 0.404 spot checks match M22. | as above |
| SD charcoal/asphalt/concrete | UE lower than Lagarde; USGS 0.086 between fresh and worn | VERIFIED | See S1–S5, S8. | as above |
| SD cloth model | Cotton broadcloth 0.600 vs 0.794 | VERIFIED | Tables 5.1 and 5.3. | De Deken |
| SD white marble / plastic F0 | See close checks 1–2 | VERIFIED as disagreement; marble min **FIX** | 0.83 vs 72.8 % stand; range min should be 35.4 %. | API; Ward95 and S22 files |

## Source table

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| physicallybased.info API v2 | https://api.physicallybased.info/v2/materials | 200, JSON header CC0 1.0 Universal, updated 202611031901 | Yes: CC0 1.0 |
| physically-based-api repo | https://github.com/AntonPalmqvist/physically-based-api | opened earlier this task (LICENSE CC0) | Yes |
| Lagarde 2011 blog | https://seblagarde.wordpress.com/2011/08/17/feeding-a-physical-based-lighting-mode/ | 200 | Yes: blog ©, facts only |
| Hoffman 2013 notes | https://blog.selfshadow.com/publications/s2013-shading-course/hoffman/s2013_pbs_physics_math_notes.pdf | 200 | Yes: © authors/RTR, facts only |
| Frostbite PBR v3.2 | https://seblagarde.files.wordpress.com/2015/07/course_notes_moving_frostbite_to_pbr_v32.pdf | 302→200, 54 MB PDF | Yes: EA/authors © |
| UE 4.27 PBM docs | https://dev.epicgames.com/documentation/en-us/unreal-engine/physically-based-materials?application_version=4.27 | 200 | Yes: Epic ©, facts only |
| refractiveindex.info-database | https://github.com/polyanskiy/refractiveindex.info-database (LICENSE raw) | 200, LICENSE = CC0 1.0 Universal | Yes |
| Ngan 2005 supplemental | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 200, 203 pp | Yes: © authors/MERL (facts used) |
| MERL Copyright_Notice.txt | https://people.csail.mit.edu/wojciech/BRDFDatabase/Copyright_Notice.txt | 200 | Yes: **[NC]** educational/research/non-profit only |
| MERL CSAIL page | https://cdfg.csail.mit.edu/wojciech/brdfdatabase (and the woiciech/BRDFDatabase page opened) | 200 | Yes: research and academic use only |
| Zenodo MERL record | https://zenodo.org/records/8101681 | curl 403; WebFetch 200, CC-BY-SA-4.0, 2023-06-30 | Yes: I3 correctly reports the conflict with the MERL notice |
| USGS splib05a ASCII | pubs.usgs.gov OFR 03-395 ASCII files for S7–S9, W1 | 200 | Yes: US Gov public domain |
| ECOSTRESS home | https://speclib.jpl.nasa.gov/ | 200 | Yes: Caltech © 2017 ALL RIGHTS RESERVED — **[NO-REDISTRIBUTION]** |
| ECOSTRESS S17–S34 files | https://speclib.jpl.nasa.gov/ecospeclibdata/… | 200 each | Same notice as the home page |
| De Deken 2010 thesis | https://escholarship.org/content/qt9zh862bt/qt9zh862bt.pdf | 200 (some 403 without a browser UA); 75 pp | Yes: © author, facts only |
| RGL-EPFL materials | https://rgl.epfl.ch/materials | GET 200 (HEAD 405); links creativecommons.org/publicdomain/zero/1.0/ | Yes: CC0. No leather entry |
| SpectralDB 00117 | https://spectraldb.com/measurements/00117/ | Cloudflare challenge to curl | I3’s **[NO-LICENCE]** for underlying spectra could not be re-confirmed on a fully rendered page |
| Jensen 2001 (PB marble cite only) | https://dl.acm.org/doi/10.1145/383259.383319 | ACM 403 | I3 already flags the paper as ACM © and takes the RGB from the CC0 API |
| Karis 2013 notes | https://blog.selfshadow.com/publications/s2013-shading-course/karis/s2013_pbs_epic_notes_v2.pdf | I3 says opened, no numbers taken; not re-downloaded this pass | N/A |

## Grok

One live Grok CLI pass covering I1, I2 and I3 was started at **10:17:18 AEDT** on 2026-11-17:

`~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve --no-alt-screen --prompt-file /workspace/lv-scratch/i13/grok/PROMPT.md`

The process was **still running** when this verify file was written (~10:37 AEDT; wrapper elapsed ~19 min). No number in this file was taken from Grok. Every count above was re-read from a primary page, PDF, JSON, YAML or spectrum file opened on this box.
