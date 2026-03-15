use std::{mem::size_of, sync::Arc};

use lantir_hal::{
    BlendingMode, CommandBuffer, GraphicsPipeline, GraphicsPipelineCreateInfo, PipelineLayout,
    RenderEngine, RenderingAttachmentInfo, RenderingInfo, Shader, Texture, vk,
};

use crate::{
    include_shader,
    render_pass::{DynamicConstants, RenderPass},
    resources::{PbrBlendMode, resource_manager::ResourceManager},
    scene::Scene,
    world_renderer::WorldRenderer,
};

pub struct GeometryPass {
    blend_mode: PbrBlendMode,
    pipeline: GraphicsPipeline,
    /// Normal attachment (RGBA16F)
    gbuf_normal: Arc<Texture>,
    /// Albedo attachment (RGBA8)
    gbuf_albedo: Arc<Texture>,
    /// Roughness/metallic attachment (RG8)
    gbuf_roughness_metal: Arc<Texture>,
    /// Motion vectors attachment (RG16F)
    gbuf_motion: Arc<Texture>,
    depth_target: Arc<Texture>,
}

impl GeometryPass {
    pub fn new(
        engine: &Arc<RenderEngine>,
        resource_manager: &Arc<ResourceManager>,
        gbuf_normal: Arc<Texture>,
        gbuf_albedo: Arc<Texture>,
        gbuf_roughness_metal: Arc<Texture>,
        gbuf_motion: Arc<Texture>,
        depth_target: Arc<Texture>,
        blend_mode: PbrBlendMode,
    ) -> anyhow::Result<Self> {
        let shader = Shader::new_u32(engine.clone(), include_shader!("gbuffer.hlsl"))?;

        let push_constants = [vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            offset: 0,
            size: std::mem::size_of::<DynamicConstants>() as u32,
        }];

        let pipeline_layout = PipelineLayout::new(
            engine.clone(),
            vec![resource_manager.get_meta_descriptor_set_layout().clone()],
            &push_constants,
        )?;

        // Specialization constant 0: enable alpha-test discard for Masked pass.
        let spec_map_entries = [vk::SpecializationMapEntry {
            constant_id: 0,
            offset: 0,
            size: std::mem::size_of::<u32>(),
        }];
        let spec_value: u32 = if blend_mode == PbrBlendMode::Masked { 1 } else { 0 };
        let spec_data = spec_value.to_le_bytes();

        let pipeline = GraphicsPipeline::new(
            engine.clone(),
            &GraphicsPipelineCreateInfo {
                vertex_shader: &shader,
                fragment_shader: &shader,
                layout: &pipeline_layout,
                vertex_specialization: None,
                fragment_specialization: Some(lantir_hal::Specialization {
                    map_entries: &spec_map_entries,
                    data: &spec_data,
                }),
                topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                polygon_mode: vk::PolygonMode::FILL,
                cull_mode: vk::CullModeFlags::NONE,
                front_face: vk::FrontFace::CLOCKWISE,
                // Four GBuffer color attachments:
                //   0: normal   (RGBA16F)
                //   1: albedo   (RGBA8)
                //   2: roughness/metallic (RG8)
                //   3: motion vectors (RG16F)
                color_attachment_format: vk::Format::R16G16B16A16_SFLOAT,
                extra_color_attachment_formats: &[
                    vk::Format::R8G8B8A8_UNORM,
                    vk::Format::R8G8_UNORM,
                    vk::Format::R16G16_SFLOAT,
                ],
                depth_format: vk::Format::D32_SFLOAT,
                enable_depth_write: blend_mode != PbrBlendMode::Transparent,
                depth_compare_op: vk::CompareOp::GREATER_OR_EQUAL,
                blending_mode: BlendingMode::NoBlend,
            },
        )?;

        Ok(Self {
            blend_mode,
            pipeline,
            gbuf_normal,
            gbuf_albedo,
            gbuf_roughness_metal,
            gbuf_motion,
            depth_target,
        })
    }
}

