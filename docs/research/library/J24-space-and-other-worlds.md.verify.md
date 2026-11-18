# Second check: J24-space-and-other-worlds.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its handoff of findings; the checker's full per-row table stayed with the checker.

### Round 3: J24-space-and-other-worlds.md

**35 rows checked: 33 VERIFIED, 1 FIX, 1 UNVERIFIABLE.** I read the NSSDCA Moon, Mars and Venus sheets directly, and every value in the worlds table matches. All of the computed jump, orbit and geostationary figures recompute. The geostationary note is confirmed: using the equatorial radius gives 1,436.07 min.

FIX:
- Row 29 (human g tolerance): TN D-337 (Creer, Smedal and Wingrove 1960) has no "Fig. 10" for these values. The report says its tolerance data "are presented in figures 6 through 9": figure 6 is eyeballs-out and figure 8 is eyeballs-in, both drawn as plots. The "20 g under 10 s, 10 g for 1 minute, 6 g for 10 minutes" wording is **Wikipedia's summary** ("G-force"). I couldn't read those numbers off the scanned plots, so treat them as Wikipedia-only.
  - Context the file is missing: the report's *new* boundaries come from preconditioned test pilots, with a "roughly sixfold increase" over Clarke's nonpilot data at 7 g eyeballs-out (about 0.6 min against 4 min and up). The report says these boundaries apply "only to a fairly select group".

UNVERIFIABLE:
- Row 26: plume colour by propellant. The file itself says this has no numeric source.

Missing spreads and context:
- Engines:
  - The Merlin 1D Isp values (282/311 s) carry an "update after 2020" tag in the infobox ("figures are from 2013").
  - The Raptor infobox also gives Raptor 3: 250/275 tf, 1,525 kg, T/W 163.9. Raptor 1 is 185 tf, 2,080 kg, T/W 88.94. The T/W figures are sea-level thrust over engine-only mass.
  - The infobox gives F-1 T/W as 94.1. Vacuum thrust over dry mass gives 94.4, and sea-level gives 82.3.
- LRV: the article gives a third top speed. Its record section says "designed with a top speed of about 8 mph", while the intro says 6 mph. Add this to Sources disagree.
- EMU: the article also gives a total shuttle EVA suit weight of 254-275 lb, alongside the ISS figure of 319 lb.
- G-force: Bräck's 214 g is in the article's table and in its prose, which cites news.com.au. Stapp's 46.2 g comes with "more than 25 g for 1.1 s".

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-18) |
|---|---|---|---|---|
| 26 | Plume colours | UNVERIFIABLE | no source, as the file says | n/a |
| 29 | 20 g < 10 s / 10 g 1 min / 6 g 10 min, TN D-337 Fig. 10 | FIX | figs 6-9; values are Wikipedia's wording | https://en.wikipedia.org/wiki/G-force; https://ntrs.nasa.gov/api/citations/19980223621/downloads/19980223621.pdf |
