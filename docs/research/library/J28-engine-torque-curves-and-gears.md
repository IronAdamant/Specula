# J28 Engine torque curves, power, gear ratios and final drive

Fixed after the second check (2026-11-18): the ORNL Table 2.1 page is printed p. 6 (PDF p. 15), not p. 5; the licence note no longer says the J28-data CSV files carry the EPA citation (they have only a header line; how to cite them is given in the licence note, and the CSV files themselves were not edited); added from the checker: Fig. 3.11's torque axis stops at 200 Nm (under Sources disagree), the four Ford steady-state bins the note skipped, and the source's "(disputed)" flag on two motor powers. No value the checker verified was changed.

Round 4, gap 1. Petrol (naturally aspirated and turbo), diesel and electric motor, plus one full set of gear ratios with final drive. Every number below was read from the opened file named in its row. "Computed by me" marks my arithmetic. Power is always computed by me as P = T x omega = T x rpm x 2 pi / 60 (kW = Nm x rpm / 9549.3).

Licence notes (read from the sources, plus my reading where marked):
- EPA NVFEL test data packages: US federal government work. The packages carry a "SUGGESTED CITATION" line (xlsx row 2 or 3) but no licence text. My reading: US government works are public domain in the US (17 USC 105). Facts retyped here; the raw point lists are re-exported in J28-data/. Correction (second check, 2026-11-18): the three CSV files carry no citation line, only the header `speed_rpm,torque_Nm,power_kW_computed_by_me`. Until one is added, cite each CSV with its package as named in sections 1 to 3 and in Sources 1 to 3 below (the package's "SUGGESTED CITATION" line in its xlsx).
- SAE 2020-01-1286 (Moskalik, Stuhldreher, Kargul, all US EPA): hosted on epa.gov, but the PDF is an SAE International paper (footer "Downloaded from SAE International"). Facts only (gear ratios); table not copied.
- ORNL/TM-2010/253: Oak Ridge National Laboratory report for US DOE. No open licence stated in what I read. Facts only; figure not copied.

## 1. Petrol, naturally aspirated: 2018 Toyota 2.5 L A25A-FKS (Camry)

Source: EPA NVFEL, "2018 Toyota 2.5L A25A-FKS Engine Tier 2 Fuel - Test Data Package. Version 2020-07", 2020. Zip: https://www.epa.gov/sites/default/files/2020-08/2018-toyota-2.5l-a25a-fks-engine-tier2-fuel-test-data-package-dated-07-28-20.zip

Maker rating (report Table 1 in the package .docx): rated power 151 kW @ 6600 rpm; rated torque 249 Nm @ 4800 rpm. Features listed: direct and port injection, Atkinson cycle, cooled EGR.

Method (package report, "High Load" section, paraphrased): the engine is run at high loads "near and including wide-open throttle (WOT)"; the "initial" high-load interval "captures the region of maximum torque within 2 seconds" after the step. So the "High Load Initial" sheet is the nearest thing to a measured full-load curve.

Full-load torque, xlsx sheet "High Load Initial", columns Speed (rpm) and Torque (Nm). n = repeat points at that speed; spread = lowest to highest point. Power computed by me from the highest torque and the mean speed.

| Speed (rpm) | n | Torque low (Nm) | Torque high (Nm) | Power at high torque (kW, computed by me) |
|---|---|---|---|---|
| 1003-1004 | 3 | 149.4 | 162.5 | 17.1 |
| 1155-1157 | 3 | 175.0 | 179.8 | 21.8 |
| 1327-1329 | 3 | 177.4 | 189.7 | 26.4 |
| 1499 | 1 | 198.2 | 198.2 | 31.1 |
| 1728-1729 | 3 | 192.2 | 204.9 | 37.1 |
| 2018-2022 | 3 | 208.3 | 214.4 | 45.3 |
| 2535-2536 | 3 | 216.5 | 225.5 | 59.9 |
| 2996-2997 | 3 | 213.7 | 220.3 | 69.1 |
| 3572-3573 | 3 | 224.2 | 228.9 | 85.6 |
| 4034-4037 | 3 | 229.6 | 235.2 | 99.4 |
| 4493-4495 | 2 | 233.9 | 236.9 | 111.5 |
| 4955-4958 | 3 | 234.5 | 237.5 | 123.2 |
| 5415-5417 | 3 | 226.7 | 230.4 | 130.7 |
| 5761 | 1 | 226.2 | 226.2 | 136.5 |

The test stops at about 5760 rpm, below the 6600 rpm power peak, so the top of the power curve is not measured. Shape: torque rises steeply to about 2000 rpm, then a broad plateau of about 215 to 237 Nm from 2500 to 5400 rpm.

## 2. Petrol, turbocharged: 2013 Ford 1.6 L EcoBoost (Escape)

Source: EPA NVFEL, "2013 Ford 1.6L EcoBoost Engine Tier 2 Fuel - Test Data Package. Version 2018-10", 2018. Zip: https://www.epa.gov/sites/default/files/2018-12/2013-ford-1.6l-ecoboost-engine-tier-2-fuel-test-data-package-10-25-18.zip

Maker rating (package report Table 1): 180 hp @ 5700 rpm; 240 Nm @ 1600-5000 rpm; turbocharged, spray-guided direct injection.

Sheet "High Load Initial" (one point per speed, so no spread available):

| Speed (rpm) | Torque (Nm) | Power (kW, computed by me) |
|---|---|---|
| 1501 | 197.8 | 31.1 |
| 1999 | 243.3 | 50.9 |
| 2251 | 237.4 | 55.9 |
| 2499 | 231.8 | 60.7 |
| 2750 | 225.3 | 64.9 |
| 3000 | 227.4 | 71.4 |
| 3251 | 222.7 | 75.8 |
| 3501 | 216.1 | 79.2 |

Highest steady-state torque seen per 250 rpm bin (sheet "Steady State", computed by me as the bin maximum, so these are part-load map points, not a WOT curve): 1000: 119; 1250: 158; 1500: 197; 1750: 206; 2000: 218; 2500: 219; 3000: 215; 3500: 194; 4000: 176; 4500: 170 Nm. The second check (2026-11-18) found these are 10 of the 14 nearest-250 bins; the skipped ones are 2250: 218, 2750: 208, 3250: 169 and 3750: 156 Nm, and the 3250 and 3750 dips show that the steady-state map is not monotonic. Above 3500 rpm only these steady-state points exist, so the turbo engine's high-rpm full-load torque is Not found in this package.

## 3. Diesel: 2015 BMW 3.0 L N57 (X5 xDrive35d)

Source: EPA NVFEL, "2015 BMW 3.0L N57 Engine Diesel Fuel - Test Data Package. Version 2018-06", 2018 (citation text read from xlsx row 2). Zip: https://www.epa.gov/sites/default/files/2018-12/2015-bmw-3-0-l-n57-engine-diesel-fuel-test-data-package-06-11-18.zip

Maker rating (package report Table 1): 3.0 L DOHC 24-valve I-6 diesel, 2993 cc; 255 hp @ 4000 rpm (190 kW); 413 lb-ft @ 1500 rpm (560 Nm); single variable-geometry turbo, intercooled.

The report says mapping points were taken in 1 bar BMEP steps (2 bar above 2500 rpm) and "a WOT curve was generated from this map". The WOT curve is not a separate sheet, so I took the highest measured torque (column M_TRQENG, Nm) in each 250 rpm bin of column Speed (rpm), sheet "Engine Mapping". Bin maxima and power are computed by me.

| Speed bin (rpm) | Points in bin | Highest torque (Nm) | Power (kW, computed by me) |
|---|---|---|---|
| 1000 | 13 | 311.9 | 32.7 |
| 1250 | 18 | 454.4 | 59.5 |
| 1500 | 20 | 550.0 | 86.4 |
| 1750 | 22 | 570.2 | 104.5 |
| 2000 | 21 | 574.7 | 120.4 |
| 2250 | 22 | 570.1 | 134.3 |
| 2500 | 11 | 570.6 | 149.4 |
| 2750 | 12 | 570.8 | 164.4 |
| 3000 | 11 | 571.0 | 179.4 |
| 3250 | 12 | 542.8 | 184.7 |
| 3500 | 10 | 515.6 | 189.0 |
| 3750 | 9 | 485.0 | 190.4 |
| 4000 | 9 | 442.3 | 185.3 |
| 4250 | 8 | 388.8 | 173.0 |
| 4500 | 7 | 361.3 | 170.2 |

Check (computed by me): the bin-maximum power peaks at 190.4 kW near 3750 rpm, against the 190 kW @ 4000 rpm rating. The flat 570 Nm plateau from 1750 to 3000 rpm is the classic diesel shape.

## 4. Electric motor: 2010 Toyota Prius traction motor (ORNL bench test)

Source: T. A. Burress, S. L. Campbell, C. L. Coomer, C. W. Ayers, A. A. Wereszczak, J. P. Cunningham, L. D. Marlino, L. E. Seiber, H. T. Lin (ORNL), "Evaluation of the 2010 Toyota Prius Hybrid Synergy Drive System", ORNL/TM-2010/253, March 2011 (author list read from the report title page). OSTI 1007833 (https://www.osti.gov/biblio/1007833). The OSTI download link returned HTTP 500; the PDF was opened from https://info.ornl.gov/sites/publications/files/Pub26762.pdf (its cover matches ORNL/TM-2010/253).

Published specs, Table 2.1 (printed p. 6, PDF p. 15; 2010 Prius column): motor peak power 60 kW; motor peak torque 207 Nm; speed rating 13,500 rpm; separate generator 42 kW. Same table, other columns: 2008 LS 600h 300 Nm, 10,230 rpm; Hybrid Camry 270 Nm, 14,000 rpm; 2004 Prius 400 Nm, 6,000 rpm, 50 kW @ 1200-1540 rpm. The LS 600h and Camry motor powers are left out; the source marks both "(disputed)" (second check, 2026-11-18), so keep that flag if they are ever added.

Measured: "It was verified that the 2010 Prius motor is capable of producing 60 kW for 18 seconds prior to reaching a stator temperature of 150°C" (printed p. 63, PDF p. 72).

Peak torque/power line, Fig. 3.11 (650 Vdc, printed p. 63), READ OFF THE FIGURE BY ME, about +/-5 Nm and +/-250 rpm:

| Speed (rpm) | Peak torque (Nm, read off figure) | Power (kW, computed by me) |
|---|---|---|
| 0-2000 | about 200 (flat) | 0-42 |
| 3000 | about 190 | about 60 |
| 6000 | about 78 | about 49 |
| 8000 | about 50 | about 42 |
| 13,000 | about 25 | about 34 |

Shape: constant torque up to the base speed (about 2000 to 3000 rpm), then falling torque. In this figure the curve falls faster than constant 60 kW above about 3000 rpm (60 kW at 6000 rpm would be 95 Nm, computed by me). The figure's 200 Nm plateau is a little below the 207 Nm published rating.

## 5. Gear ratios and final drive: Toyota UB80E 8-speed (2018 Camry)

Source: Moskalik, A., Stuhldreher, M., Kargul, J., "Benchmarking a 2018 Toyota Camry UB80E Eight-Speed Automatic Transmission", SAE Technical Paper 2020-01-1286, 2020, doi:10.4271/2020-01-1286. PDF on epa.gov: https://www.epa.gov/sites/default/files/2020-05/documents/sae-2020-01-1286-benchmark-2018-toyota-camry-ub80e-eight-speed-auto-trans.pdf. Table 2 (ratios, which the paper cites to its ref [14]) and Table 6 (comparison).

| Gear | UB80E | Chrysler 845RE | GM 6T40 |
|---|---|---|---|
| 1 | 5.250 | 4.717 | 4.584 |
| 2 | 3.028 | 3.143 | 2.964 |
| 3 | 1.950 | 2.106 | 1.912 |
| 4 | 1.456 | 1.667 | 1.446 |
| 5 | 1.220 | 1.285 | 1.000 |
| 6 | 1.000 | 1.000 | 0.746 |
| 7 | 0.808 | 0.893 | - |
| 8 | 0.673 | 0.667 | - |
| Reverse | 4.014 | 3.295 | 2.940 |
| Differential (final drive) | 2.802 | - | 2.89 |
| Torque capacity | 280 Nm | 450 Nm | 240 Nm |

The UB80E sits behind the A25A-FKS engine in section 1 (paper Table 1), so sections 1 and 5 make a matched engine plus gearbox pair.

Computed by me: overall ratio in 1st = 5.250 x 2.802 = 14.71; in 8th = 0.673 x 2.802 = 1.886; ratio spread 5.250 / 0.673 = 7.80.

## Sources disagree

- Toyota A25A: rated torque 249 Nm @ 4800 rpm (package Table 1) vs highest measured "High Load Initial" point 237.5 Nm @ about 4957 rpm. That is 4.6% lower (computed by me); the test cell, fuel and the 2-second capture window may explain it.
- Ford 1.6 EcoBoost: rating says 240 Nm flat from 1600 to 5000 rpm; measured high-load points fall from 243.3 Nm (1999 rpm) to 216.1 Nm (3501 rpm).
- Prius motor: published 207 Nm (Table 2.1) vs about 200 Nm on the Fig. 3.11 peak line (my read-off). Second check (2026-11-18): Fig. 3.11's torque axis stops at 200 Nm, and the peak line runs along that top edge from 0 to about 2,200 rpm, so the 200 Nm may be only the axis limit; treat the published 207 Nm as the plateau.

## Not found

- An open measured full-load curve for a battery-electric car motor (Leaf, Bolt, Model 3). OSTI title search for "2012 Nissan LEAF" returned nothing; I did not open other ORNL reports.
- Ford 1.6 EcoBoost high-load points above 3500 rpm.
- Toyota A25A points above 5761 rpm (the 6600 rpm power peak is not measured).
- Measured diesel WOT curve as a separate table: built from map maxima instead (section 3).
- Manual gearbox ratios from an opened source.

## Sources

1. US EPA NVFEL, 2018 Toyota 2.5L A25A-FKS Engine Tier 2 Fuel Test Data Package, Version 2020-07 (xlsx sheets "High Load Initial", "Steady State"; report Table 1). https://www.epa.gov/sites/default/files/2020-08/2018-toyota-2.5l-a25a-fks-engine-tier2-fuel-test-data-package-dated-07-28-20.zip
2. US EPA NVFEL, 2013 Ford 1.6L EcoBoost Engine Tier 2 Fuel Test Data Package, Version 2018-10. https://www.epa.gov/sites/default/files/2018-12/2013-ford-1.6l-ecoboost-engine-tier-2-fuel-test-data-package-10-25-18.zip
3. US EPA NVFEL, 2015 BMW 3.0L N57 Engine Diesel Fuel Test Data Package, Version 2018-06 (sheet "Engine Mapping"). https://www.epa.gov/sites/default/files/2018-12/2015-bmw-3-0-l-n57-engine-diesel-fuel-test-data-package-06-11-18.zip
4. EPA benchmarking index page: https://www.epa.gov/vehicle-and-fuel-emissions-testing/benchmarking-advanced-low-emission-light-duty-vehicle-technology
5. Moskalik, Stuhldreher, Kargul, SAE 2020-01-1286 (Tables 1, 2, 6). https://www.epa.gov/sites/default/files/2020-05/documents/sae-2020-01-1286-benchmark-2018-toyota-camry-ub80e-eight-speed-auto-trans.pdf
6. Burress et al., ORNL/TM-2010/253, Evaluation of the 2010 Toyota Prius Hybrid Synergy Drive System, 2011 (Table 2.1 printed p. 6, PDF p. 15; Fig. 3.11 and text p. 63). https://www.osti.gov/biblio/1007833 ; https://info.ornl.gov/sites/publications/files/Pub26762.pdf

Data files in J28-data/: toyota-high-load-initial-points.csv, ford-high-load-initial-points.csv, bmw-engine-mapping-points.csv (speed, torque, power computed by me).
