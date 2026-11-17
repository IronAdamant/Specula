# E1: tree form as parameters, by species

Item E1 of `LIBRARY_RESEARCH_BRIEF.md`. Compiled 2026-11-16 (Australia/Sydney). Private working file: nothing here was posted, published or committed.
## Pass 2 changes (2026-11-16, Australia/Sydney)

Second pass with WebSearch, WebFetch and curl/pdftotext only. The Grok CLI is still signed out and was not tried. Rows were added and ⚠ flags resolved; the structure was not changed (one new subsection, §4.7, holds the crown-ratio table). New sources S18–S34 are in §1 and §7, all accessed 2026-11-16.

Library Verify fixes (from `E1-tree-form-parameters.md.verify.md`):
- V1. CA black oak render set (§4.1): "1DownAngle 30 ±30" corrected to 1DownAngle 30, 1DownAngleV −30 (S1 Appendix p.126). The same notation was fixed in the aspen (60, −50) and tupelo (60, −40) rows.
- V2. Norway spruce angle (§4.2): dropped "seed-source range ≈ 83–91°". The 91° value is now labelled as one seed source (Moldoviţa) at one site.
- V3. Okabe 2015 does not use the word "folklore". Reworded in §2.3, the oak and beech phyllotaxis rows, and §5, to the paper's words: the list "is often presented" and "the phyllotaxis fraction is not a determined trait of each species".
- V4. *Q. alba* b1 = 1.5212: no clean copy was found, so the ⚠ stays (§6 item 6).
- V5. S6 Pretzsch link: the Stellenbosch PDF now returns HTTP 500. Replaced with the Universidad de Chile repository record (200) plus the DOI. The numbers are unchanged.

Research added:
- Branch ratios per order from field data: Lau et al. 2019 (S18: manual vs TreeQSM α, β, θ on 484 branches) and derived per-bifurcation ratios. Tredennick et al. 2013 (S20, savanna length scaling) and Sperry et al. 2012 (S21, model defaults) also added. All in §2.1. Sone et al. 2009 abstract (S34) was read, and §6 item 10 is now partly resolved.
- Measured branch angles by species: Bekkers et al. 2025 TLS (S19) for Scots pine (§4.2) and 6 Guyanese rainforest species (§4.5).
- Tropical and savanna species (§4.5): *Ceiba pentandra* and *Dipteryx oleifera* height and crown-area models (S22); *Acacia tortilis* and *A. raddiana* height and crown size in the Arava, Israel (S23); baobab, qualitative only (S32).
- Crown ratio by species group: USFS FIA northern US, 27 groups (S24), in new §4.7.
- Phyllotaxis from floras (§2.3): FNA (S25) and VicFlora (S33) for *Acer*, Fagaceae (*Quercus*, *Fagus*), *Betula*, Salicaceae, *Picea*, *Pinus sylvestris*, *Olea* and *Eucalyptus globulus*. Coconut leaf spirals (S30).
- Primary sources opened:
  - Bechtold 2004 (S26: western US crown width, §4.2).
  - Feldpausch et al. 2011 (S27: regional tropical H–D, §3).
  - Saguaro: Drezner 2003 abstract (S29) and the NPS Organ Pipe brochure (S28). This resolves §6 item 1.
  - Jenkins et al. 2003 (S31): opened, but it gives biomass only, so it is not used for form.
- ⚠ still open: *Q. alba* digit (S7); Chave 2014 page of Eq. 6 (Crossref confirms the article runs pp.3177–3190); Mandelbrot 1983 and Aratsu 1998 not opened; Bentley 2013 numbers (paywalled; the ESA copy times out); beech 30.7° vs 54.1°. New ⚠: S24 group labels were assigned by row order (see §4.7).

## 0. Read this first

- **Every number has a source key** (S1, S2, …) that points to a full entry in §7, with a URL and the access date 2026-11-16. Where a value comes from one table or equation, that is named in the row.
- **Measured or chosen.** Each row says whether the value is a *field measurement / fitted regression* ("meas"), a *model or rendering parameter someone chose* ("param"), or a *descriptive range from a reference work* ("desc"). Weber & Penn (S1), ABOP (S2), Runions (S3) and the Mäkelä model (S12) are **param**. They are chosen values, not measurements.
- **Verification.** "✔" means I read the number in the primary source text myself (PDF via `pdftotext`, or the publisher/agency page). "⚠" means I could not confirm it in the primary source, so treat it as unverified. Every ⚠ item is also listed in §6.
- **Method note:** the Grok CLI pass the brief asked for (`grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve --prompt-file …`) **did not run**. The CLI exited at once with "Not signed in" (it needs `grok login --device-code`, which only the owner can do). So all research here was done with WebSearch, WebFetch and curl/pdftotext against the primary sources. No Grok output is used.
- Scratch copies of the sources are in `/workspace/scratch-e1/`, outside any repo. No datasets were downloaded into the engine.

## 1. Sources and licences (summary)

| Key | Source | What it gives | Licence, in plain words |
|---|---|---|---|
| S1 | Weber & Penn 1995, SIGGRAPH | render parameters for 4 trees (Appendix, p.126) | Copyrighted (ACM). Numbers cited as facts. **These are the authors' chosen render parameters, designed from photographs, not measurements.** |
| S2 | Prusinkiewicz & Lindenmayer 1990, *The Algorithmic Beauty of Plants* (ABOP) | Honda / Aono–Kunii model constants (Tables 2.1–2.3, pp.57–61); 137.5° (p.99) | Copyrighted book (Springer-Verlag). The authors post a free PDF. Parameter values cited as facts; no wording or figures copied. |
| S3 | Runions, Lane & Prusinkiewicz 2007, space colonization | algorithm parameters (pipe exponent n, kill distance, radius of influence) | Copyrighted (Eurographics). Free author PDF. Values cited as facts. |
| S4 | Minamino & Tateno 2014, PLoS ONE | Leonardo / pipe-model field test, branch angles, *Fagus crenata*, *Abies homolepis* | **CC BY 4.0** (open). |
| S5 | Eloy 2011, Phys. Rev. Lett. | Leonardo exponent range from the literature, and the model's prediction | Copyrighted (APS). Author PDF and arXiv (arXiv non-exclusive licence). Numbers cited as facts. |
| S6 | Pretzsch et al. 2015, Urban For. Urban Green. | crown radius–DBH and height–DBH allometry for 22 species | **CC BY-NC-ND 4.0** (open, but no commercial use and no derivatives *of the article*; the regression coefficients are facts). |
| S7 | Bechtold 2003, South. J. Appl. For. (USDA Forest Service) | crown width–DBH equations, 87 eastern US species | Author is a US Forest Service employee (US government work). The journal reprint is stamped "Not for further reproduction": cite the numbers, don't redistribute the PDF. |
| S8 | Chave et al. 2014, Global Change Biology | pantropical height–DBH model | Copyrighted (Wiley). Author-hosted PDF. Numbers cited as facts. |
| S9 | Loubota Panzou et al. 2021, Global Ecol. Biogeogr. | tropical crown allometry exponents, forest vs savanna | **CC BY 4.0**. |
| S10 | Jucker et al. 2017, Global Change Biology | global DBH from height × crown diameter | **CC BY 4.0** (stated in the article). |
| S11 | Budeanu, Şofletea & Pârnuţă 2012, Not. Bot. Horti Agrobo. | Norway spruce dominant-branch insertion angle | Open-access journal; licence not stated in the text I read. Treat as copyrighted, numbers cited as facts. |
| S12 | Mäkelä, Vanninen & Ikonen 1997, Silva Fennica 31(3):369–380 | Scots pine branch-angle **model input** | Open access (journal archive). Licence for this article not confirmed on its page. |
| S13 | Okabe 2015, Scientific Reports 5:15358 | phyllotaxis divergence angles, fraction series, species examples | **CC BY 4.0**. |
| S14 | USDA Forest Service FEIS species reviews (several authors, 1991–2006) | descriptive heights, crown shapes, branching (Joshua tree, black spruce, etc.) | **US government work, public domain** (some photos on those pages are credited ©, and none are used here). |
| S15 | US National Park Service, Saguaro NP web pages (2024) | saguaro arm-onset height and age | **US government work, public domain**. |
| S16 | Bentley et al. 2013, Ecology Letters 16:1069–1078 | qualitative only (abstract): branching is area-preserving | Copyrighted (Wiley). Only the abstract was read. |
| S17 | Brunori et al. 2017, via review in PMC12753925 | olive *biomass* equation (used only to correct a wrong claim) | Review is open (PMC). Secondary citation. |
| S18 | Lau et al. 2019, For. Ecol. Manage. 439:132–145 | measured α (radius), β (length), θ scaling exponents; manual vs TreeQSM; Guyana | Post-print **CC BY-NC-ND** (no commercial use, no derivatives of the article; the numbers are facts). |
| S19 | Bekkers, Evers, Lau & Buckley 2025, AoB PLANTS 17(2) plae071 | TLS-derived 1st/2nd-order branch angles by species | **CC BY 4.0**. |
| S20 | Tredennick, Bentley & Hanan 2013, PLoS ONE 8(3) e58241 | model exponent predictions; savanna length-scaling result (qualitative) | **CC BY**. |
| S21 | Sperry et al. 2012, Funct. Ecol. (author PDF) | default branch length and radius ratios used in a model | Copyrighted (Wiley/BES). Author PDF. Numbers cited as facts; **param**. |
| S22 | Martínez Cano et al. 2019, Biogeosciences 16:847–862, + supplement | species H–D (gMM) and crown-area–D models, 162 Panama species | **CC BY 4.0**. |
| S23 | Winters et al. 2025, Open J. Ecol. 15(2):194–204 | *Acacia tortilis* / *raddiana* height and crown size | **CC BY 4.0**. |
| S24 | Westfall, Westfall & Randolph 2019, Forest Science 65(5):593–601 (USFS treesearch 61347) | FIA uncompacted / compacted crown ratio by species group | **US government work, public domain** (stated on the article). |
| S25 | Flora of North America (FNA) online treatments | leaf / branch arrangement by genus or family | © Flora of North America Association. Facts only; no wording or figures copied beyond short quoted terms. |
| S26 | Bechtold 2004, West. J. Appl. For. 19(4):245–251 | crown width–DBH, 53 western US species | US Forest Service author (US government work); journal reprint. Cite numbers, do not redistribute the PDF. |
| S27 | Feldpausch et al. 2011, Biogeosciences 8:1081–1106 | regional tropical ln H–ln D models | **CC BY 3.0**. |
| S28 | US NPS, Organ Pipe Cactus NM, saguaro brochure (2016 PDF) | saguaro height–age and first-arm height/age at Organ Pipe | **US government work, public domain**. |
| S29 | Drezner 2003, Am. J. Bot. 90(6):911–914 (abstract) | saguaro site growth factors | Copyrighted (BSA/Wiley). Abstract only. |
| S30 | Davis 1971, Fibonacci Quarterly 9(3):237–244 | number of leaf spirals in palms incl. coconut | Copyrighted (Fibonacci Association). Facts only. |
| S31 | Jenkins et al. 2003, Forest Science 49(1):12–35 | biomass equations only (opened to resolve a flag) | US government work. Not used for form. |
| S32 | Assogbadjo et al. 2006, Ann. Bot. 97(5):819–830 (abstract) | baobab morphometrics across Benin climate zones (qualitative) | Copyrighted (OUP); free on PMC, but only the abstract could be read (full text 403 / HTML to curl). |
| S33 | VicFlora (Royal Botanic Gardens Victoria) | *Olea* and *Eucalyptus globulus* leaf arrangement | Text **CC BY 4.0**; most images CC BY-NC-SA 4.0 (per VicFlora copyright page; text only used here). |
| S34 | Sone et al. 2009, J. Plant Res. 122:41–52 (abstract) | Leonardo-rule field test, *Acer rufinerve* | Copyrighted (Springer / BSJ). Abstract only. |

