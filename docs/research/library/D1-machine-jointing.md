# D1: How machines are jointed (joint types, arm and leg layouts, actuator limits, excavator, crane and track geometry)

*Specula private library, item D1 (Machines). These are private research notes, not for publication.*

- **Brief item (D1):** "How built machines are jointed: joint kinds, the joint layouts of published humanoid and four-legged robots, how a piston's length follows a joint's angle, track and wheel geometry". Wanted as tables and the geometry. The brief's suggested sources were robotics texts and manufacturers' published specifications. The task also asked for vehicle suspension and steering, cranes and actuator limits.
- **Web reading:** 2026-11-17, about 11:22–11:45 AEDT. All URLs were accessed on 2026-11-17.
- **Grok pass:** grok-4.6 with live search, reasoning effort xhigh, run as one focused pass of 5 questions (`/workspace/d1work/grok_prompt.txt`). It ran **2026-11-17 11:22–11:33 AEDT** and its output is in `grok_d1.txt`. I treated everything Grok returned as unverified leads, and a Grok number is marked Verified only after I read it in the source myself.
- **Scratch:** `/workspace/d1work`, not in any repo. Datasheets and papers were saved there for reading only; no datasets were downloaded.
- **Status key:**
  - **Verified:** I read the number in the manufacturer document or the paper itself.
  - **Verified (preprint / author copy):** I read it in an arXiv or author-hosted copy, not the publisher's version.
  - **Verified (search/publisher excerpt):** I read it in a search-index excerpt of the source text, because the page blocks scripts.
  - **Verified (computed):** I worked it out from verified rows; the arithmetic is shown.
  - **Unverified:** the reason is given in the row.
- **Licence flags:**
  - ⚠ARR: copyright, all rights reserved. Use the numbers and facts only; don't reuse the text or drawings.
  - ⚠NC: non-commercial terms.
  - Open licences are named where they apply.
  - Manufacturer datasheets are all ⚠ARR, and their specifications are "subject to change". Each row names the document's revision or date.

**Row count: 53 data rows: 48 Verified (all sub-kinds), 5 Unverified (rows 23, 24, 37, 48, 52).** Updated 17 Nov 2026 after Library Verify: rows 12 and 14 fixed; rows 17, 18, 32, 35 and 51 upgraded.

## 1. Joint types (kinematic pairs) and counting degrees of freedom

| # | Quantity | Value | Units | Applies to | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 1 | Freedoms f and constraints c of common joints | Revolute R: f 1 (c 2 planar, 5 spatial). Prismatic P: f 1 (2, 5). Helical H (screw): f 1 (–, 5). Cylindrical C: f 2 (–, 4). Universal U: f 2 (–, 4). Spherical S (ball and socket): f 3 (–, 3) | DOF | Any rigid-body mechanism | — | Lynch & Park, *Modern Robotics*, 2017, §2.2.1, Table 2.1 (p. 17 of the May 2017 preprint) | ⚠ARR (Cambridge UP; authors' free preprint) | Verified (preprint / author copy) | https://hades.mech.northwestern.edu/images/7/7f/MR.pdf · https://doi.org/10.1017/9781316661239 |
| 2 | Grübler's formula | dof = m(N − 1 − J) + Σ fᵢ, where m = 3 for planar and 6 for spatial mechanisms, N counts links including ground, and J counts joints. It's exact only if the joint constraints are independent; otherwise it gives a lower bound | — | Open and closed chains | — | Lynch & Park 2017, §2.2.2, Eq. 2.4 | ⚠ARR | Verified (preprint / author copy) | https://hades.mech.northwestern.edu/images/7/7f/MR.pdf |
| 3 | Joint convention | Every joint connects exactly two links. A U joint is two orthogonal R joints. An S joint is a U joint plus spin about the axis | — | Modelling rule | — | Lynch & Park 2017, §2.2.1 and the companion video page | ⚠ARR | Verified (search/publisher excerpt) | https://modernrobotics.northwestern.edu/nu-gm-book-resource/2-2-degrees-of-freedom-of-a-robot/ |

## 2. Industrial and collaborative arms

