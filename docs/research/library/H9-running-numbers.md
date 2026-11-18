# H8: Running: stride, cadence, joint angles, torques and work at three speeds

Fixed after the second check (2026-11-18):
- Every row of the table now carries its standard deviation (mean ± SD across runners), as Table 4 prints them; the ankle dorsiflexion row has its three values in place of "about 26.4 to 26.8".
- Three more rows of Table 4 added (stride width, peak hip extension torque, peak ankle plantarflexion torque; means only, as the checker relayed them).
- The runners are "regular runners" in the paper's words; the Europe PMC copy (PMC5426356) is linked, since peerj.com refuses scripted fetches; a "Sources" list added at the end.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Brief section 3, "Motion" (walk and run cycles). Extends round-two H7 (walking). Walk-cycle picture sheets are listed as a gap in README.

## H8-1. Treadmill running at 2.5, 3.5 and 4.5 m/s

Source: Fukuchi R.K., Fukuchi C.A., Duarte M. (2017), "A public dataset of running biomechanics and the effects of running speed on lower extremity kinematics and kinetics", *PeerJ* 5:e3298, doi:10.7717/peerj.3298, licence CC BY 4.0. Values retyped from Table 4 (mean ± SD). https://peerj.com/articles/3298/ (full text also at https://europepmc.org/article/PMC/PMC5426356)

What was measured: 28 regular runners (the paper's words; this file first said "recreational") on an instrumented treadmill, 3D motion capture and force plates, means and standard deviations across runners. Angles in degrees (flexion positive, extension negative); torques in Nm per kg body mass; work in J per kg.

| Variable (Table 4) | 2.5 m/s | 3.5 m/s | 4.5 m/s |
|---|---|---|---|
| Stride length (m) | 1.86 ± 0.11 | 2.46 ± 0.15 | 2.96 ± 0.20 |
| Cadence (strides per minute) | 80.82 ± 4.63 | 85.68 ± 5.27 | 91.74 ± 6.69 |
| Max hip flexion (°) | 43.75 ± 6.06 | 52.76 ± 5.75 | 60.50 ± 6.06 |
| Max hip extension (°) | −3.58 ± 4.85 | −7.95 ± 4.58 | −11.75 ± 4.78 |
| Max knee flexion (°) | 93.52 ± 10.36 | 108.68 ± 10.65 | 119.12 ± 10.37 |
| Max ankle dorsiflexion (°) | 26.36 ± 2.93 | 26.54 ± 2.49 | 26.79 ± 2.51 |
| Max ankle plantarflexion (°) | −16.62 ± 5.50 | −20.47 ± 4.71 | −23.17 ± 4.72 |
| Max ankle eversion (°) | −4.91 ± 2.74 | −6.59 ± 2.99 | −7.81 ± 3.59 |
| Peak hip flexion torque (Nm/kg) | 0.78 ± 0.11 | 1.15 ± 0.14 | 1.49 ± 0.19 |
| Peak knee extension torque (Nm/kg) | 2.84 ± 0.45 | 3.18 ± 0.50 | 3.41 ± 0.47 |
| Knee negative work (J/kg) | −1.50 ± 0.18 | −2.15 ± 0.24 | −3.03 ± 0.30 |
| Vertical ground reaction impulse (Ns/kg) | 5.04 ± 0.09 | 5.12 ± 0.10 | 5.19 ± 0.10 |
| Stride width (m) | 0.10 | 0.09 | 0.08 |
| Peak hip extension torque (Nm/kg) | −1.06 | −1.37 | −1.67 |
| Peak ankle plantarflexion torque (Nm/kg) | 2.03 | 2.23 | 2.34 |

The last three rows were added after the second check, as the checker relayed them from Table 4 (means only; their standard deviations are in the table and not retyped).

Computed by me: stride time = 60 / cadence = 0.742 s, 0.700 s, 0.654 s; steps per minute = 2 × cadence = 162, 171, 183. Stride length × strides per second reproduces the belt speed to within 1% (1.86 × 80.82 / 60 = 2.51 m/s; 2.46 × 85.68 / 60 = 3.51; 2.96 × 91.74 / 60 = 4.53).

Animation cue (computed by me from the table): going from 2.5 to 4.5 m/s, runners lengthen the stride by 59% but raise cadence by only 14%; peak knee flexion in swing grows by about 26°.

## Sources disagree

- None within this file. Round-two H7 walking cadences are in steps, these are strides (one stride = two steps).

## Not found

- Full joint-angle curves over the gait cycle: the dataset (figshare, normalised to 101 points) returned HTTP 403 to the box; only Table 4 summary values were used.
- Further rows of Table 4 not retyped: peak ankle dorsiflexion torque; hip, knee and ankle positive and negative work (other than the knee's negative work); braking and propulsive anterior-posterior impulses.
- Arm swing and trunk rotation amplitudes in running (not in Table 4).
- Sprint (over 7 m/s) joint data.

## Sources

- Fukuchi R.K., Fukuchi C.A., Duarte M. (2017), "A public dataset of running biomechanics and the effects of running speed on lower extremity kinematics and kinetics", *PeerJ* 5: e3298, doi:10.7717/peerj.3298. https://peerj.com/articles/3298/ ; https://europepmc.org/article/PMC/PMC5426356 . CC BY 4.0. Accessed 2026-11-17.