## 2. Cross-species rules

### 2.1 Pipe model / Leonardo (da Vinci) rule: d^Δ = Σ d_i^Δ

| Quantity | Value | Spread / range | What, where, n | Kind | Source | ✔/⚠ |
|---|---|---|---|---|---|---|
| Leonardo exponent Δ, literature range for "a large number of species" | — | 1.8 < Δ < 2.3 | review statement citing Mandelbrot 1983, Aratsu 1998, Sone et al. 2009 | meas (secondary) | S5 p.1 (eq. 1 text) | ✔ in S5; primary studies ⚠ not opened |
| Δ predicted by the wind-stress self-similar model | — | 1.93 < Δ < 2.21 | for fractal dimension 2 < D < 3 and Weibull modulus m = 10 | model | S5 (text after eq. for Δ) | ✔ |
| Weibull modulus of wood used in that model | — | typically 5 < m < 20 | wood | lit. value | S5 | ✔ |
| Fractal-model ratios: l_k/l_{k+1} = N^(1/D), d_k/d_{k+1} = N^(1/Δ) | formula | — | N = daughters per node | model | S5 eq. 2 | ✔ |
| Uniform-stress index Σ(d_i³ cos θ_i)/d_M³, *Fagus crenata* | 0.97 | ± 0.02 SE | 34 branching points (all with 2 daughters) on 13 lower branches of 6 trees about 30 yr old; Nikko, Japan | meas | S4 Results, Fig. 7A | ✔ |
| Same index, *Abies homolepis* | 0.88 | ± 0.02 SE | 79 branching points (39 with 2 daughters, 37 with 3, 3 with ≥4) on 13 branches of 7 trees | meas | S4 Results, Fig. 7B | ✔ |
| Elastic-similarity index, *F. crenata* | 0.88 | ± 0.03 SE | as above | meas | S4 Results | ✔ |
| Daughters per branching point | 2 (*F. crenata*); 2–5 (*A. homolepis*) | — | as above | meas | S4 Methods | ✔ |
| Branching angle of lateral daughters | — | 10–50° (*F. crenata*); 50–80° (*A. homolepis*) | as above | meas | S4 Discussion | ✔ |
| Branch cross-section, vertical ÷ horizontal diameter | 1.065 (*F. crenata*); 1.034 (*A. homolepis*) | ± 0.007; ± 0.003 SE | as above | meas | S4 Methods | ✔ |
| Mother-branch diameter range measured | — | 8.3–56.0 mm (*F. c.*); 6.0–49.1 mm (*A. h.*) | as above | meas | S4 Methods | ✔ |
| Sample trees: height | 14.7 m (*F. c.*); 13.8 m (*A. h.*) | ±1.3 SD, range 14.1–16.9; ±5.5 SD, range 9.4–22.6 | as above | meas | S4 Methods | ✔ |
| Sample trees: DBH | 26.4 cm (*F. c.*); 31.6 cm (*A. h.*) | ±1.5 SD; ±13.1 SD | as above | meas | S4 Methods | ✔ |
| Width decrease per branching when two equal daughters keep area (Δ = 2): w2/w1 = 1/√2 | 0.707 | — | Honda-type L-system | param | S2 p.57 | ✔ |
| Pipe exponent n in r^n = r1^n + r2^n used by space colonization | "usually between 2 and 3" | — | algorithm parameter (they cite Macdonald 1983, pp.131–135) | param | S3 §2, eq. 1 | ✔ |
| Area preservation across real trees, 5 species | "branching is area preserving" (no number read) | — | 5 functionally divergent species | meas | S16 abstract | ✔ abstract only; exponent values ⚠ not read |
| α_branch (radius scaling; r ratio = n^−α), manual measurements | 0.63 | 95% CI 0.62–0.65 (pseudo-median) | 484 branches > 10 cm diameter from 9 felled tropical trees, Guyana; DBH 61.3–97 cm; height 18.8–29.9 m | meas | S18 Table 3 | ✔ |
| α_branch, TreeQSM from TLS, same branches | 0.45 | CI 0.43–0.48 | as above | meas (TLS) | S18 Table 3 | ✔ |
| β_branch (length scaling; l ratio = n^−β), manual / TreeQSM | 0.07 / 0.42 | CI −0.06–0.2 / 0.31–0.54 | as above | meas | S18 Table 3 | ✔ |
| θ_branch (= 1/(2α+β), metabolic scaling), manual / TreeQSM | 0.50 / 0.59 | CI 0.4–0.56 / 0.53–0.65 | as above; WBE ideal α = ½, β = ⅓, θ = 0.69 | meas | S18 Table 3 | ✔ |
| Tree-level α, β, θ (TreeQSM / manual) | α 0.46 / 0.64; β 0.41 / 0.05; θ 0.56 / 0.51 | — | 9 trees | meas | S18 Results (§3, text after Table 3) | ✔ |
| Radius ratio per equal bifurcation, 2^−α (my arithmetic from S18) | manual 0.65; TreeQSM 0.73; WBE 0.71 | from the CIs above | as above | derived | S18 | ✔ arithmetic |
| Length ratio per equal bifurcation, 2^−β (my arithmetic from S18) | manual 0.95; TreeQSM 0.75; WBE 0.79 | wide (β CI crosses 0 for manual) | as above. Read: real tropical branches barely shorten per order, while TreeQSM makes them shorten. Manual is the reference | derived | S18 | ✔ arithmetic |
| TreeQSM branch diameter error by class | 10–20 cm: +40% (overestimate); 20–30 cm: −2%; 30–40 cm: −15% | n = 160 / 67 / 26 branches | Vaitarna dataset (Lau et al. 2018) | meas | S18 Table 1 | ✔ |
| Default model ratios: length 0.794, diameter 0.707 per branching | 0.794 = 2^−1/3; 0.707 = 2^−1/2 | — | model defaults: n = 2, c = 0.794 "elastic similarity", b = 0.707 "area-preserving" | **param** | S21 Table 2 | ✔ |
| Length ∝ D^b predicted exponent b | MST 2/3; STRESS 1/2; GEOM 1 | — | theory | model | S20 Table 1 | ✔ |
| Measured savanna length-scaling | exponents "greater than predicted by MST", rising with rainfall; closest to GEOM (values shown only in a figure) | — | 25 felled trees, 286 branches, 3 species (*Detarium microcarpum*, *Combretum geitynophylum*, *C. glutinosum*), 3 West African savanna sites | meas (qualitative) | S20 Results / Discussion | ✔ text; numbers ⚠ figure only |
| Leonardo rule, *Acer rufinerve* | held at branching nodes with one shed branch, **not** at nodes without shedding | — | 295 nodes, 13 untreated trees (decussate species) | meas | S34 abstract | ✔ abstract |

**Main finding:** in the one field study I verified (S4), real branching points sit close to area preservation. Deviation grows when lateral daughters are heavy and leave at wide angles. For a generator, Δ ≈ 2 (range 1.8–2.3, S5) is the supported default.

### 2.2 Length and angle per branching order (model constants, not measurements)

ABOP Honda monopodial trees (S2 Table 2.1, p.57; ✔). r1 = trunk contraction ratio, r2 = lateral contraction ratio, a0 = branching angle off the trunk, a2 = angle for lateral axes, divergence d = 137.5°, width ratio wr = 0.707:

```csv
figure,r1,r2,a0_deg,a2_deg,source
2.6a,0.9,0.6,45,45,S2 Table 2.1 p.57
2.6b,0.9,0.9,45,45,S2 Table 2.1 p.57
2.6c,0.9,0.8,45,45,S2 Table 2.1 p.57
2.6d,0.9,0.7,30,-30,S2 Table 2.1 p.57
```

