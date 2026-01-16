use bevy_a11y::AccessibilityPlugin;
use bevy_app::{PluginGroup, PluginGroupBuilder, TaskPoolPlugin};
use bevy_diagnostic::FrameCountPlugin;
use bevy_input::InputPlugin;
use bevy_log::LogPlugin;
use bevy_time::TimePlugin;
use bevy_window::{Window, WindowPlugin, WindowResolution};
use bevy_winit::WinitPlugin;
use lantir_render::bevy::plugins::RenderPlugin;

pub struct LantirDefaultPlugins {
    pub title: String,
}

impl PluginGroup for LantirDefaultPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            // rendering plugins
            .add(WindowPlugin {
                primary_window: Some(Window {
                    title: self.title,
                    resolution: WindowResolution::default(),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .add(AccessibilityPlugin)
            .add(WinitPlugin::default())
            .add(InputPlugin)
            .add(RenderPlugin)
            // minimal plugins
            .add(TaskPoolPlugin::default())
            .add(FrameCountPlugin)
            .add(TimePlugin)
            // debug plugins
            .add(LogPlugin::default())
    }
}
