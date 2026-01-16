use bevy_ecs::{
    entity::Entity,
    message::MessageReader,
    query::With,
    system::{Query, ResMut},
    world::World,
};
use bevy_window::{PrimaryWindow, Window, WindowResized};
use bevy_winit::WINIT_WINDOWS;
use lantir_hal::{RenderEngine, RenderEngineConfig, vk};

use crate::{
    bevy::components::{Camera, Material, Mesh, Transform},
    world_renderer::{WorldRenderer, WorldRendererConfig},
};

pub fn init_world_renderer_system(world: &mut World) {
    let primary_window = {
        let mut query = world.query_filtered::<Entity, With<PrimaryWindow>>();
        let Some(entity) = query.iter(world).next() else {
            return;
        };
        entity
    };

    let maybe_world_renderer = WINIT_WINDOWS.with(|cell| {
        let winit_windows = cell.borrow();
        let Some(window) = winit_windows.get_window(primary_window) else {
            return None;
        };

        let inner_size = window.inner_size();
        if inner_size.width == 0 || inner_size.height == 0 {
            return None;
        }

        let extent = vk::Extent2D {
            width: inner_size.width,
            height: inner_size.height,
        };

        let engine = {
            let config = RenderEngineConfig {
                debug: true,
                frames_in_flight: 2,
            };
            RenderEngine::new(window, &config)
        }
        .ok()?;

        let world_renderer = WorldRenderer::new(
            engine,
            &WorldRendererConfig {
                draw_extent: extent,
                window_extent: extent,
            },
        )
        .ok()?;

        Some(world_renderer)
    });

    let Some(world_renderer) = maybe_world_renderer else {
        return;
    };

    world.insert_resource(world_renderer);
}

pub fn render_system(
    mut world_renderer: ResMut<WorldRenderer>,
    query: Query<(&Mesh, &Material, &Transform)>,
    camera_query: Query<&Camera>,
) {
    let draw_items = query
        .iter()
        .map(
            |(&Mesh(mesh), &Material(material), &Transform(transform))| {
                crate::resources::DrawItem {
                    mesh,
                    material,
                    transform,
                }
            },
        )
        .collect::<Vec<_>>();

    let Ok(&Camera(camera)) = camera_query.single() else {
        return;
    };

    let scene = crate::scene::Scene {
        draw_items: &draw_items,
        camera,
    };

    world_renderer.draw_frame(&scene).unwrap();
}

pub fn resize_system(
    mut world_renderer: ResMut<WorldRenderer>,
    mut events: MessageReader<WindowResized>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    for _ev in events.read() {
        let Ok(win) = windows.single() else {
            continue;
        };
        let w = win.physical_width();
        let h = win.physical_height();
        if w == 0 || h == 0 {
            continue;
        }
        world_renderer
            .resize(vk::Extent2D {
                width: w,
                height: h,
            })
            .unwrap();
    }
}