ABOP Aono–Kunii sympodial trees (S2 Table 2.2, p.58; ✔). r1, r2 are the two daughter contraction ratios and a1, a2 the two branching angles; wr = 0.707:

```csv
figure,r1,r2,a1_deg,a2_deg,source
2.7a,0.9,0.7,5,65,S2 Table 2.2 p.58
2.7b,0.9,0.7,10,60,S2 Table 2.2 p.58
2.7c,0.9,0.8,20,50,S2 Table 2.2 p.58
2.7d,0.9,0.8,35,35,S2 Table 2.2 p.58
```

ABOP ternary-branching trees (S2 Table 2.3, p.61; ✔). d1, d2 = divergence angles, a = branching angle, lr = elongation rate, e = tropism susceptibility, n = derivation steps; width-increase rate vr = 1.732:

```csv
figure,d1_deg,d2_deg,a_deg,lr,tropism_T,e,n,source
2.8a,94.74,132.63,18.95,1.109,"0,-1,0",0.22,6,S2 Table 2.3 p.61
2.8b,137.50,137.50,18.95,1.109,"0,-1,0",0.14,8,S2 Table 2.3 p.61
2.8c,112.50,157.50,22.50,1.790,"-0.02,-1,0",0.27,8,S2 Table 2.3 p.61
2.8d,180.00,252.00,36.00,1.070,"-0.61,0.77,-0.19",0.40,6,S2 Table 2.3 p.61
```

Space colonization (S3, ✔): the kill distance d_k is a multiple of the node spacing D. The figure examples use d_k = 2D and 20D, with N = 375–12000 attraction points; radius of influence d_i = ∞, 17D (shrubs, Fig. 4) or 8D (trees, Fig. 5); N and d_k in Fig. 3 (S3). These are illustration settings, not recommended values.

### 2.3 Phyllotaxis (leaf arrangement angle)

| Rule | Value | Source | ✔/⚠ |
|---|---|---|---|
| Golden / Fibonacci divergence angle 360°·τ⁻² | ≈137.5° | S2 p.99; S13 Table 1 | ✔ |
| Same angle as a fraction of a turn | 0.382 | S13 Table 1 | ✔ |
| Main fraction series | 1/2, 1/3, 2/5, 3/8, 5/13, 8/21, 13/34 (→ 180°, 120°, 144°, 135°, 138.5°…) | S13 text | ✔ (the degree equivalents are plain arithmetic) |
| Traditional textbook examples (S13 says this list "is often presented") | 1/2 elm, lime, linden; 1/3 beech, hazel; 2/5 oak, cherry, apple, holly, plum; 3/8 poplar, rose, pear; 5/13 almond | S13 Introduction | ✔ that S13 says this; S13 says "the phyllotaxis fraction is not a determined trait of each species" |
| Anomalous series angles, with species | 99.5° (*Cunninghamia*, *Cedrus deodara*); 151.1° (*Betula alba*, *Corylus*); 106.4° (*Abies balsamea*); 137.5° (*Picea abies* cone, *Populus deltoides*) | S13 Table 1 | ✔ |
| Decussate (opposite pairs, each pair turned 90°) | 90° | S2 §3.3 (lilac inflorescence, Fig. 3.20, pp.93–94) | ✔ |
| Weber & Penn "nRotate" spiral angle used for most branch levels | 140° (77° and 80° for some levels; −120° alternating for willow) | S1 Appendix | ✔ (param) |
| *Acer* (maples) | leaves **opposite** (rarely subopposite) → decussate | S25 FNA *Acer* | ✔ |
| Fagaceae (*Quercus*, *Fagus*) | leaves **alternate, spirally arranged** (no fraction given) | S25 FNA Fagaceae | ✔ |
| *Betula* | branches, branchlets and leaves "nearly 2-ranked"; leaves mostly on short shoots | S25 FNA *Betula* | ✔ (compare the historic 151.1° for *B. alba*, S13) |
| Salicaceae (*Salix*, *Populus*) | leaves **alternate, spirally arranged** (opposite or subopposite in *Salix purpurea*); *Salix* branching sympodial | S25 FNA Salicaceae, *Salix* | ✔ |
| *Picea* | **branches whorled**; leaves borne singly, spreading in all directions | S25 FNA *Picea* | ✔ |
| *Pinus sylvestris* | leaves (needles) **2 per fascicle**; branches spreading to ascending | S25 FNA *P. sylvestris* var. *sylvestris* | ✔ |
| *Olea* (olive) | leaves **opposite** | S33 VicFlora *Olea* | ✔ |
| *Eucalyptus globulus* | juvenile leaves **opposite** "for many pairs"; adult leaves **alternate** (heteroblastic) | S33 VicFlora *E. globulus* | ✔ |
| Coconut (*Cocos nucifera*) | 5 visible leaf spirals (as in *Copernicia*); oil palm 8; *Phoenix canariensis* 13. No measured divergence angle given for coconut | S30 p.238–239 | ✔ |

## 3. Height–DBH and crown–DBH allometry (cross-species models)

| Model | Equation and coefficients | Units | Data | Source | ✔ |
|---|---|---|---|---|---|
| Pantropical H–D | ln H = 0.893 − E + 0.760 ln D − 0.0340 (ln D)²; E = (0.178·TS − 0.938·CWD − 6.61·PS)·10⁻³ | H m, D cm | harvest data, 58 tropical sites, 4004 trees ≥ 5 cm | S8 Eq. 6a/6b | ✔ (author PDF; journal page number not checked) |
| Global D from height × crown diameter | D = 0.557·(H·CD)^0.809·exp(0.056²/2) | D cm, H m, CD m | 108,753 trees worldwide; RMSE 9.7 cm | S10 Eq. 6 | ✔ |
| Tropical crown-diameter exponent (Cdia ∝ D^b), forest | b = 0.63 | [0.57–0.69] 95% CI | 87,737 trees, 205 forest + 40 savanna sites (pantropical) | S9 Table 1 | ✔ |
| Same, savanna | b = 0.75 | [0.67–0.82] | as above | S9 Table 1 | ✔ |
| Crown-depth exponent, forest / savanna | 0.60 [0.47–0.72] / 0.69 [0.67–0.72] | — | as above | S9 Table 1 | ✔ |
| Crown-volume exponent, forest / savanna | 1.88 [1.64–2.12] / 2.15 [2.10–2.21] | — | as above | S9 Table 1 | ✔ |
| Tropical regional H–D (log–log), pantropical | ln H = 1.2229 + 0.5320 ln D | H m, D cm | ≈40,000 trees (36,386 primary + 3,569 secondary forest), 22 countries, D ≥ 10 cm | S27 Appendix Table A2 (dm version: Table 2, SE 0.0151 / 0.0070) | ✔ |
| Same, by continent (β0, β1) | Asia 1.2194, 0.5767; Africa 0.9043, 0.6170; Australia 1.2113, 0.5202; S. America 1.3760, 0.4854 | H m, D cm | as above | S27 Table A2 | ✔ |
| Same, by region (β0, β1) | C. Africa 1.1525, 0.5547; E. Africa 0.6757, 0.6521; W. Africa 0.8946, 0.6365; Brazilian Shield 1.3818, 0.4651; E-C Amazonia 1.1562, 0.5072; Guyana Shield 1.5473, 0.4985; W. Amazonia 1.4799, 0.4669; Australia 1.2078, 0.5214; Asia 1.2156, 0.5782 | H m, D cm | as above | S27 Table A2 | ✔ |

## 4. Per-species tables

**Units and model forms used below:**
- **Pretzsch (S6):** ln(cr) = a + α·ln(d) for crown radius (95% quantile, i.e. open-grown maximum crown). ln(h) = a + α·ln(d) for height (5% quantile, i.e. the *low* end of height at a given diameter, as for open-grown trees). d is DBH in cm at 1.3 m; cr and h are in m. cr25 and h25 are the paper's own predictions at d = 25 cm. Data are mostly Southern German long-term plots (LTP), plus 9 cities worldwide. I checked the signs of a by recomputing cr25 and h25 from a and α; all match the printed values.
- **Bechtold (S7) Table 3, Eq. 4:** LCW = b0 + b1·D (+ b2·D²). LCW is the mean crown width of *stand-grown* trees in **ft**, and D is DBH in **inches** (D ≥ 5.0 in.). Data are Forest Health Monitoring plots in 24 eastern US states, 1991–1999. RSQ and CV are given as reported.

### 4.1 Temperate broadleaf

**Oak: *Quercus robur* (Europe); *Q. alba*, *Q. rubra* (eastern US)**

| Parameter | Value | Spread / n | Kind | Source | ✔ |
|---|---|---|---|---|---|
| *Q. robur* data range | d 3.6–196.1 cm (mean 35.2); cr 0.2–13.8 m (mean 3.1); age 16–220 yr (mean 104) | n = 4768 LTP, plus Cape Town 27 park / 44 street | meas | S6 Table 1 | ✔ |
| *Q. robur* crown radius | a = −1.4370, α = 0.8157; cr25 = 3.3 m | SE(a) 0.0353, SE(α) 0.0103; n = 4839 | meas | S6 Table 2 | ✔ |
| *Q. robur* height (5% quantile) | a = −0.5239, α = 0.7163; h25 = 5.9 m | SE 0.2355 / 0.0551; n = 69 (small) | meas | S6 Table 4 | ✔ |
| *Q. alba* crown width | b0 = 5.9658, b1 = 1.5212 | RSQ 0.68, RMSE 4.8 ft, CV 23%; n = 1586 | meas | S7 Table 3 / Table 1 | ✔ (b1 is OCR-garbled "1 S212" in the PDF; read as 1.5212. Eq. 5 gives 1.5158) |
| *Q. rubra* crown width | b0 = 6.2141, b1 = 1.4026 | RSQ 0.61, RMSE 5.7, CV 25% | meas | S7 Table 3 | ✔ |
| CA black oak (*Q. kelloggii*) render set | Shape 2; BaseSize 0.05; Ratio 0.018, RatioPower 1.3; 1DownAngle 30, 1DownAngleV −30 (negative V: angle varies along the parent, not ±); 1Rotate 80; 1Branches 40; 1Length 0.8; 2DownAngle 45 ±10; 2Rotate 140; 2Branches 120; 2Length 0.2; Leaves 25 | — | **param** | S1 Appendix p.126 | ✔ |
| Phyllotaxis | 2/5 (144°) in the textbook list that S13 says "is often presented"; S13 says "the phyllotaxis fraction is not a determined trait of each species" | — | desc | S13 | ✔ (that S13 reports it) |

