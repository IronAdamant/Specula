# 2D slice sketch (Bevy 0.19)

Conceptual — adapt to project modules and exact 0.19 APIs.

```rust
// Camera (OnEnter Playing or once in setup)
commands.spawn((
    Camera2d,
    Name::new("MainCamera"),
));

// Player sprite
let texture: Handle<Image> = asset_server.load("sprites/player.png");
commands.spawn((
    Sprite {
        image: texture,
        ..default()
    },
    Transform::from_xyz(0.0, 0.0, 1.0),
    Player,
    Name::new("Player"),
));

// Movement (Update, in Playing)
fn player_movement(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut q: Query<&mut Transform, With<Player>>,
) {
    let Ok(mut tf) = q.single_mut() else { return };
    let mut dir = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) { dir.y += 1.0; }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) { dir.y -= 1.0; }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) { dir.x -= 1.0; }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) { dir.x += 1.0; }
    if dir != Vec3::ZERO {
        tf.translation += dir.normalize() * 200.0 * time.delta_secs();
    }
}
```

If `Sprite { image }` / `Camera2d` names differ slightly in the pin, prefer compiling against Bevy 0.19 docs in-tree over inventing APIs — adjust field names to what the compiler accepts.

## Asset note

Commit a real PNG under `assets/sprites/` for production templates. Procedural `Color` sprites are only temporary scaffolding.
