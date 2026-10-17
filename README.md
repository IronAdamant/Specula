# Specula

*Latin: watchtower.* A **zero-dependency, agent-native game engine and editor in Rust**.

Multiple AI agents — Claude Code, Grok Build, any MCP client — connect to one live world and build different parts of it
in real time. The agent protocol is the engine's primary interface; the human editor is built on the same protocol.

- **Zero third-party crates:** Rust `std` plus direct OS and driver calls (Win32, Vulkan).
- **Vulkan**, Windows first, cross-platform planned (Linux, macOS via Metal).
- **Sight built in:** agents get their own cameras, exact on-screen bounds and same-frame captures.
- **2D and 3D**, hot-reloaded Rust game logic, text-first world files.

**Status:** M2 complete (rendering, true sight, 2D, animation, procedural models; the first game, *Sepulcra*, is a
playable slice). M3 (gameplay as data, physics, animation) is planned and waiting for the owner's go-ahead.
See [HANDOFF.md](HANDOFF.md) for the current state, [docs/ROADMAP.md](docs/ROADMAP.md) and
[docs/ENGINE_PLAN.md](docs/ENGINE_PLAN.md).

The goal: let a solo developer or a small studio build games at the scale of a large company by running swarms of AI
agents in one live engine.

The previous Bevy-based tooling (formerly *Grok-Bevy*) is preserved at tag
[`bevy-tooling-final`](https://github.com/IronAdamant/Specula/tree/bevy-tooling-final); its docs are in
`docs/archive/bevy-era/`.

## License

MIT OR Apache-2.0.
