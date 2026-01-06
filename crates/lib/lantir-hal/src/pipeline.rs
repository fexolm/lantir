use crate::descriptor_set::DescriptorSetLayout;
use crate::resource::{DeferDrop, Resource};
use crate::shader::Shader;
use crate::RenderEngine;
use ash::vk;
use std::sync::Arc;

pub type PipelineLayout = Resource<PipelineLayoutData>;

impl PipelineLayout {
    pub fn new(
        engine: Arc<RenderEngine>,
        descriptor_sets: Vec<Arc<DescriptorSetLayout>>,
    ) -> anyhow::Result<Arc<Self>> {
        let data = PipelineLayoutData::new(&engine, descriptor_sets)?;
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
    ) -> anyhow::Result<Self> {
        let layouts = descriptor_sets.iter().map(|s| s.layout).collect::<Vec<_>>();

        let info = vk::PipelineLayoutCreateInfo::default().set_layouts(&layouts);

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

    pub(crate) layout: Arc<PipelineLayout>,

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
            .name(c"main");

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
