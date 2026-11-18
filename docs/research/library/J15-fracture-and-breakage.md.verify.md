# Second check: J15-fracture-and-breakage.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its findings; its per-row table stayed with the checker.

### Round 3: J15-fracture-and-breakage.md

**28 rows: 25 VERIFIED, 3 FIX, 0 UNVERIFIABLE.**

FIX:
- Row 2.1 (page refs): Wood Handbook FPL-GTR-282 text on fracture toughness, moisture and speed of sound is on **p. 5-17** (PDF page 133), not p. 5-27. Tables 5-9 and 5-10 are on **p. 5-28** (PDF page 144). Values themselves all match.
- Row 3.2 (licence): OpenStax *University Physics Vol. 1* is **CC BY-NC-SA 4.0**, not CC BY 4.0. The page reads: "This book uses the Creative Commons Attribution-NonCommercial-ShareAlike License", and also says the content may not be ingested into large language models or generative AI offerings without OpenStax's prior written permission. Same error as round-two OpenStax rows. Send to Legal Desk before any use.
- Row D.2: "mode II is 4 to 6 times mode I" does not match the handbook ranges. Range endpoints give 1,650/550 = 3.0 to 2,400/220 = 10.9; like-for-like endpoints give 1,650/220 = 7.5 and 2,400/550 = 4.4; Table 5-10 paired species give 4.7 (Scots pine) to 7.0 (Douglas-fir). Say "about 4 to 7 times (Table 5-10 species), 3 to 11 across the full ranges".

Notes:
- Table 5-10 Mode II orientation: by column position in the PDF text layer, Douglas-fir 2,230, western hemlock 2,240, Scots pine 2,050, southern pine 2,070 and red spruce 2,190 sit under Mode II **TL**; red spruce 1,665 sits under Mode II **RL**. Douglas-fir RL Mode I is 360 (330 lbf). The file's caution is fair; this reading is offered, not certain.
- OpenStax table label is "Hardwood", with no "along grain"; the file's gloss is an addition.
- Wikipedia "Granite": the EduMine ref supports density 2.65-2.75 only; "compressive strength usually above 200 MPa" has no ref of its own (the next ref is for viscosity).
- G = K²/E (plane stress) is unsourced in the file; it matches Wikipedia "Fracture mechanics" (`G = K_I^2/E` for plane stress). Add a source.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| 2.1 | Mode I 220-550, mode II 1,650-2,400 kPa·m^½, "p. 5-27" | FIX | values right; page is 5-17 | https://research.fs.usda.gov/download/treesearch/62200.pdf |
| 3.2 | OpenStax CC BY 4.0 | FIX | CC BY-NC-SA 4.0 + no-AI-ingestion notice | as 3.1 |
| D.2 | Mode II 4-6 × mode I | FIX | 4.7-7.0 by species; 3-11 full ranges | (computed) |