**Beech: *Fagus sylvatica*; *F. crenata*; *F. grandifolia***

| Parameter | Value | Spread / n | Kind | Source | ✔ |
|---|---|---|---|---|---|
| *F. sylvatica* data range | d 2.9–191.4 cm (mean 23.1); cr 0.1–14.3 m (mean 3.0); age 16–283 yr | n = 14,963 | meas | S6 Table 1 | ✔ |
| *F. sylvatica* crown radius | a = 0.0111, α = 0.4710; cr25 = 4.6 m | SE 0.0135 / 0.0045 | meas | S6 Table 2 | ✔ |
| *F. sylvatica* height | a = −0.2154, α = 0.8607; h25 = 12.9 m | SE 0.0764 / 0.0235; n = 640 | meas | S6 Table 4 | ✔ |
| *F. crenata* branch angle (lateral daughters) | 10–50° | 34 branching points, 6 trees | meas | S4 | ✔ |
| *F. grandifolia* crown width | b0 = 11.5347, b1 = 1.1368 | RSQ 0.38, RMSE 6.8, CV 31%; n = 777 | meas | S7 Table 3 | ✔ |
| Phyllotaxis | 1/3 (120°) in the textbook list (S13: "is often presented"; "the phyllotaxis fraction is not a determined trait of each species") | — | desc | S13 | ✔ |

**Maple: *Acer pseudoplatanus*; *A. saccharum***

| Parameter | Value | Spread / n | Kind | Source | ✔ |
|---|---|---|---|---|---|
| *A. pseudoplatanus* crown radius | a = −0.4446, α = 0.5339; cr25 = 3.6 m | SE 0.0447 / 0.0162; n = 942; d 4.7–56.7 cm | meas | S6 Tables 1–2 | ✔ |
| *A. pseudoplatanus* height | a = 0.9669, α = 0.5858; h25 = 17.3 m | SE 0.0376 / 0.0112; n = 3248 | meas | S6 Table 4 | ✔ |
| *A. saccharum* crown width | b0 = 9.9644, b1 = 1.0545 | RSQ 0.38, RMSE 5.4, CV 27%; n = 2259 | meas | S7 Table 3 | ✔ |
| *A. saccharum* mature size | 27–37 m tall, 76–91 cm DBH; extremes > 40 m, > 1.5 m DBH; lives 300–400 yr | — | desc | S14 (Tirmenstein 1991) | ✔ |

**Birch: *Betula pendula*; *B. papyrifera***

| Parameter | Value | Spread / n | Kind | Source | ✔ |
|---|---|---|---|---|---|
| *B. pendula* crown radius | a = −0.9688, α = 0.6483; cr25 = 3.1 m | SE 0.0766 / 0.0316; n = 31 (small); d 5.0–38.7 cm | meas | S6 | ✔ |
| *B. pendula* height | a = 0.6952, α = 0.5490; h25 = 11.7 m | SE 0.1773 / 0.0768; n = 26 | meas | S6 Table 4 | ✔ |
| *B. papyrifera* crown width | b0 = 5.2177, b1 = 1.3332 | RSQ 0.34, RMSE 4.7, CV 29%; n = 889 | meas | S7 Table 3 | ✔ |
| *B. papyrifera* form | 21–24 m tall, 25–30 cm DBH (up to 75 cm); Alaska 6–18 m. Narrow crown in forest, wide crown from near the base in the open; often multi-stemmed | — | desc | S14 (Uchytil 1991) | ✔ |
| Divergence angle, *Betula alba* | 151.1° (anomalous series) | — | meas (historic) | S13 Table 1 | ✔ |

**Aspen and tupelo (render references only)**

| Parameter | Value | Kind | Source | ✔ |
|---|---|---|---|---|
| Quaking aspen render set | Shape 7; BaseSize 0.4; Scale 13 ±3; Levels 3; Ratio 0.015, RatioPower 1.2; 1DownAngle 60, 1DownAngleV −50; 1Rotate 140; 1Branches 50; 1Length 0.3; 2DownAngle 45 ±10; 2Branches 30; 2Length 0.6; Leaves 25; LeafScale 0.17 | **param** | S1 Appendix | ✔ |
| *Populus tremuloides* crown width (measured) | b0 = 4.6305, b1 = 1.2798; RSQ 0.51; n = 1375 | meas | S7 Table 3 | ✔ |
| Black tupelo render set | Shape 4; BaseSize 0.2; Scale 23 ±5; Levels 4; Ratio 0.015, RatioPower 1.3; 1DownAngle 60, 1DownAngleV −40; 1Rotate 140; 1Branches 50; 2DownAngle 30; 3DownAngle 45; Leaves 6 | **param** | S1 Appendix | ✔ |

### 4.2 Conifers (temperate and boreal)

**Scots pine: *Pinus sylvestris***

| Parameter | Value | Spread / n | Kind | Source | ✔ |
|---|---|---|---|---|---|
| Crown radius | a = −1.0345, α = 0.6381; cr25 = 2.8 m | SE 0.0527 / 0.0172; n = 1609; d 5.3–76.2 cm; age 16–200 | meas | S6 | ✔ |
| Height | a = −0.3449, α = 0.9326; h25 = 14.3 m | SE 0.1807 / 0.0552; n = 331 | meas | S6 Table 4 | ✔ |
| Crown width (eastern US plantations) | b0 = 6.3738, b1 = 0.8917 | RSQ 0.20, RMSE 3.9, CV 29%; n = 87 | meas | S7 Table 3 | ✔ |
| Branch insertion angle at emergence | 45° + N(0, 5°) | then increases with whorl age | **param** (the authors say some parameters were set "through guesswork", with no fitting) | S12 §2.4.1 | ✔ |
| Branch azimuth shift between yearly whorls | 30° clockwise; branches evenly spaced round the stem | — | **param** | S12 §2.4.1 | ✔ |
| Crown shape | "open and spreading"; more branches per whorl than red or white pine | — | desc | S14 (Sullivan 1993) | ✔ |
| Branch angle, 1st order (trunk to branch), TLS | median 81°; IQR 62–99°; normal fit μ 82°, σ 29° | 10 trees, Loobos, Netherlands; H 12.5–23.8 m (mean 18.6), DBH 22.3–32.9 cm (mean 28.9); QSM vs point-cloud RMSE 9.28° | meas (TLS) | S19 Tables 1, 4, 5 | ✔ |
| Branch angle, 2nd order (branch to sub-branch), TLS | median 69°; IQR 52–90°; μ 72°, σ 31° | as above; RMSE 11.59° | meas (TLS) | S19 Table 5 | ✔ |
| Needles | 2 per fascicle | — | desc | S25 | ✔ |

**Norway spruce: *Picea abies***

| Parameter | Value | Spread / n | Kind | Source | ✔ |
|---|---|---|---|---|---|
| Crown radius | a = −0.6713, α = 0.5083; cr25 = 2.6 m | SE 0.0202 / 0.0063; n = 10,775; d 2.2–98.9 cm | meas | S6 | ✔ |
| Height | a = −0.1753, α = 0.9113; h25 = 15.8 m | SE 0.0347 / 0.0108; n = 6713 | meas | S6 Table 4 | ✔ |
| Crown width (eastern US) | b0 = 4.5152, b1 = 1.0654 | RSQ 0.58, RMSE 2.6, CV 21%; n = 100 | meas | S7 Table 3 | ✔ |
| Dominant-branch insertion angle (from stem) | ≈ 83–84° (mean of 33 seed sources, both trials); trial means ≈ 84° (Avrig) and ≈ 83° (Breţcu); the ≈ 91° value is one seed source (Moldoviţa) at one site, not a trial mean | 30-yr-old progeny trials, Romania; whorl at about 2.2 m | meas (**coarse**: scored as index 1 = < 90°, 2 = 90°, 3 = > 90°, then converted to degrees) | S11 Results | ✔ |
| Crown shape | narrowly conic when young, broadly columnar when old | — | desc | S14 (Sullivan 1994) | ✔ |
| Cone-scale divergence | 137.5° | — | meas (historic) | S13 Table 1 | ✔ |

**Douglas fir: *Pseudotsuga menziesii***

| Parameter | Value | Spread / n | Kind | Source | ✔ |
|---|---|---|---|---|---|
| Data range (German plots) | d 10.8–64.4 cm (mean 32.7); cr 1.0–6.9 m; age 40–51 | n = 1613 | meas | S6 Table 1 | ✔ |
| Crown radius | a = −1.2808, α = 0.7522; cr25 = 3.1 m | SE 0.1006 / 0.0291 | meas | S6 Table 2 | ✔ |
| Height | a = 0.1377, α = 0.8005; h25 = 15.1 m | SE 0.0433 / 0.0133; n = 2677 | meas | S6 Table 4 | ✔ |
| Crown width, western US (Eq. 3a: LCW ft = b0 + b1·D + b2·D², D in.) | b0 = 5.7753, b1 = 1.0639, b2 = −0.0109 | RSQ 0.51, RMSE 4.4 ft, CV 27%; n = 4088, D 5.0–68.7 in. | meas | S26 Tables 1, 3 | ✔ |

