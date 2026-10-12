# 3D slice sketch (Bevy 0.19)

Conceptual — adjust to exact 0.19 types if compiler differs.

```rust
// Ground
commands.spawn((
    Mesh3d(meshes.add(Plane3d::default().mesh().size(20.0, 20.0))),
    MeshMaterial3d(materials.add(Color::srgb(0.3, 0.35, 0.3))),
    Transform::default(),
    Name::new("Ground"),
));

// Player
commands.spawn((
    Mesh3d(meshes.add(Capsule3d::default())),
    MeshMaterial3d(materials.add(Color::srgb(0.2, 0.5, 0.9))),
    Transform::from_xyz(0.0, 1.0, 0.0),
    Player,
    Name::new("Player"),
));

// Light
commands.spawn((
    PointLight {
        intensity: 2_000_000.0,
        shadows_enabled: true,
        ..default()
    },
    Transform::from_xyz(4.0, 8.0, 4.0),
    Name::new("KeyLight"),
));

// Camera
commands.spawn((
    Camera3d::default(),
    Transform::from_xyz(-4.0, 6.0, 10.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
    Name::new("MainCamera"),
));

// Movement on XZ
fn player_movement(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut q: Query<&mut Transform, With<Player>>,
) {
    let Ok(mut tf) = q.single_mut() else { return };
    let mut dir = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) { dir.z -= 1.0; }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) { dir.z += 1.0; }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) { dir.x -= 1.0; }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) { dir.x += 1.0; }
    if dir != Vec3::ZERO {
        tf.translation += dir.normalize() * 5.0 * time.delta_secs();
        tf.translation.y = 1.0; // keep on plane for capsule center
    }
}
```

## glTF later

```text
assets/models/player.glb
```

Load via Bevy scene APIs for 0.19; keep `Name` on the root for BRP.
