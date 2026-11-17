# Corpus: the human body as numbers

Status: **built, slice 1 of M6 part 4** (2026-11-17). The crate is `specula-corpus`. It holds what a
human figure can be measured against and built from: dimensions, a frame of joints, what each part
weighs, and two checks. The surface that wears these numbers (the mesh, its rig) is the next slice, in
the modelling crate.

Why it exists is in [ROADMAP](../ROADMAP.md), M6, "Form within an agent's reach": an agent describes a
figure in numbers and cannot see whether an arm is too long. The owner's rule for it (2026-11-16): the
proportion tables are "written first, as data, from published body measurements that are free to use,
each with its source", and "looked up, not written from an agent's memory".

## Where each number comes from

| What | Source | How it got here | How it is held |
|---|---|---|---|
| 93 body dimensions and age, of 1,986 women and 4,082 men | ANSUR II, the US Army's survey of 2010 to 2012: its two public data files. A US Government work, public domain | **Computed**, never typed: `cargo run -p specula-examen -- corpus tables FEMALE.csv MALE.csv` reads the files and prints `crates/specula-corpus/src/ansur_data.rs`. The files are fetched by whoever runs it and are never in this repository | A test holds 22 dimensions' mean, spread and 5th and 95th percentiles against the survey's printed report (Gordon et al. 2014, NATICK/TR-15/007), and 8 fitted lines against the library research, which computed them separately ([A1](../research/library/A1-segment-lengths.md)) |
| Where a joint's centre is from the landmark a tape finds | de Leva 1996, J Biomech 29(9):1223-1230, Table 2 (millimetres on his subjects), scaled to the person's stature | Typed into the tool, read on the image of the printed page | The upper arm between centres comes to 0.163 of stature on the survey's women against de Leva's own 0.159, and the shank to 0.249 against his 0.249 (a test) |
| What each part weighs, where its centre of mass is, its radii of gyration | The same paper, Table 4 | Typed (`inertia.rs`), read on the image of the printed page | Three tests: every number against the research's own typing of the same rows ([A4](../research/library/A4-segment-masses-com.md), another hand and a second checker); the five segments the paper gives between two pairs of endpoints agree in millimetres; the trunk's parts give the paper's whole-trunk centre |
| How far apart the hip joints are | Bell, Brand and Pedersen 1989: each centre 0.36 of the breadth between the front points of the hip bones from the middle. That breadth: de Leva's Table 3 | Typed into the tool. **Bell's paper was not opened**: the figure is as the motion-capture literature gives it | - |
| Where the ankle is along the foot | Hajaghazadeh et al. 2018, Health Scope 7(3):e14063 (580 students' feet, Urmia, Iran): the middle of the two malleoli | One number, 0.245 of the foot's length from the heel, for both sexes (`body.rs` says why not the survey's two) | - |

## What a row is

One dimension of one sex: its mean, standard deviation and 5th and 95th percentiles; its line on
stature, with the spread around the line and the correlation; its plane on stature and weight, with
the spread around that; what each of fourteen traits adds; and what spread is left.

Besides the survey's own 93 there are 20 **links**, made by the tool for each person before anything
is averaged: the survey's own (thigh link, calf link and the like, by its report's subtractions) and
the skeleton's, between joint centres (upper arm, forearm, hand, thigh, shank, trunk, and the heights
of the shoulder, hip and knee centres).

## Traits

Stature and weight say most of a body. What tells two people of one stature and weight apart is
carried by fourteen named traits: age, legs, arms, shoulders, hips, chest, waist, neck, biceps,
thighs, calves, head, hands, feet.

- A trait is read on one dimension (legs on the hip centre's height, shoulders on the biacromial
  breadth). It is **what is left of that dimension** once stature, weight and the traits before it
  are accounted for.
- So in the people measured the traits are uncorrelated with each other, and one unit is one
  standard deviation. A crowd is made by drawing each trait at random about zero; that is an
  approximation (uncorrelated is not independent, and age is not spread as a bell: most soldiers
  are young).
