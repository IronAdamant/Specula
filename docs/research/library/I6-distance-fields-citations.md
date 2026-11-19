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
