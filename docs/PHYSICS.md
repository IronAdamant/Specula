# Physics (optional) — Bevy 0.19

Specula **default 2D/3D kits do not enable a physics crate**. Short demos use transform movement + simple distance checks, which is enough for GAME_DOD.

Add physics only when the design needs solid colliders, gravity/jumps, rigid bodies, or continuous collision.

## Pins (Bevy 0.19)

| Dimension | Crate | Version | Kit feature |
|-----------|-------|---------|-------------|
| 2D | `avian2d` | **0.7** | `physics` |
| 3D | `avian3d` | **0.7** | `physics` |

### Opt-in on scaffolded kits / demos

Templates and dogfood demos declare an optional feature (not in `default`):

```bash
# 2D
cargo run --features remote,capture,physics

# 3D
cargo run --features remote,capture,physics
```

```toml
# Already in scaffolded Cargo.toml when using current templates:
avian2d = { version = "0.7", optional = true }   # 2D
avian3d = { version = "0.7", optional = true }   # 3D
physics = ["dep:avian2d"]  # or avian3d
```

With `physics` enabled, kits register `PhysicsPlugins::default()` only — default gameplay remains transform-based until you add `RigidBody` / colliders yourself.

### Manual pin (older projects)

```toml
# 2D
avian2d = "0.7"

# 3D
avian3d = "0.7"
```

Version table source: [avianphysics/avian](https://github.com/avianphysics/avian). Re-check when bumping Bevy.

## When to add

- Side-view platformer (jumps, floors, one-way platforms)  
- 3D bodies that fall / push / stack  
- Projectiles that must not tunnel  

## When not to add

- Top-down collect-and-avoid demos (kit default)  
- Strategy / factory placement games — usually raycasts + custom sim, not a full solver  
- Pure UI / menu games  

## Agent recipe (sketch)

1. Enable `--features physics` (or add the pin above).  
2. Spawn `RigidBody` + `Collider` on player/ground; keep **`Name`** for BRP.  
3. Own movement **either** via physics velocity **or** manual `Transform` — not both fighting each other.  
4. Watch **B0001**: do not double-mutate the same transform queries across systems without `ParamSet` / exclusive ownership.  
5. Verify with `bevy-agent-loop` captures after enabling.

## Product stance

- Skills document pins; kits stay transform-based by default (fast compile, simple DoD).  
- Optional Cargo `physics` feature is wired on templates/demos; gameplay rewrite is agent-owned.  
- Genre design (balance, factory graphs) remains user/agent-owned — not a Specula physics concern.

## Related

- Skills: `bevy-2d-game`, `bevy-3d-game`, `bevy-production`  
- [GAME_DOD.md](GAME_DOD.md) — short demos do not require physics  
