# R7-5b Numbers from the browser-downloaded sources (round 7)

Compiled 2026-11-24. These six files were obtained in a human browser after scripted downloads were blocked (bot checks, timeouts, or a non-file page). No sign-in, captcha bypass, pirate site or unofficial upload was used. Every number below was read from the file named in the Source column; values marked **computed** are my arithmetic on the source's numbers. Journal page numbers are used for Henderson and Cruse; PDF page numbers (HAL cover = PDF p. 1) for Mei and Cordonnier; ACM printed pages 41-50 for Musgrave.

## Sources

| ID | Source | Status | Free route used | Licence / rights | File in sources/browser-downloads/ |
|---|---|---|---|---|---|
| Y1 | Henderson DM (2010). Pterosaur body mass estimates from three-dimensional mathematical slicing. *J. Vertebr. Paleontol.* 30(3): 768-785. doi:10.1080/02724631003758334 | Exact (B15), answers W4 | Wayback snapshot of the former doc.rero.ch copy, http://web.archive.org/web/20250708064108/https://doc.rero.ch/record/32754/files/PAL_E3629.pdf (saved as PAL_E3629.pdf). Starts %PDF-1.4; 18 pages | (c) 2010 Society of Vertebrate Paleontology; all rights reserved. Facts only | Henderson 2010 - Pterosaur body mass from 3D slicing.pdf (1,072,538 bytes) |
| Y2 | Mei X, Decaudin P, Hu B-G (2007). Fast hydraulic erosion simulation and visualization on GPU. PG '07, 15th Pacific Conference on Computer Graphics and Applications, Maui, Oct 2007, pp. 47-56. doi:10.1109/PG.2007.15. HAL inria-00402079 | Exact (A40), answers Q45 (hydraulic) | HAL PDF https://inria.hal.science/inria-00402079/file/FastErosion_PG07.pdf (bot check had blocked scripts). Starts %PDF-1.4; 11 pages including HAL cover | HAL author deposit; licence not stated on the cover. Cite the conference paper | Mei 2007 - Fast hydraulic erosion on GPU.pdf (785,720 bytes) |
| Y3 | Cordonnier G, Braun J, Cani M-P, Benes B, Galin E, Peytavie A, Guerin E (2016). Large scale terrain generation from tectonic uplift and fluvial erosion. *Comput. Graph. Forum* 35(2): 165-175 (EUROGRAPHICS 2016). doi:10.1111/cgf.12820. HAL hal-01262376 | Exact (A41), answers Q45 (stream-power) | HAL PDF https://inria.hal.science/hal-01262376/file/2016_cordonnier.pdf (bot check had blocked scripts). Starts %PDF-1.4; 12 pages including HAL cover | HAL author deposit; licence not stated on the cover. Cite the journal | Cordonnier 2016 - Terrain from tectonic uplift and fluvial erosion.pdf (10,771,406 bytes) |
| Y4 | Musgrave FK, Kolb CE, Mace RS (1989). The synthesis and rendering of eroded fractal terrains. *ACM SIGGRAPH Comput. Graph.* 23(3): 41-50. doi:10.1145/74334.74337 | Exact (B22), answers Q45 (thermal; also their hydraulic model) | ACM Digital Library PDF https://dl.acm.org/doi/pdf/10.1145/74334.74337 (scripts timed out). Starts %PDF-1.3; 10 pages | (c) 1989 ACM; the first page carries ACM's permission-to-copy notice (ACM-0-89791-312-4/89/007/0041 $00.75) | Musgrave 1989 - Eroded fractal terrains.pdf (5,828,847 bytes) |
| Y5 | Cruse H (1976). The function of the legs in the free walking stick insect, *Carausius morosus*. *J. Comp. Physiol.* 112: 235-262. doi:10.1007/BF00606541 | Exact (A14), answers U3 | Bielefeld repository green copy, https://pub.uni-bielefeld.de/record/1681261 (bot check had blocked scripts); saved as Cruse_11.pdf. Starts %PDF-1.3; 28 pages | (c) by Springer-Verlag 1976; OpenAlex lists the repository licence as "other-oa". Cite the journal | Cruse 1976 - Legs of the free walking stick insect.pdf (1,769,061 bytes) |
| Y6 | Castillo-Figueroa D (2020). Ecological morphology of Neotropical bat wing structures. *Zool. Stud.* 59: 60, supplement zoolstud-59-060-s001.docx (Table S1). doi:10.6620/ZS.2020.59-60 | Exact supplement (A27); does **not** close Q17's per-bone millimetre request | PMC instance file https://pmc.ncbi.nlm.nih.gov/articles/instance/8181164/bin/zoolstud-59-060-s001.docx (scripts had received a non-file page). ZIP/DOCX magic PK; 15,836 bytes; Word header "Zoological Studies 59: 60 (2020)"; created 2020-10-15, modified 2020-11-20 | Journal copyright; the docx itself states no licence. The article was already read; this is only the supplement | Castillo-Figueroa 2020 - Supplement S1 MDS loadings.docx (15,836 bytes) |

