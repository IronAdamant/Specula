# I2 — Fitting one garment or armour set to many bodies: methods, parameters, measured fabric mechanics, real-time cost, licences

Changelog
- 2026-11-17 (AEDT): First version. Grok CLI pass (grok-4.6, xhigh, live search; run in /tmp/grokpass/i2b) was used only to find leads. Every row below was re-opened in its primary source (talk slides/OCR, video transcript, official docs, paper PDF or arXiv abstract, licence file) on 2026-11-17. Grok leads that could not be confirmed are listed under "Not found", not in the tables.

Scope: methods that put one authored garment/armour piece on many body shapes (games, platforms, research), measured fabric mechanics usable as sim parameters, stated real-time costs, and licence status. Pictures and wording are not copied; short quotes are marked as quotes.

Licence flags used: **[NC]** non-commercial only · **[ND/no-redistribution]** · **[NO-LICENCE]** no licence stated · **[COPYLEFT]** · **[CLOSED]** proprietary method (description only, no code).

## 1. Shipped-game and platform pipelines

| # | Title / product (year) | Method: how one piece fits many bodies | Inputs / parameters | Stated limits / what went wrong | Licence | Source URL · accessed |
|---|---|---|---|---|---|---|
| G1 | The Sims 4 CAS — "Innovations in The Sims 4 Character Creator", Sri Nair (EA), GDC 2015 | Body shape edits are carried to clothing through deformation maps (DMAPs) in addition to blend shapes, so clothing follows body changes | DMAP per body region; artist-painted mask of "loose fitting" vertices that switches to a separate loose-fit DMAP | Talk: a DMAP built on the unclothed body "cannot map well to a loose fitting clothing" (pinching at the middle of the garment); fixed with a second DMAP plus painted mask | [CLOSED] | https://gdcvault.com/play/1022086/Innovations-in-The-Sims-4 ; transcript https://www.youtube.com/watch?v=s7R_HHxCokU · 2026-11-17 |
| G2 | Destiny — Scott Shepherd (Bungie technical art lead), GDC 2014 | Gear authored per gender; an early semi-automatic path took finished male gear and used wrap deformers + blend shapes to make the female version | Wrap deformer, blend shapes | Talk: the auto path merged all "bits" into one mesh before conforming, losing the modular system's flexibility; "to meet our high quality bar" artists spent "longer and longer" hand-fixing the result, so the approach was dropped | [CLOSED] | https://www.youtube.com/watch?v=sSSMC4O6J8Q ; https://www.polygon.com/2014/3/21/5533684/bungies-destiny-extensive-character-customization · 2026-11-17 |
| G3 | Kingdom Come: Deliverance — "Adaptive Clothing System", Tomas Barak (Warhorse), GDC Europe 2015 | Layer order body → cloth → chain mail → plate → decoration; outer layers cover inner ones (influence matrix). Each piece has an artist-made base shape and a "shrunk" shape (base + shrink morph). On equip, rays are traced per triangle between layers in bind T-pose; each vertex gets weight W blending base (W=1, no adaptation) and shrunk (W=0, maximal adaptation); weights smoothed on the vertex-edge graph. Fat/slim body proportions are stored as morph variations "applied once" | Base mesh, shrink morph, variation morphs, alpha mask (read during trace), n random barycentric samples per triangle, hash grid | Drawbacks slide: only layered shapes supported; one draw call per piece; "not instant" — correct shape appears after a fraction of a second; future work: BVH built offline, refit online | [CLOSED] | https://archive.org/stream/GDCEU2015Barak/GDCEU2015-Barak_djvu.txt (slide OCR) · 2026-11-17 |
| G4 | Roblox Layered Clothing (cage mesh deformer; Studio beta 2021-04-30; fit update announced 2025-03-14) | Each accessory has an inner cage, an outer cage and a render mesh. Roblox pairs vertices of two cages "by their UV coordinates" and reshapes the render mesh so its cage matches the target cage — so any item fits any body that shares the cage UV layout. Deformation is "static": runs once, then normal skinning/physics | Inner/outer cage, shared cage UV layout, WrapLayer/WrapTarget, layer order, WrapLayer.Puffiness | 2025 post: old algorithm used a radial basis function for both cage alignment and fit, which "has a tendency to overshoot"; cage alignment moved to a linear deformer (RBF kept for clothing fit), both inner and outer cages now sampled, extra compression pass to stop layers looking "thick and puffy". "Performance is expected to be the same or better" (no ms given) | Platform feature; assets under Roblox terms; docs public. [CLOSED] engine code | https://create.roblox.com/docs/art/modeling/cage-meshes ; https://devforum.roblox.com/t/cage-mesh-deformer-studio-beta/1196727 ; https://devforum.roblox.com/t/coming-soon-improved-layered-clothing-fit/3550013 · 2026-11-17 |
| G5 | Second Life Fitted Mesh (needs viewer ≥ 3.7.0.286015; skeleton files updated 2014-04-15) | Clothing is skinned to the avatar's collision bones as well as normal bones; Body Shape sliders move those bones, so clothing follows each avatar's shape | Collision bones: original 19 (PELVIS, BELLY, CHEST …) + 7 added (BUTT, LEFT_PEC, RIGHT_PEC, LEFT/RIGHT_HANDLE, LOWER_BACK, UPPER_BACK); creator chooses which to weight | Wiki tip: use alpha masks to hide body under mesh to avoid clipping. BELLY, BUTT and PECs are also driven by avatar physics | Platform feature; content creator-owned | https://wiki.secondlife.com/wiki/Mesh/Rigging_Fitted_Mesh · 2026-11-17 |
| G6 | MetaHuman wardrobe (Epic, current docs) | Wearing an outfit "will cause it to automatically resize to fit the current body", and it keeps resizing if body shape changes | Body shape; outfit asset | "Short delay" the first time an item is worn while assets are prepared (no ms given) | Unreal/MetaHuman EULA. [CLOSED] | https://dev.epicgames.com/documentation/en-us/metahuman/hair-and-clothing-tools · 2026-11-17 |
| G7 | Cyberpunk 2077 "garment support" (community reverse-engineering, not a CDPR talk) | Engine squashes inner garments under outer ones so pants tuck into boots and shirts under jackets. Order from a garmentScore built from mesh component prefixes (h0_, t0_, s0_, l0_ …) and size tags (wiki example: Tight = −1000, XLarge = +2000; prefix t0_ = +30) | Morph-target "parameters" on the mesh (appear as shape keys in Blender); prefixes; tags | Wiki: only works on garments equipped through the game's transaction system — not on NPCs "as of October 2024" | Game [CLOSED]; wiki is community documentation | https://wiki.redmodding.org/cyberpunk-2077-modding/for-mod-creators-theory/3d-modelling/garment-support-how-does-it-work · 2026-11-17 |
| G8 | Rumbleverse — "Building Champions: Character Art Pipeline", Steven Kosanovich (Iron Galaxy), GDC 2023 | Accessories are built to be "stretched to six different body shapes", from very thin to short and round | Six body shapes | Vault abstract only; detailed method not confirmed | [CLOSED] | https://www.gdcvault.com/play/1028733/Building-Champions-Character-Art-Pipeline · 2026-11-17 |
| G9 | APB customisation — Maurizio Sciglio & Simon Taylor (Realtime Worlds), GDC 2010 | Body/face via standard additive morphs. Clothing is either a morph item or an extra-mesh item, merged with the base mesh and its texture combined into one ("One mesh to rule them all!") with mesh culling and a shared UV layout | Morph targets, UV layout, cull masks | Goal stated as single draw call per character, render/memory cost independent of item count; slides list heterogeneous surfaces (clothing, hair, skin) and complex masking as difficulties | [CLOSED] | https://media.gdcvault.com/gdc10/slides/Sciglio_Maurizio_APBCustomisationSystem.pdf · 2026-11-17 |
| G10 | Spider-Man (Insomniac) — "Realtime Cloth Solutions", Sophie Brennan, GDC 2019 | Compares hand animation, extra joints, baked (pre-rendered) sim and real-time sim for cloth on characters; real-time chosen | — | Slide p. 11: baked sim "can't adapt well to gameplay or real-time changes like character costumes on different body types". Real-time cons: micro-wrinkles out of scope, "Expensive to run", "lacks self-collision" | [CLOSED] (Havok middleware) | https://media.gdcvault.com/gdc2019/presentations/Brennan_Sophie_Realtime_Cloth_Solutions.pdf · 2026-11-17 |
| G11 | Assassin's Creed Unity / Far Cry 4 cloth — Alexis Vaisse (Ubisoft), GDC 2015 | Cloth is skinned first; each simulated vertex may move only inside a sphere around its skinned position, radius from painted vertex colour. A low-res sim mesh drives a high-res visual mesh ("mapping") | Vertex-paint radius, sim mesh, visual mesh | Simulating the high-res mesh is "too costly" and looks "too silky, too light"; cloth is usually on the frame's critical path | [CLOSED] | https://archive.org/download/GDC2015Vaisse/GDC2015-Vaisse_djvu.txt ; https://www.gdcvault.com/play/1022421/Ubisoft-Cloth-Simulation-Performance-Postmortem · 2026-11-17 |

