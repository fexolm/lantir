mod camera;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
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
) -> anyhow::Result<()> {
    let scene = lantir_gltf::load_gltf(world_renderer, gltf_bytes)?;

    scene.spawn(commands);

    Ok(())
}

fn load_scene_system(world_renderer: Res<WorldRenderer>, mut commands: Commands) {
    load_gltf(
        &mut commands,
        &*world_renderer,
        include_bytes!("../assets/drift_track.glb"),
    )
    .expect("load_gltf");
    commands.insert_resource(SceneLoaded);
}

fn main() -> anyhow::Result<()> {
    let mut app = App::new();

    app.add_plugins(LantirDefaultPlugins {
        title: "Lantir Example".to_string(),
    });
    app.insert_resource(WinitSettings::game());

    app.add_plugins(SpectatorCameraPlugin);

    app.add_systems(Update, load_scene_system.run_if(run_once));

    app.run();
    Ok(())
}
