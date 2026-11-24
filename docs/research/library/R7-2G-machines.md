# R7-2G Machines: steering geometry, tracks, suspension travel, robot joints, three-view dimensions (Q29 to Q33)

Round 7 library research. **Access date for every source in this file: 2026-11-24**. All values are copied as printed by the source unless marked **computed**. Manufacturer specification sheets are facts published by the maker; they are not peer-reviewed. Unit conversions and any geometry I derived are marked **computed**.

## Sources used (licence and access)

| Key | Source | Free to read? | Licence | What I read |
|---|---|---|---|---|
| M1 | Toyota (GB) PLC, "Toyota Corolla Technical Specifications" (press PDF dated 2025-03-05). media.toyota.co.uk/wp-content/uploads/sites/5/pdf/250305-Corolla-Tech-Specs.pdf | Yes | Manufacturer press material; no licence stated: **treat as all rights reserved** (facts only used) | full PDF, "Steering" and "Exterior dimensions" blocks |
| M2 | Toyota Motor Sales USA, "2025 Corolla" e-brochure. toyota.com/content/dam/toyota/brochures/pdf/2025/corolla_ebrochure.pdf | Yes | Manufacturer brochure, all rights reserved | full PDF, specifications page (turning circle, wheelbase) |
| M3 | Isuzu Commercial Truck of America, "'27 M/Y Isuzu Trucks NRR EV" specification sheet (dated 2025-08-05, "Specifications 19.4"). Hosted at californiahvip.org/wp-content/uploads/2026/05/AL-MY26-Isuzu-Spec-Sheet-260701.pdf | Yes | Manufacturer specification sheet, all rights reserved | full PDF: "Chassis dimensions", "Dimension constants", "Model description", "Turning diameters" |
| M4 | Caterpillar, "320 Hydraulic Excavator Technical Specifications", AEXQ3758-01 (11-2025), © 2025 Caterpillar. s7d2.scene7.com/is/content/Caterpillar/CM20240927-d80d6-c6a9b | Yes | © Caterpillar, all rights reserved | full PDF: "Track", "Dimensions" (Reach boom column) |
| M5 | Headquarters, Department of the Army (2005). *TM 9-2350-261-10, Operator's Manual for Carrier, Personnel, Full Tracked, Armored, M113A2 (and M577A2, M1064, M1068, M1059, M901A1)*, August 2005. Read page by page on nsndepot.com/Library/TM/TM-9-2350-261-10 (pages 26, 79, 93 to 96 of 761) | Yes | **US Government work, "Distribution Statement A: Approved for public release; distribution is unlimited"** (public domain in the US). Third-party host (NSN Depot) shows the manual's text | WP 0002 00 "Tabulated data" (manual pp. 0002 00-43 to -46), WP 0002 00-29 (suspension), track-shoe warning |
| M6 | Unitree Robotics, "Unitree Go1" product page. unitree.com/go1/ | Yes | Manufacturer page, all rights reserved | full page: "Parameters/Version" table, "Flexible and Adaptive Joints" |
| M7 | Unitree Robotics, unitree_ros repository, file robots/go1_description/urdf/go1.urdf (github.com/unitreerobotics/unitree_ros, master branch as fetched 2026-11-24) | Yes | **BSD-3-Clause** (repository licence via GitHub API) | joint `<limit>` and `<origin>` elements of FR leg |
| M8 | Unitree Robotics, "About G1" developer documentation (last updated 2026-09-17). support.unitree.com/home/en/G1_developer/about_G1 | Yes | Manufacturer documentation, all rights reserved | "Joint serial number and joint limit" table; "Robot specifications" table |
| M9 | Unitree Robotics, unitree_ros, robots/g1_description/g1_29dof.urdf and robots/g1_description/motor_info.md (master, fetched 2026-11-24) | Yes | **BSD-3-Clause** | left-leg, waist and left-arm joint `<limit>`/`<origin>`; motor table |
| M10 | Hutter M, Gehring C, Jud D, Lauber A, Bellicoso CD, Tsounis V, Hwangbo J, Bodie K, Fankhauser P, Bloesch M, Diethelm R, Bachmann S, Melzer A, Hoepflinger M (2016). "ANYmal - a highly mobile and dynamic quadrupedal robot." *2016 IEEE/RSJ International Conference on Intelligent Robots and Systems (IROS)*, pp. 38-44 (page range from Crossref). doi:10.1109/IROS.2016.7758092. Author copy: ETH Research Collection doi:10.3929/ethz-a-010686165 | Yes (ETH copy) | ETH Research Collection: **"In Copyright - Non-Commercial Use Permitted"** | full text, sections II.A and III.B |

