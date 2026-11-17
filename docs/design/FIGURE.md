# Figure: a person's surface from a measured body

Status: **in progress, slice 2 of M6 part 4** (2026-11-17). The shape `figure` builds and is closed; it
has been looked at eight times in a day and **does not look right yet** (what is wrong is listed
below). It has no rig, and it has not been reviewed by a model that did not write it. The numbers it is
built from are [CORPUS.md](CORPUS.md)'s.

## What it is

A shape of the modelling crate ([PROCEDURAL_MODELLING.md](PROCEDURAL_MODELLING.md), tier 3). One part of
a recipe is one part of a person:

```json
{ "recipe": 1, "name": "walker", "collider": "none", "smooth_angle": 60,
  "parts": [
    { "name": "body",   "material": "skin", "shape": { "op": "figure", "part": "body",   "sex": "female", "stature": 1.70, "build": 0.5 } },
    { "name": "head",   "material": "skin", "shape": { "op": "figure", "part": "head",   "sex": "female", "stature": 1.70, "build": 0.5 } },
    { "name": "hand_l", "material": "skin", "shape": { "op": "figure", "part": "hand_l", "sex": "female", "stature": 1.70, "build": 0.5 } },
    { "name": "hand_r", "material": "skin", "shape": { "op": "figure", "part": "hand_r", "sex": "female", "stature": 1.70, "build": 0.5 } },
    { "name": "foot_l", "material": "skin", "shape": { "op": "figure", "part": "foot_l", "sex": "female", "stature": 1.70, "build": 0.5 } },
    { "name": "foot_r", "material": "skin", "shape": { "op": "figure", "part": "foot_r", "sex": "female", "stature": 1.70, "build": 0.5 } } ] }
```

| Field | |
|---|---|
| `sex` | `female` or `male` (the survey tabulates two) |
| `part` | `body`, `head`, `hand_l`, `hand_r`, `foot_l`, `foot_r`, or `whole` (everything on one coarse grid, with mittens and shoes: for a figure seen from afar) |
| `stature` | metres; left out, the mean of the sex |
| `build` or `weight` | the weight as standard errors from the usual one for the stature (0), or in kilograms |
| `age` | years, 17 to 58 |
| `traits` | `{ "legs": 1.2, "shoulders": -0.8 }`, in standard deviations ([CORPUS.md](CORPUS.md), "Traits") |
| `arms`, `legs` | degrees each arm and each leg is held out (35 and 7): the pose it is modelled in |
| `smooth`, `resolution` | how far parts melt into each other; cells along the part's longest side (body 112, head 40, hands 56, feet 48, whole 72) |

A body the survey does not cover is the recipe's error, in the body's own words (a stature nobody had,
traits that fold it).

## Why parts

One grid cannot hold a finger beside a trunk: a finger is 9 mm and a trunk's cell 15 mm. Each part is
meshed on a grid as fine as it needs. A fine part holds **the body's own limb** (the same column: the
forearm in a hand, the shank in a foot, the neck in a head) and is cut by a box a little way up it; the
body is cut by the same box, four millimetres short of it. So the two meet along one thin line, each
cut's face inside the other, and no surface is made twice (two copies of one surface on two grids show
as a ragged line: the third to the seventh looks have it).

## Where each size comes from

- **Measured.** A girth of the survey is a radius (the girth over two pi) at the height it was measured
  at. The trunk's sections are as broad and as deep as the survey's breadths and depths at the hips, the
  waist and the chest. Joint places are the body's frame.
- **A finger's bones** are in the proportions Buryanov and Kotiuk measured on radiographs of 66 adults
  (2010, Int J Morphol 28(3):755-758), scaled to the body's own hand. Their article is licensed
  non-commercially; its measurements are used as cited facts
  ([research/library/A6-hands-feet.md](../research/library/A6-hands-feet.md),
  [REVIEW.md](../research/library/REVIEW.md), "Rights").
- **A palm's thickness** follows from its breadth and its girth, taken as two flat sides with a half
  circle at each end.
