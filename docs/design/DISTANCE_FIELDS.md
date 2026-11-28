# Distance fields: what the engine relies on, with proofs

The sculpt modeller builds shapes from distance fields
([PROCEDURAL_MODELLING.md](PROCEDURAL_MODELLING.md), tier 3), and the shape library's forms are
to become nodes of its tree (the library's own page, `FORMAE.md`, comes with its branch). This
page states each fact that work
leans on and proves it here, from first principles, in the engine's own words.

**Why it is written here.** The facts are standard, but several of the papers that hold them
are behind paywalls, the best-known free write-ups carry no licence for their text, and one
figure (the cubic blend's) is stated as a result in no peer-reviewed paper that seven passes
of search could find ([the citations](../research/library/I6-distance-fields-citations.md)). A
proof needs no one's permission. Everything below is original to this repository and carries
its licence (MIT or Apache 2.0); the sources are named at the end for credit and for further
reading, and nothing is copied from them.

**How it is held.** A proof on a page can be wrong. Every numbered statement of sections 2 to
5 is also checked in numbers by `crates/specula-model/tests/distance_field_facts.rs`, which uses
none of the modeller's code: it holds each bound against a nearest point found by search, and
each "reached exactly when" against the case that reaches it. Section 7's are held by the tests
of the code it is about, against a reference written apart from it and against nearest points
found by search.

## 1. Words

- A **solid** is a closed set `S` in the plane or in space, not empty and not everything, that
  is the closure of its own inside (no stray sheets, threads or points). Its **surface** `∂S`
  is its boundary.
- `d(x, A)` is the distance from a point `x` to a closed set `A`: the least `|x − a|` over
  `a` in `A`.
- The **signed distance** of `S` is `s(x) = d(x, ∂S)` outside `S` and `−d(x, ∂S)` inside:
  how far to the surface, counted negative within. Outside, `d(x, ∂S) = d(x, S)`.
- A function `f` is **1-Lipschitz** when `|f(x) − f(y)| ≤ |x − y|` for all `x`, `y`: it never
  changes faster than one unit for one unit moved.
- A field `f` is a **bound** for `S` when it has the sign of `s` (negative or zero on `S`,
  positive off it, zero on `∂S`) and `|f(x)| ≤ |s(x)|` everywhere: it may fall short of the
  distance and never exceeds it. It is **exact** where `f = s`.

The modeller's rule, which every node of a sculpt tree is held to: **outside a shape its
field is exact where the node can make it so, and everywhere it is a bound.** A field that
exceeds the distance makes the mesher skip space that holds surface. A field that falls
short costs time and is safe.

## 2. A distance never changes faster than one for one

**2.1.** For a closed set `A`, `x ↦ d(x, A)` is 1-Lipschitz.

*Proof.* Let `a` be a point of `A` nearest `y`. Then
`d(x, A) ≤ |x − a| ≤ |x − y| + |y − a| = |x − y| + d(y, A)`. Swap `x` and `y`. ∎

**2.2.** The signed distance `s` of a solid `S` is 1-Lipschitz.

*Proof.* If `x` and `y` are both outside or both inside, this is 2.1 for `A = ∂S`. Let `x`
be outside and `y` inside. On the segment from `y` to `x`, take the last point `p` that lies
in `S` (there is one, since `S` is closed); points just past it are outside, so `p` is on
`∂S`. Then `s(x) − s(y) = d(x, ∂S) + d(y, ∂S) ≤ |x − p| + |p − y| = |x − y|`, the last
because `p` is on the segment. ∎

**2.3. A 1-Lipschitz field never exceeds the distance to its own zero set.** If `f` is
1-Lipschitz and `Z`, the set where `f = 0`, is not empty, then `|f(x)| ≤ d(x, Z)`.

*Proof.* For any `z` in `Z`, `|f(x)| = |f(x) − f(z)| ≤ |x − z|`. Take the nearest `z`. ∎

So a 1-Lipschitz field with the sign of `s`, zero on `∂S`, is a bound for `S`. This is the
one property the mesher needs: where `|f(x)| > r`, no surface lies within `r` of `x`.

**2.4. Moving and turning keep everything; scaling evenly keeps everything.** If `f` is the
signed distance (or a bound) of `S` and `M` is a move or a turn, `f(M⁻¹x)` is that of
`M(S)`. For an even scale by `σ > 0`, `σ·f(x/σ)` is.

**2.5. Scaling unevenly keeps a bound, at a price.** Let `f` be a 1-Lipschitz bound for `S`,
let `S′` be `S` scaled by `σᵢ > 0` along each axis, and `g(x) = f(x₁/σ₁, x₂/σ₂, x₃/σ₃)`.
Write `σ_min` and `σ_max` for the least and the greatest of the `σᵢ`. Then `σ_min · g` is a
1-Lipschitz bound for `S′`. If `f` is exact outside `S`, then outside `S′` it reads short by
no more than the ratio of the scales: `(σ_min / σ_max) · d(x, S′) ≤ σ_min · g(x) ≤ d(x, S′)`.

*Proof.* `|g(x) − g(y)| ≤ |(x − y) / σ|` taken coordinate by coordinate, which is at most
`|x − y| / σ_min`; so `σ_min · g` is 1-Lipschitz, it has the sign of `f` at the unscaled
point and is zero on the scaled surface, and 2.3 makes it a bound. For the lower limit, let
`y` be `x` unscaled and `a` the point of `S` nearest `y`, so that `|y − a| = f(y) = g(x)`.
The scaled point `a′` is in `S′`, and `|x − a′| ≤ σ_max |y − a| = σ_max · g(x)`. So
`d(x, S′) ≤ σ_max · g(x)`. ∎

**2.6. The ellipsoid's field is no bound inside, and no factor holds it.** Let `E` be the
ellipsoid of half axes `e₁, e₂, e₃`, with least `e_min`, greatest `e_max` and ratio
`ρ = e_max / e_min`. The modeller's field for it is `f(x) = k₀ (k₀ − 1) / k₁`, with
`k₀ = |x / e|` and `k₁ = |x / e²|` taken coordinate by coordinate. Write `s` for the signed
distance of `E`. Then:

1. *Outside, `f` never exceeds the distance:* `0 < f(x) ≤ s(x)`.
2. *Inside, it may read up to `ρ` times too deep, and never less than `1 / ρ` of the depth:*
   `(1 − k₀) e_min ≤ −f(x) ≤ (1 − k₀) e_max` and `(1 − k₀) e_min ≤ −s(x) ≤ (1 − k₀) e_max`.
3. *At the surface its slope is one; near the middle it has no bound*: as `x` goes to the
   middle along a direction `u`, `f` goes to `−|u / e| / |u / e²|`, which is `−e_max` along
   the longest axis and `−e_min` along the shortest. So within any distance of the middle
   `f` changes by `e_max − e_min`, and no factor bounds how fast it changes.

*Proof.* (1) Let `x` be outside (`k₀ > 1`) and `y = x / k₀`, a point of the surface. The
gradient of `k₀²` at `y` is `2 y / e²`, so the plane touching `E` at `y` has the normal
`n = x / e²` (a multiple of it), and `x` lies `(x − y) · n / |n| = (1 − 1/k₀) (x · x/e²) / k₁
= (1 − 1/k₀) k₀² / k₁ = f(x)` beyond it. `E` is convex, so it lies wholly behind that plane,
and every point of `E` is at least `f(x)` from `x`. (2) `k₀` changes by at most `1 / e_min`
for each unit moved (each coordinate is divided by at least `e_min`), so the ball of radius
`(1 − k₀) e_min` about `x` stays where `k₀ < 1`, inside `E`; and `x / k₀` is on the surface,
`(1 − k₀) |x / k₀| ≤ (1 − k₀) e_max` away. That is the depth's pair. For the field's:
`−f = (1 − k₀) · k₀ / k₁`, and `k₀ / k₁ = |x / e| / |x / e²|` lies between `e_min` and
`e_max`, since each coordinate of `x / e` is that of `x / e²` times its `eᵢ`. (3) Both `k₀`
and `k₁` are of degree one in `x`, so `k₀ / k₁` depends only on the direction; at the middle
`k₀` goes to nothing. ∎

So `|f(x)| ≤ ρ · |s(x)|` everywhere, and near the middle of a flat one it comes close (the
most measured by sampling on 2026-11-24, so less than or equal to the true greatest: 0.90 of
`ρ` at `ρ` = 2, 0.83 at 3, 0.82 at 5, 0.76 at 10, 0.60 at 25 and 0.57 at 100), and the
slope is unbounded inside for every `ρ > 1` (measured: 5.5 at `ρ` = 1.5, 520 at 100); outside,
far from a flat one, it reaches hundreds (950 at `ρ` = 100). **The modeller once took a
factor of 2 for it**; 2.7 says what that cost.

**2.7. What skipping and pruning need, and what a factor gives.** The mesher fills a block of
samples by the sign at its middle `c` when no surface lies within the block's reach `R`; it
prunes a child out of a union over a ball when the child's least value in the ball is above
the others' greatest. Both rest on a **bound of the field over a ball**. A field `f` with
factor `L` (that is, `|f(x) − f(y)| ≤ L |x − y|`) and slack `σ` gives one at once: over the
ball of radius `R` about `c` it lies within `f(c) ± (L R + σ)`; and joins, meetings, cuts,
smooth joins, swellings, shells and noise keep a factor (3.1, 5.3, 5.6; a constant moves
nothing; noise of amount `a` adds `2a` to `σ`). **A field with no factor gives none**: by 2.6
an ellipsoid's middle may read `e_max` deep at a point `e_min` from its surface, so a block
there passes the mesher's test `|f(c)| > L R + σ + h` (with `L` = 2 and `h` a cell) with
surface in it (an ellipsoid of half axes 0.5,
0.02 and 0.5, meshed at 60 with a block's middle on its middle plane, came out 9,010
triangles where it is 8,290: four blocks filled solid, a bulge each side). Nor is a ratio
`|f| ≤ ρ |s|` enough for a tree: a swelling, a shell, a smooth join or noise moves the surface
to another level of the field, and the ratio says nothing of other levels (for every level,
it is the factor itself).

So the modeller takes the bound over a ball **from the node, not from one factor**, wherever
a node's factor is none (`field_range`, worked out through the tree: [PROCEDURAL_MODELLING.md](PROCEDURAL_MODELLING.md),
tier 3). For the ellipsoid it uses 2.6 itself. Its least over the ball of radius `r` about
`x`: as `field_range` has it from where its field is at most a level. Its greatest: let
`t = |x| (1 − 1/k₀)` outside (the distance to `x / k₀`, a point of the surface, so at least
`s(x)`) and `t = −(1 − k₀) e_min` inside (at least `s(x)`, by 2.6 (2)); then `s ≤ t + r`
over the ball (2.2). Where `t + r ≥ 0` the field is at most `t + r` (outside by 2.6 (1); and
inside it is below nothing). Where `t + r < 0` the whole ball is inside, at least
`−(t + r)` deep, and the field is at most `(t + r) / ρ` (by 2.6 (2): `−f ≥ (1 − k₀) e_min ≥
−s / ρ`).

**What is not proved here.** Two other parts have no factor either, and the modeller asks
their range too, but their ranges rest on reasoning in the code, not on a statement of this
page. A column whose breadth and depth change in different proportions changes, far to one
side, faster than its factor (`column_lip`) by an allowance for each of its own radii out
(`column_wander`); `field_range` bounds it by that allowance, and that bound is **held by a
test** (`a_fields_range_over_a_ball_holds_every_value_in_it`, and the mesher's
`every_primitives_factor_bounds_it_or_its_range_is_asked`), **not proved**. A repetition's
field jumps at its cells' edges, and `field_range` goes to every copy a ball can reach (at most
eight; past that it says nothing). And **pruning still takes such a column's span from
`column_wander`'s allowance, not from a range** (`sdf::pruned`): the review of 2026-11-24
found no tree where that is wrong, but it is not proved. What would close both: a proof that a
column's field is within `L` times its signed distance with `L = sqrt(1 + (κ g)²)` (`g` the
steepest growth of either radius along it, `κ = 2^(1/2 − 1/s)` for squareness `s`, how far a
point of a superellipse moves when its radii do), including its rounded ends, and a range
built on it for both the mesher and pruning.

