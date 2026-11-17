# C2: Gaits of quadruped mammals (footfalls, duty factor, phase, transition speeds, stride scaling, spine)


> **Verify fixes applied 2026-11-16 10:35 AEDT (from C2-mammal-gaits.md.verify.md):** (1) Schilling & Hackert 2006 speeds replaced with per-species means ± SD (spine table, first small-mammal row). (2) Sources-disagree item 2: Alexander 1993 uses leg length L; hip height is the Alexander & Jayes 1983 convention. (3) Trot–gallop Fr noted as printed 2.53 in Alexander 1993. No other rows changed.

Researched 2026-11-16 for the M6 libraries (brief: `LIBRARY_RESEARCH_BRIEF.md`, item C2). All sources accessed 2026-11-16.

## How to read this file

- **Status** (every row):
  - **V**: I read the number in the source itself, in its full text or its official abstract (PubMed or the publisher).
  - **V-sec**: I read the number in a primary paper that quotes another paper (named in the row). I did not open the original.
  - **D**: Derived. I computed it from V numbers (the formula is given), so it carries no spread of its own.
  - **U**: Unverified. It comes from a secondary summary or from memory and was not seen in a primary source. Do not use it until it is checked.
- **Phase**: footfall (touch-down) time as a fraction of the stride, with **left hind (LH) = 0.00**. LF, RH and RF are the other feet.
- **Duty factor (DF)**: stance time ÷ stride time.
- **Fr**: Froude number = v²/(g·h), where h = hip height or leg length (each row says which). û = Fr^0.5 (dimensionless speed).
- **Method note**: the grok CLI passes the brief asked for could not run. The CLI on the box is not signed in (no auth file, no `XAI_API_KEY`). Everything below comes from my own WebSearch/WebFetch/PubMed/OpenAlex reads of the sources. No grok claims are included.
- **Licence**: no figures or wording were copied. The numbers are cited as facts. JEB = *Journal of Experimental Biology*, © The Company of Biologists. Its back issues are free to read but are not open-licensed unless the row says so.
- **Links**: each row gives the DOI or a URL. The full source entries, with link-check results, are in **Sources** at the bottom.

---

## 1. Footfall tables (gait × foot: phase with LH = 0, and duty factor)

### 1a. Dog: Belgian Malinois (Maes et al. 2008, JEB 211:138–149)
Population: 5 Malinois dogs, 486 sequences, overground, 0.4–10.0 m/s. Time lags are % of the f1 cycle, mean ± s.d. (Table 2, p.~143). FL = f1→f2, HL = h1→h2, PL = f1→h1 footfall. f1/h1 are the same side (APS method, Methods p.139).
Conversion to LH = 0 (D, my arithmetic; take h1 = LH): LF = 1 − PL, RH = HL, RF = 1 − PL + FL (mod 1).

| Gait (N sequences) | FL % | HL % | PL % | Status | → LF | → RH | → RF | Status of phases |
|---|---|---|---|---|---|---|---|---|
| Lateral walk (189) | 51±3 | 50±4 | 84±5 | V (Table 2) | 0.16 | 0.50 | 0.67 | D |
| Trot (180) | 51±3 | 49±4 | 50±4 | V | 0.50 | 0.49 | 0.01 (≈0, diagonal with LH) | D |
| Pace (30) | 51±3 | 50±3 | 96±3 | V | 0.04 (≈0, lateral with LH) | 0.50 | 0.55 | D |
| Transverse gallop (29) | 29±4 | 20±4 | 63±9 | V | 0.37 | 0.20 | 0.66 | D |
| Slow rotary gallop (39) | 21±3 | −18±3 | 76±6 | V | 0.24 | 0.82 (lands 0.18 *before* LH) | 0.45 | D |
| Fast rotary gallop (19) | 18±1 | −15±2 | 68±3 | V | 0.32 | 0.85 | 0.50 | D |

Dog duty factor against speed u (m/s). From Appendix 1 (pp.147–148), fit DF = a + b·ln(u), a and b ± s.e.:

| Gait | a | b | r² | Example (D) | Status |
|---|---|---|---|---|---|
| All gaits pooled (486) | 0.652±0.003 | −0.210±0.003 | 0.938 | u=2 → 0.51; u=4 → 0.36; u=8 → 0.22 | V / D |
| Trot (180) | 0.635±0.010 | −0.203±0.010 | 0.707 | u=3 → 0.41 | V / D |
| Pace (30) | 0.711±0.022 | −0.293±0.022 | 0.869 | u=3 → 0.39 | V / D |
| Transverse gallop (29) | 0.668±0.060 | −0.215±0.036 | 0.573 | u=5 → 0.32 | V / D |
| Slow rotary gallop (39) | 0.453±0.054 | −0.102±0.031 | 0.227 | u=6 → 0.27 | V / D |
| Fast rotary gallop (19) | 1.008±0.148 | −0.357±0.068 | 0.621 | u=9 → 0.22 | V / D |

- Lateral walk DF: not given. The walk rows of Appendix 1 came out of the PDF text extraction scrambled (St a=0.656 next to DF a=0.115). **Not used.**
- DF falls below 0.5 above about 2.0 m/s (≈ Fr 0.7). V (Results, p.~141).
- Forelimb DF > hindlimb DF in more than 60% of sequences. At very high speed the hind DF exceeds the fore DF. V (Discussion p.~145).

### 1b. Elephant: Asian and African (Hutchinson et al. 2006, JEB 209:3812–3827)
Population: 14 African and 48 Asian elephants (60 filmed plus 2 from Christian et al. 1999), 116–4632 kg, 621 trials, more than 2400 strides, 0.40–6.8 m/s. Phase is defined with LH = 0 in the paper itself (Methods, p.3814).

