# R9-B1 A face from measurements: DeCarlo-Metaxas-Stone, the morphable model, Farkas's system

Round 9, section B. Access date for every source: **2026-11-28**. PDFs were downloaded to the box and read with poppler `pdftotext`. Page images were checked where the text layer dropped symbols. Pages are **PDF pages** of the copies named below, with printed page numbers added where they differ. Everything is in my own words; formulas are restated in my own notation. **Method only:** no code, no data and no model file from any source was used. Blanz-Vetter's face scans are not public and are not wanted; Section 3 says what their method still gives without them. Derivations marked **(mine)** are my own reasoning, not claims of the sources.

**Questions this item answers for the lane** (brief B1):
- How is a face surface generated from a list of landmark distances and proportions?
- Which constraints are hard and which are soft?
- How many numbers does a recognisably different face need?
- How do sex and age enter?
- What goes wrong (the stated failure cases of each)?

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| S1 | DeCarlo D., Metaxas D., Stone M. (1998). "An anthropometric face model using variational techniques." *Proc. SIGGRAPH 98*, pp. 67-74. doi:10.1145/280814.280823 | **None found that is clearly legal.** The author's old Rutgers link (cs.rutgers.edu/~decarlo/pubs/sg98.pdf) now returns 404. Third-party mirrors exist (graphicon.ru, ivizlab.sfu.ca) but their permission is unclear, so **they were not used**. OpenAlex lists the paper as closed | ACM copyright | **Not read in full.** Search-engine extracts of the paper's own text were compared with S1b (see note) |
| S1b (substitute, by the same first author) | DeCarlo D.M. (1998). *Generation, Estimation and Tracking of Faces.* PhD dissertation, University of Pennsylvania (Technical Report MS-CIS-98-36). Supervisor D. Metaxas | Rutgers Center for Cognitive Science report archive: https://ruccs.rutgers.edu/images/archive/tech_rpt/tr-48/thesis.pdf (133 pp.) | "Copyright Douglas M. DeCarlo 1998" (PDF p.2); free to read, no open licence | Yes: Sections 2.3-2.4 (PDF p.30-41), Chapter 3 (PDF p.43-61), Section 4.4 (PDF p.71-73), Section 8.1 (PDF p.118-119). **Printed page = PDF page minus 10** |
| S2 | Blanz V., Vetter T. (1999). "A morphable model for the synthesis of 3D faces." *Proc. SIGGRAPH 99*, pp. 187-194. doi:10.1145/311535.311556 | University of Basel archive of the authors' group: https://shapemodelling.cs.unibas.ch/gravis-site-archive/publications/Sigg99/morphmod2.pdf (8 pp.). OpenAlex also lists a green copy at MPG PuRe (that download failed from the box) | ACM copyright; author-posted. OpenAlex licence field "other-oa" | Yes, all 8 pages (p.6-7 also as page images) |
| S3 (for Farkas's system) | Farkas L.G. (ed.) (1994). *Anthropometry of the Head and Face*, 2nd ed., Raven Press; Farkas L.G. (1987) *Anthropometric Facial Proportions in Medicine* | **None.** Both are in-copyright books with no free copy | Publisher copyright | **Not read.** Described only through S1b §2.3 and S4 |
| S4 (open account of a Farkas-style set, with sex and age) | Kesterke M.J., Raffensperger Z.D., Heike C.L., et al., Weinberg S.M. (2016). "Using the 3D Facial Norms Database to investigate craniofacial sexual dimorphism in healthy children, adolescents, and adults." *Biology of Sex Differences* 7:23. doi:10.1186/s13293-016-0076-8 | Europe PMC full text PMC4841054 (read as XML; **no page numbers, cited by section and table**) | **CC BY 4.0** | Yes: Methods, Table 2, Results (shape analysis), Discussion |

**Note on S1 against S1b.** S1b states that its Chapter 3 "was first presented in" the SIGGRAPH paper (S1b PDF p.43, printed p.33). Search-engine extracts of S1's own text (abstract, the opening of its Section 4 and the start of its results) match S1b's wording almost word for word: the B-spline control mesh, the prototype p0, the two logical steps, and the North American Caucasian young-adult data. **Whether S1 has anything that S1b lacks (a figure, a timing, a sentence on limits) is unverified.** Everything below is cited to S1b.

## 1. DeCarlo et al.: the two-step generator (S1b Chapter 3)

The idea in one line (S1b PDF p.43-44): first draw a random but plausible **set of measurements**; then build the **smoothest surface that meets them**, measured against a hand-made prototype face. Generating faces is thus reduced to generating measurement sets.

### 1.1 The measurements: Farkas's system as S1b describes it (S1b §2.3, PDF p.30-34)

- **Landmarks.** 47 named points on the face, each defined by a visible or palpable skin or bone feature. They are known by short abbreviations of anatomical terms: en (endocanthion, inner eye corner), t (tragion, top of the tragus), n (nasion), sn (subnasale), gn (gnathion, lowest chin point), or (orbitale), v (vertex), tr (trichion), ch (cheilion, mouth corner), al (alare) (PDF p.31-32, Fig. 2.3).
- **Head frame.** Five landmarks fix a canonical frame.
  - The horizontal plane runs through t and or on each side: the Frankfurt horizontal (FH).
  - The vertical midline runs through n, sn and gn.
  - The head is physically placed in FH position before measuring (PDF p.31).
- **Five kinds of measurement** (PDF p.31-32, Fig. 2.4):
  1. Shortest (straight) distance between two landmarks, e.g. en-ex (eye fissure length).
  2. Axial distance: the distance along one axis of the FH frame, e.g. v-tr height.
  3. Tangential distance: length along a prescribed path over the skin, e.g. ch-t.
  4. Inclination of a landmark-to-landmark line against a frame axis, e.g. the ear's axis against the vertical.
  5. Angle between directions, e.g. the mentocervical angle at the chin.
- **Count.** 132 measurements on the face and head, some paired left and right (PDF p.32).
- **Statistics available.** Per-group means and variances by sex, race, age and other groupings (PDF p.32). **Proportions** (ratios between two measurements) with means and variances, e.g. mouth width ch-ch roughly 1.5 times nose base width al-al (PDF p.33-34, Fig. 2.5). Proportions are said to carry correlation information and to separate groups better than single measurements (PDF p.33).
- **What is missing.** No covariance tables and no fitted distributions are published, only means and variances (PDF p.33-34).

### 1.2 Step one: drawing a plausible measurement set (S1b §3.2, PDF p.47-51)

**Why not draw each measurement on its own** (PDF p.48):
- Assume each measurement is Gaussian with its published mean and SD (the author says Farkas's tests support normality).
- Drawing each one independently gives a set that may be geometrically consistent but still looks wrong, because real measurements are correlated. The example given: the profile slopes sn-pg and li-pg vary widely between people but agree closely within a person.
- Covariances would be the natural fix but are not published, so **proportions are used as the stand-in for correlation** (PDF p.48).

**The proportion graph** (PDF p.49-50, Fig. 3.1):
- Measurements are vertices; each published proportion is an edge between two measurements.
- The graph has cycles, so only some proportions can be used. Each dependent measurement is computed **once**, from one parent, by one proportion.
- A calculation plan is a **spanning branching**: a directed tree in which each vertex has at most one incoming edge. Several base measurements are joined with dummy edges.
- Proportions may be used in either direction; the inverse ratio's distribution is approximated (PDF p.49).

**How uncertainty spreads along an edge** (PDF p.50). Parent s has mean µs and SD σs; the proportion has mean µe and SD σe; both are treated as independent Gaussians. The child d then gets:
- mean µd = µs·µe;
- variance σd² = µs²σe² + µe²σs² + σe²σs² (this is the exact variance of a product of two independent variables **(mine)**).

Each vertex's **weight** is its coefficient of variation σ/µ. A child's weight is always larger than its parent's, so precision falls with depth in the tree (PDF p.50).

**Choosing the plan** (PDF p.50-51):
- Find the branching with the **least total weight**, so the most tightly known measurements drive the rest.
- A modified Prim's algorithm grows the tree from the base, each time adding the outside vertex with the smallest induced weight. The author gives an exchange argument that this is optimal, relying on weights growing along every path (PDF p.51).
- The base measurement is not fixed in advance. The best plan is found for **each** possible base, and the best of those is used (PDF p.49).

**Drawing a face's numbers** (PDF p.49):
1. Sample the base measurement from its Gaussian.
2. Walk the tree, sampling each edge's proportion from its Gaussian and multiplying.

The same procedure can **complete a partial specification**. A user (or a human-factors study wanting an extreme) fixes some measurements, and the rest are filled in plausibly (PDF p.49).

**Validation of step one** (PDF p.58, printed p.48):
- Many measurement sets were generated and their distributions compared with the published ones.
- Means differed by about **1% on average, 4.5% at most**.
- SDs ranged from **5% lower to 20% higher** than published.
- The author notes that the mean differences are well below the differences between population groups.

### 1.3 Step two: the surface (S1b §2.4 and §3.3, PDF p.34-41, p.51-57)

**Representation** (PDF p.52-53, Fig. 3.2):
- A B-spline surface s(u,v), whose control-mesh coordinates form one vector p.
- The mesh is a **tube** with openings at the mouth and the neck, built by hand from an existing polygon face and parameterised to avoid badly distorted patches.
- A hand-made **prototype** p0 sets the generic shape every face starts from. The same mesh and the same prototype serve every face.
- The experiments used a **32 x 32** control grid (PDF p.57). That is 1,024 control points, about 3,000 free coordinates **(mine)**.
- Each landmark has a **fixed (u,v) location** on the surface.

**Base constraints** (PDF p.53): 15 constraints that hold for every face. They keep the model in FH position and give landmarks their meaning; for example, the surface tangent at the vertex v must be horizontal.

**Measurement constraints** (PDF p.53-55), one per measurement:
- **Axial distances are linear in p.** Together with the base constraints they form one matrix equation A p = b. A depends only on *which* measurements are used, so it can be prepared once; only b changes from face to face (PDF p.54).
- **The other four kinds are non-linear.** Each gets a non-negative penalty that is zero when met, and the penalties are summed into P(p) (PDF p.54-55):
  - straight distance: (distance between the two landmark points minus the target)²;
  - surface (tangential) distance: (sum of chord lengths along a fixed curve in (u,v), sampled adaptively, minus the target)²;
  - inclination: squared difference between the unit direction and the target direction (the frame axis rotated by the target angle). The direction may be between two surface points or a surface tangent;
  - angle between two directions: written as two inclination penalties, each direction measured against the other.

**Fairness (the objective)** (PDF p.36-38, p.55):
- The bending energy of a thin plate is the integral of κ1² + κ2². It is not quadratic in p, so it is replaced by the usual linearised version: the integral of the squared second parametric derivatives (uu, uv twice, vv).
- A small **membrane** (first-derivative) term, weighted α, is added to discourage uneven parameterisation. α is kept just large enough to stop approximation error (PDF p.37).
- The energy is taken on (p − p0), so the prototype itself has zero energy. The surface is therefore pulled toward **bending and stretching as little as possible away from the prototype**. One stated benefit is that sharp regions the prototype already has (nose, ears) are left alone by the fairing: the fairing is told to ignore regions where sharp curvature is expected (PDF p.55).
- In matrix form the energy is ½(p − p0)ᵀK(p − p0), with K sparse (PDF p.37-38).

**Solving** (PDF p.39-40, p.55-57):
- **Linear constraints only.** Minimise the energy subject to A p = b with Lagrange multipliers. This is one sparse symmetric linear system, [K Aᵀ; A 0][p; y] = [K p0; b] in my notation. Because the mesh and the set of constraints never change, its **LU factorisation is computed once**. That cost about 3 minutes on a 175 MHz R10000 (PDF p.58). The factors stayed roughly O(n)-sparse, so each later solve was close to linear time (PDF p.56).
- **Non-linear constraints.** The penalties enter the same system as an accumulated force term C on the right-hand side. At each step C is updated by subtracting a step size µ times the gradient of P at the last solution, and the system is solved again with the stored LU factors. µ is chosen adaptively, for example by step-doubling (PDF p.56-57).
- The author found this more stable and faster to converge than conjugate gradients with penalties. Linearising some non-linear constraints into A gave little speed-up and less stability (PDF p.57).
- **Convergence.** Rough shape within 50 iterations; up to 200 iterations for millimetre accuracy; about 1 minute per face (PDF p.58).

**Run used for the results** (PDF p.57):
- 15 base constraints plus **65 measurements**, chosen to cover the face and every kind of measurement.
- **120 proportions** as input to the proportion graph.
- Measurements from Farkas 1994 and proportions from Farkas 1987, both for **North American Caucasian young adults, men and women**.

### 1.4 Hard and soft in S1b, stated plainly

| Constraint | How enforced | Hard or soft |
|---|---|---|
| 15 base constraints (FH pose, landmark meanings) | Linear, Lagrange multipliers, exact | **Hard** |
| Axial-distance measurements | Linear, Lagrange multipliers, exact | **Hard** |
| Straight, surface-path, inclination and angle measurements | Summed penalties driven toward zero by iteration | **Soft as implemented**, run until met to about a millimetre (PDF p.57-58). A penalty that cannot reach zero leaves a residual instead of failing loudly **(mine)** |
| Fairness against the prototype | Objective minimised | **Soft**: it fills in everything the measurements leave open |
| Population statistics | Used only to *draw* the targets; once drawn, the targets are fixed | Not a constraint in the generator |
| Anthropometry in the vision model (Chapter 4) | Spring-like forces toward the mean, each weighted by a power ρ of how improbable the current value is. With ρ around 10, values within one SD are effectively ignored (PDF p.71-72). Proportions get the same treatment (PDF p.73) | **Soft prior** (a bias, never reached exactly) |

## 2. DeCarlo et al.: failure cases and limits, as stated (S1b)

1. **Independent sampling is not enough.** A set of measurements can be geometrically consistent and still look implausible, because correlations are ignored (PDF p.48).
2. **Geometric inconsistency.** A face can be built only "as long as the measurements are geometrically consistent" (PDF p.48). The author offers no automatic test for consistency.
3. **Using every Farkas constraint at once leads to conflict** (PDF p.57):
   - Some measurements give **linearly dependent** rows of A. They were found by inspection and removed by hand, to keep the system invertible.
   - Several non-linear constraints on nearby features, adding no new shape information, can be **geometrically inconsistent** and stop the iteration from converging.
   - The fix was manual: keep only the most **locally confining** constraints (those touching fewer or closer surface places).
4. **The same problem in the wider literature** (PDF p.38). If a fixed surface cannot meet all constraints, earlier work refines the mesh. Dependent or conflicting constraints are left as "open problems" (or ignored) in that literature.
5. **The prototype shows through** (PDF p.58-59):
   - Where the measurements say nothing, every face shares the prototype's shape.
   - The prototype has a measurable effect on the geometry, which shrinks as more measurements are added.
   - There is no data to validate a whole surface against a real person, which would need both the person's measurements and a scan.
6. **Measurements are silent about curvature** (PDF p.51). For example, the mentocervical angle fixes the chin's angle but not how sharply it is rounded.
7. **Ears stay coarse**, partly because few measurements exist inside the ear (PDF p.59).
8. **Statistical assumptions:**
   - Gaussian measurements and proportions, and independence between a proportion and its parent (PDF p.48, p.50).
   - The author expects the true distributions are probably *not* Gaussian, and wants per-person raw data for a real covariance analysis (PDF p.118).
   - The generated SDs ran up to 20% too wide (PDF p.58).
9. **Scope:**
   - One population (North American Caucasian young adults).
   - One prototype, one texture and fixed hair placement (PDF p.58-59).
   - No skull or interior (PDF p.118, listed as future work).
   - Faces only, at rest (measurements assume an expressionless face, PDF p.71).

**How sex and age enter S1b.**
- **Sex:** through **separate population tables** for men and women. Nothing else in the method changes. The author reports, from qualitative comparison only, that sex-typical traits came out right: the chin angle, the slope of the eyes and the height of the lower face, especially at the jaw (PDF p.58, Fig. 3.3).
- **Age:** **not modelled.** Only young adults were used. "child aging" is named only as a possible application (PDF p.118).

**How many numbers.**
- S1b's generator used **65 measurement targets plus 15 fixed base constraints**. The targets were drawn from one or a few base draws plus one proportion draw per dependent measurement, so roughly 65 random numbers per face **(mine, counting from PDF p.49-50 and p.57)**.
- S1b does **not** test how few numbers would still give a recognisably different face.
- The author's own sample: thirty generated men "exhibit the expected range of geometric variation" (PDF p.58, Fig. 3.4). This is a qualitative claim.

## 3. Blanz and Vetter (1999): the morphable model (S2), method only

**What it is** (S2 PDF p.1-3):
1. Put 3D head scans into dense point-to-point **correspondence**.
2. Stack each face's vertex coordinates into a **shape vector** S and its colours into a **texture vector** T.
3. Run PCA on each.
4. A new face is the mean plus a weighted sum of principal components, for shape and for texture separately.

**Data used, which the lane may not use and does not need:**
- 200 laser scans of young adults (100 men, 100 women), without make-up, accessories or facial hair (S2 PDF p.2).
- Each scan was cut behind the ears and above the shoulders, normalised in position, and had about 70,000 vertices (PDF p.2).

**Prior.** A multivariate normal is fitted to the 200 faces. The probability of a coefficient vector falls off as exp(−½ Σ (αi/σi)²), where σi² are the eigenvalues. This prior is what "regulates" how likely a generated face looks (S2 PDF p.3, Eq. 2).

**Segments** (PDF p.3, Fig. 2):
- The face is split into eyes, nose, mouth and the surrounding region.
- Each region is combined independently and the regions are blended at their borders.
- This multiplies how many faces the model can express from the same examples.

**Attributes: how sex, fullness and nose shape enter** (PDF p.3-4, Eq. 3, Fig. 3):
- Faces are hand-labelled for an attribute (gender, fullness, eyebrow darkness, double chin, hooked against concave nose).
- The attribute's direction is the label-weighted sum of each face's difference from the mean. Adding or subtracting multiples of it changes the attribute while keeping others as constant as possible.
- For a two-class attribute such as gender, each class gets a constant label.
- The authors assume the attribute is a **linear** function of the face. Under that assumption there is one optimal direction for the whole face space, and their formula gives the direction of least variance-normalised length (PDF p.4).
- **Age is not an attribute in the paper.** It is listed only as future data (see below).

**Distinctiveness.** A caricature multiplies all coefficients by one factor, moving the face away from the mean (PDF p.4).

**How many numbers** (PDF p.7):
- The reconstructions used the **first 100 shape and the first 100 texture principal components**, each split over 4 segments.
- Because the average is subtracted, the span of m example faces has at most m − 1 dimensions (PDF p.3, footnote 2); with 200 faces that is 199 components of each kind **(mine, arithmetic)**.
- The paper does not state how much variance 100 components explain.

**Failure cases and limits, as stated:**
1. **Only faces inside the span of the examples.** Shape and texture are restricted to the space the database spans (PDF p.4). Fitting the model cannot add a dimension (PDF p.6).
2. **Correspondence can fail.** The optic-flow correspondence is unconstrained, so it "fails on some of the more unusual" faces. A bootstrapping loop was needed to build the model (PDF p.6-7). On low-structure regions such as the forehead and cheeks, the flow is sometimes spurious and needs smoothing (PDF p.6, §5.1).
3. **Fitting a face to one image is ill-posed.** Many non-face surfaces give the same image, which is why the prior is essential (PDF p.4, p.7). Errors show mainly beyond about 60° of rotation (PDF p.7).
4. **Population limits.** The database suits Caucasian faces of middle age; extending to children, the elderly and other groups is future work (PDF p.7, §7). **Disagreement inside S2:** §2 calls the scanned subjects "young adults" (PDF p.2), while §7 says the database is sufficient for "Caucasian faces of middle age" (PDF p.7). Both are given.
5. **Face area only.** The laser scanner cannot capture hair, so the head beyond the face must be replaced (PDF p.7).
6. **Static expressions only.** Each scan takes at least 10 seconds (PDF p.7).
7. **Unusual paintings.** For the Mona Lisa, the authors had to use a different texture-transfer method because illumination-corrected texture extraction was hard (PDF p.7).
8. **Cost.** 10^5 iterations, about 50 minutes on an R10000 per fit (PDF p.7).

**What the method still gives without its data (mine):**
- The idea of a **prior on a coefficient vector**: a face is plausible when its normalised distance from the mean is small. This works with any basis, including one built from measurement deviations instead of scans.
- **Segments** with blended borders, as a way to multiply variety.
- **Attribute directions** from labelled examples, and **caricature** by scaling away from the mean. Both are geometric operations that work on any vector of numbers. For example, the engine could apply them to a Farkas-style measurement vector built from round 2's published means and SDs.
- What does *not* carry over: dense correspondence and PCA need a scan collection, which the engine will not use.

## 4. Sex and age, from an open Farkas-style data set (S4)

S4 is an open account of a Farkas-style measurement set with sex and age, from the 3D Facial Norms study.

**Sample and method** (S4 Methods, Tables 1-2):
- 1,555 people aged 3-25 (646 male, 909 female), self-identified European ancestry, screened for craniofacial conditions, in six age groups.
- **29 traditional linear anthropometric distances**:
  - 5 taken with spreading calipers: maximum cranial width eu-eu, minimum frontal width ft-ft, maximum facial width zy-zy, mandibular width go-go, maximum cranial length g-op;
  - 24 from 24 landmarks on 3D photographs.
- The landmarks were collected with intraclass correlations above 0.95.
- Sexes were compared with **height and age as covariates** (ANCOVA), with effect sizes.

**Results** (S4 Results and Discussion):
- **Sex differences at every age**, in 3-5-year-olds already for just over half the measures.
- **In shape at every age.** Females have a more upward-slanted eye fissure at all ages. After puberty, males show less prominent orbits, a broader and more projecting nose, and a more forward lower jaw.
- **Size.** Male faces were larger in every age group. The shape differences were **independent of size**.
- **When dimorphism appears:**
  - Facial measures jump just after late childhood; cranial and nasal measures jump just after puberty. All three plateau after adolescence.
  - Eye and mouth measures show no puberty spike and less dimorphism in adulthood.
- **Growth timing.** Females reach adult facial size earlier than males. Between puberty and adolescence, male nasal growth is nearly five times female growth.
- **Some differences arrive in stages.** The adult male nose involves three separate shape changes at different ages: alar widening and forward tip growth at adolescence, plus a lowering of nasion that starts in early childhood.

**Limits S4 states:**
- Only US whites.
- Broad age groups.
- **Sparse landmarks**, with no points on the brow ridges or cheeks although both differ by sex.

**What this means for the generator (mine).**
- **Sex.** Neither S1b nor S2 models sex beyond separate tables or one linear direction. S4 shows sex differences are partly **size** (handled by scale) and partly **shape at fixed size**.
- **Age.** S4 shows age is **not one direction**: different regions change at different ages. An age input therefore needs per-region growth curves, not one blend weight.

## 5. Comparison

| Question | DeCarlo et al. (S1b) | Blanz-Vetter (S2) |
|---|---|---|
| Input numbers | Named measurements and proportions (65 targets + 15 base constraints in the results) | 100 + 100 PCA weights (x 4 segments) |
| Where plausibility comes from | Published means, SDs and proportions; proportions stand in for correlation | A Gaussian prior learned from 200 scans |
| Surface between the numbers | Smoothest deviation from one prototype (thin-plate + membrane on p − p0) | Linear combination of example faces |
| Hard constraints | Base and axial (linear, exact) | Staying inside the span (by construction) |
| Soft | Non-linear measurements (penalties), fairness | Prior against image fit |
| Sex | Separate tables | Labelled linear direction |
| Age | Not modelled | Not modelled; future data |
| Main failure | Conflicting or dependent constraints, chosen by hand; prototype bias; ears | Faces outside the example span; correspondence on unusual faces; hair; one population |
| Needs restricted data? | **No.** Needs published tables and a hand-made prototype | **Yes** for the basis; the prior, segment, attribute and caricature ideas carry over |

**Lane notes (mine, not from the sources):**
1. S1b is the closest match to the owner's order of building: numbers, then a surface fitted to them with a prototype filling the gaps. It needs **no restricted data**. Its two hard parts are the ones Specula must solve anyway:
   - **Detecting dependent or conflicting constraints automatically** (S1b did it by hand; see C3);
   - **Bounding the spread of drawn values** (S1b's variance formula is the product rule of C2's interval and affine arithmetic in Gaussian form).
2. The proportion graph is a ready-made plan as data: vertices are named measurements and edges are ratios with bounds. A minimum-weight branching gives the order in which numbers are drawn. Replacing each Gaussian with a bounded range makes every number bounded by construction.
3. Since the prototype shows through wherever the numbers are silent, a dense, well-chosen landmark set and a neutral prototype matter more than solver detail. S4's warning that brow ridges and cheeks have no landmarks applies directly.
