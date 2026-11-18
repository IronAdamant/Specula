# H10: Full-cycle hip, knee and ankle curves for walking and running, and arm swing ranges

Delivered as H9 and renumbered H10 here, because the library already had an H9 (`H9-running-numbers.md`); its data folder `H9-data/` is `H10-data/` here.

Fixed after the second check (2026-11-18): the running stance knee peak is 43.4° at 14% of stride, not about 41° at 10%; Killeen's treadmill speed was 50% of each person's maximal overground speed, not a self-selected speed (also corrected under Sources disagree); Killeen's markers were Plug-in-Gait for the upper body and a modified Cleveland set for the lower body, not Plug-in Gait full-body; the Killeen paper's URL is added (Europe PMC PMC5319362, CC BY 4.0). Notes added from the checker: the paper's analysed cohort is 83 people, not the 85 the sheet gives; the paper's youngest age band is 18-39; the README's "sheet 3"; what the wrist path measures; where the 101 points come from; and that the CSV grade labels are percent grades.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Round-four gap 5. Extends round-three `H9-running-numbers.md` (peak values only) and round-two H7. Values marked "computed by me" are arithmetic on the dataset.

## H10-0. The figshare file asked for (doi 10.6084/m9.figshare.4543435)

Tried again from the box on 2026-11-17: api.figshare.com/v2/articles/4543435 (HTTP 403), /files (one 200 response, then 403 on every retry), /versions (403), ndownloader.figshare.com/articles/4543435 (403), figshare.com/ndownloader/articles/4543435/versions/5 (HTTP 202 with an empty body, meaning the archive is queued but never served). The figshare search API answers, so the block is on article and file endpoints. The PeerJ article's supplement points back to the same figshare record. **Still not obtained.** A substitute open dataset with full-cycle curves was found instead (H10-1).

## H10-1. Walking and running joint angles over the whole stride (Nuckols et al. 2020)