## Q29. Ackermann steering geometry and track-to-wheelbase ratios (Blocks a plan)

| Q | Vehicle | Quantity | Value | Source, place | Notes |
|---|---|---|---|---|---|
| Q29 | Toyota Corolla Touring Sports (estate), 2025 UK | wheelbase | 2,700 mm | M1, "Exterior dimensions" | hatchback 2,640 mm |
| Q29 | Toyota Corolla (hatchback and Touring Sports), 2025 UK | front track / rear track | 1,530 / 1,544 mm (16 in and 17/18 in wheels) | M1, "Exterior dimensions" | |
| Q29 | Toyota Corolla, 2025 UK | minimum turning radius | body 5.6 m (16 in wheel), 5.8 m (17/18 in); tyre 5.2 m (16 in), 5.4 m (17/18 in) | M1, "Steering" | rack and pinion, ratio 13.5:1 (16 in) / 13.6:1; 2.65 / 2.76 turns lock to lock |
| Q29 | Toyota Corolla sedan, 2025 US | wheelbase; turning circle | 106.3 in; 35.6 ft curb to curb | M2, specifications page | **Sources disagree** with M1 only in body style: US sedan, UK hatch/estate |
| Q29 | Corolla, track / wheelbase | ratio | 0.567 (Touring Sports), 0.580 (hatch) | **computed** from M1 (1530/2700; 1530/2640) | |
| Q29 | Corolla, maximum wheel angles | inner / outer at full lock | **Not published** in M1/M2. Ideal-Ackermann estimate from the 5.2 m tyre radius: outer about 31.3°, inner about 42.8° (Touring Sports); 30.5° / 41.8° (hatch) | **computed** (assumes the 5.2 m radius is the path of the outer front tyre, pivot spacing = front track, zero scrub radius; a geometry estimate, not a measurement) | |
| Q29 | Isuzu NRR EV (low cab forward truck, 2027 model year) | front axle track (AW) / rear axle track (CW) | 65.6 in / 65 in | M3, "Dimension constants" | |
| Q29 | Isuzu NRR EV | wheelbases | 132.5, 150, 176 in (EV); turning table also lists 109, 200, 212 in | M3, page 1 and "Turning diameters" | |
| Q29 | Isuzu NRR EV | inside (inner) wheel cut angle | 46.5° | M3, "Model description" and "Turning diameters" | outer angle not given |
| Q29 | Isuzu N-series, by wheelbase | turning diameter, curb to curb / wall to wall (design value) | 109 in: 32.8 / 38.7 ft; 132.5: 40.0 / 44.9; 150: 45.3 / 50.2; 176: 52.5 / 58.1; 200: 61.0 / 67.2; 212: 66.0 / 73.0 ft | M3, "Turning diameters" table | the table prints no unit; feet are implied by magnitude (40.0 ft = 12.2 m, **computed**) |
| Q29 | Isuzu NRR EV | ideal-Ackermann outer angle for the 46.5° inner angle | 109 in WB: 32.8°; 132.5: 34.7°; 150: 35.8°; 176: 37.1°; 200: 38.1°; 212: 38.5° | **computed**: cot(outer) = cot(46.5°) + track/WB, track 65.6 in | the real linkage is not stated to be 100% Ackermann |
| Q29 | Isuzu NRR EV | track / wheelbase | 0.60 (109 in), 0.50 (132.5 in), 0.37 (176 in) | **computed** from M3 | |
| Q29 | any named car | Ackermann percentage | **Not found** in a maker's published specification | | |

## Q30. Track pitch, sprockets and idlers of named tracked vehicles (Blocks a plan)

