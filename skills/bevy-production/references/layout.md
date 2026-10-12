# Layout responsibilities

| Path | Responsibility |
|------|----------------|
| `main.rs` | Construct `App`, add `GamePlugin`, `app.run()` only |
| `lib.rs` | `GamePlugin` implements `Plugin`; registers states + child plugins |
| `plugins/core.rs` | `DefaultPlugins`, window title/resolution, optional logging |
| `plugins/remote.rs` | `#[cfg(feature = "remote")]` → `BrpExtrasPlugin` |
| `states.rs` | Enum + `Default`; transitions documented |
| `systems/loading.rs` | Asset handles, progress, transition to MainMenu |
| `systems/menu.rs` | Start button / key → Playing; clean despawn on exit |
| `systems/gameplay.rs` | Input, movement, rules; only in Playing |
| `components.rs` | `Player`, markers, gameplay tags |
| `resources.rs` | Score, settings, asset collections |
| `assets/*` | Source art; paths must match `AssetServer` loads |

## Cargo features (required)

```toml
[features]
default = []
remote  = ["dep:bevy_brp_extras", "bevy/bevy_remote"]
capture = ["remote"]
```

## Dev profiles (recommended)

```toml
[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 3
```
