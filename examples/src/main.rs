mod camera;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_transform::components::Transform;
use bevy_winit::WinitSettings;
use camera::SpectatorCameraPlugin;
use lantir_bevy::LantirDefaultPlugins;
use lantir_render::world_renderer::WorldRenderer;

#[derive(Resource)]
struct SceneLoaded;

fn load_gltf(
    commands: &mut Commands,
    world_renderer: &WorldRenderer,
    gltf_bytes: &[u8],
) -> anyhow::Result<Vec<Entity>> {
    let scene = lantir_gltf::load_gltf(world_renderer, gltf_bytes)?;

    Ok(scene.spawn(commands))
}

#[derive(Component)]
struct Car;

fn load_scene_system(world_renderer: Res<WorldRenderer>, mut commands: Commands) {
    load_gltf(
        &mut commands,
        &*world_renderer,
        include_bytes!("../assets/drift_track.glb"),
    )
    .expect("load_track");

    let car = load_gltf(
        &mut commands,
        &*world_renderer,
        include_bytes!("../assets/porsche.glb"),
    )
    .expect("load_porsche")[0];
    commands.entity(car).insert(Car);

    world_renderer
        .resource_manager()
        .set_skybox_image(
            image::load_from_memory(include_bytes!("../assets/sunset.exr"))
                .expect("failed to load skybox image"),
            1.0,
            0.35,
        )
        .expect("failed to set skybox image");
    commands.insert_resource(SceneLoaded);
}

fn move_car_forward_system(
    mut commands: Commands,
    scene_loaded: Option<Res<SceneLoaded>>,
    mut query: Query<&mut Transform, With<Car>>,
) {
    // Run only once when scene is loaded
    let Some(_flag) = scene_loaded else { return };

    for mut transform in query.iter_mut() {
        // Local forward is +Z in this coordinate convention
        let forward = transform.rotation.mul_vec3(glam::Vec3::new(0.0, 0.0, 1.0));
        transform.translation += forward * 6.0; // 15 meters forward
    }

    // Remove the flag so this system doesn't run again
    commands.remove_resource::<SceneLoaded>();
}

fn main() -> anyhow::Result<()> {
    let mut app = App::new();

    app.add_plugins(LantirDefaultPlugins {
        title: "Lantir Example".to_string(),
    });
    app.insert_resource(WinitSettings::game());

    app.add_plugins(SpectatorCameraPlugin);

    app.add_systems(Update, load_scene_system.run_if(run_once));
    app.add_systems(Update, move_car_forward_system);

    app.run();
    Ok(())
}
