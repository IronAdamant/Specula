# R8-C1. Lightning: dielectric breakdown growth, branching angles, segment lengths, brightness and width

Round 8, Section C, item 1. Private reference notes, written in my own words. No code from sources. Page numbers are PDF pages of the copy named unless "printed p." is given. Anything marked **(mine)** is my own inference or arithmetic, not a source claim.

## Sources

| Tag | Role | Citation | Free legal copy | Licence |
|---|---|---|---|---|
| L1 | Listed | L. Niemeyer, L. Pietronero, H. J. Wiesmann, "Fractal Dimension of Dielectric Breakdown", *Physical Review Letters* 52(12):1033-1036, 19 March 1984. doi:10.1103/PhysRevLett.52.1033 (Crossref confirmed) | **None found.** APS paywall; no author or repository copy located. Content below comes through S1-S3, which describe it. | APS copyright |
| L2 | Listed | T. Reed, B. Wyvill, "Visual Simulation of Lightning", *Proc. SIGGRAPH '94* (21st annual conference), pp. 359-364, 1994. doi:10.1145/192161.192256 (Crossref confirmed) | ACM DL (https://dl.acm.org/doi/pdf/10.1145/192161.192256) returned an HTML page to my download, not the PDF; **not read**. Described through S2, S4 and L3 below. | ACM |
| L3 | Listed | T. Kim, M. C. Lin, "Fast Animation of Lightning Using an Adaptive Mesh", *IEEE TVCG* 13(2):390-402, March/April 2007. doi:10.1109/TVCG.2007.38 (Crossref confirmed) | Author copy on the UNC GAMMA project page: https://gamma.cs.unc.edu/FAST_LIGHTNING/lightning_tvcg_2007.pdf (13 PDF pages; the page says "Appearing in IEEE TVCG"). Raw: raw/round8/kimlin07.pdf | IEEE copyright; author-hosted preprint |
| S1 | Substitute for L1 (DBM rule and eta in 3D) | J. Sañudo, J. B. Gómez, F. Castaño, A. F. Pacheco, "Fractal dimension of lightning discharge", *Nonlinear Processes in Geophysics* 2(2):101-106, 1995. doi:10.5194/npg-2-101-1995 (Crossref confirmed) | https://npg.copernicus.org/articles/2/101/1995/npg-2-101-1995.pdf (scanned; I OCR'd it with tesseract; raw/round8/sanudo95-npg.pdf + .ocr.txt) | Copernicus article page: CC BY-NC-SA 2.5; PDF itself prints "© European Geophysical Society 1995" |
| S2 | Substitute for L1 and L2 (DBM description; Reed-Wyvill summary) | B. Bickel, M. Wicke, M. Gross, "Adaptive Simulation of Electrical Discharges", ETH Zurich Computer Graphics Laboratory, 2006 (venue is not printed on the PDF; not on Crossref) | https://cgl.ethz.ch/Downloads/Publications/Papers/2006/Bic06/Bic06.pdf (6 pages). Raw: bickel06.pdf | Author copy; no licence stated |
| S3 | Substitute for L2 (branch angles, segment lengths, measured data) | A. Glassner, "The Digital Ceraunoscope: Synthetic Thunder and Lightning, Part 1", *IEEE Computer Graphics and Applications* 20(2):89-93, March/April 2000. doi:10.1109/38.824552 (Crossref confirmed) | Author's site: https://www.glassner.com/wp-content/uploads/2014/04/CG-CGA-PDF-00-03-The-Digital-Ceraunoscope-1-Mar00.pdf (5 PDF pages = printed pp. 89-93). Raw: glassner00-ceraunoscope1.pdf | IEEE copyright; author-hosted |
| S4 | Substitute for L2 (later summary of the 16 degree rule and channel classes) | N. Reis, A. Ramires Fernandes, "Using a Space Colonization Algorithm for Lightning Simulation", *2022 International Conference on Graphics and Interaction (ICGI)*, 2022. doi:10.1109/ICGI57174.2022.9990427 (Crossref gives pp. 1-8; the PDF has 6 pages) | Universidade do Minho repository: https://repositorium.uminho.pt/server/api/core/bitstreams/1360691a-25e8-4eee-9643-137b8dd61ffb/content . Raw: uminho-spacecol-lightning.pdf | IEEE copyright line on p.1; repository copy |

