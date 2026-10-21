# Design: game modules (hot-reloaded Rust logic)

Status: built 2026-10-18 (M3 lane "module host"). Decision record: [ENGINE_PLAN.md](../ENGINE_PLAN.md) ("Game logic:
hot-reloaded Rust modules"). Rules it follows: [DETERMINISM.md](DETERMINISM.md), [AGENTS.md](../../AGENTS.md).

**Goal:** game logic that no building block covers is written as Rust **modules**: small `cdylib` crates the engine
builds, loads, runs every tick and reloads when they change, while the world keeps running. Gameplay as data (Lex) is
the first choice; modules are the escape hatch, and Lex calls into them through named **native hooks**.

## The pieces

| Crate / file | Role |
|---|---|
| `crates/specula-sdk` | The one crate a module depends on: the `Module` trait, `Cx` (what a module may touch), `Value` (data crossing the boundary), the C ABI tables and `export_module!`. Re-exports `dmath`, `Vec3`, `Quat`. |
| `crates/specula-logic` | The engine side, and all of the host's `unsafe`: `ModuleCode` (load a library from a versioned copy, check its ABI, call it), `BuildService` (Cargo, one at a time, merged requests), `diag` (Cargo JSON as data), `fix` (machine-applicable fixes). Knows nothing about the world. |
| `crates/specula/src/logic/` | `LogicHost`: modules at tick boundaries, the request handler (world, state store, RNG, controller, events, log), input actions, the character controller interface, and the agent tools. |
| `games/ambulo` | The example module: a capsule that walks and jumps (the M3 exit proof's playable character). |

## Lifecycle

```
logic_build {path}            (connection thread)               (engine thread, at a tick boundary)
  cargo metadata --offline: path crates only, no build code
  cargo build --offline --message-format=json ──► diagnostics as data
  copy lib to <target>/specula-modules/<name>-<pid>-<n>.dll
  LoadLibrary the copy, check ABI table ──────────────────────► describe (name, actions, hooks)
                                                                 one transaction:
                                                                   old build: stop(reload)
                                                                   new build: start(reloaded = true)
                                                                   commit (claims, journal) or roll back
                                                                 old build unloads
                                                                 every tick: tick(actions, events)
```

- **Load or reload is one transaction.** The new build is opened and ABI-checked off the engine thread, then described
  on it. Then the old build of the same name stops (`stop(reload)`), the new one starts (`reloaded = true`), and their
  world changes are committed: checked against the calling agent's claims and journaled. If the new build fails to
  start or a claim refuses the changes, the world, the state store and the RNG positions are rolled back, and the old
  build keeps running (`start_failed`, or `claimed`). Only after a commit does the old library unload. So a build that
  fails to compile, load, describe or start never replaces a running one.
- **Versioned copies.** The library is copied to a name no other load uses before loading, so Cargo can overwrite the
  original while the module runs (Windows locks loaded DLLs). The copy is deleted after unloading; copies left by
  engines that died are swept by pid at the next load.
- **Tick order.** Running modules tick after the hosted game's tick, in `(order, name)` order, each with its input
  actions for the tick and the events emitted during the previous tick.
- **Unload.** `stop(unload)` runs first (ambulo deletes what it spawned) and its changes are committed like a load's: a
  claim that refuses them refuses the unload. If `stop` itself fails, its changes are dropped and the module unloads
  anyway. The state store and RNG positions stay for a later load unless `forget_state`.
- **Who may load, replace or unload a module.** Anyone, unless another agent holds a live claim covering the module's
  root, `/<name>` (where a module keeps its things by convention: ambulo's are under `/ambulo`). Claiming a module's root
  is how an agent owns it; `claim_share` lets others rebuild it too (as in the exit proof, where a second agent edits
  the module). The start and stop changes are checked against claims as the caller's as well, so a module can't be used
  to reach into someone else's claim.

## Trust: module code runs with the engine's privileges

A module is native code in the engine's process: it can do anything the engine can (files, network, other processes).
The rules below keep builds **zero-dependency** and free of **build-time code**, and keep a module's *world* changes
inside claims, but they are no sandbox. Only load modules from people (and agents) you would let edit the engine.

What `logic_build` lets a module build from, checked against `cargo metadata --offline` before Cargo compiles anything
(`specula_logic::build::vet`):
- every crate in the build is a **path crate under the engine's working directory**; registry and git crates are
  refused (`third_party_crate`, naming the package; AGENTS.md rule 1), and so are path crates elsewhere
  (`outside_root`);
- **no build scripts** (`build_script`) and **no proc-macros** (`proc_macro`), anywhere in the build: they run code at
  build time;
- **compile-time paths stay in the module's `src/`** (`source_path`, `specula_logic::paths`): `#[path = "..."]` and
  `include_str!` / `include_bytes!` / `include!` must name a plain string literal that resolves inside `src/`, never an
  absolute path or one climbing out (a module can't compile engine sources into itself or read
  `C:/Windows/win.ini` at build time). The project tools apply the same check to every committed `.rs` file;
- **no `.cargo/` folder** between the module and the working directory (`cargo_config`): a `rustc-wrapper` or linker
  there would run; the project's own `.cargo/config.toml` applies;
- **no `rust-toolchain` or `rust-toolchain.toml`** between the module and the working directory (`toolchain_file`),
  checked before any Cargo runs (rustup would run, or download, the toolchain it names); and every Cargo the engine
  starts is pinned to the engine's own toolchain (`RUSTUP_TOOLCHAIN`, and an absolute Cargo and rustc when the engine
  knows them);
- the build's **own output** is checked again before its library may load: an artifact that isn't a path crate under
  the root, a build script that ran, or a proc-macro refuses the build and restores `Cargo.lock` (a manifest edited
  between the check and the build can't get anything loaded, though a build script it adds has already run);
- Cargo runs `--offline`, so nothing is downloaded; if resolving would change `Cargo.lock` and the build is refused, the
  lock file is put back.

## The engine owns all state

A fresh `Self::default()` handles every call, so a module has nowhere to keep state but the engine:

- **the world**: entities and their `props`, visible to agents (`world_query`), saved with the world, journaled;
- **the state store** (`Cx::state` / `set_state`): a per-module key-value map of `Value`s (≤ 1,024 keys, ≤ 1 MiB),
  kept across reloads;
- **RNG streams** (`Cx::random_*`): PCG32 per `(module, stream)`, seeded from the module's seed, positions kept
  across reloads.

So a reload loses nothing by construction. `static mut`, `thread_local!`, `OnceLock` caches and threads are against
the rules: they break determinism or dangle after an unload. (World files don't hold the state store and RNG positions;
save games do, with the events waiting for the next tick and agents' held input: [SAVES.md](SAVES.md).)

## Determinism (F2)

- The SDK offers only deterministic inputs: the tick number (no wall clock), engine RNG streams, the world, the tick's
  input **actions** (never raw keys), and events in emission order. Its maps are `BTreeMap`s; `Value` encoding is
  canonical; floats cross as exact bits.
- Modules run in a fixed order, on the engine thread, at tick boundaries. Agent input (`logic_input`) is applied per
  tick.
- A failed call changes nothing: world changes are rolled back (`World::rollback_to`), and so are state writes, RNG
  draws and events. Ids handed out during the failed call are not reused (the world's rule), and the journal is told
  about them, so `world_hash {verify}` still replays to the live hash.
- Each load, reload, unload and disable is journaled (kind `module`) with its tick, generation and **build hash** (hash
  of the library bytes, link time stamps left out) and shown in `logic_status` (DETERMINISM.md rule 8; playtest
  replays refuse other builds, traces name a module whose build differs). Module world changes are journaled as `module:<name>` (like a hosted game's ticks), and
  `world_hash {verify}` replays them to the live hash (checked by `scripts/module_reload.py`).
- The SDK can't stop a module from calling `std::time` or `f32::sin` itself, but `logic_build` reports it: the
  module's sources are scanned for platform maths, wall-clock time, `HashMap`, threads, `static mut` and
  `thread_local!` (`determinism::<rule>` warnings; DETERMINISM.md, *Lints*). ambulo carries the simulation crates'
  `clippy.toml`; copy it into a new module.
- The floating-point environment is guarded around every call (DETERMINISM.md, *The floating-point guard*): a module
  that changes the rounding mode is put right when it calls back or returns, and `logic_status` counts it
  (`fp_restored`).

## The C ABI

All in `specula_sdk::abi`; version **1** (`ABI_VERSION`).

```rust
#[repr(C)] pub struct ModuleApi {          // static in the module; `specula_module_entry()` returns it
    abi_version: u32, api_size: u32,       // checked first; a mismatch is a structured `abi_mismatch`
    sdk_version: [u8; 16],                 // CARGO_PKG_VERSION of the SDK, NUL-padded
    call: unsafe extern "C" fn(host: *const HostApi, entry: u32, input: *const u8, len: usize) -> u32,
}
#[repr(C)] pub struct HostApi {            // the engine's, valid for one call
    abi_version: u32, api_size: u32, ctx: *mut c_void,
    request: unsafe extern "C" fn(ctx, req: *const u8, len: usize, out: *mut Slice) -> u32,
    reply:   unsafe extern "C" fn(ctx, bytes: *const u8, len: usize),
}
```

- Entries: `DESCRIBE 1`, `START 2`, `TICK 3`, `HOOK 4`, `STOP 5`. Statuses: `OK 0`, `ERROR 1`, `PANIC 2`,
  `BAD_INPUT 3`, `UNKNOWN_ENTRY 4`.
- Messages are `Value`s (null, bool, i64, f64, string, list, ordered map) in a canonical little-endian encoding.
  Requests are maps with an `op` (`find`, `get`, `query`, `spawn`, `set`, `delete`, `state_get`, `state_set`, `rng`,
  `move_character`, `emit`, `log`); `spawn` and `set` take exactly the fields of the `world_spawn` and `world_set`
  tools and are validated by the same code, so modules and agents share one vocabulary and one set of error messages.
- Neither side frees the other's memory or keeps its pointers past a call. Answers live in an engine buffer valid until
  the module's next request; replies are copied by the engine.
- **Panics never cross the boundary**, provided the module unwinds: the SDK catches them in `call` (with message and
  `file:line:column` from a chained panic hook in the module's own copy of std) and returns `PANIC`; the engine's
  trampolines catch their own. The engine then rolls the call back and disables the module. **Modules must be built
  with `panic = "unwind"`** (Rust's default): with `"abort"` a panic would end the whole engine process. The SDK
  refuses to compile otherwise (`compile_error!` on `cfg(not(panic = "unwind"))`), and `logic_build` passes
  `--config profile.dev.panic="unwind"` (and release) so a module's own profile can't change it.
- **Bounded work.** At most 10,000 requests per call: past that each request is refused, the call fails with
  `too_many_requests` and the module is **disabled**, like after a panic (a deterministic cutoff, no wall clock; a
  runaway costs one frame a few tens of milliseconds, once). Replies ≤ 256 KiB; queries ≤ 4,096 ids; ≤ 256 events per
  call, each ≤ 4 KiB encoded, and ≤ 1,024 waiting for the next tick (`too_many_events`, `event_too_big`);
  `logic_status` shows small event data only and cuts echoed errors to a few hundred bytes. An infinite loop that makes
  no requests can't be stopped (no preemption); the tick's time shows it in `logic_status` (`tick_us`).
- Additive changes (new ops, new keys) keep the version; anything else bumps it. Layout is pinned by
  `abi::tests::layout_is_pinned`.
- `ModuleCode::of::<M>()` runs a module type linked into the engine through the same function pointer and messages:
  used by tests today, and the path for shipping builds on platforms that forbid loading code (F6: consoles, iOS).

## Agent tools

| Tool | What it does |
|---|---|
| `logic_build {path, check?, fix?, release?, reload?, detailed?, seed?}` | Cargo build of the module crate, diagnostics as data, optional fixes, then reload. Timings: `queued`, `build`, `reload` (`open`, `load`, `start` inside), `total`. |
| `logic_reload {module? \| library?}` | Reload from the last library (after building it yourself) or load a prebuilt one. |
| `logic_unload {module, forget_state?}` | Stop and unload. |
| `logic_status {module?, detailed?}` | Modules (status, why disabled, generation, build hash, ticks, failures, last error, tick µs, actions, held input, hooks, state size / values), recent loads, log, events, hook uses, builds. |
| `logic_input {module, actions, ticks?}` | Hold actions for N ticks (buttons true/false, axes numbers, 2D axes `[x, y]`). |
| `logic_hook {hook, args?, reason}` | Call a native hook (logged with the reason). Its world changes are checked against other agents' claims, as `play_*` tools' are; a refusal keeps nothing the hook did. |

Paths must lie under the engine's working directory (checked by their text first: no `..`, no UNC or device paths;
then on disk). What a build may use: see *Trust* above. Agent names starting with `module:` are reserved for modules'
attribution, loaded or not.

### Writing a module crate

- Depend on `specula-sdk` only (a path dependency); `crate-type = ["cdylib"]`; no `build.rs`.
- Put `#![deny(unsafe_code)]` at the top, not `forbid`: `export_module!` expands to one `#[allow(unsafe_code)]`
  exported function (`#[no_mangle]` counts as unsafe code), which `forbid` wouldn't let through (E0453).
- Give it `debug = 0` and `strip = "debuginfo"` in the workspace's `[profile.dev.package.<name>]` (see build times).
- Keep its things under `/<name>` and look things up there (ambulo's ground query is scoped to `/ambulo`, so a world
  with another game's ground still gets its own).

### Diagnostics as data and fixes

`logic_build` runs `cargo build --lib --offline --message-format=json` and returns, per diagnostic of the module's own
package (plus the errors of any other crate in the build, tagged with its `package`):
`level`, `code` (`E0308`, `unused_mut`), `message`, `file` (relative to the module), `line`, `column`, `end_line`,
`end_column`, `label`, `notes` (help/note children) and `fixes` (each with `replacement` and rustc's
`applicability`). Errors come first; at most 30 are listed (the rest counted); `detailed` adds rustc's rendering.
`fix: true` applies every `MachineApplicable` suggestion inside the module's folder (non-overlapping, back to front, as
`cargo fix` does), rebuilds, and repeats up to 3 rounds; `fixes_applied` lists them. `check: true` runs `cargo check`
only.

### Build service (swarms)

Cargo locks its target directory, so concurrent agents' builds would just queue inside Cargo, each paying a full run.
The engine has one `BuildService`: requests queue in arrival order and one Cargo runs at a time. A request identical
to one that is queued but **not yet started** (same folder, profile, check and fix flags) joins it, and every
requester gets the same report (`requests_merged`). A request never joins a running build, because the source may have
changed after it began. Measured: three agents rebuilding at once → 2 Cargo runs (`[1, 2, 2]`).

## Input: actions per tick

A module declares its actions as a `specula.input.v1` map (`ModuleInfo::actions`, e.g. ambulo's `actions.json`:
`move` (axis2, value output; WASD, arrows, left stick) and `jump` (Space, pad South)). The engine feeds window events
and polled gamepads into a per-module `specula_input::Input` every frame and **latches** them per tick: presses
shorter than a tick still arrive as `pressed`, delta axes sum over the frames, value axes report their level at the
tick. Agents hold actions with `logic_input`, on top of window input; holds carry over a reload. Headless engines get
only agent input. Modules never see keys (F6).

## Character controller interface

`Cx::move_character(id, motion)` moves a character entity by a displacement (gravity included) and returns
`{position, grounded, blocked}`. Behind it is the engine's `CharacterMover` trait
(`crates/specula/src/logic/character.rs`). Today's implementation, `Kinematic`, is a **placeholder**: a vertical capsule
(the entity's capsule collider, feet at its origin) against the world-space boxes of other entities' colliders; it lands
on tops (stepping up to 0.35 m), stops at sides and slides along them, with no slopes or ceilings. The physics lane's
P2 controller replaces it with `LogicHost::set_mover` and no module changes.

## Native hooks: the Lex escape hatch

The contract between Lex (gameplay as data) and modules:

1. **Declare.** A module lists its hooks in `ModuleInfo::hooks`: `HookDef { name, doc, args }` with `name` in
   `[a-z][a-z0-9_]{0,63}`, a one-line `doc` saying what it does and when, and `args` as a JSON Schema (text). The
   engine validates them at load. Qualified name: `<module>/<hook>` (e.g. `ambulo/launch`).
2. **Discover.** `LogicHost::hooks() -> Vec<HookInfo>` lists the running modules' hooks (`qualified()`, `doc`, `args`
   as JSON); `logic_status` shows them to agents. Lex validates a rule's `native` references against this list (an
   unknown hook is a schema error that names the available ones).
3. **Call.** `LogicHost::call_hook(hook, args: &Json, reason: &str, caller: &str, world, tick, tick_hz, commit)
   -> Result<Json, WorldError>`. `reason` is required (1..300 characters): why a hook instead of data. `caller` names
   who asked (`lex`, or the agent). The module's `Module::hook(cx, name, args)` runs with full `Cx` access (world,
   state, RNG, controller) and answers a `Value` (returned as JSON).
4. **Log.** Every use is logged (`tick, hook, caller, reason, ok`; last 256) and counted per hook (`hook_counts`), so
   the data language can grow from evidence: the hooks used most are the next Lex features.
5. **Fail safely.** A failing hook changes nothing (rolled back) and returns `hook_failed` with the module's
   structured error; a panicking or runaway one also disables its module.
6. **Commit.** The hook's world changes since a `Mark` are handed to the `commit` callback
   (`FnMut(&mut World, Mark, &str /* module:<name> */, Phase) -> Result<(), WorldError>`): Lex passes one that journals
   them as part of its own tick (exempt from claims, like the game's rules; `Ok(())`), the `logic_hook` tool one that
   checks them against the calling agent's claims first. An `Err` refuses them, and the host rolls back everything the
   hook did. The host records the call in full (`Recording::Full`, needed for the rollback) and restores the caller's
   mode after, so call it with no snapshot-mode changes pending (take them first).

Integration (after both lanes merge): the integrator connects Lex's `native` effect to `LogicHost::call_hook` with
`caller = "lex"` and the rule's reason, and runs hooks inside Lex's tick so their changes are attributed with it.

## Build times (measured 2026-10-18, this machine, windows-gnu; shared and busy, so ±30 %)

| What | Time |
|---|---|
| ambulo edit → `cargo build` (dev, opt-level 1, debuginfo; 13.5 MB DLL) | median 628 ms |
| same, `debug = 0` + `strip = "debuginfo"` for the module (1.5–2.1 MB DLL) | median 519 ms |
| same, opt-level 0 + strip | median 568 ms (opt-level 0 doesn't help a thin module; linking dominates) |
| open (copy, map, and the OS's scan of a new library) | 100–230 ms whatever the size; on the caller's thread |
| describe + stop + start (engine thread) | ~1–2 ms |
| **edit → reload in a running engine** (`scripts/module_reload.py`, headless) | **1.05–1.08 s** (budget 2 s; cargo ~0.9 s of it) |
| edit → reload in `tests/module_reload.rs` (module copy as its own workspace) | ~1.05 s |
| ambulo tick (8 requests, controller included) | 50–100 µs average |

So modules build without debug info (workspace `Cargo.toml`, `[profile.dev.package.ambulo]`; new modules add the same
two lines), and the library is opened off the engine thread, so a reload costs the window a millisecond or two.

**Engine crates at opt-level 2 in dev** (the Rust track's per-package optimisation, in the workspace `Cargo.toml`):
clean `cargo build -p specula` 58.5 s → 61.3 s, incremental after an edit in the `specula` crate 2.40 s → 2.70 s;
one agent's headless call rate (`scripts/call_rate.py`) `world_set` 1,289 → 1,917 calls/s, `world_spawn` 1,282 →
1,889, `world_query` 1,234 → 1,477, `engine_info` unchanged. Game modules stay at the dev default.

## Not done yet

- ~~Persisting the state store and RNG positions~~: save games carry them ([SAVES.md](SAVES.md), 2026-10-18).
- ~~Determinism lints for module crates; the FP-environment guard~~: done 2026-10-22 (DETERMINISM.md).
- Loading libraries on Linux/macOS (`dlopen`): `specula_sys::dynlib` is Windows-only today, so `ModuleCode::open`
  returns `unsupported` there (linked-in modules work everywhere).
- ~~A journal entry kind for loads~~: kind `module`, 2026-10-22.
- Preempting a runaway module (it would need a watchdog thread and a cooperative check in the SDK).
