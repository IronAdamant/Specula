# Round seven, checked here: machines, people, plants, terrain, water and the primary-source rows (2026-11-24)

An independent check for the project, by a model that had no part in the research: the delivered
numbers held against the source files packed with them, and the engine searched for every value
the round corrects. The scripts and their outputs named below are in a scratch folder on the
development machine and are not in the repository; the packed sources are not either.

**How much of the delivery I opened**
- **Read in full:** README; R7-2G, 2H, 2I, 2J, 3, 3b, 34, 4, 4b, 5, 5b and 6; all twelve of their verify files; brief parts 2G-2J, 3 and 4; and F2.
- **Erosion papers:** all three checked page by page. Musgrave pp. 45, 46 and 49 were read as page images.
- **Machines:** the Unitree G1 URDF, motor table and documentation; the Go1 URDF; the ANYmal paper; the Corolla, Isuzu and Cat 320 sheets; the M113A2 manual pages (host pp. 26, 79, 93-96); the Raptor sheet; the GDLS M1A1 sheet; the KNDS page; six UR techsheets and three CB3 sheets.
- **Aircraft and vehicles:** the EASA ASW 22 TCDS, NASM, Golf, 911, Tesla (CN), New Flyer, the Virginia Code table, B737, R22, the FAA AC table and the EU 2020/740 tyre table.
- **People:**
  - Dempster Tables 10-12, read as page images.
  - The five R7-2H papers' tables.
  - All 24 ANSUR female rows, checked against the engine's own computation of the same survey.
- **Animals and dinosaurs (W rows only):** Hutchinson 2011, Benson 2014, González Riga 2016, Sellers 2013 and 2017, and Henderson 2010 (the 544 kg only).
- **Terrain, water, plants and materials:** Baker 1918, the refractiveindex.info files (I computed n myself), Perovich 1996, the SPM 1984 text, Kuuluvainen 1988 (scan page image and OCR), Peng 2025, Brock 2000, the EPA and NASA files, the TU Delft, CEC and UC Davis tyre files, the Meijaard benchmark (authorised Russian translation), and the BTO, Barnes and Shivers files.
- **Not opened:** the animal Q rows' sources (another checker has them).

---

## (a) Corrected rows already used in the engine: none

None of the old W or U values is in the engine or the sample library, and neither are the quantities they describe.

**How I searched (`engsearch.py`, output `eng_hits.txt`):**
- **Files:** every `.rs .json .toml .ron .txt .csv .lex .yaml` file under `C:/sf-una/crates`, `C:/sf-una/games`, `C:/sl-tpl/crates/specula-exemplaria` and `C:/sl-fx/crates/specula-effectus`.
- **Terms:** the names and distinctive numbers of every row group (UR arms, Atlas, rolling resistance and 0.0385, bicycle, the named cars, Abrams/Leopard, every aircraft, the power law and 1/7, albedo, refractive index and plastics, feathers, the U-row terms, Secchi, Manning, figure of merit, erosion and talus, Ackermann and suspension travel, spine and clavicle, wave fetch).
- **Result:** every hit was read and all were unrelated.
- **Also searched:** Sepulcra's assets for material values, and `C:/sl-tpl/docs/design/EXEMPLARIA.md`. EXEMPLARIA names D3 as secondary but holds none of its values.

