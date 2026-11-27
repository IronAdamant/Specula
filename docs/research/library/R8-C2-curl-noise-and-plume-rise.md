# R8-C2. Curl-noise velocity fields and buoyant plume rise in wind

Round 8, Section C, item 2. Private notes, in my own words; no code from sources. Pages are PDF pages of the copy named, plus printed page numbers where they differ. **(mine)** marks my own inference or arithmetic.

## Sources

| Tag | Role | Citation | Free legal copy | Licence |
|---|---|---|---|---|
| L1 | Listed | R. Bridson, J. Hourihan, M. Nordenstam, "Curl-Noise for Procedural Fluid Flow", *ACM SIGGRAPH 2007 Papers* (ACM TOG 26(3)), Article 46, 2007. doi:10.1145/1275808.1276435 (Crossref confirmed) | Author copy: https://www.cs.ubc.ca/~rbridson/docs/bridson-siggraph2007-curlnoise.pdf (3 pages). Raw: raw/round8/bridson07-curlnoise.pdf | ACM copyright; author-hosted |
| L2 | Listed (Briggs plume rise, open statement) | U.S. EPA, *User's Guide for the Industrial Source Complex (ISC3) Dispersion Models, Volume II: Description of Model Algorithms*, EPA-454/B-95-003b, September 1995, Sec. 1.1.4 (Briggs formulas with page cites to Briggs 1969, 1971, 1972, 1975) | https://gaftp.epa.gov/Air/aqmg/SCRAM/models/other/isc3/isc3v2.pdf . Raw: isc3v2.pdf. Printed page "1-n" = PDF n+13 | US federal government work, public domain in the US |
| S1 | Second open statement of the same Briggs formulas (cross-check) | B. Cushman-Roisin (author inferred from the host name), Dartmouth ENGS 43 course notes, Chapter 8 "Local Air Pollution", Sec. 8.4 "Smokestack plumes", printed pp. 181-209 (PDF built 2008) | https://cushman.host.dartmouth.edu/courses/engs43/Chapter8.pdf . Raw: cushman-ch8-plumes.pdf. Printed p. = PDF + 180 | No licence stated; author-hosted course notes |
| -- | Briggs' own reports (1969 *Plume Rise*, AEC Critical Review Series; 1971, 1972, 1975) | Not read. ISC3 cites them by page. | Not looked for beyond L2 | -- |

## C2-1. Curl-noise (L1)

**Construction (L1 PDF 2, Sec. 2.1-2.2).**
- Velocity is the curl of a potential. In 3D the potential is a vector (psi1, psi2, psi3) and v = (d psi3/dy − d psi2/dz, d psi1/dz − d psi3/dx, d psi2/dx − d psi1/dy) (Eq. 1). In 2D the potential is a scalar (the stream function, whose contours are streamlines) and v = (d psi/dy, −d psi/dx) (Eq. 2).
- **Why it is divergence-free:** the divergence of a curl of a smooth field is identically zero, so v has no sources or sinks. Plain Perlin noise used directly as velocity is not divergence-free and so gathers particles into "gutters" (sinks) (L1 PDF 1).
- Derivatives are taken by finite differences with a very small step. Their examples used 10^-4 of the domain size, which they say is fine in single precision (PDF 2).
- Noise: in 2D psi = N(x); in 3D use three apparently uncorrelated noises, in practice the same noise evaluated at large offsets (PDF 2).
- Scale: with lattice noise in [−1, 1], N(x/L) has derivatives of order 1/L. So expect vortices about L across and speeds up to about 1/L; scale psi to set the speed (PDF 2).
- Octaves: summing octaves with a power-law fall-off in amplitude resembles physical turbulence (they point to the Kolmogorov spectrum). Time variation comes from time-varying noise (PDF 2).

**Modulation (Sec. 2.3, PDF 2):** multiply the *potential*, not the velocity, by an amplitude A(x). A(x)·v is no longer divergence-free, but the curl of A(x)·psi is.

