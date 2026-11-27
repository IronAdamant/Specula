# Second check: R8-C2-curl-noise-and-plume-rise.md

Re-read against raw/round8/bridson07-curlnoise.flow.txt, isc3v2.flow.txt and cushman-ch8-plumes.flow.txt with a page-locating script. One equation page image was checked (ISC3 PDF 22).

| row | claim | source page | result |
|---|---|---|---|
| 1 | L1 citation: ACM SIGGRAPH 2007 Papers, Article 46, doi 10.1145/1275808.1276435 | Crossref | VERIFIED |
| 2 | L2 title, number EPA-454/B-95-003b, Sept 1995 | ISC3 PDF 1 | VERIFIED |
| 3 | S1 is Chapter 8 "Local Air Pollution", printed pp. 181-209; author inferred from host | S1 PDF 1 (181), Chapter 9 starts at 209; directory listing | VERIFIED (authorship WEAK: no name printed in the chapter PDF) |
| 4 | Curl formulas in 3D and 2D, Eqs. 1-2 | L1 PDF 2 | VERIFIED |
| 5 | Divergence of a curl is zero; no gutters; plain Perlin noise makes sinks | L1 PDF 1-2 | VERIFIED |
| 6 | Finite-difference step 1e-4 of the domain, single precision | L1 PDF 2 | VERIFIED |
| 7 | 3D: three noises at large offsets | L1 PDF 2 | VERIFIED |
| 8 | Vortices about L across, speeds about 1/L | L1 PDF 2 | VERIFIED |
| 9 | Octaves with power law (Kolmogorov); time-varying noise | L1 PDF 2 | VERIFIED |
| 10 | Modulate the potential, not the velocity | L1 PDF 2 Sec. 2.3 | VERIFIED |
| 11 | No-slip: ramp the potential to zero; slip: Eq. 3 with d0 = L | L1 PDF 2 | VERIFIED |
| 12 | Ramp polynomial, Eq. 4; my check of the end values and slopes | L1 PDF 2; my arithmetic | VERIFIED |
| 13 | 3D Eq. 5 with alpha and n | L1 PDF 2 | VERIFIED |
| 14 | Medial-axis spikes; fall back to Eq. 3 near sharp edges | L1 PDF 3 | VERIFIED |
| 15 | Rigid potential Eq. 6; gradient freedom; vortex Eqs. 7-8; deforming bodies left open | L1 PDF 3 | VERIFIED |
| 16 | FlowNoise pseudo-advection named as untried | L1 PDF 2 | VERIFIED |
| 17 | u_s not below 1.0 m/s | ISC3 PDF 18 | VERIFIED |
| 18 | F_b = g v d^2 dT/(4 Ts), Eq. 1-8 | ISC3 PDF 19 | VERIFIED |
| 19 | S1 F = g r^2 w (1 − Ta/Ts), Eq. 8.14 | S1 PDF 18 | VERIFIED |
| 20 | Gradual rise 1.60 F^(1/3) x^(2/3)/u_s, Eq. 1-22; capped at final rise | ISC3 PDF 23 | VERIFIED |
| 21 | x_f = 49 F^(5/8) or 119 F^(2/5); 3.5 x* | ISC3 PDF 20-21; S1 PDF 19 | VERIFIED |
| 22 | Final rise 21.425 F^(3/4)/u and 38.71 F^(3/5)/u | ISC3 PDF 21 | VERIFIED |
| 23 | My equivalence check 1.6 x 49^(2/3) = 21.42, 1.6 x 119^(2/3) = 38.71 | my arithmetic (python) | VERIFIED |
| 24 | Momentum rise 3 d v/u; best when v/u > 4 | ISC3 PDF 21 | VERIFIED |
| 25 | Crossover dT constants 0.0297 and 0.00575 | ISC3 PDF 20 | VERIFIED |
| 26 | Stable s; E 0.020 and F 0.035 K/m; 2.6 (F/(u s))^(1/3) | ISC3 PDF 22 | VERIFIED |
| 27 | S1 stable forms 2.6 (F/(N^2 u))^(1/3), 4.0 (F/N^3)^(1/4), 0.275 (F N)^(1/4) | S1 PDF 19 | VERIFIED |
| 28 | Eq. 1-19 has a square root of s in the denominator | ISC3 PDF 22 (text layer lost the sign; Eq. 1-18's image shows √s) | WEAK (my reading) |
| 29 | Downwash 2 d (1.5 − v/u) equals S1's 4r (1.5 − w/u) | ISC3 PDF 19; S1 PDF 20 | VERIFIED |
| 30 | Bent-over in classes A-D, vertical in E-F | S1 PDF 18 | VERIFIED |
| 31 | Urban sigma_y and sigma_z table | ISC3 PDF 32 (Tables 1-3, 1-4) | VERIFIED |
| 32 | Rural uses Turner (Pasquill-Gifford) curves | ISC3 Sec. 1.1.5 (Turner 1970 cited; Tables 1-1, 1-2) | VERIFIED |
| 33 | S1 calls Eq. 8.17 dimensionally consistent | S1 PDF 19 | VERIFIED |

No FIX rows: every page cite matched on first re-read.
