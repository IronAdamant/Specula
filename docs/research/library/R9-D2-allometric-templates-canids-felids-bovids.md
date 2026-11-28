# R9-D2 Proportion as a small set of rules: allometric templates for canids, felids and bovids

Round 9, section D. Access date for every source: **2026-11-28**. Everything is in my own words. Numbers are reported as facts with their location; **no dataset was taken**. Reasoning marked **(mine)** is my own. Pages are **PDF pages** unless marked "printed".

**Scope note.** The brief asks for material **beyond rounds three and seven**. Round 3 (library C1, mammal proportions) already holds:
- Kilbourne & Hoffman 2013 whole-mammal limb-length exponents (hind 0.37, fore 0.40);
- Alexander et al. 1979 (bone lengths ∝ M^0.35);
- the abstracts of Christiansen 1999, Christiansen 2002 and Bertram & Biewener 1990.

Round 7 (R7-02, R7-2A) holds quadruped masses and bulk. This file adds:
- family-level and order-level exponents (bovids in full, artiodactyls and carnivorans by order, felids from bone geometry);
- the **covariance structure** within one canid population;
- the habitat effect that sits on top of size.

**Questions this item answers:**
- How do head, neck, trunk and limb proportions move together across canids, felids and bovids?
- Can a new animal be placed as a point between known ones?

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| S1 | Scott K.M. (1985). "Allometric trends and locomotor adaptations in the Bovidae." *Bulletin of the American Museum of Natural History* 179(2):197-288 (the title page gives 197-288; catalogues give 199-288) | Internet Archive (taxonomy archive collection): https://archive.org/download/bulletin-american-museum-natural-history-179-199-288/bulletin-american-museum-natural-history-179-199-288.pdf (96 PDF pp.). Also listed by BHL (item 166761), which was behind a bot check from the box | "Copyright © American Museum of Natural History 1985" (PDF p.3). AMNH bulletins are made freely readable by the museum; **the archive.org copy's authorisation is not separately verified** | Yes: abstract, methods, Tables 3, 4 and 8, summary (PDF p.5-8, 15-16, 20-21, 29-30). **Printed page = PDF page + 194** |
| S2 | Kilbourne B.M., Hoffman L.C. (2013). "Scale effects between body size and limb design in quadrupedal mammals." *PLoS ONE* 8(11):e78392. doi:10.1371/journal.pone.0078392 | https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0078392 | CC BY (PDF p.1; Crossref lists CC BY 4.0) | Yes: Tables 1 and 3, methods, abstract |
| S3 | Doube M., Wiktorowicz Conroy A., Christiansen P., Hutchinson J.R., Shefelbine S. (2009). "Three-dimensional geometric analysis of felid limb bone allometry." *PLoS ONE* 4(3):e4742. doi:10.1371/journal.pone.0004742 | https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0004742 | CC BY (PDF p.1: "Creative Commons Attribution License"; Crossref lists 4.0) | Yes: abstract, introduction, Tables 1-3 (Table 1 checked on a page image) |
| S4 | Chase K., Carrier D.R., Adler F.R., Jarvik T., Ostrander E.A., Lorentzen T.D., Lark K.G. (2002). "Genetic basis for systems of skeletal quantitative traits: principal component analysis of the canid skeleton." *PNAS* 99(15):9930-9935. doi:10.1073/pnas.152333099 | PubMed Central: https://pmc.ncbi.nlm.nih.gov/articles/PMC126602/ (read via WebFetch, saved as raw/round9/chase02.webfetch.txt; the box's PDF download failed) | PNAS free access on PMC; **no open licence** | Yes: abstract, methods, results, PC1-PC4, discussion, Table 1. **Cited by section**, because the WebFetch text has no page numbers |

## 1. Bovids: one family, 108 species (S1)

**Method** (S1 PDF p.5-8):
- Museum skeletons of **108 bovid species** from six collections; adults only (fused epiphyses); no zoo animals, because zoo skeletons were visibly porous.
- Species are the unit. Sexes or subspecies are split when their weights differ by more than 10%.
- Lengths are taken between articular surfaces, not to the joint centres.
- Body weight is a species average, not the specimen's own weight. S1 argues that one-off weights of wild animals carry gut-fill and hydration error, and checks the approach against a small set of specimen-weighed antelopes (PDF p.10-12).
- Length against diameter uses **560 individual specimens** (PDF p.6).
- Species are put into **five habitat groups**:
  1. flat open grassland or arid;
  2. mixed woodland and open;
  3. heavy forest;
  4. true mountains;
  5. rolling hills (PDF p.8).

**Length against body weight** (Table 3, PDF p.15, printed 209). Exponent b in length ∝ (body weight)^b, with its 95% interval. Geometric similarity would give b = 0.333.

