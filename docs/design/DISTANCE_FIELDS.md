# Distance fields: what the engine relies on, with proofs

The sculpt modeller builds shapes from distance fields
([PROCEDURAL_MODELLING.md](PROCEDURAL_MODELLING.md), tier 3), and the shape library's forms are
to become nodes of its tree ([FORMAE.md](FORMAE.md)). This page states each fact that work
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
1-Lipschitz and `Z` is the set where `f = 0`, then `|f(x)| ≤ d(x, Z)`.

*Proof.* For any `z` in `Z`, `|f(x)| = |f(x) − f(z)| ≤ |x − z|`. Take the nearest `z`. ∎

So a 1-Lipschitz field with the sign of `s`, zero on `∂S`, is a bound for `S`. This is the
one property the mesher needs: where `|f(x)| > r`, no surface lies within `r` of `x`.

**2.4. Moving and turning keep everything; scaling evenly keeps everything.** If `f` is the
signed distance (or a bound) of `S` and `M` is a move or a turn, `f(M⁻¹x)` is that of
`M(S)`. For an even scale by `σ > 0`, `σ·f(x/σ)` is.

**2.5. Scaling unevenly keeps a bound, at a price.** Let `f` be 1-Lipschitz and
`g(x) = f(x₁/σ₁, x₂/σ₂, x₃/σ₃)` with every `σᵢ > 0`. Then `σ_min · g` is 1-Lipschitz, where
`σ_min` is the least of the `σᵢ`.

*Proof.* `|g(x) − g(y)| ≤ |(x − y) / σ|` taken coordinate by coordinate, which is at most
`|x − y| / σ_min`. ∎

It is a bound of the scaled solid, and no longer exact: along the larger scales it reads
short by the ratio `σ_min / σᵢ`.

## 3. Joining, meeting and cutting

Write `a ∧ b` for the lesser and `a ∨ b` for the greater of two numbers.

**3.1.** `|a ∧ b − a′ ∧ b′| ≤ |a − a′| ∨ |b − b′|`, and the same for `∨`. So the lesser and
the greater of two 1-Lipschitz fields are 1-Lipschitz, and so is `−f`.

*Proof.* Let `δ = |a − a′| ∨ |b − b′|`. Then `a ∧ b ≤ a ≤ a′ + δ` and `a ∧ b ≤ b ≤ b′ + δ`,
so `a ∧ b ≤ a′ ∧ b′ + δ`. Swap the pairs. For `∨`, negate. ∎

Let `s_A` and `s_B` be the signed distances of solids `A` and `B`.

**3.2. A union.** `u = s_A ∧ s_B` is a bound for `A ∪ B`, and it is **exact outside**.

*Proof.* Outside both, `u(x) = d(x, A) ∧ d(x, B) = d(x, A ∪ B)`, since the nearest point of
a union is the nearer of the two nearest points. On `A ∪ B`, `u ≤ 0`. At a point `p` of
`∂(A ∪ B)`: `p` is in `A` or in `B`, so one of the two is `≤ 0`; `p` is inside neither, so
both are `≥ 0`; hence `u(p) = 0`. `u` is 1-Lipschitz by 3.1 and has the right sign and
zeros, so 2.3 makes it a bound. ∎

**Inside it can fall short.** On a line, let `A = [−2, 1]` and `B = [−1, 2]`. At `0` both
fields are `−1`, and the union `[−2, 2]` is `2` deep there.

**3.3. A meeting.** `m = s_A ∨ s_B` is a bound for `A ∩ B`, and it is **exact inside**.

*Proof.* Inside both, `m(x) = −(d(x, ∂A) ∧ d(x, ∂B))`. Let `r` be that lesser distance,
reached at a point `p` of `∂A`, say. The open ball of radius `r` about `x` lies in both
solids, so `p`, on its rim, lies in `B` (closed); it lies in `A` and not inside `A`, so it
is on `∂(A ∩ B)`. So `r` is the distance to the meeting's surface. The sign and the zeros
are checked as in 3.2, and 2.3 gives the bound. ∎

**Outside it can fall short by any ratio.** Two half-planes meeting in a thin wedge: a point
just beyond the wedge's edge, on its axis, is close to both planes and far from the edge. As
the wedge narrows, the ratio of the field to the distance goes to nothing.

