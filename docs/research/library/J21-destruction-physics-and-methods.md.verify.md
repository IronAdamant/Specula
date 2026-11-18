# Second check: J21-destruction-physics-and-methods.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its handoff of findings; the checker's full per-row table stayed with the checker.

### Round 3: J21-destruction-physics-and-methods.md

**57 rows checked: 51 VERIFIED, 5 FIX, 1 UNVERIFIABLE.** Every "computed by me" value recomputes (Table 2a, kinetic energy, hammer table, char post, free-fall table).

FIX:
- Row 16: the Mott & Linfoot / Grady (2008) statement comes from Ramesh, Hogan, Kimberley, Stickle & Tonge, "A Review of Mechanisms and Models for Dynamic Strength, Dynamic Failure, and Fragmentation" (preprint submitted to Planetary and Space Science), UAlberta repository, text lines 546-552. Name the review's authors; the "search extract" flag can be dropped, as I read the text directly.
- Row 21: the Wood Handbook page is wrong (inherited from J15). The fracture-toughness text is on p. 5-17 and Tables 5-9/5-10 are on p. 5-28, not p. 5-27. The values are right.
- Row 26: the lignin (225-450 °C peaks) and cellulose (300-350 °C) statements are on Wood Handbook p. 18-10, not p. 18-15. The char rates are correctly on p. 18-15.
- Row 47 (Red Faction): "level artists had to learn to build like structural engineers" is not in the Digital Foundry article. What it does say is Dave Baranec: the system "makes level design extremely hard" and makes memory use "staggeringly high". Swap in that quote or cite where the engineer line comes from. The GDC Vault page marks the session "free content", so the Not found note "members only" should be rechecked.
- Row 50 (Unreal Chaos): the cited Destruction Overview page supports pre-fractured geometry collections, a connection graph from nearest neighbours with initial strain values, and breaking when an impulse exceeds the limit. It does not mention Voronoi patterns or a per-level damage threshold. Either drop those or cite the Fracture Mode docs or the GDC talk.

UNVERIFIABLE:
- Row 57: the "Computational Materials Science 2006 abstract" behind "measured fragments much smaller" is still a search extract. I didn't find the primary. Drugan 2001 supports the gist on its own (row 56).

Missing spreads and context:
- Row 10: the compressive-to-tensile ratio "about 10×" really spans 8-20× in the file's own UTS numbers (concrete 2-5 MPa against 20-40+ MPa). Give the range.
- Row 1: the Concrete page also gives "20 to 32 MPa for most routine uses". Add it as the everyday band.
- Row 23: the EN 1993-1-2 table leaves out the 1,100 °C row (k_y 0.02, k_E 0.0225 per FireSolver). That matters for interpolation between 1,000 and 1,200 °C.
- Row 24: Promat gives 550 °C for columns and 620 °C for beams as conservative limiting temperatures. The file only has 550.
- Row 25: Wood Handbook Table 18-3 (p. 18-15) gives per-species charring data. That's a spread the file doesn't use.
- Row 11/56 (important for the engine): Drugan's abstract says the energy models agree with the dynamic models only at extremely high strain rates (about 5 × 10^7 s⁻¹ for dense alumina), and that "the regime of validity of the energy-based models is rather restricted". At quasi-static rates the fragment size stops depending on strain rate. Table 2a's rates of 10 to 10^4 s⁻¹ sit far below that, so its sizes are an upper bound, not an estimate. Say so under the table.
- Row 37: the 5.4 s / 40% figure comes from the NIST draft report (Aug 2008), restated in the FAQ against a 3.9 s free-fall time. The FAQ explains the extra time as Stage 1 buckling.

Upgrades (search extract to primary, read directly):
- Row 36: NIST NCSTAR 1A p. 45 (PDF p. 87) prints the free-fall drop as "approximately 8 stories or 32.0 m (105 ft), the distance traveled between times t = 1.75 s and t = 4.0 s". The FAQ gives the stages as 0-1.75 s, 1.75-4.0 s and 4.0-5.4 s.
- Row 54: I read the Müller et al. PDF directly. It covers VACD, Voronoi fracture patterns placed by impact location, and partial fracturing. Crossref lists it in ACM Transactions on Graphics (SIGGRAPH 2013).
- Row 45: the Voxagon 2026 post confirms the fixed-point integer destruction rewrite, the deterministic command examples, and "around one Mbit per client" for state sync.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-17/18) |
|---|---|---|---|---|
| 16 | Mott & Linfoot 1943; Grady 2008 ductile exponential vs brittle power law | FIX | name Ramesh et al. review authors; read directly, lines 546-552 | https://ualberta.scholaris.ca/server/api/core/bitstreams/64de9741-8620-47fc-83d8-a0985a572bb9/content |
| 21 | Wood Handbook p. 5-27; mode I 220-550, shear 1,650-2,400 kPa·m^½ | FIX | text p. 5-17, Tables 5-9/5-10 p. 5-28 | https://research.fs.usda.gov/download/treesearch/62200.pdf |
| 26 | Lignin peaks 225-450 °C; cellulose 300-350 °C | FIX | page is 18-10 | same |
| 47 | Red Faction: top-to-bottom stress scan, creaks, Havok, memory; "structural engineers" | FIX | engineer line not found; use Baranec quote | https://www.digitalfoundry.net/articles/digitalfoundry-red-faction-guerrilla-big-bang-theory-article ; https://gdcvault.com/play/1672/Blowing-Up-the-Outside-World |
| 50 | Unreal Chaos: Voronoi pre-fracture, per-level damage threshold, connection graph | FIX | Voronoi and per-level threshold not on the page | https://dev.epicgames.com/documentation/en-us/unreal-engine/destruction-overview |
| 57 | Same, Computational Materials Science 2006 abstract | UNVERIFIABLE | search extract only |, |
