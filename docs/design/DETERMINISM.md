# Design: determinism

Status: researched 2026-10-13. Deterministic maths (`dmath`) built 2026-10-13; the M1 groundwork with M1; the M3 items
(the floating-point guard, module builds in the journal, the lints) on 2026-10-22 ([below](#guards-and-lints-m3)).
Foundation **F2** in [ROADMAP.md](../ROADMAP.md).

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
   `atan2f` differs across platforms and wrote its own. We implement these in `specula_core::dmath` (polynomial
   approximations with fixed coefficients), and simulation code uses only those. `sqrt` is correctly rounded under IEEE
   754, so it is safe to use directly. So are `+ - * /`.
5. **No fused or reordered floating point.** Rust doesn't fuse `a*b+c` into FMA or reorder float operations on its
   own. Explicit `mul_add` is allowed only where it is used on every platform. Hand-written SIMD must reduce in a fixed
   lane order.
6. **Floating-point environment.** At each tick start, the simulation checks the CPU's floating-point control register
   (on x86-64, MXCSR rounding mode and flush-to-zero bits) and resets it if needed. *Why:* DLLs loaded into the process,
   such as drivers or overlays, can change it. Built: see [the guard](#the-floating-point-guard).
7. **Deterministic parallelism.** Multithreaded systems must produce the same result whatever the thread timing. Work
   is split by index, and results are merged in index order. Box2D merges per-thread bit arrays with OR to get a
   fixed contact order without sorting. Creation order is part of the state, so ids are allocated deterministically.
8. **Code is part of the input.** A hot-reloaded module is journaled as "module X reloaded at tick N, build hash H".
   A replay recorded with build H is expected to match only with build H. Mismatches across builds are reported, not
   treated as failures. Built: see [module builds](#module-builds-in-the-journal).
9. **Floats are serialised exactly.** World files and the journal store floats so they round-trip bit-exactly
   (shortest round-trip decimal, or hex bits where needed). Otherwise saving and loading changes the simulation.

## Deterministic maths: `specula_core::dmath`

Built 2026-10-13 (rule 4). Simulation code calls `dmath::sin(x)` instead of `x.sin()`.

**API.** For `f32` (what the engine uses): `sin`, `cos`, `sin_cos`, `tan`, `asin`, `acos`, `atan`, `atan2`,
`exp`, `exp2`, `ln`, `log2`, `pow`, `hypot`, and `floor`, `ceil`, `round`, `round_ties_even`, `trunc`, `fract`.
The same set exists for `f64` with a `_f64` suffix, except `pow`. `dmath::VERSION` goes into replay headers.

**How.** The functions use only IEEE basic operations (`+ - * /`, `sqrt`), conversions and integer bit operations.
Those are correctly rounded, so every platform gets the same bits. The `f32` functions compute in `f64` or
double-double (see below).
- **Polynomials:** truncated Taylor and `atanh` series with exact rational coefficients, on arguments reduced far
  enough that truncation is below 2⁻⁶⁰.
- **Tables:** π, 2/π, ln 2 and `atan(k/8)`, generated with exact integer arithmetic.
- **sin, cos and tan:** reduced by π/2 with a 4-part Cody–Waite split kept as a double-double. Above 1.6·10⁶ they use
  Payne–Hanek with 1216 bits of 2/π, so accuracy holds up to `f64::MAX`. There is no cliff for huge arguments.
- **Special values:** follow C99 Annex F, like `std`: NaN, ±0, ±inf, and the `atan2` and `pow` grids.

**Rounding `f32` results.** A plain `f(x as f64) as f32` rounds twice. When the `f64` result lands exactly on an `f32`
midpoint, ties-to-even decides the result instead of the true value. An exhaustive review scan found 17 such inputs in
the first version: ln ×5, cos ×4, and sin, atan, exp2 and acos ×2 each. So the `f32` functions now work in two steps
(Ziv's strategy):
- **Fast path:** they use the fast `f64` result unless it lies within 32 `f64` ulp of an `f32` midpoint, which is about
  1 input in 2²³.
- **Accurate path:** otherwise they compute a double-double result, with relative error ≲ 2⁻⁶², and round it with the
  tie decided by the sign of its low part.
- **pow:** the trust radius grows with |y·ln x|, because ln's error is amplified by exp.

`pow` computes exact results exactly:
- `pow(x, 2) == x * x`, `pow(x, 0.5) == sqrt(x)` and `pow(x, -1) == 1 / x`.
- Integer powers up to 64 use double-double binary powering.
- Half-integer powers of perfect squares are computed through exact square roots.
- Powers of two are computed as `2^(j·y)`.

So an exact midpoint such as `4097² = 16785409` ties to even, as IEEE requires.

**Accuracy.** The `f32` functions are **correctly rounded**, checked in two ways:
- **One-argument functions:** an exhaustive scan (`dmath::tests::exhaustive`) checks every input. The scan also requires
  the fast and accurate paths to agree on every input. Each input whose exact value lies within 2⁻²⁴ `f32` ulp of a
  midpoint was then checked against an independent 120-digit decimal oracle (`scripts/dmath_oracle.py`). No other input
  is close enough to a midpoint for the double-double error to matter.
- **atan2, pow and hypot:** 20,000 random samples each, with integer and half-integer exponents for pow, all agreeing
  with the oracle.

The first version's tests compared against mingw-w64's `f64` libm rounded to `f32`. That reference double-rounds as
well, so it could not see these cases. The tests now pin them against oracle values.

The `f64` functions (`*_f64`) are not correctly rounded. Measured against `std` and the exact references:
- sin, cos, exp, exp2, ln, log2 and hypot are within 1 ulp.
- tan is within 2 ulp.
- atan, atan2, asin and acos are within 1 ulp.
- sin and cos at 1e22, 1e300 and `f64::MAX` are within 1 ulp of 1600-bit references.

**Speed.** Release build on windows-gnu, compared with mingw-w64's libm. The machine was noisy, so these are rough:
- **Faster:** sin, cos and exp are about 0.5×, ln and pow 0.6×, hypot 0.7×.
- **About the same:** atan and atan2 at 1.0×.
- **Slower:** asin is 1.3× and tan 2.5×.

Correct rounding costs about 10–40% over casting the `f64` result, which is mostly the midpoint check. The double-double
path runs for about 1 input in 2²³.

A faster libm such as glibc or UCRT would narrow the gap. To measure:
`cargo test -p specula-core --release dmath::tests::bench -- --ignored --nocapture`.

**Why `std` can't be used.** Two problems were measured on this machine while building dmath:
- **`f32::round(0.49999997)` returns `1.0` on windows-gnu.** mingw-w64's `roundf` computes `floor(x + 0.5)`. Even
  rounding goes through libm.
- **`f64::sin` is far off on windows-gnu.** mingw-w64 uses x87 `fsin`, which reduces with a 66-bit π:
  - `sin(-6.2832)` is 5 ulp off.
  - `sin(-946140.89…)` is 11,000 ulp off.

**What is safe from `std` in simulation code:**
- `+ - * /`
- `sqrt`
- `abs`, `copysign` and negation
- `min`, `max` and `clamp`
- `to_radians` and `to_degrees` (a multiply)
- `recip`
- `as` casts
- `to_bits` and `from_bits`
- comparisons and `total_cmp`

**What isn't:** every transcendental function, plus `floor`, `ceil`, `round`, `trunc` and `fract` (use the dmath
versions), `powi` (compiler-rt; LLVM may fold it differently), `mul_add`, `cbrt`, `exp_m1`, `ln_1p`, `log10`, and
`sinh`, `cosh`, `tanh` and their inverses. Add those to dmath first if the simulation needs them.

**Golden values.** `golden_bits_f32` and `golden_hash_f64` pin the output bits. If one ever has to change, bump
`dmath::VERSION`, because old replays won't match any more.

## Verification

- **`world_hash`**: a 64-bit hash of all simulation state, taken in a fixed order every tick (cheap to compute) and
  exposed as a tool.
  - **What it covers.** The entities' content by id, the environment and the **id counter**. The id counter is
    included because it decides the ids of future spawns, and so the future of the run.
  - **Rejected calls count.** Ids are never handed out twice: a command rejected by a claim (or any rolled-back
    command) still uses up the ids it handed out, so it moves the id counter, and with it the hash.
  - **What to compare.**
    - A world hash always matches the **replay of its own journal** (`world_hash verify`, and the CI replay test):
      the journal notes the counter after rejections too.
    - Two runs' hashes match only when they received the same calls, *rejected ones included*, at the same ticks.
      Runs that differ only in rejected calls diverge in the counter (and in the ids of later spawns), not in what
      was built.
    - To compare such runs, compare entities by path and content, or the game's own simulation hash (Sepulcra's
      `State::hash`), which never sees engine ids.
- **Replay test in CI**: record a scenario journal, then replay it on the msvc, gnu and ubuntu jobs. All must reach the
  same final hash. This checks D2 on every push.
- **Twin-run test**: run the same scenario in two separate processes, so `HashMap` seeds differ, and compare hashes
  every tick.
- **Desync detector** (M3): when hashes diverge, binary-search for the first divergent tick, then compare per-component
  hashes to name the system responsible. The result is structured data an agent can act on.

## Guards and lints (M3)

Built 2026-10-22. `scripts/determinism_check.py` checks them in a running engine.

### The floating-point guard

`specula_sys::fpenv` reads and restores the control register with `core::arch::asm!` (no FFI):

| Target | Register | Bits checked | Default |
|---|---|---|---|
| x86-64 (and x86 with SSE) | MXCSR | rounding, FTZ, DAZ, exception masks (`0xffc0`) | `0x1f80` |
| AArch64 | FPCR | RMode, FZ, DN, AHP, FZ16, trap enables, FEAT_AFP FIZ, AH, NEP (`0x07c89f07`) | `0` |
| other | none | nothing (`fpenv::SUPPORTED` is false) | |

Status flags are sticky records, not modes, and are ignored. The x87 control word isn't checked: Rust does its float
maths in SSE registers on these targets.

**Where it checks.** Each change found is restored at once and recorded (`crates/specula/src/fpguard.rs`):
- `startup`: as the engine starts, in case something loaded first changed it;
- `between_ticks`: as each tick begins (rendering, the GPU driver or an overlay ran in between);
- `before_command`: before each agent command, whatever it is (world edits, model builds, sight, saves, playtests);
- around every module call (`specula_logic::ModuleCode::call`): before the call (`before_module_call`), whenever the
  module calls back (so the engine's answers to it run in the default environment), and when it returns
  (`module:<name>`). Each call also starts the module in the default environment.

**Where it reports.** `perf_frame`'s `problems` gets one `fp_env_restored` item (count, sources, the last change);
`engine_info`'s `determinism` (`fp_env` for the engine, `fp_env_modules` for module calls); `logic_status` per module
(`fp_restored`, `fp_last`). A module that changes the rounding mode can't desync the simulation: it is put right at the
boundary, and the change is counted against it (tested with `specula_logic::fixtures::FlipRounding`).

**Not guarded:** threads other than the engine's. The balance lab's sweep workers and connection threads start from
whatever their creator had (on Linux, threads inherit the environment); none of them run foreign code today.

### Module builds in the journal

- Every load, reload, unload and disable of a module is a journal entry of kind `module` (`world_history {kinds:
  ["module"]}`): tick, module, event, generation, build hash, and for a reload the build it replaced. It's a record, not
  a world change, so it can't be undone (refused by id; "undo my last change" skips it).
- **The build hash** is a hash of the library's bytes with the link time stamps a PE linker writes left out (COFF
  TimeDateStamp, CheckSum, export and debug directory stamps; measured 2026-10-22: two builds of the same source differ
  in exactly those 5 bytes). The same source rebuilt keeps its hash; any code change changes it. *Compatibility:* the
  formula changed on 2026-10-22, so save games and replay files written before then name their modules with the old
  hash and report "another build" for the same code ([SAVES.md](SAVES.md)); re-save or re-record them.
- **The desync detector** keeps the module history too (last 256 events, older ones folded into a base, which is
  only right for ticks after the newest folded event: `known_from_tick`; earlier ticks answer `modules_known: false`).
  Traces carry it (`modules` in `specula.trace/v1`, optional, so older traces still read). `desync_compare` works out which build of
  each module ran in each run at the divergent tick and names any module whose build, or load tick, differs, first in
  `components` (`module ambulo: build ... in a, ... in b`). Runs that match although builds differ get
  `module_builds_differ`. A trace without module history gives `modules_known: false`.
- **Playtest replays** record the loaded modules' builds (`modules` in the replay file). A replay is refused with
  `module_build_mismatch` (naming each module, recorded and loaded build) when they differ, unless
  `allow_module_mismatch: true`, which runs it and reports `module_builds_differ`. Replay files without the list
  aren't checked. Checkpoints now also hash the module host's state (`logic` part: stores, RNG positions, events); a
  replay file whose checkpoints have no `logic` part (written before 2026-10-22) is checked without it
  (`playtest::LATE_PARTS`).
- Save games already name each module's build and report a mismatch on load ([SAVES.md](SAVES.md)). The journal's own
  replay (`world_hash {verify}`, `desync_compare {b: "replay"}`) replays world states, so module code can't change it.

### Lints

- **Simulation crates** carry one `clippy.toml` (copied from `crates/specula-physics/clippy.toml`): `disallowed-types`
  bans `HashMap`, `HashSet`, `RandomState`; `disallowed-methods` bans the platform maths above (every transcendental,
  `floor`/`ceil`/`round`/`trunc`/`fract`, `powi`, `mul_add`, for `f32` and `f64`), `Instant::now`, `SystemTime::now`,
  `thread::spawn` and `thread::scope`. Crates: specula-physics, specula-scintilla (effects are cosmetic, but
  captures of them repeat bit for bit on every platform: [VFX.md](VFX.md)), specula-anim, specula-lex,
  specula-balance, specula-sdk, specula-via, specula-mens, specula-locus, specula-model (recipes build the meshes
  ragdolls fit their bones to and the hulls bodies collide with), sepulcra, ambulo (the module template). CI's clippy
  enforces it; `crates/specula/tests/determinism_lints.rs` holds the list.
- Every hit was fixed (dmath instead of `round`/`floor`/`fract`) or allowed with a reason (specula-model's
  `HashMap`s weld vertices and dedupe edges by lookup, or are iterated only to count or take maxima; each function
  says which): physics' stage timings, Lex's
  and the balance lab's wall-clock deadlines (a run they stop is reported as stopped, never as a result), the balance
  sweep's threads (results merged by job index), presentation code (contact-sheet framing), tests and benchmarks.
- `crates/specula/tests/determinism_lints.rs` keeps it so: every workspace member must be listed as simulation (and
  then carry the identical `clippy.toml`) or not; a new crate fails the test until it is classified.
- **Modules** built by agents may have no `clippy.toml`, and clippy is too slow for the reload budget, so
  `logic_build` scans the module's `src/` itself (`specula_logic::lint`: a small Rust tokenizer, the same names, plus
  `static mut` and `thread_local!`; files pulled in from outside `src/` with `#[path]` or `include!` aren't read;
  bounded: 512 files, 256 folders, 1 MiB a file, 4 MiB in all, 40-70 ms per MB) and adds each hit as a warning (`code: "determinism::<rule>"`, file, line, column,
  a help note); `determinism_warnings` counts them. They don't stop the build. `// determinism: allowed: <why>` at the
  end of the line, or alone on the line above, marks a deliberate use. It reads names, not types, so `x.round()` on a module's own type is
  reported too.

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
  - `specula-core` deterministic maths (done: `dmath`)
  - ordered containers in world code
  - exact float serialisation
  - tick-stamped journal
  - `world_hash`
  - replay test in CI
- **M3:**
  - the SDK exposes only deterministic APIs (done)
  - module build hashes in the journal (done 2026-10-22)
  - FP-environment guard (done 2026-10-22)
  - desync detector (done)
  - determinism lints in simulation crates and for modules (done 2026-10-22)
  - physics and gameplay animation built under these rules (physics P1-P3 and animation A3 pass the lints)
- **Later:** an ARM build in CI (D3) when an ARM target exists.
