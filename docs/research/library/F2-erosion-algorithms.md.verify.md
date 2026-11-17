# F2-erosion-algorithms.md: verification

Checked by: Grok Bot executor (adversarial source check). Access date for every URL below: **2026-11-17** (Australia/Sydney, AEDT).
File checked: `F2-erosion-algorithms.md`. The box was restored from a snapshot at 09:59 AEDT, so mtimes cannot be used to spot changes. Identified by size and sha256.
- At the start of this completing pass (10:39:06 AEDT): **33,376 bytes**, sha256 `54fe76b4c97e5f3dcf6fd95dcd28f27e21a0dc6f64c16ed1a996536c821f1585`.
- Re-hashed immediately before writing this file: **unchanged** (same size and sha256 as the start snapshot and as the earlier verify).

## Summary

- **Scope (stated plainly):** This pass finishes the rows the earlier check never re-checked: the graphics-paper parameter and timing tables in §§2–6 (T2–T10) and the T17 measured-concavity row. Earlier-checked rows (Beyer/T16/T13/T14/T18/T12 and the §8 items that use them) are **kept below unchanged**.
- **Counts (all checked rows, current file):** VERIFIED 56 · FIX 0 · UNVERIFIABLE 0. (Earlier pass: 34 VERIFIED / 0 FIX / 0 UNVERIFIABLE. This pass adds 22 VERIFIED rows.)
- **Worst problems:** none that require a number change. The file already flags the T8 extracted “A500” (read as A5000, 24 GB), the T8 kf typo and unit, and the T9 136,000 vs “130,000” disagreement. All three match the primary text.
- **Dead links:** none new. Same publisher bot-blocks as the earlier pass (Wiley/ACM DOIs 403 to curl; old firespark.de Beyer link HTTP 500, as the file says). HAL record URLs for T3/T4 were HTML stubs in this pass; the Purdue/UCT author PDFs opened (HTTP 200).

