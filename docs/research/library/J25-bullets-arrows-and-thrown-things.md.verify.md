# Second check: J25-bullets-arrows-and-thrown-things.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its handoff of findings; the checker's full per-row table stayed with the checker.

### Round 3: J25-bullets-arrows-and-thrown-things.md

**31 rows checked: 27 VERIFIED, 4 FIX, 0 UNVERIFIABLE.** All ten cartridge rows match their Wikipedia infobox, including the balsrc fields and test barrels. The ft·lbf to J conversions are right, including 9 mm XTP: the infobox says 310, and 310 ft·lbf is 420 J. Every row in J25-2 recomputes (momentum, 4 kg recoil speed and vacuum drop), and so do the trajectory checks (124.9 m and 63.7 m).

FIX:
- Row 11 (.22 LR editor note): The hidden comment in the article covers the **32 gr / 1,640 ft/s load (en5 = 191)**. It works out about 203 J by ½mv² against the listed 191. It does not cover the 40 gr load. The file's 40 gr comparison (191 J computed against 131 ft·lbf = 178 J) is the researcher's own arithmetic. It is correct, but it should be labelled "computed by me", not credited to the page note.
- Row 21 (Mary Rose draw weights, 150-160 lbf at 30 in; range 100-185 lbf): Wikipedia cites **Strickland and Hardy 2005 (*The Great Warbow*), p. 17**. It does not cite Hardy's *Longbow*.
- Row 26 (atlatl): The abstract says "the atlatls used in hunting and warfare probably did not accelerate darts much beyond 35 m/s (78 mph)". That is a claim about the weapon systems, not "well-practised throwers rarely exceed". Reword it and add the pages: *PaleoAmerica* 3(2): 161-181 (checked via Crossref and OpenAlex).
- Row 27 (slings): Skov says "adults averaged 70 meters and **males** averaged 78 meters" (Vega and Craig 2009: 1267). It does not say "adult males 78 m". The Finney figures are on p. 69.

Missing spreads and context:
- Longbow: Wikipedia gives **replicas tested at 445-823 N**, which sits between the 450 N (degraded originals) and 670-710 N (estimates) in Sources disagree. Kooi's separate 450 N is Rees 1993's calculation for a 60 g Agincourt arrow. It is not the destruction-test figure, so keep the two apart.
- Slings: Skov's own paper gives a published **45 m/s from Korfmann 1973** (an informal experiment), and says "a throw can exceed 200 m" at about 45 m/s. Use that as the high-side source in place of the forum post.
- Aston and Spencer: arrow AZ472/19 is 62.5 g with an M2 Tudor bodkin. The 255 yd figure was "recorded with MR X1-3/S1 in favourable atmospheric conditions", not the S3 row of the table.
- Compound bow: same page also gives adult draw weights of 40-80 lb and arrow speeds of 250-370 ft/s. The Design section carries a "More citations needed" tag.
- 9 mm infobox: the Norma figures use a 431 mm (17 in) barrel, not 118 mm.
- 7.62x51 M118: the balsrc names U.S. Armament for M118 and the TMs for M80. "Same" is loose wording.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-18) |
|---|---|---|---|---|
| 11 | .22 editor note | FIX | the note covers the 32 gr load | https://en.wikipedia.org/wiki/.22_long_rifle |
| 21 | 150-160 lbf, 100-185 lbf (Hardy) | FIX | Strickland and Hardy 2005, p. 17 | https://en.wikipedia.org/wiki/English_longbow |
| 26 | Atlatl ~35 m/s | FIX | wording; pp. 161-181 | https://doi.org/10.1080/20555563.2017.1301133 |
| 27 | Sling figures | FIX | males 78, adults 70 | https://digitalcommons.unl.edu/cgi/viewcontent.cgi?article=1168&context=nebanthro |
