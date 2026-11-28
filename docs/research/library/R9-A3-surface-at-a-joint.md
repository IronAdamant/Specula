# R9-A3 The surface at a joint: LBS, dual quaternions, implicit skinning

Round 9, section A. Access date for every source: **2026-11-28**. PDFs were downloaded to the box and read with poppler `pdftotext`. Pages are **PDF pages** of the copies named below. Everything is in my own words. **Method only:** the papers' shader and pseudo-code fragments were not copied. Derivations marked **(mine)** are my own algebra, checked by hand; they are not claims of the sources.

**Question this item answers for the lane.**
- Why does linear blend skinning (LBS) collapse an elbow and twist a wrist into a "candy wrapper", in mathematical terms?
- What does each remedy cost, and where does it still fail?
- How does implicit skinning keep volume and make contact, and what does it require of the field? (This ties to round 8 B1.)

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| S1 | Kavan L., Collins S., Žára J., O'Sullivan C. (2008). "Geometric skinning with approximate dual quaternion blending." *ACM TOG* 27(4), Article 105 (Crossref pages 1-23). doi:10.1145/1409625.1409627 | Author's copy: https://users.cs.utah.edu/~ladislav/kavan08geometric/kavan08geometric.pdf (23-page extended version with appendices) | ACM copyright; author-posted | Yes, PDF p.1-16 (body and the start of the appendices) |
| S2 | Vaillant R., Barthe L., Guennebaud G., Cani M.-P., Rohmer D., Wyvill B., Gourmel O., Paulin M. (2013). "Implicit skinning: real-time skin deformation with contact modeling." *ACM TOG* 32(4) (SIGGRAPH 2013), Article 125. doi:10.1145/2461912.2461960 | HAL hal-00819270: https://hal.science/hal-00819270 (file VBGCRWGP_SIGGRAPH_2013.pdf) | HAL licence "HAL Authorization" (HAL API `licence_s`); ACM copyright | Yes, all 12 PDF pages (PDF p.1 is the HAL cover; the paper is p.2-12) |
| S3 | Vaillant R., Guennebaud G., Barthe L., Wyvill B., Cani M.-P. (2014). "Robust iso-surface tracking for interactive character skinning." *ACM TOG* 33(6) (SIGGRAPH Asia 2014), pp. 1-11 (article number not confirmed). doi:10.1145/2661229.2661264 | HAL hal-01091820: https://inria.hal.science/hal-01091820 (licence page). **The HAL and IRIT downloads were truncated twice** (kept as .FAILED-truncated). The copy read was the first author's own: https://rodolphe-vaillant.fr/pivotr_media/entries/entry_059_elastic_implicit_skinning_a_robust_iso_surface_tracking_for_/elastic_implicit_skinning/elastic_implicit_skinning.pdf (11 pp.) | HAL Authorization (HAL API); ACM copyright | Yes, all 11 pages of the author copy. Whether it matches the HAL file byte for byte is **unverified** |
| (cross-ref) | Loper et al. 2015 (SMPL), see R9-A1 S4 | as R9-A1 | - | PDF p.2, p.10 used here |

## 1. LBS: definition and why it fails

**Definition** (S1 PDF p.3, Section 2.1). Each vertex is moved by every joint that influences it, and the results are averaged with that vertex's weights: v' = Σ w_i C_i v. Equivalently, the 4x4 matrices are averaged linearly before being applied.

**Why the average fails** (S1 PDF p.2-4):
- Points rotated about a centre lie on an arc, and their plain average lies **inside** the arc (S1 PDF p.2, Fig. 2). At the extreme it falls on the centre.
- A linear average of rotation matrices is generally **not a rotation**. It carries scale and shear, so the skin shrinks where weights are mixed.

**Candy wrapper, worked in S1** (PDF p.3-4, Figs. 4-5):
- Two joints (shoulder j1, elbow j2) and a vertex weighted 0.5/0.5.
- Twist j2 by 180° about the bone axis.
- The two transformed copies of the vertex are mirror images about the axis, so their average is **on the axis**: the skin collapses to a point at that cross-section.
- S1 also shows that rewriting the transforms about a different centre does not cure this (PDF p.4).

**The same idea as a closed form (mine).** Blend the identity and a rotation R(θ) about an axis with weights 0.5 each. A point at distance r from the axis maps to distance r·|cos(θ/2)| (the length of the average of two unit vectors θ apart).
- **Twist (candy wrapper):** θ about the bone's own axis. At θ = 180° the cross-section radius goes to 0. At 90° of twist it is about 0.71 r.
- **Bend (elbow collapse):** θ about an axis across the bone. Vertices near the joint with mixed weights are pulled toward the bend's centre by the same cos(θ/2) factor. Flesh on the outside of the elbow sinks and the joint looks pinched or thin. At a 90° bend the factor is again about 0.71.
- With unequal weights w and 1-w the factor is √(w² + (1-w)² + 2w(1-w)cos θ), which is smallest at w = 0.5. **(mine)**

