# Specula

*Latin: watchtower.* A **zero-dependency, agent-native game engine and editor in Rust**.

Multiple AI agents — Claude Code, Grok Build, any MCP client — connect to one live world and build different parts of it
in real time. The agent protocol is the engine's primary interface; the human editor is built on the same protocol.

- **Zero third-party crates:** Rust `std` plus direct OS and driver calls (Win32, Vulkan).
- **Vulkan**, Windows first, cross-platform planned (Linux, macOS via Metal).
- **Sight built in:** agents get their own cameras, exact on-screen bounds and same-frame captures.
- **2D and 3D**, hot-reloaded Rust game logic, text-first world files.

**Status:** M2 complete (rendering, true sight, 2D, animation, procedural models; the first game, *Sepulcra*, is a
playable slice). M3 and M4 are in progress: gameplay as data (Lex), physics, animation graphs, hot-reloaded modules
and swarm coordination are in. See [HANDOFF.md](HANDOFF.md) for the current state, [docs/ROADMAP.md](docs/ROADMAP.md)
and [docs/ENGINE_PLAN.md](docs/ENGINE_PLAN.md).

The goal: let a solo developer or a small studio build games at the scale of a large company by running swarms of AI
agents in one live engine.

The previous Bevy-based tooling (formerly *Grok-Bevy*) is preserved at tag
[`bevy-tooling-final`](https://github.com/IronAdamant/Specula/tree/bevy-tooling-final); its docs are in
`docs/archive/bevy-era/`.

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
