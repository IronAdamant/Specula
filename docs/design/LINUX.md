# Specula on Linux: what runs, what does not, and where to look for faults

Status: written 2026-11-22, for a second reader who works on a Linux machine with no GPU. No
machine of the project's own runs Linux: until one does, CI's Ubuntu job and whoever reads this
page are the only runs of the `cfg(not(windows))` side. What is said here to run on Linux was
run there by that reader or by CI, not by the page's writer; a line marked *not tried* is
exactly that.

The engine itself does not start on Linux yet. That is the platform track's work
([ROADMAP.md](../ROADMAP.md)), and it is not a fault to report. What does run is most of the
code: every crate builds, and about 3,570 tests pass, because the parts of an engine that go
wrong quietly (the simulation, the parsers, the saves, the data formats) need no window and no
GPU.

## Setting up

- Rust **1.100.0**, which `rust-toolchain.toml` pins: rustup installs it on the first cargo
  command in the checkout. No other toolchain, no `+toolchain`, no `RUSTUP_TOOLCHAIN`.
- Nothing else. There are no third-party crates and no build scripts; `Cargo.lock` lists the
  workspace only.
- An environment `RUSTFLAGS` replaces the repository's own flags. On Linux that is harmless
  (the flags in `.cargo/config.toml` are for windows-gnu); say so in a report if one was set.

## What runs