| # | Quantity | Value | Units | Model | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 4 | UR5e range of motion and speed | 6 revolute joints (base, shoulder, elbow, wrist 1–3). Each has a range of ±360° and a maximum speed of ±180°/s. Payload 5 kg; reach 850 mm; mass with cable 20.7 kg; footprint Ø151 mm; maximum TCP speed 4 m/s; repeatability ±0.03 mm | °, °/s, kg, mm | Universal Robots UR5e | ± tolerance on drawings 0.1 mm / 0.5° | UR5e technical specification (UR5e_techsheet_en, "Updated October 2026") | ⚠ARR (© 2026 Universal Robots) | Verified | https://www.universal-robots.com/manuals/EN/DataSheets/UR5e_techsheet_pdf_online/UR5e_techsheet_en.pdf |
| 5 | UR10e range of motion and speed | All joints ±360°. Base and shoulder ±120°/s; elbow and wrists ±180°/s. Payload 12.5 kg; reach 1300 mm; mass with cable 33.3 kg; footprint Ø190 mm | °, °/s, kg, mm | UR10e | — | UR10e technical specification ("Updated October 2026") | ⚠ARR | Verified | https://www.universal-robots.com/manuals/EN/DataSheets/UR10e_techsheet_pdf_online/UR10e_techsheet_en.pdf |
| 6 | UR5e DH parameters (standard DH) | d₁ 0.1625; a₂ −0.425; a₃ −0.3922; d₄ 0.1333; d₅ 0.0997; d₆ 0.0996 m. α = π/2, 0, 0, π/2, −π/2, 0 | m, rad | UR5e/UR7e | — | UR article "DH parameters for calculations of kinematics and dynamics" (table UR5e/UR7e; undated) | ⚠ARR | Verified | https://www.universal-robots.com/articles/ur/application-installation/dh-parameters-for-calculations-of-kinematics-and-dynamics/ |
| 7 | UR10e DH parameters | d₁ 0.1807; a₂ −0.6127; a₃ −0.57155; d₄ 0.17415; d₅ 0.11985; d₆ 0.11655 m. α is the same as for the UR5e | m, rad | UR10e/UR12e | — | Same UR article | ⚠ARR | Verified | same as #6 |
| 8 | UR5e link masses | Links 1–6: 3.761, 8.058, 2.846, 1.37, 1.3, 0.365 kg (centres of mass are tabulated). UR says the inertia matrices for the UR3e and UR5e are negligible | kg | UR5e/UR7e | — | Same UR article | ⚠ARR | Verified | same as #6 |
| 9 | UR joint torque by joint size | Size 0: 9 N·m; 1: 28; 2: 54; 3: 150; 4: 330. UR5e: wrists are size 1, and elbow, shoulder and base are size 3. UR10e: wrists size 2, elbow size 3, shoulder and base size 4. UR says "you cannot use the full value in your application" | N·m | UR e-Series | maxima | UR article "Max. joint torques CB3 and e-Series" | ⚠ARR | Verified | https://www.universal-robots.com/articles/ur/robot-care-maintenance/max-joint-torques-cb3-and-e-series/ |
| 10 | Franka Emika Panda joint limits | q range for J1–J7: ±2.8973, ±1.7628, ±2.8973, −3.0718 to −0.0698, ±2.8973, −0.0175 to 3.7525, ±2.8973 rad (= ±166°, ±101°, ±166°, −176° to −4°, ±166°, −1° to 215°, ±166°). Maximum speed 2.1750 rad/s (J1–4) and 2.6100 rad/s (J5–7). Torque 87 N·m (J1–4) and 12 N·m (J5–7) | rad, rad/s, N·m | Panda ("Franka Emika Robot") | limits | FCI documentation "Robot and interface specifications", Franka Emika Robot limits; the same values are in franka_ros `joint_limits.yaml` | Apache-2.0 (franka_ros); FCI docs page © Franka | Verified | https://franka.cn/FCI/control_parameters.html · https://github.com/frankaemika/franka_ros/blob/develop/franka_description/robots/panda/joint_limits.yaml |
| 11 | Panda acceleration, jerk and torque-rate limits | Joint acceleration 15, 7.5, 10, 12.5, 15, 20, 20 rad/s². Jerk 7500, 3750, 5000, 6250, 7500, 10000, 10000 rad/s³. Torque rate 1000 N·m/s for all joints | rad/s², rad/s³ | Panda | limits | FCI docs, same table | © Franka | Verified | https://franka.cn/FCI/control_parameters.html |
| 12 | Franka Research 3 joint limits | q_max 2.9007, 1.8361, 2.9007, −0.1169, 2.8763, 4.6216, 3.0508 rad; q_min −2.9007, −1.8361, −2.9007, −3.0770, −2.8763, 0.4398, −3.0508 rad (≈ ±166.2°, ±105.2°, ±166.2°, −176.3° to −6.7°, ±164.8°, 25.2° to 264.8°, ±174.8°). Speed 2.62 rad/s (J1–4), 5.26 (J5), 4.18 (J6), 5.26 (J7). Torque 87 and 12 N·m as for the Panda. Acceleration 10 rad/s² for all joints | rad, rad/s, N·m | FR3 | limits | Official FCI docs, Robot and interface specifications (also control-parameters-joint-fr3.csv); fixed per Library Verify 17 Nov 2026 (the franka.cn mirror is out of date) | © Franka (page); docs repo Apache-2.0 | Verified | https://frankarobotics.github.io/docs/robot_specifications.html |
| 13 | FR3 kinematics (modified DH, Craig convention) | J1 d 0.333; J2 α −π/2; J3 d 0.316, α π/2; J4 a 0.0825, α π/2; J5 a −0.0825, d 0.384, α −π/2; J6 α π/2; J7 a 0.088, α π/2; flange d 0.107 m | m, rad | FR3 | — | FCI docs, "Denavit–Hartenberg parameters" | © Franka | Verified | https://franka.cn/FCI/control_parameters.html |
| 14 | FR3 recommended limits (used in default URDF/rate limiter) | q_max 2.3476, 1.5454, 2.4937, −0.4226, 2.5100, 4.2841, 2.7045 rad; q_min −2.3476, −1.5454, −2.4937, −2.7714, −2.5100, 0.7773, −2.7045 rad. Speed 2, 1, 1.5, 1.25, 3, 1.5, 3 rad/s | rad, rad/s | FR3 | — | Official FCI docs, recommended position–velocity rectangle; fixed per Library Verify 17 Nov 2026 | © Franka (page); docs repo Apache-2.0 | Verified | https://frankarobotics.github.io/docs/robot_specifications.html |
| 15 | KUKA LBR iiwa 7 R800 range of motion | A1 ±170°, A2 ±120°, A3 ±170°, A4 ±120°, A5 ±170°, A6 ±120°, A7 ±175° (7 revolute joints in an alternating roll–pitch layout, with a 2-axis in-line wrist A6–A7) | ° | iiwa 7 R800 | — | KUKA "Spez LBR iiwa V5", issued 28.01.2015, pp. 10, 13 | ⚠ARR (© KUKA) | Verified | https://www.ingegneria.unicampania.it/images/laboratori/2025/aggiornamento_e_schede/Spez_LBR_iiwa_en.pdf |
| 16 | iiwa 7 speed at rated payload | A1 98, A2 98, A3 100, A4 130, A5 140, A6 180, A7 180 °/s. Mass about 23.9 kg; rated payload 7 kg; reach 800 mm | °/s, kg, mm | iiwa 7 R800 | — | Spez LBR iiwa V5, p. 13 | ⚠ARR | Verified | same as #15 |
| 17 | iiwa 7 joint torque | A1–A2 176 N·m; A3–A5 110 N·m; A6–A7 40 N·m | N·m | iiwa 7 R800 | — | KUKA "Sensitive robotics LBR iiwa" brochure, © 2016 KUKA Roboter GmbH (dealer-hosted copy; no kuka.com copy of this table found) | ⚠ARR | Verified (by Library Verify, 17 Nov 2026) | https://www.ferob.fi/wp-content/uploads/KUKA_SensitiveRobotics_LBR_iiwa_PB_en.pdf |
| 18 | FR3 datasheet range of motion | A1 ±166°, A2 ±105°, A3 ±166°, A4 −176° to −7°, A5 ±165°, A6 25° to 265°, A7 ±175°; speed 150°/s (A1–4) and 301°/s (A5–7), but A6 239°/s under FCI use; torque ±87 / ±12 N·m | °, °/s | FR3 | — | Franka "Datasheet Franka Research 3" R02212 rel. 2.5 (June 2026) | ⚠ARR (© Franka Robotics) | Verified (by Library Verify, 17 Nov 2026) | https://franka.de/hubfs/Datasheet%20Franka%20Research%203_R02212_2.5_EN_June%202026.pdf |

