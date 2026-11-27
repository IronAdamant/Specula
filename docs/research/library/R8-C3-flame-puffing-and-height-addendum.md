# R8-C3. Pool-fire puffing frequency and flame height: addendum to round 3's J11

Round 8, Section C, item 3. The brief asked me to check against round 3's J11 (`specula-reference-r3/library/J11-fireballs-and-flames.md`) and add only what is missing.

**What J11 already has (not repeated here):**
- Heskestad L = 0.235·Q^(2/5) − 1.02·D, and the 0.23 variant with 7 < Q^(2/5)/D < 700, from secondary sources (wiki, ATF, Wiley appendix extracts).
- McCaffrey persistent and intermittent heights.
- Virtual origin, plume mass flow, centreline temperature and speed.
- The 0.235 against 0.23 disagreement.

**What J11 lacks, added below:**
1. Any puffing (flicker) frequency.
2. Heskestad's non-dimensional form and its definition of Q*, from a free primary-grade document.
3. A check of how the 0.235 constant relates to the non-dimensional 3.7.

Pages are PDF pages of the copy named, with printed pages where they differ. **(mine)** marks my own arithmetic or inference.

## Sources

| Tag | Role | Citation | Free legal copy | Licence |
|---|---|---|---|---|
| L1 | Listed | B. M. Cetegen, T. A. Ahmed, "Experiments on the periodic instability of buoyant plumes and pool fires", *Combustion and Flame* 93(1-2):157-184, April 1993. doi:10.1016/0010-2180(93)90090-P | **None found.** Elsevier paywall. OSTI has only a bibliographic record with the abstract (osti.gov/biblio/6537545; saved as raw/round8/cetegen93-osti.json). | Elsevier copyright |
| L2 | Listed | G. Heskestad, "Luminous heights of turbulent diffusion flames", *Fire Safety Journal* 5(2):103-108, 1983 (cited as [Heskestad:FSJ1983] in S1); also Heskestad, SFPE Handbook chapter "Fire Plumes, Flame Height and Air Entrainment" | **None found**; not looked up beyond S1's citation. NUREG-1805 Ch. 3 (US NRC, public domain) is a free secondary copy, but both nrc.gov URLs I tried returned an Akamai "Access Denied" page (saved as nureg1805-ch3*.FAILED-not-a-pdf.html). | Elsevier / SFPE |
| S1 | Substitute for L2 (formula and Q*) and for L1 (puffing scaling, through Zukoski) | K. McGrattan, S. Hostikka, J. Floyd, R. McDermott, M. Vanella, E. Mueller, C. Paul, *Fire Dynamics Simulator Technical Reference Guide, Volume 3: Validation*, NIST Special Publication 1018-3, Sixth Edition, revision FDS-6.11.2, dated November 14, 2026. doi:10.6028/NIST.SP.1018 | https://github.com/firemodels/fds/releases/download/FDS-6.11.2/FDS_Validation_Guide.pdf (1324 PDF pages; raw/round8/fds-validation-guide-6.11.2.pdf). Printed page = PDF − 56 in the sections used. | US government work (NIST), not subject to US copyright |
| S2 | Substitute for L1 (the 1.5/√D coefficient as quoted by a later open paper) | M. Taha, S. Zhao, A. Lamorlette, J.-L. Consalvi, P. Boivin, "Large eddy simulation of fire-induced flows using Lattice-Boltzmann methods", *International Journal of Thermal Sciences* 197:108801, 2024. doi:10.1016/j.ijthermalsci.2023.108801 | HAL author manuscript hal-04338538v1: https://hal.science/hal-04338538v1/file/main_final.pdf (40 PDF pages; printed page = PDF − 1). Raw: hal-04338538.pdf | HAL authorisation licence (https://about.hal.science/hal-authorisation-v1/), per the HAL API |

## C3-1. Puffing frequency (missing from J11)

