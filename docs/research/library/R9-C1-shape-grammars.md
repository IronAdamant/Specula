# R9-C1 Shape grammars and rule systems: what each can say, how each fails

Round 9, section C. Access date for every source: **2026-11-28**. PDFs were downloaded to the box and read with poppler `pdftotext`. Pages are **PDF pages** of the copies named below. Everything is in my own words; the short rule fragments shown are paraphrased notation, not copied code. **Method only:** no code or rule sets were taken. Reasoning marked **(mine)** is my own.

**Questions this item answers** (brief C1), for each formalism:
- What can it express, and what can it not?
- What happens when a rule's result does not fit?
- How is randomness kept repeatable?
- What is hard to write or debug?
- What could be checked before running?

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| S1 | Stiny G., Gips J. (1972). "Shape grammars and the generative specification of painting and sculpture." In *Information Processing 71* (IFIP Congress 1971), North-Holland. Published page range not verified | The copy read is the authors' **submitted manuscript** ("Paper 128", 25 pp. plus figures). It was originally posted by MIT Architecture at architecture.mit.edu/sites/architecture.mit.edu/files/attachments/publications/SGIFIPSubmitted.pdf (**now 404**). Read via an offline-library mirror of MIT OpenCourseWare material: https://library.snls.org.sz/OCW/MIT%20OCW/architecture.mit.edu/sites/architecture.mit.edu/files/attachments/publications/SGIFIPSubmitted.pdf. **Whether the mirror is authorised is unverified**; the original posting was by the author's own department | No licence stated | Yes: definition, example, painting formalism (PDF p.1-24) |
| S1b | Stiny G. (1980). "Introduction to shape and shape grammars." *Environment and Planning B* 7:343-351 | **None found** (closed) | Publisher copyright | **Not read.** Its position is reported only through S4 and S3's summaries of it |
| S2 | Prusinkiewicz P., Lindenmayer A. (1990). *The Algorithmic Beauty of Plants.* Springer. Electronic edition 2004 | Author's lab site: https://algorithmicbotany.org/papers/abop/abop.pdf (240 pp.) | "© 2004 Przemyslaw Prusinkiewicz. All rights reserved" (PDF p.4); free to read from the author's site | Yes: Ch. 1 §1.1-1.10 (PDF p.13-62). **Printed page = PDF page minus 12** |
| S3 | Wonka P., Wimmer M., Sillion F., Ribarsky W. (2003). "Instant architecture." *ACM Trans. Graphics* 22(3):669-677 (SIGGRAPH 2003). doi:10.1145/882262.882324 | TU Wien author copy: https://www.cg.tuwien.ac.at/research/publications/2003/Wonka-2003-Ins/Wonka-2003-Ins-Paper.pdf (9 pp.) | ACM copyright; author-posted | Yes, all pages |
| S4 | Müller P., Wonka P., Haegler S., Ulmer A., Van Gool L. (2006). "Procedural modeling of buildings." *ACM Trans. Graphics* 25(3):614-623 (SIGGRAPH 2006). doi:10.1145/1141911.1141931 | Co-author's site: https://peterwonka.net/Publications/pdfs/2006.SG.Mueller.ProceduralModelingOfBuildings.final.pdf (10 pp.) | ACM copyright; author-posted | Yes, all pages |
| S5 | Havemann S. (2005). *Generative Mesh Modeling.* PhD dissertation, TU Braunschweig. doi:10.24355/dbbs.084-200603150100-7; also 10.2312/diss.20058171 | Eurographics Digital Library: https://diglib.eg.org/server/api/core/bitstreams/5544b5ad-68d0-4020-aaa7-d87c6dd71742/content (303 pp.). The TU Braunschweig repository sits behind a bot check and was not used | No licence stated in the PDF; free download from EG DL | Partly: Ch. 1 related work (PDF p.34-43), §2.2.4 and §2.3 on Euler operators (PDF p.75-88), start of Ch. 5 on the GML (PDF p.219-226). **PDF pages are cited; printed numbers are 8-9 lower** |