- A trait moves **every** dimension that goes with it in those people, by as much as it does in
  them. Long legs come with a longer thigh and shank and a shorter trunk. Big hands come with big
  feet. Age at the same stature and weight comes with a wider waist and longer ears.
- Every row is computed over the same people, and every step is linear. So what adds up on a
  person adds up on a body: floor to ankle, shank, thigh, trunk and head give the stature again,
  whatever the traits (tests hold it).

This is what the published body models do with thousands of scans, done here with a tape's
measurements: less shape, no licence, and every number traceable.

## A body, a frame, the checks

- `Body::with_build(sex, stature, build)` or `Body::new(sex, stature, weight)`, then `with_trait`
  and `with_age`. `dimension(name)` gives any of the 114 on that body; `spread(name)` gives what the
  body does not say of it.
- `Body::frame()` is the 29 joints it stands in, in metres (+Y up, facing +Z, left is +X).
- `parts(sex, frame)` weighs any posed frame: sixteen parts with their centres. `centre_of_mass`
  sums them.
- `proportions(sex, frame)` holds a figure's eleven lengths against people of its own stature, each
  as a number of the survey's spreads, whatever the pose (a hand is measured by way of its
  knuckle, a foot by way of its ball, the trunk along the spine). It judges nothing: a hero eight heads tall is out of
  proportion on purpose, and a project's style says what it allows.
- `balance(sex, frame)` says how far the centre of mass is inside what the planted feet cover. For a
  still figure only.

`cargo run -p specula-corpus --example body -- female 1.70 0 legs=1.5 age=40` prints all of it for
one body, and `... -- traits` what each trait moves most.

## Where it stops

- **Who was measured.** Soldiers of 17 to 58, fit for service. A body may be as short and as tall
  as the shortest and the tallest of them (1.409 to 1.829 m for women, 1.491 to 1.993 m for men),
  as light and as heavy for its stature as the lowest and the highest body mass index among them
  (16.4 to 40.8 and 15.4 to 43.5: so a very tall body may weigh more than anyone measured did),
  three standard deviations either way on a trait, and of their ages. Past that `Body` refuses,
  with the range.
- **Many traits pushed far at once.** The rows are straight lines, and enough of them pushed
  together carry a body where hardly anyone is (the navel over the bottom of the ribs: two of
  the men measured have it there, by a millimetre or two). Every body
  is checked as it is made and refused by the part that gave way (`folded`, `not_positive`).
  With the traits no further than 4 from the average together (the root of the sum of their
  squares) that cannot happen at any stature, weight and age: a test proves it over the whole
  range. Three people in four of those measured are within 4 (computed from the data); the
  furthest is at 7.6. One by one, traits are checked as each is set, so far from the average the
  order can decide; `with_traits` sets several and checks once.
- **One person in eighteen of those measured cannot be made**: 331 of the 6,068 have one trait
  past three standard deviations, which is where a single trait stops.
- **Flesh can come out narrower than bone.** Each row is its own straight line, so at the edges
  two of them can cross: the tallest, lightest woman with the broadest shoulders has a breadth
  over the deltoids (428.7 mm) under her breadth between the acromia (430.0 mm). Whatever builds
  a surface from the rows has to hold flesh to bone itself.
- **No children and no old age.** Growth tables are in the research
  ([A2](../research/library/A2-proportions-by-age.md)); proportions by age are thin there.
- **The frame is flat.** Everything but the feet is in one plane: the spine's curves are not in
  it. So the standing centre of mass falls a millimetre ahead of the ankles, with a quarter of a
  foot's length behind it and three quarters ahead: no measurement here says where a person's
  falls, and a figure with a spine's curves will put it elsewhere.
- **No head count.** The survey measured no chin-to-top height, so "heads tall" has no row.
- **de Leva's women are fifteen.** The mass shares for women rest on few people. The trunk's
  three parts are cut where his subjects' were, not at the frame's own waist (his women's lowest
  cut is 0.295 of the trunk above the hips; the navel of the survey's women is at 0.241).
- Not here yet: a face, fingers and toes, the range each joint turns through, the surface.