## 2. Research methods

| # | Paper (venue, year) | What it does for "one garment, many bodies" | Inputs / parameters | Stated limits | Code / data licence | Source URL · accessed |
|---|---|---|---|---|---|---|
| R1 | Brouet, Sheffer, Boissieux, Cani — Design Preserving Garment Transfer (ACM TOG 31(4) art. 36, SIGGRAPH 2012) | Automatic 3D analogue of pattern grading: transfers a garment from a source to a target body of different proportions while preserving design, fit and physical plausibility; outputs graded patterns | Source garment + source/target bodies | Only abstract-level check (HAL page bot-walled) | Paper ACM; no code found | https://doi.org/10.1145/2185520.2185532 · 2026-11-17 |
| R2 | Guan et al. — DRAPE: DRessing Any PErson (ACM TOG 31(4) art. 35, 2012) | Learned model from physics sims that splits shape-induced and pose-induced clothing deformation and dresses any body from a parametric model | Body shape and pose parameters | Abstract-level only | Paper ACM; no public code found | https://doi.org/10.1145/2185520.2185531 · 2026-11-17 |
| R3 | Pons-Moll et al. — ClothCap (ACM TOG 36(4) art. 73, 2017) | Captures garments from 4D scans as separate meshes on a body model and retargets them to new body shapes | 4D scan sequences | Abstract-level only | Project page; code not confirmed | https://doi.org/10.1145/3072959.3073711 ; https://clothcap.is.tue.mpg.de/ · 2026-11-17 |
| R4 | Ma et al. — CAPE (CVPR 2020) | Conditional Mesh-VAE-GAN learns clothing as an extra term on SMPL, conditioned on pose and clothing type, to "dress different body shapes" | SMPL pose/shape, clothing type | Clothing stays on SMPL topology (displacements) | **[NC]** Data & Software licence: non-commercial research/education/art; commercial, pornographic, military and surveillance use prohibited | https://arxiv.org/abs/1907.13615 ; https://cape.is.tue.mpg.de/license.html · 2026-11-17 |
| R5 | Patel, Liao, Pons-Moll — TailorNet (CVPR 2020) | Predicts garment deformation from pose, shape and style; low-frequency MLP + mixture of shape-style-specific high-frequency pose models with a narrow kernel so wrinkles are not averaged out | SMPL pose/shape; garment style subspace; 55,800-frame dataset | Abstract: joint learning of the three factors smooths fine wrinkles (motivation for the split); trained on static poses | **[NC]** LICENSE.md (© 2019 Pons-Moll, MPG): non-commercial research/education/art only; needs SMPL (also NC) | https://arxiv.org/abs/2003.04583 ; https://github.com/chaitanya100100/TailorNet/blob/master/LICENSE.md · 2026-11-17 |
| R6 | Bhatnagar et al. — Multi-Garment Net (ICCV 2019) | Garments layered on SMPL from 1–8 video frames; garments can be transferred to new body shapes and poses; digital wardrobe of 712 garments in correspondence | Video frames, garment templates | Abstract-level | Release announced in abstract; licence file not checked | https://arxiv.org/abs/1908.06903 · 2026-11-17 |
| R7 | Tiwari et al. — SIZER (ECCV 2020) | SizerNet predicts clothing as a function of body shape and garment size; dataset ~2,000 scans of 100 subjects in various sizes | Body shape, garment size label | Abstract: no prior method predicted deformation as a function of garment size | Licence not checked | https://arxiv.org/abs/2007.11610 · 2026-11-17 |
| R8 | Gundogdu et al. — GarNet (ICCV 2019) | Two-stream network fits a garment template to a body; physics-inspired loss | Garment template, body point cloud, optional cut-pattern parameters | Abstract: points on average < 1 cm from PBS, 100× faster | Licence not checked | https://arxiv.org/abs/1811.10983 · 2026-11-17 |
| R9 | De Luigi et al. — DrapeNet (CVPR 2023) | One network drapes many garments (garments as unsigned distance fields; latent-conditioned deformation field); self-supervised | Garment latent code, SMPL body | Abstract: earlier self-supervised methods trained one network per garment | **[COPYLEFT]** GPL-3.0 (GitHub API); SMPL dependency **[NC]** | https://arxiv.org/abs/2211.11277 ; https://github.com/liren2515/DrapeNet · 2026-11-17 |
| R10 | Santesteban, Otaduy, Casas — SNUG (CVPR 2022) | Self-supervised dynamic garment deformation on parametric bodies; implicit integration recast as an optimisation used as training loss | Garment template, material model, body motion | Abstract-level | Licence not checked | https://arxiv.org/abs/2204.02219 · 2026-11-17 |
| R11 | Bertiche, Madadi, Escalera — PBNS (ACM TOG 40(6), 2021) | Unsupervised pose-space deformation for skinned garments; multiple layers; "unsupervised outfit resizing" for custom avatars | Rigged garment, body | Abstract-level | **[NO-LICENCE]** GitHub API shows no licence on the repo | https://arxiv.org/abs/2012.11310 ; https://github.com/hbertiche/PBNS · 2026-11-17 |
| R12 | Abdrashitov, Raichstat, Monsen, Hill (Epic Games) — Robust Skin Weights Transfer via Weight Inpainting (SIGGRAPH Asia 2023 Tech. Comm.) | Two stages: copy body skin weights only where correspondence is high-confidence, then inpaint the rest; aimed at non-skin-tight garments | Source body mesh + weights, target garment | Abstract: existing transfer "struggle[s] with non-skin-tight garments" | MIT (GitHub API) — commercial-friendly | https://doi.org/10.1145/3610543.3626180 ; https://github.com/rin-23/RobustSkinWeightsTransferCode · 2026-11-17 |
| R13 | Wang, Hecht, Ramamoorthi, O'Brien — Example-Based Wrinkle Synthesis for Clothing Animation (ACM TOG 29(4) art. 107, 2010) | Per-joint precomputed high-res wrinkle examples blended at runtime on top of a coarse sim | Wrinkle database per joint, coarse mesh 600–800 vertices | p. 7: "not suitable for clothing that is loosely fit to the human body, such as skirts and capes"; result has more, more static wrinkles than the high-res sim | Paper ACM; no code found | https://graphics.berkeley.edu/papers/Wang-EBW-2010-07/ · 2026-11-17 |
| R14 | Cage coordinates: Ju, Schaefer, Warren — Mean value coordinates for closed triangular meshes (ACM TOG 24(3) 2005); Joshi et al. — Harmonic coordinates (ACM TOG 26(3) 2007); Lipman, Levin, Cohen-Or — Green coordinates (ACM TOG 27(3) 2008) | The maths behind cage/wrap deformers like Roblox's: a closed cage drives every point of a mesh inside it | Cage mesh | Bibliographic check only (DOIs resolve); limits not confirmed in the papers | Papers ACM | https://doi.org/10.1145/1073204.1073229 ; https://doi.org/10.1145/1275808.1276466 ; https://doi.org/10.1145/1360612.1360677 · 2026-11-17 |

