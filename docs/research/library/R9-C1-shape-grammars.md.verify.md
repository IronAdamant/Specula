# Verify: R9-C1-shape-grammars.md

Re-check done 2026-11-28. Every page citation was re-searched in the page-marked text of the copies read: stiny72, abop, wonka03, mueller06 and havemann05. URLs were re-fetched and the byte sizes compared, and the DOIs were checked on Crossref.

| Row | Claim | Source location | Result |
|---|---|---|---|
| 1 | S1 mirror URL serves the copy read | curl 200, 10,434,914 bytes = local copy | VERIFIED; mirror legality UNVERIFIABLE, as stated |
| 2 | S2 URL | https URL 200, 16,810,995 bytes = local copy; the http URL returned 500 | FIX applied (https) |
| 3 | S3 TOG 22(3):669-677 | Crossref | VERIFIED |
| 4 | S4 TOG 25(3):614-623 | Crossref | VERIFIED |
| 5 | S1 four parts (terminals, markers, rules, initial shape) | S1 p.6 | VERIFIED |
| 6 | Apply rule: similar part, transformation, substitute | S1 p.7 | VERIFIED |
| 7 | Terminals cannot be erased; stops when no rule applies | S1 p.7 | VERIFIED |
| 8 | Type B logically redundant but kept | S1 p.14 | VERIFIED |
| 9 | 3D: "no worked 3D example on paper" | S1 p.14 says sculptures are shown in the Appendix (p.27-36) | FIX applied |
| 10 | Levels and selection rule; selection rule usable as a halting test | S1 p.16-17 | VERIFIED |
| 11 | Painting rules: Venn sets, operators, partition | S1 p.19 | VERIFIED |
| 12 | Limiting shape fixes the canvas | S1 p.15, p.17, p.20 | VERIFIED (the viewfinder image is my paraphrase) |
| 13 | Emergence makes rule application hard | S3 p.3 | VERIFIED |
| 14 | Derivations steered by a human | S3 p.2 (also S4 p.2) | VERIFIED |
| 15 | Rules abstract and under-specified, so only humans apply them | S4 p.9 | VERIFIED |
| 16 | Turtle interpretation | S2 p.18-19 | VERIFIED |
| 17 | Stochastic probabilities in (0,1], summing to 1, chosen per occurrence | S2 p.40 | VERIFIED |
| 18 | Different seed gives a variation | S2 p.42 | VERIFIED |
| 19 | No matching production: "stated for parametric systems, p.54" | The identity production is stated at S2 p.16 and for context-sensitive systems at p.42; p.54 does not say it | FIX applied |
| 20 | Condition language, logical expressions | S2 p.53-54 | VERIFIED |
| 21 | Inference problem; too limited for higher plants | S2 p.23 | VERIFIED |
| 22 | Growth functions §1.9 | S2 p.48 | VERIFIED |
| 23 | L-systems: growing into each other; local decisions have global impact | S3 p.7 ("Growth Control") | VERIFIED; FIX applied (attributed to S3 only, not S3 and S4) |
| 24 | Split and conversion rules, containment | S3 p.4; the "Definitions 4.5-4.6" label was not found | FIX applied (now cited as §4, PDF p.4) |
| 25 | −∞ exclusion; don't-care interval | S3 p.6 | VERIFIED |
| 26 | One random value per rule, once per building | S3 p.6 | VERIFIED |
| 27 | Blind window only when requested | S3 p.7 | VERIFIED |
| 28 | About 250 rules, 40 attributes, about 2 weeks | S3 p.7 | VERIFIED |
| 29 | Chaos grows with rules; encoding all choices is infeasible | S3 p.2, p.6 | VERIFIED |
| 30 | Corinthian capitals modelled separately | S3 p.7 | VERIFIED |
| 31 | Merge rule could add complexity | S3 p.8 | VERIFIED |
| 32 | S4 priorities, inactive shapes kept | S4 p.3 | VERIFIED |
| 33 | Absolute and relative sizes; Repeat | S4 p.3 | VERIFIED |
| 34 | Occlusion queries; morphological operations would be needed, so approximations are used | S4 p.4-5 | VERIFIED |
| 35 | Sightline visibility; snap lines | S4 p.5 | VERIFIED |
| 36 | Fifty percent side wing; rand ranges | S4 p.6 | VERIFIED |
| 37 | Unwanted intersections cut windows | S4 p.2 (Fig. 2) | VERIFIED |
| 38 | Implausible configurations, GIS footprints; contrived rules; design unexplored; global optimisation; Boolean robustness | S4 p.9 | VERIFIED |
| 39 | GML is stack-based, PostScript-like | S5 p.219, p.226 | VERIFIED |
| 40 | Euler operators closed and complete (Theorem 2.29) | S5 p.84 | VERIFIED |
| 41 | Precondition example (same face on both sides) | S5 precondition table p.79 | FIX applied (page added) |
| 42 | Valid mesh criteria | S5 p.75 | VERIFIED |
| 43 | Self-intersection: "p.88, often tolerated for rendering" | S5 p.88-89: hard to detect (Fig. 2.22); restricting parameters would be unnatural and expensive | FIX applied (reworded to the source) |
| 44 | Undo macros | S5 p.48-49 | VERIFIED |
| 45 | Python prototype undo | S5 p.46: no invertible tools; full rebuild; interactivity fades beyond about 30 steps | FIX applied (made precise) |
| 46 | Grammar controllability; debugging facility mandatory | S5 p.35 | VERIFIED |
| 47 | GENMOD drawbacks; interval inclusion functions | S5 p.38 | VERIFIED |
| 48 | Lane notes §7; checkable-before-running items | (mine) | Labelled (mine) |

Fixes: rows 2, 9, 19, 23, 24, 41, 43, 45.
WEAK: Stiny 1980 (S1b) was not read. S1's published page range is not verified. The Havemann thesis was only partly read. Randomness in GML was not searched in full.
