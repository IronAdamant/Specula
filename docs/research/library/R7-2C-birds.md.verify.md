# Verify: R7-2C-birds.md (adversarial second check, 2026-11-24)

**Method:** the number-presence script against the B1 text and supplement tables, the B2, B3 and B4 texts, followed by an independent back-transformation of B1 Table S4 from the raw log values (`bs/t3.tsv`).

| Check | Result |
|---|---|
| B1 Table S3 (8 rows) | match the supplement table |
| B1 named counts (swan 23, emu 18, nandu 15, chicken 14, macaw 10; none with 20-22) | found in the text |
| B1 Table S4 back-transformation (**computed**) | Recomputed from the raw log values; all 13 rows match. The internal check that femur + tibiotarsus + tarsometatarsus = tll holds, which supports the inferred log10-of-metres units |
| B1 method (calliper ±0.01 mm, discs excluded, atlas without a centrum, literature body mass) | found in the text and the supplement legend |
| B3 quote (48% to 53 ± 1%), N = 5, 3 g, 0-12 m/s | found word for word |
| B4 (61 species, 125 individuals, method) | found |
| Citations | Crossref checked for B1-B4; licences from the XML (B1, B2 CC BY 4.0; B4 CC BY-NC 4.0) |

**Errors found and fixed:**
1. My first note on the swan 23 vs 26 disagreement said B1 counts to "the last vertebra whose rib does not reach the sternum". That is backwards: B1 follows Romer (cervicals are the vertebrae in front of the first vertebra with freely movable ribs) and rejects Baumel's sternal-rib definition. Corrected.
2. B1 body masses were first treated as specimen masses. B1 says they are literature species means; corrected in the column heading.

**Remaining weaknesses:**
- The units of Table S4 are inferred, not stated.
- n = 1 per species.
- The gull mass is anomalous (flagged).
- No toes, girth or wing-fold numbers; the B4 data S1 bundle was truncated.