**Solid boundaries (Sec. 2.4, PDF 2-3).**
- No-slip (v = 0): ramp the potential smoothly to zero near the solid.
- Slip (v·n = 0), the case they care about: in 2D, multiply psi by ramp(d(x)/d0), where d is the distance to the nearest solid and d0 is the width of the blend region (set d0 = L for noise of scale L) (Eq. 3). The ramp is the odd quintic 15/8 r − 10/8 r^3 + 3/8 r^5 on (−1, 1), clamped to ±1 outside (Eq. 4). **(mine)** It equals ±1 with zero slope at r = ±1.
- In 3D: with alpha = |ramp(d/d0)| and n the unit normal at the closest boundary point, keep the normal component of psi and scale only the tangential part by alpha (Eq. 5).
- **Failure case (stated by L1, PDF 3):** the normal field can jump across the medial axis of the geometry. Using Eq. 5 naively can then give spikes in the curl, especially near sharp edges. Their workaround: within d0 of edges flagged as sharp, fall back to Eq. 3 (drop the normal component). They leave a better fix to future work.

**Other potentials (Sec. 2.5, PDF 3):**
- Rigid motion with linear velocity V and angular velocity omega: psi = V × (x − x0) + ((R^2 − |x − x0|^2)/2)·omega (Eq. 6).
- Moving bodies: zero the normal part with Eq. 5, then add A(x)·psi_rigid, blended to zero away from the body.
- Vortex particle and vortex curve (for smoke rings and plumes): Eqs. 7-8.
- Potentials that differ by a gradient give the same velocity.
- Stated limitation: deforming bodies that change volume may have no valid solution; left for future work.

**Known limits (mine, labelled).**
1. Finite-difference curls are divergence-free only up to truncation error. The identity is exact only for the true derivatives.
2. Curl-noise has no dynamics: vortices do not advect each other. L1 itself suggests FlowNoise-style pseudo-advection as untested future work (PDF 2).

No later paper reporting a bug in L1 was located in this round.

## C2-2. Briggs plume rise (L2, cross-checked with S1)

Symbols: F_b is the buoyancy flux (m^4/s^3); u_s is the wind speed at stack height (L2 does not allow it below 1.0 m/s; PDF 18, printed 1-5); x is the downwind distance (m); v_s is the exit velocity; d_s is the stack inner diameter; T_s and T_a are the stack-gas and ambient temperatures (K).

- **Buoyancy flux:** F_b = g·v_s·d_s^2·(T_s − T_a)/(4 T_s) (L2 Eq. 1-8, printed 1-6 = PDF 19). S1 writes the same quantity as g·r^2·w_s·(1 − T_a/T_s) with r the stack radius (S1 Eq. 8.14, printed p. 198 = PDF 18). **(mine)** These are identical, since d = 2r.
- **Gradual (transitional) rise, the 2/3 law:** before final rise, the plume centreline rises by Δh = 1.60·F_b^(1/3)·x^(2/3)/u_s (L2 Eq. 1-22, Sec. 1.1.4.10, printed 1-10 = PDF 23, citing Briggs 1972 p. 1030). This is the "Δh ∝ F^(1/3) x^(2/3)/u" form the brief asked for. L2 uses it only for buoyancy-dominated plumes, and it is capped at the final rise for the stability class (PDF 23).
- **Distance to final rise (neutral or unstable):** x_f = 49·F_b^(5/8) for F_b < 55, and x_f = 119·F_b^(2/5) for F_b ≥ 55 (metres) (L2 Eqs. 1-12, 1-13, PDF 20-21; S1 Eqs. 8.15-8.16, printed p. 199 = PDF 19). L2 says this is 3.5·x*, where x* is the distance at which atmospheric turbulence takes over entrainment (PDF 20).
- **Final rise (neutral or unstable), L2:** Δh = 21.425·F_b^(3/4)/u_s for F_b < 55, and 38.71·F_b^(3/5)/u_s for F_b ≥ 55 (Eqs. 1-14, 1-15, printed 1-8 = PDF 21).
  - S1 instead puts x_f into the 2/3 law: Δh = 1.6·F^(1/3)·x_f^(2/3)/u (S1 Eq. 8.17, PDF 19).
  - **(mine)** These agree: 1.6·49^(2/3) = 21.43 and F^(1/3)·F^(5/12) = F^(3/4); likewise 1.6·119^(2/3) = 38.70 and F^(1/3)·F^(4/15) = F^(3/5). So L2's constants are S1's form evaluated in closed form.
