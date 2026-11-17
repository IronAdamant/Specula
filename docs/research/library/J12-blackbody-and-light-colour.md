# J12: Blackbody colour, colour temperature of light sources

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

Method: Planck's law with the constants above, integrated against the CIE 1931 2° colour-matching functions at 1 nm steps, 360-830 nm (CIE 018:2019 Table 6, dataset DOI 10.25039/CIE.DS.xvudnb9b, file https://files.cie.co.at/Publications-datasets/CIE_xyz_1931_2deg.csv; licence not stated on the dataset page I read, so the dataset itself is not copied here). XYZ was converted to linear sRGB with the IEC 61966-2-1 matrix (D65 white), negative channels clipped to 0, scaled so the largest channel is 1, then sRGB-encoded for the hex value. Because the white point is D65, a 6,500 K blackbody comes out nearly white; lower temperatures look orange, higher ones blue. Rows marked "clipped" are outside the sRGB gamut and lose saturation.

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

Spot check: the 1,000 K chromaticity (0.6528, 0.3445) matches the usual Planckian-locus value to the fourth decimal, so the integration is sound.

Brightness is not in this table (each row is normalised). Radiant exitance grows as T⁴ (Stefan-Boltzmann), so an engine should scale emitted energy separately from colour.

## J12-3. Light sources: colour temperature and output (read directly)

Source: physicallybased.info API v2, "lightsources" (header licence "CC0 1.0 Universal", updated 2026-10-21), https://api.physicallybased.info/v2/lightsources. Temperature in kelvin; where three numbers are given the database lists a range and a default. Each row's own reference is shown; many of those are Wikipedia, so they are secondary.

| Source | Category | Colour temperature (K) | Output | Reference named in the database |
|---|---|---|---|---|
| Aputure amaran 300c | Artificial | 5,600 (range 2,500 to 7,500) | 26,155 lm | Aputure Inc.: Aputure amaran 300c Product Page |
| Aputure amaran P60c | Artificial | 5,600 (range 2,500 to 7,500) | 3,000 lm | Aputure Inc.: Aputure amaran P60c Photometrics & Technical Specifications |
| Aputure STORM 1000c | Artificial | 5,600 (range 1,800 to 20,000) | 78,096 lm | Aputure Inc.: Aputure STORM 1000c Product Page |
| Aputure STORM 1200x | Artificial | 5,600 (range 2,500 to 10,000) | 117,678 lm | Aputure Inc.: Aputure STORM 1200x Product Page |
| Arri M-Series M8 | Artificial | 6,000 | 43,249 lm | ARRI GmbH.: ARRI M8 Product Page |
| Arri SkyPanel S60 Pro | Artificial | 5,600 (range 2,800 to 10,000) | 41,100 lm | ARRI GmbH.: ARRI SkyPanel S60 Pro Tech Data |
| Arri SkyPanel X | Artificial | 5,600 (range 1,500 to 20,000) | 48,000 lm | ARRI GmbH.: Arri SkyPanel X Tech Data |
| Black-body | Artificial | 798 | 1,000 lm | Wikipedia, The Free Encyclopedia: Black-body radiation |
| Candle Flame | Natural | 1,850 | 13 lm | Wikipedia, The Free Encyclopedia: Candle |
| Car Headlight | Artificial | 5,000 to 6,000 | 3,000 (range 2,000 to 5,000) lm | Bulb Facts LLC.: Lab Tested Halogen Bulbs Chart |
| Digital Display | Artificial | 6,500 (range 5,000 to 7,500) | 200 (range 100 to 300) cd/m2 | Wikipedia, The Free Encyclopedia: Color temperature |
| Firefly | Natural | not given | 0.0006 lm | Journal of General Physiology: The Brightness of the Light of the West Indian Elaterid Beetle, Pyrophorus |
| Fluorescent Tube Light | Artificial | 2,700 | 3,000 (range 500 to 5,000) lm | Wikipedia, The Free Encyclopedia: Fluorescent lamp |
| Glow Stick | Artificial | not given | 10 lm | Wikipedia, The Free Encyclopedia: Glow Stick |
| Household Light Bulb | Artificial | 2,700 (range 2,200 to 6,000) | 800 (range 400 to 1,600) lm | Wikipedia, The Free Encyclopedia: Incandescent light bulb |
| Kerosene Lamp | Natural | 1,850 | 50 (range 8 to 100) lm | Wikipedia, The Free Encyclopedia: Color temperature |
| Kino Flo MIMIK 120 | Artificial | 5,600 (range 2,700 to 6,500) | 10,000 cd/m2 | Kino Flo, Inc.: Kino Flo MIMIK 120 Product Specifications |
| Light-emitting Diode (LED) | Artificial | not given | 1 lm | Wikipedia, The Free Encyclopedia: Light-emitting Diode |
| Moon | Natural | 4,100 | 0.32 lx | Wikipedia, The Free Encyclopedia: Moonlight |
| Photo Flash | Artificial | 5,500 | 10,000 to 20,000 lm | Wikipedia, The Free Encyclopedia: Flash (photography) |
| ROE Carbon CB5 MKII | Artificial | 5,600 (range 3,200 to 6,500) | 4,400 cd/m2 | ROE Visual: Carbon Full Spectrum LED Panels Specifications |
| Sky | Natural | 10,000 | 6,000 lx | Physically Based Database: Illuminance Meter Measurement |
| Standard Illuminant | Artificial | 2,855 | 1,000 lm | Wikipedia, The Free Encyclopedia: Standard Illuminant |
| Street Light | Artificial | not given | 2,000 to 140,000 lm | Wikipedia, The Free Encyclopedia: Sodium-vapor lamp |
| Sun | Natural | 5,778 | 128,000 lx | Physically Based Database: Illuminance Meter Measurement |
Notes on the table: "lm" rows are luminous flux; "lx" rows (Sun, Moon, Sky) are illuminance on the ground; "cd/m2" rows are surface luminance. The Sun row (128,000 lx) and Sky row (6,000 lx) are the database's own illuminance-meter measurements. Ranges for film lights are the adjustable range of the product.

## J12-4. Where the eye is most sensitive (computed by me from CIE tables)

- Photopic (cone) luminous efficiency V(λ) peaks at 555 nm (CIE 018:2019 Table 1, file https://files.cie.co.at/Publications-datasets/CIE_sle_photopic.csv, maximum found by me).
- Scotopic (rod) efficiency V′(λ) peaks at 506 nm in the 1 nm CIE table (CIE 018:2019 Table 2, file https://files.cie.co.at/Publications-datasets/CIE_sle_scotopic.csv, maximum found by me). Wikipedia "Purkinje effect" rounds the rod peak to about 500 nm; see J17.

## Sources disagree

- Candle: the physicallybased.info candle and kerosene-lamp rows give 1,850 K; Wikipedia "Flame" gives a gas temperature of about 1,100 °C (1,373 K) for most of a candle flame (J11). Colour temperature of the light and gas temperature are different quantities.
- Sun: 5,778 K (physicallybased.info) is the effective surface temperature; daylight on the ground is usually quoted nearer 5,500-6,500 K. Use 5,778 K for the disk and the sky rows for diffuse light.

## Not found

- An open, citable table of real flame spectra (soot plus band emission) by fuel. The blackbody table above covers only the soot glow.
- Colour of lightning and of explosions as measured chromaticity.
