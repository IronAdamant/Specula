# G1-ocean-waves.md: verification

Checked by: Grok Bot executor (adversarial source check). Access date for every URL below: **2026-11-17** (Australia/Sydney, AEDT).
File checked: `G1-ocean-waves.md`. mtime at start was **2026-11-17 08:58:48 AEDT**, 26,875 bytes. Re-statted before writing and found **unchanged and byte-identical to the start snapshot**. Final mtime verified against: **2026-11-17 08:58:48.231 AEDT**.

## Summary

- **Scope:** Every numbered claim in the G1 tables, the researcher's own calculations, the close-check items World Data flagged, and every URL in §13. I re-integrated the PM spectrum, recomputed the 0.877 factor and the Beaufort-8 PM example, and read key scanned pages as images (JONSWAP pp. 32, 37–39; Hughes printed p. 7; Battjes pp. 467 and 470; PM p. 5187; TMA p. 979).
- **Counts (rows checked: 72):** VERIFIED 66 · FIX 6 · UNVERIFIABLE 0.
- **Worst problems (no wrong published constants; rounding and page/licence wording):**
  1. **Hs = 0.210 U²/g is a rounded-up derived coefficient, not a printed 1964 value.** The integral is correct: m₀ = α U⁴ / (4β g²), so 4√m₀ = 2√(α/β) U²/g = **0.2092** U²/g (α = 8.10×10⁻³, β = 0.74). The file's 0.210 causes the 0.0247 vs WMO 0.0246 mismatch; the unrounded coefficient with the 1.075 height factor and g = 9.81 gives 0.02465, which matches WMO eq. (1.45). Wind height for the integral is the 1964 anemometer height, **19.5 m**.
  2. **WMO eqs. (1.10)–(1.14) and g/2π = 1.56 m s⁻² sit on printed p. 6, not p. 7**, in the 2018 edition opened (same IMGW/on-disk PDF). Eqs. (1.15)–(1.16) are on p. 7.
  3. **Battjes 1974 licence:** the live ICCE record is **CC BY 4.0** (authors retain copyright; first publication licensed CC BY 4.0). The file left this unconfirmed and advised treating it as society copyright.
  4. **Whitecap examples at U = 20 m s⁻¹** in §12 are slightly under-rounded: OLS 0.112 not 0.111; robust 0.105 not 0.104. The U = 10 m s⁻¹ pair (0.0098 vs 0.0099) is right.
- **Close checks:**
  - **PM Hs integral:** not printed in Pierson & Moskowitz 1964. Re-integrated as above. Consistent with WMO H_{m0} = 0.0246 u₁₀² once 0.2092 and the 1.075 reduction are used.
  - **Monahan 1980:** OLS and robust fits confirmed from the publisher abstract. **Full paper not opened** (AMS 403 to curl; University of Galway record is metadata-only, dc.rights CC BY-NC 4.0, no bitstream). Scatter and onset remain unchecked, as the file already says.
  - **Hughes Φ error:** settled from the printed p. 7 page image. The approximation "deviates from Equation 12 by a maximum of 4 percent in the vicinity of ω_h = 1.0 with the error less than 1 percent **outside** the range 0.8 < ω_h < 1.3." The file is right; the earlier Grok reversal was wrong. This pass's Grok CLI also got the direction right.
  - **JONSWAP γ = 3.3, σ_a = 0.07, σ_b = 0.09**, fetch laws (2.4.3)–(2.4.4), TMA form and Fig. 5 parameters, WMO depth classes, Stokes 1/7, Battjes ξ thresholds, and Tessendorf FFT: all match the opened primaries. No tabulated SD for γ (file already says so).