**Documented in the sources:**
- the candy wrapper (S1 PDF p.4, Fig. 4; S1 Fig. 14 left on a real model, PDF p.14);
- loss of volume on bending (S2 PDF p.2: plain blending does not keep volume and can self-penetrate; S2 PDF p.8, Fig. 10: "visible loss of volume" on a break-dance pose);
- the "elbow collapse" (S2 PDF p.5, p.9);
- SMPL: plain LBS and DQBS give "serious artifacts" at the knees, elbows, shoulders and hips (R9-A1 S4 PDF p.2).

**What LBS has going for it:**
- It is the cheapest method.
- It supports non-rigid joint transforms (scale and shear) at no extra cost (S1 PDF p.15).
- Its smoothing near a joint can be *right* for boneless, smoothly bending parts such as a trunk or a long neck (S2 PDF p.10, "Smooth skin parts").

## 2. Dual quaternion skinning (DQS / "DLB")

**Method** (S1 Sections 3-4, PDF p.2, p.9-12):
1. Write each joint's rigid transform as a unit dual quaternion.
2. Blend the dual quaternions linearly with the vertex weights.
3. **Normalise** the result, giving a unit dual quaternion, which is always a rigid motion.
4. Apply it to the vertex (and the normal).

**Properties:**
- It is closed-form and **coordinate-invariant** (bi-invariant), so the error cannot depend on where the frames are placed (S1 PDF p.9-10).
- It is an approximation to the ideal two-transform interpolation (screw linear interpolation, ScLERP). Against ScLERP, the rotation angle differs by **less than 8.15°** and the translation by **less than 15.1%**. These are upper bounds, and usually much smaller in practice (S1 PDF p.10-11).
- For more than two transforms the ideal reference itself is not settled (S1 PDF p.11).

**What it fixes:** the blend is always rigid, so there is **no skin collapse** and no candy wrapper (S1 PDF p.14, Fig. 14).

**Costs:**
- Speed is near LBS: "slightly slower" than linear and direct quaternion blending, and over twice as fast as log-matrix or spherical blending (S1 PDF p.14, Figs. 18-19).
- **Non-rigid joints** (scale and shear, often used to fake muscle bulge) cannot be held in a dual quaternion. S1 adds a two-phase scheme: scale in the rest pose, then blend rigidly. This needs extra matrices and about **twice LBS's shader instruction count** (S1 PDF p.12-13, p.15).
- **Sign choice (antipodality).** q and -q give the same motion but blend differently, so signs must be fixed per vertex. S1's cheap pre-computed choice assumes every joint rotates less than 180° relative to its parent (S1 PDF p.12).

**Failure cases (DQS):**
- **D1 Flipping.** It always takes the shortest path, so rotating a joint past 180° makes the skin jump discontinuously (179° to 181°, S1 PDF p.15-16, Fig. 20). The authors say joint limits usually prevent this.
- **D2 Bulging ("overgrowth") at bends.** S2 shows dual quaternions producing excess bulge at the knee and elbow and smoothing the shoulder; implicit skinning corrects it (S2 PDF p.8, Fig. 10; PDF p.9, Fig. 12). S1 itself does not discuss this bulge.
  - **Sources differ in emphasis:** S1 presents DQS as artefact-free in its tests (abstract, PDF p.1). S2 and SMPL (R9-A1) show bend artefacts remaining.
- **D3 No contact.** DQS still lets the inner skin of a fold pass through itself (S2 PDF p.9, Fig. 12(b,c)).
- **D4** Non-rigid joints need the two-phase scheme above.

## 3. Implicit skinning (S2, 2013)

**Set-up** (S2 PDF p.3-5, Sections 3-4.1):
- **Input:** a mesh with a skeleton, skinning weights, and a **partition of the mesh by bone**. S2 computes the weights by heat diffusion (Baran & Popović); the partition can be made by an artist or automatically (PDF p.4).
- **One field per bone.** Fit a **Hermite RBF** (positions plus normals) to that bone's piece of the mesh.
  - Add **closure points** on the bone axis at each end, at a distance from the joint equal to the joint's distance to the nearest vertex, with normals pointing out along the bone. Without them, neighbouring pieces overlap at the joint and make bumps (PDF p.4-5, Figs. 3-4).
  - The HRBF value is then mapped smoothly into [0,1]: 1 inside, **0.5 on the surface**, 0 outside, with **compact support** of radius r. r is set to the distance from the bone to the farthest sample, so the value 1 is reached only on the bone itself (PDF p.4-5).
