# E3 verification: Grass and ground cover

Verifier pass on `/workspace/specula-research/E3-grass-ground-cover.md` (World Data).
**E3 itself was not edited.** Access date for every check below: **2026-11-16** (Australia/Sydney).

## 1. Summary

| | Count |
|---|---|
| Claim groups / rows examined | 67 |
| **VERIFIED** | 55 |
| **FIX** | 7 |
| **UNVERIFIABLE** (in this pass) | 5 |
| Numeric tokens in E3 tables (for scale; includes years, volume and page nos.) | 921 |
| Researcher’s “about 650–700 sourced values” | Plausible if counting measurement endpoints only; not independently recounted as 650–700 distinct facts |

**Grok CLI:** tried first, as required: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve`. It failed immediately with `Error: No such device or address (os error 6)`. `XAI_API_KEY` is unset. No login was attempted. All checks used WebSearch, WebFetch, curl, Crossref, PMC XML, and `pdftotext` on author/publisher PDFs.

**Coverage (not a sample of the botany or main cost tables):**

- **Checked in full:** every botany table row in §3a–3h except the SM14 compiled-bracken row (SciELO DOI returned HTTP 500; the HTML page timed out). Every JW17, BLH02 and HWJ07 published timing / FPS figure, including hardware and resolution. KUTH24 timings in the GPUOpen preprint. AMD24a blade/patch geometry. AMD24b per-node vertex/triangle budgets. Unity Terrain-details modes and the 1,023 instancing cap. Unreal `GrassVariety.grass_density` units and cull-distance behaviour. Licences for VicFlora, PlantNET, AM21, HR16, WO21, EL17, JIA24, and the KUTH24 preprint notice. Crossref abstracts for WLH07, SWD78, HMH99 and BAO05. HG93 text and Table 1 extrema, via WebFetch of the journal PDF (curl from this box received HTML, not a PDF).
- **Checked against a third-party copy or a search-indexed transcript, not the official slide deck:** all GOT21 and HZD18 numbers. Official HZD PDF still HTTP 403. GDC Vault and YouTube talk pages exist (HTTP 200). Opening GoT figures also appear in the YouTube-indexed talk text.
- **Spot-checked only:** PEL04 (GPU Gems ch. 7) qualitative method wording; no FPS found in the chapter, as claimed. AMD24b “8×4 quads per group” wording (budget numbers were checked). NZPCN disagreement figures. Fan et al. 2015 (existence only; ACM 403, correctly unused).

**Dead or blocked links (curl from this box, 2026-11-16):**

- FNA species pages: HTTP 500 (http) or 403 (https Buchloë). **Pages exist and were read with WebFetch.**
- Official HZD GDC PDF (`ubm-twvideo01.s3.amazonaws.com/.../gilbert_sanders_between_tech_and.pdf`): **403**.
- ACM, Wiley and ASA publisher DOIs: **403** (bot wall). Metadata and abstracts resolved at `api.crossref.org`.
- SciELO DOI 10.1590/1519-6984.22012: **500**.
- HG93 download URL: HTTP 200 but **HTML to curl**; WebFetch returned the paper.
- PlantNET copyright: the URL the file says 404’d is not the live page. `https://plantnet.rbgsyd.nsw.gov.au/copyright.html` returns **200** (see FIX 1).

**§5 Not found and §6 Sources disagree:** both sections are present and, on the whole, honest. Wheat blade length, *Ranunculus* and *Buchloë* heights, blade curvature as a number, and the unused [UNVERIFIED] tiller-density snippets are fair. The pampas 3–8 cm vs to 2 cm disagreement is real and correctly quoted from both floras. Corrections that belong in those sections, and are not there yet, are: the GS18 density range is an August (not June) figure; HMH99’s abstract itself excepts the 20 mm cut; PlantNET and the KUTH24 preprint have redistribution limits that §2 understates.

**Worst problems (the FIXes that matter most):**

1. PlantNET has a live copyright page that limits reproduction to personal / in-house / non-commercial use.
2. GS18 control density range 18.9–29.1 /m² is **August**, not June.
3. HR16 canopy lengths were published with SEs, which were dropped.
4. BLH02 hardware is a “1.7 Ghz Intel Pentium processor”, not “Pentium 4”.
5. The KUTH24 GPUOpen preprint says “personal use / Not for redistribution”, which was not flagged.

---

## 2. Row table

Status is VERIFIED, FIX, or UNVERIFIABLE. Related numbers in one table row were checked together.

