# Verification: D1-machine-jointing.md

Checked: 17 November 2026, Australia/Sydney (AEDT, UTC+11).
Source file: `/workspace/specula-research/D1-machine-jointing.md`
Output path (this file only): `/workspace/specula-research/D1-machine-jointing.md.verify.md`

Private adversarial source-check. No number was taken from Grok alone. Every figure below was opened in an official manufacturer page or PDF, an author or institutional copy, a peer-reviewed open-access paper, Crossref/OpenAlex/Semantic Scholar official metadata, or a clearly labelled manufacturer document on a dealer host (same standard D1 used for Cat and Liebherr). D1 itself was not edited.

## Summary

- **53 data rows** checked (sections 1–7).
- **46 VERIFIED**, including five rows D1 had marked Unverified that I could upgrade from opened sources: **17** (KUKA 2016 brochure), **18** (official FR3 datasheet R02212 rel. 2.5, June 2026), **32** (Go1 ranges are on the cited product page), **35** (official `unitree_ros` H1 URDF; the cited support.unitree.com page still returns 567), **51** (Liebherr TD 206.01.US10.2012, dealer-hosted).
- **2 FIXES**, both from a stale franka.cn mirror of the FR3 FCI tables:
  1. **Row 12 — FR3 joint position limits.** Official current FCI (`https://frankarobotics.github.io/docs/robot_specifications.html`, and `control-parameters-joint-fr3.csv`) is **q_max 2.9007, 1.8361, 2.9007, −0.1169, 2.8763, 4.6216, 3.0508 rad** and **q_min −2.9007, −1.8361, −2.9007, −3.0770, −2.8763, 0.4398, −3.0508 rad** (≈ ±166.2°, ±105.2°, ±166.2°, −176.3° to −6.7°, ±164.8°, 25.2°–264.8°, ±174.8°). D1’s ±2.7437, ±1.7837, … values match only the Chinese mirror. Speeds (2.62 / 5.26 / 4.18 / 5.26 rad/s), torques (87 and 12 N·m) and 10 rad/s² acceleration already match the official table.
  2. **Row 14 — FR3 recommended position–velocity rectangle.** Official FCI: **q_max 2.3476, 1.5454, 2.4937, −0.4226, 2.5100, 4.2841, 2.7045 rad**; **q_min −2.3476, −1.5454, −2.4937, −2.7714, −2.5100, 0.7773, −2.7045 rad**. Speeds 2, 1, 1.5, 1.25, 3, 1.5, 3 rad/s already match. Source: same official FCI page.
- **5 UNVERIFIABLE**, all already marked Unverified in D1: rows **23** (partial: 6 and 9.33 gear ratios confirmed in `MiniCheetah.h`; “single-stage planetary” and continuous 6.9 N·m / actuator mass unread — Katz thesis PDF returned 429), **24** (same thesis unread), **37** (Parmiggiani 2012 Table 1 still paywalled; no author PDF opened; the garbled “waist roll −90 to −90” warning stands), **48** (partial: official Cat PEDJ0819-01 gives a 7.5″ / 190.5 mm pitch *size class* for 315–326; the cited UC08-12 bulletin was not opened), **52** (Reimpell 2001 not read; legal preview gives the TOC only).
- **Rows skipped:** none of the 53.
- **Dead links:** none of the 31 citation URLs is a known-dead identifier. The truncated iCub URL without `+(1).pdf` is a parser artefact; the full URL in the file returns 200. Scripted fetches were blocked or empty on IEEE (202/500), World Scientific (403), hrcak (500), GitHub HTML (429; raw YAML 200), support.unitree.com (567), and MIT DSpace PDF download (429). ETH’s handle page is a JS shell; the bitstream PDF returns 200. Legal copies were opened for every row D1 marked Verified, except that FR3 limits should be taken from the official FCI site, not franka.cn.
- **Worst problems:** (1) rows 12 and 14 quote stale FR3 FCI limits from franka.cn; official current FCI agrees, in rounded degrees, with the official datasheet and with D1’s Unverified row 18, so D1’s “Sources disagree #1” (prefer the stale FCI over the datasheet) is out of date; (2) a later official UR article (Updated 4 July 2025) lists different joint-size torques than the 2015 CB3/e-Series article cited in row 9 — the 2015 numbers are correctly read, but should not be treated as the only current official maxima.

One-line summary: D1 is solid on UR, iiwa ranges/speeds, Mini Cheetah / ANYmal / Solo / Unitree product-page figures, the 2017 Cat 320 dealer sheet, Wu 2025, LTM 1100-4.2, and the two computed rows; replace the franka.cn FR3 position limits with the official FCI tables; treat rows 17, 18, 32, 35 and 51 as verified; leave the thesis, Parmiggiani table, UC08-12 bulletin and Reimpell book unread.

## File snapshot

At the start of this check (11:43:59 AEDT) D1 was copied to `/workspace/lv-scratch/d1/` (not `/tmp`). Snapshot: **35,503 bytes**, sha256 `7caf17e8bc4472719c38e6337381dee852bfb0897b80b8578aa0b27fe47d1560`, 199 lines (53 data rows), mtime 17 November 2026, **11:43:17 AEDT**. Size, time and the 43 Verified / 10 Unverified count all match Creature Data’s report.

Immediately before this verify file was written (11:52:09 AEDT), `sha256sum` and `cmp` of the workspace file against that snapshot were identical. **No content change; no row re-check was required for a moved file.** End snapshot: **35,503 bytes**, sha256 `7caf17e8bc4472719c38e6337381dee852bfb0897b80b8578aa0b27fe47d1560`.

