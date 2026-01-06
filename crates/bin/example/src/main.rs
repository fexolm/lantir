use lantir_hal::{
    vk, AccessType, ComputePipeline, CopyImageInfo, DescriptorSet,
    DescriptorSetBinding, DescriptorSetLayout, ImageBarrier, PipelineLayout, RenderEngine, RenderEngineConfig,
    Shader, Texture, TextureCreateInfo, UpdateFrequency, WriteImageInfo,
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
    pipeline: ComputePipeline,
    pipeline_layout: Arc<PipelineLayout>,
    descriptor_set: DescriptorSet,
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
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC
        };

        let texture = Texture::new(engine.clone(), &image_create_info)?;

        let shader_code = include_bytes!("gradient.spv");

        let shader = Shader::new(engine.clone(), shader_code)?;

        let draw_image_descriptor_layout = DescriptorSetLayout::new(
            engine.clone(),
            &[DescriptorSetBinding {
                stage: vk::ShaderStageFlags::COMPUTE,
                typ: vk::DescriptorType::STORAGE_IMAGE,
                binding: 0,
            }],
        )?;

        let descriptor_set =
            DescriptorSet::new(engine.clone(), draw_image_descriptor_layout.clone())?;

        let pipeline_layout =
            PipelineLayout::new(engine.clone(), vec![draw_image_descriptor_layout])?;

        let pipeline = ComputePipeline::new(engine.clone(), pipeline_layout.clone(), shader)?;

        Ok(App {
            window,
            texture,
            engine,
            frame_num: 0f32,
            pipeline,
            descriptor_set,
            pipeline_layout,
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

        self.descriptor_set.write_image(&WriteImageInfo {
            binding: 0,
            image: &self.texture,
            layout: vk::ImageLayout::GENERAL,
            descriptor_type: vk::DescriptorType::STORAGE_IMAGE,
        });

        let cb = frame.get_render_command_buffer();

        cb.cmd_image_barrier(
            &engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::Nothing],
                next_accesses: &[AccessType::ComputeShaderWrite],
                previous_layout: vk::ImageLayout::UNDEFINED,
                next_layout: vk::ImageLayout::GENERAL,
                image: &self.texture,
                aspect_mask: vk::ImageAspectFlags::COLOR,
            },
        );

        cb.cmd_bind_compute_pipeline(&engine, &self.pipeline);
        cb.cmd_bind_descriptor_set(&engine, &self.pipeline_layout, &self.descriptor_set);
        cb.cmd_dispatch(&engine, 800 / 16, 600 / 16, 1);

        cb.cmd_image_barrier(
            &engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::Nothing],
                next_accesses: &[AccessType::TransferWrite],
                previous_layout: vk::ImageLayout::UNDEFINED,
                next_layout: vk::ImageLayout::GENERAL,
                image: &swapchain_image,
                aspect_mask: vk::ImageAspectFlags::COLOR,
            },
        );

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
