use lantir_hal::{vk, AccessType, ImageBarrier, RenderEngine, RenderEngineConfig};
use std::sync::Arc;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};

const WINDOW_WIDTH: u32 = 1300;
const WINDOW_HEIGHT: u32 = 900;

struct App {
    engine: Arc<RenderEngine>,
    window: Window,
    frame_num: f32,
}

impl App {
    fn new(event_loop: &EventLoop<()>) -> anyhow::Result<Self> {
        let window = WindowBuilder::new()
            .with_title("Example App")
            .with_inner_size(winit::dpi::LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
            .build(&event_loop)?;

        let engine = {
            let config = RenderEngineConfig {
                debug: true,
                frames_in_flight: 2,
            };
            RenderEngine::new(&window, &config)?
        };

        Ok(App {
            window,
            engine,
            frame_num: 0f32,
        })
    }

    pub fn run(&mut self, event_loop: EventLoop<()>) {
        event_loop
            .run(move |ev, target| match ev {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::RedrawRequested => self.draw_frame(),
                    WindowEvent::CloseRequested => target.exit(),
                    _ => (),
                },
                Event::AboutToWait => self.window.request_redraw(),
                _ => (),
            })
            .unwrap();
    }

    fn draw_frame(&mut self) {
        let engine = &self.engine;
        let frame = engine.begin_frame().unwrap();

        let swapchain_image = engine.acquire_swapchain_image(&frame).unwrap();
        let cb = frame.get_render_command_buffer();

        let image_barrier = ImageBarrier {
            previous_accesses: &[AccessType::Nothing],
            next_accesses: &[AccessType::TransferWrite],
            previous_layout: vk::ImageLayout::UNDEFINED,
            next_layout: vk::ImageLayout::GENERAL,
            image: &swapchain_image,
            aspect_mask: vk::ImageAspectFlags::COLOR,
        };

        let flash = ((self.frame_num / 30f32).sin()).abs();
        self.frame_num += 1f32;

        cb.cmd_image_barrier(&engine, &image_barrier);

        cb.cmd_clear_color(
            &engine,
            &swapchain_image,
            vk::ImageLayout::GENERAL,
            [flash, flash, flash, 1f32],
            vk::ImageAspectFlags::COLOR,
        );

        let image_barrier = ImageBarrier {
            previous_accesses: &[AccessType::TransferWrite],
            next_accesses: &[AccessType::Present],
            previous_layout: vk::ImageLayout::GENERAL,
            next_layout: vk::ImageLayout::PRESENT_SRC_KHR,
            image: &swapchain_image,
            aspect_mask: vk::ImageAspectFlags::COLOR,
        };

        cb.cmd_image_barrier(&engine, &image_barrier);

        engine.submit_and_present(frame, &swapchain_image).unwrap();
    }
}

fn main() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = App::new(&event_loop)?;
    app.run(event_loop);
    Ok(())
}
