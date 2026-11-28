# R9-B3 The ear as a shape from numbers

Round 9, section B. Access date for every source: **2026-11-28**. PDFs were read with poppler `pdftotext`. One source was read through the fetch tool (text with page headers, no PDF page markers) and one through Europe PMC XML (cited by section). Everything is in my own words. **Method only:** the BezierPPM code and mesh, and the CT and scan data behind S2 and S1's default mesh, were not taken. Derivations marked **(mine)** are my own.

**Question this item answers** (brief B3). Is there an open parametric description of the auricle (helix, antihelix, tragus, concha, lobule), with each part's size as a share of the ear's length and breadth? (Round two's B9 holds the outer dimensions.)

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| S1 | Perfler F., Pausch F., Pollack K., Holighaus N., Majdak P. (2025). "Parametric model of the human pinna based on Bézier curves and concave deformations." *Computers in Biology and Medicine* 188:109817. doi:10.1016/j.compbiomed.2025.109817 | Austrian Academy of Sciences repository: https://epub.oeaw.ac.at/0xc1aa5572_0x00401cc1.pdf (11 pp.) | **CC BY 4.0** (PDF p.1) | Yes, all pages |
| S2 | Reed M.P., Vallier T.R., Bonifas A.C. (2025). "Development of a parametric model of adult human ear geometry." *Int. J. Industrial Ergonomics* 107:103738. doi:10.1016/j.ergon.2025.103738 | First author's site: https://mreed.umtri.umich.edu/mreed/pubs/Reed_2025_parametric_US_ear_geometry_shape_model.pdf (13 pp.). **This is the publisher's PDF; the rights line reads "All rights are reserved". Whether the author may post it is not verified**, so it is used only for stated findings | Elsevier, all rights reserved | Yes, p.1-10 for methods, PCA results, age and sex findings, and limits |
| S3 | Kent E.E., Emirzeoglu M., Altunsoy E., Özel M., Uzun A. (2021). "Comparative assessment of auricular measurements in young males and females." *Int. J. Morphology* 39(3):742-746 | SciELO Chile: https://scielo.cl/pdf/ijmorphol/v39n3/0717-9502-ijmorphol-39-03-742.pdf. The box download failed, so it was read through the fetch tool; cited by the journal's printed page range and by table number | SciELO journal; open access. Exact licence **not verified** | Yes: methods, Tables I-IV, discussion |
| S4 | Engineer P.J., Japatti S.R., Reddy B M., Tiwari A.U., Siddegowda C.Y., Hammannavar R.B. (2018). "Anthropometric assessment of the normal adult human ear." *Annals of Maxillofacial Surgery* 8(1):42 (first page per Crossref; end page not verified). doi:10.4103/ams.ams_183_17 | Europe PMC PMC6018292 (XML) | **CC BY-NC-SA 4.0** | Yes, text and measurement definitions. **Tables are images, so no numbers were extracted** |

## 1. A parametric auricle: BezierPPM (S1)

**Structure** (S1 PDF p.2-4, Eq. 1). The ear mesh is built as:
- a fixed default ear mesh;
- **plus** displacements from an **armature** of curves (the convex ridges);
- **plus** displacements from **shape keys** (the concave hollows);
- all passed through one **global affine transform** for scale, rotation and position.

**The armature** (PDF p.3-4, Table 2):
- **Nine cubic Bézier curves**, one per named ridge: lobule, helix low, helix middle, helix up, antitragus, antihelix, crus superius of the antihelix, crus inferius of the antihelix, and tragus.
- Each curve has a start point and an end point, plus two control points set through rotations at the two ends.
- Each curve moves only the vertices tied to it, with **hand-painted weights** so neighbouring curves blend smoothly.
- Each curve has **15 numbers**:
  - 6 for the two end positions;
  - 8 for the two rotations (quaternions);
  - 1 isotropic scale that pushes the tied vertices away from the curve.

**The shape keys** (PDF p.3, Table 1):
- **17 named hollows and creases**, each a blend between two limit shapes.
- Weights run from −1 to 1, except keys 15 and 16 (the upper and lower dents of the crus of the helix), which run from 0 to 1. The paper calls these two "convex protrusions" and limits them because it found no evidence of the opposite form in human ears (its wording here is muddled: it says "no evidence for them being convex"; PDF p.3).
- The named keys include:
  - the depths of the cavum conchae, cymba conchae, scapha and fossa triangularis;
  - the depths of the upper, middle and lower helix;
  - the creases of both crura of the antihelix;
  - the prominence and dents of the crus of the helix;
  - a dent in the tragus, a crease in the antitragus, and the ear-canal diameter.

**The global transform** (PDF p.4): 10 numbers (3 translation, 4 rotation, 3 scale along the three axes).

**Total.** 162 parameter dimensions (PDF p.2). The breakdown is 9 × 15 + 17 + 10 = 162 **(mine; it matches the stated total)**.

**Default ear and pose** (PDF p.2):
- The default mesh is the **average ear** of an outside ear database, taken from that database's mean principal-component weights, then cleaned and made watertight.
- It is posed with an outward flare of **28.53°** and a backward tilt of **24.01°**. Both are averages from a cited study.
- Only the front of the ear is modelled in detail (PDF p.2, p.8).

**Accuracy** (PDF p.1, p.6-8, p.10):
- Twenty ears were fitted by hand: ten real ears (four from the ITA HRTF database chosen to span ear height, three from ARI and three from Sorbonne University) and ten synthetic ears with particularly large principal components, chosen as extremes from the WiDESPREaD database, which the authors say may even be pathological (PDF p.5-6).
- The 2-mm completeness was 87 ± 4% and the 1-mm completeness 66 ± 5%.
- **Most large errors are on the back of the ear**, which the model does not try to represent (PDF p.8).
- In a synthetic test, a toy network estimated the local rotation parameters of unseen synthetic ears from rendered images; the resulting geometry erred by about **0.3 ± 0.1 mm** (PDF p.9-10).

**Failure cases and limits, as stated:**
1. **The back of the ear is not modelled**, so errors concentrate there (PDF p.8).
2. **Parameters are not always intuitive.** Because the curve and shape-key effects are weighted and summed, a curve can end up partly outside the deformed mesh (PDF p.4, Fig. 3c).
3. **The weights were painted by hand**, which ties the model to one default mesh (PDF p.4).
4. **The synthetic test proves little about real ears.** The authors say it is limited to synthetic ears and to one parameter group (the local rotations, each drawn uniformly within ±60°) (PDF p.9-10).
5. **Rendering is slow.** Meshes are made in microseconds, but rendering images through the Blender renderer took seconds per sample (PDF p.9).

**Disagreements inside S1, both given:**
- **Fitting error.** The abstract gives the mean RMS fitting error as **1.5 ± 0.2 mm** (PDF p.1). The results section gives **1.5 ± 0.4 mm** and says the 0.2 figure belongs to an earlier model in reference [20] (PDF p.6).
- **Size of the synthetic test set.** Of 10,000 synthetic ears, 7,200 trained the network, 800 were held out for validation and the remaining **2,000** were the test set (methods and conclusions, PDF p.10). The results sentence nevertheless reports the 0.3 ± 0.1 mm error "on the **800** unseen ears" (PDF p.10). Which set was scored is therefore unclear.

**What it gives the lane without its data (mine):**
- The **named-part vocabulary**: 9 convex ridges as curves and 17 named concavities as bounded weights. Each part is a named thing with bounded numbers, which is what the brief asks of detail.
- Every shape-key weight is bounded by construction.
- The default mesh is the one data-dependent part. The engine would need its own neutral ear.

## 2. Part sizes as shares of length and breadth (S3, with S4 for definitions)

**Definitions** (S3 Tables I-II; S4 Methods):
- **Ear length:** top of the helix to the bottom of the lobule.
- **Ear width:** front to back of the auricle. S4 measures it from the line where the auricle joins the face to the outermost helix.
- **Lobule length:** from the lowest point to the base of the intertragic notch. **Lobule width:** across the lobule at mid-height.
- **Concha length:** from the highest point of the antihelix to the deepest point of the intertragic notch. **Concha width:** from the anterior notch of the auricle to the point of sharpest antihelix curvature.
- **S4 adds:** ear length above and below the tragus, and tragus length.
- **S4's method:** photographs with a 1-mm grid, head in the Frankfurt horizontal.

**Means, young adults aged 18-30, 56 men and 59 women** (S3 Table III; printed in **cm** although the caption says mm):

| Measure | Men R | Men L | Women R | Women L |
|---|---|---|---|---|
| Ear length | 6.23 | 6.20 | 5.83 | 5.85 |
| Ear width | 3.41 | 3.40 | 3.04 | 2.97 |
| Lobule length | 1.85 | 1.81 | 1.61 | 1.65 |
| Lobule width | 1.92 | 2.05 | 1.76 | 1.88 |
| Concha length | 2.64 | 2.65 | 2.51 | 2.51 |
| Concha width | 2.03 | 2.09 | 1.89 | 1.87 |

**Indices** (S3 Table IV):
- auricular index (width × 100 / length): 54.1 (men, right), 52.4 (women, right);
- conchal index: 78.0 and 76.2.

**Shares, computed from S3's right-ear means (mine):**

| Share | Men | Women |
|---|---|---|
| Concha length / ear length | 0.42 | 0.43 |
| Lobule length / ear length | 0.30 | 0.28 |
| Lobule width / ear length | 0.31 | 0.30 |
| Concha width / ear width | 0.60 | 0.62 |
| Ear width / ear length | 0.55 | 0.52 |

**Not available as shares here:** helix rim width, antihelix width, and tragus size. S4 defines tragus length but its numbers are in table images; S3 has no tragus row.

**Errors inside S3, so treat its numbers with care:**
- Table III is labelled mm but printed in cm.
- The Spanish summary repeats the lobule-length figures as lobule widths.
- Its discussion quotes an inclination angle in "mm".
- The English abstract's means agree with Table III after conversion to mm.

**Findings on sex and age:**
- **S3:** men's ears were larger on most measures. Every index except the right lobular index differed significantly by sex (S3 Results).
- **S4** (505 adults aged 18-64, Maharashtrian; Discussion):
  - men's ears were slightly larger in every age group, but **not significantly**;
  - **the ear keeps growing after adulthood, length faster and for longer than width**;
  - the ear varies with population, even within India.
- **S2** (331 ears from CT scans of 224 adults aged 18-81, US; PDF p.1, p.5):
  - confirms that ears grow with age, with a **sex difference in that growth**;
  - finds that body-mass index affects the ear's position and tilt;
  - the mean female ear is smaller and tilts out more at the top (PDF p.5, Fig. 8).

## 3. How many numbers a statistical ear needs (S2, for contrast)

**What S2 found** (PDF p.4-5):
- In S2's whole-region model (scalp, pinna and canal), PC1 is mostly the ear's **sideways position**, i.e. head breadth, and takes nearly half the variance.
- **Ear size itself does not appear until PC4.**
- 8 components give about 91% of the variance; 14 give 95%.
- The authors warn that detail that matters for a design can sit in higher components.

**What it means for the lane (mine).** A data-driven count is dominated by **where the ear sits**, not by its shape. Specula should therefore keep **placement** (on the head, from round two's B9 and Farkas's ear inclination) separate from **ear shape**.

**Limits S2 states** (PDF p.2, p.8-10):
- A convenience sample of patients who needed a CT scan for medical reasons, so possibly not representative of the general population (p.2, p.10).
- The subjects' regions of origin are unknown; other populations may differ (p.8).
- Scans were taken face down or face up, and the two postures differ in where the ear sits against the skull (p.5, p.9).
- The eardrum could not be seen in most scans; its lateral position is estimated to within about 2 mm (p.8-9).

## 4. Lane notes (mine)

1. **Use BezierPPM's structure as the plan.** Nine named ridge curves, 17 bounded hollows and one placement transform. Let ear length and width (round two's B9) set the global scale.
2. **Set proportions inside the ear from shares,** such as concha about 0.42 of length and lobule about 0.29 of length. These are tied to sex and age only through overall size. The age growth (length faster than width) needs per-measure growth rates, which the open text gives only qualitatively.
3. **No open source found gives helix-rim width or antihelix width as shares (missing).**
