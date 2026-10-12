use bevy::prelude::*;

use crate::components::MenuEntity;
use crate::states::AppState;

pub fn menu_setup(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-2.0, 4.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
        Name::new("MainCamera"),
        MenuEntity,
    ));

    commands.spawn((
        PointLight {
            intensity: 1_500_000.0,
            ..default()
        },
        Transform::from_xyz(3.0, 6.0, 3.0),
        Name::new("MenuLight"),
        MenuEntity,
    ));

    commands.spawn((
        Text::new(
            "Specula 3D Demo\n\n\
             Objective: collect 3 cyan cubes — avoid the red hazard!\n\
             Stay on the platform (fall = game over).\n\n\
             Press Enter or Space to play",
        ),
        TextFont {
            font_size: FontSize::Px(24.0),
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.95, 0.95)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(40.0),
            left: Val::Px(40.0),
            ..default()
        },
        Name::new("MenuText"),
        MenuEntity,
    ));
}

pub fn menu_input(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<AppState>>) {
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) {
        next.set(AppState::Playing);
    }
}

pub fn cleanup_menu(mut commands: Commands, q: Query<Entity, With<MenuEntity>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}
