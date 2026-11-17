# E3: Grass and ground cover: botany numbers and a survey of real-time grass rendering

Research item E3 of `LIBRARY_RESEARCH_BRIEF.md`. Compiled 2026-11-16 (Australia/Sydney). Every link below was accessed on **2026-11-16**.

## Fixes after verify (2026-11-16)

Applied from `E3-grass-ground-cover.md.verify.md`:

1. **PlantNET licence (§2, non-commercial flags, ref 3):** the live copyright page https://plantnet.rbgsyd.nsw.gov.au/copyright.html (HTTP 200) allows personal, in-house or non-commercial use only; commercial use needs written permission. Now flagged. (Earlier text said the copyright page 404'd; I had tried the wrong URL.)
2. **GS18 bracken density (§3f):** the 18.9–29.1 fronds/m² control range is an **August** range, not June.
3. **HR16 canopy length (§3a):** SEs added: 68.33 ± 0.33 cm (2012), 67.00 ± 1.15 cm (2013).
4. **BLH02 hardware (§4b):** 1.7 GHz Intel Pentium (not Pentium 4), 32 MB NVIDIA GeForce3.
5. **KUTH24 licence (§2, flags):** the GPUOpen preprint is for personal use, not for redistribution.
6. **FNA bracken taxa (§3f):** *latiusculum*, *pseudocaudatum* and *pubescens* are varieties, not subspecies.
7. **HMH99 (§3d):** added the abstract's exception for the 20 mm cut, where tiller density was initially restricted.
8. **GOT21 and HZD18 (§4a, §4b, §7):** now marked **UNVERIFIABLE — corroborated only**, because the third-party copies used are not authorised distributions. The HZD GDC Vault page was added to the references.

## 1. Method

- **The grok CLI pass did not run.** I launched `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve` as the brief asks. The first launch was interrupted. The relaunch exited straight away with "Not signed in … run: grok login --device-code" (`XAI_API_KEY` is not set either). I did not try to sign in, because that needs credentials. So nothing in this file comes from grok.
- **What I did instead:** WebSearch to find sources, then I read each primary source directly:
  - floras through WebFetch;
  - open-access papers as full-text XML from the Europe PMC REST API;
  - paywalled abstracts from the Crossref API;
  - PDFs with `pdftotext`, page-tagged.
  
  All scratch files are in `/tmp/e3`. Nothing was downloaded into any repository, and nothing was posted or sent.
- **Status of each number:** every number below was read on the linked page or PDF unless its row says one of these:
  - **[UNVERIFIED]**: seen only in a search snippet;
  - **[MIRROR]**: read from a third-party copy of the primary source;
  - **[ABSTRACT]**: read from the publisher abstract only;
  - **[OCR]**: scanned table, so column alignment is uncertain.
- **Units:** as published. Floras give ranges as `min–max`, with rare extremes in brackets, for example `(1)2–4(6) mm`, where 1 and 6 are the rare extremes. A flora range is the span seen across specimens. It is not a mean ± SD. Field studies give mean ± SD or SE as stated.
- **Wording:** facts and numbers only. I copied no figures, and no wording beyond short descriptive terms such as "arching".
- **Link check:** I ran `curl -L` on every URL on 2026-11-16:
  - Most returned HTTP 200.
  - FNA pages return 500/403 to curl from this box (a bot filter), but they opened with WebFetch, and I read the numbers there.
  - ACM, Wiley and ASA DOIs return 403 to curl (a bot wall). Their metadata resolves through the Crossref API (`api.crossref.org/works/<DOI>`), which I checked for every DOI cited.
  - SciELO's DOI returns 500 to curl, but resolves in Crossref.
  - The official GDC 2018 HZD PDF returns 403.

## 2. Sources and licences

