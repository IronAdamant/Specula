# Second check: G4-sky-and-weather.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its findings; its per-row table stayed with the checker.

### Round 3: G4-sky-and-weather.md

**Rows: 22 total: 21 VERIFIED, 1 FIX, 0 UNVERIFIABLE.**

- **FIX (G4-3, lump graupel):** the size range for **V = 1.3 D^0.66** is **0.5 to 3.0 mm** (density 0.1-0.20 Mg m⁻³, N = 58), not "0.5-1.0".
  - The 0.5-1.0 mm range belongs to a different lump-graupel row, **V = 1.5 D^0.37** (density 0.2-0.45).
  - The third row is V = 1.16 D^0.46 (0.5-2.0 mm, density 0.05-0.1).
  - Source: Locatelli & Hobbs 1974, Table 1, in the Albany scan text layer.
  - The 2 mm example (2.05 m/s) still recomputes correctly.
- **WMO Table 6** matches verbatim at the live page, §2.2.1.2: High 3-8 / 5-13 / 6-18 km; Middle 2-4 / 2-7 / 2-8 km; Low surface to 2 km. The three exceptions (a)-(c) are paraphrased faithfully.
- **Gunn & Kinzer** (via MadSci, C. Morgan, NWS): all 9 values match the reproduced table, and so do the conditions (1013 mb, 20 °C).
  - The Beard 1.2-4 mm fit k = 2.01 × 10³ cm^0.5 s⁻¹ is verbatim.
  - My check: at 3 mm (r = 0.15 cm) it gives 7.8 m/s against the table's 8.06.
  - The original AMS paper was not opened, by the file or by me, so this stays secondary.
- **Locatelli & Hobbs:** the other rows match Table 1, and the examples recompute: hexagonal graupel 1.1 D^0.57 (0.8-3.2), 1.63; conical graupel 1.2 D^0.65 (0.8-3.0); dense-rimed aggregates 0.79 D^0.27 (2-12), 1.22 and 1.47.
  - **Gap partly closable:** the unrimed radiating-dendrite aggregates row reads **V = 0.8 D^0.16, 2.0-10.0 mm (N = 28, r = 0.20)** in the text layer. It's legible enough to add, with a "scan" caution.
- **Wikipedia rows** all match the live text, with small notes:
  - The Drop (liquid) 3 mm ≈ 8 m/s claim cites van Boxel's "Numerical model for the fall speed of raindrops in a waterfall simulator" (Gent 98 PDF). That's correct.
  - Snowflake: the 0.705 in (18 mm) dime-sized crystal is present and cites the NYT 2007. The live page no longer shows a "page needed" tag.
  - Cumulonimbus infobox: 500-16,000 m is present. The infobox has no inline ref, but the WMO 1975 pp. 48-50 citation does appear in the article.
- **Disagreement verified:** WMO gives 16,500-45,000 ft and Wikipedia "Cloud" gives 16,500-40,000 ft, citing the same Definitions page.
- **Licences:** the WMO Cloud Atlas copyright is WMO and Wikipedia is CC BY-SA. Neither is stated in the file; facts only.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| G4-3 Lump | 1.3 D^0.66, "0.5-1.0" | **FIX** | Range is 0.5-3.0 mm (density 0.1-0.2); 0.5-1.0 belongs to 1.5 D^0.37 | same |
