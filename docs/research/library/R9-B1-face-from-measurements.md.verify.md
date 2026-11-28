# R9-B1 verify: second, independent check

Re-check done 2026-11-28 against the downloaded copies: the DeCarlo thesis (`decarlo98thesis.pdf`, 133 PDF pages), Blanz-Vetter (`blanz99u.pdf`, 8 pp.) and Kesterke 2016 (Europe PMC XML PMC4841054). Every page cite was searched for again in a page-marked text dump (one `=== PDF p.N ===` marker per page). Symbol-damaged spots were checked on page images. Citations were checked against Crossref.

| Row | Claim | Source location | Result |
|---|---|---|---|
| 1 | DeCarlo, Metaxas, Stone 1998, SIGGRAPH 98 pp. 67-74, doi 10.1145/280814.280823 | Crossref | VERIFIED |
| 2 | Blanz, Vetter 1999, SIGGRAPH 99 pp. 187-194, doi 10.1145/311535.311556 | Crossref (pages 187-194 confirmed; earlier marked unverified) | VERIFIED |
| 3 | Kesterke et al. 2016, Biol Sex Differ 7:23, doi 10.1186/s13293-016-0076-8 | Crossref (volume 7, 2016-04-22); article number from Europe PMC | VERIFIED |
| 4 | Thesis Ch. 3 "first presented in" the SIGGRAPH paper | Thesis PDF p.43 | VERIFIED |
| 5 | 47 landmarks; FH frame; 132 measurements | PDF p.31, p.32 | VERIFIED |
| 6 | Five kinds of measurement, incl. the mentocervical angle | PDF p.32-33 | VERIFIED |
| 7 | No covariance data or fitted distributions published | PDF p.34 (the author wishes for them) | VERIFIED |
| 8 | sn-pg / li-pg example; consistent sets can still look wrong | PDF p.48 | VERIFIED |
| 9 | Branching (acyclic, at most one incoming edge), dummy edges for several bases | PDF p.49-50 | VERIFIED |
| 10 | µd = µs·µe; σd² = µs²σe² + µe²σs² + σe²σs²; independence and Gaussian assumed | PDF p.50 | VERIFIED |
| 11 | Weight σ/µ; the child's weight is always larger | PDF p.50 | VERIFIED |
| 12 | Modified Prim's algorithm; optimality argument | PDF p.50-51 | VERIFIED |
| 13 | Best plan for each possible initial measurement; fills in user-given or extreme sets | PDF p.49 | VERIFIED |
| 14 | Means differ about 1% (max 4.5%); SDs 5% lower to 20% higher | PDF p.58 | VERIFIED |
| 15 | Tube mesh with openings at mouth and neck | PDF p.53 | VERIFIED |
| 16 | Tangent at v horizontal; FH constraints (base constraints) | PDF p.53 | VERIFIED |
| 17 | 32 x 32 grid; 15 base constraints; 65 measurements; 120 proportions; North American Caucasian young adults | PDF p.57 | VERIFIED |
| 18 | Membrane term weighted α | PDF p.37 | VERIFIED |
| 19 | Prototype lets fairing ignore expected sharp regions (ears, nose) | PDF p.55 | FIX applied: a quoted phrase "smoothed away" was not in the source; replaced by a paraphrase |
| 20 | "what faces are like" given as a quote | PDF p.53 says the prototype "epitomizes the kind of shape faces have" | FIX applied: the quote marks were removed and the line paraphrased |
| 21 | Sparse LU computed once; about 3 minutes on a 175 MHz R10000; solves roughly linear time | PDF p.56, p.58 | VERIFIED |
| 22 | Step size by step-doubling; more stable than conjugate gradients; linearising gave little gain and less stability | PDF p.56-57 | VERIFIED |
| 23 | 50 iterations rough shape; up to 200 for millimetre accuracy; about 1 minute | PDF p.58 | VERIFIED |
| 24 | Dependent rows found by inspection; most locally confining constraints kept | PDF p.57 | VERIFIED |
| 25 | Automatic solutions left as open problems in earlier work | PDF p.38 | VERIFIED |
| 26 | Curvature not specified by the mentocervical angle | PDF p.51 | VERIFIED |
| 27 | Ears coarse because of few measurements | PDF p.59 | VERIFIED |
| 28 | ρ power weighting; ρ around 10; within one SD effectively ignored | PDF p.72 | VERIFIED |
| 29 | "child aging" only as an application | PDF p.118 | VERIFIED (capitalisation corrected to match the source) |
| 30 | Roughly 65 random numbers per face | Derived | Labelled (mine); counting from PDF p.49-50 and p.57 |
| 31 | Blanz: 200 young adults, 100 per sex, no makeup or facial hair, about 70,000 vertices | Blanz PDF p.2 | VERIFIED |
| 32 | Prior "regulates" likelihood | Blanz PDF p.1, p.3 | VERIFIED |
| 33 | Span at most m − 1 dimensions | Blanz PDF p.3 footnote 2, checked on the page image (the text layer drops the "− 1") | VERIFIED; the 199 is labelled (mine, arithmetic) |
| 34 | Attributes: gender, fullness, eyebrows, double chins, hooked or concave nose | Blanz PDF p.3 | VERIFIED |
| 35 | Caricatures by increasing distance from the mean | Blanz PDF p.4 | VERIFIED |
| 36 | Restricted to the database span; matching is ill-posed | Blanz PDF p.4, p.7 | VERIFIED |
| 37 | Bootstrapping; forehead and cheek flow sometimes spurious | Blanz PDF p.6 | VERIFIED |
| 38 | 100 + 100 PCs; 10^5 iterations; 50 minutes; 60° | Blanz PDF p.7 | VERIFIED on a fresh 160-dpi page image in this pass (the text layer drops these numbers) |
| 39 | Hair not capturable; 10-second scans; Mona Lisa lighting; Caucasian middle age; children and elderly future | Blanz PDF p.7 | VERIFIED |
| 40 | Internal disagreement: "young adults" (p.2) against "Caucasian faces of middle age" (p.7) | Blanz PDF p.2, p.7 | VERIFIED as both present |
| 41 | Kesterke: ICC above 0.95 for all landmarks | S4 Methods | VERIFIED |
| 42 | Male nasal growth nearly five times female between puberty and adolescence | S4 Discussion | VERIFIED |
| 43 | No landmarks on brow ridges or cheeks | S4 Discussion (limits) | VERIFIED |
| 44 | A quoted phrase "corresponding to traditional anthropometric measurements" | S4 wording is "29 traditional soft-tissue anthropometric measurements" | FIX applied: the quote was replaced by a paraphrase |
| 45 | DeCarlo paper: no clearly legal free copy | Author link 404 (tested 2026-11-28); OpenAlex closed | VERIFIED as of the access date; mirrors deliberately not used |
| 46 | Thesis licence "Copyright Douglas M. DeCarlo 1998" | Thesis PDF p.2 | VERIFIED |

Fixes: rows 19, 20 and 44 (three quotations that were not word-for-word were turned into paraphrases); row 29 (capitalisation); "(mine)" labels added to the product-variance remark and the 199 count.

WEAK: S1 (the SIGGRAPH paper itself) was not read, so anything only in the paper is missing; the thesis says its Chapter 3 is that paper's material.
