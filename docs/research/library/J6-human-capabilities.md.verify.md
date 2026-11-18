# Second check: J6-human-capabilities.md (2026-11-18)

This is the checker's summary for this file as relayed in its handoff; the checker's per-row table stayed with the checker.

### Round 2: J6-human-capabilities.md

**Rows: 19 total: 17 VERIFIED, 2 FIX, 0 UNVERIFIABLE.**

- **J6-1 world records: all 6 match** the live World Athletics pages, which I read today.
  - Bolt 9.58 (+0.9), Berlin, 16 Aug 2009.
  - Griffith Joyner 10.49 (0.0), Indianapolis, 16 Jul 1988.
  - Powell 8.95 (+0.3), Tokyo, **30 Aug 1991**.
  - Chistyakova 7.52 (+1.4), Leningrad, 11 Jun 1988.
  - Sotomayor 2.45, Salamanca, 27 Jul 1993.
  - Mahuchikh 2.10, Stade Charléty Paris, 7 Jul 2024.
  - The file gives Powell's date as "1991" only and leaves out the venues for the jumps. That's a minor fill-in, not a FIX.
- The mean speeds recompute: 100/9.58 = 10.44 m/s and 100/10.49 = 9.53 m/s.
- **J6-2 Dodds 2014: values verified.** The paper gives 60,803 obs / 49,964 participants / 26,687 female, 12 GB studies, ages 4-90, and "males' peak median grip was 51 kg between ages 29 and 39, compared to 31 kg in females between ages 26 and 42".
  - **FIX (licence fill-in).** The article is **CC BY** ("distributed under the terms of the Creative Commons Attribution License", © 2014 Dodds et al), confirmed in the Europe PMC full text PMC4256164. The file says the licence line wasn't captured.
- **J6-3 Dorn 2012: values verified** against the journal full text.
  - Population: 5 men and 4 women, 27.7±8.0 y, 73.1±8.6 kg, 176±7 cm, leg 93±5 cm, indoor synthetic track at the AIS.
  - Speeds: 3.5±0.1, 5.2±0.1, 7.0±0.1, 9.0±0.7 m/s. Peak vertical force: "2.7 BW at 3.5 m s⁻¹ to 3.6 BW at 7.0 m s⁻¹ and did not change thereafter". The shift above 7 m/s is in the paper.
  - **FIX (population per speed).** N is **9, 9, 8 and 7** at the four target speeds (3.5, 5.0, 7.0, ≥8.0 m/s), not 9 throughout. The file's "9 experienced runners" holds only for the slow speeds. The same applies to H7's pointer.
  - Licence: not stated in the file. JEB is © The Company of Biologists; facts only.
- NIOSH not-found: honest. I didn't retry it this pass.
- Dead links: none. journals.biologists.com is 403 to curl from the box but reads through a normal fetch.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| L22 | Dodds 2014 source; licence not captured | FIX | It's CC BY (Creative Commons Attribution License) | https://doi.org/10.1371/journal.pone.0113637 ; https://europepmc.org/article/PMC/PMC4256164 |
| L38 | 9 runners (5M/4F), age, mass, height, leg, AIS track | FIX | Values right, but N per speed is 9/9/8/7 | same |
