# F2: erosion as algorithms, with their parameters, cost and timings

Item F2 of `LIBRARY_RESEARCH_BRIEF.md` (§F Terrain). Compiled 2026-11-16 (Australia/Sydney). This is a private working file: nothing here was posted, published or committed.

## 0. Read this first

- **Source keys.** Every number carries a source key (T1, T2, …). Each key has a full entry in §9 with its URL and the access date, 2026-11-16.
- **Measured or chosen.** Each row is tagged with one of these:
  - **param**: a simulation constant the authors picked, a default, or a demo value.
  - **meas**: a value fitted to field data.
  - **perf**: a published timing.

  Nearly every computer-graphics constant here is **param**. The authors tuned them to look right. They are not physical measurements, and they are often in arbitrary grid units.
- **Verification marks.**
  - ✔ means I read the number in the primary text myself, from the PDF via `pdftotext` or from the code file.
  - ◐ means only the abstract or metadata was read, or the number comes from a second paper describing the source. That second paper is named in the row.
  - ⚠ means the number is doubtful as printed. Every ◐ and ⚠ item is repeated in §7.
- **Method.** Web research with WebSearch, WebFetch, curl and pdftotext. **A Grok live-search pass was also run** (Grok CLI, model grok-4.6, reasoning effort xhigh, live web search; launched about 10:48 AEDT, process exited before 10:58 AEDT; raw output kept at `/tmp/grokrun/f2.out`). Grok's claims were used only where I then opened the primary source myself; those rows are marked "found via Grok, checked ✔". Grok claims I could not check are listed in §7, and conflicts it found or caused are in §8. Scratch copies of the papers are in `/tmp/f2/`. No dataset was downloaded into any repo.
- **Licences.** The papers are copyrighted (ACM, IEEE, Eurographics/Wiley, Elsevier, AGU/Wiley) unless the row says otherwise. Only facts and numbers are cited, with no copied figures or wording. Two sources have open licences that matter for reuse:
  - T18, Gailleton et al. 2021, is **CC BY-NC 4.0 (non-commercial)**.
  - Two code repositories are **MIT**: T15 (Schott) and T16 (Lague droplet code).

## 1. Sources and licences

