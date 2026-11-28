# R9-A1 Bodies from measurements, as a method

Round 9, section A (the body surface). Access date for every source: **2026-11-28**. Each PDF was downloaded to the box and read with poppler `pdftotext`; plots and figures were read from page images. Page numbers are **PDF pages** of the copy named in the table unless a printed page is also given. Everything is in my own words. **Method only:** no code, model file, mesh, weight or data set from any source was downloaded, opened or used. Readings taken off a plot (not stated in the text) are marked **(read off plot)**. My own derivations and lane notes are marked **(mine)**.

**Question this item answers for the lane.** How does a handful of numbers (stature, weight, girths) become a whole body surface? How many independent numbers explain how much of the variation, and what are the first few? What differs between male and female models? What errors do the authors report against measured bodies? Which numbers matter, in what order?

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| S1 | Allen B., Curless B., Popović Z. (2003). "The space of human body shapes: reconstruction and parameterization from range scans." *ACM TOG* 22(3) (SIGGRAPH 2003): 587-594. doi:10.1145/882262.882311 | Authors' copy, University of Washington GRAIL: https://grail.cs.washington.edu/projects/digital-human/pub/allen03space.pdf | ACM copyright (Crossref licence link: ACM copyright policy). Author-posted copy, free to read; no open licence | Yes, all 8 PDF pages (PDF p.1-8 = TOG p.587-594) |
| S2 | Seo H., Magnenat-Thalmann N. (2003). "An automatic modeling of human bodies from sizing parameters." *Proc. I3D '03*, pp. 19-26. doi:10.1145/641480.641487 | **No free legal copy found.** ACM DL returned a bot-check page; Semantic Scholar and OpenAlex list it as closed; a CiteSeerX link failed | ACM copyright | **No** (see S2b) |
| S2b | **Labelled substitute for S2, same first author, same method, fuller:** Seo H. (2003). *Parameterized human body modeling.* Doctoral thesis, Université de Genève. doi:10.13097/archive-ouverte/unige:237 | https://archive-ouverte.unige.ch/unige:237 (PDF: https://access.archive-ouverte.unige.ch/access/metadata/89d8a776-b49e-45dd-aa11-af960f88b7d9/download) | **Sources disagree:** OpenAlex lists the UNIGE copy as **CC BY**; the UNIGE cover sheet (PDF p.1) says "protected by copyright. Please refer to copyright holder(s) for terms of use". Treat as free to read, not to copy | Yes: ch. 4.1, ch. 5 (all), ch. 8.2 (PDF p.57-90, 119-120). Printed page = PDF page minus 23 in ch. 5 |
| S3 | Anguelov D., Srinivasan P., Koller D., Thrun S., Rodgers J., Davis J. (2005). "SCAPE: shape completion and animation of people." *ACM TOG* 24(3): 408-416. doi:10.1145/1073204.1073207 | Author copy at Stanford: https://ai.stanford.edu/~drago/Projects/scape/shapecomp.pdf (the older robotics.stanford.edu link failed) | ACM copyright; author-posted copy. **SCAPE data: not used** | Yes, all 9 PDF pages |
| S4 | Loper M., Mahmood N., Romero J., Pons-Moll G., Black M.J. (2015). "SMPL: a skinned multi-person linear model." *ACM TOG* 34(6), Article 248 (pp. 1-16 in Crossref). doi:10.1145/2816795.2818013 | Author copy at MPI-IS: https://files.is.tue.mpg.de/black/papers/SMPL2015.pdf | ACM copyright; author-posted copy. The paper says the model is made available "for research purposes" (PDF p.1, p.3, p.12). **SMPL model, weights and data: not opened, not used** | Yes, PDF p.1-14 |
| S5 | **Labelled substitute** (for numbers that S1 and S3 do not give: how error falls as components are added, how many scans are needed, what the first components look like on a large data set): Pishchulin L., Wuhrer S., Helten T., Theobalt C., Schiele B. (2017). "Building statistical shape spaces for 3D human modeling." *Pattern Recognition* 67: 276-286. doi:10.1016/j.patcog.2017.02.018 | arXiv:1503.05860v2: https://arxiv.org/abs/1503.05860 | arXiv non-exclusive distribution licence (author preprint). Journal version Elsevier. **Their released model: not used** | Yes, arXiv PDF p.1-24 (page numbers are the arXiv PDF's) |
| (S6) | Hasler N. et al. (2009) "A statistical model of human pose and body shape." Considered as a further substitute | Author link at cs.ubc.ca returned a non-PDF page | - | **No**; not needed for the questions above |