## Close checks

### 1. Official Franka FCI vs franka.cn (rows 10–14, 18)

Official current docs: `https://frankarobotics.github.io/docs/robot_specifications.html` (repo `frankarobotics/docs`, Apache-2.0 LICENSE opened; page footer © 2023 Franka Robotics GmbH). The old `frankaemika.github.io` path is not required. D1 used `https://franka.cn/FCI/control_parameters.html`, which still resolves (200) but is a stale Chinese-language mirror.

**Panda / FER (rows 10–11):** official FCI joint table and `franka_ros` `joint_limits.yaml` (raw GitHub, 200; Apache-2.0 LICENSE opened) match D1 exactly: q ±2.8973, ±1.7628, ±2.8973, −3.0718 to −0.0698, ±2.8973, −0.0175 to 3.7525, ±2.8973 rad; speed 2.1750 / 2.6100 rad/s; torque 87 / 12 N·m; acceleration 15, 7.5, 10, 12.5, 15, 20, 20 rad/s²; jerk 7500, 3750, 5000, 6250, 7500, 10000, 10000 rad/s³; torque rate 1000 N·m/s. Degree conversions (±166°, ±101°, −176° to −4°, −1° to 215°) are correct. **VERIFIED.** Prefer the official FCI URL.

**FR3 DH (row 13):** official FCI modified-DH / Craig table is identical to D1 (J1 d 0.333; J2 α −π/2; J3 d 0.316, α π/2; J4 a 0.0825, α π/2; J5 a −0.0825, d 0.384, α −π/2; J6 α π/2; J7 a 0.088, α π/2; flange d 0.107 m). **VERIFIED.**

**FR3 limits (rows 12, 14):** **FIX** as in the summary. Official values also sit in `https://raw.githubusercontent.com/frankarobotics/docs/master/source/control-parameters-joint-fr3.csv`.

**FR3 datasheet (row 18):** official PDF `https://franka.de/hubfs/Datasheet%20Franka%20Research%203_R02212_2.5_EN_June%202026.pdf` opened. Document number R02212, release **2.5 (June 2026)**, © Franka Robotics. Joint ranges A1 −166/166, A2 −105/105, A3 −166/166, A4 −176/−7, A5 −165/165, A6 25/265, A7 −175/175 deg; speed A1–A4 150 deg/s, A5–A7 301 deg/s **(FCI use, A6: 239 deg/s)**; torque ±87 / ±12 N·m. These rounded degrees match official FCI (2.62 rad/s = 150.1 °/s; 5.26 = 301.4 °/s; 4.18 = 239.5 °/s). **Upgrade to VERIFIED** from this official sheet (D1’s unread “R02212 rel. 1.2 (2024/04)” was not opened; 2.5 carries the same range table plus the A6 FCI note). D1 “Sources disagree #1” should be rewritten: the official FCI and the official datasheet now agree.

### 2. UR official datasheets and the two torque articles (rows 4–9)

October 2026 official PDFs opened (© 2026 Universal Robots A/S). UR5e: 6 revolute joints, all ±360° and ±180°/s, payload 5 kg, reach 850 mm, mass including cable **20,7 kg = 20.7 kg**, footprint Ø151 mm, TCP 4 m/s, repeatability ±0.03 mm, drawing tolerance 0.1 mm / 0.5°. **Row 4 VERIFIED.** UR10e: all ±360°; base/shoulder ±120°/s; elbow and wrists ±180°/s; payload 12.5 kg; reach 1300 mm; mass including cable 33.3 kg; footprint Ø190 mm. **Row 5 VERIFIED.** (UR10e also lists TCP 4 m/s and repeatability ±0.05 mm, which D1 omitted — not a wrong number.)

DH article opened. UR5e/UR7e: d₁ 0.1625; a₂ −0.425; a₃ −0.3922; d₄ 0.1333; d₅ 0.0997; d₆ 0.0996 m; α = π/2, 0, 0, π/2, −π/2, 0. UR10e/UR12e: d₁ 0.1807; a₂ −0.6127; a₃ −0.57155; d₄ 0.17415; d₅ 0.11985; d₆ 0.11655 m. Link masses 3.761, 8.058, 2.846, 1.37, 1.3, 0.365 kg; “inertia matrix values for UR3e and UR5e are negligible”. **Rows 6–8 VERIFIED.**

Row 9’s 9 / 28 / 54 / 150 / 330 N·m, size layout, and “you cannot use the full value in your application” all match the cited 2015 article `…/max-joint-torques-cb3-and-e-series/` (still 200). **VERIFIED against that URL.** A later official article, `…/max-joint-torques-ur-series/` (**Updated on July 4th, 2025**), opened here, lists Size 2 **70 Nm**, 2.5 **107**, 3 **204**, 4 **433**, 5 **738**, and has no Size 0 or 1. Add a “Sources disagree” note and do not treat the 2015 table as the only current official maxima.

### 3. KUKA iiwa 7 (rows 15–17)

University-hosted *Spez LBR iiwa V5*, issued 28.01.2015, © 2015 KUKA Roboter GmbH, opened. p. 13: mass approx. 23.9 kg, rated payload 7 kg, reach 800 mm. Range of motion A1 ±170°, A2 ±120°, A3 ±170°, A4 ±120°, A5 ±170°, A6 ±120°, A7 ±175°. Speed with rated payload 98, 98, 100, 130, 140, 180, 180 °/s. “In-line wrist” is in the basic-data table; “alternating roll–pitch” is a fair description, not a quoted phrase. The 156 N·m figure is a **foundation load / torque about axis 1 for the iiwa 14 R820**, as D1 said. **Rows 15–16 VERIFIED.**