| Key | Source | What it gives | Licence (plainly) |
|---|---|---|---|
| FNA | Flora of North America Editorial Committee, *Flora of North America North of Mexico*, online at floranorthamerica.org. Volumes: V2 1993, V3 1997, V11 2023, V19 2006, V22 2000, V24 2007, V25 2003, V27 2007, V28 2014 | Species descriptions | © Flora of North America Association. No open licence found. Illustrations © Utah State University / FNA. Numbers used as facts. |
| VIC | VicFlora, Royal Botanic Gardens Victoria (descriptions from Walsh 1994, *Flora of Victoria* vol. 2, or as dated) | Species descriptions | Text **CC BY 4.0**. Images mostly **CC BY-NC-SA 4.0 (non-commercial)**; none used. https://vicflora.rbg.vic.gov.au/pages/copyright |
| PN | PlantNET (NSW Flora Online), Botanic Gardens of Sydney; grass treatments from Jacobs, Whalley & Wheeler 2008, *Grasses of New South Wales* 4th edn | Species descriptions | **Crown copyright (NSW); personal / in-house / non-commercial use only.** Small excerpts may be reproduced for personal, in-house or non-commercial use with acknowledgement; **commercial use (or altering, storing or transmitting the material) needs written permission** from the Royal Botanic Gardens and Domain Trust. https://plantnet.rbgsyd.nsw.gov.au/copyright.html (accessed 2026-11-16). Numbers used as facts only. |
| AG | AusGrass3 (ausgrass.org) | *Triodia basedowii* description | Not confirmed; facts only. |
| NZF | Flora of New Zealand Series online, Manaaki Whenua – Landcare Research (Edgar & Connor 2000, *Flora of NZ* Vol. V) | *Chionochloa rubra* | Not confirmed (About page 404); facts only. |
| NZPCN | NZ Plant Conservation Network species page | *Chionochloa rubra* (used only for a disagreement) | Not confirmed. |
| HG93 | Hernández-Garay, Hodgson & Matthew 1993, *Proc. NZ Grassland Assoc.* 55:133–136 | Ryegrass tiller density under grazing | Journal is free to read; licence not stated on the PDF. |
| WLH07 | Wilkins, Lovatt & Hayes 2007, *Grass Forage Sci.* 62:453–458, doi 10.1111/j.1365-2494.2007.00600.x | Ryegrass shoot density | Wiley, paywalled. Abstract only [ABSTRACT]. |
| AM21 | Al-Mana & Al-Yafrsi 2021, *Saudi J. Biol. Sci.* 28:1133 (PMC7783808) | Bermuda/paspalum stem density and height | **CC BY-NC-ND (non-commercial, no derivatives)** |
| HR16 | *Horticulture Research* 2016, doi 10.1038/hortres.2016.3 (PMC4764885) | Ryegrass leaf blade length/width | **CC BY** |
| WO21 | Wolski et al. 2021, *Sci. Rep.* 11:20592, doi 10.1038/s41598-021-99859-9 (PMC8523543) | Definition of the 1–9 turf density rating scale | **CC BY** |
| SWD78 | Sheffer, Watschke & Duich 1978, *Agron. J.* 70:686–689, doi 10.2134/agronj1978.00021962007000040039x | Kentucky bluegrass mowing height vs leaf angle and tillers | ASA, paywalled. Abstract only [ABSTRACT]. |
| HMH99 | Hernández-Garay, Matthew & Hodgson 1999, *Grass Forage Sci.* 54:347–356, doi 10.1046/j.1365-2494.1999.00187.x | Tiller size/density compensation | Wiley, paywalled. Abstract only [ABSTRACT]. |
| BAO05 | Bao 2005, *Ecol. Res.* 20:701–707, doi 10.1007/s11284-005-0088-z | *Polytrichum formosum* cover, height and density | Springer, paywalled. Abstract only [ABSTRACT]. |
| GS18 | Grange & Swallow 2018, *Aspects of Applied Biology* 139 (Royal Agricultural University repository PDF) | Bracken frond height and density | Repository copy. Licence not stated; © AAB. Facts only. |
| SM14 | Silva Matos, Xavier, Tiberio & Marrs 2014, *Braz. J. Biol.* 74(1):156–165, doi 10.1590/1519-6984.22012 | Bracken frond density and length (compiled table) | SciELO. Licence not confirmed (SciELO journals are usually CC BY; not checked). |
| EL17 | Eller et al. 2017, *Front. Plant Sci.* 8:1833 (PMC5715336) | *Phragmites* height range | **CC BY** |
| JIA24 | Jia et al. 2024, *Front. Plant Sci.* 15 (PMC11097105) | *Phragmites* height and basal diameter, mean ± SD | **CC BY** |
| JW17 | Jahrmann & Wimmer 2017, "Responsive real-time grass rendering for general 3D scenes", I3D '17, doi 10.1145/3023368.3023380 (author draft PDF, TU Wien) | Blade model, LOD, culling, timings | © ACM. Author draft read. Facts only. |
| GOT21 | Wohllaib, E. (Sucker Punch) 2021, "Advanced Graphics Summit: Procedural Grass in 'Ghost of Tsushima'", GDC 2021, GDC Vault | Blade counts, LOD, wind, cost | © GDC/UBM and Sucker Punch. Transcript and slides read from **third-party copies [MIRROR]**. |
| HZD18 | Sanders, G. (Guerrilla) 2018, "Between Tech and Art: The Vegetation of Horizon Zero Dawn", GDC 2018 | Grass LOD triangles, wind | © GDC/Guerrilla. Official PDF returns 403. Read on **docslib mirror [MIRROR]**. |
| PEL04 | Pelzer, K. 2004, "Rendering Countless Blades of Waving Grass", *GPU Gems* ch. 7 (NVIDIA, free online) | Billboard/star-quad method | © NVIDIA/Addison-Wesley; free to read. |
| BLH02 | Bakay, Lalonde & Heidrich 2002, "Real Time Animated Grass", Eurographics 2002 short paper (UBC PDF) | Shell method with FPS table | © the authors/Eurographics. |
| HWJ07 | Habel, Wimmer & Jeschke 2007, "Instant Animated Grass", *Journal of WSCG* 15(1–3):123–128 (TU Wien preprint) | Ray-cast slice method with FPS | © the authors/WSCG. |
| AMD24a | Faber & Meyer, "Procedural grass rendering", AMD GPUOpen, 20 Mar 2024 | Mesh-shader blades | © AMD; free to read. Sample code licence not checked. |
| AMD24b | AMD GPUOpen, "Work graphs mesh nodes – procedural generation", 18 Jul 2024 | Per-patch vertex and triangle budgets | © AMD; free to read. |
| KUTH24 | Kuth, Oberberger, Faber, Baumeister, Chajdas & Meyer 2024, "Real-Time Procedural Generation with GPU Work Graphs", *Proc. ACM CGIT* 7(3), doi 10.1145/3675376 (GPUOpen preprint) | Work-graph grass timings | © 2024 the owner/author(s); publication rights licensed to ACM. The preprint says it is posted **for personal use, not for redistribution**: cite the numbers, do not redistribute the PDF. |
| UNITY | Unity Manual, "Grass and other details" (Terrain, version 6000.6 page) | Engine detail modes | © Unity Technologies; docs free to read. |
| UEPY | Unreal Engine Python API, `unreal.GrassVariety` (UE 5.0) | Landscape Grass parameters and units | © Epic Games; docs free to read. |

**Non-commercial / restricted flags:**
- AM21 is CC BY-NC-ND.
- VicFlora images are CC BY-NC-SA. I used no images, only text.
- **PlantNET (PN): personal / in-house / non-commercial use only; commercial use needs written permission.**
- **KUTH24 GPUOpen preprint: personal use, not for redistribution.**

Nothing else in this file is under a non-commercial or no-redistribution licence as far as I could tell. Unconfirmed licences are listed above.

## 3. Botany tables

Sizes are as published. "—" means the source gives no value; see §5.

### 3a. Lawn and turf grasses

| Species (common) | Plant / culm height | Blade length | Blade width | Blade shape / droop | Seed head | Key | URL |
|---|---|---|---|---|---|---|---|
| *Lolium perenne* (perennial ryegrass) | culms to 100 cm | usually 10–30 cm | (1)2–4(6) mm | — | spike 3–30 cm, 5–37 spikelets | FNA V24 (Terrell) | http://floranorthamerica.org/Lolium_perenne |
| *Lolium perenne*, field-grown wild type, n=6 | canopy length 68.33 ± 0.33 cm (2012), 67.00 ± 1.15 cm (2013) | 13.07 ± 0.47 cm (2012); 12.99 ± 0.06 cm (2013) | 0.32 ± 0.02 cm (2012); 0.33 ± 0.02 cm (2013) | — | — | HR16 Table 2 | https://pmc.ncbi.nlm.nih.gov/articles/PMC4764885/ |
| *Poa pratensis* (Kentucky bluegrass) | culms 5–70(100) cm; cultivars 8–50 cm | flag-leaf blades 1.5–10 cm; cultivar innovation blades usually < 45 cm | cauline 0.4–4.5 mm; intravaginal 0.4–1 mm; cultivars (0.4)1–4 mm | soft and lax to moderately firm | panicle 2–15(20) cm | FNA V24 p.522 (Soreng) | http://floranorthamerica.org/Poa_pratensis |
| *Schedonorus arundinaceus* (tall fescue) | culms to 1.5(2) m | 11–30 cm | 4–12 mm | — | panicle 10–35 cm | FNA V24 (Darbyshire) | http://floranorthamerica.org/Schedonorus_arundinaceus |
| *Cynodon dactylon* (Bermuda) | culms 5–40(50) cm | 1–6(16) cm | (1)2–4(5) mm | — | (2)4–6(9) branches, each 2–6 cm | FNA V25 (Barkworth) | http://floranorthamerica.org/Cynodon_dactylon |
| *Cynodon* hybrid 'Tifway 419', Riyadh field plots | 13.47 cm in sun, 26.13 cm in shade (uncompacted) | — | — | — | — | AM21 Table 2 | https://pmc.ncbi.nlm.nih.gov/articles/PMC7783808/ |
| *Stenotaphrum secundatum* (buffalo / St Augustine) | culms 10–30 cm | 3–15(18) cm | 4–10 mm | blunt tips | panicle 4.5–10 cm | FNA V25 p.561 (Allred) | http://floranorthamerica.org/Stenotaphrum_secundatum |
| *Buchloë dactyloides* (American buffalograss) | — (not on page) | 2–15 cm | 1–2.5 mm | — | — | FNA V25 (Snow) | https://floranorthamerica.org/Buchlo%C3%AB_dactyloides |
| *Pennisetum clandestinum* (kikuyu) | culms 3–45 cm (decumbent) | 1–15 cm | 1–6 mm | — | panicle 2–2.7 cm, hidden in the sheaths | FNA V25 (Wipff) | http://floranorthamerica.org/Pennisetum_clandestinum |