## What the method still gives without the restricted data (mine, from S1-S5)

The SCAPE, SMPL and CAESAR-derived models are learned from scan data whose licences forbid our use. What survives without the data is the **structure**:
1. a template mesh plus per-vertex (or per-triangle) offsets;
2. shape and pose kept in separate terms;
3. a small number of shape coordinates, ordered by how much variation each explains;
4. joint centres predicted from the surface;
5. a linear map from a few measurements to those shape coordinates;
6. a check that keeps the input measurements inside the range the examples covered.

Without data we cannot learn the components. We would have to author the directions (for example, a "taller" direction and a "heavier" direction), using the papers' published orderings as a guide. The lane's existing measured tables (round 5 A3, A13-A15) can supply the targets that a fitted surface must hit.

## 1. Allen, Curless, Popović 2003 (S1)

**Definitions.**
- A **template** is one hole-free mesh.
- A **parameterization** means that every scan is re-expressed as that template, so vertex i means the same anatomical point on every body (PDF p.2).
- Each template vertex gets its own **4x4 affine transform**, so 12 unknowns per vertex (PDF p.3, Section 3).

**Fitting steps (PDF p.3-4, Sections 3.1-3.5).**
1. **Data term.** Sum of squared distances from each transformed vertex to the nearest *compatible* scan point. Compatible means the normals are within 90° and the points are within 10 cm. Each term is weighted by the scanner's confidence (PDF p.3, eq. 1).
2. **Smoothness term.** Neighbouring vertices should have similar transforms (Frobenius norm of the difference). The text notes this is smoothness of the *deformation*, not of the surface (PDF p.4, eq. 2).
3. **Marker term.** Known landmarks on the scan should land on their template vertices (PDF p.4, eq. 3).
4. **Optimiser.** L-BFGS-B. Run first on a low-resolution template against a smoothed scan, then at full resolution. The term weights change through four stages: markers first, then the data term is allowed in, and finally the data term dominates (PDF p.4).
5. **Holes.** Where the nearest scan point is on a hole boundary, the data weight is 0, so the smoothness term carries the template shape across the gap. Confidence values are scaled and clamped so that the weight tapers into holes. Areas known to scan badly (ears, fingers) can be marked so that the template wins there (PDF p.4-5).

**From fitted bodies to a shape space (PDF p.5-7, Sections 4.2-4.5).**
- **PCA.** Stack each fitted body as a 3n vector, subtract the mean, and take principal components with variances in decreasing order. New people are made by sampling the Gaussian that the PCA represents. **PCA was run separately on the male and female data** (PDF p.6).
- **Data used.** 125 male and 125 female CAESAR scans (PDF p.2).
- **Feature analysis (measurements to body).** A linear map M takes a feature vector (for example height and weight, plus a constant) to PCA weights. M is solved with a pseudoinverse over the training set (PDF p.6, eqs. 5-6).
  - The authors use the **cube root of weight**, because weight scales roughly with volume and the map is linear (PDF p.6).
  - To edit an existing person, add M times the change in features to that person's PCA weights (PDF p.6, eq. 7; Fig. 10 on PDF p.8 shows height and weight edits).
- **Markerless and marker-only fitting.** Search in PCA space with a prior that penalises each weight by its own standard deviation (eq. 8). The number of components is small at first and is increased in stages (PDF p.6-7). With markers only (74 points), the same prior gives an approximate whole body (PDF p.7, Fig. 8d-e).
- **Skeleton and skin transfer.** Because vertices correspond, skinning weights copy over directly. Joints are placed by inverse kinematics from 2-3 surface points per joint (PDF p.7, Section 4.6).