## W4. Mass, assumed span and density of *Quetzalcoatlus northropi* (Y1)

**U3-style confirmation of the brief's 544 kg:** yes, Table 2, p. 776, printed as 544 kg (no uncertainty). Assumed wingspan 11.2 m and axial body length 6.36 m (Table 1, p. 771; sources Wellnhofer 1991a and Langston 1981). Resulting bulk density 724 kg/m3 and total volume 0.751 m3 (Table 3, p. 777). **W4 closed from Henderson's own tables.**

### Assigned regional densities (Materials and Methods, pp. 771-773)

These are the densities Henderson applied to the models, not the resulting bulk densities in Table 3.

| Quantity | Value | Where |
|---|---|---|
| Muscled portions of the arms | 900 g/liter | p. 771 |
| Legs | 1000 g/liter | p. 771 |
| Bird and pterosaur trunk (pelvic + thoracic; birds also include caudal) | 850 g/liter (air sacs taken as about 15% of trunk volume; soft tissue 1000 g/liter) | pp. 771-773 |
| Head and neck (birds and pterosaurs) | 300 g/liter (from a domestic goose neck, Bramwell and Whitfield 1974) | pp. 771-773 |
| 'Rhamphorhynchoid' tails | 1000 g/liter | p. 773 |
| Pterodactyloid tails | 850 g/liter | p. 773 |
| Patagia | 1100 g/liter (slightly above water, for muscle, vessels and actinofibrils) | p. 773 |
| Patagium thickness | 0.015 cm ('rhamphorhynchoids') and 0.03 cm (pterodactyloids); 0.015 cm is the average thickness reported for *Pteropus giganteus* by Bramwell and Whitfield 1974 | p. 771 |
| Arm basal width | one quarter of the average length of humerus, ulna and fourth metacarpal; basal depth half that width; then reduced by a factor of two thirds at each subsequent joint | p. 771 |
| Leg basal width | one quarter of the average length of femur, tibia and pes; distal reduction factor 0.55 | p. 771 |

Mean bulk density of the six bird models is 806 g/liter (SD = 30.1); of the seven 'rhamphorhynchoids' 743 g/liter (SD = 85.7); of the seven pterodactyloids 680 g/liter (SD = 39.3) (Table 3, p. 777). A 10% change in transverse axial dimensions changed estimated mass by approximately 12%; the axial body is always more than 72% of mass, average 82% (p. 776, referring to Table 2).

### *Q. northropi* from the tables (and related model outputs)

