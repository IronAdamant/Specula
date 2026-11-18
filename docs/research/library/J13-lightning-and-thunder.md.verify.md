# Second check: J13-lightning-and-thunder.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its findings; its per-row table stayed with the checker.

### Round 3: J13-lightning-and-thunder.md

**Rows: 32 total: 30 VERIFIED, 1 FIX, 1 UNVERIFIABLE.**

- **FIX (Sources disagree, channel temperature):** the file says Wikipedia's "about 30,000 °C" cites "the same NWS page". It doesn't: Wikipedia cites a different NWS page, "How Hot Is Lightning?", at https://weather.gov/safety/lightning-temperature (ref name DepCom2025).
  - The NWS pages the file uses say 50,000 °F (about 27,760 °C). The disagreement stands, but the attribution needs correcting.
- **UNVERIFIABLE:** the megaflash duration of **7.391 s**.
  - The WMO release I opened gives 829 ± 8 km, 22 October 2017, and eastern Texas to near Kansas City, but no duration.
  - The BAMS PDF returns 403 to a direct fetch. The value rests only on the file's search-result extract.
- **NOAA pages, all matching:**
  - NWS overview: 50,000 °F and "about 5 times hotter than the surface of the sun"; 25 million flashes a year in the US; heard about 10 miles away; strikes 10 miles out; "a mile every 5 seconds".
  - NWS Thunder page: 50,000 °F, 5 times; "only about 10 miles"; crack or click means nearby; rumble means "at least several miles away"; the loud boom is the main channel reaching the ground.
  - NSSL: "around 50,000 °F, much hotter than the sun" (no "5 times" on this page); "within ten yards or so"; up to 25 miles; tearing, then click or crack, then rumbling; "less than roughly a hundred yards"; "dividing by 3 gives kilometers".
- **HyperPhysics** (curl 500; opened with a web fetch) matches: 0.2 s, strokes under 1 ms, flicker; Williams' "several hundred million volts" and up to 10 kA; Uman's 5,000-20,000 A, Apollo 15 at 100,000 A, over 200,000 A, about 100 A continuing current, and 15,000-60,000 °F.
  - **Small wording issue:** HyperPhysics says continuing current "is the cause of lightning fires", not "most lightning-started fires".
- **Wikipedia "Lightning"** (secondary) matches:
  - Median 0.52 s (Kákona 2023); strokes "around 60 to 70 microseconds", citing HyperPhysics, though that page does not give that number. Treat it as Wikipedia-only.
  - Leader "hundreds of milliseconds" against "a few dozen microseconds"; 30 kA, 15 C, 1 GJ; positive flashes 120 kA and 350 C (Hasbrouck, LLNL S&TR); 100,000 km/s (Idone et al.).
  - "In excess of 5 km"; 44 (± 5) per second, about 1.4 billion (Oliver 2005, p. 452 link); Maracaibo 250 a day (Albrecht et al., TRMM); the stage list (Cooray 2014, pp. 119-229).
- **Computed values recompute:**
  - 50,000 °F is 27,760 °C.
  - Delay table at 343 m/s: 1 s 343 m, 3 s 1.03 km, 5 s 1.72 km, 10 s 3.43 km, 30 s 10.29 km, 47 s 16.12 km (10 mi = 16.09 km).

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| Megaflash duration | 7.391 s | UNVERIFIABLE | BAMS 403; snippet only | https://journals.ametsoc.org/view/journals/bams/106/9/BAMS-D-25-0037.1.pdf |
| Disagree temperature | Wikipedia "citing same NWS page" | **FIX** | Cites weather.gov/safety/lightning-temperature | https://en.wikipedia.org/wiki/Lightning |
