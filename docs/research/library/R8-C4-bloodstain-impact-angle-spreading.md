# R8-C4. Bloodstain shape: impact angle from width/length, spreading against Weber and Reynolds numbers

Round 8, Section C, item 4. Private notes in my own words; no code from sources. Pages are PDF pages of the copy named, with printed pages where they differ. **(mine)** marks my own arithmetic or inference.

## Sources

| Tag | Role | Citation | Free legal copy | Licence |
|---|---|---|---|---|
| L1 | Listed | L. Hulse-Smith, N. Z. Mehdizadeh, S. Chandra, "Deducing Drop Size and Impact Velocity from Circular Bloodstains", *Journal of Forensic Sciences* 50(1):54-63, 2005. doi:10.1520/JFS2003224 (Crossref confirmed) | **None found.** ASTM/Wiley paywall. Content below comes through L2's account of it. | ASTM / Wiley |
| L2 | Listed (open review) | D. Attinger, C. Moore, A. Donaldson, A. Jafari, H. A. Stone, "Fluid dynamics topics in bloodstain pattern analysis: Comparative review and research opportunities", *Forensic Science International* 231(1-3):375-396, September 2013. doi:10.1016/j.forsciint.2013.04.018 (Crossref confirmed) | Iowa State University Digital Repository version (cover page, then the manuscript with its own page numbers; printed page = PDF − 5 in Sec. 5): http://lib.dr.iastate.edu/me_pubs/135 ; the file I downloaded is the same repository PDF re-hosted on the first author's consulting site: https://attinger-consulting.com/wp-content/uploads/2025/10/2013-Fluid-dynamics-topics-in-bloodstain-pattern-analysis_-Comparative.pdf . Raw: attinger13-review.pdf | Repository cover: "free and open access"; no CC licence stated |
| S1 | Substitute for L1 (an open, later and more general spreading law, tested on blood) | N. Laan, K. G. de Bruin, D. Bartolo, C. Josserand, D. Bonn, "Maximum Diameter of Impacting Liquid Droplets", *Physical Review Applied* 2(4):044018, 2014. doi:10.1103/PhysRevApplied.2.044018 (Crossref confirmed) | University of Amsterdam repository (UvA-DARE), final published version: https://pure.uva.nl/ws/files/2345931/160784_Maximum_Diameter_of_Impacting_Liquid_Droplets.pdf (PDF 1 is the UvA cover; printed 044018-n = PDF n+1). Raw: laan14-maxdiam.pdf | APS copyright; UvA terms allow personal use only |

## C4-1. Impact angle from stain shape

- **Rule:** the width-to-length ratio of an elliptical stain equals the sine of the impact angle, W/L = sin(alpha). Here alpha is measured from the surface, so 90 degrees is a perpendicular hit (L2 printed p. 35 = PDF 40, Eq. 10; S1 printed 044018-5 = PDF 6).
  - Where the rule comes from: L2 says W/L rises steadily with impact angle, as seen by Balthazard et al. 1939, and that the trigonometric form was set out by Rizer 1955 (*Police Mathematics*, pp. 72-73). S1 cites Balthazard et al. 1939 for the relation. **The two sources credit different originators.**
  - Definitions (L2 PDF 40): W is the minor axis, across the direction of travel. L is the major axis, along the projection of the velocity onto the surface.
- **Why L exceeds W (L2 PDF 40):** the sideways spread W is driven only by the velocity component normal to the surface. The along-track length gets that same spread plus the extra length from the slanted path cutting the surface.
- **Worked example (L2 printed p. 37 = PDF 42, Fig. 10 caption):** a 4 mm blood drop hitting a surface at 28 degrees and 4.3 m/s left a stain with W/L = 0.44, which the formula turns into 26.1 degrees.

**Table C4-1a (computed by me): angle from W/L**

| W/L | 0.20 | 0.44 | 0.50 | 0.70 | 0.90 | 1.00 |
|---|---|---|---|---|---|---|
| alpha (degrees) | 11.5 | 26.1 | 30.0 | 44.4 | 64.2 | 90.0 |

**Failure cases:**
- A non-round stain does not prove an oblique impact. If the target moves sideways relative to the drop, the stain is stretched too (L2 PDF 40, citing Pizzola et al. 1986).
- On larger drops and steep slopes, liquid keeps running down the surface after impact and lengthens L. S1 therefore uses only the short axis W to estimate the impact (S1 PDF 6).
- L2 says the error of Eq. 10 is estimated in its refs [89] and [121] (Wells 2006; Willis et al. 2001), but gives no numbers (PDF 40).
- Misconception (L2 printed p. 36 = PDF 41): the stain width is *not* generally equal to the drop diameter. In Fig. 10, W is much larger than d0.

## C4-2. Spreading against Re and We

