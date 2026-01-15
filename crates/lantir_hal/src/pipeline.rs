use crate::RenderEngine;
use crate::descriptor_set::DescriptorSetLayout;
use crate::resource::{DeferDrop, Resource};
use crate::shader::Shader;
use ash::vk;
use std::sync::Arc;

pub type PipelineLayout = Resource<PipelineLayoutData>;

impl PipelineLayout {
    pub fn new(
        engine: Arc<RenderEngine>,
        descriptor_sets: Vec<Arc<DescriptorSetLayout>>,
        push_constants: &[vk::PushConstantRange],
    ) -> anyhow::Result<Arc<Self>> {
        let data = PipelineLayoutData::new(&engine, descriptor_sets, push_constants)?;
        Ok(Arc::new(Resource::make(engine, data)))
    }
}

pub struct PipelineLayoutData {
    pub(crate) layout: vk::PipelineLayout,

    _descriptor_sets: Vec<Arc<DescriptorSetLayout>>,
}

impl PipelineLayoutData {
    pub fn new(
        engine: &RenderEngine,
        descriptor_sets: Vec<Arc<DescriptorSetLayout>>,
        push_constants: &[vk::PushConstantRange],
    ) -> anyhow::Result<Self> {
        let layouts = descriptor_sets.iter().map(|s| s.layout).collect::<Vec<_>>();

        let info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&layouts)
            .push_constant_ranges(push_constants);

        let layout = unsafe { engine.device.create_pipeline_layout(&info, None)? };

        Ok(Self {
            layout,
            _descriptor_sets: descriptor_sets,
        })
    }
}

impl DeferDrop for PipelineLayoutData {
    fn destroy(&mut self, engine: &RenderEngine) {
        unsafe {
            engine.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

pub type ComputePipeline = Resource<ComputePipelineData>;

impl ComputePipeline {
    pub fn new(
        engine: Arc<RenderEngine>,
        layout: Arc<PipelineLayout>,
        shader: Arc<Shader>,
    ) -> anyhow::Result<Self> {
        let data = ComputePipelineData::new(&engine, layout, shader)?;
        Ok(Resource::make(engine, data))
    }
}

pub struct ComputePipelineData {
    pub(crate) pipeline: vk::Pipeline,

    pub layout: Arc<PipelineLayout>,

    _shader: Arc<Shader>,
}

impl ComputePipelineData {
    pub fn new(
        engine: &RenderEngine,
        layout: Arc<PipelineLayout>,
        shader: Arc<Shader>,
    ) -> anyhow::Result<Self> {
        let stage_info = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(shader.shader)
            .name(c"cs_main");

        let infos = [vk::ComputePipelineCreateInfo::default()
            .stage(stage_info)
            .layout(layout.layout)];

        let pipeline = unsafe {
            engine
                .device
                .create_compute_pipelines(vk::PipelineCache::null(), &infos, None)
                .map_err(|(_, e)| e)?[0]
        };

        Ok(Self {
            pipeline,
            layout,
            _shader: shader,
        })
    }
}

impl DeferDrop for ComputePipelineData {
    fn destroy(&mut self, engine: &RenderEngine) {
        unsafe {
            engine.device.destroy_pipeline(self.pipeline, None);
        }
    }
}

pub enum BlendingMode {
    AlphaBlend,
    Additive,
}

pub struct GraphicsPipelineCreateInfo<'i> {
    pub vertex_shader: &'i Arc<Shader>,
    pub fragment_shader: &'i Arc<Shader>,
    pub layout: &'i Arc<PipelineLayout>,
    pub topology: vk::PrimitiveTopology,
    pub polygon_mode: vk::PolygonMode,
    pub cull_mode: vk::CullModeFlags,
    pub front_face: vk::FrontFace,
    pub color_attachment_format: vk::Format,
    pub depth_format: vk::Format,
    pub enable_depth_write: bool,
    pub depth_compare_op: vk::CompareOp,
    pub blending_mode: BlendingMode,
}

pub type GraphicsPipeline = Resource<GraphicsPipelineData>;

impl GraphicsPipeline {
    pub fn new(
        engine: Arc<RenderEngine>,
        create_info: &GraphicsPipelineCreateInfo,
    ) -> anyhow::Result<Self> {
        let data = GraphicsPipelineData::new(&engine, create_info)?;
        Ok(Resource::make(engine.clone(), data))
    }
}

pub struct GraphicsPipelineData {
    pub(crate) pipeline: vk::Pipeline,

    pub layout: Arc<PipelineLayout>,
    _shaders: [Arc<Shader>; 2],
}

fn create_color_blend_attachment(mode: &BlendingMode) -> vk::PipelineColorBlendAttachmentState {
    let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(vk::BlendOp::ADD);

    match mode {
        BlendingMode::AlphaBlend => {
            blend_attachment.dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        }

        BlendingMode::Additive => blend_attachment.dst_color_blend_factor(vk::BlendFactor::ONE),
    }
}

impl GraphicsPipelineData {
    pub fn new(
        engine: &RenderEngine,
        create_info: &GraphicsPipelineCreateInfo,
    ) -> anyhow::Result<Self> {
        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default();

        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(create_info.topology)
            .primitive_restart_enable(false);

        let tessellation_state = vk::PipelineTessellationStateCreateInfo::default();

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(create_info.polygon_mode)
            .line_width(1.)
            .cull_mode(create_info.cull_mode)
            .front_face(create_info.front_face);

        let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(true)
            .depth_write_enable(create_info.enable_depth_write)
            .depth_compare_op(create_info.depth_compare_op)
            .depth_bounds_test_enable(false)
            .stencil_test_enable(false)
            .min_depth_bounds(0.)
            .max_depth_bounds(1.);

        let color_blend_attachments = [create_color_blend_attachment(&create_info.blending_mode)];

        let color_blending_state = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .attachments(&color_blend_attachments);

        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default()
            .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);

        let color_attachment_formats = [create_info.color_attachment_format];

        let shader_stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(create_info.vertex_shader.shader)
                .name(c"vs_main"),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(create_info.fragment_shader.shader)
                .name(c"ps_main"),
        ];

        let mut render_info = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(&color_attachment_formats)
            .depth_attachment_format(create_info.depth_format);

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .push_next(&mut render_info)
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input_state)
            .tessellation_state(&tessellation_state)
            .input_assembly_state(&input_assembly_state)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization_state)
            .multisample_state(&multisample_state)
            .color_blend_state(&color_blending_state)
            .depth_stencil_state(&depth_stencil_state)
            .layout(create_info.layout.layout)
            .dynamic_state(&dynamic_state);

        let pipeline = unsafe {
            engine
                .device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
                .map_err(|(_, e)| anyhow::anyhow!("Failed to create graphics pipeline: {e:?}"))
                .map(|v| v[0])
        }?;

        Ok(GraphicsPipelineData {
            pipeline,
            layout: create_info.layout.clone(),
            _shaders: [
                create_info.vertex_shader.clone(),
                create_info.fragment_shader.clone(),
            ],
        })
    }
}

impl DeferDrop for GraphicsPipelineData {
    fn destroy(&mut self, engine: &RenderEngine) {
        unsafe {
            engine.device.destroy_pipeline(self.pipeline, None);
        }
    }
}