**Other western US species (Bechtold 2004, S26 Table 3 Eq. 3a; LCW ft, D in. ≥ 5.0; stand-grown FIA trees)**

| Species | b0 | b1 | b2 | RSQ / RMSE ft / CV% | n | Source | ✔ |
|---|---|---|---|---|---|---|---|
| Ponderosa pine | 2.3089 | 1.1388 | −0.0089 | 0.66 / 3.3 / 25 | 1413 | S26 Tables 1, 3 | ✔ |
| Lodgepole pine | −1.1994 | 1.5154 | −0.0232 | 0.54 / 2.9 / 30 | 2761 | S26 | ✔ |
| Subalpine fir | 3.9629 | 0.6469 | — | 0.44 / 2.5 / 26 | 1262 | S26 | ✔ |
| Western larch | 4.5176 | 0.7931 | — | 0.30 / 4.0 / 33 | 183 | S26 | ✔ |
| Quaking aspen (west) | 2.5515 | 1.2029 | — | 0.50 / 2.8 / 24 | 1383 | S26 | ✔ |
| Bigleaf maple | 10.0915 | 1.1139 | — | 0.27 / 8.6 / 38 | 106 | S26 | ✔ |
| Red alder | 4.7027 | 1.3537 | — | 0.47 / 5.5 / 32 | 409 | S26 | ✔ |
| Oregon white oak | −1.3160 | 2.9311 | −0.0866 | 0.42 / 3.7 / 24 | 126 | S26 | ✔ |

**Boreal: *Picea mariana* (black spruce), *Larix laricina* / *L. decidua*, *Picea glauca***

| Parameter | Value | Spread / n | Kind | Source | ✔ |
|---|---|---|---|---|---|
| *P. mariana* crown width | b0 = 2.8233, b1 = 0.9797 | RSQ 0.24, RMSE 3.1, CV 31%; n = 349 | meas | S7 Table 3 | ✔ |
| *P. mariana* mature size | 9–15 m tall, 15–25 cm DBH; 100–200-yr trees may be only 3–6 m. Branches run from ground to crown; dead branches kept | — | desc | S14 (FEIS *Picea mariana*) | ✔ |
| *L. laricina* crown width | b0 = 1.9502, b1 = 1.4938 | RSQ 0.39; n = 163 | meas | S7 Table 3 | ✔ |
| *L. decidua* crown radius | a = −1.2919, α = 0.7131; cr25 = 2.7 m | n = 340 | meas | S6 Table 2 | ✔ |
| *L. decidua* height | a = 0.8585, α = 0.6378; h25 = 18.4 m | n = 833 | meas | S6 Table 4 | ✔ |
| *P. glauca* crown radius / height | a = −1.7910, α = 0.8276, cr25 = 2.4 m; height a = 0.9573, α = 0.5714, h25 = 16.4 m | n = 143 (Prince George, Canada) | meas | S6 Tables 2, 4 | ✔ |
| *P. glauca* crown width (US) | b0 = 6.1834, b1 = 0.8567 | n = 200 | meas | S7 Table 3 | ✔ |
| *Abies homolepis* branch angle | 50–80° | 79 branching points, 7 trees | meas | S4 | ✔ |

### 4.3 Willow

| Parameter | Value | Kind | Source | ✔ |
|---|---|---|---|---|
| *Salix nigra* crown | "broad and open with stout branches" | desc | S14 (FEIS *Salix nigra*) | ✔ |
| *S. nigra* crown width vs DBH | DBH coefficient **not significant** at P = 0.05, so no usable slope | meas | S7 Results | ✔ |
| Weeping willow render set | Shape 3; BaseSize 0.05; Levels 4; Ratio 0.03, RatioPower 2; 0BaseSplits 2; 1DownAngle 20 ±10; 1Rotate −120 ±30 (alternating); 1Branches 25; 1Curve 40, 1CurveBack 80; 2Length 1.5; 3Branches 300; AttractionUp −3 (droop); PruneRatio 1 | **param** | S1 Appendix | ✔ |
| Phyllotaxis | sources disagree: 5/13 vs 3/8 (S13 notes this) | desc | S13 | ✔ |

### 4.4 Mediterranean / dry

| Species | Parameter | Value | Kind | Source | ✔ |
|---|---|---|---|---|---|
| *Eucalyptus globulus* | height | 30–55 m | desc | S14 (Esser 1993) | ✔ |
| *E. globulus* | crown | "open crowns bearing pendulous branches" | desc | S14 | ✔ |
| *Olea europaea* | H–D, crown–D, branch angle | **still not found** (see §5); leaf arrangement opposite (S33, §2.3) | — | — | — |

### 4.5 Tropical and savanna (generic only)

No species-level H–D or crown–D equation was verified for a rainforest emergent, coconut, savanna acacia or baobab. Use S8 (H–D with the site stress index E), S9 (crown exponents: forest 0.63, savanna 0.75) and S10 (D from H·CD). *Khaya senegalensis* (Hanoi street trees, n = 163, d 44.1–123.1 cm) has S6 values: crown a = −0.5889, α = 0.6461, cr25 = 4.4 m; height a = −0.4335, α = 0.7724, h25 = 7.8 m (n = 144) (S6 Tables 1, 2, 4; ✔). *Araucaria cunninghamii* (Brisbane, n = 126): crown a = −0.6754, α = 0.3899, cr25 = 1.8 m; height a = 0.1012, α = 0.6586, h25 = 9.2 m (S6; ✔).
**Pass 2 additions, species level (§4.5):**

| Species | Parameter | Value | Spread / n | Kind | Source | ✔ |
|---|---|---|---|---|---|---|
| *Ceiba pentandra* (emergent), Panama | height, gMM H = a·D^b/(k + D^b) (H m, D cm) | a = 53.6, b = 0.79, k = 23.8 | 90% posterior intervals: a 48.1–60.7; b 0.72–0.88; k 20.2–28.1 | meas (fitted) | S22 suppl. Table S2 | ✔ |
| *C. pentandra* | crown area A = a·D^b (m², D cm) | a = 0.288, b = 1.51 | 0.143–0.576; 1.36–1.65 | meas | S22 suppl. Table S2 | ✔ |
| *C. pentandra*, model output (my arithmetic) | H and crown diameter at D = 50 / 100 / 200 cm | 25.7 / 33.0 / 39.4 m; 11.6 / 19.6 / 33.1 m | point estimates only | derived | S22 | ✔ arithmetic |
| *Dipteryx oleifera* (emergent), Panama | height gMM | a = 64.5, b = 0.80, k = 23.0 | 57.4–74.4; 0.73–0.87; 19.5–26.8 | meas | S22 suppl. Table S2 | ✔ |
| *D. oleifera* | crown area | a = 0.530, b = 1.47 | 0.222–1.178; 1.30–1.67 | meas | S22 suppl. Table S2 | ✔ |
| *D. oleifera*, model output (my arithmetic) | H and crown diameter at D = 50 / 100 / 200 cm | 32.2 / 40.9 / 48.4 m; 14.6 / 24.2 / 40.4 m | — | derived | S22 | ✔ arithmetic |
| BCI dataset context | 162 species; 9884 trees for height, 2425 for crown area; Barro Colorado Nature Monument, Panama | — | — | meas | S22 §2 | ✔ |
| Guyana rainforest canopy species: greenheart (*Chlorocardium rodiei*), kabukalli (*Goupia glabra*), mora (*Mora excelsa*), morabukea (*M. gonggrijpii*), wallaba (*Eperua falcata*), wamara (*Swartzia leiocalycina*) | 1st-order branch angle, **from horizontal**, TLS median | 42°, 43°, 39°, 45°, 47°, 41° | IQR (printed high–low): 60–10, 58–28, 62–20, 65–18, 58–16, 58–14; normal μ/σ: 32/38, 39/25, 38/30, 39/31, 34/35, 38/30; n = 10, 5, 4, 9, 5, 4 trees; H 23.8–44.2 m (mean 33.3), DBH 22–126 cm (mean 60.9) | meas (TLS) | S19 Tables 1, 5 | ✔ (QSM vs point-cloud RMSE 22.19°, R² 0.36, so treat as coarse) |
| Same six species, same order | 2nd-order angle (between 1st- and 2nd-order branch), median | 60°, 55°, 51°, 55°, 48°, 57° | IQR 39–81, 37–74, 33–65, 39–75, 32–72, 38–77; μ/σ 64/35, 57/31, 53/29, 61/33, 56/34, 65/39 | meas (TLS) | S19 Table 5 | ✔ (RMSE 18.1°) |
| *Acacia tortilis* (savanna/desert acacia), Arava, Israel | height | 2.41 m | ± 0.17 SE; n = 9 felled trees | meas | S23 Results | ✔ |
| *A. tortilis* | crown size C1 + C2 (**sum** of N–S and E–W diameters, not one diameter) | 5.55 m (≈ 2.8 m mean diameter, my arithmetic) | ± 0.32 SE; n = 9 | meas | S23 Results | ✔ |
| *A. raddiana* | height; C1 + C2 | 3.09 ± 0.27 m; 5.08 ± 0.47 m (SE; n = 9 each) | — | meas | S23 Results | ✔ |
| *A. tortilis* form | many stems branching from the base, so DBH covers only one stem; *A. raddiana* has one stem at ground level | — | desc | S23 Discussion | ✔ |
| Baobab (*Adansonia digitata*), Benin | form by climate zone | trees in the Sudanian and Sudano-Guinean zones were **short**; Guinean-zone trees **tall**; height and number of branches correlated with genetic structure | 137 trees, 6 populations | meas (qualitative) | S32 abstract | ✔ abstract; zone means ⚠ (full text not readable) |

### 4.6 Desert