- **Requirements on the field** (PDF p.4):
  1. **high smoothness**, to keep the gradient-controlled operators well behaved and the projection stable;
  2. it must **close the holes** left by cutting the mesh at joints;
  3. **compact support**, for local composition and fast evaluation.
- **Details are kept by iso-value, not by an exact fit.** Each vertex stores its own rest-pose field value. The mesh is *embedded* in the field rather than lying on the 0.5 surface, so small details survive a smooth field (PDF p.4).

**Composition** (S2 PDF p.5-6, Section 4.2):
- Pairs of bone fields at each joint are combined with a binary operator, and the results are joined with max.
- **Union (max):** produces a crease or fold when the joint bends and **avoids the elbow collapse** (PDF p.5).
- Classic smooth blends (sum, bulge-in-contact) would create unwanted bulges even with the bone straight, so **gradient-based operators** are used. They switch between union and blend according to the angle between the two fields' gradients, so there is no blending when the bones are aligned (PDF p.5-6, Figs. 5-6).
- S2 modifies the earlier gradient blend operator, because it distorted the field *inside* the shape, which matters for projection (PDF p.6, Fig. 6).
- The **bulge-in-contact** operator inflates the tissue round a contact (PDF p.6; Fig. 1(d)).

**Per-frame correction** (S2 PDF p.7, Section 5):
1. **Start** from a geometric skinning result. Dual quaternions are used in S2.
2. **Project** each vertex back to its own rest iso-value by Newton steps along the gradient, with step factor σ = 0.35 (PDF p.7).
3. **Stop at contact.** If the gradient direction turns by more than **55°** between two iterations, the vertex is taken to be on a contact surface (a gradient discontinuity) and stops there. This is what produces the contact between two skin parts instead of interpenetration (PDF p.7, Fig. 8).
4. **Tangential relaxation**, interleaved after every two projection steps. Each vertex moves toward a weighted centroid of its neighbours, using mean-value coordinates precomputed in the rest pose. The strength falls off as the vertex nears its target iso-value (PDF p.7).
5. **Local Laplacian smoothing** only around contact regions, to soften the sharp crease that the union operator leaves (PDF p.7, Fig. 9).

**How it "keeps volume" and "makes contact" (summary, from S2 PDF p.2, p.7, p.10).**
- Volume is not conserved as a number. The skin is pushed back to the iso-surfaces of rigidly moving per-bone fields, so each limb keeps its rest cross-section shape near the joint instead of shrinking.
- Contact is where the composed field's gradient breaks, and the projection stops there.

**Performance** (S2 PDF p.7-8, Table 1):
- Per-bone fields are stored in 32³ grids and operators in 128³ grids; models with 20-70 bones need 10-35 MB.
- More than 60 fps for models of thousands of vertices and more than 12 fps for tens of thousands of vertices, with most bones moving.
- Several fps for over 170,000 vertices (GTX 680 GPU).

**Failure cases (S2, "Discussion and limitations", PDF p.9-10):**
- **I1 Depends on the initial skinning.** It works better when bone influences are smooth. Deep self-intersections in the initial pose (typical of LBS) push vertices **past the bone**, and they are projected onto the **opposite** surface. The paper calls this a failure of the method (Fig. 14: LBS start fails where a DQS start works).
- **I2 Extreme poses.** Some vertices cross a contact and are stopped there when they should have reached the visible skin outside the fold.
- **I3 Smooth boneless bends** (giraffe neck, trunk, tentacle) are not handled. LBS-like smoothing may be closer to what is wanted.
- **I4 No memory between frames.** Each frame starts again from the rest pose. This can flicker unless the mesh is dense enough at the joints. It also makes the method easy to run frames in parallel.
- **I5 Coarse meshes.** Too few vertices on the contact, blend or bulge features gives poor, time-discontinuous results.
- **I6** Only binary operators exist. Contact between non-neighbouring parts (an arm against the belly) uses plain union; n-ary operators are future work (PDF p.10).

## 4. Robust iso-surface tracking (S3, 2014)

**What changes** (S3 PDF p.1-3, p.6-7):
- The mesh **tracks the iso-surface over time** instead of restarting each frame from geometric skinning.
- **Three stages per step** (PDF p.6):
  1. move each vertex by the change in its **nearest bone's** transform since the last step;
  2. Newton-project it to its rest iso-value;
  3. **tangential relaxation** derived from the as-rigid-as-possible (ARAP) energy.
