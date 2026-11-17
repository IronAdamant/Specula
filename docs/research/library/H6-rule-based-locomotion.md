# H6: Locomotion made by rule instead of motion capture (2, 4 and many legs, and flying)

**Changelog**
- 2026-11-17 09:23 AEDT (second Library Verify pass, verify file 09:15): (1) Coros 2010 PD scaling corrected — kp scales **linearly** by M′/M, not √(M′/M); kd = 2·√kp unchanged (confirmed on the rendered p. 5 image). (2) Walknet rule 4 filled in from the 2013 review (Target-net sends the front leg's position to the leg behind as its swing target / AEP; confirmed in the Europe PMC full text of PMC3755227); the "Walknet rule 4" Not-found bullet is removed.
- 2026-11-17 (AEDT): per-row access dates added. Every table row now ends with a "Source URL · accessed" cell (or "Accessed" in the source list), and every numeric prose line or bullet carries its source URL and access date (2026-11-16, the day each was checked). No values or licence calls changed; Library Verify found 0 FIX rows in H6. M9 and M11 stay abstract-level, and "drive level" in M11 is a paraphrase, not a quoted term (as Library Verify noted).


Lane H6 of `LIBRARY_RESEARCH_BRIEF.md`. Written 2026-11-16 (AEDT). All URLs were accessed on 2026-11-16.
Each number below was read from the paper's own text or table on that date. Source IDs (M1, M2 …) refer to the full source list in §6.
These are methods, not populations, so "mean ± SD" rarely applies. Where a paper gives a working range (speed bands, gain ranges, step periods), it is given as min–max so it can seed a ranged generator.
The grok CLI was not signed in on the box, so no grok passes ran. Everything here comes from direct reading of the primary PDFs and pages.

Words used:
- **Physics**: a forward-dynamics simulation produces the motion, and the controller outputs joint torques.
- **Kinematic**: poses are computed directly, with no simulation of forces.
- **Offline optimisation**: the motion is solved ahead of time and then played back.

## 1. Survey table: what each method takes and gives

| ID | Method (year) | Legs | Input | Output | Physics or kinematic | Main limits (as stated by the authors) | Code / licence | Source URL · accessed |
|---|---|---|---|---|---|---|---|---|
| M1 | SIMBICON, Yin, Loken & van de Panne (2007) | 2 | A finite-state machine with 2–8 states per gait. Each state holds a target pose (torso, swing hip/knee/ankle, stance knee/ankle), a dwell time or foot-contact trigger, and 2 balance gains cd and cv. Optionally, mocap to build targets. | Joint torques from PD control. Swing-hip target = θd0 + cd·d + cv·v (d = horizontal COM-to-stance-ankle distance, v = COM velocity). | Physics: 2D own sim (planar, 9 DOF) and 3D ODE (34 DOF) | Gains still hand-tuned for mocap-derived controllers. Only walking styles were tested from mocap. Gaits not energy-optimal. No reaction-time delay. Stairs can trip it because it "cannot see" a step. Big flight-phase motions need exact take-off momentum, which the feedback does not give. | Java demo "JSimbicon.zip" on the project page. No licence file was found in the zip, so treat it as all rights reserved. | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · 2026-11-16 |
| M2 | Generalized Biped Walking Control, Coros, Beaudoin & van de Panne (2010) | 2 | Desired speed Vd, step period T, a few spline joint-angle curves over the step phase (all zero gives a working "zero gait"), swing-foot height curve, step width, and character proportions and mass. | Joint torques. These combine PD tracking, a Jacobian-transpose gravity compensation term, and a velocity-tuning virtual force. Foot placement comes from an inverted-pendulum model (IPM). | Physics: ODE, 37 DOF humanoid | The authors note it cannot author push-recovery style. Very long periods (T > 1 s) with medium–large steps are a problem. When the IPM step is out of reach, the step is capped and recovery takes several steps. No self-collision. | Paper says it ships as the open-source project "cartwheel-3d". The Google Code archive lists the licence as `asf20` (Apache 2.0). | https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf · 2026-11-16 |
| M3 | Locomotion Skills for Simulated Quadrupeds, Coros, Karpathy, Jones, Reveret & van de Panne (2011) | 4 | Gait graph (stance/swing timing per leg over the stride), stride period T, piecewise-linear trajectories (4–6 control points), foot-placement gain sfp, and a dog skeleton. Parameters are tuned offline by optimisation against video of a real dog. | Joint torques: PD plus virtual forces via Jacobian transpose, a dual "leg frame" model, and a flexible spine. | Physics: ODE at 1000 Hz | Simplified scapula and shoulder. No self-collision. Head and tail are not modelled in detail. Turning agility is far below real dogs. Box feet. Get-up was hard to design by hand. Slower than a real dog: trot 3.0 vs 3.3 m/s, gallop 5.7 vs 6.5 m/s. | Project page offers `binary.zip` only. No source licence was found. | https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · 2026-11-16 |
| M4 | Optimal Gait and Form for Animal Locomotion, Wampler & Popović (2009) | any (monoped, biped, raptor, horse, 5-legged) | Kinematic tree of cylinder limbs (length, radius, mass), with feet and head marked. Optionally lets limb proportions vary. No starting motion or foot timing is needed. | An optimal gait cycle (30 frames) with foot-contact timing, ground forces and joint torques, plus optimal limb proportions if asked. | Offline spacetime optimisation that is physically consistent. It is not a run-time controller. | Offline only, minutes per solve. Adding body-shape variables sometimes makes the solve fail on complex skeletons, which they handle by letting the outer CMA loop pick shape. Produces playback, not closed-loop control. | Paper only. No code was found. | https://grail.cs.washington.edu/wp-content/uploads/2015/08/wampler2009oga.pdf · 2026-11-16 |
| M5 | Spore animation system, Hecker, Raabe, Enslow, DeWeese, Maynard & van Prooijen (2008) | any number of legs, plus crawling and floating bodies | Player-made body graph, plus animations authored once in "semantic" form (goals relative to body parts), plus per-leg-group gait parameters keyed to speed. | Poses per frame: retargeted animation, a procedural gait (duty factor, step trigger offset, foot arc), a "Particle IK" solver, and "Jiggles"/"Wiggles" secondary motion. | Kinematic (IK; the secondary motion is pseudo-physics) | No sense of volume, so motions such as "hug" or "rub chin" cannot be expressed. No self-collision. IK singularities can twist limbs. Anti-buckling is crude. The gait has trouble anticipating sudden player input. | Shipped in a commercial game. No code released. | https://doi.org/10.1145/1360612.1360626 · 2026-11-16 |
| M6 | "An Indie Approach to Procedural Animation", Rosen, GDC 2014 (Overgrowth) | 2 (humanoid rabbit) | A physics sphere moved by the controller, plus a few key poses: 2 per locomotion cycle (pass and reach), walk, run and crouch-walk sets, and stand and crouch poses. 13 keyframes in total for the core moves. | Final poses from velocity-facing, acceleration tilt, "surveyor wheel" stride timing (distance travelled drives cycle phase), bicubic pose interpolation, spring-damper transitions, 2-bone IK, look-at IK and per-bone softness. | Kinematic on top of simple physics movement. Ragdoll is used only for hits and death. | No numbers were given for the spring stiffness or damping ("tweaked until it looked right"). The speaker advises against relying on IK for object holding. It is a talk, not a paper. | Overgrowth engine code is Apache-2.0 on GitHub (since 2022). Game art and data are not included and may not be redistributed without permission. | https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach ; https://reducer.xgoose.org/items/8812 · 2026-11-16 |
| M7 | Realistic Modeling of Bird Flight Animations, Wu & Popović (2003) | flying | Dynamic bird model (skeleton, masses, feather aerodynamics) and a 3D flight-path spline | Wingbeat-by-wingbeat joint motion and feather poses, found by optimising wingbeat parameters (Table 1) per beat through a simulation with simplified aerodynamics | Physics simulation inside offline optimisation | Offline: 218–324 min per full path. Skips controller design, so there is no run-time control. Feathers are massless. Aerodynamics are simplified. | Paper only. No code was found. | https://grail.cs.washington.edu/projects/flight/wu2003realistic.pdf · 2026-11-16 |
| M8 | Flexible Muscle-Based Locomotion for Bipedal Creatures, Geijtenbeek, van de Panne & van der Stappen (2013) | 2 (human and imaginary bipeds) | Skeleton plus rough muscle routing (attachment and via points), target speed and heading. Muscle layout and control parameters (29–132 per model) are tuned offline with CMA-ES. | Muscle excitations → Hill-type muscle forces → joint torques | Physics: ODE 0.8.2, step 0.0003 s | Human walking and running are of lower fidelity than Wang et al. 2012, especially the upper body. Optimisation takes 2–12 h. | Paper only. No code licence was found. | https://goatstream.com/research/papers/SA2013/SA2013.pdf · 2026-11-16 |
| M9 | Robust Physics-Based Locomotion Using Low-Dimensional Planning, Mordatch, de Lasa & Hertzmann (2010) | 2 | High-level user commands, plus a planner that uses a spring-loaded inverted pendulum (SLIP) for stance and projectile motion for flight | Full-body torques optimised each step to match the plan. Walking, running, jumping and the transitions between them emerge. | Physics with online planning | Abstract-level only: I could not open the full text (ACM DL blocks the box). | Paper only | https://history.siggraph.org/learning/robust-physics-based-locomotion-using-low-dimensional-planning-by-mordatch-de-lasa-and-hertzmann/ · 2026-11-16 |
| M10 | Walknet, Cruse et al. (1998); review by Schilling, Hoinville, Schmitz & Cruse (2013) | 6 (stick insect; also robots) | Per-leg controllers (swing net, stance net, target net), coupled by coordination rules 1–6 drawn from insect experiments. Inputs are joint angles and velocities, ground contact and leg load. | Leg joint motion. Tripod, tetrapod and wave gaits emerge with smooth transitions, with no fixed gait table. | Tested both kinematically and in dynamic simulation, and on robots | Rule 6 (treading-on-tarsus) has never been implemented. The review says the model's time scales differ from the insect's in the Graham-style comparison. | 2013 review is CC BY. No code licence was checked. | https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ · 2026-11-16 |
| M11 | Central pattern generators (CPGs), Ijspeert (2008 review) | any (also swimming) | Coupled oscillators or neural nets driven by a few simple low-dimensional inputs, such as drive level | Rhythmic multi-joint output signals | Either: a pattern generator feeding a sim or robot | Abstract-level only: the full text was not accessible (closed; the green-OA copy at EPFL returned 429/500). | Paper only | https://pubmed.ncbi.nlm.nih.gov/18555958/ · 2026-11-16 |
| M12 | Ambe, Aoi, Tsuchiya & Matsuno (2021), many-legged decentralised gaits | many (millipede and centipede-like) | One phase oscillator per leg, with local sensory feedback (phase resetting) | Direct-wave, retrograde-wave and "source-wave" gaits that emerge rather than being designed in | Physics (dynamic body model) | A simple model aimed at explaining mechanism, not an animation tool | CC BY | https://pmc.ncbi.nlm.nih.gov/articles/PMC8450536/ · 2026-11-16 |
| M13 | DeepLoco, Peng, Berseth, Yin & van de Panne (2017), shown for contrast | 2 | Terrain heightmap, goals, and a reference motion (one keyframed cycle or 7 s of mocap) | Torques from a learned 2-level policy | Physics, learned by reinforcement learning | Not rule-based: it needs training and a reference motion. Included only as the modern learned alternative. | Paper (author PDF) | https://www.cs.ubc.ca/~van/papers/2017-TOG-deepLoco/2017-TOG-deepLoco.pdf · 2026-11-16 |

## 2. Published parameter values (ranges for seeded generators)

### 2a. SIMBICON (M1): Table 1, planar biped. Angles in radians, times in s. "fc" = state ends on foot contact.

Columns: state, Δt (state dwell), cd, cv, torso, swing hip (swh), swing knee (swk), swing ankle (swa), stance knee (stk), stance ankle (sta).

| Gait | State | Δt | cd | cv | tor | swh | swk | swa | stk | sta | Source URL · accessed |
|---|---|---|---|---|---|---|---|---|---|---|---|
| walk | 0,2 | 0.30 | 0.00 | 0.20 | 0.0 | 0.40 | −1.10 | 0.20 | −0.05 | 0.20 | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · 2026-11-16 |
| walk | 1,3 | fc | 2.20 | 0.00 | 0.0 | −0.70 | −0.05 | 0.20 | −0.10 | 0.20 | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · 2026-11-16 |
| in-place walk | 0,2 | 0.30 | 0.00 | 0.40 | 0.0 | 0.62 | −1.10 | 0.20 | −0.05 | 0.20 | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · 2026-11-16 |
| in-place walk | 1,3 | fc | 1.55 | 0.00 | 0.0 | −0.10 | −0.05 | 0.20 | −0.10 | 0.20 | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · 2026-11-16 |
| fast walk | 0,2 | 0.27 | 0.00 | 0.20 | −0.1 | 0.73 | −1.83 | 0.20 | −0.05 | 0.20 | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · 2026-11-16 |
| fast walk | 1,3 | fc | 2.00 | 0.00 | −0.1 | −0.70 | −0.05 | 0.20 | −0.10 | −0.06 | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · 2026-11-16 |
| high-step walk | 0,2 | 0.30 | 0.00 | 0.20 | 0.0 | 1.00 | −2.40 | 0.20 | −0.05 | 0.20 | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · 2026-11-16 |
| high-step walk | 1,3 | fc | 2.00 | 0.00 | 0.0 | −0.70 | −0.05 | 0.20 | −0.10 | 0.20 | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · 2026-11-16 |
| crouch walk | 0,2 | 0.30 | 0.00 | 0.20 | −0.2 | 1.10 | −2.17 | 0.62 | −0.97 | 0.44 | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · 2026-11-16 |
| crouch walk | 1,3 | fc | 2.20 | 0.00 | −0.3 | −0.70 | −0.05 | 0.20 | −0.92 | 0.44 | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · 2026-11-16 |
| 3D walk (sagittal) | 0,2 | 0.3 | 0.5 | 0.2 | 0 | 0.5 | −1.1 | 0.6 | −0.05 | 0 | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · 2026-11-16 |
| 3D walk (lateral) | – | – | 0.5 | 0.2 | 0 | 0 | 0 | 0 | 0 | 0 | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · 2026-11-16 |

Ranges across all the planar walks in Table 1 (walk, in-place, fast, high-step, half-bent, bent, crouch, backward-leaning backward): [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · accessed 2026-11-16]
- Swing-phase dwell Δt: 0.22–0.30 s. [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · accessed 2026-11-16]
- cv in the first state: 0.20–0.40, with 0.28 for the backward walk. [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · accessed 2026-11-16]
- cd in the second state: 0.60–2.20. [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · accessed 2026-11-16]
- Swing-hip target in the first state: 0.37–1.10 rad. [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · accessed 2026-11-16]
- Swing-knee target: −2.40 to −1.10 rad. [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · accessed 2026-11-16]

The run, fast-run, skipping and scissor-hop rows are also in Table 1. I left them out because their row labels shifted in the text extraction (the caption says the runs have 2 states). Read them from the PDF directly if needed. [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · accessed 2026-11-16]

Other SIMBICON constants:
- **Planar model:** PD kp = 300 Nm/rad, kd = 30 Nms/rad. Ground springs 100 000 N/m, damping 6000 Ns/m. Friction 0.65. Time step 0.0001 s. Torque limit 1000 Nm (370 Nm suffices except for the fast run; the basic walk fails below 90 Nm). [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · accessed 2026-11-16]
- **3D model:** ODE 0.6, time step 0.005 s, friction 0.8. kp ≤ 300 (1000 at the waist), kd = 0.1·kp. [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · accessed 2026-11-16]
- **Robustness test:** unseen 20 cm step-downs and ±6° slopes; 10 pushes at 5 s intervals. The page figure shows recovery from a 350 N, 0.2 s push. [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · accessed 2026-11-16]
- **Feedback-error learning:** feed-forward torque capped at kp·0.2 rad. The residual ratio was about 2.5% at 0.5 ms phase bins, 3.5% at 5 ms and 14% at 25 ms. [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · accessed 2026-11-16]

### 2b. Generalized biped walking (M2)
- Step-placement offset from the inverted-pendulum model: d = v·√(h/g + v²/(4g²)), then d′ = d − α·Vd with α = 0.05 for all results. Applied separately in the sagittal and coronal planes. [https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf · accessed 2026-11-16]
- Step-length cap when the target is out of reach: 0.6·L. [https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf · accessed 2026-11-16]
- Step period that works for human-scale characters: 0.2 s ≤ T ≤ 1.0 s. Stride length follows from L = Vd·T. [https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf · accessed 2026-11-16]
- Speeds shown: −0.6 to 1.7 m/s. [https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf · accessed 2026-11-16]
- PD gains: kp scaled **linearly** by M′/M when the character mass changes (M = original mass, M′ = new mass; p. 5: "we scale the kp values by M′/M"). kd = 2·√kp for all joints — the square root applies to kd only, which is critical damping for unit inertia. (Checked on the rendered page image; the PDF text layer drops the √ sign.) [https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf · accessed 2026-11-17]
- Simulation: ODE time step 0.0005 s. Friction 0.8. No torque limits in the results; adding limits of 200 Nm (hips and knees) and 100 Nm (other joints) gave no visible change. [https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf · accessed 2026-11-16]
- Model: 37 DOF, including 6 global DOF. [https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf · accessed 2026-11-16]
- Starting to walk: the COM target moves to the next stance foot, and walking triggers once the COM has covered 40% of the way there. [https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf · accessed 2026-11-16]

### 2c. Quadruped (M3)
- Dog model:
  - Shoulder height 47 cm, hip height 45 cm, mass 34 kg. [https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · accessed 2026-11-16]
  - 30 links and 67 internal DOF (7 per leg, 15 spine, 12 neck/head, 12 tail). [https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · accessed 2026-11-16]
  - ODE at 1000 Hz. Friction 1.0. Torque limit 100 Nm in the robustness tests. [https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · accessed 2026-11-16]
- Speed bands per gait:
  - walk 0–1.5 m/s [https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · accessed 2026-11-16]
  - trot 1.0–2.5 m/s [https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · accessed 2026-11-16]
  - canter 2.0–3.5 m/s [https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · accessed 2026-11-16]
  - transverse gallop 3.5–5.5 m/s [https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · accessed 2026-11-16]
- Transitions:
  - Walk→trot blends gait parameters linearly over 1.0–1.5 m/s. [https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · accessed 2026-11-16]
  - Canter→gallop blends over 3.0–3.5 m/s. [https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · accessed 2026-11-16]
  - Trot↔canter switches at 2.2 m/s, using a 0.3 s transition controller. [https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · accessed 2026-11-16]
  - The desired speed changes by at most 0.5 m/s at a time. [https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · accessed 2026-11-16]
- Largest pushes recovered (Table 1): [https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · accessed 2026-11-16]

| Gait | v (m/s) | Force (N) | Duration (s) | Source URL · accessed |
|---|---|---|---|---|
| walk | 1.0 | 80 | 0.3 | https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · 2026-11-16 |
| walk | 1.0 | 230 | 0.1 | https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · 2026-11-16 |
| trot | 2.2 | 100 | 0.35 | https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · 2026-11-16 |
| trot | 2.2 | 200 | 0.1 | https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · 2026-11-16 |
| canter | 3.0 | 120 | 0.35 | https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · 2026-11-16 |
| canter | 3.0 | 400 | 0.1 | https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · 2026-11-16 |
| transverse gallop | 5.0 | 100 | 0.35 | https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · 2026-11-16 |
| transverse gallop | 5.0 | 400 | 0.1 | https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · 2026-11-16 |

- Top speeds: simulated trot up to 3 m/s (it looks worse above 2.4 m/s); gallop up to 5.7 m/s. The real dog was filmed trotting at 3.3 m/s, and dogs gallop at 6.5 m/s. [https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · accessed 2026-11-16]

### 2d. Optimal gait and form (M4)
- Models and DOF: monoped 10, simplified biped 16, velociraptor 33, horse 29, pentaped 32. [https://grail.cs.washington.edu/wp-content/uploads/2015/08/wampler2009oga.pdf · accessed 2026-11-16]
- 30 frames per gait cycle. [https://grail.cs.washington.edu/wp-content/uploads/2015/08/wampler2009oga.pdf · accessed 2026-11-16]
- Each spacetime solve takes about 1 min for the simple biped and 10–20 min for the horse. [https://grail.cs.washington.edu/wp-content/uploads/2015/08/wampler2009oga.pdf · accessed 2026-11-16]
- Outer CMA settings: λ = 96, µ = 32, c_cov = 0.3. Useful results after about 50 iterations. [https://grail.cs.washington.edu/wp-content/uploads/2015/08/wampler2009oga.pdf · accessed 2026-11-16]
- Muscle-stress cap σmax = 50 N/cm², taken from endurance-trained athletes. [https://grail.cs.washington.edu/wp-content/uploads/2015/08/wampler2009oga.pdf · accessed 2026-11-16]
- The optimiser picks walk or run by itself depending on the requested speed. Figure 3 shows the biped at 0.7 and 3.0 m/s. [https://grail.cs.washington.edu/wp-content/uploads/2015/08/wampler2009oga.pdf · accessed 2026-11-16]

### 2e. Spore (M5)
- Gait per leg group:
  - The duty factor is the fraction of the cycle a foot is on the ground.
  - The step trigger is each foot's phase offset.
  - Legs are grouped by roughly equal length. Groups get cycle frequencies in small whole-number ratios.
  - Gait parameters are authored against speed and interpolated.
- IK: limbs may compress to 10% of rest length and stretch to 120%. [https://doi.org/10.1145/1360612.1360626 · accessed 2026-11-16]
- Speed: about 0.2 ms per frame for a 25-body creature on a 1.7 GHz Pentium-M, about 35% of it in IK. [https://doi.org/10.1145/1360612.1360626 · accessed 2026-11-16]
- Testing: several hundred creatures × about a thousand animations, with about a 90% pass rate. [https://doi.org/10.1145/1360612.1360626 · accessed 2026-11-16]

### 2f. Overgrowth (M6), from the talk
- 13 keyframes cover the core movement set. [https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach ; https://reducer.xgoose.org/items/8812 · accessed 2026-11-16]
- Each locomotion speed has 2 keys (pass and reach), giving 6 locomotion keys for walk, run and crouch-walk. There are also 2 standing keys (stand and crouch). [https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach ; https://reducer.xgoose.org/items/8812 · accessed 2026-11-16]
- The ledge climb uses 1 keyframe plus IK. [https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach ; https://reducer.xgoose.org/items/8812 · accessed 2026-11-16]
- The falling reaction uses 3 flail keyframes. [https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach ; https://reducer.xgoose.org/items/8812 · accessed 2026-11-16]
- Receiver reloads use 2–3 keys with spring interpolation. [https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach ; https://reducer.xgoose.org/items/8812 · accessed 2026-11-16]
- The talk states a physics fact: a gait with half the time per step allows only a quarter of the vertical fall distance. This is why bounce flattens as speed rises.
- No numeric spring constants were given.

### 2g. Bird flight (M7): Table 2

| | Eagle | Raven | Sparrow | Source URL · accessed |
|---|---|---|---|---|
| Mass (kg) | 3.7 | 0.5 | 0.025 | https://grail.cs.washington.edu/projects/flight/wu2003realistic.pdf · 2026-11-16 |
| Time step (s) | 0.001 | 0.0007 | 0.00025 | https://grail.cs.washington.edu/projects/flight/wu2003realistic.pdf · 2026-11-16 |
| Flight time (s) | 6.7 | 5.2 | 3.8 | https://grail.cs.washington.edu/projects/flight/wu2003realistic.pdf · 2026-11-16 |
| Optimisation time (min) | 218 | 232 | 324 | https://grail.cs.washington.edu/projects/flight/wu2003realistic.pdf · 2026-11-16 |
| Joint gain k1 range | 20–1000 | 2–100 | 0.01–0.5 | https://grail.cs.washington.edu/projects/flight/wu2003realistic.pdf · 2026-11-16 |
| Joint damping k2 range | 0.02–2 | 0.002–0.2 | 0.00001–0.001 | https://grail.cs.washington.edu/projects/flight/wu2003realistic.pdf · 2026-11-16 |

- Bird model: 11 links and 21 joint DOF, plus feather DOF. Each flight feather is a 2-DOF joint driven by the shoulder, elbow and wrist. [https://grail.cs.washington.edu/projects/flight/wu2003realistic.pdf · accessed 2026-11-16]
- Machine: 2.8 GHz Pentium 4. [https://grail.cs.washington.edu/projects/flight/wu2003realistic.pdf · accessed 2026-11-16]

### 2h. Muscle-based bipeds (M8)
- Neural delays (following Geyer & Herr 2010): 20 ms for foot muscles, 10 ms for lower-leg muscles, 5 ms for upper-leg and other muscles. [https://goatstream.com/research/papers/SA2013/SA2013.pdf · accessed 2026-11-16]
- Activation constant 100 s⁻¹. [https://goatstream.com/research/papers/SA2013/SA2013.pdf · accessed 2026-11-16]
- Start state: activation 0.02. [https://goatstream.com/research/papers/SA2013/SA2013.pdf · accessed 2026-11-16]
- Evaluation: each candidate runs for 10 or 20 s; 500–3000 CMA generations; 2–12 h on a standard PC; runs in real time once optimised. [https://goatstream.com/research/papers/SA2013/SA2013.pdf · accessed 2026-11-16]
- Models: 20 muscles for the human-like templates; 29–132 optimised parameters per model (Table 3). [https://goatstream.com/research/papers/SA2013/SA2013.pdf · accessed 2026-11-16]

### 2i. Walknet coordination rules (M10), as stated in the 2013 review
Each rule is passed from a sender leg to a receiver leg. Together they set when each leg starts its swing:

| Rule | What it does | Source URL · accessed |
|---|---|---|
| 1 | Delays the receiver's swing while the sender is swinging, plus a speed-dependent delay. Back-to-front, same side. | https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ · 2026-11-16 |
| 2 | Moves the receiver's lift-off point forward 270 ms after the sender starts stance, for 50 ms. Back-to-front and across sides. | https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ · 2026-11-16 |
| 3 | Moves the receiver's lift-off forward when the sender reaches a speed-dependent position. | https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ · 2026-11-16 |
| 4 | Represented by a version of the Target-net (Dean 1990) that sends the current position of the **anterior (front) leg** to the leg behind, which uses it as its **swing target** — i.e. rule 4 sets the receiver's AEP (where the swinging foot lands). No numeric gains given in the review. | https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ · 2026-11-17 |
| 5 | Shares load between neighbouring legs. | https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ · 2026-11-16 |
| 6 | Not implemented. | https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ · 2026-11-16 |

According to the review, rules 1–4 plus mechanical coupling are enough to produce tripod, tetrapod and wave gaits with smooth transitions: [https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ · accessed 2026-11-16]
- Tripod gait: at least 3 legs on the ground at all times. [https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ · accessed 2026-11-16]
- Tetrapod gait: at least 4 legs on the ground. [https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ · accessed 2026-11-16]
- Wave gait: at least 5 legs on the ground. [https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ · accessed 2026-11-16]

### 2j. DeepLoco (M13): learned contrast
- The high-level policy runs at 2 Hz (step time scale) and the low-level policy at 30 Hz. [https://www.cs.ubc.ca/~van/papers/2017-TOG-deepLoco/2017-TOG-deepLoco.pdf · accessed 2026-11-16]
- The reference motion was 7 s of segmented mocap, or one keyframed cycle. [https://www.cs.ubc.ca/~van/papers/2017-TOG-deepLoco/2017-TOG-deepLoco.pdf · accessed 2026-11-16]

## 3. Method families by leg count

| Legs | Rule-based, real-time | Offline, optimised | Notes for a seeded Rust generator | Source URL · accessed |
|---|---|---|---|---|
| 2 | M1 SIMBICON; M2 generalized walking; M6 Overgrowth (kinematic) | M8 muscle-based; M4 | M6 needs no physics. M1 and M2 need a rigid-body sim with PD joints. The M1 and M2 parameter rows above give direct ranges. | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf ; https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach ; https://reducer.xgoose.org/items/8812 ; https://goatstream.com/research/papers/SA2013/SA2013.pdf ; https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf ; https://grail.cs.washington.edu/wp-content/uploads/2015/08/wampler2009oga.pdf · 2026-11-16 |
| 4 | M3 quadruped (physics); M5 Spore gait (kinematic) | M4 (horse) | The M3 gait speed bands and switch speeds are ready ranges. Gait graphs come from the paper's figures, not tables. | https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf ; https://doi.org/10.1145/1360612.1360626 ; https://grail.cs.washington.edu/wp-content/uploads/2015/08/wampler2009oga.pdf · 2026-11-16 |
| many | M5 Spore gait; M10 Walknet rules; M12 oscillators | M4 (pentaped) | M5 duty factor and phase offset is the simplest way to cover any leg count. M10 and M12 make gaits emerge from local rules. | https://doi.org/10.1145/1360612.1360626 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ ; https://grail.cs.washington.edu/wp-content/uploads/2015/08/wampler2009oga.pdf ; https://pmc.ncbi.nlm.nih.gov/articles/PMC8450536/ · 2026-11-16 |
| flying | none found that is rule-based and real-time and has published numbers | M7 bird flight | M7 gives mass, gain and time-step ranges for 3 birds but is offline. | https://grail.cs.washington.edu/projects/flight/wu2003realistic.pdf · 2026-11-16 |

## 4. Licence notes

- Every paper here is cited as fact. Formulas and numbers are restated in my own words and tables. No figures were copied.
  - The ACM papers (M1–M5, M7–M9, M13) are © ACM or the authors. [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf ; https://doi.org/10.1145/1360612.1360626 ; https://grail.cs.washington.edu/projects/flight/wu2003realistic.pdf ; https://history.siggraph.org/learning/robust-physics-based-locomotion-using-low-dimensional-planning-by-mordatch-de-lasa-and-hertzmann/ ; https://www.cs.ubc.ca/~van/papers/2017-TOG-deepLoco/2017-TOG-deepLoco.pdf · accessed 2026-11-16]
  - The author-hosted PDFs are free to read but are not openly licensed.
  - The Spore PDF I read is a third-party course copy hosted at Chalmers. The author's own site was down when I checked.
- Code:
  - Overgrowth code is **Apache-2.0**, so commercial use is allowed with the licence text and notices kept. Its art and data are excluded. [https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach ; https://reducer.xgoose.org/items/8812 · accessed 2026-11-16]
  - cartwheel-3d (M2) is listed as **Apache 2.0** in the Google Code archive. [https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf · accessed 2026-11-16]
  - JSimbicon (M1) and the M3 binary have **no licence**, so do not reuse them. [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf ; https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · accessed 2026-11-16]
- Open-access papers: M10 (2013 review) and M12 are **CC BY**. Reuse is allowed with attribution, including commercially. [https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ ; https://pmc.ncbi.nlm.nih.gov/articles/PMC8450536/ · accessed 2026-11-16]
- Talk: the GDC Vault page for M6 is © Informa. The talk text I used is a third-party automatic transcript (reducer.xgoose.org) of GDC's YouTube upload. Only facts are restated. [https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach ; https://reducer.xgoose.org/items/8812 · accessed 2026-11-16]
- Shipping derived numbers (gains, speed bands, rules) as facts in a generator is standard citation practice. It does not carry the papers' copyright, but each value should be credited to its source.

## 5. Gaps and caveats, in brief
- The SIMBICON run and skip rows were not transcribed (extraction misalignment).
- Mordatch 2010, Ijspeert 2008, Raibert & Hodgins 1991, Cruse 1998 and Won et al. 2017 (dragon flight) could only be checked at abstract or metadata level. [https://history.siggraph.org/learning/robust-physics-based-locomotion-using-low-dimensional-planning-by-mordatch-de-lasa-and-hertzmann/ ; https://pubmed.ncbi.nlm.nih.gov/18555958/ · accessed 2026-11-16]
- The Rosen talk numbers come from an auto-transcript. The keyframe counts were cross-checked within the transcript (10:28 and 8:32), not against slides. [https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach ; https://reducer.xgoose.org/items/8812 · accessed 2026-11-16]

## 6. Full source list (all accessed 2026-11-16)

| ID | Citation | URL(s) | Link check (2026-11-16) | Licence | Accessed |
|---|---|---|---|---|---|
| M1 | Yin K, Loken K, van de Panne M. SIMBICON: Simple biped locomotion control. *ACM TOG* 26(3), Article 105, 2007. DOI 10.1145/1276377.1276509 | https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf ; https://www.cs.ubc.ca/~van/papers/Simbicon.htm ; https://doi.org/10.1145/1276377.1276509 | PDF 200, project page 200. DOI is valid in Crossref; ACM DL blocks curl (403). Table 1 and §7 numbers were read from the PDF. | © ACM. Demo code has no licence. | 2026-11-16 |
| M2 | Coros S, Beaudoin P, van de Panne M. Generalized biped walking control. *ACM TOG* 29(4), 2010. DOI 10.1145/1778765.1781156 | https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf ; https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/index.html ; https://code.google.com/archive/p/cartwheel-3d | PDF 200. Page 200. Archive 200; its project.json gives licence `asf20`. | © ACM. Code Apache 2.0. | 2026-11-16 |
| M3 | Coros S, Karpathy A, Jones B, Reveret L, van de Panne M. Locomotion skills for simulated quadrupeds. *ACM TOG* 30(4), 2011. DOI 10.1145/2010324.1964954 | https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf ; https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/index.html | 200 / 200. Gait speed bands, Table 1 and §7 were read. | © ACM. Binary only. | 2026-11-16 |
| M4 | Wampler K, Popović Z. Optimal gait and form for animal locomotion. *ACM TOG* 28(3), Article 60, 2009. DOI 10.1145/1531326.1531366 | https://grail.cs.washington.edu/wp-content/uploads/2015/08/wampler2009oga.pdf | 200. §5 and §6 were read. | © ACM | 2026-11-16 |
| M5 | Hecker C, Raabe B, Enslow RW, DeWeese J, Maynard J, van Prooijen K. Real-time motion retargeting to highly varied user-created morphologies. *ACM TOG* 27(3), Article 27, 2008. DOI 10.1145/1360612.1360626 | https://doi.org/10.1145/1360612.1360626 ; read copy: https://www.cse.chalmers.se/edu/year/2011/course/TDA361/Advanced%20Computer%20Graphics/027-hecker%20copy.pdf ; author copy: https://www.chrishecker.com/images/c/cb/Sporeanim-siggraph08.pdf | DOI valid in Crossref. Chalmers copy 200, with §4.2, §4.3 and §5 read. The author copy returned "site temporarily down" at 10:23 AEDT. | © ACM. The read copy is third-party. | 2026-11-16 |
| M6 | Rosen D. Animation Bootcamp: An Indie Approach to Procedural Animation. GDC 2014 (Wolfire Games). | https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach ; video: https://www.youtube.com/watch?v=LNidsMesxSE ; transcript: https://reducer.xgoose.org/items/8812 ; code: https://github.com/WolfireGames/overgrowth | All 200. The Vault page confirms the title, speaker and GDC 2014. Counts come from the transcript at 5:29–8:37, 10:28, 11:42, 17:04 and 18:07. Repo licence Apache-2.0 per the GitHub API and README. | Talk © Informa/GDC. Code Apache-2.0. Assets proprietary. | 2026-11-16 |
| M7 | Wu J-c, Popović Z. Realistic modeling of bird flight animations. *ACM TOG* 22(3):888–895, 2003. DOI 10.1145/882262.882360 | https://grail.cs.washington.edu/projects/flight/wu2003realistic.pdf | 200. Tables 1 and 2 were read. | © ACM | 2026-11-16 |
| M8 | Geijtenbeek T, van de Panne M, van der Stappen AF. Flexible muscle-based locomotion for bipedal creatures. *ACM TOG* 32(6), 2013. DOI 10.1145/2508363.2508399 | https://goatstream.com/research/papers/SA2013/SA2013.pdf | 200. §4, §5 and Table 3 were read. | © ACM | 2026-11-16 |
| M9 | Mordatch I, de Lasa M, Hertzmann A. Robust physics-based locomotion using low-dimensional planning. *ACM TOG* 29(4), 2010. DOI 10.1145/1778765.1778808 | https://history.siggraph.org/learning/robust-physics-based-locomotion-using-low-dimensional-planning-by-mordatch-de-lasa-and-hertzmann/ ; https://dl.acm.org/doi/10.1145/1778765.1778808 | History page 200 (abstract). ACM 403 to curl. | © ACM | 2026-11-16 |
| M10 | Schilling M, Hoinville T, Schmitz J, Cruse H. Walknet, a bio-inspired controller for hexapod walking. *Biol Cybern* 107(4):397–419, 2013. DOI 10.1007/s00422-013-0563-5. Original: Cruse H et al. *Neural Networks* 11(7–8):1435–1447, 1998. DOI 10.1016/S0893-6080(98)00067-7 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3755227/ ; https://doi.org/10.1016/S0893-6080(98)00067-7 | PMC 200; the full text was read via the Europe PMC API. The Springer DOI returned 500 at check time. The 1998 DOI returned 200. | 2013 review CC BY. 1998 paper © Elsevier. | 2026-11-16 |
| M11 | Ijspeert AJ. Central pattern generators for locomotion control in animals and robots: a review. *Neural Networks* 21(4):642–653, 2008. DOI 10.1016/j.neunet.2008.03.014 | https://pubmed.ncbi.nlm.nih.gov/18555958/ ; https://infoscience.epfl.ch/entities/publication/a4bd6bb4-0d67-420c-be64-0e07ac44b816 | PubMed 203 (page served). EPFL record page 200. | © Elsevier. An EPFL green copy exists. | 2026-11-16 |
| M12 | Ambe Y, Aoi S, Tsuchiya K, Matsuno F. Generation of direct-, retrograde-, and source-wave gaits in multi-legged locomotion in a decentralized manner via embodied sensorimotor interaction. *Front Neural Circuits* 15:706064, 2021. DOI 10.3389/fncir.2021.706064 | https://pmc.ncbi.nlm.nih.gov/articles/PMC8450536/ | 200. Abstract read. | CC BY | 2026-11-16 |
| M13 | Peng XB, Berseth G, Yin K, van de Panne M. DeepLoco. *ACM TOG* 36(4), 2017. DOI 10.1145/3072959.3073602 | https://www.cs.ubc.ca/~van/papers/2017-TOG-deepLoco/2017-TOG-deepLoco.pdf | 200 | © ACM | 2026-11-16 |
| L1 | Apache License 2.0 | https://www.apache.org/licenses/LICENSE-2.0 | 200 | – | 2026-11-16 |
| L2 | CC BY 4.0 deed (M10 states "Creative Commons Attribution License"; M12 is CC BY per Europe PMC) | https://creativecommons.org/licenses/by/4.0/ | 200 | – | 2026-11-16 |
| L3 | ACM copyright policy (Crossref licence link for the ACM papers) | https://www.acm.org/publications/policies/copyright_policy | 403 to curl (bot block) | – | 2026-11-16 |

## Not found
- **Flying:** no rule-based, real-time flight controller with published numeric parameters. Wu & Popović (M7) is offline. Won et al. 2017 "How to train your dragon" (DOI 10.1145/3130800.3130833) is closed, and I did not read it.
- **Overgrowth:** no numbers for spring stiffness, damping, acceleration-tilt gain or surveyor-wheel radius. The talk gives none, and I did not search the Apache-2.0 code for them.
- **Spore:** no values for duty factor or step-trigger offset per leg count. The paper describes the parameters but gives no tables.
- **Quadruped gait graphs (M3):** only in figures and supplemental material; no numeric table in the paper.
- **Full text not read:**
  - Mordatch 2010 (M9).
  - Ijspeert 2008 (M11).
  - Raibert & Hodgins 1991 "Animation of dynamic legged locomotion" (DOI 10.1145/122718.122755; ACM 403). Its hopping, foot-placement rules and gains are unverified here.
  - Kry et al. 2009 "Modal locomotion" (DOI 10.1111/j.1467-8659.2009.01368.x). The HAL PDF did not download.
- **Rain World and other indie procedural creature talks:** not searched in this pass.
- **SIMBICON:** the run, fast-run, skip and scissor-hop rows were not transcribed.

## Sources disagree
- **SIMBICON article number:** Crossref and the Coros 2010 reference list give *ACM TOG* 26(3) Article 105. The Mordatch 2010 reference list, as shown on the SIGGRAPH History Archive, gives Article 81. The DOI is the same either way, so cite by DOI. [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf ; https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf ; https://history.siggraph.org/learning/robust-physics-based-locomotion-using-low-dimensional-planning-by-mordatch-de-lasa-and-hertzmann/ · accessed 2026-11-16]
- **SIMBICON push figure:** the project page caption says the push was "350N;0.2s". The PDF's Figure 1 caption says "350N, 0.2s". Same value, no conflict. [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf · accessed 2026-11-16]
- **Damping gains:** SIMBICON uses kd = 0.1·kp (3D) or kp 300 / kd 30 (planar). Coros 2010 uses kd = 2·√kp (critical damping for unit inertia) with kp scaled to mass. These are different conventions, so do not mix gain sets between the two methods. [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf ; https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf · accessed 2026-11-16]
- **Real vs simulated dog speeds (M3):** simulated trot up to 3.0 m/s vs the captured dog at 3.3 m/s; simulated gallop 5.7 m/s vs about 6.5 m/s for a real dog. The authors state the gap themselves. [https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf · accessed 2026-11-16]
- **Rule-based vs optimised:** papers that call themselves "simple" or "rule-based" (M1–M3) still use offline optimisation for some parameters (M3 entirely; M2 none). M4, M7 and M8 are optimisation-first. In this survey "rule-based" means that the run-time behaviour is a fixed set of rules; parameters may come from offline tuning. [https://www.cs.ubc.ca/~van/papers/2007-siggraph-simbicon.pdf ; https://www.cs.ubc.ca/~van/papers/2010-TOG-gbwc/paper.pdf ; https://www.cs.ubc.ca/~van/papers/2011-TOG-quadruped/paper.pdf ; https://grail.cs.washington.edu/wp-content/uploads/2015/08/wampler2009oga.pdf ; https://grail.cs.washington.edu/projects/flight/wu2003realistic.pdf ; https://goatstream.com/research/papers/SA2013/SA2013.pdf · accessed 2026-11-16]
