# Design: macOS, natively

Status: **a brief, written on Windows on 2026-11-16 from the Windows code. Nothing in it has been run on a
Mac.** It is for the Claude instance that does the port on the owner's Mac, and it is that instance's to
correct as it learns. Roadmap: M6's part 13 ([ROADMAP.md](../ROADMAP.md), M6 and the Platform
track). Rules: [AGENTS.md](../../AGENTS.md). State: [HANDOFF.md](../../HANDOFF.md).

## What was decided (owner, 2026-11-16)

- **Apple Silicon only** ("M2 Mac. Intel based macs will not be sufficient").
- **A clone of its own**, as an outside user would have ("I expect people externally when they use it to
  pull anyway. So we'll mimic that"). What that instance can rely on is what is in the repository: it has
  none of the development laptop's notes.
- **Native**: Cocoa through the Objective-C runtime, Metal, the system's audio. No MoltenVK, no
  translation layer, no third-party crate.
- **Apple's linker from the Xcode Command Line Tools is accepted** on macOS (there is no linker inside
  the Rust toolchain there, as there is on windows-gnu). Everything else stays zero-dependency.
- **The shader language comes before the Metal renderer.** No Metal shader is written by hand.
- **An Apple Developer account** is the owner's to talk through with that instance. Without one the app
  is unsigned and macOS warns when it is opened.

## How the work is split

The laptop builds the engine, the shader language and the renderer's inner interface. **The Mac's work is
what only a Mac can run**, and it comes last (owner, 2026-11-16: "Put Mac to last. This is a separate
problem and seperate development and will need to be handled by a separate instance based in Mac
system"): it is M6's part 13, after the laptop's twelve. It pushes a branch; `main` moves only when the
owner asks.

| | On the Mac | Needs from the laptop |
|---|---|---|
| **First half** | the workspace builds and its tests pass; window, input, gamepads, sound, module loading, fonts; determinism on ARM | nothing but the repository as it then stands |
| **Second half** | the Metal renderer; then every live check | the shader language writing MSL; the inner interface in `specula-gpu`; the live checks' runner and the checks in Rust |

## First: what does the Mac say today?

Nobody has built this workspace on macOS. CI builds it on Linux, so the `cfg(not(windows))` side compiles
and its tests pass there, with no GPU and no desktop. The first hour on the Mac is a finding in itself:

1. Install Rust with `rustup` (the checkout's `rust-toolchain.toml` picks the version), git, and the Xcode
   Command Line Tools.
2. `cargo run -p specula-examen -- gate --all --no-gpu`. Write down what fails, before fixing anything.
3. Expect: tests that assume Linux when they are not on Windows (font file names, `/proc`, a folder for
   settings), and `aarch64` meeting code that has only run on `x86_64`.

## What has a Windows side and nothing else

Read from the code. Each needs its macOS side behind `#[cfg(target_os = "macos")]`, with the other
platforms still answering "unsupported on this platform" (AGENTS rule 4).

| Where | What it does on Windows | Likely on macOS (to be confirmed there) |
|---|---|---|
| `specula-sys` `dynlib` | loads a library by bare name from System32 only, beside the executable, or by a path the engine wrote | `dlopen` / `dlsym`, with the same three entry points and the same refusals: a system library by its full system path, never by a bare name that the search path could answer |
| `specula-sys` `interrupt` | Ctrl+C and console close | `SIGINT` / `SIGTERM` |
| `specula-sys` `net` | which ports have listeners, from the OS table, without opening a socket | the system's process information (`libproc`); never a bind beyond loopback |
| `specula-sys` `digest` | SHA-256 from the OS (`bcrypt`) | CommonCrypto's SHA-256, which is in the system library. **Never hand-rolled** (rule 12) |
| `specula-sys` power, disk space, thread time | Win32 calls | the system's own calls |
| `specula-platform` window (`win32.rs`, 1,185 lines) | a window, its events, its size and scale, IME position | `NSApplication`, `NSWindow` and a view with a `CAMetalLayer`, through `objc_msgSend` |
| `specula-platform` `textinput`, `keys` | text and IME | `NSTextInputClient` |
| `specula-platform` `gamepad` | XInput, loaded at run time | the Game Controller framework |
| `specula-platform` `timer` | the high-resolution timer | `mach_absolute_time` or the standard library's clock |
| `specula-audio` `backend` (`wasapi.rs`, `com.rs`) | a device, a render thread, shared stats | an output audio unit with a render callback |
| `specula-logic` | builds a module as a `cdylib` and loads its `.dll` | `.dylib`, through the loader above |
| `specula-text` | system CJK faces by file name | the system's font files: **none of the names in the code has been opened on a Mac** (ROADMAP, Platform track) |
| the `specula` binary | the other side of a `cfg` in seven files: the launcher's engines, settings folders, safe paths, disk | per-user folders under the user's Library; process handling |

## Traps that can be seen from here

- **AppKit wants the main thread.** On Windows the window is pumped on a thread of its own so that tool
  calls do not stall while it is dragged. On macOS the window and its events must be on the process's
  first thread. The engine's loop has to be arranged around that, not the other way round.
- **Load frameworks at run time.** The Windows side never links an import library: each DLL is declared
  with raw-dylib in one crate, or loaded at run time. The same shape on macOS is to open the Objective-C
  runtime and each framework with `dlopen` through `dynlib`, so that building needs nothing but the
  system library. One crate owns each (rule 2's reason holds: two owners of one library is how imports go
  missing).
- **Every call into Objective-C is `unsafe` FFI** with its `// SAFETY:` comment (rule 8), and
  `objc_msgSend` must be called through a function pointer of the exact signature of the method. On
  `aarch64` a wrong signature is silent corruption, not a crash.
- **Code must be signed to run on Apple Silicon.** The linker signs what it links, ad hoc. A module that
  is hot-reloaded is a library the engine copies and loads again: write each copy under a new name and
  never over a file that is loaded.
- **Determinism is a promise across machines** ([DETERMINISM.md](DETERMINISM.md)): the same journal gives
  the same `world_hash` on ARM as on x86. `specula-sys`'s floating-point guard (`fpenv.rs`) is written
  for x86's control word; ARM has its own. Run `journal_replay` and Sepulcra's headless sweep on the Mac
  and compare the hashes with the laptop's. A difference is a finding of the first order.
- **Reversed Z, clip space and the texture origin** are Vulkan's in every shader and matrix (AGENTS rule
  6). Metal's clip space has the same depth range and the other y. The shader language's MSL back end
  and the renderer's inner interface are where that is settled, once.
- **No recording or capture of the screen, and hands off the person's pointer and keyboard** (HANDOFF,
  "On the development laptop: what never to do"). All of that list holds on the Mac.
- **Loopback only** (rule 12).

## How the Mac's work is verified

- The gate, there: `cargo run -p specula-examen -- gate`. It needs no Python.
- The live checks are still Python scripts until each is ported (ROADMAP M6, part 2). Run on the Mac only
  the ones that have been ported; say which were not run.
- A picture is checked by a model that did not make it, against the engine's own packet, as on Windows.
- **Said plainly in every report:** what was run on the Mac, what was not, and what was only read.

## Not known

- Whether the full Xcode is needed for Metal's shader compiler, or the Command Line Tools are enough
  (the MSL the shader language writes can be compiled by the system at run time, which needs neither).
- The cost of the renderer on the M2's GPU, and which of the quality presets it holds.
- Whether an unsigned build is acceptable for the first release, or the owner takes a developer account.
