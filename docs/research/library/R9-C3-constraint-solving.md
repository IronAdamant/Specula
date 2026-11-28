# R9-C3 Constraint solving: under- and over-determined systems, and reporting what failed

Round 9, section C. Access date for every source: **2026-11-28**. PDFs were read with poppler `pdftotext`. Everything is in my own words. **Method only:** no solver code was taken. Reasoning marked **(mine)** is my own.

**Questions this item answers** (brief C3):
- How do constraint solvers classify systems as under-, well- or over-determined?
- What do they do in each case?
- How can the solver tell the user which constraint is unmet?

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| S1 | Bettig B., Hoffmann C.M. (2011). "Geometric constraint solving in parametric computer-aided design." *J. Computing and Information Science in Engineering* 11(2):021001. doi:10.1115/1.3593408 | **None found.** The Purdue copy tried in this round did not download (saved as `bettig11.FAILED-not-a-pdf.html`) | ASME copyright | **Not read.** Substitutes: S2 and S3, by the same senior author |
| S2 (substitute) | Hoffmann C.M., Joan-Arinyo R. (2005). "A brief on constraint solving." *Computer-Aided Design and Applications* 2(5):655-663. doi:10.1080/16864360.2005.10738330 | OpenAlex lists a green copy at the co-author's university repository: http://hdl.handle.net/2117/83305. The copy read is the publisher PDF with a Taylor & Francis cover page (10 pp.); **whether it is the same file as the repository copy is unverified** | OpenAlex licence field "other-oa"; no licence printed on the PDF | Yes, all pages. **Printed page = PDF page + 653** |
| S3 (substitute) | Hoffmann C.M. (2004, May). "Summary of basic 2D constraint solving." Published in *Int. J. Product Lifecycle Management* 1(2):143-149 (2006). doi:10.1504/ijplm.2006.008674 | Purdue CGVLab: https://www.cs.purdue.edu/cgvlab/www/resources/papers/Hoffmann-IJPLM-2006-Summary_of_Basis_2D_Constaint_Solving.pdf (6 pp., byte-identical to the copy read) | No licence stated; author's lab | Yes, all pages |

## 1. Definitions (S2 PDF p.4; S3 PDF p.1)

- **Under-constrained:** infinitely many solutions that are not just moved or rotated copies of each other.
- **Well-constrained:** finitely many solutions, up to moving and rotating the whole.
- **Over-constrained:** deleting one or more constraints would make it well-constrained.

**These are structural categories** (S2 PDF p.4):
- They look at which elements are tied to which, not at the values.
- A structurally well-constrained problem can still be **under-constrained for particular values**. In S2's example, a point is fixed by two angles unless the angles sum to 90°, when its position becomes undetermined (Fig. 3, printed on PDF p.5).
- Specific values can also make a problem **inconsistent**, which structure cannot see.

## 2. Counting degrees of freedom: the cheapest test (S3 PDF p.3)

**The constraint graph:**
- **Vertices** are geometric elements. Each is weighted by its degrees of freedom: 2 for a point or a free line, 3 for a circle of unknown radius, 1 for a line forced horizontal.
- **Edges** are constraints. Each is weighted by how many degrees it removes: 1 for a distance between two points, 2 for making two points coincide.

**Deficiency** = sum of vertex weights − sum of edge weights.
- A well-constrained 2D figure that is free to sit anywhere should have a deficiency of **3** (two moves and one turn).
- If it is fixed to a coordinate frame, the deficiency should be **0**.

