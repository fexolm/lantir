use bevy_app::{App, Last, Plugin, PostStartup, Update};
use bevy_ecs::prelude::{not, resource_exists};
use bevy_ecs::schedule::IntoScheduleConfigs;

use crate::bevy::systems::{init_world_renderer_system, render_system, resize_system};
use crate::world_renderer::WorldRenderer;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        // Try once early (PostStartup). If the window reports an invalid size
        // (e.g. Wayland sentinel u32::MAX), the system returns without inserting
        // WorldRenderer, so we also retry every Update frame until it succeeds.
        app.add_systems(PostStartup, init_world_renderer_system);
        app.add_systems(
            Update,
            init_world_renderer_system
                .run_if(not(resource_exists::<WorldRenderer>)),
        );
        app.add_systems(Last, (render_system, resize_system).chain());
    }
}
