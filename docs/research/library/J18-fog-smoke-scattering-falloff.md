# J18: Visibility in fog, haze and smoke; light scattering and falloff

Fixed after the second check (2026-11-18):
- No value was wrong. The sentence on why fog looks white (Mie) moved under "Unconfirmed": no source gives it.
- Sources upgraded: the WMO MOR row is now cited to the PDF itself (p. 1, section 9.1.1; p. 5, eq. 9.5), not a search extract; the smoke rows to the FDS User's Guide itself (SP 1019, section 22.10.5, p. 362), not PyroSim.
- Added: the spread of soot's mass-specific extinction (8,700 ± 1,100 m²/kg) and what it does to Table J18-2a; the WMO's wider range of contrast thresholds under "Sources disagree"; that the Wikipedia "Visibility" sentences carry no references.
- A "Sources" list added at the end.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Values marked "computed by me" are arithmetic. Light falloff and colour in water are already in round-one G3 (Pope and Fry absorption), not repeated here. Inverse-square falloff is in J16-4.

## J18-1. Contrast loss and visual range (read directly)

| Item | Value or formula | Source |
|---|---|---|
| Contrast with distance | C(x) = exp(−b_ext · x), a Beer-Lambert decay; b_ext = extinction coefficient (scattering plus absorption, per metre) | Wikipedia "Visibility", https://en.wikipedia.org/wiki/Visibility (secondary) |
| Perceptible contrast | 0.018-0.03 in daylight lab tests; 2% (0.02) is the usual choice | same |
| Koschmieder equation | visual range x_V = 3.912 / b_ext (3.912 = −ln 0.02) | same |
| Cleanest air (Rayleigh only, sea level, 520 nm) | b_ext ≈ 13.2 × 10⁻⁶ /m, so visibility at most about 296 km | same |
| Meteorological optical range (MOR) | path length that reduces the luminous flux of a collimated beam from a 2,700 K incandescent lamp to 5% of its starting value; so MOR = (1/σ) · ln(1/0.05) ≈ 3 / σ | WMO, *Guide to Instruments and Methods of Observation*, WMO-No. 8, Part I, chapter 9, preliminary 2018 edition, https://old.wmo.int/extranet/pages/prog/www/IMOP/publications/CIMO-Guide/Prelim_2018_ed/8_I_9_en_MR_tc.pdf: definition on PDF p. 1 (section 9.1.1), the formula on PDF p. 5 (eq. 9.5) (read from the PDF by the second checker) |
| Particle size that hurts visibility most | 0.1-1.0 µm diameter, per unit mass | Wikipedia "Visibility" |
| Fog | visibility under 1 km | Wikipedia "Visibility" and "Fog" (international definition; "Fog" cites the US Federal Meteorological Handbook No. 1, ch. 8) |
| Mist | 1-2 km | Wikipedia "Visibility" |
| Haze | 2-5 km | Wikipedia "Visibility" |
| Reported as zero | under 100 m | Wikipedia "Visibility" |

### Table J18-1a. Visibility against extinction coefficient (computed by me)

| b_ext (per m) | Koschmieder range (2%) | WMO MOR (5%) | Sky condition |
|---|---|---|---|
| 0.0000132 | 296 km | 227 km | Rayleigh air only |
| 0.0001 | 39 km | 30 km | clear |
| 0.001 | 3.9 km | 3.0 km | haze |
| 0.004 | 980 m | 750 m | fog threshold by Koschmieder |
| 0.0196 | 200 m | 153 m | thick fog |
| 0.04 | 98 m | 75 m | "zero" visibility |
| 0.1 | 39 m | 30 m | dense fog |

Rendering use: fog factor = exp(−b · d) is the same law, so pick b from the visibility you want: b = 3.912 / visibility.

## J18-2. Smoke (read directly)

| Item | Value or formula | Source |
|---|---|---|
| Visibility in smoke | S = C / K, K = extinction coefficient (per m); C = 3 for light-reflecting signs, C = 8 for light-emitting signs | NIST Fire Dynamics Simulator User's Guide (NIST SP 1019, Sixth Edition, revision FDS-6.11.1), section 22.10.5 "Visibility and Obscuration", p. 362, eq. 22.23, citing ref. [85] Mulholland, *SFPE Handbook* 3rd ed. (2002) for C = 8 and C = 3; the default VISIBILITY_FACTOR is 3 (read by the second checker; this file first read it via the Thunderhead Engineering PyroSim and fdsrun documentation) |
| Mass-specific extinction of soot | K_m = 8,700 m²/kg (default, for flaming wood and plastics), so K = K_m · soot density; spread 8,700 ± 1,100 m²/kg at 633 nm for most flaming fuels | same, p. 362; the spread from the guide's footnote 7, citing ref. [86] Mulholland and Croarkin, *Fire and Materials* 24 (2000) 227-230 |