| Species | Parameter | Value | Kind | Source | ✔ |
|---|---|---|---|---|---|
| *Yucca brevifolia* (Joshua tree) | height | 5–20 m (written "20- to 70-foot"); trees > 10 m rare | desc | S14 (Gucker 2006) | ✔ |
| Joshua tree | trunk diameter | 0.3–0.9 m, expanded base | desc | S14 | ✔ |
| Joshua tree | height before first branching | 0.9–3 m | desc | S14 | ✔ |
| Joshua tree | branching rule | dichotomous or nearly so; a branch forms after the terminal bud dies (flowering or insect damage) | desc | S14 | ✔ |
| Joshua tree | branch length / fork interval | branches 2–5 m or longer; fork every 0.6–0.9 m; inner branches erect, outer horizontal or drooping | desc | S14 | ✔ |
| *Carnegiea gigantea* (saguaro) | first-arm height | "some" first arms at about 3.25 m (preliminary citizen-science data); arms start at 8–20 ft (≈2.4–6.1 m) | meas (prelim.) / desc | S15 (arms project page; growth page) | ✔ |
| Saguaro | age at first arm | usually 50–70 yr; up to 100 yr in some areas | desc | S15 | ✔ |
| Saguaro | height–age milestones | 1–3 in at 5–10 yr; 2 ft at 20–40 yr; 6–8 ft at 35–70 yr; healthy adults about 40 ft at > 150 yr; tallest recorded 78 ft (no arms) | desc | S15 growth page | ✔ |
| Saguaro | first-arm compass direction | tends to face south or southeast; second arm often north | meas (prelim.) | S15 arms page | ✔ |
| Saguaro, Organ Pipe Cactus NM | height–age milestones and first arm | 10 yr; 30 yr; 3 ft at 50 yr; 6½ ft at 70 yr; **first arm at 15–16 ft (≈4.6–4.9 m) at 95–100 yr**; 43 ft at 200 yr | desc (agency) | S28 | ✔ |
| Saguaro | site growth factor (age at a given height scales with it) | Saguaro NP East 1.0 (baseline); SNP West 0.743; Organ Pipe 0.617 | meas (regression) | S29 abstract | ✔ |

### 4.7 Crown ratio by species group (USFS FIA, northern US; pass 2)

Westfall, Westfall & Randolph 2019 (S24) Table 1. Uncompacted crown ratio (UNCR: live-crown length ÷ total height, with no gap filling) and compacted crown ratio (CCR: lower branches moved up visually to fill gaps). Trees with DBH ≥ 1.0 in. across 24 northern US states. Values are means, with IQR in brackets; minima of 0.01–0.10 and maxima of 0.85–0.99 for every group. ⚠ **Group labels:** the Table 1 PDF text has no group names, so rows were matched to the Table 2 group numbers 1–27 by order. Both tables have 27 rows in the same layout.

| # | Species group | n | mean DBH in. | UNCR mean [IQR] | CCR mean [IQR] | Source |
|---|---|---|---|---|---|---|
| 1 | Loblolly and shortleaf pines | 2,110 | 9.3 | 0.42 [0.20] | 0.31 [0.18] | S24 T1/T2 |
| 2 | Other yellow pines | 1,979 | 8.7 | 0.45 [0.30] | 0.31 [0.20] | S24 |
| 3 | Eastern white pine | 6,406 | 10.0 | 0.52 [0.30] | 0.37 [0.20] | S24 |
| 4 | Jack pine | 1,722 | 7.4 | 0.52 [0.30] | 0.39 [0.18] | S24 |
| 5 | Spruce and balsam fir | 31,755 | 5.2 | 0.61 [0.35] | 0.45 [0.30] | S24 |
| 6 | Eastern hemlock | 6,588 | 9.0 | 0.67 [0.32] | 0.47 [0.25] | S24 |
| 7 | Other eastern softwoods | 16,289 | 7.6 | 0.63 [0.31] | 0.44 [0.22] | S24 |
| 8 | Red pine | 3,856 | 8.7 | 0.53 [0.29] | 0.41 [0.20] | S24 |
| 9 | Select white oaks | 9,500 | 10.1 | 0.60 [0.30] | 0.41 [0.15] | S24 |
| 10 | Select red oaks | 5,992 | 11.4 | 0.52 [0.20] | 0.38 [0.15] | S24 |
| 11 | Other white oaks | 4,003 | 9.5 | 0.52 [0.20] | 0.35 [0.10] | S24 |
| 12 | Other red oaks | 6,539 | 10.7 | 0.54 [0.25] | 0.38 [0.15] | S24 |
| 13 | Hickory | 6,462 | 8.1 | 0.61 [0.30] | 0.42 [0.20] | S24 |
| 14 | Yellow birch | 4,432 | 8.0 | 0.58 [0.25] | 0.42 [0.15] | S24 |
| 15 | Hard maple | 17,831 | 8.2 | 0.58 [0.25] | 0.41 [0.12] | S24 |
| 16 | Soft maple | 28,463 | 7.8 | 0.52 [0.21] | 0.37 [0.15] | S24 |
| 17 | Beech | 7,551 | 6.5 | 0.67 [0.35] | 0.45 [0.20] | S24 |
| 18 | Sweetgum | 858 | 8.5 | 0.50 [0.23] | 0.36 [0.15] | S24 |
| 19 | Tupelo and blackgum | 1,789 | 6.5 | 0.60 [0.30] | 0.40 [0.20] | S24 |
| 20 | Ash | 10,648 | 7.5 | 0.49 [0.25] | 0.35 [0.15] | S24 |
| 21 | Cottonwood and aspen | 14,078 | 7.4 | 0.42 [0.17] | 0.33 [0.15] | S24 |
| 22 | Basswood | 3,122 | 9.3 | 0.53 [0.25] | 0.38 [0.15] | S24 |
| 23 | Yellow poplar | 2,447 | 11.0 | 0.49 [0.23] | 0.35 [0.10] | S24 |
| 24 | Black walnut | 1,621 | 9.9 | 0.53 [0.25] | 0.37 [0.15] | S24 |
| 25 | Other eastern soft hardwoods | 24,135 | 7.3 | 0.53 [0.26] | 0.36 [0.20] | S24 |
| 26 | Other eastern hard hardwoods | 6,233 | 6.6 | 0.54 [0.30] | 0.36 [0.20] | S24 |
| 27 | Eastern noncommercial hardwoods | 8,445 | 4.0 | 0.58 [0.35] | 0.38 [0.15] | S24 |

Conversion model: UNCR = 1/(1 + exp(β0 + β1·CCR + β2·ln DBH)), with β per group in S24 Table 2. Examples: beech β0 0.6889, β1 −4.6325, β2 0.3158; cottonwood/aspen 1.6683, −4.4202, 0.0402 ✔. Pass 2 did not find species means for other regions.

## 5. Not found (searched, not verified in any primary source)

- **Olive (*Olea europaea*):** no verified height–DBH, crown–DBH or branch-angle equation. A search summary claimed Brunori et al. 2017 gave "crown diameter = 0.1202·d^2.2159". The review that cites it (S17, Table 4) lists it as a **biomass** equation. **Not a crown equation: do not use.** Pass 2: leaf arrangement is now verified (opposite, S33). An Andria (Italy) old-grove biometry paper (Ann. For. Sci. 2007, doi 10.1051/forest:2007026) was found, but its PDF and HTML were blocked (HTML stub / 403), so its numbers are **unverified** and not used.
- **Coconut (*Cocos nucifera*):** frond count, stem H–D and crown width. The candidate (a KAU campus study on ResearchGate) could not be opened. Numbers in search summaries (8–18 m, DBH 20–32 cm, 24–35 fronds, crown exponent 0.36) are **unverified**. Pass 2: only the leaf-spiral count (5) is verified (S30). A CIRAD PDF (agritrop 314368) downloaded corrupt.
- **Baobab (*Adansonia digitata*):** no H–D or crown–D equation found. The open Kenya survey (PMC8172389) measured DBH and height but I found no reported means or equation in its text. Pass 2: Assogbadjo et al. 2006 (S32). Only the abstract was readable: tall in the Guinean zone, short in the Sudanian zone. A search summary gave zone mean heights of 13.5–21.2 m and crown diameters of 14.2–17.0 m, but these are **unverified** (PMC and OUP full text were blocked).
- **Savanna acacia (*Vachellia tortilis*):** the Turkana morphometric paper and the Indian Forester equations could not be opened. Pass 2: Arava (Israel) height and crown size verified (S23, §4.5). These are small hyper-arid trees (n = 9), not East African savanna giants. No H–D equation.
- **Rainforest emergent (*Ceiba*, *Shorea*, *Dipteryx*):** Pass 2: *Ceiba pentandra* and *Dipteryx oleifera* H–D and crown models verified (S22, §4.5). Still nothing for *Shorea* or other dipterocarps, and no measured branch angle for *Ceiba*.
- **Species-specific branch angles** for oak, maple, birch, Douglas fir, eucalyptus, willow and olive: none verified. A TLS beech study (Forest Ecosystems 2024, doi 10.1016/j.fecs.2024.100279) and Can. J. For. Res. 2021 (beech 30.7° pure vs 54.1° mixed stands, per a search summary) could not be opened (empty or 403). **Unverified.**
- **Branch length and diameter ratios per order from field measurement:** Bentley et al. 2013 (S16) has them, but only the abstract was readable (pass 2: still paywalled, and the ESA copy timed out on curl and WebFetch). Pass 2 adds Lau et al. 2019 field exponents (S18, §2.1). Malhi et al. 2018 (TLS QSM, Wytham Woods) could not be downloaded (the PDF fetch returned an HTML stub). West–Brown–Enquist theoretical ratios were not checked.
- **Crown ratio (live crown length ÷ height) by species:** pass 2 added northern US FIA group means (S24, §4.7). Still missing: southern (Randolph 2010, download empty), western and European species means. Bechtold's data cover crown ratios 5–99% but give no species mean in the text I read.
- **Per-species phyllotaxis from a botanical flora:** only the "often presented" textbook list and anomalous examples in S13. Pass 2 added arrangement (opposite / alternate-spiral / whorled / fascicled) from FNA and VicFlora (§2.3). No flora gives a numeric divergence angle for these trees. eFloras / Flora of China returned HTTP 500.
- **Jenkins et al. 2003, Bechtold 2004 (western US), Feldpausch 2011/2012, GlobAllomeTree, FVS height–diameter equations, Shinozaki 1964, Sone et al. 2009, West et al. 1999:** Pass 2 opened Jenkins 2003 (S31: biomass only, not form), Bechtold 2004 (S26), Feldpausch 2011 (S27) and the Sone 2009 abstract (S34). Still not opened: GlobAllomeTree, FVS H–D equations, Shinozaki 1964, West et al. 1999, Feldpausch 2012.
- **Runions et al. 2007 recommended values:** the paper gives only illustration settings, no species parameter sets.

