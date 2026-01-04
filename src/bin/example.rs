use lantir::backend::{RenderBackend, RenderBackendConfig};
use std::sync::Arc;
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

fn main() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Example App")
            .with_inner_size(winit::dpi::LogicalSize::new(800f32, 600f32))
            .build(&event_loop)?,
    );

    let _backend = {
        let config = RenderBackendConfig {
            swapchain_extent: [1280, 1024],
            debug: true,
        };
        RenderBackend::new(&window, config)?;
    };

    Ok(())
}
