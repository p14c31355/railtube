use ::bevy::prelude::*;

fn main() {
    fn nothing() {
        App::new().run();
    }

    fn main() {
        App::new()
            .add_plugins(DefaultPlugins)
            .add_systems(Startup, setup)
            .run();

        fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
            // カメラを追加
            commands.spawn(Camera3dBundle {
                transform: Transform::from_xyz(0.0, 6., 12.0)
                    .looking_at(Vec3::new(0., 1., 0.), Vec3::Y),
                ..default())
            };
            // 光を追加
commands.spawn(PointLightBundle {
    point_light: PointLight {
        intensity: 9000.0,
        range: 100.,
        shadows_enabled: true,
        ..default())
    }},
    transform: Transform::from_xyz(8.0, 16.0, 8.0),
    ..default()
    let sphere = meshes.add(shape::UVSphere::default().into());
    commands.spawn(PbrBundle {
        mesh: sphere,
        // このxyzはカメラの向きと同じ
        transform: Transform::from_xyz(0.0, 1.0, 0.0),
        ..default()
});
