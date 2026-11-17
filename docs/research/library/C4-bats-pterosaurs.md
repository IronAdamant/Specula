# C4: Bats and pterosaurs (wings on four-limbed bodies, for dragons)

Research for the Specula game-engine library. Written 2026-11-16 (AEDT); completeness check and follow-up 2026-11-17 08:50–09:10 AEDT. Sources were read on 2026-11-16 unless a row says 2026-11-17. Every table row now carries its URL in the last column, and every link was re-checked on 2026-11-17 (results in §9).

**Grok pass.** One Grok live-search pass ran on 2026-11-16, 10:49–11:00 AEDT (grok-4.6, reasoning effort xhigh, live search). No second Grok pass was run on 2026-11-17: the first pass already covered every section, and §8 records what it added or contradicted.

**Method.** I read the primary papers directly (PMC HTML, Europe PMC XML, author or repository PDFs) with curl, WebFetch and WebSearch. On top of that I ran one Grok live-search pass (grok-4.6, reasoning effort xhigh, 2026-11-16 10:49–11:00 AEDT). I treated everything Grok returned as an unverified lead. A Grok number is in this file as **Verified** only if I opened the primary source myself and found the same number there. Grok leads I could not open stay **Unverified**, and the reason is given. Section 8 lists what the Grok pass added or contradicted.

**Status key.**
- **Verified**: I opened the cited source on 2026-11-16 and the number is there, in the stated table or page.
- **Verified (computed)**: I derived the value by arithmetic from Verified numbers. The arithmetic is shown or described.
- **Unverified**: the source could not be opened (paywall, CloudFront or PMC bot-block, login), or the number comes from a secondary citation only.

**Units.** Mass M in kg unless stated (Brower & Veinus 1981 use grams and centimetres). Span b in m. Wing area S in m². Aspect ratio AR = b²/S (dimensionless). Wing loading Q = Mg/S in N m⁻² (1 g cm⁻² = 98.1 N m⁻²). FL = forearm length. LD3, LD4, LD5 = total length of bat digits III, IV and V (metacarpal plus all phalanges), mm. Hu = humerus. McIV = wing metacarpal. WP1 to WP4 = pterosaur wing-finger phalanges 1 to 4.

**Licence summary.** Most numbers are facts taken from papers. Facts are not copyrightable, but the tables and wording are, so nothing here copies prose or reproduces a whole table layout. Each source's licence is stated in the source list.

---

## 1. Wing membranes and where they attach

### 1.1 Bats

| Membrane | Attachment / extent | Source (location) | Population | Status | URL (read 2026-11-16; link re-checked 2026-11-17) |
|---|---|---|---|---|---|
| Propatagium | Runs between the neck and digit I (thumb), in front of the arm | Anthwal et al. 2023, Background para. 2 (S1) | General chiropteran anatomy | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC10161559/ |
| Dactylopatagium | Runs between the forelimb digits (II–V). Formed partly by loss of interdigital apoptosis | Anthwal et al. 2023, Background (S1) | General | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC10161559/ |
| Plagiopatagium | Runs between the caudal margin of the forelimb and the hindlimb. Present in all bat species. Develops as outgrowths from the body flank that later merge with the limbs, at embryonic stage 15 (outgrowth) and stage 16 (fusion) in all 16 species examined | Anthwal et al. 2023, Abstract and Table 1 (S1) | 16 species, 9 families (corrected 2026-11-17 per Library Verify) | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC10161559/ |
| Uropatagium | Runs between the hindlimbs and the tail | Anthwal et al. 2023, Background (S1) | General | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC10161559/ |
| Outline landmarks (molossids) | Landmark list for a bat wing outline (molossids): propatagium from the shoulder to digit I; dactylopatagium minus, medius and major (trailing-edge junctions at the distal phalanges of digits III, IV and V); plagiopatagium posterior edge, joining the **foot** (landmark 25); calcar tip (39); end of uropatagium (40) | Argoitia, Teta & Cassini 2025, Table 1 (40 landmarks) (S2) | 6 molossid species, Argentina | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC12690267/ |
| Plagiopatagium distal insertion | Varies by species: at the **tarsus/ankle** in some species, at the **base of the toes** in others (for example *Myotis grisescens* at the tarsus, *M. lucifugus* at the toe bases) | Morgan et al. 2019 (US bat field key), Fig. 2 caption (S3) | *Myotis* spp., USA | Verified (2026-11-17: PMC page opened; caption says *M. grisescens* attaches to the tarsus, *M. lucifugus* "and others" to the foot at the base of the toes) | https://pmc.ncbi.nlm.nih.gov/articles/PMC6537616/ |
| Calcar | A cartilaginous, variably mineralised spur from the calcaneum (ankle) that supports the uropatagium trailing edge. Length and shape vary by species | Stanchak & Santana 2018 (Anat Rec, doi 10.1002/ar.23740); Stanchak et al. 2019 (Evolution) | — | **Unverified**: Wiley/OUP paywall, search snippet only | https://doi.org/10.1002/ar.23740 (Wiley, paywalled; not read) |
| Calcar keel | A keeled calcar is present in *Myotis sodalis* and *M. leibii*; unkeeled in *M. lucifugus* and *M. austroriparius* | Morgan et al. 2019, Fig. 2 caption (S3) | *Myotis* spp., USA | Verified (2026-11-17) | https://pmc.ncbi.nlm.nih.gov/articles/PMC6537616/ |
| Arm-wing / hand-wing partition | Norberg & Rayner 1987, Fig. 2 (p. 347) splits the wing into arm-wing (plagiopatagium, area S_aw, length ℓ_aw) and hand-wing (dactylopatagium, S_hw, ℓ_hw). Tip length ratio T_ℓ = ℓ_hw/ℓ_aw; tip area ratio T_s = S_hw/S_aw; tip shape index I = T_s/(T_ℓ − T_s) | Norberg & Rayner 1987 (S4), via Grok | 257 species | **Unverified**: regulations.gov copy is CloudFront-blocked from the box; Royal Society copy is paywalled. Definitions agree with the abstract (S4) | https://royalsocietypublishing.org/doi/10.1098/rstb.1987.0030 |

**Bat jump technique (affects membrane layout on the ground).** Bats push off from the **wrists** of the folded wings, usually with the feet already off the ground. Wing muscles supply the force (Gardiner & Nudds 2011, Summary; S5). In *Desmodus*, the pectoral limbs generate the upward thrust, the hindlimbs only orient the body, and the thumbs extend the push (Schutt et al. 1997, abstract; S6). Verified.

### 1.2 Pterosaurs