- **The engine's own, judged by looking and by nothing else**: where a limb is thickest along its
  length; how a knee and an elbow narrow; the size of the buttocks' and the chest's swell (and that a
  man's is flatter); the slope from the neck to the shoulder; the head's height below the ear; the
  lengths of the small toes; the pose's angles. None of these is a measurement, and each is a number
  in `figure.rs` beside a comment that says so.

## What it is made of

The trunk is one column from the hips to the shoulders (its sizes on a curve through the sections),
with an egg for each buttock, one for each breast on a woman, and a round cone sloping from the neck's
base to each shoulder. Each limb is two columns. A thigh's top is an ellipse, not a circle: two thighs
lie side by side within the hips' breadth, so each reaches from a finger's breadth short of the middle
line to the hips' side and is as deep as its girth leaves. A hand is four metacarpals side by side, three bones to each finger
and a thumb; a foot a heel, an instep, a flat body and five toes; a head a skull as broad and as long
as measured and a jaw. Everything is joined by near-unions.

## The looks so far

Every version is captured in the engine as it first comes out and kept, the bad ones too (the owner,
2026-11-17), with its numbers and what was wrong: `gallery-full/part4/figure/NOTES.md` on the
development machine (the gallery's originals are not tracked). In short:

| Look | What it showed |
|---|---|
| v0 | One coarse grid, circles for sections: tube mannequins, a broken cone for a hand, hooves |
| v1 | Six parts on their own grids: real hands and feet; a stepped barrel of a trunk; rings on the limbs |
| v2 | Columns: the first that reads as a person; ball shoulders, a pipe neck, cuffs at wrist and ankle |
| v3 | Joins by one surface: the cuffs gone |
| v4 | A wrong turn at the shoulders: a shelf with a pipe on it |
| v5 | More sections for the slope: still a shelf |
| v6 | A muscle from the neck to each shoulder: they slope |
| v7 | The ankle's ledge found by measuring pixels and removed; a man's chest flattened |
| v8 | The body cut where the fine parts begin; a foot with an arch; kneecaps. **Eight different bodies in a row** (slender, heavy, older, athletic, long-legged, broad) |
| v9 | Each trait as a pair of bodies alike but for it: each differs where its trait says |
| v10 | A wider join of legs and trunk: looked better, **measured worse** (hips 4 to 6 cm wide, the crotch filled in by 10 cm); taken out |
| v11 | Mended by measuring (below): limbs with no rings, thighs as ellipses, depths as surveyed |

A model that did not take the captures checked v6 and **did not pass it as it had been described**: the
joins still showed, and the waist is not narrower than the chest. It checked v8 too, and did not pass that
as described either: the eight differ in height, girth and shoulders as their numbers say, but where the
legs begin could not be told apart (the long-legged body's hip joints are 46 mm higher by its rows, which
is 7 pixels in that picture); the joins are thin lines now and still seams; neighbours' hands overlapped
in the row. The same numbers showed the figure's crotch 5 cm under the measured one (the trunk's rounded
bottom), which is now cut at the measured height.

## Measured against its body

The reference pictures came on 2026-11-17 ([the index](../research/library/reference-images/README.md)).
Holding them beside the captures showed what the eye could and could not settle, and the figure is now
**measured**: a test builds the body's mesh, cuts it with a level plane at each height the survey
measured people at, and compares the breadth, the depth and a tight tape's girth of each closed curve
with the body's own rows (`figure::tests::the_figure_measures_what_its_body_does`; `figure_measures`
prints the table). Before it, the chest and the waist were 5 to 7% shallow, a man's hips 6% wide, a
thigh up to 12% thin and the legs parted 5 to 8 cm low, and none of it had been seen.

What the test holds now, on a typical woman and a heavy man: every breadth and depth (chest, waist,
hips, buttocks) within 3%; the girths of the buttocks, a thigh, the lower thigh, a calf and the neck
within 5%; the armpit within 2% of its height. Held loosely, each for a reason that is the figure's:

- **The waist's girth is about 6% short and the chest's a tenth (women) to a fifth (men) short.** The
  trunk's sections are ellipses; a person's are squarer, and at the chest a tape also passes over the
  muscles that go to the arms. No rounded section as broad and as deep as the survey's chest can be as
  far round as its chest girth. The mend is a squareness for a column's sections and those muscles:
  not built.
- **The legs part 2.5 to 5 cm below the measured crotch**: the two thighs' surfaces melt into each
  other at the top.

Not measured yet: the shoulders' breadth across the arms' muscles (the pose's arms are out), the arms'
girths, the head's part and the hands and feet.

Numbers (release engine, integrated adapter): 45,000 to 53,000 triangles a figure (at v7: body 18,700;
head 6,800; hands 7,300 each; feet 4,800 each), built in about a second, 0.6e9 to 0.67e9 of the 1.5e9
work units a build may spend (with the weights as recalibrated the same day:
[PROCEDURAL_MODELLING.md](PROCEDURAL_MODELLING.md), tier 3); 0.35 ms of GPU time a frame with two on
screen at 1280x720.

## What is wrong, and what is not there

An independent check of v11 did not pass it either. It confirmed that the limbs' rings and the boxy
crotch are gone and that the legs part with a gap; it failed a man's chest (a third try at a swell read
as one mound, and is taken out: a man's chest is the plain column) and the back view (unlit, the
capture's fault twice). Its list, and what the numbers add:

- The trunk is a straight column: too round by the tape (above), ruler-flat at the back and the belly,
  a sharp corner where the buttock starts, no curve in the lower back (the frame is flat), no ribcage,
  navel or collarbones. (That the typical man has no waist is the survey: 28.9 cm at the chest, 32.6 at
  the waist, 34.6 at the hips.)
- A hard crease where the skull sits on the neck, the head a ball cut flat below with no jaw from the
  front; the head is an egg (a face is its own work, for its own lane).
- Cuff lines at the wrists and ankles where each fine part is cut; a faint edge across the pelvis and
  a dimple above the crotch.
- Legs as tubes: knees nearly as wide as the upper thigh, little calf, no fold under the buttock, no
  ankle bones. Arms: a step at the elbow, the forearm fatter than the upper arm there, no elbow or
  biceps; the shoulders small on a sloping line. A man's knees read as creases.
- The back has not been seen lit.
- Eight bodies have been looked at from afar, and only the two typical ones up close.
- The levels of detail have not been looked at. A hand is 7,300 triangles.
- **No rig.** The frame's joints are the bones' places; binding is not written.
- Held against pictures of people only by eye so far (Richer's plates, 2026-11-17); no outline has
  been compared at one scale.
- The body and the hands may join two sheets along a few edges (the tops of the thighs, between
  fingers): closed, and not a clean surface there.
