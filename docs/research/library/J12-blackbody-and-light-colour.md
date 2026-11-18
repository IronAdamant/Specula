# J12: Blackbody colour, colour temperature of light sources

Fixed after the second check (2026-11-18):
- J12-2: the CIE dataset's licence is CC BY-SA 4.0 (the metadata file beside each CSV), not "not stated".
- J12-3: the Sun's 128,000 lx is the database's Space variant (above the atmosphere), not illuminance on the ground; the Midday variant (100,000 lx) and Sunset variant (3,000 lx at 2,600 K) added, and the notes corrected.
- J12-3: the other variants the database lists added (Sky: Overcast Day, Moonless Clear Night; Moon: Crescent; Arri M8: Medium, Spot; Car Headlight: Halogen; Fluorescent: 4,000 and 6,500 K; LED: White); the car headlight row shown is the LED variant, and its reference is the Aftermarket LED Kits Chart.
- Moved under "Unconfirmed": the spot check's "usual Planckian-locus value" and "daylight on the ground ... 5,500-6,500 K", both unsourced.
- The Sun's 128 against 100 klx added under "Sources disagree"; a "Sources" list added at the end.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Values marked "computed by me" are arithmetic, never measurements.

## J12-1. Constants (read directly)

| Constant | Value | Source |
|---|---|---|
| Second radiation constant c2 | 1.438776877 × 10⁻² m·K | CODATA, NIST, https://physics.nist.gov/cgi-bin/cuu/Value?c22ndrc |
| First radiation constant c1 | 3.741771852 × 10⁻¹⁶ W·m² | CODATA, NIST, https://physics.nist.gov/cgi-bin/cuu/Value?c11strc |
| Wien displacement constant b | 2.897771955 × 10⁻³ m·K | CODATA, NIST, https://physics.nist.gov/cgi-bin/cuu/Value?bwien |
| Luminous efficacy at 540 THz (about 555 nm) | 683 lm/W exactly (defines the candela) | BIPM, *The International System of Units* (SI Brochure, 9th ed., 2019), section 2.3.1, https://www.bipm.org/en/publications/si-brochure |

(All three CODATA values were read from the NIST pages listed on 2026-11-17; they are exact because h, c and k are fixed in the SI.)

## J12-2. Blackbody colour against temperature (computed by me)

Method: Planck's law with the constants above, integrated against the CIE 1931 2° colour-matching functions at 1 nm steps, 360-830 nm (CIE 018:2019 Table 6, dataset DOI 10.25039/CIE.DS.xvudnb9b, file https://files.cie.co.at/Publications-datasets/CIE_xyz_1931_2deg.csv; licence CC BY-SA 4.0, given as the rightsIdentifier in the metadata file beside each CSV, for example https://files.cie.co.at/Publications-datasets/CIE_xyz_1931_2deg.csv_metadata.json, which also confirms the table numbers: xyz Table 6, photopic Table 1, scotopic Table 2; the tables may therefore be redistributed with attribution and under the same licence, though they are not copied here). XYZ was converted to linear sRGB with the IEC 61966-2-1 matrix (D65 white), negative channels clipped to 0, scaled so the largest channel is 1, then sRGB-encoded for the hex value. Because the white point is D65, a 6,500 K blackbody comes out nearly white; lower temperatures look orange, higher ones blue. Rows marked "clipped" are outside the sRGB gamut and lose saturation.