- **What it is (L1 abstract, OSTI record):** near the base of a buoyant flame or plume, a toroidal vortex forms within about one burner diameter of the source, rises, and the cycle repeats. L1 found that helium plumes (no combustion) at 1.10 m and 0.30 m diameters also puffed, with frequencies scaling like flames, as D^(-1/2). Puffing continued as long as a diffusion flame was sustained, but weakened as the fuel was diluted. L1 ties the D^(-1/2) scaling to the convection speed of the vortices in the first diameter of height.
- **L1's correlation, as quoted by S2:** f = 1.5/√D (f in Hz, D in m) for buoyant diffusion flames (pool fires) of various fuels. S2 evaluates it as 1.5 Hz at D = 1 m (S2 printed pp. 16-17 = PDF 17-18, Sec. 3.2.3). The abstract itself gives only the D^(-1/2) scaling, not the 1.5. **The coefficient 1.5 is therefore verified only through S2, not from L1's own text.**
- **Zukoski's form (S1 Sec. 6.3, printed p. 347 = PDF 403, Eq. 6.7):** S1 says Zukoski showed the mean puffing frequency goes as the inverse square root of the burner diameter, and writes f = 0.50·√(g/D). **(mine)** With g = 9.81 m/s², this is f = 1.566/√D, about 4% above Cetegen and Ahmed's 1.5.
- **Measured checks:**
  - Sandia 1 m methane pool fire, Test 17: 1.65 Hz measured (S1 printed p. 357 = PDF 413, citing Tieszen et al. 2002).
  - S2's 1 m helium plume: 1.37 ± 0.1 Hz measured. S2 compares it with Cetegen and Kasper's helium-plume correlation f = 0.8·Ri^0.38·U/D, which gives 1.34 Hz at Ri about 76 (valid for Ri < 100; Ri = (rho_inf − rho)·g·D/(rho_inf·U²)). S2 says the gap from 1.5/√D is expected, because that correlation was built for diffusion flames (S2 PDF 17-18).
- **Failure case (S2 PDF 17):** in S2's lattice-Boltzmann LES the coarse grid predicted 1.69 Hz, while the medium and fine grids gave 1.39 Hz. Under-resolved simulations overshoot the puffing frequency; S2 says the finer mesh is needed to capture the inlet instabilities that drive puffing.

**Table C3-1a (computed by me): puffing frequency in Hz**

| D (m) | 0.1 | 0.3 | 1 | 3 | 10 |
|---|---|---|---|---|---|
| Cetegen-Ahmed 1.5/√D | 4.74 | 2.74 | 1.50 | 0.87 | 0.47 |
| Zukoski 0.50·√(g/D) | 4.95 | 2.86 | 1.57 | 0.90 | 0.50 |

## C3-2. Heskestad flame height: non-dimensional form (adds to J11-4)

- **S1 Sec. 3.37 (printed p. 52 = PDF 108, Eq. 3.3), citing L2 and the SFPE Handbook:** L_f/D = 3.7·(Q*)^(2/5) − 1.02, with Q* = Q / (rho_inf·c_p·T_inf·√g·D^(5/2)), where rho_inf, c_p and T_inf are the ambient density, specific heat and temperature. S1 repeats it as L_f = D·(3.7·Q*^(2/5) − 1.02) (Eq. 3.39, printed pp. 166-167 = PDF 222-223).
- **What Q* means (S1 printed p. 166 = PDF 222):** S1 calls Q* the fire Froude number, roughly the ratio of fuel exit velocity to buoyant plume velocity. Jet fires have large Q*; typical accidental fires are near 1.
- **Definition of flame height (S1 Sec. 6.2, printed p. 341 = PDF 397):** experiments use the height where luminous flame is seen 50% of the time. FDS instead uses the height at which 95-99% of the heat has been released.
- **Validity caveat (S1 Sec. 6.2.1, printed p. 341 = PDF 397):** Heskestad's is one of many flame-height correlations. Their spread is significant, especially at low Q*, where burner details such as shape matter. **(mine)** J11's 7 < Q^(2/5)/D < 700 range is the dimensional version of the same caution.
- **Range S1 compared over (Table 3.56, printed p. 168 = PDF 224):** FDS was checked against Heskestad from 10^2 to 10^7 kW at D = 1.1 m (Q* 10^-1 to 10^4). That is the range of S1's simulations, not a published validity limit for L2.

## C3-3. Constant check: 3.7 against 0.235 (mine)

Turning 3.7·Q*^(2/5)·D into Q^(2/5) gives a constant of 3.7 / (rho·c_p·T·√g)^(2/5):
- With rho = 1.2 kg/m³, c_p = 1.0 kJ/(kg·K) and T = 293 K, the group is 1101 and the constant is **0.225**.
- With 1.204, 1.005 and 293.15, it is **0.224**.
- Going the other way, J11's 0.235 corresponds to a non-dimensional constant of about **3.87-3.88**.

So the 0.23 form in J11 matches S1's 3.7. The 0.235 form is about 4-5% higher than 3.7 at normal ambient conditions. This sharpens J11's "0.235 against 0.23, about 2%" note: the gap between the two dimensional forms is about 2%, but 0.235 against the non-dimensional 3.7 is about 4-5%. I did not find which ambient conditions Heskestad assumed for 0.235, because L2 was not read.

## Not found or not verified

- L1's own text: the 1.5 coefficient, its fit range in D, and the fuels used. These come only through S2 and the OSTI abstract.
- L2's own text and its stated range of validity.
- NUREG-1805 Ch. 3: download blocked (Access Denied).