**Stated limits and failure cases (S1).**
- **F1-1** Without markers the optimiser falls into local minima; for example, the left arm can start to align with the right arm and never recover. There is also a trivial global minimum: scale every transform to zero and place the collapsed mesh on the scan (PDF p.4). Markers are needed to start. Once enough bodies are fitted, the PCA search can replace them, but if a new scan is far from the template the transform-space match "will fail" (PDF p.6).
- **F1-2** The poses of template and target must be similar. Different poses need dissimilar transforms at bending joints, and the smoothness term penalises exactly that (PDF p.7).
- **F1-3** Small pose differences in the data **leaked into the PCA**. Some components were height or body type, but several clearly included pose (PDF p.7). This is the reason SCAPE and SMPL separate pose (sections 3-4 below).
- **F1-4** Lips were misaligned in under 5% of cases, because there are few face markers (PDF p.7).
- **F1-5** PCA assumes one multivariate Gaussian; the authors call it only "suggestive" of the true distribution (PDF p.7).
- **F1-6** Skeleton transfer is imprecise, because a marker's offset from its joint really depends on body thickness (PDF p.7). Animation is only as good as the template's skinning (PDF p.7).
- **Not given in S1:** a percentage of variance per component, or an error in mm against measured bodies. The only numbers are the 10 cm compatibility threshold and the under-5% lip failures.

## 2. Seo & Magnenat-Thalmann (S2 not available; S2b thesis read)

**Inputs: eight numbers** (S2b ch. 5.1, PDF p.74 = printed p.51, Fig. 5-2):
- 3 lengths: stature, crotch length (crotch height above the ground), and arm length (acromion over the elbow to the wrist);
- 5 girths: neck, bust/chest, under-bust, waist and hip.

The author chose these because a clothing-industry source names them as the primary body measurements for size assignment. The author says that supporting dozens of measurements was out of scope.

**Data** (S2b ch. 4.1, PDF p.57):
- 120 European adult scans, 60 of each sex, from a Tecmath scanner;
- faces removed;
- reduced to at most 75,000 triangles, with holes filled;
- all subjects in the same erect stance, lightly clothed.

**Body = rigid part + elastic part** (S2b ch. 5 intro, PDF p.73 = p.50):
- the **joint** part is per-joint scale and translation in x, y, z, which sets global proportion;
- the **displacement** part is per-vertex offsets on the skin, which give the detailed shape.

The template has 33 joints (6 degrees of freedom each) and 841 skin vertices.