### 3b. Cereals and tall ornamental grasses

| Species | Height | Blade length | Blade width | Shape / droop | Seed head | Key | URL |
|---|---|---|---|---|---|---|---|
| *Avena sativa* (oats) | culms 35–180 cm | 8–45 cm | 3–14(25) mm | — | panicle (6)15–40 × 5–15 cm, nodding; spikelets (18)25–32 mm | FNA V24 (Baum) | http://floranorthamerica.org/Avena_sativa |
| *Triticum aestivum* (wheat) | culms 14–150 cm | — | 6–15(20) mm | — | spike (3.5)6–18 cm; awns to 12 cm; club wheat spike 3.5–6 cm | FNA V24 (Morrison) | http://floranorthamerica.org/Triticum_aestivum |
| *Cortaderia selloana* (pampas) | culms 2–4 m | to 2 m | 3–8 cm (as printed; see §6) | ascending, arching | panicle 30–130 cm | FNA V25 p.299 (Allred) | http://floranorthamerica.org/Cortaderia_selloana |
| *Cortaderia selloana* (pampas) | culms to 4.5 m | to 1.8 m | to 2 cm | — | panicle 25–100 cm | VIC (Walsh 2021) | https://vicflora.rbg.vic.gov.au/flora/taxon/7204f38d-332b-4e70-8771-65ad2b7e36c0 |

### 3c. Tussock, hummock and native grasses

| Species | Height | Blade | Clump / tussock size | Seed head | Key | URL |
|---|---|---|---|---|---|---|
| *Themeda triandra* (kangaroo grass) | culms to c. 1 m | to 50 cm × 5 mm | — | inflorescence 10–25 cm, commonly drooping; racemes 1.5–2.2 cm; awn 4–8 cm | VIC (Walsh 1994) | https://vicflora.rbg.vic.gov.au/flora/taxon/5c4beccd-3eef-434d-9769-4d187abb91e9 |
| *Themeda triandra* | to c. 1.2 m | width 2–5 mm | — | inflorescence mostly 10–25 cm; awn 4–6 cm | PN | https://plantnet.rbgsyd.nsw.gov.au/cgi-bin/NSWfl.pl?page=nswfl&lvl=sp&name=Themeda~triandra |
| *Poa labillardierei* (common tussock grass) | culms to 130 cm | — | — | panicle to 25 × 15 cm | VIC | https://vicflora.rbg.vic.gov.au/flora/taxon/b5dd411d-ca1e-4886-a989-7745ffd40afb |
| *P. labillardierei* var. *labillardierei* | — | to 80 cm × 3.5 mm | tussock usually at least half the culm height | — | VIC | https://vicflora.rbg.vic.gov.au/flora/taxon/cf4809be-cafe-48f1-9599-bc29cabf1554 |
| *Triodia basedowii* (spinifex) | — | — | hummock to c. 1 m high, 1.5 m across | inflorescence 7–13 cm | PN | https://plantnet.rbgsyd.nsw.gov.au/cgi-bin/NSWfl.pl?page=nswfl&lvl=sp&name=Triodia~basedowii |
| *Triodia basedowii* | culms 15–130 cm | (2)5–25 cm × 0.7 mm, straight, needle-like | — | panicle (3)6–13(16) cm | AG | https://www.ausgrass.org/content/triodia-basedowii |
| *Chionochloa rubra* (red tussock, NZ) | culm to 1.5 m | to 1 m long, 1.2 mm in diameter (rolled, needle-like); sheath to 30 cm | — | inflorescence to 45 cm | NZF (Edgar & Connor 2000) | https://floraseries.landcareresearch.co.nz/taxa/5d2bb3e3-e3f4-446e-ae15-31d24b0bd38d |

### 3d. Shoot and tiller density (turf and sward)

| System | Measure | Value | Conditions | Key | URL |
|---|---|---|---|---|---|
| Perennial ryegrass 'Grasslands Nui' + white clover, sheep-grazed, NZ | tillers/m² | 3,500 at the start (text) | Hard grazing to 4 cm every 14 d, or lax to 7 cm every 21 d; plots 9.3 × 9.3 m, 3 replicates; Sep 1992 – Mar 1993 | HG93 p.134 | https://www.nzgajournal.org.nz/index.php/ProNZGA/article/download/2064/1692/3457 |
| Same | tillers/m² | 3,260–12,740 across treatments and dates; SE 306–1,636 **[OCR]** | Table 1, cores | HG93 Table 1 | same |
| Perennial ryegrass forage swards | shoots/m² | 3,689 (stay-green line), 4,478 (normal), Nov 2006 **[ABSTRACT]** | Field swards | WLH07 | https://doi.org/10.1111/j.1365-2494.2007.00600.x |
| Hybrid Bermuda 'Tifway 419', Riyadh | stems per 100 cm² | sun 258.33 (uncompacted), 252.77 (compacted); shade 164.83, 115.27 | Mowing height not found | AM21 Table 3 | https://pmc.ncbi.nlm.nih.gov/articles/PMC7783808/ |
| Seashore paspalum, sun, uncompacted | stems per 100 cm² | 362.27; 'Sea Isle 2000' 369.00 | — | AM21 Table 3 | same |
| All AM21 turfs | stems per 100 cm² | 115.27–369.00 (= c. 11,500–36,900 per m², my unit conversion) | — | AM21 Table 3 | same |
| Kentucky bluegrass, 62 cultivars | qualitative | Tiller density rose as mowing height fell (5.1 → 2.5 → 1.3 cm); 18 of 62 cultivars changed leaf angle by < 6° **[ABSTRACT]**; counts not in abstract | — | SWD78 | https://doi.org/10.2134/agronj1978.00021962007000040039x |
| Ryegrass mini-swards cut at 20, 40, 80, 120 and 160 mm | qualitative | Tiller density rose as cutting height fell, **except the 20 mm treatment, where tiller density was initially restricted** **[ABSTRACT]** | — | HMH99 | https://doi.org/10.1046/j.1365-2494.1999.00187.x |
| Sports turf "density" rating | 1–9 scale = % ground cover | 1 = 0–5 %, 2 = 6–15 %, 3 = 16–25 %, 4 = 26–40 %, 5 = 41–60 %, 6 = 61–75 %, 7 = 76–85 %, 8 = 86–95 %, 9 = 96–100 % | COBORU / Polish variety-office method | WO21 Methods | https://pmc.ncbi.nlm.nih.gov/articles/PMC8523543/ |

