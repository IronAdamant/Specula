# SDF citation list for the sculpt-primitives design doc
Compiled 20 Nov 2026. Every URL below was fetched today with curl or WebFetch; status codes noted. Quotes are copied from the fetched source text. Anything I could not confirm is marked "not confirmed".

Local copies of everything fetched: /workspace/specula-research/raw/

---

## 1. Inigo Quilez, 2D and 3D distance functions

### 1a. 2D SDFs
- Citation: Inigo Quilez, "2D distance functions", iquilezles.org (no date shown on the page).
- URL: https://iquilezles.org/articles/distfunctions2d/ (200; note the site returns 404/500 to some non-browser user agents and to rapid repeat requests, a normal browser loads it)
- Title note: the HTML title tag is the generic site title. The page's breadcrumb (set from the page's own `gMenu` script) reads "articles :: 2D distance functions". The articles index links it as "2D SDFs".
- Relevant entries (all labelled "exact" on the page): "Segment - exact" (sdSegment, Shadertoy 3tdSDj), "Arc - exact" (sdArc, Shadertoy wl23RK), "Polygon - exact" (sdPolygon, Shadertoy wdBXRW). Also Ring, Pie, Horseshoe, Parabola Segment, Quadratic Bezier etc.
- What it claims: "most of the 3D primitives are grown as extrusions or revolutions of these 2D shapes"; the functions "use the minimum number of square roots and divisions possible". Caveat in the text: "all formulas and code are derived by myself (unless otherwise stated), so there might be errors or better ways to do things."

