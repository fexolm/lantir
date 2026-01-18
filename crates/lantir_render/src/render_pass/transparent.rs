use std::sync::Arc;

use lantir_hal::{
    BlendingMode, CommandBuffer, GraphicsPipeline, GraphicsPipelineCreateInfo, PipelineLayout,
    RenderEngine, RenderingAttachmentInfo, RenderingInfo, Shader, Texture, vk,
};

use crate::{
    include_shader,
    render_pass::RenderPass,
    resources::resource_manager::ResourceManager,
    scene::{CameraTransform, Scene},
    world_renderer::WorldRenderer,
};

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct GPUDrawPushConstants {
    pub render_matrix: glam::Mat4,
}

pub struct TransparentPass {
    pipeline: GraphicsPipeline,
    color_target: Arc<Texture>,
    depth_target: Arc<Texture>,
}

impl TransparentPass {
    pub fn new(
        engine: &Arc<RenderEngine>,
        resource_manager: &Arc<ResourceManager>,
        color_format: vk::Format,
        color_target: Arc<Texture>,
        depth_target: Arc<Texture>,
    ) -> anyhow::Result<Self> {
        let shader = Shader::new_u32(engine.clone(), include_shader!("transparent.hlsl"))?;

        let push_constants = [vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::VERTEX,
            offset: 0,
            size: std::mem::size_of::<CameraTransform>() as u32,
        }];

        let pipeline_layout = PipelineLayout::new(
            engine.clone(),
            vec![resource_manager.get_meta_descriptor_set_layout().clone()],
            &push_constants,
        )?;

        let pipeline = GraphicsPipeline::new(
            engine.clone(),
            &GraphicsPipelineCreateInfo {
                vertex_shader: &shader,
                fragment_shader: &shader,
                layout: &pipeline_layout,
                topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                polygon_mode: vk::PolygonMode::FILL,
                cull_mode: vk::CullModeFlags::NONE,
                front_face: vk::FrontFace::CLOCKWISE,
                color_attachment_format: color_format,
                depth_format: vk::Format::D32_SFLOAT,
                enable_depth_write: false,
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

impl RenderPass for TransparentPass {
    fn name(&self) -> &'static str {
        "TransparentPass"
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
            load_op: vk::AttachmentLoadOp::LOAD,
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
            load_op: vk::AttachmentLoadOp::LOAD,
        };

        cb.cmd_begin_rendering(
            renderer.get_engine(),
            &RenderingInfo {
                extent: renderer.draw_extent(),
                color_attachments: &[color_att],
                depth_attachment: Some(&depth_att),
            },
        );

        cb.cmd_bind_graphics_pipeline(renderer.get_engine(), &self.pipeline);

        cb.cmd_bind_descriptor_sets(
            renderer.get_engine(),
            &self.pipeline.layout,
            &[renderer.get_resource_manager().get_meta_descriptor_set()],
            vk::PipelineBindPoint::GRAPHICS,
            0,
        );

        cb.cmd_push_constants(
            renderer.get_engine(),
            &self.pipeline.layout,
            vk::ShaderStageFlags::VERTEX,
            0,
            &scene.camera,
        );

        let mut transparent_list: Vec<(usize, f32)> = Vec::new();
        for (i, item) in scene.draw_items.iter().enumerate() {
            let mat = rm.get_material(item.material);
            if !mat.is_transparent() {
                continue;
            }

            // Compute approximate depth by transforming the item's origin into camera space.
            let world_pos = item.transform * glam::Vec4::new(0.0, 0.0, 0.0, 1.0);
            let view_pos = scene.camera.view * world_pos;
            let depth = view_pos.z;
            transparent_list.push((i, depth));
        }

        // Back-to-front: farther objects first. In view space more negative z is usually farther,
        // so sort ascending (more negative first).
        transparent_list.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut commands = Vec::with_capacity(transparent_list.len());
        for &(draw_idx, _) in &transparent_list {
            let item = &scene.draw_items[draw_idx];
            let mesh = rm.get_mesh(item.mesh);
            commands.push(vk::DrawIndexedIndirectCommand {
                index_count: mesh.index_count,
                instance_count: 1,
                first_index: mesh.index_offset,
                vertex_offset: mesh.vertex_offset,
                first_instance: draw_idx as u32,
            });
        }

        if commands.is_empty() {
            cb.cmd_end_rendering(renderer.get_engine());
            return Ok(());
        }

        let indirect_buffer_offset = renderer
            .get_resource_manager()
            .add_indirect_draw_commands(&commands)? as u64
            * size_of::<vk::DrawIndexedIndirectCommand>() as u64;

        cb.cmd_draw_indexed_indirect(
            renderer.get_engine(),
            &renderer.get_resource_manager().get_global_indirect_buffer(),
            indirect_buffer_offset,
            commands.len() as u32,
            size_of::<vk::DrawIndexedIndirectCommand>() as u32,
        );

        cb.cmd_end_rendering(renderer.get_engine());
        Ok(())
    }
}