- **Dead / blocked links (the documents themselves were still read from an open copy except as noted):**
  - W5 Wikimedia Commons PDF: **429** to curl today. Same file already on disk (`/tmp/g1/hughes84.pdf`, 46 pp., printed p. 7 read as an image). USACE work; front matter unclassified / approved for public release.
  - W1 DOI and W3 AGU DOI: 500 / 403. Open copies used: Ifremer PM PDF (200) and Max Planck TMA PDF (200).
  - W10 AMS abstract page: **403** today. Abstract text confirmed from the researcher's earlier AMS HTML and from multiple publisher/metadata records. Full PDF still not opened.
- **Rows skipped:** none of the G1 number tables.

## Rows

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| §2 eq. (12) | S(ω)dω = (α g²/ω⁵) exp[−β(ω₀/ω)⁴] dω, 0 to ∞; area = variance | VERIFIED | p. 5187 page image | https://data-ww3.ifremer.fr/COURS/OLD_COURSES/LECTURES/Pierson_Moskowitz_JGR1964.pdf |
| §2 α | 8.10 × 10⁻³ | VERIFIED | p. 5187 | same |
| §2 β | 0.74 | VERIFIED | p. 5187 | same |
| §2 ω₀ | g/U, U = weather-ship wind | VERIFIED | p. 5187 | same |
| §2 anemometer | about 19.5 m; *Weather Explorer* and *Weather Reporter* | VERIFIED | p. 5189; conclusions also "winds measured at 19.5 meters" | same |
| §2 W4 f_p | f_p = 0.877 g/(2π u), u at 19.5 m | VERIFIED | W4 eq. (1.44), p. 20; u in (1.43) is 19.5 m | https://www.imgw.pl/sites/default/files/2021-06/guidetowave-analysisandforecasting.pdf |
| §2 ω_p from β | (4β/5)^{1/4} = 0.877 | VERIFIED | recomputed 0.87716 | from W1 β |
| §2 W4 H_{m0} | 0.0246 u², u at 10 m; 19.5 m divided by 1.075 | VERIFIED | eqs. (1.45) pp. 20–21; footnote 7 on p. 21 | same W4 PDF |
| §2 H_{m0} vs f_p | H_{m0} = 0.04 f_p^{-2} | VERIFIED | eq. (1.46), p. 21 | same W4 PDF |
| §2 Hs from 1964 constants | H_s = 4√m₀ = 0.210 U²/g; m₀ = α U⁴/(4β g²); not printed in 1964; 0.210/g × 1.075² ≈ 0.0247 vs 0.0246 | **FIX** | Integral and m₀ formula are correct, and the 1964 paper does **not** print this Hs law. Coefficient is **2√(α/β) = 0.2092** (report 0.209 U²/g at the 19.5 m height). Then 0.2092/g × 1.075² = 0.02465 (g = 9.81), matching W4's 0.0246. The 0.0247 figure is an artefact of rounding up to 0.210. | W1 PDF (absence) + W4 |
| §2 WMO α | 0.0081 | VERIFIED | eq. (1.43), p. 20; rounded from 8.10×10⁻³ | same W4 PDF |
| §2 420 selected records | W4 p. 20; not counted in W1 by the researcher | VERIFIED | W4 p. 20 does say "a subset of 420 selected wave measurements… 1955 and 1960" | same W4 PDF |
| §3 spectrum | eq. (2.4.1), five-parameter form, p. 32 | VERIFIED | page image p. 32 | TU Delft scan https://repository.tudelft.nl/file/File_db601bf5-107d-4468-a335-d0cb659853e2 (record: https://repository.tudelft.nl/record/uuid:f204e188-13b9-49d8-a6dc-4fb7c20562fc) |
| §3 σ | σ_a for f ≤ f_m; σ_b for f > f_m | VERIFIED | p. 32 | same |
| §3 E_PM | eq. (2.4.2), p. 32 | VERIFIED | | same |
| §3 PM exponent footnote | "normally written as −0.74 (f/f_w)^{-4}" with f_w = g/(2π U_{19}); "the two expressions are identical" | VERIFIED | footnote, p. 32 page image; it is **U_{19}**, not U_{10}. Check 1.25 × 0.877⁴ = 0.739 ≈ 0.74 also recomputed. | same |
| §3 x̃ | g x / U_{10}² | VERIFIED | p. 32 | same |
| §3 f̃_m | 3.5 x̃^{-0.33}, best fit; Mitsuyasu 1969 same exponent | VERIFIED | eq. (2.4.3), p. 37 page image | same |
| §3 α fetch law | 0.076 x̃^{-0.22}; −0.4 better for *their* α slope | VERIFIED | eq. (2.4.4), p. 37 | same |
| §3 mean shape | γ = 3.3, σ_a = 0.07, σ_b = 0.09 | VERIFIED | eq. (2.4.5), p. 37 page image | same |
| §3 scatter of γ, σ | ~20% sampling r.m.s. "considerably smaller than observed"; residual geophysically real; no clear fetch trend; some indication of γ peaking near x̃ = 10³; no tabulated SD | VERIFIED | p. 37 | same |
| §3 ε̃ | 1.6 × 10^{-7} x̃, ε̃ = ∫E(f) df · g² / U^4 | VERIFIED | eq. (2.4.7), p. 38; paper writes U_{10} | same |
| §3 exponent check | 4(−0.33) − (−0.22) + 1 = −0.1, within ±0.2 | VERIFIED | eqs. (2.4.8)–(2.4.9), p. 38 | same |
| §3 W4 growing sea | H_{m0} = 0.0414 f_p^{-2}(f_p u)^{1/3}; f_p = 0.148 H_{m0}^{-0.6} u^{0.2} | VERIFIED | W4 eqs. (1.48)–(1.49), p. 21; Hasselmann et al. 1976 as given by WMO | W4 PDF |
| §4 TMA form | E_TMA(f,H) = E_J(f) φ_K(ω_H) | VERIFIED | W3 eqs. (10), p. 979 page image; also W4 eq. (1.50), p. 22 | https://pure.mpg.de/rest/items/item_2514801_2/component/file_3514893/content |
| §4 ω_H | 2π f (H/g)^{1/2} | VERIFIED | eq. (8), p. 979 | same |
| §4 exact φ_K | ratio of k^{-3} ∂k/∂f at depth H to infinite depth | VERIFIED | eq. (7), p. 979 | same |
| §4 shallow saturated | E_k(f,H) = B g H ω^{-3}/2 for ω_H < 1 | VERIFIED | eq. (9), p. 979 | same |
| §4 Fig. 5 params | f_m = 0.1 Hz, α = 0.01, γ = 3.3, σ_a = 0.07, σ_b = 0.09 | VERIFIED | Fig. 5 caption, p. 979 | same |
| §4 ~2800 spectra | TEXEL, MARSEN, ARSLOE | VERIFIED | abstract, p. 975 | same |
| §4 Hughes iterative Φ | eqs. (12)–(14), p. 7 | VERIFIED | page image, printed p. 7 | local USACE PDF (W5 Wikimedia 429 today) |
| §4 Thompson & Vincent approx | Φ = ½ ω_h² (ω_h ≤ 1); 1 − ½(2−ω_h)² (ω_h > 1) | VERIFIED | eq. (15), p. 7 page image | same |
| §4 Φ error | max 4% near ω_h = 1.0; < 1% *outside* 0.8 < ω_h < 1.3 | VERIFIED | exact sentence on printed p. 7. Direction settled. | same |
| §5 near peak | θ_σ ≈ 19° ± 5° (linear array); pitch-roll ≈ 27° ± 8° (f_m ≈ 0.4 Hz), reasonably close to 31° for cos²θ; Station 10 also ≈ 30° at f = f_m | VERIFIED | W2 §2.5, p. 38 page image | TU Delft scan |
| §5 high frequency | uniform half-plane, θ_σ ≈ 43°, at f > 2 to 3 f_m | VERIFIED | p. 38 | same |
| §5 source-function form | frequency-independent cos²θ; conclusions "rather insensitive" | VERIFIED | p. 39 page image | same |
| §6 Beaufort 0–12 heights | W8/W9 probable and probable-maximum, digit-for-digit | VERIFIED | All 13 force rows match both the JMA 2006.3 table and the Met Office page (W9 prints trailing .0). W8 remark on "open sea, remote from land" and "never be used in the reverse way" confirmed. Wind m/s intervals can differ slightly (W8 force 8 is 17.2–20.7 m s⁻¹; W9 lists 17–21); the file's "digit-for-digit" claim is about the wave-height columns, which do agree. | https://marine.kishou.go.jp/brochure/beaufort-en.pdf ; https://weather.metoffice.gov.uk/guides/coast-and-sea/beaufort-scale |
| §6 PM vs Beaufort 8 example | 0.0246 × 19² ≈ 8.9 m vs probable 5.5 m | VERIFIED | 8.8806 m | W4 + W8 |
| §7 finite depth | ω² = g k tanh(k h); c² = (g/k) tanh(k h) | VERIFIED | W4 eqs. (1.7), (1.9), p. 6 | W4 PDF |
| §7 deep-water limit | ω² = g k; λ = g T²/(2π); c = g T/(2π) | **FIX** | Equations **(1.10)–(1.13) are on printed p. 6**, not p. 7 | W4 PDF |
| §7 SI units | g/2π = 1.56 m s⁻²; λ = 1.56 T² m; c = 1.56 T m s⁻¹ | **FIX** | Printed **p. 6**, not p. 7 | W4 PDF |
| §7 shallow limit | c = √(g h) when h < λ/25 | **FIX** | Eq. **(1.14) is at the foot of p. 6**, not p. 7 | W4 PDF |
| §7 transitional | c = c₀ tanh(k₀ h), λ = λ₀ tanh(k₀ h) | VERIFIED | eqs. (1.15)–(1.16), p. 7 | W4 PDF |
| §7 theoretical deep | displacements < 4% below λ/2; called deep when h ≥ λ/2 | VERIFIED | §1.2.5, p. 6 | W4 PDF |
| §7 practical classes | deep h > λ/4 (~20% at λ/4); transitional λ/25 < h < λ/4; shallow h < λ/25 | VERIFIED | p. 6 | W4 PDF |
| §7 group velocity | half the phase speed in deep water | VERIFIED | p. 5 ("worth noting here… half the phase speed"); §1.3.2 begins p. 11 | W4 PDF |
| §8 Stokes | H/λ = 1/7; 120° crest; 7H/T = λ/T | VERIFIED | §1.2.7, pp. 8–9, citing Stokes 1847, 1880 | W4 PDF |
| §8 observed steepness | seldom greater than one tenth; more trochoidal | VERIFIED | §1.2.7, pp. 8–9 | W4 PDF |
| §8 depth-limited | h_b = 1.28 H_b (γ_b ≈ 0.78) | VERIFIED | eq. (1.17), p. 9; no author named on that line | W4 PDF |
| §8 McCowan 0.78 | correctly left unchecked | VERIFIED | not used as a verified number, as stated | — |
| §8 Iribarren ξ | ξ = tan α / √(H/L₀) | VERIFIED | W7 eq. (5), p. 467 page image | https://icce-ojs-tamu.tdl.org/icce/article/download/2921/2586 (DOI 10.9753/icce.v14.26 → article/view/2921) |
| §8 ξ₀ types | surging/collapsing ξ₀ > 3.3; plunging 0.5 < ξ₀ < 3.3; spilling ξ₀ < 0.5; slopes 1:5, 1:10, 1:20 | VERIFIED | p. 470 page image | same |
| §8 ξ_b types | > 2.0 / 0.4–2.0 / < 0.4 | VERIFIED | p. 470 | same |
| §9 OLS whitecap | W = 2.95 × 10^{-6} U^{3.52} | VERIFIED | W10 abstract (U at 10 m). Full paper not opened. | AMS abstract (403 today); text confirmed from the publisher abstract |
| §9 robust | W = 3.84 × 10^{-6} U^{3.41} | VERIFIED | same abstract | same |
| §9 "better than any previously published" | as in the abstract | VERIFIED | | same |
| §12 W(U) examples | at U = 10: 0.0098 vs 0.0099; at U = 20: 0.111 vs 0.104 | **FIX** | U = 10 is right. At **U = 20 m s⁻¹: OLS 0.112, robust 0.105** (recomputed). | from W10 abstract coefficients |
| §10 P_h(k) | A exp[−1/(k L)²] / k⁴ × \|k̂ · ŵ\|² | VERIFIED | W6 eq. (40) | https://jtessen.people.clemson.edu/reports/papers_files/coursenotes2004.pdf |
| §10 L | V²/g; ŵ = wind direction | VERIFIED | | same |
| §10 A | "a numeric constant"; no default | VERIFIED | | same |
| §10 small-wave cutoff | exp(−k² ℓ²) | VERIFIED | eq. (41) | same |
| §10 Fourier grid | N, M = 16…2048, powers of two; 128–512 often enough; 1024 and 2048 for extreme detail; above 2048 floating-point accuracy | VERIFIED | "How big should the Fourier grid be?" | same |
| §10 film examples | Waterworld and Titanic 2048², ~3 cm | VERIFIED | | same |
| §10 facet size | dx, dz "need never go below 2 cm or so" | VERIFIED | | same |
| §10 patch size | "10 meters to 2" wrapping to "kilometers" | VERIFIED | full sentence is "10 meters to 2 kilometers on a side" | same |
| §10 worked example | 512², 1000 m, V = 31 m s⁻¹, ℓ = 1 m | VERIFIED | first example set | same |
| §10 timing | "nearly interactive" 512² on a 1+ GHz CPU; no millisecond figure | VERIFIED | | same |
| §10 dispersion | ω² = g k; g k tanh(k D); g k (1 + k² L²) | VERIFIED | eqs. (31)–(33) | same |
| §0 / §1 Battjes licence | "not confirmed… Treat as publisher/society copyright until re-checked" (Grok had reported CC BY 4.0) | **FIX** | Live ICCE record **is CC BY 4.0**: "Authors retain copyright and grant the Proceedings right of first publication… licensed under a Creative Commons Attribution License." Official PDF https://icce-ojs-tamu.tdl.org/icce/article/download/2921/2586 is the same 165,899-byte file already in `/tmp/g1/battjes74.pdf`. | https://icce-ojs-tamu.tdl.org/icce/article/view/2921 |
| W4 licence wording | © WMO 2018; short extracts if the complete source is indicated; ISBN 978-92-63-10702-2 | VERIFIED | verso of the 2018 edition | W4 PDF |
| W5 public domain | US Government work | VERIFIED | USACE CERC-84-7; unclassified front matter | local USACE PDF |
| W6 licence | © 1999–2004 Jerry Tessendorf; no CC | VERIFIED | | W6 PDF |