### 1b. 3D SDFs, including extrusion and revolution
- Citation: Inigo Quilez, "distance functions", iquilezles.org (no date shown on the page).
- URL: https://iquilezles.org/articles/distfunctions/ (200)
- Title note: breadcrumb reads "articles :: distance functions"; the index links it as "3D SDFs".
- Section to cite: "Creating more 3D SDFs, from 2D SDFs" (functions opRevolution and opExtrusion; demo https://www.shadertoy.com/view/4lyfzw, which returns 403 to curl).
- What it claims: "if the the 2D SDF we start with is an exact SDF, the resulting 3D volume is exact as well", and that 3D booleans of basic forms do "not produce an exact SDF" while revolution or extrusion "does produce the correct SDF". The same page labels Union, Subtraction and Intersection as "bound, bound, bound".
- Separate extrusion/revolution article: none found. The articles index (https://iquilezles.org/articles/, 200) has no separate entry; the topic lives in this section.

### Licence for items 1a, 1b and 6 (applies site-wide)
- Stated on the articles index page (https://iquilezles.org/articles/), not repeated on the individual article pages: "all technical code snippets you'll find are under the MIT license so you can easily reuse them, but the mathematical/shader art is protected and requires a license for use." The "MIT license" text links to https://opensource.org/licenses/MIT.
- Article text and figures: no licence stated. Footer on each page only says "inigo quilez - learning computer graphics since 1994"; no copyright line or text licence found. Treat article prose and images as all rights reserved by default (my inference, not a stated term).
- Shadertoy code headers saying MIT: not confirmed. Shadertoy pages return 403 to curl and render no source to WebFetch, so I could not read a header.

---

## 2. Baerentzen and Aanaes (2005), angle weighted pseudonormal
- Citation: J. Andreas Bærentzen and Henrik Aanæs. "Signed Distance Computation Using the Angle Weighted Pseudonormal." IEEE Transactions on Visualization and Computer Graphics, 11(3):243-253, May/June 2005. DOI: 10.1109/TVCG.2005.49. ISSN 1077-2626. (Crossref and DTU Orbit agree on volume, issue, pages, DOI. PubMed 15868824 gives "2005 May-Jun;11(3):243-53".)
- Links:
  - DOI: https://doi.org/10.1109/TVCG.2005.49 (resolves to https://ieeexplore.ieee.org/document/1407857/, which returns 500 to curl; publisher bot block, not a dead link)
  - Free copy, DTU Orbit: https://backend.orbit.dtu.dk/ws/files/3977815/B%C3%A6rentzen.pdf (200, PDF). Cover sheet says "Document Version: Publisher's PDF, also known as Version of record".
  - DTU Orbit record: https://orbit.dtu.dk/en/publications/signed-distance-computation-using-the-angle-weighted-pseudonormal/ (200)
- Licence/access: IEEE copyright (Crossref licence link points to IEEE's standard licence page). The DTU Orbit copy is free to read under DTU's "General rights" notice: "Copyright and moral rights ... are retained by the authors and/or other copyright owners". No open licence.
- What it claims (abstract): the angle weighted pseudonormal "allows us to discriminate between points that are inside and points that are outside a mesh, regardless of whether a mesh vertex, edge, or face is the closest feature", so the sign "can be computed as an integral part of the distance computation", with an algorithm "for computing the signed distance to a closed C0 mesh". The guarantee assumes a closed, orientable 2-manifold; the paper notes real meshes may "fail to be watertight" or "self-intersect".

---

## 3. Jacobson, Kavan, Sorkine-Hornung (2013), generalized winding numbers
- Citation: Alec Jacobson, Ladislav Kavan, Olga Sorkine-Hornung. "Robust Inside-Outside Segmentation using Generalized Winding Numbers." ACM Transactions on Graphics 32(4), Article 33, 12 pages (Proc. SIGGRAPH 2013). DOI: 10.1145/2461912.2461916.
  - Article number 33 and pages 33:1-33:12 come from the authors' lab BibTeX (https://igl.ethz.ch/publications/igl-bibtex.php, 200). Crossref lists only "1-12" and no article number; the author PDF does not print the article number. So "Article 33" is author-confirmed, not publisher-confirmed.
- Links:
  - DOI: https://doi.org/10.1145/2461912.2461916 (resolves to dl.acm.org, 403 to curl; publisher bot block)
  - Project page: https://igl.ethz.ch/projects/winding-number/ (200). Notes "SIGGRAPH Test-of-Time Award 2024".
  - Author PDF: https://igl.ethz.ch/projects/winding-number/robust-inside-outside-segmentation-using-generalized-winding-numbers-siggraph-2013-jacobson-et-al.pdf (200)
- Licence/access: ACM copyright (Crossref licence link is ACM's copyright policy). Author copy free to read; no open licence stated.
- What it claims (abstract): "By generalizing the winding number for arbitrary triangle meshes, we define a function that is a perfect segmentation for watertight input and is well-behaved otherwise." Requires only "reasonably consistent orientation of the input triangle mesh". Body: the function has a "±1 jump" across the surface and is "harmonic" away from it; 3D winding number defined as w(p) = Ω(p)/4π (solid angle).

---

## 4. Barill et al. (2018), fast winding numbers
- Citation: Gavin Barill, Neil G. Dickson, Ryan Schmidt, David I.W. Levin, Alec Jacobson. "Fast Winding Numbers for Soups and Clouds." ACM Transactions on Graphics 37(4), Article 43, 12 pages, August 2018 (SIGGRAPH 2018). DOI: 10.1145/3197517.3201337. (Article 43 and the DOI are printed on the author PDF; Crossref agrees on volume, issue, DOI.)
  - Name note: the PDF and Crossref say "Neil G. Dickson"; the current project page lists the second author as "Nia Dickson". Check which name the author prefers before publishing.
- Links:
  - DOI: https://doi.org/10.1145/3197517.3201337 (dl.acm.org, 403 to curl; publisher bot block)
  - Project page: https://www.dgp.toronto.edu/projects/fast-winding-numbers/ (200)
  - Author PDF: https://www.dgp.toronto.edu/projects/fast-winding-numbers/fast-winding-numbers-for-soups-and-clouds-siggraph-2018-barill-et-al.pdf (200)
- Licence/access: ACM copyright (PDF carries ACM's standard "Permission to make digital or hard copies..." notice). Author copy free to read. Code is on the project page (libigl igl::fast_winding_number); code licence not checked here.
- What it claims (abstract): "Generalized winding numbers provide a robust answer for triangle meshes, regardless of defects such as self-intersections, holes or degeneracies"; it extends them to point clouds and gives "a tree-based algorithm to reduce the asymptotic complexity ... while closely approximating the exact value" (Fig. 1: "up to 1000x speedup"). Lists "signing distances" as an application. It also says sign by angle weighted pseudonormals [Baerentzen and Aanaes 2005] "requires not just a closed, watertight mesh but also good mesh-quality for numerically trustworthy normal computation".

---

## 5. Hart (1996), sphere tracing
- Citation: John C. Hart. "Sphere tracing: a geometric method for the antialiased ray tracing of implicit surfaces." The Visual Computer 12(10):527-545, December 1996. DOI: 10.1007/s003710050084. (Crossref and Springer page agree.)
- Links:
  - DOI: https://doi.org/10.1007/s003710050084 (curl gets 500 from link.springer.com; WebFetch of https://link.springer.com/article/10.1007/s003710050084 loads: "Published: December 1996")
  - Free copy: https://graphics.stanford.edu/courses/cs348b-20-spring-content/uploads/hart.pdf (200). Caution: this is a ResearchGate download of the author's upload, paginated "5-76", "5-77" (course-notes style), not the journal layout. A 2004 reference in Dekkers et al. says "An earlier version appeared in the SIGGRAPH '93 course notes as WSU Technical Report EECS-93-015". So the free copy is probably the earlier version; that it matches the journal text is not confirmed. Quote definitions from the journal if you need exact wording.
- Licence/access: Springer copyright, paywalled. Free copy is a course upload; no open licence.
- What it claims (from the free copy; some math symbols were lost in PDF extraction and are restored here from context):
  - Definition 2: f is a "signed distance bound" of its implicit surface if and only if |f(x)| <= d(x, f^-1(0)). "If equality holds ... then f is a signed distance function."
  - Definition 3: f is Lipschitz over D if there is a positive finite λ with |f(x) - f(y)| <= λ||x - y||; "The Lipschitz constant, denoted Lip f, is the minimum λ satisfying (8)."
  - Theorem 1: "Let f be Lipschitz with Lipschitz bound λ >= Lip f. Then the function f/λ is a signed distance bound of its implicit surface."
  - Why not overstate: sphere tracing "marches along the ray toward its first intersection in steps guaranteed not to penetrate the implicit surface"; a looser Lipschitz bound "causes a poorer distance underestimate, which adversely affect[s] the efficiency".
  - Plain reading: the field may underestimate distance (slower) but must never exceed the true distance; a 1-Lipschitz field with the right zero set meets this.

---

## 6. Smooth minimum and its error bound

### 6a. Quilez, smooth minimum
- Citation: Inigo Quilez, "Smooth minimum for SDFs", iquilezles.org. Breadcrumb reads "articles :: smooth minimum - 2013". The page itself says nothing about revisions, but Barbier et al. (6b) cite it as "[Qui24b] Quilez I.: Bounding volumes for smooth sdf operators, 2024. Normalization, thickness and bounds." So the article was evidently updated around 2024. Cite as "2013, updated" or with an access date.
- URL: https://iquilezles.org/articles/smin/ (200)
- Licence: as in item 1 (code MIT per the articles index; text licence not stated).
- Does it state the bound? Yes, in the section "Normalization, thickness and bounds":
  - In the article's normalised form (the code multiplies k first, e.g. quadratic: `k *= 4.0; h = max(k-abs(a-b),0)/k; return min(a,b) - h*h*k*(1.0/4.0);`), "the value of the parameter k matches exactly the maximum inflation or thickening that the shapes a and b undergo", and "k is also the bounding box expansion required to perfectly bound the smooth-minimum". The maximum occurs at a = b.
  - For the older, unnormalised quadratic form (h = max(k-|a-b|,0)/k; min(a,b) - h*h*k/4) the same maximum works out to k/4. That is my arithmetic from the code (h = 1 at a = b); the page does not print "k/4" as a stated bound.
  - Lipschitz/conservative: section "Gradients of the DD and CD family": the gradient is a linear interpolation of the input gradients, so "because the length is shorter than 1 we know that they will never overestimate the true distance, and therefore we can use them safely for raymarching and collision detection". Section "Properties": "ALL the smooth-minimums in the CD Family are guaranteed to produce underestimates"; the Circular Geometrical smooth-min "violates this property". The summary table marks Quadratic, Cubic, Quartic, Circular, Exponential, Sigmoid and Root as Conservative = Yes, Circular Geometric = No. It also says underestimation regions for the DD/CD families "span to infinity, ie, non-local", and that CD smins are "not associative".
  - This is an author's web article, not peer reviewed, and its proofs are informal.

### 6b. Best peer-reviewed source found
- Citation: Wilhem Barbier, Mathieu Sanchez, Axel Paris, Élie Michel, Thibaud Lambert, Tamy Boubekeur, Mathias Paulin, Theo Thonat. "Lipschitz Pruning: Hierarchical Simplification of Primitive-Based SDFs." Computer Graphics Forum 44(2), e70057, 2025 (Eurographics 2025). DOI: 10.1111/cgf.70057.
- Links: DOI https://doi.org/10.1111/cgf.70057 (Wiley, 403 to curl); free PDF from Eurographics Digital Library https://diglib.eg.org/server/api/core/bitstreams/d7139fe4-fb28-4a06-8011-c813fa4d59b5/content (200, PDF; item page https://diglib.eg.org/items/2a25950b-725a-4ad9-aeb4-76cf4008b142).
- Licence: open access, CC BY 4.0. The PDF says "This is an open access article under the terms of the Creative Commons Attribution License"; Crossref licence is creativecommons.org/licenses/by/4.0.
- What it claims: defines UNION(a,b,k) = min(a,b) - φ(|a-b|,k) with quadratic blending φ(d,k) = (1/(4k)) max(k-d,0)^2 citing [DVOG04, Qui24b]; works only with operators "that are 1-Lipschitz, meaning that ||∇f|| <= 1 everywhere" [Har96]; says smooth CSG operators "require the bounding box to be extended by the blending radius [Qui24b]". It does not prove the smooth-min deviation bound itself; its maximum, φ(0,k) = k/4, follows from the formula but is not stated as a theorem in the text I searched.

### 6c. Earlier peer-reviewed Lipschitz blending (text not checked)
- Citation: Daniel Dekkers, Kees van Overveld, Rob Golsteijn. "Combining CSG modeling with soft blending using Lipschitz-based implicit surfaces." The Visual Computer 20(6):380-391, August 2004. DOI: 10.1007/s00371-002-0198-3. (Crossref and the Springer page agree.)
- Link: https://link.springer.com/article/10.1007/s00371-002-0198-3 (loads via WebFetch; curl gets 500). Paywalled; no free copy found.
- What the abstract says: "A class of various blending functions sharing some desirable properties like differentiability and intuitive blend control are given. The functions defining the CSG objects satisfy the Lipschitz condition". Whether it states an explicit deviation bound: not confirmed (only the abstract was readable).

### Bottom line for item 6
I found no peer-reviewed paper that states the polynomial smooth-min deviation bound (k, or k/4 unnormalised) as a result. The clearest real source for the bound and for the never-overestimates property is Quilez's smin article (sections "Normalization, thickness and bounds", "Gradients of the DD and CD family", "Properties"). Barbier et al. 2025 (CC BY) is the best peer-reviewed source that uses the quadratic smooth union inside a 1-Lipschitz framework. Dekkers et al. 2004 is the likely earlier peer-reviewed origin of Lipschitz soft blending, but I could not read its body.

---

## Not confirmed / caveats
1. MIT headers inside Quilez's Shadertoy shaders (Shadertoy blocks curl and renders no code to WebFetch).
2. A text/figure licence for iquilezles.org articles (none stated; only the code is MIT).
3. Publication dates for the distfunctions and distfunctions2d articles (none on the pages). The exact revision date of the smin article (breadcrumb says 2013; Barbier et al. cite a 2024 version).
4. Jacobson 2013 "Article 33" is from the authors' BibTeX only; Crossref has no article number.
5. Whether the free Hart PDF matches the 1996 journal text (it looks like the earlier course-notes/tech-report version).
6. Whether Dekkers et al. 2004 states an explicit blend error bound.
7. The second author of Barill et al. 2018 is listed as "Neil G. Dickson" in the paper and "Nia Dickson" on the current project page.
8. Publisher landing pages (IEEE, ACM, Springer, Wiley) gave 403/500 to curl. That is bot blocking, not dead links; the DOIs are registered in Crossref.

---

# Round 2 (20 Nov 2026)
Same rules as round 1. Raw copies in /workspace/specula-research/raw/. "Independent" means the source makes the claim in its own words, with its own definitions or derivation. "Cites" means it rests the claim on the original.

## R2.1 Smooth-min error bound and Lipschitz claims

Short answer: I still found no peer-reviewed paper that states the polynomial smooth min's maximum deviation from min (k/4 in the unnormalised form) as a result or theorem. I did find three peer-reviewed papers, all open access, that state the Lipschitz / never-overestimate side in their own text. Hart 1996 also gives Lipschitz bounds for two older blends (soft objects and the pseudonorm blend), but not for smooth min.

### A. Hubert-Brierre, Guerin, Peytavie, Galin (2025): blending operators are 1-Lipschitz
- Citation: Pierre Hubert-Brierre, Eric Guérin, Adrien Peytavie, Eric Galin. "Accelerating Signed Distance Functions." Computer Graphics Forum 44(7), 2025 (Pacific Graphics 2025). DOI: 10.1111/cgf.70258 (Crossref; Crossref gives no page or article number).
- Link: author PDF https://perso.liris.cnrs.fr/eric.galin/Articles/2025-lod.pdf (200, slow server; PDF metadata "Pacific Graphics 2025, CGF Vol 44, No 7").
- Licence/access: Wiley terms per Crossref (not CC). The author copy is free to read.
- Statement, Section 3 "Overview and notations": "Primitives are continuous 1-Lipschitz field functions, i.e., ∀p ∈ R3, ∥∇f(p)∥ ≤ 1 ... Internal nodes are operators such as Boolean and blending functions that combine the field functions of their sub-trees, as well as deformations nodes ... which are also 1-Lipschitz."
- Also Section 3: "smooth Boolean operators introduced in [DVOG04] require the function to be Lipschitz, or at least continuous." A conservative field is defined as "|f| ≤ |d|".
- Status: independent statement, but asserted without proof. No deviation bound.

### B. Barbier et al. (2025), already in round 1: restated more precisely
- Section 3.2: "We focus specifically on exact and conservative SDFs where the distance function f preserves the 1-Lipschitz property (∥∇f∥ ≤ 1 everywhere [Har96])." Section 3.3: quadratic kernel φ(d,k) = (1/(4k)) max(k-d,0)^2 [DVOG04, Qui24b]. Section 6.1: "Our method is restricted to primitives and operators that are 1-Lipschitz". Section 5.2: "smooth CSG operators require the bounding box to be extended by the blending radius [Qui24b]".
- Status: applies the smooth operators inside a 1-Lipschitz framework and cites Quilez for the bounding expansion. Not an independent proof. Appendix A ("A condition for Lipschitz pruning") proves the pruning test, not a bound on the blend.
- The project slides (https://wbrbr.org/publications/LipschitzPruning/slides/, 200) reportedly say "Smooth boolean operations maintain the 1-Lipschitz property". Not confirmed: the page renders that text with JavaScript and my fetch did not contain it; it showed up only in a search snippet.

### C. Hart (1996), Appendix D: Lipschitz bounds for older blends (independent, peer-reviewed; read from the free manuscript copy)
- Theorem 5 (Appendix D.1, "Soft Metablobbies"): for the soft-object blend f(x) = T - Σ C_Ri(||x - p_i||) built from a piecewise cubic, Hart bounds the distance to the blend B by f(x) together with the radii R_i, eq. (38). The exact symbols were lost in PDF extraction, so check eq. (38) in the PDF before quoting it. The proof uses "Lip C(r) = |C'(R/2)| = 3/(2R)" and "The Lipschitz constant of a sum is bounded by the sum of the Lipschitz constants".
- Appendix D.2, "Superelliptic Blends": the pseudonorm blend of Rockwood and Owen "returns the p-norm distance to the blended union of implicit surfaces of signed distance functions"; Table 1 lists "pseudonorm blend" under signed distance bound.
- Status: a real Lipschitz statement for blends, but for metaball and pseudonorm blends, not the polynomial smin.

### D. Candidates checked that do NOT state a smooth-min bound
- Galin, Guérin, Paris, Peytavie. "Segment Tracing Using Local Lipschitz Bounds." Computer Graphics Forum 39(2):545-554, 2020. DOI 10.1111/cgf.13951. Author copy: https://hal.science/hal-02507361/file/lipschitz-author-version.pdf (200; HAL licence "HAL Authorization"). It uses the field convention (blend f = fA + fB, union = max) and states (Sec. 4.2) "λ of a blending node is defined as λ = λA + λB". That is a Lipschitz rule for additive blends of compact-support fields, not smin of distances.
- Sharp and Jacobson. "Spelunking the Deep: Guaranteed Queries on General Neural Implicit Surfaces via Range Analysis." SIGGRAPH 2022; arXiv:2202.02444 (200). It does range analysis for neural fields and has no smooth-min bound. It notes global Lipschitz bounds on existing networks "are extremely" loose, which is not relevant here.
- Reiner, Mückl, Dachsbacher. "Interactive modeling of implicit surfaces using a direct visualization approach with signed distance functions." Computers & Graphics 35(3):596-603, 2011. DOI 10.1016/j.cag.2011.03.010. Free PDF https://cg.ivd.kit.edu/publications/p2011/IntModelingSDF/IntModelingSDF.pdf (per search; I did not download it). From the search extract only: blending is a linear mix "(1 − α)·φA + α·φB", and non-uniform scale needs "A final multiplication by min(sx, sy, sz) ensures that the gradient magnitude does not exceed 1". No smin bound seen.
- Dekkers, van Overveld, Golsteijn (2004): no author copy found (Springer paywall; the TU/e research portal record has no file). Full text not confirmed. Two 2025 papers (A and B) credit it as the source of smooth Boolean operators on Lipschitz fields.
- Not checked in full text (no free copy reached, or clearly off-topic from the abstracts): Barthe/Dodgson et al. gradient/soft CSG operators, Pasko and Savchenko R-function blending, Bernhardt et al., Wyvill et al. "Extending the CSG tree", Gourmel et al. gradient-based blend (these work on bounded potential fields, not distances), Keinert et al. 2014, Seyb et al. 2019, Aydinlilar and Zanni 2023. Treat all as not confirmed.

### E. Independent restatement of the conservative-distance condition (supports Hart)
- Zoë Marschner, Silvia Sellán, Hsueh-Ti Derek Liu, Alec Jacobson. "Constructive Solid Geometry on Neural Signed Distance Fields." SIGGRAPH Asia 2023 Conference Papers (SA '23), 12 pages. DOI: 10.1145/3610548.3618170.
- Link: https://zoemarschner.com/papers/csg_on_neural_sdfs.pdf (200).
- Licence: the PDF says "This work is licensed under a Creative Commons Attribution International 4.0 License." (Crossref's licence field points only to ACM's copyright policy page.)
- Statement, Section 3.1 "A Categorization of Implicit Functions": "Conservative SDF ... one often enforces the conservative distance property; namely, |fθ(x)| ≤ |d(x, Σ)|". It also says the eikonal property "is merely a necessary, but not sufficient, condition" for a true SDF.
- Status: independent wording and taxonomy; cites Hart 1996 for sphere tracing.

### Recommended wording for the design doc
- Bound: cite Quilez smin article, section "Normalization, thickness and bounds" (web source), plus Barbier et al. 2025 Sec. 5.2 as peer-reviewed use of "bounding box ... extended by the blending radius [Qui24b]".
- Lipschitz: cite Quilez "Gradients of the DD and CD family" for the argument, and Hubert-Brierre et al. 2025 Sec. 3 and Barbier et al. 2025 Sec. 3.2 / 6.1 as peer-reviewed statements that blend operators are treated as 1-Lipschitz.
- No source found proves the k/4 figure; it is a one-line derivation from the formula (h = 1 at a = b).

## R2.2 Hart: what the free copy is
- Stanford copy (https://graphics.stanford.edu/courses/cs348b-20-spring-content/uploads/hart.pdf): page 1 is a ResearchGate cover ("Article in The Visual Computer · June 1995", "uploaded by John C. Hart on 28 July 2015"). The paper itself is footnoted "Manuscript, July 1994. Recommended for publication: The Visual Computer." and paginated 5-70 to 5-119, which looks like it was reprinted in a course-notes volume (section 5). Which course notes: not confirmed. The author address is Washington State University. This corrects round 1: it is a July 1994 manuscript, not the 1993 tech report.
- A second copy of the same manuscript is listed at https://users.soe.ucsc.edu/~pang/160/s15/projects/darpeter/literature/rayCastingImplicitSurfaces.pdf. The search extract shows the same "Manuscript, July 1994" footnote and 5-70 page; curl got a TLS error, so I did not download it myself.
- CiteSeerX record 10.1.1.48.3825 indexes this manuscript (dated 1994).
- Journal version wording vs manuscript: not confirmed. The Springer full text is paywalled (the PDF URL returns an HTML page). Evidence it was edited: the journal abstract (Illinois Experts, https://experts.illinois.edu/en/publications/sphere-tracing-a-geometric-method-for-the-antialiased-ray-tracing/, 200, 527-545, 12(10)) differs from the manuscript abstract ("Sphere tracing is a new technique for rendering implicit surfaces that uses geometric distance" vs the manuscript's "... using geometric distance. Distance-based models are common ..."). So whether Definitions 2, 3 and Theorem 1 keep the same numbering and wording in the journal is not confirmed.
- Do not use: Semantic Scholar and OpenAlex list an "open access" Hart PDF at univ-paris8.hal.science/hal-04421881. The HAL API shows that record is a different 2019 French paper on shadercoding (Fouquart and Chen) with mismatched metadata.
- Safe citation: cite the journal (12(10):527-545, DOI 10.1007/s003710050084) and, if quoting, say "quoted from the 1994 author manuscript". Independent restatements of the same condition are R2.1 E (Marschner et al. 2023), and Barbier et al. 2025 Sec. 1 ("The norm of its gradient is bounded by 1 ... These properties imply that |f(p)| is a lower bound on the exact distance", cites Hart).
- Textbooks (Real-Time Rendering, PBRT, Bloomenthal's "Introduction to Implicit Surfaces"): not checked, no free text reached. Not confirmed.

## R2.3 Licences and names

### Quilez article text licence
- Checked https://iquilezles.org/ (200), /live/ (200), /articles/ (200); /contact/ and /about/ return 404. The only licence statement is the articles index sentence quoted in round 1 (code MIT; "mathematical/shader art is protected and requires a license for use"). No licence for the article prose was found. Wayback snapshots exist (e.g. 20220601194750, 20240521090748 for /articles/), but the archive refused my downloads, so the history of the statement is not confirmed. His GitHub and Shadertoy profile: not checked (Shadertoy blocks curl with a Cloudflare challenge; the box browser was in use by another agent).
- Conclusion: text and figures have no stated licence. Cite and link them; do not copy the prose or images.

### Shadertoy MIT header wording
- Not confirmed from Shadertoy itself (the API needs a key, and the page loads its code with JavaScript behind Cloudflare).
- Secondary evidence: a third-party Shadertoy corpus on GitHub holds a copy of Quilez shader clXBW4 ("Distance to a regular pentagon", which references iquilezles.org/articles/distfunctions2d). Its header reads: "// The MIT License" / "// Copyright © 2019 Inigo Quilez" / "// Permission is hereby granted, free of charge, ..." (standard MIT text). Source: https://github.com/IfkumRfnl/holytoy/blob/d80aeeb01533970978921f651a5da3b08f8b5c64/corpus/shadertoy/v2/shaders/clXBW4/image.glsl. This is a repost, not an authoritative source; the year varies per shader. The specific shaders linked from the articles (wdBXRW, 3tdSDj, wl23RK, 4lyfzw) were not in that corpus.

### Barill et al. 2018 second author
- As published: "NEIL G. DICKSON, Side Effects Software Inc." (paper PDF). Crossref, OpenAlex and Semantic Scholar also say "Neil G. Dickson".
- Current: the authors' project page lists "Nia Dickson" (SideFX) and its BibTeX uses "Nia Dickson". An NVIDIA author page shows a "Nia Dickson" (no SideFX link stated there). ACM DL and DBLP author records: not confirmed (ACM timed out; DBLP serves a bot challenge).
- Recommendation: follow the authors' own current BibTeX, "Nia Dickson", optionally with "(published as Neil G. Dickson)" so a reader can match the PDF. The DOI identifies the paper either way.

### Jacobson et al. 2013 article number
- "Article 33" / "33:1-33:12" confirmed in two places: the authors' lab BibTeX, and the reference list of Barill et al. 2018 ("32, 4 (2013), 33:1–33:12"). Both are from the same research group, so neither is independent.
- A web search summary also said "Article 33" (from a references.bib on GitHub). Weak.
- Publisher confirmation: not confirmed. ACM DL timed out; Crossref, OpenAlex and Semantic Scholar list only pages "1-12"; DBLP is behind a bot challenge. Semantic Scholar gives the DBLP key journals/tog/JacobsonKS13.

## Still not confirmed after round 2
1. A peer-reviewed statement of the smooth-min maximum deviation (k or k/4).
2. A proof, as opposed to an assertion, of 1-Lipschitz smooth min in a peer-reviewed paper.
3. The Dekkers et al. 2004 full text.
4. Whether Hart's journal text matches the 1994 manuscript's Definitions 2, 3 and Theorem 1.
5. The course-notes volume the Hart manuscript was printed in.
6. The Quilez text licence, and the MIT header seen directly on Shadertoy.
7. Jacobson 2013 "Article 33" from ACM or DBLP directly.
8. The Dickson name on the ACM DL record.
9. Textbook coverage (RTR, PBRT, Bloomenthal).

## Round 3 (browser check, 20 Nov 2026)

- Quilez MIT header, read directly on Shadertoy: iq's own "Pentagon - distance 2D", https://www.shadertoy.com/view/llVyWW, "Created by iq in 2018-09-07". Code begins: "// The MIT License" / "// Copyright © 2018 Inigo Quilez" / "// Permission is hereby granted, free of charge, to any person obtaining a copy of this software ...". Note: clXBW4 (cited in Round 2) is a fork by beans_please, not iq's original; use llVyWW instead.
- Jacobson, Kavan, Sorkine-Hornung 2013: CONFIRMED. DBLP: "ACM Trans. Graph. 32(4): 33:1-33:12 (2013)". ACM DL: "Volume 32, Issue 4 / Article No.: 33, Pages 1 - 12", published 21 July 2013.
- Barill et al. 2018: CONFIRMED article 43, pages 1-12 (ACM DL), published 30 July 2018. Both ACM DL and DBLP list the second author as "Neil G. Dickson". Third author: ACM "Ryan Schmidt", DBLP "Ryan M. Schmidt". Suggested citation: authors as published (Neil G. Dickson); the authors' own page now uses "Nia Dickson", so "Nia Dickson (published as Neil G. Dickson)" remains an option if preferred.
- Still not confirmed: licence for Quilez article text; Dekkers 2004 full text; Hart journal wording vs 1994 manuscript; peer-reviewed statement of the smooth-min deviation bound.

## Round 4 (browser check, 20 Nov 2026)

- Hart 1996: Springer page (https://link.springer.com/article/10.1007/s003710050084) shows only metadata, "Published: December 1996, Volume 12, pages 527-545"; no abstract or preview; PDF is paywalled (EUR 39.95). Journal wording therefore NOT confirmed. A second free copy, https://www.ioccc.org/2011/zucker/sphere-tracing.pdf (ResearchGate cover "June 1995 ... Source: CiteSeer", footers 5-76/5-77, a course-notes or preprint version), carries the same Definitions 2 and 3 and Theorem 1 wording and numbering as the 1994 manuscript (symbols partly garbled in text extraction, check in the PDF). Two independent free copies agree with each other; neither is the journal pagination. Other free copies listed by Google Scholar: https://3dvar.com/Hart1996Sphere.pdf (not opened), academia.edu copy.
- Dekkers, van Overveld, Golsteijn: journal abstract (Springer, Visual Computer 20:380-391, 1 Aug 2004) says "The functions defining the CSG objects satisfy the Lipschitz condition". Free earlier version: TU Eindhoven technical report dated 29 Oct 1996, http://cthrough.nl/ComputerGraphics/CSGLipschitz.pdf (not the journal text, not peer-reviewed). Section 2.1 aims for Lipschitz constant 1 and gives a sufficient condition on the blend term: -1 <= fb'(x, n) <= 0 for x >= 0; Appendix A PROVES (by induction from exact skeletal distances) that the blended union max(a,c) + fb(|a-c|, n), and intersection/difference via negation, stay Lipschitz. This is the closest thing found to a proof that a smooth blend stays within the "never overstates" rule (here for a max-based union with a blend term). It does not state a numeric deviation bound; the deviation is fb(0, n), which is not stated explicitly in the extracted text. Formulas were partly garbled in extraction; check in the PDF.
- Quilez article text licence: final answer, none exists. Live site and Wayback snapshots (2019 distfunctions page, 2021 old index) show only the footer "inigo quilez - learning computer graphics since 1994". The only licence statement anywhere is on https://iquilezles.org/articles/ : code snippets are MIT; "the mathematical/shader art is protected and requires a license for use." Cite article text as copyright of the author, no licence.
- Peer-reviewed numeric smooth-min deviation bound: still none found after four passes. Best sources: Quilez smin article (bound stated), Dekkers 1996 TR Appendix A (Lipschitz proof), Barbier et al. 2025 and Hubert-Brierre et al. 2025 (peer-reviewed use of the 1-Lipschitz rule).

---

# Round 5: modern free replacements (20 Nov 2026)

Goal: modern (about 2010 onwards, preferably 2018+), free and legal sources that cover the same ground as the paywalled Hart 1996 journal text and Dekkers, van Overveld and Golsteijn 2004, plus any independent free derivation of a smooth-min deviation bound. Every kept PDF or page below was opened today and the quotes are copied from that text. Shadow libraries were not used. Local copies: `/workspace/specula-research/raw/round5/` (Galin is also at `/workspace/specula-research/raw/st.pdf`).

Short answer: there are strong modern free replacements for Hart's Lipschitz fact, distance bounds under min/max CSG, and sphere-tracing convergence. There is a modern free write-up of Dekkers-style polynomial smooth CSG (cubic form, parameter k in the formula as k/6). There is still no modern peer-reviewed paper that states, as a result, that polynomial smin deviates from min by at most k/4, k/6, or log(2)*k. The numeric bounds remain clearly stated only in Quilez's article (already in item 6a). An independent 2016 blog derives the quadratic smin formula itself (the subtractive term is k*h*(1-h), which peaks at k/4) but does not name that peak as a bound.

## Kept sources

### R5.1 Bálint, Valasek and Gergó (2019). Operations on Signed Distance Functions. Peer-reviewed.

- Citation: Csaba Bálint, Gábor Valasek, and Lajos Gergó. "Operations on Signed Distance Functions." Acta Cybernetica 24(1):17-28, 2019. DOI: 10.14232/actacyb.24.1.2019.3.
- Free links (both 200, PDF, checked today): official journal PDF https://www.cyber.bibl.u-szeged.hu/index.php/actcybern/article/download/4004/3973 (item page https://www.cyber.bibl.u-szeged.hu/index.php/actcybern/article/view/4004); author copy https://people.inf.elte.hu/csabix/publications/articles/acta-2018-sdf-operations.pdf. The DOI resolver itself returned 500 today; use the www journal URL.
- Licence/access: free journal PDF from Acta Cybernetica (University of Szeged). No Creative Commons line appears in the PDF.
- What it claims: an independent, self-contained proof that every signed distance function is Lipschitz with smallest constant 1, plus a theorem on when f-c remains an SDF after an offset. Corollary 1: "Every signed distance function is Lipschitz continuous and their smallest Lipschitz constant is 1. Formally: forall f : R3 -> R SDF : inf Lip f = min Lip f = 1." The proof (from Lemma 2, the triangle inequality for distances to a closed set) does not rest on quoting Hart for this fact; Hart is cited for sphere tracing.
- Replaces: Hart 1996 on the Lipschitz / unit-slope property of SDFs (the foundation of conservative sphere-tracing steps). Does not treat smooth blends or an smin deviation bound.

### R5.2 Bálint, Valasek and Gergó (2023). Operations on Signed Distance Function Estimates. Peer-reviewed.

- Citation: Csaba Bálint, Gábor Valasek, and Lajos Gergó. "Operations on Signed Distance Function Estimates." Computer-Aided Design and Applications 20(6):1154-1174, 2023. DOI: 10.14733/cadaps.2023.1154-1174.
- Free link (200, PDF, checked today): https://www.cad-journal.net/files/vol_20/CAD_20(6)_2023_1154-1174.pdf
- Licence/access: free to read on the journal site. The PDF footer says "© 2023 CAD Solutions, LLC, http://www.cad-journal.net". Not CC. The DOI landing page returned 500 today; the direct PDF works.
- What it claims: min/max CSG of exact SDFs is not in general an exact SDF ("the error can be arbitrary large under certain conditions"). They work with signed distance function estimates (SDFEs), a subset of Hart's signed distance lower bounds, and prove closure and a precision bound. Theorem 6 (Set operations): if f and g are SDFEs, then union h=min(f,g), intersection h=max(f,g), and difference h=max(f,-g) "produce an SDFE" (under a boundedness condition) with a stated precision involving the set-contact smoothness modulus; "The theorem directly implies sphere tracing convergence on the resulting representation for the union, intersection, and difference operations." They also write that Hart "proved that signed distance lower bounds are closed under minimum".
- Replaces: Hart 1996 on distance bounds under Boolean operations and on sphere-tracing convergence for CSG trees. This is the strongest modern free treatment of that ground. Hard min/max only; no smooth-min deviation bound.

### R5.3 Galin, Guérin, Paris and Peytavie (2020). Segment Tracing Using Local Lipschitz Bounds. Peer-reviewed.

- Citation: Éric Galin, Éric Guérin, Axel Paris, and Adrien Peytavie. "Segment Tracing Using Local Lipschitz Bounds." Computer Graphics Forum 39(2):545-554, 2020. DOI: 10.1111/cgf.13951.
- Free link: HAL author version https://hal.science/hal-02507361v1/file/lipschitz-author-version.pdf (landing https://hal.science/hal-02507361). This exact file was successfully fetched earlier today (local copy `/workspace/specula-research/raw/st.pdf`, PDF metadata "HAL", 11 pages). A later unauthenticated fetch hit a HAL bot-check page, so if the PDF URL challenges you, use the HAL record in a browser, or the project page https://perso.liris.cnrs.fr/aparis/public_html/projects/galin2020_Segment.html (links "Preprint" to HAL and "PDF" to the EG Digital Library item https://diglib.eg.org/items/e18f85b8-1d88-4e40-9799-85babda84cbe).
- Licence/access: author version in HAL; the HAL record marks the file licence as "HAL Authorization". Not CC. The journal version is Wiley/EG.
- What it claims: a segment-local Lipschitz bound is used to enlarge the safe sphere-tracing step. Section 4 derives Lipschitz bounds for primitives, Boolean and blending operators, affine maps and deformations. For their field-function convention, "the global Lipschitz bound λ of a blending node is defined as λ = λA + λB, and the bounds for Boolean operators are defined as: λ = max(λA, λB)". Union/intersection are max/min of fields; blending is the additive field f = fA + fB, not polynomial smin of distances. Sphere tracing itself is taken from Hart [Har96].
- Replaces: Hart 1996 on Lipschitz bounds under operations and on sphere-tracing practice (a modern, tighter, local version of the same conservative-step idea). Not a replacement for Dekkers or for an smin deviation bound. Already noted in Round 2 as not stating a smooth-min bound.

### R5.4 Bálint and Valasek (2018). Accelerating Sphere Tracing. Peer-reviewed.

- Citation: Csaba Bálint and Gábor Valasek. "Accelerating Sphere Tracing." In EG 2018 - Short Papers (Eurographics 2018). The Eurographics Association, 2018. DOI: 10.2312/egs.20181037.
- Free link (200, PDF, checked today): https://people.inf.elte.hu/csabix/publications/articles/eurographics-2018-shortpaper.pdf (author page). EG item: https://diglib.eg.org/items/ce5ffffe-d78a-4d90-8ace-5184cc0f336c.
- Licence/access: "c 2018 The Author(s) / Eurographics Proceedings c 2018 The Eurographics Association." Author-hosted copy, free to read. Not CC.
- What it claims: a sphere-tracing variant that takes larger steps near planar surfaces, compared with "classical (by Hart) and relaxed (Keinert et al.) sphere tracing". Also a multi-resolution strategy. It assumes the usual Lip f <= 1 setting rather than re-proving it.
- Replaces: Hart 1996 on the sphere-tracing algorithm (a modern peer-reviewed acceleration and comparison). Not a Lipschitz proof, and not about smooth blends.

### R5.5 Gillespie, Yang, Botsch and Crane (2024). Ray Tracing Harmonic Functions. Peer-reviewed.

- Citation: Mark Gillespie, Denise Yang, Mario Botsch, and Keenan Crane. "Ray Tracing Harmonic Functions." ACM Transactions on Graphics 43(4), Article 99, July 2024, 18 pages. DOI: 10.1145/3658201.
- Free link (200, PDF, checked today): https://markjgillespie.com/Research/harnack-tracing/HarnackTracing.pdf (project page https://markjgillespie.com/Research/harnack-tracing/).
- Licence/access: author copy. The PDF says "© 2024 Copyright held by the owner/author(s)." No CC line found in the extracted text. Free to read on the author's site.
- What it claims: a clear modern restatement of Hart's conservative step, then a different tracer for harmonic functions. Opening of Section 1: "Our method is inspired by the sphere tracing algorithm of Hart [1996], which ray traces level sets of a function f with a known Lipschitz bound, i.e., a value C > 0 such that, for all points x, y we have |f(x) - f(y)| <= C ||x - y||. At any time t, one can hence be certain that no point of the surface S is contained within a ball of radius R = (f(r(t)) - f*)/C, providing a conservative step size for ray tracing. An important special case are signed distance functions (SDFs), with Lipschitz constant C = 1."
- Replaces: Hart 1996 as a citable modern, free, peer-reviewed write-up of the Lipschitz step and the C=1 SDF case. It is a restatement, not an independent proof (that proof is in R5.1). It also offers Harnack tracing as an alternative when the function is harmonic rather than Lipschitz. No smin bound.

### R5.6 Zanni (2023 arXiv; journal 2025). Synchronized-tracing of implicit surfaces. Peer-reviewed journal; this pass read the arXiv text.

- Citation of the version opened: Cédric Zanni. "Synchronized-tracing of implicit surfaces." arXiv:2304.09673v1 [cs.GR], 19 April 2023. https://arxiv.org/abs/2304.09673
- Later journal (not opened this pass: the HAL journal PDF was blocked by a bot check): Cédric Zanni. "Synchronized Tracing of Primitive-based Implicit Volumes." ACM Transactions on Graphics 44(1), Article 6, 2025. DOI: 10.1145/3702227. Treat quotes as from the 2023 arXiv, which may differ from the journal wording.
- Free link (200, PDF, checked today): https://arxiv.org/pdf/2304.09673
- Licence/access: arXiv preprint. Standard arXiv reuse; no extra CC line was found on the first pages of v1.
- What it claims: a modern write-up of "smooth polynomial bounded operator[s] [Dekkers et al. 2004]" used with sphere tracing. Equations (4)-(5) give the C^2 smooth union: g_union(f0, f1, k) = csg_union(f0, f1) - d(f0, f1, k), with d(f0, f1, k) = (k/6) * max(1 - |f0-f1|/k, 0)^3. Smooth intersection and difference add the corresponding d term (eqs. 6-7). When f0 = f1, this formula gives d = k/6, but the paper does not state that peak as a deviation bound or theorem. For tracing they do not claim the compact operators stay 1-Lipschitz: "In practice, we set [the Lipschitz bound] to 1.45 to accommodate for field compression associated with our compact operators."
- Replaces: Dekkers 2004 as a modern, free, detailed source for polynomial smooth CSG (union, intersection, difference) on approximate SDFs, with an explicit cubic kernel. It is not a proof that the blend stays 1-Lipschitz, and it is not a stated numeric deviation bound.

### R5.7 Santos (2016). Smooth Minimum (technical blog). Not peer-reviewed.

- Citation: Vinicius Santos. "Smooth Minimum." Personal blog, updated 7 July 2016.
- Free link (200, HTML, checked today): https://vgracianos.github.io/blog/smin/
- Licence/access: no licence stated on the page. Footer "© 2022 Vinicius Santos." Treat the prose as all rights reserved; fair quotation for a citation list is fine.
- What it claims: an independent derivation of Quilez's quadratic polynomial smin, written because "I have used the below code in some of my shaders but never had the time to fully comprehend it, so I decided to derive it myself." Starting from a C^1-matching requirement at the edges of the blend band, he arrives at f(a,b,k) = mix(a, b, h) - k*h*(1-h), with h = 1/2 + (a-b)/(2k) inside |a-b| < k. He does not write "the maximum deviation is k/4". That peak follows from his own formula: at a = b, h = 1/2, the subtracted term is k*(1/2)*(1/2) = k/4.
- Replaces: nothing in the peer-reviewed sense. It is a free, independent derivation of the quadratic smin formula (the operator used in the design doc), useful alongside Quilez item 6a. It does not prove Lipschitz / "never overstates".

### R5.8 Seyb, Jacobson, Nowrouzezahrai and Jarosz (2019). Non-linear sphere tracing for rendering deformed signed distance fields. Peer-reviewed.

- Citation: Dario Seyb, Alec Jacobson, Derek Nowrouzezahrai, and Wojciech Jarosz. "Non-linear sphere tracing for rendering deformed signed distance fields." ACM Transactions on Graphics 38(6), Article 229, November 2019, 12 pages. DOI: 10.1145/3355089.3356502.
- Free link (200, PDF, checked today): https://cim.mcgill.ca/~derek/files/publication-files/seyb19nonlinear.pdf (McGill author copy). The Dartmouth copy https://cs.dartmouth.edu/~wjarosz/publications/seyb19nonlinear.pdf returned 500 today; the project page is https://cs.dartmouth.edu/~wjarosz/publications/seyb19nonlinear.html.
- Licence/access: author copy. The PDF says "© 2019 Copyright held by the owner/author(s). Publication rights licensed to ACM." Not CC.
- What it claims: SDF "metric properties also admit efficient surface rendering with sphere tracing"; CSG and "smooth blending" are listed as modelling strengths. The new work is conservative walking along a curved ray in undeformed space so deformed SDFs can still be traced without an inverse deformation. It uses the SDF/Lipschitz metric rather than proving bounds for blends.
- Replaces: Hart 1996 only as a modern extension of sphere tracing (to forward deformations). Not a Lipschitz proof and not a Dekkers or smin-bound replacement.

## Checked and dropped, or already in earlier rounds

- Keinert, Schäfer, Korndörfer, Ganse and Stamminger (2014), "Enhanced Sphere Tracing," STAG 2014, DOI 10.2312/stag.20141233. The EG Digital Library item exists, but every direct PDF URL tried today (EG bitstream, open.fau.de, erleuchtet.org) returned HTML or an empty/500 body. Dropped: could not open a legal copy this pass.
- Aydinlilar and Zanni (2023), "Forward inclusion functions for ray-tracing implicit surfaces," Computers & Graphics 114:190-200, DOI 10.1016/j.cag.2023.05.026. The HAL preprint URL returned a bot-check page. Dropped: could not open.
- Gourmel, Barthe, Cani, Wyvill, Bernhardt, Paulin and Grasberger (2013), "A Gradient-Based Implicit Blend," ACM TOG. Author PDF opened (https://www.cs.uvic.ca/~blob/publications/TOG_gradient-blend-final.pdf, 200). It is about C-infinity composition of functionally-based implicit fields using gradients, not Lipschitz SDFs or smin of distances. Dropped as a Hart/Dekkers replacement.
- Bernhardt, Barthe, Cani and Wyvill (2010), "Implicit Blending Revisited," Computer Graphics Forum 29(2). Author PDF opened (https://www.cs.uvic.ca/~blob/publications/ibr.pdf, 200). Localised F-rep blending; no Lipschitz, SDF, or sphere-tracing claims in the extracted text. Dropped as a replacement.
- Canezin, Guennebaud and Barthe (2013), "Adequate Inner Bound for Geometric Modeling with Compact Field Functions." A PDF was fetched from IRIT but Poppler could not read a trailer/xref table, so the text could not be verified. Dropped.
- Sharp and Jacobson (2022), "Spelunking the Deep," SIGGRAPH 2022; arXiv:2202.02444 (200, PDF). Already checked in Round 2: range analysis for general (often neural) implicits when a useful Lipschitz constant is not known. Mentions sphere tracing but has no smin bound and is not a replacement for Hart's SDF Lipschitz theory.
- Reiner, Mückl and Dachsbacher (2011), "Interactive modeling of implicit surfaces using a direct visualization approach with signed distance functions," Computers & Graphics 35(3):596-603. Author PDF opened (https://cg.ivd.kit.edu/publications/p2011/IntModelingSDF/IntModelingSDF.pdf, 200). Useful background on interactive SDF modelling and sphere tracing, including the observation that |grad phi| > 1 is unsafe for sphere tracing while 0 < |grad phi| < 1 only slows it down. Weaker than R5.1-R5.4 as a Hart replacement; no smin bound. Not kept in the short list.
- Pei (2026), "Witnessed Boolean Contracts: Sound Local Distance Guarantees for Approximate Implicit CSG," arXiv:2610.39650v1, 29 Oct 2026 (PDF opened). A new preprint on per-query certificates for approximate CSG. It is not a replacement for Hart or Dekkers, and it does not state an smin deviation bound.
- "Coleman Fundamentals": not identified as a distinct free source. Left out.
- Already in earlier rounds and still the right peer-reviewed support for "never overstates" / 1-Lipschitz modelling practice (but not a numeric smin bound): Marschner et al. 2023 (CC BY), Barbier et al. 2025 (CC BY), Hubert-Brierre et al. 2025 (author copy). Quilez's smin article remains the clearest statement of the numeric deviation bounds and of the conservative (underestimate) property.

## Bottom line for Claude's design doc

- For Lipschitz SDFs, conservative steps, and CSG min/max that remain a distance bound with sphere-tracing convergence: cite R5.1 and R5.2 as modern free peer-reviewed sources that prove the claims themselves. Use R5.3 and R5.4 for local Lipschitz bounds under operators and for a modern tracer. Use R5.5 if a short, citable restatement of the C=1 conservative step is enough.
- For smooth/soft polynomial CSG: cite R5.6 (Zanni, cubic kernel, explicit k/6 in the formula) together with the free Dekkers 1996 technical report already in Round 4 (the Lipschitz proof for a blend term) and Quilez item 6a (bounds and "never overestimates"). R5.7 is an independent non-peer-reviewed derivation of the quadratic formula.
- Smooth-min deviation bound in a modern peer-reviewed source: still not found as a stated result or theorem. Closest peer-reviewed formulas: Zanni eq. (5) (max of d is k/6 at f0=f1, unstated) and Barbier et al. 2025 (quadratic kernel phi(d,k)=(1/(4k)) max(k-d,0)^2, so phi(0,k)=k/4, also unstated as a bound). The sources that actually state the bound remain Quilez (item 6a).

# Round 5 second check (20 Nov 2026)

Independent check of the Round 5 kept sources. I did not use the earlier local copies in `/workspace/specula-research/raw/round5/`. Every stated free link was fetched again today. Bibliographic metadata was checked against Crossref and OpenAlex; DBLP was not usable this pass (Anubis bot-check). No shadow libraries were used. Fresh files were written under `/tmp/round5-secondcheck/`.

Status key: CONFIRMED (link, metadata, claim, licence, and peer-review status hold); CORRECTED (the item is usable, but the Round 5 write-up needs a specific fix); FAILED (the stated free PDF could not be re-read, or a claim could not be confirmed from a freshly fetched source).

## Kept sources

### R5.1 Bálint, Valasek and Gergó (2019). CONFIRMED.

- Link: official journal PDF https://www.cyber.bibl.u-szeged.hu/index.php/actcybern/article/download/4004/3973 returned 200, application/pdf, 870500 bytes. It is the stated paper: header "Acta Cybernetica 24 (2019) 17-28", title "Operations on Signed Distance Functions", authors Csaba Bálint, Gábor Valasek, and Lajos Gergó, DOI 10.14232/actacyb.24.1.2019.3 printed on the first page. Item page https://www.cyber.bibl.u-szeged.hu/index.php/actcybern/article/view/4004 also 200; its citation_* meta tags match volume 24, issue 1, pages 17-28. Author copy https://people.inf.elte.hu/csabix/publications/articles/acta-2018-sdf-operations.pdf also 200 (820683 bytes, 13 pages, dated 10 Feb 2019); the official file is the 12-page journal version dated 20 May 2019. Quotes below are from the official PDF. The DOI resolver itself returned 500 today, as Round 5 said; Crossref still has the record.
- Metadata: Crossref and OpenAlex agree with the cited authors, title, Acta Cybernetica 24(1):17-28, 2019, DOI 10.14232/actacyb.24.1.2019.3. Crossref issued date 21 May 2019. ISSN 0324-721X (print); the journal also lists 2676-993X (online).
- Claim: official PDF, after Lemma 2 (triangle inequality for distance to a closed set: "d(x, A) - d(y, A) <= d(x, y)"), Corollary 1 reads: "Every signed distance function is Lipschitz continuous and their smallest Lipschitz constant is 1. Formally: forall f : R3 -> R SDF : inf Lip f = min Lip f = 1." The proof starts from Lemma 2 and does not rest on quoting Hart for this fact. Hart is cited for sphere tracing in the introduction ("Hart noted in [10] that SDFs could be rendered efficiently using a technique called sphere tracing"). The paper's main theorem is about when f-c remains an SDF after an offset. No smooth-min bound.
- Licence/access: free journal PDF. No Creative Commons line in the PDF or on the item page (DC.Rights is an empty "Copyright (c)" field). The journal about page says "The content is available free of charge." OpenAlex marks it diamond OA, licence null. Treat as free to read, all rights reserved unless a later journal term is found.
- Peer-reviewed: yes. Acta Cybernetica is a University of Szeged journal that solicits original unpublished scientific papers.

### R5.2 Bálint, Valasek and Gergó (2023). CONFIRMED.

- Link: https://www.cad-journal.net/files/vol_20/CAD_20(6)_2023_1154-1174.pdf returned 200, application/pdf, 4434917 bytes. It is the stated paper. The DOI landing page returned 500 today (same as Round 5); the journal issue page https://www.cad-journal.net/files/vol_20/Vol20No6.html (200, title "CAD Journal | Volume 20 Number 6") lists this article at 1154-1174 with the same DOI and a relative PDF link.
- Metadata: Crossref and OpenAlex have the title, authors, pages 1154-1174, DOI 10.14733/cadaps.2023.1154-1174, date 6 Mar 2023, and journal name "Computer-Aided Design and Applications", but they omit volume and issue. The publisher PDF footer and the official issue page both print "Computer-Aided Design & Applications, 20(6), 2023, 1154-1174", and the PDF prints "DOI: https://doi.org/10.14733/cadaps.2023.1154-1174". The 20(6) citation is therefore confirmed from the publisher, not from Crossref. Crossref author names lack accents; the journal site and PDF use Bálint, Valasek, Gergó.
- Claim, introduction: "the resulting function is not an exact SDF, and the error can be arbitrary large under certain conditions" (the paper's wording is "arbitrary large", not "arbitrarily"). They work with "a subset of Hart's signed distance lower bounds called signed distance function estimates (SDFE)". On Hart and min: "He also proved that signed distance lower bounds are closed under minimum". Theorem 6 (Set operations), section 9: if f and g are SDFEs and 0 < delta <= diam{f=0}, then union h=min(f,g) (for f0- union g0- if f0+ is bounded), intersection h=max(f,g) (for f0- intersect g0- if f0- is bounded), and difference h=max(f,-g) (for f0- minus g0- if f0- is bounded) "produce an SDFE" with a stated precision involving the set-contact smoothness modulus. Immediately after: "The theorem is proved in Appendix H. The theorem directly implies sphere tracing convergence on the resulting representation for the union, intersection, and difference operations." Hard min/max only; no smooth-min deviation bound.
- Licence/access: free to read on the journal site. PDF footer: "© 2023 CAD Solutions, LLC, http://www.cad-journal.net". Not CC. The website footer elsewhere says "Copyright © U-turn Press LLC." OpenAlex marks it diamond OA, licence null.
- Peer-reviewed: yes.

### R5.3 Galin, Guérin, Paris and Peytavie (2020). FAILED (stated free PDF not obtained this pass; bibliographic metadata otherwise confirmed).

- Link: the stated HAL file https://hal.science/hal-02507361v1/file/lipschitz-author-version.pdf, and the same path on hal.archives-ouvertes.fr and inria.hal.science, all returned HTTP 200 with a 12533-byte Anubis "Making sure you're not a bot!" HTML page, not a PDF. WebFetch of the HAL file URL returned 403. The EG Digital Library "PDF" link (https://diglib.eg.org/bitstream/handle/10.1111/cgf13951/v39i2pp545-554.pdf, also linked from Axel Paris's project page) returned a DSpace login page, not the file. Eric Galin's Articles directory has no 2020 segment-tracing PDF; his 2020 page points at the same HAL document URL. I did not use the earlier local copy at `/workspace/specula-research/raw/st.pdf`. So the operator claims (lambda = max(lambda_A, lambda_B) for Booleans, lambda = lambda_A + lambda_B for blends, additive field f = fA + fB) were not re-read from a freshly fetched source this pass.
- Metadata, confirmed without the PDF: Crossref and OpenAlex have title "Segment Tracing Using Local Lipschitz Bounds", Computer Graphics Forum 39(2):545-554, 2020, DOI 10.1111/cgf.13951. OpenAlex authors: Éric Galin, Éric Guérin, Axel Paris, Adrien Peytavie (Crossref drops the accents on the first names). The Paris and Peytavie project pages, and the Paris page BibTeX, agree on volume, number, and pages. The work is a CGF journal article (peer-reviewed).
- Licence/access, from the HAL record as indexed (the record page itself was also bot-checked for me): author version, HAL Authorization, not CC. The journal version is Wiley / Eurographics. OpenAlex marks green OA via HAL.
- Because the text was not re-opened, I do not confirm or correct the quoted Lipschitz-operator sentences.

### R5.4 Bálint and Valasek (2018). CONFIRMED.

- Link: https://people.inf.elte.hu/csabix/publications/articles/eurographics-2018-shortpaper.pdf returned 200, application/pdf, 2352355 bytes. It is the stated paper: "Accelerating Sphere Tracing", Csaba Bálint and Gábor Valasek, header "EUROGRAPHICS 2018 / O. Diamanti and A. Vaxman" and "Short Paper". The EG item https://diglib.eg.org/items/ce5ffffe-d78a-4d90-8ace-5184cc0f336c (200) and DOI https://doi.org/10.2312/egs.20181037 resolve to that item. Crossref has no record for this DOI (plain 404 from the API), which is common for Eurographics short-paper DOIs; OpenAlex has the work and lists pages 29-32, matching the EG bitstream path .../egs20181037/029-032.pdf. The citation as EG 2018 Short Papers is right; do not add "CGF Vol 37, No 2" from the PDF Subject metadata field, which looks like a template leftover.
- Claim, abstract: "a sphere tracing variant designed to take optimal step sizes near planar surfaces"; "compare its performance to classical (by Hart) and relaxed (Keinert et al.) sphere tracing"; "Our second contribution is a multi-resolution rendering strategy that can be used with any sphere tracing variant." They assume the usual unit Lipschitz setting rather than re-proving it: "Let Lip f <= 1 denote the Lipschitz constant [EEJ04] of function f. Since f is an SDFE, Lip g <= Lip f + Lip(t -> alpha t) <= 1 + alpha." The paper cites sphere tracing as Hart [Har94]; that year typo in their bibliography is theirs, not Round 5's. No Lipschitz proof of the Hart 1996 kind, and no smooth-blend bound.
- Licence/access: footer "c 2018 The Author(s) / Eurographics Proceedings c 2018 The Eurographics Association." Author-hosted copy, free to read, not CC.
- Peer-reviewed: yes (Eurographics short papers).

### R5.5 Gillespie, Yang, Botsch and Crane (2024). CORRECTED (licence, and where the quote sits).

- Link: https://markjgillespie.com/Research/harnack-tracing/HarnackTracing.pdf returned 200, application/pdf, 11155458 bytes, 18 pages. It is the stated paper. ACM Reference Format on page 1: "Mark Gillespie, Denise Yang, Mario Botsch, and Keenan Crane. 2024. Ray Tracing Harmonic Functions. ACM Trans. Graph. 43, 4, Article 99 (July 2024), 18 pages. https://doi.org/10.1145/3658201". Project page https://markjgillespie.com/Research/harnack-tracing/ also 200. The ACM official PDF URL returned 403 to curl.
- Metadata: Crossref and OpenAlex agree on title, authors, ACM Transactions on Graphics 43(4), July 2024, DOI 10.1145/3658201, 18 pages. They list pages as 1-18 and omit the article number; Article 99 is confirmed from the author PDF's ACM reference line. Crossref issued date 19 July 2024.
- Claim: the quoted restatement is in Section 1, but not at the opening of that section (it follows a short capability list). The left-column text is: "Our method is inspired by the sphere tracing algorithm of Hart [1996], which ray traces level sets of a function f with a known Lipschitz bound, i.e., a value C > 0 such that, for all points x, y we have |f(x) - f(y)| <= C ||x - y||. At any time t, one can hence be certain that no point of the surface S is contained within a ball of radius R = (f(r(t)) - f*)/C, providing a conservative step size for ray tracing. An important special case are signed distance functions (SDFs), with Lipschitz constant C = 1." (The PDF joins the last two sentences of the radius clause with a dash; Round 5 used a comma. The wording is otherwise as quoted.) It is a restatement, not an independent proof. No smin bound.
- Licence/access, correction: the author PDF prints "© 2024 Copyright held by the owner/author(s)." and the usual ACM third-party-components sentence, with no Creative Commons line in the extracted text, as Round 5 said. However, Crossref lists a CC BY 4.0 licence on the version of record (delay 0 days), and OpenAlex also marks the published TOG article as cc-by, hybrid OA. For the design doc, the published article is CC BY 4.0; the author PDF I fetched does not itself display that badge.
- Peer-reviewed: yes (ACM TOG).

### R5.6 Zanni (2023 arXiv; journal 2025). CORRECTED (arXiv and journal licences; article number not in Crossref/OpenAlex).

- Link: https://arxiv.org/pdf/2304.09673 returned 200, application/pdf, 8771895 bytes, 16 pages. The abs page https://arxiv.org/abs/2304.09673 (200) has title "Synchronized-tracing of implicit surfaces", author Cédric Zanni, "Submitted on 19 Apr 2023", arXiv:2304.09673. This is v1, as cited. The later journal PDF was not opened this pass (ACM 403; inria.hal.science/hal-04765895 also served an Anubis page).
- Metadata of the opened version: confirmed as above. Journal record (not opened): Crossref and OpenAlex have Cédric Zanni, "Synchronized Tracing of Primitive-based Implicit Volumes", ACM Transactions on Graphics 44(1), pages 1-15, DOI 10.1145/3702227. Crossref issued 13 Nov 2024 and lists published 28 Feb 2025; OpenAlex has publication_date 28 Oct 2024. Neither Crossref nor OpenAlex lists an article number, so "Article 6" was not confirmed from those publisher APIs. Treat quotes as from the 2023 arXiv, which may differ from the journal wording, as Round 5 already warned.
- Claim, from arXiv v1: "on a family of smooth polynomial bounded operator [Dekkers et al. 2004]". Equations (4)-(5): "The C^2 variant for the smooth union is defined as: g_union(f0, f1, k) = csg_union(f0, f1) - d(f0, f1, k)" (4), with d(f0, f1, k) = (k/6) * max(1 - |f0-f1|/k, 0)^3 (5) (the 3 is a superscript on the max term; k/6 is written as a stacked fraction, which is why a text search for the string "k/6" misses it). Smooth intersection and difference: g_cap = csg_cap + d(f0, f1, k) (6); g_diff = csg_diff + d(f0, -f1, k) (7). The paper does not state the peak d = k/6 at f0 = f1 as a deviation bound or theorem. For tracing: "We set the Lipschitz bound to obtain artifact-free rendering on all presented examples. In practice, we set it to 1.45 to accommodate for field compression associated with our compact operators."
- Licence/access, correction: the arXiv abs page links the standard arXiv non-exclusive distribution licence 1.0 (http://arxiv.org/licenses/nonexclusive-distrib/1.0/), not a CC licence and not only a vague "standard arXiv reuse". The later journal version, according to Crossref and OpenAlex, is CC BY 4.0 (delay 0 days), hybrid OA. I did not open that journal file.
- Peer-reviewed: the journal article is; this pass read the arXiv preprint.

### R5.7 Santos (2016 blog). CONFIRMED.

- Link: https://vgracianos.github.io/blog/smin/ returned 200, text/html, 23441 bytes. Title "Smooth Minimum", by Vinicius Santos. meta datePublished / visible "Updated: July 7, 2016".
- Metadata: not a journal article; the citation as a personal blog post updated 7 July 2016 is right.
- Claim: "I have used the below code in some of my shaders but never had the time to fully comprehend it, so I decided to derive it myself." He starts from a C^1-matching requirement at the edges of the blend band and arrives at f(a, b, k) = mix(a, b, h) - k h (1-h), with h = 1/2 + (a-b)/(2k) inside |a-b| < k. He does not write "the maximum deviation is k/4". That peak follows from the formula: at a = b, h = 1/2, the subtracted term is k*(1/2)*(1/2) = k/4. No Lipschitz or "never overstates" proof.
- Licence/access: no content licence on the page. Footer "© 2022 Vinicius Santos." The site theme is MIT-licensed; that does not licence the post. Treat the prose as all rights reserved.
- Peer-reviewed: no.

### R5.8 Seyb, Jacobson, Nowrouzezahrai and Jarosz (2019). CONFIRMED.

- Link: https://cim.mcgill.ca/~derek/files/publication-files/seyb19nonlinear.pdf returned 200, application/pdf, 9841240 bytes, 12 pages. It is the stated paper. ACM reference line: "ACM Trans. Graph. 38, 6, Article 229 (November 2019), 12 pages. https://doi.org/10.1145/3355089.3356502". The Dartmouth project page https://cs.dartmouth.edu/~wjarosz/publications/seyb19nonlinear.html redirected to http://outage.dartmouth.edu/ with HTTP 500, as Round 5 found for the Dartmouth PDF.
- Metadata: Crossref and OpenAlex agree on title, authors, ACM Transactions on Graphics 38(6), DOI 10.1145/3355089.3356502, 12 pages. They list pages as 1-12 and omit the article number; Article 229 is confirmed from the PDF. Crossref issued 8 Nov 2019, published-print 31 Dec 2019.
- Claim, first page: "smooth blending, enable powerful and intuitive sculpting tools for creating complex SDF models. SDF metric properties also admit efficient surface rendering with sphere tracing". Nearby, implicit representations are credited with "trivially robust constructive solid geometry operations, domain repetition and smooth blending." The new work is conservative walking along a curved ray in undeformed space so deformed SDFs can still be traced without an inverse deformation ("we do not have to evaluate D^{-1} inside the integral"). It uses the SDF/Lipschitz metric rather than proving bounds for blends. Not a Hart Lipschitz proof and not a Dekkers or smin-bound replacement.
- Licence/access: "© 2019 Copyright held by the owner/author(s). Publication rights licensed to ACM. This is the author's version of the work. It is posted here for your personal use. Not for redistribution." Not CC. OpenAlex marks the ACM PDF as bronze OA. Crossref licence URL is ACM's copyright policy.
- Peer-reviewed: yes (ACM TOG / SIGGRAPH Asia 2019).

## Smooth-min numeric deviation bound (independent search)

Round 5's claim: no modern peer-reviewed paper states, as a result, that polynomial smin deviates from min by at most k/4, k/6, or log(2)*k.

I searched arXiv (export API and the HTML search), the open web, and Google Scholar-style queries for those constants together with smooth min / smooth union / SDF / signed distance / sphere tracing. HAL record pages were bot-blocked, so HAL was only covered via web-search snippets, not a full-text HAL crawl. Shadow libraries were not used.

- CONFIRMED as to peer-reviewed sources: I did not find a modern peer-reviewed paper that states that numeric bound as a result or theorem. Alblas, Brune, Yeung and Wolterink, "Going Off-Grid: Continuous Implicit Neural Representations for 3D Vascular Modeling," arXiv:2207.14663v2, 16 Sep 2022, later STACOM 2022 (MICCAI workshop, peer-reviewed, reported as pages 79-90), gives the quadratic kernel in eq. (5): gamma_smooth = (1/(4k)) * max(k - |SDF1 - SDF2|, 0)^2, and does not state that the peak is k/4. That is the same "formula present, bound unstated" situation Round 5 already noted for Zanni eq. (5) and for Barbier et al. 2025.
- CORRECTED as to "no modern source at all states it": Li, Bohan J., et al., "SDFStent: Real-time interactive virtual stenting via SDF deformation fields," arXiv:2605.22009v1, 21 May 2026, does state the k/4 deviation explicitly. Eq. (10) is the quadratic smin, and eq. (13) is "smin(alpha, beta) = alpha - k/4" at alpha = beta, with the prose "the resulting SDF value is reduced by exactly k/4 mm, leading to a slight inflation of the shape by k/4 mm." This is a preprint "Under review at Computer Methods and Programs in Biomedicine", so it is not yet peer-reviewed and does not overturn the peer-reviewed half of the Round 5 claim. If that paper is later accepted, it would be a citable modern statement of the quadratic bound (still not k/6 or log(2)*k).
- Informal pages (notes and blogs) state k/4 or k/6, including Quilez's smin article already in item 6a. Those are not peer-reviewed.

## Bottom line of this check

R5.1, R5.2, R5.4, R5.7 and R5.8 can be used as Round 5 wrote them. R5.5 is fine to cite for the C=1 restatement; add that the published TOG article is CC BY 4.0, and that the quote sits in Section 1 rather than at its first paragraph. R5.6's equations and the 1.45 Lipschitz remark are confirmed from arXiv v1; record the arXiv licence as non-exclusive-distrib 1.0, and the later journal as CC BY 4.0 according to Crossref/OpenAlex (journal file not opened). R5.3's operator quotes were not re-verified, because every stated legal PDF URL was bot-checked or login-walled today; the citation metadata itself is sound. The "no modern peer-reviewed numeric smin bound" conclusion still holds, with SDFStent 2026 as a clear non-peer-reviewed statement of the quadratic k/4 reduction.