| Quantity | Value | Where |
|---|---|---|
| Axial body length | 6.36 m | Table 1, p. 771 |
| Wingspan used | 11.2 m (conservative; Langston 1981). Lawson (1975) estimated 11 to 21 m and suggested 15.5 m | Table 1; p. 782 |
| Total mass | 544 kg | Table 2, p. 776; Table 3, p. 777 |
| Axial mass | 474 kg (87.2%) | Table 2 |
| Single leg / single arm / single patagium | 13.2 kg (2.43%); 26.0 kg (4.78%); 1.10 kg (0.203%). Patagial masses are for one side | Table 2 footnote |
| Total volume; bulk density | 0.751 m3; 724 kg/m3 | Table 3, p. 777 |
| Weight; full wing area; wing loading | 5.34 x 10^3 N; 13.4 m2 (92.4% of frontal area); 369 N/m2 | Table 4, p. 779 |
| Frontal area; axial body frontal area | 14.5 m2; 1.10 m2 (7.58%) | Table 4 |
| Mass relative to *Pteranodon longiceps* (18.6 kg) | "almost 30 times" ("29.2 to be exact") | p. 773 |

Fig. 10 caption (p. 781) states that wing loadings were computed using the estimated masses "multiplied [by] the gravitational acceleration constant of 9.81 m/s2". **Computed check:** 544 kg x 9.81 m/s2 = 5336.64 N, which rounds to the table's 5.34 x 10^3 N.

### Other pterosaur totals (Table 2, p. 776; span and length Table 1, p. 771; bulk density Table 3, p. 777)

Masses are kilograms. 'Rhamphorhynchoids': *Anurognathus ammoni* 0.00478 (span 0.216 m, length 0.0645 m, density 719 kg/m3); *Dimorphodon macronyx* 1.30 (1.42 m, 1.01 m, 630); *Eudimorphodon ranzii* 0.325 (1.00 m, 0.800 m, 723); *Jeholopterus ninchengensis* 0.0750 in Table 2 (span 0.773 m, length 0.202 m); *Preondactylus buffarinii* 0.0437 in Table 2 (span 0.446 m, length 0.400 m); *Rhamphorhynchus muensteri* 0.986 (1.62 m, 1.01 m, 913); *Sordes pilosus* 0.0758 (0.634 m, 0.415 m, 711). Pterodactyloids: *Anhanguera santanae* 6.52 (4.09 m, 1.11 m, 621); *Dsungaripterus weii* 10.7 (3.60 m, 1.41 m, 715); *Pteranodon longiceps* 18.6 (5.34 m, 1.91 m, 654); *Pterodactylus* sp. 0.708 (1.48 m, 0.715 m, 653); *Pterodaustro guinazui* 0.590 (1.23 m, 0.696 m, 714); *Q. northropi* as above; *Tupuxuara longicristatus* 22.8 (4.11 m, 1.92 m, 678).

**Table 3 typo (flagged, not corrected):** on p. 777 Table 3 prints *Preondactylus buffarinii* at 0.0750 kg, 0.0000988 m3, 759 kg/m3 and *Jeholopterus ninchengensis* at 0.0437 kg, 0.0000587 m3, 744 kg/m3, swapping the two Table 2 masses. I quote each table as printed. Table 1 spells the Jeholopterus species *ninchengensis*; the abstract has *ningchengensis*.

Bird model totals (Appendix 1; densities Table 3): wandering albatross 7.36 kg (796 kg/m3), great bustard 16.0 kg (824), Canada goose 5.99 kg (804), ostrich 80.7 kg (852), rock dove 0.268 kg (794), European starling 0.0834 kg (763).

### Predictions and thinner models for *Q. northropi* (pp. 781-783)

These are not the W4 table value; they are Henderson's own comparisons.