## 6. Sources disagree / caveats / unverified

1. **Saguaro arm onset.** S15 (NPS) says first arms at 50–70 yr (up to 100 yr) and about 3.25 m in early data. A search summary attributed ~4.5 m and ~95–100 yr to Drezner 2003 (Am. J. Bot. 90:911) and an NPS brochure; **Pass 2: resolved.** The 95–100 yr and 15–16 ft (≈4.6–4.9 m) first-arm figures are in the NPS **Organ Pipe** brochure (S28), not in Drezner 2003. The Drezner 2003 abstract (S29) gives no arm height; it gives site growth factors (Organ Pipe 0.617 vs Saguaro NP East 1.0), so Organ Pipe saguaros reach a given height later. Both sources are right for their own site. Arm onset varies with region and rainfall (S15).
2. **Willow phyllotaxis.** 5/13 vs 3/8, depending on the reference (S13). More generally, S13 says fractions are not fixed per species.
3. **Height at a given DBH depends on the quantile and stand type.** S6 heights are the **5% quantile** (open-grown, short for their diameter). For example, *Q. robur* h25 = 5.9 m from only n = 69. Forest-grown trees of the same DBH are much taller. S7 crown widths are for **stand-grown** trees, so they are narrower than S6's 95%-quantile open-grown crowns. These are not contradictions, but don't mix them.
4. **Pipe exponent.** S3 uses n "between 2 and 3" (from Macdonald 1983). The measured literature range in S5 is 1.8–2.3. ABOP uses exactly 2 (wr = 0.707). Prefer ≈2 for real trees.
5. **Biomechanics vs Leonardo.** S4 finds *F. crenata* fits the uniform-stress model best (index 0.97), while *A. homolepis* is closer to plain area preservation.
6. **OCR uncertainty in S7:** *Q. alba* b1 is printed as "1 S212" and read as 1.5212. Treat the third digit as ⚠. Pass 2 / Library Verify: Eq. 5 prints 1.5158 and no clean copy was found, so the ⚠ stays.
7. **Norway spruce branch angle (S11)** was scored on a 3-step index, so the degree values are coarse conversions, not protractor readings.
8. **Scots pine 45° ± 5° (S12)** is a model input partly set by guesswork, not a measurement.
9. **Chave 2014 page number:** the equation was checked in the author's PDF. The journal page (p.3183, per treeallometry.org) was not checked ⚠.
10. **Primary studies behind the S5 range** (Mandelbrot 1983; Aratsu 1998; Sone et al. 2009) were not opened ⚠. Pass 2: the Sone 2009 abstract (S34) was read. It is a field test of whether the rule holds (295 nodes), not an exponent value. Mandelbrot 1983 (a book) and Aratsu 1998 (a thesis) were not reachable, so the 1.8–2.3 range still rests on S5.
11. **Pass 2: TLS vs manual branch data.** TreeQSM overestimates thin-branch (10–20 cm) diameter by about 40% and gives very different length exponents (β 0.42 vs manual 0.07; S18). Use the manual values for ratios per order. S19 tropical 1st-order angles have RMSE 22° against the point cloud, so treat them as coarse.
12. **Pass 2: angle reference frames differ.** S19 measures tropical 1st-order angles from the **horizontal**, but Scots pine and all 2nd-order angles from the **parent axis**. S4 and S11 measure from the stem. Convert before comparing (from horizontal = 90° − from vertical stem, for upward branches).
13. **Pass 2 ⚠ items:** S24 group labels were assigned by row order (§4.7). S20 savanna length exponents appear only in a figure, so no numbers are given (§2.1). Baobab zone mean heights and crowns come from a search summary only (§4.5, §5). Andria olive and coconut search-summary numbers are not used (§5).

## 7. References (all accessed 2026-11-16)

"Link check" is the result of opening the link on that date. A 403/500 from curl is a bot-block; in those cases the content was read through the alternative URL given.

- **S1** Weber, J. & Penn, J. (1995). Creation and rendering of realistic trees. *Proc. SIGGRAPH '95*, 119–128. DOI https://doi.org/10.1145/218380.218427 (ACM blocks curl, 403). Full text read at https://courses.cs.duke.edu/fall02/cps124/resources/p119-weber.pdf (200). Appendix "Parameter List", printed p.126. © ACM.
- **S2** Prusinkiewicz, P. & Lindenmayer, A. (1990). *The Algorithmic Beauty of Plants*. Springer-Verlag. Author PDF https://algorithmicbotany.org/papers/abop/abop.pdf (206/200). Tables 2.1 (p.57), 2.2 (p.58), 2.3 (p.61); 137.5° on p.99. © Springer / authors.
- **S3** Runions, A., Lane, B. & Prusinkiewicz, P. (2007). Modeling trees with a space colonization algorithm. *Eurographics Workshop on Natural Phenomena*. https://algorithmicbotany.org/papers/colonization.egwnp2007.pdf (206/200). Eq. 1; Figs. 3–5. © Eurographics / authors.
- **S4** Minamino, R. & Tateno, M. (2014). Tree branching: Leonardo da Vinci's rule versus biomechanical models. *PLoS ONE* 9(4): e93535. https://doi.org/10.1371/journal.pone.0093535 (200). CC BY 4.0.
- **S5** Eloy, C. (2011). Leonardo's rule, self-similarity, and wind-induced stresses in trees. *Phys. Rev. Lett.* 107, 258101. https://doi.org/10.1103/PhysRevLett.107.258101 (APS blocks curl, 403). Read at https://www.irphe.fr/~eloy/assets/pdf/PRL2011.pdf (200) and https://arxiv.org/abs/1105.2591 (200). © APS.
- **S6** Pretzsch, H. et al. (2015). Crown size and growing space requirement of common tree species in urban centres, parks, and forests. *Urban For. Urban Green.* 14(3): 466–479. https://doi.org/10.1016/j.ufug.2015.04.006 (200). Open repository record (Universidad de Chile; repository page lists **CC BY-NC-ND 3.0 Chile**, while the article states CC BY-NC-ND 4.0, so both are flagged): https://repositorio.uchile.cl/handle/2250/135852 (200 on 2026-11-16; its PDF bitstream returns an HTML page to curl). Pass 1 numbers were read from a full-text copy formerly at the Stellenbosch URL (http://academic.sun.ac.za/forestry/people/Bdt/…, now HTTP 500, link retired) and re-checked by Library Verify. Tables 1, 2, 4. CC BY-NC-ND 4.0 (stated in the article).
- **S7** Bechtold, W.A. (2003). Crown-diameter prediction models for 87 species of stand-grown trees in the eastern United States. *South. J. Appl. For.* 27(4): 269–278. https://research.fs.usda.gov/treesearch/6206 (200); PDF https://research.fs.usda.gov/treesearch/download/6206.pdf (200); DOI https://doi.org/10.1093/sjaf/27.4.269 (403 to curl). Tables 1, 3. US Forest Service author; reprint marked "Not for further reproduction".
- **S8** Chave, J. et al. (2014). Improved allometric models to estimate the aboveground biomass of tropical trees. *Global Change Biology* 20(10): 3177–3190. https://doi.org/10.1111/gcb.12629 (403 to curl). Read at https://jeromechave.github.io/pubs/chave-gcb14.pdf (200). Eq. 6a/6b. © Wiley.
- **S9** Loubota Panzou, G.J. et al. (2021). Pantropical variability in tree crown allometry. *Global Ecol. Biogeogr.* 30(2): 459–475. https://doi.org/10.1111/geb.13231 (403 to curl). Read at https://agritrop.cirad.fr/599164/1/Loubota%26al%282020%29PantropicalVariabilityInTreeCrownAllometry.pdf (200). Table 1. CC BY 4.0.
- **S10** Jucker, T. et al. (2017). Allometric equations for integrating remote sensing imagery into forest monitoring programmes. *Global Change Biology* 23: 177–190. https://doi.org/10.1111/gcb.13388 (403 to curl). Read at https://research-information.bris.ac.uk/ws/files/232002669/gcb.13388.pdf (200); also https://pmc.ncbi.nlm.nih.gov/articles/PMC6849852/ (200). Eq. 6. CC BY 4.0.
- **S11** Budeanu, M., Şofletea, N. & Pârnuţă, G. (2012). Qualitative traits of Norway spruce depending on first-order branches: evaluation in comparative trials. *Not. Bot. Horti Agrobo.* 40(2): 295–301. https://doi.org/10.15835/nbha4028148 (500 to curl). Read at https://www.notulaebotanicae.ro/index.php/nbha/article/download/8148/7042/30157 (opened via WebFetch; curl 403). Licence not stated in the text read.
- **S12** Mäkelä, A., Vanninen, P. & Ikonen, V.-P. (1997). An application of process-based modelling to the development of branchiness in Scots pine. *Silva Fennica* 31(3): 369–380. https://www.silvafennica.fi/pdf/5632 (200). §2.4.1. Open access; article licence not confirmed.
- **S13** Okabe, T. (2015). Biophysical optimality of the golden angle in phyllotaxis. *Scientific Reports* 5: 15358. https://www.nature.com/articles/srep15358 (200); Table 1 at https://www.nature.com/articles/srep15358/tables/1. CC BY 4.0.
- **S14** USDA Forest Service, Fire Effects Information System (FEIS), US government work, public domain. All 200:
  - Tirmenstein 1991, *Acer saccharum*: https://www.fs.usda.gov/database/feis/plants/tree/acesac/all.html
  - Uchytil 1991, *Betula papyrifera*: https://www.fs.usda.gov/database/feis/plants/tree/betpap/all.html
  - *Picea mariana*: https://www.fs.usda.gov/database/feis/plants/tree/picmar/all.html
  - Sullivan 1994, *Picea abies*: https://www.fs.usda.gov/database/feis/plants/tree/picabi/all.html
  - Sullivan 1993, *Pinus sylvestris*: https://www.fs.usda.gov/database/feis/plants/tree/pinsyl/all.html
  - *Salix nigra*: https://www.fs.usda.gov/database/feis/plants/tree/salnig/all.html
  - Esser 1993, *Eucalyptus globulus*: https://www.fs.usda.gov/database/feis/plants/tree/eucglo/all.html
  - Gucker 2006, *Yucca brevifolia*: https://www.fs.usda.gov/database/feis/plants/tree/yucbre/all.html
