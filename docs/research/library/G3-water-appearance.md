# G3: how water looks — absorption and scattering, colour, foam, and real-time drawing

Item G3 of `LIBRARY_RESEARCH_BRIEF.md` (§G Water). Compiled 2026-11-17 (Australia/Sydney). Private working file: nothing here was posted, published or committed.

## Fixes after verify (2026-11-17, Australia/Sydney, AEDT)

Applied from `G3-water-appearance.md.verify.md`. The Brewster reflectance was recomputed at n = 1.333 before editing.

1. **Brewster unpolarised reflectance:** R_unpol = **0.0391 (3.91%)**, not 0.0392 (3.92%). The angle 53.12° and R_p = 0 are unchanged. Pope & Fry Table 3 standard deviations remain unopened and are not invented.

## 0. Read this first

- **Source keys.** Every number has a key (O1, O2, …) with a full entry in §10. Each entry gives a URL and the access date, 2026-11-17.
- **Kind of number.** **meas**, **desc**, or **calc**, as in `G1-ocean-waves.md`.
- **Verification marks.** ✔ primary text or a labelled transcription I opened; **[OCR]**; **[IMG]**; ⚠ doubtful (listed in §8).
- **Method.** WebSearch, WebFetch, curl, pdftotext and page images. Scratch copies are in `/tmp/g3/` (never in a repo). **A Grok live-search pass was run** on 16 Nov 2026 (Grok CLI, grok-4.6, reasoning effort xhigh, live web search; prompt `/tmp/grokrun/g3.txt`; the process exited the same morning; raw output `/tmp/grokrun/g3.out`). No second pass was run after the 17 Nov reset, to keep usage lean. Every Grok claim used here was re-opened by me in the primary source. Many of Grok's optical tables (Jerlov, Solonenko & Mobley, Forel-Ule, Cox & Munk, Koepke, game-talk milliseconds) were *not* opened in a licensed primary this pass and are not used as verified numbers; they are listed in §8. One starting-point error Grok flagged is in §8 as well.
- **Licences.** Pope & Fry (1997) is © Optical Society of America / Optica. The spectrum values in §2 are from the OMLC transcription of that paper (O2), which I opened; the journal PDF itself was not opened (paywall). NVIDIA GPU Gems Chapter 1 is hosted by NVIDIA from the 2004 book (© NVIDIA / Addison-Wesley). Tessendorf notes are copyright 1999–2004 Jerry Tessendorf (see G1, W6). From copyrighted sources I use facts and numbers only.

## 1. Sources and licences

| Key | Source | Gives | Licence |
|---|---|---|---|
| O1 | Pope & Fry (1997) Applied Optics 36(33):8710–8723 | Integrating-cavity absorption of pure water, 380–700 nm. Abstract: minimum 0.0044 ± 0.0006 m⁻¹ at 418 nm | © OSA/Optica. DOI 10.1364/AO.36.008710. Journal PDF not opened this pass |
| O2 | OMLC transcription of Pope & Fry, `pope97.dat`, and the OMLC abstract page | a(λ) at 2.5 nm steps, 380–727.5 nm, in cm⁻¹; abstract text including the 418 nm minimum | Third-party transcription of O1. No separate licence stated on the short page I opened. Not a substitute for the journal Table 3 uncertainties |
| O3 | Tessendorf, *Simulating Ocean Water*, 1999–2004 course notes | FFT ocean, Phillips-type spectrum, grid sizes and the only published timing | © 1999–2004 Jerry Tessendorf. Cross-ref G1 source W6 |
| O4 | Finch (2004) "Effective Water Simulation from Physical Models", GPU Gems ch. 1 | Real-time sum-of-sines / Gerstner water on the GPU; used in *Uru: Ages Beyond Myst*; no millisecond cost | © NVIDIA / Addison-Wesley. Official HTML hosted at developer.nvidia.com |

## 2. Pure-water absorption (Pope & Fry 1997)

Pope & Fry measured the absorption coefficient of pure water in an integrating cavity, 380–700 nm (O1). I did not open the journal PDF. Two things I did open:

- The published abstract, as reproduced on the OMLC page (O2 ✔): the blue absorption is "significantly lower than had previously been generally believed"; the minimum is **0.0044 ± 0.0006 m⁻¹ at 418 nm**; the results are described as in good agreement with the same group's photothermal measurements in the companion paper.
- The OMLC file `pope97.dat` (O2 ✔), which stores a in cm⁻¹ and cites O1. Values below are 100 × the file (so a in m⁻¹). Per-wavelength standard deviations from the paper's Table 3 were not opened and are not invented.

| λ (nm) | a (m⁻¹) from O2 | Kind | Source |
|---|---|---|---|
| 400 | 0.00663 | meas (transcription) | O2 ✔, from O1 |
| 418 | 0.0044 ± 0.0006 (abstract minimum) | meas | O1 abstract via O2 ✔. The dat file at 417.5 / 420 nm is 0.00442 / 0.00454 m⁻¹, consistent with that minimum |
| 450 | 0.00922 | meas (transcription) | O2 ✔ |
| 500 | 0.0204 | meas (transcription) | O2 ✔ |
| 550 | 0.0565 | meas (transcription) | O2 ✔ |
| 600 | 0.2224 | meas (transcription) | O2 ✔ |
| 650 | 0.340 | meas (transcription) | O2 ✔ |
| 700 | 0.624 | meas (transcription) | O2 ✔ |

Pure-water absorption therefore rises by about two orders of magnitude from the blue minimum to 700 nm. That is why, in the absence of scatterers, a deep column looks blue: red light is absorbed in a much shorter path.

Smith & Baker (1981), Kou, Labrie & Chylek (1993) and later scattering papers (Morel 1974; Zhang et al. 2009) were not opened in the primary documents this pass. Grok reported that Smith & Baker's blue absorption is substantially higher than Pope & Fry (about 2.6× at 400 nm) and that the two agree better longward of about 600 nm. I have not checked those rows myself (§8).

## 3. How the surface reflects (Fresnel)

I did not open a measured air–water reflectance table. The unpolarised Fresnel reflectance of a flat dielectric interface, with incident medium n₁ = 1 and water n₂ = 1.333 (the conventional visible-band refractive index of water relative to air), is a straightforward calculation. Brewster's angle is arctan(n₂/n₁).

| Quantity | Value | Kind | Source |
|---|---|---|---|
| Assumed n | 1.333 (air–water, visible; an assumption for this calculation, not a measurement I opened) | desc / assumption | conventional value |
| Normal incidence | R = ((n−1)/(n+1))² = 0.0204 (2.04%) | calc | Fresnel formula, n = 1.333 |
| 30° | R_unpol = 0.0214 (2.14%); R_s = 0.0309, R_p = 0.0119 | calc | same |
| 60° | R_unpol = 0.0597 (5.97%); R_s = 0.1151, R_p = 0.0043 | calc | same |
| 80° | R_unpol = 0.3479 (34.8%); R_s = 0.4570, R_p = 0.2388 | calc | same |
| Brewster angle | 53.12°, at which R_p = 0 and R_unpol = R_s / 2 = 0.0391 (3.91%) | calc | θ_B = arctan(1.333) |

The often-quoted "about 2% at normal incidence, Brewster near 53°" matches this calculation. Real ocean reflectance also depends on surface slope (Cox & Munk 1954) and on solar elevation (Payne 1972); those papers were not opened this pass (§8).

## 4. Colour, Jerlov types, Forel-Ule, foam

These were in the brief and in the Grok pass, but I did not open a licensed copy of Jerlov (1976), Solonenko & Mobley (2015), Wernand & van der Woerd (2010), Novoa et al. (2013), Whitlock et al. (1982), Koepke (1984) or Frouin et al. (1996). I have not entered their tables. See §8.

One starting-point correction, also unchecked in the originals by me: Grok reports that "Berman et al. 1985, Weddell Sea, Secchi 79 m" mixes two papers (Berman 1985, 53 m, eastern Mediterranean, 40 cm disk; Gieskes et al. 1987, 79–80 m, Weddell polynya, 1-foot disk). Until those papers are opened, that 79 m figure should not be used.

## 5. Real-time water: Tessendorf FFT

Full numbers are in `G1-ocean-waves.md` §10 (source O3 = G1 W6), checked on the 1999–2004 notes. The points that matter for drawing water:

| Quantity | Value | Kind | Source |
|---|---|---|---|
| Method | Statistical height field as an inverse FFT of a Phillips-type spectrum P_h(k), with time evolution from the dispersion relation | desc | O3 / G1 W6 eqs. (36), (40) ✔ |
| A in P_h(k) | "a numeric constant"; no default given | desc | O3 ✔ |
| Practical grids | N, M = 16…2048, powers of two; 128–512 often enough; 1024 and 2048 for extreme detail | desc | O3 ✔ |
| Film examples | Waterworld and Titanic: 2048 × 2048, about 3 cm spacing | desc | O3 ✔ |
| Worked example | 512 × 512, patch 1000 m × 1000 m, V = 31 m s⁻¹, small-wave cutoff ℓ = 1 m | desc | O3 ✔ |
| Published timing | 512 × 512 FFTs "at nearly interactive rates" on a 1+ GHz CPU. No GPU millisecond figure | desc | O3 ✔ |
| Finch's 2004 comment on FFT | "Moderate-size grids may be processed in real time on consumer-level PCs", but "as of this writing, even real-time versions of these techniques are limited to implementation on the CPU" | desc | O4 ✔. That was 2004; later GPU FFT oceans exist, but this chapter does not give a GPU ms number |

## 6. Real-time water: Gerstner and sum-of-sines (Finch 2004)

Fournier & Reeves (1986), "A Simple Model of Ocean Waves" (SIGGRAPH), is the usual computer-graphics source for Gerstner / trochoidal waves on a shoreline. I did not open that paper this pass (no numbers used).

Finch (O4), GPU Gems Chapter 1, is a real-time, all-GPU scheme used extensively in Cyan Worlds' *Uru: Ages Beyond Myst* (O4 ✔). It runs two related simulations: geometric undulation of a base mesh, and a dynamic tiling normal map for finer ripples. Both represent height as a sum of periodic waves, starting from sines and moving to Gerstner-like shapes. Parameters given a physical meaning: wavelength L (frequency w = 2π/L), amplitude A, speed S (as a phase constant). The intended range is "a small pond to the ocean as viewed from a cove or island." It is "not a rigorous physical simulation."

Earlier real-time citations in the same chapter: Isidoro et al. 2002, four sine waves in a vertex shader; Laeuchli 2002, three Gerstner waves in a shader (O4 ✔).

**Published per-frame GPU cost:** not found in the chapter I opened.

## 7. Other real-time water (survey, costs mostly not found)

Costs are GPU- and resolution-specific and are useless without both. I did not open Sea of Thieves (SIGGRAPH 2018 Advances), Assassin's Creed III / Black Flag water talks, Crest, Unreal's Water system, Horizon Forbidden West, Atlas "Wakes", Ghost Recon, NVIDIA WaveWorks, Bruneton et al. 2010, Horvath 2015 (DigiPro), or Jeschke et al. 2018 in a primary slide deck or paper this pass. Grok also reported that several of those talks do not publish a millisecond figure on the pages it could open. I have not entered any unverified ms numbers.

Tessendorf's "nearly interactive" 512² on a 1+ GHz CPU (O3) and Finch's qualitative "entirely on the GPU, no struggle over CPU usage" (O4) are the only performance statements I have from opened primaries.

## 8. Not found / not checked against the primary source