Row 17: official KUKA-authored brochure *Sensitive robotics LBR iiwa*, © 2016 KUKA Roboter GmbH, opened from a dealer host (`https://www.ferob.fi/wp-content/uploads/KUKA_SensitiveRobotics_LBR_iiwa_PB_en.pdf`; metadata 18 April 2016). For the 7 kg arm: A1–A2 **176 Nm**, A3–A5 **110 Nm**, A6–A7 **40 Nm**, with the same ranges and speeds as the spec. **Upgrade to VERIFIED** (dealer-hosted KUKA brochure; a currently hosted kuka.com URL for this full torque table was not found).

### 4. Second-hand Cat 320 and official later sheets (rows 38–42, 48)

Peterson-hosted PDF prints **AEXQ2214 (09-2017), Build Number 07A (Europe), © 2017 Caterpillar**. I could not find this exact sheet on cat.com / Scene7. Numbers in the dealer copy match D1:

- Reach Boom (5.7 m); R2.9B1 / R2.5B1 working range 6720 / 6300, 9860 / 9470, 9370 / 9170, 6490 / 6290, 5190 / 4800 mm; bucket tip radius 1570 mm.
- Cylinders: boom 120 / 1260, stick 140 / 1504, bucket 120 / 1104 mm.
- Swing 11.3 rpm, 74 kN·m; operating weight **21 600 kg** (Reach boom, R2.9, GD 1.19 m³, 600 mm shoes, 4.2 mt counterweight); net power 121 kW; travel 5.7 km/h.
- Bucket 150 kN ISO / 134 kN SAE; stick 106 / 103 kN (R2.9) or 118 / 114 kN (R2.5).
- Track length to centre of rollers 3650 mm; gauge 2380 mm; 49 shoes; 8 track rollers; tail swing 2833 mm; ground clearance 468 mm; counterweight clearance 1048 mm.

**Rows 38–41 VERIFIED** against that dated sheet. **Row 42 VERIFIED**, with a minor omission: the same sheet also lists optional **900 mm** shoes, not only 600 / 700 / 790 mm.

Official later Caterpillar PDFs opened from Scene7: **AEXQ3761-02 (12-2025)** and **AEXQ3764-01 (12-2025)**, Build 07H, © 2025 Caterpillar. Cylinders are unchanged. Swing is **11.25 rpm / 82 kN·m**. AEXQ3761-02 lists operating weight **21 800 kg**. The public cat.com 320 T4/Stage V page lists **22 600 kg** and ISO bucket / stick forces **141 kN / 107 kN**. D1’s “Sources disagree #4” (quote the configuration with the weight) is correct; Grok’s claim that 21 600 kg is “not found” on AEXQ2214 is wrong — it is on the first Weights block of that PDF.

**Row 48:** UC08-12 was not opened (no official URL found; third-party manual sites were not used). Official Cat brochure **PEDJ0819-01**, © 2022 Caterpillar, `https://s7d2.scene7.com/is/content/Caterpillar/CM20220301-1febf-b7596`, states a **7.5″ (190.5 mm) pitch size class (315–326)**. That supports the pitch *class* for a 320-class machine, not the unread bulletin’s wording about “older 320/320B”. **UNVERIFIABLE (partial).**

### 5. Liebherr LTM 1100-4.2 and 5.2 (rows 49–51)

Tiong Woon distributor PDF opens and prints **TD 187.02.DEFISR12.2012**, Liebherr-Werk Ehingen. No current official liebherr.com host for this model was found (the 4-axle line is now LTM 1120-4.2). Numbers in the distributor copy match D1: slewing 360° at **0 – 1,7** rpm; luffing **approx. 60 seconds to reach 82° boom angle**; telescoping **approx. 400 seconds for boom extension from 11.5 m – 60 m**; hoist **0 – 115 m/min** on a single line, **19 mm / 250 m**, **77 kN**; 1 base + 6 telescopic sections, each independently extendable; **17.2 t** basic counterweight; **7 m – 14 m** lattice section; luffing gear **1 differential ram** (Differentialzylinder); all 4 axles steered; all axles hydro-pneumatically suspended. **Rows 49–50 VERIFIED** against that dated Liebherr document on a distributor host.

**Row 51:** dealer-hosted Liebherr document **TD 206.01.US10.2012** opened (`https://nchcranehire.ie/wp-content/uploads/2025/02/LTM-1100-5-2-TD.pdf`; footer Liebherr-Werk Ehingen GmbH, www.liebherr.com). This is a US-unit edition, not Grok’s “lwe-td-206-00-defisr10-2017”. It confirms: 1 base + 5 telescopic sections; boom length **38 ft – 171 ft** (= 11.58–52.12 m, i.e. 11.5–52 m); **0 - 2 rpm**; **approx. 40 seconds to reach 82°**; **approx. 360 seconds** for 38–171 ft. **Upgrade to VERIFIED** from this Liebherr TD, with the document-code and units caveat. Do not mix 4.2 and 5.2, as D1 already said.

### 6. Preprints vs published copies, and the ANYmal licence (rows 19–28)