**What the code adds to these ranges, and why it changes nothing above.** A column's range
reads its box of level 0 and its ratio from one walk of its curve, made before any range is
asked (`outline_columns`: the box's search and the mesher each make one), where it once walked
the curve at every ask: the walk is the same function either way, so the outline changes what
a range costs, never what it says. And a prune that goes down into a loose child it kept takes
that child's own children's ranges from the walk that gave the child's (`sdf::pruned`, its
noted ranges) instead of walking them again: that walk took them over a ball about the same
middle (by the same steps as `local`), only larger (each frame widens the radius by a
ten-thousandth), and a range over a ball holds over every smaller ball about its middle. So
each is a range of this page asked of a larger ball: never tighter, never unsound. That the
code does these two things as said is held by test (`the_mesher_walks_each_column_once`,
`a_prune_takes_each_loose_childs_range_once`, and the meshes and ranges compared bit for bit
with the two lanes before them), not by proof.

## 3. Joining, meeting and cutting

Write `a ∧ b` for the lesser and `a ∨ b` for the greater of two numbers.

**3.1.** `|a ∧ b − a′ ∧ b′| ≤ |a − a′| ∨ |b − b′|`, and the same for `∨`. So the lesser and
the greater of two 1-Lipschitz fields are 1-Lipschitz, and so is `−f`.

*Proof.* Let `δ = |a − a′| ∨ |b − b′|`. Then `a ∧ b ≤ a ≤ a′ + δ` and `a ∧ b ≤ b ≤ b′ + δ`,
so `a ∧ b ≤ a′ ∧ b′ + δ`. Swap the pairs. For `∨`, negate. ∎

Let `s_A` and `s_B` be the signed distances of solids `A` and `B`.

**3.2. A union.** `u = s_A ∧ s_B` is a bound for `A ∪ B` (which is taken not to be the whole
of space), and it is **exact outside**.

*Proof.* Outside both, `u(x) = d(x, A) ∧ d(x, B) = d(x, A ∪ B)`, since the nearest point of
a union is the nearer of the two nearest points. On `A ∪ B`, `u ≤ 0`. At a point `p` of
`∂(A ∪ B)`: `p` is in `A` or in `B`, so one of the two is `≤ 0`; `p` is inside neither, so
both are `≥ 0`; hence `u(p) = 0`. `u` is 1-Lipschitz by 3.1 and has the right sign and
zeros, so 2.3 makes it a bound. ∎

**Inside it can fall short.** On a line, let `A = [−2, 1]` and `B = [−1, 2]`. At `0` both
fields are `−1`, and the union `[−2, 2]` is `2` deep there.

**3.3. A meeting.** Let `A ∩ B` be a solid (the two overlap in more than a touch). Then
`m = s_A ∨ s_B` is a bound for it, and it is **exact inside**.

*Proof.* Inside both, `m(x) = −(d(x, ∂A) ∧ d(x, ∂B))`. Let `r` be that lesser distance,
reached at a point `p` of `∂A`, say. The open ball of radius `r` about `x` meets neither
surface, and it holds `x`, which is in both solids; so it lies wholly inside both, and so
inside their meeting: **no point of the meeting's surface is nearer than `r`.** And one is
exactly `r` away: `p`, on the ball's rim, lies in `B` (closed), and in `A` without being
inside `A`, so it is on `∂(A ∩ B)`. So `r` is the distance to the meeting's surface. The
sign and the zeros are checked as in 3.2, and 2.3 gives the bound. ∎

**Outside it can fall short by any ratio.** Two half-planes meeting in a wedge of half angle
`α`: a point on the wedge's axis, beyond its edge and at distance 1 from it, is only
`sin α` from each plane. So the field reads `sin α` where the distance is 1, and as the
wedge narrows the ratio goes to nothing.

**3.4. A cut.** `A` less the inside of `B` is the meeting of `A` with the outside of `B`,
whose signed distance is `−s_B`. Where anything of `A` is left that is a solid,
`s_A ∨ (−s_B)` is a bound for it, exact inside it.