## 3. Measured fabric mechanics (usable as simulation parameters)

### 3a. Kawabata KES-FB — Ramaiah, Radhalakshmi, Nachane, "Comparative Analysis of Low-Stress Mechanical Properties of Woven and Knitted Apparel Fabrics", fibre2fashion article (undated; PDF created 2011), Table 3, PDF p. 5

Population: one fabric lot each — A knitted innerwear single jersey 100 % cotton; B knitted outerwear single jersey 95 % cotton / 5 % polyurethane; C woven plain shirting 100 % cotton; D woven plain suiting 100 % cotton; all dyed, unfinished. Each value is the mean of 10 tests on 5 specimens; **no SD or range reported**. Conditioning 65 ± 2 % RH (temperature OCR garbled). **Units are not printed in Table 3**; the units column below gives the usual KES-FB units for each symbol (convention, not from this paper). Licence: **[NO-LICENCE]** trade-site PDF, no reuse licence stated.

| Symbol (property) | Usual KES unit (not printed) | A knit CO | B knit CO/PU | C woven shirt CO | D woven suiting CO | Source URL · accessed |
|---|---|---|---|---|---|---|
| LT (tensile linearity) | — | 0.632 | 0.849 | 0.597 | 0.667 | https://static.fibre2fashion.com/articleresources/PdfFiles/60/5963.pdf · 2026-11-17 |
| WT (tensile energy) | gf·cm/cm² | 32.78 | 6.72 | 9.91 | 10.50 | same PDF, Table 3 · 2026-11-17 |
| RT (tensile resilience) | % | 19.76 | 42.09 | 49.94 | 38.00 | same PDF, Table 3 · 2026-11-17 |
| B (bending rigidity) | gf·cm²/cm | 0.0066 | 0.0162 | 0.0270 | 0.1249 | same PDF, Table 3 · 2026-11-17 |
| 2HB (bending hysteresis) | gf·cm/cm | 0.0081 | 0.0432 | 0.0277 | 0.1820 | same PDF, Table 3 · 2026-11-17 |
| G (shear stiffness) | gf/(cm·deg) | 0.47 | 0.48 | 0.52 | 1.58 | same PDF, Table 3 · 2026-11-17 |
| 2HG (shear hysteresis 0.5°) | gf/cm | 2.49 | 1.88 | 1.24 | 3.54 | same PDF, Table 3 · 2026-11-17 |
| 2HG5 (shear hysteresis 5°) | gf/cm | 2.30 | 1.81 | 1.79 | 7.74 | same PDF, Table 3 · 2026-11-17 |
| LC (compression linearity) | — | 0.341 | 0.3141 | 0.242 | 0.340 | same PDF, Table 3 · 2026-11-17 |
| WC (compression energy) | gf·cm/cm² | 0.378 | 0.313 | 0.181 | 0.376 | same PDF, Table 3 · 2026-11-17 |
| RC (compression resilience) | % | 39.47 | 31.91 | 37.84 | 34.31 | same PDF, Table 3 · 2026-11-17 |
| MIU (friction coefficient) | — | 0.205 | 0.280 | 0.191 | 0.188 | same PDF, Table 3 · 2026-11-17 |
| MMD (mean deviation of MIU) | — | 0.0100 | 0.0170 | 0.0208 | 0.0173 | same PDF, Table 3 · 2026-11-17 |
| SMD (geometric roughness) | µm | 4.61 | 3.77 | 9.43 | 11.67 | same PDF, Table 3 · 2026-11-17 |
| T (thickness) | mm | 0.919 | 0.943 | 0.610 | 1.022 | same PDF, Table 3 · 2026-11-17 |
| W (mass per area) | mg/cm² | 12.55 | 15.86 | 12.52 | 24.60 | same PDF, Table 3 · 2026-11-17 |

