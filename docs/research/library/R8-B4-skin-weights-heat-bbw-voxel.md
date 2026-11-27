# R8-B4. Skin weights from the skeleton alone

Round 8, section B. Prepared 2026-11-27; access date 2026-11-27.

**The question.** For each method: the equations it solves; whether it needs only the mesh and the bones; its cost in terms of vertex count; and its known artefacts (weights leaking between a hand and a thigh).

## Sources

| # | Citation | Free legal copy | Licence | Read? / page scheme |
|---|---|---|---|---|
| L1 | Baran I., Popović J. (2007). "Automatic rigging and animation of 3D characters." *ACM Trans. Graph.* 26(3), Article 72. doi:10.1145/1276377.1276467 | MIT Open Access (author's final manuscript): https://dspace.mit.edu/handle/1721.1/100396 | "Made available in accordance with the publisher's policy" (MIT cover page) | Yes. **PDF pages** (PDF 1 is the MIT cover) |
| L2 | Jacobson A., Baran I., Popović J., Sorkine O. (2011). "Bounded biharmonic weights for real-time deformation." *ACM Trans. Graph.* 30(4) (Proc. SIGGRAPH 2011), Article 78. doi:10.1145/2010324.1964973 | Author/IGL copy: https://igl.ethz.ch/projects/bbw/bounded-biharmonic-weights-siggraph-2011-jacobson-et-al.pdf | Author copy; © ACM | Yes. PDF pages (8) |
| L3 | Dionne O., de Lasa M. (2013). "Geodesic voxel binding for production character meshes." *Proc. SCA 2013*: 173-180. doi:10.1145/2485895.2485919 | **None found.** ACM is paywalled. The Eurographics Digital Library record has no open file. A scispace.com mirror exists but its status is unclear, and it returned an HTML page to the box rather than a PDF | - | **No.** Only the abstract and introduction text shown in the search result was seen |
| Sub-1 | **Substitute for L3 (vendor documentation of the same method, from the authors' employer):** Autodesk, *Maya 2025 Help: Bind Skin Options* (Geodesic Voxel bind method). https://help.autodesk.com/cloudhelp/2025/ENU/Maya-CharacterAnimation/files/GUID-CF2C698A-44BB-4CA0-BCB9-DB36500DA812.htm | Publisher site (free) | © Autodesk; documentation terms | Yes (saved as raw/round8/maya-bind-skin-options.html). Cited by section |

## 1. Heat-diffusion weights (L1 §4, PDF 7; equation checked against the text)

- **Idea:**
  - Treat the character as an insulated heat-conducting body.
  - Hold bone i at temperature 1 and all other bones at 0.
  - The equilibrium temperature at a surface vertex is bone i's weight there.
- **Simplification:** solving in the volume would need a tetrahedralisation and would be slow, so L1 solves on the **surface only** and adds heat from the nearest bone at each vertex (PDF 7).
- **Equation:** at equilibrium, ∂wⁱ/∂t = Δwⁱ + H(pⁱ − wⁱ) = 0. Equivalently:

  **(−Δ + H) wⁱ = H pⁱ** (eq. 1), where:
  - Δ is the discrete surface Laplacian (cotangent formula);
  - pⁱ_j = 1 if bone i is the nearest bone to vertex j, and 0 otherwise;
  - H is diagonal, with **H_jj = c/d(j)²**, d(j) being the distance from vertex j to its nearest bone. **H_jj = 0 if the segment from the vertex to that bone leaves the character's volume** (tested on a precomputed distance field);
  - if k bones are equidistant, then pⁱ_j = 1/k for each of them and H_jj = kc/d(j)².
- **The constant c:** c ≈ 0.22 gives transitions similar to the volume solution. Pinocchio uses **c = 1** ("anisotropic heat diffusion") because it looks more natural (PDF 7).
- **Partition of unity is automatic:** summing eq. 1 over i gives (−Δ + H)Σwⁱ = H·1, so Σwⁱ = 1 (PDF 7).
- **Cost:** the matrix −Δ + H is the same for every bone, so factor it **once** (sparse Cholesky; Pinocchio uses TAUCS), then back-substitute once per bone (PDF 7). That is one sparse n×n factorisation (n = surface vertices) plus m solves.
- A rejected shortcut: forcing unambiguous vertices to weight 1 gave negligible speed-up and "occasional artifacts" (PDF 7).
- **Needs:** the surface mesh, the embedded bones, and a **well-defined interior** for the visibility test. L1 lists removing that assumption as future work (PDF 8).
- **Known artefacts:**
  - from L1 (PDF 8): a rubbery look for dresses and armour (the method ignores material), and weak results at hips and at the shoulder/neck, where hand-tuned weights are better. LBS's own faults (knee, hip) are also visible (Fig. 11, PDF 7);
  - from L2 (PDF 4): heat weights are **not smooth at the handles**;
  - from Sub-1: "Heat Map binding often fails on real-world meshes".

**Leaks (hand to thigh).** Diffusion runs only along the surface's connectivity. A hand vertex whose nearest bone is a thigh bone gets heat from it only if the straight segment to that bone stays inside the volume (H_jj = 0 otherwise). So on a separate hand mesh that merely touches the thigh, the leak is blocked by connectivity plus the visibility test. **My inference:** if the hand and thigh are *welded* into one surface (as one-surface meshing would do), diffusion can cross the weld; the visibility test then only gates the nearest-bone source term.

## 2. Bounded biharmonic weights (L2 §3, PDF 2-5)

- **Problem (eqs. 2-6, PDF 2):**

  minimise Σ_j ½∫_Ω (Δw_j)² dV, subject to:
  - w_j = δ_jk on handle H_k;
  - w_j linear on cage faces;
  - **Σ_j w_j = 1** (partition of unity);
  - **0 ≤ w_j ≤ 1**.

  The Euler-Lagrange equation without the bounds is the biharmonic equation Δ²w_j = 0 (PDF 3).
- **Properties (PDF 3):**
  - smooth;
  - non-negative;
  - shape-aware, meaning the weights fall off with distance *inside the shape* rather than Euclidean distance;
  - local and sparse;
  - **no local maxima**. This is observed in all tests and is "likely" helped by the bounds, but **not proven**.
- **Without the bounds:** biharmonic weights go negative or above one, have maxima away from the handles, and give non-local influence (PDF 4, Fig. 4).
- **Discretisation (PDF 4):** linear FEM on a triangle mesh (2D) or **tetrahedral mesh (3D, built with TetGen)** that includes points sampled along the bones. For example, the Armadillo (43,234 vertices plus 120 bone samples) gave a 46,898-vertex tet mesh.
- **Solver:** sparse QP (MOSEK) for all handles at once.
- **Faster variant (PDF 4-5):** drop the partition of unity, solve each handle's QP separately, then normalise per vertex. Results are "mostly negligible" differences and 50× faster for the Gargoyle's 7 handles (PDF 5).
- **Cost (Table 2, PDF 6):** bind time per handle ranged from 0.09 s to 12.09 s. The slowest case had |Ω| = 142,073 elements. The QP time is **superlinear** in the number of unknowns (PDF 4).
- **Needs:** the shape's volume (a tet mesh of the interior) and the handles (bones, as sampled segments). Once the volume exists, any embedded object, even polygon soup, can be deformed (PDF 6).
- **Limitations (PDF 6):**
  - bind-time optimisation cost;
  - 3D needs a volume discretisation;
  - no linear precision, so the method is unsuitable for pure cage interpolation.

**Leaks.** Because L2 solves in the volume, the weights follow distance inside the shape. Two parts that touch in the volume (a hand resting on a thigh in the rest pose) become one connected region there. **My inference:** influence can then cross the contact, as in any volume method. L2 does not discuss this case.

## 3. Geodesic voxel binding (L3 not read; Sub-1 and the L3 abstract)

**From the L3 abstract (as shown on the ACM and search pages):**
- Voxelise the rest-pose mesh.
- Compute the geodesic distance from the voxels on each bone to all non-exterior voxels.
- Weights then come from a simple fall-off.
- It is designed for production meshes that may be **non-manifold, non-watertight, self-intersecting or in several components**, and weights can be adjusted at pose time without re-computation.
- **The exact fall-off and the voxelisation details are not verified** (paper not read).

**From Sub-1 (Maya docs):**
- Parameters: *Falloff* (0 smoother, 1 more rigid; **default 0.2**), *Resolution*, and *Validate voxel state*.
- **Leak artefact, stated directly:** at coarse resolution, "knee and hand voxels become connected", so moving the arm deforms the knee. The fix is a finer voxel resolution, trading bind time for quality.

**Needs:** the mesh (any quality) and the bones. **Cost:** stated only qualitatively ("interactive rates", L3 abstract). It grows with voxel resolution (Sub-1).

## 4. Summary table

| Method | Solves | Needs only mesh + bones? | Cost (n = vertices) | Leak / artefacts |
|---|---|---|---|---|
| Heat (L1) | (−Δ + H)wⁱ = Hpⁱ on the surface | Yes, plus a closed interior for the visibility test | 1 sparse factorisation (n×n) + m back-substitutions | Not smooth at bones; hips/shoulders; leaks only through connectivity or a visible nearest bone; "often fails on real-world meshes" (Sub-1) |
| BBW (L2) | min ½∫(Δw)² with bounds and partition of unity (QP) | No: needs a tet mesh of the volume | Superlinear QP; 0.1-12 s per handle (Table 2) | Volume contact merges regions (my inference); no proof of no local maxima |
| Geodesic voxel (L3, via Sub-1) | geodesic distance in voxels + fall-off | Yes; tolerant of bad meshes | grows with voxel resolution | Coarse voxels connect hand and knee (Sub-1) |

**For one-surface bodies (my synthesis).** The engine's body is one welded surface built from the same skeleton. Pure surface diffusion (L1) loses its natural barrier wherever limbs are welded. A volume or voxel method (L2, L3) leaks wherever parts touch in the rest pose, unless the voxel size is below the gap. Either way, the rest pose should keep limbs apart by more than one voxel or element.

## Disagreements
- **Smoothness of heat weights:** L2 says heat weights are not smooth at handles; L1 does not discuss smoothness at bones.
- **Robustness of heat weights:** Sub-1 says heat binding "often fails on real-world meshes"; L1 reports success on its 16 test characters (PDF 8).

## Not obtained or not verified
- L3 full text: no free legal copy, so its equations and pages are not given.
- The size of L1's c relative to the mesh scale is as stated (c/d²). Whether c is scale-free: yes, because Δ and H both have units of length⁻² (PDF 7).