| T (K) | Peak wavelength (nm), b/T | CIE x | CIE y | Linear sRGB (R, G, B) | sRGB hex | Note |
|---|---|---|---|---|---|---|
| 800 | 3,622 (infrared) | 0.6813 | 0.3179 | 1.000, 0.000, 0.000 | #FF0000 | clipped; first dull red glow |
| 1,000 | 2,898 | 0.6528 | 0.3445 | 1.000, 0.009, 0.000 | #FF1700 | clipped |
| 1,200 | 2,415 | 0.6250 | 0.3675 | 1.000, 0.058, 0.000 | #FF4400 | clipped |
| 1,500 | 1,932 | 0.5857 | 0.3931 | 1.000, 0.133, 0.000 | #FF6600 | clipped |
| 1,800 | 1,610 | 0.5492 | 0.4082 | 1.000, 0.208, 0.000 | #FF7E00 | clipped; about candle light |
| 2,000 | 1,449 | 0.5267 | 0.4133 | 1.000, 0.257, 0.008 | #FF8B16 | |
| 2,500 | 1,159 | 0.4770 | 0.4137 | 1.000, 0.372, 0.068 | #FFA449 | |
| 2,700 | 1,073 | 0.4599 | 0.4106 | 1.000, 0.415, 0.099 | #FFAD59 | warm household lamp |
| 3,000 | 966 | 0.4369 | 0.4041 | 1.000, 0.477, 0.154 | #FFB86D | |
| 3,500 | 828 | 0.4053 | 0.3907 | 1.000, 0.571, 0.259 | #FFC78B | |
| 4,000 | 724 | 0.3804 | 0.3767 | 1.000, 0.654, 0.377 | #FFD3A5 | |
| 4,500 | 644 | 0.3608 | 0.3635 | 1.000, 0.727, 0.502 | #FFDEBC | |
| 5,000 | 580 | 0.3451 | 0.3516 | 1.000, 0.792, 0.628 | #FFE6D0 | |
| 5,500 | 527 | 0.3324 | 0.3410 | 1.000, 0.849, 0.754 | #FFEDE1 | |
| 5,778 | 502 | 0.3264 | 0.3357 | 1.000, 0.877, 0.822 | #FFF1EA | Sun's effective temperature as used by physicallybased.info |
| 6,000 | 483 | 0.3221 | 0.3318 | 1.000, 0.899, 0.875 | #FFF3F0 | |
| 6,500 | 446 | 0.3135 | 0.3236 | 1.000, 0.943, 0.992 | #FFF8FE | near the D65 white |
| 7,000 | 414 | 0.3064 | 0.3165 | 0.906, 0.890, 1.000 | #F4F2FF | |
| 8,000 | 362 | 0.2952 | 0.3048 | 0.765, 0.802, 1.000 | #E3E7FF | |
| 10,000 | 290 | 0.2806 | 0.2883 | 0.609, 0.695, 1.000 | #CDD9FF | blue sky value used by physicallybased.info |
| 15,000 | 193 | 0.2637 | 0.2673 | 0.460, 0.582, 1.000 | #B5C9FF | |
| 20,000 | 145 | 0.2565 | 0.2576 | 0.406, 0.536, 1.000 | #ABC1FF | |
| 30,000 | 97 | 0.2501 | 0.2488 | 0.361, 0.497, 1.000 | #A2BBFF | the limit colour is a pale blue |

Second check (2026-11-18): the checker integrated Planck's law against the same CIE CSV independently (360-830 nm, 1 nm, IEC sRGB matrix) and all 23 rows matched on peak wavelength, x, y, linear RGB and hex to the last printed digit. With a higher-precision sRGB matrix one channel shifts by at most 0.001, and the hex changes only at 6,000 K (#FFF3F1 against #FFF3F0). The claim that the 1,000 K value matches a published locus value is unsourced and moved under "Unconfirmed".

Brightness is not in this table (each row is normalised). Radiant exitance grows as T⁴ (Stefan-Boltzmann), so an engine should scale emitted energy separately from colour.

## J12-3. Light sources: colour temperature and output (read directly)

Source: physicallybased.info API v2, "lightsources" (header licence "CC0 1.0 Universal", updated 2026-10-21), https://api.physicallybased.info/v2/lightsources. Temperature in kelvin; where three numbers are given the database lists a range and a default. Each row's own reference is shown; many of those are Wikipedia, so they are secondary.

