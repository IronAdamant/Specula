# Research: Rust's costs, and how to turn them into strengths

Written 2026-10-13. The owner's position: Rust is as efficient as C++ and safer, and its costs (unsafe at the edges,
the strictness of the language, compile times) should be made non-issues or turned into strengths. This document
measures where we stand, lays out how, and feeds the **Rust track** in [ROADMAP.md](../ROADMAP.md).

## Where we stand (measured 2026-10-13)

rustc 1.97.1, `x86_64-pc-windows-gnu`, 32 hardware threads, `cargo build -p specula`:

| Build | Time |
|---|---|
| Clean dev build (whole engine, `opt-level = 1`) | **8.2 s** |
| Incremental, after touching the `specula` bin crate | **1.2 s** |
| Incremental, after touching `specula-core` (everything above it rebuilds) | **2.0 s** |
| Clean release build (thin LTO, 1 codegen unit) | **12.7 s** |
| Release `specula.exe` size | **1.16 MB** |

**Unsafe code is already confined.** Counting `unsafe {`, `unsafe fn` and `unsafe impl` sites:

| Crate | Sites |
|---|---|
| `specula-gpu` | 87 |
| `specula-platform` | 36 |
| `specula-sys` | 11 |
| `specula-core` | 0 |
| `specula-protocol` | 0 |
| `specula` | 0 |

Every site carries a `// SAFETY:` comment. Today that is checked by review, not by a machine.

**The zero-dep rule is already the biggest compile-time win.** Engines built on deep crate trees compile hundreds of
crates on a clean build; we compile six. The job now is to **keep** it this fast as the engine grows to ten or twenty
times its size, and to make the fast path structural rather than lucky.

---

## 1. Compile times: from non-issue to strength

### Keep it fast

- [ ] **Compile-time budgets, enforced like performance budgets (F3).** For example:
  - clean dev build under 30 s
  - incremental engine rebuild under 3 s
  - game-module edit → reloaded and running under 2 s

  A CI step records `cargo build --timings` (built into Cargo, writing an HTML report per crate) and fails on a budget
  miss. Agents see the slowest crates as data.
- [ ] **Thin game modules.** Hot-reloaded game modules depend only on `specula-sdk`: small types and a C-ABI function
  table, never the engine crates. Editing gameplay never recompiles the engine, so the 2 s loop holds even when the
  engine is huge.
- [ ] **Generic discipline at crate boundaries.** Generic code is compiled again in every crate that uses it
  (monomorphisation), so heavy generics in public APIs push compile cost onto callers. Use concrete types or `dyn` at
  crate boundaries, and keep generics inside crates.
- [ ] **Crates sized for parallelism.** Crates compile in parallel when they don't depend on each other. Keep the
  dependency graph wide, with lanes as sibling crates, not a tall chain. `--timings` shows the critical path.
- [ ] **Per-package optimisation in dev.** Engine crates at `opt-level = 2` in dev, since they rarely change and run
  hot. Game modules at `opt-level = 0`, since they change constantly. The dev engine runs fast and module edits
  compile fast.
- [ ] **`cargo check` first.** Agents type-check (no code generation) before building. Most errors are found in the
  check step, at a fraction of build time.

### Turn it into a strength

- [ ] **Compiler errors as data: a `logic_build` tool.** The engine runs Cargo with `--message-format=json` and returns
  diagnostics as structured data: error code, file, line and column spans, explanation, and **machine-applicable
  suggestions**. Fixes rustc marks as machine-applicable are applied automatically, as `cargo fix` does. Compile errors
  become the cheapest bugs in the project: found in seconds, located exactly, often fixed automatically. In C++
  engines, the equivalent bug is often a crash found days later.
- [ ] **A build scheduler for swarms.** Cargo locks the whole build directory, so two agents building in one target
  directory take turns. Rust's 2025H2 goal describes `cargo check` stalling `cargo build` this way. Today we give each
  worktree its own target directory, as in M0. Long-term, `logic_build` becomes a **build service**: the engine queues
  build requests, merges duplicates (ten agents asking for the same build get one build), and reports progress. The
  lock becomes a scheduler.
