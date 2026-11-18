# Second check: J7-animal-capabilities.md (2026-11-18)

This is the checker's summary for this file as relayed in its handoff; the checker's per-row table stayed with the checker.

### Round 2: J7-animal-capabilities.md

**Rows: 14 total: 12 VERIFIED, 1 FIX, 1 UNVERIFIABLE.**

- All the numbers in Table J7-1 match the bioRxiv preprint text:
  - "b = 0.24 and 0.27" for running and flying, and swimming b = 0.36;
  - "a = 144 and 26", "nearly six times faster";
  - i = 0.72 flying, 0.6 running, 0.56 swimming;
  - the Fig. 2 legend gives n = 55 flying, 453 running and 109 swimming, which sums to 617;
  - "617 data points for 470 species", 5.7×10⁻⁸ to 108,000 kg, R² = 0.893;
  - "parameter values in Supplementary Table 4".
- The endotherm/ectotherm finding is also correct: endotherms are faster when flying or running, and the pattern reverses in water.
- **FIX (licence missing).** The bioRxiv API lists the preprint's licence as **CC BY-NC-ND** (`cc_by_nc_nd`). The file states no licence. Flag ⚠NC-ND: the facts can be used, but the text and figures can't be reused. I didn't check the Nature Ecol Evol 2017 version's licence; Europe PMC shows it as not open access.
- **UNVERIFIABLE (memory-sourced form).** The "commonly cited form v_max = a·M^b·(1 − e^(−h·M^i))" doesn't come from any source read; the equations are images. The file itself flags it as needing a check, which is honest. It still counts as a memory or secondary value and must not be used until it's read from the paper.
- The h, swimming a and units gap is honest and real. Without Supplementary Table 4 the table can't produce speeds, as the file says.
- Dead links: none. bioRxiv rate-limits curl (HTTP 429) from the box but reads through a normal fetch.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| L7 | Licence (not stated) | FIX | It's CC BY-NC-ND (preprint) | https://api.biorxiv.org/details/biorxiv/10.1101/095018 |
| L9 | v_max = a·M^b·(1 − e^(−h·M^i)) "commonly cited" | UNVERIFIABLE | Equations are images; this is memory-sourced. The file flags it correctly | same |