| Element | b ± 95% CI | r | Fits the elastic prediction (0.25)? |
|---|---|---|---|
| Scapula | 0.348 ± 0.013 | 0.833 | no, barely |
| Humerus | 0.267 ± 0.012 | 0.811 | yes |
| Radius | 0.260 ± 0.018 | 0.680 | yes |
| Metacarpal | 0.186 ± 0.031 | **0.372** | no |
| Whole forelimb | 0.239 ± 0.017 | 0.677 | yes |
| Femur | 0.259 ± 0.014 | 0.811 | yes |
| Tibia | 0.209 ± 0.014 | 0.677 | no |
| Metatarsal | 0.175 ± 0.026 | **0.473** | no |
| Whole hindlimb | 0.216 ± 0.014 | 0.704 | no |
| Forelimb plus scapula | 0.262 ± 0.016 | 0.769 | yes |
| **Body length** | 0.269 ± 0.021 | 0.885 | yes |
| **Thoracic circumference** | 0.369 ± 0.002 | 0.925 | no, barely |

(Values rounded from S1's four decimals.)

**What the table says as a template** (S1 PDF p.16, p.29-30):
- **Every length scales with negative allometry.** Big bovids have relatively shorter legs.
- At high weights, total limb length grows slowly or not at all.
- The **trunk girth grows faster than isometry** (0.369), and **body length** grows slower (0.269). Large bovids are stockier.
- **Proximal bones (humerus, femur) follow size closely. Distal bones (metapodials) do not:** r is only 0.37 to 0.47, because habitat matters more than size for them (PDF p.5, p.20).
- **Habitat shifts distal lengths independently of size:**
  - open-country runners have long distal elements;
  - mountain climbers have short metapodials;
  - woodland and hill species are in between (PDF p.5, p.30).
- **Within habitat groups** the exponents still vary widely (Table 4, PDF p.21: metacarpal 0.153, 0.201 and 0.118; femur 0.270, 0.276 and 0.246).
  - The table labels the three columns Groups 1, 2 and 3, but its footnote says groups 1, 2 and **4**. **This discrepancy is unresolved in the source.**
- **Other size effects** (PDF p.30):
  - large ungulates hold their bones less angled, reducing bending;
  - large ungulates put smaller multiples of body weight on their limbs when running;
  - the **centre of mass shifts forward** in large bovids, giving negative allometry in hindlimb elements.
- **Length against diameter** (Table 8, PDF p.29): exponents of 0.61 to 0.76. Every bone gets **thicker relative to its length** as size rises. Only tibia and metatarsal fit the elastic value of 0.67.
- **S1's stated limit** (PDF p.30): no single scaling theory, elastic or otherwise, explains all the dimensions, least of all in taxonomically mixed groups.

## 2. Artiodactyls and carnivorans by order (S2)

**Method** (S2 PDF p.3, p.5):
- 44 species. Each **whole limb** was removed, thawed and weighed.
- Length was measured from the pivot (hip or shoulder) to the tip of the limb **in its passively flexed position**, to avoid known posture differences between small and large mammals.
- Body mass is the specimen's own recorded mass where it could be measured; otherwise a literature value (Smith et al. 2003) was used (PDF p.3).

**Samples** (Table 1, PDF p.5):
- **Carnivora (17):** canids (coyote, grey wolf, African hunting dog, bat-eared fox, grey fox, red fox), felids (cheetah, caracal, bobcat, mountain lion), and mustelids, procyonids and others.
- **Artiodactyla (12):** eight bovids (impala, springbok, black wildebeest, waterbuck, klipspringer, grysbok, bushbuck, greater kudu), three cervids and the okapi.

**Limb length against body mass** (Table 3, PDF p.7; slope with 95% interval; the test is against 1/3):

| Group | n | Hindlimb slope | Forelimb slope |
|---|---|---|---|
| All mammals | 44 | 0.37 (0.342-0.408) | 0.40 (0.367-0.434) |
| Artiodactyla | 12 | 0.30 (0.243-0.381) | 0.34 (0.271-0.434) |
| Carnivora | 17 | **0.42** (0.322-0.542) | **0.42** (0.333-0.533) |
| Cursorial species | 25 | 0.29 (0.265-0.313) | 0.32 (0.288-0.350) |

**Limb mass** scales with geometric similarity (limb mass ∝ body mass^1.0) in both limbs. Forelimb inertial properties other than mass show positive allometry (S2 PDF p.1).

**The sources disagree on bovid limb scaling. Both views are given.**
- **S1 (bovids only, skeletal lengths, 108 species):** whole forelimb b = 0.24 and hindlimb b = 0.22, clearly below isometry.
- **S2 (artiodactyls including bovids, whole flexed limbs, 12 species):** hind 0.30 and fore 0.34, with intervals that include 1/3.
- Likely reasons **(mine)**:
  - S2's sample is small and mixes cervids and the okapi with bovids;
  - S2 measures the soft-tissue limb in a flexed pose, not summed bone lengths;
  - S1 uses species-average weights, S2 mostly specimen masses;
  - S1 shows that the curve flattens at high weights, so the slope depends on the size range sampled.

## 3. Felids: posture held constant, bones thicken (S3)

**Why felids:** posture and gait do not change with body mass across nine felid species (Day & Jayne, cited in S3 PDF p.2). Felids are therefore the cleanest family for pure size effects: **they scale bone shape rather than change posture**, unlike similar-sized animals in other groups (S3 PDF p.1).

**Method** (S3 PDF p.1):
- CT of 9 limb bones from 13 individuals of 9 species, from domestic cat to tiger.
- 11 geometric parameters per slice; exponents computed at every **5% of bone length**.
- Exponents are phylogenetically corrected, with no fixed intercept.

**Earlier felid length exponents** (S3 Table 1, PDF p.2, from Anyonge 1993; read on the page image):
- body mass ∝ femur length^3.20 and ∝ humerus length^3.13 (isometry is 3);
- inverted, length ∝ M^0.31 (femur) and M^0.32 (humerus) **(mine)**. This is mild negative allometry, much milder than in bovids.

**Bone diameter against length** (S3 Table 2, PDF p.3; maximum midshaft diameter ∝ length^a, ± 95% CI; isometry is 1):
- humerus 1.36 ± 0.46;
- radius 1.34 ± 0.47;
- femur 1.11 ± 0.27;
- tibia 1.19 ± 0.21;
- third metacarpal 1.06 ± 0.21;
- third metatarsal 1.26 ± 0.35;
- scapula 4.08 ± 2.13 (very uncertain).

**Joints** (S3 Table 3, PDF p.5): humeral head radius ∝ humeral length^1.34 and femoral head radius ∝ femoral length^1.26. **Joint surfaces grow faster than bone length.**

**S3's main finding** (PDF p.1):
- Allometry varies along each bone with local function.
- Joint surfaces and muscle attachments show stronger positive allometry than the midshaft.
- A single exponent per bone is a simplification.

## 4. Canids: how proportions co-vary within one population (S4)

**Method** (S4 Materials and Methods; Results):
- 330 Portuguese Water Dogs, all at least 2 years old, descended from 31 founders.
- Five radiographs each (skull profile, skull ventral-dorsal, pelvis, forelimb, hindlimb).
- Males and females were adjusted to the same mean.
- Principal component analysis on the trait correlation matrix.

**Result** (S4 Results, Table 1):
- Ten components explain 75% of variation.
- The first four explain 60.7%: PC1 43.6%, PC2 8.1%, PC3 4.6%, PC4 4.5%.

| Component | What moves together | Heritability (Table 1) |
|---|---|---|
| PC1 | **Overall size.** All loadings positive and similar (most 0.08-0.13) | 0.23 |
| PC2 | **Pelvis against head and neck.** Small pelvis and lumbar vertebrae go with large posterior face, small anterior face and large jaw and neck muscle attachments | 0.55 |
| PC3 | **Skull and limb length against skull width and height** (including cranial volume). Greyhound-like (long head, long limbs) versus Pit Bull-like (broad head, short limbs) | 0.24 |
| PC4 | **Skull and limb length against bone robustness** of limbs and axial skeleton | 0.70 |

**Interpretation in S4** (Discussion):
- After size, the main axes are **trade-offs**: running build versus fighting build.
- S4 links them to growth. Puppies have short faces and stout limbs, adults longer faces and slender limbs.
- S4 cites Wayne's view that differences between dog breeds, and between dogs and wild canids, echo the proportions seen during a dog's growth (heterochrony).

**Limit:**
- one breed;
- a within-population analysis (not across canid species);
- radiograph metrics;
- loadings given only for a sample of traits in Table 2.

## 5. A template you can interpolate: what the sources support (mine)

1. **Size first, then shape axes.**
   - Every source separates size from shape: S1 (size against habitat), S2 (order-level slopes), S3 (posture held constant), S4 (PC1 against PCs 2-4).
   - A family template can therefore be **log-size plus a few shape scores**. Place a new animal by its mass, then by its position on two or three trade-off axes:
     - cursorial against robust (S4 PC3 and PC4; S1 habitat groups);
     - head-neck against pelvis (S4 PC2).
2. **Family exponents differ and should not be shared:**
   - limb length is about M^0.22-0.24 in bovids (S1), about M^0.31-0.32 in felids (S3 Table 1), and about M^0.42 across Carnivora (S2, wide interval);
   - trunk girth in bovids is about M^0.37 (S1).
   - **Use the exponent of the family being generated.** Where the sources disagree (S1 against S2 for bovids), keep both as bounds.
3. **Distal elements carry the habitat signal.** Metapodial length is weakly predicted by size (S1 r = 0.37 and 0.47), so it should be a free shape parameter within a range, not a size rule.
4. **Thickness rises with length in every family.** Diameter ∝ length^(1.1-1.4) in felids (S3) and length ∝ diameter^(0.61-0.76) in bovids (S1). Joint radii rise faster than length (S3). A scaled-up animal that keeps its proportions exactly will look **too slender**.
5. **Not found openly:**
   - a cross-species canid or felid head-neck-trunk dataset with exponents;
   - neck length scaling in any of the three families;
   - the full S1 appendix species tables were not read (Appendix 2 summary statistics run to PDF p.48-93).
