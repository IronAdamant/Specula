//! Headless BRP smoke target: no window/GPU required for query/mutate tests.
//!
//! ```sh
//! cargo run -p specula_sample --bin brp_headless --features remote
//! ```

use bevy::prelude::*;
use bevy::remote::http::RemoteHttpPlugin;
use bevy::remote::RemotePlugin;

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(bevy::log::LogPlugin::default())
        .add_plugins(TransformPlugin)
        .add_plugins(RemotePlugin::default())
        .add_plugins(RemoteHttpPlugin::default())
        .register_type::<AgentCube>()
        .register_type::<AgentMarker>()
        .insert_resource(AgentMarker {
            label: "specula-headless".into(),
            counter: 0,
        })
        .add_systems(Startup, setup)
        .run();
}

/// Reflectable component the agent can query and mutate over BRP.
#[derive(Component, Reflect, Clone, Debug, Default)]
#[reflect(Component)]
struct AgentCube {
    /// Scale-like scalar for mutation demos.
    heat: f32,
}

#[derive(Resource, Reflect, Clone, Debug, Default)]
#[reflect(Resource)]
struct AgentMarker {
    label: String,
    counter: u32,
}

fn setup(mut commands: Commands) {
    commands.spawn((
        Name::new("HeadlessEntity"),
        Transform::from_xyz(1.0, 2.0, 3.0),
        GlobalTransform::default(),
        AgentCube { heat: 1.0 },
    ));
    commands.spawn((
        Name::new("ChildA"),
        Transform::default(),
        GlobalTransform::default(),
    ));
}
