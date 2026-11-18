# J1: Friction between materials

Fixed after the second check (2026-11-18):
- Licence of OpenStax Table 6.1 corrected from CC BY 4.0 to CC BY-NC-SA 4.0 (non-commercial). Link only: use the coefficients as facts, do not copy the table into a product.
- Added a Sources list. The checker's summary is in `J1-friction.md.verify.md`.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Part 3 of the round-two brief. Values a game can expose as adjustable defaults.

## Table J1-1. Static and kinetic friction coefficients (read directly)

Source: OpenStax, *University Physics Volume 1*, section 6.2 Friction, Table 6.1 "Approximate Coefficients of Static and Kinetic Friction", CC BY-NC-SA 4.0 (as stated on the page, accessed 2026-11-17; non-commercial: the coefficients are single facts, but the table as a compiled presentation is not to be redistributed commercially, so link only), https://openstax.org/books/university-physics-volume-1/pages/6-2-friction (accessed 2026-11-17). The book calls these approximate; it does not state test conditions.

| Materials in contact | Static μs | Kinetic μk | Source |
|---|---|---|---|
| Rubber on dry concrete | 1.0 | 0.7 | OpenStax UP1 Table 6.1, https://openstax.org/books/university-physics-volume-1/pages/6-2-friction |
| Rubber on wet concrete | 0.5–0.7 | 0.3–0.5 | same |
| Wood on wood | 0.5 | 0.3 | same |
| Waxed wood on wet snow | 0.14 | 0.1 | same |
| Metal on wood | 0.5 | 0.3 | same |
| Steel on steel (dry) | 0.6 | 0.3 | same |
| Steel on steel (oiled) | 0.05 | 0.03 | same |
| Teflon on steel | 0.04 | 0.04 | same |
| Bone lubricated by synovial fluid | 0.016 | 0.015 | same |
| Shoes on wood | 0.9 | 0.7 | same |
| Shoes on ice | 0.1 | 0.05 | same |
| Ice on ice | 0.1 | 0.03 | same |
| Steel on ice | 0.04 | 0.02 | same |

## Table J1-2. Braking deceleration implied by μ (computed by me)

Computed as a = μ·g with g = 9.80665 m/s² (standard gravity, see J10). This assumes all wheels or feet are braking at the friction limit on level ground; real vehicles stop more slowly (reaction time, load transfer, ABS behaviour). Use as an upper bound only.

| μ | a (m/s²) | example pairing from J1-1 |
|---|---|---|
| 1.0 | 9.81 | rubber on dry concrete, static |
| 0.7 | 6.86 | rubber on dry concrete, kinetic (locked wheels) |
| 0.5 | 4.90 | rubber on wet concrete, static, low end |
| 0.3 | 2.94 | rubber on wet concrete, kinetic, low end |
| 0.1 | 0.98 | shoes on ice, static |

## Not found

- An open, citable table of tyre-on-gravel, tyre-on-sand or tyre-on-snow coefficients. Ground-surface appearance values are in the existing library file F3 (ground materials); no friction numbers were added there.
- Friction as a function of speed or temperature (for example ice near 0 °C): not found in an open primary source in this pass.

## Sources disagree

- None checked in this pass. OpenStax gives single values; published tyre data vary widely with tread, load and water depth, so treat the table as game defaults, not measurements.

## Sources

- OpenStax, *University Physics Volume 1*, section 6.2 Friction, Table 6.1, CC BY-NC-SA 4.0 (link only), https://openstax.org/books/university-physics-volume-1/pages/6-2-friction (accessed 2026-11-17).
