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

A module is native code in the engine's process: it can do anything the engine can, with the owner's user rights
(read and write files, start programs, open sockets, read the window). **No source check can make that safe**: safe
Rust can write a file into the Startup folder with `std::fs`, and the compiler has known soundness holes. So the
boundary is **who may load a module**, and the checks below are lints behind it (defence in depth), not a sandbox.

### Who may load a module (2026-11-07, review of m5-gmods)

- `logic_build`, `logic_reload` and `logic_unload` need **the owner, or a lead the operator granted** (`--lead NAME`,
  `--owner NAME`; `authority::may_load_native`, AUTHORITY.md). Nobody becomes lead by asking (AUTHORITY.md), so
  with no grants at all nobody loads native code. Refused calls answer `lead_only` (`refused_at:
  logic.native_code`), and the engine thread checks again when a library loads or a module unloads, whatever the
  caller's thread checked.
- **Opening a folder runs none of its code; `--run` runs what its owner last built** (2026-11-16,
  [FIRST_HOUR.md](FIRST_HOUR.md)). A project's game file names its module folders. `--run` on the command line
  that starts the engine (`specula play NAME`) has the engine build and load them at start: through the same
  build service and the same checks as an owner's `logic_build`, as the identity `operator:run` (a session id
  no connection can have, admitted only when the flag is given, its name reserved). Without the flag the
  project's screens show and no module is loaded until an owner or a granted lead builds one.
  - **Only what an owner or a granted lead last built** (`crates/specula/src/approved.rs`; reviews R1 and R2
    of M6 part 3). A builder may write a module's sources and the game file through the project tools, and may
    only propose a build. Were `--run` to build what is on disk, restarting the engine would be the builder's
    way past the review. So every build an owner or lead asked for, and that loaded, leaves a record: the
    digest of the module's folder, and the digest of the project root's build configuration (its `Cargo.toml`,
    a toolchain file, everything under `.cargo/`: a wrapper named there is a program the build runs). `--run`
    builds a module only while both still hold (the module is held to its digest with its folder held, so
    nothing is swapped in between; the configuration is read again after the build). What changed since, by
    anyone and any way (an agent's `project_write`, an undo, an editor, a pull), is not loaded at start: the
    trace (`run.module`) and `game_state`'s `problems` say which and why, the other modules still load, and an
    owner or lead builds the changed one, which is the review.
  - **A module `--run` loads keeps its helper crates inside its own folder.** The record covers the module's
    folder. A crate elsewhere that the module compiles (anything but the engine's SDK and core) could change
    after the review with the module's digest unchanged: such a module is not recorded, the lead's build says
    so (`at_start`), and `--run` loads nothing of it. It is the rule a proposed module already keeps
    (`unpinned_sources`). Not built: a record that also pins such crates, for modules that share one.
  - **What `--run` loads is built in the operator's own folder** (review R3). Cargo takes a library it finds
    in a build folder as built from the sources when the file times say so, and a module's usual build folder
    is under the cache, which by default lies in the project's folder: it can come with a project. So the
    start-up build has a folder of its own that no project brings: `<settings>/run-builds` when the operator
    named the settings folder, otherwise (Windows) `Specula\r` under the user's local application data; the
    engine's short folder when that would be too deep to link. Counted with the other module builds
    (`disk_status`).
    - **Not covered, said plainly:** an owner's or a lead's own `logic_build` still builds under the cache. A
      project someone sends should come without its `.specula-cache` (`specula new`'s `.gitignore` leaves it
      out); delete one that came with a project before building it. Moving every module build out of the
      project's folder needs whole build folders pruned by age first (ROADMAP M6, with the remote agents'
      security work).
  - **The record is the operator's, not the project's**: kept in the per-user settings folder (`--settings
    DIR`, by default the user's own), by the project's folder. No tool writes there for an agent, and a
    project someone sends carries none. With `--settings off` there is no record and `--run` loads nothing.
    `specula new` records the module it writes and the workspace it writes (the engine's own text), so a new
    project plays at once.
  - **`--run-as-is`** (`specula play NAME --as-is`) is the operator saying that what is on disk now is theirs
    to run, whoever wrote it: for code they wrote by hand, or a project they have read. It is the only way a
    module no owner has built here runs at start, and it approves nothing: the next `--run` holds to the
    record as before.
  - **`specula run`** (a headless run of a project's game by a script of inputs: [FIRST_HOUR.md](FIRST_HOUR.md),
    "A headless run") is the same word from the operator, for that one run: it builds and loads what is on
    disk, as `--run-as-is` does, in an engine that ends with the run, and approves nothing.
- **A builder proposes.** Any other agent's `logic_build` builds nothing: the engine files a **proposal** (the module's
  folder, who, the tick, and the SHA-256 of every file in the folder, `specula_logic::digest`) and answers `lead_only`
  with it. `project_write {build}` by a builder proposes the same way. `logic_status` lists proposals (at most 32; one
  per agent and folder, the newest). The owner or lead reviews the files and runs `logic_build {path, approve: <id>}`:
  it builds only while the folder still hashes as proposed (`sources_changed` otherwise), so nobody can swap the
  sources between the review and the load. A builder can't approve, load a library (`logic_reload`) or unload a
  module. Proposals live in the engine run (not saved).
- **The sources checked are the sources compiled** (review R2, 2026-11-12). A digest before and after the build
  isn't enough: sources swapped while Cargo runs and put back afterwards hash the same. So every build that loads:
  - **holds the module folder** from the first digest until the load (`specula_logic::build::hold`). The engine's own
    writers leave a held folder alone: a `project_write` or delete there is committed and journaled as usual, and its
    disk write waits (`pending: module_building`) until the build ends;
  - compares each file's **length and modification time** as well as the bytes (`digest::module_snapshot`), so a
    write by anything else during the build loads nothing (`sources_changed`).
- **An approved build is self-contained.** The digest covers the module's folder, so an approved build that compiled
  a crate from anywhere else (but the engine's own `specula-sdk` and `specula-core`, by path) loads nothing
  (`unpinned_sources`): a helper crate outside the folder could change after the review without the digest changing.
  A proposed module keeps its helper crates inside its folder. The owner's or lead's own builds (no `approve`) may
  use helper crates elsewhere in the project; they are the reviewer.
- This guards against agents acting **through the engine**. A program with its own access to the disk already has
  the owner's user rights and is outside it.
- **Refused tries go to the security monitor.** Every refused native-code call goes through
  `logic::proposals::report_refusal` (a `logic.native_refused` trace event). A proposal is ordinary work. A refused
  approve, library load or unload is an attempt (`NativeRefusal::is_attempt`): the input monitor records it on its
  `native_code` channel (docs/design/SECURITY.md), which freezes an agent below lead (a security incident) and
  alerts the owner about a lead the operator didn't grant. The tools' descriptions say so.
- `logic_hook` and `logic_input` stay open: they call code the owner or lead already loaded, and their world changes
  are checked against claims as the caller's.

### M6: a sandbox for untrusted modules

Until then, modules are trusted code. For agents the owner doesn't trust to load code (other companies' swarms, cloud
agents), M6 needs a real sandbox, and the choice is the owner's:
- **An engine-run WASM interpreter** (zero dependencies, written here): modules compile to `wasm32`; the engine
  interprets them with the `Cx` API as the only imports, so a module can reach nothing else. Portable (mobile, F6) and
  deterministic by construction; slower than native (an interpreter, unless a JIT is written later).
- **A separate low-integrity process** per module (Windows' low integrity level or an AppContainer; a sandbox profile
  on macOS; namespaces and seccomp on Linux), speaking the same byte messages over a pipe: native speed, but three
  OS-specific sandboxes to get right and a process hop per call.
Both keep today's SDK and ABI messages; native modules would stay for the owner's own code.

What `logic_build` lets a module build from, checked against `cargo metadata --offline` before Cargo compiles anything
(`specula_logic::build::vet`):
- every crate in the build is a **path crate under the engine's working directory**; registry and git crates are
  refused (`third_party_crate`, naming the package; AGENTS.md rule 1), and so are path crates elsewhere
  (`outside_root`);
- **no build scripts** (`build_script`) and **no proc-macros** (`proc_macro`), anywhere in the build: they run code at
  build time;
- **compile-time paths stay in the module's folder** (`source_path`, `specula_logic::paths`). The authoritative check
  runs **after** the build: rustc's dep-info for the module crate lists every file the compiler read, however the
  source spelled the path (a renamed `include_str!`, a `macro_rules!` wrapper, escapes, `cfg_attr`), and each must lie
  inside the module's folder (its `src/` and top-level files such as `actions.json`; never `target/`). A build that read
  anything else is refused (no library is loaded, and its diagnostics are dropped, since an `include!` error can echo
  the file it read). The dep-info read is the one beside the build's own copy of the library, told by the
  library's bytes and never by a date (`paths::dep_infos_of`: every library the build made must have one, and should two builds have left the same library, every one is checked): Cargo keeps that copy in `deps/`, and from Rust
  1.100 in `build/<package>/<hash>/out/`, and a target folder two toolchains have built into holds both, each
  with a dep-info that describes its own library only. A library with no such copy is refused. A token scan of
  the sources gives the early hint before Cargo runs and at every commit of a
  mirrored `.rs` file. **What this is for:** determinism and lint coverage (no code compiled in that the lints didn't
  scan, nothing compiled from engine sources) and not embedding host files into a module. It is **not a sandbox**: a
  module is native code in the engine's process and can read any file at run time;
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
  lock file is put back;
- **no native reach** (`specula_logic::build::check_reach`, 2026-11-07): the module's normal dependencies may not
  include an engine crate that calls the operating system (`os_crate`: `specula-sys`, `-platform`, `-input`, `-gpu`,
  `-audio`, `-protocol`, `-logic`, `specula`; dev-dependencies don't count), and neither its sources nor another path
  crate it depends on may declare foreign functions, link a native library or use inline assembly (`native_code`, with
  the file and line). The source check runs **after** the build, over every file rustc compiled (each crate's
  dep-info, any extension: `#[path = "imp.txt"]`, `include!`, a `[lib] path` outside `src/`) for the module and every
  other crate in the build except the engine's own SDK and core, which are trusted **by canonical path, never by
  name**: the crates this engine was built from, and `crates/specula-sdk` and `crates/specula-core` under the
  engine's working directory. A game project outside the engine's repository keeps its copy of the SDK there (a
  copy anywhere else in the project is checked like any other crate, and its ABI's `unsafe` is refused). Refused (`native_code`, naming the crate, file and line): any `extern` that isn't `extern crate`, `extern fn`
  or `extern "abi" fn` (so a macro-built ABI counts), `#[link]`, `link_name`, any mention of `asm`, `global_asm` or
  `naked_asm`, any `unsafe`, `std`'s `fs`, `process`, `net`, `os` and `env` (by path, in a `use` tree, or spliced from a
  macro variable), `Command::new` and sockets; and files too many, too big or unreadable to check. This is a **lint**
  that keeps the honest path honest, raceable by whoever can write the files during a build: the boundary is *Who may
  load a module* above ([GAMEPLAY_MODULES.md](GAMEPLAY_MODULES.md), *What a module can reach*).

## The engine owns all state

A fresh `Self::default()` handles every call, so a module has nowhere to keep state but the engine:

- **the world**: entities and their `props`, visible to agents (`world_query`), saved with the world, journaled;
- **the state store** (`Cx::state` / `set_state`): a per-module key-value map of `Value`s (≤ 1,024 keys, ≤ 1 MiB),
  kept across reloads;
- **RNG streams** (`Cx::random_*`): PCG32 per `(module, stream)`, seeded from the module's seed, positions kept
  across reloads. At most **1,024 streams per module** (`MAX_RNG_STREAMS`): a new name past that is refused with
  `rng_full` (old streams still draw), like `state_full`. Save games write at most that many and read at most that
  many, so every save loads. Name streams by **purpose** (`loot`, `spawns`, `ai`), not one per entity or per tick:
  a stream per entity grows without bound as entities come and go. For per-entity randomness, draw from the purpose
  stream in a fixed order (entities sorted by id), or mix the entity id into the draw. The streams' hash for the desync
  detector is kept as they change (a wrapping sum of per-stream hashes), so its per-tick cost doesn't grow with them.
  **A stream begins again when asked** (`Cx::random_restart`, the request `rng_restart`; 2026-11-17): its next
  number is the first it ever gave, for a run that starts again and must be the same game. A stream goes on from
  where it was otherwise, and nothing restarts one behind the rules' back. The restart is undone with a call that
  fails, like a draw; restarting a stream nobody drew from makes none. (Seven of the eight one-screen builds of the
  measured comparison's last four runs wrote a generator of their own in their rules for want of this: the job asks that a restart begin "a fresh
  run with the same seed".)

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
  `rng_restart`, `move_character`, `emit`, `log`, `trace`, `sound`); `sound` (2026-11-07) asks the engine to play a recipe after the
  call commits, presentation only ([GAMEPLAY_MODULES.md](GAMEPLAY_MODULES.md), *Sound*); `spawn` and `set` take exactly the fields of the `world_spawn` and `world_set`
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
| `logic_build {path, check?, fix?, release?, reload?, detailed?, seed?, approve?}` | Owner or granted lead: Cargo build of the module crate, diagnostics as data, optional fixes, then reload (`approve` builds a proposal's exact sources). Anyone else: files a proposal, builds nothing. Timings: `queued`, `build`, `reload` (`open`, `load`, `start` inside), `total`. |
| `logic_reload {module? \| library?}` | Owner or granted lead: reload from the last library (after building it yourself) or load a prebuilt one. |
| `logic_unload {module, forget_state?}` | Owner or granted lead: stop and unload. |
| `logic_status {module?, detailed?}` | Modules (status, why disabled, generation, build hash, ticks, failures, first and last error, tick µs, actions, held input, hooks, state size / values), recent loads, log, events, hook uses, builds, proposals. |
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

A failed build's `error.code` is `compile_errors` (read the diagnostics), or one of the build's own:
`cargo_missing`, `cargo_failed`, `timeout`, `not_a_module`, `fix_failed`, and **`build_path_too_long`**
(2026-11-15). On Windows rustc hands the linker a long list of arguments in a file inside the build folder
(`<cache>/modules/<name>-<hash>/<profile>/build/<package>/<hash>/out/rustcXXXXXX/linker-arguments`), and the
Rust toolchain's linker opens a path of at most 259 characters. One character past that, it reads none of its
arguments and fails with a message about something else (`'-fuse-linker-plugin', but liblto_plugin.dll not
found`: the argument that switches the plugin off was in the file). The build service recognises exactly that
(`build::unread_link_arguments`: a link error whose notes show the linker saw no arguments, and an argument
file past the limit; any other link error keeps its own diagnostics) and answers with `error.file`,
`error.chars` and `error.limit`. What builds: a shallower project folder, or the engine started with
`--cache DIR` in a shorter folder (module builds go under the cache). `cargo check` never links, so
`check: true` passes in a folder where the build does not.

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

## What physics tells a module (2026-11-21)

Until this date a module was told nothing of physics: a bounce had to be guessed from a ball's height, and a body
that rolled off its floor fell for ever with nobody the wiser (found by making a ball on a ramp by hand through a
new project, the first cold use of 3D and physics from a module). Both are now **events from the engine**
(`crates/specula/src/physics_told.rs`), pushed after the tick's physics step and heard by every module at the next
tick, as `ui`, `setting` and `run` are. They are part of the simulation: the same run tells the same events at the
same ticks, and nothing is kept between ticks to tell them.

- **`contact`**, for a pair of which at least one entity is heard: it carries the tag `contacts`, or a group above
  it does (a game says which of its things it wants to hear of: a pile of crates is not a thousand events a second;
  a tag on the game's root hears of everything under it). `kind` is `begin` or `end` for two solid things, `enter`
  or `exit` for a trigger. `a` and `b` are the entities' paths and `a_id`, `b_id` their ids; when only one is heard
  it is `a`, and `normal` runs from `a` to `b`. `point`, and `speed` in m/s. At most 32 a tick: what is past that is
  one last `contact` of `kind` `more` with their `count`.
- **`Cx::touching(id)`** asks, at any tick, what touches a thing now (for a trigger: what is in it): ids in
  increasing order, at most 64, as of the last physics step. Any thing with a collider can be asked about, tagged
  or not, so "is it on the ground" and "what is in this zone" wait for no event. It is answered where the physics
  is lent to the modules (a tick, and the calls the engine makes for an agent); anywhere else it is refused with
  `no_physics`.
- **`fell`**, when a dynamic body passes 5 m beyond everything that stands still or is moved by the rules, in the
  direction gravity pulls: `entity` (its path), `id`, `position`. Told once, at the tick it passes. At most 8 a tick.
  With no gravity, or nothing that stands still, nothing can fall out and nothing is told. A kinematic body (a
  character) is moved by the rules and is never told of.

Neither may crowd out what the game itself says: they are pushed only while 256 of the 1,024 waiting events' room
is left. `physics_info` lists the bodies that are out of the world now (`out_of_world`), and a headless run
(`specula run`, `specula check`) that ends with one out there says so beside its state.

**The speed of a `begin` is how fast the two closed while the contact formed.** The physics used to say what the
solver had left of it at the step they touch: 0.4 m/s for a ball that fell half a metre (3.1 m/s), in `physics_events`
too. A contact now keeps the fastest its bodies closed before they touch, and the snapshot (version 3) carries it, so
a run restored at any step of a fall says the same speed to the bit (`a_begin_says_how_fast_the_two_closed...`).

**Limits.** A ragdoll's parts are not entities: their touches are not told as events, and `touching` answers the
entity they belong to, once. A module hears all of a heard thing's touches or none (it filters by `a` and `b`).

## Character controller interface

`Cx::move_character(id, motion)` moves a character entity by a displacement (gravity included) and returns
`{position, grounded, blocked}`. Behind it is the engine's `CharacterMover` trait
(`crates/specula/src/logic/character.rs`). The engine sets the **physics P2 controller**
(`crates/specula/src/physics/mover.rs`, `PhysicsMover`, through `LogicHost::set_mover`; `logic_status` names it):
sweep and slide, steps, the slope limit, snapping down stairs, moving platforms and pushing dynamic bodies, with no
module changes. How it fits the host:

- **The space is lent, not shared.** The engine owns its physics by value; around module calls (ticks, loads,
  hooks) it moves it into the mover (`Engine::lend_physics`, which first syncs the space with the world, outside any
  call) and takes it back after (`return_physics`). A move never syncs or changes the space: syncing mid-call set
  poses, woke bodies and reset contacts that a rolled-back call then left behind, and twin runs parted (review of the
  lane, finding 3; a twin test now covers it). What a module spawned or moved earlier in the same call or tick is met
  from the next tick on.
- **Rollback-safe.** A failed call rolls the world back but not the space, so a move never changes the space: the
  controller *plans* it (`Space::plan_character`) from what is underfoot now (`Space::character_ground`: it keeps no
  memory of its own) and the result is written as world changes: the character's position, and the velocity changes
  of the dynamic bodies it pushed (as `physics_impulse` writes them). A rolled-back call takes all of it with it.
- **Displacement in, velocity inside.** The SDK's motion includes gravity (modules own their vertical speed), so the
  adapter turns it into a velocity for one tick and runs the controller with its gravity off. The character is the
  entity's own convex collider (not a compound: the controller sweeps one shape), else a 0.3 × 1.8 m capsule on its
  origin; `body.character` tunes the controller when
  the entity has one.
- The old `Kinematic` box-scan controller is the fallback when no space is lent (unit tests of the host).

Proof: `physics::mover` tests walk a module (Ambulo's walk) up three steps onto a landing and down a 20° ramp, stop it
against a wall, repeat runs bit for bit, and show a rolled-back call leaves neither the move nor its pushes behind;
`scripts/module_reload.py` still passes with the physics controller.

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
- Building in a folder of any depth on Windows: today a build folder too deep for the toolchain's linker is
  refused by name (`build_path_too_long`, above), not built (ROADMAP M6).