### 3b. Wang, O'Brien, Ramamoorthi — "Data-Driven Elastic Models for Cloth: Modeling and Measurement", ACM TOG 30(4) art. 71, SIGGRAPH 2011, Table 1, p. 7

Population: one sample of each of 10 named fabrics. Model: piecewise-linear anisotropic stretching (24 parameters) + bending (15 parameters) per fabric, fitted to a custom capture rig; the full parameter sets are in the paper's supplemental document (not opened here). "Error" = weighted mean distance between simulated and captured cloth (mm); no SD given. Cost per fabric (p. 7): ~24 h = 2 h cutting + 1 h capture + 2 h labelling + 19 h optimisation (four Xeon X7560 2.27 GHz octo-core CPUs). Licence: paper © ACM / authors; **no explicit licence for the parameter database** — treat as [NO-LICENCE] for reuse.

| Fabric (paper name) | Composition | Common use | Areal density kg/m² | Fit error mm | Source URL · accessed |
|---|---|---|---|---|---|
| Ivory Rib Knit | 95 % cotton / 5 % spandex | Underwear | 0.276 | 1.92 | https://graphics.berkeley.edu/papers/Wang-DDE-2011-08/Wang-DDE-2011-08.pdf · 2026-11-17 |
| Pink Ribbon Brown | 100 % polyester | Blanket | 0.228 | 1.66 | same PDF, Table 1 · 2026-11-17 |
| White Dots on Black | 100 % polyester | Tablecloth | 0.128 | 2.42 | same PDF, Table 1 · 2026-11-17 |
| Navy Sparkle Sweat | 96 % polyester / 4 % spandex | Sweater | 0.224 | 3.24 | same PDF, Table 1 · 2026-11-17 |
| Camel Ponte Roma | 60 % polyester / 40 % rayon | Jacket | 0.284 | 1.67 | same PDF, Table 1 · 2026-11-17 |
| Gray Interlock | 60 % cotton / 40 % polyester | T-shirt | 0.187 | 2.58 | same PDF, Table 1 · 2026-11-17 |
| 11oz Black Denim | 99 % cotton / 1 % spandex | Jeans | 0.324 | 2.30 | same PDF, Table 1 · 2026-11-17 |
| White Swim Solid | 87 % nylon / 13 % spandex | Swimsuit | 0.204 | 1.57 | same PDF, Table 1 · 2026-11-17 |
| Tango Red Jet Set | 100 % polyester | Fashion dress | 0.113 | 2.06 | same PDF, Table 1 · 2026-11-17 |
| Royal Target | 65 % cotton / 35 % polyester | Pants | 0.220 | 0.89 | same PDF, Table 1 · 2026-11-17 |

