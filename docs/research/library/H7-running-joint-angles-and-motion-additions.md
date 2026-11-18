# H7: Running joint angles by speed, and what part 2 adds to H1, C2, B3, H5

Second check (2026-11-18): clean (24 of 24 rows verified; Table H7-1 matches Fukuchi et al. 2017, Table 4, exactly). Small additions:
- Table H7-1 is a selection from the paper's Table 4: it leaves out the maximum ankle dorsiflexion (DF) torque, the work rows and the anterior-posterior impulses. Do not treat it as the full Table 4.
- The pointer to J6-3 (Dorn et al. 2012) now gives the runners per speed (9, 9, 8, 7), as the second check of J6 corrected.
- The paper's full text is also on Europe PMC (PMC5426356), where the article is CC BY. Added a Sources list. The checker's summary is in `H7-running-joint-angles-and-motion-additions.md.verify.md`.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Part 2 of the round-two brief ("motion as numbers"). Most of part 2 already exists in the library; this file adds only what is new and points to the rest.

## Already in the library (not repeated here)

| Need from the brief | Where it already is |
|---|---|
| Walking joint-angle curves through the gait cycle, by age and sex | H1 section 2 (Senden et al. 2024, OSF t72cw, 246 adults, three treadmill speeds) |
| Pelvis (tilt, obliquity, rotation) and trunk (tilt, lateral flexion, rotation) curves through the walk | H1 section 2 (Senden 2024 sheets `Rotation_Pelvic*` and `Rotation_Trunk*`) |
| Shoulder and elbow swing in walking | H1 section 5, ranges only; H1 itself flags both sources as weak (one is CC BY-NC) |
| Stride length and cadence against walking speed; walk-run transition | H1 sections 3 and 4 |
| Running peaks (earlier sources) | H1 |
| Which foot is down when, for quadruped gaits (phase and duty factor), transition speeds, stride scaling | C2 sections 1 to 3 |
| Bird gait and wingbeat | C3 |
| Facial muscles to expressions (FACS action units and their muscles, basic-expression AU table, magnitudes, timing) | B3 tables 1 to 5 |
| Face-expression pictures | `images/3-head` (Darwin 1872 with Bell's face-muscle figure, Le Brun, Duchenne) |
| Recorded motion whose licence allows reuse | H5, plus the new `motion-datasets.md` beside this library folder |

## Table H7-1. Running: peak joint angles and stride by speed (read directly)

Source: Fukuchi RK, Fukuchi CA, Duarte M (2017) "A public dataset of running biomechanics and the effects of running speed on lower extremity kinematics and kinetics", *PeerJ* 5:e3298, Table 4, https://peerj.com/articles/3298/ (full text also on Europe PMC, PMC5426356, https://europepmc.org/article/PMC/PMC5426356, where the article is CC BY; read there by the second check because peerj.com refuses automated fetches). The table below is a selection from Table 4: it leaves out the maximum ankle dorsiflexion (DF) torque, the work rows and the anterior-posterior impulses. The dataset itself (figshare DOI 10.6084/m9.figshare.4543435) is stated in the paper to be "made available under the CC-BY license (https://creativecommons.org/licenses/by/4.0/)".

Population and speed: 28 regular runners (weekly mileage over 20 km, 10-km race pace of 5 min/km or faster), instrumented dual-belt treadmill, standard neutral shoes, 2.5, 3.5 and 4.5 m/s. Sex and age split were not in the text read. Sign convention as in the paper: flexion and dorsiflexion positive; extension, plantarflexion and eversion negative. Values are mean ± SD.

| Variable | 2.5 m/s | 3.5 m/s | 4.5 m/s |
|---|---|---|---|
| Stride length (m) | 1.86 ± 0.11 | 2.46 ± 0.15 | 2.96 ± 0.20 |
| Cadence (strides per minute) | 80.82 ± 4.63 | 85.68 ± 5.27 | 91.74 ± 6.69 |
| Stride width (m) | 0.10 ± 0.02 | 0.09 ± 0.02 | 0.08 ± 0.02 |
| Max hip flexion (°) | 43.75 ± 6.06 | 52.76 ± 5.75 | 60.50 ± 6.06 |
| Max hip extension (°) | −3.58 ± 4.85 | −7.95 ± 4.58 | −11.75 ± 4.78 |
| Max knee flexion (°) | 93.52 ± 10.36 | 108.68 ± 10.65 | 119.12 ± 10.37 |
| Max ankle dorsiflexion (°) | 26.36 ± 2.93 | 26.54 ± 2.49 | 26.79 ± 2.51 |
| Max ankle plantarflexion (°) | −16.62 ± 5.50 | −20.47 ± 4.71 | −23.17 ± 4.72 |
| Max eversion (°) | −4.91 ± 2.74 | −6.59 ± 2.99 | −7.81 ± 3.59 |
| Max hip flexor torque (Nm/kg) | 0.78 ± 0.11 | 1.15 ± 0.14 | 1.49 ± 0.19 |
| Max hip extensor torque (Nm/kg) | −1.06 ± 0.14 | −1.37 ± 0.19 | −1.67 ± 0.21 |
| Max knee extensor torque (Nm/kg) | 2.84 ± 0.45 | 3.18 ± 0.50 | 3.41 ± 0.47 |
| Max ankle plantarflexor torque (Nm/kg) | 2.03 ± 0.22 | 2.23 ± 0.23 | 2.34 ± 0.25 |
| Vertical GRF impulse (Ns/kg) | 5.04 ± 0.09 | 5.12 ± 0.10 | 5.19 ± 0.10 |

Foot strike at 2.5, 3.5 and 4.5 m/s (same paper, Results): rearfoot 68%, 68%, 61%; midfoot 14%, 18%, 21%; forefoot 18%, 14%, 18%. Speed did not change foot-strike pattern significantly.

**Use for tests:** the full time-normalised curves (0 to 100% of the cycle, hip, knee and ankle in 3D) are in the figshare processed files (the paper's Table 3 describes the columns). Those curves are what a test can hold the engine's running motion against.

## Faster running

See J6-3 (Dorn et al. 2012: 9 runners, of whom 9, 9, 8 and 7 ran the four target speeds; 3.5 to 9.0 m/s; peak vertical force 2.7 to 3.6 body weights; strategy shift at about 7 m/s).

## Not found

- The running curves themselves: figshare (api.figshare.com and figshare.com) returns HTTP 403 to the box, so the processed curve file was not downloaded. Claude or the user can fetch it from a normal browser: https://doi.org/10.6084/m9.figshare.4543435.
- Joint-angle curves for sprinting (above 5 m/s) in an open dataset. Dorn 2012 has curves only as figures.
- Open shoulder and elbow joint-angle curves through walking and running from a strong, commercially usable source: still weak (see H1).
- Quadruped joint-angle curves (as opposed to footfall timing): not found in an open primary table in this pass.

## Sources

- Fukuchi RK, Fukuchi CA, Duarte M (2017), *PeerJ* 5:e3298, CC BY, https://peerj.com/articles/3298/ (accessed 2026-11-17); Europe PMC full text https://europepmc.org/article/PMC/PMC5426356 (opened by the second check, 2026-11-17).
- The paper's dataset on figshare, CC BY 4.0 (as the paper states), https://doi.org/10.6084/m9.figshare.4543435 (returned HTTP 403 to automated fetches, 2026-11-17; not downloaded).
- Dorn TW, Schache AG, Pandy MG (2012), *J Exp Biol* 215: 1944–1956, via J6-3, https://journals.biologists.com/jeb/article/215/11/1944/10883/Muscular-strategy-shift-in-human-running (accessed 2026-11-17).
- Senden et al. (2024), walking gait data, OSF t72cw, via H1, https://osf.io/t72cw/ (named here; read and dated in H1).