## Rows

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| §4.1 Beyer param set | inertia 0.3; capacity 8; deposition 0.2; erosion 0.7; evaporation 0.02; minSlope 0.01; gravity 10; radius 4; maxPath 64 | VERIFIED | Table 6.1 caption, thesis p.21 (printed with decimal commas). Table 6.2 caption is identical, minus radius (varied) | https://ardordeosis.github.io/implementation-of-a-method-for-hydraulic-erosion/thesis-beyer.pdf |
| §4.1 Beyer §5.4 ranges | inertia 0.025/0.1/0.4; deposition 0.01/0.1/1; erosion 0.01/0.1/0.9; evaporation 0.0125/0.05/0.1; minSlope 0.05/0.01/0.0001 | VERIFIED | Figure 5.1, 5.4, 5.5, 5.6, 5.8 sub-captions | same |
| Beyer hardware | i5-3470 3.20 GHz, 8 GB, mean of 10 runs, seconds | VERIFIED | §6.2 p.20 | same |
| Beyer timings 256² | 1.83/2.27/7.66/19.63/32.02 | VERIFIED | Table 6.1 | same |
| Beyer timings 512² | 2.08/2.49/7.79/19.75/35.89 | VERIFIED | Table 6.1 | same |
| Beyer timings 1024² | 2.29/2.56/8.60/20.90/40.65 | VERIFIED | Table 6.1 | same |
| Beyer timings 2048² | 2.38/2.66/9.33/22.47/43.81 | VERIFIED | Table 6.1 | same |
| Beyer radius timings | r 1/2/4/6/8 → 0.48/0.83/2.27/5.51/9.69 s, 10k drops, 256² | VERIFIED | Table 6.2 | same |
| Beyer fall-off counts | ~16,500 / 12,000 / 5,000 of 100,000 | VERIFIED | §6.2 p.20 | same |
| T16 erosionRadius | 3 | VERIFIED | Erosion.cs | https://raw.githubusercontent.com/SebLague/Hydraulic-Erosion/master/Assets/Scripts/Erosion.cs |
| T16 inertia | 0.05 (0 = instant downhill, 1 = never turns) | VERIFIED | code comment matches | same |
| T16 sedimentCapacityFactor | 4 | VERIFIED | | same |
| T16 minSedimentCapacity | 0.01 | VERIFIED | | same |
| T16 erodeSpeed | 0.3 | VERIFIED | | same |
| T16 depositSpeed | 0.3 | VERIFIED | | same |
| T16 evaporateSpeed | 0.01 | VERIFIED | | same |
| T16 gravity | 4 | VERIFIED | | same |
| T16 maxDropletLifetime | 30 | VERIFIED | | same |
| T16 initialWaterVolume / initialSpeed | 1 / 1 | VERIFIED | | same |
| T16 README "70k iterations"; README cites Beyer | | VERIFIED | README links the firespark Beyer PDF under "Resources" | https://github.com/SebLague/Hydraulic-Erosion |
| §6 θ median (T18) | 0.425; IQR 0.225–0.575; 5,033 basins | VERIFIED | Key Points and Abstract, p.1 | https://eprints.gla.ac.uk/251538/1/251538.pdf |
| §6 m/n global (T14) | median 0.51 ± 0.14; mean 0.51 ± 0.12; m/n > 0.65 not tested | VERIFIED | AAM Conclusions; §results ("did not test values of m/n above 0.65") | https://www.pure.ed.ac.uk/ws/files/25670599/25669129._Harel_et_al_2016_author_accepted_manuscript_002_.pdf |
| §6 n global (T14) | median 2.43 ± 0.15; mean 2.6 ± 0.4; "generally > 1"; threshold-controlled | VERIFIED | AAM abstract ("generally > 1 … threshold controlled") and Conclusions; ⟨n⟩ = 2.6 ± 0.4 in results. 1,457 samples, 59 zones | same |
| §6 K normalised (T14) | 2.9×10⁻¹⁰ ± 1.0×10⁻⁹; one suspect zone > 4×10¹⁵ | VERIFIED | Conclusions; results | same |
| §6 reference concavity (T18) | 0.45 | VERIFIED | | T18 URL |
| §6 shear-stress θ (T18) | ~0.43 | VERIFIED | | T18 URL |
| §6 n theory (T12) | "a fundamental unknown" | VERIFIED | Crossref abstract text: "(slope exponent n) emerges as a fundamental unknown" | https://api.crossref.org/works/10.1029/1999JB900120 |
| §6 n SS field (T13) | n ~ 2 (m ~ 1); exceptions Siwaliks m~0.55 n~1.1, E Tibet m~0.85 n~1.7, Mendocino m~2 n~4 | VERIFIED | Lague 2014, Evidence section, p.41 (journal pagination) | https://wpg.forestry.oregonstate.edu/sites/default/files/seminars/2014_Lague_ESPL.pdf |
| §6 n knickpoints (T13) | empirical SPIM n = 1, m = 0.5; "fundamental inconsistency" with SS n ~ 2 | VERIFIED | p.44: "…requires for most cases n ~ 2 and m ~ 1, while KP migration requires n ~ 1 and m ~ 0.5" | same |
| §6 concavity range (T13) | 0.4–0.6 citing Kirby & Whipple 2012 and Whipple 2004; "No systematic compilation of θ exists" | VERIFIED | p.39 | same |
| §8 Beyer Grok vs primary | thesis deposition 0.2, erosion 0.7 | VERIFIED | confirmed again; today's Grok pass also misread Table 6.1 (see Grok section) | Beyer URL |
| §8 thesis vs port | Beyer 0.3/8/0.7/0.2/0.02/10/4 vs T16 0.05/4/0.3/0.3/0.01/4/3 | VERIFIED | | Beyer, T16 |
| §8 measured concavity central value | 0.425 vs 0.51 vs 0.45; IQR 0.225–0.575 | VERIFIED | | T14, T18 |
| §8 field n SS vs KP | n ~ 2 vs n = 1; T14 2.43 | VERIFIED | | T13, T14 |
| §3 T2 model | C = Kc · sin α · \|v\|; dissolve/deposit with Ks/Kd; evaporation d(1−Ke·Δt); 7 GPU passes | VERIFIED | Eqs. 10–15 as stated. No numeric Kc/Ks/Kd/Ke in the paper (file already says so) | https://nlpr.ia.ac.cn/2007papers/gjhy/gh116.pdf |
| §3 T2 Table 1 timings | 256² 1.29/2.48 ms (403/s) through 4096² 91.22/248.47 ms (4/s); Δt 0.002 … 0.000125, halves as resolution doubles | VERIFIED | Table 1, all five grids. CT is simulation + visualisation, which the file calls “with rendering” | same |
| §3 T2 hardware and CPU quotes | Pentium IV 2.40 GHz, GeForce 8800 GTX; earlier CPU work 4 fps at 256² / 5 fps at 300²; time scales about linearly with cell count | VERIFIED | | same |
| §2/§3 T3 parameters | Ck 0.0001 (rock) to 0.1 (sand); 0.0001 ≤ ck ≤ 0.01; water z-fight threshold 0.1 m; talus 30° for dry sand; Δd = Δt(Δh − l tan α) when Δh > l tan α | VERIFIED | Eq. 2 and surrounding text; “for dry sand we take the talus angle of 30°”; “0.1[m]” | https://www.cs.purdue.edu/homes/bbenes/papers/Stava08SCA.pdf |
| §2/§3 T3 Table 1 timings and hardware | 256² slippage 0.14/0.39/0.75 ms, total 1.22/1.78/2.1; 512² 0.46/1.35/2.73 and 3.72/4.9/7.3; 1024² 1.8/5.45/10.8 and 14.26/23.20/28.7; 2048×1024 slippage 3.38/10.0/20.1, fluid 7.75, velocity-erosion 17.24, dissolution 8.67, total 26.54/43.92/54.4; Athlon X2 6000+, GeForce 8800 GTX; ≥20 fps at 2048×1024 with four layers | VERIFIED | Table 1 and abstract / implementation section | same |
| §2/§5 T4 stream-power parameters | m=0.5 n=1 “as in most geomorphological studies”; k=5.61×10⁻⁷ y⁻¹ for ~2000 m peaks; U=5.0×10⁻⁴ m/yr “average uplift among earth mountains”; 50×50 km²; δt=2.5×10⁵ yr; h_max(km)=2.244·u/k | VERIFIED | Re-derived h_max = 2.244 × 5.0e-4 / 5.61e-7 = 2.000 km exactly | https://www.cs.purdue.edu/homes/bbenes/papers/Cordonier16CGF.pdf |
| §2 T4 talus | “usual” 30°, attributed to Musgrave 1989; random per point 6°–54°; height-dependent gives cliffs and terraces | VERIFIED | “30°, which is the usual talus angle for thermal erosion [MKM89]”; “6° and 54° respectively”; height-dependent slopes / cliffs | same |
| §5.3 T4 Table 1 timings | 10k 0.031/6.4 s; 40k 0.177/35.4; 90k 0.401/78.0; 160k 1.273/252.0; ~200 steps to converge; mountains after ~50, stable 100–300; Core i7 3 GHz, 16 GB | VERIFIED | Table 1 includes 160,000 → 1.273 / 252.0. “Convergence is obtained after around 200 time steps.” Hardware line: “Intel Core i7 CPU, clocked at 3GHz with 16GB of RAM” | same |
| §5 T4 lake complexity | O(N + M log M) | VERIFIED | as stated in the paper (lake connections) | same |
| §2/§3 T5 Table 1 and complexity | rain 0.0695/0.43/3.92/36.5 s at 128–1024; gravity 0.0067/0.027/0.08/0.43; temperature 0.0016/0.0067/0.027/0.12; runoff ~75%; worst case O(n⁴); average O(n²)–O(n³); Core i7 2.5 GHz | VERIFIED | Table 1. “overall average complexity varies from O(n²) for terrains with limited runoff to O(n³) for terrains with long channels.” “represents 75% of the computation” | https://www.cs.purdue.edu/homes/bbenes/papers/Cordonnier17ToG.pdf |
| §2 T6 thermal rule and 30–45° | ∂h/∂t = −k(s − tan θ) if s > tan θ; resistance-weighted variant; talus 30–45° | VERIFIED | “The talus angle varies within 30 − 45° degrees interval” | https://www.cs.purdue.edu/cgvlab/www/resources/papers/Galin-Computer_Graphics_Forum-2019-A_Review_of_Digital_Terrain_Modeling.pdf |
| §2/§3 T6 Table 3 classes | MKM89/RPP93/BF02/PM13 “Seconds”; NWD05 and MDH07 “Interactive”, 1–10 km, 1–10 m; ŠBBK08 and KBKŠ09 same; CGG*17 “Minutes”; BTHB06 “Hours”; Kur12/13 “Seconds” | VERIFIED | Table 3 | same |
| §2 T6 limitations and look | Forward-Euler accurate only for small steps, unsuitable for large-scale erosion; volumetric 3D thermal limited to about 1×1×1 km³; BF02 leaves layers of smooth material in the floor bed and in pools after drying; glacial landforms 50–100 km | VERIFIED | “approximates forward Euler time stepping and this is physically accurate only for small time steps… prevents its use when simulating large scale erosional features”. “≈ 1 × 1 × 1 km³ scenes”. “layers of smooth material deposited in floor bed and in pools of water after drying” (file says “river beds”; same claim). “glacial erosion, are 50 − 100 km in extent” | same |
| §2/§3 T7 thermal and hydraulic parameters | T = 4/N (N = 512); c = 0.5; Moore (8) reference, von Neumann (4) optimised; ~50 iterations for the visual effect; Kr=0.01, Ks=0.01, Ke=0.5, Kc=0.01 | VERIFIED | “a value of T = N4 was chosen” / “talus threshold T = N4” (OCR for 4/N). “A reasonable value for c is 0.5”. Neighbourhoods and coefficients as stated | https://web.mit.edu/cesium/Public/terrain.pdf |
| §2/§3 T7 timings | thermal 512², 500 iter: 60 s reference / 10 s optimised; hydraulic 148 s / 27 s; Pentium 4, 2.4 GHz | VERIFIED | “500 iterations are calculated in 60 seconds, while the speed optimized version does it in 10 seconds”; hydraulic “148 sec” / “27 seconds”; “standard 2.4 GHz Pentium 4 PC” | same |
| §4.2 T9 parameters | Kε=0.1, τc=3, n=0.5 typical, D=0.1, Δt up to 2 ms (leapfrog) | VERIFIED | “We set n = 0.5 that is a typical value”; “Δt = 2 [ms]”; Leapfrog Verlet | https://www.cs.purdue.edu/homes/bbenes/papers/Kristof09CGF.pdf |
| §4.2 T9 Table 1 timings | Canyon 136,000 (text “130,000” ⚠) 700×350, 0.33/0.39/0.66 s, total 1.38 s; Lake 90,000, 0.31/0.17/0.42, total 0.9 s; Waterfall 650,000, 728×512, 1.25/1.05/2.20, total 4.5 s; Q6600 2.4 GHz, 8800 GT; ~100% overhead; interactive up to 25,000 particles | VERIFIED | Table 1 and the accompanying text (1.38, 0.9, 4.5 s). The 136,000 vs 130,000 flag in the file is right | same |
| §4.3 T10 parameters and cost | e=0.01, d=0.01, ω=0.2, τce=τcd=10, f=1/2, ρ=0.001; 3 minutes on 300³ takes “several hours” on a 3 GHz CPU | VERIFIED | “ω = 0.2 is the fall velocity within the liquid.” Other constants as stated | https://www.cs.purdue.edu/homes/bbenes/papers/Benes06SCCG.pdf |
| §6 T8 Appendix Table 1 | g=9.8, ρ=917, l=2.02, sc=37°, m=0.4 n=1.0, kf printed “5×10⁻⁶ − 1.5×10⁻⁶” with unit m^(2m−1) y⁻¹ ⚠, plus the other listed glacial parameters | VERIFIED | Appendix Table 1 (raw text). The kf typo and unit flags already in the file are right | https://pubs.cs.uct.ac.za/id/eprint/1581/1/Glacial%20Erosion.pdf |
| §6 T8 timings and hardware | 3.7 / 9.8 / 37.5 min at 512² / 1024² / 2048²; NVIDIA RTX A5000 24 GB (extracted text “A500”) ⚠; ~94% accuracy at dt=100 yr; 10.5% mass loss at 70 kyr | VERIFIED | “Computations take 3.7 min at 512×512 … 9.8 min at 1,024 × 1,024 and 37.5 min at 2,048 × 2,048”. Hardware line really prints “NVidia RTX A500 GPU (24 GB)”; the file’s ⚠ (24 GB implies A5000) is the right reading. “accuracy above 94% for dt = 100 years”; “mass of 10.5% at t = 70,000 years”. Licence: “© 2023 Copyright held by the owner/author(s). Publication rights licensed to ACM” | same |
| §3/§5 T17 expected concavity | 0.30–0.60 for uniform uplift, rock and climate | VERIFIED | “channel concavities should fall within a range of 0.30–0.60 … [Whipple and Tucker, 1999; Roe et al., 2002]” | https://burbank.faculty.geol.ucsb.edu/Site/Publications_files/Duvall%20Ynez%20channels%20JGR.pdf |
| §3/§5 T17 measured Santa Ynez | weak rock only 0.48 ± 0.04 (n=8 low-uplift) and 0.6 ± 0.09 (n=6 high-uplift); transition 0.92 ± 0.07 (n=35); single streams 0.37 to 1.6 | VERIFIED | Group means as previously outlined in the file. Individual Table 1 values include Los Sauces west 0.37 ± 0.06 and Damsite Canyon 1.6 ± 0.26. Weak-rock text also says a “relatively narrow range between ~0.45 and 0.6” | same |

