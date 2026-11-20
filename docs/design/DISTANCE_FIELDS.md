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

**How it is held.** A proof on a page can be wrong. Every numbered statement below is also
checked in numbers by `crates/specula-model/tests/distance_field_facts.rs`, which uses none
of the modeller's code: it holds each bound against a nearest point found by search, and each
"reached exactly when" against the case that reaches it.

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
3. For the polynomial joins, `u = f ∧ g` wherever `|f − g| ≥ k`: **the join changes the
   field only where the two fields are within `k` of each other in value.** That is about
   values, not places: the fields of two discs with one centre differ by the same amount
   everywhere, and a join changes them everywhere or nowhere. The exponential join changes
   every value, by an amount that dies away as `exp(−|f − g|/k)`.

*Proof.* 1 is 5.3. 2 is 5.1 applied at each point; then 4.3 for exact fields, and for a
bound, `f(x) ≤ D` with `f(x) ≥ ρ · d(x, A)` gives `d(x, A) ≤ D / ρ`. For the wedge: on its
axis beyond its edge the meeting's field is `sin α` times the distance to the edge (3.3),
so it stays under `D` out to `D / sin α`. 3 is 5.1. ∎

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
- **A part that is only a bound outside needs more, and a box cannot say how much** (5.4): a
  part scaled unevenly reaches `D` over the ratio of its scales; a meeting or a cut can
  reach any distance along a thin edge. So the modeller takes a joined part's reach from
  where its field is at most `D`, not from its box widened by `D`; and a smooth meeting of
  a part that is only a bound inside can eat deeper than `D` (5.6).
- **An uneven scale costs the ratio of its least to its greatest factor** (2.5).

## 7. Where the same facts are published

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

## 8. How this page was checked

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

Nothing on the page follows a source's wording.