## 1. Stiny and Gips (1972): the original shape grammar (S1)

**Definition** (S1 PDF p.6-7). A shape grammar has four parts:
1. a finite set of **terminal** shapes;
2. a finite set of **marker** shapes (non-terminals), kept separate from the terminals;
3. a finite set of **shape rules** u → v;
4. an **initial shape**.

Terminals may be used many times at any scale or orientation. Each rule's left side combines terminals with a marker. Its right side is one of three types:
- **A:** the same terminals alone (the marker is removed);
- **B:** the same terminals with a marker;
- **C:** the same terminals plus added terminals and a marker.

Type B is logically redundant but was kept because it helped (PDF p.14).

**Applying a rule** (PDF p.7):
1. Find a part of the current shape that is **geometrically similar** to a rule's left side, markers included.
2. Find the transformation (scale, translation, rotation, mirror) that makes the two identical.
3. Apply the same transformation to the right side and substitute it.

**Stopping and the language** (PDF p.7):
- Generation stops when **no rule applies**.
- The language is every shape reachable **without markers left**; it can be infinite.
- Terminals, once placed, **can never be erased**, because every rule's right side keeps its left side's terminals.

**Markers** (PDF p.9):
- They restrict where a rule may apply.
- They set the scale relation between the rule and the shape.
- They force or end continuation; in the example, the rule that removes the marker ends the derivation.

**Turning a language into finished works: levels and selection** (PDF p.16-17):
- Each terminal gets a **level**, like the depth of a phrase in a sentence: 0 for the initial shape, and N + 1 for anything a rule adds at level N.
- A **selection rule** (m, n) admits only shapes whose levels lie between a minimum m and a maximum n.
- Because terminals and their levels are permanent, the selection rule **can serve as a halting test** (PDF p.17).

**Painting rules** (PDF p.19-20):
- Each level is treated as a set in a Venn diagram.
- Rules map set expressions (union, intersection, complement, exclusive or) to a paint.
- The left sides must **partition** the whole, so every area is painted exactly one way.
- A **limiting shape** acts like a camera viewfinder: it fixes the canvas and the scale.

**What it can and cannot do (S1 and my reading):**
- **Can:** any shape built from repeated, transformed copies of a few terminals, at any dimension. S1 says sculptures from grammars defining 3D languages are shown in its Appendix (PDF p.14; the Appendix is PDF p.27-36, mostly figures, and was only skimmed).
- **Cannot (as defined here):**
  - remove a terminal once placed;
  - express numeric parameters, except through scale;
  - choose between applicable rules; no selection policy is defined, and the example stops whenever the end rule is chosen (PDF p.9).
- **Rule result does not fit:** there is no notion of "fit". A rule applies only where its left side matches exactly up to a similarity transform.
- **Randomness:** none is defined. Which rule to apply, and where, is left open.
- **Checkable before running:** the selection rule's level bounds; whether every area falls under exactly one painting rule (a partition test on set expressions) **(mine)**.

**Later criticisms, from secondary accounts (S3, S4):**
- Shape grammars match **sub-shapes**: a rule can match shapes that **emerge** from overlapping lines without having been placed as units. This makes automatic application hard (S3 PDF p.3).
- In architectural practice, shape-grammar derivations were usually steered by a person, which prevents rapid modelling of large areas (S3 PDF p.2).
- Architectural rules in the literature are powerful but abstract and under-specified, so only people can apply them (S4 PDF p.9).
- **S1b (Stiny 1980), the formal treatment of shapes and emergence, was not read.**

## 2. L-systems (S2)

**Core idea** (S2 PDF p.15-16, printed p.3-4):
- A string is rewritten by productions **in parallel**: every letter is replaced in the same step.
- Chomsky grammars, by contrast, rewrite one symbol at a time.
- Parallel rewriting is meant to model cells dividing at the same time.
- Geometry comes from a separate **turtle interpretation** of the final string (PDF p.18-19).

