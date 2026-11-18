# Second check: J27-sound-loudness.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its handoff of findings; the checker's full per-row table stayed with the checker.

### Round 3: J27-sound-loudness.md

**15 rows checked: 11 VERIFIED, 2 FIX, 2 UNVERIFIABLE.** I recomputed the fall-off table (6.02, 12.04, 18.06, 24.08, 30.10, 36.12 and 42.14 dB) and the chainsaw example (110 − 40 = 70 dB at 100 m). Every SPL row matches the live "Examples of sound pressure" table on Wikipedia, with its distance and its citation. The rows the file left out (the .30-06 rifle, the jackhammer, and the jet engine at 100-30 m) really are tagged citation needed (April 2026).

FIX:
- Row 3 (Krakatoa): the article cites Winchester 2003 at **p. 218**, not p. 2. It also cites Brüel & Kjær, whose quote says **160 km** (8.5 kPa, 172 dB), against 165 km in the table. That gap should go in Sources disagree.
- Row 14 (Sources disagree, threshold of pain): the 120-134 dB range is not from one source. The article draws it from **three references combined**: Nave (HyperPhysics), NIOSH *Preventing Occupational Hearing Loss* 1996 p. 88, and Shure. The HyperPhysics page itself didn't return text to curl, so I couldn't confirm what it says.

UNVERIFIABLE:
- Row 2a: air absorption (ISO 9613-1 not opened, as the file says).
- Row 2b: line source falls 3 dB per doubling (unsourced, as the file says).

Missing spreads and context:
- Balloons: the table also lists a balloon **crushed** to rupture, at 159 dB at the ear and 147 dB at 0.95 m.
- The *Canadian Audiologist* citation is internally inconsistent: it is dated 9 January 2014, but the URL path says volume 3, issue 6, 2016.
- NIOSH: the same table gives the exchange ladder of 88 / 91 / 94 / 97 / 100 dBA for 4 h / 2 h / 1 h / 30 min / 15 min. That is useful for exposure timers in the game.
- Vacuum cleaner: the source is Simpson and Bruce, EPA Rept. No. 550/9-81-101.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-18) |
|---|---|---|---|---|
| 2a | Air absorption (ISO 9613-1) | UNVERIFIABLE | not opened | n/a |
| 2b | Line source 3 dB per doubling | UNVERIFIABLE | unsourced | n/a |
| 3 | Krakatoa 165 km 172 dB, Winchester p. 2 | FIX | p. 218; B&K says 160 km | https://en.wikipedia.org/wiki/Sound_pressure |
| 14 | Disagree: pain range "in the cited source" | FIX | three sources | https://en.wikipedia.org/wiki/Sound_pressure |