- **Momentum-dominated jets (L2 Eq. 1-16, PDF 21):** Δh = 3·d_s·v_s/u_s. L2 says Briggs suggests this is most applicable when v_s/u_s > 4.
- **Which regime applies (neutral or unstable):** if T_s − T_a is at least a crossover value, the plume is buoyancy-dominated. The crossover is 0.0297·T_s·v_s^(1/3)/d_s^(2/3) for F_b < 55, and 0.00575·T_s·v_s^(2/3)/d_s^(1/3) for F_b ≥ 55 (L2 Eqs. 1-10, 1-11, PDF 20).
- **Stable air:**
  - Stability parameter s = (g/T_a)·(dθ/dz). Defaults are dθ/dz = 0.020 K/m for class E and 0.035 K/m for class F (L2 Eq. 1-17, PDF 22).
  - Final rise: Δh = 2.6·(F_b/(u_s·s))^(1/3) (Eq. 1-20, PDF 22).
  - S1's matching form is 2.6·(F/(N^2·u))^(1/3) when u ≥ 0.275·(F·N)^(1/4). In near-calm air it is 4.0·(F/N^3)^(1/4) (S1 Eqs. 8.18-8.19, PDF 19). **(mine)** s in L2 is S1's N^2.
  - Distance to final rise in stable air: L2 Eq. 1-19 has the constant 2.0715 times u_s, divided by the square root of s. The text layer loses the root sign, so the root is my reading; the image of Eq. 1-18 on the same page does show √s.
- **Stack-tip downwash (L2 Eq. 1-7, PDF 19; S1 Eq. 8.20, PDF 20):** when v_s < 1.5·u_s, the effective stack top is lowered by 2·d_s·(1.5 − v_s/u_s); otherwise there is no correction. S1 writes the same as 4r·(1.5 − w_s/u). **(mine)** These are the same.
- **Bent-over or vertical (S1 PDF 18):** S1 treats plumes as bent over in Pasquill classes A-D and vertical in E-F.
- **Units warning (mine):** the constants 21.425, 38.71, 49 and 119, and the 55 m^4/s^3 switch point, are dimensional. They only hold in SI units (m, s, K). S1 calls the 2/3 law itself dimensionally consistent (PDF 19).

## C2-3. Plume width against distance (spread)

L2 Table 1-3 and Table 1-4 (printed 1-19 = PDF 32) give the Briggs urban (McElroy-Pooler) widths sigma_y and sigma_z in metres, with x in metres:

| Class | sigma_y | sigma_z |
|---|---|---|
| A | 0.32 x (1 + 0.0004 x)^(-1/2) | 0.24 x (1 + 0.001 x)^(1/2) |
| B | 0.32 x (1 + 0.0004 x)^(-1/2) | 0.24 x (1 + 0.001 x)^(1/2) |
| C | 0.22 x (1 + 0.0004 x)^(-1/2) | 0.20 x |
| D | 0.16 x (1 + 0.0004 x)^(-1/2) | 0.14 x (1 + 0.0003 x)^(-1/2) |
| E | 0.11 x (1 + 0.0004 x)^(-1/2) | 0.08 x (1 + 0.0015 x)^(-1/2) |
| F | 0.11 x (1 + 0.0004 x)^(-1/2) | 0.08 x (1 + 0.0015 x)^(-1/2) |

L2 uses Pasquill-Gifford curve fits (Turner) for rural sites in a separate set of tables, not copied here. **(mine)** The centreline height comes from C2-2 (the stack height plus the rise, growing as x^(2/3) until x_f). The visible plume half-width is a few sigma, and stronger wind lowers the rise as 1/u. No source I read gives a separate plume *radius* law tied to the rise, such as the entrainment-coefficient form r ≈ beta·z. **Not found here.**

## Not found or not verified

- Briggs' original reports were not read; L2 cites their pages.
- A later paper documenting a bug in curl-noise: none located.
- L2 Eq. 1-19's square root is an inference from a garbled text layer.