- **Spread factor:** d_max/d0 is the maximum contact diameter divided by the drop diameter. In general it depends on Re, We and Fr (L2 printed p. 32 = PDF 37).
- **Hulse-Smith et al. (L1, as given in L2 PDF 37, Eq. 8, checked on the page image):** d_max/d0 ≈ (1/2)·Re^(1/4). L2 says it was argued for cases where We is much larger than Re, with gravity ignored during impact.
- **Spine count, the "method of spines" (L1 via L2 printed p. 33 = PDF 38, Eq. 9, checked on the page image):** N = C·√We, with C a target-material constant of order 1.
  - Eqs. 8 and 9 together give d0 and v0 from the stain diameter and the spine count, provided the blood's rho, mu and sigma and the target's C are known.
  - Limits L2 reports (from Adam 2012 and Hulse-Smith and Illes): low-energy impacts make no spines; the spine count saturates at high We; spines can merge or blur into edge ripples, so counting is subjective. Hulse-Smith and Illes recommend that a single person count all spines in a given study (PDF 38).
- **Regimes (L2 printed p. 34 = PDF 39):**
  - As inertia grows against capillary and viscous forces, the stain goes from a smooth edge, to spines, to splashing.
  - Splash threshold (L2 printed p. 40 = PDF 45, Eq. 11): Re·We^2 = K^4, with K a target-dependent constant estimated at 57.7 (L2's ref [136]). **(mine)** This is the same as We·Re^(1/2) = K.
- **S1's universal law (S1 printed 044018-3 to -4 = PDF 4-5):**
  - S1 tested water, two water-glycerol mixtures and blood at 0.5-6 m/s. No single power law (Re^(1/5), Re^(1/4), We^(1/2) or We^(1/4)) collapsed all the data. *This directly challenges a pure Re^(1/4) law such as L1's Eq. 8 outside its stated We >> Re assumption.*
  - S1's fit: with P = We·Re^(-2/5), (D_max/D0)·Re^(-1/5) = P^(1/2)/(A + P^(1/2)), A = 1.24 ± 0.01, R^2 = 0.95. A momentum-based alternative fitted worse (R^2 = 0.83).
  - Range: 10 < We < 1700 and 70 < Re < 17000 (S1 PDF 5).
  - Limits: P much greater than 1 recovers Re^(1/5) (viscous); P much less than 1 recovers We^(1/2) (capillary).
- **Which viscosity to use for blood:**
  - S1: blood is shear-thinning, but its points fall on the same curve when the high-shear viscosity eta_inf = 4.8 mPa·s is used. Fit: eta = eta_inf + k·gamma_dot^(n−1), with k = 38 mPa·s^n and n = 0.41 (S1 PDF 5, Fig. 4).
  - L2 reaches the same practical answer by argument. Impact shear rates are at least about 1000 s^-1, so high-shear viscosities apply to the early spreading; later stages and drying run at lower shear and higher viscosity (L2 printed pp. 38-40 = PDF 43-45).
- **Oblique version (S1 printed 044018-5 = PDF 6, Eq. 4):** use only the normal velocity v·sin(alpha) for the short axis. This gives W_max/D0 = Re^(1/5)·P^(1/2)·sin(alpha) / (A + P^(1/2)·(sin alpha)^(4/5)). **(mine)** I re-derived this by putting v·sin(alpha) into Eq. 3: Re scales by sin alpha and P by (sin alpha)^(8/5), which reproduces the printed form.
- **Accuracy (S1 PDF 6):** impact speeds back-calculated from stain width, volume and angle averaged 1.004 times the measured speed, with a standard deviation of 10%.

## C4-3. Is the dried stain the maximum spread? (sources differ)

- **S1 (PDF 5):** yes, for their conditions. The blood contact line stays pinned after reaching its maximum, unlike water on a hydrophobic surface, which retracts. So the dried stain diameter equals D_max.
- **L2 (printed pp. 34-35 = PDF 39-40):** treat this as a questionable assumption. It is probably fine on hydrophilic surfaces, but on hydrophobic ones such as greasy tiles the stain can end up smaller than the maximum spread (citing Balthazard et al.). Also, stain size gives fall distance only if the drop size is known.
- **Target dependence (L2 PDF 39):** most blood impact studies used paper, so their results strictly apply to paper. Balthazard limited his results to one type of cardboard, and roughness promotes splashing.

## Known problems in later work

- S1 shows that the single-exponent laws in earlier literature, including the Re^(1/4) form used by L1, do not collapse data across liquids (S1 PDF 4).
- Citation errors in L2's reference list (mine, checked against Crossref): ref [126] Yarin is printed as "vol. 38, pp. 159-159, 2005", but Crossref gives *Annu. Rev. Fluid Mech.* 38:159-192, 2006.

## Not found or not verified

- L1's own text: its exact fitting data, validity limits and C values per surface are known only through L2.
- Numeric error bands for W/L = sin(alpha): L2 cites them but does not state them.