## 3. Legged robots: layouts, segments and actuators

| # | Quantity | Value | Units | Model | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 19 | Mini Cheetah size and legs | Mass 9 kg; about 0.3 m tall; upper and lower leg links 0.21 m and 0.21 m; 0.38 m between front and hind hips; 25 cm ground clearance standing. 12 actuators: abduction/adduction, then hip, then knee per leg | kg, m | MIT Mini Cheetah | — | Kim D. et al., ICRA 2020 ("Vision aided…"), §II and Fig. 2 (author PDF); Kim et al. 2019 arXiv 1909.06586, Fig. 3 | ⚠ARR (IEEE; author copies) | Verified (preprint / author copy) | https://dhkim0821.github.io/papers/2020_ICRA_Vision.pdf · https://dhkim0821.github.io/papers/arxiv_wbic_mini-cheetah.pdf |
| 20 | Mini Cheetah actuator limits | Maximum joint velocity 40 rad/s and maximum joint torque 17 N·m, as stated by Kim et al. 2019 citing Katz 2019 | rad/s, N·m | Mini Cheetah | maxima | Kim et al. 2019 (arXiv 1909.06586), §V-A | ⚠ARR (arXiv copy) | Verified (preprint) | https://dhkim0821.github.io/papers/arxiv_wbic_mini-cheetah.pdf |
| 21 | Mini Cheetah in use | Top stable speed 3.7 m/s (Froude number 4.65), with 4 m/s seen just before a fall. Hip joints reached 34 rad/s and 25.5 N·m during the run. Joint gains kp 3 N·m/rad and kd 0.3 N·m·s/rad (1 for abduction joints) | m/s, rad/s, N·m | Mini Cheetah | — | Kim et al. 2019, §V-A, Table I text | ⚠ARR | Verified (preprint) | https://dhkim0821.github.io/papers/arxiv_wbic_mini-cheetah.pdf |
| 22 | Mini Cheetah published summary | Custom backdrivable modular actuators; about 0.3 m and 9 kg; speeds up to 2.45 m/s with cMPC; 360° backflips | m/s | Mini Cheetah | — | Katz, Di Carlo & Kim, ICRA 2019, abstract | ⚠ARR (IEEE) | Verified (search/publisher excerpt; abstract) | https://doi.org/10.1109/ICRA.2019.8793865 |
| 23 | Mini Cheetah gearing | 6:1 single-stage planetary in each module; knee has an extra 1.55:1, giving 9.33:1 (from the lab's CAD header `_kneeGearRatio`) (Grok's reading) | ratio | Mini Cheetah | — | Katz 2018 MIT thesis; Cheetah-Software `MiniCheetah.h` (Grok) | ⚠ARR / lab code | **Unverified** (not read; the 6:1 also appears in a non-primary glossary excerpt) | https://github.com/mit-biomimetics/Cheetah-Software |
| 24 | Mini Cheetah actuator continuous torque | 6.9 N·m continuous; actuator about 440–480 g (Grok flags its own uncertainty) | N·m, g | Mini Cheetah actuator | — | Katz 2018 S.M. thesis (Grok) | ⚠ARR | **Unverified** | https://dspace.mit.edu/handle/1721.1/118671 |
| 25 | ANYmal layout and segments | 3 joints per leg in the order hip abduction/adduction (HAA), hip flexion/extension (HFE), knee flexion/extension (KFE), "mammalian", with point feet. Links are offset so all joints can rotate fully. Thigh and shank about 250 mm; mass a little under 30 kg; 0.5 m tall; 650 Wh battery (3 kg); under 280 W even at full-speed running | mm, kg, W | ANYmal (2016) | approx. | Hutter et al. 2016 IROS, §II-A, §II-B (ETH accepted version) | In Copyright, non-commercial use permitted (ETH) ⚠NC; IEEE © | Verified (author copy) | https://www.research-collection.ethz.ch/handle/20.500.11850/118642 |
| 26 | ANYdrive joint actuator | Series elastic: high-torque motor, harmonic drive and a rotational spring. At 48 V: 12 rad/s and 40 N·m maximum. Torque-control bandwidth > 70 Hz; position accuracy 0.025°; torque resolution 0.08 N·m | rad/s, N·m, Hz | ANYmal ANYdrive | — | Hutter 2016, §III-B and abstract | ⚠NC (ETH) | Verified (author copy) | https://www.research-collection.ethz.ch/handle/20.500.11850/118642 |
| 27 | Solo layout | 8 identical actuator modules: 2 per leg, hip and knee flexion only, so each leg moves in its sagittal plane. Mass 2.2 kg; standing hip height about 24 cm (34 cm maximum); body length 42 cm and width 33 cm; jumps up to 65 cm | cm, kg | Solo (Solo8, open-source quadruped) | — | Grimminger et al. 2020 RA-L (arXiv 1910.00093v2), abstract and §II-C | ⚠ARR (IEEE); hardware design BSD-3-Clause | Verified (preprint) | https://arxiv.org/abs/1910.00093 |
| 28 | Solo actuator | Segment length 160 mm; module mass 150 g; gear reduction N = 9; τ_joint = kᵢ·i·N with kᵢ = 0.025 N·m/A, so τ = 0.225·i; τ_max 2.7 N·m at 12 A; 10 kHz torque control | mm, g, N·m | Solo | — | Grimminger 2020, §II-A | ⚠ARR | Verified (preprint) | https://arxiv.org/abs/1910.00093 |
| 29 | Unitree Go2 joint ranges | Body (hip abduction) −48° to 48°; thigh −200° to 90°; shank (knee) −156° to −48°; 12 motors | ° | Unitree Go2 | — | Unitree Go2 product page, specification table | ⚠ARR (© Unitree) | Verified | https://www.unitree.com/go2 |
| 30 | Unitree Go2 torque and mass | Maximum joint torque about 45 N·m (largest motor; the 12 motors differ); mass with battery about 15 kg; speed up to about 3.7 m/s (about 5 m/s maximum for the upper variants) | N·m, kg, m/s | Go2 | "about" | Go2 product page | ⚠ARR | Verified | https://www.unitree.com/go2 |
| 31 | Unitree Go1 joints | Body and thigh joint motor C1-8: 520 g, 23.70 N·m maximum. Knee: C1-8 × 1.5 ratio, 35.55 N·m maximum. Mass 12 kg; folded size 0.588 × 0.22 × 0.29 m | N·m, g, kg | Go1 | maxima | Unitree Go1 page | ⚠ARR | Verified | https://www.unitree.com/mobile/go1 |
| 32 | Go1 joint ranges | Body −49° to 49°; thigh −39° to 257°; shank −161° to −51° | ° | Go1 | — | Unitree Go1 product page (also official go1.urdf) | ⚠ARR | Verified (by Library Verify, 17 Nov 2026) | https://www.unitree.com/mobile/go1 |
| 33 | Unitree H1 humanoid layout | Each leg 5 DOF (hip × 3, knee × 1, ankle × 1); each arm 4 DOF; thigh and calf 400 mm each; arm 338 mm; size (1520 + 285) × 570 × 220 mm; mass about 47 kg. **H1-2:** legs 6 DOF (ankle × 2) and arms 7 DOF (shoulder 3, elbow 1, wrist 3); 27 DOF total; arm length 685 mm; about 70 kg | DOF, mm, kg | Unitree H1 / H1-2 | "about" | Unitree H1 product page, parameter table | ⚠ARR | Verified | https://www.unitree.com/h1 |
| 34 | H1 peak joint torques | Knee about 360 N·m; hip about 220 N·m; ankle about 59 N·m; arm about 75 N·m. **H1-2:** waist about 220 N·m; ankle 75 × 2 N·m; shoulder about 120; elbow about 120; wrist about 30 N·m. H1 speed 3.3 m/s | N·m, m/s | H1 / H1-2 | "about" | Unitree H1 page | ⚠ARR | Verified | https://www.unitree.com/h1 |
| 35 | H1 joint limits | Hip yaw and roll ±0.43 rad; hip pitch −3.14 to 2.53; knee −0.26 to 2.05; ankle −0.87 to 0.52; torso ±2.35 rad | rad | H1 | — | Official unitree_ros h1.urdf (the support.unitree.com developer docs block automated reads) | ⚠ARR (no licence stated for the URDF; facts only) | Verified (by Library Verify, 17 Nov 2026) | https://raw.githubusercontent.com/unitreerobotics/unitree_ros/master/robots/h1_description/urdf/h1.urdf |
| 36 | iCub humanoid degrees of freedom | Eyes 3; head (neck) 3; chest (torso) 3; arms 7 each (shoulder 3, elbow 1, wrist 3); hands 9 each (19 coupled joints); legs 6 each; 53 in total. Height about 105 cm; width 36.5 cm; 30 kg (33 kg with battery); comparable to a four-year-old child | DOF, cm, kg | iCub | — | IIT "Technical specifications of the iCub platform", Rev 2.5 | ⚠ARR (© IIT) | Verified | https://www.iit.it/documents/175012/528824/Technical-specs_iCub_robot_Rev_2.3_05082019+(1).pdf |
| 37 | iCub joint ranges | e.g. hip FE −120° to +45°, knee 0° to 130°, ankle FE −60° to +70°, shoulder pitch −95° to +10° (Grok's reading). Grok's table also lists "waist roll −90 to −90", which can't be right | ° | iCub | — | Parmiggiani et al. 2012 *IJHR* 9(4):1250027, Table 1 (Grok) | ⚠ARR (World Scientific) | **Unverified** (paywalled; one row is clearly garbled) | https://doi.org/10.1142/S0219843612500272 |

## 4. Hydraulic excavator: geometry, cylinders, tracks, and cylinder length vs joint angle

| # | Quantity | Value | Units | Model | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 38 | Cat 320 boom, stick and working range | Reach boom 5.7 m. With stick R2.9B1 (2.9 m): maximum dig depth 6720 mm; maximum reach at ground 9860 mm; maximum cutting height 9370 mm; maximum loading height 6490 mm; vertical wall depth 5190 mm. With R2.5B1: 6300, 9470, 9170, 6290, 4800 mm. Bucket tip radius 1570 mm | m, mm | Caterpillar 320 (2017 next-gen) | — | Cat "320 Hydraulic Excavator Technical Specifications" AEXQ2214 (09-2017), working-range table | ⚠ARR (© Caterpillar; dealer-hosted copy) | Verified | https://www.petersoncat.com/sites/cat/files/products/documents/1508919190-33586.pdf |
| 39 | Cat 320 cylinders | Boom (2 cylinders) bore 120 mm, stroke 1260 mm; stick bore 140, stroke 1504 mm; bucket bore 120, stroke 1104 mm | mm | Cat 320 | — | AEXQ2214, Hydraulic System | ⚠ARR | Verified | same as #38 |
| 40 | Cat 320 swing, weight and power | Swing speed 11.3 rpm; maximum swing torque 74 kN·m; operating weight 21,600 kg (reach boom, R2.9 stick, GD 1.19 m³ bucket, 600 mm shoes, 4.2 t counterweight); net power 121 kW; maximum travel speed 5.7 km/h | rpm, kN·m, kg | Cat 320 | — | AEXQ2214, Swing / Weights / Engine | ⚠ARR | Verified | same as #38 |
| 41 | Cat 320 digging forces | Bucket 150 kN (ISO) / 134 kN (SAE); stick 106 kN (ISO) / 103 kN (SAE) with R2.9, or 118 / 114 kN with R2.5 | kN | Cat 320 | — | AEXQ2214, working-range table | ⚠ARR | Verified | same as #38 |
| 42 | Cat 320 track geometry | Track length to centre of rollers 3650 mm; track gauge 2380 mm; 49 shoes per side; 8 track rollers per side; shoe widths 600, 700 or 790 mm; tail-swing radius 2833 mm; ground clearance 468 mm; counterweight clearance 1048 mm | mm | Cat 320 | — | AEXQ2214, Dimensions and Track | ⚠ARR | Verified | same as #38 |
| 43 | Cylinder length vs joint angle (law of cosines) | For a cylinder between pins at fixed distances DO and EO from the joint O, with cylinder length L₁: ∠DOE = arccos[(DO² + EO² − L₁²)/(2·DO·EO)], and the joint angle θ₁ = ∠DOE + ∠EOF − ∠DOPₓ. The inverse is L₁ = √(DO² + EO² − 2·DO·EO·cos ∠DOE). The same construction is used for the stick and bucket (the bucket via its four-bar linkage) | m, rad | Excavator boom, stick and bucket | — | Wu et al. 2025 *PLoS ONE* 20:e0337421, Eqs. 7–15 | CC BY 4.0 | Verified | https://doi.org/10.1371/journal.pone.0337421 |
| 44 | Example: prototype excavator cylinder strokes | Boom cylinder 560–780 mm (stroke 220); stick 822–1342 (stroke 520); bucket 520–780 (stroke 260) | mm | Wu 2025 small prototype (boom OA = 1199.8 mm) | — | Wu 2025, Table 2 | CC BY 4.0 | Verified | https://doi.org/10.1371/journal.pone.0337421 |
| 45 | Example: joint angle ranges from those strokes | θ₁ (boom) −13.02° to 37.13°; θ₂ (stick) 16.95° to 126.42°; θ₃ (bucket) −34.14° to 55.45° | ° | Wu 2025 prototype | — | Wu 2025, Results | CC BY 4.0 | Verified | https://doi.org/10.1371/journal.pone.0337421 |
| 46 | Example: link and pin dimensions | OA (boom) 1199.8, AB (stick) 661.1, BC (bucket) 634.9 mm. Pins: DO 290.52, EO 819, FO 804, EF 156, GO 189.74, GA 1102.89, IA 318.57, IB 972.18, KJ 737.27, LK 264, JB 891.3, JA 326.69, BK 156, LM 264, MB 149.83, MC 659.95 mm | mm | Wu 2025 prototype | — | Wu 2025, Table 3 | CC BY 4.0 | Verified | https://doi.org/10.1371/journal.pone.0337421 |
| 47 | Moment arm of a cylinder | Differentiating #43: with the cylinder between points at distances r_A and r_B from the joint and included angle θ, L = √(r_A² + r_B² − 2r_A·r_B·cos θ). The moment arm is ℓ = dL/dθ = r_A·r_B·sin θ / L, so joint torque = F·ℓ. The arm is largest near θ = 90° and zero when the cylinder lines up with the joint | m | Any cylinder-driven revolute joint | — | Derived by me from #43. Grok gives the same form from Hines 2010 (Georgia Tech thesis, Eqs. 15–19, pp. 17–18) | — | Verified (computed; math identity) | https://doi.org/10.1371/journal.pone.0337421 |
| 48 | Track pitch | 190.5 mm (7.50 in) for the older 320/320B; not given on the 2017 sheet (Grok's reading) | mm | Cat 320B / 320 Classic | — | Cat undercarriage bulletin UC08-12, Aug 2008 (Grok; third-party copy) | ⚠ARR | **Unverified** | — |

## 5. Mobile crane

| # | Quantity | Value | Units | Model | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 49 | Crane drive speeds | Slewing 360° at 0–1.7 rpm. Luffing takes about 60 s to reach 82° boom angle. Telescoping from 11.5 to 60 m takes about 400 s. Hoist up to 115 m/min on a single line (19 mm rope, 250 m, 77 kN line pull) | rpm, s, m | Liebherr LTM 1100-4.2 | approx. | Liebherr technical data TD 187.02.DEFISR12.2012, "Drives" | ⚠ARR (© Liebherr-Werk Ehingen; distributor-hosted copy) | Verified | https://www.tiongwoon.com/sites/default/files/2024-10/LTM_1100-4.2_Catalogue.pdf |
| 50 | Crane boom build | Telescopic boom: 1 base section and 6 telescopic sections, each extendable on its own; 17.2 t base counterweight; 7–14 m lattice extension; luffing by 1 differential cylinder; 4 axles, all steered, hydro-pneumatic suspension | — | LTM 1100-4.2 | — | Same document, equipment text | ⚠ARR | Verified | same as #49 |
| 51 | LTM 1100-5.2 | Telescopic boom 38–171 ft (11.58–52.12 m; 1 base and 5 telescopic sections); boom to 82° in about 40 s; slewing 0–2 rpm; telescoping about 360 s | m, °, rpm, s | LTM 1100-5.2 | — | Liebherr TD 206.01.US10.2012 (US edition, dealer-hosted copy) | ⚠ARR | Verified (by Library Verify, 17 Nov 2026; different model from #49) | https://nchcranehire.ie/wp-content/uploads/2025/02/LTM-1100-5-2-TD.pdf |

## 6. Vehicle steering and suspension

| # | Quantity | Value | Units | Model | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 52 | Passenger-car alignment settings | Front camber 0° to −1°20′; caster 4–8° for RWD (8–11° with designed caster offset) and 1–4° for FWD; kingpin inclination 11° to 15°30′; scrub radius −18 to +20 mm; toe-in +15′ ± 10′ (RWD) and 0° ± 10′ (FWD); rear toe 10–20′. Example (a Mercedes): KPI 14°40′, caster 10°10′, scrub radius −14 mm (Grok's reading) | °, ′, mm | Passenger cars (general) | ranges | Reimpell, Stoll & Betzler, *The Automotive Chassis*, 2nd ed., SAE / Butterworth-Heinemann 2001, §3.5–3.10 (Grok) | ⚠ARR (book) | **Unverified** (book not read; a peer-reviewed paper I opened cites Reimpell but quotes no numbers) | https://books.google.com/books?id=CW7u2yBCNIYC |

## 7. Link-length ratios (computed from the verified rows)

| # | Quantity | Value | Units | Model | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 53 | Upper : lower link ratio | UR5e 0.425 / 0.3922 = 1.08. UR10e 0.6127 / 0.57155 = 1.07. Mini Cheetah 0.21 / 0.21 = 1.00. ANYmal about 250 / 250 mm = about 1.0. Unitree H1 400 / 400 = 1.00. Cat 320 boom : stick = 5.7 / 2.9 = 1.97. Wu prototype OA : AB = 1199.8 / 661.1 = 1.81 | ratio | As named | — | Computed by me from #6, #7, #19, #25, #33, #38 and #46 | — | Verified (computed) | see the rows cited |

## Not found

- **Ackermann percentage and typical track/wheelbase ratio:** no primary source was read. Grok also found none in Reimpell. No vehicle steering or suspension number is Verified in this file; row 52 is Unverified.
- **Track pitch, sprocket and idler sizes for a named current excavator:** the Cat 320 2017 sheet doesn't give them.
- **Joint-range tables for the Mini Cheetah and ANYmal:** ANYmal states only that the joints rotate fully.
- **Gear ratios and peak joint speeds:** not found for Unitree Go1, Go2 and H1, or for ANYmal (its N isn't given as a number).
- **Valkyrie joint-range table:** Grok found no range table in Radford et al. 2015; I didn't read the paper.
- **KUKA iiwa DH parameters:** none from the maker.
- **Crane minimum luffing angle:** not stated.
- **Joint ranges for a published full-size humanoid, read in a primary source:** only the iCub DOF list (#36) and the H1 layout and torques (#33–34) are verified.

## Sources disagree

1. **FR3 joint ranges (resolved):** the old franka.cn FCI mirror gave J1 ±157°, J6 31°–259° and J7 ±173°. The current official FCI docs (#12) and the official FR3 datasheet rel. 2.5 (#18) agree once rounded to degrees (±166°, 25°–265°, ±175°). Use the official FCI values (#12). The only remaining difference is A6 speed: 301°/s on the datasheet in general, but 239°/s under FCI use.
1a. **UR joint torques:** the 2015 CB3/e-Series article (#9) gives 9/28/54/150/330 N·m by joint size. A newer official UR-Series article (updated 4 Jul 2025, https://www.universal-robots.com/articles/ur/robot-care-maintenance/max-joint-torques-ur-series/) lists 70/107/204/433/738 N·m and omits sizes 0–1. Use #9 for UR5e/UR10e e-Series and the 2025 article for UR-Series arms.
2. **Mini Cheetah torque:** Kim et al. 2019 report 25.5 N·m at the hip during running, but in the same paragraph give a maximum joint torque of 17 N·m. The extra knee reduction Grok reports (×1.55, #23) would explain more than 17 N·m at some joints, but which joint the 25.5 N·m was measured on, and its gearing, is unresolved. The robot's top speed is 2.45 m/s in Katz 2019 (cMPC) vs 3.7 m/s in Kim 2019 (a newer controller).
3. **UR arm mass:** the October 2026 datasheets give UR5e 20.7 kg and UR10e 33.3 kg. Grok reports 20.6 kg and 33.5 kg from the 2023–24 collective datasheet. Revisions differ, so use the dated document.
4. **Cat 320 operating weight:** the 2017 sheet (#40) gives 21,600 kg. Grok's 2024 sheet gives 21,800 kg (a different bucket, shoes and counterweight). Search excerpts quote 22,600 kg for another configuration. The configuration matters, so quote it with the weight.
5. **Liebherr LTM 1100:** the 4.2 model (#49: 0–1.7 rpm, 60 s to 82°, boom 11.5–60 m) and the 5.2 model (Grok: 0–2 rpm, 40 s, boom 11.5–52 m) are different machines. Don't mix them.

## What Grok added or contradicted

The pass ran 11:22–11:33 AEDT; the output is in `/workspace/d1work/grok_d1.txt`.

- **Added, then confirmed by me:**
  - the UR joint-size torques (#9);
  - the FR3 FCI limits and modified DH parameters (#12–13). I also added the FR3 recommended limits (#14) and the Panda limits (#10–11) from the same page;
  - the KUKA iiwa 7 ranges and speeds (#15–16), from the KUKA spec PDF;
  - the Unitree Go1 torques (#31) and H1 layout and torques (#33–34);
  - the iCub DOF breakdown (#36).
- **Matched my own reading:** the UR5e and UR10e ranges, speeds and DH (#4–7); ANYmal (#25–26); Solo (#27–28); Mini Cheetah 40 rad/s and 17 N·m (#20); Cat 320 cylinders, working range and track (#38–42); Go2 ranges (#29).
- **Added but not confirmed (all Unverified):**
  - iiwa torques (#17);
  - FR3 datasheet ranges (#18);
  - Mini Cheetah gearing and continuous torque (#23–24);
  - Go1 joint ranges (#32);
  - H1 joint limits (#35);
  - the iCub range table (#37), which has a garbled row;
  - Cat 320B track pitch (#48);
  - LTM 1100-5.2 (#51);
  - Reimpell alignment ranges (#52).
- **Contradicted or questionable:**
  - Grok's FR3 datasheet angles disagree with the FCI table (Sources disagree #1).
  - Grok's arm-mass and Cat 320 weight figures come from other document revisions (#3–4).
  - For cylinder kinematics, Grok's best open source was a master's thesis (Hines 2010). I used the peer-reviewed CC BY paper Wu et al. 2025 (#43–46) instead, and derived the moment arm (#47).

## Sources (all accessed 2026-11-17)

| Source | Licence | URL |
|---|---|---|
| Lynch KM, Park FC 2017. *Modern Robotics: Mechanics, Planning, and Control*. Cambridge UP (authors' preprint, May 2017), §2.2 | ⚠ARR (free preprint for personal use) | https://hades.mech.northwestern.edu/images/7/7f/MR.pdf · https://doi.org/10.1017/9781316661239 |
| Universal Robots, UR5e technical specification (updated Oct 2026) | ⚠ARR | https://www.universal-robots.com/manuals/EN/DataSheets/UR5e_techsheet_pdf_online/UR5e_techsheet_en.pdf |
| Universal Robots, UR10e technical specification (updated Oct 2026) | ⚠ARR | https://www.universal-robots.com/manuals/EN/DataSheets/UR10e_techsheet_pdf_online/UR10e_techsheet_en.pdf |
| Universal Robots, "DH parameters for calculations of kinematics and dynamics" (support article) | ⚠ARR | https://www.universal-robots.com/articles/ur/application-installation/dh-parameters-for-calculations-of-kinematics-and-dynamics/ |
| Universal Robots, "Max. joint torques CB3 and e-Series" (support article) | ⚠ARR | https://www.universal-robots.com/articles/ur/robot-care-maintenance/max-joint-torques-cb3-and-e-series/ |
| Franka, FCI documentation "Robot and interface specifications" (Chinese mirror of the FCI docs; tables are numeric) | © Franka; docs repo Apache-2.0 (checked by Library Verify); page © 2023 Franka Robotics | https://franka.cn/FCI/control_parameters.html |
| Franka Robotics, FCI docs "Robot and interface specifications" (official, current; used for #12, #14) | Page © 2023 Franka Robotics; docs repo Apache-2.0 | https://frankarobotics.github.io/docs/robot_specifications.html (accessed 17 Nov 2026) |
| Franka Robotics, Datasheet Franka Research 3, R02212 rel. 2.5, June 2026 (#18) | ⚠ARR | https://franka.de/hubfs/Datasheet%20Franka%20Research%203_R02212_2.5_EN_June%202026.pdf (accessed 17 Nov 2026) |
| KUKA Roboter GmbH 2016, Sensitive robotics LBR iiwa brochure, dealer-hosted (#17) | ⚠ARR | https://www.ferob.fi/wp-content/uploads/KUKA_SensitiveRobotics_LBR_iiwa_PB_en.pdf (accessed 17 Nov 2026) |
| Unitree, unitree_ros h1.urdf (#35) | No licence stated; facts only | https://raw.githubusercontent.com/unitreerobotics/unitree_ros/master/robots/h1_description/urdf/h1.urdf (accessed 17 Nov 2026) |
| Liebherr-Werk Ehingen, LTM 1100-5.2 TD 206.01.US10.2012, dealer-hosted (#51) | ⚠ARR | https://nchcranehire.ie/wp-content/uploads/2025/02/LTM-1100-5-2-TD.pdf (accessed 17 Nov 2026) |
| Universal Robots, Max. joint torques UR-Series (updated 4 Jul 2025; Sources disagree 1a) | ⚠ARR | https://www.universal-robots.com/articles/ur/robot-care-maintenance/max-joint-torques-ur-series/ (accessed 17 Nov 2026) |
| Franka Emika, franka_ros `joint_limits.yaml` (Panda) | Apache-2.0 | https://github.com/frankaemika/franka_ros/blob/develop/franka_description/robots/panda/joint_limits.yaml |
| KUKA, *LBR iiwa 7 R800, LBR iiwa 14 R820 Specification*, Spez LBR iiwa V5, issued 28.01.2015 (university-hosted copy) | ⚠ARR | https://www.ingegneria.unicampania.it/images/laboratori/2025/aggiornamento_e_schede/Spez_LBR_iiwa_en.pdf |
| Katz B, Di Carlo J, Kim S 2019. Mini Cheetah: a platform for pushing the limits of dynamic quadruped control. *ICRA 2019* (abstract) | ⚠ARR (IEEE) | https://doi.org/10.1109/ICRA.2019.8793865 |
| Kim D, Di Carlo J, Katz B, Bledt G, Kim S 2019. Highly dynamic quadruped locomotion via whole-body impulse control and MPC. arXiv 1909.06586 (author PDF) | ⚠ARR | https://dhkim0821.github.io/papers/arxiv_wbic_mini-cheetah.pdf |
| Kim D et al. 2020. Vision aided dynamic exploration of unstructured terrain with a small-scale quadruped robot. *ICRA 2020* (author PDF) | ⚠ARR (IEEE) | https://dhkim0821.github.io/papers/2020_ICRA_Vision.pdf |
| Hutter M et al. 2016. ANYmal – a highly mobile and dynamic quadrupedal robot. *IROS 2016*, 38–44 (ETH accepted version) | ⚠NC (ETH: In Copyright, non-commercial use permitted); IEEE © | https://www.research-collection.ethz.ch/handle/20.500.11850/118642 · https://www.research-collection.ethz.ch/server/api/core/bitstreams/09cfc536-6001-4295-b9ae-4e6b049c151e/content · https://doi.org/10.1109/IROS.2016.7758092 |
| Grimminger F et al. 2020. An open torque-controlled modular robot architecture for legged locomotion research. *IEEE RA-L* 5(2):3650–3657 (arXiv 1910.00093v2) | ⚠ARR (IEEE); hardware BSD-3-Clause | https://arxiv.org/abs/1910.00093 |
| Unitree Robotics, Go2 product page | ⚠ARR | https://www.unitree.com/go2 |
| Unitree Robotics, Go1 product page | ⚠ARR | https://www.unitree.com/mobile/go1 |
| Unitree Robotics, H1 / H1-2 product page | ⚠ARR | https://www.unitree.com/h1 |
| Unitree H1 developer docs (Grok lead; not read) | ⚠ARR | https://support.unitree.com/home/en/H1_developer |
| IIT, *Technical specifications of the iCub platform*, Rev 2.5 | ⚠ARR | https://www.iit.it/documents/175012/528824/Technical-specs_iCub_robot_Rev_2.3_05082019+(1).pdf |
| Parmiggiani A et al. 2012. The design of the iCub humanoid robot. *Int J Humanoid Robotics* 9(4):1250027 (Grok lead; not read) | ⚠ARR | https://doi.org/10.1142/S0219843612500272 |
| Katz BG 2018. A low cost modular actuator for dynamic robots. MIT S.M. thesis (Grok lead; not read) | ⚠ARR | https://dspace.mit.edu/handle/1721.1/118671 |
| MIT Biomimetic Robotics Lab, Cheetah-Software (Grok lead; not read) | lab code | https://github.com/mit-biomimetics/Cheetah-Software |
| Caterpillar, *320 Hydraulic Excavator Technical Specifications*, AEXQ2214 (09-2017) (dealer-hosted copy) | ⚠ARR | https://www.petersoncat.com/sites/cat/files/products/documents/1508919190-33586.pdf |
| Wu H, Ding X, Liu G, Zhu X, Wang H 2025. Structural optimization of the excavator boom under extreme working conditions using EDEM–ADAMS coupled simulation. *PLoS ONE* 20:e0337421 | CC BY 4.0 | https://doi.org/10.1371/journal.pone.0337421 · https://pmc.ncbi.nlm.nih.gov/articles/PMC12637966/ |
| Liebherr-Werk Ehingen, *LTM 1100-4.2 Technical data*, TD 187.02.DEFISR12.2012 (distributor-hosted copy) | ⚠ARR | https://www.tiongwoon.com/sites/default/files/2024-10/LTM_1100-4.2_Catalogue.pdf |
| Reimpell J, Stoll H, Betzler JW 2001. *The Automotive Chassis: Engineering Principles*, 2nd ed. SAE / Butterworth-Heinemann (Grok lead; not read) | ⚠ARR (book) | https://books.google.com/books?id=CW7u2yBCNIYC |
| Šagi G, Lulić Z, Ilinčić P 2017. Multi-objective optimization model in the vehicle suspension system development process. *Tehnički vjesnik* (opened; gives no alignment values) | open access (licence not checked) | https://doi.org/10.17559/TV-20150220151816 |

*Link check, 2026-11-17 at 11:42–11:43 AEDT (curl with redirects followed; output in d1work/linkcheck.txt):*
- 31 unique URLs: 26 returned 200 and 1 returned 202 (IEEE via DOI).
- 3 DOIs resolve correctly (302 from doi.org), but the publisher landing page then refuses automated fetches:
  - IROS 2016 ANYmal → ieeexplore: 500. The ETH accepted-version links in the same row return 200.
  - Parmiggiani 2012 → worldscientific: 403. This is a Grok lead and was not read.
  - Šagi 2017 → hrcak.srce.hr: 500.
- 1 URL blocks bots: support.unitree.com H1_developer returned 567. It is a Grok-lead source for Unverified row #35 and needs a browser check.
- Earlier failures I fixed:
  - The http:// Modern Robotics PDF returned 500, so I switched it to https (200).
  - The MIT hdl.handle.net link returned 500, so I replaced it with dspace.mit.edu (200).
  - The ETH handle page now returns 200, and I added the ETH PDF bitstream (200).
  - The full iCub spec URL ending in "+(1).pdf" returns 200.
- Scratch is in /workspace/d1work (not in any repo). PDFs were saved for reading only; no datasets were downloaded.
