mod camera;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_input::InputPlugin;
use bevy_window::{PrimaryWindow, Window, WindowPlugin, WindowResized};
use bevy_winit::{WinitPlugin, WinitWindows};
use lantir_render::resources::{DrawItem, INVALID_RESOURCE_HANDLE};
use lantir_render::scene::Scene;
use lantir_render::world_renderer::{self, WorldRenderer, WorldRendererConfig};
use lantir_hal::{RenderEngine, RenderEngineConfig, vk};

use crate::camera::{SpectatorCamera, SpectatorCameraPlugin};

const WINDOW_WIDTH: u32 = 1300;
const WINDOW_HEIGHT: u32 = 900;

#[derive(Resource)]
struct DrawItems {
    draw_items: Vec<DrawItem>,
}

struct RenderState {
    world_renderer: WorldRenderer,
}

fn init_render_exclusive(world: &mut World) {
    if world.get_non_send_resource::<RenderState>().is_some() {
        return;
    }

    // The window is created by bevy_window/bevy_winit.
    let primary_entity = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .iter(world)
        .next()
        .expect("PrimaryWindow entity missing");

    // WinitWindows is stored as a non-send resource.
    let winit_windows = world
        .get_non_send_resource::<WinitWindows>()
        .expect("WinitWindows missing (did you add WinitPlugin?)");
    let Some(winit_window) = winit_windows.get_window(primary_entity) else {
        // Not ready yet; winit window is created very early, but this keeps init robust.
        return;
    };

    let image_extent = vk::Extent2D {
        width: winit_window.inner_size().width,
        height: winit_window.inner_size().height,
    };
    let draw_extent = image_extent;

    let engine = {
        let config = RenderEngineConfig {
            debug: true,
            frames_in_flight: 2,
        };
        RenderEngine::new(&**winit_window, &config).expect("RenderEngine::new")
    };

    let world_renderer = world_renderer::WorldRenderer::new(
        engine,
        &WorldRendererConfig {
            draw_extent,
            window_extent: draw_extent,
        },
    )
    .expect("WorldRenderer::new");

    let draw_items = load_gltf_draw_items(&world_renderer, include_bytes!("../assets/track.glb"))
        .expect("load_gltf_draw_items");

    world.insert_resource(DrawItems { draw_items });

    world.insert_non_send_resource(RenderState { world_renderer });
}

fn load_gltf_draw_items(world_renderer: &WorldRenderer, gltf_bytes: &[u8]) -> anyhow::Result<Vec<DrawItem>> {
    let scene = lantir_gltf::load_gltf(world_renderer, gltf_bytes)?;
    Ok(scene
        .nodes
        .into_iter()
        .filter_map(|node| {
            let mesh = node.mesh?;
            let material = node.material.unwrap_or(INVALID_RESOURCE_HANDLE);
            Some(DrawItem {
                transform: node.transform,
                mesh,
                material,
            })
        })
        .collect())
}

fn resize_system(
    mut render: NonSendMut<RenderState>,
    mut events: EventReader<WindowResized>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    // WindowResized reports logical size; WorldRenderer/swapchain wants physical pixels.
    for _ev in events.read() {
        let Ok(win) = windows.get_single() else {
            continue;
        };
        let w = win.physical_width();
        let h = win.physical_height();
        if w == 0 || h == 0 {
            continue;
        }
        render
            .world_renderer
            .resize(vk::Extent2D { width: w, height: h })
            .unwrap();
    }
}

fn render_system(
    mut render: NonSendMut<RenderState>,
    draw_items: Res<DrawItems>,
    camera: Res<SpectatorCamera>,
) {
    let scene = Scene {
        camera: camera.get_camera(),
        draw_items: &draw_items.draw_items,
    };

    render.world_renderer.draw_frame(&scene).unwrap();
}

fn render_ready(render: Option<NonSend<RenderState>>) -> bool {
    render.is_some()
}

fn main() -> anyhow::Result<()> {
    let mut app = App::new();

    app.add_plugins(WindowPlugin {
        primary_window: Some(Window {
            title: "Example App".to_string(),
            resolution: (WINDOW_WIDTH as f32, WINDOW_HEIGHT as f32).into(),
            ..Default::default()
        }),
        ..Default::default()
    });
    app.add_plugins(bevy_a11y::AccessibilityPlugin::default());
    app.add_plugins(WinitPlugin::<bevy_winit::WakeUp>::default());
    app.add_plugins(InputPlugin::default());
    app.add_plugins(SpectatorCameraPlugin::default());

    app.add_systems(Update, init_render_exclusive);

    app.add_systems(
        Update,
        (resize_system, render_system)
            .chain()
            .run_if(render_ready),
    );

    app.run();
    Ok(())
}
