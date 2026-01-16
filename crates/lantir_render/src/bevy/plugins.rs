use bevy_app::{App, Last, Plugin, PostStartup};
use bevy_ecs::schedule::IntoScheduleConfigs;

use crate::bevy::systems::{init_world_renderer_system, render_system, resize_system};

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, init_world_renderer_system);
        app.add_systems(Last, (render_system, resize_system).chain());
    }
}
