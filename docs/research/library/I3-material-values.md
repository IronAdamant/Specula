# I3 — Measured material values: base colour, roughness, F0 / specular

**Changelog**
- 2026-11-17 10:39 AEDT (Library Verify I3 pass, verify file 10:38): F0 rounding fixed for PI2 0.0512, PI3 0.0499, PI4 0.0348, PI5 0.0390 (recomputed ((n−1)/(n+1))² from the API IORs). ECOSTRESS spread fixed: marble 35.4–72.8 % (min is S22 pink marble 35.43, not S24 36.02) and basalt 4.1–16.7 % (S32 VIS mean re-computed today from the spectrum file: 4.148), in §stone spread and Sources disagree.
- 2026-11-17 09:27 AEDT: Skin, eye, hair, nail, tooth, blood and bone content removed as out of scope (now in B7): rows D5, D6, D8, D9, §7 K1–K11, source 12 (Meng), the Skin F0 disagreement and the hair/nails/skin Not-found bullets. Added §5b plastics (22 MERL/Ngan Cook-Torrance rows PL1–PL22, 7 IOR→F0 rows PI1–PI7, 10 spectrophotometer everyday-object rows E1–E10), 18 ECOSTRESS stone spectra S17–S34 (granite, marble, sandstone, slate, basalt), the RGL-EPFL CC0 licence, the MERL copyright-notice wording, an updated leather status, and two new Sources-disagree bullets. New rows are dated 2026-11-17; older rows keep 2026-11-16. A second Grok pass (/tmp/grokpass/i3b) supplied leads; every added number was re-read from the primary file today.

Every row carries its own source URL and access date (2026-11-16 for original rows, 2026-11-17 for rows added or re-checked today). Each link was opened from the box or via fetch on the date shown.
A live grok pass (grok-4.6, xhigh, live search, finished 11:00 AEDT) was used for leads only. Every number below was re-read from its primary page or file by me. Grok-only items I could not open are listed under Not found.

## Conventions
- "lin" = linear sRGB/Rec.709 primaries, 0–1.
- "sRGB8" = 8-bit sRGB-encoded value that I computed from lin with the IEC 61966-2-1 piecewise formula (clamped to 0–1). It is not a source number.
- F0 = Fresnel reflectance at normal incidence.
- For dielectrics, F0 = ((n−1)/(n+1))² where only an IOR is given. Rows marked "computed" are my arithmetic.
- For metals, base colour = F0 colour (metalness workflow).
- **Roughness warning:** physicallybased.info roughness values (0, 0.5, 0.9, 1.0) are authored defaults, not measurements. Measured roughness only comes from BRDF fits (MERL/Ngan; UCSD cloth thesis). Those fits use Beckmann/Cook-Torrance "m" (RMS slope), not the UE/Disney perceptual "roughness". A common mapping is perceptual r ≈ √m, but that is an interpretation, not a source value.
- **Three different "albedos":**
  - (a) spectrally integrated F0 from lab n,k (metals);
  - (b) spectrophotometer reflectance of a real object;
  - (c) broadband solar albedo (includes near-IR).
  - They are not interchangeable; each row says which it is.

## 1. Metals — F0 / base colour (linear RGB)