**Method steps (S2b ch. 5.2-5.3, PDF p.73-79).**
1. Fit the template to every scan (ch. 4). Record the joint parameters and the vertex displacements, both in one fixed posture so that posture does not enter the displacements (ch. 5.3.2, PDF p.77).
2. Compress both with PCA. The first 30 components were kept for joints and for displacements; the thesis says 30 were enough to reconstruct the examples (PDF p.73, p.77).
3. **Normalise the measurement space** with PCA, keeping all dimensions (ch. 5.3.3, PDF p.78). The reasons given:
   - stature has the largest spread and would dominate distances;
   - correlated measurements (neck and hip are the thesis's example) would count twice.
4. Fit **Gaussian radial basis interpolators** from the normalised 8-number point to each PCA coefficient. There are 270 interpolators in all: 30x6 for joints and 30x3 for displacements (PDF p.73, p.88).
   - Joint interpolators include a linear term.
   - Displacement interpolators do not. The thesis argues that fat and muscle are strongly non-linear, so a purely elastic fit suits them (PDF p.77).
   - Joint parameters are stored as absolute, not parent-relative, because the interpolator knows nothing of the hierarchy (PDF p.77).
5. **Populations:** in the normalised space, sample each axis independently from a Gaussian whose default variance is the database variance (ch. 5.3.4, PDF p.78).
6. **Exaggeration:** scale the body vector away from the mean (ch. 5.3.5, PDF p.78-79).
7. **Validity check:** project the input measurements into the normalised space and test them against the min and max of the examples on each axis. This box is the "valid measurement volume". An input outside it is moved to the nearest point inside it, or the last valid input is reused (ch. 5.3.6, PDF p.79-80, Fig. 5-5). A min/max rule table from population statistics was considered and set aside (PDF p.79-81).
8. After reshaping, re-bind the skin to the skeleton in the dress pose (ch. 5.4, PDF p.83).

**Stated errors** (ch. 5.5.2, PDF p.90 = p.67, Table 5-4): leave-one-out. Each example is removed in turn, its 8 numbers are fed back in, and the girths are measured on the output.
- The thesis reports an average correlation of **0.980** and an average mean difference of **0.623 cm** over the five girths.
- Per girth (correlation; mean difference): bust 0.973, 0.654 cm; under-bust 0.984, 0.585 cm; waist 0.989, 0.692 cm; hip 0.982, 0.618 cm; neck 0.974, 0.567 cm.
- Note (mine): this tests whether the **input girths are reproduced**. It does not compare the whole surface against the held-out scan.

**Stated limits and failure cases (S2b).**
- **F2-1** The interpolation is "rather sensitive" to wrong measurement input, especially with few examples. This is why the validity volume exists (ch. 5.3.6, PDF p.79). Some of the demonstration bodies use inputs outside the captured range (ch. 5.5, PDF p.83).
- **F2-2** Not every bone transform follows from a measurement. Driving bones directly from measurements "hardly guarantees" a realistic shape, which is why examples are interpolated instead (ch. 5 intro, PDF p.72).
- **F2-3** Pose-dependent shape comes only from the template's initial skinning, which needed many hand changes. The thesis calls for one framework covering pose and morphology (ch. 8.2, PDF p.119-120).
- **F2-4** The body vectors depend on the initial skinning weights. A strongly weighted upper arm needs only a small scale to look large (ch. 8.2, PDF p.120).
- **F2-5** Database size: the thesis cites a rule of thumb of 10-20 observations per variable (ch. 8.2, PDF p.119). Rebuilding the RBF system costs order n^3 (ch. 5.5.1, PDF p.88).
- **Unverified:** whether the I3D paper (S2) uses exactly the same 8 measurements and numbers. I could not read it.

## 3. SCAPE (S3)

**Split.** Every triangle's deformation is a product of three parts (PDF p.1, p.3-5):
- R: a rigid rotation of its body part (pose);
- Q: a non-rigid pose deformation (muscle bulging);
- S: a body-shape deformation.

The surface is rebuilt by least squares from the deformed triangles. The authors argue (PDF p.2) for working with triangle *deformations* rather than point displacements: displacements cannot be meaningfully multiplied, so pose displacements learned on a large person look wrong when added to a small person.

**Data.**
- Pose model: about 70 scans of one man in different poses (PDF p.3).
- Shape model: scans of 37 people in a similar pose (PDF p.3). The shape PCA section says it used "45 instances" (PDF p.5). **Unexplained difference:** 37 people against 45 instances. Some people may have more than one scan; the paper does not say.

**Skeleton.** Found automatically. The first result had 18 parts; the two crotch parts and two chest parts were then merged, leaving 16 (PDF p.3).

**Pose deformation (Q).** Each triangle's Q is predicted by **linear regression** from the rotations of its two nearest joints (PDF p.3-4). Joints with one or two real degrees of freedom are restricted, because allowing three can cause overfitting (PDF p.4).

**Shape space.** PCA over the per-triangle shape matrices S, with one model for both sexes (PDF p.5, Section 5.2). The authors found that shapes even three standard deviations out still look human (PDF p.5).

**First four components** (stated in the text, PDF p.5, Fig. 5): weight and height; gender; abdominal fat and chest muscles; bulkiness of the chest against the hips.

**Shape completion.** From markers alone (56 per frame in mocap), find pose and shape that fit, then optionally let the surface leave the model space to fit partial scans (PDF p.6-7).

**Stated limits and failure cases (S3, "Discussion and Limitations", PDF p.7-8).**
- **F3-1** Pose and shape are independent, so muscular people do not bulge more, and fat does not hide muscle (PDF p.7-8).
- **F3-2** Pose and shape spaces need differently structured training sets (one person in many poses; many people in one pose). Learning both from mixed data would reach an unpredictable local minimum (PDF p.8).
- **F3-3** Pose deformation is a linear regression on adjacent joints. This is good enough for completion but may not be enough for pure animation; non-linear regression is left for future work (PDF p.8).
- **F3-4** Only deformation driven by joints is modelled: no muscle tensing at a fixed angle, no faces, and no jiggle of fat in motion (PDF p.8).
- **F3-5** There is no pose prior, so occluded parts cannot be inferred from visible ones (PDF p.8).
- **F3-6** An armpit artefact came from hole-filling in the template (PDF p.4).
- **F3-7** Rebuilding requires a least-squares solve. S5 notes this cost and builds a simplified variant without it (S5 PDF p.6).

## 4. SMPL (S4)

**Model** (PDF p.4-5, Section 3, eqs. 5-10):
- The template has 6890 vertices and 23 joints. The same mesh topology is used for men and women.
- Pose is 72 numbers: 3 per joint plus 3 for the root.
- The rest shape is the template plus **shape blend shapes**, which are linear in the shape coefficients (orthonormal PCA directions), plus **pose blend shapes**, which are linear in the elements of the joint rotation matrices minus their rest values. There are 23x9 = 207 of these.
- Subtracting the rest rotations guarantees zero correction in the rest pose (PDF p.5).
- Joint centres are a **learned regression from the shaped rest surface**. The authors call accurate joints "critical", because wrong joints give artefacts when posed (PDF p.5). The regressor is sparse and non-negative, so each joint is a weighted mix of a few surface vertices (PDF p.7, Fig. 7).
- Standard LBS or DQS then poses the result. At most 4 parts influence a vertex (PDF p.4).

**Separating shape from pose** (PDF p.8, Section 4.2):
- Each training scan is first **pose-normalised**, by solving for the rest-pose shape that matches it once posed.
- PCA is then run on these rest shapes, in vertex space.
- The authors stress that without the pose step, pose variation would be captured inside the shape directions. This matches F1-3 above.
- They also argue that PCA on vertices is more principled than PCA on triangle deformations (as in SCAPE), because triangle deformations do not live in a Euclidean space.

**Data and sexes.**
- Separate **male and female** shape models (PDF p.2, p.8).
- Shape data: about 1700 male and 2100 female CAESAR registrations (PDF p.6).
- Pose data: 1786 registrations of 40 people, 20 of each sex (PDF p.6).

**How many numbers, how much variation** (PDF p.8, Fig. 9):
- Fig. 9 plots the cumulative relative variance of vertex positions in the CAESAR data against the number of shape coefficients, from 0 to 30.
- **(read off plot)** The male and female SMPL curves pass about 0.9 by roughly 3-5 coefficients and approach 1.0 by 30.
- The BlendSCAPE curves (triangle deformations) are much lower, roughly 0.65-0.7 at 30.
- The authors warn that the two curves measure different things (vertex variance against deformation variance), so they are not directly comparable.

**First components** (PDF p.8, Fig. 8; shown for the female model): PC1 and PC2 are shown at ±2 SD and PC3 at ±5 SD. **The text does not name them.**
- **(read off figure, mine)** PC1 changes overall size and stature together.
- PC2 runs from heavy to slim at a similar height.
- PC3 is a subtler change of proportion that I cannot name reliably.
- The joints move linearly with shape.

**Errors against measured bodies** (PDF p.9-10, Figs. 11-12):
- Mean absolute vertex error when fitting held-out registrations falls as shape coefficients are added.
- **(read off plot)** For SMPL it falls from about 6.5-7 mm with very few coefficients to about 3.2 mm at 300.
- Errors for fitting a known person in new poses lie between about 4.6 and 7 mm.
- The text says SMPL beats BlendSCAPE by about 0.5 mm, and that SMPL-LBS and SMPL-DQBS are "essentially identical".
- **Plain LBS** with no pose blend shapes is clearly worse (PDF p.10).

**Stated limits and failure cases (S4).**
- **F4-1** Pose corrections do not depend on body shape: the same correction is applied to every body (PDF p.14, "Limitations").
- **F4-2** Shape is static. There is no breathing, facial motion, muscle tension or soft-tissue dynamics; DMPL is a separate add-on (PDF p.12-14).
- **F4-3** Wrong joint centres give posing artefacts (PDF p.5).
- **F4-4** A sparse variant, where each joint only affects nearby vertices, is a little less accurate and sometimes shows slight discontinuities at the borders between joint influences (PDF p.10, Section 5.2).
- **F4-5** Without pose normalisation, the shape basis absorbs pose (PDF p.8).
- Plain LBS and DQBS give "serious artifacts" at the knees, elbows, shoulders and hips (PDF p.2, Fig. 2 caption). See R9-A3.

## 5. Substitute S5 (Pishchulin et al. 2017): sample size, error curves, first components

- **Model used:** a simplified SCAPE (S-SCAPE). PCA is run on vertex positions in one standard posture; pose is added afterwards by LBS on a 15-bone skeleton with 30 pose parameters. There is no least-squares rebuild, at the cost of losing pose-dependent bulging (PDF p.6-7, Section 2.2, eqs. 1-3).
- **Measures** (PDF p.16, Section 5.1):
  - *Generalisation:* leave-one-out fitting error in mm, against the number of components.
  - *Specificity:* distance from 10,000 random samples to their nearest training body.
- **Sample size** (PDF p.18, Section 5.3):
  - Error improves most between 50 and 100 training bodies.
  - 100 bodies generalise well but generate poorly.
  - 1,000 bodies give "a high-quality" space; going to 4,307 helps only slightly.
- **(read off plot, Fig. 3, PDF p.17)** Generalisation error is roughly 15-20 mm with 1 component and roughly 4-5 mm by 30.
- **Pose normalisation** before PCA makes the space more compact and the random samples more realistic (PDF p.18-19, Section 5.4).
- **First five components** (PDF p.19-20, Fig. 4, ±3 SD):
  - The older public space mixes posture (arm movement, tilting) into its main components.
  - The authors' space without pose normalisation still keeps small posture effects.
  - With pose normalisation, posture is "completely" factored out.
  - **(read off figure, mine)** In every row, component I is overall size and stature, and component II is slim against heavy (belly and chest).
- **Reconstruction from sparse data** (PDF p.21, Section 6.2): good accuracy with as few as 20 shape parameters, stable up to 50. A model learned from a narrower data set got worse as parameters were added, which the authors suspect is overfitting to noise.
- **Failure cases:**
  - **F5-1** Small or narrow training sets give poor random bodies (PDF p.18).
  - **F5-2** Posture contaminates the shape components unless it is normalised out (PDF p.19).
  - **F5-3** The simplified model loses muscle bulging with pose (PDF p.7).

## Where sources disagree (give both)

| Topic | View A | View B |
|---|---|---|
| One model or one per sex | SCAPE: one PCA; gender is its own component (S3 PDF p.5) | Allen and SMPL: separate male and female PCA (S1 PDF p.6; S4 PDF p.2, p.8) |
| What PCA is taken over | Per-triangle deformations (S3; BlendSCAPE in S4) | Vertex positions; S4 argues this is more principled (S4 PDF p.8) |
| What PC1 is | "weight and height" together (S3 PDF p.5, stated) | Overall size and stature, with weight in PC2 (S4 Fig. 8; S5 Fig. 4; both read off figures, mine) |
| Measurements to shape | Linear map with the cube root of weight (S1 PDF p.6) | Gaussian RBF over 8 normalised measurements, non-linear (S2b ch. 5.2-5.3) |
| SCAPE shape data size | 37 people (S3 PDF p.3) | 45 instances (S3 PDF p.5) |
| Seo thesis licence | CC BY (OpenAlex record) | "protected by copyright" (UNIGE cover, S2b PDF p.1) |

## Which numbers matter, and in what order (mine, synthesised; not stated by any one source)

1. **Overall size/stature first.** It is the first component in every figure that shows one (S3 stated; S4 and S5 read off figures). S2b also notes that stature has the largest spread of its 8 measurements (PDF p.78).
2. **Weight or girth at fixed height second:** slim against heavy, mostly belly and chest (S3 stated; S4 and S5 read off figures). S1 enters weight as its cube root.
3. **Sex.** It is either a component (S3) or a choice of model (S1, S4).
4. **Then trunk proportions:** abdominal fat and chest muscle, then chest bulk against hips (S3 PC3-PC4). Seo's girths (under-bust, waist, hip, neck) refine this region.
5. **(read off plot)** A few components carry most of the vertex variance in SMPL (about 90% by roughly 3-5). Errors in mm keep falling slowly out to tens or hundreds of components (S4 Fig. 11; S5 Fig. 3).

## For the lane (mine)

- **What carries over to the lane:**
  - keep **shape (identity) and pose separate** (S3, S4);
  - predict **joint centres from the surface or the measurements**, not as a separate input (S4 PDF p.5, p.7);
  - **normalise the measurement inputs and reject or clamp combinations outside the observed range**, for example an under-bust larger than the bust (S2b ch. 5.3.6).
- **Interpolating between measured levels.** Seo's split (a joint part for proportion, plus a smooth displacement field for detail) maps directly onto the lane's "distance field round the skeleton shaped by measured breadths, depths and girths". Measured girths set cross-sections at their levels, and a smooth interpolant (RBF in S2b) supplies the surface between the levels.
- **Failure checks to copy:**
  - F2-1 (out-of-range inputs);
  - F1-3, F4-5 and F5-2 (pose contaminating shape);
  - F3-1 and F4-1 (pose corrections that ignore body shape).