Range across the 10 fabrics: density 0.113–0.324 kg/m²; fit error 0.89–3.24 mm (computed here from Table 1).

## 4. Stated real-time / runtime cost

| # | System | Number as stated | Conditions (hardware, mesh size, population) | Source URL · accessed |
|---|---|---|---|---|
| C1 | KCD adaptive clothing (ray-cast fit on equip, background thread, cacheable) | Body+cloth: 207,312 rays, 110 ms (1.88 M rays/s). Body+cloth+chain: 417,400 rays, 365 ms (1.14 M rays/s). Body+cloth+chain+plate+deco: 770,028 rays, 879 ms (0.88 M rays/s) | One character, Intel Core i7-4770 @ 3.4 GHz (OCR reads "17-4770"); single example per setup, no spread | https://archive.org/stream/GDCEU2015Barak/GDCEU2015-Barak_djvu.txt · 2026-11-17 |
| C2 | Wang et al. 2010 wrinkle synthesis | Total 0.084 s/frame (11.93 FPS) male, 0.086 s (11.65 FPS) female; wrinkle synthesis on GPU 0.009 s / 0.008 s; Fig. 1: high-res reference sim of a 25k-vertex mesh took 2–3 min/frame | Table 1 p. 6: averages over 500 frames; male 46k, female 34k cloth vertices; Dell T7500 dual quad-core 2.26 GHz + Quadro FX 5800 | https://graphics.berkeley.edu/papers/Wang-EBW-2010-07/Wang-EBW-2010-07.pdf · 2026-11-17 |
| C3 | Ubisoft cloth (AC Unity) | Simulation ~40 % vs pre- & post-simulation (skinning, interpolation, mapping, tangent space) ~60 % of cloth cost; mapping "~10x faster than simulation" per vertex; GPU version runs up to 32 cloth items in one Dispatch instead of 50+, 64 vertices per thread group | Slide OCR, no ms in OCR'd slides | https://archive.org/download/GDC2015Vaisse/GDC2015-Vaisse_djvu.txt · 2026-11-17 |
| C4 | TailorNet inference | Paper: 1–2 ms/frame averaged over a batch of 21 (20–40 ms per batch); README: one sample at a time ≈ 20 ms/frame for all garments (21 independent MLPs) | Authors' GPU; no SD | https://github.com/chaitanya100100/TailorNet (README) · 2026-11-17 |
| C5 | PBNS | "over 14.000 FPS (for an outfit with 24K triangles!)"; model "a few MBs"; training "a few minutes, even without a GPU" | Authors' claim in README; hardware not stated there | https://github.com/hbertiche/PBNS (README) · 2026-11-17 |
| C6 | GarNet | < 1 cm mean point error vs PBS, 100× faster than PBS | arXiv v3 abstract; per-frame ms not confirmed | https://arxiv.org/abs/1811.10983 · 2026-11-17 |
| C7 | TailorNet vs PBS | "more than 1000 times faster" than the physics sim it learns from | Abstract | https://arxiv.org/abs/2003.04583 · 2026-11-17 |
| C8 | Roblox layered clothing | Deformation runs once, then skinning; 2025 change "expected to be the same or better" in performance — no ms | Official posts | https://devforum.roblox.com/t/cage-mesh-deformer-studio-beta/1196727 · 2026-11-17 |