### 3e. Meadow flowers

| Species | Plant height | Flower head / petal | Leaf | Key | URL |
|---|---|---|---|---|---|
| *Leucanthemum vulgare* (oxeye daisy) | 10–30(100+) cm | involucre 12–20+ mm across; 13–34+ rays, laminae 12–20(35+) mm | basal blades 12–35(50+) × 8–20(30) mm | FNA V19 p.558 (Strother) | http://floranorthamerica.org/Leucanthemum_vulgare |
| *Ranunculus acris* (meadow buttercup) | — (not given) | 5 petals, 8–11(17) × 7–13 mm | basal blades 1.8–5.2 × 2.7–9.8 cm | FNA V3 (Whittemore) | http://floranorthamerica.org/Ranunculus_acris |
| *Taraxacum officinale* (dandelion) | (1)5–40(60) cm | involucre 14–25 mm; 40–100+ florets; pappus 5–6(8) mm; beak 7–9 mm | (4)5–45 × (0.7)1–10 cm | FNA V19 (Brouillet) | http://floranorthamerica.org/Taraxacum_officinale |
| *Centaurea cyanus* (cornflower) | 20–100 cm | involucre 12–16 mm; 25–35 florets; ray-like sterile corollas 20–25 mm | — | FNA V19 (Keil & Ochsmann) | http://floranorthamerica.org/Centaurea_cyanus |
| *Papaver rhoeas* (corn poppy) | to 8 dm | petals to 3.5 cm; capsule to 2 cm | leaves to 15 cm | FNA V3 | http://floranorthamerica.org/Papaver_rhoeas |
| *Trifolium repens* (white clover) | 10–40 cm | head 1.5–3.5 × 1.5–3.5 cm, 20–40+ flowers; peduncle 1.5–30 cm | leaflets 0.6–4 × 0.4–2.5 cm; petiole 5–20 cm | FNA V11 2023 (Vincent & Gillett) | http://floranorthamerica.org/Trifolium_repens |
| *Trifolium pratense* (red clover) | 20–70 cm (var. *pratense* 20–40, *sativum* 40–100, *americanum* 30–100) | head 1.2–7.7 × 0.7–2.2 cm, 75–100+ flowers | leaflets 1.5–5 × 0.7–1.5 cm | FNA V11 2023 | http://floranorthamerica.org/Trifolium_pratense |

### 3f. Ferns (bracken)

| Item | Value | Conditions | Key | URL |
|---|---|---|---|---|
| *Pteridium aquilinum* petiole length | 0.3–3.5 m | Species range; no blade length given | FNA V2 (Jacobs & Peck) | http://floranorthamerica.org/Pteridium_aquilinum |
| Pinnule angle to costa | 45–60° (var. *latiusculum*, var. *pseudocaudatum*); nearly 90° (var. *pubescens*); FNA ranks these as varieties within subsp. *aquilinum* | — | FNA V2 | same |
| Frond height, untreated control (UK) | June 93.4 cm; August 172.9 cm (mean 2013–17); June control 81.4–117.7 cm across 2012–17 | Cirencester; 5 × 5 m plots × 3 | GS18 Table 1 | https://rau.repository.guildhe.ac.uk/id/eprint/16088/1/AAB_Grange__Swallow_2018.pdf |
| Frond density, untreated control | June 22.7 /m²; August 23.0 /m² (2013–17 means); **August** control range 18.9–29.1 /m² | same | GS18 Table 1 | same |
| After management (June / August height; June / August density) | cut once 67.4 / 90.9 cm and 16.1 / 9.8 /m²; cut twice 63.2 / 83.8 cm and 14.6 / 8.1 /m²; pulled 74.6 / 98.3 cm and 20.9 / 11.8 /m² | same | GS18 Table 1 | same |
| European *P. aquilinum*, compiled from other studies | frond density 1.5–58.8 /m²; frond length 31.5–245 cm (e.g. Watt 1964 Lakenheath 1.5 /m² at 31.5 cm up to 50.25 /m² at 125 cm; den Ouden 2000 open site 15.4 /m² at 245 cm; Le Duc 2000 Sourhope 58.8 /m² at 56.5 cm) | Secondary compilation | SM14 Table 2 | https://doi.org/10.1590/1519-6984.22012 |
| Brazil *P. esculentum* subsp. *arachnoideum* | density 4.6–20.4 /m²; length 146–321 cm; own data 12.7 ± 1.2 /m² (savanna, n=30), 12.8 ± 1.3 /m² (Atlantic forest, n=20) | — | SM14 Tables 2–3 | same |

### 3g. Reeds and rushes

| Species | Height | Stem diameter | Blade | Head | Key | URL |
|---|---|---|---|---|---|---|
| *Phragmites australis* (common reed) | culms 1–4 m | 0.5–1.5 cm | 15–40 × 2–4 cm | panicle 15–35 × 8–20 cm | FNA V25 (Allred) | http://floranorthamerica.org/Phragmites_australis |
| *P. australis*, regional ranges | up to 5 m (Mediterranean); usually 2–3.5 m (temperate Europe) | — | — | — | EL17 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5715336/ |
| *P. australis*, Yangtze estuary tidal flats, mean ± SD | L 108.93 ± 40.16; L(IN) 115.20 ± 17.27; M 145.32 ± 34.51; H 199.56 ± 54.33 cm | basal 6.09 ± 1.34; 4.99 ± 1.20; 5.73 ± 1.39; 7.41 ± 1.98 mm | — | — | JIA24 Table 1 | https://pmc.ncbi.nlm.nih.gov/articles/PMC11097105/ |
| *Typha latifolia* (cattail / bulrush) | shoots 150–300 cm | flowering shoots 1–2 cm thick mid-stem; 3–7 mm near the inflorescence | widest blades 10–23(29) mm (fresh) | female spike 5–25 cm × 5–8 mm in flower, 24–36 mm thick in fruit | FNA V22 (S.G. Smith) | http://floranorthamerica.org/Typha_latifolia |
| *Juncus effusus* (soft rush) | 4–13 dm | culms 1–2.5 mm | none (blade absent) | — (forms large clumps) | FNA V22 (Brooks & Clemants) | http://floranorthamerica.org/Juncus_effusus |