| Quantity | Value | Where |
|---|---|---|
| Body-length regression from the six other pterodactyloids | 1623 kg (Fig. 6C); 1079 kg or 198% above the 544 kg model | p. 781 |
| Wingspan regression from the six other pterodactyloids | 154 kg (Fig. 6D). To make the 544 kg model match 154 kg, average density would have to be 226 g/liter, "approximately 77% empty space" | p. 782 |
| Combined 'rhamphorhynchoid' + pterodactyloid regressions, *Q. northropi* excluded | 204 kg from body length; 179 kg from wingspan | p. 782 |
| Same wingspan regression with Lawson's 15.5 m | 354 kg, "still just 65% of the 'wire-frame' model" | p. 782 |
| Geometric scaling from *T. longicristatus* 22.8 kg, mass proportional to span cubed: 22.8 kg / (4.11 m)^3 = QuetzMass / (11.2 m)^3 | 461 kg | p. 782, eqn (3) |
| Same, mass proportional to body length cubed: 22.8 kg / (1.92 m)^3 = QuetzMass / (6.36 m)^3 | 829 kg | p. 782, eqn (4) |
| 64 kg (MacCready 1985, 11 m span) and 70 kg (Chatterjee and Templin 2004, 10.39 m) if forced onto the Fig. 3G shape | would require average densities of 85 g/liter and 88 g/liter, "more than 90% of the body was filled with air" | p. 781 |
| Trunk approximated as an elliptic cylinder: mean dorso-ventral radius 23.3 cm, mean width 20.8 cm, length 2.43 m | volume 0.370 m3; at 850 kg/m3, mass 315 kg. The model's own trunk mass is 407 kg (the cylinder is "22.6% less") | p. 782 |
| Neck and head at 300 kg/m3 | 27.7 kg and 19.2 kg. These plus the 315 kg cylinder give 361 kg, "just 23.8% below" the Table 2 axial mass of 474 kg | p. 782 |
| Fig. 11 thinner whole-body model: width reduced 20% and depth 25%, same densities, same length and span | total mass 268 kg | Fig. 11 caption, p. 783 |
| Extremely thin axial body (same 20% / 25% cuts; pectoral width 42 cm, maximum skull width 14 cm, minimum neck width 9 cm) | axial mass 198 kg, against the original 474 kg | p. 783 |

Henderson argues the 544 kg form is better read as secondarily flightless. That is his interpretation, not a measured number.

Log-log regressions (Q. northropi excluded from the pterosaur fits) as printed, with BM in kg and BL, WS in m: birds, BM = 10.5·BL^{2.81±0.101} and BM = 1.04·WS^{2.32±0.444}; all pterosaurs, BM = 2.00·BL^{2.53±0.285} and BM = 0.288·WS^{2.66±0.123} (p. 777). Split by group: BM_rhamphos = 0.667·BL^{1.84±0.297} and BM_pteros = 2.62·BL^{3.48±0.361} (p. 779); BM_rhamphos = 0.300·WS^{2.74±0.239} and BM_pteros = 0.315·WS^{2.56±0.355} ("to three significant figures", p. 779). Wing loading scaled as mass^{0.21} in the five volant bird models and as mass^{0.309} in the pterosaur models (p. 780).

## Q45. Published erosion steps and parameters (Y2, Y3, Y4)

### Thermal erosion: Musgrave, Kolb and Mace 1989 (Y4), section 4.2, p. 46

At each time step t+1, the altitude difference between vertex v and each neighbour u is compared with a global constant talus angle T (in the formula this is a height-difference threshold, not a number of degrees). If the slope exceeds T, a fixed percentage c_t of the excess is moved onto the neighbour:

- if a_v^t - a_u^t > T:  a_u^{t+1} = a_u^t + c_t (a_v^t - a_u^t - T)
- if a_v^t - a_u^t <= T: a_u^{t+1} = a_u^t

"With care taken to assure the equitable distribution of talus material to all neighboring vertices, the slope to the neighboring vertices asymptotically approaches the talus angle." **The paper does not give a numeric value for T in degrees.** Cordonnier et al. 2016 later call 30 degrees "the usual talus angle for thermal erosion [MKM89]"; that 30 degree figure is not in this PDF, so it is not used as a Musgrave number here.

Plate 9 (p. 49) example, also described on p. 45: 2000 time steps of hydraulic erosion and thermal weathering; c_t = 0.001; K_c = 5.0, K_d = 0.1, K_s = 0.3; "Rain" deposited on each vertex every 65 timesteps in an amount equal to 0.001 a_v^t (the body text on p. 45 says "approximately one one thousandth of the height of the vertex"). The run took "approximately 4 hours of CPU time on a Silicon Graphics Iris 4D/70 workstation." Plates 7 and 8 show thermal weathering alone on a non-uniform-lacunarity patch, described as a rough approximation of sand dunes (p. 46). No separate numeric T or c_t is given for plates 7-8.

