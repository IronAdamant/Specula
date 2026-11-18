# J2: Bounce (coefficient of restitution)

Fixed after the second check (2026-11-18):
- Where heights are measured: the ITF does say (TB 04/01: from and to the bottom of the ball). The method note is corrected.
- The 10-and-under table, Appendix I(d), is on printed p. 20 (PDF p. 23), not printed p. 19. Its 254 cm drop height is now stated as sourced (Appendix I(e) and TB 04/01), not assumed.
- The IFAB page now serves *Laws of the Game 2026/27*; the Law 2 values are unchanged.
- Nittaku's 240–265 mm moved to "Unconfirmed" (no URL; likely the old celluloid specification).
- Added: the ITTF Handbook 2020 confirms law 2.3.2 (2.7 g) and law 2.3.1 (40 mm diameter); Table Tennis Canada independently gives 240–260 mm; the official ITTF leaflet's Wayback address. Added a Sources list. The checker's summary is in `J2-restitution.md.verify.md`.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Sports governing bodies publish drop-and-rebound tests for their balls. I computed a coefficient of restitution from each.

**How e was computed (by me):** e = √(h_rebound / h_drop). This ignores air drag, which matters for a light, fuzzy tennis ball, so the true e at impact is a little higher than shown. It also assumes both heights are measured to the same point on the ball. FIBA says it measures both to the underside of the ball, and so does the ITF: its Technical Booklet 2026, Test Method TB 04/01 (printed p. 11), drops the ball from 254 ± 0.3 cm "measured from the bottom of the ball" and measures the rebound "again to the bottom of the ball" (https://www.world.tennis/media/15648/2026-technical-booklet.pdf, second check 2026-11-17). ITTF does not say in the text read.

## Table J2-1. Governing-body ball rebound specifications (read directly) and e (computed)

| Ball | Drop height | Rebound | Surface | e (computed) | Source |
|---|---|---|---|---|---|
| Tennis, Type 1 (fast) | 254 cm (100 in) | 138–151 cm | rigid surface per ITF test | 0.737–0.771 | ITF *Rules of Tennis 2026*, Appendix I, PDF pp. 22–23, https://www.world.tennis/media/7221/2026-rules-of-tennis-english.pdf; test method ITF Technical Booklet TB 04/01 (254 ± 0.3 cm drop), https://www.world.tennis/media/15648/2026-technical-booklet.pdf |
| Tennis, Types 2 and 3 | 254 cm | 135–147 cm | same | 0.729–0.761 | same |
| Tennis, high altitude | 254 cm | 122–135 cm | same | 0.693–0.729 | same |
| Tennis, 10-and-under Stage 3 red foam | 254 cm (sourced: Appendix I(e) says all rebound tests follow the ITF regulations, and TB 04/01 sets 254 cm) | 85–105 cm | same | 0.578–0.643 | ITF 2026 Appendix I(d), printed p. 20 (PDF p. 23), same PDF; drop height from Appendix I(e) and TB 04/01 |
| Tennis, Stage 3 red standard | 254 cm (sourced, as above) | 90–105 cm | same | 0.595–0.643 | same |
| Tennis, Stage 2 orange | 254 cm (sourced, as above) | 105–120 cm | same | 0.643–0.687 | same |
| Tennis, Stage 1 green | 254 cm (sourced, as above) | 120–135 cm | same | 0.687–0.729 | same |
| Basketball (all sizes) | about 1,800 mm, to underside of ball | 1,035–1,085 mm, to underside of ball | per FIBA test | 0.758–0.776 | FIBA *Official Basketball Rules 2024: Basketball Equipment*, §2.4 and Table 1, printed p. 12 of 32, https://assets.fiba.basketball/image/upload/documents-corporate-fiba-official-rules-2024-official-basketball-rules-and-basketball-equipment.pdf |
| Table tennis | 305 mm | 240–260 mm | standard steel block | 0.887–0.923 | ITTF Technical Leaflet T3 "The Ball", 06/10, read from a third-party mirror (silo.tips), so secondary. The ITTF original was not reached; an official 2011 copy is in the Wayback Machine (https://web.archive.org/web/20110304120600/http://www.ittf.com/stories/pictures/T3_40mmBall.pdf, not opened by either check). Table Tennis Canada's Q&A (ttcan.ca, Plastic_Balls_Q_A.pdf) independently gives 240–260 mm (per the second check). |

## Table J2-2. Ball sizes and masses from the same documents (read directly)

| Ball | Mass | Size | Pressure | Source |
|---|---|---|---|---|
| Tennis (Types 1 and 2) | 56.0–59.4 g | diameter 6.54–6.86 cm | not specified in the rule | ITF 2026 Appendix I (as above) |
| Tennis Type 3 (slow) | 56.0–59.4 g | diameter 7.00–7.30 cm | | same |
| Tennis high altitude | 56.0–59.4 g | 6.54–6.86 cm | pressurised; for play above 1,219 m | same |
| Tennis 10-and-under: red foam, red, orange, green | 25.0–43.0, 36.0–49.0, 36.0–46.9, 47.0–51.5 g | 8.00–9.00, 7.00–8.00, 6.00–6.86, 6.30–6.86 cm | | ITF 2026 Appendix I(d), printed p. 20 (PDF p. 23) |
| Basketball size 7 | 580–620 g | circumference 750–770 mm | | FIBA 2024 Table 1 (as above) |
| Basketball size 6 | 510–550 g | 715–730 mm | | same |
| Basketball size 5 | 465–495 g | 685–700 mm | | same |
| Basketball size 5, lightweight | 360–390 g | 685–700 mm | | same |
| Football (soccer) | 410–450 g at the start of the match | circumference 68–70 cm | 0.6–1.1 atm at sea level | IFAB *Laws of the Game 2026/27*, Law 2 (the values are unchanged from 2025/26, the edition first read), https://theifab.com/laws/latest/the-ball/ |
| Table tennis | 2.7 g (law 2.3.2); leaflet tolerance 2.67–2.77 g | 40 mm diameter (law 2.3.1) | | Laws 2.3.1 and 2.3.2: official ITTF Handbook 2020 (confirmed by the second check, 2026-11-17). Tolerance 2.67–2.77 g: ITTF T3 via mirror only (secondary) |

## Sources disagree

- Table tennis rebound: the ITTF leaflet (via mirror) and Table Tennis Canada give 240–260 mm from 305 mm. Nittaku's 240–265 mm, listed here before, could not be confirmed and is likely the old celluloid specification rather than a disagreement (see "Unconfirmed" below). Use the ITTF range.

## Not found

- Restitution for everyday material pairs (wood on wood, steel on concrete, rubber on wood) in an open primary table. Not added rather than guessed.
- Golf and baseball restitution limits: not read from the rule documents in this pass.

## Unconfirmed (second check, 2026-11-18)

Not to be used until a source that gives the value has been opened.

- **Nittaku table-tennis rebound, 240–265 mm from 305 mm (e up to 0.932, computed).** The file gave no URL for "Nittaku's product notes". The checker found 240–265 mm only in a PingSkills forum post, where it is given for the older celluloid balls (https://www.pingskills.com/table-tennis-forum/the-table-tennis-ball-bounce), which suggests an old celluloid specification rather than a disagreement with ITTF.

## Sources

- ITF, *Rules of Tennis 2026*, Appendix I (printed pp. 19–20, PDF pp. 22–23), https://www.world.tennis/media/7221/2026-rules-of-tennis-english.pdf (accessed 2026-11-17).
- ITF, *Technical Booklet 2026*, Test Method TB 04/01 (printed p. 11), https://www.world.tennis/media/15648/2026-technical-booklet.pdf (accessed 2026-11-17).
- FIBA, *Official Basketball Rules 2024: Basketball Equipment*, §2.4 and Table 1, https://assets.fiba.basketball/image/upload/documents-corporate-fiba-official-rules-2024-official-basketball-rules-and-basketball-equipment.pdf (accessed 2026-11-17).
- IFAB, *Laws of the Game 2026/27*, Law 2 The Ball, https://theifab.com/laws/latest/the-ball/ (accessed 2026-11-17).
- ITTF Technical Leaflet T3 "The Ball" (06/10), read from a third-party mirror on silo.tips (secondary; accessed 2026-11-17). Official 2011 copy (not opened): https://web.archive.org/web/20110304120600/http://www.ittf.com/stories/pictures/T3_40mmBall.pdf.
- ITTF Handbook 2020, laws 2.3.1 and 2.3.2 (opened by the second check, 2026-11-17; its address was not given in the checker's summary).
- Table Tennis Canada, Plastic Balls Q&A (ttcan.ca, Plastic_Balls_Q_A.pdf; per the second check, 2026-11-17; full address not given in the checker's summary).
- PingSkills forum, "The table tennis ball bounce", https://www.pingskills.com/table-tennis-forum/the-table-tennis-ball-bounce (checked by the second check, 2026-11-17; for the unconfirmed Nittaku row only).
