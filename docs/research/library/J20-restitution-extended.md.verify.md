# Second check: J20-restitution-extended.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its handoff of findings; the checker's full per-row table stayed with the checker.

### Round 3: J20-restitution-extended.md

**37 rows: 33 VERIFIED, 4 FIX, 0 UNVERIFIABLE.**

- FIX (wrong study): the "e = 1.01 for smooth spheres at 1 m/s, scatter < 0.01, systematic error; 0.93 at 0.01 m/s; etched spheres lower, same trend" result is **Beger (2012)**, 10 mm soda-lime **glass** beads (smooth ±0.1 µm, etched ±5 µm) in a 1.5 m vacuum tube, 0.01-1 m/s. It is not King et al. (2011). Güttler et al. section 3.2, PDF p.9. King et al. (6.24-18.76 mm steel, brass, aluminium and Delrin spheres on steel or granite) is reported only as "non-monotonic", with no e values quoted.
- FIX (follows from the above): in "Sources disagree", "smooth steel spheres e above 1 (King et al. 2011)" should be "smooth glass beads, e = 1.01 (Beger 2012)".
- FIX (range): J20-3 says Rocscience Rt runs "0.5 to 0.99", but the file's own table has Rt 0.20 (pyroclastic fill) and 0.24 (detritus fans), and 0.489 (vegetated limestone pile). The range should be 0.20-0.99.
- FIX (attribution): the golf driver values 0.845 at 90 mph and 0.797 at 130 mph are cited on Wikipedia to the USGA article "Do Long Hitters Get An Unfair Advantage?" (https://www.usga.org/content/usga/home-page/articles/2011/04/do-long-hitters-get-an-unfair-advantage-2147496940.html). The 0.83 limit is cited to the USGA conforming-club page. racquetresearch.com/coeffici.htm backs the tennis-racquet 0.85 quote, not golf.
- Note: the "earlier study, e = 0.8 ± 0.06" is Durda et al. (2011): 1 m granite spheres hung on cranes, 0.1-1.5 m/s. Name it.
- Missing spreads:
  - Rocscience's user-feedback rows widen concrete and rock a lot. Takamatsu concrete has Rn 0.48 ± 0.19 and 0.85, and weathered rock 0.47 ± 0.30 up to 1.00. Norway blockfields are 0.50 ± 0.06. All are on the same table page.
  - "Uncovered limestone blast pile" in Rocscience has exactly the same values as "Limestone face" (0.315/0.064/0.712/0.116), which looks like a duplicate in the source.
- Citation detail: Rocscience lists Chau, Wong & Lee as 1996. Crossref gives Int. J. Rock Mech. Min. Sci. 35(4-5):662-663, June **1998**, so the file's 1998 is right.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| 2.3 | King et al. 2011: e = 1.01 … 0.93 … etched | FIX | Beger 2012, glass beads | same, PDF p.9 |
| 3.1 | Rt 0.5-0.99 | FIX | 0.20-0.99 | Rocscience table |
| 4.3 | Golf 0.83 limit; 0.845 at 90 mph to 0.797 at 130 mph; sources | FIX | values right; speed values cite USGA "Long Hitters" article, not racquetresearch | same |
| D.2 | Smooth steel e > 1 (King) | FIX | smooth glass, Beger 2012 | Güttler p.9 |