### 3h. Mosses

| Species | Shoot height | Other | Key | URL |
|---|---|---|---|---|
| *Polytrichum commune* | stems (2)5–10(70) cm; var. *perigoniale* to 6 cm | leaves 6–8(12) mm; seta 5–9 cm; capsule 3–6 mm | FNA V27 (G.L. Smith Merrill) | http://floranorthamerica.org/Polytrichum_commune |
| *Hylocomium splendens* (stair-step moss) | plants to 20 cm | annual fronds 20–35(50) mm wide (to 8 mm in arctic forms), tiered | FNA V28 (Rohrer) | http://floranorthamerica.org/Hylocomium_splendens |
| *Polytrichum formosum*, cutover land, E. Tibetan Plateau | 105.5 ± 53.3 mm | frequency 23.8 ± 10.3 %; cover 1.8 ± 2.5 %; density 1,358.9 ± 1,744.1 shoots/m²; biomass 7.3 ± 12.5 g/m² **[ABSTRACT]** | BAO05 | https://doi.org/10.1007/s11284-005-0088-z |

## 4. Survey of real-time grass rendering

### 4a. Method classes

| Class | Example (key) | Representation | Wind | LOD | Notes / cost as stated | URL |
|---|---|---|---|---|---|---|
| Billboards / star quads | PEL04 (2004) | 3 intersecting textured quads ("star"), each texture showing many blades; sorted back to front and alpha-blended | per cluster (CPU, Codecreatures 2002), per vertex (sine), or per object | fade in and out with distance | No timings in the chapter | https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-7-rendering-countless-blades-waving-grass |
| Shells | BLH02 (2002) | stacked semi-transparent shells offset along the surface normal | per-vertex frame index by distance from the wind source | — | FPS table below; fill-rate limited | https://www.cs.ubc.ca/labs/imager/tr/2002/bakay2002a/bakay.2002a.pdf |
| Ray-cast slices in the fragment shader | HWJ07 (2007) | implicit slices composited front to back in a pixel shader; 8×8 patches, 16 slices | noise texture (gusts plus turbulence) | — | 140 fps average vs 90 fps for billboards (below) | https://www.cg.tuwien.ac.at/research/publications/2007/Habel_2007_IAG/Habel_2007_IAG-Preprint.pdf |
| Geometry blades + compute physics | JW17 (2017) | each blade a quadratic Bézier (3 control points), tessellated | recovery spring + gravity + analytic wind waves + sphere collisions | hardware-tessellation LOD; orientation, frustum, distance (n levels) and occlusion culling | Tables below | https://www.cg.tuwien.ac.at/research/publications/2017/JAHRMANN-2017-RRTG/JAHRMANN-2017-RRTG-draft.pdf |
| Procedural compute blades + indirect draw | GOT21 (2021) **[UNVERIFIABLE — corroborated only]** | cubic Bézier blades generated per tile in compute; 16 floats per blade | 2D Perlin noise scrolled along the wind direction, plus sine bobbing phased by a per-blade hash | high LOD 15 vertices, low LOD 7; low-LOD tiles twice the size with the same blade count; high LOD drops 3 of 4 blades before the transition; short grass folds into 2 blades | "just over 1 million" blades considered, about 83,000 drawn, about 2.5 ms end to end **[MIRROR transcript]**; hardware not stated in the talk (game shipped on PS4) | https://gdcvault.com/play/1027214/ |
| Authored mesh cards + vertex wind | HZD18 (2018) **[UNVERIFIABLE — corroborated only]** | grass meshes in 3 LODs | global wind force field (about 150 µs to compute) + layered sines | LOD1 20–36 triangles (high shader); LOD2 10–18 (high); LOD3 10–18 (low shader); animation scaled down and vertices pushed down with distance | depth prepass gives zero overdraw; 256×128 alpha textures; AA at most 1 ms at 1080p on PS4; target 30 fps at 1080p/4K **[MIRROR]** | https://docslib.org/doc/8059789/the-vegetation-of-horizon-zero-dawn |
| Mesh shaders | AMD24a (2024) | quadratic Bézier blade; 8 vertices / 6 triangles per blade; up to 32 blades per mesh-shader group (256 vertices, 192 primitives); group size 128 | sines (inspired by HZD) + Perlin noise over time | blade count lerped from MAX to 2 by distance, with a fractional last blade and the remaining blades widened | No ms figures on the page | https://gpuopen.com/learn/mesh_shaders/mesh_shaders-procedural_grass_rendering/ |
| Work graphs + mesh nodes | AMD24b / KUTH24 (2024) | GPU-generated grass patches (AMD24a shader) plus flowers, insects and mushrooms | as AMD24a | dense tile 16×16 patches vs sparse 1 quad per patch | Tables below; Radeon RX 7900 XTX | https://gpuopen.com/learn/work_graphs_mesh_nodes/work_graphs_mesh_nodes-procedural_generation/ |
| Engine: Unity Terrain details | UNITY | Instanced mesh (recommended), Vertex Lit mesh, Grass mesh, Grass Texture (quads, optional billboard) | Grass mesh and Grass Texture modes move in the wind | — | Instanced details are drawn in batches of at most 1,023 | https://docs.unity3d.com/Manual/terrain-Grass.html |
| Engine: Unreal Landscape Grass | UEPY | static-mesh instances scattered on a landscape | (material-driven; not on the page) | Start/End Cull Distance fade; whole cluster culled beyond End Cull Distance | `grass_density` is defined as **instances per 10 square metres** | https://dev.epicgames.com/documentation/en-us/unreal-engine/python-api/class/GrassVariety?application_version=5.0 |

### 4b. Costs as published

**JW17**
- Test system: GTX 780M, i7-4800 at 2.7 GHz, 32 GB RAM, 1024×768, 8× MSAA, OpenGL 4.5 (p.7).
- Nature scene: 397,881 blades and 1,000 collision spheres (p.7).
- Table 1 (p.8):