- [ ] **Adopt Cargo's shared cache when it's stable.** Cargo has fine-grained build-directory locking in progress
  (cargo PR #16155), and a **cross-workspace shared build cache** is an accepted 2026 Rust project goal (nightly first).
  Once stable, every worktree and every agent shares compiled artefacts.

### Toolchain levers to evaluate (stable only)

**Policy:** Specula builds on **stable** Rust. Nightly features are evaluated in a side CI job and adopted when they
reach stable.

| Lever | Status (2026-10) | Plan |
|---|---|---|
| **LLD linker** | The default on Linux x86_64 since Rust 1.90 (Sept 2025). The Rust team measured ~7× faster linking on an incremental ripgrep rebuild. Our windows-gnu self-contained setup currently links with the bundled GNU `ld`, and the toolchain also ships `rust-lld.exe`. | [ ] Try `rust-lld` on windows-gnu (still the toolchain's own linker, so zero third-party) and measure link time with `--timings`. |
| **Cranelift backend** for debug builds | Nightly-only preview component (`rustc-codegen-cranelift-preview`); not on stable | [ ] Side-job evaluation on nightly; adopt for dev builds if it stabilises |
| **Parallel compiler front end** (`-Z threads`) | Nightly; an accepted 2026 project goal, still working towards stabilisation | [ ] Same: evaluate on nightly, adopt on stable |
| **Shared build cache** | 2026 goal, nightly first | [ ] Adopt when stable (above) |

---

## 2. Unsafe code: from "trusted by convention" to "checked by machine"

The goal is for the compiler to *prove* memory safety for everything except a small, audited FFI layer, and for every
rule about that layer to be enforced mechanically.

- [ ] **`#![forbid(unsafe_code)]` in every crate without FFI.** That's `specula-core`, `specula-protocol` and `specula`
  today, plus every future world, physics, animation, UI, audio-mixing and SDK crate. The compiler then guarantees
  those crates have no memory-safety bugs. That's **physics, animation, gameplay and UI, all proven memory-safe**. A
  C++ engine can't offer that.
- [ ] **Workspace lints in `Cargo.toml`** (`[workspace.lints]`), so the rules live in one place:
  - `clippy::undocumented_unsafe_blocks = "deny"`: a missing `// SAFETY:` comment becomes a build error, not a review
    note.
  - `clippy::missing_safety_doc = "deny"`: every `unsafe fn` documents its contract.
  - `unsafe_op_in_unsafe_fn = "deny"`: unsafe operations inside unsafe functions still need their own block and
    comment (today only `win32.rs` sets it).
- [ ] **Move to Rust edition 2024** (we're on 2021). It makes `extern` blocks explicitly `unsafe extern` and warns on
  unsafe operations in unsafe functions by default. The migration is mostly mechanical (`cargo fix --edition`).
- [ ] **Safe walls.** FFI crates expose only safe APIs; no `unsafe` crosses a crate boundary. The one unavoidable
  boundary, the hot-reload C ABI, is generated by a single macro in `specula-sdk` with layout tests, as `vk.rs` has
  today.
- [ ] **Fuzz every parser.** Everything that reads untrusted bytes is attack surface and crash surface: JSON, HTTP,
  PNG, inflate, and later TrueType, glTF, WAV/Vorbis and saves. That includes MCP requests, downloaded assets and
  players' save files. Build a small zero-dep mutation fuzzer in the workspace, seeded from test files and run
  time-boxed in CI. Agents are good at writing fuzz targets and triaging their crashes.
- [ ] **Keep Vulkan validation and sync validation** in every GPU test (already done), and run the GPU live check on
  both vendors at every milestone.
- [ ] **Unsafe-auditor role.** A reviewer agent re-checks every diff that touches an `unsafe` site: is the SAFETY claim
  true? The unsafe-site count per crate is tracked, so growth is visible.
- [ ] **Miri** (a Rust interpreter that detects undefined behaviour) for any non-FFI unsafe code that appears later,
  such as custom allocators or arenas. Miri can't run FFI, which is why FFI stays in the three edge crates.

---

## 3. The language's strictness, as a strength

Rust's strictness pushes code towards the architecture the engine wants anyway, and it turns the compiler into a
reviewer for a swarm.

- [ ] **Data-oriented state.** The engine owns all state in arrays keyed by stable or generational ids, never webs of
  references. This sidesteps borrow-checker fights. It is also **cache-friendly (fast)**, **deterministically ordered
  (F2)** and **trivially serialisable** (saves, replays, world files). Rust's constraints and our foundations point the
  same way.
- [ ] **Types as guardrails for agents:**
  - newtype ids (`EntityId`, `MeshId`, already used)
  - units where mix-ups are likely (ticks vs. seconds, pixels vs. metres)
  - exhaustive `match` on enums: when an agent adds a variant, the compiler lists **every** place that must handle it
  - `#[must_use]` on results and handles

  In a swarm, "the compiler found all 14 places" beats "someone remembered 12 of them".
- [ ] **Fearless concurrency.** `Send` and `Sync` make data races compile errors. That matters when render, physics
  and streaming go multithreaded, and when many agents write the code.
- [ ] **Foundations as lints.** Clippy's `disallowed_methods` and `disallowed_types` (configured per crate in
  `clippy.toml`) turn the determinism rules into build errors in simulation crates:
  - `std::collections::HashMap` / `HashSet` with the default hasher → use ordered or fixed-hash containers (determinism
    rule 3)
  - `f32::sin`, `cos`, `atan2`, `exp`, `ln`, `powf` (and `f64`) → use `specula_core::math` (rule 4)
  - `std::time::Instant::now` and `SystemTime::now` → the simulation knows only the tick (rule 1)

  A swarm can't accidentally break determinism. **This is a strength C++ engines can't easily copy.** The exact
  clippy paths for primitive methods like `f32::sin` still need confirming with a test when this is configured; if
  clippy can't match them, a small workspace test that scans simulation crates for those names does the same job.

---

## 4. What we will not do

- **Depend on nightly Rust.** Stable only. Nightly features are evaluated, not adopted.
- **Use third-party build tools** (sccache, mold and similar). The zero-dep spirit extends to the build. The toolchain's
  own components (rust-lld, Cranelift once stable) are fine.
- **Trade safety for compile speed.** For example, we won't blanket `unsafe` to dodge the borrow checker.

## Sources

- Our own measurements (above), `cargo build --timings`.
- Rust blog, ["Faster linking times with 1.90.0 stable on Linux using the LLD linker"](https://blog.rust-lang.org/2025/09/01/rust-lld-on-1.90.0-stable/);
  [Rust 1.90.0 release](https://blog.rust-lang.org/2025/09/18/Rust-1.90.0/).
- Rust project goals: [parallel front end (2026)](https://goals.rust-lang.org/2026/parallel-front-end.html),
  [Cargo cross-workspace cache (2026)](https://goals.rust-lang.org/2026/cargo-cross-workspace-cache.html),
  [Cargo build-dir layout (2025H2)](https://rust-lang.github.io/goals/2025h2/cargo-build-dir-layout.html);
  [cargo PR #16155](https://github.com/rust-lang/cargo/pull/16155) (fine-grained locking).
- [rustc_codegen_cranelift readme](https://rust.googlesource.com/rust/+show/HEAD/compiler/rustc_codegen_cranelift/Readme.md).