Three engine points touch this delivery. None needs a change:
- **Segment masses** (`C:/sf-una/crates/specula-corpus/src/inertia.rs:45-56`) come from de Leva 1996, read on the printed page.
  - Dempster (R7-5, U25) does not correct them. It is a different cadaver series with different segment cuts (thigh 9.7 % against de Leva's 14.16 % for men). Keep de Leva.
  - U25's "de Leva still open" is out of date for the engine.
- **Thermal erosion** (`C:/sf-una/crates/specula-model/src/terrain.rs:527-555`):
  - The default talus is 35° (`src/recipe/procgen.rs:1060`); Sepulcra uses 33° (`games/sepulcra/assets/recipes/graveyard_ground.json:17`).
  - Musgrave prints no talus angle in degrees (confirmed on the page), so nothing in the engine rested on one.
  - The engine moves 0.25 of the excess to the steepest neighbour. Musgrave moves c_t of the excess to every lower neighbour.
- **Droplet erosion** (`terrain.rs:473`) mixes two sources: Lague's port values (inertia 0.05, capacity 4, erode and deposit 0.3, gravity 4) with Beyer's evaporation 0.02 (Lague's is 0.01). This is not a W/U row; it is a note for the erosion lane.

---

## (b) Errors found in the delivery

1. **M1A1 weight (R7-3b W56; README item 4).**
   - Delivered: 67.6 tons, with "63 tons looks like an earlier figure".
   - The same GDLS sheet prints "Power to Weight Ratio - 23.8 hp/ton" with 1,500 hp. That implies 63.0 tons (1,500 / 67.6 would be 22.2 hp/ton).
   - The maker sheet contradicts itself, so the page's 63 tons is not refuted by it.
   - File: `part5/.../work/w3b/m1a1.txt`.
2. **G1 thigh length (R7-2G Q32, computed).**
   - Delivered: hip-pitch to knee 0.332 m, leg 0.632 m.
   - The URDF pitches the hip-roll joint by −0.1749 rad and the knee by +0.1749 rad. With those rotations the knee sits 0.3366 m below the hip-pitch axis and 0.000 m ahead of it; the straight-line distance is 0.341 m. Hip pitch to ankle pitch is 0.637 m vertical.
   - The verify file's reason ("ignores the 0.078 m fore-aft offset") is also wrong: in the zero pose the fore-aft offsets cancel.
   - Files: `g1_29dof.urdf` lines 81-234; my working is in `g1leg.py`.
3. **G1 "arm span about 0.45 m" (R7-2G).** 0.45 m is the robot's standing width (1320 × 450 × 200 mm). It cannot be the arm span of a 1.32 m humanoid. Mark it as maker text and do not use it. File: `unitree_g1_doc.txt` lines 47 and 60.
4. **PET refractive index (README item 11).**
   - The README presents 1.569 as a correction of 1.575.
   - n = 1.5691 at 589.3 nm is right, but it comes from one dataset (Zhang 2020, one commercial sample).
   - PET's index varies with crystallinity and orientation, so this is a second value, not a correction. R7-3b itself only says "differs".
5. **Leopard 2A6 (README item 5; W57).** The KNDS block is headed "Performance characteristics of the Leopard 2 A6M", the mine-protected variant. The figures (65 t, 3.77-4.00 m, 2.64 m) are right for the A6M. They do not correct a 2A6 row.
6. **Other "corrections" that are really disagreements (README items 7 and 9).**
   - UR5e, UR10e and UR30 masses: confirmed on the "Updated October 2026" techsheets. The page's figures may simply be earlier sheet revisions, so read these as "current sheet differs".
   - T. rex: Hutchinson 2011 uses volumetric models, Erickson 2004 used limb-bone scaling. The two are different methods, not one correcting the other.
7. **Cordonnier unit caution (R7-5b).**
   - R7-5b calls h_max = 2.244 u/k ambiguous in its units. It is not.
   - With m = 0.5 and n = 1, k is per year, so u/k is a length and 2.244 is dimensionless: 2.244 × 0.891 km = 2,000 m.
8. **Dempster (R7-5, U25).**
   - The limb means quoted are the left-side means. The right side differs by 0.1: upper limb 4.9, arm 2.7, forearm and hand 2.2, forearm 1.6, thigh 9.6, leg and foot 5.9.
   - Head and neck, thorax, and abdomen plus pelvis are means of 7 cadavers, not 8.
   - Report pp. 186-188 (PDF pp. 209-211).
9. **SPM wave formulas (R7-5 Q49).**
   - R7-5 omits the caps printed with the same equations: the fetch formulas hold only up to the fully developed sea (eqs 3-36 to 3-38: gH/U_A² = 0.2433, gT/U_A = 8.134, gt/U_A = 7.15 × 10⁴). It also omits the duration-limited eq 3-35.
   - Without the caps, a long fetch gives waves taller than a fully developed sea.
   - The wind-stress conversion, eq 3-28a on p. 3-30, is illegible in the OCR. U_A = 0.71 U^1.23 is from my own knowledge; check it on the page.
10. **Scots pine branches per whorl (R7-2I Q42 says "not found").** Kuuluvainen 1988, already packed, gives 9.4 branches per whorl (s.d. 1.3, range 6.4 to 10.8) in Table 3, p. 152. The sample is 12 trees, 2.73 m tall, crown width 1.70 m.
11. **Peng 2025 (R7-2I verify says "UNVERIFIABLE").** The full paper is packed (`A5.txt`) and the RMSE values are in it. The verify verdict is out of date.
12. **SPM page image (R7-5 verify says "checked on the page image").** The SPM PDF is not in the pack, so that check cannot be repeated from the delivery.

Minor: the FAA helicopter table's text layer is shifted by one row, but R7-3 read the right rows. The FAA table gives the R22 rotor as 26 ft against the POH's 25 ft 2 in; the POH figure is the one to use.

---

## (c) The W and U table: counts

The full table is in the last section below.
- **Rows corrected and used in the engine:** none. None of the 82 W and 25 U values is in the engine.
- **W rows confirmed from a packed file:** about 45 (rows labelled "Yes", "Yes, but" or "Substitute yes" in the full table).
- **W rows not checkable** (source missing or packed as a block page):
  - W1-W3 (Andres & Langston: Cloudflare page)
  - W12 (Paul 2019: Incapsula page)
  - W18 (Longrich: Access Denied)
  - W29-W35 (Boston Dynamics: search extracts only)
  - W64 and W66 (af.mil: Access Denied)
  - the PH78 abstract for W70 (captcha page)
  - W72-W75 (LBL and Lagarde pages)
  - W81 (empty file)
- **U rows checked by me:** U16, U19, U21 (substitute), U23, U24 and U25 hold. U22 is not checkable. The animal U rows are left to the other checker.

---

## (d) The erosion papers

**Musgrave, Kolb and Mace 1989**
- **Hydraulic (p. 45).** At each time step, for each lower neighbour u of vertex v:
  - Water passed: Δw = min(w_v, (w_v + a_v) − (w_u + a_u)).
  - If Δw ≤ 0: deposit, a_v += K_d s_v, and s_v ← (1 − K_d) s_v.
  - Otherwise move the water and set the capacity c_s = K_c Δw. If s_v ≥ c_s: s_u += c_s, a_v += K_d(s_v − c_s), s_v ← (1 − K_d)(s_v − c_s). Otherwise: s_u += s_v + K_s(c_s − s_v), a_v −= K_s(c_s − s_v), s_v ← 0.
  - In 2-D, water and sediment go to the lower neighbours in proportion to their elevation differences.
  - Rain is about 1/1000 of the vertex height, every 60 to 100 steps (Plate 9: every 65 steps).
- **Thermal (p. 46).** If a_v − a_u > T, then a_u += c_t(a_v − a_u − T).
  - T is a height difference between neighbours, and no value of T appears anywhere in the paper.
  - Only u's update is written; v must lose the same amount.
- **Constants:** only Plate 9 (p. 49): K_c 5.0, K_d 0.1, K_s 0.3, c_t 0.001, 2,000 steps, about 4 h on an Iris 4D/70. No time step or stability condition is given.
- **R7-5b** is correct. It leaves out the 60-100-step rain interval.
- **F2** wrongly gives the "usual 30°" talus angle to Musgrave; it is Cordonnier's (and Šťava's for dry sand). F2's continuous form ∂h/∂t = −k(s − tan θ) is Galin's, not Musgrave's.

**Mei, Decaudin and Hu 2007** — five steps per iteration:
1. Water: d1 = d + Δt·r.
2. Flow through virtual pipes:
   - flux f_L = max(0, f_L + Δt·A·g·Δh_L / l), Δh measured on terrain plus water;
   - scaled by K = min(1, d1·l_X·l_Y / (Σf·Δt)) so a cell cannot lose more water than it holds;
   - net volume change ΔV, then d2 = d1 + ΔV / (l_X l_Y);
   - velocity u = W_X / (l_Y·d̄), with d̄ = (d1 + d2) / 2;
   - no outflow through the domain edge.
3. Erosion and deposition: capacity C = K_c·sin α·|v|, with α clamped to a user minimum on flat ground. If C > s, dissolve K_s(C − s); otherwise deposit K_d(s − C).
4. Sediment moved by semi-Lagrangian advection, which is unconditionally stable.
5. Evaporation: d ← d2(1 − K_e Δt).

- **Stability:** a CFL condition, Δt·u ≤ l_X and Δt·v ≤ l_Y.
- **Constants:** none printed for K_c, K_s, K_d, K_e, A, l or r. Table 1 gives only Δt: 0.002 at 256² halving to 0.000125 at 4096².
- R7-5b is correct and complete, and F2 agrees.

**Cordonnier et al. 2016**
- **Equation:** dh/dt = u − kA^m s^n, with n = 1 and m = 0.5.
- **Graph:** Poisson samples joined by a constrained Delaunay triangulation; Voronoi cell areas; border nodes held at sea level.
- **Loop:**
  1. Each node drains to its lowest neighbour (its receiver), forming stream trees.
  2. Lakes are connected by overflow through passes, in O(N + M log M).
  3. Drainage area and slope are computed from leaves to roots.
  4. The implicit solve (eq 2, which R7-5b does not write out):
     h_i(t+δt) = [h_i + δt(u_i + kA_i^m h_j(t+δt) / |p_i − p_j|)] / [1 + kA_i^m δt / |p_i − p_j|],
     walked from the roots outward, in O(N).
- **Stop:** when the change in elevation falls below a threshold (no value printed).
- **Slope correction:** limit slopes to 30°; 6° to 54° when the slope is modulated.
- **Defaults:** U 5.0×10⁻⁴ m/y, k 5.61×10⁻⁷ /y, δt 2.5×10⁵ y, a 50 × 50 km terrain, 100-300 steps (about 200). Only the ratio u/k matters, and h_max = 2.244 u/k.
- R7-5b's numbers are all correct. F2 agrees.

---

## (e) to (g): counts

- **(e) Numbers confirmed against the pack:**

  | File | About | Notes |
  |---|---|---|
  | R7-2G | 100 | The Cat sheet also gives 35.7 kPa ground pressure, which the delivery did not report |
  | R7-2H | 130 | The 24 ANSUR female rows are identical to `specula-corpus/src/ansur_data.rs` |
  | R7-3, 3b, 34 | 120 | |
  | R7-5 | 50 | |
  | R7-5b | 40 | |
  | R7-6 | 45 | |
- **(f) Not checkable here:**
  - The NHANES Q38 statistics: raw data are not packed; the table matches the worker's own output file.
  - All splib07a reflectances (the library is not packed).
  - The Go1 product page.
  - The W rows listed in (c), and U22.
- **(g) The verify files:**
  - I re-checked more than ten of their VERIFIED rows against the sources, and all held.
  - Every FIX they list was applied in the main files.
  - Where they fall short: G1 thigh and arm span, the M1A1 hp/ton line, the SPM page-image claim, the out-of-date Peng verdict, and an R7-5b verify that is only a summary.
  - In short, they are reliable on transcription and weak on what a number means (frames, units, variants, contradictions inside one source).

---

## (h) The builders' table

| Area | Good enough to type in now | Thin | Engine's own estimate (marked as its own), and what to anchor it to |
|---|---|---|---|
| Wheeled vehicles | R7-2G Q29: Corolla wheelbase, tracks and turning radius; Isuzu 46.5° inner cut and turning diameters. R7-3 W52-W55 dimensions and tracks. Raptor suspension travel 356 mm front, 381 mm rear | Maximum steering angles are only computed ideal-Ackermann estimates | Ackermann %; suspension travel for cars, SUVs and lorries (anchor: the Raptor as upper bound, plus ride frequencies) |
| Tracked vehicles | Cat 320 track figures; M113A2 manual figures | Tanks: M1A1 sheet contradicts itself; Leopard figures are for the A6M variant | Track pitch and wheel diameters (anchor: track length divided by shoe count, labelled computed) |
| Legged robots | G1 URDF limits (use these, not the spec table: same 310° span) and leg lengths of 0.337 m and 0.300 m; Go1 URDF; ANYmal | G1 torque has three figures: URDF effort is the simulator limit, 121 N m the motor rating, 90 (G1) / 120 (EDU) N m the product's maximum knee torque | Spot and Atlas joint ranges (anchor: human ranges, narrowed) |
| Arms | UR techsheets (W20-W28) | | |
| Aircraft | FAA and EASA type certificates (W60, W61, W63, W67), B737, NASM, rotor figures (W69) | C-130 and F-16: fact sheets not packed | Glide ratios for birds and dragons (anchor: Henderson's wing loadings and file C3) |
| People | ANSUR II via the corpus (already in the engine); spine curves (Hasegawa); clavicle; scapula angles and level; thigh and arm muscle volumes | Spine over 40; scapula by sex; stairs (one small study, suspect statistics); NHANES bands for age (recompute from the raw data first) | Under-bust and armpit girths; head and trunk shares by age |
| Segment masses | de Leva, already in the corpus | Dempster, only as a cross-check | |
| Trees | Young Scots pine branch angle by whorl (57.0, 74.2, 89.6, 93.5, 81.9°, SD 25 to 10) and 9.4 branches per whorl; Douglas-fir internode 0.86 m | Young 2.7 m trees only | Every other species' branch angles; length and radius ratios per order (anchor: E1's pipe model) |
| Terrain and erosion | Everything in (d) | Talus angle in degrees: 30° is the graphics convention; F1's field value is 35-45° | Mei's K constants (anchor: Šťava and Olsen, as in F2) |
| Materials and light | Perovich sea-ice albedos; NASA TP 2643 desert sand spectra; the plastics' refractive indices (one sample each) | splib07a values (check before using); asphalt from one LBL page | Charcoal, concrete, soil and grass albedos |
| Water and waves | SPM eqs 3-33 and 3-34 together with their caps, eqs 3-36 to 3-38 | The U_A conversion (check it on the page) | Jerlov Kd, Secchi, waterfall path, eddy size, shore slopes (anchor: files G1-G3) |

---

## (i) What I did not get to

- The animal rows, apart from U16.
- Simons and Richardson PP 422-J, Spicer 2000 and Lowell 2014: packed but not opened.
- Not every number of W52-W65.
- The Baker 1918 page image: the OCR mapping rests on row order.
- The researcher's other working folders.

---

## The W and U rows in full

"Pack" says whether I could confirm the round-seven outcome from a packed file. Not one old value is in the engine.

**W rows**
- **W1-W3** (C10, Quetzalcoatlus span 5.2-25.8 / 10-11 m, Greenewalt 30-440 kg, 200-250 kg): confirmed or partly. Not in the pack (Andres is a Cloudflare page).
- **W4** (544 kg): confirmed. Pack: yes (Henderson Table 2: 544 kg, 0.751 m³, 724 kg/m³).
- **W5-W8:** not found or citation only.
- **W9:** not needed (Sellers 2013's 83,230 kg is packed).
- **W10** (90 t): confirmed. Pack: yes (Benson 2014: 90,000 kg, interval 67.4-124 t).
- **W11** (96.4 t): not in the main text. Pack: yes (60.4 t for *Notocolossus*).
- **W12** (65-75 t): partly. Not in the pack.
- **W13:** substitute (Sellers 2013, 83,230 kg). Pack: yes.
- **W14, W16, W17:** Hutchinson 2011 disagrees with Erickson. Pack: yes.
- **W15:** not found.
- **W18** (hatchling): confirmed by the delivery. Not in the pack (Access Denied).
- **W19:** substitute (Sellers 2017, 7.7 m/s). Pack: yes.
- **W20-W28** (UR arms): confirmed. W22 corrected to Ø151 mm, 20.7 kg; W23 to 33.3 kg; W26 to 63.5 kg. Pack: yes.
- **W29-W35** (Boston Dynamics): W30 "corrected" to 5 ft 9 in and 180 lb. Not in the pack (search extracts only).
- **W36:** partly (μ 0.70 computed from the statute table). Pack: yes (Virginia Code).
- **W37-W39, W44, W46, W47, W49, W51:** not found.
- **W40:** substitute (road Crr 0.0016-0.0032). Pack: yes.
- **W41:** partly (EU tyre classes only). Pack: yes.
- **W42** (EU class A 0.0065): confirmed. Pack: yes.
- **W43:** substitutes (CEC, UC Davis). Pack: yes.
- **W45:** label corrected (Morin's stage coach at a walk on muddy broken-stone road, 77 and 146 lb/ton). Pack: yes (Baker 1918 OCR).
- **W48** (45.6°): confirmed by arithmetic.
- **W50** (benchmark bicycle): corrected to 4.2924 and 6.0243 m/s. Pack: yes.
- **W52-W55** (cars, bus): confirmed or partly. Pack: yes.
- **W56** (M1A1): corrected. Pack: yes, but see error 1.
- **W57** (Leopard): corrected. Pack: yes, but the figures are the A6M's (error 5).
- **W58** (ground pressure): partly (M1A1 95.1 kPa). Pack: yes.
- **W59-W61, W63, W65, W67, W69:** confirmed, W67 corrected (850 kg belongs to the BLE). Pack: yes.
- **W62, W68:** not found.
- **W64, W66** (C-130, F-16C): partly or disagree. Not in the pack (af.mil Access Denied).
- **W70:** partly, with the EPA and NASA substitutes. Pack: yes for the substitutes; the Peterson and Hennessey abstract is not packed.
- **W71, W74:** not found.
- **W72, W73** (asphalt): partly. Not in the pack (LBL page).
- **W75:** partly, plus Perovich. Pack: Perovich yes.
- **W76** (plastics): confirmed, with PET differing. Pack: yes.
- **W77-W79** (feathers): partly. Pack: yes (BTO).
- **W80:** mostly not found.
- **W81:** partly. Not in the pack (empty file).
- **W82:** settled; not re-checked.

**U rows**
- **U1-U15, U17, U18:** animal rows, not checked by me. Of these, U1 was corrected; U3, U4, U5 and U6 confirmed; U8 partly; U2, U7, U9-U15 and U17 not found; U18 partly.
- **U16** (Castillo digit minima): corrected to *E. furinalis*. Pack: yes.
- **U19** (beech, coconut, baobab): partly. Pack: yes (Peng 2025 does not contain 30.7° or 54.1°).
- **U20:** not found.
- **U21** (ground albedo): substitute (NASA TP 2643). Pack: yes.
- **U22** (Secchi record): attribution corrected to Gieskes et al. 1987. Not in the pack.
- **U23** (Manning n): partly. Pack: yes (Barnes, n = 0.024 to 0.075).
- **U24** (figure of merit 0.75): confirmed. Pack: yes (Shivers: 0.75-0.80, falling to 0.715 and 0.705).
- **U25** (proportions by age): partly. Pack: yes (Dempster, see error 8). The engine uses de Leva.