| Quantity | African | Asian | Status, location |
|---|---|---|---|
| P_LF observed range | 0.12–0.26 | 0.080–0.31 | V, Table 2 (p.~3816) |
| P_RH observed range | 0.41–0.55 | 0.38–0.56 | V, Table 2 |
| P_RF observed range | 0.60–0.77 | 0.60–0.80 | V, Table 2 |
| P_LF vs û (linear) | 0.16 + 0.043·û (R² 0.13) | 0.16 + 0.041·û (R² 0.34) | V, Table 3 (see note) |
| P_RH vs û | 0.49 + 0.0088·û (R² 0.0098) | 0.49 + 0.010·û (R² 0.044) | V, Table 3 |
| P_RF vs û | 0.65 + 0.042·û (R² 0.10) | 0.65 + 0.047·û (R² 0.31) | V, Table 3 |
| Mean DF (all limbs) vs û | 0.53 − 0.32·log10(û) (R² 0.89) | 0.51 − 0.32·log10(û) (R² 0.84) | V, Table 3 |
| DF observed range | 0.44–0.85 | 0.37–0.78 | V, Table 2 |
| DF fore − hind (β_diff) | −0.010 to 0.070 | −0.050 to 0.10 | V, Table 2 |
| Modal pattern | LF 20–25% of stride after LH; RH 25–30% after LF; RF 20–25% after RH; spread ±0.1 around the modes | same | V, Results text (p.~3816) |

- Note on Table 3: its footnote prints "linear: y = ax + b", but the values only match the observed ranges if read as y = a + b·x. The log fit only matches if log = log10. That reading is mine (D).
- Example (D, African): at û = 1.0 the phases are LF 0.20, RH 0.50, RF 0.69 and DF = 0.53.
- Elephants keep a lateral-sequence footfall at every speed. They never trot, pace or gallop. V (Abstract).

### 1c. Horse: Icelandic, all gaits (Robilliard, Pfau & Wilson 2007, JEB 210:187–197)
Population: 8 Icelandic horses, ridden overground, hoof accelerometers, 9687 LF strides. Speed ranges (V, Results p.189): walk 0.6–3.2 m/s (N=1755), tölt 1.2–7.6 (2878), trot 1.8–6.7 (2147), pace 2.8–10.6 (662), left canter 4.0–8.2 (267), left gallop 4.9–8.2 (112), right canter 4.8–8.9 (871), right gallop 4.7–8.8 (178). Canters and gallops were all transverse (Methods).

Duty factor per limb = mean stance ÷ mean stride time. This is **D**, computed from the **V** means in Table 2 (p.190) and Table 3 (p.~192). The paper gives s.d. for stance and stride time but no s.d. for these ratios.

| Gait @ speed (m/s) | Stride time ms (mean±s.d.) [V] | DF LF | DF RF | DF LH | DF RH | Stride freq Hz (D) |
|---|---|---|---|---|---|---|
| Walk @1.5 (N≈1021) | 904±79 | 0.60 | 0.60 | 0.62 | 0.61 | 1.11 |
| Walk @2.0 (N≈623) | 887±77 | 0.59 | 0.60 | 0.60 | 0.61 | 1.13 |
| Tölt @3.0 (N≈188) | 571±48 | 0.44 | 0.44 | 0.50 | 0.50 | 1.75 |
| Tölt @4.0 (N≈824) | 524±28 | 0.40 | 0.40 | 0.44 | 0.44 | 1.91 |
| Tölt @5.0 (N≈434) | 486±21 | 0.38 | 0.37 | 0.40 | 0.40 | 2.06 |
| Trot @3.5 (N≈445) | 600±39 | 0.43 | 0.43 | 0.43 | 0.42 | 1.67 |
| Trot @4.0 (N≈618) | 569±36 | 0.39 | 0.40 | 0.38 | 0.40 | 1.76 |
| Trot @5.0 (N≈277) | 511±36 | 0.39 | 0.36 | 0.37 | 0.37 | 1.96 |
| Pace @6.0 (N≈67) | 471±27 | 0.33 | 0.33 | 0.35 | 0.35 | 2.12 |
| Pace @8.0 (N≈73) | 426±25 | 0.29 | 0.30 | 0.32 | 0.32 | 2.35 |
| Left canter @6.0 (N=55) | 463±27 | 0.35 | 0.34 | 0.35 | 0.35 | 2.16 |
| Left gallop @7.0 (N=39) | 415±18 | 0.34 | 0.33 | 0.35 | 0.36 | 2.41 |
| Right canter @6.0 (N≈232) | 472±29 | 0.34 | 0.35 | 0.35 | 0.35 | 2.12 |
| Right gallop @7.0 (N≈14) | 434±27 | 0.32 | 0.33 | 0.34 | 0.34 | 2.30 |

Left–right timing. Ratio = (RF_on − LF_on)/stride or (RH_on − LH_on)/stride, mean ± s.d. (N). **V**, Table 4 (p.~193):

| Gait | Forelimb ratio | Hindlimb ratio |
|---|---|---|
| Walk | 0.50±0.01 (1714) | 0.50±0.02 (1481) |
| Tölt | 0.50±0.03 (2658) | 0.50±0.03 (2506) |
| Trot | 0.50±0.03 (1973) | 0.49±0.03 (1782) |
| Pace | 0.51±0.04 (638) | 0.49±0.04 (566) |
| Left canter | 0.73±0.04 (267) | 0.32±0.04 (262) |
| Left gallop | 0.78±0.03 (112) | 0.29±0.03 (106) |
| Right canter | 0.27±0.03 (830) | 0.67±0.05 (628) |
| Right gallop | 0.28±0.06 (178) | 0.41±0.17 (158) |

- Footfall order (V, Table 1): walk and tölt LH, LF, RH, RF. Trot (LH+RF), (RH+LF). Pace (LH+LF), (RH+RF). Left canter RH, then RF and LH together or close, then LF. Left gallop RH, LH, RF, LF.
- **LH-referenced horse phases (LF relative to LH) are not in this paper**, because it uses LF as the reference. See "Not found".
- Hind stance longer than fore stance by gait (V, Results p.189): walk +2.7%, tölt +9.2%, trot −1.6%, pace +5.1%, left canter −0.9%, right canter +3.2%, left gallop +3.0%, right gallop +4.1%.

### 1d. Transverse vs rotary gallop, many species (Biancardi & Minetti 2012, JEB 215:4144–4156)
Population: 152 strides (31 transverse, 121 rotary) of 15 species (the Fig. 5 caption says 12 species), from the authors' own and commercial slow-motion film. Values are mean ± s.d., Table 7 (p.~4152). Forefeet lag = time between the two fore footfalls. Hindfeet lag is the same for the hind pair, negative = opposite lead. Pair lag = first forefoot → first hindfoot. All are % of stride (Glossary). Phase conversion as in 1a (D; h1 = LH).

