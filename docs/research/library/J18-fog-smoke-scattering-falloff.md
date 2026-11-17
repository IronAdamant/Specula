# J18: Visibility in fog, haze and smoke; light scattering and falloff

Researched 2026-11-17 (all URLs accessed 2026-11-17). Values marked "computed by me" are arithmetic. Light falloff and colour in water are already in round-one G3 (Pope and Fry absorption), not repeated here. Inverse-square falloff is in J16-4.

## J18-1. Contrast loss and visual range (read directly)

| Item | Value or formula | Source |
|---|---|---|
| Contrast with distance | C(x) = exp(−b_ext · x), a Beer-Lambert decay; b_ext = extinction coefficient (scattering plus absorption, per metre) | Wikipedia "Visibility", https://en.wikipedia.org/wiki/Visibility (secondary) |
| Perceptible contrast | 0.018-0.03 in daylight lab tests; 2% (0.02) is the usual choice | same |
| Koschmieder equation | visual range x_V = 3.912 / b_ext (3.912 = −ln 0.02) | same |
| Cleanest air (Rayleigh only, sea level, 520 nm) | b_ext ≈ 13.2 × 10⁻⁶ /m, so visibility at most about 296 km | same |
| Meteorological optical range (MOR) | path length that reduces the luminous flux of a collimated beam from a 2,700 K incandescent lamp to 5% of its starting value; so MOR = −ln 0.05 / σ ≈ 3.0 / σ (the formula step computed by me) | WMO, *Guide to Instruments and Methods of Observation*, WMO-No. 8, Part I, chapter 9, preliminary 2018 edition, https://old.wmo.int/extranet/pages/prog/www/IMOP/publications/CIMO-Guide/Prelim_2018_ed/8_I_9_en_MR_tc.pdf (read via search extract) |
| Particle size that hurts visibility most | 0.1-1.0 µm diameter, per unit mass | Wikipedia "Visibility" |
| Fog | visibility under 1 km | Wikipedia "Visibility" and "Fog" (international definition) |
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
| Visibility in smoke | S = C / K, K = extinction coefficient (per m); C = 3 for light-reflecting signs, C = 8 for light-emitting signs | NIST Fire Dynamics Simulator User's Guide (NIST SP 1019), section on visibility, via Thunderhead Engineering PyroSim documentation and the fdsrun documentation (secondary to the FDS guide) |
| Mass-specific extinction of soot | K_m = 8,700 m²/kg (flaming, default), so K = K_m · soot density | same |

### Table J18-2a. Smoke for a target visibility (computed by me)

| Visibility S | K needed (reflecting sign, C = 3) | K needed (lit sign, C = 8) | Soot concentration for C = 3 |
|---|---|---|---|
| 1 m | 3.0 /m | 8.0 /m | 345 mg/m³ |
| 3 m | 1.0 /m | 2.7 /m | 115 mg/m³ |
| 5 m | 0.6 /m | 1.6 /m | 69 mg/m³ |
| 10 m | 0.3 /m | 0.8 /m | 34 mg/m³ |
| 20 m | 0.15 /m | 0.4 /m | 17 mg/m³ |
| 30 m | 0.1 /m | 0.27 /m | 11.5 mg/m³ |

A lit sign is seen about 2.7 times farther than a painted one in the same smoke (8/3, computed by me), which is a good rule for exit signs, torches and windows in a smoky room.

## J18-3. Why fog is white and haze is blue (read directly)

- Air molecules scatter short wavelengths far more (Rayleigh); the clean-air extinction above is quoted at 520 nm because it varies with wavelength (Wikipedia "Visibility").
- Fog and mist are mainly water droplets; haze and smoke can be smaller particles (Wikipedia "Visibility"). Droplets much larger than the wavelength scatter all colours about equally, which is why fog looks white (Mie regime; general optics, stated here without a numeric source).

## Sources disagree

- Visual range constant: 3.912 (2% contrast, Koschmieder) against about 3.0 (5% transmission, WMO MOR). The same air gives an MOR about 23% shorter than the Koschmieder range (computed by me).
- Fog threshold: all sources agree on 1 km; US aviation also uses statute-mile categories (not used here).

## Not found

- Extinction coefficients for specific fog types (advection, radiation) with droplet size from an open primary.
- Smoke extinction for wood versus plastic fires from an open primary (the FDS value is a single default).
- Wavelength dependence of fog extinction as a table.