**Variants:**
- **Bracketed** strings make branches (§1.6).
- **Stochastic** (§1.7, PDF p.40-42):
  - each production has a probability in (0, 1], and for any letter the probabilities of its productions sum to 1;
  - each occurrence of a letter chooses independently, so different occurrences can take different productions in the same step (PDF p.40).
- **Context-sensitive** (§1.8, PDF p.42-47). A production applies only when the letter has given neighbours on the left, the right or both. Neighbours can be found across branches, and some symbols can be declared ignored for context. This is how signals (nutrients, hormones) travel through a structure.
- **Parametric** (§1.10, PDF p.52-57):
  - modules carry real-valued parameters;
  - productions have a **condition** (a logical expression on the parameters) and arithmetic expressions for the successor's parameters;
  - the condition language uses +, −, *, /, ^, relations, and and/or/not (PDF p.53);
  - an empty condition counts as true.

**What happens when nothing applies.** **A letter with no applicable production simply replaces itself.** For OL-systems this is the assumed identity production (PDF p.16, printed p.4); it is restated for context-sensitive systems (PDF p.42, printed p.30). Non-matching is not an error; the letter is carried forward unchanged.

**Repeatable randomness** (PDF p.42, printed p.30):
- The book notes that running the same stochastic L-system with **different seed values for the random number generator** gives a variation of the image. Implicitly, the same seed gives the same result.
- **Nothing is said about keeping results stable when the rules are edited** (mine: with one shared random stream, adding a production shifts every later draw).

**Expressiveness and limits:**
- **Can:** branching growth, signals, timed development, continuous parameters.
- **Cannot easily:** spatial constraints between parts. S3, from the architecture side, says L-systems make it hard to stop objects growing into each other, and that local decisions can have global effects (S3 PDF p.7). Within S2's Chapter 1, rules see only their string neighbours, not space **(mine)**.
- **Hard to write:** finding an L-system that produces a given structure (the **inference problem**). Known algorithms are too limited to be practical for higher plants (PDF p.23, printed p.11).

**Checkable before running (mine):**
- that each letter's production probabilities sum to 1;
- the conditions and arithmetic of parametric productions, for division by zero or out-of-range values, by interval arithmetic over parameter bounds (see C2);
- growth rates of string length, through growth functions (§1.9).

## 3. Split grammars: Instant Architecture (S3)

**The restriction** (S3 §4, PDF p.4). A set grammar over basic shapes (cuboids, cylinders, prisms, ...) with two rule types:
- **Split:** one shape is cut into several (for example an n × m × k grid with parameterised planes) that fill the same volume.
- **Conversion:** one shape is replaced by another that must lie **inside the original's volume**.

Because every new shape is contained in its parent, **objects cannot grow into each other**. The authors present this as the answer to the growth-control problem of L-systems (PDF p.7).

