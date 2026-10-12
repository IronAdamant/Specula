# Design: determinism

Status: researched 2026-10-13, not yet implemented. Foundation **F2** in [ROADMAP.md](../ROADMAP.md).

**The rule:** the same starting world, the same seed and the same inputs give the same simulation, bit for bit, on
every run.

## Why it comes first

Determinism is what turns a swarm of agents into a QA department:

- **Headless playtests.** An agent scripts inputs, runs 10,000 ticks and checks the result. If it isn't deterministic,
  a failure might not happen again, and the agent can't tell a real bug from noise.
- **Replays.** A journal plus a seed reproduces any session exactly. A bug report becomes a file an agent can replay
  and bisect.
- **Regression checks.** If a change alters the hash of a replay that shouldn't change, it's flagged immediately.
- **Future multiplayer.** Lockstep and rollback netcode both require it.

Retrofitting determinism means auditing every system that was written without it, so it's a rule from M1 on.

## Levels

| Level | Guarantee | Needed for | Target |
|---|---|---|---|
| D1 | Same binary, same machine | Replays, local playtests | M1 |
| D2 | Same source, any x86-64 Windows/Linux machine, msvc or gnu build | Playtest swarms on any machine, CI | M1 |
| D3 | Across CPU architectures (x86-64 ↔ ARM) | Mac/ARM handhelds, cross-platform multiplayer | Designed for; tested when ARM targets exist |

Box2D v3 meets D3 in practice. It tests x64 and ARM with MSVC, Clang and GCC in CI, using a scene whose final
transforms are hashed ([Catto, "Determinism", 2024](https://box2d.org/posts/2024/08/determinism/)). Our rules follow
the same approach.

## What is deterministic and what isn't

**The simulation** is deterministic. It includes:
- world state
- game logic modules
- physics
- gameplay-relevant animation (hitboxes, root motion)
- the RNG

**Everything else** reads simulation state but never writes it, so it is free to be non-deterministic:
- rendering
- display-only animation blending
- audio
- UI
- wall-clock time
- network timing

Agents' commands are **inputs**. They arrive at unpredictable wall-clock times, but each one is applied at a tick
boundary and journaled with that tick number. The journal is therefore a complete replay log.

## Rules

1. **Fixed timestep.** The simulation advances in fixed ticks (60 Hz by default, configurable per game). The tick
   number is the only clock the simulation can see. Rendering interpolates between the last two ticks
   ([Fiedler, "Fix Your Timestep!"](https://gafferongames.com/post/fix_your_timestep/)).
2. **One RNG, seeded.** The engine provides a small, fast, seedable generator (PCG or xoshiro family, written in
   `specula-core`). There is one stream per system, so adding random calls to one system doesn't shift another. The
   SDK exposes no other randomness.
3. **No random iteration order.** Rust's `HashMap` default hasher is seeded randomly *per process*, so iterating one
   gives a different order each run. Simulation code uses `Vec` indexed by stable ids, `BTreeMap`, or a `HashMap` with a
   fixed hasher, and never iterates a `HashMap` where order affects results. A test helper runs a system twice in
   separate processes and compares hashes.
4. **Our own transcendental functions.** `f32::sin`, `cos`, `atan2`, `exp`, `ln` and `powf` call the platform maths
   library. It differs between the msvc and gnu builds and between OSes, and can differ in the last bit. Box2D found
   `atan2f` differs across platforms and wrote its own. We implement these in `specula-core::math` (polynomial
   approximations with fixed coefficients), and simulation code uses only those. `sqrt` is correctly rounded under IEEE
   754, so it is safe to use directly. So are `+ - * /`.
5. **No fused or reordered floating point.** Rust doesn't fuse `a*b+c` into FMA or reorder float operations on its
   own. Explicit `mul_add` is allowed only where it is used on every platform. Hand-written SIMD must reduce in a fixed
   lane order.
6. **Floating-point environment.** At each tick start, the simulation checks the CPU's floating-point control register
   (on x86-64, MXCSR rounding mode and flush-to-zero bits) and resets it if needed. *Why:* DLLs loaded into the process,
   such as drivers or overlays, can change it.
7. **Deterministic parallelism.** Multithreaded systems must produce the same result whatever the thread timing. Work
   is split by index, and results are merged in index order. Box2D merges per-thread bit arrays with OR to get a
   fixed contact order without sorting. Creation order is part of the state, so ids are allocated deterministically.
8. **Code is part of the input.** A hot-reloaded module is journaled as "module X reloaded at tick N, build hash H".
   A replay recorded with build H is expected to match only with build H. Mismatches across builds are reported, not
   treated as failures.
9. **Floats are serialised exactly.** World files and the journal store floats so they round-trip bit-exactly
   (shortest round-trip decimal, or hex bits where needed). Otherwise saving and loading changes the simulation.

## Verification

- **`world_hash`**: a 64-bit hash of all simulation state, taken in a fixed order every tick (cheap to compute) and
  exposed as a tool.
- **Replay test in CI**: record a scenario journal, then replay it on the msvc, gnu and ubuntu jobs. All must reach the
  same final hash. This checks D2 on every push.
- **Twin-run test**: run the same scenario in two separate processes, so `HashMap` seeds differ, and compare hashes
  every tick.
- **Desync detector** (M3): when hashes diverge, binary-search for the first divergent tick, then compare per-component
  hashes to name the system responsible. The result is structured data an agent can act on.

## Costs and limits

- Our own trig functions are slightly slower and slightly less accurate than the platform's. That doesn't matter for
  games, and Box2D accepts the same trade.
- Fixed-point maths isn't needed. Floats with these rules are enough, and Box2D chose floats too.
- Rendering stays free to use platform maths and GPU maths, because the GPU never feeds the simulation. **The GPU is
  never the source of gameplay truth.** GPU-driven particles are cosmetic only.

## Integration plan

- **M1:**
  - fixed tick clock
  - seeded RNG
  - `specula-core` deterministic maths
  - ordered containers in world code
  - exact float serialisation
  - tick-stamped journal
  - `world_hash`
  - replay test in CI
- **M3:**
  - the SDK exposes only deterministic APIs
  - module build hashes in the journal
  - FP-environment guard
  - desync detector
  - physics and gameplay animation built under these rules
- **Later:** an ARM build in CI (D3) when an ARM target exists.