### Hydraulic erosion in the same paper (Y4), section 4.1, pp. 45-46

Each vertex v at time t has altitude a_v^t, water volume w_v^t, and suspended sediment s_v^t. At each timestep, excess water and sediment are passed from v to each lower neighbour u. The water passed is

Δw = min( w_v^t , (w_v^t + a_v^t) - (w_u^t + a_u^t) ).

If Δw <= 0, a fraction of the sediment at v is deposited: a_v^{t+1} = a_v^t + K_d s_v^t and s_v^{t+1} = (1 - K_d) s_v^t.

Otherwise the water is moved (w_v^{t+1} = w_v^t - Δw, w_u^{t+1} = w_u^t + Δw) and the sediment capacity of that water is c_s = K_c Δw. Then:

- if s_v^t >= c_s: s_u^{t+1} = s_u^t + c_s; a_v^{t+1} = a_v^t + K_d (s_v^t - c_s); s_v^{t+1} = (1 - K_d)(s_v^t - c_s)
- otherwise: s_u^{t+1} = s_u^t + s_v^t + K_s (c_s - s_v^t); a_v^{t+1} = a_v^t - K_s (c_s - s_v^t); s_v^{t+1} = 0

K_c is the sediment capacity constant ("maximum amount of sediment which may be suspended in a unit of water"); K_s the soil softness constant (rate soil is converted to sediment); K_d the deposition constant (rate sediment settles and is added to altitude). In a 2-D implementation, water and sediment go to all lower neighbours "in amounts proportional to their respective differences in overall elevation." Example constants are those already quoted from Plate 9.

### Hydraulic erosion: Mei, Decaudin and Hu 2007 (Y2)

Five iterated steps (PDF p. 4, also summarised at the end of section 3):

1. Water increment: d1(x,y) = d_t(x,y) + Δt · r_t(x,y)  (eqn 1). r_t is water arriving per time unit from rainfall (random drop locations) or fixed river sources; the user sets location, radius and intensity.
2. Flow simulation with a virtual-pipe shallow-water model (eqns 2-9): outflow flux in each of four directions is accelerated by hydrostatic pressure, then scaled by K = min(1, d1·l_X l_Y / ((f^L+f^R+f^T+f^B)·Δt)) so a cell cannot lose more water than it holds (eqns 2-5). Water height is updated from net flux (eqns 6-7). Horizontal velocity follows from the flux (eqns 8-9). "No slip" boundaries: outflow through a domain edge is set to zero. Stability is limited by a CFL-like condition Δt·u <= l_X, Δt·v <= l_Y (PDF p. 6).
3. Erosion and deposition (eqns 10-12). Transport capacity C(x,y) = K_c · sin(α(x,y)) · |v(x,y)|, with α the local tilt angle. If C > s_t: b_{t+Δt} = b_t - K_s (C - s_t) and s1 = s_t + K_s (C - s_t). If C <= s_t: b_{t+Δt} = b_t + K_d (s_t - C) and s1 = s_t - K_d (s_t - C). K_s is a dissolving constant, K_d a deposition constant, K_c a sediment capacity constant. For very flat cells, α may be limited by a user-specified minimum threshold; no default for that threshold is given.
4. Sediment transport by semi-Lagrangian advection: s_{t+Δt}(x,y) = s1(x - u·Δt, y - v·Δt) (eqns 13-14).
5. Evaporation at constant temperature: d_{t+Δt}(x,y) = d2(x,y) · (1 - K_e · Δt) (eqn 15). K_e is an evaporation constant.

**The paper does not publish default numeric values for K_s, K_d, K_c, K_e, g, pipe area A, pipe length l, or rainfall intensity r_t.** Symbols only. What it does publish are the time steps and grid sizes used for the performance tests (Table 1, PDF p. 10; times in milliseconds; CPS = cycles per second, one cycle = one simulation + one visualisation; measured on the Fig. 7 mountain; Pentium IV 2.40 GHz, NVIDIA GeForce 8800 GTX):