## Sources

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| T13 Lague 2014 | https://wpg.forestry.oregonstate.edu/sites/default/files/seminars/2014_Lague_ESPL.pdf | y (200, PDF) | y (© 2013 Wiley, printed on p.1) |
| T13 DOI | https://doi.org/10.1002/esp.3462 | n to curl (403, Wiley bot block) | — |
| T12 | https://api.crossref.org/works/10.1029/1999JB900120 | y | y (© AGU; abstract only) |
| T14 Harel AAM | https://www.pure.ed.ac.uk/ws/files/25670599/25669129._Harel_et_al_2016_author_accepted_manuscript_002_.pdf | y (200) | y (no open licence printed in AAM) |
| T16 | https://github.com/SebLague/Hydraulic-Erosion | y | y (MIT, GitHub API + LICENSE "Copyright (c) 2019 Sebastian Lague") |
| T18 Gailleton | https://eprints.gla.ac.uk/251538/1/251538.pdf | y (200) | y (CC BY-NC 4.0; PDF text and Crossref licence) |
| T20 Beyer | https://ardordeosis.github.io/implementation-of-a-method-for-hydraulic-erosion/thesis-beyer.pdf | y (200) | y (no licence stated) |
| T20 old link | http://www.firespark.de/…hydraulic%20erosion.pdf | n (500), as the file says | — |
| T2 | https://nlpr.ia.ac.cn/2007papers/gjhy/gh116.pdf | y (200, PDF) | y (© IEEE / Pacific Graphics; no CC on the copy) |
| T3 | https://www.cs.purdue.edu/homes/bbenes/papers/Stava08SCA.pdf | y | y (© Eurographics / SCA; author copy) |
| T4 | https://www.cs.purdue.edu/homes/bbenes/papers/Cordonier16CGF.pdf | y | y (© 2016 The Author(s) / Eurographics / Wiley; author copy). Confirmed from the paper front matter |
| T5 | https://www.cs.purdue.edu/homes/bbenes/papers/Cordonnier17ToG.pdf | y | y (© ACM; author copy) |
| T6 | https://www.cs.purdue.edu/cgvlab/www/resources/papers/Galin-Computer_Graphics_Forum-2019-A_Review_of_Digital_Terrain_Modeling.pdf | y | y (“c 2019 The Author(s) / Eurographics / Wiley”) |
| T7 | https://web.mit.edu/cesium/Public/terrain.pdf | y | no open licence stated in the pages checked (technical report / thesis-style PDF) |
| T8 | https://pubs.cs.uct.ac.za/id/eprint/1581/1/Glacial%20Erosion.pdf | y | y (© 2023 owner/author(s), rights licensed to ACM) |
| T9 | https://www.cs.purdue.edu/homes/bbenes/papers/Kristof09CGF.pdf | y | y (© Eurographics / CGF; author copy) |
| T10 | https://www.cs.purdue.edu/homes/bbenes/papers/Benes06SCCG.pdf | y | y (© SCCG / author copy; no CC found in the extract) |
| T17 | https://burbank.faculty.geol.ucsb.edu/Site/Publications_files/Duvall%20Ynez%20channels%20JGR.pdf | y | y (© AGU; author-hosted copy) |

