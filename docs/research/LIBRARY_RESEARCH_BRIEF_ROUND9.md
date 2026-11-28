# Research brief, round nine: procedural generation, the methods

From Claude (the lead on Specula) to the researcher, 2026-11-28. Same terms as rounds seven and
eight: for each item give the citation, a working link to a free legal copy (or say there is none),
the licence, and **the content asked for, in your own words with page or section numbers**: the
definitions, the method's steps, its stated conditions and limits, and above all **the known failure
cases**. Nothing copied whole from an in-copyright text. No code and no data set is taken from any
source: Specula writes its own, in Rust, with no third-party crates, and several of the models named
below have licences that forbid using their data. **What is wanted from those is the method, never
the data.** A second check of each file, as before. Where a listed source cannot be had, a labelled
substitute held to the same standard is welcome.

**Why now.** The owner has set the order in which Specula makes a person or a creature: numbers,
then a skeleton that is the rig, then one base surface grown round it, then detail (a face, hands,
nails) sculpted on at named places, then one mesh. And, later, a "procedurally generated generator":
the plan that turns numbers into a model becomes data itself, with every number bounded. **The
measured numbers are largely delivered already** (rounds one to seven: body dimensions, trunk
sections, hands, faces, joint ranges, animals). What is missing is how others have **built
generators from such numbers**, and how bounds are carried through a computation.

Sections are in order of need: **A is first** (the lane that uses it starts within a day), B next,
C to E can follow.

---

## A. A body's surface from its skeleton and a few measurements (first)

**The lane.** A person's base surface is to be one distance field round the skeleton, shaped by
measured breadths, depths and girths at levels along each bone. Round five delivered those numbers
(the files A3, A13, A14, A15). The engine's first figure, built from columns of sections between
joints, has a trunk like a column, tubes for legs and a seam at the neck. What is wanted is how a
believable surface is made **between** the measured levels, and round the joints.

**A1. Bodies from measurements, as a method.**
- Allen, Curless, Popović (2003), "The space of human body shapes: reconstruction and
  parameterization from range scans".
- Seo and Magnenat-Thalmann (2003), "An automatic modeling of human bodies from sizing parameters".
- Anguelov et al. (2005), "SCAPE"; Loper et al. (2015), "SMPL" (**method only**: how shape and pose
  are separated, how many shape numbers are needed, what the first components mean in plain terms:
  height, weight, sex, build).
- *Wanted:* how each turns a handful of numbers (stature, weight, girths) into a whole surface; how
  many independent numbers explain how much of human variation, and what the first few are; how
  male and female differ in those terms; the stated errors against measured bodies. The engine will
  not use any of their data: it needs to know **which numbers matter and in what order**.

**A2. Muscles and fat as shapes on a skeleton.**
- Scheepers, Parent, Carlson, May (1997), "Anatomy-based modeling of the human musculature";
  Wilhelms and Van Gelder (1997), "Anatomically based modeling".
- Any open account of how subcutaneous fat is distributed by site and sex as a thickness map (the
  delivered A15 has skinfolds at sites: what joins them into a surface).
- *Wanted:* which shapes they use for a muscle belly and a tendon (ellipsoids, generalised
  cylinders), how a muscle's cross-section changes with the joint's angle, how the skin is laid over
  them, and which muscles cannot be left out without the silhouette going wrong (the shoulder, the
  chest, the buttock, the calf).

**A3. The surface at a joint.**
- Vaillant et al. (2013), "Implicit skinning: real-time skin deformation with contact modeling";
  Vaillant et al. (2014), "Robust iso-surface tracking for interactive character skinning".
- Kavan, Collins, Žára, O'Sullivan (2008), "Geometric skinning with approximate dual quaternion
  blending" (the candy-wrapper and collapsing-elbow faults of linear blending, and what removes
  them).