| Quantity | Transverse | Rotary | Status |
|---|---|---|---|
| Duty factor, all feet % | 29.1±5.5 | 23.3±5.2 | V |
| Forelimb DF % | 29.6±6.1 | 22.9±6.3 | V |
| Hindlimb DF % | 28.6±5.5 | 23.6±4.8 | V |
| Fore/hind DF ratio | 1.04±0.14 | 0.96±0.17 | V |
| Forefeet lag % | 22.2±6.6 | 17.6±5.2 | V |
| Hindfeet lag % | 16.4±3.7 | −12.9±4.2 | V |
| Pair lag % | 73.9±6.8 | 70.3±8.3 | V |
| Hindfeet ratio % (stride supported by ≥1 hind foot) | 45.0±7.6 | 36.7±7.1 | V |
| Midtime lag % | 29.6±7.6 | 44.6±6.4 | V |
| → Phases LH / RH / LF / RF | 0 / 0.16 / 0.26 / 0.48 | 0 / 0.87 (−0.13) / 0.30 / 0.47 | D |

- Species classified by gallop type (V, Table 1): 37 species gallop transverse only, 35 rotary only, and 17 switch with speed (rotary at high speed).
- Examples: horse and camel transverse; cheetah rotary; some dogs switch with speed.

### 1e. Cheetah and racing greyhound, rotary gallop (Hudson, Corr & Wilson 2012, JEB 215:2425–2434)
Population: 9 cheetahs (53 strides, 8.7–17.8 m/s) and 6 retired racing greyhounds (47 strides, 9.9–19.0 m/s), all adult. Body mass 30–45 kg (cheetahs) and 28–35 kg (greyhounds). V (Table 1 p.2427; Results p.2428).

- Both species used a rotary gallop at these speeds (V).
- Cheetah duty factors were significantly lower than greyhound ones at most speeds, except the lead hind limb (V, p.2429).
- Duty factor appears only as LMM coefficients (Table 2). There is no table of mean DF by speed, so **no DF numbers are reported here**. Values are readable only from Fig. 4, which I did not digitise.
- Minimum swing time: 0.20 s (cheetah) and 0.19 s (greyhound). V (Discussion p.~2431).

### 1f. Gaits defined only qualitatively (half-bound, bound, amble/rack/running walk)

| Gait | What is defined | Source | Status |
|---|---|---|---|
| Half-bound | Forefeet with a lead (FL < 50%). Hind lag = 0 (hind feet together). | Maes et al. 2008 Table 1 (after Abourachid 2003); Renous, Herbin & Gasc 2004 | V (definition only) |
| Bound | FL = 0 and HL = 0 (both pairs together). PL depends on speed. | same | V (definition only) |
| Symmetrical gaits | Within-pair lags of 50% ± 5% | Renous et al. 2004 p.~100; Maes 2008 Results | V |
| Tölt (Icelandic amble) | Symmetrical 4-beat gait. Single and double support alternate. True tölt pattern only over a narrow speed range. At 4.7±0.53 m/s, 60% of cycles were 4-beat pace. 23 horses, 10 strides each, at 2.9±0.28, 3.7±0.29 and 4.7±0.53 m/s. | Zips et al. 2001, EVJ Suppl 33:109–111 (abstract) | V |
| Tölt mechanics | 10 Icelandic horses, 27 trials, 0.89–5.98 m/s. Lateral-sequence single-foot. Bouncing (running) mechanics. Fr above the walk–run value. | Biknevicius et al. 2006, AJVR 67:1505 (abstract) | V |

---

## 2. Gait-transition speeds

| Transition | Species / population | Speed (m/s) | Froude number | Spread | Source, location | Status |
|---|---|---|---|---|---|---|
| Walk → trot | Horses, 9 adults, 90–720 kg, leg length L 0.7–1.4 m, treadmill | 1.6 (smallest) to 2.3 (largest) | **≈0.35** (v²/gL, L = leg length) | Abstract gives no spread. It says horses switch "in a narrow range of speeds". | Griffin, Kram, Wickler & Hoyt 2004, JEB 207:4215, Summary | V |
| Walk → trot (general) | Quadrupedal mammals | n/a | **≈0.5** | none given | Alexander 1993, J Robot Soc Japan 11:314, p.~316, citing Alexander & Jayes 1983 | V-sec |
| Trot → gallop (general) | Quadrupedal mammals | n/a | **≈2.5** | none given | Alexander 1993 (as above), citing A&J 1983 | V-sec |
| Trot → gallop (general) | Mice to rhinoceros | n/a | **2 to 3** | range | Iriarte-Díaz, Bozinovic & Vásquez 2006, JEB 209:4061, Discussion, citing A&J 1983 | V-sec |
| Trot → gallop | Degu (*Octodon degus*), treadmill: control, loaded, overfed | n/a | 2.8±0.1, 2.7±0.1, 2.8±0.1 | ± as given (hip height from 2 individuals) | Iriarte-Díaz et al. 2006, Discussion | V |
| Trot → gallop (scaling) | 16 species, 30 g mouse to horse | v_tg = **1.53·M^0.24** (M in kg) | n/a | no CI in the secondary source | Heglund, Taylor & McMahon 1974, Science 186:1112, as quoted in Hutchinson et al. 2006 Table 6 (p.~3824) | V-sec |
| Stride frequency at trot → gallop | same | f = **4.48·M^−0.14** Hz | n/a | n/a | Heglund et al. 1974 via Hutchinson 2006 Table 6 | V-sec |
| Stride length at trot → gallop | same | L = **0.35·M^0.38** m | n/a | n/a | Heglund et al. 1974 via Hutchinson 2006 Table 6 | V-sec |
| Minimum trotting speed | 16 species | v = **0.593·M^0.249** | n/a | n/a | Heglund & Taylor 1988, JEB 138:301, via Hutchinson 2006 Table 6 | V-sec |
| Equivalent speeds (min, preferred, max) in trot and gallop | 16 species, treadmill | all scale ≈ **M^0.2**. Max sustained = 2.6× min in trot and 2.1× min in gallop. Stride frequency at equivalent speeds ≈ **M^−0.15**. | n/a | n/a | Heglund & Taylor 1988, Abstract | V |
| Trot → gallop speed exponent | same | v_tg ∝ M^0.216; f ∝ M^−0.150. Mouse ≈0.8 m/s, 170 kg pony ≈5 m/s. Preferred trot 1.09·M^0.222 with f 3.35·M^−0.130. | n/a | n/a | Heglund & Taylor 1988, read only in a secondary summary (Original Wisdom web page) | **U** |
| Trot → gallop trigger | Horses (n not in abstract) | Trot → gallop happens where galloping costs *more* energy. With added load it happens at a lower speed but the same peak force. | n/a | n/a | Farley & Taylor 1991, Science 253:306, Abstract | V (qualitative) |
| Aerial phase appears (hindquarters first) | Elephants | n/a | **Fr ≈ 1** (h = hip height) | n/a | Hutchinson et al. 2006, Abstract | V |
| Fastest elephant | Asian, Thailand group | 6.8 m/s (bull ≈2790 kg) | 2.8 (bull); max for the group **3.4** (û 1.8) | single maxima | Hutchinson et al. 2006, Results p.~3817 | V |
| DF < 0.5 (start of running) | Malinois dogs | > 2.0 | ≈0.7 (h = withers height) | n/a | Maes et al. 2008, Results | V |
| Cycle duration stops falling; stride length drives speed after this | Malinois dogs | ≈4.0 | ≈2 | n/a | Maes et al. 2008, Discussion p.145 | V |
| Speed ranges used per gait | Icelandic horses, 8 | see 1c | n/a | min–max | Robilliard et al. 2007, Results p.189 | V |
| Preferred walk, trot and gallop speeds (Hoyt & Taylor) | 3 small horses, 110–170 kg, treadmill | not in the abstract | n/a | n/a | Hoyt & Taylor 1981, Nature 292:239 (paywalled) | **Not verified, so no number given** |