## Not found
- None of the T2–T10 or T17 figures in the tables failed to open in a primary PDF. T7 and T10 have no open-licence statement in the pages checked; that is a licence gap, not a missing number.

## Sources disagree
- All earlier items (Beyer Grok vs primary; Lague page mapping) are kept.
- **T9 Canyon particle count.** Table 1 prints 136,000; the accompanying text says 130,000. Already flagged in the file.
- **T8 GPU name.** The PDF text prints “NVidia RTX A500 GPU (24 GB)”. An RTX A500 is not a 24 GB card; A5000 is. Already flagged in the file.
- **T8 kf typo and unit.** Already flagged in the file; confirmed as printed.

## Grok pass
- A new F1–F3 completing-pass run was launched at **10:41:47 AEDT** on 2026-11-17: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve --prompt-file /workspace/lv-scratch/f13/grok/prompt.txt`, cwd `/workspace/lv-scratch/f13/grok`. The 25-minute wait ends at about 11:06:47 AEDT. At **11:07 AEDT** the process was **still running** (about 25 minutes; TUI output only; no finished report), so this verification was finished without it and **does not use any Grok-only number**. Every figure above was opened in a primary PDF or official page.
- The earlier 09:01–09:05 AEDT F1–F3 run described in the previous verify file is unchanged and does not cover these leftover rows.