| Source | Category | Colour temperature (K) | Output | Reference named in the database |
|---|---|---|---|---|
| Aputure amaran 300c | Artificial | 5,600 (range 2,500 to 7,500) | 26,155 lm | Aputure Inc.: Aputure amaran 300c Product Page |
| Aputure amaran P60c | Artificial | 5,600 (range 2,500 to 7,500) | 3,000 lm | Aputure Inc.: Aputure amaran P60c Photometrics & Technical Specifications |
| Aputure STORM 1000c | Artificial | 5,600 (range 1,800 to 20,000) | 78,096 lm | Aputure Inc.: Aputure STORM 1000c Product Page |
| Aputure STORM 1200x | Artificial | 5,600 (range 2,500 to 10,000) | 117,678 lm | Aputure Inc.: Aputure STORM 1200x Product Page |
| Arri M-Series M8 | Artificial | 6,000 | 43,249 lm (other variants: Medium 31,671 lm; Spot 22,053 lm) | ARRI GmbH.: ARRI M8 Product Page |
| Arri SkyPanel S60 Pro | Artificial | 5,600 (range 2,800 to 10,000) | 41,100 lm | ARRI GmbH.: ARRI SkyPanel S60 Pro Tech Data |
| Arri SkyPanel X | Artificial | 5,600 (range 1,500 to 20,000) | 48,000 lm | ARRI GmbH.: Arri SkyPanel X Tech Data |
| Black-body | Artificial | 798 | 1,000 lm | Wikipedia, The Free Encyclopedia: Black-body radiation |
| Candle Flame | Natural | 1,850 | 13 lm | Wikipedia, The Free Encyclopedia: Candle |
| Car Headlight (LED variant) | Artificial | 5,000 to 6,000 | 3,000 (range 2,000 to 5,000) lm | Bulb Facts LLC.: Aftermarket LED Kits Chart (the database also cites the Lab Tested Halogen Bulbs Chart, which belongs to the Halogen variant) |
| Car Headlight (Halogen variant) | Artificial | 3,400 to 4,000 | 1,500 (range 1,000 to 2,000) lm | Bulb Facts LLC.: Lab Tested Halogen Bulbs Chart |
| Digital Display | Artificial | 6,500 (range 5,000 to 7,500) | 200 (range 100 to 300) cd/m2 | Wikipedia, The Free Encyclopedia: Color temperature |
| Firefly | Natural | not given | 0.0006 lm | Journal of General Physiology: The Brightness of the Light of the West Indian Elaterid Beetle, Pyrophorus |
| Fluorescent Tube Light | Artificial | 2,700 (other variants: 4,000 and 6,500) | 3,000 (range 500 to 5,000) lm | Wikipedia, The Free Encyclopedia: Fluorescent lamp |
| Glow Stick | Artificial | not given | 10 lm | Wikipedia, The Free Encyclopedia: Glow Stick |
| Household Light Bulb | Artificial | 2,700 (range 2,200 to 6,000) | 800 (range 400 to 1,600) lm | Wikipedia, The Free Encyclopedia: Incandescent light bulb |
| Kerosene Lamp | Natural | 1,850 | 50 (range 8 to 100) lm | Wikipedia, The Free Encyclopedia: Color temperature |
| Kino Flo MIMIK 120 | Artificial | 5,600 (range 2,700 to 6,500) | 10,000 cd/m2 | Kino Flo, Inc.: Kino Flo MIMIK 120 Product Specifications |
| Light-emitting Diode (LED) | Artificial | not given (White variant: 10,596) | 1 lm | Wikipedia, The Free Encyclopedia: Light-emitting Diode |
| Moon | Natural | 4,100 | 0.32 lx (Crescent variant: 0.05 lx) | Wikipedia, The Free Encyclopedia: Moonlight |
| Photo Flash | Artificial | 5,500 | 10,000 to 20,000 lm | Wikipedia, The Free Encyclopedia: Flash (photography) |
| ROE Carbon CB5 MKII | Artificial | 5,600 (range 3,200 to 6,500) | 4,400 cd/m2 | ROE Visual: Carbon Full Spectrum LED Panels Specifications |
| Sky (Clear) | Natural | 10,000 | 6,000 lx | Physically Based Database: Illuminance Meter Measurement |
| Sky (Overcast Day) | Natural | 6,500 | 2,000 (range 100 to 2,000) lx | (reference not retyped) |
| Sky (Moonless Clear Night) | Natural | 6,500 | 0.002 lx | (reference not retyped) |
| Standard Illuminant | Artificial | 2,855 | 1,000 lm | Wikipedia, The Free Encyclopedia: Standard Illuminant |
| Street Light | Artificial | not given | 2,000 to 140,000 lm | Wikipedia, The Free Encyclopedia: Sodium-vapor lamp |
| Sun (Space variant: above the atmosphere) | Natural | 5,778 | 128,000 lx | Physically Based Database: Illuminance Meter Measurement |
| Sun (Midday variant: on the ground) | Natural | 5,778 | 100,000 (range 400 to 100,000) lx | (reference not retyped) |
| Sun (Sunset variant) | Natural | 2,600 | 3,000 lx | (reference not retyped) |