| Item | Finding | Source (location) | Population | Status | URL (read 2026-11-16; link re-checked 2026-11-17) |
|---|---|---|---|---|---|
| Brachiopatagium (main wing membrane) trailing edge | Reaches the **lower leg or ankle** in every specimen where the brachiopatagium is completely preserved. Proposed to be universal in Pterosauria | Elgin, Hone & Frey 2011, Abstract and Discussion (S7) | Review of soft-tissue specimens: *Anurognathus ammoni*, *Beipiaopterus chenianus*, *Eosipterus yangi*, *Eudimorphodon ranzii*, *Jeholopterus ningchengensis*, *Rhamphorhynchus muensteri*, *Sordes pilosus*, plus azhdarchoids SMNK PAL 3830, 3855, 3900, 6409 | Verified | https://app.pan.pl/archive/published/app56/app20090145.pdf |
| Torso/hip and knee attachments | Torso/hip: no specimen conclusively shows it. Knee: suggested only by the Vienna *Pterodactylus* (NHMW 1975/1756). Tibia: *Eosipterus* (membrane reaches about mid-tibia; probably further, possibly the ankle). Attributed to incomplete membranes or post-mortem contraction | Elgin et al. 2011, pp. 103–107 and Discussion (S7) | — | Verified | https://app.pan.pl/archive/published/app56/app20090145.pdf |
| Ankle attachment ≠ broad wing | The trailing-edge curvature and the forelimb:hindlimb length ratio also set the wing shape, so an ankle attachment still allows a high-aspect-ratio wing | Elgin et al. 2011, Abstract (S7) | — | Verified | https://app.pan.pl/archive/published/app56/app20090145.pdf |
| Default model in mass/flight modelling | Broad-chord, **ankle-attached** brachiopatagia for all taxa. Pelvis-attached wings are shown in no specimen; knee-attached wings appear only, ambiguously, in Solnhofen *Pterodactylus* | Witton 2008, §2.2.1 (p. 147–148) (S8) | 20 morphs, 19 genera | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| Wind-tunnel wing model membranes | Propatagium leading edge runs from the neck to the pteroid tip, then to the knuckle (wing-finger base). Cheiropatagium trailing edge runs to the distal end of the lower leg | Wilkinson, Unwin & Ellington 2006, §4 (S9) | *Anhanguera santanae* AMNH 22555, *Coloborhynchus robustus* SMNK 1133 PAL | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC1560000/ |
| Propatagium share of area | The propatagium region, from wing root to knuckle, is over half the projected wing area in ornithocheirids | Wilkinson et al. 2006, Discussion, citing Wilkinson 2002 thesis (S9) | Ornithocheirids | Verified (as stated in S9; the thesis itself was not opened) | https://pmc.ncbi.nlm.nih.gov/articles/PMC1560000/ |
| Pteroid joint geometry | Head offset 30° ventral and 10° medial from the shaft. At maximum extension the shaft points forward, about 10° below horizontal. During flexion the head rotates 20° laterally. At the flexion limit the shaft points ventromedially, 45° below horizontal. A purely medial orientation is impossible without disarticulating the joint | Wilkinson et al. 2006, §3 (S9) | Santana Fm. ornithocheirids | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC1560000/ |
| Propatagium (forewing) aerodynamics | Broad-forewing model reached C_L,max 2.4, against about 1.5 for conventional aerofoils, with max L:D 18.1. Best propatagium deflection 30°–40° over angles of attack 2°–16°. Deflection > 50° gave no further benefit | Wilkinson et al. 2006, §5 (S9) | Wing-section models: chord 173 mm (no forewing), 198 mm (narrow), 218–239 mm (broad); span 706 mm; spar diameter 16 mm (Table 1) | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC1560000/ |
| Pteroid orientation, opposite view | A **medial** pteroid (narrow, shallow triangular propatagium) is supported. An anterior pteroid would fail in bending except at very low mass in steady flight. Medial propatagium area < 0.10 m² for *Coloborhynchus* | Palmer & Dyke 2010, §2 and Results (S10) | *Coloborhynchus robustus* model, span 5.8 m; pteroid preserved length 184 mm (Palmer) or 178 mm (Wilkinson 2006), reconstructed 238 mm; pteroid ≈ 47% of ulna length in ornithocheirids (citing Wilkinson et al. 2006) | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2842769/ |
| Pteroid material values | E = 22 GPa, tensile yield strain 7360 µε, yield stress 162 MPa. Wall thickness 0.47–0.70 mm | Palmer & Dyke 2010, §2(b)–(c) (S10) | Bird bone analogues (Currey 2004) | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2842769/ |
| Uropatagium / cruropatagium | *Sordes*: membrane between the hindlimbs, controlled by pedal digit V | Unwin & Bakhurina 1994 (Nature 371:62), via Grok and via Elgin et al. 2011 | *Sordes pilosus* PIN 2585/3 | **Unverified**: Nature paywall. The ankle attachment in *Sordes* is cited in S7 and S8 (Verified as a citation) | https://doi.org/10.1038/371062a0 (Nature, paywalled; not read); citation in https://app.pan.pl/archive/published/app56/app20090145.pdf (read 2026-11-16) |
| Membrane insertion, alternative view | Pterodactyloid wing narrower, knee/femur insertion; uropatagium from knee to tail | Chatterjee & Templin 2004 (GSA Spec. Pap. 376), pp. 14–18, via Grok | Vienna *Pterodactylus* NHMW 1975/1756 | **Unverified**: the only copy Grok found is on a third-party repository of uncertain legitimacy; I did not open it | https://doi.org/10.1130/0-8137-2376-0.1 (GSA, paywalled; not read) |
| *Pteranodon* membrane insertion | *Pteranodon* patagium attached to the femora | Bennett 2001 (Palaeontographica A 260), via Grok | *Pteranodon* | **Unverified**: not openly available | No open URL found (not read) |

---

## 2. Finger / digit bone proportions

### 2.1 Bats: digits III–V against forearm (Castillo-Figueroa 2020)

Source: Castillo-Figueroa 2020, Zoological Studies 59:e60, Table 1 (S11). 1512 adult dry museum specimens of 97 Neotropical species (MPUJ-MAMM, Bogotá), digital calipers (0.01 mm), one measurer. Table 1 as rendered on PMC has **85** species rows; the text says 97 species. Values are mean ± SD in mm, and n per species is 1–243. LD = metacarpal plus phalanges of that digit.

