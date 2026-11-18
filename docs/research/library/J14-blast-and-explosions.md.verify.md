# Second check: J14-blast-and-explosions.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its findings; its per-row table stayed with the checker.

### Round 3: J14-blast-and-explosions.md

**37 rows: 27 VERIFIED, 2 FIX, 8 UNVERIFIABLE.**

FIX:
- Row 1.2: the file implies a Wikipedia "Hopkinson-Cranz scaling" source; that page does not exist (Wikipedia API returns `missing`, search finds no article). Z = R / W^(1/3) is verified instead from Filice et al. 2022 (text and Eq. 2-4). Cite Filice.
- Row 2.10: Brode at Z = 2 recomputes to 156.4 kPa, not 158 kPa, so the gap below Kinney-Graham (208 kPa) is 25%, not 24%. Corrected range: 18-25% (Z = 1: 826 kPa vs 1,009 kPa, 18%, matches).

UNVERIFIABLE (Goel et al. 2012 PDF sits behind a JavaScript bot-check; the DSJ article page confirms only title, authors, Def. Sci. J. 62(5) 300-306 and DOI 10.14429/dsj.62.1149; the Wayback copy timed out):
- 1.3 ground-burst factor 1.8; 1.4 Kingery-Bulmash valid to Z = 40; 2.3 Kinney-Graham impulse (Eq. 15); 2.4 Brode; 2.5 Henrych bands; 2.6 Sadovskiy; 2.7 Goel conclusions quote; 2.8 Friedlander form and Larcher b = 5.2777 Z^-1.1975.
- Note: Row 1.4 also conflicts with Filice et al. 2022 Table 3, which gives the simplified Kingery-Bulmash peak-overpressure fit as valid over Z = 0.2-198.5 m·kg^-1/3. Add this to "Sources disagree".

Gaps (data on the cited pages that the file leaves out):
- Wikipedia "Overpressure" (NIOSH quote): 1% fatalities at 35-45 psi, 99% at 55-65 psi.
- Same page (DTIC quote): 50% eardrum rupture at 15 psi; severe lung injury 70-100 psi; 50:50 survival about 500 psi (instantaneous peak).
- Filice's 400 g at 2 m gives a raw Z of 2.71; the paper's 2.76 implies a TNT-equivalence factor. The file should say Z = 2.76 is after TNT conversion.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| 1.2 | Z = R / W^(1/3), Hopkinson-Cranz | FIX | Formula right (Filice text); Wikipedia page does not exist | https://dspace.vsb.cz/server/api/core/bitstreams/11c04d04-45f1-4c91-8d55-36d3df0da36b/content |
| 1.3 | Ground burst ×1.8 | UNVERIFIABLE | Goel PDF blocked | https://publicationsdrdo.in/index.php/dsj/article/view/1149 |
| 1.4 | KB charts to Z = 40 | UNVERIFIABLE | Goel PDF blocked; Filice Table 3 says 0.2-198.5 | as 1.3 |
| 2.3 | K-G impulse 0.067, 0.23, 1.55 | UNVERIFIABLE | Goel only | as 1.3 |
| 2.4 | Brode two-range formula | UNVERIFIABLE | Goel only | as 1.3 |
| 2.5 | Henrych three Z bands | UNVERIFIABLE | Goel only | as 1.3 |
| 2.6 | Sadovskiy 0.085/0.3/0.8 MPa | UNVERIFIABLE | Goel only | as 1.3 |
| 2.7 | Goel conclusions, "a wide variation in results" | UNVERIFIABLE | Goel only | as 1.3 |
| 2.8 | Friedlander + Larcher b | UNVERIFIABLE | Goel only | as 1.3 |
| 2.10 | Brode 826 / 158 kPa, 18-24% below | FIX | Z=2 gives 156.4 kPa; range 18-25% | (computed) |