| Command | What it is | On Linux |
|---|---|---|
| `cargo build --workspace --all-targets` | everything, tests and examples included | builds (CI) |
| `cargo test --workspace --no-fail-fast` | every test binary, whatever fails before it | passes (CI; 3,569 passed and 68 ignored by the second reader's count on `a51cc44a`) |
| `cargo clippy --workspace --all-targets -- -D warnings` | the only clippy run of the `cfg(not(windows))` side | clean (CI) |
| `cargo test -p specula-examen --no-fail-fast` | the document guards: links, private paths, line endings | passes (CI). **It reads the whole working folder**: a folder unpacked inside the clone fails the links test |
| `cargo run -p specula-examen -- gate --no-gpu` | the gate, sized to what changed | *not tried*. Without `--no-gpu` it fails on a machine with no adapter, by design |
| `specula new NAME --in DIR --settings DIR` | makes a project | works (second reader) |
| `cargo run --release -p specula-physics --example physics_bench -- --quick` | the physics budget scenes, each with its final state hash | *not tried*; needs no engine |
| `cargo run -p sepulcra --example headless --release -- --seed 1` | the first game's rules with no engine: a seed, a layout, a sweep, a recording replayed | *not tried*; needs no engine |

Tests that need a GPU, a window, an audio device or a desktop skip themselves and pass, each
saying why. A test that **fails** for want of one is a fault of the test (AGENTS.md, rule 7).

## What does not run, and the words it says

None of these is news. Each is one missing piece of the platform layer.

| What | The words | Where |
|---|---|---|
| Starting the engine, in a window or headless: `specula run`, `check`, `tune`, `find`, `open`, and so every live check (`examen live`) | `GPU init failed: Vulkan is unsupported on this platform for now` | the engine draws headless runs too, and Vulkan is loaded through `specula_sys::dynlib`, which has no `dlopen` side yet |
| Loading a game's module | `loading module libraries is not implemented on this platform yet` | `crates/specula-logic/src/loader.rs` |
| Any runtime-loaded library | `runtime library loading is not implemented on this platform yet` | `crates/specula-sys/src/lib.rs`, `dynlib` |
| A project's lock | `a project's lock is not implemented on this platform yet` | `crates/specula/src/launch_engine.rs` |
| Ctrl+C handling, the table of listening ports, free disk space | `... is not implemented on this platform yet` | `crates/specula-sys/src/lib.rs` |
| Windows, gamepads, the clipboard, an audio device | `... unsupported on this platform` | `specula-platform`, `specula-audio` |

So nothing that plays a game built from a module can be reproduced on Linux today: the six
comparison jobs, the carrying bridge as a builder met it, `specula find` on a board. Those are
read on Linux (the ledgers, the code) and run on Windows.

**What it would take to play a game headless on Linux**, in order: `dlopen`, `dlsym` and
`dlclose` behind `dynlib` (the C library is already linked by `std`, so rule 1 holds); the
module host loading a `.so`; then either Vulkan through a software driver (Mesa's lavapipe,
which a machine with no GPU can install) or a run that draws nothing unless a frame is asked
for. **This is the second reader's to build** (the owner, 2026-11-22): it has the Linux machine,
and its work comes to `main` by the owner's word; the Windows side pulls it. The rules that
bind it are AGENTS.md's, above all: no third-party crate, no build script, each `unsafe`
block at a foreign boundary with its `SAFETY` comment, every crate still compiling and
passing on Windows, a structured error where something is still missing, loopback only.

## Where a Linux machine finds what this one cannot

In the order of what is most likely to turn something up.

### 1. The same world on another machine

[DETERMINISM.md](DETERMINISM.md) promises level D2: the same source gives the same simulation,
bit for bit, on any x86-64 Windows or Linux machine. Only tests with a value written into them
hold that across systems, and there are few:

- `crates/specula/tests/journal_replay.rs`: a journal replayed to one pinned hash
  (`EXPECTED_HASH`). It passes on CI's Ubuntu, which is the proof so far.
- `crates/specula-core/src/dmath/tests.rs`: the deterministic maths against values written in
  (`GOLDEN`).

Everything else compares a run with a second run on the same machine. So print these on Linux
and hold them against the values below, taken on Windows (windows-gnu, the pinned Rust) at the
commit this page came in with:

```text
cargo run --release -p specula-physics --example physics_bench -- --quick
cargo run -p sepulcra --example headless --release -- --seed 1
cargo run -p sepulcra --example headless --release -- --layout all --sweep 200
```

| What | On Windows |
|---|---|
| `physics_bench --quick`, pyramid 20 | hash `7e4ab44048e3ffc3` (211 bodies, 400 contacts, top box at y = 19.469, asleep) |
| awake 1000 | hash `4384cba1c4a4750f` (1,273 contacts) |
| sleeping 10000 | hash `62b70f5e695058f4` |
| characters 100 | hash `6142c4ea8b3c7bf3` |
| `headless --seed 1` | `outcome Won at 12:07 (tick 21838), wave 10, tombstones 5/10, dust 410, lane jumps 22, walls broken 0`; `final hash f169f62d0fe6954b` |
| `headless --layout all --sweep 200` | centre-heavy: won 200; no anti-air: lost 200, all at wave 6; full block: lost 200 (`w3:7 w4:157 w5:36`); no defence: lost 200, all at wave 2 |

(The times those commands print are this machine's and mean nothing elsewhere.)


One differing hash is a finding of the first rank: say the scene, both values, `rustc -vV` and
the CPU. Physics with more than one thread (`--threads N`, `--coloured`) is promised the same
hash as with one; that is worth a run of its own.

### 2. Bytes and text from outside

Whatever reads a file someone else wrote must answer with an error, never a panic, a hang or
memory without limit. The second reader's 20,000 changed inputs each (2026-11-22) found no
panic in `png::decode`, `deflate::zlib_decompress`, `jpeg::decode`, `Json::parse`,
`toml::parse`, `base64::decode` (all `specula-core`), `GameFile::parse` (`specula-ludus`) and
`Value::decode` (`specula-sdk`), and one hang in a physics snapshot's settings, mended the same
day (`a_damaged_snapshot_is_refused_or_restored_and_never_hangs`,
`crates/specula-physics/src/snapshot.rs`).

Not yet tried by anyone but their own tests:

| Reader | Where |
|---|---|
| The rest of a physics snapshot (bodies, joints, contacts), with a step after every restore that was accepted | `Space::restore_state` |
| Ogg/Vorbis and WAV | `specula_audio::vorbis::decode`, `wav::decode` |
| TrueType | `specula_text::font::Font::from_bytes` |
| glTF and GLB | `specula_gltf::doc::load` |
| Baked light | `specula_lux::file::decode` |
| A screen, a template, a condition | `specula_ui::screen::Screen::parse` and `state.rs` |
| String tables and plural rules | `specula_lingua::table::Table::parse`, `plural.rs` |
| Sound and music recipes | `specula_audio::synth::recipe::Recipe::parse`, `music::Music::parse` |
| Model recipes, styles, texture graphs (a recipe can also ask for unbounded work: `tests/hostile_recipes.rs` is the pattern) | `specula_model::recipe::Recipe::parse` |
| Lex documents and expressions | `specula_lex::doc::Document::parse`, `expr::parse` |
| Behaviour trees, timelines, effects | `specula_mens`, `specula_tempus`, `specula_scintilla` |
| HTTP/1.1 and MCP requests: the one reader that faces a socket | `specula-protocol` (`tests/mcp_http.rs` shows how a server is stood up in a test) |
| A world's save above the physics section | `crates/specula/src/world.rs`, `physics_save.rs`: inside the engine crate, reachable from its own tests |

A probe is a test or an example in a scratch crate **outside the clone** that depends on the
crate by path. Keep each input that fails as a file: a report with the bytes is a report that
can be mended the same day.

### 3. Tests that pass by this machine's clock or its file system

A gate must not pass by luck (AGENTS.md, "The gate"). `crates/specula/tests/clock_guard.rs`
reads upper bounds on times only. It cannot see a test that needs the machine to be slow, the
kind that turned CI red on 2026-11-21, or one that sleeps a few milliseconds and expects a
file's time to have moved. So:

- Run the suite several times, and again with the machine busy and with
  `--test-threads=1` and `=64`. A test that fails once in any of them is a finding, with the
  words it failed with: "it passes when run alone" is not an answer here.
- `SPECULA_TEST_WORKER_DELAY_MS=50 cargo test -p specula --lib` starts every worker thread
  late: no result may depend on when a worker runs.
- Look where a test calls `thread::sleep` between two writes of a file
  (`crates/specula/src/project.rs`, the tests near `back_up_and_release`): on a file system
  that keeps times by the second, does it still see the second write?
- A file system that tells `A.json` from `a.json`, has no drive letters and makes real
  symbolic links is itself a probe. The places that reason about paths: a check's frame
  (`frame_place`, `crates/specula/src/launch_run.rs`), a project's disk mirror
  (`crates/specula/src/project.rs`, `disk.rs`), a module's source folder
  (`crates/specula-logic`).

### 4. The side of the code only Linux compiles

`grep -rn "cfg(not(windows))" crates` lists it. Each such block is meant to return a clear
"unsupported" error with machine-readable fields (rules 4 and 5). One that panics, returns
success having done nothing, or says something untrue about why, is a finding.

### 5. What is built and not yet reviewed

[HANDOFF.md](../../HANDOFF.md), "What is built and not reviewed", is the list. The rows a Linux
machine can run, being pure code:

- **The carrying slab** (`Space::carry`, `crates/specula-physics/src/space.rs`; tests in
  `tests/scenarios.rs`). The lead's own doubts: a ball that rolls from the ground onto a
  carrier already moving (the second reader's scene for it did not test what was meant, and
  the test written since covers a landing and a bounce, not a roll across an edge); two
  carriers touching one body; a carrier that turns round within the step a body lands in.
- **The sculpt mesher and the sculpt's box** (`specula-model`): the review of the box said
  "do not merge" (it grows on ordinary sculpts), and its mend is unfinished.
- **The rounded shape's field** (`roundness`): "the sure way cannot fail" is false at its
  64-step cap, and three rules are held by no test.
- **The shape library** (`specula-formae`): 65 plane figures, 82 solids, 12 curves, each with
  closed-form measures and an exact point count before building. A measure that disagrees
  with the mesh built from it is a fault a script can find across the whole table.
- **The run's own additions of 2026-11-22**: a check's `set`, the numbers no rule read, how
  loud a sound was asked to be heard. Their only whole proof is a live check
  (`search_check`), which Linux cannot run; their parsing is in unit tests.

## Known already: not worth a report

- The engine does not start on Linux (above).
- `specula find` did not find a script for the bridge board of the 3D change job, in 90 s or
  500 s ([LEVERAGE.md](LEVERAGE.md), the results of 2026-11-22).
- A kinematic body's velocity reads zero to the rules. Written on the world page; not mended.
- The always-read pages and the largest files grew between the two comparisons.
- Three lookups that panic on a name not in the engine's own tables (`specula-formae`
  `spec.rs`, `specula-scintilla` `library.rs`, `specula-corpus` `ansur.rs`): no caller hands
  them a name from outside. A path found from a tool's argument to one of them would be a
  finding.

## What makes a report usable

- The commit (`git rev-parse HEAD`), `rustc -vV`, the kernel and the CPU.
- The command as typed, its exit code, and the words of the test that failed (the gate and CI
  print them at the end).
- For a changed input: the file, and the command that reads it.
- Each statement marked as run, read in the code, or reasoned. The reports of 2026-11-22 did
  this, and it is why they could be checked in an hour.
- A report changes nothing in the repository. Work on the Linux side that is meant for `main`
  is a commit of its own, saying what was run on Linux and what was not run on Windows, so
  that the other side knows what it still has to try.