## C1-1. The dielectric breakdown model (DBM, the "eta model")

As described by L3 (PDF 3, Sec. 4.1), S2 (PDF 2-3, Sec. 3.1) and S1 (PDF 1-2, Sec. 2), all citing L1:

1. Set up a grid holding an electric potential phi. The discharge (the growing pattern) is held at phi = 0; the opposite electrode is held at phi = 1. In L1's original set-up the seed is a single point at the centre and a surrounding circle is at phi = 1 (L3 PDF 3, Fig. 2a). For lightning L3 puts a small negative seed at the top and sets the bottom edge (ground) to phi = 1 (L3 PDF 3, Fig. 2b). S1 uses a box with phi = 0 on the top face (cloud base) and phi = 1 on the bottom face (ground), periodic side faces, and only the centre of the top face allowed to start the discharge (S1 PDF 1, Sec. 2).
2. Solve Laplace's equation (del^2 phi = 0) over the grid with the pattern and electrodes as fixed boundaries (L3 PDF 3, Eq. 1; S1 PDF 2, Eq. 1).
3. List the candidate cells: every cell adjacent to the pattern that is not yet in it (L3 PDF 3; S2 PDF 3 calls this set N).
4. Pick one candidate at random with probability p_i = phi_i^eta / (sum over candidates of phi_j^eta) (L3 PDF 3, Eq. 2; S2 PDF 3, Eq. 1; S1 PDF 2, Eq. 2). S2 draws it by giving each candidate an interval of [0,1] of that length and using a uniform random number (S2 PDF 3).
5. Add the chosen cell to the pattern (it becomes phi = 0, part of the boundary), update the candidate list, and repeat from step 2 (L3 PDF 3; S2 PDF 3).
6. Stop when the pattern touches the ground (L3 PDF 9, Sec. 6.3.1; S1 PDF 2).

**Fractal dimension.** L3 (PDF 2) and S2 (PDF 3) both say the discharge has a fractal dimension of "approximately 1.7", citing L1. I could not check L1's own figure or its conditions (2D, eta = 1) because L1 is not available (row in blocked-downloads).

**Eta.** L3 (PDF 3 Fig. 3 and PDF 10 Sec. 8): eta = 1 gives dense branching, and branching thins as eta rises. L3 cites Hastings that at eta = 4 the pattern becomes a non-fractal one-dimensional curve, so L3 treats the useful range as about 1 to 4.

**Disagreement on eta for real lightning (S1 against L3).** S1 grows 3D patterns on a 60 x 100 x 100 lattice and measures the fractal dimension D of the 3D pattern and D_p of its projection onto a vertical plane (S1 PDF 4, Tables 1-2):

| eta | 2 | 3 | 4 | 6 | 9 |
|---|---|---|---|---|---|
| D (3D), 60x100x100 | 2.17 ± 0.04 | 1.90 ± 0.03 | 1.73 ± 0.03 | 1.51 ± 0.02 | 1.34 ± 0.02 |
| D_p (vertical projection) | 1.88 ± 0.02 | 1.69 ± 0.02 | 1.54 ± 0.02 | 1.34 ± 0.01 | 1.21 ± 0.01 |

Photographs of lightning give D_p about 1.34 (Tsonis, as quoted by S1 PDF 1 and PDF 5). S1 matches that at eta about 6, which corresponds to a 3D dimension of about 1.51, and concludes that lightning is "considerably more deterministic" than eta = 2 suggests (S1 PDF 5, Sec. 3-4). The patterns at eta = 6 and 9 are still fractal in S1. So S1 (3D, cloud-to-ground) places the realistic eta above L3's 1 to 4 range (taken from a 2D result). **(mine)** The two are not strictly in conflict, since Hastings' transition is for a different (2D) geometry, but anyone tuning eta should know the sources point in different directions.