| Grid | Δt | ST (ms) | VT (ms) | CT (ms) | CPS |
|---|---|---|---|---|---|
| 256 x 256 | 0.002 | 1.29 | 1.28 | 2.48 | 403 |
| 512 x 512 | 0.001 | 2.38 | 2.97 | 5.38 | 186 |
| 1024 x 1024 | 0.0005 | 6.65 | 10.31 | 16.95 | 59 |
| 2048 x 2048 | 0.00025 | 22.88 | 40.00 | 62.50 | 16 |
| 4096 x 4096 | 0.000125 | 91.22 | 155.89 | 248.47 | 4 |

Implementation: quantities packed into three 2-D textures; seven GPU fragment passes per iteration (PDF pp. 7-8). Related-work figure: Benes 2007 reported 5 fps for a 300 x 300 grid (PDF p. 4). No units are stated for Δt in Table 1 (the equations treat it as the simulation time step).

### Stream-power erosion: Cordonnier et al. 2016 (Y3)

Stream-power equation (eqn 1, PDF p. 4, citing Whipple and Tucker 1999):

dh(p)/dt = u(p) - k A(p)^m s(p)^n

with local slope s(p) = ∇h(p), A(p) the upstream drainage area, u(p) the uplift rate, and k an erosion constant. The paper uses n = 1 and m = 0.5, and notes that the ratio m/n is "thought of being m/n ≈ 0.5". Typical scales for the equation: "between 10^5 and 10^7 years and a few tens to hundreds of kilometers" (PDF p. 4).

Iteration, starting from a Poisson-sampled planar graph on domain Ω, all node heights h_k = 0, border nodes held at sea level as river mouths (PDF pp. 4-5):

1. Build directed stream trees: each node sends water to its lowest neighbour (the receiver).
2. Connect lakes by overflow (pass-height algorithm, complexity O(N + M log M), M = number of lakes) to get modified trees T-tilde.
3. Compute drainage area A_k = a_k + sum of children's A (a_k is the Voronoi cell area) and slope s(p_k) = (h_k - h_l) / ||p_k - p_l||, where l is the receiver.
4. Solve eqn 1 with an implicit scheme. For n = 1, eqn 2 (PDF p. 7) gives h_i(t+δt) from the already-updated receiver height, walking each tree from root to leaves, O(N).

Stop when |h_k(t+δt) - h_k(t)| is below a threshold (the numeric threshold is not given). Thermal-erosion correction after Musgrave et al. 1989: "if the result of the stream power equation leads to slopes higher than 30°, we reduce the change of elevation so as to keep slopes in the prescribed range" (PDF p. 7). The 30° figure is stated here as their usual maximum talus angle, attributed to MKM89.

**Default experiment parameters** ("unless stated differently", PDF p. 7):

| Quantity | Value | Where |
|---|---|---|
| Terrain size | 50 x 50 km2 | p. 7 |
| Maximum tectonic uplift U | 5.0 x 10^{-4} m y^{-1} ("the average uplift among earth mountains") | p. 7 |
| Erosion rate k | 5.61 x 10^{-7} y^{-1} ("for mountains to culminate at about 2 000 m") | p. 7 |
| Time step δt | 2.5 x 10^5 y | p. 7 |
| Convergence | "typically between 100-300 steps"; Table 1 footnote "around 200 time steps"; mountains "fully shaped after 50 iterations" and "geometry stops evolving after 100-300 iterations" (Fig. 12, PDF pp. 7-9) | pp. 7-9 |
| Hardware | Intel Core i7, 3 GHz, 16 GB RAM | p. 7 |

Fitted rule from their experiments (PDF p. 8): "the height in kilometers follows the rule h_max = 2.244 u/k". **Unit caution (computed, not a new measurement):** using the printed defaults, u/k = (5.0 x 10^{-4} m y^{-1}) / (5.61 x 10^{-7} y^{-1}) = 891.3 m, and 2.244 x 891.3 = 2000 m = 2 km. The printed sentence says "in kilometers"; the product 2.244 u/k equals 2 km only if u is taken in km y^{-1}, or equals 2000 m if u is in m y^{-1}. I do not choose between those readings.