## Sources

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| W1 copy | https://data-ww3.ifremer.fr/COURS/OLD_COURSES/LECTURES/Pierson_Moskowitz_JGR1964.pdf | y (200, PDF 523,559 B) | y (© AGU 1964; Ifremer hosts a course copy) |
| W1 DOI | https://doi.org/10.1029/JZ069i024p05181 | n (500 today) | — |
| W2 record | https://repository.tudelft.nl/record/uuid:f204e188-13b9-49d8-a6dc-4fb7c20562fc | y (200) | y, with a tighter reuse note: TU Delft says "Other than for strictly personal use, it is not permitted to download, forward or distribute the text" and "License info not available" (not CC) |
| W2 PDF | https://repository.tudelft.nl/file/File_db601bf5-107d-4468-a335-d0cb659853e2 | y | same |
| W3 MPG | https://pure.mpg.de/rest/items/item_2514801_2/component/file_3514893/content | y (200, PDF 1,561,531 B) | y (© AGU 1985) |
| W3 DOI | https://doi.org/10.1029/JC090iC01p00975 | n (403) | — |
| W4 record | https://library.wmo.int/records/item/31871-guide-to-wave-analysis-and-forecasting | y (200) | y |
| W4 PDF | https://www.imgw.pl/sites/default/files/2021-06/guidetowave-analysisandforecasting.pdf | y (200, 13,769,849 B) | y (© WMO 2018; short extracts with source; ISBN 978-92-63-10702-2) |
| W4 DOI | https://doi.org/10.25607/OBP-1523.2 | y (200, Ocean Best Practices handle) | y |
| W5 Wikimedia | https://upload.wikimedia.org/wikipedia/commons/1/12/The_TMA_shallow-water_spectrum_description_and_applications_-_USACE-p266001coll1-7361.pdf | n (429 today) | y, from the USACE PDF itself (public domain / public release) |
| W6 | https://jtessen.people.clemson.edu/reports/papers_files/coursenotes2004.pdf | y (200, PDF 2,785,653 B) | y |
| W7 DOI / record | https://doi.org/10.9753/icce.v14.26 → https://icce-ojs-tamu.tdl.org/icce/article/view/2921 | y (200) | **FIX:** live record is CC BY 4.0, not "unconfirmed" |
| W7 PDF | https://icce-ojs-tamu.tdl.org/icce/article/download/2921/2586 | y (200, 165,899 B) | CC BY 4.0 |
| W8 | https://marine.kishou.go.jp/brochure/beaufort-en.pdf | y (200, 171,766 B) | y (official JMA table) |
| W9 | https://weather.metoffice.gov.uk/guides/coast-and-sea/beaufort-scale | y (200) | y (official Met Office table; page also © Crown Copyright) |
| W10 abstract | https://journals.ametsoc.org/view/journals/phoc/10/12/1520-0485_1980_010_2094_opldoo_2_0_co_2.xml | n (403 today) | y as © AMS; Galway item metadata also lists CC BY-NC 4.0 but is metadata-only (no full-text bitstream) |
| W10 DOI | https://doi.org/10.1175/1520-0485(1980)010<2094:OPLDOO>2.0.CO;2 | n (500 today) | — |

