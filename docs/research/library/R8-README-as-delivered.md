# Library research, round 8: README and coverage map

Prepared 2026-11-27 in answer to the round-eight brief (`brief.md` in this folder). **Access date for every source: 2026-11-27.**

How the files were made:
- Content is given in my own words with page or section cites (printed page and PDF page where they differ).
- Nothing is copied whole from an in-copyright text, and no code is taken from any source.
- My own derivations and syntheses are labelled as mine.
- Where sources disagree, both readings are given.
- Each library file has a `.verify.md` beside it: an independent second check, row by row (VERIFIED / FIX / WEAK / UNVERIFIABLE). The fixes were applied to the main file.
- Following the owner's added instruction, a listed source that was missing, unreadable or thin was backed by a **labelled substitute**, held to the same standard. See `blocked-downloads-ALTERNATIVES.md`.
- Raw downloads are in `specula-research/raw/round8/` on the working box. These include page-marked text (`*.pg.txt`, with `=== PDF p.N ===` markers) and page images used to check formulas. Fetches that returned HTML instead of a PDF are kept as `*.FAILED-not-a-pdf.html`.

## Section A: soft edges on raised figures (complete, checked)

| Item | File | Primary sources read | Substitutes | Not obtained | Check rows / fixes |
|---|---|---|---|---|---|
| A1 Straight skeleton: definition, events, degeneracy | R8-A1-straight-skeleton-definition-events.md | AAAG95 (J.UCS), Eppstein-Erickson 99, Huber thesis 2011 | S2b (AA authors' own version) for COCOON '96 | COCOON '96 LNCS text | 38 / 2 |
| A2 Doing it robustly | R8-A2-robust-straight-skeleton.md | Huber 2011, Huber-Held 2012, PHH12, CGAL manual; FO read from an unclear-status copy | Eder-Held-Palfrader 2020 (CC BY), Wang MIT report | Yakersberg 2004; FO legal copy | 32 / 0 (3 WEAK) |
| A3 Offsets with round and mitred joins | R8-A3-offsets-round-mitre.md | Palfrader-Held 2015, Held-Huber 2009, Eppstein-Erickson 99 | W3C SVG 2 §13.5.5, §13.5.7 (mitre limit) | Held 1998; VRONI 2001 | 22 / 2 |
| A4 Merging near points without breaking the mesh | R8-A4-snap-rounding-link-condition-foldover.md | Hobby (TR), Halperin-Packer ISR, de Berg-Halperin-Overmars, Dey et al. 1999, Hoppe 93, Garland-Heckbert 97 | Packer MSc (for ISR); Garland thesis and Floater (fold-over) | Guibas-Marimont; Hershberger 2013; Goodrich et al. 1997 | 41 / 3 |
| A5 Exact answers from floating point | R8-A5-exact-predicates-shewchuk.md | Shewchuk 1997 (robustr), robnotes | Rust RFC 3514 (float semantics) | none | 25 / 3 |
| A6 Capping a figure with holes | R8-A6-capping-holes-fist-cdt-flips.md | Triangle 1996, Shewchuk CDT paper | Held CGI'98 (for FIST), Eberly ear clipping (CC BY 4.0), Shewchuk delnotes (for Lawson) | FIST 2001; Lawson 1977 | 39 / 5 |
| A7 Predicting the least edge | R8-A7-local-feature-size-least-edge.md | Ruppert (NASA RNR-94-002), delnotes §6.3, Triangle, Amenta-Bern-Kamvysselis 98 | none | J. Algorithms version (pages) | 26 / 3 |

### Key answers, section A
- **A4.**
  - **Snap rounding (SR):** each rounded segment stays within half a pixel of the original in x and in y, which is at most √2/2 pixel in Euclidean distance (Guibas-Marimont via de Berg et al. p. 2). No segment passes completely over a vertex, and no new crossings appear (Hobby Thm 4.1). The failure case is near-degenerate vertex-edge gaps.
  - **Iterated snap rounding (ISR):** gives a half-pixel vertex-to-edge clearance (Lemma 3.3), but drift can grow to Θ(n²) pixels (Packer Thm 3.8).
  - **Edge collapse:** collapsing ab is safe when the link condition holds, Lk(a) ∩ Lk(b) = Lk(ab) (Dey et al.; the ω-cone handles boundaries).
  - **Fold-over:** test the exact orient2d sign of every incident triangle before and after the move. Floater Thm 4.1 (p. 691) gives the global one-to-one guarantee for convex-combination maps with a convex boundary.
- **A5** (ε = 2^-53, from Shewchuk 1997, printed pp. 40-41 and the tables):
  - **orient2d**: A = (3ε+16ε²)·(|x5|+|x6|); B′ = 2ε+12ε²; C = (3ε+8ε²)|B′| + (9ε²+64ε³).
  - **orient3d**: A = 7ε+56ε²; B′ = 3ε+28ε²; C = 26ε²+288ε³.
  - **incircle**: A = 10ε+96ε²; B′ = 4ε+48ε²; C = 44ε²+576ε³.
  - Splitting uses the multiplier 2^27+1 (Dekker, Thm 17). The method needs round-to-nearest-even, with no extended-precision double rounding. Rust's float semantics (RFC 3514) promise this except on i586 x87 and 32-bit ARM NEON.
- **A7.**
  - lfs is 1-Lipschitz.
  - Ruppert: the least edge is at least lfs/(C_S+1) for angle bounds up to 20.7°, although his text also says "< 20°" (both readings are given).
  - Shewchuk Thm 39: with B ≥ √2, no edge is shorter than lfs_min.
  - My derivation:
    - mitre-inset edge length L(d) = L − d(cot(α_a/2)+cot(α_b/2));
    - round chord length 2d·sin(φ/(2n));
    - no collisions between non-incident features for d < lfs_min/2.

## Sections B, C, D

| Item | File | Primary sources read | Substitutes / additions (labelled) | Not obtained | Verify rows / fixes |
|---|---|---|---|---|---|
| B1 Convolution fields, slope bounds | R8-B1-convolution-fields-slope-bounds.md | Bloomenthal-Shoemake 91, Zanni et al. 13 (SCALIS, HAL), Angles et al. 17 | Hart 1996 sphere tracing (Lipschitz bound; copy legality WEAK) | Sherstyuk 1999 (thesis embargoed); SCALIS ray-tracing paper | 29 / 0 (1 WEAK) |
| B2 Meshing: DC, manifold DC, MC33 | R8-B2-meshing-dual-contouring-mc33.md | Ju et al. 02, Schaefer-Ju-Warren 07, Chernyaev 95 (CERN), Lewiner et al. 03 | Custódio et al. 13 (MC33 bugs) | Ju-Udeshi; Nielson DMC (cited only) | 21 / 4 |
| B3 Quadric simplification with attributes | R8-B3-quadric-simplification-attributes.md | Garland-Heckbert 97, 98; Hoppe 99 | Garland thesis (via A4) | none | 23 / 2 |
| B4 Skin weights | R8-B4-skin-weights-heat-bbw-voxel.md | Baran-Popović 07 (MIT OA), Jacobson et al. 11 | Autodesk Maya docs for Dionne-de Lasa | Dionne-de Lasa 2013 | 21 / 0 (1 WEAK) |
| C1 Lightning (DBM) | R8-C1-lightning-dbm-branching.md | Kim-Lin 07 | Sañudo et al. 95 (CC BY-NC-SA), Bickel et al. 06, Glassner 00, Reis-Fernandes 22 | NPW 1984; Reed-Wyvill 94 | 39 / 2 (1 WEAK) |
| C2 Curl noise and plume rise | R8-C2-curl-noise-and-plume-rise.md | Bridson et al. 07; EPA ISC3 Vol II (public domain) | Cushman-Roisin ch. 8 | Briggs originals | 33 / 0 (1 WEAK) |
| C3 Flame puffing and height (addendum to J11) | R8-C3-flame-puffing-and-height-addendum.md | none (both listed items blocked) | NIST FDS Validation Guide (public domain), Taha et al. 24 (HAL) | Cetegen-Ahmed 93; Heskestad 83; NUREG-1805 | 19 / 0 |
| C4 Bloodstain impact angle and spreading | R8-C4-bloodstain-impact-angle-spreading.md | Attinger et al. 13 | Laan et al. 14 | Hulse-Smith et al. 05 | 27 / 0 |
| C5 Flash thresholds | R8-C5-flash-thresholds-wcag-itu.md | WCAG 2.2 + Understanding + G176; ITU-R BT.1702-3 | EA IRIS README (BSD-3) | ISO 9241-391; Harding FPA, PEAT internals | 37 / 2 |
| D1 Baluster and mouldings | R8-D1-baluster-and-moulding-constructions.md | Gibbs 1732 (IA scan, public domain) | Ware, *American Vignola* Pt I (public domain) | a measured survey of real balusters | 25 / 0 (1 WEAK) |
| D2 Tower bell profile | R8-D2-tower-bell-profile.md | (none listed) Grimthorpe 1903 (Gutenberg) | EB 1911 "Bell" | an open modern true-harmonic profile table | 19 / 1 (1 WEAK) |
| D3 Drinking vessels | R8-D3-drinking-vessel-proportions.md (+ R8-D3-met-drinking-vessels-data.csv) | (none listed) Met Collection API (CC0), Parpinello et al. 18 (CC BY) | ISO 3591 via Wikipedia; Attwood et al. 12 | MPRG 1998 Guide; POTS 1983; ISO 3591 text | 18 / 3 (3 WEAK) |

### Key answers, sections B to D
- **B1.**
  - None of the listed kernel papers states a gradient bound. Hart gives Lip C_R = 3/(2R).
  - My bound: Lip f ≤ (max|dk/du|/σ)·(skeleton weight in support). Kernel maxima are tabulated.
  - Hart's Theorem 5 is misprinted; use the form from the proof.
- **B2.**
  - DC is always closed but not manifold.
  - Manifold DC guarantees manifold output under Prop. 1 (χ = 1, 0/2 face crossings).
  - MC33 handles two sheets per cell with face and interior tests, but the published tables have bugs (Custódio 13).
  - No method guarantees freedom from self-intersection.
- **B3.**
  - The QEM formulas.
  - Fold-over: test normal flips (penalise or reject).
  - QEM does not keep manifoldness, so add the A4 link condition.
  - Attributes: Garland-Heckbert 98 (Rⁿ) against Hoppe 99 (R³ projection).
  - Bone weights: clamp and renormalise (mine).
- **B4.**
  - Heat weights solve (−Δ+H)w = Hp: one factorisation.
  - BBW is a QP on a tet mesh.
  - Voxel binding leaks where parts touch.
  - Sources disagree on heat-weight robustness.
- **C1.**
  - DBM: p ∝ φ^η, giving dimension about 1.7.
  - η of 1-4 (Kim-Lin) against η ≈ 6 matching photos in 3D (Sañudo).
  - Sum probabilities in ascending order (rounding bug).
  - Glassner's segment and flash numbers.
- **C2.**
  - Curl-noise boundary ramp and Eq. 5.
  - Briggs (ISC3): F_b = g v d² ΔT/(4T_s).
  - Final rise 21.425F^(3/4)/u (F < 55) and 38.71F^(3/5)/u.
  - Gradual rise 1.60F^(1/3)x^(2/3)/u.
- **C3.**
  - Puffing frequency f ≈ 1.5/√D against Zukoski's 0.50√(g/D).
  - Heskestad L_f/D = 3.7Q*^(2/5) − 1.02. J11's 0.235 corresponds to 3.87.
- **C4.**
  - W/L = sin α.
  - d_max/d0 ≈ ½Re^(1/4).
  - Splash threshold K = 57.7.
  - Laan Padé law with A = 1.24.
  - Sources disagree on whether the stain equals D_max.
- **C5.**
  - WCAG: no more than 3 flashes per second, or under the thresholds. A general flash is a 10% change with the darker image below 0.80. Area: 0.006 sr = 25% of a 10° field (341×256 px; 21,824 px²).
  - Red flash (WCAG 2.2 note): R/(R+G+B) ≥ 0.8 and Δu′v′ > 0.2.
  - ITU: 20 cd/m² below 160; Michelson 1/17 at or above 160; 25% of the *screen*; 360/334 ms; 5 s caution.
- **D1.**
  - Gibbs: stone baluster 8 parts high by 2 wide (H = 4W); gap ½W; neck ½W; plinth and abacus 1 part each; belly at about 0.4H (from the plate).
  - Ware: W = H/3; centres about H/2; cap and base ¼H each; belly at H/3.
  - Gibbs's arc constructions: cyma (diagonal halved, intersecting arcs); ovolo centre; cavetto radius 6/5 of the fillet projection; torus a semicircle; scotia from two centres on a 3/7 split.
- **D2.**
  - Grimthorpe: D in 24 parts. Inside sweep a quarter ellipse, 14 × 6.
  - Sound-bow D/13 (D/12 heavy); waist D/39.
  - Inside shoulder diameter ½D at 0.583D.
  - Crown arc radius 16½-18 parts (≈0.71D); lip arc 3½-4 parts; pit ≥ 5/3 D.
  - EB1911's "edge D/10, height 12× thickness" disagrees.
- **D3.**
  - Tumbler and beaker H/D median 1.2-1.3; mug 1.25.
  - Goblet overall about 1.8 (historical) to 2.4 (ISO).
  - Goblet bowl H/maxD about 1.5; rim/max about 0.7; stem plus foot about 35% of the height; foot D ≈ bowl max D.