- **The ARAP rotations are fixed in advance.** They are blended bone rotations (dual quaternions with harmonic weights), depending only on the rest pose and the bone transforms, not on the current vertices. This keeps the energy linear, smooth across region borders, and able to return to the rest pose. Computing the rotations from the deformation instead (polar decomposition) is costlier, non-linear, only C0 at borders, and makes the skin appear to slide (PDF p.6-7, Figs. 7-8).
- Each iteration keeps vertices on the surface by restricting them to their current tangent plane (PDF p.7).
- **New composition operators** that create a real contact surface inside the 0.5 iso-surface:
  - a contact operator g_dc runs from clean union (d_c = 0) to full contact (d_c = 1);
  - a bulge-in-contact operator g_db adds inflation.
  - Both are built as grids by **biharmonic interpolation** of a profile curve at value 0.5 (PDF p.3-6, Figs. 5-6).
  - Per S3, the earlier gradient operators gave the right outer shape but not a contact surface; vertices merely stopped at gradient discontinuities, which was unstable (PDF p.2-3).

**Gains claimed:**
- Bends of about 150° without artefacts (PDF p.8, Fig. 1(e)).
- Deep interpenetrations (arm into belly) are handled.
- Skin "elasticity": the skin stretches across the belly when the torso twists, and relaxation can be painted off by region (PDF p.8, Figs. 1, 11, 14).
- Insensitive to the skinning weights, which only set the target rotations (PDF p.8, Fig. 8).
- An extra field primitive can make a biceps bulge with bending (PDF p.8, Fig. 10).
- 30-300 fps when one joint moves (PDF p.7).

**Failure cases (S3, Conclusion, PDF p.9):**
- **T1 History dependence.** The result for a pose is no longer determined by the pose alone. It can depend on the whole animation history and on the **time step**.
- **T2 Time-step trade-off.** Large steps give poor initial guesses (the implicit-skinning failures come back); tiny steps are expensive. The authors' automatic step choice is conservative; a straight arm to 90° in one step still worked.
- **T3** Automatically building the fields at **complex joints** is still an open problem.
- **T4 (inherited from S2)** Weights that are not smooth enough make plain implicit-skinning projection fail; S3 says this is why it needs extra weight editing (PDF p.8).

## 5. Comparison: what each remedy costs and where it still fails

| Method | Candy wrapper | Elbow collapse / volume loss | Bulge at bend | Contact in fold | Cost | Still fails |
|---|---|---|---|---|---|---|
| LBS | Yes (S1 p.3-4) | Yes (S2 p.5, p.8) | No | No; self-intersects | Cheapest; scale and shear free | Everywhere weights mix at large angles |
| DQS | No (S1 p.14) | No collapse | **Yes, overgrowth** (S2 p.8-9) | No | About LBS; about 2x with scale (S1 p.14-15) | >180° flip (S1 p.15-16); signs; bend bulge |
| Pose-corrective blend shapes (SMPL, R9-A1) | Corrected by learned offsets | Corrected | Corrected | Not modelled | Many learned offsets (207 x 3 x 6890) | Offsets ignore body shape (A1 F4-1); need data |
| Implicit skinning (S2) | Corrected (S2 p.9, Fig. 13) | Corrected (union) | Optional bulge operator | **Yes** (stop at gradient break) | GPU, 10-35 MB fields; >12 fps at tens of thousands of vertices | Deep initial intersections; extreme poses; boneless bends; flicker |
| Tracking (S3) | Corrected | Corrected | Operator | **Yes**, deep contacts | Iterative; depends on step | History and time-step dependence; complex-joint fields |

## For the lane (mine)

- **Field requirements map straight onto the round 8 B1 field.** If the lane already has one distance field round the skeleton, then S2's three needs apply directly:
  - a **smooth** field;
  - **closed** at joint cuts (S2 uses closure points on the bone axis);
  - **compact support** per bone.
  - Keep a separate field per bone (or per segment) so that they can be **combined at each joint** with union, blend or contact operators. One single summed field cannot give a crease on the inside of a bend and a smooth outside.
- **For a static figure (no animation)**, history (T1) and flicker (I4) do not matter. The relevant parts are the **composition operator at each joint** and the **0.5 iso-value with per-vertex offsets** used to keep detail.
- **Do not start a projection from plain LBS on deep bends** (I1). Start from DQS, or from the rest pose moved by the nearest bone (S3 stage 1).
- **Expected look of each method at a 90° elbow:**
  - LBS pinches to about 0.71 of the radius on mixed-weight rings **(mine)**;
  - DQS keeps the radius but overgrows;
  - union of rigid per-bone fields gives a crease with a contact surface.
