use std::sync::Arc;

use lantir_hal::{
    BlendingMode, CommandBuffer, GraphicsPipeline, GraphicsPipelineCreateInfo, PipelineLayout,
    RenderEngine, RenderingAttachmentInfo, RenderingInfo, Shader, Texture, vk,
};

use crate::{
    include_shader,
    render_pass::{DynamicConstants, RenderPass},
    resources::resource_manager::ResourceManager,
    scene::Scene,
    world_renderer::WorldRenderer,
};

pub struct SkyPass {
    pipeline: GraphicsPipeline,
    color_target: Arc<Texture>,
    depth_target: Arc<Texture>,
}

impl SkyPass {
    pub fn new(
        engine: &Arc<RenderEngine>,
        resource_manager: &Arc<ResourceManager>,
        color_format: vk::Format,
        color_target: Arc<Texture>,
        depth_target: Arc<Texture>,
    ) -> anyhow::Result<Self> {
        let shader = Shader::new_u32(engine.clone(), include_shader!("sky.hlsl"))?;

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

        let pipeline = GraphicsPipeline::new(
            engine.clone(),
            &GraphicsPipelineCreateInfo {
                vertex_shader: &shader,
                fragment_shader: &shader,
                layout: &pipeline_layout,
                vertex_specialization: None,
                fragment_specialization: None,
                topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                polygon_mode: vk::PolygonMode::FILL,
                cull_mode: vk::CullModeFlags::NONE,
                front_face: vk::FrontFace::CLOCKWISE,
                color_attachment_format: color_format,
                extra_color_attachment_formats: &[],
                depth_format: vk::Format::D32_SFLOAT,
                enable_depth_write: false,
                depth_compare_op: vk::CompareOp::GREATER_OR_EQUAL,
                blending_mode: BlendingMode::NoBlend,
            },
        )?;

        Ok(Self {
            pipeline,
            color_target,
            depth_target,
        })
    }
}

impl RenderPass for SkyPass {
    fn name(&self) -> &'static str {
        "SkyPass"
    }

    fn execute(
        &self,
        renderer: &WorldRenderer,
        scene: &Scene,
        cb: &CommandBuffer,
    ) -> anyhow::Result<()> {
        let color_att = RenderingAttachmentInfo {
            image: &*self.color_target,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            clear_value: {
                let mut cv = vk::ClearValue::default();
                cv.color.float32 = [1.0, 0.0, 1.0, 1.0];
                cv
            },
            load_op: vk::AttachmentLoadOp::CLEAR,
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
            load_op: vk::AttachmentLoadOp::CLEAR,
        };

        cb.cmd_begin_rendering(
            renderer.get_engine(),
            &RenderingInfo {
                extent: renderer.draw_extent(),
                color_attachments: &[color_att],
                depth_attachment: Some(&depth_att),
            },
        );

        let push = DynamicConstants {
            viewproj: scene.camera.viewproj,
            inv_viewproj: scene.camera.inv_viewproj,
            camera_pos: scene.camera.camera_pos,
        };

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
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &push,
        );

        cb.cmd_draw(renderer.get_engine(), 3, 1);
        cb.cmd_end_rendering(renderer.get_engine());
        Ok(())
    }
}