| Nature scene | All features | Collision off | Culling off |
|---|---|---|---|
| FPS | 123 | 129 | 78 |
| Frame time (ms) | 8.130 | 7.742 | 12.821 |
| Blades drawn | 43,128 | 43,128 | 168,333 |
| Blades culled | 125,205 | 125,205 | 0 |
| Physics (ms) | 0.547 | 0.041 | 0.519 |
| Visibility (ms) | 1.401 | 1.392 | 2.375 |
| Rendering (ms) | 2.057 | 2.082 | 3.872 |

- JW17 Table 2 (p.8), blades culled by each test: orientation 44,695; frustum 79,533; distance 46,965; occlusion 6,025.
- Helicopter scene: 900,000 blades (p.8). Table 3:

| Helicopter scene | All features | Collision off | Culling off |
|---|---|---|---|
| FPS | 56 | 56 | 35 |
| Frame time (ms) | 17.860 | 17.692 | 28.624 |
| Blades drawn | 165,135 | 165,135 | 503,382 |
| Blades culled | 338,247 | 338,247 | 0 |
| Physics (ms) | 1.421 | 1.372 | 1.570 |
| Visibility (ms) | 6.817 | 6.792 | 8.142 |
| Rendering (ms) | 5.471 | 5.398 | 9.149 |

Demo code: https://github.com/klejah/ResponsiveGrassDemo

**BLH02** Table 1 (32 MB NVIDIA GeForce3, 1.7 GHz Intel Pentium; the paper does not say Pentium 4):

| Shells | Polygons | FPS 640×480 | FPS 1024×768 |
|---|---|---|---|
| 0 | 512 | 293.5 | 130.8 |
| 8 | 4,608 | 47.0 | 22.8 |
| 16 | 8,704 | 25.6 | 12.4 |
| 32 | 16,896 | 13.4 | 6.5 |
| 64 | 33,280 | 6.9 | 3.3 |

**HWJ07**
- Hardware: Pentium 4 3.2 GHz, GeForce 7900 GT, Shader Model 3.
- 1024×768, 8×8 patches with 16 slices, iteration depth 4: 140 fps average.
- Billboard comparison (32×2 slices per patch): 90 fps average.

**GOT21** (talk timestamps from the third-party transcript **[MIRROR]**; **UNVERIFIABLE against an authorised copy — corroborated only**)

| Item | Value | Timestamp / slide |
|---|---|---|
| Blades considered per frame | "just over 1 million" | ~1:00–2:02 |
| Blades rendered | about 83,000 (and "almost a hundred thousand" at 24:01) | ~1:00–2:02; 24:01 |
| End-to-end GPU cost | about 2.5 ms | ~1:00–2:02 |
| Data per blade | 16 floats | 4:01; slide 10 |
| Placement texture | 512×512, about 1 texel per 39 cm | 3:02 |
| Vertices per blade | high LOD 15, low LOD 7 | slides 20, 23 |
| Tile buffering | buffer of 8 tiles: 4 computed while 4 draw | 7:00 |
| Field-asset tiles kept | nearest 3×3 | 18:00 |

**AMD24b** per-patch budgets (work-graph mesh nodes):

| Node | Thread groups | Vertices | Triangles |
|---|---|---|---|
| Dense grass (DetailedTile, 16×16 patches) | 512 | 65,536 | 49,152 |
| Sparse grass (1 quad per patch, 8×4 quads per group) | 8 | 1,024 | 512 |
| Flower patch [0] | 5 | 402 | 584 |
| Flower patch [1] | 1 | 88 | 44 |

**KUTH24** (Radeon RX 7900 XTX, GPU timers; GPUOpen preprint)
- Whole scene augmentation: 79,710 instances in 3.74 ms (abstract).
- High-density clutter test (Fig. 10d): 79,674 grass patches, 3,359 flowers, 1,038 bee swarms, 3 butterfly swarms and 281 mushroom patches; generating the draw list takes 0.39 ms.
- Fig. 11, overview view:
  - baseline: generation 3.24 ms, render 27.74 ms, 79,710 draw calls;
  - with coalescing: 3.14 ms, 0.62 ms, 965 draw calls.

## 5. Not found

**Botany**
- **Blade curvature or droop as a number** (radius or angle) for any species. Floras use words only ("lax", "arching", "drooping"). The only angle figures are SWD78's leaf-angle change (abstract, no per-cultivar values) and the bracken pinnule angles.
- **Wheat blade length** (FNA gives width only).
- **Plant heights not given:** *Ranunculus acris* (FNA V3) and the *Buchloë* culm height (not on the FNA page).
- **Tiller density for Kentucky bluegrass and tall fescue turf**, as counts from a source I could open.
  - A Kentucky bluegrass figure of 6,440 tillers/m² (Can. J. Plant Sci., paper P04-121) could not be opened (403). **[UNVERIFIED]**, not used in the tables.
  - Blue fescue 6.0–6.6 × 10⁴ shoots/m² (HortScience 44(5):1444, 2009) could not be read. **[UNVERIFIED]**, not used.
- **Wild meadow tiller density** (mixed-species hay meadow, shoots/m²): no primary figure found.
- **Typical clump or tussock diameter** for *Themeda*, *Poa labillardierei*, *Chionochloa*, pampas and *Juncus*. Only *Triodia basedowii* has a hummock size.
- **Seed-head counts per m²** for any grass.
- **Moss cushion height and cover for cushion mosses** (e.g. *Leucobryum*, *Grimmia*, Sphagnum hummocks). Only *Polytrichum* (shoot height) and *Hylocomium* were found.
- **Bracken blade (lamina) length** separately from the petiole.
- ***Phragmites* shoot density**: Tóth & Szabó 2012, *Ann. Limnol.* 48:241–251 (Lake Balaton) appears in search snippets with 88, 49 and 34 shoots/m², and with 295 cm tall / 7.5 mm thick vs 141 cm / 3.5 mm. **[UNVERIFIED]**: the PDF was blocked, so these are not used in the tables.
- **Marrs & Watt 2006**, *J. Ecol.* 94:1272–1321 (Biological Flora of Pteridium): the paper exists, but I could not open its frond numbers (reported elsewhere as 1–80 fronds/m², maximum 4.4 m). **[UNVERIFIED]**.