- **S15** US National Park Service, Saguaro National Park, both 200, public domain:
  - "Saguaro Arms Citizen Science Project" (updated 2 May 2024): https://www.nps.gov/sagu/learn/saguaro-arms-project.htm
  - "Saguaro Growth" (updated 19 Dec 2024): https://www.nps.gov/sagu/learn/nature/saguaro-growth.htm
- **S16** Bentley, L.P. et al. (2013). An empirical assessment of tree branching networks and implications for plant allometric scaling models. *Ecology Letters* 16: 1069–1078. https://doi.org/10.1111/ele.12127 (403 to curl). Abstract read via the Crossref API https://api.crossref.org/works/10.1111/ele.12127 (200). © Wiley.
- **S17** Review of orchard allometry, Table 4 (cites Brunori et al. 2017): https://pmc.ncbi.nlm.nih.gov/articles/PMC12753925/ (200). Open (PMC).
- **S18** Lau, A., Martius, C., Bartholomeus, H., Shenkin, A., Jackson, T., Malhi, Y., … Bentley, L.P. (2019). Estimating architecture-based metabolic scaling exponents of tropical trees using terrestrial LiDAR and 3D modelling. *For. Ecol. Manage.* 439: 132–145. https://doi.org/10.1016/j.foreco.2019.02.019. Post-print read at https://edepot.wur.nl/472345 (200). Tables 1, 3. CC BY-NC-ND.
- **S19** Bekkers, Evers, Lau & Buckley (2025). Improving the 3D representation of plant architecture and parameterization efficiency of functional–structural tree models using terrestrial LiDAR data. *AoB PLANTS* 17(2): plae071. https://pmc.ncbi.nlm.nih.gov/articles/PMC11826235/ (opened via Europe PMC full-text XML https://www.ebi.ac.uk/europepmc/webservices/rest/PMC11826235/fullTextXML, 200). Tables 1, 4, 5. CC BY 4.0.
- **S20** Tredennick, A.T., Bentley, L.P. & Hanan, N.P. (2013). Allometric convergence in savanna trees and implications for the use of plant scaling models in variable ecosystems. *PLoS ONE* 8(3): e58241. https://doi.org/10.1371/journal.pone.0058241 (200). Table 1, Results. CC BY.
- **S21** Sperry, J.S., Smith, D.D., Savage, V.M., Enquist, B.J., McCulloh, K.A., Reich, P.B., Bentley, L.P. & von Allmen, E.I. (2012). A species-level model for metabolic scaling in trees I. *Functional Ecology* 26: 1054–1065. https://doi.org/10.1111/j.1365-2435.2012.02022.x; author PDF https://sperry.biology.utah.edu/publications/Sperry%20FuncEcol%202012.pdf (200). Table 2 defaults (n = 2; c = 0.794, elastic similarity; b = 0.707, area-preserving). © Wiley/BES.
- **S22** Martínez Cano, I. et al. (2019). Tropical tree height and crown allometries for the Barro Colorado Nature Monument, Panama: a comparison of alternative hierarchical models incorporating interspecific variation in relation to life history traits. *Biogeosciences* 16: 847–862. https://bg.copernicus.org/articles/16/847/2019/ (200); supplement https://bg.copernicus.org/articles/16/847/2019/bg-16-847-2019-supplement.pdf (200), Table S2. CC BY 4.0.
- **S23** Winters, G., Alexander, C., Tran, T.H., Matteucci, G. & Groner, E. (2025). Estimation of aboveground biomass of *Acacia* trees in the hyper-arid Arava, Israel using allometric analysis. *Open J. Ecol.* 15(2): 194–204. https://www.scirp.org/pdf/oje2025152_41381718.pdf (200). Results. CC BY 4.0.
- **S24** Westfall, J.A., Westfall, M.B.E. & Randolph, K.C. (2019). Modeling relations between compacted and uncompacted crown ratio for the northern United States. *Forest Science* 65(5): 593–601. https://research.fs.usda.gov/treesearch/61347 (200); PDF https://research.fs.usda.gov/download/treesearch/61347.pdf (200). Tables 1, 2. Public domain (US government work).
- **S25** Flora of North America online (opened via WebFetch; curl 403): *Acer* https://floranorthamerica.org/Acer; Fagaceae https://floranorthamerica.org/Fagaceae; *Quercus* https://floranorthamerica.org/Quercus; *Fagus* https://floranorthamerica.org/Fagus; *Betula* https://floranorthamerica.org/Betula; Salicaceae https://floranorthamerica.org/Salicaceae; *Salix* https://floranorthamerica.org/Salix; *Picea* https://floranorthamerica.org/Picea; *Pinus sylvestris* var. *sylvestris* https://floranorthamerica.org/Pinus_sylvestris_var._sylvestris. © FNA Association.
- **S26** Bechtold, W.A. (2004). Largest-crown-width prediction models for 53 species in the western United States. *West. J. Appl. For.* 19(4): 245–251. https://doi.org/10.1093/wjaf/19.4.245; PDF https://research.fs.usda.gov/download/treesearch/7732.pdf (200). Tables 1, 3.
- **S27** Feldpausch, T.R. et al. (2011). Height–diameter allometry of tropical forest trees. *Biogeosciences* 8: 1081–1106. https://bg.copernicus.org/articles/8/1081/2011/bg-8-1081-2011.pdf (200). Table 2, Appendix Table A2. CC BY 3.0.
- **S28** US National Park Service, Organ Pipe Cactus National Monument, saguaro brochure (PDF created 2016): https://www.npshistory.com/publications/orpi/brochures/saguaro.pdf (200). Public domain.
- **S29** Drezner, T.D. (2003). Saguaro (*Carnegiea gigantea*, Cactaceae) age–height relationships and growth: the development of a general growth curve. *Am. J. Bot.* 90(6): 911–914. https://doi.org/10.3732/ajb.90.6.911. Abstract read via https://api.crossref.org/works/10.3732/ajb.90.6.911 (200) and Europe PMC (PMID 21659186).
- **S30** Davis, T.A. (1971). Why Fibonacci sequence for palm leaf spirals? *Fibonacci Quarterly* 9(3): 237–244. https://www.fq.math.ca/Scanned/9-3/davis1.pdf (200).
- **S31** Jenkins, J.C., Chojnacky, D.C., Heath, L.S. & Birdsey, R.A. (2003). National-scale biomass estimators for United States tree species. *Forest Science* 49(1): 12–35. https://research.fs.usda.gov/download/treesearch/6996.pdf (200). Eq. bm = exp(β0 + β1 ln dbh), Table 4. Biomass only.
- **S32** Assogbadjo, A.E., Kyndt, T., Sinsin, B., Gheysen, G. & Van Damme, P. (2006). Patterns of genetic and morphometric diversity in baobab (*Adansonia digitata*) populations across different climatic zones of Benin (West Africa). *Ann. Bot.* 97(5): 819–830. https://doi.org/10.1093/aob/mcl043 (timed out). Abstract read via the Europe PMC API https://www.ebi.ac.uk/europepmc/webservices/rest/search?query=DOI:10.1093/aob/mcl043&resultType=core&format=json (200). The PMC full text https://pmc.ncbi.nlm.nih.gov/articles/PMC2803429/ returned 403.
- **S33** VicFlora, Royal Botanic Gardens Victoria: *Olea* https://vicflora.rbg.vic.gov.au/flora/taxon/29bc24b8-4e5e-45c4-9f67-17c21a23f956 (opened via WebFetch; source Jeanes 1999, Flora of Victoria 4); *Eucalyptus globulus* https://vicflora.rbg.vic.gov.au/flora/taxon/1dfdd922-7fa3-4fc4-bb6e-96cfa112337a (opened via WebFetch; source Brooker & Slee 1996).
- **S34** Sone, K., Suzuki, A.A., Miyazawa, S., Noguchi, K. & Terashima, I. (2009). Maintenance mechanisms of the pipe model relationship and Leonardo da Vinci's rule in the branching architecture of *Acer rufinerve* trees. *J. Plant Res.* 122: 41–52. https://doi.org/10.1007/s10265-008-0177-5. Abstract read via the Europe PMC API (search DOI:10.1007/s10265-008-0177-5, 200).
