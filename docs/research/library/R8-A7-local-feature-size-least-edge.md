# R8-A7 Predicting the least edge: local feature size and its Lipschitz property

Round 8, section A. Access date for every source: **2026-11-27**. PDFs were downloaded to the box and read with poppler. Ruppert's report uses Type-3 fonts, so it was OCR'd with Tesseract, and every formula used here was checked on a rendered page image. All wording is mine; no code was taken from any source.

**Question this item answers.** Give the definition of local feature size (lfs) and its Lipschitz property, so the engine can say **before building** whether an offset of a figure, at a given level of detail, will have an edge shorter than a given length.

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| L1 | **Listed: Ruppert J. (1995).** "A Delaunay refinement algorithm for quality 2-dimensional mesh generation." *J. Algorithms* 18(3): 548-585. Read as NASA Ames technical report **RNR-94-002** (1994, 41 pp., marked "to appear in J. Algorithms") from the NASA NAS technical-report archive, https://www.nas.nasa.gov/assets/nas/pdf/techreports/1994/rnr-94-002.pdf (`ruppert94.pdf`; re-downloaded 2026-11-27 and identical by MD5) | US-government technical report | NASA report; no licence text (US government work, believed public domain; not confirmed) | Yes, all pages, by OCR plus page images. Page = PDF page. **Journal pages not mapped** |
| S1 | **Supporting (later exposition and sharper constants):** Shewchuk J. R. (2012). *Lecture Notes on Delaunay Mesh Generation*, §6.3 (Ruppert's algorithm), https://people.eecs.berkeley.edu/~jrs/meshpapers/delnotes.pdf | Author's copy | No licence stated | §6.3.2-6.3.4. Printed page = PDF page - 4 |
| S2 | Supporting: Shewchuk J. R. (1996), "Triangle" (see R8-A6 L2), for practical limits | Author's copy | - | p. 5-7 |
| S3 | **Supporting (the other meaning of "local feature size"):** Amenta N., Bern M., Kamvysselis M. (1998). "A new Voronoi-based surface reconstruction algorithm." *Proc. SIGGRAPH 98*, 415-421, doi:10.1145/280814.280947 (pages confirmed via Crossref). Author copy https://web.cs.ucdavis.edu/~amenta/pubs/crust.pdf (the file name says "crust", but the paper is this one) | Author's copy | © ACM | §3.1-3.2 (PDF 2-3) |

## 1. Definition (L1 Definition 1, p. 11)

Given a planar straight-line graph (PSLG) X, the **local feature size** lfs(p) at any point p of the plane is **the radius of the smallest disk centred at p that meets two non-incident vertices or segments of X.** Two features are *incident* if they intersect. So a segment and its own end point, or two segments sharing an end point, are incident and do not count as a pair. L1 Fig. 5 (p. 12), disk D3: a smaller disk would touch two segments, but they are incident.

- lfs is defined at every point of the plane, not just on X, and is continuous (L1 p. 11).
- **lfs_min** (S1 Theorem 39 and its footnote, printed p. 92 = PDF 96) is the shortest distance between two non-incident features. It also equals the minimum of lfs over the **input vertices**.
- **Caution (same footnote):** lfs_min is **not** a lower bound of lfs over the whole domain. The midpoint of a segment of length lfs_min has lfs = lfs_min / 2.

## 2. The Lipschitz property (L1 Lemma 1, p. 11-12; S1 Lemma 37, printed p. 89 = PDF 93)

**For any PSLG X and any points p, q: lfs(q) ≤ lfs(p) + |pq|.** By symmetry, |lfs(p) - lfs(q)| ≤ |pq|. lfs is 1-Lipschitz: its slope in any direction is at most 1 (L1 p. 11). S1 p. 88 says its directional derivatives, where they exist, lie in [-1, 1].

**Proof (one line, in my words).** The disk of radius lfs(p) around p touches two non-incident features. The disk of radius lfs(p) + |pq| around q contains that disk, so it touches them too.

**Use.** lfs at one point bounds it everywhere nearby: lfs(q) ≥ lfs(p) - |pq|. So one evaluation covers a neighbourhood, and a lower bound over a region can be certified from a finite set of samples.

## 3. What lfs bounds in a Ruppert mesh

| Result | Statement | Conditions | Source |
|---|---|---|---|
| Ruppert Lemma 2 (boxed conditions) | C_S ≥ C_T ≥ 1; C_T ≥ 1 + 2 C_S sin α; **C_S ≥ 1 + √2 C_T** | All three can hold together iff **α < arcsin(1/(2√2)) ≈ 20.7°**. Example: C_T = (1 + 2 sin α)/(1 - 2√2 sin α), C_S = (1 + √2)/(1 - 2√2 sin α); for α = 10°, C_T = 2.8 and C_S = 5 work | L1 p. 13-16 (image of p. 15 checked) |
| Ruppert Theorem 1 | Every output vertex p has nearest neighbour at distance **≥ lfs(p)/(C_S + 1)** | input angles ≥ 90° (L1 p. 14) | L1 p. 16 |
| Ruppert Theorem 2 | Number of output vertices ≤ C₁ ∫_B dx / lfs(x)² over the bounding square B; the proof works for any **C₁ ≥ 4(2C_S + 3)²/π** (disks of radius lfs(p)/(2(C_S + 1)) around the vertices are disjoint) | same | L1 p. 16-17 |
| Shewchuk Theorem 39 | Ruppert's algorithm terminates and **no edge is shorter than lfs_min** | **B ≥ √2** (circumradius / shortest edge), **input angles between incident segments ≥ 60°** | S1 printed p. 92-93 = PDF 96-97 |
| Shewchuk Lemma 41 / Theorem 42 | Nearest neighbour ≥ **lfs(v)/(D_S + 1)** | B > √2 and input angles > 60° (strict) | S1 printed p. 94-95 = PDF 98-99 |
| Worked constants | For a 15° minimum angle, B ≈ 1.93, D_T ≈ 5.66, D_S ≈ 9.01: spacing at worst about 10 times smaller than lfs (about 7 times away from boundaries) | no acute input angles | S1 printed p. 96 = PDF 100 |

**Angle relation (my arithmetic, matching S1 printed p. 93).** The minimum angle θ and the ratio B are related by sin θ = 1/(2B). B = √2 gives θ = arcsin(1/(2√2)) ≈ 20.7°.

### Disagreements between the sources

1. **Input-angle assumption: 90° versus 60°.** L1 assumes every input angle is at least 90° (p. 14; p. 23 restates it). S1 proves termination with input angles of at least 60° (Theorem 39) and notes this is weaker than Ruppert's assumption (S1 printed p. 89).
2. **The angle bound inside L1: "< 20°" versus 20.7°.** L1 says the algorithm halts for any α < 20° (p. 11; also Theorem 3 on p. 23). Its own Lemma 2 computation gives α < arcsin(1/(2√2)) ≈ 20.7° (p. 15). S1 states 20.7° (printed p. 96). The 20° figure looks like a rounded statement. Both are given here.
3. **Practical limit.**
   - L1 p. 11: "up to about 30°" in practice.
   - S2 p. 5: Triangle usually halts at 33.8° and often fails at 33.9°.
   - S1 Fig. 6.15 (PDF 100): on the Lake Superior PSLG it terminates at 34.2° and not at 34.3° or above.
   These differences come from different inputs and implementations. They are not contradictions.

### Known failure cases

- **Small input angles.** With small input angles the basic algorithm can loop forever.
  - L1 §5 (p. 24-26): "corner-lopping" / shield circles; angles larger than 2α are subdivided into pieces of α to 2α.
  - L1 p. 40: a heuristic of concentric circular shells of radius D·2^i around each input vertex (D for example 0.01). A subsegment touching an input vertex is split at the shell nearest its midpoint (the shell number log₂ of the midpoint distance over D, rounded). Without this, "the basic algorithm ... would loop endlessly" on many small angles (Fig. 26).
  - S2 p. 6-7:
    - Mitchell's bound on the ratio of adjacent edge lengths in a quality triangulation is (2 cos θ)^(180°/θ), which is 27 for θ = 30°;
    - a cascade of small angles at 30° fails near input angles of about 0.6°;
    - concentric shells are effective above about 5°, and Triangle warns about input angles below 5°;
    - convex-hull edges create hidden small angles, so triangles are removed early.
- **lfs uses Euclidean, not interior, distance (L1 p. 25, Fig. 13).** Two "arms" of a non-convex polygon that are close across the **outside** create a small lfs inside, and so small triangles, "suboptimal if only an interior triangulation" is wanted. Ruppert adapts earlier fixes at a constant-factor cost. For an offset engine this means **lfs is conservative** for a figure that nearly touches itself across a gap.
- **Two different "local feature sizes" exist.** In the surface-reconstruction literature the same idea is measured as the **distance from a point of a smooth curve or surface to its medial axis** (S3 §3.2, PDF 3). S3 also notes that at sharp corners the medial axis **touches** the surface, so that measure is **zero at corners** (PDF 3).
  - For polygons, which have corners, Ruppert's PSLG definition is the usable one.
  - Do not mix the two definitions: their values differ near every vertex.

## 4. Predicting the least edge of an offset before building it (my derivation, labelled)

None of the sources gives this procedure. It combines the facts above with the offset kinematics of R8-A1/A3, and is offered for checking.

**Setting.** P is a simple polygon (or PSLG) with edge lengths L_i and interior angles α_j. The offset is at distance d, by mitre or by round joins. The arcs are drawn at "detail" h: the largest allowed gap (sagitta) between an arc and its chord.

**Fact A (input edges).** Every input edge pq has |pq| ≥ lfs(p), because the disk of radius |pq| at p reaches q, which is not incident to p. So **the shortest input edge is at least lfs_min.** Equality is possible, for example in a short edge's own neighbourhood.

**Fact B (mitre offset, straight-skeleton kinematics).**
- A mitre vertex with interior angle α moves along its bisector at speed 1/sin(α/2) (R8-A3, Palfrader-Held).
- Its component along each adjacent edge is cot(α/2).
- So, until the first skeleton event, an inward offset edge between vertices with angles α_a and α_b has length
  L_i(d) = L_i - d·(cot(α_a/2) + cot(α_b/2)).
- Convex corners (α < 180°) shorten edges. Reflex corners (α > 180°, negative cotangent) lengthen them.
- For an outward offset, replace d by -d.
- So **the edge drops below ℓ at d_i = (L_i - ℓ)/(cot(α_a/2) + cot(α_b/2))** when the denominator is positive.
- **The first edge below ℓ is at min_i d_i, but only if that is before the first event** (an edge or split event, R8-A1). After an event, recompute on the new polygon.

**Fact C (round offset at detail h).**
- An arc of radius d and turning angle φ (φ = π - α at a convex corner of an outward offset, α - π at a reflex corner of an inward one) needs n = ⌈φ / (2 arccos(1 - h/d))⌉ chords to keep the sagitta ≤ h.
- With equal chords, each has length 2d·sin(φ/(2n)).
- **For a nearly straight corner (small φ), n = 1 and the chord is 2d·sin(φ/2), which tends to 0.** So the round offset has an edge shorter than ℓ whenever some joined corner has turning angle φ < 2 arcsin(ℓ/(2d)), unless such tiny arcs are merged or dropped.
- The straight parts follow Fact B with the round-join trims (d·cot(α/2) at each convex corner of an inward offset).

**Fact D (collisions between non-incident features, using lfs).**
- If the offset curves of two non-incident features f and g meet at a point x, then dist(x, f) = dist(x, g) = d, so dist(f, g) ≤ 2d.
- Hence **for d < lfs_min / 2, no two non-incident features' offsets collide.** The only changes up to that distance are the corner effects of Facts B and C.
- Locally, by the Lipschitz property: around a point p, nothing new collides within distance lfs(p)/2 of p.
- (By the S3 caveat, this is Ruppert's lfs, and it is conservative across exterior gaps, L1 Fig. 13.)

**Fact E (if the offset is then meshed by Delaunay refinement).** Apply S1 Theorem 39 / 42 to the offset as a new PSLG X′. No mesh edge will be shorter than lfs_min(X′), and near a vertex v no shorter than lfs_X′(v)/(D_S + 1). This holds for minimum angles up to 20.7° and input angles ≥ 60°; Ruppert's own statement assumes 90°.

**Resulting check, before building:**
1. Compute lfs_min of the input (the shortest distance between non-incident features), and the corner angles.
2. If d < lfs_min/2, only corner effects matter. Evaluate Facts B and C in closed form.
3. Otherwise, run the skeleton event computation (R8-A1) up to distance d, and apply Fact B piece by piece.
4. Report the predicted minimum edge length against ℓ.

## Summary of key answers

- **lfs(p)** = radius of the smallest disk at p meeting two non-incident vertices or segments (Ruppert Def. 1, p. 11).
- **Lipschitz:** |lfs(p) - lfs(q)| ≤ |pq| (Ruppert Lemma 1; Shewchuk Lemma 37).
- **lfs_min** = minimum of lfs over the input vertices. Every input edge is ≥ lfs at its end points.
- **Mesh bounds:** Ruppert, nearest neighbour ≥ lfs/(C_S + 1) (α < 20.7°, inputs ≥ 90°). Shewchuk, no edge < lfs_min (B ≥ √2, inputs ≥ 60°), and ≥ lfs/(D_S + 1).
- **Offsets (my derivation):** mitre edges shrink at rate cot(α_a/2) + cot(α_b/2); round arcs give chords 2d·sin(φ/(2n)), which are tiny for small turning angles; no non-incident collisions for d < lfs_min/2.

## Not obtained or not verified

- The *J. Algorithms* version of L1 was not read (the 1994 report was used), so journal pages are not given.
- Section 4 is entirely my derivation. The cot formula is standard trigonometry consistent with R8-A3, but no source states it in this form.
