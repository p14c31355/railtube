use bevy::app::App;
use bevy::DefaultPlugins;
use bevy::app::Startup;
use bevy::ecs::system::Commands;
use bevy::ecs::system::ResMut;
use bevy::asset::Assets;
use bevy::render::mesh::Mesh;
use bevy::core_pipeline::core_3d::Camera3dBundle;
use bevy::transform::components::Transform;
use bevy::math::Vec3;
use bevy::pbr::PointLightBundle;
use bevy::pbr::PointLight;
use bevy::render::mesh::shape::UVSphere;
use bevy::pbr::PbrBundle;
use bevy::transform::components::Transform;
use bevy::render::mesh::shape::UVSphere;
use bevy::utils::default;

fn main() {
    fn nothing() {
        App::new().run();
    }

    fn camera() {
        App::new()
            .add_plugins(DefaultPlugins)
            .add_systems(Startup, setup)
            .run();

        fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
            // カメラを追加
            commands.spawn(Camera3dBundle {
                transform: Transform::from_xyz(0.0, 6., 12.0)
                    .looking_at(Vec3::new(0., 1., 0.), Vec3::Y),
            });
            // 光を追加
            commands.spawn(PointLightBundle {
                point_light: PointLight {
                    intensity: 9000.0,
                    range: 100.,
                    shadows_enabled: true,
                    transform: Transform::from_xyz(8.0, 16.0, 8.0),
                },
            });
            let sphere = meshes.add(shape::UVSphere::default().into());
            commands.spawn(PbrBundle {
                mesh: sphere,
                // このxyzはカメラの向きと同じ
                transform: Transform::from_xyz(0.0, 1.0, 0.0),
                ..default()
            });
        }
    }
}