**What this asks of the mesher.** A union, a meeting or a cut of exact fields is exact on one
side and only a bound on the other. Anything that then measures from the field on its weak
side (a swelling outward of a meeting, a shell of a union's inside) is measuring from a
bound and comes out thin. The mesher is told a slack where that matters.

## 4. Raising and turning a plane figure

Let `R` be a plane solid with exact signed distance `s₂`.

**4.1. Raised: a slab.** The solid `R × [−h, h]` (the figure given a thickness of `2h > 0`)
has the exact signed distance

`f(p, z) = (w₁ ∨ w₂) ∧ 0 + |(w₁ ∨ 0, w₂ ∨ 0)|`, with `w₁ = s₂(p)` and `w₂ = |z| − h`.

*Proof.* Outside, the squared distance from `(p, z)` to a point `(q, ζ)` of the slab is
`|p − q|² + (z − ζ)²`, and `q` and `ζ` are chosen apart: the least is
`d(p, R)² + d(z, [−h, h])²`, which is the second term (the first is zero outside). Inside,
the slab's surface is the wall `∂R × [−h, h]` and the two caps `R × {±h}`. The nearest
point of the wall is straight across at the same height, `d(p, ∂R) = −w₁` away; the nearest
cap is straight up or down, `h − |z| = −w₂` away. The depth is the lesser of the two, and
the first term is minus that (the second is zero inside). ∎

**4.2. Turned: a solid of revolution.** Let `R` lie in the half-plane `r ≥ 0` of points
`(r, y)`, and turn it about the `y` axis: the solid of points `(x, y, z)` with
`(√(x² + z²), y)` in `R`. Let `R*` be `R` together with its mirror image in the axis, and
`s*` the exact signed distance of `R*` in the whole plane. Then the solid's exact signed
distance is `f(x, y, z) = s*(√(x² + z²), y)`.

*Proof.* Let `P` have radius `r` and angle `θ`, and `Q` radius `r′` and angle `θ′`. Then
`|P − Q|² = r² + r′² − 2 r r′ cos(θ − θ′) + (y − y′)² ≥ (r − r′)² + (y − y′)²`, with
equality when `θ′ = θ`. So for any set that is the same at every angle, the distance from
`P` to it is the distance measured in the half-plane through `P` and the axis. Apply it to
the solid (for a point outside it) and to the solid's outside (for a point inside it, whose
depth is its distance to the outside): the distance in space is the distance, in the
half-plane, to `R` or to the part of the half-plane not in `R`. A point of `R*`, or of its
outside, that lies across the axis is no nearer to `(r, y)` than its mirror image on this
side, since `|r + r′| ≥ |r − r′|` for `r, r′ ≥ 0`. So the distances to `R*` and to its
outside in the whole plane are the same numbers. ∎

**Why the mirror.** If `R` touches the axis, the stretch of axis it touches is not a surface
of the solid. A signed distance of `R` alone would count it as one, and read too shallow
inside near the axis: a bound, not exact. Outside it makes no difference.

**4.3. Swelling.** Let `S_ρ` be `S` swollen by `ρ ≥ 0`: the points within `ρ` of `S`. For a
point `x` further than `ρ` from `S`, `d(x, S_ρ) = d(x, S) − ρ`. So if `f` is exact outside
`S`, then `f − ρ` is exact outside `S_ρ`.

*Proof.* Walk from `x` toward its nearest point `a` of `S` and stop `ρ` short: that point is
in `S_ρ`, so `d(x, S_ρ) ≤ d(x, S) − ρ`. And any point `q` of `S_ρ` is within `ρ` of some
`a′` in `S`, so `|x − q| ≥ |x − a′| − ρ ≥ d(x, S) − ρ`. ∎

Inside, and for a shrinking (`ρ < 0`), `f − ρ` is 1-Lipschitz and so a bound of the solid it
makes, not its distance in general.

**4.4. A figure sampled.** Let two solids have surfaces within `ε` of each other (every point
of each surface is within `ε` of the other surface), and differ only at points within `ε` of
both surfaces, as a curved figure and an outline sampled along it do. Then their signed
distances differ by at most `ε` wherever they have the same sign, and by at most `2ε`
everywhere.

*Proof.* Let `a` be the point of the first surface nearest `x`, and `a′` a point of the
second within `ε` of `a`. Then the second surface is at most `|x − a| + ε` from `x`; and
the same the other way round. So the two distances to the surfaces differ by at most `ε`.
Where the signs differ, `x` is in one solid and not the other, so within `ε` of both
surfaces, and the two signed distances, each at most `ε` in size, differ by at most
`2ε`. ∎

## 5. Smooth joins

A smooth join of two fields replaces the lesser, `m = a ∧ b`, by something that dips below
it where the two are close. Let `k > 0` be the join's width, `δ = |a − b|`, and
`h = ((k − δ) ∨ 0) / k`, which runs from 1 where the fields are equal to 0 where they are
`k` apart.

| | The join | Its smoothness |
|---|---|---|
| Quadratic | `q(a, b) = m − k h² / 4` | once differentiable |
| Cubic | `c(a, b) = m − k h³ / 6` | twice differentiable |
| Exponential | `e(a, b) = −k ln(exp(−a/k) + exp(−b/k)) = m − k ln(1 + exp(−δ/k))` | every order |

**5.1. How far below the lesser.**

- `0 ≤ m − q ≤ k/4`
- `0 ≤ m − c ≤ k/6`
- `0 < m − e ≤ k ln 2`

Each upper limit is reached exactly where `a = b`. The two polynomial joins equal `m`
wherever `δ ≥ k`; the exponential one is below `m` everywhere.

*Proof.* `h` lies in `[0, 1]`, is 1 only at `δ = 0`, and is 0 from `δ = k` on; `h²/4` and
`h³/6` take their greatest values, `1/4` and `1/6`, at `h = 1`. For the exponential join,
`ln(1 + exp(−t))` falls from `ln 2` at `t = 0` toward 0 and never reaches it. ∎

**5.2. Each join's slopes are two shares that add to one.** Each of `q`, `c`, `e` has
continuous partial derivatives `∂/∂a` and `∂/∂b`, both `≥ 0`, with `∂/∂a + ∂/∂b = 1`.

*Proof.* Take the polynomial joins in the general form `F = m − k φ(h)`, with
`φ(h) = hⁿ / (2n)` for a whole number `n ≥ 2` (the quadratic join is `n = 2`, the cubic
`n = 3`). Where `a < b` and `δ < k`: `m = a` and `h = 1 − (b − a)/k`, so
`∂F/∂a = 1 − φ′(h)` and `∂F/∂b = φ′(h)`. They add to one, and both are `≥ 0` because
`φ′(h) = hⁿ⁻¹/2` lies between 0 and `1/2`. Where `a > b`, the same holds with `a` and `b`
changing places. Where `δ > k`, `F = m` and the partials are `(1, 0)` or `(0, 1)`. That
leaves two lines, and one fact serves both: **a continuous function that has partial
derivatives on either side of a line, tending to the same limits from both sides, has those
limits as its partial derivatives on the line** (by the mean value theorem, applied along a
short step that crosses it).

- *On the line `a = b`.* From either side `h → 1` and `φ′(1) = 1/2`, so both partials tend
  to `1/2` from both sides. **This is where the 4 and the 6 come from**:
  `φ(h) = hⁿ / (2n)` has `φ′(1) = 1/2` for every `n`, which is what makes the two sides
  agree, and the join of degree `n` then dips by `k φ(1) = k / (2n)`.
- *Where `δ = k`.* `h = 0` and `φ′(0) = 0` (this is where `n ≥ 2` is needed: for `n = 1`
  it would be `1/2`, and the partials would step). The partials tend to `(1, 0)` or
  `(0, 1)` from both sides, which are the lesser's own.
- *Second derivatives.* Write `t = b − a`; then `F = (a + b)/2 − ψ(t)` with
  `ψ(t) = |t|/2 + k φ(h)` and `h = (1 − |t|/k) ∨ 0`. `ψ′` is continuous everywhere (it is
  `∂F/∂b − 1/2`, which the two points above made continuous), so the same fact about a line
  can be used once more, on `ψ′`. For `0 < |t| < k`, `ψ″(t) = φ″(1 − |t|/k) / k`, an even
  function, so it tends to one limit at `t = 0` and `ψ` is twice differentiable there. At
  `|t| = k` it tends to `φ″(0)/k`, and beyond it
  `ψ″ = 0`: for the quadratic join `φ″(0) = 1/2`, a step, so it is once differentiable and
  no more; for `n ≥ 3`, `φ″(0) = 0`, so the join is twice differentiable. It is no
  smoother than that: `ψ‴` is an odd function tending to `∓ φ‴(1)/k²` at `t = 0`, and
  `φ‴(1) = (n − 1)(n − 2)/2` is not zero for `n ≥ 3`. So the quadratic join is once
  differentiable, and every join of degree three or more exactly twice.

For the exponential join, `∂e/∂a = exp(−a/k) / (exp(−a/k) + exp(−b/k))` and the like for
`b`: two positive shares of one. ∎

**5.3. A smooth join of 1-Lipschitz fields is 1-Lipschitz.** If `F` is any function of two
numbers with continuous partials that are `≥ 0` and add to one, then
`|F(a, b) − F(a′, b′)| ≤ |a − a′| ∨ |b − b′|`; so for 1-Lipschitz fields `f` and `g`,
`x ↦ F(f(x), g(x))` is 1-Lipschitz.

