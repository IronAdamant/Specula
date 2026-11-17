# H2 — Other human movement: turning, stairs, jumping, reaching, sit-to-stand, carrying, falling

**Changelog**
- 2026-11-17 09:24 AEDT (Library Verify pass, verify file 09:18): S3/S4 Greve 2013 trunk flexion labelled "angle with the horizontal" (Table 2 footnote a); S5 and the STS-duration disagreement bullet now say Li 2021 is 30 healthy adult **men** (Methods 2.1: "30 healthy adult male subjects"; limitations: "no female subjects"). Both confirmed in the Europe PMC full text. Values unchanged.
- 2026-11-17 (AEDT): per-row access dates added. Every data row (the pipe-separated ID rows) now ends with its source URL and an access date (2026-11-16, the day each was checked); "same" in a URL cell was replaced by the URL itself. Rows that had no URL column (turning, jumping and reaching blocks) gained a "Source URL" column. Numeric notes and "Sources disagree" bullets carry their URL and date too. No values were changed; H2 has no Library Verify file yet.


Access date for every URL: 2026-11-16. Every link below was opened from the box on that date (HTTP 200 for PMC pages; DOI links resolve to the publisher).
Every number was read from the cited table, or from the abstract where the row says "abstract". Values are mean ± SD unless the row says otherwise.
Rules for the seeded Rust generators: use mean ± SD as the sampling band. Where a min–max is given, clamp to it. Do not mix populations (young vs older, lab vs real-life) within one generator.
Licence column: CC BY = commercial reuse of the paper's text and figures allowed with attribution. CC BY-NC = no commercial reuse of the paper. "Publisher ©" = paywalled or not open licence; facts only, nothing copied. Numeric facts themselves are not copyrightable, but cite the source anyway.

## 1. Sit-to-stand (STS)

