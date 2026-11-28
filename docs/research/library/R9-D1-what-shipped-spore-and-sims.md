# R9-D1 What shipped: Spore's creatures (Hecker et al. 2008 and the Maxis sketches) and Sims 1994

Round 9, section D. Access date for every source: **2026-11-28**. Everything is in my own words. **Method only:** no code, data or assets were taken. Reasoning marked **(mine)** is my own. Pages are **PDF pages** of the copies named below. For the S2 and S3 slide decks, "slide N" means PDF page N (PDF page 1 is a blank cover).

**Questions this item answers** (brief D1, "Wanted"):
- How did the body's surface follow the skeleton? What kind of field or sweep was used, and how did limbs join the trunk without a seam?
- How was one animation made to fit any body?
- What limits do the makers state?

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| S1 | Hecker C., Raabe B., Enslow R.W., DeWeese J., Maynard J., van Prooijen K. (2008). "Real-time motion retargeting to highly varied user-created morphologies." *ACM Trans. Graphics* 27(3), Article 27, 11 pp. (SIGGRAPH 2008). doi:10.1145/1360612.1360626 | The author's own posting (chrishecker.com/images/c/cb/Sporeanim-siggraph08.pdf) returned an error on 2026-11-28; the site says it is "temporarily down". The copy read is 4,899,270 bytes, the authors' dvips build dated May 2008; **its download URL was not logged**. One public copy is listed on a university course reading list: https://www.cmlab.csie.ntu.edu.tw/~ming/images/new_reference_papers_ICG/Proj_10_Sporeanim-siggraph08_Real-time%20Motion%20Retargeting.pdf. That URL returned a TLS error and HTTP 500 from the box today, so it is **not confirmed working**, and that copy's authorisation is unverified. OpenAlex marks the paper closed | ACM copyright | Yes, all 11 pages plus references |
| S2 | Choy L., Ingram R., Quigley O., Sharp B., Willmott A. (2007). "Rigblocks: player-deformable objects." *ACM SIGGRAPH 2007 Sketches*, Article 83. doi:10.1145/1278780.1278880 | Co-author's page (Willmott, CMU): https://www.cs.cmu.edu/~ajw/s2007/0248-Rigblocks.pdf and slides 0248-Rigblocks-slides.pdf | ACM copyright; author-posted | Yes (1-page abstract and 26 slides) |
| S3 | DeBry D., Goffin H., Hecker C., Quigley O., Shodhan S., Willmott A. (2007). "Player-driven procedural texturing." *ACM SIGGRAPH 2007 Sketches*, Article 81. doi:10.1145/1278780.1278878 | https://www.cs.cmu.edu/~ajw/s2007/0311-PaintingModels.pdf and 0311-PaintingModels-slides.pdf | ACM copyright; author-posted | Yes (1-page abstract and 28 slides) |
| S4 | Hecker C. "My Liner Notes for Spore." Web page, chrishecker.com/My_Liner_Notes_for_Spore | **Down on 2026-11-28** ("temporarily down"). No archived copy could be reached (the Wayback Machine was unreachable from the box, and WebFetch found no stored copy; archive.today returned 403) | Unknown | **Not read.** This is where the maker's own account of the creature skin appears to be. See §1 |
| S5 | Sims K. (1994). "Evolving virtual creatures." *Proc. SIGGRAPH 94*, pp. 15-22. doi:10.1145/192161.192167 | Author's site: https://www.karlsims.com/papers/siggraph94.pdf. The box's curl was refused; the text was read via WebFetch of that URL (saved as raw/round9/sims94.webfetch.txt) | ACM copyright; author-posted | Yes, full text. **Cited by section**, because the WebFetch text has no page markers |

## 1. How the creature's surface followed the skeleton

**What the open sources say first-hand:**
- **The body is a special base block.** The player controls a basic skeleton by adjusting a spline and gluing limbs to it. **The mesh is generated with metaballs,** and other rigblocks (mouths, hands, feet and so on) are attached to the body (S2 slides, PDF p.13).
- The player shapes "a malleable clay-like torso containing the spine" and attaches limbs and deformable parts from a palette (S1 PDF p.2).
- A finished character is made of **bodies** (anatomical parts, spine vertebrae, limbs), meshes and textures, with **20 to 80 bodies** in a typical character (S1 PDF p.2).
- Bodies are the bone equivalents. They carry transforms, bounding boxes, parent and child links, and **capability tags** ("caps") that say what each body is, for example a hand body carries the grasper cap (S1 PDF p.2).
- The bodies form a **directed acyclic graph with a serial chain of spine bodies at the root** (S1 PDF p.2).
- Bodies can carry **scalar deform curves** that drive small mesh animations (hands opening, mouths closing). These are standardised per part type and kept out of the retargeting system (S1 PDF p.2).