**3.4. A cut.** `A` less the inside of `B` is the meeting of `A` with the outside of `B`,
whose signed distance is `−s_B`. So `s_A ∨ (−s_B)` is a bound for it, exact inside it.

**What this asks of the mesher.** A union, a meeting or a cut of exact fields is exact on one
side and only a bound on the other. Anything that then measures from the field on its weak
side (a swelling outward of a meeting, a shell of a union's inside) is measuring from a
bound and comes out thin. The mesher is told a slack where that matters.

## 4. Raising and turning a plane figure

Let `R` be a plane solid with exact signed distance `s₂`.

**4.1. Raised: a slab.** The solid `R × [−h, h]` (the figure given a thickness of `2h`) has
the exact signed distance

`f(p, z) = (w₁ ∨ w₂) ∧ 0 + |(w₁ ∨ 0, w₂ ∨ 0)|`, with `w₁ = s₂(p)` and `w₂ = |z| − h`.

*Proof.* Outside, the squared distance from `(p, z)` to a point `(q, ζ)` of the slab is
`|p − q|² + (z − ζ)²`, and `q` and `ζ` are chosen apart: the least is
`d(p, R)² + d(z, [−h, h])²`, which is the second term (the first is zero outside). Inside,
the slab's surface is the wall `∂R × [−h, h]` and the two caps `R × {±h}`. The nearest
point of the wall is straight across at the same height, `d(p, ∂R) = −w₁` away; the nearest
cap is straight up or down, `h − |z| = −w₂` away. The depth is the lesser, which is the
first term (the second is zero inside). ∎

**4.2. Turned: a solid of revolution.** Let `R` lie in the half-plane `r ≥ 0` of points
`(r, y)`, and turn it about the `y` axis: the solid of points `(x, y, z)` with
`(√(x² + z²), y)` in `R`. Let `R*` be `R` together with its mirror image in the axis, and
`s*` the exact signed distance of `R*` in the whole plane. Then the solid's exact signed
distance is `f(x, y, z) = s*(√(x² + z²), y)`.

*Proof.* Let `P` have radius `r` and angle `θ`, and `Q` radius `r′` and angle `θ′`. Then
`|P − Q|² = r² + r′² − 2 r r′ cos(θ − θ′) + (y − y′)² ≥ (r − r′)² + (y − y′)²`, with
equality when `θ′ = θ`. So the nearest point of any set that is the same at every angle lies
in the half-plane through `P` and the axis, at the distance measured in that half-plane.
Apply it to the solid (for a point outside) and to its outside (for a point inside): the
distance in space is the distance, in the half-plane, to `R` or to the part of the
half-plane not in `R`. A point of `R*`, or of its outside, that lies across the axis is no
nearer to `(r, y)` than its mirror image on this side, since `|r + r′| ≥ |r − r′|` for
`r, r′ ≥ 0`. So the distances to `R*` and to its outside in the whole plane are the same
numbers. ∎

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

*Proof.* Take the polynomial joins in the general form `F = m − k φ(h)`, with `φ(h) = h²/4`
or `h³/6`. Where `a < b` and `δ < k`: `m = a` and `h = 1 − (b − a)/k`, so
`∂F/∂a = 1 − φ′(h)` and `∂F/∂b = φ′(h)`. They add to one, and both are `≥ 0` because
`φ′(h)` is `h/2` or `h²/2`, between 0 and `1/2`. Where `a > b`, the same with the two
swapped. Three places need care:

- *On the line `a = b`.* From either side `h → 1` and `φ′(1) = 1/2`, so both partials tend
  to `1/2` from both sides: they are continuous across it. **This is where the 4 and the 6
  come from**: `φ(h) = hⁿ / (2n)` has `φ′(1) = 1/2` for every `n`, and the join of degree
  `n` dips by `k / (2n)`.
- *Where `δ = k`.* `h = 0` and `φ′(0) = 0`: the partials are `(1, 0)` or `(0, 1)`, which are
  the lesser's own. They are continuous there too.
- *Second derivatives.* Write `t = b − a`; then `F = (a + b)/2 − ψ(t)` with
  `ψ(t) = |t|/2 + k φ(1 − |t|/k)`. `ψ″(t) = φ″(1 − |t|/k) / k`, an even function, so it is
  continuous across `t = 0`. At `|t| = k` it tends to `φ″(0)/k`, which is `1/(2k)` for the
  quadratic join (a jump, since beyond it `ψ″ = 0`) and `0` for the cubic one. So the
  quadratic join is once differentiable and the cubic twice.

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
   within the set where `f ∧ g ≤ D`. If `f` and `g` are exact outside, that set is `A ∪ B`
   swollen by `D`: **a smooth join thickens the union by at most `D`**, and a box that
   bounds the union bounds the join once it is widened by `D` on every side.
3. For the polynomial joins, `u = f ∧ g` wherever `|f − g| ≥ k`: **nothing changes further
   than that from where the two fields are equal.** The exponential join changes
   everything, by an amount that dies away as `exp(−|f − g|/k)`.

*Proof.* 1 is 5.3. 2 is 5.1 applied at each point, and 4.3 for the last sentence. 3 is
5.1. ∎

**It is a bound and not the distance.** Where the join is at work the field's slope is a
mix of the two fields' slopes by the shares of 5.2. For two unit slopes at an angle `θ` to
each other and equal shares, the mixed slope has length `cos(θ/2)`: two walls meeting
square give `0.707`. A slope under one is a field that reads short. So `u` is safe and
slack through the join, and the mesher is told so.

**5.5. Several at once.** The exponential join of `n` fields,
`−k ln(Σ exp(−aᵢ/k))`, lies between `(∧ aᵢ) − k ln n` and `∧ aᵢ`, reaching the lower limit
where all are equal; and joining two at a time gives the same value in any order (the sums
simply add). The polynomial joins applied two at a time are **not** the same in every
order, and `n` fields joined so dip by at most `(n − 1) D`.

*Proof.* The greatest term of the sum lies between the sum over `n` and the sum itself. For
the nesting: each join is at least the lesser of its two inputs less `D`, and joins rise
with their inputs (5.2), so each further field costs at most one more `D`. ∎

**5.6. The smooth meeting and the smooth cut.** `−F(−a, −b)` lies between `a ∨ b` and
`a ∨ b + D`, with the same shares and the same smoothness. So a smooth meeting of two solids
lies inside their meeting, eaten back by at most `D` where they cross, and a smooth cut
likewise; both are 1-Lipschitz, and both are bounds for the solids they make.

**5.7. One width that means what it says.** With `k` replaced by `4k` in the quadratic join,
by `6k` in the cubic and by `k / ln 2` in the exponential, the greatest dip is `k` itself: a
join "of `k` metres" then thickens by at most `k` metres, whichever kind it is. The
polynomial joins then reach as far as the two fields are `4k` or `6k` apart.

## 6. What follows for the modeller

- **A form raised or turned is exact** (4.1, 4.2) when the plane figure's own distance is.
  The shape library holds each figure as exact lines and arcs, so that distance can be
  exact; where a figure is a sampled curve, the field is exact for the sampled outline and
  off from the true curve by the sampling's stated error.
- **A tree of unions, meetings, cuts and smooth joins of 1-Lipschitz fields is 1-Lipschitz**
  (3.1, 5.3, 5.6), so it never exceeds the distance to the surface it makes (2.3). That is
  all pruning needs.
- **It is exact only on the sides named in 3.2 to 3.4, and slack through every smooth join**
  (5.4). A node that needs a true distance (a swelling, a shell) is given one where the tree
  has one, and otherwise says what it was given.
- **Bounds grow by each join's dip** (5.4, 5.5): a box about the parts, widened by `D` for
  one join, by `(n − 1) D` for `n` polynomial joins nested, by `k ln n` for `n` joined at
  once exponentially.
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
  Visual Computer* 20(6), 2004.
- The exponential join's limits as a numbered result: Molnar and Ames, *IEEE Control Systems
  Letters* 7, 2023 (Theorem 4); Rabiee and Hoagg, *Automatica* 171, 2025 (Proposition 1);
  Igel, ICML 2024 (equations 5 and 6).
- The quadratic join's quarter of `k`: Li et al., "SDFStent", *Computer Methods and Programs
  in Biomedicine* 287, 2026.
- The cubic join's sixth of `k`, and the general `k / (2n)` of 5.2: stated as a result in no
  peer-reviewed paper found. It is proved above.