---

## 3. Stride length and stride frequency against speed and size (equations)

λ = stride length, h = hip height, f = stride frequency, M = body mass (kg), u = speed (m/s), Fr = u²/(g·h), û = Fr^0.5.

| Equation | Applies to | Coefficient spread | Source, location | Status |
|---|---|---|---|---|
| λ/h = **2.4·Fr^0.34** | Walking quadrupeds | not given in the secondary source | Alexander & Jayes 1983, J Zool 201:135, via Hutchinson 2006 Table 6 | V-sec |
| λ/h = **1.9·Fr^0.40** | Running, cursorial mammals | n/a | A&J 1983 via Hutchinson 2006 Table 6 | V-sec |
| λ/h = **2.7·Fr^0.28** | Running, non-cursorial mammals | n/a | A&J 1983 via Hutchinson 2006 Table 6 | V-sec |
| DF_fore = **0.52·Fr^−0.14** (walk); **0.52·Fr^−0.28** (run) | Quadrupeds | n/a | A&J 1983 via Hutchinson 2006 Table 6 | V-sec |
| DF_hind = **0.51·Fr^−0.18** (walk); **0.53·Fr^−0.28** (run) | Quadrupeds | n/a | A&J 1983 via Hutchinson 2006 Table 6 | V-sec |
| λ/h = **2.3·û^0.60** (= 2.3·Fr^0.30) | Mammals | n/a | Alexander 1977, as cited in Hutchinson 2006 Table 6 (Hutchinson's reference list gives "Allometry of the limbs of antelopes", J Zool 183:125) | V-sec |
| S/L = **2.5·(v²/gL)^0.3** | Animals, legs of length L | n/a | Alexander 1993, J Robot Soc Japan 11:314, Eq. 2 (p.~316) | V |
| f at max sustained gallop = **4.7·M^−0.162** Hz | 16 quadruped species | n/a | Heglund & Taylor 1988 via Hutchinson 2006 Table 6 (printed there as "0.162"; the negative sign is implied by its predicted values) | V-sec |
| Elephant λ/h = **2.0·û^0.35** (African, R² 0.84); **2.1·û^0.33** (Asian, R² 0.86) | 62 elephants | R² only | Hutchinson 2006 Table 3 | V |
| Elephant relative f = f·(h/g)^0.5 = **0.49·û^0.65** (Af, R² 0.95); **0.48·û^0.67** (As, R² 0.95) | same | R² only | Hutchinson 2006 Table 3 | V |
| Elephant λ (m) = **1.8·u^0.32** (Af, R² 0.44); **2.0·u^0.33** (As, R² 0.61) | same | R² only | Hutchinson 2006 Table 3 | V |
| Elephant f (Hz) = **0.56·u^0.68** (Af, R² 0.78); **0.50·u^0.67** (As, R² 0.81) | same | R² only | Hutchinson 2006 Table 3 | V |
| Dog λ (m) = **0.489 + 0.297·u** (±0.009, ±0.003; r² 0.964) | Malinois, all gaits, 0.4–10 m/s | ± s.e. | Maes 2008 Appendix 1 | V |
| Dog f (Hz) = **1.314 + 0.762·ln u** (±0.012, ±0.011; r² 0.910) | same | ± s.e. | Maes 2008 Appendix 1 | V |
| Dog λ by gait: trot 0.498+0.286u; pace 0.252+0.365u; transverse gallop 0.695+0.256u; slow rotary 0.380+0.346u; fast rotary −2.363+0.599u | Malinois | ± s.e. in source | Maes 2008 Appendix 1 | V |
| Cheetah mean f rises **2.4 → 3.2 Hz** over 8.7–17.8 m/s; greyhound f shows no consistent trend; maximum f **3.9 Hz** in both | 9 cheetahs, 6 greyhounds | range | Hudson et al. 2012, Discussion p.~2430 | V |
| Cheetah longer strides than greyhound at any speed; λ rises linearly with u in both | same | LMM (Table 2) | Hudson 2012 Results | V (qualitative) |
| Horse stride times by gait and speed | Icelandic | mean ± s.d. | Robilliard 2007 Tables 2–3 (see 1c) | V |
| Horse ≈2¼ strides/s at ≈35 mph; cheetah ≈2½ strides/s at ≈45 mph; cheetah stride 21–26 ft | 1 cheetah (film), racehorse Citation (film) | n/a | Hildebrand 1959, read only in the Original Wisdom summary | **U** |

---

## 4. Spine flexion and extension

| Species, gait, speed | Measure | Value | Spread | Population, method | Source, location | Status |
|---|---|---|---|---|---|---|
| Horse, walk, 1.6 m/s, treadmill | Flexion–extension ROM per vertebra caudal to T10 | ≈7° | "approximately", no s.d. in abstract | 5 Dutch Warmblood, bone-fixated markers T6–S3 | Faber et al. 2000, AJVR 61:399, Abstract | V |
| Horse, walk | Lateral bending; axial rotation | up to 5.6° (cranial thoracic and pelvis); AR 4° (T6) rising to 13° (tuber coxae) | none | same | same | V |
| Horse, trot, 4.0 m/s, treadmill | FE ROM, all vertebrae | 2.8–4.9° | range | 5 young Dutch Warmblood, bone-fixated | Faber et al. 2001, AJVR 62:757, Abstract | V |
| Horse, trot | Lateral bending; axial rotation | LB 1.9–3.6°; AR 4.6–5.8° (T10 3.1°, T13 3.3°) | range | same | same | V |
| Horse, canter, 7.3 m/s, treadmill | Maximal ROM: FE; LB; AR | **15.8±1.3°**; 5.2±0.7°; 7.8±1.2° | ± s.d. | 5 Dutch Warmblood, Steinmann pins in 8 vertebrae and tubera coxae | Faber et al. 2001, EVJ Suppl 33:145, Abstract | V |
| Horse, canter | Largest segmental FE | L5–S3, up to 8.6° | max | same | same | V |
| Cheetah, gallop, 15–18 m/s | Change in the angle between the neck-root–T12 line and the T12–tail-root line | **0.47±0.029 rad = 26.9±1.7°** (D conversion) | ± s.e., 8 strides | 4 adult male cheetahs, 40–50 kg (data of Kamimura et al. 2021) | Kamimura et al. 2022, Front Bioeng Biotechnol 10:825638, Section 3 | V |
| Greyhound, fast straight gallop | Change in lumbosacral sagittal curvature (max − min) | 97.5° | single sequence, no spread | 1 YouTube video, traced | Belyaev et al. 2024, J Anat 244:205, Fig. 10 caption | V (as reported; weak method) |
| Wolf chasing hare | same | 56° and 89° | single sequences | BBC footage | Belyaev et al. 2024, Fig. 10 | V (weak method) |
| Dog, walk and trot | Lumbosacral sagittal motion used vs available (passive) | 3–4° vs 28–37° | range | literature (Benninger 2004; Pylypchuk 1975; Wachs 2016) | Belyaev 2024, citing those | V-sec |
| Small mammals (tree shrew, kowari, grey short-tailed opossum, cui, pika), gallop and half-bound; species mean speeds kowari 0.92±0.06, opossum 1.15±0.19, tree shrew 1.48±0.15, pika 1.18±0.05, cui 1.14±0.12 m/s (Schilling & Hackert 2006, https://hal.science/hal-00365971/file/3925.pdf) | Number of presacral joints that bend | 7±1 | ± | 1 individual per species, high-speed cineradiography | Schilling & Hackert 2006, JEB 209:3925, Abstract | V |
| same | Effective angular movement, last presacral joint (PS1) | 7–22° (pika 7–8°; tree shrew and kowari 17–22°) | range across species | same | Schilling & Hackert 2006, Results p.~3932 | V |
| same | Joints cranial to PS3 | 1–9° each | range | same | same | V |
| same | "Pelvic movement" amplitude: stance; swing | 29–44°; 40–51° | range | same | Schilling & Hackert 2006, Results and Discussion | V |
| Dog; cheetah; horse | "Pelvic movement" amplitude | dog 50°; cheetah 65°; horse 20° | none | as cited | Schilling & Hackert 2006 Discussion, citing Alexander et al. 1980 (dog) and Hildebrand 1959 (cheetah, horse) | V-sec |
| Cheetah; horse | Change in the pelvis-to-scapula angle in the gallop | ≈130°; ≈60° | none | film, 1 cheetah, racehorse Citation | Hildebrand 1959, J Mammal 40:481, read only in the Original Wisdom summary | **U** |
| Malinois dog | Lumbar sagittal flexion becomes visible | above 8.0 m/s | n/a | 5 dogs | Maes 2008 Discussion | V (qualitative) |
| Small mammals | Timing | Maximum spinal flexion between the last third of swing and touch-down; maximum extension in the first third of swing | n/a | as above | Schilling & Hackert 2006, Abstract | V |

---

## Not found

- **Horse footfall phases with LH = 0 and per-gait spread** (walk, trot, canter, gallop). Hildebrand 1965 (Science 150:701) and Hildebrand 1977 (J Mammal 58:131) are paywalled, and I could not open them. Robilliard 2007 uses LF as its reference and does not give the LH→LF lag.
- **Hildebrand's numeric gait formula ranges** (lateral-sequence walk LH→LF lag of 0–50% against hind duty factor, and his asymmetrical-gait lead parameters). Only the definitions were verified, through Robilliard 2007 (Introduction) and Cartmill et al. 2002. No ranges came from the original papers.
- **Domestic cat**: footfall phases, duty factors and transition speeds. Searched, not found in an accessible primary source.
- **Cheetah per-foot duty factors and phases as numbers**: Hudson 2012 shows them only in figures and LMM coefficients. Hildebrand 1959 and 1961 are paywalled.
- **Half-bound and bound**: no measured phases or DFs, only the definitions. Schilling & Hackert 2006 filmed half-bounds but do not tabulate lags.
- **Rack, running walk, paso and other American gaited-horse ambles**: no numbers found. Nicodemus & Clayton 2003 (Appl Anim Behav Sci 80:133) was not reachable.
- **Hoyt & Taylor 1981 preferred speeds** (horse walk, trot, gallop): paywalled, and the abstract has none.
- **Heglund & Taylor 1988 coefficient table (Table 5)**: the JEB PDF is blocked to my tools. Only the abstract and two equations quoted by Hutchinson 2006 were confirmed.
- **Alexander & Jayes 1983, full text**: paywalled (Wiley). Its coefficients were confirmed only as quoted in Hutchinson 2006 Table 6, and its transition Froude numbers only as quoted by Alexander 1993 and Iriarte-Díaz 2006.
- **Hutchinson et al. 2003 (Nature 422:493)**: elephant Fr maximum. Not opened. Hutchinson 2006 covers that ground.
- **Greyhound or dog thoracolumbar ROM in degrees from a proper kinematic study** (bone markers or X-ray). The only number found is Belyaev 2024's estimate from a video.
- **Muybridge plates as numbers**: they are public domain, but no footfall timing was extracted from them. That would need someone to measure the plates frame by frame.

## Sources disagree (and data problems)

1. **Stride-length rule coefficient.** For λ/h = k·Fr^0.3, Alexander 1977 (as quoted by Hutchinson 2006 Table 6) gives k = **2.3**. Alexander 1993, Eq. 2 gives k = **2.5**. Alexander & Jayes 1983 (via Hutchinson) splits it into walk 2.4·Fr^0.34 and run 1.9·Fr^0.40 (cursorial) or 2.7·Fr^0.28 (non-cursorial). At Fr = 1 these give λ/h of 2.3, 2.5, 2.4, 1.9 and 2.7.
2. **Walk–trot Froude number.** Griffin 2004 found ≈**0.35** in horses, using leg length. Alexander 1993's general figure is ≈**0.5**, using leg length L in Fr = v²/gL (hip height is the Alexander & Jayes 1983 convention; https://www.jstage.jst.go.jp/article/jrsj1983/11/3/11_3_314/_pdf). Malinois dogs drop below DF 0.5 at Fr ≈ 0.7 (withers height). Part of the gap is the length used (hip height, leg length or withers height).
3. **Trot–gallop Froude number.** Alexander 1993 says ≈2.5 (printed as 2.53). Iriarte-Díaz 2006 says 2–3 (degus 2.7–2.8). The Malinois cycle-duration plateau is at ≈Fr 2.
4. **Horse and cheetah spine in Hildebrand 1959.** Schilling & Hackert 2006 cite it as **"pelvic movement" 65° (cheetah) and 20° (horse)**. The Original Wisdom summary says the **pelvis–scapula angle changes ≈130° and ≈60°**. These may be two different measures (pelvic rotation vs the angle between pelvis and scapula). Not resolved, because the primary is paywalled.
5. **Cheetah spine size.** Kamimura 2022 measured a neck–T12–tail-root bend of **27°** (±1.7° s.e.), while the Hildebrand-derived figures above are 65° to 130°. The methods differ: line-angle markers on modern video against traced 1950s film.
6. **Horse spine.** Faber 2001 canter FE 15.8° (bone pins, treadmill) against "pelvic movement" 20° (Hildebrand 1959, film, racing gallop): different gait, method and measure.
7. **Elephant duty factors, fore vs hind.** Hutchinson 2006 found forelimb DF > hindlimb DF in 93% (African) and 91% (Asian) of trials. They note this contradicts Hildebrand 1976, who reported roughly equal fore and hind DF.
8. **Elephant "trot".** Hutchinson 2006 report that Hildebrand (1965, 1966, 1976) said elephants use a "slow trot". Hutchinson saw only lateral-sequence patterns.
9. **Heglund & Taylor 1988 scope.** The abstract says the horses were "200 kg". The secondary summary reports a 680 kg horse in the tables and masses of 170, 270 and 300 kg in the text. Not checked against the primary.
10. **Biancardi 2012 species count.** The text says 15 species. The Fig. 5 caption says 12.
11. **Robilliard 2007 Table 2 typos** as printed or extracted: tölt 2.0 m/s RH stance "30±67" (probably 300) and tölt 3.5 m/s LH swing "28±27" (probably 28x). Those rows are not used above.
12. **Hutchinson 2006 Table 3 footnote** says linear y = ax + b, but only y = a + bx fits the observed ranges (see 1b).

---

## Sources (full entries, licence, link check; all accessed 2026-11-16)

"Link check" records what I did on 2026-11-16:
- **WebFetch OK**: the page opened through the fetch tool and showed the cited content.
- **curl 200 or 203**: the link answered from the box.
- **curl 403**: publisher bot-block (Cloudflare). The DOI resolved (302 seen) and the content was confirmed another way, as noted.

| # | Full reference | URLs | Licence (plain) | Link check | Supports |
|---|---|---|---|---|---|
| S1 | Robilliard JJ, Pfau T, Wilson AM (2007). Gait characterisation and classification in horses. *J Exp Biol* 210(2):187–197. doi:10.1242/jeb.02611. PMID 17210956. | https://doi.org/10.1242/jeb.02611 · PDF https://journals.biologists.com/jeb/article-pdf/210/2/187/1260108/187.pdf · https://pubmed.ncbi.nlm.nih.gov/17210956/ | Published paper, © Company of Biologists, free to read. Numbers cited as facts. | PDF: WebFetch OK (full text, Tables 1–4). DOI: curl 403. PubMed: curl 203. | 1c, 2 |
| S2 | Hutchinson JR, Schwerda D, Famini DJ, Dale RHI, Fischer MS, Kram R (2006). The locomotor kinematics of Asian and African elephants: changes with speed and size. *J Exp Biol* 209(19):3812–3827. doi:10.1242/jeb.02443. PMID 16985198. | https://doi.org/10.1242/jeb.02443 · PDF https://journals.biologists.com/jeb/article-pdf/209/19/3812/1541905/3812.pdf · repository copy https://researchonline.rvc.ac.uk/id/eprint/1143/1/1143.pdf | Published paper, © CoB. OpenAlex tags the RVC repository copy CC BY (not checked on the copy). Numbers cited as facts. | PDF: WebFetch OK (Tables 1–6). DOI and RVC: curl 403. PubMed: 203. | 1b, 2, 3 (incl. quoted A&J 1983, Heglund 1974, H&T 1988, Alexander 1977) |
| S3 | Biancardi CM, Minetti AE (2012). Biomechanical determinants of transverse and rotary gallop in cursorial mammals. *J Exp Biol* 215(23):4144–4156. doi:10.1242/jeb.073031. PMID 22933611. | https://doi.org/10.1242/jeb.073031 · https://pubmed.ncbi.nlm.nih.gov/22933611/ · archived PDF http://web.archive.org/web/2017id_/http://jeb.biologists.org/content/jexbio/215/23/4144.full.pdf | Published paper. OpenAlex tags CC BY, but no licence line in the PDF, so treat as cited facts. | Archived PDF: curl 200 at 09:50, read in full (Tables 1, 2, 7, Glossary); later retry 500 (Wayback flaky). DOI: 403. PubMed: 203. | 1d |
| S4 | Hudson PE, Corr SA, Wilson AM (2012). High speed galloping in the cheetah (*Acinonyx jubatus*) and the racing greyhound (*Canis familiaris*): spatio-temporal and kinetic characteristics. *J Exp Biol* 215(14):2425–2434. doi:10.1242/jeb.066720. PMID 22723482. | https://doi.org/10.1242/jeb.066720 · PDF https://journals.biologists.com/jeb/article-pdf/215/14/2425/1280636/2425.pdf | Published paper, © CoB. Cited facts. | PDF: WebFetch OK (Tables 1–2, text). DOI: 403. PubMed: 203. | 1e, 3 |
| S5 | Maes LD, Herbin M, Hackert R, Bels VL, Abourachid A (2008). Steady locomotion in dogs: temporal and associated spatial coordination patterns and the effect of speed. *J Exp Biol* 211(1):138–149. doi:10.1242/jeb.008243. PMID 18083742. | https://doi.org/10.1242/jeb.008243 · PDF https://journals.biologists.com/jeb/article-pdf/211/1/138/1262882/138.pdf | Published paper, © CoB. Cited facts. | PDF: WebFetch OK (Tables 1–3, Appendix 1). DOI: 403. PubMed: 203. | 1a, 1f, 2, 3, 4 |
| S6 | Griffin TM, Kram R, Wickler SJ, Hoyt DF (2004). Biomechanical and energetic determinants of the walk–trot transition in horses. *J Exp Biol* 207(24):4215–4223. doi:10.1242/jeb.01277. PMID 15531642. | https://doi.org/10.1242/jeb.01277 · https://pubmed.ncbi.nlm.nih.gov/15531642/ · https://journals.biologists.com/jeb/article-pdf/207/24/4215/1547034/4215.pdf | Published paper, © CoB. Cited facts. | Publisher page: WebFetch OK (Summary). PubMed abstract via NCBI E-utilities OK. DOI: 403. | 2 |
| S7 | Heglund NC, Taylor CR (1988). Speed, stride frequency and energy cost per stride: how do they change with body size and gait? *J Exp Biol* 138:301–318. doi:10.1242/jeb.138.1.301. PMID 3193059. | https://journals.biologists.com/jeb/article/138/1/301/5532/Speed-Stride-Frequency-and-Energy-Cost-Per-Stride · https://pubmed.ncbi.nlm.nih.gov/3193059/ | Published paper, © CoB. Cited facts. | Abstract page: WebFetch OK. Full PDF not reachable (Wayback 500, publisher 403). | 2, 3 |
| S8 | Heglund NC, Taylor CR, McMahon TA (1974). Scaling stride frequency and gait to animal size: mice to horses. *Science* 186(4169):1112–1113. doi:10.1126/science.186.4169.1112. PMID 4469699. | https://doi.org/10.1126/science.186.4169.1112 · https://pubmed.ncbi.nlm.nih.gov/4469699/ | Paywalled (AAAS). Cited facts. | PubMed abstract OK (qualitative only). Equations only via S2 Table 6. DOI: 403. | 2 |
| S9 | Alexander RMcN, Jayes AS (1983). A dynamic similarity hypothesis for the gaits of quadrupedal mammals. *J Zool* 201(1):135–152. doi:10.1111/j.1469-7998.1983.tb04266.x. | https://doi.org/10.1111/j.1469-7998.1983.tb04266.x | Paywalled (Wiley/ZSL). Cited facts. | Abstract seen on the Wiley page through web search. DOI: curl 403. Coefficients via S2, transition Fr via S10 and S11. | 2, 3 |
| S10 | Alexander RMcN (1993). Gaits of mammals and turtles. *J Robotics Society of Japan* 11(3):314–319. doi:10.7210/jrsj.11.314. | https://www.jstage.jst.go.jp/article/jrsj1983/11/3/11_3_314/_pdf | Published review, © Robotics Society of Japan, free on J-STAGE. Cited facts. | curl 200, PDF read (p.~316, Eq. 2). | 2, 3 |
| S11 | Iriarte-Díaz J, Bozinovic F, Vásquez RA (2006). What explains the trot–gallop transition in small mammals? *J Exp Biol* 209(20):4061–4066. doi:10.1242/jeb.02473. | https://doi.org/10.1242/jeb.02473 · https://journals.biologists.com/jeb/article/209/20/4061/16386/What-explains-the-trot-gallop-transition-in-small | Published paper, © CoB. Cited facts. | Discussion text seen on the publisher page through web search. DOI: 403. | 2 |
| S12 | Faber M, Schamhardt H, van Weeren R, Johnston C, Roepstorff L, Barneveld A (2000). Basic three-dimensional kinematics of the vertebral column of horses walking on a treadmill. *Am J Vet Res* 61(4):399–406. doi:10.2460/ajvr.2000.61.399. PMID 10772104. | https://doi.org/10.2460/ajvr.2000.61.399 · https://pubmed.ncbi.nlm.nih.gov/10772104/ | Published paper (AVMA). Cited facts. | DOI: curl 200. PubMed abstract OK. | 4 |
| S13 | Faber M, Johnston C, Schamhardt H, van Weeren R, Roepstorff L, Barneveld A (2001). Basic three-dimensional kinematics of the vertebral column of horses trotting on a treadmill. *Am J Vet Res* 62(5):757–764. doi:10.2460/ajvr.2001.62.757. PMID 11341399. | https://doi.org/10.2460/ajvr.2001.62.757 · https://pubmed.ncbi.nlm.nih.gov/11341399/ | Published paper. Cited facts. | DOI: curl 200. PubMed abstract OK. | 4 |
| S14 | Faber M, Johnston C, Schamhardt HC, van Weeren PR, Roepstorff L, Barneveld A (2001). Three-dimensional kinematics of the equine spine during canter. *Equine Vet J Suppl* (33):145–149. doi:10.1111/j.2042-3306.2001.tb05378.x. PMID 11721556. | https://doi.org/10.1111/j.2042-3306.2001.tb05378.x · https://pubmed.ncbi.nlm.nih.gov/11721556/ | Published paper (Wiley). Cited facts. | PubMed abstract OK (203). DOI: 403. | 4 |
| S15 | Kamimura T, Sato K, Aoi S, Higurashi Y, Wada N, Tsuchiya K, Sano A, Matsuno F (2022). Three characteristics of cheetah galloping improve running performance through spinal movement: a modeling study. *Front Bioeng Biotechnol* 10:825638. doi:10.3389/fbioe.2022.825638. | https://doi.org/10.3389/fbioe.2022.825638 · https://pmc.ncbi.nlm.nih.gov/articles/PMC9049215/ | **Open: CC BY 4.0** (OpenAlex and PMC licence tag). | DOI and PMC: curl 200. Full text WebFetch OK (Section 3, measured data). | 4 |
| S16 | Belyaev RI, Nikolskaia AV, Bushuev AV, Panyutina AA, Kozhanova NN, et al. (2024). Running, jumping, hunting, and scavenging: functional analysis of vertebral mobility and backbone properties in carnivorans. *J Anat* 244(2):205–. doi:10.1111/joa.13955. PMID 37837214. PMCID PMC10780164. | https://doi.org/10.1111/joa.13955 · https://europepmc.org/article/PMC/PMC10780164 | **Open: CC BY 4.0** (licence statement read in the full-text XML). | Full-text XML from the Europe PMC REST API OK. DOI and EPMC page: curl 403. | 4 |
| S17 | Schilling N, Hackert R (2006). Sagittal spine movements of small therian mammals during asymmetrical gaits. *J Exp Biol* 209(19):3925–3939. doi:10.1242/jeb.02400. PMID 16985208. HAL hal-00365971. | https://doi.org/10.1242/jeb.02400 · https://hal.science/hal-00365971/file/3925.pdf | Published paper, © CoB. Author copy on HAL (no CC licence stated). Cited facts. | HAL PDF: curl 200, read in full. DOI: 403. | 4 |
| S18 | Zips S, Peham C, Scheidl M, Licka T, Girtler D (2001). Motion pattern of the toelt of Icelandic horses at different speeds. *Equine Vet J Suppl* (33):109–111. doi:10.1111/j.2042-3306.2001.tb05371.x. PMID 11721548. | https://pubmed.ncbi.nlm.nih.gov/11721548/ · https://doi.org/10.1111/j.2042-3306.2001.tb05371.x | Published paper. Cited facts. | PubMed abstract OK. DOI: 403. | 1f |
| S19 | Biknevicius AR, Mullineaux DR, Clayton HM (2006). Locomotor mechanics of the tölt in Icelandic horses. *Am J Vet Res* 67(9):1505–1510. doi:10.2460/ajvr.67.9.1505. PMID 16948593. | https://doi.org/10.2460/ajvr.67.9.1505 · https://pubmed.ncbi.nlm.nih.gov/16948593/ | Published paper. Cited facts. | DOI: curl 200. PubMed abstract OK. | 1f |
| S20 | Renous S, Herbin M, Gasc J-P (2004). Contribution to the analysis of gaits: practical elements to complement the Hildebrand method. *C R Biologies* 327(2):99–103. doi:10.1016/j.crvi.2003.11.005. | https://doi.org/10.1016/j.crvi.2003.11.005 · PDF https://comptes-rendus.academie-sciences.fr/biologies/item/10.1016/j.crvi.2003.11.005.pdf | © Académie des sciences / Elsevier, "All rights reserved", free to read. Cited facts. | curl 200, PDF read. | 1f |
| S21 | Farley CT, Taylor CR (1991). A mechanical trigger for the trot–gallop transition in horses. *Science* 253(5017):306–308. doi:10.1126/science.1857965. PMID 1857965. | https://pubmed.ncbi.nlm.nih.gov/1857965/ | Paywalled. Cited facts. | PubMed abstract OK. | 2 |
| S22 | Hoyt DF, Taylor CR (1981). Gait and the energetics of locomotion in horses. *Nature* 292:239–240. doi:10.1038/292239a0. | https://www.nature.com/articles/292239a0 | Paywalled (Springer Nature). | curl 200 and WebFetch OK (abstract only: 3 horses, 110–170 kg). | 2 (no numbers taken) |
| S23 | Hildebrand M (1959). Motions of the running cheetah and horse. *J Mammal* 40(4):481–495. doi:10.2307/1376265. | https://doi.org/10.2307/1376265 · https://academic.oup.com/jmammal/article-abstract/40/4/481/893218 | Paywalled (OUP/JSTOR). | DOI: 403. Primary not read. | 3, 4 (U rows; cited figures via S17 and S24) |
| S24 | Original Wisdom, "Tracking Literature" summaries of Hildebrand 1959 and Heglund & Taylor 1988 (web pages, updated 9 Nov 2026). Secondary, not peer-reviewed. | https://www.originalwisdom.com/tracking-literature/1959-hildebrand-motions-running-cheetah-horse/ · https://www.originalwisdom.com/tracking-literature/1988-heglund-taylor-speed-stride-frequency-energy/ | Website, © the site. Used only as leads; every number from it is marked U. | curl 200, WebFetch OK. | U rows only |
| S25 | Cartmill M, Lemelin P, Schmitt D (2002). Support polygons and symmetrical gaits in mammals. *Zool J Linn Soc* 136(3):401–420. doi:10.1046/j.1096-3642.2002.00038.x. | https://doi.org/10.1046/j.1096-3642.2002.00038.x | Published paper. Cited facts. | Abstract read via OpenAlex. DOI: 403. | Definitions of diagonality D and duty factor S (Hildebrand convention: % of cycle by which LH footfall precedes LF) |
| S26 | Muybridge E (1887). *Animal Locomotion: an electro-photographic investigation of consecutive phases of animal movements, 1872–1885.* Philadelphia: University of Pennsylvania. 781 plates (Vol. IX: Horses). | https://commons.wikimedia.org/wiki/File:Animal_locomotion._Plate_626_(Boston_Public_Library).jpg · https://www.digitalcommonwealth.org/collections/commonwealth:70795t73j | **Public domain in the US** (published 1887). Wikimedia notes it may not be public domain in some countries outside the US. The Boston Public Library scans are on Commons. | curl 200 and WebFetch OK (rights statement read). | Reference imagery only; no numbers extracted |

Every link above is reached by DOI, PubMed, PMC/Europe PMC or publisher URL. Nothing was downloaded into any repository. Scratch reading copies are in `/workspace/c2work/` only (outside any repo).