**How S1 measures D (S1 PDF 4).** Radius of gyration R_g of the pattern against point count N; R_g grows as N^beta, and D = 1/beta. Finite-size check on a 30 x 50 x 50 lattice gave nearly the same values (Tables 1b, 2b).

**Failure cases and practical notes for the DBM.**
- Cost: the Laplace solve has to be redone after every added cell. S1 used over-relaxation with omega about 1.6 (about 15 iterations to a relative error below 0.001; omega = 1 needs two to three times as many) and first relaxes a small cube around the new point (S1 PDF 2). S1's runs took 5.5 to 10 CPU hours for 900-1600 points at eta = 2 (S1 PDF 2). L3 reports its octree solver 22 times faster than the regular grid at 128^3 and 150 times faster at 256^3 (L3 PDF 7, Fig. 12 caption), and says the adaptive solver can be no better, or slightly worse, than the regular grid when boundaries are complex (L3 PDF 11, Sec. 10).
- **Rounding bug (S1 PDF 2):** when normalising the probabilities, the sum in the denominator must be added in ascending order; otherwise the rounding errors are large, even for small eta. **(mine)** This matters most at high eta, where phi^eta for far-away cells underflows towards zero.
- Grid artefacts: visible regularity when grid cells are much bigger than pixels; L3's fix is to place each node at a random point inside its cell rather than at the cell centre (L3 PDF 9-10, Sec. 7).

## C1-2. Repeated strokes and steering (L3)

- Dart leaders: L3 replaces Laplace with a Poisson equation, del^2 phi = -4 pi rho, and after a bolt deposits positive charge density rho along its channel, so the next bolt is drawn to roughly the same path; rho is cleared and re-laid after each bolt (L3 PDF 4, Sec. 4.2, Eqs. 3-4). L3's results use rho = 1000/(4 pi) along the main channel, a 512 x 512 grid and eta = 1 unless stated otherwise (L3 PDF 10, Sec. 9).
- Repelling the bolt: set an obstacle's interior to phi = 0 (same charge as the bolt) and do not let cells next to it become candidates (L3 PDF 10, Sec. 8).
- Glassner's timing figures (S3 printed p. 89-90 = PDF 1-2): the stepped leader advances about 50 m in about 1 microsecond, then pauses about 50 microseconds; a new dart leader follows after about 30 ms; S3's system defaults to four flashes with a mean gap of 30 ms (S3 printed p. 92 = PDF 4).

## C1-3. Branching angles and segment lengths (geometric models)

- **The 16 degree figure, with different meanings in different sources:**
  - S3 (printed p. 90 = PDF 2) credits Hill (measurements of nine digitised strokes) with the finding that the mean absolute angle between adjacent short segments is about 16 degrees, and that this small-scale wiggle does not depend on height. On the next page S3 says its own 40 Apollo-programme strokes "agreed with Hill's value of a 16-degree mean branching angle" (printed p. 91 = PDF 3).
  - L3 (PDF 2, Sec. 2) says Reed and Wyvill based their model on the observation that most branches deviate by an average of 16 degrees from their parent branches, generating segments with angles normally distributed around 16 degrees.
  - S4 (PDF 2) says each segment and each branch turns on average 16 degrees from the one before.
  - So the same number is reported both as the angle between neighbouring segments of one channel and as the angle of a branch from its parent. I could not read L2 to settle which Reed and Wyvill meant.
- **Ribner and Roy random walk (via S3 printed p. 90 = PDF 2):** each new point lies in a cone around the average direction of the last few segments. Their best fit averaged the last four strokes, added a downward bias of 1/20 of the random direction's strength, and used a mean cone half-angle of 30 degrees.
- **Glassner's two-pass method (S3 printed pp. 91-92 = PDF 3-4):**
  - Pass 1 (skeleton): from the top, draw segment lengths from a height-dependent probability density, point each new segment within a 30 degree cone around the average of the last four, push it slightly downward and add wind shear. A branch is simply a second segment started from the same point. Stop when any chain reaches the ground.
  - The density for slab i (eight height slabs, 0 to 6 km) is h_i = a_i/y^2 + b_i/y + c_i + d_i*y + e_i*y^2, with coefficients in S3 Table 1 (printed p. 91 = PDF 3; I read the equation form from the page image because the text layer is garbled). Densities are linearly interpolated between the two nearest heights. Per the Fig. 1 caption, y is the segment length as a percentage of the total stroke length. The fit is within 7% maximum absolute relative error.
  - Segments get shorter nearer the ground, so the skeleton branches more there (printed p. 91).
  - Pass 2 (fine detail): break each skeleton segment into 3-5 m zig-zags. Walk along a double cone around the segment in random steps with a mean of 3 m, perturbing from the previous point. The cone radius at the middle is 25% of the skeleton segment length (printed p. 92 = PDF 4).
  - Why 3-5 m: at 3 km a 3 m segment subtends about 0.06 degrees, below the about 0.08 degree pixel of a 1024-pixel image with a 40 degree half-angle (printed p. 91 = PDF 3).