ID  | Metal          | Value (lin R,G,B)            | sRGB8 (computed) | Kind | Source                                                  | Licence | URL | Accessed
M1  | Iron           | 0.560, 0.570, 0.580          | 197,199,200      | a    | Epic UE 4.27 docs, "Measured BaseColors for metals"     | Epic docs © (facts only) | https://dev.epicgames.com/documentation/en-us/unreal-engine/physically-based-materials?application_version=4.27 | 2026-11-16
M2  | Silver         | 0.971519, 0.959915, 0.915324 | 252,250,245      | a    | Lagarde 2011 blog, metal table ("pure laboratory material") | blog © (facts only) | https://seblagarde.wordpress.com/2011/08/17/feeding-a-physical-based-lighting-mode/ | 2026-11-16
M3  | Aluminium      | 0.913183, 0.921494, 0.924524 | 245,246,246      | a    | Lagarde 2011                                            | blog ©  | same as M2 | 2026-11-16
M4  | Gold           | 1, 0.765557, 0.336057        | 255,227,157      | a    | Lagarde 2011                                            | blog ©  | same as M2 | 2026-11-16
M5  | Copper         | 0.955008, 0.637427, 0.538163 | 250,209,194      | a    | Lagarde 2011                                            | blog ©  | same as M2 | 2026-11-16
M6  | Chromium       | 0.549585, 0.556114, 0.554256 | 196,197,196      | a    | Lagarde 2011                                            | blog ©  | same as M2 | 2026-11-16
M7  | Nickel         | 0.659777, 0.608679, 0.525649 | 212,205,192      | a    | Lagarde 2011                                            | blog ©  | same as M2 | 2026-11-16
M8  | Titanium       | 0.541931, 0.496791, 0.449419 | 194,187,179      | a    | Lagarde 2011                                            | blog ©  | same as M2 | 2026-11-16
M9  | Cobalt / Platinum | 0.662124,0.654864,0.633732 / 0.672411,0.637331,0.585456 | — | a | Lagarde 2011                                     | blog ©  | same as M2 | 2026-11-16
M10 | UE 4.27 repeats M2–M9 rounded to 3 dp (e.g. Gold 1.000,0.766,0.336), citing Lagarde and Karis | — | — | a | Epic UE 4.27 docs | Epic docs © | same as M1 | 2026-11-16
M11 | Iron / Copper / Gold / Aluminium / Silver (F0, linear and sRGB-encoded given by source) | Fe 0.56,0.57,0.58; Cu 0.95,0.64,0.54; Au 1.00,0.71,0.29; Al 0.91,0.92,0.92; Ag 0.95,0.93,0.88 | source sRGB: Fe 0.77,0.78,0.78; Cu 0.98,0.82,0.76; Au 1.00,0.86,0.57; Al 0.96,0.96,0.97; Ag 0.98,0.97,0.95 | a | Hoffman, SIGGRAPH 2013 "Background: Physics and Math of Shading" notes, Table 1 (table credited to Real-Time Rendering 3rd ed.) | Notes © authors/RTR (facts only) | https://blog.selfshadow.com/publications/s2013-shading-course/hoffman/s2013_pbs_physics_math_notes.pdf | 2026-11-16
M12 | Aluminum (PB)  | 0.916, 0.923, 0.924          | 245,246,246      | a    | physicallybased.info API v2 (derived from refractiveindex.info Cheng Al via OpenPBR App. D method) | CC0 1.0 | https://api.physicallybased.info/v2/materials | 2026-11-16
M13 | Gold (PB)      | 1.059, 0.773, 0.307 (R > 1 = outside Rec.709 gamut) | 255(clamped),228,150 | a | physicallybased.info, from Babar & Weaver Au | CC0 | same as M12 | 2026-11-16
M14 | Copper (PB)    | 0.932, 0.623, 0.522          | 247,207,191      | a    | physicallybased.info, from Johnson & Christy Cu          | CC0     | same as M12 | 2026-11-16
M15 | Silver (PB)    | 0.991, 0.985, 0.974          | 254,253,252      | a    | physicallybased.info, from Babar & Weaver Ag             | CC0     | same as M12 | 2026-11-16
M16 | Iron (PB)      | 0.530, 0.513, 0.494          | 192,190,187      | a    | physicallybased.info, from refractiveindex.info 3d/metals/iron | CC0 | same as M12 | 2026-11-16
M17 | Stainless steel (PB) | 0.669, 0.639, 0.598    | 214,209,203      | a    | physicallybased.info, refractiveindex.info 3d/metals/stainless_steel | CC0 | same as M12 | 2026-11-16
M18 | Brass (PB)     | 0.910, 0.778, 0.423          | 245,228,174      | a    | physicallybased.info, refractiveindex.info 3d/metals/brass | CC0  | same as M12 | 2026-11-16
M19 | Chromium / Nickel / Titanium (PB) | Cr 0.654,0.685,0.701; Ni 0.697,0.641,0.563; Ti 0.441,0.400,0.361 | — | a | physicallybased.info (Sytchkova Cr; Johnson Ni; Palm Ti) | CC0 | same as M12 | 2026-11-16
M20 | Zinc / Lead / Platinum / Cobalt (PB) | Zn 0.808,0.844,0.865; Pb 0.626,0.640,0.693; Pt 0.765,0.730,0.676; Co 0.699,0.704,0.671 | — | a | physicallybased.info | CC0 | same as M12 | 2026-11-16
M21 | Cast iron, real object (spectrophotometer) | 0.027, 0.026, 0.026; metalness 1; roughness 0.3 (authored) | 46,45,45 | b | physicallybased.info / Palmqvist measurement | CC0 | https://github.com/AntonPalmqvist/physically-based-api/tree/main/data/materials/cast-iron/ | 2026-11-16
M22 | Spot check of n,k at ~550 nm → normal-incidence R (my arithmetic, single wavelength; not an RGB F0) | Ag 0.984 (n 0.0517, k 3.483 @0.539 µm, Babar); Al 0.922 (n 0.593, k 5.286 @0.551, Cheng); Au 0.807 (n 0.329, k 2.249 @0.539, Babar); Cr 0.681 (n 3.598, k 4.898, Sytchkova); Cu 0.619 (n 1.02, k 2.577 @0.549, Johnson & Christy 1972); Ni 0.644 (n 1.92, k 3.61, J&C); Ti 0.404 (n 1.864, k 2.074, Palm) | — | a | refractiveindex.info database YAML files | CC0 1.0 | https://refractiveindex.info/?shelf=main&book=Cu&page=Johnson (and Au/Babar, Ag/Babar, Al/Cheng, Ni/Johnson, Cr/Sytchkova, Ti/Palm pages; YAML at https://github.com/polyanskiy/refractiveindex.info-database) | 2026-11-16
M22 matches the PB green channel within about 0.01–0.03 for each metal (e.g. Cu 0.619 vs 0.623, Ti 0.404 vs 0.400). This is a consistency check, not a new value.

### Measured metal BRDF fits (roughness), MERL 100 via Ngan 2005
Cook-Torrance fit columns are: diffuse d(R,G,B); specular s(R,G,B); p0 = F0; p1 = m (Beckmann RMS slope); fit error.
The diffuse/specular scale is the MERL radiometric scale used in the fit, not a calibrated albedo. Use these rows for m and the F0 trend only.
Licence for all MN rows: MERL data is research/academic only on the CSAIL page, but the Zenodo dump is CC BY-SA 4.0. That conflict is unresolved; see "Sources disagree".
Source for all MN rows: Ngan, Durand & Matusik 2005, supplemental (per-material tables). https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf. Accessed 2026-11-16.
ID  | MERL material         | F0 (p0) | m (p1)  | Fit error
MN1 | aluminium             | 0.59    | 0.00776 | 0.36
MN2 | chrome                | 0.991   | 0.00935 | 0.557
MN3 | chrome-steel          | 0.608   | 0.00718 | 0.4
MN4 | steel                 | 0.584   | 0.00714 | 0.314
MN6 | copper                | 0.999   | 0.00719 | 0.555
MN7 | brass                 | 0.596   | 0.00668 | 0.41
MN8 | nickel                | 0.733   | 0.0424  | 0.114
MN9 | tungsten-carbide      | 0.625   | 0.00559 | 0.569
MN10| black-oxidized-steel  | 0.035   | 0.19    | 0.00307
MN11| alum-bronze           | 0.203   | 0.0367  | 0.0434
Polished MERL metals have m ≈ 0.006–0.009, i.e. near-mirror. The fit errors for these are large (0.3–0.57), so the Cook-Torrance fit is poor for them. Treat the F0 values as unreliable for polished metals; use M1–M20 instead.

## 2. Dielectric F0 (specular) ranges

