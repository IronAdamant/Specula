# Verification: C8-coat-patterns.md

Checked: 17 November 2026, Australia/Sydney (AEDT, UTC+11).
Source file: `/workspace/specula-research/C8-coat-patterns.md`
Output path (this file only): `/workspace/specula-research/C8-coat-patterns.md.verify.md`

Private adversarial source-check. No number was taken from Grok, a search snippet, or a pirate mirror. Every figure below was opened in a publisher page, PMC/Europe PMC full text (JATS XML), Crossref metadata, an official supplementary file, or a legal repository/preprint PDF (arXiv Pearson; Oxford ORA publisher PDF for Liu 2006). Key Pearson figure pages were also rendered and read as images.

## Summary

- **76 data rows** checked (sections 1–6: 17 + 12 + 8 + 11 + 14 + 14).
- **63 VERIFIED.** Almost every C8 “Verified” number that I could open matches the source, with those units and that population. One C8 “Unverified” row is upgraded here: row 10 (Pearson F–k map axes), after the official arXiv figure was viewed.
- **0 number FIXES.** One **licence clarification** (not a data-value fix): Frohnhöfer et al. 2013 is printed and deposited as **CC BY 3.0**; Crossref lists CC BY-NC-SA 3.0 only from a 11 September 2020 metadata update. See close check 1.
- **13 UNVERIFIABLE:** row 10’s upgrade is offset by four downgrades in this pass (row 47, Kondo & Asai 1995 abstract not obtained; row 40, Nakamasu 2009 full text not opened; rows 49–50, Allen 2011 Results not opened) plus the nine Grok-lead / unopened rows C8 already marked Unverified (61–62, 70–76). Row 51 (Allen abstract) was opened via Europe PMC and stays VERIFIED.
- **Rows skipped:** none of the 76.
- **Dead links:** no citation DOI used in C8 is a known dead identifier. Scripted HEAD/GET to many publisher landings (Development, PNAS, Royal Society, Science Advances, APS, Europe PMC PDF endpoints) returned 403 or 500; the same items still resolve as Crossref works or opened via Europe PMC XML / ORA / arXiv / PLOS. PMC2689028 and PMC3061134 fullTextXML returned HTTP 500 in this pass.
- **Worst problem:** the two close-check full texts C8 had only as search excerpts (Nakamasu 2009; Allen 2011 Results) could not be opened legally in this pass, so those numbers are left Unverified. No opened source contained a wrong number.

One-line summary: C8 is solid on every source that opened; upgrade Pearson’s F–k axes, treat Frohnhöfer as printed CC BY 3.0 (Crossref’s later NC-SA looks like a metadata mismatch), and leave Nakamasu, Allen’s Results table, and the Grok-lead books/thesis Unverified until those texts are opened.

## File snapshot

At the start of this check both C7 and C8 were copied to `/workspace/lv-scratch/c78/` (not `/tmp`). C8 snapshot: **39,308 bytes**, sha256 `9b7e94713b480c3c6f7421ceca1c7ea8f321f008b7877acb27783e7c2720aefa`, 216 lines (76 data rows). Creature Data’s reported mtime 17 November 2026, 10:42:41 AEDT is recorded but **mtime is not trusted** after the 09:59 AEDT restore; identity is by size and hash.

Immediately before this verify file was written (11:13:10 AEDT), `sha256sum` and `cmp` of the workspace file against that snapshot were identical. **No content change; no row re-check was required.**

## Close checks

### 1. Licences: Wilting 2015, Miyazawa 2010, Frohnhöfer 2013

**Wilting et al. 2015**, *Sci. Adv.* 1:e1400175, PMC4640610. Opened: Europe PMC fullTextXML. The article permissions block is CC BY-NC 4.0 (“not for commercial advantage”). C8’s ⚠NC (CC BY-NC 4.0) is **correct**.

**Miyazawa, Okamoto and Kondo 2010**, *Nat. Commun.* 1:66, PMC2982180. Opened: Europe PMC fullTextXML. The article states the Creative Commons Attribution-NonCommercial-Share Alike 3.0 Unported License (`creativecommons.org/licenses/by-nc-sa/3.0/`). C8’s ⚠NC (CC BY-NC-SA 3.0) is **correct**.

