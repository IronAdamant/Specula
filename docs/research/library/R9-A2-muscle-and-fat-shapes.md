# R9-A2 Muscles and fat as shapes on a skeleton

Round 9, section A. Access date for every source: **2026-11-28**. PDFs were downloaded to the box and read with poppler `pdftotext`; tables were checked against the page layout. Pages are **PDF pages**, with printed pages where the copy has them. Everything is in my own words. **Method only:** no code (the dissertation contains code listings in the AL language; none was taken) and no data set was used. The fat-thickness figures quoted are published summary values from a CC BY article and from an openly readable journal article, attributed row by row. Plot readings are marked **(read off plot)**, and my own derivations and lane notes are marked **(mine)**.

**Question this item answers for the lane.**
- What shapes stand in for a muscle belly and a tendon, and how does a cross-section change with joint angle?
- How is skin laid over the muscles?
- Which muscles cannot be left out (shoulder, chest, buttock, calf)?
- What open account is there of subcutaneous fat by site and sex, and what turns site values into a surface?

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| S1 | Scheepers F., Parent R.E., Carlson W.E., May S.F. (1997). "Anatomy-based modeling of the human musculature." *Proc. SIGGRAPH '97*, pp. 163-172. doi:10.1145/258734.258827 | **None found.** The ACM PDF returned a bot-check page. A copy exists on a personal site (chrisevans3d.com), but its legality is unclear, so it was **not used** | ACM copyright | **No** (see S1b) |
| S1b | **Labelled substitute for S1, same first author, the work the paper reports:** Scheepers C.F. (1996). *Anatomy-based surface generation for articulated models of human figures.* PhD dissertation, The Ohio State University. OhioLINK ETD accession osu1487935125879411 | https://etd.ohiolink.edu/acprod/odb_etd/r/etd/search/10?p10_accession_num=osu1487935125879411 (free PDF, 334 pages, a UMI microfilm scan with OCR text) | The OhioLINK page says it is provided by OSU "for research and educational purposes, and may be under copyright by the author", "All Rights Reserved". Free to read, not to copy | Yes: ch. III 3.4-3.5 (PDF p.85-96), ch. V (PDF p.140-198), ch. VI (PDF p.198-223), ch. VII (PDF p.225-236). **Printed page = PDF page minus 25** (for example PDF p.141 = p.116) |
| S2 | Wilhelms J., Van Gelder A. (1997). "Anatomically based modeling." *Proc. SIGGRAPH '97*, pp. 173-180. doi:10.1145/258734.258833. Tech-report version: UCSC-CRL-97-10, 15 April 1997 | Legal free copy: UCSC tech report https://tr.soe.ucsc.edu/sites/default/files/technical-reports/UCSC-CRL-97-10.pdf . **It was unreachable from the box on 2026-11-28** (timeouts), so it was not read. What was read is the 8-page conference text from the Stanford course reading list: https://www-graphics.stanford.edu/courses/cs448-01-spring/papers/wilhelms.pdf (internal title "fauna_sig.dvi", i.e. the authors' own build) | ACM copyright for the paper; the course copy's permission is unstated | Yes, all 8 pages of the Stanford copy. Whether the TR text differs is **unverified** |
| S3 | Kruschitz R., Wallner-Liebmann S.J., Hamlin M.J., Moser M., Ludvik B., Schnedl W.J., Tafeit E. (2013). "Detecting body fat: a weighty problem. BMI versus subcutaneous fat patterns in athletes and non-athletes." *PLoS ONE* 8(8): e72002. doi:10.1371/journal.pone.0072002 | https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0072002 | **CC BY 4.0** (Crossref; the article's copyright line says Creative Commons Attribution) | Yes, all 9 pages |
| S4 | Jürimäe T., Jürimäe J., Wallner S.J., Lipp R.W., Schnedl W.J., Möller R., Tafeit E. (2007). "Relationships between body fat measured by DXA and subcutaneous adipose tissue thickness measured by Lipometer in adults." *J. Physiol. Anthropol.* 26(4): 513-516. doi:10.2114/jpa2.26.513 | https://www.jstage.jst.go.jp/article/jpa2/26/4/26_4_513/_pdf | Free on J-STAGE; **no licence statement found** in the PDF or in Crossref | Yes, all 4 pages |
| (S5) | Möller R. et al. (2000). Lipometer SAT-Top method and site definitions, *Am. J. Hum. Biol.* 12(2): 231-239 | Wiley; no free copy looked for further | Publisher copyright | **No.** The site definitions in it are therefore **unverified** here |

## 1. Muscle shapes (S1b; S2)

**S1b: three belly primitives, each set by length, height and width** (ch. V 5.2, PDF p.142-145 = p.117-120, Fig. 32):
- **Fusiform belly = ellipsoid.** To bulge naturally, it keeps its **volume and its height-to-width ratio** constant. When the belly's length changes (set by origin and insertion as the joint turns), the cross-section radii are recomputed from the fixed volume. Shortening therefore fattens the belly and stretching thins it (PDF p.143-144, eqs. 5.1-5.4, Fig. 33). The dissertation credits the ellipsoid idea to Wilhelms.
- **Parallel belly = elliptic cylinder capped by two elliptic half-ellipsoids.** Used for sheet-like and strap-like muscles, because an ellipsoid tapers too much at the ends (PDF p.144-145).
- **Triangular belly** ("tapering" or "expanding") = a truncated elliptic cylinder with caps (Fig. 32, PDF p.143).
- **Isometric tension** (ch. V 5.3.1.2, PDF p.151-152, Fig. 39): a tension value t between 0 and 1 changes the height-to-width ratio (taller and narrower) at constant volume. A muscle can then bulge with no joint motion, controlled by a gain k (the code fragment shows 1.6, PDF p.155).

**S1b: muscle models built from those bellies.**
- **Simple model:** one belly between point origin and insertion, with optional tendons (5.3.1, PDF p.149-155).
- **Multi-belly model:** origins and insertions are space curves, and several bellies are placed along them automatically. This covers wide muscles (5.3.2, PDF p.156-159).
- **General model:** a tube of bicubic patches swept along a curve and capped. It handles muscles that bend round other structures, that a straight belly cannot fit, or that attach by flat tendons (5.3.3, PDF p.159-165). Its volume is approximated by three truncated elliptic cones, ignoring the caps (PDF p.164, footnote).
- **Tendons** do not change length or width but can bend. A tendon moves the effective end of the deformable belly and so changes how the muscle deforms (ch. V intro, PDF p.141). Straight tendons are ellipsoids or cylinders; bending ones are **generalised cylinders**, i.e. an ellipse swept along a spline (5.4, PDF p.167-172).

**S2 (Wilhelms & Van Gelder): deformed discretised cylinders** (Section 4, PDF p.2-4).
- A default muscle is defined entirely by **two origins and two insertions** on bones. These are stored relative to the bone's bounding box so that they carry over to another individual (PDF p.2).
- The body of the muscle is **7 sections between 8 elliptical slices**, each slice polygonised (8 points by default). There are no explicit tendons (PDF p.2-3).
- End slices sit between the two origins and between the two insertions. Middle slices are spaced along the line and pushed slightly outward from the bone, which curves the path. Their X and Y are scaled to give a fusiform belly that is wider than it is thick (Section 4.2, PDF p.3).
- **Non-default shapes:** per-slice width and thickness scales, slice moves and rotations, and an optional **pivot** frame the path bends round (for example the quadriceps over the knee). Table 1 lists the user parameters (Section 4.3, PDF p.3-4). Pectoralis major is shown as flat, triangular and arched round the ribs (Fig. 4A).
- **Animation** (Section 4.4, PDF p.4): when a spanned joint moves, rebuild the default shape and apply the user edits. Then scale the width and thickness of the inner slices by rest length divided by present length. This is approximate volume keeping: exact between parallel slices, second-order error between non-parallel ones, and no change at the end slices.
- Earlier ellipsoid muscles were enough only for simple fusiform muscles with one origin and one insertion. In the monkey model only the lower-arm muscles are ellipsoids (PDF p.2).

**How a cross-section changes with joint angle: the sources compared.**
- In both sources the cross-section scales from the change in muscle length (origin to insertion distance, through any pivot).
- In S1b the ellipse **area** goes inversely with length (volume fixed) and its **shape** stays fixed unless tension changes it.
- In S2 **width and thickness** each scale by rest length over present length. The area then goes as (rest/present)^2, so **S2's rule does not keep volume** as stated: (rest/present)^2 times present length is not constant **(mine, from the stated rule; the paper itself says volume is preserved "exactly" between parallel slices)**.
- **This is a disagreement to note.** Scaling both axes by rest/present multiplies area by (rest/present)^2, so the volume of a section of length L goes as 1/L. Exact preservation would need each axis scaled by the square root of rest/present. Either the paper's "length" is per-section in a way that changes this, or the claim is loose. **Unresolved; flagged.**

## 2. Laying skin over muscle (S1b; S2)

**S2 (volumetric skin, then a bound elastic mesh)** (Section 5, PDF p.4-7):
1. In the rest pose, voxelise the space around the body. The density is positive inside any part and zero outside; some parts are given negative density to carve hollows (eye sockets).
2. Blur with a Gaussian whose width the user sets, then extract an iso-surface at a user threshold.
   - Settings reported: grid up to 225 cells, inside value 200, Gaussian SD 3.2, threshold 35.
   - Result: about 75,000 skin vertices and 150,000 triangles (PDF p.4, p.7).
3. **Anchor** each skin vertex to its nearest component and store its rest position in that component's local frame (the "virtual anchor").
   - For muscles, the position is stored as (s,t,u) in a tri-affine cell between two adjacent slices. Inverting the cell needs a Newton solve, once.
   - The paper says that mapping into a single slice frame instead gives bumps, because the mapping jumps from slice to slice (Section 5.1, PDF p.4-5, Fig. 6).
4. Each frame, map the anchors forward, then **relax** a spring mesh (Section 5.2, PDF p.5-6):
   - Edge stiffness is set from the two adjacent triangle areas over the squared length. Uniform stiffness made the skin look unevenly stretched.
   - Each vertex is tied to its anchor by a zero-length spring. Anchor stiffness scales with the vertex's triangle area times a constant; 0.10 lets the skin slide.
   - Springs are weaker in compression (typically a factor of 0.1), and rest lengths are a little shorter than extracted (for example 90%), so the skin sits slightly stretched.
   - A sphere under each anchor stops the skin sinking in.
   - About 30 iterations gave near-equilibrium; 5-10 were often acceptable (PDF p.7).
5. **Reuse across bodies by scaling** (Section 5.3, PDF p.6-7): components are sized relative to each segment's bounding box. Extremities can be enlarged before voxelising for detail and the skin reused at normal size. Moving the anchors outward thickens the skin, which makes a plumper body (Fig. 9).

**S1b (swept patch skin pushed out by implicit primitives)** (ch. VI 6.2-6.4, PDF p.199-217):
1. Fields are **implicit primitives** with a cubic fall-off of finite radius R. Contributions add, and the surface is an iso-value of the sum (6.2, PDF p.199-200, eq. 6.1).
2. **Initial skin:** sweep an elliptic profile along a spline through the joint positions, as a bicubic patch tube. The width and height of the profile can be set at each path point. The density of the control hull is chosen separately from the number of path points (6.4.4.1, PDF p.212-214).
3. **Adjust:** move each control point outward along its own direction until the summed field of the implicit versions of bones, muscles, tendons and fat pads falls to zero.
   - Blending between primitives carries over into the skin.
   - The radius R sets how far the skin stands off, which is the "filler fat" (6.4.1, PDF p.204-205; 6.4.4.2, PDF p.216).
4. **Skin folds:** pin control points on the flexion side of a joint so that they are not pushed (PDF p.216).
5. Fat pads (Section 3 below) enter as more implicit primitives.
6. Animation tests repeated the elbow-flexion sequence at several values of R (6.4.5, PDF p.217).

## 3. Which muscles cannot be left out

The dissertation's **Table 4** (ch. III, PDF p.88 = p.63) lists superficial muscles that create or influence surface form. Paraphrasing the regional groups on PDF p.86-88:
- Neck: sternocleidomastoid among others.
- Trunk: pectoralis major, latissimus dorsi, trapezius, serratus anterior.
- Shoulder: **deltoid**, with supraspinatus, infraspinatus and the teres muscles.
- Hip: **gluteus maximus**, gluteus medius, tensor fasciae latae.
- Leg: **gastrocnemius**, soleus, tibialis anterior and others.

The dissertation's point (PDF p.85; ch. VII, PDF p.227):
- superficial muscles make surface form directly;
- deeper muscles affect it only in some postures.

The brief's four regions therefore map to **deltoid; pectoralis major; gluteus maximus; gastrocnemius (with soleus)** **(mine, matched against Table 4)**.

The models that handle these muscles (S1b ch. V):
- Wide-attachment muscles (latissimus dorsi, trapezius) **cannot** be done accurately with the point-to-point model; they need the multi-belly or general model (PDF p.150, p.159-160).
- S2 models pectoralis major as a flat, triangular, arched shape (Fig. 4A, PDF p.3).

## 4. Subcutaneous fat by site and sex

**What a fat layer is, and where the pads are (S1b ch. III 3.5, PDF p.91, p.95-96).**
- Fat forms a continuous layer under the skin. It is thicker on the trunk than on the limbs and differs by person and by sex; women have a higher fat percentage and differently placed deposits (PDF p.91).
- **Major fat pads** sit in the same places in both sexes but differ in thickness and extent (PDF p.91).
- **Table 5 (PDF p.96)** lists the major pads, with the sex notes it gives. My paraphrase:
  - cervico-dorsal (over the trapezius at the vertebra prominens; usually women and the elderly);
  - post-deltoid (back of the upper arm; especially women; gives the female arm contour);
  - axillary;
  - breast;
  - pectoral (men; squares off the lower chest);
  - flank (masks the iliac crest; more evident in women);
  - abdominal (around and below the navel in women);
  - pre-pubic;
  - **gluteal** (sets buttock size and form, often hiding gluteus maximus; more in women);
  - subtrochanteric (moves the female's greatest girth below the trochanters);
  - patellar;
  - popliteal.
- Except for the skeleton, fat is the main cause of male/female differences in form. The female "high-waisted" outline comes from the lateral and posterior pelvic pads (PDF p.95).
- In lean people, the lack of fat, not extra muscle, is what sharpens surface definition (PDF p.95).

**Site thicknesses (Lipometer optical device, 15 standard sites, right side, standing).** The fifteen sites are: neck, triceps, biceps, upper back, front chest, lateral chest, upper abdomen, lower abdomen, lower back, hip, front thigh, lateral thigh, rear thigh, inner thigh and calf (S3 p.2-3 and p.5; S4 p.2).
- S3 reports a calibration against CT with r = 0.99, and repeatability CVs from 1.9% (front chest) to 12.2% (rear thigh) (S3 p.5).
- S3 site location: right side, standing (p.5). The exact landmarks are in S5 and S3's Figure S1, **not read: unverified**.

Values in mm (S3 Tables 1-2, PDF p.2-3, **median (Q1-Q3)**, young non-athletic controls aged about 15-30; S4 Table 1, PDF p.2, **mean ± SD**, adults aged 22-62):

| Site | S3 men, non-athletes (n=32) | S3 women, non-athletes (n=21) | S4 men (n=28) | S4 women (n=53) |
|---|---|---|---|---|
| Neck | 3.7 (2.5-6.2) | 5.8 (4.0-7.6) | 4.6 ± 4.3 | 9.6 ± 5.9 |
| Triceps | 4.9 (3.5-6.5) | 12.5 (9.9-14.3) | 4.7 ± 2.8 | 15.1 ± 6.2 |
| Biceps | 3.0 (2.1-3.7) | 5.3 (4.0-7.6) | 2.1 ± 1.7 | 9.9 ± 4.9 |
| Upper back | 3.6 (2.5-4.8) | 4.9 (3.7-6.3) | 3.4 ± 2.8 | 9.9 ± 5.8 |
| Front chest | 3.8 (2.8-5.7) | 8.6 (4.4-10.3) | 4.8 ± 4.1 | 11.1 ± 6.9 |
| Lateral chest | 4.2 (2.7-5.9) | 6.3 (4.6-10.0) | 4.4 ± 4.1 | 14.2 ± 9.6 |
| Upper abdomen | 5.4 (3.5-8.0) | 7.4 (5.5-10.7) | 7.0 ± 6.4 | 13.4 ± 6.9 |
| Lower abdomen | 5.6 (3.6-9.1) | 10.2 (6.0-12.9) | 6.2 ± 5.0 | 11.0 ± 6.6 |
| Lower back | 6.4 (3.8-8.7) | 11.4 (8.6-13.8) | 6.1 ± 3.4 | 12.0 ± 5.5 |
| Hip | 6.3 (4.5-8.8) | 8.5 (5.4-12.0) | 6.0 ± 4.6 | 13.4 ± 6.6 |
| Front thigh | 3.2 (2.5-4.8) | 10.3 (8.0-11.5) | 2.6 ± 1.6 | 8.2 ± 3.2 |
| Lateral thigh | 4.0 (3.1-5.5) | 10.4 (9.9-12.7) | 2.3 ± 1.3 | 8.6 ± 3.0 |
| Rear thigh | 3.5 (2.4-4.8) | 7.2 (6.1-8.0) | 1.9 ± 1.2 | 6.0 ± 3.0 |
| Inner thigh | 4.9 (3.8-7.8) | 11.2 (9.8-12.6) | 4.2 ± 3.0 | 10.1 ± 3.6 |
| Calf | 3.0 (2.2-3.9) | 6.3 (4.8-7.1) | 1.4 ± 0.9 | 3.8 ± 1.9 |

Notes on the table:
- The S3 values are copied from a CC BY 4.0 article, with attribution as above.
- In the PDF text layer, "±" renders as "6", so "5.863.6" means 5.8 ± 3.6. S3's tables state "median ± interquartile range (Q1-Q3)"; I give the median and the Q1-Q3 range. S4's "X̄SD" pairs were split as mean and SD; each value has one decimal place, so the split is unambiguous.
- Context: S3 male non-athletes were matched to the athletes on BMI (median 22.4; S3 p.2). S4 women had a mean BMI of 27.7 and DXA fat of 34.0%, against 24.8 and 15.3% for the men (S4 p.2).

**Where the two fat sources disagree, or differ in kind.**
- *Thickest site in men:* the lower back (6.4 mm) and hip (6.3 mm) in S3's young controls, against the upper abdomen (7.0 mm) in S4's older adults.
- *Thickest sites in women:* triceps (12.5 mm) and lower back (11.4 mm) in S3, against triceps (15.1 mm) and lateral chest (14.2 mm) in S4.
- S3 reports medians and S4 means, and the age groups differ. These are differences of sample, not necessarily contradictions. **Both are given.**
- *Which sites best predict whole-body fat (S4 p.1-3):* the upper back and inner thigh in men (stepwise regression, 87.1% of the variance of DXA body-fat %), and the lateral chest, biceps and calf in women (78.5%).
- S4 itself notes that its own earlier study in Estonians found **different** best predictors: the neck and hip in men, and the calf and hip in women (S4 p.3). The choice of predictor sites is therefore sample-dependent.
- *Which sites best separate athletes from non-athletes (S3 abstract, p.1):* in men the neck (cut-off 2.3 mm) and the trunk compartment (15.5 mm); in women the upper back (3.3 mm) and the arm compartment (15.9 mm).
- **Sex differences:** in S4, women were thicker at every site (p < 0.001 throughout, Table 1). In S3, the female controls' medians exceed the male controls' at every site (read from Tables 1-2; S3 did not test sex differences directly).

**What joins the sites into a surface: no open source found that does it.**
- None of S1b-S4 gives a continuous thickness map interpolated between sites. S3 and S4 measure 15 points only.
- The nearest method is S1b's: fat pads as **flattened ellipsoids** of given volume and flatness, placed relative to bones, plus **filler fat** as the implicit radius R that pushes the skin off the muscles (ch. VI 6.3-6.4, PDF p.201-205).
- Such a map remains **missing**: see the blocked-downloads list. Lipometer SAT-Top papers from the Graz group may hold denser maps; not found open in this round.

## 5. Stated limits and failure cases

**S1b (Scheepers).**
- **F1** The point-to-point muscle cannot represent wide attachments (latissimus dorsi, trapezius) (PDF p.150). Muscles that bend round structures (brachioradialis), that one straight belly cannot fit (infraspinatus), or that have flat tendons (trapezius) need the general model (PDF p.159-160). Many independent bellies may be anatomically wrong for a muscle that has no separate bellies (PDF p.160).
- **F2** The general model's volume is approximate (cones; caps ignored) (PDF p.164).
- **F3** Branch points of the skeleton (hand webs between fingers) are not handled automatically by the swept skin (PDF p.216; ch. VII, PDF p.235-236).
- **F4** Nothing stops non-adjacent segments interpenetrating; there is no collision handling (ch. VII, PDF p.235).
- **F5** Muscles were built by hand. Deriving origin, insertion and volume automatically is future work (PDF p.235).
- **F6** Only the upper limb was built. The claim that the method extends "straightforwardly" to the rest of the body is the author's, **untested in the dissertation** (ch. VII, PDF p.226).
- **F7** Implicit surfaces must be re-polygonised whenever the field changes, and ray-casting them is slow. This is why S1b moves patch control points instead of rendering the field (PDF p.202). S1b calls the volumetric skin step in S2 a "major limitation" (PDF p.204).

**S2 (Wilhelms & Van Gelder).**
- **F8** Volume is kept only approximately (Section 4.4, PDF p.4). See the disagreement noted in Section 1.
- **F9** The skin handles collision only with the tissue under it. When a joint folds, skin from the two sides interpenetrates, hidden in the fold. Skin pressing on another body part is not modelled (PDF p.6).
- **F10** Components interpenetrate. Generalised tissue does not deform. The authors give "problems in certain extreme positions" with no detail (PDF p.7).
- **F11** Making the muscles was tedious trial and error (PDF p.7).
- **F12** Re-extracting the skin in each pose was rejected, because consistency (for example of texture) would be lost (PDF p.7).

**S3/S4 (fat data).**
- **F13** These are 15 single points on the right side only, measured standing; a whole body cannot be built from them without a model.
- **F14** The samples are small and regional (Austria and New Zealand in S3; S4's study was approved by the Graz ethics committee (S4 p.1-2), and where its subjects were recruited is not stated in what I read: **unverified**).
- **F15** The device's reliability varies by site (CV up to 12.2% at the rear thigh, S3 p.5).

## For the lane (mine)

- **The lane already has a distance field round the skeleton.** S1b's skin is exactly a field-pushed offset round swept ellipses. Muscle bellies can enter the field as ellipsoid primitives whose radii come from the volume-keeping rule, and the fall-off radius plays the part of "filler fat".
- **A fat map from the round 5 skinfold sites (A15)** could be built as a smooth scalar on the surface (for example radial-basis interpolation between site values, with Table 5 pads as extra local bumps). The lane would then **check** it against S3/S4 at the 15 sites. This step is my proposal; no source here validates it.
- **Cross-section with joint angle:** use S1b's constant-volume, constant-ratio rule rather than S2's per-axis length ratio, which overshoots (see Section 1).
- **Muscles that cannot be dropped for silhouette:** deltoid, pectoralis major, gluteus maximus (plus the gluteal fat pad), and gastrocnemius with soleus. Next would be trapezius and latissimus dorsi, which need the wide-attachment models.