## 5. Lessons from the makers (summary)
- Auto-conforming a finished piece to a new body works for tight layers but fails on loose ones: Sims 4 needed a separate loose-fit DMAP; Wang 2010 excludes skirts and capes; Robust Skin Weights Transfer exists because transfer breaks on non-skin-tight garments.
- Fully automatic retargeting can cost more artist time than authoring (Destiny dropped wrap + blend shapes).
- Cage/UV-paired deformers (Roblox) and bone-driven fitting (Second Life) scale to user content; Roblox's own failure mode was RBF overshoot and puffy stacked layers.
- Fitting on equip is affordable when done once and cached (KCD ~0.1–0.9 s on a background thread; Roblox "static" deform).

## 6. Licence and patent notes
- Every MPI/SMPL-based model here (CAPE, TailorNet; SMPL itself, see H5) is **[NC]**. DrapeNet code is GPL-3.0 **[COPYLEFT]** but still needs SMPL **[NC]**. PBNS repo **[NO-LICENCE]**. Robust Skin Weights Transfer code is MIT.
- Ramaiah KES table and Wang 2011 database: **[NO-LICENCE]** for reuse; the numbers can be cited as facts with the source, but neither the PDF nor the dataset should be copied into a repo.
- Game/platform methods are **[CLOSED]**: description only. No patent search was done for this file (see Not found).

