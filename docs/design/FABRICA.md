# Fabrica: a base model from numbers, its skeleton inside, detail sculpted on

The order in which Specula makes a person or a creature, set by the owner on 2026-11-28. It is a
plan and an order of work, not a new crate: every step is a piece that exists or is already on the
roadmap. This page says how they join, what changes in their order, and what is noted for later.

## The owner's words

> "At some point, could we consider putting procedurally generated generator? It may make it
> easier to set up models like these, as mathematcally, everything has their boundaries."
>
> "It may be the way to make 3D models much easier to generate, and you can then sculpt in
> additional stuff like nails, faces and hand."
>
> "So we generate a base model, with animation rigs inside already as skeleton, male/female,
> resize as needed. And adding sculp note. All of this in Rust, with numbers to help set up forms."

And, earlier, the standing wish this serves: a skeleton with every figure, "so that a character is a
change and not a rebuild".

## The order

**numbers, then the skeleton, then the base surface, then sculpted detail, then one mesh.**

| Step | What it is | Where it lives | State (2026-11-28) |
|---|---|---|---|
| 1. Numbers set up the form | stature, weight, sex and a few traits in; every other dimension follows, each inside its measured range | `specula-corpus` ([CORPUS.md](CORPUS.md)); for other creatures a kind's numbers ([EXEMPLARIA.md](EXEMPLARIA.md)) | people: built and reviewed; creatures: the frame (S0) is merged |
| 2. The skeleton comes from the numbers, and it is the rig | joint centres and bone lengths from the same numbers; nothing is fitted afterwards | the corpus's frame; a plan's bones | people: built; a plan declares its own |
| 3. The base surface is grown round the skeleton | one distance field for the whole body, meshed once, then simplified; skin weights from the same skeleton | the `body` shape in `specula-model` (stage S1); research R8-B1 to B4 | **next**, after the sculpt branch merges. The older figure ([FIGURE.md](FIGURE.md)) is built from columns and does not look right: its mends are made here, as the body's first test, and not as a lane of their own |
| 4. A resize is a change of numbers | the skeleton and the surface move together; a character is a change, never a rebuild | steps 1 to 3 | follows from 3 |
| 5. Detail is sculpted on at named places | a face, a hand, nails: pieces that are themselves named and bounded, added at anchors the base keeps | the shape library's forms as sculpt nodes ([FORMAE.md](FORMAE.md), the stage called 5b); the base keeps its anchors through meshing | open; moved up in part 4's order by this page |
| 6. One mesh, with its levels of detail and its rig | the detail is part of the one field, so a nail grows out of a finger and is not stuck on it | the sculpt mesher | its branch is in its last review round |

## What the bounds are for

Every number has a range, and a plan declares the relations its numbers must keep. So the engine can
say **before it builds** how large a thing can be, how many parts and triangles it can have, and
whether any draw of a kind could make a broken body: the sample library's reach does this today for
a plan's relations (EXEMPLARIA.md, "For the plans' lanes"). The same holds a level up: a base that is
bounded can promise the anchors a detail is attached to, and a detail that is bounded can promise it
fits there.

## What changes in the order of work

- **Stage S1 (`body`) is built with a person as its first test**, from the corpus's numbers and frame:
  the figure rebuilt as a `body` must measure as the figure does (its own test's table), and it is
  looked at in clay and curvature against the faults FIGURE.md lists (a trunk like a column, tubes
  for legs, a seam at the neck). "People last" in EXEMPLARIA.md's order becomes: **the base person
  comes with S1; people's kinds, faces, hair and the walk come later**, as they did.
- **The `body` keeps its anchors.** A body's named places (a fingertip, the face's region, a joint)
  come out of the meshing as anchors, as the shape library's forms carry theirs, so step 5 has
  something to attach to.
- **The form as a sculpt node (5b) comes before faces and hands**, not after everything else in
  part 4.
- A quadruped, a bird or a dragon is the same order with another skeleton and other numbers: their
  plans (EXEMPLARIA.md, section 3) do not change.

## Noted, not designed: a plan as data

Today a kind is data and a plan is Rust (`tree`, `wheeled`, `box_creature`). The owner's first
sentence above asks for the plan to be data too: parts, how they attach, repeat, mirror and taper,
every number bounded, so that a new sort of thing is set up without writing Rust. It is **not
designed and not started.** It is to be drawn out of three or four plans written by hand (trees,
machines, a quadruped), which are the evidence for what the format must say, and validated as Lex
validates gameplay rules ([LEX.md](LEX.md)): typed text, checked, compiled, the same on every
machine. Open before any design: which things it covers (rigid and branching things first; a skinned
creature's base through the `body`); how a join stays closed when it is declared and not coded; how
the reach's check of bounds generalises to every derived number; where motion comes from. It is
raised again when the third plan written by hand lands.
