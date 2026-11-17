# B8 — Character-creator controls and open parametric body/face models

> Verify fixes applied 2026-11-17: Anny default local-change list has **254** labels (my recount of `local_change_labels` on the shape docs, `local_changes="default"`), not “about 255”. Elden Ring’s 57 face-structure sliders marked **UNVERIFIABLE** (eldenring.wiki.gg returned a Cloudflare challenge in the Library Verify pass).

Access date for every source: **2026-11-16**. Every row gives its source, the section it came from and the URL. "My count" means I counted list items on the cited page myself; it is not a number the source publishes. Game creators are copyrighted products: I record facts only and copied no assets. Wiki text is under each wiki's own licence; I checked it only where stated.

## Part A — Shipped character creators

### A1. Skyrim (2011, vanilla)

Sources:
- Fextralife, *Character Creation* — https://skyrim.wiki.fextralife.com/Character_Creation
- UESP, *Skyrim:Character Creation*, §Appearance — https://en.uesp.net/wiki/Skyrim:Character_Creation

| Tab | Controls listed | Type | My count |
|---|---|---|---|
| Body | Sex; Presets (each race has 20 presets, 10 male and 10 female); Skin Tone (slider, no colour picker); Weight | preset + slider | 4 |
| Head | Complexion; Dirt; Dirt Color; Scars; War Paint; War Paint Color | preset / colour | 6 |
| Face | Nose Type; Nose Height; Nose Length; Jaw Width; Jaw Height; Jaw Forward; Cheekbone Height; Cheekbone Width; Cheek Color; Laugh Lines; Cheek Color Lower; Nose Color; Chin Color; Neck Color; Forehead Color | preset + sliders + tints | 15 |
| Eyes | Eye Shape; Eye Color; Eye Height; Eye Width; Eye Depth; Eyeliner Color; Eye Shadow; Eye Tint | | 8 |
| Brow | Brow Type; Brow Height; Brow Width; Brow Forward | | 4 |
| Mouth | Mouth Shape; Mouth Height; Mouth Forward; Chin Width; Chin Length; Chin Forward; Lip Color | | 7 |
| Hair | Hair; Facial Hair (not available to Argonians or to females of any race); Hair Color | | 3 |

- Total controls (my count from Fextralife): 47.
- There are 10 playable races (UESP).
- UESP adds that the Weight slider increases muscle. Race-specific extras: jewellery for some races, feathers/horns for Argonians only, fur patterns for Khajiit in place of war paint.
- Appearance cannot be changed later except through vampirism or the Dawnguard *Surgery* quest.

### A2. Fallout 4 (2015)

Sources:
- Fallout Wiki (fallout.wiki), *Fallout 4 Character Creation*, page text under CC BY-SA 4.0 (page footer) — https://fallout.wiki/wiki/Fallout_4_Character_Creation
- Fandom Fallout wiki gallery (wikitext via API) — https://fallout.fandom.com/wiki/Fallout_4_character_creation
- Fandom *Sole Survivor* notes — https://fallout.fandom.com/wiki/Sole_Survivor

Controls:
- **Face:** no sliders. Features are sculpted with a cursor at the mirror. "Most facial features can be manually sculpted and adjusted" (fallout.wiki).
- **Extras:** markings and blemishes, each with an adjustable **percentage** intensity.
- **Body:** a **triangle** interface.
- **Presets:** the gallery shows 15 default female and 15 default male faces.
- SPECIAL: 21 points to allocate on top of a base of 1 per stat; maximum 10 per stat at creation.

### A3. Cyberpunk 2077 (post-2.2 content)

Source: Cyberpunk Wiki (Fandom), *Cyberpunk 2077 Character Customization*, §Appearance table (wikitext via API) — https://cyberpunk.fandom.com/wiki/Cyberpunk_2077_Character_Customization

Options are mostly **discrete choices**, not continuous morphs. F = feminine body type, M = masculine body type.

