# AppState sketch (Bevy 0.19)

Adapt names to the project; keep the four states stable for Specula contracts.

```rust
use bevy::prelude::*;

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    Loading,
    MainMenu,
    Playing,
    Paused,
}

// In GamePlugin::build:
// app.init_state::<AppState>();
// app.add_systems(OnEnter(AppState::Loading), setup_loading);
// app.add_systems(Update, loading_progress.run_if(in_state(AppState::Loading)));
// app.add_systems(OnEnter(AppState::MainMenu), setup_menu);
// app.add_systems(OnExit(AppState::MainMenu), cleanup_menu);
// app.add_systems(Update, menu_input.run_if(in_state(AppState::MainMenu)));
// app.add_systems(OnEnter(AppState::Playing), setup_gameplay);
// app.add_systems(Update, (
//     player_movement,
//     pause_toggle,
// ).run_if(in_state(AppState::Playing)));
// app.add_systems(Update, unpause.run_if(in_state(AppState::Paused)));
```

Rules:

- Spawn screen UI/entities on **OnEnter**; despawn on **OnExit** (use a cleanup marker component + query).
- Gameplay simulation runs only in **Playing**.
- Pause freezes gameplay systems by leaving `Playing` (or by a `Time::<Virtual>` pause pattern if you adopt one later — keep it simple first).
