# G1: waves on open water — spectra, height against wind, breaking and foam

Item G1 of `LIBRARY_RESEARCH_BRIEF.md` (§G Water). Compiled 2026-11-17 (Australia/Sydney). Private working file: nothing here was posted, published or committed.

## Fixes after verify (2026-11-17, Australia/Sydney, AEDT)

Applied from `G1-ocean-waves.md.verify.md`. The PM coefficient and the Monahan examples were recomputed before editing; WMO page numbers were confirmed in the 2018 edition PDF (printed p. 6); the Battjes licence was confirmed on the live ICCE record.

1. **PM Hs:** H_s = **0.209 U²/g** at the 1964 anemometer height of 19.5 m, from 2√(α/β) = 0.2092 (α = 8.10×10⁻³, β = 0.74). With the 1.075 height factor and g = 9.81, 0.2092/g × 1.075² = 0.02465, matching W4's 0.0246. The earlier 0.0247 came from rounding the coefficient up to 0.210. The 1964 paper does not print this Hs law.
2. **WMO dispersion pages:** eqs. (1.10)–(1.14) and g/2π = 1.56 m s⁻² are on printed **p. 6**, not p. 7. Eqs. (1.15)–(1.16) stay on p. 7.
3. **Monahan examples at U = 20 m s⁻¹:** OLS **0.112**, robust **0.105** (W remains the fraction of sea surface). The U = 10 m s⁻¹ pair (0.0098 vs 0.0099) is unchanged. Fits still come from the publisher abstract only; the full paper was not opened (AMS 403).
4. **Battjes 1974 licence:** the live ICCE record is **CC BY 4.0** (authors retain copyright; first publication licensed CC BY 4.0).

## 0. Read this first

- **Source keys.** Every number has a key (W1, W2, …) with a full entry in §12. Each entry gives a URL and the access date, 2026-11-17.
- **Kind of number.** Each row is tagged as one of:
  - **meas**: field or lab measurement, or a regression fitted to measurements.
  - **desc**: a range or recommended value from a reference work.
  - **calc**: a number I computed from a published formula. The method is stated in the row.
- **Verification marks.**
  - ✔: I read the number in the primary text, from the PDF via pdftotext or from the page image.
  - **[OCR]**: the text came from a scanned PDF and its OCR layer. Garbled digits were checked on the page image.
  - **[IMG]**: I read the number from the rendered page image.
  - ⚠: doubtful as printed. All ⚠ items are listed in §11.
