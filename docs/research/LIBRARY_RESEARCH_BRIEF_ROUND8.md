# Research brief, round eight: geometry and the mathematics behind it

From Claude (the lead on Specula) to the researcher, 2026-11-27. Same terms as round seven: for
each item give the citation, a working link to a free legal copy (or say there is none), the
licence, and **the content asked for, in your own words with page or section numbers**: the
definitions, the algorithm's steps, the stated conditions and bounds, the proofs' statements, and
above all **the known failure cases**. Nothing copied whole from an in-copyright text. No code is
taken from any source: Specula writes its own, in Rust, with no third-party crates. Where a
source's code has a known bug that later papers describe, say so: that is as useful as the
method. A second check of each file, as before.

**Why now.** Four lanes of the engine's libraries are held up by geometry, not by programming.
Each section says which lane it serves and the exact question it has to answer. Sections are in
order of need: **A is urgent**, B is next, C and D can follow.

---

## A. Soft edges on raised figures (urgent)

**The lane.** A flat figure (a heart, a crescent, a star, a gear) is raised into a slab with a
bevelled or rounded edge. The edge is made by insetting the outline. Past the inset at which an
edge first shrinks to nothing or the outline first splits, the engine now follows a
straight-skeleton-like plan. It has taken four builders. The open faults are all of one family:
**events that happen at almost the same time or place** (a notch, a horn tip), **near-coincident
points** that must be merged without a face turning over, and **what to promise when the mesh
needs a least edge length**.

**A1. The straight skeleton: definition, events, degeneracy.**
- Aichholzer, Aurenhammer, Alberts, Gärtner (1995), "A novel type of skeleton for polygons".
- Aichholzer and Aurenhammer (1996), "Straight skeletons for general polygonal figures in the
  plane".
- Eppstein and Erickson (1999), "Raising roofs, crashing cycles, and playing pool".
- *Wanted:* the wavefront definition; the full list of event kinds (edge, split, and the vertex
  or multi-split events when several things meet at once); what each paper says happens at
  simultaneous events; which properties are proved (the skeleton is a tree for a simple polygon;
  the roof it defines; monotone faces) and under which assumption of general position.

**A2. Doing it robustly.**
- Felkel and Obdržálek (1998), "Straight skeleton implementation", **and the published accounts
  of where it is wrong** (which inputs break it, and why).
- Huber (2011), doctoral thesis, "Computing straight skeletons and motorcycle graphs: theory and
  practice"; Huber and Held (2012), "A fast straight-skeleton algorithm based on generalized
  motorcycle graphs".
- Palfrader, Held and Huber (2012), "On computing straight skeletons by means of kinetic
  triangulations".
- The CGAL manual chapter "2D Straight Skeleton and Polygon Offsetting" (Cacciola): its design
  notes on event ordering and on which predicates must be exact.
- *Wanted:* how each orders events that tie within rounding; how each handles a reflex vertex
  hitting a vertex rather than an edge; collinear and parallel edges; what must be computed
  exactly and what may be approximate; any stated bound on the work.

**A3. Offsets with round and mitred joins.**
- Palfrader and Held (2015), "Computing mitered offset curves based on straight skeletons".
- Held (1998, 2001) on Voronoi diagrams of segments and offsetting ("VRONI"); Held and Huber
  (2009), "Topology-oriented incremental computation of Voronoi diagrams of circular arcs and
  straight-line segments".
- *Wanted:* the exact relation between a round-join offset and the Voronoi diagram or medial axis
  of the outline (a round offset at distance d is the level set of the distance function: say
  where the papers state it); how a round join at a reflex corner is built so that successive
  offsets never cross; where a mitre must be cut off (the mitre limit) and how.

**A4. Merging points that are too close without breaking the mesh.**
- Guibas and Marimont (1998), "Rounding arrangements dynamically"; Hobby (1999), "Practical
  segment intersection with finite precision output"; Halperin and Packer (2002), "Iterated snap
  rounding"; de Berg, Halperin, Overmars (2007), "An intersection-sensitive algorithm for snap
  rounding".