| Row | Claim | Status | Note or correction | URL checked |
|---|---|---|---|---|
| 3a *Lolium perenne* FNA | culms to 100 cm; blades usually 10–30 cm, (1)2–4(6) mm; spike 3–30 cm, 5–37 spikelets; Terrell, V24 | VERIFIED | Matches the live FNA page. | http://floranorthamerica.org/Lolium_perenne |
| 3a *L. perenne* HR16 Table 2 | n=6 wild type; canopy 68.33 cm (2012), 67.00 cm (2013); blade 13.07±0.47 / 12.99±0.06 cm; width 0.32±0.02 / 0.33±0.02 cm | **FIX** | Blade figures match Table 2. Canopy was published **with SE**: **68.33±0.33 cm (2012)** and **67.00±1.15 cm (2013)**. n=6 is supported (six plants / six field replicates). Licence CC BY is correct. | https://pmc.ncbi.nlm.nih.gov/articles/PMC4764885/ |
| 3a *Poa pratensis* FNA | culms 5–70(100) cm; cultivars 8–50 cm; flag leaf 1.5–10 cm; cultivar innovation blades usually < 45 cm; widths as stated; panicle 2–15(20) cm; “soft and lax to moderately firm”; p.522 Soreng | VERIFIED | Including the cultivar paragraph. “Usually shorter than 45 cm” is the wording. | http://floranorthamerica.org/Poa_pratensis |
| 3a *Schedonorus arundinaceus* FNA | culms to 1.5(2) m; blades 11–30 cm × 4–12 mm; panicle 10–35 cm; Darbyshire V24 | VERIFIED | | http://floranorthamerica.org/Schedonorus_arundinaceus |
| 3a *Cynodon dactylon* FNA | culms 5–40(50) cm; blades 1–6(16) cm × (1)2–4(5) mm; (2)4–6(9) branches each 2–6 cm; Barkworth V25 | VERIFIED | | http://floranorthamerica.org/Cynodon_dactylon |
| 3a *Cynodon* ‘Tifway 419’ AM21 Table 2 | 13.47 cm sun, 26.13 cm shade, uncompacted | VERIFIED | Table 2, NC (no compaction) columns. | https://pmc.ncbi.nlm.nih.gov/articles/PMC7783808/ |
| 3a *Stenotaphrum secundatum* FNA | culms 10–30 cm; blades 3–15(18) cm × 4–10 mm, blunt tips; panicle 4.5–10 cm; p.561 Allred | VERIFIED | | http://floranorthamerica.org/Stenotaphrum_secundatum |
| 3a *Buchloë dactyloides* FNA | no culm height on page; blades 2–15 cm × 1–2.5 mm; Snow V25 | VERIFIED | Honest dash. | https://floranorthamerica.org/Buchlo%C3%AB_dactyloides |
| 3a *Pennisetum clandestinum* FNA | culms 3–45 cm decumbent; blades 1–15 cm × 1–6 mm; panicle 2–2.7 cm, hidden in sheaths; Wipff V25 | VERIFIED | “Concealed in the sheaths.” | http://floranorthamerica.org/Pennisetum_clandestinum |
| 3b *Avena sativa* FNA | culms 35–180 cm; blades 8–45 cm × 3–14(25) mm; panicle (6)15–40 × 5–15 cm, nodding; spikelets (18)25–32 mm; Baum V24 | VERIFIED | | http://floranorthamerica.org/Avena_sativa |
| 3b *Triticum aestivum* FNA | culms 14–150 cm; width 6–15(20) mm, no blade length; spike (3.5)6–18 cm; awns to 12 cm; club wheat 3.5–6 cm; Morrison V24 | VERIFIED | Club-wheat length is in the discussion, as used. | http://floranorthamerica.org/Triticum_aestivum |
| 3b *Cortaderia selloana* FNA | culms 2–4 m; blades to 2 m × 3–8 cm; ascending, arching; panicle 30–130 cm; p.299 Allred | VERIFIED | “3–8 cm” is printed. The file is right to distrust it and quote VicFlora beside it. | http://floranorthamerica.org/Cortaderia_selloana |
| 3b *C. selloana* VIC | culms to 4.5 m; blades to 1.8 m × to 2 cm; panicle 25–100 cm; Walsh 2021 | VERIFIED | Text CC BY 4.0 confirmed on the copyright page. | https://vicflora.rbg.vic.gov.au/flora/taxon/7204f38d-332b-4e70-8771-65ad2b7e36c0 |
| 3c *Themeda triandra* VIC | culms to c. 1 m; blade to 50 cm × 5 mm; inflorescence commonly drooping, 10–25 cm; racemes 1.5–2.2 cm; awn 4–8 cm; Walsh 1994 | VERIFIED | | https://vicflora.rbg.vic.gov.au/flora/taxon/5c4beccd-3eef-434d-9769-4d187abb91e9 |
| 3c *T. triandra* PN | to c. 1.2 m; width 2–5 mm; inflorescence mostly 10–25 cm; awn 4–6 cm | VERIFIED | Licence: see FIX 1 / source table. | https://plantnet.rbgsyd.nsw.gov.au/cgi-bin/NSWfl.pl?page=nswfl&lvl=sp&name=Themeda~triandra |
| 3c *Poa labillardierei* VIC | culms to 130 cm; panicle to 25 × 15 cm | VERIFIED | | https://vicflora.rbg.vic.gov.au/flora/taxon/b5dd411d-ca1e-4886-a989-7745ffd40afb |
| 3c var. *labillardierei* VIC | blades to 80 cm × 3.5 mm; tussock usually at least half the culm height | VERIFIED | “Tussock usually at least half as high as the culm.” | https://vicflora.rbg.vic.gov.au/flora/taxon/cf4809be-cafe-48f1-9599-bc29cabf1554 |
| 3c *Triodia basedowii* PN | hummock to c. 1 m high, 1.5 m across; inflorescence 7–13 cm | VERIFIED | Source says “1.5 m diam.” | https://plantnet.rbgsyd.nsw.gov.au/cgi-bin/NSWfl.pl?page=nswfl&lvl=sp&name=Triodia~basedowii |
| 3c *T. basedowii* AG | culms 15–130 cm; blades (2)5–25 cm × 0.7 mm, straight, needle-like; panicle (3)6–13(16) cm | VERIFIED | Source: “straight, aciculate, conduplicate.” Aciculate = needle-like. | https://www.ausgrass.org/content/triodia-basedowii |
| 3c *Chionochloa rubra* NZF | culm to 1.5 m; blade to 1 m × 1.2 mm diam.; sheath to 30 cm; inflorescence to 45 cm | VERIFIED | “Acicular junceous” supports needle-like; “rolled” is a mild paraphrase. **Lemma to 6 mm** is on the page. | https://floraseries.landcareresearch.co.nz/taxa/5d2bb3e3-e3f4-446e-ae15-31d24b0bd38d |
| §6 NZF vs NZPCN florets | NZF lemma 6 mm, anthers 5 mm vs NZPCN lemma to 5 mm, anthers to 3.5 mm | UNVERIFIABLE | Lemma 6 mm is on the NZF page. **Anthers 5 mm were not in the fetched NZF text** (page may be truncated). NZPCN species page not re-opened. | NZF URL above; https://www.nzpcn.org.nz/ |
| 3d HG93 text | 3,500 tillers/m² at start; Nui + Tahora, sheep, 9.3 × 9.3 m, 3 reps; hard 4 cm / 14 d or lax 7 cm / 21 d; Sep 1992–Mar 1993; p.134 | VERIFIED | “The initial tiller population estimated over all treatments was 3500 tillers/m².” Journal PDF, pp. 133–136. | https://www.nzgajournal.org.nz/index.php/ProNZGA/article/download/2064/1692/3457 |
| 3d HG93 Table 1 [OCR] | 3,260–12,740 tillers/m²; SE 306–1,636 | VERIFIED | WebFetch OCR of Table 1 shows 3,260 (min) and 12,740 (max) and SEs 306, 466, 1,115, 1,636, 1,248. The [OCR] caveat is fair; the extrema are real. Column-to-date mapping remains messy in the scan. | same |
| 3d WLH07 [ABSTRACT] | 3,689 stay-green and 4,478 normal shoots/m², Nov 2006 | VERIFIED | Exact abstract wording: after the last harvest in November 2006, 3689 and 4478 shoots m⁻². Correctly flagged [ABSTRACT]. | https://doi.org/10.1111/j.1365-2494.2007.00600.x (Crossref abstract) |
| 3d AM21 Table 3 Tifway | stems/100 cm²: sun 258.33 NC, 252.77 C; shade 164.83 NC, 115.27 C | VERIFIED | | https://pmc.ncbi.nlm.nih.gov/articles/PMC7783808/ |
| 3d AM21 paspalum / Sea Isle / range | sun uncompacted 362.27 and 369.00; range 115.27–369.00 ≈ 11,500–36,900 /m² | VERIFIED | Unit conversion is the author’s, labelled as such (115.27 × 100 = 11,527, rounded as “c. 11,500”). Mowing height really is absent (no “mowing” in the paper). | same |
| 3d SWD78 [ABSTRACT] | tiller density rose as height fell 5.1 → 2.5 → 1.3 cm; 18 of 62 cultivars leaf-angle change < 6° | VERIFIED | Abstract: more procumbent at 1.3 and 2.5 than at 5.1 cm; 18 of 62 varied by less than 6°; tiller density increased as cutting height decreased. Correctly [ABSTRACT]. | https://doi.org/10.2134/agronj1978.00021962007000040039x (Crossref) |
| 3d HMH99 [ABSTRACT] | cut at 20–160 mm; tiller density rose as cutting height fell | **FIX** | Heights 20, 40, 80, 120 and 160 mm are right, but the abstract itself says density increased as height fell **“except for the 20-mm treatment where tiller density was initially restricted.”** The qualitative row overstates a clean inverse. | https://doi.org/10.1046/j.1365-2494.1999.00187.x (Crossref) |
| 3d WO21 density 1–9 scale | 1=0–5% … 9=96–100%; COBORU / Polish variety-office method | VERIFIED | Matches Methods verbatim. CC BY correct. | https://pmc.ncbi.nlm.nih.gov/articles/PMC8523543/ |
| 3e *Leucanthemum vulgare* FNA | 10–30(100+) cm; involucre 12–20+ mm across; 13–34+ rays, laminae 12–20(35+) mm; basal blades 12–35(50+) × 8–20(30) mm; p.558 Strother | VERIFIED | | http://floranorthamerica.org/Leucanthemum_vulgare |
| 3e *Ranunculus acris* FNA | height not given; 5 petals 8–11(17) × 7–13 mm; basal blades 1.8–5.2 × 2.7–9.8 cm; Whittemore V3 | VERIFIED | Honest dash. | http://floranorthamerica.org/Ranunculus_acris |
| 3e *Taraxacum officinale* FNA | (1)5–40(60) cm; involucre 14–25 mm; 40–100+ florets; pappus 5–6(8) mm; beak 7–9 mm; leaves (4)5–45 × (0.7)1–10 cm; Brouillet V19 | VERIFIED | | http://floranorthamerica.org/Taraxacum_officinale |
| 3e *Centaurea cyanus* FNA | 20–100 cm; involucre 12–16 mm; 25–35 florets; sterile corollas 20–25 mm | VERIFIED | FNA does not say “across” here (unlike *Leucanthemum*); the file also does not add “across.” | http://floranorthamerica.org/Centaurea_cyanus |
| 3e *Papaver rhoeas* FNA | to 8 dm; petals to 3.5 cm; capsule to 2 cm; leaves to 15 cm | VERIFIED | | http://floranorthamerica.org/Papaver_rhoeas |
| 3e *Trifolium repens* FNA V11 | 10–40 cm; head 1.5–3.5 × 1.5–3.5 cm, 20–40+ flowers; peduncle 1.5–30 cm; leaflets 0.6–4 × 0.4–2.5 cm; petiole 5–20 cm; Vincent & Gillett | VERIFIED | | http://floranorthamerica.org/Trifolium_repens |
| 3e *T. pratense* FNA V11 | 20–70 cm; vars 20–40 / 40–100 / 30–100; head 1.2–7.7 × 0.7–2.2 cm, 75–100+ flowers; leaflets 1.5–5 × 0.7–1.5 cm | VERIFIED | Varietal heights are in the discussion, as used. | http://floranorthamerica.org/Trifolium_pratense |
| 3f *Pteridium* FNA petiole 0.3–3.5 m; no blade length | species-range petiole; no blade length given | VERIFIED | Jacobs & Peck, V2. | http://floranorthamerica.org/Pteridium_aquilinum |
| 3f *Pteridium* pinnule angle | 45–60° (subsp. *latiusculum*, *pseudocaudatum*); c. 90° (*pubescens*) | **FIX** | Angles match the key, but FNA ranks these as **varieties**, not subspecies (they sit inside subsp. *aquilinum*). Correct to “var. *latiusculum*, *pseudocaudatum*” (45°–60°) and “var. *pubescens*” (nearly 90°). | same |
| 3f GS18 Table 1 heights / treatment densities | Control June 93.4 cm, August 172.9 cm; June control 81.4–117.7 cm (2012–17); cut once 67.4/90.9 cm and 16.1/9.8 /m²; cut twice 63.2/83.8 cm and 14.6/8.1 /m²; pulled 74.6/98.3 cm and 20.9/11.8 /m²; Cirencester, 5×5 m × 3 | VERIFIED | Table 1 is 2013–17 means, as stated. June height range 81.4–117.7 is in the text for 2012–17. Plots 5 m × 5 m, three replicates, near Cirencester. | https://rau.repository.guildhe.ac.uk/id/eprint/16088/1/AAB_Grange__Swallow_2018.pdf |
| 3f GS18 control densities | June 22.7 /m²; August 23.0 /m²; **June range 18.9–29.1 /m²** | **FIX** | Table 1 means 22.7 (June) and 23.0 (August) are right. The range **18.9–29.1 fronds m⁻² is the August control range** (“at this time” in the August-density paragraph; “approximately twice those of the re-growth August plant densities”). | same |
| 3f SM14 compiled table | European 1.5–58.8 /m² and 31.5–245 cm; Brazil 4.6–20.4 /m² and 146–321 cm; own data 12.7±1.2 (n=30), 12.8±1.3 (n=20) | UNVERIFIABLE | File already flags this as a compilation not checked against Watt / den Ouden / Le Duc. SciELO DOI HTTP 500; HTML fetch timed out. Not re-checked. | https://doi.org/10.1590/1519-6984.22012 |
| 3g *Phragmites* FNA | culms 1–4 m; 0.5–1.5 cm thick; blades 15–40 × 2–4 cm; panicle 15–35 × 8–20 cm; Allred V25 | VERIFIED | | http://floranorthamerica.org/Phragmites_australis |
| 3g EL17 | up to 5 m Mediterranean; usually 2–3.5 m temperate Europe | VERIFIED | “can reach heights of up to 5 m, while temperate European Phragmites usually has stem heights of 2–3.5 m.” CC BY. | https://pmc.ncbi.nlm.nih.gov/articles/PMC5715336/ |
| 3g JIA24 Table 1 | heights 108.93±40.16, 115.20±17.27, 145.32±34.51, 199.56±54.33 cm; basal 6.09±1.34, 4.99±1.20, 5.73±1.39, 7.41±1.98 mm; L, L(IN), M, H | VERIFIED | All eight means ± SD match Table 1. CC BY. | https://pmc.ncbi.nlm.nih.gov/articles/PMC11097105/ |
| 3g *Typha latifolia* FNA | shoots 150–300 cm; flowering shoots 1–2 cm mid-stem, 3–7 mm near the inflorescence; widest blades 10–23(29) mm fresh; female spike 5–25 cm × 5–8 mm in flower, 24–36 mm in fruit; S.G. Smith V22 | VERIFIED | | http://floranorthamerica.org/Typha_latifolia |
| 3g *Juncus effusus* FNA | 4–13 dm; culms 1–2.5 mm; blade absent; large clumps; Brooks & Clemants V22 | VERIFIED | Diameter is “at top of sheaths” (omitted, minor). “Often large clumps” is in the text. | http://floranorthamerica.org/Juncus_effusus |
| 3h *Polytrichum commune* FNA | stems (2)5–10(70) cm; var. *perigoniale* to 6 cm; leaves 6–8(12) mm; seta 5–9 cm; capsule 3–6 mm; G.L. Smith Merrill V27 | VERIFIED | Var. height is “Plants to 6 cm.” | http://floranorthamerica.org/Polytrichum_commune |
| 3h *Hylocomium splendens* FNA | plants to 20 cm; fronds 20–35(50) mm wide, to 8 mm in arctic forms, tiered; Rohrer V28 | VERIFIED | Source: “as narrow as 8 mm in reduced arctic-alpine forms”; “ascending series of flat, frondose tiers.” “Annual fronds” is inferred, not FNA’s word. | http://floranorthamerica.org/Hylocomium_splendens |
| 3h BAO05 [ABSTRACT] | 105.5±53.3 mm; frequency 23.8±10.3%; cover 1.8±2.5%; density 1,358.9±1,744.1 /m²; biomass 7.3±12.5 g/m² | VERIFIED | All five figures are in the Crossref abstract, with SD. Correctly [ABSTRACT]. Publisher is now Wiley (journal was Springer); still paywalled. | https://doi.org/10.1007/s11284-005-0088-z (Crossref) |
| 4a PEL04 method | 3 intersecting textured quads (“star”); wind options; fade with distance; no timings | UNVERIFIABLE / partial | Chapter exists (HTTP 200). No “fps” or “timing” in the fetched text, so “no timings” is credible. “Star” / fade wording was not fully extracted in this pass. Codecreatures is mentioned. | https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-7-rendering-countless-blades-waving-grass |
| 4a/4b BLH02 shells + Table 1 | GeForce3 32 MB, Pentium **4** 1.7 GHz; shell/polygon/FPS rows as tabulated | **FIX** | **Every Table 1 number matches** (0/512/293.5/130.8 … 64/33,280/6.9/3.3). Hardware should read **“1.7 Ghz Intel Pentium processor”** and “32 Megabyte NVidia Geforce3.” The paper does **not** say Pentium 4. | https://www.cs.ubc.ca/labs/imager/tr/2002/bakay2002a/bakay.2002a.pdf |
| 4a/4b HWJ07 | P4 3.2 GHz, 7900 GT, Shader Model 3; 1024×768, 8×8 patches, 16 slices, iteration depth 4: 140 fps avg; billboards 32×2 slices/patch: 90 fps avg | VERIFIED | Hardware, SM 3.0, 1024×768, depth 4, 140 fps, and “32 × 2 slices per grass patch and 8 × 8 patches … 90 frames per second” all match the TU Wien preprint. **Caveat:** the 140 fps scene is “8 × 8 grass patches, where each patch contains 16 slices **in u and v direction**,” which may mean 16×16 rather than 16 slices total (the denser set is explicitly “32 × 32”). | https://www.cg.tuwien.ac.at/research/publications/2007/Habel_2007_IAG/Habel_2007_IAG-Preprint.pdf |
| 4a/4b JW17 all timings | GTX 780M, i7-4800 2.7 GHz, 32 GB, 1024×768, 8×MSAA, OpenGL 4.5 (p.7); 397,881 blades + 1,000 spheres; Tables 1–3 as printed; helicopter 900,000 blades; demo GitHub | VERIFIED | Checked against the TU Wien author-draft PDF (`pdftotext`). Every FPS, ms, drawn, culled and per-test cull count matches. Hardware and 1024×768 / 8× MSAA are in §7 Results (p.7). Tables are on the following results pages (file says p.8; consistent with the draft layout). GitHub URL is printed in the paper. Quadratic Bézier + stated culling modes match the method text. | https://www.cg.tuwien.ac.at/research/publications/2017/JAHRMANN-2017-RRTG/JAHRMANN-2017-RRTG-draft.pdf |
| 4a/4b GOT21 [MIRROR] | ~1 million considered; ~83,000 drawn; ~2.5 ms; 16 floats; 512×512 ≈ 1 texel / 39 cm; LOD 15 / 7 verts; 8-tile buffer; nearest 3×3 field-asset tiles; “almost a hundred thousand” at 24:01 | UNVERIFIABLE against official slides | Opening figures, 16 floats, 512×512 and ~39 cm appear in the YouTube-indexed talk text (timestamps ~1:00–4:01 match). 15 / 7 vertices are on the third-party slide PDF (`/tmp/e3/got.pdf`, Data Dungeon copy). 8-tile double-buffer and “nearest 3x3 tiles” are in the third-party transcript, not confirmed from official captions. GDC Vault https://gdcvault.com/play/1027214/ and YouTube https://www.youtube.com/watch?v=Ibe1JBF5i5Y exist (HTTP 200). **No official studio-hosted deck was found.** The Data Dungeon / gameindustrylibrary copies are unauthorised third-party archives of a © GDC / Sucker Punch talk. The [MIRROR] flag is correct and should stay. | Vault, YouTube, third-party transcript and slides as cited in E3 §8 |
| 4a HZD18 [MIRROR] | LOD1 20–36 tris (high shader); LOD2 10–18 (high); LOD3 10–18 (low); global wind field ~150 µs; 256×128 textures; AA ≤1 ms at 1080p PS4; 30 fps at 1080p/4K; depth prepass zero overdraw | UNVERIFIABLE in full; partly corroborated | Official PDF still **HTTP 403**. Search-indexed text of that same official PDF, and the docslib copy, confirm the **LOD triangle counts and “around 150 microseconds”** wind-field compute. 256×128, AA 1 ms, 30 fps target and zero-overdraw were not re-read from the official file. docslib is an unauthorised third-party copy of a © GDC / Guerrilla deck. [MIRROR] flag is correct and should stay. | Official: https://ubm-twvideo01.s3.amazonaws.com/o1/vault/gdc2018/presentations/gilbert_sanders_between_tech_and.pdf (403). Copy: https://docslib.org/doc/8059789/the-vegetation-of-horizon-zero-dawn . Vault: https://gdcvault.com/play/1025530/Between-Tech-and-Art-The . YouTube: https://www.youtube.com/watch?v=wavnKZNSYqU |
| 4a AMD24a | quadratic Bézier; 8 verts / 6 tris; ≤32 blades/group; 256 verts, 192 primitives; group size 128; count lerped from MAX to 2 with a fractional last blade, remainder widened; sines (HZD-inspired) + Perlin; no ms on the page | VERIFIED | All geometry and LOD numbers match the 20 Mar 2024 GPUOpen page. “No ms figures” is right. | https://gpuopen.com/learn/mesh_shaders/mesh_shaders-procedural_grass_rendering/ |
| 4b AMD24b budgets | Dense 512 / 65,536 / 49,152; sparse 8 / 1,024 / 512; flower [0] 5 / 402 / 584; flower [1] 1 / 88 / 44 | VERIFIED | Exact table on the 18 Jul 2024 page. Dense thread groups are **16×16×2 = 512** (not simply 16×16). “8×4 quads per group” was not re-located in the fetched text (budget numbers still stand). | https://gpuopen.com/learn/work_graphs_mesh_nodes/work_graphs_mesh_nodes-procedural_generation/ |
| 4b KUTH24 | 79,710 instances in 3.74 ms (abstract, RX 7900 XTX); Fig. 10d 79,674 grass, 3,359 flowers, 1,038 bees, 3 butterfly swarms, 281 mushrooms, 0.39 ms; Fig. 11 overview 3.24/27.74/79,710 vs 3.14/0.62/965 | VERIFIED | All figures match the GPUOpen preprint. Hardware is in the abstract. | https://gpuopen.com/download/Real-Time_Procedural_Generation_with_GPU_Work_Graphs-GPUOpen_preprint.pdf |
| 4b KUTH24 licence note | “© 2024 the owner/author(s); publication rights licensed to ACM” | **FIX** | The preprint also says: **“It is posted here for your personal use. Not for redistribution.”** That no-redistribution term is missing from §2 and from the “nothing else is non-commercial” paragraph. Facts may still be cited; the PDF itself may not be redistributed. | same |
| 4a UNITY | Instanced mesh (recommended), Vertex Lit, Grass mesh, Grass Texture (optional billboard); Grass mesh and Grass Texture move in the wind; instanced details in batches of at most 1,023; Manual 6000.6 | VERIFIED | Page now lives at docs.unity.com (engine 6000.6) after redirect. “Objects are rendered in batches of 1,023 or fewer.” | https://docs.unity3d.com/Manual/terrain-Grass.html |
| 4a UEPY | `grass_density` = instances per 10 square metres; Start/End Cull Distance; cluster culled beyond End Cull Distance | VERIFIED | UE 5.0 Python API page, as cited. | https://dev.epicgames.com/documentation/en-us/unreal-engine/python-api/class/GrassVariety?application_version=5.0 |
| §2 / §5 PlantNET licence | “Copyright page URL I tried returned 404. Treat as © Botanic Gardens of Sydney; facts only.” | **FIX** | Live page: **https://plantnet.rbgsyd.nsw.gov.au/copyright.html** (HTTP 200). Crown copyright (NSW). Reproduction without permission is only for **personal, in-house or non-commercial** use of small excerpts, with acknowledgement. **Commercial use, or storing/transmitting the material, needs written permission** from the Royal Botanic Gardens and Domain Trust. This should be in the non-commercial flags. | https://plantnet.rbgsyd.nsw.gov.au/copyright.html |
| §2 VicFlora licence | Text CC BY 4.0; images mostly CC BY-NC-SA 4.0 | VERIFIED | Exact wording on https://vicflora.rbg.vic.gov.au/pages/copyright | same |
| §2 AM21 licence | CC BY-NC-ND | VERIFIED | PMC licence statement. Correctly flagged. | PMC7783808 |
| §2 FNA licence | © FNA Association; no open licence found; illustrations © Utah State University / FNA; facts only | VERIFIED as stated on the site | WebFetch pages show “Illustration copyright Utah State University” or “Flora of North America Association.” No open-licence statement was found on the species pages. (The source XML lives in a public Bitbucket repo; that repo’s licence was not checked here.) | FNA pages + http://floranorthamerica.org/How_to_Cite (curl 500; site reachable via WebFetch on species pages) |
| §5 Not found / §6 disagreements | Sections present; unused [UNVERIFIED] snippets kept out of tables; pampas, *Themeda*, *Phragmites*, GoT 83k vs ~100k, turf-density scale | VERIFIED as honest, with the GS18 and HMH99 caveats above | Fan 2015 correctly unused (ACM 403, no abstract). Official HZD 403 correctly reported. | — |
| JW17 demo repo | https://github.com/klejah/ResponsiveGrassDemo | VERIFIED | HTTP 200; also printed in the paper. | same |

