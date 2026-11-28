# Verify: R9-D1-what-shipped-spore-and-sims.md

Re-check done 2026-11-28. The checker re-read the page-marked text of S1 (hecker08b), the S2 and S3 abstracts and slides, and the S5 WebFetch text, located each S5 claim's section by script, and checked the DOIs on Crossref. URLs were re-tried.

| Row | Claim | Source location | Result |
|---|---|---|---|
| 1 | S1 citation, six authors, TOG 27(3) Art. 27 | Crossref (pp. 1-11, including van Prooijen); PDF p.1 author block | VERIFIED |
| 2 | S1 URL status | Author site down; NTU URL TLS error then 500; OpenAlex closed | VERIFIED as stated; FIX applied (download source marked as not logged, not "found through") |
| 3 | S2 DOI and article 83 | Crossref 10.1145/1278780.1278880 | VERIFIED |
| 4 | S3 DOI, article 81, six authors | Crossref 10.1145/1278780.1278878 | VERIFIED |
| 5 | S2 and S3 CMU URLs | cs.cmu.edu/~ajw/s2007 index lists all four files | VERIFIED |
| 6 | S4 down; archives unreachable | curl and WebFetch today | VERIFIED |
| 7 | S5 SIGGRAPH 94 pp. 15-22 | Crossref | VERIFIED |
| 8 | Body block; spline skeleton; metaball mesh; rigblocks attached | S2 slide 13 | VERIFIED |
| 9 | Clay-like torso; 20-80 bodies; caps; DAG with spine chain at root; deform curves | S1 p.2 | VERIFIED |
| 10 | Third-party metaball details | docs.rs symbios-avatar note (secondary) | Marked UNVERIFIED in the text; not relied on |
| 11 | Rigblock unit-interval animations, handles | S2 abstract | VERIFIED |
| 12 | Deforms add, not average; Maya-matching accumulation; Euler order-independence | S2 abstract §3; slide 18 | VERIFIED |
| 13 | Baking and retarget to reduced skeleton | S2 slides 19-24 | VERIFIED |
| 14 | Swarm particle brushes; cubemap UVs; spine and limb paint; oriented strokes; base, coat, detail; themes | S3 abstract; slides 15-16 | VERIFIED |
| 15 | Procedural UV types in the vertex shader; two colours | S3 abstract; slide 18 | VERIFIED |
| 16 | Splatter, charting with LSCM, welding, decimation, AO, background baking | S3 slides 21-26; abstract §4 | VERIFIED |
| 17 | G and S, mirroring flag m | S1 p.3-4 | VERIFIED |
| 18 | Contexts: type, spatial, setspace, extent, limb modifier; auto-query ruled out | S1 p.3 | VERIFIED |
| 19 | Movement-mode table | S1 p.4-5 | VERIFIED |
| 20 | Blend groups; Hermite keys; key remapping; multi-character preview | S1 p.5 | VERIFIED |
| 21 | Branch predicates (three-valued), last resort; variants; variant product; mirroring | S1 p.6 | VERIFIED |
| 22 | Gait: legs, rational ratios, duty factor, step trigger, 1-6 authored and 7 or more procedural, crawl | S1 p.6-7 | VERIFIED |
| 23 | IK objectives and three failure modes | S1 p.7 | VERIFIED |
| 24 | Particles, length constraints, masses, C1 soft correction | S1 p.8 | VERIFIED |
| 25 | Other solvers slower and harder to tune | S1 p.7 | VERIFIED |
| 26 | Spine branch points; least-squares spline; cubic failed, quintic Hermite enough | S1 p.8 | VERIFIED |
| 27 | Anti-buckling; 5 outer iterations; 10% and 120% | S1 p.9 | VERIFIED |
| 28 | Aim preconditioner | S1 p.9 | VERIFIED |
| 29 | Jiggles passive; Wiggles harmed animation | S1 p.9 | VERIFIED |
| 30 | Stochastic testing; about 90%; 0.2 ms, 25 bodies, 1.7 GHz Pentium-M; 35% in IK | S1 p.10 | VERIFIED |
| 31 | Limitations 1-6 (volume, self-collision, failure grace, singularities with footnote 11, learning curve, future work) | S1 p.10 | VERIFIED |
| 32 | Sims genotype: node contents, joint types, recursive limit, terminal-only, reflection, no categories | S5 §2 | VERIFIED |
| 33 | Nested neural graphs; adjacency; central neurons | S5 §3.4 | VERIFIED |
| 34 | Population 300; survival 1/5 | S5 §6 | VERIFIED |
| 35 | Mutation steps; garbage collection | S5 §6.1 | VERIFIED |
| 36 | Crossover and grafting | S5 §6.2 | VERIFIED |
| 37 | Viability: too many parts; persistent interpenetration discarded | S5 §5 | VERIFIED |
| 38 | Exploitation of simulation bugs | S5 §4 | VERIFIED |
| 39 | Skin and materials as future work | S5 §8 | VERIFIED |
| 40 | Lane notes §6 | (mine) | Labelled (mine) |

Fixes: row 2, and slide numbering clarified in the header.
WEAK: the skin method (field kernel, placement, polygonisation, weights) is missing, because S4 is down. S1's free-copy status is weak (the author's site is down; the other URL is unconfirmed). S5 is cited by section because the WebFetch text has no page numbers.
