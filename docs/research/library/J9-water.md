# J9: Water and other fluids (density, viscosity, sound)

Researched 2026-11-17 (all URLs accessed 2026-11-17). All from OpenStax, *University Physics Volume 1*, CC BY 4.0. Wave and river shapes are in the existing library files G1 and G2.

## Table J9-1. Density of water (read directly)

Source: OpenStax UP1 section 14.1, Table 14.2 "Densities of Water", https://openstax.org/books/university-physics-volume-1/pages/14-1-fluids-density-and-pressure.

| State | ρ (kg/m³) |
|---|---|
| Ice at 0 °C | 917 |
| Water at 0 °C | 999.8 |
| Water at 4 °C | 1,000 |
| Water at 20 °C | 998.2 |
| Water at 100 °C | 958.4 |
| Steam at 100 °C, 101.3 kPa | 167 (as printed in the table) |
| Sea water at 0 °C | 1,030 |

Note: the table's steam figure of 167 kg/m³ looks too high for steam at one atmosphere (ideal-gas estimate is about 0.6 kg/m³). Check before use; this may be a typo in the source.

## Table J9-2. Viscosity (read directly)

Source: OpenStax UP1 section 14.7, Table 14.4 "Coefficients of Viscosity of Various Fluids", https://openstax.org/books/university-physics-volume-1/pages/14-7-viscosity-and-turbulence. Units 10⁻³ Pa·s.

| Fluid | T (°C) | η (10⁻³ Pa·s) |
|---|---|---|
| Air | 0 / 20 / 40 / 100 | 0.0171 / 0.0181 / 0.0190 / 0.0218 |
| Water | 0 / 20 / 37 / 40 / 100 | 1.792 / 1.002 / 0.6947 / 0.653 / 0.282 |
| Whole blood | 20 / 37 | 3.015 / 2.084 |
| Blood plasma | 20 / 37 | 1.810 / 1.257 |
| Ethyl alcohol | 20 | 1.20 |
| Methanol | 20 | 0.584 |
| Mercury | 20 | 0.0450 (as printed) |
| Milk | 20 | 3.0 |
| Olive oil | 20 | 138 |
| Corn oil | 20 | 65 |
| Motor oil SAE 10 | 30 | 200 |
| Heavy machine oil | 20 | 660 |
| Glycerin | 20 | 1,500 |
| Maple syrup | 20 | 2,000–3,000 |
| Honey | 20 | 2,000–10,000 |
| Ammonia, CO₂, helium, hydrogen, oxygen, steam | 20, 20, 20, 0, 20, 100 | 0.00974, 0.0147, 0.0196, 0.0090, 0.0203, 0.0130 |

Reynolds number for flow in a tube (same section): N_R = 2ρvr/η; laminar below about 2,000 (the section's worked example uses this threshold).

## Table J9-3. Speed of sound (read directly)

Source: OpenStax UP1 section 17.2, Table 17.1 "Speed of Sound in Various Media", https://openstax.org/books/university-physics-volume-1/pages/17-2-speed-of-sound. In air at sea level, v = 331 m/s · √(1 + T_C/273 °C); 331 m/s at 0 °C, 343 m/s at 20 °C (same section).

| Medium | v (m/s) |
|---|---|
| Air, 0 °C | 331 |
| Carbon dioxide, 0 °C | 259 |
| Oxygen, 0 °C | 316 |
| Helium, 0 °C | 965 |
| Hydrogen, 0 °C | 1,290 |
| Ethanol, 20 °C | 1,160 |
| Mercury, 20 °C | 1,450 |
| Fresh water, 20 °C | 1,480 |
| Sea water, 20 °C | 1,540 |
| Human tissue, 20 °C | 1,540 |
| Vulcanised rubber | 54 |
| Polyethylene | 920 |
| Marble | 3,810 |
| Pyrex glass | 5,640 |
| Lead | 1,960 |
| Aluminium | 5,120 |
| Steel | 5,960 |

## Cross-check (computed by me)

Water against air: density 1,000 / 1.29 ≈ 775 times (J3, J9-1); viscosity 1.002 / 0.0181 ≈ 55 times at 20 °C. Hirt et al. (J7) state "water is 800 times denser and 60 times more viscous" than air, which agrees roughly.

## Not found

- Surface tension values (OpenStax has them in another volume; not read in this pass).
- Drag of swimming humans and animals.