impl RenderPass for GeometryPass {
    fn name(&self) -> &'static str {
        match self.blend_mode {
            PbrBlendMode::Opaque => "GeometryPass::Opaque",
            PbrBlendMode::Masked => "GeometryPass::Masked",
            PbrBlendMode::Transparent => "GeometryPass::Transparent",
        }
    }

    fn execute(
        &self,
        renderer: &WorldRenderer,
        scene: &Scene,
        cb: &CommandBuffer,
    ) -> anyhow::Result<()> {
        let rm = renderer.get_resource_manager();

        let normal_att = RenderingAttachmentInfo {
            image: &*self.gbuf_normal,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            clear_value: {
                let mut cv = vk::ClearValue::default();
                cv.color.float32 = [0.0, 0.0, 0.0, 0.0];
                cv
            },
            load_op: if self.blend_mode == PbrBlendMode::Opaque {
                vk::AttachmentLoadOp::CLEAR
            } else {
                vk::AttachmentLoadOp::LOAD
            },
        };

        let albedo_att = RenderingAttachmentInfo {
            image: &*self.gbuf_albedo,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            clear_value: {
                let mut cv = vk::ClearValue::default();
                cv.color.float32 = [0.0, 0.0, 0.0, 0.0];
                cv
            },
            load_op: if self.blend_mode == PbrBlendMode::Opaque {
                vk::AttachmentLoadOp::CLEAR
            } else {
                vk::AttachmentLoadOp::LOAD
            },
        };

        let rm_att = RenderingAttachmentInfo {
            image: &*self.gbuf_roughness_metal,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            clear_value: {
                let mut cv = vk::ClearValue::default();
                cv.color.float32 = [0.5, 0.0, 0.0, 0.0];
                cv
            },
            load_op: if self.blend_mode == PbrBlendMode::Opaque {
                vk::AttachmentLoadOp::CLEAR
            } else {
                vk::AttachmentLoadOp::LOAD
            },
        };

        let motion_att = RenderingAttachmentInfo {
            image: &*self.gbuf_motion,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            clear_value: {
                let mut cv = vk::ClearValue::default();
                cv.color.float32 = [0.0, 0.0, 0.0, 0.0];
                cv
            },
            load_op: if self.blend_mode == PbrBlendMode::Opaque {
                vk::AttachmentLoadOp::CLEAR
            } else {
                vk::AttachmentLoadOp::LOAD
            },
        };

        let depth_att = RenderingAttachmentInfo {
            image: &*self.depth_target,
            layout: vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
            clear_value: {
                let mut cv = vk::ClearValue::default();
                cv.depth_stencil = vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                };
                cv
            },
            load_op: if self.blend_mode == PbrBlendMode::Opaque {
                vk::AttachmentLoadOp::CLEAR
            } else {
                vk::AttachmentLoadOp::LOAD
            },
        };

        cb.cmd_begin_rendering(
            renderer.get_engine(),
            &RenderingInfo {
                extent: renderer.draw_extent(),
                color_attachments: &[normal_att, albedo_att, rm_att, motion_att],
                depth_attachment: Some(&depth_att),
            },
        );

        cb.cmd_bind_graphics_pipeline(renderer.get_engine(), &self.pipeline);
        cb.cmd_bind_descriptor_sets(
            renderer.get_engine(),
            &self.pipeline.layout,
            &[rm.get_meta_descriptor_set()],
            vk::PipelineBindPoint::GRAPHICS,
            0,
        );

        let push = DynamicConstants {
            viewproj: scene.camera.viewproj,
            inv_viewproj: scene.camera.inv_viewproj,
            prev_viewproj: renderer.prev_viewproj(),
            camera_pos: scene.camera.camera_pos,
        };
        cb.cmd_push_constants(
            renderer.get_engine(),
            &self.pipeline.layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &push,
        );

        let mut commands: Vec<vk::DrawIndexedIndirectCommand> = Vec::new();

        let mut draw_indices = scene
            .draw_items
            .iter()
            .enumerate()
            .filter(|(_i, item)| {
                let mat = rm.get_material(item.material);
                mat.blend_mode == self.blend_mode
            })
            .map(|(i, _)| i)
            .collect::<Vec<usize>>();

        if self.blend_mode == PbrBlendMode::Transparent {
            draw_indices.sort_by_cached_key(|&i| {
                let item = &scene.draw_items[i];
                let world_pos = item.model_matrix * glam::Vec4::new(0.0, 0.0, 0.0, 1.0);
                let view_pos = scene.camera.view * world_pos;
                (view_pos.z * 1_000_000.0) as i64
            });
        }

        for idx in draw_indices {
            let item = &scene.draw_items[idx];
            commands.push(vk::DrawIndexedIndirectCommand {
                index_count: rm.get_mesh_index_count(item.mesh),
                instance_count: 1,
                first_index: rm.get_mesh_index_offset(item.mesh),
                vertex_offset: rm.get_mesh_vertex_offset(item.mesh),
                first_instance: idx as u32,
            });
        }

        if !commands.is_empty() {
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
        }

        cb.cmd_end_rendering(renderer.get_engine());
        Ok(())
    }
}