ID  | Measure                                     | Population (n)                        | Value                                     | Source, table          | Licence | URL | Accessed
S1  | STS duration, single rise (s)               | young, Table-1 age 24.3±2 y (enrolled n=15)| 1.71 ± 0.17                               | Greve 2013, Table 2    | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC3808394/ | 2026-11-16
S2  | STS duration, single rise (s)               | older, Table-1 age 76.4±5.2 y (enrolled n=11)| 1.78 ± 0.17                               | Greve 2013, Table 2    | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC3808394/ | 2026-11-16
S3  | Peak trunk flexion during STS (deg, "angle with the horizontal" per Table 2 footnote a)         | young (enrolled n=15)                 | 36.95 ± 7.98                              | Greve 2013, Table 2    | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC3808394/ | 2026-11-17
S4  | Peak trunk flexion during STS (deg, "angle with the horizontal" per Table 2 footnote a)         | older (enrolled n=11)                 | 33.69 ± 6.73                              | Greve 2013, Table 2    | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC3808394/ | 2026-11-17
S5  | STS motion time, natural speed (s)          | healthy young **men**, age 25.7±2.0 y (n=30, all male)  | 1.60 (mean only; SD not given in text)    | Li 2021, Discussion    | CC BY-NC| https://pmc.ncbi.nlm.nih.gov/articles/PMC8183776/ | 2026-11-17
S6  | STS phase 1, flexion momentum (s)           | healthy older women, 71.9±5.8 y (n=17)| 0.62 ± 0.09                               | Hayamizu 2025, Table 4 | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ | 2026-11-16
S7  | STS phase 2, momentum transfer (s)          | same (n=17)                           | 0.19 ± 0.12                               | Hayamizu 2025, Table 4 | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ | 2026-11-16
S8  | STS phase 3, extension (s)                  | same (n=17)                           | 0.99 ± 0.25                               | Hayamizu 2025, Table 4 | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ | 2026-11-16
S9  | STS total (s)                               | same (n=17)                           | 1.80 ± 0.33                               | Hayamizu 2025, Table 4 | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ | 2026-11-16
S10 | Hip flexion, seated start (deg)             | same (n=17)                           | 72.2 ± 5.3                                | Hayamizu 2025, Table 3 | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ | 2026-11-16
S11 | Knee flexion, seated start (deg)            | same (n=17)                           | 88.4 ± 5.8                                | Hayamizu 2025, Table 3 | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ | 2026-11-16
S12 | Ankle dorsiflexion, seated start (deg)      | same (n=17)                           | 12.0 ± 8.3                                | Hayamizu 2025, Table 3 | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ | 2026-11-16
S13 | Hip flexion, end of phase 1 (seat-off) (deg)| same (n=17)                           | 95.5 ± 10.0                               | Hayamizu 2025, Table 3 | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ | 2026-11-16
S14 | Knee flexion, end of phase 1 (deg)          | same (n=17)                           | 82.8 ± 6.0                                | Hayamizu 2025, Table 3 | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ | 2026-11-16
S15 | Ankle dorsiflexion, end of phase 1 (deg)    | same (n=17)                           | 16.8 ± 7.5                                | Hayamizu 2025, Table 3 | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ | 2026-11-16
S16 | Hip / knee / ankle, end of phase 2 (deg)    | same (n=17)                           | 86.7±10.1 / 76.2±6.0 / 20.1±8.5           | Hayamizu 2025, Table 3 | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ | 2026-11-16
S17 | Hip / knee / ankle, standing end (deg)      | same (n=17)                           | −1.0±4.9 / 7.8±5.0 / 4.2±6.7              | Hayamizu 2025, Table 3 | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ | 2026-11-16
S18 | 5-rep STS test, total duration, fast (s)    | 20–40 y (n≈35–38)                     | 7.86 ± 0.71                               | Van Roie 2019, Table 1 | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC6336282/ | 2026-11-16
S19 | 5-rep STS test, total duration, fast (s)    | 40–55 y (n≈28–29)                     | 8.47 ± 0.99                               | Van Roie 2019, Table 1 | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC6336282/ | 2026-11-16
S20 | 5-rep STS test, total duration, fast (s)    | 55–70 y (n≈28–31)                     | 8.59 ± 1.01                               | Van Roie 2019, Table 1 | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC6336282/ | 2026-11-16
S21 | 5-rep STS, "sit-to-stand transition duration" (s) | 20–40 / 40–55 / 55–70 y         | 4.10±0.37 / 4.39±0.49 / 4.47±0.49         | Van Roie 2019, Table 1 | CC BY   | https://pmc.ncbi.nlm.nih.gov/articles/PMC6336282/ | 2026-11-16
Notes:
- Hayamizu: the seat was set to knee-joint height, arms were crossed, and the knee started at 90°. Angles were measured from a high-speed camera. The population is older Japanese women only. [https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ · accessed 2026-11-16]
- Van Roie: the test was done as fast as possible with arms crossed, on a 0.46 m chair. S21 is very likely the sum of the five rising transitions. The paper defines the sub-phases but the table does not say "summed", so do not divide it by 5 without checking. [https://pmc.ncbi.nlm.nih.gov/articles/PMC6336282/ · accessed 2026-11-16]

## 2. Turning (180° standing turn, IMU)

All rows are from Khobkhun 2021 (Sensors), Table 1. Older group: n=21, age 66.4±3.25 y. Young group: n=21, age 22.47±2.18 y. Licence CC BY 4.0. URL: https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ (accessed 2026-11-16)
Values are mean (SD), given as slow / moderate / fast.
ID  | Measure                                 | Older                                   | Young | Source URL | Accessed
T1  | Head onset (s)                           | 0.63(0.07) / 0.59(0.04) / 0.54(0.06)    | 0.53(0.08) / 0.54(0.07) / 0.52(0.05) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ | 2026-11-16
T2  | Thorax onset (s)                         | 0.67(0.07) / 0.62(0.06) / 0.58(0.07)    | 0.57(0.07) / 0.58(0.08) / 0.55(0.06) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ | 2026-11-16
T3  | Pelvis onset (s)                         | 0.71(0.09) / 0.68(0.06) / 0.65(0.09)    | 0.65(0.06) / 0.60(0.07) / 0.57(0.04) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ | 2026-11-16
T4  | Leading foot onset (s)                   | 0.92(0.13) / 0.81(0.08) / 0.81(0.13)    | 0.91(0.18) / 0.82(0.10) / 0.69(0.08) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ | 2026-11-16
T5  | Trailing foot onset (s)                  | 1.28(0.14) / 1.22(0.21) / 1.17(0.21)    | 1.25(0.27) / 1.13(0.15) / 0.89(0.11) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ | 2026-11-16
T6  | Peak head yaw velocity (deg/s)           | 128.17(15.17) / 141.32(23.35) / 155.45(24.08) | 136.75(24.18) / 181.45(19.60) / 271.53(48.23) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ | 2026-11-16
T7  | Peak head–thorax separation (deg)        | 9.45(3.33) / 14.67(5.23) / 18.70(7.56)  | 16.53(7.19) / 17.63(5.99) / 21.75(7.70) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ | 2026-11-16
T8  | Peak head–pelvis separation (deg)        | 9.89(3.61) / 15.74(5.93) / 19.08(8.60)  | 15.70(6.85) / 18.49(5.99) / 22.46(6.61) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ | 2026-11-16
T9  | Total steps to turn 180° (n)             | 4.81(0.80) / 4.16(0.24) / 3.38(0.49)    | 4.31(0.60) / 3.43(0.40) / 2.95(0.31) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ | 2026-11-16
T10 | "Step duration" (s), as labelled         | 3.23(0.18) / 2.43(0.17) / 2.15(0.32)    | 3.05(0.26) / 2.22(0.22) / 1.72(0.21) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ | 2026-11-16
T11 | Step size (deg of turn per step)         | 59.21(8.57) / 67.68(9.65) / 75.22(8.91) | 73.47(11.54) / 74.71(11.16) / 79.46(12.91) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ | 2026-11-16
Use for animation: the turn runs top-down, head first, then thorax, then pelvis, then feet. That order is consistent in every cell of T1–T5. [https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ · accessed 2026-11-16]
- T10 is labelled "step duration", but its size (2–3 s) matches the whole stepping period of the turn, not a single step. Treat it as total turn stepping time. [https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ · accessed 2026-11-16]
- The table's "step frequency (n)" row is ambiguous, so it is left out.

## 3. Stairs

ID  | Measure                                     | Population (n)            | Value                                         | Source                          | Licence     | URL | Accessed
ST1 | Time to ascend 6 steps, fast, no handrail (s)| 20–40 y (n≈35–38)        | 1.45 ± 0.19                                   | Van Roie 2019, Table 1          | CC BY       | https://pmc.ncbi.nlm.nih.gov/articles/PMC6336282/ | 2026-11-16
ST2 | same (s)                                    | 40–55 y                   | 1.52 ± 0.17                                   | Van Roie 2019, Table 1          | CC BY       | https://pmc.ncbi.nlm.nih.gov/articles/PMC6336282/ | 2026-11-16
ST3 | same (s)                                    | 55–70 y                   | 1.78 ± 0.34                                   | Van Roie 2019, Table 1          | CC BY       | https://pmc.ncbi.nlm.nih.gov/articles/PMC6336282/ | 2026-11-16
ST4 | same, carrying +10% body mass (s)           | 20–40 / 40–55 / 55–70 y   | 1.48±0.18 / 1.57±0.17 / 1.81±0.29             | Van Roie 2019, Table 1          | CC BY       | https://pmc.ncbi.nlm.nih.gov/articles/PMC6336282/ | 2026-11-16
ST5 | Stair geometry used for ST1–ST4             | —                         | riser 18 cm, tread 26.5 cm                    | Van Roie 2019, Methods          | CC BY       | https://pmc.ncbi.nlm.nih.gov/articles/PMC6336282/ | 2026-11-16
ST6 | Ascent vs descent: peak hip angle difference (deg) | young 18–39 y (n=33, 16M/17F) | ascent greater by 28.10 (SD 4.08)         | Protopapadaki 2007, abstract    | Publisher © | https://doi.org/10.1016/j.clinbiomech.2006.09.010 ; https://europepmc.org/abstract/MED/17126461 | 2026-11-16
ST7 | Ascent vs descent: knee angle difference (deg)| same                    | ascent greater by 3.39 (SD 7.20)              | Protopapadaki 2007, abstract    | Publisher © | https://doi.org/10.1016/j.clinbiomech.2006.09.010 | 2026-11-16
ST8 | Descent vs ascent: ankle dorsiflexion / plantarflexion (deg) | same     | descent greater by 9.90 (SD 3.80) / 8.78 (SD 4.80) | Protopapadaki 2007, abstract | Publisher © | https://doi.org/10.1016/j.clinbiomech.2006.09.010 | 2026-11-16
ST9 | Stair geometry used for ST6–ST8             | —                         | riser 18 cm, tread 28.5 cm, 4 steps           | Protopapadaki 2007, abstract    | Publisher © | https://doi.org/10.1016/j.clinbiomech.2006.09.010 | 2026-11-16
Note: ST1–ST4 are maximum-speed trials. Per step that works out to about 0.24 s for young adults (my arithmetic: 1.45 / 6), which is far faster than normal stair walking. [https://pmc.ncbi.nlm.nih.gov/articles/PMC6336282/ · accessed 2026-11-16]
- No open-access source with absolute peak stair joint angles was found; see Not found.
- The open dataset Lencioni 2019 (Sci Data, CC BY 4.0, https://pmc.ncbi.nlm.nih.gov/articles/PMC6897988/) contains raw stair kinematics. Its paper tables describe the data structure only, so no summary numbers are taken from it. (accessed 2026-11-16)
- The dataset was not downloaded.

## 4. Jumping (countermovement jump, CMJ)

All rows are from Koivunen 2026 (Scand J Med Sci Sports), Tables 1 and 2. Pooled Finnish data: males n=24,804; females n=5,413. Licence CC BY 4.0. URL: https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ (accessed 2026-11-16)
Values are CMJ height in cm, mean (SD).
ID  | Age group | Males: n, CMJ (cm)  | Females: n, CMJ (cm) | Source URL | Accessed
J1  | 6–10      | 1205, 22.1 (3.4)    | 184, 21.5 (4.1) | https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ | 2026-11-16
J2  | 11–15     | 21492, 27.3 (5.3)   | 3496, 24.5 (4.0) | https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ | 2026-11-16
J3  | 16–20     | 1764, 35.3 (4.7)    | 402, 26.9 (4.0) | https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ | 2026-11-16
J4  | 21–25     | 39, 34.6 (10.3)     | 50, 27.9 (8.8) | https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ | 2026-11-16
J5  | 26–30     | 54, 34.9 (8.5)      | 64, 23.5 (6.4) | https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ | 2026-11-16
J6  | 31–35     | 71, 32.4 (7.8)      | 75, 22.8 (7.0) | https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ | 2026-11-16
J7  | 36–40     | 50, 30.3 (5.9)      | 102, 21.8 (6.0) | https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ | 2026-11-16
J8  | 41–45     | 63, 28.6 (5.3)      | 70, 19.5 (5.3) | https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ | 2026-11-16
J9  | 46–50     | 35, 27.2 (5.7)      | 380, 19.8 (4.6) | https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ | 2026-11-16
J10 | 51–55     | 10, 26.2 (4.1)      | 502, 18.7 (3.9) | https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ | 2026-11-16
J11 | 56–60     | 10, 24.7 (3.6)      | 17, 16.2 (3.5) | https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ | 2026-11-16
J12 | 61–65     | 4, 23.7 (4.4)       | 45, 13.3 (3.2) | https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ | 2026-11-16
J13 | >65       | 7, 22.1 (3.0)       | 26, 15.0 (3.9) | https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ | 2026-11-16
- The same tables give peak power in W/kg; for example, males aged 21–25 reach 47.5 (5.9). [https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ · accessed 2026-11-16]
- Adult cells with n < 20 are unstable; widen them or merge them with the neighbouring groups. [https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ · accessed 2026-11-16]
- (J13 male mean age 69.6 y.) [https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ · accessed 2026-11-16]

## 5. Reaching (seated reach-to-grasp and place, real objects)

All rows are from Viau 2004 (J NeuroEng Rehabil), Tables 2 and 3. Healthy adults n=8, physical (non-VR) condition only. Licence CC BY 2.0. URL: https://pmc.ncbi.nlm.nih.gov/articles/PMC546398/ (accessed 2026-11-16)
ID  | Measure                                                | Healthy, physical condition | Source URL | Accessed
R1  | Movement time, onset to grasp (s)                       | 0.68 ± 0.17 | https://pmc.ncbi.nlm.nih.gov/articles/PMC546398/ | 2026-11-16
R2  | Time of peak wrist velocity (% of reach)                | 44.9 ± 10.4 | https://pmc.ncbi.nlm.nih.gov/articles/PMC546398/ | 2026-11-16
R3  | Time of maximum grip aperture (% of reach)              | 72.5 ± 12.5 | https://pmc.ncbi.nlm.nih.gov/articles/PMC546398/ | 2026-11-16
R4  | Maximum grip aperture (mm), ball                        | 95.7 ± 16.4 | https://pmc.ncbi.nlm.nih.gov/articles/PMC546398/ | 2026-11-16
R5  | Hand-path curvature index, reach (path / straight line) | 1.39 ± 0.16 | https://pmc.ncbi.nlm.nih.gov/articles/PMC546398/ | 2026-11-16
R6  | Movement time, onset to placing (transport) (s)         | 0.84 ± 0.29 | https://pmc.ncbi.nlm.nih.gov/articles/PMC546398/ | 2026-11-16
R7  | Curvature index, transport                              | 1.14 ± 0.03 | https://pmc.ncbi.nlm.nih.gov/articles/PMC546398/ | 2026-11-16
R8  | Elbow extension during transport (deg)                  | 25.6 ± 6.9 | https://pmc.ncbi.nlm.nih.gov/articles/PMC546398/ | 2026-11-16
R9  | Shoulder flexion during transport (deg)                 | 24.8 ± 5.2 | https://pmc.ncbi.nlm.nih.gov/articles/PMC546398/ | 2026-11-16
R10 | Shoulder abduction during transport (deg)               | 13.6 ± 5.2 | https://pmc.ncbi.nlm.nih.gov/articles/PMC546398/ | 2026-11-16
R11 | Wrist extension at placing (deg)                        | 18.2 ± 12.1 | https://pmc.ncbi.nlm.nih.gov/articles/PMC546398/ | 2026-11-16
For animation: the hand reaches peak speed a little before mid-reach and opens widest at about 3/4 of the reach. Those are the classic Jeannerod-style timing anchors. [https://pmc.ncbi.nlm.nih.gov/articles/PMC546398/ · accessed 2026-11-16]

## 6. Carrying (backpack load, treadmill 1.25 m/s)

ID  | Measure                                         | Population (n)          | Value                                                                 | Source                     | Licence     | URL | Accessed
C1  | Unloaded baseline: step length / width / time   | healthy adults (n=8)    | 0.662 m / 0.142 m / 0.532 s (means; SD in figures only)               | Huang & Kuo 2014, Results  | Publisher © (Company of Biologists; free on PMC, not CC) | https://pmc.ncbi.nlm.nih.gov/articles/PMC3922835/ | 2026-11-16
C2  | Unloaded double-support time (s)                | same                    | 0.179                                                                 | Huang & Kuo 2014, Results  | same        | https://pmc.ncbi.nlm.nih.gov/articles/PMC3922835/ | 2026-11-16
C3  | Load range tested                               | same                    | backpack up to 40% body weight                                        | Huang & Kuo 2014, abstract | same        | https://pmc.ncbi.nlm.nih.gov/articles/PMC3922835/ | 2026-11-16
C4  | Effect of load on step length, width, duration  | same                    | no significant change                                                 | Huang & Kuo 2014, Results  | same        | https://pmc.ncbi.nlm.nih.gov/articles/PMC3922835/ | 2026-11-16
C5  | Double support as % of step period              | same                    | about 29% unloaded → about 38% at max load (about +30% duration)      | Huang & Kuo 2014, Results  | same        | https://pmc.ncbi.nlm.nih.gov/articles/PMC3922835/ | 2026-11-16
C6  | Step-width variability                          | same                    | about +50% over the load range (P = 0.01)                             | Huang & Kuo 2014, Results  | same        | https://pmc.ncbi.nlm.nih.gov/articles/PMC3922835/ | 2026-11-16
C7  | Extra metabolic power per kg carried (W/kg load)| same                    | about 7.6                                                             | Huang & Kuo 2014, abstract | same        | https://pmc.ncbi.nlm.nih.gov/articles/PMC3922835/ | 2026-11-16
C8  | Direction of change with backpack (meta-analysis, 54 studies) | mixed     | trunk flexion ↑, hip and ankle ROM ↑, cadence ↑, stride length ↓ (no pooled magnitudes in abstract) | Liew 2016, abstract | Publisher © | https://doi.org/10.1123/jab.2015-0339 ; https://europepmc.org/abstract/MED/27705050 | 2026-11-16

## 7. Falling

ID  | Measure                                                   | Population (n)                         | Value                                                   | Source                          | Licence     | URL | Accessed
F1  | Fall duration, onset of imbalance → pelvis impact (ms)    | long-term care, 80±9.8 y (25 falls, 23 people) | 1271 ± 648                                      | Choi 2015, abstract             | Publisher © | https://doi.org/10.1016/j.jbiomech.2015.02.025 ; https://europepmc.org/abstract/MED/25769730 | 2026-11-16
F2  | Descent duration, start of descent → pelvis impact (ms)   | same                                   | 583 ± 255                                               | Choi 2015, abstract             | Publisher © | https://doi.org/10.1016/j.jbiomech.2015.02.025 | 2026-11-16
F3  | Vertical impact velocity, pelvis / head / hand (m/s)      | same                                   | 2.14±0.63 / 2.91±0.86 / 2.87±1.60                       | Choi 2015, abstract             | Publisher © | https://doi.org/10.1016/j.jbiomech.2015.02.025 | 2026-11-16
F4  | Impacts: pelvis / head / hand (falls of 25)               | same                                   | 25 / 12 / 21                                            | Choi 2015, abstract             | Publisher © | https://doi.org/10.1016/j.jbiomech.2015.02.025 | 2026-11-16
F5  | Critical-phase duration, real-life falls (ms)             | 3 frail adults 85–95 y, 26 falls       | 1946 ± 639 (participant B 1933±538, C 1883±719, A single fall 2750) | Vlaeyen 2013, Table 2 | CC BY 2.0 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3850536/ | 2026-11-16
F6  | Initial fall direction (% of 26)                          | same                                   | backward 54, sideways 23, forward 15, straight down 8   | Vlaeyen 2013, Table 2           | CC BY 2.0   | https://pmc.ncbi.nlm.nih.gov/articles/PMC3850536/ | 2026-11-16
F7  | Landing configuration (% of 26)                           | same                                   | backward 62, sideways 27, forward 11                    | Vlaeyen 2013, Table 2           | CC BY 2.0   | https://pmc.ncbi.nlm.nih.gov/articles/PMC3850536/ | 2026-11-16
F8  | Activity before fall (% of 26)                            | same                                   | transitioning 35, walking 27, bending down 19, dressing 15, bathing 4 | Vlaeyen 2013, Table 2 | CC BY 2.0 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3850536/ | 2026-11-16
F9  | Body sites hit (% of 26)                                  | same                                   | pelvis 89, torso 81, head 62, elbow/forearm 62, hand/wrist 46, shoulder 46, knee 23 | Vlaeyen 2013, Table 2 | CC BY 2.0 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3850536/ | 2026-11-16
For a fall generator: sample the direction from F6, and the duration from F1 or F5 (but see Sources disagree). [https://doi.org/10.1016/j.jbiomech.2015.02.025 · accessed 2026-11-16]

## Source list (all accessed 2026-11-16)

1. Greve C, Zijlstra W, Hortobágyi T, Bongers RM. Not all is lost: old adults retain flexibility in motor behaviour during sit-to-stand. PLoS One 2013;8(10):e77760. doi:10.1371/journal.pone.0077760.
   CC BY. https://pmc.ncbi.nlm.nih.gov/articles/PMC3808394/ (accessed 2026-11-16)
2. Li J, Xue Q, Yang S, et al. Kinematic analysis of the human body during sit-to-stand in healthy young adults. Medicine (Baltimore) 2021;100(22):e26208. doi:10.1097/MD.0000000000026208.
   CC BY-NC 4.0. https://pmc.ncbi.nlm.nih.gov/articles/PMC8183776/ (accessed 2026-11-16)
3. Hayamizu Y, Nagafusa T, Sasaki K, et al. Comparing EMG muscle activities and kinematics during sit-to-stand transitions in patients with adult spinal deformity versus healthy controls. J Clin Med 2025;14(7):2514. doi:10.3390/jcm14072514.
   CC BY 4.0. https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ (accessed 2026-11-16)
4. Van Roie E, Van Driessche S, Huijben B, et al. A body-fixed-sensor-based analysis of stair ascent and sit-to-stand to detect age-related differences in leg-extensor power. PLoS One 2019;14(1):e0210653. doi:10.1371/journal.pone.0210653.
   CC BY 4.0. https://pmc.ncbi.nlm.nih.gov/articles/PMC6336282/ (accessed 2026-11-16)
5. Khobkhun F, Hollands M, Richards J. The effect of different turn speeds on whole-body coordination in younger and older healthy adults. Sensors 2021;21(8):2827. doi:10.3390/s21082827.
   CC BY 4.0. https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ (accessed 2026-11-16)
6. Protopapadaki A, Drechsler WI, Cramp MC, et al. Hip, knee, ankle kinematics and kinetics during stair ascent and descent in healthy young individuals. Clin Biomech 2007;22(2):203–210. doi:10.1016/j.clinbiomech.2006.09.010.
   Publisher ©; abstract only. https://doi.org/10.1016/j.clinbiomech.2006.09.010 ; https://europepmc.org/abstract/MED/17126461 (accessed 2026-11-16)
7. Lencioni T, Carpinella I, Rabuffetti M, et al. Human kinematic, kinetic and EMG data during different walking and stair ascending and descending tasks. Sci Data 2019;6:309. doi:10.1038/s41597-019-0323-z.
   CC BY 4.0. https://pmc.ncbi.nlm.nih.gov/articles/PMC6897988/ (not used for numbers) (accessed 2026-11-16)
8. Koivunen K, Laakkonen EK, Ahtiainen JP, Walker S. Effect of age and sex on lower extremity power production capacity throughout the lifespan based on 30 217 Finnish participant data. Scand J Med Sci Sports 2026;36(6):e70322. doi:10.1111/sms.70322.
   CC BY 4.0. https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ (accessed 2026-11-16)
9. Viau A, Feldman AG, McFadyen BJ, Levin MF. Reaching in reality and virtual reality: a comparison of movement kinematics in healthy subjects and in adults with hemiparesis. J NeuroEng Rehabil 2004;1:11. doi:10.1186/1743-0003-1-11.
   CC BY 2.0. https://pmc.ncbi.nlm.nih.gov/articles/PMC546398/ (accessed 2026-11-16)
10. Huang TW, Kuo AD. Mechanics and energetics of load carriage during human walking. J Exp Biol 2014;217(4):605–613. doi:10.1242/jeb.091587.
    © Company of Biologists; free to read on PMC, no CC licence. https://pmc.ncbi.nlm.nih.gov/articles/PMC3922835/ (accessed 2026-11-16)
11. Liew B, Morris S, Netto K. The effect of backpack carriage on the biomechanics of walking: a systematic review and preliminary meta-analysis. J Appl Biomech 2016;32(6):614–629. doi:10.1123/jab.2015-0339.
    Publisher ©; abstract only. https://doi.org/10.1123/jab.2015-0339 ; https://europepmc.org/abstract/MED/27705050 (accessed 2026-11-16)
12. Choi WJ, Wakeling JM, Robinovitch SN. Kinematic analysis of video-captured falls experienced by older adults in long-term care. J Biomech 2015;48(6):911–920. doi:10.1016/j.jbiomech.2015.02.025.
    Publisher ©; abstract only. https://doi.org/10.1016/j.jbiomech.2015.02.025 ; https://europepmc.org/abstract/MED/25769730 (accessed 2026-11-16)
13. Vlaeyen E, Deschodt M, Debard G, et al. Fall incidents unraveled: a series of 26 video-based real-life fall events in three frail older persons. BMC Geriatr 2013;13:103. doi:10.1186/1471-2318-13-103.
    CC BY 2.0. https://pmc.ncbi.nlm.nih.gov/articles/PMC3850536/ (accessed 2026-11-16)

## Not found

- Absolute peak hip, knee and ankle angles for stair ascent and descent in healthy adults, from an open-access source. Protopapadaki 2007 gives only ascent-minus-descent differences in its abstract, and the full text is paywalled.
- Natural-pace stair step time. Only maximum-speed 6-step times were found (ST1–ST4).
- CMJ joint kinematics (countermovement depth, peak knee and hip flexion, flight time distributions). Only jump height and power by age and sex were found.
- An SD for the Li 2021 natural STS time (1.60 s). It is not in the text that was read.
- STS joint angles for healthy young adults with SD. Hayamizu gives older women only. The Greve 2013 joint-angle figures are graphs, not tables.
- Absolute joint angles while carrying, and how trunk lean changes with backpack mass. Liew 2016 gives directions only in its abstract; the full text is paywalled.
- Carrying in the hands or arms (boxes, shopping bags). Not covered.
- Turning while walking (as opposed to 180° on the spot): turn angle vs. duration. Not found in a usable open table.
- Lab or young-adult fall durations and joint ranges (e.g., trunk angle at impact). Only real-life older-adult falls were found.
- Reaching at different distances or heights, standing reach, overhead reach. Viau 2004 is one seated task with n=8.

## Sources disagree

- Fall duration: Choi 2015 gives 1271 ± 648 ms from loss of balance to pelvis impact (25 falls, long-term care). Vlaeyen 2013 gives 1946 ± 639 ms for the "critical phase" (26 falls, 3 people). The two papers define the phase start and end differently, and Vlaeyen's sample is only 3 people. Use Choi for the impact timing; treat Vlaeyen as an upper bound. [https://doi.org/10.1016/j.jbiomech.2015.02.025 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC3850536/ · accessed 2026-11-16]
- STS duration: Li 2021 gives 1.60 s (30 young men, natural speed); Greve 2013 gives 1.71 ± 0.17 s (young); Hayamizu 2025 gives 1.80 ± 0.33 s (older women). Each uses a different start/end definition and a different seat height. A 1.5–2.1 s single-rise band covers all three. [https://pmc.ncbi.nlm.nih.gov/articles/PMC3808394/ ; https://pmc.ncbi.nlm.nih.gov/articles/PMC8183776/ ; https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ · accessed 2026-11-16]
- Greve 2013: Methods say 15 young (23.8±2.2 y) and 11 older (76±5.1 y) adults were enrolled, but Table 1 gives 24.3±2 and 76.4±5.2 y. The analysed n is not stated in the tables. [https://pmc.ncbi.nlm.nih.gov/articles/PMC3808394/ · accessed 2026-11-16]
- Khobkhun 2021: Table 1 and Table 2 give different young-group head-onset values (Table 1: 0.53/0.54/0.52 s; Table 2: 0.53/0.53/0.53 s). Table 1 is used above. [https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ · accessed 2026-11-16]
- Khobkhun 2021: the young group's step frequency at "slow" (3.26) is higher than at "moderate" (3.10), while the older group decreases monotonically. The row's unit is "(n)", which is unclear, so it is left out of this file. [https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ · accessed 2026-11-16]