Range reported in the text (Verified): FL from 28.64 mm (*Rhogeessa io*) to 88.52 mm (*Phyllostomus hastatus*). LD3 from 50.31 mm (*Eptesicus furinalis*, n = 1; the paper's text says *E. fuscus*, which is wrong; corrected per Library Verify 2026-11-17) to 176.2 mm (*Noctilio leporinus*). LD4 from 41.99 mm (*E. furinalis*, n = 1) to 132.67 mm (*N. leporinus*). LD5 from 33.81 mm (*Cynomops planirostris*) to 120.22 mm (*Chrotopterus auritus*).

**Per-species examples** (means from Table 1; ratios are mine):

| Species | Guild | n | FL mm | LD3 mm | LD4 mm | LD5 mm | LD3/FL | LD4/FL | LD5/FL | Status | URL (read 2026-11-16; link re-checked 2026-11-17) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| *Molossus molossus* | UAI (open-space aerial insectivore) | 14 | 38.89 ± 2.51 | 78.08 ± 6.84 | 58.37 ± 3.56 | 39.61 ± 2.35 | 2.01 | 1.50 | 1.02 | Verified (ratios computed) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/ |
| *Molossus rufus* | UAI | 5 | 49.88 ± 0.43 | 100.01 ± 7.12 | 71.85 ± 2.07 | 49.98 ± 1.65 | 2.01 | 1.44 | 1.00 | Verified (ratios computed) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/ |
| *Cynomops planirostris* | UAI | 4 | 32.77 ± 0.52 | 64.1 ± 1.74 | 49.2 ± 1.87 | 33.81 ± 1.10 | 1.96 | 1.50 | 1.03 | Verified (ratios computed) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/ |
| *Saccopteryx bilineata* | BAI (background-cluttered aerial insectivore) | 6 | 47.11 ± 1.67 | 84.41 ± 4.10 | 58.87 ± 2.14 | 58.50 ± 2.27 | 1.79 | 1.25 | 1.24 | Verified (ratios computed) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/ |
| *Pteronotus parnellii* | CAI (cluttered-space aerial insectivore) | 4 | 60.23 ± 1.47 | 95.4 ± 3.50 | 75.24 ± 0.46 | 73.46 ± 0.59 | 1.58 | 1.25 | 1.22 | Verified (ratios computed) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/ |
| *Noctilio leporinus* (fishing bat) | BTP (trawler/piscivore) | 2 | 84.32 ± 1.25 | 176.205 ± 2.80 | 132.67 ± 2.30 | 100.96 ± 2.85 | 2.09 | 1.57 | 1.20 | Verified (ratios computed) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/ |
| *Carollia perspicillata* | CUF (understorey frugivore) | 243 | 41.90 ± 1.78 | 88.24 ± 4.31 | 63.20 ± 3.41 | 63.00 ± 3.24 | 2.11 | 1.51 | 1.50 | Verified (ratios computed) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/ |
| *Phyllostomus hastatus* | CGO (gleaning omnivore) | 13 | 88.52 ± 2.14 | 161.07 ± 4.68 | 118.09 ± 4.49 | 106.99 ± 2.52 | 1.82 | 1.33 | 1.21 | Verified (ratios computed) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/ |
| *Trachops cirrhosus* | CGC (gleaning carnivore) | 6 | 60.24 ± 1.84 | 118.73 ± 3.95 | 87.05 ± 2.78 | 90.115 ± 3.07 | 1.97 | 1.45 | 1.50 | Verified (ratios computed) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/ |

**Guild summaries.** These are medians of species-mean ratios, with min–max in brackets, computed from Table 1. I excluded three rows with internal errors (see Sources disagree 1), leaving 82 of the 85 PMC rows. Library Verify re-computed n and some medians from the official PDF Table 1 (97 species, 94 after exclusions); the fixed cells are marked. Library Verify also recomputed UAI and BTP from the official 97-row table; they stand as printed.

| Guild | Species | LD3/FL | LD4/FL | LD5/FL | LD3/LD5 | Status | URL (read 2026-11-16; link re-checked 2026-11-17) |
|---|---|---|---|---|---|---|---|
| UAI (open-space aerial insectivore) | 6 | 1.98 [1.88–2.07] | 1.49 [1.42–1.71] | 1.03 [1.00–1.71] | 1.92 [1.21–2.00] | Verified (computed; checked against official PDF Table 1 by Library Verify 2026-11-17) | https://zoolstud.sinica.edu.tw/Journals/59/59-60.pdf (official PDF; PMC: https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/) |
| BAI (background aerial insectivore) | 23 | 1.74 [1.36–2.18] | 1.36 [1.06–1.58] | 1.25 [0.99–1.41] | 1.40 [1.26–1.62] | Fixed 2026-11-17 per Library Verify (official PDF Table 1, 97 species); all cells checked against the official table | https://zoolstud.sinica.edu.tw/Journals/59/59-60.pdf (official PDF; PMC: https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/) |
| BTP (trawler/piscivore) | 2 | 2.02 [1.95–2.09] | 1.51 [1.46–1.57] | 1.15 [1.10–1.20] | 1.76 [1.75–1.78] | Verified (computed; checked against official PDF Table 1 by Library Verify 2026-11-17) | https://zoolstud.sinica.edu.tw/Journals/59/59-60.pdf (official PDF; PMC: https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/) |
| CCF (canopy frugivore) | 21 | 2.19 [2.03–2.34] | 1.57 [1.48–1.65] | 1.49 [1.40–1.54] | 1.46 [1.42–1.54] | Fixed 2026-11-17 per Library Verify (official PDF Table 1, 97 species); all cells checked against the official table | https://zoolstud.sinica.edu.tw/Journals/59/59-60.pdf (official PDF; PMC: https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/) |
| CUF (understorey frugivore) | 17 | 2.14 [2.07–2.45] | 1.57 [1.51–1.72] | 1.50 [1.46–1.72] | 1.44 [1.40–1.48] | n only fixed (medians stand) 2026-11-17 per Library Verify (official PDF Table 1, 97 species); all cells checked against the official table | https://zoolstud.sinica.edu.tw/Journals/59/59-60.pdf (official PDF; PMC: https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/) |
| CGN (nectarivore) | 10 | 2.04 [1.84–2.19] | 1.46 [1.34–1.51] | 1.36 [1.24–1.43] | 1.52 [1.39–1.66] | Fixed 2026-11-17 per Library Verify (official PDF Table 1, 97 species); all cells checked against the official table | https://zoolstud.sinica.edu.tw/Journals/59/59-60.pdf (official PDF; PMC: https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/) |
| All species after exclusions | 94 | 2.03 [1.36–2.45] | 1.50 [1.06–1.72] | 1.43 [0.99–1.72] | 1.44 [1.17–2.00] | Fixed 2026-11-17 per Library Verify (official PDF Table 1, 97 species); all cells checked against the official table | https://zoolstud.sinica.edu.tw/Journals/59/59-60.pdf (official PDF; PMC: https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/) |

The author's own conclusion (Results/Discussion of S11): aerial foragers in uncluttered or background space (BAI, UAI) have a higher LD3/FL and a lower LD5/FL than gleaners in clutter. **Corrected 2026-11-17 per Library Verify:** the LD3/FL part does not hold for BAI on the official medians (BAI 1.74 vs gleaners about 2.14–2.19); the lower LD5/FL part stands. Metacarpals are associated with open-space and background insectivores; terminal phalanges separate the guilds. **For a dragon:** a fast, open-air flyer gets a long digit III and a short digit V (LD5 ≈ FL). A slow, manoeuvrable flyer gets digit V ≈ 1.5 × FL, which makes a broad wing.

Per-bone metacarpal and phalanx lengths (P2.1 … P5.3) were measured but are only published as MDS loadings (supplement Table S1, docx). **Not tabulated as mm**; see Not found.

### 2.2 Pterosaurs: wing-finger phalanges against humerus

**(a) Allometric equations across pterosaurs.** Brower & Veinus 1981, Table 1 (S12). RMA regressions of log length (cm) on log mass (g), 16 taxa, mass about 4 g to 15 kg. Length = a·M^b. Masses are the authors' geometric-model estimates, not weighed animals.

| Bone | a | b (± SE) | r | N | Isometry 0.33 rejected? | Status | URL (read 2026-11-16; link re-checked 2026-11-17) |
|---|---|---|---|---|---|---|---|
| Humerus | 0.7279 | 0.3528 ± 0.0152 | 0.985 | 16 | no | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Radius + ulna | 1.110 | 0.3582 ± 0.0135 | 0.989 | 16 | no | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Metacarpal IV, pterodactyloids | 0.7264 | 0.4619 ± 0.0173 | 0.992 | 11 | yes (+) | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Metacarpal IV, "rhamphorhynchoids" | 0.4939 | 0.2977 ± 0.0083 | 0.998 | 5 | yes (−) | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Wing phalanx 1 | 1.033 | 0.4316 ± 0.0328 | 0.953 | 16 | yes (+) | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Wing phalanx 2 | 1.056 | 0.4089 ± 0.0242 | 0.972 | 16 | yes (+) | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Wing phalanx 3 | 0.8789 | 0.3952 ± 0.0242 | 0.970 | 16 | yes (+) | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Wing phalanx 4 | 0.8974 | 0.3323 ± 0.0300 | 0.933 | 16 | no | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Femur | 0.6682 | 0.3852 ± 0.0279 | 0.957 | 16 | no | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Tibia | 0.9752 | 0.3915 ± 0.0224 | 0.973 | 16 | yes (+) | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Bones in one wing (sum) | 6.351 | 0.3935 ± 0.0173 | 0.984 | 16 | yes (+) | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Proximal shaft diameter: humerus / WP1 / femur | 0.07799 / 0.06205 / 0.04307 | 0.4040 / 0.4247 / 0.4394 | 0.908 / 0.905 / 0.874 | 16 | — | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |

The authors' summary: wing-bone lengths grade from isometry in the proximal (humerus, radius/ulna) and distal (WP4) elements to positive allometry in the middle elements (McIV in pterodactyloids, WP1–WP3).

**Ratios implied by those equations** (Verified, computed; my arithmetic, ratio = bone/humerus):

| Body mass | McIV (ptero-dactyloid) | WP1 | WP2 | WP3 | WP4 | Radius+ulna | Femur | Status | URL (read 2026-11-16; link re-checked 2026-11-17) |
|---|---|---|---|---|---|---|---|---|---|
| 10 g | 1.28 | 1.70 | 1.65 | 1.33 | 1.18 | 1.54 | 0.99 | Verified (computed) | https://kuscholarworks.ku.edu/handle/1808/3677 |
| 100 g | 1.65 | 2.04 | 1.88 | 1.47 | 1.12 | 1.56 | 1.07 | Verified (computed) | https://kuscholarworks.ku.edu/handle/1808/3677 |
| 1 kg | 2.12 | 2.45 | 2.14 | 1.62 | 1.07 | 1.58 | 1.15 | Verified (computed) | https://kuscholarworks.ku.edu/handle/1808/3677 |
| 15 kg (≈ *Pteranodon*) | 2.85 | 3.03 | 2.49 | 1.82 | 1.01 | 1.61 | 1.25 | Verified (computed) | https://kuscholarworks.ku.edu/handle/1808/3677 |

Do not extrapolate above about 15 kg: the largest animal in the dataset is *Pteranodon* at 14.94 kg.

**(b) Growth series within taxa (with 95% CIs).** Cerqueira et al. 2026, J. Anat., Tables 1–3 (S13). Log lengths, standardized major axis (SMA). 127 specimens measured from scaled photographs with MB-Ruler: Eupterodactyloidea N = 28, Euctenochasmatia N = 18, *Rhamphorhynchus muensteri* N = 81. The log base is not stated; the slope b does not depend on it.

| Group | X (proxy) | Y | b (95% CI) | R² | Allometry | Status | URL (read 2026-11-16; link re-checked 2026-11-17) |
|---|---|---|---|---|---|---|---|
| Eupterodactyloidea | radius/ulna | humerus | 1.06 (0.97–1.17) | 0.95 | isometric | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ |
| Eupterodactyloidea | radius/ulna | McIV | 1.27 (1.16–1.40) | 0.94 | positive | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ |
| Eupterodactyloidea | radius/ulna | WP1 | 1.19 (1.08–1.31) | 0.95 | positive | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ |
| Eupterodactyloidea | radius/ulna | WP2 | 1.20 (1.11–1.30) | 0.97 | positive | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ |
| Eupterodactyloidea | radius/ulna | WP3 | 1.19 (1.07–1.34) | 0.95 | positive | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ |
| Eupterodactyloidea | radius/ulna | WP4 | 1.27 (1.02–1.58) | 0.83 | positive | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ |
| Eupterodactyloidea | radius/ulna | femur | 0.96 (0.87–1.07) | 0.96 | isometric | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ |
| Euctenochasmatia | tibia/tarsus | humerus | 0.78 (0.68–0.88) | 0.94 | negative | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ |
| Euctenochasmatia | tibia/tarsus | WP1 | 0.97 (0.87–1.09) | 0.95 | isometric | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ |
| Euctenochasmatia | tibia/tarsus | WP4 | 0.78 (0.67–0.91) | 0.93 | negative | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ |
| *R. muensteri* | WP1 | humerus | 0.82 (0.76–0.89) | 0.88 | negative | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ |
| *R. muensteri* | WP1 | McIV | 0.71 (0.66–0.78) | 0.88 | negative | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ |
| *R. muensteri* | WP1 | WP2 | 1.07 (1.04–1.11) | 0.98 | positive | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ |
| *R. muensteri* | WP1 | WP3 | 1.09 (1.04–1.14) | 0.96 | positive | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ |
| *R. muensteri* | WP1 | WP4 | 0.99 (0.93–1.05) | 0.94 | isometric | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ |
| *R. muensteri* | WP1 | tail | 0.97 (0.90–1.05) | 0.93 | isometric | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC13645348/ |

**For a dragon:** in pterodactyloid-like builds, the hand and wing finger grow faster than the arm (b > 1), so bigger animals get proportionally longer wing fingers. In a *Rhamphorhynchus*-like build the humerus falls behind the wing finger as the animal grows.

**(c) Individual specimens** (lengths in mm; ratios are mine, bone/humerus):

| Specimen | Hu | Ulna | McIV | WP1 | WP2 | WP3 | WP4 | Ul/Hu | McIV/Hu | WP1/Hu | WP2/Hu | WP3/Hu | WP4/Hu | (WP1–4)/Hu | Source | Status | URL (read 2026-11-16; link re-checked 2026-11-17) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| *Pterodactylus antiquus* RGM St 18 184 | 44 | 60 | 44 | 58 | 58 | 51 | 39 | 1.36 | 1.00 | 1.32 | 1.32 | 1.16 | 0.89 | 4.68 | Elgin & Hone 2020 Table 1 (S14) | Verified (ratios computed) | https://palaeo-electronica.org/content/2020/2979-solnhofen-pterodactyloids-table |
| *Germanodactylus cristatus* BSP AS I 745 | 56 | 75 | 66 | 84 | 78 | 66 | 55 | 1.34 | 1.18 | 1.50 | 1.39 | 1.18 | 0.98 | 5.05 | S14 | Verified (ratios computed) | https://palaeo-electronica.org/content/2020/2979-solnhofen-pterodactyloids-table |
| *Aurorazhdarcho micronyx* NMB Sh 110 | 63 | 84 | 99 | 123 | 85 | 59 | 52 | 1.33 | 1.57 | 1.95 | 1.35 | 0.94 | 0.83 | 5.06 | S14 | Verified (ratios computed) | https://palaeo-electronica.org/content/2020/2979-solnhofen-pterodactyloids-table |
| *Cycnorhamphus suevicus* GPIT 80 | 64 | 88 | 111 | 142 | 117 | 85 | 71 | 1.38 | 1.73 | 2.22 | 1.83 | 1.33 | 1.11 | 6.48 | S14 | Verified (ratios computed) | https://palaeo-electronica.org/content/2020/2979-solnhofen-pterodactyloids-table |
| "Big wings" SMNK PAL 6990 (large Solnhofen pterodactyloid) | 129 | 174 | 165 | 224 | 216 | 142 | 72 | 1.35 | 1.28 | 1.74 | 1.67 | 1.10 | 0.56 | 5.07 | S14 | Verified (ratios computed) | https://palaeo-electronica.org/content/2020/2979-solnhofen-pterodactyloids-table |
| "Big wings" MB.R.5591.1 (Hu reconstructed*) | 117* | 158 | 141 | 215 | 191 | 141 | 93 | 1.35 | 1.21 | 1.84 | 1.63 | 1.21 | 0.79 | 5.47 | S14 | Verified (ratios computed) | https://palaeo-electronica.org/content/2020/2979-solnhofen-pterodactyloids-table |
| *Rhamphorhynchus muensteri* NHMUK PV OR 37002 (giant) | 70–78 | 103 | — | 133 (as preserved) | 168 / 176 | 139 / 136 | 136 / 137 | 1.32 | — | ≥1.71 | 2.15 | 1.78 | 1.74 | — | Hone & McDavid 2025 Table 1 (S15); ratios use Hu = 78 and the left side | Verified (ratios computed) | https://pmc.ncbi.nlm.nih.gov/articles/PMC11700493/ |

S14 also gives half-span (forelimb total, FL) for SMNK PAL 6990 as 1120 mm and for *Cycnorhamphus* GPIT 80 as 678 mm. The wing finger (WP1–4) makes up 58% and 61% of the forelimb in those two specimens (654/1120 and 415/678). Verified (computed).

**Giant humeri** (Verified, single values, no spread): *Quetzalcoatlus northropi* holotype humerus, complete left, **544 mm** (Witton & Habib 2010, text; S16). Brower & Veinus 1981 (p. 26; S12) give it as 52 cm, citing Lawson 1975, and give *Pteranodon* humerus as 29 cm (citing Bramwell & Whitfield 1974). *Q. northropi* and *Hatzegopteryx* humeral diaphysis widths are both 80 mm anteroposterior after correcting for crushing (*Hatzegopteryx* was reported as 90 mm) (S16). See Sources disagree 5.

---

## 3. Wing loading and aspect ratio against mass

### 3.1 Bats

| Relation (M in g, b in cm, S in cm², Q in g cm⁻²) | a | b | r | N | Source | Status | URL (read 2026-11-16; link re-checked 2026-11-17) |
|---|---|---|---|---|---|---|---|
| Span = a·M^b | 11.63 | 0.3240 ± 0.0214 (SE) | 0.969 | 14 | Brower & Veinus 1981 Table 2, bat data from Greenewalt 1962 (S12) | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Mass = a·S^b | 0.01442 | 1.524 ± 0.120 | 0.959 | 15 | same | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Wing loading Q = a·M^b | 0.05231 | 0.4021 ± 0.045 | 0.908 | 14 | same | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Same relations in SI (my conversion): b(m) ≈ 1.09·M(kg)^0.324; Q(N m⁻²) ≈ 82·M(kg)^0.402 | — | — | — | — | computed from the rows above | Verified (computed) | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Mass (kg) = 0.5088·b(m)^3.0294 | — | — | — | 102 bats, b = 0.16–1.3 m | Witton 2008 eq. 15, data from Norberg & Rayner 1987 and Norberg et al. 2000 (S8) | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| All bats, RMA, SI units: b = 1.200·M^0.322; S = 0.203·M^0.639; AR = 12.44·M^0.148; Q = 67.16·M^0.444 | — | — | — | 257 species | Norberg & Rayner 1987 Table 4 (p. 366), via Grok (S4) | **Unverified**: PDF blocked; Grok reports no 95% CIs printed, only significance asterisks | https://royalsocietypublishing.org/doi/10.1098/rstb.1987.0030 |
| Microchiroptera: b = 1.262·M^0.332; S = 0.183·M^0.614; AR = 17.10·M^0.209; Q = 104.6·M^0.541 | — | — | — | 208 | Norberg & Rayner 1987 Table 4, via Grok | **Unverified** | https://royalsocietypublishing.org/doi/10.1098/rstb.1987.0030 |
| Pteropodids: b = 1.2·M^0.35, S = 0.24·M^0.71, AR = 8.6·M^0.11, Q ≈ 45·M^0.33 | — | — | — | — | Norberg & Rayner 1987 p. 382, via Grok | **Unverified** | https://royalsocietypublishing.org/doi/10.1098/rstb.1987.0030 |
| Bat mass range: < 0.002 kg (*Craseonycteris*) to about 1.5 kg (*Pteropus*) | — | — | — | — | Norberg & Rayner 1987 p. 400, via Grok | **Unverified** | https://royalsocietypublishing.org/doi/10.1098/rstb.1987.0030 |

Qualitative findings (Verified from the Norberg & Rayner 1987 abstract, S4): high flight speed goes with high wing loading. Fast hawkers of high-flying insects have small, pointed wings. Gleaners in clutter have short, rounded tips and often short, broad wings. Piscivores have very long wings with long, rounded tips. Carnivores carry loads and have relatively large wing areas. Speed correlates with mass, wing loading and tip proportions, but not directly with aspect ratio.

### 3.2 Pterosaurs: mass, span, area, AR, Q by taxon

Witton 2008, Table 4 (p. 151) (S8). Masses come from a skeletal-mass method (bone density 2.0 g cm⁻³ and the Prange et al. 1979 bird skeletal-mass regression), applied through his scaling equation; wings are ankle-attached. Single model values per taxon; no spread given. AR and Q are as printed.

| Taxon | b (m) | Skeletal mass (printed "kg", actually g; see Sources disagree 4) | M (kg) | S (m²) | AR | Q (N m⁻²) | Limb ratio | Status | URL (read 2026-11-16; link re-checked 2026-11-17) |
|---|---|---|---|---|---|---|---|---|---|
| *Preondactylus* | 0.45 | 5.43 | 0.08 | 0.030 | 6.59 | 25.27 | 3.02 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Dimorphodon* | 1.01 | 86.43 | 1.24 | 0.153 | 6.68 | 79.76 | 3.28 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Anurognathus* | 0.35 | 2.46 | 0.04 | 0.016 | 8.01 | 22.39 | 3.47 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Eudimorphodon* | 0.85 | 27.89 | 0.40 | 0.079 | 9.21 | 50.00 | 4.38 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Rhamphorhynchus* | 0.89 | 18.33 | 0.26 | 0.068 | 11.75 | 37.99 | 5.71 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Sordes* | 0.56 | 10.04 | 0.14 | 0.044 | 7.14 | 32.23 | 3.72 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Nurhachius* | 2.28 | 278.40 | 4.00 | 0.361 | 14.39 | 108.61 | 4.52 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Anhanguera* | 4.01 | 833.91 | 11.98 | 0.964 | 16.68 | 121.88 | 4.99 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Pteranodon* (large) | 5.96 | 2553.54 | 36.68 | 1.990 | 17.85 | 180.82 | 4.97 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Pteranodon* (small) | 3.70 | 732.97 | 10.53 | 0.765 | 17.90 | 135.02 | 4.97 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Nyctosaurus* | 2.04 | 185.21 | 2.66 | 0.182 | 22.92 | 143.72 | 7.81 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Pterodactylus* | 0.43 | 4.62 | 0.07 | 0.018 | 10.10 | 35.55 | 3.25 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Huanhepterus* | 2.42 | 404.94 | 5.82 | 0.735 | 7.97 | 77.65 | 3.00 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Ctenochasma* | 0.68 | 11.72 | 0.17 | 0.051 | 9.22 | 32.65 | 3.50 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Pterodaustro* | 1.19 | 60.70 | 0.87 | 0.128 | 11.03 | 66.61 | 4.93 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Dsungaripterus* | 2.51 | 627.02 | 9.01 | 0.624 | 10.09 | 141.58 | 3.23 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Sinopterus* | 1.14 | 63.23 | 0.91 | 0.146 | 8.91 | 61.08 | 3.16 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Huaxiapterus* | 1.43 | 71.63 | 1.03 | 0.209 | 9.77 | 48.23 | 3.08 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Zhejiangopterus* | 2.90 | 646.60 | 9.29 | 1.027 | 8.19 | 88.69 | 2.80 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| *Quetzalcoatlus* | 9.64 | 18034.65 | 259.06 | 11.363 | 8.18 | 223.66 | 2.81 | Verified | https://epub.ub.uni-muenchen.de/12010/ |

Limb ratio = (humerus + ulna + carpals + McIV + wing phalanges) / (femur + tibia). Pterosaur AR ranges from 6.6 (*Preondactylus*) to 22.9 (*Nyctosaurus*). Basal pterosaurs span an AR range of 5.16, pterodactyloids 14.95. Most taxa cluster at AR 7.3–10.4 with limb ratio 2.7–3.7 (S8 text). Verified.

Brower & Veinus 1981, Table 4 (S12), lighter older estimates, per specimen. Columns: mass g, area cm², span cm, AR, Q g cm⁻². Example: *Pteranodon ingens* specimen 27: 14,940 g, 25,300 cm², 695 cm, AR 19.09, Q 0.5904 g cm⁻² (= 57.9 N m⁻²). Verified. I checked the AR and Q arithmetic (695²/25300 = 19.09; 14940/25300 = 0.5905).

Witton & Habib 2010, Table 1 (S16), compiles published planforms (Verified):

| Taxon | b (m) | M (kg) | S (m²) | AR | Q (N m⁻²) | Original source cited in S16 | URL (read 2026-11-16; link re-checked 2026-11-17) |
|---|---|---|---|---|---|---|---|
| *Pteranodon* | 6.95 | 14.95 | 2.53 | 19.10 | 57.93 | Brower 1983 | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ (Table 1) |
| *Pteranodon* | 6.95 | 16.6 | 4.62 | 10.455 | 35.248 | Hazlehurst & Rayner 1992 | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ (Table 1) |
| *Pteranodon* | 6.95 | 16.60 | 2.65 | 18.23 | 61.45 | Chatterjee & Templin 2004 | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ (Table 1) |
| *Pteranodon* | 5.43 | 36.68 | 1.60 | 18.42 | 224.79 | Witton 2008 (but see Sources disagree 3) | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ (Table 1) |
| *Quetzalcoatlus* | 10.39 | 70.00 | 9.55 | 11.30 | 71.91 | Chatterjee & Templin 2004 | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ (Table 1) |
| *Quetzalcoatlus* | 9.64 | 259.06 | 11.36 | 8.18 | 223.66 | Witton 2008 | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ (Table 1) |
| *Diomedea exulans* (wandering albatross) | 3.46 / 3.03 | 8.16 / 8.73 | 0.66 / 0.61 | 18.00 / 15.03 | 120.71 / 140.17 | Pennycuick; Spear & Ainley | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ (Table 1) |

### 3.3 Pterosaur allometry of the whole wing

| Relation | Equation | N | Source | Status | URL (read 2026-11-16; link re-checked 2026-11-17) |
|---|---|---|---|---|---|
| Span vs mass (RMA, cm, g) | b = 12.24·M^0.4014 (SE 0.0161, r 0.987) | 16 | Brower & Veinus 1981 Table 2 (S12) | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Mass vs span (RMA) | M = 0.001951·b^2.491 | 16 | S12 Table 2 | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Wing area vs mass (RMA) | S = 12.43·M^0.7877 (r 0.996) | 16 | S12 Table 2 | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Wing loading vs mass (RMA) | Q = 0.07472·M^0.2274 (g cm⁻², g) | 16 | S12 Table 2 | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Aspect ratio vs mass | AR = 7.9012·M^0.1017 (r 0.079, i.e. no meaningful relation) | 16 | S12 Table 2 | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| Hang-gliders, for comparison: Q = 0.007527·M^0.3713 | — | 99 | S12 Table 2 | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| All pterosaurs, mass vs span (kg, m) | m = 0.551·b^2.516 | 20 morphs | Witton 2008 eq. 8 (S8) | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| Basal pterosaurs | m = 0.681·b^2.807 | — | Witton 2008 eq. 9 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| Pterodactyloids | m = 0.519·b^2.550 | — | Witton 2008 eq. 10 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| Earlier "light" fits, as quoted by Witton 2008 | m = 0.1833·b^2.4823 (Brower & Veinus); 0.206·b^2.3329 (Hazlehurst & Rayner 1992); 0.1863·b^2.4767 (Chatterjee & Templin 2004) | — | Witton 2008 eqs. 11–13 | Verified (as quoted) | https://epub.ub.uni-muenchen.de/12010/ |
| Modern birds, mass vs span | m = 0.623·b^2.346 | 96 birds, b = 0.089–3.5 m | Witton 2008 eq. 14 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| Ornithocheirid subset (*Nurhachius*, *Nyctosaurus*, *Pteranodon*, *Anhanguera*, Witton 2008 data) | M = 0.4526·b^2.4232 (kg, m) → 32 kg at 5.8 m | 4 taxa | Palmer & Dyke 2010 §2(c) (S10) | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2842769/ |

---

## 4. Quadrupedal launch: mechanics and numbers

### 4.1 Bats (the living model)

| Quantity | Value | Spread | Population | Source | Status | URL (read 2026-11-16; link re-checked 2026-11-17) |
|---|---|---|---|---|---|---|
| Peak vertical force, *Desmodus rotundus* | up to **9.5 × body weight**, reached in about **30 ms** | max | common vampire bat, N = 12 jumps | Schutt et al. 1997, abstract (S6) | Verified | https://pubmed.ncbi.nlm.nih.gov/9359889/ |
| Mean impulse | 0.0580 N s | ± 0.007 SD | N = 12 | S6 abstract | Verified | https://pubmed.ncbi.nlm.nih.gov/9359889/ |
| Mean take-off velocity | 2.38 m s⁻¹ | ± 0.24 SD | N = 12 | S6 abstract | Verified | https://pubmed.ncbi.nlm.nih.gov/9359889/ |
| "Typically" 6.51 × body weight (Schutt et al. as cited) | 6.51 × BW | — | — | Gardiner & Nudds 2011, Introduction (S5) | Verified (secondary citation; see Sources disagree 6) | https://pmc.ncbi.nlm.nih.gov/articles/PMC3110502/ |
| Who does what | Pectoral limbs thrust; hindlimbs stabilise and orient the body over the pectoral limbs; thumbs stabilise and lengthen the force pulse | — | — | S6 abstract | Verified | https://pubmed.ncbi.nlm.nih.gov/9359889/ |
| Take-off angle | 70°–80° from horizontal, no difference between species | — | 5 insectivorous species | Gardiner & Nudds 2011, Results; Table 1 (S5) | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC3110502/ |

Gardiner & Nudds 2011 Table 1 (S5), means ± s.e., high-speed video at 125–250 frames s⁻¹, bats jumping from a flat platform (Verified):

| Species | N | Mass g | FL mm | Take-off angle ° | Jump height m | Take-off speed m s⁻¹ | Jump force N | Force / weight | URL (read 2026-11-16; link re-checked 2026-11-17) |
|---|---|---|---|---|---|---|---|---|---|
| *Myotis blythii* | 5 | 26.3 ± 0.7 | 59.9 ± 0.6 | 72.9 ± 5.9 | 0.130 ± 0.010 | 1.55 ± 0.10 | 1.21 ± 0.06 | 4.68 ± 0.35 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3110502/ |
| *Myotis capaccinii* | 15 | 8.7 ± 0.4 | 42.0 ± 0.4 | 66.0 ± 3.4 | 0.064 ± 0.006 | 1.01 ± 0.06 | 0.29 ± 0.03 | 3.37 ± 0.20 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3110502/ |
| *Myotis myotis* | 12 | 26.4 ± 0.5 | 63.3 ± 0.4 | 73.7 ± 3.8 | 0.131 ± 0.006 | 1.59 ± 0.06 | 1.18 ± 0.04 | 4.57 ± 0.22 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3110502/ |
| *Rhinolophus blasii* | 6 | 10.5 ± 0.7 | 46.0 ± 0.6 | 83.2 ± 5.4 | 0.066 ± 0.009 | 1.02 ± 0.09 | 0.36 ± 0.05 | 3.55 ± 0.32 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3110502/ |
| *Miniopterus schreibersii* | 17 | 11.5 ± 0.4 | 45.4 ± 0.3 | 70.5 ± 3.2 | 0.092 ± 0.005 | 1.34 ± 0.05 | 0.48 ± 0.03 | 4.24 ± 0.18 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3110502/ |

Scaling across those 5 species, least squares, exponent ± s.e. (S5, Table 2; Verified): forearm ∝ M^0.342±0.028 (r² 0.981); jump height ∝ M^0.631±0.110 (r² 0.916); take-off speed ∝ M^0.381±0.096 (r² 0.839); jump force ∝ M^1.255±0.074 (r² 0.990). The models predict a force exponent of 0.666, so force rises much faster with size than the models expect. *Desmodus* lies far outside the 95% confidence limits for speed and force, but within them for forearm length.

Altenbach 1979 vampire-bat film data (best 2.4 m s⁻¹; others 1.3–2.2 m s⁻¹; jump angle 50–70°; brachial adduction about 60 ms after jump start): via Grok. **Unverified**: the archive copy was not opened.

### 4.2 Pterosaurs

| Finding | Numbers | Source | Status | URL (read 2026-11-16; link re-checked 2026-11-17) |
|---|---|---|---|---|
| Humerus is much stronger in bending than the femur. Strength ratio (Z/L of humerus ÷ Z/L of femur, natural log) for all three pterosaurs lies above every sampled bird. The closest bird is *Phaethon rubricauda*, which has reduced femora (difference p < 0.001) | Fig. 2. No per-species ratio printed in the text | Habib 2008, Results (S17). Pterosaurs: *Anhanguera piscator* NSM-PV 19892 (span about 5 m), *Zhejiangopterus linhaiensis* ZMNH M1323 (about 3 m), *Dorygnathus banthensis* PSB 759 | Verified | https://epub.ub.uni-muenchen.de/12011/ |
| In birds, femur strength exceeds humerus strength above about **500 g** body mass | Fig. 1 | Habib 2008 (S17); CT data, NMNH bird collection | Verified | https://epub.ub.uni-muenchen.de/12011/ |
| Expected bird long-bone masses (Cubo & Casinos 1994): humerus 0.351 g and femur 0.160 g at 100 g; humerus 36.264 g and femur 32.143 g at 10 kg | — | Habib 2008 (S17) | Verified (as quoted) | https://epub.ub.uni-muenchen.de/12011/ |
| Long-bone length / diameter exponents vs mass. Birds: femur 0.35 / 0.42 (Prange 1979), 0.342 / 0.383 (Olmos 1996); humerus 0.46 / 0.39 (Prange), 0.430 / 0.368 (Olmos). Pterosaurs: femur 0.385 / 0.439, humerus 0.353 / 0.404 (Brower & Veinus 1981) | — | Habib 2008 Table 1 (S17) | Verified | https://epub.ub.uni-muenchen.de/12011/ |
| Bone strength scaling in birds: I and J exponents femur 1.662 / 1.647, humerus 1.405 / 1.409 (Cubo & Casinos 1998) | — | Habib 2008 Table 2 (S17) | Verified | https://epub.ub.uni-muenchen.de/12011/ |
| Azhdarchid relative failure force (RFF) vs a bird of the same mass. Humerus: *Bennettazhia* 2.66×, *Montanazhdarcho* 1.90×, *Quetzalcoatlus* sp. 2.69×, *Q. northropi* 2.82× / 2.62× / 2.25× at 180 / 200 / 250 kg. Femur of *Quetzalcoatlus* sp.: 0.30× | — | Witton & Habib 2010 Table 2 (S16) | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ |
| Launch sequence: from a standing quadrupedal start, the animal vaults over its forelimbs, "superficially vampire bat-like", then flaps hard to gain height. Larger pterosaurs have disproportionately long wing metacarpals, which adds leverage in the vault | — | Witton & Habib 2010, Discussion, Fig. 6 (S16) | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ |
| A 200–250 kg, 10 m span azhdarchid "could easily launch from a standing start" without slopes or headwinds | No numbers given | S16, citing Habib unpublished data | Verified (as stated; the underlying data are unpublished) | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ |
| Bipedal-launch view: no method for launching a pterosaur above 70 kg; an 85 kg *Q. northropi* could not take off running even with a strong headwind; giants need downhill runs and headwinds | — | Chatterjee & Templin 2004, as reported in S16 and Habib 2008 | Verified (as reported in S16 / S17) | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/; https://epub.ub.uni-muenchen.de/12011/ |
| Flight muscle used mainly for launch and climb-out. *Quetzalcoatlus*: best glide 13.3 m s⁻¹ and minimum sink speed 8.80 m s⁻¹ (narrow Chatterjee & Templin planform), or 24.9 and 16.3 m s⁻¹ (broad Witton planform). Maximum-range speed under anaerobic power 48.3 m s⁻¹, with 1 m s⁻¹ climb-out; one-minute burst range 2.88 km. Flight program settings: C_L max 1.8 steady, 2.2 for pterosaurs; flap:glide ratio 0.2; span hard stop at 80% of resting span | single model runs | S16 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ |
| "Launch in under 1 s"; burst speed 120 km h⁻¹, cruise 90 km h⁻¹ | — | Press interviews around Habib 2008 / Witton & Habib 2010, via Grok | **Unverified**: not in any paper I opened | No URL; press claim only (not read) |

---

## 5. Largest flyers: mass vs span and limits

| Claim | Numbers | Source | Status | URL (read 2026-11-16; link re-checked 2026-11-17) |
|---|---|---|---|---|
| Procellariiform flapping and wing scaling (major axis, M in kg, 95% CI). High-frequency flapping HF = 6.1·M^−0.30 (CI γ 5.9–6.4, β −0.33 to −0.27). Low-frequency LF = 3.9·M^−0.18 (CI 3.8–4.0; −0.20 to −0.16). WS = 1.3·M^0.37 m (CI 1.2–1.3; 0.34–0.40). WA = 0.15·M^0.58 m² (CI 0.14–0.16; 0.53–0.62) | 26 birds of 5 species for flapping; 22 birds of 4 species for wing size | Sato et al. 2009 Table 1 (S18) | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2670537/ |
| Size limit for albatross-like soarers: HF and LF lines cross at **41 kg** (bootstrap 95% CI **26–75 kg**, 100,000 replicates), equal to **5.1 m** span | — | S18 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2670537/ |
| The wandering albatross, about 12 kg, may be a practical limit with a safety margin | — | S18 Abstract | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2670537/ |
| Same bird scaling gives *Pteranodon* 93 kg at 6.95 m and *Quetzalcoatlus* 276 kg at 10.4 m | — | S18 Discussion | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2670537/ |
| Largest reliable pterosaur size: **10–11 m span, 200–250 kg**. Proposed 13 m / 544 kg giants are overestimates. *Arambourgiania* 11–13 m estimates are unlikely; *Hatzegopteryx* humerus matches *Q. northropi* after correcting for crushing | — | Witton & Habib 2010 (S16) | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ |
| *Pteranodon* density check: 93 kg in about 40 L of volume would mean 2.235 g cm⁻³. Most birds are 0.6–0.9 g cm⁻³; non-flying animals about 1 g cm⁻³ | — | S16 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ |
| Henderson 2010 *Q. northropi* 544 kg (torso 1.8 m long). S16 says that torso is about 2.77× too long (actual about 0.65 m); correcting it gives about 240 kg. Henderson's slimmer alternative model: axial mass 198 kg vs 474 kg | — | S16, citing Henderson 2010 | Verified (as reported in S16). Henderson's own tables: Unverified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ |
| A 13 m pterosaur would weigh almost twice a 10 m one under pterosaur scaling | — | S16 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ |
| Pterosaur mass range in Witton 2008: 0.035 kg (*Anurognathus*, b = 0.352 m) to 259 kg (*Quetzalcoatlus*, b = 9.64 m), over 6000× | — | S8 §3.1 | Verified | https://epub.ub.uni-muenchen.de/12010/ |
| Brower & Veinus 1981 *Q. northropi* estimates: 74.54 kg (from 11.43 m span); Q 0.8888 g cm⁻² (≈ 87 N m⁻²); S 8.386 m². Scaled-up *Pteranodon*: 86.13 kg / 8.136 m² from humerus scaling (52/29), or 66.45 kg / 6.844 m² from span scaling (11.43/6.95) | — | S12 pp. 26–27 | Verified | https://kuscholarworks.ku.edu/handle/1808/3677 |
| *Argentavis magnificens* (largest flying bird; Miocene, Argentina): M 70 kg, W 687 N, S 8.11 m², b 7 m, AR 6.04, b/l 5.56, Q 84.6 N m⁻², power available 170 W, cruise 19 m s⁻¹ (67 km h⁻¹). Minimum power for sustained flight 600 W (about 3.5× available). Minimum glide angle about 3° at 18 m s⁻¹. Cannot do continuous flapping or a standing take-off; can take off running down a 10° slope at a running speed of 5 m s⁻¹; take-off into a headwind of about 5 m s⁻¹ is a separate scenario (corrected 2026-11-17 per Library Verify) | single model | Chatterjee, Templin & Campbell 2007 Table 1 and text (S19) | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC1906724/ |
| California condor (same table): 9.5 kg, S 1.32 m², b 2.74 m, AR 5.7, Q 70.6 N m⁻², 17 m s⁻¹ | — | S19 Table 1 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC1906724/ |
| Large bustards (up to 22 kg) may be the upper limit of flight for birds | — | Henderson 2010, as cited in S16 | Verified (as cited) | https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/ |
| Pennycuick: flapping power required ∝ M^7/6 and available ∝ M^2/3; continuous-flapping ceiling about 12 kg | — | via Grok | **Unverified**: Pennycuick's books were not opened. The 12 kg figure I did verify is Sato's albatross figure, not Pennycuick's | No URL; Pennycuick books not opened |
| Mass of 4.4 m span *Anhanguera piscator* (humerus 255.0 mm, midshaft 32.1 mm, cortex 1.11 mm) | — | Naish et al. 2021, via Grok | **Unverified**: not opened | No URL identified; not opened |

**Rule of thumb for the engine (Verified, computed from S8):** with the Witton heavy scaling m = 0.551·b^2.516, a 4 m dragon is about 18 kg, 7 m about 74 kg, 10 m about 181 kg and 12 m about 286 kg. The light scaling (0.19·b^2.48) gives about 1/3 of those masses. Taking Q ≈ 220 N m⁻² as the highest well-supported value (*Quetzalcoatlus*, Witton 2008) and Sato's 41 kg (CI 26–75) as the limit for albatross-style flappers brackets what the papers consider believable for membrane-winged giants.

---

## 6. Not found

- **Norberg & Rayner 1987 Table 1 and Table 4** (per-species bat mass, span, area, AR, Q, tip indices; family allometries). The regulations.gov copy returned CloudFront 403 from the box; the Royal Society copy is paywalled. Grok's transcribed coefficients are listed as Unverified (§3.1).
- **Norberg 1981** (Phil Trans B 292:359), bat wing and leg allometry with CIs: paywalled.
- **Per-bone bat metacarpal and phalanx lengths** (Castillo-Figueroa 2020): measured, but only published as size-scaled MDS loadings in supplement Table S1. No mm table.
- **Bat plagiopatagium insertion by species** (ankle vs toe base): only the *Myotis* examples in Morgan et al. 2019 Fig. 2 (S3, opened 2026-11-17). No species-by-species table of insertion points was found. Calcar length data (Stanchak et al. 2019) are paywalled.
- **Schutt et al. 1997 full text** (force-time traces, per-jump data): only the abstract was read.
- **Chatterjee & Templin 2004** tables (bone lengths of 10 genera; aerodynamic data): no copy that is clearly legitimate and open. Grok's numbers are not used.
- **Bennett 2001** (*Pteranodon* bone tables), **Kellner & Tomida 2000** (*Anhanguera*), **Wellnhofer 1975** (*Rhamphorhynchus*): not openly available. Cerqueira et al. 2026 Data S1 (raw lengths of 127 specimens) exists but I did not download it (dataset rule).
- **Henderson 2010** JVP tables: paywalled.
- **Explicit pterosaur launch speeds, times or forces** in a peer-reviewed table: none found. Witton & Habib 2010 cite unpublished data.
- **Wing loading upper limit expressed as a single published number for bats**: not found. The heaviest bats are about 1.5 kg (Unverified, Grok).

## 7. Sources disagree

1. **Castillo-Figueroa 2020 Table 1 internal errors.** *Artibeus lituratus* has LD3 equal to FL (LD3/FL = 1.00) but LD4/FL = 1.54, which is impossible because digit III is always longer than digit IV in bats. *Phylloderma stenops* (LD3/FL 1.26) and *Platyrrhinus lineatus* (LD4 = LD3, ratio 2.27) are also inconsistent with every other species. *Molossops temminckii* and *Rhinophylla alethina* list LD4 = LD5 exactly. *Eptesicus furinalis* LD3/FL = 1.36 is the lowest in the table. I excluded the first three from guild statistics. Also, the text says 97 species but Table 1 on PMC lists 85; the official PDF Table 1 (https://zoolstud.sinica.edu.tw/Journals/59/59-60.pdf) has all 97, and the fixed guild cells use it. Licence still unconfirmed: © Academia Sinica, no CC statement found, so treat as facts only.
2. **Membrane insertion in pterosaurs.** Elgin et al. 2011 and Witton 2008: ankle or lower leg in all pterosaurs. Chatterjee & Templin 2004 (via Grok, Unverified): knee/femur in pterodactyloids. Bennett 2001 (via Grok, Unverified): femur in *Pteranodon*. Padian & Rayner 1993: torso or hip (cited and rejected in S7).
3. **Witton 2008 *Pteranodon* figures as re-reported in Witton & Habib 2010.** Witton 2008 Table 4 gives the large *Pteranodon* morph b = 5.96 m, S = 1.990 m², AR 17.85, Q 180.82 N m⁻² (36.68 kg). Witton & Habib 2010 Table 1, citing Witton 2008, give b = 5.43 m, S = 1.60 m², AR 18.42, Q 224.79 N m⁻² for the same 36.68 kg, and Table 3 again uses 5.43 m. The *Quetzalcoatlus* rows match. The engine should use the original (Witton 2008).
4. **Witton 2008 Table 4 unit label.** The "Skeletal mass (kg)" column must be grams. For example, *Preondactylus* skeletal mass 5.43 with a body mass of 0.08 kg, and *Quetzalcoatlus* 18034.65 with a body mass of 259 kg. Witton & Habib 2010 Table 3 prints the same values as grams.
5. ***Quetzalcoatlus northropi* humerus length.** 544 mm (Witton & Habib 2010, Lawson 1975 holotype); 52 cm (Brower & Veinus 1981, also citing Lawson 1975a); 531 mm (Chatterjee & Templin 2004 Table 2, via Grok, Unverified).
6. **Vampire-bat jump force.** Schutt et al. 1997 abstract: peak up to 9.5 × body weight. Gardiner & Nudds 2011, citing Schutt: "typically 6.51 times body weight", later "over 6 times". Grok reports the Schutt mean peak force as 6.51 ± 1.57 N, which would be a force, not a multiple of weight (Unverified), so Gardiner & Nudds may have mislabelled newtons as body-weight multiples.
7. **Gardiner & Nudds 2011 text vs table.** The text says all five species produce 3.5–4.5 × body weight. Table 1 gives 3.37 (*M. capaccinii*) to 4.68 (*M. blythii*).
8. **Pteroid orientation.** Wilkinson et al. 2006: anteroventral in flight (broad propatagium, C_L,max 2.4). Palmer & Dyke 2010: medial (it would fail in bending otherwise). Both are Verified; they disagree.
9. **Pterosaur mass scaling coefficient.** Witton 2008: m = 0.551·b^2.516. Earlier studies: coefficients about 0.18–0.21 (eqs 11–13), so masses about 3× lower at the same span. Brower & Veinus 1981 Table 2 RMA (0.001951 g·cm^−2.491, which converts to ≈ 0.189 kg m^−2.491) and their Table 3 least squares (0.002253·b^2.459, ≈ 0.185) are both slightly different from the 0.1833·b^2.4823 that Witton attributes to "Brower & Veinus 1983".
10. **Giant masses, all Verified as reported.** *Q. northropi*: 70 kg (Chatterjee & Templin 2004), 74.5 kg (Brower & Veinus 1981), 200–250 kg (Witton & Habib 2010), 259 kg (Witton 2008), 276 kg (Sato et al. 2009 bird scaling), 544 kg (Henderson 2010, per S16). *Pteranodon*: 14.94 kg (Brower & Veinus), 16.6 kg (Chatterjee & Templin), 36.68 kg (Witton 2008), 93 kg (Sato et al.).
11. **Cerqueira et al. 2026 method.** The abstract and methods say standardized major axis (SMA), but one methods sentence says ordinary least squares (OLS).
12. **Witton & Habib 2010 range speed.** One passage gives an expected maximum-range speed "exceeding 30 m/s" (Pennycuick defaults, 200 kg). The Results give 48.3 m s⁻¹ under anaerobic power. These are different assumptions, not an error, but both appear in the same paper.
13. **Grok vs primary on dates and attributions** (Grok's corrections were confirmed): Schutt et al. is 1997, not 2007. Castillo-Figueroa is 2020 (Zool. Stud. 59:e60, published 20 Nov 2020), not 2021. The published pteroid numbers are Wilkinson et al. 2006, not the 2002 thesis. Grok attributed the "~12 kg" continuous-flapping limit to Pennycuick; the 12 kg I could verify is Sato et al.'s wandering-albatross figure.
14. **Cerqueira et al. 2026 discussion vs its own Table 1.** One discussion sentence swaps the femur and tibia allometry relative to Table 1 (flagged by Library Verify 2026-11-17). Use Table 1.

## 8. What the Grok pass added or contradicted

**Added, then confirmed in the primary source:**
- the open LMU copy of Witton 2008 (Table 4 and eqs 8–15 were then read from the page image);
- Sato et al. 2009 Table 1 with CIs;
- Wilkinson et al. 2006 pteroid joint angles;
- Palmer & Dyke 2010 pteroid mechanics and the M = 0.4526·b^2.4232 fit;
- Chatterjee et al. 2007 *Argentavis* Table 1;
- Gardiner & Nudds 2011 as the second bat-jump dataset (I then read its Tables 1–2 from the PMC images).

**Added but left Unverified:** Norberg & Rayner 1987 Table 4 coefficients; Chatterjee & Templin 2004 bone lengths; Henderson 2010 Table 2 masses; Altenbach 1979; Naish et al. 2021 *Anhanguera* humerus; Bennett 2001 femoral wing attachment; press "< 1 s" launch claims.

**2026-11-17 follow-up (no Grok):** the US bat field key (S3), which Grok had pointed to and which was blocked on 2026-11-16, opened today. Its Fig. 2 caption confirms the tarsus vs toe-base insertion, so that row is now Verified, and I added a calcar-keel row.

**Contradicted by Grok and confirmed by me:** the years for Schutt (1997) and Castillo-Figueroa (2020), and the Wilkinson citation (2006).

**Conflicts Grok surfaced, logged above:** items 2, 5, 6 and 8.

**Grok errors or limits:** Grok said it could not open the Elgin et al. 2011 PDF (it opened fine from app.pan.pl). Grok attributed the 12 kg limit to Pennycuick. Grok could not extract Witton 2008 Table 4 (I read it from the rendered page).

---

## 9. Sources

All read 2026-11-16, except S3 (2026-11-17).

**Link re-check, 2026-11-17 (curl, browser user agent):** HTTP 200 for app.pan.pl (S7), both LMU epub pages (S8, S17), the KU bitstream and the KU handle page https://kuscholarworks.ku.edu/handle/1808/3677 (S12), palaeo-electronica.org (S14), and every pmc.ncbi.nlm.nih.gov page (S1, S2, S3, S5, S9, S10, S11, S13, S15, S16, S18, S19). PubMed (S6) returned 203 with content. Three links fail from the box only, through bot-blocking rather than dead links: the europepmc.org pages (403), so the table column uses the PMC equivalents for S1, S2 and S13; royalsocietypublishing.org for S4 (403, also via doi.org); and the hdl.handle.net redirect for S12 (500), replaced by the KU handle page above. Paywalled DOIs cited for Unverified rows: Stanchak & Santana 2018, https://doi.org/10.1002/ar.23740 (actual title: "Assessment of the hindlimb membrane musculature of bats", Anat. Rec.); Unwin & Bakhurina 1994, https://doi.org/10.1038/371062a0; Chatterjee & Templin 2004, https://doi.org/10.1130/0-8137-2376-0.1 (GSA Special Paper 376). All three DOIs were confirmed on Crossref on 2026-11-17.

- **S1** Anthwal N, Urban DJ, Sadier A, et al. 2023. Insights into the formation and diversification of a novel chiropteran wing membrane from embryonic development. *BMC Biology* 21:101. doi:10.1186/s12915-023-01598-y. Europe PMC PMC10161559: https://europepmc.org/article/PMC/PMC10161559. **CC BY 4.0.**
- **S2** Argoitia MA, Teta P, Cassini GH. 2025. Wing morphology, foraging strategies, and flight performance in six sympatric species of molossid bats from Argentina. *Integrative Organismal Biology* 7:obaf044. doi:10.1093/iob/obaf044. https://europepmc.org/article/PMC/PMC12690267. **CC BY 4.0.**
- **S3** Morgan CN, Ammerman LK, Demere KD, Doty JB, Nakazawa YJ, Mauldin MR. 2019. Field identification key and guide for bats of the United States of America. *Occasional Papers, Museum of Texas Tech University* 360. PMID 31148880. https://pmc.ncbi.nlm.nih.gov/articles/PMC6537616/ (NIH author manuscript; opened 2026-11-17). Free to read on PMC; no open licence is stated. Some authors are CDC staff, but not all, so do not treat it as public domain. Facts only.
- **S4** Norberg UM, Rayner JMV. 1987. Ecological morphology and flight in bats. *Phil. Trans. R. Soc. B* 316:335–427. doi:10.1098/rstb.1987.0030. https://royalsocietypublishing.org/doi/10.1098/rstb.1987.0030 (abstract open; full text paywalled). © Royal Society. Abstract used; tables Unverified.
- **S5** Gardiner JD, Nudds RL. 2011. No apparent ecological trend to the flight-initiating jump performance of five bat species. *J. Exp. Biol.* 214:2182–2188. doi:10.1242/jeb.055871. https://pmc.ncbi.nlm.nih.gov/articles/PMC3110502/ (tables T1, T2 are images on PMC). © 2011 Company of Biologists; free to read on PMC; no open licence.
- **S6** Schutt WA Jr, Altenbach JS, Chang YH, et al. 1997. The dynamics of flight-initiating jumps in the common vampire bat *Desmodus rotundus*. *J. Exp. Biol.* 200:3003–3012. doi:10.1242/jeb.200.23.3003. PubMed abstract: https://pubmed.ncbi.nlm.nih.gov/9359889/. © Company of Biologists; abstract only.
- **S7** Elgin RA, Hone DWE, Frey E. 2011. The extent of the pterosaur flight membrane. *Acta Palaeontologica Polonica* 56(1):99–111. doi:10.4202/app.2009.0145. PDF: https://app.pan.pl/archive/published/app56/app20090145.pdf. Free to read; no licence statement in the PDF (APP's current policy is CC BY; not confirmed for this 2011 article).
- **S8** Witton MP. 2008. A new approach to determining pterosaur body mass and its implications for pterosaur flight. *Zitteliana* B28:143–158. https://epub.ub.uni-muenchen.de/12010/ (PDF zitteliana_2008_b28_08.pdf). © 2008 Bayerische Staatssammlung für Paläontologie und Geologie; free to read on the LMU open-access repository; no open licence.
- **S9** Wilkinson MT, Unwin DM, Ellington CP. 2006. High lift function of the pteroid bone and forewing of pterosaurs. *Proc. R. Soc. B* 273:119–126. doi:10.1098/rspb.2005.3278. https://pmc.ncbi.nlm.nih.gov/articles/PMC1560000/. © 2005 Royal Society; free to read on PMC.
- **S10** Palmer C, Dyke GJ. 2010. Biomechanics of the unique pterosaur pteroid. *Proc. R. Soc. B* 277:1121–1127. doi:10.1098/rspb.2009.1899. https://pmc.ncbi.nlm.nih.gov/articles/PMC2842769/. © 2009 Royal Society; free to read on PMC.
- **S11** Castillo-Figueroa D. 2020. Ecological morphology of Neotropical bat wing structures. *Zoological Studies* 59:e60. doi:10.6620/ZS.2020.59-60. https://pmc.ncbi.nlm.nih.gov/articles/PMC8181164/. Open access. The journal says copyright belongs to Zoological Studies; Wikipedia lists the journal licence as CC BY 4.0; the article page shows no licence. Treat as free to read with an unconfirmed licence.
- **S12** Brower JC, Veinus J. 1981. Allometry in pterosaurs. *University of Kansas Paleontological Contributions* Paper 105. http://hdl.handle.net/1808/3677 (KU ScholarWorks, marked "openAccess"; text bitstream via https://kuscholarworks.ku.edu/server/api/core/bitstreams/c8bb6ea1-8801-47e1-87d1-d253274a56ad/content). Free to read; no licence stated.
- **S13** Cerqueira GM, Da Silva JL, Werneburg I, Bronzati M, Aires ASS. 2026. Allometric patterns in *Rhamphorhynchus muensteri* and their role in the paleoecology, life habits, and taxonomic framework of pterosaurs. *Journal of Anatomy*. doi:10.1111/joa.70146. https://europepmc.org/article/PMC/PMC13645348. **CC BY 4.0.**
- **S14** Elgin RA, Hone DWE. 2020. A review of two large Jurassic pterodactyloid specimens from the Solnhofen of southern Germany. *Palaeontologia Electronica* 23(1):a13. doi:10.26879/741. Table: https://palaeo-electronica.org/content/2020/2979-solnhofen-pterodactyloids-table. **CC BY-NC-SA 4.0** (non-commercial: facts only, do not copy the table into a commercial product).
- **S15** Hone DWE, McDavid SN. 2025. A giant specimen of *Rhamphorhynchus muensteri* and comments on the ontogeny of rhamphorhynchines. *PeerJ* 13:e18587. doi:10.7717/peerj.18587. https://pmc.ncbi.nlm.nih.gov/articles/PMC11700493/. **CC BY 4.0.**
- **S16** Witton MP, Habib MB. 2010. On the size and flight diversity of giant pterosaurs, the use of birds as pterosaur analogues and comments on pterosaur flightlessness. *PLoS ONE* 5(11):e13982. doi:10.1371/journal.pone.0013982. https://pmc.ncbi.nlm.nih.gov/articles/PMC2981443/. **CC BY.**
- **S17** Habib MB. 2008. Comparative evidence for quadrupedal launch in pterosaurs. *Zitteliana* B28:159–166. https://epub.ub.uni-muenchen.de/12011/. © 2008 BSPG; free to read; no open licence.
- **S18** Sato K, Sakamoto KQ, Watanuki Y, et al. 2009. Scaling of soaring seabirds and implications for flight abilities of giant pterosaurs. *PLoS ONE* 4(4):e5400. doi:10.1371/journal.pone.0005400. https://pmc.ncbi.nlm.nih.gov/articles/PMC2670537/. **CC BY 4.0.**
- **S19** Chatterjee S, Templin RJ, Campbell KE Jr. 2007. The aerodynamics of *Argentavis*, the world's largest flying bird from the Miocene of Argentina. *PNAS* 104(30):12398–12403. doi:10.1073/pnas.0702040104. https://pmc.ncbi.nlm.nih.gov/articles/PMC1906724/. © 2007 National Academy of Sciences; PNAS open-access option (free to read).