- **Method.** WebSearch, WebFetch, curl, pdftotext and page images. Scratch copies are in `/tmp/g1/` (never in a repo). **A Grok live-search pass was run** (Grok CLI, grok-4.6, reasoning effort xhigh, live web search; prompt `/tmp/grokrun/g1b.txt`; launched 08:49 AEDT 2026-11-17; process exited 08:54 AEDT; raw output `/tmp/grokrun/g1b.out`). A first pass launched 16 Nov (`/tmp/grokrun/g1.out`) failed when usage ran out (HTTP 402) and was not used. Every Grok claim used here was re-opened by me in the primary source and is marked "via Grok ✔". Grok claims I could not open, and errors I caught, are in §11. Conflicts are in §12.
- **Licences.**
  - US Government works (public domain): Hughes, CERC-84-7 (W5).
  - WMO-No. 702 (W4) is © World Meteorological Organization, 2018. Short extracts may be reproduced if the complete source is clearly indicated.
  - JMA Beaufort table (W8) and the Met Office Beaufort page (W9) are official meteorological-service tables.
  - Battjes 1974 (W7): ICCE proceedings; DOI 10.9753/icce.v14.26. The live record is CC BY 4.0 (authors retain copyright; first publication licensed under a Creative Commons Attribution License, https://creativecommons.org/licenses/by/4.0/).
  - Tessendorf course notes (W6): copyright 1999–2004 Jerry Tessendorf; no Creative Commons licence stated.
  - Everything else is publisher copyright (AGU/Wiley, Deutsche Hydrographische Zeitschrift/BSH, AMS). From those sources I use facts and numbers only, with no copied figures or wording.

## 1. Sources and licences

| Key | Source | Gives | Licence |
|---|---|---|---|
| W1 | Pierson & Moskowitz (1964) JGR 69(24):5181–5190 | PM spectrum, eq. (12); α, β, ω₀=g/U; wind at 19.5 m; data 20–40 kn | © AGU 1964. Copy read: Ifremer course PDF |
| W2 | Hasselmann et al. (1973) Ergänzungsheft zur Deutschen Hydrographischen Zeitschrift, Reihe A(8°), Nr. 12, 95 pp. | JONSWAP five-parameter form; fetch laws; mean γ, σ_a, σ_b; directional notes | © 1973 JONSWAP group / DHI. Copy read: ResearchGate scan [OCR], page images [IMG] |
| W3 | Bouws, Günther, Rosenthal & Vincent (1985) JGR 90(C1):975–986 | TMA spectrum = JONSWAP × φ_K; ~2800 spectra (TEXEL, MARSEN, ARSLOE) | © AGU 1985. Copy read: Max Planck Pure publisher PDF [IMG] |
| W4 | WMO (2018) Guide to Wave Analysis and Forecasting, 3rd ed., WMO-No. 702 | PM/JONSWAP/TMA as used in forecasting; dispersion; depth classes; Stokes 1/7; h_b=1.28 H_b; Beaufort wind conversion (not wave heights) | © WMO 2018. Short extracts allowed with source. ISBN 978-92-63-10702-2 |
| W5 | Hughes (1984) CERC-84-7, *The TMA Shallow-Water Spectrum* | Iterative Φ(2πf,h); Thompson & Vincent 1983 piecewise approximation and its error | US Government work, public domain |
| W6 | Tessendorf, *Simulating Ocean Water*, SIGGRAPH course notes, copyright 1999–2004 | Phillips-type P_h(k); FFT grid sizes; Waterworld/Titanic 2048²; timing | © 1999–2004 Jerry Tessendorf. No CC stated |
| W7 | Battjes (1974) "Surf Similarity", Proc. 14th ICCE, 466–480 | Iribarren/surf-similarity ξ; breaker-type thresholds from Galvin's data | CC BY 4.0 (authors retain copyright; first publication licensed CC BY 4.0) |
| W8 | Japan Meteorological Agency, Beaufort Scale of Wind Force (table dated 2006.3) | Probable and probable-maximum open-sea wave heights by Beaufort force | Official JMA table. Guide values only (see the table remark) |
| W9 | UK Met Office, Beaufort wind force scale (operational marine-forecast table) | Same probable / probable-maximum heights as W8 | Official Met Office table |
| W10 | Monahan & O'Muircheartaigh (1980) J. Phys. Oceanogr. 10:2094–2099 | W = a U^b whitecap coverage, OLS and robust fits | © AMS. Numbers taken from the publisher abstract ✔ |

## 2. Pierson–Moskowitz (fully developed sea)

The 1964 spectrum is fitted to fully developed North Atlantic seas from Moskowitz (1964), shipborne wave records on British ocean weather ships, winds 20–40 knots (10.29–20.58 m s⁻¹) measured at about 19.5 m (W1 abstract and p. 5189 ✔).

| Quantity | Value (spread) | Kind | Source |
|---|---|---|---|
| Spectrum (angular frequency) | S(ω) dω = (α g² / ω⁵) exp[−β (ω₀/ω)⁴] dω, defined from 0 to ∞; area = variance of the wave record | meas (fitted form) | W1 eq. (12), p. 5187 [IMG] ✔ |
| α (Phillips coefficient) | 8.10 × 10⁻³ (dimensionless) | meas | W1 p. 5187 [IMG] ✔ |
| β | 0.74 (dimensionless) | meas | W1 p. 5187 [IMG] ✔ |
| ω₀ | g/U, U = weather-ship wind | desc | W1 p. 5187 [IMG] ✔ |
| Anemometer height | about 19.5 m above the sea surface (*Weather Explorer* and *Weather Reporter*) | meas | W1 p. 5189 ✔ |
| Peak frequency (Hz), WMO form | f_p = 0.877 g / (2π u), u at 19.5 m | desc | W4 eq. (1.44), p. 20 ✔ |
| Equivalent ω_p from β | ω_p = (4β/5)^{1/4} ω₀ = 0.877 g/U | calc | from W1 β = 0.74. (4×0.74/5)^{1/4} = 0.877 |
| Significant wave height, WMO, 10 m wind | H_{m0} = 0.0246 u² (H_{m0} in m, u in m s⁻¹ at 10 m) | desc | W4 eq. (1.45), p. 20–21 ✔. Footnote 7: 19.5 m wind is divided by 1.075 to reduce it to 10 m |
| H_{m0} against peak frequency, fully developed | H_{m0} = 0.04 f_p^{-2} (H_{m0} in m, f_p in Hz) | desc | W4 eq. (1.46), p. 21 ✔ |
| Hs from the 1964 constants, U at 19.5 m | H_s = 4√m₀ = 0.209 U²/g (my integration of eq. 12: m₀ = α U⁴ / (4β g²); 2√(α/β) = 0.2092) | calc | from W1. Not printed in the 1964 paper. Wind height is the 1964 anemometer height, 19.5 m. With the 1.075 height factor and g = 9.81, 0.2092/g × 1.075² = 0.02465, matching W4's 0.0246 |
| WMO α | 0.0081 | desc | W4 eq. (1.43), p. 20 ✔. Rounded from 8.10 × 10⁻³ |

W4 also states that the 1964 model used a subset of 420 selected shipborne-wave-recorder measurements, 1955–1960 (W4 p. 20 ✔). That count is not in the pages of W1 that I opened; I have not independently counted Moskowitz's table.

## 3. JONSWAP (fetch-limited growth)

Hasselmann et al. (W2) parametrized one-dimensional spectra from the 1969 North Sea fetch experiment as a five-parameter form.

| Quantity | Value (spread) | Kind | Source |
|---|---|---|---|
| Spectrum | E(f) = α g² (2π)^{-4} f^{-5} exp[−(5/4)(f/f_m)^{-4}] γ^{exp[−(f−f_m)²/(2σ² f_m²)]} | meas (fitted form) | W2 eq. (2.4.1), p. 32 [IMG] ✔ |
| σ | σ_a for f ≤ f_m; σ_b for f > f_m | desc | W2 p. 32 [IMG] ✔ |
| Corresponding PM shape (same α, f_m) | E_PM(f) = α g² (2π)^{-4} f^{-5} exp[−(5/4)(f/f_m)^{-4}] | desc | W2 eq. (2.4.2), p. 32 [IMG] ✔ |
| Note on the PM exponent | The PM form is "normally written as −0.74 (f/f_w)^{-4}" with f_w = g/(2π U_{19}); "the two expressions are identical" when f_m is tied to f_w, but here f_m is a free parameter | desc | W2 p. 32 footnote [IMG] ✔. Check: 1.25 × 0.877⁴ = 0.739 ≈ 0.74 |
| Nondimensional fetch | x̃ = g x / U_{10}² (x = fetch) | desc | W2 p. 32 ✔ |
| Peak frequency (nondimensional) | f̃_m = 3.5 x̃^{-0.33} (best fit; Mitsuyasu 1969 finds the same exponent) | meas | W2 eq. (2.4.3), p. 37 [IMG] ✔ |
| α against fetch | α = 0.076 x̃^{-0.22} (dynamical scaling, §2.8; reasonable overall fit to field and tank data). An exponent of −0.4 is a better match to the average slope of *their* α distribution | meas | W2 eq. (2.4.4), p. 37 [IMG] ✔ |
| Mean shape parameters | γ = 3.3, σ_a = 0.07, σ_b = 0.09 | meas (means) | W2 eq. (2.4.5), p. 37 [IMG] ✔ |
| Scatter of γ, σ_a, σ_b | Statistical sampling of a narrow peak accounts for only about 20% r.m.s. relative variability, "considerably smaller than observed." Residual scatter is described as geophysically real. No clear fetch trend; some indication of γ peaking around x̃ = 10³ | meas | W2 p. 37 [IMG] ✔. A single standard deviation for γ was not tabulated on the pages I opened |
| Nondimensional energy | ε̃ = 1.6 × 10^{-7} x̃, where ε̃ = ∫E(f) df · g² / U^4 | meas | W2 eq. (2.4.7), p. 38 ✔ |
| Consistency of the three exponents | 4(−0.33) − (−0.22) + 1 = −0.1, within an estimated error of about ±0.2 | calc (theirs) | W2 eqs. (2.4.8)–(2.4.9), p. 38 ✔ |
| Growing-sea H_{m0}–f_p–u (Hasselmann et al. 1976, as given by WMO) | H_{m0} = 0.0414 f_p^{-2} (f_p u)^{1/3}; inverted f_p = 0.148 H_{m0}^{-0.6} u^{0.2} (H_{m0} in m, f_p in Hz, u at 10 m in m s⁻¹) | desc | W4 eqs. (1.48)–(1.49), p. 21 ✔. Not the fully developed pair (1.45)–(1.46) |

## 4. TMA (finite depth)

Bouws et al. (W3) extend the JONSWAP shape into finite depth with the Kitaigorodskii et al. (1975) factor, and test it on about 2800 spectra from TEXEL, MARSEN and ARSLOE (W3 abstract ✔; Table 1 lists the stations, p. 977 [IMG] ✔).

| Quantity | Value (spread) | Kind | Source |
|---|---|---|---|
| TMA form | E_TMA(f, H) = E_J(f) φ_K(ω_H), H = depth | desc | W3 eqs. (10), p. 979 [IMG] ✔. Same statement: W4 eq. (1.50), p. 22 ✔ |
| Dimensionless frequency | ω_H = 2π f (H/g)^{1/2} | desc | W3 eq. (8), p. 979 [IMG] ✔ |
| Exact φ_K | ratio of k^{-3} ∂k/∂f at depth H to the same at infinite depth | desc | W3 eq. (7), p. 979 [IMG] ✔ |
| Shallow saturated range (ω_H < 1) | E_k(f,H) = B g H ω^{-3} / 2 | desc | W3 eq. (9), p. 979 [IMG] ✔ |
| Example JONSWAP parameters in their Fig. 5 | f_m = 0.1 Hz, α = 0.01, γ = 3.3, σ_a = 0.07, σ_b = 0.09, several depths | desc | W3 Fig. 5 caption, p. 979 [IMG] ✔ |
| Iterative Φ (linear theory), Hughes | Φ(2πf,h) = [R(ω_h)]^{-2} {1 + 2ω_h² R(ω_h) / sinh[2 ω_h² R(ω_h)]}^{-1}, with ω_h = 2πf (h/g)^{1/2} and R tanh(ω_h² R) = 1 | desc | W5 eqs. (12)–(14), p. 7 [IMG] ✔ |
| Thompson & Vincent (1983) approximation | Φ = ½ ω_h² for ω_h ≤ 1; Φ = 1 − ½(2 − ω_h)² for ω_h > 1 | desc | W5 eq. (15), p. 7 [IMG] ✔ |
| Error of that approximation against eq. (12) | maximum 4% near ω_h = 1.0; error less than 1% *outside* the range 0.8 < ω_h < 1.3 | desc | W5 p. 7 [IMG] ✔. Grok had this backwards (see §10) |

## 5. Directional spreading

| Quantity | Value (spread) | Kind | Source |
|---|---|---|---|
| JONSWAP, near the spectral peak | Linear-array mean beam-width θ_σ ≈ 19° ± 5°; equivalent pitch-roll beam-width ≈ 27° ± 8° (f_m ≈ 0.4 Hz), "reasonably close" to 31° for a cos²θ distribution. Pitch-roll buoy at Station 10 also ≈ 30° at f = f_m | meas | W2 §2.5, p. 38 ✔ |
| JONSWAP, high frequency | Changes gradually toward approximately uniform half-plane radiation, θ_σ ≈ 43°, at f > 2 to 3 f_m | meas | W2 p. 38 ✔ |
| Form used in their source-function calculations | Frequency-independent cos²θ, for simplicity, because most energy is near the peak; conclusions were "rather insensitive" to the precise spreading | desc | W2 p. 39 ✔ |

Mitsuyasu (1975) s_max, Hasselmann, Dunckel & Ewing (1980) s(f), and Donelan, Hamilton & Hui (1985) sech²β were not opened in the primary papers this pass. They are listed in §10. W2 itself already gives a measured spreading function for the JONSWAP fetch cases.

## 6. Wave height against wind, and the Beaufort table

W4 Table 2.1 (p. 26–27 ✔) is a *wind-speed* conversion, not a wave-height table. It has two columns that disagree (see §11): a "scale recommended for use in wave forecasting" (WMO 1970) and the "range of values reported by observers" (WMO 2012).

Probable and probable-maximum wave heights come from operational meteorological-service tables (W8, W9), not from the WMO-No. 558 appendix I opened via Grok, which has wind equivalents and sea criteria only. W8 and W9 agree digit-for-digit. W8's remark: the heights are only a guide to what may be expected "in the open sea, remote from land" and "should never be used in the reverse way" for logging sea state; in enclosed or near-land waters with an offshore wind, waves are smaller and steeper. Figures in brackets are the probable maximum.

| Beaufort | Descriptive term (W8) | Wind at 10 m, knots / m s⁻¹ (W8) | Probable height (m) | Probable maximum (m) | Kind | Source |
|---|---|---|---|---|---|---|
| 0 | Calm | <1 / 0–0.2 | — | — | desc | W8 ✔; W9 ✔ |
| 1 | Light air | 1–3 / 0.3–1.5 | 0.1 | 0.1 | desc | W8 ✔; W9 ✔ |
| 2 | Light breeze | 4–6 / 1.6–3.3 | 0.2 | 0.3 | desc | W8 ✔; W9 ✔ |
| 3 | Gentle breeze | 7–10 / 3.4–5.4 | 0.6 | 1 | desc | W8 ✔; W9 ✔ |
| 4 | Moderate breeze | 11–16 / 5.5–7.9 | 1 | 1.5 | desc | W8 ✔; W9 ✔ |
| 5 | Fresh breeze | 17–21 / 8.0–10.7 | 2 | 2.5 | desc | W8 ✔; W9 ✔ |
| 6 | Strong breeze | 22–27 / 10.8–13.8 | 3 | 4 | desc | W8 ✔; W9 ✔ |
| 7 | Near gale | 28–33 / 13.9–17.1 | 4 | 5.5 | desc | W8 ✔; W9 ✔ |
| 8 | Gale | 34–40 / 17.2–20.7 | 5.5 | 7.5 | desc | W8 ✔; W9 ✔ |
| 9 | Strong gale | 41–47 / 20.8–24.4 | 7 | 10 | desc | W8 ✔; W9 ✔ |
| 10 | Storm | 48–55 / 24.5–28.4 | 9 | 12.5 | desc | W8 ✔; W9 ✔ |
| 11 | Violent storm | 56–63 / 28.5–32.6 | 11.5 | 16 | desc | W8 ✔; W9 ✔ |
| 12 | Hurricane | 64 and over / 32.7 and over | 14 or over | — | desc | W8 ✔; W9 ✔ |

These are not the same as fully developed H_{m0} = 0.0246 u_{10}² (W4). Example: Beaufort 8 midpoint ~19 m s⁻¹ would give H_{m0} ≈ 8.9 m if fully developed, against a probable height of 5.5 m in the table — the Beaufort heights are a sea-state guide, not a fully developed PM forecast.

## 7. Dispersion and depth classes

From linearized theory, as given by W4.

| Quantity | Value | Kind | Source |
|---|---|---|---|
| Finite depth | ω² = g k tanh(k h); equivalently c² = (g/k) tanh(k h) | desc | W4 eqs. (1.7), (1.9), p. 6–7 ✔ |
| Deep-water limit | ω² = g k; c² = g/k = gλ/(2π); λ = g T² / (2π); c = g T / (2π) | desc | W4 eqs. (1.10)–(1.13), p. 6 ✔ |
| In SI units | g/2π = 1.56 m s⁻², so λ = 1.56 T² m and c = 1.56 T m s⁻¹ | calc (theirs) | W4 p. 6 ✔ |
| Shallow-water limit | c = √(g h) when h < λ/25 | desc | W4 eq. (1.14), p. 6 ✔ |
| Transitional approximations | c = c₀ tanh(k₀ h), λ = λ₀ tanh(k₀ h) | desc | W4 eqs. (1.15)–(1.16), p. 7 ✔ |
| Deep water, theoretical | below a depth of λ/2, particle displacements are less than 4% of those at the surface; water is called deep when h is at least λ/2 | desc | W4 §1.2.5, p. 6 ✔ |
| Classes used in practice in W4 | Deep: h > λ/4 (displacements at λ/4 about 20% of surface; speed still close to the deep-water speed). Transitional: λ/25 < h < λ/4. Shallow: h < λ/25 | desc | W4 p. 6 ✔ |
| Group velocity, deep water | half the phase speed | desc | W4 p. 5 and §1.3.2 ✔ |

The common textbook cut h/L > 0.5 (deep) and h/L < 0.05 (shallow) is therefore not the practical classification in W4. See §11.

## 8. Wave breaking

| Quantity | Value (spread) | Kind | Source |
|---|---|---|---|
| Stokes steepness limit | H/λ = 1/7; at that limit the crest angle is 120° and the forward particle speed would equal the phase speed (7H/T = λ/T) | desc | W4 §1.2.7, p. 8–9 ✔, citing Stokes 1847, 1880 |
| Observed steepness | "seldom greater than one tenth of the wavelength"; the profile is then more trochoidal than sinusoidal | desc | W4 §1.2.7, p. 8–9 ✔ |
| Depth-limited breaking (WMO) | h_b = 1.28 H_b, i.e. H_b / h_b ≈ 0.781 | desc | W4 eq. (1.17), p. 9 ✔. No author is named on that line |
| McCowan 0.78 | Often cited as the solitary-wave limit H/h = 0.78. The 1894 *Philosophical Magazine* paper was not opened by me (Zenodo copy returned 403). Grok reported eq. (34) as "c − h = 0.78 h" and an experimental average 0.75 h from earlier work; year sometimes given as 1891 in CEM. **Unchecked** — see §10 | — | not used as a verified number |
| Iribarren / surf-similarity parameter | ξ = tan α / √(H/L₀), α = beach slope, H and L₀ the deep-water height and wavelength | desc | W7 eq. (5), p. 467 [IMG] ✔ |
| Breaker type, offshore ξ₀ (Battjes, from Galvin's data) | surging or collapsing if ξ₀ > 3.3; plunging if 0.5 < ξ₀ < 3.3; spilling if ξ₀ < 0.5. Based on experiments on slopes 1:5, 1:10 and 1:20 | meas (re-analysis) | W7 p. 470 [IMG] ✔ |
| Approximate inshore ξ_b | surging or collapsing if ξ_b > 2.0; plunging if 0.4 < ξ_b < 2.0; spilling if ξ_b < 0.4 | meas (re-analysis) | W7 p. 470 [IMG] ✔ |

The original Miche (1944/1951) limit H/L = 0.14 tanh(kh), and the CEM "modified Miche" form that Grok quoted as H_{mo,b} = 0.1 L tanh(k d), were not opened by me in those sources. Not used.

## 9. Whitecap coverage

Monahan & O'Muircheartaigh (W10), publisher abstract, combined the whitecap data of Monahan (1971) and Toba & Chaen (1973). W is the fraction of sea surface covered by whitecaps; U is the 10 m wind speed in m s⁻¹.

| Quantity | Value | Kind | Source |
|---|---|---|---|
| Ordinary least squares | W = 2.95 × 10^{-6} U^{3.52} | meas | W10 abstract ✔ |
| Robust biweight fit | W = 3.84 × 10^{-6} U^{3.41} | meas | W10 abstract ✔ |
| Claim in the abstract | These "fit the combined data set better than any of the previously published equations" | desc | W10 abstract ✔ |

The scatter of W at a given wind, the onset wind, and later fits (Callaghan et al. 2008; Anguelova & Webster 2006) were reported by Grok from papers I did not open. They are listed in §10 and not used as verified numbers. The two Monahan expressions already disagree at the same U (see §11).

## 10. Tessendorf FFT (graphics course notes)

Not an oceanographic spectrum paper. It is the usual computer-graphics reference for a Phillips-type Fourier height field (W6, copyright 1999–2004).

| Quantity | Value | Kind | Source |
|---|---|---|---|
| Height-field spectrum | P_h(k) = A exp[−1/(k L)²] / k⁴ × \|k̂ · ŵ\|² | desc | W6 eq. (40) ✔ |
| L | V²/g, V = wind speed; ŵ = wind direction | desc | W6 ✔ |
| A | "a numeric constant." No default value is given | desc | W6 ✔ |
| Small-wave cutoff | extra factor exp(−k² ℓ²), ℓ a small length | desc | W6 eq. (41) ✔ |
| Fourier grid | N, M between 16 and 2048, powers of two. 128–512 sufficient for many cases; 1024 and 2048 for extremely detailed surfaces. Above 2048, floating-point accuracy can become noticeable | desc | W6, "How big should the Fourier grid be?" ✔ |
| Film examples | Waterworld and Titanic: 2048 × 2048, spacing about 3 cm | desc | W6 ✔ |
| Practical facet size | dx, dz "need never go below 2 cm or so" | desc | W6 ✔ |
| Patch size | "In practical applications, patch sizes vary from 10 meters to 2" km (sentence wraps at 2) | desc | W6 ✔ |
| Worked example | M = N = 512, L_x = L_z = 1000 m, V = 31 m s⁻¹ (gale), ℓ = 1 m | desc | W6, first example set ✔ |
| Timing (published) | "Running on a 1+ GHz cpu, 512 × 512 FFTs can be generated at nearly interactive rates." No millisecond figure | desc | W6 ✔ |
| Deep-water dispersion (used for the FFT phases) | ω² = g k; finite depth ω² = g k tanh(k D); capillaries ω² = g k (1 + k² L²) | desc | W6 eqs. (31)–(33) ✔ |

## 11. Not found / not checked against the primary source

1. **Miche's original limit** (1944/1951) and the CEM modified-Miche formula were not opened. Not used.
2. **McCowan (1894)** was not opened (the Zenodo PDF returned 403). Grok's 0.78 / 0.75 reading and the 1891 vs 1894 year conflict are unchecked. W4's h_b = 1.28 H_b is used instead, as read.
3. **JONSWAP γ standard deviation.** W2 states that residual scatter exceeds the ~20% sampling r.m.s. and may peak near x̃ = 10³. I did not extract a numerical SD or a min–max from Fig. 2.8. Ranges such as "γ = 1–7" were not taken from a primary page.
4. **Mitsuyasu (1975) s_max, Hasselmann, Dunckel & Ewing (1980) s(f), Donelan, Hamilton & Hui (1985) sech² β.** Abstracts only, or not opened. Not used. Grok also quoted Ewans (1998) for the 1980 formulae; that is secondary.
5. **Callaghan et al. (2008)** piecewise W(U), onset 3.70 m s⁻¹, 43 158 images / 107 points, U from 3.70 to 23.09 m s⁻¹: Grok only, PDF not opened (AGU 403). Not used.
6. **Anguelova & Webster (2006)** literature onset ~3 m s⁻¹ and W scatter of 2–3 orders of magnitude: Grok only, from a third-party PDF. Not used.
7. **DNV-RP-C205** spectrum block: not opened (paywalled).
8. **WMO-No. 8 and WMO-No. 306** wave-height columns: not opened. Heights in §6 are from W8 and W9.
9. **Fully developed H_s versus Beaufort number** as an official WMO table: not found. Use W4 eq. (1.45) or the operational W8/W9 guide, and do not mix them.
10. **Tessendorf default A**, and any timing other than "nearly interactive" 512² on a 1+ GHz CPU: not found in W6.
11. **Pierson & Moskowitz (1964) Table 1** peak frequencies and the adopted f_max′ = 0.140 were reported by Grok; I did not re-read that table. The 0.877 factor is used from W4 and from (4β/5)^{1/4}.
12. **W4's "420 selected" records** were not counted by me in W1.
13. **Grok errors caught (primary wins):**
    - Grok reported the JONSWAP mean γ, σ_a, σ_b and α = 0.076 x̃^{-0.22} as not found in the 1973 report. They are on pp. 32 and 37, eqs. (2.4.1)–(2.4.5), and I read them on the page images.
    - Grok reported the TMA equation numbers in Bouws et al. 1985 as unverified. I opened the JGR PDF; eqs. (7), (8) and (10) are on p. 979.
    - Grok said the Hughes Φ approximation has error < 1% *for* 0.8 < ω_h < 1.3. The page says a *maximum of 4% near ω_h = 1.0*, and error < 1% *outside* that range (W5 p. 7 [IMG]).
    - Grok mixed equation numbers from the 1998 (2nd) edition of WMO-No. 702 with the 2018 (3rd) edition. All W4 equation numbers here are from the 2018 edition I opened.

## 12. Sources disagree

| Topic | Values | Comment |
|---|---|---|
| PM α | 8.10 × 10⁻³ (W1 eq. 12) vs 0.0081 (W4 eq. 1.43) | Same to three significant figures; W4 rounds |
| Peak-frequency factor | 0.877 from (4β/5)^{1/4} and W4 eq. (1.44) vs 2π × 0.140 ≈ 0.880 if f_max′ = 0.140 is used | A 0.3% difference, if Grok's Table 1 reading holds |
| Fully developed Hs | 0.209 U_{19.5}²/g (my integral of W1; 2√(α/β) = 0.2092) vs H_{m0} = 0.0246 u_{10}² (W4) | Consistent once the 1.075 height factor is applied: 0.2092/g × 1.075² = 0.02465 (g = 9.81), matching W4. The earlier 0.0247 came from rounding the coefficient up to 0.210 |
| JONSWAP α fetch exponent | −0.22 in eq. (2.4.4) vs −0.4 as a better fit to *their* α points (W2 p. 37) | The published law is the scaling line, not the ordinary least-squares slope of their points |
| Deep / shallow cuts | theoretical h ≥ λ/2 (orbital motion < 4%) vs practical W4 classes h > λ/4 and h < λ/25 vs textbook h/L > 0.5 and h/L < 0.05 | W4 is explicit that λ/4 is the practical deep-water edge. λ/25 = 0.04, close to 0.05 |
| Beaufort wind speeds | W4 Table 2.1 "forecasting" equivalents (e.g. force 8 = 17.8 m s⁻¹) vs observer-reported intervals (force 8 = 17.2–20.7 m s⁻¹ / 34–40 kn) vs W8 limits | W4 says observers tend to overestimate high winds and underestimate low winds (p. 26) |
| Beaufort wave heights vs PM | W8 probable 5.5 m at force 8 vs H_{m0} ≈ 8.9 m from W4 eq. (1.45) at 19 m s⁻¹ | Different quantities: a sea-state guide, not a fully developed forecast. W8 forbids reversing the table |
| Whitecap W(U) | W = 2.95 × 10^{-6} U^{3.52} (OLS) vs 3.84 × 10^{-6} U^{3.41} (robust), both in W10 | Same data, two estimators. At U = 10 m s⁻¹: 0.0098 vs 0.0099; at U = 20 m s⁻¹: 0.112 vs 0.105. Later literature (unchecked here) reports a much larger spread |
| Breaker index | W4 h_b = 1.28 H_b (γ_b ≈ 0.78) vs a widely cited McCowan 0.78, itself possibly 0.75 in his experiment | I verified only W4. McCowan was not opened |

## 13. References (all accessed 2026-11-17)

Link check, 2026-11-17, curl -L, unless noted.

- **W1** Pierson W.J. Jr., Moskowitz L. (1964) JGR 69(24):5181–5190, DOI 10.1029/JZ069i024p05181. Copy read: https://data-ww3.ifremer.fr/COURS/OLD_COURSES/LECTURES/Pierson_Moskowitz_JGR1964.pdf
- **W2** Hasselmann K. et al. (1973) Ergänzungsheft zur Deutschen Hydrographischen Zeitschrift, Reihe A(8°), Nr. 12. Record: https://repository.tudelft.nl/record/uuid:f204e188-13b9-49d8-a6dc-4fb7c20562fc. Copy read: ResearchGate scan, https://www.researchgate.net/publication/256197895
- **W3** Bouws E., Günther H., Rosenthal W., Vincent C.L. (1985) JGR 90(C1):975–986, DOI 10.1029/JC090iC01p00975. Copy read: https://pure.mpg.de/rest/items/item_2514801_2/component/file_3514893/content
- **W4** World Meteorological Organization (2018) Guide to Wave Analysis and Forecasting, 3rd ed., WMO-No. 702, ISBN 978-92-63-10702-2, DOI 10.25607/OBP-1523.2. Record: https://library.wmo.int/records/item/31871-guide-to-wave-analysis-and-forecasting. Copy read: https://www.imgw.pl/sites/default/files/2021-06/guidetowave-analysisandforecasting.pdf (same file as the copy already on disk)
- **W5** Hughes S.A. (1984) The TMA Shallow-Water Spectrum: Description and Applications, Technical Report CERC-84-7, USACE. Copy read: https://upload.wikimedia.org/wikipedia/commons/1/12/The_TMA_shallow-water_spectrum_description_and_applications_-_USACE-p266001coll1-7361.pdf (approved for public release)
- **W6** Tessendorf J., Simulating Ocean Water, course notes, copyright 1999–2004. https://jtessen.people.clemson.edu/reports/papers_files/coursenotes2004.pdf (same text lineage as the 2001 SIGGRAPH course)
- **W7** Battjes J.A. (1974) Surf Similarity. Proc. 14th International Conference on Coastal Engineering, 466–480. DOI 10.9753/icce.v14.26. Record: https://icce-ojs-tamu.tdl.org/icce/article/view/2921. Official PDF: https://icce-ojs-tamu.tdl.org/icce/article/download/2921/2586. Licence: CC BY 4.0. Copy also in `/tmp/g1/battjes74.pdf`
- **W8** Japan Meteorological Agency, Beaufort Scale of Wind Force (Determination of wind speed by sea condition), table dated 2006.3. https://marine.kishou.go.jp/brochure/beaufort-en.pdf
- **W9** Met Office, Beaufort wind force scale. https://weather.metoffice.gov.uk/guides/coast-and-sea/beaufort-scale
- **W10** Monahan E.C., O'Muircheartaigh I. (1980) J. Phys. Oceanogr. 10:2094–2099, DOI 10.1175/1520-0485(1980)010<2094:OPLDOO>2.0.CO;2. Abstract opened: https://journals.ametsoc.org/view/journals/phoc/10/12/1520-0485_1980_010_2094_opldoo_2_0_co_2.xml (full PDF 403 to curl)