| Q | Vehicle | Quantity | Value | Source, place |
|---|---|---|---|---|
| Q30 | Caterpillar 320 (20 t class), long undercarriage | track shoe width | 790 mm (31 in) triple grouser, 10 mm thick, as configured | M4, "Track" and "Weights" |
| Q30 | Cat 320 | number of shoes, each side | 49 | M4, "Track" |
| Q30 | Cat 320 | track rollers / carrier rollers, each side | 8 / 2 | M4, "Track" |
| Q30 | Cat 320 | track length (overall) | 4,450 mm | M4, "Dimensions" item 7 |
| Q30 | Cat 320 | length to centre of rollers (idler-to-sprocket centres) | 3,650 mm | M4, "Dimensions" item 8 |
| Q30 | Cat 320 | track gauge | 2,380 mm | M4, item 9 |
| Q30 | Cat 320 | undercarriage width (790 mm shoes) | 3,170 mm | M4, item 10 |
| Q30 | Cat 320 | ground clearance | 470 mm | M4, item 6 |
| Q30 | Cat 320 | track pitch (link length), sprocket teeth, idler and roller diameters | **Not found** in M4 | |
| Q30 | M113A2 armoured personnel carrier (US Army) | track shoes per side (new track) | left 63, right 64 | M5, WP 0002 00-46 "Tracks"; also the warning on p. 26 of the host |
| Q30 | M113A2 | track shoes per side (old, stretched track) | left 62, right 63 | M5, track-adjust warning (host p. 26) |
| Q30 | M113A2 | road wheels | "ten on each side" (as paired wheels); five road-wheel arms each side on individual torsion bars; shock absorbers at the 1st, 2nd and 5th road wheels | M5, WP 0002 00-29 "Suspension system" |
| Q30 | M113A2 | drive | final-drive sprockets at the front; idler wheels with track adjusters | M5, WP 0002 00-29 |
| Q30 | M113A2 | ground pressure at gross weight | 7.97 psi (54.9 kPa) | M5, WP 0002 00-43 |
| Q30 | M113A2 | track pitch, sprocket teeth, wheel diameters, track width, length on ground | **Not found** in the operator's manual pages read (pages 2 to 140 of 761 scanned for "pitch", "shoes", "sprocket") | |

## Q31. Suspension travel by vehicle class (Improves a plan)

| Q | Class | Result |
|---|---|---|
| Q31 | passenger car, SUV, off-road vehicle, lorry, tank | **Not found from a primary source this round.** The Ford Media Center page for the 2021 F-150 Raptor (wheel travel figures appear in search summaries) answered 403 / redirected when fetched; the 2017 Raptor key-messages PDF answered 403. The M113A2 operator's manual (M5) gives no wheel travel. Secondary websites (tuning shops) give "75 to 125 mm for cars" type ranges with no measurement behind them; not used. |

## Q32. Joint ranges of a published humanoid robot and a four-legged robot (Blocks a plan)