| Aspect | Options | Changeable later at |
|---|---|---|
| Body type | 2 | locked |
| Voice tone | 2 | locked |
| Skin tone | 12 | Ripperdoc |
| Skin type | 5 | Ripperdoc |
| Hairstyle | 70 F / 60 M | Mirror or Ripperdoc |
| Hair colour | 35 | Mirror or Ripperdoc |
| Eyes | 21 | Ripperdoc |
| Eye colour | 71 | Mirror or Ripperdoc |
| Eyebrows | 14 | Mirror or Ripperdoc |
| Eyebrow colour | 35 | Mirror or Ripperdoc |
| Eyelash colour | 35 | Mirror or Ripperdoc |
| Nose | 22 F / 21 M | Ripperdoc |
| Mouth | 22 F / 21 M | Ripperdoc |
| Jaw | 22 F / 21 M | Ripperdoc |
| Ears | 22 F / 21 M | Ripperdoc |
| Beard (masculine only) | 13 | Mirror or Ripperdoc |
| Beard style | 1, 3 or 7 depending on beard | Mirror or Ripperdoc |
| Beard colour | 35 | Mirror or Ripperdoc |
| Cyberware | 17 | Ripperdoc |
| Facial scars | 14 | Mirror or Ripperdoc |
| Facial tattoos | 16 | Ripperdoc |
| Piercings | 15 F / 17 M | Mirror or Ripperdoc |
| Piercing colour | 16 | Mirror or Ripperdoc |
| Teeth | 5 | Mirror or Ripperdoc |
| Eye makeup | 37 | Mirror or Ripperdoc |
| Eye makeup colour | 14 | Mirror or Ripperdoc |
| Lip makeup style | 4 | Mirror or Ripperdoc |
| Lip makeup | 38 | Mirror or Ripperdoc |
| Lip makeup colour | 1, 2, 6, 8 or 14 | Mirror or Ripperdoc |
| Cheek makeup | 25 | Mirror or Ripperdoc |
| Cheek makeup colour | 4 or 16 | Mirror or Ripperdoc |
| Blemishes | 4 | Ripperdoc |
| Blemish colour | 6 | Ripperdoc |
| Nails | 2 | Mirror or Ripperdoc |
| Nail colour | 54 | Mirror or Ripperdoc |
| Chest (feminine only) | 3 | Ripperdoc |
| Nipples | 4 F / 2 M | Ripperdoc |
| Body tattoos | 8 | Ripperdoc |
| Body scars | 3 | Mirror or Ripperdoc |
| Genitals | 4 | Ripperdoc |
| Penis size | 3 | Ripperdoc |
| Pubic hair style | 6 | Mirror or Ripperdoc |
| Pubic hair colour | 5 | Mirror or Ripperdoc |

Every aspect except body type and voice can be changed after creation.

### A4. Baldur's Gate 3 (2023)

Source: bg3.wiki, *Character creation*, §Appearance — https://bg3.wiki/wiki/Character_creation

Options are presets or colour choices with intensity sliders.

| Control | Details from source |
|---|---|
| Body type | 4 options for elves, drow, half-elves, humans and tieflings; 2 for all other races; locked after creation |
| Identity | female / male / non-binary (changes how dialogue addresses you) |
| Voice | 8 options |
| Face | presets, depending on race and subrace |
| Skin colour | all colours available to most races; dragonborn have a separate palette |
| Maturity | depth of age lines |
| Scarring | presets |
| Freckles | quantity and intensity |
| Vitiligo | amount of depigmentation |
| Crest, chin, jaw | dragonborn only, replacing maturity, scars, freckles and vitiligo |
| Genitals | 2 types; number of variants depends on race |
| Body art | tattoo and facial piercing (dragonborn: piercing only) |
| Eyes | iris/sclera colour; heterochromia option |
| Makeup | eye makeup; lip colour, gloss and metallic tint |
| Hair | style, colour, highlight colour, greying intensity and colour (not for dragonborn) |
| Facial hair | not for elves, drow or dragonborn |
| Horns | shape, colour, tip colour (tieflings only) |
| Draconic Bloodline | scale shape and colour (Draconic Bloodline sorcerers only) |
| Tail | 3 designs or none (dragonborn only) |

### A5. Elden Ring (2022)

Source: Eldenpedia (eldenring.wiki.gg), *Character Creation* — https://eldenring.wiki.gg/wiki/Character_Creation. The 57-slider total below is **UNVERIFIABLE** in the Library Verify pass (Cloudflare challenge to curl, the MediaWiki API and archive.org; WebFetch truncated).

Bases:
- Body Type A or B.
- Age: Young, Mature or Aged. This is appearance only.
- 6 voices: Young 1–2, Mature 1–2, Aged 1–2.

Face template:
- Bone Structure 1–6.
- Form Emphasis, Apparent Age and Facial Aesthetic (masculine ↔ feminine), each **0–255**.

**Face Structure sliders, all 0–255:**

