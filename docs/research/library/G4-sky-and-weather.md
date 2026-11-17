# G4: Sky and weather: cloud levels, rain, snow and hail fall speeds and sizes, fog

Researched 2026-11-17 (all URLs accessed 2026-11-17). Brief section 3, "Sky and weather". Values marked "computed by me" are arithmetic. Sun and sky brightness and colour by elevation are in J16 (round three) and round-two J-files; fog extinction and visibility are in J18; wave height against wind is in the main library's G1.

## G4-1. Cloud levels (WMO International Cloud Atlas, read directly)

WMO, *International Cloud Atlas*, "Definitions of clouds", section 2.2.1.2, Table 6, https://cloudatlas.wmo.int/en/clouds-definitions.html. Levels overlap and vary with latitude.

| Level | Genera | Polar | Temperate | Tropical |
|---|---|---|---|---|
| High | cirrus, cirrocumulus, cirrostratus | 3-8 km | 5-13 km | 6-18 km |
| Middle | altocumulus, altostratus, nimbostratus | 2-4 km | 2-7 km | 2-8 km |
| Low | stratus, stratocumulus, cumulus, cumulonimbus | surface to 2 km | surface to 2 km | surface to 2 km |

Exceptions given by the Atlas: altostratus often extends higher; nimbostratus usually extends into the other two levels; cumulus and cumulonimbus bases are low but their tops may reach the middle and high levels. Cumulonimbus spans about 500-16,000 m (Wikipedia infobox, citing WMO *International Cloud Atlas* 1975 pp. 48-50).

## G4-2. Raindrop fall speed by size (retyped selection)

Gunn and Kinzer, "The terminal velocity of fall for water droplets in stagnant air", *J. Meteorology* 6 (1949) 243-248, sea-level conditions (1013 mb, 20 °C); table as reproduced by C. Morgan (US National Weather Service forecaster), MadSci Network, https://www.madsci.org/posts/archives/2000-07/962626446.Ph.r.html. Original paper: https://journals.ametsoc.org/view/journals/atsc/6/4/1520-0469_1949_006_0243_ttvoff_2_0_co_2.xml (not opened).

| Diameter (mm) | 0.1 | 0.2 | 0.5 | 1.0 | 2.0 | 3.0 | 4.0 | 5.0 | 5.8 |
|---|---|---|---|---|---|---|---|---|---|
| Fall speed (m/s) | 0.27 | 0.72 | 2.06 | 4.03 | 6.49 | 8.06 | 8.83 | 9.09 | 9.17 |

Large drops level off near 9 m/s because they flatten. The same page gives Beard's (1976, *J. Atmos. Sci.* 33:851) fit for 1.2 to 4 mm: u = k r^0.5 with k = 2.01 × 10³ cm^0.5 s⁻¹. A Wikipedia cross-check: a 3 mm droplet falls at about 8 m/s ("Drop (liquid)", citing van Boxel 1998).

## G4-3. Snow and graupel fall speeds (read from a scanned table; check before use)

Locatelli and Hobbs, "Fall speeds and masses of solid precipitation particles", *J. Geophys. Res.* 79(15) (1974) 2185-2197, doi:10.1029/JC079i015p02185, Table 1; scanned copy https://www.atmos.albany.edu/facstaff/rfovell/ATM562/locatelli-hobbs-1974.pdf. V in m/s, D maximum dimension in mm. The text layer is OCR from a scan, so exponents were read with care and only clearly legible rows are used.

| Particle | V against D | Size range (mm) | Example (computed by me) |
|---|---|---|---|
| Hexagonal graupel | V = 1.1 D^0.57 | 0.8-3.2 | 2 mm: 1.6 m/s |
| Conical graupel | V = 1.2 D^0.65 | 0.8-3.0 | |
| Lump graupel | V = 1.3 D^0.66 | 0.5-1.0 (one subset; others to 3 mm) | 2 mm: 2.1 m/s |
| Aggregates of densely rimed radiating assemblages of dendrites | V = 0.79 D^0.27 | 2-12 | 5 mm: 1.2 m/s; 10 mm: 1.5 m/s |

Game use: rain at 4-9 m/s reads as streaks; snow at about 1 m/s reads as drifting flakes; graupel and small hail fall at 1.5-3 m/s and bounce.

## G4-4. Hail and snowflake sizes

- Hail: stones are 5 mm to 15 cm across; can exceed 0.5 kg (Wikipedia "Hail", citing AMS *Glossary of Meteorology* 2009 and NOAA NSSL). METAR code GR for 5 mm or more, GS for smaller hail and graupel. US NWS severe-hail threshold 1 inch from January 2010 (was 0.75 in); stones over 2 cm usually damaging.
- Snowflakes: single crystals up to about 18 mm (0.705 in) have been observed (Wikipedia "Snowflake", page flagged "page needed").

## G4-5. Fog

- Fog reduces visibility below 1 km; mist is less (Wikipedia "Fog", citing WMO aviation hazards page and US Federal Meteorological Handbook No. 1, ch. 8). Fog can be only tens of centimetres deep over some terrain with no wind (same article, citing ARM education newsletter, 2008). Extinction and contrast thresholds are in J18.

## Sources disagree

- High cloud heights: WMO Table 6 gives 5-13 km temperate (16,500-45,000 ft), while Wikipedia "Cloud" lists 16,500-40,000 ft for temperate regions, citing the same WMO page.

## Not found

- Sun and moon brightness and colour by elevation with sources: covered in J16 and round-two files; nothing new added here.
- Cloud sizes (horizontal extent of cumulus by type) from a primary source.
- Measured fog liquid water contents by fog type (only mentioned as g/m³ ranges in secondary sources, not read).
- Locatelli and Hobbs rows for unrimed dendrite aggregates: the scanned exponents were not legible enough to use.