All degree values converted from radians are **computed** (rad × 180/π). Torque = maximum output torque as stated by the source ("effort" in a URDF is the simulator's torque limit, set by the maker).

### Unitree G1 humanoid (about 1.32 m standing, about 35 kg)

| Q | Joint (left side; right side mirrors roll/yaw signs) | Range, rad (M8 = M9) | Range, deg (**computed**) | URDF effort, N m (M9) | URDF velocity, rad/s (M9) |
|---|---|---|---|---|---|
| Q32 | hip pitch | -2.5307 to 2.8798 | -145.0 to 165.0 | 88 | 32 |
| Q32 | hip roll | -0.5236 to 2.9671 | -30.0 to 170.0 | 88 | 32 |
| Q32 | hip yaw | -2.7576 to 2.7576 | -158.0 to 158.0 | 88 | 32 |
| Q32 | knee | -0.087267 to 2.8798 | -5.0 to 165.0 | 139 | 20 |
| Q32 | ankle pitch | -0.87267 to 0.5236 | -50.0 to 30.0 | 35 | 30 |
| Q32 | ankle roll | -0.2618 to 0.2618 | -15.0 to 15.0 | 35 | 30 |
| Q32 | waist yaw | -2.618 to 2.618 | -150.0 to 150.0 | 88 | 32 |
| Q32 | waist roll / pitch (29-DoF version) | -0.52 to 0.52 | -29.8 to 29.8 | 35 | 30 |
| Q32 | shoulder pitch | -3.0892 to 2.6704 | -177.0 to 153.0 | 25 | 37 |
| Q32 | shoulder roll | -1.5882 to 2.2515 | -91.0 to 129.0 | 25 | 37 |
| Q32 | shoulder yaw | -2.618 to 2.618 | -150.0 to 150.0 | 25 | 37 |
| Q32 | elbow | -1.0472 to 2.0944 | -60.0 to 120.0 | 25 | 37 |
| Q32 | wrist roll | -1.972222054 to 1.972222054 | -113.0 to 113.0 | 25 | 37 |
| Q32 | wrist pitch / yaw | -1.614429558 to 1.614429558 | -92.5 to 92.5 | 5 | 22 |

| Q | G1 quantity | Value | Source |
|---|---|---|---|
| Q32 | standing size H × W × D; mass | 1320 × 450 × 200 mm; about 35 kg | M8, "Robot specifications" |
| Q32 | calf + thigh length | 0.6 m | M8 |
| Q32 | arm span | about 0.45 m | M8 |
| Q32 | maximum knee torque | 90 N m (G1), 120 N m (G1 EDU) | M8 |
| Q32 | motor table (motor type: joints, effort N m, velocity rad/s) | 5010-16: neck yaw, wrist pitch/yaw, 13.4, 41.5; 5019-16: shoulder roll/yaw, elbow, wrist roll, ankle, neck pitch, 41, 34.1; 6010-32: shoulder pitch, waist roll/pitch, 60, 30.6; 7519-14.3: waist yaw, hip yaw, 77, 34.9; 7519-22.5: hip pitch/roll, knee, 121, 22.1 | M9, motor_info.md |
| Q32 | leg link offsets (URDF joint origins, m) | hip roll 0.030465 below hip pitch; hip yaw 0.12412 below hip roll; knee 0.17734 below hip yaw (and 0.078 back); ankle pitch 0.30001 below knee; ankle roll 0.017558 below ankle pitch | M9 |
| Q32 | hip-pitch-to-knee vertical offset; knee-to-ankle-pitch | 0.332 m; 0.300 m | **computed** from M9 offsets |

### Unitree Go1 quadruped (12 kg)

| Q | Go1 quantity | Value | Source |
|---|---|---|---|
| Q32 | joint ranges (product-page joint table) | body (hip abduction) -49 to 49°; thigh -39 to 257°; shank (knee) -161 to -51° | M6, joints section |
| Q32 | joint ranges (URDF, front-right leg) | hip -0.863 to 0.863 rad; thigh -0.686 to 4.501 rad; calf -2.818 to -0.888 rad (= -49.4 to 49.4°, -39.3 to 257.9°, -161.5 to -50.9°, **computed**) | M7 |
| Q32 | maximum torque | body/thigh joint 23.70 N m; knee joint 35.55 N m (1.5× ratio) | M6; M7 effort 23.7 / 23.7 / 35.55 |
| Q32 | joint velocity limit | hip and thigh 30.1 rad/s; calf 20.06 rad/s | M7 |
| Q32 | link lengths | thigh 0.213 m (hip-to-knee); calf 0.213 m (knee-to-foot); hip offset 0.08 m sideways from the hip-abduction axis; abduction joints at ±0.1881 m fore-aft, ±0.04675 m lateral from the body centre | M7 joint origins |
| Q32 | folded size; mass | 0.588 × 0.22 × 0.29 m; 12 kg | M6 |
| Q32 | speed | 0 to 3.7 m/s (Edu version) | M6, parameters table |

### ANYmal (2016 research quadruped)

| Q | ANYmal quantity | Value | Source |
|---|---|---|---|
| Q32 | thigh and shank link length | "approximate link length of 250 mm" | M10, sec. II.A |
| Q32 | mass; height | "slightly less than 30 kg"; 0.5 m tall | M10, abstract and II.A |
| Q32 | joint range | all leg joints "can be fully rotated" (continuous; range shown only in Fig. 3) | M10, II.A |
| Q32 | joint speed; torque | 12 rad/s (688 deg/s, **computed**); maximal torque 40 N m (48 V nominal) | M10, III.B |
| Q32 | sensing | position accuracy 0.025°; torque resolution 0.08 N m; torque bandwidth 70 Hz at low amplitude, falling to 24 Hz at 10 N m amplitude | M10, sec. III.B to III.C |

## Q33. Three-view dimensions of public-domain machines (Improves a plan)

| Q | Machine | Quantity | Value as printed | Metric (**computed** where the source gave inches only) | Source |
|---|---|---|---|---|---|
| Q33 | M113A2 (all variants except M1064) | overall length | 191.00 in (485.14 cm) | 4,851 mm | M5, WP 0002 00-43 |
| Q33 | M1064 mortar carrier | overall length | 209.38 in (531.83 cm) | | M5 |
| Q33 | M113A2 family | width, widest (overall) | 105-3/4 in (268.61 cm) | | M5 |
| Q33 | M113A2 family | width, narrowest (track covers off) | 100 in (254.00 cm) | | M5 |
| Q33 | M1064 | width | 83-3/4 in (212.72 cm) | (printed as given; it is smaller than the "narrowest" width of the family, so it may be a different dimension; not resolved) | M5 |
| Q33 | M113A2 and M1064 | height to top of machine-gun pintle | 87-1/2 in (222.25 cm) | | M5 |
| Q33 | M577A2 and M1068 | height to top of antenna guards | 106-1/2 in (270.51 cm) | | M5 |
| Q33 | M1059 | height to top of smoke generator | 100-3/4 in (255.91 cm) | | M5 |
| Q33 | M113A2 family | ground clearance | 17-1/8 in (43.48 cm) | | M5 |
| Q33 | M113A2 | gross weight | 25,007 lb (11,353 kg) | | M5 |
| Q33 | M113A2 | centre of gravity: height; behind sprocket centres | 39-3/4 in (100.96 cm); 82-5/8 in (209.86 cm) | | M5, WP 0002 00-44 |
| Q33 | M113A2 | crew (including driver) | 13 | | M5, WP 0002 00-43 |
| Q33 | M577A2 / M1059 / M1064 / M1068 | gross weight | 25,813 lb (11,719 kg) / 24,400 lb (11,077 kg) / 27,635 lb (12,546 kg) / 26,834 lb (12,182 kg) | | M5, WP 0002 00-43 |
| Q33 | M113A2 | fording depth; water speed | 40 in (101.6 cm); 3.6 mph (5.79 km/h) | | M5, WP 0002 00-45 |
| Q33 | M113A2 | steepest grade / side slope; wall; trench | 60%; 30%; 2 ft; 5-1/2 ft (1.67 m) | | M5, WP 0002 00-44/-45 |
| Q33 | Toyota Corolla hatchback / Touring Sports (2025 UK) | length; width; height; front / rear overhang; ground clearance | 4,370 / 4,650 mm; 1,790 mm; 1,460 mm; 935 / 795 (hatch), 1,015 (TS) mm; 135 mm | | M1, "Exterior dimensions" (not public domain; a named car for scale) |
| Q33 | Caterpillar 320 (Reach boom 5.7 m, R2.9 stick) | machine length with boom/stick/bucket; with boom only | 9,530 mm; 8,450 mm | | M4, "Dimensions" (not public domain; given because a 20 t excavator was asked for) |
| Q33 | Cat 320 | top of cab height; with boom/stick/bucket | 2,960 mm; 3,160 mm | | M4 |
| Q33 | Cat 320 | upper-frame width; tail swing radius; counterweight clearance | 2,780; 2,830; 1,050 mm | | M4 |
| Q33 | Cat 320 | boom length; stick length | 5.7 m (18'8"); R2.9 (9'6") | | M4 |
| Q33 | Cat 320 | boom cylinder bore / stroke | 120 mm / 1,260 mm | | M4 |

## Not found

- Q29: maximum inner and outer steering angles for a named passenger car from the maker (only the Isuzu inner angle was found); any Ackermann percentage. A Mitsubishi service page (mitsubishitechinfo.com, "Steering angle check": inner 38°50' ± 1°30', outer 32°20' reference) was read, but the page does not name the model, so it is not used.
- Q30: track pitch, sprocket tooth count and pitch diameter, idler and road-wheel diameters for either vehicle. (Search summaries claim "6 in pitch, 10-tooth sprocket" for the M113; I did not see it in the manual pages read, so it is not reported.)
- Q31: suspension travel for any class from a primary source.
- Q32: iCub, Valkyrie, Atlas, Spot, MIT Cheetah primary joint tables were not opened. A search summary says Radford et al. 2015 (Valkyrie) has no joint-range table; not checked.
- Q33: public-domain three-view drawings for a wheeled vehicle (HMMWV TM) were not read.

## Sources disagree

- **Unitree G1 hip pitch range:** the joint-limit table and URDF give -2.5307 to 2.8798 rad (-145° to +165°, **computed**), while the same documentation's specification table says "Hip joint: P±154°". The knee is -5° to 165° in the limit table and 0 to 165° in the specification table. Prefer the joint-limit table/URDF for simulation.
- **Unitree G1 joint torque:** URDF effort is 88 N m (hip) and 139 N m (knee); motor_info.md gives 121 N m for the 7519-22.5 motor driving hip pitch/roll and knee; the product table gives 90 N m (G1) / 120 N m (EDU) maximum knee torque. Three numbers from one maker for the same joint.
- **G1 waist yaw:** joint-limit table ±2.618 rad (±150°, **computed**) vs specification table "Waist joint: Z±155°".
- **G1 leg length:** product table "calf + thigh 0.6 m" against 0.332 + 0.300 = 0.632 m from URDF joint offsets (**computed**; the URDF hip-pitch-to-knee path includes the hip roll and yaw housings).
- **M113A2 width:** 105-3/4 in widest vs 83-3/4 in printed for the M1064 (see Q33 row).
