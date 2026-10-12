//! Specula sample: minimal 3D scene with optional BRP + capture.
//!
//! ```sh
//! cargo run -p specula_sample --features remote,capture
//! ```

use bevy::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Specula Sample".into(),
            resolution: (1280, 720).into(),
            ..default()
        }),
        ..default()
    }));

    #[cfg(feature = "remote")]
    {
        // BrpExtrasPlugin enables RemotePlugin + RemoteHttpPlugin (port 15702)
        // and registers brp_extras/* (screenshot, diagnostics, input, …).
        app.add_plugins(bevy_brp_extras::BrpExtrasPlugin::default());
    }

    app.add_systems(Startup, setup);
    app.run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(8.0, 8.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.25, 0.35, 0.25))),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Name::new("Ground"),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.8, 0.2, 0.3))),
        Transform::from_xyz(0.0, 0.5, 0.0),
        Name::new("AgentCube"),
    ));

    commands.spawn((
        PointLight {
            shadow_maps_enabled: true,
            intensity: 2_000_000.0,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
        Name::new("KeyLight"),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::new(0.0, 0.5, 0.0), Vec3::Y),
        Name::new("MainCamera"),
    ));
}