**Kim 2019** author PDF / arXiv 1909.06586v1 opened. 40 rad/s and 17 N·m, 3.7 m/s (Froude 4.65), 4 m/s then a fall, hip 34 rad/s and 25.5 N·m (commanded, and the paper notes it can exceed the 17 N·m cap), kp = 3 N·m/rad, kd = 0.3 N·m·s/rad, abduction kd = 1, all match. The 40 rad/s / 17 N·m sentence does not itself cite Katz; 2.45 m/s and 250 W are cited as [9] (Katz ICRA 2019). **Rows 20–21 VERIFIED (preprint).**

**Kim 2020** author PDF opened. 9 kg, 0.3 m tall, upper and lower links **0.21 m and 0.21 m**, 0.38 m between front and hind hips, 25 cm ground clearance, abduction / hip / knee actuators, Fig. 2 — match **row 19**. The phrase “12 actuators” is the 4×3 layout, not a quoted numeral. **VERIFIED (author copy).** Lab CAD header `MiniCheetah.h` uses hip 0.209 m and knee 0.195 m (commented alternatives 0.175 and 0.20) — a small disagreement with the paper’s 0.21 / 0.21, not an error in the cited paper.

**Katz ICRA 2019 abstract** confirmed from Semantic Scholar and OpenAlex (IEEE landing returned 202): custom backdrivable modular actuators; about 0.3 m and 9 kg; up to 2.45 m/s with cMPC; 360° backflips. **Row 22 VERIFIED (abstract).** Full IEEE PDF not opened.

**ANYmal:** ETH accepted PDF and metadata opened. Licence **In Copyright - Non-Commercial Use Permitted** (`http://rightsstatements.org/page/InC-NC/1.0/`). ⚠NC is **correct**. Layout, ~250 mm, slightly less than 30 kg, 0.5 m, ~650 Wh / 3 kg, <280 W, 48 V, 12 rad/s, 40 N·m, >70 Hz, 0.025°, 0.08 N·m, series-elastic + harmonic drive, joints can be fully rotated — all match. **Rows 25–26 VERIFIED (author copy).** IEEE landing returned 500; published pagination 38–44 was not re-read in the IEEE PDF.

**Solo:** arXiv 1910.00093v2 opened. 8 identical modules, 2-DoF sagittal legs, 2.2 kg, ~24 cm standing / 34 cm max hip, 42 × 33 cm, jump 65 cm from 24 cm standing (Fig. 1), 160 mm, 150 g, N = 9, kᵢ = 0.025 N·m/A, τ = 0.225·i, τ_max 2.7 N·m at 12 A, 10 kHz; “hardware blueprints and software … BSD-3-clause”. Published IEEE RA-L DOI **10.1109/LRA.2020.2976639**, 5(2):3650–3657 (April 2020) confirmed in Crossref; IEEE PDF not opened. **Rows 27–28 VERIFIED (preprint).**

### 7. Unitree pages and H1 limits (rows 29–35)

Go2 product page: body −48~48°, thigh −200°~90°, shank −156°~−48°, 12 motors; about 45 N·m; weight with battery about 15 kg; 0–3.7 m/s (MAX ~5 m/s) on the upper variants. **Rows 29–30 VERIFIED.**

Go1 product page (`/mobile/go1`): C1-8 520 g, 23.70 N·m; knee C1-8 × 1.5, 35.55 N·m; mass 12 kg; folded 0.588 × 0.22 × 0.29 m. **Row 31 VERIFIED.** The same page also has **body −49~49°, thigh −39~257°, shank −161~−51°** under “Large joint movement space”. D1 marked row 32 Unverified as “not on the page text I fetched”; those numbers are on the cited page (and match official `go1.urdf` to the nearest degree: ±0.863, −0.686–4.501, −2.818–−0.888 rad). **Upgrade row 32 to VERIFIED.**

H1 / H1-2 product page: layout, 400 mm thigh and calf, 338 / 685 mm arms, (1520+285)×570×220 mm, about 47 / 70 kg, 27 DoF on H1-2, torques ~360 / 220 / 59 / 75 and H1-2 waist ~220, ankle 75×2, shoulder ~120, elbow ~120, wrist ~30 N·m, H1 speed 3.3 m/s — all match. **Rows 33–34 VERIFIED.**

**Row 35:** support.unitree.com returned **567** (including `/H1_developer/About_H1`). Official `https://raw.githubusercontent.com/unitreerobotics/unitree_ros/master/robots/h1_description/urdf/h1.urdf` opened and matches Grok’s unread-docs figures exactly: hip yaw/roll ±0.43, hip pitch −3.14 to 2.53, knee −0.26 to 2.05, ankle −0.87 to 0.52, torso ±2.35 rad. **Upgrade to VERIFIED** from the official URDF, with the docs URL still blocked.

### 8. iCub (rows 36–37)

IIT PDF opened. Cover: **Revision 2.5**, date 05/08/2019 (change log 2.5 dated 05/08/2018). The filename says Rev_2.3; D1’s row correctly calls it Rev 2.5. DoF table and 53 total, ~105 cm, 36.5 cm, 30 kg (33 kg with battery), comparable to a four-year-old, all match. **Row 36 VERIFIED.**

**Row 37:** DOI resolves; World Scientific landing 403. No author PDF opened. OpenAlex abstract has no range table. The warning about “waist roll −90 to −90” is kept. **UNVERIFIABLE.**

### 9. Wu 2025 and computed rows 47 and 53 (rows 43–47, 53)