## 7. Source list (all accessed 2026-11-17)

| # | Source | URL | Licence of the source page/file |
|---|---|---|---|
| S1 | Nair, GDC 2015 Sims 4 (Vault + YouTube transcript) | https://gdcvault.com/play/1022086/Innovations-in-The-Sims-4 ; https://www.youtube.com/watch?v=s7R_HHxCokU | © GDC/EA |
| S2 | Shepherd, GDC 2014 Destiny (YouTube); Polygon report 2014-03-21 | https://www.youtube.com/watch?v=sSSMC4O6J8Q ; https://www.polygon.com/2014/3/21/5533684/bungies-destiny-extensive-character-customization | © |
| S3 | Barak, GDC Europe 2015 KCD (archive.org slide OCR) | https://archive.org/stream/GDCEU2015Barak/GDCEU2015-Barak_djvu.txt | © Warhorse/GDC |
| S4 | Roblox cage meshes doc | https://create.roblox.com/docs/art/modeling/cage-meshes | Roblox docs |
| S5 | Roblox devforum beta post (2021-04-30) | https://devforum.roblox.com/t/cage-mesh-deformer-studio-beta/1196727 | Roblox |
| S6 | Roblox devforum fit post (2025-03-14) | https://devforum.roblox.com/t/coming-soon-improved-layered-clothing-fit/3550013 | Roblox |
| S7 | Second Life wiki, Fitted Mesh | https://wiki.secondlife.com/wiki/Mesh/Rigging_Fitted_Mesh | Linden Lab wiki |
| S8 | MetaHuman hair and clothing tools | https://dev.epicgames.com/documentation/en-us/metahuman/hair-and-clothing-tools | Epic docs |
| S9 | Cyberpunk 2077 modding wiki, garment support | https://wiki.redmodding.org/cyberpunk-2077-modding/for-mod-creators-theory/3d-modelling/garment-support-how-does-it-work | Community wiki |
| S10 | Kosanovich, GDC 2023 Rumbleverse (Vault abstract) | https://www.gdcvault.com/play/1028733/Building-Champions-Character-Art-Pipeline | © GDC |
| S11 | Sciglio & Taylor, GDC 2010 APB slides | https://media.gdcvault.com/gdc10/slides/Sciglio_Maurizio_APBCustomisationSystem.pdf | © |
| S12 | Brennan, GDC 2019 slides | https://media.gdcvault.com/gdc2019/presentations/Brennan_Sophie_Realtime_Cloth_Solutions.pdf | © |
| S13 | Vaisse, GDC 2015 (archive.org OCR; Vault page) | https://archive.org/download/GDC2015Vaisse/GDC2015-Vaisse_djvu.txt ; https://www.gdcvault.com/play/1022421/Ubisoft-Cloth-Simulation-Performance-Postmortem | © |
| S14 | Brouet et al. 2012 | https://doi.org/10.1145/2185520.2185532 | ACM |
| S15 | Guan et al. 2012 DRAPE | https://doi.org/10.1145/2185520.2185531 | ACM |
| S16 | Pons-Moll et al. 2017 ClothCap | https://doi.org/10.1145/3072959.3073711 | ACM |
| S17 | CAPE arXiv + licence | https://arxiv.org/abs/1907.13615 ; https://cape.is.tue.mpg.de/license.html | **[NC]** |
| S18 | TailorNet arXiv, README, LICENSE | https://arxiv.org/abs/2003.04583 ; https://github.com/chaitanya100100/TailorNet | **[NC]** |
| S19 | MGN arXiv | https://arxiv.org/abs/1908.06903 | arXiv |
| S20 | SIZER arXiv | https://arxiv.org/abs/2007.11610 | arXiv |
| S21 | GarNet arXiv | https://arxiv.org/abs/1811.10983 | arXiv |
| S22 | DrapeNet arXiv + repo | https://arxiv.org/abs/2211.11277 ; https://github.com/liren2515/DrapeNet | GPL-3.0 **[COPYLEFT]** |
| S23 | SNUG arXiv | https://arxiv.org/abs/2204.02219 | arXiv |
| S24 | PBNS arXiv + repo | https://arxiv.org/abs/2012.11310 ; https://github.com/hbertiche/PBNS | **[NO-LICENCE]** |
| S25 | Robust Skin Weights Transfer (DOI, repo) | https://doi.org/10.1145/3610543.3626180 ; https://github.com/rin-23/RobustSkinWeightsTransferCode | MIT |
| S26 | Wang et al. 2010 wrinkle synthesis PDF | https://graphics.berkeley.edu/papers/Wang-EBW-2010-07/Wang-EBW-2010-07.pdf | ACM author copy |
| S27 | Ju 2005 / Joshi 2007 / Lipman 2008 (DOIs) | https://doi.org/10.1145/1073204.1073229 ; https://doi.org/10.1145/1275808.1276466 ; https://doi.org/10.1145/1360612.1360677 | ACM |
| S28 | Ramaiah et al. KES-FB article | https://static.fibre2fashion.com/articleresources/PdfFiles/60/5963.pdf | **[NO-LICENCE]** |
| S29 | Wang, O'Brien, Ramamoorthi 2011 PDF | https://graphics.berkeley.edu/papers/Wang-DDE-2011-08/Wang-DDE-2011-08.pdf | ACM author copy; database **[NO-LICENCE]** |