ID  | Material / class        | F0 (linear)                         | Source                                                       | Licence | URL | Accessed
D1  | Water                   | 0.02 (Hoffman); 0.020373188 (Lagarde) | Hoffman 2013 notes Table 1; Lagarde 2011                    | © (facts) | https://blog.selfshadow.com/publications/s2013-shading-course/hoffman/s2013_pbs_physics_math_notes.pdf ; https://seblagarde.wordpress.com/2011/08/17/feeding-a-physical-based-lighting-mode/ | 2026-11-16
D2  | Plastic / glass         | 0.03 (low) – 0.05 (high); glass 0.04 | Hoffman 2013 Table 1; Lagarde 2011                           | ©       | same | 2026-11-16
D3  | Gemstones               | ruby 0.077, diamond 0.172 (Lagarde); "glass high / ruby" 0.08, diamond 0.17 (Hoffman) | Lagarde 2011; Hoffman 2013 | © | same | 2026-11-16
D4  | Rule: non-gem dielectrics | 0.02–0.05; never below 0.02; use 0.04 if unknown | Hoffman 2013 text; Lagarde 2011 | © | same | 2026-11-16
D7  | Ice / milk / quartz / alcohol | 0.0179 / 0.0222 / 0.0456 / 0.0200 | Lagarde 2011                                             | ©       | https://seblagarde.wordpress.com/2011/08/17/feeding-a-physical-based-lighting-mode/ | 2026-11-16
D10 | Frostbite gemstone range | f0 about 8% (ruby) to 17% (diamond); Frostbite caps its reflectance parameter at 16% | Lagarde & de Rousiers 2014, "Moving Frostbite to PBR" v3.2, §3.2 | EA/authors © | https://seblagarde.files.wordpress.com/2015/07/course_notes_moving_frostbite_to_pbr_v32.pdf | 2026-11-16

## 3. Stone, concrete, asphalt, brick, soil (base colour / albedo)