---

## 3. Source table

| Key | URL | Resolves? | Licence stated correctly? |
|---|---|---|---|
| FNA | http://floranorthamerica.org/<species> (https for *Buchloë*) | **y** via WebFetch; **n** to curl (500/403) | **y** as a conservative facts-only / no-open-licence reading. Illustrations © USU / FNAA confirmed on pages. |
| VIC | https://vicflora.rbg.vic.gov.au/… and /pages/copyright | y (200) | **y** (text CC BY 4.0; images CC BY-NC-SA 4.0) |
| PN | species CGI URLs; copyright at https://plantnet.rbgsyd.nsw.gov.au/copyright.html | Species pages **y**. Copyright: researcher’s 404 is **wrong**; live page **y** | **n** — missing the personal/non-commercial reproduction limit (FIX 1) |
| AG | https://www.ausgrass.org/content/triodia-basedowii | y (200) | Unconfirmed, as stated (not re-investigated) |
| NZF | https://floraseries.landcareresearch.co.nz/taxa/5d2bb3e3-e3f4-446e-ae15-31d24b0bd38d | y (200) | Unconfirmed, as stated |
| NZPCN | https://www.nzpcn.org.nz/ (site root only in the file) | Root **y**; species URL not given | Unconfirmed, as stated |
| HG93 | https://www.nzgajournal.org.nz/index.php/ProNZGA/article/download/2064/1692/3457 | **y** via WebFetch (paper text); curl saw HTML | Fair (“free to read; licence not stated on the PDF”) |
| WLH07 | https://doi.org/10.1111/j.1365-2494.2007.00600.x | Metadata **y** (Crossref); publisher page 403 to curl | y (Wiley, paywalled, abstract only) |
| AM21 | https://pmc.ncbi.nlm.nih.gov/articles/PMC7783808/ | y (200) | **y** (CC BY-NC-ND) |
| HR16 | https://pmc.ncbi.nlm.nih.gov/articles/PMC4764885/ | y (200) | **y** (CC BY) |
| WO21 | https://pmc.ncbi.nlm.nih.gov/articles/PMC8523543/ and doi 10.1038/s41598-021-99859-9 | y (200) | **y** (CC BY) |
| SWD78 | https://doi.org/10.2134/agronj1978.00021962007000040039x | Crossref **y**; publisher 403 | y (ASA/Wiley, paywalled, abstract only) |
| HMH99 | https://doi.org/10.1046/j.1365-2494.1999.00187.x | Crossref **y**; publisher 403 | y (Wiley, paywalled, abstract only) |
| BAO05 | https://doi.org/10.1007/s11284-005-0088-z | Crossref **y**; publisher 403 | Mostly y. File says “Springer, paywalled”; the journal is now on Wiley, still paywalled. |
| GS18 | https://rau.repository.guildhe.ac.uk/id/eprint/16088/1/AAB_Grange__Swallow_2018.pdf | y (200, real PDF) | Unconfirmed / © AAB, as stated (not re-read) |
| SM14 | https://doi.org/10.1590/1519-6984.22012 | **n** here (DOI 500; HTML timed out) | Unconfirmed, as stated (“usually CC BY; not checked”) |
| EL17 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5715336/ | y (200) | **y** (CC BY) |
| JIA24 | https://pmc.ncbi.nlm.nih.gov/articles/PMC11097105/ | y (200) | **y** (CC BY) |
| JW17 | Author draft PDF + doi 10.1145/3023368.3023380 | Draft PDF **y** (200). ACM DOI 403. | y (© ACM; author draft; facts only) |
| GOT21 | Vault / YouTube / third-party transcript and slides | Vault and YouTube **y**. Official deck not found. Mirrors are third-party. | y that it is © GDC/UBM and Sucker Punch, and that copies are [MIRROR]. **The copies are not an authorised official distribution.** Talk terms follow GDC / studio copyright; no open licence. |
| HZD18 | Official S3 PDF + docslib + Vault | Official PDF **n** (403). docslib **y**. Vault page exists. | Same as GOT21: [MIRROR] is correct; docslib is not official. © GDC/Guerrilla. |
| PEL04 | https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-7-rendering-countless-blades-waving-grass | y (200) | y (© NVIDIA/Addison-Wesley; free to read) — not re-read in full |
| BLH02 | https://www.cs.ubc.ca/labs/imager/tr/2002/bakay2002a/bakay.2002a.pdf | y (200, real PDF) | y as “© the authors/Eurographics” at the level stated (not re-litigated) |
| HWJ07 | TU Wien preprint PDF | y (200, real PDF) | y as stated (not re-litigated) |
| AMD24a | https://gpuopen.com/learn/mesh_shaders/mesh_shaders-procedural_grass_rendering/ | y (200) | y (© AMD; free to read). Sample-code licence still unchecked, as stated. |
| AMD24b | https://gpuopen.com/learn/work_graphs_mesh_nodes/work_graphs_mesh_nodes-procedural_generation/ | y (200) | y (© AMD; free to read) |
| KUTH24 | doi 10.1145/3675376 + GPUOpen preprint PDF | Preprint **y** (200, real PDF). ACM 403. | **n / incomplete** — missing “personal use / Not for redistribution” on the preprint (FIX 5) |
| UNITY | https://docs.unity3d.com/Manual/terrain-Grass.html | y (200; redirects to docs.unity.com engine 6000.6) | y (© Unity; docs free to read) |
| UEPY | Unreal Python API GrassVariety 5.0 | y (200) | y (© Epic; docs free to read) |
| Fan 2015 | https://doi.org/10.1145/2699276.2699283 | ACM 403; existence via Crossref not re-proven beyond the 403 target, consistent with “exists, unused” | n/a (no numbers taken) |