## Not found
- Penava, Šimić Penava, Knezić 2014 (Fibres & Textiles in Eastern Europe 22(2) 57–63) Young's modulus / Poisson's ratio by direction for cotton, wool, wool+lycra and PES plain weaves: Grok gave a table, but every PDF host (BazTech, SciSpace, Paperity) was bot-walled or 403 on 2026-11-17, so the numbers are **not** included. Paper exists (abstract confirmed via search results). https://bibliotekanauki.pl/issues/13060 · 2026-11-17
- Wang 2011 full 24 stretch + 15 bending parameters per fabric (supplemental PDF not opened).
- Miguel et al. 2012 (CGF 31(2), https://doi.org/10.1111/j.1467-8659.2012.03031.x) and Clyde, Teran, Tamstorf 2017 (SCA, https://doi.org/10.1145/3099564.3099577) fitted parameter tables: PDFs not opened; no numbers included.
- Kawabata/Niwa "ideal suiting" ranges: only a secondary scrape was found; not included.
- Ubisoft GDC Europe 2014 per-platform "dancers with cloth" counts and PS4 2 ms GPU figure: Grok cited SlideShare; not confirmed in a primary copy.
- Star Wars Battlefront II skirt self-collision cost (~380 µs / ~150 µs): Grok found only a YouTube transcript with unconfirmed talk identity; not included.
- Ready Player Me `meshDisplacement` / outfit template pipeline: docs not reachable on 2026-11-17; not included.
- Rumbleverse detailed method (region tagging, layers): only the Vault abstract was readable.
- No GDC/SIGGRAPH talk found in which Blizzard (WoW) or ArenaNet (GW2) describe fitting one armour mesh to all races; their wikis describe per-race armour.
- Licence files for MGN, SIZER, GarNet, SNUG and ClothCap were not checked.
- No patent search was done for cage-based or garment-fitting methods.

## Sources disagree
- TailorNet speed: paper states 1–2 ms per frame (batch average of 21); the authors' README says ≈ 20 ms per frame when run one sample at a time. Use 20 ms for interactive single-character use. https://github.com/chaitanya100100/TailorNet · 2026-11-17
- Destiny speaker's surname: "Shepherd" in Polygon; the YouTube auto-transcript renders it "Shepard". Polygon spelling used. https://www.polygon.com/2014/3/21/5533684/bungies-destiny-extensive-character-customization · 2026-11-17
- KCD CPU: the archive.org OCR reads "17-4770"; this is read as Intel Core i7-4770 (OCR of "i7"). https://archive.org/stream/GDCEU2015Barak/GDCEU2015-Barak_djvu.txt · 2026-11-17