*Proof.* Along the straight path from `(a′, b′)` to `(a, b)`, the change of `F` is the
integral of `F_a · (a − a′) + F_b · (b − b′)`. With `F_a, F_b ≥ 0` and `F_a + F_b = 1`, the
integrand lies between `−(|a − a′| ∨ |b − b′|)` and `+(|a − a′| ∨ |b − b′|)` at every
point. Then use that `f` and `g` each change by at most `|x − y|`. ∎

**5.4. What a smooth join of two solids is.** Let `f` and `g` be bounds for solids `A` and
`B` that are 1-Lipschitz, let `F` be one of the three joins, `D` its greatest dip (`k/4`,
`k/6` or `k ln 2`), and `u = F(f, g)`. Then:

1. `u` is 1-Lipschitz, so by 2.3 **it never exceeds the distance to the joined surface** (the
   set where `u = 0`).
2. `f ∧ g − D ≤ u ≤ f ∧ g`. So the joined solid, where `u ≤ 0`, holds `A ∪ B`, and lies
   within the set where `f ∧ g ≤ D`.
   - **If `f` and `g` are exact outside**, that set is `A ∪ B` swollen by `D`: a smooth
     join thickens the union by at most `D`, and a box that bounds the union bounds the
     join once it is widened by `D` on every side.
   - **If a field is only a bound outside, the join can reach further.** Where
     `f ≥ ρ · d(x, A)` outside `A` for a ratio `0 < ρ ≤ 1`, the set where `f ≤ D` lies
     within `D / ρ` of `A`. A part that was exact outside and is then scaled unevenly has
     `ρ = σ_min / σ_max` (2.5). A meeting or a cut has no ratio that holds for every one
     (3.3): it depends on the angle at which its surfaces meet. A lens of two unit discs
     whose centres are 1 apart has `ρ = 0.875`; the edge of a wedge of half angle `α` has
     `ρ = sin α`, so joined smoothly to another part it can grow by `D / sin α`, which is
     as large as one likes.
   - **A meeting or a cut reaches no further than what it is made of.** Its field is the
     greater of its fields (3.3), so it is at least each of them: where an operand's field
     is exact outside, the meeting's field outside that operand is at least the distance to
     it. So the set where the meeting's field is at most `D` lies within `D` of every such
     operand: the growth along a thin edge can run far from the meeting and never far from
     what was met. (A cut's first operand is such an operand.)
   - **For a pair joined polynomially, one exact part is enough, at the price of the whole
     width.** A point of the joined solid that lies outside both `A` and `B` has both fields
     at most `k (1 − h/2)²`, so at most `k`; if either field is exact outside, the point is
     within `k` of that part. (The exponential join has no such limit: it thickens a part a
     little however far away the other is.)
3. For the polynomial joins, `u = f ∧ g` wherever `|f − g| ≥ k`: **the join changes the
   field only where the two fields are within `k` of each other in value.** That is about
   values, not places: the fields of two discs with one centre differ by the same amount
   everywhere, and a join changes them everywhere or nowhere. The exponential join changes
   every value, by an amount that dies away as `exp(−|f − g|/k)`.

*Proof.* 1 is 5.3. 2 is 5.1 applied at each point; then 4.3 for exact fields, and for a
bound, `f(x) ≤ D` with `f(x) ≥ ρ · d(x, A)` gives `d(x, A) ≤ D / ρ`. For the wedge: on its
axis beyond its edge the meeting's field is `sin α` times the distance to the edge (3.3),
so it stays under `D` out to `D / sin α`. For the meeting: `f ∨ g ≥ f`, and `f` is the
distance to `A` outside `A`. For the pair: outside both, the lesser field `m` is above zero,
so `u ≤ 0` needs `k hⁿ/(2n) ≥ m`; then `h > 0` and the fields differ by `k (1 − h)`. The
greater field is `m + k (1 − h) ≤ k hⁿ/(2n) + k (1 − h) ≤ k (h²/4 + 1 − h) = k (1 − h/2)²`,
since `hⁿ/(2n) ≤ h²/4` for `0 ≤ h ≤ 1` and `n ≥ 2`. 3 is 5.1. ∎

**In general it is a bound and not the distance.** Where the join is at work the field's
slope is a mix of the two fields' slopes by the shares of 5.2. For two unit slopes at an
angle `θ` to each other and equal shares, the mixed slope has length `cos(θ/2)`: two walls
meeting square give `0.707`. A slope under one is a field that reads short. So wherever
the two slopes are not parallel, `u` is safe and slack through the join, and the mesher is
told so. (Where they are parallel it can stay exact: two discs with one centre join into
the exact field of a slightly larger disc.)

**5.5. Several at once.** The exponential join of `n` fields,
`−k ln(Σ exp(−aᵢ/k))`, lies between `(∧ aᵢ) − k ln n` and `∧ aᵢ`, reaching the lower limit
where all are equal; and joining two at a time gives the same value in any order (the sums
simply add). The polynomial joins applied two at a time are **not** the same in every
order (with `k = 1`, the quadratic join of `0` and `0` and then `0.5` is `−0.265625`, and of
`0.5` and `0` and then `0` is `−0.2822265625`), and `n` fields joined so, which takes
`n − 1` joins, dip by at most `(n − 1) D`: at most one `D` for each join made.

**A chain does better: joined one after another with one width `k`, any number of fields
dip by less than `k` in all** (which is `4D` for the quadratic join and `6D` for the
cubic). A chain is: join the first two, then the result with the third, and so on. **A tree
does not**: `2ʲ` equal fields joined in pairs, the pairs in pairs and so on, dip by `j · D`
(32 equal fields joined quadratically dip by `1.25 k`).

*Proof.* The greatest term of the sum lies between the sum over `n` and the sum itself. For
the nesting: each join is at least the lesser of its two inputs less `D`, and joins rise
with their inputs (5.2), so each further field costs at most one more `D`.

For the chain, let `r` be the running value and `m` the least field so far, and suppose
`r ≥ m − k` (true at the start, where `r = m`). Join the next field `y`, and let `m′` be
the lesser of `m` and `y`. If `r ≥ y`, the result is at least `y − D`, which is at least
`m′ − k`. If `r < y`, write `r = m′ − k + α`, with `α ≥ 0` by the supposition. Then
`y − r ≥ m′ − r = k − α`, so `h ≤ α/k`, and since `h ≤ 1` the dip `k hⁿ/(2n)` is at most
`k h/(2n) ≤ α/(2n)`. So the result is at least `m′ − k + α (1 − 1/(2n))`, which is at
least `m′ − k`. So the supposition holds for the next step, and for every step. For the
tree: equal fields `a` join to `a − D`; two such results, equal again, join to `a − 2D`;
and so on. ∎

**5.6. The smooth meeting and the smooth cut.** `G(a, b) = −F(−a, −b)` lies between `a ∨ b`
and `a ∨ b + D`; its two slopes are `F`'s, taken at `(−a, −b)`, so they too are shares of
one, and it is as smooth as `F`. So the smooth meeting of two 1-Lipschitz fields is
1-Lipschitz, the solid it makes lies inside the meeting of the two solids, and it is a
bound for that solid.

**How much it eats.** It keeps every point where `a ∨ b ≤ −D`. If both fields are exact
inside, `a ∨ b` is minus the meeting's depth (3.3), so the smooth meeting eats the meeting
back by at most `D`. Where it eats is along the meeting's surface, wherever the two
fields are close in value there (within `k` for the polynomial joins; the exponential one
eats a little everywhere), which need not be near where the surfaces cross: two discs with
one centre and radii within `k` are eaten all round. **If a field is only a bound inside, as a union's is (3.2), a point deeper than `D`
can be cut away.** A smooth cut is the smooth meeting with `−s_B`, and the same holds.

**5.7. One width that means what it says.** With `k` replaced by `4k` in the quadratic join,
by `6k` in the cubic and by `k / ln 2` in the exponential, the greatest dip is `k` itself: a
join "of `k` metres" then dips by at most `k`, whichever kind it is, and thickens by at most
`k` metres where the fields are exact outside (5.4). The polynomial joins then reach as far
as the two fields are `4k` or `6k` apart.

## 6. What follows for the modeller

- **A form raised is exact** when the plane figure's own distance is (4.1). **A form turned
  is exact** when the distance used is that of the figure together with its mirror image
  in the axis (4.2); with the figure's own distance it reads short inside, near the axis.
  The shape library holds each figure as exact lines and arcs, so those distances can be
  exact; where a figure is a sampled curve, the field is exact for the sampled outline and
  within twice the sampling's error of the true curve's (4.4).
- **A tree of unions, meetings, cuts and smooth joins of 1-Lipschitz fields is 1-Lipschitz**
  (3.1, 5.3, 5.6), so it never exceeds the distance to the surface it makes (2.3). That is
  all pruning needs.
- **It is exact for certain only on the sides named in 3.2 to 3.4, and in general slack
  through a smooth join** (5.4). A node that needs a true distance (a swelling, a shell) is
  given one where the tree has one, and otherwise says what it was given.
- **Bounds grow by each join's dip, where the parts' fields are exact outside** (5.4, 5.5):
  a box about the parts, widened by `D` for each polynomial join made (`(n − 1) D` for `n`
  parts joined two at a time in any arrangement), or by `k ln n` for `n` parts joined at
  once exponentially. **A chain of polynomial joins with one width `k` needs `k` and no
  more, however many parts it has** (5.5); a group nested inside a group is a tree, and
  the widenings of the two add.
- **A part that is only a bound outside needs more** (5.4). A part scaled unevenly reaches
  `D` over the ratio of its scales; and whatever moves its surface to another level of its
  field (a swelling, a shell) moves it by that level over the ratio, by the same
  inequality. A meeting or a cut can reach any distance from itself along a thin edge, and
  never more than `D` from an operand that is exact outside: boxed by its operands it
  needs `D` and no more, and boxed tightly about itself no widening is enough. A pair with
  one exact part needs the join's whole width `k`. And a smooth meeting of a part that is
  only a bound inside can eat deeper than `D` (5.6).
- **An uneven scale costs the ratio of its least to its greatest factor** (2.5).
- **A part with no factor is bounded by its own range, not by a number** (2.6, 2.7): a flat or
  long ellipsoid, a column whose proportions change, a repetition. The mesher's skip and its
  pruning ask such a part's range over the ball; a tree of parts that all have factors keeps
  the one quick test.

## 7. A bone's flesh, and a body of bones

The `body` shape ([FABRICA.md](FABRICA.md), step 3) is one field for a whole skeleton, meshed
once. A sample near one bone should ask that bone and its neighbours, not all of them, and the
mesher's quick skip needs a factor. So the field is built to have one: **every part of it is
1-Lipschitz by construction, whatever its sections do along the bone**, and the body's own
culling is exact. The code is `crates/specula-model/src/caro.rs`; its tests hold each statement
below by sampling (`caro::tests`, against a reference in `f64` written apart from the code and
against nearest points found by search), not `distance_field_facts.rs`.

**Words.** A **bone** runs from `a` to `b`; `e` is its direction, and `s` and `w` complete a
frame turned with it (`s` toward its side, where breadth is measured). A point `x` has the
bone's coordinates `t = (x − a)·e` along it and `y = (u, v) = ((x − a)·s, (x − a)·w)` across it:
a move and a turn, which keep every distance (2.4). The bone has **sections** at stations
`t₀ < t₁ < … < t_J` (metres along it), each a half breadth `A_j` and a half depth `B_j`, and one
**squareness** `p ≥ 2` (2 an ellipse; higher, toward a rounded box). Write `q_j = B_j / A_j`.

**7.1. The profile, and how fast it changes.** Between stations, `A` is the cubic through
`A_j` and `A_{j+1}` with given slopes at both (a Hermite piece), and so is `q`; `B = q A`. The
slope at the first and the last station is 0; at a station between, it is the harmonic mean
`2 r r′ / (r + r′)` of the rises `r`, `r′` of the two pieces beside it when they have one sign,
and 0 otherwise. Then **each piece of `A` and of `q` runs one way, and stays between its end
values**; `A`, `q` and `B` have continuous slopes.

*Proof.* On a piece of span `Δ` and secant `δ = (A_{j+1} − A_j) / Δ`, write the end slopes as
`α δ` and `β δ`. Each slope is 0 or a harmonic mean of `δ` and a rise of the same sign, and
`2 r r′ / (r + r′) ≤ 2 r′`, so `α` and `β` lie in `[0, 2]` (for `δ = 0` both slopes are 0 and
the piece is level). With `f = (t − t_j) / Δ`, the piece's slope is `δ` times
`6f(1 − f) + α (1 − 4f + 3f²) + β (3f² − 2f)`, which is affine in `(α, β)`; at the four corners
of `[0, 2]²` it is `6f(1 − f)`, `2 − 2f`, `2f` and `6(f − ½)² + ½`, none negative on `[0, 1]`. So
it never has the sign opposite to `δ`. The slopes agree at each station by construction. ∎

So on a piece `A` lies between `A_j` and `A_{j+1}`, and the greatest of `|A′|` is the greatest
of a quadratic on `[0, 1]` (at an end or where it turns): the code takes it exactly, from the
numbers it stores. Write `g` for a number at least the steepest growth of the least half size
`m = min(A, B)` (where `q ≥ 1` on a piece, `m = A`; where `q ≤ 1`, `m = B`; else either), and
`k` for a number at least the greatest of `A |q′|`, which is 0 when the proportions do not
change.

**7.2. The section's gauge.** For `p ≥ 2` let `N_p(z) = (|z₁|^p + |z₂|^p)^{1/p}`, and let the
section's gauge be `n_t(y) = N_p(u / A(t), v / B(t))`. The section is `n_t ≤ 1`. Then
`|n_t(y) − n_t(y′)| ≤ |y − y′| / m(t)`; the section holds the disc of radius `m` about the axis
and lies within the disc of radius `M ≤ √(A² + B²)` (`M = max(A, B)` for `p = 2`).

*Proof.* `N_p` is a norm (Minkowski's inequality). For a unit vector `z`, `|z_i|^p ≤ z_i²`, so
`N_p(z) ≤ 1`: `N_p` never exceeds the length. So
`|n_t(y) − n_t(y′)| ≤ N_p((u − u′)/A, (v − v′)/B) ≤ |((u − u′)/A, (v − v′)/B)| ≤ |y − y′| / m`.
A point of the disc of radius `m` has `n_t ≤ |y| / m ≤ 1`; a point of the section lies in the
box `|u| ≤ A`, `|v| ≤ B`, and for `p = 2` in the ellipse, which lies within `max(A, B)`. ∎

For `p < 2` the corners turn in and `N_p` exceeds the length by up to `2^{1/p − 1/2}`; such
sections are refused.

**7.3. A bone's field is 1-Lipschitz, and a bound on both sides, whatever its proportions
do.** Let `τ` be `t` held to `[t₀, t_J]`, `m = m(τ)`, `e = (t − τ) / m`, and
`n = √(e² + n_τ(y)²)`. The bone's **flesh** `K` is where `n ≤ 1`: at each station of
`[t₀, t_J]` exactly the section there, and past each end a cap reaching `m` of that end beyond
it (a half ellipsoid for `p = 2`). Let `R` be at least the largest `M` along the bone, `c` the
distance from `x` to the axis's segment from `t₀` to `t_J`, less `R`, and

`ψ = m · (min(n, 2) − 1)`, `L = √(1 + (g + 2k)²)`, `F = max(ψ / L, c)`.

Then (1) `ψ` changes by at most `L` for each metre moved; (2) `F` is 1-Lipschitz; (3) `F` has
the sign of `K`'s signed distance and is zero exactly on `K`'s surface, so **`F` is a bound for
`K`, outside and inside**: `|F(x)| ≤ d(x, ∂K)`; (4) it reads short by at most a ratio near the
bone and a constant far from it: outside, where `n ≤ 2`, `F ≥ s / (L ρ)` with `ρ = M / m` of the
station (or end) nearest along the axis, and everywhere outside `F ≥ s − (R − m_min)`.

*A lemma first.* **A continuous function that is Lipschitz near every point, and whose gradient,
wherever it has one, is at most `L` long, changes by at most `L` for each metre.** Take `x`, `y`
and a small offset `z`. The points without a gradient fill no volume (Rademacher's theorem), so
for almost every `z` the segment from `x + z` to `y + z` meets them in a set of no length; along
that segment the function is Lipschitz in one variable, the integral of its derivative, and its
derivative is the gradient's component, at most `L |x − y|`. So it changes by at most
`L |x − y|` between `x + z` and `y + z`, and by continuity between `x` and `y`. ∎

*Proof of (1).* `ψ` is continuous (at the ends `e = 0` from both sides; at `n = 2` both
branches agree) and Lipschitz near every point. Where it has a gradient:

- *Along the bone, `n < 2`.* Across, `∇_y ψ = m ∇n_τ`, at most 1 long (7.2). Along, write
  `α = A′/A`, `β = B′/B`. Since `N_p` is of degree one, `ũ ∂N/∂ũ + ṽ ∂N/∂ṽ = N` with both terms
  `|z_i|^p N^{1−p} ≥ 0` (Euler); so `∂n/∂t = −n (ω_u α + ω_v β)` with shares `ω_u, ω_v ≥ 0`
  adding to one, and `∂ψ/∂t = n (m′ − m Λ) − m′`, `Λ = ω_u α + ω_v β`. Where `m = A`,
  `m′ − m Λ = ω_v A (α − β) = −ω_v A q′/q`, and `q ≥ 1` there; where `m = B = qA`,
  `m′ − m Λ = ω_u B (β − α) = ω_u A q′`. Where `A = B` and `m` has a slope, `A′ = B′`, `q′ = 0`
  and the term is 0. So `|m′ − m Λ| ≤ A |q′| ≤ k`, and `|∂ψ/∂t| ≤ n k + g ≤ 2k + g`. Across
  and along are square to each other: `|∇ψ|² ≤ 1 + (g + 2k)² = L²`.
- *Along the bone, `n > 2`.* `ψ = m(τ)`, whose gradient is `m′ e`: at most `g ≤ L`.
- *Past an end.* `A`, `B` and `m` are the end's. `n` is the gauge of the cap's body, and
  `|∇n|² = (e² / m² + n_τ² |∇n_τ|²) / n² ≤ (e² + n_τ²) / (m² n²) = 1 / m²` (7.2), so
  `|∇ψ| ≤ 1`.

The lemma gives (1). ∎ (2) `ψ / L` and `c` are 1-Lipschitz (2.1 for `c`), and so is their
greater (3.1). (3) `K` lies in the capsule `c ≤ 0`: a point of a section is within `M ≤ R` of
the axis, and a point `(τ′, y′)` of a cap, `τ′` past the end, has
`τ′² + |y′|² ≤ m² (1 − n_τ²) + M² n_τ² ≤ R²`. So on `K` `c ≤ 0` (and `c < 0` inside it), and
`F` is 0 where `n = 1`, below 0 where `n < 1` and at least `ψ / L > 0` where `n > 1`. `n` grows
along every ray from the axis (it is of degree one there), so `n = 1` is `K`'s surface. Then
2.3. (4) Where `n ≤ 2`, the point `y / n_τ(y)` at the same station is on the surface, so
`s ≤ |y| (1 − 1/n) ≤ M (n − 1)`, while `F ≥ ψ / L = m (n − 1) / L`; past an end the same holds
with the end's point `(x − b) / n`, which is within `M` of the end. Everywhere, let `P` be the
point of the axis's segment nearest `x`: `K` holds the disc (or half ball, at an end) of radius
`m(P) ≥ m_min` about `P` toward `x`, so `s ≤ |x − P| − m_min`, and `c = |x − P| − R`. ∎

**Where the proportions change.** This is where 2.7 found no factor for the sculpt's column:
far out to one side a field that follows one section's gauge changes along the bone by
`n k` for `n` sections out, without limit. **The clamp at `n = 2` is what removes it**: it
costs `L` nothing beyond `2k`, and beyond it the capsule `c` carries the field. Nothing is
asked of how the proportions change but that the profile is 7.1's: `k` is 0 when they do not,
and `L = √(1 + g²)`, the sculpt column's own factor. For one section all along a bone and
`p = 2`, `ψ` is the decisions' candidate itself: a capsule of radius `m` in the space stretched
by `A / m` and `B / m` across, scaled by the least stretch, 1 (2.5).

**7.4. Swells.** A swell along the bone (a calf, a forearm's belly) is a section and is in 7.3.
A swell off it (a belly, a buttock, a kneecap) is an ellipsoid of half axes `r₁, r₂, r₃` along
the bone's frame about a centre `o`: its field is
`F = max(r_min (|(x − o) / r| − 1), |x − o| − r_max)`, taken coordinate by coordinate in the
frame. **It is 1-Lipschitz and a bound for the ellipsoid, both sides.** *Proof.* The first term
is 2.5 applied to the unit ball's exact field `|x| − 1`, the second is 2.1, and the ellipsoid
lies in the ball of radius `r_max`; then 3.1 and the argument of 7.3 (3). ∎ It joins the body
as a bone does (7.5) and is culled by its ball as a bone is by its capsule (7.6).

**The shapes of a muscle's belly.** The muscle models in the research (round 9, R9-A2,
section 1, not yet in this repository) use three: a spindle (an ellipsoid), a strap (an
elliptic cylinder closed by two half ellipsoids) and a fan (a truncated elliptic cone, closed
the same way). The first is 7.4's swell. The other two are **a bone part of two sections**,
laid along the muscle's line: 7.3 holds for them as it stands, with caps of the end's least
half size. So every belly those models use has a proved bound here; a belly that keeps its
volume as its joint turns (they shorten and fatten) is a change of these numbers with the pose,
which this page does not treat.

**7.5. A body: the bones joined in order of their values.** Let `F₁ … F_n` be the bones' and
swells' fields at `x`, and `K > 0` the body's **smoothing width**. Sort the values from the
least; join the first two by the quadratic join of width `K` (section 5,
`q(a, b) = a ∧ b − K h² / 4`), the result with the third, and so on: that is `G`. Let `T > 0`
and `H = G ∧ T`. Then:

1. `G` changes by at most the greatest change of the `F_i`, so it and `H` are 1-Lipschitz.
2. `F_min − K ≤ G ≤ F_min`, where `F_min` is the least of the `F_i`.
3. `H` has the sign of `G` and the same zeros, so **`H` is a bound for the joined body, both
   sides.**
4. **The bulge.** The joined body holds every part and lies where `F_min ≤ K/4` for two parts,
   and where `F_min ≤ K` for any number. Where that level is under every part's `m / L`, **it
   lies within `(K/4) L ρ` of the plain union for two parts, and within `K L ρ` for any number**
   (`L` and `ρ` the greatest of the parts'). For round bones of one size (`L = ρ = 1`) the two
   parts' `K/4` is reached wherever their fields are equal on the surface, and that includes a
   **straight joint**: two bones end to end bulge by `K/4` in a ring at their joint.

*Proof.* (1) Where the order of the values is fixed, `G` is a chain of joins, each of which
changes by at most the greater change of its two inputs (5.3). Where two values are equal, the
two orders join the same numbers and give the same `G`, so `G` is continuous; along a straight
path between two lists of values the order changes at finitely many places, and the bound holds
on each piece between. So `|G(x) − G(y)| ≤ max_i |F_i(x) − F_i(y)| ≤ |x − y|`. `H` by 3.1.
(2) 5.5's chain, with the first value the least. (3) as 3.1 and 2.3; `T > 0` keeps every zero.
(4) Where `G ≤ 0`, (2) and 5.1 give `F_min ≤ K/4` for one join and `F_min ≤ K` for a chain.
At such a point, if `F_i ≤ K < m_i / L_i` then `n < 2` for that part (beyond it `F_i ≥ m_i /
L_i`), and 7.3 (4) gives `s_i ≤ L_i ρ_i F_i`. For equal fields `q(f, f) = f − K/4`. ∎

The join is the sculpt's quadratic one, but the order is by value, not by the recipe: the
pairwise join depends on its order (5.5), and the cull below needs every order to give one
answer.

**Which join, and what it does at a joint.** The research on skinning at a joint (round 9,
R9-A3, sections 3 and 4, not yet in this repository) names three ways to compose two bones'
fields and a fault of each. A plain union leaves a **crease** in the bend of a joint. A smooth
join that sees only the two values (this one) fills that crease but **bulges even where the
bones run straight**. Joins that switch by the angle between the two fields' gradients (no
blend where the bones are aligned, a blend or a contact where they bend) avoid both, and no
source states a bound on how fast they change (R8-B1). **This field takes the value-only join,
for its proof**, and the measurement agrees with 7.5 (4): two round bones of one size, at 0 to
150 degrees, with widths from a twentieth of their radius to twice it, never lie more than
`K/4` outside their union, and at 0 degrees the ring is `K/4` within a tenth
(`a_joint_keeps_the_bound_and_bulges_by_a_quarter_of_the_width`). At a bend the same `K/4` is
reached along the outside, on the plane that halves the bend, where the two fields are equal;
inside the bend the join fills the crease. `K = 0` gives the plain union: no bulge, the crease
kept. What a proof can carry toward the third way is 7.7.

**7.7. A width that changes from place to place.** For the quadratic join `q(a, b; K)`,
`−1/4 ≤ ∂q/∂K ≤ 0`. So if the width is a function `K(x) > 0` that changes by at most `Λ` a
metre, and `f`, `g` are 1-Lipschitz, then `q(f, g; K(x))` changes by at most `1 + Λ/4` a
metre; a chain of `n` parts so joined, by at most `1 + (n − 1) Λ/4`.

*Proof.* Where `|a − b| ≥ K`, `q` is the lesser and does not depend on `K`. Elsewhere, with
`δ = |a − b|` and `h = 1 − δ/K`, `q = a ∧ b − K h²/4` and `∂h/∂K = δ/K² = (1 − h)/K`, so
`∂q/∂K = −h²/4 − (K h/2)(1 − h)/K = −h (2 − h)/4`, which lies in `[−1/4, 0]` for `h` in
`[0, 1]`; it is continuous across `δ = K`, where both sides are 0. Along a segment, the change
of `q(f, g; K)` is at most the greater change of `f` and `g` (5.3) plus a quarter of the change
of `K`. In a chain each join adds at most a quarter of the change of `K`, and each passes on the
change of its inputs with shares that add to one. ∎

So a width that falls toward nothing where two bones run on from one another, and rises where
they bend, can be had with a stated factor (`Λ` from how fast the width is made to change):
dividing by that factor keeps the field a bound. Held by test
(`a_join_of_changing_width_changes_by_a_quarter_of_it`). Which width, and how it is made from
the skeleton, is 7.8's, which also finds the chain's factor `1 + Λ` whatever its length.

**7.6. The body's cull is exact.** Let `c_i` be each part's capsule term (7.3's `c`; a swell's
`|x − o| − r_max`), so `F_i ≥ c_i`, and let `ε = T + 2K`. **`H(x)` is the same number when only
the parts with `c_i(x) ≤ ε` are joined**, every `x`. So a sample asks only the parts whose
capsule, widened by `ε`, holds it; a block of radius `r` about `o` needs only the parts with
`c_i(o) ≤ ε + r`; and the culled value is `H` itself, a bound (7.5 (3)) and 1-Lipschitz, which
is all the mesher's skip needs.

*Proof.* Let `S` be the kept parts and `m_S` the least of their values. Every other value is
above `ε`. If `m_S ≤ ε − K`: in the sorted chain the running value is at most `m_S` from its
first step on (a join is at most its lesser input, and the first value is the least of all,
which is in `S`), so when a dropped part's value `y` comes it is at least `K` above the running
value, `h = 0`, and the join returns the running value unchanged. The chain over all is then the
chain over `S`, step for step. If `m_S > ε − K`: every value is above `ε − K`, so both chains are
at least `ε − 2K = T` (7.5 (2)), and both `H` are `T`. ∎

**Over a ball.** As `H` is 1-Lipschitz, over the ball of radius `r` about `o` it lies in
`[H(o) − r, (H(o) + r) ∧ T]`; and since `G ≥ F_min − K ≥ min_i c_i − K`, it is also at least
`(min_i c_i(o) − r − K) ∧ T`, which prunes a large block far from every bone without asking any
bone's field.

**In `f32`.** The code computes each of these in single precision, with these allowances (each
counted in the code, held by test against the reference in `f64`; the count is not proved line
by line here):

- a value is within `σ(x) = 2⁻¹⁴ (|x|₁ + W)` of the exact one, `W` being the greatest `|a|₁ + R`
  of the parts plus `9K + T` (about 350 rounding units are counted, of the 1,024 allowed);
- the derived numbers `L`, `R` and `g`, `k` are worked out in `f64` from the numbers stored and
  rounded up, so each is at least the exact one for the profile the code evaluates;
- the cull holds **bit for bit**: the dropped parts' joins return their input unchanged in
  `f32` too (a difference above `K` rounds to at least `K`), and the second case needs `ε` a
  little above `T + 2K`, by `2⁻¹⁴ (ε + K)`, for the rounding of a chain of up to 64 joins;
- a range over a ball and the parts it needs are widened by `2 σ` at the ball's farthest point.

**7.8. A width that falls where two bones run straight on.** A **joint** is two bones that
meet end to end (a child's head at its parent's tail, or two children's heads at one place),
each with its **outward direction** from the joint (`e` at a head, `−e` at a tail), and the
**bend** `θ` between one's direction into the joint and the other's out of it
(`cos θ = −o₁ · o₂`; 0 straight on). The joint's **width** is the body's `K` times
`s(u)`, `u = (1 − cos θ) / (1 − cos 40°)` held to `[0, 1]`, `s(u) = u² (3 − 2u)`, or its own
where the body gives one (at most `K`): nothing straight on, all of `K` from 40 degrees. Its
**seam** `Z` is the solid disc about the joint square to `o₂ − o₁`, of radius `R + K` and half
thickness `Λ R` (`R` the greater reach of the two bones, `Λ = 1/2`). The width at a point is

`K(x) = min(K, min over joints of (K_j + Λ d(x, Z_j)))`,

the body's everywhere but near a joint that runs on. Join the sorted values as in 7.5 with the
width `K(x)` at each point: that is `G`, and the body's field is `H = (G ∧ T) / (1 + Λ)` (and
`G ∧ T` where no joint's width is under `K`). Then:

1. `K(x)` changes by at most `Λ` a metre, and `0 ≤ K(x) ≤ K`.
2. For fixed values, `G` changes by at most the change of the width: `|∂G/∂K| ≤ 1`, **however
   many parts are joined** (7.7 alone gives `(n − 1)/4`).
3. So `G` changes by at most `1 + Λ` a metre, and `H` by at most one: **`H` is 1-Lipschitz**
   and, as it has `G`'s sign and zeros, a bound for the joined body, both sides.
4. 7.5 (2) and (4) and 7.6 hold as they stand with `K` the greatest width: the cull's `ε` is
   still `T + 2K`, and it is still exact.

*Proof.* (1) The distance to a solid disc is 1-Lipschitz (2.1; the disc is convex), so each
`K_j + Λ d(x, Z_j)` changes by at most `Λ`, and so do their least and its least with `K` (3.1).
(2) Fix the values, sorted, and let `r` be the running value and `y ≥ r` the next (a join is at
most its lesser input and the first value is the least, so the running value never passes the
next). With `h = 1 − (y − r)/K`, 7.7 gives `∂q/∂K = −h(2 − h)/4`, and from `q = r − K h²/4`,
`∂q/∂r = 1 − h/2`. Write `D` for `−∂G/∂K` after a join: `D₁ = h(2 − h)/4 ≤ 1/4`, and
`D ← (1 − h/2) D + h(2 − h)/4`; if `D ≤ 1` before, after it is at most
`1 − h/2 + h/2 − h²/4 = 1 − h²/4 ≤ 1`. Where `|y − r| ≥ K` the join is the lesser and both
derivatives are 0 and 1; `q` has continuous derivatives in `K` across `y − r = K` (7.7), and
`G → F_min` as `K → 0` (7.5 (2)), so `G` changes by at most `|K − K′|` between any two widths.
(3) Between `x` and `x′`: `|G(F(x), K(x)) − G(F(x′), K(x))| ≤ max_i |F_i(x) − F_i(x′)| ≤ |x − x′|`
(7.5 (1), at the fixed width `K(x)`), and `|G(F(x′), K(x)) − G(F(x′), K(x′))| ≤ |K(x) − K(x′)| ≤
Λ |x − x′|` by (2) and (1). Then 3.1 for `∧ T`, and the positive divisor keeps signs and zeros.
(4) 7.5 (2) needs only `K(x) ≤ K`. In 7.6's first case a dropped value is at least `K ≥ K(x)`
above the running value, so its join returns the running value; in the second both chains are
at least `ε − 2K = T`. The division comes after `∧ T`, so both are the same number divided. ∎

**Why the bend of the joint, and not the angle of the fields' gradients.** R9-A3 (section 3)
describes composing a joint by the angle between its two fields' gradients at the point: no
blend where they are aligned, a blend or a contact where they bend. That angle changes without
bound near a bone's axis and across the places where a field's slope breaks (the gauge's clamp,
the capsule's takeover: "What 7.3 to 7.6 do not say"), so no `Λ` bounds it, and dividing by
`1 + Λ` would need one. The bend is a number of the joint, constant in space; only the seam's
place depends on `x`, and through a distance, whose `Λ` is the one chosen.

**Why the seam is that size, and what it costs.** At a straight joint of two round bones of
radius `r ≤ R`, `z` from the joint plane along the surface, the two fields differ by
`√(z² + r²) − r`, so the join acts where that is under `K(x)`; with `K_j = 0` and the width
growing as `Λ (z − a)` past the seam's half thickness `a`, the join can act at the surface only
where `√(z² + r²) − r < Λ (z − a)`, and with `a = Λ R` that leaves next to nothing: to second
order in `z / r` the difference is `z²/(2r)` and `z² − 2rΛz + 2rΛ²R ≥ (z − rΛ)² ≥ 0`. That the
ring is nothing is not proved beyond that order; it is measured. Measured (`a_straight_joint_has_no_ring`): chains of round bones of radius 2
to 10 cm through two straight joints, widths 5 mm to 4 cm, 1,500 rays each: the joined surface
lies within `5 · 10⁻⁵` of `K/4` of the plain union (the halving's own precision), against the
whole `K/4` before; bent by 1 or 2 degrees, within `6 · 10⁻⁵` of it. At a bend of 45 degrees and
more the width is the body's and the bulge is `K/4` as before
(`a_bent_joint_keeps_its_bulge_under_the_bound`, 0 to 150 degrees: never past `K/4`). The
price: where any joint runs on, the field reads short by another `1 + Λ = 1.5` (the mesher
skips less), and within a seam a swell meets its bone with less smoothing, as the joint's two
bones do. Held by test: the factor over a million pairs (`a_jointed_body_changes_no_faster_than_the_distance`),
each value within its slack of a reference in `f64` that works out the width apart, and the
cull bit for bit on the four skeletons jointed and random chains.

**Where three or more bones meet** (a pelvis, a fork) each pair that meets end to end is a
joint, and the width is the least of them: two hips in one line through their joint take no
width there, and a third bone meeting the two inside that seam (a spine) meets them with a
crease in the seam's thickness. A body that wants otherwise gives that joint its own width.

**What 7.3 to 7.6 do not say.**

- The field is no distance: it reads short by `L ρ` near a flat bone and by `R − m` far out,
  and inside it may read as shallow as the capsule. That costs samples, not soundness.
- The surface is exactly the sections and the caps in exact arithmetic; in `f32` it lies within
  `L ρ σ` of them. How smooth it is between pieces is not stated: `A` and `q` have continuous
  slopes, so the surface along a bone does, and a cap meets its end with a continuous slope
  because the end slopes are 0. How smooth the joined surface is, beyond what section 5 says of
  one join, is not stated either.
- The bulge is bounded, not removed, at a bend. With 7.5's one width a straight joint bulges as
  much as a bend (7.5 (4)); 7.8's width removes the ring at a straight joint (measured, not
  proved to be nothing) and keeps `K/4` at a bend.
- The field is continuous, not smooth. Its slope breaks where the gauge is held at 2, where
  `m` changes from `A` to `B`, and where the capsule takes over from `ψ / L`; the last can lie
  within `K` of a surface (just outside the widest station of a bone whose `L > 1`), so a join
  that acts there can carry a small break of slope into the joined surface. Skinning by
  projection onto the fields' levels (R9-A3) needs smooth fields: this one is not yet.
- The cull asks every part's capsule at every sample; it is exact, not cheap. A block that asks
  `c_i(o) ≤ ε + r` once and passes its list to its samples is.

## 8. Where the same facts are published

For credit, and for a reader who wants more. Details of each (links, licences, what each
states and where) are in [the citations](../research/library/I6-distance-fields-citations.md).

- That a signed distance is 1-Lipschitz, and what a swelling keeps: Bálint, Valasek and
  Gergó, "Operations on Signed Distance Functions", *Acta Cybernetica* 24(1), 2019.
- That a 1-Lipschitz field is a safe step for tracing a ray, and the words "distance bound":
  Hart, "Sphere tracing", *The Visual Computer* 12(10), 1996.
- Unions, meetings and cuts as bounds, and how far they can fall short: Bálint, Valasek and
  Gergó, "Operations on Signed Distance Function Estimates", *Computer-Aided Design and
  Applications* 20(6), 2023.
- Raising and turning a plane field, and the polynomial and exponential joins with their
  widths: Quilez, "distance functions", "2D distance functions" and "smooth minimum"
  (iquilezles.org).
- Smooth joins that keep the Lipschitz condition: Dekkers, van Overveld and Golsteijn,
  "Combining CSG modeling with soft blending using Lipschitz-based implicit surfaces", *The
  Visual Computer* 20(6), 2004 (the journal's abstract only was read; its proof was read in
  the authors' free technical report of 1996).
- The exponential join's limits as a numbered result: Molnar and Ames, *IEEE Control Systems
  Letters* 7, 2023 (Theorem 4); Rabiee and Hoagg, *Automatica* 171, 2025 (Proposition 1);
  Igel, ICML 2024 (equations 5 and 6).
- The quadratic join's quarter of `k`: Li et al., "SDFStent", *Computer Methods and Programs
  in Biomedicine* 287, 2026.
- The cubic join's sixth of `k`, and the general `k / (2n)` of 5.2: stated as a result in no
  peer-reviewed paper found. It is proved above.

## 9. How this page was checked

A proof is checked by someone who did not write it. A model that wrote none of this page
read every statement and proof, searched for counterexamples with code of its own, and
seeded faults into the test (2026-11-20). It found the numbered statements 2.1 to 5.5 and
5.7 true, and four things wrong, all corrected above:

- **5.6 was false as first written**: it said a smooth meeting is "eaten back by at most
  `D` where they cross". That needs both fields exact inside, and it eats wherever the
  fields are close in value, not where the surfaces cross.
- **Section 6 said a box widened by `D` bounds a smooth join**, without the condition 5.4
  carries (fields exact outside). A join onto a meeting can bridge a gap of any width along
  a thin edge. This one was on the unsafe side: a box too small.
- **Section 6 gave `(n − 1) D` for `n` joins** where 5.5 proves it for `n` fields: one dip
  too few.
- **Section 6 said a turned form is exact when the figure's own distance is**, where 4.2
  needs the figure with its mirror image.

It also found two proofs with a step missing (3.3: that no point of the meeting's surface
is nearer; 5.2: that the slopes exist on the two lines, not only their limits), now
written in, and that the mirror image, the smooth meeting, joins in space and the count of
nested joins were held by no real test: they are now. Smaller errors it found, all
corrected: the join's `ψ` written wrongly beyond `|t| = k`; degree 1 not excluded; a
sentence that implied higher degrees are smoother than twice differentiable; a gloss in
5.4 that read "values" as "places"; a slip of sign in 4.1; a sentence on the wedge that
contradicted itself; and a credit to a paper whose text had not been read (section 7 now
says which text was).

**A second model, which wrote neither the page nor the first review, then checked the
corrected page** (2026-11-20) and found it sound: no numbered statement false, every proof
proving its statement, and each of the first review's findings mended at its cause. It
held the page against probes of its own (among them 3,000 random trees of every operation
with bounds for leaves, whose field never changed faster than 0.999995 for one) and seeded
eleven faults into the test, one at a time. What it found, all minor and all taken up
above:

- **Three sentences said less than is true** (on the safe side): that a join "is a bound
  and not the distance" (two discs with one centre join into an exact disc), that a
  meeting or a cut "has no such ratio at all" (each has one; no one ratio serves all), and
  that a smooth meeting eats "wherever the two fields are within `k`" (it eats along the
  meeting's surface, where they are).
- **A fact the modeller leans on was missing**: a chain of polynomial joins with one width
  dips by less than that width however long it is, where a tree of them does not. It is
  5.5's second part now, with its proof and a test.
- **5.2's step to the second derivative** used the fact about a line on `ψ′` without saying
  that `ψ′` is continuous.
- **One test could not fail for the fault it was named for**: the slope through a join was
  measured across the line of equal fields, where any function that treats its two fields
  alike reads the same. It is measured from each side now, and the test shows that the
  plain lesser fails it.

**The page was then held against the modeller's own boxes by builds** (2026-11-20; a third
model, which meshed each case twice, inside the modeller's box and inside one wide enough
for the whole surface). It showed that section 6 still said one thing wrongly for the
modeller: that a join onto a meeting or a cut needs its reach "taken from the field, not
from a box". A meeting is at least each field it is made of, so boxed by its operands it
needs `D` and no more (5.4's fourth point, new); and it found that a pair needs only one
exact part, at the price of the whole width (5.4's fifth point, new, proved above). What
does lose surface in the modeller is the uneven scale of 2.5: two parts stretched ten to
one and joined, or one such part swollen, pass the box, because the box was widened by the
level and not by the level over the ratio. That is the modeller's fault and not the
page's; it is recorded with the modeller
([PROCEDURAL_MODELLING.md](PROCEDURAL_MODELLING.md), tier 3).

**Section 7 has not yet been read by a model that did not write it** (written 2026-11-28 by the
body's third builder). The convolution papers that shape a body round a skeleton state no bound
on their fields' slopes ([R8-B1](../research/library/R8-B1-convolution-fields-slope-bounds.md));
section 7 takes none from them and none from sphere tracing's, whose fifth theorem is misprinted
in the copy read (the form its proof gives is what R8-B1 records).

Nothing on the page follows a source's wording.