---

## 4. Licence flags that should be added to E3 §2

- **PlantNET (PN):** Crown copyright, NSW. Personal / in-house / non-commercial reproduction of small excerpts is allowed with acknowledgement. Commercial use, alteration, storage or transmission needs written permission from the Royal Botanic Gardens and Domain Trust. https://plantnet.rbgsyd.nsw.gov.au/copyright.html (accessed 2026-11-16).
- **KUTH24 preprint:** “© 2024 Copyright held by the owner/author(s). Publication rights licensed to ACM. This is the author’s version of the work. It is posted here for your personal use. **Not for redistribution.**”

AM21 CC BY-NC-ND and VicFlora image CC BY-NC-SA were already flagged correctly.

---

## 5. GDC third-party copies

Neither Ghost of Tsushima (Wohllaib, GDC 2021) nor Horizon Zero Dawn (Sanders, GDC 2018) official slide PDFs were obtained from GDC Vault or the studios in this pass. The official 2018 HZD PDF on UBM S3 still returns 403. The YouTube recordings and Vault catalogue pages exist. Numbers in E3 come from unauthorised third-party copies (docslib; archive.thedatadungeon.com; gameindustrylibrary.com auto-captions). Those copies are **not authorised official distributions**. The talks remain © GDC/UBM and the studios. The file’s [MIRROR] labels are right and should not be promoted to primary.

