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
(Filled in when complete; see below.)