| Key | Source | What it gives | Licence, in plain words | Read |
|---|---|---|---|---|
| T1 | Musgrave, Kolb & Mace 1989, "The synthesis and rendering of eroded fractal terrains", SIGGRAPH Comput. Graph. 23(3):41–50 | first thermal and hydraulic erosion on heightfields | © ACM. **Full text not obtained**: the ACM PDF is bot-blocked and a mirror served HTML | ◐ abstract (Crossref), plus descriptions in T6, T7 and T4 |
| T2 | Mei, Decaudin & Hu 2007, "Fast hydraulic erosion simulation and visualization on GPU", Pacific Graphics 2007, pp.47–56 | virtual-pipe shallow-water model with capacity, dissolve, deposit and evaporate steps; GPU timings | © IEEE. Institutional copy (NLPR) | ✔ |
| T3 | Šťava, Beneš, Brisbin & Křivánek 2008, "Interactive terrain modeling using hydraulic erosion", SCA 2008, pp.201–210 | multi-layer version of T2; Ck per material; talus slippage; timings by layer count | © Eurographics Association. Author copy (Purdue) | ✔ |
| T4 | Cordonnier, Braun, Cani, Beneš, Galin, Peytavie & Guérin 2016, "Large scale terrain generation from tectonic uplift and fluvial erosion", CGF 35(2):165–175 | stream power with m, n, k, u; O(N) solver; lake routing; timings | © 2016 The Author(s); CGF © Eurographics & Wiley. Author copy, also on HAL | ✔ |
| T5 | Cordonnier, Galin, Gain, Beneš, Guérin, Peytavie & Cani 2017, "Authoring landscapes by combining ecosystem and terrain erosion simulation", ACM TOG 36(4) | stochastic-event erosion (rain, gravity, temperature, lightning, vegetation); timings per event | © 2017 owner/author(s); publication rights ACM. Author copy | ✔ |
| T6 | Galin, Guérin, Peytavie, Cordonnier, Cani, Beneš & Gain 2019, "A review of digital terrain modeling", CGF 38(2):553–577 | thermal equation; talus-angle range; scale and speed classes for every method | © 2019 The Author(s); CGF © Eurographics & Wiley. Author copy, also on HAL | ✔ |
| T7 | Olsen 2004, "Realtime procedural terrain generation", IMADA, Univ. of Southern Denmark (tech. report) | thermal T = 4/N; hydraulic Kr, Ks, Ke, Kc defaults; CPU timings | Copyright not stated in the copy; treat as all rights reserved | ✔ |
| T8 | Cordonnier, Jouvet, Peytavie, Braun, Cani, Beneš, Galin, Guérin & Gain 2023, "Forming terrains by glacial erosion", ACM TOG 42(4), August 2023 (the author copy prints "Article 1"; final article number not checked) | full glacial parameter table; timings | © owner/author(s); publication rights ACM. Author copy (UCT eprint) | ✔ |
| T9 | Krištof, Beneš, Křivánek & Šťava 2009, "Hydraulic erosion using smoothed particle hydrodynamics", CGF 28(2) (Eurographics 2009) | particle (SPH) erosion; Kε, τc; timings | © Eurographics & Blackwell. Author copy (Purdue) | ✔ |
| T10 | Beneš 2006, "Physically-based hydraulic erosion", SCCG 2006 | 3D (voxel) erosion constants; cost | Copyright not stated in the copy | ✔ |
| T11 | Braun & Willett 2013, "A very efficient O(n), implicit and parallel method to solve the stream power equation…", Geomorphology 180–181:170–179 | FastScape solver | © Elsevier. **Full text and abstract not obtained** (paywalled; WebFetch got 400) | ◐ metadata (Crossref), plus description in T4 |
| T12 | Whipple & Tucker 1999, "Dynamics of the stream-power river incision model…", JGR 104(B8):17661–17674 | theory: n is the key unknown; response time | © AGU. Abstract only | ◐ abstract (Crossref) |
| T13 | Lague 2014, "The stream power river incision model: evidence, theory and beyond", ESPL 39:38–61 | review: SPIM validity; threshold-stochastic regime; n = 1 vs n > 1; field n by dataset | © 2013 Wiley. Full text read from a seminar copy hosted by Oregon State (found via Grok) | ✔ |
| T14 | Harel, Mudd & Attal 2016, "Global analysis of the stream power law parameters based on worldwide ¹⁰Be denudation rates", Geomorphology 268:184–196 | measured m/n, n and K; 1,457 basins in 59 zones | © Elsevier. Author accepted manuscript (Edinburgh Pure); no open licence stated | ✔ AAM |
| T15 | Schott, Paris, Fournier, Guérin & Galin 2023, "Large-scale terrain authoring through interactive erosion simulation", ACM TOG 42(5) | uplift-domain stream power, interactive on GPU | Paper © ACM; **code MIT**. Paper text not obtained (HAL serves an anti-bot stub to curl and 403 to WebFetch) | ◐ abstract; ✔ code defaults |
| T16 | S. Lague (Sebastian), "Hydraulic-Erosion" (GitHub; Unity droplet erosion after Beyer 2015) | droplet (particle) parameters as defaults | **MIT** | ✔ code |
| T17 | Duvall, Kirby & Burbank 2004, JGR 109, F03002 | concavity range 0.30–0.60 (theory plus earlier studies); measured concavities | © AGU. Author-hosted copy (UCSB) | ✔ |
| T18 | Gailleton, Mudd, Clubb, Grieve & Hurst 2021, "Impact of changing concavity indices on channel steepness and divide migration metrics", JGR Earth Surface 126 | measured concavity θ in 5,033 basins | **CC BY-NC 4.0 (non-commercial)** | ✔ |
| T20 | Beyer 2015, "Implementation of a method for hydraulic erosion", Bachelor's thesis, TU München (found via Grok) | the original droplet-erosion parameter set and timings | Licence not stated in the PDF; treat as all rights reserved | ✔ |
| T19 | Kirby & Whipple 2012, "Expression of active tectonics in erosional landscapes", J. Struct. Geol. 44:54–75 | review of θ and ksn | © Elsevier. Not opened | ◐ metadata only |

## 2. Thermal erosion (talus relaxation)

**Rule.** Material moves downhill wherever the local slope is steeper than the talus angle θ:
∂h/∂t = −k (s − tan θ) if s > tan θ (T6 ✔).

Two variants:
- Resistance-weighted (Roudier): −ρ(p)(s − tan θ) (T6 ✔).
- Diffusion, ∂h/∂t = −kΔh. T6 suggests it as a smoother alternative.

| Parameter | Value | Tag | Source |
|---|---|---|---|
| Talus angle, "usual" | 30° (attributed to Musgrave 1989) | param | T4 §thermal correction ✔ (T1 not opened) |
| Talus angle, range for earth materials | 30–45° | param/desc | T6 §4.1 ✔ |
| Talus angle, dry sand | 30° | param | T3 §5 ✔ |
| Talus angle, random per point | 6°–54° (stylised variant) | param | T4 ✔ |
| Talus angle, height-dependent | gives cliffs and terraces | param | T4 ✔ |
| Talus threshold in grid units (height difference between neighbours, terrain scaled to [0,1]) | T = 4/N (N = grid side, 512) | param | T7 ✔ |
| Fraction of the excess moved per iteration | c = 0.5 | param | T7 ✔ |
| Neighbourhood | Moore (8) in the reference version; von Neumann (4) in the optimised one | param | T7 ✔ |
| Iterations needed | about 50 for the visual effect (500 used for timing) | param | T7 ✔ |
| Šťava slippage | Δd = Δt (Δh − l·tan α) when Δh > l·tan α (l = cell spacing) | param | T3 ✔ |
| Critical slope for debris flow (glacial paper) | sc = 37° | param | T8 App. Table 1 ✔ |

**Cost.** One pass is O(N²) for an N×N grid. Galin rates the class MKM89/RPP93/BF02/PM13 as running in "Seconds" (T6 Table 3 ✔).

