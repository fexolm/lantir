use lantir_hal::{
    vk, AccessType, CopyImageInfo, DescriptorSet, DescriptorSetBinding,
    DescriptorSetLayout, GraphicsPipeline, GraphicsPipelineCreateInfo, ImageBarrier, PipelineLayout,
    RenderEngine, RenderEngineConfig, RenderingAttachmentInfo, RenderingInfo, Shader, Texture,
    TextureCreateInfo, UpdateFrequency,
};
use shaderc::{CompilationArtifact, ShaderKind};
use std::sync::Arc;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};

const WINDOW_WIDTH: u32 = 1300;
const WINDOW_HEIGHT: u32 = 900;

fn compile_shader(
    name: &str,
    source: &str,
    shader_kind: ShaderKind,
) -> anyhow::Result<CompilationArtifact> {
    let mut compiler = shaderc::Compiler::new()?;
    let mut options = shaderc::CompileOptions::new()?;
    options.set_target_env(
        shaderc::TargetEnv::Vulkan,
        shaderc::EnvVersion::Vulkan1_3 as u32,
    );
    options.set_target_spirv(shaderc::SpirvVersion::V1_5);

    options.set_optimization_level(shaderc::OptimizationLevel::Performance);

    let binary_result =
        compiler.compile_into_spirv(source, shader_kind, name, "main", Some(&options))?;

    assert_eq!(Some(&0x07230203), binary_result.as_binary().first());

    Ok(binary_result)
}

struct App {
    engine: Arc<RenderEngine>,
    window: Window,
    frame_num: f32,
    texture: Texture,
    pipeline: GraphicsPipeline,
    pipeline_layout: Arc<PipelineLayout>,
    descriptor_set: DescriptorSet,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct ComputePushConstants {
    data1: glam::Vec4,
    data2: glam::Vec4,
    data3: glam::Vec4,
    data4: glam::Vec4,
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
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
        };

        let texture = Texture::new(engine.clone(), &image_create_info)?;

        let frag_shader = {
            let code = include_str!("shaders/triangle.frag");
            Shader::new(
                engine.clone(),
                compile_shader("gradient", code, ShaderKind::Fragment)?.as_binary_u8(),
            )?
        };

        let vert_shader = {
            let code = include_str!("shaders/triangle.vert");
            Shader::new(
                engine.clone(),
                compile_shader("gradient", code, ShaderKind::Vertex)?.as_binary_u8(),
            )?
        };

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

        let pipeline_layout = PipelineLayout::new(engine.clone(), vec![], &[])?;

        let pipeline_info = GraphicsPipelineCreateInfo {
            vertex_shader: &vert_shader,
            fragment_shader: &frag_shader,
            layout: &pipeline_layout,
            topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            polygon_mode: vk::PolygonMode::FILL,
            cull_mode: vk::CullModeFlags::NONE,
            front_face: vk::FrontFace::CLOCKWISE,
            color_attachment_format: vk::Format::R8G8B8A8_UNORM,
            depth_format: vk::Format::UNDEFINED,
        };

        let pipeline = GraphicsPipeline::new(engine.clone(), &pipeline_info)?;

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
        self.frame_num += 1f32;

        let extent = vk::Extent2D {
            width: 800,
            height: 600,
        };

        let engine = &self.engine;
        let frame = engine.begin_frame().unwrap();

        let swapchain_image = engine.acquire_swapchain_image(&frame).unwrap();

        let cb = frame.get_render_command_buffer();

        cb.cmd_image_barrier(
            &engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::Nothing],
                next_accesses: &[AccessType::FragmentShaderWrite],
                previous_layout: vk::ImageLayout::UNDEFINED,
                next_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                image: &self.texture,
                aspect_mask: vk::ImageAspectFlags::COLOR,
            },
        );

        // cb.cmd_bind_descriptor_set(&engine, &self.pipeline_layout, &self.descriptor_set);

        cb.cmd_begin_rendering(
            &engine,
            &RenderingInfo {
                color_attachments: &[RenderingAttachmentInfo {
                    image: &self.texture,
                    layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                }],
                depth_attachment: None,
                extent,
            },
        );

        cb.cmd_bind_graphics_pipeline(&engine, &self.pipeline);

        cb.cmd_set_viewport(&engine, extent);
        cb.cmd_set_scissor(&engine, extent);

        cb.cmd_draw(&engine, 3, 1);

        cb.cmd_end_rendering(&engine);

        cb.cmd_image_barrier(
            &engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::ColorAttachmentWrite],
                next_accesses: &[AccessType::TransferRead],
                previous_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                next_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                image: &self.texture,
                aspect_mask: vk::ImageAspectFlags::COLOR,
            },
        );

        cb.cmd_image_barrier(
            &engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::Nothing],
                next_accesses: &[AccessType::TransferWrite],
                previous_layout: vk::ImageLayout::UNDEFINED,
                next_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                image: &swapchain_image,
                aspect_mask: vk::ImageAspectFlags::COLOR,
            },
        );

        {
            let copy_image_info = CopyImageInfo {
                src_image: &self.texture,
                src_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                src_aspect_mask: vk::ImageAspectFlags::COLOR,
                src_extent: extent,
                dst_image: &swapchain_image,
                dst_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                dst_aspect_mask: vk::ImageAspectFlags::COLOR,
                dst_extent: extent,
            };

            cb.cmd_copy_image(&self.engine, &copy_image_info);
        }

        let image_barrier = ImageBarrier {
            previous_accesses: &[AccessType::TransferWrite],
            next_accesses: &[AccessType::Present],
            previous_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
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
