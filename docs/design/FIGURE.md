# Figure: a person's surface from a measured body

Status: **in progress, slice 2 of M6 part 4** (2026-11-17). The shape `figure` builds and is closed; it
has been looked at twelve times and **does not look right yet** (what is wrong is listed below). It
is measured against its body by a test, it has a skeleton and has taken its first steps, and it has not
been reviewed by a model that did not write it. The numbers it is
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
| `arms`, `legs` | degrees each arm and each leg is held out (35 and 7): the pose it is modelled in. Arms from 0 to 60: hanging, they melt into the body's sides (a pose for measuring, not for looking); past 60 a hand's cut is not built |
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

The trunk is one column from the hips to the shoulders (its sizes on a curve through the sections,
its sections squarer than ellipses: flatter in front and behind, as a person's are),
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
| v12 | A skeleton from the same body, and a first walk: she strides, and the pelvis tears |
| v13 | Bound by the flesh: the tear is gone; the heel went to the shin (rings at the ankles) |
| v14 | A joint's plane divides its two bones: the heel is the foot's again |
| v15 | Each part lists the bones of what it meets: the ankles' rings are weaker; the back seen lit for the first time |
| v16 | After two reviews' mends, and a trunk of squarer sections: the waist measures right, a man's chest is flat |

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
hips, buttocks) within 3%; the girths of the waist, the buttocks, a thigh, the lower thigh, a calf and
the neck within 5% (measured: within 3.4% on four bodies; the waist within 1.6%); the armpit within 2%
of its height. Held loosely, each for a reason that is the figure's:

- **The chest's girth is a twentieth (women) to a sixth (men) short.** With ellipses for sections the
  waist was 6% short too and the chest a tenth to a fifth; a column's sections now have a squareness
  (`square`: 2 an ellipse, 3 the trunk, 8 nearly a box), which brought the waist and the hips to
  within 2% and the chest halfway. What is left at the chest is what is not there: the muscles that go
  from the chest and the back to the arms, over which a tape passes. No section as broad and as deep
  as the survey's chest can be as far round as its chest girth.
- **The legs part 2.5 to 5 cm below the measured crotch**: the two thighs' surfaces melt into each
  other at the top.

Not measured yet: the shoulders' breadth across the arms' muscles (the pose's arms are out), the arms'
girths, the head's part and the hands and feet.

Numbers (release engine, integrated adapter): 45,000 to 53,000 triangles a figure (at v7: body 18,700;
head 6,800; hands 7,300 each; feet 4,800 each), built in about a second, 0.6e9 to 0.67e9 of the 1.5e9
work units a build may spend (with the weights as recalibrated the same day:
[PROCEDURAL_MODELLING.md](PROCEDURAL_MODELLING.md), tier 3); 0.35 ms of GPU time a frame with two on
screen at 1280x720.

## The skeleton

A recipe's rig can be the same measured body (the owner, 2026-11-17: a skeleton with every figure, so
that a character is a change and not a rebuild):

```json
"rig": { "blend": 0.05, "figure": { "sex": "female", "stature": 1.7, "build": 0.5 } },
"parts": [
  { "name": "body", "bones": ["hips", "spine", "chest", "neck", "head", "upperarm_l", "upperarm_r",
                              "forearm_l", "forearm_r", "hand_l", "hand_r", "thigh_l", "thigh_r",
                              "shin_l", "shin_r", "foot_l", "foot_r"],
    "shape": { "op": "figure", "part": "body", "sex": "female", "stature": 1.7, "build": 0.5 } },
  { "name": "head",   "bones": ["chest", "neck", "head"], "shape": { "op": "figure", "part": "head", "...": "the same body" } },
  { "name": "hand_l", "bones": ["forearm_l", "hand_l"], "shape": { "op": "figure", "part": "hand_l", "...": "the same body" } },
  { "name": "foot_l", "bones": ["shin_l", "foot_l"],    "shape": { "op": "figure", "part": "foot_l", "...": "the same body" } }
]
```