| Implementation | Grid | Iterations | Time | Hardware | Source |
|---|---|---|---|---|---|
| Olsen reference thermal | 512² | 500 | 60 s | Pentium 4, 2.4 GHz | T7 ✔ perf |
| Olsen optimised thermal | 512² | 500 | 10 s | same | T7 ✔ perf |
| Šťava slippage, 1 / 2 / 4 layers | 256² | per step | 0.14 / 0.39 / 0.75 ms | Athlon X2 6000+, GeForce 8800 GTX | T3 Table 1 ✔ perf |
| same | 512² | per step | 0.46 / 1.35 / 2.73 ms | same | T3 ✔ |
| same | 1024² | per step | 1.8 / 5.45 / 10.8 ms | same | T3 ✔ |
| same | 2048×1024 | per step | 3.38 / 10.0 / 20.1 ms | same | T3 ✔ |
| Cordonnier 2017 "temperature" event (frost breaking rock) | 128 / 256 / 512 / 1024 | per step | 0.0016 / 0.0067 / 0.027 / 0.12 s | Core i7, 2.5 GHz | T5 Table 1 ✔ |
| Cordonnier 2017 "gravity" event (material falls to its rest angle) | 128 / 256 / 512 / 1024 | per step | 0.0067 / 0.027 / 0.08 / 0.43 s | same | T5 ✔ |

