mod camera;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_winit::WinitSettings;
use camera::SpectatorCameraPlugin;
use lantir_bevy::LantirDefaultPlugins;
use lantir_render::bevy::components::{Material, Mesh, Transform};
use lantir_render::resources::INVALID_RESOURCE_HANDLE;
use lantir_render::world_renderer::WorldRenderer;

#[derive(Resource)]
struct SceneLoaded;

fn load_gltf_draw_items(
    commands: &mut Commands,
    world_renderer: &WorldRenderer,
    gltf_bytes: &[u8],
) -> anyhow::Result<()> {
    let scene = lantir_gltf::load_gltf(world_renderer, gltf_bytes)?;

    for node in scene.nodes {
        let Some(mesh) = node.mesh else {
            continue;
        };
        let material = node.material.unwrap_or(INVALID_RESOURCE_HANDLE);

        commands.spawn((Mesh(mesh), Material(material), Transform(node.transform)));
    }

    Ok(())
}

fn load_scene_system(world_renderer: Res<WorldRenderer>, mut commands: Commands) {
    load_gltf_draw_items(
        &mut commands,
        &*world_renderer,
        include_bytes!("../assets/track.glb"),
    )
    .expect("load_gltf_draw_items");
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