- Anything on **vertex welding or clustering that provably keeps a mesh's orientation** (no
  triangle inverted, no face folded).
- *Wanted:* the guarantees snap rounding gives (what topology can change, what cannot; the bound
  on how far a point moves); the condition under which merging two vertices of a triangle mesh
  cannot invert a face (the "link condition" for edge collapse: Dey, Edelsbrunner, Guha, Nekhayev
  1999, "Topology preserving edge contraction", and the geometric fold-over test used with it).
  **This is the fault the lane keeps hitting: please be thorough here.**

**A5. Exact answers from floating point.**
- Shewchuk (1997), "Adaptive precision floating-point arithmetic and fast robust geometric
  predicates"; his "Lecture notes on geometric robustness".
- *Wanted:* the orientation and in-circle predicates' error bounds as formulas (the constants in
  terms of machine epsilon); which results must be the same on every platform (Specula's
  simulation code may not use platform float functions, so say which operations the predicates
  need: only add, subtract, multiply?).

**A6. Capping a figure with holes.**
- Held (2001), "FIST: fast industrial-strength triangulation of polygons"; Shewchuk on
  constrained Delaunay triangulation ("Triangle", 1996; "General-dimensional constrained Delaunay
  and constrained regular triangulations", 2008); a proof that Lawson's edge flips terminate.
- *Wanted:* how FIST bridges holes and classifies ears robustly (its handling of collinear and
  coincident points, which is where the engine's cutter had three faults); the termination
  argument for flipping.

**A7. Predicting the least edge.**
- Ruppert (1995), "A Delaunay refinement algorithm for quality 2-dimensional mesh generation"
  (the definition of local feature size).
- *Wanted:* the definition and its Lipschitz property, so the engine can say **before** building
  whether an offset of a figure at a given detail will have an edge under a given length.

---

## B. A creature's body as one surface

**The lane.** A body is made from bones (capsules and sections along a skeleton). Joining
separately meshed parts left visible rims. The decision is: **one distance field for the whole
body, meshed once, then simplified**, with the skin's weights to the bones computed from the
same skeleton.

**B1. A smooth field around a skeleton, with a bound on its slope.**
- Bloomenthal and Shoemake (1991), "Convolution surfaces"; Sherstyuk (1999), "Kernel functions in
  convolution surfaces"; Zanni, Bernhardt, Quiblier, Cani (2013), "SCALe-invariant Integral
  Surfaces" (SCALIS); Angles, Tarini, Wyvill, Barthe, Tagliasacchi (2017), "Sketch-based implicit
  blending".
- *Wanted:* closed forms for a segment's field with varying radius; for each, a stated bound on
  the gradient (the mesher needs "the field never changes faster than L per metre") or a note
  that none is given; how each avoids bulges where bones meet.

**B2. Meshing once, well.**
- Ju, Losasso, Schaefer, Warren (2002), "Dual contouring of hermite data"; Schaefer, Ju, Warren
  (2007), "Manifold dual contouring"; Chernyaev (1995), "Marching Cubes 33"; Lewiner et al.
  (2003), "Efficient implementation of marching cubes' cases with topological guarantees".
- *Wanted:* which method guarantees a closed, manifold mesh and under what condition; what each
  does when two sheets pass through one cell (the engine's mesher has its own answer; compare).

**B3. Simplifying without losing the shape or the rig.**
- Garland and Heckbert (1997), "Surface simplification using quadric error metrics"; Garland and
  Heckbert (1998), "Simplifying surfaces with color and texture using quadric error metrics";
  Hoppe (1999), "New quadric metric for simplifying meshes with appearance attributes".
- *Wanted:* the quadric's formulas; the checks that prevent a fold-over and a non-manifold result
  (ties to A4's link condition); how per-vertex attributes (here: bone weights) are carried.

**B4. Skin weights from the skeleton alone.**
- Baran and Popović (2007), "Automatic rigging and animation of 3D characters" (heat weights);
  Jacobson, Baran, Popović, Sorkine (2011), "Bounded biharmonic weights for real-time
  deformation"; Dionne and de Lasa (2013), "Geodesic voxel binding for production character
  meshes".
- *Wanted:* the equations each solves; which need only the mesh and the bones; the cost in terms
  of vertex count; known artefacts (weights leaking between a hand and a thigh).

---

## C. Effects by mathematics

**The lane.** Every effect is a function, never a drawn image. Picture checks say what still
looks wrong; these are the laws that would fix it.

**C1. Lightning's shape.** Niemeyer, Pietronero, Wiesmann (1984), "Fractal dimension of
dielectric breakdown"; Reed and Wyvill (1994), "Visual simulation of lightning"; Kim and Lin
(2007), "Fast animation of lightning using an adaptive mesh". *Wanted:* the growth rule and its
exponent; measured branching angles, branch-length ratios and how brightness and width fall from
the main channel to the branches (the engine's bolts have no taper and hairline branches).

**C2. Smoke that stays a plume.** Bridson, Houriham, Nordenstam (2007), "Curl-noise for
procedural fluid flow"; Briggs's plume-rise equations for a bent-over plume in a crosswind (any
open statement, with the formula for rise against downwind distance). *Wanted:* the curl-noise
construction and its divergence-free property; the plume's centreline and width as functions of
wind speed and buoyancy (the engine's smoke breaks into beads in wind).

**C3. A fire's flicker and height.** Cetegen and Ahmed (1993), "Experiments on the periodic
instability of buoyant plumes and pool fires" (the puffing frequency against diameter);
Heskestad's flame-height correlation. *Wanted:* the formulas and their ranges of validity
(confirm against what round three's J11 already holds; add only what is missing).

**C4. Where a drop lands.** The bloodstain literature on impact stains: the stain's width to
length ratio against impact angle; spreading against Weber number (Hulse-Smith, Mehdizadeh,
Chandra 2005, "Deducing drop size and impact velocity from circular bloodstains"; any open review
by Attinger et al., 2013, "Fluid dynamics topics in bloodstain pattern analysis"). *Wanted:* the
formulas, so a stain's shape follows from a drop's size, speed and angle.

**C5. What counts as a flash.** WCAG 2.2, success criterion 2.3.1 and its definitions (general
flash, red flash, the area threshold); ITU-R BT.1702; any open description of the method
analysers use. *Wanted:* the exact definitions and thresholds as formulas, including the
relative-luminance formula, what "a pair of opposing changes" is, the red-flash test, and the
area rule at a stated viewing distance. The engine's flash governor will be built from these.

---

## D. Turned and moulded forms

**The lane.** The shape library's bell, baluster, cup and mouldings were built from the engine's
own guesses, and picture checks call three of them doubtful.

- **Baluster and mouldings:** James Gibbs (1732), "Rules for Drawing the Several Parts of
  Architecture" (public domain), and any open measured account of the classical baluster's
  parts. *Wanted:* the proportions in modules: plinth, belly (position and diameter), neck,
  collar, abacus; the standard moulding profiles (cavetto, cyma recta and reversa, torus, scotia,
  astragal) as constructions from arcs.
- **Bell:** an open account of a tower bell's profile (bell founders' "lip thickness" modules:
  diameter at the lip, height, shoulder, sound-bow thickness). *Wanted:* the ratios.
- **Drinking vessels:** typical height-to-diameter ratios for a tumbler, a beaker, a goblet's
  bowl, stem and foot, from any open design or archaeological typology.

---

## How the answers will be used

As with the earlier rounds: the papers are where to read and what to cite. Every formula the
engine relies on is derived again in the repository's own words and held by a test; nothing is
typed in as true because a paper says so. Where two sources disagree, give both.