**Choosing among rules: attribute matching** (PDF p.5-7):
- Shapes and rules both carry attributes. A rule's attribute is an interval plus a containment flag. A shape's attribute adds a priority and a probability distribution.
- **Stage 1 (deterministic):** a rule is a candidate only if, for **every** attribute, the rule's and shape's intervals overlap (or satisfy containment when flagged). Otherwise its score is −∞ and it is excluded. Scores add up priorities.
- **Stage 2 (stochastic):** among the top-scoring candidates, pick the one with the highest product of densities (each shape distribution evaluated at the rule interval's midpoint) times one **per-rule random number**.
- **Geometric fit is an attribute.** Width, height and depth are computed for each shape automatically. A rule that needs a wider space than the shape provides is **excluded** (PDF p.6-7, rule 5 in Fig. 8).
- **"Only when asked" rules.** Setting the containment flag on a rule means the default "don't care" interval of a shape never satisfies it. Example: a blind (fake) window (PDF p.7).
- **Control grammar.** A separate small context-free grammar spreads attributes over regions in an orderly way (for example the ground floor, or every second column), so that design ideas are not scattered at random (PDF p.5).

**Repeatable randomness.** The random number for each rule is **drawn once per building**. The same rule then wins wherever the same attributes and candidates recur, keeping a building coherent (for example all windows alike) without storing a history (PDF p.6).

**Scale of use.** About 250 rules and 40 attributes; about two weeks to write the rule database; all the paper's buildings come from that one database (PDF p.7).

**Failure cases and limits, as stated:**
1. **Random rule choice gives chaos,** and more so as the database grows (Fig. 2, PDF p.2). Encoding every design choice in the grammar is infeasible (PDF p.2).
2. **Fine detail is out of range.** For example Corinthian capitals have to be modelled separately (PDF p.7).
3. **No answer is given for when no rule survives matching** (not found in the text; mine).
4. **Future rule types**, such as merging adjacent shapes, risk making the grammar unnecessarily complicated (PDF p.8).

## 4. CGA shape: procedural buildings (S4)

**Rule form** (S4 PDF p.3). Each rule has:
- an id;
- a predecessor symbol;
- a **guard condition**;
- a successor;
- a **probability**.

The rule applies only when its guard is true. Replaced shapes are kept as **inactive** shapes in a tree, not deleted, so rules can query the hierarchy (PDF p.3).

**Order.** Rules get **priorities** by level of detail, and the derivation always expands the highest-priority shape first. This is a modified breadth-first order that guarantees low detail before high detail (PDF p.3).

**Fitting to size** (PDF p.3):
- Split sizes can be **absolute** (do not scale) or **relative** (share what is left). A relative size gets its share of the scope's size minus the sum of the absolute sizes.
- **Repeat** tiles an element as many times as fit, rounding the count and stretching each tile to fill.
- **Component split** turns a volume into faces, edges or vertices.
- The authors note that a split dimensioned for one scope size must be scaled for others (PDF p.3).

**Context: what makes CGA more than a split grammar** (PDF p.4-5):
- **Occlusion queries.** Rule guards can ask whether the current shape is unoccluded, partly occluded or fully occluded. The question can be asked against all shapes, active shapes only, shapes with a given label, or all except the shape's own ancestors. Example: a tile becomes a window if clear, a wall if partly hidden, and nothing if fully hidden. Precise tests would need morphological operations, so approximations are used.
- **Sightline visibility,** for example toward the street.
- **Snap lines.** Splits may snap to dominant planes and lines, such as other volumes' faces, so façades line up (PDF p.5).

**Randomness** (PDF p.6). Rules use random draws in ranges and probabilistic alternatives, for example a 50% chance that a side wing matches the tallest solid. **How draws are seeded or kept stable is not described** (not found; mine).

**Failure cases and limits, as stated** (PDF p.1-2, p.8-9):
1. **Without occlusion, intersections cut windows wrongly.** Volumes placed by a stochastic grammar intersect and cut windows or other elements in unnatural ways (Fig. 2).
2. **Implausible shape configurations** are a general disadvantage of the procedural approach, especially from arbitrary GIS footprints (PDF p.9).
3. **Carelessly written rules** can be contrived, understood only by their author, and have **unwanted side effects** (PDF p.9).
4. **Small complex details** (roof bricks, capitals, window grills) were built in another modelling package (PDF p.9).
5. **Uncertain for design.** The authors say application to architectural design is unexplored and might need significant changes (PDF p.9).
6. **No global optimisation.** It might do better but would be harder to model and slower (PDF p.9).

**Strength claimed** (PDF p.9). Robust and efficient because there are no Boolean geometry operations, which are hard to implement reliably.

## 5. GML: generative modelling as a program (S5)

**What it is** (S5 PDF p.219, p.226):
- A small **stack-based** language in the style of PostScript.
- Shapes are stored as the operations that generate them, not as their result.
- Simple operations are combined into larger ones.

**Euler operators** (PDF p.75, p.84). Mesh changes are made with Euler operators (make or kill a vertex, edge, face, shell or ring). The thesis states a theorem: **Euler operators are closed and complete** on closed orientable manifold meshes with rings.
- **Closed:** applying one to a valid mesh always gives a valid mesh.
- **Complete:** every such mesh can be built with them.
- **Condition:** each operator must be used legally. For example, an edge with the same face on both sides cannot be deleted with the ordinary edge-face operator (precondition table, PDF p.79). Checking these preconditions is "a subtle and important issue" for implementers (PDF p.84).
- A valid mesh means every edge touches exactly two faces, every vertex has a cycle of edges, and the surface is orientable (PDF p.75).

**What it does not guarantee** (PDF p.88-89). Topology, not geometry. Meshes with the same connectivity can still **intersect themselves** once embedded, and such self-intersections are sometimes hard to detect (Fig. 2.22, PDF p.89). The thesis argues that restricting parameters to self-intersection-free ranges would be unnatural and computationally expensive, which is why graphics uses meshes rather than strict cell complexes (PDF p.89).

**Undo** (PDF p.46-49):
- Operations are grouped into macros. Undo and redo work macro by macro, which also gives a "semantic" level of detail.
- An earlier Python prototype had no invertible mesh tools. Undo deleted the last code lines, and every parameter change rebuilt the whole model from scratch, so interactivity faded beyond about 30 modelling steps. This taught the author to value invertible low-level operations (PDF p.46).

**Comments on grammars in S5's related work** (PDF p.35):
- With grammars, the user designs shape only indirectly. Non-experts find it hard to foresee what a rule set will do.
- Inconsistent results (doors on the second floor) appear for reasons that seem obscure.
- **A rule set may misbehave only in very specific circumstances, which makes a grammar debugger mandatory.**
- On GENMOD (Snyder 1992; see C2), S5 names two drawbacks: modelling requires programming, and there is no support for stitching parametric patches into manifolds (PDF p.38). S5 also notes that GENMOD evaluates every object with interval inclusion functions as well as point values (PDF p.38).

**Randomness.** Not discussed in the parts read (mine; not searched in full).

## 6. Comparison

| Question | Shape grammar (S1) | L-system (S2) | Split grammar (S3) | CGA (S4) | GML (S5) |
|---|---|---|---|---|---|
| Unit rewritten | Sub-shape found by similarity | Every letter, in parallel | One set member (a basic shape) | One shape (scope) | Program operations on a mesh |
| Parameters | Scale only | Real-valued (parametric) | Attributes as intervals | Expressions, guards | Full language |
| Context | Markers | String neighbours | Attributes, control grammar | Occlusion, visibility, snap lines | Anything the program queries |
| Rule does not fit | Does not match, so not applied | Module copies itself | Excluded by size attributes | Guard false, so the next rule is tried; relative sizes absorb the rest | Precondition violated (implementer must check) |
| Overlap and growth into others | Not addressed | Not prevented | **Prevented** by containment | Detected by occlusion queries, then handled by rules | Topology always valid; self-intersection possible |
| Randomness | None defined | Probabilities; seed | One draw per rule per building | Random ranges and probabilities | Not discussed in parts read |
| Hard to debug | Emergent sub-shapes | Inference; global effects | Chaos with many rules | Side effects of careless rules | Indirect design (S5's own view) |

## 7. Lane notes (mine, not from the sources)

1. **The containment rule (S3) is the strongest guarantee in this set that can be checked before running.** Every child lies inside its parent, so a whole derivation's bounds are known from the root. Combined with interval bounds on every parameter (C2), a plan can be bounded before any geometry is built.
2. **"Does not fit" should be an explicit, reported outcome,** not a silent skip (S2's self-copy) or a silent exclusion (S3's −∞). S4's guard-plus-fallback chain (window, wall, nothing) is the most legible model: every alternative is named.
3. **Repeatable randomness:** S3's one-draw-per-rule-per-building is the only stated scheme, and it buys coherence. For edit-stability, derive each draw from a hash of the rule id and the shape's path in the derivation tree, not from a shared stream. This is a design suggestion, not a sourced method.
4. **Debugging:** S4 keeps inactive shapes in the tree, and S5 has invertible macros. Both make every intermediate state inspectable, which S5 argues is mandatory.