ID  | Material              | Value                                   | sRGB8 (computed) | Kind | Source | Licence | URL | Accessed
S1  | Charcoal              | 0.02 (UE, intensity, polarised) / 0.04 (Lagarde, "includes some specular") | 39 / 56 | c→ | UE 4.27 docs; Lagarde 2011 (from Wikipedia albedo list) | © | https://dev.epicgames.com/documentation/en-us/unreal-engine/physically-based-materials?application_version=4.27 ; https://seblagarde.wordpress.com/2011/08/17/feeding-a-physical-based-lighting-mode/ | 2026-11-16
S2  | Fresh asphalt         | 0.02 (UE) / 0.04 (Lagarde)             | 39 / 56          | c    | same | © | same | 2026-11-16
S3  | Worn asphalt          | 0.08 (UE) / 0.12 (Lagarde)             | 80 / 97          | c    | same | © | same | 2026-11-16
S4  | Fresh / new concrete  | 0.51 (UE) / 0.55 (Lagarde)             | 189 / 196        | c    | same | © | same | 2026-11-16
S5  | Bare soil, green grass, desert sand, ocean ice, fresh snow | UE: 0.13, 0.21, 0.36, 0.56, 0.81; Lagarde: 0.17, 0.25, 0.40, 0.5–0.7, 0.80–0.90 | — | c | same | © | same | 2026-11-16
S6  | Diffuse range rule    | sRGB 50–240 for non-metal diffuse        | —                | —    | Lagarde 2011 | © | https://seblagarde.wordpress.com/2011/08/17/feeding-a-physical-based-lighting-mode/ | 2026-11-16
S7  | Concrete (USGS WTC01-37A), reflectance at 450/550/650 nm | 0.452 / 0.520 / 0.555 | ~191 at 550 nm | b | USGS splib05a, Clark et al. 2003 OFR 03-395 | US Gov, public domain | https://pubs.usgs.gov/of/2003/ofr-03-395/ASCII/A/concrete_wtc01-37a.9797.asc | 2026-11-16
S8  | Asphalt "Blck_Road old" (USGS GDS376) at 450/550/650 nm | 0.069 / 0.086 / 0.103 | ~83 | b | USGS splib05a | public domain | https://pubs.usgs.gov/of/2003/ofr-03-395/ASCII/A/black_old_asphaltroof_gds376.9737.asc | 2026-11-16
S9  | Limestone (USGS CU02-11A) at 450/550/650 nm | 0.158 / 0.191 / 0.214 | ~121 | b | USGS splib05a | public domain | https://pubs.usgs.gov/of/2003/ofr-03-395/ASCII/S/limestone_cu02-11a.8575.asc | 2026-11-16
S10 | Asphalt fresh (PB)    | lin 0.0429, 0.0414, 0.0401; authored roughness 0.5; IOR 1.6 → F0 0.053 (computed) | 58,57,56 | b | physicallybased.info (cites SpectralDB 00117, Jakubiec) | CC0 (SpectralDB's own licence not stated) | https://api.physicallybased.info/v2/materials ; https://spectraldb.com/measurements/00117/ | 2026-11-16
S11 | Concrete (PB)         | lin 0.51 ×3 (taken from Lagarde/UE, not a new measurement) | 189 | c | physicallybased.info | CC0 | https://api.physicallybased.info/v2/materials | 2026-11-16
S12 | Brick (PB, spectrophotometer, 1 sample) | lin 0.262, 0.095, 0.061 | 140,87,70 | b | physicallybased.info / Palmqvist | CC0 | https://github.com/AntonPalmqvist/physically-based-api/tree/main/data/materials/brick/ | 2026-11-16
S13 | Terracotta / clay (PB, spectrophotometer) | lin 0.555,0.212,0.110 / 0.256,0.118,0.091 | 196,127,93 / 138,96,85 | b | physicallybased.info | CC0 | https://api.physicallybased.info/v2/materials | 2026-11-16
S14 | Marble (PB = Jensen 2001 diffuse reflectance Rd) | lin 0.830, 0.791, 0.753; IOR 1.5 → F0 0.04 | 235,230,225 | b | physicallybased.info, citing Jensen et al. 2001 (SIGGRAPH, BSSRDF) | CC0 (Jensen paper ACM ©) | https://api.physicallybased.info/v2/materials | 2026-11-16
S15 | Sand (PB)             | lin 0.440, 0.386, 0.231                  | 177,167,132      | b    | physicallybased.info (cites SpectralDB 00464 and Lagarde) | CC0 | same as S14 | 2026-11-16
S16 | Rust (PB, spectrophotometer) | lin 0.181, 0.057, 0.017; non-metal | 118,68,35 | b | physicallybased.info | CC0 | same as S14 | 2026-11-16

ECOSTRESS spectral library rows (S17–S34), added 2026-11-17. Value = reflectance in **percent** at 450 / 550 / 650 nm, plus the mean over the 0.40–0.70 µm channels (n = 301 points per spectrum; mean computed by me). One rock sample per row; no spread is given per sample. Measurement: JHU "becknic" files = directional (10°) hemispherical reflectance; JPL "perkin" files = hemispherical reflectance; JHU quotes ±3 % absolute accuracy for its directional-hemispherical data. Kind b (lab spectrophotometer, not RGB). Licence: ECOSTRESS home page says "Copyright © 2017, California Institute of Technology. ALL RIGHTS RESERVED" with a courtesy credit line — **[NO-REDISTRIBUTION]** (cite facts; do not copy files into a repo).

ID  | Rock sample (ECOSTRESS name, origin) | R% 450 / 550 / 650 nm; VIS mean | Kind | Source | Licence | URL | Accessed
S17 | Granite_H1, alkalic granite, Quincy MA (Ward's W-4) | 15.47 / 17.01 / 16.24; VIS mean 16.23 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.igneous.felsic.solid.all.granite_h1.jhu.becknic.spectrum.txt | 2026-11-17
S18 | Granite_H2, Santa Rita Mts AZ (NMNH 113640-17) | 14.54 / 18.73 / 24.70; VIS mean 19.51 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.igneous.felsic.solid.all.granite_h2.jhu.becknic.spectrum.txt | 2026-11-17
S19 | Granite_H3, Santa Rita Mts AZ (NMNH 113640-13) | 19.26 / 21.33 / 22.16; VIS mean 20.85 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.igneous.felsic.solid.all.granite_h3.jhu.becknic.spectrum.txt | 2026-11-17
S20 | Granite_H5, Santa Rita Mts AZ (NMNH 113640-15), pinkish | 23.70 / 27.53 / 32.41; VIS mean 27.91 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.igneous.felsic.solid.all.granite_h5.jhu.becknic.spectrum.txt | 2026-11-17
S21 | White marble, West Rutland VT (Ward95) | 69.60 / 74.42 / 75.34; VIS mean 72.82 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.metamorphic.marble.solid.vswir.ward95.jpl.perkin.spectrum.txt | 2026-11-17
S22 | Pink marble, Tate GA (Ward96) | 29.95 / 35.69 / 40.35; VIS mean 35.43 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.metamorphic.marble.solid.vswir.ward96.jpl.perkin.spectrum.txt | 2026-11-17
S23 | Dolomite marble, Thornwood NY (Ward97) | 40.44 / 47.12 / 50.89; VIS mean 45.94 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.metamorphic.marble.solid.vswir.ward97.jpl.perkin.spectrum.txt | 2026-11-17
S24 | Dolomitic marble, coarse (Hunt & Salisbury No. 459) | 36.50 / 36.67 / 34.99; VIS mean 36.02 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.metamorphic.marble.coarse.all.marble1.jhu.becknic.spectrum.txt | 2026-11-17
S25 | Dolomitic marble, coarse (Hunt & Salisbury No. 458) | 65.89 / 69.96 / 69.27; VIS mean 68.04 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.metamorphic.marble.coarse.all.marble4.jhu.becknic.spectrum.txt | 2026-11-17
S26 | Gray sandstone, Berea OH (Ward58) | 35.29 / 42.63 / 44.76; VIS mean 40.66 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.sedimentary.sandstone.solid.vswir.ward58.jpl.perkin.spectrum.txt | 2026-11-17
S27 | Sandstone (flagstone), Masonville NY (Ward61) | 15.20 / 17.13 / 17.26; VIS mean 16.47 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.sedimentary.sandstone.solid.vswir.ward61.jpl.perkin.spectrum.txt | 2026-11-17
S28 | Arkosic sandstone, coarse (Hunt & Salisbury No. 362) | 23.82 / 35.23 / 42.14; VIS mean 33.37 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.sedimentary.sandstone.coarse.all.sandstone_1.jhu.becknic.spectrum.txt | 2026-11-17
S29 | Gray slate, near Bangor PA (Ward92) | 9.87 / 9.55 / 9.21; VIS mean 9.54 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.metamorphic.slate.solid.vswir.ward92.jpl.perkin.spectrum.txt | 2026-11-17
S30 | Red slate, Granville NY (Ward91) | 15.15 / 16.16 / 24.58; VIS mean 19.07 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.metamorphic.slate.solid.vswir.ward91.jpl.perkin.spectrum.txt | 2026-11-17
S31 | Gray slate, coarse (Hunt & Salisbury No. 307) | 18.51 / 19.54 / 18.62; VIS mean 18.78 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.metamorphic.slate.coarse.all.slate1.jhu.becknic.spectrum.txt | 2026-11-17
S32 | Basalt_H1, Chimney Rock NJ (Ward's W-36) | 3.88 / 4.32 / 4.50; VIS mean 4.15 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.igneous.mafic.solid.all.basalt_h1.jhu.becknic.spectrum.txt | 2026-11-17
S33 | Basalt_H2, Juan de Fuca Ridge (NMNH 111276) | 7.92 / 7.76 / 8.02; VIS mean 7.85 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.igneous.mafic.solid.all.basalt_h2.jhu.becknic.spectrum.txt | 2026-11-17
S34 | Basalt_7, Kilauea (KI75-1-50.8) | 15.26 / 17.04 / 17.36; VIS mean 16.68 | b | ECOSTRESS v1 (Meerdink et al. 2019), spectrum file | Caltech © all rights reserved | https://speclib.jpl.nasa.gov/ecospeclibdata/rock.igneous.mafic.solid.all.basalt_7.jhu.becknic.spectrum.txt | 2026-11-17
Spread by rock type (VIS mean, these samples only): granite 16.2–27.9 % (n = 4); marble 35.4–72.8 % (n = 5; white marble 72.8 %); sandstone 16.5–40.7 % (n = 3); slate 9.5–19.1 % (n = 3); basalt 4.1–16.7 % (n = 3). Reflectance ≈ diffuse albedo here; it is not linear-sRGB base colour (no colour-matching was done). [https://speclib.jpl.nasa.gov/ · accessed 2026-11-17]

## 4. Wood

ID  | Material                             | Value                                  | Source | Licence | URL | Accessed
W1  | Fresh pine plywood (USGS GDS365), reflectance 450/550/650 nm | 0.202 / 0.322 / 0.560 (sRGB8 at 550 nm ≈ 154, computed) | USGS splib05a | public domain | https://pubs.usgs.gov/of/2003/ofr-03-395/ASCII/A/plywood_gds365.10499.asc | 2026-11-16
W2  | MERL finished woods: Cook-Torrance F0 / m | cherry-235 0.0441 / 0.139; fruitwood-241 0.0233 / 0.0924; ipswich-pine-221 0.0192 / 0.12; red-oak-215 0.0248 / 0.116; special-walnut-224 0.0688 / 0.258; walnut-dark-2716 0.123 / 0.0844; pickled-oak-260 0.999 / 0.158 (fit outlier) | Ngan et al. 2005 supplemental, per-material tables | MERL data: research-only vs Zenodo CC BY-SA (conflict) | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-16
W3  | MERL wood fit errors                 | 0.0035–0.017 (good fits, unlike the metals) | same | same | same | 2026-11-16
For a generator, finished (varnished) wood measures as F0 ≈ 0.02–0.07 with Beckmann m ≈ 0.09–0.26. Under the √m mapping that is perceptual roughness ≈ 0.3–0.5 (my interpretation). The source gives no raw-versus-varnished pair.

## 5. Cloth / fabric

ID  | Material (UCSD gantry, one swatch each) | Lambert ρd (fit error) | Torrance-Sparrow ρd / ks / m (error), IOR fixed 1.5 | Source | Licence | URL | Accessed
C1  | Cotton broadcloth          | 0.600 (0.134) | 0.794 / 0.357 / 0.396 (0.0532) | De Deken 2010, MS thesis UCSD, "A BRDF analysis of cloth", Tables 5.1 and 5.3 | thesis © author (facts only) | https://escholarship.org/uc/item/9zh862bt | 2026-11-16
C2  | Cotton canvas              | 0.538 (0.140) | 0.667 / 0.333 / 0.371 (0.0521) | same | same | same | 2026-11-16
C3  | Cotton twill               | 0.704 (0.106) | 0.788 / 0.255 / 0.395 (0.0417) | same | same | same | 2026-11-16
C4  | Cotton jersey knit         | 0.648 (0.142) | 0.742 / 0.244 / 0.312 (0.0503) | same | same | same | 2026-11-16
C5  | Linen light / medium / heavy | 0.643 / 0.480 / 0.446 | m 0.278 / 0.331 / 0.316 | same | same | same | 2026-11-16
C6  | Wool flannel / suiting / tropical | 0.323 / 0.359 / 0.289 | m 0.275 / 0.290 / 0.285 | same | same | same | 2026-11-16
C7  | Silk crepe de chine / shantung | 0.457 / 0.414 | m 0.553 / 0.354 | same | same | same | 2026-11-16
C8  | Nylon activewear / swimwear | 0.684 / 0.818 | m 0.255 / 0.251 | same | same | same | 2026-11-16
C9  | Polyester interlock        | 0.772 (0.262) | 0.874 / 0.288 / 0.244 (0.0790) | same | same | same | 2026-11-16
C10 | Velvet stretch / cotton velvet | 0.586 / 0.456 | m 1.000 / 0.219 | same (Oren-Nayar velvet stretch: ρd 0.628, σ 0.289) | same | same | 2026-11-16
C11 | Satins (silk charmeuse, polyester satin) | ρd fits saturate at 1.000 with high error (0.38–0.44): the model fails | — | same | same | same | 2026-11-16
C12 | MERL fabrics, Cook-Torrance F0 / m | fabric-beige 0.158 / 0.741; fabric-red 0.297 / 0.442; fabric-pink-felt 0.129 / 0.643; fabric-green 0.0446 / 0.558; nylon 0.0423 / 0.0662 | Ngan 2005 supplemental | MERL conflict (see Sources disagree) | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-16
Notes:
- The thesis gives scalar ρd values. The table does not state the colour channel.
- The thesis says none of the cloths are Lambertian, and that grazing-angle sheen dominates the error. Use a sheen lobe rather than plain microfacet for cloth.

## 5b. Plastics and everyday objects (added 2026-11-17)

### Measured plastic BRDF fits — MERL 100 via Ngan, Durand & Matusik 2005 supplemental, Cook-Torrance row
Same conventions and caveats as the MN rows: p0 = F0, p1 = Beckmann m, diffuse on the MERL radiometric scale (not calibrated albedo). One physical sample each; no spread. Licence: MERL Copyright_Notice.txt (opened 2026-11-17): "Copyright 2006 Mitsubishi Electric Research Laboratories All Rights Reserved. Permission to use, copy and modify … for educational, research and non-profit purposes, is hereby granted"; commercial products need permission from MERL — **[NC]** (Zenodo CC BY-SA conflict: see Sources disagree).

ID  | MERL material | Diffuse d (R,G,B) | F0 (p0) | m (p1) | Fit error | Supplemental PDF page | URL | Accessed
PL1 | acrylic-blue | 0.0143, 0.033, 0.0639 | 0.117 | 0.0137 | 0.0155 | p. 3 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL2 | acrylic-green | 0.0173, 0.0646, 0.0243 | 0.0832 | 0.0111 | 0.02 | p. 5 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL3 | acrylic-violet | 0.0689, 0.0246, 0.029 | 0.0726 | 0.0129 | 0.0274 | p. 7 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL4 | acrylic-white | 0.313, 0.258, 0.155 | 0.0368 | 0.0119 | 0.0221 | p. 9 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL5 | acrylic-yellow | 0.225, 0.233, 0.0489 | 0.399 (high; treat as fit artefact) | 0.00882 | 0.0188 | p. 11 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL6 | black-phenolic | — | 0.0388 | 0.023 | 0.115 | p. 27 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL7 | black-plastic-soft | — | 0.136 | 0.325 | 0.00177 | p. 29 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL8 | delrin | 0.3, 0.251, 0.151 | 0.052 | 0.143 | 0.00545 | p. 61 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL9 | gray-plastic | — | 0.0654 | 0.026 | 0.0178 | p. 85 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL10 | green-plastic | — | 0.999 (fit outlier) | 0.00954 | 0.0483 | p. 91 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL11 | maroon-plastic | — | 0.999 (fit outlier) | 0.0094 | 0.0595 | p. 99 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL12 | phenolic-yellow | — | 0.153 | 0.0196 | 0.0529 | p. 131 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL13 | pink-plastic | — | 0.0282 | 0.352 | 0.00424 | p. 137 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL14 | polyethylene | 0.236, 0.214, 0.147 | 0.0855 | 0.643 | 0.00641 | p. 139 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL15 | polyurethane | — | 0.0509 | 0.701 | 0.00126 | p. 143 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL16 | pvc | — | 0.0277 | 0.0387 | 0.0163 | p. 155 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL17 | red-phenolic | — | 0.12 | 0.0212 | 0.0402 | p. 163 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL18 | red-plastic | — | 0.085 | 0.262 | 0.00376 | p. 165 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL19 | red-plastic-specular | — | 0.0978 | 0.0157 | 0.044 | p. 167 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL20 | teflon | 0.308, 0.266, 0.182 | 0.0203 | 0.139 | 0.00639 | p. 181 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL21 | yellow-matte-plastic | — | 0.056 | 0.0489 | 0.0158 | p. 199 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
PL22 | yellow-plastic | — | 0.0431 | 0.282 | 0.00409 | p. 201 | https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf | 2026-11-17
Reading: excluding the two 0.999 outliers and acrylic-yellow, plastic F0 spans 0.020 (teflon) – 0.153 (phenolic-yellow), median 0.065 (my arithmetic over those 19 rows); glossy plastics have m ≈ 0.01–0.05 and matte/soft plastics m ≈ 0.26–0.70. Many fitted F0 values exceed the 0.04–0.05 an IOR of 1.5–1.6 implies; Cook-Torrance F0 here absorbs fit error and is not an optical constant. [https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf · accessed 2026-11-17]

### Plastic IOR → F0 — physicallybased.info API v2 (CC0 1.0)
Colour 1,1,1 and roughness 0 are authored placeholders; the IOR values cite Wikipedia / refractiveindex.info. F0 = ((n−1)/(n+1))² computed by me.

ID  | Plastic | IOR (source) | F0 (computed) | URL | Accessed
PI1 | Acrylic (PMMA) | 1.4905 | 0.0388 | https://api.physicallybased.info/v2/materials | 2026-11-17
PI2 | Polycarbonate (PC) | 1.5848 | 0.0512 | https://api.physicallybased.info/v2/materials | 2026-11-17
PI3 | PET | 1.575 | 0.0499 | https://api.physicallybased.info/v2/materials | 2026-11-17
PI4 | PLA | 1.4586 | 0.0348 | https://api.physicallybased.info/v2/materials | 2026-11-17
PI5 | Polypropylene (PP) | 1.492 | 0.0390 | https://api.physicallybased.info/v2/materials | 2026-11-17
PI6 | Polyurethane (PUR) | 1.6 | 0.0533 | https://api.physicallybased.info/v2/materials | 2026-11-17
PI7 | PVC | 1.542 | 0.0455 | https://api.physicallybased.info/v2/materials | 2026-11-17

### Everyday objects measured with a spectrophotometer — physicallybased.info API v2 (CC0 1.0)
Colour = linear sRGB as published; roughness is **authored** (not measured). One object each; no spread. Underlying measurements: Palmqvist (spectrophotometer, own measurements) or Jakubiec Spectral Materials Database (SpectralDB, licence not stated — **[NO-LICENCE]** for the underlying spectra).

ID  | Object | Colour (lin sRGB) | Roughness (authored) | IOR | Measured by | URL | Accessed
E1 | Cardboard | 0.351, 0.208, 0.110 | 0.5 | 1.5 | Palmqvist | https://api.physicallybased.info/v2/materials | 2026-11-17
E2 | Tire (rubber) | 0.023 ×3 | 0.7 | 1.5 | Palmqvist | https://api.physicallybased.info/v2/materials | 2026-11-17
E3 | Office paper | 0.794, 0.834, 0.884 | 0.8 | 1.5 | Palmqvist | https://api.physicallybased.info/v2/materials | 2026-11-17
E4 | Porcelain | 0.745, 0.745, 0.723 | 0 | 1.5 | Palmqvist | https://api.physicallybased.info/v2/materials | 2026-11-17
E5 | Toner (black) | 0.05 ×3 | 0.5 | 1.5 | Palmqvist | https://api.physicallybased.info/v2/materials | 2026-11-17
E6 | Egg shell (brown) | 0.493, 0.248, 0.123 | 0.9 | 1.5 | Palmqvist | https://api.physicallybased.info/v2/materials | 2026-11-17
E7 | Egg shell (white) | 0.610, 0.624, 0.631 | 0.9 | 1.5 | Palmqvist | https://api.physicallybased.info/v2/materials | 2026-11-17
E8 | Polystyrene foam | 0.8393, 0.8384, 0.841 | 0.5 | 1.6 | SpectralDB 00090 (Jakubiec) | https://api.physicallybased.info/v2/materials | 2026-11-17
E9 | Whiteboard | 0.869, 0.867, 0.771 | 0 | 1.5 | SpectralDB 00074 (Jakubiec) | https://api.physicallybased.info/v2/materials | 2026-11-17
E10 | Blackboard | 0.039 ×3 | 0.9 | 1.5 | SpectralDB 00079 (Jakubiec) | https://api.physicallybased.info/v2/materials | 2026-11-17
Not measured in the same API (do not treat as data): Car Paint 0.1 ×3 (cites Wikipedia) and Gray Card 0.18 (definition). [https://api.physicallybased.info/v2/materials · accessed 2026-11-17]

## 6. Leather
No measured leather albedo, roughness or F0 was found in an opened source (re-checked 2026-11-17): the RGL-EPFL material list (CC0) has no leather entry; the 100 MERL materials (Ngan supplemental) have no leather; physicallybased.info API v2 has no leather entry. A measured BTF of artificial leather ("leather01") exists in the UTIA BTF database (Filip, Vávra, Krupička, *Sensors* 2014, 14:19785–19805, paper CC BY 4.0), but the UTIA site returned HTTP 500 and no scalar values are tabulated in the paper. Fallback rule only: dielectric F0 0.02–0.05, 0.04 if unknown (D4). [https://rgl.epfl.ch/materials ; https://www.ncbi.nlm.nih.gov/pmc/articles/PMC4239885/ · accessed 2026-11-17]

## 7. Skin and skin-adjacent — moved out of scope
Skin, eye, hair, nail, tooth, blood and bone values were removed from this file on 2026-11-17 (out of scope for I3; see B7-skin-eye-hair-colour.md). Former rows D5, D6, D8, D9 and K1–K11 are gone.

## Source list (items 1–13 accessed 2026-11-16; items 14–17 and re-checks accessed 2026-11-17)

1. physicallybased.info / physically-based-api, A. Palmqvist. API v2 materials JSON; header says "license: CC0 1.0 Universal", updated 2026-11-03.
   https://physicallybased.info ; https://api.physicallybased.info/v2/materials ; https://github.com/AntonPalmqvist/physically-based-api
   Licence: CC0. Commercial use and redistribution allowed.
2. Lagarde S. "Feeding a physically based shading model", blog post, 2011-08-17.
   https://seblagarde.wordpress.com/2011/08/17/feeding-a-physical-based-lighting-mode/
   Licence: blog ©; facts only.
3. Hoffman N. "Background: Physics and Math of Shading", SIGGRAPH 2013 PBS course notes, Table 1 (credited to Real-Time Rendering 3rd ed.).
   https://blog.selfshadow.com/publications/s2013-shading-course/hoffman/s2013_pbs_physics_math_notes.pdf
   Licence: ©; facts only.
4. Lagarde S., de Rousiers C. "Moving Frostbite to Physically Based Rendering 3.0", SIGGRAPH 2014 course notes v3.2.
   https://seblagarde.files.wordpress.com/2015/07/course_notes_moving_frostbite_to_pbr_v32.pdf
   Licence: EA/authors ©.
5. Epic Games. "Physically Based Materials", Unreal Engine 4.27 documentation.
   https://dev.epicgames.com/documentation/en-us/unreal-engine/physically-based-materials?application_version=4.27
   Licence: Epic ©; facts only.
6. Karis B. "Real Shading in Unreal Engine 4", SIGGRAPH 2013 notes.
   https://blog.selfshadow.com/publications/s2013-shading-course/karis/s2013_pbs_epic_notes_v2.pdf
   Link opened (HTTP 200). No numbers taken directly; UE docs cite it.
7. Polyanskiy M. N. refractiveindex.info database: Johnson & Christy 1972 (Cu, Ni), Babar & Weaver 2015 (Au, Ag), Cheng (Al), Sytchkova (Cr), Palm (Ti).
   https://refractiveindex.info ; https://github.com/polyanskiy/refractiveindex.info-database
   Licence: CC0 1.0 (repository LICENSE file).
8. Matusik W., Pfister H., Brand M., McMillan L. "A Data-Driven Reflectance Model", ACM TOG 22(3), 2003 (MERL BRDF database).
   https://cdfg.csail.mit.edu/wojciech/brdfdatabase ("MERL provides this data for research and academic use only")
   Zenodo record https://zenodo.org/records/8101681 (API shows licence cc-by-sa-4.0, published 2023-06-30). The DOI https://doi.org/10.5281/zenodo.8101680 returned 403 to curl.
9. Ngan A., Durand F., Matusik W. "Experimental Analysis of BRDF Models", EGSR 2005, plus supplemental document.
   https://people.csail.mit.edu/addy/research/brdf/ ; https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf
   Licence: © authors/MERL.
10. Clark R. N. et al. 2003, USGS Digital Spectral Library splib05a, Open-File Report 03-395.
    https://pubs.usgs.gov/of/2003/ofr-03-395/datatable.html
    Licence: US Government work, public domain.
11. De Deken J. "A BRDF analysis of cloth", MS thesis, UC San Diego, 2010.
    https://escholarship.org/uc/item/9zh862bt (PDF https://escholarship.org/content/qt9zh862bt/qt9zh862bt.pdf)
    Licence: © author.
13. Jakubiec J. A. Spectral Materials Database.
    https://spectraldb.com/measurements/00117/
    Licence: none stated on the site home page; cited by physicallybased.info.

14. ECOSTRESS Spectral Library v1, JPL/Caltech (Meerdink S. K. et al. 2019, *Remote Sensing of Environment* 230:111196; Baldridge A. M. et al. 2009, RSE 113:711–715).
    https://speclib.jpl.nasa.gov/ ; files under https://speclib.jpl.nasa.gov/ecospeclibdata/ (individual URLs in S17–S34). Accessed 2026-11-17.
    Licence: "Copyright © 2017, California Institute of Technology. ALL RIGHTS RESERVED." **[NO-REDISTRIBUTION]**; cite facts only.
15. RGL-EPFL Material Database (Dupuy & Jakob 2018, ACM TOG 37(6)).
    https://rgl.epfl.ch/materials . Accessed 2026-11-17.
    Licence: CC0 (page links creativecommons.org/publicdomain/zero/1.0). Used only to confirm there is no leather entry.
16. MERL BRDF database copyright notice.
    https://people.csail.mit.edu/wojciech/BRDFDatabase/Copyright_Notice.txt . Accessed 2026-11-17.
    Licence: research/educational/non-profit only **[NC]**.
17. Filip J., Vávra R., Krupička M. "Rapid material appearance acquisition using consumer hardware", *Sensors* 14:19785–19805, 2014.
    https://www.ncbi.nlm.nih.gov/pmc/articles/PMC4239885/ . Accessed 2026-11-17. Paper CC BY 4.0; UTIA BTF data terms not readable (site 500).

## Not found

- **Leather:** no measured albedo, roughness or F0 in any opened source (RGL, MERL, physicallybased.info checked 2026-11-17). Leads not usable: UTIA BTF leather01 (site HTTP 500; data are BTF images, not scalars); UTIA BRDF leather03 (Filip, Havran, Myszkowski 2016 workshop PDF, not extracted); Bonn BTF leather (site JS-only, licence unknown); Microsoft "Pocket Reflectometry" supplemental lists a leather school bag (numbers not extracted); ISO 17502:2013 and ISO 22700:2019 are test methods, not data.
- **Wood:** no measured raw-versus-varnished pair for the same species. Species albedos for oak, teak and walnut as calibrated RGB: none. MERL woods give fits only.
- **Stone:** granite, marble, sandstone, slate and basalt spectra now added from ECOSTRESS (S17–S34). Still missing: measured roughness/gloss for polished vs rough-cut stone, and colour-matched (linear sRGB) stone base colours from a measurement source. USGS splib07a has no granite, marble, sandstone or slate entries (Grok lead, not re-checked by me).
- **Cloth:**
  - Denim: none.
  - RGL-EPFL (CC0) has cloth samples (e.g. acrylic felt, satin) but its data are tabulated BRDF files, not scalar values; no numbers were extracted. UTIA BTF fabrics not opened (site 500).
  - Fibre IORs for cotton, silk, wool and polyester (grok lead: Aliaga et al. 2016 poster) were not opened by me.
- **Ngan conventions:** the absolute scale of the Ngan diffuse coefficients (whether ρd/π or albedo) and the MERL colour channel conventions were not confirmed.
- **Measured roughness:** no measured roughness for "rough" metals (brushed, sand-blasted, rusty). The PB values are authored. The Ngan brushed-aluminium anisotropic data exists as raw data only.

## Sources disagree

- **MERL licence:**
  - The MERL Copyright_Notice.txt (opened 2026-11-17) grants use "for educational, research and non-profit purposes" only; commercial use needs MERL's permission. The CSAIL page says research and academic use only.
  - The Zenodo 2023 record lists CC BY-SA 4.0.
  - Until MERL confirms, treat MERL-derived values as research-only for shipping. The values in W2, C12 and MN are small numeric facts, but the safest course is not to ship data derived from the BRDF files.
- **Gold F0:**
  - Lagarde/UE 1, 0.766, 0.336.
  - physicallybased.info 1.059, 0.773, 0.307 (out of gamut).
  - Hoffman/RTR 1.00, 0.71, 0.29.
  - The differences come from different n,k datasets and integration methods.
- **Iron F0:**
  - Hoffman/UE 0.56, 0.57, 0.58.
  - physicallybased.info 0.530, 0.513, 0.494 (warmer, darker).
- **Chromium F0:**
  - Lagarde 0.550, 0.556, 0.554.
  - physicallybased.info 0.654, 0.685, 0.701.
  - Spot check from Sytchkova n,k at 550 nm gives 0.681, which supports physicallybased.info.
  - Lagarde's source data is unknown (filmetrics).
- **Titanium F0:**
  - Lagarde 0.542, 0.497, 0.449.
  - physicallybased.info 0.441, 0.400, 0.361.
  - Palm n,k at 550 nm gives 0.404, which supports physicallybased.info.
- **Albedo of charcoal, asphalt and concrete:**
  - UE values (0.02, 0.02/0.08, 0.51) are lower than Lagarde's (0.04, 0.04/0.12, 0.55).
  - UE's values are "polarised" (specular removed); Lagarde's include specular.
  - USGS old road asphalt at 550 nm is 0.086, between fresh and worn.
- **Cloth albedo depends on the model:**
  - Cotton broadcloth ρd is 0.600 under a Lambert fit but 0.794 under a Torrance-Sparrow fit (same data).
  - Pick one model and keep its parameters together.

- **White marble albedo:** physicallybased.info (Jensen 2001 diffuse reflectance) 0.830, 0.791, 0.753 vs ECOSTRESS white marble Ward95 72.8 % VIS mean (69.6–75.3 % at 450–650 nm). Different samples and quantities (subsurface Rd vs hemispherical reflectance); the ECOSTRESS marbles range 35.4–72.8 % across 5 samples. [https://api.physicallybased.info/v2/materials ; https://speclib.jpl.nasa.gov/ecospeclibdata/rock.metamorphic.marble.solid.vswir.ward95.jpl.perkin.spectrum.txt · accessed 2026-11-17]
- **Plastic F0:** IOR-derived F0 for common plastics is 0.035–0.053 (physicallybased.info IORs), but Ngan's Cook-Torrance fits to measured MERL plastics give 0.020–0.153 (and 0.999 outliers). The fitted value mixes Fresnel with fit error; use IOR-derived F0 for shading and the MERL m for roughness. [https://people.csail.mit.edu/addy/research/ngan05_brdf_supplemental_doc.pdf ; https://api.physicallybased.info/v2/materials · accessed 2026-11-17]
