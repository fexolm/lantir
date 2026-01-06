use lantir_hal::{
    vk, AccessType, CopyImageInfo, ImageBarrier, RenderEngine, RenderEngineConfig,
    Texture, TextureCreateInfo, UpdateFrequency,
};
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
    texture: Texture,
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

        let image_create_info = TextureCreateInfo {
            image_type: vk::ImageType::TYPE_2D,
            update_frequency: UpdateFrequency::PerFrame,
            format: vk::Format::R8G8B8A8_UNORM,
            extent: vk::Extent2D {
                width: 800,
                height: 600,
            },
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
        };

        let texture = engine.create_texture(&image_create_info)?;

        Ok(App {
            window,
            texture,
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
        {
            let image_barrier = ImageBarrier {
                previous_accesses: &[AccessType::Nothing],
                next_accesses: &[AccessType::TransferWrite],
                previous_layout: vk::ImageLayout::UNDEFINED,
                next_layout: vk::ImageLayout::GENERAL,
                image: &self.texture,
                aspect_mask: vk::ImageAspectFlags::COLOR,
            };

            cb.cmd_image_barrier(&engine, &image_barrier);
        }

        {
            let image_barrier = ImageBarrier {
                previous_accesses: &[AccessType::Nothing],
                next_accesses: &[AccessType::TransferWrite],
                previous_layout: vk::ImageLayout::UNDEFINED,
                next_layout: vk::ImageLayout::GENERAL,
                image: &swapchain_image,
                aspect_mask: vk::ImageAspectFlags::COLOR,
            };

            cb.cmd_image_barrier(&engine, &image_barrier);
        }

        {
            let flash = ((self.frame_num / 30f32).sin()).abs();
            self.frame_num += 1f32;

            cb.cmd_clear_color(
                &engine,
                &self.texture,
                vk::ImageLayout::GENERAL,
                [flash, flash, flash, 1f32],
                vk::ImageAspectFlags::COLOR,
            );
        }

        {
            let extent = vk::Extent2D {
                width: 800,
                height: 600,
            };

            let copy_image_info = CopyImageInfo {
                src_image: &self.texture,
                src_layout: vk::ImageLayout::GENERAL,
                src_aspect_mask: vk::ImageAspectFlags::COLOR,
                src_extent: extent,
                dst_image: &swapchain_image,
                dst_layout: vk::ImageLayout::GENERAL,
                dst_aspect_mask: vk::ImageAspectFlags::COLOR,
                dst_extent: extent,
            };

            cb.cmd_copy_image(&self.engine, &copy_image_info);
        }

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
