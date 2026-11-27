# R7-2G Machines: adversarial second check

Checked 2026-11-24 by re-reading each cached source file (PDF text extracts, URDF files, page captures) line by line against every row of R7-2G-machines.md, and re-deriving every **computed** value with a script. Crossref was queried for the ANYmal citation.

| Q | Row checked | Result | Evidence / action |
|---|---|---|---|
| Q29 | Corolla wheelbase 2,640 / 2,700 mm; front track 1,530; rear 1,544 | VERIFIED | M1 "Exterior dimensions" block, same for 16 in and 17/18 in wheels |
| Q29 | Corolla turning radius body 5.6 / 5.8 m, tyre 5.2 / 5.4 m; ratio 13.5 / 13.6:1; 2.65 / 2.76 turns | VERIFIED | M1 "Steering" block |
| Q29 | Corolla US 106.3 in; 35.6 ft curb to curb | VERIFIED | M2 spec table, all trims |
| Q29 | track / wheelbase 0.567, 0.580 (computed) | VERIFIED | 1530/2700 = 0.5667; 1530/2640 = 0.5795 |
| Q29 | Corolla ideal outer 31.3° / inner 42.8° (TS), 30.5° / 41.8° (hatch), computed | VERIFIED (arithmetic); assumptions flagged | asin(2.7/5.2) = 31.3°; atan(2.7/(√(5.2²−2.7²)−1.53)) = 42.8°. The meaning of "tyre" radius is not defined by Toyota, so the row is labelled an estimate |
| Q29 | Isuzu AW 65.6 in, CW 65 in | VERIFIED | M3 "Dimension constants" |
| Q29 | Isuzu WB 132.5 / 150 / 176 in | VERIFIED | M3 header line "WB 132.5 in., 150 in., 176 in." |
| Q29 | Isuzu inside cut angle 46.5° | VERIFIED | M3, text in two places |
| Q29 | Isuzu turning diameters (six wheelbases) | VERIFIED; FIX applied | Table matches. Fix: table prints no unit; added note that feet are implied |
| Q29 | Isuzu ideal outer angles 32.8 to 38.5° (computed) | VERIFIED | re-derived: cot⁻¹(cot 46.5° + 65.6/WB). Cross-check: implied outer-wheel path diameter for 132.5 in WB = 38.8 ft vs published 40.0 ft curb to curb (tyre width adds), consistent |
| Q30 | Cat 320 shoe 790 mm, 10 mm thick triple grouser; 49 shoes; 8 track / 2 carrier rollers | VERIFIED | M4 "Track" and "Weights" ("790 mm (31 in) Width, 10 mm (0.39 in) Thick Triple Grouser Track Shoes") |
| Q30 | Cat 320 track length 4,450; roller centres 3,650; gauge 2,380; undercarriage width 3,170; ground clearance 470 mm | VERIFIED | M4 "Dimensions" items 6 to 10 (re-grepped: 470, 4450, 3650, 2380, 3170 mm) |
| Q30 | M113A2 63 left / 64 right new; 62 / 63 old | VERIFIED | M5 warning text (host p. 26) "63 track shoes on the left ... 64 track shoes on the right"; old 62/63 |
| Q30 | M113A2 road wheels "ten on each side"; sprockets drive | VERIFIED | M5 host p. 79 line "Road wheels - ten on each side to support weight of carrier"; "Tracks - on each side driven by final drive sprockets" |
| Q30 | five road-wheel arms on torsion bars; shocks at 1st, 2nd, 5th | VERIFIED (first reading); wording not re-grepped this pass | M5 WP 0002 00-29 |
| Q30 | M113A2 ground pressure 7.97 psi (54.9 kPa) | VERIFIED | M5 p. 0002 00-43 |
| Q32 | G1 joint limits (14 rows) and degree conversions | VERIFIED | M9 URDF `<limit>` values identical to M8 table; conversions re-run |
| Q32 | G1 efforts 88/88/88/139/35/35, waist 88/35/35, arm 25, wrist pitch/yaw 5; velocities 32/20/30/37/22 | VERIFIED | M9 URDF limit tags |
| Q32 | G1 specs 1320×450×200 mm, ~35 kg, calf+thigh 0.6 m, arm span ~0.45 m, knee torque 90 / 120 N m | VERIFIED | M8 specification table rows |
| Q32 | G1 motor table | VERIFIED (from cached motor_info.md at first reading) | |
| Q32 | G1 offsets 0.030465, 0.12412, 0.17734 (x −0.078273), 0.30001, 0.017558 | VERIFIED | M9 joint origins |
| Q32 | G1 hip-pitch-to-knee 0.332 m (computed) | VERIFIED as arithmetic (sum of z offsets) | Note: it ignores the 0.078 m fore-aft knee offset and the 0.025 m yaw offset; the straight-line distance would be slightly larger. Left as labelled |
| Q32 | Go1 URDF hip ±0.863, thigh −0.686 to 4.501, calf −2.818 to −0.888; efforts 23.7/23.7/35.55; velocities 30.1/30.1/20.06; offsets 0.1881, 0.04675, 0.08, 0.213 | VERIFIED | M7 FR leg tags |
| Q32 | Go1 calf length 0.213 m (knee-to-foot) | VERIFIED | M7 FR_foot_fixed origin z −0.213 |
| Q32 | Go1 marketing ranges and torques 23.70 / 35.55 N m, 12 kg, 0.588×0.22×0.29 m, 3.7 m/s Edu | VERIFIED (from first reading; page not cached as a file) | FIX applied: removed an Air/Pro speed split and a section heading that I could not trace to the page |
| Q32 | ANYmal 250 mm links; 30 kg, 0.5 m; fully rotated joints; 12 rad/s, 40 N m at 48 V; 0.025°; 0.08 N m; 70 → 24 Hz; | VERIFIED | M10 text lines (sec. II.A, III.B); FIX applied: page range 38-44 confirmed by Crossref; section reference corrected |
| Q33 | M113A2 length, widths, heights, clearance, weights, CG, crew, fording, grade, trench | VERIFIED | M5 host pages 93 to 95 (manual 0002 00-43 to -45), all numbers matched; added the other variants' weights from the same page |
| Q33 | M1064 width 83-3/4 in | VERIFIED as printed; flagged | The same number (83-3/4 in) is printed for the M1064 centre-of-gravity distance on the next page; possible typesetting copy, left flagged in Sources disagree |
| Q33 | Cat 320 lengths 9,530 / 8,450; cab 2,960, with boom/stick/bucket 3,160 mm; boom 5.7 m; R2.9 stick; boom cylinder 120 / 1,260 mm | VERIFIED | M4 dimension table and cylinder table |
| Q33 | Cat 320 upper-frame width 2,780; tail swing 2,830; counterweight clearance 1,050 | VERIFIED | M4 items 3 to 5 (2780, 2830, 1050 mm) |
| Q33 | Corolla length / width / height / overhangs / clearance | VERIFIED | M1 block |

Summary: 30 rows checked; 27 VERIFIED against the cached source in this pass, 3 VERIFIED from the first reading only (M113 suspension wording, G1 motor table, Go1 product page); 0 UNVERIFIABLE; 4 FIXes applied (unit note, two untraceable Go1 details removed, ANYmal page range and section).