### Table J18-2a. Smoke for a target visibility (computed by me)

| Visibility S | K needed (reflecting sign, C = 3) | K needed (lit sign, C = 8) | Soot concentration for C = 3 |
|---|---|---|---|
| 1 m | 3.0 /m | 8.0 /m | 345 mg/m³ |
| 3 m | 1.0 /m | 2.7 /m | 115 mg/m³ |
| 5 m | 0.6 /m | 1.6 /m | 69 mg/m³ |
| 10 m | 0.3 /m | 0.8 /m | 34 mg/m³ |
| 20 m | 0.15 /m | 0.4 /m | 17 mg/m³ |
| 30 m | 0.1 /m | 0.27 /m | 11.5 mg/m³ |

With K_m's spread of ± 1,100 m²/kg (about ± 13%), the soot column spans about ± 13% too: for example, 1 m visibility needs 306-395 mg/m³ (computed by the second checker).

A lit sign is seen about 2.7 times farther than a painted one in the same smoke (8/3, computed by me), which is a good rule for exit signs, torches and windows in a smoky room.

## J18-3. Why fog is white and haze is blue (read directly)

- Air molecules scatter short wavelengths far more (Rayleigh); the clean-air extinction above is quoted at 520 nm because it varies with wavelength (Wikipedia "Visibility").
- Fog and mist are mainly water droplets; haze and smoke can be smaller particles (Wikipedia "Visibility"). Why fog looks white has no confirmed source here: see "Unconfirmed" below.

## Sources disagree

- Visual range constant: 3.912 (2% contrast, Koschmieder) against about 3.0 (5% transmission, WMO MOR). The same air gives an MOR about 23% shorter than the Koschmieder range (computed by me).
- Fog threshold: all sources agree on 1 km; US aviation also uses statute-mile categories (not used here).
- Perceptible contrast: Wikipedia "Visibility" gives 0.018-0.03; WMO-No. 8 (PDF p. 3, section 9.1.2) says proposed contrast thresholds range from 0.0077 to 0.06 "or even 0.2", and that aviation takes 0.05 because runway markings have low contrast, which is why the WMO uses 5%.

Note on "Visibility" (second check): every sentence of Wikipedia "Visibility" used in J18-1 (0.018-0.03, 3.912, 13.2 × 10⁻⁶ /m, 296 km, 0.1-1.0 µm, the fog, mist and haze bands, zero under 100 m) carries no inline reference on the page. The values match the page but rest on Wikipedia alone.

## Not found

- Extinction coefficients for specific fog types (advection, radiation) with droplet size from an open primary.
- Smoke extinction for wood versus plastic fires from an open primary (the FDS value is a single default, with a spread of ± 1,100 m²/kg for most flaming fuels).
- Wavelength dependence of fog extinction as a table.

## Unconfirmed (second check, 2026-11-18)

Not to be used until a source that gives it has been opened.

- J18-3: "Droplets much larger than the wavelength scatter all colours about equally, which is why fog looks white (Mie regime; general optics, stated here without a numeric source)." The checker's reason: the file gives no source for it, and neither Wikipedia "Visibility" nor "Fog" states it in those words.

## Sources

- Wikipedia, "Visibility" (no inline references on the sentences used). https://en.wikipedia.org/wiki/Visibility . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Fog" (citing the US Federal Meteorological Handbook No. 1, ch. 8). https://en.wikipedia.org/wiki/Fog . CC BY-SA 4.0. Accessed 2026-11-17.
- WMO, *Guide to Instruments and Methods of Observation*, WMO-No. 8, Part I, chapter 9, preliminary 2018 edition (sections 9.1.1 and 9.1.2, eq. 9.5). https://old.wmo.int/extranet/pages/prog/www/IMOP/publications/CIMO-Guide/Prelim_2018_ed/8_I_9_en_MR_tc.pdf . Accessed 2026-11-17.
- NIST, *Fire Dynamics Simulator User's Guide*, NIST SP 1019, Sixth Edition, revision FDS-6.11.1, section 22.10.5, p. 362, eq. 22.23 and footnote 7 (citing Mulholland, *SFPE Handbook* 3rd ed. (2002), and Mulholland and Croarkin, *Fire and Materials* 24 (2000) 227-230; neither opened). No link given.
- Thunderhead Engineering, PyroSim documentation, and the fdsrun documentation (secondary copies of the FDS guide's visibility section, as first read). No link given.
- Round-one G3 and J16-4 of this library (water; inverse-square falloff).