Other published parameter choices: min and max thermal slope angles 6° and 54° when the slope is modulated (PDF p. 10); a 3-D Perlin-noise modulation of the maximum slope (Fig. 17); height-dependent maximum slope to produce cliffs (Figs 15, 18). Without lake connections the water eventually finds a way out "after 4-5 more iterations than the total convergence of the erosion with lake connection" (Fig. 16 is after 175 iterations) (PDF p. 10). Stream-power erosion without uplift "converges toward a flat terrain", so "small time steps" and "only a few iterations" are required (no numeric Δt given for that case).

Table 1 (PDF p. 8), simulation time vs sample count:

| # samples | One time step (s) | Convergence time (s) |
|---|---|---|
| 10 000 | 0.031 | 6.4 |
| 40 000 | 0.177 | 35.4 |
| 90 000 | 0.401 | 78.0 |
| 160 000 | 1.273 | 252.0 |

**Computed check:** 35.4/0.177 = 200.0; 252.0/1.273 = 198.0, consistent with "around 200 time steps."

Q45 is now answered for all three named methods (steps plus the parameters the papers actually print). Mei's hydraulic constants remain user-set; Musgrave's talus angle T is unnamed as a number of degrees in that paper.

## U3. Stick-insect leg segments, Cruse 1976 Table 3 (Y5)

Table 3, p. 243, "mean values of some linear measures and the total body weight of these stick insects. The values in brackets show the standard deviations. Number of measured animals: 20." The preceding paragraph (p. 242) says "These values are obtained from 40 different animals." Those two statements disagree; I quote both and use the table caption's n = 20 when citing the table. No tarsus lengths appear in Table 3.

| Quantity | Mean (SD) | Where |
|---|---|---|
| Total length of the body | 73.2 (4.6) mm | Table 3, p. 243 |
| Distance, front of head to coxa of prothorax | 7.1 (0.8) mm | Table 3 |
| to coxa of mesothorax | 24.6 (2.1) mm | Table 3 |
| to coxa of metathorax | 35.5 (2.8) mm | Table 3 |
| to centre of gravity of the whole animal | 35.8 (3.0) mm | Table 3 |
| Foreleg: coxa; trochanterofemur; tibia | 1.6 (0.2); 14.7 (1.3); 13.7 (1.4) mm | Table 3 |
| Middleleg: coxa; trochanterofemur; tibia | 1.5 (0.2); 11.4 (0.9); 10.7 (0.9) mm | Table 3 |
| Hindleg: coxa; trochanterofemur; tibia | 1.5 (0.2); 12.3 (1.5); 11.8 (1.7) mm | Table 3 |
| Total weight | 820 (160) mg | Table 3 |

**U3 confirmed:** the brief's front coxa 1.6 ± 0.2, trochanterofemur 14.7 ± 1.3, tibia 13.7 ± 1.4 mm and body 73.2 ± 4.6 mm match Table 3 exactly, for *Carausius morosus*. The table adds the middle and hind legs, the body landmarks, and 820 ± 160 mg.

Table 4, p. 243, distance from the base of the coxae to the walking surface (mm, mean (SD)):

| Segment | Horizontal path and horizontal plane | Hanging from the horizontal beam | Up the vertical path |
|---|---|---|---|
| Prothorax | 9.1 (1.4) | 19.0 (2.0) | 7.6 (2.3) |
| Mesothorax | 5.9 (0.9) | 19.0 (2.0) | 7.3 (2.1) |
| Metathorax | 4.3 (0.6) | 20.0 (2.5) | 7.8 (2.2) |

Related method numbers, not U3: the walking path is "30 mm broad, 40 cm long" with two 5 mm wide balsa beams (p. 243); force traces use "the unit of the abscissa is 1 s, and the unit of the ordinate is 1 mN (≈ 100 mp)" (p. 243); Table 2 (p. 241) gives anterior and posterior extreme tarsus positions (mm) with "the minimum sample size of any measurement was n = 63."