PLOS / PMC12637966 opened. **CC BY 4.0** confirmed. Eqs. 7–8 give ∠DOE = arccos[(DO²+EO²−L₁²)/(2·DO·EO)] and θ₁ = ∠DOE + ∠EOF − ∠DOPₓ (plus the stick/bucket loops through eq. 15). Table 2: boom 560–780 (stroke 220), stick 822–1342 (520), bucket 520–780 (260). Table 3: OA 1199.8, BA/AB 661.1, CB/BC 634.9, and the listed pin distances all match. Joint ranges θ₁ −13.02° to 37.13°, θ₂ 16.95° to 126.42°, θ₃ −34.14° to 55.45°. **Rows 43–46 VERIFIED.**

**Row 47:** L = √(r_A² + r_B² − 2 r_A r_B cos θ); dL/dθ = r_A r_B sin θ / L. Recomputed: d/dθ of the square root is (1/2)L⁻¹ · (2 r_A r_B sin θ) = r_A r_B sin θ / L. The arm is largest near 90° and zero when the cylinder lines up (sin θ = 0). **VERIFIED (computed).**

**Row 53:** 0.425/0.3922 = 1.0836 → **1.08**; 0.6127/0.57155 = 1.0720 → **1.07**; 0.21/0.21 = 1.00; 250/250 = 1.0; 400/400 = 1.00; 5.7/2.9 = 1.9655 → **1.97**; 1199.8/661.1 = 1.8149 → **1.81**. **VERIFIED (computed).**

### 10. Modern Robotics (rows 1–3)

Authors’ May 2017 preprint opened (`https://hades.mech.northwestern.edu/images/7/7f/MR.pdf`). Front matter: personal use only, not for further distribution; cite Cambridge 2017. ⚠ARR (free preprint for personal use) is fair; the “no further distribution” term is stricter than a bare ARR flag. Table 2.1 is on **p. 17** and matches row 1 (planar c is N/A, not “–”, for H, C, U, S). Eq. 2.4 is dof = m(N − 1 − J) + Σ fᵢ, exact only if constraints are independent, otherwise a lower bound. **Rows 1–2 VERIFIED (preprint).**

§2.2.1: “Every joint connects exactly two links”; a U joint is “a pair of revolute joints arranged so that their joint axes are orthogonal.” The companion video page (200) says a spherical joint has “the two degrees of freedom of the universal joint plus spinning about the axis.” **Row 3 VERIFIED** (book + companion page, not merely a search snippet).

### 11. Unverified rows still unread (23–24, 37, 48, 52)

Covered above. Reimpell Google Books landing returns 200; no legal preview of the §3.5–3.10 numbers was obtained. Katz thesis handle resolves; PDF download 429.

## Row table

