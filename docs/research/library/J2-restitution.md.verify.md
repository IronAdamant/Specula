# Second check: J2-restitution.md (2026-11-18)

This is the checker's summary for this file as relayed in its handoff; the checker's per-row table stayed with the checker.

### Round 2: J2-restitution.md

**Rows: 22 total: 17 VERIFIED, 4 FIX, 1 UNVERIFIABLE.**

- **FIX (method note, L5).** The ITF *does* say where heights are measured. Technical Booklet 2026, Test Method TB 04/01 (printed p. 11) says the ball is dropped from 254 ± 0.3 cm "measured from the bottom of the ball" and the rebound is measured "again to the bottom of the ball". So ITF and FIBA both measure to the underside. ITTF still doesn't say in the text read.
- **FIX (page ref, L14-17).** The 10-and-under table, Appendix I(d), is on **printed p. 20 (PDF p. 23)**, not printed p. 19. Printed p. 19 (PDF p. 22) holds Appendix I(b)-(c). All the numbers match.
- **FIX (edition label, L33).** https://theifab.com/laws/latest/the-ball/ now serves **Laws of the Game 2026/27**; 2025/26 is the previous edition. The Law 2 values are unchanged: 68-70 cm, 410-450 g at the start of the match, 0.6-1.1 atm at sea level.
- **FIX (row L14 drop height "assumed").** The assumption holds: Appendix I(e) says all rebound tests follow the ITF regulations, and TB 04/01 sets 254 cm. It can be stated as sourced rather than assumed.
- **Every e value recomputes exactly** as e = √(h_r/h_d). The tennis values are 0.737-0.771, 0.729-0.761, 0.693-0.729, 0.578-0.643, 0.595-0.643, 0.643-0.687 and 0.687-0.729. Basketball is 0.758-0.776, table tennis 0.887-0.923, and the Nittaku upper bound 0.932.
- **Secondary-only flags.** The table tennis rebound of 240-260 mm from 305 mm and the 2.67-2.77 g tolerance are still only from the silo.tips mirror of ITTF T3 (06/10). The official 2011 copy exists in the Wayback Machine (web.archive.org/web/20110304120600/http://www.ittf.com/stories/pictures/T3_40mmBall.pdf), but archive.org doesn't open from my computer. Table Tennis Canada's Q&A (ttcan.ca Plastic_Balls_Q_A.pdf) independently gives 240-260 mm. **Law 2.3.2 (2.7 g) is now confirmed in the official ITTF Handbook 2020**, together with 2.3.1 (40 mm diameter), which gives the table tennis size the file lists as "not read".
- **UNVERIFIABLE:** the Nittaku 240-265 mm figure (L38) has no URL in the file. A PingSkills forum post gives 240-265 mm for the older celluloid balls, which suggests that's an old celluloid spec rather than a disagreement.
- Dead links: none. ITF Rules PDF, ITF Technical Booklet PDF, FIBA PDF and IFAB page all return 200.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note / correction | URL checked (2026-11-17) |
|---|---|---|---|---|
| L5 | Heights measurement point: ITF doesn't say | FIX | TB 04/01: measured from and to the bottom of the ball | https://www.world.tennis/media/15648/2026-technical-booklet.pdf |
| L14 | Stage 3 red foam 85-105 cm, e 0.578-0.643, "printed p. 19" | FIX | The values match. Page is printed p. 20 / PDF p. 23. Drop height is sourced via I(e) + TB 04/01, not just assumed | same |
| L33 | Football 410-450 g, 68-70 cm, 0.6-1.1 atm, Laws 2025/26 | FIX | The values match. The latest URL is now the 2026/27 edition | https://theifab.com/laws/latest/the-ball/ |
| L38 | Nittaku 240-265 mm | UNVERIFIABLE | No URL. 240-265 mm matches the old celluloid spec quoted on PingSkills | https://www.pingskills.com/table-tennis-forum/the-table-tennis-ball-bounce |