Notes on the table: "lm" rows are luminous flux; "lx" rows (Sun, Moon, Sky) are illuminance on the ground, except the Sun's Space variant (128,000 lx), which is above the atmosphere: the database's own description says "The solar illuminance constant is 128 000 lux in space, but atmospheric extinction on earth brings it down to around 100 000 lux." Use the Midday variant, 100,000 lx, for the ground. "cd/m2" rows are surface luminance. The Sky (Clear) row (6,000 lx) is the database's own illuminance-meter measurement; the Sun entry's references include a meter measurement together with Wikipedia, Schlyter and the NASA Sun fact sheet (second check of J16). Ranges for film lights are the adjustable range of the product.

Variants (second check, 2026-11-18): the table first took only the first variant of each light; the variants the checker relayed are now in it (Sun, Sky, Moon, Arri M8, Car Headlight, Fluorescent, LED). The database also lists 23 temperatures under Black-body and 53 variants under Standard Illuminant, not retyped here.

## J12-4. Where the eye is most sensitive (computed by me from CIE tables)

- Photopic (cone) luminous efficiency V(λ) peaks at 555 nm (CIE 018:2019 Table 1, file https://files.cie.co.at/Publications-datasets/CIE_sle_photopic.csv, maximum found by me).
- Scotopic (rod) efficiency V′(λ) peaks at 506 nm in the 1 nm CIE table (CIE 018:2019 Table 2, file https://files.cie.co.at/Publications-datasets/CIE_sle_scotopic.csv, maximum found by me). Wikipedia "Purkinje effect" rounds the rod peak to about 500 nm; see J17.

## Sources disagree

- Candle: the physicallybased.info candle and kerosene-lamp rows give 1,850 K; Wikipedia "Flame" gives a gas temperature of about 1,100 °C (1,373 K) for most of a candle flame (J11). Colour temperature of the light and gas temperature are different quantities.
- Sun: 5,778 K (physicallybased.info) is the effective surface temperature. Use 5,778 K for the disk and the sky rows for diffuse light. (A colour temperature for daylight on the ground is under "Unconfirmed".)
- Sun's illuminance: 128,000 lx (the database's Space variant, the solar illuminance constant above the atmosphere) against about 100,000 lx on the ground (its Midday variant; the same database's description says so). Not a contradiction once the variants are read: use 100,000 lx on the ground.

## Not found

- An open, citable table of real flame spectra (soot plus band emission) by fuel. The blackbody table above covers only the soot glow.
- Colour of lightning and of explosions as measured chromaticity.

## Unconfirmed (second check, 2026-11-18)

Not to be used until a source that gives the value has been opened.

- Spot check of J12-2: "the 1,000 K chromaticity (0.6528, 0.3445) matches the usual Planckian-locus value to the fourth decimal, so the integration is sound". The checker's reason: no source is cited for the "usual" value. (The arithmetic itself was reproduced by the checker; only the comparison with a published value is unconfirmed.)
- "Daylight on the ground is usually quoted nearer 5,500-6,500 K" (from "Sources disagree"). The checker's reason: unsourced.

## Sources

- CODATA values, NIST: c2 https://physics.nist.gov/cgi-bin/cuu/Value?c22ndrc ; c1 https://physics.nist.gov/cgi-bin/cuu/Value?c11strc ; b https://physics.nist.gov/cgi-bin/cuu/Value?bwien . Accessed 2026-11-17.
- BIPM, *The International System of Units* (SI Brochure, 9th ed., 2019), section 2.3.1. https://www.bipm.org/en/publications/si-brochure . Accessed 2026-11-17.
- CIE 018:2019, Tables 6, 1 and 2, dataset DOI 10.25039/CIE.DS.xvudnb9b. https://files.cie.co.at/Publications-datasets/CIE_xyz_1931_2deg.csv ; https://files.cie.co.at/Publications-datasets/CIE_sle_photopic.csv ; https://files.cie.co.at/Publications-datasets/CIE_sle_scotopic.csv ; licence from https://files.cie.co.at/Publications-datasets/CIE_xyz_1931_2deg.csv_metadata.json (and the matching metadata files). CC BY-SA 4.0. Accessed 2026-11-17.
- IEC 61966-2-1 (sRGB matrix, D65). No link. Not opened as a document.
- physicallybased.info API v2, "lightsources". https://api.physicallybased.info/v2/lightsources . CC0 1.0. Accessed 2026-11-17. (Each row's own reference, as named in the table, was not opened.)
- Wikipedia, "Purkinje effect". https://en.wikipedia.org/wiki/Purkinje_effect . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Flame" (via J11). https://en.wikipedia.org/wiki/Flame . CC BY-SA 4.0. Accessed 2026-11-17.
