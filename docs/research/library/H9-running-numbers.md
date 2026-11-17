# H8: Running: stride, cadence, joint angles, torques and work at three speeds

Researched 2026-11-17 (all URLs accessed 2026-11-17). Brief section 3, "Motion" (walk and run cycles). Extends round-two H7 (walking). Walk-cycle picture sheets are listed as a gap in README.

## H8-1. Treadmill running at 2.5, 3.5 and 4.5 m/s

Source: Fukuchi R.K., Fukuchi C.A., Duarte M. (2017), "A public dataset of running biomechanics and the effects of running speed on lower extremity kinematics and kinetics", *PeerJ* 5:e3298, doi:10.7717/peerj.3298, licence CC BY 4.0. Values retyped from Table 4 (means). https://peerj.com/articles/3298/

What was measured: 28 recreational runners on an instrumented treadmill, 3D motion capture and force plates, means across runners. Angles in degrees (flexion positive, extension negative); torques in Nm per kg body mass; work in J per kg.

| Variable (Table 4) | 2.5 m/s | 3.5 m/s | 4.5 m/s |
|---|---|---|---|
| Stride length (m) | 1.86 | 2.46 | 2.96 |
| Cadence (strides per minute) | 80.82 | 85.68 | 91.74 |
| Max hip flexion (°) | 43.75 | 52.76 | 60.50 |
| Max hip extension (°) | −3.58 | −7.95 | −11.75 |
| Max knee flexion (°) | 93.52 | 108.68 | 119.12 |
| Max ankle dorsiflexion (°) | about 26.4 to 26.8 at all speeds | | |
| Max ankle plantarflexion (°) | −16.62 | −20.47 | −23.17 |
| Max ankle eversion (°) | −4.91 | −6.59 | −7.81 |
| Peak hip flexion torque (Nm/kg) | 0.78 | 1.15 | 1.49 |
| Peak knee extension torque (Nm/kg) | 2.84 | 3.18 | 3.41 |
| Knee negative work (J/kg) | −1.50 | −2.15 | −3.03 |
| Vertical ground reaction impulse (Ns/kg) | 5.04 | 5.12 | 5.19 |

Computed by me: stride time = 60 / cadence = 0.742 s, 0.700 s, 0.654 s; steps per minute = 2 × cadence = 162, 171, 183. Stride length × strides per second reproduces the belt speed to within 1% (1.86 × 80.82 / 60 = 2.51 m/s; 2.46 × 85.68 / 60 = 3.51; 2.96 × 91.74 / 60 = 4.53).

Animation cue (computed by me from the table): going from 2.5 to 4.5 m/s, runners lengthen the stride by 59% but raise cadence by only 14%; peak knee flexion in swing grows by about 26°.

## Sources disagree

- None within this file. Round-two H7 walking cadences are in steps, these are strides (one stride = two steps).

## Not found

- Full joint-angle curves over the gait cycle: the dataset (figshare, normalised to 101 points) returned HTTP 403 to the box; only Table 4 summary values were used.
- Arm swing and trunk rotation amplitudes in running (not in Table 4).
- Sprint (over 7 m/s) joint data.
