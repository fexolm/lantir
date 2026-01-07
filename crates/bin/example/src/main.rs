use lantir_hal::{
    AccessType, AllocationCreateFlags, Buffer, CopyImageInfo, DescriptorSet, DescriptorSetBinding,
    DescriptorSetLayout, GraphicsPipeline, GraphicsPipelineCreateInfo, ImageBarrier,
    PipelineLayout, RenderEngine, RenderEngineConfig, RenderingAttachmentInfo, RenderingInfo,
    Shader, Texture, TextureCreateInfo, UpdateFrequency, vk,
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
    vertex_buffer: Buffer,
    index_buffer: Buffer,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct GPUDrawPushConstants {
    world_matrix: glam::Mat4,
    vert_address: u64,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct Vertex {
    position: glam::Vec3,
    normal: glam::Vec3,
    color: glam::Vec4,
    uv: glam::Vec2,
}

fn load_mesh(
    engine: Arc<RenderEngine>,
    vertices: &[Vertex],
    indices: &[u32],
) -> anyhow::Result<(Buffer, Buffer)> {
    let vertex_buffer_size = (std::mem::size_of::<Vertex>() * vertices.len()) as vk::DeviceSize;

    let vertex_buffer = Buffer::new(
        engine.clone(),
        &lantir_hal::BufferCreateInfo {
            size: vertex_buffer_size,
            usage: vk::BufferUsageFlags::VERTEX_BUFFER
                | vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            memory_property: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            update_frequency: UpdateFrequency::Static,
            vma_flags: AllocationCreateFlags::empty(),
        },
    )?;

    let index_buffer_size = (std::mem::size_of::<u32>() * indices.len()) as vk::DeviceSize;

    let index_buffer = Buffer::new(
        engine.clone(),
        &lantir_hal::BufferCreateInfo {
            size: index_buffer_size,
            usage: vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            memory_property: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            update_frequency: UpdateFrequency::Static,
            vma_flags: AllocationCreateFlags::empty(),
        },
    )?;

    let staging_buffer = Buffer::new(
        engine.clone(),
        &lantir_hal::BufferCreateInfo {
            size: (std::mem::size_of::<Vertex>() * vertices.len()
                + std::mem::size_of::<u32>() * indices.len()) as u64,
            usage: vk::BufferUsageFlags::TRANSFER_SRC,
            memory_property: vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_COHERENT,
            update_frequency: UpdateFrequency::Static,
            vma_flags: AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
        },
    )?;

    unsafe {
        let staging_buffer_map = staging_buffer.map()?;

        std::ptr::copy_nonoverlapping(
            vertices.as_ptr() as *const u8,
            staging_buffer_map,
            vertex_buffer_size as usize,
        );

        std::ptr::copy_nonoverlapping(
            indices.as_ptr() as *const u8,
            staging_buffer_map.add(vertex_buffer_size as usize),
            index_buffer_size as usize,
        );

        staging_buffer.unmap();
    }

    engine.immediate_submit(|cb| {
        cb.cmd_copy_buffer(
            &engine,
            &staging_buffer,
            &vertex_buffer,
            vk::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: vertex_buffer_size,
            },
        );
        cb.cmd_copy_buffer(
            &engine,
            &staging_buffer,
            &index_buffer,
            vk::BufferCopy {
                src_offset: vertex_buffer_size,
                dst_offset: 0,
                size: index_buffer_size,
            },
        );
    })?;

    Ok((vertex_buffer, index_buffer))
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
            memory_property: vk::MemoryPropertyFlags::DEVICE_LOCAL,
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

        let push_constants = [vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::VERTEX,
            offset: 0,
            size: std::mem::size_of::<GPUDrawPushConstants>() as u32,
        }];

        let pipeline_layout = PipelineLayout::new(engine.clone(), vec![], &push_constants)?;

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

        let vertices = vec![
            Vertex {
                position: glam::vec3(0.0, -0.5, 0.0),
                normal: glam::vec3(0.0, 0.0, 1.0),
                color: glam::vec4(1.0, 0.0, 0.0, 1.0),
                uv: glam::vec2(0.5, 1.0),
            },
            Vertex {
                position: glam::vec3(0.5, 0.5, 0.0),
                normal: glam::vec3(0.0, 0.0, 1.0),
                color: glam::vec4(0.0, 1.0, 0.0, 1.0),
                uv: glam::vec2(1.0, 0.0),
            },
            Vertex {
                position: glam::vec3(-0.5, 0.5, 0.0),
                normal: glam::vec3(0.0, 0.0, 1.0),
                color: glam::vec4(0.0, 0.0, 1.0, 1.0),
                uv: glam::vec2(0.0, 0.0),
            },
        ];

        let indices = vec![0, 1, 2, 2, 1, 3];

        let (vertex_buffer, index_buffer) = load_mesh(engine.clone(), &vertices, &indices)?;

        Ok(App {
            window,
            texture,
            engine,
            frame_num: 0f32,
            pipeline,
            descriptor_set,
            pipeline_layout,
            vertex_buffer,
            index_buffer,
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

        cb.begin(engine).unwrap();

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

        let push_constants = GPUDrawPushConstants {
            world_matrix: glam::Mat4::from_rotation_y(self.frame_num * 0.01),
            vert_address: self.vertex_buffer.get_device_address(),
        };

        cb.cmd_push_constants(
            &engine,
            &self.pipeline_layout,
            vk::ShaderStageFlags::VERTEX,
            0,
            &push_constants,
        );

        cb.cmd_bind_index_buffer(&engine, &self.index_buffer, vk::IndexType::UINT32);

        cb.cmd_draw_indexed(&engine, 6, 1);

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

        cb.end(engine).unwrap();

        engine.submit_and_present(frame, &swapchain_image).unwrap();
    }
}

fn main() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = App::new(&event_loop)?;
    app.run(event_loop);
    Ok(())
}
