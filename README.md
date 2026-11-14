# Specula

*Latin: watchtower.* A **zero-dependency, agent-native game engine and editor in Rust**.

Multiple AI agents — Claude Code, Grok Build, any MCP client — connect to one live world and build different parts of it
in real time. The agent protocol is the engine's primary interface; the human editor is built on the same protocol.

- **Zero third-party crates:** Rust `std` plus direct OS and driver calls (Win32, Vulkan).
- **Vulkan**, Windows first, cross-platform planned (Linux, macOS via Metal).
- **Sight built in:** agents get their own cameras, exact on-screen bounds and same-frame captures.
- **2D and 3D**, hot-reloaded Rust game logic, text-first world files.

**Status (2026-11-15):** milestones M0 to M5 are done; M6 (tuning, testing and the first release, on Windows and
macOS) is next. In the engine today: rendering with true sight, 2D and 3D, physics, animation, gameplay as data
(Lex) and hot-reloaded modules, swarm coordination, an asset studio with review and galleries, audio, text, an
interface toolkit, localisation, accessibility, a human editor built on the agents' protocol, a launcher and
asset export. The first game, *Sepulcra*, is its dogfood. It runs on Windows only so far, and talks to agents on
the same machine only.

**What it does not do yet, measured:** the same small games built through the engine and without it, by the same
model, pass the same checks, and through the engine they cost about three to five times the tokens
([docs/design/LEVERAGE.md](docs/design/LEVERAGE.md)). The engine is meant for medium to very large games, where
there is more to reuse; making it pay from the first hour, at every size, is M6's work, measured again with the
same harness.

See [HANDOFF.md](HANDOFF.md) for the current state, [docs/ROADMAP.md](docs/ROADMAP.md) and
[docs/ENGINE_PLAN.md](docs/ENGINE_PLAN.md).

**Visual history:** [docs/gallery](docs/gallery/README.md) shows the engine, its test assets and *Sepulcra* over time,
from the first frame to today.

The goal: let a solo developer or a small studio build games at the scale of a large company by running swarms of AI
agents in one live engine.

The previous Bevy-based tooling (formerly *Grok-Bevy*) is preserved at tag
[`bevy-tooling-final`](https://github.com/IronAdamant/Specula/tree/bevy-tooling-final); its docs are in
`docs/archive/bevy-era/`.

## Open it

- **Double-click `specula.exe`**, or run `specula` with no argument: the start screen lists your projects, the last
  one worked on first. Open one, make a new one, or open any folder as a project.
- **From a terminal**, for people and agents alike:

  ```
  specula projects            the projects worked on, most recent first
  specula open --last         resume the last one (or find the engine that has it open)
  specula new "My Game"       make a project
  specula list                the engines running on this machine
  specula tools               every tool's name, by family (specula tools NAME: one tool and its arguments)
  specula call engine_info    call any tool, with no MCP client (arguments from a file: specula call TOOL @args.json)
  specula help
  ```

- **MCP clients** connect to `http://127.0.0.1:7471/mcp` (`.mcp.json` does it for Claude Code opened in this
  folder).
- From source: `cargo run -p specula` (the start screen), or `cargo run -p specula -- --project FOLDER`.

A project is a folder: copy it to another machine and open it there. Details:
[docs/design/LAUNCH.md](docs/design/LAUNCH.md).

**Your assets are yours.** Models, terrain, creatures and their clips are recipes: plain text in the project,
which you or any agent can read, copy and change with a text editor. `model_export` writes any of them as a glTF
binary (`.glb`) for Blender, Godot, Unity or Unreal, and says what the file holds and what it does not. Images
are PNG and sounds are WAV or Ogg already. Details: [docs/design/EXPORT.md](docs/design/EXPORT.md).

## Network and firewall

Specula talks only to agents on the same machine.
- **The agent server listens on loopback only:** `127.0.0.1`, port 7471 by default, or `--port`. It refuses any
  other bind address. There are no per-agent auth tokens yet, so never expose it beyond the machine.
- **Ports handed to agents stay on loopback too.** Swarms that run their own engines lease ports with
  `lease_take`. To check that a port is free, Specula reads Windows' own table of listening ports and never opens a
  socket on another interface.
- **Nothing goes out:** no telemetry, no downloads. Module builds (`logic_build`) run Cargo offline.

**You should never see a Windows Firewall "allow access" prompt for Specula.** Windows asks only when a program
listens beyond the machine, and Specula doesn't. If a prompt appears for `specula.exe`, `sepulcra.exe` or a
`specula-<hash>.exe` test program:
- choose **Cancel**: nothing in Specula needs network access;
- please report it as a bug.

Development builds from 2026-10-18 had one such bug: an early port-lease check briefly listened on all
interfaces. It was caught in review and fixed before it merged. Every rebuild is a new program, so those builds
prompted repeatedly. Any firewall rules they left behind point at deleted temporary builds. You can remove them
under Windows Defender Firewall → Advanced settings → Inbound Rules (entries named `specula…` whose paths are
under `.claude\worktrees\` or a temp folder).

## License

MIT OR Apache-2.0.