**What the count cannot see (mine, from S2's definitions):**
- A global count can balance while one part is over-constrained and another under-constrained.
- That is why solvers decompose the graph rather than trusting the total (§3).

## 3. Decomposition: finding the solvable parts (S2 PDF p.6-7; S3 PDF p.3-5)

**The decomposition step:**
- Find a **minimal well-constrained subgraph** (a "cluster core") and solve it.
- Grow it by **sequential extension**: add an element whose degrees exactly match the constraints tying it to the cluster.
- **Merge** clusters that overlap pairwise in shared elements. In 2D triangle decomposition, three clusters that pairwise share an element are merged (S3 PDF p.3).

**Plan, then solve** (S3 PDF p.4):
- If the whole graph reduces to **one cluster**, the planning phase succeeds and a solution is possible in principle.
- **If decomposition fails, the solver announces that no solution can be found.** S3 stresses that this means only that triangle decomposition has no solution; a solution may still exist. This is the gap between one strategy and a fully general solver.

**Order does not matter** (S3 PDF p.5). If a graph can be decomposed one way by triangle decomposition, every order of reductions decomposes it fully (a Church-Rosser property). The order of decomposition is therefore not critical for deciding solvability.

**Generalising** (S2 PDF p.7):
- **Flow-based methods** split the system into **minimal dense subgraphs**. This generalises degree-of-freedom counting and pattern matching.
- But the parts can be arbitrarily complex and may not correspond to simple geometric constructions.
- **One cited method (Latham et al.) identifies over- and under-constrained problems and solves over-constrained ones by giving constraints priorities.**

**Analysis of systems of equations** (S2 PDF p.9):
- These methods classify a system as under-, well- or over-constrained from its structure.
- They can split it into minimal parts that are solved separately.
- One cited method (Serrano) **selects a well-constrained, solvable subset** from candidate constraints.

**Propagation** (S2 PDF p.7):
- The equations are oriented so each can be solved in turn.
- **Propagation fails when it meets a loop**, and it does not guarantee a solution even when one exists.
- Numerical solving can be combined with it to handle the loops.

## 4. What a solver should guarantee (S2 PDF p.5)

S2 lists desired properties:
- **Sound:** always halts, and any solution found meets every constraint.
- **Complete:** solves every solvable problem and **announces** unsolvability for every unsolvable one.
- **Competent** (the practical substitute, because known efficient methods are not complete): complete within a stated sub-domain. Characterising that sub-domain may itself be hard.
- **Persistent:** the same constraints always give the same solution.
- **Stable:** small changes give nearby solutions.
- **Efficient**, and **robust** against floating-point error.
- **Intensional:** finds the solution the user wants.

**Cost.** Full competence works against efficiency, because general constraint solving has **doubly exponential** complexity (S2 PDF p.5; S3 PDF p.2).

## 5. Choosing among several solutions: the root-selection problem (S3 PDF p.4-5; S2 PDF p.5)

**The problem:**
- Even simple sequential problems have several solutions, possibly exponentially many.
- Solvers usually keep the solution with the **same order type** (left-right arrangement of point triples) as the user's sketch.
- With triangle decomposition this choice does not depend on decomposition order (S3 PDF p.5).

**Failure cases** (S3 PDF p.4-5):
1. **When dimensions change, the sketch can stop being a good guide.** A different order type may be needed, and the old sketch no longer shows design intent.
2. **Exploring all roots as a tree** is systematic but not user-friendly.
3. **Automated choice is not general.** For example, requiring a non-self-intersecting polygon makes steps of the solver **NP-hard**.
4. **Dragging one element to flip its order type and re-solving** can confuse the user when several triples must flip, because the intermediate results do not show progress.
5. **Showing each step's alternatives visually** works but needs a sophisticated user.

## 6. Numerical and algebraic solving (S2 PDF p.8)

- **Generality.** Algebraic solvers turn everything into non-linear equations. They are general and can in principle be fully competent, but may be slow or unreliable at constructing solutions.
- **A well-constrained figure becomes an under-determined equation system** unless extra equations pin down position and rotation.
- **Numerical iteration needs a good starting point,** normally the user's sketch, which must be close to the intended solution. It gives little control over **which** solution is reached.
- **Relaxation** (perturb values, minimise a global error) converges slowly.
- **Newton-Raphson** is local and much faster, but it does not handle **consistently over-constrained** systems unless combined with something like least squares.
- **Homotopy (continuation)** is global, guarantees convergence and can find **all** solutions, but is slower than Newton-Raphson.
- **Soft constraints:** one cited method (Hel-Or et al.) does not require constraints to be met exactly. It models them as a stochastic process solved with a Kalman filter. This is a precedent for "soft" plausibility targets (mine).

## 7. Reporting the unmet constraint: what the sources do and do not give

**Given by the sources:**
- **Structural diagnosis:**
  - deficiency counts (S3);
  - decomposition failure, announced as "no solution by this method" (S3);
  - over- and under-constraint identification in some methods (S2, citing Latham et al.);
  - selection of a solvable subset (S2, citing Serrano).
- **Priorities on constraints**, which let an over-constrained system be solved by dropping the lowest-priority ones (S2 PDF p.7).

**Not given by the sources read:** a method that names *which single constraint* to blame when values (not structure) make a system inconsistent, nor a standard error message. S1, which may cover this for parametric CAD, was not read (**missing**).

**Design for the lane (mine, not from the sources):**
1. **Run structure checks before values.**
   - Build the constraint graph, then compute the deficiency for the whole graph and for every decomposed part.
   - Report "over-determined at part P (k extra)" or "under-determined at part P (k free)", naming the elements in P.
   - This is cheap and catches most authoring errors.
2. **Use priorities for value conflicts.** Give each constraint a priority (hard, then soft tiers). Solve hard constraints exactly and soft ones in least squares (DeCarlo's split in R9-B1 is one example).
   - If the hard set has no solution, report the **smallest hard subset** that is infeasible. Find it by deletion filtering: remove constraints one at a time and keep those whose removal makes it feasible.
   - This is a standard technique outside these sources.
3. **Use interval bounds (C2) as a fast pre-check.** If interval evaluation shows a constraint's two sides cannot meet over the parameter box, it is unmeetable for every value. Report it by name before any solving.
4. **Persistence and stability (S2) are the two properties to test.** The same seed and inputs should give the same body, and a small edit should give a small change. Root selection by order type, recorded once, is how S3's solvers keep persistence.