## Q17. Castillo-Figueroa 2020 supplement S1 (Y6)

Table S1 is titled: "Loadings matrix of wing traits calculated by MDS. Metacarpals and phalanges are scaled to size (i.e., wing structure /FL ratio). Acronyms of wing traits are found in figure 1." The file contains this one table and no other data. **There are no per-bone lengths in millimetres.** Q17's request for a millimetre metacarpal and phalanx table is not answered by this supplement (the article Table 1 still has only forearm and whole-digit lengths, as already reported in R7-2D and R7-4).

Loadings, copied at the file's printed precision:

| Wing trait | MDS1 | MDS2 |
|---|---|---|
| P2.1 | -0.06410555 | 0.06341994 |
| P3.1 | -0.06455927 | 0.03038426 |
| P3.2 | -0.01764726 | 0.09927796 |
| P3.3 | 0.09683515 | 0.02081298 |
| P3.4 | 0.21352353 | -0.11913816 |
| P4.1 | -0.04500416 | 0.01787048 |
| P4.2 | 0.00049 | 0.07648996 |
| P4.3 | 0.13479024 | -0.08246176 |
| P5.1 | 0.02375557 | -0.05771002 |
| P5.2 | -0.03329118 | -0.03506797 |
| P5.3 | 0.13915241 | -0.11366729 |

The 11 traits match the article's statement that 11 metacarpal and phalanx measurements were taken, but here they appear only as size-scaled MDS loadings.

## Self-check (every extracted number against the file)

- Henderson Table 1 spans and lengths, Table 2 masses including 544 kg, Table 3 densities including 724 kg/m3 and 0.751 m3, Table 4 wing loading 369 N/m2 and 5.34 x 10^3 N: checked on the rendered table pages (PDF pp. 4, 9, 10, 12 = journal pp. 771, 776, 777, 779). 544 x 9.81 = 5336.64 N, consistent with 5.34 x 10^3. Eqn (3) 22.8 x (11.2/4.11)^3 = 461.6 kg, consistent with 461 kg. Eqn (4) 22.8 x (6.36/1.92)^3 = 828.1 kg, consistent with 829 kg. 1623 - 544 = 1079, and 1079/544 = 1.983, consistent with "198%". Table 3 Preondactylus/Jeholopterus swap confirmed on the page image.
- Mei Table 1 times, Δt values and the five-step equation list: checked against pdftotext of PDF pp. 4-6 and 10. No default K_s, K_d, K_c, K_e found in the full text (search for numeric assignments returned none).
- Cordonnier defaults, eqn 1, m = 0.5, n = 1, 30° correction, Table 1, 6°/54°, 175 iterations, 4-5 extra iterations: checked against pdftotext of PDF pp. 4-10. 2.244 x (5.0e-4/5.61e-7) = 2000 (metres), as flagged above. Table 1 ratios match "around 200" steps.
- Musgrave hydraulic and thermal formulae and Plate 9 constants (K_c = 5.0, K_d = 0.1, K_s = 0.3, c_t = 0.001, rain every 65 steps at 0.001 a_v^t, 2000 steps, about 4 hours on a 4D/70): checked on rendered pp. 45-46 and the Plate 9 caption on p. 49. No numeric T in degrees in this PDF.
- Cruse Table 3 and Table 4: checked on the rendered journal p. 243. U3 front-leg and body-length figures match the brief exactly. n = 20 in the caption vs "40 different animals" in the p. 242 paragraph, both quoted.
- Castillo Table S1: checked by opening the docx (one table, 12 rows including the header, values as above). No millimetre column is present.

## What these files do not close

Q17 per-bone lengths in mm; Q46, Q47; the remaining blocked items listed in blocked-downloads-ALTERNATIVES.md (Wayback/Ford, old JEB scans, AMS, Wiley, RVC, Shine Dryad, figshare, and the unattempted remainder). Mei's hydraulic constants K_s, K_d, K_c, K_e have no published defaults. Musgrave's talus angle T has no published degree value in this PDF. Cruse Table 3 has no tarsus length.
