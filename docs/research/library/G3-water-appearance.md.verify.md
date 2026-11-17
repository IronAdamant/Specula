# G3-water-appearance.md: verification

Checked by: Grok Bot executor (adversarial source check). Access date for every URL below: **2026-11-17** (Australia/Sydney, AEDT).
File checked: `G3-water-appearance.md`. mtime at start was **2026-11-17 09:04:26 AEDT**, 15,024 bytes. Re-statted before writing and found **unchanged and byte-identical to the start snapshot**. Final mtime verified against: **2026-11-17 09:04:26.398 AEDT**.

## Summary

- **Scope:** Every numbered claim in the G3 tables, the Fresnel and unit-conversion calculations (recomputed), Finch 2004 as opened on the official NVIDIA page, and Tessendorf cross-references already checked for G1. World Data asked me to open Pope & Fry Table 3 if at all possible and to spot-check OMLC wavelengths against it.
- **Counts (rows checked: 25):** VERIFIED 23 · FIX 1 · UNVERIFIABLE 1.
- **Worst problems:**
  1. **Brewster unpolarised reflectance is 0.0391 (3.91%), not 0.0392 (3.92%).** Normal-incidence and 30°/60°/80° values, and the Brewster angle 53.12°, recompute exactly as printed. This is a last-digit rounding slip only.
  2. **Pope & Fry Table 3 standard deviations could not be opened in a licensed copy of the journal PDF.** Optica/OSA URLs were bot-blocked or returned 202. OMLC `pope97.dat` (200) and the OMLC abstract page (200) were opened; every listed a(λ) matches 100 × the file (cm⁻¹ → m⁻¹), and the 418 nm abstract minimum is consistent with the file at 417.5 / 420 nm (0.00442 / 0.00454 m⁻¹). A ResearchGate preview of Table 3 appeared in a web-search snippet with the same a(λ) values and with SDs (400 nm: 0.0007; 450: 0.0005; 500: 0.0011; 550: 0.0011; 600: 0.0017; 650: 0.003), but I did **not** open that as a licensed full text, so those SDs are **not** entered as verified.
- **Close checks:**
  - **Pope & Fry absorption:** abstract minimum **0.0044 ± 0.0006 m⁻¹ at 418 nm** confirmed on the OMLC page (the published abstract). OMLC dat-file spot checks: 400 → 0.00663; 450 → 0.00922; 500 → 0.0204; 550 → 0.0565; 600 → 0.2224; 650 → 0.340; 700 → 0.624 m⁻¹, all match. Table 3 SDs: not opened in the journal (see above). Licence © OSA/Optica, as stated. The file already flags that OMLC is a transcription and not a substitute for Table 3 uncertainties.
  - **Fresnel at n = 1.333:** recomputed with Snell's law and R_unpol = (R_s+R_p)/2. Normal: ((n−1)/(n+1))² = 0.020373 → 0.0204 (2.04%). 30°: 0.02144, R_s 0.03093, R_p 0.01194. 60°: 0.05969, R_s 0.11507, R_p 0.00431. 80°: 0.34792, R_s 0.45702, R_p 0.23882. Brewster: arctan(1.333) = 53.123°; R_p = 0; R_unpol = R_s/2 = 0.03914 → **0.0391 (3.91%)**. Grok's Fresnel figures in the same pass (0.0216 / 0.0653 / 0.0404) were wrong and were not used.
  - **Finch 2004:** official NVIDIA HTML opened (200). Used extensively in *Uru: Ages Beyond Myst*. Sum of sines progressing to Gerstner-like shapes; geometric undulation plus a dynamic tiling normal map; intended range "a small pond to the ocean as viewed from a cove or island"; "not a rigorous physical simulation"; runs entirely on the GPU, "no struggle over CPU usage." Parameters L (w = 2π/L), A, S as a phase constant. Isidoro et al. 2002 (four sines) and Laeuchli 2002 (three Gerstner waves) cited. FFT comment: "Moderate-size grids may be processed in real time on consumer-level PCs" but "as of this writing, even real-time versions of these techniques are limited to implementation on the CPU." **No millisecond GPU cost** in the chapter.