Where a public YouTube-indexed transcript or the official PDF’s search index independently repeats a figure (GoT: “just over 1 million,” “about 83,000,” “two and a half milliseconds,” 16 floats, 512×512, ~39 cm; HZD: LOD triangle bands and ~150 µs), those particular figures are consistent with the talk, but they are still not a substitute for the official deck.

---

## 6. Fix re-check (2026-11-16, Australia/Sydney)

Re-check of the rows listed under “Fixes after verify (2026-11-16)” at the top of `E3-grass-ground-cover.md`. **E3 itself was not edited in this pass.**

**Version checked:** `E3-grass-ground-cover.md` mtime **2026-11-16 10:17:57 AEDT**, 390 lines, 43,417 bytes. This verify file (before this section) mtime **2026-11-16 10:16:29 AEDT**, 172 lines.

Grok CLI was not required for this re-check. Sources were opened with curl, WebFetch (FNA), Crossref, Europe PMC XML, and `pdftotext`. Access date: **2026-11-16**.

| # | Claim after the fix | Status | Note | URL checked |
|---|---|---|---|---|
| 1 | PlantNET: Crown copyright (NSW); personal / in-house / non-commercial use only; commercial use or altering/storing/transmitting needs written permission from the Royal Botanic Gardens and Domain Trust; live copyright URL | **VERIFIED** | Live page HTTP 200. Wording matches the page. Flag also appears in the non-commercial list and as ref 3. | https://plantnet.rbgsyd.nsw.gov.au/copyright.html |
| 2 | KUTH24 GPUOpen preprint: posted for personal use, not for redistribution | **VERIFIED** | First page of the preprint: “This is the author’s version of the work. It is posted here for your personal use. Not for redistribution.” Also in §2 and the flags list. PDF HTTP 200. | https://gpuopen.com/download/Real-Time_Procedural_Generation_with_GPU_Work_Graphs-GPUOpen_preprint.pdf |
| 3 | HR16 canopy length 68.33 ± 0.33 cm (2012), 67.00 ± 1.15 cm (2013) | **VERIFIED** | *Horticulture Research* Table 2, wild type, mean ± s.e.; n = 6 (six field replicates, one plant each). Blade length/width figures also still match. Licence CC BY 4.0. | https://pmc.ncbi.nlm.nih.gov/articles/PMC4764885/ (Europe PMC XML) |
| 4 | HMH99: tiller density rose as cutting height fell, except the 20 mm treatment, where tiller density was initially restricted | **VERIFIED** | Crossref abstract: “Tiller population density increased with reduction in defoliation height, except for the 20-mm treatment where tiller density was initially restricted.” Heights 20, 40, 80, 120 and 160 mm confirmed. Publisher DOI still 403. | https://api.crossref.org/works/10.1046/j.1365-2494.1999.00187.x |
| 5 | FNA bracken: *latiusculum*, *pseudocaudatum* and *pubescens* are varieties within subsp. *aquilinum*; pinnule angles 45–60° (var. *latiusculum*, var. *pseudocaudatum*) and nearly 90° (var. *pubescens*) | **VERIFIED** | FNA species page (WebFetch; curl HTTP 500/403): “var. *pubescens*, var. *latiusculum*, and var. *pseudocaudatum* are in subsp. *aquilinum*.” Key: pinnules at 45°–60° to the costa vs nearly 90° for var. *pubescens*. | https://floranorthamerica.org/Pteridium_aquilinum |
| 6 | GS18 control density range 18.9–29.1 /m² is an **August** range | **VERIFIED** | After the August-density paragraph: “The range of Control frond density values at this time was between 18.9 to 29.1 fronds m⁻², approximately twice those of the re-growth August plant densities.” Table 1 means (June 22.7, August 23.0) and treatment rows are unchanged and still match. | https://rau.repository.guildhe.ac.uk/id/eprint/16088/1/AAB_Grange__Swallow_2018.pdf |
| 7 | BLH02 hardware: 1.7 GHz Intel Pentium (not Pentium 4), 32 MB NVIDIA GeForce3 | **VERIFIED** | Paper: “The test machine … has a 1.7 Ghz Intel Pentium processor with 512 Megabytes of memory. The video card is a 32 Megabyte NVidia Geforce3.” It does not say Pentium 4. | https://www.cs.ubc.ca/labs/imager/tr/2002/bakay2002a/bakay.2002a.pdf |
| 8 | GOT21 and HZD18 remain **UNVERIFIABLE — corroborated only** | **VERIFIED** as still correctly flagged | Both §4a/4b rows and the §5 notes keep the UNVERIFIABLE / [MIRROR] wording. Official HZD S3 PDF was not re-obtained. | Vault and YouTube URLs below |
| 9 | Ref 21 GDC Vault URL opens | **VERIFIED** | HTTP 200. Page title: “GDC Vault - Between Tech and Art: The Vegetation of 'Horizon Zero Dawn'.” (The GOT Vault URL in ref 20 also opens: “Procedural Grass in 'Ghost of Tsushima'.”) | https://gdcvault.com/play/1025530/Between-Tech-and-Art-The |

**Result:** all nine fixes are in and read as required. No new FIX or UNVERIFIABLE on the changed rows. The earlier FIX and UNVERIFIABLE counts in §1 above are historical (pre-fix).
