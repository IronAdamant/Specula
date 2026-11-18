# Second check: J30-sword-balance-percussion-inertia-and-cutting-tests.md (2026-11-18)

This is the second checker's summary for this note, as relayed in the checker's findings for round four; its per-row table stayed with the checker. The text below is copied word for word, leaving out only the lines that named the checker's own file locations.

#### Round 4: J30-sword-balance-percussion-inertia-and-cutting-tests.md

**30 rows checked: 28 VERIFIED, 0 FIX, 2 UNVERIFIABLE.**

I opened the Pulaczewski repository PDF (28 pages), and every page reference in the file is correct. The page references point to PDF pages, and each printed page number is one lower.
- Table 2 is on p.7, the "blade-heavy" text on p.17, Table 3 on p.18, the I = LdM formula on p.20, Appendix 1 on p.22 and Appendix 2 on pp.23-24.
- All the numbers match: masses, SDs, ranges, CoP fractions, the collected data, the kinetic-energy ranges and the 8 buckling forces.
- My recomputed figures match the file:
  - I = 0.0966 and 0.0289 kg m²
  - R = 0.333 and 0.243 m
  - all the inch and lb conversions
  - the CoP at 51-66% of blade length

The Albion specs (myArmoury Agincourt and the two Tritonworks pages) match word for word. Every result in the Edelson forum post matches, including the 108 J figure credited to Benjamin H. Abbott.

UNVERIFIABLE:
- **Row 18, the Albion Mercenary specs from the SBG forum:** the page returned HTTP 202 with a bot challenge, so I couldn't read it. It stays snippet-only.
- **Row 27, the Bronze Age cutmark study (JASR):** ScienceDirect returned 403, so the snippet facts remain unchecked (11.25 g/100 ml, 180 cm, 77.6 kg, 13 strikes, 11 with damage, depths under 1 cm).
  - Crossref confirms the title, the journal, **vol. 48, article 103843, April 2023**, and the authors **Strong and Fibiger**. The file gives no authors.
  - Its licence is the Elsevier user licence, which is not open.

Can upgrade or fill:
- **Kiel page:** it opens now (HTTP 200). Both formulas are on it (I_A = T²·m·g·p/4π², and s_C = I_PP/(m·p_PC)). The author is shown at the bottom as **"© H. Föll (Iron, Steel and Swords script)"**. The file says the author isn't shown and lists it as snippet only, so both can be upgraded to opened.
- **Williams, Edge, Capwell and Tschegg 2012 (Gladius 32):** the article page now loads (HTTP 200, though the host's SSL certificate fails a normal check). It shows CC BY 4.0, matching the file, and Crossref gives **pp. 131-184**. The gap can be retried.

Minor notes:
- The text "Paper © Maciej Pulaczewski 2018" is on PDF p.2, the journal notice. The cover sheet (p.1) carries the repository's personal-research terms. Both are as the file describes, but the © line isn't on the cover sheet itself.
- Table 2 lists IX 2804 twice (as "IX 2804" and "IX2804", both 453 g). That looks like a duplicate in the source, so the n=13 mean may count one sword twice. The file should flag this.
- The Ensis Sub Caelo author is "Vincent". The Kiel page credits a related figure to "Vincent Le Chevalier's homepage", which suggests a surname, but the blog doesn't confirm it.
- Edelson's 50 lb result at 15 ft: the arrows were stopped, but "some of the links were bent, one was broken". The file says only "all stopped".

FIX and UNVERIFIABLE rows:

| Row | Claim | Status | Note | URL checked (2026-11-18) |
|---|---|---|---|---|
| 18 | Albion Mercenary (SBG snippet) | UNVERIFIABLE | HTTP 202 challenge | https://sbg-sword-forum.forums.net/thread/11088/review-albion-mercenary |
| 27 | JASR 2023 Bronze Age snippet facts | UNVERIFIABLE | ScienceDirect 403 | https://doi.org/10.1016/j.jasrep.2023.103843 |
