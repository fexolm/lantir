use std::sync::Arc;

use lantir_hal::{
    BlendingMode, CommandBuffer, GraphicsPipeline, GraphicsPipelineCreateInfo, PipelineLayout,
    RenderEngine, RenderingAttachmentInfo, RenderingInfo, Shader, Texture, vk,
};
use shaderc::ShaderKind;

use crate::{
    render_pass::RenderPass,
    resources::resource_manager::ResourceManager,
    scene::{Camera, Scene},
    world_renderer::WorldRenderer,
};

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct GPUDrawPushConstants {
    pub render_matrix: glam::Mat4,
}

pub struct OpaquePass {
    pipeline: GraphicsPipeline,
    color_target: Arc<Texture>,
    depth_target: Arc<Texture>,
}

impl OpaquePass {
    pub fn new(
        engine: &Arc<RenderEngine>,
        resource_manager: &Arc<ResourceManager>,
        color_format: vk::Format,
        color_target: Arc<Texture>,
        depth_target: Arc<Texture>,
    ) -> anyhow::Result<Self> {
        let vertex_shader = compile_shader(
            engine.clone(),
            "opaque.vert",
            include_str!("shaders/opaque.vert"),
            ShaderKind::Vertex,
        )?;

        let fragment_shader = compile_shader(
            engine.clone(),
            "opaque.frag",
            include_str!("shaders/opaque.frag"),
            ShaderKind::Fragment,
        )?;

        let push_constants = [vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::VERTEX,
            offset: 0,
            size: std::mem::size_of::<Camera>() as u32,
        }];

        let pipeline_layout = PipelineLayout::new(
            engine.clone(),
            vec![resource_manager.get_meta_descriptor_set_layout().clone()],
            &push_constants,
        )?;

        let pipeline = GraphicsPipeline::new(
            engine.clone(),
            &GraphicsPipelineCreateInfo {
                vertex_shader: &vertex_shader,
                fragment_shader: &fragment_shader,
                layout: &pipeline_layout,
                topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                polygon_mode: vk::PolygonMode::FILL,
                cull_mode: vk::CullModeFlags::NONE,
                front_face: vk::FrontFace::CLOCKWISE,
                color_attachment_format: color_format,
                depth_format: vk::Format::D32_SFLOAT,
                enable_depth_write: true,
                depth_compare_op: vk::CompareOp::LESS_OR_EQUAL,
                blending_mode: BlendingMode::AlphaBlend,
            },
        )?;

        Ok(Self {
            pipeline,
            color_target,
            depth_target,
        })
    }
}

impl RenderPass for OpaquePass {
    fn name(&self) -> &'static str {
        "OpaquePass"
    }

    fn execute(
        &self,
        renderer: &WorldRenderer,
        scene: &Scene,
        cb: &CommandBuffer,
    ) -> anyhow::Result<()> {
        let rm = renderer.get_resource_manager();

        let color_att = RenderingAttachmentInfo {
            image: &*self.color_target,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            clear_value: {
                let mut cv = vk::ClearValue::default();
                cv.color.float32 = [0.0, 0.0, 0.0, 1.0];
                cv
            },
        };

        let depth_att = RenderingAttachmentInfo {
            image: &*self.depth_target,
            layout: vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
            clear_value: {
                let mut cv = vk::ClearValue::default();
                cv.depth_stencil = vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                };
                cv
            },
        };

        cb.cmd_begin_rendering(
            &renderer.get_engine(),
            &RenderingInfo {
                extent: renderer.draw_extent(),
                color_attachments: &[color_att],
                depth_attachment: Some(&depth_att),
            },
        );

        cb.cmd_bind_graphics_pipeline(&renderer.get_engine(), &self.pipeline);

        cb.cmd_bind_descriptor_sets(
            &renderer.get_engine(),
            &self.pipeline.layout,
            &[renderer.get_resource_manager().get_meta_descriptor_set()],
            vk::PipelineBindPoint::GRAPHICS,
            0,
        );

        cb.cmd_push_constants(
            &renderer.get_engine(),
            &self.pipeline.layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &scene.camera,
        );

        let mut commands = Vec::with_capacity(scene.draw_items.len());
        for (i, item) in scene.draw_items.iter().enumerate() {
            let mesh = rm.get_mesh(item.mesh);
            commands.push(vk::DrawIndexedIndirectCommand {
                index_count: mesh.index_count,
                instance_count: 1,
                first_index: mesh.index_offset,
                vertex_offset: mesh.vertex_offset,
                first_instance: i as u32,
            });
        }

        if commands.is_empty() {
            cb.cmd_end_rendering(&renderer.get_engine());
            return Ok(());
        }

        let indirect_buffer_offset = renderer
            .get_resource_manager()
            .add_indirect_draw_commands(&commands)? as u64
            * size_of::<vk::DrawIndexedIndirectCommand>() as u64;

        cb.cmd_draw_indexed_indirect(
            &renderer.get_engine(),
            &renderer.get_resource_manager().get_global_indirect_buffer(),
            indirect_buffer_offset,
            commands.len() as u32,
            size_of::<vk::DrawIndexedIndirectCommand>() as u32,
        );

        cb.cmd_end_rendering(&renderer.get_engine());
        Ok(())
    }
}

fn compile_shader(
    engine: Arc<RenderEngine>,
    name: &str,
    source: &str,
    shader_kind: ShaderKind,
) -> anyhow::Result<Arc<Shader>> {
    let compiler = shaderc::Compiler::new()?;
    let mut options = shaderc::CompileOptions::new()?;
    options.set_target_env(
        shaderc::TargetEnv::Vulkan,
        shaderc::EnvVersion::Vulkan1_3 as u32,
    );
    options.set_target_spirv(shaderc::SpirvVersion::V1_5);
    options.set_generate_debug_info();

    let binary_result =
        compiler.compile_into_spirv(source, shader_kind, name, "main", Some(&options))?;

    Shader::new(engine, binary_result.as_binary_u8())
}