| Row | Claim | Status | Note | URL |
|---|---|---|---|---|
| 1 | Joint freedoms/constraints, Table 2.1 p. 17 | VERIFIED | May 2017 preprint; planar c is N/A for H/C/U/S | https://hades.mech.northwestern.edu/images/7/7f/MR.pdf |
| 2 | Grübler’s formula Eq. 2.4 + independence / lower bound | VERIFIED | Exact wording on p. 17–18 | same |
| 3 | Every joint two links; U = two orthogonal R; S = U + spin | VERIFIED | Book §2.2.1 + companion video page | https://modernrobotics.northwestern.edu/nu-gm-book-resource/2-2-degrees-of-freedom-of-a-robot/ |
| 4 | UR5e range, speed, 5 kg, 850 mm, 20.7 kg, Ø151, 4 m/s, ±0.03 mm | VERIFIED | Official Oct 2026 sheet | https://www.universal-robots.com/manuals/EN/DataSheets/UR5e_techsheet_pdf_online/UR5e_techsheet_en.pdf |
| 5 | UR10e ±360°, 120/180 °/s, 12.5 kg, 1300 mm, 33.3 kg, Ø190 | VERIFIED | Official Oct 2026; TCP 4 m/s and ±0.05 mm also on the sheet, omitted | https://www.universal-robots.com/manuals/EN/DataSheets/UR10e_techsheet_pdf_online/UR10e_techsheet_en.pdf |
| 6 | UR5e DH | VERIFIED | Official article | https://www.universal-robots.com/articles/ur/application-installation/dh-parameters-for-calculations-of-kinematics-and-dynamics/ |
| 7 | UR10e DH | VERIFIED | Same article | same as #6 |
| 8 | UR5e link masses 3.761…0.365 kg; inertia negligible | VERIFIED | Same article | same as #6 |
| 9 | Size torques 9/28/54/150/330 N·m and UR5e/UR10e size map | VERIFIED | Matches the cited 2015 CB3/e-Series article. July 2025 UR-Series article lists 70/107/204/433/738 N·m and omits sizes 0–1 — add a disagreement note | https://www.universal-robots.com/articles/ur/robot-care-maintenance/max-joint-torques-cb3-and-e-series/ |
| 10 | Panda / FER joint limits | VERIFIED | Official FCI + franka_ros YAML; prefer official URL over franka.cn | https://frankarobotics.github.io/docs/robot_specifications.html |
| 11 | Panda accel, jerk, torque rate | VERIFIED | Official FCI FER table | same official FCI |
| 12 | FR3 q range, speed, torque, 10 rad/s² | **FIX** | Use official q_max 2.9007, 1.8361, 2.9007, −0.1169, 2.8763, 4.6216, 3.0508 and q_min −2.9007, −1.8361, −2.9007, −3.0770, −2.8763, 0.4398, −3.0508 rad. Speeds/torques/accel already correct | https://frankarobotics.github.io/docs/robot_specifications.html |
| 13 | FR3 modified DH | VERIFIED | Official FCI table identical | same official FCI |
| 14 | FR3 recommended rectangle | **FIX** | Official q_max 2.3476, 1.5454, 2.4937, −0.4226, 2.5100, 4.2841, 2.7045; q_min −2.3476, −1.5454, −2.4937, −2.7714, −2.5100, 0.7773, −2.7045 rad. Speeds already correct | same official FCI |
| 15 | iiwa 7 ranges | VERIFIED | Spez LBR iiwa V5 p. 13–14 | university-hosted KUKA PDF |
| 16 | iiwa 7 speeds, 23.9 kg, 7 kg, 800 mm | VERIFIED | Same, p. 13 | same |
| 17 | iiwa 7 torques 176 / 110 / 40 N·m | VERIFIED (upgrade) | © 2016 KUKA brochure, dealer-hosted | https://www.ferob.fi/wp-content/uploads/KUKA_SensitiveRobotics_LBR_iiwa_PB_en.pdf |
| 18 | FR3 datasheet ranges and 150 / 301 °/s | VERIFIED (upgrade) | Official R02212 rel. 2.5 (June 2026). Add “FCI use, A6: 239 deg/s” | https://franka.de/hubfs/Datasheet%20Franka%20Research%203_R02212_2.5_EN_June%202026.pdf |
| 19 | Mini Cheetah 9 kg, 0.3 m, 0.21/0.21 m, 0.38 m, 25 cm, 3 joints/leg | VERIFIED | 2020 author PDF §II / Fig. 2. CAD header uses 0.209 / 0.195 m | https://dhkim0821.github.io/papers/2020_ICRA_Vision.pdf |
| 20 | 40 rad/s, 17 N·m | VERIFIED | Kim 2019 §V-A (Katz citation is for 2.45 m/s and 250 W, not this sentence) | https://dhkim0821.github.io/papers/arxiv_wbic_mini-cheetah.pdf |
| 21 | 3.7 m/s, Fr 4.65, 4 m/s then fall, 34 rad/s, 25.5 N·m, gains | VERIFIED | Same §V-A; 25.5 N·m is commanded | same |
| 22 | ~0.3 m, 9 kg, 2.45 m/s cMPC, 360° backflips | VERIFIED | Official abstract (OpenAlex / Semantic Scholar); IEEE HTML 202 | https://doi.org/10.1109/ICRA.2019.8793865 |
| 23 | 6:1, extra 1.55, 9.33:1 | UNVERIFIABLE (partial) | `_abadGearRatio`/`_hipGearRatio` 6 and `_kneeGearRatio` 9.33 in MiniCheetah.h (9.33/6 = 1.555). “Single-stage planetary” and thesis unread (DSpace 429) | https://github.com/mit-biomimetics/Cheetah-Software |
| 24 | 6.9 N·m continuous; 440–480 g | UNVERIFIABLE | Katz 2018 PDF 429 | https://dspace.mit.edu/handle/1721.1/118671 |
| 25 | ANYmal layout, ~250 mm, <30 kg, 0.5 m, 650 Wh, <280 W | VERIFIED | ETH accepted PDF; ⚠NC correct | https://www.research-collection.ethz.ch/handle/20.500.11850/118642 |
| 26 | ANYdrive 12 rad/s, 40 N·m, >70 Hz, 0.025°, 0.08 N·m | VERIFIED | Same, abstract and §III-B | same |
| 27 | Solo 8 modules, 2.2 kg, 24/34 cm, 42×33 cm, 65 cm jump | VERIFIED | arXiv v2; jump is 65 cm from 24 cm standing | https://arxiv.org/abs/1910.00093 |
| 28 | 160 mm, 150 g, N=9, 0.025, 2.7 N·m at 12 A, 10 kHz | VERIFIED | arXiv v2 §II-A; BSD-3-Clause in the paper text | same |
| 29 | Go2 joint ranges | VERIFIED | Official product page | https://www.unitree.com/go2 |
| 30 | Go2 ~45 N·m, ~15 kg, ~3.7 / ~5 m/s | VERIFIED | Same page | same |
| 31 | Go1 C1-8 23.70 N·m, knee 35.55, 12 kg, 0.588×0.22×0.29 m | VERIFIED | Official Go1 page | https://www.unitree.com/mobile/go1 |
| 32 | Go1 body −49~49°, thigh −39~257°, shank −161~−51° | VERIFIED (upgrade) | On the cited page (and official go1.urdf) | same |
| 33 | H1 / H1-2 layout, 400 mm links, sizes, masses, DoF | VERIFIED | Official H1 page | https://www.unitree.com/h1 |
| 34 | H1 / H1-2 peak torques and 3.3 m/s | VERIFIED | Same page | same |
| 35 | H1 joint limits (rad) | VERIFIED (upgrade) | Official unitree_ros h1.urdf; support.unitree.com still 567 | https://raw.githubusercontent.com/unitreerobotics/unitree_ros/master/robots/h1_description/urdf/h1.urdf |
| 36 | iCub 53 DoF, ~105 cm, 36.5 cm, 30/33 kg | VERIFIED | Body is Rev 2.5; filename says 2.3 | IIT PDF with `+(1).pdf` |
| 37 | iCub joint ranges (Parmiggiani Table 1) | UNVERIFIABLE | Paywalled; no author PDF opened; “waist roll −90 to −90” still flagged | https://doi.org/10.1142/S0219843612500272 |
| 38 | Cat 320 boom 5.7 m and R2.9/R2.5 working range | VERIFIED | AEXQ2214 (09-2017) dealer copy; no official cat.com host found | https://www.petersoncat.com/sites/cat/files/products/documents/1508919190-33586.pdf |
| 39 | Cat 320 cylinders 120/1260, 140/1504, 120/1104 mm | VERIFIED | Same 2017 sheet; also unchanged in official 2025 AEXQ3761-02 | same |
| 40 | Swing 11.3 rpm, 74 kN·m, 21 600 kg, 121 kW, 5.7 km/h | VERIFIED | 21 600 kg is on the 2017 sheet (Grok missed it). Later official sheets differ | same |
| 41 | Digging forces 150/134 and 106/103 or 118/114 kN | VERIFIED | Same working-range table | same |
| 42 | Track 3650, 2380, 49 shoes, 8 rollers, 600/700/790, 2833, 468, 1048 mm | VERIFIED | 2017 sheet also lists 900 mm shoes | same |
| 43 | Cylinder length vs angle, law of cosines, Eqs. 7–15 | VERIFIED | Wu 2025, CC BY 4.0 | https://doi.org/10.1371/journal.pone.0337421 |
| 44 | Prototype strokes 560–780, 822–1342, 520–780 mm | VERIFIED | Table 2 | same |
| 45 | θ₁ −13.02–37.13°, θ₂ 16.95–126.42°, θ₃ −34.14–55.45° | VERIFIED | Results text | same |
| 46 | OA 1199.8, AB 661.1, BC 634.9 and pin list | VERIFIED | Table 3 (BA / CB in the paper) | same |
| 47 | Moment arm ℓ = r_A r_B sin θ / L | VERIFIED (computed) | Recalculated from the law of cosines | same |
| 48 | Track pitch 190.5 mm (7.50 in) for older 320/320B | UNVERIFIABLE (partial) | Official PEDJ0819-01: 7.5″ (190.5 mm) pitch class 315–326. UC08-12 not opened | official Cat Scene7 PEDJ0819-01 |
| 49 | LTM 1100-4.2 0–1.7 rpm, ~60 s to 82°, 11.5–60 m ~400 s, 115 m/min | VERIFIED | TD 187.02.DEFISR12.2012 on a distributor host; no official liebherr.com copy found | https://www.tiongwoon.com/sites/default/files/2024-10/LTM_1100-4.2_Catalogue.pdf |
| 50 | 1+6 sections, 17.2 t, 7–14 m lattice, 1 differential cylinder, 4 steered axles | VERIFIED | Same document | same |
| 51 | LTM 1100-5.2 11.5–52 m, 82° ~40 s, 0–2 rpm, ~360 s | VERIFIED (upgrade) | Liebherr TD 206.01.US10.2012 (US edition: 38–171 ft). Not the unread “defisr10-2017” code | https://nchcranehire.ie/wp-content/uploads/2025/02/LTM-1100-5-2-TD.pdf |
| 52 | Reimpell alignment ranges | UNVERIFIABLE | Book not read; Google Books landing 200, no legal numerical extract | https://books.google.com/books?id=CW7u2yBCNIYC |
| 53 | Upper:lower ratios 1.08, 1.07, 1.00, ~1.0, 1.00, 1.97, 1.81 | VERIFIED (computed) | Recalculated from #6, #7, #19, #25, #33, #38, #46 | see cited rows |