| Group | Number of sliders |
|---|---|
| Facial Balance | 6 |
| Forehead/Glabella | 6 |
| Brow Ridge | 3 |
| Eyes | 4 |
| Nose Ridge | 7 |
| Nostrils | 3 |
| Cheeks | 5 |
| Lips | 6 |
| Mouth | 6 |
| Chin | 7 |
| Jaw | 4 |
| **Total (my count)** | **57** (**UNVERIFIABLE**; wiki blocked in the verify pass) |

Other controls:
- **Hair:** 32 styles, plus colour, luster, root darkness and white hairs.
- **Eyebrows:** 17 shapes.
- **Beards:** 12 types, plus stubble.
- **Body:** Body Hair (0–255) and colour; Musculature (Standard or Muscular).
- **Favourites:** up to 15 saved appearances.

### A6. Black Desert, Dragon's Dogma 2, The Sims 4, MetaHuman Creator — not confirmed

The Grok pass gave leads for all four:
- BDO: official Adventurer's Guide categories.
- Dragon's Dogma 2: Fextralife body list, plus IGN's "40 muscle styles" and "12 nose sliders".
- The Sims 4: a journalist's count of "18 / 35 / 44" settings.
- MetaHuman: legacy 18 fixed bodies and height table.

I could not confirm them in this session:
- The BDO official wiki is behind an Incapsula bot wall; curl and WebFetch both returned an empty page.
- I did not open the others.

So no numbers are recorded for these four. See Not found.

## Part B — Open parametric body and face models