`rig.figure` takes the `figure` shape's fields (`part` and `smooth` are not read) and gives eighteen
bones, the same for every body (`figure::BONES`): `root`, `hips`, `spine`, `chest`, `neck`, `head`, and
on each side `upperarm`, `forearm`, `hand`, `thigh`, `shin`, `foot` (`_l` is the figure's left, +X).
They are the names the motion generators and the foot placement look for, so a gait made with
`"body": "auto"` drives any figure. Each bone runs from one of the body's joints to the next, in the
pose the surface is made in, so it lies inside the surface by construction (a test holds it). A hand's
bone ends at the knuckles and a foot's at its ball: fingers and toes have none yet. A rig has `bones` or
a `figure`, not both.

**How a figure's vertices are bound.** Any other rig binds a vertex to the nearest bone among its
part's bones. The first walk (v12) showed why that is wrong for a body: the hips' sides are nearer a
thigh's bone than the pelvis's, so half the pelvis swung with each leg and the surface tore across it.
A figure's rig binds by its flesh instead (`Rig::bind_fleshed`, with `figure::flesh` for the radii):

- a vertex goes to the bone it is fewest of that bone's own radii from (half the hip breadth for the
  hips, a thigh's girth as a circle for a thigh, the ankle's reach to the heel for a foot);
- at a joint, a plane divides the two bones: a limb's bone gives up what is behind its joint, and
  where one bone carries on from another's end (thigh to shin, shin to foot) the plane is square to
  their mean direction and the parent gives up what is past it. So a heel, which is behind the ankle
  along the foot's line, stays with the foot.