Source paper: Nuckols R.W., Takahashi K.Z., Farris D.J., Mizrachi S., Riemer R., Sawicki G.S. (2020), "Mechanics of walking and running up and downhill: A joint-level perspective to guide design of lower-limb exoskeletons", *PLoS ONE* 15(8): e0231996, doi:10.1371/journal.pone.0231996, CC BY 4.0 (Europe PMC PMC7454943).
Data: Dryad doi:10.5061/dryad.ns1rn8pqc, mirrored on Zenodo record 3986714 (https://zenodo.org/records/3986714), licence **CC0**. File Nuckols_PLoS_UpDown_Summary_Repo.mat (read on the box with SciPy).

What was measured (paper, Methods): 8 adults (6 men, 2 women) on an instrumented treadmill; walking at 1.25 m/s and running at 2.25 m/s; Vicon, 8 cameras, 120 Hz, 22 markers on the right leg and pelvis, 10 Hz low-pass filter; Visual3D. One stride = right heel strike to right heel strike, time-normalised to 101 points (0 to 100%). Grades: walking 0°, ±5.71°, ±8.53°; running 0°, ±2.86°, ±5.71°. The grade labels in the .mat file and in the H10-data CSV headers (up15, up10, up5, down5 and so on) are percent grades, not degrees: walk up15 = 8.53°, run up10 = 5.71° (second check, 2026-11-18; the paper confirms the mapping, the CSV headers do not say it).

Angles below are the group mean at level grade, in degrees, read from the file's Mn arrays. Sign in the file: hip flexion positive, knee flexion positive. The ankle in the file is a raw segment angle with an offset of about −70° and dorsiflexion more negative; the last column is **computed by me** as (value at 0%) minus (value now), so dorsiflexion from the heel-strike position is positive.

### Walking, 1.25 m/s, level

| % stride | Hip (°) | Knee (°) | Ankle, file value (°) | Ankle change from heel strike, dorsiflexion + (°, computed by me) |
|---|---|---|---|---|
| 0 | 22.5 | 7.0 | -68.4 | -0.0 |
| 5 | 21.1 | 12.9 | -61.5 | -6.8 |
| 10 | 18.7 | 19.6 | -64.2 | -4.1 |
| 15 | 13.6 | 20.2 | -68.6 | 0.3 |
| 20 | 8.6 | 18.0 | -71.3 | 2.9 |
| 25 | 3.8 | 15.1 | -73.1 | 4.7 |
| 30 | -0.9 | 12.4 | -74.9 | 6.5 |
| 35 | -5.0 | 10.3 | -76.6 | 8.3 |
| 40 | -8.6 | 9.3 | -78.7 | 10.3 |
| 45 | -11.8 | 10.6 | -80.7 | 12.3 |
| 50 | -13.6 | 16.0 | -81.6 | 13.2 |
| 55 | -13.5 | 25.4 | -77.6 | 9.2 |
| 60 | -9.3 | 40.8 | -63.8 | -4.6 |
| 65 | -1.0 | 57.1 | -49.9 | -18.5 |
| 70 | 8.0 | 66.5 | -55.5 | -12.9 |
| 75 | 16.1 | 66.5 | -61.2 | -7.2 |
| 80 | 21.7 | 58.0 | -65.8 | -2.6 |
| 85 | 24.5 | 41.7 | -67.3 | -1.0 |
| 90 | 24.2 | 19.7 | -67.1 | -1.3 |
| 95 | 22.3 | 2.4 | -68.0 | -0.4 |
| 100 | 22.6 | 5.6 | -68.8 | 0.5 |

Ranges (computed by me from the file): hip 38.6°, knee 66.3°, ankle 31.8°. Peak knee flexion 67.5° at 72% (swing); hip most extended −13.9° at 52%.

### Running, 2.25 m/s, level

| % stride | Hip (°) | Knee (°) | Ankle, file value (°) | Ankle change from heel strike, dorsiflexion + (°, computed by me) |
|---|---|---|---|---|
| 0 | 24.1 | 18.2 | -72.1 | -0.0 |
| 5 | 23.1 | 30.1 | -74.4 | 2.3 |
| 10 | 21.9 | 41.3 | -82.2 | 10.1 |
| 15 | 17.6 | 43.3 | -88.1 | 15.9 |
| 20 | 12.2 | 40.7 | -89.9 | 17.8 |
| 25 | 6.1 | 35.3 | -86.5 | 14.4 |
| 30 | 0.3 | 28.9 | -77.5 | 5.4 |
| 35 | -4.2 | 24.0 | -65.3 | -6.8 |
| 40 | -6.4 | 24.2 | -54.6 | -17.5 |
| 45 | -5.6 | 32.2 | -49.0 | -23.1 |
| 50 | -3.2 | 45.8 | -48.8 | -23.3 |
| 55 | -0.9 | 60.7 | -52.1 | -20.0 |
| 60 | 3.6 | 73.9 | -56.6 | -15.5 |
| 65 | 12.2 | 82.9 | -61.8 | -10.3 |
| 70 | 20.9 | 84.0 | -66.4 | -5.8 |
| 75 | 27.3 | 76.1 | -69.8 | -2.4 |
| 80 | 30.7 | 61.2 | -71.5 | -0.6 |
| 85 | 30.7 | 42.0 | -72.3 | 0.2 |
| 90 | 28.3 | 23.6 | -73.3 | 1.2 |
| 95 | 26.1 | 14.2 | -74.1 | 2.0 |
| 100 | 24.4 | 16.5 | -72.7 | 0.6 |

Ranges (computed by me): hip 37.6°, knee 70.8°, ankle 41.6°. Peak knee flexion 84.7° at 68%; stance knee flexion peak 43.4° at 14% of stride (corrected by the second check, 2026-11-18, from "about 41° at 10%"; 41.3° is only the value at 10%).

Peak joint moments at level grade (file Mom arrays, N·m per kg body mass, the paper's normalisation): walking hip −0.57 to 0.57, knee −0.30 to 0.62, ankle up to 1.63; running hip −0.59 to 0.83, knee −0.51 to 1.73, ankle up to 2.59. Average positive limb power at level: walking 1.02 W/kg, running 3.66 W/kg (paper, Fig 2 and Fig 3 captions).

**Full curves:** every grade, mean and SD at all 101 points, are in H10-data/ as four CSV files (walk and run; angle and moment), retyped by script from the CC0 file with no change except rounding to 3 decimals. The 101 points come from the data file; the paper itself does not state the point count (second check, 2026-11-18). The second check found the CSVs match the .mat Mn and SD arrays exactly at 3 decimals.

## H10-2. Arm swing in walking (Killeen et al. 2017)

Source paper: Killeen T., Easthope C.S., Filli L. et al. (2017), "Increasing cognitive load attenuates right arm swing in healthy human walking", *Royal Society Open Science* 4: 160993, doi:10.1098/rsos.160993, CC BY 4.0, full text via Europe PMC PMC5319362 (https://europepmc.org/article/PMC/PMC5319362). Data: Dryad doi:10.5061/dryad.2kd0b, Zenodo record 4977170 (https://zenodo.org/records/4977170), **CC0**, file final_for_dryad.xls (column names from the README in the record). The README says "Please use sheet 3", but the .xls has only one sheet (Sheet1, 363 rows = 121 trials × 3 conditions; second check, 2026-11-18).

What was measured: treadmill walking with the treadmill set to 50% of each person's maximal overground speed, taken from the mean of two timed 25-foot walks done at maximum speed (not a self-selected speed); markers: Plug-in-Gait for the upper body and a modified Cleveland marker set for the lower body (both corrected by the second check, 2026-11-18); baseline walking without a task (Condition 0); included cohort (README "STARTS" = 1), 85 people aged 21 to 79. Means (SD) **computed by me** from the sheet. The paper's own final analysis has **83** people (31 aged 18-39, 23 aged 40-59, 29 aged 60-80, per its Results and Table 1); the sheet filtered by STARTS = 1 gives 85 (31/25/29). The means below are correctly computed from the sheet but do not match the paper's n = 83, and the README's age bands (20-39 and so on) differ from the paper's 18-39 (second check, 2026-11-18).

| Variable (file column) | Left | Right | By age group 20-39 / 40-59 / 60-80 (right side, or left for shoulder) |
|---|---|---|---|
| Shoulder sagittal range of motion, Plug-in Gait X (°) | 31.5 (11.0) | 30.9 (11.9) | L: 32.2 / 32.4 / 30.0 |
| Elbow range of motion (°) | 27.2 (10.6) | 26.3 (11.5) | R: 24.1 / 26.4 / 28.5 |
| Peak elbow angular velocity (°/s) | 520 (155) | 520 (168) | R: 528 / 524 / 509 |
| Wrist 3D path per stride (mm) | 773 (206) | 768 (217) | R: 765 / 775 / 766 |
| Hip range of motion (°) | | 44.8 (4.0) | 45.4 / 44.2 / 44.8 |
| Knee range of motion (°) | | 61.7 (4.2) | 61.9 / 63.2 / 60.2 |
| Ankle range of motion (°) | | 30.1 (7.7) | 34.8 / 29.4 / 25.6 |
| Step length (mm) | | 587 (64) | 613 / 589 / 557 |
| Stride time (s) | | 1.10 (0.1) | 1.1 at all ages |

Group sizes: 31, 25, 29 (sheet; the paper's are 31, 23, 29). "Wrist 3D path per stride": the README defines the column only as "3d mm travelled"; the paper defines the arm-swing cycle by maximum protraction and retraction of the wrist joint centre, so "per stride" is a fair reading but is not stated in the README (second check, 2026-11-18). Arm swing is highly variable between people (SD about a third of the mean); older walkers show smaller ankle range and shorter steps but similar arm swing.

## Sources disagree

- Walking knee range: Nuckols 66.3° (8 people, 1.25 m/s) against Killeen 61.7° (85 people, treadmill at 50% of each person's maximal overground speed; corrected by the second check, 2026-11-18, from "self-selected speed"); peak swing knee flexion in both sits in the 60-68° band.
- Running peak knee flexion: Nuckols 84.7° at 2.25 m/s against Fukuchi (`H9-running-numbers.md`) 93.5° at 2.5 m/s; both rise with speed.

## Not found

- The figshare Fukuchi 2017 running curves (H10-0).
- Shoulder and elbow **curves** through the cycle for walking or running (Killeen gives ranges only; Nuckols tracked the right leg and pelvis only). Arm swing in running: no open dataset was read.
- Trunk and pelvis rotation curves.

## Sources

All accessed 2026-11-17 (as the header says), except the Killeen paper's Europe PMC URL, which was supplied by the second checker on 2026-11-18.

- Nuckols et al. (2020), *PLoS ONE* 15(8): e0231996, doi:10.1371/journal.pone.0231996, CC BY 4.0, full text via Europe PMC PMC7454943, https://europepmc.org/article/PMC/PMC7454943
- Nuckols et al. data, Dryad doi:10.5061/dryad.ns1rn8pqc, Zenodo record 3986714, CC0, https://zenodo.org/records/3986714
- Killeen et al. (2017), *R. Soc. Open Sci.* 4: 160993, doi:10.1098/rsos.160993, CC BY 4.0, full text via Europe PMC PMC5319362, https://europepmc.org/article/PMC/PMC5319362
- Killeen et al. data, Dryad doi:10.5061/dryad.2kd0b, Zenodo record 4977170, CC0, https://zenodo.org/records/4977170
- Fukuchi et al. (2017) figshare record (not obtained), https://doi.org/10.6084/m9.figshare.4543435