- **Dead links:** none among the four official G3 URLs that I could use for numbers. O1 (https://doi.org/10.1364/AO.36.008710) resolves into an Optica bot-challenge (200/202 HTML, no PDF).
- **Rows skipped:** none of the G3 number tables. Colour / Jerlov / foam / game-talk items in §4 and §8 were already marked unchecked by the researcher; I did not invent numbers for them.

## Rows

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| §2 abstract minimum | 0.0044 ± 0.0006 m⁻¹ at 418 nm; blue absorption "significantly lower than had previously been generally believed" | VERIFIED | OMLC page reproduces the published abstract | https://omlc.org/spectra/water/abs/pope97.html |
| §2 400 nm | 0.00663 m⁻¹ | VERIFIED | 100 × 0.0000663 cm⁻¹ in pope97.dat | https://omlc.org/spectra/water/data/pope97.dat |
| §2 418 nm vs dat | 417.5 / 420 nm = 0.00442 / 0.00454 m⁻¹, consistent with the minimum | VERIFIED | | same |
| §2 450 nm | 0.00922 | VERIFIED | 100 × 0.0000922 | same |
| §2 500 nm | 0.0204 | VERIFIED | 100 × 0.000204 | same |
| §2 550 nm | 0.0565 | VERIFIED | 100 × 0.000565 | same |
| §2 600 nm | 0.2224 | VERIFIED | 100 × 0.002224 | same |
| §2 650 nm | 0.340 | VERIFIED | 100 × 0.0034 | same |
| §2 700 nm | 0.624 | VERIFIED | 100 × 0.00624 | same |
| §2 / §8 Table 3 SDs | not opened; not invented | **UNVERIFIABLE** | Journal PDF not opened (Optica blocked). I did not take SDs from a search snippet. The file is right to leave them out. | https://doi.org/10.1364/AO.36.008710 (no PDF this pass) |
| §3 n | 1.333 assumed, conventional visible value, not a measurement opened here | VERIFIED | stated as an assumption | calculation |
| §3 normal incidence | R = 0.0204 (2.04%) | VERIFIED | recomputed 0.020373 | calculation |
| §3 30° | R_unpol = 0.0214; R_s = 0.0309; R_p = 0.0119 | VERIFIED | recomputed 0.02144 / 0.03093 / 0.01194 | calculation |
| §3 60° | 0.0597; 0.1151; 0.0043 | VERIFIED | 0.05969 / 0.11507 / 0.00431 | calculation |
| §3 80° | 0.3479 (34.8%); 0.4570; 0.2388 | VERIFIED | 0.34792 / 0.45702 / 0.23882 | calculation |
| §3 Brewster | 53.12°; R_p = 0; R_unpol = R_s/2 = 0.0392 (3.92%) | **FIX** | Angle is right (arctan(1.333) = 53.123°). **R_unpol = 0.0391 (3.91%)**, not 0.0392 (3.92%). | calculation |
| §5 / O3 Tessendorf method, A, grids, film, example, timing | same as G1 §10 | VERIFIED | opened the 1999–2004 notes for G1; numbers match | https://jtessen.people.clemson.edu/reports/papers_files/coursenotes2004.pdf |
| §5 Finch on FFT | "Moderate-size grids may be processed in real time on consumer-level PCs"; "as of this writing… limited to implementation on the CPU" | VERIFIED | official chapter | https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-1-effective-water-simulation-physical-models |
| §6 Uru / method | used extensively in *Uru: Ages Beyond Myst*; GPU sum of sines / Gerstner-like; geometric mesh plus tiling normal map; L, A, S; "small pond to the ocean as viewed from a cove or island"; "not a rigorous physical simulation" | VERIFIED | | same |
| §6 earlier citations | Isidoro et al. 2002, four sines; Laeuchli 2002, three Gerstner waves | VERIFIED | | same |
| §6 / §7 GPU ms cost | not found in the opened chapter | VERIFIED | also "runs entirely on the GPU… no struggle over CPU usage" is present; still no millisecond figure | same |
| §4 / §8 unchecked colour, foam, Jerlov, Cox & Munk, game talks | correctly left out of the number tables | VERIFIED | including the Berman 1985 / Gieskes 1987 caution, which the file already flags as unchecked | — |
| O1 licence | © OSA/Optica | VERIFIED | as stated; full PDF not opened | DOI record / OMLC citation |
| O2 licence | third-party transcription; no separate licence on the short page | VERIFIED | the opened OMLC page has no CC statement | OMLC page |
| O4 licence | © NVIDIA / Addison-Wesley, official HTML | VERIFIED | | NVIDIA URL |

## Sources

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| O1 DOI | https://doi.org/10.1364/AO.36.008710 | partial (200/202 HTML bot-challenge; journal PDF not opened) | y (© OSA/Optica), from the citation / abstract record |
| O2 data | https://omlc.org/spectra/water/data/pope97.dat | y (200, 2,134 B) | y as a third-party transcription with no separate licence on the page |
| O2 page | https://omlc.org/spectra/water/abs/pope97.html | y (200) | y |
| O3 | https://jtessen.people.clemson.edu/reports/papers_files/coursenotes2004.pdf | y (200) | y (© 1999–2004 Tessendorf; see G1 W6) |
| O4 | https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-1-effective-water-simulation-physical-models | y (200) | y (© NVIDIA / Addison-Wesley) |

## Grok

Same single G1–G3 pass as in the G1 verify file (09:21:41–09:27:48 AEDT, 2026-11-17). I opened or recomputed every Grok claim I used.

**Agreed:** Pope abstract minimum; OMLC dat-file ×100 matches the listed m⁻¹ values; Table 3 SDs not opened; no legal full Pope or Monahan PDF found in that pass either; Finch used in Uru and no millisecond cost; Tessendorf as in G1.

**Grok error I did not adopt:** its Fresnel table (30° 0.0216, 60° 0.0653, Brewster 0.0404). The correct values are those in the file, except the Brewster last-digit fix above. No Grok Fresnel number was taken.