**Each part lists the bones of what it meets.** The body's part lists every bone, the head's the
chest's too: where two parts meet at a cut, both sides must be allowed the same bones, or the same
place gets two different weights and the cut opens when the joint bends (the ankles showed dark
rings until the body was given the feet's bones; they are weaker since, not gone). A hand's and a foot's parts keep short lists: a fingertip
hanging beside a thigh must not find the thigh.

Tests hold that the hips' sides are led by the pelvis or the spine (and fail under plain distance),
that the thighs lead down the thigh, and that the heel is the foot's. The body fields are still written
seven times in such a recipe (the rig and six parts): one place for them is wanted.

Using it found two defects in the engine, of one root: it asked whether a build has clips of its
own where it should ask whether its meshes carry skin. A rig whose only clips are written into the
world (no anims file of its own) was filmed in its bind pose by `anim_filmstrip`, every tile alike,
while the clip moved it in the world; and such a figure, animated, did not wait for the skinned
pipelines, was drawn before they existed and was skipped by the GPU, so a capture taken straight
after it was spawned showed the world without it. Both are fixed in `models.rs` (`posed`,
`held_back_by`), each with a test, and an entity's filmstrip now refuses a model that is not ready.

An independent check of the walk's captures did not pass them. It confirmed the stride, the bent
knees, the frozen filmstrip before the fix and the moving one after, and the crease across the hips in
every tile. It also found seams that show while moving (a ring at the ankle, a notch at the wrist, lines
across the forearm, the shoulder and upper back pinched), a marching step with no heel-off and a
stride longer than a leg, the held-out arms, a stiff trunk, and that the motion checks said "OK" on all
of it: they measure sliding, floating and the loop's seam, and none asks whether a walk looks like
one.

## The review of slice 2

A model that did not write it reviewed everything this slice changed and said **do not merge**
(2026-11-17), with four major findings, each shown on a concrete case. All four are mended, and each
case is now a test:

1. **A column's field overstated distance where its sections grow steeply**, so a flange on a thin
   stem was meshed with samples on the wrong side. A column is now trusted by its steepest section.
   (The figure's own columns are gentle; 82 bodies at the limits showed no such sample.)
2. **The mesher kept a pruned tree for every quarter block for the whole build**: 188 MB for a body,
   up to 742 MB for a legal build. It keeps one a half block.
3. **Arms held closer than about 12 degrees cut the thighs out of the body** (a quarter of its volume
   at 0), silently: a hand's room, a box wider than the hand, was taken out of the whole body. Each
   fine part's room is now taken out of its own limb only.
4. **The root bone took the legs if a part listed it**, as this note said a body's part should: it
   was scored in metres against bones scored in their radii. A bone with no flesh takes nothing.

Its minor findings: a repeated child could be pruned away (mended: such a child is never dropped);
"bit for bit, colours included" was untrue for plain smooth unions by one unit in the last place
(the notes now say so); the tallest stature the survey covers was refused when written in a recipe
(mended, and a body's refusal now names its field with the number and the limit); and **a hand was
not always a closed surface at its default resolution**: where three of the mesh's vertices land
within a few micrometres of each other between two fingers, one tiny triangle was missing, leaving a
pinhole. The builder traced it that far and could not say why.

It also checked and found sound: pruning for exact shapes (no difference in 1.44 million points over
800 random trees), the cost weights, the bones inside the surface, and the three engine fixes.

**A second model reviewed the mends and said merge**, with five minor findings, and it **found the
pinhole's cause**: the mesher made the triangle, and the clean-up after it dropped any triangle under
the least area a triangle may have, whatever its corners, while its neighbours stayed. The mesher now
closes such a triangle by its shortest edge, so the two triangles on that edge go together; every
part of six bodies at the limits is closed at its default resolution, and the hands at two others
(a test holds it, with no allowance). Its other findings, mended: a column whose breadth and depth
change in different proportions was pruned by its distance alone, though far out to one side its
field changes much faster along it (now pruned with an allowance for how far out a point is, which
costs the figure about a seventh more work); a smooth plain union with a twisted or repeated child
could differ in the fifth place (it keeps every child); a weight at its limit was refused as a
stature had been; a trait's refusal did not name the trait. One it found stays as it is, and is said
here: **with the arms hanging (under about 10 degrees) the forearm's cut end melts into the thigh and
leaves a web of a few cubic centimetres past the wrist's cut** that the hand's part does not cover.
Hanging arms are a pose for measuring.

## What is wrong, and what is not there

An independent check of v11 did not pass it either. It confirmed that the limbs' rings and the boxy
crotch are gone and that the legs part with a gap; it failed a man's chest (a third try at a swell read
as one mound, and is taken out: a man's chest is the plain column) and the back view (unlit, the
capture's fault twice). Its list, and what the numbers add:

- An independent check of v16 (after both reviews and the squarer trunk) confirmed that a man's chest
  is flat now and found **a new fault that came with the squarer trunk: a pale band with squared
  corners across a man's upper chest, like a plate**. It could not see the trunk's flatter front in
  frontal light (the measure shows it; a side light is wanted), and the row from behind was again
  taken unlit.
- The trunk is a straight column: ruler-flat at the back and the belly,
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
- The back, seen lit at last (v15), is flat: no shoulder blades, no curve in the lower back.
- Eight bodies have been looked at from afar, and only the two typical ones up close.
- The levels of detail have not been looked at. A hand is 7,300 triangles.
- The pelvis no longer tears when a leg swings (v13 to v15; an independent check confirmed that
  and passed nothing else as described). A V-shaped line shows at the groin in every tile, crease
  or step it could not tell; the buttock on the bent leg's side swells into a ball with a notch under
  it; the calf's outline steps inward at the cut above each foot, like a cuff, and dark ticks remain
  at the ankles and across a forearm; a hard seam runs under the skull like a helmet's rim. The arms have no
  rest pose at the sides; fingers and toes have no bones; no motion has been made from walking data yet
  (the walk is a lunge from guessed numbers; the measured curves are in the library, H1).
- Held against pictures of people only by eye so far (Richer's plates, 2026-11-17); no outline has
  been compared at one scale.
- The body and the hands may join two sheets along a few edges (the tops of the thighs, between
  fingers): closed, and not a clean surface there.