**What it looks like** (the sources' own judgements):
- The result looks regular.
- Forward-Euler stepping is accurate only for small time steps, so the method is unsuitable for large-scale erosion (T6 ✔).
- Volumetric 3D thermal erosion has been limited to scenes of about 1×1×1 km³ (T6 ✔).

## 3. Hydraulic erosion: grid / virtual-pipe (Eulerian)

**Model (T2 ✔, eqs. 10–15).**
- Capacity: C = Kc · sin α · |v|. To avoid zero capacity on flat ground, α is clamped to a user-set minimum.
- Dissolve when C > s: b −= Ks(C − s), s += Ks(C − s).
- Deposit when C ≤ s: b += Kd(s − C), s −= Kd(s − C).
- Sediment is advected semi-Lagrangian.
- Evaporation: d ← d(1 − Ke·Δt).

The model takes 7 GPU passes per step. T3 extends it with Ck per material layer and a regolith layer.

| Parameter | Value | Tag | Source |
|---|---|---|---|
| Kc, Ks, Kd, Ke in Mei 2007 | **no numeric values given in the paper** | — | T2 ✔ (checked) |
| Ck, sediment capacity per layer | 0.0001 (rock) to 0.1 (sand) | param | T3 eq.2 ✔ |
| Maximum regolith penetration depth constant | 0.0001 ≤ ck ≤ 0.01 | param | T3 ✔ |
| Water threshold for rendering (z-fighting) | 0.1 m | param | T3 ✔ |
| Δt by grid (Mei) | 0.002 (256²), 0.001 (512²), 0.0005 (1024²), 0.00025 (2048²), 0.000125 (4096²); halves as resolution doubles (CFL) | param | T2 Table 1 ✔ |
| Beneš–Forsbach-style cellular automaton (Olsen variant): rain Kr | 0.01 | param | T7 ✔ |
| Solubility Ks | 0.01 | param | T7 ✔ |
| Evaporation Ke | 0.5 | param | T7 ✔ |
| Sediment capacity Kc | 0.01 | param | T7 ✔ |

**Timings.**

| Method | Grid | Per step | Notes | Source |
|---|---|---|---|---|
| Mei 2007 GPU, simulation only (ST) / with rendering (CT) | 256² | 1.29 / 2.48 ms (403 cycles/s) | Pentium IV 2.4 GHz, GeForce 8800 GTX | T2 Table 1 ✔ perf |
| same | 512² | 2.38 / 5.38 ms (186/s) | | T2 ✔ |
| same | 1024² | 6.65 / 16.95 ms (59/s) | | T2 ✔ |
| same | 2048² | 22.88 / 62.50 ms (16/s) | | T2 ✔ |
| same | 4096² | 91.22 / 248.47 ms (4/s) | time scales about linearly with cell count | T2 ✔ |
| Earlier CPU work, as quoted by Mei | 256² / 300² | 4 / 5 fps | Beneš and others, quoted in T2 | T2 ✔ (secondary) |
| Šťava 2008 total, 1 / 2 / 4 layers | 256² | 1.22 / 1.78 / 2.1 ms | | T3 Table 1 ✔ |
| same | 512² | 3.72 / 4.9 / 7.3 ms | | T3 ✔ |
| same | 1024² | 14.26 / 23.20 / 28.7 ms | | T3 ✔ |
| same | 2048×1024 | 26.54 / 43.92 / 54.4 ms | at 4 layers: fluid 7.75, velocity-erosion 17.24, dissolution 8.67, slippage 20.1 ms | T3 ✔ |
| Olsen hydraulic CA, reference / optimised | 512², 500 iterations | 148 s / 27 s | Pentium 4, 2.4 GHz | T7 ✔ |
| Cordonnier 2017 "rain" event | 128 / 256 / 512 / 1024 | 0.0695 / 0.43 / 3.92 / 36.5 s | runoff is about 75% of all compute; worst case O(n⁴), average O(n²)–O(n³) | T5 ✔ |

Galin's classes (T6 Table 3 ✔):
- NWD05 and MDH07 (= T2): "Interactive", 1–10 km extent, 1–10 m precision.
- ŠBBK08 (= T3) and KBKŠ09: same as above.
- CGG*17 (= T5): "Minutes".

**What it looks like** (the sources' words, paraphrased):
- Where water moves slowly, dissolution erosion dominates. Where it moves fast, force-based erosion dominates (T3 ✔).
- BF02-style models leave layers of smooth sediment in river beds and in pools after drying (T6 ✔).

## 4. Hydraulic erosion: particle / droplet / SPH (Lagrangian)

### 4.1 Droplet (Beyer-style), code defaults

T16 (MIT): Unity C#. Its README cites Beyer 2015 as the source.

**Beyer 2015 (T20) was obtained in the Grok pass** (author's GitHub Pages copy) and read with pdftotext. It has no single "default" table. The set it used for its timings (Table 6.1 caption, printed with decimal commas) differs from T16; see the second table below and §8.

| Parameter | Default | Allowed range in code | Source |
|---|---|---|---|
| erosionRadius | 3 cells | 2–8 | T16 ✔ |
| inertia | 0.05 (0 = always turns straight downhill, 1 = never turns) | 0–1 | T16 ✔ |
| sedimentCapacityFactor | 4 | — | T16 ✔ |
| minSedimentCapacity | 0.01 | — | T16 ✔ |
| erodeSpeed | 0.3 | 0–1 | T16 ✔ |
| depositSpeed | 0.3 | 0–1 | T16 ✔ |
| evaporateSpeed | 0.01 | 0–1 | T16 ✔ |
| gravity | 4 | — | T16 ✔ |
| maxDropletLifetime | 30 steps | — | T16 ✔ |
| initialWaterVolume / initialSpeed | 1 / 1 | — | T16 ✔ |
| Example run | "70k iterations" (droplets) in the README before/after image | — | T16 README ✔ |

**Beyer 2015 (T20 ✔, param), the parameter set used for the timings (Tables 6.1 and 6.2):** inertia 0.3; capacity 8; deposition 0.2; erosion 0.7; evaporation 0.02; minSlope 0.01; gravity 10; radius 4; maxPath 64.

Values shown in the thesis's §5.4 example figures (range explored, not defaults): inertia 0.025 / 0.1 / 0.4; deposition 0.01 / 0.1 / 1; erosion 0.01 / 0.1 / 0.9; evaporation 0.0125 / 0.05 / 0.1; minSlope 0.05 / 0.01 / 0.0001. Inertia must be between 0 and 1.

**Beyer 2015 timings (T20 ✔ perf).** Intel Core i5-3470 at 3.20 GHz, 8 GB RAM; each value is the mean of 10 runs, in seconds.

| Heightmap | 5,000 drops | 10,000 | 20,000 | 50,000 | 100,000 |
|---|---|---|---|---|---|
| 256 × 256 | 1.83 | 2.27 | 7.66 | 19.63 | 32.02 |
| 512 × 512 | 2.08 | 2.49 | 7.79 | 19.75 | 35.89 |
| 1024 × 1024 | 2.29 | 2.56 | 8.60 | 20.90 | 40.65 |
| 2048 × 2048 | 2.38 | 2.66 | 9.33 | 22.47 | 43.81 |

- Radius dominates the cost. For 10,000 drops on 256 × 256 with radius 1 / 2 / 4 / 6 / 8, the times are 0.48 / 0.83 / 2.27 / 5.51 / 9.69 s (Table 6.2).
- Drops that fall off the map, out of 100,000: about 16,500 at 256², 12,000 at 512² and 5,000 at 1024². This is why big maps run slightly slower.

### 4.2 SPH particles (Krištof et al. 2009)

| Parameter | Value | Source |
|---|---|---|
| Erosion strength Kε | 0.1 | T9 ✔ param |
| Critical shear stress τc | 3 | T9 ✔ param |
| Flow behaviour index | n = 0.5 ("typical value") | T9 ✔ |
| Sediment diffusion coefficient D | 0.1 | T9 ✔ |
| Time step | up to Δt = 2 ms (leapfrog) | T9 ✔ |

**Timings** (Intel Q6600 2.4 GHz, NVIDIA 8800 GT; seconds per time step; T9 Table 1 ✔ perf):

| Scene | Particles | Heightfield | SPH | Erosion | Boundary | Total per step (text) |
|---|---|---|---|---|---|---|
| Canyon | 136,000 (Table 1); "130,000" in the text ⚠ | 700×350 | 0.33 s | 0.39 s | 0.66 s | 1.38 s |
| Lake | 90,000 | — | 0.31 s | 0.17 s | 0.42 s | 0.9 s |
| Waterfall | 650,000 | 728×512 | 1.25 s | 1.05 s | 2.20 s | 4.5 s |

- The abstract claims interactive rates up to 25,000 particles.
- Erosion adds about 100% overhead on top of the SPH cost.
- Galin rates Lagrangian methods (Kur12/13) as "Seconds" (T6 ✔).

### 4.3 3D voxel (Navier–Stokes) erosion (Beneš 2006)

| Parameter | Value | Source |
|---|---|---|
| Erosion-rate constant e | 0.01 | T10 ✔ param |
| Deposition-rate constant d | 0.01 | T10 ✔ |
| Fall velocity ω | 0.2 | T10 ✔ |
| Critical shear stresses τce = τcd | 10 | T10 ✔ |
| Material friction f | 1/2 | T10 ✔ |
| ρ | 0.001 | T10 ✔ |
| Cost | 3 minutes of simulated time on a 300³ voxel grid takes "several hours" on a 3 GHz CPU | T10 ✔ perf |

Galin rates BTHB06 (3D fluid erosion) as "Hours", and BFO*07 and JFBB10 as "Minutes" at about 100 m extent and 1 m precision (T6 ✔).

## 5. Stream-power (fluvial incision with uplift)

**Model.** ∂h/∂t = u − k A^m s^n.
- At steady state, s ∝ (u/k)^(1/n) A^(−m/n), so the concavity is θ = m/n.
- Braun & Willett (T11) solve it implicitly in O(N). The steps are: order nodes by receiver, then solve along the stack (T11 via T4 ✔).
- Lake connection: in T11's optional step it is O(N√N). T4 makes it O(N + M log M), with M the number of lakes (T4 ✔).

### 5.1 Parameters used by graphics papers (param)

| Parameter | Cordonnier 2016 (T4 ✔) | Cordonnier 2023 glacial, fluvial part (T8 ✔) | Schott 2023 code defaults (T15 ✔, arbitrary units) |
|---|---|---|---|
| m | 0.5 ("as in most geomorphological studies") | 0.4 | 0.8 (`p_sa`) |
| n | 1 | 1.0 | 2.0 (`p_sl`) |
| k (kf) | 5.61×10⁻⁷ yr⁻¹, chosen to give peaks of about 2,000 m | 5×10⁻⁶ – 1.5×10⁻⁶, unit printed m^(2m−1) y⁻¹ ⚠ (range written high to low; stream-power dimensions would give m^(1−2m) y⁻¹) | 0.0005 |
| Uplift u | 5.0×10⁻⁴ m/yr ("average uplift among earth mountains") | — (glacial runs) | 0.03 (painted map × this) |
| Hillslope diffusion kh | — | 1×10⁻² – 5×10⁻² m² y⁻¹ | k_h = 200 |
| Other | δt = 2.5×10⁵ yr; domain 50×50 km²; h_max(km) = 2.244·u/k | dt = 25,000 yr; 64×64 km; 3 Myr | k_d = 10 (slope term); dt = 50; multiple-flow exponent p_flow = 1 |

**Implied θ = m/n** in these three columns: 0.5, 0.4 and 0.4 respectively.

### 5.2 Measured exponents from rivers (meas)

| Quantity | Value (spread) | Data | Source |
|---|---|---|---|
| Concavity θ (m/n), median | 0.425; interquartile range 0.225–0.575 | 5,033 basins worldwide, χ-based best fit | T18 ✔ |
| Concavity m/n, global | median 0.51 ± 0.14; mean 0.51 ± 0.12. Values above 0.65 were not tested, so the range is truncated | ¹⁰Be erosion rates, 1,457 samples, 59 zones | T14 ✔ |
| Slope exponent n, global | median 2.43 ± 0.15; mean 2.6 ± 0.4. "Generally > 1", which points to threshold-controlled incision | same | T14 ✔ |
| K normalised (m/n = 0.5) | median 2.9×10⁻¹⁰ ± 1.0×10⁻⁹ (units T⁻¹ L^(1−2m)); the mean is dominated by one suspect zone (> 4×10¹⁵) | same | T14 ✔ |
| Concavity, expected range for uniform uplift, rock and climate | 0.30–0.60 | theory (Whipple & Tucker 1999; Roe 2002) plus earlier empirical work | T17 ✔ (cites T12) |
| Reference concavity commonly used | 0.45 | convention for ksn | T18 ✔ (also T17) |
| Concavity from shear-stress incision theory | about 0.43 | theory | T18 ✔ |
| n: theory | "a fundamental unknown" | — | T12 ◐ abstract |
| n: field, steady state | "n ~ 2 (and thus m ~ 1)" for most datasets. Exceptions printed: m ~ 0.55 and n ~ 1.1 (Siwaliks tributaries), m ~ 0.85 and n ~ 1.7 (Eastern Tibet), m ~ 2 and n ~ 4 (Mendocino) | compilation of steady-state slope vs incision rate | T13 ✔ (found via Grok) |
| n: field, knickpoints | an empirical SPIM with n = 1 and m = 0.5 accounts for many knickpoint cases. Lague calls this a "fundamental inconsistency" with the steady-state n ~ 2 | review | T13 ✔ |
| Concavity, often-reported range | 0.4–0.6, citing Kirby & Whipple 2012 and Whipple 2004. Lague adds that "no systematic compilation of θ exists" | review | T13 ✔ (T19 itself not opened) |

### 5.3 Cost and timings

| Method | Size | One step | Convergence | Hardware | Source |
|---|---|---|---|---|---|
| Cordonnier 2016 | 10,000 nodes | 0.031 s | 6.4 s | Core i7 3 GHz, 16 GB | T4 Table 1 ✔ |
| same | 40,000 | 0.177 s | 35.4 s | | T4 ✔ |
| same | 90,000 | 0.401 s | 78.0 s | | T4 ✔ |
| same | 160,000 | 1.273 s | 252.0 s | | T4 ✔ |

Notes on Cordonnier 2016 (T4 ✔):
- About 200 steps to converge.
- Mountains appear after about 50 iterations; the shape is stable after 100–300.
- Galin lists this method at 10–100 km extent, 10–100 m precision, "Seconds" (T6 ✔).

Schott 2023: interactive per the abstract (T15 ◐). The Grok pass reported the paper's Table 1 (time to steady state, uniform uplift: 256² 0.1 ms/step, 0.5 s; 512² 0.3 ms, 1.3 s; 1,024² 0.8 ms, 9.1 s; 2,048² 3.3 ms, 128 s; 4,096² 12.5 ms, 1,036 s). **Not checked:** the ACM HTML timed out for me and HAL serves an anti-bot stub. Do not use these until checked (§7).

## 6. Glacial erosion (Cordonnier et al. 2023, T8 ✔)

**Model.**
- Ice: Shallow Ice Approximation, with a CNN emulator trained on Glen–Stokes solutions.
- Abrasion: ∂z/∂t = −ke ‖us‖^l (after Herman et al. 2015).
- Quarrying: −kq ‖us‖ Q, with Q = ½(erf(−us·∇z / (0.4 us)) + 1) (after Egholm 2012).
- Also fluvial stream power, hillslope diffusion and debris flow.

| Parameter | Value | Unit |
|---|---|---|
| β (mass-balance gradient) | 1.0×10⁻² | y⁻¹ |
| pmax (max accumulation) | 2 | m y⁻¹ |
| E (equilibrium-line altitude) | 5.0×10² – 2.0×10³ | m |
| B (ice rate factor) | 2.9×10⁵ | Pa y^(1/3) |
| C (sliding coefficient) | 5.0×10² – ∞ | Pa m^(−σ) y^σ |
| p | 4/3 | — |
| σ | 3/2 | — |
| g | 9.8 | m s⁻² |
| ρ (ice) | 917 | kg m⁻³ |
| l (abrasion exponent) | 2.02 | — |
| ke | 1.3×10⁻⁷ – 1.1×10⁻⁶ | m^(1−l) y^(l−1) |
| kq | 3.2×10⁻⁵ – 2.6×10⁻⁴ | — |
| kf (fluvial) | "5×10⁻⁶ − 1.5×10⁻⁶" ⚠ (as printed) | m^(2m−1) y⁻¹ as printed ⚠ |
| m, n (fluvial) | 0.4, 1.0 | — |
| kh | 1×10⁻² – 5×10⁻² | m² y⁻¹ |
| kdf | 10⁻⁴ – 2.5×10⁻⁴ | m^(2q−1) y⁻¹ |
| kda | 10 | m^(−2q) |
| q, q′ | 0.8, 2.0 | — |
| sc | 37 | degrees |

(T8 Appendix Table 1, read with `pdftotext -raw`.)

**Timings and stability (T8 ✔ perf).**
- About 2,000 steps per run: 3.7 min at 512², 9.8 min at 1024², 37.5 min at 2048².
- Hardware: NVIDIA RTX A5000, 24 GB. The extracted text reads "A500"; read as A5000 ⚠.
- About 50% of the time is CPU drainage plus GPU↔CPU transfer.
- A 16×16 km terrain at 32 m cells runs in under 5 min.
- Ice time step: 100 yr at dx = 100 m (about 94% accuracy; 10.5% mass loss at 70 kyr), CFL c = 0.1, maximum ice velocity about 100 m/yr.
- Glacial advance/retreat cycles of about 100,000 yr.

Galin (T6 ✔) on glacial work generally: glacial landforms span 50–100 km, and cost grows as O(n²).

**What it looks like.** U-shaped valleys are the classic result; the paper cites Egholm et al. 2011 for this (T8 ✔).

## 7. Not found / not checked against the primary source

1. **Musgrave et al. 1989 (T1), full text not obtained.**
   - The ACM PDF returns a bot-block page, a mirror served HTML, and the old heathershrewsbury URL is dead.
   - What is used is its Crossref abstract plus descriptions in T4, T6 and T7. The "usual talus angle 30°" is as cited by T4.
   - Musgrave's own constants were not read.
2. **Braun & Willett 2013 (T11).**
   - Not opened: Elsevier is paywalled, the abstract is elided in Crossref and Semantic Scholar, and WebFetch got 400.
   - The O(N) claim comes from the title and from T4. No FastScape grid sizes or timings were read.
3. **Whipple & Tucker 1999 (T12):** abstract only. Lague 2014 (T13) was read in full in the Grok pass.
4. **Kirby & Whipple 2012 (T19):** not opened. Grok quoted "~0.4–0.6" from a Scribd scan of p.55. That is not a primary copy, so it stays unchecked; T13 cites the same range to T19.
5. **Schott 2023 (T15) paper text.**
   - HAL served an anti-bot stub to curl and 403 to WebFetch.
   - The values given are code defaults (MIT), not values printed in the paper. No published timings were read.
6. **Beyer 2015 thesis (T20):** found in the Grok pass and read ✔. The old firespark.de link still returns HTTP 500.
7. **Mei 2007 constants:** the paper gives no numbers for Kc, Ks, Kd or Ke (✔ checked). Typical values come from T3 (Ck) and T7.
8. **Cordonnier 2017 (T5)** uses per-material friction angles but prints no numeric values.
9. **Harel 2016 (T14)** was read as the author accepted manuscript. The page and table numbers of the final Elsevier version were not checked.
10. **Edinburgh Research Explorer** (research.ed.ac.uk) shows a Cloudflare challenge. The same file on pure.ed.ac.uk opened, and that is the link given.
11. **Grok-pass claims not checked against the primary source** (do not use):
   - Schott 2023 Table 1 timings, PID gains (Kp = Kd = 0.01, Ki = 0.001), cell size 80–600 m, domains 100–1,000 km. The ACM HTML timed out and HAL blocks curl.
   - Whipple & Tucker 1999: "0.35 < m/n < 0.6" for 0.7 ≤ c ≤ 1 and b = 0.4–0.6; Table 1 King Range concavity 0.40 ± 0.10 (n = 14). Grok read these from ResearchGate/Scribd extracts, not the AGU PDF.
   - Braun & Willett 2013: "10⁸ nodes on a laptop… up to 10¹⁰ on a large multi-processor computer" (from a ScienceDirect snippet).
   - Cordonnier 2023: "Article 61" as the final article number; 1024² ice flow = 87% of 300 ms per step.
   - Third-party droplet code: henrikglass/erodr defaults (inertia 0.3, capacity 8, evaporation 0.02, erosion 0.7, deposition 0.3, gravity 1, radius 2). Landlab FastscapeEroder defaults (K 0.001, m 0.5, n 1.0) and Barnes 2019 values (k 2×10⁻⁶, n 2, m 0.8, u 2×10⁻³). These are code, not papers.
   - Musgrave 1989: Grok also found **no** numeric constants in the paper. It confirms the T1 gap rather than filling it.

## 8. Sources disagree

| Topic | Values | Comment |
|---|---|---|
| Talus angle | 30° "usual" (T4 citing T1; T3 for dry sand) vs 30–45° range (T6) vs 37° critical slope for debris flow (T8) vs random 6°–54° (T4 stylised) | 30° is the common computer-graphics default. Real angles of repose by material belong in F1 |
| Stream-power m (with n = 1) | 0.5 (T4) vs 0.4 (T8) | T4 says 0.5 is usual. Measured θ has median 0.425 (T18) but median 0.51 in T14. Different methods and data account for it |
| Measured concavity, central value | 0.425 (T18, χ-based, 5,033 basins) vs 0.51 (T14, ¹⁰Be-based, 59 zones, θ capped at 0.65) vs 0.45 reference (convention) | Spread is wide: T18's IQR runs 0.225–0.575 |
| Slope exponent n | 1 in almost all graphics models (T4, T8) vs median 2.43 measured (T14) vs Schott code default 2.0 (T15) | T12 calls n "a fundamental unknown" |
| T8 kf | "5×10⁻⁶ − 1.5×10⁻⁶", unit m^(2m−1) y⁻¹ | Probably meant 1.5×10⁻⁶ – 5×10⁻⁶. The unit sign is the inverse of what A^m implies. Both unconfirmed ⚠ |
| Beyer droplet set (Grok vs primary) | The Grok pass reported deposition 0.7 and erosion 0.2. The thesis prints p_deposition = 0,2 and p_erosion = 0,7 (Table 6.1 and 6.2 captions) | Primary wins: **deposition 0.2, erosion 0.7** (T20 ✔). Grok swapped the two |
| Droplet defaults: thesis vs ports | Beyer (T20): inertia 0.3, capacity 8, erosion 0.7, deposition 0.2, evaporation 0.02, gravity 10, radius 4. Lague port (T16): 0.05, 4, 0.3, 0.3, 0.01, 4, 3 | T16 is a re-tuned port, not the thesis values |
| Field n: steady state vs knickpoints | n ~ 2 (m ~ 1) from steady-state data vs n = 1 (m = 0.5) from knickpoint migration (T13 ✔). T14 median n 2.43 sides with steady state | Lague's own "fundamental inconsistency" |
| Schott m/n: paper vs code | Grok reports the paper says m/n ≈ 1/2 with no separate k (not checked); the code uses m 0.8, n 2.0, so m/n = 0.4, and k = 0.0005 (T15 ✔) | Quote code defaults as code, not as the paper |
| T9 Canyon particle count | 136,000 (Table 1) vs 130,000 (text) | as printed |

## 9. References (all accessed 2026-11-16)

Link check, 2026-11-16, curl -L with a browser user-agent, unless noted:
- All direct PDF and code URLs below returned 200.
- Several DOI links return 403 to curl because the publisher blocks bots (ACM, Wiley/AGU). Each has a Crossref API URL that returned 200.
- The IEEE DOI returned 500 to curl.

- **T1** Musgrave F.K., Kolb C.E., Mace R.S. (1989) SIGGRAPH Comput. Graph. 23(3):41–50.
  - https://doi.org/10.1145/74334.74337 (403 to curl)
  - Metadata and abstract: https://api.crossref.org/works/10.1145/74334.74337 (200)
- **T2** Mei X., Decaudin P., Hu B.-G. (2007) Pacific Graphics, pp.47–56.
  - https://nlpr.ia.ac.cn/2007papers/gjhy/gh116.pdf (200)
  - DOI 10.1109/PG.2007.15 (500 to curl)
  - HAL: https://inria.hal.science/inria-00402079
- **T3** Šťava O., Beneš B., Brisbin M., Křivánek J. (2008) SCA, pp.201–210.
  - https://www.cs.purdue.edu/homes/bbenes/papers/Stava08SCA.pdf (200)
  - https://diglib.eg.org/items/60afda0c-a666-4df8-90dd-9b80afc554c2 (200)
- **T4** Cordonnier G. et al. (2016) CGF 35(2):165–175, DOI 10.1111/cgf.12820.
  - https://www.cs.purdue.edu/homes/bbenes/papers/Cordonier16CGF.pdf (200)
  - https://hal.science/hal-01262376 (200)
- **T5** Cordonnier G. et al. (2017) ACM TOG 36(4).
  - https://www.cs.purdue.edu/homes/bbenes/papers/Cordonnier17ToG.pdf (200)
  - https://hal.science/hal-01518967 (200)
- **T6** Galin E. et al. (2019) CGF 38(2):553–577, DOI 10.1111/cgf.13657.
  - https://www.cs.purdue.edu/cgvlab/www/resources/papers/Galin-Computer_Graphics_Forum-2019-A_Review_of_Digital_Terrain_Modeling.pdf (200)
  - https://hal.science/hal-02097510 (200)
- **T7** Olsen J. (2004) Realtime Procedural Terrain Generation. IMADA, SDU.
  - https://web.mit.edu/cesium/Public/terrain.pdf (200)
- **T8** Cordonnier G. et al. (2023) Forming terrains by glacial erosion. ACM TOG 42(4).
  - https://pubs.cs.uct.ac.za/id/eprint/1581/1/Glacial%20Erosion.pdf (200)
  - https://hal.science/hal-04090644 (200)
- **T9** Krištof P., Beneš B., Křivánek J., Šťava O. (2009) CGF 28(2).
  - https://www.cs.purdue.edu/homes/bbenes/papers/Kristof09CGF.pdf (200)
- **T10** Beneš B. (2006) Physically-based hydraulic erosion. SCCG.
  - https://www.cs.purdue.edu/homes/bbenes/papers/Benes06SCCG.pdf (200)
- **T11** Braun J., Willett S.D. (2013) Geomorphology 180–181:170–179.
  - https://doi.org/10.1016/j.geomorph.2012.10.008 (200, landing page)
- **T12** Whipple K.X., Tucker G.E. (1999) JGR 104(B8):17661–17674.
  - https://doi.org/10.1029/1999JB900120 (403 to curl)
  - https://api.crossref.org/works/10.1029/1999JB900120 (abstract)
- **T13** Lague D. (2014) ESPL 39:38–61.
  - https://doi.org/10.1002/esp.3462 (403 to curl)
  - https://api.crossref.org/works/10.1002/esp.3462 (abstract)
  - Full text: https://wpg.forestry.oregonstate.edu/sites/default/files/seminars/2014_Lague_ESPL.pdf (200)
- **T14** Harel M.-A., Mudd S.M., Attal M. (2016) Geomorphology 268:184–196.
  - AAM: https://www.pure.ed.ac.uk/ws/files/25670599/25669129._Harel_et_al_2016_author_accepted_manuscript_002_.pdf (200)
  - https://doi.org/10.1016/j.geomorph.2016.05.035 (200)
- **T15** Schott H., Paris A., Fournier L., Guérin E., Galin E. (2023) ACM TOG 42(5), DOI 10.1145/3592787 (403 to curl).
  - https://hal.science/hal-04049125 (200, record page)
  - https://aparis69.github.io/public_html/projects/schott2023_Uplift.html (200)
  - Code (MIT): https://github.com/H-Schott/StreamPowerErosion (200)
  - Defaults: https://raw.githubusercontent.com/H-Schott/StreamPowerErosion/main/data/shaders/spe_shader.glsl (200)
- **T16** Lague S. Hydraulic-Erosion (MIT).
  - https://github.com/SebLague/Hydraulic-Erosion (200)
  - https://raw.githubusercontent.com/SebLague/Hydraulic-Erosion/master/Assets/Scripts/Erosion.cs (200)
- **T17** Duvall A., Kirby E., Burbank D. (2004) JGR 109, F03002, DOI 10.1029/2003JF000086.
  - https://burbank.faculty.geol.ucsb.edu/Site/Publications_files/Duvall%20Ynez%20channels%20JGR.pdf (200)
- **T18** Gailleton B., Mudd S.M., Clubb F.J., Grieve S.W.D., Hurst M.D. (2021) JGR Earth Surface 126, e2020JF006060, DOI 10.1029/2020JF006060 (403 to curl). **CC BY-NC.**
  - https://eprints.gla.ac.uk/251538/1/251538.pdf (200)
- **T20** Beyer H.T. (2015) Implementation of a method for hydraulic erosion. Bachelor's thesis, TU München, 15 Nov 2015. Licence not stated.
  - https://ardordeosis.github.io/implementation-of-a-method-for-hydraulic-erosion/thesis-beyer.pdf (200)
- **T19** Kirby E., Whipple K.X. (2012) J. Struct. Geol. 44:54–75.
  - https://doi.org/10.1016/j.jsg.2012.07.009 (200, landing page)