- *Wanted:* why linear blend skinning collapses an elbow and twists a wrist, with the mathematics;
  what each remedy costs and where it still fails; how implicit skinning keeps volume and makes
  contact at a bent joint, and what it needs from the field (ties to round eight's B1).

---

## B. Detail on a base: faces, hands, nails, ears

**The lane.** Detail is sculpted onto the base at named places. Each piece of detail should itself be
a named thing with bounded numbers.

**B1. A face from measurements.**
- DeCarlo, Metaxas, Stone (1998), "An anthropometric face model using variational techniques"
  (**this is the closest thing to what the engine wants: please be thorough**).
- Blanz and Vetter (1999), "A morphable model for the synthesis of 3D faces" (method only).
- Farkas's craniofacial landmarks and measurement definitions (any open account of the landmark set
  and how each measurement is taken; round two's B files hold many of the numbers).
- *Wanted:* how a face surface is generated from a list of landmark distances and proportions; which
  constraints are hard and which are soft; how many numbers a recognisably different face needs;
  how sex and age enter; what goes wrong (the stated failure cases of each).

**B2. A hand and its nails.**
- Any open anatomically based hand model built from measurements (Albrecht, Haber, Seidel 2003,
  "Construction and animation of anatomically based human hand models"; Rhee, Neumann, Lewis 2006,
  "Human hand modeling from surface anatomy").
- Open measurements of the fingernail: length, breadth and curvature across and along, by finger and
  sex (the nail plate's transverse and longitudinal curvature has a clinical literature).
- *Wanted:* how the palm's surface and the finger pads are built from the bones and the delivered A9
  dimensions; the knuckle's shape when bent; the creases' positions as shares of the segment; the
  nail's numbers.

**B3. The ear, as a shape from numbers.**
- Any open parametric description of the auricle (helix, antihelix, tragus, concha, lobule: their
  sizes as shares of the ear's length and breadth; round two's B9 has the outer dimensions).

---

## C. Generators as data, and bounds carried through them

**The lane, later.** A plan that is data: parts, how they attach, repeat, mirror and taper, with
every number bounded, so that the engine can say before it builds how large a thing can be and
whether any draw could make a broken body.

**C1. Grammars that make shapes.**
- Stiny and Gips (1972), "Shape grammars and the generative specification of painting and
  sculpture"; Stiny (1980), "Introduction to shape and shape grammars".
- Prusinkiewicz and Lindenmayer (1990), "The Algorithmic Beauty of Plants" (the authors host a free
  copy): parametric, context-sensitive and stochastic L-systems.
- Wonka et al. (2003), "Instant architecture" (split grammars); Müller et al. (2006), "Procedural
  modeling of buildings" (CGA shape).
- Havemann (2005), "Generative mesh modeling" (a stack language for shapes).
- *Wanted:* what each can and cannot express (branching, repetition, symmetry, attachment, a part
  that must fit a space); how each handles a rule whose result does not fit (refuse, clip, try
  another rule); how randomness enters and is kept repeatable; what authors of each say is hard to
  write or to debug. **Which of them could be validated before running**, and how.

**C2. Bounds through a computation.**
- Moore (1966), "Interval analysis" (any open introduction that states the rules); de Figueiredo and
  Stolfi (2004), "Affine arithmetic: concepts and applications"; Snyder (1992), "Interval analysis
  for computer graphics".
- *Wanted:* the rules for intervals under add, multiply, divide, power, sine and cosine; why plain
  intervals grow too wide when a quantity appears twice (the dependency problem) and how affine
  arithmetic reduces it; rounding outward in floating point; what is known about bounding the
  output of a procedural model (a count of parts, a bounding box) from the ranges of its
  parameters.

**C3. Constraints between numbers.**
- Any open survey of geometric constraint solving for parametric models (Bettig and Hoffmann 2011,
  "Geometric constraint solving in parametric computer-aided design"; Hoffmann's earlier surveys).
- *Wanted:* how such systems say that a set of constraints is under- or over-determined; how they
  report which constraint cannot be met; ties to C2.

---

## D. Creatures from a skeleton, by rule

**D1. What shipped.**
- Hecker et al. (2008), "Real-time motion retargeting to highly varied user-created morphologies"
  (Spore), and any open account by its makers of how a creature's skin was grown round its spine and
  limbs and how it was textured without an artist.
- Sims (1994), "Evolving virtual creatures" (the genotype as a graph of parts with repetition and
  recursion: the nearest published thing to a plan as data for a body).
- *Wanted:* how the body's surface followed the skeleton (the kind of field or sweep, how limbs
  joined the trunk without a seam); how one animation was made to fit any body; the limits the
  makers state.

**D2. Proportion as a small set of rules.**
- Any open treatment of "allometric templates" for vertebrate body plans beyond what rounds three
  and seven delivered: how head, neck, trunk and limb proportions move together across a family
  (canids, felids, bovids), so that a new animal is a point between known ones.

---

## E. Knowing a generated thing is right without looking at every one

**E1.** Any open work on measuring whether a generated body is plausible by numbers alone: outline
measures against anthropometric data, symmetry, volume against stated mass and density (round one's
A4 has segment masses), balance of the standing pose over the feet.
**E2.** Perceptual thresholds: how far a proportion can stray before people call a figure wrong
(the "uncanny" literature has little that is numeric: give only what states a measured threshold).

---

## How the answers will be used

As with the earlier rounds: the papers are where to read and what to cite. Every formula the engine
relies on is derived again in the repository's own words and held by a test; nothing is typed in as
true because a paper says so. Where two sources disagree, give both. Where a method depends on a
data set the engine may not use, say so plainly, and say what the method still gives without it.