## Source table

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| Modern Robotics preprint | https://hades.mech.northwestern.edu/images/7/7f/MR.pdf | 200 PDF | Yes (⚠ARR; also “personal use only, not for further distribution”) |
| Modern Robotics CUP DOI | https://doi.org/10.1017/9781316661239 | 200 Cambridge | Yes ⚠ARR |
| MR companion video page | https://modernrobotics.northwestern.edu/nu-gm-book-resource/2-2-degrees-of-freedom-of-a-robot/ | 200 | Yes ⚠ARR |
| UR5e Oct 2026 sheet | official DataSheets PDF | 200 PDF | Yes ⚠ARR © 2026 UR |
| UR10e Oct 2026 sheet | official DataSheets PDF | 200 PDF | Yes ⚠ARR |
| UR DH article | official support article | 200 | Yes ⚠ARR |
| UR torques CB3/e-Series | official 2015 article | 200 | Yes ⚠ARR |
| UR torques UR-Series (2025) | https://www.universal-robots.com/articles/ur/robot-care-maintenance/max-joint-torques-ur-series/ | 200 | ⚠ARR; not in D1 |
| Official FCI docs | https://frankarobotics.github.io/docs/robot_specifications.html | 200 | Page © 2023 Franka Robotics; docs repo Apache-2.0 (LICENSE opened). D1’s “Apache-2.0 per Grok” for the *docs page* is too loose |
| franka.cn FCI mirror | https://franka.cn/FCI/control_parameters.html | 200 (stale FR3 q table) | © Franka; do not prefer this over official FCI |
| franka_ros panda YAML | GitHub blob 429; raw 200 | raw 200 | Yes Apache-2.0 |
| Official FR3 datasheet R02212 2.5 | https://franka.de/hubfs/Datasheet%20Franka%20Research%203_R02212_2.5_EN_June%202026.pdf | 200 PDF | Yes © Franka Robotics |
| KUKA Spez LBR iiwa V5 | university-hosted PDF | 200 PDF | Yes ⚠ARR © 2015 KUKA |
| KUKA Sensitive robotics brochure | ferob.fi dealer PDF | 200 PDF | Yes ⚠ARR © 2016 KUKA |
| Katz ICRA 2019 DOI | https://doi.org/10.1109/ICRA.2019.8793865 | 202 IEEE (abstract via OpenAlex/S2) | Yes ⚠ARR IEEE |
| Kim 2019 author PDF | https://dhkim0821.github.io/papers/arxiv_wbic_mini-cheetah.pdf | 200 PDF | Yes ⚠ARR (arXiv / author copy) |
| Kim 2020 author PDF | https://dhkim0821.github.io/papers/2020_ICRA_Vision.pdf | 200 PDF | Yes ⚠ARR IEEE, author copy |
| Cheetah-Software | https://github.com/mit-biomimetics/Cheetah-Software | 200 | Lab code (header opened) |
| Katz 2018 thesis handle | https://dspace.mit.edu/handle/1721.1/118671 | 200 HTML; PDF 429 | ⚠ARR; PDF not read |
| ANYmal ETH handle | https://www.research-collection.ethz.ch/handle/20.500.11850/118642 | HTML is a JS shell; bitstream PDF 200 | Yes ⚠NC (In Copyright – Non-Commercial Use Permitted) |
| ANYmal IEEE DOI | https://doi.org/10.1109/IROS.2016.7758092 | 500 landing | IEEE ©; ETH terms as above |
| Grimminger arXiv | https://arxiv.org/abs/1910.00093 | 200; PDF 200 | ⚠ARR IEEE + hardware BSD-3-Clause, as D1 said |
| Solo RA-L DOI | 10.1109/LRA.2020.2976639 (Crossref) | confirmed | ⚠ARR IEEE; not the URL in D1 |
| Unitree Go2 / Go1 / H1 pages | unitree.com/go2, /mobile/go1, /h1 | 200 | Yes ⚠ARR |
| Unitree H1 developer docs | https://support.unitree.com/home/en/H1_developer | 567 | ⚠ARR; not read. Official h1.urdf used instead |
| iCub IIT spec | full `…Rev_2.3_05082019+(1).pdf` | 200 PDF | Yes ⚠ARR; body is Rev 2.5 |
| Parmiggiani 2012 DOI | https://doi.org/10.1142/S0219843612500272 | 403 landing | ⚠ARR; not read |
| Cat AEXQ2214 dealer copy | petersoncat.com … 1508919190-33586.pdf | 200 PDF | Yes ⚠ARR © 2017 Caterpillar |
| Official Cat 2025 sheets | Scene7 AEXQ3761-02, AEXQ3764-01 | 200 PDF | ⚠ARR © 2025 Caterpillar; not in D1 |
| Official Cat undercarriage PEDJ0819-01 | https://s7d2.scene7.com/is/content/Caterpillar/CM20220301-1febf-b7596 | 200 PDF | ⚠ARR © 2022 Caterpillar |
| Wu 2025 PLOS / PMC | https://doi.org/10.1371/journal.pone.0337421 · PMC12637966 | 200 | Yes CC BY 4.0 |
| LTM 1100-4.2 TD 187.02 | tiongwoon.com distributor PDF | 200 PDF | Yes ⚠ARR Liebherr-Werk Ehingen |
| LTM 1100-5.2 TD 206.01.US10.2012 | nchcranehire.ie dealer PDF | 200 PDF | ⚠ARR Liebherr; not in D1 |
| Reimpell Google Books | https://books.google.com/books?id=CW7u2yBCNIYC | 200 (no numerical extract) | Yes ⚠ARR (book) |
| Šagi 2017 DOI | https://doi.org/10.17559/TV-20150220151816 | 500 hrcak (D1 already opened a copy; no alignment numbers) | “open access (licence not checked)” as D1 said |