| Model | Mesh | Shape / identity params | Expression params | Pose / skeleton | Training data (as stated) | Licence | **Commercial?** | Source and URL |
|---|---|---|---|---|---|---|---|---|
| **MakeHuman** (1.x) | — | macro and detail "modifiers" (count not confirmed) | expressions are assets | default rig | artist-made targets, not scans | **Code AGPL-3.0+**. **Assets CC0 1.0**: base mesh and proxies, targets and modifiers, textures, clothes, poses, expressions (LICENSE.md §B–C). | Yes. CC0 assets; AGPL obligations apply to the code. | https://github.com/makehumancommunity/makehuman/blob/master/LICENSE.md |
| **MPFB2** (MakeHuman for Blender) | same assets | same | — | — | — | **Code GPLv3** ("same licence setup as MakeHuman, but with GPLv3"); assets in LICENSE.ASSETS.md (not read; per LICENSE.md, "same setup", i.e. CC0) | Yes, with GPL obligations on the code | https://github.com/makehumancommunity/mpfb2/blob/master/LICENSE.md |
| **Anny** (NAVER LABS Europe, 2025; ECCV 2026) | 13,718 vertices, 27,420 triangles (default "anny" topology). 104-bone compact rig by default; full MakeHuman rig has 163 bones. | **11 phenotype params** in [0,1]: gender, age, muscle, weight, height, proportions, cupsize, firmness, african, asian, caucasian (the last three sum to 1). Plus **254 local-change params**, typically −1…1 (my count of the printed `local_change_labels` on the shape docs, `local_changes="default"`; recounted 2026-11-17). | facial actions from the CC0 "Face Units" pack | bones per rig | MakeHuman / MPFB2 assets (artist-made) | **Code Apache-2.0**. MPFB2 assets and Face Units are **CC0 1.0**. "soma" topology is Apache-2.0. **The SMPL-X topology option is non-commercial only.** | Yes, except **SMPL-X topology (NC)** | https://github.com/naver/anny ; https://naver.github.io/anny/build/shape_parameterization.html |
| **MHR** (Meta, Momentum Human Rig, 2025) | 7 LODs (0–6) | **45** identity: 20 body, 20 head, 5 hands | **72** artist-sculpted FACS-style blendshapes (not PCA, not 1:1 AUs) | **204** model params (joint angles and scalings) | not stated in README | **Apache-2.0** | Yes | https://github.com/facebookresearch/MHR (README §Model Parameters, §License) |
| **FLAME** (MPI, 2017) | N = 5,023 vertices | **300** shape PCs | **100** expression PCs | K = 4 joints (neck, jaw, 2 eyeballs) plus global rotation; pose vector 3K+3 | shape from **3,800** CAESAR heads (2,100 F / 1,700 M); over 33,000 scans in total; pose and expression from D3DFACS and additional 4D sequences | FLAME 2017/2019/2020/2023: **non-commercial scientific research, education or artistic projects only**. **FLAME 2023 Open: CC BY 4.0** (released "11/25" per site news). | **Only FLAME 2023 Open.** All others **NC** | https://flame.is.tue.mpg.de/ ; licence https://flame.is.tue.mpg.de/modellicense.html ; paper https://download.is.tue.mpg.de/flame/flame_paper.pdf ; viewer caption https://flame.is.tue.mpg.de/interactivemodelviewer.html |
| **ICT-FaceKit** (USC ICT, Light model) | 26,719 vertices / 26,384 faces (all parts); face area 9,409 vertices | **100** PCA identity modes (Light); full model 200+ | **53** expression blendshapes | — | light-stage scan data; the related CVPR 2020 paper cites 4,000 scans | **MIT** (Light). The full model is "under a different USC specific license" (not released). | Light: yes. Full: unknown | https://github.com/USC-ICT/ICT-FaceKit (README tables) |
| **SMPL** (MPI, 2015) | N = 6,890 vertices | up to **300** shape components available (site); paper uses \|β\| = 300 in its Dyna comparison | — | K = 23 joints (+ root); pose blend shapes 207×3×6,890 = 4,278,690 params | CAESAR plus pose registrations (paper) | **SMPL-Model: non-commercial scientific research.** Commercial licence via Max Planck Innovation. **SMPL-Body** (a single body without the shape space) is **CC BY 4.0**. | **NC** (model); bodies CC BY | https://smpl.is.tue.mpg.de/ ; https://smpl.is.tue.mpg.de/modellicense.html ; https://smpl.is.tue.mpg.de/bodylicense.html ; paper https://files.is.tue.mpg.de/black/papers/SMPL2015.pdf |
| **SMPL-X** (MPI, 2019) | N = 10,475 vertices | 10 in paper; full space released 20 Aug 2020 (site news) | 10 in paper; full space released 2020 | K = 54 joints. **119 params in total**: 75 global + body/eyes/jaw, 24 hand-pose PCA, 10 shape, 10 expression. | — | **Non-commercial scientific research**; no distribution | **NC** | https://arxiv.org/abs/1904.05866 (§3) ; https://smpl-x.is.tue.mpg.de/ ; https://github.com/vchoutas/smplx |
| **MANO / SMPL+H** (MPI, 2017) | — | — | — | — | about 1,000 high-resolution scans of 31 subjects' hands | **Non-commercial scientific research** | **NC** | https://mano.is.tue.mpg.de/ ; https://mano.is.tue.mpg.de/license.html |
| **STAR** (MPI, 2020) | SMPL-compatible | — | — | sparse per-joint correctives; **20% of SMPL's parameter count** | SMPL data plus an extra 10,000 scans | **Non-commercial scientific research** | **NC** | https://arxiv.org/abs/2008.08535 ; https://star.is.tue.mpg.de/license.html |
| **SUPR** (MPI, 2022) | — | — | — | sparse, part-based; extended foot kinematics | full-body and part scans (federated) | **Non-commercial scientific research** (GitHub LICENSE) | **NC** | https://arxiv.org/abs/2210.13861 ; https://github.com/ahmedosman/SUPR |
| **BFM 2009** (Basel) | 53,490 vertices, 160,470 triangles | **199** shape PCs (plus 199 texture PCs) | — (neutral) | — | labelled with gender, height, weight and age | **Non-commercial research, evaluation or testing only. No distribution or sublicensing.** | **NC** | https://faces.dmi.unibas.ch/bfm/index.php?nav=1-1-0&id=details ; terms https://faces.dmi.unibas.ch/bfm/index.php?nav=1-2&id=downloads |
| **BFM 2017** | — | — | expression model (rank not confirmed) | — | — | "Terms of Non-Commercial Use", internal research only | **NC** | https://faces.dmi.unibas.ch/bfm/bfm2017.html |
| **GHUM / GHUML** (Google, 2020) | — | **16-D** body shape (PCA and VAE) from 4,329 scans | **20-D** facial expression | **63 joints, 124 DOF** | over 2 M mocap body configurations; over 4.8 K hand configurations | Model through a **request form**; the README states no open licence | **Treat as restricted** (terms not public in README) | https://github.com/google-research/google-research/blob/master/ghum/README.md |
| **ARKit blendshapes** (Apple) | — | — | **52** BlendShapeLocation coefficients, each 0.0 (neutral) to 1.0 (maximum) (my count of the doc's links) | — | — | Apple API (iOS/iPadOS 11+), not an open model | Apple platform terms | https://developer.apple.com/documentation/arkit/arfaceanchor/blendshapelocation |

### Licence flags (plain)
- **Free for commercial use:**
  - MakeHuman assets and MPFB2 assets (CC0)
  - Anny code (Apache-2.0) and its CC0 data
  - MHR (Apache-2.0)
  - Meta momentum library (MIT)
  - ICT-FaceKit Light (MIT)
  - FLAME 2023 **Open** (CC BY 4.0, attribution needed)
  - SMPL-**Body** files (CC BY 4.0)
- **Copyleft code:** MakeHuman (AGPL-3.0+) and MPFB2 (GPLv3).
- **NON-COMMERCIAL / RESEARCH-ONLY, flagged:**
  - FLAME 2017–2023 (non-Open)
  - SMPL-Model, SMPL-X, MANO/SMPL+H, STAR, SUPR (all MPI, with paid commercial licences available)
  - BFM 2009 and BFM 2017 (Basel)
  - Anny's optional SMPL-X topology
- **Restricted / unclear:** GHUM (request form) and the ICT full model (not released).

## Verification note
- Every Part A row was read on the cited wiki page in this session. Cyberpunk and Fallout Fandom pages were read through the MediaWiki API because the HTML returned 403.
- Every Part B number was read in a README, licence page, project page or paper PDF (SMPL 2015 PDF, SMPL-X arXiv PDF).
- **Grok live-search pass** (grok-4.6, xhigh, live search; prompt /workspace/grok-runs-B/B8.txt; ran to exit about 10:55 AEDT). Used only as leads.
  - **Added after confirmation:**
    - Skyrim tab list and presets (Fextralife)
    - Elden Ring slider list (57)
    - FO4 triangle body interface
    - ICT-FaceKit vertex table and 53 blendshapes
    - GHUM dimensions
    - BFM 2009 geometry and terms
    - SMPL-X 119-parameter breakdown
    - STAR and SUPR non-commercial licences
    - MANO scan count
    - FLAME 2023 Open CC BY 4.0
    - ARKit 52 and its 0–1 range
    - MPFB2 GPLv3
  - **Grok said "NOT FOUND", but I found it:**
    - Anny's full phenotype list (11)
    - Elden Ring body controls (Body Hair, Musculature, 15 favourites)
  - **Unconfirmed and left out:**
    - BDO, DD2, Sims 4, MetaHuman and RaceMenu figures
    - Cyberpunk 2.2 patch list (Kotaku)
    - MHR per-LOD vertex counts and 127 joints
    - FaceWarehouse and LYHM
    - SMPL training counts (1,786 registrations / 40 people)
    - SUPR 1.2 M scans
    - BFM 2009 subject demographics
    - GHUM vertex counts

## Not found
- Black Desert official control list (bot wall), Dragon's Dogma 2, The Sims 4, MetaHuman Creator body/face parameter census, and Skyrim RaceMenu (mod). None confirmed in this session.
- Any official total slider count for Skyrim, FO4 or BG3. The Skyrim and Elden Ring totals above are my counts.
- MakeHuman modifier count. FLAME 2023 Open parameter counts (assumed the same as FLAME 2023, but not checked). BFM 2017 expression rank. MHR training census.
- Licence text of MPFB2's LICENSE.ASSETS.md (not read).

## Sources disagree
- **FLAME licence:** older GitHub READMEs (TF_FLAME, FLAME_PyTorch) say FLAME is "under a Creative Commons Attribution license". The official model-licence page says FLAME 2017–2023 are **non-commercial only**, and only **FLAME 2023 Open** is CC BY 4.0. The official page wins.
- **ARKit 52 vs ICT-FaceKit 53:** ICT uses ARKit-style names with 53 shapes. Grok attributes the split to browInnerUp and cheekPuff being split into L/R. I confirmed only the 53 count in the ICT README.
- **SMPL-X dimensionality:** the CVPR 2019 paper uses 10 shape + 10 expression (119 parameters in total). The site later released the full shape and expression spaces (20 Aug 2020). Grok's lead says 300/100 and 55 joints in the code; I did not confirm that.
- **Anny bone count:** the default rig has 104 bones and the full MakeHuman rig 163 (README). Grok quoted only 163 (from the paper).
- **Elden Ring body controls:** Grok said none beyond Type A/B. The wiki lists Body Hair and Musculature.
- **Cyberpunk hairstyle count:** Grok cites Kotaku (2020 launch) at 39 per body type, vs 70 F / 60 M in the current wiki. These are different game versions; I did not confirm the Kotaku figure.

## Sources
1. Fextralife Skyrim Wiki, Character Creation — https://skyrim.wiki.fextralife.com/Character_Creation, accessed 2026-11-16.
2. UESP, Skyrim:Character Creation — https://en.uesp.net/wiki/Skyrim:Character_Creation, accessed 2026-11-16.
3. Fallout Wiki, Fallout 4 Character Creation (CC BY-SA 4.0) — https://fallout.wiki/wiki/Fallout_4_Character_Creation, accessed 2026-11-16.
4. Fallout Fandom, Fallout 4 character creation; Sole Survivor (via API) — https://fallout.fandom.com/wiki/Fallout_4_character_creation ; https://fallout.fandom.com/wiki/Sole_Survivor, accessed 2026-11-16.
5. Cyberpunk Wiki, Cyberpunk 2077 Character Customization (via API) — https://cyberpunk.fandom.com/wiki/Cyberpunk_2077_Character_Customization, accessed 2026-11-16.
6. bg3.wiki, Character creation — https://bg3.wiki/wiki/Character_creation, accessed 2026-11-16.
7. Eldenpedia, Character Creation — https://eldenring.wiki.gg/wiki/Character_Creation, accessed 2026-11-16.
8. MakeHuman LICENSE.md — https://github.com/makehumancommunity/makehuman/blob/master/LICENSE.md, accessed 2026-11-16.
9. MPFB2 LICENSE.md — https://github.com/makehumancommunity/mpfb2/blob/master/LICENSE.md, accessed 2026-11-16.
10. NAVER Anny README and shape docs — https://github.com/naver/anny ; https://naver.github.io/anny/build/shape_parameterization.html, accessed 2026-11-16.
11. Meta MHR README — https://github.com/facebookresearch/MHR ; arXiv 2511.15586 https://arxiv.org/abs/2511.15586, accessed 2026-11-16.
12. Meta momentum (MIT) — https://github.com/facebookresearch/momentum, accessed 2026-11-16.
13. FLAME site, licence, viewer, paper — https://flame.is.tue.mpg.de/ ; https://flame.is.tue.mpg.de/modellicense.html ; https://flame.is.tue.mpg.de/interactivemodelviewer.html ; https://download.is.tue.mpg.de/flame/flame_paper.pdf, accessed 2026-11-16.
14. TF_FLAME and FLAME_PyTorch READMEs — https://github.com/TimoBolkart/TF_FLAME ; https://github.com/soubhiksanyal/FLAME_PyTorch, accessed 2026-11-16.
15. USC-ICT ICT-FaceKit README — https://github.com/USC-ICT/ICT-FaceKit, accessed 2026-11-16.
16. Loper M. et al. SMPL, ACM TOG 34(6):248, 2015 — https://files.is.tue.mpg.de/black/papers/SMPL2015.pdf ; https://smpl.is.tue.mpg.de/ (model and body licences), accessed 2026-11-16.
17. Pavlakos G. et al. Expressive Body Capture (SMPL-X), CVPR 2019 — https://arxiv.org/abs/1904.05866 ; https://smpl-x.is.tue.mpg.de/ ; https://github.com/vchoutas/smplx, accessed 2026-11-16.
18. MANO — https://mano.is.tue.mpg.de/ ; https://mano.is.tue.mpg.de/license.html, accessed 2026-11-16.
19. Osman A. et al. STAR, ECCV 2020 — https://arxiv.org/abs/2008.08535 ; https://star.is.tue.mpg.de/license.html, accessed 2026-11-16.
20. Osman A. et al. SUPR, ECCV 2022 — https://arxiv.org/abs/2210.13861 ; https://github.com/ahmedosman/SUPR, accessed 2026-11-16.
21. Basel Face Model 2009 details and terms; BFM 2017 terms — https://faces.dmi.unibas.ch/bfm/index.php?nav=1-1-0&id=details ; https://faces.dmi.unibas.ch/bfm/index.php?nav=1-2&id=downloads ; https://faces.dmi.unibas.ch/bfm/bfm2017.html, accessed 2026-11-16.
22. Google GHUM README — https://github.com/google-research/google-research/blob/master/ghum/README.md, accessed 2026-11-16.
23. Apple ARKit ARFaceAnchor.BlendShapeLocation — https://developer.apple.com/documentation/arkit/arfaceanchor/blendshapelocation, accessed 2026-11-16.