**What is not in the open sources read:** the kind of field, its kernel, how metaballs were placed along spine and limbs, how the surface was polygonised, and how skin weights were set. S4, the maker's own notes, appears to cover this but was down today (**missing**).

- A third-party research note (the `symbios-avatar` crate's documentation on docs.rs) summarises S4. It says the skin was a blobby implicit surface of spherical metaballs placed along the spine and limb graph, polygonised directly rather than with marching cubes.
- **These details are UNVERIFIED secondary reporting and are not relied on here.** Re-read S4 when the site is back.

**Rigblocks: how parts deform** (S2 abstract and slides):
- A rigblock is a building-block part with **animations parameterised over [0, 1]** instead of time, driven by handles the player drags.
- **Deform animations must add, not average.** Cross-blending two deforms at 50% each would undo half of each (slide 18).
- So deltas from the rest pose are accumulated: translation, scale and rotation are summed separately and the final matrix is formed at the end, to match Maya's preview.
- S2 notes that summing Euler angles, unlike chaining quaternions, makes the result of N delta animations **order-independent**. Maxis concatenated quaternions and found the difference too small to matter.
- **Baking** (slides 19-24):
  - when the player finishes, all deform animations are removed to give a new base mesh;
  - parts that keep runtime animations (mouths) are swapped for low-bone versions;
  - the deformed pose is **retargeted onto the reduced skeleton**, because the base pose has changed.

## 2. How texture was applied without an artist (S3)

**Creatures** (S3 abstract; slides 15-16):
- The creature is a main body mesh plus attached detail parts.
- The studio's effects system (Swarm) runs **particle-driven brushes** over the mesh. Brushes paint diffuse, specular and bump textures directly.
- Particles can move over a **changing** mesh with both 2D (tangent space) and 3D control.
- UVs for the body mesh are generated quickly by **cubemap partitioning**.
- Custom components run paint **up and down the spine and limbs** and cover the skin with **oriented strokes**.
- Particle movement can be "affected by skeleton" (slide 16).
- **Player control:**
  - layered **base, coat and detail** paint scripts;
  - player-chosen colours;
  - one-click themes that pre-set combinations.

**Buildings and vehicles** (S3 abstract; slide 18):
- Parts deform drastically, so **static UVs would stretch**.
- Instead, regions are tagged with a projection type (box, cylinder, sphere, planar, disc), and the projection runs **in the vertex shader** at runtime.
- Textures are tiled swatches parameterised by **two palette colours**.

**Turning it into a game model** (S3 abstract; slides 21-26):
- **Problem:** too many meshes, materials and draw batches, and no levels of detail.
- **Fix:**
  - generate a second set of unique UVs by face clustering, optionally relaxed with LSCM, and packed;
  - render with clip position = second UV to **"splat"** every part's texture into one sheet of fixed size;
  - weld all meshes and simplify for levels of detail by vertex decimation that respects bones and discontinuities.
- **Ambient occlusion** is generated on the GPU by accumulating shadow passes. S3 calls it the "visual glue", because separately painted parts have no shared dark map.
- **Baking** runs as background jobs so the game is not blocked (slide 26).

## 3. How one animation fits any body (S1)

**The core split** (S1 PDF p.1-3):
- Animators author in a tool (Spasm) against example characters, but data is stored in a **generalised, body-independent** form.
- At runtime it is **specialised** to each character, and the resulting goals go to an IK solver.
- For a body b, a generalisation function G maps its specialised pose to generalised coordinates, and S maps back. G and S are inverses (S1 PDF p.3-4). A mirroring flag m handles the sagittal plane.

**Step 1: say which bodies, by meaning not by name** (S1 PDF p.3, §3.1):
- A channel's **context** is a filter query, like an email filter, over bodies:
  - **type** (a cap such as grasper, mouth, spine or root);
  - **spatial constraints** (front, centre or back; left, centre or right; top, centre or bottom), relative either to the whole character's bounds or to the bounding box of just the bodies with that cap (**setspace**);
  - an **extent** query that picks the single most extreme body (ties broken arbitrarily);
  - a **limb modifier** that walks up to the nearest spine body (an approximate shoulder or hip).
- Automatic query creation from clicks was **ruled out**: many different queries select the same body, so the ambiguity would cause errors (S1 PDF p.3).

**Step 2: say what matters about the motion: movement modes** (S1 PDF p.4-5, §3.2.2). Every mode keeps the **rest pose as the origin**, so all characters are at rest for the same generalised value.

| Mode | What G records | Example from S1 |
|---|---|---|
| Identity (absolute) | The same character-relative value for all selected bodies; mostly for rotation | Hold a platter level |
| Rest relative | Offset from each body's own rest pose | A wave |
| Scale: CreatureSize | Positions scaled by the character's bounding box (non-uniformly) | Big creatures, big motions |
| Scale: LimbLength | Positions scaled by the path length from the body to the nearest spine body | Long limbs move more |
| Ground relative | Vertical axis rescaled so 0 is rest height and 1 is the ground | Pound the ground with any grasper height |
| Secondary relative | A frame from rest position (0) to a second body set or a game-supplied external target (1); the frame updates as the target moves | Hand to mouth, pick fruit, shake hands |
| Lookat | Rotation frame aimed at the target, with soft limits behind the body | Nod while looking at something |

**Step 3: blend, key and preview** (S1 PDF p.5):
- Channels own bodies by priority and blend group; blending between channels is how a motion changes mode over time (grab fruit, then bring it to the mouth).
- Keys are stored on generalised values with Hermite splines.
- Changing a mode **remaps the keys** so the active body's motion stays the same.
- Spasm previews the same data on **several characters at once**.

**Step 4: bind at runtime** (S1 PDF p.5-6, §4.1):
- **Branch predicates** with three values (true, false, ignore), such as UprightSpine, HasGraspers and HasFeet, pick between separately authored versions. S1 calls this a **last resort**, because it multiplies authoring and testing.
- **Variants:** a channel can play on only one of its selected bodies, so one variant exists per candidate body, and the game picks one (the grasper nearest the fruit).
- Multi-channel animations take a **variant product**, optionally limited to same-side or opposite-side pairs. Sagittal **mirroring** is generated automatically.

**Step 5: legs are synthesised, not keyed** (S1 PDF p.6-7, §4.2):
- A leg is a path from a spine body (the hip) to a foot.
- Legs are grouped by roughly equal length, and **group length ratios are approximated by small rational numbers** to set relative gait frequencies.
- Each foot gets a **duty factor** (share of the cycle on the ground) and a **step trigger** (phase offset).
- Animators author speed-to-gait mappings for 1-6 feet; 7 or more are generated procedurally.
- Footless characters float or crawl; crawlers turn spine bodies into pseudo-feet with an inch-worm gait.

**Step 6: the Particle IK solver** (S1 PDF p.7-9, §4.3):
- **Goals:** fast, accurate and natural inside reach, and **path-independent**.
- **Three failure modes it must handle gracefully** (S1 PDF p.7):
  1. goals out of reach (reach, do not hyperextend);
  2. conflicting goals that **over-determine** a sub-tree;
  3. reachable but implausible goals.
- **Structure:** bodies become 3-DOF particles with 1-DOF length constraints between them, solved by simple iterative length correction. Each constraint has per-end "masses" that are tuning values, not physics.
  - Length errors pass through a smooth (C1) piecewise function, so constraints are **soft** and may stretch or compress.
- **Two phases:** spine first, then limbs, each tunable separately. S1 reports that more complex solvers (CCD, Jacobian, constrained dynamics) were slower and harder to tune (S1 PDF p.7).
- **Spine** (S1 PDF p.8-9):
  - particles go only at IK branch points;
  - the spine between branch points is a spline fitted by least squares. **Cubic splines failed on player spines with many inflections; quintic Hermite splines were enough**;
  - interior spine bodies are rebuilt by transporting a frame along the spline;
  - **anti-buckling:** if neighbouring spine constraints fold too sharply, constraints toward the rest pose are blended in;
  - limits: 5 outer iterations; shrink to 10% and stretch to 120% are the allowed bounds.
- **Limbs** (S1 PDF p.9): an **aim preconditioner** rotates and scales each limb so its end touches the goal before iterating (temporarily breaking lengths). This favours rotation at shoulder and hip and gave more natural poses than iterating from rest.
- **Secondary motion ("Jiggles")** (S1 PDF p.9): sub-trees with no goals get a damped pseudo-physical simulation that **never feeds back** into keyed bodies. An earlier version that set IK goals ("Wiggles") degraded the animators' work.

**Testing at scale** (S1 PDF p.10):
- Animations were tested stochastically against a changing database of player-uploaded creatures. Testers logged failures in a grid.
- Over development, several hundred creatures and a thousand animations were tested, with a pass rate of **about 90%**.
- Failures fell into code bugs, generalisation issues fixable with existing features, and issues that needed new features or branches.
- **Cost:** about **0.2 ms per frame** for a 25-body character on a 1.7 GHz Pentium-M, about 35% of it in IK.

## 4. Limits the makers state

**S1 PDF p.10:**
1. **No notion of volume or boundary.** Motions like "hug each other" or "rub your chin" cannot be expressed.
2. **Self-collision is ignored** for lack of time, which S1 calls clearly a flaw.
3. **Particle IK failure could be more graceful.** Anti-buckling can look heavy-handed.
4. **Singularities.** Path independence forces singularities (by the hairy-ball theorem, footnote 11). They are pushed to unlikely poses, but limbs can still twist near them.
5. **Learning curve.** Animators make a complete animation in hours but need **weeks** to learn what generalises. Equivalent authorings on one character can specialise completely differently on others.
6. **Future work:** more reasoning about morphology ("close in", "extended", boundary-relative); anticipation in gaits under discontinuous input.

**S2 and S3:**
- Static UVs fail on deforming parts (S3).
- Averaging deforms fails (S2).
- Many parts mean many draw batches and no levels of detail until the model is baked (S3 slide 21).

## 5. Sims 1994: the body plan as a graph (S5)

**Genotype** (S5 §2):
- A **directed graph** of nodes and connections. Each node describes one rigid box part:
  - its dimensions;
  - a joint type (rigid, revolute, twist, universal, bend-twist, twist-bend or spherical);
  - joint limits (springs beyond them);
  - a **recursive limit**;
  - local neurons.
- Each connection places a child on the parent's **surface** by position, orientation, scale and **reflection** (negative scale gives mirror-image sub-trees). Each part can mutate independently.
- A **terminal-only** flag applies a connection only at the end of a recursion, for tails or hands at the ends of chains.
- **The graph can be recurrent.** A node can link to itself or to a cycle (repetition and fractal-like chains), or to the same child several times (duplicate limbs).
- The node labels in the figures ("body", "leg") are for readers. **The genotype has no idea of functional categories.**

**Phenotype** (S5 §2): built by starting at a root node and tracing connections, giving a hierarchy of 3D parts.

**Control nested inside morphology** (S5 §3.4):
- Each morphological node holds its own neural sub-graph, so **duplicated parts get duplicated local controllers**.
- Signals may pass only between adjacent parts. One extra set of central neurons is not tied to any part.

**Evolution** (S5 §6):
- Populations were typically 300, with a survival ratio of 1/5.
- **Mutation:**
  - node parameters change with Gaussian-like steps scaled to the value, then are clamped to legal bounds;
  - new nodes are added;
  - connections are re-pointed, added or removed;
  - **unconnected nodes are garbage-collected**.
- **Mating:** crossover of node lists, or grafting.

**Validity checks before fitness** (S5 §5):
- Creatures with too many parts get zero fitness.
- **Parts that interpenetrate at birth** are pushed apart by a short simulation; if that fails, the creature is discarded.

**Failure cases stated:**
- **Physics exploitation.** Any simulation bug that leaks energy, even round-off error, **will be found and exploited** by evolution (S5 §4).
- **No skin.** Future work suggests flexible skin, scales, hair, fur and materials, possibly also coded in the genotype (S5 §8).

## 6. Lane notes (mine, not from the sources)

1. **Plan as data.** Sims' graph (recursive limit, terminal-only connections, reflection as negative scale, per-part neural sub-graphs) is the nearest published precedent for a body plan held as a graph. Spore's caps are the precedent for **semantic tags** that let later stages (animation, AI, inventory) query the body without knowing its shape. Both carry over directly as methods.
2. **Surface.** Only "metaballs over a player-controlled spline skeleton" is confirmed first-hand (S2 slide 13). The rest of the skin method is **missing** until S4 can be read. R9-A3 (surface at a joint) is the sourced fallback.
3. **Fit-any-body checks.** S1's three IK failure modes (out of reach, over-determined, implausible) are a ready test list for any posing stage. Its ~90% pass rate against a stream of real user bodies is a precedent for **stochastic validation over a corpus of generated bodies** rather than hand-checking (see E1).
4. **Deforms add, not average** (S2). Any stack of shape sliders on one body should accumulate deltas from rest. Order-independence is worth testing explicitly.