**Rendering**
- **Fan, Li, Hillesland & Sheng 2015**, "Simulation and rendering for millions of grass blades", I3D '15 pp. 55–60, doi 10.1145/2699276.2699283. The paper exists (Crossref), but the full text is paywalled (ACM 403) and Crossref has no abstract. No numbers taken.
- **Unreal Engine 5.7 "Nanite Foliage"** (Assemblies, Voxels, Skinning; Experimental): the documentation page returned 404 to WebFetch. The description rests only on search summaries, and no numbers were taken.
- **Far Cry 5** (GDC 2018, terrain/vegetation), **Boulanger et al. 2009** and the **Outerra 2012 grass blog** (cited on GoT slide 51): not checked within this pass.
- **Ghost of Tsushima:** I did not get the hardware, the per-pass ms split or the exact blade count from the official slides. The official slide deck was not reached, and the third-party PDF copy has many placeholder slides.
- **HZD:** no blade or instance counts and no grass ms cost.
- **AMD mesh-shader grass:** no ms timings on the page.
- **AM21 mowing height:** not found in the paper.

## 6. Where sources disagree

| Topic | Source A | Source B | Comment |
|---|---|---|---|
| Pampas blade width | FNA: "3–8 cm" | VIC: to 2 cm | FNA's figure may be a misprint for mm or cm; widths of 3–8 cm look implausible for this species. I quote both as printed. |
| Pampas culm height | FNA 2–4 m | VIC to 4.5 m | — |
| *Themeda triandra* height | VIC to c. 1 m | PN to c. 1.2 m | — |
| *Themeda* awn | VIC 4–8 cm | PN 4–6 cm | — |
| *Themeda* blade width | VIC to 5 mm | PN 2–5 mm | Consistent at the maximum. |
| *Chionochloa rubra* florets | NZF: lemma 6 mm, anthers 5 mm | NZPCN: lemma to 5 mm, anthers to 3.5 mm | — |
| *Phragmites* height | FNA 1–4 m | EL17 up to 5 m (Mediterranean); JIA24 mean 1.09–2.00 m on tidal flats | Region and habitat differ. |
| GoT blades drawn | about 83,000 (opening of the talk) | "almost a hundred thousand" (24:01) | Probably different scenes or rounding; both are from auto-captions. |
| Meaning of "turf density" | Counts (tillers or shoots per m²: HG93, WLH07, AM21) | 1–9 visual rating of % cover (WO21; HR16 also uses a 1–9 rating) | Not interchangeable. A web-search summary that read HR16's rating as "30,000–50,000 shoots/m²" is wrong, and I did not use it. |
| Ryegrass density scale | HG93: 3,260–12,740 tillers/m² (grazed) | WLH07: 3,689–4,478 shoots/m² (forage swards) | Grazed or mown swards are much denser than taller forage swards. |

## 7. Numbers not verified against the primary source

- **[MIRROR] GOT21 — UNVERIFIABLE, corroborated only:** all numbers come from a third-party auto-caption transcript (gameindustrylibrary.com) and a third-party slide PDF (archive.thedatadungeon.com). The GDC Vault talk page and the YouTube video exist, and the speaker's name was confirmed on the Vault page. I could not read YouTube captions directly (bot check).
- **[MIRROR] HZD18 — UNVERIFIABLE, corroborated only:** all numbers come from the docslib.org copy, because the official GDC S3 PDF returned 403. Search-indexed text of the official PDF corroborates the LOD triangle counts and the ~150 µs wind-field cost; the other HZD figures rest on the docslib copy alone.
- **[ABSTRACT] WLH07, SWD78, HMH99, BAO05:** taken from publisher abstracts (via Crossref), not the full text.
- **[OCR] HG93 Table 1:** the range 3,260–12,740 comes from OCR of a scanned table. The column-to-date mapping is uncertain. The 3,500 figure in the text was read clearly.
- **SM14:** this is a compilation of older studies (Watt 1964, den Ouden 2000, Le Duc 2000). I did not check those originals.
- **[UNVERIFIED]** figures listed in §5 are excluded from the tables.

## 8. References (all accessed 2026-11-16)

**Floras**

1. FNA: Flora of North America Editorial Committee (eds.) 1993+. *Flora of North America North of Mexico*. Online species pages at `http://floranorthamerica.org/<Genus_species>`. Citation and volume years: http://floranorthamerica.org/How_to_Cite. Pages used:
   - Lolium_perenne, Poa_pratensis, Schedonorus_arundinaceus, Cynodon_dactylon, Stenotaphrum_secundatum, Pennisetum_clandestinum;
   - Cortaderia_selloana, Avena_sativa, Triticum_aestivum, Phragmites_australis, Typha_latifolia, Juncus_effusus;
   - Pteridium_aquilinum, Polytrichum_commune, Hylocomium_splendens;
   - Leucanthemum_vulgare, Ranunculus_acris, Taraxacum_officinale, Centaurea_cyanus, Papaver_rhoeas, Trifolium_repens, Trifolium_pratense;
   - https://floranorthamerica.org/Buchlo%C3%AB_dactyloides.
   
   These opened via WebFetch; box curl gets 500/403.
2. VicFlora, Royal Botanic Gardens Victoria. Licence: https://vicflora.rbg.vic.gov.au/pages/copyright. Pages:
   - *Themeda triandra*: https://vicflora.rbg.vic.gov.au/flora/taxon/5c4beccd-3eef-434d-9769-4d187abb91e9
   - *Poa labillardierei*: https://vicflora.rbg.vic.gov.au/flora/taxon/b5dd411d-ca1e-4886-a989-7745ffd40afb
   - var. *labillardierei*: https://vicflora.rbg.vic.gov.au/flora/taxon/cf4809be-cafe-48f1-9599-bc29cabf1554
   - *Cortaderia selloana*: https://vicflora.rbg.vic.gov.au/flora/taxon/7204f38d-332b-4e70-8771-65ad2b7e36c0