## Grok

One live pass covering G1–G3, as required: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve --prompt-file /tmp/lv17g/grok_prompt.md --output-format plain`. Launched **09:21:41 AEDT** 2026-11-17; process exited **09:27:48 AEDT**; empty stderr; raw output `/tmp/lv17g/grok.out` (16,606 bytes). Live web search was on. A Grok number was used only where I then opened the cited source (or recomputed it) myself.

**Agreed with the file, and useful:** Hs = 0.21 U²/g is **not** printed in the 1964 paper; WMO 1.075 height factor and 0.0246 at 10 m; JONSWAP means and fetch laws; no tabulated γ SD; Hughes Φ error direction (this pass got it right: max 4% near ω_h = 1, < 1% outside 0.8–1.3); WMO depth classes and Stokes / 1.28; Battjes thresholds; Battjes **CC BY 4.0**; Monahan abstract coefficients; Beaufort heights agree; Tessendorf grid, timing, and no default A; TMA "about 2800" in the abstract.

**Grok errors I did not adopt:** its Fresnel numbers (see G3); JONSWAP mean shape cited as "~p. 32" (it is eq. (2.4.5) on **p. 37**); it did not open the Bouws et al. 1985 full PDF (I did, Max Planck copy); it did not open EM 25 or the 1993 Montgomery & Buffington slope bins (I did; see G2).