## Grok section

One live pass, as required: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve`, prompt in `/workspace/lv-scratch/d1/grok_prompt.txt`. Ran **17 November 2026, 11:45:58–11:50:35 AEDT** (exit 0, about 4.6 minutes), output `/workspace/lv-scratch/d1/grok/grok_d1_verify.txt`. No number in the row table above was taken from Grok without opening its source.

**Useful leads I then opened myself:** the official FCI URL and FR3 CSV; official FR3 datasheet R02212; the July 2025 UR-Series torque article; the 2016 KUKA brochure; MiniCheetah.h gear ratios; Go1 ranges on the product page; official Cat PEDJ0819-01 pitch class; Liebherr TD 206.01.US10.2012; Grimminger’s published DOI 10.1109/LRA.2020.2976639.

**Grok errors or limits, not used:** Grok said 21 600 kg was “not found” on AEXQ2214 and quoted 21 900 kg — the dealer AEXQ2214 PDF’s Weights block is **21 600 kg**. Grok reported opening `support.unitree.com/…/About_H1`; my fetches of that host returned 567, so I used the official URDF instead. Grok also could not retrieve the Katz thesis PDF (same 429). Where Grok named a different R02212 release (1.4) or a different LTM 5.2 document code, I used only the PDFs I opened (R02212 2.5; TD 206.01.US10.2012).

## Gaps already listed in D1

D1’s “Not found” list remains fair: no verified Ackermann or typical track/wheelbase ratio; no joint-range tables in the Mini Cheetah or ANYmal papers (ANYmal only says joints rotate fully); no Unitree or ANYmal gear-ratio numbers in the product pages; no Valkyrie table read; no maker DH for the iiwa; no crane minimum luffing angle. Those gaps were not filled in this pass, except the partial track-pitch class and the upgrades named above.