3. PlantNET (NSW Flora Online), Botanic Gardens of Sydney (copyright terms: https://plantnet.rbgsyd.nsw.gov.au/copyright.html):
   - https://plantnet.rbgsyd.nsw.gov.au/cgi-bin/NSWfl.pl?page=nswfl&lvl=sp&name=Triodia~basedowii
   - https://plantnet.rbgsyd.nsw.gov.au/cgi-bin/NSWfl.pl?page=nswfl&lvl=sp&name=Themeda~triandra
4. AusGrass3, *Triodia basedowii*: https://www.ausgrass.org/content/triodia-basedowii
5. Edgar, E. & Connor, H.E. 2000. *Flora of New Zealand* Vol. V (Grasses), online Flora of NZ Series, *Chionochloa rubra*: https://floraseries.landcareresearch.co.nz/taxa/5d2bb3e3-e3f4-446e-ae15-31d24b0bd38d
6. NZ Plant Conservation Network, *Chionochloa rubra* species page (https://www.nzpcn.org.nz/, species search). Used only for the disagreement row.

**Grass and turf studies**

7. Hernández-Garay, A., Hodgson, J. & Matthew, C. 1993. *Proc. NZ Grassland Assoc.* 55:133–136. https://www.nzgajournal.org.nz/index.php/ProNZGA/article/download/2064/1692/3457
8. Wilkins, P.W., Lovatt, J.A. & Hayes, R.C. 2007. "Effect of a stay-green locus on the dry-matter yield, nitrogen yield and shoot density of …". *Grass Forage Sci.* 62:453–458. https://doi.org/10.1111/j.1365-2494.2007.00600.x
9. Al-Mana, F.A. & Al-Yafrsi, M.A. 2021. *Saudi J. Biol. Sci.* 28:1133 ff. (CC BY-NC-ND). https://pmc.ncbi.nlm.nih.gov/articles/PMC7783808/
10. *Horticulture Research* 2016, doi 10.1038/hortres.2016.3 (CC BY). https://pmc.ncbi.nlm.nih.gov/articles/PMC4764885/
11. Wolski, K. et al. 2021. "The influence of the grass mixture composition on the quality and suitability for football pitches." *Sci. Rep.* 11:20592 (CC BY). https://doi.org/10.1038/s41598-021-99859-9 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC8523543/
12. Sheffer, K.M., Watschke, T.L. & Duich, J.M. 1978. "Effect of mowing height on leaf angle, leaf number, and tiller density of 62 Kentucky bluegrasses." *Agron. J.* 70:686–689. https://doi.org/10.2134/agronj1978.00021962007000040039x
13. Hernández-Garay, A., Matthew, C. & Hodgson, J. 1999. "Tiller size/density compensation in perennial ryegrass miniature swards …". *Grass Forage Sci.* 54:347–356. https://doi.org/10.1046/j.1365-2494.1999.00187.x

**Moss, bracken and reed studies**

14. Bao, W. 2005. "Structural features of *Polytrichum formosum* …". *Ecol. Res.* 20:701–707. https://doi.org/10.1007/s11284-005-0088-z
15. Grange, I. & Swallow, B. 2018. *Aspects of Applied Biology* 139. https://rau.repository.guildhe.ac.uk/id/eprint/16088/1/AAB_Grange__Swallow_2018.pdf
16. Silva Matos, D.M., Xavier, R.O., Tiberio, F.C.S. & Marrs, R.H. 2014. "A comparative study of resource allocation in *Pteridium* in different Brazilian ecosystems …". *Braz. J. Biol.* 74(1):156–165. https://doi.org/10.1590/1519-6984.22012
17. Eller, F. et al. 2017. *Front. Plant Sci.* 8:1833 (CC BY). https://pmc.ncbi.nlm.nih.gov/articles/PMC5715336/
18. Jia et al. 2024. *Front. Plant Sci.* 15 (CC BY). https://pmc.ncbi.nlm.nih.gov/articles/PMC11097105/

**Rendering**

19. Jahrmann, K. & Wimmer, M. 2017. "Responsive real-time grass rendering for general 3D scenes." I3D '17, pp. 1–10.
    - DOI: https://doi.org/10.1145/3023368.3023380
    - Author draft: https://www.cg.tuwien.ac.at/research/publications/2017/JAHRMANN-2017-RRTG/JAHRMANN-2017-RRTG-draft.pdf
    - Code: https://github.com/klejah/ResponsiveGrassDemo
20. Wohllaib, E. 2021. "Advanced Graphics Summit: Procedural Grass in 'Ghost of Tsushima'." GDC 2021.
    - GDC Vault: https://gdcvault.com/play/1027214/
    - Video: https://www.youtube.com/watch?v=Ibe1JBF5i5Y
    - Third-party copies used: transcript at gameindustrylibrary.com and slides at archive.thedatadungeon.com **[MIRROR]**.
21. Sanders, G. 2018. "Between Tech and Art: The Vegetation of Horizon Zero Dawn." GDC 2018.
    - Official PDF (returned 403): https://ubm-twvideo01.s3.amazonaws.com/o1/vault/gdc2018/presentations/gilbert_sanders_between_tech_and.pdf
    - Copy read: https://docslib.org/doc/8059789/the-vegetation-of-horizon-zero-dawn **[MIRROR]**
   - GDC Vault: https://gdcvault.com/play/1025530/Between-Tech-and-Art-The
22. Pelzer, K. 2004. "Rendering Countless Blades of Waving Grass." *GPU Gems*, ch. 7. https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-7-rendering-countless-blades-waving-grass
23. Bakay, B., Lalonde, P. & Heidrich, W. 2002. "Real Time Animated Grass." Eurographics 2002 short paper. https://www.cs.ubc.ca/labs/imager/tr/2002/bakay2002a/bakay.2002a.pdf
24. Habel, R., Wimmer, M. & Jeschke, S. 2007. "Instant Animated Grass." *Journal of WSCG* 15(1–3):123–128.
    - Publication page: https://www.cg.tuwien.ac.at/research/publications/2007/Habel_2007_IAG/
    - Preprint: https://www.cg.tuwien.ac.at/research/publications/2007/Habel_2007_IAG/Habel_2007_IAG-Preprint.pdf
25. Faber, C. & Meyer, Q. 2024 (20 Mar). "Procedural grass rendering." AMD GPUOpen. https://gpuopen.com/learn/mesh_shaders/mesh_shaders-procedural_grass_rendering/
26. AMD GPUOpen 2024 (18 Jul). "Work graphs mesh nodes – procedural generation." https://gpuopen.com/learn/work_graphs_mesh_nodes/work_graphs_mesh_nodes-procedural_generation/
27. Kuth, B., Oberberger, M., Faber, C., Baumeister, D., Chajdas, M. & Meyer, Q. 2024. "Real-Time Procedural Generation with GPU Work Graphs." *Proc. ACM Comput. Graph. Interact. Tech.* 7(3).
    - DOI: https://doi.org/10.1145/3675376
    - Preprint: https://gpuopen.com/download/Real-Time_Procedural_Generation_with_GPU_Work_Graphs-GPUOpen_preprint.pdf
28. Unity Technologies. Unity Manual, "Grass and other details" (6000.6). https://docs.unity3d.com/Manual/terrain-Grass.html
29. Epic Games. Unreal Engine Python API, `unreal.GrassVariety` (5.0). https://dev.epicgames.com/documentation/en-us/unreal-engine/python-api/class/GrassVariety?application_version=5.0
30. Fan, Z., Li, H., Hillesland, K. & Sheng, B. 2015. "Simulation and rendering for millions of grass blades." I3D '15, pp. 55–60. https://doi.org/10.1145/2699276.2699283 (existence only; see §5)