1. **Pope & Fry Table 3 uncertainties** at each wavelength, other than the abstract minimum at 418 nm. The journal PDF was not opened. The O2 transcription has no SD column.
2. **Smith & Baker (1981)** absorption and scattering, and the size of their disagreement with Pope & Fry. Grok reported a large blue discrepancy (~2.6× at 400 nm) and better agreement past ~600 nm. Unchecked by me. Not used.
3. **Kou, Labrie & Chylek (1993)** infrared absorption. Not opened. Not used for the 400–700 nm column.
4. **Morel (1974) and Zhang et al. (2009)** seawater scattering and the λ^{−4.32} / λ^{−4.30} exponents. Not opened in the primaries. Not used.
5. **Jerlov (1976)** type tables (I, IA, IB, II, III, coastal 1–9) and **Solonenko & Mobley (2015)** inherent optical properties. Abstracts or secondary mentions only. Not used. Grok also reported that Solonenko & Mobley IOPs, being fitted to Jerlov K_d, can disagree with directly measured a and b.
6. **Forel-Ule** 21-class chromaticities (Wernand 2010; Novoa 2013). Copernicus/OS PDF fetch failed (404). Not used. Grok reported disagreement among Rosen 1930, Wernand 2010 and Novoa 2013 for the brown classes FU 12–21, with FU 1 and FU 11 still near 468 nm and ~565–567 nm. Unchecked.
7. **Payne (1972)** ocean albedo vs sun elevation, and **Cox & Munk (1954)** slope-variance 0.003 + 5.12×10^{-3} U. Not opened. Not used.
8. **Whitlock (1982), Koepke (1984) ~22% effective foam reflectance, Frouin (1996)** spectral decline in the NIR. Not opened in the primaries. Not used. Grok's account — Whitlock lab foam ~55% and fairly flat in the visible; Koepke 22% as an area-weighted effective value; Frouin a stronger NIR drop — is plausible but unchecked.
9. **Fournier & Reeves (1986)** paper body. Not opened.
10. **Sea of Thieves SIGGRAPH 2018, Crest, WaveWorks, Bruneton 2010, Horvath 2015, Jeschke 2018, Horizon Forbidden West, Unreal Water, Atlas, Ghost Recon:** primary talks/papers not opened; published GPU ms not taken from Grok.
11. **Euphotic and Secchi depths by Jerlov type**, and the brief's starting-point "Berman et al. 1985 Weddell Sea 79 m." Grok reports that 79 m is Gieskes et al. 1987 (Weddell polynya, 1-foot disk) and that Berman 1985 is 53 m in the eastern Mediterranean (40 cm disk). I did not open either paper. Do not use 79 m under Berman 1985.
12. **Refractive index as a measured function of wavelength, temperature and salinity** (Quan & Fry and similar). I used n = 1.333 as a conventional visible value for the Fresnel calculation only.

## 9. Sources disagree

| Topic | Values | Comment |
|---|---|---|
| Blue absorption of "pure water" | Pope & Fry minimum 0.0044 ± 0.0006 m⁻¹ at 418 nm (O1 abstract via O2) vs older composites (Smith & Baker 1981, not opened here) that Grok reports as much higher in the blue | This is a known historical revision; Pope & Fry say so in the abstract. Until Smith & Baker is opened, I only record the direction of the disagreement |
| "2% Fresnel" | 2.04% at normal incidence for n = 1.333 (my calc) vs "2%" in many rendering texts, sometimes with n = 1.34 (which gives ((0.34/2.34)² = 0.0211, 2.11%) | The difference is small at normal incidence; it grows at large angles. State n |
| FFT vs Gerstner for real-time | Tessendorf: film-quality FFT, "nearly interactive" 512² on a 1+ GHz CPU, no GPU ms. Finch 2004: FFT still on the CPU at the time of writing; the shipped game technique is a GPU sum of sines / Gerstner | Different dates and quality targets. Later work moved FFT onto the GPU; those papers were not opened here |
| Foam reflectance (unchecked) | If Grok's reading holds: Whitlock ~55% fresh lab foam vs Koepke 22% effective vs Frouin a stronger NIR drop | Not entered as verified. Do not treat 22% and 55% as a contradiction without reading the definitions (effective vs fresh, visible vs spectral) |

## 10. References (all accessed 2026-11-17)

Link check, 2026-11-17, curl -L: 200 for every URL below unless a note says otherwise.


- **O1** Pope R.M., Fry E.S. (1997) Absorption spectrum (380–700 nm) of pure water. II. Integrating cavity measurements. Applied Optics 36(33):8710–8723. DOI 10.1364/AO.36.008710. https://doi.org/10.1364/AO.36.008710 (journal PDF not opened; abstract via O2)
- **O2** Oregon Medical Laser Center, Pope & Fry 1997 data and abstract page. Data: https://omlc.org/spectra/water/data/pope97.dat. Page: https://omlc.org/spectra/water/abs/pope97.html
- **O3** Tessendorf J., Simulating Ocean Water, course notes, copyright 1999–2004. https://jtessen.people.clemson.edu/reports/papers_files/coursenotes2004.pdf. See also G1, source W6
- **O4** Finch M. (2004) Effective Water Simulation from Physical Models. In: Fernando R. (ed.) *GPU Gems*, Chapter 1. NVIDIA / Addison-Wesley. https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-1-effective-water-simulation-physical-models