**Frohnhöfer, Krauss, Maischein and Nüsslein-Volhard 2013**, *Development* 140:2997–3007, doi `10.1242/dev.096719`, PMC3912879.

- The PMC/JATS permissions paragraph printed on the article is: “This is an Open Access article distributed under the terms of the Creative Commons Attribution License (http://creativecommons.org/licenses/by/3.0), which permits unrestricted use, distribution and reproduction in any medium provided that the original work is properly attributed.” © 2013 The Company of Biologists Ltd.
- Crossref’s current VOR licence field is `http://creativecommons.org/licenses/by-nc-sa/3.0`, with start date **2020-09-11** and delay-in-days **2615** (about seven years after the 15 July 2013 issue date).
- The official Development HTML at `https://journals.biologists.com/dev/article/140/14/2997/45781/...` opened (title, authors, abstract and body confirmed). The official PDF URL returned 403 to `curl`. Europe PMC’s search API also labels the record `cc by-nc-sa`, in line with Crossref rather than with the printed XML.
- **Settlement:** the licence printed on the paper, and deposited in the PMC full text, is **CC BY 3.0**. A CC BY 3.0 grant is irrevocable, so a later Crossref metadata row of BY-NC-SA 3.0 should not be treated as converting the article to non-commercial. C8’s conservative ⚠NC flag is understandable given the conflict, but it is stricter than the printed grant. Recommended library practice: record “CC BY 3.0 (printed / PMC); Crossref later lists CC BY-NC-SA 3.0 (from 11 Sep 2020)” and do not mark the row ⚠NC. This is a licence clarification, not a number fix. Staging sizes and stripe-order claims in rows 45–46 match the Methods/Introduction (AR 6.2, PB 7.2, PR 8.6, SP 9.6, J 11.0, J+ 13, J++ 16 mm SSL, after Parichy 2009; light interstripe at the horizontal myoseptum, then dark stripes added dorsally and ventrally; C8’s X0 / 1D / 1V names follow the paper).

### 2. Liu, Liaw and Maini 2006 — leopard and jaguar parameters

**Opened:** the official APS publisher PDF hosted in Oxford ORA, `https://ora.ox.ac.uk/objects/uuid:ca05b88a-0de3-4b0e-9c82-bde6776ad097/files/mc33323af1b9b022f65d2894a76d2b14e`, HTTP 200, 1,249,222 bytes, 8 pages, *Phys. Rev. E* 74, 011914 (2006), ©2006 The American Physical Society. ⚠ARR as C8 said; numbers only.

Equation (1) on p. 011914-1 is exactly C8 row 63: ∂u/∂t = Dδ∇²u + αu + v − r₂uv − αr₃uv²; ∂v/∂t = δ∇²v + γu + βv + r₂uv + αr₃uv², with γ = −α; D is the activator/inhibitor diffusion ratio and must be < 1; δ sets the length scale; quadratic terms favour spots, cubic favour stripes. Zero-flux boundaries; 200 × 200 grid; u, v started random in [0, 1].

Stage 1 (Eq. 3 / Fig. 3): D = 0.45, δ = 6, α = 0.899, β = −0.91, r₂ = 2, r₃ = 3.5; most-enhanced mode k = 0.27. All match row 64.

Leopard stage 2 (text and Fig. 4 caption): raise r₂ 2 → 7, then lower δ 6 → 3.8, then lower D 0.45 → 0.15. Matches row 65.

Jaguar stage 2 (text and Figs. 5, 7): raise r₂ 2 → 7 “and keep it fixed at that value for a longer time than in case of the leopard”, then lower δ to 1.8, then D 0.45 → 0.15. Other (α, β) pairs (0.77, −0.91) and (0.77, −0.99) are marked on Fig. 5. Power-spectrum peak “0.041 in the power spectrum” (spots) moves to “The most enhanced frequency is now 0.081” (rings). All match row 66. (Grok’s extract had been truncated before the final D; the ORA PDF has it.)

Fig. 1 caption: leopard spots (2 days), rings (8 weeks), rosettes (adult). Matches row 67. (Fig. 2’s jaguar ages — 5 weeks / 3 months / adult — are in the paper but not claimed in that row.)

### 3. Nakamasu 2009 and Allen 2011 — open the full text

**Nakamasu, Takahashi, Kanbe and Kondo 2009**, *PNAS* 106:8429–8434, doi `10.1073/pnas.0808622106`, PMC2689028. Europe PMC search confirms the identity (PMID 19433782; delayed-OA / not flagged Open Access; ⚠ARR as C8 said). In this pass, Europe PMC fullTextXML returned HTTP 500; Europe PMC, PNAS and PMC PDF URLs returned 403; WebFetch of the Europe PMC HTML and the pnas.org full text timed out. A copy already on the box from earlier today (`/workspace/c8work/PMC2689028.*`) is not a full text (XML 8,027 bytes, “PDF” 5,498 bytes). **Row 40 remains UNVERIFIABLE** in this pass. I did not use search excerpts to confirm the 10% / ≈15% / ≈20% / ≈10% melanophore figures.

**Allen, Cuthill, Scott-Samuel and Baddeley 2011**, *Proc. R. Soc. B* 278:1373–, doi `10.1098/rspb.2010.1734`, PMC3061134. Europe PMC search confirms the identity; Unpaywall lists a free Royal Society PDF, but that URL and the Europe PMC PDF also returned 403, and fullTextXML returned 500. The Europe PMC abstract was obtained and matches row 51 (patterning, complexity and irregularity related to habitat, tree-climbing and night activity; little phylogenetic signal). **Rows 49–50 (Results statistics: n = 35, λ, r², β, p) remain UNVERIFIABLE.** Row 51 stays VERIFIED (abstract).

### 4. Row 10 — Pearson Gray–Scott F vs k axes

**Opened:** official arXiv PDF `https://arxiv.org/pdf/patt-sol/9304003.pdf` (HTTP 200, 104,296 bytes, 16 pages; arXiv:patt-sol/9304003v1). Pages 11 and 13 were rendered at 150 dpi and read as images, and the text layer was extracted.

Figure 3 (the pattern map; ticks 0.00–0.08 vertical, 0.03–0.07 horizontal) is labelled **F on the vertical axis** and **k on the horizontal axis**. Figure 1 (the reaction-kinetics phase diagram) uses the same assignment, with F vertical (0–0.3) and k horizontal (0–0.08). C8’s inference was right. Rows 1–9 also match the PDF (Eq. 2; domain 2.5 × 2.5 periodic; D_u = 2×10⁻⁵, D_v = 10⁻⁵; 256 × 256, forward Euler, Δt = 1; initial U = 1, V = 0, central 20 × 20 set to U = 1/2, V = 1/4, ±1% noise, 200,000 steps; 10,000–20,000 steps to fill the grid, front speed on the order of 1×10⁻⁴; pattern ε at F = 0.02, k = 0.059, Liapunov exponent about 1.5×10⁻³, Liapunov time 660 steps ≈ one spot division; Hopf periodic orbit stable for k < 0.035 and unstable for k > 0.035; Turing patterns only near F = k = 0.0625 at ratio 2; “smallest ratio … about D_u/D_v = 2.8”; no patterns with equal D). **Row 10 is upgraded to VERIFIED.**

### 5. Row 35 — Ho 2019 cell densities

**Opened:** Europe PMC fullTextXML for Ho, Li, Yin, Paterson, Amblard, Gready, Yu and Headon 2019, *PLoS Biol.* 17:e3000132, PMC6383868, CC BY 4.0; and the official PLOS supplementary **S5 Data** (`pbio.3000132.s028.xlsx`, Fig. 5C, 5D, 5F, 5G).

The text is slightly awkward (“lower mesenchymal cell density than normal (at the lateral pattern margins 66 cells/100 μm versus 49 cells/100 μm after 24 hours in culture) (Fig 5C)”), but S5 Data settles the assignment. Fig 5C Overall means: **Control 66.11**, **Fc-chEDA1 (EDA-stimulated) 48.98** nuclei (C8’s 66 vs 49). Fig 5D Overall means: Control 50.96, **Ecto-D2 (EDA blocked) 97.87**, which is C8’s 98 cells/100 μm at 48 h. C8’s reading is correct. “Almost doubled” the width under ubiquitous EDA (Fig. 3D, E) is in the Results. Row-to-row packing: Discussion, “condensates in each new row are placed out of register with the previous row” and repeated iterations produce a hexagonal lattice — that is C8’s “staggered, near-hexagonal packing.” Rows 33–37 all stand. **Row 35 VERIFIED**, now from the data file as well as the wording.

### 6. Giraffe spots n = 258 and tiger stripes n = 114

**Lee, Cavener and Bond 2018**, *PeerJ* 6:e5690, PMC6173159, CC BY 4.0. Methods: “We assembled encounter histories for **258 calves** first observed as neonates for survival analysis.” Also “26 daytime surveys … between January 2012 and February 2016.” Table 1 matches rows 54–57 exactly (number 18.9, SD 7.5, CV 0.40, repeatability 0.78; area 0.04 ± 0.01, perimeter 0.99 ± 0.25, max caliper 0.29 ± 0.06 GU; circularity 0.51 ± 0.08, aspect ratio 1.69 ± 0.15, roundness 0.63 ± 0.04, solidity 0.84 ± 0.04, angle 87.96 ± 15.39°; circularity PO 0.52 ± 0.16, h² 1.04, P = 0.0037; solidity 0.53 ± 0.17, h² 1.06, P = 0.0041; number 0.20 ± 0.23, P = 0.39). Photo protocol (~150 m, perpendicular, 100–400 mm; edge-cut spots and speckles < 0.00001 GU² excluded) matches row 59. Survival wording and Table 3 top models ΔAICc 0 (number of spots, 1st season) and 0.44 (aspect ratio, 1st season), with beta CIs for number and aspect ratio excluding zero, match row 58. **n = 258 confirmed.**

**Wilting et al. 2015**, PMC4640610. Methods: “we analyzed photographs of **114 tigers**” (museum skins, camera-traps, or zoos of known origin). Table 1 / Fig. 4C: mean number of flank stripes “distinguishes 75% of tigris from 75% of sondaica” as **<23** (mainland) vs **>24** (Sunda); mainland “Thicker and less dense, especially on hindquarters”; Sunda “Thin and particularly dense on hindquarters.” Rows 68–69 match. ⚠NC as above. **n = 114 confirmed.**

### 7. Unverified Grok leads (Churcher, Heptner, Caro, Bard, Painter, Jung, Jiang)

These were **not opened** in this pass, as requested, except that the secondary quotation in row 61 (29.2 °C vs 32.5 °C) was confirmed as a sentence in Larison et al. 2015’s Discussion citing their ref. 39; the primary measurement was not read, so the row stays **UNVERIFIABLE**. Rows 62 and 70–76 stay **UNVERIFIABLE**.

## Row table

Status key: VERIFIED = opened source gives that number, units and population; FIX = opened source disagrees (none); UNVERIFIABLE = source not opened, or the claim is not in the part that opened.

| Row | Claim (short) | Status | Note | URL |
|---|---|---|---|---|
| 1 | Gray–Scott Eq. 2 | VERIFIED | arXiv p. 2, Eq. 2 | https://arxiv.org/pdf/patt-sol/9304003.pdf |
| 2 | Domain 2.5×2.5, periodic | VERIFIED | p. 3 | same |
| 3 | D_u=2e-5, D_v=1e-5 | VERIFIED | p. 3 | same |
| 4 | 256×256, Euler, Δt=1 | VERIFIED | p. 3 | same |
| 5 | IC and 200,000 steps | VERIFIED | p. 3 | same |
| 6 | 10k–20k steps; speed ~1e-4 | VERIFIED | p. 3–4 | same |
| 7 | ε: F=0.02, k=0.059; λ~1.5e-3; 660 steps | VERIFIED | p. 5–6; 1/0.0015=667, paper says 660 | same |
| 8 | Hopf stable k<0.035 | VERIFIED | p. 3 and Fig. 1 caption | same |
| 9 | Turing near F=k=0.0625; D_u/D_v≈2.8 | VERIFIED | p. 6 | same |
| 10 | F vertical 0–0.08, k horizontal 0.03–0.07 | VERIFIED | Upgrade. Fig. 3 image + text layer; same axes on Fig. 1 | same |
| 11 | Kondo–Miura saturating RD, Eqs. 1–4 | VERIFIED | Cooper 2018 Methods | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC6221541/fullTextXML |
| 12 | Denticle parameter set | VERIFIED | All 12 values; D_v/D_u=0.6/0.02=30 | same |
| 13 | L=75, T=1500, no-flux; Δx≈0.58, Δt≈0.07 (≈0.04 when D_v changed) | VERIFIED | 75/128=0.5859; Δt=Δx²/(8×0.6)=0.0715. Fig. 5 H/I uses updated Δt≈0.04 (paper Methods says D_v “reduced”; Results/Fig. 5 increase D_v, which also shrinks Δt) | same |
| 14 | 6 spots, R=4.5, (iL/5, L/2); u0=5, v0=0 | VERIFIED | Eqs. 5–6 and “n_spot=6 … (iL/5, L/2)” | same |
| 15 | Lower d_v and G_max, raise D_v → sparser skate-like | VERIFIED | Fig. 5 legend and Results (direction; S1 values not read, as C8 said) | same |
| 16 | Labyrinths between spots and inverse spots | VERIFIED | Miyazawa Results/Fig. 1 (“labyrinthine”; C8’s “maze” is a paraphrase) | PMC2982180 XML |
| 17 | Felid two-stage model (see 63–66) | VERIFIED | Liu abstract + ORA PDF | ORA PDF |
| 18 | Cut diffusion >80% at scale borders in 2D | VERIFIED | Fofonjka “reduction of all diffusion coefficients by a factor >80% along the scale boundaries” | PMC8065134 XML |
| 19 | 3D geometry alone; Gray–Scott, Schnakenberg, 3-component | VERIFIED | Results / Fig. 2 | same |
| 20 | Green (0.5, 0.25), black (0, 1), ±1% noise | VERIFIED | Methods | same |
| 21 | 2D 10–20× faster (2 vs 30 min); wrong colour 6.4%/1.5%; Δcolour 7.0%/2.2% | VERIFIED | XML: “10 to 20 times faster … typically, 2 min instead of 30 min”; Fick–Jacobs vs Bradley | same |
| 22 | Length scale −27%, curvature +40%; 20 random runs ≈25% and 34% | VERIFIED | Results / Fig. 5 (“about 25%” and “34%”) | same |
| 23 | ≈30% black scales at 18 weeks | VERIFIED | Fig. 5 / Results | same |
| 24 | Motifs 2–3 then 1–2 scales wide | VERIFIED | Fig. 5 (B, C) caption | same |
| 25 | ~1×0.5 cm = 8×4 scales (≈1.25 mm, derived) | VERIFIED | Methods: “about 1 × 0.5 cm (8 by 4 scales)”. 1 cm/8 = 0.5 cm/4 = 1.25 mm | same |
| 26 | Ages 18…106 weeks; ocelli traces at 106, gone later | VERIFIED | Fig. 4 legend | same |
| 27 | Shark timeline: st. 31 ~70 dpf none; st. 32 ~80 dpf two rows; later st. 32 ~100 dpf body rows | VERIFIED | Fig. 1 (~80 dpf); Fig. S2 (~70 dpf); Methods also calls st. 31 “~75 dpf” (beading) and st. 32 “~100 dpf” (transfer). C8 follows the figure timeline | PMC6221541 XML |
| 28 | FGF gap 6/8 at 50 dpt (0/7 controls), 7/8 at 75 dpt | VERIFIED | 50 dpt: 6/8 treated, “no gap” in DMSO n=7/7 (C8’s 0/7 controls). 75 dpt: 7/8 treated; DMSO 9/9 (C8 did not quote the 75-day control n). Table 1: 125 dpf 6/8 vs 7/7; 150 dpf 7/8 vs 9/9 | same |
| 29 | Reptile placodes, except croc face and jaw; tracts like feathers | VERIFIED | Di-Poï & Milinkovitch 2016; several tracts argued homologous to chicken | PMC5392058 XML |
| 30 | 14 feather tracts | VERIFIED | Mou et al. 2011 Introduction | PMC3057954 XML |
| 31 | Macropattern starts E7 | VERIFIED | same | same |
| 32 | Macro- vs micropattern (apteria) | VERIFIED | same | same |
| 33 | First row E6.5, then parallel wave | VERIFIED | Ho 2019 | PMC6383868 XML |
| 34 | Culture timing 10–25 h, ~25–30 h, next ~70 h | VERIFIED | Results (Fig. 4) | same |
| 35 | 66 vs 49 (24 h); 98 (EDA blocked, 48 h) | VERIFIED | S5 Data: Control 66.11 vs Fc-chEDA1 48.98; Ecto-D2 97.87 | same + PLOS S5 xlsx |
| 36 | Patterned width “almost doubled” with EDA | VERIFIED | Results, Fig. 3D, E | PMC6383868 XML |
| 37 | New row out of line; near-hexagonal | VERIFIED | Discussion: “out of register with the previous row” → hexagonal lattice | same |
| 38 | Adult zebrafish stripe ≈600 μm (quoted) | VERIFIED | Volkening quoting ref. 38 | PMC6089994 XML |
| 39 | Long-range reach ≳ half a stripe (≳300 μm, derived) | VERIFIED | “at least half a stripe width 19 (adult stripe width is ≈600 μm 38)” | same |
| 40 | Nakamasu ablation percentages | UNVERIFIABLE | Full text not opened (PMC XML 500; PDFs 403; HTML timed out) | doi 10.1073/pnas.0808622106 |
| 41 | Stage table PB 7.5/21 … J+ 13.0/70 | VERIFIED | Volkening Table 1, exact | PMC6089994 XML |
| 42 | Growth 38 μm/day length, 27 μm/day height | VERIFIED | Methods | same |
| 43 | Melanophore death p=0.0333 / day | VERIFIED | Methods | same |
| 44 | 21 to ≈75 dpf; 100 runs with SD bars | VERIFIED | “averaged across 100 simulations with error bars denoting standard deviation” | same |
| 45 | Frohnhöfer stages AR 6.2 … J++ 16 mm SSL | VERIFIED | Methods, after Parichy 2009 | PMC3912879 XML |
| 46 | Stripe order X0 then 1D/1V, then more | VERIFIED | Introduction / figure notes | same |
| 47 | Angelfish stripes keep spacing as fish grows | UNVERIFIABLE | C8 used the Nature abstract; that abstract was not re-obtained in this pass | doi 10.1038/376765a0 |
| 48 | Hybrid salmonid counts | VERIFIED | 2 crosses × 30 light spots; 4 crosses 11–30 dark spots; 8 crosses 12–30 all labyrinthine | PMC2982180 XML |
| 49 | Melanism vs habitat (n=35, λ, r², β, p) | UNVERIFIABLE | Allen Results full text not opened | doi 10.1098/rspb.2010.1734 |
| 50 | Melanism vs activity/locomotion r², β, p | UNVERIFIABLE | same | same |
| 51 | Pattern vs ecology; little phylogenetic signal | VERIFIED | Europe PMC abstract | same (abstract) |
| 52 | Cat prepattern st. 13 (≈E11) → st. 16 (≈E15), +16 days | VERIFIED | “uniform monolayer” (C8: “single-layer”); thick/thin before hair follicles | PMC8423757 XML |
| 53 | Dkk4 domains broader and earlier than follicle spacing | VERIFIED | Discussion / Supp. Fig. 3: “much broader and appear earlier than those associated with hair follicle placode spacing” | same |
| 54 | Giraffe spot count 18.9, SD 7.5, CV 0.40, R=0.78 | VERIFIED | Lee Table 1 | PMC6173159 XML |
| 55 | Spot size 0.04±0.01, 0.99±0.25, 0.29±0.06 GU | VERIFIED | Table 1 | same |
| 56 | Shape: 0.51, 1.69, 0.63, 0.84, 87.96° | VERIFIED | Table 1, all ± as claimed | same |
| 57 | Heritable circularity and solidity; size traits n.s. | VERIFIED | Table 1 PO slopes, h², P exact | same |
| 58 | Survival: larger/rounder spots; ΔAICc 0 and 0.44; n=258 | VERIFIED | 258 calves; Table 3; beta CIs for number and aspect ratio exclude zero | same |
| 59 | Photo protocol 150 m, 100–400 mm; exclude <0.00001 GU² | VERIFIED | Methods | same |
| 60 | Environment explains 30–63% of 12/18 zebra traits | VERIFIED | Larison Table 1: torso number 63%, r² 0.42, p=0.04; hind-leg definition 63%; foreleg definition 60%; hind-leg thickness 51% | PMC4448797 XML |
| 61 | Zebra skin 29.2 vs 32.5 °C | UNVERIFIABLE | Sentence is in Larison’s Discussion citing [39]; primary not read (C8 already Unverified) | same |
| 62 | Murray’s body-size / tail-taper rule | UNVERIFIABLE | Not read, as C8 said | doi 10.1016/0022-5193(81)90334-9 |
| 63 | Liu model Eq. 1 | VERIFIED | ORA PDF p. 011914-1 | ORA PDF |
| 64 | Stage 1 parameters; k=0.27; 200×200; [0,1] | VERIFIED | Eq. 3, Fig. 3 | same |
| 65 | Leopard stage 2: r₂ 2→7, δ 6→3.8, D 0.45→0.15 | VERIFIED | Figs. 4, 6 | same |
| 66 | Jaguar: r₂ held longer, δ→1.8, D→0.15; 0.041→0.081; other (α,β) | VERIFIED | Figs. 5, 7, 9–10 and text | same |
| 67 | Leopard 2 days / 8 weeks / adult | VERIFIED | Fig. 1 caption | same |
| 68 | Tiger flanks <23 vs >24; n=114; 75% cut | VERIFIED | Methods + Table 1 / Fig. 4C | PMC4640610 XML |
| 69 | Mainland thicker/less dense; Sunda thin/dense on hindquarters | VERIFIED | Table 1 | same |
| 70 | Heptner leopard/cheetah sizes | UNVERIFIABLE | Book scan not opened | archive.org |
| 71 | Churcher Grévy’s stripe widths | UNVERIFIABLE | Not opened | doi 10.2307/3504222 |
| 72 | Caro / Egri regional stripe widths | UNVERIFIABLE | Figure not opened | doi 10.1038/ncomms4535 |
| 73 | Bard embryonic spacing ~0.4 mm | UNVERIFIABLE | Abstract not obtained | doi 10.1111/j.1469-7998.1977.tb04204.x |
| 74 | Painter angelfish 3→6→12 stripes | UNVERIFIABLE | Full text not opened | doi 10.1073/pnas.96.10.5549 |
| 75 | Jung 1998 thesis spacings | UNVERIFIABLE | Thesis not opened (not peer-reviewed) | UCL Discovery |
| 76 | Jiang et al. 1999 primordium 200–300 μm | UNVERIFIABLE | Not opened | doi 10.1242/dev.126.22.4997 |

## Source table

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| Pearson 1993 arXiv | https://arxiv.org/pdf/patt-sol/9304003.pdf | Yes, HTTP 200, official PDF | ⚠ARR for the Science VoR is fair; this check used the arXiv preprint (arXiv non-exclusive deposit licence), as C8 did |
| Cooper et al. 2018 | PMC6221541 XML | Yes | CC BY 4.0, correct |
| Miyazawa et al. 2010 | PMC2982180 XML | Yes | ⚠NC, CC BY-NC-SA 3.0, correct |
| Liu et al. 2006 | ORA publisher PDF (doi 10.1103/physreve.74.011914) | Yes, HTTP 200, 1,249,222 bytes | ⚠ARR (©2006 APS), correct; numbers only |
| Fofonjka & Milinkovitch 2021 | PMC8065134 XML | Yes | CC BY 4.0, correct |
| Di-Poï & Milinkovitch 2016 | PMC5392058 XML | Yes | CC BY 4.0, correct (C8 “per PMC XML”) |
| Mou et al. 2011 | PMC3057954 XML | Yes | CC BY 4.0, correct |
| Ho et al. 2019 | PMC6383868 XML + PLOS S5 xlsx | Yes | CC BY 4.0, correct |
| Volkening & Sandstede 2018 | PMC6089994 XML | Yes | CC BY 4.0, correct |
| Nakamasu et al. 2009 | doi 10.1073/pnas.0808622106 / PMC2689028 | DOI identity confirmed; full text not opened (XML 500, PDFs 403) | ⚠ARR (PNAS) is the right default; not re-checked on the publisher page |
| Frohnhöfer et al. 2013 | PMC3912879 XML; Development HTML; Crossref | Yes (HTML and XML; official PDF 403) | Printed/PMC: CC BY 3.0. Crossref from 2020-09-11: CC BY-NC-SA 3.0. C8’s ⚠NC is conservative; printed grant is CC BY 3.0 |
| Kondo & Asai 1995 | doi 10.1038/376765a0 | Identity not re-opened | ⚠ARR (Nature) not re-checked |
| Allen et al. 2011 | doi 10.1098/rspb.2010.1734 / PMC3061134 | Abstract via Europe PMC; full text not opened | ⚠ARR (Royal Society) is the right default; publisher PDF 403 |
| Kaelin, McGowan & Barsh 2021 | PMC8423757 XML | Yes | CC BY 4.0, correct |
| Lee, Cavener & Bond 2018 | PMC6173159 XML | Yes | CC BY 4.0, correct |
| Larison et al. 2015 | PMC4448797 XML | Yes | CC BY 4.0, correct |
| Wilting et al. 2015 | PMC4640610 XML | Yes | ⚠NC, CC BY-NC 4.0, correct |
| Murray 1981; Heptner; Churcher; Caro; Bard; Painter; Jung; Jiang | as cited | Not opened | Left as C8 listed (mostly ⚠ARR or thesis) |

## Grok section

A single Grok CLI live pass covering **both C7 and C8** was started at **10:46:04 AEDT** and exited 0 at **11:01:54 AEDT** (~16 minutes, inside the ~25-minute cap). Command: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve` with `--prompt-file /workspace/lv-scratch/c78/grok/prompt.txt`. Stdout: `/workspace/lv-scratch/c78/grok/out.txt` (38,635 bytes). C7’s verify file was written while this pass was still running; this C8 file is written after it finished.

**No number in this verify file is taken from Grok.** Where Grok’s notes overlap a close check (Pearson F vertical / k horizontal; Ho 66 vs 49 vs 98; Lee n=258; Wilting n=114 and CC BY-NC; Liu two-stage parameters; Miyazawa labyrinths; Frohnhöfer staging), the claims were independently confirmed from the official texts listed above. Grok did not open Nakamasu 2009 or Allen 2011 full texts (reCAPTCHA / blocked), and neither did this pass. Grok also did not open the B7 Grok-lead list; those rows stay Unverified.

## Arithmetic re-derived

- Pearson Liapunov time: paper states 660 steps as the inverse of ~1.5×10⁻³ (1/0.0015 ≈ 666.7; the paper’s 660 is its own rounding).
- Cooper Δx = 75/128 ≈ 0.5859 (C8 ≈ 0.58); Δt = Δx²/(8×0.6) ≈ 0.0715 (C8 ≈ 0.07); D_v/D_u = 0.6/0.02 = 30.
- Fofonjka scale size: 10 mm / 8 scales = 5 mm / 4 scales = 1.25 mm.
- Volkening half-stripe: 600 μm / 2 = 300 μm.
- Ho S5 means: Control 66.113… ≈ 66; Fc-chEDA1 48.979… ≈ 49; Ecto-D2 97.869… ≈ 98.