- **Length ratios between parent and child branches:** no source I read gives a numeric ratio of branch length to parent length. S3's per-height length densities are the nearest quantitative data. Not found.

## C1-4. Brightness and width

- **Channel classes (L3 PDF 9, Sec. 6.3.2):** L3 builds a tree from the growth record. The main channel is the cell that reached the ground plus all its ancestors. Each subtree hanging off the main channel gets one bright "secondary channel", chosen as its longest path (L3 says this choice is for looks, and that how nature picks it is unclear). Other long paths above a user cut-off are also promoted; everything else is a dim side channel.
- **Wattage:** L3 quotes main-channel data of 3.9 x 10^5 to 1.3 x 10^6 W/m (from Rakov and Uman; L3 prints the two figures high-to-low). L3 found no data for secondary or side channels and used hand-tuned values. They also did not use the real main-channel figure, because without tone mapping it would wash out the whole image (L3 PDF 9).
- **Width:** L3 says the plasma channel is only a few centimetres across even for lightning. Its hypothesis is that a bolt looks thick because the bright glow saturates vision. So it draws thin lines and gets the apparent width from the glow (L3 PDF 9, Sec. 6.3). S3 also says the step-leader channel is only a few centimetres in diameter (printed p. 89 = PDF 1). S4 widens only the main channel with a dilation pass, then blurs everything with a small Gaussian (S4 PDF 6).
- **Glow kernel (L3 PDF 8-9, Sec. 6.1-6.2):** an atmospheric point spread function, a Legendre-series expansion of Henyey-Greenstein scattering (after Narasimhan and Nayar), sampled through a pinhole model (kappa = 0.025 m, bolt 2000 m away) into a convolution kernel. The settings are in Table I (PDF 10). Two practical points: subtract the smallest non-zero kernel value, or the kernel's outline shows in the image; the kernel is isotropic, so it can be applied as two 1D passes (PDF 9-10).
- **Printing error I found (mine, L3 PDF 9, Eq. 10):** the printed formula for the image-plane distance nu takes the square root of the *sum* of the x and y offsets, without squaring them. Checked on the page image. A distance has to use the squared offsets, so the equation as printed is wrong; the text around it does not discuss this.
- **Taper:** L3 notes that Maya's lightning lacks "the glow and tapering brightness of the photo" for side branches (PDF 11, Fig. 15 caption). Neither L3 nor S3 gives a numeric falloff along a branch. **No numeric brightness-taper or width-taper law found** in the sources I could read. L2 is the likely source, and it was not readable.

## Known problems in later work

- S2's reference [20] gives S1 the title "Fractal to nonfractal phase transition in the dielectric breakdown model"; the real title of NPG 2:101-106 is "Fractal dimension of lightning discharge" (S1 PDF 1; Crossref). It is a citation error in S2 (S2 PDF 7).
- L3's own stated limits (PDF 11, Sec. 10): no tone mapping; the glow kernel assumes a homogeneous medium and a single 2 km distance for the whole bolt (PDF 9).
- L3's Eq. 10 misprint (above).

## Not found or not verified

- L1's own text (fractal dimension value, set-up details): no free legal copy.
- L2's own text: download blocked; summaries via L3, S3, S4 disagree on what the 16 degrees measures.
- Numeric parent-to-child branch length ratio; numeric brightness and width taper.
